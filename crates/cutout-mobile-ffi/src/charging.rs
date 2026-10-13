//! Typed charging product commands through the shared mobile database handle.
use crate::{
    MobileGeoBoundsDto, MobileMapCoordinateDto, MobileMapPointCursorDto, RideDatabaseHandle,
    mobile_geo_bounds, mobile_map_coordinate, mobile_map_coordinate_dto, mobile_query_limit,
    persistence, ride_maps,
};
use uuid::Uuid;

/// Typed cost value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingCostDto {
    /// `Free` value.
    Free,
    /// `Paid` value.
    Paid,
}
impl From<ride_maps::ChargingCost> for MobileChargingCostDto {
    fn from(value: ride_maps::ChargingCost) -> Self {
        match value {
            ride_maps::ChargingCost::Free => Self::Free,
            ride_maps::ChargingCost::Paid => Self::Paid,
        }
    }
}
impl From<MobileChargingCostDto> for ride_maps::ChargingCost {
    fn from(value: MobileChargingCostDto) -> Self {
        match value {
            MobileChargingCostDto::Free => Self::Free,
            MobileChargingCostDto::Paid => Self::Paid,
        }
    }
}
/// Typed venue value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingVenueDto {
    /// `Restaurant` value.
    Restaurant,
    /// `Cafe` value.
    Cafe,
    /// `Bar` value.
    Bar,
    /// `EvChargingStation` value.
    EvChargingStation,
    /// `Other` value.
    Other,
}
impl From<ride_maps::ChargingVenue> for MobileChargingVenueDto {
    fn from(value: ride_maps::ChargingVenue) -> Self {
        match value {
            ride_maps::ChargingVenue::Restaurant => Self::Restaurant,
            ride_maps::ChargingVenue::Cafe => Self::Cafe,
            ride_maps::ChargingVenue::Bar => Self::Bar,
            ride_maps::ChargingVenue::EvChargingStation => Self::EvChargingStation,
            ride_maps::ChargingVenue::Other => Self::Other,
        }
    }
}
impl From<MobileChargingVenueDto> for ride_maps::ChargingVenue {
    fn from(value: MobileChargingVenueDto) -> Self {
        match value {
            MobileChargingVenueDto::Restaurant => Self::Restaurant,
            MobileChargingVenueDto::Cafe => Self::Cafe,
            MobileChargingVenueDto::Bar => Self::Bar,
            MobileChargingVenueDto::EvChargingStation => Self::EvChargingStation,
            MobileChargingVenueDto::Other => Self::Other,
        }
    }
}
/// Typed terms value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingTermsDto {
    /// `DirectFee` value.
    DirectFee,
    /// `PurchaseRequired` value.
    PurchaseRequired,
    /// `Both` value.
    Both,
}
impl From<ride_maps::ChargingPaidTerms> for MobileChargingTermsDto {
    fn from(value: ride_maps::ChargingPaidTerms) -> Self {
        match value {
            ride_maps::ChargingPaidTerms::DirectFee => Self::DirectFee,
            ride_maps::ChargingPaidTerms::PurchaseRequired => Self::PurchaseRequired,
            ride_maps::ChargingPaidTerms::Both => Self::Both,
        }
    }
}
impl From<MobileChargingTermsDto> for ride_maps::ChargingPaidTerms {
    fn from(value: MobileChargingTermsDto) -> Self {
        match value {
            MobileChargingTermsDto::DirectFee => Self::DirectFee,
            MobileChargingTermsDto::PurchaseRequired => Self::PurchaseRequired,
            MobileChargingTermsDto::Both => Self::Both,
        }
    }
}
/// Typed pricebasis value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingPriceBasisDto {
    /// `PerCharge` value.
    PerCharge,
    /// `PerHour` value.
    PerHour,
    /// `PerKwh` value.
    PerKwh,
    /// `Purchase` value.
    Purchase,
}
impl From<ride_maps::ChargingPriceBasis> for MobileChargingPriceBasisDto {
    fn from(value: ride_maps::ChargingPriceBasis) -> Self {
        match value {
            ride_maps::ChargingPriceBasis::PerCharge => Self::PerCharge,
            ride_maps::ChargingPriceBasis::PerHour => Self::PerHour,
            ride_maps::ChargingPriceBasis::PerKwh => Self::PerKwh,
            ride_maps::ChargingPriceBasis::Purchase => Self::Purchase,
        }
    }
}
impl From<MobileChargingPriceBasisDto> for ride_maps::ChargingPriceBasis {
    fn from(value: MobileChargingPriceBasisDto) -> Self {
        match value {
            MobileChargingPriceBasisDto::PerCharge => Self::PerCharge,
            MobileChargingPriceBasisDto::PerHour => Self::PerHour,
            MobileChargingPriceBasisDto::PerKwh => Self::PerKwh,
            MobileChargingPriceBasisDto::Purchase => Self::Purchase,
        }
    }
}
/// Typed vote value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingVoteDto {
    /// `Up` value.
    Up,
    /// `Down` value.
    Down,
}
impl From<ride_maps::ChargingVote> for MobileChargingVoteDto {
    fn from(value: ride_maps::ChargingVote) -> Self {
        match value {
            ride_maps::ChargingVote::Up => Self::Up,
            ride_maps::ChargingVote::Down => Self::Down,
        }
    }
}
impl From<MobileChargingVoteDto> for ride_maps::ChargingVote {
    fn from(value: MobileChargingVoteDto) -> Self {
        match value {
            MobileChargingVoteDto::Up => Self::Up,
            MobileChargingVoteDto::Down => Self::Down,
        }
    }
}
/// Typed status value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingStatusDto {
    /// `Queued` value.
    Queued,
    /// `Published` value.
    Published,
    /// `Hidden` value.
    Hidden,
    /// `Withdrawn` value.
    Withdrawn,
    /// `Removed` value.
    Removed,
}
impl From<persistence::ChargingStatus> for MobileChargingStatusDto {
    fn from(value: persistence::ChargingStatus) -> Self {
        match value {
            persistence::ChargingStatus::Queued => Self::Queued,
            persistence::ChargingStatus::Published => Self::Published,
            persistence::ChargingStatus::Hidden => Self::Hidden,
            persistence::ChargingStatus::Withdrawn => Self::Withdrawn,
            persistence::ChargingStatus::Removed => Self::Removed,
        }
    }
}
impl From<MobileChargingStatusDto> for persistence::ChargingStatus {
    fn from(value: MobileChargingStatusDto) -> Self {
        match value {
            MobileChargingStatusDto::Queued => Self::Queued,
            MobileChargingStatusDto::Published => Self::Published,
            MobileChargingStatusDto::Hidden => Self::Hidden,
            MobileChargingStatusDto::Withdrawn => Self::Withdrawn,
            MobileChargingStatusDto::Removed => Self::Removed,
        }
    }
}
/// Confirmed connector selection, including a bounded custom label.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingConnectorDto {
    /// `TypeA` connector.
    TypeA,
    /// `TypeB` connector.
    TypeB,
    /// `TypeC` connector.
    TypeC,
    /// `TypeD` connector.
    TypeD,
    /// `TypeE` connector.
    TypeE,
    /// `TypeF` connector.
    TypeF,
    /// `TypeG` connector.
    TypeG,
    /// `TypeH` connector.
    TypeH,
    /// `TypeI` connector.
    TypeI,
    /// `TypeJ` connector.
    TypeJ,
    /// `TypeK` connector.
    TypeK,
    /// `TypeL` connector.
    TypeL,
    /// `TypeM` connector.
    TypeM,
    /// `TypeN` connector.
    TypeN,
    /// `TypeO` connector.
    TypeO,
    /// `Nema5_15` connector.
    Nema5_15,
    /// `Nema5_20` connector.
    Nema5_20,
    /// `Nema14_50` connector.
    Nema14_50,
    /// `J1772` connector.
    J1772,
    /// `Type2` connector.
    Type2,
    /// `Ccs1` connector.
    Ccs1,
    /// `Ccs2` connector.
    Ccs2,
    /// `Chademo` connector.
    Chademo,
    /// `Nacs` connector.
    Nacs,
    /// `Unknown` connector.
    Unknown,
    /// Custom connector label.
    Other { label: String },
}
impl TryFrom<MobileChargingConnectorDto> for ride_maps::ChargingConnector {
    type Error = MobileChargingError;
    fn try_from(value: MobileChargingConnectorDto) -> Result<Self, Self::Error> {
        Ok(match value {
            MobileChargingConnectorDto::TypeA => Self::TypeA,
            MobileChargingConnectorDto::TypeB => Self::TypeB,
            MobileChargingConnectorDto::TypeC => Self::TypeC,
            MobileChargingConnectorDto::TypeD => Self::TypeD,
            MobileChargingConnectorDto::TypeE => Self::TypeE,
            MobileChargingConnectorDto::TypeF => Self::TypeF,
            MobileChargingConnectorDto::TypeG => Self::TypeG,
            MobileChargingConnectorDto::TypeH => Self::TypeH,
            MobileChargingConnectorDto::TypeI => Self::TypeI,
            MobileChargingConnectorDto::TypeJ => Self::TypeJ,
            MobileChargingConnectorDto::TypeK => Self::TypeK,
            MobileChargingConnectorDto::TypeL => Self::TypeL,
            MobileChargingConnectorDto::TypeM => Self::TypeM,
            MobileChargingConnectorDto::TypeN => Self::TypeN,
            MobileChargingConnectorDto::TypeO => Self::TypeO,
            MobileChargingConnectorDto::Nema5_15 => Self::Nema5_15,
            MobileChargingConnectorDto::Nema5_20 => Self::Nema5_20,
            MobileChargingConnectorDto::Nema14_50 => Self::Nema14_50,
            MobileChargingConnectorDto::J1772 => Self::J1772,
            MobileChargingConnectorDto::Type2 => Self::Type2,
            MobileChargingConnectorDto::Ccs1 => Self::Ccs1,
            MobileChargingConnectorDto::Ccs2 => Self::Ccs2,
            MobileChargingConnectorDto::Chademo => Self::Chademo,
            MobileChargingConnectorDto::Nacs => Self::Nacs,
            MobileChargingConnectorDto::Unknown => Self::Unknown,
            MobileChargingConnectorDto::Other { label } => Self::Other(
                ride_maps::ConnectorLabel::new(&label)
                    .map_err(|_| MobileChargingError::InvalidMetadata)?,
            ),
        })
    }
}
impl From<ride_maps::ChargingConnector> for MobileChargingConnectorDto {
    fn from(value: ride_maps::ChargingConnector) -> Self {
        match value {
            ride_maps::ChargingConnector::TypeA => Self::TypeA,
            ride_maps::ChargingConnector::TypeB => Self::TypeB,
            ride_maps::ChargingConnector::TypeC => Self::TypeC,
            ride_maps::ChargingConnector::TypeD => Self::TypeD,
            ride_maps::ChargingConnector::TypeE => Self::TypeE,
            ride_maps::ChargingConnector::TypeF => Self::TypeF,
            ride_maps::ChargingConnector::TypeG => Self::TypeG,
            ride_maps::ChargingConnector::TypeH => Self::TypeH,
            ride_maps::ChargingConnector::TypeI => Self::TypeI,
            ride_maps::ChargingConnector::TypeJ => Self::TypeJ,
            ride_maps::ChargingConnector::TypeK => Self::TypeK,
            ride_maps::ChargingConnector::TypeL => Self::TypeL,
            ride_maps::ChargingConnector::TypeM => Self::TypeM,
            ride_maps::ChargingConnector::TypeN => Self::TypeN,
            ride_maps::ChargingConnector::TypeO => Self::TypeO,
            ride_maps::ChargingConnector::Nema5_15 => Self::Nema5_15,
            ride_maps::ChargingConnector::Nema5_20 => Self::Nema5_20,
            ride_maps::ChargingConnector::Nema14_50 => Self::Nema14_50,
            ride_maps::ChargingConnector::J1772 => Self::J1772,
            ride_maps::ChargingConnector::Type2 => Self::Type2,
            ride_maps::ChargingConnector::Ccs1 => Self::Ccs1,
            ride_maps::ChargingConnector::Ccs2 => Self::Ccs2,
            ride_maps::ChargingConnector::Chademo => Self::Chademo,
            ride_maps::ChargingConnector::Nacs => Self::Nacs,
            ride_maps::ChargingConnector::Unknown => Self::Unknown,
            ride_maps::ChargingConnector::Other(label) => Self::Other {
                label: label.as_str().to_owned(),
            },
        }
    }
}

