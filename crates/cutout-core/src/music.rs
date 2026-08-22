use thiserror::Error;

use crate::{MonotonicTimestamp, WallClockUnixTimestamp};

/// Maximum provider/session identifier bytes accepted at the platform boundary.
pub const MAX_MUSIC_IDENTIFIER_BYTES: usize = 256;
/// Maximum title or artist bytes retained in human-readable ride history.
pub const MAX_MUSIC_DISPLAY_TEXT_BYTES: usize = 512;
/// Maximum number of music transitions retained for one ride timeline.
pub const MAX_MUSIC_TIMELINE_EVENTS: usize = 512;

/// A provider with a supported transport-control adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MusicProvider {
    /// Apple Music through the native system-player API.
    AppleMusic,
    /// Spotify through App Remote.
    Spotify,
}

/// Provider playback state projected into the shared player.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicPlaybackState {
    /// A track is actively playing.
    Playing,
    /// Playback is paused.
    Paused,
    /// The provider has stopped playback.
    Stopped,
    /// The provider is loading or buffering.
    Buffering,
    /// Playback is temporarily interrupted by the platform.
    Interrupted,
    /// Authorization is required before the provider can be used.
    Unauthorized,
    /// The provider or account cannot currently provide state.
    Unavailable,
    /// The provider connection is gone.
    Disconnected,
    /// The observation is older than the consumer's freshness policy.
    Stale,
}

/// A transport command exposed by a provider adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MusicCommand {
    /// Skip to the previous item.
    Previous,
    /// Start playback.
    Play,
    /// Pause playback.
    Pause,
    /// Skip to the next item.
    Next,
    /// Open the provider's own application.
    OpenProvider,
}

/// Capability bits projected by one provider observation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MusicCapabilities(u8);

impl MusicCapabilities {
    /// Creates an empty capability set.
    #[must_use]
    pub const fn new() -> Self {
        Self(0)
    }

    /// Adds one provider-reported command capability.
    #[must_use]
    pub const fn with(self, command: MusicCommand) -> Self {
        Self(self.0 | command.bit())
    }

    /// Returns whether the command is currently exposed.
    #[must_use]
    pub const fn supports(self, command: MusicCommand) -> bool {
        self.0 & command.bit() != 0
    }
}

impl MusicCommand {
    const fn bit(self) -> u8 {
        match self {
            Self::Previous => 1 << 0,
            Self::Play => 1 << 1,
            Self::Pause => 1 << 2,
            Self::Next => 1 << 3,
            Self::OpenProvider => 1 << 4,
        }
    }
}

/// A bounded provider or item identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MusicIdentifier(String);

impl MusicIdentifier {
    /// Validates and stores an opaque provider identifier.
    ///
    /// # Errors
    ///
    /// Returns [`MusicValidationError`] when the identifier is blank or too long.
    pub fn new(value: impl Into<String>) -> Result<Self, MusicValidationError> {
        let value = value.into();
        validate_text(
            &value,
            MAX_MUSIC_IDENTIFIER_BYTES,
            MusicTextField::Identifier,
        )?;
        Ok(Self(value))
    }

    /// Returns the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Optional display metadata for the currently playing item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicItem {
    identifier: MusicIdentifier,
    title: Option<String>,
    artist: Option<String>,
}

impl MusicItem {
    /// Validates and creates an item projection.
    ///
    /// # Errors
    ///
    /// Returns [`MusicValidationError`] when an identifier, title, or artist
    /// is blank or exceeds its bound.
    pub fn new(
        identifier: impl Into<String>,
        title: Option<String>,
        artist: Option<String>,
    ) -> Result<Self, MusicValidationError> {
        let title = validate_optional_text(title, MusicTextField::Title)?;
        let artist = validate_optional_text(artist, MusicTextField::Artist)?;
        Ok(Self {
            identifier: MusicIdentifier::new(identifier)?,
            title,
            artist,
        })
    }

    /// Returns the opaque provider item identifier.
    #[must_use]
    pub fn identifier(&self) -> &MusicIdentifier {
        &self.identifier
    }

    /// Returns the optional human-readable title.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the optional human-readable artist.
    #[must_use]
    pub fn artist(&self) -> Option<&str> {
        self.artist.as_deref()
    }
}

/// A validated playback position and optional duration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MusicPlaybackPosition {
    position_milliseconds: Option<u64>,
    duration_milliseconds: Option<u64>,
}

impl MusicPlaybackPosition {
    /// Creates a validated provider position.
    ///
    /// # Errors
    ///
    /// Returns [`MusicValidationError::PositionAfterDuration`] when both
    /// values are known and the position exceeds the duration.
    pub const fn new(
        position_milliseconds: Option<u64>,
        duration_milliseconds: Option<u64>,
    ) -> Result<Self, MusicValidationError> {
        if let (Some(position), Some(duration)) = (position_milliseconds, duration_milliseconds)
            && position > duration
        {
            return Err(MusicValidationError::PositionAfterDuration);
        }
        Ok(Self {
            position_milliseconds,
            duration_milliseconds,
        })
    }

