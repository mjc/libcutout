use super::super::{MapPointId, SpatialSchemaState, StorageError, ensure_spatial_schema};
use super::codec::Input;
use super::codec::{action_from_json, action_json, decode, encode};
use super::model::ValidatedDirectoryEntry;
use super::records::{
    EntryUpdate, apply_entry, detail, find_remote, insert_directory, point_id_from_bytes,
    remove_spatial_index, update_local_revision, update_spatial_index,
};
use super::{
    CHARGING_OUTBOX_CAPACITY, CHARGING_SYNC_PAGE_LIMIT, ChargingAction, ChargingBlockedOperation,
    ChargingChangeCursor, ChargingDirectoryEntry, ChargingOperation, ChargingOperationBlockKind,
    ChargingOperationId, ChargingOperationResolution, ChargingPageApplyResult, ChargingRevision,
    ChargingStatus, ChargingTarget, ChargingVoteReceipt, ChargingWriteReceipt,
};
use cutout_ride_maps::{ChargingSpotId, ChargingVote, NewChargingSpot};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub(super) fn count(connection: &Connection) -> Result<u32, StorageError> {
    Ok(connection.query_row("SELECT COUNT(*) FROM charging_outbox", [], |row| row.get(0))?)
}

pub(super) fn changes_cursor(
    connection: &Connection,
) -> Result<ChargingChangeCursor, StorageError> {
    let raw: i64 = connection.query_row(
        "SELECT cursor FROM charging_sync_state WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    ChargingChangeCursor::new(u64::try_from(raw).map_err(|_| StorageError::ChargingInvalidInput)?)
}

pub(super) fn directory_last_updated(connection: &Connection) -> Result<Option<u64>, StorageError> {
    let value: Option<i64> = connection.query_row(
        "SELECT last_refreshed_at_ms FROM charging_sync_state WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    value
        .map(|milliseconds| {
            u64::try_from(milliseconds).map_err(|_| StorageError::ChargingInvalidInput)
        })
        .transpose()
}

fn wall_clock_ms() -> Result<u64, StorageError> {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StorageError::ChargingInvalidInput)?
        .as_millis();
    let milliseconds =
        u64::try_from(milliseconds).map_err(|_| StorageError::ChargingInvalidInput)?;
    ChargingRevision::new(milliseconds)?;
    Ok(milliseconds)
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

fn queue_at_sequence(
    connection: &Connection,
    sequence: i64,
    id: MapPointId,
    revision: ChargingRevision,
    action: &ChargingAction,
    target: Option<&ChargingTarget>,
) -> Result<ChargingWriteReceipt, StorageError> {
    if count(connection)? >= CHARGING_OUTBOX_CAPACITY {
        return Err(StorageError::ChargingOutboxFull);
    }
    let operation_id = ChargingOperationId::from_uuid(Uuid::new_v4());
    connection.execute(
        "INSERT INTO charging_outbox(sequence,operation_id,point_id,action,local_revision,target) VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            sequence,
            operation_id.uuid().to_string(),
            id.uuid().as_bytes(),
            action_json(action)?,
            revision.get(),
            target.map(encode).transpose()?
        ],
    )?;
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
         FROM charging_outbox AS current
         WHERE blocked_kind IS NULL
           AND NOT EXISTS (
             SELECT 1 FROM charging_outbox AS prior
             WHERE prior.point_id=current.point_id AND prior.sequence<current.sequence
           )
         ORDER BY sequence LIMIT 1",
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

fn pending_operation_by_id(
    connection: &Connection,
    id: ChargingOperationId,
) -> Result<Option<PendingRow>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT operation_id,point_id,action,local_revision,target
         FROM charging_outbox AS current
         WHERE operation_id=?1 AND blocked_kind IS NULL
           AND NOT EXISTS (
             SELECT 1 FROM charging_outbox AS prior
             WHERE prior.point_id=current.point_id AND prior.sequence<current.sequence
           )",
            [id.uuid().to_string()],
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
    let operation = freeze_operation(&transaction, &pending)?;
    transaction.commit()?;
    Ok(Some(operation))
}

fn freeze_operation(
    connection: &Connection,
    pending: &PendingRow,
) -> Result<ChargingOperation, StorageError> {
    let point_id = point_id_from_bytes(&pending.point)?;
    let id = Uuid::parse_str(&pending.id)
        .map(ChargingOperationId::from_uuid)
        .map_err(|_| StorageError::ChargingInvalidInput)?;
    let action = action_from_json(&pending.action)?;
    let current = detail(connection, point_id)?.ok_or(StorageError::NotFound)?;
    let target = freeze_target(
        connection,
        &pending.id,
        &action,
        pending.target.as_deref(),
        &current,
    )?;
    Ok(ChargingOperation {
        id,
        point_id,
        local_revision: ChargingRevision::new(pending.revision)?,
        target,
        action,
    })
}

fn freeze_target(
    connection: &Connection,
    operation_id: &str,
    action: &ChargingAction,
    saved_target: Option<&str>,
    current: &super::StoredChargingSpot,
) -> Result<ChargingTarget, StorageError> {
    if let Some(target) = saved_target {
        return decode(target);
    }
    let target = match action {
        ChargingAction::Create(_) => ChargingTarget::Create,
        ChargingAction::Edit(_)
        | ChargingAction::Withdraw
        | ChargingAction::Verify
        | ChargingAction::Vote(_) => ChargingTarget::Existing {
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
        params![operation_id, encode(&target)?],
    )?;
    Ok(target)
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
    let Some(pending) = pending_operation_by_id(&transaction, id)? else {
        return replayed_acknowledgment(&transaction, id, entry.id);
    };
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

pub(super) fn acknowledge_vote_operation(
    connection: &mut Connection,
    id: ChargingOperationId,
    receipt: ChargingVoteReceipt,
) -> Result<MapPointId, StorageError> {
    ChargingRevision::new(receipt.votes_up)?;
    ChargingRevision::new(receipt.votes_down)?;
    let transaction = connection.transaction()?;
    let Some(pending) = pending_operation_by_id(&transaction, id)? else {
        let remote: Option<String> = transaction
            .query_row(
                "SELECT remote_id FROM charging_spots WHERE last_ack=?1",
                [id.uuid().to_string()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let remote = remote.ok_or(StorageError::ChargingConflict)?;
        let remote =
            ChargingSpotId::parse(&remote).map_err(|_| StorageError::ChargingInvalidInput)?;
        return replayed_acknowledgment(&transaction, id, remote);
    };
    let point_id = point_id_from_bytes(&pending.point)?;
    let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
    let action = action_from_json(&pending.action)?;
    if !action.is_vote() {
        return Err(StorageError::ChargingConflict);
    }
    let target: ChargingTarget = decode(&pending.target.ok_or(StorageError::ChargingConflict)?)?;
    let remote_id = current.remote_id.ok_or(StorageError::ChargingConflict)?;
    let entry = ChargingDirectoryEntry {
        id: remote_id,
        input: current.input.clone(),
        revision: current.remote_revision,
        change_sequence: current.change_sequence,
        status: current.status,
        verified_at_ms: current.verified_at_ms,
        vote: receipt.vote,
        vote_revision: receipt.vote_revision,
        votes_up: receipt.votes_up,
        votes_down: receipt.votes_down,
    };
    validate_acknowledgment(&current, &action, &target, receipt.applied_revision, &entry)?;
    if receipt.change_sequence.get() < current.change_sequence.get() {
        return Err(StorageError::ChargingConflict);
    }
    // The vote receipt has no body; only feed pages advance body revision or cursor.
    remove_operation(&transaction, id)?;
    transaction.execute(
        "UPDATE charging_spots SET vote=CASE WHEN EXISTS(
            SELECT 1 FROM charging_outbox WHERE point_id=?1 AND json_type(action,'$.Vote') IS NOT NULL
         ) THEN vote ELSE ?2 END,vote_revision=?3,votes_up=?4,votes_down=?5 WHERE point_id=?1",
        params![point_id.uuid().as_bytes(), receipt.vote.map(|value| match value { ChargingVote::Up => "up", ChargingVote::Down => "down" }), receipt.vote_revision.get(), receipt.votes_up, receipt.votes_down],
    )?;
    remember_acknowledgment(&transaction, point_id, id)?;
    transaction.commit()?;
    Ok(point_id)
}

fn replayed_acknowledgment(
    connection: &Connection,
    id: ChargingOperationId,
    remote_id: ChargingSpotId,
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
        || (!current.status.visible()
            && entry.status == ChargingStatus::Published
            && entry.change_sequence <= current.change_sequence)
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

fn remove_superseded_field_operations(
    connection: &Connection,
    point_id: MapPointId,
    selected: ChargingOperationId,
    selected_sequence: i64,
    supersede_fields: bool,
) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM charging_outbox
         WHERE point_id=?1 AND (operation_id=?2 OR (?3 AND sequence>?4 AND
           (json_type(action,'$.Create') IS NOT NULL
            OR json_type(action,'$.Edit') IS NOT NULL)))",
        params![
            point_id.uuid().as_bytes(),
            selected.uuid().to_string(),
            supersede_fields,
            selected_sequence
        ],
    )?;
    Ok(())
}

fn restore_pending_local_state(
    transaction: &Connection,
    current: &super::StoredChargingSpot,
) -> Result<(), StorageError> {
    let point_id = current.point_id.uuid();
    let point = point_id.as_bytes();
    let pending_vote: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM charging_outbox WHERE point_id=?1 AND json_type(action,'$.Vote') IS NOT NULL)",
        [point],
        |row| row.get(0),
    )?;
    let pending_fields: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM charging_outbox WHERE point_id=?1 AND (json_type(action,'$.Create') IS NOT NULL OR json_type(action,'$.Edit') IS NOT NULL))",
        [point],
        |row| row.get(0),
    )?;
    let pending_withdrawal: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM charging_outbox WHERE point_id=?1 AND json_extract(action,'$')='Withdraw')",
        [point],
        |row| row.get(0),
    )?;

    if pending_vote {
        transaction.execute(
            "UPDATE charging_spots SET vote=?2 WHERE point_id=?1",
            params![
                point,
                current.vote.map(|vote| match vote {
                    ChargingVote::Up => "up",
                    ChargingVote::Down => "down",
                })
            ],
        )?;
    }
    if pending_fields && let Some(input) = &current.input {
        update_spatial_index(transaction, current.point_id, input, current.status)?;
        transaction.execute(
            "UPDATE charging_spots SET input=?2 WHERE point_id=?1",
            params![point, super::codec::input_json(input)?],
        )?;
    }
    if pending_withdrawal {
        remove_spatial_index(transaction, current.point_id)?;
        transaction.execute(
            "UPDATE charging_spots SET status='withdrawn',input=NULL WHERE point_id=?1",
            [point],
        )?;
    }
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryEntryWire {
    id: String,
    input_json: Option<String>,
    revision: u64,
    change_sequence: u64,
    status: ChargingStatus,
    verified_at_ms: Option<u64>,
    vote: Option<ChargingVote>,
    vote_revision: u64,
    votes_up: u64,
    votes_down: u64,
}

