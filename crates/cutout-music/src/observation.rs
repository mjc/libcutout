use std::collections::VecDeque;

use cutout_core::{Duration, MonotonicTimestamp, WallClockUnixTimestamp};

use crate::{
    MusicPlaybackState, MusicProvider, MusicRideEventKind, MusicSnapshot,
    ids::{HistoryTransitionId, TransportRequestId},
};

const SKIP_CONFIRMATION_MAX_AGE: Duration = Duration::from_milliseconds(5_000);
const SKIP_CONFIRMATION_MAX_UNCHANGED_OBSERVATIONS: u8 = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingSkip {
    transport_id: TransportRequestId,
    issued_at: MonotonicTimestamp,
    remaining_unchanged_observations: u8,
    outcome: SkipCommandOutcome,
    matched_observation: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SkipCommandOutcome {
    Pending,
    Accepted,
    Rejected,
}

/// Wall-clock correlation retained with a history transition until durable acknowledgement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicObservationTiming {
    wall_clock_at: WallClockUnixTimestamp,
    clock_uncertainty_milliseconds: u64,
}

impl MusicObservationTiming {
    /// Creates timing metadata for one provider observation.
    #[must_use]
    pub const fn new(
        wall_clock_at: WallClockUnixTimestamp,
        clock_uncertainty_milliseconds: u64,
    ) -> Self {
        Self {
            wall_clock_at,
            clock_uncertainty_milliseconds,
        }
    }

    /// Returns the correlated wall-clock time.
    #[must_use]
    pub const fn wall_clock_at(self) -> WallClockUnixTimestamp {
        self.wall_clock_at
    }

    /// Returns the bounded host clock uncertainty.
    #[must_use]
    pub const fn clock_uncertainty_milliseconds(self) -> u64 {
        self.clock_uncertainty_milliseconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PendingHistoryKind {
    Confirmed(MusicRideEventKind),
    AwaitingSkip {
        transport_id: TransportRequestId,
        rejected_kind: Option<MusicRideEventKind>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingHistoryTransition {
    id: HistoryTransitionId,
    snapshot: MusicSnapshot,
    kind: PendingHistoryKind,
    timing: MusicObservationTiming,
}

/// A classified transition retained until the durable history owner acknowledges it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicHistoryTransition {
    id: HistoryTransitionId,
    snapshot: MusicSnapshot,
    kind: MusicRideEventKind,
    timing: MusicObservationTiming,
}

impl MusicHistoryTransition {
    /// Returns the Rust-issued durable acknowledgement identity.
    #[must_use]
    pub const fn id(&self) -> HistoryTransitionId {
        self.id
    }

    /// Returns the observation that produced this transition.
    #[must_use]
    pub const fn snapshot(&self) -> &MusicSnapshot {
        &self.snapshot
    }

    /// Returns the final, command-confirmed transition kind.
    #[must_use]
    pub const fn kind(&self) -> MusicRideEventKind {
        self.kind
    }

    /// Returns the original observation timing.
    #[must_use]
    pub const fn timing(&self) -> MusicObservationTiming {
        self.timing
    }
}

/// Result of acknowledging a history transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicHistoryTransitionAcknowledgement {
    /// The current transition was durably handled and removed.
    Acknowledged,
    /// The identity was already handled, not ready, or superseded.
    Stale,
}

/// A canonical provider observation and its optional ride-history transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicObservationDecision {
    snapshot: MusicSnapshot,
    history_transition: Option<MusicHistoryTransition>,
}

impl MusicObservationDecision {
    /// Returns the canonical observation accepted by the tracker.
    #[must_use]
    pub const fn snapshot(&self) -> &MusicSnapshot {
        &self.snapshot
    }

    /// Returns the oldest command-confirmed transition awaiting durable acknowledgement.
    #[must_use]
    pub const fn history_transition(&self) -> Option<&MusicHistoryTransition> {
        self.history_transition.as_ref()
    }
}

/// Result of ordering and classifying one canonical provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MusicObservationOutcome {
    /// The observation advanced its provider's monotonic watermark.
    Accepted(Box<MusicObservationDecision>),
    /// The observation was not newer than that provider's current value.
    OutOfOrder,
}

