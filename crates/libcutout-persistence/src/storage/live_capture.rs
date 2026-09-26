//! Incremental, bounded live-capture event persistence in the canonical database worker.

use super::{Command, QueryLimit, RideDatabase, StorageError};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// Maximum header size retained for a live capture.
pub const LIVE_CAPTURE_HEADER_LIMIT_BYTES: usize = 65_536;
/// Maximum event payload size accepted by one live-capture write.
pub const LIVE_CAPTURE_EVENT_LIMIT_BYTES: usize = 65_536;
/// Maximum total header and event bytes retained for one live capture.
pub const LIVE_CAPTURE_TOTAL_LIMIT_BYTES: u64 = 536_870_912;

/// Rust-issued identity for one live capture, independent of ride and artifact identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LiveCaptureId(Uuid);

impl LiveCaptureId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }

    fn as_string(self) -> String {
        self.0.to_string()
    }
}

/// Persisted event kind; event payload bytes retain the complete PEVCAP observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveCaptureEventKind {
    /// Link became available.
    LinkUp,
    /// Link was lost.
    LinkDown,
    /// Outbound GATT write.
    Write,
    /// Inbound GATT notification.
    Notification,
    /// Core Location observation.
    Location,
    /// Music observation.
    Music,
    /// Capture metadata update.
    Metadata,
}

impl LiveCaptureEventKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LinkUp => "link_up",
            Self::LinkDown => "link_down",
            Self::Write => "write",
            Self::Notification => "notification",
            Self::Location => "location",
            Self::Music => "music",
            Self::Metadata => "metadata",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "link_up" => Ok(Self::LinkUp),
            "link_down" => Ok(Self::LinkDown),
            "write" => Ok(Self::Write),
            "notification" => Ok(Self::Notification),
            "location" => Ok(Self::Location),
            "music" => Ok(Self::Music),
            "metadata" => Ok(Self::Metadata),
            _ => Err(StorageError::InvalidStoredValue {
                field: "live capture event kind",
                value: value.to_owned(),
            }),
        }
    }
}

/// Lifecycle state of an incrementally persisted live capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveCaptureState {
    /// Accepting ordered events.
    Active,
    /// Finished normally.
    Finished,
    /// Was active when its database worker stopped.
    Interrupted,
}

impl LiveCaptureState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Finished => "finished",
            Self::Interrupted => "interrupted",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "active" => Ok(Self::Active),
            "finished" => Ok(Self::Finished),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(StorageError::InvalidStoredValue {
                field: "live capture state",
                value: value.to_owned(),
            }),
        }
    }
}

/// Durable knowledge about whether a finished capture retained every admitted message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveCaptureIntegrity {
    /// Every message admitted by the capture writer was retained.
    Complete,
    /// The capture finished after one or more writer messages were rejected.
    Incomplete {
        /// Number of writer messages rejected before finalization.
        dropped_messages: u64,
    },
    /// Completeness cannot be proven, for example after process interruption.
    Unknown,
}

impl LiveCaptureIntegrity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete { .. } => "incomplete",
            Self::Unknown => "unknown",
        }
    }

    const fn dropped_messages(self) -> u64 {
        match self {
            Self::Incomplete { dropped_messages } => dropped_messages,
            Self::Complete | Self::Unknown => 0,
        }
    }

    fn parse(value: &str, dropped_messages: u64) -> Result<Self, StorageError> {
        match (value, dropped_messages) {
            ("complete", 0) => Ok(Self::Complete),
            ("incomplete", dropped_messages @ 1..) => Ok(Self::Incomplete { dropped_messages }),
            ("unknown", 0) => Ok(Self::Unknown),
            ("complete" | "incomplete" | "unknown", _) => Err(StorageError::InvalidStoredValue {
                field: "live capture integrity loss count",
                value: format!("{value}:{dropped_messages}"),
            }),
            _ => Err(StorageError::InvalidStoredValue {
                field: "live capture integrity",
                value: value.to_owned(),
            }),
        }
    }
}

/// One ordered event and its original serialized payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCaptureEvent {
    /// Capture-local sequence assigned by the Rust database worker.
    pub sequence: u64,
    /// Typed event category.
    pub kind: LiveCaptureEventKind,
    /// Capture-relative callback receipt time.
    pub receipt_monotonic_ms: u64,
    /// Calibrated capture-relative source time, when available.
    pub source_monotonic_offset_ms: Option<i64>,
    /// Original source wall-clock time, when the event has one.
    pub source_wall_clock_unix_ms: Option<u64>,
    /// Original event payload bytes.
    pub payload: Vec<u8>,
}

