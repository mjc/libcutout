#![cfg_attr(test, allow(clippy::disallowed_macros))]
#![allow(unsafe_code)]

#[path = "../../../tests/support/allocation_counter.rs"]
mod allocation_counter;

use allocation_counter::assert_no_allocations;

use cutout_core::{
    BatteryCurrent, BatteryLevel, BatteryLevelBasis, Capacity, CapacitySource, CaptureRecord,
    ChargeEstimateInput, ChargeEstimator, ChargeFlow, ChargeMode, ChargeProfileIdentity,
    ChargeSessionIdentity, CommandKind, Duration, GattChannel, HostSession, LinkInfo,
    ManufacturerKey, Measured, ModelCatalog, ModelCatalogEntry, ModelKey, ModelRegistryEntry,
    ModelRuntimeRegistration, MonotonicTimestamp, ParserKey, ProtocolFamily, ProtocolSession,
    RequestKey, RequestPolicy, RequestQueue, RequestScheduler, RequestUrgency, SessionInput,
    SessionKey, SessionOutput, TelemetryFreshness, TransportWriteLimit, UsablePackCapacity,
    VerificationStatus, Voltage, VoltageSagEstimator, VoltageSagInput,
};

const fn ms(value: u64) -> MonotonicTimestamp {
    MonotonicTimestamp::new(value)
}

const fn write_len(value: u16) -> TransportWriteLimit {
    TransportWriteLimit::from_bytes(value)
}

const CATALOG_GATT: [cutout_core::GattFingerprint; 1] = [cutout_core::GattFingerprint {
    service: GattChannel::from_bytes([0x11; 16]),
    characteristic: GattChannel::from_bytes([0x22; 16]),
    roles: cutout_core::GattRoles::empty()
        .with_write_without_response()
        .with_notify(),
    verification: VerificationStatus::SourceVerified,
}];

static CATALOG_REGISTRY_ENTRY: ModelRegistryEntry = ModelRegistryEntry {
    manufacturer: ManufacturerKey::new("NOSFET"),
    model: ModelKey::new("Aero"),
    protocol_family: ProtocolFamily::VeteranLeaperkimNosfet,
    advertised_name_hints: &["NF"],
    wire_model_id: None,
    battery: None,
    bms: None,
    gatt: &CATALOG_GATT,
    capabilities: cutout_core::Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestTelemetry,
    ]),
    verification: VerificationStatus::Inferred,
};

static CATALOG_ENTRIES: [ModelCatalogEntry; 1] = [ModelCatalogEntry {
    registry: &CATALOG_REGISTRY_ENTRY,
    registration: ModelRuntimeRegistration {
        parser: Some(ParserKey::new("veteran")),
        session: Some(SessionKey::new("nosfet-aero-read-only")),
    },
}];

#[derive(Default)]
struct NoOpSession;

impl ProtocolSession for NoOpSession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(info) => {
                output.push(SessionOutput::Event(cutout_core::DeviceEvent::LinkUp(info)));
            }
            SessionInput::LinkDown => {
                output.push(SessionOutput::Event(cutout_core::DeviceEvent::LinkDown));
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => {
                output.push(SessionOutput::NotificationIngest(
                    cutout_core::NotificationIngestOutcome::ignored_wrong_channel(
                        channel,
                        cutout_core::NotificationByteLen::from_bytes(bytes.len()),
                        monotonic_ms,
                    ),
                ));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(cutout_core::DeviceEvent::Tick {
                    monotonic_ms,
                }));
            }
            SessionInput::Command(_) => {}
        }
    }
}

#[test]
fn hot_paths_do_not_allocate_for_borrowed_or_bounded_inputs() {
    borrowed_or_bounded_inputs_do_not_allocate();
    large_catalog_lookup_does_not_allocate_after_setup();
    charge_estimator_update_does_not_allocate();
    voltage_sag_estimator_update_does_not_allocate();
}

