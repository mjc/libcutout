use std::sync::{Mutex, PoisonError};

use cutout_ride_maps::Coordinate;
use libcutout_persistence::{ChargingFilter, GeoBounds};

const METRO_WINDOW_METERS: f64 = 100_000.0;
const SEARCH_LIMIT: u32 = 1;
const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// Search bounds selected for an automatic nearby charging discovery.
///
/// Construction validates the bounds with the shared persistence geographic
/// bounds type while retaining the calculated floating-point edges for native
/// query adapters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargingDiscoveryBounds {
    minimum_latitude: f64,
    maximum_latitude: f64,
    minimum_longitude: f64,
    maximum_longitude: f64,
}

impl ChargingDiscoveryBounds {
    fn new(
        minimum_latitude_degrees: f64,
        maximum_latitude_degrees: f64,
        minimum_longitude_degrees: f64,
        maximum_longitude_degrees: f64,
    ) -> Option<Self> {
        GeoBounds::new(
            minimum_latitude_degrees,
            maximum_latitude_degrees,
            minimum_longitude_degrees,
            maximum_longitude_degrees,
        )
        .ok()?;
        Some(Self {
            minimum_latitude: minimum_latitude_degrees,
            maximum_latitude: maximum_latitude_degrees,
            minimum_longitude: minimum_longitude_degrees,
            maximum_longitude: maximum_longitude_degrees,
        })
    }

    /// Returns the southern edge in WGS84 degrees.
    #[must_use]
    pub const fn minimum_latitude_degrees(self) -> f64 {
        self.minimum_latitude
    }

    /// Returns the northern edge in WGS84 degrees.
    #[must_use]
    pub const fn maximum_latitude_degrees(self) -> f64 {
        self.maximum_latitude
    }

    /// Returns the western edge in WGS84 degrees.
    #[must_use]
    pub const fn minimum_longitude_degrees(self) -> f64 {
        self.minimum_longitude
    }

    /// Returns the eastern edge in WGS84 degrees. This may be less than the western edge when
    /// the interval crosses the antimeridian.
    #[must_use]
    pub const fn maximum_longitude_degrees(self) -> f64 {
        self.maximum_longitude
    }
}

/// One automatic nearby query selected by charging discovery policy.
#[derive(Clone, Debug, PartialEq)]
pub struct ChargingDiscoveryRequest<F = ChargingFilter> {
    /// Search bounds around the origin, including antimeridian crossing when needed.
    pub bounds: ChargingDiscoveryBounds,
    /// Validated location used to rank nearby spots.
    pub origin: Coordinate,
    /// Current caller-owned charging filters.
    pub filter: F,
    /// Number of nearest results requested.
    pub limit: u32,
}

/// Outcome delivered when the caller finishes an automatic nearby search.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargingDiscoverySearchResult {
    /// The query returned no nearest result and may be retried after new cache data arrives.
    Empty,
    /// The query returned a valid nearest coordinate for one-time camera use.
    Nearest(Coordinate),
    /// The query returned a nearest result, but its coordinate was unusable.
    InvalidNearest,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Phase {
    #[default]
    AwaitingViewport,
    AwaitingLocation,
    Searching,
    Empty,
    Finished,
}

#[derive(Debug)]
struct State<F> {
    phase: Phase,
    origin: Option<Coordinate>,
    filter: F,
}

impl<F> State<F>
where
    F: Clone + PartialEq,
{
    fn start_search(&mut self) -> Option<ChargingDiscoveryRequest<F>> {
        if self.phase != Phase::AwaitingLocation {
            return None;
        }
        let origin = self.origin?;
        let bounds = search_bounds(origin)?;
        self.phase = Phase::Searching;
        Some(ChargingDiscoveryRequest {
            bounds,
            origin,
            filter: self.filter.clone(),
            limit: SEARCH_LIMIT,
        })
    }
}

