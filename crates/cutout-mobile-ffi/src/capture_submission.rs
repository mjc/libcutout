//! Explicit private diagnostic Send through Rust's retained snapshot owner.

mod bulk;
mod http;
pub use bulk::*;

use crate::{RideDatabaseHandle, persistence};
use cutout_core::{CaptureSubmissionIntegrity, CaptureSubmissionReceipt};
use http::HttpCaptureTransport;
use std::sync::{Arc, atomic::AtomicBool};

/// A retained capture identity, never a foreign path or eligibility decision.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionSourceDto {
    /// Canonical SQLite capture UUID from durable writer completion or history.
    LiveCapture { id: String },
    /// Consumed writer's retained artifact UUID, independent of its file path.
    RecordedArtifact { id: String },
    /// Canonical SHA-256 of an imported PEVCAP capture without writer identity.
    PevcapDigest { sha256: String },
}

impl TryFrom<MobileCaptureSubmissionSourceDto> for persistence::CaptureSubmissionSource {
    type Error = MobileCaptureSubmissionError;

    fn try_from(source: MobileCaptureSubmissionSourceDto) -> Result<Self, Self::Error> {
        match source {
            MobileCaptureSubmissionSourceDto::LiveCapture { id } => {
                persistence::LiveCaptureId::parse(&id)
                    .map(Self::LiveCapture)
                    .map_err(|_| MobileCaptureSubmissionError::InvalidSource)
            }
            MobileCaptureSubmissionSourceDto::RecordedArtifact { id } => {
                persistence::CaptureArtifactId::parse(&id)
                    .map(Self::RecordedArtifact)
                    .map_err(|_| MobileCaptureSubmissionError::InvalidSource)
            }
            MobileCaptureSubmissionSourceDto::PevcapDigest { sha256 } => {
                persistence::PevcapDigest::parse(&sha256)
                    .map(Self::PevcapDigest)
                    .map_err(|_| MobileCaptureSubmissionError::InvalidSource)
            }
        }
    }
}

/// Native presentation of the persisted operation; it does not authorize a transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionPhaseDto {
    /// Local immutable snapshot, with no Send effect.
    Prepared,
    /// A user-authorized transfer has not received a durable receipt yet.
    Sending,
    /// Rust retained a classified transfer failure.
    Failed,
    /// The service rejected the immutable snapshot; this operation cannot be retried.
    Rejected,
    /// A matching durable service receipt has been committed locally.
    Sent,
    /// Remote cancellation has not been confirmed.
    Cancelling,
    /// Cancellation has been confirmed, preserving the original recording.
    Cancelled,
    /// Remote deletion has not been confirmed.
    Deleting,
    /// Deletion has been confirmed, preserving the original recording.
    Deleted,
}

/// Availability of the private payload in the current local database.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionLocalPayloadStateDto {
    /// Bytes are retained locally.
    Retained,
    /// Receipt-confirmed cleanup is still in progress.
    RemovalPending,
    /// Payload bytes were removed; receipt details remain available.
    Removed,
}

impl From<persistence::CaptureSubmissionLocalPayloadState>
    for MobileCaptureSubmissionLocalPayloadStateDto
{
    fn from(state: persistence::CaptureSubmissionLocalPayloadState) -> Self {
        match state {
            persistence::CaptureSubmissionLocalPayloadState::Retained => Self::Retained,
            persistence::CaptureSubmissionLocalPayloadState::RemovalPending => Self::RemovalPending,
            persistence::CaptureSubmissionLocalPayloadState::Removed => Self::Removed,
        }
    }
}

impl From<persistence::CaptureSubmissionPhase> for MobileCaptureSubmissionPhaseDto {
    fn from(phase: persistence::CaptureSubmissionPhase) -> Self {
        use persistence::CaptureSubmissionPhase as Phase;
        match phase {
            Phase::Prepared => Self::Prepared,
            Phase::Sending => Self::Sending,
            Phase::Retryable => Self::Failed,
            Phase::Rejected => Self::Rejected,
            Phase::Sent => Self::Sent,
            Phase::CancellationPending => Self::Cancelling,
            Phase::Cancelled => Self::Cancelled,
            Phase::DeletionPending => Self::Deleting,
            Phase::Deleted => Self::Deleted,
        }
    }
}

/// Categories found in the actual validated snapshot for the Send disclosure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent payload categories can be present in any combination"
)]
pub struct MobileCaptureSubmissionCategoriesDto {
    /// Known device/platform identifiers are included; raw BLE can contain others.
    pub device_identifiers: bool,
    /// Location observations are included.
    pub location: bool,
    /// User descriptions or notes are included.
    pub notes: bool,
    /// Accepted annotations or physical-action label boundaries are included.
    pub annotations: bool,
    /// User-entered settings values and notes are included.
    pub settings_observations: bool,
}

impl From<cutout_core::CaptureSubmissionCategories> for MobileCaptureSubmissionCategoriesDto {
    fn from(categories: cutout_core::CaptureSubmissionCategories) -> Self {
        Self {
            device_identifiers: categories.device_identifiers,
            location: categories.location,
            notes: categories.notes,
            annotations: categories.annotations,
            settings_observations: categories.settings_observations,
        }
    }
}

