use thiserror::Error;

const DEGREES_SCALE: f64 = 10_000_000.0;
const MIN_LATITUDE_E7: i32 = -900_000_000;
const MAX_LATITUDE_E7: i32 = 900_000_000;
const MIN_LONGITUDE_E7: i32 = -1_800_000_000;
const MAX_LONGITUDE_E7: i32 = 1_800_000_000;

/// Latitude stored as signed degrees multiplied by 10^7.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LatitudeE7(i32);

impl LatitudeE7 {
    const fn from_validated(value: i32) -> Self {
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
    const fn from_validated(value: i32) -> Self {
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
pub struct Wgs84Coordinate {
    latitude: LatitudeE7,
    longitude: LongitudeE7,
}

impl Wgs84Coordinate {
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

        Ok(Self::from_parts(
            LatitudeE7::from_validated(degrees_to_e7(latitude)),
            LongitudeE7::from_validated(degrees_to_e7(longitude)),
        ))
    }

    /// Creates a coordinate from checked fixed-point degree values.
    ///
    /// # Errors
    ///
    /// Returns a typed error when either fixed-point value is outside WGS84 bounds.
    pub fn from_fixed_parts(latitude: i32, longitude: i32) -> Result<Self, CoordinateError> {
        let latitude = LatitudeE7::try_from(latitude)?;
        let longitude = LongitudeE7::try_from(longitude)?;
        Ok(Self::from_parts(latitude, longitude))
    }

    /// Creates a coordinate from already-validated WGS84 components.
    #[must_use]
    pub const fn from_parts(latitude: LatitudeE7, longitude: LongitudeE7) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    pub(crate) const fn from_bounded_fixed_parts(latitude: i32, longitude: i32) -> Self {
        debug_assert!(latitude >= MIN_LATITUDE_E7 && latitude <= MAX_LATITUDE_E7);
        debug_assert!(longitude >= MIN_LONGITUDE_E7 && longitude <= MAX_LONGITUDE_E7);
        Self::from_parts(
            LatitudeE7::from_validated(latitude),
            LongitudeE7::from_validated(longitude),
        )
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

impl TryFrom<i32> for LatitudeE7 {
    type Error = CoordinateError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if (MIN_LATITUDE_E7..=MAX_LATITUDE_E7).contains(&value) {
            Ok(Self::from_validated(value))
        } else {
            Err(CoordinateError::LatitudeOutOfRange)
        }
    }
}

impl TryFrom<i32> for LongitudeE7 {
    type Error = CoordinateError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if (MIN_LONGITUDE_E7..=MAX_LONGITUDE_E7).contains(&value) {
            Ok(Self::from_validated(value))
        } else {
            Err(CoordinateError::LongitudeOutOfRange)
        }
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
