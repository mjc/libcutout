//! Finished-writer admission and recording provenance in the existing capture store.

use super::{Command, PevcapImportReceipt, RideDatabase, StorageError, pevcap_import_receipt};
use crate::{CaptureArtifactId, SavedCaptureArtifact};
use cutout_core::{
    CaptureOrigin, PevcapEncoding, VerificationStatus, VerifiedValue, WallClockUnixTimestamp,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

/// Provenance of a retained recording, separate from its content-addressed bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedCapture {
    /// Identity issued by the consumed writer, not a file path or content hash.
    pub artifact_id: CaptureArtifactId,
    /// Why the recording was started.
    pub origin: CaptureOrigin,
    /// Advertisement label, retained separately from the detected model.
    pub advertised_name: Option<String>,
    /// Platform peripheral identity recorded in the capture header.
    pub platform_identifier: String,
    /// Recording start time, not database publication time.
    pub started_at: WallClockUnixTimestamp,
    /// Detected model with its original verification classification.
    pub model: Option<VerifiedValue<String>>,
}

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS pevcap_recordings (
    artifact_id TEXT PRIMARY KEY NOT NULL,
    artifact_digest TEXT NOT NULL UNIQUE REFERENCES pevcap_imports(artifact_digest),
    origin TEXT NOT NULL CHECK (origin IN ('manual', 'automatic')),
    advertised_name TEXT CHECK (length(CAST(advertised_name AS BLOB)) <= 512),
    platform_identifier TEXT NOT NULL CHECK (length(CAST(platform_identifier AS BLOB)) <= 512),
    started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
    model TEXT CHECK (length(CAST(model AS BLOB)) <= 512),
    model_verification TEXT CHECK (model_verification IN
      ('unverified', 'inferred', 'source', 'hardware', 'source_and_hardware')),
    CHECK ((model IS NULL) = (model_verification IS NULL))
);";

impl RideDatabase {
    /// Retains a finished writer's original bytes without deriving a GPS ride.
    ///
    /// Call off transport/UI callback threads. The source file is never removed.
    /// An identical artifact-ID retry uses the durable receipt even if both file
    /// copies are gone. Reusing an identity or digest with conflicting recording
    /// provenance is rejected, never silently overwritten.
    ///
    /// # Errors
    /// Returns storage, source-admission or identity-conflict errors. Failure leaves
    /// the writer's saved artifact available for export or retry.
    ///
    /// An active writer is not a saved-artifact receipt:
    ///
    /// ```compile_fail
    /// use cutout_core::{CaptureOrigin, WallClockUnixTimestamp};
    /// use libcutout_persistence::{CaptureWriter, RideDatabase};
    /// fn publish_active(database: &RideDatabase, writer: &CaptureWriter) {
    ///     database.retain_finished_capture(
    ///         writer, CaptureOrigin::Manual, None, WallClockUnixTimestamp::new(1234),
    ///     ).unwrap();
    /// }
    /// ```
    pub fn retain_finished_capture(
        &self,
        artifact: &SavedCaptureArtifact,
        origin: CaptureOrigin,
        advertised_name: Option<&str>,
        published_at: WallClockUnixTimestamp,
    ) -> Result<PevcapImportReceipt, StorageError> {
        if let Some(receipt) = self.request(|reply| Command::RecordedCaptureLookup {
            id: artifact.id(),
            reply,
        })? {
            let recording = receipt
                .recording
                .as_ref()
                .ok_or(StorageError::CaptureIdentityConflict)?;
            if recording.origin != origin || recording.advertised_name.as_deref() != advertised_name
            {
                return Err(StorageError::CaptureIdentityConflict);
            }
            return Ok(receipt);
        }
        let preview = super::preflight_pevcap(artifact.path(), PevcapEncoding::Jsonl)?;
        let reader = super::pevcap_reader(artifact.path(), PevcapEncoding::Jsonl)?;
        let header = reader.header();
        let recording = RecordedCapture {
            artifact_id: artifact.id(),
            origin,
            advertised_name: advertised_name.map(str::to_owned),
            platform_identifier: header.platform_id.clone(),
            started_at: header.wall_clock_start_unix_ms,
            model: header
                .resolved_identity
                .as_ref()
                .and_then(|identity| identity.model.clone()),
        };
        for value in [
            recording.advertised_name.as_deref(),
            Some(recording.platform_identifier.as_str()),
            recording.model.as_ref().map(|model| model.value.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            super::check_pevcap_limit(
                "recording label bytes",
                512,
                u64::try_from(value.len()).unwrap_or(u64::MAX),
            )?;
        }
        self.confirm_pevcap_publication(
            &preview,
            published_at.as_milliseconds(),
            &super::CapturePublication::Recording(recording),
        )
    }
}

pub(super) fn insert(
    connection: &Connection,
    digest: &str,
    recording: &RecordedCapture,
) -> Result<(), StorageError> {
    let origin = match recording.origin {
        CaptureOrigin::Automatic => "automatic",
        CaptureOrigin::Manual => "manual",
    };
    connection.execute(
        "INSERT INTO pevcap_recordings (artifact_id, artifact_digest, origin, advertised_name,
         platform_identifier, started_at_ms, model, model_verification)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            recording.artifact_id.to_string(),
            digest,
            origin,
            recording.advertised_name,
            recording.platform_identifier,
            recording.started_at.as_milliseconds(),
            recording.model.as_ref().map(|model| model.value.as_str()),
            recording
                .model
                .as_ref()
                .map(|model| verification_to_db(model.verification))
        ],
    )?;
    Ok(())
}

