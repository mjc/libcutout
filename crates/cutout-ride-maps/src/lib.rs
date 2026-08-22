#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Rust-owned ride recording and map-display primitives.

use thiserror::Error;
use uuid::Uuid;

mod storage;

pub use storage::{
    RideMapDatabase, RideMapDatabaseOpenError, RideMapStore, RideMapStoreError, StoredRideSummary,
};

const EARTH_RADIUS_METERS: f64 = 6_371_000.0;
const MAX_VEHICLE_ID_BYTES: usize = 256;
const MAX_POINT_BATCH: usize = 1_024;

/// Monotonic host time in milliseconds.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicMilliseconds(u64);

impl MonotonicMilliseconds {
    /// Creates a timestamp from milliseconds.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the timestamp as milliseconds.
    #[must_use]
    pub const fn as_milliseconds(self) -> u64 {
        self.0
    }
}

/// A validated latitude in degrees.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Latitude(f64);

impl Latitude {
    /// Returns the latitude in degrees.
    #[must_use]
    pub const fn as_degrees(self) -> f64 {
        self.0
    }
}

/// A validated longitude in degrees.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Longitude(f64);

impl Longitude {
    /// Returns the longitude in degrees.
    #[must_use]
    pub const fn as_degrees(self) -> f64 {
        self.0
    }
}

/// Stable PEV identity used for automatic ride association.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VehicleIdentity(String);

impl VehicleIdentity {
    /// Validates and creates an identity from the platform's stable identifier.
    ///
    /// # Errors
    ///
    /// Returns [`VehicleIdentityError`] for blank or oversized identifiers.
    pub fn new(value: impl Into<String>) -> Result<Self, VehicleIdentityError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(VehicleIdentityError::Blank);
        }
        if value.len() > MAX_VEHICLE_ID_BYTES {
            return Err(VehicleIdentityError::TooLong);
        }
        Ok(Self(value))
    }

    /// Returns the platform identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validation failure for a PEV identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum VehicleIdentityError {
    /// The identity was empty or whitespace-only.
    #[error("vehicle identity is blank")]
    Blank,
    /// The identity exceeded the bounded input size.
    #[error("vehicle identity is too long")]
    TooLong,
}

/// A location observation received from the platform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocationSample {
    monotonic_at: MonotonicMilliseconds,
    wall_clock_unix_ms: u64,
    latitude: Latitude,
    longitude: Longitude,
    horizontal_accuracy_meters: f64,
}

impl LocationSample {
    /// Validates and creates a location observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for invalid coordinates, timestamps, or accuracy.
    pub fn new(
        monotonic_at: MonotonicMilliseconds,
        wall_clock_unix_ms: u64,
        latitude: f64,
        longitude: f64,
        horizontal_accuracy_meters: f64,
    ) -> Result<Self, LocationSampleError> {
        if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
            return Err(LocationSampleError::InvalidLatitude);
        }
        if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
            return Err(LocationSampleError::InvalidLongitude);
        }
        if wall_clock_unix_ms == 0 {
            return Err(LocationSampleError::InvalidWallClock);
        }
        if !horizontal_accuracy_meters.is_finite() || horizontal_accuracy_meters < 0.0 {
            return Err(LocationSampleError::InvalidAccuracy);
        }
        Ok(Self {
            monotonic_at,
            wall_clock_unix_ms,
            latitude: Latitude(latitude),
            longitude: Longitude(longitude),
            horizontal_accuracy_meters,
        })
    }

    /// Returns the monotonic observation time.
    #[must_use]
    pub const fn monotonic_at(self) -> MonotonicMilliseconds {
        self.monotonic_at
    }

    /// Returns the wall-clock observation time.
    #[must_use]
    pub const fn wall_clock_unix_ms(self) -> u64 {
        self.wall_clock_unix_ms
    }

    /// Returns the validated latitude.
    #[must_use]
    pub const fn latitude(self) -> Latitude {
        self.latitude
    }

    /// Returns the validated longitude.
    #[must_use]
    pub const fn longitude(self) -> Longitude {
        self.longitude
    }

    /// Returns the horizontal accuracy in meters.
    #[must_use]
    pub const fn horizontal_accuracy_meters(self) -> f64 {
        self.horizontal_accuracy_meters
    }
}

