//! Native `SoundCloud` player projection. The platform executes HTTP and audio only.

use cutout_music::soundcloud_player::{self as core, PlayableTrack, PlayerEffect};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Bounded official API data or selection failure.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileSoundCloudError {
    /// The Rust boundary rejected the query, response, or selection.
    #[error("Invalid SoundCloud request or response")]
    InvalidInput,
}

impl From<core::PlayerError> for MobileSoundCloudError {
    fn from(_: core::PlayerError) -> Self {
        Self::InvalidInput
    }
}

/// A fully playable track and its required uploader attribution.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MobileSoundCloudTrack {
    /// Official track URN.
    pub urn: String,
    /// Display title.
    pub title: String,
    /// `SoundCloud` uploader.
    pub uploader: String,
    /// Canonical public track page.
    pub permalink: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

impl From<PlayableTrack> for MobileSoundCloudTrack {
    fn from(track: PlayableTrack) -> Self {
        Self {
            urn: track.urn.as_str().to_owned(),
            title: track.title,
            uploader: track.uploader,
            permalink: track.permalink,
            duration_ms: track.duration_ms,
        }
    }
}

/// Metadata-only native player phase.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum MobileSoundCloudState {
    /// No selected track.
    Idle,
    /// Acquiring or preparing media.
    Loading,
    /// Playback started.
    Playing,
    /// Locally paused.
    Paused,
    /// Stream or player failure.
    Failed,
}

/// Snapshot is independent of ride history and never includes media URLs.
#[derive(Debug, uniffi::Record)]
pub struct MobileSoundCloudSnapshot {
    /// Current platform callback identity.
    pub playback_id: u64,
    /// Playback phase.
    pub state: MobileSoundCloudState,
    /// Selected track.
    pub track: Option<MobileSoundCloudTrack>,
    /// Whether a previous queued track exists.
    pub has_previous: bool,
    /// Whether a next queued track exists.
    pub has_next: bool,
}

/// Admitted platform operation. Never persist or log stream URLs.
#[derive(uniffi::Enum)]
#[allow(missing_debug_implementations)] // Signed URLs must not enter debug output.
pub enum MobileSoundCloudEffect {
    /// No operation.
    None,
    /// Acquire official streams for a selected track.
    Fetch {
        /// Callback identity.
        id: u64,
        /// Official API URL.
        endpoint: String,
    },
    /// Prepare media without automatically starting it.
    Prepare {
        /// Callback identity.
        id: u64,
        /// Admitted signed HLS URL.
        url: String,
    },
    /// Resume the existing platform player.
    Play,
    /// Pause the existing platform player.
    Pause,
}

impl From<PlayerEffect> for MobileSoundCloudEffect {
    fn from(effect: PlayerEffect) -> Self {
        match effect {
            PlayerEffect::None => Self::None,
            PlayerEffect::Fetch { id, endpoint } => Self::Fetch {
                id: id.value(),
                endpoint,
            },
            PlayerEffect::Prepare { id, url } => Self::Prepare {
                id: id.value(),
                url,
            },
            PlayerEffect::Play => Self::Play,
            PlayerEffect::Pause => Self::Pause,
        }
    }
}

/// Explicit native playback controls.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum MobileSoundCloudCommand {
    /// Resume.
    Play,
    /// Pause.
    Pause,
    /// Previous queued track.
    Previous,
    /// Next queued track.
    Next,
}

#[derive(Debug, Default)]
struct State {
    catalogue: Vec<PlayableTrack>,
    player: core::SoundCloudPlayer,
    authorization: Option<cutout_music::soundcloud_auth::ApiAuthorization>,
}

/// Rust-owned catalogue and playback queue retained by the app.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileSoundCloudPlayer {
    state: Mutex<State>,
}

impl MobileSoundCloudPlayer {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI owns strings/bytes at this boundary.
impl MobileSoundCloudPlayer {
    /// Creates an empty native player without credentials or I/O.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Fixed token-service URL. The mobile app never receives a client secret.
    #[must_use]
    pub fn token_endpoint(&self) -> String {
        cutout_music::soundcloud_auth::TOKEN_ENDPOINT.to_owned()
    }

    /// Accepts an expiring app token from the broker without storing it on disk.
    ///
    /// # Errors
    /// Rejects malformed token responses or invalid lifetimes.
    pub fn accept_authorization(
        &self,
        json: Vec<u8>,
        now_ms: u64,
    ) -> Result<(), MobileSoundCloudError> {
        let authorization = cutout_music::soundcloud_auth::ApiAuthorization::parse(&json, now_ms)?;
        self.lock().authorization = Some(authorization);
        Ok(())
    }

    /// Projects a provider header only within the admitted token lifetime.
    #[must_use]
    pub fn authorization(&self, now_ms: u64) -> Option<String> {
        self.lock()
            .authorization
            .as_ref()
            .and_then(|authorization| authorization.header(now_ms))
    }

    /// Invalidates an expired or rejected token; local playback remains independent.
    pub fn invalidate_authorization(&self) {
        self.lock().authorization = None;
    }

    /// Builds the official public track search URL.
    ///
    /// # Errors
    /// Rejects blank or oversized queries.
    pub fn search_endpoint(&self, query: String) -> Result<String, MobileSoundCloudError> {
        Ok(core::search_endpoint(&query)?)
    }

