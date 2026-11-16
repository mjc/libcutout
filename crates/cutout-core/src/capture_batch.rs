//! Review-sheet decisions shared by the native capture batch screens.
//!
//! The session admits rows, keeps the selection, and chooses the next send
//! effect. Localized copy, dialogs, and the upload itself stay on the platform.

use std::collections::{BTreeMap, BTreeSet};

/// How many times an in-progress review may refuse Send all before the sheet
/// waits and sends the finished plan. The review worker can still be entering
/// when the user confirms.
const COMMIT_ATTEMPTS: u8 = 5;

/// One reviewed source, without display copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureBatchRow {
    /// Canonical source identity.
    pub source_artifact_id: String,
    /// The shared review marked this source sendable.
    pub eligible: bool,
    /// Payload classification is still running.
    pub checking: bool,
}

/// Copy the send control should use. The sheet does not recompute this.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureBatchPrompt {
    /// Every sendable row is selected. An empty sheet is covered.
    pub covers_eligible: bool,
    /// Confirmation would send every row the running scan can still admit.
    pub confirms_pending_scan: bool,
}

/// What the native sheet should do next. It does not choose among these.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureBatchEffect {
    /// Stay on the current sheet.
    Idle,
    /// The review task has not finished. Ask again when it has.
    WaitForReview,
    /// Arm Send all on the review that is already running.
    CommitAll {
        /// Identity of this confirmation. Later progress from another attempt is stale.
        attempt: u64,
    },
    /// Stop the scan, then send the already selected sources.
    CancelReview,
    /// Upload these sources, in this order, for this confirmation.
    Send {
        /// Selected source identities that are still eligible.
        source_artifact_ids: Vec<String>,
        /// Identity of this confirmation.
        attempt: u64,
    },
}

/// Progress that changes which sources still need another review.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureBatchProgressKind {
    /// This source did not obtain a durable receipt.
    Failed,
    /// A receipt was stored.
    Sent,
    /// The local payload was removed after that receipt.
    Cleaned,
    /// Any other phase. It does not change the failed set.
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Row {
    id: String,
    eligible: bool,
    checking: bool,
}

/// Selection, stale-row rejection, and send routing for one review sheet.
#[derive(Debug, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate sheet condition the next send effect reads"
)]
pub struct CaptureBatchSession {
    rows: Vec<Row>,
    index: BTreeMap<String, usize>,
    selected: Vec<String>,
    filter: Option<BTreeSet<String>>,
    pending_ids: Vec<String>,
    failed: Vec<String>,
    attempt: u64,
    commit_tries: u8,
    user_adjusted: bool,
    reviewing: bool,
    waiting: bool,
    sending: bool,
    finished: bool,
    stop_requested: bool,
    requires_fresh: bool,
    has_error: bool,
    awaiting_commit: bool,
    resume_eligible: bool,
}

impl CaptureBatchSession {
    /// Starts an empty sheet. Nothing is selected and nothing is sent.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a review. `failed_only` keeps the previous failure set as the
    /// selection filter. Returns false when a review or send already owns the sheet,
    /// or when a failure retry was requested without failures.
    #[must_use]
    pub fn begin_review(&mut self, failed_only: bool) -> bool {
        if self.reviewing || self.sending {
            return false;
        }
        if failed_only && (!self.finished || self.failed.is_empty()) {
            return false;
        }
        self.filter = if failed_only {
            Some(self.failed.iter().cloned().collect())
        } else {
            None
        };
        self.rows.clear();
        self.index.clear();
        self.selected.clear();
        self.pending_ids.clear();
        self.user_adjusted = false;
        self.reviewing = true;
        self.waiting = false;
        self.finished = false;
        self.stop_requested = false;
        self.requires_fresh = true;
        self.has_error = false;
        self.awaiting_commit = false;
        self.resume_eligible = false;
        self.commit_tries = 0;
        true
    }

    /// Records one reviewed row. An eligible row does not regress to a later checking update.
    #[must_use]
    pub fn observe(&mut self, id: &str, eligible: bool, checking: bool) -> bool {
        if self
            .index
            .get(id)
            .and_then(|index| self.rows.get(*index))
            .is_some_and(|row| row.eligible && checking)
        {
            return false;
        }
        self.insert_row(id, eligible, checking);
        self.reconcile_one(id);
        true
    }