/// Bounded snapshot of a live capture and its earliest events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCaptureSnapshot {
    /// Rust-issued capture identity.
    pub id: LiveCaptureId,
    /// Current durable lifecycle state.
    pub state: LiveCaptureState,
    /// Durable completeness information, distinct from lifecycle state.
    pub integrity: LiveCaptureIntegrity,
    /// Exact header JSON supplied when capture began.
    pub header_json: Vec<u8>,
    /// Capture start wall-clock timestamp.
    pub started_at_ms: u64,
    /// Finalization timestamp, absent until normal completion.
    pub finished_at_ms: Option<u64>,
    /// Next event sequence to be assigned.
    pub next_sequence: u64,
    /// Bounded ordered event page beginning at sequence zero.
    pub events: Vec<LiveCaptureEvent>,
}

/// Schema for durable live sessions and independently queryable event payloads.
pub(super) const SCHEMA: &str = "
    CREATE TABLE live_capture_sessions (
        capture_id TEXT PRIMARY KEY NOT NULL CHECK (length(capture_id) = 36),
        state TEXT NOT NULL CHECK (state IN ('active', 'finished', 'interrupted')),
        integrity TEXT NOT NULL CHECK (integrity IN ('complete', 'incomplete', 'unknown')),
        dropped_messages INTEGER NOT NULL DEFAULT 0 CHECK (dropped_messages >= 0),
        header_json BLOB NOT NULL CHECK (length(header_json) BETWEEN 1 AND 65536),
        started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
        finished_at_ms INTEGER CHECK (finished_at_ms IS NULL OR finished_at_ms >= started_at_ms),
        next_sequence INTEGER NOT NULL DEFAULT 0 CHECK (next_sequence >= 0),
        stored_bytes INTEGER NOT NULL CHECK (stored_bytes BETWEEN 1 AND 536870912),
        CHECK ((state = 'finished') = (finished_at_ms IS NOT NULL)),
        CHECK ((integrity = 'complete' AND dropped_messages = 0)
            OR (integrity = 'incomplete' AND dropped_messages > 0)
            OR integrity = 'unknown')
    );
    CREATE TABLE live_capture_events (
        capture_id TEXT NOT NULL REFERENCES live_capture_sessions(capture_id) ON DELETE CASCADE,
        sequence INTEGER NOT NULL CHECK (sequence >= 0),
        event_kind TEXT NOT NULL CHECK (event_kind IN
            ('link_up', 'link_down', 'write', 'notification', 'location', 'music', 'metadata')),
        receipt_monotonic_ms INTEGER NOT NULL CHECK (receipt_monotonic_ms >= 0),
        source_monotonic_offset_ms INTEGER,
        source_wall_clock_unix_ms INTEGER CHECK
            (source_wall_clock_unix_ms IS NULL OR source_wall_clock_unix_ms >= 0),
        payload BLOB NOT NULL CHECK (length(payload) BETWEEN 1 AND 65536),
        PRIMARY KEY (capture_id, sequence)
    ) WITHOUT ROWID;
    CREATE INDEX live_capture_events_receipt_order
        ON live_capture_events(capture_id, receipt_monotonic_ms, sequence);
    CREATE INDEX live_capture_events_source_wall_clock
        ON live_capture_events(capture_id, source_wall_clock_unix_ms, sequence);
";

impl RideDatabase {
    /// Starts a live capture in the existing Rust-owned database worker.
    ///
    /// The header is stored before the new identity is returned. Call this on a writer task,
    /// not an Apple callback or UI executor.
    ///
    /// # Errors
    /// Returns a queue, worker, input-bound, or SQLite error.
    pub fn begin_live_capture(
        &self,
        header_json: Vec<u8>,
        started_at_ms: u64,
    ) -> Result<LiveCaptureId, StorageError> {
        validate_timestamp(started_at_ms)?;
        if header_json.is_empty() || header_json.len() > LIVE_CAPTURE_HEADER_LIMIT_BYTES {
            return Err(StorageError::LiveCaptureInputInvalid("header size"));
        }
        let id = LiveCaptureId::new();
        self.begin_live_capture_with_id(id, header_json, started_at_ms)?;
        Ok(id)
    }

    pub(crate) fn begin_live_capture_with_id(
        &self,
        id: LiveCaptureId,
        header_json: Vec<u8>,
        started_at_ms: u64,
    ) -> Result<(), StorageError> {
        validate_timestamp(started_at_ms)?;
        if header_json.is_empty() || header_json.len() > LIVE_CAPTURE_HEADER_LIMIT_BYTES {
            return Err(StorageError::LiveCaptureInputInvalid("header size"));
        }
        self.request(|reply| Command::BeginLiveCapture {
            id,
            header_json,
            started_at_ms,
            reply,
        })
    }