    /// Admits a bounded catalogue. This never replaces the current playback queue.
    ///
    /// # Errors
    /// Rejects malformed or oversized responses.
    pub fn accept_catalogue(
        &self,
        json: Vec<u8>,
    ) -> Result<Vec<MobileSoundCloudTrack>, MobileSoundCloudError> {
        let catalogue = core::parse_tracks(&json)?;
        let tracks = catalogue.iter().cloned().map(Into::into).collect();
        self.lock().catalogue = catalogue;
        Ok(tracks)
    }

    /// Selects a track from the last admitted catalogue.
    ///
    /// # Errors
    /// Rejects unknown track identities.
    pub fn select(&self, urn: String) -> Result<MobileSoundCloudEffect, MobileSoundCloudError> {
        let mut state = self.lock();
        let State {
            catalogue, player, ..
        } = &mut *state;
        Ok(player.select(catalogue, &urn)?.into())
    }

    /// Admits the full AAC HLS API endpoint, excluding previews.
    ///
    /// # Errors
    /// Rejects unsupported streams or malformed data.
    pub fn stream_endpoint(&self, json: Vec<u8>) -> Result<String, MobileSoundCloudError> {
        Ok(core::stream_endpoint(&json)?)
    }

    /// Admits the provider's signed redirect only for the current track fetch.
    ///
    /// # Errors
    /// Rejects unsupported media origins.
    pub fn stream_resolved(
        &self,
        id: u64,
        url: String,
    ) -> Result<MobileSoundCloudEffect, MobileSoundCloudError> {
        Ok(self.lock().player.stream_resolved(id, &url)?.into())
    }

    /// Commits a current platform readiness callback.
    pub fn player_ready(&self, id: u64) -> MobileSoundCloudEffect {
        self.lock().player.player_ready(id).into()
    }

    /// Handles current item completion; stale notifications cannot skip a newer track.
    ///
    /// # Errors
    /// Returns an error if the next track identity cannot be allocated.
    pub fn ended(&self, id: u64) -> Result<MobileSoundCloudEffect, MobileSoundCloudError> {
        Ok(self.lock().player.ended(id)?.into())
    }

    /// Commits a current platform failure callback.
    pub fn failed(&self, id: u64) -> MobileSoundCloudEffect {
        self.lock().player.failed(id).into()
    }

    /// Invalidates the native queue and all outstanding stream callbacks.
    pub fn stop(&self) -> MobileSoundCloudEffect {
        self.lock().player.stop().into()
    }

    /// Local pause/resume does not require HTTP or provider authorization.
    ///
    /// # Errors
    /// Returns an error if a track transition cannot be allocated.
    pub fn command(
        &self,
        command: MobileSoundCloudCommand,
    ) -> Result<MobileSoundCloudEffect, MobileSoundCloudError> {
        let command = match command {
            MobileSoundCloudCommand::Play => core::PlayerCommand::Play,
            MobileSoundCloudCommand::Pause => core::PlayerCommand::Pause,
            MobileSoundCloudCommand::Previous => core::PlayerCommand::Previous,
            MobileSoundCloudCommand::Next => core::PlayerCommand::Next,
        };
        Ok(self.lock().player.command(command)?.into())
    }

    /// Projects current metadata and queue capabilities without recording history.
    #[must_use]
    pub fn snapshot(&self) -> MobileSoundCloudSnapshot {
        let snapshot = self.lock().player.snapshot();
        let state = match snapshot.state {
            core::PlayerState::Idle => MobileSoundCloudState::Idle,
            core::PlayerState::Loading => MobileSoundCloudState::Loading,
            core::PlayerState::Playing => MobileSoundCloudState::Playing,
            core::PlayerState::Paused => MobileSoundCloudState::Paused,
            core::PlayerState::Failed => MobileSoundCloudState::Failed,
        };
        MobileSoundCloudSnapshot {
            playback_id: snapshot.playback_id.value(),
            state,
            track: snapshot.track.map(Into::into),
            has_previous: snapshot.has_previous,
            has_next: snapshot.has_next,
        }
    }
    /// Presentation-only projection for the existing compact player. No history is admitted.
    #[must_use]
    pub fn music_snapshot(&self, now_ms: u64) -> crate::MobileMusicSnapshotDto {
        use crate::{
            MobileMusicCapabilitiesDto, MobileMusicItemDto,
            MobileMusicPlaybackStateDto as Playback, MobileMusicProviderDto,
            MobileMusicSnapshotDto,
        };
        let snapshot = self.snapshot();
        let (state, play, pause) = match snapshot.state {
            MobileSoundCloudState::Idle => (Playback::Unavailable, false, false),
            MobileSoundCloudState::Loading => (Playback::Buffering, false, true),
            MobileSoundCloudState::Playing => (Playback::Playing, false, true),
            MobileSoundCloudState::Paused => (Playback::Paused, true, false),
            MobileSoundCloudState::Failed => (Playback::Unavailable, true, false),
        };
        let duration_milliseconds = snapshot.track.as_ref().map(|track| track.duration_ms);
        MobileMusicSnapshotDto {
            provider: MobileMusicProviderDto::SoundCloud,
            session_id: "soundcloud-native".to_owned(),
            state,
            item: snapshot.track.map(|track| MobileMusicItemDto {
                identifier: track.urn,
                title: Some(track.title),
                artist: Some(track.uploader),
            }),
            position_milliseconds: None,
            duration_milliseconds,
            observed_at_ms: now_ms,
            capabilities: MobileMusicCapabilitiesDto {
                previous: snapshot.has_previous,
                next: snapshot.has_next,
                play,
                pause,
                open_provider: true,
            },
        }
    }
}