/// Owns the initial nearby-discovery state and emits a validated camera target once.
///
/// The filter value belongs to the caller's application surface. The service only
/// retains, compares, and returns it as part of the selected search request.
#[derive(Debug)]
pub struct ChargingDiscovery<F = ChargingFilter> {
    state: Mutex<State<F>>,
}

impl<F> ChargingDiscovery<F>
where
    F: Clone + PartialEq,
{
    /// Creates an eligible discovery policy with the initial charging filters.
    #[must_use]
    pub fn new(filter: F) -> Self {
        Self {
            state: Mutex::new(State {
                phase: Phase::AwaitingViewport,
                origin: None,
                filter,
            }),
        }
    }

    /// Records a local viewport result and starts discovery only when it is empty.
    pub fn viewport_loaded(&self, has_local_results: bool) -> Option<ChargingDiscoveryRequest<F>> {
        let mut state = self.lock();
        if state.phase != Phase::AwaitingViewport && state.phase != Phase::AwaitingLocation {
            return None;
        }
        if has_local_results {
            state.phase = Phase::Finished;
            return None;
        }
        state.phase = Phase::AwaitingLocation;
        state.start_search()
    }

    /// Stores a validated current location and starts a pending initial search, if any.
    pub fn location_available(&self, location: Coordinate) -> Option<ChargingDiscoveryRequest<F>> {
        let mut state = self.lock();
        state.origin = Some(location);
        state.start_search()
    }

    /// Settles the in-flight query and returns a validated coordinate for one-time camera use.
    pub fn search_completed(&self, result: ChargingDiscoverySearchResult) -> Option<Coordinate> {
        let mut state = self.lock();
        if state.phase != Phase::Searching {
            return None;
        }
        match result {
            ChargingDiscoverySearchResult::Empty => {
                state.phase = Phase::Empty;
                None
            }
            ChargingDiscoverySearchResult::Nearest(coordinate) => {
                state.phase = Phase::Finished;
                Some(coordinate)
            }
            ChargingDiscoverySearchResult::InvalidNearest => {
                state.phase = Phase::Finished;
                None
            }
        }
    }

    /// Settles a failed query without qualifying for a download-triggered retry.
    pub fn search_failed(&self) {
        let mut state = self.lock();
        if state.phase == Phase::Searching {
            state.phase = Phase::Finished;
        }
    }

    /// Settles cancellation of the in-flight query.
    pub fn search_cancelled(&self) {
        let mut state = self.lock();
        if state.phase == Phase::Searching {
            state.phase = Phase::Finished;
        }
    }

    /// Applies a filter change. Once automatic discovery began, a filter change cancels it.
    pub fn set_filter(&self, filter: F) {
        let mut state = self.lock();
        if state.filter == filter {
            return;
        }
        state.filter = filter;
        if state.phase != Phase::AwaitingViewport {
            cancel(&mut state);
        }
    }

    /// Permanently cancels automatic discovery after a user camera or filter action.
    pub fn user_override(&self) {
        cancel(&mut self.lock());
    }

    /// Rearms an empty search only after downloads add no pins to the current local result.
    pub fn community_cache_updated(
        &self,
        downloaded_count: u32,
        has_local_results: bool,
    ) -> Option<ChargingDiscoveryRequest<F>> {
        let mut state = self.lock();
        if downloaded_count == 0 || has_local_results || state.phase != Phase::Empty {
            return None;
        }
        state.phase = Phase::AwaitingLocation;
        state.start_search()
    }
}

impl<F> ChargingDiscovery<F>
where
    F: Clone + PartialEq,
{
    fn lock(&self) -> std::sync::MutexGuard<'_, State<F>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn cancel<F>(state: &mut State<F>) {
    state.phase = Phase::Finished;
}

