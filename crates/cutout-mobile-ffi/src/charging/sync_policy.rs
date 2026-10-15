use super::{MobileChargingError, MobileChargingSyncOutcome, MobileChargingSyncSummaryDto};

/// Credential state observed by the native Keychain adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncCredentialState {
    Missing,
    RegistrationStarted,
    Ready,
    Unreadable,
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

/// Result of losing the Keychain registration claim and rereading its record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingRegistrationClaimResult {
    ReadyCredential,
    Unavailable,
}

/// Native action after rereading a credential when the registration claim was lost.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingRegistrationClaimAdmission {
    Synchronize,
    RequireExplicitRetry,
}

/// Action a native adapter executes for one typed sync decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncAction {
    NextRound,
    Retry { after_seconds: u32 },
    Synced,
    Pending,
    Offline,
    AuthenticationRequired,
    Conflict,
    Rejected,
    Throttled,
    StorageUnavailable,
}

/// Stable terminal category for a protocol error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncFailure {
    Offline,
    AuthenticationRequired,
    Conflict,
    Rejected,
    Throttled,
    StorageUnavailable,
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

/// One policy result and the complete progress snapshot to publish.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingSyncStep {
    pub state: MobileChargingSyncRunState,
    pub action: MobileChargingSyncAction,
}

/// Selects whether native code may register, sync, or refresh public data.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_registration_admission(
    state: MobileChargingSyncCredentialState,
    explicit_retry: bool,
) -> MobileChargingRegistrationAdmission {
    match state {
        MobileChargingSyncCredentialState::Missing => {
            MobileChargingRegistrationAdmission::BeginRegistration
        }
        MobileChargingSyncCredentialState::RegistrationStarted if explicit_retry => {
            MobileChargingRegistrationAdmission::RegisterIdentity
        }
        MobileChargingSyncCredentialState::RegistrationStarted => {
            MobileChargingRegistrationAdmission::RequireExplicitRetry
        }
        MobileChargingSyncCredentialState::Ready => {
            MobileChargingRegistrationAdmission::Synchronize
        }
        MobileChargingSyncCredentialState::Unreadable => {
            MobileChargingRegistrationAdmission::PublicRefresh
        }
    }
}

/// Prevents a lost registration claim from creating another identity.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_registration_claim_admission(
    result: MobileChargingRegistrationClaimResult,
) -> MobileChargingRegistrationClaimAdmission {
    match result {
        MobileChargingRegistrationClaimResult::ReadyCredential => {
            MobileChargingRegistrationClaimAdmission::Synchronize
        }
        MobileChargingRegistrationClaimResult::Unavailable => {
            MobileChargingRegistrationClaimAdmission::RequireExplicitRetry
        }
    }
}

/// Adds one Rust sync result and selects the next run action.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_observe_summary(
    mut state: MobileChargingSyncRunState,
    summary: MobileChargingSyncSummaryDto,
) -> MobileChargingSyncStep {
    state.pending_count = summary.pending_count;
    state.downloaded = state.downloaded.saturating_add(summary.downloaded);
    state.uploaded = state.uploaded.saturating_add(summary.uploaded);

    match summary.outcome {
        MobileChargingSyncOutcome::Offline => observe_failure(state, MobileChargingError::Network),
        MobileChargingSyncOutcome::MoreWork if state.round < 2 => {
            state.attempt = 0;
            state.round += 1;
            step(state, MobileChargingSyncAction::NextRound)
        }
        MobileChargingSyncOutcome::MoreWork | MobileChargingSyncOutcome::Pending => {
            state.attempt = 0;
            step(state, MobileChargingSyncAction::Pending)
        }
        MobileChargingSyncOutcome::Synced => {
            state.attempt = 0;
            step(state, MobileChargingSyncAction::Synced)
        }
        MobileChargingSyncOutcome::Conflict => {
            state.attempt = 0;
            step(state, MobileChargingSyncAction::Conflict)
        }
        MobileChargingSyncOutcome::Rejected => {
            state.attempt = 0;
            step(state, MobileChargingSyncAction::Rejected)
        }
        MobileChargingSyncOutcome::AuthenticationRequired => {
            state.attempt = 0;
            step(state, MobileChargingSyncAction::AuthenticationRequired)
        }
        MobileChargingSyncOutcome::Throttled => {
            state.attempt = 0;
            step(state, MobileChargingSyncAction::Throttled)
        }
        MobileChargingSyncOutcome::StorageUnavailable => {
            state.attempt = 0;
            step(state, MobileChargingSyncAction::StorageUnavailable)
        }
    }
}

/// Selects a bounded retry or terminal typed failure while retaining progress.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_observe_error(
    state: MobileChargingSyncRunState,
    error: MobileChargingError,
) -> MobileChargingSyncStep {
    observe_failure(state, error)
}

