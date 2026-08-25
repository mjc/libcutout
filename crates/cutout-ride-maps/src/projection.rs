use crate::{
    Coordinate, LatitudeE7, LongitudeE7, RideMapPoint, RideMapSegmentId, RidePointSequence,
};

/// Hard upper bound for a route projection returned to a presentation client.
pub const MAX_ROUTE_DISPLAY_POINTS: usize = 16_384;

/// A non-zero bound on the number of route points returned by a projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RouteDisplayBudget(usize);

impl RouteDisplayBudget {
    /// Creates a display budget within the Rust-owned projection bound.
    #[must_use]
    pub const fn new(value: usize) -> Option<Self> {
        if value == 0 || value > MAX_ROUTE_DISPLAY_POINTS {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the maximum number of projected points.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

/// A fixed-point grid size used to redact a route coordinate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoutePrivacyGridE7(u32);

impl RoutePrivacyGridE7 {
    /// Creates a non-zero fixed-point grid size.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Returns the grid size in fixed-point degrees.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// Privacy classification attached to an outbound route coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePrivacyClass {
    /// The caller explicitly requested the exact canonical coordinate.
    Precise,
    /// The coordinate was snapped to a privacy grid before projection.
    GridRedacted,
}

/// Explicit policy for projecting canonical coordinates beyond the domain boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePrivacyPolicy {
    /// Preserve exact coordinates for an authorized route-detail surface.
    Precise,
    /// Snap both coordinate components down to the supplied fixed-point grid.
    Grid(RoutePrivacyGridE7),
}

impl RoutePrivacyPolicy {
    /// Creates a grid-redaction policy.
    #[must_use]
    pub const fn grid(size: RoutePrivacyGridE7) -> Self {
        Self::Grid(size)
    }

    fn project(self, coordinate: Coordinate) -> (Coordinate, RoutePrivacyClass) {
        match self {
            Self::Precise => (coordinate, RoutePrivacyClass::Precise),
            Self::Grid(grid) => {
                let latitude = snap_to_grid(coordinate.latitude().as_i32(), grid);
                let longitude = snap_to_grid(coordinate.longitude().as_i32(), grid);
                let coordinate = match Coordinate::from_fixed_parts(latitude, longitude) {
                    Ok(coordinate) => coordinate,
                    Err(_) => coordinate,
                };
                (coordinate, RoutePrivacyClass::GridRedacted)
            }
        }
    }
}

/// An inclusive geographic viewport in fixed-point WGS84 degrees.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteViewport {
    minimum_latitude: LatitudeE7,
    maximum_latitude: LatitudeE7,
    minimum_longitude: LongitudeE7,
    maximum_longitude: LongitudeE7,
}

impl RouteViewport {
    /// Creates a viewport, rejecting inverted latitude or longitude bounds.
    #[must_use]
    pub const fn new(
        minimum_latitude: LatitudeE7,
        maximum_latitude: LatitudeE7,
        minimum_longitude: LongitudeE7,
        maximum_longitude: LongitudeE7,
    ) -> Option<Self> {
        if minimum_latitude.as_i32() > maximum_latitude.as_i32()
            || minimum_longitude.as_i32() > maximum_longitude.as_i32()
        {
            None
        } else {
            Some(Self {
                minimum_latitude,
                maximum_latitude,
                minimum_longitude,
                maximum_longitude,
            })
        }
    }

    fn contains(self, coordinate: Coordinate) -> bool {
        let latitude = coordinate.latitude().as_i32();
        let longitude = coordinate.longitude().as_i32();
        (self.minimum_latitude.as_i32()..=self.maximum_latitude.as_i32()).contains(&latitude)
            && (self.minimum_longitude.as_i32()..=self.maximum_longitude.as_i32())
                .contains(&longitude)
    }
}

/// One bounded, privacy-classified route point for a presentation client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteDisplayPoint {
    sequence: RidePointSequence,
    segment_id: RideMapSegmentId,
    coordinate: Coordinate,
    privacy_class: RoutePrivacyClass,
}

impl RouteDisplayPoint {
    /// Returns the canonical route sequence.
    #[must_use]
    pub const fn sequence(self) -> RidePointSequence {
        self.sequence
    }

    /// Returns the canonical route segment.
    #[must_use]
    pub const fn segment_id(self) -> RideMapSegmentId {
        self.segment_id
    }

    /// Returns the privacy-projected coordinate.
    #[must_use]
    pub const fn coordinate(self) -> Coordinate {
        self.coordinate
    }

    /// Returns the classification applied before projection.
    #[must_use]
    pub const fn privacy_class(self) -> RoutePrivacyClass {
        self.privacy_class
    }
}

/// Projects canonical route points into a bounded, viewport-aware representation.
///
/// Selection preserves the first and last point visible in the viewport and samples
/// the remaining points at deterministic evenly spaced ordinals. The input slice is
/// never copied in full, and every returned coordinate has the requested privacy
/// policy applied before it leaves this domain API.
#[must_use]
pub fn project_route_points(
    points: &[RideMapPoint],
    first_sequence: RidePointSequence,
    viewport: Option<RouteViewport>,
    budget: RouteDisplayBudget,
    privacy: RoutePrivacyPolicy,
) -> Vec<RouteDisplayPoint> {
    let is_visible = |point: RideMapPoint| {
        viewport.is_none_or(|viewport| viewport.contains(point.sample().coordinate()))
    };
    let candidate_count = points
        .iter()
        .copied()
        .filter(|point| is_visible(*point))
        .count();
    let output_count = candidate_count.min(budget.as_usize());
    if output_count == 0 {
        return Vec::new();
    }

    let mut projected = Vec::with_capacity(output_count);
    let mut candidate_ordinal = 0_usize;
    let mut output_ordinal = 0_usize;
    let mut next_target = 0_usize;
    for (input_ordinal, point) in points.iter().copied().enumerate() {
        if !is_visible(point) {
            continue;
        }
        if candidate_ordinal == next_target {
            let (coordinate, privacy_class) = privacy.project(point.sample().coordinate());
            projected.push(RouteDisplayPoint {
                sequence: first_sequence.saturating_add(as_u64(input_ordinal)),
                segment_id: point.segment_id(),
                coordinate,
                privacy_class,
            });
            output_ordinal += 1;
            if output_ordinal == output_count {
                break;
            }
            next_target = evenly_spaced_ordinal(output_ordinal, output_count, candidate_count);
        }
        candidate_ordinal += 1;
    }
    projected
}

fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn evenly_spaced_ordinal(
    output_ordinal: usize,
    output_count: usize,
    candidate_count: usize,
) -> usize {
    if output_count <= 1 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let numerator = (output_ordinal as u128) * (candidate_count.saturating_sub(1) as u128);
    #[allow(clippy::cast_possible_truncation)]
    {
        (numerator / (output_count - 1) as u128) as usize
    }
}

fn snap_to_grid(value: i32, grid: RoutePrivacyGridE7) -> i32 {
    let grid = i64::from(grid.as_u32());
    let snapped = i64::from(value).div_euclid(grid) * grid;
    #[allow(clippy::cast_possible_truncation)]
    {
        snapped as i32
    }
}
