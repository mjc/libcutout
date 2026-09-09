//! Thin mobile binding for Rust-owned music connection retry admission.

use std::sync::{Mutex, PoisonError};

use cutout_music::connection::MusicConnection;

/// Bounded foreground provider connection attempts; never stores credentials.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileMusicConnection {
    inner: Mutex<MusicConnection>,
}

#[uniffi::export]
impl MobileMusicConnection {
    /// Starts a new explicit monitoring session's retry budget.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the SDK may attempt connection at this monotonic millisecond time.
    #[must_use]
    pub fn begin_attempt(&self, now_ms: u64) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .begin_attempt(now_ms)
    }

    /// Starts an attempt and returns its identity for SDK callback validation.
    #[must_use]
    pub fn begin_attempt_id(&self, now_ms: u64) -> Option<u64> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .begin_attempt_id(now_ms)
    }

    /// Reports successful connection and resets the retry budget.
    pub fn established(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .established();
    }

    /// Accepts success only from the currently active attempt.
    pub fn established_for(&self, attempt_id: u64) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .established_for(attempt_id)
    }

    /// Reports disconnection without discarding credentials or the attempt count.
    pub fn disconnected(&self, now_ms: u64) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .disconnected(now_ms);
    }

    /// Accepts disconnection only from the currently active attempt.
    pub fn disconnected_for(&self, attempt_id: u64, now_ms: u64) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .disconnected_for(attempt_id, now_ms)
    }

    /// Reports a failed attempt without treating a transport error as auth failure.
    pub fn failed(&self, now_ms: u64) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failed(now_ms);
    }

    /// Accepts failure only from the currently active attempt.
    pub fn failed_for(&self, attempt_id: u64, now_ms: u64) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failed_for(attempt_id, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::MobileMusicConnection;

    #[test]
    fn mobile_connection_preserves_bounded_domain_retry_behavior() {
        let connection = MobileMusicConnection::new();
        assert!(connection.begin_attempt(0));
        assert!(!connection.begin_attempt(9_999));
        assert!(connection.begin_attempt(10_000));
        connection.failed(10_000);
        assert!(!connection.begin_attempt(11_999));
        assert!(connection.begin_attempt(12_000));
        connection.disconnected(12_000);
        assert!(!connection.begin_attempt(14_000));
        connection.established();
        assert!(connection.begin_attempt(14_000));
    }

    #[test]
    fn mobile_connection_rejects_stale_callbacks() {
        let connection = MobileMusicConnection::new();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert!(connection.failed_for(first, 100));
        let second = connection.begin_attempt_id(2_100).expect("second attempt");
        assert!(!connection.failed_for(first, 2_200));
        assert!(!connection.established_for(first));
        assert!(connection.failed_for(second, 2_200));
    }
}
