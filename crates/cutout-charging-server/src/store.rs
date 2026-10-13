use std::{
    fmt,
    net::IpAddr,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cutout_ride_maps::{ChargingCost, ChargingVote};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    Error,
    model::{
        ChangesPage, Counts, CreateRequest, MutationReceipt, OperationId, PublicSpot, SpotInput,
        SpotMutation, SpotPage, SpotState, SpotStatus, ValidatedChangesQuery, ValidatedListQuery,
        ValidatedSpotInput, VoteReceipt, VoteRequest, VoteState,
    },
};

/// A durable SQLite directory. SQL and migrations stay behind this boundary.
#[derive(Clone, Debug)]
pub struct Store {
    connection: Arc<Mutex<Connection>>,
    work: Arc<Semaphore>,
}

#[derive(Clone, Debug)]
pub(crate) struct Actor {
    pub id: String,
    pub role: Role,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    Contributor,
    Moderator,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::Contributor => "contributor",
            Self::Moderator => "moderator",
        }
    }
}

impl rusqlite::types::FromSql for Role {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "contributor" => Ok(Self::Contributor),
            "moderator" => Ok(Self::Moderator),
            _ => Err(rusqlite::types::FromSqlError::Other(Box::new(
                Error::Unavailable,
            ))),
        }
    }
}

/// Public registration always has a source budget and cannot grant moderation.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Registration {
    Public { source: IpAddr },
    Operator { role: Role },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenMode {
    Initialize,
    Existing,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OperationKind {
    Spot,
    Vote,
    Report,
}

impl OperationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Spot => "spot",
            Self::Vote => "vote",
            Self::Report => "report",
        }
    }
}

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

#[derive(Serialize)]
pub(crate) struct Identity {
    pub id: String,
    pub token: String,
}

struct ContributorTokenHash(String);

impl ContributorTokenHash {
    fn from_token(token: &str) -> Result<Self, Error> {
        if token.len() != 64 || !token.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(Error::Unauthorized);
        }
        Ok(Self(hash(token.as_bytes())))
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Identity")
            .field("id", &self.id)
            .field("token", &"[redacted]")
            .finish()
    }
}

