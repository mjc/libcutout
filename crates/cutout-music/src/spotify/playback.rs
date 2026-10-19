//! Bounded Spotify Web API observations and request policy. Credentials and I/O stay native.

use std::collections::BTreeMap;

use cutout_core::MonotonicTimestamp;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::{
    MusicCapabilities, MusicCommand, MusicIdentifier, MusicItem, MusicPlaybackPosition,
    MusicPlaybackState, MusicProvider, MusicSnapshot,
};

const MAX_RESPONSE_BYTES: u64 = 1_048_576;

/// Provider failures without response bodies, credentials or URLs in diagnostic text.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SpotifyPlaybackError {
    /// The bounded response is not a valid playback observation.
    #[error("invalid Spotify playback response")]
    InvalidPayload,
    /// Access-token renewal is required.
    #[error("Spotify authorization expired")]
    Unauthorized,
    /// The account, device or grant refuses this operation.
    #[error("Spotify playback access denied")]
    Forbidden,
    /// No request may be issued before the provider's retry deadline.
    #[error("Spotify playback rate limited")]
    RateLimited {
        /// Remaining delay on the host monotonic clock.
        retry_after_milliseconds: u64,
    },
    /// The provider cannot supply this response or operation.
    #[error("Spotify playback unavailable")]
    Unavailable,
}

/// Parsed and validated provider metadata; no credential or audio content is retained.
#[derive(Clone, Debug)]
pub struct SpotifyPlayback {
    base: MusicSnapshot,
    device_id: Option<MusicIdentifier>,
    restricted: bool,
    is_playing: bool,
    allowed: MusicCapabilities,
    artwork_url: Option<String>,
}

impl SpotifyPlayback {
    /// Parses a bounded playback response and validates shared music invariants.
    ///
    /// # Errors
    /// Returns [`SpotifyPlaybackError::InvalidPayload`] for malformed, oversized or inconsistent input.
    pub fn parse(body: &[u8]) -> Result<Self, SpotifyPlaybackError> {
        if u64::try_from(body.len()).unwrap_or(u64::MAX) > MAX_RESPONSE_BYTES {
            return Err(SpotifyPlaybackError::InvalidPayload);
        }
        let wire: WirePlayback =
            serde_json::from_slice(body).map_err(|_| SpotifyPlaybackError::InvalidPayload)?;
        let artwork_url = wire.artwork_url();
        let position = MusicPlaybackPosition::new(
            wire.progress_ms.and_then(|v| u64::try_from(v).ok()),
            wire.item
                .as_ref()
                .and_then(|v| v.duration_ms)
                .and_then(|v| u64::try_from(v).ok()),
        )
        .map_err(|_| SpotifyPlaybackError::InvalidPayload)?;
        let item = wire
            .item
            .as_ref()
            .and_then(WireItem::known_item)
            .transpose()
            .map_err(|_| SpotifyPlaybackError::InvalidPayload)?;
        let state = if wire.is_playing {
            MusicPlaybackState::Playing
        } else {
            MusicPlaybackState::Paused
        };
        let base = MusicSnapshot::new(
            MusicProvider::Spotify,
            "spotify-web-api",
            state,
            item,
            position,
            MonotonicTimestamp::new(0),
            MusicCapabilities::new(),
        )
        .map_err(|_| SpotifyPlaybackError::InvalidPayload)?;
        let mut allowed = MusicCapabilities::new().with(MusicCommand::OpenProvider);
        for (action, command) in [
            ("skipping_prev", MusicCommand::Previous),
            ("resuming", MusicCommand::Play),
            ("pausing", MusicCommand::Pause),
            ("skipping_next", MusicCommand::Next),
        ] {
            if wire
                .actions
                .as_ref()
                .and_then(|a| a.disallows.as_ref())
                .and_then(|a| a.get(action))
                != Some(&true)
            {
                allowed = allowed.with(command);
            }
        }
        let device_id = wire
            .device
            .as_ref()
            .and_then(|v| v.id.as_ref())
            .filter(|id| !id.trim().is_empty())
            .map(|id| MusicIdentifier::new(id.clone()))
            .transpose()
            .map_err(|_| SpotifyPlaybackError::InvalidPayload)?;
        Ok(Self {
            base,
            device_id,
            restricted: wire
                .device
                .as_ref()
                .and_then(|v| v.is_restricted)
                .unwrap_or(false),
            is_playing: wire.is_playing,
            allowed,
            artwork_url,
        })
    }