fn directory_entry_json(entry: &ChargingDirectoryEntry) -> Result<String, StorageError> {
    encode(&DirectoryEntryWire {
        id: entry.id.as_uuid().to_string(),
        input_json: entry
            .input
            .as_ref()
            .map(super::codec::input_json)
            .transpose()?,
        revision: entry.revision.get(),
        change_sequence: entry.change_sequence.get(),
        status: entry.status,
        verified_at_ms: entry.verified_at_ms,
        vote: entry.vote,
        vote_revision: entry.vote_revision.get(),
        votes_up: entry.votes_up,
        votes_down: entry.votes_down,
    })
}

fn directory_entry_from_json(value: &str) -> Result<ChargingDirectoryEntry, StorageError> {
    let wire: DirectoryEntryWire = decode(value)?;
    let entry = ChargingDirectoryEntry {
        id: ChargingSpotId::parse(&wire.id).map_err(|_| StorageError::ChargingInvalidInput)?,
        input: wire
            .input_json
            .map(|value| decode::<Input>(&value)?.validated())
            .transpose()?,
        revision: ChargingRevision::new(wire.revision)?,
        change_sequence: ChargingRevision::new(wire.change_sequence)?,
        status: wire.status,
        verified_at_ms: wire.verified_at_ms,
        vote: wire.vote,
        vote_revision: ChargingRevision::new(wire.vote_revision)?,
        votes_up: wire.votes_up,
        votes_down: wire.votes_down,
    };
    ValidatedDirectoryEntry::new(&entry)?;
    Ok(entry)
}

