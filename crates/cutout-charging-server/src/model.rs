use cutout_ride_maps::{
    ChargingCost, ChargingDetails, ChargingPaidTerms as PaidTerms, ChargingPrice as Price,
    ChargingSupply as Supply, ChargingVenue as Venue, ChargingVote, Coordinate, NewChargingSpot,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Error;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pin {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
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

/// Only validation can construct this input; persistence accepts this state.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct ValidatedSpotInput(SpotInput);

impl ValidatedSpotInput {
    pub(crate) fn as_input(&self) -> &SpotInput {
        &self.0
    }
}

impl SpotInput {
    pub(crate) fn validate(mut self) -> Result<ValidatedSpotInput, Error> {
        let coordinate =
            Coordinate::from_fixed_parts(self.coordinate.latitude_e7, self.coordinate.longitude_e7)
                .map_err(|_| Error::Invalid)?;
        let details = ChargingDetails::new(
            self.name.as_deref(),
            self.venue,
            self.hours.as_deref(),
            self.paid_terms,
            self.price.clone(),
            self.supplies.clone(),
        )
        .map_err(|_| Error::Invalid)?;
        let validated = NewChargingSpot::new(coordinate, &self.access_note, self.cost)
            .map_err(|_| Error::Invalid)?
            .with_details(details)
            .map_err(|_| Error::Invalid)?;
        validated
            .access_note()
            .as_str()
            .clone_into(&mut self.access_note);
        self.name = validated.details().name().map(str::to_owned);
        self.hours = validated.details().hours().map(str::to_owned);
        Ok(ValidatedSpotInput(self))
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
pub(crate) struct CreateRequest<Input = SpotInput> {
    pub operation_id: String,
    pub spot: Input,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditRequest<Input = SpotInput> {
    pub operation_id: String,
    pub expected_revision: u64,
    pub spot: Input,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SpotStatus {
    Published,
    Hidden,
    Withdrawn,
    Removed,
}

impl SpotStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Hidden => "hidden",
            Self::Withdrawn => "withdrawn",
            Self::Removed => "removed",
        }
    }

    pub(crate) fn allows_edit(self) -> bool {
        match self {
            Self::Published | Self::Hidden => true,
            Self::Withdrawn | Self::Removed => false,
        }
    }
}

impl rusqlite::types::FromSql for SpotStatus {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "published" => Ok(Self::Published),
            "hidden" => Ok(Self::Hidden),
            "withdrawn" => Ok(Self::Withdrawn),
            "removed" => Ok(Self::Removed),
            _ => Err(rusqlite::types::FromSqlError::Other(Box::new(
                Error::Unavailable,
            ))),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SpotState {
    pub id: String,
    pub revision: u64,
    pub status: SpotStatus,
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
    pub spot_status: SpotStatus,
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
    Edit(EditRequest<ValidatedSpotInput>),
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
pub(crate) enum ModerationAction<Input = SpotInput> {
    Hide,
    Restore,
    Remove,
    Correct { spot: Input },
    Duplicate { canonical_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModerationRequest<Input = SpotInput> {
    pub operation_id: String,
    pub expected_revision: u64,
    pub note: String,
    pub action: ModerationAction<Input>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolveRequest {
    pub operation_id: String,
    pub expected_revision: u64,
    pub note: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportStatus {
    Open,
    Resolved,
}

impl ReportStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

impl rusqlite::types::FromSql for ReportStatus {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            _ => Err(rusqlite::types::FromSqlError::Other(Box::new(
                Error::Unavailable,
            ))),
        }
    }
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
    pub status: ReportStatus,
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
    pub status: ReportStatus,
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

#[cfg(test)]
mod tests {
    use super::{CreateRequest, id};

    #[test]
    fn shared_validation_preserves_the_existing_create_fingerprint_bytes() {
        let wire = r#"{"operation_id":"00000000-0000-4000-8000-000000000001","spot":{"coordinate":{"latitude_e7":400000000,"longitude_e7":-1050000000},"access_note":" Ask staff. ","cost":"paid","name":" Cafe ","venue":"cafe","hours":" Open hours ","paid_terms":"direct_fee","price":{"amount_minor":125,"currency":"USD","minor_unit_exponent":2,"basis":"per_hour"},"supplies":[{"voltage_volts":120,"connectors":[{"other":" Socket X "}]}]}}"#;
        let raw: CreateRequest = serde_json::from_str(wire).expect("wire fixture");
        let validated = CreateRequest {
            operation_id: id(&raw.operation_id).expect("operation identity"),
            spot: raw.spot.validate().expect("valid input"),
        };
        let previous_bytes = r#"{"operation_id":"00000000-0000-4000-8000-000000000001","spot":{"coordinate":{"latitude_e7":400000000,"longitude_e7":-1050000000},"access_note":"Ask staff.","cost":"paid","name":"Cafe","venue":"cafe","hours":"Open hours","paid_terms":"direct_fee","price":{"amount_minor":125,"currency":"USD","minor_unit_exponent":2,"basis":"per_hour"},"supplies":[{"voltage_volts":120,"connectors":[{"other":"Socket X"}]}]}}"#;
        assert_eq!(
            serde_json::to_string(&validated).expect("canonical request"),
            previous_bytes
        );
    }
}
