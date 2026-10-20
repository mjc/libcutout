//! Registration admission and bounded charging synchronization policy.

/// Credential state reported by the platform credential adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialState {
    Missing,
    RegistrationStarted,
    Ready,
    Unreadable,
    DeletionPending,
    Deleted,
}

/// Work admitted for the currently observed credential state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationAdmission {
    BeginRegistration,
    RegisterIdentity,
    Synchronize,
    PublicRefresh,
    RequireExplicitRetry,
}

/// Stable terminal result of a synchronization run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncResult {
    Synced,
    Pending,
    Conflict,
    Rejected,
    AuthenticationRequired,
    Throttled,
    Offline,
    StorageUnavailable,
}

/// Outcome of one bounded synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncOutcome {
    MoreWork,
    Finished { result: SyncResult },
}

/// Committed progress and outcome from one synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncSummary {
    pub pending_count: u32,
    pub downloaded: u32,
    pub uploaded: u32,
    pub outcome: SyncOutcome,
}

/// Error classes used by synchronization policy.
///
/// The binding adapter maps its richer boundary errors into these policy
/// categories. Storage and authorization remain distinct because they produce
/// different user-visible terminal results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncError {
    Unauthorized,
    Conflict,
    Rejected,
    Throttled,
    Network,
    Unavailable,
    StorageFailure,
}

/// Work selected after observing a pass or a request error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncAction {
    NextRound,
    Retry { after_seconds: u32 },
    Finished { result: SyncResult },
}

/// Progress retained across bounded rounds and request retries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SyncRunState {
    pub round: u32,
    pub attempt: u32,
    pub pending_count: u32,
    pub downloaded: u32,
    pub uploaded: u32,
}

/// One policy decision and the complete progress snapshot to publish.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncStep {
    pub state: SyncRunState,
    pub action: SyncAction,
}

/// Selects whether registration, authenticated sync, or public refresh is
/// admitted for the observed credential state.
#[must_use]
pub fn registration_admission(
    state: CredentialState,
    explicit_retry: bool,
) -> RegistrationAdmission {
    match state {
        CredentialState::Missing => RegistrationAdmission::BeginRegistration,
        CredentialState::RegistrationStarted if explicit_retry => {
            RegistrationAdmission::RegisterIdentity
        }
        CredentialState::RegistrationStarted => RegistrationAdmission::RequireExplicitRetry,
        CredentialState::Ready => RegistrationAdmission::Synchronize,
        CredentialState::Unreadable
        | CredentialState::DeletionPending
        | CredentialState::Deleted => RegistrationAdmission::PublicRefresh,
    }
}

/// Adds one pass summary to retained progress and selects the next action.
#[must_use]
pub fn observe_summary(mut state: SyncRunState, summary: SyncSummary) -> SyncStep {
    state.pending_count = summary.pending_count;
    state.downloaded = state.downloaded.saturating_add(summary.downloaded);
    state.uploaded = state.uploaded.saturating_add(summary.uploaded);

    match summary.outcome {
        SyncOutcome::MoreWork if state.round < 2 => {
            state.attempt = 0;
            state.round += 1;
            step(state, SyncAction::NextRound)
        }
        SyncOutcome::MoreWork => finish(state, SyncResult::Pending),
        SyncOutcome::Finished {
            result: SyncResult::Offline,
        } => observe_error(state, SyncError::Network),
        SyncOutcome::Finished { result } => finish(state, result),
    }
}

/// Selects a bounded retry or terminal result while retaining progress.
#[must_use]
pub fn observe_error(state: SyncRunState, error: SyncError) -> SyncStep {
    if error == SyncError::Network && state.attempt < 2 {
        let after_seconds = 1 << state.attempt;
        let mut state = state;
        state.attempt += 1;
        return step(state, SyncAction::Retry { after_seconds });
    }
    step(
        state,
        SyncAction::Finished {
            result: result_for_error(error),
        },
    )
}

/// Maps a policy error to its stable terminal synchronization result.
#[must_use]
pub const fn result_for_error(error: SyncError) -> SyncResult {
    match error {
        SyncError::Unauthorized => SyncResult::AuthenticationRequired,
        SyncError::Conflict => SyncResult::Conflict,
        SyncError::Rejected => SyncResult::Rejected,
        SyncError::Throttled => SyncResult::Throttled,
        SyncError::Network | SyncError::Unavailable => SyncResult::Offline,
        SyncError::StorageFailure => SyncResult::StorageUnavailable,
    }
}

