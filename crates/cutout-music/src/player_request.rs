//! Correlation and timeout admission for provider player-state callbacks.

const REQUEST_TIMEOUT_MS: u64 = 10_000;
const OBSERVATION_TIMEOUT_MS: u64 = 30_000;

/// Result of applying a provider player-state callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicPlayerRequestCompletion {
    /// The callback matched the current request.
    Accepted,
    /// The callback belonged to a timed-out or reset request.
    Stale,
}

/// One outstanding player-state request, replaceable if its callback is lost.
#[derive(Debug, Default)]
pub struct MusicPlayerRequest {
    last_id: u64,
    pending: Option<(u64, u64)>,
    last_observed_at: Option<u64>,
}

impl MusicPlayerRequest {
    /// Admits a request when none is pending or the previous request timed out.
    /// The platform supplies a monotonic timestamp and captures the returned ID.
    #[must_use]
    pub fn begin(&mut self, now_ms: u64) -> Option<u64> {
        if let Some((_, started_at)) = self.pending
            && now_ms.saturating_sub(started_at) < REQUEST_TIMEOUT_MS
        {
            return None;
        }
        self.last_id = self.last_id.checked_add(1)?;
        self.pending = Some((self.last_id, now_ms));
        Some(self.last_id)
    }

    /// Accepts only the outstanding callback; late callbacks cannot clear a retry.
    #[must_use]
    pub fn complete(&mut self, request_id: u64) -> MusicPlayerRequestCompletion {
        if self.pending.is_some_and(|(id, _)| id == request_id) {
            self.pending = None;
            MusicPlayerRequestCompletion::Accepted
        } else {
            MusicPlayerRequestCompletion::Stale
        }
    }

    /// Starts or refreshes the monotonic freshness window for this session.
    /// Connected sessions seed it before the first player-state response;
    /// verified updates then refresh it as they arrive.
    pub fn mark_observed(&mut self, now_ms: u64) {
        self.last_observed_at = Some(now_ms);
    }

    /// Whether the most recently verified player state is too old to present
    /// as current. An idle request has no observation to expire.
    #[must_use]
    pub fn is_stale(&self, now_ms: u64) -> bool {
        self.last_observed_at
            .is_some_and(|observed_at| now_ms.saturating_sub(observed_at) > OBSERVATION_TIMEOUT_MS)
    }

    /// Invalidates outstanding work without reusing IDs from the old connection.
    pub fn reset(&mut self) {
        self.pending = None;
        self.last_observed_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::MusicPlayerRequest;

    #[test]
    fn verified_observation_expires_without_a_callback() {
        let mut request = MusicPlayerRequest::default();
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
