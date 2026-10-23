//! Explicit private submission of an immutable, durably completed Bluetooth capture.

use super::{
    CapturePublication, LiveCaptureHistoryEntry, LiveCaptureId, LiveCaptureIntegrity,
    LiveCaptureState, PevcapImportReceipt, Reply, RideDatabase, StorageError,
};
use crate::CaptureArtifactId;
use cutout_core::{
    CaptureAttachments, CaptureEvidenceError, CaptureEvidenceManifest, CaptureSubmissionCategories,
    CaptureSubmissionIntegrity, CaptureSubmissionMetadata, CaptureSubmissionProvenance,
    CaptureSubmissionReceipt, CaptureSubmissionTransformation, PevcapEncoding,
    validate_capture_submission,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    io::{self, Cursor, Read},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

/// A lookup key, never independent authority to submit an arbitrary file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSubmissionSource {
    /// A durably finalized database recording.
    LiveCapture(LiveCaptureId),
    /// A consumed writer's retained identity, including database aliases.
    RecordedArtifact(CaptureArtifactId),
}

/// Persisted progress of one explicit Send operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSubmissionPhase {
    /// Local fixed snapshot; no transmission has been authorized.
    Prepared,
    /// The user authorized a transfer attempt.
    Sending,
    /// A bounded transfer failed; explicit retry may reuse the same operation.
    Retryable,
    /// The service rejected the transfer.
    Rejected,
    /// Remote cancellation must be confirmed before reporting cancellation.
    CancellationPending,
    /// The local operation and any remote transfer have been cancelled.
    Cancelled,
    /// A validated service receipt was committed locally.
    Sent,
    /// Remote deletion must be confirmed before reporting deletion.
    DeletionPending,
    /// Remote deletion completed; local payload removal is tracked separately.
    Deleted,
}

/// Whether the private payload bytes remain available in this local database.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSubmissionLocalPayloadState {
    /// Existing data defaults to retained until a valid receipt authorizes cleanup.
    #[default]
    Retained,
    /// A validated receipt authorizes bounded local payload removal.
    RemovalPending,
    /// Payload rows were removed; receipt and source metadata remain available.
    Removed,
}

/// Current durable operation, projected without granting access to its bytes.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CaptureSubmissionStatus {
    /// Stable UUID for every retry of this snapshot.
    pub operation_id: String,
    /// Exact immutable bytes and their source provenance.
    pub metadata: CaptureSubmissionMetadata,
    /// Categories actually observed by full-stream validation.
    pub categories: CaptureSubmissionCategories,
    /// Current durable phase.
    pub phase: CaptureSubmissionPhase,
    /// Local payload retention after receipt-confirmed Send.
    #[serde(default)]
    pub local_payload_state: CaptureSubmissionLocalPayloadState,
    /// Credential identity bound by the first explicit Send.
    pub account_id: Option<String>,
    /// Receipt of durable remote completion, if confirmed.
    pub receipt: Option<CaptureSubmissionReceipt>,
    /// Classified last failure, without payload, notes or credentials.
    pub failure: Option<CaptureSubmissionTransferError>,
    revision: u64,
}

/// Transient transport progress for one admitted Send attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSubmissionProgressPhase {
    /// Payload bytes are being provided to the HTTP request body.
    Uploading,
    /// All payload bytes were provided; durable service receipt is still required.
    VerifyingReceipt,
}

/// Progress is never persisted or interpreted as a remote completion receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureSubmissionProgress {
    /// Stable operation for this immutable snapshot.
    pub operation_id: String,
    /// Durable revision fencing this specific admitted attempt.
    pub attempt_id: u64,
    /// Payload bytes consumed by the transport, without HTTP or TLS overhead.
    pub bytes_sent: u64,
    /// Exact full snapshot length, including optional note and screenshots.
    pub total_bytes: u64,
    /// Transport/receipt distinction for presentation.
    pub phase: CaptureSubmissionProgressPhase,
}

/// Synchronous observer; implementations should dispatch presentation work promptly.
pub type CaptureSubmissionProgressCallback = Arc<dyn Fn(CaptureSubmissionProgress) + Send + Sync>;

#[derive(Clone)]
struct CaptureSubmissionProgressReporter {
    database: RideDatabase,
    operation_id: String,
    attempt_id: u64,
    total_bytes: u64,
    callback: CaptureSubmissionProgressCallback,
    active: Arc<AtomicBool>,
}

impl CaptureSubmissionProgressReporter {
    fn report(&self, bytes_sent: u64) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let Ok(current) = self.database.submission_operation(&self.operation_id) else {
            return;
        };
        if current.revision != self.attempt_id
            || current.phase != CaptureSubmissionPhase::Sending
            || bytes_sent > self.total_bytes
        {
            return;
        }
        (self.callback)(CaptureSubmissionProgress {
            operation_id: self.operation_id.clone(),
            attempt_id: self.attempt_id,
            bytes_sent,
            total_bytes: self.total_bytes,
            phase: if bytes_sent == self.total_bytes {
                CaptureSubmissionProgressPhase::VerifyingReceipt
            } else {
                CaptureSubmissionProgressPhase::Uploading
            },
        });
    }
}

struct CaptureSubmissionProgressGuard(Option<CaptureSubmissionProgressReporter>);

impl Drop for CaptureSubmissionProgressGuard {
    fn drop(&mut self) {
        if let Some(progress) = &self.0 {
            progress.active.store(false, Ordering::Release);
        }
    }
}

/// Classified transport failure used by Rust's operation policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSubmissionTransferError {
    /// Streaming byte or count limits were exceeded.
    #[error("capture transfer is too large")]
    TooLarge,
    /// The received PEVCAP structure is invalid.
    #[error("capture transfer contains invalid evidence")]
    InvalidCapture,
    /// Provider music metadata is prohibited in this payload.
    #[error("capture transfer contains music metadata")]
    MusicPresent,
    /// Durable private storage quota is exhausted.
    #[error("capture transfer storage quota is exhausted")]
    QuotaExceeded,
    /// The remote operation is already tombstoned.
    #[error("capture transfer was cancelled")]
    Cancelled,
    /// No complete network result was obtained.
    #[error("capture transfer could not reach the service")]
    Network,
    /// The credential is unavailable or revoked.
    #[error("capture transfer requires authentication")]
    Unauthorized,
    /// The identity is not permitted to access this operation.
    #[error("capture transfer access was denied")]
    Forbidden,
    /// An operation identity was reused with different content.
    #[error("capture transfer identity conflicts")]
    Conflict,
    /// The selected bytes were rejected.
    #[error("capture transfer was rejected")]
    Rejected,
    /// The service's quota or request budget is exhausted.
    #[error("capture transfer quota is exhausted")]
    Throttled,
    /// The service cannot currently complete the operation.
    #[error("capture transfer service is unavailable")]
    Unavailable,
}

