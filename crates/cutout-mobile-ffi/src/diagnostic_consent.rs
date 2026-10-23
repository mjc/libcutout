//! Optional diagnostic recording preferences through the existing database owner.

use crate::{
    CutoutSessionStateHandle, MobileCaptureGenerationDto, MobileCaptureOriginDto,
    MobileRideDatabaseError, map_ride_database_error,
};

/// Only an explicit user action may admit a diagnostic capture.
pub(crate) const fn admits_capture_origin(origin: MobileCaptureOriginDto) -> bool {
    match origin {
        MobileCaptureOriginDto::Manual => true,
        MobileCaptureOriginDto::Automatic => false,
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Admits capture attempts initiated by an explicit manual user action.
    pub fn begin_consented_capture(
        &self,
        origin: MobileCaptureOriginDto,
    ) -> Option<MobileCaptureGenerationDto> {
        if !admits_capture_origin(origin) {
            return None;
        }
        self.begin_capture(origin)
    }

    /// Reads durable consent. Missing storage cannot authorize automatic recording.
    /// Call off native UI and callback threads.
    ///
    /// # Errors
    /// Returns a storage failure if the preference cannot be read.
    pub fn automatic_diagnostic_recording_enabled(&self) -> Result<bool, MobileRideDatabaseError> {
        self.diagnostic_database
            .as_ref()
            .map_or(Ok(false), |database| {
                database
                    .automatic_diagnostic_recording_enabled()
                    .map_err(map_ride_database_error)
            })
    }

    /// Commits explicit consent and identifies only the automatic writer to retire on withdrawal.
    /// This never disconnects Bluetooth, stops a ride or cancels a manual capture.
    ///
    /// # Errors
    /// Returns a storage failure without acknowledging an uncommitted choice.
    pub fn set_automatic_diagnostic_recording_enabled(
        &self,
        enabled: bool,
    ) -> Result<Option<MobileCaptureGenerationDto>, MobileRideDatabaseError> {
        let database = self
            .diagnostic_database
            .as_ref()
            .ok_or(MobileRideDatabaseError::StorageFailure)?;
        database
            .save_automatic_diagnostic_recording_enabled(enabled)
            .map_err(map_ride_database_error)?;
        Ok(if enabled {
            None
        } else {
            self.lock_inner()
                .session_state()
                .capture
                .automatic_capture_to_finish()
                .map(Into::into)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn legacy_automatic_consent_does_not_authorize_capture_but_manual_does() {
        let directory =
            std::env::temp_dir().join(format!("cutout-capture-policy-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let database_path = directory.join("rides.sqlite");
        let database =
            crate::open_ride_database(database_path.to_string_lossy().into_owned()).unwrap();
        database
            .inner
            .save_automatic_diagnostic_recording_enabled(true)
            .unwrap();
        let automatic = CutoutSessionStateHandle::with_database(std::sync::Arc::clone(&database));
        assert!(
            automatic
                .begin_consented_capture(MobileCaptureOriginDto::Automatic)
                .is_none()
        );
        assert!(
            automatic
                .begin_consented_capture(MobileCaptureOriginDto::Manual)
                .is_some()
        );
        database.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
}
