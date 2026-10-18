//! Public connection-boundary coverage for the source-pinned Refloat settings.
//!
//! The config image is an independent Refloat 1.3.0 serializer output. VESC and
//! custom-app response frames below are assembled from the documented wire
//! layouts with an independent CRC-16/XMODEM implementation.

use cutout_core::{
    DeviceCommandDto, DeviceSettingValue, MonotonicMillisDto, MonotonicTimestamp, ProtocolFamily,
    SessionInputDto, SessionOutput, SettingCommandStatus, SettingId, SettingTransportStatus,
    TransportAction,
};
use cutout_protocols::{DeviceConnectionSession, DeviceDetectionEvent, VESC_NOTIFY_CHANNEL};

const MOTOR_DEFAULT: &[u8; 488] =
    include_bytes!("fixtures/vesc-settings/vesc-7.00-motor-default.bin");
const APP_DEFAULT: &[u8; 284] = include_bytes!("fixtures/vesc-settings/vesc-7.00-app-default.bin");

fn firmware_config_reply(command: u8, image: &[u8]) -> Vec<u8> {
    let mut payload = vec![command];
    payload.extend_from_slice(image);
    vesc_frame(&payload)
}

const VESC_IDENTITY_REPLY: &[u8] = &[
    2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0, 38,
    208, 3,
];

fn crc16_xmodem(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0_u16, |crc, byte| {
        let mut value = crc ^ (u16::from(*byte) << 8);
        for _ in 0..8 {
            value = if value & 0x8000 != 0 {
                (value << 1) ^ 0x1021
            } else {
                value << 1
            };
        }
        value
    })
}

fn vesc_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 6);
    if let Ok(length) = u8::try_from(payload.len()) {
        frame.extend_from_slice(&[2, length]);
    } else {
        frame.push(3);
        frame.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_be_bytes());
    }
    frame.extend_from_slice(payload);
    frame.extend_from_slice(&crc16_xmodem(payload).to_be_bytes());
    frame.push(3);
    frame
}

fn refloat_info(package_name: &str, major: u8, minor: u8, patch: u8) -> Vec<u8> {
    let mut app = vec![101, 0, 2, 0]; // interface, INFO command, version, flags
    let mut package = [0_u8; 20];
    package[..package_name.len()].copy_from_slice(package_name.as_bytes());
    app.extend_from_slice(&package);
    app.extend_from_slice(&[major, minor, patch]);
    app.extend_from_slice(&[0; 20]); // empty version suffix
    app.extend_from_slice(&0_u32.to_be_bytes()); // git hash
    app.extend_from_slice(&1_000_u32.to_be_bytes()); // tick rate
    app.extend_from_slice(&0_u32.to_be_bytes()); // capabilities
    app.push(0); // extra flags
    let mut payload = vec![36]; // COMM_CUSTOM_APP_DATA
    payload.append(&mut app);
    vesc_frame(&payload)
}

fn vesc_firmware_reply(major: u8, minor: u8) -> Vec<u8> {
    // Firmware-info reply; command and response shape are from VESC 7.0.
    let mut payload = vec![157, major, minor, 0];
    payload.extend_from_slice(b"abc123\0");
    payload.extend_from_slice(b"userhash\0");
    vesc_frame(&payload)
}

fn vesc_controller_7_values_reply() -> Vec<u8> {
    // Independently sourced selective-values reply with CONTROLLER_ID set to 7.
    let payload = vec![
        50, 0, 2, 161, 138, 0, 0, 0, 0, 0, 4, 0, 0, 3, 221, 1, 119, 255, 255, 170, 43, 0, 7,
    ];
    vesc_frame(&payload)
}

fn notification(
    owner: &mut DeviceConnectionSession,
    token: &cutout_core::ConnectionAttemptToken,
    bytes: Vec<u8>,
    at: u64,
) {
    if let Some(step) = owner.ingest(
        token,
        &SessionInputDto::Notification {
            channel: VESC_NOTIFY_CHANNEL.as_bytes(),
            bytes,
            monotonic_ms: MonotonicMillisDto { milliseconds: at },
        },
    ) {
        assert!(
            !has_config_write(&step),
            "startup and passive readback never mutate the custom config"
        );
        // This helper discards transport steps, so model the host submitting any
        // newly emitted standalone configuration read before delivering replies.
        for (frame, operation_id) in transport_writes(&step)
            .into_iter()
            .zip(transport_write_operation_ids(&step))
        {
            if frame_payload(&frame)
                .first()
                .is_some_and(|command| *command == 14 || *command == 17 || *command == 93)
                && let Some(operation_id) = operation_id.filter(|id| *id & (1 << 63) != 0)
            {
                assert!(owner.mark_setting_read_transport(
                    token,
                    operation_id,
                    SettingTransportStatus::Submitted,
                    MonotonicTimestamp::new(at),
                ));
            }
        }
    }
}

fn vesc_session_with_startup() -> (
    DeviceConnectionSession,
    cutout_core::ConnectionAttemptToken,
    cutout_protocols::DeviceConnectionStep,
) {
    let mut owner = DeviceConnectionSession::default();
    owner.begin_attempt(
        "Refloat 1.3.0 test controller".into(),
        MonotonicTimestamp::new(0),
    );
    let token = owner.snapshot().connection.token.unwrap();
    assert!(owner.session_state_mut().connection.connected(&token));
    owner
        .session_state_mut()
        .settings
        .require_managed_transport();
    owner
        .observe_for_attempt(
            &token,
            DeviceDetectionEvent::Notification {
                bytes: VESC_IDENTITY_REPLY,
            },
        )
        .expect("current VESC wire evidence is retained");
    owner.resolve(&token, false, MonotonicTimestamp::new(1));
    assert_eq!(
        owner
            .snapshot()
            .identity
            .as_ref()
            .map(|identity| identity.protocol),
        Some(ProtocolFamily::Vesc)
    );
    let startup = owner
        .ingest(
            &token,
            &SessionInputDto::LinkUp {
                monotonic_ms: MonotonicMillisDto { milliseconds: 2 },
                max_write_len: None,
            },
        )
        .expect("current VESC session starts on LinkUp");
    (owner, token, startup)
}

