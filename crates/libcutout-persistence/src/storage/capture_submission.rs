//! Explicit private submission of an immutable, durably completed Bluetooth capture.

use super::{
    CapturePublication, LiveCaptureHistoryEntry, LiveCaptureId, LiveCaptureIntegrity,
    LiveCaptureState, PevcapImportReceipt, Reply, RideDatabase, StorageError,
};
use crate::CaptureArtifactId;
use cutout_core::{
    CaptureSubmissionCategories, CaptureSubmissionIntegrity, CaptureSubmissionMetadata,
    CaptureSubmissionProvenance, CaptureSubmissionReceipt, CaptureSubmissionTransformation,
    PevcapEncoding, validate_capture_submission,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    io::{self, Cursor, Read},
    pin::Pin,
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
    /// Remote deletion completed; the original local recording remains.
    Deleted,
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
    /// Credential identity bound by the first explicit Send.
    pub account_id: Option<String>,
    /// Receipt of durable remote completion, if confirmed.
    pub receipt: Option<CaptureSubmissionReceipt>,
    /// Classified last failure, without payload, notes or credentials.
    pub failure: Option<CaptureSubmissionTransferError>,
    revision: u64,
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
                if self.read_bytes != self.bytes
                    || hex::encode(self.digest_state.clone().finalize()) != self.digest
                {
                    return Err(io::Error::other("capture snapshot integrity failed"));
                }
                self.complete = true;
                return Ok(0);
            }
        }
    }
}

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS capture_submissions (
    source_artifact_id TEXT PRIMARY KEY NOT NULL CHECK(length(source_artifact_id) = 36),
    operation_id TEXT NOT NULL UNIQUE CHECK(length(operation_id) = 36),
    snapshot_digest TEXT NOT NULL REFERENCES pevcap_imports(artifact_digest),
    status_json TEXT NOT NULL CHECK(json_valid(status_json))
        CHECK(json_extract(status_json, '$.operation_id') = operation_id)
        CHECK(json_extract(status_json, '$.metadata.sha256') = snapshot_digest)
        CHECK(json_extract(status_json, '$.metadata.provenance.source_artifact_id') = source_artifact_id)
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
    Insert {
        status: Box<CaptureSubmissionStatus>,
        reply: Reply<CaptureSubmissionStatus>,
    },
    Apply {
        operation_id: String,
        action: Action,
        reply: Reply<Result<CaptureSubmissionStatus, CaptureSubmissionError>>,
    },
}

impl RideDatabase {
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
        let metadata = CaptureSubmissionMetadata {
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
        let status = CaptureSubmissionStatus {
            operation_id: Uuid::new_v4().to_string(),
            metadata,
            categories: summary.categories,
            phase: CaptureSubmissionPhase::Prepared,
            account_id: None,
            receipt: None,
            failure: None,
            revision: 0,
        };
        self.request(|reply| {
            super::Command::CaptureSubmission(Command::Insert {
                status: Box::new(status),
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

    fn submission_payload(&self, status: &CaptureSubmissionStatus) -> CaptureSubmissionPayload {
        use sha2::Digest;
        CaptureSubmissionPayload {
            database: self.clone(),
            digest: status.metadata.sha256.clone(),
            bytes: status.metadata.bytes,
            sequence: 0,
            current: Cursor::new(Vec::new()),
            read_bytes: 0,
            digest_state: sha2::Sha256::new(),
            complete: false,
        }
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
        let status = self.submission_action(
            operation_id,
            Action::Start {
                account_id: account_id.to_owned(),
            },
        )?;
        if status.phase == CaptureSubmissionPhase::Sent {
            return Ok(status);
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
                transport
                    .upload(
                        operation_id,
                        token,
                        &status.metadata,
                        self.submission_payload(&status),
                    )
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
) -> Result<CaptureSubmissionStatus, StorageError> {
    if let Some(existing) = lookup_status(
        connection,
        Some(&status.metadata.provenance.source_artifact_id),
        None,
    )? {
        return Ok(existing);
    }
    let json = serde_json::to_string(status).map_err(|_| invalid_status())?;
    connection.execute("INSERT INTO capture_submissions (source_artifact_id, operation_id, snapshot_digest, status_json) VALUES (?1, ?2, ?3, ?4)",
        params![status.metadata.provenance.source_artifact_id, status.operation_id, status.metadata.sha256, json])?;
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

pub(super) fn dispatch(connection: &mut Connection, command: Command) {
    match command {
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
        Command::Insert { status, reply } => {
            let _ = reply.send(insert_status(connection, &status));
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
