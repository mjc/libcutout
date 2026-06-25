use std::{
    mem::size_of,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant as StdInstant},
};

use btleplug::api::{CharPropFlags, Characteristic};
use bytes::Bytes;
use cutout_core::{
    DeviceCommand, DeviceEvent, DiagnosticError, FirmwareInfo, GattChannel, Measured,
    NotificationByteLen, NotificationIngestOutcome, ParserDiagnosticCount, ParserDiagnostics,
    ParserError, ParserGapEvidence, PayloadBodyLen, PayloadClassifier, PevcapDirection,
    PevcapResolvedIdentity, ProtocolFamily, ProtocolSelector, ProtocolSession, RawFieldValue,
    ReadOnlyResponse, ReservedPayloadEvidence, SemanticEventCount, SessionInput, SessionOutput,
    SettingsEntry, SettingsReadback, TelemetryDelta, TransportAction, ValueQuality, ValueSource,
    VerificationStatus, VerifiedValue, WriteMode,
};
use futures_util::{StreamExt, stream};
use smallvec::smallvec;
use uuid::Uuid;

use super::crate_name;

type WriteRecord = (Uuid, Bytes, WriteMode);

const fn ms(value: u64) -> cutout_core::MonotonicMillis {
    cutout_core::MonotonicMillis::new(value)
}
type WriteLog = Arc<Mutex<Vec<WriteRecord>>>;
type NotificationLog = Arc<Mutex<Vec<crate::BtleNotification>>>;

#[derive(Default)]
struct TestIdentityObserver {
    saw_connection: bool,
    notifications: usize,
}

impl crate::BridgeIdentityObserver for TestIdentityObserver {
    fn observe_connection(&mut self, summary: &crate::ConnectionSummary) {
        self.saw_connection = summary.observation.name.is_some();
    }

    fn observe_notification(&mut self, _notification: &crate::BtleNotification) {
        self.notifications = self.notifications.saturating_add(1);
    }

    fn resolution(&self) -> Option<crate::BridgeIdentityResolution> {
        self.saw_connection
            .then(|| crate::BridgeIdentityResolution {
                manufacturer: (self.notifications > 0).then_some("TestCo"),
                model: (self.notifications > 0).then_some("Observer"),
                confidence: if self.notifications > 0 {
                    crate::BridgeIdentityConfidence::Model
                } else {
                    crate::BridgeIdentityConfidence::HintsOnly
                },
                evidence: if self.notifications > 0 {
                    crate::BridgeIdentityEvidence::empty()
                        .with(crate::BridgeIdentityEvidenceKind::AdvertisedNameHint)
                        .with(crate::BridgeIdentityEvidenceKind::GattHint)
                        .with(crate::BridgeIdentityEvidenceKind::PassiveFamilyMatch)
                        .with(crate::BridgeIdentityEvidenceKind::BannerModelMatch)
                } else {
                    crate::BridgeIdentityEvidence::empty()
                        .with(crate::BridgeIdentityEvidenceKind::AdvertisedNameHint)
                        .with(crate::BridgeIdentityEvidenceKind::GattHint)
                },
            })
    }
}

static OVERSIZED_BTLE_VALUE: [u8; 513] = [0; 513];

const fn protocol_writes(value: usize) -> crate::ProtocolWriteCount {
    crate::ProtocolWriteCount::new(value)
}

const fn writes(value: usize) -> crate::TransportWriteCount {
    crate::TransportWriteCount::new(value)
}

const fn subscribes(value: usize) -> crate::SubscribeCount {
    crate::SubscribeCount::new(value)
}

const fn notifications(value: usize) -> crate::NotificationCount {
    crate::NotificationCount::new(value)
}

const fn telemetry_events(value: usize) -> crate::TelemetryEventCount {
    crate::TelemetryEventCount::new(value)
}

const fn read_only_responses(value: usize) -> crate::ReadOnlyResponseCount {
    crate::ReadOnlyResponseCount::new(value)
}

const fn diagnostic_events(value: usize) -> crate::DiagnosticEventCount {
    crate::DiagnosticEventCount::new(value)
}

const fn parser_diag_count(value: u64) -> ParserDiagnosticCount {
    ParserDiagnosticCount::new(value)
}

const fn disconnects(value: usize) -> crate::DisconnectCount {
    crate::DisconnectCount::new(value)
}

fn speed(value: i32) -> Measured<cutout_core::Speed> {
    Measured::reported(cutout_core::Speed::from_millimetres_per_second(value))
}

fn voltage(value: i32) -> Measured<cutout_core::Voltage> {
    Measured::reported(cutout_core::Voltage::from_millivolts(value))
}

fn battery_percent_estimated(value: u8) -> Measured<cutout_core::Percent> {
    Measured::estimated(cutout_core::Percent::from_percent(value))
}

fn decode_outcome_evidence(
    outcome: crate::bridge::NotificationDecodeOutcome,
) -> cutout_core::NotificationEvidence {
    match outcome {
        crate::bridge::NotificationDecodeOutcome::Ignored(evidence)
        | crate::bridge::NotificationDecodeOutcome::BufferedFragment(evidence)
        | crate::bridge::NotificationDecodeOutcome::ParserGap(evidence)
        | crate::bridge::NotificationDecodeOutcome::KnownReserved(evidence)
        | crate::bridge::NotificationDecodeOutcome::ParserDiagnostic(evidence)
        | crate::bridge::NotificationDecodeOutcome::SemanticEvents(evidence) => evidence,
    }
}

#[test]
fn exposes_the_expected_name() {
    assert_eq!(crate_name(), "cutout-btle");
}

#[test]
fn peripheral_identifier_is_a_typed_backend_handle() {
    let identifier = crate::PeripheralIdentifier::new("platform-id-7");

    assert_eq!(identifier.as_str(), "platform-id-7");
    assert_eq!(identifier.to_string(), "platform-id-7");
    assert_eq!(identifier.into_inner(), "platform-id-7");
}

#[test]
fn bluetooth_address_filters_platform_null_placeholder() {
    assert_eq!(crate::BluetoothAddress::new("00:00:00:00:00:00"), None);

    let address = crate::BluetoothAddress::new("AA:BB:CC:DD:EE:FF").expect("valid address");
    assert_eq!(address.as_str(), "AA:BB:CC:DD:EE:FF");
    assert_eq!(address.to_string(), "AA:BB:CC:DD:EE:FF");
}

#[test]
fn known_uuid_fields_classify_to_zst_markers() {
    let battery_service = crate::ServiceSummary {
        uuid: <crate::StandardBatteryService as crate::KnownGattUuid>::UUID,
        primary: true,
        characteristics: Vec::new().into(),
    };
    let battery_level = crate::CharacteristicSummary {
        uuid: <crate::StandardBatteryLevelCharacteristic as crate::KnownGattUuid>::UUID,
        service_uuid: <crate::StandardBatteryService as crate::KnownGattUuid>::UUID,
        properties: CharPropFlags::READ,
    };

    assert_eq!(
        battery_service.gatt_uuid(),
        crate::GattUuid::StandardBatteryService(crate::StandardBatteryService)
    );
    assert_eq!(
        battery_level.gatt_uuid(),
        crate::GattUuid::StandardBatteryLevelCharacteristic(
            crate::StandardBatteryLevelCharacteristic
        )
    );
    assert_eq!(
        battery_level.service_gatt_uuid(),
        crate::GattUuid::StandardBatteryService(crate::StandardBatteryService)
    );
}

#[test]
fn peripheral_observation_exposes_typed_identity_views() {
    let observation = crate::PeripheralObservation {
        identifier: "backend-42".to_owned(),
        address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
        name: None,
        rssi: None,
        advertised_services: crate::AdvertisedServices::new(),
        manufacturer_data: crate::ManufacturerDataSummaries::new(),
    };

    let identifier = observation.platform_identifier();
    assert_eq!(identifier.as_str(), "backend-42");
    assert_eq!(
        identifier.as_str().as_ptr(),
        observation.identifier.as_ptr()
    );

    let address = observation
        .bluetooth_address()
        .expect("address is normalized");
    assert_eq!(
        address.as_str().as_ptr(),
        observation.address.as_deref().expect("address").as_ptr()
    );
    assert_eq!(address.as_str(), "AA:BB:CC:DD:EE:FF");
}

