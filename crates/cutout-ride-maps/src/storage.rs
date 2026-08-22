use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    LocationSample, MonotonicMilliseconds, RideRecording, RideState, RoutePoint, RoutePointBatch,
    VehicleIdentity,
};

const SCHEMA: &str = r"
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS ride_map_ride (
    ride_id TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('recording', 'paused', 'stopped', 'saved', 'discarded')),
    point_count INTEGER NOT NULL CHECK (point_count >= 0),
    distance_meters REAL NOT NULL CHECK (distance_meters >= 0.0),
    duration_milliseconds INTEGER NOT NULL CHECK (duration_milliseconds >= 0),
    associated_vehicle TEXT
);

CREATE TABLE IF NOT EXISTS ride_map_point (
    ride_id TEXT NOT NULL REFERENCES ride_map_ride(ride_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    segment_id INTEGER NOT NULL CHECK (segment_id >= 0),
    wall_clock_unix_ms INTEGER NOT NULL CHECK (wall_clock_unix_ms > 0),
    monotonic_ms INTEGER NOT NULL,
    latitude_degrees REAL NOT NULL CHECK (latitude_degrees >= -90.0 AND latitude_degrees <= 90.0),
    longitude_degrees REAL NOT NULL CHECK (longitude_degrees >= -180.0 AND longitude_degrees <= 180.0),
    horizontal_accuracy_meters REAL NOT NULL CHECK (horizontal_accuracy_meters >= 0.0),
    PRIMARY KEY (ride_id, sequence)
);

CREATE INDEX IF NOT EXISTS ride_map_point_cursor
    ON ride_map_point (ride_id, sequence);

PRAGMA user_version = 1;
";

const SCHEMA_VERSION: i64 = 1;

/// A bounded, Rust-owned history summary.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredRideSummary {
    ride_id: Uuid,
    state: RideState,
    point_count: u64,
    distance_meters: f64,
    duration_milliseconds: u64,
    associated_vehicle: Option<String>,
}

impl StoredRideSummary {
    /// Returns the stable ride identity.
    #[must_use]
    pub const fn ride_id(&self) -> Uuid {
        self.ride_id
    }

    /// Returns the persisted lifecycle state.
    #[must_use]
    pub const fn state(&self) -> RideState {
        self.state
    }

    /// Returns the number of admitted points.
    #[must_use]
    pub const fn point_count(&self) -> u64 {
        self.point_count
    }

    /// Returns the cumulative distance in meters.
    #[must_use]
    pub const fn distance_meters(&self) -> f64 {
        self.distance_meters
    }

    /// Returns the elapsed monotonic duration in milliseconds.
    #[must_use]
    pub const fn duration_milliseconds(&self) -> u64 {
        self.duration_milliseconds
    }

    /// Returns the associated vehicle identity, when confirmed.
    #[must_use]
    pub fn associated_vehicle(&self) -> Option<&str> {
        self.associated_vehicle.as_deref()
    }
}

/// Errors returned by the local ride-map store.
#[derive(Debug, Error)]
pub enum RideMapStoreError {
    /// `SQLite` rejected an operation.
    #[error("ride-map sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored UUID was malformed.
    #[error("stored ride identity is invalid: {0}")]
    InvalidRideId(String),
    /// A stored lifecycle value was not one of the typed states.
    #[error("stored ride state is invalid: {0}")]
    InvalidState(String),
    /// A stored route point failed the domain constructors.
    #[error("stored route point is invalid: {0}")]
    InvalidPoint(String),
    /// A domain value could not be represented by `SQLite`'s signed integers.
    #[error("ride-map value is out of SQLite range")]
    ValueOutOfRange,
    /// The database was written by a newer unsupported schema.
    #[error("unsupported ride-map schema version: {0}")]
    UnsupportedSchema(i64),
    /// The storage worker stopped before completing a request.
    #[error("ride-map storage worker stopped: {0}")]
    Worker(String),
}

