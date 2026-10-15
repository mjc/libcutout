//! Explicit editor ownership for charging power selection and inclusion.

use std::sync::Arc;

use super::conversion::supply_dto;
use super::{MobileChargingError, MobileChargingSupplyDto};
use crate::ride_maps;

/// Origin of the displayed power choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingPowerDraftOriginDto {
    /// Suggested from the current country and venue.
    Suggested,
    /// Chosen or edited by the rider, or restored from an observation.
    Manual,
}

/// Whether the displayed power choice may be included in an observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingPowerDraftConfirmationDto {
    /// The choice still needs an explicit include action.
    Unconfirmed,
    /// The choice may be included.
    Confirmed,
}

/// Read-only state needed to render a charging power row and prepare a save.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingPowerDraftSnapshot {
    /// Current visible voltage and connector selection.
    pub selection: MobileChargingSupplyDto,
    /// Whether the choice is suggested or manual.
    pub origin: MobileChargingPowerDraftOriginDto,
    /// Whether explicit inclusion is required.
    pub confirmation: MobileChargingPowerDraftConfirmationDto,
    /// Present only when the current selection is confirmed.
    pub included_supply: Option<MobileChargingSupplyDto>,
}

/// Rust-owned provenance and inclusion transitions for one editor row.
#[derive(Debug, uniffi::Object)]
pub struct MobileChargingPowerDraft {
    draft: ride_maps::ChargingPowerDraft,
}

#[uniffi::export]
impl MobileChargingPowerDraft {
    /// Returns the visible choice and its explicit inclusion state.
    #[must_use]
    pub fn snapshot(&self) -> MobileChargingPowerDraftSnapshot {
        MobileChargingPowerDraftSnapshot {
            selection: selection_dto(self.draft.selection()),
            origin: if self.draft.is_manual() {
                MobileChargingPowerDraftOriginDto::Manual
            } else {
                MobileChargingPowerDraftOriginDto::Suggested
            },
            confirmation: if self.draft.is_confirmed() {
                MobileChargingPowerDraftConfirmationDto::Confirmed
            } else {
                MobileChargingPowerDraftConfirmationDto::Unconfirmed
            },
            included_supply: self.draft.confirmed_supply().map(supply_dto),
        }
    }

    /// Moves the current row to manual/unconfirmed as soon as editing begins.
    #[must_use]
    pub fn begin_edit(&self) -> Arc<Self> {
        Arc::new(Self {
            draft: self.draft.clone().begin_edit(),
        })
    }

    /// Applies a new country observation. Manual choices survive; replaced suggestions need confirmation.
    #[must_use]
    pub fn with_country(&self, country_code: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            draft: self.draft.clone().with_country(country(country_code)),
        })
    }

    /// Applies a venue change. Manual rows survive, including EV context.
    #[must_use]
    pub fn with_kind(&self, ev_station: bool) -> Arc<Self> {
        let draft = self.draft.clone().with_venue_kind(if ev_station {
            ride_maps::ChargingSupplyKind::EvStation
        } else {
            ride_maps::ChargingSupplyKind::Outlet
        });
        Arc::new(Self { draft })
    }

    /// Applies a complete valid manual edit. Editing is an explicit inclusion action.
    ///
    /// # Errors
    /// Returns [`MobileChargingError::InvalidMetadata`] for an invalid supply.
    pub fn edit(&self, supply: MobileChargingSupplyDto) -> Result<Arc<Self>, MobileChargingError> {
        let supply = super::conversion::supply(supply)?;
        Ok(Arc::new(Self {
            draft: self.draft.clone().with_selection(supply).confirm(),
        }))
    }

    /// Explicitly includes the current displayed choice.
    #[must_use]
    pub fn confirm(&self) -> Arc<Self> {
        Arc::new(Self {
            draft: self.draft.clone().confirm(),
        })
    }
}

/// Starts a country-derived row, unconfirmed until the rider includes it.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_power_draft(
    country_code: Option<String>,
    ev_station: bool,
) -> Arc<MobileChargingPowerDraft> {
    Arc::new(MobileChargingPowerDraft {
        draft: ride_maps::ChargingPowerDraft::new(country(country_code), kind(ev_station)),
    })
}

/// Restores an existing observation as a confirmed manual row without changing connector order.
///
/// # Errors
/// Returns [`MobileChargingError::InvalidMetadata`] for an invalid supply.
#[uniffi::export]
pub fn mobile_charging_power_draft_restored(
    supply: MobileChargingSupplyDto,
    country_code: Option<String>,
    ev_station: bool,
) -> Result<Arc<MobileChargingPowerDraft>, MobileChargingError> {
    let draft = ride_maps::ChargingPowerDraft::new(country(country_code), kind(ev_station))
        .with_selection(super::conversion::supply(supply)?)
        .confirm();
    Ok(Arc::new(MobileChargingPowerDraft { draft }))
}

