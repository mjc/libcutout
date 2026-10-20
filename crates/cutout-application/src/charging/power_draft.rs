use cutout_ride_maps::{
    ChargingCountry, ChargingPowerDraft as DomainDraft, ChargingSupply, ChargingSupplyKind,
    DraftChargingSupply, SupplyVoltage,
};

/// Read-only state needed to render a charging power row and prepare a save.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingPowerDraftSnapshot {
    /// Current visible voltage and connector selection.
    pub selection: DraftChargingSupply,
    /// Whether the choice is suggested or manual.
    pub manual: bool,
    /// Present only when the current selection is confirmed.
    pub included_supply: Option<ChargingSupply>,
}

/// Rust-owned provenance and inclusion transitions for one charging editor row.
#[derive(Clone, Debug)]
pub struct ChargingPowerDraft {
    draft: DomainDraft,
}

impl ChargingPowerDraft {
    /// Starts a country-derived row, unconfirmed until the rider includes it.
    #[must_use]
    pub fn new(country_code: Option<&str>, ev_station: bool) -> Self {
        Self {
            draft: DomainDraft::new(country(country_code), kind(ev_station)),
        }
    }

    /// Returns the visible choice and its explicit inclusion state.
    #[must_use]
    pub fn snapshot(&self) -> ChargingPowerDraftSnapshot {
        ChargingPowerDraftSnapshot {
            selection: self.draft.selection().clone(),
            manual: self.draft.is_manual(),
            included_supply: self.draft.confirmed_supply().cloned(),
        }
    }

    /// Applies a new country observation. Manual choices survive; replaced suggestions need confirmation.
    #[must_use]
    pub fn with_country(&self, country_code: Option<&str>) -> Self {
        Self {
            draft: self.draft.clone().with_country(country(country_code)),
        }
    }

    /// Applies a venue change. Manual rows survive, including EV context.
    #[must_use]
    pub fn with_kind(&self, ev_station: bool) -> Self {
        Self {
            draft: self.draft.clone().with_venue_kind(kind(ev_station)),
        }
    }

    /// Marks the row manual and unconfirmed, then includes a complete valid replacement.
    #[must_use]
    pub fn edit(&self, supply: Option<ChargingSupply>) -> Self {
        let draft = self.draft.clone().begin_edit();
        let draft = match supply {
            Some(supply) => draft.with_selection(supply).confirm(),
            None => draft,
        };
        Self { draft }
    }

    /// Explicitly includes the current displayed choice.
    #[must_use]
    pub fn confirm(&self) -> Self {
        Self {
            draft: self.draft.clone().confirm(),
        }
    }
}

/// Validates a complete voltage using the charging domain's range rules.
#[must_use]
pub fn charging_power_voltage_is_valid(voltage_volts: u16) -> bool {
    SupplyVoltage::new(voltage_volts).is_ok()
}

fn country(value: Option<&str>) -> Option<ChargingCountry> {
    value.and_then(|code| ChargingCountry::new(code).ok())
}

const fn kind(ev_station: bool) -> ChargingSupplyKind {
    if ev_station {
        ChargingSupplyKind::EvStation
    } else {
        ChargingSupplyKind::Outlet
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_ride_maps::{ChargingConnector, ConnectorLabel, OutletCount};

    fn custom_supply() -> ChargingSupply {
        ChargingSupply::new(
            Some(SupplyVoltage::new(240).expect("voltage is valid")),
            vec![
                ChargingConnector::Other(ConnectorLabel::new("über plug").expect("label is valid")),
                ChargingConnector::Type2,
            ],
        )
        .expect("supply is valid")
        .with_outlet_count(Some(4))
        .expect("count is compatible")
    }

    #[test]
    fn country_replacement_requires_reconfirmation_but_manual_edits_are_confirmed() {
        let suggestion = ChargingPowerDraft::new(Some("US"), false).confirm();
        let confirmed = suggestion.snapshot();
        assert!(!confirmed.manual);
        assert!(confirmed.included_supply.is_some());

        let replacement = suggestion.with_country(Some("GB")).snapshot();
        assert!(!replacement.manual);
        assert!(replacement.included_supply.is_none());

        let edited = ChargingPowerDraft::new(None, true)
            .edit(Some(custom_supply()))
            .snapshot();
        assert!(edited.manual);
        assert_eq!(edited.included_supply, Some(custom_supply()));
    }

    #[test]
    fn manual_connector_order_and_ev_context_survive_country_changes() {
        let draft = ChargingPowerDraft::new(None, false).edit(Some(custom_supply()));
        let before = draft.snapshot();
        let changed = draft.with_kind(true).with_country(Some("DE")).snapshot();
        assert!(changed.manual);
        assert_eq!(changed.selection, before.selection);
        assert_eq!(changed.included_supply, before.included_supply);
    }

    #[test]
    fn count_only_edits_confirm_and_invalid_or_incomplete_edits_revoke_inclusion() {
        let draft = ChargingPowerDraft::new(Some("US"), false);
        let selection = ChargingSupply::new(None, Vec::new())
            .expect("empty supply is a valid partial row")
            .with_outlet_count(Some(5))
            .expect("count is compatible");
        let edited = draft.edit(Some(selection));
        let snapshot = edited.snapshot();
        assert!(snapshot.manual);
        assert_eq!(
            snapshot.selection.outlet_count().map(OutletCount::get),
            Some(5)
        );
        assert_eq!(
            snapshot
                .included_supply
                .as_ref()
                .and_then(ChargingSupply::outlet_count)
                .map(OutletCount::get),
            Some(5)
        );

        let revoked = edited.edit(None).snapshot();
        assert!(revoked.manual);
        assert!(revoked.included_supply.is_none());
    }

    #[test]
    fn unsupported_country_uses_unknown_and_voltage_range_is_checked_by_domain() {
        let snapshot = ChargingPowerDraft::new(Some("é"), false).snapshot();
        assert_eq!(
            snapshot.selection.connectors(),
            &[ChargingConnector::Unknown]
        );
        assert!(!charging_power_voltage_is_valid(0));
        assert!(charging_power_voltage_is_valid(2_000));
        assert!(!charging_power_voltage_is_valid(2_001));
    }
}
