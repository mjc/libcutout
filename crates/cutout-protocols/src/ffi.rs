use cutout_core::{
    HostSession, ParserDiagnosticsDto, SessionInputDto, SessionOutputDto, TelemetrySnapshotDto,
};

use crate::{BegodeFalconModel, NosfetAeroModel, ReadOnlySession};

type AeroReadOnlyHost = HostSession<ReadOnlySession<NosfetAeroModel, false>>;
type FalconReadOnlyHost = HostSession<ReadOnlySession<BegodeFalconModel, true>>;

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

    /// Drives one owned DTO input through the wrapped protocol reactor.
    pub fn ingest(&mut self, input: &SessionInputDto) {
        self.host.ingest(input.as_session_input());
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

fn drain_host_outputs<S>(host: &mut HostSession<S>) -> Vec<SessionOutputDto>
where
    S: cutout_core::ProtocolSession,
{
    host.drain_outputs().into_iter().map(Into::into).collect()
}

#[cfg(test)]
mod tests {
    use cutout_core::{
        DeviceCommandDto, LinkInfo, SessionEventDto, SessionInputDto, SessionOutputDto,
        TransportActionDto,
    };

    use crate::{BEGODE_DATA_CHANNEL, VETERAN_DATA_CHANNEL};

    use super::{new_begode_falcon_read_only_session, new_nosfet_aero_read_only_session};

    #[test]
    fn concrete_aero_session_drives_link_up_and_drains_owned_outputs() {
        let mut session = new_nosfet_aero_read_only_session();

        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: 1,
            max_write_len: Some(185),
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
            monotonic_ms: 1,
            max_write_len: Some(185),
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
    fn concrete_session_exposes_snapshot_and_diagnostics_dtos() {
        let mut session = new_begode_falcon_read_only_session();
        let channel = BEGODE_DATA_CHANNEL.as_bytes();
        let mut malformed = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
        malformed[20] = 0;

        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: 1,
            max_write_len: Some(185),
        });
        let _ = session.drain_outputs();

        session.ingest(&SessionInputDto::Notification {
            channel,
            bytes: malformed.to_vec(),
            monotonic_ms: 42,
        });

        assert_eq!(session.current_snapshot().at_ms, None);
        assert!(session.diagnostics().malformed_frames > 0);
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
            monotonic_ms: 7,
            max_write_len: Some(20),
        };

        session.ingest(&SessionInputDto::from(cutout_core::SessionInput::LinkUp(
            link,
        )));

        assert!(!session.drain_outputs().is_empty());
    }
}
