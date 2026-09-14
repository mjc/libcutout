//! Canonical provider observations and bounded metadata.

use cutout_core::MonotonicTimestamp;
use thiserror::Error;

/// Maximum provider/session identifier bytes accepted at the platform boundary.
pub const MAX_MUSIC_IDENTIFIER_BYTES: usize = 256;
/// Maximum title or artist bytes retained in human-readable ride history.
pub const MAX_MUSIC_DISPLAY_TEXT_BYTES: usize = 512;

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

impl MusicPlaybackState {
    /// Localized title key when a provider has not supplied item metadata.
    #[must_use]
    pub const fn fallback_title_key(self) -> &'static str {
        match self {
            Self::Playing => "music.state.playing",
            Self::Paused => "music.state.paused",
            Self::Stopped => "music.state.stopped",
            Self::Buffering => "music.state.buffering",
            Self::Interrupted => "music.state.interrupted",
            Self::Unauthorized => "music.state.authorization_required",
            Self::Unavailable => "music.state.unavailable",
            Self::Disconnected => "music.state.disconnected",
            Self::Stale => "music.state.stale",
        }
    }
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
        if value.trim().is_empty() {
            return Err(MusicValidationError::BlankIdentifier);
        }
        if value.len() > MAX_MUSIC_IDENTIFIER_BYTES {
            return Err(MusicValidationError::IdentifierTooLong);
        }
        Ok(Self(value))
    }

    /// Returns the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical bounded text supplied for human-readable presentation.
///
/// Provider whitespace-only values are absent. Longer values are truncated at
/// a UTF-8 boundary because display metadata is optional and must not reject an
/// otherwise usable observation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MusicDisplayText(String);

impl MusicDisplayText {
    /// Canonicalizes optional provider text.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let mut end = value.len().min(MAX_MUSIC_DISPLAY_TEXT_BYTES);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        Some(Self(value[..end].to_owned()))
    }

    /// Returns canonical display text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Optional display metadata for the currently playing item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicItem {
    identifier: MusicIdentifier,
    title: Option<MusicDisplayText>,
    artist: Option<MusicDisplayText>,
}

impl MusicItem {
    /// Validates and creates an item projection.
    ///
    /// # Errors
    ///
    /// Returns [`MusicValidationError`] when the identifier is invalid.
    /// Optional display text is canonicalized and bounded independently.
    pub fn new(
        identifier: impl Into<String>,
        title: Option<String>,
        artist: Option<String>,
    ) -> Result<Self, MusicValidationError> {
        Ok(Self {
            identifier: MusicIdentifier::new(identifier)?,
            title: title.and_then(MusicDisplayText::new),
            artist: artist.and_then(MusicDisplayText::new),
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
        self.title.as_ref().map(MusicDisplayText::as_str)
    }

    /// Returns the optional human-readable artist.
    #[must_use]
    pub fn artist(&self) -> Option<&str> {
        self.artist.as_ref().map(MusicDisplayText::as_str)
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

/// Validation failures at the music/provider boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MusicValidationError {
    /// A required provider or item identifier was blank.
    #[error("music identifier is blank")]
    BlankIdentifier,
    /// A provider or item identifier exceeded its bounded input size.
    #[error("music identifier is too long")]
    IdentifierTooLong,
    /// Playback position exceeded its known duration.
    #[error("music position is after duration")]
    PositionAfterDuration,
    /// Observation time cannot follow its associated event.
    #[error("music observation is after event")]
    ObservationAfterEvent,
    /// A transition kind does not match the provider playback state.
    #[error("music event kind does not match playback state")]
    EventKindStateMismatch,
}
