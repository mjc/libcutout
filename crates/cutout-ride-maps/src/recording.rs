use crate::{
    LocationAdmission, LocationSample, MonotonicMilliseconds, RideEvent, RideLifecycleState,
    RidePointCount, RideSummary, TransitionError, distance_between_millimetres,
};

const MAX_HORIZONTAL_ACCURACY_MILLIMETRES: u32 = 100_000;
const MAX_GAP_MILLISECONDS: u64 = 30_000;
const MAX_IMPLIED_SPEED_MILLIMETRES_PER_SECOND: u64 = 100_000;

/// A non-empty platform identity for a connected vehicle.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VehicleIdentity(String);

/// Error returned when a vehicle identity is empty after trimming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VehicleIdentityError;

impl std::fmt::Display for VehicleIdentityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("vehicle identity must not be empty")
    }
}

impl std::error::Error for VehicleIdentityError {}

impl VehicleIdentity {
    /// Creates an identity after trimming surrounding whitespace.
    #[must_use]
    pub fn new(value: impl AsRef<str>) -> Option<Self> {
        let value = value.as_ref().trim();
        (!value.is_empty()).then(|| Self(value.to_owned()))
    }

    /// Returns the platform identity string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for VehicleIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<VehicleIdentity> for String {
    fn from(identity: VehicleIdentity) -> Self {
        identity.0
    }
}

impl TryFrom<&str> for VehicleIdentity {
    type Error = VehicleIdentityError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(VehicleIdentityError)
    }
}

/// Maximum age of telemetry that qualifies a route point as fresh.
pub const TELEMETRY_FRESHNESS_MILLISECONDS: u64 = 2_000;
/// Maximum number of live route points retained by the in-memory projection.
pub const MAX_LIVE_ROUTE_POINTS: usize = 4_096;

/// One accepted route sample with its Rust-owned segment identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RideMapPoint {
    sample: LocationSample,
    segment_id: RideMapSegmentId,
    telemetry_state: RouteTelemetryState,
}

/// Stable segment identity within one ride recording.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RideMapSegmentId(u64);

impl RideMapSegmentId {
    /// Creates a segment identity from its persisted representation.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the persisted representation.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Provenance of the vehicle telemetry associated with one route point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RouteTelemetryState {
    /// The point was recorded before a vehicle was associated.
    GpsOnly,
    /// A vehicle was associated but no telemetry sample was observed.
    AssociatedNoTelemetry,
    /// A fresh telemetry sample was observed for the point.
    AssociatedFresh,
    /// The latest telemetry sample was stale for the point.
    AssociatedStale,
}

impl RouteTelemetryState {
    /// Returns the stable `SQLite` representation.
    #[must_use]
    pub const fn storage_value(self) -> i64 {
        match self {
            Self::GpsOnly => 0,
            Self::AssociatedNoTelemetry => 1,
            Self::AssociatedFresh => 2,
            Self::AssociatedStale => 3,
        }
    }

    /// Decodes the stable `SQLite` representation.
    #[must_use]
    pub const fn from_storage(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::GpsOnly),
            1 => Some(Self::AssociatedNoTelemetry),
            2 => Some(Self::AssociatedFresh),
            3 => Some(Self::AssociatedStale),
            _ => None,
        }
    }
}

/// Result of observing one confirmed telemetry timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TelemetryObservation {
    /// The timestamp became the newest telemetry evidence.
    Observed,
    /// The timestamp was already observed.
    AlreadyObserved,
    /// The ride has no confirmed vehicle association.
    NotAssociated,
    /// The timestamp moved backwards.
    TimestampOutOfOrder,
    /// The ride is not open for telemetry.
    RideNotOpen,
}

impl RideMapPoint {
    /// Creates a segmented route point.
    #[must_use]
    pub const fn new(
        sample: LocationSample,
        segment_id: RideMapSegmentId,
        telemetry_state: RouteTelemetryState,
    ) -> Self {
        Self {
            sample,
            segment_id,
            telemetry_state,
        }
    }