    /// Appends an event; the database worker assigns its capture-local sequence transactionally.
    ///
    /// # Errors
    /// Returns a queue, worker, input-bound, inactive-session, or SQLite error.
    pub fn append_live_capture_event(
        &self,
        id: LiveCaptureId,
        kind: LiveCaptureEventKind,
        receipt_monotonic_ms: u64,
        source_monotonic_offset_ms: Option<i64>,
        source_wall_clock_unix_ms: Option<u64>,
        payload: Vec<u8>,
    ) -> Result<u64, StorageError> {
        validate_timestamp(receipt_monotonic_ms)?;
        if let Some(source_wall_clock_unix_ms) = source_wall_clock_unix_ms {
            validate_timestamp(source_wall_clock_unix_ms)?;
        }
        if payload.is_empty() || payload.len() > LIVE_CAPTURE_EVENT_LIMIT_BYTES {
            return Err(StorageError::LiveCaptureInputInvalid("event payload size"));
        }
        self.request(|reply| Command::AppendLiveCaptureEvent {
            id,
            kind,
            receipt_monotonic_ms,
            source_monotonic_offset_ms,
            source_wall_clock_unix_ms,
            payload,
            reply,
        })
    }

    /// Marks a live capture finished; retrying with the same finish time is idempotent.
    ///
    /// # Errors
    /// Returns a queue, worker, inactive-session, timestamp, or SQLite error.
    pub fn finish_live_capture(
        &self,
        id: LiveCaptureId,
        finished_at_ms: u64,
    ) -> Result<(), StorageError> {
        self.finish_live_capture_with_integrity(id, finished_at_ms, LiveCaptureIntegrity::Complete)
    }

    /// Finishes a live capture while durably preserving known writer-message loss.
    ///
    /// `Unknown` is reserved for old or interrupted captures and cannot be used to claim a
    /// successful finish.
    ///
    /// # Errors
    /// Returns a queue, worker, inactive-session, integrity, timestamp, or SQLite error.
    pub fn finish_live_capture_with_integrity(
        &self,
        id: LiveCaptureId,
        finished_at_ms: u64,
        integrity: LiveCaptureIntegrity,
    ) -> Result<(), StorageError> {
        validate_timestamp(finished_at_ms)?;
        match integrity {
            LiveCaptureIntegrity::Unknown
            | LiveCaptureIntegrity::Incomplete {
                dropped_messages: 0,
            } => return Err(StorageError::LiveCaptureInputInvalid("finish integrity")),
            LiveCaptureIntegrity::Complete | LiveCaptureIntegrity::Incomplete { .. } => {}
        }
        self.request(|reply| Command::FinishLiveCapture {
            id,
            finished_at_ms,
            integrity,
            reply,
        })
    }

    /// Replaces the staged header after Rust applies capture metadata changes.
    ///
    /// # Errors
    /// Returns a queue, worker, size-bound, inactive-session, or SQLite error.
    pub(crate) fn update_live_capture_header(
        &self,
        id: LiveCaptureId,
        header_json: Vec<u8>,
    ) -> Result<(), StorageError> {
        if header_json.is_empty() || header_json.len() > LIVE_CAPTURE_HEADER_LIMIT_BYTES {
            return Err(StorageError::LiveCaptureInputInvalid("header size"));
        }
        self.request(|reply| Command::UpdateLiveCaptureHeader {
            id,
            header_json,
            reply,
        })
    }

    /// Reads a bounded event page together with durable capture lifecycle metadata.
    ///
    /// # Errors
    /// Returns a queue, worker, missing-capture, or SQLite error.
    pub fn live_capture(
        &self,
        id: LiveCaptureId,
        limit: QueryLimit,
    ) -> Result<LiveCaptureSnapshot, StorageError> {
        self.live_capture_page(id, None, limit)
    }

    /// Reads one bounded live-capture event page after an optional sequence cursor.
    ///
    /// The snapshot's `next_sequence` is the total number of admitted events, so a caller can
    /// determine whether another page exists without loading the entire capture.
    ///
    /// # Errors
    /// Returns a queue, worker, missing-capture, or SQLite error.
    pub fn live_capture_page(
        &self,
        id: LiveCaptureId,
        after_sequence: Option<u64>,
        limit: QueryLimit,
    ) -> Result<LiveCaptureSnapshot, StorageError> {
        let after_sequence = after_sequence
            .map(i64::try_from)
            .transpose()
            .map_err(|_| StorageError::LiveCaptureInputInvalid("sequence range"))?;
        self.request(|reply| Command::ReadLiveCapture {
            id,
            after_sequence,
            limit,
            reply,
        })
    }
}

