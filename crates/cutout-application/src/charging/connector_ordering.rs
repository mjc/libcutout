use cutout_ride_maps::{ChargingConnector, ChargingCountry, Coordinate};
use libcutout_persistence::{RideDatabase, StorageError};

/// Returns the complete connector catalogue ordered by locally stored nearby prevalence,
/// followed by the location country's fallback order.
///
/// The coordinate is used only against the local database; this operation does not transmit it.
///
/// # Errors
/// Returns the storage error encountered while reading local charging spots.
pub fn charging_connector_choices_near(
    database: &RideDatabase,
    origin: Coordinate,
    country: Option<ChargingCountry>,
) -> Result<Vec<ChargingConnector>, StorageError> {
    database.charging_connector_choices_near(origin, country)
}
