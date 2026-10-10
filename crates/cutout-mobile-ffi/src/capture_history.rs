//! Capture-history projection through the existing database handle.

use crate::{
    CaptureJsonlExport, CaptureWriterFinish, CaptureWriterSlot, MobileCaptureArtifactIdDto,
    MobileCaptureOriginDto, MobilePevcapCaptureBuilder, MobilePevcapEncodingDto,
    MobilePevcapImportReceiptDto, MobileRideDatabaseError, MobileRideIdDto,
    MobileVerifiedStringDto, MobileWallClockUnixMillisDto, RideDatabaseHandle,
    map_ride_database_error, mobile_pevcap_receipt, mobile_query_limit, mobile_ride_id,
    persistence,
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
                CaptureWriterSlot::Complete(Ok(CaptureWriterFinish::FileSaved(artifact))) => {
                    (**artifact).clone()
                }
                CaptureWriterSlot::Complete(Ok(CaptureWriterFinish::DatabaseFinished {
                    jsonl_export: CaptureJsonlExport::Available(artifact),
                    ..
                })) => (**artifact).clone(),
                CaptureWriterSlot::Ready
                | CaptureWriterSlot::Recording(_)
                | CaptureWriterSlot::Finalizing
                | CaptureWriterSlot::Complete(
                    Ok(CaptureWriterFinish::DatabaseFinished { .. }) | Err(_),
                ) => {
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

/// Session-row history continuation, independent of optional file exports.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileLiveCaptureHistoryCursorDto {
    /// Recording start wall clock.
    pub started_at_milliseconds: u64,
    /// Canonical database capture identity.
    pub live_capture_id: String,
}

/// Capture retained in the canonical database, including interrupted captures.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileLiveCaptureHistoryEntryDto {
    /// Canonical database capture identity.
    pub live_capture_id: String,
    /// Whether recovery found an unfinished session.
    pub interrupted: bool,
    /// Durable completeness evidence.
    pub integrity: crate::MobileCaptureIntegrityDto,
    /// Consumed-writer provenance, if publication succeeded.
    pub recording: Option<MobileRecordedCaptureDto>,
    /// Peripheral identity retained in the header.
    pub platform_identifier: String,
    /// Recording start wall clock.
    pub started_at_milliseconds: u64,
    /// Terminal wall clock; absent after interruption.
    pub finished_at_milliseconds: Option<u64>,
    /// Exact retained raw event count.
    pub event_count: u64,
    /// Uncompressed retained bytes excluding line delimiters.
    pub stored_bytes: u64,
}

/// One bounded page of inactive database captures.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileLiveCaptureHistoryPageDto {
    /// Bounded inactive-capture rows.
    pub captures: Vec<MobileLiveCaptureHistoryEntryDto>,
    /// Continuation when more sessions exist.
    pub next_cursor: Option<MobileLiveCaptureHistoryCursorDto>,
}

/// Result of an explicit export; this is not capture-completion authority.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileLiveCaptureExportDto {
    /// Synced export file location.
    pub path: String,
    /// SHA-256 of exact exported bytes.
    pub content_digest: String,
}

#[uniffi::export]
impl RideDatabaseHandle {
    /// Queries only bounded session metadata, without reading captured event payloads.
    ///
    /// # Errors
    /// Returns invalid cursor, limit or storage errors.
    pub fn list_live_capture_history(
        &self,
        cursor: Option<MobileLiveCaptureHistoryCursorDto>,
        limit: u32,
    ) -> Result<MobileLiveCaptureHistoryPageDto, MobileRideDatabaseError> {
        let cursor = cursor
            .map(|cursor| {
                Ok::<_, MobileRideDatabaseError>(persistence::LiveCaptureHistoryCursor {
                    started_at_ms: cursor.started_at_milliseconds,
                    id: persistence::LiveCaptureId::parse(&cursor.live_capture_id)
                        .map_err(map_ride_database_error)?,
                })
            })
            .transpose()?;
        let page = self
            .inner
            .list_live_capture_history(cursor, mobile_query_limit(limit)?)
            .map_err(map_ride_database_error)?;
        Ok(MobileLiveCaptureHistoryPageDto {
            captures: page
                .captures
                .into_iter()
                .map(|capture| MobileLiveCaptureHistoryEntryDto {
                    live_capture_id: capture.id.to_string(),
                    interrupted: capture.state == persistence::LiveCaptureState::Interrupted,
                    integrity: capture.integrity.into(),
                    recording: capture.recording.map(Into::into),
                    platform_identifier: capture.platform_identifier,
                    started_at_milliseconds: capture.started_at_ms,
                    finished_at_milliseconds: capture.finished_at_ms,
                    event_count: capture.event_count,
                    stored_bytes: capture.stored_bytes,
                })
                .collect(),
            next_cursor: page
                .next_cursor
                .map(|cursor| MobileLiveCaptureHistoryCursorDto {
                    started_at_milliseconds: cursor.started_at_ms,
                    live_capture_id: cursor.id.to_string(),
                }),
        })
    }

    /// Exports exact retained JSONL on explicit user demand, off native callback/UI threads.
    /// An existing output is never overwritten; failed partial files are removed.
    ///
    /// # Errors
    /// Returns malformed identity, active-capture, output or storage failure.
    pub fn export_live_capture(
        &self,
        live_capture_id: &str,
        path: String,
    ) -> Result<MobileLiveCaptureExportDto, MobileRideDatabaseError> {
        let id =
            persistence::LiveCaptureId::parse(live_capture_id).map_err(map_ride_database_error)?;
        let content_digest = self
            .inner
            .export_live_capture(id, std::path::Path::new(&path))
            .map_err(|_| MobileRideDatabaseError::StorageFailure)?;
        Ok(MobileLiveCaptureExportDto {
            path,
            content_digest,
        })
    }
}
