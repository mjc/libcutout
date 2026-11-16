//! Canonical provider observations and bounded metadata.

use cutout_core::MonotonicTimestamp;
use thiserror::Error;

/// Maximum provider/session identifier bytes accepted at the platform boundary.
pub(crate) const MAX_MUSIC_IDENTIFIER_BYTES: usize = 256;
/// Maximum title or artist bytes retained in human-readable ride history.
pub(crate) const MAX_MUSIC_DISPLAY_TEXT_BYTES: usize = 512;

/// A provider with a supported transport-control adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MusicProvider {
    /// Apple Music through the native system-player API.
    AppleMusic,
    /// Spotify through a native SDK or its account playback API.
    Spotify,
    /// `SoundCloud` app handoff; public iOS APIs expose no local playback state.
    SoundCloud,
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
    pub(crate) const fn is_skip(self) -> bool {
        match self {
            Self::Previous | Self::Next => true,
            Self::Play | Self::Pause | Self::OpenProvider => false,
        }
    }

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
    pub(crate) fn with_playback_observation(
        &self,
        state: MusicPlaybackState,
        observed_at: MonotonicTimestamp,
        capabilities: MusicCapabilities,
    ) -> Self {
        Self {
            state,
            observed_at,
            capabilities,
            ..self.clone()
        }
    }

    pub(crate) fn soundcloud_unavailable(observed_at: MonotonicTimestamp) -> Self {
        Self {
            provider: MusicProvider::SoundCloud,
            session_id: MusicIdentifier("soundcloud-local-unavailable".to_owned()),
            state: MusicPlaybackState::Unavailable,
            item: None,
            position_milliseconds: None,
            duration_milliseconds: None,
            observed_at,
            capabilities: MusicCapabilities::new().with(MusicCommand::OpenProvider),
        }
    }

    /// Validates and creates a provider observation.
    ///
    /// # Errors
    ///
    /// Returns [`MusicValidationError`] when the session identifier is invalid
    /// or the playback position is inconsistent with its duration. `SoundCloud`
    /// cannot supply playback, metadata, position, or transport capabilities.
    pub fn new(
        provider: MusicProvider,
        session_id: impl Into<String>,
        state: MusicPlaybackState,
        item: Option<MusicItem>,
        position: MusicPlaybackPosition,
        observed_at: MonotonicTimestamp,
        capabilities: MusicCapabilities,
    ) -> Result<Self, MusicValidationError> {
        if provider == MusicProvider::SoundCloud
            && (state != MusicPlaybackState::Unavailable
                || item.is_some()
                || position.position_milliseconds().is_some()
                || position.duration_milliseconds().is_some()
                || capabilities.0 & !MusicCommand::OpenProvider.bit() != 0)
        {
            return Err(MusicValidationError::ProviderObservationUnsupported);
        }
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

    /// Projects this observation as stale.
    ///
    /// The clock moves forward by one millisecond, unless it is already
    /// `u64::MAX`. A later caller clock replaces that step. Previous, play,
    /// pause, and next are cleared. Open-provider stays. Spotify play returns
    /// when that retained open-provider bit means reconnect is still possible.
    /// Item, position, duration, session, and provider stay.
    #[must_use]
    pub fn stale_projection(&self, observed_at: MonotonicTimestamp) -> Self {
        Self {
            state: MusicPlaybackState::Stale,
            observed_at: MonotonicTimestamp::new(advance_observed_at(
                self.observed_at.as_milliseconds(),
                observed_at.as_milliseconds(),
            )),
            capabilities: stale_observation_capabilities(
                self.provider,
                self.capabilities.supports(MusicCommand::OpenProvider),
            ),
            ..self.clone()
        }
    }
}

/// Validation failures at the music/provider boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MusicValidationError {
    /// The provider has no public interface for the reported observation.
    #[error("provider playback observation is unsupported")]
    ProviderObservationUnsupported,
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

/// Whether Spotify may expose Play so the listener can reconnect.
///
/// Stopped, disconnected, stale, and unavailable qualify when `can_reconnect`
/// is set. Playing, paused, buffering, interrupted, and unauthorized keep the
/// ordinary transport rules.
#[must_use]
pub const fn spotify_reconnect_grants_play(state: MusicPlaybackState, can_reconnect: bool) -> bool {
    if !can_reconnect {
        return false;
    }
    match state {
        MusicPlaybackState::Stopped
        | MusicPlaybackState::Disconnected
        | MusicPlaybackState::Stale
        | MusicPlaybackState::Unavailable => true,
        MusicPlaybackState::Playing
        | MusicPlaybackState::Paused
        | MusicPlaybackState::Buffering
        | MusicPlaybackState::Interrupted
        | MusicPlaybackState::Unauthorized => false,
    }
}