fn vesc_session() -> (DeviceConnectionSession, cutout_core::ConnectionAttemptToken) {
    let (owner, token, startup) = vesc_session_with_startup();
    assert!(
        transport_writes(&startup)
            .iter()
            .all(|frame| frame_payload(frame).first() != Some(&14)),
        "initial telemetry does not assume a firmware configuration schema"
    );
    (owner, token)
}

fn refloat_session(
    package: Option<(&str, u8, u8, u8)>,
    firmware: (u8, u8),
    config: Option<&[u8]>,
) -> (DeviceConnectionSession, cutout_core::ConnectionAttemptToken) {
    let (mut owner, token) = vesc_session();
    if let Some((name, major, minor, patch)) = package {
        notification(
            &mut owner,
            &token,
            refloat_info(name, major, minor, patch),
            10,
        );
    }
    notification(
        &mut owner,
        &token,
        vesc_firmware_reply(firmware.0, firmware.1),
        11,
    );
    notification(&mut owner, &token, vesc_controller_7_values_reply(), 12);
    if let Some(config) = config {
        let mut response = vec![93, 0];
        response.extend_from_slice(config);
        notification(&mut owner, &token, vesc_frame(&response), 13);
        // Firmware objects answer their own serialized read domains. Deliver the
        // independently generated defaults before admitting writes in these fixtures.
        for (command, image) in [(14, MOTOR_DEFAULT.as_slice()), (17, APP_DEFAULT.as_slice())] {
            notification(
                &mut owner,
                &token,
                firmware_config_reply(command, image),
                13,
            );
        }
    }
    (owner, token)
}

fn known_refloat_session() -> (DeviceConnectionSession, cutout_core::ConnectionAttemptToken) {
    let config = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    refloat_session(Some(("Refloat", 1, 3, 0)), (7, 0), Some(config))
}

fn refloat_custom_frame(command: u8, body: &[u8]) -> Vec<u8> {
    let mut payload = vec![36, 101, command]; // VESC custom app data, Refloat, command
    payload.extend_from_slice(body);
    vesc_frame(&payload)
}

fn refloat_telemetry(package_state: u8, speed_f16: u16) -> [Vec<u8>; 2] {
    refloat_telemetry_with_mode(package_state, speed_f16, 0, false)
}

fn refloat_telemetry_with_mode(
    package_state: u8,
    speed_f16: u16,
    package_mode: u8,
    darkride: bool,
) -> [Vec<u8>; 2] {
    // Refloat realtime ids: motor.speed and imu.pitch, with no runtime ids.
    let ids_body = [
        2, 11, b'm', b'o', b't', b'o', b'r', b'.', b's', b'p', b'e', b'e', b'd', 9, b'i', b'm',
        b'u', b'.', b'p', b'i', b't', b'c', b'h', 0,
    ];
    let ids = refloat_custom_frame(32, &ids_body);

    // Caller-selected package state and speed encoded as IEEE-754 binary16.
    let mut data_body = vec![0, 0]; // mask, extra flags
    data_body.extend_from_slice(&1_u32.to_be_bytes()); // package ticks
    let state_flags = (u32::from(package_state) << 24)
        | (u32::from(package_mode) << 28)
        | if darkride { 1 << 17 } else { 0 };
    data_body.extend_from_slice(&state_flags.to_be_bytes());
    data_body.extend_from_slice(&speed_f16.to_be_bytes()); // speed
    data_body.extend_from_slice(&0_u16.to_be_bytes()); // pitch
    [ids, refloat_custom_frame(31, &data_body)]
}

fn stationary_refloat_telemetry() -> [Vec<u8>; 2] {
    refloat_telemetry(2, 0)
}

fn frame_payload(frame: &[u8]) -> &[u8] {
    let (start, length) = match frame.first().copied() {
        Some(2) => (2, usize::from(frame[1])),
        Some(3) => (3, usize::from(u16::from_be_bytes([frame[1], frame[2]]))),
        _ => panic!("expected a VESC UART frame"),
    };
    &frame[start..start + length]
}

fn transport_writes(step: &cutout_protocols::DeviceConnectionStep) -> Vec<Vec<u8>> {
    step.result
        .outputs
        .iter()
        .filter_map(|output| {
            let SessionOutput::Transport(TransportAction::Write { bytes, .. }) = output else {
                return None;
            };
            Some(bytes.as_slice().to_vec())
        })
        .collect()
}

fn transport_write_operation_ids(
    step: &cutout_protocols::DeviceConnectionStep,
) -> Vec<Option<u64>> {
    step.result
        .outputs
        .iter()
        .filter_map(|output| {
            let SessionOutput::Transport(TransportAction::Write { bytes, .. }) = output else {
                return None;
            };
            Some(
                bytes
                    .operation_id()
                    .map(cutout_core::TransportOperationId::get),
            )
        })
        .collect()
}

fn has_config_write(step: &cutout_protocols::DeviceConnectionStep) -> bool {
    transport_writes(step)
        .iter()
        .any(|frame| frame_payload(frame).first() == Some(&95))
}

fn submit_pwm_tiltback(
    owner: &mut DeviceConnectionSession,
    token: &cutout_core::ConnectionAttemptToken,
    at: u64,
) -> cutout_protocols::DeviceConnectionStep {
    owner
        .submit_setting(
            token,
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(81),
            MonotonicTimestamp::new(at),
        )
        .expect("known Refloat setting request is handled")
}