/// Admission and persistence failures, independent of native presentation.
#[derive(Debug, thiserror::Error)]
pub enum CaptureSubmissionError {
    /// User-entered context violates its portable limits.
    #[error(transparent)]
    InvalidEvidence(#[from] CaptureEvidenceError),
    /// The source or operation does not exist.
    #[error("capture submission was not found")]
    NotFound,
    /// No finished writer state exists, or its integrity is unknown.
    #[error("capture recording is not durably finished")]
    NotFinished,
    /// A real consumed-writer provenance publication is still required.
    #[error("capture recording provenance is unavailable")]
    ProvenanceUnavailable,
    /// Full-stream validation or original digest verification failed.
    #[error("capture submission bytes are invalid")]
    InvalidCapture,
    /// This operation belongs to another credential identity.
    #[error("capture submission belongs to another account")]
    WrongAccount,
    /// An incompatible attempt is already active or terminal.
    #[error("capture submission cannot perform this action in its current state")]
    InvalidState,
    /// A storage operation could not complete.
    #[error("capture submission storage failed")]
    Storage(#[from] StorageError),
}

/// Sendable future returned by a bounded native HTTP adapter.
pub type CaptureSubmissionFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CaptureSubmissionTransferError>> + Send + 'a>>;

/// An existing native credential, borrowed only for this explicit effect.
/// No credential value is persisted in submission state.
#[derive(Clone, Copy)]
pub struct CaptureSubmissionCredential<'a> {
    /// Identity of the already-authorized native credential.
    pub account_id: &'a str,
    /// Secret used only by the bounded transport adapter.
    pub token: &'a str,
}

impl std::fmt::Debug for CaptureSubmissionCredential<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CaptureSubmissionCredential")
            .finish_non_exhaustive()
    }
}

/// HTTP effects only; persistence owns admission, identity and completion policy.
pub trait CaptureSubmissionTransport: Sync {
    /// Transfers the supplied retained snapshot without selecting other content.
    fn upload<'a>(
        &'a self,
        operation_id: &'a str,
        token: &'a str,
        metadata: &'a CaptureSubmissionMetadata,
        payload: CaptureSubmissionPayload,
    ) -> CaptureSubmissionFuture<'a, CaptureSubmissionReceipt>;
    /// Finds a lost completion receipt; absence permits the same complete transfer.
    fn receipt<'a>(
        &'a self,
        operation_id: &'a str,
        token: &'a str,
    ) -> CaptureSubmissionFuture<'a, Option<CaptureSubmissionReceipt>>;
    /// Confirms a remote tombstone, even when no upload completed.
    fn delete<'a>(
        &'a self,
        operation_id: &'a str,
        token: &'a str,
    ) -> CaptureSubmissionFuture<'a, ()>;
}

/// Rust-only bounded reader over immutable bytes in the existing capture store.
/// It has no source path and is never exported through `UniFFI`.
pub struct CaptureSubmissionPayload {
    database: RideDatabase,
    digest: String,
    bytes: u64,
    sequence: u64,
    current: Cursor<Vec<u8>>,
    read_bytes: u64,
    digest_state: sha2::Sha256,
    complete: bool,
    raw_complete: bool,
    expected_digest: String,
    evidence: Cursor<Vec<u8>>,
    progress: Option<CaptureSubmissionProgressReporter>,
}

impl std::fmt::Debug for CaptureSubmissionPayload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CaptureSubmissionPayload")
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

impl CaptureSubmissionPayload {
    /// Exact expected length, usable for a streaming HTTP content length.
    #[must_use]
    pub const fn byte_count(&self) -> u64 {
        self.bytes
    }

    /// Reports bytes provided to the HTTP body, never remote receipt or a resumable offset.
    /// Call only when the current read chunk is handed to the request body stream.
    pub fn report_upload_progress(&self) {
        if let Some(progress) = &self.progress {
            progress.report(self.read_bytes);
        }
    }
}

impl Read for CaptureSubmissionPayload {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        use sha2::Digest;
        if buffer.is_empty() || self.complete {
            return Ok(0);
        }
        loop {
            let count = self.current.read(buffer)?;
            if count != 0 {
                self.read_bytes = self
                    .read_bytes
                    .checked_add(count as u64)
                    .filter(|count| *count <= self.bytes)
                    .ok_or_else(|| io::Error::other("capture snapshot length changed"))?;
                self.digest_state.update(&buffer[..count]);
                return Ok(count);
            }
            if self.raw_complete {
                let count = self.evidence.read(buffer)?;
                if count != 0 {
                    self.read_bytes = self
                        .read_bytes
                        .checked_add(count as u64)
                        .filter(|n| *n <= self.bytes)
                        .ok_or_else(|| io::Error::other("capture snapshot length changed"))?;
                    self.digest_state.update(&buffer[..count]);
                    return Ok(count);
                }
                if self.read_bytes != self.bytes
                    || hex::encode(self.digest_state.clone().finalize()) != self.expected_digest
                {
                    return Err(io::Error::other("capture snapshot integrity failed"));
                }
                self.complete = true;
                return Ok(0);
            }
            let chunk = self
                .database
                .pevcap_capture_chunk(&self.digest, self.sequence)
                .map_err(|_| io::Error::other("capture snapshot storage failed"))?;
            if let Some(chunk) = chunk {
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("capture snapshot sequence overflow"))?;
                self.current = Cursor::new(chunk);
            } else {
                self.raw_complete = true;
            }
        }
    }
}

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS capture_submissions (
    source_artifact_id TEXT PRIMARY KEY NOT NULL CHECK(length(source_artifact_id) = 36),
    operation_id TEXT NOT NULL UNIQUE CHECK(length(operation_id) = 36),
    snapshot_digest TEXT NOT NULL REFERENCES pevcap_imports(artifact_digest),
    evidence_json TEXT,
    status_json TEXT NOT NULL CHECK(json_valid(status_json))
        CHECK(json_extract(status_json, '$.operation_id') = operation_id)
        CHECK(coalesce(json_extract(status_json, '$.metadata.evidence.capture_sha256'), json_extract(status_json, '$.metadata.sha256')) = snapshot_digest)
        CHECK(json_extract(status_json, '$.metadata.provenance.source_artifact_id') = source_artifact_id)
);
CREATE TABLE IF NOT EXISTS capture_evidence (
    source_artifact_id TEXT PRIMARY KEY NOT NULL CHECK(length(source_artifact_id) = 36),
    evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json))
);";

pub(super) enum SourceRecord {
    Live(LiveCaptureHistoryEntry),
    File(PevcapImportReceipt),
}

struct SnapshotSource {
    origin: cutout_core::CaptureOrigin,
    integrity: CaptureSubmissionIntegrity,
    provenance: CaptureSubmissionProvenance,
    encoding: PevcapEncoding,
}

pub(super) enum Action {
    Start {
        account_id: String,
    },
    Removal {
        account_id: Option<String>,
        deleting: bool,
    },
    Complete {
        revision: u64,
        receipt: CaptureSubmissionReceipt,
    },
    Failure {
        revision: u64,
        failure: CaptureSubmissionTransferError,
    },
    Removed {
        revision: u64,
    },
}

