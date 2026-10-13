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
        }
        Ok(())
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
    pub(crate) fn into_validated(self) -> Result<ValidatedListQuery, Error> {
        let limit = PageSize::new(self.limit)?;
        let bounds =
            DiscoveryBounds::new(self.south_e7, self.north_e7, self.west_e7, self.east_e7)?;
        Ok(ValidatedListQuery {
            after: self
                .after
                .map(|value| id(&value))
                .transpose()?
                .unwrap_or_default(),
            cost: self.cost,
            bounds,
            limit,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageSize(u32);

impl PageSize {
    const DEFAULT: u32 = 50;

    fn new(value: Option<u32>) -> Result<Self, Error> {
        let value = value.unwrap_or(Self::DEFAULT);
        (1..=100)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(Error::Invalid)
    }

    pub(crate) fn count(self) -> usize {
        self.0 as usize
    }
    pub(crate) fn lookahead(self) -> u32 {
        self.0 + 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveryBounds {
    south: i32,
    north: i32,
    west: i32,
    east: i32,
}

impl DiscoveryBounds {
    fn new(
        south: Option<i32>,
        north: Option<i32>,
        west: Option<i32>,
        east: Option<i32>,
    ) -> Result<Self, Error> {
        let (south, north, west, east) = match (south, north, west, east) {
            (None, None, None, None) => (-900_000_000, 900_000_000, -1_800_000_000, 1_800_000_000),
            (Some(south), Some(north), Some(west), Some(east)) => (south, north, west, east),
            _ => return Err(Error::Invalid),
        };
        Coordinate::from_fixed_parts(south, west).map_err(|_| Error::Invalid)?;
        Coordinate::from_fixed_parts(north, east).map_err(|_| Error::Invalid)?;
        if south > north {
            return Err(Error::Invalid);
        }
        Ok(Self {
            south,
            north,
            west,
            east,
        })
    }

    pub(crate) fn south_e7(self) -> i32 {
        self.south
    }
    pub(crate) fn north_e7(self) -> i32 {
        self.north
    }
    pub(crate) fn west_e7(self) -> i32 {
        self.west
    }
    pub(crate) fn east_e7(self) -> i32 {
        self.east
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedListQuery {
    after: String,
    cost: Option<ChargingCost>,
    bounds: DiscoveryBounds,
    limit: PageSize,
}

impl ValidatedListQuery {
    pub(crate) fn after(&self) -> &str {
        &self.after
    }
    pub(crate) fn cost(&self) -> Option<ChargingCost> {
        self.cost
    }
    pub(crate) fn bounds(&self) -> DiscoveryBounds {
        self.bounds
    }
    pub(crate) fn limit(&self) -> PageSize {
        self.limit
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ValidatedChangesQuery {
    after: u64,
    limit: PageSize,
}

impl ValidatedChangesQuery {
    pub(crate) fn after(&self) -> u64 {
        self.after
    }
    pub(crate) fn limit(&self) -> PageSize {
        self.limit
    }
}

impl ChangesQuery {
    pub(crate) fn into_validated(self) -> Result<ValidatedChangesQuery, Error> {
        let limit = PageSize::new(self.limit)?;
        let after = self.after.unwrap_or_default();
        if after > i64::MAX as u64 {
            return Err(Error::Invalid);
        }
        Ok(ValidatedChangesQuery { after, limit })
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
    pub operation_id: OperationId,
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
    pub operation_id: OperationId,
    pub expected_revision: u64,
    pub note: String,
    pub action: ModerationAction<Input>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolveRequest {
    pub operation_id: OperationId,
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

#[derive(Clone, Debug)]
pub(crate) struct ValidatedReportsQuery {
    after: String,
    status: Option<ReportStatus>,
    limit: PageSize,
}

impl ValidatedReportsQuery {
    pub(crate) fn after(&self) -> &str {
        &self.after
    }
    pub(crate) fn status(&self) -> Option<ReportStatus> {
        self.status
    }
    pub(crate) fn limit(&self) -> PageSize {
        self.limit
    }
}

impl ReportsQuery {
    pub(crate) fn into_validated(self) -> Result<ValidatedReportsQuery, Error> {
        Ok(ValidatedReportsQuery {
            after: self
                .after
                .map(|value| id(&value))
                .transpose()?
                .unwrap_or_default(),
            status: self.status,
            limit: PageSize::new(self.limit)?,
        })
    }
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
    pub operation_id: OperationId,
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
    use super::{ChangesQuery, CreateRequest, ListQuery, OperationId, ReportStatus, ReportsQuery};

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

    #[test]
    fn list_page_size_and_bounds_are_validated_as_a_unit() {
        let default = list(None, None, None, None, None, None)
            .into_validated()
            .unwrap();
        assert_eq!(default.limit().count(), 50);
        assert_eq!(default.limit().lookahead(), 51);
        for size in [1, 100] {
            let query = list(None, None, None, None, None, Some(size))
                .into_validated()
                .unwrap();
            assert_eq!(query.limit().count(), size as usize);
            assert_eq!(query.limit().lookahead(), size + 1);
        }
        for size in [0, 101] {
            assert!(
                list(None, None, None, None, None, Some(size))
                    .into_validated()
                    .is_err()
            );
        }

        assert!(
            list(Some(0), None, Some(0), Some(1), None, None)
                .into_validated()
                .is_err()
        );
        assert!(
            list(Some(2), Some(1), Some(0), Some(1), None, None)
                .into_validated()
                .is_err()
        );
        assert!(
            list(Some(0), Some(900_000_001), Some(0), Some(1), None, None)
                .into_validated()
                .is_err()
        );
        let antimeridian = list(
            Some(-10),
            Some(10),
            Some(1_700_000_000),
            Some(-1_700_000_000),
            None,
            None,
        )
        .into_validated()
        .unwrap();
        assert_eq!(antimeridian.bounds().west_e7(), 1_700_000_000);
        assert_eq!(antimeridian.bounds().east_e7(), -1_700_000_000);
        assert_eq!(antimeridian.bounds().south_e7(), -10);
        assert_eq!(antimeridian.bounds().north_e7(), 10);
    }

    #[test]
    fn cursors_are_normalized_and_bounded() {
        let uppercase = "00000000-0000-4000-8000-0000000000AB";
        let validated = list(None, None, None, None, Some(uppercase.into()), None)
            .into_validated()
            .unwrap();
        assert_eq!(validated.after(), "00000000-0000-4000-8000-0000000000ab");
        assert!(
            list(None, None, None, None, Some("bad".into()), None)
                .into_validated()
                .is_err()
        );

        let reports = ReportsQuery {
            after: Some(uppercase.into()),
            limit: None,
            status: Some(ReportStatus::Open),
        }
        .into_validated()
        .unwrap();
        assert_eq!(reports.after(), "00000000-0000-4000-8000-0000000000ab");
        assert_eq!(reports.status(), Some(ReportStatus::Open));
        assert_eq!(reports.limit().count(), 50);
        assert!(
            ReportsQuery {
                after: Some("bad".into()),
                limit: None,
                status: None
            }
            .into_validated()
            .is_err()
        );

        let changes = ChangesQuery {
            after: Some(i64::MAX as u64),
            limit: Some(100),
        }
        .into_validated()
        .unwrap();
        assert_eq!(changes.after(), i64::MAX as u64);
        assert_eq!(changes.limit().lookahead(), 101);
        assert!(
            ChangesQuery {
                after: Some(i64::MAX as u64 + 1),
                limit: None
            }
            .into_validated()
            .is_err()
        );
        assert!(
            ChangesQuery {
                after: None,
                limit: Some(0)
            }
            .into_validated()
            .is_err()
        );
    }

    fn list(
        south_e7: Option<i32>,
        north_e7: Option<i32>,
        west_e7: Option<i32>,
        east_e7: Option<i32>,
        after: Option<String>,
        limit: Option<u32>,
    ) -> ListQuery {
        ListQuery {
            south_e7,
            north_e7,
            west_e7,
            east_e7,
            cost: None,
            after,
            limit,
        }
    }
}