/// Moves an observation clock forward by one millisecond.
///
/// `u64::MAX` stays at the top of the clock. A later caller clock replaces
/// the one-millisecond step.
#[must_use]
pub const fn advance_observed_at(previous_ms: u64, requested_ms: u64) -> u64 {
    let advanced = if previous_ms == u64::MAX {
        u64::MAX
    } else {
        previous_ms + 1
    };
    if requested_ms > advanced {
        requested_ms
    } else {
        advanced
    }
}

/// Capabilities for a stale observation.
///
/// Open-provider is retained when the source observation had it. Spotify play
/// is restored only when reconnect is still possible for that retained bit.
#[must_use]
pub const fn stale_observation_capabilities(
    provider: MusicProvider,
    open_provider: bool,
) -> MusicCapabilities {
    let capabilities = if open_provider {
        MusicCapabilities::new().with(MusicCommand::OpenProvider)
    } else {
        MusicCapabilities::new()
    };
    let reconnect = match provider {
        MusicProvider::Spotify => {
            spotify_reconnect_grants_play(MusicPlaybackState::Stale, open_provider)
        }
        MusicProvider::AppleMusic | MusicProvider::SoundCloud => false,
    };
    if reconnect {
        capabilities.with(MusicCommand::Play)
    } else {
        capabilities
    }
}

/// Disconnected observation recorded when the app leaves the foreground.
///
/// The session id is `music-observation-gap`. Position and duration are absent.
/// The only capability is open-provider. A caller-supplied item is retained.
///
/// # Errors
///
/// Returns [`MusicValidationError::ProviderObservationUnsupported`] for
/// `SoundCloud`, which can only carry an unavailable open-provider snapshot.
pub fn observation_gap_snapshot(
    provider: MusicProvider,
    item: Option<MusicItem>,
    observed_at: MonotonicTimestamp,
) -> Result<MusicSnapshot, MusicValidationError> {
    MusicSnapshot::new(
        provider,
        "music-observation-gap",
        MusicPlaybackState::Disconnected,
        item,
        MusicPlaybackPosition::default(),
        observed_at,
        MusicCapabilities::new().with(MusicCommand::OpenProvider),
    )
}

/// Apple Music transport capabilities for one system-player observation.
///
/// Previous and next require an item. Play is available while paused or
/// stopped. Pause is available while playing. Open-provider is always available.
#[must_use]
pub const fn apple_music_transport_capabilities(
    item_present: bool,
    state: MusicPlaybackState,
) -> MusicCapabilities {
    let mut capabilities = MusicCapabilities::new().with(MusicCommand::OpenProvider);
    if item_present {
        capabilities = capabilities
            .with(MusicCommand::Previous)
            .with(MusicCommand::Next);
    }
    match state {
        MusicPlaybackState::Paused | MusicPlaybackState::Stopped => {
            capabilities.with(MusicCommand::Play)
        }
        MusicPlaybackState::Playing => capabilities.with(MusicCommand::Pause),
        MusicPlaybackState::Buffering
        | MusicPlaybackState::Interrupted
        | MusicPlaybackState::Unauthorized
        | MusicPlaybackState::Unavailable
        | MusicPlaybackState::Disconnected
        | MusicPlaybackState::Stale => capabilities,
    }
}