pub(super) enum Command {
    Evidence {
        source: CaptureSubmissionSource,
        save: Option<CaptureAttachments>,
        reply: Reply<Result<CaptureAttachments, CaptureSubmissionError>>,
    },
    SnapshotEvidence {
        operation_id: String,
        reply: Reply<Vec<u8>>,
    },
    Source {
        source: CaptureSubmissionSource,
        reply: Reply<Option<SourceRecord>>,
    },
    Lookup {
        artifact_id: String,
        reply: Reply<Option<CaptureSubmissionStatus>>,
    },
    LookupOperation {
        operation_id: String,
        reply: Reply<Option<CaptureSubmissionStatus>>,
    },
    ManagedFileRemovalAuthorization {
        digest: String,
        size: u64,
        reply: Reply<bool>,
    },
    Insert {
        status: Box<CaptureSubmissionStatus>,
        evidence_json: Option<String>,
        reply: Reply<CaptureSubmissionStatus>,
    },
    Apply {
        operation_id: String,
        action: Action,
        reply: Reply<Result<CaptureSubmissionStatus, CaptureSubmissionError>>,
    },
}

impl RideDatabase {
    pub(super) fn managed_capture_file_removal_authorized(
        &self,
        digest: &str,
        size: u64,
    ) -> Result<bool, StorageError> {
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::ManagedFileRemovalAuthorization {
                digest: digest.to_owned(),
                size,
                reply,
            })
        })
    }

    pub(super) fn submission_source(
        &self,
        source: CaptureSubmissionSource,
    ) -> Result<SourceRecord, CaptureSubmissionError> {
        self.request(|reply| super::Command::CaptureSubmission(Command::Source { source, reply }))?
            .ok_or(CaptureSubmissionError::NotFound)
    }

    /// Reads existing status only. Opening a detail or relaunching never transmits data.
    /// # Errors
    /// Returns missing source identity or storage failures.
    pub fn capture_submission(
        &self,
        source: CaptureSubmissionSource,
    ) -> Result<Option<CaptureSubmissionStatus>, CaptureSubmissionError> {
        // A recorded identity may still resolve a prepared snapshot after local source cleanup.
        let artifact_id = match source {
            CaptureSubmissionSource::RecordedArtifact(id) => id.to_string(),
            CaptureSubmissionSource::LiveCapture(_) => match self.submission_source(source)? {
                SourceRecord::Live(entry) => match entry.recording {
                    Some(recording) => recording.artifact_id.to_string(),
                    None => return Ok(None),
                },
                SourceRecord::File(_) => return Err(CaptureSubmissionError::NotFound),
            },
        };
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::Lookup { artifact_id, reply })
        })
        .map_err(Into::into)
    }

    /// Reads saved notes/screenshots, or the frozen context for an existing Send.
    /// # Errors
    /// Returns missing source or storage failures.
    pub fn capture_evidence(
        &self,
        source: CaptureSubmissionSource,
    ) -> Result<CaptureAttachments, CaptureSubmissionError> {
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::Evidence {
                source,
                save: None,
                reply,
            })
        })?
    }

    /// Saves context for one recording without altering the raw original.
    /// # Errors
    /// Rejects invalid context or edits to an authorized immutable Send.
    pub fn save_capture_evidence(
        &self,
        source: CaptureSubmissionSource,
        evidence: CaptureAttachments,
    ) -> Result<CaptureAttachments, CaptureSubmissionError> {
        evidence.validate()?;
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::Evidence {
                source,
                save: Some(evidence),
                reply,
            })
        })?
    }

    /// Freezes and validates one durable recording locally, without network effects.
    /// The private original is preserved; the snapshot reuses the canonical PEVCAP store.
    /// Call off native callback and UI threads.
    /// # Errors
    /// Returns typed durability/provenance, malformed capture or storage failures.
    pub fn prepare_capture_submission(
        &self,
        source: CaptureSubmissionSource,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        if let Some(status) = self.capture_submission(source)? {
            return Ok(status);
        }
        let record = self.submission_source(source)?;
        let temporary = tempfile::tempdir().map_err(StorageError::from)?;
        let snapshot = temporary.path().join("snapshot.jsonl");
        let SnapshotSource {
            origin,
            integrity,
            provenance,
            encoding,
        } = self.export_submission_source(record, &snapshot)?;
        let summary = validate_capture_submission(
            std::fs::File::open(&snapshot).map_err(StorageError::from)?,
            encoding,
        )
        .map_err(|_| CaptureSubmissionError::InvalidCapture)?;
        let evidence = self.capture_evidence(source)?;
        let evidence_json = if evidence.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&evidence)
                    .map_err(|_| CaptureSubmissionError::InvalidCapture)?,
            )
        };
        let mut metadata = CaptureSubmissionMetadata {
            evidence: None,
            sha256: summary.sha256,
            bytes: summary.bytes,
            encoding,
            origin,
            integrity,
            provenance,
        };
        metadata
            .validate()
            .map_err(|_| CaptureSubmissionError::InvalidCapture)?;
        let preview = super::preflight_pevcap(
            &snapshot,
            encoding,
            Some(crate::pevcap_limits::PevcapLimits::IMPORT),
        )?;
        if preview.artifact_digest() != metadata.sha256 || preview.artifact_size() != metadata.bytes
        {
            return Err(CaptureSubmissionError::InvalidCapture);
        }
        self.confirm_pevcap_publication(
            &preview,
            super::ride_write::wall_clock_now_milliseconds()?,
            &CapturePublication::SubmissionSnapshot,
        )?;
        if let Some(json) = &evidence_json {
            use sha2::Digest;
            let mut digest = sha2::Sha256::new();
            let mut raw = std::fs::File::open(&snapshot).map_err(StorageError::from)?;
            let mut buffer = vec![0; 64 * 1024];
            loop {
                let count = raw.read(&mut buffer).map_err(StorageError::from)?;
                if count == 0 {
                    break;
                }
                digest.update(&buffer[..count]);
            }
            digest.update(json.as_bytes());
            metadata.evidence = Some(CaptureEvidenceManifest::new(
                metadata.sha256.clone(),
                metadata.bytes,
                &evidence,
            )?);
            metadata.sha256 = hex::encode(digest.finalize());
            metadata.bytes = metadata
                .bytes
                .checked_add(json.len() as u64)
                .ok_or(CaptureSubmissionError::InvalidCapture)?;
            metadata
                .validate()
                .map_err(|_| CaptureSubmissionError::InvalidCapture)?;
        }
        let mut categories = summary.categories;
        categories.notes |= !evidence.note.is_empty();
        let status = CaptureSubmissionStatus {
            operation_id: Uuid::new_v4().to_string(),
            metadata,
            categories,
            phase: CaptureSubmissionPhase::Prepared,
            local_payload_state: CaptureSubmissionLocalPayloadState::Retained,
            account_id: None,
            receipt: None,
            failure: None,
            revision: 0,
        };
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::Insert {
                status: Box::new(status),
                evidence_json,
                reply,
            })
        })
        .map_err(Into::into)
    }

    fn export_submission_source(
        &self,
        record: SourceRecord,
        snapshot: &std::path::Path,
    ) -> Result<SnapshotSource, CaptureSubmissionError> {
        match record {
            SourceRecord::Live(entry) => {
                if entry.state != LiveCaptureState::Finished {
                    return Err(CaptureSubmissionError::NotFinished);
                }
                let integrity = match entry.integrity {
                    LiveCaptureIntegrity::Complete => CaptureSubmissionIntegrity::Complete,
                    LiveCaptureIntegrity::Incomplete { dropped_messages } => {
                        CaptureSubmissionIntegrity::Incomplete { dropped_messages }
                    }
                    LiveCaptureIntegrity::Unknown => {
                        return Err(CaptureSubmissionError::NotFinished);
                    }
                };
                let recording = entry
                    .recording
                    .ok_or(CaptureSubmissionError::ProvenanceUnavailable)?;
                self.export_live_capture(entry.id, snapshot)
                    .map_err(|_| CaptureSubmissionError::InvalidCapture)?;
                Ok(SnapshotSource {
                    origin: recording.origin,
                    integrity,
                    provenance: CaptureSubmissionProvenance {
                        source_artifact_id: recording.artifact_id.to_string(),
                        live_capture_id: Some(entry.id.to_string()),
                        source_sha256: None,
                        transformation:
                            CaptureSubmissionTransformation::DatabaseExportMusicExcluded,
                    },
                    encoding: PevcapEncoding::Jsonl,
                })
            }
            SourceRecord::File(receipt) => {
                let recording = receipt
                    .recording
                    .ok_or(CaptureSubmissionError::ProvenanceUnavailable)?;
                let temporary = tempfile::tempdir().map_err(StorageError::from)?;
                let original = temporary.path().join("original");
                self.export_pevcap_capture(&receipt.artifact_digest, &original)
                    .map_err(|_| CaptureSubmissionError::InvalidCapture)?;
                let mut prefix = [0; 8];
                let count = std::fs::File::open(&original)
                    .and_then(|mut input| input.read(&mut prefix))
                    .map_err(StorageError::from)?;
                let encoding = if count == 8 && prefix == cutout_core::PEVCAP_MAGIC {
                    PevcapEncoding::Binary
                } else {
                    PevcapEncoding::Jsonl
                };
                std::fs::copy(&original, snapshot).map_err(StorageError::from)?;
                let transformation = CaptureSubmissionTransformation::Unmodified;
                Ok(SnapshotSource {
                    origin: recording.origin,
                    integrity: CaptureSubmissionIntegrity::Complete,
                    provenance: CaptureSubmissionProvenance {
                        source_artifact_id: recording.artifact_id.to_string(),
                        live_capture_id: None,
                        source_sha256: Some(receipt.artifact_digest),
                        transformation,
                    },
                    encoding,
                })
            }
        }
    }

    fn submission_action(
        &self,
        operation_id: &str,
        action: Action,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::Apply {
                operation_id: operation_id.to_owned(),
                action,
                reply,
            })
        })?
    }

    fn submission_operation(
        &self,
        operation_id: &str,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::LookupOperation {
                operation_id: operation_id.to_owned(),
                reply,
            })
        })?
        .ok_or(CaptureSubmissionError::NotFound)
    }

    fn submission_payload(
        &self,
        status: &CaptureSubmissionStatus,
    ) -> Result<CaptureSubmissionPayload, CaptureSubmissionError> {
        use sha2::Digest;
        let evidence = self.request(|reply| {
            super::Command::CaptureSubmission(Command::SnapshotEvidence {
                operation_id: status.operation_id.clone(),
                reply,
            })
        })?;
        Ok(CaptureSubmissionPayload {
            database: self.clone(),
            digest: status.metadata.capture_sha256().to_owned(),
            bytes: status.metadata.bytes,
            sequence: 0,
            current: Cursor::new(Vec::new()),
            read_bytes: 0,
            digest_state: sha2::Sha256::new(),
            complete: false,
            raw_complete: false,
            expected_digest: status.metadata.sha256.clone(),
            evidence: Cursor::new(evidence),
            progress: None,
        })
    }

    /// Runs only the selected user-authorized transfer. Every retry first recovers a lost receipt.
    /// No pending operation is selected automatically.
    /// # Errors
    /// Returns incompatible operation/account or storage failures; transport failures persist in status.
    pub async fn send_capture_submission<T: CaptureSubmissionTransport>(
        &self,
        operation_id: &str,
        account_id: &str,
        token: &str,
        transport: &T,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        self.send_capture_submission_observed(operation_id, account_id, token, transport, None)
            .await
    }

    async fn send_capture_submission_observed<T: CaptureSubmissionTransport>(
        &self,
        operation_id: &str,
        account_id: &str,
        token: &str,
        transport: &T,
        progress: Option<CaptureSubmissionProgressCallback>,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        let status = self.submission_action(
            operation_id,
            Action::Start {
                account_id: account_id.to_owned(),
            },
        )?;
        if status.phase == CaptureSubmissionPhase::Sent {
            return Ok(status);
        }
        let progress = progress.map(|callback| CaptureSubmissionProgressReporter {
            database: self.clone(),
            operation_id: status.operation_id.clone(),
            attempt_id: status.revision,
            total_bytes: status.metadata.bytes,
            callback,
            active: Arc::new(AtomicBool::new(true)),
        });
        let _progress_guard = CaptureSubmissionProgressGuard(progress.clone());
        if let Some(progress) = &progress {
            progress.report(0);
        }
        let result = match transport.receipt(operation_id, token).await {
            Ok(Some(receipt)) => Ok(receipt),
            Ok(None) => {
                let current = self.submission_operation(operation_id)?;
                if current.revision != status.revision
                    || current.phase != CaptureSubmissionPhase::Sending
                {
                    return Ok(current);
                }
                let mut payload = self.submission_payload(&status)?;
                payload.progress = progress;
                transport
                    .upload(operation_id, token, &status.metadata, payload)
                    .await
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(receipt) if receipt.validate_for(operation_id, &status.metadata).is_ok() => self
                .submission_action(
                    operation_id,
                    Action::Complete {
                        revision: status.revision,
                        receipt,
                    },
                ),
            Ok(_) => self.submission_action(
                operation_id,
                Action::Failure {
                    revision: status.revision,
                    failure: CaptureSubmissionTransferError::Conflict,
                },
            ),
            Err(failure) => self.submission_action(
                operation_id,
                Action::Failure {
                    revision: status.revision,
                    failure,
                },
            ),
        }
    }

    /// Runs an explicit transfer with transient progress for this attempt only.
    /// # Errors
    /// Returns the same admission, storage and transport results as ordinary Send.
    pub async fn send_capture_submission_with_progress<T: CaptureSubmissionTransport>(
        &self,
        operation_id: &str,
        account_id: &str,
        token: &str,
        transport: &T,
        progress: CaptureSubmissionProgressCallback,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        self.send_capture_submission_observed(
            operation_id,
            account_id,
            token,
            transport,
            Some(progress),
        )
        .await
    }

    /// Cancels an explicit Send and confirms a service tombstone when transmission began.
    /// # Errors
    /// Returns account/state or storage failures; unconfirmed cancellation remains pending.
    pub async fn cancel_capture_submission<T: CaptureSubmissionTransport>(
        &self,
        operation_id: &str,
        credential: Option<CaptureSubmissionCredential<'_>>,
        transport: &T,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        self.remove_capture_submission(operation_id, credential, transport, false)
            .await
    }

    /// Deletes only this remote submission; the private local recording is retained.
    /// # Errors
    /// Returns account/state or storage failures; unconfirmed deletion remains pending.
    pub async fn delete_capture_submission<T: CaptureSubmissionTransport>(
        &self,
        operation_id: &str,
        account_id: &str,
        token: &str,
        transport: &T,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        self.remove_capture_submission(
            operation_id,
            Some(CaptureSubmissionCredential { account_id, token }),
            transport,
            true,
        )
        .await
    }

    async fn remove_capture_submission<T: CaptureSubmissionTransport>(
        &self,
        operation_id: &str,
        credential: Option<CaptureSubmissionCredential<'_>>,
        transport: &T,
        deleting: bool,
    ) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
        let status = self.submission_action(
            operation_id,
            Action::Removal {
                account_id: credential.map(|credential| credential.account_id.to_owned()),
                deleting,
            },
        )?;
        if let CaptureSubmissionPhase::Cancelled | CaptureSubmissionPhase::Deleted = status.phase {
            return Ok(status);
        }
        let credential = credential.ok_or(CaptureSubmissionError::WrongAccount)?;
        match transport.delete(operation_id, credential.token).await {
            Ok(()) => self.submission_action(
                operation_id,
                Action::Removed {
                    revision: status.revision,
                },
            ),
            Err(failure) => self.submission_action(
                operation_id,
                Action::Failure {
                    revision: status.revision,
                    failure,
                },
            ),
        }
    }
}

