//! Recording state shared by persistence owners and the mobile command boundary.

use cutout_ride_maps::{
    MonotonicMilliseconds, RideEvent, RideLifecycleState, RideMapRecorder, VehicleIdentity,
};
use num_traits::ToPrimitive;

use crate::{RideDatabase, RideId};

#[cfg(test)]
mod tests;

/// Platform permission evidence for location acquisition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LocationAuthorization {
    /// The platform has not reported a permission decision.
    #[default]
    NotDetermined,
    /// The rider denied location access.
    Denied,
    /// Platform policy restricts location access.
    Restricted,
    /// Foreground location access is authorized.
    WhenInUse,
    /// Background location access is authorized.
    Always,
}

/// Location-service observations supplied by the native adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocationEnvironment {
    /// Current platform permission.
    pub authorization: LocationAuthorization,
    /// Whether system location services are enabled.
    pub services_enabled: bool,
    /// Whether the provider has reported a recoverable acquisition failure.
    pub temporarily_unavailable: bool,
}

/// Why a ride can or cannot currently receive location updates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationAvailability {
    /// Platform observations have not arrived yet.
    Checking,
    /// Location acquisition is permitted and available.
    Ready,
    /// An active recording needs a location permission request.
    PermissionRequired,
    /// The rider denied location access.
    Denied,
    /// Platform policy restricts location access.
    Restricted,
    /// System location services are disabled.
    ServicesDisabled,
    /// Location updates may recover while acquisition remains requested.
    TemporarilyUnavailable,
}

/// Native acquisition work requested by the Rust recording owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationDemand {
    /// No location updates or permission prompt are needed.
    Idle,
    /// An active recording needs a location permission request.
    RequestPermission,
    /// Keep location updates active for the current recording.
    Record,
}

/// Native action that can recover the current location acquisition state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationRecoveryAction {
    /// No user action or permission prompt is currently appropriate.
    None,
    /// Ask the platform for location permission.
    RequestPermission,
    /// Open system settings so the rider can restore location access.
    OpenSettings,
}

/// Immutable location status and acquisition intent for one owner revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocationAcquisition {
    /// Revision shared with the recording owner.
    pub revision: u64,
    /// Reason location is available or unavailable.
    pub availability: LocationAvailability,
    /// Required native acquisition work.
    pub demand: LocationDemand,
    /// Native action selected by Rust to recover the current acquisition state.
    pub recovery_action: LocationRecoveryAction,
}

/// Native location evidence and generation-fenced diagnostic-capture demand.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LocationAcquisitionState {
    environment: Option<LocationEnvironment>,
    diagnostic_capture_generation: u64,
    diagnostic_capture_active: bool,
}

/// Source selected for a live ride-speed readout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingSpeedSource {
    /// Speed decoded from the associated vehicle.
    Vehicle,
    /// Speed reported by the phone location provider.
    PhoneGps,
}

/// A fresh live speed and its observation source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordingSpeed {
    /// Signed speed in millimetres per second.
    pub millimetres_per_second: i32,
    /// Source of the selected observation.
    pub source: RecordingSpeedSource,
}

#[derive(Clone, Copy, Debug)]
struct SpeedObservation {
    millimetres_per_second: i32,
    observed_at_milliseconds: u64,
    recording_generation: u64,
}

#[derive(Clone, Copy, Debug)]
struct PhoneGpsReceipt {
    observed_at_milliseconds: u64,
    recording_generation: u64,
}

/// Selects fresh, ride-correlated speed from vehicle telemetry and phone GPS.
#[derive(Clone, Copy, Debug)]
pub struct RecordingSpeedState {
    vehicle: Option<SpeedObservation>,
    phone_gps: Option<SpeedObservation>,
    phone_gps_receipt: Option<PhoneGpsReceipt>,
    motion: [Option<RideMotionObservation>; 64],
    motion_count: usize,
    latest_verified_motion_at: Option<u64>,
    first_verified_motion_at: Option<u64>,
    retired_motion_before: Option<u64>,
}

/// Motion ownership established by fresh telemetry from the verified associated wheel.
///
/// Parked remains authoritative through missing telemetry and link loss. Only a newer
/// verified moving observation releases it; Bluetooth loss alone cannot distinguish an
/// accidental drop from a powered-off wheel in a car.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideMotionObservation {
    /// Whether the shared connected-wheel movement threshold classified the wheel as parked.
    pub parked: bool,
    /// Original logical source time, never the time of a later GPS callback.
    pub observed_at_milliseconds: u64,
    /// Last parked transition, retained through Riding until a material point crosses it.
    pub last_stationary_at_milliseconds: Option<u64>,
}

