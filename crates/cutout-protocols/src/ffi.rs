use cutout_core::{
    Capabilities, ControlRefusal, ControlRefusalDto, ControlRefusalReason, DeviceCommand,
    HostSession, ParserDiagnosticsDto, SessionEventDto, SessionInputDto, SessionOutputDto,
    TelemetrySnapshotDto,
};

use crate::{BegodeFalconModel, NosfetAeroModel, ReadOnlySession};

type AeroReadOnlyHost = HostSession<ReadOnlySession<NosfetAeroModel, false>>;
type FalconReadOnlyHost = HostSession<ReadOnlySession<BegodeFalconModel, true>>;

/// Owned result of one concrete mobile session step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcreteSessionStepResultDto {
    /// Owned outputs emitted by the wrapped session.
    pub outputs: Vec<SessionOutputDto>,

    /// Stable error value surfaced from outputs or construction checks.
    pub error: Option<ConcreteSessionErrorDto>,
}

/// Stable concrete mobile-wrapper error DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcreteSessionErrorDto {
    /// A command was refused by the read-only protocol shell.
    CommandRefused {
        /// Refusal details from the core domain model.
        refusal: ControlRefusalDto,
    },

    /// The requested Falcon construction profile is not supported yet.
    UnsupportedFalconProfile {
        /// Unsupported Falcon construction profile.
        profile: ConcreteFalconProfileDto,
    },
}

/// Concrete Falcon construction profile for mobile bindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcreteFalconProfileDto {
    /// Default known Falcon profile.
    Default,

    /// Deliberate unsupported sentinel used to keep binding errors typed.
    Unsupported,
}

/// Concrete mobile-binding read-only session wrapper for NOSFET Aero.
#[derive(Clone, Debug)]
pub struct ConcreteAeroReadOnlySession {
    host: AeroReadOnlyHost,
}

