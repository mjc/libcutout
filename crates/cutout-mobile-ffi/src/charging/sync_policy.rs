use super::{
    MobileChargingError, MobileChargingSyncOutcome, MobileChargingSyncResult,
    MobileChargingSyncSummaryDto,
};
use cutout_application::charging::sync as application;

/// Credential state observed by the native Keychain adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncCredentialState {
    Missing,
    RegistrationStarted,
    Ready,
    Unreadable,
    DeletionPending,
    Deleted,
}

impl From<MobileChargingSyncCredentialState> for application::CredentialState {
    fn from(state: MobileChargingSyncCredentialState) -> Self {
        match state {
            MobileChargingSyncCredentialState::Missing => Self::Missing,
            MobileChargingSyncCredentialState::RegistrationStarted => Self::RegistrationStarted,
            MobileChargingSyncCredentialState::Ready => Self::Ready,
            MobileChargingSyncCredentialState::Unreadable => Self::Unreadable,
            MobileChargingSyncCredentialState::DeletionPending => Self::DeletionPending,
            MobileChargingSyncCredentialState::Deleted => Self::Deleted,
        }
    }
}

/// Native work selected for the observed credential state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingRegistrationAdmission {
    BeginRegistration,
    RegisterIdentity,
    Synchronize,
    PublicRefresh,
    RequireExplicitRetry,
}

impl From<application::RegistrationAdmission> for MobileChargingRegistrationAdmission {
    fn from(admission: application::RegistrationAdmission) -> Self {
        match admission {
            application::RegistrationAdmission::BeginRegistration => Self::BeginRegistration,
            application::RegistrationAdmission::RegisterIdentity => Self::RegisterIdentity,
            application::RegistrationAdmission::Synchronize => Self::Synchronize,
            application::RegistrationAdmission::PublicRefresh => Self::PublicRefresh,
            application::RegistrationAdmission::RequireExplicitRetry => Self::RequireExplicitRetry,
        }
    }
}

/// Action a native adapter executes for one typed sync decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncAction {
    NextRound,
    Retry { after_seconds: u32 },
    Finished { result: MobileChargingSyncResult },
}

impl From<application::SyncAction> for MobileChargingSyncAction {
    fn from(action: application::SyncAction) -> Self {
        match action {
            application::SyncAction::NextRound => Self::NextRound,
            application::SyncAction::Retry { after_seconds } => Self::Retry { after_seconds },
            application::SyncAction::Finished { result } => Self::Finished {
                result: result.into(),
            },
        }
    }
}

/// Progress retained across the bounded run, including request retries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingSyncRunState {
    pub round: u32,
    pub attempt: u32,
    pub pending_count: u32,
    pub downloaded: u32,
    pub uploaded: u32,
}

impl From<MobileChargingSyncRunState> for application::SyncRunState {
    fn from(state: MobileChargingSyncRunState) -> Self {
        Self {
            round: state.round,
            attempt: state.attempt,
            pending_count: state.pending_count,
            downloaded: state.downloaded,
            uploaded: state.uploaded,
        }
    }
}

impl From<application::SyncRunState> for MobileChargingSyncRunState {
    fn from(state: application::SyncRunState) -> Self {
        Self {
            round: state.round,
            attempt: state.attempt,
            pending_count: state.pending_count,
            downloaded: state.downloaded,
            uploaded: state.uploaded,
        }
    }
}

/// One policy result and the complete progress snapshot to publish.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingSyncStep {
    pub state: MobileChargingSyncRunState,
    pub action: MobileChargingSyncAction,
}

impl From<application::SyncStep> for MobileChargingSyncStep {
    fn from(step: application::SyncStep) -> Self {
        Self {
            state: step.state.into(),
            action: step.action.into(),
        }
    }
}