fn search_bounds(origin: Coordinate) -> Option<ChargingDiscoveryBounds> {
    let latitude = origin.latitude_degrees();
    let longitude = origin.longitude_degrees();
    let half_angular_span = (METRO_WINDOW_METERS / 2.0 / EARTH_RADIUS_METERS).to_degrees();
    let minimum_latitude = (latitude - half_angular_span).max(-90.0);
    let maximum_latitude = (latitude + half_angular_span).min(90.0);
    let longitude_half_span =
        (METRO_WINDOW_METERS / 2.0) / (EARTH_RADIUS_METERS * latitude.to_radians().cos().abs());
    let (minimum_longitude, maximum_longitude) =
        if !longitude_half_span.is_finite() || longitude_half_span >= std::f64::consts::PI {
            (-180.0, 180.0)
        } else {
            (
                wrap_longitude(longitude - longitude_half_span.to_degrees()),
                wrap_longitude(longitude + longitude_half_span.to_degrees()),
            )
        };
    ChargingDiscoveryBounds::new(
        minimum_latitude,
        maximum_latitude,
        minimum_longitude,
        maximum_longitude,
    )
}

fn wrap_longitude(longitude: f64) -> f64 {
    let wrapped = (longitude + 180.0).rem_euclid(360.0) - 180.0;
    // rem_euclid maps the exact positive antimeridian to -180; retain its positive-edge spelling.
    if wrapped <= -180.0 && longitude > 0.0 {
        180.0
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    struct Filter(u8);

    fn location(latitude: f64, longitude: f64) -> Coordinate {
        Coordinate::from_degrees(latitude, longitude).expect("coordinate is valid")
    }

    fn discovery() -> ChargingDiscovery<Filter> {
        ChargingDiscovery::new(Filter::default())
    }

    #[test]
    fn location_before_viewport_waits_and_empty_viewport_starts_one_search() {
        let discovery = discovery();
        assert_eq!(discovery.location_available(location(39.74, -104.99)), None);
        let request = discovery
            .viewport_loaded(false)
            .expect("empty viewport starts search");
        assert_eq!(request.origin, location(39.74, -104.99));
        assert_eq!(request.filter, Filter::default());
        assert_eq!(request.limit, 1);
        assert!(discovery.viewport_loaded(false).is_none());
    }

    #[test]
    fn viewport_and_location_can_arrive_in_either_order_and_local_pins_finish() {
        let location_first = discovery();
        assert!(location_first.viewport_loaded(false).is_none());
        assert!(
            location_first
                .location_available(location(39.0, -105.0))
                .is_some()
        );

        let viewport_first = discovery();
        assert!(
            viewport_first
                .location_available(location(39.0, -105.0))
                .is_none()
        );
        assert!(viewport_first.viewport_loaded(false).is_some());

        let populated = discovery();
        assert!(populated.viewport_loaded(true).is_none());
        assert!(
            populated
                .location_available(location(39.0, -105.0))
                .is_none()
        );
    }

    #[test]
    fn waiting_viewport_updates_can_finish_with_pins_or_remain_ready_for_location() {
        let populated = discovery();
        assert!(populated.viewport_loaded(false).is_none());
        assert!(populated.viewport_loaded(true).is_none());
        assert!(
            populated
                .location_available(location(39.0, -105.0))
                .is_none()
        );

        let still_empty = discovery();
        assert!(still_empty.viewport_loaded(false).is_none());
        assert!(still_empty.viewport_loaded(false).is_none());
        assert!(
            still_empty
                .location_available(location(39.0, -105.0))
                .is_some()
        );
    }

    #[test]
    fn empty_search_rearms_only_after_downloads_leave_local_cache_empty() {
        let discovery = discovery();
        discovery.location_available(location(39.0, -105.0));
        discovery
            .viewport_loaded(false)
            .expect("first search starts");
        discovery.search_completed(ChargingDiscoverySearchResult::Empty);
        assert!(
            discovery
                .location_available(location(39.1, -105.0))
                .is_none()
        );
        assert!(discovery.community_cache_updated(0, false).is_none());
        assert!(discovery.community_cache_updated(2, true).is_none());
        let request = discovery
            .community_cache_updated(2, false)
            .expect("downloaded cache without local results rearms search");
        assert_eq!(request.origin, location(39.1, -105.0));
        assert!(discovery.community_cache_updated(2, false).is_none());
    }

    #[test]
    fn a_nearest_result_emits_one_camera_target_and_overrides_prevent_rearm() {
        let discovery = discovery();
        discovery.location_available(location(39.0, -105.0));
        discovery.viewport_loaded(false).expect("search starts");
        let nearest = location(39.1, -105.1);
        assert_eq!(
            discovery.search_completed(ChargingDiscoverySearchResult::Nearest(nearest)),
            Some(nearest)
        );
        assert_eq!(
            discovery.search_completed(ChargingDiscoverySearchResult::Nearest(location(
                39.2, -105.2
            ))),
            None
        );
        assert!(discovery.community_cache_updated(1, false).is_none());

        let cancelled = ChargingDiscovery::new(Filter::default());
        cancelled.location_available(location(39.0, -105.0));
        cancelled.viewport_loaded(false).expect("search starts");
        cancelled.search_completed(ChargingDiscoverySearchResult::Empty);
        cancelled.user_override();
        assert!(cancelled.community_cache_updated(1, false).is_none());
    }

    #[test]
    fn changed_filter_cancels_pending_discovery_but_equal_filter_does_not() {
        let waiting = discovery();
        waiting.viewport_loaded(false);
        waiting.set_filter(Filter::default());
        assert!(waiting.location_available(location(39.0, -105.0)).is_some());

        let changed = discovery();
        changed.viewport_loaded(false);
        changed.set_filter(Filter(1));
        assert!(changed.location_available(location(39.0, -105.0)).is_none());
    }

    #[test]
    fn invalid_nearest_and_failed_or_cancelled_searches_do_not_rearm() {
        let invalid = discovery();
        invalid.location_available(location(39.0, -105.0));
        invalid.viewport_loaded(false).expect("search starts");
        assert_eq!(
            invalid.search_completed(ChargingDiscoverySearchResult::InvalidNearest),
            None
        );
        assert!(invalid.community_cache_updated(1, false).is_none());

        let failed = discovery();
        failed.location_available(location(39.0, -105.0));
        failed.viewport_loaded(false).expect("search starts");
        failed.search_failed();
        assert!(failed.community_cache_updated(1, false).is_none());

        let cancelled = discovery();
        cancelled.location_available(location(39.0, -105.0));
        cancelled.viewport_loaded(false).expect("search starts");
        cancelled.search_cancelled();
        assert!(cancelled.community_cache_updated(1, false).is_none());
    }

    #[test]
    fn search_bounds_cross_the_antimeridian_and_cover_polar_longitudes() {
        let antimeridian = discovery();
        antimeridian.location_available(location(0.0, 179.9));
        let bounds = antimeridian
            .viewport_loaded(false)
            .expect("search starts")
            .bounds;
        assert!(bounds.minimum_longitude_degrees() > bounds.maximum_longitude_degrees());
        assert!(bounds.minimum_longitude_degrees() > 179.0);
        assert!(bounds.maximum_longitude_degrees() < -179.0);
        assert!(bounds.maximum_latitude_degrees() - bounds.minimum_latitude_degrees() < 1.0);

        let polar = discovery();
        polar.location_available(location(90.0, 20.0));
        let bounds = polar.viewport_loaded(false).expect("search starts").bounds;
        assert!(bounds.minimum_latitude_degrees() < 90.0);
        assert_eq!(bounds.maximum_latitude_degrees(), 90.0);
        assert_eq!(bounds.minimum_longitude_degrees(), -180.0);
        assert_eq!(bounds.maximum_longitude_degrees(), 180.0);
    }
}