    /// Replaces the sheet with the review's final rows.
    ///
    /// A cancelled review, or a row that is still checking, leaves the sheet needing
    /// another review. A completed scan selects every allowed eligible row unless
    /// the user changed the selection.
    pub fn finish_review(&mut self, rows: &[CaptureBatchRow], cancelled: bool) {
        let scan_completed = !cancelled && rows.iter().all(|row| !row.checking);
        self.rows.clear();
        self.index.clear();
        for row in rows {
            self.insert_row(&row.source_artifact_id, row.eligible, row.checking);
        }
        if scan_completed && !self.user_adjusted {
            self.selected.clear();
            let ids = self
                .rows
                .iter()
                .filter(|row| row.eligible && self.allows(&row.id))
                .map(|row| row.id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                self.select(&id);
            }
        } else {
            let kept = {
                let eligible = self.eligible_set();
                self.selected
                    .iter()
                    .filter(|id| eligible.contains(id.as_str()))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            self.selected = kept;
        }
        self.reviewing = false;
        self.requires_fresh = !scan_completed;
        if !scan_completed {
            self.restore_finished_failure_set();
        }
        if scan_completed && !self.awaiting_commit {
            self.stop_requested = false;
            self.failed.clear();
            self.filter = None;
        }
    }

    /// The review failed before it could return rows.
    ///
    /// A previous confirmation's failure set stays retryable.
    pub fn fail_review(&mut self) {
        self.reviewing = false;
        self.waiting = false;
        self.sending = false;
        self.awaiting_commit = false;
        self.resume_eligible = false;
        self.requires_fresh = true;
        self.has_error = true;
        self.restore_finished_failure_set();
    }

    /// The review task ended without a row list and without a storage error.
    ///
    /// A previous confirmation's failure set stays retryable.
    pub fn note_review_interrupted(&mut self) {
        self.reviewing = false;
        self.requires_fresh = true;
        self.restore_finished_failure_set();
    }

    /// Applies one checkbox change. Ineligible rows and a busy send do not change.
    #[must_use]
    pub fn set_selected(&mut self, id: &str, selected: bool) -> bool {
        if self.sending || !self.is_eligible(id) {
            return false;
        }
        self.user_adjusted = true;
        if selected {
            self.select(id);
        } else {
            self.deselect(id);
        }
        true
    }

    /// Chooses Send all, a narrowed send, or no send from the current selection.
    #[must_use]
    pub fn request_send(&mut self) -> CaptureBatchEffect {
        if self.waiting || self.sending || self.selected.is_empty() {
            return CaptureBatchEffect::Idle;
        }
        let requested = self
            .selected
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let sending_all = requested == self.eligible_set();
        if self.reviewing && sending_all {
            self.start_attempt();
            self.pending_ids.clone_from(&self.selected);
            self.resume_eligible = false;
            return CaptureBatchEffect::CommitAll {
                attempt: self.attempt,
            };
        }
        if self.reviewing {
            self.waiting = true;
            self.pending_ids.clone_from(&self.selected);
            self.resume_eligible = false;
            return CaptureBatchEffect::CancelReview;
        }
        if self.finished || self.has_error || self.requires_fresh {
            return CaptureBatchEffect::Idle;
        }
        let ids = self.selected.clone();
        self.start_send(ids)
    }

    /// Whether the send control covers every sendable row, and whether that
    /// confirmation would include rows the scan has not finished.
    #[must_use]
    pub fn prompt(&self) -> CaptureBatchPrompt {
        let covers_eligible = self.selected_count() == self.eligible_count();
        CaptureBatchPrompt {
            covers_eligible,
            confirms_pending_scan: self.reviewing && !self.selected.is_empty() && covers_eligible,
        }
    }

    /// The running review accepted Send all.
    #[must_use]
    pub fn note_commit_accepted(&mut self, attempt: u64) -> bool {
        if attempt != self.attempt || !self.sending {
            return false;
        }
        self.awaiting_commit = true;
        true
    }

    /// The running review did not accept Send all yet.
    #[must_use]
    pub fn note_commit_rejected(&mut self, attempt: u64) -> CaptureBatchEffect {
        if attempt != self.attempt || !self.sending {
            return CaptureBatchEffect::Idle;
        }
        self.commit_tries = self.commit_tries.saturating_add(1);
        if self.reviewing && self.commit_tries < COMMIT_ATTEMPTS {
            return CaptureBatchEffect::CommitAll {
                attempt: self.attempt,
            };
        }
        self.sending = false;
        self.awaiting_commit = false;
        if self.reviewing {
            self.resume_eligible = true;
            return CaptureBatchEffect::WaitForReview;
        }
        self.start_send(self.eligible_ids())
    }

    /// Continues a send that was waiting for the review task to leave.
    #[must_use]
    pub fn review_settled(&mut self) -> CaptureBatchEffect {
        self.reviewing = false;
        if self.awaiting_commit {
            return CaptureBatchEffect::Idle;
        }
        if self.waiting {
            self.waiting = false;
            let ids = self.eligible_ids_in(&self.pending_ids);
            return self.start_send(ids);
        }
        if self.resume_eligible {
            self.resume_eligible = false;
            return self.start_send(self.eligible_ids());
        }
        CaptureBatchEffect::Idle
    }

    /// Send all was accepted, but the review finished without a summary.
    #[must_use]
    pub fn commit_summary_missing(&mut self, attempt: u64) -> CaptureBatchEffect {
        if attempt != self.attempt || !self.awaiting_commit {
            return CaptureBatchEffect::Idle;
        }
        self.awaiting_commit = false;
        self.sending = false;
        if self.reviewing {
            self.resume_eligible = true;
            return CaptureBatchEffect::WaitForReview;
        }
        self.start_send(self.eligible_ids())
    }

    /// Records source progress for the current confirmation only.
    #[must_use]
    pub fn note_progress(
        &mut self,
        id: &str,
        kind: CaptureBatchProgressKind,
        attempt: u64,
    ) -> bool {
        if !self.sending || attempt != self.attempt {
            return false;
        }
        match kind {
            CaptureBatchProgressKind::Failed => self.remember_failure(id),
            CaptureBatchProgressKind::Sent | CaptureBatchProgressKind::Cleaned => {
                self.failed.retain(|failed| failed != id);
            }
            CaptureBatchProgressKind::Other => {}
        }
        true
    }

    /// Applies the terminal batch summary for this confirmation.
    #[must_use]
    pub fn note_summary(&mut self, failed: &[String], cancelled: u64, attempt: u64) -> bool {
        if attempt != self.attempt || (!self.sending && !self.awaiting_commit) {
            return false;
        }
        self.failed = failed.to_vec();
        if cancelled > 0 {
            self.requires_fresh = true;
        }
        self.finished = true;
        self.sending = false;
        self.waiting = false;
        self.awaiting_commit = false;
        self.resume_eligible = false;
        self.has_error = false;
        true
    }

    /// The upload call failed before a summary. The named sources stay failed.
    #[must_use]
    pub fn note_send_failure(&mut self, ids: &[String], attempt: u64) -> bool {
        self.fail_attempt(attempt, ids)
    }

    /// The Send all call failed before a summary. No extra sources are marked failed.
    #[must_use]
    pub fn note_operation_failure(&mut self, attempt: u64) -> bool {
        self.fail_attempt(attempt, &[])
    }

    /// Asks the active send to stop after the source already admitted.
    #[must_use]
    pub fn request_stop(&mut self) -> bool {
        if !self.sending || self.stop_requested {
            return false;
        }
        self.stop_requested = true;
        self.requires_fresh = true;
        true
    }

    /// The send control can be used.
    #[must_use]
    pub fn can_send(&self) -> bool {
        if self.sending
            || self.waiting
            || self.finished
            || self.has_error
            || self.selected.is_empty()
        {
            return false;
        }
        self.reviewing || !self.requires_fresh
    }

    /// Selected identities in selection order.
    #[must_use]
    pub fn selected_ids(&self) -> Vec<String> {
        self.selected.clone()
    }

    /// Whether this source is currently selected.
    #[must_use]
    pub fn is_selected(&self, id: &str) -> bool {
        self.selected.iter().any(|selected| selected == id)
    }

    /// Sources whose latest confirmation did not finish with a receipt.
    #[must_use]
    pub fn failed_ids(&self) -> Vec<String> {
        self.failed.clone()
    }

    /// Sendable rows on the sheet.
    #[must_use]
    pub fn eligible_count(&self) -> u64 {
        u64::try_from(self.rows.iter().filter(|row| row.eligible).count()).unwrap_or(u64::MAX)
    }

    /// Rows that are neither sendable nor still being checked.
    #[must_use]
    pub fn skipped_count(&self) -> u64 {
        u64::try_from(
            self.rows
                .iter()
                .filter(|row| !row.eligible && !row.checking)
                .count(),
        )
        .unwrap_or(u64::MAX)
    }

    /// How many sources are selected.
    #[must_use]
    pub fn selected_count(&self) -> u64 {
        u64::try_from(self.selected.len()).unwrap_or(u64::MAX)
    }

    /// A review task is open.
    #[must_use]
    pub const fn is_reviewing(&self) -> bool {
        self.reviewing
    }

    /// A narrowed send is waiting for the review task to leave.
    #[must_use]
    pub const fn is_waiting(&self) -> bool {
        self.waiting
    }

    /// A confirmation is in progress.
    #[must_use]
    pub const fn is_sending(&self) -> bool {
        self.sending
    }

    /// The latest confirmation reached a summary or a call failure.
    #[must_use]
    pub const fn did_finish(&self) -> bool {
        self.finished
    }

    /// Stop was requested for the confirmation now running.
    #[must_use]
    pub const fn did_request_stop(&self) -> bool {
        self.stop_requested
    }

    /// Another send needs a new review.
    #[must_use]
    pub const fn requires_fresh_review(&self) -> bool {
        self.requires_fresh
    }

    /// Review or send reported an error and no later review cleared it.
    #[must_use]
    pub const fn has_error(&self) -> bool {
        self.has_error
    }

    /// Send all was accepted and its summary has not been applied.
    #[must_use]
    pub const fn is_awaiting_commit(&self) -> bool {
        self.awaiting_commit
    }

    /// Identity of the confirmation now running, or zero before the first one.
    #[must_use]
    pub const fn attempt(&self) -> u64 {
        self.attempt
    }

    fn start_attempt(&mut self) {
        self.attempt = self.attempt.saturating_add(1);
        self.sending = true;
        self.stop_requested = false;
        self.has_error = false;
        self.failed.clear();
        self.awaiting_commit = false;
        self.commit_tries = 0;
    }

    fn start_send(&mut self, ids: Vec<String>) -> CaptureBatchEffect {
        if self.sending || self.finished || self.has_error || ids.is_empty() {
            return CaptureBatchEffect::Idle;
        }
        self.pending_ids.clone_from(&ids);
        self.start_attempt();
        CaptureBatchEffect::Send {
            source_artifact_ids: ids,
            attempt: self.attempt,
        }
    }

    fn restore_finished_failure_set(&mut self) {
        if !self.failed.is_empty() {
            self.finished = true;
        }
    }

    fn fail_attempt(&mut self, attempt: u64, ids: &[String]) -> bool {
        if attempt != self.attempt || (!self.sending && !self.awaiting_commit) {
            return false;
        }
        for id in ids {
            self.remember_failure(id);
        }
        self.has_error = true;
        self.finished = true;
        self.sending = false;
        self.waiting = false;
        self.awaiting_commit = false;
        true
    }

    fn remember_failure(&mut self, id: &str) {
        if !self.failed.iter().any(|failed| failed == id) {
            self.failed.push(id.to_owned());
        }
    }

    fn insert_row(&mut self, id: &str, eligible: bool, checking: bool) {
        if let Some(&index) = self.index.get(id) {
            if let Some(row) = self.rows.get_mut(index) {
                row.eligible = eligible;
                row.checking = checking;
            }
            return;
        }
        let index = self.rows.len();
        self.index.insert(id.to_owned(), index);
        self.rows.push(Row {
            id: id.to_owned(),
            eligible,
            checking,
        });
    }

    fn reconcile_one(&mut self, id: &str) {
        if !self.is_eligible(id) {
            self.deselect(id);
            return;
        }
        if self.user_adjusted || !self.allows(id) {
            return;
        }
        self.select(id);
    }

    fn allows(&self, id: &str) -> bool {
        self.filter
            .as_ref()
            .is_none_or(|filter| filter.contains(id))
    }

    fn is_eligible(&self, id: &str) -> bool {
        self.index
            .get(id)
            .and_then(|index| self.rows.get(*index))
            .is_some_and(|row| row.eligible)
    }

    fn eligible_set(&self) -> BTreeSet<&str> {
        self.rows
            .iter()
            .filter(|row| row.eligible)
            .map(|row| row.id.as_str())
            .collect()
    }

    fn eligible_ids(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| row.eligible)
            .map(|row| row.id.clone())
            .collect()
    }

