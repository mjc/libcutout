//! Shared Spotify Web API policy; native SDKs retain credentials and execute HTTP.

use std::sync::Arc;

use cutout_core::MonotonicTimestamp;
use cutout_music::spotify::playback::SpotifyPlaybackError;

use crate::{
    CoreMusicCommand, CoreMusicPlaybackState, MobileMusicCommandDto, MobileMusicPlaybackStateDto,
    MobileMusicSnapshotDto,
};
use cutout_application::music::spotify_playback as application;

/// Bounded provider errors; no raw response or credential is included.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileSpotifyPlaybackError {
    /// Invalid, oversized or inconsistent playback metadata.
    #[error("invalid Spotify playback response")]
    InvalidPayload,
    /// The native SDK must renew the access token.
    #[error("Spotify authorization expired")]
    Unauthorized,
    /// Provider permissions, account or device refuse playback.
    #[error("Spotify playback access denied")]
    Forbidden,
    /// The provider's current monotonic retry window.
    #[error("Spotify playback rate limited")]
    RateLimited {
        /// Remaining delay before another HTTP effect can be admitted.
        retry_after_milliseconds: u64,
    },
    /// No usable provider response or command.
    #[error("Spotify playback unavailable")]
    Unavailable,
}

impl From<SpotifyPlaybackError> for MobileSpotifyPlaybackError {
    fn from(error: SpotifyPlaybackError) -> Self {
        match error {
            SpotifyPlaybackError::InvalidPayload => Self::InvalidPayload,
            SpotifyPlaybackError::Unauthorized => Self::Unauthorized,
            SpotifyPlaybackError::Forbidden => Self::Forbidden,
            SpotifyPlaybackError::RateLimited {
                retry_after_milliseconds,
            } => Self::RateLimited {
                retry_after_milliseconds,
            },
            SpotifyPlaybackError::Unavailable => Self::Unavailable,
        }
    }
}

/// Immutable, validated Spotify metadata shared by both native consumers.
#[derive(Debug, uniffi::Object)]
pub struct MobileSpotifyPlayback {
    inner: application::SpotifyPlayback,
}

#[uniffi::export]
impl MobileSpotifyPlayback {
    /// Parses native response bytes without accepting audio or unbounded data.
    /// # Errors
    /// Returns a typed invalid-payload error for rejected input.
    #[uniffi::constructor]
    #[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary bytes")]
    pub fn parse(body: Vec<u8>) -> Result<Arc<Self>, MobileSpotifyPlaybackError> {
        Ok(Arc::new(Self {
            inner: application::SpotifyPlayback::parse(&body)?,
        }))
    }

    /// Provider playback flag, without issuing a command.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.inner.is_playing()
    }

    /// Device from the current response, never a playback transfer destination.
    #[must_use]
    pub fn device_id(&self) -> Option<String> {
        self.inner.device_id().map(str::to_owned)
    }

    /// Known item identity used by native artwork caching.
    #[must_use]
    pub fn item_identifier(&self) -> Option<String> {
        self.inner.item_identifier().map(str::to_owned)
    }

    /// Validated HTTPS artwork URL without credentials.
    #[must_use]
    pub fn artwork_url(&self) -> Option<String> {
        self.inner.artwork_url().map(str::to_owned)
    }

    /// Retains metadata while removing the active playback device.
    #[must_use]
    pub fn without_active_device(&self) -> Arc<Self> {
        Arc::new(Self {
            inner: self.inner.without_active_device(),
        })
    }

    /// Applies shared lifecycle freshness and capability rules to the observation.
    #[must_use]
    pub fn snapshot(
        &self,
        state: MobileMusicPlaybackStateDto,
        observed_at_ms: u64,
    ) -> MobileMusicSnapshotDto {
        (&self.inner.snapshot(
            CoreMusicPlaybackState::from(state),
            MonotonicTimestamp::new(observed_at_ms),
        ))
            .into()
    }
}

