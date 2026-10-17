use super::super::StorageError;
use rusqlite::{Connection, params};

/// Durable gate for community mutations, independent of Keychain and view lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommunityAccountState {
    /// Existing credentials may contribute.
    Active,
    /// Deletion may have reached the service; only confirmation retries are allowed.
    Deleting,
    /// Confirmed deletion; retained public pins no longer belong to this account.
    Deleted {
        /// Completion time returned by the service.
        completed_at_milliseconds: u64,
    },
}

pub(crate) enum AccountEvent {
    Read,
    BeginDeletion,
    CompleteDeletion { at_ms: u64 },
}

pub(super) fn state(connection: &Connection) -> Result<CommunityAccountState, StorageError> {
    let (phase, completed): (String, Option<u64>) = connection.query_row(
        "SELECT account_state, account_deleted_at_ms FROM charging_sync_state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    match (phase.as_str(), completed) {
        ("active", None) => Ok(CommunityAccountState::Active),
        ("deleting", None) => Ok(CommunityAccountState::Deleting),
        ("deleted", Some(completed_at_milliseconds)) => Ok(CommunityAccountState::Deleted {
            completed_at_milliseconds,
        }),
        _ => Err(StorageError::ChargingInvalidInput),
    }
}

pub(super) fn ensure_active(connection: &Connection) -> Result<(), StorageError> {
    match state(connection)? {
        CommunityAccountState::Active => Ok(()),
        CommunityAccountState::Deleting | CommunityAccountState::Deleted { .. } => {
            Err(StorageError::CommunityAccountUnavailable)
        }
    }
}

pub(super) fn apply(
    connection: &mut Connection,
    event: AccountEvent,
) -> Result<CommunityAccountState, StorageError> {
    if let AccountEvent::Read = event {
        return state(connection);
    }
    let transaction = connection.transaction()?;
    let current = state(&transaction)?;
    match (current, event) {
        (CommunityAccountState::Active, AccountEvent::BeginDeletion) => {
            transaction.execute(
                "UPDATE charging_sync_state SET account_state='deleting' WHERE singleton=1",
                [],
            )?;
        }
        (CommunityAccountState::Deleting, AccountEvent::CompleteDeletion { at_ms }) => {
            i64::try_from(at_ms).map_err(|_| StorageError::ChargingInvalidInput)?;
            transaction.execute("DELETE FROM charging_outbox", [])?;
            // The installation has one community identity. Retire only operations already
            // bound by Send, in the same transaction as its confirmed account deletion.
            // Prepared local snapshots and the original recordings remain untouched.
            transaction.execute(
                "UPDATE capture_submissions SET status_json = json_set(status_json,
                 '$.phase', 'deleted', '$.receipt', NULL, '$.failure', NULL,
                 '$.revision', json_extract(status_json, '$.revision') + 1)
                 WHERE json_extract(status_json, '$.account_id') IS NOT NULL",
                [],
            )?;
            transaction.execute(
                "UPDATE charging_spots SET owned=0,
                 vote=NULL,vote_revision=0,last_ack=NULL",
                [],
            )?;
            transaction.execute(
                "UPDATE charging_sync_state SET account_state='deleted',account_deleted_at_ms=?1,
                 cursor=0,last_refreshed_at_ms=NULL WHERE singleton=1",
                params![at_ms],
            )?;
        }
        (CommunityAccountState::Active, AccountEvent::CompleteDeletion { .. }) => {
            return Err(StorageError::ChargingConflict);
        }
        (
            _,
            AccountEvent::Read
            | AccountEvent::BeginDeletion
            | AccountEvent::CompleteDeletion { .. },
        ) => {}
    }
    let result = state(&transaction)?;
    transaction.commit()?;
    Ok(result)
}