/// Validation failure for a platform location observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum LocationSampleError {
    /// Latitude was not finite or was outside the WGS84 range.
    #[error("latitude is invalid")]
    InvalidLatitude,
    /// Longitude was not finite or was outside the WGS84 range.
    #[error("longitude is invalid")]
    InvalidLongitude,
    /// Wall-clock timestamp was not populated.
    #[error("wall-clock timestamp is invalid")]
    InvalidWallClock,
    /// Horizontal accuracy was not finite or was negative.
    #[error("horizontal accuracy is invalid")]
    InvalidAccuracy,
}

/// Rust-owned thresholds for admitting location observations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocationAdmissionPolicy {
    /// Maximum accepted horizontal accuracy in meters.
    pub max_horizontal_accuracy_meters: f64,
    /// A gap longer than this starts a new route segment.
    pub max_gap_milliseconds: u64,
    /// Maximum implied speed for a point without a reported speed field.
    pub max_implied_speed_meters_per_second: f64,
}

impl Default for LocationAdmissionPolicy {
    fn default() -> Self {
        Self {
            max_horizontal_accuracy_meters: 100.0,
            max_gap_milliseconds: 30_000,
            max_implied_speed_meters_per_second: 100.0,
        }
    }
}

/// Current mutable phase of a ride recording.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideState {
    /// Points are admitted.
    Recording,
    /// The ride remains open but points are ignored.
    Paused,
    /// The ride is terminal and cannot accept more points.
    Stopped,
    /// The stopped ride was durably saved.
    Saved,
    /// The stopped ride was explicitly discarded.
    Discarded,
}

/// A validated point retained in one route segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoutePoint {
    sequence: u64,
    sample: LocationSample,
    segment_id: u64,
}

impl RoutePoint {
    /// Returns the monotonic cursor sequence for this point.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns the source location sample.
    #[must_use]
    pub const fn sample(self) -> LocationSample {
        self.sample
    }

    /// Returns the Rust-owned segment identifier.
    #[must_use]
    pub const fn segment_id(self) -> u64 {
        self.segment_id
    }
}

/// One contiguous admitted route segment.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteSegment {
    id: u64,
    points: Vec<RoutePoint>,
}

impl RouteSegment {
    /// Returns the segment identifier.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns the admitted points in order.
    #[must_use]
    pub fn points(&self) -> &[RoutePoint] {
        &self.points
    }
}

/// Why a location was rejected at the Rust boundary.
#[derive(Clone, Copy, Debug, PartialEq, Error)]
pub enum LocationRejection {
    /// The fix is less accurate than the configured threshold.
    #[error("horizontal accuracy exceeds the admission threshold")]
    AccuracyTooLow,
    /// The observation is not newer than the last admitted point.
    #[error("location observation is out of order")]
    OutOfOrder,
    /// The observation repeats the last coordinate and timestamp.
    #[error("location observation is a duplicate")]
    Duplicate,
    /// The implied travel speed exceeds the configured threshold.
    #[error("location jump exceeds the implied-speed threshold")]
    UnrealisticJump,
}

/// Why an observation was ignored without mutating the route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationIgnoredReason {
    /// The ride is paused.
    Paused,
    /// The ride has stopped.
    Stopped,
    /// The ride has been saved.
    Saved,
    /// The ride has been discarded.
    Discarded,
}

/// Result of attempting to admit a location observation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoutePointDecision {
    /// The point was retained; `segment_started` marks a gap boundary.
    Accepted {
        /// The retained point.
        point: RoutePoint,
        /// Whether this point starts a new segment.
        segment_started: bool,
    },
    /// The point was rejected without route mutation.
    Rejected(LocationRejection),
    /// The point was valid input but the ride was not admitting points.
    Ignored(LocationIgnoredReason),
}

/// A bounded route batch returned by a cursor query.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutePointBatch {
    points: Vec<RoutePoint>,
    next_cursor: u64,
    has_more: bool,
}

impl RoutePointBatch {
    /// Returns the bounded point list.
    #[must_use]
    pub fn points(&self) -> &[RoutePoint] {
        &self.points
    }

