//! Capture-history projection through the existing database handle.

use crate::{
    CaptureWriterSlot, MobileCaptureArtifactIdDto, MobileCaptureOriginDto,
    MobilePevcapCaptureBuilder, MobilePevcapEncodingDto, MobilePevcapImportReceiptDto,
    MobileRideDatabaseError, MobileRideIdDto, MobileVerifiedStringDto,
    MobileWallClockUnixMillisDto, RideDatabaseHandle, map_ride_database_error,
    mobile_pevcap_receipt, mobile_query_limit, mobile_ride_id, persistence,
};
use std::sync::{Arc, PoisonError};

/// Rust-retained recording identity and device labels; not foreign completion authority.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileRecordedCaptureDto {
    pub artifact_id: MobileCaptureArtifactIdDto,
    pub origin: MobileCaptureOriginDto,
    pub advertised_name: Option<String>,
    pub platform_identifier: String,
    pub started_at: MobileWallClockUnixMillisDto,
    pub model: Option<MobileVerifiedStringDto>,
}

impl From<persistence::RecordedCapture> for MobileRecordedCaptureDto {
    fn from(recording: persistence::RecordedCapture) -> Self {
        Self {
            artifact_id: MobileCaptureArtifactIdDto {
                value: recording.artifact_id.to_string(),
            },
            origin: recording.origin.into(),
            advertised_name: recording.advertised_name,
            platform_identifier: recording.platform_identifier,
            started_at: MobileWallClockUnixMillisDto {
                milliseconds: recording.started_at.as_milliseconds(),
            },
            model: recording.model.map(|model| MobileVerifiedStringDto {
                value: model.value,
                verification: model.verification.into(),
            }),
        }
    }
}

/// Stable continuation for descending import-time/digest pagination.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePevcapCaptureCursorDto {
    pub imported_at_milliseconds: u64,
    pub artifact_digest: String,
}

/// A capture with complete original bytes retained in SQLite, independent of files or GPS.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileStoredPevcapCaptureDto {
    pub recording: Option<MobileRecordedCaptureDto>,
    pub artifact_digest: String,
    pub encoding: MobilePevcapEncodingDto,
    pub artifact_size: u64,
    /// Durable import time, not the recording's start time.
    pub imported_at_milliseconds: u64,
    pub record_count: u64,
    /// Locations admitted to the derived ride, not all raw observations.
    pub location_count: u64,
    pub ride_id: Option<MobileRideIdDto>,
}

/// One Rust-bounded page of SQLite-backed captures.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePevcapCapturePageDto {
    pub captures: Vec<MobileStoredPevcapCaptureDto>,
    pub next_cursor: Option<MobilePevcapCaptureCursorDto>,
}

#[uniffi::export]
impl RideDatabaseHandle {
    /// Retains the actual consumed-writer receipt, never a caller-supplied path or DTO.
    /// Call off native callback/UI threads; failures leave the saved file available.
    ///
    /// # Errors
    /// Returns `CaptureNotFinished` for ready, active, finalizing or failed writers,
    /// or the typed persistence error. Flushing is not sufficient admission.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI exports require owned object and optional string arguments"
    )]
    pub fn retain_finished_capture(
        &self,
        builder: Arc<MobilePevcapCaptureBuilder>,
        origin: MobileCaptureOriginDto,
        advertised_name: Option<String>,
        published_at: MobileWallClockUnixMillisDto,
    ) -> Result<MobilePevcapImportReceiptDto, MobileRideDatabaseError> {
        let artifact = {
            let slot = builder
                .writer
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match &*slot {
                CaptureWriterSlot::Complete(Ok(artifact)) => artifact.clone(),
                CaptureWriterSlot::Ready
                | CaptureWriterSlot::Recording(_)
                | CaptureWriterSlot::Finalizing
                | CaptureWriterSlot::Complete(Err(_)) => {
                    return Err(MobileRideDatabaseError::CaptureNotFinished);
                }
            }
        };
        self.inner
            .retain_finished_capture(
                &artifact,
                origin.into(),
                advertised_name.as_deref(),
                published_at.into_core(),
            )
            .map(mobile_pevcap_receipt)
            .map_err(map_ride_database_error)
    }

    /// Reads published capture metadata without opening source or managed artifact files.
    ///
    /// Like other database queries, call off native callback/UI threads. This does not import
    /// live files or migrate file-only receipts. Refresh the first page to see newer imports.
    ///
    /// # Errors
    /// Returns typed cursor, limit or storage errors; no unbounded query is admitted.
    pub fn list_pevcap_captures(
        &self,
        cursor: Option<MobilePevcapCaptureCursorDto>,
        limit: u32,
    ) -> Result<MobilePevcapCapturePageDto, MobileRideDatabaseError> {
        let limit = mobile_query_limit(limit)?;
        let cursor = cursor
            .map(|cursor| {
                persistence::PevcapCaptureCursor::new(
                    cursor.imported_at_milliseconds,
                    &cursor.artifact_digest,
                )
            })
            .transpose()
            .map_err(map_ride_database_error)?;
        self.inner
            .list_pevcap_captures(cursor, limit)
            .map(|page| MobilePevcapCapturePageDto {
                captures: page
                    .captures
                    .into_iter()
                    .map(|capture| MobileStoredPevcapCaptureDto {
                        recording: capture.recording.map(Into::into),
                        artifact_digest: capture.artifact_digest,
                        encoding: capture.encoding.into(),
                        artifact_size: capture.artifact_size,
                        imported_at_milliseconds: capture.imported_at_milliseconds,
                        record_count: capture.record_count,
                        location_count: capture.location_count,
                        ride_id: capture.ride_id.map(mobile_ride_id),
                    })
                    .collect(),
                next_cursor: page.next_cursor.map(|cursor| MobilePevcapCaptureCursorDto {
                    imported_at_milliseconds: cursor.imported_at_milliseconds(),
                    artifact_digest: cursor.artifact_digest(),
                }),
            })
            .map_err(map_ride_database_error)
    }
}
