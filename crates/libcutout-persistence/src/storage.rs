use cutout_core::{PevcapEncoding, PevcapReader};
use cutout_ride_maps::{
    Coordinate, LocationAdmission, LocationSample, LocationSource, RideEvent, RideLifecycleState,
    RideSummary, TransitionError, distance_between_millimetres,
};
use hex::encode as hex_encode;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
};
use thiserror::Error;
use uuid::Uuid;

const COMMAND_QUEUE_CAPACITY: usize = 64;
const CURRENT_SCHEMA_VERSION: i64 = 5;
const APPLICATION_ID: i64 = 0x4355_544f;
const MAX_QUERY_LIMIT: u32 = 500;

/// Origin of a canonical ride record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideSource {
    /// A ride recorded from live platform location updates.
    Live,
    /// A ride projected from an imported PEVCAP artifact.
    PevcapImport,
}

impl RideSource {
    const fn as_db(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::PevcapImport => "pevcap_import",
        }
    }
}

/// Stable identifier for one canonical ride.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RideId(Uuid);

impl RideId {
    /// Creates a new random ride identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the UUID representation.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }

    /// Creates an identifier from a UUID returned by the mobile boundary.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}

impl Default for RideId {
    fn default() -> Self {
        Self::new()
    }
}

/// Mandatory upper bound for a growing database query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryLimit(u32);

impl QueryLimit {
    /// Validates a non-zero query limit no larger than the service maximum.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidQueryLimit`] when the value is outside the supported range.
    pub fn new(value: u32) -> Result<Self, StorageError> {
        (1..=MAX_QUERY_LIMIT)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(StorageError::InvalidQueryLimit(value))
    }

    const fn get(self) -> u32 {
        self.0
    }
}

/// Stable cursor for descending ride-history pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideCursor {
    created_at_ms: u64,
    ride_id: RideId,
}

impl RideCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(created_at_milliseconds: u64, ride_id: RideId) -> Self {
        Self {
            created_at_ms: created_at_milliseconds,
            ride_id,
        }
    }

    /// Returns the creation-time component.
    #[must_use]
    pub const fn created_at_milliseconds(self) -> u64 {
        self.created_at_ms
    }

    /// Returns the ride-identifier component.
    #[must_use]
    pub const fn ride_id(self) -> RideId {
        self.ride_id
    }
}

/// Bounded ride-history projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RideRecord {
    id: RideId,
    source: RideSource,
    state: RideLifecycleState,
    created_at_ms: u64,
    updated_at_ms: u64,
    summary: RideSummary,
}

impl RideRecord {
    /// Returns the ride identifier.
    #[must_use]
    pub const fn id(self) -> RideId {
        self.id
    }

    /// Returns the persisted lifecycle state.
    #[must_use]
    pub const fn state(self) -> RideLifecycleState {
        self.state
    }

    /// Returns the ride origin.
    #[must_use]
    pub const fn source(self) -> RideSource {
        self.source
    }

    /// Returns the creation time in Unix milliseconds.
    #[must_use]
    pub const fn created_at_milliseconds(self) -> u64 {
        self.created_at_ms
    }

    /// Returns the last durable update time in Unix milliseconds.
    #[must_use]
    pub const fn updated_at_milliseconds(self) -> u64 {
        self.updated_at_ms
    }

    /// Returns the Rust-derived summary.
    #[must_use]
    pub const fn summary(self) -> RideSummary {
        self.summary
    }
}

/// One bounded page of ride-history projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RidePage {
    rides: Vec<RideRecord>,
    next_cursor: Option<RideCursor>,
}

impl RidePage {
    /// Returns the page records in stable newest-first order.
    #[must_use]
    pub fn rides(&self) -> &[RideRecord] {
        &self.rides
    }

    /// Returns the cursor for the next page, when more records exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<RideCursor> {
        self.next_cursor
    }
}

/// Stable cursor for ascending route-point pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutePointCursor(u64);

impl RoutePointCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(sequence: u64) -> Self {
        Self(sequence)
    }

    /// Returns the sequence component.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.0
    }
}

/// One canonical route point with its stable ride sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoutePoint {
    sequence: u64,
    sample: LocationSample,
}

impl RoutePoint {
    /// Returns the stable zero-based sequence within the ride.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns the admitted canonical sample.
    #[must_use]
    pub const fn sample(self) -> LocationSample {
        self.sample
    }
}

/// One bounded page of canonical route points.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutePointPage {
    points: Vec<RoutePoint>,
    next_cursor: Option<RoutePointCursor>,
}

impl RoutePointPage {
    /// Returns points in stable ascending sequence order.
    #[must_use]
    pub fn points(&self) -> &[RoutePoint] {
        &self.points
    }

    /// Returns the cursor for the next page, when more points exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<RoutePointCursor> {
        self.next_cursor
    }
}

/// Bounded startup state produced by Rust after recovery.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BootstrapSnapshot {
    recovered_rides: Arc<[RideId]>,
}

impl BootstrapSnapshot {
    /// Returns rides changed from recording to interrupted during acquisition.
    #[must_use]
    pub fn recovered_rides(&self) -> &[RideId] {
        &self.recovered_rides
    }
}

/// Parsed `SQLite` version components.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SqliteVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl SqliteVersion {
    /// Returns the major version.
    #[must_use]
    pub const fn major(self) -> u32 {
        self.major
    }

    /// Returns the minor version.
    #[must_use]
    pub const fn minor(self) -> u32 {
        self.minor
    }

    /// Returns the patch version.
    #[must_use]
    pub const fn patch(self) -> u32 {
        self.patch
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        let mut parts = value.split('.');
        let major = parse_version_part(parts.next(), value)?;
        let minor = parse_version_part(parts.next(), value)?;
        let patch = parse_version_part(parts.next(), value)?;
        if parts.next().is_some() {
            return Err(StorageError::InvalidSqliteVersion(value.to_owned()));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

fn parse_version_part(value: Option<&str>, whole: &str) -> Result<u32, StorageError> {
    value
        .ok_or_else(|| StorageError::InvalidSqliteVersion(whole.to_owned()))?
        .parse()
        .map_err(|_| StorageError::InvalidSqliteVersion(whole.to_owned()))
}

/// `SQLite` capabilities observed from the opened connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteCapabilities {
    version: SqliteVersion,
    rtree: bool,
    fts5: bool,
}

/// Persisted voltage-sag model for one stable device identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoltageSagModelRecord {
    /// Schema version of the model payload.
    pub schema_version: u16,
    /// Effective resistance in milliohms.
    pub effective_resistance_milliohms: u32,
    /// Number of observations contributing to the model.
    pub observations: u16,
    /// Whether the model was verified against hardware.
    pub hardware_verified: bool,
    /// Last learning wall-clock timestamp in Unix milliseconds.
    pub last_learned_wall_clock_milliseconds: u64,
}

/// Durable result of one PEVCAP artifact import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapImportReceipt {
    /// Canonical ride created for the artifact.
    pub ride_id: RideId,
    /// SHA-256 digest of the source artifact, as lowercase hexadecimal.
    pub artifact_digest: String,
    /// Number of transport records read from the artifact.
    pub record_count: u64,
    /// Number of phone-location samples admitted to the ride.
    pub location_count: u64,
    /// Whether this digest was already imported.
    pub duplicate: bool,
}

/// Stable identifier for a stored trail definition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TrailId(Uuid);

impl TrailId {
    /// Creates a new trail identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the UUID representation.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }

    /// Creates an identifier from a UUID returned by the mobile boundary.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}

impl Default for TrailId {
    fn default() -> Self {
        Self::new()
    }
}

/// One canonical trail segment stored in the spatial index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrailSegment {
    /// Owning trail identifier.
    pub trail_id: TrailId,
    /// Stable sequence within the trail.
    pub sequence: u32,
    /// Segment start coordinate.
    pub start: Coordinate,
    /// Segment end coordinate.
    pub end: Coordinate,
}

/// One charging/food point stored in the spatial index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapPoint {
    /// Stable point identifier.
    pub id: MapPointId,
    /// User-visible name.
    pub name: String,
    /// Point coordinate.
    pub coordinate: Coordinate,
}

/// Stable identifier for a stored map point.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MapPointId(u64);

impl MapPointId {
    /// Restores an identifier returned by a previous query page.
    #[must_use]
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }

    /// Returns the integer representation used at the mobile boundary.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Validated fixed-point WGS84 query bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeoBounds {
    minimum_latitude: i32,
    maximum_latitude: i32,
    minimum_longitude: i32,
    maximum_longitude: i32,
}

impl GeoBounds {
    /// Validates geographic bounds. A minimum longitude greater than the maximum denotes an
    /// antimeridian-crossing viewport.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidGeographicBounds`] for non-finite, out-of-range, or reversed
    /// latitude bounds.
    pub fn new(
        minimum_latitude_degrees: f64,
        maximum_latitude_degrees: f64,
        minimum_longitude_degrees: f64,
        maximum_longitude_degrees: f64,
    ) -> Result<Self, StorageError> {
        let minimum = Coordinate::from_degrees(minimum_latitude_degrees, minimum_longitude_degrees)
            .map_err(|_| StorageError::InvalidGeographicBounds)?;
        let maximum = Coordinate::from_degrees(maximum_latitude_degrees, maximum_longitude_degrees)
            .map_err(|_| StorageError::InvalidGeographicBounds)?;
        if minimum.latitude().as_i32() > maximum.latitude().as_i32() {
            return Err(StorageError::InvalidGeographicBounds);
        }
        Ok(Self {
            minimum_latitude: minimum.latitude().as_i32(),
            maximum_latitude: maximum.latitude().as_i32(),
            minimum_longitude: minimum.longitude().as_i32(),
            maximum_longitude: maximum.longitude().as_i32(),
        })
    }