/// Validates a complete voltage using the charging domain's range rules.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_power_voltage_is_valid(voltage_volts: u16) -> bool {
    ride_maps::SupplyVoltage::new(voltage_volts).is_ok()
}

fn country(value: Option<String>) -> Option<ride_maps::ChargingCountry> {
    value.and_then(|code| ride_maps::ChargingCountry::new(&code).ok())
}

const fn kind(ev_station: bool) -> ride_maps::ChargingSupplyKind {
    if ev_station {
        ride_maps::ChargingSupplyKind::EvStation
    } else {
        ride_maps::ChargingSupplyKind::Outlet
    }
}

fn selection_dto(value: &ride_maps::DraftChargingSupply) -> MobileChargingSupplyDto {
    MobileChargingSupplyDto {
        voltage_volts: value.voltage().map(ride_maps::SupplyVoltage::as_u16),
        connectors: value.connectors().iter().cloned().map(Into::into).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charging::MobileChargingConnectorDto;

    #[test]
    fn country_replacement_requires_reconfirmation_but_manual_edits_are_confirmed() {
        let draft = mobile_charging_power_draft(Some("US".into()), false).confirm();
        assert_eq!(
            draft.snapshot().confirmation,
            MobileChargingPowerDraftConfirmationDto::Confirmed
        );

        let replacement = draft.with_country(Some("GB".into())).snapshot();
        assert_eq!(
            replacement.origin,
            MobileChargingPowerDraftOriginDto::Suggested
        );
        assert_eq!(
            replacement.confirmation,
            MobileChargingPowerDraftConfirmationDto::Unconfirmed
        );
        assert!(replacement.included_supply.is_none());

        let edited = mobile_charging_power_draft(None, true)
            .edit(MobileChargingSupplyDto {
                voltage_volts: None,
                connectors: vec![MobileChargingConnectorDto::Other {
                    label: "über plug".into(),
                }],
            })
            .unwrap()
            .snapshot();
        assert_eq!(edited.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert_eq!(
            edited.confirmation,
            MobileChargingPowerDraftConfirmationDto::Confirmed
        );
        assert_eq!(edited.included_supply, Some(edited.selection.clone()));
    }

    #[test]
    fn restored_unknown_and_custom_connectors_keep_order_and_ev_venue_context() {
        let supply = MobileChargingSupplyDto {
            voltage_volts: Some(240),
            connectors: vec![
                MobileChargingConnectorDto::Other {
                    label: "über plug".into(),
                },
                MobileChargingConnectorDto::Type2,
            ],
        };
        let restored = mobile_charging_power_draft_restored(supply.clone(), None, false).unwrap();
        let changed = restored
            .with_kind(true)
            .with_country(Some("DE".into()))
            .snapshot();
        assert_eq!(changed.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert_eq!(
            changed.confirmation,
            MobileChargingPowerDraftConfirmationDto::Confirmed
        );
        assert_eq!(changed.selection, supply);
        assert_eq!(changed.included_supply, Some(supply));

        let unknown = mobile_charging_power_draft_restored(
            MobileChargingSupplyDto {
                voltage_volts: None,
                connectors: vec![MobileChargingConnectorDto::Unknown],
            },
            Some("é".into()),
            false,
        )
        .unwrap()
        .snapshot();
        assert_eq!(
            unknown.selection.connectors,
            vec![MobileChargingConnectorDto::Unknown]
        );

        let unsupported_country = mobile_charging_power_draft(Some("é".into()), false).snapshot();
        assert_eq!(
            unsupported_country.selection.connectors,
            vec![MobileChargingConnectorDto::Unknown]
        );
    }

    #[test]
    fn manual_edit_uses_domain_validation() {
        let error = mobile_charging_power_draft(None, false).edit(MobileChargingSupplyDto {
            voltage_volts: Some(2_001),
            connectors: vec![MobileChargingConnectorDto::Unknown],
        });
        assert!(error.is_err());
    }

    #[test]
    fn incomplete_edit_has_manual_provenance_and_cannot_be_included() {
        let draft = mobile_charging_power_draft(Some("US".into()), false)
            .confirm()
            .begin_edit()
            .snapshot();
        assert_eq!(draft.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert_eq!(
            draft.confirmation,
            MobileChargingPowerDraftConfirmationDto::Unconfirmed
        );
        assert!(draft.included_supply.is_none());
        assert_eq!(draft.selection.voltage_volts, Some(120));
    }
}