#[test]
// Keep the complete per-kind receipt/readback procedure in one sequence for auditability.
#[allow(clippy::too_many_lines)]
fn firmware_configurations_keep_their_own_preflight_receipts_and_full_image_readback() {
    for (get, set, id, value, baseline, offset, replacement) in [
        (
            14,
            13,
            SettingId::BatterySeriesCellCount,
            16,
            MOTOR_DEFAULT.as_slice(),
            462,
            vec![16],
        ),
        (
            17,
            16,
            SettingId::ImuSampleRate,
            1_000,
            APP_DEFAULT.as_slice(),
            229,
            1_000_u16.to_be_bytes().to_vec(),
        ),
    ] {
        let (mut owner, token) = known_refloat_session();
        for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
            notification(&mut owner, &token, bytes, 20 + index as u64);
        }
        let accepted = owner
            .submit_setting(
                &token,
                id,
                DeviceSettingValue::Number(value),
                MonotonicTimestamp::new(22),
            )
            .unwrap();
        assert!(accepted.result.error.is_none());
        assert_eq!(frame_payload(&transport_writes(&accepted)[0]), &[get]);
        let request_id = owner
            .settings_snapshot()
            .settings
            .iter()
            .find(|setting| setting.id == id)
            .unwrap()
            .request_id
            .unwrap();
        assert!(owner.mark_setting_transport(
            &token,
            id,
            request_id,
            SettingTransportStatus::Submitted,
            MonotonicTimestamp::new(23)
        ));

        // A Refloat answer is a separate object and cannot authorize this write.
        let mut unrelated = vec![93, 0];
        unrelated.extend_from_slice(include_bytes!(
            "fixtures/refloat-settings/refloat-1.3.0-default.bin"
        ));
        let unrelated = owner
            .ingest(
                &token,
                &SessionInputDto::Notification {
                    channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                    bytes: vesc_frame(&unrelated),
                    monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
                },
            )
            .unwrap();
        assert_eq!(transport_writes(&unrelated), Vec::<Vec<u8>>::new());

        let prepared = owner
            .ingest(
                &token,
                &SessionInputDto::Notification {
                    channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                    bytes: firmware_config_reply(get, baseline),
                    monotonic_ms: MonotonicMillisDto { milliseconds: 25 },
                },
            )
            .unwrap();
        let writes = transport_writes(&prepared);
        assert_eq!(writes.len(), 2);
        let mut expected = baseline.to_vec();
        expected[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert_eq!(frame_payload(&writes[0]).first(), Some(&set));
        assert_eq!(&frame_payload(&writes[0])[1..], expected);
        assert_eq!(frame_payload(&writes[1]), &[get]);
        assert!(owner.mark_setting_transport(
            &token,
            id,
            request_id,
            SettingTransportStatus::Submitted,
            MonotonicTimestamp::new(26)
        ));
        // After mutation handoff, motion revokes future mutations, not the harmless
        // verification read that belongs to the already-submitted procedure.
        for bytes in refloat_telemetry(3, 0x3c00) {
            notification(&mut owner, &token, bytes, 26);
        }
        assert!(
            owner.setting_transport_is_current(&token, request_id, MonotonicTimestamp::new(27)),
            "a submitted mutation must still be allowed its read-only verification"
        );
        assert!(owner.mark_setting_transport(
            &token,
            id,
            request_id,
            SettingTransportStatus::Submitted,
            MonotonicTimestamp::new(27)
        ));
        notification(&mut owner, &token, vesc_frame(&[set]), 28);
        assert_eq!(
            owner
                .settings_snapshot()
                .settings
                .iter()
                .find(|setting| setting.id == id)
                .unwrap()
                .status,
            SettingCommandStatus::WaitingForConfirmation
        );
        notification(
            &mut owner,
            &token,
            firmware_config_reply(get, &expected),
            29,
        );
        assert_eq!(
            owner
                .settings_snapshot()
                .settings
                .iter()
                .find(|setting| setting.id == id)
                .unwrap()
                .status,
            SettingCommandStatus::Confirmed
        );
    }
}

#[test]
fn firmware_configuration_is_available_without_a_refloat_package() {
    let (mut owner, token) = refloat_session(None, (7, 0), None);
    notification(
        &mut owner,
        &token,
        firmware_config_reply(14, MOTOR_DEFAULT),
        13,
    );
    notification(
        &mut owner,
        &token,
        firmware_config_reply(17, APP_DEFAULT),
        13,
    );
    let descriptors = owner.settings_descriptors();
    assert_eq!(descriptors.len(), 75);
    assert!(
        descriptors
            .iter()
            .any(|descriptor| descriptor.id == SettingId::BatterySeriesCellCount)
    );
    assert!(
        descriptors
            .iter()
            .all(|descriptor| descriptor.id != SettingId::PwmTiltback)
    );
    assert!(
        descriptors
            .iter()
            .all(|descriptor| descriptor.access == cutout_protocols::SettingAccess::ReadOnly)
    );
    assert!(
        owner
            .submit_setting(
                &token,
                SettingId::BatterySeriesCellCount,
                DeviceSettingValue::Number(16),
                MonotonicTimestamp::new(14)
            )
            .is_err(),
        "configuration reads cannot supply missing parked/disengaged evidence"
    );
}

#[test]
fn newly_observed_configuration_change_revokes_a_queued_whole_image_mutation() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap()
        .request_id
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23)
    ));
    let original = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(original);
    let prepared = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert!(has_config_write(&prepared));
    let mut replacement = preflight;
    // This independent LED setting changed after preflight, before native handoff.
    replacement[2 + 180] ^= 1;
    notification(&mut owner, &token, vesc_frame(&replacement), 25);
    assert!(!owner.setting_transport_is_current(&token, request_id, MonotonicTimestamp::new(26)));
}

#[test]
fn complete_ordinary_catalog_retains_every_observation_across_bounded_readbacks() {
    let (owner, _) = known_refloat_session();
    assert_eq!(owner.settings_descriptors().len(), 166);
    let snapshot = owner.settings_snapshot();
    assert_eq!(snapshot.settings.len(), 166);
    assert!(
        snapshot
            .settings
            .iter()
            .all(|setting| setting.current.is_some()),
        "tail fields must not be truncated by the bounded settings response"
    );
}

#[test]
fn generic_vesc_without_refloat_package_exposes_no_refloat_controls() {
    let (owner, _) = refloat_session(None, (7, 0), None);
    assert_eq!(owner.settings_descriptors().len(), 0);
}