    const fn crosses_antimeridian(self) -> bool {
        self.minimum_longitude > self.maximum_longitude
    }
}

/// Stable cursor for trail-segment pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrailSegmentCursor {
    trail_id: TrailId,
    sequence: u32,
}

impl TrailSegmentCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(trail_id: TrailId, sequence: u32) -> Self {
        Self { trail_id, sequence }
    }

    /// Returns the trail identifier component.
    #[must_use]
    pub const fn trail_id(self) -> TrailId {
        self.trail_id
    }

    /// Returns the segment sequence component.
    #[must_use]
    pub const fn sequence(self) -> u32 {
        self.sequence
    }
}

/// One bounded page of trail-segment projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrailSegmentPage {
    segments: Vec<TrailSegment>,
    next_cursor: Option<TrailSegmentCursor>,
}

impl TrailSegmentPage {
    /// Returns the page in stable trail-identifier/sequence order.
    #[must_use]
    pub fn segments(&self) -> &[TrailSegment] {
        &self.segments
    }

    /// Returns the next stable page cursor, when more candidates exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<TrailSegmentCursor> {
        self.next_cursor
    }
}

/// Stable cursor for map-point pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapPointCursor(MapPointId);

impl MapPointCursor {
    /// Restores a cursor returned by a previous query page.
    #[must_use]
    pub const fn new(id: MapPointId) -> Self {
        Self(id)
    }

    /// Returns the point identifier component.
    #[must_use]
    pub const fn id(self) -> MapPointId {
        self.0
    }
}

/// One bounded page of map-point projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapPointPage {
    points: Vec<MapPoint>,
    next_cursor: Option<MapPointCursor>,
}

impl MapPointPage {
    /// Returns the page in stable identifier order.
    #[must_use]
    pub fn points(&self) -> &[MapPoint] {
        &self.points
    }

    /// Returns the next stable page cursor, when more candidates exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<MapPointCursor> {
        self.next_cursor
    }
}

impl SqliteCapabilities {
    /// Returns the runtime `SQLite` version.
    #[must_use]
    pub const fn sqlite_version(&self) -> SqliteVersion {
        self.version
    }

    /// Returns whether the built-in R*Tree module is available.
    #[must_use]
    pub const fn has_rtree(&self) -> bool {
        self.rtree
    }

    /// Returns whether FTS5 is available.
    #[must_use]
    pub const fn has_fts5(&self) -> bool {
        self.fts5
    }
}

/// Errors from the Rust-owned ride database service.
#[derive(Debug, Error)]
pub enum StorageError {
    /// The database path could not be used.
    #[error("invalid database path")]
    InvalidPath,
    /// The database is already owned by another path in this process.
    #[error("a different database is already open in this process")]
    AlreadyOpenForDifferentPath,
    /// `SQLite` reported an error.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem setup failed.
    #[error("database filesystem error: {0}")]
    Io(#[from] std::io::Error),
    /// Migration found a schema newer than this build supports.
    #[error("unsupported database schema version {0}")]
    UnsupportedSchemaVersion(i64),
    /// The file is `SQLite` but is not Cutout's application database.
    #[error("invalid Cutout database identity")]
    InvalidDatabaseIdentity,
    /// `SQLite` reported that the file failed its quick integrity check.
    #[error("database integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    /// A growing query was not bounded by a supported limit.
    #[error("invalid query limit {0}; expected 1..={MAX_QUERY_LIMIT}")]
    InvalidQueryLimit(u32),
    /// Geographic bounds were non-finite, out of range, or had reversed latitudes.
    #[error("invalid geographic bounds")]
    InvalidGeographicBounds,
    /// A persisted value could not be decoded.
    #[error("invalid stored {field}: {value}")]
    InvalidStoredValue {
        /// Field that failed decoding.
        field: &'static str,
        /// Bounded stored value for diagnostics.
        value: String,
    },
    /// `SQLite` reported a malformed version string.
    #[error("invalid SQLite version {0}")]
    InvalidSqliteVersion(String),
    /// The requested ride does not exist.
    #[error("ride was not found")]
    NotFound,
    /// The requested lifecycle transition is invalid.
    #[error("invalid ride lifecycle transition: {0}")]
    Transition(#[from] TransitionError),
    /// The ride cannot accept a point in its current state.
    #[error("ride is not accepting location samples in state {0:?}")]
    InvalidRideState(RideLifecycleState),
    /// The bounded command queue is full.
    #[error("ride database command queue is full")]
    QueueFull,
    /// The worker stopped before accepting a command.
    #[error("ride database worker stopped")]
    WorkerStopped,
    /// The worker response channel was dropped.
    #[error("ride database response was dropped")]
    ResponseDropped,
    /// A second worker could not be started.
    #[error("could not start ride database worker: {0}")]
    WorkerStart(String),
    /// PEVCAP decoding failed after the artifact was opened.
    #[error("PEVCAP import failed: {0}")]
    PevcapImport(String),
    /// The requested spatial `SQLite` extension is unavailable.
    #[error("SQLite R*Tree capability is unavailable")]
    SpatialCapabilityUnavailable,
    /// The spatial schema could not be initialized.
    #[error("SQLite spatial schema initialization failed: {0}")]
    SpatialSchemaInitialization(String),
}

enum SpatialSchemaState {
    Uninitialized,
    Ready,
    Unavailable,
    Failed(String),
}

struct OwnerEntry {
    path: PathBuf,
    service_id: Uuid,
    bootstrap: BootstrapSnapshot,
    sender: SyncSender<Command>,
    join: Option<JoinHandle<()>>,
}

static OWNER: OnceLock<Mutex<Option<OwnerEntry>>> = OnceLock::new();

fn owner() -> &'static Mutex<Option<OwnerEntry>> {
    OWNER.get_or_init(|| Mutex::new(None))
}

/// Opaque synchronous handle to the one process-owned ride database worker.
#[derive(Clone, Debug)]
pub struct RideDatabase {
    sender: SyncSender<Command>,
    service_id: Uuid,
    bootstrap: BootstrapSnapshot,
}

impl RideDatabase {
    /// Opens or reuses the one canonical database service for this process.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the path, schema, `SQLite` runtime, or worker cannot be opened.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let canonical_path = canonical_database_path(path)?;
        let mut owner = owner().lock().map_err(|_| StorageError::WorkerStopped)?;
        if owner
            .as_ref()
            .and_then(|entry| entry.join.as_ref())
            .is_some_and(JoinHandle::is_finished)
        {
            if let Some(mut stale) = owner.take() {
                if let Some(join) = stale.join.take() {
                    let _ = join.join();
                }
            }
        }
        if let Some(existing) = owner.as_ref() {
            if existing.path != canonical_path {
                return Err(StorageError::AlreadyOpenForDifferentPath);
            }
            return Ok(Self {
                sender: existing.sender.clone(),
                service_id: existing.service_id,
                bootstrap: existing.bootstrap.clone(),
            });
        }

        let mut connection = Connection::open(&canonical_path)?;
        let bootstrap = configure_connection(&mut connection)?;
        let service_id = Uuid::new_v4();
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let join = thread::Builder::new()
            .name("cutout-ride-maps-db".to_owned())
            .spawn(move || worker_loop(connection, &receiver))
            .map_err(|error| StorageError::WorkerStart(error.to_string()))?;
        let handle = Self {
            sender: sender.clone(),
            service_id,
            bootstrap: bootstrap.clone(),
        };
        *owner = Some(OwnerEntry {
            path: canonical_path,
            service_id,
            bootstrap,
            sender,
            join: Some(join),
        });
        Ok(handle)
    }

    /// Returns the process-wide service identity.
    #[must_use]
    pub const fn service_id(&self) -> Uuid {
        self.service_id
    }

    /// Returns the bounded startup recovery snapshot produced during acquisition.
    #[must_use]
    pub const fn bootstrap(&self) -> &BootstrapSnapshot {
        &self.bootstrap
    }

    /// Returns runtime `SQLite` capabilities through the worker.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker is stopped or cannot report its capabilities.
    pub fn capabilities(&self) -> Result<SqliteCapabilities, StorageError> {
        self.request(|reply| Command::Capabilities { reply })
    }

    /// Creates a draft ride record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot create the row.
    pub fn create_ride(
        &self,
        source: RideSource,
        created_at_ms: u64,
    ) -> Result<RideId, StorageError> {
        self.request(move |reply| Command::CreateRide {
            source,
            created_at_ms,
            reply,
        })
    }

    /// Stores the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identifier is empty or the worker cannot commit it.
    pub fn save_selected_device(
        &self,
        platform_identifier: &str,
        updated_at_ms: u64,
    ) -> Result<(), StorageError> {
        if platform_identifier.trim().is_empty() {
            return Err(StorageError::InvalidStoredValue {
                field: "platform identifier",
                value: "empty".to_owned(),
            });
        }
        self.request(move |reply| Command::SaveSelectedDevice {
            platform_identifier: platform_identifier.to_owned(),
            updated_at_ms,
            reply,
        })
    }

    /// Loads the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query the value.
    pub fn selected_device(&self) -> Result<Option<String>, StorageError> {
        self.request(|reply| Command::SelectedDevice { reply })
    }

