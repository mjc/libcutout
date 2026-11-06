//! Application ownership for live ride lifecycle commands and durable restoration.

pub mod music;
mod recovery;
pub mod verified_connection;
pub use recovery::{RideRecoveryDisposition, RideRecoveryPolicy, RideRecoveryResult};
#[cfg(feature = "test-support")]
pub use verified_connection::VerifiedConnectionTestPolicy;
pub use verified_connection::{
    AUTO_RESUME_RIDE_WINDOW_MILLISECONDS, PreparedVerifiedConnectionAdmission,
    VerifiedConnectionAdmissionError,
};

use crate::music::observation_admission::StopFence;
use cutout_music::MusicHistoryPolicy;
use cutout_ride_maps::{
    AdmittedLocationSample, LocationAdmission, LocationRecordingDecision, MonotonicMilliseconds,
    MovingTimeObservation, MovingTimeSource, RideEvent, RideLifecycleState, RideMapMetadata,
    RideMapPoint, RideMapRecorder, RideRecordingTiming, VehicleIdentity,
};
use libcutout_persistence::{
    LocationWriteResult, MovingTimeCheckpoint, PendingLiveRideCreation, PendingLocationWrite,
    PendingRideLifecycleTransition, PendingRideRestoreSnapshot, RecordingSpeed,
    RecordingSpeedState, RideDatabase, RideId, RideLocationObservationCheckpoint,
    RideLocationObservationCheckpointOutcome, RideMotionObservation, RideRestoreSnapshot,
    RideStationaryLocationCheckpoint, StorageError,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, PoisonError, Weak},
};
use thiserror::Error;

const MAX_PENDING_LOCATION_WRITES: usize = 64;

/// Verified vehicle identity attached to one received telemetry frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedRideVehicle {
    /// Canonical platform identifier for the associated wheel.
    pub platform_identifier: String,
    /// Connection generation that authenticated this frame's source.
    pub connection_generation: u64,
}

/// Wheel speed and its original logical source timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideVehicleSpeedSample {
    /// Speed in millimetres per second.
    pub millimetres_per_second: i32,
    /// Monotonic time when the wheel observed this speed.
    pub source_at_ms: u64,
}

/// Result of recording one telemetry receipt and its correlated motion evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideTelemetryResult {
    /// Domain decision for the recorder telemetry clock.
    pub observation: cutout_ride_maps::TelemetryObservation,
    /// Whether durable parked/riding state changed.
    pub motion_changed: bool,
}

#[derive(Clone, Debug)]
struct LatestVehicleMovingTime {
    ride_id: RideId,
    ride_generation: u64,
    connection_generation: u64,
    platform_identifier: String,
    observation: MovingTimeObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestorePhase {
    Pending,
    Ready,
    RecoveryRequired,
}

#[derive(Debug)]
struct RecordingState {
    recorder: RideMapRecorder,
    admission_recorder: RideMapRecorder,
    ride_id: Option<RideId>,
    revision: u64,
    generation: u64,
    speed_generation_started_ms: u64,
    speed: RecordingSpeedState,
    latest_vehicle_moving_time: Option<LatestVehicleMovingTime>,
    checkpoint: Option<MovingTimeCheckpoint>,
    /// Last durable telemetry receipt, which is not treated as fresh after restoration.
    persisted_telemetry_receipt: Option<(RideId, u64)>,
    recoverable_updated_at_ms: Option<u64>,
    last_connected_vehicle: Option<VehicleIdentity>,
    monotonic_epoch_offset_ms: u64,
    last_connection_transition_generation: Option<u64>,
    last_connection_transition_ride_id: Option<RideId>,
    last_connection_admission_generation: Option<u64>,
    music_history_policy: Option<MusicHistoryPolicy>,
    music_history_available: bool,
    /// Reservation counter for cached music-history policy. A receipt applies only when it still
    /// owns this value, so an older policy write cannot replace a newer change or deletion.
    music_cache_epoch: u64,
    pending_locations: VecDeque<PendingLocation>,
    settled_locations: VecDeque<RideLocationWriteOutcome>,
    next_location_request_id: u64,
    next_command_id: u64,
    pending_command_id: Option<u64>,
    restore_phase: RestorePhase,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            recorder: RideMapRecorder::new(),
            admission_recorder: RideMapRecorder::new(),
            ride_id: None,
            revision: 0,
            generation: 0,
            speed_generation_started_ms: 0,
            speed: RecordingSpeedState::default(),
            latest_vehicle_moving_time: None,
            checkpoint: None,
            persisted_telemetry_receipt: None,
            recoverable_updated_at_ms: None,
            last_connected_vehicle: None,
            monotonic_epoch_offset_ms: 0,
            last_connection_transition_generation: None,
            last_connection_transition_ride_id: None,
            last_connection_admission_generation: None,
            music_history_policy: None,
            music_history_available: true,
            music_cache_epoch: 0,
            pending_locations: VecDeque::new(),
            settled_locations: VecDeque::new(),
            next_location_request_id: 1,
            next_command_id: 1,
            pending_command_id: None,
            restore_phase: RestorePhase::Pending,
        }
    }
}

/// Token that fences lifecycle commands against replacement rides and recording intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideRecordingToken {
    /// Canonical persisted ride identity.
    pub ride_id: RideId,
    /// Application generation for the active recording interval.
    pub generation: u64,
}

/// Portable current ride state published after a durable command settles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideRecordingSnapshot {
    /// Canonical ride identity, if a ride is restored or active.
    pub ride_id: Option<RideId>,
    /// Monotonically increasing application projection revision.
    pub revision: u64,
    /// Generation used to reject commands from prior recording intervals.
    pub generation: u64,
    /// Latest logical monotonic timestamp represented by recorder timing.
    pub last_event_at_ms: u64,
    /// Current domain lifecycle state.
    pub lifecycle: Option<RideLifecycleState>,
    /// Token used to fence lifecycle requests against this ride generation.
    pub command_token: Option<RideRecordingToken>,
    /// Latest committed moving-time checkpoint, if database-backed.
    pub checkpoint: Option<MovingTimeCheckpoint>,
    /// Durable telemetry receipt retained for persistence, not as live connection evidence.
    pub persisted_telemetry_receipt: Option<(RideId, u64)>,
    /// Timestamp used to decide whether an interrupted ride remains recoverable.
    pub recoverable_updated_at_ms: Option<u64>,
    /// Last verified vehicle retained as the candidate for a later GPS-only start.
    pub last_connected_vehicle: Option<VehicleIdentity>,
    /// Offset aligning the current process monotonic clock to a restored recording timeline.
    pub monotonic_epoch_offset_ms: u64,
    /// Latest logical recorder timestamp, used for lifecycle operations without caller time.
    pub last_monotonic_ms: u64,
    /// Latest telemetry observation accepted by the recorder.
    pub last_telemetry_at_ms: Option<u64>,
    /// Number of background gaps retained in the route projection.
    pub background_gap_count: u64,
    /// Connection generation last reconciled with this ride.
    pub last_connection_transition_generation: Option<u64>,
    /// Ride against which the last connection generation was reconciled.
    pub last_connection_transition_ride_id: Option<RideId>,
    /// Music-history retention policy restored for this ride, when available.
    pub music_history_policy: Option<MusicHistoryPolicy>,
    /// Whether optional music history was available during restore.
    pub music_history_available: bool,
    /// Number of route points in the current in-memory projection.
    pub route_point_count: u64,
    /// Sequence number of the first retained route point.
    pub first_point_sequence: u64,
    /// Distance retained by the route projection.
    pub distance_millimetres: u64,
    /// Retained route points in canonical sequence order.
    pub route_points: Vec<RideMapPoint>,
    /// Number of canonical route segments.
    pub segment_count: u64,
    /// Recording duration at the latest logical timestamp.
    pub duration_milliseconds: u64,
    /// Telemetry provenance at the latest logical timestamp.
    pub telemetry_state: cutout_ride_maps::RouteTelemetryState,
    /// Candidate vehicle identity, if one is retained.
    pub candidate_vehicle: Option<String>,
    /// Associated verified vehicle identity, if one is retained.
    pub associated_vehicle: Option<String>,
    /// Monotonic vehicle-association timestamp.
    pub associated_at_ms: Option<u64>,
    /// Fresh ride-correlated speed selected at the snapshot timestamp.
    pub speed: Option<RecordingSpeed>,
    /// Latest durable motion state used to reject parked or retired GPS observations.
    pub motion: Option<RideMotionObservation>,
}

/// Failure preparing or settling a durable recording workflow.
#[derive(Debug, Error)]
pub enum RideRecordingError {
    /// Restoration has not completed successfully.
    #[error("ride recording restoration is not ready")]
    RestorationNotReady,
    /// A durable command may have committed, but its result could not be observed.
    #[error("ride recording requires durable restoration after an unknown commit outcome")]
    RecoveryRequired,
    /// Another durable command already owns the lifecycle boundary.
    #[error("a ride recording command is already pending")]
    CommandPending,
    /// No persisted ride is available for a lifecycle transition.
    #[error("there is no active ride")]
    NoActiveRide,
    /// The caller's ride or generation token is stale.
    #[error("ride recording command token is stale")]
    StaleCommand,
    /// The requested lifecycle transition is invalid.
    #[error("ride recording lifecycle transition is invalid")]
    InvalidTransition,
    /// The process-local command identifier space was exhausted.
    #[error("ride recording command identifiers are exhausted")]
    CommandIdExhausted,
    /// Durable storage could not accept or complete the command.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Required music history did not settle before Stop.
    #[error("ride stop is blocked by music history admission: {0:?}")]
    MusicAdmission(crate::music::observation_admission::AdmissionError),
}

#[derive(Debug)]
struct CommandLease {
    state: Weak<Mutex<RecordingState>>,
    id: u64,
}

impl Drop for CommandLease {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
            if state.pending_command_id == Some(self.id) {
                state.pending_command_id = None;
            }
        }
    }
}

/// Prepared asynchronous restoration whose ticket is consumed by `finish_restore`.
#[must_use = "finish the restore operation or drop it to release admission"]
#[derive(Debug)]
pub struct PreparedRideRestore {
    lease: CommandLease,
    storage: Option<PendingRideRestoreSnapshot>,
}

