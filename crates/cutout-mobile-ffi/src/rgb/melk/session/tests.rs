use super::*;
use crate::{MobileMelkLightingError, MobileMelkLightingRestoreStateDto};

const ID: &str = "11111111-1111-1111-1111-111111111111";

#[test]
fn restored_peripherals_wait_for_bluetooth_power_before_transport_operations() {
    for (connected, pending) in [(true, false), (false, true), (false, false)] {
        let core = MobileMelkLightingSessionCore::new();
        core.start(Some(ID.into()));
        core.handle(MobileMelkLightingSessionEventDto::Restored {
            name: Some("MELK-OC21".into()),
            platform_identifier: ID.into(),
            connected,
            pending,
        });
        assert_eq!(core.snapshot().platform_identifier.as_deref(), Some(ID));
        assert!(
            core.drain_actions().is_empty(),
            "restoration precedes the radio state callback"
        );
        core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
            powered_on: true,
            state_code: 5,
        });
        let actions = core.drain_actions();
        if connected {
            assert!(actions.iter().any(|action| matches!(action,
                MobileMelkLightingSessionActionDto::DiscoverServices { platform_identifier, .. } if platform_identifier == ID
            )));
        } else if pending {
            assert!(!actions.iter().any(|action| matches!(
                action,
                MobileMelkLightingSessionActionDto::Connect { .. }
            )));
            assert!(actions.iter().any(|action| matches!(
                action,
                MobileMelkLightingSessionActionDto::ArmTimer {
                    timer: MobileMelkLightingTimerDto::ConnectionAttempt,
                    ..
                }
            )));
        } else {
            assert!(actions.iter().any(|action| matches!(action,
                MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == ID
            )));
        }
        core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
            powered_on: true,
            state_code: 5,
        });
        assert!(core.drain_actions().is_empty());
    }
}

#[test]
fn unavailable_restoration_does_not_scan_before_bluetooth_is_powered_on() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::RestoreUnavailable);
    assert!(core.drain_actions().is_empty());
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert_eq!(
        core.drain_actions(),
        [MobileMelkLightingSessionActionDto::RestorePeripheral {
            platform_identifier: ID.into(),
        }]
    );
}

#[test]
fn connection_completion_before_the_radio_callback_is_deferred() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::Restored {
        name: None,
        platform_identifier: ID.into(),
        connected: false,
        pending: true,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::Connected {
        name: None,
        platform_identifier: ID.into(),
    });
    assert!(core.drain_actions().is_empty());
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert!(core.drain_actions().iter().any(|action| matches!(action,
        MobileMelkLightingSessionActionDto::DiscoverServices { platform_identifier, .. } if platform_identifier == ID
    )));
}

#[test]
fn power_loss_discards_deferred_restoration_before_the_next_power_cycle() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::Restored {
        name: None,
        platform_identifier: ID.into(),
        connected: true,
        pending: false,
    });
    assert!(core.drain_actions().is_empty());
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: false,
        state_code: 4,
    });
    assert_eq!(core.snapshot().platform_identifier, None);
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert_eq!(
        core.drain_actions(),
        [MobileMelkLightingSessionActionDto::RestorePeripheral {
            platform_identifier: ID.into(),
        }]
    );
}