/// Maps a protocol error to its stable sync failure category.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_failure(error: MobileChargingError) -> MobileChargingSyncFailure {
    failure_for_error(error)
}

/// Converts an error into the terminal summary category used by the engine.
#[must_use]
pub(super) fn outcome_for_error(error: MobileChargingError) -> MobileChargingSyncOutcome {
    match failure_for_error(error) {
        MobileChargingSyncFailure::Offline => MobileChargingSyncOutcome::Offline,
        MobileChargingSyncFailure::AuthenticationRequired => {
            MobileChargingSyncOutcome::AuthenticationRequired
        }
        MobileChargingSyncFailure::Conflict => MobileChargingSyncOutcome::Conflict,
        MobileChargingSyncFailure::Rejected => MobileChargingSyncOutcome::Rejected,
        MobileChargingSyncFailure::Throttled => MobileChargingSyncOutcome::Throttled,
        MobileChargingSyncFailure::StorageUnavailable => {
            MobileChargingSyncOutcome::StorageUnavailable
        }
    }
}

fn failure_for_error(error: MobileChargingError) -> MobileChargingSyncFailure {
    match error {
        MobileChargingError::Unauthorized => MobileChargingSyncFailure::AuthenticationRequired,
        MobileChargingError::Conflict => MobileChargingSyncFailure::Conflict,
        MobileChargingError::Rejected => MobileChargingSyncFailure::Rejected,
        MobileChargingError::Throttled => MobileChargingSyncFailure::Throttled,
        MobileChargingError::Network | MobileChargingError::Unavailable => {
            MobileChargingSyncFailure::Offline
        }
        _ => MobileChargingSyncFailure::StorageUnavailable,
    }
}

fn observe_failure(
    mut state: MobileChargingSyncRunState,
    error: MobileChargingError,
) -> MobileChargingSyncStep {
    if error == MobileChargingError::Network && state.attempt < 2 {
        let after_seconds = 1 << state.attempt;
        state.attempt += 1;
        return step(state, MobileChargingSyncAction::Retry { after_seconds });
    }
    step(state, action_for_error(error))
}

fn action_for_error(error: MobileChargingError) -> MobileChargingSyncAction {
    match error {
        MobileChargingError::Unauthorized => MobileChargingSyncAction::AuthenticationRequired,
        MobileChargingError::Conflict => MobileChargingSyncAction::Conflict,
        MobileChargingError::Rejected => MobileChargingSyncAction::Rejected,
        MobileChargingError::Throttled => MobileChargingSyncAction::Throttled,
        MobileChargingError::Network | MobileChargingError::Unavailable => {
            MobileChargingSyncAction::Offline
        }
        _ => MobileChargingSyncAction::StorageUnavailable,
    }
}

