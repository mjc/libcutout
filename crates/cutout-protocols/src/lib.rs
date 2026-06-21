#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

//! Protocol-family scaffolding for Cutout.

use arrayvec::ArrayVec;
use cutout_core::{
    Capabilities, CommandKind, DeviceCommand, DeviceEvent, GattChannel, PollRequest, PollingPlan,
    ProtocolSession, RequestPolicy, RequestQueue, RequestUrgency, SessionInput, SessionOutput,
    TransportAction, WriteMode, WritePayload, WritePayloadTooLong,
};
use thiserror::Error;

/// Placeholder write channel for NOSFET/Veteran-family sessions.
pub const AERO_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xA1; 16]);

/// Placeholder write channel for Begode-family sessions.
pub const FALCON_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xB1; 16]);

const DEFAULT_POLICY: RequestPolicy = RequestPolicy {
    timeout_ms: 1_000,
    max_retries: 1,
    min_interval_ms: 100,
};

const MAX_PROVISIONAL_REQUEST_LEN: usize = 24;

const AERO_POLL_PLAN: PollingPlan<5> = PollingPlan::new([
    PollRequest::new(
        CommandKind::RequestTelemetry,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestBatteryInfo,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestIdentity,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestFirmwareInfo,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestDiagnostics,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
]);

const FALCON_POLL_PLAN: PollingPlan<4> = PollingPlan::new([
    PollRequest::new(
        CommandKind::RequestTelemetry,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestIdentity,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestFirmwareInfo,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestBatteryInfo,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
]);

/// NOSFET/Veteran-family read-only probe identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AeroProbe {
    /// Request identity/model information.
    Identity,

    /// Request firmware or protocol version information.
    FirmwareInfo,

    /// Request live telemetry.
    Telemetry,

    /// Request battery or BMS information.
    BatteryInfo,

    /// Request diagnostic information.
    Diagnostics,
}

impl AeroProbe {
    /// Maps a generic command kind to an Aero/Veteran probe.
    #[must_use]
    pub const fn from_command_kind(kind: CommandKind) -> Option<Self> {
        match kind {
            CommandKind::RequestIdentity => Some(Self::Identity),
            CommandKind::RequestFirmwareInfo => Some(Self::FirmwareInfo),
            CommandKind::RequestTelemetry => Some(Self::Telemetry),
            CommandKind::RequestBatteryInfo => Some(Self::BatteryInfo),
            CommandKind::RequestDiagnostics => Some(Self::Diagnostics),
            CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => None,
        }
    }

    /// Returns the temporary placeholder label for this probe.
    #[must_use]
    pub const fn placeholder_label(self) -> &'static [u8] {
        match self {
            Self::Identity => b"aero:identity",
            Self::FirmwareInfo => b"aero:firmware",
            Self::Telemetry => b"aero:telemetry",
            Self::BatteryInfo => b"aero:battery",
            Self::Diagnostics => b"aero:diagnostics",
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Identity => CommandKind::RequestIdentity,
            Self::FirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::Telemetry => CommandKind::RequestTelemetry,
            Self::BatteryInfo => CommandKind::RequestBatteryInfo,
            Self::Diagnostics => CommandKind::RequestDiagnostics,
        }
    }
}

/// Begode-family read-only probe identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FalconProbe {
    /// Request identity/model information.
    Identity,

    /// Request firmware or protocol version information.
    FirmwareInfo,

    /// Request live telemetry.
    Telemetry,

    /// Request battery or BMS information.
    BatteryInfo,
}

impl FalconProbe {
    /// Maps a generic command kind to a Begode/Falcon probe.
    #[must_use]
    pub const fn from_command_kind(kind: CommandKind) -> Option<Self> {
        match kind {
            CommandKind::RequestIdentity => Some(Self::Identity),
            CommandKind::RequestFirmwareInfo => Some(Self::FirmwareInfo),
            CommandKind::RequestTelemetry => Some(Self::Telemetry),
            CommandKind::RequestBatteryInfo => Some(Self::BatteryInfo),
            CommandKind::RequestDiagnostics
            | CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => None,
        }
    }

    /// Returns the temporary placeholder label for this probe.
    #[must_use]
    pub const fn placeholder_label(self) -> &'static [u8] {
        match self {
            Self::Identity => b"falcon:identity",
            Self::FirmwareInfo => b"falcon:firmware",
            Self::Telemetry => b"falcon:telemetry",
            Self::BatteryInfo => b"falcon:battery",
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Identity => CommandKind::RequestIdentity,
            Self::FirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::Telemetry => CommandKind::RequestTelemetry,
            Self::BatteryInfo => CommandKind::RequestBatteryInfo,
        }
    }
}

