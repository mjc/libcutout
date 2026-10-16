//! Cleanup of inactive durable event streams, preserving contiguous event identities.
use super::{StorageError, transform};
use crate::storage::live_capture::payload;
use rusqlite::{Connection, OptionalExtension, params};

struct Event {
    sequence: i64,
    kind: String,
    bytes: Vec<u8>,
    encoding: i64,
    original: Option<i64>,
}

fn next_event(
    connection: &Connection,
    id: &str,
    after: i64,
) -> Result<Option<Event>, StorageError> {
    Ok(connection.query_row("SELECT sequence,event_kind,payload,payload_encoding,payload_original_bytes FROM live_capture_events WHERE capture_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 1", params![id,after], |row| Ok(Event { sequence:row.get(0)?,kind:row.get(1)?,bytes:row.get(2)?,encoding:row.get(3)?,original:row.get(4)? })).optional()?)
}

/// Called before modifying any owned file, so an active legacy stream requires Stop & Save.
pub(super) fn check_active(connection: &Connection) -> Result<(), StorageError> {
    let mut statement =
        connection.prepare("SELECT capture_id FROM live_capture_sessions WHERE state='active'")?;
    let ids = statement.query_map([], |row| row.get::<_, String>(0))?;
    for id in ids {
        let id = id?;
        let mut after = -1;
        while let Some(Event {
            sequence,
            kind,
            bytes,
            encoding,
            original,
        }) = next_event(connection, &id, after)?
        {
            after = sequence;
            let bytes = payload::decode(bytes, encoding, original)?;
            if kind == "music"
                || !matches!(transform::line(&bytes)?, transform::LineOutcome::Unchanged)
            {
                return Err(StorageError::LiveCaptureInputInvalid(
                    "stop and save the active legacy capture before removing music metadata",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn purge(connection: &mut Connection) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
    let mut after_id = String::new();
    loop {
        let id = transaction.query_row("SELECT capture_id FROM live_capture_sessions WHERE state!='active' AND capture_id>?1 ORDER BY capture_id LIMIT 1", [&after_id], |row| row.get::<_,String>(0)).optional()?;
        let Some(id) = id else {
            break;
        };
        after_id.clone_from(&id);
        let mut after = -1;
        let mut next = 0_i64;
        while let Some(Event {
            sequence,
            kind,
            bytes,
            encoding,
            original,
        }) = next_event(&transaction, &id, after)?
        {
            after = sequence;
            let bytes = payload::decode(bytes, encoding, original)?;
            match (kind.as_str(), transform::line(&bytes)?) {
                ("music", _) | (_, transform::LineOutcome::Removed) => {
                    transaction.execute(
                        "DELETE FROM live_capture_events WHERE capture_id=?1 AND sequence=?2",
                        params![id, sequence],
                    )?;
                    continue;
                }
                (_, transform::LineOutcome::Unchanged) => {}
                (_, transform::LineOutcome::Rewritten(retained)) => {
                    let encoded = payload::encode(&retained)?;
                    transaction.execute("UPDATE live_capture_events SET payload=?1,payload_encoding=?2,payload_original_bytes=?3 WHERE capture_id=?4 AND sequence=?5",params![encoded.bytes.as_ref(),encoded.encoding,encoded.original_bytes,id,sequence])?;
                }
            }
            if sequence != next {
                // Destination slots are already empty: visit the original sequence in order.
                // Defer foreign keys while moving every child, including structured BLE evidence.
                for table in [
                    "live_capture_ble_raw_telemetry_fields",
                    "live_capture_ble_semantic_telemetry",
                    "live_capture_ble_observations",
                    "live_capture_location_observations",
                    "live_capture_events",
                ] {
                    transaction.execute(
                        &format!(
                            "UPDATE {table} SET sequence=?1 WHERE capture_id=?2 AND sequence=?3"
                        ),
                        params![next, id, sequence],
                    )?;
                }
            }
            next += 1;
        }
        transaction.execute("UPDATE live_capture_sessions SET next_sequence=?1, stored_bytes=length(header_json)+(SELECT COALESCE(SUM(COALESCE(payload_original_bytes,length(payload))),0) FROM live_capture_events WHERE capture_id=?2) WHERE capture_id=?2",params![next,id])?;
    }
    transaction.commit()?;
    Ok(())
}
