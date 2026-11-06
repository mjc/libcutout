//! Startup ride restoration and interrupted-ride recovery policy.

use super::{
    RecordingState, RideRecordingApplication, RideRecordingError, RideRecordingSnapshot,
    ensure_command, restore_projection,
};
use cutout_ride_maps::{
    MonotonicMilliseconds, RideDurationMilliseconds, RideEvent, RideLifecycleState,
    RideRecoveryDecision, RideRecoveryKind, VehicleIdentity, WallClockUnixMilliseconds,
    ride_recovery_decision,
};
use libcutout_persistence::{
    CommittedRideMovingTimeCheckpoint, PendingRideRestoreSnapshot, RideId, RideRestoreSnapshot,
};
use std::sync::PoisonError;

/// Inputs needed to apply the startup recovery policy to a restored ride.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideRecoveryPolicy {
    /// Current process monotonic timestamp.
    pub monotonic_now_ms: u64,
    /// Current wall-clock Unix timestamp.
    pub wall_clock_now_ms: u64,
    /// Maximum age for resuming an interrupted, non-paused ride.
    pub auto_resume_window_ms: u64,
    /// Maximum age for keeping an explicitly paused ride open.
    pub explicit_pause_window_ms: u64,
    /// Whether automatic recovery is enabled.
    pub enabled: bool,
    /// Whether automatic recording is enabled for newly created rides.
    pub autostart_enabled: bool,
    /// Device selected in preferences; recovery cross-checks it against the snapshot's last
    /// connected vehicle before creating a replacement ride.
    pub selected_vehicle_identifier: Option<String>,
}

/// Recovery operation applied after the durable ride snapshot is loaded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideRecoveryDisposition {
    /// No active ride was stored.
    Empty,
    /// An active ride was restored and its monotonic timeline aligned.
    Active,
    /// A recently interrupted ride was resumed.
    Resumed,
    /// A recently explicitly paused ride remains paused.
    PauseRetained,
    /// Recovery policy was disabled and the restored lifecycle was left unchanged.
    Unchanged,
    /// A stale ride was saved and remains the current ride.
    Saved,
    /// An empty stale ride was discarded.
    Discarded,
    /// A stale ride was settled and a replacement live ride was created atomically.
    Replaced,
    /// An interrupted vehicle ride stays inactive until a verified connection admits it.
    AwaitingConnection,
}

/// Result of durable startup restoration and recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideRecoveryResult {
    /// Current application projection.
    pub snapshot: RideRecordingSnapshot,
    /// Recovery action taken for the restored ride.
    pub disposition: RideRecoveryDisposition,
}

impl RideRecordingApplication {
    /// Loads and publishes a restore snapshot, applying recovery with one durable settlement.
    ///
    /// # Errors
    /// Returns an error if the restore ticket is stale, the storage worker fails, or the
    /// persisted ride cannot be transitioned or settled consistently.
    pub fn finish_restore_with_policy(
        &self,
        operation: super::PreparedRideRestore,
        policy: &RideRecoveryPolicy,
    ) -> Result<RideRecoveryResult, RideRecordingError> {
        let super::PreparedRideRestore { lease, storage } = operation;
        let command_id = lease.id;
        let restored = storage
            .map(PendingRideRestoreSnapshot::wait_result)
            .transpose()?;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        ensure_command(&state, command_id)?;
        if state.generation.checked_add(1).is_none() {
            state.restore_phase = super::RestorePhase::RecoveryRequired;
            state.pending_command_id = None;
            return Err(RideRecordingError::CommandIdExhausted);
        }
        let context = restored.as_ref().and_then(recovery_context);
        if let Some(snapshot) = restored {
            restore_projection(&mut state, &snapshot, policy.enabled);
        } else {
            clear_projection(&mut state);
        }
        let disposition = if let Some(context) = context {
            apply_recovery(self.database.as_ref(), &mut state, context, policy)?
        } else {
            RideRecoveryDisposition::Empty
        };
        state.restore_phase = super::RestorePhase::Ready;
        state.pending_command_id = None;
        Ok(RideRecoveryResult {
            snapshot: super::snapshot_from(&state),
            disposition,
        })
    }
}

#[derive(Clone, Debug)]
struct RecoveryContext {
    ride_id: RideId,
    lifecycle: RideLifecycleState,
    updated_at_ms: u64,
    timing_last_ms: u64,
    explicit_pause: bool,
    point_count: u64,
    has_telemetry_receipt: bool,
    recorded_duration: Option<RideDurationMilliseconds>,
    last_connected_vehicle: Option<VehicleIdentity>,
    has_vehicle_identity: bool,
}