impl Default for RecordingSpeedState {
    fn default() -> Self {
        Self {
            vehicle: None,
            phone_gps: None,
            phone_gps_receipt: None,
            motion: [None; 64],
            motion_count: 0,
            latest_verified_motion_at: None,
            first_verified_motion_at: None,
            retired_motion_before: None,
        }
    }
}

impl RecordingSpeedState {
    /// Restores durable motion ownership without fabricating a fresh speed or connection.
    pub fn restore_motion(&mut self, observation: RideMotionObservation) {
        self.motion = [None; 64];
        if !observation.parked
            && let Some(at) = observation.last_stationary_at_milliseconds
            && at < observation.observed_at_milliseconds
        {
            self.motion[0] = Some(RideMotionObservation {
                parked: true,
                observed_at_milliseconds: at,
                last_stationary_at_milliseconds: Some(at),
            });
            self.motion[1] = Some(observation);
            self.motion_count = 2;
        } else {
            self.motion[0] = Some(observation);
            self.motion_count = 1;
        }
        self.latest_verified_motion_at = Some(observation.observed_at_milliseconds);
        self.first_verified_motion_at =
            self.motion[0].map(|motion| motion.observed_at_milliseconds);
        self.retired_motion_before = None;
    }

    /// Admits motion evidence only after the caller verifies wheel identity and connection.
    ///
    /// The bounded history stores transitions rather than every telemetry frame. Callbacks
    /// within retired history are rejected by the recording owner; they never become
    /// unknown fallback or borrow a future wheel observation. True initial unknown motion
    /// before the first verified observation retains GPS-only admission.
    pub fn observe_verified_motion(
        &mut self,
        millimetres_per_second: i32,
        source_at_milliseconds: u64,
        receipt_at_milliseconds: u64,
    ) -> bool {
        if receipt_at_milliseconds < source_at_milliseconds
            || receipt_at_milliseconds.saturating_sub(source_at_milliseconds)
                > cutout_ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS
            || self
                .latest_verified_motion_at
                .is_some_and(|at| source_at_milliseconds <= at)
        {
            return false;
        }
        self.latest_verified_motion_at = Some(source_at_milliseconds);
        self.first_verified_motion_at
            .get_or_insert(source_at_milliseconds);
        let parked =
            !cutout_core::Speed::from_millimetres_per_second(millimetres_per_second).is_moving();
        if self
            .latest_motion()
            .is_some_and(|previous| previous.parked == parked)
        {
            return false;
        }
        if self.motion_count == self.motion.len() {
            self.motion.rotate_left(1);
            self.motion_count -= 1;
            self.retired_motion_before =
                self.motion[0].map(|motion| motion.observed_at_milliseconds);
        }
        let last_stationary_at_milliseconds = if parked {
            Some(source_at_milliseconds)
        } else {
            self.latest_motion()
                .and_then(|motion| motion.last_stationary_at_milliseconds)
        };
        self.motion[self.motion_count] = Some(RideMotionObservation {
            parked,
            observed_at_milliseconds: source_at_milliseconds,
            last_stationary_at_milliseconds,
        });
        self.motion_count += 1;
        true
    }

    /// Returns the latest logical-ride motion transition for durable checkpointing.
    #[must_use]
    pub fn latest_motion(&self) -> Option<RideMotionObservation> {
        self.motion_count
            .checked_sub(1)
            .and_then(|index| self.motion[index])
    }

    /// Whether wheel-owned distance is parked at this GPS source time.
    #[must_use]
    pub fn parked_at(&self, source_at_milliseconds: u64) -> bool {
        self.motion_at(source_at_milliseconds)
            .is_some_and(|motion| motion.parked)
    }

    /// Latest transition at or before the provider's source clock.
    #[must_use]
    pub fn motion_at(&self, source_at_milliseconds: u64) -> Option<RideMotionObservation> {
        self.motion[..self.motion_count]
            .iter()
            .rev()
            .flatten()
            .find(|motion| motion.observed_at_milliseconds <= source_at_milliseconds)
            .copied()
    }