/// Bounded encoded request payload plus correlation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRequest<P> {
    /// Family-specific probe represented by this request.
    pub probe: P,

    /// Generic command kind used for scheduler and response correlation.
    pub command: CommandKind,

    /// Bounded provisional request bytes.
    pub payload: ArrayVec<u8, MAX_PROVISIONAL_REQUEST_LEN>,

    /// GATT write mode required by this request.
    pub mode: WriteMode,
}

/// Provisional request encoder for NOSFET Aero/Veteran-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AeroRequestEncoder;

impl AeroRequestEncoder {
    /// Encodes a supported Aero/Veteran-family probe.
    #[must_use]
    pub fn encode(probe: AeroProbe) -> EncodedRequest<AeroProbe> {
        EncodedRequest {
            probe,
            command: probe.command_kind(),
            payload: provisional_payload(probe.placeholder_label()),
            mode: WriteMode::WithResponse,
        }
    }

    /// Encodes a generic command if it belongs to the Aero/Veteran probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<AeroProbe>> {
        AeroProbe::from_command_kind(kind).map(Self::encode)
    }
}

/// Provisional request encoder for Begode/Falcon-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FalconRequestEncoder;

impl FalconRequestEncoder {
    /// Encodes a supported Begode/Falcon-family probe.
    #[must_use]
    pub fn encode(probe: FalconProbe) -> EncodedRequest<FalconProbe> {
        EncodedRequest {
            probe,
            command: probe.command_kind(),
            payload: provisional_payload(probe.placeholder_label()),
            mode: WriteMode::WithResponse,
        }
    }

    /// Encodes a generic command if it belongs to the Begode/Falcon probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<FalconProbe>> {
        FalconProbe::from_command_kind(kind).map(Self::encode)
    }
}

/// Protocol device family used by capture-backed fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceFamily {
    /// NOSFET Aero or Veteran-family protocol.
    NosfetAero,

    /// Begode Falcon or Begode-family protocol.
    BegodeFalcon,
}

/// Family-specific request probe used by fixture records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolProbe {
    /// NOSFET/Veteran-family probe.
    Aero(AeroProbe),

    /// Begode/Falcon-family probe.
    Falcon(FalconProbe),
}

impl ProtocolProbe {
    /// Maps a device family and generic command kind into a protocol probe.
    ///
    /// # Errors
    ///
    /// Returns [`RequestFixtureError::UnsupportedCommand`] when the command is
    /// unsupported by the selected family.
    pub fn from_family_command(
        family: DeviceFamily,
        command: CommandKind,
    ) -> Result<Self, RequestFixtureError> {
        match family {
            DeviceFamily::NosfetAero => AeroProbe::from_command_kind(command)
                .map(Self::Aero)
                .ok_or(RequestFixtureError::UnsupportedCommand { family, command }),
            DeviceFamily::BegodeFalcon => FalconProbe::from_command_kind(command)
                .map(Self::Falcon)
                .ok_or(RequestFixtureError::UnsupportedCommand { family, command }),
        }
    }

    /// Returns the family that owns this probe.
    #[must_use]
    pub const fn family(self) -> DeviceFamily {
        match self {
            Self::Aero(_) => DeviceFamily::NosfetAero,
            Self::Falcon(_) => DeviceFamily::BegodeFalcon,
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Aero(probe) => probe.command_kind(),
            Self::Falcon(probe) => probe.command_kind(),
        }
    }
}

/// Optional service/characteristic channels observed for a fixture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FixtureChannels {
    /// Optional GATT service or endpoint group identifier.
    pub service: Option<GattChannel>,

    /// Optional GATT characteristic or write endpoint identifier.
    pub characteristic: Option<GattChannel>,
}

/// Provenance category for capture-backed request fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureProvenance {
    /// Observed from a Bluetooth capture.
    BluetoothCapture,

    /// Observed from an application trace.
    AppTrace,

    /// Taken from source-attributed vendor or protocol documentation.
    VendorDocumentation,
}

/// Whether request fixture bytes have been verified against real hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareVerification {
    /// Fixture bytes have not been verified against real Bluetooth hardware.
    Unverified,

    /// Fixture bytes have been verified against real Bluetooth hardware.
    VerifiedOnBluetooth,
}