    fn eligible_ids_in(&self, ids: &[String]) -> Vec<String> {
        let eligible = self.eligible_set();
        ids.iter()
            .filter(|id| eligible.contains(id.as_str()))
            .cloned()
            .collect()
    }

    fn select(&mut self, id: &str) {
        if !self.selected.iter().any(|selected| selected == id) {
            self.selected.push(id.to_owned());
        }
    }

    fn deselect(&mut self, id: &str) {
        self.selected.retain(|selected| selected != id);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaptureBatchEffect, CaptureBatchProgressKind, CaptureBatchRow, CaptureBatchSession,
    };

    fn row(id: &str, eligible: bool, checking: bool) -> CaptureBatchRow {
        CaptureBatchRow {
            source_artifact_id: id.to_owned(),
            eligible,
            checking,
        }
    }

    fn send_ids(effect: CaptureBatchEffect) -> Vec<String> {
        let CaptureBatchEffect::Send {
            source_artifact_ids,
            ..
        } = effect
        else {
            panic!("expected a send");
        };
        source_artifact_ids
    }

    #[test]
    fn checking_row_is_unselected_until_it_becomes_eligible() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("later", false, true));
        assert_eq!(session.selected_ids(), [] as [String; 0]);
        assert!(session.requires_fresh_review());
        assert!(!session.can_send());

        assert!(session.observe("later", true, false));
        session.finish_review(&[row("later", true, false)], false);

        assert_eq!(session.selected_ids(), ["later"]);
        assert!(!session.requires_fresh_review());
        assert!(session.can_send());
        assert!(!session.is_reviewing());
    }

    #[test]
    fn eligible_row_does_not_regress_to_checking() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("ready", true, false));
        assert!(!session.observe("ready", false, true));
        assert!(session.is_selected("ready"));
        assert_eq!(session.eligible_count(), 1);
    }

    #[test]
    fn send_all_while_reviewing_commits_instead_of_uploading_early() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("ready", true, false));
        assert!(session.observe("later", false, true));
        let prompt = session.prompt();
        assert!(prompt.covers_eligible);
        assert!(prompt.confirms_pending_scan);

        let effect = session.request_send();
        let CaptureBatchEffect::CommitAll { attempt } = effect else {
            panic!("expected send all");
        };
        assert!(session.note_commit_accepted(attempt));
        assert!(session.is_awaiting_commit());
        session.finish_review(
            &[row("ready", true, false), row("later", true, false)],
            false,
        );
        assert!(matches!(session.review_settled(), CaptureBatchEffect::Idle));
        assert!(session.note_summary(&[], 0, attempt));
        assert!(session.did_finish());
        assert!(matches!(session.request_send(), CaptureBatchEffect::Idle));
    }

    #[test]
    fn narrowed_selection_cancels_the_scan_and_sends_only_that_source() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("first", true, false));
        assert!(session.observe("second", true, false));
        assert!(session.set_selected("second", false));
        let prompt = session.prompt();
        assert!(!prompt.covers_eligible);
        assert!(!prompt.confirms_pending_scan);

        assert!(matches!(
            session.request_send(),
            CaptureBatchEffect::CancelReview
        ));
        session.finish_review(
            &[row("first", true, false), row("second", true, false)],
            false,
        );
        assert_eq!(send_ids(session.review_settled()), ["first".to_owned()]);
    }

    #[test]
    fn rejected_send_all_retries_then_sends_the_finished_eligible_rows() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("ready", true, false));
        let effect = session.request_send();
        let CaptureBatchEffect::CommitAll { attempt } = effect else {
            panic!("expected send all");
        };
        let mut effect = CaptureBatchEffect::CommitAll { attempt };
        for _ in 0..5 {
            let CaptureBatchEffect::CommitAll { attempt } = effect else {
                panic!("expected another commit");
            };
            effect = session.note_commit_rejected(attempt);
        }
        assert!(matches!(effect, CaptureBatchEffect::WaitForReview));
        assert!(session.observe("later", true, false));
        session.finish_review(
            &[row("ready", true, false), row("later", true, false)],
            false,
        );
        assert_eq!(
            send_ids(session.review_settled()),
            ["ready".to_owned(), "later".to_owned()]
        );
    }

    #[test]
    fn stale_progress_does_not_replace_the_current_attempt() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("source", true, false));
        session.finish_review(&[row("source", true, false)], false);
        let effect = session.request_send();
        let CaptureBatchEffect::Send { attempt, .. } = effect else {
            panic!("expected send");
        };
        assert!(session.note_progress("source", CaptureBatchProgressKind::Failed, attempt));
        assert_eq!(session.failed_ids(), ["source"]);
        assert!(session.note_summary(&["source".to_owned()], 0, attempt));
        assert_eq!(session.failed_ids(), ["source"]);

        assert!(session.begin_review(true));
        assert!(session.observe("source", true, false));
        assert!(session.observe("other", true, false));
        session.finish_review(
            &[row("source", true, false), row("other", true, false)],
            false,
        );
        assert_eq!(session.selected_ids(), ["source"]);

        let effect = session.request_send();
        let CaptureBatchEffect::Send { attempt: next, .. } = effect else {
            panic!("expected the retry send");
        };
        assert_ne!(next, attempt);
        assert!(session.note_progress("source", CaptureBatchProgressKind::Sent, next));
        assert!(!session.note_progress("source", CaptureBatchProgressKind::Failed, attempt));
        assert_eq!(session.failed_ids(), Vec::<String>::new());
        assert!(session.note_summary(&[], 0, next));
        assert_eq!(session.failed_ids(), [] as [String; 0]);
    }

    #[test]
    fn failed_only_review_remains_retryable_after_the_review_fails() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("source", true, false));
        session.finish_review(&[row("source", true, false)], false);
        let effect = session.request_send();
        let CaptureBatchEffect::Send { attempt, .. } = effect else {
            panic!("expected send");
        };
        assert!(session.note_summary(&["source".to_owned()], 0, attempt));

        assert!(session.begin_review(true));
        session.fail_review();
        assert!(session.did_finish());
        assert_eq!(session.failed_ids(), ["source"]);
        assert!(session.has_error());
        assert!(session.begin_review(true));
        session.note_review_interrupted();
        assert!(session.did_finish());
        assert!(session.begin_review(true));
        session.finish_review(&[row("source", true, false)], true);

        let mut fresh = CaptureBatchSession::new();
        assert!(fresh.begin_review(false));
        fresh.fail_review();
        assert!(!fresh.did_finish());
        assert!(!fresh.begin_review(true));

        assert!(session.did_finish());
        assert_eq!(session.failed_ids(), ["source"]);
        assert!(session.begin_review(true));
        session.finish_review(&[row("source", true, true)], false);
        assert!(session.did_finish());
        assert_eq!(session.failed_ids(), ["source"]);
        assert!(session.begin_review(true));
    }

    #[test]
    fn stop_requires_a_fresh_review_before_another_send() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("source", true, false));
        session.finish_review(&[row("source", true, false)], false);
        let effect = session.request_send();
        let CaptureBatchEffect::Send { attempt, .. } = effect else {
            panic!("expected send");
        };
        assert!(session.request_stop());
        assert!(!session.request_stop());
        assert!(session.note_summary(&[], 1, attempt));
        assert!(session.requires_fresh_review());
        assert!(matches!(session.request_send(), CaptureBatchEffect::Idle));

        assert!(session.begin_review(false));
        assert!(session.observe("source", true, false));
        session.finish_review(&[row("source", true, false)], false);
        assert!(!session.requires_fresh_review());
        assert!(session.can_send());
        assert!(!session.is_sending());
    }

    #[test]
    fn user_adjusted_selection_survives_the_completed_scan() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("first", true, false));
        assert!(session.observe("second", true, false));
        assert!(session.set_selected("second", false));

        session.finish_review(
            &[
                row("first", true, false),
                row("second", true, false),
                row("third", true, false),
            ],
            false,
        );

        assert_eq!(session.selected_ids(), ["first"]);
        assert!(session.can_send());
    }

    #[test]
    fn cancelled_or_checking_review_stays_stale() {
        let mut session = CaptureBatchSession::new();
        assert!(session.begin_review(false));
        assert!(session.observe("ready", true, false));
        session.finish_review(
            &[row("ready", true, false), row("later", false, true)],
            false,
        );
        assert!(session.requires_fresh_review());
        assert!(matches!(session.request_send(), CaptureBatchEffect::Idle));

        assert!(session.begin_review(false));
        assert!(session.observe("ready", true, false));
        session.finish_review(&[row("ready", true, false)], true);
        assert!(session.requires_fresh_review());
        assert!(!session.can_send());
    }
}