    /// Clears the selected platform-local device identifier.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot commit the deletion.
    pub fn clear_selected_device(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::ClearSelectedDevice { reply })
    }

    /// Stores a learned voltage-sag model for one device identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identity is empty or the worker cannot commit it.
    pub fn save_voltage_sag_model(
        &self,
        device_identity: &str,
        model: VoltageSagModelRecord,
    ) -> Result<(), StorageError> {
        if device_identity.trim().is_empty() {
            return Err(StorageError::InvalidStoredValue {
                field: "device identity",
                value: "empty".to_owned(),
            });
        }
        self.request(move |reply| Command::SaveVoltageSagModel {
            device_identity: device_identity.to_owned(),
            model,
            reply,
        })
    }

    /// Loads a learned voltage-sag model for one device identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query the value.
    pub fn voltage_sag_model(
        &self,
        device_identity: &str,
    ) -> Result<Option<VoltageSagModelRecord>, StorageError> {
        self.request(move |reply| Command::VoltageSagModel {
            device_identity: device_identity.to_owned(),
            reply,
        })
    }

    /// Removes a learned voltage-sag model.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the identity is empty or the worker cannot commit the deletion.
    pub fn remove_voltage_sag_model(&self, device_identity: &str) -> Result<(), StorageError> {
        if device_identity.trim().is_empty() {
            return Err(StorageError::InvalidStoredValue {
                field: "device identity",
                value: "empty".to_owned(),
            });
        }
        self.request(move |reply| Command::RemoveVoltageSagModel {
            device_identity: device_identity.to_owned(),
            reply,
        })
    }

    /// Stores opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot commit the marker.
    pub fn save_ride_session_marker(&self, marker: &[u8]) -> Result<(), StorageError> {
        self.request(move |reply| Command::SaveRideSessionMarker {
            marker: marker.to_vec(),
            reply,
        })
    }

    /// Loads opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query the marker.
    pub fn ride_session_marker(&self) -> Result<Option<Vec<u8>>, StorageError> {
        self.request(|reply| Command::RideSessionMarker { reply })
    }

    /// Clears opaque Rust-owned ride-session marker bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot commit the deletion.
    pub fn clear_ride_session_marker(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::ClearRideSessionMarker { reply })
    }

    /// Streams a PEVCAP artifact into a Rust-owned imported ride.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the artifact cannot be hashed, decoded, or committed.
    pub fn import_pevcap(
        &self,
        path: &Path,
        encoding: PevcapEncoding,
        created_at_ms: u64,
    ) -> Result<PevcapImportReceipt, StorageError> {
        self.request(move |reply| Command::ImportPevcap {
            path: path.to_owned(),
            encoding,
            created_at_ms,
            reply,
        })
    }

    /// Creates an empty canonical trail definition.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the worker fails.
    pub fn create_trail(&self, name: &str) -> Result<TrailId, StorageError> {
        self.request(move |reply| Command::CreateTrail {
            name: name.to_owned(),
            reply,
        })
    }

    /// Appends a segment to a canonical trail and indexes its bounding box.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the trail is missing.
    pub fn append_trail_segment(
        &self,
        trail_id: TrailId,
        sequence: u32,
        start: Coordinate,
        end: Coordinate,
    ) -> Result<(), StorageError> {
        self.request(move |reply| Command::AppendTrailSegment {
            trail_id,
            sequence,
            start,
            end,
            reply,
        })
    }

    /// Returns trail segments intersecting a WGS84 bounding box.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the worker fails.
    pub fn trail_segments_in_bounds(
        &self,
        bounds: GeoBounds,
        cursor: Option<TrailSegmentCursor>,
        limit: QueryLimit,
    ) -> Result<TrailSegmentPage, StorageError> {
        self.request(move |reply| Command::TrailSegmentsInBounds {
            bounds,
            cursor,
            limit,
            reply,
        })
    }

    /// Stores a charging/food map point and indexes its coordinate.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the point is invalid.
    pub fn create_map_point(
        &self,
        name: &str,
        coordinate: Coordinate,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| Command::CreateMapPoint {
            name: name.to_owned(),
            coordinate,
            reply,
        })
    }

    /// Returns charging/food points intersecting a WGS84 bounding box.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when spatial indexing is unavailable or the worker fails.
    pub fn map_points_in_bounds(
        &self,
        bounds: GeoBounds,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
    ) -> Result<MapPointPage, StorageError> {
        self.request(move |reply| Command::MapPointsInBounds {
            bounds,
            cursor,
            limit,
            reply,
        })
    }

    /// Rebuilds every derived spatial index from canonical fixed-point rows.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when `RTree` is unavailable or rebuilding cannot commit atomically.
    pub fn rebuild_spatial_indexes(&self) -> Result<(), StorageError> {
        self.request(|reply| Command::RebuildSpatialIndexes { reply })
    }

    /// Writes a consistent `SQLite` backup to a caller-selected file.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the destination cannot be prepared or copied.
    pub fn backup_to(&self, path: &Path) -> Result<(), StorageError> {
        let destination = canonical_backup_path(path)?;
        self.request(move |reply| Command::Backup { destination, reply })
    }

    /// Exports one canonical ride summary as a versioned JSON document.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing or the destination cannot be written.
    pub fn export_ride_json(&self, ride_id: RideId, path: &Path) -> Result<(), StorageError> {
        let destination = canonical_backup_path(path)?;
        self.request(move |reply| Command::ExportRideJson {
            ride_id,
            destination,
            reply,
        })
    }

    /// Applies one lifecycle event to a ride.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing, the transition is invalid, or the
    /// worker cannot process the command.
    pub fn transition(
        &self,
        ride_id: RideId,
        event: RideEvent,
    ) -> Result<RideLifecycleState, StorageError> {
        self.request(move |reply| Command::Transition {
            ride_id,
            event,
            reply,
        })
    }

    /// Appends one location through the worker, returning duplicate/out-of-order admission.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the ride is missing, not accepting samples, or the worker
    /// cannot commit the point.
    pub fn append_location(
        &self,
        ride_id: RideId,
        sample: LocationSample,
    ) -> Result<LocationAdmission, StorageError> {
        self.request(move |reply| Command::AppendLocation {
            ride_id,
            sample,
            reply,
        })
    }

    /// Loads the durable summary projection for one ride.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist or another worker error
    /// prevents the query.
    pub fn summary(&self, ride_id: RideId) -> Result<RideSummary, StorageError> {
        self.request(move |reply| Command::Summary { ride_id, reply })
    }

    /// Lists one bounded page of visible rides in stable newest-first order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot query or decode the page.
    pub fn list_rides(
        &self,
        cursor: Option<RideCursor>,
        limit: QueryLimit,
    ) -> Result<RidePage, StorageError> {
        self.request(move |reply| Command::ListRides {
            cursor,
            limit,
            reply,
        })
    }

    /// Loads one bounded page of canonical route points in ascending sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] when the ride does not exist, or another typed storage
    /// error when the page cannot be decoded.
    pub fn route_points(
        &self,
        ride_id: RideId,
        cursor: Option<RoutePointCursor>,
        limit: QueryLimit,
    ) -> Result<RoutePointPage, StorageError> {
        self.request(move |reply| Command::RoutePoints {
            ride_id,
            cursor,
            limit,
            reply,
        })
    }

    /// Stops the process-wide worker and releases its ownership slot.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the worker cannot stop or its ownership slot cannot be
    /// released.
    pub fn shutdown(self) -> Result<(), StorageError> {
        self.request(|reply| Command::Shutdown { reply })?;
        let mut owner = owner().lock().map_err(|_| StorageError::WorkerStopped)?;
        let Some(mut existing) = owner.take() else {
            return Ok(());
        };
        if existing.service_id != self.service_id {
            *owner = Some(existing);
            return Err(StorageError::WorkerStopped);
        }
        if let Some(join) = existing.join.take() {
            join.join().map_err(|_| StorageError::WorkerStopped)?;
        }
        Ok(())
    }

    fn request<T>(&self, build: impl FnOnce(Reply<T>) -> Command) -> Result<T, StorageError> {
        let (reply, response) = response_channel();
        self.enqueue(build(reply))?;
        receive(&response)
    }

    fn enqueue(&self, command: Command) -> Result<(), StorageError> {
        self.sender.try_send(command).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => StorageError::QueueFull,
            mpsc::TrySendError::Disconnected(_) => StorageError::WorkerStopped,
        })
    }
}

type Reply<T> = mpsc::Sender<Result<T, StorageError>>;

fn response_channel<T>() -> (Reply<T>, Receiver<Result<T, StorageError>>) {
    mpsc::channel()
}

fn receive<T>(response: &Receiver<Result<T, StorageError>>) -> Result<T, StorageError> {
    response.recv().map_err(|_| StorageError::ResponseDropped)?
}

enum Command {
    Capabilities {
        reply: Reply<SqliteCapabilities>,
    },
    CreateRide {
        source: RideSource,
        created_at_ms: u64,
        reply: Reply<RideId>,
    },
    SaveSelectedDevice {
        platform_identifier: String,
        updated_at_ms: u64,
        reply: Reply<()>,
    },
    SelectedDevice {
        reply: Reply<Option<String>>,
    },
    ClearSelectedDevice {
        reply: Reply<()>,
    },
    SaveVoltageSagModel {
        device_identity: String,
        model: VoltageSagModelRecord,
        reply: Reply<()>,
    },
    VoltageSagModel {
        device_identity: String,
        reply: Reply<Option<VoltageSagModelRecord>>,
    },
    RemoveVoltageSagModel {
        device_identity: String,
        reply: Reply<()>,
    },
    SaveRideSessionMarker {
        marker: Vec<u8>,
        reply: Reply<()>,
    },
    RideSessionMarker {
        reply: Reply<Option<Vec<u8>>>,
    },
    ClearRideSessionMarker {
        reply: Reply<()>,
    },
    ImportPevcap {
        path: PathBuf,
        encoding: PevcapEncoding,
        created_at_ms: u64,
        reply: Reply<PevcapImportReceipt>,
    },
    CreateTrail {
        name: String,
        reply: Reply<TrailId>,
    },
    AppendTrailSegment {
        trail_id: TrailId,
        sequence: u32,
        start: Coordinate,
        end: Coordinate,
        reply: Reply<()>,
    },
    TrailSegmentsInBounds {
        bounds: GeoBounds,
        cursor: Option<TrailSegmentCursor>,
        limit: QueryLimit,
        reply: Reply<TrailSegmentPage>,
    },
    CreateMapPoint {
        name: String,
        coordinate: Coordinate,
        reply: Reply<MapPointId>,
    },
    MapPointsInBounds {
        bounds: GeoBounds,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
        reply: Reply<MapPointPage>,
    },
    RebuildSpatialIndexes {
        reply: Reply<()>,
    },
    Backup {
        destination: PathBuf,
        reply: Reply<()>,
    },
    ExportRideJson {
        ride_id: RideId,
        destination: PathBuf,
        reply: Reply<()>,
    },
    Transition {
        ride_id: RideId,
        event: RideEvent,
        reply: Reply<RideLifecycleState>,
    },
    AppendLocation {
        ride_id: RideId,
        sample: LocationSample,
        reply: Reply<LocationAdmission>,
    },
    Summary {
        ride_id: RideId,
        reply: Reply<RideSummary>,
    },
    ListRides {
        cursor: Option<RideCursor>,
        limit: QueryLimit,
        reply: Reply<RidePage>,
    },
    RoutePoints {
        ride_id: RideId,
        cursor: Option<RoutePointCursor>,
        limit: QueryLimit,
        reply: Reply<RoutePointPage>,
    },
    Shutdown {
        reply: Reply<()>,
    },
}