pub(super) fn apply_changes_page(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    entries: &[ChargingDirectoryEntry],
    next_cursor: ChargingChangeCursor,
) -> Result<ChargingPageApplyResult, StorageError> {
    if entries.len() > CHARGING_SYNC_PAGE_LIMIT {
        return Err(StorageError::ChargingInvalidInput);
    }
    let mut previous = None;
    for entry in entries {
        entry.validate()?;
        let sequence = entry.change_sequence.get();
        if previous.is_some_and(|value| sequence <= value) || sequence > next_cursor.get() {
            return Err(StorageError::ChargingInvalidInput);
        }
        previous = Some(sequence);
    }
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let current_cursor = changes_cursor(&transaction)?;
    if next_cursor < current_cursor {
        return Err(StorageError::ChargingInvalidInput);
    }
    let mut applied_entries = 0_u32;
    let mut blocked_operations = 0_u32;
    for entry in entries {
        if entry.change_sequence.get() <= current_cursor.get() {
            continue;
        }
        apply_change_entry(&transaction, entry, &mut blocked_operations)?;
        applied_entries = applied_entries
            .checked_add(1)
            .ok_or(StorageError::ChargingInvalidInput)?;
    }
    let refreshed_at_ms = wall_clock_ms()?;
    transaction.execute(
        "UPDATE charging_sync_state SET cursor=?1,last_refreshed_at_ms=?2 WHERE singleton=1",
        params![next_cursor.get(), refreshed_at_ms],
    )?;
    transaction.commit()?;
    Ok(ChargingPageApplyResult {
        cursor: next_cursor,
        applied_entries,
        blocked_operations,
    })
}

