//! FIFO ownership for music observations and durable history effects.

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, PoisonError},
    task::{Context, Poll, Waker},
};

use cutout_music::MusicHistoryTransition;

const MAX_PENDING: usize = 64;

/// Stable identity for one recording generation captured by an observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingIdentity {
    /// Ride identity.
    pub ride_id: String,
    /// Input generation.
    pub generation: u64,
}

/// Application failure while coordinating required music-history work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionError {
    /// The queue, lease, or Stop fence no longer owns the requested operation.
    Stale,
    /// Earlier required history work is unresolved or incomplete.
    Incomplete,
    /// A Stop fence is already active or observations remain pending.
    Pending,
    /// Admission identifiers have been exhausted.
    Exhausted,
}

/// Failure returned while classifying an admitted observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassificationError<E> {
    /// The lease no longer owned the requested operation.
    Admission(AdmissionError),
    /// The application's observation operation failed.
    Operation(E),
}

/// Result of admitting one callback before asynchronous identity reads.
#[derive(Clone, Debug)]
pub enum Admission {
    /// The callback owns a FIFO lease.
    Admitted(Arc<ObservationLease>),
    /// The bounded queue is full.
    Full,
    /// Stable identities are exhausted.
    Exhausted,
}

/// State of one observation lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseState {
    /// An earlier observation owns the required head.
    Pending,
    /// This lease owns the next observation operation.
    Ready,
    /// The lease was retired or released.
    Retired,
}

/// Application-owned queue and receipt coordinator.
#[derive(Debug, Default)]
pub struct ObservationQueue {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct State {
    next_id: u64,
    epoch: u64,
    publication_id: Option<u64>,
    entries: VecDeque<Entry>,
    stop_id: u64,
    stop: Option<StopState>,
    failure: bool,
}

#[derive(Debug)]
struct Entry {
    id: u64,
    owner: Option<u64>,
    waker: Option<Waker>,
    phase: Phase,
}

#[derive(Debug)]
enum Phase {
    Waiting,
    Classified {
        recording: Option<RecordingIdentity>,
        transition: Option<Box<MusicHistoryTransition>>,
    },
    Writing,
    Receipt(HistoryReceipt),
    Settled,
    Failed,
}

impl Phase {
    fn terminal(&self) -> bool {
        match self {
            Self::Settled | Self::Failed => true,
            Self::Waiting | Self::Classified { .. } | Self::Writing | Self::Receipt(_) => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct StopState {
    id: u64,
    epoch: u64,
    cutoff: Option<u64>,
    failed: bool,
    failure_observed: bool,
}

/// Authoritative persistence result for one attempted history write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryReceipt {
    /// Durable insertion succeeded.
    Recorded,
    /// The event was already the newest stored event.
    Duplicate,
    /// History was disabled by the user.
    Disabled,
    /// The event was older than stored history.
    OutOfOrder,
    /// The recording had closed before the write.
    RideNotOpen,
    /// The persistence event limit was reached.
    Full,
}

/// Fence covering the observations admitted before a Stop command.
#[derive(Debug)]
pub struct StopFence {
    queue: Arc<ObservationQueue>,
    id: u64,
}

/// An admitted observation and its required history receipt state.
#[derive(Debug)]
pub struct ObservationLease {
    queue: Arc<ObservationQueue>,
    owner: Option<u64>,
    epoch: u64,
    id: u64,
}

/// Admission handle retained across native asynchronous work.
#[derive(Debug)]
pub struct ObservationRequest {
    queue: Arc<ObservationQueue>,
    epoch: u64,
    admission: Admission,
}

impl ObservationQueue {
    /// Whether no admission or Stop identity has been consumed.
    pub fn is_unused(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.next_id == 0 && state.stop_id == 0 && state.epoch == 0
    }

    /// Checks whether a lease belongs to this queue instance.
    pub fn owns(&self, lease: &ObservationLease) -> bool {
        std::ptr::eq(self, lease.queue.as_ref())
    }
    /// Admits a callback before any native asynchronous operation.
    pub fn admit(self: &Arc<Self>, owner: Option<u64>) -> Arc<ObservationRequest> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let epoch = state.epoch;
        let admission = match state.next_id.checked_add(1) {
            None => Admission::Exhausted,
            Some(_) if state.entries.len() >= MAX_PENDING => Admission::Full,
            Some(next_id) => {
                let id = state.next_id;
                state.next_id = next_id;
                state.entries.push_back(Entry {
                    id,
                    owner,
                    waker: None,
                    phase: Phase::Waiting,
                });
                Admission::Admitted(Arc::new(ObservationLease {
                    queue: Arc::clone(self),
                    owner,
                    epoch,
                    id,
                }))
            }
        };
        Arc::new(ObservationRequest {
            queue: Arc::clone(self),
            epoch,
            admission,
        })
    }

    /// Starts a Stop fence covering all observations accepted so far.
    ///
    /// # Errors
    /// Returns `Pending` if another fence is active, or `Exhausted` if fence IDs are exhausted.
    pub fn begin_stop(self: &Arc<Self>) -> Result<StopFence, AdmissionError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.stop.is_some() {
            return Err(AdmissionError::Pending);
        }
        let id = state.stop_id;
        state.stop_id = id.checked_add(1).ok_or(AdmissionError::Exhausted)?;
        state.stop = Some(StopState {
            id,
            epoch: state.epoch,
            cutoff: state.entries.back().map(|entry| entry.id),
            failed: std::mem::take(&mut state.failure),
            failure_observed: false,
        });
        Ok(StopFence {
            queue: Arc::clone(self),
            id,
        })
    }