fn borrowed_or_bounded_inputs_do_not_allocate() {
    let mut link_up_host = HostSession::new(NoOpSession);
    assert_no_allocations("host link-up", || {
        link_up_host.ingest_link_up(LinkInfo {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len(185)),
        });
        let drained = link_up_host.drain_outputs();

        assert_eq!(drained.len(), 1);
    });

    let mut notification_host = HostSession::new(NoOpSession);
    let notification = vec![0x11, 0x22, 0x33];
    assert_no_allocations("notification ingest", || {
        notification_host.ingest_notification_owned(
            GattChannel::from_bytes([0x22; 16]),
            notification,
            ms(20),
        );
        let drained = notification_host.drain_outputs();

        assert_eq!(drained.len(), 1);
    });

    assert_no_allocations("request queue churn", || {
        let mut queue = RequestQueue::<3>::new();
        let telemetry = cutout_core::QueuedRequest::new(
            RequestKey::new(CommandKind::RequestTelemetry),
            RequestPolicy::default(),
        );
        let identity = cutout_core::QueuedRequest::new(
            RequestKey::new(CommandKind::RequestIdentity),
            RequestPolicy::default(),
        );

        assert_eq!(queue.enqueue(telemetry), Ok(()));
        assert_eq!(queue.enqueue(identity), Ok(()));
        assert_eq!(queue.pop_next(), Some(telemetry));
        assert_eq!(queue.pop_next(), Some(identity));
        assert_eq!(queue.pop_next(), None);
    });

    assert_no_allocations("request scheduler aging churn", || {
        let mut scheduler = RequestScheduler::<3>::new();
        let telemetry = cutout_core::QueuedRequest::new(
            RequestKey::new(CommandKind::RequestTelemetry),
            RequestPolicy::default(),
        );
        let identity = cutout_core::QueuedRequest::with_urgency(
            RequestKey::new(CommandKind::RequestIdentity),
            RequestPolicy::default(),
            RequestUrgency::Critical,
        );
        let firmware = cutout_core::QueuedRequest::with_urgency(
            RequestKey::new(CommandKind::RequestFirmwareInfo),
            RequestPolicy::default(),
            RequestUrgency::Critical,
        );

        assert_eq!(scheduler.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.enqueue_by_urgency(firmware), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(firmware));
        assert_eq!(scheduler.pop_next(), Some(telemetry));
    });

    assert_no_allocations("model catalog lookup", || {
        let catalog = ModelCatalog::new(&CATALOG_ENTRIES);
        let entry = catalog
            .find_model(ManufacturerKey::new("NOSFET"), ModelKey::new("Aero"))
            .expect("static catalog entry exists");

        assert_eq!(entry.registry.model, "Aero");
    });

    // Warm platform CPU-feature detection outside the measured steady state.
    std::hint::black_box(cutout_core::registry_entries_hash(&[
        &CATALOG_REGISTRY_ENTRY,
    ]));
    assert_no_allocations("registry fingerprint", || {
        std::hint::black_box(cutout_core::registry_entries_hash(&[]));
        std::hint::black_box(cutout_core::registry_entries_hash(&[
            &CATALOG_REGISTRY_ENTRY,
        ]));
    });

    assert_no_allocations("model catalog registration lookup", || {
        let catalog = ModelCatalog::new(&CATALOG_ENTRIES);
        let parser_entry = catalog
            .find_parser(ParserKey::new("veteran"))
            .expect("static parser registration exists");
        let session_entry = catalog
            .find_session(SessionKey::new("nosfet-aero-read-only"))
            .expect("static session registration exists");

        assert_eq!(parser_entry.model_key(), ModelKey::new("Aero"));
        assert_eq!(session_entry.model_key(), ModelKey::new("Aero"));
    });

    let mut replay_host = HostSession::new(NoOpSession);
    let mut replay_outputs = Vec::with_capacity(4);
    let replay_records = [CaptureRecord::notification(
        GattChannel::from_bytes([0x44; 16]),
        vec![0x11, 0x22, 0x33],
        ms(30),
    )];
    assert_no_allocations("borrowed replay capture notification", || {
        cutout_core::replay_capture_into(&mut replay_host, &replay_records, &mut replay_outputs);

        assert_eq!(replay_outputs.len(), 1);
    });
}

