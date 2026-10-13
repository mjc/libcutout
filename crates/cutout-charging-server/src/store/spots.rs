use super::votes::{clear_spot_votes, counts};
use super::{
    Actor, OperationKind, Store, begin_write, db_error, ensure_actor, fingerprint, json,
    mutation_receipt, next_change, next_revision, now_ms, quota, remember, replay, write_quota,
};
use crate::{
    Error,
    model::{
        ChangesPage, CreateRequest, MutationReceipt, OperationId, PublicSpot, SpotInput,
        SpotMutation, SpotPage, SpotState, SpotStatus, ValidatedChangesQuery, ValidatedListQuery,
        ValidatedSpotInput,
    },
};
use cutout_ride_maps::ChargingCost;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

struct PreparedMutation {
    operation_id: OperationId,
    expected_revision: u64,
    fingerprint: String,
    change: OwnerChange,
}

enum OwnerChange {
    Edit(ValidatedSpotInput),
    Withdraw,
}

impl PreparedMutation {
    fn new(target: &str, mutation: SpotMutation) -> Result<Self, Error> {
        match mutation {
            SpotMutation::Edit(request) => Ok(Self {
                fingerprint: fingerprint(&format!("edit:{target}"), &request)?,
                operation_id: request.operation_id,
                expected_revision: request.expected_revision,
                change: OwnerChange::Edit(request.spot),
            }),
            SpotMutation::Withdraw(request) => Ok(Self {
                fingerprint: fingerprint(&format!("withdraw:{target}"), &request)?,
                operation_id: request.operation_id,
                expected_revision: request.expected_revision,
                change: OwnerChange::Withdraw,
            }),
        }
    }
}

/// A spot whose owner, revision and editable status were checked in this transaction.
struct EditableSpot {
    revision: u64,
}

impl EditableSpot {
    fn load(
        connection: &Connection,
        actor: &Actor,
        target: &str,
        expected_revision: u64,
    ) -> Result<Self, Error> {
        let stored = connection
            .query_row(
                "SELECT owner_id, revision, status FROM spots WHERE id = ?1",
                [target],
                |row| {
                    Ok(SpotOwner {
                        owner_id: row.get(0)?,
                        revision: row.get(1)?,
                        status: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(db_error)?
            .ok_or(Error::NotFound)?;
        if stored.owner_id != actor.id {
            return Err(Error::Forbidden);
        }
        if stored.revision != expected_revision || !stored.status.allows_edit() {
            return Err(Error::Conflict);
        }
        Ok(Self {
            revision: stored.revision,
        })
    }
}

struct SpotOwner {
    owner_id: String,
    revision: u64,
    status: SpotStatus,
}

/// All metadata advanced together by one successful owner edit or withdrawal.
struct SpotUpdate {
    revision: u64,
    at_ms: u64,
    sequence: u64,
}

impl SpotUpdate {
    fn prepare(connection: &Connection, actor: &Actor, spot: &EditableSpot) -> Result<Self, Error> {
        let revision = next_revision(spot.revision)?;
        let at_ms = now_ms()?;
        write_quota(connection, actor, at_ms)?;
        let sequence = next_change(connection)?;
        Ok(Self {
            revision,
            at_ms,
            sequence,
        })
    }
}

impl Store {
    pub(crate) async fn create(
        &self,
        actor: Actor,
        request: CreateRequest<ValidatedSpotInput>,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let fingerprint = fingerprint("create", &request)?;
            if let Some((target, revision)) =
                replay(&transaction, &actor, &request.operation_id, &fingerprint)?
            {
                return mutation_receipt(&transaction, &target, request.operation_id, revision);
            }
            let now = now_ms()?;
            charge_creation_budget(&transaction, &actor, now)?;
            let target = Uuid::new_v4().to_string();
            let sequence = next_change(&transaction)?;
            insert_spot(&transaction, &actor, &target, &request.spot, now, sequence)?;
            remember(
                &transaction,
                &actor,
                &request.operation_id,
                &fingerprint,
                &target,
                OperationKind::Spot,
                1,
            )?;
            let receipt = mutation_receipt(&transaction, &target, request.operation_id, 1)?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }

    pub(crate) async fn detail(&self, target: String) -> Result<PublicSpot, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            published_spot(&transaction, &target, Error::NotFound)
        })
        .await
    }

    pub(crate) async fn list(&self, query: ValidatedListQuery) -> Result<SpotPage, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            let targets = discovery_targets(&transaction, &query)?;
            let has_more = targets.len() > query.limit().count();
            let items = targets
                .into_iter()
                .take(query.limit().count())
                .map(|target| published_spot(&transaction, &target, Error::Unavailable))
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if has_more {
                items.last().map(|spot| spot.id.clone())
            } else {
                None
            };
            Ok(SpotPage { items, next_cursor })
        })
        .await
    }

    pub(crate) async fn mutate(
        &self,
        actor: Actor,
        target: String,
        mutation: SpotMutation,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let mutation = PreparedMutation::new(&target, mutation)?;
            if let Some((target, revision)) = replay(
                &transaction,
                &actor,
                &mutation.operation_id,
                &mutation.fingerprint,
            )? {
                return mutation_receipt(&transaction, &target, mutation.operation_id, revision);
            }
            let editable =
                EditableSpot::load(&transaction, &actor, &target, mutation.expected_revision)?;
            let update = SpotUpdate::prepare(&transaction, &actor, &editable)?;
            apply_owner_change(&transaction, &target, mutation.change, &update)?;
            remember(
                &transaction,
                &actor,
                &mutation.operation_id,
                &mutation.fingerprint,
                &target,
                OperationKind::Spot,
                update.revision,
            )?;
            let receipt = mutation_receipt(
                &transaction,
                &target,
                mutation.operation_id,
                update.revision,
            )?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }

    pub(crate) async fn changes(&self, query: ValidatedChangesQuery) -> Result<ChangesPage, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            ensure_change_cursor(&transaction, query.after())?;
            let targets = changed_targets(&transaction, &query)?;
            let has_more = targets.len() > query.limit().count();
            let items = targets
                .into_iter()
                .take(query.limit().count())
                .map(|target| state(&transaction, &target))
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = items
                .last()
                .map_or(query.after(), |state| state.change_sequence);
            Ok(ChangesPage {
                items,
                next_cursor,
                has_more,
            })
        })
        .await
    }
}

