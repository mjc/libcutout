use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_music::player_request::{
    MusicArtworkRequest, MusicPlayerRequest, MusicPlayerRequestCompletion,
};

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

/// Rust-owned bounded retry and identity policy for provider artwork requests.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileMusicArtworkRequest {
    inner: Mutex<MusicArtworkRequest>,
}

#[uniffi::export]
impl MobileMusicArtworkRequest {
    /// Creates a fresh artwork request budget.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts the next bounded attempt when no request is pending.
    #[must_use]
    pub fn begin(&self) -> Option<u64> {
        self.lock_inner().begin()
    }

    /// Accepts only the current callback or deadline.
    #[must_use]
    pub fn complete(&self, request_id: u64) -> MobileMusicPlayerRequestCompletion {
        match self.lock_inner().complete(request_id) {
            MusicPlayerRequestCompletion::Accepted => MobileMusicPlayerRequestCompletion::Accepted,
            MusicPlayerRequestCompletion::Stale => MobileMusicPlayerRequestCompletion::Stale,
        }
    }

    /// Whether another attempt remains in the current track budget.
    #[must_use]
    pub fn can_retry(&self) -> bool {
        self.lock_inner().can_retry()
    }

    /// Starts a new track or connection budget.
    pub fn reset(&self) {
        self.lock_inner().reset();
    }
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

    /// Starts or refreshes the player-state freshness window using the platform monotonic clock.
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

impl MobileMusicArtworkRequest {
    fn lock_inner(&self) -> MutexGuard<'_, MusicArtworkRequest> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl MobileMusicPlayerRequest {
    fn lock_inner(&self) -> MutexGuard<'_, MusicPlayerRequest> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MobileMusicArtworkRequest, MobileMusicPlayerRequest, MobileMusicPlayerRequestCompletion,
    };

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

    #[test]
    fn artwork_binding_bounds_retries_and_rejects_timed_out_callbacks() {
        let request = MobileMusicArtworkRequest::new();
        let timed_out = request.begin().expect("initial request");
        assert_eq!(
            request.complete(timed_out),
            MobileMusicPlayerRequestCompletion::Accepted
        );
        let replacement = request.begin().expect("retry");
        assert_eq!(
            request.complete(timed_out),
            MobileMusicPlayerRequestCompletion::Stale
        );
        assert_eq!(
            request.complete(replacement),
            MobileMusicPlayerRequestCompletion::Accepted
        );
        let final_attempt = request.begin().expect("final attempt");
        assert_eq!(
            request.complete(final_attempt),
            MobileMusicPlayerRequestCompletion::Accepted
        );
        assert_eq!(request.begin(), None);
    }
}
