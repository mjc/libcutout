//! Concrete `UniFFI` mobile binding surface for Cutout.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_core::{
    CommandKindDto, ControlRefusalReasonDto, DeviceCommandDto, ParserDiagnosticsDto,
    SessionInputDto, SessionOutputDto, TelemetrySnapshotDto, TransportActionDto,
};
use cutout_protocols::{
    ConcreteAeroReadOnlySession, ConcreteFalconProfileDto, ConcreteFalconReadOnlySession,
    ConcreteSessionErrorDto, ConcreteSessionStepResultDto, new_nosfet_aero_read_only_session,
    try_new_begode_falcon_read_only_session,
};

uniffi::setup_scaffolding!();

/// Mobile DTO command kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCommandDto {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request telemetry.
    RequestTelemetry,

    /// Request firmware information.
    RequestFirmwareInfo,

    /// Request battery information.
    RequestBatteryInfo,

    /// Request diagnostics.
    RequestDiagnostics,

    /// Sound a horn or alert.
    SoundHorn,
}

/// Mobile DTO input kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionInputKindDto {
    /// Link-up input.
    LinkUp,

    /// Device command input.
    Command,
}

/// Mobile DTO input.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionInputDto {
    /// Input kind.
    pub kind: MobileSessionInputKindDto,

    /// Monotonic timestamp in milliseconds.
    pub monotonic_ms: u64,

    /// Maximum write length, when known.
    pub max_write_len: Option<u16>,

    /// Command for command inputs.
    pub command: Option<MobileCommandDto>,
}

/// Mobile output kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionOutputKindDto {
    /// Subscribe transport action.
    Subscribe,

    /// Write transport action.
    Write,

    /// Non-transport event.
    Event,

    /// Disconnect transport action.
    Disconnect,
}

/// Mobile output DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionOutputDto {
    /// Output kind.
    pub kind: MobileSessionOutputKindDto,

    /// Transport channel bytes.
    pub channel: Vec<u8>,

    /// Transport payload bytes.
    pub bytes: Vec<u8>,
}

/// Mobile step-error kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileSessionStepErrorKindDto {
    /// Command was refused.
    CommandRefused,

    /// Falcon profile was not supported.
    UnsupportedFalconProfile,
}

/// Mobile session step error DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionStepErrorDto {
    /// Error kind.
    pub kind: MobileSessionStepErrorKindDto,

    /// Command associated with the error, if any.
    pub command: Option<MobileCommandDto>,

    /// Refusal reason, if any.
    pub reason: Option<String>,
}

/// Mobile result of one session step.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileSessionStepResultDto {
    /// Owned outputs from the step.
    pub outputs: Vec<MobileSessionOutputDto>,

    /// Stable error from the step, if any.
    pub error: Option<MobileSessionStepErrorDto>,
}

/// Mobile telemetry snapshot DTO.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileTelemetrySnapshotDto {
    /// Snapshot timestamp in monotonic milliseconds.
    pub at_ms: Option<u64>,

    /// Reported voltage in millivolts.
    pub voltage_mv: Option<i32>,

    /// Estimated battery percent.
    pub battery_percent_estimated: Option<u8>,
}

/// Mobile parser diagnostics DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileParserDiagnosticsDto {
    /// Malformed frame count.
    pub malformed_frames: u64,

    /// Bad checksum count.
    pub bad_checksums: u64,

    /// Oversized frame count.
    pub oversized_frames: u64,
}

/// Mobile Falcon construction profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileFalconProfileDto {
    /// Default known Falcon profile.
    Default,

    /// Deliberate unsupported sentinel used to keep binding errors typed.
    Unsupported,
}

/// Mobile Falcon construction error.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileSessionConstructorError {
    /// Requested Falcon profile is not supported.
    #[error("unsupported Falcon profile")]
    UnsupportedFalconProfile,
}

/// Mobile-facing wrapper for a NOSFET Aero read-only session.
#[derive(Debug, uniffi::Object)]
pub struct AeroReadOnlySession {
    inner: Mutex<ConcreteAeroReadOnlySession>,
}

