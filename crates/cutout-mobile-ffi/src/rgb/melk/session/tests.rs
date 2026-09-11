use super::*;

const ID: &str = "11111111-1111-1111-1111-111111111111";

fn ready_core() -> std::sync::Arc<MobileMelkLightingSessionCore> {
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
        service_uuids: vec![cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes().to_vec()],
        error: None,
    });
    core.drain_actions();
    core.handle(
        MobileMelkLightingSessionEventDto::CharacteristicsDiscovered {
            name: Some("MELK-OC21  6A".into()),
            service_uuid: cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes().to_vec(),
            characteristics: vec![
                MobileMelkLightingCharacteristicEvidenceDto {
                    uuid: cutout_protocols::MELK_WRITE_CHANNEL.as_bytes().to_vec(),
                    write_without_response: true,
                    notify_or_indicate: false,
                },
                MobileMelkLightingCharacteristicEvidenceDto {
                    uuid: cutout_protocols::MELK_NOTIFY_CHANNEL.as_bytes().to_vec(),
                    write_without_response: false,
                    notify_or_indicate: true,
                },
            ],
            error: None,
        },
    );
    core.drain_actions();
    core.handle(MobileMelkLightingSessionEventDto::NotificationState {
        characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_bytes().to_vec(),
        ready: true,
        can_send: true,
        error: None,
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
