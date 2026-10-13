//! Charging metadata and outbox commands executed by the existing database owner.
use super::{
    Command as DatabaseCommand, GeoBounds, MapPointCursor, MapPointId, QueryLimit, Reply,
    RideDatabase, SpatialRowId, SpatialSchemaState, StorageError, ensure_spatial_schema,
};
use cutout_ride_maps::{
    ChargingConnector, ChargingCost, ChargingDetails, ChargingPaidTerms, ChargingPrice,
    ChargingSpotId, ChargingSupply, ChargingVenue, ChargingVote, Coordinate, LocationSample,
    LocationSource, NewChargingSpot, distance_between_millimetres,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum number of pending community writes retained by an installation.
pub const CHARGING_OUTBOX_CAPACITY: u32 = 1_024;
pub(super) const SCHEMA: &str = "
CREATE TABLE charging_spots (
 point_id BLOB PRIMARY KEY NOT NULL REFERENCES map_points(id) ON DELETE CASCADE,
 input TEXT CHECK(input IS NULL OR (json_valid(input) AND length(CAST(input AS BLOB)) <= 32768)),
 owned INTEGER NOT NULL CHECK(owned IN (0,1)),
 status TEXT NOT NULL CHECK(status IN ('queued','published','hidden','withdrawn','removed')),
 local_revision INTEGER NOT NULL CHECK(local_revision > 0),
 remote_id TEXT UNIQUE, remote_revision INTEGER NOT NULL DEFAULT 0 CHECK(remote_revision >= 0),
 change_sequence INTEGER NOT NULL DEFAULT 0 CHECK(change_sequence >= 0),
 verified_at_ms INTEGER CHECK(verified_at_ms IS NULL OR verified_at_ms >= 0),
 vote TEXT CHECK(vote IS NULL OR vote IN ('up','down')),
 vote_revision INTEGER NOT NULL DEFAULT 0 CHECK(vote_revision >= 0),
 votes_up INTEGER NOT NULL DEFAULT 0 CHECK(votes_up >= 0),
 votes_down INTEGER NOT NULL DEFAULT 0 CHECK(votes_down >= 0),
 last_ack TEXT
);
CREATE TABLE charging_outbox (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 operation_id TEXT NOT NULL UNIQUE,
 point_id BLOB NOT NULL REFERENCES charging_spots(point_id),
 action TEXT NOT NULL CHECK(json_valid(action) AND length(CAST(action AS BLOB)) <= 32768),
 local_revision INTEGER NOT NULL CHECK(local_revision > 0),
 target TEXT CHECK(target IS NULL OR json_valid(target))
);
CREATE INDEX charging_outbox_point ON charging_outbox(point_id,sequence);
";

/// A checked revision, including zero for a contributor who has never voted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct ChargingRevision(u64);
impl ChargingRevision {
    /// Checks that SQLite can retain the revision exactly.
    /// # Errors
    /// Rejects values exceeding SQLite's signed integer range.
    pub fn new(value: u64) -> Result<Self, StorageError> {
        i64::try_from(value).map_err(|_| StorageError::ChargingInvalidInput)?;
        Ok(Self(value))
    }
    /// Returns the exact revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
    fn next(self) -> Result<Self, StorageError> {
        Self::new(
            self.0
                .checked_add(1)
                .ok_or(StorageError::ChargingConflict)?,
        )
    }
}
impl TryFrom<u64> for ChargingRevision {
    type Error = StorageError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<ChargingRevision> for u64 {
    fn from(value: ChargingRevision) -> Self {
        value.0
    }
}

/// An immutable identity used for server idempotency across retries and reopen.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ChargingOperationId(Uuid);
impl ChargingOperationId {
    /// Restores an existing operation identity.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    /// Returns the operation UUID.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }
}

/// Local queue state or an acknowledged server state; legacy points have no charging row.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingStatus {
    /// A new spot awaits server acceptance.
    Queued,
    /// The server accepted the spot for discovery.
    Published,
    /// Moderation hid the spot.
    Hidden,
    /// The contributor withdrew the spot.
    Withdrawn,
    /// Moderation removed the spot.
    Removed,
}
impl ChargingStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Published => "published",
            Self::Hidden => "hidden",
            Self::Withdrawn => "withdrawn",
            Self::Removed => "removed",
        }
    }
    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "queued" => Ok(Self::Queued),
            "published" => Ok(Self::Published),
            "hidden" => Ok(Self::Hidden),
            "withdrawn" => Ok(Self::Withdrawn),
            "removed" => Ok(Self::Removed),
            _ => Err(StorageError::ChargingInvalidInput),
        }
    }
    const fn visible(self) -> bool {
        match self {
            Self::Queued | Self::Published => true,
            Self::Hidden | Self::Withdrawn | Self::Removed => false,
        }
    }
}

