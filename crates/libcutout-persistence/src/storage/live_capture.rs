//! Incremental, bounded live-capture event persistence in the canonical database worker.

use super::{Command, QueryLimit, RideDatabase, StorageError};
use cutout_core::{PevcapPhoneLocation, PevcapPhoneLocationError};
use cutout_ride_maps::LocationAdmission;
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

impl std::fmt::Display for LiveCaptureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
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
    /// Structured Core Location facts when this event is a decoded location observation.
    pub location: Option<LiveCaptureLocationObservation>,
}

/// Whether the source location fields were validated before route admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveCaptureLocationValidation {
    /// The source observation was retained without attempting semantic validation.
    NotEvaluated,
    /// Required location fields passed Rust validation.
    Valid,
    /// Required location fields failed Rust validation; the raw payload remains authoritative.
    Rejected(PevcapPhoneLocationError),
}

impl LiveCaptureLocationValidation {
    const fn as_db(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::NotEvaluated => ("not_evaluated", None),
            Self::Valid => ("valid", None),
            Self::Rejected(reason) => ("rejected", Some(validation_reason_as_db(reason))),
        }
    }

    fn from_db(state: &str, reason: Option<&str>) -> Result<Self, StorageError> {
        match (state, reason) {
            ("not_evaluated", None) => Ok(Self::NotEvaluated),
            ("valid", None) => Ok(Self::Valid),
            ("rejected", Some(reason)) => validation_reason_from_db(reason).map(Self::Rejected),
            _ => Err(StorageError::InvalidStoredValue {
                field: "live capture location validation",
                value: format!("{state}:{reason:?}"),
            }),
        }
    }
}

const fn validation_reason_as_db(reason: PevcapPhoneLocationError) -> &'static str {
    match reason {
        PevcapPhoneLocationError::MissingWallClockTimestamp => "missing_wall_clock_timestamp",
        PevcapPhoneLocationError::InvalidLatitude => "invalid_latitude",
        PevcapPhoneLocationError::InvalidLongitude => "invalid_longitude",
        PevcapPhoneLocationError::InvalidAltitude => "invalid_altitude",
    }
}

fn validation_reason_from_db(value: &str) -> Result<PevcapPhoneLocationError, StorageError> {
    match value {
        "missing_wall_clock_timestamp" => Ok(PevcapPhoneLocationError::MissingWallClockTimestamp),
        "invalid_latitude" => Ok(PevcapPhoneLocationError::InvalidLatitude),
        "invalid_longitude" => Ok(PevcapPhoneLocationError::InvalidLongitude),
        "invalid_altitude" => Ok(PevcapPhoneLocationError::InvalidAltitude),
        _ => Err(StorageError::InvalidStoredValue {
            field: "live capture location validation reason",
            value: value.to_owned(),
        }),
    }
}

/// Whether and how a validated location was admitted to the canonical route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveCaptureLocationAdmission {
    /// Route policy has not evaluated this observation.
    NotEvaluated,
    /// Route policy returned a decision.
    Evaluated(LocationAdmission),
}

impl LiveCaptureLocationAdmission {
    const fn as_db(self) -> &'static str {
        match self {
            Self::NotEvaluated => "not_evaluated",
            Self::Evaluated(LocationAdmission::Accepted) => "accepted",
            Self::Evaluated(LocationAdmission::Duplicate) => "duplicate",
            Self::Evaluated(LocationAdmission::OutOfOrder) => "out_of_order",
            Self::Evaluated(LocationAdmission::AccuracyTooLow) => "accuracy_too_low",
            Self::Evaluated(LocationAdmission::UnrealisticJump) => "unrealistic_jump",
        }
    }

    fn from_db(value: &str) -> Result<Self, StorageError> {
        match value {
            "not_evaluated" => Ok(Self::NotEvaluated),
            "accepted" => Ok(Self::Evaluated(LocationAdmission::Accepted)),
            "duplicate" => Ok(Self::Evaluated(LocationAdmission::Duplicate)),
            "out_of_order" => Ok(Self::Evaluated(LocationAdmission::OutOfOrder)),
            "accuracy_too_low" => Ok(Self::Evaluated(LocationAdmission::AccuracyTooLow)),
            "unrealistic_jump" => Ok(Self::Evaluated(LocationAdmission::UnrealisticJump)),
            _ => Err(StorageError::InvalidStoredValue {
                field: "live capture location admission",
                value: value.to_owned(),
            }),
        }
    }
}

