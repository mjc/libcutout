//! Retained `SoundCloud` catalogue, authorization, and playback coordination.

use cutout_music::soundcloud_player::{self as core, PlayableTrack, PlayerEffect};
use std::sync::{Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
struct State {
    catalogue: Vec<PlayableTrack>,
    player: core::SoundCloudPlayer,
    authorization: Option<cutout_music::soundcloud_auth::ApiAuthorization>,
}

/// One retained `SoundCloud` player owner shared by native clients.
#[derive(Debug, Default)]
pub struct SoundCloudPlayerService {
    state: Mutex<State>,
}

impl SoundCloudPlayerService {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns the fixed broker endpoint; no provider secret is held by this service.
    #[must_use]
    pub fn token_endpoint() -> String {
        cutout_music::soundcloud_auth::TOKEN_ENDPOINT.to_owned()
    }

    /// Admits an expiring authorization response without persisting credentials.
    ///
    /// # Errors
    /// Returns the domain validation error for malformed token data.
    pub fn accept_authorization(&self, json: &[u8], now_ms: u64) -> Result<(), core::PlayerError> {
        let authorization = cutout_music::soundcloud_auth::ApiAuthorization::parse(json, now_ms)?;
        self.lock().authorization = Some(authorization);
        Ok(())
    }

    /// Returns the authorization header only while its lifetime remains valid.
    #[must_use]
    pub fn authorization(&self, now_ms: u64) -> Option<String> {
        self.lock()
            .authorization
            .as_ref()
            .and_then(|authorization| authorization.header(now_ms))
    }

    /// Invalidates authorization while leaving local playback intact.
    pub fn invalidate_authorization(&self) {
        self.lock().authorization = None;
    }

    /// Builds the official public track search URL.
    ///
    /// # Errors
    /// Returns the domain validation error for a blank or oversized query.
    pub fn search_endpoint(query: &str) -> Result<String, core::PlayerError> {
        core::search_endpoint(query)
    }

    /// Replaces the admitted catalogue without replacing the playback queue.
    ///
    /// # Errors
    /// Returns the domain validation error for malformed or oversized response data.
    pub fn accept_catalogue(&self, json: &[u8]) -> Result<Vec<PlayableTrack>, core::PlayerError> {
        let catalogue = core::parse_tracks(json)?;
        let tracks = catalogue.clone();
        self.lock().catalogue = catalogue;
        Ok(tracks)
    }

    /// Selects one track from the admitted catalogue.
    ///
    /// # Errors
    /// Returns the domain validation error when the track is not in the catalogue.
    pub fn select(&self, urn: &str) -> Result<PlayerEffect, core::PlayerError> {
        let mut state = self.lock();
        let State {
            catalogue, player, ..
        } = &mut *state;
        player.select(catalogue, urn)
    }

    /// Admits the full HLS API endpoint and rejects previews.
    ///
    /// # Errors
    /// Returns the domain validation error for unsupported stream data.
    pub fn stream_endpoint(json: &[u8]) -> Result<String, core::PlayerError> {
        core::stream_endpoint(json)
    }

    /// Admits a signed redirect only for the currently active fetch.
    ///
    /// # Errors
    /// Returns the domain validation error for stale fetches or unsupported origins.
    pub fn stream_resolved(&self, id: u64, url: &str) -> Result<PlayerEffect, core::PlayerError> {
        self.lock().player.stream_resolved(id, url)
    }

    /// Commits a current platform readiness callback.
    pub fn player_ready(&self, id: u64) -> PlayerEffect {
        self.lock().player.player_ready(id)
    }

    /// Handles completion; stale callbacks cannot advance a newer selection.
    ///
    /// # Errors
    /// Returns the domain error if a subsequent playback identity cannot be allocated.
    pub fn ended(&self, id: u64) -> Result<PlayerEffect, core::PlayerError> {
        self.lock().player.ended(id)
    }

    /// Commits a current platform failure callback.
    pub fn failed(&self, id: u64) -> PlayerEffect {
        self.lock().player.failed(id)
    }

    /// Ends the content session, clearing catalogue, queue, and stream callbacks.
    pub fn stop(&self) -> PlayerEffect {
        let mut state = self.lock();
        state.catalogue.clear();
        state.player.stop()
    }

    /// Applies an explicit local playback command.
    ///
    /// # Errors
    /// Returns the domain error if a track transition cannot be allocated.
    pub fn command(&self, command: core::PlayerCommand) -> Result<PlayerEffect, core::PlayerError> {
        self.lock().player.command(command)
    }

    /// Returns a presentation projection without admitting listening history.
    #[must_use]
    pub fn snapshot(&self) -> core::PlayerSnapshot {
        self.lock().player.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_clears_admitted_catalogue_and_rejects_old_selection() -> Result<(), core::PlayerError> {
        let player = SoundCloudPlayerService::default();
        let catalogue = br#"{"collection":[{"urn":"soundcloud:tracks:42","title":"Ride song","duration":120000,"permalink_url":"https://soundcloud.com/artist/song","user":{"username":"Uploader"},"access":"playable","streamable":true}]}"#;
        player.accept_catalogue(catalogue)?;
        assert!(matches!(
            player.select("soundcloud:tracks:42")?,
            PlayerEffect::Fetch { .. }
        ));
        player.command(core::PlayerCommand::Pause)?;
        player.stop();
        assert_eq!(player.snapshot().track, None);
        match player.select("soundcloud:tracks:42") {
            Err(core::PlayerError::UnknownTrack) => {}
            result => panic!("expected stale selection rejection, got {result:?}"),
        }
        player.accept_catalogue(catalogue)?;
        assert!(matches!(
            player.select("soundcloud:tracks:42")?,
            PlayerEffect::Fetch { .. }
        ));
        Ok(())
    }
}
