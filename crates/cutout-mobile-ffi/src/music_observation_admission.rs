//! `UniFFI` projections for application-owned music observation receipts.

use std::sync::{Arc, PoisonError};

use cutout_application::music::observation_admission as application;
use cutout_music::MusicHistoryTransition;

use crate::{
    MobileMusicHistoryTransition, MobileMusicTimelineOutcomeDto,
    MobileMusicTimelineRecordResultDto, MobileRideMapCore, MobileRideMapCoreErrorDto,
    MobileRideMapRecordingTokenDto, ride_maps,
};

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
    /// Provider retirement, required settlement, or release ended ownership.
    Retired,
}

#[derive(Debug, Default)]
pub(crate) struct MusicObservationAdmissionQueue {
    inner: Arc<application::ObservationQueue>,
}

/// Lightweight admission result and post-await ownership fence.
#[derive(Debug, uniffi::Object)]
pub struct MobileMusicObservationRequest {
    inner: Arc<application::ObservationRequest>,
    admission: MobileMusicObservationAdmission,
}

/// Native handle for one application-owned observation lease.
#[derive(Debug, uniffi::Object)]
pub struct MobileMusicObservationLease {
    inner: Arc<application::ObservationLease>,
    owner: Option<Arc<()>>,
}

/// Native handle for a Stop prefix fence.
#[derive(Debug)]
pub(crate) struct MusicObservationStopFence {
    inner: application::StopFence,
}

fn owner_key(owner: &Arc<()>) -> u64 {
    Arc::as_ptr(owner) as usize as u64
}

fn map_admission_error(error: application::AdmissionError) -> MobileRideMapCoreErrorDto {
    match error {
        application::AdmissionError::Stale => MobileRideMapCoreErrorDto::StaleRideCommand,
        application::AdmissionError::Incomplete => {
            MobileRideMapCoreErrorDto::MusicObservationIncomplete
        }
        application::AdmissionError::Pending => MobileRideMapCoreErrorDto::AdmissionPending,
        application::AdmissionError::Exhausted => {
            MobileRideMapCoreErrorDto::Storage("music observation identifiers exhausted".into())
        }
    }
}
fn map_classification_error(
    error: application::ClassificationError<MobileRideMapCoreErrorDto>,
) -> MobileRideMapCoreErrorDto {
    match error {
        application::ClassificationError::Admission(error) => map_admission_error(error),
        application::ClassificationError::Operation(error) => error,
    }
}

impl MusicObservationAdmissionQueue {
    pub(crate) fn is_unused(&self) -> bool {
        self.inner.is_unused()
    }
    pub(crate) fn begin_stop(
        self: &Arc<Self>,
    ) -> Result<MusicObservationStopFence, MobileRideMapCoreErrorDto> {
        self.inner
            .begin_stop()
            .map(|inner| MusicObservationStopFence { inner })
            .map_err(map_admission_error)
    }
    pub(crate) fn require_quiescent_stop(&self) -> Result<(), MobileRideMapCoreErrorDto> {
        self.inner
            .require_quiescent_stop()
            .map_err(map_admission_error)
    }
    pub(crate) fn admit_for_owner(
        self: &Arc<Self>,
        owner: &Arc<()>,
    ) -> Arc<MobileMusicObservationRequest> {
        let request = self.inner.admit(Some(owner_key(owner)));
        let admission = match request.admission() {
            application::Admission::Admitted(inner) => MobileMusicObservationAdmission::Admitted {
                lease: Arc::new(MobileMusicObservationLease {
                    inner,
                    owner: Some(Arc::clone(owner)),
                }),
            },
            application::Admission::Full => MobileMusicObservationAdmission::Full,
            application::Admission::Exhausted => MobileMusicObservationAdmission::Exhausted,
        };
        Arc::new(MobileMusicObservationRequest {
            inner: request,
            admission,
        })
    }
    pub(crate) fn retire_if<T>(&self, owner: &Arc<()>, operation: impl FnOnce() -> (bool, T)) -> T {
        self.inner.retire_owner(owner_key(owner), operation)
    }
    pub(crate) fn with_ready<T>(
        &self,
        lease: &MobileMusicObservationLease,
        operation: impl FnOnce() -> T,
    ) -> Result<T, MobileRideMapCoreErrorDto> {
        self.inner
            .with_ready(&lease.inner, operation)
            .map_err(map_admission_error)
    }
    pub(crate) fn owns(&self, lease: &MobileMusicObservationLease) -> bool {
        self.inner.owns(&lease.inner)
    }
    pub(crate) fn skip_history(
        &self,
        lease: &MobileMusicObservationLease,
    ) -> Result<(), MobileRideMapCoreErrorDto> {
        if !self.inner.owns(&lease.inner) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        lease
            .inner
            .classify(None, || Ok::<_, MobileRideMapCoreErrorDto>(((), None)))
            .map_err(map_classification_error)
    }
    pub(crate) fn observe<T>(
        &self,
        lease: &MobileMusicObservationLease,
        recording: Option<MobileRideMapRecordingTokenDto>,
        operation: impl FnOnce() -> Result<
            Option<(T, Option<MusicHistoryTransition>)>,
            MobileRideMapCoreErrorDto,
        >,
    ) -> Result<Option<T>, MobileRideMapCoreErrorDto> {
        if !self.inner.owns(&lease.inner) {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        }
        let recording = recording.map(|token| application::RecordingIdentity {
            ride_id: token.ride_id,
            generation: token.generation,
        });
        lease
            .inner
            .classify(recording, || {
                operation().map(|decision| {
                    decision.map_or((None, None), |(value, transition)| {
                        (Some(value), transition)
                    })
                })
            })
            .map_err(map_classification_error)
    }
}

