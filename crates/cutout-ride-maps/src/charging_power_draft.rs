use serde::{Deserialize, Serialize};

use crate::{ChargingConnector, ChargingInputError, ChargingSupply, SupplyVoltage};

/// Canonical two-letter country context, supplied by location or user correction.
/// A UI language or locale must not be substituted for the user's physical country.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct ChargingCountry(String);

impl ChargingCountry {
    /// Normalizes a two-letter ASCII code. Unsupported codes remain explicit Unknown suggestions.
    /// # Errors
    /// Rejects a value that does not have the two-letter country-code shape.
    pub fn new(value: &str) -> Result<Self, ChargingInputError> {
        if value.len() != 2 || !value.bytes().all(|v| v.is_ascii_alphabetic()) {
            return Err(ChargingInputError::Country);
        }
        Ok(Self(value.to_ascii_uppercase()))
    }
    /// Returns the normalized country code; this does not prove physical location.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ChargingCountry {
    type Error = ChargingInputError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}
impl From<ChargingCountry> for String {
    fn from(value: ChargingCountry) -> Self {
        value.0
    }
}

/// Whether a draft describes a household outlet or an EV connector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargingSupplyKind {
    /// Suggest a supported national household outlet.
    Outlet,
    /// Start Unknown rather than inferring an EV connector from the country.
    EvStation,
}

