//! Bounded, restartable consolidation of legacy duplicate semantic snapshots.

use super::{StorageError, live_capture};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const SEMANTIC_BATCH_ROWS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BatchState {
    Pending,
    Deferred,
    Finished,
}

/// Removes at most one small batch of semantic rows already represented exactly by event bytes.
///
/// Progress and deletion share a transaction. A process stop therefore either retains the
/// complete batch or resumes strictly after the last committed row. Independent, malformed, and
/// oversized semantic rows remain available as explicit overrides.
pub(super) fn remove_redundant_semantic_batch(
    connection: &mut Connection,
) -> Result<BatchState, StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let has_active_work: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE state IN ('active', 'paused'))
             OR EXISTS(SELECT 1 FROM live_capture_sessions WHERE state = 'active')
             OR EXISTS(SELECT 1 FROM pevcap_import_work)
             OR EXISTS(SELECT 1 FROM pevcap_captures WHERE receipt_digest IS NULL)",
        [],
        |row| row.get(0),
    )?;
    if has_active_work {
        return Ok(BatchState::Deferred);
    }
    let progress: Option<(Option<String>, Option<i64>, bool)> = transaction
        .query_row(
            "SELECT semantic_capture_id, semantic_sequence, semantic_complete
             FROM storage_normalization_progress WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((cursor_capture_id, cursor_sequence, semantic_complete)) = progress else {
        return Err(StorageError::InvalidDatabaseIdentity);
    };
    if semantic_complete {
        return Ok(BatchState::Finished);
    }

    let entries = read_batch(&transaction, cursor_capture_id.as_deref(), cursor_sequence)?;

    let Some((last_capture_id, last_sequence, _, _, _, _)) = entries.last() else {
        transaction.execute(
            "UPDATE storage_normalization_progress
             SET semantic_complete = 1, compaction_pending = 1
             WHERE singleton = 1",
            [],
        )?;
        transaction.commit()?;
        return Ok(BatchState::Finished);
    };
    for (capture_id, sequence, snapshot_size, payload, encoding, original_bytes) in &entries {
        if !(1..=live_capture::LIVE_CAPTURE_EVENT_LIMIT_BYTES as i64).contains(snapshot_size) {
            continue;
        }
        let (Some(payload), Some(encoding)) = (payload.as_ref(), *encoding) else {
            continue;
        };
        let Ok(payload) = live_capture::payload::decode(payload.clone(), encoding, *original_bytes)
        else {
            continue;
        };
        let Ok(projection) = read_projection(&transaction, capture_id, *sequence) else {
            continue;
        };
        if live_capture::semantic_telemetry_matches_event(&payload, &projection) {
            transaction.execute(
                "DELETE FROM live_capture_ble_semantic_telemetry
                 WHERE capture_id = ?1 AND sequence = ?2",
                params![capture_id, sequence],
            )?;
        }
    }
    let completed = entries.len() < SEMANTIC_BATCH_ROWS;
    transaction.execute(
        "UPDATE storage_normalization_progress
         SET semantic_capture_id = ?1, semantic_sequence = ?2,
             semantic_complete = ?3, compaction_pending = ?3
         WHERE singleton = 1",
        params![last_capture_id, last_sequence, completed],
    )?;
    transaction.commit()?;
    Ok(if completed {
        BatchState::Finished
    } else {
        BatchState::Pending
    })
}

type SemanticBatchEntry = (String, i64, i64, Option<Vec<u8>>, Option<i64>, Option<i64>);

fn read_batch(
    connection: &Connection,
    cursor_capture_id: Option<&str>,
    cursor_sequence: Option<i64>,
) -> Result<Vec<SemanticBatchEntry>, StorageError> {
    let mut statement = if cursor_capture_id.is_some() {
        connection.prepare(
            "SELECT semantic.capture_id, semantic.sequence,
                    length(CAST(semantic.snapshot_json AS BLOB)),
                    event.payload, event.payload_encoding, event.payload_original_bytes
             FROM live_capture_ble_semantic_telemetry AS semantic
             LEFT JOIN live_capture_events AS event
               ON event.capture_id = semantic.capture_id AND event.sequence = semantic.sequence
             WHERE (semantic.capture_id, semantic.sequence) > (?1, ?2)
             ORDER BY semantic.capture_id, semantic.sequence
             LIMIT ?3",
        )?
    } else {
        connection.prepare(
            "SELECT semantic.capture_id, semantic.sequence,
                    length(CAST(semantic.snapshot_json AS BLOB)),
                    event.payload, event.payload_encoding, event.payload_original_bytes
             FROM live_capture_ble_semantic_telemetry AS semantic
             LEFT JOIN live_capture_events AS event
               ON event.capture_id = semantic.capture_id AND event.sequence = semantic.sequence
             ORDER BY semantic.capture_id, semantic.sequence
             LIMIT ?1",
        )?
    };
    let mut entries = Vec::with_capacity(SEMANTIC_BATCH_ROWS);
    if let Some(capture_id) = cursor_capture_id {
        let rows = statement.query_map(
            params![capture_id, cursor_sequence, SEMANTIC_BATCH_ROWS],
            read_semantic_batch_entry,
        )?;
        entries.extend(rows.collect::<Result<Vec<_>, _>>()?);
    } else {
        let rows = statement.query_map([SEMANTIC_BATCH_ROWS], read_semantic_batch_entry)?;
        entries.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    Ok(entries)
}

fn read_semantic_batch_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<SemanticBatchEntry> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn read_projection(
    connection: &Connection,
    capture_id: &str,
    sequence: i64,
) -> Result<cutout_core::PevcapSemanticTelemetry, StorageError> {
    let (observed_at_ms, provenance, schema_version, library_version, snapshot_json): (
        Option<i64>,
        String,
        u16,
        String,
        String,
    ) = connection.query_row(
        "SELECT observed_at_ms, provenance, snapshot_schema_version, library_version, snapshot_json
         FROM live_capture_ble_semantic_telemetry
         WHERE capture_id = ?1 AND sequence = ?2",
        params![capture_id, sequence],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;
    let provenance = match provenance.as_str() {
        "live_session" => cutout_core::PevcapTelemetryProvenance::LiveSession,
        "replay" => cutout_core::PevcapTelemetryProvenance::Replay,
        _ => return Err(StorageError::InvalidDatabaseIdentity),
    };
    let observed_at_ms = observed_at_ms
        .map(u64::try_from)
        .transpose()
        .map_err(|_| StorageError::InvalidDatabaseIdentity)?
        .map(cutout_core::MonotonicTimestamp::new);
    Ok(cutout_core::PevcapSemanticTelemetry {
        observed_at_ms,
        provenance,
        snapshot_schema_version: schema_version,
        library_version,
        snapshot_json,
    })
}