/// Capture/spec-backed request fixture record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestFixture {
    /// Device family the fixture applies to.
    pub family: DeviceFamily,

    /// Family-specific probe encoded by this fixture.
    pub probe: ProtocolProbe,

    /// Generic command kind used for scheduler and response correlation.
    pub command: CommandKind,

    /// Transport write behavior observed for the request.
    pub mode: WriteMode,

    /// Bounded request bytes.
    pub bytes: WritePayload,

    /// Optional service/characteristic evidence.
    pub channels: FixtureChannels,

    /// Source category for the fixture evidence.
    pub provenance: FixtureProvenance,

    /// Hardware verification state for the fixture.
    pub hardware_verification: HardwareVerification,
}

impl RequestFixture {
    /// Creates a request fixture after validating family/probe and byte bounds.
    ///
    /// # Errors
    ///
    /// Returns [`RequestFixtureError::FamilyMismatch`] when the probe belongs
    /// to a different family, or [`RequestFixtureError::PayloadTooLong`] when
    /// the request bytes exceed the core transport write bound.
    pub fn new(
        family: DeviceFamily,
        probe: ProtocolProbe,
        mode: WriteMode,
        bytes: &[u8],
        channels: FixtureChannels,
        provenance: FixtureProvenance,
        hardware_verification: HardwareVerification,
    ) -> Result<Self, RequestFixtureError> {
        let probe_family = probe.family();
        if family != probe_family {
            return Err(RequestFixtureError::FamilyMismatch {
                family,
                probe_family,
            });
        }
        Ok(Self {
            family,
            probe,
            command: probe.command_kind(),
            mode,
            bytes: WritePayload::try_from_slice(bytes)
                .map_err(RequestFixtureError::PayloadTooLong)?,
            channels,
            provenance,
            hardware_verification,
        })
    }
}

/// Request fixture validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RequestFixtureError {
    /// Probe belongs to a different protocol family.
    #[error("fixture family {family:?} does not match probe family {probe_family:?}")]
    FamilyMismatch {
        /// Fixture device family.
        family: DeviceFamily,

        /// Family implied by the probe.
        probe_family: DeviceFamily,
    },

    /// Command is unsupported by a protocol family.
    #[error("command {command:?} is unsupported by fixture family {family:?}")]
    UnsupportedCommand {
        /// Device family requested for mapping.
        family: DeviceFamily,

        /// Unsupported command.
        command: CommandKind,
    },

    /// Request bytes exceed the transport payload bound.
    #[error(transparent)]
    PayloadTooLong(WritePayloadTooLong),
}

/// Initial read-only session shell for NOSFET Aero/Veteran-family devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AeroReadOnlySession {
    connected: bool,
    queue: RequestQueue<5>,
}

impl AeroReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub fn capabilities() -> Capabilities {
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
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            }
            SessionInput::LinkDown => {
                self.connected = false;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                if self.connected {
                    enqueue_aero_plan(&mut self.queue);
                    drain_aero_queue(&mut self.queue, output);
                }
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms,
                len: bytes.len(),
            })),
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

/// Initial read-only session shell for Begode Falcon devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FalconReadOnlySession {
    connected: bool,
    queue: RequestQueue<4>,
}

impl FalconReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub fn capabilities() -> Capabilities {
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
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            }
            SessionInput::LinkDown => {
                self.connected = false;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                if self.connected {
                    enqueue_falcon_plan(&mut self.queue);
                    drain_falcon_queue(&mut self.queue, output);
                }
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms,
                len: bytes.len(),
            })),
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

fn enqueue_aero_plan(queue: &mut RequestQueue<5>) {
    let _result = AERO_POLL_PLAN.enqueue_into(queue);
}

fn enqueue_falcon_plan(queue: &mut RequestQueue<4>) {
    let _result = FALCON_POLL_PLAN.enqueue_into(queue);
}

fn drain_aero_queue(queue: &mut RequestQueue<5>, output: &mut Vec<SessionOutput>) {
    while let Some(request) = queue.pop_next() {
        let Some(encoded) = AeroRequestEncoder::encode_command(request.key.command) else {
            continue;
        };
        let Ok(bytes) = WritePayload::try_from_slice(encoded.payload.as_slice()) else {
            continue;
        };
        output.push(SessionOutput::Transport(TransportAction::Write {
            channel: AERO_WRITE_CHANNEL,
            bytes,
            mode: encoded.mode,
        }));
    }
}

