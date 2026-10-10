//! Bounded ownership before a music callback performs an asynchronous identity read.

use crate::{
    MobileCaptureWriteOutcomeDto, MobileMusicCaptureTarget, MobileMusicHistoryTransition,
    MobileMusicObservationDecision, MobileMusicTimelineOutcomeDto,
    MobileMusicTimelineRecordResultDto, MobileRideMapCore, MobileRideMapCoreErrorDto,
    MobileRideMapRecordingTokenDto, ride_maps,
};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, PoisonError},
    task::{Context, Poll, Waker},
};

const MAX_PENDING_MUSIC_OBSERVATIONS: usize = 64;

#[derive(Debug, Default)]
pub(crate) struct MusicObservationAdmissionQueue {
    state: Mutex<AdmissionState>,
    changed: Condvar,
}
#[derive(Debug, Default)]
struct AdmissionState {
    next_id: u64,
    publication_id: Option<u64>,
    epoch: u64,
    order: VecDeque<AdmissionEntry>,
    next_stop_id: u64,
    stop: Option<StopObligation>,
    failure: Option<IncompleteReason>,
}
#[derive(Debug)]
struct StopObligation {
    id: u64,
    epoch: u64,
    cutoff: Option<u64>,
    failure: Option<IncompleteReason>,
    failure_observed: bool,
}
#[derive(Clone, Copy, Debug)]
enum IncompleteReason {
    Abandoned,
    HistoryUnsettled,
    CaptureRejected,
    ClassificationFailed,
    RequiredFailed,
    ProviderRetired,
}
#[derive(Debug)]
struct AdmissionEntry {
    id: u64,
    owner: Option<Arc<()>>,
    waker: Option<Waker>,
    phase: ObservationPhase,
}
#[derive(Debug)]
struct OriginalTransition {
    transition: Box<MobileMusicHistoryTransition>,
}
#[derive(Debug)]
enum ObservationPhase {
    Waiting,
    Observed {
        recording: Option<MobileRideMapRecordingTokenDto>,
        transition: Option<Box<MobileMusicHistoryTransition>>,
        failed: bool,
    },
    Writing(OriginalTransition),
    HistorySettled {
        original: OriginalTransition,
        outcome: MobileMusicTimelineOutcomeDto,
    },
    Settled,
    Failed,
}

impl ObservationPhase {
    fn is_terminal(&self) -> bool {
        match self {
            Self::Settled | Self::Failed => true,
            _ => false,
        }
    }
}

impl AdmissionState {
    fn required_head(&self) -> Option<&AdmissionEntry> {
        self.order.iter().find(|entry| !entry.phase.is_terminal())
    }
    fn publication_current(&self, epoch: u64, id: u64) -> bool {
        self.epoch == epoch
            && self.order.iter().any(|entry| entry.id == id)
            && (self.ready(epoch, id) || self.publication_id == Some(id))
    }
    fn blocked(&self, id: u64) -> bool {
        self.stop.as_ref().is_some_and(|stop| {
            stop.epoch != self.epoch || stop.cutoff.is_none_or(|cutoff| id > cutoff)
        })
    }
    fn ready(&self, epoch: u64, id: u64) -> bool {
        self.epoch == epoch
            && self.required_head().is_some_and(|entry| entry.id == id)
            && !self.blocked(id)
    }
    fn fail(&mut self, epoch: u64, id: u64, reason: IncompleteReason) {
        if let Some(stop) = &mut self.stop
            && stop.epoch == epoch
            && stop.cutoff.is_some_and(|cutoff| id <= cutoff)
        {
            stop.failure.get_or_insert(reason);
        } else {
            self.failure.get_or_insert(reason);
        }
    }
    fn head_waker(&mut self) -> Option<Waker> {
        let id = self.required_head()?.id;
        if self.blocked(id) {
            return None;
        }
        self.order
            .iter_mut()
            .find(|entry| entry.id == id)?
            .waker
            .take()
    }
}