#[test]
fn disconnect_radio_state_also_gates_restoration_operations() {
    let core = ready_core();
    core.handle(MobileMelkLightingSessionEventDto::Disconnected {
        reason: "radio off".into(),
        powered_on: false,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::Restored {
        name: None,
        platform_identifier: ID.into(),
        connected: true,
        pending: false,
    });
    assert!(core.drain_actions().is_empty());
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert!(core.drain_actions().iter().any(|action| matches!(action,
        MobileMelkLightingSessionActionDto::DiscoverServices { platform_identifier, .. } if platform_identifier == ID
    )));
}

#[test]
fn late_subscription_errors_preserve_the_pending_reconnect() {
    let core = ready_core();
    core.handle(MobileMelkLightingSessionEventDto::Disconnected {
        reason: "link lost".into(),
        powered_on: true,
    });
    core.drain_actions();
    let retrying = core.snapshot();
    // Subscription errors may arrive after CoreBluetooth reports a disconnect.
    core.handle(MobileMelkLightingSessionEventDto::NotificationState {
        characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
        ready: false,
        can_send: false,
        error: Some("subscription lost".into()),
    });
    core.drain_actions();
    assert_eq!(core.snapshot(), retrying);
    core.handle(MobileMelkLightingSessionEventDto::TimerFired {
        timer: MobileMelkLightingTimerDto::Reconnect,
        can_send: true,
    });
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Connecting
    );
    assert!(core.drain_actions().iter().any(|action| matches!(
        action, MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == ID
    )));
}

#[test]
fn stopped_sessions_ignore_late_transport_callbacks() {
    for event in [
        MobileMelkLightingSessionEventDto::BluetoothState {
            powered_on: true,
            state_code: 5,
        },
        MobileMelkLightingSessionEventDto::Restored {
            name: Some("MELK-OC21".into()),
            platform_identifier: ID.into(),
            connected: true,
            pending: false,
        },
        MobileMelkLightingSessionEventDto::Connected {
            name: Some("MELK-OC21".into()),
            platform_identifier: ID.into(),
        },
        MobileMelkLightingSessionEventDto::ConnectFailed {
            reason: "late failure".into(),
        },
        MobileMelkLightingSessionEventDto::Notification {
            characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
            bytes: vec![1, 2, 3],
        },
    ] {
        let core = ready_core();
        core.stop();
        let stopped = core.snapshot();
        core.handle(event.clone());
        assert_eq!(core.snapshot(), stopped, "late event: {event:?}");
        assert!(core.drain_actions().is_empty());
        assert!(core.drain_notifications().is_empty());
    }
}

#[test]
fn resume_restarts_exhausted_retries_and_keeps_the_remembered_identity() {
    let core = ready_core();
    for _ in 0..4 {
        core.handle(MobileMelkLightingSessionEventDto::ConnectFailed {
            reason: "accessory unavailable".into(),
        });
        core.drain_actions();
    }
    assert!(matches!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Failed { .. }
    ));

    core.handle(MobileMelkLightingSessionEventDto::Resume);
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Idle
    );
    assert_eq!(core.snapshot().platform_identifier, None);
    assert_eq!(
        core.drain_actions(),
        [MobileMelkLightingSessionActionDto::RestartTransport]
    );

    // Duplicate lifecycle notifications must not keep recreating the adapter.
    core.handle(MobileMelkLightingSessionEventDto::Resume);
    assert!(core.drain_actions().is_empty());
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::RestoreUnavailable);
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("unrelated controller".into()),
        platform_identifier: "22222222-2222-2222-2222-222222222222".into(),
        rssi: -30,
    });
    assert!(core.drain_actions().is_empty());
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: None,
        platform_identifier: ID.into(),
        rssi: -60,
    });
    assert!(core.drain_actions().iter().any(|action| matches!(
        action, MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == ID
    )));
    // The retry budget belongs to this fresh session.
    core.handle(MobileMelkLightingSessionEventDto::ConnectFailed {
        reason: "temporary failure".into(),
    });
    assert!(matches!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Retrying { attempt: 1, .. }
    ));
}

#[test]
fn resume_recovers_a_power_loss_but_does_not_revive_an_explicitly_stopped_session() {
    let core = ready_core();
    core.handle(MobileMelkLightingSessionEventDto::Disconnected {
        reason: "Bluetooth off".into(),
        powered_on: false,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::Resume);
    assert_eq!(
        core.drain_actions(),
        [MobileMelkLightingSessionActionDto::RestartTransport]
    );
    core.stop();
    core.handle(MobileMelkLightingSessionEventDto::Resume);
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Disconnected
    );
    assert!(core.drain_actions().is_empty());
}