#[test]
fn peripheral_observation_classifies_advertised_services() {
    let unknown = Uuid::from_u128(0x6e40_0003_b5a3_f393_e0a9_e50e_24dc_ca9e);
    let observation = crate::PeripheralObservation {
        identifier: "backend-42".to_owned(),
        address: None,
        name: None,
        rssi: None,
        advertised_services: smallvec![
            <crate::SharedFfe0Service as crate::KnownGattUuid>::UUID,
            unknown,
        ],
        manufacturer_data: crate::ManufacturerDataSummaries::new(),
    };

    let services = observation.advertised_service_uuids().collect::<Vec<_>>();

    assert_eq!(
        services,
        vec![
            crate::GattUuid::SharedFfe0Service(crate::SharedFfe0Service),
            crate::GattUuid::Other(unknown),
        ]
    );
    assert!(observation.advertises::<crate::SharedFfe0Service>());
    assert!(!observation.advertises::<crate::StandardBatteryService>());
}

#[test]
fn connection_target_matches_on_address_and_name() {
    let target = crate::ConnectionTarget {
        address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
        identifier: None,
        name_contains: Some("Aero".to_owned()),
    };
    let observation = crate::PeripheralObservation {
        identifier: "peripheral-id".to_owned(),
        address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
        name: Some("NOSFET Aero".to_owned()),
        rssi: Some(-42),
        advertised_services: smallvec![],
        manufacturer_data: crate::ManufacturerDataSummaries::new(),
    };

    assert!(target.matches(&observation));
}

#[test]
fn btleplug_reconnect_host_reuses_target_and_scan_duration() {
    let target = crate::ConnectionTarget {
        address: None,
        identifier: Some("corebluetooth-id".to_owned()),
        name_contains: Some("NF2557".to_owned()),
    };

    let host = crate::BtleplugReconnectHost::new(target.clone(), crate::ScanWindow::from_secs(7));

    assert_eq!(host.target(), &target);
    assert_eq!(host.scan_for(), crate::ScanWindow::from_secs(7));
}

#[tokio::test]
async fn targeted_scan_wait_returns_as_soon_as_match_is_found() {
    let started = StdInstant::now();
    let mut attempts = 0_u8;

    let result = crate::scan::wait_for_scan_match(
        crate::ScanWindow::from_millis(200),
        crate::units::ScanPollInterval::from_millis(5),
        || {
            attempts += 1;
            async move {
                if attempts == 1 {
                    Err(crate::BtleError::NoPeripheralMatched)
                } else {
                    Ok("matched")
                }
            }
        },
    )
    .await
    .expect("second poll matches");

    assert_eq!(result, "matched");
    assert_eq!(attempts, 2);
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "targeted scan should not wait for the full scan period after a match"
    );
}

#[tokio::test]
async fn targeted_scan_wait_times_out_without_match() {
    let started = StdInstant::now();
    let mut attempts = 0_u8;

    let result = crate::scan::wait_for_scan_match::<(), _, _>(
        crate::ScanWindow::from_millis(15),
        crate::units::ScanPollInterval::from_millis(5),
        || {
            attempts += 1;
            async { Err(crate::BtleError::NoPeripheralMatched) }
        },
    )
    .await;

    assert!(matches!(result, Err(crate::BtleError::NoPeripheralMatched)));
    assert!(attempts >= 2);
    assert!(started.elapsed() >= crate::ScanWindow::from_millis(15).as_duration());
}

#[tokio::test]
async fn targeted_scan_wait_returns_non_match_errors_immediately() {
    let started = StdInstant::now();
    let mut attempts = 0_u8;

    let result = crate::scan::wait_for_scan_match::<(), _, _>(
        crate::ScanWindow::from_millis(200),
        crate::units::ScanPollInterval::from_millis(5),
        || {
            attempts += 1;
            async {
                Err(crate::BtleError::Bridge(
                    crate::SessionBridgeError::MissingNotifyEndpoint {
                        channel: GattChannel::from_bytes([0x44; 16]),
                    },
                ))
            }
        },
    )
    .await;

    assert!(matches!(result, Err(crate::BtleError::Bridge(_))));
    assert_eq!(attempts, 1);
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn connection_target_matches_on_platform_identifier() {
    let target = crate::ConnectionTarget {
        address: None,
        identifier: Some("cb-uuid-1234".to_owned()),
        name_contains: None,
    };
    let observation = crate::PeripheralObservation {
        identifier: "cb-uuid-1234".to_owned(),
        address: None,
        name: Some("NF2557".to_owned()),
        rssi: Some(-42),
        advertised_services: smallvec![],
        manufacturer_data: crate::ManufacturerDataSummaries::new(),
    };

    assert!(target.matches(&observation));
}

#[test]
fn peripheral_observation_renders_manufacturer_data_without_payload_bytes() {
    let observation = crate::PeripheralObservation {
        identifier: "peripheral-id".to_owned(),
        address: None,
        name: Some("Generic".to_owned()),
        rssi: Some(-60),
        advertised_services: smallvec![],
        manufacturer_data: smallvec![
            crate::ManufacturerDataSummary {
                company_id: 0x004c,
                len: crate::ManufacturerDataLen::new(6),
            },
            crate::ManufacturerDataSummary {
                company_id: 0x000f,
                len: crate::ManufacturerDataLen::new(2),
            },
        ],
    };

    assert_eq!(
        observation.to_string(),
        "id=peripheral-id name=Generic rssi=-60 services=[] manufacturer_data=[004c:6b,000f:2b]"
    );
}

#[test]
fn operation_timeout_error_names_the_backend_operation() {
    let error = crate::BtleError::OperationTimedOut {
        operation: "start scan",
        after: Duration::from_secs(10),
    };

    assert_eq!(
        error.to_string(),
        "bluetooth operation timed out: start scan after 10s"
    );
}

#[test]
fn btle_errors_expose_actionable_desktop_diagnostic_hints() {
    let no_adapter = crate::BtleError::NoAdapterAvailable;
    assert_eq!(
        no_adapter.diagnostic_hint(),
        "enable Bluetooth, grant Bluetooth permission to this terminal, and verify the OS exposes an adapter"
    );

    let no_match = crate::BtleError::NoPeripheralMatched;
    assert_eq!(
        no_match.diagnostic_hint(),
        "power on the device, keep it nearby, increase --seconds, or use --name-contains/--identifier to narrow selection"
    );

    let timed_out = crate::BtleError::OperationTimedOut {
        operation: "discover services",
        after: Duration::from_secs(5),
    };
    assert_eq!(
        timed_out.diagnostic_hint(),
        "retry the operation, move closer to the device, and check whether another app is holding the BLE connection"
    );

    let missing_endpoint =
        crate::BtleError::Bridge(crate::SessionBridgeError::MissingSessionEndpoint);
    assert_eq!(
        missing_endpoint.diagnostic_hint(),
        "inspect GATT services and select a device exposing a writable and notify-capable session characteristic"
    );
}

#[test]
fn connection_summary_renders_services_and_characteristics() {
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![crate::CharacteristicSummary {
                uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
            }]
            .into(),
        }]
        .into(),
    };

    assert!(summary.to_string().contains("AA:BB:CC:DD:EE:FF"));
    assert!(summary.to_string().contains("NOSFET Aero"));
    assert!(summary.to_string().contains("ffe0"));
    assert!(summary.to_string().contains("ffe1"));
}

#[test]
fn connection_summary_selects_explicit_notify_characteristic() {
    let requested = Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb);
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: None,
            name: Some("Raw device".to_owned()),
            rssi: None,
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE,
                },
                crate::CharacteristicSummary {
                    uuid: requested,
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::NOTIFY,
                },
            ]
            .into(),
        }]
        .into(),
    };

    assert_eq!(
        summary
            .select_notify_characteristic(Some(requested))
            .map(|characteristic| characteristic.uuid),
        Some(requested)
    );
}

#[test]
fn connection_summary_rejects_explicit_non_notify_characteristic() {
    let requested = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: None,
            name: Some("Raw device".to_owned()),
            rssi: None,
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![
                crate::CharacteristicSummary {
                    uuid: requested,
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE,
                },
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::NOTIFY,
                },
            ]
            .into(),
        }]
        .into(),
    };

    assert!(
        summary
            .select_notify_characteristic(Some(requested))
            .is_none()
    );
}

#[tokio::test]
async fn raw_notification_capture_subscribes_and_filters_selected_characteristic() {
    let service = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
    let selected = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
    let other = Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb);
    let peripheral = RecordingPeripheral::with_notifications(vec![
        crate::BtleNotification::from_raw_bytes(other, service, Bytes::from_static(b"\x99")),
        crate::BtleNotification::from_raw_bytes(
            selected,
            service,
            Bytes::from_static(b"\x01\x02\x03"),
        ),
    ]);
    let characteristic = crate::CharacteristicSummary {
        uuid: selected,
        service_uuid: service,
        properties: CharPropFlags::NOTIFY,
    };

    let records = crate::capture_raw_notifications(
        &peripheral,
        &characteristic,
        crate::NotificationWindow::from_millis(5),
    )
    .await
    .expect("raw notification capture succeeds");

    assert_eq!(
        peripheral
            .subscribes
            .lock()
            .expect("subscribe log")
            .as_slice(),
        &[selected]
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].characteristic, selected);
    assert_eq!(records[0].service, service);
    assert_eq!(records[0].bytes.as_raw_bytes(), [0x01, 0x02, 0x03]);
}

