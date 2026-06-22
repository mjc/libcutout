use cutout_core::{
    Capabilities, CommandKind, DeviceCommand, DeviceEvent, GattChannel, ProtocolSession,
    SessionInput, SessionOutput, TransportAction,
};

use crate::{FALCON_WRITE_CHANNEL, VETERAN_DATA_CHANNEL};

trait ReadOnlySessionProfile {
    fn subscribe_channel() -> GattChannel;

    fn accepts_notification(channel: GattChannel) -> bool;
}

struct AeroReadOnlySessionProfile;

impl ReadOnlySessionProfile for AeroReadOnlySessionProfile {
    fn subscribe_channel() -> GattChannel {
        VETERAN_DATA_CHANNEL
    }

    fn accepts_notification(channel: GattChannel) -> bool {
        channel == VETERAN_DATA_CHANNEL
    }
}

struct FalconReadOnlySessionProfile;

impl ReadOnlySessionProfile for FalconReadOnlySessionProfile {
    fn subscribe_channel() -> GattChannel {
        FALCON_WRITE_CHANNEL
    }

    fn accepts_notification(_: GattChannel) -> bool {
        true
    }
}

fn handle_read_only_session<P: ReadOnlySessionProfile>(
    connected: &mut bool,
    input: SessionInput<'_>,
    output: &mut Vec<SessionOutput>,
) {
    match input {
        SessionInput::LinkUp(info) => {
            *connected = true;
            output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            output.push(SessionOutput::Transport(TransportAction::Subscribe {
                channel: P::subscribe_channel(),
            }));
        }
        SessionInput::LinkDown => {
            *connected = false;
            output.push(SessionOutput::Event(DeviceEvent::LinkDown));
        }
        SessionInput::Tick { monotonic_ms } => {
            output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
        }
        SessionInput::Notification {
            channel,
            bytes,
            monotonic_ms,
        } => {
            if *connected && P::accepts_notification(channel) {
                output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                    channel,
                    monotonic_ms,
                    len: bytes.len(),
                }));
            }
        }
        SessionInput::Command(
            DeviceCommand::RequestIdentity
            | DeviceCommand::RequestTelemetry
            | DeviceCommand::RequestFirmwareInfo
            | DeviceCommand::RequestBatteryInfo
            | DeviceCommand::RequestDiagnostics
            | DeviceCommand::RequestSettings
            | DeviceCommand::SetLights(_)
            | DeviceCommand::SoundHorn
            | DeviceCommand::SetRawMotorCurrent { .. },
        ) => {}
    }
}

/// Minimal read-only session shell for NOSFET Aero/Veteran-family devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AeroReadOnlySession {
    connected: bool,
}

impl AeroReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub const fn capabilities() -> Capabilities {
        Capabilities::from_supported_commands([
            CommandKind::RequestIdentity,
            CommandKind::RequestFirmwareInfo,
            CommandKind::RequestTelemetry,
            CommandKind::RequestBatteryInfo,
            CommandKind::RequestDiagnostics,
        ])
    }
}

impl ProtocolSession for AeroReadOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        handle_read_only_session::<AeroReadOnlySessionProfile>(&mut self.connected, input, output);
    }
}

/// Minimal read-only session shell for Begode Falcon devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FalconReadOnlySession {
    connected: bool,
}

impl FalconReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub const fn capabilities() -> Capabilities {
        Capabilities::from_supported_commands([
            CommandKind::RequestIdentity,
            CommandKind::RequestFirmwareInfo,
            CommandKind::RequestTelemetry,
            CommandKind::RequestBatteryInfo,
        ])
    }
}

impl ProtocolSession for FalconReadOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        handle_read_only_session::<FalconReadOnlySessionProfile>(
            &mut self.connected,
            input,
            output,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;
    use cutout_core::{LinkInfo, TransportAction};

    const TEST_CHANNEL: GattChannel = GattChannel::from_bytes([0x11; 16]);

    struct TestProfile;

    impl ReadOnlySessionProfile for TestProfile {
        fn subscribe_channel() -> GattChannel {
            TEST_CHANNEL
        }

        fn accepts_notification(channel: GattChannel) -> bool {
            channel == TEST_CHANNEL
        }
    }

    #[test]
    fn shared_read_only_session_link_up_subscribes_profile_channel() {
        let mut connected = false;
        let mut output = Vec::new();

        handle_read_only_session::<TestProfile>(
            &mut connected,
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 7,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        assert!(connected);
        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::Transport(TransportAction::Subscribe { channel }) if *channel == TEST_CHANNEL
        )));
    }

    #[test]
    fn shared_read_only_session_accepts_matching_notifications_when_connected() {
        let mut connected = true;
        let mut output = Vec::new();

        handle_read_only_session::<TestProfile>(
            &mut connected,
            SessionInput::Notification {
                channel: TEST_CHANNEL,
                bytes: &[0x01, 0x02, 0x03],
                monotonic_ms: 11,
            },
            &mut output,
        );

        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::Event(DeviceEvent::NotificationReceived { channel, monotonic_ms, len })
                if *channel == TEST_CHANNEL && *monotonic_ms == 11 && *len == 3
        )));
    }

    #[test]
    fn shared_read_only_session_ignores_notifications_when_disconnected() {
        let mut connected = false;
        let mut output = Vec::new();

        handle_read_only_session::<TestProfile>(
            &mut connected,
            SessionInput::Notification {
                channel: TEST_CHANNEL,
                bytes: &[0x01, 0x02, 0x03],
                monotonic_ms: 11,
            },
            &mut output,
        );

        assert!(output.is_empty());
    }

    #[test]
    fn read_only_session_shells_remain_small() {
        assert_eq!(size_of::<AeroReadOnlySession>(), 1);
        assert_eq!(size_of::<FalconReadOnlySession>(), 1);
    }
}
