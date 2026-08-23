use crate::{
    LocationAdmission, LocationSample, RideEvent, RideLifecycleState, RideSummary, TransitionError,
    distance_between_millimetres,
};

/// One accepted route sample with its Rust-owned segment identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RideMapPoint {
    sample: LocationSample,
    segment_id: u64,
}

impl RideMapPoint {
    /// Creates a segmented route point.
    #[must_use]
    pub const fn new(sample: LocationSample, segment_id: u64) -> Self {
        Self { sample, segment_id }
    }

    /// Returns the canonical location sample.
    #[must_use]
    pub const fn sample(self) -> LocationSample {
        self.sample
    }

    /// Returns the segment identity within the ride.
    #[must_use]
    pub const fn segment_id(self) -> u64 {
        self.segment_id
    }
}

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
#[derive(Clone, Debug)]
pub struct RideMapRecorder {
    state: Option<RideLifecycleState>,
    created_at_milliseconds: u64,
    candidate_vehicle: Option<String>,
    associated_vehicle: Option<String>,
    points: Vec<RideMapPoint>,
    summary: RideSummary,
    segment_id: u64,
    segment_started: bool,
    last_monotonic_milliseconds: u64,
}

impl Default for RideMapRecorder {
    fn default() -> Self {
        Self::new()
    }
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
            summary: RideSummary::from_stored(0, 0),
            segment_id: 0,
            segment_started: false,
            last_monotonic_milliseconds: 0,
        }
    }

    /// Restores a bounded active projection from canonical route samples.
    #[must_use]
    pub fn restored(
        state: RideLifecycleState,
        created_at_milliseconds: u64,
        points: Vec<RideMapPoint>,
    ) -> Self {
        let point_count = points.len() as u64;
        let distance_millimetres = points
            .windows(2)
            .filter(|pair| pair[0].segment_id() == pair[1].segment_id())
            .map(|pair| distance_between_millimetres(pair[0].sample(), pair[1].sample()))
            .sum();
        Self {
            state: Some(state),
            created_at_milliseconds,
            candidate_vehicle: None,
            associated_vehicle: None,
            last_monotonic_milliseconds: points.last().map_or(created_at_milliseconds, |point| {
                point.sample().monotonic_milliseconds()
            }),
            segment_id: points.last().map_or(0, |point| point.segment_id()),
            segment_started: false,
            summary: RideSummary::from_stored(point_count, distance_millimetres),
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

    /// Returns the current segment identity for the next accepted point.
    #[must_use]
    pub const fn current_segment_id(&self) -> u64 {
        self.segment_id
    }

    /// Returns the projected canonical points.
    #[must_use]
    pub fn points(&self) -> &[RideMapPoint] {
        &self.points
    }

    /// Returns the point count, distance, and duration projection.
    #[must_use]
    pub fn summary(&self) -> RideSummary {
        self.summary
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
        self.summary = RideSummary::from_stored(0, 0);
        self.segment_id = 0;
        self.segment_started = true;
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
        if self.state == Some(RideLifecycleState::Paused) && state == RideLifecycleState::Active {
            self.segment_id = self.segment_id.saturating_add(1);
            self.segment_started = true;
        }
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
        let previous = self.points.last().map(|point| point.sample());
        sample.admission(previous.as_ref())
    }

    /// Records a sample after durable storage has accepted it.
    pub fn record_sample(&mut self, sample: LocationSample) -> bool {
        let segment_started = self.segment_started;
        let distance = if segment_started {
            0
        } else {
            self.points.last().map_or(0, |previous| {
                distance_between_millimetres(previous.sample(), sample)
            })
        };
        self.summary = RideSummary::from_stored(
            self.summary.point_count().saturating_add(1),
            self.summary.distance_millimetres().saturating_add(distance),
        );
        self.last_monotonic_milliseconds = sample.monotonic_milliseconds();
        self.points.push(RideMapPoint::new(sample, self.segment_id));
        self.segment_started = false;
        segment_started
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
        recorder.apply_transition(RideLifecycleState::Active);
        assert_eq!(recorder.current_segment_id(), 0);
        recorder.apply_transition(RideLifecycleState::Paused);
        recorder.apply_transition(RideLifecycleState::Active);
        assert_eq!(recorder.current_segment_id(), 1);
        assert!(recorder.record_sample(sample(1_003, 40.001)));
        assert_eq!(
            recorder.points().last().map(|point| point.segment_id()),
            Some(1)
        );
    }

    #[test]
    fn association_requires_the_snapshotted_candidate_and_stays_stable() {
        let mut no_candidate = RideMapRecorder::new();
        no_candidate.start(1_000, None).expect("starts");
        assert_eq!(
            no_candidate.observe_vehicle("pev-1", 1_001),
            VehicleAssociation::CandidateMissing
        );
        assert!(no_candidate.associated_vehicle().is_none());

        let mut associated = RideMapRecorder::new();
        associated
            .start(1_000, Some("pev-1".to_owned()))
            .expect("starts");
        assert_eq!(
            associated.observe_vehicle("pev-1", 1_001),
            VehicleAssociation::Associated
        );
        assert_eq!(
            associated.observe_vehicle("pev-2", 1_002),
            VehicleAssociation::IdentityMismatch
        );
        assert_eq!(associated.associated_vehicle(), Some("pev-1"));
    }

    #[test]
    fn changed_coordinates_at_the_same_monotonic_time_are_out_of_order() {
        let mut recorder = RideMapRecorder::new();
        recorder.start(1_000, None).expect("starts");
        let first = sample(1_001, 40.0);
        recorder.record_sample(first);
        let changed = sample(1_001, 40.001);
        assert_eq!(
            recorder.check_sample(&changed),
            LocationAdmission::OutOfOrder
        );
    }
}
