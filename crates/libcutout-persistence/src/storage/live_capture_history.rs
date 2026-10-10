//! Session-row history and provenance, without copying or scanning captured events.

use super::{
    Command, LiveCaptureId, LiveCaptureIntegrity, LiveCaptureState, QueryLimit, RecordedCapture,
    RideDatabase, StorageError,
};
use crate::{CaptureArtifactId, SavedDatabaseCapture};
use cutout_core::{CaptureOrigin, PevcapEncoding, PevcapReader, WallClockUnixTimestamp};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable descending session-history continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCaptureHistoryCursor {
    /// Recording start, independent of publication time.
    pub started_at_ms: u64,
    /// Tie-break identity.
    pub id: LiveCaptureId,
}

/// Durable capture metadata; completeness and lifecycle are independent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCaptureHistoryEntry {
    /// Canonical source identity for explicit export.
    pub id: LiveCaptureId,
    /// Finished or recovered interruption state.
    pub state: LiveCaptureState,
    /// Persisted completeness evidence, never inferred from an export.
    pub integrity: LiveCaptureIntegrity,
    /// Provenance retained from the consumed writer, when publication succeeded.
    pub recording: Option<RecordedCapture>,
    /// Peripheral identity from the durable header, also available after interruption.
    pub platform_identifier: String,
    /// Recording start.
    pub started_at_ms: u64,
    /// Finalization wall clock, unavailable for interrupted captures.
    pub finished_at_ms: Option<u64>,
    /// Exact retained raw event count.
    pub event_count: u64,
    /// Uncompressed JSONL header and event bytes, excluding delimiters.
    pub stored_bytes: u64,
}

/// Bounded session history page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCaptureHistoryPage {
    /// Inactive captures, newest recording first.
    pub captures: Vec<LiveCaptureHistoryEntry>,
    /// Present only when more session rows exist.
    pub next_cursor: Option<LiveCaptureHistoryCursor>,
}

#[derive(Serialize, Deserialize)]
struct RecordingContext {
    artifact_id: String,
    manual: bool,
    advertised_name: Option<String>,
    published_at_ms: u64,
}

impl RideDatabase {
    /// Retains consumed-writer provenance on the existing session row, without JSONL export or
    /// re-import. Retrying identical provenance is idempotent; conflicting identity is rejected.
    ///
    /// # Errors
    /// Returns invalid metadata, mismatched completion authority or storage errors. Failure does
    /// not remove the already-completed capture from database history.
    pub fn retain_finished_database_capture(
        &self,
        receipt: &SavedDatabaseCapture,
        origin: CaptureOrigin,
        advertised_name: Option<&str>,
        published_at: WallClockUnixTimestamp,
    ) -> Result<(), StorageError> {
        let time = published_at.as_milliseconds();
        if time == 0
            || time > i64::MAX as u64
            || advertised_name.is_some_and(|name| name.len() > 512)
        {
            return Err(invalid(
                "recording provenance",
                "invalid time or advertisement length",
            ));
        }
        let context = RecordingContext {
            artifact_id: receipt.artifact_id().to_string(),
            manual: origin == CaptureOrigin::Manual,
            advertised_name: advertised_name.map(str::to_owned),
            published_at_ms: time,
        };
        let context_json = serde_json::to_string(&context)
            .map_err(|error| invalid("recording provenance", &error.to_string()))?;
        let header_json = receipt
            .final_header()
            .to_jsonl_line()
            .map_err(|error| invalid("capture header", &error.to_string()))?
            .into_bytes();
        self.request(|reply| Command::RetainLiveCaptureContext {
            id: receipt.live_capture_id(),
            header_json,
            context_json,
            reply,
        })
    }

    /// Lists durable inactive captures using session metadata only. This includes interrupted
    /// sessions and provenance-publication failures; neither is hidden by a missing export.
    ///
    /// # Errors
    /// Returns invalid stored metadata, cursor or storage failures.
    pub fn list_live_capture_history(
        &self,
        cursor: Option<LiveCaptureHistoryCursor>,
        limit: QueryLimit,
    ) -> Result<LiveCaptureHistoryPage, StorageError> {
        if cursor
            .as_ref()
            .is_some_and(|cursor| cursor.started_at_ms > i64::MAX as u64)
        {
            return Err(invalid(
                "capture history cursor",
                "time exceeds SQLite range",
            ));
        }
        self.request(|reply| Command::ListLiveCaptureHistory {
            cursor,
            limit,
            reply,
        })
    }
}

