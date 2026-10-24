//! Shared, user-confirmed bulk capture submission.

use super::super::{PevcapCaptureCursor, QueryLimit, StoredPevcapCapture, normalization};
use super::{
    CaptureSubmissionError, CaptureSubmissionLocalPayloadState, CaptureSubmissionSource,
    CaptureSubmissionStatus, CaptureSubmissionTransport, LiveCaptureId, LiveCaptureIntegrity,
    LiveCaptureState, PevcapEncoding, RideDatabase,
};
use crate::CaptureArtifactId;
use cutout_core::{
    CaptureAttachments, CaptureEvidenceManifest, CaptureSubmissionCategories,
    CaptureSubmissionIntegrity, CaptureSubmissionOrigin, CaptureSubmissionPayloadSummary,
    PevcapEncoding as CoreEncoding, validate_capture_submission,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Whether a source can be admitted by a reviewed batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSubmissionBatchEligibility {
    /// A durable inactive capture with a valid payload.
    Eligible,
    /// The source already has a matching durable submission receipt.
    AlreadySubmitted,
    /// The source is still being written.
    Active,
    /// The source contains no observations.
    Empty,
    /// The source exceeds the existing supported upload bound.
    TooLarge,
    /// The source failed strict full-stream validation.
    Corrupt,
    /// The managed source is not available for a bounded read.
    Unavailable,
}

/// Kind of the selected source; aliases collapse onto the canonical source identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSubmissionBatchSourceKind {
    /// Inactive database-backed recording, including interrupted sessions.
    LiveCapture,
    /// Canonical imported capture retained in SQLite.
    RecordedArtifact,
    /// Recognized app-owned legacy original file.
    ManagedFile,
}

/// One terminal or in-flight batch event for a canonical source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSubmissionBatchItemPhase {
    /// A source is currently being processed.
    Current,
    /// A valid durable receipt is retained.
    Sent,
    /// Local cleanup is durably complete.
    Cleaned,
    /// The current source failed.
    Failed,
    /// The source was not admitted.
    Skipped,
    /// Admission stopped after cancellation was requested.
    Cancelled,
}

/// Candidate disclosure for the immutable review shown before Send.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureSubmissionBatchCandidate {
    /// Stable source identity used for explicit selection.
    pub source_artifact_id: String,
    /// Source kind used by the shared persistence owner.
    pub source_kind: CaptureSubmissionBatchSourceKind,
    /// Safe user-facing source label.
    pub display_name: String,
    /// Whether this candidate can be admitted.
    pub eligibility: CaptureSubmissionBatchEligibility,
    /// Raw source digest when known from bounded validation.
    pub source_sha256: Option<String>,
    /// Raw source byte count, before optional saved context.
    pub source_bytes: u64,
    /// Exact transfer digest for a previously prepared operation.
    pub transfer_sha256: Option<String>,
    /// Exact transfer byte count for a previously prepared operation.
    pub transfer_bytes: Option<u64>,
    /// Source or prepared-operation recording origin.
    pub origin: CaptureSubmissionOrigin,
    /// Source or prepared-operation completeness.
    pub integrity: CaptureSubmissionIntegrity,
    /// Categories found by full-stream validation, when supported.
    pub categories: Option<CaptureSubmissionCategories>,
    /// Saved note and screenshot count disclosed before Send.
    pub evidence: Option<CaptureEvidenceManifest>,
    /// Persisted operation state and receipt, if already prepared.
    pub status: Option<CaptureSubmissionStatus>,
    /// Typed reason for a non-eligible row.
    pub skip_reason: Option<CaptureSubmissionBatchEligibility>,
    /// Recognized file/database aliases collapsed into this source row.
    pub alias_count: u64,
}

#[derive(Clone, Debug)]
enum PlannedSource {
    LiveCapture {
        id: LiveCaptureId,
        digest: String,
        bytes: u64,
        status_fingerprint: Option<String>,
        evidence_fingerprint: String,
    },
    RecordedArtifact {
        locator: CaptureSubmissionSource,
        digest: String,
        bytes: u64,
        encoding: CoreEncoding,
        status_fingerprint: Option<String>,
        evidence_fingerprint: String,
    },
    CanonicalCapture {
        locator: CaptureSubmissionSource,
        digest: String,
        bytes: u64,
        encoding: CoreEncoding,
        status_fingerprint: Option<String>,
        evidence_fingerprint: String,
    },
    ManagedFile {
        path: PathBuf,
        root: PathBuf,
        fingerprint: normalization::FileFingerprint,
        digest: String,
        bytes: u64,
        encoding: CoreEncoding,
        status_fingerprint: Option<String>,
        evidence_fingerprint: String,
    },
}

/// In-memory reviewed inventory; it contains no payload snapshots.
#[derive(Clone, Debug)]
pub struct CaptureSubmissionBatchPlan {
    candidates: Arc<Vec<CaptureSubmissionBatchCandidate>>,
    sources: Arc<HashMap<String, PlannedSource>>,
    managed_aliases: Arc<HashMap<String, Vec<ManagedFileLocator>>>,
    database_path: PathBuf,
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
struct ManagedFileLocator {
    path: PathBuf,
    root: PathBuf,
    fingerprint: normalization::FileFingerprint,
    digest: String,
    bytes: u64,
}

impl CaptureSubmissionBatchPlan {
    /// Candidate metadata reviewed by the user.
    #[must_use]
    pub fn candidates(&self) -> &[CaptureSubmissionBatchCandidate] {
        &self.candidates
    }

    /// Stops admission after the current source finishes.
    pub fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Progress associated with one canonical source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureSubmissionBatchProgress {
    /// Stable source identity.
    pub source_artifact_id: String,
    /// Current durable milestone.
    pub phase: CaptureSubmissionBatchItemPhase,
    /// Stable operation identity, if preparation completed.
    pub operation_id: Option<String>,
    /// Current persisted status, including pending local cleanup.
    pub status: Option<CaptureSubmissionStatus>,
    /// Typed reason for a failure or skip.
    pub reason: Option<CaptureSubmissionBatchEligibility>,
    /// Number of selected sources.
    pub total: usize,
    /// Sources with durable receipts.
    pub sent: usize,
    /// Sources with durable local cleanup complete.
    pub cleaned: usize,
    /// Sources that failed.
    pub failed: usize,
    /// Sources skipped.
    pub skipped: usize,
    /// Sources left unadmitted after cancellation.
    pub cancelled: usize,
    /// Current in-flight source count, zero or one.
    pub current: usize,
    /// Bytes streamed for the current source, when transport has started.
    pub bytes_sent: Option<u64>,
    /// Exact immutable transfer size for the current source.
    pub total_bytes: Option<u64>,
}

/// Aggregate result for one explicit batch invocation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptureSubmissionBatchSummary {
    /// Sources with validated durable receipts, including those already cleaned.
    pub sent: usize,
    /// Subset of sent sources whose local payload cleanup completed.
    pub cleaned: usize,
    /// Sources that failed.
    pub failed: usize,
    /// Stable source IDs that failed and can be reviewed again for an explicit retry.
    pub failed_source_artifact_ids: Vec<String>,
    /// Sources skipped by eligibility or user selection.
    pub skipped: usize,
    /// Sources left unadmitted after cancellation.
    pub cancelled: usize,
    /// Canonical source IDs whose receipt is durable while local cleanup remains pending.
    pub pending_cleanup_source_artifact_ids: Vec<String>,
}

/// Synchronous observer for one source-level batch event.
pub type CaptureSubmissionBatchProgressCallback =
    Arc<dyn Fn(CaptureSubmissionBatchProgress) + Send + Sync>;

impl RideDatabase {
    /// Builds a local-only, read-only inventory for one explicit review.
    pub fn review_capture_submission_batch(
        &self,
        managed_capture_directories: &[PathBuf],
    ) -> Result<CaptureSubmissionBatchPlan, CaptureSubmissionError> {
        review(self, managed_capture_directories)
    }