#[test]
fn explicit_settings_read_requests_only_config_without_mutating_it() {
    let (mut owner, token) = known_refloat_session();
    let step = owner
        .ingest(
            &token,
            &SessionInputDto::CommandAt {
                command: DeviceCommandDto::RequestSettings,
                monotonic_ms: MonotonicMillisDto { milliseconds: 14 },
            },
        )
        .expect("settings request is a supported read-only command");
    let writes = transport_writes(&step);
    assert_eq!(writes.len(), 1);
    assert_eq!(frame_payload(&writes[0]), [93, 0]);
    assert!(
        transport_write_operation_ids(&step)[0].is_some(),
        "a config read needs an identity so native rejection/cancellation can settle it"
    );
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Keeps bootstrapping, reply ownership and later guarded write admission in one lifecycle."
)]
fn automatic_motor_config_probe_is_owned_and_unlocks_later_supported_settings() {
    let (mut owner, token, startup) = vesc_session_with_startup();
    let firmware_info_reads = transport_writes(&startup)
        .into_iter()
        .filter(|frame| frame_payload(frame).first() == Some(&157))
        .collect::<Vec<_>>();
    assert_eq!(firmware_info_reads.len(), 1);
    assert_eq!(frame_payload(&firmware_info_reads[0]), &[157]);

    assert!(
        transport_writes(&startup)
            .iter()
            .all(|frame| frame_payload(frame).first() != Some(&14))
    );
    notification(&mut owner, &token, vesc_firmware_reply(7, 0), 2);
    let qualified = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_controller_7_values_reply(),
                monotonic_ms: MonotonicMillisDto { milliseconds: 3 },
            },
        )
        .expect("supported firmware and controller facts qualify configuration inventory");
    let startup_motor_reads = transport_writes(&qualified)
        .into_iter()
        .zip(transport_write_operation_ids(&qualified))
        .filter(|(frame, _)| frame_payload(frame).first() == Some(&14))
        .collect::<Vec<_>>();
    assert_eq!(startup_motor_reads.len(), 1);
    let (motor_read, motor_read_id) = &startup_motor_reads[0];
    let motor_read_id = motor_read_id.expect("qualified configuration reads are wrapper-owned");
    assert_eq!(frame_payload(motor_read), &[14]);
    assert_eq!(owner.settings_descriptors(), Vec::new());
    assert!(owner.mark_setting_read_transport(
        &token,
        motor_read_id,
        SettingTransportStatus::Queued,
        MonotonicTimestamp::new(3),
    ));

    let tick = owner
        .ingest(
            &token,
            &SessionInputDto::Tick {
                monotonic_ms: MonotonicMillisDto { milliseconds: 102 },
            },
        )
        .expect("connected VESC session accepts timer ticks");
    let duplicate_reads = transport_writes(&tick)
        .into_iter()
        .zip(transport_write_operation_ids(&tick))
        .filter(|(frame, _)| frame_payload(frame).first() == Some(&14))
        .collect::<Vec<_>>();
    assert!(
        duplicate_reads.is_empty(),
        "ticks cannot duplicate an owned read"
    );
    notification(&mut owner, &token, vesc_controller_7_values_reply(), 103);
    assert_eq!(
        owner
            .telemetry_snapshot()
            .and_then(|snapshot| snapshot.speed),
        None
    );
    notification(
        &mut owner,
        &token,
        firmware_config_reply(17, APP_DEFAULT),
        103,
    );
    assert!(
        owner.mark_setting_read_transport(
            &token,
            motor_read_id,
            SettingTransportStatus::Submitted,
            MonotonicTimestamp::new(103),
        ),
        "another config kind must not retire Motor read ownership"
    );

    notification(
        &mut owner,
        &token,
        firmware_config_reply(14, MOTOR_DEFAULT),
        104,
    );
    assert!(
        !owner.mark_setting_read_transport(
            &token,
            motor_read_id,
            SettingTransportStatus::Submitted,
            MonotonicTimestamp::new(104),
        ),
        "the matching Motor response retires its read receipt"
    );
    notification(&mut owner, &token, vesc_controller_7_values_reply(), 105);
    assert!(
        owner
            .telemetry_snapshot()
            .is_some_and(|snapshot| snapshot.speed.is_some())
    );
    // A repeated firmware observation cannot restart a completed configuration inventory.
    let firmware = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_firmware_reply(7, 0),
                monotonic_ms: MonotonicMillisDto { milliseconds: 106 },
            },
        )
        .expect("supported firmware facts complete the firmware identity");
    assert!(
        !transport_writes(&firmware)
            .iter()
            .any(|frame| frame_payload(frame).first() == Some(&17))
    );
    assert!(
        owner
            .settings_descriptors()
            .iter()
            .any(|descriptor| descriptor.id == SettingId::ImuSampleRate)
    );

    notification(&mut owner, &token, refloat_info("Refloat", 1, 3, 0), 107);
    let mut refloat_config = vec![93, 0];
    refloat_config.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    notification(&mut owner, &token, vesc_frame(&refloat_config), 108);

    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 109 + index as u64);
    }
    assert!(
        owner
            .settings_descriptors()
            .iter()
            .any(|descriptor| descriptor.id == SettingId::BatterySeriesCellCount)
    );

    let request = owner
        .submit_setting(
            &token,
            SettingId::BatterySeriesCellCount,
            DeviceSettingValue::Number(17),
            MonotonicTimestamp::new(111),
        )
        .expect("supported motor-image setting is admitted after stationary evidence");
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::BatterySeriesCellCount)
        .and_then(|setting| setting.request_id)
        .expect("write procedure receives an operation identity");
    assert!(
        transport_writes(&request)
            .iter()
            .any(|frame| frame_payload(frame) == [14])
    );
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::BatterySeriesCellCount,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(112),
    ));
    let prepared = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: firmware_config_reply(14, MOTOR_DEFAULT),
                monotonic_ms: MonotonicMillisDto { milliseconds: 113 },
            },
        )
        .expect("the owned preflight read returns the complete motor image");
    assert!(
        transport_writes(&prepared)
            .iter()
            .any(|frame| frame_payload(frame).first() == Some(&13))
    );
}

#[test]
fn unsupported_firmware_geometry_remains_read_only_telemetry_evidence() {
    let (mut owner, token, startup) = vesc_session_with_startup();
    assert!(
        transport_writes(&startup)
            .iter()
            .all(|frame| frame_payload(frame).first() != Some(&14))
    );

    notification(&mut owner, &token, vesc_firmware_reply(6, 0), 4);
    notification(
        &mut owner,
        &token,
        firmware_config_reply(14, MOTOR_DEFAULT),
        5,
    );
    notification(&mut owner, &token, vesc_controller_7_values_reply(), 6);

    assert_eq!(owner.settings_descriptors(), Vec::new());
    assert!(
        owner
            .telemetry_snapshot()
            .is_some_and(|snapshot| snapshot.speed.is_some())
    );
}

