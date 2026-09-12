//! Thin mobile binding for Rust-owned music connection retry admission.

use std::sync::{Mutex, PoisonError};

use cutout_music::connection::{MusicConnection, MusicConnectionCallback};

/// Whether a provider callback matched the current connection lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicConnectionCallback {
    /// The callback was accepted for the current attempt or session.
    Accepted,
    /// The callback belonged to an older or unknown attempt.
    Stale,
}

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
    #[must_use]
    pub fn established_for(&self, attempt_id: u64) -> MobileMusicConnectionCallback {
        map_callback(
            self.inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .established_for(attempt_id),
        )
    }

    /// Reports disconnection without discarding credentials or the attempt count.
    pub fn disconnected(&self, now_ms: u64) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .disconnected(now_ms);
    }

    /// Accepts disconnection only from the active attempt or connected session.
    #[must_use]
    pub fn disconnected_for(&self, attempt_id: u64, now_ms: u64) -> MobileMusicConnectionCallback {
        map_callback(
            self.inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .disconnected_for(attempt_id, now_ms),
        )
    }

    /// Reports a failed attempt without treating a transport error as auth failure.
    pub fn failed(&self, now_ms: u64) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failed(now_ms);
    }

    /// Accepts failure only from the active attempt or connected session.
    #[must_use]
    pub fn failed_for(&self, attempt_id: u64, now_ms: u64) -> MobileMusicConnectionCallback {
        map_callback(
            self.inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .failed_for(attempt_id, now_ms),
        )
    }
}

fn map_callback(callback: MusicConnectionCallback) -> MobileMusicConnectionCallback {
    match callback {
        MusicConnectionCallback::Accepted => MobileMusicConnectionCallback::Accepted,
        MusicConnectionCallback::Stale => MobileMusicConnectionCallback::Stale,
    }
}

#[cfg(test)]
mod tests {
    use super::{MobileMusicConnection, MobileMusicConnectionCallback};

    #[test]
    fn mobile_connection_preserves_bounded_domain_retry_behavior() {
        let connection = MobileMusicConnection::new();
        assert!(connection.begin_attempt_id(0).is_some());
        assert!(connection.begin_attempt_id(9_999).is_none());
        assert!(connection.begin_attempt_id(10_000).is_some());
        connection.failed(10_000);
        assert!(connection.begin_attempt_id(11_999).is_none());
        assert!(connection.begin_attempt_id(12_000).is_some());
        connection.disconnected(12_000);
        assert!(connection.begin_attempt_id(14_000).is_none());
        connection.established();
        assert!(connection.begin_attempt_id(14_000).is_some());
    }

    #[test]
    fn mobile_connection_rejects_stale_callbacks() {
        let connection = MobileMusicConnection::new();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert_eq!(
            connection.failed_for(first, 100),
            MobileMusicConnectionCallback::Accepted
        );
        let second = connection.begin_attempt_id(2_100).expect("second attempt");
        assert_eq!(
            connection.failed_for(first, 2_200),
            MobileMusicConnectionCallback::Stale
        );
        assert_eq!(
            connection.established_for(first),
            MobileMusicConnectionCallback::Stale
        );
        assert_eq!(
            connection.failed_for(second, 2_200),
            MobileMusicConnectionCallback::Accepted
        );
    }

    #[test]
    fn mobile_connection_preserves_attempt_identity_after_success() {
        let connection = MobileMusicConnection::new();
        let first = connection.begin_attempt_id(0).expect("first attempt");
        assert_eq!(
            connection.established_for(first),
            MobileMusicConnectionCallback::Accepted
        );
        assert_eq!(
            connection.disconnected_for(first, 100),
            MobileMusicConnectionCallback::Accepted
        );
        let second = connection
            .begin_attempt_id(2_100)
            .expect("recovery attempt");
        assert_ne!(first, second);
        assert_eq!(
            connection.failed_for(first, 2_200),
            MobileMusicConnectionCallback::Stale
        );
        assert_eq!(
            connection.established_for(second),
            MobileMusicConnectionCallback::Accepted
        );
    }
}