#[test]
fn resume_preserves_active_connections_and_their_timers_and_commands() {
    let ready = ready_core();
    assert!(ready.set_power(true));
    let initializing = initializing_core();
    let connecting = MobileMelkLightingSessionCore::new();
    connecting.start(Some(ID.into()));
    connecting.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    connecting.handle(MobileMelkLightingSessionEventDto::Restored {
        name: None,
        platform_identifier: ID.into(),
        connected: false,
        pending: true,
    });
    connecting.drain_actions();
    let retrying = ready_core();
    retrying.handle(MobileMelkLightingSessionEventDto::ConnectFailed {
        reason: "temporary".into(),
    });
    retrying.drain_actions();
    let scanning = MobileMelkLightingSessionCore::new();
    scanning.start(None);
    scanning.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    scanning.drain_actions();
    for core in [&ready, &initializing, &connecting, &retrying, &scanning] {
        let before = core.snapshot();
        core.handle(MobileMelkLightingSessionEventDto::Resume);
        assert_eq!(core.snapshot(), before);
        assert!(core.drain_actions().is_empty());
    }
    connecting.handle(MobileMelkLightingSessionEventDto::ConnectTimeout);
    assert_eq!(
        connecting.snapshot().state,
        MobileMelkLightingSessionStateDto::Scanning
    );
    initializing.handle(MobileMelkLightingSessionEventDto::TimerFired {
        timer: MobileMelkLightingTimerDto::Initialization,
        can_send: true,
    });
    assert!(
        initializing
            .drain_actions()
            .iter()
            .any(|action| matches!(action, MobileMelkLightingSessionActionDto::Write { .. }))
    );
    assert_eq!(ready.snapshot().command_status, 1);
}

#[test]
fn resume_does_not_retry_an_invalid_remembered_identity() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some("invalid".into()));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    let before = core.snapshot();
    core.handle(MobileMelkLightingSessionEventDto::Resume);
    assert_eq!(core.snapshot(), before);
    assert!(core.drain_actions().is_empty());
}

fn initializing_core() -> std::sync::Arc<MobileMelkLightingSessionCore> {
    initializing_core_with_name(Some(ID), Some("MELK-OC21  6A"))
}

fn initializing_core_with_name(
    preferred_identifier: Option<&str>,
    callback_name: Option<&str>,
) -> std::sync::Arc<MobileMelkLightingSessionCore> {
    let core = MobileMelkLightingSessionCore::new();
    core.start(preferred_identifier.map(str::to_owned));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::RestoreUnavailable);
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21  6A".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    if preferred_identifier.is_none() {
        verify_probe(&core, ID, callback_name);
        core.select_candidate(ID.into());
    }
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::Connected {
        name: callback_name.map(str::to_owned),
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
            name: callback_name.map(str::to_owned),
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
    finish_initialization(&core);
    core
}

fn finish_initialization(core: &MobileMelkLightingSessionCore) {
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
}

#[test]
fn first_pairing_keeps_the_advertised_name_when_connected_callbacks_omit_it() {
    let core = initializing_core_with_name(None, None);
    assert_eq!(core.snapshot().name.as_deref(), Some("MELK-OC21  6A"));
    finish_initialization(&core);
    assert!(core.set_power(true));
}

#[test]
fn first_pairing_remembers_the_verified_identity_across_session_recovery() {
    let core = initializing_core_with_name(None, Some("MELK-OC21"));
    finish_initialization(&core);
    core.handle(MobileMelkLightingSessionEventDto::NotificationState {
        characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
        ready: false,
        can_send: false,
        error: Some("subscription lost".into()),
    });
    core.handle(MobileMelkLightingSessionEventDto::Resume);
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert_eq!(
        core.drain_actions(),
        [MobileMelkLightingSessionActionDto::RestorePeripheral {
            platform_identifier: ID.into(),
        }]
    );
    core.handle(MobileMelkLightingSessionEventDto::RestoreUnavailable);
    core.drain_actions();
    core.drain_candidates();
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21".into()),
        platform_identifier: "22222222-2222-2222-2222-222222222222".into(),
        rssi: -30,
    });
    assert!(
        core.drain_candidates().is_empty(),
        "recovery must stay with the paired controller"
    );
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: None,
        platform_identifier: ID.into(),
        rssi: -60,
    });
    assert!(core.drain_actions().iter().any(|action| matches!(
        action, MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == ID
    )));
}