/// Full-precision typed location facts linked to one ordered raw capture event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCaptureLocationObservation {
    /// Source-reported coordinates, altitude, and optional measurement values.
    pub location: PevcapPhoneLocation,
    /// Whether the platform marked the sample as simulated, when available.
    pub simulated: Option<bool>,
    /// Whether the platform marked the sample as accessory-produced, when available.
    pub produced_by_accessory: Option<bool>,
    /// Rust validation result, kept separate from route admission.
    pub validation: LiveCaptureLocationValidation,
    /// Rust route decision, or an explicit not-yet-evaluated state.
    pub admission: LiveCaptureLocationAdmission,
}

impl LiveCaptureLocationObservation {
    fn source_wall_clock_unix_ms(&self) -> Option<u64> {
        (self.location.wall_clock_unix_ms != 0).then_some(self.location.wall_clock_unix_ms)
    }
}

pub(super) struct LiveCaptureEventAppend<'a> {
    pub(super) id: LiveCaptureId,
    pub(super) kind: LiveCaptureEventKind,
    pub(super) receipt_monotonic_ms: u64,
    pub(super) source_monotonic_offset_ms: Option<i64>,
    pub(super) source_wall_clock_unix_ms: Option<u64>,
    pub(super) payload: &'a [u8],
    pub(super) location: Option<&'a LiveCaptureLocationObservation>,
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