fn charge_estimator_update_does_not_allocate() {
    let mut estimator = ChargeEstimator::new();
    let capacity = UsablePackCapacity::new(
        Capacity::from_milliamp_hours(10_000),
        CapacitySource::ProtocolProfile,
        VerificationStatus::SourceAndHardwareVerified,
    );
    let update = |estimator: &mut ChargeEstimator, at: u64| {
        estimator.update(ChargeEstimateInput {
            session: ChargeSessionIdentity::new(1),
            profile: ChargeProfileIdentity::new(1),
            at: ms(at),
            observed_at: ms(at),
            battery_current: Some(Measured::reported(BatteryCurrent::current_in(2_000))),
            charge_mode: Measured::reported(ChargeMode::Charging),
            flow: Measured::reported(ChargeFlow::Charging),
            battery_level: BatteryLevelBasis::reported(Measured::reported(
                BatteryLevel::from_percent(50),
            )),
            usable_capacity: capacity,
            battery_temperature: None,
            voltage_sag: None,
            freshness: TelemetryFreshness::new(Duration::from_seconds(60)),
        })
    };

    assert_no_allocations("charge estimator update", || {
        assert!(matches!(
            update(&mut estimator, 0),
            cutout_core::ChargeEstimateState::CollectingSamples { .. }
        ));
        assert!(matches!(
            update(&mut estimator, 15_000),
            cutout_core::ChargeEstimateState::CollectingSamples { .. }
        ));
        assert!(matches!(
            update(&mut estimator, 30_000),
            cutout_core::ChargeEstimateState::Available(_)
        ));
    });
}

fn voltage_sag_estimator_update_does_not_allocate() {
    let mut estimator = VoltageSagEstimator::new();
    let update = |estimator: &mut VoltageSagEstimator, at, voltage, current| {
        estimator.update(VoltageSagInput {
            at: ms(at),
            flow: ChargeFlow::Discharging,
            voltage: Measured::reported(Voltage::from_millivolts(voltage)),
            battery_current: Measured::reported(BatteryCurrent::current_out(
                u32::try_from(current).unwrap(),
            )),
            freshness: TelemetryFreshness::new(Duration::from_seconds(30)),
        })
    };

    assert_no_allocations("voltage sag estimator update", || {
        assert_eq!(update(&mut estimator, 0, 100_000, 0), None);
        assert!(update(&mut estimator, 1_000, 99_000, 10_000).is_some());
    });
}

fn large_catalog_lookup_does_not_allocate_after_setup() {
    let large_catalog_entries: Vec<_> = (0..1_000).map(synthetic_catalog_entry).collect();

    assert_no_allocations("large model catalog borrowed lookup", || {
        let catalog = ModelCatalog::new(&large_catalog_entries);
        let model_entry = catalog
            .find_model_names("Synthetic", "Model0999")
            .expect("synthetic model exists");

        assert_eq!(model_entry.registry.model, "Model0999");
        assert!(matches!(
            catalog.resolve_advertised_name("PEV-0999"),
            cutout_core::CatalogModelResolution::Matched(entry)
                if entry.registry.model == "Model0999"
        ));
    });
}

fn synthetic_catalog_entry(index: usize) -> ModelCatalogEntry {
    let hint = leak_static_str(format!("PEV-{index:04}"));
    let model = leak_static_str(format!("Model{index:04}"));
    let hints = Box::leak(Box::new([hint]));
    let registry = Box::leak(Box::new(ModelRegistryEntry {
        manufacturer: ManufacturerKey::new("Synthetic"),
        model: ModelKey::new(model),
        protocol_family: ProtocolFamily::VeteranLeaperkimNosfet,
        advertised_name_hints: hints,
        wire_model_id: None,
        battery: None,
        bms: None,
        gatt: &CATALOG_GATT,
        capabilities: cutout_core::Capabilities::from_supported_commands([
            CommandKind::RequestIdentity,
            CommandKind::RequestTelemetry,
        ]),
        verification: VerificationStatus::Inferred,
    }));

    ModelCatalogEntry {
        registry,
        registration: ModelRuntimeRegistration {
            parser: Some(ParserKey::new(leak_static_str(format!(
                "synthetic-parser-{index:04}"
            )))),
            session: Some(SessionKey::new(leak_static_str(format!(
                "synthetic-session-{index:04}"
            )))),
        },
    }
}

fn leak_static_str(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}
