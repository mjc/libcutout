//! Durable live recording orchestration shared by platform adapters.
use crate::{
    PendingLocationWrite, QueryLimit, RideDatabase, RideId, RoutePointCursor, StorageError,
};
use cutout_core::{ConnectionAttemptToken, PevcapPhoneLocation};
use cutout_music::MusicHistoryPolicy;
use cutout_ride_maps as ride_maps;
use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

/// Failures applying an operation to the selected recording.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RecordingError {
    /// A live ride is already open.
    #[error("a ride is already recording")]
    AlreadyRecording,
    /// No live ride is open.
    #[error("no active ride")]
    NoActiveRide,
    /// The requested lifecycle event is not valid for the current state.
    #[error("invalid ride transition")]
    InvalidTransition,
    /// The recording changed after this command was requested.
    #[error("ride changed before command could be applied")]
    StaleCommand,
    /// The supplied location values are invalid.
    #[error("invalid location")]
    InvalidLocation,
    /// The route display budget, viewport, or privacy policy is invalid.
    #[error("invalid route projection")]
    InvalidRouteProjection,
    /// A live route projection was cancelled by its caller.
    #[error("live route projection cancelled")]
    Cancelled,
    /// The canonical database rejected the operation.
    #[error("ride map storage failure: {0}")]
    Storage(String),
    /// The provider observation did not satisfy the bounded music contract.
    #[error("invalid music input: {0}")]
    InvalidMusicInput(String),
}

impl RecordingError {
    fn storage_unavailable() -> Self {
        Self::Storage("Rust ride database is unavailable".to_owned())
    }
}
impl From<StorageError> for RecordingError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Transition(_) | StorageError::InvalidRideState(_) => {
                Self::InvalidTransition
            }
            other => Self::Storage(other.to_string()),
        }
    }
}

/// Reason a location was not appended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingDecisionReason {
    /// The ride is paused, stopped, or otherwise not recording.
    RideNotRecording,
    /// The sample repeats the latest accepted location.
    DuplicateLocation,
    /// The sample timestamp is not newer than the latest accepted sample.
    TimestampOutOfOrder,
    /// The reported horizontal accuracy exceeds the admission threshold.
    AccuracyTooLow,
    /// The sample implies an impossible travel speed.
    UnrealisticJump,
}

/// Admission and durable completion of one location sample.
#[derive(Clone, Debug, PartialEq)]
pub enum RecordingDecision {
    /// The location was accepted into the canonical route.
    Accepted {
        /// The resulting route point.
        point: RecordingPoint,
        /// Whether this point starts a new route segment.
        segment_started: bool,
    },
    /// The sample passed in-memory admission and is queued for durable persistence.
    Pending {
        /// The point already admitted to the Rust-owned in-memory route.
        point: RecordingPoint,
        /// Whether this point starts a new route segment.
        segment_started: bool,
    },
    /// The location was rejected as invalid input.
    Rejected {
        /// Stable admission reason.
        reason: RecordingDecisionReason,
    },
    /// The location was ignored because the ride is not recording.
    Ignored {
        /// Stable admission reason.
        reason: RecordingDecisionReason,
    },
    /// Durable persistence failed after in-memory admission.
    StorageError {
        /// Bounded diagnostic suitable for logging at the mobile boundary.
        message: String,
    },
}

/// A canonical live route point with durable sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecordingPoint {
    /// Stable zero-based point sequence within the ride.
    pub sequence: u64,
    /// Canonical location and segment/telemetry provenance.
    pub point: ride_maps::RideMapPoint,
}
/// Canonical recording totals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecordingSummary {
    /// Number of durably accepted points.
    pub point_count: u64,
    /// Canonical path distance.
    pub distance_millimetres: u64,
    /// Recording time excluding pauses.
    pub duration_milliseconds: u64,
}
/// Correlation identity captured when platform input is acquired.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordingToken {
    /// Canonical identity of the selected recording.
    pub ride_id: RideId,
    /// Input generation, changed on each lifecycle transition.
    pub generation: u64,
}
/// Immutable durable recording state evaluated at a caller-supplied time.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordingSnapshot {
    /// Canonical identity of the selected recording.
    pub ride_id: RideId,
    /// Monotonic revision across recording and lifecycle changes.
    pub revision: u64,
    /// Correlation token for commands in every lifecycle state.
    pub command_token: RecordingToken,
    /// Acquisition token present only while actively recording.
    pub recording_token: Option<RecordingToken>,
    /// Authoritative durable lifecycle.
    pub state: ride_maps::RideLifecycleState,
    /// Valid actions offered to the rider.
    pub allowed_actions: Vec<ride_maps::RideEvent>,
    /// Confirmed telemetry freshness at snapshot time.
    pub telemetry_state: ride_maps::RouteTelemetryState,
    /// Durable route totals evaluated at snapshot time.
    pub summary: RecordingSummary,
    /// Number of canonical route segments.
    pub segment_count: u64,
    /// Confirmed platform-local vehicle identifier.
    pub associated_vehicle: Option<String>,
    /// Whether terminal route endpoint annotations are meaningful.
    pub recorded_bounds_available: bool,
}
const MAX_PENDING_LOCATION_WRITES: usize = 64;
const AUTO_RESUME_RIDE_WINDOW_MILLISECONDS: u64 = 3 * 60 * 60 * 1_000;

