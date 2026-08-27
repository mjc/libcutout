use crate::{
    Coordinate, LatitudeE7, LongitudeE7, RideMapPoint, RideMapSegmentId, RidePointCount,
    RidePointSequence, RideSegmentStartReason,
};

/// Failure returned when a route projection is cancelled before completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RouteProjectionError {
    /// The caller requested that the projection stop.
    #[error("route projection cancelled")]
    Cancelled,
}

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
                let latitude = snap_to_grid(
                    coordinate.latitude().as_i32(),
                    grid,
                    -900_000_000,
                    900_000_000,
                );
                let longitude = snap_to_grid(
                    coordinate.longitude().as_i32(),
                    grid,
                    -1_800_000_000,
                    1_800_000_000,
                );
                let coordinate = Coordinate::from_bounded_fixed_parts(latitude, longitude);
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
    /// Creates a viewport, rejecting inverted latitude bounds.
    ///
    /// A minimum longitude greater than the maximum denotes an antimeridian-crossing viewport.
    #[must_use]
    pub const fn new(
        minimum_latitude: LatitudeE7,
        maximum_latitude: LatitudeE7,
        minimum_longitude: LongitudeE7,
        maximum_longitude: LongitudeE7,
    ) -> Option<Self> {
        if minimum_latitude.as_i32() > maximum_latitude.as_i32() {
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

    /// Returns the inclusive minimum latitude.
    #[must_use]
    pub const fn minimum_latitude(self) -> LatitudeE7 {
        self.minimum_latitude
    }

    /// Returns the inclusive maximum latitude.
    #[must_use]
    pub const fn maximum_latitude(self) -> LatitudeE7 {
        self.maximum_latitude
    }

    /// Returns the inclusive minimum longitude.
    #[must_use]
    pub const fn minimum_longitude(self) -> LongitudeE7 {
        self.minimum_longitude
    }

    /// Returns the inclusive maximum longitude.
    #[must_use]
    pub const fn maximum_longitude(self) -> LongitudeE7 {
        self.maximum_longitude
    }

    /// Returns whether the viewport crosses the antimeridian.
    #[must_use]
    pub const fn crosses_antimeridian(self) -> bool {
        self.minimum_longitude.as_i32() > self.maximum_longitude.as_i32()
    }

    /// Returns whether a coordinate lies inside this viewport.
    #[must_use]
    pub fn contains(self, coordinate: Coordinate) -> bool {
        let latitude = coordinate.latitude().as_i32();
        let longitude = coordinate.longitude().as_i32();
        let latitude_visible =
            (self.minimum_latitude.as_i32()..=self.maximum_latitude.as_i32()).contains(&latitude);
        let longitude_visible = if self.crosses_antimeridian() {
            longitude >= self.minimum_longitude.as_i32()
                || longitude <= self.maximum_longitude.as_i32()
        } else {
            (self.minimum_longitude.as_i32()..=self.maximum_longitude.as_i32()).contains(&longitude)
        };
        latitude_visible && longitude_visible
    }
}

/// One bounded, privacy-classified route point for a presentation client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteDisplayPoint {
    sequence: RidePointSequence,
    segment_id: RideMapSegmentId,
    segment_start_reason: RideSegmentStartReason,
    canonical_point_count: Option<RidePointCount>,
    coordinate: Coordinate,
    privacy_class: RoutePrivacyClass,
}

/// Bounded metadata needed to render one visible route segment.
///
/// The visible count and sequence range describe points retained in the bounded display
/// projection, not the complete canonical segment. The optional canonical count is populated
/// when the source projection has complete segment cardinality. A segment with one retained point
/// can be represented by an annotation or accessibility item even though a line renderer cannot
/// draw it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteSegmentDisplayMetadata {
    segment_id: RideMapSegmentId,
    start_reason: RideSegmentStartReason,
    visible_point_count: u64,
    canonical_point_count: Option<RidePointCount>,
    first_visible_sequence: Option<RidePointSequence>,
    last_visible_sequence: Option<RidePointSequence>,
}

impl RouteSegmentDisplayMetadata {
    /// Returns the canonical segment identity.
    #[must_use]
    pub const fn segment_id(self) -> RideMapSegmentId {
        self.segment_id
    }

