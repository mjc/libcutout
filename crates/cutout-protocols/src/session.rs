use core::marker::PhantomData;
use cutout_core::{
    Capabilities, CommandKind, DeviceCommand, DeviceEvent, GattChannel, ProtocolFamily,
    ProtocolSession, SessionInput, SessionOutput, TransportAction,
};

use crate::{FALCON_WRITE_CHANNEL, VETERAN_DATA_CHANNEL};

/// Static manufacturer identifier for a supported model spec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Manufacturer {
    /// NOSFET hardware using the Veteran/LeaperKim/NOSFET protocol family.
    Nosfet,

    /// Begode/Gotway hardware.
    Begode,
}

/// Type-level read-only model contract.
pub trait ReadOnlyModelSpec {
    /// Device manufacturer.
    const MANUFACTURER: Manufacturer;

    /// Protocol family used by this model.
    const PROTOCOL: ProtocolFamily;

    /// Stable model name.
    const MODEL: &'static str;

    /// Commands this read-only model session can schedule.
    const CAPABILITIES: Capabilities;

    /// GATT characteristic to subscribe to after link-up.
    const SUBSCRIBE_CHANNEL: GattChannel;
}

/// NOSFET Aero read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NosfetAeroModel;

impl ReadOnlyModelSpec for NosfetAeroModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Nosfet;
    const MODEL: &'static str = "NOSFET Aero";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::VeteranLeaperkimNosfet;
    const CAPABILITIES: Capabilities = Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestBatteryInfo,
        CommandKind::RequestDiagnostics,
    ]);
    const SUBSCRIBE_CHANNEL: GattChannel = VETERAN_DATA_CHANNEL;
}

/// Begode Falcon read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeFalconModel;

impl ReadOnlyModelSpec for BegodeFalconModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Begode;
    const MODEL: &'static str = "Begode Falcon";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::BegodeGotway;
    const CAPABILITIES: Capabilities = Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestBatteryInfo,
    ]);
    const SUBSCRIBE_CHANNEL: GattChannel = FALCON_WRITE_CHANNEL;
}

fn handle_read_only_session<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool>(
    connected: &mut bool,
    input: SessionInput<'_>,
    output: &mut Vec<SessionOutput>,
) {
    match input {
        SessionInput::LinkUp(info) => {
            *connected = true;
            output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            output.push(SessionOutput::Transport(TransportAction::Subscribe {
                channel: M::SUBSCRIBE_CHANNEL,
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
            if *connected && (ACCEPT_ANY_NOTIFICATION || channel == M::SUBSCRIBE_CHANNEL) {
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

/// Generic read-only session shell for one statically-known model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadOnlySession<M, const ACCEPT_ANY_NOTIFICATION: bool> {
    connected: bool,
    model: PhantomData<fn() -> M>,
}

impl<M, const ACCEPT_ANY_NOTIFICATION: bool> Default
    for ReadOnlySession<M, ACCEPT_ANY_NOTIFICATION>
{
    fn default() -> Self {
        Self {
            connected: false,
            model: PhantomData,
        }
    }
}

impl<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool>
    ReadOnlySession<M, ACCEPT_ANY_NOTIFICATION>
{
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub const fn capabilities() -> Capabilities {
        M::CAPABILITIES
    }

    /// Returns this session's manufacturer.
    #[must_use]
    pub const fn manufacturer() -> Manufacturer {
        M::MANUFACTURER
    }

    /// Returns this session's protocol family.
    #[must_use]
    pub const fn protocol() -> ProtocolFamily {
        M::PROTOCOL
    }

    /// Returns this session's stable model name.
    #[must_use]
    pub const fn model() -> &'static str {
        M::MODEL
    }
}

impl<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool> ProtocolSession
    for ReadOnlySession<M, ACCEPT_ANY_NOTIFICATION>
{
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        handle_read_only_session::<M, ACCEPT_ANY_NOTIFICATION>(&mut self.connected, input, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;
    use cutout_core::{LinkInfo, TransportAction};

    const TEST_CHANNEL: GattChannel = GattChannel::from_bytes([0x11; 16]);

    struct TestModel;

    impl ReadOnlyModelSpec for TestModel {
        const MANUFACTURER: Manufacturer = Manufacturer::Nosfet;
        const MODEL: &'static str = "test";
        const PROTOCOL: ProtocolFamily = ProtocolFamily::VeteranLeaperkimNosfet;
        const CAPABILITIES: Capabilities =
            Capabilities::from_supported_commands([CommandKind::RequestTelemetry]);
        const SUBSCRIBE_CHANNEL: GattChannel = TEST_CHANNEL;
    }

    #[test]
    fn shared_read_only_session_link_up_subscribes_profile_channel() {
        let mut connected = false;
        let mut output = Vec::new();

        handle_read_only_session::<TestModel, false>(
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

        handle_read_only_session::<TestModel, false>(
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

        handle_read_only_session::<TestModel, false>(
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
        assert_eq!(size_of::<ReadOnlySession<NosfetAeroModel, false>>(), 1);
        assert_eq!(size_of::<ReadOnlySession<BegodeFalconModel, true>>(), 1);
    }

    #[test]
    fn read_only_session_identity_comes_from_model_spec() {
        assert_eq!(
            ReadOnlySession::<NosfetAeroModel, false>::manufacturer(),
            Manufacturer::Nosfet
        );
        assert_eq!(
            ReadOnlySession::<NosfetAeroModel, false>::protocol(),
            ProtocolFamily::VeteranLeaperkimNosfet
        );
        assert_eq!(
            ReadOnlySession::<NosfetAeroModel, false>::model(),
            "NOSFET Aero"
        );

        assert_eq!(
            ReadOnlySession::<BegodeFalconModel, true>::manufacturer(),
            Manufacturer::Begode
        );
        assert_eq!(
            ReadOnlySession::<BegodeFalconModel, true>::protocol(),
            ProtocolFamily::BegodeGotway
        );
        assert_eq!(
            ReadOnlySession::<BegodeFalconModel, true>::model(),
            "Begode Falcon"
        );
    }

    #[test]
    fn generic_read_only_session_uses_model_capabilities() {
        assert_eq!(
            ReadOnlySession::<TestModel, false>::capabilities(),
            Capabilities::from_supported_commands([CommandKind::RequestTelemetry])
        );
    }
}