#[uniffi::export]
impl MobileMusicObservationRequest {
    #[must_use]
    pub fn admission(&self) -> MobileMusicObservationAdmission {
        self.admission.clone()
    }
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.inner.is_current()
    }
    /// Consumes a matching observation after its required effects have receipts.
    ///
    /// # Errors
    /// Rejects a stale request or an observation with incomplete required effects.
    pub fn settle(&self) -> Result<(), MobileRideMapCoreErrorDto> {
        let MobileMusicObservationAdmission::Admitted { lease } = &self.admission else {
            return Err(MobileRideMapCoreErrorDto::StaleRideCommand);
        };
        lease.inner.settle().map_err(map_admission_error)
    }
    pub fn fail_required_effects(&self) {
        if let MobileMusicObservationAdmission::Admitted { lease } = &self.admission {
            lease.inner.required_failed();
        }
    }
    pub fn release(&self) {
        if let MobileMusicObservationAdmission::Admitted { lease } = &self.admission {
            lease.inner.release();
        }
    }
}

#[uniffi::export]
impl MobileMusicObservationLease {
    /// Waits until this observation owns the FIFO head.
    ///
    /// # Errors
    /// Rejects retired ownership, a stale lease or a cancelled exclusive wait.
    pub async fn wait(
        &self,
    ) -> Result<MobileMusicObservationLeaseState, MobileRideMapCoreErrorDto> {
        self.inner
            .wait()
            .await
            .map(|state| match state {
                application::LeaseState::Pending => MobileMusicObservationLeaseState::Pending,
                application::LeaseState::Ready => MobileMusicObservationLeaseState::Ready,
                application::LeaseState::Retired => MobileMusicObservationLeaseState::Retired,
            })
            .map_err(map_admission_error)
    }
    #[must_use]
    pub fn state(&self) -> MobileMusicObservationLeaseState {
        match self.inner.state() {
            application::LeaseState::Pending => MobileMusicObservationLeaseState::Pending,
            application::LeaseState::Ready => MobileMusicObservationLeaseState::Ready,
            application::LeaseState::Retired => MobileMusicObservationLeaseState::Retired,
        }
    }
    pub fn release(&self) {
        self.inner.release();
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
            && self.inner.owned_by(owner_key(owner))
        {
            Ok(())
        } else {
            Err(MobileRideMapCoreErrorDto::StaleRideCommand)
        }
    }
    pub(crate) fn classification_failed(&self) {
        self.inner.classification_failed();
    }
    pub(crate) fn history_failed(&self) {
        self.inner.history_failed();
    }
    pub(crate) fn history_settled(&self, result: &MobileMusicTimelineRecordResultDto) {
        let receipt = match result.outcome {
            MobileMusicTimelineOutcomeDto::Recorded => application::HistoryReceipt::Recorded,
            MobileMusicTimelineOutcomeDto::Duplicate => application::HistoryReceipt::Duplicate,
            MobileMusicTimelineOutcomeDto::Disabled => application::HistoryReceipt::Disabled,
            MobileMusicTimelineOutcomeDto::OutOfOrder => application::HistoryReceipt::OutOfOrder,
            MobileMusicTimelineOutcomeDto::RideNotOpen => application::HistoryReceipt::RideNotOpen,
            MobileMusicTimelineOutcomeDto::Full => application::HistoryReceipt::Full,
        };
        self.inner.history_receipt(receipt);
    }
    pub(crate) fn claim_write(
        &self,
    ) -> Result<
        (MobileRideMapRecordingTokenDto, MobileMusicHistoryTransition),
        MobileRideMapCoreErrorDto,
    > {
        let (recording, transition) = self.inner.claim_write().map_err(map_admission_error)?;
        Ok((
            MobileRideMapRecordingTokenDto {
                ride_id: recording.ride_id,
                generation: recording.generation,
            },
            project_history_transition(&transition),
        ))
    }
}

impl MusicObservationStopFence {
    pub(crate) fn wait_result(&self) -> Result<(), MobileRideMapCoreErrorDto> {
        self.inner.wait_result().map_err(map_admission_error)
    }
}

fn project_history_transition(transition: &MusicHistoryTransition) -> MobileMusicHistoryTransition {
    MobileMusicHistoryTransition {
        id: transition.id().into(),
        snapshot: transition.snapshot().into(),
        kind: transition.kind().into(),
        wall_clock_at_ms: transition.timing().wall_clock_at().as_milliseconds(),
        clock_uncertainty_ms: transition.timing().clock_uncertainty_milliseconds(),
    }
}

/// Exact payload from a history association that can no longer be durably appended.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicUnsettledHistoryTransition {
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
            && !queue.owns(&lease)
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