fn finish(mut state: SyncRunState, result: SyncResult) -> SyncStep {
    state.attempt = 0;
    step(state, SyncAction::Finished { result })
}

fn step(state: SyncRunState, action: SyncAction) -> SyncStep {
    SyncStep { state, action }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(
        pending_count: u32,
        downloaded: u32,
        uploaded: u32,
        outcome: SyncOutcome,
    ) -> SyncSummary {
        SyncSummary {
            pending_count,
            downloaded,
            uploaded,
            outcome,
        }
    }

    fn finished(result: SyncResult) -> SyncOutcome {
        SyncOutcome::Finished { result }
    }

    fn offline() -> SyncOutcome {
        finished(SyncResult::Offline)
    }

    #[test]
    fn registration_admission_requires_explicit_retry_for_uncertain_state() {
        assert_eq!(
            registration_admission(CredentialState::RegistrationStarted, false),
            RegistrationAdmission::RequireExplicitRetry
        );
        assert_eq!(
            registration_admission(CredentialState::RegistrationStarted, true),
            RegistrationAdmission::RegisterIdentity
        );
        assert_eq!(
            registration_admission(CredentialState::Missing, false),
            RegistrationAdmission::BeginRegistration
        );
        assert_eq!(
            registration_admission(CredentialState::Ready, false),
            RegistrationAdmission::Synchronize
        );
        assert_eq!(
            registration_admission(CredentialState::Unreadable, true),
            RegistrationAdmission::PublicRefresh
        );
    }

    #[test]
    fn retries_only_network_errors_with_bounded_delays_and_keeps_progress() {
        let state = SyncRunState {
            pending_count: 2,
            downloaded: 3,
            uploaded: 4,
            ..Default::default()
        };
        let first = observe_error(state, SyncError::Network);
        assert_eq!(first.action, SyncAction::Retry { after_seconds: 1 });
        assert_eq!(first.state.downloaded, 3);
        assert_eq!(first.state.uploaded, 4);
        let second = observe_error(first.state, SyncError::Network);
        assert_eq!(second.action, SyncAction::Retry { after_seconds: 2 });
        let last = observe_error(second.state, SyncError::Network);
        assert_eq!(
            last.action,
            SyncAction::Finished {
                result: SyncResult::Offline
            }
        );
        assert_eq!(last.state.pending_count, 2);
        assert_eq!(last.state.attempt, 2);
    }

    #[test]
    fn offline_summary_uses_the_same_bounded_retry_budget_as_an_error() {
        let first = observe_summary(SyncRunState::default(), summary(2, 1, 3, offline()));
        assert_eq!(first.action, SyncAction::Retry { after_seconds: 1 });
        let second = observe_summary(first.state, summary(2, 0, 0, offline()));
        assert_eq!(second.action, SyncAction::Retry { after_seconds: 2 });
        let terminal = observe_summary(second.state, summary(2, 0, 0, offline()));
        assert_eq!(
            terminal.action,
            SyncAction::Finished {
                result: SyncResult::Offline
            }
        );
        assert_eq!(terminal.state.downloaded, 1);
        assert_eq!(terminal.state.uploaded, 3);
        assert_eq!(terminal.state.attempt, 2);
    }

    #[test]
    fn summary_decisions_accumulate_progress_and_bound_rounds() {
        let first = observe_summary(
            SyncRunState::default(),
            summary(2, 3, 1, SyncOutcome::MoreWork),
        );
        assert_eq!(first.action, SyncAction::NextRound);
        let second = observe_summary(first.state, summary(1, 2, 3, SyncOutcome::MoreWork));
        assert_eq!(second.action, SyncAction::NextRound);
        assert_eq!(second.state.downloaded, 5);
        assert_eq!(second.state.uploaded, 4);
        let final_step = observe_summary(second.state, summary(1, 4, 2, SyncOutcome::MoreWork));
        assert_eq!(
            final_step.action,
            SyncAction::Finished {
                result: SyncResult::Pending
            }
        );
        assert_eq!(final_step.state.round, 2);
        assert_eq!(final_step.state.downloaded, 9);
        assert_eq!(final_step.state.uploaded, 6);
    }

    #[test]
    fn pending_outbox_work_continues_until_the_round_limit() {
        let first = observe_summary(
            SyncRunState::default(),
            summary(9, 0, 8, SyncOutcome::MoreWork),
        );
        assert_eq!(first.action, SyncAction::NextRound);
        let second = observe_summary(first.state, summary(1, 0, 8, SyncOutcome::MoreWork));
        assert_eq!(second.action, SyncAction::NextRound);
        let final_step = observe_summary(second.state, summary(1, 0, 8, SyncOutcome::MoreWork));
        assert_eq!(
            final_step.action,
            SyncAction::Finished {
                result: SyncResult::Pending
            }
        );
        assert_eq!(final_step.state.uploaded, 24);
    }

    #[test]
    fn terminal_conflict_and_rejection_keep_the_current_round_progress() {
        let progress = SyncRunState {
            downloaded: 3,
            uploaded: 1,
            ..Default::default()
        };
        let conflict = observe_summary(progress, summary(2, 4, 5, finished(SyncResult::Conflict)));
        assert_eq!(
            conflict.action,
            SyncAction::Finished {
                result: SyncResult::Conflict
            }
        );
        assert_eq!(conflict.state.pending_count, 2);
        assert_eq!(conflict.state.downloaded, 7);
        assert_eq!(conflict.state.uploaded, 6);
        let rejected = observe_summary(progress, summary(1, 2, 3, finished(SyncResult::Rejected)));
        assert_eq!(
            rejected.action,
            SyncAction::Finished {
                result: SyncResult::Rejected
            }
        );
        assert_eq!(rejected.state.downloaded, 5);
        assert_eq!(rejected.state.uploaded, 4);
    }

    #[test]
    fn unavailable_errors_are_offline_without_being_retried() {
        let step = observe_error(
            SyncRunState {
                attempt: 1,
                ..Default::default()
            },
            SyncError::Unavailable,
        );
        assert_eq!(
            step.action,
            SyncAction::Finished {
                result: SyncResult::Offline
            }
        );
        assert_eq!(step.state.attempt, 1);
        assert_eq!(
            result_for_error(SyncError::Unavailable),
            SyncResult::Offline
        );
    }

    #[test]
    fn error_results_preserve_storage_and_authorization_categories() {
        assert_eq!(
            result_for_error(SyncError::Unauthorized),
            SyncResult::AuthenticationRequired
        );
        assert_eq!(
            result_for_error(SyncError::StorageFailure),
            SyncResult::StorageUnavailable
        );
        assert_eq!(result_for_error(SyncError::Conflict), SyncResult::Conflict);
        assert_eq!(result_for_error(SyncError::Rejected), SyncResult::Rejected);
        assert_eq!(
            result_for_error(SyncError::Throttled),
            SyncResult::Throttled
        );
    }

    #[test]
    fn attempt_reset_differs_between_round_progress_and_error_completion() {
        let progress = SyncRunState {
            attempt: 1,
            ..Default::default()
        };
        let next_round = observe_summary(progress, summary(1, 0, 0, SyncOutcome::MoreWork));
        assert_eq!(next_round.state.attempt, 0);

        let terminal_error = observe_error(progress, SyncError::StorageFailure);
        assert_eq!(terminal_error.state.attempt, 1);

        let terminal_summary = observe_summary(
            progress,
            summary(0, 0, 0, finished(SyncResult::StorageUnavailable)),
        );
        assert_eq!(terminal_summary.state.attempt, 0);
    }

    #[test]
    fn progress_counters_saturate_instead_of_wrapping() {
        let step = observe_summary(
            SyncRunState {
                downloaded: u32::MAX,
                uploaded: u32::MAX,
                ..Default::default()
            },
            summary(0, 1, 2, finished(SyncResult::Synced)),
        );
        assert_eq!(step.state.downloaded, u32::MAX);
        assert_eq!(step.state.uploaded, u32::MAX);
    }

    #[test]
    fn deleted_credentials_only_admit_public_refresh() {
        for state in [CredentialState::DeletionPending, CredentialState::Deleted] {
            for explicit_retry in [false, true] {
                assert_eq!(
                    registration_admission(state, explicit_retry),
                    RegistrationAdmission::PublicRefresh
                );
            }
        }
    }
}