    /// Returns why this segment began.
    #[must_use]
    pub const fn start_reason(self) -> RideSegmentStartReason {
        self.start_reason
    }

    /// Returns the number of points retained for this segment in this display projection.
    ///
    /// This is not the cardinality of the canonical source segment.
    #[must_use]
    pub const fn visible_point_count(self) -> u64 {
        self.visible_point_count
    }

    /// Returns the number of points in the canonical source segment, when known.
    #[must_use]
    pub const fn canonical_point_count(self) -> Option<RidePointCount> {
        self.canonical_point_count
    }

    /// Returns the first retained projected point sequence.
    #[must_use]
    pub const fn first_visible_sequence(self) -> Option<RidePointSequence> {
        self.first_visible_sequence
    }

    /// Returns the last retained projected point sequence.
    #[must_use]
    pub const fn last_visible_sequence(self) -> Option<RidePointSequence> {
        self.last_visible_sequence
    }

    /// Returns whether this segment has exactly one retained display point.
    ///
    /// A retained singleton does not imply that the canonical source segment contains one point;
    /// display-budget sampling may retain only one point from a larger source segment.
    #[must_use]
    pub const fn is_retained_singleton(self) -> bool {
        self.visible_point_count == 1
    }
}

/// Groups bounded projected points into ordered segment render metadata.
///
/// The input is expected in canonical sequence order, as returned by the projection APIs. The
/// result is bounded by the number of projected points and therefore cannot grow with an
/// unprojected route.
#[must_use]
pub fn route_segment_display_metadata(
    points: impl IntoIterator<Item = RouteDisplayPoint>,
) -> Vec<RouteSegmentDisplayMetadata> {
    let mut segments: Vec<RouteSegmentDisplayMetadata> = Vec::new();
    for point in points {
        if let Some(segment) = segments.last_mut()
            && segment.segment_id == point.segment_id
        {
            segment.visible_point_count = segment.visible_point_count.saturating_add(1);
            if segment.canonical_point_count.is_none() {
                segment.canonical_point_count = point.canonical_point_count;
            }
            segment.last_visible_sequence = Some(point.sequence);
            continue;
        }
        segments.push(RouteSegmentDisplayMetadata {
            segment_id: point.segment_id,
            start_reason: point.segment_start_reason,
            visible_point_count: 1,
            canonical_point_count: point.canonical_point_count,
            first_visible_sequence: Some(point.sequence),
            last_visible_sequence: Some(point.sequence),
        });
    }
    segments
}

/// Canonical route endpoints and whether each endpoint is inside the requested viewport.
///
/// Endpoint visibility is intentionally separate from the bounded display points. A display
/// budget may omit an endpoint even when it is in the viewport, and a viewport may omit an
/// endpoint while still returning interior route points. Presentation code must use the sequence
/// values to annotate only an actually displayed canonical endpoint.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RouteEndpointMetadata {
    start_sequence: Option<RidePointSequence>,
    end_sequence: Option<RidePointSequence>,
    start_visible: bool,
    end_visible: bool,
}

impl RouteEndpointMetadata {
    /// Creates endpoint metadata for a canonical route.
    #[must_use]
    pub const fn new(
        start_sequence: Option<RidePointSequence>,
        end_sequence: Option<RidePointSequence>,
        start_visible: bool,
        end_visible: bool,
    ) -> Self {
        Self {
            start_sequence,
            end_sequence,
            start_visible,
            end_visible,
        }
    }

    /// Returns the canonical first-point sequence, when the route has points.
    #[must_use]
    pub const fn start_sequence(self) -> Option<RidePointSequence> {
        self.start_sequence
    }

    /// Returns the canonical last-point sequence, when the route has points.
    #[must_use]
    pub const fn end_sequence(self) -> Option<RidePointSequence> {
        self.end_sequence
    }

    /// Returns whether the canonical first point lies in the requested viewport.
    #[must_use]
    pub const fn start_visible(self) -> bool {
        self.start_visible
    }

    /// Returns whether the canonical last point lies in the requested viewport.
    #[must_use]
    pub const fn end_visible(self) -> bool {
        self.end_visible
    }
}

