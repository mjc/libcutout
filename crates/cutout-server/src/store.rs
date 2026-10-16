use crate::{
    Error,
    model::{MutationReceipt, OperationId},
};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

mod backup;
mod budgets;
mod identity;
mod spots;
#[cfg(test)]
mod tests;
mod votes;

pub(crate) use budgets::{QuotaWindow, quota, write_quota};
pub(crate) use identity::{
    Actor, DeletionReceipt, Identity, ModeratorActor, Registration, Role, ensure_actor,
};
pub(crate) use spots::{published_spot, state};
pub(crate) use votes::clear_spot_votes;

/// A durable SQLite directory. SQL and migrations stay behind this boundary.
#[derive(Clone, Debug)]
pub struct Store {
    connection: Arc<Mutex<Connection>>,
    work: Arc<Semaphore>,
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
        let mut connection = open_connection(path, mode)?;
        prepare_schema(&mut connection, mode)?;
        configure_durability(&connection)?;
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

    pub(crate) async fn ready(&self) -> Result<(), Error> {
        self.call(|connection| {
            let version: u32 = connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .map_err(db_error)?;
            if version != 5 {
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
        self.call(move |connection| backup::backup_database(connection, &output))
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

pub(crate) fn next_revision(value: u64) -> Result<u64, Error> {
    value
        .checked_add(1)
        .filter(|value| i64::try_from(*value).is_ok())
        .ok_or(Error::Conflict)
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

fn open_connection(path: &Path, mode: OpenMode) -> Result<Connection, Error> {
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
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(db_error)?;
    Ok(connection)
}

fn prepare_schema(connection: &mut Connection, mode: OpenMode) -> Result<(), Error> {
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
            migrate_account_deletion(connection)?;
            migrate_alpha_signup(connection)?;
            migrate_alpha_signup_sync(connection)?;
            migrate_alpha_signup_removal(connection)?;
        }
        1 if application == 0x4353_5054 && mode == OpenMode::Initialize => {}
        1 | 2 if application == 0x4353_5054 && mode == OpenMode::Existing => {
            return Err(Error::Unavailable);
        }
        2 if application == 0x4353_5054 && mode == OpenMode::Initialize => {
            migrate_alpha_signup(connection)?;
            migrate_alpha_signup_sync(connection)?;
            migrate_alpha_signup_removal(connection)?;
        }
        3 if application == 0x4353_5054 && mode == OpenMode::Initialize => {
            migrate_alpha_signup_sync(connection)?;
            migrate_alpha_signup_removal(connection)?;
        }
        4 if application == 0x4353_5054 && mode == OpenMode::Initialize => {
            migrate_alpha_signup_removal(connection)?;
        }
        5 if application == 0x4353_5054 => return Ok(()),
        _ => return Err(Error::Unavailable),
    }
    if version == 1 {
        migrate_account_deletion(connection)?;
        migrate_alpha_signup(connection)?;
        migrate_alpha_signup_sync(connection)?;
        migrate_alpha_signup_removal(connection)?;
    }
    Ok(())
}

fn migrate_alpha_signup(connection: &mut Connection) -> Result<(), Error> {
    let transaction = connection.transaction().map_err(db_error)?;
    transaction
        .execute_batch(include_str!("../migrations/003_alpha_signup.sql"))
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)
}

fn migrate_alpha_signup_sync(connection: &mut Connection) -> Result<(), Error> {
    let transaction = connection.transaction().map_err(db_error)?;
    transaction
        .execute_batch(include_str!("../migrations/004_alpha_signup_sync.sql"))
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)
}

fn migrate_alpha_signup_removal(connection: &mut Connection) -> Result<(), Error> {
    let transaction = connection.transaction().map_err(db_error)?;
    transaction
        .execute_batch(include_str!("../migrations/005_alpha_signup_removal.sql"))
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)
}

fn migrate_account_deletion(connection: &mut Connection) -> Result<(), Error> {
    let transaction = connection.transaction().map_err(db_error)?;
    transaction
        .execute_batch(include_str!("../migrations/002_account_deletion.sql"))
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)
}

fn configure_durability(connection: &Connection) -> Result<(), Error> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys=ON;
             PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA secure_delete=ON;",
        )
        .map_err(db_error)?;
    Ok(())
}