    /// Returns the cursor to use for the next query.
    #[must_use]
    pub const fn next_cursor(&self) -> u64 {
        self.next_cursor
    }

    /// Returns whether another bounded batch is available.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

/// Cumulative facts for the current ride.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RideSummary {
    point_count: u64,
    distance_meters: f64,
    first_at: Option<MonotonicMilliseconds>,
    last_at: Option<MonotonicMilliseconds>,
}

impl RideSummary {
    /// Returns the number of admitted points.
    #[must_use]
    pub const fn point_count(self) -> u64 {
        self.point_count
    }

    /// Returns cumulative great-circle distance in meters.
    #[must_use]
    pub const fn distance_meters(self) -> f64 {
        self.distance_meters
    }

    /// Returns elapsed monotonic recording time in milliseconds.
    #[must_use]
    pub const fn duration_milliseconds(self) -> u64 {
        match (self.first_at, self.last_at) {
            (Some(first), Some(last)) => last
                .as_milliseconds()
                .saturating_sub(first.as_milliseconds()),
            _ => 0,
        }
    }
}

/// A Rust-owned in-memory ride recording.
#[derive(Clone, Debug)]
pub struct RideRecording {
    ride_id: Uuid,
    state: RideState,
    policy: LocationAdmissionPolicy,
    segments: Vec<RouteSegment>,
    summary: RideSummary,
    last_point: Option<RoutePoint>,
    next_sequence: u64,
    force_new_segment: bool,
    candidate_vehicle: Option<VehicleIdentity>,
    associated_vehicle: Option<VehicleIdentity>,
    associated_at: Option<MonotonicMilliseconds>,
}

impl RideRecording {
    /// Starts a GPS-only ride and snapshots the last connected PEV candidate, if any.
    #[must_use]
    pub fn start_gps_only(
        _started_at: MonotonicMilliseconds,
        candidate_vehicle: Option<VehicleIdentity>,
        policy: LocationAdmissionPolicy,
    ) -> Self {
        Self {
            ride_id: Uuid::new_v4(),
            state: RideState::Recording,
            policy,
            segments: Vec::new(),
            summary: RideSummary {
                point_count: 0,
                distance_meters: 0.0,
                first_at: None,
                last_at: None,
            },
            last_point: None,
            next_sequence: 0,
            force_new_segment: true,
            candidate_vehicle,
            associated_vehicle: None,
            associated_at: None,
        }
    }

    pub(crate) fn from_persisted(
        ride_id: Uuid,
        state: RideState,
        associated_vehicle: Option<VehicleIdentity>,
        points: Vec<RoutePoint>,
    ) -> Result<Self, String> {
        let mut segments = Vec::new();
        let mut previous = None;
        let mut total_distance_meters = 0.0;
        let mut first_at = None;
        let mut last_at = None;
        let mut next_sequence = 0;
        let mut point_count = 0_u64;

        for point in points {
            if point.sequence == 0
                || previous.is_some_and(|previous: RoutePoint| {
                    point.sequence <= previous.sequence
                        || point.sample.monotonic_at() <= previous.sample.monotonic_at()
                })
            {
                return Err("persisted route points are not strictly ordered".to_owned());
            }
            if let Some(previous_point) = previous
                && point.segment_id == previous_point.segment_id
            {
                total_distance_meters += distance_meters(previous_point.sample, point.sample);
            }
            if segments
                .last()
                .is_none_or(|segment: &RouteSegment| segment.id != point.segment_id)
            {
                segments.push(RouteSegment {
                    id: point.segment_id,
                    points: Vec::new(),
                });
            }
            let Some(segment) = segments.last_mut() else {
                return Err("persisted route segment could not be reconstructed".to_owned());
            };
            segment.points.push(point);
            first_at.get_or_insert(point.sample.monotonic_at());
            last_at = Some(point.sample.monotonic_at());
            next_sequence = point.sequence;
            point_count = point_count.saturating_add(1);
            previous = Some(point);
        }

        Ok(Self {
            ride_id,
            state,
            policy: LocationAdmissionPolicy::default(),
            segments,
            summary: RideSummary {
                point_count,
                distance_meters: total_distance_meters,
                first_at,
                last_at,
            },
            last_point: previous,
            next_sequence,
            force_new_segment: true,
            candidate_vehicle: None,
            associated_vehicle,
            associated_at: None,
        })
    }