    /// Whether the source clock falls inside known motion history that was retired.
    #[must_use]
    pub fn motion_history_retired_at(&self, source_at_milliseconds: u64) -> bool {
        self.first_verified_motion_at
            .is_some_and(|first| source_at_milliseconds >= first)
            && self
                .retired_motion_before
                .is_some_and(|oldest| source_at_milliseconds < oldest)
    }

    /// Replaces the vehicle observation when its speed is valid.
    pub fn observe_vehicle(
        &mut self,
        millimetres_per_second: Option<i32>,
        at_milliseconds: u64,
        recording_generation: u64,
    ) {
        if let Some(millimetres_per_second) = millimetres_per_second {
            if self.vehicle.is_some_and(|previous| {
                previous.recording_generation == recording_generation
                    && previous.observed_at_milliseconds > at_milliseconds
            }) {
                return;
            }
            self.vehicle = Some(SpeedObservation {
                millimetres_per_second,
                observed_at_milliseconds: at_milliseconds,
                recording_generation,
            });
        }
    }

    /// Accepts strictly newer phone receipts within a recording generation.
    ///
    /// Invalid speed still establishes receipt order; it cannot allow a replay to replace
    /// a valid reading or extend that reading's freshness.
    pub fn observe_phone_gps(
        &mut self,
        metres_per_second: Option<f64>,
        at_milliseconds: u64,
        recording_generation: u64,
    ) {
        if self.phone_gps_receipt.is_some_and(|previous| {
            previous.recording_generation == recording_generation
                && previous.observed_at_milliseconds >= at_milliseconds
        }) {
            return;
        }
        self.phone_gps_receipt = Some(PhoneGpsReceipt {
            observed_at_milliseconds: at_milliseconds,
            recording_generation,
        });
        let Some(speed) = metres_per_second.filter(|speed| speed.is_finite() && *speed >= 0.0)
        else {
            return;
        };
        let millimetres_per_second = speed * 1_000.0;
        if millimetres_per_second > f64::from(i32::MAX) {
            return;
        }
        let Some(millimetres_per_second) = millimetres_per_second.round().to_i32() else {
            return;
        };
        self.phone_gps = Some(SpeedObservation {
            millimetres_per_second,
            observed_at_milliseconds: at_milliseconds,
            recording_generation,
        });
    }

    /// Selects fresh vehicle speed first, falling back to fresh GPS speed.
    #[must_use]
    pub fn selected_at(
        &self,
        lifecycle: RideLifecycleState,
        recording_generation: u64,
        at_milliseconds: u64,
    ) -> Option<RecordingSpeed> {
        if lifecycle != RideLifecycleState::Active {
            return None;
        }
        let fresh = |observation: SpeedObservation| {
            (observation.recording_generation == recording_generation
                && at_milliseconds >= observation.observed_at_milliseconds
                && at_milliseconds.saturating_sub(observation.observed_at_milliseconds)
                    <= cutout_ride_maps::TELEMETRY_FRESHNESS_MILLISECONDS)
                .then_some(observation.millimetres_per_second)
        };
        self.vehicle
            .and_then(fresh)
            .map(|millimetres_per_second| RecordingSpeed {
                millimetres_per_second,
                source: RecordingSpeedSource::Vehicle,
            })
            .or_else(|| {
                self.phone_gps
                    .and_then(fresh)
                    .map(|millimetres_per_second| RecordingSpeed {
                        millimetres_per_second,
                        source: RecordingSpeedSource::PhoneGps,
                    })
            })
    }
}

impl LocationAcquisitionState {
    /// Replaces native permission/provider evidence when it changes.
    pub fn observe_environment(&mut self, environment: LocationEnvironment) -> bool {
        if self.environment == Some(environment) {
            return false;
        }
        self.environment = Some(environment);
        true
    }

    /// Updates diagnostic capture demand, rejecting stale or already-closed generations.
    pub fn observe_diagnostic_capture(&mut self, generation: u64, active: bool) -> bool {
        if generation < self.diagnostic_capture_generation
            || (generation == self.diagnostic_capture_generation
                && (!self.diagnostic_capture_active || active))
        {
            return false;
        }
        self.diagnostic_capture_generation = generation;
        self.diagnostic_capture_active = active;
        true
    }