/// Saved user context for one recording, separate from raw Bluetooth bytes.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureEvidenceDto {
    /// Exact user-entered note.
    pub note: String,
    /// Explicitly selected JPEG screenshots in display order.
    pub screenshots: Vec<Vec<u8>>,
    /// Portable UTF-8 note budget.
    pub note_limit_bytes: u32,
    /// Portable image count limit.
    pub screenshot_limit: u32,
    /// Portable encoded JPEG byte limit.
    pub screenshot_limit_bytes: u64,
}

impl From<cutout_core::CaptureAttachments> for MobileCaptureEvidenceDto {
    fn from(evidence: cutout_core::CaptureAttachments) -> Self {
        Self {
            note: evidence.note,
            screenshots: evidence.screenshots.into_iter().map(|s| s.jpeg).collect(),
            note_limit_bytes: u32::try_from(cutout_core::MAX_CAPTURE_NOTE_BYTES)
                .unwrap_or(u32::MAX),
            screenshot_limit: u32::try_from(cutout_core::MAX_CAPTURE_SCREENSHOTS)
                .unwrap_or(u32::MAX),
            screenshot_limit_bytes: cutout_core::MAX_CAPTURE_SCREENSHOT_BYTES as u64,
        }
    }
}

/// A receipt already validated and committed by the Rust operation owner.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionReceiptDto {
    /// Retry-stable operation UUID.
    pub operation_id: String,
    /// Digest of the exact submitted snapshot.
    pub sha256: String,
    /// Durable remote byte length.
    pub bytes: u64,
    /// Service commit time in Unix milliseconds.
    pub received_at_milliseconds: u64,
}

impl From<CaptureSubmissionReceipt> for MobileCaptureSubmissionReceiptDto {
    fn from(receipt: CaptureSubmissionReceipt) -> Self {
        Self {
            operation_id: receipt.operation_id,
            sha256: receipt.sha256,
            bytes: receipt.bytes,
            received_at_milliseconds: receipt.received_at_milliseconds,
        }
    }
}

/// Bounded failure classification, without service body, payload or credentials.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionFailureDto {
    /// No complete network result was obtained.
    Network,
    /// The service could not authenticate the retained credential.
    Unauthorized,
    /// The authenticated account cannot access this operation.
    Forbidden,
    /// The operation or receipt conflicts with the fixed snapshot.
    Conflict,
    /// The service rejected the request.
    Rejected,
    /// The request budget was exhausted.
    Throttled,
    /// The service could not complete the request.
    Unavailable,
    /// The payload exceeded a service resource limit.
    TooLarge,
    /// The payload failed capture validation.
    InvalidCapture,
    /// The payload contains excluded music metadata.
    MusicPresent,
    /// Private capture storage is full.
    QuotaExceeded,
    /// The remote operation was already cancelled.
    Cancelled,
}

impl From<persistence::CaptureSubmissionTransferError> for MobileCaptureSubmissionFailureDto {
    fn from(failure: persistence::CaptureSubmissionTransferError) -> Self {
        use persistence::CaptureSubmissionTransferError as Failure;
        match failure {
            Failure::Network => Self::Network,
            Failure::Unauthorized => Self::Unauthorized,
            Failure::Forbidden => Self::Forbidden,
            Failure::Conflict => Self::Conflict,
            Failure::Rejected => Self::Rejected,
            Failure::Throttled => Self::Throttled,
            Failure::Unavailable => Self::Unavailable,
            Failure::TooLarge => Self::TooLarge,
            Failure::InvalidCapture => Self::InvalidCapture,
            Failure::MusicPresent => Self::MusicPresent,
            Failure::QuotaExceeded => Self::QuotaExceeded,
            Failure::Cancelled => Self::Cancelled,
        }
    }
}

/// Provenance origin retained by a private Send snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionOriginDto {
    /// Captured during ordinary connection or ride use.
    Automatic,
    /// Captured after explicit recording action.
    Manual,
    /// Legacy source does not establish how recording began.
    Unknown,
}

impl From<cutout_core::CaptureSubmissionOrigin> for MobileCaptureSubmissionOriginDto {
    fn from(origin: cutout_core::CaptureSubmissionOrigin) -> Self {
        match origin {
            cutout_core::CaptureSubmissionOrigin::Automatic => Self::Automatic,
            cutout_core::CaptureSubmissionOrigin::Manual => Self::Manual,
            cutout_core::CaptureSubmissionOrigin::Unknown => Self::Unknown,
        }
    }
}

/// Current Rust-owned operation, safe to display after relaunch without sending.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionStatusDto {
    /// Stable identity for this selected snapshot and every explicit retry.
    pub operation_id: String,
    /// Current persisted phase, including unconfirmed remote removal.
    pub phase: MobileCaptureSubmissionPhaseDto,
    /// Retained recording provenance; unknown is not inferred from the Send action.
    pub origin: MobileCaptureSubmissionOriginDto,
    /// Durable completeness. Use this value rather than interpreting a zero loss count.
    pub integrity: crate::MobileCaptureIntegrityDto,
    /// Local capture payload availability, independent from remote deletion state.
    pub local_payload_state: MobileCaptureSubmissionLocalPayloadStateDto,
    /// Actual included categories, independent of native presentation flags.
    pub categories: MobileCaptureSubmissionCategoriesDto,
    /// Known observation loss disclosed with incomplete durable recordings.
    pub dropped_messages: u64,
    /// Exact immutable submitted byte count.
    pub byte_count: u64,
    /// Matching durable receipt, when one has been committed.
    pub receipt: Option<MobileCaptureSubmissionReceiptDto>,
    /// Last classified failure without raw response or evidence bytes.
    pub failure: Option<MobileCaptureSubmissionFailureDto>,
}