/// Fixed-origin HTTP effect without credentials; the native SDK supplies its token.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MobileSpotifyHttpRequest {
    /// Fully encoded endpoint URL.
    pub url: String,
    /// GET, PUT or POST.
    pub method: String,
}

impl From<application::SpotifyRequest> for MobileSpotifyHttpRequest {
    fn from(request: application::SpotifyRequest) -> Self {
        Self {
            url: request.url,
            method: request.method,
        }
    }
}

/// Rust-owned request and retry policy for one native Web API client.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileSpotifyHttpSession {
    inner: application::SpotifyHttpPolicy,
}

#[uniffi::export]
impl MobileSpotifyHttpSession {
    /// Starts an independent HTTP policy lifetime with no credentials.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: application::SpotifyHttpPolicy::new(),
        })
    }

    /// Admits metadata observation without starting playback.
    /// # Errors
    /// Returns the remaining rate limit or an unavailable effect.
    pub fn playback_request(
        &self,
        now_ms: u64,
    ) -> Result<MobileSpotifyHttpRequest, MobileSpotifyPlaybackError> {
        Ok(self.inner.playback_request(now_ms)?.into())
    }

    /// Admits an explicit command for the observed device only.
    /// # Errors
    /// Refuses invalid device identity, handoff or a current rate limit.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn command_request(
        &self,
        command: MobileMusicCommandDto,
        device_id: String,
        now_ms: u64,
    ) -> Result<MobileSpotifyHttpRequest, MobileSpotifyPlaybackError> {
        Ok(self
            .inner
            .command_request(CoreMusicCommand::from(command), &device_id, now_ms)?
            .into())
    }

    /// Classifies a native response and records rate-limit deadlines.
    /// # Errors
    /// Distinguishes authorization, refusal, rate limit, oversized and unavailable responses.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns boundary strings"
    )]
    pub fn receive_response(
        &self,
        status: u16,
        retry_after: Option<String>,
        body_length: u64,
        now_ms: u64,
    ) -> Result<bool, MobileSpotifyPlaybackError> {
        Ok(self
            .inner
            .receive_response(status, retry_after.as_deref(), body_length, now_ms)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_consumers_receive_the_same_validated_snapshot_and_inactive_copy() {
        let playback = MobileSpotifyPlayback::parse(br#"{"device":{"id":"phone"},"is_playing":true,"item":{"type":"track","uri":"spotify:track:abc","name":"Song"}}"#.to_vec()).expect("valid response");
        assert!(playback.is_playing());
        assert_eq!(playback.device_id().as_deref(), Some("phone"));
        let snapshot = playback.snapshot(MobileMusicPlaybackStateDto::Playing, 10);
        assert!(snapshot.capabilities.pause);
        assert_eq!(
            snapshot.item.expect("known track").title.as_deref(),
            Some("Song")
        );
        let inactive = playback.without_active_device();
        assert!(inactive.device_id().is_none());
        assert!(
            !inactive
                .snapshot(MobileMusicPlaybackStateDto::Stopped, 20)
                .capabilities
                .next
        );
        assert_eq!(
            inactive.item_identifier().as_deref(),
            Some("spotify:track:abc")
        );
    }

    #[test]
    fn native_http_handle_retains_retry_deadline_across_methods() {
        let session = MobileSpotifyHttpSession::new();
        assert_eq!(session.playback_request(0).expect("read").method, "GET");
        assert!(matches!(
            session.receive_response(429, Some("45".into()), 0, 10),
            Err(MobileSpotifyPlaybackError::RateLimited {
                retry_after_milliseconds: 45_000
            })
        ));
        assert!(matches!(
            session.command_request(MobileMusicCommandDto::Next, "phone".into(), 20),
            Err(MobileSpotifyPlaybackError::RateLimited {
                retry_after_milliseconds: 44_990
            })
        ));
        assert!(
            session
                .command_request(MobileMusicCommandDto::Next, "phone".into(), 45_010)
                .is_ok()
        );
    }
}