fn source_record(
    connection: &Connection,
    source: CaptureSubmissionSource,
) -> Result<Option<SourceRecord>, StorageError> {
    let (live, artifact) = match source {
        CaptureSubmissionSource::LiveCapture(id) => (Some(id), None),
        CaptureSubmissionSource::RecordedArtifact(id) => (None, Some(id)),
    };
    if let Some(entry) = super::live_capture_history::lookup(connection, live, artifact)? {
        return Ok(Some(SourceRecord::Live(entry)));
    }
    match artifact {
        Some(id) => super::recorded_capture::receipt_for_id(connection, id)
            .map(|receipt| receipt.map(SourceRecord::File)),
        None => Ok(None),
    }
}

fn invalid_status() -> StorageError {
    StorageError::InvalidStoredValue {
        field: "capture submission",
        value: "invalid persisted state".to_owned(),
    }
}

fn decode_status(json: &str) -> Result<CaptureSubmissionStatus, StorageError> {
    let status: CaptureSubmissionStatus =
        serde_json::from_str(json).map_err(|_| invalid_status())?;
    status.metadata.validate().map_err(|_| invalid_status())?;
    Uuid::parse_str(&status.operation_id).map_err(|_| invalid_status())?;
    if let Some(receipt) = &status.receipt {
        receipt
            .validate_for(&status.operation_id, &status.metadata)
            .map_err(|_| invalid_status())?;
    }
    Ok(status)
}

