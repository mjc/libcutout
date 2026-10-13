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
        let minimum = NewChargingSpot::new(coordinate, &self.access_note, self.cost)
            .map_err(|_| Error::Invalid)?;
        minimum
            .access_note()
            .as_str()
            .clone_into(&mut self.access_note);
        self.name = optional_text(self.name, 120)?;
        self.hours = optional_text(self.hours, 500)?;
        self.validate_cost_metadata()?;
        if let Some(price) = &self.price {
            price.validate()?;
        }
        self.validate_supplies()?;
        Ok(ValidatedSpotInput(self))
    }

    fn validate_cost_metadata(&self) -> Result<(), Error> {
        if self.cost == ChargingCost::Free && (self.paid_terms.is_some() || self.price.is_some()) {
            return Err(Error::Invalid);
        }
        Ok(())
    }

    fn validate_supplies(&mut self) -> Result<(), Error> {
        if self.supplies.len() > 8 {
            return Err(Error::Invalid);
        }
        for supply in &mut self.supplies {
            supply.validate()?;
        }
        Ok(())
    }
}

impl Price {
    fn validate(&self) -> Result<(), Error> {
        if self.amount_minor == 0
            || self.amount_minor > 9_000_000_000_000_000
            || self.minor_unit_exponent > 4
            || self.currency.len() != 3
            || !self
                .currency
                .bytes()
                .all(|value| value.is_ascii_uppercase())
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

impl Supply {
    fn validate(&mut self) -> Result<(), Error> {
        if let Some(voltage) = self.voltage_volts
            && !(1..=2_000).contains(&voltage)
        {
            return Err(Error::Invalid);
        }
        if self.connectors.len() > 16 {
            return Err(Error::Invalid);
        }
        for connector in &mut self.connectors {
            connector.normalize()?;
        }
        if self.connectors.contains(&Connector::Unknown) && self.connectors.len() != 1 {
            return Err(Error::Invalid);
        }
        for (index, connector) in self.connectors.iter().enumerate() {
            if self.connectors[..index].contains(connector) {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
}

impl Connector {
    fn normalize(&mut self) -> Result<(), Error> {
        if let Self::Other(label) = self {
            *label = optional_text(Some(std::mem::take(label)), 80)?.ok_or(Error::Invalid)?;
            if Self::is_known_id(label) {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }

    fn is_known_id(label: &str) -> bool {
        let normalized = label.to_ascii_lowercase();
        let value = serde::de::value::StrDeserializer::<serde::de::value::Error>::new(&normalized);
        Self::deserialize(value).is_ok()
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OperationId(String);

impl OperationId {
    pub(crate) fn parse(value: &str) -> Result<Self, Error> {
        id(value).map(Self)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for OperationId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OperationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl rusqlite::types::ToSql for OperationId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.0.as_str()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRequest<Input = SpotInput> {
    pub operation_id: OperationId,
    pub spot: Input,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditRequest<Input = SpotInput> {
    pub operation_id: OperationId,
    pub expected_revision: u64,
    pub spot: Input,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WithdrawRequest {
    pub operation_id: OperationId,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VoteRequest {
    pub operation_id: OperationId,
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
    pub operation_id: OperationId,
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
    pub operation_id: OperationId,
    pub applied_revision: u64,
    #[serde(flatten)]
    pub state: VoteState,
}

#[derive(Clone, Debug)]
pub(crate) enum SpotMutation {
    Edit(EditRequest<ValidatedSpotInput>),
    Withdraw(WithdrawRequest),
}

mod moderation;
mod queries;

pub(crate) use moderation::{
    AuditPage, AuditRecord, ModerationAction, ModerationNote, ModerationRequest, ModeratorSpot,
    ReportReason, ReportReceipt, ReportRecord, ReportRequest, ReportStatus, ReportsPage,
    ResolveRequest, ValidatedReportRequest,
};
pub(crate) use queries::{
    ChangesPage, ChangesQuery, ListQuery, ReportsQuery, SpotPage, ValidatedChangesQuery,
    ValidatedListQuery, ValidatedReportsQuery,
};

#[cfg(test)]
mod tests {
    use super::{Connector, CreateRequest, OperationId};
    use crate::Error;

    #[test]
    fn custom_connector_labels_cannot_alias_catalogue_ids() {
        for label in [" type_c ", "TYPE_C", "J1772", "unknown", "nema_5_15", " \t"] {
            let mut connector = Connector::Other(label.into());
            assert_eq!(connector.normalize(), Err(Error::Invalid), "{label:?}");
        }
        let mut connector = Connector::Other(" Socket X ".into());
        connector.normalize().expect("custom label");
        assert_eq!(connector, Connector::Other("Socket X".into()));
    }

    #[test]
    fn validated_spot_keeps_canonical_request_bytes() {
        let wire = r#"{"operation_id":"00000000-0000-4000-8000-000000000001","spot":{"coordinate":{"latitude_e7":400000000,"longitude_e7":-1050000000},"access_note":" Ask staff. ","cost":"paid","name":" Cafe ","venue":"cafe","hours":" Open hours ","paid_terms":"direct_fee","price":{"amount_minor":125,"currency":"USD","minor_unit_exponent":2,"basis":"per_hour"},"supplies":[{"voltage_volts":120,"connectors":[{"other":" Socket X "}]}]}}"#;
        let raw: CreateRequest = serde_json::from_str(wire).expect("request fixture");
        let validated = CreateRequest {
            operation_id: raw.operation_id,
            spot: raw.spot.validate().expect("valid spot metadata"),
        };
        let expected = r#"{"operation_id":"00000000-0000-4000-8000-000000000001","spot":{"coordinate":{"latitude_e7":400000000,"longitude_e7":-1050000000},"access_note":"Ask staff.","cost":"paid","name":"Cafe","venue":"cafe","hours":"Open hours","paid_terms":"direct_fee","price":{"amount_minor":125,"currency":"USD","minor_unit_exponent":2,"basis":"per_hour"},"supplies":[{"voltage_volts":120,"connectors":[{"other":"Socket X"}]}]}}"#;
        assert_eq!(serde_json::to_string(&validated).unwrap(), expected);
    }

    #[test]
    fn operation_id_deserialization_normalizes_and_rejects_invalid_uuids() {
        let value = r#""00000000-0000-4000-8000-0000000000AB""#;
        let operation_id: OperationId = serde_json::from_str(value).expect("valid UUID");
        assert_eq!(
            operation_id.as_str(),
            "00000000-0000-4000-8000-0000000000ab"
        );
        assert_eq!(
            serde_json::to_string(&operation_id).expect("string form"),
            r#""00000000-0000-4000-8000-0000000000ab""#
        );
        assert!(serde_json::from_str::<OperationId>(r#""bad""#).is_err());
        assert!(serde_json::from_str::<OperationId>("42").is_err());
    }
}