    /// Rejects synchronous Stop while admitted work remains unresolved.
    ///
    /// # Errors
    /// Returns `Pending` while work or a fence is active, or `Incomplete` for unobserved failure.
    pub fn require_quiescent_stop(&self) -> Result<(), AdmissionError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.stop.is_some() || state.entries.iter().any(|entry| !entry.phase.terminal()) {
            return Err(AdmissionError::Pending);
        }
        if std::mem::take(&mut state.failure) {
            return Err(AdmissionError::Incomplete);
        }
        Ok(())
    }

    /// Atomically retires queue ownership for one provider owner.
    pub fn retire_owner<T>(&self, owner: u64, operation: impl FnOnce() -> (bool, T)) -> T {
        self.retire_matching(Some(owner), operation)
    }

    /// Retires all queue ownership and advances the global epoch.
    pub fn retire_all(&self) {
        self.retire_matching(None, || (true, ()));
    }

    fn retire_matching<T>(&self, owner: Option<u64>, operation: impl FnOnce() -> (bool, T)) -> T {
        let (wake, result) = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let (retire, result) = operation();
            let mut wake = Vec::new();
            if retire {
                let epoch = state.epoch;
                let count = state.entries.len();
                for _ in 0..count {
                    let Some(mut entry) = state.entries.pop_front() else {
                        break;
                    };
                    let matches = owner.is_none_or(|id| entry.owner == Some(id));
                    if matches {
                        if !entry.phase.terminal() {
                            mark_failure(&mut state, epoch, entry.id);
                        }
                        if let Some(waker) = entry.waker.take() {
                            wake.push(waker);
                        }
                    } else {
                        state.entries.push_back(entry);
                    }
                }
                if owner.is_none() {
                    state.epoch = state.epoch.saturating_add(1);
                }
                if let Some(waker) = head_waker(&mut state) {
                    wake.push(waker);
                }
            }
            (wake, result)
        };
        self.changed.notify_all();
        for waker in wake {
            waker.wake();
        }
        result
    }

    /// Runs a synchronous operation only while the lease owns the FIFO head.
    ///
    /// # Errors
    /// Returns `Stale` when this queue does not own the lease or it is not the FIFO head.
    pub fn with_ready<T>(
        &self,
        lease: &ObservationLease,
        operation: impl FnOnce() -> T,
    ) -> Result<T, AdmissionError> {
        if !std::ptr::eq(self, lease.queue.as_ref()) {
            return Err(AdmissionError::Stale);
        }
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !ready(&state, lease.epoch, lease.id) {
            return Err(AdmissionError::Stale);
        }
        let waiting = match state
            .entries
            .iter()
            .find(|entry| entry.id == lease.id)
            .map(|entry| &entry.phase)
        {
            Some(Phase::Waiting) => true,
            Some(
                Phase::Classified { .. }
                | Phase::Writing
                | Phase::Receipt(_)
                | Phase::Settled
                | Phase::Failed,
            )
            | None => false,
        };
        if !waiting {
            return Err(AdmissionError::Stale);
        }
        state.publication_id = Some(lease.id);
        Ok(operation())
    }
}