#[uniffi::export]
impl AeroReadOnlySession {
    /// Creates a NOSFET Aero read-only session.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(new_nosfet_aero_read_only_session()),
        })
    }

    /// Drives one input and returns owned outputs plus any stable error DTO.
    pub fn ingest_checked(&self, input: MobileSessionInputDto) -> MobileSessionStepResultDto {
        let input = SessionInputDto::from(input);
        MobileSessionStepResultDto::from(self.lock_inner().ingest_checked(&input))
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    pub fn current_snapshot(&self) -> MobileTelemetrySnapshotDto {
        self.lock_inner().current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    pub fn diagnostics(&self) -> MobileParserDiagnosticsDto {
        self.lock_inner().diagnostics().into()
    }
}

impl AeroReadOnlySession {
    fn lock_inner(&self) -> MutexGuard<'_, ConcreteAeroReadOnlySession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for AeroReadOnlySession {
    fn default() -> Self {
        Self {
            inner: Mutex::new(new_nosfet_aero_read_only_session()),
        }
    }
}

impl From<MobileSessionInputDto> for SessionInputDto {
    fn from(input: MobileSessionInputDto) -> Self {
        match input.kind {
            MobileSessionInputKindDto::LinkUp => Self::LinkUp {
                monotonic_ms: input.monotonic_ms,
                max_write_len: input.max_write_len,
            },
            MobileSessionInputKindDto::Command => Self::Command(
                input
                    .command
                    .unwrap_or(MobileCommandDto::RequestTelemetry)
                    .into(),
            ),
        }
    }
}

impl From<MobileCommandDto> for DeviceCommandDto {
    fn from(command: MobileCommandDto) -> Self {
        match command {
            MobileCommandDto::RequestIdentity => Self::RequestIdentity,
            MobileCommandDto::RequestTelemetry => Self::RequestTelemetry,
            MobileCommandDto::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            MobileCommandDto::RequestBatteryInfo => Self::RequestBatteryInfo,
            MobileCommandDto::RequestDiagnostics => Self::RequestDiagnostics,
            MobileCommandDto::SoundHorn => Self::SoundHorn,
        }
    }
}

impl From<CommandKindDto> for MobileCommandDto {
    fn from(command: CommandKindDto) -> Self {
        match command {
            CommandKindDto::RequestIdentity => Self::RequestIdentity,
            CommandKindDto::RequestTelemetry => Self::RequestTelemetry,
            CommandKindDto::RequestFirmwareInfo => Self::RequestFirmwareInfo,
            CommandKindDto::RequestBatteryInfo => Self::RequestBatteryInfo,
            CommandKindDto::RequestDiagnostics
            | CommandKindDto::RequestSettings
            | CommandKindDto::SetLights
            | CommandKindDto::SetRawMotorCurrent => Self::RequestDiagnostics,
            CommandKindDto::SoundHorn => Self::SoundHorn,
        }
    }
}

impl From<SessionOutputDto> for MobileSessionOutputDto {
    fn from(output: SessionOutputDto) -> Self {
        match output {
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel }) => Self {
                kind: MobileSessionOutputKindDto::Subscribe,
                channel: channel.to_vec(),
                bytes: Vec::new(),
            },
            SessionOutputDto::Transport(TransportActionDto::Write { channel, bytes, .. }) => Self {
                kind: MobileSessionOutputKindDto::Write,
                channel: channel.to_vec(),
                bytes,
            },
            SessionOutputDto::Transport(TransportActionDto::Disconnect) => Self {
                kind: MobileSessionOutputKindDto::Disconnect,
                channel: Vec::new(),
                bytes: Vec::new(),
            },
            SessionOutputDto::Event(_) => Self {
                kind: MobileSessionOutputKindDto::Event,
                channel: Vec::new(),
                bytes: Vec::new(),
            },
        }
    }
}

impl From<ConcreteSessionStepResultDto> for MobileSessionStepResultDto {
    fn from(result: ConcreteSessionStepResultDto) -> Self {
        Self {
            outputs: result.outputs.into_iter().map(Into::into).collect(),
            error: result.error.map(Into::into),
        }
    }
}

impl From<ConcreteSessionErrorDto> for MobileSessionStepErrorDto {
    fn from(error: ConcreteSessionErrorDto) -> Self {
        match error {
            ConcreteSessionErrorDto::CommandRefused { refusal } => Self {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                command: Some(refusal.command.into()),
                reason: Some(control_refusal_reason_text(refusal.reason).to_owned()),
            },
            ConcreteSessionErrorDto::UnsupportedFalconProfile { .. } => Self {
                kind: MobileSessionStepErrorKindDto::UnsupportedFalconProfile,
                command: None,
                reason: None,
            },
        }
    }
}

impl From<TelemetrySnapshotDto> for MobileTelemetrySnapshotDto {
    fn from(snapshot: TelemetrySnapshotDto) -> Self {
        Self {
            at_ms: snapshot.at_ms,
            voltage_mv: snapshot.voltage_mv.map(|value| value.value),
            battery_percent_estimated: snapshot.battery_percent_estimated.map(|value| value.value),
        }
    }
}

