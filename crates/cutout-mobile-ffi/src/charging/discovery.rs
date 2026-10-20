use cutout_application::charging::discovery::{
    ChargingDiscovery as ApplicationChargingDiscovery, ChargingDiscoveryRequest,
    ChargingDiscoverySearchResult,
};

use crate::{MobileGeoBoundsDto, MobileMapCoordinateDto, ride_maps};

use super::MobileChargingFilterDto;

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

/// `UniFFI` adapter for the portable charging-discovery application service.
#[derive(Debug, uniffi::Object)]
pub struct MobileChargingDiscovery {
    discovery: ApplicationChargingDiscovery<MobileChargingFilterDto>,
}

#[uniffi::export]
impl MobileChargingDiscovery {
    /// Creates an eligible discovery policy with the initial charging filters.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(filter: MobileChargingFilterDto) -> Self {
        Self {
            discovery: ApplicationChargingDiscovery::new(filter),
        }
    }

    /// Records a local viewport result and starts discovery only when it is empty.
    pub fn viewport_loaded(
        &self,
        has_local_results: bool,
    ) -> Option<MobileChargingDiscoveryRequestDto> {
        self.discovery
            .viewport_loaded(has_local_results)
            .map(request_dto)
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
        self.discovery
            .location_available(coordinate)
            .map(request_dto)
    }

    /// Finishes the query and returns a validated nearest coordinate for one-time camera use.
    pub fn search_completed(
        &self,
        nearest: Option<MobileMapCoordinateDto>,
    ) -> Option<MobileMapCoordinateDto> {
        let result = match nearest {
            None => ChargingDiscoverySearchResult::Empty,
            Some(nearest) => match ride_maps::Coordinate::from_degrees(
                nearest.latitude_degrees,
                nearest.longitude_degrees,
            ) {
                Ok(coordinate) => ChargingDiscoverySearchResult::Nearest(coordinate),
                Err(_) => ChargingDiscoverySearchResult::InvalidNearest,
            },
        };
        self.discovery.search_completed(result).map(coordinate_dto)
    }

    /// Settles a failed query without qualifying for a download-triggered retry.
    pub fn search_failed(&self) {
        self.discovery.search_failed();
    }

    /// Settles cancellation of the in-flight query.
    pub fn search_cancelled(&self) {
        self.discovery.search_cancelled();
    }

    /// Applies a filter change. Once automatic discovery began, a filter change cancels it.
    pub fn set_filter(&self, filter: MobileChargingFilterDto) {
        self.discovery.set_filter(filter);
    }

    /// Permanently cancels automatic discovery after a user camera or filter action.
    pub fn user_override(&self) {
        self.discovery.user_override();
    }

    /// Rearms an empty search only after downloads add no pins to the current local result.
    pub fn community_cache_updated(
        &self,
        downloaded_count: u32,
        has_local_results: bool,
    ) -> Option<MobileChargingDiscoveryRequestDto> {
        self.discovery
            .community_cache_updated(downloaded_count, has_local_results)
            .map(request_dto)
    }
}

fn request_dto(
    request: ChargingDiscoveryRequest<MobileChargingFilterDto>,
) -> MobileChargingDiscoveryRequestDto {
    MobileChargingDiscoveryRequestDto {
        bounds: MobileGeoBoundsDto {
            minimum_latitude_degrees: request.bounds.minimum_latitude_degrees(),
            maximum_latitude_degrees: request.bounds.maximum_latitude_degrees(),
            minimum_longitude_degrees: request.bounds.minimum_longitude_degrees(),
            maximum_longitude_degrees: request.bounds.maximum_longitude_degrees(),
        },
        origin: coordinate_dto(request.origin),
        filter: request.filter,
        limit: request.limit,
    }
}

fn coordinate_dto(value: ride_maps::Coordinate) -> MobileMapCoordinateDto {
    MobileMapCoordinateDto {
        latitude_degrees: value.latitude_degrees(),
        longitude_degrees: value.longitude_degrees(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(latitude: f64, longitude: f64) -> MobileMapCoordinateDto {
        MobileMapCoordinateDto {
            latitude_degrees: latitude,
            longitude_degrees: longitude,
        }
    }

    #[test]
    fn request_preserves_raw_filter_values_and_validated_coordinates() {
        let filter = MobileChargingFilterDto {
            connector: Some(super::super::MobileChargingConnectorDto::Other {
                label: String::new(),
            }),
            voltage_volts: Some(0),
            ..MobileChargingFilterDto::default()
        };
        let discovery = MobileChargingDiscovery::new(filter.clone());
        assert!(
            discovery
                .location_available(location(39.0, -105.0))
                .is_none()
        );

        let request = discovery
            .viewport_loaded(false)
            .expect("empty viewport starts search");
        assert_eq!(request.origin, location(39.0, -105.0));
        assert_eq!(request.filter, filter);
        assert_eq!(request.limit, 1);
        assert!(request.bounds.minimum_latitude_degrees.is_finite());
        assert!(request.bounds.maximum_latitude_degrees.is_finite());
        assert!(request.bounds.minimum_longitude_degrees.is_finite());
        assert!(request.bounds.maximum_longitude_degrees.is_finite());
    }

    #[test]
    fn invalid_location_is_ignored_and_invalid_nearest_finishes_without_rearm() {
        let discovery = MobileChargingDiscovery::new(MobileChargingFilterDto::default());
        assert!(discovery.location_available(location(91.0, 0.0)).is_none());
        assert!(
            discovery
                .location_available(location(39.0, -105.0))
                .is_none()
        );
        discovery
            .viewport_loaded(false)
            .expect("valid location remains eligible");

        assert_eq!(discovery.search_completed(Some(location(91.0, 0.0))), None);
        assert!(discovery.community_cache_updated(1, false).is_none());
    }

    #[test]
    fn current_exported_methods_forward_filter_cancellation_and_camera_result() {
        let discovery = MobileChargingDiscovery::new(MobileChargingFilterDto::default());
        discovery.viewport_loaded(false);
        discovery.set_filter(MobileChargingFilterDto {
            voltage_volts: Some(120),
            ..MobileChargingFilterDto::default()
        });
        assert!(
            discovery
                .location_available(location(39.0, -105.0))
                .is_none()
        );

        let discovery = MobileChargingDiscovery::new(MobileChargingFilterDto::default());
        discovery.location_available(location(39.0, -105.0));
        discovery
            .viewport_loaded(false)
            .expect("empty viewport starts search");
        assert_eq!(
            discovery.search_completed(Some(location(39.1, -105.1))),
            Some(location(39.1, -105.1))
        );
        assert_eq!(
            discovery.search_completed(Some(location(39.2, -105.2))),
            None
        );
    }
}