impl Store {
    /// Opens/migrates a directory database and refuses unknown future schemas.
    ///
    /// # Errors
    /// Returns [`Error::Unavailable`] for open, migration or schema failures.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::open_database(path, OpenMode::Initialize)
    }

    /// Opens an existing initialized database without creating or migrating it.
    ///
    /// # Errors
    /// Returns [`Error::Unavailable`] for missing, uninitialized or incompatible databases.
    pub fn open_existing(path: &Path) -> Result<Self, Error> {
        Self::open_database(path, OpenMode::Existing)
    }

    fn open_database(path: &Path, mode: OpenMode) -> Result<Self, Error> {
        if path == Path::new(":memory:") || path.to_string_lossy().starts_with("file:") {
            return Err(Error::Invalid);
        }
        if mode == OpenMode::Initialize {
            match create_empty(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(Error::Unavailable),
            }
        }
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(db_error)?;
        let application: u32 = connection
            .pragma_query_value(None, "application_id", |row| row.get(0))
            .map_err(db_error)?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(db_error)?;
        match version {
            0 if mode == OpenMode::Initialize => {
                let tables: u64 = connection
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(db_error)?;
                if application != 0 || tables != 0 {
                    return Err(Error::Unavailable);
                }
                let transaction = connection.transaction().map_err(db_error)?;
                transaction
                    .execute_batch(include_str!("../migrations/001_directory.sql"))
                    .map_err(db_error)?;
                transaction.commit().map_err(db_error)?;
            }
            1 if application == 0x4353_5054 => {}
            _ => return Err(Error::Unavailable),
        }
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )
            .map_err(db_error)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            work: Arc::new(Semaphore::new(32)),
        })
    }

    // ponytail: one serialized SQLite connection; use a measured pool only if contention warrants it.
    pub(crate) async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let permit = self
            .work
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Unavailable)?;
        let connection = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut connection = connection.lock().map_err(|_| Error::Unavailable)?;
            operation(&mut connection)
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }

    pub(crate) async fn identity(&self, registration: Registration) -> Result<Identity, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            let now = now_ms()?;
            let role = registration_role(&transaction, registration, now)?;
            let identity = new_identity();
            insert_identity(&transaction, &identity, role, now)?;
            transaction.commit().map_err(db_error)?;
            Ok(identity)
        })
        .await
    }

    pub(crate) async fn authenticate(&self, token: String) -> Result<Actor, Error> {
        let token = ContributorTokenHash::from_token(&token)?;
        self.call(move |connection| authenticated_actor(connection, &token))
            .await
    }

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
            state(&transaction, &target)?.spot.ok_or(Error::NotFound)
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

    pub(crate) async fn vote(
        &self,
        actor: Actor,
        target: String,
        request: VoteRequest,
    ) -> Result<VoteReceipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            ensure_actor(&transaction, &actor)?;
            let fingerprint = fingerprint(&format!("vote:{target}"), &request)?;
            if let Some((target, revision)) =
                replay(&transaction, &actor, &request.operation_id, &fingerprint)?
            {
                return vote_receipt(
                    &transaction,
                    &target,
                    &actor,
                    request.operation_id,
                    revision,
                );
            }
            let revision =
                next_vote_revision(&transaction, &target, &actor, request.expected_revision)?;
            write_quota(&transaction, &actor, now_ms()?)?;
            save_vote(&transaction, &target, &actor, revision, request.vote)?;
            advance_spot_change(&transaction, &target)?;
            remember(
                &transaction,
                &actor,
                &request.operation_id,
                &fingerprint,
                &target,
                OperationKind::Vote,
                revision,
            )?;
            let receipt = vote_receipt(
                &transaction,
                &target,
                &actor,
                request.operation_id,
                revision,
            )?;
            transaction.commit().map_err(db_error)?;
            Ok(receipt)
        })
        .await
    }

    pub(crate) async fn current_vote(
        &self,
        actor: Actor,
        target: String,
    ) -> Result<VoteState, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            ensure_actor(&transaction, &actor)?;
            let state = vote_state(&transaction, &target, &actor)?;
            if state.spot_status != SpotStatus::Published {
                return Err(Error::NotFound);
            }
            Ok(state)
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

    pub(crate) async fn ready(&self) -> Result<(), Error> {
        self.call(|connection| {
            let version: u32 = connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .map_err(db_error)?;
            if version != 1 {
                return Err(Error::Unavailable);
            }
            connection
                .query_row(
                    "SELECT value FROM counters WHERE name='change_sequence'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .map_err(db_error)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn revoke(&self, target: String) -> Result<(), Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            revoke_contributor(&transaction, &target)?;
            let targets = contributor_voted_spots(&transaction, &target)?;
            clear_contributor_votes(&transaction, &target)?;
            for spot in targets {
                advance_spot_change(&transaction, &spot)?;
            }
            transaction.commit().map_err(db_error)
        })
        .await
    }

    pub(crate) async fn check(&self) -> Result<(), Error> {
        self.call(|connection| {
            let mut statement = connection.prepare("PRAGMA quick_check").map_err(db_error)?;
            for row in statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(db_error)?
            {
                if row.map_err(db_error)? != "ok" {
                    return Err(Error::Unavailable);
                }
            }
            Ok(())
        })
        .await
    }

    pub(crate) async fn backup(&self, output: std::path::PathBuf) -> Result<(), Error> {
        self.call(move |connection| {
            if output.try_exists().map_err(|_| Error::Unavailable)? {
                return Err(Error::Conflict);
            }
            let temporary = output.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
            let name = temporary.to_str().ok_or(Error::Invalid)?;
            create_empty(&temporary).map_err(|_| Error::Unavailable)?;
            let result = (|| {
                connection
                    .execute("VACUUM INTO ?1", [name])
                    .map_err(db_error)?;
                std::fs::File::open(&temporary)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| Error::Unavailable)?;
                std::fs::hard_link(&temporary, &output).map_err(|_| Error::Unavailable)?;
                let parent = output
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                std::fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|_| Error::Unavailable)?;
                Ok(())
            })();
            let _ = std::fs::remove_file(&temporary);
            result
        })
        .await
    }
}

pub(crate) fn begin_write(connection: &mut Connection) -> Result<Transaction<'_>, Error> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)
}

