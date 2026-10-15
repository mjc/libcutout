use serde::{Deserialize, Serialize};
use std::num::NonZeroU16;
use thiserror::Error;

use crate::ChargingCost;

/// Invalid optional charging information.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ChargingInputError {
    /// Current-country context must use a two-letter ASCII code.
    #[error("invalid current-country code")]
    Country,
    /// Supply voltage must be between 1 and 2,000 whole volts.
    #[error("supply voltage must be between 1 and 2000 volts")]
    VoltageOutOfRange,
    /// A supplied text value is blank, too long, or contains unsupported controls.
    #[error("invalid charging text")]
    Text,
    /// A supply group has more than sixteen connectors.
    #[error("too many connectors")]
    TooManyConnectors,
    /// A connector appears more than once in a supply group.
    #[error("duplicate connector")]
    DuplicateConnector,
    /// Unknown must be the only connector in its group.
    #[error("unknown connector cannot be combined with another connector")]
    UnknownConnectorCombined,
    /// An explicitly supplied outlet count must be greater than zero.
    #[error("outlet count must be greater than zero")]
    OutletCountZero,
    /// A spot has more than eight supply groups.
    #[error("too many supply groups")]
    TooManySupplies,
    /// The amount, currency shape, or minor-unit exponent is invalid.
    #[error("invalid charge price")]
    Price,
    /// A free spot contains a price or payment condition.
    #[error("free spot cannot contain payment details")]
    PaidDetailsForFree,
}

/// A known supply voltage, in whole volts; absence represents Unknown.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct SupplyVoltage(u16);

impl SupplyVoltage {
    /// Editable 120 V suggestion for a U.S. outlet; not a verified observation.
    pub const US_OUTLET_SUGGESTION: Self = Self(120);
    /// Validates a known supply voltage.
    ///
    /// # Errors
    /// Returns [`ChargingInputError::VoltageOutOfRange`] outside 1–2,000 volts.
    pub fn new(value: u16) -> Result<Self, ChargingInputError> {
        if !(1..=2_000).contains(&value) {
            return Err(ChargingInputError::VoltageOutOfRange);
        }
        Ok(Self(value))
    }

    /// Returns the known voltage in whole volts.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for SupplyVoltage {
    type Error = ChargingInputError;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<SupplyVoltage> for u16 {
    fn from(value: SupplyVoltage) -> Self {
        value.0
    }
}

/// A known number of outlets in one supply group.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct OutletCount(NonZeroU16);

impl OutletCount {
    /// Validates a nonzero outlet count.
    ///
    /// # Errors
    /// Returns [`ChargingInputError::OutletCountZero`] for zero.
    pub fn new(value: u16) -> Result<Self, ChargingInputError> {
        NonZeroU16::new(value)
            .map(Self)
            .ok_or(ChargingInputError::OutletCountZero)
    }

    /// Returns the number of outlets.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl TryFrom<u16> for OutletCount {
    type Error = ChargingInputError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<OutletCount> for u16 {
    fn from(value: OutletCount) -> Self {
        value.get()
    }
}

/// A nonblank label for a connector outside the stable catalogue.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConnectorLabel(String);
impl ConnectorLabel {
    /// Trims and validates a label, bounded to eighty UTF-8 bytes.
    /// # Errors
    /// Returns [`ChargingInputError::Text`] for blank, oversized or control-containing
    /// text, or a label matching a known connector ID.
    pub fn new(value: &str) -> Result<Self, ChargingInputError> {
        let value = text(value, 80)?;
        if ChargingConnector::is_known_id(&value) {
            return Err(ChargingInputError::Text);
        }
        Ok(Self(value))
    }
    /// Returns the normalized label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ConnectorLabel {
    type Error = ChargingInputError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}
impl From<ConnectorLabel> for String {
    fn from(value: ConnectorLabel) -> Self {
        value.0
    }
}

/// Optional venue category, independent of access cost and power.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingVenue {
    /// Restaurant venue.
    Restaurant,
    /// Cafe venue.
    Cafe,
    /// Bar venue.
    Bar,
    /// An electric-vehicle charging station.
    EvChargingStation,
    /// A different category or labelled connector.
    Other,
}

/// Stable connector IDs; these do not assert adapter or electrical compatibility.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingConnector {
    /// Household type A connector.
    TypeA,
    /// Household type C connector.
    TypeC,
    /// Household type D connector.
    TypeD,
    /// Household type E connector.
    TypeE,
    /// Household type F connector.
    TypeF,
    /// Household type G connector.
    TypeG,
    /// Household type H connector.
    TypeH,
    /// Household type I connector.
    TypeI,
    /// Household type J connector.
    TypeJ,
    /// Household type K connector.
    TypeK,
    /// Household type L connector.
    TypeL,
    /// Household type M connector.
    TypeM,
    /// Household type N connector.
    TypeN,
    /// Household type O connector.
    TypeO,
    #[serde(rename = "nema_5_15")]
    /// NEMA 5-15 connector.
    Nema5_15,
    #[serde(rename = "nema_5_20")]
    /// NEMA 5-20 connector.
    Nema5_20,
    #[serde(rename = "nema_14_50")]
    /// NEMA 14-50 connector.
    Nema14_50,
    #[serde(rename = "nema_14_30")]
    /// NEMA 14-30 connector.
    Nema14_30,
    #[serde(rename = "nema_10_30")]
    /// NEMA 10-30 connector.
    Nema10_30,
    #[serde(rename = "nema_6_50")]
    /// NEMA 6-50 connector.
    Nema6_50,
    #[serde(rename = "nema_10_50")]
    /// NEMA 10-50 connector.
    Nema10_50,
    /// SAE J1772 connector.
    J1772,
    #[serde(rename = "type_2")]
    /// IEC 62196 Type 2 electric-vehicle connector.
    Type2,
    /// CCS Combo 1 connector.
    Ccs1,
    /// CCS Combo 2 connector.
    Ccs2,
    /// `CHAdeMO` connector.
    Chademo,
    /// NACS / SAE J3400 connector.
    Nacs,
    /// Connector information is explicitly unknown.
    Unknown,
    /// A different category or labelled connector.
    Other(ConnectorLabel),
}

