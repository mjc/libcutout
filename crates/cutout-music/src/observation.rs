use std::collections::VecDeque;

use cutout_core::{Duration, MonotonicTimestamp};

use crate::{
    MusicPlaybackState, MusicProvider, MusicRideEventKind, MusicSnapshot, ids::TransportRequestId,
};

const SKIP_CONFIRMATION_MAX_AGE: Duration = Duration::from_milliseconds(5_000);
const SKIP_CONFIRMATION_MAX_UNCHANGED_OBSERVATIONS: u8 = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingSkip {
    transport_id: TransportRequestId,
    issued_at: MonotonicTimestamp,
    remaining_unchanged_observations: u8,
}

impl PendingSkip {
    fn applies_at(&self, observed_at: MonotonicTimestamp) -> bool {
        observed_at >= self.issued_at
    }

    fn is_expired_at(&self, observed_at: MonotonicTimestamp) -> bool {
        observed_at >= self.issued_at
            && observed_at.saturating_duration_since(self.issued_at) > SKIP_CONFIRMATION_MAX_AGE
    }
}

/// A canonical provider observation and its optional ride-history transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicObservationDecision {
    snapshot: MusicSnapshot,
    transition: Option<MusicRideEventKind>,
}

impl MusicObservationDecision {
    /// Returns the canonical observation accepted by the tracker.
    #[must_use]
    pub const fn snapshot(&self) -> &MusicSnapshot {
        &self.snapshot
    }

    /// Returns the meaningful transition produced by this observation.
    #[must_use]
    pub const fn transition(&self) -> Option<MusicRideEventKind> {
        self.transition
    }
}

/// Result of ordering and classifying one canonical provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MusicObservationOutcome {
    /// The observation advanced its provider's monotonic watermark.
    Accepted(MusicObservationDecision),
    /// The observation was not newer than that provider's current value.
    OutOfOrder,
}

/// Owns provider observation ordering and command-to-transition correlation.
///
/// Platform adapters report canonical observations and accepted previous/next
/// commands. They do not choose ride-history transition kinds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MusicObservationTracker {
    apple_music: Option<MusicSnapshot>,
    spotify: Option<MusicSnapshot>,
    pending_skips: VecDeque<PendingSkip>,
}

