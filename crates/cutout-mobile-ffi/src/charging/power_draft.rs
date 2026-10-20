//! `UniFFI` adapters for the charging power-draft application service.

use std::sync::Arc;

use cutout_application::charging::power_draft::{
    ChargingPowerDraft as ApplicationChargingPowerDraft,
    ChargingPowerDraftSnapshot as ApplicationSnapshot,
};

use super::MobileChargingSupplyDto;
use super::conversion::supply_dto;

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
    draft: ApplicationChargingPowerDraft,
}

#[uniffi::export]
impl MobileChargingPowerDraft {
    /// Returns the visible choice and its explicit inclusion state.
    #[must_use]
    pub fn snapshot(&self) -> MobileChargingPowerDraftSnapshot {
        snapshot_dto(&self.draft.snapshot())
    }

    /// Applies a new country observation. Manual choices survive; replaced suggestions need confirmation.
    #[must_use]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI owns string arguments"
    )]
    pub fn with_country(&self, country_code: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            draft: self.draft.with_country(country_code.as_deref()),
        })
    }

    /// Applies a venue change. Manual rows survive, including EV context.
    #[must_use]
    pub fn with_kind(&self, ev_station: bool) -> Arc<Self> {
        Arc::new(Self {
            draft: self.draft.with_kind(ev_station),
        })
    }

    /// Marks the row manual and unconfirmed, then includes a complete valid replacement.
    #[must_use]
    pub fn edit(&self, supply: Option<MobileChargingSupplyDto>) -> Arc<Self> {
        let supply = supply.and_then(|supply| super::conversion::supply(supply).ok());
        Arc::new(Self {
            draft: self.draft.edit(supply),
        })
    }

    /// Explicitly includes the current displayed choice.
    #[must_use]
    pub fn confirm(&self) -> Arc<Self> {
        Arc::new(Self {
            draft: self.draft.confirm(),
        })
    }
}

/// Starts a country-derived row, unconfirmed until the rider includes it.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns string arguments"
)]
pub fn mobile_charging_power_draft(
    country_code: Option<String>,
    ev_station: bool,
) -> Arc<MobileChargingPowerDraft> {
    Arc::new(MobileChargingPowerDraft {
        draft: ApplicationChargingPowerDraft::new(country_code.as_deref(), ev_station),
    })
}

/// Validates a complete voltage using the charging domain's range rules.
#[must_use]
#[uniffi::export]
pub fn mobile_charging_power_voltage_is_valid(voltage_volts: u16) -> bool {
    cutout_application::charging::power_draft::charging_power_voltage_is_valid(voltage_volts)
}

fn snapshot_dto(value: &ApplicationSnapshot) -> MobileChargingPowerDraftSnapshot {
    MobileChargingPowerDraftSnapshot {
        selection: selection_dto(&value.selection),
        origin: if value.manual {
            MobileChargingPowerDraftOriginDto::Manual
        } else {
            MobileChargingPowerDraftOriginDto::Suggested
        },
        included_supply: value.included_supply.as_ref().map(supply_dto),
    }
}

fn selection_dto(value: &cutout_ride_maps::DraftChargingSupply) -> MobileChargingSupplyDto {
    MobileChargingSupplyDto {
        voltage_volts: value.voltage().map(cutout_ride_maps::SupplyVoltage::as_u16),
        connectors: value.connectors().iter().cloned().map(Into::into).collect(),
        outlet_count: value.outlet_count().map(cutout_ride_maps::OutletCount::get),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charging::MobileChargingConnectorDto;

    #[test]
    fn adapter_maps_domain_custom_connector_and_confirmation_state_to_exported_snapshot() {
        let draft = mobile_charging_power_draft(None, false).edit(Some(MobileChargingSupplyDto {
            voltage_volts: Some(240),
            connectors: vec![MobileChargingConnectorDto::Other {
                label: "über plug".into(),
            }],
            outlet_count: Some(4),
        }));
        let snapshot = draft.snapshot();
        assert_eq!(snapshot.origin, MobileChargingPowerDraftOriginDto::Manual);
        assert_eq!(
            snapshot.selection.connectors[0],
            MobileChargingConnectorDto::Other {
                label: "über plug".into(),
            }
        );
        assert_eq!(snapshot.included_supply, Some(snapshot.selection.clone()));
    }
}