impl From<ParserDiagnosticsDto> for MobileParserDiagnosticsDto {
    fn from(diagnostics: ParserDiagnosticsDto) -> Self {
        Self {
            malformed_frames: diagnostics.malformed_frames,
            bad_checksums: diagnostics.bad_checksums,
            oversized_frames: diagnostics.oversized_frames,
        }
    }
}

impl From<MobileFalconProfileDto> for ConcreteFalconProfileDto {
    fn from(profile: MobileFalconProfileDto) -> Self {
        match profile {
            MobileFalconProfileDto::Default => Self::Default,
            MobileFalconProfileDto::Unsupported => Self::Unsupported,
        }
    }
}

impl From<ConcreteSessionErrorDto> for MobileSessionConstructorError {
    fn from(error: ConcreteSessionErrorDto) -> Self {
        match error {
            ConcreteSessionErrorDto::CommandRefused { .. }
            | ConcreteSessionErrorDto::UnsupportedFalconProfile { .. } => {
                Self::UnsupportedFalconProfile
            }
        }
    }
}

fn control_refusal_reason_text(reason: ControlRefusalReasonDto) -> &'static str {
    match reason {
        ControlRefusalReasonDto::WrongSafetyClass => "wrong_safety_class",
        ControlRefusalReasonDto::MissingArm => "missing_arm",
        ControlRefusalReasonDto::WrongModel => "wrong_model",
        ControlRefusalReasonDto::ExpiredArm => "expired_arm",
        ControlRefusalReasonDto::CurrentLimitExceeded => "current_limit_exceeded",
        ControlRefusalReasonDto::UnsupportedCommand => "unsupported_command",
    }
}

/// Mobile-facing wrapper for a Begode Falcon read-only session.
#[derive(Debug, uniffi::Object)]
pub struct FalconReadOnlySession {
    inner: Mutex<ConcreteFalconReadOnlySession>,
}

#[uniffi::export]
impl FalconReadOnlySession {
    /// Creates a Begode Falcon read-only session with the default profile.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// default profile is unavailable.
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, MobileSessionConstructorError> {
        Self::with_profile(MobileFalconProfileDto::Default)
    }

    /// Creates a Begode Falcon read-only session with an explicit profile.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// selected profile is unavailable.
    #[uniffi::constructor]
    pub fn with_profile(
        profile: MobileFalconProfileDto,
    ) -> Result<Arc<Self>, MobileSessionConstructorError> {
        Ok(Arc::new(Self {
            inner: Mutex::new(try_new_begode_falcon_read_only_session(profile.into())?),
        }))
    }

    /// Drives one input and returns owned outputs plus any stable error DTO.
    pub fn ingest_checked(&self, input: MobileSessionInputDto) -> MobileSessionStepResultDto {
        let input = SessionInputDto::from(input);
        MobileSessionStepResultDto::from(self.lock_inner().ingest_checked(&input))
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    pub fn drain_outputs(&self) -> Vec<MobileSessionOutputDto> {
        self.lock_inner()
            .drain_outputs()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    pub fn current_snapshot(&self) -> MobileTelemetrySnapshotDto {
        self.lock_inner().current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    pub fn diagnostics(&self) -> MobileParserDiagnosticsDto {
        self.lock_inner().diagnostics().into()
    }
}

impl FalconReadOnlySession {
    fn lock_inner(&self) -> MutexGuard<'_, ConcreteFalconReadOnlySession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aero_wrapper_constructs_and_exposes_diagnostics() {
        let session = AeroReadOnlySession::new();

        assert_eq!(session.diagnostics().malformed_frames, 0);
    }

    #[test]
    fn falcon_wrapper_surfaces_unsupported_command_error() {
        let session = FalconReadOnlySession::new().expect("default profile should construct");

        let result = session.ingest_checked(MobileSessionInputDto {
            kind: MobileSessionInputKindDto::Command,
            monotonic_ms: 0,
            max_write_len: None,
            command: Some(MobileCommandDto::SoundHorn),
        });

        assert!(matches!(
            result.error,
            Some(MobileSessionStepErrorDto {
                kind: MobileSessionStepErrorKindDto::CommandRefused,
                ..
            })
        ));
    }

    #[test]
    fn falcon_wrapper_rejects_unsupported_profile() {
        let result = FalconReadOnlySession::with_profile(MobileFalconProfileDto::Unsupported);

        assert!(matches!(
            result,
            Err(MobileSessionConstructorError::UnsupportedFalconProfile)
        ));
    }
}
