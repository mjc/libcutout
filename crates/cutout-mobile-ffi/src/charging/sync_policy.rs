use super::{
    MobileChargingError, MobileChargingSyncOutcome, MobileChargingSyncResult,
    MobileChargingSyncSummaryDto,
};

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

/// Native work selected for the observed credential state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingRegistrationAdmission {
    BeginRegistration,
    RegisterIdentity,
    Synchronize,
    PublicRefresh,
    RequireExplicitRetry,
}

/// Action a native adapter executes for one typed sync decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncAction {
    NextRound,
    Retry { after_seconds: u32 },
    Finished { result: MobileChargingSyncResult },
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
        MobileChargingSyncCredentialState::Unreadable
        | MobileChargingSyncCredentialState::DeletionPending
        | MobileChargingSyncCredentialState::Deleted => {
            MobileChargingRegistrationAdmission::PublicRefresh
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
        MobileChargingSyncOutcome::MoreWork if state.round < 2 => {
            state.attempt = 0;
            state.round += 1;
            step(state, MobileChargingSyncAction::NextRound)
        }
        MobileChargingSyncOutcome::MoreWork => finish(state, MobileChargingSyncResult::Pending),
        MobileChargingSyncOutcome::Finished {
            result: MobileChargingSyncResult::Offline,
        } => observe_failure(state, MobileChargingError::Network),
        MobileChargingSyncOutcome::Finished { result } => finish(state, result),
    }
}

fn finish(
    mut state: MobileChargingSyncRunState,
    result: MobileChargingSyncResult,
) -> MobileChargingSyncStep {
    state.attempt = 0;
    step(state, MobileChargingSyncAction::Finished { result })
}

/// Selects a bounded retry or terminal result while retaining progress.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_observe_error(
    state: MobileChargingSyncRunState,
    error: MobileChargingError,
) -> MobileChargingSyncStep {
    observe_failure(state, error)
}

/// Maps a protocol error to its stable sync result.
#[uniffi::export]
#[must_use]
pub fn mobile_charging_sync_result(error: MobileChargingError) -> MobileChargingSyncResult {
    result_for_error(error)
}

/// Converts an error into its terminal sync result.
#[must_use]
pub(super) fn result_for_error(error: MobileChargingError) -> MobileChargingSyncResult {
    match error {
        MobileChargingError::Unauthorized => MobileChargingSyncResult::AuthenticationRequired,
        MobileChargingError::Conflict => MobileChargingSyncResult::Conflict,
        MobileChargingError::Rejected => MobileChargingSyncResult::Rejected,
        MobileChargingError::Throttled => MobileChargingSyncResult::Throttled,
        MobileChargingError::Network | MobileChargingError::Unavailable => {
            MobileChargingSyncResult::Offline
        }
        _ => MobileChargingSyncResult::StorageUnavailable,
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
    step(
        state,
        MobileChargingSyncAction::Finished {
            result: result_for_error(error),
        },
    )
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
        assert_eq!(
            last.action,
            MobileChargingSyncAction::Finished {
                result: MobileChargingSyncResult::Offline
            }
        );
        assert_eq!(last.state.pending_count, 2);
        assert_eq!(last.state.attempt, 2);
    }

    #[test]
    fn offline_summary_uses_the_same_bounded_retry_budget_as_an_error() {
        let first = mobile_charging_sync_observe_summary(
            MobileChargingSyncRunState::default(),
            summary(2, 1, 3, offline()),
        );
        assert_eq!(
            first.action,
            MobileChargingSyncAction::Retry { after_seconds: 1 }
        );
        let second = mobile_charging_sync_observe_summary(first.state, summary(2, 0, 0, offline()));
        assert_eq!(
            second.action,
            MobileChargingSyncAction::Retry { after_seconds: 2 }
        );
        let terminal =
            mobile_charging_sync_observe_summary(second.state, summary(2, 0, 0, offline()));
        assert_eq!(
            terminal.action,
            MobileChargingSyncAction::Finished {
                result: MobileChargingSyncResult::Offline
            }
        );
        assert_eq!(terminal.state.downloaded, 1);
        assert_eq!(terminal.state.uploaded, 3);
        assert_eq!(terminal.state.attempt, 2);
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
        assert_eq!(
            final_step.action,
            MobileChargingSyncAction::Finished {
                result: MobileChargingSyncResult::Pending
            }
        );
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
        assert_eq!(
            final_step.action,
            MobileChargingSyncAction::Finished {
                result: MobileChargingSyncResult::Pending
            }
        );
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
            summary(2, 4, 5, finished(MobileChargingSyncResult::Conflict)),
        );
        assert_eq!(
            conflict.action,
            MobileChargingSyncAction::Finished {
                result: MobileChargingSyncResult::Conflict
            }
        );
        assert_eq!(conflict.state.pending_count, 2);
        assert_eq!(conflict.state.downloaded, 7);
        assert_eq!(conflict.state.uploaded, 6);
        let rejected = mobile_charging_sync_observe_summary(
            progress,
            summary(1, 2, 3, finished(MobileChargingSyncResult::Rejected)),
        );
        assert_eq!(
            rejected.action,
            MobileChargingSyncAction::Finished {
                result: MobileChargingSyncResult::Rejected
            }
        );
        assert_eq!(rejected.state.downloaded, 5);
        assert_eq!(rejected.state.uploaded, 4);
    }

    fn finished(result: MobileChargingSyncResult) -> MobileChargingSyncOutcome {
        MobileChargingSyncOutcome::Finished { result }
    }

    fn offline() -> MobileChargingSyncOutcome {
        finished(MobileChargingSyncResult::Offline)
    }

    #[test]
    fn unavailable_errors_are_offline_without_being_retried() {
        let step = mobile_charging_sync_observe_error(
            MobileChargingSyncRunState {
                attempt: 1,
                ..Default::default()
            },
            MobileChargingError::Unavailable,
        );
        assert_eq!(
            step.action,
            MobileChargingSyncAction::Finished {
                result: MobileChargingSyncResult::Offline
            }
        );
        assert_eq!(step.state.attempt, 1);
        assert_eq!(
            mobile_charging_sync_result(MobileChargingError::Unavailable),
            MobileChargingSyncResult::Offline
        );
    }
}

#[cfg(test)]
mod account_deletion_tests {
    use super::*;
    #[test]
    fn deleted_credentials_never_admit_registration_or_authenticated_sync() {
        for state in [
            MobileChargingSyncCredentialState::DeletionPending,
            MobileChargingSyncCredentialState::Deleted,
        ] {
            for explicit_retry in [false, true] {
                assert_eq!(
                    mobile_charging_registration_admission(state, explicit_retry),
                    MobileChargingRegistrationAdmission::PublicRefresh
                );
            }
        }
    }
}
