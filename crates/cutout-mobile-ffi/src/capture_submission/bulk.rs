//! Rust-owned reviewed capture batch inventory and serial Send boundary.

use super::{
    CaptureSubmissionClientHandle, MobileCaptureSubmissionError, MobileCaptureSubmissionOriginDto,
    MobileCaptureSubmissionStatusDto,
};
use crate::{MobileCaptureIntegrityDto, persistence};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

/// Eligibility for one immutable Rust-reviewed source row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionBatchEligibilityDto {
    Eligible,
    AlreadySubmitted,
    Active,
    Empty,
    TooLarge,
    Corrupt,
    Unavailable,
    /// Listed from saved metadata. The payload has not been read yet.
    Checking,
}

impl From<persistence::CaptureSubmissionBatchEligibility>
    for MobileCaptureSubmissionBatchEligibilityDto
{
    fn from(value: persistence::CaptureSubmissionBatchEligibility) -> Self {
        use persistence::CaptureSubmissionBatchEligibility as Eligibility;
        match value {
            Eligibility::Eligible => Self::Eligible,
            Eligibility::AlreadySubmitted => Self::AlreadySubmitted,
            Eligibility::Active => Self::Active,
            Eligibility::Empty => Self::Empty,
            Eligibility::TooLarge => Self::TooLarge,
            Eligibility::Corrupt => Self::Corrupt,
            Eligibility::Unavailable => Self::Unavailable,
            Eligibility::Checking => Self::Checking,
        }
    }
}

/// Rust-resolved source class; aliases appear once under their canonical source ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionBatchSourceKindDto {
    LiveCapture,
    RecordedArtifact,
    ManagedFile,
}

impl From<persistence::CaptureSubmissionBatchSourceKind>
    for MobileCaptureSubmissionBatchSourceKindDto
{
    fn from(value: persistence::CaptureSubmissionBatchSourceKind) -> Self {
        match value {
            persistence::CaptureSubmissionBatchSourceKind::LiveCapture => Self::LiveCapture,
            persistence::CaptureSubmissionBatchSourceKind::RecordedArtifact => {
                Self::RecordedArtifact
            }
            persistence::CaptureSubmissionBatchSourceKind::ManagedFile => Self::ManagedFile,
        }
    }
}

/// One canonical-source event emitted by the serial batch owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionBatchItemPhaseDto {
    Current,
    Sent,
    Cleaned,
    Failed,
    Skipped,
    Cancelled,
}

impl From<persistence::CaptureSubmissionBatchItemPhase>
    for MobileCaptureSubmissionBatchItemPhaseDto
{
    fn from(value: persistence::CaptureSubmissionBatchItemPhase) -> Self {
        match value {
            persistence::CaptureSubmissionBatchItemPhase::Current => Self::Current,
            persistence::CaptureSubmissionBatchItemPhase::Sent => Self::Sent,
            persistence::CaptureSubmissionBatchItemPhase::Cleaned => Self::Cleaned,
            persistence::CaptureSubmissionBatchItemPhase::Failed => Self::Failed,
            persistence::CaptureSubmissionBatchItemPhase::Skipped => Self::Skipped,
            persistence::CaptureSubmissionBatchItemPhase::Cancelled => Self::Cancelled,
        }
    }
}

/// Limited evidence disclosure for the reviewed source; image bytes stay private.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionBatchEvidenceDto {
    pub note_bytes: u32,
    pub screenshots: u32,
}

impl From<cutout_core::CaptureEvidenceManifest> for MobileCaptureSubmissionBatchEvidenceDto {
    fn from(value: cutout_core::CaptureEvidenceManifest) -> Self {
        Self {
            note_bytes: value.note_bytes,
            screenshots: value.screenshots,
        }
    }
}