/// Computes endpoint metadata from explicitly sequenced canonical points.
///
/// `points` may be a bounded tail or another non-contiguous canonical subset. The explicit
/// `source_point_count` determines the canonical endpoint sequence values; a retained point is
/// marked visible only when its supplied sequence and coordinate match that canonical endpoint.
#[must_use]
pub fn route_endpoint_metadata(
    points: impl IntoIterator<Item = (RidePointSequence, RideMapPoint)>,
    source_point_count: u64,
    viewport: Option<RouteViewport>,
) -> RouteEndpointMetadata {
    let Some(end_offset) = source_point_count.checked_sub(1) else {
        return RouteEndpointMetadata::default();
    };
    let start_sequence = RidePointSequence::new(0);
    let end_sequence = RidePointSequence::new(end_offset);
    let mut start_visible = false;
    let mut end_visible = false;
    for (sequence, point) in points {
        let visible =
            viewport.is_none_or(|viewport| viewport.contains(point.sample().coordinate()));
        if sequence == start_sequence {
            start_visible = visible;
        }
        if sequence == end_sequence {
            end_visible = visible;
        }
    }
    RouteEndpointMetadata::new(
        Some(start_sequence),
        Some(end_sequence),
        start_visible,
        end_visible,
    )
}

/// Counts segment runs in canonical route order.
///
/// Route points are ordered by their stable sequence before they reach the projection layer, so
/// each segment is represented by one contiguous run. Keeping this operation run-based avoids
/// allocating a set while callers stream a durable route.
#[must_use]
pub fn count_segment_runs(segment_ids: impl IntoIterator<Item = RideMapSegmentId>) -> usize {
    let mut previous = None;
    let mut count = 0;
    for segment_id in segment_ids {
        if previous != Some(segment_id) {
            count += 1;
            previous = Some(segment_id);
        }
    }
    count
}

/// Incremental bounded route projection state.
///
/// Durable stores use this accumulator while they stream `SQLite` rows, so projecting a long route
/// never requires retaining all canonical points in memory.
#[derive(Debug)]
pub struct RouteProjectionAccumulator {
    output_count: usize,
    candidate_count: usize,
    privacy: RoutePrivacyPolicy,
    projected: Vec<RouteDisplayPoint>,
    output_ordinal: usize,
    next_target: usize,
}

impl RouteProjectionAccumulator {
    /// Creates an accumulator for a known candidate count.
    #[must_use]
    pub fn new(
        candidate_count: usize,
        budget: RouteDisplayBudget,
        privacy: RoutePrivacyPolicy,
    ) -> Self {
        let output_count = candidate_count.min(budget.as_usize());
        Self {
            output_count,
            candidate_count,
            privacy,
            projected: Vec::with_capacity(output_count),
            output_ordinal: 0,
            next_target: 0,
        }
    }

    /// Adds one candidate in ascending route order.
    pub fn push(&mut self, candidate_ordinal: usize, sequence: u64, point: RideMapPoint) {
        self.push_with_canonical_point_count(candidate_ordinal, sequence, point, None);
    }

    /// Adds one candidate with an optional complete source-segment cardinality.
    pub fn push_with_canonical_point_count(
        &mut self,
        candidate_ordinal: usize,
        sequence: u64,
        point: RideMapPoint,
        canonical_point_count: Option<RidePointCount>,
    ) {
        if self.output_ordinal == self.output_count || candidate_ordinal != self.next_target {
            return;
        }
        let (coordinate, privacy_class) = self.privacy.project(point.sample().coordinate());
        self.projected.push(RouteDisplayPoint {
            sequence: RidePointSequence::new(sequence),
            segment_id: point.segment_id(),
            segment_start_reason: point.segment_start_reason(),
            canonical_point_count,
            coordinate,
            privacy_class,
        });
        self.output_ordinal += 1;
        if self.output_ordinal < self.output_count {
            self.next_target =
                evenly_spaced_ordinal(self.output_ordinal, self.output_count, self.candidate_count);
        }
    }