    /// Current provider playback flag; this does not start playback.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// Device selected by the latest provider observation, never a replacement device.
    #[must_use]
    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_ref().map(MusicIdentifier::as_str)
    }

    /// Known track or episode identifier for bounded native artwork caching.
    #[must_use]
    pub fn item_identifier(&self) -> Option<&str> {
        self.base.item().map(|item| item.identifier().as_str())
    }

    /// An HTTPS artwork URL without embedded credentials.
    #[must_use]
    pub fn artwork_url(&self) -> Option<&str> {
        self.artwork_url.as_deref()
    }

    /// Retains the last metadata when Spotify reports no active device.
    #[must_use]
    pub fn without_active_device(&self) -> Self {
        Self {
            device_id: None,
            is_playing: false,
            ..self.clone()
        }
    }

    /// Projects lifecycle freshness and provider restrictions into shared capabilities.
    #[must_use]
    pub fn snapshot(
        &self,
        state: MusicPlaybackState,
        observed_at: MonotonicTimestamp,
    ) -> MusicSnapshot {
        let controls = match state {
            MusicPlaybackState::Playing | MusicPlaybackState::Paused => {
                self.device_id.is_some() && !self.restricted
            }
            MusicPlaybackState::Stopped
            | MusicPlaybackState::Buffering
            | MusicPlaybackState::Interrupted
            | MusicPlaybackState::Unauthorized
            | MusicPlaybackState::Unavailable
            | MusicPlaybackState::Disconnected
            | MusicPlaybackState::Stale => false,
        };
        let mut capabilities = MusicCapabilities::new().with(MusicCommand::OpenProvider);
        if controls {
            for command in [
                MusicCommand::Previous,
                MusicCommand::Next,
                MusicCommand::Play,
                MusicCommand::Pause,
            ] {
                let state_allows = match command {
                    MusicCommand::Play => state == MusicPlaybackState::Paused,
                    MusicCommand::Pause => state == MusicPlaybackState::Playing,
                    MusicCommand::Previous | MusicCommand::Next => true,
                    MusicCommand::OpenProvider => false,
                };
                if state_allows && self.allowed.supports(command) {
                    capabilities = capabilities.with(command);
                }
            }
        }
        self.base
            .with_playback_observation(state, observed_at, capabilities)
    }
}

/// Native HTTP effect without an access token; the platform supplies authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpotifyHttpRequest {
    /// Fully encoded fixed-origin endpoint.
    pub url: String,
    /// GET, PUT or POST, selected by the explicit action.
    pub method: String,
}

/// One Web API client's monotonic rate-limit deadline.
#[derive(Debug, Default)]
pub struct SpotifyHttpSession {
    retry_at_ms: Option<u64>,
}

impl SpotifyHttpSession {
    /// Admits metadata observation without a playback side effect.
    /// # Errors
    /// Returns a current rate limit or an unavailable endpoint.
    pub fn playback_request(
        &self,
        now_ms: u64,
    ) -> Result<SpotifyHttpRequest, SpotifyPlaybackError> {
        self.admit(now_ms)?;
        request("", "GET", "additional_types", "track,episode")
    }

    /// Targets only the explicitly supplied observed device and never transfers playback.
    /// # Errors
    /// Refuses an empty/oversized device, provider handoff or current rate limit.
    pub fn command_request(
        &self,
        command: MusicCommand,
        device_id: &str,
        now_ms: u64,
    ) -> Result<SpotifyHttpRequest, SpotifyPlaybackError> {
        self.admit(now_ms)?;
        MusicIdentifier::new(device_id).map_err(|_| SpotifyPlaybackError::Unavailable)?;
        let (path, method) = match command {
            MusicCommand::Play => ("/play", "PUT"),
            MusicCommand::Pause => ("/pause", "PUT"),
            MusicCommand::Previous => ("/previous", "POST"),
            MusicCommand::Next => ("/next", "POST"),
            MusicCommand::OpenProvider => return Err(SpotifyPlaybackError::Unavailable),
        };
        request(path, method, "device_id", device_id)
    }

