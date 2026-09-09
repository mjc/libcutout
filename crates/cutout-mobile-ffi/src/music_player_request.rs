use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_music::player_request::MusicPlayerRequest;

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
    pub fn complete(&self, request_id: u64) -> bool {
        self.lock_inner().complete(request_id)
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
    use super::MobileMusicPlayerRequest;

    #[test]
    fn binding_rejects_late_callbacks_after_timeout_and_reset() {
        let request = MobileMusicPlayerRequest::new();
        let first = request.begin(0).expect("first request");
        assert_eq!(request.begin(9_999), None);
        let retry = request.begin(10_000).expect("timed out");
        assert!(!request.complete(first));
        assert!(request.complete(retry));
        let old_connection = request.begin(10_001).expect("next poll");
        request.reset();
        let current = request.begin(10_002).expect("reconnected");
        assert!(!request.complete(old_connection));
        assert!(request.complete(current));
    }
}