    /// Returns the stable ride identifier.
    #[must_use]
    pub const fn ride_id(&self) -> Uuid {
        self.ride_id
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> RideState {
        self.state
    }

    /// Returns the cumulative summary.
    #[must_use]
    pub const fn summary(&self) -> RideSummary {
        self.summary
    }

    /// Returns all route segments in their Rust-owned order.
    #[must_use]
    pub fn segments(&self) -> &[RouteSegment] {
        &self.segments
    }

    /// Returns the vehicle associated after a confirmed identity match.
    #[must_use]
    pub fn associated_vehicle(&self) -> Option<&VehicleIdentity> {
        self.associated_vehicle.as_ref()
    }

    /// Returns when the vehicle association was confirmed.
    #[must_use]
    pub const fn associated_at(&self) -> Option<MonotonicMilliseconds> {
        self.associated_at
    }

    /// Pauses point admission without ending the logical ride.
    pub fn pause(&mut self) -> bool {
        if self.state == RideState::Recording {
            self.state = RideState::Paused;
            self.force_new_segment = true;
            true
        } else {
            false
        }
    }

    /// Resumes point admission for the same logical ride.
    pub fn resume(&mut self) -> bool {
        if self.state == RideState::Paused {
            self.state = RideState::Recording;
            true
        } else {
            false
        }
    }

    /// Stops the ride permanently.
    pub fn stop(&mut self) -> bool {
        if matches!(self.state, RideState::Recording | RideState::Paused) {
            self.state = RideState::Stopped;
            true
        } else {
            false
        }
    }

    /// Finalizes a stopped ride as durably saved.
    pub fn save(&mut self) -> bool {
        if self.state == RideState::Stopped {
            self.state = RideState::Saved;
            true
        } else {
            false
        }
    }

    /// Marks a stopped ride as explicitly discarded.
    pub fn discard(&mut self) -> bool {
        if self.state == RideState::Stopped {
            self.state = RideState::Discarded;
            true
        } else {
            false
        }
    }

    /// Associates the snapshotted PEV after a confirmed connection event.
    ///
    /// Returns `true` only for the first matching association while the ride is open.
    pub fn observe_vehicle_connection(
        &mut self,
        identity: &VehicleIdentity,
        at: MonotonicMilliseconds,
    ) -> bool {
        if !matches!(self.state, RideState::Recording | RideState::Paused)
            || self.associated_vehicle.is_some()
            || self.candidate_vehicle.as_ref() != Some(identity)
        {
            return false;
        }
        self.associated_vehicle = Some(identity.clone());
        self.associated_at = Some(at);
        true
    }

    /// Attempts to admit one platform observation into the current route.
    pub fn append_location(&mut self, sample: LocationSample) -> RoutePointDecision {
        match self.state {
            RideState::Paused => return RoutePointDecision::Ignored(LocationIgnoredReason::Paused),
            RideState::Stopped => {
                return RoutePointDecision::Ignored(LocationIgnoredReason::Stopped);
            }
            RideState::Saved => return RoutePointDecision::Ignored(LocationIgnoredReason::Saved),
            RideState::Discarded => {
                return RoutePointDecision::Ignored(LocationIgnoredReason::Discarded);
            }
            RideState::Recording => {}
        }

        if sample.horizontal_accuracy_meters() > self.policy.max_horizontal_accuracy_meters {
            return RoutePointDecision::Rejected(LocationRejection::AccuracyTooLow);
        }

        let gap = self.last_point.map(|point| {
            sample
                .monotonic_at()
                .as_milliseconds()
                .saturating_sub(point.sample.monotonic_at().as_milliseconds())
        });
        if let Some(previous) = self.last_point {
            let current_at = sample.monotonic_at().as_milliseconds();
            let previous_at = previous.sample.monotonic_at().as_milliseconds();
            if current_at <= previous_at {
                if current_at == previous_at
                    && sample.latitude() == previous.sample.latitude()
                    && sample.longitude() == previous.sample.longitude()
                {
                    return RoutePointDecision::Rejected(LocationRejection::Duplicate);
                }
                return RoutePointDecision::Rejected(LocationRejection::OutOfOrder);
            }
            if gap.is_some_and(|value| value <= self.policy.max_gap_milliseconds) {
                let elapsed_milliseconds = current_at.saturating_sub(previous_at);
                let elapsed_seconds = match u32::try_from(elapsed_milliseconds) {
                    Ok(value) => f64::from(value) / 1_000.0,
                    Err(_) => f64::from(u32::MAX) / 1_000.0,
                };
                let implied_speed = distance_meters(previous.sample, sample) / elapsed_seconds;
                if implied_speed > self.policy.max_implied_speed_meters_per_second {
                    return RoutePointDecision::Rejected(LocationRejection::UnrealisticJump);
                }
            }
        }

        self.next_sequence = self.next_sequence.saturating_add(1);
        let segment_started = self.force_new_segment
            || gap.is_some_and(|value| value > self.policy.max_gap_milliseconds);
        if segment_started {
            self.segments.push(RouteSegment {
                id: u64::try_from(self.segments.len()).unwrap_or(u64::MAX),
                points: Vec::new(),
            });
        }
        let segment_id = u64::try_from(self.segments.len().saturating_sub(1)).unwrap_or(u64::MAX);
        let point = RoutePoint {
            sequence: self.next_sequence,
            sample,
            segment_id,
        };
        if let Some(segment) = self.segments.last_mut() {
            segment.points.push(point);
        }
        if !segment_started {
            if let Some(previous) = self.last_point {
                self.summary.distance_meters += distance_meters(previous.sample, sample);
            }
        }
        self.summary.point_count = self.summary.point_count.saturating_add(1);
        if self.summary.first_at.is_none() {
            self.summary.first_at = Some(sample.monotonic_at());
        }
        self.summary.last_at = Some(sample.monotonic_at());
        self.last_point = Some(point);
        self.force_new_segment = false;
        RoutePointDecision::Accepted {
            point,
            segment_started,
        }
    }

    /// Returns a bounded point batch after the supplied sequence cursor.
    #[must_use]
    pub fn points_after(&self, after_sequence: u64, limit: usize) -> RoutePointBatch {
        let capped_limit = limit.min(MAX_POINT_BATCH);
        let points: Vec<RoutePoint> = self
            .segments
            .iter()
            .flat_map(|segment| segment.points.iter().copied())
            .filter(|point| point.sequence > after_sequence)
            .take(capped_limit)
            .collect();
        let next_cursor = points.last().map_or(after_sequence, |point| point.sequence);
        let has_more = self
            .segments
            .iter()
            .flat_map(|segment| segment.points.iter())
            .any(|point| point.sequence > next_cursor);
        RoutePointBatch {
            points,
            next_cursor,
            has_more,
        }
    }
}

fn distance_meters(previous: LocationSample, current: LocationSample) -> f64 {
    let lat1 = previous.latitude().as_degrees().to_radians();
    let lat2 = current.latitude().as_degrees().to_radians();
    let delta_lat = lat2 - lat1;
    let delta_lon =
        (current.longitude().as_degrees() - previous.longitude().as_degrees()).to_radians();
    let a =
        (delta_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
    let central_angle = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_METERS * central_angle
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{
        LocationAdmissionPolicy, LocationSample, MonotonicMilliseconds, RideMapDatabase,
        RideMapDatabaseOpenError, RideMapStore, RideRecording, RideState, RoutePointDecision,
        VehicleIdentity,
    };

    fn sample(at: u64, latitude: f64, longitude: f64) -> LocationSample {
        LocationSample::new(
            MonotonicMilliseconds::new(at),
            at.saturating_add(1_700_000_000_000),
            latitude,
            longitude,
            5.0,
        )
        .expect("fixture is valid")
    }

    #[test]
    fn gps_only_recording_admits_points_and_reports_summary() {
        let mut ride = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            None,
            LocationAdmissionPolicy::default(),
        );

        assert_eq!(ride.state(), RideState::Recording);
        assert!(matches!(
            ride.append_location(sample(100, 39.7392, -104.9903)),
            RoutePointDecision::Accepted { .. }
        ));
        assert!(matches!(
            ride.append_location(sample(1_100, 39.7393, -104.9902)),
            RoutePointDecision::Accepted { .. }
        ));
        assert_eq!(ride.summary().point_count(), 2);
        assert!(ride.summary().distance_meters() > 0.0);
    }

    #[test]
    fn invalid_and_out_of_order_points_are_rejected_without_mutation() {
        let mut ride = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            None,
            LocationAdmissionPolicy::default(),
        );

        assert!(matches!(
            ride.append_location(sample(100, 39.7392, -104.9903)),
            RoutePointDecision::Accepted { .. }
        ));
        let before = ride.summary();
        assert!(matches!(
            ride.append_location(sample(99, 39.7393, -104.9902)),
            RoutePointDecision::Rejected(_)
        ));
        assert_eq!(ride.summary(), before);
    }