    /// Sends only reviewed candidate identities, in one shared serial owner.
    pub async fn send_capture_submission_batch<T: CaptureSubmissionTransport>(
        &self,
        plan: &CaptureSubmissionBatchPlan,
        selected_source_artifact_ids: &[String],
        account_id: &str,
        token: &str,
        transport: &T,
        progress: CaptureSubmissionBatchProgressCallback,
    ) -> Result<CaptureSubmissionBatchSummary, CaptureSubmissionError> {
        send_selected(
            self,
            plan,
            selected_source_artifact_ids,
            account_id,
            token,
            transport,
            progress,
        )
        .await
    }
}

const BATCH_HISTORY_PAGE_SIZE: u32 = 100;
const FILE_HASH_CHUNK_BYTES: usize = 64 * 1024;

fn review(
    database: &RideDatabase,
    managed_capture_directories: &[PathBuf],
) -> Result<CaptureSubmissionBatchPlan, CaptureSubmissionError> {
    let limit = QueryLimit::new(BATCH_HISTORY_PAGE_SIZE)?;
    let mut candidates = Vec::new();
    let mut sources = HashMap::new();
    let mut source_indices = HashMap::new();
    let mut canonical_digests = HashMap::<String, String>::new();
    let mut managed_aliases = HashMap::<String, Vec<ManagedFileLocator>>::new();

    let mut live_cursor = None;
    loop {
        let page = database.list_live_capture_history(live_cursor.clone(), limit)?;
        for entry in page.captures {
            let source_id = entry.submission.source_artifact_id.clone();
            if entry.submission.source != CaptureSubmissionSource::LiveCapture(entry.id) {
                return Err(CaptureSubmissionError::InvalidCapture);
            }
            if source_indices.contains_key(&source_id) {
                continue;
            }
            let status = entry.submission.status.clone();
            let fingerprint = status_fingerprint(status.as_ref());
            let evidence =
                database.capture_evidence(CaptureSubmissionSource::LiveCapture(entry.id))?;
            let source = PlannedSource::LiveCapture {
                id: entry.id,
                digest: String::new(),
                bytes: entry.stored_bytes,
                status_fingerprint: fingerprint,
                evidence_fingerprint: evidence_fingerprint(&evidence),
            };
            let preview = if entry.state == LiveCaptureState::Active {
                Err(CaptureSubmissionBatchEligibility::Active)
            } else if entry.event_count == 0 {
                Err(CaptureSubmissionBatchEligibility::Empty)
            } else if entry.stored_bytes > cutout_core::MAX_CAPTURE_SUBMISSION_BYTES {
                Err(CaptureSubmissionBatchEligibility::TooLarge)
            } else {
                preview_live_capture(database, entry.id, &evidence).map_err(map_preview_error)
            };
            let (summary, failure) = match preview {
                Ok(summary) => (Some(summary), None),
                Err(failure) => (None, Some(failure)),
            };
            let candidate = candidate_for_live(
                source_id.clone(),
                &entry,
                status,
                &evidence,
                summary.as_ref(),
                failure,
            );
            let source = match (source, candidate.source_sha256.as_ref()) {
                (
                    PlannedSource::LiveCapture {
                        id,
                        status_fingerprint,
                        evidence_fingerprint,
                        ..
                    },
                    Some(digest),
                ) => PlannedSource::LiveCapture {
                    id,
                    digest: digest.clone(),
                    bytes: candidate.source_bytes,
                    status_fingerprint,
                    evidence_fingerprint,
                },
                (source, _) => source,
            };
            if let Some(digest) = candidate.source_sha256.clone() {
                canonical_digests.insert(digest, source_id.clone());
            }
            let index = candidates.len();
            source_indices.insert(source_id.clone(), index);
            sources.insert(source_id, source);
            candidates.push(candidate);
        }
        live_cursor = page.next_cursor;
        if live_cursor.is_none() {
            break;
        }
    }

    let mut capture_cursor: Option<PevcapCaptureCursor> = None;
    loop {
        let page = database.list_pevcap_captures(capture_cursor, limit)?;
        for capture in page.captures {
            let source_id = capture.submission.source_artifact_id.clone();
            let locator = capture.submission.source;
            let digest = capture.artifact_digest.clone();
            if let Some(index) = source_indices.get(&source_id).copied() {
                candidates[index].alias_count += 1;
                canonical_digests.insert(digest, source_id);
                continue;
            }
            let status = capture.submission.status.clone();
            let source = if capture.recording.is_some() {
                PlannedSource::RecordedArtifact {
                    locator,
                    digest: digest.clone(),
                    bytes: capture.artifact_size,
                    encoding: capture.encoding,
                    status_fingerprint: status_fingerprint(status.as_ref()),
                    evidence_fingerprint: String::new(),
                }
            } else {
                PlannedSource::CanonicalCapture {
                    locator,
                    digest: digest.clone(),
                    bytes: capture.artifact_size,
                    encoding: capture.encoding,
                    status_fingerprint: status_fingerprint(status.as_ref()),
                    evidence_fingerprint: String::new(),
                }
            };
            let evidence = database.capture_evidence(locator)?;
            let source = match source {
                PlannedSource::RecordedArtifact {
                    locator,
                    digest,
                    bytes,
                    encoding,
                    status_fingerprint,
                    ..
                } => PlannedSource::RecordedArtifact {
                    locator,
                    digest,
                    bytes,
                    encoding,
                    status_fingerprint,
                    evidence_fingerprint: evidence_fingerprint(&evidence),
                },
                PlannedSource::CanonicalCapture {
                    locator,
                    digest,
                    bytes,
                    encoding,
                    status_fingerprint,
                    ..
                } => PlannedSource::CanonicalCapture {
                    locator,
                    digest,
                    bytes,
                    encoding,
                    status_fingerprint,
                    evidence_fingerprint: evidence_fingerprint(&evidence),
                },
                source => source,
            };
            let preview = if capture.record_count == 0 {
                Err(CaptureSubmissionBatchEligibility::Empty)
            } else if capture.artifact_size > cutout_core::MAX_CAPTURE_SUBMISSION_BYTES {
                Err(CaptureSubmissionBatchEligibility::TooLarge)
            } else {
                preview_canonical_capture(
                    database,
                    &digest,
                    capture.artifact_size,
                    capture.encoding,
                    &evidence,
                )
                .map_err(map_preview_error)
            };
            let (summary, failure) = match preview {
                Ok(summary) => (Some(summary), None),
                Err(failure) => (None, Some(failure)),
            };
            let candidate = candidate_for_imported(
                source_id.clone(),
                &capture,
                status,
                &evidence,
                summary.as_ref(),
                failure,
            );
            canonical_digests.insert(digest, source_id.clone());
            source_indices.insert(source_id.clone(), candidates.len());
            sources.insert(source_id, source);
            candidates.push(candidate);
        }
        capture_cursor = page.next_cursor;
        if capture_cursor.is_none() {
            break;
        }
    }

    let mut file_digests = HashMap::<String, String>::new();
    for root in managed_capture_directories {
        let Some((canonical_root, is_export_root)) = validated_root(root)? else {
            continue;
        };
        let mut paths = fs::read_dir(&canonical_root)
            .map_err(super::StorageError::from)?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(super::StorageError::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        for path in paths {
            let Some(kind) = super::super::capture_file_dedup::managed_capture_file_kind(
                path.file_name().unwrap_or_default(),
            ) else {
                continue;
            };
            if is_export_root
                || kind != super::super::capture_file_dedup::CaptureFileKind::ManagedOriginal
            {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    candidates.push(unavailable_file_candidate(&path));
                    continue;
                }
            };
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                candidates.push(unavailable_file_candidate(&path));
                continue;
            }
            let fingerprint = normalization::FileFingerprint::from_metadata(&metadata);
            let mut file = match File::open(&path) {
                Ok(file) => file,
                Err(_) => {
                    candidates.push(unavailable_file_candidate(&path));
                    continue;
                }
            };
            if !normalization::same_file_identity(
                fingerprint,
                normalization::FileFingerprint::from_metadata(
                    &file.metadata().map_err(super::StorageError::from)?,
                ),
            ) {
                candidates.push(unavailable_file_candidate(&path));
                continue;
            }
            let digest = match hash_file(&mut file, fingerprint.len()) {
                Ok(digest) => digest,
                Err(_) => {
                    candidates.push(unavailable_file_candidate(&path));
                    continue;
                }
            };
            if !file_path_still_matches(&path, &file, fingerprint)? {
                candidates.push(unavailable_file_candidate(&path));
                continue;
            }
            if let Some(source_id) = canonical_digests.get(&digest) {
                if let Some(index) = source_indices.get(source_id).copied() {
                    candidates[index].alias_count += 1;
                }
                managed_aliases
                    .entry(source_id.clone())
                    .or_default()
                    .push(ManagedFileLocator {
                        path: path.clone(),
                        root: canonical_root.clone(),
                        fingerprint,
                        digest: digest.clone(),
                        bytes: fingerprint.len(),
                    });
                continue;
            }
            if let Some(source_id) = file_digests.get(&digest) {
                if let Some(index) = source_indices.get(source_id).copied() {
                    candidates[index].alias_count += 1;
                }
                managed_aliases
                    .entry(source_id.clone())
                    .or_default()
                    .push(ManagedFileLocator {
                        path: path.clone(),
                        root: canonical_root.clone(),
                        fingerprint,
                        digest: digest.clone(),
                        bytes: fingerprint.len(),
                    });
                continue;
            }

            let artifact_id = super::legacy_capture_artifact_id(&digest)?;
            let status = database
                .capture_submission(CaptureSubmissionSource::RecordedArtifact(artifact_id))?;
            let evidence = CaptureAttachments::default();
            let preview = if fingerprint.len() > cutout_core::MAX_CAPTURE_SUBMISSION_BYTES {
                Err(CaptureSubmissionBatchEligibility::TooLarge)
            } else {
                inspect_payload(&path, CoreEncoding::Jsonl, &evidence).map_err(map_preview_error)
            };
            let (summary, failure) = match preview {
                Ok(summary) => (Some(summary), None),
                Err(failure) => (None, Some(failure)),
            };
            let candidate = candidate_for_file(
                artifact_id.to_string(),
                &path,
                fingerprint.len(),
                &digest,
                status.clone(),
                &evidence,
                summary.as_ref(),
                failure,
            );
            let status_fingerprint = status_fingerprint(status.as_ref());
            sources.insert(
                artifact_id.to_string(),
                PlannedSource::ManagedFile {
                    path: path.clone(),
                    root: canonical_root.clone(),
                    fingerprint,
                    digest: digest.clone(),
                    bytes: fingerprint.len(),
                    encoding: CoreEncoding::Jsonl,
                    status_fingerprint,
                    evidence_fingerprint: evidence_fingerprint(&evidence),
                },
            );
            managed_aliases.insert(
                artifact_id.to_string(),
                vec![ManagedFileLocator {
                    path: path.clone(),
                    root: canonical_root.clone(),
                    fingerprint,
                    digest: digest.clone(),
                    bytes: fingerprint.len(),
                }],
            );
            file_digests.insert(digest, artifact_id.to_string());
            source_indices.insert(artifact_id.to_string(), candidates.len());
            candidates.push(candidate);
        }
    }

