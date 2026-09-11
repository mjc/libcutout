use super::*;
use crate::{MobileMelkLightingError, MobileMelkLightingRestoreStateDto};

const ID: &str = "11111111-1111-1111-1111-111111111111";

fn initializing_core() -> std::sync::Arc<MobileMelkLightingSessionCore> {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21  6A".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::Connected {
        name: Some("MELK-OC21  6A".into()),
        platform_identifier: ID.into(),
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::ServicesDiscovered {
        service_uuids: vec![cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid().into()],
        error: None,
    });
    core.drain_actions();
    core.handle(
        MobileMelkLightingSessionEventDto::CharacteristicsDiscovered {
            name: Some("MELK-OC21  6A".into()),
            service_uuid: cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid().into(),
            characteristics: vec![
                MobileMelkLightingCharacteristicEvidenceDto {
                    uuid: cutout_protocols::MELK_WRITE_CHANNEL.as_uuid().into(),
                    write_without_response: true,
                    notify_or_indicate: false,
                },
                MobileMelkLightingCharacteristicEvidenceDto {
                    uuid: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
                    write_without_response: false,
                    notify_or_indicate: true,
                },
            ],
            error: None,
        },
    );
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::NotificationState {
        characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
        ready: true,
        can_send: true,
        error: None,
    });
    core.drain_actions();
    core
}

fn ready_core() -> std::sync::Arc<MobileMelkLightingSessionCore> {
    let core = initializing_core();
    core.handle(MobileMelkLightingSessionEventDto::TimerFired {
        timer: MobileMelkLightingTimerDto::Initialization,
        can_send: true,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::TimerFired {
        timer: MobileMelkLightingTimerDto::Initialization,
        can_send: true,
    });
    core.drain_actions();
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Ready
    );
    core
}

#[test]
fn reducer_owns_gatt_initialization_and_ready_gate() {
    let core = ready_core();
    let snapshot = core.snapshot();
    assert_eq!(snapshot.platform_identifier.as_deref(), Some(ID));
    assert!(snapshot.notification_ready);
}

#[test]
fn reducer_coalesces_color_preview_writes_and_waits_for_capacity() {
    let core = ready_core();
    assert!(core.set_solid_color(255, 0, 0));
    assert!(core.set_solid_color(0, 255, 0));
    assert!(core.drain_actions().is_empty());
    core.flush_writes(true);
    let actions = core.drain_actions();
    assert_eq!(actions.len(), 2);
    let MobileMelkLightingSessionActionDto::Write { write, .. } = &actions[0] else {
        panic!("expected a color write")
    };
    assert_eq!(write.payload, [0x7e, 0, 5, 3, 0, 255, 0, 0, 0xef]);
    assert!(matches!(
        actions[1],
        MobileMelkLightingSessionActionDto::ArmTimer {
            timer: MobileMelkLightingTimerDto::WriteDrain,
            ..
        }
    ));
}

#[test]
fn reducer_rejects_malformed_remembered_identity_before_scanning() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some("not-a-uuid".into()));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Failed {
            reason: "Remembered lighting identity is invalid".into()
        }
    );
    assert!(core.drain_actions().is_empty());
}

#[test]
fn restarting_discards_output_from_the_previous_session() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(None);
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21  6A".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    core.handle(MobileMelkLightingSessionEventDto::Notification {
        characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
        bytes: vec![1],
    });

    core.stop();
    core.start(None);

    assert!(core.drain_actions().is_empty());
    assert!(core.drain_records().is_empty());
    assert!(core.drain_candidates().is_empty());
    assert!(core.drain_notifications().is_empty());
}

#[test]
fn connection_attempt_timeout_cancels_and_returns_to_scanning() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21  6A".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    core.drain_actions();

    core.handle(MobileMelkLightingSessionEventDto::TimerFired {
        timer: MobileMelkLightingTimerDto::ConnectionAttempt,
        can_send: false,
    });

    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Scanning
    );
    assert_eq!(core.snapshot().platform_identifier, None);
    let actions = core.drain_actions();
    assert!(actions.iter().any(|action| matches!(
        action,
        MobileMelkLightingSessionActionDto::CancelConnect { platform_identifier }
            if platform_identifier == ID
    )));
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, MobileMelkLightingSessionActionDto::Scan))
    );
}

#[test]
fn first_pairing_rescans_after_bluetooth_recovers() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(None);
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21  6A".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    core.drain_actions();
    core.select_candidate(ID.into());
    core.drain_actions();

    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: false,
        state_code: 4,
    });
    core.handle(MobileMelkLightingSessionEventDto::Disconnected {
        reason: "Bluetooth unavailable".into(),
        powered_on: false,
    });
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });

    assert!(
        core.drain_actions()
            .iter()
            .any(|action| matches!(action, MobileMelkLightingSessionActionDto::Scan))
    );
}

#[test]
fn disconnect_marks_pending_command_unconfirmed() {
    let core = ready_core();
    assert!(core.set_power(true));
    assert_eq!(core.snapshot().command_status, 1);

    core.handle(MobileMelkLightingSessionEventDto::Disconnected {
        reason: "link lost".into(),
        powered_on: false,
    });

    assert_eq!(core.snapshot().command_status, 3);
}

#[test]
fn non_color_writes_cannot_exceed_the_queue_limit() {
    let core = ready_core();
    for _ in 0..31 {
        assert!(core.set_power(true));
    }
    assert!(core.set_solid_color(1, 2, 3));

    assert!(!core.set_power(false));
}

#[test]
fn restore_rejects_invalid_brightness_as_brightness_error() {
    let core = ready_core();
    assert_eq!(
        core.apply_state(MobileMelkLightingRestoreStateDto {
            power_on: true,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 101,
            playback: None,
        }),
        Err(MobileMelkLightingError::InvalidBrightness)
    );
}

#[test]
fn stop_discards_pending_platform_actions() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(None);
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.stop();

    assert!(core.drain_actions().is_empty());
}

#[test]
fn final_initialization_write_waits_before_ready() {
    let core = initializing_core();
    core.handle(MobileMelkLightingSessionEventDto::TimerFired {
        timer: MobileMelkLightingTimerDto::Initialization,
        can_send: true,
    });
    core.drain_actions();
    assert!(matches!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Discovering
    ));
    assert!(!core.set_power(true));
}

#[test]
fn preferred_connection_timeouts_are_bounded() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.drain_actions();

    for _ in 0..4 {
        core.handle(MobileMelkLightingSessionEventDto::Discovered {
            name: Some("MELK-OC21  6A".into()),
            platform_identifier: ID.into(),
            rssi: -60,
        });
        core.handle(MobileMelkLightingSessionEventDto::TimerFired {
            timer: MobileMelkLightingTimerDto::ConnectionAttempt,
            can_send: false,
        });
    }

    assert!(matches!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Failed { .. }
    ));
}