/// Owns live recording policy and its durable database operations.
#[derive(Debug)]
pub struct RideRecordingSession {
    database: Option<RideDatabase>,
    ride_id: Option<RideId>,
    revision: u64,
    generation: u64,
    connection_attempt: Option<ConnectionAttemptToken>,
    recorder: ride_maps::RideMapRecorder,
    admission_recorder: ride_maps::RideMapRecorder,
    music_history_policy: MusicHistoryPolicy,
    pending_location_writes: VecDeque<PendingMapLocationWrite>,
    recoverable_updated_at_milliseconds: Option<u64>,
    monotonic_epoch_offset_milliseconds: u64,
    initialization_error: Option<RecordingError>,
}

#[derive(Debug)]
struct PendingMapLocationWrite {
    ride_id: RideId,
    sample: ride_maps::LocationSample,
    point: RecordingPoint,
    segment_started: bool,
    write: PendingLocationWrite,
}

impl RideRecordingSession {
    /// Associates confirmed vehicle evidence, preserving unfinished rider choices.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when identity is invalid or a required durable transition fails.
    pub fn ensure_recording_for_vehicle(
        &mut self,
        platform_identifier: &str,
        at_ms: u64,
    ) -> Result<RecordingSnapshot, RecordingError> {
        let identity = ride_maps::VehicleIdentity::new(platform_identifier)
            .ok_or(RecordingError::InvalidTransition)?;
        if matches!(
            self.recorder.state(),
            Some(ride_maps::RideLifecycleState::Active | ride_maps::RideLifecycleState::Paused)
        ) && self
            .recorder
            .associated_vehicle()
            .is_some_and(|associated| associated != identity.as_str())
        {
            self.transition_at(ride_maps::RideEvent::Stop, at_ms)?;
            self.transition_at(ride_maps::RideEvent::Save, at_ms)?;
        }
        if self.recorder.state() == Some(ride_maps::RideLifecycleState::Interrupted) {
            let wall_clock_milliseconds: u64 = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| RecordingError::Storage(error.to_string()))?
                .as_millis()
                .try_into()
                .map_err(|error: std::num::TryFromIntError| {
                    RecordingError::Storage(error.to_string())
                })?;
            let matches_vehicle = self.recorder.associated_vehicle() == Some(platform_identifier)
                || (self.recorder.associated_vehicle().is_none()
                    && self.recorder.candidate_vehicle() == Some(platform_identifier));
            let within_resume_window =
                self.recoverable_updated_at_milliseconds
                    .is_some_and(|updated_at| {
                        wall_clock_milliseconds
                            .checked_sub(updated_at)
                            .is_some_and(|age| age <= AUTO_RESUME_RIDE_WINDOW_MILLISECONDS)
                    });
            if matches_vehicle && within_resume_window {
                self.transition_at(ride_maps::RideEvent::Resume, at_ms)?;
                self.recoverable_updated_at_milliseconds = None;
            }
        }
        if self.recorder.state().is_none_or(|current| {
            matches!(
                current,
                ride_maps::RideLifecycleState::Interrupted
                    | ride_maps::RideLifecycleState::Saved
                    | ride_maps::RideLifecycleState::Discarded
            )
        }) {
            self.start_gps_only(at_ms, Some(platform_identifier.to_owned()))?;
        }