impl From<MobileChargingSyncResult> for application::SyncResult {
    fn from(result: MobileChargingSyncResult) -> Self {
        match result {
            MobileChargingSyncResult::Synced => Self::Synced,
            MobileChargingSyncResult::Pending => Self::Pending,
            MobileChargingSyncResult::Conflict => Self::Conflict,
            MobileChargingSyncResult::Rejected => Self::Rejected,
            MobileChargingSyncResult::AuthenticationRequired => Self::AuthenticationRequired,
            MobileChargingSyncResult::Throttled => Self::Throttled,
            MobileChargingSyncResult::Offline => Self::Offline,
            MobileChargingSyncResult::StorageUnavailable => Self::StorageUnavailable,
        }
    }
}

impl From<application::SyncResult> for MobileChargingSyncResult {
    fn from(result: application::SyncResult) -> Self {
        match result {
            application::SyncResult::Synced => Self::Synced,
            application::SyncResult::Pending => Self::Pending,
            application::SyncResult::Conflict => Self::Conflict,
            application::SyncResult::Rejected => Self::Rejected,
            application::SyncResult::AuthenticationRequired => Self::AuthenticationRequired,
            application::SyncResult::Throttled => Self::Throttled,
            application::SyncResult::Offline => Self::Offline,
            application::SyncResult::StorageUnavailable => Self::StorageUnavailable,
        }
    }
}

impl From<MobileChargingSyncOutcome> for application::SyncOutcome {
    fn from(outcome: MobileChargingSyncOutcome) -> Self {
        match outcome {
            MobileChargingSyncOutcome::MoreWork => Self::MoreWork,
            MobileChargingSyncOutcome::Finished { result } => Self::Finished {
                result: result.into(),
            },
        }
    }
}

impl From<MobileChargingSyncSummaryDto> for application::SyncSummary {
    fn from(summary: MobileChargingSyncSummaryDto) -> Self {
        Self {
            pending_count: summary.pending_count,
            downloaded: summary.downloaded,
            uploaded: summary.uploaded,
            outcome: summary.outcome.into(),
        }
    }
}

impl From<MobileChargingError> for application::SyncError {
    fn from(error: MobileChargingError) -> Self {
        match error {
            MobileChargingError::Unauthorized => Self::Unauthorized,
            MobileChargingError::Conflict => Self::Conflict,
            MobileChargingError::Rejected => Self::Rejected,
            MobileChargingError::Throttled => Self::Throttled,
            MobileChargingError::Network => Self::Network,
            MobileChargingError::Unavailable => Self::Unavailable,
            MobileChargingError::InvalidPin
            | MobileChargingError::AccessNoteBlank
            | MobileChargingError::AccessNoteTooLong
            | MobileChargingError::AccessNoteControlCharacter
            | MobileChargingError::InvalidMetadata
            | MobileChargingError::InvalidIdentifier
            | MobileChargingError::InvalidLimit
            | MobileChargingError::NotOwned
            | MobileChargingError::NotFound
            | MobileChargingError::OutboxFull
            | MobileChargingError::StorageFailure => Self::StorageFailure,
        }
    }
}

/// Selects whether native code may register, sync, or refresh public data.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_registration_admission(
    state: MobileChargingSyncCredentialState,
    explicit_retry: bool,
) -> MobileChargingRegistrationAdmission {
    application::registration_admission(state.into(), explicit_retry).into()
}

/// Adds one Rust sync result and selects the next run action.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_observe_summary(
    state: MobileChargingSyncRunState,
    summary: MobileChargingSyncSummaryDto,
) -> MobileChargingSyncStep {
    application::observe_summary(state.into(), summary.into()).into()
}

/// Selects a bounded retry or terminal result while retaining progress.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_observe_error(
    state: MobileChargingSyncRunState,
    error: MobileChargingError,
) -> MobileChargingSyncStep {
    application::observe_error(state.into(), error.into()).into()
}

/// Maps a protocol error to its stable sync result.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_result(error: MobileChargingError) -> MobileChargingSyncResult {
    result_for_error(error)
}

