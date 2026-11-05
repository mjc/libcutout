//! Verified-connection lifecycle planning and durable admission.

use super::{
    CommandLease, RecordingState, RideRecordingApplication, RideRecordingError,
    RideRecordingSnapshot, ensure_command, require_ready, snapshot_from,
};
use cutout_music::MusicHistoryPolicy;
use cutout_ride_maps::{
    MonotonicMilliseconds, RideEvent, RideLifecycleState, RideMapRecorder, RideRecoveryDecision,
    RideRecoveryKind, VehicleAssociation, VehicleIdentity, WallClockUnixMilliseconds,
    ride_recovery_decision,
};
use libcutout_persistence::{
    PendingVerifiedConnectionAdmission, VerifiedConnectionLifecycleMutation,
    VerifiedConnectionRideMetadata, VerifiedConnectionRideTarget,
};
use std::{
    sync::PoisonError,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

/// Automatic resume window for an interrupted ride.
pub const AUTO_RESUME_RIDE_WINDOW_MILLISECONDS: u64 = 3 * 60 * 60 * 1_000;

/// Automatic recording override available to integration tests through the `test-support` feature.
#[cfg(feature = "test-support")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedConnectionTestPolicy {
    /// Keep verified connections from starting or resuming a ride.
    ManualOnly,
    /// Allow the verified-connection planner to start or resume a ride.
    StartAndResume,
}