#[test]
fn reducer_owns_gatt_initialization_and_ready_gate() {
    let core = ready_core();
    let snapshot = core.snapshot();
    assert_eq!(snapshot.platform_identifier.as_deref(), Some(ID));
    assert!(snapshot.notification_ready);
}

#[test]
fn restored_notifications_cannot_bypass_profile_verification() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::Restored {
        name: Some("MELK-OC21".into()),
        platform_identifier: ID.into(),
        connected: true,
        pending: false,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::NotificationState {
        characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
        ready: true,
        can_send: true,
        error: None,
    });
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Discovering
    );
    assert!(!core.snapshot().notification_ready);
    assert!(!core.set_power(true));
    assert!(core.drain_actions().is_empty());
}

#[test]
fn connected_and_restored_gatt_discovery_have_a_deadline() {
    for restored in [false, true] {
        let core = MobileMelkLightingSessionCore::new();
        core.start(Some(ID.into()));
        core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
            powered_on: true,
            state_code: 5,
        });
        if restored {
            core.handle(MobileMelkLightingSessionEventDto::Restored {
                name: Some("MELK-OC21".into()),
                platform_identifier: ID.into(),
                connected: true,
                pending: false,
            });
        } else {
            core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
                powered_on: true,
                state_code: 5,
            });
            core.handle(MobileMelkLightingSessionEventDto::RestoreUnavailable);
            core.handle(MobileMelkLightingSessionEventDto::Discovered {
                name: Some("MELK-OC21".into()),
                platform_identifier: ID.into(),
                rssi: -60,
            });
            core.drain_actions();
            core.handle(MobileMelkLightingSessionEventDto::Connected {
                name: Some("MELK-OC21".into()),
                platform_identifier: ID.into(),
            });
        }
        let timer = core
            .drain_actions()
            .into_iter()
            .find_map(|action| match action {
                MobileMelkLightingSessionActionDto::ArmTimer {
                    timer,
                    delay_milliseconds,
                } => {
                    assert_eq!(delay_milliseconds, 15_000);
                    Some(timer)
                }
                _ => None,
            })
            .expect("GATT discovery must not hang indefinitely");
        core.handle(MobileMelkLightingSessionEventDto::TimerFired {
            timer,
            can_send: true,
        });
        assert_eq!(
            core.snapshot().state,
            MobileMelkLightingSessionStateDto::Scanning
        );
        assert!(core.drain_actions().iter().any(|action| matches!(
            action, MobileMelkLightingSessionActionDto::CancelConnect { platform_identifier } if platform_identifier == ID
        )));
        core.handle(MobileMelkLightingSessionEventDto::NotificationState {
            characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid().into(),
            ready: true,
            can_send: true,
            error: None,
        });
        assert!(!core.set_power(true));
    }
}

#[test]
fn rejected_remembered_profile_cancels_the_active_connection() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(Some(ID.into()));
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::Restored {
        name: Some("MELK-OC21".into()),
        platform_identifier: ID.into(),
        connected: true,
        pending: false,
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::ServicesDiscovered {
        service_uuids: Vec::new(),
        error: None,
    });

    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Failed {
            reason: "missing FFF0 service".into()
        }
    );
    assert!(core.drain_actions().iter().any(|action| matches!(
        action,
        MobileMelkLightingSessionActionDto::CancelConnect { platform_identifier }
            if platform_identifier == ID
    )));
}