impl ObservationRequest {
    /// Returns the admission result.
    #[must_use]
    pub fn admission(&self) -> Admission {
        self.admission.clone()
    }
    /// Whether this request can still publish replaceable presentation.
    pub fn is_current(&self) -> bool {
        match &self.admission {
            Admission::Admitted(lease) => publication_current(
                &lease
                    .queue
                    .state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
                lease.epoch,
                lease.id,
            ),
            Admission::Full | Admission::Exhausted => {
                self.queue
                    .state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .epoch
                    == self.epoch
            }
        }
    }
    /// Settles required effects after a durable history receipt.
    ///
    /// # Errors
    /// Returns `Stale` if the lease cannot settle, or `Incomplete` while required work is unresolved.
    pub fn settle(&self) -> Result<(), AdmissionError> {
        match &self.admission {
            Admission::Admitted(lease) => lease.settle(),
            _ => Err(AdmissionError::Stale),
        }
    }
    /// Marks required processing as failed.
    pub fn fail_required_effects(&self) {
        if let Admission::Admitted(lease) = &self.admission {
            lease.required_failed();
        }
    }
    /// Releases the lease; unsettled required work remains a Stop failure.
    pub fn release(&self) {
        if let Admission::Admitted(lease) = &self.admission {
            lease.release();
        }
    }
}