/// Durable local detail, using the existing map-point identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredChargingSpot {
    /// Existing local map-point UUID.
    pub point_id: MapPointId,
    /// Optional public fields; tombstones have no retained public body.
    pub input: Option<NewChargingSpot>,
    /// True only for spots created by this installation.
    pub owned: bool,
    /// Durable local state, independent of unacknowledged requests.
    pub status: ChargingStatus,
    /// Optimistic local write revision.
    pub local_revision: ChargingRevision,
    /// Server identity, assigned on publication.
    pub remote_id: Option<ChargingSpotId>,
    /// Last acknowledged server spot revision.
    pub remote_revision: ChargingRevision,
    /// Directory change sequence, independent of spot revision.
    pub change_sequence: ChargingRevision,
    /// Server-confirmed access verification time.
    pub verified_at_ms: Option<u64>,
    /// Requested contributor vote; pending requests do not alter counts.
    pub vote: Option<ChargingVote>,
    /// Server-confirmed contributor vote revision.
    pub vote_revision: ChargingRevision,
    /// Last server-confirmed positive count.
    pub votes_up: u64,
    /// Last server-confirmed negative count.
    pub votes_down: u64,
}

/// Result returned only after local data and its outbox operation commit together.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargingWriteReceipt {
    /// Retained map-point identity.
    pub point_id: MapPointId,
    /// Revision committed by this command.
    pub revision: ChargingRevision,
    /// Durable community operation identity.
    pub operation_id: ChargingOperationId,
}

/// Validated public directory data received by the future sync client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingDirectoryEntry {
    /// Server-assigned identity; distinct from an existing local identity.
    pub id: ChargingSpotId,
    /// Current public body; present only for published spots.
    pub input: Option<NewChargingSpot>,
    /// Server spot revision.
    pub revision: ChargingRevision,
    /// Server change sequence.
    pub change_sequence: ChargingRevision,
    /// Server status; Queued is never a server state.
    pub status: ChargingStatus,
    /// Confirmed access verification timestamp.
    pub verified_at_ms: Option<u64>,
    /// Contributor vote returned by an authenticated read or receipt.
    pub vote: Option<ChargingVote>,
    /// Contributor vote revision.
    pub vote_revision: ChargingRevision,
    /// Aggregate positive votes.
    pub votes_up: u64,
    /// Aggregate negative votes.
    pub votes_down: u64,
}
impl ChargingDirectoryEntry {
    fn validate(&self) -> Result<(), StorageError> {
        if self.status == ChargingStatus::Queued
            || self.revision.get() == 0
            || self.change_sequence.get() == 0
            || (self.status == ChargingStatus::Published) != self.input.is_some()
        {
            return Err(StorageError::ChargingInvalidInput);
        }
        for value in [self.votes_up, self.votes_down]
            .into_iter()
            .chain(self.verified_at_ms)
        {
            ChargingRevision::new(value)?;
        }
        Ok(())
    }
}

/// Frozen endpoint target and revision for an idempotent server retry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChargingTarget {
    /// The server assigns an identity on create.
    Create,
    /// Subsequent commands use the server identity and a frozen expected revision.
    Existing {
        /// Server identity.
        id: ChargingSpotId,
        /// Spot revision or, for votes, contributor vote revision.
        expected_revision: ChargingRevision,
    },
}

/// Public action retained in the outbox, without ride or credential data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChargingAction {
    /// Create a spot with validated fields.
    Create(NewChargingSpot),
    /// Replace public fields and move the existing pin.
    Edit(NewChargingSpot),
    /// Withdraw the contributor's spot.
    Withdraw,
    /// Confirm access without changing power facts.
    Verify,
    /// Set or clear a contributor vote; never increment aggregate counts locally.
    Vote(Option<ChargingVote>),
}

/// One retryable command; obtaining it freezes the target in SQLite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingOperation {
    /// Stable idempotency key.
    pub id: ChargingOperationId,
    /// Local point identity.
    pub point_id: MapPointId,
    /// Committed local revision.
    pub local_revision: ChargingRevision,
    /// Frozen server target.
    pub target: ChargingTarget,
    /// Validated public action.
    pub action: ChargingAction,
}

/// Optional discovery filters applied before pagination or distance ranking.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChargingFilter {
    /// Explicit access category, when selected.
    pub cost: Option<ChargingCost>,
    /// Confirmed connector only; an absent supply does not match.
    pub connector: Option<ChargingConnector>,
    /// Confirmed supply voltage only.
    pub voltage: Option<cutout_ride_maps::SupplyVoltage>,
}

/// Bounded page using the existing stable UUID cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingPage {
    /// Matching detail rows.
    pub spots: Vec<StoredChargingSpot>,
    /// Cursor returned only when another matching row exists.
    pub next_cursor: Option<MapPointCursor>,
}