#[test]
fn disconnect_after_early_confirmation_marks_a_partial_state_unconfirmed() {
    let core = ready_core();
    assert!(
        core.apply_state(MobileMelkLightingRestoreStateDto {
            power_on: true,
            red: 12,
            green: 34,
            blue: 56,
            brightness: 80,
            playback: None,
        })
        .unwrap()
    );
    core.flush_writes(true);
    core.drain_actions();

    assert!(!core.mark_last_command_confirmed());
    core.handle(MobileMelkLightingSessionEventDto::Disconnected {
        reason: "link lost during state update".into(),
        powered_on: true,
    });

    assert_eq!(core.snapshot().command_status, 3);
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
    core.handle(MobileMelkLightingSessionEventDto::RestoreUnavailable);
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
    core.handle(MobileMelkLightingSessionEventDto::RestoreUnavailable);
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
    core.drain_actions();
    let failed = core.snapshot();
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    assert_eq!(core.snapshot(), failed);
    assert!(core.drain_actions().is_empty());
}

#[test]
fn radio_off_refuses_candidate_selection_and_late_scan_results() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(None);
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    core.drain_actions();
    core.drain_candidates();
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: false,
        state_code: 4,
    });
    let failed = core.snapshot();
    core.select_candidate(ID.into());
    assert_eq!(core.snapshot(), failed);
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    assert!(core.drain_actions().is_empty());
    assert!(core.drain_candidates().is_empty());
}

fn scanning_core() -> std::sync::Arc<MobileMelkLightingSessionCore> {
    let core = MobileMelkLightingSessionCore::new();
    core.start(None);
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert!(
        core.drain_actions()
            .iter()
            .any(|action| matches!(action, MobileMelkLightingSessionActionDto::Scan))
    );
    core
}

#[test]
fn discovery_refuses_malformed_platform_identifiers() {
    for identifier in ["", "not-a-uuid"] {
        let core = scanning_core();
        core.handle(MobileMelkLightingSessionEventDto::Discovered {
            name: Some("MELK-OC21".into()),
            platform_identifier: identifier.into(),
            rssi: -60,
        });
        assert!(core.drain_candidates().is_empty());
        assert_eq!(core.drain_candidate_removals(), [identifier.to_owned()]);
        core.select_candidate(identifier.into());
        assert!(core.drain_actions().is_empty());
    }
}

#[test]
fn changed_names_do_not_remove_protocol_verified_candidates() {
    let core = scanning_core();
    discover_verified(&core, ID, Some("MELK-OC21"));
    core.drain_candidates();
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("aero lights".into()),
        platform_identifier: ID.into(),
        rssi: -40,
    });
    assert_eq!(
        core.drain_candidates()[0].name.as_deref(),
        Some("aero lights")
    );
    assert!(core.drain_candidate_removals().is_empty());
}

#[test]
fn unsupported_gatt_withdraws_the_selected_candidate_before_rescanning() {
    let core = scanning_core();
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21".into()),
        platform_identifier: ID.into(),
        rssi: -60,
    });
    core.drain_candidates();
    core.select_candidate(ID.into());
    core.handle(MobileMelkLightingSessionEventDto::Connected {
        name: Some("MELK-OC21".into()),
        platform_identifier: ID.into(),
    });
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::ServicesDiscovered {
        service_uuids: vec![],
        error: None,
    });
    assert_eq!(core.drain_candidate_removals(), [ID.to_owned()]);
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Scanning
    );
    let actions = core.drain_actions();
    assert!(actions.iter().any(|action| matches!(
        action, MobileMelkLightingSessionActionDto::CancelConnect { platform_identifier }
            if platform_identifier == ID
    )));
    // Scanning remains active while the read-only probe runs.
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, MobileMelkLightingSessionActionDto::Write { .. }))
    );
    assert!(!core.set_power(true));
    core.select_candidate(ID.into());
    assert!(core.drain_actions().is_empty());
}

#[test]
fn matching_gatt_admits_every_connected_name() {
    for name in [Some("aero lights"), Some("AiDot-28EC"), None] {
        let core = initializing_core_with_name(Some(ID), name);
        finish_initialization(&core);
        assert!(core.snapshot().notification_ready);
        assert!(core.set_power(true));
    }
}

