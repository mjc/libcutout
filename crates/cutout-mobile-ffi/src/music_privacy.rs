use crate::{
    CaptureWriterSlot, MobilePevcapCaptureBuilder, MobileRideMapCore, MobileRideMapCoreErrorDto,
    map_storage_core_error,
};
use std::{path::PathBuf, sync::PoisonError};

/// Admits a content-specific Spotify link without accepting arbitrary URLs.
#[uniffi::export]
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary strings"
)]
pub fn spotify_content_url(identifier: String) -> Option<String> {
    cutout_music::spotify::content_url(&identifier)
}

#[uniffi::export]
impl MobileRideMapCore {
    /// Removes Spotify ride history and obsolete music metadata in owned Bluetooth captures.
    /// Rides and other providers' listening history remain. Native code retires provider work
    /// and awaits owned writer publication before requesting this worker-ordered cleanup.
    ///
    /// # Errors
    /// Returns a storage error if the durable cleanup cannot be completed.
    pub fn delete_spotify_music_history(
        &self,
        capture_directories: Vec<String>,
    ) -> Result<(), MobileRideMapCoreErrorDto> {
        let database = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .database
            .clone()
            .ok_or_else(MobileRideMapCoreErrorDto::storage_unavailable)?;
        let directories: Vec<PathBuf> =
            capture_directories.into_iter().map(PathBuf::from).collect();
        database
            .inner
            .purge_capture_music_metadata(&directories)
            .map_err(map_storage_core_error)?;
        database
            .inner
            .delete_spotify_music_history()
            .map_err(map_storage_core_error)
    }
}

#[uniffi::export]
impl MobilePevcapCaptureBuilder {
    /// Waits for prior finalization/publication and flushes a live database writer.
    /// File-only live writers must finish before their owned file can be rewritten.
    pub fn prepare_music_metadata_cleanup(&self) -> bool {
        let _finish = self
            .writer_finish_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        match &*writer {
            CaptureWriterSlot::Ready | CaptureWriterSlot::Complete(Ok(_)) => true,
            CaptureWriterSlot::Recording(writer) => {
                if self
                    .database
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .is_none()
                {
                    return false;
                }
                matches!(
                    writer.flush_outcome(),
                    crate::persistence::CaptureFlushOutcome::Flushed
                )
            }
            CaptureWriterSlot::Finalizing | CaptureWriterSlot::Complete(Err(_)) => false,
        }
    }
}

/// Produces a music-free share copy of a completed app-owned local capture.
///
/// # Errors
/// Returns a storage error if the source cannot be safely transformed or synced.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary paths")]
pub fn export_capture_file_without_music(
    source_path: String,
    path: String,
) -> Result<crate::MobileLiveCaptureExportDto, crate::MobileRideDatabaseError> {
    let content_digest = crate::persistence::export_capture_file_without_music(
        std::path::Path::new(&source_path),
        std::path::Path::new(&path),
    )
    .map_err(|_| crate::MobileRideDatabaseError::StorageFailure)?;
    Ok(crate::MobileLiveCaptureExportDto {
        path,
        content_digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_music_cleanup_requires_file_writer_completion() {
        let path = std::env::temp_dir().join(format!(
            "cutout-music-cleanup-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        let builder = MobilePevcapCaptureBuilder::new(
            crate::MobileWallClockUnixMillisDto {
                milliseconds: 1_700_000_000_000,
            },
            "test-wheel".to_owned(),
            None,
        );
        assert!(builder.start_writer(path.to_string_lossy().into_owned()));
        assert!(
            !builder.prepare_music_metadata_cleanup(),
            "cannot rewrite an active buffered file"
        );
        assert!(builder.finish_writer());
        assert!(builder.prepare_music_metadata_cleanup());
        std::fs::remove_file(path).expect("remove fixture");
    }
}