#[derive(Clone, Debug)]
struct RideMetadata {
    ride_id: Uuid,
    state: RideState,
    point_count: u64,
    distance_meters: f64,
    duration_milliseconds: u64,
    associated_vehicle: Option<String>,
}

impl RideMetadata {
    fn from_recording(recording: &RideRecording) -> Self {
        Self {
            ride_id: recording.ride_id(),
            state: recording.state(),
            point_count: recording.summary().point_count(),
            distance_meters: recording.summary().distance_meters(),
            duration_milliseconds: recording.summary().duration_milliseconds(),
            associated_vehicle: recording
                .associated_vehicle()
                .map(VehicleIdentity::as_str)
                .map(str::to_owned),
        }
    }
}

/// Single-owner `SQLite` persistence for canonical ride records and route points.
#[derive(Debug)]
pub struct RideMapStore {
    connection: Connection,
}

/// Rust-owned single-writer service for production mobile storage.
#[derive(Clone, Debug)]
pub struct RideMapDatabase {
    inner: Arc<RideMapDatabaseInner>,
}

#[derive(Debug)]
struct RideMapDatabaseInner {
    commands: SyncSender<StoreCommand>,
    service_id: u64,
}

enum StoreCommand {
    Metadata {
        metadata: RideMetadata,
        reply: mpsc::Sender<Result<(), RideMapStoreError>>,
    },
    AppendPoint {
        metadata: RideMetadata,
        point: RoutePoint,
        reply: mpsc::Sender<Result<(), RideMapStoreError>>,
    },
    Save {
        recording: Box<RideRecording>,
        reply: mpsc::Sender<Result<(), RideMapStoreError>>,
    },
    Delete {
        ride_id: Uuid,
        reply: mpsc::Sender<Result<(), RideMapStoreError>>,
    },
    List {
        limit: usize,
        reply: mpsc::Sender<Result<Vec<StoredRideSummary>, RideMapStoreError>>,
    },
    Points {
        ride_id: Uuid,
        after_sequence: u64,
        limit: usize,
        reply: mpsc::Sender<Result<RoutePointBatch, RideMapStoreError>>,
    },
    Recover {
        reply: mpsc::Sender<Result<Option<RideRecording>, RideMapStoreError>>,
    },
    Shutdown,
}

#[derive(Debug)]
struct RegistryEntry {
    path: PathBuf,
    database: Weak<RideMapDatabaseInner>,
}

static DATABASE_REGISTRY: OnceLock<Mutex<Option<RegistryEntry>>> = OnceLock::new();
static NEXT_SERVICE_ID: AtomicU64 = AtomicU64::new(1);

