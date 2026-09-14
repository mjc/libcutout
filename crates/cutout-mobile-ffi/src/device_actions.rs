//! Thin semantic action projection from the shared connection owner.
use crate::device_settings::setting_enum;
use crate::{
    CutoutSessionStateHandle, MobileConnectionAttemptSnapshotDto, MobileConnectionAttemptTokenDto,
    MobileControlRefusalReasonDto, MobileDeviceSessionStepDto, MobileValueQualityDto,
    MobileValueSourceDto, MobileVerificationStatusDto,
};
use cutout_core::{DeviceActionId, DeviceActionProgress, DeviceActionStatus, DeviceActionStep};
use cutout_protocols::{
    ActionAccess, ActionConfirmation, ActionDescriptor, ActionRequestError, ActionRole,
    DeviceActionSubmissionError,
};

setting_enum!(
    MobileDeviceActionIdDto,
    DeviceActionId,
    "Semantic device action identity.",
    [Horn, ResetTripMeter, GyroCalibration]
);
setting_enum!(
    MobileDeviceActionProgressDto,
    DeviceActionProgress,
    "Measured physical procedure phase.",
    [AdjustingAttitude, ReadyToCalibrate]
);
setting_enum!(
    MobileDeviceActionStatusDto,
    DeviceActionStatus,
    "Shared device action lifecycle.",
    [
        Idle,
        WaitingForProgress,
        ReadyForNextStep,
        SentWithoutConfirmation,
        Refused,
        Failed
    ]
);
setting_enum!(
    MobileDeviceActionStepDto,
    DeviceActionStep,
    "Rust-selected procedure step.",
    [Invoke, PrepareGyroCalibration, StartGyroCalibration]
);
setting_enum!(
    MobileDeviceActionAccessDto,
    ActionAccess,
    "Static action invocation availability.",
    [Available, Unverified]
);
setting_enum!(
    MobileDeviceActionConfirmationDto,
    ActionConfirmation,
    "Available action completion evidence.",
    [None, ProgressReadback]
);
setting_enum!(
    MobileDeviceActionRoleDto,
    ActionRole,
    "Native action presentation role.",
    [Momentary, Destructive, Procedure]
);

/// Native-neutral semantic action definition.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceActionDescriptorDto {
    /// Stable semantic identity.
    pub id: MobileDeviceActionIdDto,
    /// Shared action label key.
    pub label_key: String,
    /// Shared procedure/consequence key.
    pub help_key: String,
    /// Stable display order.
    pub order: u16,
    /// Confirmation or procedure presentation.
    pub role: MobileDeviceActionRoleDto,
    /// Static protocol availability.
    pub access: MobileDeviceActionAccessDto,
    /// Available completion evidence.
    pub confirmation: MobileDeviceActionConfirmationDto,
}
impl From<ActionDescriptor> for MobileDeviceActionDescriptorDto {
    fn from(value: ActionDescriptor) -> Self {
        Self {
            id: value.id.into(),
            label_key: value.label_key.into(),
            help_key: value.help_key.into(),
            order: value.order,
            role: value.role.into(),
            access: value.access.into(),
            confirmation: value.confirmation.into(),
        }
    }
}

/// Original evidence for a reported action phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceActionProgressReadingDto {
    /// Semantic procedure phase.
    pub value: MobileDeviceActionProgressDto,
    /// Original measurement source.
    pub source: MobileValueSourceDto,
    /// Original measurement quality.
    pub quality: MobileValueQualityDto,
    /// Original measurement verification.
    pub verification: MobileVerificationStatusDto,
}

/// Next protocol-owned procedure step or its current unavailability reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileDeviceActionNextStepDto {
    /// The shared owner permits this next step.
    Available { step: MobileDeviceActionStepDto },
    /// Waiting for measured procedure progress.
    Busy,
    /// The previous procedure requires restarting the wheel.
    RestartRequired,
}