impl From<persistence::CaptureSubmissionStatus> for MobileCaptureSubmissionStatusDto {
    fn from(status: persistence::CaptureSubmissionStatus) -> Self {
        let dropped_messages = match status.metadata.integrity {
            CaptureSubmissionIntegrity::Complete | CaptureSubmissionIntegrity::Unknown => 0,
            CaptureSubmissionIntegrity::Incomplete { dropped_messages } => dropped_messages,
        };
        Self {
            operation_id: status.operation_id,
            phase: status.phase.into(),
            origin: status.metadata.origin.into(),
            integrity: match status.metadata.integrity {
                CaptureSubmissionIntegrity::Complete => crate::MobileCaptureIntegrityDto::Complete,
                CaptureSubmissionIntegrity::Incomplete { dropped_messages } => {
                    crate::MobileCaptureIntegrityDto::Incomplete { dropped_messages }
                }
                CaptureSubmissionIntegrity::Unknown => crate::MobileCaptureIntegrityDto::Unknown,
            },
            local_payload_state: status.local_payload_state.into(),
            categories: status.categories.into(),
            dropped_messages,
            byte_count: status.metadata.bytes,
            receipt: status.receipt.map(Into::into),
            failure: status.failure.map(Into::into),
        }
    }
}

/// Live transport state, distinct from durable receipt confirmation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionProgressPhaseDto {
    /// The HTTP body is consuming immutable snapshot bytes.
    Uploading,
    /// All bytes were provided; Rust is waiting for a matching durable receipt.
    VerifyingReceipt,
}

/// Ephemeral progress for one admitted Send attempt; never restored on relaunch.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionProgressDto {
    /// Snapshot operation, stable across retries.
    pub operation_id: String,
    /// Rust-owned revision of this specific attempt.
    pub attempt_id: u64,
    /// Payload bytes supplied to the HTTP body, excluding HTTP/TLS overhead.
    pub bytes_sent: u64,
    /// Full immutable snapshot size, including note and screenshots.
    pub total_bytes: u64,
    /// Upload completion does not establish service receipt.
    pub phase: MobileCaptureSubmissionProgressPhaseDto,
}

impl From<persistence::CaptureSubmissionProgress> for MobileCaptureSubmissionProgressDto {
    fn from(progress: persistence::CaptureSubmissionProgress) -> Self {
        Self {
            operation_id: progress.operation_id,
            attempt_id: progress.attempt_id,
            bytes_sent: progress.bytes_sent,
            total_bytes: progress.total_bytes,
            phase: match progress.phase {
                persistence::CaptureSubmissionProgressPhase::Uploading => {
                    MobileCaptureSubmissionProgressPhaseDto::Uploading
                }
                persistence::CaptureSubmissionProgressPhase::VerifyingReceipt => {
                    MobileCaptureSubmissionProgressPhaseDto::VerifyingReceipt
                }
            },
        }
    }
}

/// Receives bounded transient progress only while this Rust attempt is current.
#[uniffi::export(callback_interface)]
pub trait MobileCaptureSubmissionProgressListener: Send + Sync {
    /// Dispatch native presentation promptly; callbacks may arrive off the main thread.
    fn on_progress(&self, progress: MobileCaptureSubmissionProgressDto);
}

/// Admission or local storage failure; transfer failures remain in persisted status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileCaptureSubmissionError {
    /// Note exceeds the shared UTF-8 budget.
    #[error("The note must be 1,024 bytes or less.")]
    NoteTooLong,
    /// Too many screenshots were selected.
    #[error("Choose up to four screenshots.")]
    TooManyScreenshots,
    /// One encoded screenshot is too large.
    #[error("Each screenshot must be 5 MiB or less.")]
    ScreenshotTooLarge,
    /// The picker did not produce a supported image.
    #[error("The screenshot could not be read.")]
    InvalidScreenshot,
    /// The supplied source is absent or is not a capture UUID.
    #[error("capture source is invalid")]
    InvalidSource,
    /// The Rust owner has no valid durable completion for this action.
    #[error("capture is not ready for private submission")]
    NotReady,
    /// The fixed service transport could not be configured.
    #[error("capture submission service is unavailable")]
    Unavailable,
    /// The operation cannot use the provided account/credential.
    #[error("capture submission requires its existing account")]
    Unauthorized,
    /// The request cannot currently fit the service's rate budget.
    #[error("capture submission request budget is exhausted")]
    RateLimited,
    /// The service has no remaining private capture storage.
    #[error("private capture storage quota is exhausted")]
    QuotaExceeded,
    /// The snapshot failed its full-stream validation.
    #[error("capture submission is invalid")]
    InvalidCapture,
    /// Local persistence or its worker could not complete the operation.
    #[error("capture submission storage failed")]
    StorageFailure,
    /// Cancellation prevented completion.
    #[error("capture submission was cancelled")]
    Cancelled,
    /// Receipt identity did not match the selected immutable snapshot.
    #[error("capture submission receipt did not match")]
    ReceiptMismatch,
}