impl ConcreteAeroReadOnlySession {
    /// Creates a read-only session wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: HostSession::new(ReadOnlySession::<NosfetAeroModel, false>::default()),
        }
    }

    /// Drives one owned DTO input through the wrapped protocol reactor.
    pub fn ingest(&mut self, input: &SessionInputDto) {
        self.host.ingest(input.as_session_input());
    }

    /// Drives one DTO input and returns owned outputs plus any stable error DTO.
    #[must_use]
    pub fn ingest_checked(&mut self, input: &SessionInputDto) -> ConcreteSessionStepResultDto {
        self.ingest(input);
        checked_drain_outputs(
            &mut self.host,
            input,
            ReadOnlySession::<NosfetAeroModel, false>::capabilities(),
        )
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutputDto> {
        drain_host_outputs(&mut self.host)
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    #[must_use]
    pub fn current_snapshot(&self) -> TelemetrySnapshotDto {
        self.host.current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    #[must_use]
    pub fn diagnostics(&self) -> ParserDiagnosticsDto {
        self.host.diagnostics().into()
    }
}

impl Default for ConcreteAeroReadOnlySession {
    fn default() -> Self {
        Self::new()
    }
}

/// Concrete mobile-binding read-only session wrapper for Begode Falcon.
#[derive(Clone, Debug)]
pub struct ConcreteFalconReadOnlySession {
    host: FalconReadOnlyHost,
}

impl ConcreteFalconReadOnlySession {
    /// Creates a read-only session wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: HostSession::new(ReadOnlySession::<BegodeFalconModel, true>::default()),
        }
    }

    /// Creates a read-only session wrapper for a selected Falcon profile.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// selected profile is not supported by the concrete wrapper.
    pub fn try_new(profile: ConcreteFalconProfileDto) -> Result<Self, ConcreteSessionErrorDto> {
        match profile {
            ConcreteFalconProfileDto::Default => Ok(Self::new()),
            ConcreteFalconProfileDto::Unsupported => {
                Err(ConcreteSessionErrorDto::UnsupportedFalconProfile { profile })
            }
        }
    }

    /// Drives one owned DTO input through the wrapped protocol reactor.
    pub fn ingest(&mut self, input: &SessionInputDto) {
        self.host.ingest(input.as_session_input());
    }

    /// Drives one DTO input and returns owned outputs plus any stable error DTO.
    #[must_use]
    pub fn ingest_checked(&mut self, input: &SessionInputDto) -> ConcreteSessionStepResultDto {
        self.ingest(input);
        checked_drain_outputs(
            &mut self.host,
            input,
            ReadOnlySession::<BegodeFalconModel, true>::capabilities(),
        )
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutputDto> {
        drain_host_outputs(&mut self.host)
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    #[must_use]
    pub fn current_snapshot(&self) -> TelemetrySnapshotDto {
        self.host.current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    #[must_use]
    pub fn diagnostics(&self) -> ParserDiagnosticsDto {
        self.host.diagnostics().into()
    }
}

impl Default for ConcreteFalconReadOnlySession {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates a NOSFET Aero read-only session wrapper.
#[must_use]
pub fn new_nosfet_aero_read_only_session() -> ConcreteAeroReadOnlySession {
    ConcreteAeroReadOnlySession {
        host: HostSession::new(ReadOnlySession::<NosfetAeroModel, false>::default()),
    }
}

/// Creates a Begode Falcon read-only session wrapper.
#[must_use]
pub fn new_begode_falcon_read_only_session() -> ConcreteFalconReadOnlySession {
    ConcreteFalconReadOnlySession {
        host: HostSession::new(ReadOnlySession::<BegodeFalconModel, true>::default()),
    }
}

/// Creates a Begode Falcon read-only session wrapper for a selected profile.
///
/// # Errors
///
/// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
/// selected profile is not supported by the concrete wrapper.
pub fn try_new_begode_falcon_read_only_session(
    profile: ConcreteFalconProfileDto,
) -> Result<ConcreteFalconReadOnlySession, ConcreteSessionErrorDto> {
    ConcreteFalconReadOnlySession::try_new(profile)
}

fn checked_drain_outputs<S>(
    host: &mut HostSession<S>,
    input: &SessionInputDto,
    capabilities: Capabilities,
) -> ConcreteSessionStepResultDto
where
    S: cutout_core::ProtocolSession,
{
    let outputs = drain_host_outputs(host);
    ConcreteSessionStepResultDto {
        error: first_error(input, capabilities, &outputs),
        outputs,
    }
}

fn first_error(
    input: &SessionInputDto,
    capabilities: Capabilities,
    outputs: &[SessionOutputDto],
) -> Option<ConcreteSessionErrorDto> {
    outputs.iter().find_map(output_error).or_else(|| {
        input_command_refusal(input, capabilities)
            .map(|refusal| ConcreteSessionErrorDto::CommandRefused { refusal })
    })
}

fn output_error(output: &SessionOutputDto) -> Option<ConcreteSessionErrorDto> {
    match output {
        SessionOutputDto::Event(SessionEventDto::ControlRefusal(refusal)) => {
            Some(ConcreteSessionErrorDto::CommandRefused { refusal: *refusal })
        }
        SessionOutputDto::Transport(_)
        | SessionOutputDto::ReadOnly(_)
        | SessionOutputDto::Event(_)
        | SessionOutputDto::NotificationIngest(_) => None,
    }
}

fn input_command_refusal(
    input: &SessionInputDto,
    capabilities: Capabilities,
) -> Option<ControlRefusalDto> {
    let SessionInputDto::Command(command) = input else {
        return None;
    };
    let command = DeviceCommand::from(*command);
    let kind = command.kind();
    (!capabilities.supports_command_kind(kind)).then(|| {
        ControlRefusalDto::from(ControlRefusal {
            command: kind,
            safety_class: command.safety_class(),
            reason: ControlRefusalReason::UnsupportedCommand,
        })
    })
}

fn drain_host_outputs<S>(host: &mut HostSession<S>) -> Vec<SessionOutputDto>
where
    S: cutout_core::ProtocolSession,
{
    host.drain_outputs().into_iter().map(Into::into).collect()
}

#[cfg(test)]
mod tests {
    use cutout_core::{
        CommandKindDto, ControlRefusalDto, ControlRefusalReasonDto, DeviceCommandDto, LinkInfo,
        MonotonicMillisDto, MonotonicTimestamp, ParserDiagnosticCountDto, SafetyClassDto,
        SessionEventDto, SessionInputDto, SessionOutputDto, TransportActionDto,
        TransportWriteLimit, TransportWriteLimitDto,
    };

    use crate::{BEGODE_DATA_CHANNEL, VETERAN_DATA_CHANNEL};

    use super::{
        ConcreteFalconProfileDto, ConcreteSessionErrorDto, new_begode_falcon_read_only_session,
        new_nosfet_aero_read_only_session, try_new_begode_falcon_read_only_session,
    };

    const fn ms(value: u64) -> MonotonicMillisDto {
        MonotonicMillisDto {
            milliseconds: value,
        }
    }

    const fn write_len(value: u16) -> TransportWriteLimit {
        TransportWriteLimit::from_bytes(value)
    }

    const fn write_len_dto(value: u16) -> TransportWriteLimitDto {
        TransportWriteLimitDto { bytes: value }
    }

    #[test]
    fn concrete_aero_session_drives_link_up_and_drains_owned_outputs() {
        let mut session = new_nosfet_aero_read_only_session();

        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });

        assert!(session.drain_outputs().iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel })
                if *channel == VETERAN_DATA_CHANNEL.as_bytes()
        )));
    }

    #[test]
    fn concrete_falcon_session_maps_command_dto_to_write_output() {
        let mut session = new_begode_falcon_read_only_session();
        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });
        let _ = session.drain_outputs();

        session.ingest(&SessionInputDto::Command(DeviceCommandDto::RequestIdentity));

        assert!(session.drain_outputs().iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { channel, bytes, .. })
                if *channel == BEGODE_DATA_CHANNEL.as_bytes() && bytes == b"N"
        )));
    }

    #[test]
    fn checked_ingest_surfaces_unsupported_command_as_error_dto() {
        let mut session = new_begode_falcon_read_only_session();
        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });
        let _ = session.drain_outputs();

        let result = session.ingest_checked(&SessionInputDto::Command(DeviceCommandDto::SoundHorn));

        let expected_refusal = ControlRefusalDto {
            command: CommandKindDto::SoundHorn,
            safety_class: SafetyClassDto::BenignControl,
            reason: ControlRefusalReasonDto::UnsupportedCommand,
        };
        assert_eq!(
            result.error,
            Some(ConcreteSessionErrorDto::CommandRefused {
                refusal: expected_refusal
            })
        );
        assert!(result.outputs.iter().all(|output| !matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { .. })
        )));
    }

    #[test]
    fn falcon_profile_constructor_rejects_unsupported_profile_with_error_dto() {
        assert_eq!(
            try_new_begode_falcon_read_only_session(ConcreteFalconProfileDto::Unsupported)
                .expect_err("unsupported profile should return typed error"),
            ConcreteSessionErrorDto::UnsupportedFalconProfile {
                profile: ConcreteFalconProfileDto::Unsupported
            }
        );
    }

    #[test]
    fn falcon_profile_constructor_accepts_default_profile() {
        let mut session =
            try_new_begode_falcon_read_only_session(ConcreteFalconProfileDto::Default)
                .expect("default Falcon profile should construct");

        let result = session.ingest_checked(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel })
                if *channel == BEGODE_DATA_CHANNEL.as_bytes()
        )));
    }

    #[test]
    fn concrete_session_exposes_snapshot_and_diagnostics_dtos() {
        let mut session = new_begode_falcon_read_only_session();
        let channel = BEGODE_DATA_CHANNEL.as_bytes();
        let mut malformed = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
        malformed[20] = 0;

        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });
        let _ = session.drain_outputs();

        session.ingest(&SessionInputDto::Notification {
            channel,
            bytes: malformed.to_vec(),
            monotonic_ms: ms(42),
        });

        assert_eq!(session.current_snapshot().at_ms, None);
        assert_eq!(
            session.diagnostics().malformed_frames,
            ParserDiagnosticCountDto { count: 1 }
        );
        assert!(session.drain_outputs().iter().any(|output| {
            matches!(
                output,
                SessionOutputDto::Event(SessionEventDto::DiagnosticError(_))
            )
        }));
    }

    #[test]
    fn concrete_session_accepts_core_link_info_roundtrip_inputs() {
        let mut session = new_begode_falcon_read_only_session();
        let link = LinkInfo {
            monotonic_ms: MonotonicTimestamp::new(7),
            max_write_len: Some(write_len(20)),
        };

        session.ingest(&SessionInputDto::from(cutout_core::SessionInput::LinkUp(
            link,
        )));

        assert!(!session.drain_outputs().is_empty());
    }
}
