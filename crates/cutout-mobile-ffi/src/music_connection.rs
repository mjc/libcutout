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

    /// Reports successful connection and resets the retry budget.
    pub fn established(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .established();
    }

    /// Reports disconnection without discarding credentials or the attempt count.
    pub fn disconnected(&self, now_ms: u64) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .disconnected(now_ms);
    }

    /// Reports a failed attempt without treating a transport error as auth failure.
    pub fn failed(&self, now_ms: u64) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failed(now_ms);
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
}