fn step(
    state: MobileChargingSyncRunState,
    action: MobileChargingSyncAction,
) -> MobileChargingSyncStep {
    MobileChargingSyncStep { state, action }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(
        pending_count: u32,
        downloaded: u32,
        uploaded: u32,
        outcome: MobileChargingSyncOutcome,
    ) -> MobileChargingSyncSummaryDto {
        MobileChargingSyncSummaryDto {
            pending_count,
            downloaded,
            uploaded,
            outcome,
        }
    }

    #[test]
    fn registration_admission_requires_explicit_retry_for_uncertain_state() {
        assert_eq!(
            mobile_charging_registration_admission(
                MobileChargingSyncCredentialState::RegistrationStarted,
                false,
            ),
            MobileChargingRegistrationAdmission::RequireExplicitRetry
        );
        assert_eq!(
            mobile_charging_registration_admission(
                MobileChargingSyncCredentialState::RegistrationStarted,
                true,
            ),
            MobileChargingRegistrationAdmission::RegisterIdentity
        );
        assert_eq!(
            mobile_charging_registration_admission(
                MobileChargingSyncCredentialState::Missing,
                false,
            ),
            MobileChargingRegistrationAdmission::BeginRegistration
        );
        assert_eq!(
            mobile_charging_registration_admission(MobileChargingSyncCredentialState::Ready, false,),
            MobileChargingRegistrationAdmission::Synchronize
        );
        assert_eq!(
            mobile_charging_registration_admission(
                MobileChargingSyncCredentialState::Unreadable,
                true,
            ),
            MobileChargingRegistrationAdmission::PublicRefresh
        );
        assert_eq!(
            mobile_charging_registration_claim_admission(
                MobileChargingRegistrationClaimResult::Unavailable,
            ),
            MobileChargingRegistrationClaimAdmission::RequireExplicitRetry
        );
        assert_eq!(
            mobile_charging_registration_claim_admission(
                MobileChargingRegistrationClaimResult::ReadyCredential,
            ),
            MobileChargingRegistrationClaimAdmission::Synchronize
        );
    }

    #[test]
    fn retries_only_network_errors_with_bounded_delays_and_keeps_progress() {
        let state = MobileChargingSyncRunState {
            pending_count: 2,
            downloaded: 3,
            uploaded: 4,
            ..Default::default()
        };
        let first = mobile_charging_sync_observe_error(state, MobileChargingError::Network);
        assert_eq!(
            first.action,
            MobileChargingSyncAction::Retry { after_seconds: 1 }
        );
        assert_eq!(first.state.downloaded, 3);
        assert_eq!(first.state.uploaded, 4);
        let second = mobile_charging_sync_observe_error(first.state, MobileChargingError::Network);
        assert_eq!(
            second.action,
            MobileChargingSyncAction::Retry { after_seconds: 2 }
        );
        let last = mobile_charging_sync_observe_error(second.state, MobileChargingError::Network);
        assert_eq!(last.action, MobileChargingSyncAction::Offline);
        assert_eq!(last.state.pending_count, 2);
    }

    #[test]
    fn offline_summary_uses_the_same_bounded_retry_budget_as_an_error() {
        let first = mobile_charging_sync_observe_summary(
            MobileChargingSyncRunState::default(),
            summary(2, 1, 3, MobileChargingSyncOutcome::Offline),
        );
        assert_eq!(
            first.action,
            MobileChargingSyncAction::Retry { after_seconds: 1 }
        );
        let second = mobile_charging_sync_observe_summary(
            first.state,
            summary(2, 0, 0, MobileChargingSyncOutcome::Offline),
        );
        assert_eq!(
            second.action,
            MobileChargingSyncAction::Retry { after_seconds: 2 }
        );
        let terminal = mobile_charging_sync_observe_summary(
            second.state,
            summary(2, 0, 0, MobileChargingSyncOutcome::Offline),
        );
        assert_eq!(terminal.action, MobileChargingSyncAction::Offline);
        assert_eq!(terminal.state.downloaded, 1);
        assert_eq!(terminal.state.uploaded, 3);
    }

    #[test]
    fn summary_decisions_accumulate_progress_and_bound_rounds() {
        let first = mobile_charging_sync_observe_summary(
            MobileChargingSyncRunState::default(),
            summary(2, 3, 1, MobileChargingSyncOutcome::MoreWork),
        );
        assert_eq!(first.action, MobileChargingSyncAction::NextRound);
        let second = mobile_charging_sync_observe_summary(
            first.state,
            summary(1, 2, 3, MobileChargingSyncOutcome::MoreWork),
        );
        assert_eq!(second.action, MobileChargingSyncAction::NextRound);
        assert_eq!(second.state.downloaded, 5);
        assert_eq!(second.state.uploaded, 4);
        let final_step = mobile_charging_sync_observe_summary(
            second.state,
            summary(1, 4, 2, MobileChargingSyncOutcome::MoreWork),
        );
        assert_eq!(final_step.action, MobileChargingSyncAction::Pending);
        assert_eq!(final_step.state.round, 2);
        assert_eq!(final_step.state.downloaded, 9);
        assert_eq!(final_step.state.uploaded, 6);
    }

    #[test]
    fn pending_outbox_work_continues_until_the_round_limit() {
        let first = mobile_charging_sync_observe_summary(
            MobileChargingSyncRunState::default(),
            summary(9, 0, 8, MobileChargingSyncOutcome::MoreWork),
        );
        assert_eq!(first.action, MobileChargingSyncAction::NextRound);
        let second = mobile_charging_sync_observe_summary(
            first.state,
            summary(1, 0, 8, MobileChargingSyncOutcome::MoreWork),
        );
        assert_eq!(second.action, MobileChargingSyncAction::NextRound);
        let final_step = mobile_charging_sync_observe_summary(
            second.state,
            summary(1, 0, 8, MobileChargingSyncOutcome::MoreWork),
        );
        assert_eq!(final_step.action, MobileChargingSyncAction::Pending);
        assert_eq!(final_step.state.uploaded, 24);
    }

    #[test]
    fn terminal_conflict_and_rejection_keep_the_current_round_progress() {
        let progress = MobileChargingSyncRunState {
            downloaded: 3,
            uploaded: 1,
            ..Default::default()
        };
        let conflict = mobile_charging_sync_observe_summary(
            progress,
            summary(2, 4, 5, MobileChargingSyncOutcome::Conflict),
        );
        assert_eq!(conflict.action, MobileChargingSyncAction::Conflict);
        assert_eq!(conflict.state.pending_count, 2);
        assert_eq!(conflict.state.downloaded, 7);
        assert_eq!(conflict.state.uploaded, 6);
        let rejected = mobile_charging_sync_observe_summary(
            progress,
            summary(1, 2, 3, MobileChargingSyncOutcome::Rejected),
        );
        assert_eq!(rejected.action, MobileChargingSyncAction::Rejected);
        assert_eq!(rejected.state.downloaded, 5);
        assert_eq!(rejected.state.uploaded, 4);
    }
}