#[test]
fn candidate_cap_reports_every_identifier_that_must_be_removed() {
    let core = MobileMelkLightingSessionCore::new();
    core.start(None);
    core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    let identifiers = (1..=33_u128)
        .map(|value| uuid::Uuid::from_u128(value).to_string())
        .collect::<Vec<_>>();
    for identifier in &identifiers {
        core.handle(MobileMelkLightingSessionEventDto::Discovered {
            name: Some("MELK-OC21".into()),
            platform_identifier: identifier.clone(),
            rssi: -60,
        });
        if identifier != &identifiers[32] {
            verify_probe(&core, identifier, Some("aero lights"));
        }
        core.drain_candidates();
        core.drain_actions();
    }

    assert_eq!(
        core.drain_candidate_removals(),
        [identifiers[32].clone()],
        "Rust must tell the UI and transport cache which overflow peripheral to release"
    );
    core.select_candidate(identifiers[32].clone());
    core.drain_actions();
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Scanning
    );
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: Some("MELK-OC21 6A".into()),
        platform_identifier: identifiers[0].clone(),
        rssi: -40,
    });
    let updated = core.drain_candidates();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].rssi, -40);
    assert!(core.drain_candidate_removals().is_empty());
    core.select_candidate(identifiers[0].clone());
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Connecting
    );
}

#[test]
fn discovery_probes_without_listing_or_writing_until_gatt_is_verified() {
    for name in [Some("aero lights"), Some("MELK-OC99"), None] {
        let core = scanning_core();
        core.handle(MobileMelkLightingSessionEventDto::Discovered {
            name: name.map(str::to_owned),
            platform_identifier: ID.into(),
            rssi: -40,
        });
        assert!(core.drain_candidates().is_empty());
        assert!(core.drain_actions().iter().any(|action| matches!(action,
            MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == ID)));
        core.handle(MobileMelkLightingSessionEventDto::Connected {
            name: name.map(str::to_owned),
            platform_identifier: ID.into(),
        });
        core.handle(MobileMelkLightingSessionEventDto::ServicesDiscovered {
            service_uuids: vec![cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid().into()],
            error: None,
        });
        core.handle(
            MobileMelkLightingSessionEventDto::CharacteristicsDiscovered {
                name: name.map(str::to_owned),
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
        assert_eq!(core.drain_candidates().len(), 1);
        assert!(!core.drain_actions().iter().any(|action| matches!(
            action,
            MobileMelkLightingSessionActionDto::Write { .. }
                | MobileMelkLightingSessionActionDto::Subscribe { .. }
        )));
        assert!(!core.set_power(true));
        core.select_candidate(ID.into());
        assert_eq!(
            core.snapshot().state,
            MobileMelkLightingSessionStateDto::Connecting
        );
    }
}

fn discover_verified(core: &MobileMelkLightingSessionCore, id: &str, name: Option<&str>) {
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: name.map(str::to_owned),
        platform_identifier: id.into(),
        rssi: -60,
    });
    verify_probe(core, id, name);
    core.drain_actions();
}

