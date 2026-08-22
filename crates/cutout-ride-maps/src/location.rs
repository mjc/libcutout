use crate::Coordinate;

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
    coordinate: Coordinate,
    monotonic_milliseconds: u64,
    wall_clock_unix_milliseconds: u64,
    horizontal_accuracy_millimetres: Option<u32>,
    source: LocationSource,
}

impl LocationSample {
    /// Creates a location sample from already validated platform values.
    #[must_use]
    pub const fn new(
        coordinate: Coordinate,
        monotonic_milliseconds: u64,
        wall_clock_unix_milliseconds: u64,
        horizontal_accuracy_millimetres: Option<u32>,
        source: LocationSource,
    ) -> Self {
        Self {
            coordinate,
            monotonic_milliseconds,
            wall_clock_unix_milliseconds,
            horizontal_accuracy_millimetres,
            source,
        }
    }

    /// Returns the coordinate.
    #[must_use]
    pub const fn coordinate(self) -> Coordinate {
        self.coordinate
    }

    /// Returns the monotonic timestamp in milliseconds.
    #[must_use]
    pub const fn monotonic_milliseconds(self) -> u64 {
        self.monotonic_milliseconds
    }

    /// Returns the wall-clock timestamp in Unix milliseconds.
    #[must_use]
    pub const fn wall_clock_unix_milliseconds(self) -> u64 {
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
        if self.monotonic_milliseconds < previous.monotonic_milliseconds {
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
}