        let at_ms = self.logical_monotonic_milliseconds(at_ms);
        self.observe_vehicle(&identity, ride_maps::MonotonicMilliseconds::new(at_ms))?;
        let lifecycle = self.recorder.state().ok_or(RecordingError::NoActiveRide)?;
        self.snapshot(lifecycle).ok_or(RecordingError::NoActiveRide)
    }

    fn persist_vehicle_evidence(
        &self,
        candidate: Option<&str>,
        associated: Option<&str>,
        associated_at: Option<ride_maps::MonotonicMilliseconds>,
        telemetry_at: Option<ride_maps::MonotonicMilliseconds>,
    ) -> Result<(), RecordingError> {
        if let (Some(database), Some(id)) = (self.database.as_ref(), self.ride_id) {
            database
                .update_ride_map_metadata(
                    id,
                    candidate,
                    associated,
                    associated_at.map(ride_maps::MonotonicMilliseconds::as_u64),
                    telemetry_at.map(ride_maps::MonotonicMilliseconds::as_u64),
                )
                .map_err(RecordingError::from)?;
        }
        Ok(())
    }

    fn observe_vehicle(
        &mut self,
        identity: &ride_maps::VehicleIdentity,
        at: ride_maps::MonotonicMilliseconds,
    ) -> Result<ride_maps::VehicleAssociation, RecordingError> {
        let association = self.admission_recorder.vehicle_association(identity, at);
        if association == ride_maps::VehicleAssociation::Associated {
            self.persist_vehicle_evidence(
                None,
                Some(identity.as_str()),
                Some(at),
                self.admission_recorder.last_telemetry_at_milliseconds(),
            )?;
            let _ = self.recorder.observe_vehicle(identity, at);
            let _ = self.admission_recorder.observe_vehicle(identity, at);
            self.revision = self.revision.saturating_add(1);
        }
        Ok(association)
    }

    fn observe_telemetry(
        &mut self,
        identity: &ride_maps::VehicleIdentity,
        at: ride_maps::MonotonicMilliseconds,
    ) -> Result<ride_maps::TelemetryObservation, RecordingError> {
        let observation = self.admission_recorder.telemetry_observation(identity, at);
        if observation == ride_maps::TelemetryObservation::Observed {
            self.persist_vehicle_evidence(
                self.admission_recorder.candidate_vehicle(),
                self.admission_recorder.associated_vehicle(),
                self.admission_recorder.associated_at_milliseconds(),
                Some(at),
            )?;
            let _ = self.recorder.observe_telemetry(identity, at);
            let _ = self.admission_recorder.observe_telemetry(identity, at);
            self.revision = self.revision.saturating_add(1);
        }
        Ok(observation)
    }

    /// Consumes completed writes and rebuilds speculative admission from durable state.
    pub fn poll_location_writes(&mut self) -> Vec<RecordingDecision> {
        let current_ride_id = self.ride_id;
        let mut completed = Vec::new();
        let mut remaining = VecDeque::with_capacity(self.pending_location_writes.len());
        while let Some(mut pending) = self.pending_location_writes.pop_front() {
            let Some(result) = pending.write.try_result() else {
                remaining.push_back(pending);
                continue;
            };
            if current_ride_id.as_ref() != Some(&pending.ride_id) {
                continue;
            }
            completed.push(match result {
                Ok(result) if result.admission() == ride_maps::LocationAdmission::Accepted => {
                    self.recorder.record_sample(pending.sample);
                    self.revision = self.revision.saturating_add(1);
                    let mut point = pending.point;
                    if let Some(sequence) = result.sequence() {
                        point.sequence = sequence;
                    }
                    RecordingDecision::Accepted {
                        point,
                        segment_started: pending.segment_started,
                    }
                }
                Ok(result) if result.admission() == ride_maps::LocationAdmission::Duplicate => {
                    RecordingDecision::Ignored {
                        reason: RecordingDecisionReason::DuplicateLocation,
                    }
                }
                Ok(result) if result.admission() == ride_maps::LocationAdmission::OutOfOrder => {
                    RecordingDecision::Rejected {
                        reason: RecordingDecisionReason::TimestampOutOfOrder,
                    }
                }
                Ok(result)
                    if result.admission() == ride_maps::LocationAdmission::AccuracyTooLow =>
                {
                    RecordingDecision::Rejected {
                        reason: RecordingDecisionReason::AccuracyTooLow,
                    }
                }
                Ok(result)
                    if result.admission() == ride_maps::LocationAdmission::UnrealisticJump =>
                {
                    RecordingDecision::Rejected {
                        reason: RecordingDecisionReason::UnrealisticJump,
                    }
                }
                Ok(_) => RecordingDecision::StorageError {
                    message: "unknown location admission result".to_owned(),
                },
                Err(error) => RecordingDecision::StorageError {
                    message: error.to_string(),
                },
            });
        }
        self.pending_location_writes = remaining;
        // Rebuild the admission projection from the durable projection plus only writes that are
        // still pending. This removes a pending point after a durable rejection.
        let mut rebuilt = self.recorder.clone();
        for pending in &self.pending_location_writes {
            rebuilt.record_sample(pending.sample);
        }
        self.admission_recorder = rebuilt;
        completed
    }

    fn apply_music_history_policy(&mut self, policy: MusicHistoryPolicy) {
        self.music_history_policy = policy;
    }

    fn reset_music_history_policy(&mut self) {
        self.music_history_policy = MusicHistoryPolicy::Disabled;
    }

    fn transition_state(
        &mut self,
        event: ride_maps::RideEvent,
        at_milliseconds: u64,
    ) -> Result<(RideId, ride_maps::RideLifecycleState), RecordingError> {
        let Some(id) = self.ride_id else {
            return Err(RecordingError::NoActiveRide);
        };
        let Some(current) = self.recorder.state() else {
            return Err(RecordingError::NoActiveRide);
        };
        let next = current
            .apply(event)
            .map_err(|_| RecordingError::InvalidTransition)?;
        if let Some(database) = self.database.as_ref() {
            database
                .transition_at(id, event, at_milliseconds)
                .map_err(RecordingError::from)?;
        }
        Ok((id, next))
    }

    /// Admits a validated platform location through the canonical recording policy.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when there is no recording, the location is invalid, or durable submission fails.
    pub fn ingest_location(
        &mut self,
        monotonic_ms: u64,
        wall_clock_unix_ms: u64,
        latitude_degrees: f64,
        longitude_degrees: f64,
        horizontal_accuracy_meters: f64,
    ) -> Result<RecordingDecision, RecordingError> {
        let monotonic_ms = self.logical_monotonic_milliseconds(monotonic_ms);
        let Some(id) = self.ride_id else {
            return Err(RecordingError::NoActiveRide);
        };
        if self.admission_recorder.state() != Some(ride_maps::RideLifecycleState::Active) {
            return Ok(RecordingDecision::Ignored {
                reason: RecordingDecisionReason::RideNotRecording,
            });
        }
        if wall_clock_unix_ms == 0 {
            return Err(RecordingError::InvalidLocation);
        }
        let sample = ride_maps::LocationSample::new(
            ride_maps::Coordinate::from_degrees(latitude_degrees, longitude_degrees)
                .map_err(|_| RecordingError::InvalidLocation)?,
            ride_maps::MonotonicMilliseconds::new(monotonic_ms),
            ride_maps::WallClockUnixMilliseconds::new(wall_clock_unix_ms),
            Some(horizontal_accuracy_millimetres(horizontal_accuracy_meters)?),
            ride_maps::LocationSource::Live,
        );
        match self.admission_recorder.check_sample(&sample) {
            ride_maps::LocationAdmission::Duplicate => {
                return Ok(RecordingDecision::Ignored {
                    reason: RecordingDecisionReason::DuplicateLocation,
                });
            }
            ride_maps::LocationAdmission::OutOfOrder => {
                return Ok(RecordingDecision::Rejected {
                    reason: RecordingDecisionReason::TimestampOutOfOrder,
                });
            }
            ride_maps::LocationAdmission::AccuracyTooLow => {
                return Ok(RecordingDecision::Rejected {
                    reason: RecordingDecisionReason::AccuracyTooLow,
                });
            }
            ride_maps::LocationAdmission::UnrealisticJump => {
                return Ok(RecordingDecision::Rejected {
                    reason: RecordingDecisionReason::UnrealisticJump,
                });
            }
            ride_maps::LocationAdmission::Accepted => {}
        }
        let telemetry_state =
            self.admission_recorder
                .telemetry_state_at(ride_maps::MonotonicMilliseconds::new(
                    sample.monotonic_milliseconds().as_u64(),
                ));
        let sequence = self.admission_recorder.point_count();
        let (segment_id, segment_started, start_reason) =
            self.admission_recorder.next_sample_metadata(sample);
        let point =
            Self::point_from_location(sample, sequence, segment_id, start_reason, telemetry_state);
        if let Some(database) = self.database.as_ref() {
            if self.pending_location_writes.len() >= MAX_PENDING_LOCATION_WRITES {
                return Ok(RecordingDecision::StorageError {
                    message: "ride location write queue is full".to_owned(),
                });
            }
            let write =
                database.queue_location(id, sample, segment_id, start_reason, telemetry_state)?;
            self.admission_recorder.record_sample(sample);
            self.pending_location_writes
                .push_back(PendingMapLocationWrite {
                    ride_id: id,
                    sample,
                    point,
                    segment_started,
                    write,
                });
            return Ok(RecordingDecision::Pending {
                point,
                segment_started,
            });
        }
        self.admission_recorder.record_sample(sample);
        self.recorder = self.admission_recorder.clone();
        self.revision = self.revision.saturating_add(1);
        Ok(RecordingDecision::Accepted {
            point,
            segment_started,
        })
    }

    /// Loads the most recent recoverable ride using the shared database worker.
    #[must_use]
    pub fn new(database: Option<RideDatabase>) -> Self {
        let mut state = Self {
            database,
            ride_id: None,
            revision: 0,
            generation: 0,
            connection_attempt: None,
            recorder: ride_maps::RideMapRecorder::new(),
            admission_recorder: ride_maps::RideMapRecorder::new(),
            music_history_policy: MusicHistoryPolicy::Disabled,
            pending_location_writes: VecDeque::new(),
            recoverable_updated_at_milliseconds: None,
            monotonic_epoch_offset_milliseconds: 0,
            initialization_error: None,
        };
        if let Err(error) = state.restore_active_ride() {
            state.initialization_error = Some(error);
        }
        state
    }

    fn restored_route_samples(
        database: &RideDatabase,
        ride_id: RideId,
        point_count: u64,
    ) -> Result<Vec<ride_maps::RideMapPoint>, RecordingError> {
        let tail_start = point_count.saturating_sub(ride_maps::MAX_LIVE_ROUTE_POINTS as u64);
        let mut cursor = tail_start.checked_sub(1).map(RoutePointCursor::new);
        let mut samples = Vec::new();
        loop {
            let page = database.route_points(ride_id, cursor, QueryLimit::new(500)?)?;
            samples.extend(page.points().iter().map(|point| {
                ride_maps::RideMapPoint::new_with_start_reason(
                    point.sample(),
                    ride_maps::RideMapSegmentId::new(point.segment_id()),
                    point.telemetry_state(),
                    point.start_reason(),
                )
            }));
            cursor = page.next_cursor();
            if cursor.is_none() {
                return Ok(samples);
            }
        }
    }

    fn restore_active_ride(&mut self) -> Result<(), RecordingError> {
        let Some(database) = self.database.as_ref() else {
            return Ok(());
        };
        let Some(ride) = database
            .newest_recoverable_ride()
            .map_err(RecordingError::from)?
        else {
            return Ok(());
        };
        let background_gap_count = ride.background_gap_count();
        let samples = Self::restored_route_samples(
            database,
            ride.id(),
            ride.summary().point_count().as_u64(),
        )?;
        let last_restored_monotonic = samples
            .last()
            .map(|sample| sample.sample().monotonic_milliseconds().as_u64());
        let restored_start_milliseconds = ride
            .monotonic_created_at_milliseconds()
            .or_else(|| {
                last_restored_monotonic
                    .map(|last| last.saturating_sub(ride.duration_milliseconds()))
            })
            .unwrap_or(0);
        let timing_last = ride
            .monotonic_last_event_milliseconds()
            .or(last_restored_monotonic)
            .unwrap_or(restored_start_milliseconds);
        let timing = ride_maps::RideRecordingTiming::new(
            ride_maps::MonotonicMilliseconds::new(timing_last),
            ride.paused_at_milliseconds()
                .map(ride_maps::MonotonicMilliseconds::new),
            ride_maps::RideDurationMilliseconds::new(ride.paused_duration_milliseconds()),
            ride_maps::RideDurationMilliseconds::new(ride.completed_duration_milliseconds()),
        );
        self.recorder = ride_maps::RideMapRecorder::restored_with_metadata_and_summary_and_timing(
            ride.state(),
            ride_maps::MonotonicMilliseconds::new(restored_start_milliseconds),
            ride_maps::RideMapMetadata {
                candidate_vehicle: ride
                    .candidate_vehicle()
                    .and_then(ride_maps::VehicleIdentity::new),
                associated_vehicle: ride
                    .associated_vehicle()
                    .and_then(ride_maps::VehicleIdentity::new),
                associated_at_milliseconds: ride
                    .associated_at_milliseconds()
                    .map(ride_maps::MonotonicMilliseconds::new),
                last_telemetry_at_milliseconds: ride
                    .last_telemetry_at_milliseconds()
                    .map(ride_maps::MonotonicMilliseconds::new),
                background_gap_count: ride_maps::BackgroundGapCount::new(background_gap_count),
            },
            samples,
            ride.summary(),
            timing,
        );
        self.ride_id = Some(ride.id());
        self.recoverable_updated_at_milliseconds = Some(ride.updated_at_milliseconds());
        self.admission_recorder = self.recorder.clone();
        let Some(active_id) = self.ride_id.as_ref() else {
            return Err(RecordingError::NoActiveRide);
        };
        let ride_id = *active_id;
        if let Ok(policy) = database.music_history_policy(ride_id) {
            self.music_history_policy = policy;
        }
        Ok(())
    }

    fn point_from_location(
        sample: ride_maps::LocationSample,
        sequence: u64,
        segment_id: ride_maps::RideMapSegmentId,
        start_reason: ride_maps::RideSegmentStartReason,
        telemetry_state: ride_maps::RouteTelemetryState,
    ) -> RecordingPoint {
        RecordingPoint {
            sequence,
            point: ride_maps::RideMapPoint::new_with_start_reason(
                sample,
                segment_id,
                telemetry_state,
                start_reason,
            ),
        }
    }

    fn summary(&self) -> RecordingSummary {
        let summary = self.recorder.summary();
        RecordingSummary {
            point_count: summary.point_count().as_u64(),
            distance_millimetres: summary.distance_millimetres(),
            duration_milliseconds: self.recorder.duration_milliseconds().as_u64(),
        }
    }

    fn snapshot(&self, state: ride_maps::RideLifecycleState) -> Option<RecordingSnapshot> {
        let ride_id = self.ride_id?;
        Some(RecordingSnapshot {
            ride_id,
            revision: self.revision,
            command_token: RecordingToken {
                ride_id,
                generation: self.generation,
            },
            recording_token: (state == ride_maps::RideLifecycleState::Active).then_some(
                RecordingToken {
                    ride_id,
                    generation: self.generation,
                },
            ),
            state,
            allowed_actions: state.recording_actions(),
            telemetry_state: self.recorder.telemetry_state_at(
                self.recorder
                    .recording_timing()
                    .last_monotonic_milliseconds(),
            ),
            summary: self.summary(),
            segment_count: self.recorder.segment_count().as_u64(),
            associated_vehicle: self.recorder.associated_vehicle().map(str::to_owned),
            recorded_bounds_available: matches!(
                state,
                ride_maps::RideLifecycleState::Stopped
                    | ride_maps::RideLifecycleState::Interrupted
                    | ride_maps::RideLifecycleState::Discarded
                    | ride_maps::RideLifecycleState::Saved
                    | ride_maps::RideLifecycleState::Imported
            ),
        })
    }

    fn snapshot_at(
        &self,
        state: ride_maps::RideLifecycleState,
        at_milliseconds: u64,
    ) -> Option<RecordingSnapshot> {
        let at_milliseconds = self.logical_monotonic_milliseconds(at_milliseconds);
        self.snapshot_at_logical(state, at_milliseconds)
    }

    fn snapshot_at_logical(
        &self,
        state: ride_maps::RideLifecycleState,
        at_milliseconds: u64,
    ) -> Option<RecordingSnapshot> {
        let mut snapshot = self.snapshot(state)?;
        snapshot.telemetry_state = self
            .recorder
            .telemetry_state_at(ride_maps::MonotonicMilliseconds::new(at_milliseconds));
        snapshot.summary.duration_milliseconds = self
            .recorder
            .duration_milliseconds_at(ride_maps::MonotonicMilliseconds::new(at_milliseconds))
            .as_u64();
        Some(snapshot)
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "caller-owned candidate identity is retained"
    )]
    /// Starts a new recording with an optional previous-vehicle hint.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when the requested recording transition is invalid or durable persistence fails.
    pub fn start_gps_only(
        &mut self,
        at_ms: u64,
        last_connected_vehicle: Option<String>,
    ) -> Result<RecordingSnapshot, RecordingError> {
        if self.recorder.state().is_some_and(|current| {
            !matches!(
                current,
                ride_maps::RideLifecycleState::Stopped
                    | ride_maps::RideLifecycleState::Interrupted
                    | ride_maps::RideLifecycleState::Saved
                    | ride_maps::RideLifecycleState::Discarded
            )
        }) {
            return Err(RecordingError::AlreadyRecording);
        }
        self.monotonic_epoch_offset_milliseconds = 0;
        let mut staged_recorder = self.recorder.clone();
        staged_recorder
            .start(
                ride_maps::MonotonicMilliseconds::new(at_ms),
                last_connected_vehicle
                    .as_deref()
                    .and_then(ride_maps::VehicleIdentity::new),
            )
            .map_err(|_| RecordingError::AlreadyRecording)?;
        let id = if let Some(database) = self.database.as_ref() {
            let wall_clock_milliseconds: u64 = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| RecordingError::Storage(error.to_string()))?
                .as_millis()
                .try_into()
                .map_err(|error: std::num::TryFromIntError| {
                    RecordingError::Storage(error.to_string())
                })?;
            database
                .create_started_live_ride(
                    wall_clock_milliseconds,
                    at_ms,
                    last_connected_vehicle.as_deref(),
                )
                .map_err(RecordingError::from)?
        } else {
            RideId::new()
        };
        self.recorder = staged_recorder.clone();
        self.admission_recorder = staged_recorder;
        self.ride_id = Some(id);
        self.revision = self.revision.saturating_add(1);
        self.generation = self.generation.saturating_add(1);
        self.reset_music_history_policy();
        self.pending_location_writes.clear();
        self.recoverable_updated_at_milliseconds = None;
        self.snapshot(ride_maps::RideLifecycleState::Active)
            .ok_or(RecordingError::NoActiveRide)
    }
}