    /// Returns the provider playback position.
    #[must_use]
    pub const fn position_milliseconds(self) -> Option<u64> {
        self.position_milliseconds
    }

    /// Returns the provider item duration.
    #[must_use]
    pub const fn duration_milliseconds(self) -> Option<u64> {
        self.duration_milliseconds
    }
}

/// A validated provider observation used by Swift presentation and ride history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicSnapshot {
    provider: MusicProvider,
    session_id: MusicIdentifier,
    state: MusicPlaybackState,
    item: Option<MusicItem>,
    position_milliseconds: Option<u64>,
    duration_milliseconds: Option<u64>,
    observed_at: MonotonicTimestamp,
    capabilities: MusicCapabilities,
}

impl MusicSnapshot {
    /// Validates and creates a provider observation.
    ///
    /// # Errors
    ///
    /// Returns [`MusicValidationError`] when the session identifier is invalid
    /// or the playback position is inconsistent with its duration.
    pub fn new(
        provider: MusicProvider,
        session_id: impl Into<String>,
        state: MusicPlaybackState,
        item: Option<MusicItem>,
        position: MusicPlaybackPosition,
        observed_at: MonotonicTimestamp,
        capabilities: MusicCapabilities,
    ) -> Result<Self, MusicValidationError> {
        Ok(Self {
            provider,
            session_id: MusicIdentifier::new(session_id)?,
            state,
            item,
            position_milliseconds: position.position_milliseconds(),
            duration_milliseconds: position.duration_milliseconds(),
            observed_at,
            capabilities,
        })
    }

    /// Returns the provider.
    #[must_use]
    pub const fn provider(&self) -> MusicProvider {
        self.provider
    }

    /// Returns the provider session identifier.
    #[must_use]
    pub fn session_id(&self) -> &MusicIdentifier {
        &self.session_id
    }

    /// Returns the playback state.
    #[must_use]
    pub const fn state(&self) -> MusicPlaybackState {
        self.state
    }

    /// Returns the current item, when known.
    #[must_use]
    pub fn item(&self) -> Option<&MusicItem> {
        self.item.as_ref()
    }

    /// Returns the provider playback position.
    #[must_use]
    pub const fn position_milliseconds(&self) -> Option<u64> {
        self.position_milliseconds
    }

    /// Returns the provider item duration.
    #[must_use]
    pub const fn duration_milliseconds(&self) -> Option<u64> {
        self.duration_milliseconds
    }

    /// Returns when this observation was received on the host monotonic clock.
    #[must_use]
    pub const fn observed_at(&self) -> MonotonicTimestamp {
        self.observed_at
    }

    /// Returns provider-reported command capabilities.
    #[must_use]
    pub const fn capabilities(&self) -> MusicCapabilities {
        self.capabilities
    }
}

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

/// A low-rate music transition accepted into one ride timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicRideEvent {
    provider: MusicProvider,
    item_identifier: Option<MusicIdentifier>,
    title: Option<String>,
    artist: Option<String>,
    kind: MusicRideEventKind,
    monotonic_at: MonotonicTimestamp,
    wall_clock_at: WallClockUnixTimestamp,
    clock_uncertainty_milliseconds: u64,
}

/// Clock values associated with one ride music transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicEventTiming {
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
    /// Returns [`MusicValidationError`] when an identifier or display field is
    /// invalid.
    pub fn new(
        provider: MusicProvider,
        item_identifier: Option<String>,
        title: Option<String>,
        artist: Option<String>,
        kind: MusicRideEventKind,
        timing: MusicEventTiming,
    ) -> Result<Self, MusicValidationError> {
        Ok(Self {
            provider,
            item_identifier: item_identifier.map(MusicIdentifier::new).transpose()?,
            title: validate_optional_text(title, MusicTextField::Title)?,
            artist: validate_optional_text(artist, MusicTextField::Artist)?,
            kind,
            monotonic_at: timing.monotonic_at,
            wall_clock_at: timing.wall_clock_at,
            clock_uncertainty_milliseconds: timing.clock_uncertainty_milliseconds,
        })
    }

    /// Builds a privacy-filtered event from a provider observation.
    #[must_use]
    pub fn from_snapshot(
        snapshot: &MusicSnapshot,
        kind: MusicRideEventKind,
        monotonic_at: MonotonicTimestamp,
        wall_clock_at: WallClockUnixTimestamp,
        clock_uncertainty_milliseconds: u64,
        policy: MusicHistoryPolicy,
    ) -> Option<Self> {
        if policy == MusicHistoryPolicy::Disabled {
            return None;
        }
        let item = snapshot.item();
        let (item_identifier, title, artist) = match policy {
            MusicHistoryPolicy::Disabled => return None,
            MusicHistoryPolicy::OpaqueItem => {
                (item.map(|item| item.identifier().clone()), None, None)
            }
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
                monotonic_at,
                wall_clock_at,
                clock_uncertainty_milliseconds,
            },
        )
        .ok()
    }

    /// Returns the event kind.
    #[must_use]
    pub const fn kind(&self) -> MusicRideEventKind {
        self.kind
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
        self.title.as_deref()
    }

    /// Returns retained artist text, when human-readable history is enabled.
    #[must_use]
    pub fn artist(&self) -> Option<&str> {
        self.artist.as_deref()
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
    /// The provider connection ended.
    ProviderDisconnected,
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
    /// The ride is no longer accepting music transitions.
    RideNotOpen,
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

    /// Appends one event while enforcing order, deduplication, and capacity.
    pub fn append(&mut self, event: MusicRideEvent) -> MusicTimelineOutcome {
        if let Some(previous) = self.events.last() {
            if event.monotonic_at() < previous.monotonic_at() {
                return MusicTimelineOutcome::OutOfOrder;
            }
            if event == *previous {
                return MusicTimelineOutcome::Duplicate;
            }
        }
        if self.events.len() == MAX_MUSIC_TIMELINE_EVENTS {
            return MusicTimelineOutcome::Full;
        }
        self.events.push(event);
        MusicTimelineOutcome::Recorded
    }

    /// Returns the retained events in monotonic order.
    #[must_use]
    pub fn events(&self) -> &[MusicRideEvent] {
        &self.events
    }
}

