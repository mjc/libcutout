//! Recording state shared by persistence owners and the mobile command boundary.

use cutout_ride_maps::{
    MonotonicMilliseconds, RideEvent, RideLifecycleState, RideMapRecorder, VehicleIdentity,
};

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

/// Immutable location status and acquisition intent for one owner revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocationAcquisition {
    /// Revision shared with the recording owner.
    pub revision: u64,
    /// Reason location is available or unavailable.
    pub availability: LocationAvailability,
    /// Required native acquisition work.
    pub demand: LocationDemand,
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
    location_environment: Option<LocationEnvironment>,
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
            location_environment: None,
        }
    }

    /// Updates native location evidence without changing the ride lifecycle.
    pub fn observe_location_environment(&mut self, environment: LocationEnvironment) {
        if self.location_environment != Some(environment) {
            self.location_environment = Some(environment);
            self.revision = self.revision.saturating_add(1);
            if let Some(snapshot) = &mut self.snapshot {
                snapshot.revision = self.revision;
            }
        }
    }

    /// Projects native acquisition work from location evidence and the current ride lifecycle.
    #[must_use]
    pub fn location_acquisition(&self) -> LocationAcquisition {
        let availability = match self.location_environment {
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
        let demand = if self.recorder.state() == Some(RideLifecycleState::Active) {
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
        LocationAcquisition {
            revision: self.revision,
            availability,
            demand,
        }
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