/// Admission precedes any native asynchronous operation.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum MobileMusicObservationAdmission {
    /// Exact ownership retained until settlement or abandonment.
    Admitted {
        lease: Arc<MobileMusicObservationLease>,
    },
    /// The bounded queue cannot accept this observation yet.
    Full,
    /// Receipt identities are exhausted; no observation was accepted.
    Exhausted,
}
/// Whether one receipt may access observation correlation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicObservationLeaseState {
    /// An earlier observation or Stop still owns recording admission.
    Pending,
    /// This receipt exclusively owns the next observation.
    Ready,
    /// Provider retirement, required settlement, or release ended classification ownership.
    Retired,
}
/// Lightweight admission result and post-await ownership fence.
#[derive(Debug, uniffi::Object)]
pub struct MobileMusicObservationRequest {
    queue: Arc<MusicObservationAdmissionQueue>,
    epoch: u64,
    admission: MobileMusicObservationAdmission,
}
#[uniffi::export]
impl MobileMusicObservationRequest {
    #[must_use]
    pub fn admission(&self) -> MobileMusicObservationAdmission {
        self.admission.clone()
    }
    /// Whether this retained request can publish replaceable native presentation.
    /// Required settlement permits the next classification without releasing optional readback.
    #[must_use]
    pub fn is_current(&self) -> bool {
        match &self.admission {
            MobileMusicObservationAdmission::Admitted { lease } => lease
                .queue
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .publication_current(lease.epoch, lease.id),
            MobileMusicObservationAdmission::Full | MobileMusicObservationAdmission::Exhausted => {
                self.queue
                    .state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .epoch
                    == self.epoch
            }
        }
    }
    /// Confirms authoritative history settlement and the required native capture effect.
    ///
    /// # Errors
    /// Returns incomplete effects or stale observation ownership.
    pub fn settle(
        &self,
        capture: Option<MobileCaptureWriteOutcomeDto>,
    ) -> Result<(), MobileRideMapCoreErrorDto> {
        let MobileMusicObservationAdmission::Admitted { lease } = &self.admission else {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        };
        lease.settle(capture)
    }
    /// Reports a definitive required-processing error before optional native readback.
    /// Settled, released, and retired ownership is unchanged.
    pub fn fail_required_effects(&self) {
        if let MobileMusicObservationAdmission::Admitted { lease } = &self.admission {
            lease.required_failed();
        }
    }
    /// Abandons unsettled ownership; it cannot acknowledge durable success.
    pub fn release(&self) {
        if let MobileMusicObservationAdmission::Admitted { lease } = &self.admission {
            lease.release();
        }
    }
}
#[derive(Debug, uniffi::Object)]
pub struct MobileMusicObservationLease {
    queue: Arc<MusicObservationAdmissionQueue>,
    owner: Option<Arc<()>>,
    epoch: u64,
    id: u64,
}
impl MusicObservationAdmissionQueue {
    pub(crate) fn is_unused(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.next_id == 0 && state.next_stop_id == 0 && state.epoch == 0
    }
    pub(crate) fn begin_stop(
        self: &Arc<Self>,
    ) -> Result<MusicObservationStopFence, MobileRideMapCoreErrorDto> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.stop.is_some() {
            return Err(MobileRideMapCoreErrorDto::AdmissionPending);
        }
        let id = state.next_stop_id;
        state.next_stop_id = id.checked_add(1).ok_or_else(|| {
            MobileRideMapCoreErrorDto::Storage("music Stop fence identifiers exhausted".into())
        })?;
        state.stop = Some(StopObligation {
            id,
            epoch: state.epoch,
            cutoff: state.order.back().map(|entry| entry.id),
            failure: state.failure.take(),
            failure_observed: false,
        });
        Ok(MusicObservationStopFence {
            queue: Arc::clone(self),
            id,
        })
    }
    /// Synchronous legacy Stop must never overtake admitted observations.
    pub(crate) fn require_quiescent_stop(&self) -> Result<(), MobileRideMapCoreErrorDto> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.order.iter().any(|entry| !entry.phase.is_terminal()) || state.stop.is_some() {
            return Err(MobileRideMapCoreErrorDto::AdmissionPending);
        }
        if state.failure.take().is_some() {
            return Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete);
        }
        Ok(())
    }
    /// Classifies an authoritative non-recordable source without inventing a write.
    pub(crate) fn skip_history(
        self: &Arc<Self>,
        lease: &MobileMusicObservationLease,
    ) -> Result<(), MobileRideMapCoreErrorDto> {
        self.observe(lease, None, || Ok(None)).map(|_| ())
    }
    #[cfg(test)]
    pub(crate) fn admit(self: &Arc<Self>) -> Arc<MobileMusicObservationRequest> {
        self.admit_owned(None)
    }
    pub(crate) fn admit_for_owner(
        self: &Arc<Self>,
        owner: &Arc<()>,
    ) -> Arc<MobileMusicObservationRequest> {
        self.admit_owned(Some(Arc::clone(owner)))
    }
    fn admit_owned(self: &Arc<Self>, owner: Option<Arc<()>>) -> Arc<MobileMusicObservationRequest> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let id = state.next_id;
        let admission = if let Some(next_id) = id.checked_add(1) {
            if state.order.len() == MAX_PENDING_MUSIC_OBSERVATIONS {
                MobileMusicObservationAdmission::Full
            } else {
                state.next_id = next_id;
                state.order.push_back(AdmissionEntry {
                    id,
                    owner: owner.clone(),
                    waker: None,
                    phase: ObservationPhase::Waiting,
                });
                MobileMusicObservationAdmission::Admitted {
                    lease: Arc::new(MobileMusicObservationLease {
                        queue: Arc::clone(self),
                        owner,
                        epoch: state.epoch,
                        id,
                    }),
                }
            }
        } else {
            MobileMusicObservationAdmission::Exhausted
        };
        Arc::new(MobileMusicObservationRequest {
            queue: Arc::clone(self),
            epoch: state.epoch,
            admission,
        })
    }
    #[cfg(test)]
    pub(crate) fn retire(&self) {
        self.retire_matching(None, || (true, ()));
    }
    pub(crate) fn retire_if<T>(&self, owner: &Arc<()>, operation: impl FnOnce() -> (bool, T)) -> T {
        self.retire_matching(Some(owner), operation)
    }
    fn retire_matching<T>(
        &self,
        owner: Option<&Arc<()>>,
        operation: impl FnOnce() -> (bool, T),
    ) -> T {
        let (notifications, result) = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let (retire, result) = operation();
            let mut notifications = Vec::new();
            if retire {
                let epoch = state.epoch;
                let count = state.order.len();
                for _ in 0..count {
                    let Some(entry) = state.order.pop_front() else {
                        break;
                    };
                    let matching = owner.is_none_or(|owner| {
                        entry
                            .owner
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, owner))
                    });
                    if !matching {
                        state.order.push_back(entry);
                        continue;
                    }
                    if match entry.phase {
                        ObservationPhase::Settled | ObservationPhase::Failed => false,
                        _ => true,
                    } {
                        state.fail(epoch, entry.id, IncompleteReason::ProviderRetired);
                    }
                    if let Some(notification) = entry.waker {
                        notifications.push(notification);
                    }
                }
                if owner.is_none() {
                    state.epoch = state.epoch.saturating_add(1);
                }
                if let Some(notification) = state.head_waker() {
                    notifications.push(notification);
                }
            }
            (notifications, result)
        };
        self.changed.notify_all();
        for notification in notifications {
            notification.wake();
        }
        result
    }
    pub(crate) fn with_ready<T>(
        self: &Arc<Self>,
        lease: &MobileMusicObservationLease,
        operation: impl FnOnce() -> T,
    ) -> Result<T, MobileRideMapCoreErrorDto> {
        if !Arc::ptr_eq(self, &lease.queue) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.ready(lease.epoch, lease.id) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        match state
            .order
            .iter()
            .find(|entry| entry.id == lease.id)
            .map(|entry| &entry.phase)
        {
            Some(ObservationPhase::Waiting) => {
                state.publication_id = Some(lease.id);
                Ok(operation())
            }
            _ => Err(MobileRideMapCoreErrorDto::StaleRideCommand),
        }
    }
    pub(crate) fn observe(
        self: &Arc<Self>,
        lease: &MobileMusicObservationLease,
        recording: Option<MobileRideMapRecordingTokenDto>,
        operation: impl FnOnce() -> Result<
            Option<MobileMusicObservationDecision>,
            MobileRideMapCoreErrorDto,
        >,
    ) -> Result<Option<MobileMusicObservationDecision>, MobileRideMapCoreErrorDto> {
        if !Arc::ptr_eq(self, &lease.queue) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !state.ready(lease.epoch, lease.id) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        state.publication_id = Some(lease.id);
        let entry = state
            .order
            .iter_mut()
            .find(|entry| entry.id == lease.id)
            .ok_or(MobileRideMapCoreErrorDto::StaleRideCommand)?;
        if let ObservationPhase::Waiting = entry.phase {
        } else {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        let result = operation();
        if result.is_err() {
            entry.phase = ObservationPhase::Failed;
            state.fail(
                lease.epoch,
                lease.id,
                IncompleteReason::ClassificationFailed,
            );
        } else {
            entry.phase = ObservationPhase::Observed {
                recording,
                transition: result
                    .as_ref()
                    .ok()
                    .and_then(|decision| decision.as_ref())
                    .and_then(|decision| decision.history_transition.clone())
                    .map(Box::new),
                failed: false,
            };
        }
        let notification = state.head_waker();
        drop(state);
        self.changed.notify_all();
        if let Some(notification) = notification {
            notification.wake();
        }
        result
    }
}