impl ObservationLease {
    /// Waits until this receipt owns the FIFO head or has been retired.
    ///
    /// # Errors
    /// Returns `Stale` if the lease has been retired or no longer belongs to this queue epoch.
    pub async fn wait(&self) -> Result<LeaseState, AdmissionError> {
        LeaseWait {
            queue: Arc::clone(&self.queue),
            epoch: self.epoch,
            id: self.id,
            waker: None,
        }
        .await
    }
    /// Reports current FIFO ownership.
    pub fn state(&self) -> LeaseState {
        let state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.epoch != self.epoch
            || !state
                .entries
                .iter()
                .any(|entry| entry.id == self.id && !entry.phase.terminal())
        {
            LeaseState::Retired
        } else if ready(&state, self.epoch, self.id) {
            LeaseState::Ready
        } else {
            LeaseState::Pending
        }
    }
    /// Runs classification and retains its optional history transition.
    ///
    /// # Errors
    /// Returns an admission error if the lease is stale, or the operation's error on failure.
    pub fn classify<T, E>(
        &self,
        recording: Option<RecordingIdentity>,
        operation: impl FnOnce() -> Result<(T, Option<MusicHistoryTransition>), E>,
    ) -> Result<T, ClassificationError<E>> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !ready(&state, self.epoch, self.id) {
            return Err(ClassificationError::Admission(AdmissionError::Stale));
        }
        let index = state
            .entries
            .iter()
            .position(|entry| entry.id == self.id)
            .ok_or(ClassificationError::Admission(AdmissionError::Stale))?;
        let waiting = match &state.entries[index].phase {
            Phase::Waiting => true,
            Phase::Classified { .. }
            | Phase::Writing
            | Phase::Receipt(_)
            | Phase::Settled
            | Phase::Failed => false,
        };
        if !waiting {
            return Err(ClassificationError::Admission(AdmissionError::Stale));
        }
        let (value, transition) = match operation() {
            Ok(value) => value,
            Err(error) => {
                state.entries[index].phase = Phase::Failed;
                mark_failure(&mut state, self.epoch, self.id);
                self.queue.changed.notify_all();
                return Err(ClassificationError::Operation(error));
            }
        };
        state.entries[index].phase = Phase::Classified {
            recording,
            transition: transition.map(Box::new),
        };
        state.publication_id = Some(self.id);
        let wake = head_waker(&mut state);
        drop(state);
        self.queue.changed.notify_all();
        if let Some(waker) = wake {
            waker.wake();
        }
        Ok(value)
    }
    /// Claims a recorded transition exactly once for persistence admission.
    ///
    /// # Errors
    /// Returns `Stale` when this lease is not ready or has no recorded transition to claim.
    pub fn claim_write(
        &self,
    ) -> Result<(RecordingIdentity, MusicHistoryTransition), AdmissionError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !ready(&state, self.epoch, self.id) {
            return Err(AdmissionError::Stale);
        }
        let entry = state
            .entries
            .iter_mut()
            .find(|entry| entry.id == self.id)
            .ok_or(AdmissionError::Stale)?;
        let previous = std::mem::replace(&mut entry.phase, Phase::Waiting);
        match previous {
            Phase::Classified {
                recording: Some(recording),
                transition: Some(transition),
            } => {
                entry.phase = Phase::Writing;
                Ok((recording, *transition))
            }
            other => {
                entry.phase = other;
                Err(AdmissionError::Stale)
            }
        }
    }
    /// Records the authoritative persistence receipt for a claimed write.
    pub fn history_receipt(&self, receipt: HistoryReceipt) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if ready(&state, self.epoch, self.id)
            && let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == self.id)
            && let Phase::Writing = entry.phase
        {
            entry.phase = Phase::Receipt(receipt);
        }
    }
    /// Marks the claimed history write as failed.
    pub fn history_failed(&self) {
        self.fail_phase(PhaseTag::Writing);
    }
    /// Marks observation classification as failed.
    pub fn classification_failed(&self) {
        self.fail_phase(PhaseTag::Waiting);
    }
    fn fail_phase(&self, expected: PhaseTag) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !ready(&state, self.epoch, self.id) {
            return;
        }
        let eligible = expected.matches(&Phase::Waiting);
        let Some(index) = state.entries.iter().position(|entry| entry.id == self.id) else {
            return;
        };
        if expected.matches(&state.entries[index].phase) {
            state.entries[index].phase = Phase::Failed;
            if eligible {
                state.publication_id = Some(self.id);
            }
            mark_failure(&mut state, self.epoch, self.id);
        }
        let wake = head_waker(&mut state);
        drop(state);
        self.queue.changed.notify_all();
        if let Some(waker) = wake {
            waker.wake();
        }
    }
    /// Settles the required part of this receipt after receipt/no-write proof.
    ///
    /// # Errors
    /// Returns `Incomplete` for unresolved or failed required work and `Stale` for retired leases.
    pub fn settle(&self) -> Result<(), AdmissionError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.epoch == self.epoch
            && state.entries.iter().any(|entry| {
                entry.id == self.id
                    && match entry.phase {
                        Phase::Failed => true,
                        Phase::Waiting
                        | Phase::Classified { .. }
                        | Phase::Writing
                        | Phase::Receipt(_)
                        | Phase::Settled => false,
                    }
            })
        {
            return Err(AdmissionError::Incomplete);
        }
        if !ready(&state, self.epoch, self.id) {
            return Err(AdmissionError::Stale);
        }
        let index = state
            .entries
            .iter()
            .position(|entry| entry.id == self.id)
            .ok_or(AdmissionError::Stale)?;
        let accepted = match &state.entries[index].phase {
            Phase::Classified {
                recording: None, ..
            }
            | Phase::Classified {
                transition: None, ..
            }
            | Phase::Receipt(
                HistoryReceipt::Recorded | HistoryReceipt::Duplicate | HistoryReceipt::Disabled,
            ) => true,
            Phase::Waiting
            | Phase::Classified { .. }
            | Phase::Writing
            | Phase::Receipt(
                HistoryReceipt::OutOfOrder | HistoryReceipt::RideNotOpen | HistoryReceipt::Full,
            )
            | Phase::Settled
            | Phase::Failed => false,
        };
        let rejected_receipt = match state.entries[index].phase {
            Phase::Receipt(_) => true,
            Phase::Waiting
            | Phase::Classified { .. }
            | Phase::Writing
            | Phase::Settled
            | Phase::Failed => false,
        };
        if let Phase::Settled = state.entries[index].phase {
            return Err(AdmissionError::Stale);
        }
        if !accepted {
            if rejected_receipt {
                state.entries[index].phase = Phase::Failed;
                mark_failure(&mut state, self.epoch, self.id);
            }
            return Err(AdmissionError::Incomplete);
        }
        state.entries[index].phase = Phase::Settled;
        let wake = head_waker(&mut state);
        drop(state);
        self.queue.changed.notify_all();
        if let Some(waker) = wake {
            waker.wake();
        }
        Ok(())
    }
    /// Releases this lease, recording incomplete required work if unsettled.
    pub fn release(&self) {
        let wake = {
            let mut state = self
                .queue
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.epoch != self.epoch {
                return;
            }
            let Some(index) = state.entries.iter().position(|entry| entry.id == self.id) else {
                return;
            };
            let Some(entry) = state.entries.remove(index) else {
                return;
            };
            if !entry.phase.terminal() {
                mark_failure(&mut state, self.epoch, self.id);
            }
            (entry.waker, head_waker(&mut state))
        };
        self.queue.changed.notify_all();
        for waker in [wake.0, wake.1].into_iter().flatten() {
            waker.wake();
        }
    }
    /// Fails this lease's required operations.
    pub fn required_failed(&self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.epoch != self.epoch {
            return;
        }
        let Some(index) = state.entries.iter().position(|entry| entry.id == self.id) else {
            return;
        };
        if state.entries[index].phase.terminal() {
            return;
        }
        let eligible = ready(&state, self.epoch, self.id);
        state.entries[index].phase = Phase::Failed;
        if eligible {
            state.publication_id = Some(self.id);
        }
        mark_failure(&mut state, self.epoch, self.id);
        let wake = head_waker(&mut state);
        drop(state);
        self.queue.changed.notify_all();
        if let Some(waker) = wake {
            waker.wake();
        }
    }
    /// Checks that this lease belongs to one provider owner.
    #[must_use]
    pub fn owned_by(&self, owner: u64) -> bool {
        self.owner == Some(owner)
    }
}