    candidates.sort_by(|left, right| left.source_artifact_id.cmp(&right.source_artifact_id));
    Ok(CaptureSubmissionBatchPlan {
        candidates: Arc::new(candidates),
        sources: Arc::new(sources),
        managed_aliases: Arc::new(managed_aliases),
        database_path: database.path.as_ref().clone(),
        cancelled: Arc::new(AtomicBool::new(false)),
        running: Arc::new(AtomicBool::new(false)),
    })
}

fn candidate_for_live(
    source_artifact_id: String,
    entry: &super::LiveCaptureHistoryEntry,
    status: Option<CaptureSubmissionStatus>,
    evidence: &CaptureAttachments,
    preview: Option<&ReviewedPayload>,
    failure: Option<CaptureSubmissionBatchEligibility>,
) -> CaptureSubmissionBatchCandidate {
    let origin = status.as_ref().map_or_else(
        || {
            entry
                .recording
                .as_ref()
                .map_or(CaptureSubmissionOrigin::Unknown, |recording| {
                    recording.origin.into()
                })
        },
        |status| status.metadata.origin,
    );
    let integrity = status.as_ref().map_or_else(
        || submission_integrity(entry.integrity),
        |status| status.metadata.integrity,
    );
    candidate_from_parts(
        source_artifact_id,
        CaptureSubmissionBatchSourceKind::LiveCapture,
        entry.platform_identifier.clone(),
        preview.map_or(entry.stored_bytes, |preview| preview.summary.bytes),
        origin,
        integrity,
        evidence,
        preview,
        status,
        failure,
    )
}

fn candidate_for_imported(
    source_artifact_id: String,
    capture: &StoredPevcapCapture,
    status: Option<CaptureSubmissionStatus>,
    evidence: &CaptureAttachments,
    preview: Option<&ReviewedPayload>,
    failure: Option<CaptureSubmissionBatchEligibility>,
) -> CaptureSubmissionBatchCandidate {
    let origin = status.as_ref().map_or_else(
        || {
            capture
                .recording
                .as_ref()
                .map_or(CaptureSubmissionOrigin::Unknown, |recording| {
                    recording.origin.into()
                })
        },
        |status| status.metadata.origin,
    );
    let integrity = status
        .as_ref()
        .map_or(CaptureSubmissionIntegrity::Unknown, |status| {
            status.metadata.integrity
        });
    candidate_from_parts(
        source_artifact_id,
        CaptureSubmissionBatchSourceKind::RecordedArtifact,
        capture.recording.as_ref().map_or_else(
            || "Imported capture".to_owned(),
            |recording| recording.platform_identifier.clone(),
        ),
        capture.artifact_size,
        origin,
        integrity,
        evidence,
        preview,
        status,
        failure,
    )
}

fn candidate_for_file(
    source_artifact_id: String,
    path: &Path,
    bytes: u64,
    digest: &str,
    status: Option<CaptureSubmissionStatus>,
    evidence: &CaptureAttachments,
    preview: Option<&ReviewedPayload>,
    failure: Option<CaptureSubmissionBatchEligibility>,
) -> CaptureSubmissionBatchCandidate {
    let mut candidate = candidate_from_parts(
        source_artifact_id,
        CaptureSubmissionBatchSourceKind::ManagedFile,
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Managed capture")
            .to_owned(),
        bytes,
        CaptureSubmissionOrigin::Unknown,
        CaptureSubmissionIntegrity::Unknown,
        evidence,
        preview,
        status,
        failure,
    );
    candidate.source_sha256 = Some(digest.to_owned());
    candidate
}

fn unavailable_file_candidate(path: &Path) -> CaptureSubmissionBatchCandidate {
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Managed capture")
        .to_owned();
    let source_artifact_id = format!(
        "unavailable-managed:{}",
        hex::encode(Sha256::digest(
            path.as_os_str().to_string_lossy().as_bytes()
        ))
    );
    CaptureSubmissionBatchCandidate {
        source_artifact_id,
        source_kind: CaptureSubmissionBatchSourceKind::ManagedFile,
        display_name: label,
        eligibility: CaptureSubmissionBatchEligibility::Unavailable,
        source_sha256: None,
        source_bytes: 0,
        transfer_sha256: None,
        transfer_bytes: None,
        origin: CaptureSubmissionOrigin::Unknown,
        integrity: CaptureSubmissionIntegrity::Unknown,
        categories: None,
        evidence: None,
        status: None,
        skip_reason: Some(CaptureSubmissionBatchEligibility::Unavailable),
        alias_count: 0,
    }
}

fn candidate_from_parts(
    source_artifact_id: String,
    source_kind: CaptureSubmissionBatchSourceKind,
    display_name: String,
    source_bytes: u64,
    origin: CaptureSubmissionOrigin,
    integrity: CaptureSubmissionIntegrity,
    evidence: &CaptureAttachments,
    preview: Option<&ReviewedPayload>,
    status: Option<CaptureSubmissionStatus>,
    failure: Option<CaptureSubmissionBatchEligibility>,
) -> CaptureSubmissionBatchCandidate {
    let evidence_manifest = preview.and_then(|reviewed| {
        CaptureEvidenceManifest::new(
            reviewed.summary.sha256.clone(),
            reviewed.summary.bytes,
            evidence,
        )
        .ok()
    });
    let already_submitted = status.as_ref().is_some_and(valid_receipt);
    let eligibility = if already_submitted {
        CaptureSubmissionBatchEligibility::AlreadySubmitted
    } else {
        failure.unwrap_or(CaptureSubmissionBatchEligibility::Eligible)
    };
    let transfer_sha256 = status
        .as_ref()
        .map(|status| status.metadata.sha256.clone())
        .or_else(|| preview.map(|reviewed| reviewed.transfer_sha256.clone()));
    let transfer_bytes = status
        .as_ref()
        .map(|status| status.metadata.bytes)
        .or_else(|| preview.map(|reviewed| reviewed.transfer_bytes));
    let source_sha256 = preview.map(|reviewed| reviewed.summary.sha256.clone());
    let categories = preview.map(|reviewed| {
        let mut categories = reviewed.summary.categories;
        categories.notes |= !evidence.note.is_empty();
        categories
    });
    CaptureSubmissionBatchCandidate {
        source_artifact_id,
        source_kind,
        display_name,
        eligibility,
        source_sha256,
        source_bytes,
        transfer_sha256,
        transfer_bytes,
        origin,
        integrity,
        categories,
        evidence: status
            .as_ref()
            .and_then(|status| status.metadata.evidence.clone())
            .or(evidence_manifest),
        status,
        skip_reason: (eligibility != CaptureSubmissionBatchEligibility::Eligible)
            .then_some(eligibility),
        alias_count: 0,
    }
}

fn submission_integrity(integrity: LiveCaptureIntegrity) -> CaptureSubmissionIntegrity {
    match integrity {
        LiveCaptureIntegrity::Complete => CaptureSubmissionIntegrity::Complete,
        LiveCaptureIntegrity::Incomplete { dropped_messages } => {
            CaptureSubmissionIntegrity::Incomplete { dropped_messages }
        }
        LiveCaptureIntegrity::Unknown => CaptureSubmissionIntegrity::Unknown,
    }
}

fn valid_receipt(status: &CaptureSubmissionStatus) -> bool {
    status.receipt.as_ref().is_some_and(|receipt| {
        receipt
            .validate_for(&status.operation_id, &status.metadata)
            .is_ok()
    })
}

fn map_preview_error(error: CaptureSubmissionError) -> CaptureSubmissionBatchEligibility {
    match error {
        CaptureSubmissionError::TooLarge => CaptureSubmissionBatchEligibility::TooLarge,
        CaptureSubmissionError::NotFinished => CaptureSubmissionBatchEligibility::Empty,
        CaptureSubmissionError::NotFound | CaptureSubmissionError::ProvenanceUnavailable => {
            CaptureSubmissionBatchEligibility::Unavailable
        }
        CaptureSubmissionError::InvalidCapture | CaptureSubmissionError::InvalidEvidence(_) => {
            CaptureSubmissionBatchEligibility::Corrupt
        }
        CaptureSubmissionError::WrongAccount
        | CaptureSubmissionError::InvalidState
        | CaptureSubmissionError::Storage(_) => CaptureSubmissionBatchEligibility::Unavailable,
    }
}