/// Prepared durable start, accepted before the database worker runs.
#[must_use = "finish the start operation or drop it to release admission"]
#[derive(Debug)]
pub struct PreparedRideStart {
    lease: CommandLease,
    at_ms: u64,
    candidate_vehicle: Option<VehicleIdentity>,
    music_history_policy: Option<MusicHistoryPolicy>,
    storage: Option<PendingLiveRideCreation>,
}

/// Prepared durable lifecycle transition, accepted before the database worker runs.
#[must_use = "finish the transition or drop it to release admission"]
#[derive(Debug)]
pub struct PreparedRideTransition {
    lease: CommandLease,
    ride_id: RideId,
    transition: cutout_ride_maps::ValidatedRideTransition,
    at_ms: u64,
    monotonic_epoch_offset_ms: u64,
    event: RideEvent,
    last_telemetry_at_ms: Option<u64>,
    storage: Option<PendingRideLifecycleTransition>,
}

/// Prepared Stop command fenced behind all music history admitted before it.
#[must_use = "finish the stop operation or drop it to release admission"]
#[derive(Debug)]
pub struct PreparedRideStop {
    lease: CommandLease,
    ride_id: RideId,
    transition: cutout_ride_maps::ValidatedRideTransition,
    at_ms: u64,
    monotonic_epoch_offset_ms: u64,
    last_telemetry_at_ms: Option<u64>,
    music_fence: StopFence,
}

#[derive(Debug)]
struct PendingLocation {
    request_id: u64,
    ride_id: RideId,
    admitted: AdmittedLocationSample,
    point: RideMapPoint,
    sequence: u64,
    storage: PendingLocationWrite,
}

/// Result of applying route admission and submitting durable work for one fix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideLocationAdmission {
    /// A durable route write was queued and will be reported by `poll_location_writes`.
    Queued {
        /// Application request identity for the pending write.
        request_id: u64,
        /// Rust-issued route point metadata captured at admission.
        point: RideMapPoint,
        /// Canonical sequence assigned when the sample entered the pending route.
        sequence: u64,
    },
    /// A route point accepted directly by an in-memory recorder with no database worker.
    AcceptedInMemory {
        /// Rust-issued route point metadata captured at admission.
        point: RideMapPoint,
        /// Canonical sequence assigned to the accepted route point.
        sequence: u64,
    },
    /// Repeated live values advanced only the durable observation clock.
    ClockOnly,
    /// The known vehicle was parked; the provider observation advanced without a route point.
    Parked,
    /// The sample predates retained wheel-motion history and cannot use unknown-motion fallback.
    MotionHistoryRetired,
    /// The bounded queue of pending and unsettled durable writes is full.
    QueueFull,
    /// The fix did not satisfy route admission policy.
    Rejected(LocationAdmission),
}

impl RideLocationAdmission {
    /// Whether this outcome admits the sample as a phone-speed observation.
    #[must_use]
    pub const fn advances_phone_speed(self) -> bool {
        match self {
            Self::Queued { .. }
            | Self::AcceptedInMemory { .. }
            | Self::ClockOnly
            | Self::Parked => true,
            _ => false,
        }
    }
}

/// Builds a phone GPS moving-time observation using the shared speed and freshness rules.
#[must_use]
pub fn phone_moving_time_observation(
    speed_meters_per_second: Option<f64>,
    source_at_ms: u64,
    receipt_at_ms: u64,
) -> MovingTimeObservation {
    let speed_millimetres_per_second = receipt_at_ms
        .checked_sub(source_at_ms)
        .filter(|age| *age <= cutout_ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS)
        .and_then(|_| phone_moving_time_speed_millimetres_per_second(speed_meters_per_second));
    MovingTimeObservation {
        source: MovingTimeSource::PhoneGps,
        speed_millimetres_per_second,
        source_at: MonotonicMilliseconds::new(source_at_ms),
        receipt_at: MonotonicMilliseconds::new(receipt_at_ms),
    }
}

fn phone_moving_time_speed_millimetres_per_second(
    speed_meters_per_second: Option<f64>,
) -> Option<i32> {
    let speed = speed_meters_per_second.filter(|speed| speed.is_finite() && *speed >= 0.0)?;
    let millimetres_per_second = speed * 1_000.0;
    if !millimetres_per_second.is_finite() || millimetres_per_second > f64::from(i32::MAX) {
        return None;
    }
    Some(crate::location::round_f64_to_i32(millimetres_per_second))
}

/// Whether a failed location admission still represents an accepted clock-only observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideLocationAdmissionFailureDisposition {
    /// The sample did not advance any recording observation.
    NotObserved,
    /// The sample matched the existing point and advanced only the logical observation clock.
    ClockOnlyObservation,
}

/// Admission failure with the application-level observation disposition retained.
#[derive(Debug)]
pub struct RideLocationAdmissionFailure {
    /// The underlying recording or storage failure.
    pub error: RideRecordingError,
    /// The application decision made before the failure occurred.
    pub disposition: RideLocationAdmissionFailureDisposition,
}

impl From<RideRecordingError> for RideLocationAdmissionFailure {
    fn from(error: RideRecordingError) -> Self {
        Self {
            error,
            disposition: RideLocationAdmissionFailureDisposition::NotObserved,
        }
    }
}

impl From<StorageError> for RideLocationAdmissionFailure {
    fn from(error: StorageError) -> Self {
        Self::from(RideRecordingError::Storage(error))
    }
}

impl RideLocationAdmissionFailure {
    fn clock_only(error: RideRecordingError) -> Self {
        Self {
            error,
            disposition: RideLocationAdmissionFailureDisposition::ClockOnlyObservation,
        }
    }
}

/// One terminal durable result, returned in original submission order.
#[derive(Debug)]
pub struct RideLocationWriteOutcome {
    /// Application request identity allocated at admission.
    pub request_id: u64,
    /// Owning canonical ride.
    pub ride_id: RideId,
    /// Route point metadata captured when the write was admitted.
    pub point: RideMapPoint,
    /// Canonical sequence assigned when the sample entered the pending route.
    pub sequence: u64,
    /// The durable writer's typed admission and sequence result.
    pub result: Result<LocationWriteResult, StorageError>,
    /// Whether the committed point was applied to the current in-memory projection.
    pub projection_applied: bool,
    /// Ride projection captured when this durable outcome settled.
    pub snapshot: RideRecordingSnapshot,
}

/// Rust-owned state and command admission for a database-backed ride recording.
#[derive(Debug)]
pub struct RideRecordingApplication {
    database: Option<RideDatabase>,
    state: Arc<Mutex<RecordingState>>,
}

impl RideRecordingApplication {
    /// Creates an un-restored application owner around the shared database worker.
    #[must_use]
    pub fn new(database: RideDatabase) -> Self {
        Self {
            database: Some(database),
            state: Arc::new(Mutex::new(RecordingState::default())),
        }
    }

    /// Creates an in-memory ride recorder with no durable database worker.
    #[must_use]
    pub fn in_memory() -> Self {
        let state = RecordingState {
            restore_phase: RestorePhase::Ready,
            ..RecordingState::default()
        };
        Self {
            database: None,
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// Queues the consistent durable snapshot needed to recover the current ride.
    ///
    /// # Errors
    /// Returns an error when another command is pending or the storage worker rejects the
    /// restore request.
    pub fn begin_restore(&self) -> Result<PreparedRideRestore, RideRecordingError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.pending_command_id.is_some() {
            return Err(RideRecordingError::CommandPending);
        }
        let id = reserve_command_id(&mut state)?;
        let storage = self
            .database
            .as_ref()
            .map(RideDatabase::queue_restore_snapshot)
            .transpose()?;
        state.pending_command_id = Some(id);
        state.restore_phase = RestorePhase::Pending;
        Ok(PreparedRideRestore {
            lease: CommandLease {
                state: Arc::downgrade(&self.state),
                id,
            },
            storage,
        })
    }

    /// Waits for and atomically publishes the durable ride restoration snapshot.
    ///
    /// # Errors
    /// Returns an error when the restore ticket is stale or the storage worker fails.
    pub fn finish_restore(
        &self,
        operation: PreparedRideRestore,
    ) -> Result<RideRecordingSnapshot, RideRecordingError> {
        let PreparedRideRestore { lease, storage } = operation;
        let id = lease.id;
        let snapshot = storage
            .map(PendingRideRestoreSnapshot::wait_result)
            .transpose()?;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        ensure_command(&state, id)?;
        if state.generation.checked_add(1).is_none() {
            state.restore_phase = RestorePhase::RecoveryRequired;
            state.pending_command_id = None;
            return Err(RideRecordingError::CommandIdExhausted);
        }
        if let Some(snapshot) = snapshot {
            restore_projection(&mut state, &snapshot, false);
        } else {
            state.recorder = RideMapRecorder::new();
            state.admission_recorder = state.recorder.clone();
            state.ride_id = None;
            state.checkpoint = None;
            state.revision = state.revision.saturating_add(1);
            state.generation = state.generation.saturating_add(1);
        }
        state.restore_phase = RestorePhase::Ready;
        state.pending_command_id = None;
        Ok(snapshot_from(&state))
    }

    /// Prepares a new live ride and its optional music-history policy in one durable command.
    ///
    /// # Errors
    /// Returns an error when restoration is incomplete, another command is pending, the current
    /// lifecycle cannot start a ride, or storage rejects creation.
    pub fn begin_start(
        &self,
        at_ms: u64,
        created_at_unix_ms: u64,
        candidate_vehicle: Option<VehicleIdentity>,
        music_history_policy: Option<MusicHistoryPolicy>,
    ) -> Result<PreparedRideStart, RideRecordingError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        if state.pending_command_id.is_some() {
            return Err(RideRecordingError::CommandPending);
        }
        if state
            .recorder
            .state()
            .is_some_and(|lifecycle| match lifecycle {
                RideLifecycleState::Stopped
                | RideLifecycleState::Interrupted
                | RideLifecycleState::Saved
                | RideLifecycleState::Discarded => false,
                _ => true,
            })
        {
            return Err(RideRecordingError::InvalidTransition);
        }
        let id = reserve_command_id(&mut state)?;
        let storage = self
            .database
            .as_ref()
            .map(|database| {
                database.queue_create_started_live_ride_with_music_policy(
                    created_at_unix_ms,
                    at_ms,
                    candidate_vehicle.as_ref().map(VehicleIdentity::as_str),
                    music_history_policy,
                )
            })
            .transpose()?;
        state.pending_command_id = Some(id);
        Ok(PreparedRideStart {
            lease: CommandLease {
                state: Arc::downgrade(&self.state),
                id,
            },
            at_ms,
            candidate_vehicle,
            music_history_policy,
            storage,
        })
    }