/// Structured location values remain separate from, and foreign-keyed to, raw event bytes.
pub(super) const LOCATION_SCHEMA: &str = "
    CREATE TABLE live_capture_location_observations (
        capture_id TEXT NOT NULL,
        sequence INTEGER NOT NULL CHECK (sequence >= 0),
        latitude_degrees REAL NOT NULL,
        longitude_degrees REAL NOT NULL,
        altitude_meters REAL NOT NULL,
        horizontal_accuracy_meters REAL,
        vertical_accuracy_meters REAL,
        speed_meters_per_second REAL,
        speed_accuracy_meters_per_second REAL,
        course_degrees REAL,
        course_accuracy_degrees REAL,
        simulated INTEGER CHECK (simulated IS NULL OR simulated IN (0, 1)),
        produced_by_accessory INTEGER
            CHECK (produced_by_accessory IS NULL OR produced_by_accessory IN (0, 1)),
        validation_state TEXT NOT NULL
            CHECK (validation_state IN ('not_evaluated', 'valid', 'rejected')),
        validation_reason TEXT CHECK (validation_reason IS NULL OR validation_reason IN
            ('missing_wall_clock_timestamp', 'invalid_latitude', 'invalid_longitude', 'invalid_altitude')),
        route_admission TEXT NOT NULL CHECK (route_admission IN
            ('not_evaluated', 'accepted', 'duplicate', 'out_of_order',
             'accuracy_too_low', 'unrealistic_jump')),
        CHECK ((validation_state = 'rejected') = (validation_reason IS NOT NULL)),
        PRIMARY KEY (capture_id, sequence),
        FOREIGN KEY (capture_id, sequence)
            REFERENCES live_capture_events(capture_id, sequence) ON DELETE CASCADE
    ) WITHOUT ROWID;
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
            location: None,
            reply,
        })
    }

    /// Appends one raw location event and its typed facts in the same ordered transaction.
    ///
    /// A rejected observation is retained with its validation reason; route admission is stored
    /// independently so raw evidence is never erased by projection policy.
    ///
    /// # Errors
    /// Returns a queue, worker, input-bound, inactive-session, validation, or SQLite error.
    pub fn append_live_capture_location(
        &self,
        id: LiveCaptureId,
        receipt_monotonic_ms: u64,
        source_monotonic_offset_ms: Option<i64>,
        observation: LiveCaptureLocationObservation,
        payload: Vec<u8>,
    ) -> Result<u64, StorageError> {
        validate_timestamp(receipt_monotonic_ms)?;
        let validation_result = observation.location.canonical();
        match (observation.validation, validation_result) {
            (LiveCaptureLocationValidation::Valid, Ok(_))
            | (LiveCaptureLocationValidation::NotEvaluated, _) => {}
            (LiveCaptureLocationValidation::Rejected(expected), Err(actual))
                if expected == actual => {}
            _ => {
                return Err(StorageError::LiveCaptureInputInvalid(
                    "location validation state",
                ));
            }
        }
        let validation_is_incomplete = match observation.validation {
            LiveCaptureLocationValidation::Valid => false,
            LiveCaptureLocationValidation::NotEvaluated
            | LiveCaptureLocationValidation::Rejected(_) => true,
        };
        let admission_was_evaluated = match observation.admission {
            LiveCaptureLocationAdmission::NotEvaluated => false,
            LiveCaptureLocationAdmission::Evaluated(_) => true,
        };
        if validation_is_incomplete && admission_was_evaluated {
            return Err(StorageError::LiveCaptureInputInvalid(
                "location admission before validation",
            ));
        }
        if let Some(source_wall_clock_unix_ms) = observation.source_wall_clock_unix_ms() {
            validate_timestamp(source_wall_clock_unix_ms)?;
        }
        if payload.is_empty() || payload.len() > LIVE_CAPTURE_EVENT_LIMIT_BYTES {
            return Err(StorageError::LiveCaptureInputInvalid("event payload size"));
        }
        self.request(|reply| Command::AppendLiveCaptureEvent {
            id,
            kind: LiveCaptureEventKind::Location,
            receipt_monotonic_ms,
            source_monotonic_offset_ms,
            source_wall_clock_unix_ms: observation.source_wall_clock_unix_ms(),
            payload,
            location: Some(observation),
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
    event: &LiveCaptureEventAppend<'_>,
) -> Result<u64, StorageError> {
    let LiveCaptureEventAppend {
        id,
        kind,
        receipt_monotonic_ms,
        source_monotonic_offset_ms,
        source_wall_clock_unix_ms,
        payload,
        location,
    } = event;
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
    if let Some(location) = location {
        insert_location(&transaction, *id, sequence, location)?;
    }
    transaction.commit()?;
    Ok(sequence)
}

fn insert_location(
    transaction: &rusqlite::Transaction<'_>,
    id: LiveCaptureId,
    sequence: u64,
    observation: &LiveCaptureLocationObservation,
) -> Result<(), StorageError> {
    let (validation_state, validation_reason) = observation.validation.as_db();
    transaction.execute(
        "INSERT INTO live_capture_location_observations
         (capture_id, sequence, latitude_degrees, longitude_degrees, altitude_meters,
          horizontal_accuracy_meters, vertical_accuracy_meters, speed_meters_per_second,
          speed_accuracy_meters_per_second, course_degrees, course_accuracy_degrees,
          simulated, produced_by_accessory, validation_state, validation_reason, route_admission)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            id.as_string(),
            sequence,
            observation.location.latitude_degrees,
            observation.location.longitude_degrees,
            observation.location.altitude_meters,
            observation.location.horizontal_accuracy_meters,
            observation.location.vertical_accuracy_meters,
            observation.location.speed_meters_per_second,
            observation.location.speed_accuracy_meters_per_second,
            observation.location.course_degrees,
            observation.location.course_accuracy_degrees,
            observation.simulated.map(i64::from),
            observation.produced_by_accessory.map(i64::from),
            validation_state,
            validation_reason,
            observation.admission.as_db(),
        ],
    )?;
    Ok(())
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

struct LiveCaptureEventRow {
    sequence: u64,
    kind: String,
    receipt_monotonic_ms: u64,
    source_monotonic_offset_ms: Option<i64>,
    source_wall_clock_unix_ms: Option<u64>,
    payload: Vec<u8>,
    latitude_degrees: Option<f64>,
    longitude_degrees: Option<f64>,
    altitude_meters: Option<f64>,
    horizontal_accuracy_meters: Option<f64>,
    vertical_accuracy_meters: Option<f64>,
    speed_meters_per_second: Option<f64>,
    speed_accuracy_meters_per_second: Option<f64>,
    course_degrees: Option<f64>,
    course_accuracy_degrees: Option<f64>,
    simulated: Option<i64>,
    produced_by_accessory: Option<i64>,
    validation_state: Option<String>,
    validation_reason: Option<String>,
    route_admission: Option<String>,
}

impl LiveCaptureEventRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            sequence: row.get(0)?,
            kind: row.get(1)?,
            receipt_monotonic_ms: row.get(2)?,
            source_monotonic_offset_ms: row.get(3)?,
            source_wall_clock_unix_ms: row.get(4)?,
            payload: row.get(5)?,
            latitude_degrees: row.get(6)?,
            longitude_degrees: row.get(7)?,
            altitude_meters: row.get(8)?,
            horizontal_accuracy_meters: row.get(9)?,
            vertical_accuracy_meters: row.get(10)?,
            speed_meters_per_second: row.get(11)?,
            speed_accuracy_meters_per_second: row.get(12)?,
            course_degrees: row.get(13)?,
            course_accuracy_degrees: row.get(14)?,
            simulated: row.get(15)?,
            produced_by_accessory: row.get(16)?,
            validation_state: row.get(17)?,
            validation_reason: row.get(18)?,
            route_admission: row.get(19)?,
        })
    }

    fn into_event(self) -> Result<LiveCaptureEvent, StorageError> {
        let Self {
            sequence,
            kind,
            receipt_monotonic_ms,
            source_monotonic_offset_ms,
            source_wall_clock_unix_ms,
            payload,
            latitude_degrees,
            longitude_degrees,
            altitude_meters,
            horizontal_accuracy_meters,
            vertical_accuracy_meters,
            speed_meters_per_second,
            speed_accuracy_meters_per_second,
            course_degrees,
            course_accuracy_degrees,
            simulated,
            produced_by_accessory,
            validation_state,
            validation_reason,
            route_admission,
        } = self;
        let location = read_location_observation(
            sequence,
            source_wall_clock_unix_ms,
            LiveCaptureLocationRow {
                latitude_degrees,
                longitude_degrees,
                altitude_meters,
                horizontal_accuracy_meters,
                vertical_accuracy_meters,
                speed_meters_per_second,
                speed_accuracy_meters_per_second,
                course_degrees,
                course_accuracy_degrees,
                simulated,
                produced_by_accessory,
                validation_state,
                validation_reason,
                route_admission,
            },
        )?;
        Ok(LiveCaptureEvent {
            sequence,
            kind: LiveCaptureEventKind::parse(&kind)?,
            receipt_monotonic_ms,
            source_monotonic_offset_ms,
            source_wall_clock_unix_ms,
            payload,
            location,
        })
    }
}