fn apply_change_entry(
    transaction: &rusqlite::Transaction<'_>,
    entry: &ChargingDirectoryEntry,
    blocked_operations: &mut u32,
) -> Result<(), StorageError> {
    let checked = ValidatedDirectoryEntry::new(entry)?;
    let Some(point_id) = find_remote(transaction, entry.id)? else {
        let id = MapPointId::from_uuid(entry.id.as_uuid());
        insert_directory(transaction, id, &checked)?;
        apply_entry(transaction, id, &checked, EntryUpdate::ReplaceBody)?;
        return Ok(());
    };
    let current = detail(transaction, point_id)?.ok_or(StorageError::NotFound)?;
    if entry.change_sequence <= current.change_sequence {
        return Ok(());
    }
    let public_only_vote = entry.vote_revision == ChargingRevision::ZERO;
    if entry.revision < current.remote_revision
        || (!public_only_vote && entry.vote_revision < current.vote_revision)
    {
        return Err(StorageError::ChargingConflict);
    }
    let pending_count: u32 = transaction.query_row(
        "SELECT COUNT(*) FROM charging_outbox WHERE point_id=?1",
        [point_id.uuid().as_bytes()],
        |row| row.get(0),
    )?;
    if pending_count > 0 {
        let (conflict, has_pending_fields) =
            review_pending_changes(transaction, point_id, &current, entry)?;
        if let Some(operation_id) = conflict {
            let was_blocked: bool = transaction.query_row(
                "SELECT blocked_kind IS NOT NULL FROM charging_outbox WHERE operation_id=?1",
                [&operation_id],
                |row| row.get(0),
            )?;
            store_conflict(transaction, &operation_id, &current, entry, "conflict")?;
            if !was_blocked {
                *blocked_operations = blocked_operations
                    .checked_add(1)
                    .ok_or(StorageError::ChargingInvalidInput)?;
            }
        }
        if current.status == ChargingStatus::Withdrawn && entry.status == ChargingStatus::Published
        {
            update_remote_metadata(transaction, point_id, entry, current.pending_vote)?;
            transaction.execute(
                "UPDATE charging_spots SET status='withdrawn' WHERE point_id=?1",
                [point_id.uuid().as_bytes()],
            )?;
            remove_spatial_index(transaction, point_id)?;
        } else {
            let update = if has_pending_fields && entry.status.visible() {
                EntryUpdate::PreserveInput
            } else {
                EntryUpdate::ReplaceBody
            };
            apply_entry(transaction, point_id, &checked, update)?;
            if current.pending_vote || public_only_vote {
                restore_vote(transaction, point_id, &current, public_only_vote)?;
            } else if entry.input.is_none() && entry.status != ChargingStatus::Published {
                restore_vote(transaction, point_id, &current, true)?;
                transaction.execute(
                    "UPDATE charging_spots SET votes_up=?2,votes_down=?3 WHERE point_id=?1",
                    params![
                        point_id.uuid().as_bytes(),
                        current.votes_up,
                        current.votes_down
                    ],
                )?;
            }
        }
        update_local_revision(transaction, point_id, current.local_revision.next()?)?;
        return Ok(());
    }
    // A local withdrawal is a tombstone even if a later page or delayed receipt carries an old body.
    if current.status == ChargingStatus::Withdrawn && entry.status == ChargingStatus::Published {
        update_remote_metadata(transaction, point_id, entry, false)?;
        remove_spatial_index(transaction, point_id)?;
        update_local_revision(transaction, point_id, current.local_revision.next()?)?;
        return Ok(());
    }
    apply_entry(transaction, point_id, &checked, EntryUpdate::ReplaceBody)?;
    if public_only_vote {
        restore_vote(transaction, point_id, &current, true)?;
    }
    update_local_revision(transaction, point_id, current.local_revision.next()?)?;
    Ok(())
}

