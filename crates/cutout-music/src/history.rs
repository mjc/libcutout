//! Privacy-filtered ride events and bounded music timelines.

use cutout_core::{MonotonicTimestamp, WallClockUnixTimestamp};
use thiserror::Error;

use crate::{
    MusicDisplayText, MusicIdentifier, MusicPlaybackState, MusicProvider, MusicSnapshot,
    MusicValidationError,
};

/// Maximum number of music transitions retained for one ride timeline.
pub const MAX_MUSIC_TIMELINE_EVENTS: usize = 512;

/// The user's ride-history privacy choice for music metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicHistoryPolicy {
    /// Do not retain music observations.
    Disabled,
    /// Retain provider and opaque item identifiers only.
    OpaqueItem,
    /// Retain bounded title and artist text as well as opaque identifiers.
    HumanReadable,
}

/// Durable state of the music-history record associated with one ride.
///
/// A missing row is distinct from an explicit disabled choice, and a deletion
/// leaves a tombstone so callers do not mistake forgotten history for a ride
/// that was never associated with music.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicHistoryState {
    /// No music-history choice has been recorded for the ride.
    Missing,
    /// The user explicitly disabled music-history retention.
    Disabled,
    /// Only opaque identifiers remain after display metadata was redacted.
    Redacted,
    /// Bounded human-readable metadata is retained.
    HumanReadable,
    /// Music history was explicitly deleted while the ride was preserved.
    Deleted,
}

/// A low-rate music transition accepted into one ride timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicRideEvent {
    provider: MusicProvider,
    item_identifier: Option<MusicIdentifier>,
    title: Option<MusicDisplayText>,
    artist: Option<MusicDisplayText>,
    kind: MusicRideEventKind,
    observed_at: Option<MonotonicTimestamp>,
    monotonic_at: MonotonicTimestamp,
    wall_clock_at: WallClockUnixTimestamp,
    clock_uncertainty_milliseconds: u64,
}

/// Clock values associated with one ride music transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicEventTiming {
    /// Original provider observation time; absent for legacy/imported events.
    pub observed_at: Option<MonotonicTimestamp>,
    /// Host monotonic event time.
    pub monotonic_at: MonotonicTimestamp,
    /// Wall-clock event time.
    pub wall_clock_at: WallClockUnixTimestamp,
    /// Host clock uncertainty in milliseconds.
    pub clock_uncertainty_milliseconds: u64,
}

impl MusicRideEvent {
    /// Creates a validated event read from the ride store.
    ///
    /// # Errors
    ///
    /// Returns [`MusicValidationError`] when an identifier or timestamp is invalid.
    pub fn new(
        provider: MusicProvider,
        item_identifier: Option<String>,
        title: Option<String>,
        artist: Option<String>,
        kind: MusicRideEventKind,
        timing: MusicEventTiming,
    ) -> Result<Self, MusicValidationError> {
        if timing
            .observed_at
            .is_some_and(|observed| observed > timing.monotonic_at)
        {
            return Err(MusicValidationError::ObservationAfterEvent);
        }
        Ok(Self {
            observed_at: timing.observed_at,
            provider,
            item_identifier: item_identifier.map(MusicIdentifier::new).transpose()?,
            title: title.and_then(MusicDisplayText::new),
            artist: artist.and_then(MusicDisplayText::new),
            kind,
            monotonic_at: timing.monotonic_at,
            wall_clock_at: timing.wall_clock_at,
            clock_uncertainty_milliseconds: timing.clock_uncertainty_milliseconds,
        })
    }

