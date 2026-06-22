use core::marker::PhantomData;
use cutout_core::{
    Capabilities, CommandKind, DeviceCommand, DeviceEvent, GattChannel, MonotonicMillis,
    ParserDiagnostics, ParserError, ProtocolFamily, ProtocolSession, SafetyClass, SessionInput,
    SessionOutput, TransportAction,
};

use crate::{
    FALCON_WRITE_CHANNEL, VETERAN_DATA_CHANNEL, VeteranFrame, VeteranFrameReassembler,
    VeteranReassemblyError, VeteranTelemetry, VeteranTelemetryError,
};

/// Static manufacturer identifier for a supported model spec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Manufacturer {
    /// NOSFET hardware using the Veteran/LeaperKim/NOSFET protocol family.
    Nosfet,

    /// Begode/Gotway hardware.
    Begode,
}

/// Static protocol model contract.
pub trait ProtocolModelSpec {
    /// Device manufacturer.
    const MANUFACTURER: Manufacturer;

    /// Protocol family used by this model.
    const PROTOCOL: ProtocolFamily;

    /// Stable model name.
    const MODEL: &'static str;
}

/// Type-level operation class marker.
pub trait ProtocolOperation: Sized {
    /// Safety class for this operation class.
    const SAFETY_CLASS: SafetyClass;

    /// Returns the operation safety class.
    #[must_use]
    fn safety_class(self) -> SafetyClass {
        Self::SAFETY_CLASS
    }
}

/// Read-only request operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadOnlyOperation;

impl ProtocolOperation for ReadOnlyOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::ReadOnly;
}

/// Settings writes that require stationary-state validation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SettingsWriteOperation;

impl ProtocolOperation for SettingsWriteOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::StationaryOnly;
}

/// Benign controls such as lights or horn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BenignControlOperation;

impl ProtocolOperation for BenignControlOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::BenignControl;
}

/// Dangerous actuation or motion-affecting controls.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DangerousActuationOperation;

impl ProtocolOperation for DangerousActuationOperation {
    const SAFETY_CLASS: SafetyClass = SafetyClass::Actuation;
}

/// Type-level read-only request capability.
pub trait SupportsReadRequests: ProtocolModelSpec {
    /// Operation marker for read requests.
    const READ_OPERATION: ReadOnlyOperation = ReadOnlyOperation;

    /// Commands this read-only model session can schedule.
    const READ_CAPABILITIES: Capabilities;

    /// GATT characteristic to subscribe to after link-up.
    const SUBSCRIBE_CHANNEL: GattChannel;

    /// Stateful decoder for accepted notifications from this model.
    type NotificationDecoder: ReadOnlyNotificationDecoder + Default;
}

/// Decoder hook for read-only model notification streams.
pub trait ReadOnlyNotificationDecoder {
    /// Resets model-specific parser state.
    fn reset(&mut self);

    /// Handles an accepted notification payload.
    fn handle_notification(
        &mut self,
        bytes: &[u8],
        monotonic_ms: MonotonicMillis,
        output: &mut Vec<SessionOutput>,
    );
}

/// No-op notification decoder for models without typed notification decoding yet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopNotificationDecoder;

impl ReadOnlyNotificationDecoder for NoopNotificationDecoder {
    fn reset(&mut self) {}

    fn handle_notification(
        &mut self,
        _bytes: &[u8],
        _monotonic_ms: MonotonicMillis,
        _output: &mut Vec<SessionOutput>,
    ) {
    }
}

/// Veteran/LeaperKim/NOSFET notification decoder for NOSFET Aero telemetry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VeteranNotificationDecoder {
    reassembler: VeteranFrameReassembler,
}

impl ReadOnlyNotificationDecoder for VeteranNotificationDecoder {
    fn reset(&mut self) {
        self.reassembler.reset();
    }

    fn handle_notification(
        &mut self,
        bytes: &[u8],
        monotonic_ms: MonotonicMillis,
        output: &mut Vec<SessionOutput>,
    ) {
        for byte in bytes {
            match self.reassembler.feed_byte(*byte) {
                Ok(Some(frame)) => push_veteran_frame(&frame, monotonic_ms, output),
                Ok(None) => {}
                Err(VeteranReassemblyError::CrcMismatch) => {
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        diagnostics_for(ParserError::BadChecksum),
                    )));
                }
                Err(VeteranReassemblyError::InvalidFrame) => {
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        diagnostics_for(ParserError::MalformedFrame),
                    )));
                }
            }
        }
    }
}