    /// Returns the canonical location sample.
    #[must_use]
    pub const fn sample(self) -> LocationSample {
        self.sample
    }

    /// Returns the segment identity within the ride.
    #[must_use]
    pub const fn segment_id(self) -> RideMapSegmentId {
        self.segment_id
    }

    /// Returns the Rust-owned telemetry provenance.
    #[must_use]
    pub const fn telemetry_state(self) -> RouteTelemetryState {
        self.telemetry_state
    }
}

/// Vehicle association result for one connected platform identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
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

/// Persisted association and telemetry metadata for a live recording projection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RideMapMetadata {
    /// Candidate vehicle identity retained for automatic association.
    pub candidate_vehicle: Option<VehicleIdentity>,
    /// Confirmed associated vehicle identity.
    pub associated_vehicle: Option<VehicleIdentity>,
    /// Monotonic timestamp of the confirmed association.
    pub associated_at_milliseconds: Option<MonotonicMilliseconds>,
    /// Newest observed vehicle telemetry timestamp.
    pub last_telemetry_at_milliseconds: Option<MonotonicMilliseconds>,
}

/// Rust-owned live recording projection independent of storage or FFI DTOs.
#[derive(Clone, Debug)]
pub struct RideMapRecorder {
    state: Option<RideLifecycleState>,
    created_at_milliseconds: MonotonicMilliseconds,
    candidate_vehicle: Option<VehicleIdentity>,
    associated_vehicle: Option<VehicleIdentity>,
    associated_at_milliseconds: Option<MonotonicMilliseconds>,
    last_telemetry_at_milliseconds: Option<MonotonicMilliseconds>,
    points: Vec<RideMapPoint>,
    first_point_sequence: u64,
    summary: RideSummary,
    segment_id: RideMapSegmentId,
    segment_started: bool,
    last_monotonic_milliseconds: MonotonicMilliseconds,
    paused_at_milliseconds: Option<MonotonicMilliseconds>,
    paused_duration_milliseconds: u64,
    completed_duration_milliseconds: u64,
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
            created_at_milliseconds: MonotonicMilliseconds::new(0),
            candidate_vehicle: None,
            associated_vehicle: None,
            associated_at_milliseconds: None,
            last_telemetry_at_milliseconds: None,
            points: Vec::new(),
            first_point_sequence: 0,
            summary: RideSummary::from_stored(RidePointCount::new(0), 0),
            segment_id: RideMapSegmentId::new(0),
            segment_started: false,
            last_monotonic_milliseconds: MonotonicMilliseconds::new(0),
            paused_at_milliseconds: None,
            paused_duration_milliseconds: 0,
            completed_duration_milliseconds: 0,
        }
    }

    /// Restores a bounded active projection from canonical route samples.
    #[must_use]
    pub fn restored(
        state: RideLifecycleState,
        created_at_milliseconds: MonotonicMilliseconds,
        points: Vec<RideMapPoint>,
    ) -> Self {
        Self::restored_with_metadata(
            state,
            created_at_milliseconds,
            None,
            None,
            None,
            None,
            points,
        )
    }

    /// Restores a projection with persisted association and telemetry metadata.
    #[must_use]
    pub fn restored_with_metadata(
        state: RideLifecycleState,
        created_at_milliseconds: MonotonicMilliseconds,
        candidate_vehicle: Option<VehicleIdentity>,
        associated_vehicle: Option<VehicleIdentity>,
        associated_at_milliseconds: Option<MonotonicMilliseconds>,
        last_telemetry_at_milliseconds: Option<MonotonicMilliseconds>,
        points: Vec<RideMapPoint>,
    ) -> Self {
        let point_count = RidePointCount::from_usize(points.len());
        let distance_millimetres = points
            .windows(2)
            .filter(|pair| pair[0].segment_id() == pair[1].segment_id())
            .map(|pair| distance_between_millimetres(pair[0].sample(), pair[1].sample()))
            .sum();
        Self::restored_with_summary(
            state,
            created_at_milliseconds,
            RideMapMetadata {
                candidate_vehicle,
                associated_vehicle,
                associated_at_milliseconds,
                last_telemetry_at_milliseconds,
            },
            points,
            RideSummary::from_stored(point_count, distance_millimetres),
        )
    }

    /// Restores a bounded projection with a summary calculated from all persisted points.
    #[must_use]
    pub fn restored_with_metadata_and_summary(
        state: RideLifecycleState,
        created_at_milliseconds: MonotonicMilliseconds,
        metadata: RideMapMetadata,
        points: Vec<RideMapPoint>,
        summary: RideSummary,
    ) -> Self {
        Self::restored_with_summary(state, created_at_milliseconds, metadata, points, summary)
    }

    fn restored_with_summary(
        state: RideLifecycleState,
        created_at_milliseconds: MonotonicMilliseconds,
        metadata: RideMapMetadata,
        mut points: Vec<RideMapPoint>,
        summary: RideSummary,
    ) -> Self {
        if points.len() > MAX_LIVE_ROUTE_POINTS {
            let excess = points.len() - MAX_LIVE_ROUTE_POINTS;
            points.drain(..excess);
        }
        let first_point_sequence = summary
            .point_count()
            .saturating_sub(RidePointCount::from_usize(points.len()))
            .as_u64();
        let last_monotonic_milliseconds = points.last().map_or(created_at_milliseconds, |point| {
            point.sample().monotonic_milliseconds()
        });
        let completed_duration_milliseconds = if matches!(
            state,
            RideLifecycleState::Active | RideLifecycleState::Paused
        ) {
            0
        } else {
            last_monotonic_milliseconds.saturating_sub(created_at_milliseconds)
        };
        Self {
            state: Some(state),
            created_at_milliseconds,
            candidate_vehicle: metadata.candidate_vehicle,
            associated_vehicle: metadata.associated_vehicle,
            associated_at_milliseconds: metadata.associated_at_milliseconds,
            last_telemetry_at_milliseconds: metadata.last_telemetry_at_milliseconds,
            last_monotonic_milliseconds,
            paused_at_milliseconds: (state == RideLifecycleState::Paused)
                .then_some(last_monotonic_milliseconds),
            paused_duration_milliseconds: 0,
            completed_duration_milliseconds,
            segment_id: points
                .last()
                .map_or(RideMapSegmentId::new(0), |point| point.segment_id()),
            segment_started: false,
            first_point_sequence,
            summary,
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
        self.associated_vehicle
            .as_ref()
            .map(VehicleIdentity::as_str)
    }

    /// Returns the candidate identity retained for automatic association.
    #[must_use]
    pub fn candidate_vehicle(&self) -> Option<&str> {
        self.candidate_vehicle.as_ref().map(VehicleIdentity::as_str)
    }

    /// Returns the monotonic association timestamp.
    #[must_use]
    pub const fn associated_at_milliseconds(&self) -> Option<MonotonicMilliseconds> {
        self.associated_at_milliseconds
    }

    /// Returns the newest observed telemetry timestamp.
    #[must_use]
    pub const fn last_telemetry_at_milliseconds(&self) -> Option<MonotonicMilliseconds> {
        self.last_telemetry_at_milliseconds
    }

    /// Returns the current segment identity for the next accepted point.
    #[must_use]
    pub const fn current_segment_id(&self) -> RideMapSegmentId {
        self.segment_id
    }

    /// Returns the Rust-owned number of route segments admitted to the ride.
    #[must_use]
    pub const fn segment_count(&self) -> u64 {
        if self.summary.point_count().is_zero() {
            0
        } else {
            self.segment_id.value().saturating_add(1)
        }
    }

    /// Returns the projected canonical points.
    #[must_use]
    pub fn points(&self) -> &[RideMapPoint] {
        &self.points
    }

    /// Returns the sequence number of the first retained in-memory point.
    #[must_use]
    pub const fn first_point_sequence(&self) -> u64 {
        self.first_point_sequence
    }

    /// Returns the total number of accepted points, including points evicted from the live tail.
    #[must_use]
    pub const fn point_count(&self) -> u64 {
        self.summary.point_count().as_u64()
    }

    /// Returns the point count, distance, and duration projection.
    #[must_use]
    pub fn summary(&self) -> RideSummary {
        self.summary
    }

    /// Returns elapsed recording time using the latest accepted monotonic sample.
    #[must_use]
    pub const fn duration_milliseconds(&self) -> u64 {
        self.duration_milliseconds_at(self.last_monotonic_milliseconds)
    }

    /// Returns active elapsed recording time at the supplied monotonic timestamp.
    #[must_use]
    pub const fn duration_milliseconds_at(&self, at_milliseconds: MonotonicMilliseconds) -> u64 {
        match self.state {
            Some(RideLifecycleState::Active) => self.active_duration_at(at_milliseconds),
            Some(RideLifecycleState::Paused) => {
                let paused_at = match self.paused_at_milliseconds {
                    Some(value) => value,
                    None => at_milliseconds,
                };
                self.active_duration_at(paused_at)
            }
            _ => self.completed_duration_milliseconds,
        }
    }

    const fn active_duration_at(&self, at_milliseconds: MonotonicMilliseconds) -> u64 {
        at_milliseconds
            .saturating_sub(self.created_at_milliseconds)
            .saturating_sub(self.paused_duration_milliseconds)
    }

    /// Starts a new recording projection.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::Invalid`] when another recording is open.
    pub fn start(
        &mut self,
        at_milliseconds: MonotonicMilliseconds,
        candidate_vehicle: Option<VehicleIdentity>,
    ) -> Result<(), TransitionError> {
        if !matches!(
            self.state,
            None | Some(
                RideLifecycleState::Stopped
                    | RideLifecycleState::Interrupted
                    | RideLifecycleState::Saved
                    | RideLifecycleState::Discarded,
            )
        ) {
            return Err(TransitionError::Invalid);
        }
        self.state = Some(RideLifecycleState::Active);
        self.created_at_milliseconds = at_milliseconds;
        self.last_monotonic_milliseconds = at_milliseconds;
        self.candidate_vehicle = candidate_vehicle;
        self.associated_vehicle = None;
        self.associated_at_milliseconds = None;
        self.last_telemetry_at_milliseconds = None;
        self.points.clear();
        self.first_point_sequence = 0;
        self.summary = RideSummary::from_stored(RidePointCount::new(0), 0);
        self.segment_id = RideMapSegmentId::new(0);
        self.segment_started = true;
        self.paused_at_milliseconds = None;
        self.paused_duration_milliseconds = 0;
        self.completed_duration_milliseconds = 0;
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
        self.apply_transition_at(state, self.last_monotonic_milliseconds);
    }

    /// Applies a previously validated lifecycle state at a monotonic timestamp.
    pub fn apply_transition_at(
        &mut self,
        state: RideLifecycleState,
        at_milliseconds: MonotonicMilliseconds,
    ) {
        let at_milliseconds = at_milliseconds
            .max(self.created_at_milliseconds)
            .max(self.last_monotonic_milliseconds);
        match (self.state, state) {
            (Some(RideLifecycleState::Active), RideLifecycleState::Paused) => {
                self.paused_at_milliseconds = Some(at_milliseconds);
            }
            (Some(RideLifecycleState::Paused), RideLifecycleState::Active) => {
                if let Some(paused_at) = self.paused_at_milliseconds.take() {
                    self.paused_duration_milliseconds = self
                        .paused_duration_milliseconds
                        .saturating_add(at_milliseconds.saturating_sub(paused_at));
                }
            }
            (
                Some(RideLifecycleState::Active | RideLifecycleState::Paused),
                RideLifecycleState::Stopped | RideLifecycleState::Interrupted,
            ) => {
                self.completed_duration_milliseconds =
                    self.duration_milliseconds_at(at_milliseconds);
            }
            _ => {}
        }
        if self.state == Some(RideLifecycleState::Paused) && state == RideLifecycleState::Active {
            self.segment_id = self.segment_id.next();
            self.segment_started = true;
        }
        self.last_monotonic_milliseconds = self.last_monotonic_milliseconds.max(at_milliseconds);
        self.state = Some(state);
    }

    /// Reconciles one connected vehicle identity with the recording.
    #[must_use]
    pub fn observe_vehicle(
        &mut self,
        platform_identifier: &VehicleIdentity,
        at_milliseconds: MonotonicMilliseconds,
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
        if let Some(associated) = self.associated_vehicle.as_ref() {
            return if associated == platform_identifier {
                VehicleAssociation::AlreadyAssociated
            } else {
                VehicleAssociation::IdentityMismatch
            };
        }
        if let Some(candidate) = self.candidate_vehicle.as_ref()
            && candidate != platform_identifier
        {
            return VehicleAssociation::IdentityMismatch;
        }
        self.associated_vehicle = Some(platform_identifier.clone());
        self.candidate_vehicle = None;
        self.associated_at_milliseconds = Some(at_milliseconds);
        VehicleAssociation::Associated
    }

    /// Records confirmed vehicle telemetry without backfilling prior points.
    #[must_use]
    pub fn observe_telemetry(
        &mut self,
        at_milliseconds: MonotonicMilliseconds,
    ) -> TelemetryObservation {
        if !matches!(
            self.state,
            Some(RideLifecycleState::Active | RideLifecycleState::Paused)
        ) {
            return TelemetryObservation::RideNotOpen;
        }
        let Some(associated_at) = self.associated_at_milliseconds else {
            return TelemetryObservation::NotAssociated;
        };
        if at_milliseconds < associated_at || at_milliseconds < self.created_at_milliseconds {
            return TelemetryObservation::TimestampOutOfOrder;
        }
        if self.last_telemetry_at_milliseconds == Some(at_milliseconds) {
            return TelemetryObservation::AlreadyObserved;
        }
        if self
            .last_telemetry_at_milliseconds
            .is_some_and(|previous| at_milliseconds < previous)
        {
            return TelemetryObservation::TimestampOutOfOrder;
        }
        self.last_telemetry_at_milliseconds = Some(at_milliseconds);
        TelemetryObservation::Observed
    }

    /// Returns the telemetry provenance for a point at the supplied monotonic time.
    #[must_use]
    pub fn telemetry_state_at(
        &self,
        at_milliseconds: MonotonicMilliseconds,
    ) -> RouteTelemetryState {
        if self.associated_vehicle.is_none() {
            return RouteTelemetryState::GpsOnly;
        }
        let Some(last) = self.last_telemetry_at_milliseconds else {
            return RouteTelemetryState::AssociatedNoTelemetry;
        };
        if at_milliseconds < last {
            return RouteTelemetryState::AssociatedNoTelemetry;
        }
        if at_milliseconds.saturating_sub(last) > TELEMETRY_FRESHNESS_MILLISECONDS {
            RouteTelemetryState::AssociatedStale
        } else {
            RouteTelemetryState::AssociatedFresh
        }
    }

    /// Checks a candidate sample against the latest accepted sample.
    #[must_use]
    pub fn check_sample(&self, sample: &LocationSample) -> LocationAdmission {
        let previous = self.points.last().map(|point| point.sample());
        if sample
            .horizontal_accuracy_millimetres()
            .is_some_and(|accuracy| accuracy > MAX_HORIZONTAL_ACCURACY_MILLIMETRES)
        {
            return LocationAdmission::AccuracyTooLow;
        }
        let admission = sample.admission(previous.as_ref());
        if admission != LocationAdmission::Accepted {
            return admission;
        }
        let Some(previous) = previous else {
            return LocationAdmission::Accepted;
        };
        let elapsed = sample
            .monotonic_milliseconds()
            .saturating_sub(previous.monotonic_milliseconds());
        if elapsed <= MAX_GAP_MILLISECONDS {
            let distance = distance_between_millimetres(previous, *sample);
            if u128::from(distance) * 1_000
                > u128::from(MAX_IMPLIED_SPEED_MILLIMETRES_PER_SECOND) * u128::from(elapsed)
            {
                return LocationAdmission::UnrealisticJump;
            }
        }
        LocationAdmission::Accepted
    }

    /// Records a sample after durable storage has accepted it.
    pub fn record_sample(&mut self, sample: LocationSample) -> bool {
        let gap_started = self.points.last().is_some_and(|previous| {
            sample
                .monotonic_milliseconds()
                .saturating_sub(previous.sample().monotonic_milliseconds())
                > MAX_GAP_MILLISECONDS
        });
        let segment_started = self.segment_started || gap_started;
        if gap_started {
            self.segment_id = self.segment_id.next();
        }
        let distance = if segment_started {
            0
        } else {
            self.points.last().map_or(0, |previous| {
                distance_between_millimetres(previous.sample(), sample)
            })
        };
        self.summary = RideSummary::from_stored(
            self.summary
                .point_count()
                .saturating_add(RidePointCount::new(1)),
            self.summary.distance_millimetres().saturating_add(distance),
        );
        self.last_monotonic_milliseconds = sample.monotonic_milliseconds();
        self.points.push(RideMapPoint::new(
            sample,
            self.segment_id,
            self.telemetry_state_at(sample.monotonic_milliseconds()),
        ));
        if self.points.len() > MAX_LIVE_ROUTE_POINTS {
            let excess = self.points.len() - MAX_LIVE_ROUTE_POINTS;
            self.points.drain(..excess);
        }
        self.first_point_sequence = self
            .summary
            .point_count()
            .saturating_sub(RidePointCount::from_usize(self.points.len()))
            .as_u64();
        self.segment_started = false;
        segment_started
    }
}

