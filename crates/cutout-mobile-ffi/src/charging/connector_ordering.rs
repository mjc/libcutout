use super::{
    MobileChargingConnectorDto, MobileChargingError, api::run_database, conversion::storage_error,
};
use crate::{MobileMapCoordinateDto, RideDatabaseHandle, mobile_map_coordinate, ride_maps};

#[uniffi::export(async_runtime = "tokio")]
impl RideDatabaseHandle {
    /// Returns all connectors ordered by nearby spot prevalence, then GPS country fallback.
    /// Reads the local database only; this operation sends no location to the server.
    /// # Errors
    /// Returns invalid location/country or local storage failures.
    pub async fn charging_connector_choices_near(
        &self,
        origin: MobileMapCoordinateDto,
        location_country_code: Option<String>,
    ) -> Result<Vec<MobileChargingConnectorDto>, MobileChargingError> {
        let origin = mobile_map_coordinate(origin).map_err(|_| MobileChargingError::InvalidPin)?;
        let country = location_country_code
            .as_deref()
            .map(ride_maps::ChargingCountry::new)
            .transpose()
            .map_err(|_| MobileChargingError::InvalidMetadata)?;
        run_database(self.inner.clone(), move |database| {
            database
                .charging_connector_choices_near(origin, country)
                .map(|choices| choices.into_iter().map(Into::into).collect())
                .map_err(storage_error)
        })
        .await
    }
}