impl ChargingAction {
    const fn is_vote(&self) -> bool {
        match self {
            Self::Vote(_) => true,
            Self::Create(_) | Self::Edit(_) | Self::Withdraw | Self::Verify => false,
        }
    }
}

pub(super) enum Command {
    Save {
        input: NewChargingSpot,
        reply: Reply<ChargingWriteReceipt>,
    },
    Change {
        id: MapPointId,
        expected: ChargingRevision,
        action: ChargingAction,
        reply: Reply<ChargingWriteReceipt>,
    },
    Detail {
        id: MapPointId,
        reply: Reply<Option<StoredChargingSpot>>,
    },
    Page {
        bounds: GeoBounds,
        filter: ChargingFilter,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
        reply: Reply<ChargingPage>,
    },
    Nearby {
        bounds: GeoBounds,
        origin: Coordinate,
        filter: ChargingFilter,
        limit: QueryLimit,
        reply: Reply<Vec<StoredChargingSpot>>,
    },
    Next {
        reply: Reply<Option<ChargingOperation>>,
    },
    Cache {
        entry: ChargingDirectoryEntry,
        reply: Reply<MapPointId>,
    },
    Acknowledge {
        id: ChargingOperationId,
        applied: ChargingRevision,
        entry: ChargingDirectoryEntry,
        reply: Reply<MapPointId>,
    },
    Count {
        reply: Reply<u32>,
    },
}
impl RideDatabase {
    /// Atomically stores a new minimum-field spot and queues its community submission.
    /// # Errors
    /// Returns storage failures or a full outbox; neither leaves a partial pin.
    pub fn save_charging_spot(
        &self,
        input: NewChargingSpot,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Save { input, reply }))
    }
    /// Edits/moves one owned spot without changing its UUID.
    /// # Errors
    /// Rejects stale revisions, tombstones, directory-owned pins and full queues.
    pub fn edit_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
        input: NewChargingSpot,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Edit(input))
    }
    /// Withdraws an owned pin and queues the server withdrawal, including before first publication.
    /// # Errors
    /// Rejects stale revisions, tombstones, non-owned pins and full queues.
    pub fn withdraw_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Withdraw)
    }
    /// Queues explicit access verification without fabricating a confirmation timestamp.
    /// # Errors
    /// Rejects stale revisions and spots not currently available for verification.
    pub fn verify_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Verify)
    }
    /// Queues the requested vote state without changing confirmed counts.
    /// # Errors
    /// Rejects stale revisions and spots not currently available for voting.
    pub fn vote_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
        vote: Option<ChargingVote>,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Vote(vote))
    }
    fn charging_change(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
        action: ChargingAction,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Change {
                id,
                expected,
                action,
                reply,
            })
        })
    }
    /// Reads charging detail; legacy points have no charging row.
    /// # Errors
    /// Returns worker/storage failures, never invented defaults for absent metadata.
    pub fn charging_spot(
        &self,
        id: MapPointId,
    ) -> Result<Option<StoredChargingSpot>, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Detail { id, reply }))
    }
    /// Queries visible charging spots through the existing `RTree` with bounded pagination.
    /// # Errors
    /// Returns worker or storage failures.
    pub fn charging_spots_in_bounds(
        &self,
        bounds: GeoBounds,
        filter: ChargingFilter,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
    ) -> Result<ChargingPage, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Page {
                bounds,
                filter,
                cursor,
                limit,
                reply,
            })
        })
    }
    /// Ranks all matching spots in the bounds by exact great-circle distance with bounded memory.
    /// # Errors
    /// Returns worker or storage failures.
    pub fn nearby_charging_spots(
        &self,
        bounds: GeoBounds,
        origin: Coordinate,
        filter: ChargingFilter,
        limit: QueryLimit,
    ) -> Result<Vec<StoredChargingSpot>, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Nearby {
                bounds,
                origin,
                filter,
                limit,
                reply,
            })
        })
    }
    /// Freezes and reads the oldest pending command; repeated reads return identical retry data.
    /// # Errors
    /// Returns worker/storage failures or inconsistent retained queue data.
    pub fn next_charging_operation(&self) -> Result<Option<ChargingOperation>, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Next { reply }))
    }
    /// Returns the bounded pending queue size.
    /// # Errors
    /// Returns worker/storage failures.
    pub fn charging_outbox_count(&self) -> Result<u32, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Count { reply }))
    }
    /// Caches a server snapshot using the same map-point owner and spatial index.
    /// # Errors
    /// Rejects malformed/stale snapshots and refuses to overwrite pending local changes.
    pub fn cache_charging_spot(
        &self,
        entry: ChargingDirectoryEntry,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Cache { entry, reply }))
    }
    /// Atomically accepts a matching server receipt and advances the pending queue.
    /// # Errors
    /// Rejects incorrect identities/revisions and receipts for an unclaimed or later operation.
    pub fn acknowledge_charging_operation(
        &self,
        id: ChargingOperationId,
        applied: ChargingRevision,
        entry: ChargingDirectoryEntry,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Acknowledge {
                id,
                applied,
                entry,
                reply,
            })
        })
    }
}

