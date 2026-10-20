use cutout_ride_maps::{
    AccessNoteError, ChargingConnector, ChargingCost, ChargingDetails, ChargingInputError,
    ChargingPaidTerms, ChargingPrice, ChargingPriceBasis, ChargingSupply, ChargingVenue,
    ConnectorLabel, Coordinate, NewChargingSpot, SupplyVoltage,
};

/// Raw connector row accepted from an editor before domain validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChargingEditorConnector {
    Known(ChargingConnector),
    Other(String),
}

/// Raw supply row accepted from an editor before domain validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingEditorSupply {
    pub voltage_volts: Option<u16>,
    pub connectors: Vec<ChargingEditorConnector>,
    pub outlet_count: Option<u16>,
}

/// Raw price data accepted from an editor before domain validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingEditorPrice {
    pub amount_minor: u64,
    pub currency: String,
    pub minor_unit_exponent: u8,
    pub basis: ChargingPriceBasis,
}

/// Complete editor form, independent of a UI or binding representation.
#[derive(Clone, Debug, PartialEq)]
pub struct ChargingEditorInput {
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub access_note: String,
    pub cost: ChargingCost,
    pub name: Option<String>,
    pub venue: Option<ChargingVenue>,
    pub hours: Option<String>,
    pub terms: Option<ChargingPaidTerms>,
    pub price: Option<ChargingEditorPrice>,
    pub supplies: Vec<ChargingEditorSupply>,
}

/// UI-independent editor field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargingEditorField {
    Coordinate,
    AccessNote,
    Name,
    Hours,
    Payment,
    Supply,
}

/// UI-independent explanation of the first rejected editor field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargingEditorReason {
    Invalid,
    Required,
    TooLong,
    ControlCharacter,
    TooMany,
    Duplicate,
    UnknownCombined,
    PaymentForFree,
    Incomplete,
}

/// First invalid editor field. Supply row indices are zero-based.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargingEditorIssue {
    pub field: ChargingEditorField,
    pub reason: ChargingEditorReason,
    pub supply_index: Option<u32>,
}

/// Validated spot or the first constructor-level editor issue.
pub type ChargingEditorValidation = Result<NewChargingSpot, ChargingEditorIssue>;