fn recovery_context(snapshot: &RideRestoreSnapshot) -> Option<RecoveryContext> {
    let ride = snapshot.ride()?;
    Some(RecoveryContext {
        ride_id: ride.id(),
        lifecycle: ride.state(),
        updated_at_ms: ride.updated_at_milliseconds(),
        timing_last_ms: ride
            .monotonic_last_event_milliseconds()
            .or_else(|| {
                snapshot
                    .route_points()
                    .last()
                    .map(|point| point.sample().monotonic_milliseconds().as_u64())
            })
            .unwrap_or(0),
        explicit_pause: ride.paused_at_milliseconds().is_some(),
        point_count: ride.summary().point_count().as_u64(),
        has_telemetry_receipt: ride.last_telemetry_at_milliseconds().is_some(),
        recorded_duration: ride.last_recorded_point_duration_milliseconds(),
        last_connected_vehicle: snapshot
            .last_connected_device()
            .and_then(VehicleIdentity::new),
        has_vehicle_identity: ride.candidate_vehicle().is_some()
            || ride.associated_vehicle().is_some(),
    })
}

fn apply_recovery(
    database: Option<&libcutout_persistence::RideDatabase>,
    state: &mut RecordingState,
    context: RecoveryContext,
    policy: &RideRecoveryPolicy,
) -> Result<RideRecoveryDisposition, RideRecordingError> {
    if context.lifecycle == RideLifecycleState::Active {
        if !policy.enabled {
            return Ok(RideRecoveryDisposition::Unchanged);
        }
        let receipt = state
            .persisted_telemetry_receipt
            .filter(|(ride_id, _)| *ride_id == context.ride_id)
            .map_or(0, |(_, at)| at);
        let associated = state
            .recorder
            .associated_at_milliseconds()
            .map_or(0, MonotonicMilliseconds::as_u64);
        state.monotonic_epoch_offset_ms = context
            .timing_last_ms
            .max(associated)
            .max(receipt)
            .saturating_sub(policy.monotonic_now_ms);
        state.recoverable_updated_at_ms = None;
        return Ok(RideRecoveryDisposition::Active);
    }
    if !policy.enabled {
        return Ok(RideRecoveryDisposition::Unchanged);
    }
    let recovery_kind = if context.explicit_pause {
        RideRecoveryKind::ExplicitPause
    } else {
        RideRecoveryKind::InvoluntaryInterruption
    };
    match ride_recovery_decision(
        recovery_kind,
        WallClockUnixMilliseconds::new(policy.wall_clock_now_ms),
        WallClockUnixMilliseconds::new(context.updated_at_ms),
    ) {
        RideRecoveryDecision::Resume => {
            // Vehicle rides resume through verified connection admission. Remembered identity
            // alone must not make a disconnected recording active after process restoration.
            if !context.has_vehicle_identity {
                resume_restored(
                    state,
                    database,
                    context.ride_id,
                    context.timing_last_ms,
                    policy.monotonic_now_ms,
                )?;
                state.recoverable_updated_at_ms = None;
                return Ok(RideRecoveryDisposition::Resumed);
            }
            return Ok(RideRecoveryDisposition::AwaitingConnection);
        }
        RideRecoveryDecision::KeepPaused => return Ok(RideRecoveryDisposition::PauseRetained),
        RideRecoveryDecision::Finalize => {}
    }

    let discard_empty = context.point_count == 0 && !context.has_telemetry_receipt;
    let replacement_candidate = context.last_connected_vehicle.filter(|identity| {
        policy.autostart_enabled
            && policy.selected_vehicle_identifier.as_deref() == Some(identity.as_str())
    });
    let candidate = replacement_candidate.as_ref().map(VehicleIdentity::as_str);
    let committed = database
        .map(|database| {
            database.settle_recovered_ride_with_checkpoint(
                context.ride_id,
                discard_empty,
                policy.wall_clock_now_ms,
                policy.monotonic_now_ms,
                candidate,
            )
        })
        .transpose()?;
    state.recoverable_updated_at_ms = None;
    let committed = committed.flatten();
    if let Some(committed) = committed
        && committed.ride_id() != context.ride_id
    {
        reset_after_recovery(state);
        start_replacement(
            state,
            committed,
            policy.monotonic_now_ms,
            replacement_candidate,
        )?;
        return Ok(RideRecoveryDisposition::Replaced);
    }
    if discard_empty {
        reset_after_recovery(state);
        return Ok(RideRecoveryDisposition::Discarded);
    }
    if let Some(committed) = committed {
        if committed.ride_id() != context.ride_id {
            return Err(RideRecordingError::StaleCommand);
        }
        state.checkpoint = Some(committed.checkpoint());
    }
    save_recovered_projection(state, context.explicit_pause, context.recorded_duration)?;
    Ok(RideRecoveryDisposition::Saved)
}