impl MusicObservationTracker {
    /// Creates an empty observation tracker.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            apple_music: None,
            spotify: None,
            pending_skips: VecDeque::new(),
        }
    }

    pub(crate) fn issue_skip(
        &mut self,
        transport_id: TransportRequestId,
        issued_at: MonotonicTimestamp,
    ) {
        self.pending_skips.push_back(PendingSkip {
            transport_id,
            issued_at,
            remaining_unchanged_observations: SKIP_CONFIRMATION_MAX_UNCHANGED_OBSERVATIONS,
        });
    }

    pub(crate) fn cancel_skip(&mut self, transport_id: TransportRequestId) {
        self.pending_skips
            .retain(|pending| pending.transport_id != transport_id);
    }

    /// Clears observations and pending command correlation.
    pub(crate) fn reset(&mut self) {
        self.reset_observations();
        self.clear_pending_skips();
    }

    pub(crate) fn clear_pending_skips(&mut self) {
        self.pending_skips.clear();
    }

    /// Clears observation baselines while retaining pending command correlation.
    pub(crate) fn reset_observations(&mut self) {
        self.apple_music = None;
        self.spotify = None;
    }

    /// Orders and classifies one canonical provider observation.
    pub(crate) fn observe(&mut self, snapshot: MusicSnapshot) -> MusicObservationOutcome {
        let previous = self.latest(snapshot.provider()).cloned();
        if previous
            .as_ref()
            .is_some_and(|previous| previous.observed_at() >= snapshot.observed_at())
        {
            return MusicObservationOutcome::OutOfOrder;
        }

        self.expire_skips(snapshot.observed_at());
        let skip_applies = self
            .pending_skips
            .front()
            .is_some_and(|pending| pending.applies_at(snapshot.observed_at()));
        let transition = classify_transition(previous.as_ref(), &snapshot, skip_applies);
        self.resolve_skip(&snapshot, skip_applies, transition);
        self.replace_latest(snapshot.clone());

        MusicObservationOutcome::Accepted(MusicObservationDecision {
            snapshot,
            transition,
        })
    }

    fn latest(&self, provider: MusicProvider) -> Option<&MusicSnapshot> {
        match provider {
            MusicProvider::AppleMusic => self.apple_music.as_ref(),
            MusicProvider::Spotify => self.spotify.as_ref(),
        }
    }

    fn replace_latest(&mut self, snapshot: MusicSnapshot) {
        match snapshot.provider() {
            MusicProvider::AppleMusic => self.apple_music = Some(snapshot),
            MusicProvider::Spotify => self.spotify = Some(snapshot),
        }
    }

    fn expire_skips(&mut self, observed_at: MonotonicTimestamp) {
        self.pending_skips
            .retain(|pending| !pending.is_expired_at(observed_at));
    }

    fn resolve_skip(
        &mut self,
        current: &MusicSnapshot,
        skip_applies: bool,
        transition: Option<MusicRideEventKind>,
    ) {
        if !skip_applies {
            return;
        }
        if transition == Some(MusicRideEventKind::Skip) || is_terminal_state(current.state()) {
            self.pending_skips.pop_front();
            return;
        }
        let Some(pending) = self.pending_skips.front_mut() else {
            return;
        };
        pending.remaining_unchanged_observations =
            pending.remaining_unchanged_observations.saturating_sub(1);
        if pending.remaining_unchanged_observations == 0 {
            self.pending_skips.pop_front();
        }
    }
}

fn classify_transition(
    previous: Option<&MusicSnapshot>,
    current: &MusicSnapshot,
    skip_applies: bool,
) -> Option<MusicRideEventKind> {
    if current.state() == MusicPlaybackState::Disconnected {
        return previous
            .is_none_or(|previous| previous.state() != MusicPlaybackState::Disconnected)
            .then_some(MusicRideEventKind::ProviderDisconnected);
    }
    if matches!(
        current.state(),
        MusicPlaybackState::Unauthorized
            | MusicPlaybackState::Unavailable
            | MusicPlaybackState::Stale
    ) {
        return None;
    }
    let Some(previous) = previous else {
        return current.item().map(|_| MusicRideEventKind::ItemChanged);
    };
    if previous.provider() != current.provider() {
        return Some(MusicRideEventKind::ItemChanged);
    }
    if current.state() == MusicPlaybackState::Stopped
        && previous.state() != MusicPlaybackState::Stopped
    {
        return Some(MusicRideEventKind::Stopped);
    }
    let previous_item = previous.item().map(crate::MusicItem::identifier);
    let current_item = current.item().map(crate::MusicItem::identifier);
    if previous_item != current_item {
        return Some(
            if skip_applies && previous_item.is_some() && current_item.is_some() {
                MusicRideEventKind::Skip
            } else {
                MusicRideEventKind::ItemChanged
            },
        );
    }
    if skip_applies
        && previous_item.is_some()
        && current_item.is_some()
        && previous
            .position_milliseconds()
            .zip(current.position_milliseconds())
            .is_some_and(|(previous, current)| current < previous)
    {
        return Some(MusicRideEventKind::Skip);
    }
    match (previous.state(), current.state()) {
        (_, MusicPlaybackState::Playing) if previous.state() != MusicPlaybackState::Playing => {
            Some(MusicRideEventKind::Play)
        }
        (_, MusicPlaybackState::Paused) if previous.state() != MusicPlaybackState::Paused => {
            Some(MusicRideEventKind::Pause)
        }
        _ => None,
    }
}

