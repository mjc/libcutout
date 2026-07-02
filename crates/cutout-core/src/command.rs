//! Device command, safety classification, and control refusal types.

use crate::{Duration, MonotonicTimestamp, PhaseCurrent};

/// Command requested by the host application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceCommand {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request a telemetry update.
    RequestTelemetry,

    /// Request firmware or protocol version information.
    RequestFirmwareInfo,

    /// Request battery or BMS information.
    RequestBatteryInfo,

    /// Request device diagnostics.
    RequestDiagnostics,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights(LightState),

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current in milliamps.
    SetRawMotorCurrent {
        /// Target motor/phase current in milliamps.
        current: PhaseCurrent,
    },
}

impl DeviceCommand {
    /// Returns the stable command kind, excluding command payload values.
    #[must_use]
    pub const fn kind(self) -> CommandKind {
        match self {
            Self::RequestIdentity => CommandKind::RequestIdentity,
            Self::RequestTelemetry => CommandKind::RequestTelemetry,
            Self::RequestFirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::RequestBatteryInfo => CommandKind::RequestBatteryInfo,
            Self::RequestDiagnostics => CommandKind::RequestDiagnostics,
            Self::RequestSettings => CommandKind::RequestSettings,
            Self::SetLights(_) => CommandKind::SetLights,
            Self::SoundHorn => CommandKind::SoundHorn,
            Self::SetRawMotorCurrent { .. } => CommandKind::SetRawMotorCurrent,
        }
    }

    /// Returns the safety class for this command.
    #[must_use]
    pub const fn safety_class(self) -> SafetyClass {
        self.kind().safety_class()
    }

    /// Returns command metadata.
    #[must_use]
    pub const fn metadata(self) -> CommandMetadata {
        CommandMetadata {
            kind: self.kind(),
            safety_class: self.safety_class(),
        }
    }
}

/// Device light state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightState {
    /// Lights off.
    Off,

    /// Lights on.
    On,
}

/// Stable command discriminator, excluding command payload values.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CommandKind {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request a telemetry update.
    RequestTelemetry,

    /// Request firmware or protocol version information.
    RequestFirmwareInfo,

    /// Request battery or BMS information.
    RequestBatteryInfo,

    /// Request device diagnostics.
    RequestDiagnostics,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights,

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current.
    SetRawMotorCurrent,
}

impl CommandKind {
    /// Returns the safety class for this command kind.
    #[must_use]
    pub const fn safety_class(self) -> SafetyClass {
        match self {
            Self::RequestIdentity
            | Self::RequestTelemetry
            | Self::RequestFirmwareInfo
            | Self::RequestBatteryInfo
            | Self::RequestDiagnostics
            | Self::RequestSettings => SafetyClass::ReadOnly,
            Self::SetLights | Self::SoundHorn => SafetyClass::BenignControl,
            Self::SetRawMotorCurrent => SafetyClass::Actuation,
        }
    }
}

/// Safety class for a device command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyClass {
    /// Read-only request with no state change expected.
    ReadOnly,

    /// Benign control such as lights or horn.
    BenignControl,

    /// Setting that should only be changed while stationary.
    StationaryOnly,

    /// Direct actuation or motion-affecting control.
    Actuation,

    /// Firmware update or firmware mutation operation.
    Firmware,
}

/// Command metadata available before transport writes are generated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandMetadata {
    /// Stable command kind.
    pub kind: CommandKind,

    /// Safety class for this command.
    pub safety_class: SafetyClass,
}

/// Reason a command is unavailable in the current context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedReason {
    /// The command kind is not reported as supported.
    CommandNotSupported(CommandKind),
}

/// Short-lived authorization token for dangerous actuation commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DangerousActuationArm {
    /// Model this token was issued for.
    pub model: &'static str,

    /// Monotonic expiry time in milliseconds.
    pub expires_at_ms: MonotonicTimestamp,
}

