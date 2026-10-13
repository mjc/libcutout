use super::{
    MobileChargingError, MobileChargingFilterDto, MobileChargingInputDto, MobileChargingPriceDto,
    MobileChargingSpotDto, MobileChargingSupplyDto, MobileChargingTermsDto, MobileChargingVenueDto,
    MobileChargingWriteReceiptDto,
};
use crate::{
    MobileMapCoordinateDto, mobile_map_coordinate, mobile_map_coordinate_dto, persistence,
    ride_maps,
};
use uuid::Uuid;

#[allow(
    clippy::needless_pass_by_value,
    reason = "Consumes errors returned by the database request"
)]
pub(super) fn storage_error(value: persistence::StorageError) -> MobileChargingError {
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
pub(super) fn point_id(value: &str) -> Result<persistence::MapPointId, MobileChargingError> {
    Uuid::parse_str(value)
        .map(persistence::MapPointId::from_uuid)
        .map_err(|_| MobileChargingError::InvalidIdentifier)
}
pub(super) fn revision(value: u64) -> Result<persistence::ChargingRevision, MobileChargingError> {
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
pub(super) fn input(
    value: MobileChargingInputDto,
) -> Result<ride_maps::NewChargingSpot, MobileChargingError> {
    let coordinate = charging_coordinate(value.coordinate)?;
    let minimum = charging_minimum(coordinate, &value.access_note, value.cost.into())?;
    let price = charging_price(value.price)?;
    let supplies = value
        .supplies
        .into_iter()
        .map(supply)
        .collect::<Result<_, _>>()?;
    let details = charging_details(
        value.name.as_deref(),
        value.venue,
        value.hours.as_deref(),
        value.terms,
        price,
        supplies,
    )?;

    minimum.with_details(details).map_err(invalid_metadata)
}

fn charging_coordinate(
    coordinate: MobileMapCoordinateDto,
) -> Result<ride_maps::Coordinate, MobileChargingError> {
    mobile_map_coordinate(coordinate).map_err(|_| MobileChargingError::InvalidPin)
}

fn charging_minimum(
    coordinate: ride_maps::Coordinate,
    access_note: &str,
    cost: ride_maps::ChargingCost,
) -> Result<ride_maps::NewChargingSpot, MobileChargingError> {
    ride_maps::NewChargingSpot::new(coordinate, access_note, cost).map_err(|error| match error {
        ride_maps::AccessNoteError::Blank => MobileChargingError::AccessNoteBlank,
        ride_maps::AccessNoteError::TooLong => MobileChargingError::AccessNoteTooLong,
        ride_maps::AccessNoteError::ControlCharacter => {
            MobileChargingError::AccessNoteControlCharacter
        }
    })
}

fn charging_price(
    price: Option<MobileChargingPriceDto>,
) -> Result<Option<ride_maps::ChargingPrice>, MobileChargingError> {
    price
        .map(|price| {
            ride_maps::ChargingPrice::new(
                price.amount_minor,
                &price.currency,
                price.minor_unit_exponent,
                price.basis.into(),
            )
        })
        .transpose()
        .map_err(invalid_metadata)
}

fn charging_details(
    name: Option<&str>,
    venue: Option<MobileChargingVenueDto>,
    hours: Option<&str>,
    terms: Option<MobileChargingTermsDto>,
    price: Option<ride_maps::ChargingPrice>,
    supplies: Vec<ride_maps::ChargingSupply>,
) -> Result<ride_maps::ChargingDetails, MobileChargingError> {
    ride_maps::ChargingDetails::new(
        name,
        venue.map(Into::into),
        hours,
        terms.map(Into::into),
        price,
        supplies,
    )
    .map_err(invalid_metadata)
}

fn invalid_metadata(_: impl std::fmt::Debug) -> MobileChargingError {
    MobileChargingError::InvalidMetadata
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
        price: details.price().map(price_dto),
        supplies: details.supplies().iter().map(supply_dto).collect(),
    }
}

fn price_dto(price: &ride_maps::ChargingPrice) -> MobileChargingPriceDto {
    MobileChargingPriceDto {
        amount_minor: price.amount_minor(),
        currency: price.currency().to_owned(),
        minor_unit_exponent: price.minor_unit_exponent(),
        basis: price.basis().into(),
    }
}

fn supply_dto(supply: &ride_maps::ChargingSupply) -> MobileChargingSupplyDto {
    MobileChargingSupplyDto {
        voltage_volts: supply.voltage().map(ride_maps::SupplyVoltage::as_u16),
        connectors: supply
            .connectors()
            .iter()
            .cloned()
            .map(Into::into)
            .collect(),
    }
}
pub(super) fn spot_dto(value: &persistence::StoredChargingSpot) -> MobileChargingSpotDto {
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
pub(super) fn receipt_dto(
    value: persistence::ChargingWriteReceipt,
) -> MobileChargingWriteReceiptDto {
    MobileChargingWriteReceiptDto {
        id: value.point_id.uuid().to_string(),
        local_revision: value.revision.get(),
        operation_id: value.operation_id.uuid().to_string(),
    }
}
pub(super) fn filter(
    value: MobileChargingFilterDto,
) -> Result<persistence::ChargingFilter, MobileChargingError> {
    let voltage = charging_voltage(value.voltage_volts)?;
    Ok(persistence::ChargingFilter {
        cost: value.cost.map(Into::into),
        connector: value.connector.map(TryInto::try_into).transpose()?,
        voltage,
    })
}

fn charging_voltage(
    voltage_volts: Option<u16>,
) -> Result<Option<ride_maps::SupplyVoltage>, MobileChargingError> {
    voltage_volts
        .map(ride_maps::SupplyVoltage::new)
        .transpose()
        .map_err(invalid_metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MobileChargingConnectorDto, MobileChargingCostDto, MobileChargingPriceBasisDto};
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