fn verify_probe(core: &MobileMelkLightingSessionCore, id: &str, name: Option<&str>) {
    core.handle(MobileMelkLightingSessionEventDto::Connected {
        name: name.map(str::to_owned),
        platform_identifier: id.into(),
    });
    core.handle(MobileMelkLightingSessionEventDto::ServicesDiscovered {
        service_uuids: vec![cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid().into()],
        error: None,
    });
    core.handle(
        MobileMelkLightingSessionEventDto::CharacteristicsDiscovered {
            name: name.map(str::to_owned),
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
}

#[test]
fn probes_advance_past_noise_failure_and_timeout_without_writes() {
    for failure in 0..3 {
        let core = scanning_core();
        let next = "22222222-2222-2222-2222-222222222222";
        for id in [ID, next] {
            core.handle(MobileMelkLightingSessionEventDto::Discovered {
                name: None,
                platform_identifier: id.into(),
                rssi: -40,
            });
        }
        core.drain_actions();
        match failure {
            0 => core.handle(MobileMelkLightingSessionEventDto::ConnectFailed {
                reason: "unreachable".into(),
            }),
            1 => core.handle(MobileMelkLightingSessionEventDto::TimerFired {
                timer: MobileMelkLightingTimerDto::ConnectionAttempt,
                can_send: false,
            }),
            _ => {
                core.handle(MobileMelkLightingSessionEventDto::Connected {
                    name: None,
                    platform_identifier: ID.into(),
                });
                core.handle(MobileMelkLightingSessionEventDto::ServicesDiscovered {
                    service_uuids: vec![],
                    error: None,
                });
            }
        }
        assert!(core.drain_candidates().is_empty());
        let actions = core.drain_actions();
        assert!(actions.iter().any(|action| matches!(action, MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == next)));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, MobileMelkLightingSessionActionDto::Write { .. }))
        );
        verify_probe(&core, next, None);
        assert_eq!(core.drain_candidates()[0].platform_identifier, next);
    }
}

#[test]
fn unpaired_platform_restoration_cannot_select_another_vehicles_accessory() {
    let core = scanning_core();
    core.handle(MobileMelkLightingSessionEventDto::Restored {
        name: Some("aero lights".into()),
        platform_identifier: ID.into(),
        connected: true,
        pending: false,
    });
    assert!(core.drain_actions().is_empty());
    assert_eq!(core.snapshot().platform_identifier, None);
}

#[test]
fn choosing_verified_lights_interrupts_an_unrelated_probe() {
    let core = scanning_core();
    discover_verified(&core, ID, None);
    let other = "22222222-2222-2222-2222-222222222222";
    core.handle(MobileMelkLightingSessionEventDto::Discovered {
        name: None,
        platform_identifier: other.into(),
        rssi: -20,
    });
    core.drain_actions();
    core.select_candidate(ID.into());
    let actions = core.drain_actions();
    assert!(actions.iter().any(|action| matches!(action, MobileMelkLightingSessionActionDto::CancelConnect { platform_identifier } if platform_identifier == other)));
    assert!(actions.iter().any(|action| matches!(action, MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == ID)));
    assert_eq!(core.snapshot().platform_identifier.as_deref(), Some(ID));
}

#[test]
fn first_pairing_renamed_lights_requires_selection_and_full_initialization() {
    let core = initializing_core_with_name(None, Some("aero lights"));
    assert!(!core.set_power(true));
    finish_initialization(&core);
    assert_eq!(
        core.snapshot().state,
        MobileMelkLightingSessionStateDto::Ready
    );
    assert_eq!(core.snapshot().name.as_deref(), Some("aero lights"));
    assert!(core.set_power(true));
}

#[test]
fn temporary_probe_failure_does_not_hide_a_device_for_the_session() {
    for timed_out in [false, true] {
        let core = scanning_core();
        core.handle(MobileMelkLightingSessionEventDto::Discovered {
            name: None,
            platform_identifier: ID.into(),
            rssi: -40,
        });
        core.drain_actions();
        if timed_out {
            core.handle(MobileMelkLightingSessionEventDto::TimerFired {
                timer: MobileMelkLightingTimerDto::ConnectionAttempt,
                can_send: false,
            });
        } else {
            core.handle(MobileMelkLightingSessionEventDto::ConnectFailed {
                reason: "busy".into(),
            });
        }
        core.drain_actions();
        core.handle(MobileMelkLightingSessionEventDto::Discovered {
            name: Some("aero lights".into()),
            platform_identifier: ID.into(),
            rssi: -40,
        });
        assert!(core.drain_actions().iter().any(|action| matches!(action, MobileMelkLightingSessionActionDto::Connect { platform_identifier } if platform_identifier == ID)));
    }
}