pub(super) fn lookup(
    connection: &Connection,
    digest: &str,
) -> Result<Option<RecordedCapture>, StorageError> {
    connection
        .query_row(
            "SELECT artifact_id, origin, advertised_name, platform_identifier, started_at_ms,
         model, model_verification FROM pevcap_recordings WHERE artifact_digest = ?1",
            [digest],
            |row| Ok(read_recording(row)),
        )
        .optional()?
        .transpose()
}

pub(super) fn receipt_for_id(
    connection: &Connection,
    id: CaptureArtifactId,
) -> Result<Option<PevcapImportReceipt>, StorageError> {
    let digest: Option<String> = connection
        .query_row(
            "SELECT r.artifact_digest FROM pevcap_recordings r
         JOIN pevcap_captures c ON c.receipt_digest = r.artifact_digest WHERE r.artifact_id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    match digest {
        Some(digest) => pevcap_import_receipt(connection, &digest, true),
        None => Ok(None),
    }
}

fn read_recording(row: &Row<'_>) -> Result<RecordedCapture, StorageError> {
    let id: String = row.get(0)?;
    let artifact_id = Uuid::parse_str(&id)
        .map(CaptureArtifactId::from_uuid)
        .map_err(|_| invalid("recording identity", id))?;
    let origin: String = row.get(1)?;
    let origin = match origin.as_str() {
        "manual" => CaptureOrigin::Manual,
        "automatic" => CaptureOrigin::Automatic,
        _ => return Err(invalid("recording origin", origin)),
    };
    let model = match (
        row.get::<_, Option<String>>(5)?,
        row.get::<_, Option<String>>(6)?,
    ) {
        (None, None) => None,
        (Some(value), Some(verification)) => Some(VerifiedValue {
            value,
            verification: verification_from_db(&verification)?,
        }),
        _ => {
            return Err(invalid(
                "recording model",
                "incomplete verification".to_owned(),
            ));
        }
    };
    Ok(RecordedCapture {
        artifact_id,
        origin,
        advertised_name: row.get(2)?,
        platform_identifier: row.get(3)?,
        started_at: WallClockUnixTimestamp::new(row.get(4)?),
        model,
    })
}

const fn verification_to_db(value: VerificationStatus) -> &'static str {
    match value {
        VerificationStatus::Unverified => "unverified",
        VerificationStatus::Inferred => "inferred",
        VerificationStatus::SourceVerified => "source",
        VerificationStatus::HardwareVerified => "hardware",
        VerificationStatus::SourceAndHardwareVerified => "source_and_hardware",
    }
}

fn verification_from_db(value: &str) -> Result<VerificationStatus, StorageError> {
    match value {
        "unverified" => Ok(VerificationStatus::Unverified),
        "inferred" => Ok(VerificationStatus::Inferred),
        "source" => Ok(VerificationStatus::SourceVerified),
        "hardware" => Ok(VerificationStatus::HardwareVerified),
        "source_and_hardware" => Ok(VerificationStatus::SourceAndHardwareVerified),
        _ => Err(invalid("recording model verification", value.to_owned())),
    }
}

fn invalid(field: &'static str, value: String) -> StorageError {
    StorageError::InvalidStoredValue { field, value }
}