    /// Selects native acquisition work from the current ride and diagnostic-capture state.
    #[must_use]
    pub fn acquisition(
        &self,
        revision: u64,
        lifecycle: Option<RideLifecycleState>,
    ) -> LocationAcquisition {
        location_acquisition_for(
            revision,
            lifecycle,
            self.environment,
            self.diagnostic_capture_active,
        )
    }
}

/// Applies the shared location policy to a lifecycle snapshot and native environment.
#[must_use]
pub fn location_acquisition_for(
    revision: u64,
    lifecycle: Option<RideLifecycleState>,
    environment: Option<LocationEnvironment>,
    diagnostic_capture_location_active: bool,
) -> LocationAcquisition {
    let availability = match environment {
        None => LocationAvailability::Checking,
        Some(environment) if !environment.services_enabled => {
            LocationAvailability::ServicesDisabled
        }
        Some(LocationEnvironment {
            authorization: LocationAuthorization::NotDetermined,
            ..
        }) => LocationAvailability::PermissionRequired,
        Some(LocationEnvironment {
            authorization: LocationAuthorization::Denied,
            ..
        }) => LocationAvailability::Denied,
        Some(LocationEnvironment {
            authorization: LocationAuthorization::Restricted,
            ..
        }) => LocationAvailability::Restricted,
        Some(LocationEnvironment {
            authorization: LocationAuthorization::WhenInUse | LocationAuthorization::Always,
            temporarily_unavailable: true,
            ..
        }) => LocationAvailability::TemporarilyUnavailable,
        Some(_) => LocationAvailability::Ready,
    };
    let acquisition_requested =
        lifecycle == Some(RideLifecycleState::Active) || diagnostic_capture_location_active;
    let demand = if acquisition_requested {
        match availability {
            LocationAvailability::Ready | LocationAvailability::TemporarilyUnavailable => {
                LocationDemand::Record
            }
            LocationAvailability::PermissionRequired => LocationDemand::RequestPermission,
            _ => LocationDemand::Idle,
        }
    } else {
        LocationDemand::Idle
    };
    let recovery_action = if acquisition_requested {
        match availability {
            LocationAvailability::PermissionRequired => LocationRecoveryAction::RequestPermission,
            LocationAvailability::Denied | LocationAvailability::ServicesDisabled => {
                LocationRecoveryAction::OpenSettings
            }
            _ => LocationRecoveryAction::None,
        }
    } else {
        LocationRecoveryAction::None
    };
    LocationAcquisition {
        revision,
        availability,
        demand,
        recovery_action,
    }
}

/// Correlates acquired input with a ride and its recording generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordingToken {
    /// Ride that was active when the input was acquired.
    pub ride_id: RideId,
    /// Generation changed by recording lifecycle transitions.
    pub generation: u64,
}

/// Immutable lifecycle snapshot of a recording owned by Rust.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingSnapshot {
    /// Identifier created by the recording owner.
    pub ride_id: RideId,
    /// Revision advanced when a recording change is published.
    pub revision: u64,
    /// Token for inputs acquired while recording is active.
    pub recording_token: Option<RecordingToken>,
    /// Published recording lifecycle state.
    pub state: RideLifecycleState,
}

/// Failure to start an in-memory recording.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RecordingError {
    /// A recording is already active or paused.
    #[error("a ride is already recording")]
    AlreadyRecording,
    /// No ride is available for a lifecycle transition.
    #[error("no active ride")]
    NoActiveRide,
    /// The requested transition is invalid for the current lifecycle state.
    #[error("invalid ride transition")]
    InvalidTransition,
    /// Durable changes must be prepared and completed through an asynchronous command.
    #[error("database-backed recording requires an asynchronous command")]
    DatabaseCommandRequired,
}

/// Rust owner of recording state, with an optional shared database worker.
///
/// Construction performs no restore or database writes. This foundation only supports
/// synchronous starts without storage. Database-backed start, restore, and lifecycle changes
/// require asynchronous preparation and completion; command coordination remains at the FFI
/// boundary until that machinery is extracted.
#[derive(Debug)]
pub struct RideRecordingSession {
    database: Option<RideDatabase>,
    recorder: RideMapRecorder,
    revision: u64,
    generation: u64,
    snapshot: Option<RecordingSnapshot>,
    location_acquisition: LocationAcquisitionState,
}

