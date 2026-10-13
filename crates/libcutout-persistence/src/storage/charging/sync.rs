use super::super::{MapPointId, SpatialSchemaState, StorageError, ensure_spatial_schema};
use super::codec::{action_from_json, action_json, decode, encode};
use super::model::ValidatedDirectoryEntry;
use super::records::{
    EntryUpdate, apply_entry, detail, find_remote, insert_directory, point_id_from_bytes,
    update_local_revision,
};
use super::{
    CHARGING_OUTBOX_CAPACITY, ChargingAction, ChargingDirectoryEntry, ChargingOperation,
    ChargingOperationId, ChargingRevision, ChargingStatus, ChargingTarget, ChargingWriteReceipt,
};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

pub(super) fn count(connection: &Connection) -> Result<u32, StorageError> {
    Ok(connection.query_row("SELECT COUNT(*) FROM charging_outbox", [], |row| row.get(0))?)
}
pub(super) fn queue(
    connection: &Connection,
    id: MapPointId,
    revision: ChargingRevision,
    action: &ChargingAction,
) -> Result<ChargingWriteReceipt, StorageError> {
    if count(connection)? >= CHARGING_OUTBOX_CAPACITY {
        return Err(StorageError::ChargingOutboxFull);
    }
    let operation_id = ChargingOperationId::from_uuid(Uuid::new_v4());
    connection.execute("INSERT INTO charging_outbox(operation_id,point_id,action,local_revision) VALUES (?1,?2,?3,?4)", params![operation_id.uuid().to_string(), id.uuid().as_bytes(), action_json(action)?, revision.get()])?;
    Ok(ChargingWriteReceipt {
        point_id: id,
        revision,
        operation_id,
    })
}
struct PendingRow {
    id: String,
    point: Vec<u8>,
    action: String,
    revision: u64,
    target: Option<String>,
}

fn pending_operation(connection: &Connection) -> Result<Option<PendingRow>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT operation_id,point_id,action,local_revision,target
         FROM charging_outbox ORDER BY sequence LIMIT 1",
            [],
            |row| {
                Ok(PendingRow {
                    id: row.get(0)?,
                    point: row.get(1)?,
                    action: row.get(2)?,
                    revision: row.get(3)?,
                    target: row.get(4)?,
                })
            },
        )
        .optional()?)
}

pub(super) fn next_operation(
    connection: &mut Connection,
) -> Result<Option<ChargingOperation>, StorageError> {
    let transaction = connection.transaction()?;
    let Some(pending) = pending_operation(&transaction)? else {
        return Ok(None);
    };
    let operation = freeze_operation(&transaction, pending)?;
    transaction.commit()?;
    Ok(Some(operation))
}