/// Held by the Rust lifecycle worker until durable Stop acknowledgement.
#[derive(Debug)]
pub(crate) struct MusicObservationStopFence {
    queue: Arc<MusicObservationAdmissionQueue>,
    id: u64,
}
impl MusicObservationStopFence {
    /// Blocks only the independently owned Rust lifecycle worker.
    pub(crate) fn wait_result(&self) -> Result<(), MobileRideMapCoreErrorDto> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loop {
            let stop = state
                .stop
                .as_ref()
                .filter(|stop| stop.id == self.id)
                .ok_or(MobileRideMapCoreErrorDto::StaleRideCommand)?;
            let pending = stop.epoch == state.epoch
                && state.order.iter().any(|entry| {
                    stop.cutoff.is_some_and(|cutoff| entry.id <= cutoff)
                        && match entry.phase {
                            ObservationPhase::Settled | ObservationPhase::Failed => false,
                            _ => true,
                        }
                });
            if !pending {
                let incomplete = stop.failure.is_some();
                if incomplete {
                    if let Some(stop) = &mut state.stop {
                        stop.failure_observed = true;
                    }
                    return Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete);
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
impl Drop for MusicObservationStopFence {
    fn drop(&mut self) {
        let notification = {
            let mut state = self
                .queue
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.stop.as_ref().is_none_or(|stop| stop.id != self.id) {
                return;
            }
            if let Some(stop) = state.stop.take()
                && !stop.failure_observed
                && let Some(failure) = stop.failure
            {
                state.failure.get_or_insert(failure);
            }
            state.head_waker()
        };
        self.queue.changed.notify_all();
        if let Some(notification) = notification {
            notification.wake();
        }
    }
}
#[uniffi::export]
impl MobileMusicObservationLease {
    /// Suspends until the receipt owns the FIFO head or is retired.
    ///
    /// # Errors
    /// Rejects a second waiter for the same receipt.
    pub async fn wait(
        &self,
    ) -> Result<MobileMusicObservationLeaseState, MobileRideMapCoreErrorDto> {
        LeaseWait {
            queue: Arc::clone(&self.queue),
            epoch: self.epoch,
            id: self.id,
            waker: None,
        }
        .await
    }
    #[must_use]
    pub fn state(&self) -> MobileMusicObservationLeaseState {
        let state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.epoch != self.epoch
            || !state
                .order
                .iter()
                .any(|entry| entry.id == self.id && !entry.phase.is_terminal())
        {
            MobileMusicObservationLeaseState::Retired
        } else if state.ready(self.epoch, self.id) {
            MobileMusicObservationLeaseState::Ready
        } else {
            MobileMusicObservationLeaseState::Pending
        }
    }
    pub fn release(&self) {
        let notifications = {
            let mut state = self
                .queue
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.epoch != self.epoch {
                return;
            }
            let Some(index) = state.order.iter().position(|entry| entry.id == self.id) else {
                return;
            };
            let Some(entry) = state.order.remove(index) else {
                return;
            };
            let failure = match &entry.phase {
                ObservationPhase::Settled | ObservationPhase::Failed => None,
                ObservationPhase::Writing(_) | ObservationPhase::HistorySettled { .. } => {
                    Some(IncompleteReason::HistoryUnsettled)
                }
                _ => Some(IncompleteReason::Abandoned),
            };
            if let Some(failure) = failure {
                state.fail(self.epoch, self.id, failure);
            }
            (entry.waker, state.head_waker())
        };
        self.queue.changed.notify_all();
        if let Some(notification) = notifications.0 {
            notification.wake();
        }
        if let Some(notification) = notifications.1 {
            notification.wake();
        }
    }
}
impl MobileMusicObservationLease {
    pub(crate) fn validate_provider_owner(
        &self,
        owner: &Arc<()>,
    ) -> Result<(), MobileRideMapCoreErrorDto> {
        if self
            .owner
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, owner))
        {
            Ok(())
        } else {
            Err(MobileRideMapCoreErrorDto::StaleRideCommand)
        }
    }
    fn settle(
        &self,
        capture: Option<MobileCaptureWriteOutcomeDto>,
    ) -> Result<(), MobileRideMapCoreErrorDto> {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.epoch == self.epoch
            && state.order.iter().any(|entry| {
                entry.id == self.id
                    && match entry.phase {
                        ObservationPhase::Failed => true,
                        _ => false,
                    }
            })
        {
            return Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete);
        }
        if !state.ready(self.epoch, self.id) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        let entry = state
            .order
            .iter_mut()
            .find(|entry| entry.id == self.id)
            .ok_or(MobileRideMapCoreErrorDto::StaleRideCommand)?;
        let needs_capture = match &entry.phase {
            ObservationPhase::Observed {
                recording,
                transition,
                failed: false,
            } if recording.is_none() || transition.is_none() => false,
            ObservationPhase::HistorySettled { original, outcome } => match outcome {
                MobileMusicTimelineOutcomeDto::Recorded => {
                    match original.transition.capture_target {
                        MobileMusicCaptureTarget::Capture { .. } => true,
                        MobileMusicCaptureTarget::NoCapture
                        | MobileMusicCaptureTarget::Unavailable => false,
                    }
                }
                MobileMusicTimelineOutcomeDto::Duplicate
                | MobileMusicTimelineOutcomeDto::Disabled => false,
                _ => {
                    entry.phase = ObservationPhase::Failed;
                    state.fail(self.epoch, self.id, IncompleteReason::HistoryUnsettled);
                    let notification = state.head_waker();
                    drop(state);
                    self.queue.changed.notify_all();
                    if let Some(notification) = notification {
                        notification.wake();
                    }
                    return Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete);
                }
            },
            ObservationPhase::Settled => return Err(MobileRideMapCoreErrorDto::StaleRideCommand),
            _ => return Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete),
        };
        if capture.is_some_and(|outcome| outcome != MobileCaptureWriteOutcomeDto::Accepted) {
            entry.phase = ObservationPhase::Failed;
            state.fail(self.epoch, self.id, IncompleteReason::CaptureRejected);
            let notification = state.head_waker();
            drop(state);
            self.queue.changed.notify_all();
            if let Some(notification) = notification {
                notification.wake();
            }
            return Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete);
        }
        if needs_capture && capture.is_none() {
            return Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete);
        }
        entry.phase = ObservationPhase::Settled;
        let notification = state.head_waker();
        drop(state);
        self.queue.changed.notify_all();
        if let Some(notification) = notification {
            notification.wake();
        }
        Ok(())
    }
    fn required_failed(&self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.epoch != self.epoch {
            return;
        }
        let eligible = state.ready(self.epoch, self.id);
        let Some(entry) = state.order.iter_mut().find(|entry| entry.id == self.id) else {
            return;
        };
        if entry.phase.is_terminal() {
            return;
        }
        entry.phase = ObservationPhase::Failed;
        let own_notification = entry.waker.take();
        if eligible {
            state.publication_id = Some(self.id);
        }
        state.fail(self.epoch, self.id, IncompleteReason::RequiredFailed);
        let next_notification = state.head_waker();
        drop(state);
        self.queue.changed.notify_all();
        for notification in [own_notification, next_notification].into_iter().flatten() {
            notification.wake();
        }
    }
    pub(crate) fn classification_failed(&self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !state.ready(self.epoch, self.id) {
            return;
        }
        let Some(entry) = state.order.iter_mut().find(|entry| entry.id == self.id) else {
            return;
        };
        if let ObservationPhase::Waiting = entry.phase {
            entry.phase = ObservationPhase::Failed;
            state.publication_id = Some(self.id);
            state.fail(self.epoch, self.id, IncompleteReason::ClassificationFailed);
            let notification = state.head_waker();
            drop(state);
            self.queue.changed.notify_all();
            if let Some(notification) = notification {
                notification.wake();
            }
        }
    }
    pub(crate) fn history_failed(&self) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !state.ready(self.epoch, self.id) {
            return;
        }
        let Some(entry) = state.order.iter_mut().find(|entry| entry.id == self.id) else {
            return;
        };
        if let ObservationPhase::Writing(_) = entry.phase {
            entry.phase = ObservationPhase::Failed;
            state.fail(self.epoch, self.id, IncompleteReason::HistoryUnsettled);
            let notification = state.head_waker();
            drop(state);
            self.queue.changed.notify_all();
            if let Some(notification) = notification {
                notification.wake();
            }
        }
    }
    pub(crate) fn history_settled(&self, result: &MobileMusicTimelineRecordResultDto) {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !state.ready(self.epoch, self.id) {
            return;
        }
        let Some(entry) = state.order.iter_mut().find(|entry| entry.id == self.id) else {
            return;
        };
        let previous = std::mem::replace(&mut entry.phase, ObservationPhase::Waiting);
        entry.phase = match previous {
            ObservationPhase::Writing(original) => ObservationPhase::HistorySettled {
                original,
                outcome: result.outcome,
            },
            other => other,
        };
    }
    pub(crate) fn claim_write(
        &self,
    ) -> Result<
        (MobileRideMapRecordingTokenDto, MobileMusicHistoryTransition),
        MobileRideMapCoreErrorDto,
    > {
        let mut state = self
            .queue
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !state.ready(self.epoch, self.id) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        let entry = state
            .order
            .iter_mut()
            .find(|entry| entry.id == self.id)
            .ok_or(MobileRideMapCoreErrorDto::StaleRideCommand)?;
        let previous = std::mem::replace(&mut entry.phase, ObservationPhase::Waiting);
        match previous {
            ObservationPhase::Observed {
                recording: Some(recording),
                transition: Some(transition),
                failed: false,
            } => {
                let result = (recording.clone(), (*transition).clone());
                entry.phase = ObservationPhase::Writing(OriginalTransition { transition });
                Ok(result)
            }
            other => {
                entry.phase = other;
                Err(MobileRideMapCoreErrorDto::StaleRideCommand)
            }
        }
    }
}
impl Drop for MobileMusicObservationLease {
    fn drop(&mut self) {
        self.release();
    }
}
struct LeaseWait {
    queue: Arc<MusicObservationAdmissionQueue>,
    epoch: u64,
    id: u64,
    waker: Option<Waker>,
}
impl Future for LeaseWait {
    type Output = Result<MobileMusicObservationLeaseState, MobileRideMapCoreErrorDto>;
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let queue = Arc::clone(&self.queue);
        let mut state = queue.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.epoch != self.epoch {
            return Poll::Ready(Ok(MobileMusicObservationLeaseState::Retired));
        }
        if state.ready(self.epoch, self.id) {
            return Poll::Ready(Ok(MobileMusicObservationLeaseState::Ready));
        }
        let Some(entry) = state.order.iter_mut().find(|entry| entry.id == self.id) else {
            return Poll::Ready(Ok(MobileMusicObservationLeaseState::Retired));
        };
        if entry.phase.is_terminal() {
            return Poll::Ready(Ok(MobileMusicObservationLeaseState::Retired));
        }
        if self.waker.is_none() && entry.waker.is_some() {
            return Poll::Ready(Err(MobileRideMapCoreErrorDto::StaleRideCommand));
        }
        let notification = context.waker().clone();
        entry.waker = Some(notification.clone());
        self.waker = Some(notification);
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
            && let Some(entry) = state.order.iter_mut().find(|entry| entry.id == self.id)
            && entry
                .waker
                .as_ref()
                .is_some_and(|notification| notification.will_wake(own))
        {
            entry.waker = None;
        }
    }
}
/// Exact payload from a history association that can no longer be durably appended.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicUnsettledHistoryTransition {
    pub capture_target: MobileMusicCaptureTarget,
    pub id: crate::MobileMusicHistoryTransitionId,
    pub snapshot: crate::MobileMusicSnapshotDto,
    pub kind: MobileMusicUnsettledHistoryKind,
    pub wall_clock_at_ms: u64,
    pub clock_uncertainty_ms: u64,
}
/// Original classification, including a command whose confirmation never settled.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicUnsettledHistoryKind {
    Confirmed {
        kind: crate::MobileMusicRideEventKindDto,
    },
    AwaitingSkip {
        transport_id: u64,
        rejected_kind: Option<crate::MobileMusicRideEventKindDto>,
    },
}
/// A closed/replaced prior ride has incomplete listening history; none was acknowledged as recorded.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicHistoryTerminalFailure {
    pub ride_id: String,
    pub transitions: Vec<MobileMusicUnsettledHistoryTransition>,
}