    /// Builds a privacy-filtered event from a provider observation.
    /// # Errors
    ///
    /// Returns [`MusicValidationError::EventKindStateMismatch`] for a transition kind that does
    /// not match the provider state, or another validation error for invalid timing/text.
    pub fn try_from_snapshot(
        snapshot: &MusicSnapshot,
        kind: MusicRideEventKind,
        monotonic_at: MonotonicTimestamp,
        wall_clock_at: WallClockUnixTimestamp,
        clock_uncertainty_milliseconds: u64,
        policy: MusicHistoryPolicy,
    ) -> Result<Option<Self>, MusicValidationError> {
        if policy == MusicHistoryPolicy::Disabled || snapshot.state() == MusicPlaybackState::Stale {
            return Ok(None);
        }
        if !kind.valid_for_state(snapshot.state()) {
            return Err(MusicValidationError::EventKindStateMismatch);
        }
        let item = snapshot.item();
        let (item_identifier, title, artist) = match policy {
            MusicHistoryPolicy::Disabled => return Ok(None),
            MusicHistoryPolicy::OpaqueItem => (
                item.filter(|item| {
                    !metadata_bearing_identifier(snapshot.provider(), item.identifier())
                })
                .map(|item| item.identifier().clone()),
                None,
                None,
            ),
            MusicHistoryPolicy::HumanReadable => (
                item.map(|item| item.identifier().clone()),
                item.and_then(|item| item.title().map(str::to_owned)),
                item.and_then(|item| item.artist().map(str::to_owned)),
            ),
        };
        Self::new(
            snapshot.provider(),
            item_identifier.map(|identifier| identifier.as_str().to_owned()),
            title,
            artist,
            kind,
            MusicEventTiming {
                observed_at: Some(snapshot.observed_at()),
                monotonic_at,
                wall_clock_at,
                clock_uncertainty_milliseconds,
            },
        )
        .map(Some)
    }

    /// Builds a privacy-filtered event, treating invalid observations as absent.
    #[must_use]
    pub fn from_snapshot(
        snapshot: &MusicSnapshot,
        kind: MusicRideEventKind,
        monotonic_at: MonotonicTimestamp,
        wall_clock_at: WallClockUnixTimestamp,
        clock_uncertainty_milliseconds: u64,
        policy: MusicHistoryPolicy,
    ) -> Option<Self> {
        Self::try_from_snapshot(
            snapshot,
            kind,
            monotonic_at,
            wall_clock_at,
            clock_uncertainty_milliseconds,
            policy,
        )
        .ok()
        .flatten()
    }

    /// Returns the event kind.
    #[must_use]
    pub const fn kind(&self) -> MusicRideEventKind {
        self.kind
    }

    /// Returns the original observation time without fabricating one for legacy data.
    #[must_use]
    pub const fn observed_at(&self) -> Option<MonotonicTimestamp> {
        self.observed_at
    }

    /// Returns the monotonic event time.
    #[must_use]
    pub const fn monotonic_at(&self) -> MonotonicTimestamp {
        self.monotonic_at
    }

    /// Returns the opaque item identifier, when known and retained.
    #[must_use]
    pub fn item_identifier(&self) -> Option<&MusicIdentifier> {
        self.item_identifier.as_ref()
    }

    /// Returns retained title text, when human-readable history is enabled.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_ref().map(MusicDisplayText::as_str)
    }

    /// Returns retained artist text, when human-readable history is enabled.
    #[must_use]
    pub fn artist(&self) -> Option<&str> {
        self.artist.as_ref().map(MusicDisplayText::as_str)
    }

    /// Returns whether two events describe the same provider transition.
    #[must_use]
    pub fn same_transition_as(&self, other: &Self) -> bool {
        let same_content = self.provider == other.provider
            && self.item_identifier == other.item_identifier
            && self.kind == other.kind;
        if self.kind.is_occurrence() || other.kind.is_occurrence() {
            return same_content
                && self.observed_at == other.observed_at
                && self.monotonic_at == other.monotonic_at
                && self.wall_clock_at == other.wall_clock_at
                && self.clock_uncertainty_milliseconds == other.clock_uncertainty_milliseconds;
        }
        same_content
    }

    /// Removes human-readable metadata and any provider identifier that embeds it.
    pub fn redact_display_metadata(&mut self) {
        self.title = None;
        self.artist = None;
        if self
            .item_identifier
            .as_ref()
            .is_some_and(|identifier| metadata_bearing_identifier(self.provider, identifier))
        {
            self.item_identifier = None;
        }
    }

    /// Returns the provider.
    #[must_use]
    pub const fn provider(&self) -> MusicProvider {
        self.provider
    }

    /// Returns the wall-clock event time.
    #[must_use]
    pub const fn wall_clock_at(&self) -> WallClockUnixTimestamp {
        self.wall_clock_at
    }

    /// Returns the clock uncertainty in milliseconds.
    #[must_use]
    pub const fn clock_uncertainty_milliseconds(&self) -> u64 {
        self.clock_uncertainty_milliseconds
    }
}

/// A low-rate transition retained in a ride timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicRideEventKind {
    /// Playback began or resumed.
    Play,
    /// Playback paused.
    Pause,
    /// The current item was skipped.
    Skip,
    /// The current item changed without an explicit skip command.
    ItemChanged,
    /// Playback stopped or reached the end of the current item.
    Stopped,
    /// The provider connection ended.
    ProviderDisconnected,
}

