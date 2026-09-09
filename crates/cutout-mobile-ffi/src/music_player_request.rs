use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_music::player_request::{MusicPlayerRequest, MusicPlayerRequestCompletion};

/// Whether a player-state callback matched the current request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicPlayerRequestCompletion {
    /// The callback matched the current request.
    Accepted,
    /// The callback belonged to a timed-out or reset request.
    Stale,
}

/// Rust-owned correlation for SDK player-state requests and lost callbacks.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileMusicPlayerRequest {
    inner: Mutex<MusicPlayerRequest>,
}

#[uniffi::export]
impl MobileMusicPlayerRequest {
    /// Creates an idle player-state request boundary.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Starts a request or replaces one whose callback has not arrived in time.
    #[must_use]
    pub fn begin(&self, now_ms: u64) -> Option<u64> {
        self.lock_inner().begin(now_ms)
    }

    /// Whether the callback belongs to the current outstanding request.
    #[must_use]
    pub fn complete(&self, request_id: u64) -> MobileMusicPlayerRequestCompletion {
        match self.lock_inner().complete(request_id) {
            MusicPlayerRequestCompletion::Accepted => MobileMusicPlayerRequestCompletion::Accepted,
            MusicPlayerRequestCompletion::Stale => MobileMusicPlayerRequestCompletion::Stale,
        }
    }

    /// Records a verified player-state update using the platform monotonic clock.
    pub fn mark_observed(&self, now_ms: u64) {
        self.lock_inner().mark_observed(now_ms);
    }

    /// Whether the cached player state is older than the Rust freshness policy.
    #[must_use]
    pub fn is_stale(&self, now_ms: u64) -> bool {
        self.lock_inner().is_stale(now_ms)
    }

    /// Discards outstanding work after disconnect without reusing callback IDs.
    pub fn reset(&self) {
        self.lock_inner().reset();
    }
}

impl MobileMusicPlayerRequest {
    fn lock_inner(&self) -> MutexGuard<'_, MusicPlayerRequest> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::{MobileMusicPlayerRequest, MobileMusicPlayerRequestCompletion};

    #[test]
    fn binding_rejects_late_callbacks_after_timeout_and_reset() {
        let request = MobileMusicPlayerRequest::new();
        let first = request.begin(0).expect("first request");
        assert_eq!(request.begin(9_999), None);
        let retry = request.begin(10_000).expect("timed out");
        assert_eq!(
            request.complete(first),
            MobileMusicPlayerRequestCompletion::Stale
        );
        assert_eq!(
            request.complete(retry),
            MobileMusicPlayerRequestCompletion::Accepted
        );
        let old_connection = request.begin(10_001).expect("next poll");
        request.reset();
        let current = request.begin(10_002).expect("reconnected");
        assert_eq!(
            request.complete(old_connection),
            MobileMusicPlayerRequestCompletion::Stale
        );
        assert_eq!(
            request.complete(current),
            MobileMusicPlayerRequestCompletion::Accepted
        );
    }

    #[test]
    fn binding_expires_cached_observation_without_a_callback() {
        let request = MobileMusicPlayerRequest::new();
        assert!(!request.is_stale(30_000));
        request.mark_observed(1_000);
        assert!(!request.is_stale(31_000));
        assert!(request.is_stale(31_001));
        request.mark_observed(31_001);
        assert!(!request.is_stale(61_001));
        request.reset();
        assert!(!request.is_stale(u64::MAX));
    }
}