    /// Classifies a bounded native response and records provider rate limits.
    /// # Errors
    /// Distinguishes access expiry, refusal, rate limiting and unavailable responses.
    pub fn receive_response(
        &mut self,
        status: u16,
        retry_after: Option<&str>,
        body_length: u64,
        now_ms: u64,
    ) -> Result<bool, SpotifyPlaybackError> {
        match status {
            200..=299 if body_length <= MAX_RESPONSE_BYTES => Ok(status != 204),
            401 => Err(SpotifyPlaybackError::Unauthorized),
            403 => Err(SpotifyPlaybackError::Forbidden),
            429 => {
                let seconds = retry_after
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .unwrap_or(30.0)
                    .clamp(1.0, 86_400.0);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "finite clamped duration is within u64"
                )]
                let delay = (seconds * 1000.0).ceil() as u64;
                let next_deadline = now_ms.saturating_add(delay);
                let deadline = self
                    .retry_at_ms
                    .map_or(next_deadline, |current| current.max(next_deadline));
                self.retry_at_ms = Some(deadline);
                Err(SpotifyPlaybackError::RateLimited {
                    retry_after_milliseconds: deadline.saturating_sub(now_ms),
                })
            }
            _ => Err(SpotifyPlaybackError::Unavailable),
        }
    }

    fn admit(&self, now_ms: u64) -> Result<(), SpotifyPlaybackError> {
        if let Some(deadline) = self.retry_at_ms.filter(|deadline| *deadline > now_ms) {
            return Err(SpotifyPlaybackError::RateLimited {
                retry_after_milliseconds: deadline - now_ms,
            });
        }
        Ok(())
    }
}

fn request(
    path: &str,
    method: &str,
    key: &str,
    value: &str,
) -> Result<SpotifyHttpRequest, SpotifyPlaybackError> {
    let mut url = Url::parse(&format!("https://api.spotify.com/v1/me/player{path}"))
        .map_err(|_| SpotifyPlaybackError::Unavailable)?;
    url.query_pairs_mut().append_pair(key, value);
    Ok(SpotifyHttpRequest {
        url: url.into(),
        method: method.to_owned(),
    })
}

#[derive(Deserialize)]
struct WirePlayback {
    device: Option<WireDevice>,
    is_playing: bool,
    progress_ms: Option<i64>,
    item: Option<WireItem>,
    actions: Option<WireActions>,
}
#[derive(Deserialize)]
struct WireDevice {
    id: Option<String>,
    is_restricted: Option<bool>,
}
#[derive(Deserialize)]
struct WireActions {
    disallows: Option<BTreeMap<String, bool>>,
}
#[derive(Deserialize)]
struct WireItem {
    uri: Option<String>,
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    artists: Option<Vec<WireArtist>>,
    show: Option<WireArtist>,
    album: Option<WireAlbum>,
    images: Option<Vec<WireImage>>,
    duration_ms: Option<i64>,
}
#[derive(Deserialize)]
struct WireArtist {
    name: String,
}
#[derive(Deserialize)]
struct WireAlbum {
    images: Option<Vec<WireImage>>,
}
#[derive(Deserialize)]
struct WireImage {
    url: String,
    width: Option<i64>,
}

impl WireItem {
    fn known_item(&self) -> Option<Result<MusicItem, crate::MusicValidationError>> {
        match self.kind.as_deref() {
            Some("track" | "episode") => self.uri.as_ref().map(|uri| {
                MusicItem::new(
                    uri.clone(),
                    self.name.clone(),
                    self.artists
                        .as_ref()
                        .map(|artists| {
                            artists
                                .iter()
                                .map(|artist| artist.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .or_else(|| self.show.as_ref().map(|show| show.name.clone())),
                )
            }),
            _ => None,
        }
    }
}

impl WirePlayback {
    fn artwork_url(&self) -> Option<String> {
        let item = self.item.as_ref()?;
        let images = item
            .images
            .as_ref()
            .or_else(|| item.album.as_ref().and_then(|album| album.images.as_ref()))?;
        let image = images
            .iter()
            .filter(|image| image.width.unwrap_or(0) >= 256)
            .min_by_key(|image| image.width.unwrap_or(0))
            .or_else(|| images.first())?;
        let url = Url::parse(&image.url).ok()?;
        (url.scheme() == "https" && url.username().is_empty() && url.password().is_none())
            .then(|| url.into())
    }
}