#[allow(clippy::too_many_lines)]
fn worker_loop(mut connection: Connection, receiver: &Receiver<Command>) {
    let mut spatial_schema = SpatialSchemaState::Uninitialized;
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Capabilities { reply } => {
                let _ = reply.send(sqlite_capabilities(&connection));
            }
            Command::CreateRide {
                source,
                created_at_ms,
                reply,
            } => {
                let _ = reply.send(create_ride(&connection, source, created_at_ms));
            }
            Command::SaveSelectedDevice {
                platform_identifier,
                updated_at_ms,
                reply,
            } => {
                let _ = reply.send(save_selected_device(
                    &connection,
                    &platform_identifier,
                    updated_at_ms,
                ));
            }
            Command::SelectedDevice { reply } => {
                let _ = reply.send(selected_device(&connection));
            }
            Command::ClearSelectedDevice { reply } => {
                let _ = reply.send(clear_selected_device(&connection));
            }
            Command::SaveVoltageSagModel {
                device_identity,
                model,
                reply,
            } => {
                let _ = reply.send(save_voltage_sag_model(&connection, &device_identity, model));
            }
            Command::VoltageSagModel {
                device_identity,
                reply,
            } => {
                let _ = reply.send(voltage_sag_model(&connection, &device_identity));
            }
            Command::RemoveVoltageSagModel {
                device_identity,
                reply,
            } => {
                let _ = reply.send(remove_voltage_sag_model(&connection, &device_identity));
            }
            Command::SaveRideSessionMarker { marker, reply } => {
                let _ = reply.send(save_ride_session_marker(&connection, &marker));
            }
            Command::RideSessionMarker { reply } => {
                let _ = reply.send(ride_session_marker(&connection));
            }
            Command::ClearRideSessionMarker { reply } => {
                let _ = reply.send(clear_ride_session_marker(&connection));
            }
            Command::ImportPevcap {
                path,
                encoding,
                created_at_ms,
                reply,
            } => {
                let _ = reply.send(import_pevcap(
                    &mut connection,
                    &path,
                    encoding,
                    created_at_ms,
                ));
            }
            Command::CreateTrail { name, reply } => {
                let _ = reply.send(create_trail(&connection, &mut spatial_schema, &name));
            }
            Command::AppendTrailSegment {
                trail_id,
                sequence,
                start,
                end,
                reply,
            } => {
                let _ = reply.send(append_trail_segment(
                    &mut connection,
                    &mut spatial_schema,
                    trail_id,
                    sequence,
                    start,
                    end,
                ));
            }
            Command::TrailSegmentsInBounds {
                bounds,
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(trail_segments_in_bounds(
                    &connection,
                    &mut spatial_schema,
                    bounds,
                    cursor,
                    limit,
                ));
            }
            Command::CreateMapPoint {
                name,
                coordinate,
                reply,
            } => {
                let _ = reply.send(create_map_point(
                    &mut connection,
                    &mut spatial_schema,
                    &name,
                    coordinate,
                ));
            }
            Command::MapPointsInBounds {
                bounds,
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(map_points_in_bounds(
                    &connection,
                    &mut spatial_schema,
                    bounds,
                    cursor,
                    limit,
                ));
            }
            Command::RebuildSpatialIndexes { reply } => {
                let _ = reply.send(rebuild_spatial_indexes(
                    &mut connection,
                    &mut spatial_schema,
                ));
            }
            Command::Backup { destination, reply } => {
                let _ = reply.send(backup(&connection, &destination));
            }
            Command::ExportRideJson {
                ride_id,
                destination,
                reply,
            } => {
                let _ = reply.send(export_ride_json(&connection, ride_id, &destination));
            }
            Command::Transition {
                ride_id,
                event,
                reply,
            } => {
                let _ = reply.send(transition_ride(&connection, ride_id, event));
            }
            Command::AppendLocation {
                ride_id,
                sample,
                reply,
            } => {
                let _ = reply.send(append_location(&mut connection, ride_id, sample));
            }
            Command::Summary { ride_id, reply } => {
                let _ = reply.send(load_summary(&connection, ride_id));
            }
            Command::ListRides {
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(list_rides(&connection, cursor, limit));
            }
            Command::RoutePoints {
                ride_id,
                cursor,
                limit,
                reply,
            } => {
                let _ = reply.send(route_points(&connection, ride_id, cursor, limit));
            }
            Command::Shutdown { reply } => {
                let _ = reply.send(Ok(()));
                break;
            }
        }
    }
}

fn canonical_database_path(path: &Path) -> Result<PathBuf, StorageError> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().ok_or(StorageError::InvalidPath)?;
    let filename = absolute.file_name().ok_or(StorageError::InvalidPath)?;
    std::fs::create_dir_all(parent)?;
    Ok(parent.canonicalize()?.join(filename))
}

fn canonical_backup_path(path: &Path) -> Result<PathBuf, StorageError> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().ok_or(StorageError::InvalidPath)?;
    let filename = absolute.file_name().ok_or(StorageError::InvalidPath)?;
    std::fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let destination = parent.join(filename);
    if destination.exists() {
        return Err(StorageError::InvalidPath);
    }
    Ok(destination)
}

fn configure_connection(connection: &mut Connection) -> Result<BootstrapSnapshot, StorageError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    let foreign_keys: i64 =
        connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    let quick_check: String =
        connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Err(StorageError::IntegrityCheckFailed(quick_check));
    }
    if !sqlite_capabilities(connection)?.has_rtree() {
        return Err(StorageError::SpatialCapabilityUnavailable);
    }
    migrate(connection)?;
    verify_current_schema(connection)?;
    let recovered_rides = recover_interrupted_rides(connection)?;
    Ok(BootstrapSnapshot {
        recovered_rides: recovered_rides.into(),
    })
}

#[allow(clippy::too_many_lines)]
fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchemaVersion(version));
    }
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != 0 && application_id != APPLICATION_ID {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    match version {
        0 => {
            let user_table_count: u64 = connection.query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            if user_table_count != 0 {
                return Err(StorageError::InvalidDatabaseIdentity);
            }
            connection.execute_batch("BEGIN IMMEDIATE;")?;
            if let Err(error) = create_current_schema(connection) {
                let _ = connection.execute_batch("ROLLBACK;");
                return Err(error);
            }
            connection.execute_batch(
                "PRAGMA application_id = 1129665615;
                 PRAGMA user_version = 5;
                 COMMIT;",
            )?;
        }
        1 => {
            verify_legacy_schema(connection)?;
            connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE selected_device (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    platform_identifier TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE voltage_sag_models (
                    device_identity TEXT PRIMARY KEY NOT NULL,
                    schema_version INTEGER NOT NULL,
                    effective_resistance_milliohms INTEGER NOT NULL,
                    observations INTEGER NOT NULL,
                    hardware_verified INTEGER NOT NULL CHECK (hardware_verified IN (0, 1)),
                    last_learned_wall_clock_ms INTEGER NOT NULL
                );
                CREATE TABLE ride_session_marker (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    marker BLOB NOT NULL
                );
                PRAGMA user_version = 2;
                COMMIT;
                ",
            )?;
            migrate(connection)?;
        }
        2 => {
            verify_legacy_schema(connection)?;
            connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE pevcap_imports (
                    artifact_digest TEXT PRIMARY KEY NOT NULL,
                    artifact_path TEXT NOT NULL,
                    ride_id TEXT NOT NULL REFERENCES rides(id),
                    record_count INTEGER NOT NULL,
                    location_count INTEGER NOT NULL,
                    imported_at_ms INTEGER NOT NULL
                );
                PRAGMA user_version = 3;
                COMMIT;
                ",
            )?;
            migrate(connection)?;
        }
        3 => migrate_v3_to_current(connection)?,
        4 => migrate_v4_to_current(connection)?,
        CURRENT_SCHEMA_VERSION => {
            if application_id != APPLICATION_ID {
                return Err(StorageError::InvalidDatabaseIdentity);
            }
        }
        _ => return Err(StorageError::InvalidDatabaseIdentity),
    }
    Ok(())
}

