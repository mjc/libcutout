use cutout_ride_maps::{ChargingCost, ChargingVote, Coordinate, NewChargingSpot};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Error;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pin {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Venue {
    Restaurant,
    Cafe,
    Bar,
    EvChargingStation,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Connector {
    TypeA,
    TypeB,
    TypeC,
    TypeD,
    TypeE,
    TypeF,
    TypeG,
    TypeH,
    TypeI,
    TypeJ,
    TypeK,
    TypeL,
    TypeM,
    TypeN,
    TypeO,
    #[serde(rename = "nema_5_15")]
    Nema5_15,
    #[serde(rename = "nema_5_20")]
    Nema5_20,
    #[serde(rename = "nema_14_50")]
    Nema14_50,
    J1772,
    #[serde(rename = "type_2")]
    Type2,
    Ccs1,
    Ccs2,
    Chademo,
    Nacs,
    Unknown,
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Supply {
    pub voltage_volts: Option<u16>,
    #[serde(default)]
    pub connectors: Vec<Connector>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PaidTerms {
    DirectFee,
    PurchaseRequired,
    Both,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PriceBasis {
    PerCharge,
    PerHour,
    PerKwh,
    Purchase,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Price {
    pub amount_minor: u64,
    pub currency: String,
    pub minor_unit_exponent: u8,
    pub basis: PriceBasis,
}

/// Wire input is validated before it reaches persistence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpotInput {
    pub coordinate: Pin,
    pub access_note: String,
    pub cost: ChargingCost,
    pub name: Option<String>,
    pub venue: Option<Venue>,
    pub hours: Option<String>,
    pub paid_terms: Option<PaidTerms>,
    pub price: Option<Price>,
    #[serde(default)]
    pub supplies: Vec<Supply>,
}

impl SpotInput {
    pub(crate) fn validate(mut self) -> Result<Self, Error> {
        let coordinate =
            Coordinate::from_fixed_parts(self.coordinate.latitude_e7, self.coordinate.longitude_e7)
                .map_err(|_| Error::Invalid)?;
        let minimum = NewChargingSpot::new(coordinate, &self.access_note, self.cost)
            .map_err(|_| Error::Invalid)?;
        minimum
            .access_note()
            .as_str()
            .clone_into(&mut self.access_note);
        self.name = optional_text(self.name, 120)?;
        self.hours = optional_text(self.hours, 500)?;
        if self.cost == ChargingCost::Free && (self.paid_terms.is_some() || self.price.is_some()) {
            return Err(Error::Invalid);
        }
        if let Some(price) = &self.price
            && (price.amount_minor == 0
                || price.amount_minor > 9_000_000_000_000_000
                || price.minor_unit_exponent > 4
                || price.currency.len() != 3
                || !price
                    .currency
                    .bytes()
                    .all(|value| value.is_ascii_uppercase()))
        {
            return Err(Error::Invalid);
        }
        if self.supplies.len() > 8 {
            return Err(Error::Invalid);
        }
        for supply in &mut self.supplies {
            if let Some(voltage) = supply.voltage_volts
                && !(1..=2_000).contains(&voltage)
            {
                return Err(Error::Invalid);
            }
            if supply.connectors.len() > 16 {
                return Err(Error::Invalid);
            }
            for connector in &mut supply.connectors {
                if let Connector::Other(label) = connector {
                    *label = optional_text(Some(label.clone()), 80)?.ok_or(Error::Invalid)?;
                }
            }
            if supply.connectors.contains(&Connector::Unknown) && supply.connectors.len() != 1 {
                return Err(Error::Invalid);
            }
            for (index, connector) in supply.connectors.iter().enumerate() {
                if supply.connectors[..index].contains(connector) {
                    return Err(Error::Invalid);
                }
            }
        }
        Ok(self)
    }
}

pub(crate) fn optional_text(value: Option<String>, limit: usize) -> Result<Option<String>, Error> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > limit
        || value
            .chars()
            .any(|value| value.is_control() && !['\n', '\r', '\t'].contains(&value))
    {
        return Err(Error::Invalid);
    }
    Ok(Some(value.to_owned()))
}

pub(crate) fn id(value: &str) -> Result<String, Error> {
    Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| Error::Invalid)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRequest {
    pub operation_id: String,
    pub spot: SpotInput,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditRequest {
    pub operation_id: String,
    pub expected_revision: u64,
    pub spot: SpotInput,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WithdrawRequest {
    pub operation_id: String,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VoteRequest {
    pub operation_id: String,
    pub expected_revision: u64,
    #[serde(deserialize_with = "Option::<ChargingVote>::deserialize")]
    pub vote: Option<ChargingVote>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Counts {
    pub up: u64,
    pub down: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PublicSpot {
    pub id: String,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub change_sequence: u64,
    pub last_verified_at_ms: Option<u64>,
    #[serde(flatten)]
    pub input: SpotInput,
    pub votes: Counts,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SpotState {
    pub id: String,
    pub revision: u64,
    pub status: String,
    pub change_sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spot: Option<PublicSpot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MutationReceipt {
    pub operation_id: String,
    pub applied_revision: u64,
    pub state: SpotState,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct VoteState {
    pub revision: u64,
    pub vote: Option<ChargingVote>,
    pub votes: Counts,
    pub spot_status: String,
    pub change_sequence: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct VoteReceipt {
    pub operation_id: String,
    pub applied_revision: u64,
    #[serde(flatten)]
    pub state: VoteState,
}

#[derive(Clone, Debug)]
pub(crate) enum SpotMutation {
    Edit(EditRequest),
    Withdraw(WithdrawRequest),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListQuery {
    pub south_e7: Option<i32>,
    pub north_e7: Option<i32>,
    pub west_e7: Option<i32>,
    pub east_e7: Option<i32>,
    pub cost: Option<ChargingCost>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

impl ListQuery {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if !(1..=100).contains(&self.limit.unwrap_or(50)) {
            return Err(Error::Invalid);
        }
        match (self.south_e7, self.north_e7, self.west_e7, self.east_e7) {
            (None, None, None, None) => {}
            (Some(south), Some(north), Some(west), Some(east)) => {
                Coordinate::from_fixed_parts(south, west).map_err(|_| Error::Invalid)?;
                Coordinate::from_fixed_parts(north, east).map_err(|_| Error::Invalid)?;
                if south > north {
                    return Err(Error::Invalid);
                }
            }
            _ => return Err(Error::Invalid),
        }
        if let Some(after) = &self.after {
            id(after)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SpotPage {
    pub items: Vec<PublicSpot>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangesQuery {
    pub after: Option<u64>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ChangesPage {
    pub items: Vec<SpotState>,
    pub next_cursor: u64,
    pub has_more: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportReason {
    WrongLocation,
    Unavailable,
    WrongAccess,
    WrongPrice,
    WrongVoltage,
    WrongConnector,
    Duplicate,
    Abusive,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportRequest {
    pub operation_id: String,
    pub reason: ReportReason,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ModerationAction {
    Hide,
    Restore,
    Remove,
    Correct { spot: SpotInput },
    Duplicate { canonical_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModerationRequest {
    pub operation_id: String,
    pub expected_revision: u64,
    pub note: String,
    pub action: ModerationAction,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolveRequest {
    pub operation_id: String,
    pub expected_revision: u64,
    pub note: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportStatus {
    Open,
    Resolved,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportsQuery {
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub status: Option<ReportStatus>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReportRecord {
    pub id: String,
    pub spot_id: String,
    pub reporter_id: String,
    pub revision: u64,
    pub reason: ReportReason,
    pub message: Option<String>,
    pub status: String,
    pub created_at_ms: u64,
    pub resolved_at_ms: Option<u64>,
    pub resolution_note: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReportReceipt {
    pub operation_id: String,
    pub id: String,
    pub applied_revision: u64,
    pub revision: u64,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReportsPage {
    pub items: Vec<ReportRecord>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ModeratorSpot {
    pub state: SpotState,
    pub owner_id: String,
    pub input: Option<SpotInput>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AuditRecord {
    pub sequence: u64,
    pub actor_id: String,
    pub target_id: String,
    pub action: String,
    pub note: String,
    pub canonical_id: Option<String>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AuditPage {
    pub items: Vec<AuditRecord>,
    pub next_cursor: u64,
    pub has_more: bool,
}