/// Maps an Android `PlaybackState` integer onto the shared playback state.
///
/// The platform constants are none `0`, stopped `1`, paused `2`, playing `3`,
/// fast-forwarding `4`, rewinding `5`, buffering `6`, and connecting `8`.
/// Playing, fast-forwarding, and rewinding are playing. Buffering and
/// connecting are buffering. Stopped and none are stopped. Every other value,
/// including error `7`, is unavailable.
#[must_use]
pub const fn android_media_playback_state(state: i32) -> MusicPlaybackState {
    match state {
        3..=5 => MusicPlaybackState::Playing,
        2 => MusicPlaybackState::Paused,
        6 | 8 => MusicPlaybackState::Buffering,
        0..=1 => MusicPlaybackState::Stopped,
        _ => MusicPlaybackState::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MusicCapabilities, MusicCommand, MusicItem, MusicPlaybackPosition, MusicPlaybackState,
        MusicProvider, MusicSnapshot, MusicValidationError, advance_observed_at,
        android_media_playback_state, apple_music_transport_capabilities, observation_gap_snapshot,
        spotify_reconnect_grants_play,
    };
    use cutout_core::MonotonicTimestamp;

    fn playing(provider: MusicProvider, open_provider: bool) -> MusicSnapshot {
        let mut capabilities = MusicCapabilities::new()
            .with(MusicCommand::Previous)
            .with(MusicCommand::Pause)
            .with(MusicCommand::Next);
        if open_provider {
            capabilities = capabilities.with(MusicCommand::OpenProvider);
        }
        MusicSnapshot::new(
            provider,
            "session",
            MusicPlaybackState::Playing,
            Some(
                MusicItem::new("track", Some("Song".to_owned()), Some("Artist".to_owned()))
                    .expect("item"),
            ),
            MusicPlaybackPosition::new(Some(10), Some(100)).expect("position"),
            MonotonicTimestamp::new(10),
            capabilities,
        )
        .expect("snapshot")
    }

    #[test]
    fn spotify_reconnect_play_is_limited_to_four_states() {
        for state in [
            MusicPlaybackState::Stopped,
            MusicPlaybackState::Disconnected,
            MusicPlaybackState::Stale,
            MusicPlaybackState::Unavailable,
        ] {
            assert!(spotify_reconnect_grants_play(state, true), "{state:?}");
            assert!(!spotify_reconnect_grants_play(state, false), "{state:?}");
        }
        for state in [
            MusicPlaybackState::Playing,
            MusicPlaybackState::Paused,
            MusicPlaybackState::Buffering,
            MusicPlaybackState::Interrupted,
            MusicPlaybackState::Unauthorized,
        ] {
            assert!(!spotify_reconnect_grants_play(state, true), "{state:?}");
        }
    }

    #[test]
    fn stale_projection_advances_the_clock_and_restores_only_spotify_reconnect_play() {
        let snapshot = playing(MusicProvider::Spotify, true);
        let stale = snapshot.stale_projection(MonotonicTimestamp::new(10));
        assert_eq!(stale.state(), MusicPlaybackState::Stale);
        assert_eq!(stale.observed_at(), MonotonicTimestamp::new(11));
        assert_eq!(stale.session_id().as_str(), "session");
        assert_eq!(stale.provider(), MusicProvider::Spotify);
        assert_eq!(
            stale.item().map(|item| item.identifier().as_str()),
            Some("track")
        );
        assert_eq!(stale.position_milliseconds(), Some(10));
        assert_eq!(stale.duration_milliseconds(), Some(100));
        assert!(stale.capabilities().supports(MusicCommand::Play));
        assert!(stale.capabilities().supports(MusicCommand::OpenProvider));
        assert!(!stale.capabilities().supports(MusicCommand::Previous));
        assert!(!stale.capabilities().supports(MusicCommand::Pause));
        assert!(!stale.capabilities().supports(MusicCommand::Next));
        assert_eq!(
            snapshot
                .stale_projection(MonotonicTimestamp::new(50))
                .observed_at(),
            MonotonicTimestamp::new(50)
        );
        assert_eq!(advance_observed_at(u64::MAX, 1), u64::MAX);
        assert_eq!(advance_observed_at(u64::MAX, u64::MAX), u64::MAX);
        let at_max = MusicSnapshot::new(
            MusicProvider::Spotify,
            "session",
            MusicPlaybackState::Paused,
            None,
            MusicPlaybackPosition::default(),
            MonotonicTimestamp::new(u64::MAX),
            MusicCapabilities::new().with(MusicCommand::OpenProvider),
        )
        .expect("max clock");
        assert_eq!(
            at_max
                .stale_projection(MonotonicTimestamp::new(1))
                .observed_at(),
            MonotonicTimestamp::new(u64::MAX)
        );

        let apple =
            playing(MusicProvider::AppleMusic, true).stale_projection(MonotonicTimestamp::new(10));
        assert!(!apple.capabilities().supports(MusicCommand::Play));
        assert!(!apple.capabilities().supports(MusicCommand::Pause));
        assert!(apple.capabilities().supports(MusicCommand::OpenProvider));

        let closed =
            playing(MusicProvider::Spotify, false).stale_projection(MonotonicTimestamp::new(10));
        assert!(!closed.capabilities().supports(MusicCommand::Play));
        assert!(!closed.capabilities().supports(MusicCommand::OpenProvider));
    }

    #[test]
    fn observation_gap_is_disconnected_with_only_open_provider() {
        let item = MusicItem::new("track", Some("Song".to_owned()), None).expect("item");
        let snapshot = observation_gap_snapshot(
            MusicProvider::AppleMusic,
            Some(item),
            MonotonicTimestamp::new(42),
        )
        .expect("apple gap");
        assert_eq!(snapshot.session_id().as_str(), "music-observation-gap");
        assert_eq!(snapshot.state(), MusicPlaybackState::Disconnected);
        assert_eq!(snapshot.observed_at(), MonotonicTimestamp::new(42));
        assert_eq!(snapshot.position_milliseconds(), None);
        assert_eq!(snapshot.duration_milliseconds(), None);
        assert_eq!(snapshot.item().and_then(MusicItem::title), Some("Song"));
        assert!(snapshot.capabilities().supports(MusicCommand::OpenProvider));
        assert!(!snapshot.capabilities().supports(MusicCommand::Play));
        assert!(!snapshot.capabilities().supports(MusicCommand::Pause));
        assert!(!snapshot.capabilities().supports(MusicCommand::Previous));
        assert!(!snapshot.capabilities().supports(MusicCommand::Next));
        assert!(
            observation_gap_snapshot(MusicProvider::Spotify, None, MonotonicTimestamp::new(1))
                .expect("spotify gap")
                .item()
                .is_none()
        );
        assert_eq!(
            observation_gap_snapshot(MusicProvider::SoundCloud, None, MonotonicTimestamp::new(1)),
            Err(MusicValidationError::ProviderObservationUnsupported)
        );
    }

    #[test]
    fn apple_music_transport_follows_item_presence_and_playback_state() {
        for state in [
            MusicPlaybackState::Playing,
            MusicPlaybackState::Paused,
            MusicPlaybackState::Stopped,
            MusicPlaybackState::Buffering,
            MusicPlaybackState::Interrupted,
            MusicPlaybackState::Unauthorized,
            MusicPlaybackState::Unavailable,
            MusicPlaybackState::Disconnected,
            MusicPlaybackState::Stale,
        ] {
            let with_item = apple_music_transport_capabilities(true, state);
            let without_item = apple_music_transport_capabilities(false, state);
            assert!(with_item.supports(MusicCommand::Previous), "{state:?}");
            assert!(with_item.supports(MusicCommand::Next), "{state:?}");
            assert!(with_item.supports(MusicCommand::OpenProvider), "{state:?}");
            assert!(!without_item.supports(MusicCommand::Previous), "{state:?}");
            assert!(!without_item.supports(MusicCommand::Next), "{state:?}");
            assert!(
                without_item.supports(MusicCommand::OpenProvider),
                "{state:?}"
            );
            let play = state == MusicPlaybackState::Paused || state == MusicPlaybackState::Stopped;
            let pause = state == MusicPlaybackState::Playing;
            assert_eq!(with_item.supports(MusicCommand::Play), play, "{state:?}");
            assert_eq!(with_item.supports(MusicCommand::Pause), pause, "{state:?}");
            assert_eq!(without_item.supports(MusicCommand::Play), play, "{state:?}");
            assert_eq!(
                without_item.supports(MusicCommand::Pause),
                pause,
                "{state:?}"
            );
        }
    }

    #[test]
    fn android_playback_state_uses_the_platform_integers() {
        let cases = [
            (3, MusicPlaybackState::Playing),
            (4, MusicPlaybackState::Playing),
            (5, MusicPlaybackState::Playing),
            (2, MusicPlaybackState::Paused),
            (6, MusicPlaybackState::Buffering),
            (8, MusicPlaybackState::Buffering),
            (1, MusicPlaybackState::Stopped),
            (0, MusicPlaybackState::Stopped),
            (7, MusicPlaybackState::Unavailable),
            (9, MusicPlaybackState::Unavailable),
            (10, MusicPlaybackState::Unavailable),
            (11, MusicPlaybackState::Unavailable),
            (-1, MusicPlaybackState::Unavailable),
        ];
        for (state, expected) in cases {
            assert_eq!(android_media_playback_state(state), expected, "{state}");
        }
    }
}