fn create_current_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "
        CREATE TABLE rides (
            id TEXT PRIMARY KEY NOT NULL,
            source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
            state TEXT NOT NULL CHECK (state IN ('draft', 'active', 'paused', 'stopped', 'interrupted', 'discarded', 'saved', 'imported')),
            created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
            point_count INTEGER NOT NULL CHECK (point_count >= 0),
            distance_mm INTEGER NOT NULL CHECK (distance_mm >= 0)
        );
        CREATE INDEX rides_history_order ON rides(created_at_ms DESC, id DESC);
        CREATE TABLE ride_points (
            ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            monotonic_ms INTEGER NOT NULL CHECK (monotonic_ms >= 0),
            wall_clock_ms INTEGER NOT NULL CHECK (wall_clock_ms >= 0),
            latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
            longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000),
            horizontal_accuracy_mm INTEGER CHECK (horizontal_accuracy_mm IS NULL OR horizontal_accuracy_mm >= 0),
            source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
            PRIMARY KEY (ride_id, sequence),
            UNIQUE (ride_id, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7)
        );
        CREATE TABLE selected_device (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            platform_identifier TEXT NOT NULL CHECK (length(platform_identifier) BETWEEN 1 AND 512),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
        );
        CREATE TABLE voltage_sag_models (
            device_identity TEXT PRIMARY KEY NOT NULL CHECK (length(device_identity) BETWEEN 1 AND 512),
            schema_version INTEGER NOT NULL CHECK (schema_version = 1),
            effective_resistance_milliohms INTEGER NOT NULL CHECK (effective_resistance_milliohms <= 10000),
            observations INTEGER NOT NULL CHECK (observations <= 65535),
            hardware_verified INTEGER NOT NULL CHECK (hardware_verified IN (0, 1)),
            last_learned_wall_clock_ms INTEGER NOT NULL CHECK (last_learned_wall_clock_ms >= 0)
        );
        CREATE TABLE ride_session_marker (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            marker BLOB NOT NULL CHECK (length(marker) BETWEEN 1 AND 4096)
        );
        CREATE TABLE pevcap_imports (
            artifact_digest TEXT PRIMARY KEY NOT NULL CHECK (length(artifact_digest) = 64),
            artifact_path TEXT NOT NULL CHECK (length(artifact_path) BETWEEN 1 AND 4096),
            ride_id TEXT REFERENCES rides(id),
            outcome TEXT NOT NULL CHECK (outcome IN ('ride_and_capture', 'capture_only')),
            artifact_size INTEGER NOT NULL CHECK (artifact_size >= 0),
            record_count INTEGER NOT NULL CHECK (record_count >= 0),
            location_count INTEGER NOT NULL CHECK (location_count >= 0),
            imported_at_ms INTEGER NOT NULL CHECK (imported_at_ms >= 0)
        );
        CREATE TABLE pevcap_import_work (
            artifact_digest TEXT PRIMARY KEY NOT NULL CHECK (length(artifact_digest) = 64),
            artifact_path TEXT NOT NULL CHECK (length(artifact_path) BETWEEN 1 AND 4096),
            ride_id TEXT REFERENCES rides(id) ON DELETE CASCADE
        );
        CREATE TABLE trails (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512)
        );
        CREATE TABLE trail_segments (
            id INTEGER PRIMARY KEY,
            trail_id TEXT NOT NULL REFERENCES trails(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            start_lat_e7 INTEGER NOT NULL CHECK (start_lat_e7 BETWEEN -900000000 AND 900000000),
            start_lon_e7 INTEGER NOT NULL CHECK (start_lon_e7 BETWEEN -1800000000 AND 1800000000),
            end_lat_e7 INTEGER NOT NULL CHECK (end_lat_e7 BETWEEN -900000000 AND 900000000),
            end_lon_e7 INTEGER NOT NULL CHECK (end_lon_e7 BETWEEN -1800000000 AND 1800000000),
            UNIQUE (trail_id, sequence)
        );
        CREATE VIRTUAL TABLE trail_segments_rtree
            USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
        CREATE TABLE map_points (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512),
            latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
            longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000)
        );
        CREATE VIRTUAL TABLE map_points_rtree
            USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
        ",
    )?;
    Ok(())
}

fn verify_legacy_schema(connection: &Connection) -> Result<(), StorageError> {
    for table in ["rides", "ride_points"] {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StorageError::InvalidDatabaseIdentity);
        }
    }
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type IN ('table', 'view') AND name = ?1)",
            [table],
            |row| row.get(0),
        )
        .map_err(StorageError::from)
}

fn migrate_v3_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    let has_spatial = table_exists(connection, "trails")?
        && table_exists(connection, "trail_segments")?
        && table_exists(connection, "map_points")?;
    let transaction = connection.transaction()?;
    if has_spatial {
        transaction.execute_batch(
            "
            DROP TABLE IF EXISTS trail_segments_rtree;
            DROP TABLE IF EXISTS map_points_rtree;
            ALTER TABLE trail_segments RENAME TO trail_segments_legacy;
            ALTER TABLE trails RENAME TO trails_legacy;
            ALTER TABLE map_points RENAME TO map_points_legacy;
            ",
        )?;
    }
    transaction.execute_batch(
        "
        ALTER TABLE pevcap_imports RENAME TO pevcap_imports_legacy;
        ALTER TABLE ride_points RENAME TO ride_points_legacy;
        ALTER TABLE rides RENAME TO rides_legacy;
        ALTER TABLE selected_device RENAME TO selected_device_legacy;
        ALTER TABLE voltage_sag_models RENAME TO voltage_sag_models_legacy;
        ALTER TABLE ride_session_marker RENAME TO ride_session_marker_legacy;
        ",
    )?;
    create_current_schema(&transaction)?;
    if has_spatial {
        transaction.execute_batch(
            "
            INSERT INTO trails SELECT * FROM trails_legacy;
            INSERT INTO trail_segments
                (id, trail_id, sequence, start_lat_e7, start_lon_e7, end_lat_e7, end_lon_e7)
            SELECT id, trail_id, sequence, start_lat_e7, start_lon_e7, end_lat_e7, end_lon_e7
            FROM trail_segments_legacy;
            INSERT INTO map_points SELECT * FROM map_points_legacy;
            INSERT INTO trail_segments_rtree
            SELECT id,
                   min(start_lat_e7, end_lat_e7), max(start_lat_e7, end_lat_e7),
                   min(start_lon_e7, end_lon_e7), max(start_lon_e7, end_lon_e7)
            FROM trail_segments;
            INSERT INTO map_points_rtree
            SELECT id, latitude_e7, latitude_e7, longitude_e7, longitude_e7 FROM map_points;
            DROP TABLE trail_segments_legacy;
            DROP TABLE trails_legacy;
            DROP TABLE map_points_legacy;
            ",
        )?;
    }
    transaction.execute_batch(
        "
        INSERT INTO rides SELECT * FROM rides_legacy;
        INSERT INTO ride_points SELECT * FROM ride_points_legacy;
        INSERT INTO selected_device SELECT * FROM selected_device_legacy;
        INSERT INTO voltage_sag_models SELECT * FROM voltage_sag_models_legacy;
        INSERT INTO ride_session_marker SELECT * FROM ride_session_marker_legacy;
        INSERT INTO pevcap_imports
            (artifact_digest, artifact_path, ride_id, outcome, artifact_size,
             record_count, location_count, imported_at_ms)
        SELECT artifact_digest, artifact_path, ride_id, 'ride_and_capture', 0,
               record_count, location_count, imported_at_ms
        FROM pevcap_imports_legacy;
        DROP TABLE pevcap_imports_legacy;
        DROP TABLE ride_points_legacy;
        DROP TABLE rides_legacy;
        DROP TABLE selected_device_legacy;
        DROP TABLE voltage_sag_models_legacy;
        DROP TABLE ride_session_marker_legacy;
        PRAGMA application_id = 1129665615;
        PRAGMA user_version = 5;
        ",
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_v4_to_current(connection: &mut Connection) -> Result<(), StorageError> {
    verify_legacy_schema(connection)?;
    let has_spatial = table_exists(connection, "trails")?
        && table_exists(connection, "trail_segments")?
        && table_exists(connection, "map_points")?;
    let transaction = connection.transaction()?;
    if has_spatial {
        transaction.execute_batch(
            "
            DROP TABLE IF EXISTS trail_segments_rtree;
            DROP TABLE IF EXISTS map_points_rtree;
            CREATE VIRTUAL TABLE trail_segments_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            CREATE VIRTUAL TABLE map_points_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            ",
        )?;
    } else {
        transaction.execute_batch(
            "
            CREATE TABLE trails (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512)
            );
            CREATE TABLE trail_segments (
                id INTEGER PRIMARY KEY,
                trail_id TEXT NOT NULL REFERENCES trails(id) ON DELETE CASCADE,
                sequence INTEGER NOT NULL CHECK (sequence >= 0),
                start_lat_e7 INTEGER NOT NULL CHECK (start_lat_e7 BETWEEN -900000000 AND 900000000),
                start_lon_e7 INTEGER NOT NULL CHECK (start_lon_e7 BETWEEN -1800000000 AND 1800000000),
                end_lat_e7 INTEGER NOT NULL CHECK (end_lat_e7 BETWEEN -900000000 AND 900000000),
                end_lon_e7 INTEGER NOT NULL CHECK (end_lon_e7 BETWEEN -1800000000 AND 1800000000),
                UNIQUE (trail_id, sequence)
            );
            CREATE VIRTUAL TABLE trail_segments_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            CREATE TABLE map_points (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512),
                latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
                longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000)
            );
            CREATE VIRTUAL TABLE map_points_rtree
                USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
            ",
        )?;
    }
    transaction.execute_batch(
        "
        INSERT INTO trail_segments_rtree
        SELECT id,
               min(start_lat_e7, end_lat_e7), max(start_lat_e7, end_lat_e7),
               min(start_lon_e7, end_lon_e7), max(start_lon_e7, end_lon_e7)
        FROM trail_segments;
        INSERT INTO map_points_rtree
        SELECT id, latitude_e7, latitude_e7, longitude_e7, longitude_e7 FROM map_points;
        PRAGMA user_version = 5;
        ",
    )?;
    transaction.commit()?;
    Ok(())
}