#[test]
fn connection_summary_selects_standard_battery_level_characteristic() {
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![crate::CharacteristicSummary {
                uuid: Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb),
                properties: CharPropFlags::READ,
            }]
            .into(),
        }]
        .into(),
    };

    assert_eq!(
        summary
            .battery_level_characteristic()
            .map(|characteristic| characteristic.uuid),
        Some(Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb))
    );
}

#[test]
fn battery_level_percent_rejects_malformed_backend_values() {
    assert_eq!(
        crate::BatteryLevelPercent::from_backend_byte(88).map(crate::BatteryLevelPercent::get),
        Some(88)
    );
    assert_eq!(
        crate::BatteryLevelPercent::from_backend_byte(100).map(crate::BatteryLevelPercent::get),
        Some(100)
    );
    assert_eq!(crate::BatteryLevelPercent::from_backend_byte(101), None);
}

#[test]
fn connection_summary_uses_identifier_when_address_is_unavailable() {
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "cb-uuid-1234".to_owned(),
            address: None,
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![].into(),
    };

    assert!(summary.to_string().contains("id=cb-uuid-1234"));
    assert!(summary.to_string().contains("name=NOSFET Aero"));
}

#[test]
fn capture_record_formats_write_bytes_with_provenance() {
    let record = crate::SessionCaptureRecord::Write {
        monotonic_ms: crate::MonotonicMs::new(7),
        characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
        mode: WriteMode::WithoutResponse,
        bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"\x01\x23\xab\xcd")),
        provenance: crate::WriteProvenance::Provisional,
    };

    assert_eq!(
        record.to_string(),
        "write t_ms=7 characteristic=0000ffe1-0000-1000-8000-00805f9b34fb mode=without-response bytes=0123abcd provisional=true"
    );
}

#[test]
fn capture_record_formats_notification_bytes_with_service() {
    let record = crate::SessionCaptureRecord::Notification {
        monotonic_ms: crate::MonotonicMs::new(11),
        characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
        service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
        bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"\xde\xad\xbe\xef")),
    };

    assert_eq!(
        record.to_string(),
        "notification t_ms=11 characteristic=0000ffe1-0000-1000-8000-00805f9b34fb service=0000ffe0-0000-1000-8000-00805f9b34fb bytes=deadbeef"
    );
}

#[test]
fn captured_btle_packet_distinguishes_malformed_bytes_from_attribute_values() {
    let valid = crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"\xde\xad"));

    assert!(matches!(
        valid,
        crate::CapturedBtlePacket::AttributeValue(_)
    ));
    assert_eq!(valid.as_raw_bytes(), [0xde, 0xad]);
    assert_eq!(valid.into_raw_bytes().as_ref(), [0xde, 0xad]);

    let oversized =
        crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(&OVERSIZED_BTLE_VALUE));
    let crate::CapturedBtlePacket::Malformed(malformed) = oversized else {
        panic!("oversized attribute value should be malformed");
    };
    assert_eq!(
        malformed.reason(),
        crate::MalformedBtlePacketReason::OversizedAttributeValue {
            max: NotificationByteLen::new(512)
        }
    );
    assert_eq!(malformed.as_raw_bytes().len(), 513);
    assert_eq!(malformed.into_raw_bytes().len(), 513);
}

#[test]
fn malformed_capture_records_render_the_btle_violation_tag() {
    let record = crate::SessionCaptureRecord::Notification {
        monotonic_ms: crate::MonotonicMs::new(11),
        characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
        service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
        bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(&OVERSIZED_BTLE_VALUE)),
    };

    let rendered = record.to_string();
    assert!(rendered.contains("btle=malformed_oversized_attribute_value max=512"));
}

#[test]
fn session_capture_converts_to_pevcap_with_summary_metadata() {
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "cb-uuid".to_owned(),
            address: None,
            name: Some("NF2557".to_owned()),
            rssi: Some(-67),
            advertised_services: smallvec![Uuid::from_u128(
                0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
            )],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![crate::CharacteristicSummary {
                uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
            }]
            .into(),
        }]
        .into(),
    };
    let capture = crate::SessionCapture {
        records: pevcap_conversion_capture_records(),
        report: crate::SessionBridgeReport::default(),
    };

    let pevcap = capture
        .to_pevcap(
            &summary,
            crate::PevcapSessionMetadata {
                wall_clock_start_unix_ms: cutout_core::WallClockUnixMillis::new(1_725_000_123_456),
                platform_id: "darwin",
                library_version: "0.1.0",
                registry_hash: [0x42; 32],
                resolved_identity: Some(PevcapResolvedIdentity {
                    protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
                    model: Some(VerifiedValue {
                        value: "NOSFET Aero".to_owned(),
                        verification: VerificationStatus::Inferred,
                    }),
                    firmware: None,
                }),
                annotations: &["live aero"],
            },
        )
        .expect("session capture converts to PEVCAP");

    assert_eq!(
        pevcap.header.wall_clock_start_unix_ms,
        cutout_core::WallClockUnixMillis::new(1_725_000_123_456)
    );
    assert_eq!(pevcap.header.platform_id, "darwin");
    assert_eq!(
        pevcap.header.write_limit,
        Some(cutout_core::TransportWriteLen::new(23))
    );
    assert_eq!(pevcap.header.advertised_services.len(), 1);
    assert_eq!(pevcap.header.gatt_fingerprints.len(), 1);
    assert_eq!(
        pevcap
            .header
            .resolved_identity
            .as_ref()
            .and_then(|identity| identity.model.as_ref().map(|model| model.value.as_str())),
        Some("NOSFET Aero")
    );
    assert_eq!(pevcap.records.len(), 4);
    assert_eq!(pevcap.records[0].direction, PevcapDirection::LinkUp);
    assert_eq!(pevcap.records[0].monotonic_ms, ms(0));
    assert_eq!(
        pevcap.records[0].link_max_write_len,
        Some(cutout_core::TransportWriteLen::new(23))
    );
    assert_eq!(pevcap.records[1].direction, PevcapDirection::Outbound);
    assert_eq!(
        pevcap.records[1].write_mode,
        Some(WriteMode::WithoutResponse)
    );
    assert_eq!(pevcap.records[1].bytes.as_ref(), b"N");
    assert_eq!(pevcap.records[2].direction, PevcapDirection::Inbound);
    let advertised_service = pevcap.header.advertised_services.first().copied();
    assert_eq!(pevcap.records[2].service, advertised_service);
    assert_eq!(pevcap.records[2].bytes.as_ref(), b"NAME=NF2557");
    assert_eq!(pevcap.records[3].direction, PevcapDirection::LinkDown);
    assert_eq!(pevcap.records[3].monotonic_ms, ms(4));
}

fn pevcap_conversion_capture_records() -> Vec<crate::SessionCaptureRecord> {
    let characteristic = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
    let service = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
    vec![
        crate::SessionCaptureRecord::Link {
            monotonic_ms: crate::MonotonicMs::new(0),
            max_write_len: Some(crate::NegotiatedWriteLen::from_mtu(23)),
        },
        crate::SessionCaptureRecord::Subscribe {
            monotonic_ms: crate::MonotonicMs::new(1),
            characteristic,
        },
        crate::SessionCaptureRecord::Write {
            monotonic_ms: crate::MonotonicMs::new(2),
            characteristic,
            mode: WriteMode::WithoutResponse,
            bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"N")),
            provenance: crate::WriteProvenance::Stable,
        },
        crate::SessionCaptureRecord::Notification {
            monotonic_ms: crate::MonotonicMs::new(3),
            characteristic,
            service,
            bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"NAME=NF2557")),
        },
        crate::SessionCaptureRecord::LinkDown {
            monotonic_ms: crate::MonotonicMs::new(4),
        },
    ]
}

