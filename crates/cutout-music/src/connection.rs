//! Bounded connection attempts for a foreground music provider session.

const MAXIMUM_ATTEMPTS: u8 = 3;
const RETRY_DELAY_MS: u64 = 2_000;
const ATTEMPT_TIMEOUT_MS: u64 = 10_000;

/// Retry admission, without credential ownership or platform SDK side effects.
///
/// A new explicit monitoring session creates a fresh policy. Failures and
/// disconnects retain the attempt budget; only a successful connection resets it.
#[derive(Debug, Default)]
pub struct MusicConnection {
    attempts: u8,
    in_flight_since: Option<u64>,
    retry_at: u64,
    next_attempt_id: u64,
    active_attempt_id: Option<u64>,
    connected_id: Option<u64>,
}

impl MusicConnection {
    /// Admits at most three attempts, including ones whose SDK callback is lost.
    /// `now_ms` must use the same monotonic clock throughout this session.
    #[must_use]
    pub fn begin_attempt(&mut self, now_ms: u64) -> bool {
        self.begin_attempt_id(now_ms).is_some()
    }

    /// Starts an attempt and returns its identity for callback validation.
    #[must_use]
    pub fn begin_attempt_id(&mut self, now_ms: u64) -> Option<u64> {
        if self.attempts >= MAXIMUM_ATTEMPTS || now_ms < self.retry_at {
            return None;
        }
        if let Some(started_at) = self.in_flight_since
            && now_ms.saturating_sub(started_at) < ATTEMPT_TIMEOUT_MS
        {
            return None;
        }
        self.attempts += 1;
        self.in_flight_since = Some(now_ms);
        let attempt_id = self.next_attempt_id.max(1);
        self.next_attempt_id = attempt_id.wrapping_add(1).max(1);
        self.active_attempt_id = Some(attempt_id);
        self.connected_id = None;
        Some(attempt_id)
    }

    /// Resets the retry budget after the provider confirms connection.
    pub fn established(&mut self) {
        *self = Self::default();
    }

    /// Schedules recovery after a disconnect without granting additional attempts.
    pub fn disconnected(&mut self, now_ms: u64) {
        if let Some(attempt_id) = self.active_attempt_id.or(self.connected_id) {
            let _ = self.disconnected_for(attempt_id, now_ms);
        } else {
            self.schedule_retry(now_ms);
        }
    }

    /// Schedules a retry; a connection error does not imply rejected credentials.
    pub fn failed(&mut self, now_ms: u64) {
        if let Some(attempt_id) = self.active_attempt_id.or(self.connected_id) {
            let _ = self.failed_for(attempt_id, now_ms);
        } else {
            self.schedule_retry(now_ms);
        }
    }

    /// Accepts a failure only from the current attempt or connected session.
    pub fn failed_for(&mut self, attempt_id: u64, now_ms: u64) -> bool {
        if self.active_attempt_id != Some(attempt_id) && self.connected_id != Some(attempt_id) {
            return false;
        }
        self.in_flight_since = None;
        self.active_attempt_id = None;
        self.connected_id = None;
        self.schedule_retry(now_ms);
        true
    }

    fn schedule_retry(&mut self, now_ms: u64) {
        self.retry_at = self.retry_at.max(now_ms.saturating_add(RETRY_DELAY_MS));
    }

    /// Accepts a disconnect only from the current attempt or connected session.
    pub fn disconnected_for(&mut self, attempt_id: u64, now_ms: u64) -> bool {
        self.failed_for(attempt_id, now_ms)
    }

    /// Accepts success only from the currently active attempt.
    pub fn established_for(&mut self, attempt_id: u64) -> bool {
        if self.active_attempt_id != Some(attempt_id) {
            return false;
        }
        self.established();
        self.connected_id = Some(attempt_id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::MusicConnection;

    #[test]
    fn a_failed_attempt_retries_after_delay_without_resetting_budget() {
        let mut connection = MusicConnection::default();
        assert!(connection.begin_attempt(0));
        connection.failed(100);
        assert!(!connection.begin_attempt(2_099));
        assert!(connection.begin_attempt(2_100));
        connection.failed(2_200);
        assert!(connection.begin_attempt(4_200));
        connection.failed(4_300);
        assert!(!connection.begin_attempt(100_000));
    }

    #[test]
    fn a_lost_callback_times_out_but_attempts_remain_bounded() {
        let mut connection = MusicConnection::default();
        assert!(connection.begin_attempt(0));
        assert!(!connection.begin_attempt(9_999));
        assert!(connection.begin_attempt(10_000));
        assert!(!connection.begin_attempt(19_999));
        assert!(connection.begin_attempt(20_000));
        assert!(!connection.begin_attempt(30_000));
    }

    #[test]
    fn disconnect_preserves_budget_and_established_resets_it() {
        let mut connection = MusicConnection::default();
        for now_ms in [0, 2_000, 4_000] {
            assert!(connection.begin_attempt(now_ms));
            connection.disconnected(now_ms);
        }
        assert!(!connection.begin_attempt(6_000));
        connection.established();
        connection.disconnected(6_000);
        assert!(!connection.begin_attempt(7_999));
        assert!(connection.begin_attempt(8_000));
    }

    #[test]
    fn backwards_or_overflowing_clock_cannot_shorten_an_in_flight_attempt() {
        let mut connection = MusicConnection::default();
        assert!(connection.begin_attempt(u64::MAX - 1));
        assert!(!connection.begin_attempt(0));
        assert!(!connection.begin_attempt(u64::MAX));
    }

    #[test]
    fn stale_callbacks_do_not_extend_a_pending_retry_or_clear_a_new_attempt() {
        let mut connection = MusicConnection::default();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert!(connection.failed_for(first, 100));
        assert!(!connection.failed_for(first, 200));

        let second = connection.begin_attempt_id(2_100).expect("second attempt");
        assert!(!connection.failed_for(first, 2_200));
        assert!(!connection.begin_attempt_id(2_200).is_some());
        assert!(connection.failed_for(second, 2_200));
        assert!(!connection.begin_attempt_id(4_199).is_some());
        assert!(connection.begin_attempt_id(4_200).is_some());
    }

    #[test]
    fn stale_success_cannot_reset_a_new_attempt() {
        let mut connection = MusicConnection::default();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert!(connection.failed_for(first, 100));
        let second = connection.begin_attempt_id(2_100).expect("second attempt");
        assert!(!connection.established_for(first));
        assert!(!connection.begin_attempt_id(2_200).is_some());
        assert!(connection.established_for(second));
        assert!(connection.begin_attempt_id(2_200).is_some());
    }

    #[test]
    fn disconnect_after_success_schedules_recovery_without_accepting_stale_events() {
        let mut connection = MusicConnection::default();
        let attempt = connection.begin_attempt_id(0).expect("attempt");
        assert!(connection.established_for(attempt));
        assert!(connection.disconnected_for(attempt, 100));
        assert!(!connection.disconnected_for(attempt, 200));
        assert!(!connection.begin_attempt_id(2_099).is_some());
        assert!(connection.begin_attempt_id(2_100).is_some());
    }
}