fn charge_creation_budget(connection: &Connection, actor: &Actor, now: u64) -> Result<(), Error> {
    write_quota(connection, actor, now)?;
    quota(
        connection,
        &format!("new-spots:{}", actor.id),
        86_400_000,
        20,
        now,
    )
}

fn insert_spot(
    connection: &Connection,
    actor: &Actor,
    target: &str,
    spot: &ValidatedSpotInput,
    now: u64,
    sequence: u64,
) -> Result<(), Error> {
    let input = spot.as_input();
    connection
        .execute(
            "INSERT INTO spots (
            id, owner_id, revision, status, latitude_e7, longitude_e7,
            cost, body, created_at_ms, updated_at_ms, change_sequence
         ) VALUES (?1, ?2, 1, 'published', ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
            params![
                target,
                actor.id,
                input.coordinate.latitude_e7,
                input.coordinate.longitude_e7,
                cost(input.cost),
                json(spot)?,
                now,
                sequence
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn published_spot(
    connection: &Connection,
    target: &str,
    hidden_error: Error,
) -> Result<PublicSpot, Error> {
    state(connection, target)?.spot.ok_or(hidden_error)
}

fn discovery_targets(
    connection: &Connection,
    query: &ValidatedListQuery,
) -> Result<Vec<String>, Error> {
    let bounds = query.bounds();
    let mut statement = connection
        .prepare(
            "SELECT id FROM spots
         WHERE status = 'published' AND id > ?1
           AND (?2 IS NULL OR cost = ?2)
           AND latitude_e7 BETWEEN ?3 AND ?4
           AND ((?5 <= ?6 AND longitude_e7 BETWEEN ?5 AND ?6)
             OR (?5 > ?6 AND (longitude_e7 >= ?5 OR longitude_e7 <= ?6)))
         ORDER BY id LIMIT ?7",
        )
        .map_err(db_error)?;
    statement
        .query_map(
            params![
                query.after(),
                query.cost().map(cost),
                bounds.south_e7(),
                bounds.north_e7(),
                bounds.west_e7(),
                bounds.east_e7(),
                query.limit().lookahead()
            ],
            |row| row.get(0),
        )
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)
}

fn apply_owner_change(
    connection: &Connection,
    target: &str,
    change: OwnerChange,
    update: &SpotUpdate,
) -> Result<(), Error> {
    match change {
        OwnerChange::Edit(input) => edit_spot(connection, target, &input, update),
        OwnerChange::Withdraw => withdraw_spot(connection, target, update),
    }
}

fn edit_spot(
    connection: &Connection,
    target: &str,
    input: &ValidatedSpotInput,
    update: &SpotUpdate,
) -> Result<(), Error> {
    let input = input.as_input();
    connection
        .execute(
            "UPDATE spots SET revision = ?2, latitude_e7 = ?3, longitude_e7 = ?4,
            cost = ?5, body = ?6, updated_at_ms = ?7, change_sequence = ?8,
            verified_at_ms = NULL
         WHERE id = ?1",
            params![
                target,
                update.revision,
                input.coordinate.latitude_e7,
                input.coordinate.longitude_e7,
                cost(input.cost),
                json(input)?,
                update.at_ms,
                update.sequence
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

fn withdraw_spot(connection: &Connection, target: &str, update: &SpotUpdate) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE spots SET revision = ?2, status = 'withdrawn', body = NULL,
            latitude_e7 = 0, longitude_e7 = 0, updated_at_ms = ?3,
            verified_at_ms = NULL, change_sequence = ?4
         WHERE id = ?1",
            params![target, update.revision, update.at_ms, update.sequence],
        )
        .map_err(db_error)?;
    clear_spot_votes(connection, target)
}

fn ensure_change_cursor(connection: &Connection, after: u64) -> Result<(), Error> {
    let current: u64 = connection
        .query_row(
            "SELECT value FROM counters WHERE name = 'change_sequence'",
            [],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if after > current {
        return Err(Error::Conflict);
    }
    Ok(())
}

fn changed_targets(
    connection: &Connection,
    query: &ValidatedChangesQuery,
) -> Result<Vec<String>, Error> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM spots WHERE change_sequence > ?1 ORDER BY change_sequence LIMIT ?2",
        )
        .map_err(db_error)?;
    statement
        .query_map(params![query.after(), query.limit().lookahead()], |row| {
            row.get(0)
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)
}

fn cost(value: ChargingCost) -> &'static str {
    match value {
        ChargingCost::Free => "free",
        ChargingCost::Paid => "paid",
    }
}

pub(crate) fn state(connection: &Connection, target: &str) -> Result<SpotState, Error> {
    let stored = connection
        .query_row(
            "SELECT revision, status, body, created_at_ms, updated_at_ms,
                verified_at_ms, change_sequence, duplicate_of
         FROM spots WHERE id = ?1",
            [target],
            StoredSpotRow::read,
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::NotFound)?;
    let stored = StoredSpot::try_from(stored)?;
    stored.into_state(connection, target)
}

struct StoredSpotRow {
    revision: u64,
    status: SpotStatus,
    body: Option<String>,
    created_at_ms: u64,
    updated_at_ms: u64,
    verified_at_ms: Option<u64>,
    change_sequence: u64,
    duplicate_of: Option<String>,
}

impl StoredSpotRow {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            revision: row.get(0)?,
            status: row.get(1)?,
            body: row.get(2)?,
            created_at_ms: row.get(3)?,
            updated_at_ms: row.get(4)?,
            verified_at_ms: row.get(5)?,
            change_sequence: row.get(6)?,
            duplicate_of: row.get(7)?,
        })
    }
}