#[derive(Clone, Copy, Debug)]
enum MusicTextField {
    Identifier,
    Title,
    Artist,
}

impl MusicTextField {
    const fn label(self) -> &'static str {
        match self {
            Self::Identifier => "identifier",
            Self::Title => "title",
            Self::Artist => "artist",
        }
    }
}

/// Validation failures at the music/provider boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MusicValidationError {
    /// A required or optional text value was blank.
    #[error("music {0} is blank")]
    Blank(&'static str),
    /// A text value exceeded the bounded input size.
    #[error("music {0} is too long")]
    TooLong(&'static str),
    /// Playback position exceeded its known duration.
    #[error("music position is after duration")]
    PositionAfterDuration,
}

fn validate_text(
    value: &str,
    max_bytes: usize,
    field: MusicTextField,
) -> Result<(), MusicValidationError> {
    if value.trim().is_empty() {
        return Err(MusicValidationError::Blank(field.label()));
    }
    if value.len() > max_bytes {
        return Err(MusicValidationError::TooLong(field.label()));
    }
    Ok(())
}

fn validate_optional_text(
    value: Option<String>,
    field: MusicTextField,
) -> Result<Option<String>, MusicValidationError> {
    value
        .map(|value| validate_text(&value, MAX_MUSIC_DISPLAY_TEXT_BYTES, field).map(|()| value))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(item: Option<MusicItem>) -> MusicSnapshot {
        MusicSnapshot::new(
            MusicProvider::AppleMusic,
            "session",
            MusicPlaybackState::Playing,
            item,
            MusicPlaybackPosition::new(Some(10), Some(100)).expect("valid position"),
            MonotonicTimestamp::new(10),
            MusicCapabilities::new()
                .with(MusicCommand::Previous)
                .with(MusicCommand::Pause)
                .with(MusicCommand::Next)
                .with(MusicCommand::OpenProvider),
        )
        .expect("valid music fixture")
    }

    #[test]
    fn snapshot_rejects_position_after_duration() {
        let error = MusicPlaybackPosition::new(Some(101), Some(100))
            .expect_err("position must not exceed duration");
        assert_eq!(error, MusicValidationError::PositionAfterDuration);
    }

    #[test]
    fn history_policy_redacts_display_metadata() {
        let item = MusicItem::new(
            "track-1",
            Some("Song".to_owned()),
            Some("Artist".to_owned()),
        )
        .expect("valid item");
        let opaque = MusicRideEvent::from_snapshot(
            &snapshot(Some(item)),
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("opaque policy records an event");
        assert_eq!(
            opaque.item_identifier().map(MusicIdentifier::as_str),
            Some("track-1")
        );
        assert_eq!(opaque.title(), None);
        assert_eq!(opaque.artist(), None);
    }

    #[test]
    fn disabled_history_does_not_create_an_event() {
        assert!(
            MusicRideEvent::from_snapshot(
                &snapshot(None),
                MusicRideEventKind::Play,
                MonotonicTimestamp::new(10),
                WallClockUnixTimestamp::new(100),
                2,
                MusicHistoryPolicy::Disabled,
            )
            .is_none()
        );
    }

    #[test]
    fn timeline_rejects_old_and_coalesces_duplicate_events() {
        let event = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Play,
            MonotonicTimestamp::new(10),
            WallClockUnixTimestamp::new(100),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        let mut timeline = MusicTimeline::new();
        assert_eq!(
            timeline.append(event.clone()),
            MusicTimelineOutcome::Recorded
        );
        assert_eq!(timeline.append(event), MusicTimelineOutcome::Duplicate);
        let old = MusicRideEvent::from_snapshot(
            &snapshot(None),
            MusicRideEventKind::Pause,
            MonotonicTimestamp::new(9),
            WallClockUnixTimestamp::new(99),
            2,
            MusicHistoryPolicy::OpaqueItem,
        )
        .expect("valid event");
        assert_eq!(timeline.append(old), MusicTimelineOutcome::OutOfOrder);
        assert_eq!(timeline.events().len(), 1);
    }
}