pub(super) fn result_for_error(error: MobileChargingError) -> MobileChargingSyncResult {
    application::result_for_error(error.into()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_export_preserves_admission_for_each_credential_state() {
        let cases = [
            (
                MobileChargingSyncCredentialState::Missing,
                false,
                MobileChargingRegistrationAdmission::BeginRegistration,
            ),
            (
                MobileChargingSyncCredentialState::RegistrationStarted,
                false,
                MobileChargingRegistrationAdmission::RequireExplicitRetry,
            ),
            (
                MobileChargingSyncCredentialState::RegistrationStarted,
                true,
                MobileChargingRegistrationAdmission::RegisterIdentity,
            ),
            (
                MobileChargingSyncCredentialState::Ready,
                true,
                MobileChargingRegistrationAdmission::Synchronize,
            ),
            (
                MobileChargingSyncCredentialState::Unreadable,
                false,
                MobileChargingRegistrationAdmission::PublicRefresh,
            ),
            (
                MobileChargingSyncCredentialState::DeletionPending,
                true,
                MobileChargingRegistrationAdmission::PublicRefresh,
            ),
            (
                MobileChargingSyncCredentialState::Deleted,
                true,
                MobileChargingRegistrationAdmission::PublicRefresh,
            ),
        ];

        for (state, explicit_retry, expected) in cases {
            assert_eq!(
                mobile_charging_registration_admission(state, explicit_retry),
                expected
            );
        }
    }

    #[test]
    fn all_mobile_errors_keep_their_existing_sync_result_mapping() {
        let cases = [
            (
                MobileChargingError::Unauthorized,
                MobileChargingSyncResult::AuthenticationRequired,
            ),
            (
                MobileChargingError::Conflict,
                MobileChargingSyncResult::Conflict,
            ),
            (
                MobileChargingError::Rejected,
                MobileChargingSyncResult::Rejected,
            ),
            (
                MobileChargingError::Throttled,
                MobileChargingSyncResult::Throttled,
            ),
            (
                MobileChargingError::Network,
                MobileChargingSyncResult::Offline,
            ),
            (
                MobileChargingError::Unavailable,
                MobileChargingSyncResult::Offline,
            ),
            (
                MobileChargingError::InvalidPin,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::AccessNoteBlank,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::AccessNoteTooLong,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::AccessNoteControlCharacter,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::InvalidMetadata,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::InvalidIdentifier,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::InvalidLimit,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::NotOwned,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::NotFound,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::OutboxFull,
                MobileChargingSyncResult::StorageUnavailable,
            ),
            (
                MobileChargingError::StorageFailure,
                MobileChargingSyncResult::StorageUnavailable,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(mobile_charging_sync_result(error), expected, "{error:?}");
        }
    }

    #[test]
    fn summary_export_preserves_progress_and_action() {
        let step = mobile_charging_sync_observe_summary(
            MobileChargingSyncRunState {
                round: 1,
                attempt: 1,
                pending_count: 8,
                downloaded: 4,
                uploaded: 3,
            },
            MobileChargingSyncSummaryDto {
                pending_count: 2,
                downloaded: 2,
                uploaded: 5,
                outcome: MobileChargingSyncOutcome::MoreWork,
            },
        );

        assert_eq!(step.state.round, 2);
        assert_eq!(step.state.attempt, 0);
        assert_eq!(step.state.pending_count, 2);
        assert_eq!(step.state.downloaded, 6);
        assert_eq!(step.state.uploaded, 8);
        assert_eq!(step.action, MobileChargingSyncAction::NextRound);
    }

    #[test]
    fn network_error_export_preserves_bounded_retry_and_existing_native_shape() {
        let step = mobile_charging_sync_observe_error(
            MobileChargingSyncRunState {
                attempt: 1,
                pending_count: 3,
                downloaded: 4,
                uploaded: 5,
                ..Default::default()
            },
            MobileChargingError::Network,
        );

        assert_eq!(
            step.action,
            MobileChargingSyncAction::Retry { after_seconds: 2 }
        );
        assert_eq!(step.state.attempt, 2);
        assert_eq!(step.state.pending_count, 3);
        assert_eq!(step.state.downloaded, 4);
        assert_eq!(step.state.uploaded, 5);
    }
}