fn lookup_status(
    connection: &Connection,
    artifact: Option<&str>,
    operation: Option<&str>,
) -> Result<Option<CaptureSubmissionStatus>, StorageError> {
    let json: Option<String> = connection.query_row(
        "SELECT status_json FROM capture_submissions WHERE source_artifact_id = ?1 OR operation_id = ?2",
        params![artifact, operation], |row| row.get(0),
    ).optional()?;
    json.map(|json| decode_status(&json)).transpose()
}

fn persist_status(
    connection: &Connection,
    status: &CaptureSubmissionStatus,
) -> Result<(), StorageError> {
    let json = serde_json::to_string(status).map_err(|_| invalid_status())?;
    connection.execute(
        "UPDATE capture_submissions SET status_json = ?2 WHERE operation_id = ?1",
        params![status.operation_id, json],
    )?;
    Ok(())
}

fn insert_status(
    connection: &Connection,
    status: &CaptureSubmissionStatus,
    evidence_json: Option<&str>,
) -> Result<CaptureSubmissionStatus, StorageError> {
    if let Some(existing) = lookup_status(
        connection,
        Some(&status.metadata.provenance.source_artifact_id),
        None,
    )? {
        return Ok(existing);
    }
    let json = serde_json::to_string(status).map_err(|_| invalid_status())?;
    let current: Option<String> = connection
        .query_row(
            "SELECT evidence_json FROM capture_evidence WHERE source_artifact_id=?1",
            [&status.metadata.provenance.source_artifact_id],
            |row| row.get(0),
        )
        .optional()?;
    let current_evidence: CaptureAttachments = current
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| invalid_status())?
        .unwrap_or_default();
    let expected: CaptureAttachments = evidence_json
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| invalid_status())?
        .unwrap_or_default();
    if current_evidence != expected {
        return Err(invalid_status());
    }
    connection.execute("INSERT INTO capture_submissions (source_artifact_id, operation_id, snapshot_digest, status_json, evidence_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![status.metadata.provenance.source_artifact_id, status.operation_id, status.metadata.capture_sha256(), json, evidence_json])?;
    Ok(status.clone())
}

fn apply(
    connection: &Connection,
    operation_id: &str,
    action: Action,
) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
    let mut status = lookup_status(connection, None, Some(operation_id))?
        .ok_or(CaptureSubmissionError::NotFound)?;
    match action {
        Action::Start { account_id } => {
            if !start(connection, &mut status, account_id)? {
                return Ok(status);
            }
        }
        Action::Removal {
            account_id,
            deleting,
        } => {
            if !remove(&mut status, account_id.as_deref(), deleting)? {
                return Ok(status);
            }
        }
        Action::Complete { revision, receipt } => {
            if revision != status.revision || status.phase != CaptureSubmissionPhase::Sending {
                return Ok(status);
            }
            receipt
                .validate_for(operation_id, &status.metadata)
                .map_err(|_| CaptureSubmissionError::InvalidCapture)?;
            status.receipt = Some(receipt);
            status.phase = CaptureSubmissionPhase::Sent;
            status.local_payload_state = CaptureSubmissionLocalPayloadState::RemovalPending;
            status.failure = None;
        }
        Action::Failure { revision, failure } => {
            if revision != status.revision {
                return Ok(status);
            }
            if status.phase == CaptureSubmissionPhase::Sending {
                status.phase = match failure {
                    CaptureSubmissionTransferError::Cancelled => CaptureSubmissionPhase::Cancelled,
                    CaptureSubmissionTransferError::Conflict
                    | CaptureSubmissionTransferError::Rejected
                    | CaptureSubmissionTransferError::Forbidden
                    | CaptureSubmissionTransferError::TooLarge
                    | CaptureSubmissionTransferError::InvalidCapture
                    | CaptureSubmissionTransferError::MusicPresent => {
                        CaptureSubmissionPhase::Rejected
                    }
                    _ => CaptureSubmissionPhase::Retryable,
                };
            }
            status.failure = Some(failure);
        }
        Action::Removed { revision } => {
            if revision != status.revision {
                return Ok(status);
            }
            status.phase = match status.phase {
                CaptureSubmissionPhase::CancellationPending => CaptureSubmissionPhase::Cancelled,
                CaptureSubmissionPhase::DeletionPending => CaptureSubmissionPhase::Deleted,
                _ => return Ok(status),
            };
            status.failure = None;
        }
    }
    status.revision = status
        .revision
        .checked_add(1)
        .ok_or(CaptureSubmissionError::InvalidState)?;
    persist_status(connection, &status)?;
    Ok(status)
}