enum StoredSpotContent {
    Published(SpotInput),
    Hidden,
    Withdrawn,
    Removed,
}

impl StoredSpotContent {
    fn status(&self) -> SpotStatus {
        match self {
            Self::Published(_) => SpotStatus::Published,
            Self::Hidden => SpotStatus::Hidden,
            Self::Withdrawn => SpotStatus::Withdrawn,
            Self::Removed => SpotStatus::Removed,
        }
    }
}

struct StoredSpot {
    revision: u64,
    content: StoredSpotContent,
    created_at_ms: u64,
    updated_at_ms: u64,
    verified_at_ms: Option<u64>,
    change_sequence: u64,
    duplicate_of: Option<String>,
}

impl TryFrom<StoredSpotRow> for StoredSpot {
    type Error = Error;

    fn try_from(row: StoredSpotRow) -> Result<Self, Error> {
        let content = match row.status {
            SpotStatus::Published => {
                let body = row.body.as_deref().ok_or(Error::Unavailable)?;
                StoredSpotContent::Published(
                    serde_json::from_str(body).map_err(|_| Error::Unavailable)?,
                )
            }
            SpotStatus::Hidden => StoredSpotContent::Hidden,
            SpotStatus::Withdrawn => StoredSpotContent::Withdrawn,
            SpotStatus::Removed => StoredSpotContent::Removed,
        };
        Ok(Self {
            revision: row.revision,
            content,
            created_at_ms: row.created_at_ms,
            updated_at_ms: row.updated_at_ms,
            verified_at_ms: row.verified_at_ms,
            change_sequence: row.change_sequence,
            duplicate_of: row.duplicate_of,
        })
    }
}

impl StoredSpot {
    fn into_state(self, connection: &Connection, target: &str) -> Result<SpotState, Error> {
        let status = self.content.status();
        let spot = match self.content {
            StoredSpotContent::Published(input) => Some(PublicSpot {
                id: target.to_owned(),
                revision: self.revision,
                created_at_ms: self.created_at_ms,
                updated_at_ms: self.updated_at_ms,
                last_verified_at_ms: self.verified_at_ms,
                change_sequence: self.change_sequence,
                input,
                votes: counts(connection, target)?,
            }),
            StoredSpotContent::Hidden
            | StoredSpotContent::Withdrawn
            | StoredSpotContent::Removed => None,
        };
        Ok(SpotState {
            id: target.to_owned(),
            revision: self.revision,
            status,
            change_sequence: self.change_sequence,
            spot,
            duplicate_of: self.duplicate_of,
        })
    }
}