/// Source, transfer, provenance, categories and evidence disclosed before confirmation.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionBatchCandidateDto {
    pub source_artifact_id: String,
    pub source_kind: MobileCaptureSubmissionBatchSourceKindDto,
    pub display_name: String,
    pub eligibility: MobileCaptureSubmissionBatchEligibilityDto,
    pub source_sha256: Option<String>,
    pub source_bytes: u64,
    pub transfer_sha256: Option<String>,
    pub transfer_bytes: Option<u64>,
    pub origin: MobileCaptureSubmissionOriginDto,
    pub integrity: MobileCaptureIntegrityDto,
    pub categories: Option<super::MobileCaptureSubmissionCategoriesDto>,
    pub evidence: Option<MobileCaptureSubmissionBatchEvidenceDto>,
    pub status: Option<MobileCaptureSubmissionStatusDto>,
    pub skip_reason: Option<MobileCaptureSubmissionBatchEligibilityDto>,
    pub alias_count: u64,
}

impl From<persistence::CaptureSubmissionBatchCandidate>
    for MobileCaptureSubmissionBatchCandidateDto
{
    fn from(value: persistence::CaptureSubmissionBatchCandidate) -> Self {
        let integrity = match value.integrity {
            cutout_core::CaptureSubmissionIntegrity::Complete => {
                MobileCaptureIntegrityDto::Complete
            }
            cutout_core::CaptureSubmissionIntegrity::Incomplete { dropped_messages } => {
                MobileCaptureIntegrityDto::Incomplete { dropped_messages }
            }
            cutout_core::CaptureSubmissionIntegrity::Unknown => MobileCaptureIntegrityDto::Unknown,
        };
        Self {
            source_artifact_id: value.source_artifact_id,
            source_kind: value.source_kind.into(),
            display_name: value.display_name,
            eligibility: value.eligibility.into(),
            source_sha256: value.source_sha256,
            source_bytes: value.source_bytes,
            transfer_sha256: value.transfer_sha256,
            transfer_bytes: value.transfer_bytes,
            origin: value.origin.into(),
            integrity,
            categories: value.categories.map(Into::into),
            evidence: value.evidence.map(Into::into),
            status: value.status.map(Into::into),
            skip_reason: value.skip_reason.map(Into::into),
            alias_count: value.alias_count,
        }
    }
}

/// Serial progress, including the durable operation state and cleanup distinction.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionBatchProgressDto {
    pub source_artifact_id: String,
    pub phase: MobileCaptureSubmissionBatchItemPhaseDto,
    pub operation_id: Option<String>,
    pub status: Option<MobileCaptureSubmissionStatusDto>,
    pub reason: Option<MobileCaptureSubmissionBatchEligibilityDto>,
    pub total: u64,
    pub sent: u64,
    pub cleaned: u64,
    pub failed: u64,
    pub skipped: u64,
    pub cancelled: u64,
    pub current: u64,
    pub bytes_sent: Option<u64>,
    pub total_bytes: Option<u64>,
}

impl From<persistence::CaptureSubmissionBatchProgress> for MobileCaptureSubmissionBatchProgressDto {
    fn from(value: persistence::CaptureSubmissionBatchProgress) -> Self {
        Self {
            source_artifact_id: value.source_artifact_id,
            phase: value.phase.into(),
            operation_id: value.operation_id,
            status: value.status.map(Into::into),
            reason: value.reason.map(Into::into),
            total: u64::try_from(value.total).unwrap_or(u64::MAX),
            sent: u64::try_from(value.sent).unwrap_or(u64::MAX),
            cleaned: u64::try_from(value.cleaned).unwrap_or(u64::MAX),
            failed: u64::try_from(value.failed).unwrap_or(u64::MAX),
            skipped: u64::try_from(value.skipped).unwrap_or(u64::MAX),
            cancelled: u64::try_from(value.cancelled).unwrap_or(u64::MAX),
            current: u64::try_from(value.current).unwrap_or(u64::MAX),
            bytes_sent: value.bytes_sent,
            total_bytes: value.total_bytes,
        }
    }
}

/// Receives source-level and bounded transport progress. Dispatch promptly to native UI.
#[uniffi::export(callback_interface)]
pub trait MobileCaptureSubmissionBatchProgressListener: Send + Sync {
    fn on_progress(&self, progress: MobileCaptureSubmissionBatchProgressDto);
}

/// Receives one capture when review first lists it, and again when its payload check finishes.
#[uniffi::export(callback_interface)]
pub trait MobileCaptureSubmissionBatchReviewListener: Send + Sync {
    fn on_candidate(&self, candidate: MobileCaptureSubmissionBatchCandidateDto);
}