fn start(
    connection: &Connection,
    status: &mut CaptureSubmissionStatus,
    account_id: String,
) -> Result<bool, CaptureSubmissionError> {
    if Uuid::parse_str(&account_id).is_err()
        || status
            .account_id
            .as_ref()
            .is_some_and(|owner| owner != &account_id)
    {
        return Err(CaptureSubmissionError::WrongAccount);
    }
    let account_state: String = connection
        .query_row(
            "SELECT account_state FROM charging_sync_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::from)?;
    if account_state != "active" {
        return Err(CaptureSubmissionError::WrongAccount);
    }
    match status.phase {
        CaptureSubmissionPhase::Sent => return Ok(false),
        CaptureSubmissionPhase::Prepared | CaptureSubmissionPhase::Retryable => {}
        _ => return Err(CaptureSubmissionError::InvalidState),
    }
    if status.local_payload_state != CaptureSubmissionLocalPayloadState::Retained {
        return Err(CaptureSubmissionError::InvalidState);
    }
    status.account_id = Some(account_id);
    status.phase = CaptureSubmissionPhase::Sending;
    status.failure = None;
    Ok(true)
}

fn remove(
    status: &mut CaptureSubmissionStatus,
    account_id: Option<&str>,
    deleting: bool,
) -> Result<bool, CaptureSubmissionError> {
    if (deleting && account_id.is_none())
        || status
            .account_id
            .as_deref()
            .is_some_and(|owner| Some(owner) != account_id)
    {
        return Err(CaptureSubmissionError::WrongAccount);
    }
    if deleting && status.account_id.is_none() {
        return Err(CaptureSubmissionError::InvalidState);
    }
    if let CaptureSubmissionPhase::Deleted | CaptureSubmissionPhase::Cancelled = status.phase {
        return Ok(false);
    }
    status.phase = if status.account_id.is_none() {
        CaptureSubmissionPhase::Cancelled
    } else if deleting {
        CaptureSubmissionPhase::DeletionPending
    } else {
        CaptureSubmissionPhase::CancellationPending
    };
    status.failure = None;
    Ok(true)
}

pub(super) fn recover(connection: &Connection) -> Result<(), StorageError> {
    connection.execute("UPDATE capture_submissions SET status_json = json_set(status_json,
        '$.phase', 'retryable', '$.failure', 'network', '$.revision', json_extract(status_json, '$.revision') + 1)
        WHERE json_extract(status_json, '$.phase') = 'sending'", [])?;
    Ok(())
}

/// Scans one bounded status page and removes at most one receipt-authorized payload batch.
///
/// The cursor belongs to the worker. It advances over skipped rows and completed
/// operations, stays before a partially removed operation, and resets at end-of-scan.
#[allow(
    clippy::too_many_lines,
    reason = "receipt validation, bounded deletion and its progress commit share one transaction"
)]
pub(super) fn cleanup_sent_capture_batch(
    connection: &mut Connection,
    cursor: &mut Option<String>,
) -> Result<super::normalization::BatchState, StorageError> {
    use CaptureSubmissionLocalPayloadState as PayloadState;
    use CaptureSubmissionPhase as Phase;

    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let candidates = {
        let mut statement = transaction.prepare(
            "SELECT operation_id, status_json FROM capture_submissions
             WHERE operation_id > ?1
               AND json_extract(status_json, '$.phase')
                    IN ('sent', 'deleted', 'cancellation_pending', 'cancelled', 'deletion_pending')
               AND coalesce(json_extract(status_json, '$.local_payload_state'), 'retained')
                    IN ('retained', 'removal_pending')
             ORDER BY operation_id LIMIT 64",
        )?;
        let rows = statement.query_map([cursor.as_deref().unwrap_or("")], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let page_is_full = candidates.len() == 64;
    let mut scanned_through = cursor.clone();
    let mut selected = None;
    for (operation_id, json) in &candidates {
        let before_candidate = scanned_through.clone();
        scanned_through = Some(operation_id.clone());
        let Ok(mut status) = decode_status(json) else {
            continue;
        };
        if status.operation_id.as_str() != operation_id.as_str()
            || (status.phase != Phase::Sent
                && status.phase != Phase::Deleted
                && status.phase != Phase::CancellationPending
                && status.phase != Phase::Cancelled
                && status.phase != Phase::DeletionPending)
            || status.receipt.is_none()
            || status.local_payload_state == PayloadState::Removed
        {
            continue;
        }
        if !submission_payload_is_exclusive(&transaction, &status)? {
            continue;
        }
        if status.local_payload_state == PayloadState::Retained {
            status.local_payload_state = PayloadState::RemovalPending;
            persist_status(&transaction, &status)?;
        }
        let mut has_payload = false;
        let mut removable_payload = false;
        if let Some(live_id) = status.metadata.provenance.live_capture_id.as_deref() {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM live_capture_events WHERE capture_id = ?1)",
                [live_id],
                |row| row.get(0),
            )?;
            has_payload |= exists;
            removable_payload |= exists
                && no_unfinished_submission_references(
                    &transaction,
                    &status.operation_id,
                    None,
                    Some(live_id),
                )?;
        }
        for digest in cleanup_digests(&transaction, &status)? {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pevcap_capture_chunks WHERE artifact_digest = ?1)",
                [&digest],
                |row| row.get(0),
            )?;
            has_payload |= exists;
            removable_payload |= exists
                && no_unfinished_submission_references(
                    &transaction,
                    &status.operation_id,
                    Some(&digest),
                    None,
                )?;
        }
        if has_payload && !removable_payload {
            continue;
        }
        selected = Some((status, before_candidate));
        break;
    }
    let Some((mut status, cursor_before_selected)) = selected else {
        transaction.commit()?;
        *cursor = if page_is_full { scanned_through } else { None };
        return Ok(if page_is_full {
            super::normalization::BatchState::Pending
        } else {
            super::normalization::BatchState::Finished
        });
    };

    // Evidence attachments are payload too. Keep their manifest and digest in the
    // durable status, but clear both stored JSON copies once the validated receipt
    // and exclusive source ownership authorize cleanup.
    transaction.execute(
        "UPDATE capture_submissions SET evidence_json = NULL WHERE operation_id = ?1",
        [&status.operation_id],
    )?;
    transaction.execute(
        "DELETE FROM capture_evidence WHERE source_artifact_id = ?1",
        [&status.metadata.provenance.source_artifact_id],
    )?;

    let live_id = status.metadata.provenance.live_capture_id.as_deref();
    let mut payload_removed = false;
    if let Some(live_id) = live_id
        && no_unfinished_submission_references(
            &transaction,
            &status.operation_id,
            None,
            Some(live_id),
        )?
    {
        let removed = transaction.execute(
            "DELETE FROM live_capture_events
                 WHERE capture_id = ?1 AND sequence IN (
                    SELECT sequence FROM live_capture_events WHERE capture_id = ?1
                    ORDER BY sequence LIMIT 64
                 )",
            [live_id],
        )?;
        payload_removed |= removed != 0;
    }

    let digests = cleanup_digests(&transaction, &status)?;
    let mut remaining_chunk_budget = 64_i64;
    for digest in &digests {
        if remaining_chunk_budget == 0 {
            break;
        }
        if no_unfinished_submission_references(
            &transaction,
            &status.operation_id,
            Some(digest),
            None,
        )? {
            let removed = transaction.execute(
                "DELETE FROM pevcap_capture_chunks
                 WHERE artifact_digest = ?1 AND sequence IN (
                    SELECT sequence FROM pevcap_capture_chunks
                    WHERE artifact_digest = ?1 ORDER BY sequence LIMIT ?2
                 )",
                params![digest, remaining_chunk_budget],
            )?;
            payload_removed |= removed != 0;
            remaining_chunk_budget -= i64::try_from(removed).map_err(|_| invalid_status())?;
        }
    }

    let live_payload_remains = if let Some(live_id) = live_id {
        transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM live_capture_events WHERE capture_id = ?1)",
            [live_id],
            |row| row.get::<_, bool>(0),
        )?
    } else {
        false
    };
    let mut archive_payload_remains = false;
    for digest in &digests {
        let exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pevcap_capture_chunks WHERE artifact_digest = ?1)",
            [digest],
            |row| row.get::<_, bool>(0),
        )?;
        archive_payload_remains |= exists;
        if !exists
            && no_unfinished_submission_references(
                &transaction,
                &status.operation_id,
                Some(digest),
                None,
            )?
        {
            let removed = transaction.execute(
                "DELETE FROM pevcap_captures WHERE artifact_digest = ?1",
                [digest],
            )?;
            payload_removed |= removed != 0;
        }
    }
    if !live_payload_remains && !archive_payload_remains {
        status.local_payload_state = PayloadState::Removed;
        // `revision` fences an in-flight Send/Delete transport action. Local
        // maintenance must not invalidate a remote delete already in progress.
        persist_status(&transaction, &status)?;
    }
    if payload_removed {
        transaction.execute(
            "UPDATE storage_normalization_progress
             SET compaction_pending = 1, compaction_complete = 0 WHERE singleton = 1",
            [],
        )?;
    }
    transaction.commit()?;
    *cursor = if !live_payload_remains && !archive_payload_remains {
        Some(status.operation_id)
    } else {
        cursor_before_selected
    };
    Ok(super::normalization::BatchState::Pending)
}