/// Failure preparing or settling a verified-connection admission.
#[derive(Debug, Error)]
pub enum VerifiedConnectionAdmissionError {
    /// The platform identifier is not a valid ride-map vehicle identity.
    #[error("invalid vehicle identity")]
    InvalidVehicleIdentity,
    /// The wall clock could not be represented as a Unix millisecond timestamp.
    #[error("wall clock is unavailable: {0}")]
    WallClock(String),
    /// The shared ride lifecycle owner rejected the operation.
    #[error(transparent)]
    Ride(#[from] RideRecordingError),
    /// The persistence worker rejected or failed the admission transaction.
    #[error(transparent)]
    Storage(#[from] libcutout_persistence::StorageError),
}

#[derive(Clone, Debug)]
struct AdmissionPreview {
    recorder: RideMapRecorder,
    admission_recorder: RideMapRecorder,
    ride_id: Option<libcutout_persistence::RideId>,
    monotonic_epoch_offset_ms: u64,
    recoverable_updated_at_ms: Option<u64>,
    created_ride: bool,
    metadata: Option<VerifiedConnectionRideMetadata>,
}

impl AdmissionPreview {
    fn from_state(state: &RecordingState) -> Self {
        Self {
            recorder: state.recorder.clone(),
            admission_recorder: state.admission_recorder.clone(),
            ride_id: state.ride_id,
            monotonic_epoch_offset_ms: state.monotonic_epoch_offset_ms,
            recoverable_updated_at_ms: state.recoverable_updated_at_ms,
            created_ride: false,
            metadata: None,
        }
    }

    fn transition(
        &mut self,
        event: RideEvent,
        at_ms: u64,
        occurred_at_ms: u64,
        mutations: &mut Vec<VerifiedConnectionLifecycleMutation>,
    ) -> Result<(), RideRecordingError> {
        let ride_id = self.ride_id.ok_or(RideRecordingError::NoActiveRide)?;
        let current = self
            .recorder
            .state()
            .ok_or(RideRecordingError::NoActiveRide)?;
        let transition = current
            .transition(event)
            .map_err(|_| RideRecordingError::InvalidTransition)?;
        let is_recoverable = match current {
            RideLifecycleState::Interrupted | RideLifecycleState::Paused => true,
            _ => false,
        };
        let epoch_offset = if event == RideEvent::Resume && is_recoverable {
            self.recorder
                .recording_timing()
                .last_monotonic_milliseconds()
                .as_u64()
                .saturating_sub(at_ms)
        } else {
            self.monotonic_epoch_offset_ms
        };
        let logical_at_ms = at_ms.saturating_add(epoch_offset);
        let logical_at = MonotonicMilliseconds::new(logical_at_ms);
        self.recorder
            .apply_transition_at(transition, logical_at)
            .map_err(|_| RideRecordingError::InvalidTransition)?;
        self.admission_recorder
            .apply_transition_at(transition, logical_at)
            .map_err(|_| RideRecordingError::InvalidTransition)?;
        self.monotonic_epoch_offset_ms = epoch_offset;
        if event == RideEvent::Resume {
            self.recoverable_updated_at_ms = None;
        }
        mutations.push(VerifiedConnectionLifecycleMutation::Transition {
            ride_id,
            event,
            occurred_at_ms,
            monotonic_at_ms: Some(logical_at_ms),
        });
        Ok(())
    }

    fn start_live(
        &mut self,
        at_ms: u64,
        created_at_ms: u64,
        candidate_vehicle: &str,
        music_history_policy: MusicHistoryPolicy,
        mutations: &mut Vec<VerifiedConnectionLifecycleMutation>,
    ) -> Result<(), RideRecordingError> {
        let candidate_identity = VehicleIdentity::new(candidate_vehicle);
        self.recorder
            .start(MonotonicMilliseconds::new(at_ms), candidate_identity)
            .map_err(|_| RideRecordingError::InvalidTransition)?;
        self.admission_recorder = self.recorder.clone();
        self.ride_id = None;
        self.created_ride = true;
        self.monotonic_epoch_offset_ms = 0;
        self.recoverable_updated_at_ms = None;
        mutations.push(VerifiedConnectionLifecycleMutation::StartLive {
            created_at_ms,
            monotonic_created_at_ms: at_ms,
            candidate_vehicle: Some(candidate_vehicle.to_owned()),
            music_history_policy,
        });
        Ok(())
    }

    fn associate_vehicle(&mut self, identity: &VehicleIdentity, at_ms: u64) {
        if !self
            .recorder
            .state()
            .is_some_and(RideLifecycleState::is_recording)
        {
            return;
        }
        let at = MonotonicMilliseconds::new(at_ms);
        let association = self.admission_recorder.observe_vehicle(identity, at);
        if association == VehicleAssociation::Associated {
            let _ = self.recorder.observe_vehicle(identity, at);
        }
        let target = if self.created_ride {
            Some(VerifiedConnectionRideTarget::Created)
        } else {
            self.ride_id.map(VerifiedConnectionRideTarget::Existing)
        };
        if let Some(target) = target {
            self.metadata = Some(VerifiedConnectionRideMetadata {
                target,
                candidate_vehicle: self
                    .admission_recorder
                    .candidate_vehicle()
                    .map(str::to_owned),
                associated_vehicle: self
                    .admission_recorder
                    .associated_vehicle()
                    .map(str::to_owned),
                associated_at_ms: self
                    .admission_recorder
                    .associated_at_milliseconds()
                    .map(MonotonicMilliseconds::as_u64),
                last_telemetry_at_ms: self
                    .admission_recorder
                    .last_telemetry_at_milliseconds()
                    .map(MonotonicMilliseconds::as_u64),
                moving_time_source_boundary_at_ms: match association {
                    VehicleAssociation::Associated | VehicleAssociation::AlreadyAssociated => {
                        Some(at_ms)
                    }
                    _ => None,
                },
            });
        }
    }
}

/// Prepared verified connection transaction, reconciled by `finish_verified_connection_admission`.
#[must_use = "finish the verified-connection admission or drop it to release its fence"]
#[derive(Debug)]
pub struct PreparedVerifiedConnectionAdmission {
    lease: Option<CommandLease>,
    storage: Option<PendingVerifiedConnectionAdmission>,
    immediate_snapshot: Option<RideRecordingSnapshot>,
    immediate_noop: bool,
    selected_mutations: Vec<VerifiedConnectionLifecycleMutation>,
    test_policy_override: bool,
    selected_preview: AdmissionPreview,
    platform_identifier: String,
    connection_generation: u64,
    logical_at_ms: u64,
}

impl PreparedVerifiedConnectionAdmission {
    fn settled_repeat(
        state: &RecordingState,
        identity: &VehicleIdentity,
        connection_generation: u64,
        logical_at_ms: u64,
    ) -> Self {
        let snapshot = snapshot_from(state);
        Self {
            lease: None,
            storage: None,
            immediate_snapshot: snapshot.lifecycle.map(|_| snapshot),
            immediate_noop: true,
            selected_mutations: Vec::new(),
            test_policy_override: false,
            selected_preview: AdmissionPreview::from_state(state),
            platform_identifier: identity.as_str().to_owned(),
            connection_generation,
            logical_at_ms,
        }
    }

    /// Returns whether this admission is an already-settled idempotent repeat.
    #[must_use]
    pub const fn is_immediate_noop(&self) -> bool {
        self.immediate_noop
    }
}

impl RideRecordingApplication {
    /// Plans and queues one verified connection against the shared recording owner.
    ///
    /// The supplied monotonic timestamp belongs to the current process session. Wall-clock time
    /// is read only when a database is attached, matching durable creation and recovery metadata.
    ///
    /// # Errors
    /// Returns an error for invalid identity, unfinished restoration or another pending command,
    /// an unavailable clock, invalid lifecycle policy, or persistence queue failure.
    pub fn begin_verified_connection_admission(
        &self,
        platform_identifier: &str,
        at_ms: u64,
        connection_generation: u64,
        music_history_policy: MusicHistoryPolicy,
    ) -> Result<PreparedVerifiedConnectionAdmission, VerifiedConnectionAdmissionError> {
        self.begin_verified_connection_admission_with_override(
            platform_identifier,
            at_ms,
            connection_generation,
            music_history_policy,
            None,
        )
    }

    /// Plans an admission with an explicit automatic-recording policy for integration tests.
    ///
    /// This method is compiled only when the `test-support` feature is enabled. It executes the
    /// production planner and durable transaction path while overriding only the policy decision.
    ///
    /// # Errors
    /// Returns the same failures as [`Self::begin_verified_connection_admission`].
    #[cfg(feature = "test-support")]
    pub fn begin_verified_connection_admission_with_test_policy(
        &self,
        platform_identifier: &str,
        at_ms: u64,
        connection_generation: u64,
        music_history_policy: MusicHistoryPolicy,
        policy: VerifiedConnectionTestPolicy,
    ) -> Result<PreparedVerifiedConnectionAdmission, VerifiedConnectionAdmissionError> {
        self.begin_verified_connection_admission_with_override(
            platform_identifier,
            at_ms,
            connection_generation,
            music_history_policy,
            Some(policy == VerifiedConnectionTestPolicy::StartAndResume),
        )
    }

    fn begin_verified_connection_admission_with_override(
        &self,
        platform_identifier: &str,
        at_ms: u64,
        connection_generation: u64,
        music_history_policy: MusicHistoryPolicy,
        automatic_recording_override: Option<bool>,
    ) -> Result<PreparedVerifiedConnectionAdmission, VerifiedConnectionAdmissionError> {
        let identity = VehicleIdentity::new(platform_identifier)
            .ok_or(VerifiedConnectionAdmissionError::InvalidVehicleIdentity)?;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        require_ready(&state)?;

        if state.last_connection_transition_generation == Some(connection_generation)
            && state.last_connection_transition_ride_id == state.ride_id
        {
            return Ok(PreparedVerifiedConnectionAdmission::settled_repeat(
                &state,
                &identity,
                connection_generation,
                at_ms.saturating_add(state.monotonic_epoch_offset_ms),
            ));
        }
        if state.pending_command_id.is_some() {
            return Err(RideRecordingError::CommandPending.into());
        }

        let next_command_id = state.next_command_id;
        let following_command_id = next_command_id
            .checked_add(1)
            .ok_or(RideRecordingError::CommandIdExhausted)?;
        let logical_at_ms = at_ms.saturating_add(state.monotonic_epoch_offset_ms);
        let connection_generation_consumed =
            state.last_connection_admission_generation == Some(connection_generation);
        let mut selected_preview = AdmissionPreview::from_state(&state);
        let mut unselected_preview = AdmissionPreview::from_state(&state);
        let mut selected_mutations = Vec::new();
        let updated_at_ms = if self.database.is_some() {
            wall_clock_milliseconds()?
        } else {
            0
        };

        let should_auto_record = match automatic_recording_override {
            Some(override_policy) => !connection_generation_consumed && override_policy,
            None => should_start_recording(
                self.database.as_ref(),
                connection_generation_consumed,
                identity.as_str(),
            )?,
        };
        if should_auto_record {
            plan_auto_recording(
                &mut selected_preview,
                identity.as_str(),
                at_ms,
                updated_at_ms,
                music_history_policy,
                &mut selected_mutations,
            )?;
        }

        if selected_preview
            .recorder
            .state()
            .is_some_and(RideLifecycleState::is_recording)
        {
            selected_preview.associate_vehicle(&identity, logical_at_ms);
        }
        if unselected_preview
            .recorder
            .state()
            .is_some_and(RideLifecycleState::is_recording)
        {
            unselected_preview.associate_vehicle(&identity, logical_at_ms);
        }

        let storage = self
            .database
            .as_ref()
            .map(|database| {
                database.queue_verified_connection_admission(
                    identity.as_str(),
                    updated_at_ms,
                    selected_mutations.clone(),
                    selected_preview.metadata.clone(),
                    unselected_preview.metadata.clone(),
                )
            })
            .transpose()?;

        state.next_command_id = following_command_id;
        state.pending_command_id = Some(next_command_id);
        Ok(PreparedVerifiedConnectionAdmission {
            lease: Some(CommandLease {
                state: std::sync::Arc::downgrade(&self.state),
                id: next_command_id,
            }),
            storage,
            immediate_snapshot: None,
            immediate_noop: false,
            selected_mutations,
            test_policy_override: automatic_recording_override.is_some(),
            selected_preview,
            platform_identifier: identity.as_str().to_owned(),
            connection_generation,
            logical_at_ms,
        })
    }

    /// Waits for and applies the durable outcome of a verified connection admission.
    ///
    /// # Errors
    /// Returns an error if persistence fails, the admission fence is stale, or its committed ride
    /// checkpoint contradicts the planned lifecycle transition.
    pub fn finish_verified_connection_admission(
        &self,
        mut operation: PreparedVerifiedConnectionAdmission,
    ) -> Result<Option<RideRecordingSnapshot>, VerifiedConnectionAdmissionError> {
        if operation.immediate_noop {
            return Ok(operation.immediate_snapshot.take());
        }
        let command_id = operation.lease.as_ref().map(|lease| lease.id);
        let outcome = match operation
            .storage
            .take()
            .map(PendingVerifiedConnectionAdmission::wait_result)
            .transpose()
        {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Some(command_id) = command_id {
                    return Err(self.command_storage_error(command_id, error).into());
                }
                return Err(error.into());
            }
        };

        // Settle older admitted route points before applying lifecycle transitions, as the
        // previous recording owner did while holding its admission lock.
        let _ = self.poll_location_writes();
        let lease = operation
            .lease
            .as_ref()
            .ok_or(RideRecordingError::StaleCommand)?;
        let (created_ride_id, committed_checkpoint, selected_device_matches) =
            match reconcile_admission_receipt(self.database.as_ref(), &operation, outcome.as_ref())
            {
                Ok(receipt) => receipt,
                Err(error) => {
                    let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
                    if ensure_command(&state, lease.id).is_ok() {
                        state.pending_command_id = None;
                        state.restore_phase = super::RestorePhase::RecoveryRequired;
                    }
                    return Err(error);
                }
            };
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        ensure_command(&state, lease.id)?;
        apply_verified_admission_receipt(
            &mut state,
            &operation,
            selected_device_matches,
            created_ride_id,
            committed_checkpoint,
            self.database.is_none(),
        )?;
        state.last_connection_admission_generation = Some(operation.connection_generation);

        let Some(lifecycle) = state.recorder.state() else {
            state.pending_command_id = None;
            drop(state);
            operation.lease.take();
            return Ok(None);
        };
        if !lifecycle.is_recording() {
            let snapshot = snapshot_from(&state);
            state.pending_command_id = None;
            drop(state);
            operation.lease.take();
            return Ok(Some(snapshot));
        }

        let identity = VehicleIdentity::new(&operation.platform_identifier)
            .ok_or(VerifiedConnectionAdmissionError::InvalidVehicleIdentity)?;
        let mut staged = state.admission_recorder.clone();
        let mut durable_staged = state.recorder.clone();
        let association = staged.observe_vehicle(
            &identity,
            MonotonicMilliseconds::new(operation.logical_at_ms),
        );
        if association == VehicleAssociation::Associated {
            let _ = durable_staged.observe_vehicle(
                &identity,
                MonotonicMilliseconds::new(operation.logical_at_ms),
            );
            state.revision = state.revision.saturating_add(1);
        }
        state.recorder = durable_staged;
        state.admission_recorder = staged;
        let confirmed = match association {
            VehicleAssociation::Associated | VehicleAssociation::AlreadyAssociated => true,
            _ => false,
        };
        if confirmed {
            state.last_connection_transition_generation = Some(operation.connection_generation);
            state.last_connection_transition_ride_id = state.ride_id;
        }
        state.pending_command_id = None;
        let snapshot = snapshot_from(&state);
        drop(state);
        operation.lease.take();
        Ok(Some(snapshot))
    }
}

type AdmissionReceipt = (
    Option<libcutout_persistence::RideId>,
    Option<libcutout_persistence::CommittedRideMovingTimeCheckpoint>,
    bool,
);

fn reconcile_admission_receipt(
    database: Option<&libcutout_persistence::RideDatabase>,
    operation: &PreparedVerifiedConnectionAdmission,
    outcome: Option<&libcutout_persistence::VerifiedConnectionAdmissionOutcome>,
) -> Result<AdmissionReceipt, VerifiedConnectionAdmissionError> {
    let mut created_ride_id = outcome.and_then(|outcome| outcome.created_ride_id);
    let mut committed_checkpoint = outcome.and_then(|outcome| outcome.committed_ride_checkpoint);
    let mut selected_device_matches =
        outcome.is_some_and(|outcome| outcome.selected_device_matches);
    if operation.test_policy_override
        && !selected_device_matches
        && !operation.selected_mutations.is_empty()
        && let Some(database) = database
    {
        (created_ride_id, committed_checkpoint) = persist_test_override_mutations(
            database,
            operation,
            created_ride_id,
            committed_checkpoint,
        )?;
        selected_device_matches = true;
    }
    Ok((
        created_ride_id,
        committed_checkpoint,
        selected_device_matches,
    ))
}

fn persist_test_override_mutations(
    database: &libcutout_persistence::RideDatabase,
    operation: &PreparedVerifiedConnectionAdmission,
    mut created_ride_id: Option<libcutout_persistence::RideId>,
    mut checkpoint: Option<libcutout_persistence::CommittedRideMovingTimeCheckpoint>,
) -> Result<
    (
        Option<libcutout_persistence::RideId>,
        Option<libcutout_persistence::CommittedRideMovingTimeCheckpoint>,
    ),
    VerifiedConnectionAdmissionError,
> {
    for mutation in &operation.selected_mutations {
        match mutation {
            VerifiedConnectionLifecycleMutation::Transition {
                ride_id,
                event,
                monotonic_at_ms,
                ..
            } => {
                checkpoint = Some(
                    database
                        .queue_transition_at(
                            *ride_id,
                            *event,
                            monotonic_at_ms.unwrap_or(operation.logical_at_ms),
                        )?
                        .wait_committed_result()?,
                );
            }
            VerifiedConnectionLifecycleMutation::StartLive {
                created_at_ms,
                monotonic_created_at_ms,
                candidate_vehicle,
                music_history_policy,
            } => {
                let committed = database
                    .queue_create_started_live_ride_with_music_policy(
                        *created_at_ms,
                        *monotonic_created_at_ms,
                        candidate_vehicle.as_deref(),
                        Some(*music_history_policy),
                    )?
                    .wait_committed_result()?;
                created_ride_id = Some(committed.ride_id());
                checkpoint = Some(committed);
            }
        }
    }

    if let Some(metadata) = operation.selected_preview.metadata.as_ref() {
        let ride_id = match metadata.target {
            VerifiedConnectionRideTarget::Existing(ride_id) => ride_id,
            VerifiedConnectionRideTarget::Created => {
                created_ride_id.ok_or(RideRecordingError::InvalidTransition)?
            }
        };
        let metadata = VerifiedConnectionRideMetadata {
            target: VerifiedConnectionRideTarget::Existing(ride_id),
            candidate_vehicle: metadata.candidate_vehicle.clone(),
            associated_vehicle: metadata.associated_vehicle.clone(),
            associated_at_ms: metadata.associated_at_ms,
            last_telemetry_at_ms: metadata.last_telemetry_at_ms,
            moving_time_source_boundary_at_ms: metadata.moving_time_source_boundary_at_ms,
        };
        let wall_clock_ms = wall_clock_milliseconds()?;
        let metadata_outcome = database
            .queue_verified_connection_admission(
                &operation.platform_identifier,
                wall_clock_ms,
                Vec::new(),
                Some(metadata.clone()),
                Some(metadata),
            )?
            .wait_result()?;
        if let Some(committed) = metadata_outcome.committed_ride_checkpoint {
            checkpoint = Some(committed);
        }
    }
    Ok((created_ride_id, checkpoint))
}

fn apply_verified_admission_receipt(
    state: &mut RecordingState,
    operation: &PreparedVerifiedConnectionAdmission,
    selected_device_matches: bool,
    created_ride_id: Option<libcutout_persistence::RideId>,
    committed_checkpoint: Option<libcutout_persistence::CommittedRideMovingTimeCheckpoint>,
    in_memory_owner: bool,
) -> Result<(), RideRecordingError> {
    let selected_device_matches = selected_device_matches || in_memory_owner;
    state.last_connected_vehicle = VehicleIdentity::new(&operation.platform_identifier);

    if selected_device_matches
        && let Err(error) = apply_selected_mutations(
            state,
            operation,
            created_ride_id,
            committed_checkpoint,
            in_memory_owner,
        )
    {
        if !in_memory_owner {
            state.pending_command_id = None;
            state.restore_phase = super::RestorePhase::RecoveryRequired;
        }
        return Err(error);
    }

    if selected_device_matches
        && let Some(checkpoint) = committed_checkpoint
        && state.ride_id == Some(checkpoint.ride_id())
    {
        if state.recorder.state() != Some(checkpoint.lifecycle()) {
            state.pending_command_id = None;
            state.restore_phase = super::RestorePhase::RecoveryRequired;
            return Err(RideRecordingError::InvalidTransition);
        }
        if state
            .checkpoint
            .is_none_or(|current| current.revision() < checkpoint.checkpoint().revision())
        {
            state.checkpoint = Some(checkpoint.checkpoint());
        }
    }
    Ok(())
}

fn should_start_recording(
    database: Option<&libcutout_persistence::RideDatabase>,
    connection_generation_consumed: bool,
    identity: &str,
) -> Result<bool, VerifiedConnectionAdmissionError> {
    let Some(database) = database else {
        return Ok(false);
    };
    Ok(!connection_generation_consumed
        && database.ride_autostart_enabled()?
        && database.selected_device()?.as_deref() == Some(identity))
}

fn apply_selected_mutations(
    state: &mut RecordingState,
    operation: &PreparedVerifiedConnectionAdmission,
    created_ride_id: Option<libcutout_persistence::RideId>,
    committed_checkpoint: Option<libcutout_persistence::CommittedRideMovingTimeCheckpoint>,
    in_memory_owner: bool,
) -> Result<(), RideRecordingError> {
    for mutation in &operation.selected_mutations {
        match mutation {
            VerifiedConnectionLifecycleMutation::Transition {
                event,
                monotonic_at_ms,
                ..
            } => {
                apply_transition_mutation(
                    state,
                    *event,
                    *monotonic_at_ms,
                    operation.logical_at_ms,
                )?;
            }
            VerifiedConnectionLifecycleMutation::StartLive {
                monotonic_created_at_ms,
                candidate_vehicle,
                music_history_policy,
                ..
            } => apply_start_mutation(
                state,
                created_ride_id,
                committed_checkpoint,
                in_memory_owner,
                *monotonic_created_at_ms,
                candidate_vehicle.as_deref(),
                *music_history_policy,
            )?,
        }
    }
    state.monotonic_epoch_offset_ms = operation.selected_preview.monotonic_epoch_offset_ms;
    if !operation.selected_mutations.is_empty() {
        state.last_connection_transition_generation = Some(operation.connection_generation);
        state.last_connection_transition_ride_id = state.ride_id;
    }
    Ok(())
}

fn apply_transition_mutation(
    state: &mut RecordingState,
    event: RideEvent,
    monotonic_at_ms: Option<u64>,
    fallback_at_ms: u64,
) -> Result<(), RideRecordingError> {
    let transition = state
        .recorder
        .state()
        .ok_or(RideRecordingError::NoActiveRide)?
        .transition(event)
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    let at = MonotonicMilliseconds::new(monotonic_at_ms.unwrap_or(fallback_at_ms));
    state
        .recorder
        .apply_transition_at(transition, at)
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    state
        .admission_recorder
        .apply_transition_at(transition, at)
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    if event == RideEvent::Resume {
        state.recoverable_updated_at_ms = None;
    }
    state.revision = state.revision.saturating_add(1);
    state.generation = state.generation.saturating_add(1);
    Ok(())
}

fn apply_start_mutation(
    state: &mut RecordingState,
    created_ride_id: Option<libcutout_persistence::RideId>,
    committed_checkpoint: Option<libcutout_persistence::CommittedRideMovingTimeCheckpoint>,
    in_memory_owner: bool,
    monotonic_created_at_ms: u64,
    candidate_vehicle: Option<&str>,
    music_history_policy: MusicHistoryPolicy,
) -> Result<(), RideRecordingError> {
    let ride_id = match (created_ride_id, in_memory_owner) {
        (Some(ride_id), _) => ride_id,
        (None, true) => libcutout_persistence::RideId::new(),
        (None, false) => return Err(RideRecordingError::InvalidTransition),
    };
    let checkpoint = committed_checkpoint
        .filter(|checkpoint| checkpoint.ride_id() == ride_id)
        .map(libcutout_persistence::CommittedRideMovingTimeCheckpoint::checkpoint);
    if !in_memory_owner && checkpoint.is_none() {
        return Err(RideRecordingError::InvalidTransition);
    }
    state
        .recorder
        .start(
            MonotonicMilliseconds::new(monotonic_created_at_ms),
            candidate_vehicle.and_then(VehicleIdentity::new),
        )
        .map_err(|_| RideRecordingError::InvalidTransition)?;
    state.admission_recorder = state.recorder.clone();
    state.ride_id = Some(ride_id);
    state.checkpoint = checkpoint;
    state.speed = super::RecordingSpeedState::default();
    state.speed_generation_started_ms = monotonic_created_at_ms;
    state.latest_vehicle_moving_time = None;
    state.persisted_telemetry_receipt = None;
    state.recoverable_updated_at_ms = None;
    state.music_history_policy = Some(music_history_policy);
    state.revision = state.revision.saturating_add(1);
    state.generation = state.generation.saturating_add(1);
    Ok(())
}

fn plan_auto_recording(
    preview: &mut AdmissionPreview,
    identity: &str,
    at_ms: u64,
    occurred_at_ms: u64,
    music_history_policy: MusicHistoryPolicy,
    mutations: &mut Vec<VerifiedConnectionLifecycleMutation>,
) -> Result<(), RideRecordingError> {
    let current_vehicle = preview
        .recorder
        .associated_vehicle()
        .or_else(|| preview.recorder.candidate_vehicle());
    let recording_another_vehicle = preview
        .recorder
        .state()
        .is_some_and(RideLifecycleState::is_recording)
        && current_vehicle.is_some_and(|vehicle| vehicle != identity);
    if recording_another_vehicle {
        preview.transition(RideEvent::Stop, at_ms, occurred_at_ms, mutations)?;
        preview.transition(RideEvent::Save, at_ms, occurred_at_ms, mutations)?;
    }

    if preview.recorder.state() == Some(RideLifecycleState::Interrupted) {
        let matches_vehicle = preview.recorder.associated_vehicle() == Some(identity)
            || (preview.recorder.associated_vehicle().is_none()
                && preview.recorder.candidate_vehicle() == Some(identity));
        let recovery_decision = preview.recoverable_updated_at_ms.map(|updated_at| {
            ride_recovery_decision(
                RideRecoveryKind::InvoluntaryInterruption,
                WallClockUnixMilliseconds::new(occurred_at_ms),
                WallClockUnixMilliseconds::new(updated_at),
            )
        });
        if matches_vehicle && recovery_decision == Some(RideRecoveryDecision::Resume) {
            preview.transition(RideEvent::Resume, at_ms, occurred_at_ms, mutations)?;
        }
    }

    if preview
        .recorder
        .state()
        .is_none_or(RideLifecycleState::allows_auto_recording_replacement)
    {
        preview.start_live(
            at_ms,
            occurred_at_ms,
            identity,
            music_history_policy,
            mutations,
        )?;
    }
    Ok(())
}

fn wall_clock_milliseconds() -> Result<u64, VerifiedConnectionAdmissionError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| VerifiedConnectionAdmissionError::WallClock(error.to_string()))?
        .as_millis()
        .try_into()
        .map_err(|error: std::num::TryFromIntError| {
            VerifiedConnectionAdmissionError::WallClock(error.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride_recording::RideRecordingApplication;
    use cutout_ride_maps::{MonotonicMilliseconds, RideLifecycleState};
    use libcutout_persistence::RideDatabase;
    use std::{path::PathBuf, sync::Mutex};
    use uuid::Uuid;

    static DATABASE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn database_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "cutout-application-verified-connection-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    fn interrupted_preview(updated_at_ms: u64) -> AdmissionPreview {
        let identity = VehicleIdentity::new("wheel-a").expect("valid identity");
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(MonotonicMilliseconds::new(1_000), Some(identity))
            .expect("ride starts");
        let transition = RideLifecycleState::Active
            .transition(RideEvent::Interrupt)
            .expect("ride can be interrupted");
        recorder
            .apply_transition_at(transition, MonotonicMilliseconds::new(5_000))
            .expect("interruption is applied");
        AdmissionPreview {
            admission_recorder: recorder.clone(),
            recorder,
            ride_id: Some(libcutout_persistence::RideId::new()),
            monotonic_epoch_offset_ms: 0,
            recoverable_updated_at_ms: Some(updated_at_ms),
            created_ride: false,
            metadata: None,
        }
    }

    #[test]
    fn resume_window_and_monotonic_offset_are_applied_by_the_admission_plan() {
        let mut recent = interrupted_preview(100_000);
        let mut mutations = Vec::new();
        plan_auto_recording(
            &mut recent,
            "wheel-a",
            200,
            101_000,
            MusicHistoryPolicy::Disabled,
            &mut mutations,
        )
        .expect("recent matching ride resumes");
        assert_eq!(recent.recorder.state(), Some(RideLifecycleState::Active));
        assert_eq!(recent.monotonic_epoch_offset_ms, 4_800);
        assert_eq!(mutations.len(), 1);
        assert!(matches!(
            mutations[0],
            VerifiedConnectionLifecycleMutation::Transition {
                event: RideEvent::Resume,
                monotonic_at_ms: Some(5_000),
                ..
            }
        ));

        let mut expired = interrupted_preview(100_000);
        let mut mutations = Vec::new();
        plan_auto_recording(
            &mut expired,
            "wheel-a",
            200,
            100_000 + AUTO_RESUME_RIDE_WINDOW_MILLISECONDS + 1,
            MusicHistoryPolicy::Disabled,
            &mut mutations,
        )
        .expect("expired ride can be replaced");
        assert_eq!(expired.recorder.state(), Some(RideLifecycleState::Active));
        assert_eq!(expired.monotonic_epoch_offset_ms, 0);
        assert!(matches!(
            mutations.as_slice(),
            [VerifiedConnectionLifecycleMutation::StartLive { .. }]
        ));
    }

    #[test]
    fn selected_connection_commits_start_and_repeated_generation_is_idempotent() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).expect("database opens");
        database
            .save_selected_device("wheel-a", 1)
            .expect("device is selected");
        database
            .save_ride_autostart_enabled(true)
            .expect("autostart is enabled");
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().expect("restore queues"))
            .expect("restore completes");

        let started = application
            .finish_verified_connection_admission(
                application
                    .begin_verified_connection_admission(
                        "wheel-a",
                        100,
                        1,
                        MusicHistoryPolicy::Disabled,
                    )
                    .expect("verified connection queues"),
            )
            .expect("verified connection commits")
            .expect("new ride is published");
        assert_eq!(started.lifecycle, Some(RideLifecycleState::Active));
        let ride_id = started.ride_id.expect("ride identity is assigned");
        let repeated = application
            .finish_verified_connection_admission(
                application
                    .begin_verified_connection_admission(
                        "wheel-a",
                        101,
                        1,
                        MusicHistoryPolicy::Disabled,
                    )
                    .expect("repeat is an immediate no-op"),
            )
            .expect("repeat completes")
            .expect("current ride remains published");
        assert_eq!(repeated.ride_id, Some(ride_id));
        assert_eq!(repeated.revision, started.revision);

        database.clone().shutdown().expect("database shuts down");
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn explicit_policy_override_persists_a_ride_without_changing_device_selection() {
        let _guard = DATABASE_TEST_LOCK.lock().unwrap();
        let path = database_path();
        let database = RideDatabase::open(&path).expect("database opens");
        let application = RideRecordingApplication::new(database.clone());
        application
            .finish_restore(application.begin_restore().expect("restore queues"))
            .expect("restore completes");

        let snapshot = application
            .finish_verified_connection_admission(
                application
                    .begin_verified_connection_admission_with_test_policy(
                        "wheel-a",
                        100,
                        1,
                        MusicHistoryPolicy::Disabled,
                        VerifiedConnectionTestPolicy::StartAndResume,
                    )
                    .expect("override admission queues"),
            )
            .expect("override admission completes")
            .expect("ride is published");

        assert_eq!(snapshot.lifecycle, Some(RideLifecycleState::Active));
        assert_eq!(snapshot.associated_vehicle.as_deref(), Some("wheel-a"));
        assert_eq!(database.selected_device().expect("selection reads"), None);
        let rides = database
            .list_rides(None, libcutout_persistence::QueryLimit::new(10).unwrap())
            .expect("rides load");
        assert!(rides.rides().iter().any(|ride| {
            Some(ride.id()) == snapshot.ride_id
                && ride.candidate_vehicle().is_none()
                && ride.associated_vehicle() == Some("wheel-a")
                && ride.state() == RideLifecycleState::Active
        }));

        database.clone().shutdown().expect("database shuts down");
        drop(application);
        drop(database);
        let _ = std::fs::remove_file(path);
    }
}
