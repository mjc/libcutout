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
}

impl MusicConnection {
    /// Admits at most three attempts, including ones whose SDK callback is lost.
    /// `now_ms` must use the same monotonic clock throughout this session.
    #[must_use]
    pub fn begin_attempt(&mut self, now_ms: u64) -> bool {
        if self.attempts >= MAXIMUM_ATTEMPTS || now_ms < self.retry_at {
            return false;
        }
        if let Some(started_at) = self.in_flight_since
            && now_ms.saturating_sub(started_at) < ATTEMPT_TIMEOUT_MS
        {
            return false;
        }
        self.attempts += 1;
        self.in_flight_since = Some(now_ms);
        true
    }

    /// Resets the retry budget after the provider confirms connection.
    pub fn established(&mut self) {
        *self = Self::default();
    }

    /// Schedules recovery after a disconnect without granting additional attempts.
    pub fn disconnected(&mut self, now_ms: u64) {
        self.failed(now_ms);
    }

    /// Schedules a retry; a connection error does not imply rejected credentials.
    pub fn failed(&mut self, now_ms: u64) {
        self.in_flight_since = None;
        self.retry_at = now_ms.saturating_add(RETRY_DELAY_MS);
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
}