fn verify_current_schema(connection: &Connection) -> Result<(), StorageError> {
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(StorageError::InvalidDatabaseIdentity);
    }
    for table in [
        "rides",
        "ride_points",
        "selected_device",
        "voltage_sag_models",
        "ride_session_marker",
        "pevcap_imports",
        "pevcap_import_work",
        "trails",
        "trail_segments",
        "trail_segments_rtree",
        "map_points",
        "map_points_rtree",
    ] {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StorageError::InvalidDatabaseIdentity);
        }
    }
    Ok(())
}

fn recover_interrupted_rides(connection: &mut Connection) -> Result<Vec<RideId>, StorageError> {
    let transaction = connection.transaction()?;
    let recovered = {
        let mut statement = transaction
            .prepare("SELECT id FROM rides WHERE state IN ('active', 'paused') ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut recovered = Vec::new();
        for row in rows {
            let value = row?;
            let id = Uuid::parse_str(&value).map_err(|_| StorageError::InvalidStoredValue {
                field: "ride identifier",
                value,
            })?;
            recovered.push(RideId::from_uuid(id));
        }
        recovered
    };
    transaction.execute(
        "UPDATE rides SET state = 'interrupted' WHERE state IN ('active', 'paused')",
        [],
    )?;
    transaction.commit()?;
    Ok(recovered)
}

fn sqlite_capabilities(connection: &Connection) -> Result<SqliteCapabilities, StorageError> {
    let version_string: String =
        connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let version = SqliteVersion::parse(&version_string)?;
    let mut statement = connection.prepare("PRAGMA compile_options")?;
    let options = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut rtree = false;
    let mut fts5 = false;
    for option in options {
        match option?.as_str() {
            "ENABLE_RTREE" => rtree = true,
            "ENABLE_FTS5" => fts5 = true,
            _ => {}
        }
    }
    Ok(SqliteCapabilities {
        version,
        rtree,
        fts5,
    })
}

fn backup(connection: &Connection, destination: &Path) -> Result<(), StorageError> {
    connection.execute("VACUUM INTO ?1", params![destination.to_string_lossy()])?;
    Ok(())
}

fn export_ride_json(
    connection: &Connection,
    ride_id: RideId,
    destination: &Path,
) -> Result<(), StorageError> {
    let (source, state, created_at_ms, updated_at_ms, point_count, distance_mm): (
        String,
        String,
        u64,
        u64,
        u64,
        u64,
    ) = connection
        .query_row(
            "SELECT source, state, created_at_ms, updated_at_ms, point_count, distance_mm
             FROM rides WHERE id = ?1",
            params![ride_id.uuid().to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or(StorageError::NotFound)?;
    let json = format!(
        "{{\"schema_version\":1,\"ride_id\":\"{}\",\"source\":\"{}\",\"state\":\"{}\",\"created_at_ms\":{},\"updated_at_ms\":{},\"point_count\":{},\"distance_mm\":{}}}",
        ride_id.uuid(),
        source,
        state,
        created_at_ms,
        updated_at_ms,
        point_count,
        distance_mm
    );
    std::fs::write(destination, json)?;
    Ok(())
}

fn create_ride(
    connection: &Connection,
    source: RideSource,
    created_at_ms: u64,
) -> Result<RideId, StorageError> {
    let ride_id = RideId::new();
    connection.execute(
        "INSERT INTO rides (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
         VALUES (?1, ?2, 'draft', ?3, ?3, 0, 0)",
        params![ride_id.uuid().to_string(), source.as_db(), created_at_ms],
    )?;
    Ok(ride_id)
}

fn ensure_spatial_schema(
    connection: &Connection,
    state: &mut SpatialSchemaState,
) -> Result<(), StorageError> {
    match state {
        SpatialSchemaState::Ready => return Ok(()),
        SpatialSchemaState::Unavailable => return Err(StorageError::SpatialCapabilityUnavailable),
        SpatialSchemaState::Failed(message) => {
            return Err(StorageError::SpatialSchemaInitialization(message.clone()));
        }
        SpatialSchemaState::Uninitialized => {}
    }
    if !sqlite_capabilities(connection)?.has_rtree() {
        *state = SpatialSchemaState::Unavailable;
        return Err(StorageError::SpatialCapabilityUnavailable);
    }
    for table in [
        "trails",
        "trail_segments",
        "trail_segments_rtree",
        "map_points",
        "map_points_rtree",
    ] {
        if !table_exists(connection, table)? {
            let message = format!("missing migrated table {table}");
            *state = SpatialSchemaState::Failed(message.clone());
            return Err(StorageError::SpatialSchemaInitialization(message));
        }
    }
    *state = SpatialSchemaState::Ready;
    Ok(())
}

fn create_trail(
    connection: &Connection,
    state: &mut SpatialSchemaState,
    name: &str,
) -> Result<TrailId, StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::InvalidStoredValue {
            field: "trail name",
            value: "empty".to_owned(),
        });
    }
    ensure_spatial_schema(connection, state)?;
    let trail_id = TrailId::new();
    connection.execute(
        "INSERT INTO trails (id, name) VALUES (?1, ?2)",
        params![trail_id.uuid().to_string(), name],
    )?;
    Ok(trail_id)
}

fn append_trail_segment(
    connection: &mut Connection,
    state: &mut SpatialSchemaState,
    trail_id: TrailId,
    sequence: u32,
    start: Coordinate,
    end: Coordinate,
) -> Result<(), StorageError> {
    ensure_spatial_schema(connection, state)?;
    let transaction = connection.transaction()?;
    let trail = trail_id.uuid().to_string();
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM trails WHERE id = ?1)",
        params![trail],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::NotFound);
    }
    transaction.execute(
        "INSERT INTO trail_segments
            (trail_id, sequence, start_lat_e7, start_lon_e7, end_lat_e7, end_lon_e7)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            trail,
            sequence,
            start.latitude().as_i32(),
            start.longitude().as_i32(),
            end.latitude().as_i32(),
            end.longitude().as_i32(),
        ],
    )?;
    let segment_id = transaction.last_insert_rowid();
    let min_lat = start.latitude().as_i32().min(end.latitude().as_i32());
    let max_lat = start.latitude().as_i32().max(end.latitude().as_i32());
    let min_lon = start.longitude().as_i32().min(end.longitude().as_i32());
    let max_lon = start.longitude().as_i32().max(end.longitude().as_i32());
    transaction.execute(
        "INSERT INTO trail_segments_rtree
            (id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![segment_id, min_lat, max_lat, min_lon, max_lon],
    )?;
    transaction.commit()?;
    Ok(())
}

fn trail_segments_in_bounds(
    connection: &Connection,
    state: &mut SpatialSchemaState,
    bounds: GeoBounds,
    cursor: Option<TrailSegmentCursor>,
    limit: QueryLimit,
) -> Result<TrailSegmentPage, StorageError> {
    ensure_spatial_schema(connection, state)?;
    let mut statement = connection.prepare(
        "SELECT s.trail_id, s.sequence, s.start_lat_e7, s.start_lon_e7,
                s.end_lat_e7, s.end_lon_e7
         FROM trail_segments_rtree r
         JOIN trail_segments s ON s.id = r.id
         WHERE r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2
           AND ((?5 = 0 AND r.max_lon_e7 >= ?3 AND r.min_lon_e7 <= ?4)
             OR (?5 = 1 AND (r.max_lon_e7 >= ?3 OR r.min_lon_e7 <= ?4)))
           AND (?6 IS NULL OR s.trail_id > ?6 OR (s.trail_id = ?6 AND s.sequence > ?7))
         ORDER BY s.trail_id, s.sequence
         LIMIT ?8",
    )?;
    let cursor_trail = cursor.map(|cursor| cursor.trail_id.uuid().to_string());
    let cursor_sequence = cursor.map(|cursor| cursor.sequence);
    let rows = statement.query_map(
        params![
            bounds.minimum_latitude,
            bounds.maximum_latitude,
            bounds.minimum_longitude,
            bounds.maximum_longitude,
            bounds.crosses_antimeridian(),
            cursor_trail,
            cursor_sequence,
            i64::from(limit.get()) + 1,
        ],
        trail_segment_from_row,
    )?;
    let mut segments = rows.collect::<Result<Vec<_>, _>>()?;
    let has_more = segments.len() > limit.get() as usize;
    if has_more {
        segments.pop();
    }
    let next_cursor =
        has_more
            .then(|| segments.last())
            .flatten()
            .map(|segment| TrailSegmentCursor {
                trail_id: segment.trail_id,
                sequence: segment.sequence,
            });
    Ok(TrailSegmentPage {
        segments,
        next_cursor,
    })
}

fn trail_segment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrailSegment> {
    let value: String = row.get(0)?;
    let trail_id = Uuid::parse_str(&value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(StorageError::InvalidStoredValue {
                field: "trail identifier",
                value,
            }),
        )
    })?;
    let start = Coordinate::from_fixed_parts(row.get(2)?, row.get(3)?)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    let end = Coordinate::from_fixed_parts(row.get(4)?, row.get(5)?)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(TrailSegment {
        trail_id: TrailId::from_uuid(trail_id),
        sequence: row.get(1)?,
        start,
        end,
    })
}

fn create_map_point(
    connection: &mut Connection,
    state: &mut SpatialSchemaState,
    name: &str,
    coordinate: Coordinate,
) -> Result<MapPointId, StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::InvalidStoredValue {
            field: "map point name",
            value: "empty".to_owned(),
        });
    }
    ensure_spatial_schema(connection, state)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO map_points (name, latitude_e7, longitude_e7) VALUES (?1, ?2, ?3)",
        params![
            name,
            coordinate.latitude().as_i32(),
            coordinate.longitude().as_i32()
        ],
    )?;
    let id = transaction.last_insert_rowid();
    transaction.execute(
        "INSERT INTO map_points_rtree
            (id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7)
         VALUES (?1, ?2, ?2, ?3, ?3)",
        params![
            id,
            coordinate.latitude().as_i32(),
            coordinate.longitude().as_i32()
        ],
    )?;
    transaction.commit()?;
    u64::try_from(id)
        .map(MapPointId)
        .map_err(|_| StorageError::InvalidStoredValue {
            field: "map point identifier",
            value: id.to_string(),
        })
}