fn preview_live_capture(
    database: &RideDatabase,
    id: LiveCaptureId,
    evidence: &CaptureAttachments,
) -> Result<ReviewedPayload, CaptureSubmissionError> {
    let directory = tempfile::tempdir().map_err(super::StorageError::from)?;
    let path = directory.path().join("review.jsonl");
    database
        .export_live_capture(id, &path)
        .map_err(|_| CaptureSubmissionError::NotFound)?;
    inspect_payload(&path, CoreEncoding::Jsonl, evidence)
}

fn preview_canonical_capture(
    database: &RideDatabase,
    digest: &str,
    expected_bytes: u64,
    encoding: PevcapEncoding,
    evidence: &CaptureAttachments,
) -> Result<ReviewedPayload, CaptureSubmissionError> {
    let directory = tempfile::tempdir().map_err(super::StorageError::from)?;
    let path = directory.path().join("review.pevcap");
    database.export_pevcap_capture(digest, &path)?;
    let reviewed = inspect_payload(&path, encoding, evidence)?;
    if reviewed.summary.bytes != expected_bytes || reviewed.summary.sha256 != digest {
        return Err(CaptureSubmissionError::InvalidCapture);
    }
    Ok(reviewed)
}

#[derive(Clone, Debug)]
struct ReviewedPayload {
    summary: CaptureSubmissionPayloadSummary,
    transfer_sha256: String,
    transfer_bytes: u64,
}

fn inspect_payload(
    path: &Path,
    encoding: CoreEncoding,
    evidence: &CaptureAttachments,
) -> Result<ReviewedPayload, CaptureSubmissionError> {
    evidence.validate()?;
    let summary = validate_capture_submission(
        File::open(path).map_err(super::StorageError::from)?,
        encoding,
    )
    .map_err(|_| CaptureSubmissionError::InvalidCapture)?;
    let evidence_json = if evidence.is_empty() {
        None
    } else {
        Some(serde_json::to_vec(evidence).map_err(|_| CaptureSubmissionError::InvalidCapture)?)
    };
    let mut digest = Sha256::new();
    let mut file = File::open(path).map_err(super::StorageError::from)?;
    let mut buffer = vec![0; FILE_HASH_CHUNK_BYTES];
    let mut bytes = 0_u64;
    loop {
        let count = file.read(&mut buffer).map_err(super::StorageError::from)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(u64::try_from(count).map_err(|_| CaptureSubmissionError::InvalidCapture)?)
            .ok_or(CaptureSubmissionError::InvalidCapture)?;
        digest.update(&buffer[..count]);
    }
    if bytes != summary.bytes || hex::encode(digest.clone().finalize()) != summary.sha256 {
        return Err(CaptureSubmissionError::InvalidCapture);
    }
    let transfer_bytes = checked_transfer_size(
        bytes,
        evidence_json.as_ref().map_or(0, |json| json.len() as u64),
    )?;
    if let Some(json) = evidence_json {
        digest.update(json);
    }
    Ok(ReviewedPayload {
        summary,
        transfer_sha256: hex::encode(digest.finalize()),
        transfer_bytes,
    })
}

fn checked_transfer_size(
    source_bytes: u64,
    evidence_bytes: u64,
) -> Result<u64, CaptureSubmissionError> {
    let transfer_bytes = source_bytes
        .checked_add(evidence_bytes)
        .ok_or(CaptureSubmissionError::TooLarge)?;
    if transfer_bytes > cutout_core::MAX_CAPTURE_SUBMISSION_BYTES {
        return Err(CaptureSubmissionError::TooLarge);
    }
    Ok(transfer_bytes)
}

fn validated_root(path: &Path) -> Result<Option<(PathBuf, bool)>, CaptureSubmissionError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(super::StorageError::from(error).into()),
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let is_export_root = matches!(name, "CaptureExports" | "diagnostic-exports");
    if !matches!(
        name,
        "Documents" | "DiagnosticCaptures" | "CaptureExports" | "diagnostic-exports"
    ) {
        return Ok(None);
    }
    Ok(Some((
        fs::canonicalize(path).map_err(super::StorageError::from)?,
        is_export_root,
    )))
}

fn hash_file(file: &mut File, expected_bytes: u64) -> Result<String, std::io::Error> {
    let mut digest = Sha256::new();
    let mut buffer = vec![0; FILE_HASH_CHUNK_BYTES];
    let mut bytes = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        digest.update(&buffer[..count]);
    }
    if bytes != expected_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "managed capture changed while hashing",
        ));
    }
    Ok(hex::encode(digest.finalize()))
}

fn file_path_still_matches(
    path: &Path,
    file: &File,
    fingerprint: normalization::FileFingerprint,
) -> Result<bool, CaptureSubmissionError> {
    let descriptor = file.metadata().map_err(super::StorageError::from)?;
    let path_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(super::StorageError::from(error).into()),
    };
    Ok(path_metadata.file_type().is_file()
        && !path_metadata.file_type().is_symlink()
        && normalization::same_file_identity(
            fingerprint,
            normalization::FileFingerprint::from_metadata(&descriptor),
        )
        && normalization::same_file_identity(
            fingerprint,
            normalization::FileFingerprint::from_metadata(&path_metadata),
        ))
}

fn status_fingerprint(status: Option<&CaptureSubmissionStatus>) -> Option<String> {
    status
        .and_then(|status| serde_json::to_vec(status).ok())
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
}

fn evidence_fingerprint(evidence: &CaptureAttachments) -> String {
    serde_json::to_vec(evidence).map_or_else(
        |_| String::new(),
        |bytes| hex::encode(Sha256::digest(bytes)),
    )
}

async fn send_selected<T: CaptureSubmissionTransport>(
    database: &RideDatabase,
    plan: &CaptureSubmissionBatchPlan,
    selected_source_artifact_ids: &[String],
    account_id: &str,
    token: &str,
    transport: &T,
    progress: CaptureSubmissionBatchProgressCallback,
) -> Result<CaptureSubmissionBatchSummary, CaptureSubmissionError> {
    if database.path.as_ref() != &plan.database_path {
        return Err(CaptureSubmissionError::InvalidState);
    }
    plan.running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| CaptureSubmissionError::InvalidState)?;
    let _run_guard = BatchRunGuard(Arc::clone(&plan.running));
    let candidates = plan
        .candidates
        .iter()
        .map(|candidate| (candidate.source_artifact_id.as_str(), candidate))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    if selected_source_artifact_ids
        .iter()
        .any(|id| !candidates.contains_key(id.as_str()) || !seen.insert(id.as_str()))
    {
        return Err(CaptureSubmissionError::InvalidState);
    }
    let selected = plan
        .candidates
        .iter()
        .filter(|candidate| seen.contains(candidate.source_artifact_id.as_str()))
        .map(|candidate| candidate.source_artifact_id.clone())
        .collect::<Vec<_>>();
    let total = selected.len();
    let mut summary = CaptureSubmissionBatchSummary::default();
    for (position, source_id) in selected.iter().enumerate() {
        let candidate = candidates[source_id.as_str()];
        if plan.cancelled.load(Ordering::Acquire) {
            for cancelled_source in &selected[position..] {
                summary.cancelled += 1;
                emit_progress(
                    &progress,
                    cancelled_source,
                    CaptureSubmissionBatchItemPhase::Cancelled,
                    None,
                    None,
                    total,
                    &summary,
                    0,
                );
            }
            break;
        }
        if candidate.eligibility == CaptureSubmissionBatchEligibility::AlreadySubmitted {
            let status = candidate_source(plan, source_id)
                .and_then(|source| database.capture_submission(source).ok().flatten());
            let has_receipt = status.as_ref().is_some_and(valid_receipt);
            let file_cleaned =
                remove_managed_file_after_receipt(database, plan, source_id, status.as_ref())
                    .unwrap_or(false);
            if !has_receipt {
                summary.failed += 1;
                summary.failed_source_artifact_ids.push(source_id.clone());
                emit_progress(
                    &progress,
                    source_id,
                    CaptureSubmissionBatchItemPhase::Failed,
                    status,
                    Some(CaptureSubmissionBatchEligibility::Unavailable),
                    total,
                    &summary,
                    0,
                );
                continue;
            }
            summary.sent += 1;
            let cleaned = has_receipt
                && status.as_ref().is_some_and(|status| {
                    valid_receipt(status)
                        && status.local_payload_state == CaptureSubmissionLocalPayloadState::Removed
                })
                && file_cleaned;
            if cleaned {
                summary.cleaned += 1;
                emit_progress(
                    &progress,
                    source_id,
                    CaptureSubmissionBatchItemPhase::Cleaned,
                    status,
                    None,
                    total,
                    &summary,
                    0,
                );
            } else {
                summary
                    .pending_cleanup_source_artifact_ids
                    .push(source_id.clone());
                emit_progress(
                    &progress,
                    source_id,
                    CaptureSubmissionBatchItemPhase::Sent,
                    status,
                    Some(CaptureSubmissionBatchEligibility::AlreadySubmitted),
                    total,
                    &summary,
                    0,
                );
            }
            continue;
        }
        if candidate.eligibility != CaptureSubmissionBatchEligibility::Eligible {
            summary.skipped += 1;
            emit_progress(
                &progress,
                source_id,
                CaptureSubmissionBatchItemPhase::Skipped,
                candidate.status.clone(),
                candidate.skip_reason,
                total,
                &summary,
                0,
            );
            continue;
        }
        emit_progress(
            &progress,
            source_id,
            CaptureSubmissionBatchItemPhase::Current,
            candidate.status.clone(),
            None,
            total,
            &summary,
            1,
        );
        match prepare_and_send_one(
            database, plan, source_id, candidate, account_id, token, transport, &progress, total,
            &summary,
        )
        .await
        {
            Ok(status) if valid_receipt(&status) => {
                summary.sent += 1;
                let file_cleaned =
                    remove_managed_file_after_receipt(database, plan, source_id, Some(&status))
                        .unwrap_or(false);
                if status.local_payload_state == CaptureSubmissionLocalPayloadState::Removed
                    && file_cleaned
                {
                    summary.cleaned += 1;
                    emit_progress(
                        &progress,
                        source_id,
                        CaptureSubmissionBatchItemPhase::Cleaned,
                        Some(status),
                        None,
                        total,
                        &summary,
                        0,
                    );
                } else {
                    summary
                        .pending_cleanup_source_artifact_ids
                        .push(source_id.clone());
                    emit_progress(
                        &progress,
                        source_id,
                        CaptureSubmissionBatchItemPhase::Sent,
                        Some(status),
                        None,
                        total,
                        &summary,
                        0,
                    );
                }
            }
            Ok(status) => {
                summary.failed += 1;
                summary.failed_source_artifact_ids.push(source_id.clone());
                emit_progress(
                    &progress,
                    source_id,
                    CaptureSubmissionBatchItemPhase::Failed,
                    Some(status),
                    Some(CaptureSubmissionBatchEligibility::Unavailable),
                    total,
                    &summary,
                    0,
                );
            }
            Err(_) => {
                summary.failed += 1;
                summary.failed_source_artifact_ids.push(source_id.clone());
                let status = candidate_source(plan, source_id)
                    .and_then(|source| database.capture_submission(source).ok().flatten());
                emit_progress(
                    &progress,
                    source_id,
                    CaptureSubmissionBatchItemPhase::Failed,
                    status,
                    Some(CaptureSubmissionBatchEligibility::Unavailable),
                    total,
                    &summary,
                    0,
                );
            }
        }
    }
    Ok(summary)
}

