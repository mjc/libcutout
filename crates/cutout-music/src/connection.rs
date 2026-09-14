//! Bounded connection attempts for a foreground music provider session.

use cutout_core::{Duration, MonotonicTimestamp};

use crate::ids::ConnectionAttemptId;

const MAXIMUM_ATTEMPTS: u8 = 3;
const RETRY_DELAY: Duration = Duration::from_milliseconds(2_000);
const ATTEMPT_TIMEOUT: Duration = Duration::from_milliseconds(10_000);

/// Result of applying a provider connection callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicConnectionCallback {
    /// The callback matched the active attempt or connected session.
    Accepted,
    /// The callback belonged to an older or unknown attempt.
    Stale,
}

/// Retry admission, without credential ownership or platform SDK side effects.
///
/// A new explicit monitoring session creates a fresh policy. Failures and
/// disconnects retain the attempt budget; only a successful connection resets it.
#[derive(Debug, Default)]
enum MusicConnectionState {
    #[default]
    Idle,
    Connecting {
        attempt_id: ConnectionAttemptId,
        started_at: MonotonicTimestamp,
    },
    Connected {
        attempt_id: ConnectionAttemptId,
    },
    WaitingToRetry {
        not_before: MonotonicTimestamp,
    },
}

/// Bounded portable connection state for one provider session.
#[derive(Debug)]
pub struct MusicConnection {
    attempts: u8,
    last_attempt_id: ConnectionAttemptId,
    state: MusicConnectionState,
}

impl Default for MusicConnection {
    fn default() -> Self {
        Self {
            attempts: 0,
            last_attempt_id: ConnectionAttemptId::from_raw(0),
            state: MusicConnectionState::Idle,
        }
    }
}

impl MusicConnection {
    /// Returns the active attempt or connected session, when one exists.
    #[must_use]
    pub fn current_id(&self) -> Option<ConnectionAttemptId> {
        match self.state {
            MusicConnectionState::Connecting { attempt_id, .. }
            | MusicConnectionState::Connected { attempt_id } => Some(attempt_id),
            MusicConnectionState::Idle | MusicConnectionState::WaitingToRetry { .. } => None,
        }
    }

    /// Whether this identity owns an established provider connection.
    #[must_use]
    pub fn is_connected(&self, attempt_id: ConnectionAttemptId) -> bool {
        matches!(
            self.state,
            MusicConnectionState::Connected {
                attempt_id: current
            } if current == attempt_id
        )
    }

    /// Classifies a callback without changing attempt or connection state.
    #[must_use]
    pub fn classify(&self, attempt_id: ConnectionAttemptId) -> MusicConnectionCallback {
        if self.current_id() == Some(attempt_id) {
            MusicConnectionCallback::Accepted
        } else {
            MusicConnectionCallback::Stale
        }
    }

    /// Classifies a callback while enforcing the active attempt deadline.
    #[must_use]
    pub fn classify_at(
        &self,
        attempt_id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        match self.state {
            MusicConnectionState::Connected {
                attempt_id: current,
            } if current == attempt_id => MusicConnectionCallback::Accepted,
            MusicConnectionState::Connecting {
                attempt_id: current,
                started_at,
            } if current == attempt_id
                && MonotonicTimestamp::new(now_ms).saturating_duration_since(started_at)
                    < ATTEMPT_TIMEOUT =>
            {
                MusicConnectionCallback::Accepted
            }
            MusicConnectionState::Idle
            | MusicConnectionState::Connecting { .. }
            | MusicConnectionState::Connected { .. }
            | MusicConnectionState::WaitingToRetry { .. } => MusicConnectionCallback::Stale,
        }
    }