#[test]
fn session_capture_pevcap_conversion_preserves_write_response_mode() {
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("Begode_Falcon".to_owned()),
            rssi: None,
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![].into(),
    };
    let capture = crate::SessionCapture {
        records: vec![
            crate::SessionCaptureRecord::Link {
                monotonic_ms: crate::MonotonicMs::new(0),
                max_write_len: None,
            },
            crate::SessionCaptureRecord::Subscribe {
                monotonic_ms: crate::MonotonicMs::new(1),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            },
            crate::SessionCaptureRecord::Write {
                monotonic_ms: crate::MonotonicMs::new(2),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                mode: WriteMode::WithResponse,
                bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"\x01\x02")),
                provenance: crate::WriteProvenance::Provisional,
            },
        ],
        report: crate::SessionBridgeReport::default(),
    };

    let pevcap = capture
        .to_pevcap(
            &summary,
            crate::PevcapSessionMetadata {
                wall_clock_start_unix_ms: cutout_core::WallClockUnixMillis::new(1),
                platform_id: "test",
                library_version: "0.1.0",
                registry_hash: [0; 32],
                resolved_identity: None,
                annotations: &[],
            },
        )
        .expect("session capture converts to PEVCAP");

    assert_eq!(pevcap.header.write_limit, None);
    assert_eq!(pevcap.records.len(), 2);
    assert_eq!(pevcap.records[0].direction, PevcapDirection::LinkUp);
    assert_eq!(pevcap.records[1].write_mode, Some(WriteMode::WithResponse));
    assert_eq!(pevcap.records[1].bytes.as_ref(), [0x01, 0x02]);
}

#[test]
fn connection_summary_finds_write_and_notify_candidates() {
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                },
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::READ,
                },
            ]
            .into(),
        }]
        .into(),
    };

    assert_eq!(summary.write_candidates().count(), 1);
    assert_eq!(summary.notify_candidates().count(), 1);
}

#[test]
fn connection_summary_selects_session_endpoints() {
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                },
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::INDICATE,
                },
            ]
            .into(),
        }]
        .into(),
    };

    let endpoints = summary
        .select_session_endpoints()
        .expect("summary has a writable characteristic");
    assert_eq!(
        endpoints.write.uuid,
        Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb)
    );
    assert_eq!(
        endpoints
            .notify
            .expect("summary has a notify-capable characteristic")
            .uuid,
        Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb)
    );
}

#[tokio::test]
async fn drive_session_reports_hints_only_identity_from_host_observer() {
    let peripheral = RecordingPeripheral::default();
    let mut session = SubscribeOnlySession;
    let summary = begode_falcon_summary("Falcon");
    let mut observer = TestIdentityObserver::default();

    let report = crate::drive_session_with_identity_observer(
        &peripheral,
        &mut session,
        GattChannel::from_bytes(
            *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
        ),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(0),
        &mut observer,
    )
    .await
    .expect("bridge reports host-supplied identity hints");

    let identity = report.identity.expect("identity hints are reported");
    assert_eq!(
        identity.confidence,
        crate::BridgeIdentityConfidence::HintsOnly
    );
    assert_eq!(identity.model, None);
    assert_eq!(identity.manufacturer, None);
    assert!(identity.evidence.has_advertised_name_hint());
    assert!(identity.evidence.has_gatt_hint());
    assert_eq!(peripheral.writes.lock().expect("write log").len(), 0);
}

#[tokio::test]
async fn drive_session_updates_identity_from_host_observer_notifications() {
    let peripheral = RecordingPeripheral::with_notifications(vec![
        crate::BtleNotification::from_raw_bytes(
            Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            Bytes::from_static(b"\x55\xaa\0\0"),
        ),
        crate::BtleNotification::from_raw_bytes(
            Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            Bytes::from_static(b"NAME=Falcon"),
        ),
    ]);
    let mut session = SubscribeOnlySession;
    let summary = begode_falcon_summary("Begode_Falcon");
    let mut observer = TestIdentityObserver::default();

    let report = crate::drive_session_with_identity_observer(
        &peripheral,
        &mut session,
        GattChannel::from_bytes(
            *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
        ),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(10),
        &mut observer,
    )
    .await
    .expect("bridge resolves host-supplied identity");

    let identity = report.identity.expect("model identity is reported");
    assert_eq!(identity.confidence, crate::BridgeIdentityConfidence::Model);
    assert_eq!(identity.manufacturer, Some("TestCo"));
    assert_eq!(identity.model, Some("Observer"));
    assert!(identity.evidence.has_passive_family_match());
    assert!(identity.evidence.has_banner_model_match());
    assert_eq!(report.notifications, notifications(2));
    assert_eq!(peripheral.writes.lock().expect("write log").len(), 0);
}

#[tokio::test]
async fn drive_session_subscribes_and_writes_matching_transport_channels() {
    let peripheral = RecordingPeripheral::default();
    let mut session = BridgeSession::default();
    let summary = crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE,
                },
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::NOTIFY,
                },
            ]
            .into(),
        }]
        .into(),
    };

    let report = crate::drive_session(
        &peripheral,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(10),
    )
    .await
    .expect("bridge accepts matching transport outputs");

    assert_eq!(report.writes, writes(1));
    assert_eq!(report.subscribes, subscribes(1));
    assert_eq!(report.notifications, notifications(0));
    assert_eq!(
        peripheral
            .subscribes
            .lock()
            .expect("subscribe log")
            .as_slice(),
        &[Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb)]
    );
    assert_eq!(
        peripheral.writes.lock().expect("write log").as_slice(),
        &[(
            Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            Bytes::from_static(b"bridge:write"),
            WriteMode::WithResponse,
        )]
    );
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn drive_session_relays_notifications_back_into_session() {
    let peripheral =
        RecordingPeripheral::with_notification(crate::BtleNotification::from_raw_bytes(
            Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
            Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            Bytes::from_static(b"\x13\x37"),
        ));
    let mut session = BridgeSession::default();
    let summary = shared_write_notify_summary("NOSFET Aero");

    let report = crate::drive_session(
        &peripheral,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(10),
    )
    .await
    .expect("bridge consumes notifications");

    assert_eq!(report.notifications, notifications(1));
    assert_eq!(
        report.notification_bytes,
        crate::NotificationByteTotal::new(2)
    );
    assert_eq!(
        report.latest_notification_len,
        Some(NotificationByteLen::new(2))
    );
    assert_eq!(
        *session
            .last_notification_channel
            .lock()
            .expect("notification channel"),
        Some(GattChannel::from_bytes([0xA1; 16]))
    );
    assert_eq!(report.telemetry, telemetry_events(1));
    assert_eq!(report.read_only_responses, read_only_responses(2));
    assert_eq!(report.telemetry_snapshot.speed, Some(speed(1_200)));
    assert_eq!(report.telemetry_snapshot.voltage, Some(voltage(84_200)));
    assert_eq!(
        report.telemetry_snapshot.battery_percent_estimated,
        Some(battery_percent_estimated(61))
    );
    assert_eq!(
        report.firmware.expect("firmware response").firmware_major,
        Some(Measured::reported(43))
    );
    assert_eq!(
        report.settings.first().expect("settings response").entries[0]
            .expect("settings entry")
            .field,
        RawFieldValue::new(0x0014, 30)
    );
    assert_eq!(report.diagnostics, diagnostic_events(1));
    assert_eq!(
        report.diagnostics_snapshot.malformed_frames,
        parser_diag_count(1)
    );
    assert_eq!(
        report.diagnostic_errors.as_slice(),
        &[DiagnosticError::from_parser_error(
            ParserError::MalformedFrame
        )]
    );
    assert!(report.events.iter().all(|event| {
        matches!(
            event,
            crate::SessionBridgeEvent::ProcessedTelemetry { .. }
                | crate::SessionBridgeEvent::ReadOnlyResponse { .. }
                | crate::SessionBridgeEvent::Diagnostics { .. }
                | crate::SessionBridgeEvent::DiagnosticError { .. }
                | crate::SessionBridgeEvent::NotificationIngest { .. }
                | crate::SessionBridgeEvent::LinkDown { .. }
        )
    }));
    assert!(report.events.iter().any(|event| matches!(
        event,
        crate::SessionBridgeEvent::ProcessedTelemetry {
            monotonic_ms,
            ..
        } if *monotonic_ms == crate::MonotonicMs::new(2)
    )));
    assert!(report.events.iter().any(|event| matches!(
        event,
        crate::SessionBridgeEvent::Diagnostics {
            monotonic_ms,
            diagnostics,
        } if *monotonic_ms == crate::MonotonicMs::new(2)
            && diagnostics.malformed_frames == parser_diag_count(1)
    )));
    assert!(report.events.iter().any(|event| matches!(
        event,
        crate::SessionBridgeEvent::ReadOnlyResponse {
            monotonic_ms,
            response: ReadOnlyResponse::Firmware(firmware),
        } if *monotonic_ms == crate::MonotonicMs::new(2)
            && firmware.firmware_major == Some(Measured::reported(43))
    )));
    assert!(report.events.iter().any(|event| matches!(
        event,
        crate::SessionBridgeEvent::ReadOnlyResponse {
            monotonic_ms,
            response: ReadOnlyResponse::Settings(settings),
        } if *monotonic_ms == crate::MonotonicMs::new(2) && settings.entries[0].is_some()
    )));
    assert!(report.events.iter().any(|event| matches!(
        event,
        crate::SessionBridgeEvent::DiagnosticError {
            monotonic_ms,
            error,
        } if *monotonic_ms == crate::MonotonicMs::new(2)
            && error.kind == cutout_core::DiagnosticErrorKind::MalformedFrame
    )));
    assert_eq!(*session.notification_count.lock().expect("count"), 1);
}

