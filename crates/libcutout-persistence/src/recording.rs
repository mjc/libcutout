//! Recording state shared by persistence owners and the mobile command boundary.

use cutout_ride_maps::{
    MonotonicMilliseconds, RideLifecycleState, RideMapRecorder, VehicleIdentity,
};

use crate::{RideDatabase, RideId};

#[cfg(test)]
mod tests;

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
}