/// Runs the exact ordered constructor pipeline used by spot submission.
///
/// # Errors
/// Returns the first invalid editor field, preserving constructor validation order and supply row
/// index.
pub fn validate_charging_editor(input: ChargingEditorInput) -> ChargingEditorValidation {
    let coordinate = Coordinate::from_degrees(input.latitude_degrees, input.longitude_degrees)
        .map_err(|_| {
            issue(
                ChargingEditorField::Coordinate,
                ChargingEditorReason::Invalid,
            )
        })?;
    let minimum =
        NewChargingSpot::new(coordinate, &input.access_note, input.cost).map_err(|error| {
            let reason = match error {
                AccessNoteError::Blank => ChargingEditorReason::Required,
                AccessNoteError::TooLong => ChargingEditorReason::TooLong,
                AccessNoteError::ControlCharacter => ChargingEditorReason::ControlCharacter,
            };
            issue(ChargingEditorField::AccessNote, reason)
        })?;
    let price = input
        .price
        .map(|value| {
            ChargingPrice::new(
                value.amount_minor,
                &value.currency,
                value.minor_unit_exponent,
                value.basis,
            )
        })
        .transpose()
        .map_err(|_| issue(ChargingEditorField::Payment, ChargingEditorReason::Invalid))?;
    let supplies = input
        .supplies
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let supply_index = u32::try_from(index)
                .map_err(|_| issue(ChargingEditorField::Supply, ChargingEditorReason::TooMany))?;
            validate_editor_supply(value).map_err(|error| ChargingEditorIssue {
                field: ChargingEditorField::Supply,
                reason: supply_reason(error),
                supply_index: Some(supply_index),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let details = checked_details(
        input.name.as_deref(),
        input.venue,
        input.hours.as_deref(),
        input.terms,
        price,
        supplies,
    )?;
    minimum.with_details(details).map_err(|error| {
        issue(
            ChargingEditorField::Payment,
            if error == ChargingInputError::PaidDetailsForFree {
                ChargingEditorReason::PaymentForFree
            } else {
                ChargingEditorReason::Invalid
            },
        )
    })
}

/// Builds a validated supply row after the boundary has decoded known connector identifiers.
///
/// # Errors
/// Returns the charging domain validation error for an invalid voltage, connector set, or outlet
/// count.
pub fn charging_supply_checked(
    voltage_volts: Option<u16>,
    connectors: Vec<ChargingConnector>,
    outlet_count: Option<u16>,
) -> Result<ChargingSupply, ChargingInputError> {
    let voltage = voltage_volts.map(SupplyVoltage::new).transpose()?;
    ChargingSupply::new(voltage, connectors)?.with_outlet_count(outlet_count)
}

fn validate_editor_supply(
    value: ChargingEditorSupply,
) -> Result<ChargingSupply, ChargingInputError> {
    let connectors = value
        .connectors
        .into_iter()
        .map(|connector| match connector {
            ChargingEditorConnector::Known(value) => Ok(value),
            ChargingEditorConnector::Other(label) => {
                ConnectorLabel::new(&label).map(ChargingConnector::Other)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    charging_supply_checked(value.voltage_volts, connectors, value.outlet_count)
}

fn checked_details(
    name: Option<&str>,
    venue: Option<ChargingVenue>,
    hours: Option<&str>,
    terms: Option<ChargingPaidTerms>,
    price: Option<ChargingPrice>,
    supplies: Vec<ChargingSupply>,
) -> Result<ChargingDetails, ChargingEditorIssue> {
    ChargingDetails::new(name, venue, None, None, None, Vec::new())
        .map_err(|_| issue(ChargingEditorField::Name, ChargingEditorReason::Invalid))?;
    ChargingDetails::new(None, None, hours, None, None, Vec::new())
        .map_err(|_| issue(ChargingEditorField::Hours, ChargingEditorReason::Invalid))?;
    ChargingDetails::new(name, venue, hours, terms, price, supplies).map_err(|error| {
        if error == ChargingInputError::TooManySupplies {
            issue(ChargingEditorField::Supply, ChargingEditorReason::TooMany)
        } else {
            issue(ChargingEditorField::Payment, ChargingEditorReason::Invalid)
        }
    })
}

fn supply_reason(error: ChargingInputError) -> ChargingEditorReason {
    match error {
        ChargingInputError::TooManyConnectors => ChargingEditorReason::TooMany,
        ChargingInputError::DuplicateConnector => ChargingEditorReason::Duplicate,
        ChargingInputError::UnknownConnectorCombined => ChargingEditorReason::UnknownCombined,
        _ => ChargingEditorReason::Invalid,
    }
}

const fn issue(field: ChargingEditorField, reason: ChargingEditorReason) -> ChargingEditorIssue {
    ChargingEditorIssue {
        field,
        reason,
        supply_index: None,
    }
}

/// Validates a text field through the charging-details constructor's exact rules.
///
/// # Errors
/// Returns the domain validation error for invalid name or hours text.
pub fn charging_detail_text_is_valid(
    name: Option<&str>,
    hours: Option<&str>,
) -> Result<(), ChargingInputError> {
    ChargingDetails::new(name, None, None, None, None, Vec::new())?;
    ChargingDetails::new(None, None, hours, None, None, Vec::new())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_ride_maps::ChargingConnector;

    fn input() -> ChargingEditorInput {
        ChargingEditorInput {
            latitude_degrees: 40.0,
            longitude_degrees: -105.0,
            access_note: "Ask staff".into(),
            cost: ChargingCost::Free,
            name: None,
            venue: None,
            hours: None,
            terms: None,
            price: None,
            supplies: Vec::new(),
        }
    }

    #[test]
    fn editor_validation_preserves_first_error_order_and_supply_index() {
        let mut value = input();
        value.latitude_degrees = f64::NAN;
        value.access_note.clear();
        assert_eq!(
            validate_charging_editor(value).unwrap_err(),
            issue(
                ChargingEditorField::Coordinate,
                ChargingEditorReason::Invalid
            )
        );

        let mut value = input();
        value.supplies.push(ChargingEditorSupply {
            voltage_volts: None,
            connectors: vec![
                ChargingEditorConnector::Known(ChargingConnector::Unknown),
                ChargingEditorConnector::Known(ChargingConnector::Type2),
            ],
            outlet_count: None,
        });
        assert_eq!(
            validate_charging_editor(value).unwrap_err(),
            ChargingEditorIssue {
                field: ChargingEditorField::Supply,
                reason: ChargingEditorReason::UnknownCombined,
                supply_index: Some(0),
            }
        );
    }
}
