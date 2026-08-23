use crate::LocationSample;

const EARTH_RADIUS_METRES: f64 = 6_371_000.0;

/// Derived distance represented in millimetres.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DistanceMillimetres(u64);

impl DistanceMillimetres {
    /// Creates a distance from millimetres.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the distance in millimetres.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Deterministic projection of canonical ride location samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideSummary {
    point_count: u64,
    distance_millimetres: DistanceMillimetres,
}

impl RideSummary {
    /// Derives a summary from samples in monotonic order.
    #[must_use]
    pub fn from_samples(samples: &[LocationSample]) -> Self {
        let distance_metres = samples
            .windows(2)
            .map(|pair| haversine_metres(pair[0], pair[1]))
            .sum::<f64>();
        let distance_millimetres = rounded_distance_millimetres(distance_metres);
        Self {
            point_count: samples.len() as u64,
            distance_millimetres: DistanceMillimetres::new(distance_millimetres),
        }
    }

    /// Returns the number of canonical points represented by this summary.
    #[must_use]
    pub const fn point_count(self) -> u64 {
        self.point_count
    }

    /// Returns the derived distance in millimetres.
    #[must_use]
    pub const fn distance_millimetres(self) -> u64 {
        self.distance_millimetres.as_u64()
    }

    pub(crate) const fn from_stored(point_count: u64, distance_millimetres: u64) -> Self {
        Self {
            point_count,
            distance_millimetres: DistanceMillimetres::new(distance_millimetres),
        }
    }
}

pub(crate) fn distance_between_millimetres(first: LocationSample, second: LocationSample) -> u64 {
    rounded_distance_millimetres(haversine_metres(first, second))
}

fn rounded_distance_millimetres(distance_metres: f64) -> u64 {
    let scaled = (distance_metres * 1_000.0).round();
    debug_assert!(scaled.is_finite());
    debug_assert!(scaled >= 0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        scaled as u64
    }
}

fn haversine_metres(first: LocationSample, second: LocationSample) -> f64 {
    let latitude_delta = (second.coordinate().latitude_degrees()
        - first.coordinate().latitude_degrees())
    .to_radians();
    let longitude_delta = (second.coordinate().longitude_degrees()
        - first.coordinate().longitude_degrees())
    .to_radians();
    let first_latitude = first.coordinate().latitude_degrees().to_radians();
    let second_latitude = second.coordinate().latitude_degrees().to_radians();
    let haversine = (latitude_delta / 2.0).sin().powi(2)
        + first_latitude.cos() * second_latitude.cos() * (longitude_delta / 2.0).sin().powi(2);
    let haversine = haversine.clamp(0.0, 1.0);
    2.0 * EARTH_RADIUS_METRES * haversine.sqrt().atan2((1.0 - haversine).sqrt())
}