fn push_veteran_frame(
    frame: &VeteranFrame,
    monotonic_ms: MonotonicMillis,
    output: &mut Vec<SessionOutput>,
) {
    match VeteranTelemetry::decode(frame) {
        Ok(telemetry) => output.push(SessionOutput::Event(DeviceEvent::Telemetry(
            telemetry.to_delta(monotonic_ms),
        ))),
        Err(VeteranTelemetryError::FrameTooShort) => {
            output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                diagnostics_for(ParserError::MalformedFrame),
            )));
        }
    }
}

fn diagnostics_for(error: ParserError) -> ParserDiagnostics {
    let mut diagnostics = ParserDiagnostics::default();
    diagnostics.record_error(error);
    diagnostics
}

/// Type-level settings-write capability.
pub trait SupportsSettingsWrites: ProtocolModelSpec {
    /// Commands this model can write after stationary-state validation.
    const WRITE_CAPABILITIES: Capabilities;
}

/// Type-level benign-control capability.
pub trait SupportsBenignControls: ProtocolModelSpec {
    /// Commands this model can control through benign write paths.
    const CONTROL_CAPABILITIES: Capabilities;
}

/// Type-level dangerous-actuation capability.
pub trait SupportsDangerousActuation: ProtocolModelSpec {
    /// Commands this model can use for direct actuation.
    const ACTUATION_CAPABILITIES: Capabilities;
}

/// Type-level read-only model contract.
pub trait ReadOnlyModelSpec: SupportsReadRequests {
    /// Commands this read-only model session can schedule.
    const CAPABILITIES: Capabilities = Self::READ_CAPABILITIES;
}

impl<M: SupportsReadRequests> ReadOnlyModelSpec for M {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadOnlyCommandGate {
    SupportedRead(CommandKind),
    Unsupported(CommandKind),
}

fn gate_read_only_command<M: SupportsReadRequests>(command: DeviceCommand) -> ReadOnlyCommandGate {
    let kind = command.kind();
    if kind.safety_class() == SafetyClass::ReadOnly
        && M::READ_CAPABILITIES.supports_command_kind(kind)
    {
        ReadOnlyCommandGate::SupportedRead(kind)
    } else {
        ReadOnlyCommandGate::Unsupported(kind)
    }
}

/// NOSFET Aero read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NosfetAeroModel;

impl ProtocolModelSpec for NosfetAeroModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Nosfet;
    const MODEL: &'static str = "NOSFET Aero";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::VeteranLeaperkimNosfet;
}

impl SupportsReadRequests for NosfetAeroModel {
    const READ_CAPABILITIES: Capabilities = Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestBatteryInfo,
        CommandKind::RequestDiagnostics,
    ]);
    const SUBSCRIBE_CHANNEL: GattChannel = VETERAN_DATA_CHANNEL;
    type NotificationDecoder = VeteranNotificationDecoder;
}

/// Begode Falcon read-only model spec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeFalconModel;

impl ProtocolModelSpec for BegodeFalconModel {
    const MANUFACTURER: Manufacturer = Manufacturer::Begode;
    const MODEL: &'static str = "Begode Falcon";
    const PROTOCOL: ProtocolFamily = ProtocolFamily::BegodeGotway;
}

impl SupportsReadRequests for BegodeFalconModel {
    const READ_CAPABILITIES: Capabilities = Capabilities::from_supported_commands([
        CommandKind::RequestIdentity,
        CommandKind::RequestFirmwareInfo,
        CommandKind::RequestTelemetry,
        CommandKind::RequestBatteryInfo,
    ]);
    const SUBSCRIBE_CHANNEL: GattChannel = FALCON_WRITE_CHANNEL;
    type NotificationDecoder = NoopNotificationDecoder;
}

