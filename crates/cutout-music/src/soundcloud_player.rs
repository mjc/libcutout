//! Official `SoundCloud` catalogue and native-player admission, without I/O.

use serde::Deserialize;
use url::Url;

/// Maximum accepted catalogue/stream response size.
pub const MAX_RESPONSE_BYTES: usize = 512 * 1024;
/// Maximum tracks admitted into a playback queue.
pub const MAX_TRACKS: usize = 30;

/// Invalid official API data or a request outside the admitted catalogue.
#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum PlayerError {
    /// Input exceeds the bounded API contract.
    #[error("SoundCloud input is too large")]
    TooLarge,
    /// The search query is blank or too long.
    #[error("Enter a search of at most 256 bytes")]
    InvalidQuery,
    /// The provider returned malformed or unsupported data.
    #[error("SoundCloud returned an invalid response")]
    InvalidResponse,
    /// The selected track is not in the admitted catalogue.
    #[error("Select a playable SoundCloud track")]
    UnknownTrack,
}

/// Validated official track identity; arbitrary paths cannot enter requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackUrn(String);

impl TrackUrn {
    fn parse(value: String) -> Result<Self, PlayerError> {
        let id = value
            .strip_prefix("soundcloud:tracks:")
            .ok_or(PlayerError::InvalidResponse)?;
        if id.is_empty() || id.len() > 20 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(PlayerError::InvalidResponse);
        }
        Ok(Self(value))
    }

    /// The validated provider identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn stream_endpoint(&self) -> String {
        format!("https://api.soundcloud.com/tracks/{}/streams", self.0)
    }
}

/// A track admitted for full playback, with required provider attribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayableTrack {
    /// Provider URN.
    pub urn: TrackUrn,
    /// Bounded track title.
    pub title: String,
    /// Bounded uploader name, retained even when artist metadata differs.
    pub uploader: String,
    /// Canonical public track page.
    pub permalink: String,
    /// Track duration in milliseconds.
    pub duration_ms: u64,
}

#[derive(Deserialize)]
struct Catalogue {
    collection: Vec<WireTrack>,
}
#[derive(Deserialize)]
struct WireTrack {
    access: Option<TrackAccess>,
    streamable: Option<bool>,
    urn: Option<String>,
    title: Option<String>,
    duration: Option<u64>,
    permalink_url: Option<String>,
    user: Option<WireUser>,
}
#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum TrackAccess {
    Playable,
    Preview,
    Blocked,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
struct WireUser {
    username: String,
}

fn display_text(value: &str) -> String {
    value.trim().chars().take(256).collect()
}

fn https_url(value: &str, hosts: &[&str]) -> Result<Url, PlayerError> {
    let url = Url::parse(value).map_err(|_| PlayerError::InvalidResponse)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || !hosts.iter().any(|host| Some(*host) == url.host_str())
    {
        return Err(PlayerError::InvalidResponse);
    }
    Ok(url)
}

/// Builds an official, bounded public search request without any credentials.
///
/// # Errors
/// Returns an error for blank or oversized queries.
pub fn search_endpoint(query: &str) -> Result<String, PlayerError> {
    let query = query.trim();
    if query.is_empty() || query.len() > 256 {
        return Err(PlayerError::InvalidQuery);
    }
    let mut url = https_url("https://api.soundcloud.com/tracks", &["api.soundcloud.com"])?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("access", "playable")
        .append_pair("limit", "30")
        .append_pair("linked_partitioning", "true");
    Ok(url.into())
}

/// Parses playable tracks from an official linked-partition search response.
/// Unknown access modes, previews, blocked tracks, and incomplete rows are excluded.
///
/// # Errors
/// Rejects oversized or malformed JSON.
pub fn parse_tracks(json: &[u8]) -> Result<Vec<PlayableTrack>, PlayerError> {
    if json.len() > MAX_RESPONSE_BYTES {
        return Err(PlayerError::TooLarge);
    }
    let catalogue: Catalogue =
        serde_json::from_slice(json).map_err(|_| PlayerError::InvalidResponse)?;
    let mut tracks = Vec::new();
    for row in catalogue.collection {
        if row.access != Some(TrackAccess::Playable) || row.streamable != Some(true) {
            continue;
        }
        let (Some(urn), Some(title), Some(duration_ms), Some(permalink), Some(user)) = (
            row.urn,
            row.title,
            row.duration,
            row.permalink_url,
            row.user,
        ) else {
            continue;
        };
        let Ok(urn) = TrackUrn::parse(urn) else {
            continue;
        };
        let Ok(permalink) = https_url(&permalink, &["soundcloud.com", "www.soundcloud.com"]) else {
            continue;
        };
        let title = display_text(&title);
        let uploader = display_text(&user.username);
        if title.is_empty()
            || uploader.is_empty()
            || duration_ms == 0
            || tracks.iter().any(|track: &PlayableTrack| track.urn == urn)
        {
            continue;
        }
        tracks.push(PlayableTrack {
            urn,
            title,
            uploader,
            permalink: permalink.into(),
            duration_ms,
        });
        if tracks.len() == MAX_TRACKS {
            break;
        }
    }
    Ok(tracks)
}

