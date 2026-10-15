use std::sync::{Mutex, PoisonError};

use crate::{MobileGeoBoundsDto, MobileMapCoordinateDto, ride_maps};

use super::MobileChargingFilterDto;

const METRO_WINDOW_METERS: f64 = 100_000.0;
const SEARCH_LIMIT: u32 = 1;
const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// One automatic nearby query selected by charging-map discovery policy.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingDiscoveryRequestDto {
    /// Search bounds around the origin, including antimeridian crossing when needed.
    pub bounds: MobileGeoBoundsDto,
    /// Location used to rank nearby spots.
    pub origin: MobileMapCoordinateDto,
    /// Current charging-map filters.
    pub filter: MobileChargingFilterDto,
    /// Number of nearest results requested.
    pub limit: u32,
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

#[derive(Debug, Default)]
struct State {
    phase: Phase,
    origin: Option<ride_maps::Coordinate>,
    filter: MobileChargingFilterDto,
}

impl State {
    fn start_search(&mut self) -> Option<MobileChargingDiscoveryRequestDto> {
        if self.phase != Phase::AwaitingLocation {
            return None;
        }
        let origin = self.origin?;
        self.phase = Phase::Searching;
        Some(MobileChargingDiscoveryRequestDto {
            bounds: search_bounds(origin),
            origin: coordinate_dto(origin),
            filter: self.filter.clone(),
            limit: SEARCH_LIMIT,
        })
    }
}

/// Owns initial nearby-discovery state and emits a validated camera effect once.
#[derive(Debug, uniffi::Object)]
pub struct MobileChargingDiscovery {
    state: Mutex<State>,
}

#[uniffi::export]
impl MobileChargingDiscovery {
    /// Creates an eligible discovery policy with the initial charging filters.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(filter: MobileChargingFilterDto) -> Self {
        Self {
            state: Mutex::new(State {
                filter,
                ..State::default()
            }),
        }
    }

    /// Records a local viewport result and starts discovery only when it is empty.
    pub fn viewport_loaded(
        &self,
        has_local_results: bool,
    ) -> Option<MobileChargingDiscoveryRequestDto> {
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
    pub fn location_available(
        &self,
        location: MobileMapCoordinateDto,
    ) -> Option<MobileChargingDiscoveryRequestDto> {
        let coordinate = ride_maps::Coordinate::from_degrees(
            location.latitude_degrees,
            location.longitude_degrees,
        )
        .ok()?;
        let mut state = self.lock();
        state.origin = Some(coordinate);
        state.start_search()
    }

    /// Finishes the query and returns a validated nearest coordinate for one-time camera use.
    pub fn search_completed(
        &self,
        nearest: Option<MobileMapCoordinateDto>,
    ) -> Option<MobileMapCoordinateDto> {
        let mut state = self.lock();
        if state.phase != Phase::Searching {
            return None;
        }
        state.phase = if nearest.is_some() {
            Phase::Finished
        } else {
            Phase::Empty
        };
        nearest.and_then(valid_coordinate_dto)
    }

    /// Settles a failed query without qualifying for a download-triggered retry.
    pub fn search_failed(&self) {
        let mut state = self.lock();
        if state.phase != Phase::Searching {
            return;
        }
        state.phase = Phase::Finished;
    }

    /// Settles cancellation of the in-flight query.
    pub fn search_cancelled(&self) {
        let mut state = self.lock();
        if state.phase == Phase::Searching {
            state.phase = Phase::Finished;
        }
    }

    /// Applies a filter change. Once automatic discovery began, a filter change cancels it.
    pub fn set_filter(&self, filter: MobileChargingFilterDto) {
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
    ) -> Option<MobileChargingDiscoveryRequestDto> {
        let mut state = self.lock();
        if downloaded_count == 0 || has_local_results || state.phase != Phase::Empty {
            return None;
        }
        state.phase = Phase::AwaitingLocation;
        state.start_search()
    }
}

