use std::{
    fmt,
    net::IpAddr,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cutout_ride_maps::{ChargingCost, ChargingVote};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    Error,
    model::{
        ChangesPage, ChangesQuery, Counts, CreateRequest, ListQuery, MutationReceipt, PublicSpot,
        SpotInput, SpotMutation, SpotPage, SpotState, SpotStatus, ValidatedSpotInput, VoteReceipt,
        VoteRequest, VoteState,
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

#[derive(Serialize)]
pub(crate) struct Identity {
    pub id: String,
    pub token: String,
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
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            let now = now_ms()?;
            let role = match registration {
                Registration::Public { source } => {
                    registration_quota(&transaction, source, now)?;
                    Role::Contributor
                }
                Registration::Operator { role } => role,
            };
            let identity = Identity { id: Uuid::new_v4().to_string(), token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()) };
            transaction.execute("INSERT INTO contributors (id,token_hash,role,created_at_ms) VALUES (?1,?2,?3,?4)", params![identity.id, hash(identity.token.as_bytes()), role.as_str(),now]).map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            Ok(identity)
        }).await
    }

    pub(crate) async fn authenticate(&self, token: String) -> Result<Actor, Error> {
        if token.len() != 64 || !token.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(Error::Unauthorized);
        }
        self.call(move |connection| {
            connection.query_row("SELECT id,role FROM contributors WHERE token_hash=?1 AND revoked_at_ms IS NULL", [hash(token.as_bytes())], |row| Ok(Actor { id: row.get(0)?, role: row.get(1)? }))
                .optional().map_err(db_error)?.ok_or(Error::Unauthorized)
        }).await
    }

    pub(crate) async fn create(
        &self,
        actor: Actor,
        request: CreateRequest<ValidatedSpotInput>,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            ensure_actor(&transaction,&actor)?;
            let fingerprint = fingerprint("create", &request)?;
            if let Some((target,revision)) = replay(&transaction,&actor,&request.operation_id,&fingerprint)? {
                return Ok(MutationReceipt { operation_id: request.operation_id, applied_revision: revision, state: state(&transaction,&target)? });
            }
            let now = now_ms()?;
            write_quota(&transaction,&actor,now)?;
            quota(&transaction,&format!("new-spots:{}",actor.id),86_400_000,20,now)?;
            let target = Uuid::new_v4().to_string();
            let sequence = next_change(&transaction)?;
            let spot = request.spot.as_input();
            transaction.execute("INSERT INTO spots (id,owner_id,revision,status,latitude_e7,longitude_e7,cost,body,created_at_ms,updated_at_ms,change_sequence) VALUES (?1,?2,1,'published',?3,?4,?5,?6,?7,?7,?8)", params![target,actor.id,spot.coordinate.latitude_e7,spot.coordinate.longitude_e7,cost(spot.cost),json(&request.spot)?,now,sequence]).map_err(db_error)?;
            remember(&transaction,&actor,&request.operation_id,&fingerprint,&target,"spot",1)?;
            let state = state(&transaction,&target)?;
            transaction.commit().map_err(db_error)?;
            Ok(MutationReceipt { operation_id:request.operation_id, applied_revision:1, state })
        }).await
    }

    pub(crate) async fn detail(&self, target: String) -> Result<PublicSpot, Error> {
        self.call(move |connection| {
            let transaction = connection.transaction().map_err(db_error)?;
            state(&transaction, &target)?.spot.ok_or(Error::NotFound)
        })
        .await
    }

    pub(crate) async fn list(&self, query: ListQuery) -> Result<SpotPage, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction().map_err(db_error)?;
            let connection=&transaction;
            let limit = query.limit.unwrap_or(50);
            let after = query.after.as_deref().map(crate::model::id).transpose()?.unwrap_or_default();
            let mut statement = connection.prepare("SELECT id FROM spots WHERE status='published' AND id>?1 AND (?2 IS NULL OR cost=?2) AND latitude_e7 BETWEEN ?3 AND ?4 AND ((?5<=?6 AND longitude_e7 BETWEEN ?5 AND ?6) OR (?5>?6 AND (longitude_e7>=?5 OR longitude_e7<=?6))) ORDER BY id LIMIT ?7").map_err(db_error)?;
            let targets = statement.query_map(params![after,query.cost.map(cost),query.south_e7.unwrap_or(-900_000_000),query.north_e7.unwrap_or(900_000_000),query.west_e7.unwrap_or(-1_800_000_000),query.east_e7.unwrap_or(1_800_000_000),limit+1],|row| row.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let has_more = targets.len() > limit as usize;
            let items = targets.into_iter().take(limit as usize).map(|target| state(connection,&target)?.spot.ok_or(Error::Unavailable)).collect::<Result<Vec<_>,Error>>()?;
            let next_cursor = if has_more { items.last().map(|spot|spot.id.clone()) } else { None };
            Ok(SpotPage { items,next_cursor })
        }).await
    }

    pub(crate) async fn mutate(
        &self,
        actor: Actor,
        target: String,
        mutation: SpotMutation,
    ) -> Result<MutationReceipt, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            ensure_actor(&transaction,&actor)?;
            let (operation,expected,input,fingerprint)=match mutation {
                SpotMutation::Edit(request) => {
                    let fingerprint=fingerprint(&format!("edit:{target}"),&request)?;
                    (request.operation_id,request.expected_revision,Some(request.spot),fingerprint)
                },
                SpotMutation::Withdraw(request) => {
                    let fingerprint=fingerprint(&format!("withdraw:{target}"),&request)?;
                    (request.operation_id,request.expected_revision,None,fingerprint)
                },
            };
            if let Some((target,revision))=replay(&transaction,&actor,&operation,&fingerprint)? {
                return Ok(MutationReceipt {operation_id:operation,applied_revision:revision,state:state(&transaction,&target)?});
            }
            let (owner,current,status)=transaction.query_row("SELECT owner_id,revision,status FROM spots WHERE id=?1",[&target],|row|Ok((row.get::<_,String>(0)?,row.get::<_,u64>(1)?,row.get::<_,SpotStatus>(2)?))).optional().map_err(db_error)?.ok_or(Error::NotFound)?;
            if owner!=actor.id {return Err(Error::Forbidden);}
            if current!=expected || !status.allows_edit() {return Err(Error::Conflict);}
            let revision=next_revision(current)?;
            let now=now_ms()?;
            write_quota(&transaction,&actor,now)?;
            let sequence=next_change(&transaction)?;
            if let Some(input)=input {
                let input = input.as_input();
                transaction.execute("UPDATE spots SET revision=?2,latitude_e7=?3,longitude_e7=?4,cost=?5,body=?6,updated_at_ms=?7,change_sequence=?8,verified_at_ms=NULL WHERE id=?1",params![target,revision,input.coordinate.latitude_e7,input.coordinate.longitude_e7,cost(input.cost),json(&input)?,now,sequence]).map_err(db_error)?;
            } else {
                transaction.execute("UPDATE spots SET revision=?2,status='withdrawn',body=NULL,latitude_e7=0,longitude_e7=0,updated_at_ms=?3,verified_at_ms=NULL,change_sequence=?4 WHERE id=?1",params![target,revision,now,sequence]).map_err(db_error)?;
                transaction.execute("UPDATE votes SET value=NULL,revision=CASE WHEN revision<9223372036854775807 THEN revision+1 ELSE revision END WHERE spot_id=?1",[&target]).map_err(db_error)?;
            }
            remember(&transaction,&actor,&operation,&fingerprint,&target,"spot",revision)?;
            let state=state(&transaction,&target)?;
            transaction.commit().map_err(db_error)?;
            Ok(MutationReceipt {operation_id:operation,applied_revision:revision,state})
        }).await
    }

    pub(crate) async fn vote(
        &self,
        actor: Actor,
        target: String,
        request: VoteRequest,
    ) -> Result<VoteReceipt, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            ensure_actor(&transaction,&actor)?;
            let fingerprint=fingerprint(&format!("vote:{target}"),&request)?;
            if let Some((target,revision))=replay(&transaction,&actor,&request.operation_id,&fingerprint)? {
                return Ok(VoteReceipt {operation_id:request.operation_id,applied_revision:revision,state:vote_state(&transaction,&target,&actor)?});
            }
            let spot=state(&transaction,&target)?;
            if spot.status!=SpotStatus::Published {return Err(Error::NotFound);}
            let current=vote_state(&transaction,&target,&actor)?;
            if current.revision!=request.expected_revision {return Err(Error::Conflict);}
            let revision=next_revision(current.revision)?;
            write_quota(&transaction,&actor,now_ms()?)?;
            transaction.execute("INSERT INTO votes VALUES (?1,?2,?3,?4) ON CONFLICT (spot_id,contributor_id) DO UPDATE SET revision=excluded.revision,value=excluded.value",params![target,actor.id,revision,request.vote.map(vote_value)]).map_err(db_error)?;
            // Counts affect discovery/sync but do not advance the spot's edit revision.
            transaction.execute("UPDATE spots SET change_sequence=?2 WHERE id=?1",params![target,next_change(&transaction)?]).map_err(db_error)?;
            remember(&transaction,&actor,&request.operation_id,&fingerprint,&target,"vote",revision)?;
            let state=vote_state(&transaction,&target,&actor)?;
            transaction.commit().map_err(db_error)?;
            Ok(VoteReceipt {operation_id:request.operation_id,applied_revision:revision,state})
        }).await
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

    pub(crate) async fn changes(&self, query: ChangesQuery) -> Result<ChangesPage, Error> {
        self.call(move |connection| {
            let transaction=connection.transaction().map_err(db_error)?;
            let limit=query.limit.unwrap_or(50);
            let after=query.after.unwrap_or_default();
            let current:u64=transaction.query_row("SELECT value FROM counters WHERE name='change_sequence'",[],|row|row.get(0)).map_err(db_error)?;
            if after>current {return Err(Error::Conflict);}
            let mut statement=transaction.prepare("SELECT id FROM spots WHERE change_sequence>?1 ORDER BY change_sequence LIMIT ?2").map_err(db_error)?;
            let targets=statement.query_map(params![after,limit+1],|row|row.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            let has_more=targets.len()>limit as usize;
            let items=targets.into_iter().take(limit as usize).map(|target|state(&transaction,&target)).collect::<Result<Vec<_>,_>>()?;
            let next_cursor=items.last().map_or(after,|state|state.change_sequence);
            Ok(ChangesPage {items,next_cursor,has_more})
        }).await
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
            let transaction=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db_error)?;
            if transaction.execute("UPDATE contributors SET revoked_at_ms=?2 WHERE id=?1",params![target,now_ms()?]).map_err(db_error)?==0 {return Err(Error::NotFound);}
            let targets={
                let mut statement=transaction.prepare("SELECT spot_id FROM votes WHERE contributor_id=?1 AND value IS NOT NULL").map_err(db_error)?;
                statement.query_map([&target],|row|row.get::<_,String>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?
            };
            transaction.execute("UPDATE votes SET value=NULL,revision=CASE WHEN revision<9223372036854775807 THEN revision+1 ELSE revision END WHERE contributor_id=?1",[&target]).map_err(db_error)?;
            for spot in targets {transaction.execute("UPDATE spots SET change_sequence=?2 WHERE id=?1",params![spot,next_change(&transaction)?]).map_err(db_error)?;}
            transaction.commit().map_err(db_error)
        }).await
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
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(db_error)?
        .unwrap_or((0, None));
    let vote = match current.1.as_deref() {
        Some("up") => Some(ChargingVote::Up),
        Some("down") => Some(ChargingVote::Down),
        None => None,
        _ => return Err(Error::Unavailable),
    };
    Ok(VoteState {
        revision: current.0,
        vote,
        votes: if status == SpotStatus::Published {
            counts(connection, target)?
        } else {
            Counts::default()
        },
        spot_status: status,
        change_sequence: sequence,
    })
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
    operation: &str,
    fingerprint: &str,
) -> Result<Option<(String, u64)>, Error> {
    let stored = connection.query_row("SELECT request_hash,target_id,applied_revision FROM operations WHERE contributor_id=?1 AND operation_id=?2",params![actor.id,operation],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,u64>(2)?))).optional().map_err(db_error)?;
    match stored {
        Some((stored, target, revision)) if stored == fingerprint => Ok(Some((target, revision))),
        Some(_) => Err(Error::Conflict),
        None => Ok(None),
    }
}

