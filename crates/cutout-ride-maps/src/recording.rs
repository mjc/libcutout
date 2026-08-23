use crate::{
    LocationAdmission, LocationSample, RideEvent, RideLifecycleState, RideSummary, TransitionError,
};

/// Vehicle association result for one connected platform identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VehicleAssociation {
    /// The identity was associated with the recording.
    Associated,
    /// The same identity was already associated.
    AlreadyAssociated,
    /// No candidate identity was available.
    CandidateMissing,
    /// The identity conflicts with the candidate identity.
    IdentityMismatch,
    /// The callback timestamp moved backwards.
    TimestampOutOfOrder,
    /// No recording is open for association.
    RideNotOpen,
}

/// Rust-owned live recording projection independent of storage or FFI DTOs.
#[derive(Clone, Debug, Default)]
pub struct RideMapRecorder {
    state: Option<RideLifecycleState>,
    created_at_milliseconds: u64,
    candidate_vehicle: Option<String>,
    associated_vehicle: Option<String>,
    points: Vec<LocationSample>,
    last_monotonic_milliseconds: u64,
}

impl RideMapRecorder {
    /// Creates an empty recording projection.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: None,
            created_at_milliseconds: 0,
            candidate_vehicle: None,
            associated_vehicle: None,
            points: Vec::new(),
            last_monotonic_milliseconds: 0,
        }
    }

    /// Restores a bounded active projection from canonical route samples.
    #[must_use]
    pub fn restored(
        state: RideLifecycleState,
        created_at_milliseconds: u64,
        points: Vec<LocationSample>,
    ) -> Self {
        Self {
            state: Some(state),
            created_at_milliseconds,
            candidate_vehicle: None,
            associated_vehicle: None,
            last_monotonic_milliseconds: points.last().map_or(created_at_milliseconds, |point| {
                point.monotonic_milliseconds()
            }),
            points,
        }
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> Option<RideLifecycleState> {
        self.state
    }

    /// Returns the associated vehicle identity, if one exists.
    #[must_use]
    pub fn associated_vehicle(&self) -> Option<&str> {
        self.associated_vehicle.as_deref()
    }

    /// Returns the projected canonical points.
    #[must_use]
    pub fn points(&self) -> &[LocationSample] {
        &self.points
    }

    /// Returns the point count, distance, and duration projection.
    #[must_use]
    pub fn summary(&self) -> RideSummary {
        RideSummary::from_samples(&self.points)
    }

    /// Returns elapsed recording time using the latest accepted monotonic sample.
    #[must_use]
    pub const fn duration_milliseconds(&self) -> u64 {
        self.last_monotonic_milliseconds
            .saturating_sub(self.created_at_milliseconds)
    }

    /// Starts a new recording projection.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::Invalid`] when another recording is open.
    pub fn start(
        &mut self,
        at_milliseconds: u64,
        candidate_vehicle: Option<String>,
    ) -> Result<(), TransitionError> {
        if !matches!(
            self.state,
            None | Some(RideLifecycleState::Saved | RideLifecycleState::Discarded)
        ) {
            return Err(TransitionError::Invalid);
        }
        self.state = Some(RideLifecycleState::Active);
        self.created_at_milliseconds = at_milliseconds;
        self.last_monotonic_milliseconds = at_milliseconds;
        self.candidate_vehicle = candidate_vehicle;
        self.associated_vehicle = None;
        self.points.clear();
        Ok(())
    }

    /// Validates a lifecycle event without mutating the projection.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::Invalid`] when no recording is open or the event is invalid.
    pub fn validate_transition(
        &self,
        event: RideEvent,
    ) -> Result<RideLifecycleState, TransitionError> {
        self.state.ok_or(TransitionError::Invalid)?.apply(event)
    }

    /// Applies a previously validated lifecycle state.
    pub fn apply_transition(&mut self, state: RideLifecycleState) {
        self.state = Some(state);
    }

    /// Reconciles one connected vehicle identity with the recording.
    #[must_use]
    pub fn observe_vehicle(
        &mut self,
        platform_identifier: &str,
        at_milliseconds: u64,
    ) -> VehicleAssociation {
        if !matches!(
            self.state,
            Some(RideLifecycleState::Active | RideLifecycleState::Paused)
        ) {
            return VehicleAssociation::RideNotOpen;
        }
        if at_milliseconds < self.last_monotonic_milliseconds {
            return VehicleAssociation::TimestampOutOfOrder;
        }
        if self.associated_vehicle.as_deref() == Some(platform_identifier) {
            return VehicleAssociation::AlreadyAssociated;
        }
        if self
            .candidate_vehicle
            .as_deref()
            .is_some_and(|candidate| candidate != platform_identifier)
        {
            return VehicleAssociation::IdentityMismatch;
        }
        self.associated_vehicle = Some(platform_identifier.to_owned());
        self.candidate_vehicle = None;
        VehicleAssociation::Associated
    }

    /// Checks a candidate sample against the latest accepted sample.
    #[must_use]
    pub fn check_sample(&self, sample: &LocationSample) -> LocationAdmission {
        sample.admission(self.points.last())
    }

    /// Records a sample after durable storage has accepted it.
    pub fn record_sample(&mut self, sample: LocationSample) {
        self.last_monotonic_milliseconds = sample.monotonic_milliseconds();
        self.points.push(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::{RideMapRecorder, VehicleAssociation};
    use crate::{
        Coordinate, LocationAdmission, LocationSample, LocationSource, RideEvent,
        RideLifecycleState,
    };

    fn sample(monotonic: u64, latitude: f64) -> LocationSample {
        LocationSample::new(
            Coordinate::from_degrees(latitude, -105.0).expect("valid coordinate"),
            monotonic,
            1_700_000_000_000 + monotonic,
            None,
            LocationSource::Live,
        )
    }

    #[test]
    fn recorder_owns_transitions_association_and_sample_admission() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(1_000, Some("pev-1".to_owned()))
            .expect("starts");
        assert_eq!(
            recorder.validate_transition(RideEvent::Pause),
            Ok(RideLifecycleState::Paused)
        );
        assert_eq!(
            recorder.observe_vehicle("pev-1", 1_001),
            VehicleAssociation::Associated
        );

        let first = sample(1_001, 40.0);
        assert_eq!(recorder.check_sample(&first), LocationAdmission::Accepted);
        recorder.record_sample(first);
        assert_eq!(recorder.check_sample(&first), LocationAdmission::Duplicate);
        assert_eq!(
            recorder.observe_vehicle("pev-1", 1_002),
            VehicleAssociation::AlreadyAssociated
        );
    }
}
