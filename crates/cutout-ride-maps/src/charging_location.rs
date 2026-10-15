use crate::{
    Coordinate, LocationSample, LocationSource, MonotonicMilliseconds, WallClockUnixMilliseconds,
    distance_between,
};

/// Maximum age of a map location fix, in seconds.
pub const CHARGING_LOCATION_FIX_MAXIMUM_AGE_SECONDS: f64 = 120.0;

/// A validated location fix used to choose the map's current position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargingLocationFix {
    coordinate: Coordinate,
    source_timestamp_unix_seconds: f64,
    horizontal_accuracy_meters: f64,
}

impl ChargingLocationFix {
    /// Creates a map fix from platform values, rejecting invalid coordinates, timestamps,
    /// and horizontal accuracies.
    #[must_use]
    pub fn new(
        latitude_degrees: f64,
        longitude_degrees: f64,
        source_timestamp_unix_seconds: f64,
        horizontal_accuracy_meters: f64,
    ) -> Option<Self> {
        let coordinate = Coordinate::from_degrees(latitude_degrees, longitude_degrees).ok()?;
        if !source_timestamp_unix_seconds.is_finite()
            || !horizontal_accuracy_meters.is_finite()
            || horizontal_accuracy_meters < 0.0
        {
            return None;
        }
        Some(Self {
            coordinate,
            source_timestamp_unix_seconds,
            horizontal_accuracy_meters,
        })
    }

    /// Returns the WGS84 coordinate.
    #[must_use]
    pub const fn coordinate(self) -> Coordinate {
        self.coordinate
    }

    /// Returns the source timestamp in Unix seconds.
    #[must_use]
    pub const fn source_timestamp_unix_seconds(self) -> f64 {
        self.source_timestamp_unix_seconds
    }

    /// Returns horizontal accuracy in metres.
    #[must_use]
    pub const fn horizontal_accuracy_meters(self) -> f64 {
        self.horizontal_accuracy_meters
    }
}

/// Chooses a fresh map fix using the caller's wall-clock time.
///
/// Movement uses the ride-map crate's spherical Haversine distance (Earth radius 6,371 km),
/// rounded to the existing millimetre distance unit before comparison.
#[must_use]
pub fn select_charging_location_fix(
    current: Option<ChargingLocationFix>,
    candidate: ChargingLocationFix,
    now_unix_seconds: f64,
) -> Option<ChargingLocationFix> {
    if !now_unix_seconds.is_finite() || !is_fresh(candidate, now_unix_seconds) {
        return None;
    }
    let Some(current) = current.filter(|fix| is_fresh(*fix, now_unix_seconds)) else {
        return Some(candidate);
    };

    let newer = candidate.source_timestamp_unix_seconds > current.source_timestamp_unix_seconds;
    let distance_millimetres = distance_between(
        coordinate_sample(current.coordinate),
        coordinate_sample(candidate.coordinate),
    )
    .as_u64();
    // The existing distance API returns integer millimetres; accuracy arrives as fractional metres.
    #[allow(clippy::cast_precision_loss)]
    let distance_millimetres = distance_millimetres as f64;
    let moved_beyond_accuracy = newer
        && distance_millimetres
            > (current.horizontal_accuracy_meters + candidate.horizontal_accuracy_meters) * 1_000.0;
    let more_precise = candidate.horizontal_accuracy_meters < current.horizontal_accuracy_meters;
    let newer_and_at_least_as_precise =
        candidate.horizontal_accuracy_meters <= current.horizontal_accuracy_meters && newer;
    (moved_beyond_accuracy || more_precise || newer_and_at_least_as_precise).then_some(candidate)
}

fn is_fresh(fix: ChargingLocationFix, now_unix_seconds: f64) -> bool {
    let age = now_unix_seconds - fix.source_timestamp_unix_seconds;
    (0.0..=CHARGING_LOCATION_FIX_MAXIMUM_AGE_SECONDS).contains(&age)
}

fn coordinate_sample(coordinate: Coordinate) -> LocationSample {
    LocationSample::new(
        coordinate,
        MonotonicMilliseconds::new(0),
        WallClockUnixMilliseconds::new(0),
        None,
        LocationSource::Live,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(latitude: f64, accuracy: f64, timestamp: f64) -> ChargingLocationFix {
        ChargingLocationFix::new(latitude, 0.0, timestamp, accuracy).unwrap()
    }

    #[test]
    fn rejects_invalid_values_and_future_or_expired_fixes() {
        assert!(ChargingLocationFix::new(f64::NAN, 0.0, 1.0, 1.0).is_none());
        assert!(ChargingLocationFix::new(0.0, 0.0, f64::INFINITY, 1.0).is_none());
        assert!(ChargingLocationFix::new(0.0, 0.0, 1.0, -1.0).is_none());
        let current = Some(fix(0.0, 5.0, 1_000.0));
        assert_eq!(
            select_charging_location_fix(current, fix(1.0, 1.0, 1_001.0), 1_000.0),
            None
        );
        assert_eq!(
            select_charging_location_fix(current, fix(1.0, 1.0, 879.0), 1_000.0),
            None
        );
    }

    #[test]
    fn prefers_precision_equal_accuracy_recency_and_replaces_expired_current() {
        let precise = fix(0.0, 4.0, 1_000.0);
        assert_eq!(
            select_charging_location_fix(Some(fix(1.0, 20.0, 1_000.0)), precise, 1_000.0),
            Some(precise)
        );
        let newer = fix(0.0, 4.0, 1_001.0);
        assert_eq!(
            select_charging_location_fix(Some(precise), newer, 1_001.0),
            Some(newer)
        );
        let after_expiry = fix(1.0, 80.0, 1_121.0);
        assert_eq!(
            select_charging_location_fix(Some(precise), after_expiry, 1_121.0),
            Some(after_expiry)
        );
    }

    #[test]
    fn movement_threshold_uses_spherical_distance_and_combined_accuracy() {
        let current = fix(0.0, 40.0, 1_000.0);
        let inside_threshold = fix(0.000_898_3, 60.0, 1_001.0);
        let distance = distance_between(
            coordinate_sample(current.coordinate),
            coordinate_sample(inside_threshold.coordinate),
        );
        assert!(distance.as_u64() < 100_000);
        assert_eq!(
            select_charging_location_fix(Some(current), inside_threshold, 1_001.0),
            None
        );
        let beyond_threshold = fix(0.000_9, 60.0, 1_002.0);
        assert_eq!(
            select_charging_location_fix(Some(current), beyond_threshold, 1_002.0),
            Some(beyond_threshold)
        );
    }
}
