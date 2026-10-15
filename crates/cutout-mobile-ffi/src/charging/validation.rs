use super::{MobileChargingInputDto, conversion};

/// Editor field containing an invalid value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingValidationFieldDto {
    /// Map coordinate.
    Coordinate,
    /// Required access instructions.
    AccessNote,
    /// Optional public name.
    Name,
    /// Optional opening hours.
    Hours,
    /// Paid price and payment terms.
    Payment,
    /// A paired voltage and connector group.
    Supply,
}

/// Why an editor field is invalid according to the charging constructors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingValidationReasonDto {
    /// Value is malformed or outside its domain bounds.
    Invalid,
    /// Required value is missing.
    Required,
    /// Text exceeds its byte limit.
    TooLong,
    /// Text contains a forbidden control character.
    ControlCharacter,
    /// The row count exceeds the supported limit.
    TooMany,
    /// A connector appears more than once.
    Duplicate,
    /// Unknown connector cannot be combined with other connectors.
    UnknownCombined,
    /// Payment information is present for a free spot.
    PaymentForFree,
    /// The rider is still entering a value that cannot yet be encoded.
    Incomplete,
}

/// One typed editor field issue. Supply indices are zero-based.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingFieldIssueDto {
    /// Invalid field.
    pub field: MobileChargingValidationFieldDto,
    /// Constructor-level reason.
    pub reason: MobileChargingValidationReasonDto,
    /// Supply row, when `field` is `Supply`.
    pub supply_index: Option<u32>,
}

/// Rust-normalized input or the first typed issue, as a mutually exclusive result.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum MobileChargingValidationDto {
    /// All fields passed the save constructors.
    Valid { input: MobileChargingInputDto },
    /// The first field rejected by the save constructors.
    Invalid { issue: MobileChargingFieldIssueDto },
}

/// Validates editor input through the exact constructor pipeline used for saving.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_validate_editor(
    input: MobileChargingInputDto,
) -> MobileChargingValidationDto {
    match conversion::input_checked(input) {
        Ok(value) => MobileChargingValidationDto::Valid {
            input: conversion::input_dto(&value),
        },
        Err(issue) => MobileChargingValidationDto::Invalid { issue },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MobileChargingConnectorDto, MobileChargingCostDto, MobileChargingPriceBasisDto,
        MobileChargingPriceDto, MobileChargingSupplyDto, MobileMapCoordinateDto,
    };

    fn input() -> MobileChargingInputDto {
        MobileChargingInputDto {
            coordinate: MobileMapCoordinateDto {
                latitude_degrees: 40.0,
                longitude_degrees: -105.0,
            },
            access_note: "Ask staff".into(),
            cost: MobileChargingCostDto::Free,
            name: None,
            venue: None,
            hours: None,
            terms: None,
            price: None,
            supplies: Vec::new(),
        }
    }

    fn supply(connector: MobileChargingConnectorDto) -> MobileChargingSupplyDto {
        MobileChargingSupplyDto {
            voltage_volts: Some(120),
            connectors: vec![connector],
            outlet_count: None,
        }
    }

    fn first_issue(input: MobileChargingInputDto) -> MobileChargingFieldIssueDto {
        match mobile_charging_validate_editor(input) {
            MobileChargingValidationDto::Invalid { issue } => issue,
            MobileChargingValidationDto::Valid { .. } => panic!("expected invalid input"),
        }
    }

    #[test]
    fn typed_access_note_validation_uses_trimmed_utf8_bytes_and_controls() {
        let mut value = input();
        value.access_note = "é".repeat(1_001);
        assert_eq!(
            first_issue(value).reason,
            MobileChargingValidationReasonDto::TooLong
        );

        let mut value = input();
        value.access_note = "Ask\0staff".into();
        assert_eq!(
            first_issue(value).reason,
            MobileChargingValidationReasonDto::ControlCharacter
        );
    }

    #[test]
    fn typed_supply_validation_keeps_the_eight_row_and_connector_constructor_limits() {
        let mut value = input();
        value.supplies = (0..8)
            .map(|_| supply(MobileChargingConnectorDto::TypeB))
            .collect();
        assert!(matches!(
            mobile_charging_validate_editor(value.clone()),
            MobileChargingValidationDto::Valid { .. }
        ));

        value
            .supplies
            .push(supply(MobileChargingConnectorDto::TypeB));
        assert_eq!(
            first_issue(value).reason,
            MobileChargingValidationReasonDto::TooMany
        );

        let mut value = input();
        value
            .supplies
            .push(supply(MobileChargingConnectorDto::Other {
                label: "type_b".into(),
            }));
        assert_eq!(
            first_issue(value).field,
            MobileChargingValidationFieldDto::Supply
        );

        let mut value = input();
        value.supplies = vec![MobileChargingSupplyDto {
            voltage_volts: None,
            connectors: vec![
                MobileChargingConnectorDto::TypeA,
                MobileChargingConnectorDto::TypeB,
                MobileChargingConnectorDto::TypeC,
                MobileChargingConnectorDto::TypeD,
                MobileChargingConnectorDto::TypeE,
                MobileChargingConnectorDto::TypeF,
                MobileChargingConnectorDto::TypeG,
                MobileChargingConnectorDto::TypeH,
                MobileChargingConnectorDto::TypeI,
                MobileChargingConnectorDto::TypeJ,
                MobileChargingConnectorDto::TypeK,
                MobileChargingConnectorDto::TypeL,
                MobileChargingConnectorDto::TypeM,
                MobileChargingConnectorDto::TypeN,
                MobileChargingConnectorDto::TypeO,
                MobileChargingConnectorDto::Nema5_15,
                MobileChargingConnectorDto::Nema5_20,
            ],
            outlet_count: None,
        }];
        assert_eq!(
            first_issue(value).reason,
            MobileChargingValidationReasonDto::TooMany
        );
    }

    #[test]
    fn zero_outlet_count_is_rejected_at_the_mobile_boundary() {
        let mut value = input();
        value.supplies = vec![MobileChargingSupplyDto {
            voltage_volts: None,
            connectors: Vec::new(),
            outlet_count: Some(0),
        }];
        assert_eq!(
            first_issue(value).field,
            MobileChargingValidationFieldDto::Supply
        );
    }

    #[test]
    fn typed_payment_validation_matches_price_and_free_cost_rules() {
        let mut value = input();
        value.cost = MobileChargingCostDto::Paid;
        value.price = Some(MobileChargingPriceDto {
            amount_minor: 125,
            currency: "USD".into(),
            minor_unit_exponent: 2,
            basis: MobileChargingPriceBasisDto::PerHour,
        });
        assert!(matches!(
            mobile_charging_validate_editor(value.clone()),
            MobileChargingValidationDto::Valid { .. }
        ));

        value.cost = MobileChargingCostDto::Free;
        assert_eq!(
            first_issue(value).reason,
            MobileChargingValidationReasonDto::PaymentForFree
        );

        let mut value = input();
        value.cost = MobileChargingCostDto::Paid;
        value.price = Some(MobileChargingPriceDto {
            amount_minor: 125,
            currency: "usd".into(),
            minor_unit_exponent: 2,
            basis: MobileChargingPriceBasisDto::PerHour,
        });
        assert_eq!(
            first_issue(value).field,
            MobileChargingValidationFieldDto::Payment
        );
    }
}
