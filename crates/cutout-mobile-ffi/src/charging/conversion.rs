use super::{
    MobileChargingConnectorDto, MobileChargingError, MobileChargingFilterDto,
    MobileChargingInputDto, MobileChargingPriceDto, MobileChargingSpotDto, MobileChargingSupplyDto,
    MobileChargingTermsDto, MobileChargingVenueDto, MobileChargingWriteReceiptDto,
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

pub(super) fn input_dto(value: &ride_maps::NewChargingSpot) -> MobileChargingInputDto {
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
        pending_operations: value.pending_operations,
        pending_vote: value.pending_vote,
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
        venue: value.venue.map(Into::into),
    })
}

/// Applies the domain vote toggle for mobile selection controls.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_vote_selection(
    current: Option<super::MobileChargingVoteDto>,
    selected: super::MobileChargingVoteDto,
) -> Option<super::MobileChargingVoteDto> {
    let selected: ride_maps::ChargingVote = selected.into();
    let current = current.map(Into::into);
    selected.select(current).map(Into::into)
}

/// Suggests a household outlet supply, or an Unknown supply for an EV station.
/// Malformed and unsupported country codes have no country context and return Unknown.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_power_suggestion(
    country_code: Option<String>,
    ev_station: bool,
) -> MobileChargingSupplyDto {
    let country = country_code.and_then(|code| ride_maps::ChargingCountry::new(&code).ok());
    let kind = if ev_station {
        ride_maps::ChargingSupplyKind::EvStation
    } else {
        ride_maps::ChargingSupplyKind::Outlet
    };
    let draft = ride_maps::ChargingPowerDraft::new(country, kind);
    MobileChargingSupplyDto {
        voltage_volts: draft
            .selection()
            .voltage()
            .map(ride_maps::SupplyVoltage::as_u16),
        connectors: draft
            .selection()
            .connectors()
            .iter()
            .cloned()
            .map(Into::into)
            .collect(),
    }
}

/// Returns canonical household outlet choices, with the editable default first.
/// Malformed and unsupported country codes return the Unknown choice.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_outlet_choices(
    country_code: Option<String>,
) -> Vec<MobileChargingConnectorDto> {
    let country = country_code.and_then(|code| ride_maps::ChargingCountry::new(&code).ok());
    ride_maps::charging_outlet_choices(country.as_ref())
        .iter()
        .cloned()
        .map(Into::into)
        .collect()
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

    #[test]
    fn power_suggestions_follow_country_and_supply_kind() {
        assert_eq!(
            mobile_charging_power_suggestion(Some("US".into()), false),
            MobileChargingSupplyDto {
                voltage_volts: Some(120),
                connectors: vec![MobileChargingConnectorDto::Nema5_15],
            }
        );
        assert_eq!(
            mobile_charging_power_suggestion(Some("gb".into()), false),
            MobileChargingSupplyDto {
                voltage_volts: None,
                connectors: vec![MobileChargingConnectorDto::TypeG],
            }
        );
        for country_code in [Some("ZZ".to_owned()), Some("USA".to_owned()), None] {
            assert_eq!(
                mobile_charging_power_suggestion(country_code, false),
                MobileChargingSupplyDto {
                    voltage_volts: None,
                    connectors: vec![MobileChargingConnectorDto::Unknown],
                }
            );
        }
        assert_eq!(
            mobile_charging_power_suggestion(Some("US".into()), true),
            MobileChargingSupplyDto {
                voltage_volts: None,
                connectors: vec![MobileChargingConnectorDto::Unknown],
            }
        );
    }

    #[test]
    fn outlet_choices_use_the_canonical_country_catalogue() {
        assert_eq!(
            mobile_charging_outlet_choices(Some("us".into())),
            vec![
                MobileChargingConnectorDto::Nema5_15,
                MobileChargingConnectorDto::TypeA,
            ]
        );
        assert_eq!(
            mobile_charging_outlet_choices(Some("GB".into())),
            vec![MobileChargingConnectorDto::TypeG]
        );
        assert_eq!(
            mobile_charging_outlet_choices(Some("not-a-country".into())),
            vec![MobileChargingConnectorDto::Unknown]
        );
    }

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
    fn charging_spot_dto_preserves_outbox_pending_state() {
        let value = persistence::StoredChargingSpot {
            point_id: persistence::MapPointId::new(),
            input: None,
            owned: true,
            status: persistence::ChargingStatus::Queued,
            local_revision: persistence::ChargingRevision::new(1).unwrap(),
            remote_id: None,
            remote_revision: persistence::ChargingRevision::new(0).unwrap(),
            change_sequence: persistence::ChargingRevision::new(0).unwrap(),
            verified_at_ms: None,
            vote: Some(ride_maps::ChargingVote::Up),
            vote_revision: persistence::ChargingRevision::new(0).unwrap(),
            votes_up: 0,
            votes_down: 0,
            pending_operations: 2,
            pending_vote: true,
        };
        let dto = spot_dto(&value);
        assert_eq!(dto.pending_operations, 2);
        assert!(dto.pending_vote);
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

    #[test]
    fn venue_filter_converts_and_vote_selection_toggles() {
        let converted = filter(MobileChargingFilterDto {
            venue: Some(MobileChargingVenueDto::Cafe),
            ..MobileChargingFilterDto::default()
        })
        .unwrap();
        assert_eq!(converted.venue, Some(ride_maps::ChargingVenue::Cafe));

        assert_eq!(
            mobile_charging_vote_selection(None, crate::MobileChargingVoteDto::Up),
            Some(crate::MobileChargingVoteDto::Up)
        );
        assert_eq!(
            mobile_charging_vote_selection(
                Some(crate::MobileChargingVoteDto::Up),
                crate::MobileChargingVoteDto::Up,
            ),
            None
        );
        assert_eq!(
            mobile_charging_vote_selection(
                Some(crate::MobileChargingVoteDto::Up),
                crate::MobileChargingVoteDto::Down,
            ),
            Some(crate::MobileChargingVoteDto::Down)
        );
    }

    #[test]
    fn charging_venue_filter_converts_at_the_boundary() {
        let selection = filter(MobileChargingFilterDto {
            venue: Some(MobileChargingVenueDto::Cafe),
            ..MobileChargingFilterDto::default()
        })
        .unwrap();
        assert_eq!(selection.venue, Some(ride_maps::ChargingVenue::Cafe));
    }
}