pub(super) fn execute(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    command: Command,
) {
    match command {
        Command::Save { input, reply } => {
            let _ = reply.send(save(connection, spatial, &input));
        }
        Command::Change {
            id,
            expected,
            action,
            reply,
        } => {
            let _ = reply.send(change(connection, spatial, id, expected, &action));
        }
        Command::Detail { id, reply } => {
            let _ = reply.send(detail(connection, id));
        }
        Command::Page {
            bounds,
            filter,
            cursor,
            limit,
            reply,
        } => {
            let _ = reply.send(page(connection, spatial, bounds, &filter, cursor, limit));
        }
        Command::Nearby {
            bounds,
            origin,
            filter,
            limit,
            reply,
        } => {
            let _ = reply.send(nearby(connection, spatial, bounds, origin, &filter, limit));
        }
        Command::Next { reply } => {
            let _ = reply.send(next(connection));
        }
        Command::Cache { entry, reply } => {
            let _ = reply.send(cache(connection, spatial, &entry));
        }
        Command::Acknowledge {
            id,
            applied,
            entry,
            reply,
        } => {
            let _ = reply.send(acknowledge(connection, spatial, id, applied, &entry));
        }
        Command::Count { reply } => {
            let _ = reply.send(count(connection));
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    coordinate: Pin,
    access_note: String,
    cost: ChargingCost,
    name: Option<String>,
    venue: Option<ChargingVenue>,
    hours: Option<String>,
    paid_terms: Option<ChargingPaidTerms>,
    price: Option<ChargingPrice>,
    supplies: Vec<ChargingSupply>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    latitude_e7: i32,
    longitude_e7: i32,
}
impl Input {
    fn validated(self) -> Result<NewChargingSpot, StorageError> {
        let invalid = || StorageError::InvalidStoredValue {
            field: "charging input",
            value: "invalid domain value".to_owned(),
        };
        let coordinate =
            Coordinate::from_fixed_parts(self.coordinate.latitude_e7, self.coordinate.longitude_e7)
                .map_err(|_| invalid())?;
        let details = ChargingDetails::new(
            self.name.as_deref(),
            self.venue,
            self.hours.as_deref(),
            self.paid_terms,
            self.price,
            self.supplies,
        )
        .map_err(|_| invalid())?;
        NewChargingSpot::new(coordinate, &self.access_note, self.cost)
            .map_err(|_| invalid())?
            .with_details(details)
            .map_err(|_| invalid())
    }
}
#[derive(Deserialize, Serialize)]
enum ActionWire {
    Create(Input),
    Edit(Input),
    Withdraw,
    Verify,
    Vote(Option<ChargingVote>),
}
fn encode<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|_| StorageError::ChargingInvalidInput)
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(|_| StorageError::InvalidStoredValue {
        field: "charging data",
        value: "invalid encoding".to_owned(),
    })
}
fn input_json(input: &NewChargingSpot) -> Result<String, StorageError> {
    encode(&input.community_submission())
}
fn action_json(action: &ChargingAction) -> Result<String, StorageError> {
    let input = |value: &NewChargingSpot| decode::<Input>(&input_json(value)?);
    encode(&match action {
        ChargingAction::Create(value) => ActionWire::Create(input(value)?),
        ChargingAction::Edit(value) => ActionWire::Edit(input(value)?),
        ChargingAction::Withdraw => ActionWire::Withdraw,
        ChargingAction::Verify => ActionWire::Verify,
        ChargingAction::Vote(value) => ActionWire::Vote(*value),
    })
}
fn action_from_json(value: &str) -> Result<ChargingAction, StorageError> {
    Ok(match decode(value)? {
        ActionWire::Create(value) => ChargingAction::Create(value.validated()?),
        ActionWire::Edit(value) => ChargingAction::Edit(value.validated()?),
        ActionWire::Withdraw => ChargingAction::Withdraw,
        ActionWire::Verify => ChargingAction::Verify,
        ActionWire::Vote(value) => ChargingAction::Vote(value),
    })
}
fn uuid(value: &[u8]) -> Result<MapPointId, StorageError> {
    Uuid::from_slice(value)
        .map(MapPointId::from_uuid)
        .map_err(|_| StorageError::ChargingInvalidInput)
}
fn count(connection: &Connection) -> Result<u32, StorageError> {
    Ok(connection.query_row("SELECT COUNT(*) FROM charging_outbox", [], |row| row.get(0))?)
}
fn queue(
    connection: &Connection,
    id: MapPointId,
    revision: ChargingRevision,
    action: &ChargingAction,
) -> Result<ChargingWriteReceipt, StorageError> {
    if count(connection)? >= CHARGING_OUTBOX_CAPACITY {
        return Err(StorageError::ChargingOutboxFull);
    }
    let operation_id = ChargingOperationId(Uuid::new_v4());
    connection.execute("INSERT INTO charging_outbox(operation_id,point_id,action,local_revision) VALUES (?1,?2,?3,?4)", params![operation_id.0.to_string(), id.uuid().as_bytes(), action_json(action)?, revision.get()])?;
    Ok(ChargingWriteReceipt {
        point_id: id,
        revision,
        operation_id,
    })
}
fn index(
    connection: &Connection,
    id: MapPointId,
    input: &NewChargingSpot,
    visible: bool,
) -> Result<(), StorageError> {
    let pin = input.coordinate();
    connection.execute("INSERT INTO map_points(id,name,latitude_e7,longitude_e7) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,latitude_e7=excluded.latitude_e7,longitude_e7=excluded.longitude_e7", params![id.uuid().as_bytes(), input.details().name().unwrap_or("Charging spot"), pin.latitude().as_i32(), pin.longitude().as_i32()])?;
    connection.execute(
        "INSERT INTO map_point_spatial_keys(point_id) VALUES (?1) ON CONFLICT(point_id) DO NOTHING",
        [id.uuid().as_bytes()],
    )?;
    let spatial_id = SpatialRowId::from_sqlite(connection.query_row(
        "SELECT rtree_id FROM map_point_spatial_keys WHERE point_id=?1",
        [id.uuid().as_bytes()],
        |row| row.get(0),
    )?)?;
    connection.execute(
        "DELETE FROM map_points_rtree WHERE id=?1",
        [spatial_id.get()],
    )?;
    if visible {
        connection.execute(
            "INSERT INTO map_points_rtree VALUES (?1,?2,?2,?3,?3)",
            params![
                spatial_id.get(),
                pin.latitude().as_i32(),
                pin.longitude().as_i32()
            ],
        )?;
    }
    Ok(())
}
fn remove_index(connection: &Connection, id: MapPointId) -> Result<(), StorageError> {
    connection.execute("DELETE FROM map_points_rtree WHERE id IN (SELECT rtree_id FROM map_point_spatial_keys WHERE point_id=?1)", [id.uuid().as_bytes()])?;
    Ok(())
}
fn save(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    input: &NewChargingSpot,
) -> Result<ChargingWriteReceipt, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let id = MapPointId::new();
    index(&transaction, id, input, true)?;
    transaction.execute("INSERT INTO charging_spots(point_id,input,owned,status,local_revision) VALUES (?1,?2,1,'queued',1)", params![id.uuid().as_bytes(), input_json(input)?])?;
    let receipt = queue(
        &transaction,
        id,
        ChargingRevision(1),
        &ChargingAction::Create(input.clone()),
    )?;
    transaction.commit()?;
    Ok(receipt)
}
fn detail(
    connection: &Connection,
    id: MapPointId,
) -> Result<Option<StoredChargingSpot>, StorageError> {
    let mut statement = connection.prepare("SELECT input,owned,status,local_revision,remote_id,remote_revision,change_sequence,verified_at_ms,vote,vote_revision,votes_up,votes_down FROM charging_spots WHERE point_id=?1")?;
    let mut rows = statement.query([id.uuid().as_bytes()])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let input: Option<String> = row.get(0)?;
    let remote: Option<String> = row.get(4)?;
    let vote: Option<String> = row.get(8)?;
    Ok(Some(StoredChargingSpot {
        point_id: id,
        input: input
            .map(|value| decode::<Input>(&value)?.validated())
            .transpose()?,
        owned: row.get(1)?,
        status: ChargingStatus::parse(&row.get::<_, String>(2)?)?,
        local_revision: ChargingRevision::new(row.get(3)?)?,
        remote_id: remote
            .map(|value| {
                ChargingSpotId::parse(&value).map_err(|_| StorageError::ChargingInvalidInput)
            })
            .transpose()?,
        remote_revision: ChargingRevision::new(row.get(5)?)?,
        change_sequence: ChargingRevision::new(row.get(6)?)?,
        verified_at_ms: row.get(7)?,
        vote: vote
            .map(|value| decode(&format!("\"{value}\"")))
            .transpose()?,
        vote_revision: ChargingRevision::new(row.get(9)?)?,
        votes_up: row.get(10)?,
        votes_down: row.get(11)?,
    }))
}
fn change(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    id: MapPointId,
    expected: ChargingRevision,
    action: &ChargingAction,
) -> Result<ChargingWriteReceipt, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let current = detail(&transaction, id)?.ok_or(StorageError::NotFound)?;
    if current.local_revision != expected {
        return Err(StorageError::ChargingConflict);
    }
    if !(current.status.visible()
        || (current.owned
            && current.status == ChargingStatus::Hidden
            && match action {
                ChargingAction::Edit(_) | ChargingAction::Withdraw => true,
                ChargingAction::Create(_) | ChargingAction::Verify | ChargingAction::Vote(_) => {
                    false
                }
            }))
    {
        return Err(StorageError::ChargingConflict);
    }
    let revision = expected.next()?;
    match action {
        ChargingAction::Create(_) => return Err(StorageError::ChargingInvalidInput),
        ChargingAction::Edit(input) => {
            if !current.owned {
                return Err(StorageError::ChargingNotOwned);
            }
            index(&transaction, id, input, current.status.visible())?;
            transaction.execute(
                "UPDATE charging_spots SET input=?2,verified_at_ms=NULL WHERE point_id=?1",
                params![id.uuid().as_bytes(), input_json(input)?],
            )?;
        }
        ChargingAction::Withdraw => {
            if !current.owned {
                return Err(StorageError::ChargingNotOwned);
            }
            remove_index(&transaction, id)?;
            transaction.execute("UPDATE charging_spots SET status='withdrawn',input=NULL,verified_at_ms=NULL WHERE point_id=?1", [id.uuid().as_bytes()])?;
        }
        ChargingAction::Verify => {}
        ChargingAction::Vote(vote) => {
            transaction.execute(
                "UPDATE charging_spots SET vote=?2 WHERE point_id=?1",
                params![
                    id.uuid().as_bytes(),
                    vote.map(|value| match value {
                        ChargingVote::Up => "up",
                        ChargingVote::Down => "down",
                    })
                ],
            )?;
        }
    }
    transaction.execute(
        "UPDATE charging_spots SET local_revision=?2 WHERE point_id=?1",
        params![id.uuid().as_bytes(), revision.get()],
    )?;
    let receipt = queue(&transaction, id, revision, action)?;
    transaction.commit()?;
    Ok(receipt)
}