impl RideRecordingSession {
    /// Creates an idle owner without reading or writing durable state.
    #[must_use]
    pub fn new(database: Option<RideDatabase>) -> Self {
        Self {
            database,
            recorder: RideMapRecorder::new(),
            revision: 0,
            generation: 0,
            snapshot: None,
            location_acquisition: LocationAcquisitionState::default(),
        }
    }

    /// Updates native location evidence without changing the ride lifecycle.
    pub fn observe_location_environment(&mut self, environment: LocationEnvironment) {
        if self.location_acquisition.observe_environment(environment) {
            self.revision = self.revision.saturating_add(1);
            if let Some(snapshot) = &mut self.snapshot {
                snapshot.revision = self.revision;
            }
        }
    }

    /// Records whether a diagnostic capture generation requires location updates.
    ///
    /// A closed generation cannot be reopened, and delayed observations from older captures are
    /// ignored so they cannot restart GPS after a later capture has taken ownership.
    pub fn observe_diagnostic_capture_location(&mut self, generation: u64, active: bool) {
        if self
            .location_acquisition
            .observe_diagnostic_capture(generation, active)
        {
            self.revision = self.revision.saturating_add(1);
            if let Some(snapshot) = &mut self.snapshot {
                snapshot.revision = self.revision;
            }
        }
    }

    /// Projects native acquisition work from location evidence and the current ride lifecycle.
    #[must_use]
    pub fn location_acquisition(&self) -> LocationAcquisition {
        self.location_acquisition
            .acquisition(self.revision, self.recorder.state())
    }

    /// Returns the last published recording, or `None` before a recording starts.
    #[must_use]
    pub const fn snapshot(&self) -> Option<&RecordingSnapshot> {
        self.snapshot.as_ref()
    }

    /// Starts an in-memory ride at a monotonic timestamp in milliseconds.
    ///
    /// A candidate vehicle is evidence for later association, rather than a verified connection.
    ///
    /// # Errors
    ///
    /// Returns [`RecordingError::DatabaseCommandRequired`] when storage is configured, or
    /// [`RecordingError::AlreadyRecording`] if the current lifecycle cannot start another ride.
    pub fn start_gps_only(
        &mut self,
        at_milliseconds: u64,
        candidate_vehicle: Option<VehicleIdentity>,
    ) -> Result<RecordingSnapshot, RecordingError> {
        if self.database.is_some() {
            return Err(RecordingError::DatabaseCommandRequired);
        }
        self.recorder
            .start(
                MonotonicMilliseconds::new(at_milliseconds),
                candidate_vehicle,
            )
            .map_err(|_| RecordingError::AlreadyRecording)?;
        self.revision = self.revision.saturating_add(1);
        self.generation = self.generation.saturating_add(1);
        let ride_id = RideId::new();
        let snapshot = RecordingSnapshot {
            ride_id,
            revision: self.revision,
            recording_token: Some(RecordingToken {
                ride_id,
                generation: self.generation,
            }),
            state: RideLifecycleState::Active,
        };
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    /// Applies a lifecycle event to an in-memory recording.
    ///
    /// Database-backed lifecycle changes must be prepared and completed through the asynchronous
    /// command boundary so the in-memory state is published only after durable acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns [`RecordingError::DatabaseCommandRequired`] when storage is configured,
    /// [`RecordingError::NoActiveRide`] when no ride exists, or
    /// [`RecordingError::InvalidTransition`] when the event is not valid for the current state.
    pub fn transition(
        &mut self,
        event: RideEvent,
        at_milliseconds: u64,
    ) -> Result<RecordingSnapshot, RecordingError> {
        if self.database.is_some() {
            return Err(RecordingError::DatabaseCommandRequired);
        }
        let current = self.recorder.state().ok_or(RecordingError::NoActiveRide)?;
        let ride_id = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.ride_id)
            .ok_or(RecordingError::NoActiveRide)?;
        let transition = current
            .transition(event)
            .map_err(|_| RecordingError::InvalidTransition)?;
        self.recorder
            .apply_transition_at(transition, MonotonicMilliseconds::new(at_milliseconds))
            .map_err(|_| RecordingError::InvalidTransition)?;
        self.revision = self.revision.saturating_add(1);
        self.generation = self.generation.saturating_add(1);
        let state = transition.next();
        let snapshot = RecordingSnapshot {
            ride_id,
            revision: self.revision,
            recording_token: (state == RideLifecycleState::Active).then_some(RecordingToken {
                ride_id,
                generation: self.generation,
            }),
            state,
        };
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }
}