impl MusicRideEventKind {
    const fn is_occurrence(self) -> bool {
        matches!(
            self,
            Self::Skip | Self::ItemChanged | Self::ProviderDisconnected
        )
    }

    /// Returns whether this transition is valid for the provider state that produced it.
    #[must_use]
    pub const fn valid_for_state(self, state: MusicPlaybackState) -> bool {
        match self {
            Self::Play | Self::Pause => matches!(
                state,
                MusicPlaybackState::Playing
                    | MusicPlaybackState::Paused
                    | MusicPlaybackState::Buffering
            ),
            Self::Stopped => matches!(state, MusicPlaybackState::Stopped),
            Self::ProviderDisconnected => matches!(state, MusicPlaybackState::Disconnected),
            Self::Skip | Self::ItemChanged => matches!(
                state,
                MusicPlaybackState::Playing
                    | MusicPlaybackState::Paused
                    | MusicPlaybackState::Stopped
            ),
        }
    }
}

/// Result of attempting to append one event to a timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicTimelineOutcome {
    /// The event was appended.
    Recorded,
    /// The event repeats the newest event and was coalesced.
    Duplicate,
    /// The event would move the timeline backwards.
    OutOfOrder,
    /// History association is disabled.
    Disabled,
    /// The bounded event capacity has been reached.
    Full,
}

/// A bounded in-memory timeline used by the ride persistence owner.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MusicTimeline {
    events: Vec<MusicRideEvent>,
}

impl MusicTimeline {
    /// Creates an empty timeline.
    #[must_use]
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Restores the exact durable event sequence without semantic coalescing.
    ///
    /// # Errors
    ///
    /// Returns [`MusicTimelineRestoreError`] when stored rows exceed capacity
    /// or move backwards in monotonic time.
    pub fn from_stored_events(
        events: Vec<MusicRideEvent>,
    ) -> Result<Self, MusicTimelineRestoreError> {
        if events.len() > MAX_MUSIC_TIMELINE_EVENTS {
            return Err(MusicTimelineRestoreError::Full);
        }
        if events
            .windows(2)
            .any(|pair| pair[1].monotonic_at() < pair[0].monotonic_at())
        {
            return Err(MusicTimelineRestoreError::OutOfOrder);
        }
        Ok(Self { events })
    }

    /// Appends one event while enforcing order, deduplication, and capacity.
    pub fn append(&mut self, event: MusicRideEvent) -> MusicTimelineOutcome {
        let outcome = Self::admission(self.events.last(), self.events.len(), &event);
        if outcome == MusicTimelineOutcome::Recorded {
            self.events.push(event);
        }
        outcome
    }

    /// Decides admission from the bounded timeline's tail without copying its history.
    #[must_use]
    pub fn admission(
        previous: Option<&MusicRideEvent>,
        count: usize,
        event: &MusicRideEvent,
    ) -> MusicTimelineOutcome {
        if let Some(previous) = previous {
            if event.monotonic_at() < previous.monotonic_at() {
                return MusicTimelineOutcome::OutOfOrder;
            }
            if event.same_transition_as(previous) {
                return MusicTimelineOutcome::Duplicate;
            }
        }
        if count >= MAX_MUSIC_TIMELINE_EVENTS {
            return MusicTimelineOutcome::Full;
        }
        MusicTimelineOutcome::Recorded
    }

    /// Removes the newest event when its durable commit must be rolled back.
    pub fn pop_last(&mut self) -> Option<MusicRideEvent> {
        self.events.pop()
    }

    /// Returns the retained events in monotonic order.
    #[must_use]
    pub fn events(&self) -> &[MusicRideEvent] {
        &self.events
    }

    /// Removes human-readable metadata from every retained event.
    pub fn redact_display_metadata(&mut self) {
        self.events
            .iter_mut()
            .for_each(MusicRideEvent::redact_display_metadata);
    }
}

/// Failure while rebuilding a timeline from durable rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MusicTimelineRestoreError {
    /// Stored rows exceed the bounded timeline capacity.
    #[error("stored music timeline is full")]
    Full,
    /// Stored rows are not ordered by monotonic time.
    #[error("stored music timeline is out of order")]
    OutOfOrder,
}

fn metadata_bearing_identifier(provider: MusicProvider, identifier: &MusicIdentifier) -> bool {
    match provider {
        MusicProvider::AppleMusic => false,
        MusicProvider::Spotify => identifier.as_str().starts_with("spotify:local:"),
    }
}