/// Selects the official AAC HLS endpoint; preview audio is never a fallback.
///
/// # Errors
/// Rejects missing full-playback streams or URLs outside the authenticated API.
pub fn stream_endpoint(json: &[u8]) -> Result<String, PlayerError> {
    #[derive(Deserialize)]
    struct Streams {
        hls_aac_160_url: String,
    }
    if json.len() > MAX_RESPONSE_BYTES {
        return Err(PlayerError::TooLarge);
    }
    let streams: Streams =
        serde_json::from_slice(json).map_err(|_| PlayerError::InvalidResponse)?;
    Ok(https_url(&streams.hls_aac_160_url, &["api.soundcloud.com"])?.into())
}

/// Admits a signed HLS URL returned by the official stream redirect.
/// The executor must never forward OAuth credentials to this host.
///
/// # Errors
/// Rejects arbitrary media origins, non-HTTPS URLs, and embedded credentials.
pub fn admit_media_url(value: &str) -> Result<String, PlayerError> {
    Ok(https_url(value, &["playback.media-streaming.soundcloud.cloud"])?.into())
}

/// Identity fencing stream fetches and platform callbacks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackId(u64);

impl PlaybackId {
    /// Numeric projection for platform callbacks.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Explicit native-player commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerCommand {
    /// Resume the current track.
    Play,
    /// Pause locally, including a pending autoplay.
    Pause,
    /// Select the previous track in the current queue.
    Previous,
    /// Select the next track in the current queue.
    Next,
}

/// Effects executed by the platform; credentials are not part of the model.
pub enum PlayerEffect {
    /// No platform operation is admitted.
    None,
    /// Fetch the selected track's official stream descriptors.
    Fetch {
        /// Current playback lease.
        id: PlaybackId,
        /// Official API endpoint.
        endpoint: String,
    },
    /// Prepare a validated media URL without autoplay.
    Prepare {
        /// Current playback lease.
        id: PlaybackId,
        /// Admitted signed HLS URL; never persist or log it.
        url: String,
    },
    /// Resume the already prepared platform player.
    Play,
    /// Pause the platform player locally.
    Pause,
}

/// Native-player presentation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerState {
    /// No track has been selected.
    Idle,
    /// Waiting for stream acquisition or platform preparation.
    Loading,
    /// Playback was explicitly started.
    Playing,
    /// Playback is locally paused.
    Paused,
    /// The current acquisition or player failed.
    Failed,
}

/// A queue snapshot contains metadata only; no audio or signed stream URL.
#[derive(Debug)]
pub struct PlayerSnapshot {
    /// Current platform callback identity.
    pub playback_id: PlaybackId,
    /// Current player phase.
    pub state: PlayerState,
    /// Selected track, if any.
    pub track: Option<PlayableTrack>,
    /// Whether a previous queued track is available.
    pub has_previous: bool,
    /// Whether a next queued track is available.
    pub has_next: bool,
}

impl std::fmt::Debug for PlayerEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::Fetch { id, .. } => f.debug_tuple("Fetch").field(id).finish(),
            Self::Prepare { id, .. } => f.debug_tuple("Prepare").field(id).finish(),
            Self::Play => f.write_str("Play"),
            Self::Pause => f.write_str("Pause"),
        }
    }
}

#[derive(Debug)]
enum Phase {
    Idle,
    Fetching { autoplay: bool },
    Preparing { autoplay: bool },
    Ready { playing: bool },
    Failed,
}

/// Rust owner for a bounded native playback queue.
#[derive(Debug)]
pub struct SoundCloudPlayer {
    queue: Vec<PlayableTrack>,
    index: usize,
    id: PlaybackId,
    phase: Phase,
}

impl Default for SoundCloudPlayer {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            index: 0,
            id: PlaybackId(0),
            phase: Phase::Idle,
        }
    }
}

