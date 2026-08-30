use crate::Wgs84Coordinate;

/// Monotonic milliseconds used for ordering samples and lifecycle events.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicMilliseconds(u64);

impl MonotonicMilliseconds {
    /// Creates a timestamp from the platform representation.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the platform representation.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the later of the two timestamps.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        if self.0 >= other.0 { self } else { other }
    }

    /// Subtracts timestamps without underflow.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> u64 {
        self.0.saturating_sub(other.0)
    }
}

impl From<u64> for MonotonicMilliseconds {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// Unix wall-clock milliseconds carried alongside a location sample.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WallClockUnixMilliseconds(u64);

impl WallClockUnixMilliseconds {
    /// Creates a timestamp from the platform representation.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the platform representation.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl From<u64> for WallClockUnixMilliseconds {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

/// Source of one location sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationSource {
    /// A live platform location update.
    Live,
    /// A location correlated with an imported PEVCAP record.
    PevcapImport,
}

/// A validated location sample with explicit millisecond and millimetre units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocationSample {
    coordinate: Wgs84Coordinate,
    monotonic_milliseconds: MonotonicMilliseconds,
    wall_clock_unix_milliseconds: WallClockUnixMilliseconds,
    horizontal_accuracy_millimetres: Option<u32>,
    source: LocationSource,
}

impl LocationSample {
    /// Creates a location sample from already validated platform values.
    #[must_use]
    pub fn new(
        coordinate: Wgs84Coordinate,
        monotonic_milliseconds: impl Into<MonotonicMilliseconds>,
        wall_clock_unix_milliseconds: impl Into<WallClockUnixMilliseconds>,
        horizontal_accuracy_millimetres: Option<u32>,
        source: LocationSource,
    ) -> Self {
        Self {
            coordinate,
            monotonic_milliseconds: monotonic_milliseconds.into(),
            wall_clock_unix_milliseconds: wall_clock_unix_milliseconds.into(),
            horizontal_accuracy_millimetres,
            source,
        }
    }

    /// Returns the coordinate.
    #[must_use]
    pub const fn coordinate(self) -> Wgs84Coordinate {
        self.coordinate
    }

    /// Returns the monotonic timestamp in milliseconds.
    #[must_use]
    pub const fn monotonic_milliseconds(self) -> MonotonicMilliseconds {
        self.monotonic_milliseconds
    }

    /// Returns the wall-clock timestamp in Unix milliseconds.
    #[must_use]
    pub const fn wall_clock_unix_milliseconds(self) -> WallClockUnixMilliseconds {
        self.wall_clock_unix_milliseconds
    }

    /// Returns the optional horizontal accuracy in millimetres.
    #[must_use]
    pub const fn horizontal_accuracy_millimetres(self) -> Option<u32> {
        self.horizontal_accuracy_millimetres
    }

    /// Returns the provenance source.
    #[must_use]
    pub const fn source(self) -> LocationSource {
        self.source
    }

    /// Compares a candidate with the previous accepted sample.
    #[must_use]
    pub fn admission(&self, previous: Option<&Self>) -> LocationAdmission {
        let Some(previous) = previous else {
            return LocationAdmission::Accepted;
        };
        if self == previous {
            return LocationAdmission::Duplicate;
        }
        if self.monotonic_milliseconds <= previous.monotonic_milliseconds {
            return LocationAdmission::OutOfOrder;
        }
        LocationAdmission::Accepted
    }
}

/// Result of checking one candidate location sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationAdmission {
    /// The sample can be appended.
    Accepted,
    /// The sample repeats the previous accepted sample exactly.
    Duplicate,
    /// The sample would move time backwards.
    OutOfOrder,
    /// The sample's horizontal accuracy exceeds the Rust admission threshold.
    AccuracyTooLow,
    /// The sample implies an impossible travel speed for a live phone fix.
    UnrealisticJump,
}