impl RideRecordingSession {
    /// Applies a rider command only to the lifecycle that was displayed when requested.
    ///
    /// A missing token means the caller observed no recording. Telemetry and route updates
    /// do not change this token; lifecycle transitions and replacement rides do.
    ///
    /// # Errors
    /// Returns [`RecordingError::StaleCommand`] without mutation when the target changed,
    /// or the underlying lifecycle/storage error when the requested command cannot complete.
    pub fn apply_command(
        &mut self,
        expected: Option<RecordingToken>,
        event: ride_maps::RideEvent,
        at_ms: u64,
        last_connected_vehicle: Option<String>,
    ) -> Result<RecordingSnapshot, RecordingError> {
        let current = self.ride_id.map(|ride_id| RecordingToken {
            ride_id,
            generation: self.generation,
        });
        if expected != current {
            return Err(RecordingError::StaleCommand);
        }
        match event {
            ride_maps::RideEvent::Start => self.start_gps_only(at_ms, last_connected_vehicle),
            ride_maps::RideEvent::Save => self.save(),
            ride_maps::RideEvent::Discard => self.discard(),
            _ => self.transition_at(event, at_ms),
        }
    }

    fn logical_monotonic_milliseconds(&self, raw: u64) -> u64 {
        raw.saturating_add(self.monotonic_epoch_offset_milliseconds)
    }