fn managed_file_removal_authorized(
    connection: &Connection,
    digest: &str,
    size: u64,
) -> Result<bool, StorageError> {
    let file_size = size;
    let size = i64::try_from(file_size).map_err(|_| invalid_status())?;
    let mut cursor = String::new();
    loop {
        let candidates = {
            let mut statement = connection.prepare(
                "SELECT operation_id, status_json FROM capture_submissions
                 WHERE operation_id > ?1
                   AND json_extract(status_json, '$.phase') IN
                       ('sent', 'deleted', 'cancellation_pending', 'cancelled', 'deletion_pending')
                   AND coalesce(json_extract(status_json, '$.local_payload_state'), 'retained')
                       IN ('retained', 'removal_pending', 'removed')
                   AND (json_extract(status_json, '$.metadata.sha256') = ?2
                        OR json_extract(status_json, '$.metadata.provenance.source_sha256') = ?2)
                   AND json_extract(status_json, '$.receipt.sha256') =
                       json_extract(status_json, '$.metadata.sha256')
                   AND json_extract(status_json, '$.receipt.bytes') =
                       json_extract(status_json, '$.metadata.bytes')
                 ORDER BY operation_id LIMIT 64",
            )?;
            let rows = statement.query_map(params![cursor, digest], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let page_is_full = candidates.len() == 64;
        for (operation_id, json) in &candidates {
            cursor.clone_from(operation_id);
            let Ok(status) = decode_status(json) else {
                continue;
            };
            let has_confirmed_receipt_phase = match status.phase {
                CaptureSubmissionPhase::Sent
                | CaptureSubmissionPhase::Deleted
                | CaptureSubmissionPhase::CancellationPending
                | CaptureSubmissionPhase::Cancelled
                | CaptureSubmissionPhase::DeletionPending => true,
                CaptureSubmissionPhase::Prepared
                | CaptureSubmissionPhase::Sending
                | CaptureSubmissionPhase::Retryable
                | CaptureSubmissionPhase::Rejected => false,
            };
            if status.operation_id.as_str() != operation_id.as_str() || !has_confirmed_receipt_phase
            {
                continue;
            }
            let Some(receipt) = status.receipt.as_ref() else {
                continue;
            };
            if receipt
                .validate_for(&status.operation_id, &status.metadata)
                .is_err()
            {
                continue;
            }
            let snapshot_match =
                status.metadata.sha256 == digest && status.metadata.bytes == file_size;
            let source_match = status.metadata.provenance.source_sha256.as_deref() == Some(digest)
                && source_artifact_owns_file(connection, &status, digest, size)?;
            if (snapshot_match || source_match)
                && submission_payload_is_exclusive(connection, &status)?
            {
                return Ok(true);
            }
        }
        if !page_is_full {
            return Ok(false);
        }
    }
}

fn source_artifact_owns_file(
    connection: &Connection,
    status: &CaptureSubmissionStatus,
    digest: &str,
    size: i64,
) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM pevcap_recordings AS recording
            JOIN pevcap_imports AS imports USING (artifact_digest)
            WHERE recording.artifact_id = ?1 AND recording.artifact_digest = ?2
              AND imports.artifact_size = ?3
         )",
        params![
            status.metadata.provenance.source_artifact_id.as_str(),
            digest,
            size
        ],
        |row| row.get(0),
    )?)
}