#[test]
fn parsed_notifications_are_not_eligible_for_raw_transport_logging() {
    let outputs = [SessionOutput::NotificationIngest(
        NotificationIngestOutcome::semantic_events(
            ProtocolFamily::VeteranLeaperkimNosfet,
            GattChannel::from_bytes([0xA1; 16]),
            NotificationByteLen::new(77),
            ms(7),
            SemanticEventCount::new(5),
        ),
    )];

    let outcome =
        crate::bridge::notification_decode_outcome(&outputs).expect("semantic outcome present");
    assert_eq!(
        outcome.kind(),
        crate::bridge::NotificationDecodeKind::SemanticEvents
    );
    assert_eq!(
        decode_outcome_evidence(outcome).len,
        NotificationByteLen::new(77)
    );
}

#[test]
fn notification_decode_outcome_is_bounded_typed_evidence() {
    assert!(size_of::<crate::bridge::NotificationDecodeOutcome>() <= 64);
    assert_eq!(
        size_of::<crate::bridge::NotificationDecodeKind>(),
        size_of::<u8>()
    );
}

#[test]
fn accepted_fragment_notifications_are_reported_as_buffered_decoder_input() {
    let outputs = [SessionOutput::NotificationIngest(
        NotificationIngestOutcome::buffered_fragment(
            ProtocolFamily::VeteranLeaperkimNosfet,
            GattChannel::from_bytes([0xA1; 16]),
            NotificationByteLen::new(20),
            ms(3),
        ),
    )];

    let outcome =
        crate::bridge::notification_decode_outcome(&outputs).expect("buffered outcome present");
    assert_eq!(
        outcome.kind(),
        crate::bridge::NotificationDecodeKind::BufferedFragment
    );
    assert_eq!(
        decode_outcome_evidence(outcome).len,
        NotificationByteLen::new(20)
    );
}

#[test]
fn ignored_notifications_remain_eligible_for_debug_transport_logging() {
    let outputs = [SessionOutput::NotificationIngest(
        NotificationIngestOutcome::ignored_wrong_channel(
            GattChannel::from_bytes([0xA1; 16]),
            NotificationByteLen::new(20),
            ms(3),
        ),
    )];

    let outcome =
        crate::bridge::notification_decode_outcome(&outputs).expect("ignored outcome present");
    assert_eq!(
        outcome.kind(),
        crate::bridge::NotificationDecodeKind::Ignored
    );
    assert_eq!(
        decode_outcome_evidence(outcome).len,
        NotificationByteLen::new(20)
    );
}

#[test]
fn drive_session_reports_fragment_notifications_as_typed_ingest_events() {
    let mut report = crate::SessionBridgeReport::default();
    let outcome = NotificationIngestOutcome::buffered_fragment(
        ProtocolFamily::VeteranLeaperkimNosfet,
        GattChannel::from_bytes([0xA1; 16]),
        NotificationByteLen::new(20),
        ms(3),
    );

    crate::report::process_notification_ingest_outcome(
        &mut report,
        outcome,
        crate::MonotonicMs::new(3),
    );

    assert_eq!(
        report.events.as_slice(),
        &[crate::SessionBridgeEvent::NotificationIngest {
            monotonic_ms: crate::MonotonicMs::new(3),
            outcome,
        }]
    );
}

#[test]
fn semantic_notifications_suppress_transport_logging_without_raw_notification_event() {
    let outputs = [
        SessionOutput::NotificationIngest(NotificationIngestOutcome::semantic_events(
            ProtocolFamily::VeteranLeaperkimNosfet,
            GattChannel::from_bytes([0xA1; 16]),
            NotificationByteLen::new(77),
            ms(3),
            SemanticEventCount::new(5),
        )),
        SessionOutput::Event(DeviceEvent::Telemetry(TelemetryDelta {
            voltage: Some(voltage(126_000)),
            ..TelemetryDelta::empty(ms(0))
        })),
    ];

    let outcome =
        crate::bridge::notification_decode_outcome(&outputs).expect("semantic outcome present");
    assert_eq!(
        outcome.kind(),
        crate::bridge::NotificationDecodeKind::SemanticEvents
    );
    assert_eq!(
        decode_outcome_evidence(outcome).len,
        NotificationByteLen::new(77)
    );
}

#[test]
fn known_reserved_and_parser_gap_notifications_have_distinct_decode_outcomes() {
    let channel = GattChannel::from_bytes([0xA1; 16]);
    let reserved = [SessionOutput::NotificationIngest(
        NotificationIngestOutcome::known_reserved(
            ProtocolFamily::VeteranLeaperkimNosfet,
            channel,
            NotificationByteLen::new(75),
            ms(4),
            ReservedPayloadEvidence {
                classifier: PayloadClassifier::selector(ProtocolSelector::new(8)),
                body_len: PayloadBodyLen::new(24),
                verification: VerificationStatus::HardwareVerified,
            },
        ),
    )];
    let gap = [SessionOutput::NotificationIngest(
        NotificationIngestOutcome::parser_gap(
            ProtocolFamily::VeteranLeaperkimNosfet,
            channel,
            NotificationByteLen::new(77),
            ms(5),
            ParserGapEvidence {
                classifier: PayloadClassifier::selector(ProtocolSelector::new(9)),
                body_len: PayloadBodyLen::new(26),
            },
        ),
    )];
    let diagnostic = [SessionOutput::NotificationIngest(
        NotificationIngestOutcome::parser_diagnostic(
            ProtocolFamily::VeteranLeaperkimNosfet,
            channel,
            NotificationByteLen::new(77),
            ms(6),
            ParserError::BadChecksum,
        ),
    )];

    let reserved =
        crate::bridge::notification_decode_outcome(&reserved).expect("reserved outcome present");
    assert_eq!(
        reserved.kind(),
        crate::bridge::NotificationDecodeKind::KnownReserved
    );
    assert_eq!(
        decode_outcome_evidence(reserved).len,
        NotificationByteLen::new(75)
    );

    let gap = crate::bridge::notification_decode_outcome(&gap).expect("gap outcome present");
    assert_eq!(gap.kind(), crate::bridge::NotificationDecodeKind::ParserGap);
    assert_eq!(
        decode_outcome_evidence(gap).len,
        NotificationByteLen::new(77)
    );

    let diagnostic = crate::bridge::notification_decode_outcome(&diagnostic)
        .expect("diagnostic outcome present");
    assert_eq!(
        diagnostic.kind(),
        crate::bridge::NotificationDecodeKind::ParserDiagnostic
    );
    assert_eq!(
        decode_outcome_evidence(diagnostic).len,
        NotificationByteLen::new(77)
    );
}

#[tokio::test]
async fn capture_session_records_subscribe_write_and_notification_bytes() {
    let peripheral =
        RecordingPeripheral::with_notification(crate::BtleNotification::from_raw_bytes(
            Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
            Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            Bytes::from_static(b"\x13\x37"),
        ));
    let mut session = BridgeSession::default();
    let summary = shared_write_notify_summary("NOSFET Aero");

    let capture = crate::capture_session(
        &peripheral,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(10),
        crate::WriteProvenance::Provisional,
    )
    .await
    .expect("capture consumes bridge outputs");

    assert_eq!(capture.report.notifications, notifications(1));
    assert_eq!(
        capture.records,
        vec![
            crate::SessionCaptureRecord::Link {
                monotonic_ms: crate::MonotonicMs::new(0),
                max_write_len: Some(crate::NegotiatedWriteLen::from_mtu(185)),
            },
            crate::SessionCaptureRecord::Subscribe {
                monotonic_ms: crate::MonotonicMs::new(0),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            },
            crate::SessionCaptureRecord::Write {
                monotonic_ms: crate::MonotonicMs::new(1),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                mode: WriteMode::WithResponse,
                bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(
                    b"bridge:write",
                )),
                provenance: crate::WriteProvenance::Provisional,
            },
            crate::SessionCaptureRecord::Notification {
                monotonic_ms: crate::MonotonicMs::new(2),
                characteristic: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"\x13\x37")),
            },
        ]
    );
}

