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
    if payload.len() <= usize::from(u8::MAX) {
        frame.extend_from_slice(&[2, u8::try_from(payload.len()).unwrap()]);
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
    }
}

fn vesc_session() -> (DeviceConnectionSession, cutout_core::ConnectionAttemptToken) {
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
    let _ = owner.ingest(
        &token,
        &SessionInputDto::LinkUp {
            monotonic_ms: MonotonicMillisDto { milliseconds: 2 },
            max_write_len: None,
        },
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
fn generic_vesc_without_refloat_package_exposes_no_refloat_controls() {
    let (owner, _) = refloat_session(None, (7, 0), None);
    assert!(owner.settings_descriptors().is_empty());
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
fn unsupported_package_or_firmware_version_exposes_no_controls() {
    let config = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    for (package, firmware) in [
        (Some(("Other", 1, 3, 0)), (7, 0)),
        (Some(("Refloat", 1, 2, 3)), (7, 0)),
        (Some(("Refloat", 1, 3, 0)), (7, 1)),
    ] {
        let (owner, _) = refloat_session(package, firmware, Some(config));
        assert!(owner.settings_descriptors().is_empty());
    }
}

#[test]
fn wrong_refloat_schema_signature_or_length_exposes_no_controls() {
    let config = include_bytes!("fixtures/refloat-settings/refloat-1.3.0-default.bin");
    let mut wrong_signature = config.to_vec();
    wrong_signature[0] ^= 1;
    let truncated = &config[..config.len() - 1];
    for invalid_config in [wrong_signature.as_slice(), truncated] {
        let (owner, _) = refloat_session(Some(("Refloat", 1, 3, 0)), (7, 0), Some(invalid_config));
        assert!(owner.settings_descriptors().is_empty());
    }
}

#[test]
fn parked_setting_write_requires_preflight_and_matching_fresh_readback() {
    let (mut owner, token) = known_refloat_session();
    for (index, bytes) in stationary_refloat_telemetry().into_iter().enumerate() {
        notification(&mut owner, &token, bytes, 20 + index as u64);
    }
    let accepted = owner
        .submit_setting(
            &token,
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(81),
            MonotonicTimestamp::new(22),
        )
        .expect("supported semantic request reaches the session");
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
    assert!(transport_writes(&ack).is_empty());

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
    assert_eq!(
        owner.submit_setting(
            &token,
            SettingId::PwmTiltback,
            DeviceSettingValue::Number(81),
            MonotonicTimestamp::new(23)
        ),
        Err(cutout_protocols::DeviceSettingRequestError::Busy)
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
        response[70..72].copy_from_slice(&810_i16.to_be_bytes());
        notification(&mut owner, &token, vesc_frame(&response), 26);
        assert!(
            owner.setting_transport_is_current(&token, request_id, MonotonicTimestamp::new(27)),
            "an unsolicited matching image must not cancel queued mutation/read stages"
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
fn exact_refloat_package_and_schema_expose_two_non_tune_controls() {
    let (owner, _) = known_refloat_session();
    let descriptors = owner.settings_descriptors();
    let ids: std::collections::BTreeSet<_> =
        descriptors.iter().map(|descriptor| descriptor.id).collect();
    assert_eq!(
        ids,
        std::collections::BTreeSet::from([SettingId::PwmTiltback, SettingId::TiltbackSpeed,])
    );
    assert_eq!(descriptors.len(), 2);

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
    assert!(owner.settings_descriptors().is_empty());
}