/// Aggregate result of one user-confirmed serial operation.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionBatchSummaryDto {
    pub sent: u64,
    pub cleaned: u64,
    pub failed: u64,
    pub failed_source_artifact_ids: Vec<String>,
    pub skipped: u64,
    pub cancelled: u64,
    pub pending_cleanup_source_artifact_ids: Vec<String>,
}

impl From<persistence::CaptureSubmissionBatchSummary> for MobileCaptureSubmissionBatchSummaryDto {
    fn from(value: persistence::CaptureSubmissionBatchSummary) -> Self {
        Self {
            sent: u64::try_from(value.sent).unwrap_or(u64::MAX),
            cleaned: u64::try_from(value.cleaned).unwrap_or(u64::MAX),
            failed: u64::try_from(value.failed).unwrap_or(u64::MAX),
            failed_source_artifact_ids: value.failed_source_artifact_ids,
            skipped: u64::try_from(value.skipped).unwrap_or(u64::MAX),
            cancelled: u64::try_from(value.cancelled).unwrap_or(u64::MAX),
            pending_cleanup_source_artifact_ids: value.pending_cleanup_source_artifact_ids,
        }
    }
}

/// Immutable local review. No snapshots are held and creating it never transmits data.
#[derive(Debug, uniffi::Object)]
pub struct CaptureSubmissionBatchPlanHandle {
    inner: persistence::CaptureSubmissionBatchPlan,
}

#[uniffi::export]
impl CaptureSubmissionBatchPlanHandle {
    pub fn candidates(&self) -> Vec<MobileCaptureSubmissionBatchCandidateDto> {
        self.inner
            .candidates()
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    /// Stop admission after the current recording finishes.
    pub fn request_cancel(&self) {
        self.inner.request_cancel();
    }
}

#[uniffi::export(async_runtime = "tokio")]
#[allow(clippy::needless_pass_by_value, reason = "UniFFI owns boundary values")]
impl CaptureSubmissionClientHandle {
    /// Stops payload reads for the review in progress. Send uses the same flag once a plan exists.
    pub fn cancel_review(&self) {
        self.review_cancel.store(true, Ordering::Release);
    }

    /// Inventories every page and supported app-owned managed root locally for review.
    ///
    /// # Errors
    ///
    /// Returns an error when database inventory fails or the blocking worker cannot join.
    pub async fn review_all(
        &self,
        managed_capture_directories: Vec<String>,
        listener: Box<dyn MobileCaptureSubmissionBatchReviewListener>,
    ) -> Result<Arc<CaptureSubmissionBatchPlanHandle>, MobileCaptureSubmissionError> {
        self.review_cancel.store(false, Ordering::Release);
        let database = self.database.clone();
        let cancel = Arc::clone(&self.review_cancel);
        let directories = managed_capture_directories
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let plan = tokio::task::spawn_blocking(move || {
            database.review_capture_submission_batch_with_updates(
                &directories,
                cancel,
                &|candidate| listener.on_candidate(candidate.clone().into()),
            )
        })
        .await
        .map_err(|_| MobileCaptureSubmissionError::StorageFailure)??;
        Ok(Arc::new(CaptureSubmissionBatchPlanHandle { inner: plan }))
    }

    /// Runs one Rust-owned serial batch after one explicit native confirmation.
    ///
    /// # Errors
    ///
    /// Returns an error when the plan or selected identities are invalid, a source is no
    /// longer available, local persistence fails, authentication is rejected, or the remote
    /// service cannot complete the submission.
    pub async fn send_selected(
        &self,
        plan: Arc<CaptureSubmissionBatchPlanHandle>,
        selected_source_artifact_ids: Vec<String>,
        account_id: String,
        token: String,
        listener: Box<dyn MobileCaptureSubmissionBatchProgressListener>,
    ) -> Result<MobileCaptureSubmissionBatchSummaryDto, MobileCaptureSubmissionError> {
        self.database
            .send_capture_submission_batch(
                &plan.inner,
                &selected_source_artifact_ids,
                &account_id,
                &token,
                &self.transport,
                Arc::new(move |progress| listener.on_progress(progress.into())),
            )
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
}