struct BatchRunGuard(Arc<AtomicBool>);

impl Drop for BatchRunGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

async fn prepare_and_send_one<T: CaptureSubmissionTransport>(
    database: &RideDatabase,
    plan: &CaptureSubmissionBatchPlan,
    source_id: &str,
    candidate: &CaptureSubmissionBatchCandidate,
    account_id: &str,
    token: &str,
    transport: &T,
    progress: &CaptureSubmissionBatchProgressCallback,
    total: usize,
    summary: &CaptureSubmissionBatchSummary,
) -> Result<CaptureSubmissionStatus, CaptureSubmissionError> {
    let planned = plan
        .sources
        .get(source_id)
        .ok_or(CaptureSubmissionError::InvalidState)?;
    let source = candidate_source(plan, source_id).ok_or(CaptureSubmissionError::InvalidState)?;
    let current_status = database.capture_submission(source)?;
    if status_fingerprint(current_status.as_ref()) != planned_status_fingerprint(planned) {
        return Err(CaptureSubmissionError::InvalidCapture);
    }
    let external_evidence_unavailable = match planned {
        PlannedSource::ManagedFile { .. } => true,
        PlannedSource::LiveCapture { .. }
        | PlannedSource::RecordedArtifact { .. }
        | PlannedSource::CanonicalCapture { .. } => false,
    };
    let current_evidence = if external_evidence_unavailable {
        CaptureAttachments::default()
    } else {
        database.capture_evidence(source)?
    };
    if evidence_fingerprint(&current_evidence) != planned_evidence_fingerprint(planned) {
        return Err(CaptureSubmissionError::InvalidCapture);
    }
    let prepared = match planned {
        PlannedSource::LiveCapture {
            id, digest, bytes, ..
        } => {
            let current = preview_live_capture(database, *id, &current_evidence)?;
            if digest.is_empty()
                || current.summary.sha256 != *digest
                || current.summary.bytes != *bytes
                || current.summary.bytes != candidate.source_bytes
            {
                return Err(CaptureSubmissionError::InvalidCapture);
            }
            database.prepare_capture_submission(CaptureSubmissionSource::LiveCapture(*id))?
        }
        PlannedSource::RecordedArtifact {
            locator,
            digest,
            bytes,
            encoding,
            ..
        } => {
            let current =
                preview_canonical_capture(database, digest, *bytes, *encoding, &current_evidence)?;
            if candidate.source_sha256.as_deref() != Some(current.summary.sha256.as_str()) {
                return Err(CaptureSubmissionError::InvalidCapture);
            }
            database.prepare_capture_submission(*locator)?
        }
        PlannedSource::CanonicalCapture {
            locator,
            digest,
            bytes,
            encoding,
            ..
        } => {
            let current =
                preview_canonical_capture(database, digest, *bytes, *encoding, &current_evidence)?;
            if candidate.source_sha256.as_deref() != Some(current.summary.sha256.as_str()) {
                return Err(CaptureSubmissionError::InvalidCapture);
            }
            database.prepare_capture_submission(*locator)?
        }
        PlannedSource::ManagedFile {
            path,
            root,
            fingerprint,
            digest,
            bytes,
            encoding,
            ..
        } => {
            let metadata = fs::symlink_metadata(path).map_err(super::StorageError::from)?;
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || !normalization::same_file_identity(
                    *fingerprint,
                    normalization::FileFingerprint::from_metadata(&metadata),
                )
                || !path.starts_with(root)
            {
                return Err(CaptureSubmissionError::InvalidCapture);
            }
            let mut file = File::open(path).map_err(super::StorageError::from)?;
            if hash_file(&mut file, *bytes).map_err(super::StorageError::from)? != *digest
                || !file_path_still_matches(path, &file, *fingerprint)?
            {
                return Err(CaptureSubmissionError::InvalidCapture);
            }
            database.prepare_legacy_capture_submission(path, digest, *encoding)?
        }
    };
    if candidate
        .transfer_sha256
        .as_ref()
        .is_some_and(|digest| digest != &prepared.metadata.sha256)
        || candidate
            .transfer_bytes
            .is_some_and(|bytes| bytes != prepared.metadata.bytes)
    {
        return Err(CaptureSubmissionError::InvalidCapture);
    }
    let operation_id = prepared.operation_id.clone();
    database
        .send_capture_submission_with_progress(
            &operation_id,
            account_id,
            token,
            transport,
            Arc::new({
                let callback = Arc::clone(progress);
                let source_artifact_id = source_id.to_owned();
                let summary = summary.clone();
                let operation_id = operation_id.clone();
                move |send_progress| {
                    (callback)(CaptureSubmissionBatchProgress {
                        source_artifact_id: source_artifact_id.clone(),
                        phase: CaptureSubmissionBatchItemPhase::Current,
                        operation_id: Some(operation_id.clone()),
                        status: None,
                        reason: None,
                        total,
                        sent: summary.sent,
                        cleaned: summary.cleaned,
                        failed: summary.failed,
                        skipped: summary.skipped,
                        cancelled: summary.cancelled,
                        current: 1,
                        bytes_sent: Some(send_progress.bytes_sent),
                        total_bytes: Some(send_progress.total_bytes),
                    });
                }
            }),
        )
        .await
}

fn candidate_source(
    plan: &CaptureSubmissionBatchPlan,
    source_id: &str,
) -> Option<CaptureSubmissionSource> {
    match plan.sources.get(source_id)? {
        PlannedSource::LiveCapture { id, .. } => Some(CaptureSubmissionSource::LiveCapture(*id)),
        PlannedSource::RecordedArtifact { locator, .. }
        | PlannedSource::CanonicalCapture { locator, .. } => Some(*locator),
        PlannedSource::ManagedFile { .. } => CaptureArtifactId::parse(source_id)
            .ok()
            .map(CaptureSubmissionSource::RecordedArtifact),
    }
}

fn remove_managed_file_after_receipt(
    database: &RideDatabase,
    plan: &CaptureSubmissionBatchPlan,
    source_id: &str,
    status: Option<&CaptureSubmissionStatus>,
) -> Result<bool, super::StorageError> {
    let Some(status) = status.filter(|status| valid_receipt(status)) else {
        return Ok(false);
    };
    if plan
        .managed_aliases
        .get(source_id)
        .is_none_or(Vec::is_empty)
    {
        return Ok(true);
    }
    if !plan.sources.contains_key(source_id) {
        return Ok(false);
    }
    let source_digest = status
        .metadata
        .provenance
        .source_sha256
        .as_deref()
        .or_else(|| {
            status
                .metadata
                .evidence
                .as_ref()
                .map(|manifest| manifest.capture_sha256.as_str())
        })
        .or_else(|| {
            (status.metadata.provenance.transformation
                == cutout_core::CaptureSubmissionTransformation::DatabaseExportMusicExcluded)
                .then_some(status.metadata.sha256.as_str())
        });
    let mut cleaned = true;
    for alias in plan.managed_aliases.get(source_id).into_iter().flatten() {
        if !alias.path.starts_with(&alias.root) || source_digest != Some(alias.digest.as_str()) {
            cleaned = false;
            continue;
        }
        let metadata = match fs::symlink_metadata(&alias.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                cleaned = false;
                continue;
            }
        };
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || !normalization::same_file_identity(
                alias.fingerprint,
                normalization::FileFingerprint::from_metadata(&metadata),
            )
        {
            cleaned = false;
            continue;
        }
        match super::super::capture_file_dedup::remove_managed_original_after_receipt(
            database,
            &alias.root,
            &alias.path,
            &alias.digest,
            alias.bytes,
        ) {
            Ok(removed) => cleaned &= removed,
            Err(_) => cleaned = false,
        }
    }
    Ok(cleaned)
}

