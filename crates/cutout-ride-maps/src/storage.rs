use crate::{
    Coordinate, LocationAdmission, LocationSample, LocationSource, RideEvent, RideLifecycleState,
    RideSummary, TransitionError,
};
use cutout_core::{PevcapEncoding, PevcapReader};
use hex::encode as hex_encode;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
};
use thiserror::Error;
use uuid::Uuid;

const COMMAND_QUEUE_CAPACITY: usize = 64;
const CURRENT_SCHEMA_VERSION: i64 = 3;

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
    pub id: u64,
    /// User-visible name.
    pub name: String,
    /// Point coordinate.
    pub coordinate: Coordinate,
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
}

struct OwnerEntry {
    path: PathBuf,
    service_id: Uuid,
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
        if let Some(existing) = owner.as_ref() {
            if existing.path != canonical_path {
                return Err(StorageError::AlreadyOpenForDifferentPath);
            }
            return Ok(Self {
                sender: existing.sender.clone(),
                service_id: existing.service_id,
            });
        }

        let connection = Connection::open(&canonical_path)?;
        configure_connection(&connection)?;
        let service_id = Uuid::new_v4();
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let join = thread::Builder::new()
            .name("cutout-ride-maps-db".to_owned())
            .spawn(move || worker_loop(connection, &receiver))
            .map_err(|error| StorageError::WorkerStart(error.to_string()))?;
        let handle = Self {
            sender: sender.clone(),
            service_id,
        };
        *owner = Some(OwnerEntry {
            path: canonical_path,
            service_id,
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
    /// Returns [`StorageError`] when the worker cannot commit the deletion.
    pub fn remove_voltage_sag_model(&self, device_identity: &str) -> Result<(), StorageError> {
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
        minimum_latitude_degrees: f64,
        maximum_latitude_degrees: f64,
        minimum_longitude_degrees: f64,
        maximum_longitude_degrees: f64,
    ) -> Result<Vec<TrailSegment>, StorageError> {
        self.request(move |reply| Command::TrailSegmentsInBounds {
            minimum_latitude_degrees,
            maximum_latitude_degrees,
            minimum_longitude_degrees,
            maximum_longitude_degrees,
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
    ) -> Result<u64, StorageError> {
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
        minimum_latitude_degrees: f64,
        maximum_latitude_degrees: f64,
        minimum_longitude_degrees: f64,
        maximum_longitude_degrees: f64,
    ) -> Result<Vec<MapPoint>, StorageError> {
        self.request(move |reply| Command::MapPointsInBounds {
            minimum_latitude_degrees,
            maximum_latitude_degrees,
            minimum_longitude_degrees,
            maximum_longitude_degrees,
            reply,
        })
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
        minimum_latitude_degrees: f64,
        maximum_latitude_degrees: f64,
        minimum_longitude_degrees: f64,
        maximum_longitude_degrees: f64,
        reply: Reply<Vec<TrailSegment>>,
    },
    CreateMapPoint {
        name: String,
        coordinate: Coordinate,
        reply: Reply<u64>,
    },
    MapPointsInBounds {
        minimum_latitude_degrees: f64,
        maximum_latitude_degrees: f64,
        minimum_longitude_degrees: f64,
        maximum_longitude_degrees: f64,
        reply: Reply<Vec<MapPoint>>,
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
    Shutdown {
        reply: Reply<()>,
    },
}

#[allow(clippy::too_many_lines)]
fn worker_loop(mut connection: Connection, receiver: &Receiver<Command>) {
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
                let _ = reply.send(create_trail(&connection, &name));
            }
            Command::AppendTrailSegment {
                trail_id,
                sequence,
                start,
                end,
                reply,
            } => {
                let _ = reply.send(append_trail_segment(
                    &connection,
                    trail_id,
                    sequence,
                    start,
                    end,
                ));
            }
            Command::TrailSegmentsInBounds {
                minimum_latitude_degrees,
                maximum_latitude_degrees,
                minimum_longitude_degrees,
                maximum_longitude_degrees,
                reply,
            } => {
                let _ = reply.send(trail_segments_in_bounds(
                    &connection,
                    minimum_latitude_degrees,
                    maximum_latitude_degrees,
                    minimum_longitude_degrees,
                    maximum_longitude_degrees,
                ));
            }
            Command::CreateMapPoint {
                name,
                coordinate,
                reply,
            } => {
                let _ = reply.send(create_map_point(&connection, &name, coordinate));
            }
            Command::MapPointsInBounds {
                minimum_latitude_degrees,
                maximum_latitude_degrees,
                minimum_longitude_degrees,
                maximum_longitude_degrees,
                reply,
            } => {
                let _ = reply.send(map_points_in_bounds(
                    &connection,
                    minimum_latitude_degrees,
                    maximum_latitude_degrees,
                    minimum_longitude_degrees,
                    maximum_longitude_degrees,
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

fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    let foreign_keys: i64 =
        connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    migrate(connection)
}

#[allow(clippy::too_many_lines)]
fn migrate(connection: &Connection) -> Result<(), StorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE rides (
                    id TEXT PRIMARY KEY NOT NULL,
                    source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
                    state TEXT NOT NULL CHECK (state IN ('draft', 'active', 'paused', 'stopped', 'interrupted', 'discarded', 'saved', 'imported')),
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    point_count INTEGER NOT NULL CHECK (point_count >= 0),
                    distance_mm INTEGER NOT NULL CHECK (distance_mm >= 0)
                );
                CREATE TABLE ride_points (
                    ride_id TEXT NOT NULL REFERENCES rides(id) ON DELETE CASCADE,
                    sequence INTEGER NOT NULL CHECK (sequence >= 0),
                    monotonic_ms INTEGER NOT NULL,
                    wall_clock_ms INTEGER NOT NULL,
                    latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
                    longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 BETWEEN -1800000000 AND 1800000000),
                    horizontal_accuracy_mm INTEGER,
                    source TEXT NOT NULL CHECK (source IN ('live', 'pevcap_import')),
                    PRIMARY KEY (ride_id, sequence),
                    UNIQUE (ride_id, monotonic_ms, wall_clock_ms, latitude_e7, longitude_e7)
                );
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
        }
        1 => {
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
        }
        CURRENT_SCHEMA_VERSION => {}
        newer => return Err(StorageError::UnsupportedSchemaVersion(newer)),
    }
    Ok(())
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

fn ensure_spatial_schema(connection: &Connection) -> Result<(), StorageError> {
    if !sqlite_capabilities(connection)?.has_rtree() {
        return Err(StorageError::SpatialCapabilityUnavailable);
    }
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS trails (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS trail_segments (
            id INTEGER PRIMARY KEY,
            trail_id TEXT NOT NULL REFERENCES trails(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL,
            start_lat_e7 INTEGER NOT NULL,
            start_lon_e7 INTEGER NOT NULL,
            end_lat_e7 INTEGER NOT NULL,
            end_lon_e7 INTEGER NOT NULL,
            UNIQUE (trail_id, sequence)
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS trail_segments_rtree
            USING rtree(id, min_lat, max_lat, min_lon, max_lon);
        CREATE TABLE IF NOT EXISTS map_points (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            latitude_e7 INTEGER NOT NULL,
            longitude_e7 INTEGER NOT NULL
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS map_points_rtree
            USING rtree(id, min_lat, max_lat, min_lon, max_lon);
        ",
    )?;
    Ok(())
}

fn create_trail(connection: &Connection, name: &str) -> Result<TrailId, StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::InvalidStoredValue {
            field: "trail name",
            value: "empty".to_owned(),
        });
    }
    ensure_spatial_schema(connection)?;
    let trail_id = TrailId::new();
    connection.execute(
        "INSERT INTO trails (id, name) VALUES (?1, ?2)",
        params![trail_id.uuid().to_string(), name],
    )?;
    Ok(trail_id)
}

fn append_trail_segment(
    connection: &Connection,
    trail_id: TrailId,
    sequence: u32,
    start: Coordinate,
    end: Coordinate,
) -> Result<(), StorageError> {
    ensure_spatial_schema(connection)?;
    let trail = trail_id.uuid().to_string();
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM trails WHERE id = ?1)",
        params![trail],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::NotFound);
    }
    connection.execute(
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
    let segment_id = connection.last_insert_rowid();
    let min_lat = start.latitude_degrees().min(end.latitude_degrees());
    let max_lat = start.latitude_degrees().max(end.latitude_degrees());
    let min_lon = start.longitude_degrees().min(end.longitude_degrees());
    let max_lon = start.longitude_degrees().max(end.longitude_degrees());
    connection.execute(
        "INSERT INTO trail_segments_rtree (id, min_lat, max_lat, min_lon, max_lon)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![segment_id, min_lat, max_lat, min_lon, max_lon],
    )?;
    Ok(())
}

fn trail_segments_in_bounds(
    connection: &Connection,
    minimum_latitude_degrees: f64,
    maximum_latitude_degrees: f64,
    minimum_longitude_degrees: f64,
    maximum_longitude_degrees: f64,
) -> Result<Vec<TrailSegment>, StorageError> {
    ensure_spatial_schema(connection)?;
    let mut statement = connection.prepare(
        "SELECT s.trail_id, s.sequence, s.start_lat_e7, s.start_lon_e7,
                s.end_lat_e7, s.end_lon_e7
         FROM trail_segments_rtree r
         JOIN trail_segments s ON s.id = r.id
         WHERE r.max_lat >= ?1 AND r.min_lat <= ?2
           AND r.max_lon >= ?3 AND r.min_lon <= ?4
         ORDER BY s.trail_id, s.sequence",
    )?;
    let rows = statement.query_map(
        params![
            minimum_latitude_degrees,
            maximum_latitude_degrees,
            minimum_longitude_degrees,
            maximum_longitude_degrees,
        ],
        |row| {
            let trail_id = Uuid::parse_str(row.get::<_, String>(0)?.as_str()).map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(StorageError::InvalidStoredValue {
                        field: "trail identifier",
                        value: "invalid UUID".to_owned(),
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
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

fn create_map_point(
    connection: &Connection,
    name: &str,
    coordinate: Coordinate,
) -> Result<u64, StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::InvalidStoredValue {
            field: "map point name",
            value: "empty".to_owned(),
        });
    }
    ensure_spatial_schema(connection)?;
    connection.execute(
        "INSERT INTO map_points (name, latitude_e7, longitude_e7) VALUES (?1, ?2, ?3)",
        params![
            name,
            coordinate.latitude().as_i32(),
            coordinate.longitude().as_i32()
        ],
    )?;
    let id = connection.last_insert_rowid();
    connection.execute(
        "INSERT INTO map_points_rtree (id, min_lat, max_lat, min_lon, max_lon)
         VALUES (?1, ?2, ?2, ?3, ?3)",
        params![
            id,
            coordinate.latitude_degrees(),
            coordinate.longitude_degrees()
        ],
    )?;
    u64::try_from(id).map_err(|_| StorageError::InvalidStoredValue {
        field: "map point identifier",
        value: id.to_string(),
    })
}

fn map_points_in_bounds(
    connection: &Connection,
    minimum_latitude_degrees: f64,
    maximum_latitude_degrees: f64,
    minimum_longitude_degrees: f64,
    maximum_longitude_degrees: f64,
) -> Result<Vec<MapPoint>, StorageError> {
    ensure_spatial_schema(connection)?;
    let mut statement = connection.prepare(
        "SELECT p.id, p.name, p.latitude_e7, p.longitude_e7
         FROM map_points_rtree r JOIN map_points p ON p.id = r.id
         WHERE r.max_lat >= ?1 AND r.min_lat <= ?2
           AND r.max_lon >= ?3 AND r.min_lon <= ?4
         ORDER BY p.id",
    )?;
    let rows = statement.query_map(
        params![
            minimum_latitude_degrees,
            maximum_latitude_degrees,
            minimum_longitude_degrees,
            maximum_longitude_degrees,
        ],
        |row| {
            let coordinate = Coordinate::from_fixed_parts(row.get(2)?, row.get(3)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(MapPoint {
                id: row.get(0)?,
                name: row.get(1)?,
                coordinate,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
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

    let ride_id = create_ride(connection, RideSource::PevcapImport, created_at_ms)?;
    transition_ride(connection, ride_id, RideEvent::Import)?;
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
        if append_location(connection, ride_id, sample)? == LocationAdmission::Accepted {
            location_count = location_count.saturating_add(1);
        }
    }
    connection.execute(
        "INSERT INTO pevcap_imports
            (artifact_digest, artifact_path, ride_id, record_count, location_count, imported_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            digest,
            path.to_string_lossy(),
            ride_id.uuid().to_string(),
            record_count,
            location_count,
            created_at_ms,
        ],
    )?;
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
        "UPDATE rides SET state = ?2, updated_at_ms = created_at_ms WHERE id = ?1",
        params![ride_id.uuid().to_string(), state_to_db(next)],
    )?;
    Ok(next)
}

fn append_location(
    connection: &mut Connection,
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
            let distance = crate::distance_between_millimetres(previous_sample, sample);
            insert_location(connection, ride_id, previous_sequence + 1, sample, distance)?;
            Ok(LocationAdmission::Accepted)
        }
        admission => Ok(admission),
    }
}

fn insert_location(
    connection: &mut Connection,
    ride_id: RideId,
    sequence: i64,
    sample: LocationSample,
    distance_millimetres: u64,
) -> Result<(), StorageError> {
    let transaction = connection.transaction()?;
    transaction.execute(
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
    transaction.execute(
        "UPDATE rides SET point_count = point_count + 1, distance_mm = distance_mm + ?2,
         updated_at_ms = created_at_ms WHERE id = ?1",
        params![ride_id.uuid().to_string(), distance_millimetres],
    )?;
    transaction.commit()?;
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