    /// Waits for durable creation before publishing the new ride projection.
    ///
    /// # Errors
    /// Returns an error when the start ticket is stale, storage fails, or the committed ride
    /// does not have the expected active lifecycle.
    pub fn finish_start(
        &self,
        operation: PreparedRideStart,
    ) -> Result<RideRecordingSnapshot, RideRecordingError> {
        let PreparedRideStart {
            lease,
            at_ms,
            candidate_vehicle,
            music_history_policy,
            storage,
        } = operation;
        let id = lease.id;
        let committed = match storage
            .map(PendingLiveRideCreation::wait_committed_result)
            .transpose()
        {
            Ok(committed) => committed,
            Err(error) => return Err(self.command_storage_error(id, error)),
        };
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        ensure_command(&state, id)?;
        if committed.is_some_and(|committed| committed.lifecycle() != RideLifecycleState::Active) {
            state.pending_command_id = None;
            state.restore_phase = RestorePhase::RecoveryRequired;
            return Err(RideRecordingError::InvalidTransition);
        }
        if state
            .recorder
            .start(MonotonicMilliseconds::new(at_ms), candidate_vehicle)
            .is_err()
        {
            state.pending_command_id = None;
            state.restore_phase = RestorePhase::RecoveryRequired;
            return Err(RideRecordingError::InvalidTransition);
        }
        state.admission_recorder = state.recorder.clone();
        state.ride_id = Some(committed.map_or_else(
            RideId::new,
            libcutout_persistence::CommittedRideMovingTimeCheckpoint::ride_id,
        ));
        state.checkpoint =
            committed.map(libcutout_persistence::CommittedRideMovingTimeCheckpoint::checkpoint);
        state.speed = RecordingSpeedState::default();
        state.speed_generation_started_ms = at_ms;
        state.latest_vehicle_moving_time = None;
        state.persisted_telemetry_receipt = None;
        state.latest_vehicle_moving_time = None;
        state.music_history_policy = music_history_policy;
        state.revision = state.revision.saturating_add(1);
        state.generation = state.generation.saturating_add(1);
        state.pending_command_id = None;
        Ok(snapshot_from(&state))
    }