impl SoundCloudPlayer {
    /// Selects a track from the last admitted catalogue and replaces the queue.
    ///
    /// # Errors
    /// Rejects selections absent from the bounded catalogue.
    pub fn select(
        &mut self,
        tracks: &[PlayableTrack],
        urn: &str,
    ) -> Result<PlayerEffect, PlayerError> {
        let index = tracks
            .iter()
            .take(MAX_TRACKS)
            .position(|track| track.urn.as_str() == urn)
            .ok_or(PlayerError::UnknownTrack)?;
        self.queue = tracks.iter().take(MAX_TRACKS).cloned().collect();
        self.fetch(index)
    }

    fn fetch(&mut self, index: usize) -> Result<PlayerEffect, PlayerError> {
        let track = self.queue.get(index).ok_or(PlayerError::UnknownTrack)?;
        let next = self
            .id
            .0
            .checked_add(1)
            .ok_or(PlayerError::InvalidResponse)?;
        self.id = PlaybackId(next);
        self.index = index;
        self.phase = Phase::Fetching { autoplay: true };
        Ok(PlayerEffect::Fetch {
            id: self.id,
            endpoint: track.urn.stream_endpoint(),
        })
    }

    /// Admits a signed stream only for the current outstanding fetch.
    ///
    /// # Errors
    /// Rejects an invalid media origin. Stale callbacks return no effect.
    pub fn stream_resolved(&mut self, id: u64, url: &str) -> Result<PlayerEffect, PlayerError> {
        if id != self.id.0 {
            return Ok(PlayerEffect::None);
        }
        let Phase::Fetching { autoplay } = self.phase else {
            return Ok(PlayerEffect::None);
        };
        let url = admit_media_url(url)?;
        self.phase = Phase::Preparing { autoplay };
        Ok(PlayerEffect::Prepare { id: self.id, url })
    }

    /// Commits platform readiness, honoring pause commands issued while loading.
    pub fn player_ready(&mut self, id: u64) -> PlayerEffect {
        if id != self.id.0 {
            return PlayerEffect::None;
        }
        let Phase::Preparing { autoplay } = self.phase else {
            return PlayerEffect::None;
        };
        self.phase = Phase::Ready { playing: autoplay };
        if autoplay {
            PlayerEffect::Play
        } else {
            PlayerEffect::Pause
        }
    }

    /// Handles completion only for the current prepared item.
    ///
    /// # Errors
    /// Returns an error only if the next identity cannot be allocated.
    pub fn ended(&mut self, id: u64) -> Result<PlayerEffect, PlayerError> {
        if id != self.id.0 {
            return Ok(PlayerEffect::None);
        }
        let Phase::Ready { .. } = self.phase else {
            return Ok(PlayerEffect::None);
        };
        if self.index + 1 < self.queue.len() {
            self.fetch(self.index + 1)
        } else {
            self.command(PlayerCommand::Pause)
        }
    }

    /// Records only a current platform/acquisition failure.
    pub fn failed(&mut self, id: u64) -> PlayerEffect {
        if id != self.id.0 {
            return PlayerEffect::None;
        }
        if let Phase::Idle = self.phase {
            return PlayerEffect::None;
        }
        self.phase = Phase::Failed;
        PlayerEffect::Pause
    }

    /// Clears playback and invalidates every outstanding callback.
    pub fn stop(&mut self) -> PlayerEffect {
        self.queue.clear();
        self.phase = Phase::Idle;
        // No callback can match an outstanding phase after stop, even at exhaustion.
        self.id = PlaybackId(self.id.0.saturating_add(1));
        PlayerEffect::Pause
    }

    /// Executes local transport or selects another queued track.
    ///
    /// # Errors
    /// Returns an error only if a new playback identity cannot be allocated.
    pub fn command(&mut self, command: PlayerCommand) -> Result<PlayerEffect, PlayerError> {
        match command {
            PlayerCommand::Next if self.index + 1 < self.queue.len() => self.fetch(self.index + 1),
            PlayerCommand::Previous if self.index > 0 => self.fetch(self.index - 1),
            PlayerCommand::Next | PlayerCommand::Previous => Ok(PlayerEffect::None),
            PlayerCommand::Play | PlayerCommand::Pause => {
                let playing = command == PlayerCommand::Play;
                match &mut self.phase {
                    Phase::Fetching { autoplay } | Phase::Preparing { autoplay } => {
                        *autoplay = playing;
                        Ok(PlayerEffect::None)
                    }
                    Phase::Ready { playing: current } => {
                        *current = playing;
                        Ok(if playing {
                            PlayerEffect::Play
                        } else {
                            PlayerEffect::Pause
                        })
                    }
                    Phase::Failed if playing => self.fetch(self.index),
                    Phase::Idle | Phase::Failed => Ok(PlayerEffect::None),
                }
            }
        }
    }