impl MobileChargingDiscovery {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn cancel(state: &mut State) {
    state.phase = Phase::Finished;
}

fn valid_coordinate_dto(value: MobileMapCoordinateDto) -> Option<MobileMapCoordinateDto> {
    ride_maps::Coordinate::from_degrees(value.latitude_degrees, value.longitude_degrees)
        .ok()
        .map(coordinate_dto)
}

fn coordinate_dto(value: ride_maps::Coordinate) -> MobileMapCoordinateDto {
    MobileMapCoordinateDto {
        latitude_degrees: value.latitude_degrees(),
        longitude_degrees: value.longitude_degrees(),
    }
}

fn search_bounds(origin: ride_maps::Coordinate) -> MobileGeoBoundsDto {
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
    MobileGeoBoundsDto {
        minimum_latitude_degrees: minimum_latitude,
        maximum_latitude_degrees: maximum_latitude,
        minimum_longitude_degrees: minimum_longitude,
        maximum_longitude_degrees: maximum_longitude,
    }
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

    fn filter() -> MobileChargingFilterDto {
        MobileChargingFilterDto::default()
    }

    fn location(latitude: f64, longitude: f64) -> MobileMapCoordinateDto {
        MobileMapCoordinateDto {
            latitude_degrees: latitude,
            longitude_degrees: longitude,
        }
    }

    #[test]
    fn location_before_viewport_waits_and_empty_viewport_starts_one_search() {
        let discovery = MobileChargingDiscovery::new(filter());
        assert_eq!(discovery.location_available(location(39.74, -104.99)), None);
        let request = discovery.viewport_loaded(false).unwrap();
        assert_eq!(request.origin, location(39.74, -104.99));
        assert_eq!(request.filter, filter());
        assert_eq!(request.limit, 1);
        assert!(discovery.viewport_loaded(false).is_none());
    }

    #[test]
    fn viewport_before_location_waits_and_initial_pins_settle_without_search() {
        let waiting = MobileChargingDiscovery::new(filter());
        assert!(waiting.viewport_loaded(false).is_none());
        assert!(waiting.location_available(location(39.0, -105.0)).is_some());

        let populated = MobileChargingDiscovery::new(filter());
        assert!(populated.viewport_loaded(true).is_none());
        assert!(
            populated
                .location_available(location(39.0, -105.0))
                .is_none()
        );
    }

    #[test]
    fn waiting_viewport_updates_can_finish_with_pins_or_remain_ready_for_location() {
        let populated = MobileChargingDiscovery::new(filter());
        assert!(populated.viewport_loaded(false).is_none());
        assert!(populated.viewport_loaded(true).is_none());
        assert!(
            populated
                .location_available(location(39.0, -105.0))
                .is_none()
        );
        let still_empty = MobileChargingDiscovery::new(filter());
        assert!(still_empty.viewport_loaded(false).is_none());
        assert!(still_empty.viewport_loaded(false).is_none());
        assert!(
            still_empty
                .location_available(location(39.0, -105.0))
                .is_some()
        );
    }

    #[test]
    fn empty_search_does_not_repeat_and_download_rearms_only_when_local_cache_is_empty() {
        let discovery = MobileChargingDiscovery::new(filter());
        discovery.location_available(location(39.0, -105.0));
        discovery.viewport_loaded(false).unwrap();
        discovery.search_completed(None);
        assert!(
            discovery
                .location_available(location(39.1, -105.0))
                .is_none()
        );
        assert!(discovery.community_cache_updated(0, false).is_none());
        assert!(discovery.community_cache_updated(2, true).is_none());
        let request = discovery.community_cache_updated(2, false).unwrap();
        assert_eq!(request.origin, location(39.1, -105.0));
        assert!(discovery.community_cache_updated(2, false).is_none());
    }

    #[test]
    fn completion_returns_a_camera_effect_once_and_overrides_prevent_rearm() {
        let discovery = MobileChargingDiscovery::new(filter());
        discovery.location_available(location(39.0, -105.0));
        discovery.viewport_loaded(false).unwrap();
        assert_eq!(
            discovery.search_completed(Some(location(39.1, -105.1))),
            Some(location(39.1, -105.1))
        );
        assert_eq!(
            discovery.search_completed(Some(location(39.2, -105.2))),
            None
        );
        assert!(discovery.community_cache_updated(1, false).is_none());

        let cancelled = MobileChargingDiscovery::new(filter());
        cancelled.location_available(location(39.0, -105.0));
        cancelled.viewport_loaded(false).unwrap();
        assert_eq!(cancelled.search_completed(None), None);
        cancelled.user_override();
        assert!(cancelled.community_cache_updated(1, false).is_none());

        let filtered = MobileChargingDiscovery::new(filter());
        filtered.viewport_loaded(false);
        filtered.set_filter(MobileChargingFilterDto {
            cost: Some(super::super::MobileChargingCostDto::Paid),
            ..filter()
        });
        assert!(
            filtered
                .location_available(location(39.0, -105.0))
                .is_none()
        );
    }

    #[test]
    fn invalid_nearest_coordinate_finishes_without_camera_effect_or_rearm() {
        let discovery = MobileChargingDiscovery::new(filter());
        discovery.location_available(location(39.0, -105.0));
        discovery.viewport_loaded(false).unwrap();
        assert_eq!(discovery.search_completed(Some(location(91.0, 0.0))), None);
        assert!(discovery.community_cache_updated(1, false).is_none());
    }

    #[test]
    fn request_bounds_cross_the_antimeridian_without_reversing_the_short_interval() {
        let request = MobileChargingDiscovery::new(filter());
        request.location_available(location(0.0, 179.9));
        let bounds = request.viewport_loaded(false).unwrap().bounds;
        assert!(bounds.minimum_longitude_degrees > bounds.maximum_longitude_degrees);
        assert!(bounds.minimum_longitude_degrees > 179.0);
        assert!(bounds.maximum_longitude_degrees < -179.0);
        assert!(bounds.maximum_latitude_degrees - bounds.minimum_latitude_degrees < 1.0);
    }

    #[test]
    fn request_bounds_near_pole_clamp_latitude_and_cover_all_longitudes() {
        let request = MobileChargingDiscovery::new(filter());
        request.location_available(location(90.0, 20.0));
        let bounds = request.viewport_loaded(false).unwrap().bounds;
        assert!(bounds.minimum_latitude_degrees < 90.0);
        assert_eq!(bounds.maximum_latitude_degrees, 90.0);
        assert_eq!(bounds.minimum_longitude_degrees, -180.0);
        assert_eq!(bounds.maximum_longitude_degrees, 180.0);
    }

    #[test]
    fn failure_and_cancellation_do_not_qualify_for_cache_rearm() {
        let failed = MobileChargingDiscovery::new(filter());
        failed.location_available(location(39.0, -105.0));
        failed.viewport_loaded(false).unwrap();
        failed.search_failed();
        assert!(failed.community_cache_updated(1, false).is_none());

        let cancelled = MobileChargingDiscovery::new(filter());
        cancelled.location_available(location(39.0, -105.0));
        cancelled.viewport_loaded(false).unwrap();
        cancelled.search_cancelled();
        assert!(cancelled.community_cache_updated(1, false).is_none());
    }
}