#[test]
fn initial_config_read_cancellation_releases_slot_but_submitted_read_waits_for_response() {
    let (mut owner, token) = vesc_session();
    notification(&mut owner, &token, refloat_info("Refloat", 1, 3, 0), 10);
    notification(&mut owner, &token, vesc_firmware_reply(7, 0), 11);
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 12 + index as u64);
    }
    let initial = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_controller_7_values_reply(),
                monotonic_ms: MonotonicMillisDto { milliseconds: 14 },
            },
        )
        .unwrap();
    let initial_id = transport_write_operation_ids(&initial)
        .into_iter()
        .flatten()
        .next()
        .expect("initial config reads carry Rust-owned operation identity");
    assert!(owner.mark_setting_read_transport(
        &token,
        initial_id,
        SettingTransportStatus::Cancelled,
        MonotonicTimestamp::new(15),
    ));

    let retry = owner
        .ingest(
            &token,
            &SessionInputDto::CommandAt {
                command: DeviceCommandDto::RequestSettings,
                monotonic_ms: MonotonicMillisDto { milliseconds: 16 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&retry).len(), 1);
    let retry_id = transport_write_operation_ids(&retry)[0]
        .expect("an explicit retry receives a new correlation identity");
    assert_ne!(initial_id, retry_id);
    assert!(
        owner.setting_read_transport_is_current(&token, retry_id, MonotonicTimestamp::new(16),)
    );
    assert!(owner.mark_setting_read_transport(
        &token,
        retry_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(17),
    ));
    let blocked = owner
        .ingest(
            &token,
            &SessionInputDto::CommandAt {
                command: DeviceCommandDto::RequestSettings,
                monotonic_ms: MonotonicMillisDto { milliseconds: 18 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&blocked), Vec::<Vec<u8>>::new());
}

#[test]
// Keep timeout, slot-retention, and reconnect assertions together as one lifecycle regression.
#[allow(clippy::too_many_lines)]
fn submitted_procedure_read_timeout_keeps_config_slot_until_reply() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));

    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let queued = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert!(has_config_write(&queued));
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(26),
    ));

    let expired = owner
        .ingest(
            &token,
            &SessionInputDto::Tick {
                monotonic_ms: MonotonicMillisDto {
                    milliseconds: 2_023,
                },
            },
        )
        .unwrap();
    assert!(transport_writes(&expired).iter().all(|bytes| {
        frame_payload(bytes)
            .first()
            .is_none_or(|command| *command != 93)
    }));
    let blocked = owner
        .ingest(
            &token,
            &SessionInputDto::CommandAt {
                command: DeviceCommandDto::RequestSettings,
                monotonic_ms: MonotonicMillisDto {
                    milliseconds: 2_024,
                },
            },
        )
        .unwrap();
    assert!(transport_writes(&blocked).iter().all(|bytes| {
        frame_payload(bytes)
            .first()
            .is_none_or(|command| *command != 93)
    }));
    assert_eq!(
        owner.submit_setting(
            &token,
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(81),
            MonotonicTimestamp::new(2_024),
        ),
        Err(cutout_protocols::DeviceSettingRequestError::ConfigurationReadRequiresReconnect)
    );

    notification(&mut owner, &token, vesc_frame(&preflight), 2_025);
    let allowed = owner
        .ingest(
            &token,
            &SessionInputDto::CommandAt {
                command: DeviceCommandDto::RequestSettings,
                monotonic_ms: MonotonicMillisDto {
                    milliseconds: 2_026,
                },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&allowed).len(), 1);
}

#[test]
fn queued_initial_config_read_is_invalidated_by_controller_replacement() {
    let (mut owner, token) = vesc_session();
    notification(&mut owner, &token, refloat_info("Refloat", 1, 3, 0), 10);
    notification(&mut owner, &token, vesc_firmware_reply(7, 0), 11);
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 12 + index as u64);
    }
    let initial = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_controller_7_values_reply(),
                monotonic_ms: MonotonicMillisDto { milliseconds: 14 },
            },
        )
        .unwrap();
    let operation_id = transport_write_operation_ids(&initial)[0].unwrap();
    let mut replacement = frame_payload(&vesc_controller_7_values_reply()).to_vec();
    *replacement.last_mut().unwrap() = 8;
    notification(&mut owner, &token, vesc_frame(&replacement), 15);
    assert!(!owner.setting_read_transport_is_current(
        &token,
        operation_id,
        MonotonicTimestamp::new(16),
    ));
}

#[test]
fn rider_feel_tune_fields_are_not_exposed_as_individual_configuration_settings() {
    let (mut owner, token) = known_refloat_session();
    let descriptors = owner.settings_descriptors();
    assert!(
        !descriptors
            .iter()
            .any(|descriptor| descriptor.id == SettingId::LateralTiltLimit)
    );
    assert!(
        owner
            .submit_setting(
                &token,
                SettingId::LateralTiltLimit,
                DeviceSettingValue::Number(60),
                MonotonicTimestamp::new(14),
            )
            .is_err()
    );
}

#[test]
fn replacement_application_info_retires_refloat_identity_and_controls() {
    let (mut owner, token) = known_refloat_session();
    assert!(
        owner
            .settings_descriptors()
            .iter()
            .any(|descriptor| descriptor.id == SettingId::PwmTiltback)
    );
    assert_eq!(
        owner.snapshot().identity.unwrap().vesc_application,
        Some(cutout_protocols::VescApplicationDiscovery::Refloat)
    );
    notification(&mut owner, &token, refloat_info("Other", 1, 3, 0), 20);
    assert_eq!(
        owner.snapshot().identity.unwrap().vesc_application,
        Some(cutout_protocols::VescApplicationDiscovery::Absent)
    );
    assert_eq!(
        owner.snapshot().identity.unwrap().vehicle_kind(),
        cutout_core::VehicleKind::Unknown
    );
    assert_eq!(
        owner.settings_descriptors(),
        Vec::new(),
        "a contradictory application image invalidates this connection's settings authority"
    );
    assert!(
        owner
            .submit_setting(
                &token,
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(90),
                MonotonicTimestamp::new(21)
            )
            .is_err()
    );
}

#[test]
fn unsupported_package_does_not_hide_independently_validated_firmware_configuration() {
    let config = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    for (package, firmware) in [
        (Some(("Other", 1, 3, 0)), (7, 0)),
        (Some(("Refloat", 1, 2, 3)), (7, 0)),
        (Some(("Refloat", 1, 3, 0)), (7, 1)),
    ] {
        let (owner, _) = refloat_session(package, firmware, Some(config));
        let descriptors = owner.settings_descriptors();
        assert_eq!(descriptors.len(), if firmware == (7, 0) { 75 } else { 0 });
        assert!(
            !descriptors
                .iter()
                .any(|descriptor| descriptor.id == SettingId::PwmTiltback)
        );
    }
}