/// Error returned when a second production database path is opened in one process.
#[derive(Debug, Error)]
pub enum RideMapDatabaseOpenError {
    /// The database path could not be normalized.
    #[error("ride-map database path is invalid: {0}")]
    Path(#[from] std::io::Error),
    /// The process already owns a different ride-map database path.
    #[error("ride-map database is already open at another path")]
    AlreadyOpenForDifferentPath,
    /// The worker could not open or initialize the database.
    #[error("ride-map database worker failed to start: {0}")]
    Store(#[from] RideMapStoreError),
}

impl RideMapStore {
    /// Opens a database at a filesystem path and applies the current schema.
    ///
    /// Errors are returned when the file cannot be opened or the schema cannot
    /// be applied.
    ///
    /// # Errors
    ///
    /// Returns `RideMapStoreError` when the file or schema cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RideMapStoreError> {
        let connection = Connection::open(path)?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    /// Opens an isolated in-memory database for tests or replay fixtures.
    ///
    /// # Errors
    ///
    /// Returns `RideMapStoreError` when `SQLite` cannot initialize.
    pub fn open_in_memory() -> Result<Self, RideMapStoreError> {
        let connection = Connection::open_in_memory()?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> Result<(), RideMapStoreError> {
        let version = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
        if version > SCHEMA_VERSION {
            return Err(RideMapStoreError::UnsupportedSchema(version));
        }
        self.connection.execute_batch(SCHEMA)?;
        Ok(())
    }

    /// Atomically replaces one canonical ride and all of its route points.
    ///
    /// The recording has already passed the Rust admission policy; this method
    /// only persists typed values and never accepts raw coordinates.
    ///
    /// # Errors
    ///
    /// Returns `RideMapStoreError` when the transaction cannot commit or a
    /// domain value cannot be represented in `SQLite`.
    pub fn save_recording(&mut self, recording: &RideRecording) -> Result<(), RideMapStoreError> {
        let metadata = RideMetadata::from_recording(recording);
        let transaction = self.connection.transaction()?;
        insert_metadata(&transaction, &metadata)?;
        transaction.execute(
            "DELETE FROM ride_map_point WHERE ride_id = ?1",
            params![metadata.ride_id.to_string()],
        )?;

        let mut insert_point = transaction.prepare(
            "INSERT INTO ride_map_point
                (ride_id, sequence, segment_id, wall_clock_unix_ms, monotonic_ms,
                 latitude_degrees, longitude_degrees, horizontal_accuracy_meters)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for segment in recording.segments() {
            for point in segment.points() {
                let sample = point.sample();
                insert_point.execute(params![
                    metadata.ride_id.to_string(),
                    sqlite_i64(point.sequence())?,
                    sqlite_i64(point.segment_id())?,
                    sqlite_i64(sample.wall_clock_unix_ms())?,
                    sqlite_i64(sample.monotonic_at().as_milliseconds())?,
                    sample.latitude().as_degrees(),
                    sample.longitude().as_degrees(),
                    sample.horizontal_accuracy_meters(),
                ])?;
            }
        }
        drop(insert_point);
        transaction.commit()?;
        Ok(())
    }

    fn save_metadata(&mut self, metadata: &RideMetadata) -> Result<(), RideMapStoreError> {
        let transaction = self.connection.transaction()?;
        insert_metadata(&transaction, metadata)?;
        transaction.commit()?;
        Ok(())
    }

    fn append_point(
        &mut self,
        metadata: &RideMetadata,
        point: RoutePoint,
    ) -> Result<(), RideMapStoreError> {
        let transaction = self.connection.transaction()?;
        insert_metadata(&transaction, metadata)?;
        let sample = point.sample();
        transaction.execute(
            "INSERT INTO ride_map_point
                (ride_id, sequence, segment_id, wall_clock_unix_ms, monotonic_ms,
                 latitude_degrees, longitude_degrees, horizontal_accuracy_meters)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(ride_id, sequence) DO NOTHING",
            params![
                metadata.ride_id.to_string(),
                sqlite_i64(point.sequence())?,
                sqlite_i64(point.segment_id())?,
                sqlite_i64(sample.wall_clock_unix_ms())?,
                sqlite_i64(sample.monotonic_at().as_milliseconds())?,
                sample.latitude().as_degrees(),
                sample.longitude().as_degrees(),
                sample.horizontal_accuracy_meters(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Removes one discarded ride from canonical history.
    ///
    /// # Errors
    ///
    /// Returns `RideMapStoreError` when the delete cannot be committed.
    pub fn delete_recording(&mut self, ride_id: Uuid) -> Result<(), RideMapStoreError> {
        self.connection.execute(
            "DELETE FROM ride_map_ride WHERE ride_id = ?1",
            params![ride_id.to_string()],
        )?;
        Ok(())
    }

    /// Returns a bounded newest-first summary page.
    ///
    /// # Errors
    ///
    /// Returns `RideMapStoreError` for query or typed-decoding failures.
    pub fn list_summaries(
        &self,
        limit: usize,
    ) -> Result<Vec<StoredRideSummary>, RideMapStoreError> {
        let limit = sqlite_i64(limit.min(crate::MAX_POINT_BATCH))?;
        let mut statement = self.connection.prepare(
            "SELECT ride_id, state, point_count, distance_meters,
                    duration_milliseconds, associated_vehicle
             FROM ride_map_ride
             WHERE state <> 'discarded'
             ORDER BY rowid DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], decode_summary)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(RideMapStoreError::from)
    }

    /// Returns a bounded route batch after a sequence cursor.
    ///
    /// # Errors
    ///
    /// Returns `RideMapStoreError` for query or typed-decoding failures.
    pub fn points_after(
        &self,
        ride_id: Uuid,
        after_sequence: u64,
        limit: usize,
    ) -> Result<RoutePointBatch, RideMapStoreError> {
        let limit = limit.min(crate::MAX_POINT_BATCH);
        let query_limit = sqlite_i64(limit.saturating_add(1))?;
        let after_sequence_sql = sqlite_i64(after_sequence)?;
        let mut statement = self.connection.prepare(
            "SELECT sequence, segment_id, wall_clock_unix_ms, monotonic_ms,
                    latitude_degrees, longitude_degrees, horizontal_accuracy_meters
             FROM ride_map_point
             WHERE ride_id = ?1 AND sequence > ?2
             ORDER BY sequence ASC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![ride_id.to_string(), after_sequence_sql, query_limit],
            decode_point,
        )?;
        let mut points = rows.collect::<Result<Vec<_>, _>>()?;
        let has_more = points.len() > limit;
        points.truncate(limit);
        let next_cursor = points
            .last()
            .map_or(after_sequence, |point| point.sequence());
        Ok(RoutePointBatch {
            points,
            next_cursor,
            has_more,
        })
    }

    /// Reconstructs the newest non-terminal ride after an app restart.
    ///
    /// The route is revalidated by the domain constructor before it becomes
    /// available to callers; malformed durable state is never exposed as a
    /// live recording.
    ///
    /// # Errors
    ///
    /// Returns `RideMapStoreError` when metadata, identity, or route points
    /// cannot be decoded into the Rust domain.
    pub fn recover_open_recording(&self) -> Result<Option<RideRecording>, RideMapStoreError> {
        let metadata = self
            .connection
            .query_row(
                "SELECT ride_id, state, point_count, associated_vehicle
                 FROM ride_map_ride
                 WHERE state IN ('recording', 'paused', 'stopped')
                 ORDER BY rowid DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((ride_id, state, point_count, associated_vehicle)) = metadata else {
            return Ok(None);
        };
        let ride_id = Uuid::parse_str(&ride_id)
            .map_err(|error| RideMapStoreError::InvalidRideId(error.to_string()))?;
        let state = parse_state(&state)?;
        let associated_vehicle = associated_vehicle
            .map(VehicleIdentity::new)
            .transpose()
            .map_err(|error| RideMapStoreError::InvalidPoint(error.to_string()))?;
        let mut statement = self.connection.prepare(
            "SELECT sequence, segment_id, wall_clock_unix_ms, monotonic_ms,
                    latitude_degrees, longitude_degrees, horizontal_accuracy_meters
             FROM ride_map_point
             WHERE ride_id = ?1
             ORDER BY sequence ASC",
        )?;
        let points = statement
            .query_map(params![ride_id.to_string()], decode_point)?
            .collect::<Result<Vec<_>, _>>()?;
        let recovered = RideRecording::from_persisted(ride_id, state, associated_vehicle, points)
            .map_err(RideMapStoreError::InvalidPoint)?;
        let stored_point_count = u64::try_from(point_count)
            .map_err(|_| RideMapStoreError::InvalidPoint("negative point count".to_owned()))?;
        if recovered.summary().point_count() != stored_point_count {
            return Err(RideMapStoreError::InvalidPoint(
                "persisted point count does not match route".to_owned(),
            ));
        }
        Ok(Some(recovered))
    }
}

impl RideMapDatabase {
    /// Opens or acquires the process-wide single writer for a database path.
    ///
    /// A `:memory:` database is intentionally isolated for tests and replay
    /// fixtures. Filesystem paths are canonicalized before registry lookup so
    /// aliases cannot create a second same-process writer.
    ///
    /// # Errors
    ///
    /// Returns [`RideMapDatabaseOpenError`] when the path is invalid, another
    /// path already owns the process writer, or the worker cannot initialize.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RideMapDatabaseOpenError> {
        let path = path.as_ref();
        if path == Path::new(":memory:") {
            return Self::spawn(path.to_path_buf()).map_err(RideMapDatabaseOpenError::Store);
        }
        let path = canonical_database_path(path)?;
        let registry = DATABASE_REGISTRY.get_or_init(|| Mutex::new(None));
        let mut entry = registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = entry.as_ref() {
            if let Some(database) = existing.database.upgrade() {
                if existing.path != path {
                    return Err(RideMapDatabaseOpenError::AlreadyOpenForDifferentPath);
                }
                return Ok(Self { inner: database });
            }
        }
        let database = Self::spawn(path.clone()).map_err(RideMapDatabaseOpenError::Store)?;
        *entry = Some(RegistryEntry {
            path,
            database: Arc::downgrade(&database.inner),
        });
        Ok(database)
    }

    /// Returns the stable worker generation used by this handle.
    #[must_use]
    pub fn service_id(&self) -> u64 {
        self.inner.service_id
    }

    /// Saves one validated recording through the single worker.
    ///
    /// # Errors
    ///
    /// Returns storage or worker-shutdown errors.
    pub fn save_recording(&self, recording: &RideRecording) -> Result<(), RideMapStoreError> {
        let (reply, receiver) = mpsc::channel();
        self.send(StoreCommand::Save {
            recording: Box::new(recording.clone()),
            reply,
        })?;
        receiver.recv().map_err(worker_closed)?
    }

    /// Updates one ride's compact metadata without copying its route.
    ///
    /// # Errors
    ///
    /// Returns storage or worker-shutdown errors.
    pub fn save_metadata(&self, recording: &RideRecording) -> Result<(), RideMapStoreError> {
        let (reply, receiver) = mpsc::channel();
        self.send(StoreCommand::Metadata {
            metadata: RideMetadata::from_recording(recording),
            reply,
        })?;
        receiver.recv().map_err(worker_closed)?
    }

    /// Appends one admitted point and updates its compact metadata.
    ///
    /// The worker receives only the new point and metadata, never a full route
    /// clone, so location callbacks do not perform full-route persistence.
    ///
    /// # Errors
    ///
    /// Returns storage or worker-shutdown errors.
    pub fn append_point(
        &self,
        recording: &RideRecording,
        point: RoutePoint,
    ) -> Result<(), RideMapStoreError> {
        let (reply, receiver) = mpsc::channel();
        self.send(StoreCommand::AppendPoint {
            metadata: RideMetadata::from_recording(recording),
            point,
            reply,
        })?;
        receiver.recv().map_err(worker_closed)?
    }

    /// Deletes one discarded recording through the single worker.
    ///
    /// # Errors
    ///
    /// Returns storage or worker-shutdown errors.
    pub fn delete_recording(&self, ride_id: Uuid) -> Result<(), RideMapStoreError> {
        let (reply, receiver) = mpsc::channel();
        self.send(StoreCommand::Delete { ride_id, reply })?;
        receiver.recv().map_err(worker_closed)?
    }

    /// Queries bounded history summaries through the single worker.
    ///
    /// # Errors
    ///
    /// Returns storage or worker-shutdown errors.
    pub fn list_summaries(
        &self,
        limit: usize,
    ) -> Result<Vec<StoredRideSummary>, RideMapStoreError> {
        let (reply, receiver) = mpsc::channel();
        self.send(StoreCommand::List { limit, reply })?;
        receiver.recv().map_err(worker_closed)?
    }

    /// Queries a bounded route batch through the single worker.
    ///
    /// # Errors
    ///
    /// Returns storage or worker-shutdown errors.
    pub fn points_after(
        &self,
        ride_id: Uuid,
        after_sequence: u64,
        limit: usize,
    ) -> Result<RoutePointBatch, RideMapStoreError> {
        let (reply, receiver) = mpsc::channel();
        self.send(StoreCommand::Points {
            ride_id,
            after_sequence,
            limit,
            reply,
        })?;
        receiver.recv().map_err(worker_closed)?
    }

    /// Reconstructs the newest non-terminal recording after process restart.
    ///
    /// # Errors
    ///
    /// Returns storage or domain-decoding errors.
    pub fn recover_open_recording(&self) -> Result<Option<RideRecording>, RideMapStoreError> {
        let (reply, receiver) = mpsc::channel();
        self.send(StoreCommand::Recover { reply })?;
        receiver.recv().map_err(worker_closed)?
    }

    fn spawn(path: PathBuf) -> Result<Self, RideMapStoreError> {
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (commands, worker_commands) = mpsc::sync_channel(64);
        std::thread::Builder::new()
            .name("cutout-ride-map-sqlite".to_owned())
            .spawn(move || run_worker(path, worker_commands, ready_sender))
            .map_err(|error| RideMapStoreError::Worker(error.to_string()))?;
        ready_receiver.recv().map_err(worker_closed)??;
        Ok(Self {
            inner: Arc::new(RideMapDatabaseInner {
                commands,
                service_id: NEXT_SERVICE_ID.fetch_add(1, Ordering::Relaxed),
            }),
        })
    }

    fn send(&self, command: StoreCommand) -> Result<(), RideMapStoreError> {
        self.inner
            .commands
            .send(command)
            .map_err(|_| RideMapStoreError::Worker("database worker stopped".to_owned()))
    }
}

impl Drop for RideMapDatabaseInner {
    fn drop(&mut self) {
        let _ = self.commands.send(StoreCommand::Shutdown);
    }
}

#[allow(clippy::needless_pass_by_value)]
fn run_worker(
    path: PathBuf,
    commands: Receiver<StoreCommand>,
    ready: SyncSender<Result<(), RideMapStoreError>>,
) {
    let mut store = match RideMapStore::open(path) {
        Ok(store) => {
            let _ = ready.send(Ok(()));
            store
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    while let Ok(command) = commands.recv() {
        match command {
            StoreCommand::Metadata { metadata, reply } => {
                let result = store.save_metadata(&metadata);
                let _ = reply.send(result);
            }
            StoreCommand::AppendPoint {
                metadata,
                point,
                reply,
            } => {
                let result = store.append_point(&metadata, point);
                let _ = reply.send(result);
            }
            StoreCommand::Save { recording, reply } => {
                let _ = reply.send(store.save_recording(&recording));
            }
            StoreCommand::Delete { ride_id, reply } => {
                let _ = reply.send(store.delete_recording(ride_id));
            }
            StoreCommand::List { limit, reply } => {
                let _ = reply.send(store.list_summaries(limit));
            }
            StoreCommand::Points {
                ride_id,
                after_sequence,
                limit,
                reply,
            } => {
                let _ = reply.send(store.points_after(ride_id, after_sequence, limit));
            }
            StoreCommand::Recover { reply } => {
                let _ = reply.send(store.recover_open_recording());
            }
            StoreCommand::Shutdown => break,
        }
    }
}

fn worker_closed(_: mpsc::RecvError) -> RideMapStoreError {
    RideMapStoreError::Worker("database worker stopped".to_owned())
}

fn canonical_database_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    Ok(std::fs::canonicalize(parent)?.join(path.file_name().unwrap_or_default()))
}

fn insert_metadata(
    connection: &rusqlite::Transaction<'_>,
    metadata: &RideMetadata,
) -> Result<(), RideMapStoreError> {
    connection.execute(
        "INSERT INTO ride_map_ride
            (ride_id, state, point_count, distance_meters, duration_milliseconds, associated_vehicle)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(ride_id) DO UPDATE SET
            state = excluded.state,
            point_count = excluded.point_count,
            distance_meters = excluded.distance_meters,
            duration_milliseconds = excluded.duration_milliseconds,
            associated_vehicle = excluded.associated_vehicle",
        params![
            metadata.ride_id.to_string(),
            state_name(metadata.state),
            sqlite_i64(metadata.point_count)?,
            metadata.distance_meters,
            sqlite_i64(metadata.duration_milliseconds)?,
            metadata.associated_vehicle.as_deref(),
        ],
    )?;
    Ok(())
}

fn decode_summary(row: &rusqlite::Row<'_>) -> Result<StoredRideSummary, rusqlite::Error> {
    let ride_id = row.get::<_, String>(0)?;
    let state = row.get::<_, String>(1)?;
    let point_count = row.get::<_, i64>(2)?;
    let distance_meters = row.get::<_, f64>(3)?;
    let duration_milliseconds = row.get::<_, i64>(4)?;
    let associated_vehicle = row.get::<_, Option<String>>(5)?;
    Ok(StoredRideSummary {
        ride_id: Uuid::parse_str(&ride_id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        state: parse_state(&state).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        point_count: u64::try_from(point_count).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        distance_meters,
        duration_milliseconds: u64::try_from(duration_milliseconds).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        associated_vehicle,
    })
}

fn decode_point(row: &rusqlite::Row<'_>) -> Result<RoutePoint, rusqlite::Error> {
    let sequence = row.get::<_, i64>(0)?;
    let segment_id = row.get::<_, i64>(1)?;
    let wall_clock_unix_ms = row.get::<_, i64>(2)?;
    let monotonic_ms = row.get::<_, i64>(3)?;
    let latitude = row.get::<_, f64>(4)?;
    let longitude = row.get::<_, f64>(5)?;
    let horizontal_accuracy_meters = row.get::<_, f64>(6)?;
    let sequence = u64::try_from(sequence).map_err(|error| conversion_error(0, error))?;
    let segment_id = u64::try_from(segment_id).map_err(|error| conversion_error(1, error))?;
    let wall_clock_unix_ms =
        u64::try_from(wall_clock_unix_ms).map_err(|error| conversion_error(2, error))?;
    let monotonic_ms = u64::try_from(monotonic_ms).map_err(|error| conversion_error(3, error))?;
    let sample = LocationSample::new(
        MonotonicMilliseconds::new(monotonic_ms),
        wall_clock_unix_ms,
        latitude,
        longitude,
        horizontal_accuracy_meters,
    )
    .map_err(|error| conversion_error(4, error))?;
    Ok(RoutePoint {
        sequence,
        sample,
        segment_id,
    })
}

fn conversion_error<T: std::fmt::Display>(column: usize, error: T) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Integer,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}

fn sqlite_i64(value: impl TryInto<i64>) -> Result<i64, RideMapStoreError> {
    value
        .try_into()
        .map_err(|_| RideMapStoreError::ValueOutOfRange)
}

fn state_name(state: RideState) -> &'static str {
    match state {
        RideState::Recording => "recording",
        RideState::Paused => "paused",
        RideState::Stopped => "stopped",
        RideState::Saved => "saved",
        RideState::Discarded => "discarded",
    }
}

fn parse_state(value: &str) -> Result<RideState, RideMapStoreError> {
    match value {
        "recording" => Ok(RideState::Recording),
        "paused" => Ok(RideState::Paused),
        "stopped" => Ok(RideState::Stopped),
        "saved" => Ok(RideState::Saved),
        "discarded" => Ok(RideState::Discarded),
        other => Err(RideMapStoreError::InvalidState(other.to_owned())),
    }
}