/// Dangerous actuation policy for a single model/session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DangerousActuationPolicy {
    /// Model this policy allows.
    pub model: &'static str,

    /// Maximum absolute motor/phase current allowed by this policy.
    pub max_current: PhaseCurrent,

    /// Duration of newly issued arming tokens.
    pub arm_duration: Duration,
}

impl DangerousActuationPolicy {
    /// Creates an expiring arm token for this policy's model.
    #[must_use]
    pub const fn arm(self, monotonic_ms: MonotonicTimestamp) -> DangerousActuationArm {
        DangerousActuationArm {
            model: self.model,
            expires_at_ms: monotonic_ms.saturating_add_duration(self.arm_duration),
        }
    }

    /// Authorizes a dangerous actuation command if the policy and token allow it.
    ///
    /// # Errors
    ///
    /// Returns [`DangerousActuationRefusal`] when the command is not dangerous
    /// actuation, the token is missing/expired/wrong-model, or the requested
    /// current exceeds this policy's absolute limit.
    pub const fn authorize(
        self,
        command: DeviceCommand,
        monotonic_ms: MonotonicTimestamp,
        arm: Option<DangerousActuationArm>,
    ) -> Result<CommandMetadata, DangerousActuationRefusal> {
        if !matches!(command.safety_class(), SafetyClass::Actuation) {
            return Err(DangerousActuationRefusal::WrongSafetyClass);
        }

        let Some(arm) = arm else {
            return Err(DangerousActuationRefusal::MissingArm);
        };

        if !str_eq(arm.model, self.model) {
            return Err(DangerousActuationRefusal::WrongModel);
        }
        if monotonic_ms.get() > arm.expires_at_ms.get() {
            return Err(DangerousActuationRefusal::ExpiredArm);
        }
        if let DeviceCommand::SetRawMotorCurrent { current } = command
            && current.as_milliamps().saturating_abs() > self.max_current.as_milliamps()
        {
            return Err(DangerousActuationRefusal::CurrentLimitExceeded);
        }

        Ok(command.metadata())
    }
}

/// Refusal reason for dangerous actuation authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DangerousActuationRefusal {
    /// Command is not classified as dangerous actuation.
    WrongSafetyClass,

    /// No arm token was supplied.
    MissingArm,

    /// Arm token was issued for another model.
    WrongModel,

    /// Arm token has expired.
    ExpiredArm,

    /// Requested current exceeds the policy limit.
    CurrentLimitExceeded,
}

/// Host-facing control refusal details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlRefusal {
    /// Command that was refused.
    pub command: CommandKind,

    /// Safety class of the refused command.
    pub safety_class: SafetyClass,

    /// Refusal reason.
    pub reason: ControlRefusalReason,
}

/// Reason a control command was refused before transport writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRefusalReason {
    /// Command is not classified for this control shell.
    WrongSafetyClass,

    /// No required arming token was supplied.
    MissingArm,

    /// Arming token was issued for another model.
    WrongModel,

    /// Arming token has expired.
    ExpiredArm,

    /// Requested value exceeds the configured current limit.
    CurrentLimitExceeded,

    /// Command is not supported by this model/session.
    UnsupportedCommand,

    /// Command was authorized, but this shell has no encoder yet.
    ActuationEncoderUnavailable,
}

impl From<DangerousActuationRefusal> for ControlRefusalReason {
    fn from(value: DangerousActuationRefusal) -> Self {
        match value {
            DangerousActuationRefusal::WrongSafetyClass => Self::WrongSafetyClass,
            DangerousActuationRefusal::MissingArm => Self::MissingArm,
            DangerousActuationRefusal::WrongModel => Self::WrongModel,
            DangerousActuationRefusal::ExpiredArm => Self::ExpiredArm,
            DangerousActuationRefusal::CurrentLimitExceeded => Self::CurrentLimitExceeded,
        }
    }
}

const fn str_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }

    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}