#[test]
fn wrong_refloat_schema_does_not_expose_package_controls() {
    let config = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    let mut wrong_signature = config.to_vec();
    wrong_signature[0] ^= 1;
    let truncated = &config[..config.len() - 1];
    for invalid_config in [wrong_signature.as_slice(), truncated] {
        let (owner, _) = refloat_session(Some(("Refloat", 1, 3, 0)), (7, 0), Some(invalid_config));
        let descriptors = owner.settings_descriptors();
        assert_eq!(descriptors.len(), 75);
        assert!(
            !descriptors
                .iter()
                .any(|descriptor| descriptor.id == SettingId::PwmTiltback)
        );
    }
}

#[test]
fn parked_setting_write_requires_preflight_and_matching_fresh_readback() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let accepted = submit_pwm_tiltback(&mut owner, &token, 22);
    let first = transport_writes(&accepted);
    assert_eq!(first.len(), 1);
    assert_eq!(frame_payload(&first[0]), [93, 0]);

    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .expect("accepted request owns a transport id");
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));

    let baseline = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(baseline);
    let step = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .expect("verified connection accepts the preflight response");
    let writes = transport_writes(&step);
    assert_eq!(writes.len(), 2);
    let write_payload = frame_payload(&writes[0]);
    assert_eq!(&write_payload[..2], &[95, 0]);
    let mut expected = baseline.to_vec();
    expected[68..70].copy_from_slice(&810_i16.to_be_bytes());
    assert_eq!(&write_payload[2..], expected.as_slice());
    assert_eq!(frame_payload(&writes[1]), [93, 0]);

    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    let acknowledged = vec![95];
    let ack = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&acknowledged),
                monotonic_ms: MonotonicMillisDto { milliseconds: 26 },
            },
        )
        .expect("verified connection accepts firmware dispatch ACK");
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.status, SettingCommandStatus::WaitingForConfirmation);
    assert_eq!(transport_writes(&ack).len(), 0);

    let default_marker = expected.len() - 1;
    expected[default_marker] = 0; // firmware clears the source-defined final marker on save
    let mut readback = vec![93, 0];
    readback.extend_from_slice(&expected);
    let _ = owner.ingest(
        &token,
        &SessionInputDto::Notification {
            channel: VESC_NOTIFY_CHANNEL.as_bytes(),
            bytes: vesc_frame(&readback),
            monotonic_ms: MonotonicMillisDto { milliseconds: 27 },
        },
    );
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.status, SettingCommandStatus::Confirmed);
}

#[test]
fn outstanding_config_refresh_blocks_mutation_and_missing_reads_are_not_retried() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let read = owner
        .ingest(
            &token,
            &SessionInputDto::CommandAt {
                command: DeviceCommandDto::RequestSettings,
                monotonic_ms: MonotonicMillisDto { milliseconds: 22 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&read).len(), 1);
    let read_id = transport_write_operation_ids(&read)[0].unwrap();
    assert!(owner.mark_setting_read_transport(
        &token,
        read_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    assert_eq!(
        owner.submit_setting(
            &token,
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(81),
            MonotonicTimestamp::new(24)
        ),
        Err(cutout_protocols::DeviceSettingRequestError::Busy)
    );
    assert_eq!(
        owner.submit_setting(
            &token,
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(81),
            MonotonicTimestamp::new(2_022),
        ),
        Err(cutout_protocols::DeviceSettingRequestError::Busy),
        "a recently submitted read remains ordinary busy"
    );
    assert_eq!(
        owner.submit_setting(
            &token,
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(81),
            MonotonicTimestamp::new(2_023),
        ),
        Err(cutout_protocols::DeviceSettingRequestError::ConfigurationReadRequiresReconnect),
        "a timed-out read explains that reconnection is required"
    );

    let (mut silent, token) = refloat_session(Some(("Refloat", 1, 3, 0)), (7, 0), None);
    for at in [3_000, 5_000, 10_000] {
        let step = silent
            .ingest(
                &token,
                &SessionInputDto::Tick {
                    monotonic_ms: MonotonicMillisDto { milliseconds: at },
                },
            )
            .unwrap();
        assert!(
            transport_writes(&step)
                .iter()
                .all(|bytes| frame_payload(bytes).first() != Some(&93)),
            "a missing reply must not leave multiple indistinguishable config reads outstanding"
        );
    }
}

#[test]
fn same_link_controller_replacement_invalidates_settings_until_reconnect() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let frame = vesc_controller_7_values_reply();
    let mut payload = frame_payload(&frame).to_vec();
    *payload.last_mut().unwrap() = 8;
    notification(&mut owner, &token, vesc_frame(&payload), 22);
    notification(&mut owner, &token, refloat_info("Refloat", 1, 3, 0), 23);
    let mut response = vec![93, 0];
    response.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    notification(&mut owner, &token, vesc_frame(&response), 24);
    assert!(
        owner.settings_descriptors().is_empty(),
        "a new controller cannot inherit old controller settings/telemetry authorization"
    );
}

#[test]
fn matching_config_before_all_host_receipts_does_not_retire_queued_writes() {
    for submitted_stages in 0..=1 {
        let (mut owner, token) = known_refloat_session();
        for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
            notification(&mut owner, &token, bytes, 20 + index as u64);
        }
        submit_pwm_tiltback(&mut owner, &token, 22);
        let request_id = owner
            .settings_snapshot()
            .settings
            .into_iter()
            .find(|setting| setting.id == SettingId::PwmTiltback)
            .and_then(|setting| setting.request_id)
            .unwrap();
        assert!(owner.mark_setting_transport(
            &token,
            SettingId::PwmTiltback,
            request_id,
            SettingTransportStatus::Submitted,
            MonotonicTimestamp::new(23)
        ));
        let baseline = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
        let mut response = vec![93, 0];
        response.extend_from_slice(baseline);
        let step = owner
            .ingest(
                &token,
                &SessionInputDto::Notification {
                    channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                    bytes: vesc_frame(&response),
                    monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
                },
            )
            .unwrap();
        assert_eq!(transport_writes(&step).len(), 2);
        for _ in 0..submitted_stages {
            assert!(owner.mark_setting_transport(
                &token,
                SettingId::PwmTiltback,
                request_id,
                SettingTransportStatus::Submitted,
                MonotonicTimestamp::new(25)
            ));
        }
        notification(&mut owner, &token, vesc_frame(&response), 26);
        assert!(
            owner.setting_transport_is_current(&token, request_id, MonotonicTimestamp::new(27)),
            "a fresh image that still matches the mutation baseline must not cancel queued stages"
        );
        for _ in submitted_stages..2 {
            assert!(owner.mark_setting_transport(
                &token,
                SettingId::PwmTiltback,
                request_id,
                SettingTransportStatus::Submitted,
                MonotonicTimestamp::new(28)
            ));
        }
        response[70..72].copy_from_slice(&810_i16.to_be_bytes());
        notification(&mut owner, &token, vesc_frame(&response), 29);
        let setting = owner
            .settings_snapshot()
            .settings
            .into_iter()
            .find(|setting| setting.id == SettingId::PwmTiltback)
            .unwrap();
        assert_eq!(setting.status, SettingCommandStatus::Confirmed);
    }
}

