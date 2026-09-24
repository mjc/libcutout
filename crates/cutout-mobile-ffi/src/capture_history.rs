//! Capture-history projection through the existing database handle.

use crate::{
    MobilePevcapEncodingDto, MobileRideDatabaseError, MobileRideIdDto, RideDatabaseHandle,
    map_ride_database_error, mobile_query_limit, mobile_ride_id, persistence,
};

/// Stable continuation for descending import-time/digest pagination.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobilePevcapCaptureCursorDto {
    pub imported_at_milliseconds: u64,
    pub artifact_digest: String,
}

/// A capture with complete original bytes retained in SQLite, independent of files or GPS.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileStoredPevcapCaptureDto {
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
