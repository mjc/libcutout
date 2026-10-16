//! Optional diagnostic recording preferences through the existing database owner.

use crate::{
    CutoutSessionStateHandle, MobileCaptureGenerationDto, MobileCaptureOriginDto,
    MobileRideDatabaseError, map_ride_database_error,
};

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Admits a native capture attempt only when its recording purpose has consent.
    /// Explicit manual Start supplies its own consent; automatic diagnostics require the saved choice.
    pub fn begin_consented_capture(
        &self,
        origin: MobileCaptureOriginDto,
    ) -> Option<MobileCaptureGenerationDto> {
        if origin == MobileCaptureOriginDto::Automatic
            && !self
                .automatic_diagnostic_recording_enabled()
                .unwrap_or(false)
        {
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