#[cfg(test)]
mod tests {
    use super::{MonotonicMilliseconds, RideMapRecorder, VehicleAssociation};
    use crate::{
        Coordinate, LocationAdmission, LocationSample, LocationSource, RideEvent,
        RideLifecycleState, VehicleIdentity, WallClockUnixMilliseconds,
    };

    fn identity(value: &str) -> VehicleIdentity {
        VehicleIdentity::new(value).expect("valid vehicle identity")
    }

    fn monotonic(value: u64) -> MonotonicMilliseconds {
        MonotonicMilliseconds::new(value)
    }

    fn sample(monotonic_ms: u64, latitude: f64) -> LocationSample {
        LocationSample::new(
            Coordinate::from_degrees(latitude, -105.0).expect("valid coordinate"),
            monotonic(monotonic_ms),
            WallClockUnixMilliseconds::new(1_700_000_000_000 + monotonic_ms),
            None,
            LocationSource::Live,
        )
    }

    #[test]
    fn recorder_owns_transitions_association_and_sample_admission() {
        let mut recorder = RideMapRecorder::new();
        recorder
            .start(monotonic(1_000), Some(identity("pev-1")))
            .expect("starts");
        assert_eq!(
            recorder.validate_transition(RideEvent::Pause),
            Ok(RideLifecycleState::Paused)
        );
        assert_eq!(
            recorder.observe_vehicle(&identity("pev-1"), monotonic(1_001)),
            VehicleAssociation::Associated
        );

        let first = sample(1_001, 40.0);
        assert_eq!(recorder.check_sample(&first), LocationAdmission::Accepted);
        recorder.record_sample(first);
        assert_eq!(recorder.segment_count(), 1);
        assert_eq!(recorder.check_sample(&first), LocationAdmission::Duplicate);
        assert_eq!(
            recorder.observe_vehicle(&identity("pev-1"), monotonic(1_002)),
            VehicleAssociation::AlreadyAssociated
        );
        recorder.apply_transition(RideLifecycleState::Active);
        assert_eq!(recorder.current_segment_id().value(), 0);
        recorder.apply_transition(RideLifecycleState::Paused);
        recorder.apply_transition(RideLifecycleState::Active);
        assert_eq!(recorder.current_segment_id().value(), 1);
        assert!(recorder.record_sample(sample(1_003, 40.001)));
        assert_eq!(recorder.segment_count(), 2);
        assert_eq!(
            recorder
                .points()
                .last()
                .map(|point| point.segment_id().value()),
            Some(1)
        );
    }