fn freeze_operation(
    connection: &Connection,
    pending: PendingRow,
) -> Result<ChargingOperation, StorageError> {
    let point_id = point_id_from_bytes(&pending.point)?;
    let id = Uuid::parse_str(&pending.id)
        .map(ChargingOperationId::from_uuid)
        .map_err(|_| StorageError::ChargingInvalidInput)?;
    let action = action_from_json(&pending.action)?;
    let target = if let Some(target) = pending.target {
        decode(&target)?
    } else {
        let current = detail(connection, point_id)?.ok_or(StorageError::NotFound)?;
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
        connection.execute(
            "UPDATE charging_outbox SET target=?2 WHERE operation_id=?1",
            params![id.uuid().to_string(), encode(&target)?],
        )?;
        target
    };
    Ok(ChargingOperation {
        id,
        point_id,
        local_revision: ChargingRevision::new(pending.revision)?,
        target,
        action,
    })
}
pub(super) fn cache_directory_entry(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    entry: &ChargingDirectoryEntry,
) -> Result<MapPointId, StorageError> {
    let checked = ValidatedDirectoryEntry::new(entry)?;
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let existing = find_remote(&transaction, entry.id)?;
    let id = existing.unwrap_or(MapPointId::from_uuid(entry.id.as_uuid()));
    if let Some(current) = detail(&transaction, id)? {
        if cache_is_unchanged(&transaction, entry, &current)? {
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
        insert_directory(&transaction, id, &checked)?;
    }
    apply_entry(&transaction, id, &checked, EntryUpdate::ReplaceBody)?;
    if existing.is_some() {
        let current = detail(&transaction, id)?.ok_or(StorageError::NotFound)?;
        update_local_revision(&transaction, id, current.local_revision.next()?)?;
    }
    transaction.commit()?;
    Ok(id)
}
pub(super) fn acknowledge_operation(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    id: ChargingOperationId,
    applied: ChargingRevision,
    entry: &ChargingDirectoryEntry,
) -> Result<MapPointId, StorageError> {
    let checked = ValidatedDirectoryEntry::new(entry)?;
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let row = pending_operation(&transaction)?;
    if row
        .as_ref()
        .is_none_or(|pending| pending.id != id.uuid().to_string())
    {
        return replayed_acknowledgment(&transaction, id, entry.id);
    }
    let pending = row.ok_or(StorageError::ChargingConflict)?;
    let point_id = point_id_from_bytes(&pending.point)?;
    let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
    let action = action_from_json(&pending.action)?;
    let target: ChargingTarget = decode(&pending.target.ok_or(StorageError::ChargingConflict)?)?;
    validate_acknowledgment(&current, &action, &target, applied, entry)?;
    remove_operation(&transaction, id)?;
    let pending: bool = has_pending_operations(&transaction, point_id)?;
    preserve_pending_publication(&transaction, &current, entry, pending)?;
    let update = if pending {
        EntryUpdate::PreserveLocal
    } else {
        EntryUpdate::ReplaceBody
    };
    apply_entry(&transaction, point_id, &checked, update)?;
    remember_acknowledgment(&transaction, point_id, id)?;
    transaction.commit()?;
    Ok(point_id)
}

fn replayed_acknowledgment(
    connection: &Connection,
    id: ChargingOperationId,
    remote_id: cutout_ride_maps::ChargingSpotId,
) -> Result<MapPointId, StorageError> {
    let previous: Option<Vec<u8>> = connection
        .query_row(
            "SELECT point_id FROM charging_spots WHERE last_ack=?1 AND remote_id=?2",
            params![id.uuid().to_string(), remote_id.as_uuid().to_string()],
            |row| row.get(0),
        )
        .optional()?;
    previous
        .map(|value| point_id_from_bytes(&value))
        .transpose()?
        .ok_or(StorageError::ChargingConflict)
}

fn validate_acknowledgment(
    current: &super::StoredChargingSpot,
    action: &ChargingAction,
    target: &ChargingTarget,
    applied: ChargingRevision,
    entry: &ChargingDirectoryEntry,
) -> Result<(), StorageError> {
    let expected = match *target {
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
            ChargingRevision::ZERO
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
    Ok(())
}

fn preserve_pending_publication(
    transaction: &Connection,
    current: &super::StoredChargingSpot,
    entry: &ChargingDirectoryEntry,
    pending: bool,
) -> Result<(), StorageError> {
    if pending
        && current.status == ChargingStatus::Queued
        && entry.status == ChargingStatus::Published
    {
        transaction.execute(
            "UPDATE charging_spots SET status='published' WHERE point_id=?1",
            [current.point_id.uuid().as_bytes()],
        )?;
    }
    Ok(())
}

fn has_pending_operations(connection: &Connection, id: MapPointId) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM charging_outbox WHERE point_id=?1)",
        [id.uuid().as_bytes()],
        |row| row.get(0),
    )?)
}

fn cache_is_unchanged(
    connection: &Connection,
    entry: &ChargingDirectoryEntry,
    current: &super::StoredChargingSpot,
) -> Result<bool, StorageError> {
    let pending: bool = has_pending_operations(connection, current.point_id)?;
    if current.remote_id != Some(entry.id)
        || pending
        || entry.change_sequence < current.change_sequence
        || entry.revision < current.remote_revision
        || entry.vote_revision < current.vote_revision
    {
        return Err(StorageError::ChargingConflict);
    }
    Ok(entry.change_sequence == current.change_sequence
        && entry.vote_revision == current.vote_revision)
}

fn remove_operation(connection: &Connection, id: ChargingOperationId) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM charging_outbox WHERE operation_id=?1",
        [id.uuid().to_string()],
    )?;
    Ok(())
}

fn remember_acknowledgment(
    connection: &Connection,
    point_id: MapPointId,
    id: ChargingOperationId,
) -> Result<(), StorageError> {
    connection.execute(
        "UPDATE charging_spots SET last_ack=?2 WHERE point_id=?1",
        params![point_id.uuid().as_bytes(), id.uuid().to_string()],
    )?;
    Ok(())
}