struct LiveCaptureLocationRow {
    latitude_degrees: Option<f64>,
    longitude_degrees: Option<f64>,
    altitude_meters: Option<f64>,
    horizontal_accuracy_meters: Option<f64>,
    vertical_accuracy_meters: Option<f64>,
    speed_meters_per_second: Option<f64>,
    speed_accuracy_meters_per_second: Option<f64>,
    course_degrees: Option<f64>,
    course_accuracy_degrees: Option<f64>,
    simulated: Option<i64>,
    produced_by_accessory: Option<i64>,
    validation_state: Option<String>,
    validation_reason: Option<String>,
    route_admission: Option<String>,
}

fn read_location_observation(
    sequence: u64,
    source_wall_clock_unix_ms: Option<u64>,
    row: LiveCaptureLocationRow,
) -> Result<Option<LiveCaptureLocationObservation>, StorageError> {
    let LiveCaptureLocationRow {
        latitude_degrees,
        longitude_degrees,
        altitude_meters,
        horizontal_accuracy_meters,
        vertical_accuracy_meters,
        speed_meters_per_second,
        speed_accuracy_meters_per_second,
        course_degrees,
        course_accuracy_degrees,
        simulated,
        produced_by_accessory,
        validation_state,
        validation_reason,
        route_admission,
    } = row;
    let location = match (latitude_degrees, longitude_degrees, altitude_meters) {
        (None, None, None) => {
            if horizontal_accuracy_meters.is_some()
                || vertical_accuracy_meters.is_some()
                || speed_meters_per_second.is_some()
                || speed_accuracy_meters_per_second.is_some()
                || course_degrees.is_some()
                || course_accuracy_degrees.is_some()
                || simulated.is_some()
                || produced_by_accessory.is_some()
                || validation_state.is_some()
                || validation_reason.is_some()
                || route_admission.is_some()
            {
                return Err(StorageError::InvalidStoredValue {
                    field: "live capture location row",
                    value: format!("partial observation at sequence {sequence}"),
                });
            }
            None
        }
        (Some(latitude_degrees), Some(longitude_degrees), Some(altitude_meters)) => {
            let simulated = parse_optional_bool(simulated, "location simulated flag")?;
            let produced_by_accessory =
                parse_optional_bool(produced_by_accessory, "location accessory flag")?;
            let validation_state =
                validation_state.ok_or_else(|| StorageError::InvalidStoredValue {
                    field: "live capture location validation",
                    value: "missing state".to_owned(),
                })?;
            let route_admission =
                route_admission.ok_or_else(|| StorageError::InvalidStoredValue {
                    field: "live capture location admission",
                    value: "missing state".to_owned(),
                })?;
            Some(LiveCaptureLocationObservation {
                location: PevcapPhoneLocation {
                    wall_clock_unix_ms: source_wall_clock_unix_ms.unwrap_or_default(),
                    latitude_degrees,
                    longitude_degrees,
                    altitude_meters,
                    horizontal_accuracy_meters,
                    vertical_accuracy_meters,
                    speed_meters_per_second,
                    speed_accuracy_meters_per_second,
                    course_degrees,
                    course_accuracy_degrees,
                },
                simulated,
                produced_by_accessory,
                validation: LiveCaptureLocationValidation::from_db(
                    &validation_state,
                    validation_reason.as_deref(),
                )?,
                admission: LiveCaptureLocationAdmission::from_db(&route_admission)?,
            })
        }
        _ => {
            return Err(StorageError::InvalidStoredValue {
                field: "live capture location row",
                value: format!("partial coordinates at sequence {sequence}"),
            });
        }
    };
    Ok(location)
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
        "SELECT event.sequence, event.event_kind, event.receipt_monotonic_ms,
                event.source_monotonic_offset_ms, event.source_wall_clock_unix_ms, event.payload,
                location.latitude_degrees, location.longitude_degrees, location.altitude_meters,
                location.horizontal_accuracy_meters, location.vertical_accuracy_meters,
                location.speed_meters_per_second, location.speed_accuracy_meters_per_second,
                location.course_degrees, location.course_accuracy_degrees,
                location.simulated, location.produced_by_accessory,
                location.validation_state, location.validation_reason, location.route_admission
         FROM live_capture_events AS event
         LEFT JOIN live_capture_location_observations AS location
           ON location.capture_id = event.capture_id AND location.sequence = event.sequence
         WHERE event.capture_id = ?1 AND event.sequence > COALESCE(?2, -1)
         ORDER BY event.sequence LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![id.as_string(), after_sequence, limit.get()],
        LiveCaptureEventRow::from_row,
    )?;
    let events = rows
        .map(|row| row?.into_event())
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

fn parse_optional_bool(
    value: Option<i64>,
    field: &'static str,
) -> Result<Option<bool>, StorageError> {
    value
        .map(|value| match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StorageError::InvalidStoredValue {
                field,
                value: value.to_string(),
            }),
        })
        .transpose()
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