/// A validated source callback targets one currently active recording.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicObservationContext {
    /// The source clock lies in this recording's active interval.
    Ready {
        recording: MobileRideMapRecordingTokenDto,
    },
    /// The authoritative recording is no longer active.
    RideNotOpen,
    /// The source callback predates this recording's active interval.
    OutOfOrder,
}
#[uniffi::export]
impl MobileRideMapCore {
    /// Explicitly settles failed association ownership only after authoritative terminal proof.
    ///
    /// # Errors
    /// Returns stale lease ownership or unavailable Core state.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns arguments at the native boundary"
    )]
    pub fn retire_closed_music_history(
        &self,
        lifecycle: Arc<crate::MobileMusicProviderLifecycle>,
        lease: Arc<MobileMusicObservationLease>,
    ) -> Result<Option<MobileMusicHistoryTerminalFailure>, MobileRideMapCoreErrorDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state.require_ready()?;
        let current = state.ride_id.as_ref().map(crate::mobile_ride_id_string);
        let closed = match state.recorder.state() {
            Some(
                ride_maps::RideLifecycleState::Stopped
                | ride_maps::RideLifecycleState::Saved
                | ride_maps::RideLifecycleState::Discarded,
            ) => true,
            _ => false,
        };
        lifecycle.retire_closed_history(&lease, current.as_deref(), closed)
    }

    /// Resolves callback ownership off the native main executor before classification.
    ///
    /// # Errors
    /// Returns a typed failure when the earlier authoritative identity is no longer current.
    #[allow(clippy::needless_pass_by_value)]
    pub fn music_observation_context(
        &self,
        expected_ride_id: String,
        observed_at_ms: u64,
    ) -> Result<MobileMusicObservationContext, MobileRideMapCoreErrorDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        resolve_music_context(&state, &expected_ride_id, observed_at_ms)
    }

    /// Records authoritative non-recordable input before native settlement.
    ///
    /// # Errors
    /// Returns stale FIFO ownership or the existing recording context failure.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns arguments at the native boundary"
    )]
    pub fn music_observation_context_for_observation(
        &self,
        lease: Arc<MobileMusicObservationLease>,
        expected_ride_id: String,
        observed_at_ms: u64,
    ) -> Result<MobileMusicObservationContext, MobileRideMapCoreErrorDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let queue = self
            .music_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(MobileRideMapCoreErrorDto::StaleRideCommand)?;
        queue.with_ready(&lease, || ())?;
        let context = resolve_music_context(&state, &expected_ride_id, observed_at_ms)?;
        if context == MobileMusicObservationContext::OutOfOrder {
            queue.skip_history(&lease)?;
        }
        Ok(context)
    }
    /// Queues an observation for its captured recording rather than a later current ride.
    ///
    /// # Errors
    /// Returns stale ownership or the existing bounded storage admission failure.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns arguments at the native boundary"
    )]
    pub fn begin_record_music_event_for_recording(
        &self,
        expected: MobileRideMapRecordingTokenDto,
        snapshot: crate::MobileMusicSnapshotDto,
        kind: crate::MobileMusicRideEventKindDto,
        monotonic_at_ms: u64,
        wall_clock_at_ms: u64,
        clock_uncertainty_ms: u64,
    ) -> Result<Arc<crate::MobileRideMapMusicCommand>, MobileRideMapCoreErrorDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        state.validate_music_recording(&expected)?;
        state.queue_music_event(
            snapshot,
            kind,
            monotonic_at_ms,
            wall_clock_at_ms,
            clock_uncertainty_ms,
        )
    }
    /// Consumes the exact classified transition once and queues it under captured ownership.
    ///
    /// # Errors
    /// Returns stale lease/recording ownership or a bounded storage admission failure.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns arguments at the native boundary"
    )]
    pub fn begin_record_music_observation(
        &self,
        lease: Arc<MobileMusicObservationLease>,
    ) -> Result<Arc<crate::MobileRideMapMusicCommand>, MobileRideMapCoreErrorDto> {
        let state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(queue) = self
            .music_observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            && !Arc::ptr_eq(queue, &lease.queue)
        {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        let (recording, transition) = lease.claim_write()?;
        let result = state.validate_music_recording(&recording).and_then(|()| {
            state.queue_music_event(
                transition.snapshot.clone(),
                transition.kind,
                transition.snapshot.observed_at_ms,
                transition.wall_clock_at_ms,
                transition.clock_uncertainty_ms,
            )
        });
        match result {
            Ok(command) => Ok(command.with_observation(lease)),
            Err(error) => {
                lease.history_failed();
                Err(error)
            }
        }
    }
}

fn resolve_music_context(
    state: &crate::MobileRideMapCoreInner,
    expected_ride_id: &str,
    observed_at_ms: u64,
) -> Result<MobileMusicObservationContext, MobileRideMapCoreErrorDto> {
    state.require_ready()?;
    if state
        .ride_id
        .as_ref()
        .is_none_or(|ride| crate::mobile_ride_id_string(ride) != expected_ride_id)
    {
        return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
    }
    let Some(recording) = state
        .recorder
        .state()
        .and_then(|lifecycle| state.snapshot(lifecycle.into()).recording_token)
    else {
        return Ok(MobileMusicObservationContext::RideNotOpen);
    };
    let observed =
        ride_maps::MonotonicMilliseconds::new(state.logical_monotonic_milliseconds(observed_at_ms));
    if !state.recorder.accepts_recording_observation_at(observed) {
        return Ok(MobileMusicObservationContext::OutOfOrder);
    }
    Ok(MobileMusicObservationContext::Ready { recording })
}
#[cfg(test)]
mod tests {
    use super::*;

