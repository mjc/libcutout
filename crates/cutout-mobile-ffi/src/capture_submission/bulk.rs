//! Rust-owned reviewed capture batch inventory and serial Send boundary.

use super::{
    CaptureSubmissionClientHandle, MobileCaptureSubmissionError, MobileCaptureSubmissionOriginDto,
    MobileCaptureSubmissionStatusDto,
};
use crate::{MobileCaptureIntegrityDto, persistence};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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
        self.commitment.prepare();
        let database = self.database.clone();
        let cancel = Arc::clone(&self.review_cancel);
        let commitment = Arc::clone(&self.commitment);
        let on_join_failure = Arc::clone(&commitment);
        let transport = Arc::clone(&self.transport);
        let runtime = tokio::runtime::Handle::current();
        let directories = managed_capture_directories
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let joined = tokio::task::spawn_blocking(move || {
            commitment.begin();
            let mut send = FfiCommittedSend {
                commitment: Arc::clone(&commitment),
                transport,
                runtime,
            };
            let reviewed = database.review_capture_submission_batch_with_sender(
                &directories,
                cancel,
                &|candidate| listener.on_candidate(candidate.clone().into()),
                &mut send,
            );
            send.commitment.ensure_idle();
            reviewed
        })
        .await;
        if joined.is_err() {
            on_join_failure.cancel_request();
        }
        let plan = joined.map_err(|_| MobileCaptureSubmissionError::StorageFailure)??;
        Ok(Arc::new(CaptureSubmissionBatchPlanHandle { inner: plan }))
    }

    /// Confirms Send all for the review requested on this client.
    ///
    /// Returns false when no review has been requested. A confirmation that arrives
    /// after the request and before the worker enters is kept for that review.
    /// The caller sends the finished plan when this returns false.
    #[must_use]
    pub fn commit_send_all(
        &self,
        account_id: String,
        token: String,
        listener: Box<dyn MobileCaptureSubmissionBatchProgressListener>,
    ) -> bool {
        self.commitment
            .commit(account_id, token, Arc::from(listener))
    }

    pub fn take_committed_send_summary(&self) -> Option<MobileCaptureSubmissionBatchSummaryDto> {
        self.commitment.take_summary().map(Into::into)
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
                self.transport.as_ref(),
                Arc::new(move |progress| listener.on_progress(progress.into())),
            )
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
}

type ProgressListener = Arc<dyn MobileCaptureSubmissionBatchProgressListener>;

#[allow(
    clippy::struct_excessive_bools,
    reason = "request, entry, and arm are independent latch conditions"
)]
struct ReviewGate {
    in_progress: bool,
    /// Set before the worker enters, so a confirmation can land before `begin`.
    requested: bool,
    /// Confirmation arrived after the request and before the worker entered.
    pending: bool,
    armed: bool,
    covered: bool,
    arm_generation: u64,
    account_id: String,
    token: String,
    listener: Option<ProgressListener>,
    summary: persistence::CaptureSubmissionBatchSummary,
    offset: usize,
}

pub(super) struct ReviewCommitment {
    gate: Mutex<ReviewGate>,
    armed: AtomicBool,
}