    /// Validates a token and queues a lifecycle transition without publishing it early.
    ///
    /// # Errors
    /// Returns an error when restoration is incomplete, a command is pending, the token is stale,
    /// the transition is invalid, or storage rejects the transition.
    pub fn begin_transition(
        &self,
        token: RideRecordingToken,
        event: RideEvent,
        at_ms: u64,
        last_telemetry_at_ms: Option<u64>,
    ) -> Result<PreparedRideTransition, RideRecordingError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        if state.pending_command_id.is_some() {
            return Err(RideRecordingError::CommandPending);
        }
        if state.ride_id != Some(token.ride_id) || state.generation != token.generation {
            return Err(RideRecordingError::StaleCommand);
        }
        let transition = state
            .recorder
            .validate_transition(event)
            .map_err(|_| RideRecordingError::InvalidTransition)?;
        let resumes_saved_timeline = event == RideEvent::Resume
            && state.recorder.state().is_some_and(|lifecycle| {
                lifecycle == RideLifecycleState::Interrupted
                    || lifecycle == RideLifecycleState::Paused
            });
        let monotonic_epoch_offset_ms = if resumes_saved_timeline {
            state
                .recorder
                .recording_timing()
                .last_monotonic_milliseconds()
                .as_u64()
                .saturating_sub(at_ms)
        } else {
            state.monotonic_epoch_offset_ms
        };
        let logical_at_ms = at_ms.saturating_add(monotonic_epoch_offset_ms);
        let id = reserve_command_id(&mut state)?;
        let storage = self
            .database
            .as_ref()
            .map(|database| {
                database.queue_transition_at_with_telemetry(
                    token.ride_id,
                    event,
                    logical_at_ms,
                    last_telemetry_at_ms,
                )
            })
            .transpose()?;
        state.pending_command_id = Some(id);
        Ok(PreparedRideTransition {
            lease: CommandLease {
                state: Arc::downgrade(&self.state),
                id,
            },
            ride_id: token.ride_id,
            transition,
            at_ms: logical_at_ms,
            monotonic_epoch_offset_ms,
            event,
            last_telemetry_at_ms,
            storage,
        })
    }

    /// Commits lifecycle projection only after its matching durable checkpoint is returned.
    ///
    /// # Errors
    /// Returns an error when the operation is stale, storage fails, or the returned checkpoint
    /// does not match the requested transition.
    pub fn finish_transition(
        &self,
        operation: PreparedRideTransition,
    ) -> Result<RideRecordingSnapshot, RideRecordingError> {
        let PreparedRideTransition {
            lease,
            ride_id,
            transition,
            at_ms,
            event,
            last_telemetry_at_ms,
            storage,
            monotonic_epoch_offset_ms,
        } = operation;
        let id = lease.id;
        let committed = match storage
            .map(PendingRideLifecycleTransition::wait_committed_result)
            .transpose()
        {
            Ok(committed) => committed,
            Err(error) => return Err(self.command_storage_error(id, error)),
        };
        let settled_locations = self.poll_location_writes();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        ensure_command(&state, id)?;
        state.settled_locations.extend(settled_locations);
        if state.ride_id != Some(ride_id)
            || committed.is_some_and(|committed| {
                committed.ride_id() != ride_id || committed.lifecycle() != transition.next()
            })
        {
            state.pending_command_id = None;
            if committed.is_some() {
                state.restore_phase = RestorePhase::RecoveryRequired;
            }
            return Err(RideRecordingError::StaleCommand);
        }
        if let Some(at_ms) = last_telemetry_at_ms {
            state.persisted_telemetry_receipt = Some((ride_id, at_ms));
        }
        let at = MonotonicMilliseconds::new(at_ms);
        let mut recorder = state.recorder.clone();
        let mut admission_recorder = state.admission_recorder.clone();
        recorder.apply_transition_at(transition, at).map_err(|_| {
            state.pending_command_id = None;
            state.restore_phase = RestorePhase::RecoveryRequired;
            RideRecordingError::InvalidTransition
        })?;
        admission_recorder
            .apply_transition_at(transition, at)
            .map_err(|_| {
                state.pending_command_id = None;
                state.restore_phase = RestorePhase::RecoveryRequired;
                RideRecordingError::InvalidTransition
            })?;
        state.recorder = recorder;
        state.admission_recorder = admission_recorder;
        state.checkpoint =
            committed.map(libcutout_persistence::CommittedRideMovingTimeCheckpoint::checkpoint);
        state.monotonic_epoch_offset_ms = monotonic_epoch_offset_ms;
        state.revision = state.revision.saturating_add(1);
        state.generation = state.generation.saturating_add(1);
        state.latest_vehicle_moving_time = None;
        if event == RideEvent::Resume {
            state.recoverable_updated_at_ms = None;
            state.speed_generation_started_ms = at_ms;
        }
        let snapshot = snapshot_from(&state);
        if event == RideEvent::Discard {
            state.ride_id = None;
            state.checkpoint = None;
            state.persisted_telemetry_receipt = None;
            state.recoverable_updated_at_ms = None;
            state.music_history_policy = None;
            state.music_history_available = false;
            state.pending_locations.clear();
        }
        state.pending_command_id = None;
        Ok(snapshot)
    }

    /// Reserves a Stop command while retaining the music queue's FIFO fence.
    ///
    /// # Errors
    /// Returns an error when restoration is incomplete, another command is pending, or the
    /// recording token is stale or cannot transition to Stop.
    pub fn begin_stop_after_music(
        &self,
        token: RideRecordingToken,
        at_ms: u64,
        last_telemetry_at_ms: Option<u64>,
        music_fence: StopFence,
    ) -> Result<PreparedRideStop, RideRecordingError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        if state.pending_command_id.is_some() {
            return Err(RideRecordingError::CommandPending);
        }
        if state.ride_id != Some(token.ride_id) || state.generation != token.generation {
            return Err(RideRecordingError::StaleCommand);
        }
        let transition = state
            .recorder
            .validate_transition(RideEvent::Stop)
            .map_err(|_| RideRecordingError::InvalidTransition)?;
        let id = reserve_command_id(&mut state)?;
        let monotonic_epoch_offset_ms = state.monotonic_epoch_offset_ms;
        let logical_at_ms = at_ms.saturating_add(monotonic_epoch_offset_ms);
        state.pending_command_id = Some(id);
        Ok(PreparedRideStop {
            lease: CommandLease {
                state: Arc::downgrade(&self.state),
                id,
            },
            ride_id: token.ride_id,
            transition,
            at_ms: logical_at_ms,
            monotonic_epoch_offset_ms,
            last_telemetry_at_ms,
            music_fence,
        })
    }

    /// Waits for the music FIFO fence, commits Stop, and then publishes the new projection.
    ///
    /// # Errors
    /// Returns an error when the command token is stale, required music history is incomplete,
    /// storage fails, or the committed checkpoint disagrees with the requested transition.
    pub fn finish_stop_after_music(
        &self,
        operation: PreparedRideStop,
    ) -> Result<RideRecordingSnapshot, RideRecordingError> {
        let PreparedRideStop {
            lease,
            ride_id,
            transition,
            at_ms,
            monotonic_epoch_offset_ms,
            last_telemetry_at_ms,
            music_fence,
        } = operation;
        music_fence
            .wait_result()
            .map_err(RideRecordingError::MusicAdmission)?;
        let command_id = lease.id;
        let committed = match self
            .database
            .as_ref()
            .map(|database| {
                database
                    .queue_transition_at_with_telemetry(
                        ride_id,
                        RideEvent::Stop,
                        at_ms,
                        last_telemetry_at_ms,
                    )?
                    .wait_committed_result()
            })
            .transpose()
        {
            Ok(committed) => committed,
            Err(error) => return Err(self.command_storage_error(command_id, error)),
        };
        let settled_locations = self.poll_location_writes();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        ensure_command(&state, command_id)?;
        state.settled_locations.extend(settled_locations);
        if state.ride_id != Some(ride_id)
            || committed.is_some_and(|committed| {
                committed.ride_id() != ride_id || committed.lifecycle() != transition.next()
            })
        {
            state.pending_command_id = None;
            if committed.is_some() {
                state.restore_phase = RestorePhase::RecoveryRequired;
            }
            return Err(RideRecordingError::StaleCommand);
        }
        if let Some(receipt_at_ms) = last_telemetry_at_ms {
            state.persisted_telemetry_receipt = Some((ride_id, receipt_at_ms));
        }
        let at = MonotonicMilliseconds::new(at_ms);
        if state.recorder.apply_transition_at(transition, at).is_err() {
            state.pending_command_id = None;
            state.restore_phase = RestorePhase::RecoveryRequired;
            return Err(RideRecordingError::InvalidTransition);
        }
        if state
            .admission_recorder
            .apply_transition_at(transition, at)
            .is_err()
        {
            state.pending_command_id = None;
            state.restore_phase = RestorePhase::RecoveryRequired;
            return Err(RideRecordingError::InvalidTransition);
        }
        state.checkpoint =
            committed.map(libcutout_persistence::CommittedRideMovingTimeCheckpoint::checkpoint);
        state.monotonic_epoch_offset_ms = monotonic_epoch_offset_ms;
        state.latest_vehicle_moving_time = None;
        state.revision = state.revision.saturating_add(1);
        state.generation = state.generation.saturating_add(1);
        state.latest_vehicle_moving_time = None;
        state.pending_command_id = None;
        Ok(snapshot_from(&state))
    }

    /// Returns the latest committed application projection.
    #[must_use]
    pub fn snapshot(&self) -> RideRecordingSnapshot {
        snapshot_from(&self.state.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Returns whether an unsettled durable result requires restoring the ride projection.
    #[must_use]
    pub fn requires_restore(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .restore_phase
            == RestorePhase::RecoveryRequired
    }

    /// Sets the generation counter for adapter tests that exercise exhaustion handling.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn set_generation_for_test(&self, generation: u64) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .generation = generation;
    }

    /// Returns the latest telemetry receipt that still needs a durable lifecycle checkpoint.
    #[must_use]
    pub fn pending_telemetry_receipt(&self) -> Option<u64> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let ride_id = state.ride_id?;
        let at_ms = state.recorder.last_telemetry_at_milliseconds()?.as_u64();
        (state.persisted_telemetry_receipt != Some((ride_id, at_ms))).then_some(at_ms)
    }

    fn command_storage_error(&self, command_id: u64, error: StorageError) -> RideRecordingError {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.pending_command_id == Some(command_id) {
            state.pending_command_id = None;
            match &error {
                StorageError::ResponseDropped | StorageError::DeadlineExceeded => {
                    state.restore_phase = RestorePhase::RecoveryRequired;
                }
                _ => {}
            }
        }
        RideRecordingError::Storage(error)
    }

    /// Returns the current recording projection evaluated at a caller-supplied logical time.
    ///
    /// This lets an FFI adapter derive elapsed route values at the same timestamp used for
    /// its other live statistics without taking ownership of the recorder or its clock offset.
    #[must_use]
    pub fn snapshot_at(&self, logical_at_ms: u64) -> RideRecordingSnapshot {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        snapshot_from_at(&state, MonotonicMilliseconds::new(logical_at_ms))
    }

    /// Admits phone GPS speed evidence for the current recording generation.
    pub fn observe_phone_gps_speed(
        &self,
        metres_per_second: Option<f64>,
        source_at_ms: u64,
        generation: u64,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.generation != generation || source_at_ms < state.speed_generation_started_ms {
            return false;
        }
        state
            .speed
            .observe_phone_gps(metres_per_second, source_at_ms, generation);
        true
    }

    /// Admits speed evidence only after the caller verifies its associated vehicle identity.
    ///
    /// The return value says whether durable parked/riding motion state changed.
    pub fn observe_verified_vehicle_speed(
        &self,
        millimetres_per_second: Option<i32>,
        source_at_ms: u64,
        receipt_at_ms: u64,
        generation: u64,
    ) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.generation != generation
            || source_at_ms < state.speed_generation_started_ms
            || source_at_ms > receipt_at_ms
            || receipt_at_ms.saturating_sub(source_at_ms)
                > cutout_ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS
        {
            return false;
        }
        let Some(speed) = millimetres_per_second else {
            return false;
        };
        state
            .speed
            .observe_vehicle(Some(speed), source_at_ms, generation);
        state
            .speed
            .observe_verified_motion(speed, source_at_ms, receipt_at_ms)
    }

    /// Returns motion evidence at a route sample's logical source time.
    #[must_use]
    pub fn motion_at(&self, source_at_ms: u64) -> Option<RideMotionObservation> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .speed
            .motion_at(source_at_ms)
    }

    /// Returns whether the route sample predates retained verified motion history.
    #[must_use]
    pub fn motion_history_retired_at(&self, source_at_ms: u64) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .speed
            .motion_history_retired_at(source_at_ms)
    }

    /// Returns whether the associated wheel was parked at a route sample's source time.
    #[must_use]
    pub fn parked_at(&self, source_at_ms: u64) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .speed
            .parked_at(source_at_ms)
    }

    /// Restores persisted wheel motion without creating current speed or connection evidence.
    pub fn restore_motion(&self, motion: RideMotionObservation) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .speed
            .restore_motion(motion);
    }

    /// Reconciles one vehicle telemetry receipt with its optional verified speed evidence.
    ///
    /// Timestamps are logical ride monotonic milliseconds, already aligned by the caller.
    /// The owner persists association, receipt, motion and moving-time changes together.
    ///
    /// # Errors
    /// Returns an error when the recording has not been restored or persistence of updated ride
    /// metadata fails.
    #[expect(
        clippy::too_many_lines,
        reason = "Telemetry, motion, association, and checkpoint publication are staged under one lock to preserve atomic ordering."
    )]
    pub fn observe_telemetry(
        &self,
        at_ms: u64,
        speed: Option<RideVehicleSpeedSample>,
        verified_vehicle: Option<&VerifiedRideVehicle>,
    ) -> Result<RideTelemetryResult, RideRecordingError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        let mut staged = state.admission_recorder.clone();
        let mut durable_staged = state.recorder.clone();
        let observation = staged.observe_telemetry(MonotonicMilliseconds::new(at_ms));
        let accepted = observation == cutout_ride_maps::TelemetryObservation::Observed
            || observation == cutout_ride_maps::TelemetryObservation::AlreadyObserved;
        let mut staged_speed = state.speed;
        let mut motion_changed = false;
        let mut moving_time_observation = None;
        let mut latest_vehicle_moving_time = None;

        if accepted && let Some(speed) = speed {
            let source_at_ms = speed.source_at_ms;
            if source_at_ms >= state.speed_generation_started_ms && source_at_ms <= at_ms {
                let generation = state.generation;
                staged_speed.observe_vehicle(
                    Some(speed.millimetres_per_second),
                    source_at_ms,
                    generation,
                );
                if let Some(verified_vehicle) = verified_vehicle.as_ref() {
                    motion_changed = staged_speed.observe_verified_motion(
                        speed.millimetres_per_second,
                        source_at_ms,
                        at_ms,
                    );
                    let ride_id = state.ride_id;
                    let active = staged.state() == Some(RideLifecycleState::Active);
                    if active
                        && at_ms.saturating_sub(source_at_ms)
                            <= cutout_ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS
                        && staged.associated_vehicle()
                            == Some(verified_vehicle.platform_identifier.as_str())
                        && state.last_connection_transition_generation
                            == Some(verified_vehicle.connection_generation)
                        && state.last_connection_transition_ride_id == ride_id
                        && let Some(ride_id) = ride_id
                    {
                        let candidate = MovingTimeObservation {
                            source: MovingTimeSource::Vehicle,
                            speed_millimetres_per_second: Some(speed.millimetres_per_second),
                            source_at: MonotonicMilliseconds::new(source_at_ms),
                            receipt_at: MonotonicMilliseconds::new(at_ms),
                        };
                        let is_newer_than_cache = state
                            .latest_vehicle_moving_time
                            .as_ref()
                            .is_none_or(|latest| {
                                latest.observation.source_at.as_u64() < source_at_ms
                            });
                        let is_newer_than_checkpoint = state.checkpoint.is_none_or(|checkpoint| {
                            checkpoint
                                .continuation()
                                .state_parts()
                                .latest_vehicle_source_milliseconds
                                .is_none_or(|committed| committed < source_at_ms)
                        });
                        if is_newer_than_cache || is_newer_than_checkpoint {
                            moving_time_observation = Some(candidate);
                            latest_vehicle_moving_time = Some(LatestVehicleMovingTime {
                                ride_id,
                                ride_generation: state.generation,
                                connection_generation: verified_vehicle.connection_generation,
                                platform_identifier: verified_vehicle.platform_identifier.clone(),
                                observation: candidate,
                            });
                        }
                    }
                }
            }
        }

        if observation == cutout_ride_maps::TelemetryObservation::Observed || motion_changed {
            let _ = durable_staged.observe_telemetry(MonotonicMilliseconds::new(at_ms));
            if (state
                .persisted_telemetry_receipt
                .map(|(ride_id, _)| ride_id)
                != state.ride_id
                || motion_changed)
                && let (Some(database), Some(ride_id)) = (&self.database, state.ride_id)
            {
                let checkpoint = database.update_ride_map_metadata_with_moving_time(
                    ride_id,
                    staged.candidate_vehicle(),
                    staged.associated_vehicle(),
                    staged
                        .associated_at_milliseconds()
                        .map(MonotonicMilliseconds::as_u64),
                    staged
                        .last_telemetry_at_milliseconds()
                        .map(MonotonicMilliseconds::as_u64),
                    staged_speed.latest_motion(),
                    moving_time_observation,
                )?;
                publish_checkpoint(&mut state, ride_id, checkpoint);
                state.persisted_telemetry_receipt = Some((ride_id, at_ms));
            }
            state.revision = state.revision.saturating_add(1);
        }
        state.recorder = durable_staged;
        state.admission_recorder = staged;
        state.speed = staged_speed;
        if let Some(latest) = latest_vehicle_moving_time {
            state.latest_vehicle_moving_time = Some(latest);
        }
        Ok(RideTelemetryResult {
            observation,
            motion_changed,
        })
    }

    /// Returns a fresh verified wheel observation that may share a location transaction.
    #[must_use]
    pub fn verified_vehicle_observation_for_receipt(
        &self,
        receipt_at_ms: u64,
    ) -> Option<MovingTimeObservation> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        verified_vehicle_observation_for_state(&state, receipt_at_ms)
    }

    /// Applies shared route-admission policy and queues a durable live location point.
    ///
    /// Clock-only repeats are checkpointed immediately on the recording executor. Material
    /// points use the bounded SQLite worker and settle in FIFO order through `poll_location_writes`.
    ///
    /// # Errors
    /// Returns an error when restoration is incomplete, no ride is active, storage fails, or the
    /// route projection becomes stale while the write is being admitted.
    pub fn admit_location(
        &self,
        sample: cutout_ride_maps::LocationSample,
    ) -> Result<RideLocationAdmission, RideRecordingError> {
        self.admit_location_with_moving_time(sample, None)
    }

    /// Admits a location with GPS timing and the latest fresh verified wheel timing.
    ///
    /// # Errors
    /// Returns an error when restoration is incomplete, no ride is active, storage fails, or the
    /// route projection becomes stale while the write is being admitted.
    pub fn admit_location_with_moving_time(
        &self,
        sample: cutout_ride_maps::LocationSample,
        moving_time_observation: Option<MovingTimeObservation>,
    ) -> Result<RideLocationAdmission, RideRecordingError> {
        self.admit_location_with_moving_time_outcome(sample, moving_time_observation)
            .map_err(|failure| failure.error)
    }

    /// Admits a location and preserves whether a failed persistence attempt came from a
    /// clock-only observation rather than a route-point admission.
    ///
    /// # Errors
    /// Returns the underlying recording or storage error together with the decision already
    /// made by the application admission policy.
    pub fn admit_location_with_moving_time_outcome(
        &self,
        sample: cutout_ride_maps::LocationSample,
        moving_time_observation: Option<MovingTimeObservation>,
    ) -> Result<RideLocationAdmission, RideLocationAdmissionFailure> {
        self.admit_location_with_policy(sample, moving_time_observation, false)
    }

    /// Admits a location while waiting for bounded database queue capacity when required.
    ///
    /// # Errors
    /// Returns an error when restoration is incomplete, no ride is active, storage fails, or the
    /// route projection becomes stale while the write is being admitted.
    pub fn admit_location_waiting_for_capacity(
        &self,
        sample: cutout_ride_maps::LocationSample,
        moving_time_observation: Option<MovingTimeObservation>,
    ) -> Result<RideLocationAdmission, RideRecordingError> {
        self.admit_location_waiting_for_capacity_outcome(sample, moving_time_observation)
            .map_err(|failure| failure.error)
    }

    /// Admits a location while preserving the application disposition if admission fails.
    ///
    /// # Errors
    /// Returns the underlying recording or storage error together with the decision already
    /// made by the application admission policy.
    pub fn admit_location_waiting_for_capacity_outcome(
        &self,
        sample: cutout_ride_maps::LocationSample,
        moving_time_observation: Option<MovingTimeObservation>,
    ) -> Result<RideLocationAdmission, RideLocationAdmissionFailure> {
        self.admit_location_with_policy(sample, moving_time_observation, true)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Admission and its storage queue insertion are one ordered state transition; splitting it would obscure bounded FIFO and recorder projection invariants."
    )]
    fn admit_location_with_policy(
        &self,
        sample: cutout_ride_maps::LocationSample,
        moving_time_observation: Option<MovingTimeObservation>,
        wait_for_capacity: bool,
    ) -> Result<RideLocationAdmission, RideLocationAdmissionFailure> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        let ride_id = state.ride_id.ok_or(RideRecordingError::NoActiveRide)?;
        if state.recorder.associated_vehicle().is_some()
            && state
                .speed
                .motion_history_retired_at(sample.monotonic_milliseconds().as_u64())
        {
            return Ok(RideLocationAdmission::MotionHistoryRetired);
        }
        if state.recorder.associated_vehicle().is_some()
            && let Some(at) = state
                .speed
                .motion_at(sample.monotonic_milliseconds().as_u64())
                .and_then(|motion| motion.last_stationary_at_milliseconds)
        {
            state
                .recorder
                .mark_stationary_boundary(MonotonicMilliseconds::new(at));
            state
                .admission_recorder
                .mark_stationary_boundary(MonotonicMilliseconds::new(at));
        }
        if state.recorder.associated_vehicle().is_some()
            && state
                .speed
                .parked_at(sample.monotonic_milliseconds().as_u64())
        {
            if let Some(database) = self.database.as_ref()
                && !state.pending_locations.is_empty()
            {
                database.wait_for_ride_checkpoint()?;
                let outcomes = settle_ready_location_writes(&mut state);
                state.settled_locations.extend(outcomes);
            }
            let observation = match state.recorder.admit_stationary_sample(sample) {
                Ok(observation) => observation,
                Err(reason) => return Ok(RideLocationAdmission::Rejected(reason)),
            };
            if let Some(database) = self.database.as_ref() {
                match database.checkpoint_stationary_location_observation(
                    RideStationaryLocationCheckpoint {
                        ride_id,
                        expected_point_count: state.recorder.summary().point_count().as_u64(),
                        observation: sample,
                    },
                )? {
                    RideLocationObservationCheckpointOutcome::Applied
                    | RideLocationObservationCheckpointOutcome::AlreadyObserved => {}
                    RideLocationObservationCheckpointOutcome::PointChanged => {
                        return Err(RideRecordingError::StaleCommand.into());
                    }
                }
            }
            if !state.recorder.observe_stationary(observation) {
                return Err(RideRecordingError::StaleCommand.into());
            }
            state.admission_recorder = state.recorder.clone();
            state.revision = state.revision.saturating_add(1);
            return Ok(RideLocationAdmission::Parked);
        }
        let decision = if state.pending_locations.is_empty() {
            state.recorder.classify_sample(sample)
        } else {
            state
                .admission_recorder
                .admit_sample(sample)
                .map_or_else(
                    RideLocationAdmissionDecision::Rejected,
                    RideLocationAdmissionDecision::Record,
                )
                .into()
        };
        let admitted = match decision {
            LocationRecordingDecision::Rejected(reason) => {
                return Ok(RideLocationAdmission::Rejected(reason));
            }
            LocationRecordingDecision::ObserveClockOnly(observation) => {
                let expected_point_sequence = state
                    .recorder
                    .summary()
                    .point_count()
                    .as_u64()
                    .checked_sub(1)
                    .ok_or_else(|| {
                        RideLocationAdmissionFailure::clock_only(
                            RideRecordingError::InvalidTransition,
                        )
                    })?;
                if let Some(database) = self.database.as_ref() {
                    match database
                        .checkpoint_location_observation(RideLocationObservationCheckpoint {
                            ride_id,
                            expected_point_sequence,
                            observation: sample,
                        })
                        .map_err(|error| RideLocationAdmissionFailure::clock_only(error.into()))?
                    {
                        RideLocationObservationCheckpointOutcome::Applied
                        | RideLocationObservationCheckpointOutcome::AlreadyObserved => {}
                        RideLocationObservationCheckpointOutcome::PointChanged => {
                            return Err(RideLocationAdmissionFailure::clock_only(
                                RideRecordingError::StaleCommand,
                            ));
                        }
                    }
                }
                if !state.recorder.observe_clock_only(observation) {
                    return Err(RideLocationAdmissionFailure::clock_only(
                        RideRecordingError::StaleCommand,
                    ));
                }
                state.admission_recorder = state.recorder.clone();
                state.revision = state.revision.saturating_add(1);
                return Ok(RideLocationAdmission::ClockOnly);
            }
            LocationRecordingDecision::Record(admitted) => admitted,
        };

        let telemetry_state = state
            .admission_recorder
            .telemetry_state_at(sample.monotonic_milliseconds());
        let (segment_id, _, start_reason) = state.admission_recorder.next_sample_metadata(sample);
        let point =
            RideMapPoint::new_with_start_reason(sample, segment_id, telemetry_state, start_reason);
        let sequence = state.admission_recorder.summary().point_count().as_u64();
        let request_id = state.next_location_request_id;
        let next_location_request_id = request_id
            .checked_add(1)
            .ok_or(RideRecordingError::CommandIdExhausted)?;
        let mut admission_recorder = state.admission_recorder.clone();
        if !admission_recorder.record_admitted_sample(admitted) {
            return Err(RideRecordingError::InvalidTransition.into());
        }
        let Some(database) = self.database.as_ref() else {
            if !state.recorder.record_admitted_sample(admitted) {
                return Err(RideRecordingError::InvalidTransition.into());
            }
            state.admission_recorder = state.recorder.clone();
            state.revision = state.revision.saturating_add(1);
            return Ok(RideLocationAdmission::AcceptedInMemory { point, sequence });
        };
        if state
            .pending_locations
            .len()
            .saturating_add(state.settled_locations.len())
            >= MAX_PENDING_LOCATION_WRITES
        {
            return Ok(RideLocationAdmission::QueueFull);
        }
        let write = if let Some(moving_time_observation) = moving_time_observation {
            let latest_vehicle_observation = verified_vehicle_observation_for_state(
                &state,
                moving_time_observation.receipt_at.as_u64(),
            );
            if wait_for_capacity {
                database.queue_location_with_moving_time_waiting_for_capacity(
                    ride_id,
                    sample,
                    point.segment_id(),
                    point.segment_start_reason(),
                    point.telemetry_state(),
                    moving_time_observation,
                    latest_vehicle_observation,
                )?
            } else {
                database.queue_location_with_moving_time(
                    ride_id,
                    sample,
                    point.segment_id(),
                    point.segment_start_reason(),
                    point.telemetry_state(),
                    moving_time_observation,
                    latest_vehicle_observation,
                )?
            }
        } else if wait_for_capacity {
            database.queue_location_waiting_for_capacity(
                ride_id,
                sample,
                point.segment_id(),
                point.segment_start_reason(),
                point.telemetry_state(),
            )?
        } else {
            database.queue_location(
                ride_id,
                sample,
                point.segment_id(),
                point.segment_start_reason(),
                point.telemetry_state(),
            )?
        };
        state.next_location_request_id = next_location_request_id;
        state.admission_recorder = admission_recorder;
        state.pending_locations.push_back(PendingLocation {
            request_id,
            ride_id,
            admitted,
            point,
            sequence,
            storage: write,
        });
        Ok(RideLocationAdmission::Queued {
            request_id,
            point,
            sequence,
        })
    }

    /// Settles ready location writes without overtaking an unfinished predecessor.
    pub fn poll_location_writes(&self) -> Vec<RideLocationWriteOutcome> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        settle_ready_location_writes(&mut state)
    }

    /// Persists the latest telemetry receipt, then settles all earlier location writes.
    ///
    /// The receipt is kept live in the application recorder after the first durable write so
    /// later packets can refresh freshness without rewriting SQLite for every packet. This
    /// explicit barrier commits the latest receipt before publishing location outcomes.
    ///
    /// # Errors
    /// Returns an error when the telemetry metadata update or a prior location write fails.
    pub fn checkpoint(&self) -> Result<Vec<RideLocationWriteOutcome>, RideRecordingError> {
        self.checkpoint_telemetry_receipt()?;
        self.checkpoint_location_writes()
    }

    /// Persists the latest accepted telemetry receipt without waiting for location writes.
    ///
    /// This is useful when a caller must report telemetry persistence failure while still
    /// settling an independent location callback.
    ///
    /// # Errors
    /// Returns an error when the telemetry receipt cannot be persisted.
    pub fn checkpoint_telemetry_receipt(&self) -> Result<(), RideRecordingError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;
        let (Some(ride_id), Some(at_ms)) = (
            state.ride_id,
            state
                .recorder
                .last_telemetry_at_milliseconds()
                .map(MonotonicMilliseconds::as_u64),
        ) else {
            return Ok(());
        };
        if state.persisted_telemetry_receipt == Some((ride_id, at_ms)) {
            return Ok(());
        }
        let Some(database) = self.database.as_ref() else {
            return Ok(());
        };
        let checkpoint = database.update_ride_map_metadata_with_moving_time(
            ride_id,
            state.recorder.candidate_vehicle(),
            state.recorder.associated_vehicle(),
            state
                .recorder
                .associated_at_milliseconds()
                .map(MonotonicMilliseconds::as_u64),
            Some(at_ms),
            state.speed.latest_motion(),
            None,
        )?;
        publish_checkpoint(&mut state, ride_id, checkpoint);
        state.persisted_telemetry_receipt = Some((ride_id, at_ms));
        Ok(())
    }

    /// Waits for all earlier location commands to settle and returns their ordered outcomes.
    ///
    /// # Errors
    /// Returns an error when the database worker fails while fencing earlier location writes.
    pub fn checkpoint_location_writes(
        &self,
    ) -> Result<Vec<RideLocationWriteOutcome>, RideRecordingError> {
        if let Some(database) = self.database.as_ref() {
            database.wait_for_ride_checkpoint()?;
        }
        Ok(self.poll_location_writes())
    }

    /// Reports whether a durable location outcome still needs publication.
    #[must_use]
    pub fn has_pending_location_writes(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        !state.pending_locations.is_empty() || !state.settled_locations.is_empty()
    }
}