/// Owns provider observation ordering and command-to-transition correlation.
///
/// Platform adapters report canonical observations and accepted previous/next
/// commands. They do not choose ride-history transition kinds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MusicObservationTracker {
    apple_music: Option<MusicSnapshot>,
    spotify: Option<MusicSnapshot>,
    pending_skips: VecDeque<PendingSkip>,
    pending_history: VecDeque<PendingHistoryTransition>,
    last_history_id: HistoryTransitionId,
}

impl Default for MusicObservationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicObservationTracker {
    /// Creates an empty observation tracker.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            apple_music: None,
            spotify: None,
            pending_skips: VecDeque::new(),
            pending_history: VecDeque::new(),
            last_history_id: HistoryTransitionId::from_raw(0),
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
            outcome: SkipCommandOutcome::Pending,
            matched_observation: false,
        });
    }

    pub(crate) fn finish_skip(
        &mut self,
        transport_id: TransportRequestId,
        outcome: SkipCommandOutcome,
    ) {
        let Some(index) = self
            .pending_skips
            .iter()
            .position(|pending| pending.transport_id == transport_id)
        else {
            return;
        };
        if outcome == SkipCommandOutcome::Accepted && !self.pending_skips[index].matched_observation
        {
            self.pending_skips[index].outcome = SkipCommandOutcome::Accepted;
            return;
        }
        self.resolve_history_skip(transport_id, outcome);
        self.pending_skips.remove(index);
    }

    /// Clears observations and pending command correlation.
    pub(crate) fn reset(&mut self) {
        self.reset_observations();
        self.clear_pending_skips();
        self.pending_history.clear();
    }

    pub(crate) fn clear_pending_skips(&mut self) {
        let request_ids: Vec<_> = self
            .pending_skips
            .iter()
            .map(|pending| pending.transport_id)
            .collect();
        for request_id in request_ids {
            self.finish_skip(request_id, SkipCommandOutcome::Rejected);
        }
    }

    /// Clears observation baselines while retaining pending command correlation.
    pub(crate) fn reset_observations(&mut self) {
        self.apple_music = None;
        self.spotify = None;
    }

    /// Orders and classifies one canonical provider observation.
    pub(crate) fn observe(
        &mut self,
        snapshot: MusicSnapshot,
        timing: MusicObservationTiming,
    ) -> MusicObservationOutcome {
        let previous = self.latest(snapshot.provider()).cloned();
        if previous
            .as_ref()
            .is_some_and(|previous| previous.observed_at() >= snapshot.observed_at())
        {
            return MusicObservationOutcome::OutOfOrder;
        }

        self.expire_skips(snapshot.observed_at());
        let skip_index = self.pending_skips.iter().position(|pending| {
            !pending.matched_observation && pending.applies_at(snapshot.observed_at())
        });
        let skip_applies = skip_index.is_some();
        let transition = classify_transition(previous.as_ref(), &snapshot, skip_applies);
        let rejected_transition = skip_applies
            .then(|| classify_transition(previous.as_ref(), &snapshot, false))
            .flatten();
        self.enqueue_observation_transition(
            snapshot.clone(),
            timing,
            transition,
            rejected_transition,
            skip_index,
        );
        self.resolve_unchanged_skip(&snapshot, skip_index, transition);
        self.replace_latest(snapshot.clone());

        MusicObservationOutcome::Accepted(Box::new(MusicObservationDecision {
            snapshot,
            history_transition: self.current_history_transition(),
        }))
    }

    pub(crate) fn acknowledge_history_transition(
        &mut self,
        id: HistoryTransitionId,
    ) -> MusicHistoryTransitionAcknowledgement {
        let Some(current) = self.pending_history.front() else {
            return MusicHistoryTransitionAcknowledgement::Stale;
        };
        if current.id != id
            || !(match current.kind {
                PendingHistoryKind::Confirmed(_) => true,
                PendingHistoryKind::AwaitingSkip { .. } => false,
            })
        {
            return MusicHistoryTransitionAcknowledgement::Stale;
        }
        self.pending_history.pop_front();
        MusicHistoryTransitionAcknowledgement::Acknowledged
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
        let expired: Vec<_> = self
            .pending_skips
            .iter()
            .filter(|pending| pending.is_expired_at(observed_at))
            .map(|pending| pending.transport_id)
            .collect();
        for request_id in expired {
            self.finish_skip(request_id, SkipCommandOutcome::Rejected);
        }
    }

    fn enqueue_observation_transition(
        &mut self,
        snapshot: MusicSnapshot,
        timing: MusicObservationTiming,
        transition: Option<MusicRideEventKind>,
        rejected_transition: Option<MusicRideEventKind>,
        skip_index: Option<usize>,
    ) {
        let Some(kind) = transition else {
            return;
        };
        if self.pending_history.len() >= crate::MAX_MUSIC_TIMELINE_EVENTS {
            return;
        }
        let pending_kind = if kind == MusicRideEventKind::Skip {
            let Some(index) = skip_index else { return };
            let pending = &mut self.pending_skips[index];
            pending.matched_observation = true;
            match pending.outcome {
                SkipCommandOutcome::Accepted => PendingHistoryKind::Confirmed(kind),
                SkipCommandOutcome::Pending => PendingHistoryKind::AwaitingSkip {
                    transport_id: pending.transport_id,
                    rejected_kind: rejected_transition,
                },
                SkipCommandOutcome::Rejected => return,
            }
        } else {
            PendingHistoryKind::Confirmed(kind)
        };
        let Some(id) = self.last_history_id.next() else {
            return;
        };
        self.last_history_id = id;
        self.pending_history.push_back(PendingHistoryTransition {
            id,
            snapshot,
            kind: pending_kind,
            timing,
        });
        if let Some(index) = skip_index
            && kind == MusicRideEventKind::Skip
            && self.pending_skips[index].outcome == SkipCommandOutcome::Accepted
        {
            self.pending_skips.remove(index);
        }
    }

    fn resolve_unchanged_skip(
        &mut self,
        current: &MusicSnapshot,
        skip_index: Option<usize>,
        transition: Option<MusicRideEventKind>,
    ) {
        let Some(index) = skip_index else {
            return;
        };
        if transition == Some(MusicRideEventKind::Skip) {
            return;
        }
        if is_terminal_state(current.state()) {
            let request_id = self.pending_skips[index].transport_id;
            self.finish_skip(request_id, SkipCommandOutcome::Rejected);
            return;
        }
        let pending = &mut self.pending_skips[index];
        pending.remaining_unchanged_observations =
            pending.remaining_unchanged_observations.saturating_sub(1);
        if pending.remaining_unchanged_observations == 0 {
            let request_id = pending.transport_id;
            self.finish_skip(request_id, SkipCommandOutcome::Rejected);
        }
    }

    fn resolve_history_skip(
        &mut self,
        transport_id: TransportRequestId,
        outcome: SkipCommandOutcome,
    ) {
        let Some(index) = self
            .pending_history
            .iter()
            .position(|pending| match pending.kind {
                PendingHistoryKind::AwaitingSkip {
                    transport_id: current,
                    ..
                } if current == transport_id => true,
                _ => false,
            })
        else {
            return;
        };
        let replacement = match self.pending_history[index].kind {
            PendingHistoryKind::AwaitingSkip { rejected_kind, .. } => match outcome {
                SkipCommandOutcome::Accepted => Some(MusicRideEventKind::Skip),
                SkipCommandOutcome::Rejected => rejected_kind,
                SkipCommandOutcome::Pending => return,
            },
            PendingHistoryKind::Confirmed(_) => return,
        };
        if let Some(kind) = replacement {
            self.pending_history[index].kind = PendingHistoryKind::Confirmed(kind);
        } else {
            self.pending_history.remove(index);
        }
    }

    fn current_history_transition(&self) -> Option<MusicHistoryTransition> {
        let pending = self.pending_history.front()?;
        let PendingHistoryKind::Confirmed(kind) = pending.kind else {
            return None;
        };
        Some(MusicHistoryTransition {
            id: pending.id,
            snapshot: pending.snapshot.clone(),
            kind,
            timing: pending.timing,
        })
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
    if match current.state() {
        MusicPlaybackState::Buffering
        | MusicPlaybackState::Interrupted
        | MusicPlaybackState::Unauthorized
        | MusicPlaybackState::Unavailable
        | MusicPlaybackState::Stale => true,
        _ => false,
    } {
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
    match state {
        MusicPlaybackState::Stopped
        | MusicPlaybackState::Unauthorized
        | MusicPlaybackState::Unavailable
        | MusicPlaybackState::Disconnected
        | MusicPlaybackState::Stale => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MusicCapabilities, MusicItem, MusicPlaybackPosition, MusicValidationError};
    use cutout_core::WallClockUnixTimestamp;

    const fn timing(wall_clock_at: u64) -> MusicObservationTiming {
        MusicObservationTiming::new(WallClockUnixTimestamp::new(wall_clock_at), 5)
    }

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
        let _ = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("track"),
                MusicPlaybackState::Playing,
                Some(0),
                100,
            ),
            timing(1_000),
        );
        let first_id = tracker
            .current_history_transition()
            .expect("first transition")
            .id();
        let _ = tracker.acknowledge_history_transition(first_id);
        let _ = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("track"),
                MusicPlaybackState::Playing,
                Some(30_000),
                200,
            ),
            timing(2_000),
        );
        tracker.issue_skip(
            TransportRequestId::from_raw(1),
            MonotonicTimestamp::new(250),
        );

        tracker.finish_skip(
            TransportRequestId::from_raw(1),
            SkipCommandOutcome::Accepted,
        );
        let MusicObservationOutcome::Accepted(decision) = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("track"),
                MusicPlaybackState::Playing,
                Some(1_000),
                300,
            ),
            timing(3_000),
        ) else {
            panic!("new observation must be accepted");
        };
        assert_eq!(
            decision
                .history_transition()
                .map(MusicHistoryTransition::kind),
            Some(MusicRideEventKind::Skip)
        );
    }

    #[test]
    fn observation_before_command_does_not_consume_skip() {
        let mut tracker = MusicObservationTracker::new();
        tracker.issue_skip(
            TransportRequestId::from_raw(1),
            MonotonicTimestamp::new(200),
        );
        let MusicObservationOutcome::Accepted(before) = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("before"),
                MusicPlaybackState::Playing,
                None,
                100,
            ),
            timing(1_000),
        ) else {
            panic!("observation must be accepted");
        };
        assert_eq!(
            before
                .history_transition()
                .map(MusicHistoryTransition::kind),
            Some(MusicRideEventKind::ItemChanged)
        );
        let before_id = before.history_transition().expect("transition").id();
        let _ = tracker.acknowledge_history_transition(before_id);

        tracker.finish_skip(
            TransportRequestId::from_raw(1),
            SkipCommandOutcome::Accepted,
        );
        let MusicObservationOutcome::Accepted(after) = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("after"),
                MusicPlaybackState::Playing,
                None,
                300,
            ),
            timing(3_000),
        ) else {
            panic!("observation must be accepted");
        };
        assert_eq!(
            after.history_transition().map(MusicHistoryTransition::kind),
            Some(MusicRideEventKind::Skip)
        );
    }

    #[test]
    fn initial_disconnect_is_a_disconnect_with_or_without_an_item() {
        for item in [Some("track"), None] {
            let mut tracker = MusicObservationTracker::new();
            let MusicObservationOutcome::Accepted(decision) = tracker.observe(
                snapshot(
                    MusicProvider::AppleMusic,
                    item,
                    MusicPlaybackState::Disconnected,
                    None,
                    100,
                ),
                timing(1_000),
            ) else {
                panic!("observation must be accepted");
            };
            assert_eq!(
                decision
                    .history_transition()
                    .map(MusicHistoryTransition::kind),
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

    #[test]
    fn unacknowledged_history_transition_is_retried_exactly_once() {
        let mut tracker = MusicObservationTracker::new();
        let MusicObservationOutcome::Accepted(first) = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("first"),
                MusicPlaybackState::Playing,
                None,
                100,
            ),
            timing(1_000),
        ) else {
            panic!("first observation must be accepted");
        };
        let first_event = first.history_transition().expect("first transition");
        assert_eq!(
            tracker.acknowledge_history_transition(first_event.id()),
            MusicHistoryTransitionAcknowledgement::Acknowledged
        );

        let MusicObservationOutcome::Accepted(failed_write) = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("second"),
                MusicPlaybackState::Playing,
                None,
                200,
            ),
            timing(2_000),
        ) else {
            panic!("second observation must be accepted");
        };
        let pending = failed_write
            .history_transition()
            .expect("transition awaiting persistence")
            .clone();

        let MusicObservationOutcome::Accepted(retry) = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("second"),
                MusicPlaybackState::Playing,
                None,
                300,
            ),
            timing(3_000),
        ) else {
            panic!("unchanged recovery observation must be accepted");
        };
        assert_eq!(retry.history_transition(), Some(&pending));
        assert_eq!(
            tracker.acknowledge_history_transition(pending.id()),
            MusicHistoryTransitionAcknowledgement::Acknowledged
        );

        let MusicObservationOutcome::Accepted(after_ack) = tracker.observe(
            snapshot(
                MusicProvider::AppleMusic,
                Some("second"),
                MusicPlaybackState::Playing,
                None,
                400,
            ),
            timing(4_000),
        ) else {
            panic!("later observation must be accepted");
        };
        assert_eq!(after_ack.history_transition(), None);
    }

    #[test]
    fn item_change_waits_for_skip_command_outcome() {
        let mut tracker = MusicObservationTracker::new();
        let MusicObservationOutcome::Accepted(first) = tracker.observe(
            snapshot(
                MusicProvider::Spotify,
                Some("first"),
                MusicPlaybackState::Playing,
                None,
                100,
            ),
            timing(1_000),
        ) else {
            panic!("first observation must be accepted");
        };
        let first_id = first.history_transition().expect("first transition").id();
        let _ = tracker.acknowledge_history_transition(first_id);

        let request_id = TransportRequestId::from_raw(1);
        tracker.issue_skip(request_id, MonotonicTimestamp::new(150));
        let MusicObservationOutcome::Accepted(provisional) = tracker.observe(
            snapshot(
                MusicProvider::Spotify,
                Some("second"),
                MusicPlaybackState::Playing,
                None,
                200,
            ),
            timing(2_000),
        ) else {
            panic!("changed observation must be accepted");
        };
        assert_eq!(provisional.history_transition(), None);

        tracker.finish_skip(request_id, SkipCommandOutcome::Rejected);
        let MusicObservationOutcome::Accepted(after_failure) = tracker.observe(
            snapshot(
                MusicProvider::Spotify,
                Some("second"),
                MusicPlaybackState::Playing,
                None,
                300,
            ),
            timing(3_000),
        ) else {
            panic!("recovery observation must be accepted");
        };
        assert_eq!(
            after_failure
                .history_transition()
                .map(MusicHistoryTransition::kind),
            Some(MusicRideEventKind::ItemChanged)
        );
    }

    #[test]
    fn classifier_only_emits_transitions_valid_for_current_state() {
        let states = [
            MusicPlaybackState::Playing,
            MusicPlaybackState::Paused,
            MusicPlaybackState::Stopped,
            MusicPlaybackState::Buffering,
            MusicPlaybackState::Interrupted,
            MusicPlaybackState::Unauthorized,
            MusicPlaybackState::Unavailable,
            MusicPlaybackState::Disconnected,
            MusicPlaybackState::Stale,
        ];
        for state in states {
            for previous_item in [None, Some("first")] {
                let previous = previous_item
                    .map(|item| snapshot(MusicProvider::AppleMusic, Some(item), state, None, 100));
                let current = snapshot(MusicProvider::AppleMusic, Some("second"), state, None, 200);
                let transition = classify_transition(previous.as_ref(), &current, false);
                assert!(
                    transition.is_none_or(|kind| kind.valid_for_state(state)),
                    "{transition:?} is invalid for {state:?}"
                );
            }
        }
    }
}