/// A displayed power selection that cannot be serialized or passed as included power.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftChargingSupply(ChargingSupply);
impl DraftChargingSupply {
    /// Returns the displayed known voltage, or Unknown.
    #[must_use]
    pub const fn voltage(&self) -> Option<SupplyVoltage> {
        self.0.voltage()
    }
    /// Returns the displayed connectors without marking them confirmed.
    #[must_use]
    pub fn connectors(&self) -> &[ChargingConnector] {
        self.0.connectors()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Selection {
    Suggested(DraftChargingSupply),
    Manual(DraftChargingSupply),
}
impl Selection {
    fn supply(&self) -> &DraftChargingSupply {
        match self {
            Self::Suggested(supply) | Self::Manual(supply) => supply,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum PowerState {
    Unconfirmed(Selection),
    Confirmed(Selection),
}
impl PowerState {
    fn selection(&self) -> &Selection {
        match self {
            Self::Unconfirmed(selection) | Self::Confirmed(selection) => selection,
        }
    }
}

/// An editable power draft whose suggestions cannot be submitted as observations.
/// Confirm explicitly, then obtain a supply through [`Self::confirmed_supply`].
/// The draft itself cannot be serialized as a community observation.
///
/// ```compile_fail
/// use cutout_ride_maps::{ChargingPowerDraft, ChargingSupplyKind};
/// fn submit<T: serde::Serialize>(_: &T) {}
/// let draft = ChargingPowerDraft::new(None, ChargingSupplyKind::Outlet);
/// submit(&draft);
/// ```
///
/// Even the displayed selection is not an included-power payload:
///
/// ```compile_fail
/// use cutout_ride_maps::{ChargingPowerDraft, ChargingSupplyKind};
/// fn submit<T: serde::Serialize>(_: &T) {}
/// let draft = ChargingPowerDraft::new(None, ChargingSupplyKind::Outlet);
/// submit(draft.selection());
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingPowerDraft {
    country: Option<ChargingCountry>,
    kind: ChargingSupplyKind,
    state: PowerState,
}
impl ChargingPowerDraft {
    /// Starts with editable country suggestions; an EV or unsupported country starts Unknown.
    #[must_use]
    pub fn new(country: Option<ChargingCountry>, kind: ChargingSupplyKind) -> Self {
        let selection =
            Selection::Suggested(DraftChargingSupply(suggestion(country.as_ref(), kind)));
        Self {
            country,
            kind,
            state: PowerState::Unconfirmed(selection),
        }
    }
    /// Returns the currently displayed supply, whether suggested, edited or confirmed.
    #[must_use]
    pub fn selection(&self) -> &DraftChargingSupply {
        self.state.selection().supply()
    }
    /// Returns a supply only after explicit inclusion.
    #[must_use]
    pub fn confirmed_supply(&self) -> Option<&ChargingSupply> {
        match &self.state {
            PowerState::Unconfirmed(_) => None,
            PowerState::Confirmed(selection) => Some(&selection.supply().0),
        }
    }
    /// Preserves deliberate manual choices when the current country changes.
    /// A replaced suggested value requires confirmation again.
    #[must_use]
    pub fn with_country(mut self, country: Option<ChargingCountry>) -> Self {
        self.country = country;
        if let Selection::Suggested(_) = self.state.selection() {
            let replacement = suggestion(self.country.as_ref(), self.kind);
            if replacement != self.selection().0 {
                self.state =
                    PowerState::Unconfirmed(Selection::Suggested(DraftChargingSupply(replacement)));
            }
        }
        self
    }
    /// Changes the supply kind. EV suggestions never inherit domestic plugs or voltage.
    #[must_use]
    pub fn with_kind(mut self, kind: ChargingSupplyKind) -> Self {
        if self.kind == kind {
            return self;
        }
        self.kind = kind;
        match kind {
            ChargingSupplyKind::EvStation => {
                self.state = PowerState::Unconfirmed(Selection::Suggested(DraftChargingSupply(
                    suggestion(self.country.as_ref(), kind),
                )));
            }
            ChargingSupplyKind::Outlet => {
                if let Selection::Suggested(_) = self.state.selection() {
                    self.state = PowerState::Unconfirmed(Selection::Suggested(
                        DraftChargingSupply(suggestion(self.country.as_ref(), kind)),
                    ));
                }
            }
        }
        self
    }
    /// Replaces the displayed power with a validated manual selection, still unconfirmed.
    #[must_use]
    pub fn with_selection(mut self, supply: ChargingSupply) -> Self {
        self.state = PowerState::Unconfirmed(Selection::Manual(DraftChargingSupply(supply)));
        self
    }
    /// Explicitly includes the current validated voltage/connector pairing.
    #[must_use]
    pub fn confirm(mut self) -> Self {
        self.state = match self.state {
            PowerState::Unconfirmed(selection) | PowerState::Confirmed(selection) => {
                PowerState::Confirmed(selection)
            }
        };
        self
    }
}

/// Version of the explicit initial national outlet table.
pub const CHARGING_OUTLET_CATALOGUE_VERSION: u16 = 1;

/// Returns supported national outlet choices, with the editable default first.
/// Initial coverage is US, GB, DE and AU; all other or absent contexts return Unknown.
/// EV connectors use the separate catalogue and never inherit these suggestions.
/// Country families follow the [Bel guide](https://www.belfuse.com/resource-library/tech-paper/world-plug-and-input-voltage-guide).
/// NEMA labels follow the [Leviton receptacle catalogue](https://leviton.com/products/5325-w).
/// The grounded option is the initial suggestion where both grounded and ungrounded options are listed.
#[must_use]
pub fn charging_outlet_choices(country: Option<&ChargingCountry>) -> &'static [ChargingConnector] {
    match country.map(ChargingCountry::as_str) {
        Some("US") => &[ChargingConnector::Nema5_15, ChargingConnector::TypeA],
        Some("GB") => &[ChargingConnector::TypeG],
        Some("DE") => &[ChargingConnector::TypeF, ChargingConnector::TypeC],
        Some("AU") => &[ChargingConnector::TypeI],
        _ => &[ChargingConnector::Unknown],
    }
}

fn suggestion(country: Option<&ChargingCountry>, kind: ChargingSupplyKind) -> ChargingSupply {
    let (voltage, connector) = match kind {
        ChargingSupplyKind::Outlet => {
            let voltage = match country.map(ChargingCountry::as_str) {
                Some("US") => Some(SupplyVoltage::US_OUTLET_SUGGESTION),
                _ => None,
            };
            (
                voltage,
                charging_outlet_choices(country)
                    .first()
                    .cloned()
                    .unwrap_or(ChargingConnector::Unknown),
            )
        }
        ChargingSupplyKind::EvStation => (None, ChargingConnector::Unknown),
    };
    ChargingSupply::single(voltage, connector)
}