pub(super) fn retain_context(
    connection: &mut Connection,
    id: LiveCaptureId,
    header: &[u8],
    context: &str,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    let row: Option<(String, Vec<u8>, Option<String>)> = transaction.query_row(
        "SELECT state, header_json, recording_context_json FROM live_capture_sessions WHERE capture_id = ?1",
        [id.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    let (state, stored_header, previous) = row.ok_or(StorageError::NotFound)?;
    if state != "finished" || stored_header != header {
        return Err(StorageError::CaptureIdentityConflict);
    }
    if let Some(previous) = previous {
        let old: RecordingContext = serde_json::from_str(&previous)
            .map_err(|error| invalid("recording provenance", &error.to_string()))?;
        let new: RecordingContext = serde_json::from_str(context)
            .map_err(|error| invalid("recording provenance", &error.to_string()))?;
        // Publication time can change on retry; identity and provenance cannot.
        if old.artifact_id != new.artifact_id
            || old.manual != new.manual
            || old.advertised_name != new.advertised_name
        {
            return Err(StorageError::CaptureIdentityConflict);
        }
    } else {
        transaction.execute(
            "UPDATE live_capture_sessions SET recording_context_json = ?2 WHERE capture_id = ?1",
            params![id.to_string(), context],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

struct HistoryRow {
    id: String,
    state: String,
    integrity: String,
    dropped: u64,
    header_bytes: Vec<u8>,
    started_at_ms: u64,
    finished_at_ms: Option<u64>,
    event_count: u64,
    stored_bytes: u64,
    context: Option<String>,
}

impl HistoryRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            state: row.get(1)?,
            integrity: row.get(2)?,
            dropped: row.get(3)?,
            header_bytes: row.get(4)?,
            started_at_ms: row.get(5)?,
            finished_at_ms: row.get(6)?,
            event_count: row.get(7)?,
            stored_bytes: row.get(8)?,
            context: row.get(9)?,
        })
    }

    fn into_entry(mut self) -> Result<LiveCaptureHistoryEntry, StorageError> {
        self.header_bytes.push(b'\n');
        let reader = PevcapReader::new(self.header_bytes.as_slice(), PevcapEncoding::Jsonl)
            .map_err(|error| invalid("capture header", &error.to_string()))?;
        let header = reader.header();
        let recording = self
            .context
            .map(|context| {
                let context: RecordingContext = serde_json::from_str(&context)
                    .map_err(|error| invalid("recording provenance", &error.to_string()))?;
                let artifact_id = Uuid::parse_str(&context.artifact_id)
                    .map_err(|error| invalid("artifact identity", &error.to_string()))?;
                Ok::<_, StorageError>(RecordedCapture {
                    artifact_id: CaptureArtifactId::from_uuid(artifact_id),
                    origin: if context.manual {
                        CaptureOrigin::Manual
                    } else {
                        CaptureOrigin::Automatic
                    },
                    advertised_name: context.advertised_name,
                    platform_identifier: header.platform_id.clone(),
                    started_at: header.wall_clock_start_unix_ms,
                    model: header
                        .resolved_identity
                        .as_ref()
                        .and_then(|identity| identity.model.clone()),
                })
            })
            .transpose()?;
        Ok(LiveCaptureHistoryEntry {
            id: LiveCaptureId::parse(&self.id)?,
            state: LiveCaptureState::parse(&self.state)?,
            integrity: LiveCaptureIntegrity::parse(&self.integrity, self.dropped)?,
            recording,
            platform_identifier: header.platform_id.clone(),
            started_at_ms: self.started_at_ms,
            finished_at_ms: self.finished_at_ms,
            event_count: self.event_count,
            stored_bytes: self.stored_bytes,
        })
    }
}

pub(super) fn list(
    connection: &Connection,
    cursor: Option<&LiveCaptureHistoryCursor>,
    limit: QueryLimit,
) -> Result<LiveCaptureHistoryPage, StorageError> {
    let mut query = connection.prepare(
        "SELECT capture_id, state, integrity, dropped_messages, header_json, started_at_ms,
            finished_at_ms, next_sequence, stored_bytes, recording_context_json
         FROM live_capture_sessions
         WHERE state != 'active' AND (?1 IS NULL OR started_at_ms < ?1 OR (started_at_ms = ?1 AND capture_id < ?2))
         ORDER BY started_at_ms DESC, capture_id DESC LIMIT ?3")?;
    let rows = query.query_map(
        params![
            cursor.map(|c| c.started_at_ms),
            cursor.map(|c| c.id.to_string()),
            u64::from(limit.get()) + 1
        ],
        HistoryRow::from_row,
    )?;
    let mut captures = rows
        .map(|row| row?.into_entry())
        .collect::<Result<Vec<_>, StorageError>>()?;
    let has_more = captures.len() > limit.get() as usize;
    captures.truncate(limit.get() as usize);
    let next_cursor = if has_more {
        captures.last().map(|last| LiveCaptureHistoryCursor {
            started_at_ms: last.started_at_ms,
            id: last.id,
        })
    } else {
        None
    };
    Ok(LiveCaptureHistoryPage {
        captures,
        next_cursor,
    })
}

fn invalid(field: &'static str, value: &str) -> StorageError {
    StorageError::InvalidStoredValue {
        field,
        value: value.to_owned(),
    }
}