fn restore_vote(
    connection: &Connection,
    point_id: MapPointId,
    current: &super::StoredChargingSpot,
    include_revision: bool,
) -> Result<(), StorageError> {
    connection.execute(
        "UPDATE charging_spots SET vote=?2,vote_revision=CASE WHEN ?3 THEN ?4 ELSE vote_revision END WHERE point_id=?1",
        params![
            point_id.uuid().as_bytes(),
            current.vote.map(|value| match value {
                ChargingVote::Up => "up",
                ChargingVote::Down => "down",
            }),
            include_revision,
            current.vote_revision.get(),
        ],
    )?;
    Ok(())
}

fn review_pending_changes(
    connection: &Connection,
    point_id: MapPointId,
    current: &super::StoredChargingSpot,
    entry: &ChargingDirectoryEntry,
) -> Result<(Option<String>, bool), StorageError> {
    let rows: Vec<(String, String, Option<String>)> = {
        let mut statement = connection.prepare(
            "SELECT operation_id,action,target FROM charging_outbox WHERE point_id=?1 ORDER BY sequence",
        )?;
        let rows = statement.query_map([point_id.uuid().as_bytes()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let status_conflict = !entry.status.visible() || current.status != entry.status;
    let mut first_conflict = None;
    let mut has_pending_fields = false;
    for (operation_id, action_json, target_json) in rows {
        let action = action_from_json(&action_json)?;
        has_pending_fields |= action.carries_fields();
        let saved_target = target_json.as_deref();
        let target = saved_target.map(decode::<ChargingTarget>).transpose()?;
        let expected = target.as_ref().and_then(|target| match target {
            ChargingTarget::Existing {
                expected_revision, ..
            } => Some(*expected_revision),
            ChargingTarget::Create => None,
        });
        let (old_revision, incoming_revision) = if action.is_vote() {
            (current.vote_revision, entry.vote_revision)
        } else {
            (current.remote_revision, entry.revision)
        };
        let conflicts = status_conflict || incoming_revision > expected.unwrap_or(old_revision);
        if conflicts {
            // Freeze every operation whose revision domain this feed advanced. Later rows
            // remain queued behind the first review, but cannot adopt the new base silently.
            freeze_target(connection, &operation_id, &action, saved_target, current)?;
            if first_conflict.is_none() {
                first_conflict = Some(operation_id);
            }
        }
    }
    Ok((first_conflict, has_pending_fields))
}

fn store_conflict(
    connection: &Connection,
    operation_id: &str,
    current: &super::StoredChargingSpot,
    remote: &ChargingDirectoryEntry,
    kind: &str,
) -> Result<(), StorageError> {
    let action: String = connection.query_row(
        "SELECT action FROM charging_outbox WHERE operation_id=?1",
        [operation_id],
        |row| row.get(0),
    )?;
    let action = action_from_json(&action)?;
    let local_draft = if action.carries_fields() {
        current
            .input
            .as_ref()
            .map(super::codec::input_json)
            .transpose()?
    } else {
        None
    };
    connection.execute(
        "UPDATE charging_outbox SET blocked_kind=COALESCE(blocked_kind,?2) WHERE operation_id=?1",
        params![operation_id, kind],
    )?;
    connection.execute(
        "INSERT INTO charging_sync_conflicts(operation_id,local_draft,remote_entry) VALUES (?1,?2,?3)
         ON CONFLICT(operation_id) DO UPDATE SET
           local_draft=COALESCE(charging_sync_conflicts.local_draft,excluded.local_draft),
           remote_entry=excluded.remote_entry",
        params![operation_id, local_draft, directory_entry_json(remote)?],
    )?;
    Ok(())
}

fn update_remote_metadata(
    connection: &Connection,
    point_id: MapPointId,
    entry: &ChargingDirectoryEntry,
    pending_vote: bool,
) -> Result<(), StorageError> {
    connection.execute(
        "UPDATE charging_spots SET status=?2,remote_id=?3,remote_revision=?4,change_sequence=?5,
         verified_at_ms=?6,vote=CASE WHEN ?7 THEN vote ELSE ?8 END,vote_revision=?9,
         votes_up=?10,votes_down=?11 WHERE point_id=?1",
        params![
            point_id.uuid().as_bytes(),
            entry.status.as_str(),
            entry.id.as_uuid().to_string(),
            entry.revision.get(),
            entry.change_sequence.get(),
            entry.verified_at_ms,
            pending_vote,
            entry.vote.map(|value| match value {
                ChargingVote::Up => "up",
                ChargingVote::Down => "down",
            }),
            entry.vote_revision.get(),
            entry.votes_up,
            entry.votes_down,
        ],
    )?;
    Ok(())
}

pub(super) fn cache_vote(
    connection: &mut Connection,
    remote_id: ChargingSpotId,
    vote: Option<ChargingVote>,
    vote_revision: ChargingRevision,
    votes_up: u64,
    votes_down: u64,
    change_sequence: ChargingChangeCursor,
) -> Result<MapPointId, StorageError> {
    ChargingRevision::new(votes_up)?;
    ChargingRevision::new(votes_down)?;
    let point_id = find_remote(connection, remote_id)?.ok_or(StorageError::NotFound)?;
    let transaction = connection.transaction()?;
    let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
    if change_sequence.get() < current.change_sequence.get()
        || vote_revision < current.vote_revision
    {
        return Err(StorageError::ChargingConflict);
    }
    transaction.execute(
        "UPDATE charging_spots SET vote=CASE WHEN EXISTS(
            SELECT 1 FROM charging_outbox WHERE point_id=?1 AND json_type(action,'$.Vote') IS NOT NULL
         ) THEN vote ELSE ?2 END,vote_revision=?3,votes_up=?4,votes_down=?5,change_sequence=?6
         WHERE point_id=?1",
        params![
            point_id.uuid().as_bytes(),
            vote.map(|value| match value { ChargingVote::Up => "up", ChargingVote::Down => "down" }),
            vote_revision.get(),
            votes_up,
            votes_down,
            change_sequence.get(),
        ],
    )?;
    update_local_revision(&transaction, point_id, current.local_revision.next()?)?;
    transaction.commit()?;
    Ok(point_id)
}

fn block_kind_str(kind: ChargingOperationBlockKind) -> &'static str {
    match kind {
        ChargingOperationBlockKind::Conflict => "conflict",
        ChargingOperationBlockKind::Invalid => "invalid",
    }
}

fn parse_block_kind(value: &str) -> Result<ChargingOperationBlockKind, StorageError> {
    match value {
        "conflict" => Ok(ChargingOperationBlockKind::Conflict),
        "invalid" => Ok(ChargingOperationBlockKind::Invalid),
        _ => Err(StorageError::ChargingInvalidInput),
    }
}

pub(super) fn block_operation(
    connection: &mut Connection,
    id: ChargingOperationId,
    kind: ChargingOperationBlockKind,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    let row: Option<(Vec<u8>, String, String)> = transaction.query_row(
        "SELECT point_id,operation_id,action FROM charging_outbox WHERE operation_id=?1 AND target IS NOT NULL",
        [id.uuid().to_string()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    let (point, operation_id, action) = row.ok_or(StorageError::ChargingConflict)?;
    let action = action_from_json(&action)?;
    let point_id = point_id_from_bytes(&point)?;
    let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
    let local_draft = if action.carries_fields() {
        current
            .input
            .as_ref()
            .map(super::codec::input_json)
            .transpose()?
    } else {
        None
    };
    transaction.execute(
        "UPDATE charging_outbox SET blocked_kind=?2 WHERE operation_id=?1",
        params![operation_id, block_kind_str(kind)],
    )?;
    transaction.execute(
        "INSERT INTO charging_sync_conflicts(operation_id,local_draft) VALUES (?1,?2)
         ON CONFLICT(operation_id) DO UPDATE SET local_draft=COALESCE(charging_sync_conflicts.local_draft,excluded.local_draft)",
        params![operation_id, local_draft],
    )?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn list_blocked_operations(
    connection: &Connection,
) -> Result<Vec<ChargingBlockedOperation>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT o.operation_id,o.point_id,o.action,o.local_revision,o.target,o.blocked_kind,c.local_draft,c.remote_entry
         FROM charging_outbox o LEFT JOIN charging_sync_conflicts c USING(operation_id)
         WHERE o.blocked_kind IS NOT NULL ORDER BY o.sequence",
    )?;
    let mut rows = statement.query([])?;
    let mut issues = Vec::new();
    while let Some(row) = rows.next()? {
        let operation_id: String = row.get(0)?;
        let point_id = point_id_from_bytes(&row.get::<_, Vec<u8>>(1)?)?;
        let action = action_from_json(&row.get::<_, String>(2)?)?;
        let queued_revision = ChargingRevision::new(row.get(3)?)?;
        let target: Option<String> = row.get(4)?;
        let kind = parse_block_kind(&row.get::<_, String>(5)?)?;
        let local_draft: Option<String> = row.get(6)?;
        let local_draft = local_draft
            .map(|value| decode::<Input>(&value).and_then(Input::validated))
            .transpose()?;
        let remote_entry: Option<String> = row.get(7)?;
        let remote_entry = remote_entry
            .map(|value| directory_entry_from_json(&value))
            .transpose()?;
        let current = detail(connection, point_id)?.ok_or(StorageError::NotFound)?;
        let local_draft = if action.carries_fields() {
            local_draft.or(current.input)
        } else {
            None
        };
        let target = if let Some(target) = target {
            decode(&target)?
        } else if match &action {
            ChargingAction::Create(_) => true,
            ChargingAction::Edit(_)
            | ChargingAction::Withdraw
            | ChargingAction::Verify
            | ChargingAction::Vote(_) => false,
        } {
            ChargingTarget::Create
        } else {
            ChargingTarget::Existing {
                id: current
                    .remote_id
                    .or_else(|| remote_entry.as_ref().map(|entry| entry.id))
                    .ok_or(StorageError::ChargingConflict)?,
                expected_revision: if action.is_vote() {
                    current.vote_revision
                } else {
                    current.remote_revision
                },
            }
        };
        issues.push(ChargingBlockedOperation {
            operation: ChargingOperation {
                id: ChargingOperationId::from_uuid(
                    Uuid::parse_str(&operation_id)
                        .map_err(|_| StorageError::ChargingInvalidInput)?,
                ),
                point_id,
                local_revision: queued_revision,
                target,
                action,
            },
            kind,
            local_revision: current.local_revision,
            local_draft,
            can_rebase: kind == ChargingOperationBlockKind::Conflict
                && remote_entry
                    .as_ref()
                    .is_none_or(|entry| entry.status.visible()),
            remote_entry,
        });
    }
    Ok(issues)
}

#[allow(
    clippy::too_many_lines,
    reason = "the reviewed resolution and all queue changes must remain one transaction"
)]
pub(super) fn resolve_operation(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    id: ChargingOperationId,
    expected_local_revision: ChargingRevision,
    resolution: ChargingOperationResolution,
) -> Result<(), StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let row: Option<(i64, Vec<u8>, String, String)> = transaction
        .query_row(
            "SELECT sequence,point_id,blocked_kind,action FROM charging_outbox WHERE operation_id=?1",
            [id.uuid().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let (selected_sequence, point, kind_text, action_text) =
        row.ok_or(StorageError::ChargingConflict)?;
    let kind = parse_block_kind(&kind_text)?;
    let action = action_from_json(&action_text)?;
    let carries_fields = action.carries_fields();
    let point_id = point_id_from_bytes(&point)?;
    let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
    if current.local_revision != expected_local_revision {
        return Err(StorageError::ChargingConflict);
    }
    let conflict: Option<(Option<String>, Option<String>)> = transaction
        .query_row(
            "SELECT local_draft,remote_entry FROM charging_sync_conflicts WHERE operation_id=?1",
            [id.uuid().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (local_draft, remote) = conflict.ok_or(StorageError::ChargingConflict)?;
    let local_draft = local_draft
        .map(|value| decode::<Input>(&value).and_then(Input::validated))
        .transpose()?;
    let remote = remote
        .map(|value| directory_entry_from_json(&value))
        .transpose()?;
    let retries = match resolution {
        ChargingOperationResolution::Retry | ChargingOperationResolution::RebaseKeepingLocal => {
            true
        }
        ChargingOperationResolution::AcceptServer => false,
    };
    let retryable_rejection = kind == ChargingOperationBlockKind::Invalid && action.is_withdraw();
    if retries
        && ((kind == ChargingOperationBlockKind::Invalid && !retryable_rejection)
            || remote.as_ref().is_some_and(|entry| !entry.status.visible()))
    {
        return Err(StorageError::ChargingConflict);
    }
    match resolution {
        ChargingOperationResolution::Retry => {
            transaction.execute(
                "UPDATE charging_outbox SET blocked_kind=NULL WHERE operation_id=?1",
                [id.uuid().to_string()],
            )?;
        }
        ChargingOperationResolution::RebaseKeepingLocal => {
            let draft = if carries_fields {
                local_draft.or(current.input.clone())
            } else {
                None
            };
            if let Some(input) = &draft {
                transaction.execute(
                    "UPDATE charging_spots SET input=?2 WHERE point_id=?1",
                    params![point_id.uuid().as_bytes(), super::codec::input_json(input)?],
                )?;
                update_spatial_index(&transaction, point_id, input, current.status)?;
            }
            transaction.execute(
                "DELETE FROM charging_sync_conflicts WHERE operation_id=?1",
                [id.uuid().to_string()],
            )?;
            transaction.execute(
                "UPDATE charging_outbox SET operation_id=?2,blocked_kind=NULL,target=NULL WHERE operation_id=?1",
                params![id.uuid().to_string(), Uuid::new_v4().to_string()],
            )?;
            update_local_revision(&transaction, point_id, current.local_revision.next()?)?;
        }
        ChargingOperationResolution::AcceptServer => {
            let Some(entry) = remote else {
                let rejected_create = match action {
                    ChargingAction::Create(_) => true,
                    ChargingAction::Edit(_)
                    | ChargingAction::Withdraw
                    | ChargingAction::Verify
                    | ChargingAction::Vote(_) => false,
                };
                if kind != ChargingOperationBlockKind::Invalid
                    || current.remote_id.is_some()
                    || !rejected_create
                {
                    return Err(StorageError::ChargingConflict);
                }
                transaction.execute(
                    "DELETE FROM charging_outbox WHERE point_id=?1 AND local_revision<=?2",
                    params![point_id.uuid().as_bytes(), expected_local_revision.get()],
                )?;
                transaction.execute(
                    "UPDATE charging_spots SET status='withdrawn',local_revision=?2 WHERE point_id=?1",
                    params![point_id.uuid().as_bytes(), current.local_revision.next()?.get()],
                )?;
                remove_spatial_index(&transaction, point_id)?;
                transaction.commit()?;
                return Ok(());
            };
            let checked = ValidatedDirectoryEntry::new(&entry)?;
            if entry.change_sequence < current.change_sequence
                || entry.revision < current.remote_revision
            {
                return Err(StorageError::ChargingConflict);
            }
            remove_superseded_field_operations(
                &transaction,
                point_id,
                id,
                selected_sequence,
                carries_fields,
            )?;
            apply_entry(&transaction, point_id, &checked, EntryUpdate::ReplaceBody)?;
            restore_pending_local_state(&transaction, &current)?;
            update_local_revision(&transaction, point_id, current.local_revision.next()?)?;
        }
    }
    transaction.execute(
        "DELETE FROM charging_sync_conflicts WHERE operation_id=?1",
        [id.uuid().to_string()],
    )?;
    // The schema helper is deliberately called before commit so map-index failures roll back the choice.
    let _ = spatial;
    transaction.commit()?;
    Ok(())
}

pub(super) fn replace_blocked_operation(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    id: ChargingOperationId,
    expected_local_revision: ChargingRevision,
    input: &NewChargingSpot,
) -> Result<ChargingWriteReceipt, StorageError> {
    ensure_spatial_schema(connection, spatial)?;
    let transaction = connection.transaction()?;
    let row: Option<(i64, Vec<u8>, String, String)> = transaction
        .query_row(
            "SELECT sequence,point_id,blocked_kind,action FROM charging_outbox WHERE operation_id=?1",
            [id.uuid().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let (sequence, point, _, blocked_action) = row.ok_or(StorageError::ChargingConflict)?;
    let supersede_fields = action_from_json(&blocked_action)?.carries_fields();
    let point_id = point_id_from_bytes(&point)?;
    let current = detail(&transaction, point_id)?.ok_or(StorageError::NotFound)?;
    if current.local_revision != expected_local_revision || !current.owned {
        return Err(StorageError::ChargingConflict);
    }
    let remote: Option<String> = transaction
        .query_row(
            "SELECT remote_entry FROM charging_sync_conflicts WHERE operation_id=?1",
            [id.uuid().to_string()],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let reviewed_remote = remote
        .map(|value| directory_entry_from_json(&value))
        .transpose()?;
    if reviewed_remote
        .as_ref()
        .is_some_and(|entry| !entry.status.visible())
    {
        return Err(StorageError::ChargingConflict);
    }
    let action = if current.remote_id.is_some() {
        if current.status != ChargingStatus::Published {
            return Err(StorageError::ChargingConflict);
        }
        ChargingAction::Edit(input.clone())
    } else {
        if current.status != ChargingStatus::Queued {
            return Err(StorageError::ChargingConflict);
        }
        ChargingAction::Create(input.clone())
    };
    remove_superseded_field_operations(&transaction, point_id, id, sequence, supersede_fields)?;
    let revision = current.local_revision.next()?;
    transaction.execute(
        "UPDATE charging_spots SET input=?2,local_revision=?3 WHERE point_id=?1",
        params![
            point_id.uuid().as_bytes(),
            super::codec::input_json(input)?,
            revision.get()
        ],
    )?;
    update_spatial_index(&transaction, point_id, input, current.status)?;
    let target = current.remote_id.map(|id| ChargingTarget::Existing {
        id,
        expected_revision: reviewed_remote
            .as_ref()
            .map_or(current.remote_revision, |entry| entry.revision),
    });
    let receipt = queue_at_sequence(
        &transaction,
        sequence,
        point_id,
        revision,
        &action,
        target.as_ref(),
    )?;
    transaction.commit()?;
    Ok(receipt)
}