fn planned_status_fingerprint(source: &PlannedSource) -> Option<String> {
    match source {
        PlannedSource::LiveCapture {
            status_fingerprint, ..
        }
        | PlannedSource::RecordedArtifact {
            status_fingerprint, ..
        }
        | PlannedSource::CanonicalCapture {
            status_fingerprint, ..
        }
        | PlannedSource::ManagedFile {
            status_fingerprint, ..
        } => status_fingerprint.clone(),
    }
}

fn planned_evidence_fingerprint(source: &PlannedSource) -> String {
    match source {
        PlannedSource::LiveCapture {
            evidence_fingerprint,
            ..
        }
        | PlannedSource::RecordedArtifact {
            evidence_fingerprint,
            ..
        }
        | PlannedSource::CanonicalCapture {
            evidence_fingerprint,
            ..
        }
        | PlannedSource::ManagedFile {
            evidence_fingerprint,
            ..
        } => evidence_fingerprint.clone(),
    }
}

fn emit_progress(
    callback: &CaptureSubmissionBatchProgressCallback,
    source_artifact_id: &str,
    phase: CaptureSubmissionBatchItemPhase,
    status: Option<CaptureSubmissionStatus>,
    reason: Option<CaptureSubmissionBatchEligibility>,
    total: usize,
    summary: &CaptureSubmissionBatchSummary,
    current: usize,
) {
    (callback)(CaptureSubmissionBatchProgress {
        source_artifact_id: source_artifact_id.to_owned(),
        phase,
        operation_id: status.as_ref().map(|status| status.operation_id.clone()),
        status,
        reason,
        total,
        sent: summary.sent,
        cleaned: summary.cleaned,
        failed: summary.failed,
        skipped: summary.skipped,
        cancelled: summary.cancelled,
        current,
        bytes_sent: None,
        total_bytes: None,
    });
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use crate::{CaptureMetadata, CaptureWriteOutcome, CaptureWriter, CaptureWriterFinish};
    use cutout_core::{
        CaptureOrigin, GattChannel, MonotonicTimestamp, PevcapRecord, WallClockUnixTimestamp,
    };
    use rusqlite::Connection;
    use std::{
        future::Future,
        io::Read,
        path::Path,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        task::{Context, Poll, Waker},
    };

    const ACCOUNT: &str = "3a61d30c-669a-4a98-8b76-ddbb2b766e14";

    fn ready<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match Box::pin(future).as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("test transport unexpectedly pending"),
        }
    }

    fn finished_capture(database: &RideDatabase, directory: &Path, byte: u8) -> LiveCaptureId {
        let writer = CaptureWriter::start_with_database(
            directory.join(format!("capture-{byte}.jsonl")),
            WallClockUnixTimestamp::new(1_700_000_000_000 + u64::from(byte)),
            "unknown-device",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
            database.clone(),
        )
        .unwrap();
        assert_eq!(
            writer.try_send_record(PevcapRecord::inbound_notification(
                MonotonicTimestamp::new(1),
                GattChannel::from_bytes([0x11; 16]),
                GattChannel::from_bytes([0x22; 16]),
                vec![byte],
            )),
            CaptureWriteOutcome::Accepted,
        );
        let CaptureWriterFinish::DatabaseFinished {
            live_capture_id,
            receipt,
            ..
        } = writer.finish_without_export().unwrap()
        else {
            panic!("database completion required");
        };
        database
            .retain_finished_database_capture(
                &receipt,
                CaptureOrigin::Manual,
                None,
                WallClockUnixTimestamp::new(1_700_000_001_000),
            )
            .unwrap();
        live_capture_id
    }

    #[derive(Default)]
    struct BatchTransport {
        uploads: AtomicUsize,
        fail_next: Mutex<bool>,
        invalid_receipt: AtomicBool,
        deletes: AtomicUsize,
        after_upload: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    }

    impl CaptureSubmissionTransport for BatchTransport {
        fn upload<'a>(
            &'a self,
            operation_id: &'a str,
            _token: &'a str,
            metadata: &'a CaptureSubmissionMetadata,
            mut payload: CaptureSubmissionPayload,
        ) -> CaptureSubmissionFuture<'a, CaptureSubmissionReceipt> {
            Box::pin(async move {
                let mut bytes = Vec::new();
                payload
                    .read_to_end(&mut bytes)
                    .map_err(|_| CaptureSubmissionTransferError::InvalidCapture)?;
                if bytes.len() as u64 != metadata.bytes {
                    return Err(CaptureSubmissionTransferError::InvalidCapture);
                }
                self.uploads.fetch_add(1, Ordering::AcqRel);
                if std::mem::take(&mut *self.fail_next.lock().unwrap()) {
                    return Err(CaptureSubmissionTransferError::Network);
                }
                if let Some(after_upload) = self.after_upload.lock().unwrap().take() {
                    after_upload();
                }
                let mut receipt = CaptureSubmissionReceipt {
                    operation_id: operation_id.to_owned(),
                    sha256: metadata.sha256.clone(),
                    bytes: metadata.bytes,
                    received_at_milliseconds: 1_700_000_002_000,
                };
                if self.invalid_receipt.load(Ordering::Acquire) {
                    receipt.sha256 = "0".repeat(64);
                }
                Ok(receipt)
            })
        }

        fn receipt<'a>(
            &'a self,
            _operation_id: &'a str,
            _token: &'a str,
        ) -> CaptureSubmissionFuture<'a, Option<CaptureSubmissionReceipt>> {
            Box::pin(async { Ok(None) })
        }

        fn delete<'a>(
            &'a self,
            _operation_id: &'a str,
            _token: &'a str,
        ) -> CaptureSubmissionFuture<'a, ()> {
            Box::pin(async move {
                self.deletes.fetch_add(1, Ordering::AcqRel);
                Ok(())
            })
        }
    }

    fn eligible_ids(plan: &CaptureSubmissionBatchPlan) -> Vec<String> {
        plan.candidates()
            .iter()
            .filter(|candidate| {
                candidate.eligibility == CaptureSubmissionBatchEligibility::Eligible
            })
            .map(|candidate| candidate.source_artifact_id.clone())
            .collect()
    }

    #[test]
    fn batch_mixed_failure_and_success_keeps_failed_bytes_and_never_deletes_server_copies() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let first = finished_capture(&database, directory.path(), 0x31);
        let second = finished_capture(&database, directory.path(), 0x32);
        let plan = database
            .review_capture_submission_batch(&[])
            .expect("review is local-only");
        let selected = eligible_ids(&plan);
        assert_eq!(selected.len(), 2, "{:#?}", plan.candidates());

        let transport = BatchTransport {
            fail_next: Mutex::new(true),
            ..BatchTransport::default()
        };
        let result = ready(database.send_capture_submission_batch(
            &plan,
            &selected,
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();

        assert_eq!(result.failed, 1);
        assert_eq!(result.sent, 1);
        assert_eq!(result.failed_source_artifact_ids, vec![selected[0].clone()]);
        assert_eq!(transport.uploads.load(Ordering::Acquire), 2);
        assert_eq!(transport.deletes.load(Ordering::Acquire), 0);
        for id in [first, second] {
            let status = database
                .capture_submission(CaptureSubmissionSource::LiveCapture(id))
                .unwrap()
                .expect("selected capture has durable status");
            if status.phase != CaptureSubmissionPhase::Sent {
                assert_eq!(
                    status.local_payload_state,
                    CaptureSubmissionLocalPayloadState::Retained
                );
                assert!(status.receipt.is_none());
            } else {
                assert!(status.receipt.is_some());
                assert_eq!(
                    status.local_payload_state,
                    CaptureSubmissionLocalPayloadState::Removed
                );
            }
        }
        database.shutdown().unwrap();
    }

    #[test]
    fn inventory_does_not_send_and_retry_after_reopen_reuses_the_durable_operation() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&database_path).unwrap();
        let source = finished_capture(&database, directory.path(), 0x41);
        let transport = BatchTransport {
            fail_next: Mutex::new(true),
            ..BatchTransport::default()
        };

        let plan = database.review_capture_submission_batch(&[]).unwrap();
        assert_eq!(transport.uploads.load(Ordering::Acquire), 0);
        let selected = eligible_ids(&plan);
        let failed = ready(database.send_capture_submission_batch(
            &plan,
            &selected,
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(failed.failed, 1);
        let original = database
            .capture_submission(CaptureSubmissionSource::LiveCapture(source))
            .unwrap()
            .unwrap();
        let stale_confirmation = ready(database.send_capture_submission_batch(
            &plan,
            &selected,
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(stale_confirmation.failed, 1);
        assert_eq!(transport.uploads.load(Ordering::Acquire), 1);
        let after_stale_confirmation = database
            .capture_submission(CaptureSubmissionSource::LiveCapture(source))
            .unwrap()
            .unwrap();
        assert_eq!(original.operation_id, after_stale_confirmation.operation_id);
        assert_eq!(original.phase, after_stale_confirmation.phase);
        database.shutdown().unwrap();

        let reopened = RideDatabase::open(&database_path).unwrap();
        let retry_plan = reopened.review_capture_submission_batch(&[]).unwrap();
        assert_eq!(transport.uploads.load(Ordering::Acquire), 1);
        let retried = ready(reopened.send_capture_submission_batch(
            &retry_plan,
            &eligible_ids(&retry_plan),
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();
        let recovered = reopened
            .capture_submission(CaptureSubmissionSource::LiveCapture(source))
            .unwrap()
            .unwrap();
        assert_eq!(retried.failed, 0);
        assert_eq!(original.operation_id, recovered.operation_id);
        assert_eq!(transport.uploads.load(Ordering::Acquire), 2);
        assert!(recovered.receipt.is_some());
        reopened.shutdown().unwrap();
    }

    #[test]
    fn already_removed_source_without_managed_aliases_is_counted_cleaned_not_pending() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let source = finished_capture(&database, directory.path(), 0x45);
        let first_plan = database.review_capture_submission_batch(&[]).unwrap();
        let selected = eligible_ids(&first_plan);
        let first = ready(database.send_capture_submission_batch(
            &first_plan,
            &selected,
            ACCOUNT,
            "secret",
            &BatchTransport::default(),
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(first.sent, 1);

        let mut status = database
            .capture_submission(CaptureSubmissionSource::LiveCapture(source))
            .unwrap()
            .unwrap();
        for _ in 0..20 {
            if status.local_payload_state == CaptureSubmissionLocalPayloadState::Removed {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            status = database
                .capture_submission(CaptureSubmissionSource::LiveCapture(source))
                .unwrap()
                .unwrap();
        }
        assert_eq!(
            status.local_payload_state,
            CaptureSubmissionLocalPayloadState::Removed
        );

        let retry_plan = database.review_capture_submission_batch(&[]).unwrap();
        let candidate = retry_plan
            .candidates()
            .iter()
            .find(|candidate| candidate.status.as_ref().is_some_and(valid_receipt))
            .expect("durable receipt remains visible after payload retirement");
        assert_eq!(
            candidate.eligibility,
            CaptureSubmissionBatchEligibility::AlreadySubmitted
        );
        let second = ready(database.send_capture_submission_batch(
            &retry_plan,
            &[candidate.source_artifact_id.clone()],
            ACCOUNT,
            "secret",
            &BatchTransport::default(),
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(second.sent, 1);
        assert_eq!(second.cleaned, 1);
        assert!(second.pending_cleanup_source_artifact_ids.is_empty());
        database.shutdown().unwrap();
    }

    #[test]
    fn reviewed_already_submitted_source_without_current_receipt_is_failed_not_sent() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&database_path).unwrap();
        let source = finished_capture(&database, directory.path(), 0x46);
        let initial_plan = database.review_capture_submission_batch(&[]).unwrap();
        let selected = eligible_ids(&initial_plan);
        let initial = ready(database.send_capture_submission_batch(
            &initial_plan,
            &selected,
            ACCOUNT,
            "secret",
            &BatchTransport::default(),
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(initial.sent, 1);

        let reviewed = database.review_capture_submission_batch(&[]).unwrap();
        let candidate = reviewed
            .candidates()
            .iter()
            .find(|candidate| candidate.status.as_ref().is_some_and(valid_receipt))
            .expect("successful receipt is included in the review");
        assert_eq!(
            candidate.eligibility,
            CaptureSubmissionBatchEligibility::AlreadySubmitted
        );
        let status = candidate.status.as_ref().unwrap();
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute(
                "DELETE FROM capture_submissions WHERE operation_id = ?1",
                [&status.operation_id],
            )
            .unwrap();
        drop(connection);

        let no_upload = BatchTransport::default();
        let result = ready(database.send_capture_submission_batch(
            &reviewed,
            &[candidate.source_artifact_id.clone()],
            ACCOUNT,
            "secret",
            &no_upload,
            Arc::new(|_| {}),
        ))
        .unwrap();

        assert_eq!(result.sent, 0);
        assert_eq!(result.failed, 1);
        assert_eq!(
            result.failed_source_artifact_ids,
            vec![candidate.source_artifact_id.clone()]
        );
        assert!(result.pending_cleanup_source_artifact_ids.is_empty());
        assert_eq!(no_upload.uploads.load(Ordering::Acquire), 0);
        assert!(
            database
                .capture_submission(CaptureSubmissionSource::LiveCapture(source))
                .unwrap()
                .is_none()
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn source_under_limit_with_evidence_over_limit_is_typed_too_large_and_not_uploaded() {
        let max = cutout_core::MAX_CAPTURE_SUBMISSION_BYTES;
        assert!(max > 2);
        assert!(matches!(
            checked_transfer_size(max - 1, 2),
            Err(CaptureSubmissionError::TooLarge)
        ));
        assert_eq!(checked_transfer_size(max - 2, 2).unwrap(), max);

        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let source = finished_capture(&database, directory.path(), 0x47);
        let reviewed = database.review_capture_submission_batch(&[]).unwrap();
        let mut candidate = reviewed.candidates()[0].clone();
        candidate.eligibility = CaptureSubmissionBatchEligibility::TooLarge;
        candidate.skip_reason = Some(CaptureSubmissionBatchEligibility::TooLarge);
        candidate.transfer_bytes = Some(max + 1);
        let source_id = candidate.source_artifact_id.clone();
        let oversized = CaptureSubmissionBatchPlan {
            candidates: Arc::new(vec![candidate]),
            sources: Arc::clone(&reviewed.sources),
            managed_aliases: Arc::clone(&reviewed.managed_aliases),
            database_path: reviewed.database_path.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
        };
        let transport = BatchTransport::default();
        let result = ready(database.send_capture_submission_batch(
            &oversized,
            &[source_id],
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();

        assert_eq!(result.sent, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 1);
        assert_eq!(transport.uploads.load(Ordering::Acquire), 0);
        assert!(
            database
                .capture_submission(CaptureSubmissionSource::LiveCapture(source))
                .unwrap()
                .is_none()
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn invalid_receipt_keeps_local_payload_and_does_not_count_as_sent() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let source = finished_capture(&database, directory.path(), 0x49);
        let plan = database.review_capture_submission_batch(&[]).unwrap();
        let transport = BatchTransport {
            invalid_receipt: AtomicBool::new(true),
            ..BatchTransport::default()
        };

        let result = ready(database.send_capture_submission_batch(
            &plan,
            &eligible_ids(&plan),
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();

        assert_eq!(result.sent, 0);
        assert_eq!(result.cleaned, 0);
        assert_eq!(result.failed, 1);
        assert_eq!(transport.deletes.load(Ordering::Acquire), 0);
        let status = database
            .capture_submission(CaptureSubmissionSource::LiveCapture(source))
            .unwrap()
            .unwrap();
        assert!(status.receipt.is_none());
        assert_eq!(
            status.local_payload_state,
            CaptureSubmissionLocalPayloadState::Retained
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn review_includes_all_inactive_history_pages() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        for byte in 0..101_u8 {
            finished_capture(&database, directory.path(), byte);
        }
        let plan = database.review_capture_submission_batch(&[]).unwrap();
        assert_eq!(plan.candidates().len(), 101);
        assert!(
            plan.candidates()
                .iter()
                .all(|candidate| candidate.eligibility
                    == CaptureSubmissionBatchEligibility::Eligible)
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn valid_managed_file_is_reviewed_once_and_all_exact_aliases_are_removed_after_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let documents = directory.path().join("Documents");
        fs::create_dir_all(&documents).unwrap();
        let source_database = RideDatabase::open(&directory.path().join("source.sqlite")).unwrap();
        let source = finished_capture(&source_database, directory.path(), 0x71);
        let first = documents
            .join("cutout-btle-capture-1700000000000-00000000-0000-4000-8000-000000000071.jsonl");
        let second = documents
            .join("cutout-btle-capture-1700000000000-00000000-0000-4000-8000-000000000072.jsonl");
        source_database.export_live_capture(source, &first).unwrap();
        fs::copy(&first, &second).unwrap();
        source_database.shutdown().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();

        let plan = database
            .review_capture_submission_batch(std::slice::from_ref(&documents))
            .unwrap();
        assert_eq!(plan.candidates().len(), 1);
        let candidate = &plan.candidates()[0];
        assert_eq!(
            candidate.source_kind,
            CaptureSubmissionBatchSourceKind::ManagedFile
        );
        assert_eq!(
            candidate.eligibility,
            CaptureSubmissionBatchEligibility::Eligible
        );
        assert_eq!(candidate.origin, CaptureSubmissionOrigin::Unknown);
        assert_eq!(candidate.integrity, CaptureSubmissionIntegrity::Unknown);
        assert_eq!(candidate.alias_count, 1);
        assert!(candidate.transfer_sha256.is_some());
        assert!(candidate.transfer_bytes.is_some());

        let result = ready(database.send_capture_submission_batch(
            &plan,
            &[candidate.source_artifact_id.clone()],
            ACCOUNT,
            "secret",
            &BatchTransport::default(),
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(result.sent, 1, "{result:#?}");
        let source_id = CaptureArtifactId::parse(&candidate.source_artifact_id).unwrap();
        let status = database
            .capture_submission(CaptureSubmissionSource::RecordedArtifact(source_id))
            .unwrap()
            .unwrap();
        assert!(!first.exists());
        assert!(!second.exists());
        assert_eq!(result.sent, 1);
        assert_eq!(
            result.pending_cleanup_source_artifact_ids,
            vec![candidate.source_artifact_id.clone()]
        );
        assert!(valid_receipt(&status));
        assert_eq!(
            status.local_payload_state,
            CaptureSubmissionLocalPayloadState::Removed
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn published_legacy_file_reservation_is_adopted_after_reopen_and_receipt_cleans_original() {
        let directory = tempfile::tempdir().unwrap();
        let documents = directory.path().join("Documents");
        fs::create_dir_all(&documents).unwrap();
        let source_database = RideDatabase::open(&directory.path().join("source.sqlite")).unwrap();
        let source = finished_capture(&source_database, directory.path(), 0x73);
        let original = documents
            .join("cutout-btle-capture-1700000000000-00000000-0000-4000-8000-000000000073.jsonl");
        source_database
            .export_live_capture(source, &original)
            .unwrap();
        source_database.shutdown().unwrap();

        let database_path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&database_path).unwrap();
        let metadata = fs::metadata(&original).unwrap();
        let size = metadata.len();
        let digest = hash_file(&mut File::open(&original).unwrap(), size).unwrap();
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = database.prepare_legacy_capture_submission_with_hook(
                &original,
                &digest,
                PevcapEncoding::Jsonl,
                || panic!("simulate interruption after canonical publication"),
            );
        }));
        assert!(interrupted.is_err());
        let connection = Connection::open(&database_path).unwrap();
        let reserved_operation_id: String = connection
            .query_row(
                "SELECT operation_id FROM capture_submission_reservations
                 WHERE source_sha256 = ?1 AND source_size = ?2",
                rusqlite::params![digest, i64::try_from(size).unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        database.shutdown().unwrap();

        let reopened = RideDatabase::open(&database_path).unwrap();
        let plan = reopened
            .review_capture_submission_batch(std::slice::from_ref(&documents))
            .unwrap();
        assert_eq!(plan.candidates().len(), 1, "{:#?}", plan.candidates());
        assert_eq!(
            plan.candidates()[0].eligibility,
            CaptureSubmissionBatchEligibility::Eligible
        );
        assert_eq!(plan.candidates()[0].alias_count, 1);
        let source_id = plan.candidates()[0].source_artifact_id.clone();
        let result = ready(reopened.send_capture_submission_batch(
            &plan,
            std::slice::from_ref(&source_id),
            ACCOUNT,
            "secret",
            &BatchTransport::default(),
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(result.sent, 1);
        let status = reopened
            .capture_submission(candidate_source(&plan, &source_id).unwrap())
            .unwrap()
            .expect("prepared status retains reservation's operation");
        assert_eq!(status.operation_id, reserved_operation_id);
        assert!(
            !reopened
                .managed_capture_file_submission_reserved(&digest, size)
                .unwrap()
        );
        assert!(
            !original.exists(),
            "valid receipt retires the managed original"
        );
        reopened.shutdown().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn managed_root_symlink_swap_after_review_preserves_receipted_original() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let documents = directory.path().join("Documents");
        fs::create_dir_all(&documents).unwrap();
        let source_database = RideDatabase::open(&directory.path().join("source.sqlite")).unwrap();
        let source = finished_capture(&source_database, directory.path(), 0x73);
        let managed = documents
            .join("cutout-btle-capture-1700000000000-00000000-0000-4000-8000-000000000073.jsonl");
        source_database
            .export_live_capture(source, &managed)
            .unwrap();
        source_database.shutdown().unwrap();

        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let plan = database
            .review_capture_submission_batch(std::slice::from_ref(&documents))
            .unwrap();
        let candidate = &plan.candidates()[0];
        assert_eq!(
            candidate.source_kind,
            CaptureSubmissionBatchSourceKind::ManagedFile
        );
        let moved = directory.path().join("Documents-reviewed");
        let root_for_callback = documents.clone();
        let moved_for_callback = moved.clone();
        let transport = BatchTransport {
            after_upload: Mutex::new(Some(Box::new(move || {
                fs::rename(&root_for_callback, &moved_for_callback).unwrap();
                symlink(&moved_for_callback, &root_for_callback).unwrap();
            }))),
            ..BatchTransport::default()
        };
        let result = ready(database.send_capture_submission_batch(
            &plan,
            &[candidate.source_artifact_id.clone()],
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(result.sent, 1);
        assert_eq!(result.cleaned, 0);
        assert_eq!(
            result.pending_cleanup_source_artifact_ids,
            vec![candidate.source_artifact_id.clone()]
        );
        assert!(moved.join(managed.file_name().unwrap()).exists());
        let status = database
            .capture_submission(CaptureSubmissionSource::RecordedArtifact(
                CaptureArtifactId::parse(&candidate.source_artifact_id).unwrap(),
            ))
            .unwrap()
            .unwrap();
        assert!(valid_receipt(&status));
        database.shutdown().unwrap();
    }

    #[test]
    fn evidence_bearing_live_receipt_authorizes_only_the_matching_raw_file_alias() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let documents = directory.path().join("Documents");
        fs::create_dir_all(&documents).unwrap();
        let source = finished_capture(&database, directory.path(), 0x74);
        let managed = documents
            .join("cutout-btle-capture-1700000000000-00000000-0000-4000-8000-000000000074.jsonl");
        database.export_live_capture(source, &managed).unwrap();
        let evidence = CaptureAttachments {
            note: "reviewed context".to_owned(),
            screenshots: Vec::new(),
        };
        database
            .save_capture_evidence(CaptureSubmissionSource::LiveCapture(source), evidence)
            .unwrap();
        let raw_bytes = fs::metadata(&managed).unwrap().len();
        let raw_digest = hash_file(&mut File::open(&managed).unwrap(), raw_bytes).unwrap();
        let plan = database
            .review_capture_submission_batch(std::slice::from_ref(&documents))
            .unwrap();
        let candidate = &plan.candidates()[0];
        assert_eq!(
            candidate.source_kind,
            CaptureSubmissionBatchSourceKind::LiveCapture
        );
        assert_eq!(candidate.alias_count, 1);
        assert!(
            candidate
                .evidence
                .as_ref()
                .is_some_and(|evidence| evidence.note_bytes > 0)
        );
        let result = ready(database.send_capture_submission_batch(
            &plan,
            &[candidate.source_artifact_id.clone()],
            ACCOUNT,
            "secret",
            &BatchTransport::default(),
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(result.sent, 1);
        assert!(!managed.exists());
        assert!(
            database
                .managed_capture_file_removal_authorized(&raw_digest, raw_bytes)
                .unwrap()
        );
        assert!(
            !database
                .managed_capture_file_removal_authorized(&raw_digest, raw_bytes + 1)
                .unwrap()
        );
        assert!(
            !database
                .managed_capture_file_removal_authorized(&"0".repeat(64), raw_bytes)
                .unwrap()
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn changed_evidence_since_review_fails_before_network_admission() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let capture = finished_capture(&database, directory.path(), 0x61);
        let plan = database.review_capture_submission_batch(&[]).unwrap();
        let id = plan.candidates()[0].source_artifact_id.clone();
        database
            .save_capture_evidence(
                CaptureSubmissionSource::LiveCapture(capture),
                CaptureAttachments {
                    note: "changed after review".to_owned(),
                    screenshots: Vec::new(),
                },
            )
            .unwrap();
        let transport = BatchTransport::default();
        let result = ready(database.send_capture_submission_batch(
            &plan,
            &[id],
            ACCOUNT,
            "secret",
            &transport,
            Arc::new(|_| {}),
        ))
        .unwrap();
        assert_eq!(result.failed, 1);
        assert_eq!(transport.uploads.load(Ordering::Acquire), 0);
        let status = database
            .capture_submission(CaptureSubmissionSource::LiveCapture(capture))
            .unwrap();
        assert!(status.is_none());
        database.shutdown().unwrap();
    }

    #[test]
    fn cancelling_batch_after_current_item_stops_before_the_next_source() {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        finished_capture(&database, directory.path(), 0x51);
        finished_capture(&database, directory.path(), 0x52);
        let plan = database.review_capture_submission_batch(&[]).unwrap();
        let selected = eligible_ids(&plan);
        let cancel_plan = plan.clone();
        let result = ready(database.send_capture_submission_batch(
            &plan,
            &selected,
            ACCOUNT,
            "secret",
            &BatchTransport::default(),
            Arc::new(move |progress| {
                if progress.phase == CaptureSubmissionBatchItemPhase::Sent {
                    cancel_plan.request_cancel();
                }
            }),
        ))
        .unwrap();

        assert_eq!(result.sent, 1);
        assert_eq!(result.cancelled, 1);
        assert_eq!(result.failed, 0);
        database.shutdown().unwrap();
    }
}