#[test]
fn same_millisecond_preflight_readback_is_accepted_after_its_host_receipt() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(24),
    ));
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let step = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert!(
        has_config_write(&step),
        "the authorized mutation must proceed"
    );
}

#[test]
fn same_timestamp_config_generation_accepts_new_identical_reply_once() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let baseline = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    submit_pwm_tiltback(&mut owner, &token, 24);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(24),
    ));

    notification(
        &mut owner,
        &token,
        stationary_refloat_telemetry()[0].clone(),
        24,
    );
    let mut fresh_config = vec![93, 0];
    fresh_config.extend_from_slice(baseline);
    let preflight = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&fresh_config),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert!(has_config_write(&preflight));
}

#[test]
fn failed_final_read_receipt_preserves_submitted_mutation_and_fails_procedure() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let mutation_and_read = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&mutation_and_read).len(), 2);
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Rejected,
        MonotonicTimestamp::new(26),
    ));
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.transport, Some(SettingTransportStatus::Submitted));
    assert_eq!(setting.status, SettingCommandStatus::Failed);
}

#[test]
fn tick_cancellation_after_mutation_preserves_submission() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let mutation_and_read = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&mutation_and_read).len(), 2);
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    let _ = owner.tick_setting_transport(&token, MonotonicTimestamp::new(2_023));
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.transport, Some(SettingTransportStatus::Submitted));
    assert_eq!(setting.status, SettingCommandStatus::Failed);
}

#[test]
fn protocol_tick_expiry_after_mutation_preserves_submission() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let next = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&next).len(), 2);
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    let _ = owner
        .ingest(
            &token,
            &SessionInputDto::Tick {
                monotonic_ms: MonotonicMillisDto {
                    milliseconds: 2_023,
                },
            },
        )
        .unwrap();
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.transport, Some(SettingTransportStatus::Submitted));
    assert_eq!(setting.status, SettingCommandStatus::Failed);
}

#[test]
fn controller_replacement_after_mutation_preserves_submission() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let next = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&next).len(), 2);
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    let mut replacement = frame_payload(&vesc_controller_7_values_reply()).to_vec();
    *replacement.last_mut().unwrap() = 8;
    notification(&mut owner, &token, vesc_frame(&replacement), 26);
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.transport, Some(SettingTransportStatus::Submitted));
    assert_eq!(setting.status, SettingCommandStatus::Failed);
}

#[test]
fn queued_verification_read_wakeup_uses_operation_deadline_after_arm_expires() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let next = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&next).len(), 2);
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    assert_eq!(
        owner.next_wakeup_at(&token, false),
        Some(MonotonicTimestamp::new(2_022)),
        "a queued read should wake at operation expiry, not at the expired stationary arm"
    );
    assert_eq!(
        owner.next_wakeup_at(&token, false),
        Some(MonotonicTimestamp::new(2_022)),
        "repeated backpressure polls must not expose a past deadline"
    );
    assert!(owner.tick_setting_transport(&token, MonotonicTimestamp::new(2_022)));
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.transport, Some(SettingTransportStatus::Submitted));
    assert_eq!(setting.status, SettingCommandStatus::Failed);
    assert_ne!(
        owner.next_wakeup_at(&token, false),
        Some(MonotonicTimestamp::new(2_022)),
        "the exact expiry tick must retire the queued verification write"
    );
}

#[test]
fn complete_submission_survives_100ms_polling_until_readback() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .unwrap();
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let mutation_and_read = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .unwrap();
    assert_eq!(transport_writes(&mutation_and_read).len(), 2);
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(26),
    ));
    owner.tick_setting_transport(&token, MonotonicTimestamp::new(126));
    let mut expected =
        include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin").to_vec();
    expected[68..70].copy_from_slice(&810_i16.to_be_bytes());
    let default_marker = expected.len() - 1;
    expected[default_marker] = 0;
    let mut readback = vec![93, 0];
    readback.extend_from_slice(&expected);
    notification(&mut owner, &token, vesc_frame(&readback), 127);
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.status, SettingCommandStatus::Confirmed);
}

#[test]
fn non_normal_modes_refuse_settings_admission() {
    for (name, package_mode, darkride) in [
        ("Handtest", 1, false),
        ("Flywheel", 2, false),
        ("Darkride", 0, true),
    ] {
        let (mut owner, token) = known_refloat_session();
        for (index, bytes) in refloat_telemetry_with_mode(2, 0, package_mode, darkride)
            .into_iter()
            .enumerate()
        {
            notification(&mut owner, &token, bytes, 20 + index as u64);
        }
        let rejected = submit_pwm_tiltback(&mut owner, &token, 22);
        assert!(
            transport_writes(&rejected).is_empty(),
            "{name} mode must not submit even a settings preflight read"
        );
    }
}

#[test]
fn mode_change_after_preflight_submission_cancels_before_config_write() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let _ = submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .expect("accepted request owns a transport id");
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    for (index, bytes) in refloat_telemetry_with_mode(2, 0, 1, false)
        .into_iter()
        .enumerate()
    {
        notification(&mut owner, &token, bytes, 24 + index as u64);
    }

    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let step = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 26 },
            },
        )
        .expect("verified connection accepts the preflight response");
    assert!(!has_config_write(&step));
    assert_eq!(
        owner
            .settings_snapshot()
            .settings
            .into_iter()
            .find(|setting| setting.id == SettingId::PwmTiltback)
            .unwrap()
            .status,
        SettingCommandStatus::Failed
    );
}

