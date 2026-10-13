use super::conversion::{filter, input, point_id, receipt_dto, revision, spot_dto, storage_error};
use super::{
    MobileChargingError, MobileChargingFilterDto, MobileChargingInputDto, MobileChargingPageDto,
    MobileChargingSpotDto, MobileChargingVoteDto, MobileChargingWriteReceiptDto,
};
use crate::{
    MobileGeoBoundsDto, MobileMapCoordinateDto, MobileMapPointCursorDto, RideDatabaseHandle,
    mobile_geo_bounds, mobile_map_coordinate, mobile_query_limit, persistence,
};

async fn run_database<T, F>(
    database: persistence::RideDatabase,
    operation: F,
) -> Result<T, MobileChargingError>
where
    T: Send + 'static,
    F: FnOnce(persistence::RideDatabase) -> Result<T, MobileChargingError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || operation(database))
        .await
        .map_err(|_| MobileChargingError::Unavailable)?
}

#[uniffi::export(async_runtime = "tokio")]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary identifier strings"
)]
impl RideDatabaseHandle {
    /// Saves public fields and queues their community submission in one transaction.
    /// # Errors
    /// Returns field-specific validation, queue or storage errors.
    pub async fn save_charging_spot(
        &self,
        fields: MobileChargingInputDto,
    ) -> Result<MobileChargingWriteReceiptDto, MobileChargingError> {
        let fields = input(fields)?;
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .save_charging_spot(fields)
                .map(receipt_dto)
                .map_err(storage_error)
        })
        .await
    }

    /// Replaces public fields and moves the same pin using a checked local revision.
    /// # Errors
    /// Returns validation, ownership, stale-revision, queue or storage errors.
    pub async fn edit_charging_spot(
        &self,
        id: String,
        expected_revision: u64,
        fields: MobileChargingInputDto,
    ) -> Result<MobileChargingWriteReceiptDto, MobileChargingError> {
        let id = point_id(&id)?;
        let revision = revision(expected_revision)?;
        let fields = input(fields)?;
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .edit_charging_spot(id, revision, fields)
                .map(receipt_dto)
                .map_err(storage_error)
        })
        .await
    }

    /// Withdraws an owned spot and durably queues that request.
    /// # Errors
    /// Returns ownership, stale-revision, queue or storage errors.
    pub async fn withdraw_charging_spot(
        &self,
        id: String,
        expected_revision: u64,
    ) -> Result<MobileChargingWriteReceiptDto, MobileChargingError> {
        let id = point_id(&id)?;
        let revision = revision(expected_revision)?;
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .withdraw_charging_spot(id, revision)
                .map(receipt_dto)
                .map_err(storage_error)
        })
        .await
    }

    /// Queues an explicit access verification; confirmation comes from the server.
    /// # Errors
    /// Returns stale-revision, queue or storage errors.
    pub async fn verify_charging_spot(
        &self,
        id: String,
        expected_revision: u64,
    ) -> Result<MobileChargingWriteReceiptDto, MobileChargingError> {
        let id = point_id(&id)?;
        let revision = revision(expected_revision)?;
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .verify_charging_spot(id, revision)
                .map(receipt_dto)
                .map_err(storage_error)
        })
        .await
    }

    /// Sets or clears the pin vote without increasing confirmed counts locally.
    /// # Errors
    /// Returns stale-revision, queue or storage errors.
    pub async fn vote_charging_spot(
        &self,
        id: String,
        expected_revision: u64,
        vote: Option<MobileChargingVoteDto>,
    ) -> Result<MobileChargingWriteReceiptDto, MobileChargingError> {
        let id = point_id(&id)?;
        let revision = revision(expected_revision)?;
        let vote = vote.map(Into::into);
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .vote_charging_spot(id, revision, vote)
                .map(receipt_dto)
                .map_err(storage_error)
        })
        .await
    }

    /// Reads charging detail; an old map point has no charging detail.
    /// # Errors
    /// Returns invalid-identifier or storage errors.
    pub async fn charging_spot(
        &self,
        id: String,
    ) -> Result<Option<MobileChargingSpotDto>, MobileChargingError> {
        let id = point_id(&id)?;
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .charging_spot(id)
                .map(|value| value.as_ref().map(spot_dto))
                .map_err(storage_error)
        })
        .await
    }

    /// Returns a bounded filtered map page, including boxes across the antimeridian.
    /// # Errors
    /// Returns bounds, filter, cursor, limit or storage errors.
    pub async fn charging_spots_in_bounds(
        &self,
        bounds: MobileGeoBoundsDto,
        selection: MobileChargingFilterDto,
        cursor: Option<MobileMapPointCursorDto>,
        limit: u32,
    ) -> Result<MobileChargingPageDto, MobileChargingError> {
        let bounds = mobile_geo_bounds(bounds).map_err(|_| MobileChargingError::InvalidPin)?;
        let selection = filter(selection)?;
        let cursor = cursor
            .map(|value| point_id(&value.id).map(persistence::MapPointCursor::new))
            .transpose()?;
        let limit = mobile_query_limit(limit).map_err(|_| MobileChargingError::InvalidLimit)?;
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .charging_spots_in_bounds(bounds, selection, cursor, limit)
                .map(|page| MobileChargingPageDto {
                    spots: page.spots.iter().map(spot_dto).collect(),
                    next_cursor: page.next_cursor.map(|cursor| MobileMapPointCursorDto {
                        id: cursor.id().uuid().to_string(),
                    }),
                })
                .map_err(storage_error)
        })
        .await
    }

    /// Returns the nearest matching pins after exact Rust-owned distance ranking.
    /// # Errors
    /// Returns bounds, pin, filter, limit or storage errors.
    pub async fn nearby_charging_spots(
        &self,
        bounds: MobileGeoBoundsDto,
        origin: MobileMapCoordinateDto,
        selection: MobileChargingFilterDto,
        limit: u32,
    ) -> Result<Vec<MobileChargingSpotDto>, MobileChargingError> {
        let bounds = mobile_geo_bounds(bounds).map_err(|_| MobileChargingError::InvalidPin)?;
        let origin = mobile_map_coordinate(origin).map_err(|_| MobileChargingError::InvalidPin)?;
        let selection = filter(selection)?;
        let limit = mobile_query_limit(limit).map_err(|_| MobileChargingError::InvalidLimit)?;
        let database = self.inner.clone();
        run_database(database, move |database| {
            database
                .nearby_charging_spots(bounds, origin, selection, limit)
                .map(|values| values.iter().map(spot_dto).collect())
                .map_err(storage_error)
        })
        .await
    }
}