impl ChargingConnector {
    fn is_known_id(label: &str) -> bool {
        let normalized = label.to_ascii_lowercase();
        let value = serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&normalized);
        Self::deserialize(value).is_ok()
    }
}

/// Optional conditions for access to a charging spot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingPaidTerms {
    /// A charging fee is required.
    DirectFee,
    /// A purchase is required.
    PurchaseRequired,
    /// A purchase and a charging fee are required.
    Both,
}

/// Unit or purchase basis for an exact quoted amount.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingPriceBasis {
    /// Amount per charging session.
    PerCharge,
    /// Amount per hour.
    PerHour,
    /// Amount per kilowatt-hour.
    PerKwh,
    /// Required purchase amount.
    Purchase,
}

/// One validated voltage/connector pairing. No country default is inserted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "UncheckedSupply")]
pub struct ChargingSupply {
    voltage_volts: Option<SupplyVoltage>,
    connectors: Vec<ChargingConnector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outlet_count: Option<OutletCount>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedSupply {
    voltage_volts: Option<SupplyVoltage>,
    #[serde(default)]
    connectors: Vec<ChargingConnector>,
    #[serde(default)]
    outlet_count: Option<u16>,
}
impl ChargingSupply {
    /// Creates a single validated connector/voltage pairing, including Unknown.
    #[must_use]
    pub fn single(voltage: Option<SupplyVoltage>, connector: ChargingConnector) -> Self {
        Self {
            voltage_volts: voltage,
            connectors: vec![connector],
            outlet_count: None,
        }
    }
    /// Validates a single supply group. An empty selection means Unknown.
    /// # Errors
    /// Rejects more than sixteen, duplicate, or mixed Unknown connectors.
    pub fn new(
        voltage: Option<SupplyVoltage>,
        connectors: Vec<ChargingConnector>,
    ) -> Result<Self, ChargingInputError> {
        if connectors.len() > 16 {
            return Err(ChargingInputError::TooManyConnectors);
        }
        if connectors.contains(&ChargingConnector::Unknown) && connectors.len() != 1 {
            return Err(ChargingInputError::UnknownConnectorCombined);
        }
        for (index, connector) in connectors.iter().enumerate() {
            if connectors[..index].contains(connector) {
                return Err(ChargingInputError::DuplicateConnector);
            }
        }
        Ok(Self {
            voltage_volts: voltage,
            connectors,
            outlet_count: None,
        })
    }
    /// Sets the explicitly reported number of outlets in this supply group.
    ///
    /// # Errors
    /// Rejects an explicitly supplied count of zero.
    pub fn with_outlet_count(
        mut self,
        outlet_count: Option<u16>,
    ) -> Result<Self, ChargingInputError> {
        self.outlet_count = outlet_count.map(OutletCount::new).transpose()?;
        Ok(self)
    }
    /// Returns a known supply voltage, or Unknown.
    #[must_use]
    pub const fn voltage(&self) -> Option<SupplyVoltage> {
        self.voltage_volts
    }
    /// Returns the connectors paired with this voltage.
    #[must_use]
    pub fn connectors(&self) -> &[ChargingConnector] {
        &self.connectors
    }
    /// Returns the explicitly reported number of outlets, when known.
    #[must_use]
    pub const fn outlet_count(&self) -> Option<OutletCount> {
        self.outlet_count
    }
}
impl TryFrom<UncheckedSupply> for ChargingSupply {
    type Error = ChargingInputError;
    fn try_from(value: UncheckedSupply) -> Result<Self, Self::Error> {
        Self::new(value.voltage_volts, value.connectors)?.with_outlet_count(value.outlet_count)
    }
}

/// An exact quoted amount; the service does not process payments or convert currency.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "UncheckedPrice")]
pub struct ChargingPrice {
    amount_minor: u64,
    currency: String,
    minor_unit_exponent: u8,
    basis: ChargingPriceBasis,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedPrice {
    amount_minor: u64,
    currency: String,
    minor_unit_exponent: u8,
    basis: ChargingPriceBasis,
}
impl ChargingPrice {
    /// Validates an integer minor-unit amount, currency shape, scale and basis.
    /// # Errors
    /// Requires 1–9,000,000,000,000,000 minor units, three uppercase ASCII currency
    /// letters and an exponent from zero through four. It does not verify currency allocation.
    pub fn new(
        amount_minor: u64,
        currency: &str,
        minor_unit_exponent: u8,
        basis: ChargingPriceBasis,
    ) -> Result<Self, ChargingInputError> {
        if amount_minor == 0
            || amount_minor > 9_000_000_000_000_000
            || minor_unit_exponent > 4
            || currency.len() != 3
            || !currency.bytes().all(|v| v.is_ascii_uppercase())
        {
            return Err(ChargingInputError::Price);
        }
        Ok(Self {
            amount_minor,
            currency: currency.to_owned(),
            minor_unit_exponent,
            basis,
        })
    }
    /// Returns the quoted integer number of minor units.
    #[must_use]
    pub const fn amount_minor(&self) -> u64 {
        self.amount_minor
    }
    /// Returns the three-letter currency code.
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    /// Returns the decimal minor-unit exponent.
    #[must_use]
    pub const fn minor_unit_exponent(&self) -> u8 {
        self.minor_unit_exponent
    }
    /// Returns the charging or purchase basis.
    #[must_use]
    pub const fn basis(&self) -> ChargingPriceBasis {
        self.basis
    }
}
impl TryFrom<UncheckedPrice> for ChargingPrice {
    type Error = ChargingInputError;
    fn try_from(value: UncheckedPrice) -> Result<Self, Self::Error> {
        Self::new(
            value.amount_minor,
            &value.currency,
            value.minor_unit_exponent,
            value.basis,
        )
    }
}

/// Validated optional public metadata for a charging spot.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ChargingDetails {
    name: Option<String>,
    venue: Option<ChargingVenue>,
    hours: Option<String>,
    paid_terms: Option<ChargingPaidTerms>,
    price: Option<ChargingPrice>,
    supplies: Vec<ChargingSupply>,
}
impl ChargingDetails {
    /// Normalizes optional name/hours and validates the supply count.
    /// # Errors
    /// Rejects invalid text or more than eight supply groups. Use
    /// [`crate::NewChargingSpot::with_details`] to check the access-cost relationship.
    pub fn new(
        name: Option<&str>,
        venue: Option<ChargingVenue>,
        hours: Option<&str>,
        paid_terms: Option<ChargingPaidTerms>,
        price: Option<ChargingPrice>,
        supplies: Vec<ChargingSupply>,
    ) -> Result<Self, ChargingInputError> {
        if supplies.len() > 8 {
            return Err(ChargingInputError::TooManySupplies);
        }
        Ok(Self {
            name: optional_text(name, 120)?,
            venue,
            hours: optional_text(hours, 500)?,
            paid_terms,
            price,
            supplies,
        })
    }
    pub(crate) fn validate_cost(&self, cost: ChargingCost) -> Result<(), ChargingInputError> {
        if cost == ChargingCost::Free && (self.paid_terms.is_some() || self.price.is_some()) {
            return Err(ChargingInputError::PaidDetailsForFree);
        }
        Ok(())
    }
    /// Returns the optional venue name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    /// Returns the optional venue category.
    #[must_use]
    pub const fn venue(&self) -> Option<ChargingVenue> {
        self.venue
    }
    /// Returns the optional access hours.
    #[must_use]
    pub fn hours(&self) -> Option<&str> {
        self.hours.as_deref()
    }
    /// Returns the optional access conditions.
    #[must_use]
    pub const fn paid_terms(&self) -> Option<ChargingPaidTerms> {
        self.paid_terms
    }
    /// Returns the optional exact quote.
    #[must_use]
    pub const fn price(&self) -> Option<&ChargingPrice> {
        self.price.as_ref()
    }
    /// Returns the paired, explicitly supplied power groups.
    #[must_use]
    pub fn supplies(&self) -> &[ChargingSupply] {
        &self.supplies
    }
}

fn text(value: &str, limit: usize) -> Result<String, ChargingInputError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > limit
        || value
            .chars()
            .any(|v| v.is_control() && !['\n', '\r', '\t'].contains(&v))
    {
        return Err(ChargingInputError::Text);
    }
    Ok(value.to_owned())
}

fn optional_text(value: Option<&str>, limit: usize) -> Result<Option<String>, ChargingInputError> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(|v| text(v, limit))
        .transpose()
}