    #[test]
    fn duration_ticks_without_location_and_excludes_paused_time() {
        let mut recorder = RideMapRecorder::new();
        recorder.start(monotonic(1_000), None).expect("starts");
        assert_eq!(recorder.duration_milliseconds_at(monotonic(5_000)), 4_000);

        recorder.apply_transition_at(RideLifecycleState::Paused, monotonic(5_000));
        assert_eq!(recorder.duration_milliseconds_at(monotonic(10_000)), 4_000);

        recorder.apply_transition_at(RideLifecycleState::Active, monotonic(12_000));
        assert_eq!(recorder.duration_milliseconds_at(monotonic(15_000)), 7_000);

        recorder.apply_transition_at(RideLifecycleState::Stopped, monotonic(17_000));
        assert_eq!(recorder.duration_milliseconds_at(monotonic(20_000)), 9_000);
    }

    #[test]
    fn association_accepts_a_vehicle_found_during_recording_and_stays_stable() {
        let mut no_candidate = RideMapRecorder::new();
        no_candidate.start(monotonic(1_000), None).expect("starts");
        assert_eq!(
            no_candidate.observe_vehicle(&identity("pev-1"), monotonic(1_001)),
            VehicleAssociation::Associated
        );
        assert_eq!(no_candidate.associated_vehicle(), Some("pev-1"));

        let mut associated = RideMapRecorder::new();
        associated
            .start(monotonic(1_000), Some(identity("pev-1")))
            .expect("starts");
        assert_eq!(
            associated.observe_vehicle(&identity("pev-1"), monotonic(1_001)),
            VehicleAssociation::Associated
        );
        assert_eq!(
            associated.observe_vehicle(&identity("pev-2"), monotonic(1_002)),
            VehicleAssociation::IdentityMismatch
        );
        assert_eq!(associated.associated_vehicle(), Some("pev-1"));
    }

