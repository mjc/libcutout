use cutout_core::{
    Capabilities, CommandKind, DeviceCommand, DeviceEvent, ProtocolSession, SessionInput,
    SessionOutput, TransportAction,
};

use crate::{FALCON_WRITE_CHANNEL, VETERAN_DATA_CHANNEL};

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
        match input {
            SessionInput::LinkUp(info) => {
                self.connected = true;
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: VETERAN_DATA_CHANNEL,
                }));
            }
            SessionInput::LinkDown => {
                self.connected = false;
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
                if self.connected && channel == VETERAN_DATA_CHANNEL {
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
        match input {
            SessionInput::LinkUp(info) => {
                self.connected = true;
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: FALCON_WRITE_CHANNEL,
                }));
            }
            SessionInput::LinkDown => {
                self.connected = false;
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
                if self.connected {
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
}