const QUERY: &str = "SELECT c.point_id FROM map_points_rtree r JOIN map_point_spatial_keys k ON k.rtree_id=r.id JOIN charging_spots c ON c.point_id=k.point_id
WHERE c.status IN ('queued','published') AND c.input IS NOT NULL
AND r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2
AND ((?5=0 AND r.max_lon_e7 >= ?3 AND r.min_lon_e7 <= ?4) OR (?5=1 AND (r.max_lon_e7 >= ?3 OR r.min_lon_e7 <= ?4)))
AND (?6 IS NULL OR c.point_id > ?6)
AND (?7 IS NULL OR json_extract(c.input,'$.cost')=?7)
AND ((?8 IS NULL AND ?9 IS NULL) OR EXISTS (SELECT 1 FROM json_each(c.input,'$.supplies') s
 WHERE (?9 IS NULL OR json_extract(s.value,'$.voltage_volts')=?9)
 AND (?8 IS NULL OR EXISTS (SELECT 1 FROM json_each(s.value,'$.connectors') p WHERE json_quote(p.value)=?8 OR p.value=?8))))
ORDER BY c.point_id LIMIT ?10";
fn page(
    connection: &Connection,
    spatial: &mut SpatialSchemaState,
    bounds: GeoBounds,
    filter: &ChargingFilter,
    cursor: Option<MapPointCursor>,
    limit: QueryLimit,
) -> Result<ChargingPage, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let after = cursor.map(|value| value.id().uuid().as_bytes().to_vec());
    let connector = filter.connector.as_ref().map(encode).transpose()?;
    let mut statement = connection.prepare(QUERY)?;
    let rows = statement.query_map(
        params![
            bounds.minimum_latitude,
            bounds.maximum_latitude,
            bounds.minimum_longitude,
            bounds.maximum_longitude,
            bounds.crosses_antimeridian(),
            after,
            filter.cost.map(|value| match value {
                ChargingCost::Free => "free",
                ChargingCost::Paid => "paid",
            }),
            connector,
            filter.voltage.map(cutout_ride_maps::SupplyVoltage::as_u16),
            limit.get() + 1
        ],
        |row| row.get::<_, Vec<u8>>(0),
    )?;
    let mut spots = rows
        .map(|row| detail(connection, uuid(&row?)?)?.ok_or(StorageError::NotFound))
        .collect::<Result<Vec<_>, _>>()?;
    let more = spots.len() > limit.get() as usize;
    if more {
        spots.pop();
    }
    let next_cursor = more
        .then(|| {
            spots
                .last()
                .map(|value| MapPointCursor::new(value.point_id))
        })
        .flatten();
    Ok(ChargingPage { spots, next_cursor })
}
fn nearby(
    connection: &Connection,
    spatial: &mut SpatialSchemaState,
    bounds: GeoBounds,
    origin: Coordinate,
    filter: &ChargingFilter,
    limit: QueryLimit,
) -> Result<Vec<StoredChargingSpot>, StorageError> {
    let sample = |coordinate| LocationSample::new(coordinate, 0, 0, None, LocationSource::Live);
    let mut best = Vec::new();
    let mut cursor = None;
    loop {
        let page = page(connection, spatial, bounds, filter, cursor, QueryLimit(500))?;
        for spot in page.spots {
            let coordinate = spot
                .input
                .as_ref()
                .ok_or(StorageError::ChargingInvalidInput)?
                .coordinate();
            best.push((
                distance_between_millimetres(sample(origin), sample(coordinate)),
                spot.point_id,
                spot,
            ));
            best.sort_unstable_by_key(|value| (value.0, value.1));
            best.truncate(limit.get() as usize);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(best.into_iter().map(|(_, _, spot)| spot).collect())
}
type PendingRow = (String, Vec<u8>, String, u64, Option<String>);
fn next(connection: &mut Connection) -> Result<Option<ChargingOperation>, StorageError> {
    let transaction = connection.transaction()?;
    let row: Option<PendingRow> = transaction.query_row("SELECT operation_id,point_id,action,local_revision,target FROM charging_outbox ORDER BY sequence LIMIT 1", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    let Some((id, point, action, revision, target)) = row else {
        return Ok(None);
    };
    let point_id = uuid(&point)?;
    let id = Uuid::parse_str(&id)
        .map(ChargingOperationId)
        .map_err(|_| StorageError::ChargingInvalidInput)?;
    let action = action_from_json(&action)?;
    let target = if let Some(target) = target {
        decode(&target)?
    } else {
        let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
        let target = match action {
            ChargingAction::Create(_) => ChargingTarget::Create,
            _ => ChargingTarget::Existing {
                id: current.remote_id.ok_or(StorageError::ChargingConflict)?,
                expected_revision: if action.is_vote() {
                    current.vote_revision
                } else {
                    current.remote_revision
                },
            },
        };
        transaction.execute(
            "UPDATE charging_outbox SET target=?2 WHERE operation_id=?1",
            params![id.0.to_string(), encode(&target)?],
        )?;
        target
    };
    transaction.commit()?;
    Ok(Some(ChargingOperation {
        id,
        point_id,
        local_revision: ChargingRevision::new(revision)?,
        target,
        action,
    }))
}
fn find_remote(
    connection: &Connection,
    id: ChargingSpotId,
) -> Result<Option<MapPointId>, StorageError> {
    let point: Option<Vec<u8>> = connection
        .query_row(
            "SELECT point_id FROM charging_spots WHERE remote_id=?1",
            [id.as_uuid().to_string()],
            |row| row.get(0),
        )
        .optional()?;
    point.map(|value| uuid(&value)).transpose()
}
fn cache(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    entry: &ChargingDirectoryEntry,
) -> Result<MapPointId, StorageError> {
    entry.validate()?;
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let existing = find_remote(&transaction, entry.id)?;
    let id = existing.unwrap_or(MapPointId::from_uuid(entry.id.as_uuid()));
    if let Some(current) = detail(&transaction, id)? {
        let pending: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM charging_outbox WHERE point_id=?1)",
            [id.uuid().as_bytes()],
            |row| row.get(0),
        )?;
        if current.remote_id != Some(entry.id)
            || pending
            || entry.change_sequence < current.change_sequence
            || entry.revision < current.remote_revision
            || entry.vote_revision < current.vote_revision
        {
            return Err(StorageError::ChargingConflict);
        }
        if entry.change_sequence == current.change_sequence
            && entry.vote_revision == current.vote_revision
        {
            transaction.commit()?;
            return Ok(id);
        }
    } else {
        let collision: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM map_points WHERE id=?1)",
            [id.uuid().as_bytes()],
            |row| row.get(0),
        )?;
        if collision {
            return Err(StorageError::ChargingConflict);
        }
        insert_directory(&transaction, id, entry)?;
    }
    apply_entry(&transaction, id, entry, true)?;
    if existing.is_some() {
        let current = detail(&transaction, id)?.ok_or(StorageError::NotFound)?;
        transaction.execute(
            "UPDATE charging_spots SET local_revision=?2 WHERE point_id=?1",
            params![id.uuid().as_bytes(), current.local_revision.next()?.get()],
        )?;
    }
    transaction.commit()?;
    Ok(id)
}
fn insert_directory(
    connection: &Connection,
    id: MapPointId,
    entry: &ChargingDirectoryEntry,
) -> Result<(), StorageError> {
    let coordinate = entry.input.as_ref().map_or(
        Coordinate::from_fixed_parts(0, 0).map_err(|_| StorageError::ChargingInvalidInput)?,
        NewChargingSpot::coordinate,
    );
    connection.execute("INSERT INTO map_points(id,name,latitude_e7,longitude_e7) VALUES (?1,'Charging spot',?2,?3)", params![id.uuid().as_bytes(), coordinate.latitude().as_i32(), coordinate.longitude().as_i32()])?;
    connection.execute(
        "INSERT INTO charging_spots(point_id,owned,status,local_revision) VALUES (?1,0,?2,1)",
        params![id.uuid().as_bytes(), entry.status.as_str()],
    )?;
    Ok(())
}
fn apply_entry(
    connection: &Connection,
    id: MapPointId,
    entry: &ChargingDirectoryEntry,
    body: bool,
) -> Result<(), StorageError> {
    if body {
        if let Some(input) = &entry.input {
            index(connection, id, input, entry.status.visible())?;
        } else {
            remove_index(connection, id)?;
        }
        connection.execute(
            "UPDATE charging_spots SET input=?2,status=?3,vote=?4 WHERE point_id=?1",
            params![
                id.uuid().as_bytes(),
                entry.input.as_ref().map(input_json).transpose()?,
                entry.status.as_str(),
                entry.vote.map(|value| match value {
                    ChargingVote::Up => "up",
                    ChargingVote::Down => "down",
                })
            ],
        )?;
    }
    connection.execute("UPDATE charging_spots SET remote_id=?2,remote_revision=?3,change_sequence=?4,verified_at_ms=CASE WHEN ?9 THEN ?5 ELSE verified_at_ms END,vote_revision=?6,votes_up=?7,votes_down=?8 WHERE point_id=?1", params![id.uuid().as_bytes(), entry.id.as_uuid().to_string(), entry.revision.get(), entry.change_sequence.get(), entry.verified_at_ms, entry.vote_revision.get(), entry.votes_up, entry.votes_down, body])?;
    Ok(())
}
fn acknowledge(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    id: ChargingOperationId,
    applied: ChargingRevision,
    entry: &ChargingDirectoryEntry,
) -> Result<MapPointId, StorageError> {
    entry.validate()?;
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let row: Option<(String, Vec<u8>, String, Option<String>)> = transaction.query_row("SELECT operation_id,point_id,action,target FROM charging_outbox ORDER BY sequence LIMIT 1", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
    if row.as_ref().is_none_or(|value| value.0 != id.0.to_string()) {
        let previous: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT point_id FROM charging_spots WHERE last_ack=?1 AND remote_id=?2",
                params![id.0.to_string(), entry.id.as_uuid().to_string()],
                |row| row.get(0),
            )
            .optional()?;
        return previous
            .map(|value| uuid(&value))
            .transpose()?
            .ok_or(StorageError::ChargingConflict);
    }
    let (_, point, action, target) = row.ok_or(StorageError::ChargingConflict)?;
    let point_id = uuid(&point)?;
    let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
    let action = action_from_json(&action)?;
    let target: ChargingTarget = decode(&target.ok_or(StorageError::ChargingConflict)?)?;
    let expected = match target {
        ChargingTarget::Create => {
            if !match action {
                ChargingAction::Create(_) => true,
                ChargingAction::Edit(_)
                | ChargingAction::Withdraw
                | ChargingAction::Verify
                | ChargingAction::Vote(_) => false,
            } || current.remote_id.is_some()
            {
                return Err(StorageError::ChargingConflict);
            }
            ChargingRevision(0)
        }
        ChargingTarget::Existing {
            id,
            expected_revision,
        } => {
            if id != entry.id || current.remote_id != Some(id) {
                return Err(StorageError::ChargingConflict);
            }
            expected_revision
        }
    };
    if applied != expected.next()?
        || entry.revision < current.remote_revision
        || entry.change_sequence < current.change_sequence
        || entry.vote_revision < current.vote_revision
        || if action.is_vote() {
            entry.vote_revision < applied
        } else {
            entry.revision < applied
        }
    {
        return Err(StorageError::ChargingConflict);
    }
    transaction.execute(
        "DELETE FROM charging_outbox WHERE operation_id=?1",
        [id.0.to_string()],
    )?;
    let pending: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM charging_outbox WHERE point_id=?1)",
        [point_id.uuid().as_bytes()],
        |row| row.get(0),
    )?;
    if pending
        && current.status == ChargingStatus::Queued
        && entry.status == ChargingStatus::Published
    {
        transaction.execute(
            "UPDATE charging_spots SET status='published' WHERE point_id=?1",
            [point_id.uuid().as_bytes()],
        )?;
    }
    apply_entry(&transaction, point_id, entry, !pending)?;
    transaction.execute(
        "UPDATE charging_spots SET last_ack=?2 WHERE point_id=?1",
        params![point_id.uuid().as_bytes(), id.0.to_string()],
    )?;
    transaction.commit()?;
    Ok(point_id)
}

#[cfg(test)]
mod tests;