#[tokio::test]
async fn capture_session_with_commands_records_command_writes_before_tick() {
    let peripheral = RecordingPeripheral::default();
    let mut session = CommandWriteSession;
    let summary = begode_falcon_summary("Begode_Falcon");

    let capture = crate::capture_session_with_commands(
        &peripheral,
        &mut session,
        GattChannel::from_bytes(
            *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
        ),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(0),
        &[
            DeviceCommand::RequestIdentity,
            DeviceCommand::RequestFirmwareInfo,
        ],
    )
    .await
    .expect("capture records explicit command writes");

    assert_eq!(capture.report.writes, writes(2));
    assert_eq!(
        capture.records,
        vec![
            crate::SessionCaptureRecord::Link {
                monotonic_ms: crate::MonotonicMs::new(0),
                max_write_len: Some(crate::NegotiatedWriteLen::from_mtu(185)),
            },
            crate::SessionCaptureRecord::Subscribe {
                monotonic_ms: crate::MonotonicMs::new(0),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            },
            crate::SessionCaptureRecord::Write {
                monotonic_ms: crate::MonotonicMs::new(1),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                mode: WriteMode::WithoutResponse,
                bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"N")),
                provenance: crate::WriteProvenance::Stable,
            },
            crate::SessionCaptureRecord::Write {
                monotonic_ms: crate::MonotonicMs::new(2),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                mode: WriteMode::WithoutResponse,
                bytes: crate::CapturedBtlePacket::from_raw_bytes(Bytes::from_static(b"V")),
                provenance: crate::WriteProvenance::Stable,
            },
        ]
    );
}

#[tokio::test]
async fn capture_session_chunks_writes_by_negotiated_write_limit() {
    let peripheral = RecordingPeripheral::with_mtu(4);
    let mut session = LargeWriteSession;
    let summary = shared_write_notify_summary("NOSFET Aero");

    let capture = crate::capture_session(
        &peripheral,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(0),
        crate::WriteProvenance::Stable,
    )
    .await
    .expect("capture chunks oversized bridge writes");

    assert_eq!(capture.report.protocol_writes, protocol_writes(1));
    assert_eq!(capture.report.writes, writes(3));
    assert_eq!(
        peripheral.writes.lock().expect("write log").as_slice(),
        &[
            (
                Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                Bytes::from_static(b"0123"),
                WriteMode::WithoutResponse,
            ),
            (
                Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                Bytes::from_static(b"4567"),
                WriteMode::WithoutResponse,
            ),
            (
                Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                Bytes::from_static(b"89"),
                WriteMode::WithoutResponse,
            ),
        ]
    );
    let writes: Vec<_> = capture
        .records
        .iter()
        .filter_map(|record| match record {
            crate::SessionCaptureRecord::Write { bytes, mode, .. } => {
                Some((bytes.as_raw_bytes(), *mode))
            }
            crate::SessionCaptureRecord::Link { .. }
            | crate::SessionCaptureRecord::LinkDown { .. }
            | crate::SessionCaptureRecord::Subscribe { .. }
            | crate::SessionCaptureRecord::Notification { .. } => None,
        })
        .collect();
    assert_eq!(
        writes,
        vec![
            (b"0123".as_slice(), WriteMode::WithoutResponse),
            (b"4567".as_slice(), WriteMode::WithoutResponse),
            (b"89".as_slice(), WriteMode::WithoutResponse),
        ]
    );
}

#[tokio::test]
async fn drive_session_feeds_link_down_after_intentional_disconnect() {
    let peripheral = RecordingPeripheral::default();
    let mut session = DisconnectOnTickSession::default();
    let summary = shared_write_notify_summary("NOSFET Aero");

    let report = crate::drive_session(
        &peripheral,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(0),
    )
    .await
    .expect("bridge handles intentional disconnect");

    assert_eq!(report.disconnects, disconnects(1));
    assert_eq!(
        report.events.as_slice(),
        &[crate::SessionBridgeEvent::LinkDown {
            monotonic_ms: crate::MonotonicMs::new(1)
        }]
    );
    assert_eq!(*session.link_down_count.lock().expect("count"), 1);
    assert_eq!(*peripheral.disconnects.lock().expect("disconnect log"), 1);
}

#[tokio::test]
async fn capture_session_records_link_down_after_intentional_disconnect() {
    let peripheral = RecordingPeripheral::default();
    let mut session = DisconnectOnTickSession::default();
    let summary = shared_write_notify_summary("NOSFET Aero");

    let capture = crate::capture_session(
        &peripheral,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        &summary,
        summary
            .select_session_endpoints()
            .expect("summary has session endpoints"),
        crate::NotificationWindow::from_millis(0),
        crate::WriteProvenance::Stable,
    )
    .await
    .expect("capture records intentional disconnect");

    assert_eq!(
        capture.records,
        vec![
            crate::SessionCaptureRecord::Link {
                monotonic_ms: crate::MonotonicMs::new(0),
                max_write_len: Some(crate::NegotiatedWriteLen::from_mtu(185)),
            },
            crate::SessionCaptureRecord::LinkDown {
                monotonic_ms: crate::MonotonicMs::new(1)
            },
        ]
    );
    assert_eq!(capture.report.disconnects, disconnects(1));
}