impl From<persistence::CaptureSubmissionError> for MobileCaptureSubmissionError {
    fn from(error: persistence::CaptureSubmissionError) -> Self {
        use persistence::CaptureSubmissionError as Error;
        match error {
            Error::InvalidEvidence(e) => match e {
                cutout_core::CaptureEvidenceError::NoteTooLong => Self::NoteTooLong,
                cutout_core::CaptureEvidenceError::TooManyScreenshots => Self::TooManyScreenshots,
                cutout_core::CaptureEvidenceError::ScreenshotTooLarge => Self::ScreenshotTooLarge,
                cutout_core::CaptureEvidenceError::InvalidScreenshot => Self::InvalidScreenshot,
            },
            Error::NotFound => Self::InvalidSource,
            Error::NotFinished | Error::ProvenanceUnavailable | Error::InvalidState => {
                Self::NotReady
            }
            Error::InvalidCapture | Error::TooLarge => Self::InvalidCapture,
            Error::WrongAccount => Self::Unauthorized,
            Error::Storage(_) => Self::StorageFailure,
        }
    }
}

/// Executes explicit effects for one caller-selected Rust source/operation.
/// Constructing or reading this handle never starts a transfer.
#[derive(uniffi::Object)]
pub struct CaptureSubmissionClientHandle {
    database: persistence::RideDatabase,
    transport: HttpCaptureTransport,
    review_cancel: Arc<AtomicBool>,
}

impl std::fmt::Debug for CaptureSubmissionClientHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CaptureSubmissionClientHandle")
            .finish_non_exhaustive()
    }
}

#[uniffi::export(async_runtime = "tokio")]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary handles and strings"
)]
impl CaptureSubmissionClientHandle {
    /// Uses the existing database and the fixed HTTPS service, without source paths.
    /// # Errors
    /// Returns `Unavailable` when the bounded HTTP client cannot be configured.
    #[uniffi::constructor]
    pub fn new(
        database: Arc<RideDatabaseHandle>,
    ) -> Result<Arc<Self>, MobileCaptureSubmissionError> {
        let transport = HttpCaptureTransport::production()
            .map_err(|_| MobileCaptureSubmissionError::Unavailable)?;
        Ok(Arc::new(Self {
            database: database.inner.clone(),
            transport,
            review_cancel: Arc::new(AtomicBool::new(false)),
        }))
    }

    /// Reads saved context without transmission or original-file changes.
    /// # Errors
    /// Returns source or storage failures.
    pub async fn evidence(
        &self,
        source: MobileCaptureSubmissionSourceDto,
    ) -> Result<MobileCaptureEvidenceDto, MobileCaptureSubmissionError> {
        let source = source.try_into()?;
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.capture_evidence(source))
            .await
            .map_err(|_| MobileCaptureSubmissionError::StorageFailure)?
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Saves a bounded note and selected screenshots for this recording.
    /// # Errors
    /// Rejects invalid context or edits to an already authorized snapshot.
    pub async fn save_evidence(
        &self,
        source: MobileCaptureSubmissionSourceDto,
        note: String,
        screenshots: Vec<Vec<u8>>,
    ) -> Result<MobileCaptureEvidenceDto, MobileCaptureSubmissionError> {
        let source = source.try_into()?;
        let database = self.database.clone();
        let evidence = cutout_core::CaptureAttachments {
            note,
            screenshots: screenshots
                .into_iter()
                .map(|jpeg| cutout_core::CaptureScreenshot { jpeg })
                .collect(),
        };
        tokio::task::spawn_blocking(move || database.save_capture_evidence(source, evidence))
            .await
            .map_err(|_| MobileCaptureSubmissionError::StorageFailure)?
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Reads only persisted local status. Relaunch does not resume an upload.
    /// # Errors
    /// Returns an invalid source or typed local storage failure.
    pub async fn status(
        &self,
        source: MobileCaptureSubmissionSourceDto,
    ) -> Result<Option<MobileCaptureSubmissionStatusDto>, MobileCaptureSubmissionError> {
        let source = source.try_into()?;
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.capture_submission(source))
            .await
            .map_err(|_| MobileCaptureSubmissionError::StorageFailure)?
            .map(|status| status.map(Into::into))
            .map_err(Into::into)
    }