impl Drop for ObservationLease {
    fn drop(&mut self) {
        self.release();
    }
}

impl StopFence {
    /// Blocks until the covered FIFO prefix settles and returns its receipt result.
    ///
    /// # Errors
    /// Returns `Stale` if the fence has been released, or `Incomplete` for failed covered work.
    pub fn wait_result(&self) -> Result<(), AdmissionError> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loop {
            let stop = state
                .stop
                .filter(|stop| stop.id == self.id)
                .ok_or(AdmissionError::Stale)?;
            let pending = stop.epoch == state.epoch
                && state.entries.iter().any(|entry| {
                    stop.cutoff.is_some_and(|cutoff| entry.id <= cutoff) && !entry.phase.terminal()
                });
            if !pending {
                if stop.failed {
                    if let Some(active) = &mut state.stop {
                        active.failure_observed = true;
                    }
                    return Err(AdmissionError::Incomplete);
                }
                return Ok(());
            }
            state = self
                .queue
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl Drop for StopFence {
    fn drop(&mut self) {
        let wake = {
            let mut state = self
                .queue
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.stop.is_none_or(|stop| stop.id != self.id) {
                return;
            }
            if let Some(stop) = state.stop.take()
                && !stop.failure_observed
                && stop.failed
            {
                state.failure = true;
            }
            head_waker(&mut state)
        };
        self.queue.changed.notify_all();
        if let Some(waker) = wake {
            waker.wake();
        }
    }
}

#[derive(Clone, Copy)]
enum PhaseTag {
    Waiting,
    Writing,
}
impl PhaseTag {
    fn matches(self, phase: &Phase) -> bool {
        match (self, phase) {
            (Self::Waiting, Phase::Waiting) | (Self::Writing, Phase::Writing) => true,
            _ => false,
        }
    }
}

fn mark_failure(state: &mut State, epoch: u64, id: u64) {
    if let Some(stop) = &mut state.stop
        && stop.epoch == epoch
        && stop.cutoff.is_some_and(|cutoff| id <= cutoff)
    {
        stop.failed = true;
        return;
    }
    state.failure = true;
}
fn required_head(state: &State) -> Option<&Entry> {
    state.entries.iter().find(|entry| !entry.phase.terminal())
}
fn blocked(state: &State, id: u64) -> bool {
    state.stop.is_some_and(|stop| {
        stop.epoch != state.epoch || stop.cutoff.is_none_or(|cutoff| id > cutoff)
    })
}
fn ready(state: &State, epoch: u64, id: u64) -> bool {
    state.epoch == epoch
        && required_head(state).is_some_and(|entry| entry.id == id)
        && !blocked(state, id)
}
fn publication_current(state: &State, epoch: u64, id: u64) -> bool {
    state.epoch == epoch
        && state.entries.iter().any(|entry| entry.id == id)
        && (ready(state, epoch, id) || state.publication_id == Some(id))
}
fn head_waker(state: &mut State) -> Option<Waker> {
    let id = required_head(state)?.id;
    if blocked(state, id) {
        return None;
    }
    state
        .entries
        .iter_mut()
        .find(|entry| entry.id == id)?
        .waker
        .take()
}

struct LeaseWait {
    queue: Arc<ObservationQueue>,
    epoch: u64,
    id: u64,
    waker: Option<Waker>,
}
impl Future for LeaseWait {
    type Output = Result<LeaseState, AdmissionError>;
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let queue = Arc::clone(&self.queue);
        let mut state = queue.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.epoch != self.epoch {
            return Poll::Ready(Ok(LeaseState::Retired));
        }
        if ready(&state, self.epoch, self.id) {
            return Poll::Ready(Ok(LeaseState::Ready));
        }
        let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == self.id) else {
            return Poll::Ready(Ok(LeaseState::Retired));
        };
        if entry.phase.terminal() {
            return Poll::Ready(Ok(LeaseState::Retired));
        }
        if self.waker.is_none() && entry.waker.is_some() {
            return Poll::Ready(Err(AdmissionError::Stale));
        }
        let waker = context.waker().clone();
        entry.waker = Some(waker.clone());
        self.waker = Some(waker);
        Poll::Pending
    }
}
impl Drop for LeaseWait {
    fn drop(&mut self) {
        let Some(own) = &self.waker else {
            return;
        };
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.epoch == self.epoch
            && let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == self.id)
            && entry
                .waker
                .as_ref()
                .is_some_and(|waker| waker.will_wake(own))
        {
            entry.waker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_admission_serializes_classification_and_receipts() {
        let queue = Arc::new(ObservationQueue::default());
        let first = queue.admit(None);
        let later = queue.admit(None);
        let (Admission::Admitted(first_lease), Admission::Admitted(later_lease)) =
            (first.admission(), later.admission())
        else {
            panic!("both admissions fit");
        };
        assert_eq!(first_lease.state(), LeaseState::Ready);
        assert_eq!(later_lease.state(), LeaseState::Pending);
        first_lease
            .classify(None, || Ok::<_, std::convert::Infallible>(((), None)))
            .expect("non-recordable observation");
        first.settle().expect("required work complete");
        assert_eq!(later_lease.state(), LeaseState::Ready);
        later_lease
            .classify(None, || Ok::<_, std::convert::Infallible>(((), None)))
            .expect("next observation");
        later.settle().expect("required work complete");
    }

    #[test]
    fn stop_waits_for_durable_receipt_and_rejects_unaccepted_outcome() {
        let queue = Arc::new(ObservationQueue::default());
        let request = queue.admit(None);
        let Admission::Admitted(lease) = request.admission() else {
            panic!("admitted");
        };
        let fence = queue.begin_stop().expect("stop fence");
        lease
            .classify(
                Some(RecordingIdentity {
                    ride_id: "ride".into(),
                    generation: 3,
                }),
                || Ok::<_, std::convert::Infallible>(((), None)),
            )
            .expect("classification");
        request.settle().expect("no write required");
        assert_eq!(fence.wait_result(), Ok(()));
    }

    #[test]
    fn observed_failure_is_returned_once_and_explicit_retry_can_succeed() {
        let queue = Arc::new(ObservationQueue::default());
        let request = queue.admit(None);
        let fence = queue.begin_stop().expect("stop fence");
        request.fail_required_effects();
        assert_eq!(fence.wait_result(), Err(AdmissionError::Incomplete));
        request.release();
        drop(fence);
        assert_eq!(
            queue.begin_stop().expect("explicit retry").wait_result(),
            Ok(())
        );
    }

    #[test]
    fn provider_retirement_releases_only_its_leases_and_latches_failure() {
        let queue = Arc::new(ObservationQueue::default());
        let retired = queue.admit(Some(1));
        let retained = queue.admit(Some(2));
        queue.retire_owner(1, || (true, ()));
        let Admission::Admitted(retired_lease) = retired.admission() else {
            panic!("admitted");
        };
        let Admission::Admitted(retained_lease) = retained.admission() else {
            panic!("admitted");
        };
        assert_eq!(retired_lease.state(), LeaseState::Retired);
        assert_eq!(retained_lease.state(), LeaseState::Ready);
    }

    #[test]
    fn full_queue_does_not_allocate_an_accepted_receipt() {
        let queue = Arc::new(ObservationQueue::default());
        let mut requests = Vec::new();
        for _ in 0..MAX_PENDING {
            requests.push(queue.admit(None));
        }
        assert!(matches!(queue.admit(None).admission(), Admission::Full));
        for request in requests {
            request.release();
        }
    }
}