impl From<Result<DeviceActionStep, cutout_core::DeviceActionStateError>>
    for MobileDeviceActionNextStepDto
{
    fn from(value: Result<DeviceActionStep, cutout_core::DeviceActionStateError>) -> Self {
        match value {
            Ok(step) => Self::Available { step: step.into() },
            Err(cutout_core::DeviceActionStateError::Busy) => Self::Busy,
            Err(cutout_core::DeviceActionStateError::RestartRequired) => Self::RestartRequired,
        }
    }
}

/// Shared action state without invented completion.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceActionSnapshotDto {
    /// Semantic action identity.
    pub id: MobileDeviceActionIdDto,
    /// Optional measured progress and evidence.
    pub progress: Option<MobileDeviceActionProgressReadingDto>,
    /// Shared lifecycle status.
    pub status: MobileDeviceActionStatusDto,
    /// Observation age at publication time.
    pub age_ms: Option<u64>,
    /// Last requested procedure step.
    pub requested_step: Option<MobileDeviceActionStepDto>,
    /// Next step selected by the shared action lifecycle.
    pub next_step: MobileDeviceActionNextStepDto,
    /// Existing protocol guard refusal.
    pub refusal: Option<MobileControlRefusalReasonDto>,
}
impl From<cutout_core::DeviceActionSnapshot> for MobileDeviceActionSnapshotDto {
    fn from(value: cutout_core::DeviceActionSnapshot) -> Self {
        Self {
            id: value.id.into(),
            progress: value
                .progress
                .map(|progress| MobileDeviceActionProgressReadingDto {
                    value: progress.value.into(),
                    source: progress.source.into(),
                    quality: progress.quality.into(),
                    verification: progress.verification.into(),
                }),
            status: value.status.into(),
            age_ms: value.age.map(|age| age.as_milliseconds()),
            requested_step: value.requested_step.map(Into::into),
            next_step: value.next_step.into(),
            refusal: value
                .refusal
                .map(|reason| cutout_core::ControlRefusalReasonDto::from(reason).into()),
        }
    }
}

/// Atomic action catalog and owning connection.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceActionDescriptorsDto {
    /// Current connection token and readiness.
    pub connection: MobileConnectionAttemptSnapshotDto,
    /// Whether this attempt authorizes source-backed validation writes.
    pub validation_authorized: bool,
    /// Semantic action catalog.
    pub descriptors: Vec<MobileDeviceActionDescriptorDto>,
}

/// Atomic action lifecycle snapshot and owning connection.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileDeviceActionsSnapshotDto {
    /// Current connection token and readiness.
    pub connection: MobileConnectionAttemptSnapshotDto,
    /// Latest accepted monotonic input timestamp.
    pub at_ms: u64,
    /// Shared lifecycle rows.
    pub actions: Vec<MobileDeviceActionSnapshotDto>,
}

