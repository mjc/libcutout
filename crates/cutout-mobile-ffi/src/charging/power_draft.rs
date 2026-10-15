//! Explicit editor ownership for charging power selection and inclusion.

use std::sync::Arc;

use super::MobileChargingSupplyDto;
use super::conversion::supply_dto;
use crate::ride_maps;

/// Origin of the displayed power choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingPowerDraftOriginDto {
    /// Suggested from the current country and venue.
    Suggested,
    /// Chosen or edited by the rider, or restored from an observation.
    Manual,
}

/// Read-only state needed to render a charging power row and prepare a save.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingPowerDraftSnapshot {
    /// Current visible voltage and connector selection.
    pub selection: MobileChargingSupplyDto,
    /// Whether the choice is suggested or manual.
    pub origin: MobileChargingPowerDraftOriginDto,
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
            included_supply: self.draft.confirmed_supply().map(supply_dto),
        }
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

    /// Marks the row manual and unconfirmed, then includes a complete valid replacement.
    #[must_use]
    pub fn edit(&self, supply: Option<MobileChargingSupplyDto>) -> Arc<Self> {
        let draft = self.draft.clone().begin_edit();
        let draft = match supply.and_then(|supply| super::conversion::supply(supply).ok()) {
            Some(supply) => draft.with_selection(supply).confirm(),
            None => draft,
        };
        Arc::new(Self { draft })
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
        outlet_count: value.outlet_count().map(ride_maps::OutletCount::get),
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
            draft.snapshot().included_supply,
            Some(draft.snapshot().selection)
        );

        let replacement = draft.with_country(Some("GB".into())).snapshot();
        assert_eq!(
            replacement.origin,
            MobileChargingPowerDraftOriginDto::Suggested
        );
        assert!(replacement.included_supply.is_none());

        let edited = mobile_charging_power_draft(None, true)
            .edit(Some(MobileChargingSupplyDto {
                voltage_volts: None,
                connectors: vec![MobileChargingConnectorDto::Other {
                    label: "über plug".into(),
                }],
                outlet_count: None,
            }))
            .snapshot();
        assert_eq!(edited.origin, MobileChargingPowerDraftOriginDto::Manual);
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
            outlet_count: Some(4),
        };
        let restored = mobile_charging_power_draft(None, false).edit(Some(supply.clone()));
        let changed = restored
            .with_kind(true)
            .with_country(Some("DE".into()))
            .snapshot();
        assert_eq!(changed.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert_eq!(changed.selection, supply);
        assert_eq!(changed.included_supply, Some(supply));

        let unknown = mobile_charging_power_draft(Some("é".into()), false)
            .edit(Some(MobileChargingSupplyDto {
                voltage_volts: None,
                connectors: vec![MobileChargingConnectorDto::Unknown],
                outlet_count: None,
            }))
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
    fn manual_count_edit_confirms_only_the_fields_the_rider_supplied() {
        let suggested = mobile_charging_power_draft(Some("US".into()), false).snapshot();
        assert_eq!(suggested.selection.outlet_count, None);
        assert!(suggested.included_supply.is_none());

        let count_only = MobileChargingSupplyDto {
            voltage_volts: None,
            connectors: Vec::new(),
            outlet_count: Some(5),
        };
        let edited = mobile_charging_power_draft(Some("US".into()), false)
            .edit(Some(count_only.clone()))
            .snapshot();
        assert_eq!(edited.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert_eq!(edited.selection, count_only);
        assert_eq!(edited.included_supply, Some(count_only));
    }

    #[test]
    fn invalid_manual_edit_keeps_previous_diagnostic_selection_but_revokes_inclusion() {
        let before = MobileChargingSupplyDto {
            voltage_volts: Some(240),
            connectors: vec![MobileChargingConnectorDto::Type2],
            outlet_count: None,
        };
        let edited = mobile_charging_power_draft(None, false)
            .edit(Some(before.clone()))
            .edit(Some(MobileChargingSupplyDto {
                voltage_volts: Some(2_001),
                connectors: vec![MobileChargingConnectorDto::Unknown],
                outlet_count: None,
            }))
            .snapshot();
        assert_eq!(edited.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert_eq!(edited.selection, before);
        assert!(edited.included_supply.is_none());
    }

    #[test]
    fn incomplete_edit_has_manual_provenance_and_cannot_be_included() {
        let draft = mobile_charging_power_draft(Some("US".into()), false)
            .confirm()
            .edit(None)
            .snapshot();
        assert_eq!(draft.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert!(draft.included_supply.is_none());
        assert_eq!(draft.selection.voltage_volts, Some(120));
    }
}