fn validate_timestamp(value: u64) -> Result<(), StorageError> {
    if i64::try_from(value).is_ok() {
        Ok(())
    } else {
        Err(StorageError::LiveCaptureInputInvalid("timestamp range"))
    }
}

pub(super) fn begin(
    connection: &Connection,
    id: LiveCaptureId,
    header_json: &[u8],
    started_at_ms: u64,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO live_capture_sessions
         (capture_id, state, integrity, header_json, started_at_ms, stored_bytes)
         VALUES (?1, 'active', 'complete', ?2, ?3, ?4)",
        params![
            id.as_string(),
            header_json,
            started_at_ms,
            header_json.len()
        ],
    )?;
    Ok(())
}

pub(super) fn append(
    connection: &mut Connection,
    id: LiveCaptureId,
    kind: LiveCaptureEventKind,
    receipt_monotonic_ms: u64,
    source_monotonic_offset_ms: Option<i64>,
    source_wall_clock_unix_ms: Option<u64>,
    payload: &[u8],
) -> Result<u64, StorageError> {
    let transaction = connection.transaction()?;
    let session: Option<(String, i64, i64)> = transaction
        .query_row(
            "SELECT state, next_sequence, stored_bytes FROM live_capture_sessions WHERE capture_id = ?1",
            [id.as_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((state, sequence, stored_bytes)) = session else {
        return Err(StorageError::NotFound);
    };
    if state != LiveCaptureState::Active.as_str() {
        return Err(StorageError::LiveCaptureNotActive);
    }
    let sequence = u64::try_from(sequence)
        .map_err(|_| StorageError::LiveCaptureInputInvalid("sequence range"))?;
    let stored_bytes = u64::try_from(stored_bytes)
        .map_err(|_| StorageError::LiveCaptureInputInvalid("stored byte count"))?;
    let updated_bytes = stored_bytes
        .checked_add(payload.len() as u64)
        .ok_or(StorageError::LiveCaptureLimitExceeded)?;
    if updated_bytes > LIVE_CAPTURE_TOTAL_LIMIT_BYTES {
        return Err(StorageError::LiveCaptureLimitExceeded);
    }
    let next_sequence = sequence
        .checked_add(1)
        .ok_or(StorageError::LiveCaptureLimitExceeded)?;
    let changed = transaction.execute(
        "UPDATE live_capture_sessions SET next_sequence = ?1, stored_bytes = ?2
         WHERE capture_id = ?3 AND state = 'active' AND next_sequence = ?4",
        params![next_sequence, updated_bytes, id.as_string(), sequence],
    )?;
    if changed != 1 {
        return Err(StorageError::LiveCaptureNotActive);
    }
    transaction.execute(
        "INSERT INTO live_capture_events
         (capture_id, sequence, event_kind, receipt_monotonic_ms,
          source_monotonic_offset_ms, source_wall_clock_unix_ms, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id.as_string(),
            sequence,
            kind.as_str(),
            receipt_monotonic_ms,
            source_monotonic_offset_ms,
            source_wall_clock_unix_ms,
            payload
        ],
    )?;
    transaction.commit()?;
    Ok(sequence)
}

pub(super) fn finish(
    connection: &Connection,
    id: LiveCaptureId,
    finished_at_ms: u64,
    integrity: LiveCaptureIntegrity,
) -> Result<(), StorageError> {
    let current: Option<(String, Option<u64>, String, u64)> = connection
        .query_row(
            "SELECT state, finished_at_ms, integrity, dropped_messages
             FROM live_capture_sessions WHERE capture_id = ?1",
            [id.as_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((state, existing_finished_at_ms, existing_integrity, existing_dropped_messages)) =
        current
    else {
        return Err(StorageError::NotFound);
    };
    if state == LiveCaptureState::Finished.as_str()
        && existing_finished_at_ms == Some(finished_at_ms)
        && existing_integrity == integrity.as_str()
        && existing_dropped_messages == integrity.dropped_messages()
    {
        return Ok(());
    }
    if state != LiveCaptureState::Active.as_str() {
        return Err(StorageError::LiveCaptureNotActive);
    }
    let transaction = connection.unchecked_transaction()?;
    let changed = transaction.execute(
        "UPDATE live_capture_sessions
         SET state = 'finished', finished_at_ms = ?1, integrity = ?2, dropped_messages = ?3
         WHERE capture_id = ?4 AND state = 'active' AND started_at_ms <= ?1",
        params![
            finished_at_ms,
            integrity.as_str(),
            integrity.dropped_messages(),
            id.as_string()
        ],
    )?;
    if changed != 1 {
        return Err(StorageError::LiveCaptureInputInvalid(
            "finish precedes capture start",
        ));
    }
    transaction.commit()?;
    Ok(())
}

pub(super) fn update_header(
    connection: &mut Connection,
    id: LiveCaptureId,
    header_json: &[u8],
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    let previous_size: Option<i64> = transaction
        .query_row(
            "SELECT length(header_json) FROM live_capture_sessions
             WHERE capture_id = ?1 AND state = 'active'",
            [id.as_string()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(previous_size) = previous_size else {
        return Err(StorageError::LiveCaptureNotActive);
    };
    let stored_bytes: i64 = transaction.query_row(
        "SELECT stored_bytes FROM live_capture_sessions WHERE capture_id = ?1",
        [id.as_string()],
        |row| row.get(0),
    )?;
    let next_size = u64::try_from(stored_bytes)
        .ok()
        .and_then(|stored| {
            stored
                .checked_sub(u64::try_from(previous_size).ok()?)
                .and_then(|without_header| without_header.checked_add(header_json.len() as u64))
        })
        .ok_or(StorageError::LiveCaptureLimitExceeded)?;
    if next_size > LIVE_CAPTURE_TOTAL_LIMIT_BYTES {
        return Err(StorageError::LiveCaptureLimitExceeded);
    }
    transaction.execute(
        "UPDATE live_capture_sessions SET header_json = ?1, stored_bytes = ?2
         WHERE capture_id = ?3 AND state = 'active'",
        params![header_json, next_size, id.as_string()],
    )?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn read(
    connection: &Connection,
    id: LiveCaptureId,
    after_sequence: Option<i64>,
    limit: QueryLimit,
) -> Result<LiveCaptureSnapshot, StorageError> {
    let header: Option<LiveCaptureHeaderRow> = connection
        .query_row(
            "SELECT state, integrity, dropped_messages, header_json, started_at_ms,
                    finished_at_ms, next_sequence
             FROM live_capture_sessions WHERE capture_id = ?1",
            [id.as_string()],
            |row| {
                Ok(LiveCaptureHeaderRow {
                    state: row.get(0)?,
                    integrity: row.get(1)?,
                    dropped_messages: row.get(2)?,
                    header_json: row.get(3)?,
                    started_at_ms: row.get(4)?,
                    finished_at_ms: row.get(5)?,
                    next_sequence: row.get(6)?,
                })
            },
        )
        .optional()?;
    let Some(header) = header else {
        return Err(StorageError::NotFound);
    };
    let mut statement = connection.prepare(
        "SELECT sequence, event_kind, receipt_monotonic_ms, source_monotonic_offset_ms,
                source_wall_clock_unix_ms, payload
         FROM live_capture_events
         WHERE capture_id = ?1 AND sequence > COALESCE(?2, -1)
         ORDER BY sequence LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![id.as_string(), after_sequence, limit.get()],
        |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<u64>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        },
    )?;
    let events = rows
        .map(|row| {
            let (
                sequence,
                kind,
                receipt_monotonic_ms,
                source_monotonic_offset_ms,
                source_wall_clock_unix_ms,
                payload,
            ) = row?;
            Ok(LiveCaptureEvent {
                sequence,
                kind: LiveCaptureEventKind::parse(&kind)?,
                receipt_monotonic_ms,
                source_monotonic_offset_ms,
                source_wall_clock_unix_ms,
                payload,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(LiveCaptureSnapshot {
        id,
        state: LiveCaptureState::parse(&header.state)?,
        integrity: LiveCaptureIntegrity::parse(&header.integrity, header.dropped_messages)?,
        header_json: header.header_json,
        started_at_ms: header.started_at_ms,
        finished_at_ms: header.finished_at_ms,
        next_sequence: header.next_sequence,
        events,
    })
}

struct LiveCaptureHeaderRow {
    state: String,
    integrity: String,
    dropped_messages: u64,
    header_json: Vec<u8>,
    started_at_ms: u64,
    finished_at_ms: Option<u64>,
    next_sequence: u64,
}

pub(super) fn interrupt_active(connection: &Connection) -> Result<(), StorageError> {
    connection.execute(
        "UPDATE live_capture_sessions SET state = 'interrupted', integrity = 'unknown'
         WHERE state = 'active'",
        [],
    )?;
    Ok(())
}
