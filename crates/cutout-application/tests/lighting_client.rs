#![cfg_attr(test, allow(clippy::disallowed_macros))]

use cutout_application::lighting::melk::session::{
    LightingSessionAction, LightingSessionEvent, LightingSessionState, SessionReducer,
};

const PLATFORM_ID: &str = "11111111-1111-1111-1111-111111111111";

#[test]
fn rust_client_drives_radio_restore_and_gatt_discovery_through_public_service() {
    let mut session = SessionReducer::default();
    session.start(Some(PLATFORM_ID));
    session.handle(LightingSessionEvent::BluetoothState {
        powered_on: true,
        state_code: 5,
    });
    assert_eq!(
        session.drain_actions(),
        [LightingSessionAction::RestorePeripheral {
            platform_identifier: PLATFORM_ID.into(),
        }]
    );

    session.handle(LightingSessionEvent::Restored {
        name: Some("MELK-OC21".into()),
        platform_identifier: PLATFORM_ID.into(),
        connected: true,
        pending: false,
    });
    assert!(session.drain_actions().iter().any(|action| matches!(
        action,
        LightingSessionAction::DiscoverServices { platform_identifier, .. }
            if platform_identifier == PLATFORM_ID
    )));

    session.handle(LightingSessionEvent::ServicesDiscovered {
        service_uuids: vec![cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid()],
        error: None,
    });
    assert_eq!(session.snapshot().state, LightingSessionState::Discovering);
    assert!(session.drain_actions().iter().any(|action| matches!(
        action,
        LightingSessionAction::DiscoverCharacteristics { platform_identifier, .. }
            if platform_identifier == PLATFORM_ID
    )));
}