#[test]
fn whole_config_mismatch_fails_even_when_requested_field_matches() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let _ = submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .expect("accepted request owns a transport id");
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));
    let baseline = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(baseline);
    let _ = owner.ingest(
        &token,
        &SessionInputDto::Notification {
            channel: VESC_NOTIFY_CHANNEL.as_bytes(),
            bytes: vesc_frame(&preflight),
            monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
        },
    );
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));

    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(25),
    ));
    let mut mismatched = baseline.to_vec();
    mismatched[68..70].copy_from_slice(&810_i16.to_be_bytes());
    mismatched[100] ^= 1;
    let default_marker = mismatched.len() - 1;
    mismatched[default_marker] = 0;
    let mut readback = vec![93, 0];
    readback.extend_from_slice(&mismatched);
    let _ = owner.ingest(
        &token,
        &SessionInputDto::Notification {
            channel: VESC_NOTIFY_CHANNEL.as_bytes(),
            bytes: vesc_frame(&readback),
            monotonic_ms: MonotonicMillisDto { milliseconds: 26 },
        },
    );
    let setting = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .unwrap();
    assert_eq!(setting.status, SettingCommandStatus::Failed);
    assert_eq!(
        setting.current.map(|current| current.value),
        Some(DeviceSettingValue::Number(81))
    );
}

#[test]
fn preflight_transport_rejection_cancels_delayed_config_write() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let _ = submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .expect("accepted request owns a transport id");
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Rejected,
        MonotonicTimestamp::new(23),
    ));

    let mut preflight = vec![93, 0];
    preflight.extend_from_slice(include_bytes!(
        "fixtures/refloat-settings/refloat-1.3.0-default.bin"
    ));
    let step = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&preflight),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .expect("verified connection accepts the preflight response");
    assert!(!has_config_write(&step));
    assert_eq!(
        owner
            .settings_snapshot()
            .settings
            .into_iter()
            .find(|setting| setting.id == SettingId::PwmTiltback)
            .unwrap()
            .status,
        SettingCommandStatus::Failed
    );
}

#[test]
fn moving_unknown_or_stale_operating_evidence_never_emits_a_config_write() {
    let (mut unknown, unknown_token) = known_refloat_session();
    assert!(!has_config_write(&submit_pwm_tiltback(
        &mut unknown,
        &unknown_token,
        20
    )));

    let (mut moving, moving_token) = known_refloat_session();
    for (index, bytes) in refloat_telemetry(3, 0x3c00).into_iter().enumerate() {
        notification(&mut moving, &moving_token, bytes, 20 + index as u64);
    }
    assert!(!has_config_write(&submit_pwm_tiltback(
        &mut moving,
        &moving_token,
        22
    )));

    let (mut stale, stale_token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut stale, &stale_token, bytes, 20 + index as u64);
    }
    assert!(!has_config_write(&submit_pwm_tiltback(
        &mut stale,
        &stale_token,
        2_022,
    )));
}

#[test]
fn changed_preflight_baseline_cancels_without_sending_a_config_write() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let _ = submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .expect("accepted request owns a transport id");
    assert!(owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(23),
    ));

    let mut changed =
        include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin").to_vec();
    changed[100] ^= 1;
    let mut response = vec![93, 0];
    response.extend_from_slice(&changed);
    let step = owner
        .ingest(
            &token,
            &SessionInputDto::Notification {
                channel: VESC_NOTIFY_CHANNEL.as_bytes(),
                bytes: vesc_frame(&response),
                monotonic_ms: MonotonicMillisDto { milliseconds: 24 },
            },
        )
        .expect("verified connection accepts changed preflight config");
    assert!(!has_config_write(&step));
}

#[test]
fn late_setting_receipt_from_replaced_connection_is_rejected() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let _ = submit_pwm_tiltback(&mut owner, &token, 22);
    let request_id = owner
        .settings_snapshot()
        .settings
        .into_iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .and_then(|setting| setting.request_id)
        .expect("accepted request owns a transport id");
    owner.begin_attempt("replacement controller".into(), MonotonicTimestamp::new(23));
    assert!(!owner.mark_setting_transport(
        &token,
        SettingId::PwmTiltback,
        request_id,
        SettingTransportStatus::Submitted,
        MonotonicTimestamp::new(24),
    ));
}

#[test]
fn exact_schemas_expose_the_complete_ordinary_configuration_catalog() {
    let (owner, _) = known_refloat_session();
    let descriptors = owner.settings_descriptors();
    let ids: std::collections::BTreeSet<_> =
        descriptors.iter().map(|descriptor| descriptor.id).collect();
    assert!(ids.contains(&SettingId::PwmTiltback));
    assert!(ids.contains(&SettingId::TiltbackSpeed));
    assert_eq!(ids.len(), 166);
    assert_eq!(descriptors.len(), 166);

    let snapshot = owner.settings_snapshot();
    for id in ids {
        assert!(snapshot.settings.iter().any(|setting| setting.id == id));
    }
    let pwm_tiltback = snapshot
        .settings
        .iter()
        .find(|setting| setting.id == SettingId::PwmTiltback)
        .expect("source-backed PWM pushback setting is present");
    assert_eq!(
        pwm_tiltback.current.map(|current| current.value),
        Some(DeviceSettingValue::Number(80))
    );
    let speed = snapshot
        .settings
        .iter()
        .find(|setting| setting.id == SettingId::TiltbackSpeed)
        .expect("source-backed speed threshold setting is present");
    assert_eq!(
        speed.current.map(|current| current.value),
        Some(DeviceSettingValue::Disabled)
    );
}

#[test]
fn oversized_config_response_invalidates_previously_supported_catalog() {
    let (mut owner, token) = known_refloat_session();
    let mut response = vec![93, 0];
    response.extend_from_slice(&[0; 283]);
    notification(&mut owner, &token, vesc_frame(&response), 20);
    assert_eq!(owner.settings_descriptors().len(), 75);
    assert!(
        !owner
            .settings_descriptors()
            .iter()
            .any(|descriptor| descriptor.id == SettingId::PwmTiltback)
    );
}