    /// Presentation derives from the admitted queue and current playback phase.
    #[must_use]
    pub fn snapshot(&self) -> PlayerSnapshot {
        let state = match self.phase {
            Phase::Idle => PlayerState::Idle,
            Phase::Fetching { autoplay: true } | Phase::Preparing { autoplay: true } => {
                PlayerState::Loading
            }
            Phase::Fetching { autoplay: false } | Phase::Preparing { autoplay: false } => {
                PlayerState::Paused
            }
            Phase::Ready { playing: true } => PlayerState::Playing,
            Phase::Ready { playing: false } => PlayerState::Paused,
            Phase::Failed => PlayerState::Failed,
        };
        PlayerSnapshot {
            playback_id: self.id,
            state,
            track: self.queue.get(self.index).cloned(),
            has_previous: self.index > 0 && !self.queue.is_empty(),
            has_next: self.index + 1 < self.queue.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_admits_full_playback_and_excludes_previews_and_blocked_tracks() {
        let tracks = parse_tracks(
            br#"{"collection":[
            {"urn":"soundcloud:tracks:42","title":"Song","duration":120000,
             "permalink_url":"https://soundcloud.com/artist/song",
             "user":{"username":"Artist"},"access":"playable","streamable":true},
            {"urn":"soundcloud:tracks:43","access":"preview","streamable":true},
            {"urn":"soundcloud:tracks:44","access":"blocked","streamable":false}
        ]}"#,
        )
        .expect("valid official response");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].urn.as_str(), "soundcloud:tracks:42");
    }
    fn queue_tracks() -> Vec<PlayableTrack> {
        parse_tracks(
            br#"{"collection":[
            {"urn":"soundcloud:tracks:42","title":"First","duration":120000,
             "permalink_url":"https://soundcloud.com/artist/first",
             "user":{"username":"Artist"},"access":"playable","streamable":true},
            {"urn":"soundcloud:tracks:43","title":"Second","duration":120000,
             "permalink_url":"https://soundcloud.com/artist/second",
             "user":{"username":"Artist"},"access":"playable","streamable":true}
        ]}"#,
        )
        .expect("valid catalogue")
    }

    #[test]
    fn selecting_a_catalogue_track_requests_its_official_stream() {
        let mut player = SoundCloudPlayer::default();
        match player
            .select(&queue_tracks(), "soundcloud:tracks:43")
            .expect("admitted track")
        {
            PlayerEffect::Fetch { endpoint, .. } => {
                assert_eq!(
                    endpoint,
                    "https://api.soundcloud.com/tracks/soundcloud:tracks:43/streams"
                );
            }
            _ => panic!("selecting a playable track must acquire its stream"),
        }
    }
    fn fetch_id(effect: &PlayerEffect) -> u64 {
        match effect {
            PlayerEffect::Fetch { id, .. } => id.value(),
            _ => panic!("expected stream fetch"),
        }
    }

    const MEDIA: &str =
        "https://playback.media-streaming.soundcloud.cloud/manifest.m3u8?token=ephemeral";

    #[test]
    fn pause_while_loading_prevents_delayed_autoplay() {
        let mut player = SoundCloudPlayer::default();
        let id = fetch_id(
            &player
                .select(&queue_tracks(), "soundcloud:tracks:42")
                .expect("selected"),
        );
        assert!(
            match player.command(PlayerCommand::Pause).expect("paused") {
                PlayerEffect::None => true,
                _ => false,
            }
        );
        assert_eq!(player.snapshot().state, PlayerState::Paused);
        assert!(
            match player.stream_resolved(id, MEDIA).expect("valid media") {
                PlayerEffect::Prepare { .. } => true,
                _ => false,
            }
        );
        assert!(match player.player_ready(id) {
            PlayerEffect::Pause => true,
            _ => false,
        });
        assert_eq!(player.snapshot().state, PlayerState::Paused);
    }

    #[test]
    fn local_pause_and_resume_emit_no_network_effects() {
        let mut player = SoundCloudPlayer::default();
        let id = fetch_id(
            &player
                .select(&queue_tracks(), "soundcloud:tracks:42")
                .expect("selected"),
        );
        player.stream_resolved(id, MEDIA).expect("valid media");
        assert!(match player.player_ready(id) {
            PlayerEffect::Play => true,
            _ => false,
        });
        assert!(match player.command(PlayerCommand::Pause).expect("pause") {
            PlayerEffect::Pause => true,
            _ => false,
        });
        assert_eq!(player.snapshot().state, PlayerState::Paused);
        assert!(match player.command(PlayerCommand::Play).expect("play") {
            PlayerEffect::Play => true,
            _ => false,
        });
        assert_eq!(player.snapshot().state, PlayerState::Playing);
    }

    #[test]
    fn skip_fences_old_stream_ready_and_failure_callbacks() {
        let mut player = SoundCloudPlayer::default();
        let old = fetch_id(
            &player
                .select(&queue_tracks(), "soundcloud:tracks:42")
                .expect("selected"),
        );
        let current = fetch_id(&player.command(PlayerCommand::Next).expect("next"));
        assert_ne!(old, current);
        assert!(match player.stream_resolved(old, MEDIA).expect("stale") {
            PlayerEffect::None => true,
            _ => false,
        });
        assert!(match player.player_ready(old) {
            PlayerEffect::None => true,
            _ => false,
        });
        assert!(match player.failed(old) {
            PlayerEffect::None => true,
            _ => false,
        });
        assert_eq!(
            player.snapshot().track.expect("current track").urn.as_str(),
            "soundcloud:tracks:43"
        );
        player
            .stream_resolved(current, MEDIA)
            .expect("current stream");
        assert!(match player.player_ready(current) {
            PlayerEffect::Play => true,
            _ => false,
        });
        assert!(!player.snapshot().has_next);
        assert!(
            match player.command(PlayerCommand::Next).expect("queue end") {
                PlayerEffect::None => true,
                _ => false,
            }
        );
        assert!(
            match player.command(PlayerCommand::Previous).expect("previous") {
                PlayerEffect::Fetch { .. } => true,
                _ => false,
            }
        );
    }

    #[test]
    fn stopping_invalidates_fetches_and_clears_metadata() {
        let mut player = SoundCloudPlayer::default();
        let old = fetch_id(
            &player
                .select(&queue_tracks(), "soundcloud:tracks:42")
                .expect("selected"),
        );
        assert!(match player.stop() {
            PlayerEffect::Pause => true,
            _ => false,
        });
        assert!(match player.stream_resolved(old, MEDIA).expect("stale") {
            PlayerEffect::None => true,
            _ => false,
        });
        assert_eq!(player.snapshot().state, PlayerState::Idle);
        assert!(player.snapshot().track.is_none());
    }

    #[test]
    fn stream_boundary_rejects_previews_and_credential_forwarding_targets() {
        assert_eq!(
            stream_endpoint(br#"{"preview_mp3_128_url":"https://api.soundcloud.com/preview"}"#),
            Err(PlayerError::InvalidResponse)
        );
        assert_eq!(
            stream_endpoint(
                br#"{"hls_aac_160_url":"https://api.soundcloud.com.evil.test/stream"}"#
            ),
            Err(PlayerError::InvalidResponse)
        );
        for url in [
            "http://playback.media-streaming.soundcloud.cloud/a",
            "https://evil.test/a",
            "https://user:password@playback.media-streaming.soundcloud.cloud/a",
            "https://playback.media-streaming.soundcloud.cloud:8443/a",
        ] {
            assert_eq!(admit_media_url(url), Err(PlayerError::InvalidResponse));
        }
        assert_eq!(
            admit_media_url(MEDIA).expect("official signed redirect"),
            MEDIA
        );
    }

    #[test]
    fn searches_encode_input_as_data_and_reject_blank_queries() {
        assert_eq!(search_endpoint("  "), Err(PlayerError::InvalidQuery));
        let url = Url::parse(&search_endpoint("house&limit=999").expect("search")).expect("URL");
        let pairs: Vec<_> = url.query_pairs().collect();
        assert_eq!(pairs[0].1, "house&limit=999");
        assert_eq!(pairs[2].1, "30");
        assert_eq!(
            parse_tracks(&vec![b' '; MAX_RESPONSE_BYTES + 1]),
            Err(PlayerError::TooLarge)
        );
    }
    #[test]
    fn item_completion_advances_only_the_current_ready_track() {
        let mut player = SoundCloudPlayer::default();
        let id = fetch_id(
            &player
                .select(&queue_tracks(), "soundcloud:tracks:42")
                .expect("selected"),
        );
        player.stream_resolved(id, MEDIA).expect("stream");
        player.player_ready(id);
        assert!(match player.ended(id + 1).expect("stale") {
            PlayerEffect::None => true,
            _ => false,
        });
        assert!(match player.ended(id).expect("current end") {
            PlayerEffect::Fetch { .. } => true,
            _ => false,
        });
        assert_eq!(
            player.snapshot().track.expect("next track").urn.as_str(),
            "soundcloud:tracks:43"
        );
    }
}