    fn transition_inner(
        &mut self,
        event: ride_maps::RideEvent,
    ) -> Result<RecordingSnapshot, RecordingError> {
        let at_milliseconds = self
            .recorder
            .recording_timing()
            .last_monotonic_milliseconds()
            .as_u64();
        let (_, next) = self.transition_state(event, at_milliseconds)?;
        let _ = self.poll_location_writes();
        self.recorder.apply_transition(next);
        self.admission_recorder.apply_transition(next);
        self.revision = self.revision.saturating_add(1);
        self.generation = self.generation.saturating_add(1);
        self.snapshot(next).ok_or(RecordingError::NoActiveRide)
    }
    /// Applies a durable lifecycle event, rebasing recovered clocks on resume.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when the requested recording transition is invalid or durable persistence fails.
    pub fn transition_at(
        &mut self,
        event: ride_maps::RideEvent,
        at_milliseconds: u64,
    ) -> Result<RecordingSnapshot, RecordingError> {
        let epoch_offset = if event == ride_maps::RideEvent::Resume
            && self.recorder.state() == Some(ride_maps::RideLifecycleState::Interrupted)
        {
            self.recorder
                .recording_timing()
                .last_monotonic_milliseconds()
                .as_u64()
                .saturating_sub(at_milliseconds)
        } else {
            self.monotonic_epoch_offset_milliseconds
        };
        let at_milliseconds = at_milliseconds.saturating_add(epoch_offset);
        let (_, next) = self.transition_state(event, at_milliseconds)?;
        let _ = self.poll_location_writes();
        self.monotonic_epoch_offset_milliseconds = epoch_offset;
        self.recorder
            .apply_transition_at(next, ride_maps::MonotonicMilliseconds::new(at_milliseconds));
        self.admission_recorder
            .apply_transition_at(next, ride_maps::MonotonicMilliseconds::new(at_milliseconds));
        self.revision = self.revision.saturating_add(1);
        self.generation = self.generation.saturating_add(1);
        self.snapshot_at_logical(next, at_milliseconds)
            .ok_or(RecordingError::NoActiveRide)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn horizontal_accuracy_millimetres(value: f64) -> Result<u32, RecordingError> {
    if !value.is_finite() || value < 0.0 {
        return Err(RecordingError::InvalidLocation);
    }
    let millimetres = value * 1_000.0;
    if !millimetres.is_finite() || millimetres > f64::from(u32::MAX) {
        return Err(RecordingError::InvalidLocation);
    }
    Ok(millimetres as u32)
}

impl RideRecordingSession {
    /// Applies decoded telemetry correlated with its verified connection attempt.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when association or durable recording update fails.
    pub fn observe_connection_telemetry(
        &mut self,
        connection: &ConnectionAttemptToken,
        at_ms: u64,
    ) -> Result<RecordingSnapshot, RecordingError> {
        let superseded = self.connection_attempt.as_ref().is_some_and(|current| {
            connection.generation < current.generation
                || (connection.generation == current.generation && connection != current)
        });
        if !superseded {
            let open = matches!(
                self.recorder.state(),
                Some(ride_maps::RideLifecycleState::Active | ride_maps::RideLifecycleState::Paused)
            );
            if self.connection_attempt.as_ref() != Some(connection)
                || (open && self.recorder.associated_vehicle().is_none())
            {
                let snapshot =
                    self.ensure_recording_for_vehicle(&connection.platform_identifier, at_ms)?;
                if snapshot.associated_vehicle.as_deref()
                    == Some(connection.platform_identifier.as_str())
                    || !matches!(
                        snapshot.state,
                        ride_maps::RideLifecycleState::Active
                            | ride_maps::RideLifecycleState::Paused
                    )
                {
                    self.connection_attempt = Some(connection.clone());
                }
            }
            if let Some(identity) = ride_maps::VehicleIdentity::new(&connection.platform_identifier)
            {
                let logical_at = self.logical_monotonic_milliseconds(at_ms);
                self.observe_telemetry(
                    &identity,
                    ride_maps::MonotonicMilliseconds::new(logical_at),
                )?;
            }
        }
        let lifecycle = self.recorder.state().ok_or(RecordingError::NoActiveRide)?;
        self.snapshot_at(lifecycle, at_ms)
            .ok_or(RecordingError::NoActiveRide)
    }
    /// Saves the selected recording after pending writes settle.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when the requested recording transition is invalid or durable persistence fails.
    pub fn save(&mut self) -> Result<RecordingSnapshot, RecordingError> {
        let snapshot = self.transition_inner(ride_maps::RideEvent::Save)?;
        self.admission_recorder = self.recorder.clone();
        Ok(snapshot)
    }
    /// Discards the selected recording and clears its optional history context.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when the requested recording transition is invalid or durable persistence fails.
    pub fn discard(&mut self) -> Result<RecordingSnapshot, RecordingError> {
        let snapshot = self.transition_inner(ride_maps::RideEvent::Discard)?;
        self.pending_location_writes.clear();
        self.admission_recorder = self.recorder.clone();
        self.reset_music_history_policy();
        Ok(snapshot)
    }
    /// Admits timestamped phone samples only for their captured recording generation.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when canonical location admission or durable submission fails.
    pub fn ingest_location_batch(
        &mut self,
        recording: Option<RecordingToken>,
        receipt_monotonic_ms: u64,
        receipt_wall_clock_unix_ms: u64,
        samples: Vec<PevcapPhoneLocation>,
    ) -> Result<Vec<RecordingDecision>, RecordingError> {
        let Some(recording) = recording else {
            return Ok(Vec::new());
        };
        if self.ride_id != Some(recording.ride_id)
            || self.generation != recording.generation
            || self.recorder.state() != Some(ride_maps::RideLifecycleState::Active)
        {
            return Ok(Vec::new());
        }
        let mut decisions = Vec::with_capacity(samples.len());
        for sample in samples {
            let Ok(sample) = sample.canonical() else {
                continue;
            };
            let Some(horizontal_accuracy_meters) = sample.horizontal_accuracy_meters else {
                continue;
            };
            let Some(elapsed_ms) =
                receipt_wall_clock_unix_ms.checked_sub(sample.wall_clock_unix_ms)
            else {
                // A source timestamp newer than the callback receipt is invalid.
                continue;
            };
            let Some(monotonic_ms) = receipt_monotonic_ms.checked_sub(elapsed_ms) else {
                continue;
            };
            match self.ingest_location(
                monotonic_ms,
                sample.wall_clock_unix_ms,
                sample.latitude_degrees,
                sample.longitude_degrees,
                horizontal_accuracy_meters,
            ) {
                Ok(decision) => decisions.push(decision),
                // No active ride remains; every remaining sample would return NoActiveRide,
                // so stop without discarding decisions already collected.
                Err(RecordingError::NoActiveRide) => break,
                Err(error) => return Err(error),
            }
        }
        Ok(decisions)
    }

    /// Returns the current durable snapshot without querying storage.
    #[must_use]
    pub fn current_snapshot(&self, at_ms: u64) -> Option<RecordingSnapshot> {
        self.ride_id
            .zip(self.recorder.state())
            .and_then(|(_, state)| self.snapshot_at(state, at_ms))
    }
    /// Returns any failure loading the existing recording.
    #[must_use]
    pub fn initialization_error(&self) -> Option<RecordingError> {
        self.initialization_error.clone()
    }
    /// Borrows the bounded durable recorder for route projection.
    #[must_use]
    pub fn recorder(&self) -> &ride_maps::RideMapRecorder {
        &self.recorder
    }
    /// Returns the shared database worker handle, when this session is durable.
    #[must_use]
    pub fn database(&self) -> Option<&RideDatabase> {
        self.database.as_ref()
    }
    /// Returns the selected canonical ride identity, including terminal state.
    #[must_use]
    pub fn ride_id(&self) -> Option<RideId> {
        self.ride_id
    }
    /// Returns whether persistence results remain to be consumed.
    #[must_use]
    pub fn has_pending_location_writes(&self) -> bool {
        !self.pending_location_writes.is_empty()
    }
    /// Associates confirmed vehicle evidence using this session's logical clock.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when vehicle association metadata cannot be persisted.
    pub fn observe_vehicle_connection(
        &mut self,
        identifier: &str,
        at_ms: u64,
    ) -> Result<ride_maps::VehicleAssociation, RecordingError> {
        let Some(identity) = ride_maps::VehicleIdentity::new(identifier) else {
            return Ok(ride_maps::VehicleAssociation::CandidateMissing);
        };
        self.observe_vehicle(
            &identity,
            ride_maps::MonotonicMilliseconds::new(self.logical_monotonic_milliseconds(at_ms)),
        )
    }
    /// Applies confirmed telemetry using this session's logical clock.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when confirmed telemetry metadata cannot be persisted.
    pub fn observe_vehicle_telemetry(
        &mut self,
        identifier: &str,
        at_ms: u64,
    ) -> Result<ride_maps::TelemetryObservation, RecordingError> {
        let Some(identity) = ride_maps::VehicleIdentity::new(identifier) else {
            return Ok(ride_maps::TelemetryObservation::IdentityMismatch);
        };
        self.observe_telemetry(
            &identity,
            ride_maps::MonotonicMilliseconds::new(self.logical_monotonic_milliseconds(at_ms)),
        )
    }
}

impl RideRecordingSession {
    /// Returns the selected recording's retained listening policy.
    pub fn current_music_history_policy(&mut self) -> MusicHistoryPolicy {
        let Some(id) = self.ride_id else {
            return MusicHistoryPolicy::Disabled;
        };
        if let Some(database) = &self.database
            && let Ok(policy) = database.music_history_policy(id)
        {
            self.music_history_policy = policy;
        }
        self.music_history_policy
    }
    /// Changes retention for an open recording after durable persistence succeeds.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when the ride is not open or the durable policy update fails.
    pub fn set_music_history_policy(
        &mut self,
        policy: MusicHistoryPolicy,
    ) -> Result<(), RecordingError> {
        let id = self.ride_id.ok_or(RecordingError::NoActiveRide)?;
        if !matches!(
            self.recorder.state(),
            Some(ride_maps::RideLifecycleState::Active | ride_maps::RideLifecycleState::Paused)
        ) {
            return Err(RecordingError::InvalidTransition);
        }
        self.database
            .as_ref()
            .ok_or_else(RecordingError::storage_unavailable)?
            .save_music_history_policy(id, policy)?;
        self.apply_music_history_policy(policy);
        Ok(())
    }
    /// Returns selected history; discarded rides have no remaining history.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when the selected history cannot be read.
    pub fn current_music_history(&mut self) -> Option<Result<crate::MusicHistory, RecordingError>> {
        if self.recorder.state() == Some(ride_maps::RideLifecycleState::Discarded) {
            return None;
        }
        let id = self.ride_id?;
        let result = self
            .database
            .as_ref()
            .ok_or_else(RecordingError::storage_unavailable)
            .and_then(|database| database.music_history(id).map_err(Into::into));
        Some(result)
    }
    /// Removes the selected recording's listening history.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when there is no selected ride or deletion fails.
    pub fn delete_current_music_history(&mut self) -> Result<(), RecordingError> {
        let id = self.ride_id.ok_or(RecordingError::NoActiveRide)?;
        self.database
            .as_ref()
            .ok_or_else(RecordingError::storage_unavailable)?
            .delete_music_history(id)?;
        self.reset_music_history_policy();
        Ok(())
    }
    /// Records a provider transition against the durable recording clock and retention policy.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when music input is invalid or the durable history operation fails.
    pub fn record_music_event_with_sequence(
        &self,
        snapshot: &cutout_music::MusicSnapshot,
        kind: cutout_music::MusicRideEventKind,
        monotonic_at_ms: u64,
        wall_clock_at_ms: u64,
        clock_uncertainty_ms: u64,
    ) -> Result<Option<crate::MusicTimelineRecordResult>, RecordingError> {
        use cutout_core::{MonotonicTimestamp, WallClockUnixTimestamp};
        use cutout_music::{MusicPlaybackState, MusicRideEvent, MusicTimelineOutcome};
        let id = self.ride_id.ok_or(RecordingError::NoActiveRide)?;
        let monotonic_at_ms = self.logical_monotonic_milliseconds(monotonic_at_ms);
        let refused = |outcome| {
            Ok(Some(crate::MusicTimelineRecordResult {
                outcome,
                sequence: None,
            }))
        };
        if self.recorder.state() != Some(ride_maps::RideLifecycleState::Active) {
            return Ok(None);
        }
        if snapshot.state() == MusicPlaybackState::Stale
            || snapshot.observed_at() > MonotonicTimestamp::from_milliseconds(monotonic_at_ms)
        {
            return refused(MusicTimelineOutcome::OutOfOrder);
        }
        let database = self
            .database
            .as_ref()
            .ok_or_else(RecordingError::storage_unavailable)?;
        let policy = database.music_history_policy(id)?;
        let Some(event) = MusicRideEvent::try_from_snapshot(
            snapshot,
            kind,
            MonotonicTimestamp::from_milliseconds(monotonic_at_ms),
            WallClockUnixTimestamp::from_milliseconds(wall_clock_at_ms),
            clock_uncertainty_ms,
            policy,
        )
        .map_err(|error| RecordingError::InvalidMusicInput(error.to_string()))?
        else {
            return refused(MusicTimelineOutcome::Disabled);
        };
        Ok(Some(database.record_music_event_with_sequence(id, event)?))
    }
}

/// A bounded page from the selected recording.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordingPointPage {
    /// Canonical points in sequence order.
    pub points: Vec<RecordingPoint>,
    /// Cursor for the next page when more points remain.
    pub next_cursor: Option<u64>,
}
impl RideRecordingSession {
    /// Returns a bounded page without materializing the full canonical route.
    ///
    /// # Errors
    /// Returns [`RecordingError`] when the database cannot load the bounded route page.
    pub fn points_after(
        &self,
        after: Option<u64>,
        limit: u32,
    ) -> Result<RecordingPointPage, RecordingError> {
        if limit == 0 {
            return Ok(RecordingPointPage {
                points: Vec::new(),
                next_cursor: None,
            });
        }
        let limit = limit.min(500);
        if let (Some(database), Some(id)) = (&self.database, self.ride_id) {
            let page = database.route_points(
                id,
                after.map(RoutePointCursor::new),
                QueryLimit::new(limit)?,
            )?;
            return Ok(RecordingPointPage {
                points: page
                    .points()
                    .iter()
                    .map(|point| {
                        Self::point_from_location(
                            point.sample(),
                            point.sequence(),
                            ride_maps::RideMapSegmentId::new(point.segment_id()),
                            point.start_reason(),
                            point.telemetry_state(),
                        )
                    })
                    .collect(),
                next_cursor: page.next_cursor().map(RoutePointCursor::sequence),
            });
        }
        let first = self.recorder.first_point_sequence().as_u64();
        let start = after.map_or(first, |value| value.saturating_add(1));
        let mut points: Vec<_> = self
            .recorder
            .points()
            .iter()
            .copied()
            .enumerate()
            .map(|(offset, point)| RecordingPoint {
                sequence: first.saturating_add(offset as u64),
                point,
            })
            .filter(|point| point.sequence >= start)
            .take(limit as usize + 1)
            .collect();
        let next_cursor = if points.len() > limit as usize {
            points.pop();
            points.last().map(|point| point.sequence)
        } else {
            None
        };
        Ok(RecordingPointPage {
            points,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests;