fn handle_read_only_session<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool>(
    connected: &mut bool,
    decoder: &mut M::NotificationDecoder,
    input: SessionInput<'_>,
    output: &mut Vec<SessionOutput>,
) {
    match input {
        SessionInput::LinkUp(info) => {
            *connected = true;
            decoder.reset();
            output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            output.push(SessionOutput::Transport(TransportAction::Subscribe {
                channel: M::SUBSCRIBE_CHANNEL,
            }));
        }
        SessionInput::LinkDown => {
            *connected = false;
            decoder.reset();
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
                decoder.handle_notification(bytes, monotonic_ms, output);
            }
        }
        SessionInput::Command(command) => match gate_read_only_command::<M>(command) {
            ReadOnlyCommandGate::SupportedRead(_) | ReadOnlyCommandGate::Unsupported(_) => {}
        },
    }
}

/// Generic read-only session shell for one statically-known model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadOnlySession<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool> {
    connected: bool,
    decoder: M::NotificationDecoder,
    model: PhantomData<fn() -> M>,
}

impl<M: ReadOnlyModelSpec, const ACCEPT_ANY_NOTIFICATION: bool> Default
    for ReadOnlySession<M, ACCEPT_ANY_NOTIFICATION>
{
    fn default() -> Self {
        Self {
            connected: false,
            decoder: M::NotificationDecoder::default(),
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
        handle_read_only_session::<M, ACCEPT_ANY_NOTIFICATION>(
            &mut self.connected,
            &mut self.decoder,
            input,
            output,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;
    use cutout_core::{LinkInfo, Measured, TelemetryDelta, TransportAction};

    const TEST_CHANNEL: GattChannel = GattChannel::from_bytes([0x11; 16]);

    struct TestModel;

    impl ProtocolModelSpec for TestModel {
        const MANUFACTURER: Manufacturer = Manufacturer::Nosfet;
        const MODEL: &'static str = "test";
        const PROTOCOL: ProtocolFamily = ProtocolFamily::VeteranLeaperkimNosfet;
    }

    impl SupportsReadRequests for TestModel {
        const READ_CAPABILITIES: Capabilities =
            Capabilities::from_supported_commands([CommandKind::RequestTelemetry]);
        const SUBSCRIBE_CHANNEL: GattChannel = TEST_CHANNEL;
        type NotificationDecoder = NoopNotificationDecoder;
    }

    fn live_aero_frame() -> [u8; 87] {
        hex_literal::hex!(
            "dc5a5c532a7c000000000000ab41001700000cff\
             000000000226021ca8f607801afa000080c80000\
             808080808080022880803080800e310e310e2f0e\
             2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
             310e2e9e05e3ad"
        )
    }

    fn telemetry_events(output: &[SessionOutput]) -> Vec<TelemetryDelta> {
        output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::Event(DeviceEvent::Telemetry(delta)) => Some(*delta),
                _ => None,
            })
            .collect()
    }

    fn live_aero_telemetry() -> TelemetryDelta {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(
            SessionInput::Notification {
                channel: VETERAN_DATA_CHANNEL,
                bytes: &live_aero_frame(),
                monotonic_ms: 42,
            },
            &mut output,
        );

        telemetry_events(&output)
            .into_iter()
            .next()
            .expect("live Aero notification emits telemetry")
    }

    #[test]
    fn shared_read_only_session_link_up_subscribes_profile_channel() {
        let mut connected = false;
        let mut decoder = NoopNotificationDecoder;
        let mut output = Vec::new();

        handle_read_only_session::<TestModel, false>(
            &mut connected,
            &mut decoder,
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
        let mut decoder = NoopNotificationDecoder;
        let mut output = Vec::new();

        handle_read_only_session::<TestModel, false>(
            &mut connected,
            &mut decoder,
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
        let mut decoder = NoopNotificationDecoder;
        let mut output = Vec::new();

        handle_read_only_session::<TestModel, false>(
            &mut connected,
            &mut decoder,
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
        assert_eq!(size_of::<ReadOnlySession<BegodeFalconModel, true>>(), 1);
        assert!(size_of::<ReadOnlySession<NosfetAeroModel, false>>() <= 272);
    }

    #[test]
    fn nosfet_aero_session_emits_voltage_from_live_fixture_notification() {
        assert_eq!(
            live_aero_telemetry().voltage_mv,
            Some(Measured::reported(108_760))
        );
    }

    #[test]
    fn nosfet_aero_session_emits_estimated_battery_percent_from_live_fixture_notification() {
        assert_eq!(
            live_aero_telemetry().battery_percent_estimated,
            Some(Measured::estimated(47))
        );
    }

    #[test]
    fn nosfet_aero_session_emits_fixed_header_telemetry_from_live_fixture_notification() {
        let telemetry = live_aero_telemetry();

        assert_eq!(telemetry.speed_mm_s, Some(Measured::reported(0)));
        assert_eq!(telemetry.motor_current_ma, Some(Measured::reported(0)));
        assert_eq!(
            telemetry.controller_temperature_mc,
            Some(Measured::reported(33_270))
        );
        assert_eq!(telemetry.pwm_permille, Some(Measured::reported(-1_000)));
        assert_eq!(
            telemetry.distance_mm,
            Some(Measured::reported(1_551_169_000))
        );
        assert_eq!(telemetry.pitch_mdeg, Some(Measured::reported(69_060)));
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

    #[test]
    fn model_specs_expose_read_only_operation_class() {
        assert_eq!(
            NosfetAeroModel::READ_OPERATION.safety_class(),
            SafetyClass::ReadOnly
        );
        assert_eq!(
            BegodeFalconModel::READ_OPERATION.safety_class(),
            SafetyClass::ReadOnly
        );
    }

    #[test]
    fn read_only_operation_traits_preserve_model_capabilities() {
        assert_eq!(
            <NosfetAeroModel as SupportsReadRequests>::READ_CAPABILITIES,
            ReadOnlySession::<NosfetAeroModel, false>::capabilities()
        );
        assert_eq!(
            <BegodeFalconModel as SupportsReadRequests>::READ_CAPABILITIES,
            ReadOnlySession::<BegodeFalconModel, true>::capabilities()
        );
    }

    #[test]
    fn write_and_actuation_operations_have_distinct_safety_classes() {
        assert_eq!(
            SettingsWriteOperation.safety_class(),
            SafetyClass::StationaryOnly
        );
        assert_eq!(
            BenignControlOperation.safety_class(),
            SafetyClass::BenignControl
        );
        assert_eq!(
            DangerousActuationOperation.safety_class(),
            SafetyClass::Actuation
        );
    }

    #[test]
    fn read_only_gate_accepts_supported_read_commands() {
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::RequestDiagnostics),
            ReadOnlyCommandGate::SupportedRead(CommandKind::RequestDiagnostics)
        );
        assert_eq!(
            gate_read_only_command::<BegodeFalconModel>(DeviceCommand::RequestIdentity),
            ReadOnlyCommandGate::SupportedRead(CommandKind::RequestIdentity)
        );
    }

    #[test]
    fn read_only_gate_rejects_unsupported_read_commands() {
        assert_eq!(
            gate_read_only_command::<BegodeFalconModel>(DeviceCommand::RequestDiagnostics),
            ReadOnlyCommandGate::Unsupported(CommandKind::RequestDiagnostics)
        );
    }

    #[test]
    fn read_only_gate_rejects_write_control_and_actuation_commands() {
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::RequestSettings),
            ReadOnlyCommandGate::Unsupported(CommandKind::RequestSettings)
        );
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::SetLights(
                cutout_core::LightState::On
            )),
            ReadOnlyCommandGate::Unsupported(CommandKind::SetLights)
        );
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::SoundHorn),
            ReadOnlyCommandGate::Unsupported(CommandKind::SoundHorn)
        );
        assert_eq!(
            gate_read_only_command::<NosfetAeroModel>(DeviceCommand::SetRawMotorCurrent {
                current_ma: 1
            }),
            ReadOnlyCommandGate::Unsupported(CommandKind::SetRawMotorCurrent)
        );
    }

    #[test]
    fn read_only_session_never_emits_transport_for_unsupported_commands() {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::SetRawMotorCurrent { current_ma: 1 }),
            &mut output,
        );

        assert!(
            output
                .iter()
                .all(|item| !matches!(item, SessionOutput::Transport(_)))
        );
    }
}
