//! Correlation and timeout admission for provider player-state callbacks.

use cutout_core::{Duration, MonotonicTimestamp};

use crate::ids::{ArtworkRequestId, ObservationRevision, PlayerStateRequestId};

const REQUEST_TIMEOUT: Duration = Duration::from_milliseconds(10_000);
const OBSERVATION_TIMEOUT: Duration = Duration::from_milliseconds(30_000);
const MAX_ARTWORK_ATTEMPTS: u8 = 3;

/// Result of applying a provider player-state callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicPlayerRequestCompletion {
    /// The callback matched the current request.
    Accepted,
    /// The callback belonged to a timed-out or reset request.
    Stale,
}

/// Result of applying a player-state request deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicPlayerRequestExpiration {
    /// The matching request has not reached its deadline.
    Pending,
    /// The matching request reached its deadline and was retired.
    Expired,
    /// The deadline belonged to a completed or replaced request.
    Stale,
}

#[derive(Clone, Copy, Debug)]
struct PendingPlayerStateRequest {
    id: PlayerStateRequestId,
    started_at: MonotonicTimestamp,
}

#[derive(Debug, Default)]
enum PlayerStateRequestState {
    #[default]
    Available,
    Pending(PendingPlayerStateRequest),
}

impl PlayerStateRequestState {
    const fn pending(&self) -> Option<PendingPlayerStateRequest> {
        match self {
            Self::Pending(pending) => Some(*pending),
            Self::Available => None,
        }
    }

    fn retire(&mut self, id: PlayerStateRequestId) -> Option<PendingPlayerStateRequest> {
        let pending = self.pending().filter(|pending| pending.id == id)?;
        *self = Self::Available;
        Some(pending)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservationRevisionState {
    Current(ObservationRevision),
    Exhausted,
}

impl Default for ObservationRevisionState {
    fn default() -> Self {
        Self::Current(ObservationRevision::default())
    }
}

impl ObservationRevisionState {
    const fn revision(self) -> ObservationRevision {
        match self {
            Self::Current(revision) => revision,
            Self::Exhausted => ObservationRevision::from_raw(u64::MAX),
        }
    }

    fn advance(&mut self) {
        let Self::Current(current) = *self else {
            return;
        };
        *self = match current.next() {
            Some(next) => Self::Current(next),
            None => Self::Exhausted,
        };
    }
}

/// One outstanding player-state request, replaceable if its callback is lost.
#[derive(Debug)]
pub struct MusicPlayerRequest {
    last_id: PlayerStateRequestId,
    state: PlayerStateRequestState,
    last_observed_at: Option<MonotonicTimestamp>,
    observation_revision: ObservationRevisionState,
}

/// Bounded identity and retry admission for provider artwork callbacks.
#[derive(Debug, Default)]
enum ArtworkRequestState {
    #[default]
    Available,
    Pending(ArtworkRequestId),
}

/// Bounded identity and retry admission for provider artwork callbacks.
#[derive(Debug)]
pub struct MusicArtworkRequest {
    last_id: ArtworkRequestId,
    attempts: u8,
    state: ArtworkRequestState,
}

impl Default for MusicPlayerRequest {
    fn default() -> Self {
        Self {
            last_id: PlayerStateRequestId::from_raw(0),
            state: PlayerStateRequestState::Available,
            last_observed_at: None,
            observation_revision: ObservationRevisionState::default(),
        }
    }
}

impl Default for MusicArtworkRequest {
    fn default() -> Self {
        Self {
            last_id: ArtworkRequestId::from_raw(0),
            attempts: 0,
            state: ArtworkRequestState::Available,
        }
    }
}

impl MusicArtworkRequest {
    /// Admits at most three requests until the track or connection is reset.
    #[must_use]
    pub fn begin(&mut self) -> Option<ArtworkRequestId> {
        if matches!(self.state, ArtworkRequestState::Pending(_))
            || self.attempts >= MAX_ARTWORK_ATTEMPTS
        {
            return None;
        }
        let id = self.last_id.next()?;
        self.last_id = id;
        self.attempts += 1;
        self.state = ArtworkRequestState::Pending(id);
        Some(id)
    }

    /// Accepts only the current image callback or deadline.
    #[must_use]
    pub fn complete(&mut self, request_id: ArtworkRequestId) -> MusicPlayerRequestCompletion {
        match self.state {
            ArtworkRequestState::Pending(id) if id == request_id => {
                self.state = ArtworkRequestState::Available;
                MusicPlayerRequestCompletion::Accepted
            }
            ArtworkRequestState::Available | ArtworkRequestState::Pending(_) => {
                MusicPlayerRequestCompletion::Stale
            }
        }
    }

    /// Whether another attempt remains after a failure or deadline.
    #[must_use]
    pub fn can_retry(&self) -> bool {
        matches!(self.state, ArtworkRequestState::Available) && self.attempts < MAX_ARTWORK_ATTEMPTS
    }

    /// Starts a new track or connection budget without reusing callback IDs.
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.state = ArtworkRequestState::Available;
    }
}

impl MusicPlayerRequest {
    /// Admits a request when none is pending or the previous request timed out.
    /// The platform supplies a monotonic timestamp and captures the returned ID.
    #[must_use]
    pub fn begin(&mut self, now_ms: u64) -> Option<PlayerStateRequestId> {
        let now = MonotonicTimestamp::new(now_ms);
        if let Some(pending) = self.state.pending()
            && now.saturating_duration_since(pending.started_at) < REQUEST_TIMEOUT
        {
            return None;
        }
        let id = self.last_id.next()?;
        self.last_id = id;
        self.state = PlayerStateRequestState::Pending(PendingPlayerStateRequest {
            id,
            started_at: now,
        });
        Some(id)
    }