pub(crate) fn mutation_receipt(
    connection: &Connection,
    target: &str,
    operation_id: OperationId,
    applied_revision: u64,
) -> Result<MutationReceipt, Error> {
    Ok(MutationReceipt {
        operation_id,
        applied_revision,
        state: state(connection, target)?,
    })
}

fn registration_role(
    connection: &Connection,
    registration: Registration,
    now: u64,
) -> Result<Role, Error> {
    match registration {
        Registration::Public { source } => {
            registration_quota(connection, source, now)?;
            Ok(Role::Contributor)
        }
        Registration::Operator { role } => Ok(role),
    }
}

fn new_identity() -> Identity {
    Identity {
        id: Uuid::new_v4().to_string(),
        token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
    }
}

fn insert_identity(
    connection: &Connection,
    identity: &Identity,
    role: Role,
    now: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO contributors (id, token_hash, role, created_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
            params![
                identity.id,
                hash(identity.token.as_bytes()),
                role.as_str(),
                now
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

fn authenticated_actor(
    connection: &Connection,
    token: &ContributorTokenHash,
) -> Result<Actor, Error> {
    connection
        .query_row(
            "SELECT id, role FROM contributors
         WHERE token_hash = ?1 AND revoked_at_ms IS NULL",
            [&token.0],
            |row| {
                Ok(Actor {
                    id: row.get(0)?,
                    role: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::Unauthorized)
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

fn published_spot(
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

pub(crate) fn clear_spot_votes(connection: &Connection, target: &str) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE votes SET value = NULL,
            revision = CASE WHEN revision < 9223372036854775807 THEN revision + 1 ELSE revision END
         WHERE spot_id = ?1",
            [target],
        )
        .map_err(db_error)?;
    Ok(())
}

fn next_vote_revision(
    connection: &Connection,
    target: &str,
    actor: &Actor,
    expected_revision: u64,
) -> Result<u64, Error> {
    published_spot(connection, target, Error::NotFound)?;
    let current = vote_state(connection, target, actor)?;
    if current.revision != expected_revision {
        return Err(Error::Conflict);
    }
    next_revision(current.revision)
}

fn save_vote(
    connection: &Connection,
    target: &str,
    actor: &Actor,
    revision: u64,
    vote: Option<ChargingVote>,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO votes (spot_id, contributor_id, revision, value)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (spot_id, contributor_id) DO UPDATE
         SET revision = excluded.revision, value = excluded.value",
            params![target, actor.id, revision, vote.map(vote_value)],
        )
        .map_err(db_error)?;
    Ok(())
}

fn advance_spot_change(connection: &Connection, target: &str) -> Result<(), Error> {
    // Vote counts advance discovery/sync without advancing the spot's edit revision.
    connection
        .execute(
            "UPDATE spots SET change_sequence = ?2 WHERE id = ?1",
            params![target, next_change(connection)?],
        )
        .map_err(db_error)?;
    Ok(())
}

fn vote_receipt(
    connection: &Connection,
    target: &str,
    actor: &Actor,
    operation_id: OperationId,
    applied_revision: u64,
) -> Result<VoteReceipt, Error> {
    Ok(VoteReceipt {
        operation_id,
        applied_revision,
        state: vote_state(connection, target, actor)?,
    })
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

fn revoke_contributor(connection: &Connection, target: &str) -> Result<(), Error> {
    let updated = connection
        .execute(
            "UPDATE contributors SET revoked_at_ms = ?2 WHERE id = ?1",
            params![target, now_ms()?],
        )
        .map_err(db_error)?;
    if updated == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

fn contributor_voted_spots(connection: &Connection, target: &str) -> Result<Vec<String>, Error> {
    let mut statement = connection
        .prepare("SELECT spot_id FROM votes WHERE contributor_id = ?1 AND value IS NOT NULL")
        .map_err(db_error)?;
    statement
        .query_map([target], |row| row.get(0))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)
}

fn clear_contributor_votes(connection: &Connection, target: &str) -> Result<(), Error> {
    connection
        .execute(
            "UPDATE votes SET value = NULL,
            revision = CASE WHEN revision < 9223372036854775807 THEN revision + 1 ELSE revision END
         WHERE contributor_id = ?1",
            [target],
        )
        .map_err(db_error)?;
    Ok(())
}

fn create_empty(path: &Path) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map(|_| ())
}

pub(crate) fn db_error(_: rusqlite::Error) -> Error {
    Error::Unavailable
}

pub(crate) fn ensure_actor(connection: &Connection, actor: &Actor) -> Result<(), Error> {
    let role: Role = connection
        .query_row(
            "SELECT role FROM contributors WHERE id=?1 AND revoked_at_ms IS NULL",
            [&actor.id],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::Unauthorized)?;
    if role != actor.role {
        return Err(Error::Forbidden);
    }
    Ok(())
}

pub(crate) fn next_revision(value: u64) -> Result<u64, Error> {
    value
        .checked_add(1)
        .filter(|value| i64::try_from(*value).is_ok())
        .ok_or(Error::Conflict)
}

fn vote_value(value: ChargingVote) -> &'static str {
    match value {
        ChargingVote::Up => "up",
        ChargingVote::Down => "down",
    }
}

fn vote_state(connection: &Connection, target: &str, actor: &Actor) -> Result<VoteState, Error> {
    let (status, sequence) = connection
        .query_row(
            "SELECT status,change_sequence FROM spots WHERE id=?1",
            [target],
            |row| Ok((row.get::<_, SpotStatus>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(db_error)?
        .ok_or(Error::NotFound)?;
    let current = connection
        .query_row(
            "SELECT revision,value FROM votes WHERE spot_id=?1 AND contributor_id=?2",
            params![target, actor.id],
            StoredVote::read,
        )
        .optional()
        .map_err(db_error)?
        .unwrap_or_default();
    Ok(VoteState {
        revision: current.revision,
        vote: current.vote,
        votes: if status == SpotStatus::Published {
            counts(connection, target)?
        } else {
            Counts::default()
        },
        spot_status: status,
        change_sequence: sequence,
    })
}

#[derive(Default)]
struct StoredVote {
    revision: u64,
    vote: Option<ChargingVote>,
}

impl StoredVote {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let revision = row.get(0)?;
        let value: Option<String> = row.get(1)?;
        let vote = match value.as_deref() {
            Some("up") => Some(ChargingVote::Up),
            Some("down") => Some(ChargingVote::Down),
            None => None,
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(Error::Unavailable),
                ));
            }
        };
        Ok(Self { revision, vote })
    }
}

pub(crate) fn now_ms() -> Result<u64, Error> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Unavailable)?
            .as_millis(),
    )
    .map_err(|_| Error::Unavailable)
}