const fn is_terminal_state(state: MusicPlaybackState) -> bool {
    matches!(
        state,
        MusicPlaybackState::Stopped
            | MusicPlaybackState::Unauthorized
            | MusicPlaybackState::Unavailable
            | MusicPlaybackState::Disconnected
            | MusicPlaybackState::Stale
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MusicCapabilities, MusicItem, MusicPlaybackPosition, MusicValidationError};

    fn snapshot(
        provider: MusicProvider,
        item: Option<&str>,
        state: MusicPlaybackState,
        position: Option<u64>,
        observed_at: u64,
    ) -> MusicSnapshot {
        MusicSnapshot::new(
            provider,
            "session",
            state,
            item.map(|identifier| MusicItem::new(identifier, None, None).expect("valid item")),
            MusicPlaybackPosition::new(position, Some(100_000)).expect("valid position"),
            MonotonicTimestamp::new(observed_at),
            MusicCapabilities::new(),
        )
        .expect("valid snapshot")
    }

    #[test]
    fn same_item_rewind_after_skip_is_classified_from_latest_observation() {
        let mut tracker = MusicObservationTracker::new();
        let _ = tracker.observe(snapshot(
            MusicProvider::AppleMusic,
            Some("track"),
            MusicPlaybackState::Playing,
            Some(0),
            100,
        ));
        let _ = tracker.observe(snapshot(
            MusicProvider::AppleMusic,
            Some("track"),
            MusicPlaybackState::Playing,
            Some(30_000),
            200,
        ));
        tracker.issue_skip(
            TransportRequestId::from_raw(1),
            MonotonicTimestamp::new(250),
        );

        let MusicObservationOutcome::Accepted(decision) = tracker.observe(snapshot(
            MusicProvider::AppleMusic,
            Some("track"),
            MusicPlaybackState::Playing,
            Some(1_000),
            300,
        )) else {
            panic!("new observation must be accepted");
        };
        assert_eq!(decision.transition(), Some(MusicRideEventKind::Skip));
    }

    #[test]
    fn observation_before_command_does_not_consume_skip() {
        let mut tracker = MusicObservationTracker::new();
        tracker.issue_skip(
            TransportRequestId::from_raw(1),
            MonotonicTimestamp::new(200),
        );
        let MusicObservationOutcome::Accepted(before) = tracker.observe(snapshot(
            MusicProvider::AppleMusic,
            Some("before"),
            MusicPlaybackState::Playing,
            None,
            100,
        )) else {
            panic!("observation must be accepted");
        };
        assert_eq!(before.transition(), Some(MusicRideEventKind::ItemChanged));

        let MusicObservationOutcome::Accepted(after) = tracker.observe(snapshot(
            MusicProvider::AppleMusic,
            Some("after"),
            MusicPlaybackState::Playing,
            None,
            300,
        )) else {
            panic!("observation must be accepted");
        };
        assert_eq!(after.transition(), Some(MusicRideEventKind::Skip));
    }

    #[test]
    fn initial_disconnect_is_a_disconnect_with_or_without_an_item() {
        for item in [Some("track"), None] {
            let mut tracker = MusicObservationTracker::new();
            let MusicObservationOutcome::Accepted(decision) = tracker.observe(snapshot(
                MusicProvider::AppleMusic,
                item,
                MusicPlaybackState::Disconnected,
                None,
                100,
            )) else {
                panic!("observation must be accepted");
            };
            assert_eq!(
                decision.transition(),
                Some(MusicRideEventKind::ProviderDisconnected)
            );
        }
    }

    #[test]
    fn blank_optional_text_is_normalized_by_the_domain_constructor() {
        let item = MusicItem::new("track", Some("  ".to_owned()), Some(String::new()))
            .expect("blank optional text is absent");
        assert_eq!(item.title(), None);
        assert_eq!(item.artist(), None);
    }

    #[test]
    fn invalid_snapshot_fixture_remains_a_domain_error() {
        assert_eq!(
            MusicPlaybackPosition::new(Some(2), Some(1)),
            Err(MusicValidationError::PositionAfterDuration)
        );
    }
}