pub(crate) fn remember(
    connection: &Connection,
    actor: &Actor,
    operation: &str,
    fingerprint: &str,
    target: &str,
    kind: &str,
    revision: u64,
) -> Result<(), Error> {
    connection
        .execute(
            "INSERT INTO operations VALUES (?1,?2,?3,?4,?5,?6)",
            params![actor.id, operation, fingerprint, target, kind, revision],
        )
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn next_change(connection: &Connection) -> Result<u64, Error> {
    connection.query_row("UPDATE counters SET value=value+1 WHERE name='change_sequence' AND value<9223372036854775807 RETURNING value",[],|row|row.get(0)).map_err(db_error)
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
    connection.execute("INSERT INTO quotas VALUES (?1,?2,?3,1) ON CONFLICT (principal,period_ms) DO UPDATE SET window_start_ms=excluded.window_start_ms,count=CASE WHEN quotas.window_start_ms=excluded.window_start_ms THEN quotas.count+1 ELSE 1 END",params![principal,period,start]).map_err(db_error)?;
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
    connection.query_row("SELECT COALESCE(SUM(value='up'),0),COALESCE(SUM(value='down'),0) FROM votes WHERE spot_id=?1",[target],|row|Ok(Counts {up:row.get(0)?,down:row.get(1)?})).map_err(db_error)
}

pub(crate) fn state(connection: &Connection, target: &str) -> Result<SpotState, Error> {
    let row = connection.query_row("SELECT revision,status,body,created_at_ms,updated_at_ms,verified_at_ms,change_sequence,duplicate_of FROM spots WHERE id=?1",[target],|row|Ok((row.get::<_,u64>(0)?,row.get::<_,SpotStatus>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,u64>(3)?,row.get::<_,u64>(4)?,row.get::<_,Option<u64>>(5)?,row.get::<_,u64>(6)?,row.get::<_,Option<String>>(7)?))).optional().map_err(db_error)?.ok_or(Error::NotFound)?;
    let spot = if row.1 == SpotStatus::Published {
        let input: SpotInput = serde_json::from_str(row.2.as_deref().ok_or(Error::Unavailable)?)
            .map_err(|_| Error::Unavailable)?;
        Some(PublicSpot {
            id: target.to_owned(),
            revision: row.0,
            created_at_ms: row.3,
            updated_at_ms: row.4,
            last_verified_at_ms: row.5,
            change_sequence: row.6,
            input,
            votes: counts(connection, target)?,
        })
    } else {
        None
    };
    Ok(SpotState {
        id: target.to_owned(),
        revision: row.0,
        status: row.1,
        change_sequence: row.6,
        spot,
        duplicate_of: row.7,
    })
}

#[cfg(test)]
mod tests {
    use super::{Error, Registration, Role, Store, hash, quota, registration_quota};

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
