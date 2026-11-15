//! `UniFFI` view of the shared capture-batch sheet session.

use std::sync::{Arc, Mutex, PoisonError};

use cutout_core::{
    CaptureBatchEffect, CaptureBatchProgressKind, CaptureBatchRow, CaptureBatchSession,
};

/// One reviewed source passed back into the shared sheet session.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureBatchRowDto {
    /// Canonical source identity.
    pub source_artifact_id: String,
    /// The shared review marked this source sendable.
    pub eligible: bool,
    /// Payload classification is still running.
    pub checking: bool,
}

/// Next native effect for the capture batch sheet.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureBatchEffect {
    /// Stay on the current sheet.
    Idle,
    /// Ask again after the review task leaves.
    WaitForReview,
    /// Arm Send all on the review already running.
    CommitAll {
        /// Confirmation identity for progress from this attempt.
        attempt: u64,
    },
    /// Stop the scan, then send the sources already selected.
    CancelReview,
    /// Upload these sources for this confirmation.
    Send {
        /// Selected source identities that are still eligible.
        source_artifact_ids: Vec<String>,
        /// Confirmation identity for progress from this attempt.
        attempt: u64,
    },
}

/// Progress class that can change the failed-source set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureBatchProgressKind {
    /// No durable receipt.
    Failed,
    /// Receipt stored.
    Sent,
    /// Local payload removed after the receipt.
    Cleaned,
    /// No change to the failed set.
    Other,
}

/// Shared selection and send routing for one capture batch sheet.
#[derive(Debug, uniffi::Object)]
pub struct MobileCaptureBatchSession {
    inner: Mutex<CaptureBatchSession>,
}