/// Exact optional amount; no floating-point currency conversion.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingPriceDto {
    pub amount_minor: u64,
    pub currency: String,
    pub minor_unit_exponent: u8,
    pub basis: MobileChargingPriceBasisDto,
}
/// Confirmed voltage and connectors from the same supply group.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingSupplyDto {
    pub voltage_volts: Option<u16>,
    pub connectors: Vec<MobileChargingConnectorDto>,
}
/// Public charging fields; only pin, access note and cost are required.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingInputDto {
    pub coordinate: MobileMapCoordinateDto,
    pub access_note: String,
    pub cost: MobileChargingCostDto,
    pub name: Option<String>,
    pub venue: Option<MobileChargingVenueDto>,
    pub hours: Option<String>,
    pub terms: Option<MobileChargingTermsDto>,
    pub price: Option<MobileChargingPriceDto>,
    pub supplies: Vec<MobileChargingSupplyDto>,
}
/// Detail read from the existing Rust database owner.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingSpotDto {
    pub id: String,
    pub input: Option<MobileChargingInputDto>,
    pub owned: bool,
    pub status: MobileChargingStatusDto,
    pub local_revision: u64,
    pub remote_id: Option<String>,
    pub remote_revision: u64,
    pub change_sequence: u64,
    pub verified_at_ms: Option<u64>,
    pub vote: Option<MobileChargingVoteDto>,
    pub vote_revision: u64,
    pub votes_up: u64,
    pub votes_down: u64,
}
/// Durable local receipt; publication requires a separate server receipt.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingWriteReceiptDto {
    pub id: String,
    pub local_revision: u64,
    pub operation_id: String,
}
/// Discovery filters applied in Rust before pagination/ranking.
#[derive(Clone, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingFilterDto {
    pub cost: Option<MobileChargingCostDto>,
    pub connector: Option<MobileChargingConnectorDto>,
    pub voltage_volts: Option<u16>,
}
/// Bounded discovery page using the existing map-point cursor.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingPageDto {
    pub spots: Vec<MobileChargingSpotDto>,
    pub next_cursor: Option<MobileMapPointCursorDto>,
}
/// Stable error categories for field validation, conflicts and durable storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileChargingError {
    #[error("Invalid map pin")]
    InvalidPin,
    #[error("Access note is blank")]
    AccessNoteBlank,
    #[error("Access note is too long")]
    AccessNoteTooLong,
    #[error("Access note contains unsupported characters")]
    AccessNoteControlCharacter,
    #[error("Invalid charging details")]
    InvalidMetadata,
    #[error("Invalid identifier")]
    InvalidIdentifier,
    #[error("Invalid page limit")]
    InvalidLimit,
    #[error("Spot changed")]
    Conflict,
    #[error("Spot belongs to another contributor")]
    NotOwned,
    #[error("Spot not found")]
    NotFound,
    #[error("Submission queue is full")]
    OutboxFull,
    #[error("Storage is unavailable")]
    Unavailable,
    #[error("Could not save or read the spot")]
    StorageFailure,
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "Consumes errors returned by the database request"
)]
fn storage_error(value: persistence::StorageError) -> MobileChargingError {
    match value {
        persistence::StorageError::ChargingInvalidInput => MobileChargingError::InvalidMetadata,
        persistence::StorageError::ChargingConflict => MobileChargingError::Conflict,
        persistence::StorageError::ChargingNotOwned => MobileChargingError::NotOwned,
        persistence::StorageError::ChargingOutboxFull => MobileChargingError::OutboxFull,
        persistence::StorageError::NotFound => MobileChargingError::NotFound,
        persistence::StorageError::QueueFull
        | persistence::StorageError::WorkerStopped
        | persistence::StorageError::ResponseDropped
        | persistence::StorageError::DeadlineExceeded
        | persistence::StorageError::SpatialCapabilityUnavailable => {
            MobileChargingError::Unavailable
        }
        _ => MobileChargingError::StorageFailure,
    }
}
fn point_id(value: &str) -> Result<persistence::MapPointId, MobileChargingError> {
    Uuid::parse_str(value)
        .map(persistence::MapPointId::from_uuid)
        .map_err(|_| MobileChargingError::InvalidIdentifier)
}
fn revision(value: u64) -> Result<persistence::ChargingRevision, MobileChargingError> {
    persistence::ChargingRevision::new(value).map_err(|_| MobileChargingError::Conflict)
}
fn supply(
    value: MobileChargingSupplyDto,
) -> Result<ride_maps::ChargingSupply, MobileChargingError> {
    ride_maps::ChargingSupply::new(
        value
            .voltage_volts
            .map(ride_maps::SupplyVoltage::new)
            .transpose()
            .map_err(|_| MobileChargingError::InvalidMetadata)?,
        value
            .connectors
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?,
    )
    .map_err(|_| MobileChargingError::InvalidMetadata)
}
fn input(value: MobileChargingInputDto) -> Result<ride_maps::NewChargingSpot, MobileChargingError> {
    let coordinate =
        mobile_map_coordinate(value.coordinate).map_err(|_| MobileChargingError::InvalidPin)?;
    let minimum =
        ride_maps::NewChargingSpot::new(coordinate, &value.access_note, value.cost.into())
            .map_err(|error| match error {
                ride_maps::AccessNoteError::Blank => MobileChargingError::AccessNoteBlank,
                ride_maps::AccessNoteError::TooLong => MobileChargingError::AccessNoteTooLong,
                ride_maps::AccessNoteError::ControlCharacter => {
                    MobileChargingError::AccessNoteControlCharacter
                }
            })?;
    let price = value
        .price
        .map(|price| {
            ride_maps::ChargingPrice::new(
                price.amount_minor,
                &price.currency,
                price.minor_unit_exponent,
                price.basis.into(),
            )
        })
        .transpose()
        .map_err(|_| MobileChargingError::InvalidMetadata)?;
    let details = ride_maps::ChargingDetails::new(
        value.name.as_deref(),
        value.venue.map(Into::into),
        value.hours.as_deref(),
        value.terms.map(Into::into),
        price,
        value
            .supplies
            .into_iter()
            .map(supply)
            .collect::<Result<_, _>>()?,
    )
    .map_err(|_| MobileChargingError::InvalidMetadata)?;
    minimum
        .with_details(details)
        .map_err(|_| MobileChargingError::InvalidMetadata)
}
fn input_dto(value: &ride_maps::NewChargingSpot) -> MobileChargingInputDto {
    let details = value.details();
    MobileChargingInputDto {
        coordinate: mobile_map_coordinate_dto(value.coordinate()),
        access_note: value.access_note().as_str().to_owned(),
        cost: value.cost().into(),
        name: details.name().map(str::to_owned),
        venue: details.venue().map(Into::into),
        hours: details.hours().map(str::to_owned),
        terms: details.paid_terms().map(Into::into),
        price: details.price().map(|price| MobileChargingPriceDto {
            amount_minor: price.amount_minor(),
            currency: price.currency().to_owned(),
            minor_unit_exponent: price.minor_unit_exponent(),
            basis: price.basis().into(),
        }),
        supplies: details
            .supplies()
            .iter()
            .map(|supply| MobileChargingSupplyDto {
                voltage_volts: supply.voltage().map(ride_maps::SupplyVoltage::as_u16),
                connectors: supply
                    .connectors()
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect(),
            })
            .collect(),
    }
}
fn spot_dto(value: &persistence::StoredChargingSpot) -> MobileChargingSpotDto {
    MobileChargingSpotDto {
        id: value.point_id.uuid().to_string(),
        input: value.input.as_ref().map(input_dto),
        owned: value.owned,
        status: value.status.into(),
        local_revision: value.local_revision.get(),
        remote_id: value.remote_id.map(|id| id.as_uuid().to_string()),
        remote_revision: value.remote_revision.get(),
        change_sequence: value.change_sequence.get(),
        verified_at_ms: value.verified_at_ms,
        vote: value.vote.map(Into::into),
        vote_revision: value.vote_revision.get(),
        votes_up: value.votes_up,
        votes_down: value.votes_down,
    }
}
fn receipt_dto(value: persistence::ChargingWriteReceipt) -> MobileChargingWriteReceiptDto {
    MobileChargingWriteReceiptDto {
        id: value.point_id.uuid().to_string(),
        local_revision: value.revision.get(),
        operation_id: value.operation_id.uuid().to_string(),
    }
}
fn filter(
    value: MobileChargingFilterDto,
) -> Result<persistence::ChargingFilter, MobileChargingError> {
    Ok(persistence::ChargingFilter {
        cost: value.cost.map(Into::into),
        connector: value.connector.map(TryInto::try_into).transpose()?,
        voltage: value
            .voltage_volts
            .map(ride_maps::SupplyVoltage::new)
            .transpose()
            .map_err(|_| MobileChargingError::InvalidMetadata)?,
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    fn fields() -> MobileChargingInputDto {
        MobileChargingInputDto {
            coordinate: MobileMapCoordinateDto {
                latitude_degrees: 40.0,
                longitude_degrees: -105.0,
            },
            access_note: " Patio outlet ".into(),
            cost: MobileChargingCostDto::Free,
            name: None,
            venue: None,
            hours: None,
            terms: None,
            price: None,
            supplies: vec![],
        }
    }
    #[test]
    fn charging_dtos_preserve_absence_pairings_and_custom_connector_labels() {
        let minimum = input(fields()).unwrap();
        let roundtrip = input_dto(&minimum);
        assert_eq!(roundtrip.access_note, "Patio outlet");
        assert_eq!(roundtrip.supplies, []);
        assert!(roundtrip.name.is_none());
        assert!(roundtrip.price.is_none());
        let mut full = fields();
        full.cost = MobileChargingCostDto::Paid;
        full.name = Some("Coffee shop".into());
        full.venue = Some(MobileChargingVenueDto::Cafe);
        full.terms = Some(MobileChargingTermsDto::PurchaseRequired);
        full.price = Some(MobileChargingPriceDto {
            amount_minor: 125,
            currency: "USD".into(),
            minor_unit_exponent: 2,
            basis: MobileChargingPriceBasisDto::Purchase,
        });
        full.supplies = vec![
            MobileChargingSupplyDto {
                voltage_volts: Some(120),
                connectors: vec![MobileChargingConnectorDto::Nema5_15],
            },
            MobileChargingSupplyDto {
                voltage_volts: None,
                connectors: vec![MobileChargingConnectorDto::Other {
                    label: "Board charger".into(),
                }],
            },
        ];
        let value = input(full.clone()).unwrap();
        full.access_note = "Patio outlet".into();
        assert_eq!(input_dto(&value), full);
    }
    #[test]
    fn charging_boundary_classifies_fields_before_any_write() {
        let mut value = fields();
        value.access_note = " \n ".into();
        assert_eq!(input(value), Err(MobileChargingError::AccessNoteBlank));
        let mut value = fields();
        value.access_note = "a".repeat(2001);
        assert_eq!(input(value), Err(MobileChargingError::AccessNoteTooLong));
        let mut value = fields();
        value.access_note = "bad\0note".into();
        assert_eq!(
            input(value),
            Err(MobileChargingError::AccessNoteControlCharacter)
        );
        let mut value = fields();
        value.coordinate.latitude_degrees = f64::NAN;
        assert_eq!(input(value), Err(MobileChargingError::InvalidPin));
        let mut value = fields();
        value.supplies = vec![MobileChargingSupplyDto {
            voltage_volts: Some(0),
            connectors: vec![],
        }];
        assert_eq!(input(value), Err(MobileChargingError::InvalidMetadata));
        let mut value = fields();
        value.terms = Some(MobileChargingTermsDto::DirectFee);
        assert_eq!(input(value), Err(MobileChargingError::InvalidMetadata));
        assert_eq!(
            point_id("broken"),
            Err(MobileChargingError::InvalidIdentifier)
        );
        assert_eq!(
            storage_error(persistence::StorageError::ChargingConflict),
            MobileChargingError::Conflict
        );
        assert_eq!(
            storage_error(persistence::StorageError::WorkerStopped),
            MobileChargingError::Unavailable
        );
        assert_eq!(
            storage_error(persistence::StorageError::InvalidDatabaseIdentity),
            MobileChargingError::StorageFailure
        );
    }
}