    fn lease(queue: &Arc<MusicObservationAdmissionQueue>) -> Arc<MobileMusicObservationLease> {
        let MobileMusicObservationAdmission::Admitted { lease } = queue.admit().admission() else {
            panic!("available admission");
        };
        lease
    }

    fn no_history(
        queue: &Arc<MusicObservationAdmissionQueue>,
    ) -> Arc<MobileMusicObservationRequest> {
        let request = queue.admit();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("available admission");
        };
        queue
            .observe(&lease, None, || Ok(None))
            .expect("classified without history");
        request
    }

    fn writing_observation() -> (
        Arc<MobileMusicObservationRequest>,
        Arc<MobileMusicObservationLease>,
    ) {
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("available admission");
        };
        let _ = lifecycle
            .observe_admitted_music(
                Arc::clone(&lease),
                Some(MobileRideMapRecordingTokenDto {
                    ride_id: "original".into(),
                    generation: 1,
                }),
                MobileMusicCaptureTarget::Capture {
                    generation: crate::MobileCaptureGenerationDto { value: 7 },
                },
                music_snapshot(1_100),
                11_000,
                5,
            )
            .expect("history classification");
        lease.claim_write().expect("original transition");
        (request, lease)
    }

    #[test]
    fn terminal_classification_failure_does_not_wait_for_optional_release() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let request = queue.admit();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("available admission");
        };
        let fence = queue.begin_stop().expect("Stop fence");
        assert_eq!(
            queue.observe(&lease, None, || Err(
                MobileRideMapCoreErrorDto::MusicHistoryFull
            )),
            Err(MobileRideMapCoreErrorDto::MusicHistoryFull)
        );
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        request.release();
        drop(fence);
        assert_eq!(
            queue.begin_stop().expect("explicit retry").wait_result(),
            Ok(())
        );
    }

    #[test]
    fn terminal_sql_failure_does_not_wait_for_optional_release() {
        let (request, lease) = writing_observation();
        let fence = lease.queue.begin_stop().expect("Stop fence");
        lease.history_failed();
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        assert_eq!(
            request.settle(None),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        request.release();
        drop(fence);
        assert_eq!(
            lease
                .queue
                .begin_stop()
                .expect("explicit retry")
                .wait_result(),
            Ok(())
        );
    }

    #[test]
    fn terminal_capture_rejection_does_not_wait_for_optional_release() {
        let (request, lease) = writing_observation();
        lease.history_settled(&MobileMusicTimelineRecordResultDto {
            outcome: MobileMusicTimelineOutcomeDto::Recorded,
            sequence: Some(1),
            effective_policy: Some(crate::MobileMusicHistoryPolicyDto::OpaqueItem),
        });
        let fence = lease.queue.begin_stop().expect("Stop fence");
        assert_eq!(
            request.settle(Some(MobileCaptureWriteOutcomeDto::Rejected)),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        request.release();
        drop(fence);
        assert_eq!(
            lease
                .queue
                .begin_stop()
                .expect("explicit retry")
                .wait_result(),
            Ok(())
        );
    }

    #[test]
    fn foreign_classified_lease_cannot_bypass_bound_stop_queue() {
        let core = MobileRideMapCore::new();
        let bound = crate::MobileMusicProviderLifecycle::new();
        core.bind_music_observation_lifecycle(Arc::clone(&bound))
            .expect("bound owner");
        let ride = core.start_gps_only(1_000).expect("ride");
        let fence = bound.observation_queue().begin_stop().expect("Stop fence");
        let foreign = crate::MobileMusicProviderLifecycle::new();
        let request = foreign.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("available admission");
        };
        let _ = foreign
            .observe_admitted_music(
                Arc::clone(&lease),
                ride.recording_token,
                MobileMusicCaptureTarget::NoCapture,
                music_snapshot(1_100),
                11_000,
                5,
            )
            .expect("valid recording token and classified transition");
        let error = core
            .begin_record_music_observation(Arc::clone(&lease))
            .expect_err("foreign queue rejected before storage admission");
        assert_eq!(error, MobileRideMapCoreErrorDto::StaleRideCommand);
        assert!(
            lease.claim_write().is_ok(),
            "foreign transition was not consumed"
        );
        assert_eq!(fence.wait_result(), Ok(()));
    }

    #[test]
    fn stop_prefix_holds_later_observations_through_durable_stop() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = no_history(&queue);
        let second = lease(&queue);
        let fence = queue.begin_stop().expect("one stop obligation");
        let later = lease(&queue);
        first.settle(None).expect("no required effects");
        first.release();
        queue
            .observe(&second, None, || Ok(None))
            .expect("second classification");
        let second_request = MobileMusicObservationRequest {
            queue: Arc::clone(&queue),
            epoch: second.epoch,
            admission: MobileMusicObservationAdmission::Admitted { lease: second },
        };
        second_request.settle(None).expect("second settled");
        second_request.release();
        assert_eq!(fence.wait_result(), Ok(()));
        assert_eq!(later.state(), MobileMusicObservationLeaseState::Pending);
        drop(fence);
        assert_eq!(later.state(), MobileMusicObservationLeaseState::Ready);
    }

    #[test]
    fn released_unclassified_observation_fails_stop_once_then_allows_retry() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let abandoned = lease(&queue);
        abandoned.release();
        let fence = queue.begin_stop().expect("stop owns latched failure");
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        drop(fence);
        let retry = queue.begin_stop().expect("explicit stop retry");
        assert_eq!(retry.wait_result(), Ok(()));
    }

    #[test]
    fn retiring_multiple_prefix_entries_reports_one_failure_then_retry_succeeds() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = lease(&queue);
        let second = lease(&queue);
        let fence = queue.begin_stop().expect("Stop prefix");
        queue.retire();
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        first.release();
        second.release();
        drop(fence);
        assert_eq!(
            queue.begin_stop().expect("explicit retry").wait_result(),
            Ok(())
        );
    }

    #[test]
    fn replacement_lifecycle_joins_existing_fifo_and_stop_prefix() {
        let core = MobileRideMapCore::new();
        let first = crate::MobileMusicProviderLifecycle::new();
        core.bind_music_observation_lifecycle(Arc::clone(&first))
            .expect("initial owner");
        let original = first.begin_music_observation();
        let replacement = crate::MobileMusicProviderLifecycle::new();
        core.bind_music_observation_lifecycle(Arc::clone(&replacement))
            .expect("replacement shares Core queue");
        let later = replacement.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted {
            lease: original_lease,
        } = original.admission()
        else {
            panic!("admitted original");
        };
        let MobileMusicObservationAdmission::Admitted { lease: later_lease } = later.admission()
        else {
            panic!("admitted replacement");
        };
        assert!(Arc::ptr_eq(&original_lease.queue, &later_lease.queue));
        assert_eq!(
            later_lease.state(),
            MobileMusicObservationLeaseState::Pending
        );
        let fence = original_lease
            .queue
            .begin_stop()
            .expect("both lifecycle obligations");
        first
            .observation_queue()
            .skip_history(&original_lease)
            .expect("original no-history classification");
        original.settle(None).expect("original settled");
        original.release();
        assert_eq!(later_lease.state(), MobileMusicObservationLeaseState::Ready);
        replacement
            .observation_queue()
            .skip_history(&later_lease)
            .expect("replacement no-history classification");
        later.settle(None).expect("replacement settled");
        assert_eq!(fence.wait_result(), Ok(()));
    }

    #[test]
    fn shared_queue_preserves_provider_classification_ownership() {
        let core = MobileRideMapCore::new();
        let first = crate::MobileMusicProviderLifecycle::new();
        let replacement = crate::MobileMusicProviderLifecycle::new();
        core.bind_music_observation_lifecycle(Arc::clone(&first))
            .expect("first provider");
        core.bind_music_observation_lifecycle(Arc::clone(&replacement))
            .expect("replacement provider");
        let request = first.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("admitted original");
        };
        assert_eq!(
            replacement.observe_admitted_player(Arc::clone(&lease), music_snapshot(1_100)),
            Err(MobileRideMapCoreErrorDto::StaleRideCommand)
        );
        assert_eq!(
            replacement.observe_admitted_music(
                Arc::clone(&lease),
                None,
                MobileMusicCaptureTarget::NoCapture,
                music_snapshot(1_100),
                11_000,
                5
            ),
            Err(MobileRideMapCoreErrorDto::StaleRideCommand)
        );
        assert!(
            first
                .observe_admitted_player(Arc::clone(&lease), music_snapshot(1_100))
                .expect("original player owner")
                .is_some()
        );
        let _ = first
            .observe_admitted_music(
                lease,
                None,
                MobileMusicCaptureTarget::NoCapture,
                music_snapshot(1_100),
                11_000,
                5,
            )
            .expect("original history owner");
        request.settle(None).expect("no recording obligation");
    }

    #[test]
    fn provider_reset_retires_only_original_owner_in_shared_stop_prefix() {
        let core = MobileRideMapCore::new();
        let first = crate::MobileMusicProviderLifecycle::new();
        let replacement = crate::MobileMusicProviderLifecycle::new();
        core.bind_music_observation_lifecycle(Arc::clone(&first))
            .expect("first provider");
        core.bind_music_observation_lifecycle(Arc::clone(&replacement))
            .expect("replacement provider");
        let original = first.begin_music_observation();
        let later = replacement.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted {
            lease: original_lease,
        } = original.admission()
        else {
            panic!("admitted original");
        };
        let MobileMusicObservationAdmission::Admitted { lease: later_lease } = later.admission()
        else {
            panic!("admitted replacement");
        };
        let fence = first
            .observation_queue()
            .begin_stop()
            .expect("all provider obligations");
        first.reset_observation_correlation();
        assert_eq!(
            original_lease.state(),
            MobileMusicObservationLeaseState::Retired
        );
        assert_eq!(later_lease.state(), MobileMusicObservationLeaseState::Ready);
        let _ = replacement
            .observe_admitted_music(
                later_lease,
                None,
                MobileMusicCaptureTarget::NoCapture,
                music_snapshot(1_100),
                11_000,
                5,
            )
            .expect("other provider remains admitted");
        later
            .settle(None)
            .expect("replacement required work complete");
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        original.release();
        later.release();
        drop(fence);
        assert_eq!(
            replacement
                .observation_queue()
                .begin_stop()
                .expect("explicit retry")
                .wait_result(),
            Ok(())
        );
    }

    #[test]
    fn lifecycle_binding_rejects_already_used_or_in_flight_initial_queue() {
        let core = MobileRideMapCore::new();
        core.bind_music_observation_lifecycle(crate::MobileMusicProviderLifecycle::new())
            .expect("initial owner");
        let used = crate::MobileMusicProviderLifecycle::new();
        used.begin_music_observation().release();
        assert!(core.bind_music_observation_lifecycle(used).is_err());
        let accessed = crate::MobileMusicProviderLifecycle::new();
        let in_flight = accessed.observation_queue();
        assert!(
            core.bind_music_observation_lifecycle(Arc::clone(&accessed))
                .is_err()
        );
        drop(in_flight);
        core.bind_music_observation_lifecycle(accessed)
            .expect("unused queue can bind after transient access ends");
    }

    #[test]
    fn provider_retirement_fails_the_stop_prefix_and_preserves_later_gate() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let original = lease(&queue);
        let fence = queue.begin_stop().expect("stop prefix");
        queue.retire();
        let replacement = lease(&queue);
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        assert_eq!(original.state(), MobileMusicObservationLeaseState::Retired);
        assert_eq!(
            replacement.state(),
            MobileMusicObservationLeaseState::Pending
        );
        drop(fence);
        assert_eq!(replacement.state(), MobileMusicObservationLeaseState::Ready);
    }

    #[test]
    fn dropped_stop_fence_unblocks_later_observations_without_settling_prefix() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = no_history(&queue);
        let fence = queue.begin_stop().expect("stop prefix");
        assert!(queue.begin_stop().is_err(), "only one stop obligation");
        first.settle(None).expect("no required effects");
        first.release();
        let later = lease(&queue);
        assert_eq!(later.state(), MobileMusicObservationLeaseState::Pending);
        drop(fence);
        assert_eq!(later.state(), MobileMusicObservationLeaseState::Ready);
    }

    #[test]
    fn recorded_history_requires_original_capture_receipt_before_stop() {
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("available admission");
        };
        let _ = lifecycle
            .observe_admitted_music(
                Arc::clone(&lease),
                Some(MobileRideMapRecordingTokenDto {
                    ride_id: "original".into(),
                    generation: 1,
                }),
                MobileMusicCaptureTarget::Capture {
                    generation: crate::MobileCaptureGenerationDto { value: 7 },
                },
                music_snapshot(1_100),
                11_000,
                5,
            )
            .expect("history classification");
        let _ = lease.claim_write().expect("original transition");
        lease.history_settled(&MobileMusicTimelineRecordResultDto {
            outcome: MobileMusicTimelineOutcomeDto::Recorded,
            sequence: Some(1),
            effective_policy: Some(crate::MobileMusicHistoryPolicyDto::OpaqueItem),
        });
        assert_eq!(
            request.settle(None),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        request
            .settle(Some(MobileCaptureWriteOutcomeDto::Accepted))
            .expect("actual capture accepted");
        request.release();
    }

    #[test]
    fn authoritative_old_source_can_settle_without_history_but_foreign_lease_cannot() {
        let core = MobileRideMapCore::new();
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        core.bind_music_observation_lifecycle(Arc::clone(&lifecycle))
            .expect("bound lifecycle");
        let ride = core.start_gps_only(3_000).expect("ride starts");
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted {
            lease: admitted_lease,
        } = request.admission()
        else {
            panic!("available admission");
        };
        assert_eq!(
            core.music_observation_context_for_observation(
                admitted_lease,
                ride.ride_id.clone(),
                2_999
            ),
            Ok(MobileMusicObservationContext::OutOfOrder)
        );
        request
            .settle(None)
            .expect("validated non-recordable source");
        request.release();
        let foreign = Arc::new(MusicObservationAdmissionQueue::default());
        assert_eq!(
            core.music_observation_context_for_observation(lease(&foreign), ride.ride_id, 3_000,),
            Err(MobileRideMapCoreErrorDto::StaleRideCommand)
        );
        let fence = lifecycle
            .observation_queue()
            .begin_stop()
            .expect("stop prefix");
        assert_eq!(fence.wait_result(), Ok(()));
    }

    #[test]
    fn cancelling_stop_preserves_a_failure_not_yet_observed() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        lease(&queue).release();
        drop(queue.begin_stop().expect("cancelled Stop"));
        let retry = queue.begin_stop().expect("retry Stop");
        assert_eq!(
            retry.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
        drop(retry);
        assert_eq!(
            queue.begin_stop().expect("explicit retry").wait_result(),
            Ok(())
        );
    }

    #[test]
    fn settled_first_prefix_entry_does_not_hold_second_required_observation() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = no_history(&queue);
        let second = queue.admit();
        let MobileMusicObservationAdmission::Admitted {
            lease: second_lease,
        } = second.admission()
        else {
            panic!("second admitted");
        };
        let fence = queue.begin_stop().expect("both observations precede Stop");
        first.settle(None).expect("first required work complete");
        assert_eq!(
            second_lease.state(),
            MobileMusicObservationLeaseState::Ready
        );
        assert!(
            first.is_current(),
            "settlement alone need not retire optional publication"
        );
        queue
            .skip_history(&second_lease)
            .expect("second may classify before first readback releases");
        second.settle(None).expect("second required work complete");
        assert_eq!(fence.wait_result(), Ok(()));
        assert!(
            second.is_current(),
            "latest settled receipt retains optional publication"
        );
    }

    #[test]
    fn failed_first_prefix_entry_does_not_hold_second_required_observation() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = queue.admit();
        let MobileMusicObservationAdmission::Admitted { lease: first_lease } = first.admission()
        else {
            panic!("first admitted");
        };
        let second = queue.admit();
        let MobileMusicObservationAdmission::Admitted {
            lease: second_lease,
        } = second.admission()
        else {
            panic!("second admitted");
        };
        let fence = queue.begin_stop().expect("both observations precede Stop");
        assert_eq!(
            queue.observe(&first_lease, None, || Err(
                MobileRideMapCoreErrorDto::MusicHistoryFull
            )),
            Err(MobileRideMapCoreErrorDto::MusicHistoryFull)
        );
        assert_eq!(
            second_lease.state(),
            MobileMusicObservationLeaseState::Ready
        );
        queue
            .skip_history(&second_lease)
            .expect("failed optional readback cannot hold next classification");
        second.settle(None).expect("second required work complete");
        assert_eq!(
            fence.wait_result(),
            Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete)
        );
    }

    #[test]
    fn newer_classification_retires_only_older_optional_publication() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = no_history(&queue);
        let MobileMusicObservationAdmission::Admitted { lease: first_lease } = first.admission()
        else {
            panic!("first admitted");
        };
        let second = queue.admit();
        let MobileMusicObservationAdmission::Admitted {
            lease: second_lease,
        } = second.admission()
        else {
            panic!("second admitted");
        };
        first.settle(None).expect("required work complete");
        assert!(
            first.is_current(),
            "mere later admission is not publication replacement"
        );
        queue
            .with_ready(&second_lease, || ())
            .expect("next provider classification starts");
        assert!(
            !first.is_current(),
            "late old readback cannot replace newer provider publication"
        );
        assert!(second.is_current());
        assert!(
            queue.observe(&first_lease, None, || Ok(None)).is_err(),
            "terminal lease cannot classify again"
        );
    }

    #[test]
    fn failing_pending_required_request_wakes_its_own_waiter_without_replacing_head() {
        use std::{
            sync::atomic::{AtomicUsize, Ordering},
            task::Wake,
        };
        #[derive(Default)]
        struct CountedWake(AtomicUsize);
        impl Wake for CountedWake {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = queue.admit();
        let pending = queue.admit();
        let MobileMusicObservationAdmission::Admitted { lease } = pending.admission() else {
            panic!("pending admitted");
        };
        let notification_count = Arc::new(CountedWake::default());
        let waker = Waker::from(Arc::clone(&notification_count));
        let mut context = Context::from_waker(&waker);
        let mut waiting = Box::pin(lease.wait());
        assert_eq!(waiting.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(notification_count.0.load(Ordering::SeqCst), 0);
        pending.fail_required_effects();
        assert_eq!(notification_count.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            waiting.as_mut().poll(&mut context),
            Poll::Ready(Ok(MobileMusicObservationLeaseState::Retired))
        );
        assert!(
            first.is_current(),
            "pending failure cannot replace current required head"
        );
        assert!(!pending.is_current());
    }

    #[test]
    fn required_processing_failure_settles_stop_before_optional_release() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let request = queue.admit();
        let fence = queue.begin_stop().expect("accepted request precedes Stop");
        request.fail_required_effects();
        let (send, receive) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let _ = send.send(fence.wait_result());
        });
        let before_release = receive.recv_timeout(std::time::Duration::from_millis(100));
        request.release();
        waiter.join().expect("Stop waiter terminates after cleanup");
        assert_eq!(
            before_release,
            Ok(Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete))
        );
        request.fail_required_effects();
        let successful = no_history(&queue);
        successful.settle(None).expect("required work complete");
        successful.fail_required_effects();
        assert_eq!(
            queue
                .begin_stop()
                .expect("new Stop does not inherit fabricated failure")
                .wait_result(),
            Ok(())
        );
        assert!(
            successful.is_current(),
            "no-op failure preserves terminal publication currency"
        );
    }

    #[test]
    fn malformed_player_observation_fails_stop_before_optional_release() {
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("admitted malformed source");
        };
        let fence = lifecycle
            .observation_queue()
            .begin_stop()
            .expect("accepted source precedes Stop");
        let mut snapshot = music_snapshot(1_100);
        snapshot
            .item
            .as_mut()
            .expect("fixture item")
            .identifier
            .clear();
        assert!(lifecycle.observe_admitted_player(lease, snapshot).is_err());
        let (send, receive) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let _ = send.send(fence.wait_result());
        });
        let before_release = receive.recv_timeout(std::time::Duration::from_millis(100));
        request.release();
        waiter.join().expect("Stop waiter settles after cleanup");
        assert_eq!(
            before_release,
            Ok(Err(MobileRideMapCoreErrorDto::MusicObservationIncomplete))
        );
    }

    #[test]
    fn settled_prefix_does_not_wait_for_optional_readback_release() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = no_history(&queue);
        let fence = queue.begin_stop().expect("Stop fence");
        let later = lease(&queue);
        first.settle(None).expect("required work complete");
        assert!(
            first.is_current(),
            "optional readback retains FIFO ownership"
        );
        assert_eq!(fence.wait_result(), Ok(()));
        first.release();
        assert_eq!(later.state(), MobileMusicObservationLeaseState::Pending);
        drop(fence);
        assert_eq!(later.state(), MobileMusicObservationLeaseState::Ready);
    }

    #[test]
    fn async_lease_wait_is_suspended_and_woken_only_by_predecessor_settlement() {
        use std::{
            future::Future,
            sync::atomic::{AtomicUsize, Ordering},
            task::{Context, Poll, Wake, Waker},
        };
        #[derive(Default)]
        struct CountedWake(AtomicUsize);
        impl Wake for CountedWake {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = lease(&queue);
        let second = lease(&queue);
        let notification_count = Arc::new(CountedWake::default());
        let waker = Waker::from(Arc::clone(&notification_count));
        let mut context = Context::from_waker(&waker);
        let mut waiting = Box::pin(second.wait());
        assert_eq!(waiting.as_mut().poll(&mut context), Poll::Pending);
        assert_eq!(notification_count.0.load(Ordering::SeqCst), 0);
        first.release();
        assert_eq!(notification_count.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            waiting.as_mut().poll(&mut context),
            Poll::Ready(Ok(MobileMusicObservationLeaseState::Ready))
        );
    }

    #[test]
    fn fifo_lease_keeps_later_identity_lookup_behind_durable_predecessor() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = lease(&queue);
        let second = lease(&queue);
        assert_eq!(first.state(), MobileMusicObservationLeaseState::Ready);
        assert_eq!(second.state(), MobileMusicObservationLeaseState::Pending);
        first.release();
        assert_eq!(first.state(), MobileMusicObservationLeaseState::Retired);
        assert_eq!(second.state(), MobileMusicObservationLeaseState::Ready);
    }

    #[test]
    fn full_lease_queue_rejects_without_replacing_existing_owned_observations() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let owned: Vec<_> = (0..MAX_PENDING_MUSIC_OBSERVATIONS)
            .map(|_| lease(&queue))
            .collect();
        assert!(matches!(
            queue.admit().admission(),
            MobileMusicObservationAdmission::Full
        ));
        assert_eq!(owned[0].state(), MobileMusicObservationLeaseState::Ready);
        drop(owned);
        assert_eq!(
            lease(&queue).state(),
            MobileMusicObservationLeaseState::Ready
        );
    }

    #[test]
    fn retirement_and_abandonment_release_capacity_and_reject_late_receipts() {
        let queue = Arc::new(MusicObservationAdmissionQueue::default());
        let first = lease(&queue);
        let second = lease(&queue);
        drop(first);
        assert_eq!(second.state(), MobileMusicObservationLeaseState::Ready);
        queue.retire();
        let replacement = lease(&queue);
        assert_eq!(second.state(), MobileMusicObservationLeaseState::Retired);
        second.release();
        assert_eq!(replacement.state(), MobileMusicObservationLeaseState::Ready);
    }
    #[test]
    fn source_before_new_active_interval_is_rejected_before_music_classification() {
        let core = MobileRideMapCore::new();
        let _ = core.start_gps_only(1_000).expect("first ride");
        let _ = core.stop_at(2_000).expect("stop first");
        let next = core.start_gps_only(3_000).expect("next ride");
        assert_eq!(
            core.music_observation_context(next.ride_id.clone(), 2_999),
            Ok(MobileMusicObservationContext::OutOfOrder),
        );
        assert!(matches!(
            core.music_observation_context(next.ride_id, 3_000),
            Ok(MobileMusicObservationContext::Ready { .. }),
        ));
    }

    #[test]
    fn expected_recording_cannot_write_to_replacement_ride_after_identity_query() {
        let core = MobileRideMapCore::new();
        let first = core.start_gps_only(1_000).expect("first ride");
        let expected = first.recording_token.expect("first recording");
        let _ = core.stop_at(2_000).expect("stop first");
        let replacement = core.start_gps_only(3_000).expect("next ride");
        let rejected = core.begin_record_music_event_for_recording(
            expected,
            music_snapshot(3_100),
            crate::MobileMusicRideEventKindDto::ItemChanged,
            3_100,
            30_000,
            5,
        );
        assert!(matches!(
            rejected,
            Err(MobileRideMapCoreErrorDto::StaleRideCommand)
        ));
        assert_eq!(
            core.current_snapshot(3_100)
                .expect("replacement remains")
                .ride_id,
            replacement.ride_id
        );
    }

    #[test]
    fn new_ride_cannot_reset_or_reassociate_unsettled_prior_history() {
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        let first = lifecycle
            .observe_music_for_ride(Some("first-ride".into()), music_snapshot(1_100), 11_000, 5)
            .expect("first observation")
            .expect("accepted");
        let original = first
            .history_transition
            .expect("unsettled original transition");
        assert_eq!(
            lifecycle.observe_music_for_ride(
                Some("second-ride".into()),
                music_snapshot(3_100),
                31_000,
                7,
            ),
            Err(MobileRideMapCoreErrorDto::PendingMusicHistory),
        );
        let retry = lifecycle
            .observe_music_for_ride(Some("first-ride".into()), music_snapshot(1_200), 12_000, 9)
            .expect("same ride retry")
            .expect("accepted");
        assert_eq!(retry.history_transition, Some(original));
    }

    #[test]
    fn unchanged_song_starts_new_ride_association_after_prior_transition_settles() {
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        let first = lifecycle
            .observe_music_for_ride(Some("first-ride".into()), music_snapshot(1_100), 11_000, 5)
            .expect("first observation")
            .expect("accepted");
        let pending = first.history_transition.expect("initial song");
        let _ = lifecycle.acknowledge_history_transition(pending.id);
        let second = lifecycle
            .observe_music_for_ride(Some("second-ride".into()), music_snapshot(3_100), 31_000, 7)
            .expect("new ride observation")
            .expect("accepted");
        let initial = second
            .history_transition
            .expect("unchanged song still starts new ride");
        assert_eq!(initial.snapshot.observed_at_ms, 3_100);
        assert_eq!(initial.wall_clock_at_ms, 31_000);
        assert_eq!(initial.clock_uncertainty_ms, 7);
        assert_eq!(
            initial.kind,
            crate::MobileMusicRideEventKindDto::ItemChanged
        );
    }

    #[test]
    fn failed_old_ride_history_returns_exact_terminal_receipt_before_new_baseline() {
        let core = MobileRideMapCore::new();
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        let first = core.start_gps_only(1_000).expect("first ride");
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("admitted");
        };
        let original = lifecycle
            .observe_admitted_music(
                Arc::clone(&lease),
                first.recording_token,
                MobileMusicCaptureTarget::Capture {
                    generation: crate::MobileCaptureGenerationDto { value: 7 },
                },
                music_snapshot(1_100),
                11_000,
                7,
            )
            .expect("observed")
            .expect("accepted")
            .history_transition
            .expect("pending");
        assert!(
            core.begin_record_music_observation(Arc::clone(&lease))
                .is_err(),
            "no storage fixture rejects write"
        );
        request.release();
        let _ = core.stop_at(2_000).expect("stop old");
        let next = core.start_gps_only(3_000).expect("new ride");
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("admitted");
        };
        let failure = core
            .retire_closed_music_history(Arc::clone(&lifecycle), Arc::clone(&lease))
            .expect("terminal check")
            .expect("old association must be reported incomplete");
        assert_eq!(failure.ride_id, first.ride_id);
        assert_eq!(failure.transitions.len(), 1);
        assert_eq!(
            failure.transitions[0].capture_target,
            original.capture_target
        );
        assert_eq!(failure.transitions[0].id, original.id);
        assert_eq!(failure.transitions[0].snapshot, original.snapshot);
        assert_eq!(failure.transitions[0].wall_clock_at_ms, 11_000);
        assert_eq!(failure.transitions[0].clock_uncertainty_ms, 7);
        assert_eq!(
            failure.transitions[0].kind,
            MobileMusicUnsettledHistoryKind::Confirmed {
                kind: original.kind
            }
        );
        let next = lifecycle
            .observe_admitted_music(
                lease,
                next.recording_token,
                MobileMusicCaptureTarget::Capture {
                    generation: crate::MobileCaptureGenerationDto { value: 8 },
                },
                music_snapshot(3_100),
                31_000,
                9,
            )
            .expect("new baseline")
            .expect("accepted")
            .history_transition
            .expect("first song in new ride");
        assert_eq!(next.snapshot.observed_at_ms, 3_100);
        assert_eq!(next.wall_clock_at_ms, 31_000);
        assert_eq!(next.kind, crate::MobileMusicRideEventKindDto::ItemChanged);
    }

    #[test]
    fn player_source_watermark_is_independent_from_pending_history_and_ride_interval() {
        let lifecycle = crate::MobileMusicProviderLifecycle::new();
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("admitted");
        };
        let first = music_snapshot(1_100);
        assert_eq!(
            lifecycle.observe_admitted_player(Arc::clone(&lease), first.clone()),
            Ok(Some(first))
        );
        let _ = lifecycle
            .observe_admitted_music(
                lease,
                Some(MobileRideMapRecordingTokenDto {
                    ride_id: "old".into(),
                    generation: 1,
                }),
                MobileMusicCaptureTarget::Unavailable,
                music_snapshot(1_100),
                11_000,
                5,
            )
            .expect("pending old");
        request.release();
        let request = lifecycle.begin_music_observation();
        let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
            panic!("admitted");
        };
        let latest = music_snapshot(1_200);
        assert_eq!(
            lifecycle.observe_admitted_player(Arc::clone(&lease), latest.clone()),
            Ok(Some(latest))
        );
        assert_eq!(
            lifecycle.observe_admitted_player(Arc::clone(&lease), music_snapshot(1_100)),
            Ok(None)
        );
        assert_eq!(
            lifecycle.observe_admitted_music(
                lease,
                Some(MobileRideMapRecordingTokenDto {
                    ride_id: "unverified-new".into(),
                    generation: 1
                }),
                MobileMusicCaptureTarget::Unavailable,
                music_snapshot(1_200),
                12_000,
                5
            ),
            Err(MobileRideMapCoreErrorDto::PendingMusicHistory)
        );
    }

    fn music_snapshot(observed_at_ms: u64) -> crate::MobileMusicSnapshotDto {
        crate::MobileMusicSnapshotDto {
            provider: crate::MobileMusicProviderDto::AppleMusic,
            session_id: "session".into(),
            state: crate::MobileMusicPlaybackStateDto::Playing,
            item: Some(crate::MobileMusicItemDto {
                identifier: "same-song".into(),
                title: Some("Song".into()),
                artist: None,
            }),
            position_milliseconds: None,
            duration_milliseconds: None,
            observed_at_ms,
            capabilities: crate::MobileMusicCapabilitiesDto {
                previous: false,
                play: false,
                pause: true,
                next: false,
                open_provider: true,
            },
        }
    }
}