    /// Invalidates the active attempt/session without reusing callback identities.
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.state = MusicConnectionState::Idle;
    }

    /// Starts an attempt and returns its identity for callback validation.
    #[must_use]
    pub fn begin_attempt_id(&mut self, now_ms: u64) -> Option<ConnectionAttemptId> {
        if self.attempts >= MAXIMUM_ATTEMPTS {
            return None;
        }
        let now = MonotonicTimestamp::new(now_ms);
        match self.state {
            MusicConnectionState::Connecting { started_at, .. }
                if now.saturating_duration_since(started_at) < ATTEMPT_TIMEOUT =>
            {
                return None;
            }
            MusicConnectionState::WaitingToRetry { not_before } if now < not_before => return None,
            MusicConnectionState::Idle
            | MusicConnectionState::Connecting { .. }
            | MusicConnectionState::Connected { .. }
            | MusicConnectionState::WaitingToRetry { .. } => {}
        }
        let attempt_id = self.last_attempt_id.next()?;
        self.attempts += 1;
        self.last_attempt_id = attempt_id;
        self.state = MusicConnectionState::Connecting {
            attempt_id,
            started_at: now,
        };
        Some(attempt_id)
    }

    /// Accepts a failure only from the current attempt or connected session.
    #[must_use]
    pub fn failed_for(
        &mut self,
        attempt_id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        if self.classify_at(attempt_id, now_ms) == MusicConnectionCallback::Stale {
            return MusicConnectionCallback::Stale;
        }
        self.schedule_retry(now_ms);
        MusicConnectionCallback::Accepted
    }

    fn schedule_retry(&mut self, now_ms: u64) {
        let requested = MonotonicTimestamp::new(now_ms).saturating_add_duration(RETRY_DELAY);
        let not_before = match self.state {
            MusicConnectionState::WaitingToRetry { not_before } => not_before.max(requested),
            MusicConnectionState::Idle
            | MusicConnectionState::Connecting { .. }
            | MusicConnectionState::Connected { .. } => requested,
        };
        self.state = MusicConnectionState::WaitingToRetry { not_before };
    }

    /// Accepts a disconnect only from the current attempt or connected session.
    #[must_use]
    pub fn disconnected_for(
        &mut self,
        attempt_id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        self.failed_for(attempt_id, now_ms)
    }

    /// Accepts success only before the active attempt's deadline.
    #[must_use]
    pub fn established_for_at(
        &mut self,
        attempt_id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        if !matches!(
            self.state,
            MusicConnectionState::Connecting {
                attempt_id: current,
                ..
            } if current == attempt_id
        ) || self.classify_at(attempt_id, now_ms) == MusicConnectionCallback::Stale
        {
            return MusicConnectionCallback::Stale;
        }
        self.attempts = 0;
        self.state = MusicConnectionState::Connected { attempt_id };
        MusicConnectionCallback::Accepted
    }

    /// Retires an attempt whose deadline elapsed before a late success callback.
    #[must_use]
    pub fn expired_for(
        &mut self,
        attempt_id: ConnectionAttemptId,
        now_ms: u64,
    ) -> MusicConnectionCallback {
        let MusicConnectionState::Connecting {
            attempt_id: current,
            started_at,
        } = self.state
        else {
            return MusicConnectionCallback::Stale;
        };
        if current != attempt_id
            || MonotonicTimestamp::new(now_ms).saturating_duration_since(started_at)
                < ATTEMPT_TIMEOUT
        {
            return MusicConnectionCallback::Stale;
        }
        self.schedule_retry(now_ms);
        MusicConnectionCallback::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::{MusicConnection, MusicConnectionCallback};
    use crate::ids::ConnectionAttemptId;

    #[test]
    fn a_failed_attempt_retries_after_delay_without_resetting_budget() {
        let mut connection = MusicConnection::default();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert_eq!(
            connection.failed_for(first, 100),
            MusicConnectionCallback::Accepted
        );
        assert!(connection.begin_attempt_id(2_099).is_none());
        let second = connection.begin_attempt_id(2_100).expect("second attempt");
        assert_eq!(
            connection.failed_for(second, 2_200),
            MusicConnectionCallback::Accepted
        );
        let third = connection.begin_attempt_id(4_200).expect("third attempt");
        assert_eq!(
            connection.failed_for(third, 4_300),
            MusicConnectionCallback::Accepted
        );
        assert!(connection.begin_attempt_id(100_000).is_none());
    }

    #[test]
    fn a_lost_callback_times_out_but_attempts_remain_bounded() {
        let mut connection = MusicConnection::default();
        assert!(connection.begin_attempt_id(0).is_some());
        assert!(connection.begin_attempt_id(9_999).is_none());
        assert!(connection.begin_attempt_id(10_000).is_some());
        assert!(connection.begin_attempt_id(19_999).is_none());
        assert!(connection.begin_attempt_id(20_000).is_some());
        assert!(connection.begin_attempt_id(30_000).is_none());
    }

    #[test]
    fn disconnect_preserves_budget_and_success_resets_it() {
        let mut connection = MusicConnection::default();
        for now_ms in [0, 2_000] {
            let attempt = connection
                .begin_attempt_id(now_ms)
                .expect("bounded attempt");
            assert_eq!(
                connection.disconnected_for(attempt, now_ms),
                MusicConnectionCallback::Accepted
            );
        }
        let third = connection.begin_attempt_id(4_000).expect("third attempt");
        assert_eq!(
            connection.established_for_at(third, 4_000),
            MusicConnectionCallback::Accepted
        );
        assert_eq!(
            connection.disconnected_for(third, 6_000),
            MusicConnectionCallback::Accepted
        );
        assert!(connection.begin_attempt_id(7_999).is_none());
        assert!(connection.begin_attempt_id(8_000).is_some());
    }

    #[test]
    fn backwards_or_overflowing_clock_cannot_shorten_an_in_flight_attempt() {
        let mut connection = MusicConnection::default();
        assert!(connection.begin_attempt_id(u64::MAX - 1).is_some());
        assert!(connection.begin_attempt_id(0).is_none());
        assert!(connection.begin_attempt_id(u64::MAX).is_none());
    }

    #[test]
    fn stale_callbacks_do_not_extend_a_pending_retry_or_clear_a_new_attempt() {
        let mut connection = MusicConnection::default();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert_eq!(
            connection.failed_for(first, 100),
            MusicConnectionCallback::Accepted
        );
        assert_eq!(
            connection.failed_for(first, 200),
            MusicConnectionCallback::Stale
        );

        let second = connection.begin_attempt_id(2_100).expect("second attempt");
        assert_eq!(
            connection.failed_for(first, 2_200),
            MusicConnectionCallback::Stale
        );
        assert!(connection.begin_attempt_id(2_200).is_none());
        assert_eq!(
            connection.failed_for(second, 2_200),
            MusicConnectionCallback::Accepted
        );
        assert!(connection.begin_attempt_id(4_199).is_none());
        assert!(connection.begin_attempt_id(4_200).is_some());
    }

    #[test]
    fn stale_success_cannot_reset_a_new_attempt() {
        let mut connection = MusicConnection::default();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert_eq!(
            connection.failed_for(first, 100),
            MusicConnectionCallback::Accepted
        );
        let second = connection.begin_attempt_id(2_100).expect("second attempt");
        assert_eq!(
            connection.established_for_at(first, 2_100),
            MusicConnectionCallback::Stale
        );
        assert!(connection.begin_attempt_id(2_200).is_none());
        assert_eq!(
            connection.established_for_at(second, 2_100),
            MusicConnectionCallback::Accepted
        );
        assert!(connection.begin_attempt_id(2_200).is_some());
    }

    #[test]
    fn late_success_expires_current_attempt_and_schedules_recovery() {
        let mut connection = MusicConnection::default();
        let attempt = connection.begin_attempt_id(0).expect("attempt");
        assert_eq!(
            connection.expired_for(attempt, 10_000),
            MusicConnectionCallback::Accepted
        );
        assert_eq!(connection.classify(attempt), MusicConnectionCallback::Stale);
        assert!(connection.begin_attempt_id(11_999).is_none());
        assert!(connection.begin_attempt_id(12_000).is_some());
    }

    #[test]
    fn an_attempt_expires_before_a_late_success_callback() {
        let mut connection = MusicConnection::default();
        let attempt = connection.begin_attempt_id(0).expect("attempt");
        assert_eq!(
            connection.established_for_at(attempt, 10_000),
            MusicConnectionCallback::Stale
        );
    }

    #[test]
    fn disconnect_after_success_schedules_recovery_without_accepting_stale_events() {
        let mut connection = MusicConnection::default();
        let attempt = connection.begin_attempt_id(0).expect("attempt");
        assert_eq!(
            connection.established_for_at(attempt, 0),
            MusicConnectionCallback::Accepted
        );
        assert_eq!(
            connection.disconnected_for(attempt, 100),
            MusicConnectionCallback::Accepted
        );
        assert_eq!(
            connection.disconnected_for(attempt, 200),
            MusicConnectionCallback::Stale
        );
        assert!(connection.begin_attempt_id(2_099).is_none());
        assert!(connection.begin_attempt_id(2_100).is_some());
    }

    #[test]
    fn successful_connection_does_not_reuse_attempt_identity_on_recovery() {
        let mut connection = MusicConnection::default();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert_eq!(
            connection.established_for_at(first, 0),
            MusicConnectionCallback::Accepted
        );
        assert_eq!(
            connection.disconnected_for(first, 100),
            MusicConnectionCallback::Accepted
        );
        let second = connection
            .begin_attempt_id(2_100)
            .expect("recovery attempt");
        assert_ne!(first, second);
        assert_eq!(
            connection.failed_for(first, 2_200),
            MusicConnectionCallback::Stale
        );
        assert_eq!(
            connection.established_for_at(first, 2_200),
            MusicConnectionCallback::Stale
        );
        assert_eq!(
            connection.disconnected_for(first, 2_300),
            MusicConnectionCallback::Stale
        );
        assert_eq!(
            connection.established_for_at(second, 2_200),
            MusicConnectionCallback::Accepted
        );
    }

    #[test]
    fn attempt_identity_exhaustion_leaves_connection_state_unchanged() {
        let mut connection = MusicConnection {
            last_attempt_id: ConnectionAttemptId::from_raw(u64::MAX),
            ..MusicConnection::default()
        };
        let before = connection.attempts;

        assert_eq!(connection.begin_attempt_id(0), None);
        assert_eq!(connection.attempts, before);
        assert_eq!(connection.current_id(), None);
    }
}