fn map_points_in_bounds(
    connection: &Connection,
    state: &mut SpatialSchemaState,
    bounds: GeoBounds,
    cursor: Option<MapPointCursor>,
    limit: QueryLimit,
) -> Result<MapPointPage, StorageError> {
    ensure_spatial_schema(connection, state)?;
    let mut statement = connection.prepare(
        "SELECT p.id, p.name, p.latitude_e7, p.longitude_e7
         FROM map_points_rtree r JOIN map_points p ON p.id = r.id
         WHERE r.max_lat_e7 >= ?1 AND r.min_lat_e7 <= ?2
           AND ((?5 = 0 AND r.max_lon_e7 >= ?3 AND r.min_lon_e7 <= ?4)
             OR (?5 = 1 AND (r.max_lon_e7 >= ?3 OR r.min_lon_e7 <= ?4)))
           AND p.id > ?6
         ORDER BY p.id
         LIMIT ?7",
    )?;
    let after = cursor
        .map(|cursor| i64::try_from(cursor.0.get()))
        .transpose()
        .map_err(|_| StorageError::InvalidStoredValue {
            field: "map point cursor",
            value: "out of range".to_owned(),
        })?
        .unwrap_or(0);
    let rows = statement.query_map(
        params![
            bounds.minimum_latitude,
            bounds.maximum_latitude,
            bounds.minimum_longitude,
            bounds.maximum_longitude,
            bounds.crosses_antimeridian(),
            after,
            i64::from(limit.get()) + 1,
        ],
        |row| {
            let coordinate = Coordinate::from_fixed_parts(row.get(2)?, row.get(3)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(MapPoint {
                id: MapPointId(row.get(0)?),
                name: row.get(1)?,
                coordinate,
            })
        },
    )?;
    let mut points = rows.collect::<Result<Vec<_>, _>>()?;
    let has_more = points.len() > limit.get() as usize;
    if has_more {
        points.pop();
    }
    let next_cursor = has_more
        .then(|| points.last())
        .flatten()
        .map(|point| MapPointCursor(point.id));
    Ok(MapPointPage {
        points,
        next_cursor,
    })
}

fn rebuild_spatial_indexes(
    connection: &mut Connection,
    state: &mut SpatialSchemaState,
) -> Result<(), StorageError> {
    ensure_spatial_schema(connection, state)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        DELETE FROM trail_segments_rtree;
        INSERT INTO trail_segments_rtree
        SELECT id,
               min(start_lat_e7, end_lat_e7), max(start_lat_e7, end_lat_e7),
               min(start_lon_e7, end_lon_e7), max(start_lon_e7, end_lon_e7)
        FROM trail_segments;
        DELETE FROM map_points_rtree;
        INSERT INTO map_points_rtree
        SELECT id, latitude_e7, latitude_e7, longitude_e7, longitude_e7 FROM map_points;
        ",
    )?;
    transaction.commit()?;
    Ok(())
}

fn artifact_digest(path: &Path) -> Result<String, StorageError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_encode(digest.finalize()))
}

fn import_pevcap(
    connection: &mut Connection,
    path: &Path,
    encoding: PevcapEncoding,
    created_at_ms: u64,
) -> Result<PevcapImportReceipt, StorageError> {
    let path = path.canonicalize().map_err(|_| StorageError::InvalidPath)?;
    let artifact_size = path.metadata()?.len();
    let digest = artifact_digest(&path)?;
    if let Some(existing) = connection
        .query_row(
            "SELECT ride_id, record_count, location_count FROM pevcap_imports WHERE artifact_digest = ?1",
            params![digest],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                ))
            },
        )
        .optional()?
    {
        let ride_id = Uuid::parse_str(&existing.0)
            .map(RideId::from_uuid)
            .map_err(|_| StorageError::InvalidStoredValue {
                field: "PEVCAP ride identifier",
                value: existing.0,
            })?;
        return Ok(PevcapImportReceipt {
            ride_id,
            artifact_digest: digest,
            record_count: existing.1,
            location_count: existing.2,
            duplicate: true,
        });
    }

    let transaction = connection.transaction()?;
    let ride_id = create_ride(&transaction, RideSource::PevcapImport, created_at_ms)?;
    transition_ride(&transaction, ride_id, RideEvent::Import)?;
    let file = File::open(&path)?;
    let mut reader = PevcapReader::new(BufReader::new(file), encoding)
        .map_err(|error| StorageError::PevcapImport(error.to_string()))?;
    let mut record_count = 0_u64;
    let mut location_count = 0_u64;
    while let Some(record) = reader
        .next_record()
        .map_err(|error| StorageError::PevcapImport(error.to_string()))?
    {
        record_count = record_count.saturating_add(1);
        let Some(location) = record.phone_location else {
            continue;
        };
        let coordinate =
            Coordinate::from_degrees(location.latitude_degrees, location.longitude_degrees)
                .map_err(|error| StorageError::PevcapImport(error.to_string()))?;
        let accuracy = location.horizontal_accuracy_meters;
        let horizontal_accuracy_millimetres = if accuracy.is_finite() && accuracy >= 0.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some((accuracy * 1_000.0).round() as u32)
        } else {
            None
        };
        let sample = LocationSample::new(
            coordinate,
            record.monotonic_ms.as_milliseconds(),
            location.wall_clock_unix_ms,
            horizontal_accuracy_millimetres,
            LocationSource::PevcapImport,
        );
        if append_location_in_transaction(&transaction, ride_id, sample)?
            == LocationAdmission::Accepted
        {
            location_count = location_count.saturating_add(1);
        }
    }
    transaction.execute(
        "INSERT INTO pevcap_imports
            (artifact_digest, artifact_path, ride_id, outcome, artifact_size,
             record_count, location_count, imported_at_ms)
         VALUES (?1, ?2, ?3, 'ride_and_capture', ?4, ?5, ?6, ?7)",
        params![
            digest,
            path.to_string_lossy(),
            ride_id.uuid().to_string(),
            artifact_size,
            record_count,
            location_count,
            created_at_ms,
        ],
    )?;
    transaction.commit()?;
    Ok(PevcapImportReceipt {
        ride_id,
        artifact_digest: digest,
        record_count,
        location_count,
        duplicate: false,
    })
}

fn save_selected_device(
    connection: &Connection,
    platform_identifier: &str,
    updated_at_ms: u64,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO selected_device (id, platform_identifier, updated_at_ms)
         VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET platform_identifier = excluded.platform_identifier,
             updated_at_ms = excluded.updated_at_ms",
        params![platform_identifier, updated_at_ms],
    )?;
    Ok(())
}

fn selected_device(connection: &Connection) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT platform_identifier FROM selected_device WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn clear_selected_device(connection: &Connection) -> Result<(), StorageError> {
    connection.execute("DELETE FROM selected_device WHERE id = 1", [])?;
    Ok(())
}

fn save_voltage_sag_model(
    connection: &Connection,
    device_identity: &str,
    model: VoltageSagModelRecord,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO voltage_sag_models
            (device_identity, schema_version, effective_resistance_milliohms, observations,
             hardware_verified, last_learned_wall_clock_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(device_identity) DO UPDATE SET schema_version = excluded.schema_version,
             effective_resistance_milliohms = excluded.effective_resistance_milliohms,
             observations = excluded.observations,
             hardware_verified = excluded.hardware_verified,
             last_learned_wall_clock_ms = excluded.last_learned_wall_clock_ms",
        params![
            device_identity,
            model.schema_version,
            model.effective_resistance_milliohms,
            model.observations,
            model.hardware_verified,
            model.last_learned_wall_clock_milliseconds,
        ],
    )?;
    Ok(())
}