    /// Returns whether the bounded output is complete.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.output_ordinal == self.output_count
    }

    /// Finishes the projection and returns only the bounded display points.
    #[must_use]
    pub fn finish(self) -> Vec<RouteDisplayPoint> {
        self.projected
    }
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

    /// Returns why the canonical segment began.
    #[must_use]
    pub const fn segment_start_reason(self) -> RideSegmentStartReason {
        self.segment_start_reason
    }

    /// Returns the canonical source-segment cardinality, when known.
    #[must_use]
    pub const fn canonical_point_count(self) -> Option<RidePointCount> {
        self.canonical_point_count
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
    project_route_points_cancellable(points, first_sequence, viewport, budget, privacy, || false)
        .unwrap_or_default()
}

/// Projects canonical route points while checking a caller-owned cancellation predicate.
///
/// The predicate is checked before scanning candidates and between every candidate. Callers that
/// need a typed cancellation result should use this function; [`project_route_points`] remains a
/// compatibility wrapper for projections that cannot be cancelled.
///
/// # Errors
///
/// Returns [`RouteProjectionError::Cancelled`] when the predicate requests cancellation.
pub fn project_route_points_cancellable(
    points: &[RideMapPoint],
    first_sequence: RidePointSequence,
    viewport: Option<RouteViewport>,
    budget: RouteDisplayBudget,
    privacy: RoutePrivacyPolicy,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<Vec<RouteDisplayPoint>, RouteProjectionError> {
    if is_cancelled() {
        return Err(RouteProjectionError::Cancelled);
    }
    let is_visible = |point: RideMapPoint| {
        viewport.is_none_or(|viewport| viewport.contains(point.sample().coordinate()))
    };
    let candidate_count = points.iter().copied().try_fold(0usize, |count, point| {
        if is_cancelled() {
            return Err(RouteProjectionError::Cancelled);
        }
        Ok(count + usize::from(is_visible(point)))
    })?;
    let mut accumulator = RouteProjectionAccumulator::new(candidate_count, budget, privacy);
    let mut candidate_ordinal = 0;
    for (offset, point) in points.iter().copied().enumerate() {
        if is_cancelled() {
            return Err(RouteProjectionError::Cancelled);
        }
        if !is_visible(point) {
            continue;
        }
        let sequence = first_sequence.saturating_add(as_u64(offset)).as_u64();
        accumulator.push(candidate_ordinal, sequence, point);
        candidate_ordinal += 1;
        if accumulator.is_complete() {
            break;
        }
    }
    if is_cancelled() {
        return Err(RouteProjectionError::Cancelled);
    }
    Ok(accumulator.finish())
}

/// Projects a stream of already viewport-filtered canonical points without retaining the route.
///
/// The caller supplies the number of candidates because durable stores can count and stream
/// visible points separately. This keeps the output bounded while reusing the same deterministic
/// LOD and privacy policy as the in-memory route projection.
#[must_use]
pub fn project_route_points_from_iter(
    points: impl IntoIterator<Item = (u64, RideMapPoint)>,
    candidate_count: usize,
    budget: RouteDisplayBudget,
    privacy: RoutePrivacyPolicy,
) -> Vec<RouteDisplayPoint> {
    let mut accumulator = RouteProjectionAccumulator::new(candidate_count, budget, privacy);
    for (candidate_ordinal, (sequence, point)) in points.into_iter().enumerate() {
        accumulator.push(candidate_ordinal, sequence, point);
        if accumulator.is_complete() {
            break;
        }
    }
    accumulator.finish()
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

fn snap_to_grid(value: i32, grid: RoutePrivacyGridE7, minimum: i32, maximum: i32) -> i32 {
    let grid = i64::from(grid.as_u32());
    let snapped = i64::from(value).div_euclid(grid) * grid;
    // Euclidean flooring can cross the negative world boundary when the boundary is not
    // divisible by the grid. Move that bucket inward before clamping the representable domain.
    let snapped = if snapped < i64::from(minimum) {
        snapped + grid
    } else {
        snapped
    };
    let snapped = snapped.clamp(i64::from(minimum), i64::from(maximum));
    #[allow(clippy::cast_possible_truncation)]
    {
        snapped as i32
    }
}