fn drain_falcon_queue(queue: &mut RequestQueue<4>, output: &mut Vec<SessionOutput>) {
    while let Some(request) = queue.pop_next() {
        let Some(encoded) = FalconRequestEncoder::encode_command(request.key.command) else {
            continue;
        };
        let Ok(bytes) = WritePayload::try_from_slice(encoded.payload.as_slice()) else {
            continue;
        };
        output.push(SessionOutput::Transport(TransportAction::Write {
            channel: FALCON_WRITE_CHANNEL,
            bytes,
            mode: encoded.mode,
        }));
    }
}

fn provisional_payload(bytes: &[u8]) -> ArrayVec<u8, MAX_PROVISIONAL_REQUEST_LEN> {
    let mut payload = ArrayVec::new();
    for byte in bytes {
        let pushed = payload.try_push(*byte);
        debug_assert!(pushed.is_ok());
    }
    payload
}

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-protocols"
}

#[cfg(test)]
mod tests {
    use super::crate_name;
    use cutout_core::{
        Capabilities, CommandKind, LinkInfo, ProtocolSession, SessionInput, SessionOutput,
        TransportAction, WriteMode,
    };

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-protocols");
    }

    #[test]
    fn aero_session_exposes_read_only_capabilities() {
        let capabilities = crate::AeroReadOnlySession::capabilities();

        assert_eq!(
            capabilities,
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
                CommandKind::RequestDiagnostics,
            ])
        );
    }

    #[test]
    fn falcon_session_exposes_read_only_capabilities() {
        let capabilities = crate::FalconReadOnlySession::capabilities();

        assert_eq!(
            capabilities,
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
            ])
        );
    }

    #[test]
    fn aero_probe_mapping_accepts_supported_read_only_commands() {
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestIdentity),
            Some(crate::AeroProbe::Identity)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestFirmwareInfo),
            Some(crate::AeroProbe::FirmwareInfo)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestTelemetry),
            Some(crate::AeroProbe::Telemetry)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestBatteryInfo),
            Some(crate::AeroProbe::BatteryInfo)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestDiagnostics),
            Some(crate::AeroProbe::Diagnostics)
        );
    }

    #[test]
    fn falcon_probe_mapping_rejects_unsupported_read_only_commands() {
        assert_eq!(
            crate::FalconProbe::from_command_kind(CommandKind::RequestDiagnostics),
            None
        );
        assert_eq!(
            crate::FalconProbe::from_command_kind(CommandKind::RequestSettings),
            None
        );
    }

    #[test]
    fn probe_placeholder_labels_are_stable() {
        assert_eq!(
            crate::AeroProbe::Identity.placeholder_label(),
            b"aero:identity"
        );
        assert_eq!(
            crate::AeroProbe::Diagnostics.placeholder_label(),
            b"aero:diagnostics"
        );
        assert_eq!(
            crate::FalconProbe::FirmwareInfo.placeholder_label(),
            b"falcon:firmware"
        );
        assert_eq!(
            crate::FalconProbe::BatteryInfo.placeholder_label(),
            b"falcon:battery"
        );
    }

    #[test]
    fn aero_encoder_preserves_probe_command_write_mode_and_payload() {
        let request = crate::AeroRequestEncoder::encode(crate::AeroProbe::FirmwareInfo);

        assert_eq!(request.probe, crate::AeroProbe::FirmwareInfo);
        assert_eq!(request.command, CommandKind::RequestFirmwareInfo);
        assert_eq!(request.mode, WriteMode::WithResponse);
        assert_eq!(request.payload.as_slice(), b"aero:firmware");
    }

    #[test]
    fn falcon_encoder_preserves_probe_command_write_mode_and_payload() {
        let request = crate::FalconRequestEncoder::encode(crate::FalconProbe::BatteryInfo);

        assert_eq!(request.probe, crate::FalconProbe::BatteryInfo);
        assert_eq!(request.command, CommandKind::RequestBatteryInfo);
        assert_eq!(request.mode, WriteMode::WithResponse);
        assert_eq!(request.payload.as_slice(), b"falcon:battery");
    }

    #[test]
    fn aero_encoder_rejects_unsupported_command_family() {
        assert_eq!(
            crate::AeroRequestEncoder::encode_command(CommandKind::RequestSettings),
            None
        );
    }

    #[test]
    fn falcon_encoder_rejects_unsupported_diagnostics_probe() {
        assert_eq!(
            crate::FalconRequestEncoder::encode_command(CommandKind::RequestDiagnostics),
            None
        );
    }

    #[test]
    fn request_fixture_preserves_metadata_and_bounded_payload() {
        let fixture = crate::RequestFixture::new(
            crate::DeviceFamily::NosfetAero,
            crate::ProtocolProbe::Aero(crate::AeroProbe::Identity),
            WriteMode::WithResponse,
            b"\xdc\x5a\x5c",
            crate::FixtureChannels {
                service: Some(crate::AERO_WRITE_CHANNEL),
                characteristic: Some(crate::AERO_WRITE_CHANNEL),
            },
            crate::FixtureProvenance::BluetoothCapture,
            crate::HardwareVerification::VerifiedOnBluetooth,
        )
        .expect("fixture is valid");

        assert_eq!(fixture.family, crate::DeviceFamily::NosfetAero);
        assert_eq!(fixture.command, CommandKind::RequestIdentity);
        assert_eq!(fixture.bytes.as_slice(), b"\xdc\x5a\x5c");
        assert_eq!(
            fixture.provenance,
            crate::FixtureProvenance::BluetoothCapture
        );
        assert_eq!(
            fixture.hardware_verification,
            crate::HardwareVerification::VerifiedOnBluetooth
        );
    }

    #[test]
    fn request_fixture_rejects_oversized_request_bytes() {
        let bytes = vec![0; cutout_core::MAX_TRANSPORT_WRITE_LEN + 1];

        assert_eq!(
            crate::RequestFixture::new(
                crate::DeviceFamily::NosfetAero,
                crate::ProtocolProbe::Aero(crate::AeroProbe::Telemetry),
                WriteMode::WithResponse,
                &bytes,
                crate::FixtureChannels::default(),
                crate::FixtureProvenance::VendorDocumentation,
                crate::HardwareVerification::Unverified,
            ),
            Err(crate::RequestFixtureError::PayloadTooLong(
                cutout_core::WritePayloadTooLong {
                    len: cutout_core::MAX_TRANSPORT_WRITE_LEN + 1,
                    max: cutout_core::MAX_TRANSPORT_WRITE_LEN,
                }
            ))
        );
    }

    #[test]
    fn request_fixture_rejects_probe_from_wrong_family() {
        assert_eq!(
            crate::RequestFixture::new(
                crate::DeviceFamily::BegodeFalcon,
                crate::ProtocolProbe::Aero(crate::AeroProbe::Diagnostics),
                WriteMode::WithResponse,
                b"probe",
                crate::FixtureChannels::default(),
                crate::FixtureProvenance::AppTrace,
                crate::HardwareVerification::Unverified,
            ),
            Err(crate::RequestFixtureError::FamilyMismatch {
                family: crate::DeviceFamily::BegodeFalcon,
                probe_family: crate::DeviceFamily::NosfetAero,
            })
        );
    }

    #[test]
    fn protocol_probe_rejects_unsupported_family_command() {
        assert_eq!(
            crate::ProtocolProbe::from_family_command(
                crate::DeviceFamily::BegodeFalcon,
                CommandKind::RequestDiagnostics,
            ),
            Err(crate::RequestFixtureError::UnsupportedCommand {
                family: crate::DeviceFamily::BegodeFalcon,
                command: CommandKind::RequestDiagnostics,
            })
        );
    }

    #[test]
    fn aero_session_does_not_poll_while_disconnected() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(SessionInput::Tick { monotonic_ms: 10 }, &mut output);

        assert!(transport_writes(&output).is_empty());
    }

    #[test]
    fn aero_session_emits_polls_in_scheduler_order_after_link_up() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert_eq!(
            transport_writes(&output),
            vec![
                (crate::AERO_WRITE_CHANNEL, b"aero:identity".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:firmware".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:telemetry".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:battery".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:diagnostics".to_vec()),
            ]
        );
    }

    #[test]
    fn falcon_session_emits_polls_in_scheduler_order_after_link_up() {
        let mut session = crate::FalconReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(20),
            }),
            &mut output,
        );
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert_eq!(
            transport_writes(&output),
            vec![
                (crate::FALCON_WRITE_CHANNEL, b"falcon:identity".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:firmware".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:telemetry".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:battery".to_vec()),
            ]
        );
    }

    #[test]
    fn aero_session_clears_queued_polls_on_link_down() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(SessionInput::LinkDown, &mut output);
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert!(transport_writes(&output).is_empty());
    }

    fn transport_writes(output: &[SessionOutput]) -> Vec<(cutout_core::GattChannel, Vec<u8>)> {
        output
            .iter()
            .filter_map(|item| {
                let SessionOutput::Transport(TransportAction::Write {
                    channel,
                    bytes,
                    mode,
                }) = item
                else {
                    return None;
                };
                assert_eq!(*mode, WriteMode::WithResponse);
                Some((*channel, bytes.as_slice().to_vec()))
            })
            .collect()
    }
}