    /// Accepts only the outstanding callback; late callbacks cannot clear a retry.
    #[must_use]
    pub fn complete(
        &mut self,
        request_id: PlayerStateRequestId,
        now_ms: u64,
    ) -> MusicPlayerRequestCompletion {
        let Some(pending) = self.state.retire(request_id) else {
            return MusicPlayerRequestCompletion::Stale;
        };
        if MonotonicTimestamp::new(now_ms).saturating_duration_since(pending.started_at)
            < REQUEST_TIMEOUT
        {
            MusicPlayerRequestCompletion::Accepted
        } else {
            MusicPlayerRequestCompletion::Stale
        }
    }

    /// Retires only a matching request whose deadline has elapsed.
    #[must_use]
    pub fn expire(
        &mut self,
        request_id: PlayerStateRequestId,
        now_ms: u64,
    ) -> MusicPlayerRequestExpiration {
        let Some(pending) = self
            .state
            .pending()
            .filter(|pending| pending.id == request_id)
        else {
            return MusicPlayerRequestExpiration::Stale;
        };
        if MonotonicTimestamp::new(now_ms).saturating_duration_since(pending.started_at)
            < REQUEST_TIMEOUT
        {
            return MusicPlayerRequestExpiration::Pending;
        }
        let _ = self.state.retire(request_id);
        MusicPlayerRequestExpiration::Expired
    }

    /// Completes a poll only when no newer push observation superseded it.
    #[must_use]
    pub fn complete_if_current(
        &mut self,
        request_id: PlayerStateRequestId,
        observation_revision: ObservationRevision,
        now_ms: u64,
    ) -> MusicPlayerRequestCompletion {
        if self.observation_revision != ObservationRevisionState::Current(observation_revision) {
            let _ = self.state.retire(request_id);
            return MusicPlayerRequestCompletion::Stale;
        }
        self.complete(request_id, now_ms)
    }

    /// Starts or refreshes the monotonic freshness window for this session.
    /// Connected sessions seed it before the first player-state response;
    /// verified updates then refresh it as they arrive.
    pub fn mark_observed(&mut self, now_ms: u64) {
        self.observation_revision.advance();
        self.last_observed_at = Some(MonotonicTimestamp::new(now_ms));
    }

    /// Revision of the latest authoritative observation.
    #[must_use]
    pub const fn observation_revision(&self) -> ObservationRevision {
        self.observation_revision.revision()
    }

    /// Whether the most recently verified player state is too old to present
    /// as current. An idle request has no observation to expire.
    #[must_use]
    pub fn is_stale(&self, now_ms: u64) -> bool {
        self.last_observed_at.is_some_and(|observed_at| {
            MonotonicTimestamp::new(now_ms).saturating_duration_since(observed_at)
                > OBSERVATION_TIMEOUT
        })
    }

    /// Invalidates outstanding work without reusing IDs from the old connection.
    pub fn reset(&mut self) {
        self.state = PlayerStateRequestState::Available;
        self.last_observed_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{MusicArtworkRequest, MusicPlayerRequest, MusicPlayerRequestCompletion};

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

    #[test]
    fn artwork_requests_exhaust_one_bounded_budget() {
        let mut request = MusicArtworkRequest::default();
        for _ in 0..3 {
            let id = request.begin().expect("bounded attempt");
            assert_eq!(request.complete(id), MusicPlayerRequestCompletion::Accepted);
        }
        assert_eq!(request.begin(), None);
        assert!(!request.can_retry());
        assert_eq!(request.begin(), None);
    }

    #[test]
    fn artwork_deadline_and_reset_reject_late_callbacks() {
        let mut request = MusicArtworkRequest::default();
        let timed_out = request.begin().expect("initial attempt");
        assert_eq!(
            request.complete(timed_out),
            MusicPlayerRequestCompletion::Accepted
        );
        let replacement = request.begin().expect("retry");
        assert_eq!(
            request.complete(timed_out),
            MusicPlayerRequestCompletion::Stale
        );
        request.reset();
        let next_track = request.begin().expect("new track");
        assert_eq!(
            request.complete(replacement),
            MusicPlayerRequestCompletion::Stale
        );
        assert_eq!(
            request.complete(next_track),
            MusicPlayerRequestCompletion::Accepted
        );
    }
}