fn settle_ready_location_writes(state: &mut RecordingState) -> Vec<RideLocationWriteOutcome> {
    let mut outcomes = state.settled_locations.drain(..).collect::<Vec<_>>();
    while let Some(pending) = state.pending_locations.front_mut() {
        let result = pending.storage.try_result();
        let Some(result) = result else { break };
        let Some(pending) = state.pending_locations.pop_front() else {
            break;
        };
        let is_current = state.ride_id == Some(pending.ride_id);
        let projection_applied = match &result {
            Ok(write) if is_current && write.admission() == LocationAdmission::Accepted => {
                let applied = state.recorder.record_admitted_sample(pending.admitted);
                if applied {
                    publish_checkpoint(state, pending.ride_id, write.moving_time_checkpoint());
                    state.revision = state.revision.saturating_add(1);
                }
                applied
            }
            _ => false,
        };
        outcomes.push(RideLocationWriteOutcome {
            request_id: pending.request_id,
            ride_id: pending.ride_id,
            point: pending.point,
            sequence: pending.sequence,
            result,
            projection_applied,
            snapshot: snapshot_from(state),
        });
    }
    let mut rebuilt = state.recorder.clone();
    for pending in &state.pending_locations {
        let _ = rebuilt.record_admitted_sample(pending.admitted);
    }
    state.admission_recorder = rebuilt;
    outcomes
}