/// Typed semantic action refusal before protocol execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileDeviceActionSubmissionError {
    /// The requested connection is no longer verified.
    #[error("connection unavailable")]
    ConnectionUnavailable,
    /// A procedure is waiting for measured progress.
    #[error("action busy")]
    Busy,
    /// The procedure requires a wheel restart.
    #[error("wheel restart required")]
    RestartRequired,
    /// The protocol does not expose this action.
    #[error("action unavailable")]
    Unavailable,
    /// The write requires explicit validation mode.
    #[error("action is unverified")]
    Unverified,
    /// The semantic procedure step is invalid.
    #[error("invalid action step")]
    InvalidStep,
}
impl From<DeviceActionSubmissionError> for MobileDeviceActionSubmissionError {
    fn from(value: DeviceActionSubmissionError) -> Self {
        match value {
            DeviceActionSubmissionError::ConnectionUnavailable => Self::ConnectionUnavailable,
            DeviceActionSubmissionError::Busy => Self::Busy,
            DeviceActionSubmissionError::RestartRequired => Self::RestartRequired,
            DeviceActionSubmissionError::Profile(ActionRequestError::Unavailable) => {
                Self::Unavailable
            }
            DeviceActionSubmissionError::Profile(ActionRequestError::Unverified) => {
                Self::Unverified
            }
            DeviceActionSubmissionError::Profile(ActionRequestError::InvalidStep) => {
                Self::InvalidStep
            }
        }
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Projects protocol-owned action definitions and connection identity atomically.
    #[must_use]
    pub fn action_descriptors(&self) -> MobileDeviceActionDescriptorsDto {
        let inner = self.lock_inner();
        MobileDeviceActionDescriptorsDto {
            connection: inner.state.connection.snapshot().into(),
            validation_authorized: inner.validation_authorized(),
            descriptors: inner
                .action_descriptors()
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
    /// Projects shared action state and connection identity atomically.
    #[must_use]
    pub fn actions_snapshot(&self) -> MobileDeviceActionsSnapshotDto {
        let snapshot = self.lock_inner().actions_snapshot();
        MobileDeviceActionsSnapshotDto {
            connection: (&snapshot.connection).into(),
            at_ms: snapshot.at.get(),
            actions: snapshot.actions.into_iter().map(Into::into).collect(),
        }
    }
    /// Invokes the next Rust-selected step through existing live protocol guards.
    ///
    /// # Errors
    /// Returns the protocol owner's typed refusal before transport execution.
    pub fn submit_action(
        &self,
        token: MobileConnectionAttemptTokenDto,
        id: MobileDeviceActionIdDto,
        monotonic_ms: u64,
    ) -> Result<MobileDeviceSessionStepDto, MobileDeviceActionSubmissionError> {
        self.lock_inner()
            .submit_action(
                &token.into(),
                id.into(),
                cutout_core::MonotonicTimestamp::new(monotonic_ms),
            )
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_action_boundary_preserves_unconfirmed_reset_and_stale_refusal() {
        let handle = CutoutSessionStateHandle::new();
        let token = handle
            .begin_connection_attempt("A".into(), 0)
            .token
            .unwrap();
        handle.connection_link_established(token.clone());
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        handle.observe_connection_notification(token.clone(), frame.clone());
        handle.resolve_device_session(token.clone(), false, 1);
        let mut inner = handle.lock_inner();
        let _ = inner.ingest(
            &token.clone().into(),
            &cutout_core::SessionInputDto::LinkUp {
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                max_write_len: None,
            },
        );
        let _ = inner.ingest(
            &token.clone().into(),
            &cutout_core::SessionInputDto::Notification {
                channel: cutout_protocols::VETERAN_DATA_CHANNEL.as_bytes(),
                bytes: frame,
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
            },
        );
        drop(inner);
        let catalog = handle.action_descriptors();
        assert_eq!(catalog.connection.token, Some(token.clone()));
        let reset = catalog
            .descriptors
            .iter()
            .find(|item| item.id == MobileDeviceActionIdDto::ResetTripMeter)
            .unwrap();
        assert_eq!(reset.confirmation, MobileDeviceActionConfirmationDto::None);
        let before_refusal = handle.actions_snapshot();
        assert_eq!(
            handle
                .submit_action(token.clone(), MobileDeviceActionIdDto::ResetTripMeter, 3)
                .unwrap_err(),
            MobileDeviceActionSubmissionError::Unverified
        );
        assert_eq!(handle.actions_snapshot(), before_refusal);
        assert!(handle.set_device_controls_validation(token.clone(), true));
        handle
            .submit_action(token.clone(), MobileDeviceActionIdDto::ResetTripMeter, 3)
            .unwrap();
        assert_eq!(
            handle
                .actions_snapshot()
                .actions
                .into_iter()
                .find(|action| action.id == MobileDeviceActionIdDto::ResetTripMeter)
                .unwrap()
                .status,
            MobileDeviceActionStatusDto::SentWithoutConfirmation
        );
        let replacement = handle.begin_connection_attempt("B".into(), 4);
        assert_eq!(
            handle
                .submit_action(token, MobileDeviceActionIdDto::ResetTripMeter, 5)
                .unwrap_err(),
            MobileDeviceActionSubmissionError::ConnectionUnavailable
        );
        assert_eq!(handle.actions_snapshot().connection, replacement);
        assert!(handle.actions_snapshot().actions.is_empty());
    }
}