fn cost(value: ChargingCost) -> &'static str {
    match value {
        ChargingCost::Free => "free",
        ChargingCost::Paid => "paid",
    }
}

pub(crate) fn hash(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
pub(crate) fn json(value: &impl Serialize) -> Result<String, Error> {
    serde_json::to_string(value).map_err(|_| Error::Unavailable)
}
pub(crate) fn fingerprint(kind: &str, value: &impl Serialize) -> Result<String, Error> {
    Ok(hash(format!("{kind}:{}", json(value)?).as_bytes()))
}

pub(crate) fn replay(
    connection: &Connection,
    actor: &Actor,
    operation: &OperationId,
    fingerprint: &str,
) -> Result<Option<(String, u64)>, Error> {
    let stored = connection
        .query_row(
            "SELECT request_hash, target_id, applied_revision FROM operations
         WHERE contributor_id = ?1 AND operation_id = ?2",
            params![actor.id, operation],
            |row| {
                Ok(StoredOperation {
                    fingerprint: row.get(0)?,
                    target: row.get(1)?,
                    revision: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(db_error)?;
    match stored {
        Some(stored) if stored.fingerprint == fingerprint => {
            Ok(Some((stored.target, stored.revision)))
        }
        Some(_) => Err(Error::Conflict),
        None => Ok(None),
    }
}

struct StoredOperation {
    fingerprint: String,
    target: String,
    revision: u64,
}

pub(crate) fn remember(
    connection: &Connection,
    actor: &Actor,
    operation: &OperationId,
    fingerprint: &str,
    target: &str,
    kind: OperationKind,
    revision: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO operations (
                contributor_id, operation_id, request_hash, target_id, kind, applied_revision
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                actor.id,
                operation,
                fingerprint,
                target,
                kind.as_str(),
                revision
            ],
        )
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn next_change(connection: &Connection) -> Result<u64, Error> {
    connection
        .query_row(
            "UPDATE counters SET value = value + 1
         WHERE name = 'change_sequence' AND value < 9223372036854775807
         RETURNING value",
            [],
            |row| row.get(0),
        )
        .map_err(db_error)
}

pub(crate) fn quota(
    connection: &Connection,
    principal: &str,
    period: u64,
    limit: u32,
    now: u64,
) -> Result<(), Error> {
    let start = now / period * period;
    let count: Option<u32> = connection
        .query_row(
            "SELECT count FROM quotas WHERE principal=?1 AND period_ms=?2 AND window_start_ms=?3",
            params![principal, period, start],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_error)?;
    if count.unwrap_or_default() >= limit {
        return Err(Error::Throttled);
    }
    connection
        .execute(
            "INSERT INTO quotas (principal, period_ms, window_start_ms, count)
         VALUES (?1, ?2, ?3, 1)
         ON CONFLICT (principal, period_ms) DO UPDATE
         SET window_start_ms = excluded.window_start_ms,
             count = CASE WHEN quotas.window_start_ms = excluded.window_start_ms
                          THEN quotas.count + 1 ELSE 1 END",
            params![principal, period, start],
        )
        .map_err(db_error)?;
    Ok(())
}

fn registration_quota(connection: &Connection, source: IpAddr, now: u64) -> Result<(), Error> {
    let principal = format!(
        "registration-source:{}",
        hash(source.to_canonical().to_string().as_bytes())
    );
    quota(connection, &principal, 60_000, 20, now)?;
    quota(connection, &principal, 3_600_000, 120, now)?;
    quota(connection, "registration-global", 60_000, 200, now)?;
    quota(connection, "registration-global", 3_600_000, 1_200, now)
}

pub(crate) fn write_quota(connection: &Connection, actor: &Actor, now: u64) -> Result<(), Error> {
    quota(connection, &format!("write:{}", actor.id), 60_000, 30, now)?;
    quota(
        connection,
        &format!("write:{}", actor.id),
        3_600_000,
        600,
        now,
    )
}

pub(crate) fn counts(connection: &Connection, target: &str) -> Result<Counts, Error> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(value = 'up'), 0), COALESCE(SUM(value = 'down'), 0)
         FROM votes WHERE spot_id = ?1",
            [target],
            |row| {
                Ok(Counts {
                    up: row.get(0)?,
                    down: row.get(1)?,
                })
            },
        )
        .map_err(db_error)
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

#[cfg(test)]
mod tests {
    use super::{
        Error, Registration, Role, SpotStatus, Store, hash, quota, registration_quota, state,
    };

    #[test]
    fn published_content_is_checked_at_readback_and_hidden_content_stays_private() {
        let connection = rusqlite::Connection::open_in_memory().expect("database");
        connection
            .execute_batch(include_str!("../migrations/001_directory.sql"))
            .expect("schema");
        connection
            .execute(
                "INSERT INTO contributors (id, token_hash, role, created_at_ms)
             VALUES ('owner', 'token-hash', 'contributor', 1)",
                [],
            )
            .expect("owner");
        let body = r#"{"coordinate":{"latitude_e7":400000000,"longitude_e7":-1050000000},"access_note":"Ask staff","cost":"free","name":null,"venue":null,"hours":null,"paid_terms":null,"price":null,"supplies":[]}"#;
        connection
            .execute(
                "INSERT INTO spots (id, owner_id, revision, status, latitude_e7, longitude_e7,
                cost, body, created_at_ms, updated_at_ms, change_sequence)
             VALUES ('spot', 'owner', 1, 'published', 400000000, -1050000000,
                'free', ?1, 1, 1, 1)",
                [body],
            )
            .expect("spot");
        let published = state(&connection, "spot").expect("published state");
        assert_eq!(
            published.spot.expect("public content").input.access_note,
            "Ask staff"
        );

        connection
            .execute("UPDATE spots SET body = NULL WHERE id = 'spot'", [])
            .expect("missing body");
        assert_eq!(
            state(&connection, "spot").expect_err("published body required"),
            Error::Unavailable
        );
        connection
            .execute("UPDATE spots SET body = 'malformed' WHERE id = 'spot'", [])
            .expect("malformed body");
        assert_eq!(
            state(&connection, "spot").expect_err("published body must decode"),
            Error::Unavailable
        );

        connection
            .execute("UPDATE spots SET status = 'hidden' WHERE id = 'spot'", [])
            .expect("hidden body");
        let hidden = state(&connection, "spot").expect("hidden state");
        assert_eq!(hidden.status, SpotStatus::Hidden);
        assert!(hidden.spot.is_none());
    }

    #[tokio::test]
    async fn registration_has_a_global_backstop_without_storing_raw_addresses() {
        let directory = tempfile::TempDir::new().expect("directory");
        let store = Store::open(&directory.path().join("quota.sqlite")).expect("store");
        store.call(|connection| {
            for suffix in 1..=200 {
                let transaction = connection.transaction().expect("transaction");
                registration_quota(&transaction, std::net::Ipv4Addr::new(192, 0, 2, suffix).into(), 1_000)?;
                transaction.commit().expect("commit");
            }
            let transaction = connection.transaction().expect("transaction");
            assert_eq!(registration_quota(&transaction, std::net::Ipv4Addr::new(192, 0, 2, 201).into(), 1_000), Err(Error::Throttled));
            drop(transaction);
            let raw: u64 = connection.query_row("SELECT COUNT(*) FROM quotas WHERE principal LIKE '%192.0.2.%'", [], |row| row.get(0)).expect("private principals");
            assert_eq!(raw, 0);
            let global: u64 = connection.query_row("SELECT count FROM quotas WHERE principal='registration-global' AND period_ms=60000", [], |row| row.get(0)).expect("global budget");
            assert_eq!(global, 200, "a rejected registration must not consume another budget");
            Ok(())
        }).await.expect("backstop");
    }

    #[tokio::test]
    async fn quota_budgets_persist_and_reset_only_for_a_new_window() {
        let directory = tempfile::TempDir::new().expect("directory");
        let path = directory.path().join("quota.sqlite");
        let store = Store::open(&path).expect("store");
        store
            .call(|connection| {
                quota(connection, "contributor", 60_000, 2, 1_000)?;
                quota(connection, "contributor", 60_000, 2, 2_000)
            })
            .await
            .expect("two allowed writes");
        drop(store);
        let reopened = Store::open(&path).expect("reopen");
        assert_eq!(
            reopened
                .call(|connection| quota(connection, "contributor", 60_000, 2, 3_000))
                .await,
            Err(Error::Throttled)
        );
        reopened
            .call(|connection| quota(connection, "contributor", 60_000, 2, 60_000))
            .await
            .expect("new window");
    }

    #[tokio::test]
    async fn authentication_rejects_unknown_persisted_roles() {
        let directory = tempfile::TempDir::new().expect("directory");
        let path = directory.path().join("roles.sqlite");
        let store = Store::open(&path).expect("store");
        let identity = store
            .identity(Registration::Operator {
                role: Role::Contributor,
            })
            .await
            .expect("identity");
        let connection = rusqlite::Connection::open(&path).expect("inspect database");
        connection
            .execute_batch("PRAGMA ignore_check_constraints=ON")
            .expect("simulate damaged role");
        connection
            .execute(
                "UPDATE contributors SET role='unknown' WHERE id=?1",
                [&identity.id],
            )
            .expect("damaged role");
        assert_eq!(
            store
                .authenticate(identity.token)
                .await
                .expect_err("unknown role rejected"),
            Error::Unavailable
        );
    }

    #[tokio::test]
    async fn contributor_tokens_are_hashed_in_storage_and_redacted_in_debug() {
        let directory = tempfile::TempDir::new().expect("directory");
        let path = directory.path().join("tokens.sqlite");
        let store = Store::open(&path).expect("store");
        let identity = store
            .identity(Registration::Operator {
                role: Role::Contributor,
            })
            .await
            .expect("identity");
        assert!(!format!("{identity:?}").contains(&identity.token));
        let connection = rusqlite::Connection::open(&path).expect("inspect durable record");
        let stored: String = connection
            .query_row(
                "SELECT token_hash FROM contributors WHERE id=?1",
                [&identity.id],
                |row| row.get(0),
            )
            .expect("stored hash");
        assert_eq!(stored, hash(identity.token.as_bytes()));
        assert_ne!(stored, identity.token);
    }
}