    /// Freezes the Rust-approved immutable snapshot locally for disclosure.
    /// # Errors
    /// Rejects unfinalized, unproven or invalid evidence without transmitting anything.
    pub async fn prepare(
        &self,
        source: MobileCaptureSubmissionSourceDto,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        let source = source.try_into()?;
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.prepare_capture_submission(source))
            .await
            .map_err(|_| MobileCaptureSubmissionError::StorageFailure)?
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Runs exactly the user-confirmed operation, including receipt recovery for a retry.
    /// # Errors
    /// Returns typed admission/storage errors; bounded HTTP failures persist in status.
    pub async fn send(
        &self,
        operation_id: String,
        account_id: String,
        token: String,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        self.database
            .send_capture_submission(&operation_id, &account_id, &token, &self.transport)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Runs the explicit Send with byte progress fenced to its admitted attempt.
    /// # Errors
    /// Returns the same typed outcomes as Send. Only a matching committed receipt is Sent.
    pub async fn send_with_progress(
        &self,
        operation_id: String,
        account_id: String,
        token: String,
        listener: Box<dyn MobileCaptureSubmissionProgressListener>,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        self.database
            .send_capture_submission_with_progress(
                &operation_id,
                &account_id,
                &token,
                &self.transport,
                Arc::new(move |progress| listener.on_progress(progress.into())),
            )
            .await
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Cancels the chosen operation; local prepared cancellation needs no account.
    /// # Errors
    /// Unconfirmed remote cancellation remains pending; partial credentials are refused.
    pub async fn cancel(
        &self,
        operation_id: String,
        account_id: Option<String>,
        token: Option<String>,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        let credential = match (account_id.as_deref(), token.as_deref()) {
            (Some(account_id), Some(token)) => {
                Some(persistence::CaptureSubmissionCredential { account_id, token })
            }
            (None, None) => None,
            _ => return Err(MobileCaptureSubmissionError::Unauthorized),
        };
        self.database
            .cancel_capture_submission(&operation_id, credential, &self.transport)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Deletes only the remote copy using the retained operation and existing account.
    /// # Errors
    /// Unconfirmed deletion remains pending and preserves the local original.
    pub async fn delete(
        &self,
        operation_id: String,
        account_id: String,
        token: String,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        self.database
            .delete_capture_submission(&operation_id, &account_id, &token, &self.transport)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{
        CaptureOrigin, GattChannel, MonotonicTimestamp, PevcapRecord, WallClockUnixTimestamp,
    };
    use persistence::{CaptureMetadata, CaptureWriteOutcome, CaptureWriter, CaptureWriterFinish};
    use serde_json::Value;
    use std::{
        net::{Ipv4Addr, SocketAddr},
        path::{Path, PathBuf},
    };
    use uuid::Uuid;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Result<Self, std::io::Error> {
            let path =
                std::env::temp_dir().join(format!("cutout-capture-submission-{}", Uuid::new_v4()));
            std::fs::create_dir(&path)?;
            Ok(Self(path.canonicalize()?))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn finished_capture(
        directory: &Path,
    ) -> Result<
        (Arc<RideDatabaseHandle>, MobileCaptureSubmissionSourceDto),
        Box<dyn std::error::Error>,
    > {
        let database = crate::open_ride_database(
            directory
                .join("local.sqlite")
                .to_string_lossy()
                .into_owned(),
        )?;
        let writer = CaptureWriter::start_with_database(
            directory.join("private-original.jsonl"),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "unsupported-unverified-device",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec!["capture_label=lamp_button_pressed".into()],
            },
            database.inner.clone(),
        )?;
        assert_eq!(
            writer.try_send_record(PevcapRecord::inbound_notification(
                MonotonicTimestamp::new(1),
                GattChannel::from_bytes([0x11; 16]),
                GattChannel::from_bytes([0x22; 16]),
                vec![0xff, 0x00, 0xa5],
            )),
            CaptureWriteOutcome::Accepted
        );
        let CaptureWriterFinish::DatabaseFinished {
            live_capture_id,
            receipt,
            ..
        } = writer.finish_without_export()?
        else {
            return Err("expected durable database completion".into());
        };
        database.inner.retain_finished_database_capture(
            &receipt,
            CaptureOrigin::Manual,
            Some("Unverified advertisement"),
            WallClockUnixTimestamp::new(1_700_000_001_000),
        )?;
        Ok((
            database,
            MobileCaptureSubmissionSourceDto::LiveCapture {
                id: live_capture_id.to_string(),
            },
        ))
    }

    #[test]
    fn source_requires_a_retained_uuid_instead_of_accepting_a_path() {
        for source in [
            MobileCaptureSubmissionSourceDto::LiveCapture {
                id: "/tmp/capture.jsonl".into(),
            },
            MobileCaptureSubmissionSourceDto::RecordedArtifact {
                id: "not-a-writer-receipt".into(),
            },
            MobileCaptureSubmissionSourceDto::PevcapDigest {
                sha256: "NOT-A-SHA256".into(),
            },
        ] {
            assert_eq!(
                persistence::CaptureSubmissionSource::try_from(source),
                Err(MobileCaptureSubmissionError::InvalidSource)
            );
        }
        let id = Uuid::new_v4().to_string();
        assert!(
            persistence::CaptureSubmissionSource::try_from(
                MobileCaptureSubmissionSourceDto::LiveCapture { id: id.clone() }
            )
            .is_ok()
        );
        assert!(
            persistence::CaptureSubmissionSource::try_from(
                MobileCaptureSubmissionSourceDto::RecordedArtifact { id }
            )
            .is_ok()
        );
        assert!(
            persistence::CaptureSubmissionSource::try_from(
                MobileCaptureSubmissionSourceDto::PevcapDigest {
                    sha256: "ab".repeat(32),
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn status_projection_preserves_actual_loss_categories_and_pending_removal()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let mut status = database
            .inner
            .prepare_capture_submission(source.try_into()?)?;
        let operation = status.operation_id.clone();
        let byte_count = status.metadata.bytes;
        status.metadata.integrity = CaptureSubmissionIntegrity::Incomplete {
            dropped_messages: 7,
        };
        status.phase = persistence::CaptureSubmissionPhase::DeletionPending;
        status.failure = Some(persistence::CaptureSubmissionTransferError::Unavailable);
        let projected: MobileCaptureSubmissionStatusDto = status.into();
        assert_eq!(projected.operation_id, operation);
        assert_eq!(projected.byte_count, byte_count);
        assert_eq!(projected.dropped_messages, 7);
        assert!(projected.categories.device_identifiers);
        assert!(!projected.categories.location);
        assert!(projected.categories.annotations);
        assert_eq!(projected.phase, MobileCaptureSubmissionPhaseDto::Deleting);
        assert_eq!(
            projected.failure,
            Some(MobileCaptureSubmissionFailureDto::Unavailable)
        );
        assert!(projected.receipt.is_none());
        assert_eq!(
            MobileCaptureSubmissionPhaseDto::from(persistence::CaptureSubmissionPhase::Rejected),
            MobileCaptureSubmissionPhaseDto::Rejected
        );
        assert_eq!(
            MobileCaptureSubmissionPhaseDto::from(persistence::CaptureSubmissionPhase::Retryable),
            MobileCaptureSubmissionPhaseDto::Failed
        );
        assert_eq!(
            MobileCaptureSubmissionPhaseDto::from(
                persistence::CaptureSubmissionPhase::CancellationPending
            ),
            MobileCaptureSubmissionPhaseDto::Cancelling
        );
        database.inner.clone().shutdown()?;
        Ok(())
    }

    #[test]
    fn status_projection_keeps_unknown_origin_and_integrity_explicit()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let mut status = database
            .inner
            .prepare_capture_submission(source.try_into()?)?;
        status.metadata.origin = cutout_core::CaptureSubmissionOrigin::Unknown;
        status.metadata.integrity = CaptureSubmissionIntegrity::Unknown;

        let projected: MobileCaptureSubmissionStatusDto = status.into();
        assert_eq!(projected.origin, MobileCaptureSubmissionOriginDto::Unknown);
        assert_eq!(
            projected.integrity,
            crate::MobileCaptureIntegrityDto::Unknown
        );
        assert_eq!(projected.dropped_messages, 0);
        Ok(())
    }

    #[tokio::test]
    async fn persisted_service_rejection_is_projected_without_retry_admission()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct RejectingTransport {
            uploads: AtomicUsize,
        }

        impl persistence::CaptureSubmissionTransport for RejectingTransport {
            fn upload<'a>(
                &'a self,
                _operation_id: &'a str,
                _token: &'a str,
                _metadata: &'a cutout_core::CaptureSubmissionMetadata,
                _payload: persistence::CaptureSubmissionPayload,
            ) -> persistence::CaptureSubmissionFuture<'a, CaptureSubmissionReceipt> {
                self.uploads.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Err(persistence::CaptureSubmissionTransferError::Forbidden) })
            }

            fn receipt<'a>(
                &'a self,
                _operation_id: &'a str,
                _token: &'a str,
            ) -> persistence::CaptureSubmissionFuture<'a, Option<CaptureSubmissionReceipt>>
            {
                Box::pin(async { Ok(None) })
            }

            fn delete<'a>(
                &'a self,
                _operation_id: &'a str,
                _token: &'a str,
            ) -> persistence::CaptureSubmissionFuture<'a, ()> {
                Box::pin(async { Err(persistence::CaptureSubmissionTransferError::Unavailable) })
            }
        }

        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let transport = RejectingTransport {
            uploads: AtomicUsize::new(0),
        };
        let prepared = database
            .inner
            .prepare_capture_submission(source.clone().try_into()?)?;
        let account_id = Uuid::new_v4().to_string();
        let rejected = database
            .inner
            .send_capture_submission(
                &prepared.operation_id,
                &account_id,
                "test-token",
                &transport,
            )
            .await?;
        assert_eq!(
            rejected.phase,
            persistence::CaptureSubmissionPhase::Rejected
        );
        let client = CaptureSubmissionClientHandle {
            database: database.inner.clone(),
            review_cancel: Arc::new(AtomicBool::new(false)),
            transport: HttpCaptureTransport::loopback("http://127.0.0.1:1/")?,
        };
        let projected = client.status(source).await?.ok_or("status missing")?;
        assert_eq!(projected.phase, MobileCaptureSubmissionPhaseDto::Rejected);
        assert_eq!(
            projected.failure,
            Some(MobileCaptureSubmissionFailureDto::Forbidden)
        );
        assert!(projected.receipt.is_none());
        let retry = database
            .inner
            .send_capture_submission(
                &prepared.operation_id,
                &account_id,
                "test-token",
                &transport,
            )
            .await
            .map(MobileCaptureSubmissionStatusDto::from)
            .map_err(MobileCaptureSubmissionError::from);
        assert_eq!(retry, Err(MobileCaptureSubmissionError::NotReady));
        assert_eq!(transport.uploads.load(Ordering::Relaxed), 1);
        database.inner.clone().shutdown()?;
        Ok(())
    }

    #[tokio::test]
    async fn prepared_capture_can_be_cancelled_locally_without_credentials()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let client = CaptureSubmissionClientHandle {
            database: database.inner.clone(),
            review_cancel: Arc::new(AtomicBool::new(false)),
            transport: HttpCaptureTransport::loopback("http://127.0.0.1:1/")?,
        };
        assert!(client.status(source.clone()).await?.is_none());
        let prepared = client.prepare(source.clone()).await?;
        assert_eq!(prepared.phase, MobileCaptureSubmissionPhaseDto::Prepared);
        assert_eq!(
            client
                .cancel(
                    prepared.operation_id.clone(),
                    Some(Uuid::new_v4().to_string()),
                    None
                )
                .await,
            Err(MobileCaptureSubmissionError::Unauthorized)
        );
        let cancelled = client.cancel(prepared.operation_id, None, None).await?;
        assert_eq!(cancelled.phase, MobileCaptureSubmissionPhaseDto::Cancelled);
        assert!(cancelled.receipt.is_none());
        assert_eq!(client.status(source).await?, Some(cancelled));
        database.inner.clone().shutdown()?;
        Ok(())
    }

    #[tokio::test]
    #[allow(
        clippy::too_many_lines,
        reason = "one server-backed scenario verifies receipt, local cleanup and remote deletion together"
    )]
    async fn ffi_send_receives_a_receipt_removes_local_payload_and_deletes_remote_copy()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let store = cutout_server::Store::open(&directory.0.join("server.sqlite"))?;
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let base_url = format!("http://{}/", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                cutout_server::router(store).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        let http = cutout_core::http::client_builder()
            .expect("platform verifier")
            .no_proxy()
            .build()?;
        let identity: Value = http
            .post(format!("{base_url}v1/identities"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let account_id = identity["id"]
            .as_str()
            .ok_or("identity id missing")?
            .to_owned();
        let token = identity["token"]
            .as_str()
            .ok_or("identity token missing")?
            .to_owned();
        let client = CaptureSubmissionClientHandle {
            database: database.inner.clone(),
            review_cancel: Arc::new(AtomicBool::new(false)),
            transport: HttpCaptureTransport::loopback(&base_url)?,
        };
        assert!(client.status(source.clone()).await?.is_none());
        let prepared = client.prepare(source.clone()).await?;
        assert!(prepared.receipt.is_none());
        assert!(!prepared.categories.location);
        let sent = client
            .send(
                prepared.operation_id.clone(),
                account_id.clone(),
                token.clone(),
            )
            .await?;
        assert_eq!(sent.phase, MobileCaptureSubmissionPhaseDto::Sent);
        assert_eq!(
            sent.local_payload_state,
            MobileCaptureSubmissionLocalPayloadStateDto::RemovalPending
        );
        let receipt = sent.receipt.clone().ok_or("durable receipt missing")?;
        assert_eq!(receipt.operation_id, prepared.operation_id);
        assert_eq!(receipt.bytes, prepared.byte_count);
        let repeated_send = client
            .send(
                prepared.operation_id.clone(),
                account_id.clone(),
                token.clone(),
            )
            .await?;
        assert_eq!(repeated_send.phase, sent.phase);
        assert_eq!(repeated_send.operation_id, sent.operation_id);
        assert_eq!(repeated_send.receipt, sent.receipt);
        assert!(matches!(
            repeated_send.local_payload_state,
            MobileCaptureSubmissionLocalPayloadStateDto::RemovalPending
                | MobileCaptureSubmissionLocalPayloadStateDto::Removed
        ));
        let cleanup_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        let removed = loop {
            let status = client
                .status(source.clone())
                .await?
                .ok_or("sent submission status disappeared during cleanup")?;
            if status.local_payload_state == MobileCaptureSubmissionLocalPayloadStateDto::Removed {
                break status;
            }
            if tokio::time::Instant::now() >= cleanup_deadline {
                return Err(
                    "receipt-confirmed local payload cleanup did not finish before deadline".into(),
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        assert_eq!(removed.phase, MobileCaptureSubmissionPhaseDto::Sent);
        assert_eq!(removed.receipt, Some(receipt.clone()));
        let persistence::CaptureSubmissionSource::LiveCapture(id) = source.clone().try_into()?
        else {
            return Err("expected live source".into());
        };
        assert!(
            database
                .inner
                .export_live_capture(id, &directory.0.join("after-send.jsonl"))
                .is_err()
        );
        let wrong_account = client
            .delete(
                prepared.operation_id.clone(),
                Uuid::new_v4().to_string(),
                token.clone(),
            )
            .await;
        assert_eq!(
            wrong_account,
            Err(MobileCaptureSubmissionError::Unauthorized)
        );
        let deleted = client
            .delete(prepared.operation_id.clone(), account_id, token.clone())
            .await?;
        assert_eq!(deleted.phase, MobileCaptureSubmissionPhaseDto::Deleted);
        assert_eq!(deleted.receipt.as_ref(), Some(&receipt));
        assert_eq!(
            deleted.local_payload_state,
            MobileCaptureSubmissionLocalPayloadStateDto::Removed
        );
        let deleted_status = client
            .status(source.clone())
            .await?
            .ok_or("deleted submission status disappeared")?;
        assert_eq!(
            deleted_status.phase,
            MobileCaptureSubmissionPhaseDto::Deleted
        );
        assert_eq!(deleted_status.receipt, Some(receipt.clone()));
        assert_eq!(
            deleted_status.local_payload_state,
            MobileCaptureSubmissionLocalPayloadStateDto::Removed
        );
        let remote = http
            .get(format!(
                "{base_url}v1/capture-submissions/{}",
                prepared.operation_id
            ))
            .bearer_auth(token)
            .send()
            .await?;
        assert_eq!(remote.status(), reqwest::StatusCode::NOT_FOUND);
        task.abort();
        database.inner.clone().shutdown()?;
        Ok(())
    }

    struct ProgressListener(Arc<std::sync::Mutex<Vec<MobileCaptureSubmissionProgressDto>>>);

    impl MobileCaptureSubmissionProgressListener for ProgressListener {
        fn on_progress(&self, progress: MobileCaptureSubmissionProgressDto) {
            self.0.lock().unwrap().push(progress);
        }
    }

    fn gated_capture_router(
        received: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        matching: bool,
    ) -> axum::Router {
        use axum::{
            Router,
            body::Bytes,
            http::{HeaderMap, StatusCode},
            routing::get,
        };
        Router::new().route(
            "/v1/capture-submissions/{operation}",
            get(|| async { StatusCode::NOT_FOUND }).put(
                move |axum::extract::Path(operation): axum::extract::Path<String>,
                      headers: HeaderMap,
                      body: Bytes| {
                    let received = Arc::clone(&received);
                    let release = Arc::clone(&release);
                    async move {
                        let metadata: cutout_core::CaptureSubmissionMetadata =
                            serde_json::from_str(
                                headers.get("x-cutout-capture").unwrap().to_str().unwrap(),
                            )
                            .unwrap();
                        let summary = cutout_core::validate_capture_submission_payload(
                            body.as_ref(),
                            &metadata,
                        )
                        .unwrap();
                        assert_eq!(body.len() as u64, metadata.bytes);
                        let receipt = CaptureSubmissionReceipt {
                            operation_id: operation,
                            sha256: if matching {
                                summary.sha256
                            } else {
                                "a".repeat(64)
                            },
                            bytes: metadata.bytes,
                            received_at_milliseconds: 123,
                        };
                        received.notify_one();
                        release.notified().await;
                        axum::Json(receipt)
                    }
                },
            ),
        )
    }

    #[tokio::test]
    async fn ffi_progress_waits_for_matching_receipt_after_http_consumes_all_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::Mutex;
        use tokio::sync::Notify;
        for matching in [true, false] {
            let directory = TestDirectory::new()?;
            let (database, source) = finished_capture(&directory.0)?;
            let received = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            let router =
                gated_capture_router(Arc::clone(&received), Arc::clone(&release), matching);
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
            let base_url = format!("http://{}/", listener.local_addr()?);
            let server = tokio::spawn(async move {
                let _ = axum::serve(listener, router).await;
            });
            let client = CaptureSubmissionClientHandle {
                database: database.inner.clone(),
                review_cancel: Arc::new(AtomicBool::new(false)),
                transport: HttpCaptureTransport::loopback(&base_url)?,
            };
            client
                .save_evidence(
                    source.clone(),
                    "Device access note".into(),
                    vec![vec![0xff, 0xd8, 0xff, 0xd9]; 4],
                )
                .await?;
            let prepared = client.prepare(source.clone()).await?;
            let events = Arc::new(Mutex::new(Vec::new()));
            let send = client.send_with_progress(
                prepared.operation_id.clone(),
                Uuid::new_v4().to_string(),
                "test-token".into(),
                Box::new(ProgressListener(Arc::clone(&events))),
            );
            tokio::pin!(send);
            tokio::select! {
                () = received.notified() => {},
                result = &mut send => return Err(format!("Send completed before receipt was released: {result:?}").into()),
                () = tokio::time::sleep(std::time::Duration::from_secs(10)) => return Err("HTTP body was not received within the test deadline".into()),
            }
            let pending = client.status(source).await?.ok_or("status missing")?;
            assert_eq!(pending.phase, MobileCaptureSubmissionPhaseDto::Sending);
            assert!(pending.receipt.is_none());
            {
                let progress = events.lock().unwrap();
                assert_eq!(progress[0].bytes_sent, 0);
                assert!(
                    progress
                        .iter()
                        .all(|event| event.operation_id == prepared.operation_id
                            && event.total_bytes == prepared.byte_count
                            && event.attempt_id == progress[0].attempt_id)
                );
                let last = progress.last().ok_or("progress missing")?;
                assert_eq!(last.bytes_sent, prepared.byte_count);
                assert_eq!(
                    last.phase,
                    MobileCaptureSubmissionProgressPhaseDto::VerifyingReceipt
                );
            }
            release.notify_one();
            let complete = tokio::time::timeout(std::time::Duration::from_secs(10), send).await??;
            assert_eq!(
                complete.phase,
                if matching {
                    MobileCaptureSubmissionPhaseDto::Sent
                } else {
                    MobileCaptureSubmissionPhaseDto::Rejected
                }
            );
            assert_eq!(complete.receipt.is_some(), matching);
            server.abort();
            database.inner.clone().shutdown()?;
        }
        Ok(())
    }
}