#[tokio::test]
async fn capture_reconnecting_session_restores_subscription_after_disconnect() {
    let first = RecordingPeripheral::default();
    let second = RecordingPeripheral::default();
    let mut host = FakeReconnectHost::new(vec![first.clone(), second.clone()]);
    let mut session = ReconnectOnceSession::default();

    let reconnecting_capture = crate::capture_reconnecting_session_with_summaries(
        &mut host,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        crate::NotificationWindow::from_millis(0),
        crate::MaxReconnectLinks::at_least_one(2),
        crate::WriteProvenance::Stable,
    )
    .await
    .expect("fake host reconnects once");
    let capture = reconnecting_capture.capture;

    assert_eq!(host.connects, 2);
    assert_eq!(reconnecting_capture.attempts.len(), 2);
    assert_eq!(
        reconnecting_capture.attempts[0].attempt,
        crate::ReconnectAttempt::new(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[0].report.subscribes,
        subscribes(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[0].report.disconnects,
        disconnects(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[1].attempt,
        crate::ReconnectAttempt::new(2)
    );
    assert_eq!(
        reconnecting_capture.attempts[1].report.subscribes,
        subscribes(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[1].report.disconnects,
        disconnects(0)
    );
    assert_eq!(
        reconnecting_capture.attempts[0]
            .summary
            .observation
            .name
            .as_deref(),
        Some("NOSFET Aero")
    );
    assert_eq!(
        reconnecting_capture.attempts[1]
            .summary
            .observation
            .name
            .as_deref(),
        Some("NOSFET Aero")
    );
    assert_eq!(capture.report.subscribes, subscribes(2));
    assert_eq!(capture.report.disconnects, disconnects(1));
    assert_eq!(*session.link_ups.lock().expect("link ups"), 2);
    assert_eq!(*session.link_downs.lock().expect("link downs"), 1);
    assert_eq!(first.subscribes.lock().expect("first subscribes").len(), 1);
    assert_eq!(
        second.subscribes.lock().expect("second subscribes").len(),
        1
    );
    assert_eq!(*first.disconnects.lock().expect("first disconnects"), 1);
    assert_eq!(*second.disconnects.lock().expect("second disconnects"), 0);
    assert_eq!(
        capture.records,
        vec![
            crate::SessionCaptureRecord::Link {
                monotonic_ms: crate::MonotonicMs::new(0),
                max_write_len: Some(crate::NegotiatedWriteLen::from_mtu(185)),
            },
            crate::SessionCaptureRecord::Subscribe {
                monotonic_ms: crate::MonotonicMs::new(0),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            },
            crate::SessionCaptureRecord::LinkDown {
                monotonic_ms: crate::MonotonicMs::new(1)
            },
            crate::SessionCaptureRecord::Link {
                monotonic_ms: crate::MonotonicMs::new(2),
                max_write_len: Some(crate::NegotiatedWriteLen::from_mtu(185)),
            },
            crate::SessionCaptureRecord::Subscribe {
                monotonic_ms: crate::MonotonicMs::new(2),
                characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            },
        ]
    );
}

#[tokio::test]
async fn capture_reconnecting_session_preserves_partial_capture_when_reconnect_fails() {
    let first = RecordingPeripheral::default();
    let mut host = FakeReconnectHost::new(vec![first.clone()]);
    let mut session = ReconnectOnceSession::default();

    let reconnecting_capture = crate::capture_reconnecting_session_with_summaries(
        &mut host,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        crate::NotificationWindow::from_millis(0),
        crate::MaxReconnectLinks::at_least_one(2),
        crate::WriteProvenance::Stable,
    )
    .await
    .expect("partial reconnect evidence is preserved when the next link fails");
    let capture = reconnecting_capture.capture;

    assert_eq!(host.connects, 2);
    assert_eq!(reconnecting_capture.attempts.len(), 1);
    assert_eq!(capture.report.subscribes, subscribes(1));
    assert_eq!(capture.report.disconnects, disconnects(1));
    assert_eq!(*session.link_ups.lock().expect("link ups"), 1);
    assert_eq!(*session.link_downs.lock().expect("link downs"), 1);
    assert_eq!(*first.disconnects.lock().expect("first disconnects"), 1);
    assert!(
        capture
            .records
            .iter()
            .any(|record| matches!(record, crate::SessionCaptureRecord::LinkDown { .. }))
    );
}

#[tokio::test]
async fn capture_reconnecting_session_retries_after_external_notification_stream_end() {
    let first = RecordingPeripheral::with_notification(crate::BtleNotification::from_raw_bytes(
        Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
        Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
        Bytes::from_static(b"\x13\x37"),
    ));
    let second = RecordingPeripheral::with_notification(crate::BtleNotification::from_raw_bytes(
        Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
        Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
        Bytes::from_static(b"\x24\x68"),
    ));
    let mut host = FakeReconnectHost::new(vec![first.clone(), second.clone()]);
    let mut session = ExternalLinkLossSession::default();

    let reconnecting_capture = crate::capture_reconnecting_session_with_summaries(
        &mut host,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        crate::NotificationWindow::from_millis(10),
        crate::MaxReconnectLinks::at_least_one(2),
        crate::WriteProvenance::Stable,
    )
    .await
    .expect("fake host reconnects after external link loss");
    let capture = reconnecting_capture.capture;

    assert_eq!(host.connects, 2);
    assert_eq!(reconnecting_capture.attempts.len(), 2);
    assert_eq!(
        reconnecting_capture.attempts[0].report.notifications,
        notifications(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[0].report.disconnects,
        disconnects(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[1].report.notifications,
        notifications(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[1].report.disconnects,
        disconnects(1)
    );
    assert_eq!(capture.report.notifications, notifications(2));
    assert_eq!(capture.report.disconnects, disconnects(2));
    assert_eq!(*session.link_ups.lock().expect("link ups"), 2);
    assert_eq!(*session.link_downs.lock().expect("link downs"), 2);
    assert_eq!(*first.disconnects.lock().expect("first disconnects"), 0);
    assert_eq!(*second.disconnects.lock().expect("second disconnects"), 0);
    assert!(capture.records.iter().any(|record| matches!(
        record,
        crate::SessionCaptureRecord::Notification {
            monotonic_ms,
            ..
        } if *monotonic_ms == crate::MonotonicMs::new(2)
    )));
    assert!(capture.records.iter().any(|record| matches!(
        record,
        crate::SessionCaptureRecord::LinkDown {
            monotonic_ms,
        } if *monotonic_ms == crate::MonotonicMs::new(3)
    )));
    assert!(capture.records.iter().any(|record| matches!(
        record,
        crate::SessionCaptureRecord::Link {
            monotonic_ms,
            ..
        } if *monotonic_ms == crate::MonotonicMs::new(4)
    )));
}

#[tokio::test]
async fn capture_reconnecting_session_retries_after_external_notification_idle() {
    let first = RecordingPeripheral::with_open_notifications(vec![
        crate::BtleNotification::from_raw_bytes(
            Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
            Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            Bytes::from_static(b"\x13\x37"),
        ),
    ]);
    let second = RecordingPeripheral::with_notification(crate::BtleNotification::from_raw_bytes(
        Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
        Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
        Bytes::from_static(b"\x24\x68"),
    ));
    let mut host = FakeReconnectHost::new(vec![first.clone(), second]);
    let mut session = ExternalLinkLossSession::default();

    let reconnecting_capture = crate::capture_reconnecting_session_with_summaries(
        &mut host,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        crate::NotificationWindow::from_millis(2_000),
        crate::MaxReconnectLinks::at_least_one(2),
        crate::WriteProvenance::Stable,
    )
    .await
    .expect("fake host reconnects after idle link loss");
    let capture = reconnecting_capture.capture;

    assert_eq!(host.connects, 2);
    assert_eq!(reconnecting_capture.attempts.len(), 2);
    assert_eq!(
        reconnecting_capture.attempts[0].report.notifications,
        notifications(1)
    );
    assert_eq!(
        reconnecting_capture.attempts[0].report.disconnects,
        disconnects(1)
    );
    assert_eq!(capture.report.notifications, notifications(2));
    assert_eq!(capture.report.disconnects, disconnects(2));
    assert_eq!(*session.link_ups.lock().expect("link ups"), 2);
    assert_eq!(*session.link_downs.lock().expect("link downs"), 2);
    assert_eq!(*first.disconnects.lock().expect("first disconnects"), 0);
    assert!(capture.records.iter().any(|record| matches!(
        record,
        crate::SessionCaptureRecord::LinkDown {
            monotonic_ms,
        } if *monotonic_ms == crate::MonotonicMs::new(3)
    )));
    assert!(capture.records.iter().any(|record| matches!(
        record,
        crate::SessionCaptureRecord::Link {
            monotonic_ms,
            ..
        } if *monotonic_ms == crate::MonotonicMs::new(4)
    )));
}

#[tokio::test]
async fn capture_reconnecting_session_cancels_commands_after_reconnect() {
    let first = RecordingPeripheral::default();
    let second = RecordingPeripheral::default();
    let mut host = FakeReconnectHost::new(vec![first.clone(), second.clone()]);
    let mut session = CommandThenDisconnectSession::default();

    let capture = crate::capture_reconnecting_session_with_commands(
        &mut host,
        &mut session,
        GattChannel::from_bytes([0xA1; 16]),
        crate::NotificationWindow::from_millis(0),
        crate::MaxReconnectLinks::at_least_one(2),
        crate::WriteProvenance::Stable,
        &[
            DeviceCommand::RequestIdentity,
            DeviceCommand::RequestFirmwareInfo,
        ],
    )
    .await
    .expect("fake host reconnects after first-link commands");

    assert_eq!(capture.attempts.len(), 2);
    assert_eq!(capture.attempts[0].report.writes, writes(2));
    assert_eq!(capture.attempts[1].report.writes, writes(0));
    assert_eq!(
        first.writes.lock().expect("first writes").as_slice(),
        &[
            (
                Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                Bytes::from_static(b"N"),
                WriteMode::WithoutResponse,
            ),
            (
                Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                Bytes::from_static(b"V"),
                WriteMode::WithoutResponse,
            ),
        ]
    );
    assert!(second.writes.lock().expect("second writes").is_empty());
    assert_eq!(
        second.subscribes.lock().expect("second subscribes").len(),
        1
    );
}

#[derive(Default)]
struct BridgeSession {
    notification_count: Arc<Mutex<usize>>,
    last_notification_channel: Arc<Mutex<Option<GattChannel>>>,
}

struct SubscribeOnlySession;

impl ProtocolSession for SubscribeOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        if matches!(input, SessionInput::LinkUp(_)) {
            output.push(SessionOutput::Transport(TransportAction::Subscribe {
                channel: GattChannel::from_bytes(
                    *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                ),
            }));
        }
    }
}

struct CommandWriteSession;

#[derive(Default)]
struct CommandThenDisconnectSession {
    link_ups: usize,
}

struct LargeWriteSession;

#[derive(Default)]
struct DisconnectOnTickSession {
    link_down_count: Arc<Mutex<usize>>,
}

#[derive(Default)]
struct ReconnectOnceSession {
    link_ups: Arc<Mutex<usize>>,
    link_downs: Arc<Mutex<usize>>,
}

#[derive(Default)]
struct ExternalLinkLossSession {
    link_ups: Arc<Mutex<usize>>,
    link_downs: Arc<Mutex<usize>>,
}

struct FakeReconnectHost {
    peripherals: std::collections::VecDeque<RecordingPeripheral>,
    connects: usize,
}

impl FakeReconnectHost {
    fn new(peripherals: Vec<RecordingPeripheral>) -> Self {
        Self {
            peripherals: peripherals.into(),
            connects: 0,
        }
    }
}

#[async_trait::async_trait]
impl crate::ReconnectingSessionHost for FakeReconnectHost {
    type Peripheral = RecordingPeripheral;

    async fn connect(
        &mut self,
    ) -> Result<(Self::Peripheral, crate::ConnectionSummary), crate::BtleError> {
        self.connects += 1;
        self.peripherals
            .pop_front()
            .map(|peripheral| (peripheral, shared_write_notify_summary("NOSFET Aero")))
            .ok_or(crate::BtleError::NoPeripheralMatched)
    }
}

impl ProtocolSession for CommandWriteSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(_) => {
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: GattChannel::from_bytes(
                        *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                    ),
                }));
            }
            SessionInput::Command(command) => {
                let bytes = match command {
                    DeviceCommand::RequestIdentity => b"N".as_slice(),
                    DeviceCommand::RequestFirmwareInfo => b"V".as_slice(),
                    _ => return,
                };
                output.push(SessionOutput::Transport(TransportAction::Write {
                    channel: GattChannel::from_bytes(
                        *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                    ),
                    bytes: cutout_core::WritePayload::try_from_slice(bytes)
                        .expect("fixture payload fits"),
                    mode: WriteMode::WithoutResponse,
                }));
            }
            SessionInput::LinkDown
            | SessionInput::Tick { .. }
            | SessionInput::Notification { .. } => {}
        }
    }
}

