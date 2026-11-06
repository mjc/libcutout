//! Shared Spotify metadata validation and native HTTP admission state.

use std::sync::{Mutex, PoisonError};

use cutout_core::MonotonicTimestamp;
use cutout_music::spotify::playback::{
    SpotifyHttpRequest, SpotifyHttpSession, SpotifyPlayback as ValidatedSpotifyPlayback,
    SpotifyPlaybackError,
};
use cutout_music::{MusicCommand, MusicPlaybackState, MusicSnapshot};

/// Validated Spotify metadata used by presentation and explicit command policy.
#[derive(Clone, Debug)]
pub struct SpotifyPlayback {
    inner: ValidatedSpotifyPlayback,
}

impl SpotifyPlayback {
    /// Parses bounded provider response bytes.
    ///
    /// # Errors
    /// Returns a typed error for invalid or unsupported provider responses.
    pub fn parse(body: &[u8]) -> Result<Self, SpotifyPlaybackError> {
        Ok(Self {
            inner: ValidatedSpotifyPlayback::parse(body)?,
        })
    }

    /// Returns the provider's current playback flag without issuing a command.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.inner.is_playing()
    }

    /// Returns the observed device, when present.
    #[must_use]
    pub fn device_id(&self) -> Option<&str> {
        self.inner.device_id()
    }

    /// Returns the observed item identifier, when present.
    #[must_use]
    pub fn item_identifier(&self) -> Option<&str> {
        self.inner.item_identifier()
    }

    /// Returns validated HTTPS artwork, when present.
    #[must_use]
    pub fn artwork_url(&self) -> Option<&str> {
        self.inner.artwork_url()
    }

    /// Removes the active device while retaining validated item metadata.
    #[must_use]
    pub fn without_active_device(&self) -> Self {
        Self {
            inner: self.inner.without_active_device(),
        }
    }

    /// Projects metadata with the current native playback state and timestamp.
    #[must_use]
    pub fn snapshot(
        &self,
        state: MusicPlaybackState,
        observed_at: MonotonicTimestamp,
    ) -> MusicSnapshot {
        self.inner.snapshot(state, observed_at)
    }
}

/// One native HTTP request without an access token.
pub type SpotifyRequest = SpotifyHttpRequest;

/// One thread-safe HTTP policy owner for request admission and retry deadlines.
#[derive(Debug, Default)]
pub struct SpotifyHttpPolicy {
    inner: Mutex<SpotifyHttpSession>,
}

impl SpotifyHttpPolicy {
    /// Creates an independent policy lifetime with no credentials.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Admits metadata observation without starting playback.
    ///
    /// # Errors
    /// Returns the current rate limit or an unavailable endpoint.
    pub fn playback_request(&self, now_ms: u64) -> Result<SpotifyRequest, SpotifyPlaybackError> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .playback_request(now_ms)
    }

    /// Admits an explicit command for the observed device only.
    ///
    /// # Errors
    /// Refuses an invalid device identity, app handoff, or current rate limit.
    pub fn command_request(
        &self,
        command: MusicCommand,
        device_id: &str,
        now_ms: u64,
    ) -> Result<SpotifyRequest, SpotifyPlaybackError> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .command_request(command, device_id, now_ms)
    }

    /// Classifies a native response and retains provider rate-limit deadlines.
    ///
    /// # Errors
    /// Distinguishes authorization, refusal, rate-limit, and invalid responses.
    pub fn receive_response(
        &self,
        status: u16,
        retry_after: Option<&str>,
        body_length: u64,
        now_ms: u64,
    ) -> Result<bool, SpotifyPlaybackError> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .receive_response(status, retry_after, body_length, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_retains_rate_limit_across_observation_and_command_requests() {
        let policy = SpotifyHttpPolicy::new();
        assert_eq!(
            policy.playback_request(0).expect("read request").method,
            "GET"
        );
        assert!(matches!(
            policy.receive_response(429, Some("45"), 0, 10),
            Err(SpotifyPlaybackError::RateLimited {
                retry_after_milliseconds: 45_000
            })
        ));
        assert!(matches!(
            policy.command_request(MusicCommand::Next, "phone", 20),
            Err(SpotifyPlaybackError::RateLimited {
                retry_after_milliseconds: 44_990
            })
        ));
        assert!(
            policy
                .command_request(MusicCommand::Next, "phone", 45_010)
                .is_ok()
        );
    }

    #[test]
    fn response_limits_and_explicit_command_semantics_remain_domain_owned() {
        let policy = SpotifyHttpPolicy::new();
        assert_eq!(
            policy.command_request(MusicCommand::OpenProvider, "phone", 0),
            Err(SpotifyPlaybackError::Unavailable)
        );
        assert_eq!(
            policy.receive_response(200, None, 1_048_577, 0),
            Err(SpotifyPlaybackError::Unavailable)
        );
    }
}
