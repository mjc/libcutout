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
    TransportAction, WriteMode, WritePayload,
};

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