    #[test]
    fn changed_coordinates_at_the_same_monotonic_time_are_out_of_order() {
        let mut recorder = RideMapRecorder::new();
        recorder.start(monotonic(1_000), None).expect("starts");
        let first = sample(1_001, 40.0);
        recorder.record_sample(first);
        let changed = sample(1_001, 40.001);
        assert_eq!(
            recorder.check_sample(&changed),
            LocationAdmission::OutOfOrder
        );
    }

    #[test]
    fn long_location_gap_starts_a_new_segment_without_false_distance() {
        let mut recorder = RideMapRecorder::new();
        recorder.start(monotonic(1_000), None).expect("starts");
        recorder.record_sample(sample(1_001, 40.0));
        let second = sample(40_000, 40.001);
        assert_eq!(recorder.check_sample(&second), LocationAdmission::Accepted);
        assert!(recorder.record_sample(second));
        assert_eq!(recorder.current_segment_id().value(), 1);
        assert_eq!(recorder.segment_count(), 2);
        assert_eq!(recorder.summary().distance_millimetres(), 0);
    }

    #[test]
    fn live_projection_retains_tail_without_losing_summary_or_sequence() {
        let mut recorder = RideMapRecorder::new();
        recorder.start(monotonic(1_000), None).expect("starts");
        for index in 0..(u32::try_from(super::MAX_LIVE_ROUTE_POINTS).expect("bounded") + 2) {
            let monotonic = 1_001 + u64::from(index);
            let latitude = 40.0 + (f64::from(index) * 0.000_000_01);
            let point = sample(monotonic, latitude);
            assert_eq!(recorder.check_sample(&point), LocationAdmission::Accepted);
            recorder.record_sample(point);
        }

        assert_eq!(recorder.points().len(), super::MAX_LIVE_ROUTE_POINTS);
        assert_eq!(
            recorder.point_count(),
            (super::MAX_LIVE_ROUTE_POINTS + 2) as u64
        );
        assert_eq!(recorder.first_point_sequence(), 2);
        assert_eq!(
            recorder
                .points()
                .first()
                .map(|point| point.sample().monotonic_milliseconds()),
            Some(monotonic(1_003))
        );
    }
}