enum RideLocationAdmissionDecision {
    Record(AdmittedLocationSample),
    Rejected(LocationAdmission),
}

impl From<RideLocationAdmissionDecision> for LocationRecordingDecision {
    fn from(decision: RideLocationAdmissionDecision) -> Self {
        match decision {
            RideLocationAdmissionDecision::Record(admitted) => Self::Record(admitted),
            RideLocationAdmissionDecision::Rejected(reason) => Self::Rejected(reason),
        }
    }
}

fn reserve_command_id(state: &mut RecordingState) -> Result<u64, RideRecordingError> {
    let id = state.next_command_id;
    state.next_command_id = id
        .checked_add(1)
        .ok_or(RideRecordingError::CommandIdExhausted)?;
    Ok(id)
}

fn ensure_command(state: &RecordingState, id: u64) -> Result<(), RideRecordingError> {
    if state.pending_command_id == Some(id) {
        Ok(())
    } else {
        Err(RideRecordingError::StaleCommand)
    }
}

fn publish_checkpoint(
    state: &mut RecordingState,
    ride_id: RideId,
    checkpoint: MovingTimeCheckpoint,
) {
    if state.ride_id != Some(ride_id)
        || state
            .checkpoint
            .is_some_and(|current| current.revision() >= checkpoint.revision())
    {
        return;
    }
    state.checkpoint = Some(checkpoint);
}

fn verified_vehicle_observation_for_state(
    state: &RecordingState,
    receipt_at_ms: u64,
) -> Option<MovingTimeObservation> {
    let latest = state.latest_vehicle_moving_time.as_ref()?;
    let ride_id = state.ride_id?;
    if latest.ride_id != ride_id
        || latest.ride_generation != state.generation
        || state.last_connection_transition_generation != Some(latest.connection_generation)
        || state.last_connection_transition_ride_id != Some(ride_id)
        || state
            .last_connected_vehicle
            .as_ref()
            .map(VehicleIdentity::as_str)
            != Some(latest.platform_identifier.as_str())
        || state.recorder.associated_vehicle() != Some(latest.platform_identifier.as_str())
    {
        return None;
    }
    let source_at_ms = latest.observation.source_at.as_u64();
    if source_at_ms > receipt_at_ms
        || receipt_at_ms.saturating_sub(source_at_ms)
            > cutout_ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS
        || state.checkpoint.is_some_and(|checkpoint| {
            checkpoint
                .continuation()
                .state_parts()
                .latest_vehicle_source_milliseconds
                .is_some_and(|committed| committed >= source_at_ms)
        })
    {
        return None;
    }
    let mut observation = latest.observation;
    observation.receipt_at = MonotonicMilliseconds::new(receipt_at_ms);
    Some(observation)
}

fn require_ready(state: &RecordingState) -> Result<(), RideRecordingError> {
    match state.restore_phase {
        RestorePhase::Ready => Ok(()),
        RestorePhase::Pending => Err(RideRecordingError::RestorationNotReady),
        RestorePhase::RecoveryRequired => Err(RideRecordingError::RecoveryRequired),
    }
}

fn snapshot_from(state: &RecordingState) -> RideRecordingSnapshot {
    snapshot_from_at(
        state,
        state
            .recorder
            .recording_timing()
            .last_monotonic_milliseconds(),
    )
}