impl ProtocolSession for LargeWriteSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        if matches!(input, SessionInput::Tick { .. }) {
            output.push(SessionOutput::Transport(TransportAction::Write {
                channel: GattChannel::from_bytes([0xA1; 16]),
                bytes: cutout_core::WritePayload::try_from_slice(b"0123456789")
                    .expect("fixture payload fits"),
                mode: WriteMode::WithoutResponse,
            }));
        }
    }
}

impl ProtocolSession for CommandThenDisconnectSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(_) => {
                self.link_ups += 1;
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                }));
            }
            SessionInput::Command(command) => {
                let bytes = match command {
                    DeviceCommand::RequestIdentity => b"N".as_slice(),
                    DeviceCommand::RequestFirmwareInfo => b"V".as_slice(),
                    _ => return,
                };
                output.push(SessionOutput::Transport(TransportAction::Write {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                    bytes: cutout_core::WritePayload::try_from_slice(bytes)
                        .expect("fixture payload fits"),
                    mode: WriteMode::WithoutResponse,
                }));
            }
            SessionInput::Tick { .. } => {
                if self.link_ups == 1 {
                    output.push(SessionOutput::Transport(TransportAction::Disconnect));
                }
            }
            SessionInput::LinkDown | SessionInput::Notification { .. } => {}
        }
    }
}

impl ProtocolSession for DisconnectOnTickSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::Tick { .. } => {
                output.push(SessionOutput::Transport(TransportAction::Disconnect));
            }
            SessionInput::LinkDown => {
                *self.link_down_count.lock().expect("link down count") += 1;
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::LinkUp(_)
            | SessionInput::Command(_)
            | SessionInput::Notification { .. } => {}
        }
    }
}

impl ProtocolSession for ReconnectOnceSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(_) => {
                let mut link_ups = self.link_ups.lock().expect("link ups");
                *link_ups += 1;
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                }));
            }
            SessionInput::Tick { .. } => {
                if *self.link_ups.lock().expect("link ups") == 1 {
                    output.push(SessionOutput::Transport(TransportAction::Disconnect));
                }
            }
            SessionInput::LinkDown => {
                *self.link_downs.lock().expect("link downs") += 1;
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Command(_) | SessionInput::Notification { .. } => {}
        }
    }
}

impl ProtocolSession for ExternalLinkLossSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(_) => {
                *self.link_ups.lock().expect("link ups") += 1;
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                }));
            }
            SessionInput::LinkDown => {
                *self.link_downs.lock().expect("link downs") += 1;
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Command(_)
            | SessionInput::Notification { .. }
            | SessionInput::Tick { .. } => {}
        }
    }
}

impl ProtocolSession for BridgeSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(_) => {
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                }));
            }
            SessionInput::Tick { .. } => {
                output.push(SessionOutput::Transport(TransportAction::Write {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                    bytes: cutout_core::WritePayload::try_from_slice(b"bridge:write")
                        .expect("fixture payload fits"),
                    mode: WriteMode::WithResponse,
                }));
            }
            SessionInput::Notification { channel, .. } => {
                *self
                    .notification_count
                    .lock()
                    .expect("notification counter") += 1;
                *self
                    .last_notification_channel
                    .lock()
                    .expect("notification channel") = Some(channel);
                output.push(SessionOutput::NotificationIngest(
                    NotificationIngestOutcome::semantic_events(
                        ProtocolFamily::VeteranLeaperkimNosfet,
                        GattChannel::from_bytes([0xA1; 16]),
                        NotificationByteLen::new(2),
                        ms(0),
                        SemanticEventCount::new(1),
                    ),
                ));
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    TelemetryDelta {
                        speed: Some(speed(1_200)),
                        voltage: Some(voltage(84_200)),
                        battery_percent_estimated: Some(battery_percent_estimated(61)),
                        ..TelemetryDelta::empty(ms(0))
                    },
                )));
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    ReadOnlyResponse::Firmware(FirmwareInfo {
                        firmware_major: Some(Measured::reported(43)),
                        ..FirmwareInfo::default()
                    }),
                )));
                output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    ReadOnlyResponse::Settings(SettingsReadback {
                        entries: [
                            Some(SettingsEntry {
                                field: RawFieldValue::new(0x0014, 30),
                                source: ValueSource::Reported,
                                quality: ValueQuality::Known,
                                verification: VerificationStatus::HardwareVerified,
                            }),
                            None,
                            None,
                            None,
                        ],
                    }),
                )));
                output.push(SessionOutput::Event(DeviceEvent::DiagnosticError(
                    DiagnosticError::from_parser_error(ParserError::MalformedFrame),
                )));
                output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                    ParserDiagnostics {
                        malformed_frames: parser_diag_count(1),
                        ..ParserDiagnostics::default()
                    },
                )));
            }
            SessionInput::LinkDown | SessionInput::Command(_) => {}
        }
    }
}

#[derive(Clone, Debug)]
struct RecordingPeripheral {
    subscribes: Arc<Mutex<Vec<Uuid>>>,
    writes: WriteLog,
    notifications: NotificationLog,
    disconnects: Arc<Mutex<usize>>,
    mtu: u16,
    keep_notifications_open: bool,
}

impl Default for RecordingPeripheral {
    fn default() -> Self {
        Self {
            subscribes: Arc::new(Mutex::new(Vec::new())),
            writes: Arc::new(Mutex::new(Vec::new())),
            notifications: Arc::new(Mutex::new(Vec::new())),
            disconnects: Arc::new(Mutex::new(0)),
            mtu: 185,
            keep_notifications_open: false,
        }
    }
}

impl RecordingPeripheral {
    fn with_mtu(mtu: u16) -> Self {
        Self {
            mtu,
            ..Self::default()
        }
    }

    fn with_notification(notification: crate::BtleNotification) -> Self {
        Self::with_notifications(vec![notification])
    }

    fn with_notifications(notifications: Vec<crate::BtleNotification>) -> Self {
        Self {
            notifications: Arc::new(Mutex::new(notifications)),
            ..Self::default()
        }
    }

    fn with_open_notifications(notifications: Vec<crate::BtleNotification>) -> Self {
        Self {
            notifications: Arc::new(Mutex::new(notifications)),
            keep_notifications_open: true,
            ..Self::default()
        }
    }
}

fn begode_falcon_summary(name: &str) -> crate::ConnectionSummary {
    crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some(name.to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![Uuid::from_u128(
                0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
            )],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![crate::CharacteristicSummary {
                uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
            }]
            .into(),
        }]
        .into(),
    }
}

fn shared_write_notify_summary(name: &str) -> crate::ConnectionSummary {
    crate::ConnectionSummary {
        observation: crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some(name.to_owned()),
            rssi: Some(-42),
            advertised_services: crate::AdvertisedServices::new(),
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        },
        services: vec![crate::ServiceSummary {
            uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            primary: true,
            characteristics: vec![
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                },
                crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::NOTIFY,
                },
            ]
            .into(),
        }]
        .into(),
    }
}

#[async_trait::async_trait]
impl crate::SessionPeripheral for RecordingPeripheral {
    fn mtu(&self) -> u16 {
        self.mtu
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), crate::BtleError> {
        self.subscribes
            .lock()
            .expect("subscribe log")
            .push(characteristic.uuid);
        Ok(())
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        chunk: crate::BtleWriteChunk<'_>,
        mode: WriteMode,
    ) -> Result<(), crate::BtleError> {
        self.writes.lock().expect("write log").push((
            characteristic.uuid,
            Bytes::copy_from_slice(chunk.as_slice()),
            mode,
        ));
        Ok(())
    }

    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn stream::Stream<Item = crate::BtleNotification> + Send>>, crate::BtleError>
    {
        let notifications = self.notifications.lock().expect("notification log").clone();
        if self.keep_notifications_open {
            Ok(Box::pin(
                stream::iter(notifications).chain(stream::pending()),
            ))
        } else {
            Ok(Box::pin(stream::iter(notifications)))
        }
    }

    async fn disconnect(&self) -> Result<(), crate::BtleError> {
        *self.disconnects.lock().expect("disconnect log") += 1;
        Ok(())
    }
}