impl ReviewCommitment {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            armed: AtomicBool::new(false),
            gate: Mutex::new(ReviewGate {
                in_progress: false,
                requested: false,
                pending: false,
                armed: false,
                covered: false,
                arm_generation: 0,
                account_id: String::new(),
                token: String::new(),
                listener: None,
                summary: persistence::CaptureSubmissionBatchSummary::default(),
                offset: 0,
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ReviewGate> {
        self.gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn prepare(&self) {
        self.lock().requested = true;
    }

    pub(super) fn begin(&self) {
        let mut gate = self.lock();
        let keep_commit = gate.pending;
        gate.pending = false;
        gate.in_progress = true;
        gate.requested = true;
        gate.covered = false;
        gate.summary = persistence::CaptureSubmissionBatchSummary::default();
        gate.offset = 0;
        if keep_commit {
            gate.armed = true;
            self.armed.store(true, Ordering::Release);
            return;
        }
        gate.armed = false;
        gate.arm_generation = 0;
        gate.account_id.clear();
        gate.token.clear();
        gate.listener = None;
        self.armed.store(false, Ordering::Release);
    }

    fn commit(&self, account_id: String, token: String, listener: ProgressListener) -> bool {
        let mut gate = self.lock();
        if !gate.in_progress && !gate.requested {
            return false;
        }
        gate.account_id = account_id;
        gate.token = token;
        gate.listener = Some(listener);
        gate.armed = true;
        gate.arm_generation = 1;
        self.armed.store(true, Ordering::Release);
        if !gate.in_progress {
            gate.pending = true;
        }
        true
    }

    fn cancel_request(&self) {
        let mut gate = self.lock();
        gate.requested = false;
        gate.pending = false;
        gate.in_progress = false;
        if gate.covered {
            return;
        }
        gate.armed = false;
        gate.listener = None;
        gate.account_id.clear();
        gate.token.clear();
        self.armed.store(false, Ordering::Release);
    }

    fn take_summary(&self) -> Option<persistence::CaptureSubmissionBatchSummary> {
        let mut gate = self.lock();
        if !gate.covered {
            return None;
        }
        gate.covered = false;
        Some(gate.summary.clone())
    }

    fn ensure_idle(&self) {
        let mut gate = self.lock();
        gate.in_progress = false;
        gate.requested = false;
    }
}

struct FfiCommittedSend {
    commitment: Arc<ReviewCommitment>,
    transport: Arc<super::HttpCaptureTransport>,
    runtime: tokio::runtime::Handle,
}

impl persistence::CommittedReviewSend for FfiCommittedSend {
    fn armed(&self) -> bool {
        self.commitment.armed.load(Ordering::Acquire)
    }

    fn arm_generation(&self) -> u64 {
        self.commitment.lock().arm_generation
    }

    fn send(
        &mut self,
        database: &persistence::RideDatabase,
        plan: &persistence::CaptureSubmissionBatchPlan,
        source_ids: &[String],
    ) -> persistence::CaptureSubmissionBatchSummary {
        if source_ids.is_empty() {
            return persistence::CaptureSubmissionBatchSummary::default();
        }
        let (account_id, token, listener, offset, base) = {
            let gate = self.commitment.lock();
            (
                gate.account_id.clone(),
                gate.token.clone(),
                gate.listener.clone(),
                gate.offset,
                gate.summary.clone(),
            )
        };
        let total = offset + source_ids.len();
        let database = database.clone();
        let plan = plan.clone();
        let attempted = source_ids.to_vec();
        let transport = Arc::clone(&self.transport);
        let batch = self.runtime.block_on(async move {
            database
                .send_capture_submission_batch(
                    &plan,
                    &attempted,
                    &account_id,
                    &token,
                    transport.as_ref(),
                    Arc::new(move |mut progress| {
                        progress.current = offset + progress.current;
                        progress.total = total;
                        progress.sent += base.sent;
                        progress.cleaned += base.cleaned;
                        progress.failed += base.failed;
                        progress.skipped += base.skipped;
                        progress.cancelled += base.cancelled;
                        if let Some(listener) = &listener {
                            listener.on_progress(progress.into());
                        }
                    }),
                )
                .await
        });
        let batch = match batch {
            Ok(summary) => summary,
            Err(_) => persistence::CaptureSubmissionBatchSummary {
                failed: source_ids.len(),
                failed_source_artifact_ids: source_ids.to_vec(),
                ..persistence::CaptureSubmissionBatchSummary::default()
            },
        };
        let mut gate = self.commitment.lock();
        gate.offset += source_ids.len();
        gate.summary.sent += batch.sent;
        gate.summary.cleaned += batch.cleaned;
        gate.summary.failed += batch.failed;
        gate.summary.skipped += batch.skipped;
        gate.summary.cancelled += batch.cancelled;
        gate.summary
            .failed_source_artifact_ids
            .extend(batch.failed_source_artifact_ids.iter().cloned());
        gate.summary
            .pending_cleanup_source_artifact_ids
            .extend(batch.pending_cleanup_source_artifact_ids.iter().cloned());
        batch
    }

    fn complete(&mut self, flushed_generation: u64) -> bool {
        let mut gate = self.commitment.lock();
        if gate.armed && gate.arm_generation != flushed_generation {
            return true;
        }
        gate.in_progress = false;
        gate.covered = gate.armed;
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::Ordering};

    use super::{
        MobileCaptureSubmissionBatchProgressDto, MobileCaptureSubmissionBatchProgressListener,
        ReviewCommitment,
    };

    struct Nop;

    impl MobileCaptureSubmissionBatchProgressListener for Nop {
        fn on_progress(&self, _progress: MobileCaptureSubmissionBatchProgressDto) {}
    }

    fn listener() -> Arc<dyn MobileCaptureSubmissionBatchProgressListener> {
        Arc::new(Nop)
    }

    fn armed(commitment: &ReviewCommitment) -> bool {
        commitment.armed.load(Ordering::Acquire)
    }

    #[test]
    fn commit_after_prepare_survives_begin() {
        let commitment = ReviewCommitment::new();
        commitment.prepare();
        assert!(commitment.commit("account".to_owned(), "token".to_owned(), listener()));

        commitment.begin();

        let gate = commitment.lock();
        assert!(gate.in_progress);
        assert!(!gate.pending);
        assert!(gate.armed);
        assert_eq!(gate.arm_generation, 1);
        assert_eq!(gate.account_id, "account");
        assert_eq!(gate.token, "token");
        assert!(gate.listener.is_some());
        drop(gate);
        assert!(armed(&commitment));
    }

    #[test]
    fn commit_before_prepare_is_rejected_and_begin_clears_the_arm() {
        let commitment = ReviewCommitment::new();
        assert!(!commitment.commit("account".to_owned(), "token".to_owned(), listener()));

        commitment.begin();

        let gate = commitment.lock();
        assert!(gate.in_progress);
        assert!(!gate.pending);
        assert!(!gate.armed);
        assert_eq!(gate.account_id, "");
        assert!(gate.listener.is_none());
        drop(gate);
        assert!(!armed(&commitment));
    }

    #[test]
    fn commit_after_ensure_idle_is_rejected() {
        let commitment = ReviewCommitment::new();
        commitment.prepare();
        commitment.begin();
        commitment.ensure_idle();

        assert!(!commitment.commit("account".to_owned(), "token".to_owned(), listener()));

        let gate = commitment.lock();
        assert!(!gate.in_progress);
        assert!(!gate.requested);
        assert!(!gate.armed);
        drop(gate);
        assert!(!armed(&commitment));
    }

    #[test]
    fn cancel_request_drops_an_uncovered_pending_arm() {
        let commitment = ReviewCommitment::new();
        commitment.prepare();
        assert!(commitment.commit("account".to_owned(), "token".to_owned(), listener()));

        commitment.cancel_request();

        assert!(!commitment.commit("later".to_owned(), "token".to_owned(), listener()));
        let gate = commitment.lock();
        assert!(!gate.pending);
        assert!(!gate.requested);
        assert!(!gate.armed);
        assert_eq!(gate.account_id, "");
        drop(gate);
        assert!(!armed(&commitment));
    }

    #[test]
    fn cancel_request_keeps_a_covered_arm() {
        let commitment = ReviewCommitment::new();
        commitment.prepare();
        assert!(commitment.commit("account".to_owned(), "token".to_owned(), listener()));
        commitment.lock().covered = true;

        commitment.cancel_request();

        let gate = commitment.lock();
        assert!(gate.armed);
        assert!(gate.covered);
        assert_eq!(gate.account_id, "account");
        drop(gate);
        assert!(armed(&commitment));
    }
}