fn snapshot_from_at(
    state: &RecordingState,
    latest_at: MonotonicMilliseconds,
) -> RideRecordingSnapshot {
    let lifecycle = state.recorder.state();
    let command_token = state.ride_id.map(|ride_id| RideRecordingToken {
        ride_id,
        generation: state.generation,
    });
    RideRecordingSnapshot {
        ride_id: state.ride_id,
        revision: state.revision,
        generation: state.generation,
        last_event_at_ms: state
            .recorder
            .recording_timing()
            .last_monotonic_milliseconds()
            .as_u64(),
        lifecycle,
        command_token,
        checkpoint: state.checkpoint,
        persisted_telemetry_receipt: state.persisted_telemetry_receipt,
        recoverable_updated_at_ms: state.recoverable_updated_at_ms,
        last_connected_vehicle: state.last_connected_vehicle.clone(),
        monotonic_epoch_offset_ms: state.monotonic_epoch_offset_ms,
        last_monotonic_ms: state
            .recorder
            .recording_timing()
            .last_monotonic_milliseconds()
            .as_u64(),
        last_telemetry_at_ms: state
            .recorder
            .last_telemetry_at_milliseconds()
            .map(MonotonicMilliseconds::as_u64),
        background_gap_count: state.recorder.background_gap_count().as_u64(),
        last_connection_transition_generation: state.last_connection_transition_generation,
        last_connection_transition_ride_id: state.last_connection_transition_ride_id,
        music_history_policy: state.music_history_policy,
        music_history_available: state.music_history_available,
        route_point_count: state.recorder.summary().point_count().as_u64(),
        first_point_sequence: state.recorder.first_point_sequence().as_u64(),
        distance_millimetres: state.recorder.summary().distance_millimetres(),
        route_points: state.recorder.points().to_vec(),
        segment_count: state.recorder.segment_count().as_u64(),
        duration_milliseconds: state.recorder.duration_milliseconds_at(latest_at).as_u64(),
        telemetry_state: state.recorder.telemetry_state_at(latest_at),
        candidate_vehicle: state.recorder.candidate_vehicle().map(str::to_owned),
        associated_vehicle: state.recorder.associated_vehicle().map(str::to_owned),
        associated_at_ms: state
            .recorder
            .associated_at_milliseconds()
            .map(MonotonicMilliseconds::as_u64),
        speed: lifecycle.and_then(|lifecycle| {
            state
                .speed
                .selected_at(lifecycle, state.generation, latest_at.as_u64())
        }),
        motion: state.speed.latest_motion(),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "The durable snapshot must be projected into all recording-owned state together."
)]
fn restore_projection(
    state: &mut RecordingState,
    restored: &RideRestoreSnapshot,
    apply_automatic_recovery: bool,
) {
    state.last_connection_transition_generation = None;
    state.last_connection_transition_ride_id = None;
    state.last_connection_admission_generation = None;
    state.last_connected_vehicle = restored
        .last_connected_device()
        .and_then(VehicleIdentity::new);
    state.music_history_policy = restored.music_history_policy();
    state.music_history_available = restored.music_history_available();
    let Some(ride) = restored.ride() else {
        state.recorder = RideMapRecorder::new();
        state.admission_recorder = RideMapRecorder::new();
        state.speed = RecordingSpeedState::default();
        state.ride_id = None;
        state.checkpoint = None;
        state.persisted_telemetry_receipt = None;
        state.latest_vehicle_moving_time = None;
        state.recoverable_updated_at_ms = None;
        state.revision = state.revision.saturating_add(1);
        state.generation = state.generation.saturating_add(1);
        return;
    };
    let points: Vec<_> = restored
        .route_points()
        .iter()
        .map(|point| {
            RideMapPoint::new_with_start_reason(
                point.sample(),
                cutout_ride_maps::RideMapSegmentId::new(point.segment_id()),
                point.telemetry_state(),
                point.start_reason(),
            )
        })
        .collect();
    let last_restored_monotonic = points
        .last()
        .map(|point| point.sample().monotonic_milliseconds().as_u64());
    let created_at_ms = ride
        .monotonic_created_at_milliseconds()
        .or_else(|| {
            last_restored_monotonic.map(|last| last.saturating_sub(ride.duration_milliseconds()))
        })
        .unwrap_or(0);
    let metadata = RideMapMetadata {
        candidate_vehicle: ride.candidate_vehicle().and_then(VehicleIdentity::new),
        associated_vehicle: ride.associated_vehicle().and_then(VehicleIdentity::new),
        associated_at_milliseconds: ride
            .associated_at_milliseconds()
            .map(MonotonicMilliseconds::new),
        // A historical receipt proves persistence, not a current live connection.
        last_telemetry_at_milliseconds: None,
        background_gap_count: cutout_ride_maps::BackgroundGapCount::default(),
    };
    let created_at = MonotonicMilliseconds::new(created_at_ms);
    let timing = RideRecordingTiming::new(
        MonotonicMilliseconds::new(
            ride.monotonic_last_event_milliseconds()
                .or(last_restored_monotonic)
                .unwrap_or(created_at.as_u64()),
        ),
        ride.paused_at_milliseconds()
            .map(MonotonicMilliseconds::new),
        cutout_ride_maps::RideDurationMilliseconds::new(ride.paused_duration_milliseconds()),
        cutout_ride_maps::RideDurationMilliseconds::new(ride.completed_duration_milliseconds()),
    );
    let restored_lifecycle = match ride.state() {
        RideLifecycleState::Interrupted
            if apply_automatic_recovery && ride.paused_at_milliseconds().is_some() =>
        {
            RideLifecycleState::Paused
        }
        state => state,
    };
    let recorder = RideMapRecorder::restored_with_metadata_and_summary_and_timing(
        restored_lifecycle,
        created_at,
        metadata,
        points,
        ride.summary(),
        timing,
    );
    let mut recorder = recorder;
    if let Some(motion) = ride.motion_observation()
        && let Some(at) = motion.last_stationary_at_milliseconds
    {
        recorder.mark_stationary_boundary(MonotonicMilliseconds::new(at));
    }
    if let Some(sample) = ride.stationary_location_observation() {
        recorder.restore_stationary_observation(sample);
    } else if let (Some(at), Some(wall_clock)) = (
        ride.last_location_observed_monotonic_milliseconds(),
        ride.last_location_observed_wall_clock_milliseconds(),
    ) {
        recorder.restore_location_observation(
            MonotonicMilliseconds::new(at),
            cutout_ride_maps::WallClockUnixMilliseconds::new(wall_clock),
        );
    }
    state.recorder = recorder.clone();
    state.admission_recorder = recorder;
    state.speed = RecordingSpeedState::default();
    state.speed_generation_started_ms = last_restored_monotonic.unwrap_or(created_at_ms);
    if let Some(motion) = ride.motion_observation() {
        state.speed.restore_motion(motion);
    }
    state.ride_id = Some(ride.id());
    state.checkpoint = Some(ride.moving_time_checkpoint());
    state.persisted_telemetry_receipt = ride
        .last_telemetry_at_milliseconds()
        .map(|at| (ride.id(), at));
    state.recoverable_updated_at_ms = Some(ride.updated_at_milliseconds());
    state.revision = state.revision.saturating_add(1);
    state.generation = state.generation.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::{
        PendingLiveRideCreation, PendingRideLifecycleTransition, RideMapRecorder,
        RideRecordingApplication, RideRecordingError,
    };
    use cutout_ride_maps::{
        Coordinate, LocationAdmission, LocationSample, LocationSource, MonotonicMilliseconds,
        RideEvent, RideLifecycleState, WallClockUnixMilliseconds,
    };
    use libcutout_persistence::{RideDatabase, StorageError};
    use std::{path::PathBuf, sync::Mutex};
    use uuid::Uuid;

    pub(super) static DATABASE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn database_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "cutout-application-ride-recording-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    #[test]
    fn durable_ride_commands_are_fenced_and_publish_only_after_commit() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).unwrap();
        let application = RideRecordingApplication::new(database.clone());

        let restored = application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        assert_eq!(restored.lifecycle, None);

        let started = application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(started.lifecycle, Some(RideLifecycleState::Active));
        let active_token = started.command_token.unwrap();

        let first_fix = LocationSample::new(
            Coordinate::from_degrees(39.7, -104.9).unwrap(),
            MonotonicMilliseconds::new(110),
            WallClockUnixMilliseconds::new(1_700_000_000_010),
            Some(10_000),
            LocationSource::Live,
        );
        let request_id = match application.admit_location(first_fix).unwrap() {
            super::RideLocationAdmission::Queued { request_id, .. } => request_id,
            other => panic!("first fix should queue a route point: {other:?}"),
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let location_outcome = loop {
            if let Some(outcome) = application.poll_location_writes().into_iter().next() {
                break outcome;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "location write timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(location_outcome.request_id, request_id);
        assert_eq!(
            location_outcome.result.unwrap().admission(),
            LocationAdmission::Accepted
        );
        assert!(location_outcome.projection_applied);
        assert_eq!(application.snapshot().route_point_count, 1);

        let repeated = LocationSample::new(
            first_fix.coordinate(),
            MonotonicMilliseconds::new(120),
            WallClockUnixMilliseconds::new(1_700_000_000_020),
            Some(10_000),
            LocationSource::Live,
        );
        assert_eq!(
            application.admit_location(repeated).unwrap(),
            super::RideLocationAdmission::ClockOnly
        );
        assert_eq!(application.snapshot().route_point_count, 1);
        let out_of_order = LocationSample::new(
            first_fix.coordinate(),
            MonotonicMilliseconds::new(119),
            WallClockUnixMilliseconds::new(1_700_000_000_019),
            Some(10_000),
            LocationSource::Live,
        );
        assert_eq!(
            application.admit_location(out_of_order).unwrap(),
            super::RideLocationAdmission::Rejected(LocationAdmission::OutOfOrder)
        );

        let pause = application
            .begin_transition(active_token, RideEvent::Pause, 200, None)
            .unwrap();
        assert!(matches!(
            application.begin_transition(active_token, RideEvent::Stop, 201, None),
            Err(RideRecordingError::CommandPending)
        ));
        let paused = application.finish_transition(pause).unwrap();
        assert_eq!(paused.lifecycle, Some(RideLifecycleState::Paused));
        assert!(paused.command_token.is_some());
        assert!(matches!(
            application.begin_transition(active_token, RideEvent::Resume, 300, None),
            Err(RideRecordingError::StaleCommand)
        ));

        let resumed = application
            .finish_transition(
                application
                    .begin_transition(paused.command_token.unwrap(), RideEvent::Resume, 100, None)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(resumed.lifecycle, Some(RideLifecycleState::Active));
        assert!(resumed.generation > paused.generation);
        assert_eq!(resumed.monotonic_epoch_offset_ms, 100);
        assert_eq!(resumed.last_monotonic_ms, 200);

        database.clone().shutdown().unwrap();
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn in_memory_owner_accepts_lifecycle_commands_without_a_database() {
        let application = RideRecordingApplication::in_memory();
        let started = application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(started.lifecycle, Some(RideLifecycleState::Active));
        assert!(started.checkpoint.is_none());
        let token = started.command_token.unwrap();
        let stopped = application
            .finish_transition(
                application
                    .begin_transition(token, RideEvent::Stop, 200, None)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(stopped.lifecycle, Some(RideLifecycleState::Stopped));
    }

    #[test]
    fn unknown_committed_results_require_restore_before_more_commands() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).unwrap();
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();

        let mut start = application
            .begin_start(100, 1_700_000_000_000, None, None)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let start_commit = loop {
            if let Some(result) = start
                .storage
                .as_mut()
                .and_then(PendingLiveRideCreation::try_committed_result)
            {
                break result.unwrap();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "start commit timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        let start_id = start_commit.ride_id();
        assert!(matches!(
            application.finish_start(start),
            Err(RideRecordingError::Storage(StorageError::ResponseDropped))
        ));
        assert!(matches!(
            application.begin_start(101, 1_700_000_000_001, None, None),
            Err(RideRecordingError::RecoveryRequired)
        ));

        let restored_start = application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        assert_eq!(restored_start.ride_id, Some(start_id));
        let active = restored_start.command_token.unwrap();

        let mut pause = application
            .begin_transition(active, RideEvent::Pause, 200, None)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let pause_commit = loop {
            if let Some(result) = pause
                .storage
                .as_mut()
                .and_then(PendingRideLifecycleTransition::try_committed_result)
            {
                break result.unwrap();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "pause commit timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(pause_commit.lifecycle(), RideLifecycleState::Paused);
        assert!(matches!(
            application.finish_transition(pause),
            Err(RideRecordingError::Storage(StorageError::ResponseDropped))
        ));
        assert!(matches!(
            application.begin_transition(active, RideEvent::Resume, 300, None),
            Err(RideRecordingError::RecoveryRequired)
        ));

        let restored_pause = application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        assert_eq!(restored_pause.lifecycle, Some(RideLifecycleState::Paused));
        assert!(
            application
                .begin_transition(
                    restored_pause.command_token.unwrap(),
                    RideEvent::Resume,
                    300,
                    None
                )
                .is_ok()
        );

        database.clone().shutdown().unwrap();
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn committed_start_projection_failure_fences_until_restore() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).unwrap();
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();

        let start = application
            .begin_start(100, 1_700_000_000_000, None, None)
            .unwrap();
        application
            .state
            .lock()
            .unwrap()
            .recorder
            .start(MonotonicMilliseconds::new(50), None)
            .unwrap();
        assert!(matches!(
            application.finish_start(start),
            Err(RideRecordingError::InvalidTransition)
        ));
        assert!(matches!(
            application.begin_start(200, 1_700_000_000_100, None, None),
            Err(RideRecordingError::RecoveryRequired)
        ));

        let restored = application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        assert_eq!(restored.lifecycle, Some(RideLifecycleState::Active));
        assert!(restored.ride_id.is_some());

        database.clone().shutdown().unwrap();
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn committed_transition_projection_failure_fences_until_restore() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).unwrap();
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        let started = application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .unwrap(),
            )
            .unwrap();
        let pause = application
            .begin_transition(started.command_token.unwrap(), RideEvent::Pause, 200, None)
            .unwrap();

        {
            let mut state = application.state.lock().unwrap();
            state.recorder = RideMapRecorder::new();
            state
                .recorder
                .start(MonotonicMilliseconds::new(100), None)
                .unwrap();
            let stop = state
                .recorder
                .state()
                .unwrap()
                .transition(RideEvent::Stop)
                .unwrap();
            state
                .recorder
                .apply_transition_at(stop, MonotonicMilliseconds::new(150))
                .unwrap();
        }
        assert!(matches!(
            application.finish_transition(pause),
            Err(RideRecordingError::InvalidTransition)
        ));
        assert!(matches!(
            application.begin_start(300, 1_700_000_000_200, None, None),
            Err(RideRecordingError::RecoveryRequired)
        ));

        let restored = application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        assert_eq!(restored.lifecycle, Some(RideLifecycleState::Paused));

        database.clone().shutdown().unwrap();
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn snapshot_at_projects_duration_and_telemetry_at_requested_time() {
        let application = RideRecordingApplication::in_memory();
        let started = application
            .finish_start(application.begin_start(100, 1_000, None, None).unwrap())
            .unwrap();

        assert_eq!(started.duration_milliseconds, 0);
        assert_eq!(application.snapshot_at(1_250).duration_milliseconds, 1_150);
    }

    #[test]
    fn speed_and_motion_evidence_is_generation_fenced_by_the_owner() {
        let application = RideRecordingApplication::in_memory();
        let started = application
            .finish_start(application.begin_start(100, 1_000, None, None).unwrap())
            .unwrap();
        let generation = started.generation;

        assert!(application.observe_phone_gps_speed(Some(4.0), 200, generation));
        assert_eq!(
            application.snapshot_at(250).speed,
            Some(libcutout_persistence::RecordingSpeed {
                millimetres_per_second: 4_000,
                source: libcutout_persistence::RecordingSpeedSource::PhoneGps,
            })
        );
        assert!(application.observe_verified_vehicle_speed(Some(0), 300, 300, generation));
        assert!(application.parked_at(300));
        assert!(!application.observe_phone_gps_speed(Some(9.0), 400, generation + 1));
    }

    #[test]
    fn telemetry_receipt_updates_the_application_owned_recorder() {
        let application = RideRecordingApplication::in_memory();
        application
            .finish_start(application.begin_start(100, 1_000, None, None).unwrap())
            .unwrap();
        let admission = application
            .begin_verified_connection_admission(
                "vehicle-a",
                100,
                1,
                cutout_music::MusicHistoryPolicy::Disabled,
            )
            .unwrap();
        application
            .finish_verified_connection_admission(admission)
            .unwrap();
        let prior_revision = application.snapshot().revision;

        let receipt = application.observe_telemetry(200, None, None).unwrap();
        assert_eq!(
            receipt.observation,
            cutout_ride_maps::TelemetryObservation::Observed
        );
        assert_eq!(application.snapshot().revision, prior_revision + 1);
    }

    #[test]
    fn pending_duplicate_is_rejected_while_clock_only_repeat_is_admitted() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).unwrap();
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        application
            .finish_start(application.begin_start(100, 1_000, None, None).unwrap())
            .unwrap();

        let blocker = rusqlite::Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let sample = LocationSample::new(
            Coordinate::from_degrees(39.7, -104.9).unwrap(),
            MonotonicMilliseconds::new(110),
            WallClockUnixMilliseconds::new(1_010),
            Some(10_000),
            LocationSource::Live,
        );
        let request_id = match application.admit_location(sample).unwrap() {
            super::RideLocationAdmission::Queued { request_id, .. } => request_id,
            other => panic!("first fix should queue while the database is locked: {other:?}"),
        };
        let duplicate = LocationSample::new(
            sample.coordinate(),
            MonotonicMilliseconds::new(120),
            WallClockUnixMilliseconds::new(1_020),
            Some(10_000),
            LocationSource::Live,
        );
        assert_eq!(
            application.admit_location(sample).unwrap(),
            super::RideLocationAdmission::Rejected(LocationAdmission::Duplicate)
        );

        blocker.execute_batch("ROLLBACK;").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let outcome = loop {
            if let Some(outcome) = application.poll_location_writes().into_iter().next() {
                break outcome;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "location write timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(outcome.request_id, request_id);
        assert_eq!(
            outcome.result.unwrap().admission(),
            LocationAdmission::Accepted
        );
        assert_eq!(
            application.admit_location(duplicate).unwrap(),
            super::RideLocationAdmission::ClockOnly
        );
        assert_eq!(application.snapshot().route_point_count, 1);
        database.clone().shutdown().unwrap();
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_telemetry_persistence_does_not_publish_staged_speed() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).unwrap();
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().unwrap())
            .unwrap();
        application
            .finish_start(application.begin_start(100, 1_000, None, None).unwrap())
            .unwrap();

        let admission = application
            .begin_verified_connection_admission(
                "vehicle-a",
                100,
                1,
                cutout_music::MusicHistoryPolicy::Disabled,
            )
            .unwrap();
        application
            .finish_verified_connection_admission(admission)
            .unwrap();

        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_telemetry BEFORE UPDATE OF last_telemetry_at_ms ON rides
                 BEGIN SELECT RAISE(ABORT, 'injected telemetry failure'); END;",
            )
            .unwrap();
        let prior_revision = application.snapshot().revision;
        let result = application.observe_telemetry(
            200,
            Some(super::RideVehicleSpeedSample {
                millimetres_per_second: 4_000,
                source_at_ms: 150,
            }),
            None,
        );
        assert!(result.is_err());
        assert_eq!(application.snapshot().revision, prior_revision);
        assert_eq!(application.snapshot_at(200).speed, None);

        connection
            .execute_batch("DROP TRIGGER reject_telemetry;")
            .unwrap();
        database.clone().shutdown().unwrap();
        drop(connection);
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stop_waits_behind_the_music_observation_fence() {
        let application = RideRecordingApplication::in_memory();
        let started = application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .unwrap(),
            )
            .unwrap();
        let queue =
            std::sync::Arc::new(crate::music::observation_admission::ObservationQueue::default());
        let fence = queue.begin_stop().unwrap();
        let stopped = application
            .finish_stop_after_music(
                application
                    .begin_stop_after_music(started.command_token.unwrap(), 200, None, fence)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(stopped.lifecycle, Some(RideLifecycleState::Stopped));
    }

    #[test]
    fn discard_returns_terminal_snapshot_then_clears_current_ride_owner() {
        let application = RideRecordingApplication::in_memory();
        let started = application
            .finish_start(
                application
                    .begin_start(100, 1_700_000_000_000, None, None)
                    .unwrap(),
            )
            .unwrap();
        let stopped = application
            .finish_transition(
                application
                    .begin_transition(started.command_token.unwrap(), RideEvent::Stop, 200, None)
                    .unwrap(),
            )
            .unwrap();
        let discarded = application
            .finish_transition(
                application
                    .begin_transition(
                        stopped.command_token.unwrap(),
                        RideEvent::Discard,
                        201,
                        None,
                    )
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(discarded.lifecycle, Some(RideLifecycleState::Discarded));
        assert_eq!(discarded.ride_id, Some(started.ride_id.unwrap()));
        assert_eq!(application.snapshot().ride_id, None);
        assert_eq!(application.snapshot().music_history_policy, None);
    }
}