#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns the strings at this boundary"
)]
impl MobileCaptureBatchSession {
    /// Creates an empty sheet.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(CaptureBatchSession::new()),
        })
    }

    /// Opens a review. A failure retry without failures does nothing.
    #[must_use]
    pub fn begin_review(&self, failed_only: bool) -> bool {
        self.lock().begin_review(failed_only)
    }

    /// Records one row. Returns false when an eligible row would regress to checking.
    #[must_use]
    pub fn observe(&self, id: String, eligible: bool, checking: bool) -> bool {
        self.lock().observe(&id, eligible, checking)
    }

    /// Replaces the sheet with the review's final rows.
    pub fn finish_review(&self, rows: Vec<MobileCaptureBatchRowDto>, scan_completed: bool) {
        let rows = rows
            .into_iter()
            .map(|row| CaptureBatchRow {
                source_artifact_id: row.source_artifact_id,
                eligible: row.eligible,
                checking: row.checking,
            })
            .collect::<Vec<_>>();
        self.lock().finish_review(&rows, scan_completed);
    }

    /// Records a review that failed before returning rows.
    pub fn fail_review(&self) {
        self.lock().fail_review();
    }

    /// Records a review task that ended without rows or a storage error.
    pub fn note_review_interrupted(&self) {
        self.lock().note_review_interrupted();
    }

    /// Applies one checkbox change.
    #[must_use]
    pub fn set_selected(&self, id: String, selected: bool) -> bool {
        self.lock().set_selected(&id, selected)
    }

    /// Chooses the next effect for this confirmation.
    #[must_use]
    pub fn request_send(&self, ids: Vec<String>) -> MobileCaptureBatchEffect {
        effect(self.lock().request_send(&ids))
    }

    /// Records that the running review accepted Send all.
    #[must_use]
    pub fn note_commit_accepted(&self, attempt: u64) -> bool {
        self.lock().note_commit_accepted(attempt)
    }

    /// Records that the running review did not accept Send all.
    #[must_use]
    pub fn note_commit_rejected(&self, attempt: u64) -> MobileCaptureBatchEffect {
        effect(self.lock().note_commit_rejected(attempt))
    }

    /// Continues a send that waited for the review task.
    #[must_use]
    pub fn review_settled(&self) -> MobileCaptureBatchEffect {
        effect(self.lock().review_settled())
    }

    /// Send all was accepted, but no summary was stored.
    #[must_use]
    pub fn commit_summary_missing(&self, attempt: u64) -> MobileCaptureBatchEffect {
        effect(self.lock().commit_summary_missing(attempt))
    }

    /// Records source progress for the current confirmation.
    #[must_use]
    pub fn note_progress(
        &self,
        id: String,
        kind: MobileCaptureBatchProgressKind,
        attempt: u64,
    ) -> bool {
        self.lock().note_progress(&id, kind.into(), attempt)
    }

    /// Applies the terminal summary for this confirmation.
    #[must_use]
    pub fn note_summary(
        &self,
        failed_source_artifact_ids: Vec<String>,
        cancelled: u64,
        attempt: u64,
    ) -> bool {
        self.lock()
            .note_summary(&failed_source_artifact_ids, cancelled, attempt)
    }

    /// The upload call failed. The named sources stay failed.
    #[must_use]
    pub fn note_send_failure(&self, ids: Vec<String>, attempt: u64) -> bool {
        self.lock().note_send_failure(&ids, attempt)
    }

    /// Send all failed before a summary.
    #[must_use]
    pub fn note_operation_failure(&self, attempt: u64) -> bool {
        self.lock().note_operation_failure(attempt)
    }

    /// Stops the active send after the source already admitted.
    #[must_use]
    pub fn request_stop(&self) -> bool {
        self.lock().request_stop()
    }

    /// The send control can be used.
    #[must_use]
    pub fn can_send(&self) -> bool {
        self.lock().can_send()
    }

    /// Selected identities in selection order.
    #[must_use]
    pub fn selected_ids(&self) -> Vec<String> {
        self.lock().selected_ids()
    }

    /// Whether this source is selected.
    #[must_use]
    pub fn is_selected(&self, id: String) -> bool {
        self.lock().is_selected(&id)
    }

    /// Sources that still need another review after the latest confirmation.
    #[must_use]
    pub fn failed_ids(&self) -> Vec<String> {
        self.lock().failed_ids()
    }

    /// Sendable rows.
    #[must_use]
    pub fn eligible_count(&self) -> u64 {
        self.lock().eligible_count()
    }

    /// Rows that are neither sendable nor still being checked.
    #[must_use]
    pub fn skipped_count(&self) -> u64 {
        self.lock().skipped_count()
    }

    /// How many sources are selected.
    #[must_use]
    pub fn selected_count(&self) -> u64 {
        self.lock().selected_count()
    }

    /// A review task is open.
    #[must_use]
    pub fn is_reviewing(&self) -> bool {
        self.lock().is_reviewing()
    }

    /// A narrowed send is waiting for review to leave.
    #[must_use]
    pub fn is_waiting(&self) -> bool {
        self.lock().is_waiting()
    }

    /// A confirmation is in progress.
    #[must_use]
    pub fn is_sending(&self) -> bool {
        self.lock().is_sending()
    }

    /// The latest confirmation finished.
    #[must_use]
    pub fn did_finish(&self) -> bool {
        self.lock().did_finish()
    }

    /// Stop was requested for the confirmation now running.
    #[must_use]
    pub fn did_request_stop(&self) -> bool {
        self.lock().did_request_stop()
    }

    /// Another send needs a new review.
    #[must_use]
    pub fn requires_fresh_review(&self) -> bool {
        self.lock().requires_fresh_review()
    }

    /// Review or send reported an error.
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.lock().has_error()
    }

    /// Send all was accepted and its summary is still outstanding.
    #[must_use]
    pub fn is_awaiting_commit(&self) -> bool {
        self.lock().is_awaiting_commit()
    }

    /// Identity of the confirmation now running.
    #[must_use]
    pub fn attempt(&self) -> u64 {
        self.lock().attempt()
    }
}

impl MobileCaptureBatchSession {
    fn lock(&self) -> std::sync::MutexGuard<'_, CaptureBatchSession> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<MobileCaptureBatchProgressKind> for CaptureBatchProgressKind {
    fn from(kind: MobileCaptureBatchProgressKind) -> Self {
        match kind {
            MobileCaptureBatchProgressKind::Failed => Self::Failed,
            MobileCaptureBatchProgressKind::Sent => Self::Sent,
            MobileCaptureBatchProgressKind::Cleaned => Self::Cleaned,
            MobileCaptureBatchProgressKind::Other => Self::Other,
        }
    }
}

fn effect(effect: CaptureBatchEffect) -> MobileCaptureBatchEffect {
    match effect {
        CaptureBatchEffect::Idle => MobileCaptureBatchEffect::Idle,
        CaptureBatchEffect::WaitForReview => MobileCaptureBatchEffect::WaitForReview,
        CaptureBatchEffect::CommitAll { attempt } => {
            MobileCaptureBatchEffect::CommitAll { attempt }
        }
        CaptureBatchEffect::CancelReview => MobileCaptureBatchEffect::CancelReview,
        CaptureBatchEffect::Send {
            source_artifact_ids,
            attempt,
        } => MobileCaptureBatchEffect::Send {
            source_artifact_ids,
            attempt,
        },
    }
}