fn resume_restored(
    state: &mut RecordingState,
    database: Option<&libcutout_persistence::RideDatabase>,
    ride_id: RideId,
    timing_last_ms: u64,
    monotonic_now_ms: u64,
) -> Result<(), RideRecordingError> {
    let epoch_offset = timing_last_ms.saturating_sub(monotonic_now_ms);
    let logical_at = monotonic_now_ms.saturating_add(epoch_offset);
    let event = RideEvent::Resume;
    let transition = state
        .recorder
        .validate_transition(event)
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    let committed = database
        .map(|database| {
            database
                .queue_transition_at(ride_id, event, logical_at)?
                .wait_committed_result()
        })
        .transpose()?;
    if committed.is_some_and(|committed| {
        committed.ride_id() != ride_id || committed.lifecycle() != RideLifecycleState::Active
    }) {
        return Err(RideRecordingError::StaleCommand);
    }
    let at = MonotonicMilliseconds::new(logical_at);
    state
        .recorder
        .apply_transition_at(transition, at)
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    state
        .admission_recorder
        .apply_transition_at(transition, at)
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    state.monotonic_epoch_offset_ms = epoch_offset;
    state.checkpoint = committed.map(CommittedRideMovingTimeCheckpoint::checkpoint);
    state.revision = state.revision.saturating_add(1);
    state.generation = state.generation.saturating_add(1);
    Ok(())
}

fn save_recovered_projection(
    state: &mut RecordingState,
    explicit_pause: bool,
    recorded_duration: Option<RideDurationMilliseconds>,
) -> Result<(), RideRecordingError> {
    let mut recorders = [state.recorder.clone(), state.admission_recorder.clone()];
    for recorder in &mut recorders {
        if let Some(recorded_duration) = recorded_duration.filter(|_| !explicit_pause) {
            recorder
                .save_interrupted_at_recorded_duration(recorded_duration)
                .map_err(|_| RideRecordingError::InvalidTransition)?;
        } else {
            if explicit_pause {
                let transition = recorder
                    .validate_transition(RideEvent::Stop)
                    .map_err(|_| RideRecordingError::InvalidTransition)?;
                recorder
                    .apply_transition(transition)
                    .map_err(|_| RideRecordingError::InvalidTransition)?;
            }
            let transition = recorder
                .validate_transition(RideEvent::Save)
                .map_err(|_| RideRecordingError::InvalidTransition)?;
            recorder
                .apply_transition(transition)
                .map_err(|_| RideRecordingError::InvalidTransition)?;
        }
    }
    [state.recorder, state.admission_recorder] = recorders;
    state.revision = state.revision.saturating_add(1);
    state.generation = state.generation.saturating_add(1);
    Ok(())
}

fn reset_after_recovery(state: &mut RecordingState) {
    state.recorder = super::RideMapRecorder::new();
    state.admission_recorder = state.recorder.clone();
    state.ride_id = None;
    state.checkpoint = None;
    state.persisted_telemetry_receipt = None;
    state.speed = libcutout_persistence::RecordingSpeedState::default();
    state.pending_locations.clear();
    state.music_history_policy = None;
    state.music_history_available = true;
    state.revision = state.revision.saturating_add(1);
    state.generation = state.generation.saturating_add(1);
}

fn clear_projection(state: &mut RecordingState) {
    reset_after_recovery(state);
    state.last_connected_vehicle = None;
}

fn start_replacement(
    state: &mut RecordingState,
    committed: CommittedRideMovingTimeCheckpoint,
    monotonic_now_ms: u64,
    candidate: Option<VehicleIdentity>,
) -> Result<(), RideRecordingError> {
    if committed.lifecycle() != RideLifecycleState::Active {
        return Err(RideRecordingError::InvalidTransition);
    }
    let mut recorder = super::RideMapRecorder::new();
    recorder
        .start(MonotonicMilliseconds::new(monotonic_now_ms), candidate)
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    state.recorder = recorder.clone();
    state.admission_recorder = recorder;
    state.ride_id = Some(committed.ride_id());
    state.checkpoint = Some(committed.checkpoint());
    state.persisted_telemetry_receipt = None;
    state.speed = libcutout_persistence::RecordingSpeedState::default();
    state.speed_generation_started_ms = monotonic_now_ms;
    state.monotonic_epoch_offset_ms = 0;
    state.recoverable_updated_at_ms = None;
    state.pending_locations.clear();
    state.revision = state.revision.saturating_add(1);
    state.generation = state.generation.saturating_add(1);
    Ok(())
}