fn voltage_sag_model(
    connection: &Connection,
    device_identity: &str,
) -> Result<Option<VoltageSagModelRecord>, StorageError> {
    connection
        .query_row(
            "SELECT schema_version, effective_resistance_milliohms, observations,
                    hardware_verified, last_learned_wall_clock_ms
             FROM voltage_sag_models WHERE device_identity = ?1",
            params![device_identity],
            |row| {
                Ok(VoltageSagModelRecord {
                    schema_version: row.get(0)?,
                    effective_resistance_milliohms: row.get(1)?,
                    observations: row.get(2)?,
                    hardware_verified: row.get(3)?,
                    last_learned_wall_clock_milliseconds: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(StorageError::from)
}

fn remove_voltage_sag_model(
    connection: &Connection,
    device_identity: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM voltage_sag_models WHERE device_identity = ?1",
        params![device_identity],
    )?;
    Ok(())
}

fn save_ride_session_marker(connection: &Connection, marker: &[u8]) -> Result<(), StorageError> {
    if marker.is_empty() {
        return clear_ride_session_marker(connection);
    }
    connection.execute(
        "INSERT INTO ride_session_marker (id, marker) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET marker = excluded.marker",
        params![marker],
    )?;
    Ok(())
}

fn ride_session_marker(connection: &Connection) -> Result<Option<Vec<u8>>, StorageError> {
    connection
        .query_row(
            "SELECT marker FROM ride_session_marker WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn clear_ride_session_marker(connection: &Connection) -> Result<(), StorageError> {
    connection.execute("DELETE FROM ride_session_marker WHERE id = 1", [])?;
    Ok(())
}

fn transition_ride(
    connection: &Connection,
    ride_id: RideId,
    event: RideEvent,
) -> Result<RideLifecycleState, StorageError> {
    let (state, _source): (String, String) = connection
        .query_row(
            "SELECT state, source FROM rides WHERE id = ?1",
            params![ride_id.uuid().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(StorageError::NotFound)?;
    let current = state_from_db(&state)?;
    let next = current.apply(event)?;
    connection.execute(
        "UPDATE rides SET state = ?2,
         updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000 WHERE id = ?1",
        params![ride_id.uuid().to_string(), state_to_db(next)],
    )?;
    Ok(next)
}

fn append_location(
    connection: &mut Connection,
    ride_id: RideId,
    sample: LocationSample,
) -> Result<LocationAdmission, StorageError> {
    let transaction = connection.transaction()?;
    let admission = append_location_in_transaction(&transaction, ride_id, sample)?;
    transaction.commit()?;
    Ok(admission)
}

fn append_location_in_transaction(
    connection: &Connection,
    ride_id: RideId,
    sample: LocationSample,
) -> Result<LocationAdmission, StorageError> {
    let state: String = connection
        .query_row(
            "SELECT state FROM rides WHERE id = ?1",
            params![ride_id.uuid().to_string()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(StorageError::NotFound)?;
    let state = state_from_db(&state)?;
    if !matches!(
        state,
        RideLifecycleState::Active | RideLifecycleState::Paused | RideLifecycleState::Imported
    ) {
        return Err(StorageError::InvalidRideState(state));
    }

    let previous = connection
        .query_row(
            "SELECT sequence, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7, horizontal_accuracy_mm, source
             FROM ride_points WHERE ride_id = ?1 ORDER BY sequence DESC LIMIT 1",
            params![ride_id.uuid().to_string()],
            |row| {
                let coordinate = Coordinate::from_fixed_parts(row.get(3)?, row.get(4)?).map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Integer,
                        Box::new(StorageError::InvalidStoredValue {
                            field: "coordinate",
                            value: "out of range".to_owned(),
                        }),
                    )
                })?;
                let source = source_from_db(row.get::<_, String>(6)?.as_str()).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok((
                    row.get::<_, i64>(0)?,
                    LocationSample::new(
                        coordinate,
                        row.get::<_, u64>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, Option<u32>>(5)?,
                        source,
                    ),
                ))
            },
        )
        .optional()?;
    let Some((previous_sequence, previous_sample)) = previous else {
        insert_location(connection, ride_id, 0, sample, 0)?;
        return Ok(LocationAdmission::Accepted);
    };
    match sample.admission(Some(&previous_sample)) {
        LocationAdmission::Accepted => {
            let distance = distance_between_millimetres(previous_sample, sample);
            match insert_location(connection, ride_id, previous_sequence + 1, sample, distance) {
                Ok(()) => Ok(LocationAdmission::Accepted),
                Err(StorageError::Sqlite(rusqlite::Error::SqliteFailure(error, _)))
                    if error.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    Ok(LocationAdmission::Duplicate)
                }
                Err(error) => Err(error),
            }
        }
        admission => Ok(admission),
    }
}

fn insert_location(
    connection: &Connection,
    ride_id: RideId,
    sequence: i64,
    sample: LocationSample,
    distance_millimetres: u64,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO ride_points
            (ride_id, sequence, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7, horizontal_accuracy_mm, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            ride_id.uuid().to_string(),
            sequence,
            sample.monotonic_milliseconds(),
            sample.wall_clock_unix_milliseconds(),
            sample.coordinate().latitude().as_i32(),
            sample.coordinate().longitude().as_i32(),
            sample.horizontal_accuracy_millimetres(),
            source_to_db(sample.source()),
        ],
    )?;
    connection.execute(
        "UPDATE rides SET point_count = point_count + 1, distance_mm = distance_mm + ?2,
         updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000 WHERE id = ?1",
        params![ride_id.uuid().to_string(), distance_millimetres],
    )?;
    Ok(())
}

fn load_summary(connection: &Connection, ride_id: RideId) -> Result<RideSummary, StorageError> {
    connection
        .query_row(
            "SELECT point_count, distance_mm FROM rides WHERE id = ?1",
            params![ride_id.uuid().to_string()],
            |row| {
                Ok(RideSummary::from_stored(
                    row.get::<_, u64>(0)?,
                    row.get::<_, u64>(1)?,
                ))
            },
        )
        .optional()?
        .ok_or(StorageError::NotFound)
}

fn list_rides(
    connection: &Connection,
    cursor: Option<RideCursor>,
    limit: QueryLimit,
) -> Result<RidePage, StorageError> {
    let fetch_limit = i64::from(limit.get()) + 1;
    let mut rides = Vec::new();
    if let Some(cursor) = cursor {
        let mut statement = connection.prepare(
            "SELECT id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm
             FROM rides
             WHERE state != 'draft'
               AND (created_at_ms < ?1 OR (created_at_ms = ?1 AND id < ?2))
             ORDER BY created_at_ms DESC, id DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![
                cursor.created_at_ms,
                cursor.ride_id.uuid().to_string(),
                fetch_limit
            ],
            ride_record_from_row,
        )?;
        for row in rows {
            rides.push(row?);
        }
    } else {
        let mut statement = connection.prepare(
            "SELECT id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm
             FROM rides WHERE state != 'draft'
             ORDER BY created_at_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([fetch_limit], ride_record_from_row)?;
        for row in rows {
            rides.push(row?);
        }
    }
    let has_more = rides.len() > limit.get() as usize;
    if has_more {
        rides.pop();
    }
    let next_cursor = has_more
        .then(|| rides.last())
        .flatten()
        .map(|last| RideCursor {
            created_at_ms: last.created_at_ms,
            ride_id: last.id,
        });
    Ok(RidePage { rides, next_cursor })
}

fn ride_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RideRecord> {
    let id_value: String = row.get(0)?;
    let id = Uuid::parse_str(&id_value)
        .map(RideId::from_uuid)
        .map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(StorageError::InvalidStoredValue {
                    field: "ride identifier",
                    value: id_value,
                }),
            )
        })?;
    let source_value: String = row.get(1)?;
    let source = ride_source_from_db(&source_value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let state_value: String = row.get(2)?;
    let state = state_from_db(&state_value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(RideRecord {
        id,
        source,
        state,
        created_at_ms: row.get(3)?,
        updated_at_ms: row.get(4)?,
        summary: RideSummary::from_stored(row.get(5)?, row.get(6)?),
    })
}

fn route_points(
    connection: &Connection,
    ride_id: RideId,
    cursor: Option<RoutePointCursor>,
    limit: QueryLimit,
) -> Result<RoutePointPage, StorageError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE id = ?1)",
        [ride_id.uuid().to_string()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::NotFound);
    }
    let after = cursor.map_or(-1_i64, |cursor| i64::try_from(cursor.0).unwrap_or(i64::MAX));
    let fetch_limit = i64::from(limit.get()) + 1;
    let mut statement = connection.prepare(
        "SELECT sequence, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7,
                horizontal_accuracy_mm, source
         FROM ride_points
         WHERE ride_id = ?1 AND sequence > ?2
         ORDER BY sequence ASC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![ride_id.uuid().to_string(), after, fetch_limit],
        route_point_from_row,
    )?;
    let mut points = Vec::new();
    for row in rows {
        points.push(row?);
    }
    let has_more = points.len() > limit.get() as usize;
    if has_more {
        points.pop();
    }
    let next_cursor = has_more
        .then(|| points.last())
        .flatten()
        .map(|point| RoutePointCursor(point.sequence));
    Ok(RoutePointPage {
        points,
        next_cursor,
    })
}

fn route_point_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoutePoint> {
    let source_value: String = row.get(6)?;
    let source = source_from_db(&source_value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let coordinate = Coordinate::from_fixed_parts(row.get(3)?, row.get(4)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    Ok(RoutePoint {
        sequence: row.get(0)?,
        sample: LocationSample::new(coordinate, row.get(1)?, row.get(2)?, row.get(5)?, source),
    })
}

fn ride_source_from_db(value: &str) -> Result<RideSource, StorageError> {
    match value {
        "live" => Ok(RideSource::Live),
        "pevcap_import" => Ok(RideSource::PevcapImport),
        other => Err(StorageError::InvalidStoredValue {
            field: "ride source",
            value: other.to_owned(),
        }),
    }
}

fn state_to_db(state: RideLifecycleState) -> &'static str {
    match state {
        RideLifecycleState::Draft => "draft",
        RideLifecycleState::Active => "active",
        RideLifecycleState::Paused => "paused",
        RideLifecycleState::Stopped => "stopped",
        RideLifecycleState::Interrupted => "interrupted",
        RideLifecycleState::Discarded => "discarded",
        RideLifecycleState::Saved => "saved",
        RideLifecycleState::Imported => "imported",
    }
}

fn state_from_db(value: &str) -> Result<RideLifecycleState, StorageError> {
    match value {
        "draft" => Ok(RideLifecycleState::Draft),
        "active" => Ok(RideLifecycleState::Active),
        "paused" => Ok(RideLifecycleState::Paused),
        "stopped" => Ok(RideLifecycleState::Stopped),
        "interrupted" => Ok(RideLifecycleState::Interrupted),
        "discarded" => Ok(RideLifecycleState::Discarded),
        "saved" => Ok(RideLifecycleState::Saved),
        "imported" => Ok(RideLifecycleState::Imported),
        other => Err(StorageError::InvalidStoredValue {
            field: "ride state",
            value: other.to_owned(),
        }),
    }
}

fn source_to_db(source: LocationSource) -> &'static str {
    match source {
        LocationSource::Live => "live",
        LocationSource::PevcapImport => "pevcap_import",
    }
}

fn source_from_db(value: &str) -> Result<LocationSource, StorageError> {
    match value {
        "live" => Ok(LocationSource::Live),
        "pevcap_import" => Ok(LocationSource::PevcapImport),
        other => Err(StorageError::InvalidStoredValue {
            field: "location source",
            value: other.to_owned(),
        }),
    }
}