    #[test]
    fn large_gap_starts_a_new_segment_and_cursor_batches_are_bounded() {
        let policy = LocationAdmissionPolicy {
            max_gap_milliseconds: 1_000,
            ..LocationAdmissionPolicy::default()
        };
        let mut ride = RideRecording::start_gps_only(MonotonicMilliseconds::new(100), None, policy);
        ride.append_location(sample(100, 39.7392, -104.9903));
        let decision = ride.append_location(sample(2_000, 39.7393, -104.9902));

        assert!(matches!(
            decision,
            RoutePointDecision::Accepted {
                segment_started: true,
                ..
            }
        ));
        let batch = ride.points_after(0, 1);
        assert_eq!(batch.points().len(), 1);
        assert!(batch.has_more());
    }

    #[test]
    fn matching_last_pev_associates_without_changing_ride_identity() {
        let vehicle = VehicleIdentity::new("pev-1").expect("fixture identity");
        let mut ride = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            Some(vehicle.clone()),
            LocationAdmissionPolicy::default(),
        );
        let ride_id = ride.ride_id();

        assert!(ride.associated_vehicle().is_none());
        assert!(ride.observe_vehicle_connection(&vehicle, MonotonicMilliseconds::new(200)));
        assert_eq!(ride.ride_id(), ride_id);
        assert_eq!(ride.associated_vehicle(), Some(&vehicle));
        assert_eq!(ride.associated_at(), Some(MonotonicMilliseconds::new(200)));
        assert!(!ride.observe_vehicle_connection(&vehicle, MonotonicMilliseconds::new(201)));
    }

    #[test]
    fn gaps_and_pauses_do_not_add_false_distance_between_segments() {
        let policy = LocationAdmissionPolicy {
            max_gap_milliseconds: 1_000,
            ..LocationAdmissionPolicy::default()
        };
        let mut ride = RideRecording::start_gps_only(MonotonicMilliseconds::new(100), None, policy);
        ride.append_location(sample(100, 39.7392, -104.9903));
        ride.append_location(sample(200, 39.7393, -104.9902));
        let distance_before_gap = ride.summary().distance_meters();
        ride.append_location(sample(2_000, 40.0000, -105.0000));
        assert!((ride.summary().distance_meters() - distance_before_gap).abs() < f64::EPSILON);
        assert!(ride.pause());
        assert!(ride.resume());
        ride.append_location(sample(2_100, 40.1000, -105.1000));
        assert!((ride.summary().distance_meters() - distance_before_gap).abs() < f64::EPSILON);
    }

    #[test]
    fn sqlite_store_round_trips_summaries_and_bounded_points() {
        let mut store = RideMapStore::open_in_memory().expect("store opens");
        let mut ride = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            Some(VehicleIdentity::new("pev-1").expect("fixture identity")),
            LocationAdmissionPolicy::default(),
        );
        ride.append_location(sample(100, 39.7392, -104.9903));
        ride.append_location(sample(1_100, 39.7393, -104.9902));
        ride.observe_vehicle_connection(
            &VehicleIdentity::new("pev-1").expect("fixture identity"),
            MonotonicMilliseconds::new(1_200),
        );
        assert!(ride.stop());

        store.save_recording(&ride).expect("recording saves");
        let summaries = store.list_summaries(10).expect("summaries query");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].ride_id(), ride.ride_id());
        assert_eq!(summaries[0].point_count(), 2);
        assert_eq!(summaries[0].associated_vehicle(), Some("pev-1"));

        let points = store
            .points_after(ride.ride_id(), 0, 1)
            .expect("points query");
        assert_eq!(points.points().len(), 1);
        assert!(points.has_more());
    }

    #[test]
    fn stopped_rides_can_be_saved_or_discarded_but_not_recorded_again() {
        let mut saved = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            None,
            LocationAdmissionPolicy::default(),
        );
        assert!(saved.stop());
        assert!(saved.save());
        assert_eq!(saved.state(), RideState::Saved);
        assert!(!saved.save());
        assert!(matches!(
            saved.append_location(sample(200, 39.7393, -104.9902)),
            RoutePointDecision::Ignored(_)
        ));

        let mut discarded = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            None,
            LocationAdmissionPolicy::default(),
        );
        assert!(discarded.stop());
        assert!(discarded.discard());
        assert_eq!(discarded.state(), RideState::Discarded);
        assert!(!discarded.discard());
    }

    #[test]
    fn sqlite_store_reopens_a_saved_ride_from_a_file() {
        let path = std::env::temp_dir().join(format!("cutout-ride-map-{}.sqlite", Uuid::new_v4()));
        let mut ride = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            None,
            LocationAdmissionPolicy::default(),
        );
        ride.append_location(sample(100, 39.7392, -104.9903));
        assert!(ride.stop());
        assert!(ride.save());

        {
            let mut store = RideMapStore::open(&path).expect("file store opens");
            store.save_recording(&ride).expect("ride saves");
        }
        let reopened = RideMapStore::open(&path).expect("file store reopens");
        let summaries = reopened.list_summaries(10).expect("history queries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].state(), RideState::Saved);
        std::fs::remove_file(path).expect("fixture database is removable");
    }

    #[test]
    fn sqlite_store_recovers_latest_open_ride_after_reopen() {
        let path = std::env::temp_dir().join(format!(
            "cutout-ride-map-recovery-{}.sqlite",
            Uuid::new_v4()
        ));
        let vehicle = VehicleIdentity::new("pev-recovered").expect("fixture identity");
        let mut ride = RideRecording::start_gps_only(
            MonotonicMilliseconds::new(100),
            Some(vehicle.clone()),
            LocationAdmissionPolicy::default(),
        );
        ride.append_location(sample(100, 39.7392, -104.9903));
        ride.observe_vehicle_connection(&vehicle, MonotonicMilliseconds::new(200));

        {
            let mut store = RideMapStore::open(&path).expect("file store opens");
            store.save_recording(&ride).expect("open ride checkpoints");
        }

        let reopened = RideMapStore::open(&path).expect("file store reopens");
        let recovered = reopened
            .recover_open_recording()
            .expect("recovery query succeeds")
            .expect("open ride is recoverable");
        assert_eq!(recovered.ride_id(), ride.ride_id());
        assert_eq!(recovered.state(), RideState::Recording);
        assert_eq!(recovered.summary().point_count(), 1);
        assert_eq!(
            recovered.associated_vehicle().map(VehicleIdentity::as_str),
            Some("pev-recovered")
        );
        assert_eq!(recovered.points_after(0, 10).points().len(), 1);
        std::fs::remove_file(path).expect("fixture database is removable");
    }

    #[test]
    fn production_database_handles_share_one_rust_worker() {
        let path =
            std::env::temp_dir().join(format!("cutout-ride-worker-{}.sqlite", Uuid::new_v4()));
        let first = RideMapDatabase::open(&path).expect("worker opens");
        let second = RideMapDatabase::open(&path).expect("same worker is reused");
        assert_eq!(first.service_id(), second.service_id());
        assert!(matches!(
            RideMapDatabase::open(path.with_file_name("different.sqlite")),
            Err(RideMapDatabaseOpenError::AlreadyOpenForDifferentPath)
        ));
        drop(second);
        drop(first);
        std::fs::remove_file(path).expect("fixture database is removable");
    }
}