fn submission_payload_is_exclusive(
    connection: &Connection,
    status: &CaptureSubmissionStatus,
) -> Result<bool, StorageError> {
    if let Some(live_id) = status.metadata.provenance.live_capture_id.as_deref() {
        let active_capture: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM live_capture_sessions WHERE capture_id = ?1 AND state = 'active'
             )",
            [live_id],
            |row| row.get(0),
        )?;
        if active_capture
            || !no_unfinished_submission_references(
                connection,
                &status.operation_id,
                None,
                Some(live_id),
            )?
        {
            return Ok(false);
        }
    }
    for digest in cleanup_digests(connection, status)? {
        if !no_unfinished_submission_references(
            connection,
            &status.operation_id,
            Some(&digest),
            None,
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn cleanup_digests(
    connection: &Connection,
    status: &CaptureSubmissionStatus,
) -> Result<Vec<String>, StorageError> {
    let mut digests = vec![status.metadata.capture_sha256().to_owned()];
    if let Some(source_digest) = status.metadata.provenance.source_sha256.as_deref()
        && source_digest != digests[0]
    {
        digests.push(source_digest.to_owned());
    }
    let recorded_digest: Option<String> = connection
        .query_row(
            "SELECT artifact_digest FROM pevcap_recordings WHERE artifact_id = ?1",
            [&status.metadata.provenance.source_artifact_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(source_digest) = recorded_digest
        && !digests.contains(&source_digest)
    {
        digests.push(source_digest);
    }
    Ok(digests)
}

fn no_unfinished_submission_references(
    connection: &Connection,
    own_operation_id: &str,
    digest: Option<&str>,
    live_capture_id: Option<&str>,
) -> Result<bool, StorageError> {
    if let Some(digest) = digest {
        let import_work_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pevcap_import_work WHERE artifact_digest = ?1)",
            [digest],
            |row| row.get(0),
        )?;
        if import_work_exists {
            return Ok(false);
        }
    }
    let mut statement = connection.prepare(
        "SELECT operation_id, status_json FROM capture_submissions
         WHERE operation_id != ?1 AND (
            (?2 IS NOT NULL AND (snapshot_digest = ?2 OR
                json_extract(status_json, '$.metadata.provenance.source_sha256') = ?2 OR
                json_extract(status_json, '$.metadata.provenance.source_artifact_id') IN
                    (SELECT artifact_id FROM pevcap_recordings WHERE artifact_digest = ?2) OR
                json_extract(status_json, '$.metadata.provenance.live_capture_id') IN
                    (SELECT capture_id FROM live_capture_sessions WHERE
                     json_extract(recording_context_json, '$.artifact_id') IN
                        (SELECT artifact_id FROM pevcap_recordings WHERE artifact_digest = ?2))))
            OR (?3 IS NOT NULL AND
                json_extract(status_json, '$.metadata.provenance.live_capture_id') = ?3)
         )",
    )?;
    let rows = statement.query_map(params![own_operation_id, digest, live_capture_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (_, json) = row?;
        let Ok(status) = decode_status(&json) else {
            return Ok(false);
        };
        if (status.phase != CaptureSubmissionPhase::Sent
            && status.phase != CaptureSubmissionPhase::Deleted
            && status.phase != CaptureSubmissionPhase::CancellationPending
            && status.phase != CaptureSubmissionPhase::Cancelled
            && status.phase != CaptureSubmissionPhase::DeletionPending)
            || status.receipt.is_none()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn dispatch(connection: &mut Connection, command: Command) {
    match command {
        Command::Evidence {
            source,
            save,
            reply,
        } => {
            let _ = reply.send(Ok(evidence_command(connection, source, save)));
        }
        Command::SnapshotEvidence {
            operation_id,
            reply,
        } => {
            let result = connection.query_row("SELECT coalesce(evidence_json, '') FROM capture_submissions WHERE operation_id=?1", [operation_id], |row| row.get::<_, String>(0)).map(String::into_bytes).map_err(Into::into);
            let _ = reply.send(result);
        }
        Command::Source { source, reply } => {
            let _ = reply.send(source_record(connection, source));
        }
        Command::Lookup { artifact_id, reply } => {
            let _ = reply.send(lookup_status(connection, Some(&artifact_id), None));
        }
        Command::LookupOperation {
            operation_id,
            reply,
        } => {
            let _ = reply.send(lookup_status(connection, None, Some(&operation_id)));
        }
        Command::ManagedFileRemovalAuthorization {
            digest,
            size,
            reply,
        } => {
            let _ = reply.send(managed_file_removal_authorized(connection, &digest, size));
        }
        Command::Insert {
            status,
            evidence_json,
            reply,
        } => {
            let _ = reply.send(insert_status(connection, &status, evidence_json.as_deref()));
        }
        Command::Apply {
            operation_id,
            action,
            reply,
        } => {
            let _ = reply.send(Ok(apply(connection, &operation_id, action)));
        }
    }
}

#[cfg(test)]
mod tests;

fn evidence_command(
    connection: &mut Connection,
    source: CaptureSubmissionSource,
    save: Option<CaptureAttachments>,
) -> Result<CaptureAttachments, CaptureSubmissionError> {
    let record = source_record(connection, source)?.ok_or(CaptureSubmissionError::NotFound)?;
    let artifact = match record {
        SourceRecord::Live(entry) => entry.recording.map(|r| r.artifact_id),
        SourceRecord::File(entry) => entry.recording.map(|r| r.artifact_id),
    }
    .ok_or(CaptureSubmissionError::ProvenanceUnavailable)?
    .to_string();
    let transaction = connection.transaction().map_err(StorageError::from)?;
    if let Some(evidence) = save {
        evidence.validate()?;
        if let Some(status) = lookup_status(&transaction, Some(&artifact), None)? {
            match status.phase {
                CaptureSubmissionPhase::Prepared
                | CaptureSubmissionPhase::Cancelled
                | CaptureSubmissionPhase::Deleted => {
                    transaction
                        .execute(
                            "DELETE FROM capture_submissions WHERE source_artifact_id=?1",
                            [&artifact],
                        )
                        .map_err(StorageError::from)?;
                }
                _ => return Err(CaptureSubmissionError::InvalidState),
            }
        }
        let json = serde_json::to_string(&evidence).map_err(|_| invalid_status())?;
        transaction.execute("INSERT INTO capture_evidence(source_artifact_id,evidence_json) VALUES(?1,?2) ON CONFLICT(source_artifact_id) DO UPDATE SET evidence_json=excluded.evidence_json", params![artifact,json]).map_err(StorageError::from)?;
        transaction.commit().map_err(StorageError::from)?;
        return Ok(evidence);
    }
    let json: Option<String> = transaction
        .query_row(
            "SELECT evidence_json FROM capture_submissions WHERE source_artifact_id=?1",
            [&artifact],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)?
        .flatten();
    let json = match json {
        Some(json) => Some(json),
        None => transaction
            .query_row(
                "SELECT evidence_json FROM capture_evidence WHERE source_artifact_id=?1",
                [&artifact],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)?,
    };
    let evidence: CaptureAttachments = json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| invalid_status())?
        .unwrap_or_default();
    evidence.validate()?;
    Ok(evidence)
}
