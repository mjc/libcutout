use crate::{MobileRideMapCore, MobileRideMapCoreErrorDto, map_storage_core_error};
use std::sync::PoisonError;

/// Admits a content-specific Spotify link without accepting arbitrary URLs.
#[uniffi::export]
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary strings"
)]
pub fn spotify_content_url(identifier: String) -> Option<String> {
    cutout_application::music::privacy::spotify_content_url(&identifier)
}

#[uniffi::export]
impl MobileRideMapCore {
    /// Removes Spotify ride history while preserving rides and other providers' history.
    /// # Errors
    /// Returns a storage error if the durable cleanup cannot be completed.
    pub fn delete_spotify_music_history(&self) -> Result<(), MobileRideMapCoreErrorDto> {
        let database = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .database
            .clone()
            .ok_or_else(MobileRideMapCoreErrorDto::storage_unavailable)?;
        cutout_application::music::privacy::delete_spotify_music_history(&database.inner)
            .map_err(map_storage_core_error)
    }
}
