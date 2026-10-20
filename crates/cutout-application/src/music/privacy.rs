//! Privacy-preserving music links and durable history deletion.

use libcutout_persistence::{RideDatabase, StorageError};

/// Admits only a content-specific Spotify link, never an arbitrary URL.
#[must_use]
pub fn spotify_content_url(identifier: &str) -> Option<String> {
    cutout_music::spotify::content_url(identifier)
}

/// Removes Spotify listening events while preserving rides and other providers.
///
/// # Errors
/// Returns the storage failure when the durable cleanup cannot commit.
pub fn delete_spotify_music_history(database: &RideDatabase) -> Result<(), StorageError> {
    database.delete_spotify_music_history()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_content_links_reject_arbitrary_urls() {
        assert!(spotify_content_url("spotify:track:4uLU6hMCjMI75M1A2tKUQC").is_some());
        assert!(spotify_content_url("https://example.invalid/track/abc").is_none());
    }
}
