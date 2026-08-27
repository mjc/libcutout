use thiserror::Error;

const DEGREES_SCALE: f64 = 10_000_000.0;

/// Latitude stored as signed degrees multiplied by 10^7.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LatitudeE7(i32);

impl LatitudeE7 {
    /// Creates a fixed-point latitude after the caller has validated its range.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// Returns the fixed-point value.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

/// Longitude stored as signed degrees multiplied by 10^7.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LongitudeE7(i32);

impl LongitudeE7 {
    /// Creates a fixed-point longitude after the caller has validated its range.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// Returns the fixed-point value.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

/// A validated WGS84 coordinate represented without floating-point storage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Coordinate {
    latitude: LatitudeE7,
    longitude: LongitudeE7,
}

impl Coordinate {
    /// Creates a coordinate from WGS84 degrees.
    ///
    /// # Errors
    ///
    /// Returns a typed error for non-finite or out-of-range values.
    pub fn from_degrees(latitude: f64, longitude: f64) -> Result<Self, CoordinateError> {
        if !latitude.is_finite() || !longitude.is_finite() {
            return Err(CoordinateError::NonFinite);
        }
        if !(-90.0..=90.0).contains(&latitude) {
            return Err(CoordinateError::LatitudeOutOfRange);
        }
        if !(-180.0..=180.0).contains(&longitude) {
            return Err(CoordinateError::LongitudeOutOfRange);
        }

        Ok(Self {
            latitude: LatitudeE7::new(degrees_to_e7(latitude)),
            longitude: LongitudeE7::new(degrees_to_e7(longitude)),
        })
    }

    /// Creates a coordinate from checked fixed-point degree values.
    ///
    /// # Errors
    ///
    /// Returns a typed error when either fixed-point value is outside WGS84 bounds.
    pub fn from_fixed_parts(latitude: i32, longitude: i32) -> Result<Self, CoordinateError> {
        if !(-900_000_000..=900_000_000).contains(&latitude) {
            return Err(CoordinateError::LatitudeOutOfRange);
        }
        if !(-1_800_000_000..=1_800_000_000).contains(&longitude) {
            return Err(CoordinateError::LongitudeOutOfRange);
        }
        Ok(Self {
            latitude: LatitudeE7::new(latitude),
            longitude: LongitudeE7::new(longitude),
        })
    }

    pub(crate) const fn from_bounded_fixed_parts(latitude: i32, longitude: i32) -> Self {
        Self {
            latitude: LatitudeE7::new(latitude),
            longitude: LongitudeE7::new(longitude),
        }
    }

    /// Returns the fixed-point latitude.
    #[must_use]
    pub const fn latitude(self) -> LatitudeE7 {
        self.latitude
    }

    /// Returns the fixed-point longitude.
    #[must_use]
    pub const fn longitude(self) -> LongitudeE7 {
        self.longitude
    }

    /// Returns the latitude as degrees for derived calculations.
    #[must_use]
    pub fn latitude_degrees(self) -> f64 {
        f64::from(self.latitude.0) / DEGREES_SCALE
    }

    /// Returns the longitude as degrees for derived calculations.
    #[must_use]
    pub fn longitude_degrees(self) -> f64 {
        f64::from(self.longitude.0) / DEGREES_SCALE
    }
}

fn degrees_to_e7(value: f64) -> i32 {
    let scaled = (value * DEGREES_SCALE).round();
    debug_assert!(scaled.is_finite());
    debug_assert!(scaled >= f64::from(i32::MIN));
    debug_assert!(scaled <= f64::from(i32::MAX));
    #[allow(clippy::cast_possible_truncation)]
    {
        scaled as i32
    }
}

/// Failure while validating a geographic coordinate.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CoordinateError {
    /// One or both input values were NaN or infinite.
    #[error("coordinate contains a non-finite value")]
    NonFinite,
    /// Latitude was outside the WGS84 range.
    #[error("latitude is outside -90..=90 degrees")]
    LatitudeOutOfRange,
    /// Longitude was outside the WGS84 range.
    #[error("longitude is outside -180..=180 degrees")]
    LongitudeOutOfRange,
}
