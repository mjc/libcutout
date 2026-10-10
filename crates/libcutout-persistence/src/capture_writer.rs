//! Bounded PEVCAP ingestion and durable completion evidence.
//! Mobile captures use SQLite as their live source; JSONL exports are created on explicit demand.

use crate::storage::{
    LiveCaptureEventKind, LiveCaptureId, LiveCaptureIntegrity, LiveCaptureLocationAdmission,
    LiveCaptureLocationObservation, LiveCaptureLocationValidation, LiveCaptureState, QueryLimit,
    RideDatabase,
};
use cutout_core::{
    CaptureLabelState, GattChannel, GattFingerprint, PEVCAP_CAPTURE_LABEL_ANNOTATION_KEY,
    PevcapDirection, PevcapHeader, PevcapLocationSample, PevcapMusicEvent, PevcapRecord,
    PevcapResolvedIdentity, TransportWriteLimit, WallClockUnixTimestamp,
};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[cfg(test)]
mod database_completion_tests;
#[cfg(test)]
mod export_digest_tests;
mod material;
use material::{MaterialBaseline, MaterialDecision, PreparedMaterialRecord};

const CAPTURE_POLICY_ANNOTATION_KEY: &str = "capture_recording_policy=";

/// Observation retention contract for one capture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaptureRecordingPolicy {
    /// Retain every admitted transport observation, including repeated telemetry.
    #[default]
    EveryObservation,
    /// Retain stationary telemetry when material values change; all other observations remain complete.
    MaterialChanges,
}

/// Event-scoped transport evidence, established by the current connection and decoder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaptureRecordAdmission {
    /// No complete stationary telemetry proof; retain this observation unconditionally.
    #[default]
    EveryObservation,
    /// This input is a complete telemetry notification from the verified connection with fresh stationary speed.
    StationaryTelemetry,
}

const LIVE_CAPTURE_EXPORT_PAGE_SIZE: u32 = 32;

/// Rust-generated identity of one capture artifact, independent of filenames and devices.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CaptureArtifactId(Uuid);

impl std::fmt::Display for CaptureArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Outcome of a non-blocking write to the capture queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureWriteOutcome {
    /// The event was accepted by the bounded queue.
    Accepted,
    /// Observations were rejected at admission; the writer remains usable.
    AdmissionLost {
        /// Number of observations rejected by this request, excluding earlier losses.
        dropped_messages: u64,
    },
    /// The writer is closed, stopped, or the request could not be admitted.
    Failed,
}

/// Durability of one flush barrier, separate from observation completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureFlushOutcome {
    /// All evidence preceding the admitted barrier is durable.
    Flushed,
    /// The healthy writer did not admit this control request; retry is possible.
    Rejected,
    /// The writer failed or an admitted barrier has no valid durable receipt.
    Failed {
        /// First fatal writer cause, retained across later failures.
        message: String,
    },
}

impl CaptureArtifactId {
    pub(crate) const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}

/// Evidence produced only after a consumed writer has durably completed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedCaptureArtifact {
    id: CaptureArtifactId,
    live_capture_id: Option<LiveCaptureId>,
    path: PathBuf,
    status: CaptureWriterStatus,
    content_digest: String,
    final_header: PevcapHeader,
}

impl SavedCaptureArtifact {
    /// Identity assigned when the writer was created.
    #[must_use]
    pub const fn id(&self) -> CaptureArtifactId {
        self.id
    }
    /// Identity of the incrementally stored live capture, when database-backed writing was used.
    #[must_use]
    pub const fn live_capture_id(&self) -> Option<LiveCaptureId> {
        self.live_capture_id
    }
    /// Exact artifact location, not just its display filename.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Final queue and byte counters.
    #[must_use]
    pub const fn status(&self) -> &CaptureWriterStatus {
        &self.status
    }

    pub(crate) fn content_digest(&self) -> &str {
        &self.content_digest
    }

    pub(crate) const fn final_header(&self) -> &PevcapHeader {
        &self.final_header
    }
}

/// Completion authority for a database capture, produced only by a consumed writer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedDatabaseCapture {
    artifact_id: CaptureArtifactId,
    live_capture_id: LiveCaptureId,
    final_header: PevcapHeader,
}

impl SavedDatabaseCapture {
    /// Original writer identity, independent of any later export file.
    #[must_use]
    pub const fn artifact_id(&self) -> CaptureArtifactId {
        self.artifact_id
    }
    /// Canonical database identity.
    #[must_use]
    pub const fn live_capture_id(&self) -> LiveCaptureId {
        self.live_capture_id
    }
    pub(crate) const fn final_header(&self) -> &PevcapHeader {
        &self.final_header
    }
}

/// Durable terminal result of consuming a capture writer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureWriterFinish {
    /// A synced JSONL file is the durable source for a file-only writer.
    FileSaved(Box<SavedCaptureArtifact>),
    /// SQLite is finalized; a JSONL file is an optional export of its durable contents.
    DatabaseFinished {
        /// Canonical SQLite capture identity.
        live_capture_id: LiveCaptureId,
        /// Durable completeness result established at the terminal barrier.
        integrity: LiveCaptureIntegrity,
        /// Optional PEVCAP JSONL export result.
        jsonl_export: CaptureJsonlExport,
        /// Unforgeable consumed-writer authority for retaining recording provenance.
        receipt: Box<SavedDatabaseCapture>,
        /// Final writer counters, including admitted and dropped messages.
        status: CaptureWriterStatus,
    },
}

/// Result of the optional JSONL export after SQLite finalization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureJsonlExport {
    /// A synced export file is available for sharing or receipt publication.
    Available(Box<SavedCaptureArtifact>),
    /// Export was intentionally skipped; SQLite remains the durable source.
    NotAttempted,
    /// SQLite is durable, but creating or syncing the optional export failed.
    Failed(String),
}

/// Read-only queue and failure evidence retained after active ownership is consumed.
#[derive(Clone, Debug)]
pub struct CaptureWriterMonitor(Arc<CaptureWriterState>);

impl CaptureWriterMonitor {
    /// Retains an error from a failed writer start without inventing an active writer.
    #[must_use]
    pub fn failed(error: String) -> Self {
        let state = Arc::new(CaptureWriterState::default());
        state.fail(error);
        Self(state)
    }
    /// Current bounded writer instrumentation.
    #[must_use]
    pub fn status(&self) -> CaptureWriterStatus {
        self.0.status()
    }
}

const CAPTURE_WRITER_QUEUE_CAPACITY: usize = 256;
/// Maximum location observations admitted in one native callback batch.
pub const CAPTURE_LOCATION_BATCH_CAPACITY: usize = 64;
const CAPTURE_WRITER_BUFFER_BYTES: u64 = 128 * 1024;
const CAPTURE_WRITER_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
const CAPTURE_WRITER_SYNC_INTERVAL: Duration = Duration::from_secs(3);

/// Rust-owned status for the bounded capture writer queue.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptureWriterStatus {
    /// Messages accepted by the queue and not yet processed.
    pub queued_messages: u64,
    /// Highest number of accepted messages waiting to be written.
    pub peak_queued_messages: u64,
    /// Messages rejected because the queue was full or closed.
    pub dropped_messages: u64,
    /// Serialized retained capture bytes; database-backed captures export them at finalization.
    pub bytes_written: u64,
    /// Total successful write payload bytes, including header rewrites.
    pub physical_bytes_written: u64,
    /// Whether the writer has encountered an unrecoverable error.
    pub failed: bool,
    /// First fatal writer error, if one exists.
    pub last_error: Option<String>,
}

#[derive(Debug)]
struct CaptureWriterState {
    queued_messages: AtomicU64,
    peak_queued_messages: AtomicU64,
    dropped_messages: AtomicU64,
    incomplete_messages: AtomicU64,
    bytes_written: AtomicU64,
    physical_bytes_written: AtomicU64,
    failure: Mutex<CaptureWriterFailure>,
}

/// Admission loss cannot be mistaken for a stopped worker or erase its first cause.
#[derive(Debug, Default)]
enum CaptureWriterFailure {
    #[default]
    Healthy,
    AdmissionLoss {
        first_reason: String,
    },
    Fatal {
        first_cause: String,
    },
}

impl Default for CaptureWriterState {
    fn default() -> Self {
        Self {
            queued_messages: AtomicU64::new(0),
            peak_queued_messages: AtomicU64::new(0),
            dropped_messages: AtomicU64::new(0),
            incomplete_messages: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            physical_bytes_written: AtomicU64::new(0),
            failure: Mutex::new(CaptureWriterFailure::Healthy),
        }
    }
}

impl CaptureWriterState {
    fn fail(&self, error: impl Into<String>) {
        let mut failure = self.failure.lock().unwrap_or_else(PoisonError::into_inner);
        match &*failure {
            CaptureWriterFailure::Fatal { .. } => {}
            CaptureWriterFailure::Healthy | CaptureWriterFailure::AdmissionLoss { .. } => {
                *failure = CaptureWriterFailure::Fatal {
                    first_cause: error.into(),
                };
            }
        }
    }

    fn record_admission_loss(&self, count: u64, reason: &str) {
        if count == 0 {
            return;
        }
        self.dropped_messages.fetch_add(count, Ordering::AcqRel);
        self.incomplete_messages.fetch_add(count, Ordering::AcqRel);
        let mut failure = self.failure.lock().unwrap_or_else(PoisonError::into_inner);
        match &*failure {
            CaptureWriterFailure::Healthy => {
                *failure = CaptureWriterFailure::AdmissionLoss {
                    first_reason: reason.into(),
                };
            }
            CaptureWriterFailure::AdmissionLoss { .. } | CaptureWriterFailure::Fatal { .. } => {}
        }
    }

    fn admission_loss_outcome(&self, dropped_messages: u64) -> CaptureWriteOutcome {
        match &*self.failure.lock().unwrap_or_else(PoisonError::into_inner) {
            CaptureWriterFailure::Healthy | CaptureWriterFailure::AdmissionLoss { .. } => {
                CaptureWriteOutcome::AdmissionLost { dropped_messages }
            }
            CaptureWriterFailure::Fatal { .. } => CaptureWriteOutcome::Failed,
        }
    }

    fn file_completion_error(&self) -> Option<String> {
        match &*self.failure.lock().unwrap_or_else(PoisonError::into_inner) {
            CaptureWriterFailure::Healthy => None,
            CaptureWriterFailure::AdmissionLoss { first_reason } => Some(first_reason.clone()),
            CaptureWriterFailure::Fatal { first_cause } => Some(first_cause.clone()),
        }
    }

    fn status(&self) -> CaptureWriterStatus {
        let (failed, last_error) =
            match &*self.failure.lock().unwrap_or_else(PoisonError::into_inner) {
                CaptureWriterFailure::Healthy | CaptureWriterFailure::AdmissionLoss { .. } => {
                    (false, None)
                }
                CaptureWriterFailure::Fatal { first_cause } => (true, Some(first_cause.clone())),
            };
        CaptureWriterStatus {
            queued_messages: self.queued_messages.load(Ordering::Acquire),
            peak_queued_messages: self.peak_queued_messages.load(Ordering::Acquire),
            dropped_messages: self.dropped_messages.load(Ordering::Acquire),
            bytes_written: self.bytes_written.load(Ordering::Acquire),
            physical_bytes_written: self.physical_bytes_written.load(Ordering::Acquire),
            failed,
            last_error,
        }
    }
}

/// Capture header facts forwarded by transport adapters.
#[derive(Clone, Debug)]
pub struct CaptureMetadata {
    /// Observed advertised service identifiers.
    pub advertised_services: Vec<GattChannel>,
    /// Observed GATT services and characteristics.
    pub gatt_fingerprints: Vec<GattFingerprint>,
    /// Protocol-resolved device identity, separate from advertising.
    pub resolved_identity: Option<PevcapResolvedIdentity>,
    /// Ordered capture annotations.
    pub annotations: Vec<String>,
}

enum CaptureWriterMessage {
    Record,
    Location(PevcapLocationSample),
    Music(PevcapMusicEvent),
    Metadata(CaptureMetadata),
    Barrier(
        CaptureBarrier,
        SyncSender<Result<CaptureWriterBarrierResult, String>>,
    ),
}

struct CaptureWriterEventLine {
    json_line: String,
    kind: LiveCaptureEventKind,
    receipt_monotonic_ms: u64,
    source_monotonic_offset_ms: Option<i64>,
    source_wall_clock_unix_ms: Option<u64>,
    location: Option<LiveCaptureLocationObservation>,
    record: Option<PevcapRecord>,
    prepared_material: Option<PreparedMaterialRecord>,
}

struct DatabaseCapture {
    database: RideDatabase,
    id: LiveCaptureId,
    policy: CaptureRecordingPolicy,
}

enum CaptureWriterStorage {
    File(File),
    Database(DatabaseCapture),
}

enum CaptureWriterBarrierResult {
    Flushed,
    FileFinished {
        content_digest: String,
        final_header: Box<PevcapHeader>,
    },
    DatabaseFinished {
        integrity: LiveCaptureIntegrity,
        jsonl_export: CaptureWriterJsonlExport,
        final_header: Box<PevcapHeader>,
    },
}

enum CaptureWriterJsonlExport {
    Available { content_digest: String },
    NotAttempted,
    Failed(String),
}

enum CaptureWriterAction {
    Suppressed,
    Event(Box<CaptureWriterEventLine>),
    Metadata(CaptureMetadata),
    Barrier(
        CaptureBarrier,
        SyncSender<Result<CaptureWriterBarrierResult, String>>,
    ),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CaptureBarrier {
    Flush,
    Finish,
    FinishWithoutExport,
}

impl CaptureBarrier {
    const fn finishes(self) -> bool {
        match self {
            Self::Flush => false,
            Self::Finish | Self::FinishWithoutExport => true,
        }
    }
}

struct CaptureFlushState {
    bytes_since_flush: u64,
    last_flush: Instant,
    last_sync: Instant,
}

impl Default for CaptureFlushState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            bytes_since_flush: 0,
            last_flush: now,
            last_sync: now,
        }
    }
}

#[derive(Debug)]
struct CaptureRecordPool {
    records: Mutex<VecDeque<(PevcapRecord, CaptureRecordAdmission)>>,
}

/// Cloneable, bounded admission capability; unlike a writer handle, it never waits for storage.
#[derive(Clone)]
pub struct CaptureWriterIngress {
    sender: SyncSender<CaptureWriterMessage>,
    records: Arc<CaptureRecordPool>,
    state: Arc<CaptureWriterState>,
    accepting: Arc<Mutex<bool>>,
}

impl std::fmt::Debug for CaptureWriterIngress {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CaptureWriterIngress")
    }
}

impl CaptureWriterIngress {
    fn try_send(&self, message: CaptureWriterMessage) -> CaptureWriteOutcome {
        let accepting = self
            .accepting
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !*accepting {
            self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
            return CaptureWriteOutcome::Failed;
        }
        try_send_message(&self.sender, &self.state, message)
    }

    fn close_and_send(&self, message: CaptureWriterMessage) -> CaptureWriteOutcome {
        let mut accepting = self
            .accepting
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !*accepting {
            self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
            return CaptureWriteOutcome::Failed;
        }
        *accepting = false;
        drop(accepting);
        send_terminal_message(&self.sender, &self.state, message)
    }

    /// Admits one transport record without waiting for disk I/O.
    pub fn try_send_record(&self, record: PevcapRecord) -> CaptureWriteOutcome {
        self.try_send_record_with_admission(record, CaptureRecordAdmission::EveryObservation)
    }

    /// Admits a record with event-scoped decoder evidence; policy comparison happens on the worker.
    pub fn try_send_record_with_admission(
        &self,
        record: PevcapRecord,
        admission: CaptureRecordAdmission,
    ) -> CaptureWriteOutcome {
        let accepting = self
            .accepting
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !*accepting {
            self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
            return CaptureWriteOutcome::Failed;
        }
        let mut records = self
            .records
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if records.len() == records.capacity() {
            self.state
                .record_admission_loss(1, "capture writer queue is full");
            return self.state.admission_loss_outcome(1);
        }
        records.push_back((record, admission));
        match try_send_message(&self.sender, &self.state, CaptureWriterMessage::Record) {
            CaptureWriteOutcome::Accepted => CaptureWriteOutcome::Accepted,
            outcome @ (CaptureWriteOutcome::AdmissionLost { .. } | CaptureWriteOutcome::Failed) => {
                records.pop_back();
                outcome
            }
        }
    }

    /// Counts a native batch that its caller rejected before converting observations.
    ///
    /// A nonempty rejected batch makes an open capture incomplete without stopping the worker.
    /// Empty batches are a no-op; callbacks after closure cannot change durable integrity.
    #[must_use]
    pub fn reject_location_batch(&self, sample_count: usize) -> CaptureWriteOutcome {
        if sample_count == 0 {
            return CaptureWriteOutcome::Accepted;
        }
        let accepting = self
            .accepting
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let count = sample_count as u64;
        if !*accepting {
            self.state
                .dropped_messages
                .fetch_add(count, Ordering::AcqRel);
            return CaptureWriteOutcome::Failed;
        }
        self.state
            .record_admission_loss(count, "capture location batch rejected at admission");
        self.state.admission_loss_outcome(count)
    }

    /// Admits a bounded batch of independent location observations in one ordering step.
    pub fn record_location_batch(&self, locations: &[PevcapLocationSample]) -> CaptureWriteOutcome {
        if locations.len() > CAPTURE_LOCATION_BATCH_CAPACITY {
            return self.reject_location_batch(locations.len());
        }
        let accepting = self
            .accepting
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !*accepting {
            self.state
                .dropped_messages
                .fetch_add(locations.len() as u64, Ordering::AcqRel);
            return CaptureWriteOutcome::Failed;
        }
        for (index, location) in locations.iter().enumerate() {
            match try_send_message(
                &self.sender,
                &self.state,
                CaptureWriterMessage::Location(*location),
            ) {
                CaptureWriteOutcome::Accepted => {}
                CaptureWriteOutcome::AdmissionLost { dropped_messages } => {
                    let remaining = locations.len().saturating_sub(index + 1) as u64;
                    self.state
                        .record_admission_loss(remaining, "capture writer queue is full");
                    return self
                        .state
                        .admission_loss_outcome(dropped_messages + remaining);
                }
                CaptureWriteOutcome::Failed => {
                    let remaining = locations.len().saturating_sub(index + 1) as u64;
                    self.state
                        .record_admission_loss(remaining, "capture writer stopped");
                    return CaptureWriteOutcome::Failed;
                }
            }
        }
        CaptureWriteOutcome::Accepted
    }

    /// Admits one independent location observation without waiting for disk I/O.
    #[must_use]
    pub fn record_location(&self, location: PevcapLocationSample) -> CaptureWriteOutcome {
        self.try_send(CaptureWriterMessage::Location(location))
    }
}

fn try_send_message(
    sender: &SyncSender<CaptureWriterMessage>,
    state: &CaptureWriterState,
    message: CaptureWriterMessage,
) -> CaptureWriteOutcome {
    let is_capture_message = match &message {
        CaptureWriterMessage::Barrier(_, _) => false,
        CaptureWriterMessage::Record
        | CaptureWriterMessage::Location(_)
        | CaptureWriterMessage::Music(_)
        | CaptureWriterMessage::Metadata(_) => true,
    };
    let queued_messages = state.queued_messages.fetch_add(1, Ordering::AcqRel) + 1;
    match sender.try_send(message) {
        Ok(()) => {
            state
                .peak_queued_messages
                .fetch_max(queued_messages, Ordering::AcqRel);
            CaptureWriteOutcome::Accepted
        }
        Err(TrySendError::Full(_)) => {
            state.queued_messages.fetch_sub(1, Ordering::AcqRel);
            if is_capture_message {
                state.record_admission_loss(1, "capture writer queue is full");
                state.admission_loss_outcome(1)
            } else {
                state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                CaptureWriteOutcome::Failed
            }
        }
        Err(TrySendError::Disconnected(_)) => {
            state.queued_messages.fetch_sub(1, Ordering::AcqRel);
            state.dropped_messages.fetch_add(1, Ordering::AcqRel);
            if is_capture_message {
                state.incomplete_messages.fetch_add(1, Ordering::AcqRel);
            }
            state.fail("capture writer stopped");
            CaptureWriteOutcome::Failed
        }
    }
}

fn send_terminal_message(
    sender: &SyncSender<CaptureWriterMessage>,
    state: &CaptureWriterState,
    message: CaptureWriterMessage,
) -> CaptureWriteOutcome {
    let queued_messages = state.queued_messages.fetch_add(1, Ordering::AcqRel) + 1;
    if sender.send(message).is_ok() {
        state
            .peak_queued_messages
            .fetch_max(queued_messages, Ordering::AcqRel);
        CaptureWriteOutcome::Accepted
    } else {
        state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        state.dropped_messages.fetch_add(1, Ordering::AcqRel);
        state.fail("capture writer stopped");
        CaptureWriteOutcome::Failed
    }
}

impl CaptureRecordPool {
    fn new(capacity: usize) -> Self {
        Self {
            records: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn take(&self) -> Option<(PevcapRecord, CaptureRecordAdmission)> {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }
}

/// Active capture ownership. Consuming `finish` is the only route to a saved artifact.
///
/// ```compile_fail
/// use libcutout_persistence::CaptureWriter;
/// fn finish_twice(writer: CaptureWriter) {
///     let _ = writer.finish();
///     let _ = writer.finish(); // the active writer has been consumed
/// }
/// ```
#[derive(Debug)]
pub struct CaptureWriter {
    artifact_id: CaptureArtifactId,
    live_capture_id: Option<LiveCaptureId>,
    recording_policy: CaptureRecordingPolicy,
    path: PathBuf,
    ingress: CaptureWriterIngress,
    state: Arc<CaptureWriterState>,
    join: Option<JoinHandle<()>>,
}

impl CaptureWriter {
    /// Maximum admitted messages waiting for the writer; admission never grows this queue.
    pub const QUEUE_CAPACITY: usize = CAPTURE_WRITER_QUEUE_CAPACITY;

    /// Creates a new file and starts its bounded background writer. Existing files are never overwritten.
    ///
    /// # Errors
    /// Returns the header, file creation, or worker startup error.
    pub fn start(
        path: PathBuf,
        wall_clock_start_unix_ms: WallClockUnixTimestamp,
        platform_id: &str,
        write_limit: Option<TransportWriteLimit>,
        metadata: &CaptureMetadata,
    ) -> Result<Self, String> {
        Self::start_inner(
            path,
            wall_clock_start_unix_ms,
            platform_id,
            write_limit,
            metadata,
            None,
        )
    }

    /// Starts the bounded SQLite writer. The JSONL artifact is exported from durable rows only
    /// after finalization; callback admission remains bounded and never waits for SQLite.
    ///
    /// # Errors
    /// Returns the header, file creation, or worker startup error.
    pub fn start_with_database(
        path: PathBuf,
        wall_clock_start_unix_ms: WallClockUnixTimestamp,
        platform_id: &str,
        write_limit: Option<TransportWriteLimit>,
        metadata: &CaptureMetadata,
        database: RideDatabase,
    ) -> Result<Self, String> {
        Self::start_inner(
            path,
            wall_clock_start_unix_ms,
            platform_id,
            write_limit,
            metadata,
            Some((database, CaptureRecordingPolicy::EveryObservation)),
        )
    }

    /// Starts a SQLite capture with an explicit observation retention contract.
    ///
    /// # Errors
    /// Returns invalid header, existing artifact, or worker startup errors.
    pub fn start_with_database_and_policy(
        path: PathBuf,
        wall_clock_start_unix_ms: WallClockUnixTimestamp,
        platform_id: &str,
        write_limit: Option<TransportWriteLimit>,
        metadata: &CaptureMetadata,
        database: RideDatabase,
        policy: CaptureRecordingPolicy,
    ) -> Result<Self, String> {
        Self::start_inner(
            path,
            wall_clock_start_unix_ms,
            platform_id,
            write_limit,
            metadata,
            Some((database, policy)),
        )
    }

    fn start_inner(
        path: PathBuf,
        wall_clock_start_unix_ms: WallClockUnixTimestamp,
        platform_id: &str,
        write_limit: Option<TransportWriteLimit>,
        metadata: &CaptureMetadata,
        database: Option<(RideDatabase, CaptureRecordingPolicy)>,
    ) -> Result<Self, String> {
        let mut metadata = metadata.clone();
        let recording_policy = database
            .as_ref()
            .map_or(CaptureRecordingPolicy::EveryObservation, |(_, policy)| {
                *policy
            });
        set_policy_annotation(&mut metadata, recording_policy);
        if !metadata_reserves_label_closures(&metadata) {
            return Err(
                "capture annotation capacity cannot retain closing label boundaries".into(),
            );
        }
        let header = capture_header(
            wall_clock_start_unix_ms,
            platform_id,
            write_limit,
            &metadata,
        )?;
        let (storage, live_capture_id) = match database {
            Some((database, policy)) => {
                if path.exists() {
                    return Err("capture artifact already exists".into());
                }
                let id = LiveCaptureId::new();
                let capture = DatabaseCapture {
                    database,
                    id,
                    policy,
                };
                (CaptureWriterStorage::Database(capture), Some(id))
            }
            None => (
                CaptureWriterStorage::File(sync_parent_directory_after(&path, || {
                    OpenOptions::new()
                        .create_new(true)
                        .read(true)
                        .write(true)
                        .open(&path)
                })?),
                None,
            ),
        };
        let (sender, receiver) = sync_channel(CAPTURE_WRITER_QUEUE_CAPACITY);
        let records = Arc::new(CaptureRecordPool::new(CAPTURE_WRITER_QUEUE_CAPACITY));
        let state = Arc::new(CaptureWriterState::default());
        let ingress = CaptureWriterIngress {
            sender: sender.clone(),
            records: Arc::clone(&records),
            state: Arc::clone(&state),
            accepting: Arc::new(Mutex::new(true)),
        };
        let thread_records = Arc::clone(&records);
        let thread_state = Arc::clone(&state);
        let artifact_id = CaptureArtifactId(Uuid::new_v4());
        let writer_path = path.clone();
        let mut header = header;
        let join = thread::Builder::new()
            .name("cutout-pevcap-writer".into())
            .spawn(move || {
                let result = match storage {
                    CaptureWriterStorage::Database(database_capture) => {
                        write_database_capture_stream(
                            &writer_path,
                            &mut header,
                            &receiver,
                            &thread_records,
                            &thread_state,
                            &database_capture,
                        )
                    }
                    CaptureWriterStorage::File(file) => write_capture_stream(
                        &writer_path,
                        file,
                        &mut header,
                        &receiver,
                        &thread_records,
                        &thread_state,
                    ),
                };
                if let Err(error) = result {
                    thread_state.fail(error);
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            artifact_id,
            live_capture_id,
            recording_policy,
            path,
            ingress,
            state,
            join: Some(join),
        })
    }

    fn try_send(&self, message: CaptureWriterMessage) -> CaptureWriteOutcome {
        self.ingress.try_send(message)
    }

    /// Admits a transport record without waiting for disk I/O.
    pub fn try_send_record(&self, record: PevcapRecord) -> CaptureWriteOutcome {
        self.ingress.try_send_record(record)
    }

    /// Returns a cloneable event-admission handle for native callback ingress.
    #[must_use]
    pub fn ingress(&self) -> CaptureWriterIngress {
        self.ingress.clone()
    }

    /// Waits for all admitted data and metadata to become durable.
    ///
    /// # Errors
    /// Returns queue, worker, or storage failure.
    pub fn flush(&self) -> Result<(), String> {
        match self.flush_outcome() {
            CaptureFlushOutcome::Flushed => Ok(()),
            CaptureFlushOutcome::Rejected => Err("capture writer flush was not admitted".into()),
            CaptureFlushOutcome::Failed { message } => Err(message),
        }
    }

    /// Waits for one durable barrier without confusing queue rejection with worker failure.
    #[must_use]
    pub fn flush_outcome(&self) -> CaptureFlushOutcome {
        let status = self.state.status();
        if status.failed {
            return CaptureFlushOutcome::Failed {
                message: status
                    .last_error
                    .unwrap_or_else(|| "capture writer failed".into()),
            };
        }
        let (sender, receiver) = sync_channel(0);
        if self.try_send(CaptureWriterMessage::Barrier(CaptureBarrier::Flush, sender))
            != CaptureWriteOutcome::Accepted
        {
            let status = self.state.status();
            return if status.failed {
                CaptureFlushOutcome::Failed {
                    message: status
                        .last_error
                        .unwrap_or_else(|| "capture writer failed".into()),
                }
            } else {
                CaptureFlushOutcome::Rejected
            };
        }
        match receiver.recv() {
            Ok(Ok(CaptureWriterBarrierResult::Flushed)) => CaptureFlushOutcome::Flushed,
            Ok(Err(error)) => self.fail_flush(error),
            Ok(Ok(
                CaptureWriterBarrierResult::FileFinished { .. }
                | CaptureWriterBarrierResult::DatabaseFinished { .. },
            )) => self.fail_flush("capture writer finished before flush".into()),
            Err(_) => self.fail_flush("capture writer stopped before flush".into()),
        }
    }

    fn fail_flush(&self, message: String) -> CaptureFlushOutcome {
        self.state.fail(&message);
        CaptureFlushOutcome::Failed {
            message: self.state.status().last_error.unwrap_or(message),
        }
    }

    /// Consumes the active writer and reports durable completion separately from JSONL export.
    ///
    /// # Errors
    /// Returns queue, worker, or primary-storage failure. Optional database-backed export
    /// failures are returned inside [`CaptureWriterFinish::DatabaseFinished`].
    pub fn finish(self) -> Result<CaptureWriterFinish, String> {
        self.finish_barrier(CaptureBarrier::Finish)
    }

    /// Completes SQLite without producing an optional JSONL file. File-only writers still sync
    /// their canonical file. Explicit export can later read the durable database capture.
    ///
    /// # Errors
    /// Returns queue, worker or primary-storage failure.
    pub fn finish_without_export(self) -> Result<CaptureWriterFinish, String> {
        self.finish_barrier(CaptureBarrier::FinishWithoutExport)
    }

    fn finish_barrier(mut self, kind: CaptureBarrier) -> Result<CaptureWriterFinish, String> {
        let (sender, receiver) = sync_channel(0);
        if self
            .ingress
            .close_and_send(CaptureWriterMessage::Barrier(kind, sender))
            != CaptureWriteOutcome::Accepted
        {
            return Err(self
                .state
                .status()
                .last_error
                .unwrap_or_else(|| "capture writer finish failed".into()));
        }
        let completion = receiver
            .recv()
            .map_err(|_| "capture writer stopped before finish".to_string())?;
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| "capture writer thread panicked".to_string())?;
        }
        let finalization = match completion? {
            CaptureWriterBarrierResult::FileFinished {
                content_digest,
                final_header,
            } => {
                if let Some(error) = self.state.file_completion_error() {
                    return Err(error);
                }
                CaptureWriterFinish::FileSaved(Box::new(SavedCaptureArtifact {
                    id: self.artifact_id,
                    live_capture_id: self.live_capture_id,
                    path: self.path,
                    status: self.state.status(),
                    content_digest,
                    final_header: *final_header,
                }))
            }
            CaptureWriterBarrierResult::DatabaseFinished {
                integrity,
                jsonl_export,
                final_header,
            } => {
                let live_capture_id = self
                    .live_capture_id
                    .ok_or_else(|| "database writer has no live capture identity".to_string())?;
                let status = self.state.status();
                let jsonl_export = match jsonl_export {
                    CaptureWriterJsonlExport::Available { content_digest } => {
                        CaptureJsonlExport::Available(Box::new(SavedCaptureArtifact {
                            id: self.artifact_id,
                            live_capture_id: Some(live_capture_id),
                            path: self.path,
                            status: status.clone(),
                            content_digest,
                            final_header: (*final_header).clone(),
                        }))
                    }
                    CaptureWriterJsonlExport::NotAttempted => CaptureJsonlExport::NotAttempted,
                    CaptureWriterJsonlExport::Failed(error) => CaptureJsonlExport::Failed(error),
                };
                CaptureWriterFinish::DatabaseFinished {
                    live_capture_id,
                    integrity,
                    jsonl_export,
                    status,
                    receipt: Box::new(SavedDatabaseCapture {
                        artifact_id: self.artifact_id,
                        live_capture_id,
                        final_header: *final_header,
                    }),
                }
            }
            CaptureWriterBarrierResult::Flushed => {
                return Err("capture writer flushed instead of finishing".into());
            }
        };
        Ok(finalization)
    }

    /// Finishes the writer and requires an exported JSONL file.
    ///
    /// Prefer [`Self::finish`] when SQLite is the canonical source and file export is optional.
    ///
    /// # Errors
    /// Returns a primary-storage error or an explicit error when durable database contents have
    /// no JSONL export.
    pub fn finish_exported(self) -> Result<SavedCaptureArtifact, String> {
        match self.finish()? {
            CaptureWriterFinish::FileSaved(artifact)
            | CaptureWriterFinish::DatabaseFinished {
                jsonl_export: CaptureJsonlExport::Available(artifact),
                ..
            } => Ok(*artifact),
            CaptureWriterFinish::DatabaseFinished {
                integrity,
                jsonl_export: CaptureJsonlExport::NotAttempted,
                ..
            } => Err(format!(
                "capture is durably finalized with {integrity:?} integrity; JSONL export was not attempted"
            )),
            CaptureWriterFinish::DatabaseFinished {
                jsonl_export: CaptureJsonlExport::Failed(error),
                ..
            } => Err(format!(
                "capture is durably finalized but JSONL export failed: {error}"
            )),
        }
    }

    /// Returns a cheap monitor that remains usable after the writer is consumed.
    #[must_use]
    pub fn monitor(&self) -> CaptureWriterMonitor {
        CaptureWriterMonitor(Arc::clone(&self.state))
    }

    /// Admits a metadata update without waiting for disk I/O.
    #[must_use]
    pub fn update_metadata(&self, mut metadata: CaptureMetadata) -> CaptureWriteOutcome {
        set_policy_annotation(&mut metadata, self.recording_policy);
        if !metadata_reserves_label_closures(&metadata) {
            return CaptureWriteOutcome::Failed;
        }
        self.try_send(CaptureWriterMessage::Metadata(metadata))
    }

    /// Admits a location observation without waiting for disk I/O.
    #[must_use]
    pub fn record_location(&self, location: PevcapLocationSample) -> CaptureWriteOutcome {
        self.ingress.record_location(location)
    }

    /// Admits an already privacy-filtered music event without waiting for disk I/O.
    #[must_use]
    pub fn record_music(&self, music: PevcapMusicEvent) -> CaptureWriteOutcome {
        self.try_send(CaptureWriterMessage::Music(music))
    }
}

fn metadata_reserves_label_closures(metadata: &CaptureMetadata) -> bool {
    let labels =
        CaptureLabelState::from_annotations(metadata.annotations.iter().map(String::as_str));
    metadata.annotations.len() + labels.active().len() <= cutout_core::PEVCAP_MAX_ANNOTATIONS
}

fn set_policy_annotation(metadata: &mut CaptureMetadata, policy: CaptureRecordingPolicy) {
    metadata
        .annotations
        .retain(|annotation| !annotation.starts_with(CAPTURE_POLICY_ANNOTATION_KEY));
    match policy {
        CaptureRecordingPolicy::EveryObservation => {}
        CaptureRecordingPolicy::MaterialChanges => metadata
            .annotations
            .push("capture_recording_policy=material_changes".into()),
    }
}

fn preserve_policy_annotation(header: &PevcapHeader, metadata: &mut CaptureMetadata) {
    metadata
        .annotations
        .retain(|annotation| !annotation.starts_with(CAPTURE_POLICY_ANNOTATION_KEY));
    if let Some(annotation) = header
        .annotations
        .iter()
        .find(|annotation| annotation.starts_with(CAPTURE_POLICY_ANNOTATION_KEY))
    {
        metadata.annotations.push(annotation.clone());
    }
}

fn capture_header(
    wall_clock_start_unix_ms: WallClockUnixTimestamp,
    platform_id: &str,
    write_limit: Option<TransportWriteLimit>,
    metadata: &CaptureMetadata,
) -> Result<PevcapHeader, String> {
    let annotations = metadata
        .annotations
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    PevcapHeader::new(
        wall_clock_start_unix_ms,
        platform_id,
        write_limit,
        &metadata.advertised_services,
        &metadata.gatt_fingerprints,
        None,
        metadata.resolved_identity.clone(),
        env!("CARGO_PKG_VERSION"),
        [0; 32],
        &annotations,
    )
    .map_err(|error| format!("invalid capture header: {error}"))
}

fn write_capture_stream(
    path: &Path,
    file: File,
    header: &mut PevcapHeader,
    receiver: &Receiver<CaptureWriterMessage>,
    records: &CaptureRecordPool,
    state: &CaptureWriterState,
) -> Result<(), String> {
    let mut writer = BufWriter::new(file);
    let header_line = header.to_jsonl_line().map_err(|error| error.to_string())?;
    let header_bytes = write_line(&mut writer, &header_line)?;
    state
        .physical_bytes_written
        .fetch_add(header_bytes as u64, Ordering::AcqRel);
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_mut()
        .sync_data()
        .map_err(|error| error.to_string())?;
    let mut flush = CaptureFlushState::default();
    let mut pending_metadata = None;
    let mut material = MaterialBaseline::new(CaptureRecordingPolicy::EveryObservation);

    while let Ok(message) = receiver.recv() {
        state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        match capture_writer_action(message, records, &mut material)? {
            CaptureWriterAction::Suppressed => {}
            CaptureWriterAction::Event(event) => {
                write_capture_event_line(&mut writer, &event.json_line, state, &mut flush)?;
            }
            CaptureWriterAction::Metadata(metadata) => pending_metadata = Some(metadata),
            CaptureWriterAction::Barrier(kind, reply) => {
                let result = flush_capture_barrier(
                    path,
                    &mut writer,
                    header,
                    &mut pending_metadata,
                    &mut flush,
                    state,
                    kind,
                )
                .and_then(|()| match kind {
                    CaptureBarrier::Flush => Ok(CaptureWriterBarrierResult::Flushed),
                    CaptureBarrier::Finish | CaptureBarrier::FinishWithoutExport => {
                        let content_digest = digest_open_file(writer.get_mut())?;
                        Ok(CaptureWriterBarrierResult::FileFinished {
                            content_digest,
                            final_header: Box::new(header.clone()),
                        })
                    }
                });
                reply_capture_writer_result(result, &reply)?;
                if kind.finishes() {
                    return Ok(());
                }
            }
        }
    }
    flush_capture_barrier(
        path,
        &mut writer,
        header,
        &mut pending_metadata,
        &mut flush,
        state,
        CaptureBarrier::Finish,
    )?;
    Ok(())
}

fn write_database_capture_stream(
    path: &Path,
    header: &mut PevcapHeader,
    receiver: &Receiver<CaptureWriterMessage>,
    records: &CaptureRecordPool,
    state: &CaptureWriterState,
    capture: &DatabaseCapture,
) -> Result<(), String> {
    let header_line = header.to_jsonl_line().map_err(|error| error.to_string())?;
    let header_size = header_line.len() as u64;
    capture
        .database
        .begin_live_capture_with_id(
            capture.id,
            header_line.into_bytes(),
            header.wall_clock_start_unix_ms.as_milliseconds(),
        )
        .map_err(|error| format!("could not start live SQLite capture: {error}"))?;
    state
        .bytes_written
        .fetch_add(header_size + 1, Ordering::AcqRel);

    let mut pending_metadata = None;
    let mut material = MaterialBaseline::new(capture.policy);
    while let Ok(message) = receiver.recv() {
        state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        match capture_writer_action(message, records, &mut material)? {
            CaptureWriterAction::Suppressed => {}
            CaptureWriterAction::Event(event) => {
                let prepared = event.prepared_material;
                let payload = event.json_line.into_bytes();
                let serialized_size = payload.len() as u64 + 1;
                if let Some(record) = event.record {
                    capture
                        .database
                        .append_live_capture_record(capture.id, record, payload)
                } else if let Some(location) = event.location {
                    capture.database.append_live_capture_location(
                        capture.id,
                        event.receipt_monotonic_ms,
                        event.source_monotonic_offset_ms,
                        location,
                        payload,
                    )
                } else {
                    capture.database.append_live_capture_event(
                        capture.id,
                        event.kind,
                        event.receipt_monotonic_ms,
                        event.source_monotonic_offset_ms,
                        event.source_wall_clock_unix_ms,
                        payload,
                    )
                }
                .map_err(|error| format!("could not persist live capture event: {error}"))?;
                material.commit(prepared);
                state
                    .bytes_written
                    .fetch_add(serialized_size, Ordering::AcqRel);
            }
            CaptureWriterAction::Metadata(metadata) => pending_metadata = Some(metadata),
            CaptureWriterAction::Barrier(kind, reply) => {
                if finish_database_capture_barrier(
                    path,
                    header,
                    &mut pending_metadata,
                    state,
                    capture,
                    kind,
                    &reply,
                )? {
                    return Ok(());
                }
            }
        }
    }

    finalize_database_capture_metadata(
        header,
        &mut pending_metadata,
        state,
        CaptureBarrier::Finish,
    )?;
    persist_database_capture(capture, header, CaptureBarrier::FinishWithoutExport, state)?;
    Ok(())
}

fn finish_database_capture_barrier(
    path: &Path,
    header: &mut PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
    state: &CaptureWriterState,
    capture: &DatabaseCapture,
    kind: CaptureBarrier,
    reply: &SyncSender<Result<CaptureWriterBarrierResult, String>>,
) -> Result<bool, String> {
    let result = finalize_database_capture_metadata(header, pending_metadata, state, kind)
        .and_then(|()| persist_database_capture(capture, header, kind, state))
        .and_then(|integrity| match kind {
            CaptureBarrier::Flush => Ok(CaptureWriterBarrierResult::Flushed),
            CaptureBarrier::Finish | CaptureBarrier::FinishWithoutExport => {
                let integrity = integrity.ok_or_else(|| {
                    "finished database capture has no integrity result".to_string()
                })?;
                let jsonl_export = if kind == CaptureBarrier::FinishWithoutExport {
                    CaptureWriterJsonlExport::NotAttempted
                } else {
                    match integrity {
                        LiveCaptureIntegrity::Complete => {
                            match export_database_capture(path, capture, state) {
                                Ok(content_digest) => {
                                    CaptureWriterJsonlExport::Available { content_digest }
                                }
                                Err(error) => CaptureWriterJsonlExport::Failed(error),
                            }
                        }
                        LiveCaptureIntegrity::Incomplete { .. } => {
                            CaptureWriterJsonlExport::NotAttempted
                        }
                        LiveCaptureIntegrity::Unknown => CaptureWriterJsonlExport::Failed(
                            "finished live capture has unknown integrity".into(),
                        ),
                    }
                };
                Ok(CaptureWriterBarrierResult::DatabaseFinished {
                    integrity,
                    jsonl_export,
                    final_header: Box::new(header.clone()),
                })
            }
        });
    reply_capture_writer_result(result, reply)?;
    Ok(kind.finishes())
}

fn finalize_database_capture_metadata(
    header: &mut PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
    state: &CaptureWriterState,
    kind: CaptureBarrier,
) -> Result<(), String> {
    if kind.finishes() {
        close_pending_capture_labels(header, pending_metadata)?;
    }
    let Some(mut metadata) = pending_metadata.take() else {
        return Ok(());
    };
    preserve_policy_annotation(header, &mut metadata);
    let previous_size = header
        .to_jsonl_line()
        .map_err(|error| error.to_string())?
        .len() as u64
        + 1;
    *header = capture_header(
        header.wall_clock_start_unix_ms,
        header.platform_id.as_str(),
        header.write_limit,
        &metadata,
    )?;
    let next_size = header
        .to_jsonl_line()
        .map_err(|error| error.to_string())?
        .len() as u64
        + 1;
    let _ = state
        .bytes_written
        .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            Some(
                current
                    .saturating_sub(previous_size)
                    .saturating_add(next_size),
            )
        });
    Ok(())
}

fn persist_database_capture(
    capture: &DatabaseCapture,
    header: &PevcapHeader,
    kind: CaptureBarrier,
    state: &CaptureWriterState,
) -> Result<Option<LiveCaptureIntegrity>, String> {
    let header_line = header.to_jsonl_line().map_err(|error| error.to_string())?;
    if kind.finishes() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis();
        let now_ms = u64::try_from(now_ms).map_err(|error| error.to_string())?;
        let finished_at_ms = now_ms.max(header.wall_clock_start_unix_ms.as_milliseconds());
        let dropped_messages = state.incomplete_messages.load(Ordering::Acquire);
        let integrity = if dropped_messages == 0 {
            LiveCaptureIntegrity::Complete
        } else {
            LiveCaptureIntegrity::Incomplete { dropped_messages }
        };
        capture
            .database
            .finalize_live_capture(
                capture.id,
                header_line.into_bytes(),
                finished_at_ms,
                integrity,
            )
            .map_err(|error| format!("could not finish live SQLite capture: {error}"))?;
        return Ok(Some(integrity));
    }
    capture
        .database
        .update_live_capture_header(capture.id, header_line.into_bytes())
        .map_err(|error| format!("could not persist live capture header: {error}"))?;
    Ok(None)
}

impl RideDatabase {
    /// Streams exact retained JSONL bytes on explicit demand. Interrupted/incomplete captures
    /// remain exportable as evidence; their history integrity is never promoted to complete.
    /// Existing files are never overwritten and failed partial exports are removed.
    ///
    /// # Errors
    /// Returns storage, active-capture, output or sync failures. Database contents are preserved.
    pub fn export_live_capture(&self, id: LiveCaptureId, path: &Path) -> Result<String, String> {
        let capture = DatabaseCapture {
            database: self.clone(),
            id,
            policy: CaptureRecordingPolicy::default(),
        };
        export_database_capture(path, &capture, &CaptureWriterState::default())
    }
}

fn export_database_capture(
    path: &Path,
    capture: &DatabaseCapture,
    state: &CaptureWriterState,
) -> Result<String, String> {
    let limit =
        QueryLimit::new(LIVE_CAPTURE_EXPORT_PAGE_SIZE).map_err(|error| error.to_string())?;
    let first_page = capture
        .database
        .live_capture_page(capture.id, None, limit)
        .map_err(|error| format!("could not read finalized live capture: {error}"))?;
    if first_page.state == LiveCaptureState::Active {
        return Err("live capture must be inactive before export".into());
    }

    let mut created = false;
    let result = (|| {
        let file = sync_parent_directory_after(path, || {
            let file = OpenOptions::new().create_new(true).write(true).open(path)?;
            created = true;
            Ok(file)
        })?;
        let mut output = BufWriter::new(file);
        let header = std::str::from_utf8(&first_page.header_json)
            .map_err(|error| format!("stored capture header is not UTF-8: {error}"))?;
        let mut bytes_written = write_line(&mut output, header)? as u64;
        let mut digest = Sha256::new();
        digest.update(header.as_bytes());
        digest.update(b"\n");

        let mut page = Some(first_page);
        let mut after_sequence = None;
        loop {
            let snapshot = match page.take() {
                Some(snapshot) => snapshot,
                None => capture
                    .database
                    .live_capture_page(capture.id, after_sequence, limit)
                    .map_err(|error| format!("could not continue live capture export: {error}"))?,
            };
            for event in &snapshot.events {
                output
                    .write_all(&event.payload)
                    .and_then(|()| output.write_all(b"\n"))
                    .map_err(|error| error.to_string())?;
                digest.update(&event.payload);
                digest.update(b"\n");
                bytes_written = bytes_written.saturating_add(event.payload.len() as u64 + 1);
            }
            let cursor = snapshot.events.last().map(|event| event.sequence);
            let has_more = cursor
                .and_then(|sequence| sequence.checked_add(1))
                .is_some_and(|next_sequence| next_sequence < snapshot.next_sequence);
            if !has_more {
                break;
            }
            after_sequence = cursor;
        }

        output.flush().map_err(|error| error.to_string())?;
        let file = output
            .into_inner()
            .map_err(|error| error.into_error().to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        state
            .physical_bytes_written
            .fetch_add(bytes_written, Ordering::AcqRel);
        Ok(hex::encode(digest.finalize()))
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(path);
    }
    result
}

fn capture_writer_action(
    message: CaptureWriterMessage,
    records: &CaptureRecordPool,
    material: &mut MaterialBaseline,
) -> Result<CaptureWriterAction, String> {
    match &message {
        CaptureWriterMessage::Location(_)
        | CaptureWriterMessage::Music(_)
        | CaptureWriterMessage::Metadata(_) => material.reset(),
        CaptureWriterMessage::Record | CaptureWriterMessage::Barrier(_, _) => {}
    }
    match message {
        CaptureWriterMessage::Record => {
            let (record, admission) = records
                .take()
                .ok_or_else(|| "capture record slot was empty".to_string())?;
            let prepared_material = match material.prepare(&record, admission) {
                MaterialDecision::Suppress => return Ok(CaptureWriterAction::Suppressed),
                MaterialDecision::Retain(prepared) => prepared,
            };
            let kind = match record.direction {
                PevcapDirection::LinkUp => LiveCaptureEventKind::LinkUp,
                PevcapDirection::LinkDown => LiveCaptureEventKind::LinkDown,
                PevcapDirection::Inbound => LiveCaptureEventKind::Notification,
                PevcapDirection::Outbound => LiveCaptureEventKind::Write,
            };
            Ok(CaptureWriterAction::Event(Box::new(
                CaptureWriterEventLine {
                    json_line: record.to_jsonl_line().map_err(|error| error.to_string())?,
                    kind,
                    receipt_monotonic_ms: record.monotonic_ms.as_milliseconds(),
                    source_monotonic_offset_ms: None,
                    source_wall_clock_unix_ms: record
                        .phone_location
                        .map(|location| location.wall_clock_unix_ms),
                    location: None,
                    record: Some(record),
                    prepared_material,
                },
            )))
        }
        CaptureWriterMessage::Location(location) => {
            let validation = location
                .location
                .canonical()
                .map_or_else(LiveCaptureLocationValidation::Rejected, |_| {
                    LiveCaptureLocationValidation::Valid
                });
            Ok(CaptureWriterAction::Event(Box::new(
                CaptureWriterEventLine {
                    json_line: location
                        .to_jsonl_line()
                        .map_err(|error| error.to_string())?,
                    kind: LiveCaptureEventKind::Location,
                    receipt_monotonic_ms: location.receipt_monotonic_ms.as_milliseconds(),
                    source_monotonic_offset_ms: location.source_monotonic_offset_ms,
                    source_wall_clock_unix_ms: (location.location.wall_clock_unix_ms != 0)
                        .then_some(location.location.wall_clock_unix_ms),
                    location: Some(LiveCaptureLocationObservation {
                        location: location.location,
                        raw_source_timestamp_bits: location
                            .source_timestamp_unix_seconds
                            .map(f64::to_bits),
                        simulated: location.simulated,
                        produced_by_accessory: location.produced_by_accessory,
                        validation,
                        admission: LiveCaptureLocationAdmission::NotEvaluated,
                    }),
                    record: None,
                    prepared_material: None,
                },
            )))
        }
        CaptureWriterMessage::Music(music) => Ok(CaptureWriterAction::Event(Box::new(
            CaptureWriterEventLine {
                json_line: music.to_jsonl_line().map_err(|error| error.to_string())?,
                kind: LiveCaptureEventKind::Music,
                receipt_monotonic_ms: music.monotonic_at.as_milliseconds(),
                source_monotonic_offset_ms: None,
                source_wall_clock_unix_ms: Some(music.wall_clock_unix_ms.as_milliseconds()),
                location: None,
                record: None,
                prepared_material: None,
            },
        ))),
        CaptureWriterMessage::Metadata(metadata) => Ok(CaptureWriterAction::Metadata(metadata)),
        CaptureWriterMessage::Barrier(kind, reply) => Ok(CaptureWriterAction::Barrier(kind, reply)),
    }
}

fn flush_capture_barrier(
    path: &Path,
    writer: &mut BufWriter<File>,
    header: &mut PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
    flush: &mut CaptureFlushState,
    state: &CaptureWriterState,
    kind: CaptureBarrier,
) -> Result<(), String> {
    if kind.finishes() {
        close_pending_capture_labels(header, pending_metadata)?;
    }
    if rewrite_pending_capture_metadata(path, writer, header, pending_metadata, state)? {
        *flush = CaptureFlushState::default();
        Ok(())
    } else {
        maybe_flush(writer, flush, true)
    }
}

fn write_capture_event_line(
    writer: &mut BufWriter<File>,
    line: &str,
    state: &CaptureWriterState,
    flush: &mut CaptureFlushState,
) -> Result<(), String> {
    let bytes = write_line(writer, line)? as u64;
    state.bytes_written.fetch_add(bytes, Ordering::AcqRel);
    state
        .physical_bytes_written
        .fetch_add(bytes, Ordering::AcqRel);
    flush.bytes_since_flush = flush.bytes_since_flush.saturating_add(bytes);
    maybe_flush(writer, flush, false)
}

fn reply_capture_writer_result(
    result: Result<CaptureWriterBarrierResult, String>,
    reply: &SyncSender<Result<CaptureWriterBarrierResult, String>>,
) -> Result<(), String> {
    let failure = result.as_ref().err().cloned();
    let _ = reply.send(result);
    failure.map_or(Ok(()), Err)
}

fn digest_open_file(file: &mut File) -> Result<String, String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    file.seek(SeekFrom::End(0))
        .map_err(|error| error.to_string())?;
    Ok(hex::encode(digest.finalize()))
}

fn rewrite_pending_capture_metadata(
    path: &Path,
    writer: &mut BufWriter<File>,
    header: &mut PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
    state: &CaptureWriterState,
) -> Result<bool, String> {
    let Some(mut metadata) = pending_metadata.take() else {
        return Ok(false);
    };
    preserve_policy_annotation(header, &mut metadata);
    let new_header = capture_header(
        header.wall_clock_start_unix_ms,
        header.platform_id.as_str(),
        header.write_limit,
        &metadata,
    )?;
    *header = new_header;
    let bytes = rewrite_capture_header(path, writer, header)?;
    state
        .physical_bytes_written
        .fetch_add(bytes, Ordering::AcqRel);
    Ok(true)
}

/// Finalization owns interval closure, including transport loss and producer drop.
/// A background flush is not an interval boundary.
fn close_pending_capture_labels(
    header: &PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
) -> Result<bool, String> {
    let annotations = pending_metadata
        .as_ref()
        .map_or(header.annotations.as_slice(), |metadata| {
            metadata.annotations.as_slice()
        });
    let mut labels = CaptureLabelState::from_annotations(annotations.iter().map(String::as_str));
    if labels.active().is_empty() {
        return Ok(false);
    }
    if annotations.len() + labels.active().len() > cutout_core::PEVCAP_MAX_ANNOTATIONS {
        return Err("capture annotation capacity cannot retain closing label boundaries".into());
    }
    let metadata = pending_metadata.get_or_insert_with(|| CaptureMetadata {
        advertised_services: header.advertised_services.to_vec(),
        gatt_fingerprints: header.gatt_fingerprints.to_vec(),
        resolved_identity: header.resolved_identity.clone(),
        annotations: header.annotations.to_vec(),
    });
    metadata.annotations.extend(labels.close().map(|boundary| {
        format!(
            "{PEVCAP_CAPTURE_LABEL_ANNOTATION_KEY}={}",
            boundary.annotation_value()
        )
    }));
    Ok(true)
}

fn write_line(writer: &mut BufWriter<File>, line: &str) -> Result<usize, String> {
    writer
        .write_all(line.as_bytes())
        .and_then(|()| writer.write_all(b"\n"))
        .map(|()| line.len() + 1)
        .map_err(|error| error.to_string())
}

fn maybe_flush(
    writer: &mut BufWriter<File>,
    flush: &mut CaptureFlushState,
    force_sync: bool,
) -> Result<(), String> {
    let now = Instant::now();
    if flush.bytes_since_flush >= CAPTURE_WRITER_BUFFER_BYTES
        || now.duration_since(flush.last_flush) >= CAPTURE_WRITER_FLUSH_INTERVAL
        || force_sync
    {
        writer.flush().map_err(|error| error.to_string())?;
        flush.bytes_since_flush = 0;
        flush.last_flush = now;
    }
    if force_sync || now.duration_since(flush.last_sync) >= CAPTURE_WRITER_SYNC_INTERVAL {
        writer
            .get_mut()
            .sync_data()
            .map_err(|error| error.to_string())?;
        flush.last_sync = now;
    }
    Ok(())
}

fn rewrite_capture_header(
    path: &Path,
    writer: &mut BufWriter<File>,
    header: &PevcapHeader,
) -> Result<u64, String> {
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_mut()
        .sync_data()
        .map_err(|error| error.to_string())?;
    let input = File::open(path).map_err(|error| error.to_string())?;
    let permissions = input
        .metadata()
        .map_err(|error| error.to_string())?
        .permissions();
    let mut reader = BufReader::new(input);
    let mut old_header = Vec::new();
    reader
        .read_until(b'\n', &mut old_header)
        .map_err(|error| error.to_string())?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut output = NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    let header_bytes = write_line_to_file(
        output.as_file_mut(),
        &header.to_jsonl_line().map_err(|error| error.to_string())?,
    )?;
    let copied_bytes =
        std::io::copy(&mut reader, output.as_file_mut()).map_err(|error| error.to_string())?;
    output
        .as_file()
        .set_permissions(permissions)
        .map_err(|error| error.to_string())?;
    output
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    sync_parent_directory_after(path, || output.persist(path).map_err(|error| error.error))?;
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    *writer = BufWriter::new(file);
    Ok(header_bytes as u64 + copied_bytes)
}

fn write_line_to_file(file: &mut File, line: &str) -> Result<usize, String> {
    file.write_all(line.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map(|()| line.len() + 1)
        .map_err(|error| error.to_string())
}

/// File data sync does not persist creation or replacement of its directory entry.
fn sync_parent_directory_after<T>(
    path: &Path,
    operation: impl FnOnce() -> std::io::Result<T>,
) -> Result<T, String> {
    let result = operation().map_err(|error| error.to_string())?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn stationary_record(at: u64) -> PevcapRecord {
        PevcapRecord::inbound_notification(
            cutout_core::MonotonicTimestamp::new(at),
            GattChannel::from_bytes([0x11; 16]),
            GattChannel::from_bytes([0x22; 16]),
            vec![0xaa, 0xbb],
        )
        .with_telemetry(cutout_core::RawTelemetryReadback::default())
        .with_semantic_telemetry(cutout_core::PevcapSemanticTelemetry {
            observed_at_ms: Some(cutout_core::MonotonicTimestamp::new(at)),
            provenance: cutout_core::PevcapTelemetryProvenance::LiveSession,
            snapshot_schema_version: 1,
            library_version: "test".into(),
            snapshot_json: format!(
                r#"{{"at_ms":{at},"speed_observed_at_ms":{at},"speed":0,"nested":{{"at_ms":42}}}}"#
            ),
        })
    }

    fn policy_capture(
        policy: CaptureRecordingPolicy,
        observations: Vec<(PevcapRecord, CaptureRecordAdmission)>,
    ) -> Vec<serde_json::Value> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let writer = CaptureWriter::start_with_database_and_policy(
            path.clone(),
            WallClockUnixTimestamp::new(100),
            "test",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
            database,
            policy,
        )
        .unwrap();
        let original_lines = observations
            .iter()
            .map(|(record, _)| record.to_jsonl_line().unwrap())
            .collect::<Vec<_>>();
        let ingress = writer.ingress();
        for (record, admission) in observations {
            assert_eq!(
                ingress.try_send_record_with_admission(record, admission),
                CaptureWriteOutcome::Accepted
            );
        }
        let completion = writer.finish().unwrap();
        let CaptureWriterFinish::DatabaseFinished {
            integrity: LiveCaptureIntegrity::Complete,
            jsonl_export: CaptureJsonlExport::Available(_),
            status,
            ..
        } = completion
        else {
            panic!("policy capture did not finish complete with an export");
        };
        assert_eq!(status.dropped_messages, 0);
        assert_eq!(status.queued_messages, 0);
        assert!(!status.failed);
        assert_eq!(status.bytes_written, fs::metadata(&path).unwrap().len());
        let contents = fs::read_to_string(path).unwrap();
        for line in contents.lines().skip(1) {
            assert!(
                original_lines.iter().any(|original| original == line),
                "export changed a retained record"
            );
        }
        contents
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn material_policy_suppresses_only_clock_changes_after_a_committed_baseline() {
        let records = policy_capture(
            CaptureRecordingPolicy::MaterialChanges,
            (1..=3)
                .map(|at| {
                    (
                        stationary_record(at),
                        CaptureRecordAdmission::StationaryTelemetry,
                    )
                })
                .collect(),
        );
        assert_eq!(records.len(), 2);
        assert_eq!(records[1]["record"]["monotonic_ms"], 1);
        assert!(
            records[0]
                .to_string()
                .contains("capture_recording_policy=material_changes")
        );
    }

    #[test]
    fn every_observation_policy_preserves_stationary_duplicates() {
        let records = policy_capture(
            CaptureRecordingPolicy::EveryObservation,
            (1..=3)
                .map(|at| {
                    (
                        stationary_record(at),
                        CaptureRecordAdmission::StationaryTelemetry,
                    )
                })
                .collect(),
        );
        assert_eq!(records.len(), 4);
    }

    #[test]
    fn unclassified_fragment_breaks_the_material_baseline() {
        let records = policy_capture(
            CaptureRecordingPolicy::MaterialChanges,
            vec![
                (
                    stationary_record(1),
                    CaptureRecordAdmission::StationaryTelemetry,
                ),
                (
                    stationary_record(2),
                    CaptureRecordAdmission::EveryObservation,
                ),
                (
                    stationary_record(3),
                    CaptureRecordAdmission::StationaryTelemetry,
                ),
                (
                    stationary_record(4),
                    CaptureRecordAdmission::StationaryTelemetry,
                ),
            ],
        );
        assert_eq!(records.len(), 4);
        assert_eq!(records[3]["record"]["monotonic_ms"], 3);
    }

    #[test]
    fn material_policy_preserves_payload_metadata_and_unknown_clock_changes() {
        let original = stationary_record(1);
        let mut changed = stationary_record(2);
        changed.bytes = vec![0xaa, 0xcc].into();
        let mut metadata = stationary_record(3);
        metadata.service = Some(GattChannel::from_bytes([0x33; 16]));
        let mut nested_clock = stationary_record(4);
        nested_clock
            .semantic_telemetry
            .as_mut()
            .unwrap()
            .snapshot_json =
            r#"{"at_ms":4,"speed_observed_at_ms":4,"speed":0,"nested":{"at_ms":43}}"#.into();
        let mut missing_clock = stationary_record(5);
        missing_clock
            .semantic_telemetry
            .as_mut()
            .unwrap()
            .snapshot_json = r#"{"at_ms":5,"speed":0,"nested":{"at_ms":42}}"#.into();
        let mut absent_observation = stationary_record(6);
        absent_observation
            .semantic_telemetry
            .as_mut()
            .unwrap()
            .observed_at_ms = None;
        let records = policy_capture(
            CaptureRecordingPolicy::MaterialChanges,
            vec![
                original,
                changed,
                metadata,
                nested_clock,
                missing_clock,
                absent_observation,
            ]
            .into_iter()
            .map(|record| (record, CaptureRecordAdmission::StationaryTelemetry))
            .collect(),
        );
        assert_eq!(records.len(), 7);
    }

    #[test]
    fn default_admission_retains_every_record_in_material_policy() {
        let records = policy_capture(
            CaptureRecordingPolicy::MaterialChanges,
            (1..=3)
                .map(|at| (stationary_record(at), CaptureRecordAdmission::default()))
                .collect(),
        );
        assert_eq!(records.len(), 4);
    }

    #[test]
    fn later_metadata_cannot_change_the_capture_recording_contract() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        };
        let writer = CaptureWriter::start_with_database_and_policy(
            path.clone(),
            WallClockUnixTimestamp::new(100),
            "test",
            None,
            &metadata,
            database,
            CaptureRecordingPolicy::MaterialChanges,
        )
        .unwrap();
        let mut replacement = metadata;
        replacement
            .annotations
            .push("capture_recording_policy=every_observation".into());
        assert_eq!(
            writer.update_metadata(replacement),
            CaptureWriteOutcome::Accepted
        );
        writer.finish().unwrap();
        let capture = fs::read_to_string(path).unwrap();
        assert!(capture.contains("capture_recording_policy=material_changes"));
        assert!(!capture.contains("capture_recording_policy=every_observation"));
    }

    #[test]
    fn material_policy_rejects_excess_metadata_before_it_can_poison_finalization() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        };
        let writer = CaptureWriter::start_with_database_and_policy(
            path.clone(),
            WallClockUnixTimestamp::new(100),
            "test",
            None,
            &metadata,
            database,
            CaptureRecordingPolicy::MaterialChanges,
        )
        .unwrap();
        let mut excess = metadata;
        excess.annotations = (0..cutout_core::PEVCAP_MAX_ANNOTATIONS)
            .map(|index| format!("note={index}"))
            .collect();
        assert_eq!(writer.update_metadata(excess), CaptureWriteOutcome::Failed);
        assert!(!writer.monitor().status().failed);
        assert_eq!(writer.monitor().status().dropped_messages, 0);
        writer.finish().unwrap();
        let capture = fs::read_to_string(path).unwrap();
        assert!(capture.contains("capture_recording_policy=material_changes"));
        assert!(!capture.contains("note=0"));
    }

    #[test]
    fn material_policy_reserves_active_label_closures_before_metadata_admission() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        };
        let writer = CaptureWriter::start_with_database_and_policy(
            path.clone(),
            WallClockUnixTimestamp::new(100),
            "test",
            None,
            &metadata,
            database,
            CaptureRecordingPolicy::MaterialChanges,
        )
        .unwrap();
        let mut excess = metadata;
        excess.annotations = vec!["note=test".into(); cutout_core::PEVCAP_MAX_ANNOTATIONS - 2];
        excess.annotations.push("capture_label=ride_start".into());
        assert_eq!(writer.update_metadata(excess), CaptureWriteOutcome::Failed);
        assert!(!writer.monitor().status().failed);
        assert_eq!(writer.monitor().status().dropped_messages, 0);
        writer.finish().unwrap();
        let capture = fs::read_to_string(path).unwrap();
        assert!(!capture.contains("capture_label=ride_start"));
    }

    #[test]
    fn default_capture_keeps_full_annotation_capacity_and_cannot_forge_material_policy() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let mut metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: (0..cutout_core::PEVCAP_MAX_ANNOTATIONS)
                .map(|index| format!("note={index}"))
                .collect(),
        };
        let writer = CaptureWriter::start_with_database(
            path.clone(),
            WallClockUnixTimestamp::new(100),
            "test",
            None,
            &metadata,
            database,
        )
        .unwrap();
        writer.flush().unwrap();
        metadata.annotations[0] = "capture_recording_policy=material_changes".into();
        assert_eq!(
            writer.update_metadata(metadata),
            CaptureWriteOutcome::Accepted
        );
        writer.finish().unwrap();
        let capture = fs::read_to_string(path).unwrap();
        assert!(!capture.contains("capture_recording_policy="));
        assert!(capture.contains("note=7"));
    }

    #[test]
    fn header_rewrite_preserves_unowned_staging_sibling_and_record_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let sibling = path.with_extension("jsonl.tmp");
        fs::write(&sibling, b"unrelated staging contents").unwrap();
        let mut metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        };
        let writer = CaptureWriter::start(
            path.clone(),
            WallClockUnixTimestamp::new(0),
            "test",
            None,
            &metadata,
        )
        .unwrap();
        assert_eq!(
            writer.try_send_record(PevcapRecord::link_up(
                cutout_core::MonotonicTimestamp::new(1),
                None,
            )),
            CaptureWriteOutcome::Accepted
        );
        writer.flush().unwrap();
        let original = fs::read_to_string(&path).unwrap();
        let (_, original_records) = original.split_once('\n').unwrap();
        #[cfg(unix)]
        let original_mode = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(&path).unwrap().permissions().mode()
        };

        metadata.annotations.push("note=updated".into());
        assert_eq!(
            writer.update_metadata(metadata),
            CaptureWriteOutcome::Accepted
        );
        writer.flush().unwrap();
        assert_eq!(fs::read(&sibling).unwrap(), b"unrelated staging contents");
        let rewritten = fs::read_to_string(&path).unwrap();
        let (_, rewritten_records) = rewritten.split_once('\n').unwrap();
        assert_eq!(rewritten_records, original_records);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode(),
                original_mode
            );
        }

        assert_eq!(
            writer.try_send_record(PevcapRecord::link_down(
                cutout_core::MonotonicTimestamp::new(2),
            )),
            CaptureWriteOutcome::Accepted
        );
        writer.finish().unwrap();
        let capture = cutout_core::PevcapCapture::decode(
            &fs::read(path).unwrap(),
            cutout_core::PevcapEncoding::Jsonl,
        )
        .unwrap();
        assert_eq!(capture.records.len(), 2);
        assert_eq!(capture.header.annotations.as_slice(), &["note=updated"]);
    }

    #[test]
    fn directory_sync_accepts_relative_file_names_and_preserves_operation_errors() {
        assert_eq!(
            sync_parent_directory_after(Path::new("capture.jsonl"), || Ok(42)).unwrap(),
            42
        );
        let error = sync_parent_directory_after(Path::new("capture.jsonl"), || {
            Err::<(), _>(std::io::Error::other("creation rejected"))
        })
        .unwrap_err();
        assert_eq!(error, "creation rejected");
    }

    #[test]
    fn capture_with_no_room_for_label_closure_cannot_produce_a_saved_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let mut annotations = vec!["note=test".into(); cutout_core::PEVCAP_MAX_ANNOTATIONS - 1];
        annotations.push("capture_label=ride_start".into());
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations,
        };
        let result = CaptureWriter::start(
            path.clone(),
            WallClockUnixTimestamp::new(0),
            "test",
            None,
            &metadata,
        );
        let Err(error) = result else {
            panic!("invalid label capacity opened a capture writer");
        };
        assert!(error.contains("closing label boundaries"));
        assert!(
            !path.exists(),
            "invalid metadata must not create an artifact"
        );
    }

    #[test]
    fn cloned_ingress_rejects_locations_after_writer_finalization() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let writer = CaptureWriter::start(
            path.clone(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
        )
        .unwrap();
        let ingress = writer.ingress();
        let location = PevcapLocationSample::new(
            cutout_core::MonotonicTimestamp::new(12),
            cutout_core::PevcapPhoneLocation {
                wall_clock_unix_ms: 1_700_000_000_012,
                latitude_degrees: 39.7,
                longitude_degrees: -104.9,
                altitude_meters: 1.0,
                horizontal_accuracy_meters: None,
                vertical_accuracy_meters: None,
                speed_meters_per_second: None,
                speed_accuracy_meters_per_second: None,
                course_degrees: None,
                course_accuracy_degrees: None,
            },
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            ingress.record_location(location),
            CaptureWriteOutcome::Accepted
        );
        writer.finish().unwrap();
        assert_eq!(
            ingress.record_location(location),
            CaptureWriteOutcome::Failed
        );
        assert_eq!(
            cutout_core::PevcapCapture::decode(
                &fs::read(path).unwrap(),
                cutout_core::PevcapEncoding::Jsonl
            )
            .unwrap()
            .locations
            .len(),
            1
        );
    }

    #[test]
    fn finalization_uses_latest_metadata_and_does_not_duplicate_closed_intervals() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let mut metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec!["capture_label=ride_start".into()],
        };
        let writer = CaptureWriter::start(
            path.clone(),
            WallClockUnixTimestamp::new(0),
            "test",
            None,
            &metadata,
        )
        .unwrap();
        metadata.annotations.extend([
            "capture_label=ride_stop".into(),
            "capture_label=balancing_start".into(),
        ]);
        assert_eq!(
            writer.update_metadata(metadata),
            CaptureWriteOutcome::Accepted
        );
        writer.finish().unwrap();
        let capture = fs::read_to_string(path).unwrap();
        assert_eq!(capture.matches("capture_label=ride_stop").count(), 1);
        assert_eq!(capture.matches("capture_label=balancing_stop").count(), 1);
    }

    #[test]
    fn directory_sync_failure_is_reported_after_successful_file_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let result = sync_parent_directory_after(&path, || {
            let file = File::create(&path)?;
            fs::remove_file(&path)?;
            fs::remove_dir(directory.path())?;
            Ok(file)
        });
        assert!(
            result.is_err(),
            "successful file creation must not mask directory-open failure"
        );
    }

    #[test]
    fn finishing_capture_closes_labels_but_background_flush_does_not() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![
                "capture_label=ride_start".into(),
                "capture_label=balancing_start".into(),
            ],
        };
        let writer = CaptureWriter::start(
            path.clone(),
            WallClockUnixTimestamp::new(0),
            "test",
            None,
            &metadata,
        )
        .unwrap();
        writer.flush().unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("ride_stop"));
        let artifact = match writer.finish().unwrap() {
            CaptureWriterFinish::FileSaved(artifact) => artifact,
            CaptureWriterFinish::DatabaseFinished { .. } => {
                panic!("file-only capture unexpectedly used SQLite")
            }
        };
        let capture = fs::read_to_string(artifact.path()).unwrap();
        assert_eq!(capture.matches("capture_label=ride_stop").count(), 1);
        assert_eq!(capture.matches("capture_label=balancing_stop").count(), 1);
    }

    #[test]
    fn database_capture_with_admission_loss_is_durable_but_not_exported() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let artifact_path = directory.path().join("capture.jsonl");
        let database = RideDatabase::open(&database_path).unwrap();
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        };
        let writer = CaptureWriter::start_with_database(
            artifact_path.clone(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test",
            None,
            &metadata,
            database.clone(),
        )
        .unwrap();
        writer.flush().unwrap();
        assert_eq!(
            writer.record_location(database_test_location()),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(
            writer.try_send_record(PevcapRecord::inbound_notification(
                cutout_core::MonotonicTimestamp::new(11),
                GattChannel::from_bytes([0x11; 16]),
                GattChannel::from_bytes([0x22; 16]),
                vec![0xaa, 0xbb],
            )),
            CaptureWriteOutcome::Accepted
        );
        let rejected_locations =
            vec![database_test_location(); CAPTURE_LOCATION_BATCH_CAPACITY + 1];
        assert_eq!(
            writer.ingress().record_location_batch(&rejected_locations),
            CaptureWriteOutcome::AdmissionLost {
                dropped_messages: 65
            }
        );
        assert!(!writer.monitor().status().failed);
        assert_eq!(writer.monitor().status().last_error, None);
        assert_eq!(
            writer.try_send_record(PevcapRecord::link_down(
                cutout_core::MonotonicTimestamp::new(12)
            )),
            CaptureWriteOutcome::Accepted,
            "known loss must not prevent retaining subsequent data"
        );
        writer.flush().unwrap();
        let capture_id = writer.live_capture_id.unwrap();

        let completion = writer
            .finish()
            .expect("incomplete SQLite capture is still durably finalized");
        match completion {
            CaptureWriterFinish::DatabaseFinished {
                integrity:
                    LiveCaptureIntegrity::Incomplete {
                        dropped_messages: 65,
                    },
                jsonl_export: CaptureJsonlExport::NotAttempted,
                status,
                ..
            } => {
                assert_eq!(status.dropped_messages, 65);
                assert_eq!(status.queued_messages, 0);
                assert!(!status.failed);
                assert_eq!(status.last_error, None);
            }
            other => panic!("unexpected completion: {other:?}"),
        }
        assert!(!artifact_path.exists());
        let snapshot = database
            .live_capture(capture_id, QueryLimit::new(10).unwrap())
            .unwrap();
        assert_eq!(snapshot.state, LiveCaptureState::Finished);
        assert_eq!(
            snapshot.integrity,
            LiveCaptureIntegrity::Incomplete {
                dropped_messages: 65
            }
        );
        assert_eq!(snapshot.events.len(), 3);
        assert_eq!(snapshot.events[2].kind, LiveCaptureEventKind::LinkDown);
        let connection = rusqlite::Connection::open(&database_path).unwrap();
        let structured: (String, Vec<u8>, Vec<u8>) = connection
            .query_row(
                "SELECT ble.direction, ble.characteristic_uuid, ble.transport_payload
                 FROM live_capture_ble_observations AS ble
                 WHERE ble.capture_id = ?1 AND ble.sequence = 1",
                [capture_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(structured.0, "inbound");
        assert_eq!(structured.1, [0x11; 16]);
        assert_eq!(structured.2, [0xaa, 0xbb]);
        database.shutdown().unwrap();
    }

    #[test]
    fn database_export_collision_does_not_hide_durable_capture_completion() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let artifact_path = directory.path().join("capture.jsonl");
        let database = RideDatabase::open(&database_path).unwrap();
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        };
        let writer = CaptureWriter::start_with_database(
            artifact_path.clone(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test",
            None,
            &metadata,
            database.clone(),
        )
        .unwrap();
        let capture_id = writer.live_capture_id.unwrap();
        writer.flush().unwrap();
        fs::write(&artifact_path, b"existing user file").unwrap();

        let completion = writer
            .finish()
            .expect("JSONL export failure must not erase SQLite finalization");
        match completion {
            CaptureWriterFinish::DatabaseFinished {
                live_capture_id: completed_id,
                integrity: LiveCaptureIntegrity::Complete,
                jsonl_export: CaptureJsonlExport::Failed(_),
                ..
            } => assert_eq!(completed_id, capture_id),
            other => panic!("unexpected completion: {other:?}"),
        }
        assert_eq!(fs::read(&artifact_path).unwrap(), b"existing user file");

        let snapshot = database
            .live_capture(capture_id, QueryLimit::new(10).unwrap())
            .unwrap();
        assert_eq!(snapshot.state, LiveCaptureState::Finished);
        assert_eq!(snapshot.integrity, LiveCaptureIntegrity::Complete);
        database.shutdown().unwrap();
    }

    fn database_test_location() -> PevcapLocationSample {
        PevcapLocationSample::new(
            cutout_core::MonotonicTimestamp::new(10),
            cutout_core::PevcapPhoneLocation {
                wall_clock_unix_ms: 1_700_000_000_010,
                latitude_degrees: 39.7,
                longitude_degrees: -104.9,
                altitude_meters: 1.0,
                horizontal_accuracy_meters: None,
                vertical_accuracy_meters: None,
                speed_meters_per_second: None,
                speed_accuracy_meters_per_second: None,
                course_degrees: None,
                course_accuracy_degrees: None,
            },
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn callbacks_rejected_after_close_do_not_change_durable_integrity() {
        let (sender, _receiver) = sync_channel(1);
        let state = Arc::new(CaptureWriterState::default());
        let ingress = CaptureWriterIngress {
            sender,
            records: Arc::new(CaptureRecordPool::new(1)),
            state: Arc::clone(&state),
            accepting: Arc::new(Mutex::new(false)),
        };

        assert_eq!(
            ingress.record_location(database_test_location()),
            CaptureWriteOutcome::Failed
        );
        assert_eq!(state.status().dropped_messages, 1);
        assert_eq!(state.incomplete_messages.load(Ordering::Acquire), 0);
        assert_eq!(
            ingress.reject_location_batch(0),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(
            ingress.reject_location_batch(65),
            CaptureWriteOutcome::Failed
        );
        assert_eq!(state.status().dropped_messages, 66);
        assert_eq!(state.incomplete_messages.load(Ordering::Acquire), 0);
        assert!(!state.status().failed);
    }

    #[test]
    fn partial_location_batch_loss_counts_only_rejected_suffix() {
        let (sender, receiver) = sync_channel(2);
        let state = Arc::new(CaptureWriterState::default());
        let ingress = CaptureWriterIngress {
            sender,
            records: Arc::new(CaptureRecordPool::new(2)),
            state: Arc::clone(&state),
            accepting: Arc::new(Mutex::new(true)),
        };
        let location = database_test_location();
        assert_eq!(
            ingress.record_location(location),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(
            ingress.record_location_batch(&[location; 4]),
            CaptureWriteOutcome::AdmissionLost {
                dropped_messages: 3
            }
        );
        let status = state.status();
        assert_eq!(status.queued_messages, 2);
        assert_eq!(status.dropped_messages, 3);
        assert_eq!(state.incomplete_messages.load(Ordering::Acquire), 3);
        assert!(!status.failed);
        assert_eq!(status.last_error, None);
        for _ in 0..2 {
            let CaptureWriterMessage::Location(retained) = receiver.recv().unwrap() else {
                panic!("accepted location was not retained in its queue slot");
            };
            assert_eq!(
                retained.to_jsonl_line().unwrap(),
                location.to_jsonl_line().unwrap()
            );
            state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        }
        assert_eq!(
            ingress.record_location(location),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(state.incomplete_messages.load(Ordering::Acquire), 3);
    }

    #[test]
    fn record_pool_loss_retains_the_previous_accepted_record() {
        let (sender, receiver) = sync_channel(2);
        let state = Arc::new(CaptureWriterState::default());
        let records = Arc::new(CaptureRecordPool::new(1));
        let ingress = CaptureWriterIngress {
            sender,
            records: Arc::clone(&records),
            state: Arc::clone(&state),
            accepting: Arc::new(Mutex::new(true)),
        };
        let accepted = stationary_record(1);
        assert_eq!(
            ingress.try_send_record(accepted.clone()),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(
            ingress.try_send_record(stationary_record(2)),
            CaptureWriteOutcome::AdmissionLost {
                dropped_messages: 1
            }
        );
        assert!(matches!(
            receiver.recv().unwrap(),
            CaptureWriterMessage::Record
        ));
        assert_eq!(records.take().unwrap().0, accepted);
        state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        assert_eq!(
            ingress.try_send_record(stationary_record(3)),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(state.incomplete_messages.load(Ordering::Acquire), 1);
        assert!(!state.status().failed);
    }

    #[test]
    fn file_capture_with_admission_loss_refuses_a_saved_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.jsonl");
        let writer = CaptureWriter::start(
            path.clone(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
        )
        .unwrap();
        assert_eq!(
            writer.ingress().reject_location_batch(0),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(writer.monitor().status().dropped_messages, 0);
        assert_eq!(
            writer
                .ingress()
                .record_location_batch(&[database_test_location(); 65]),
            CaptureWriteOutcome::AdmissionLost {
                dropped_messages: 65
            }
        );
        let retained = PevcapRecord::link_down(cutout_core::MonotonicTimestamp::new(12));
        let retained_line = retained.to_jsonl_line().unwrap();
        assert_eq!(
            writer.try_send_record(retained),
            CaptureWriteOutcome::Accepted
        );
        writer.flush().unwrap();
        assert!(!writer.monitor().status().failed);
        assert!(
            writer
                .finish()
                .unwrap_err()
                .contains("rejected at admission")
        );
        let contents = fs::read_to_string(path).unwrap();
        assert_eq!(contents.lines().nth(1), Some(retained_line.as_str()));
        assert_eq!(contents.lines().count(), 2);
    }

    fn controlled_flush_writer() -> (CaptureWriter, Receiver<CaptureWriterMessage>) {
        let (sender, receiver) = sync_channel(1);
        let state = Arc::new(CaptureWriterState::default());
        let writer = CaptureWriter {
            artifact_id: CaptureArtifactId(Uuid::new_v4()),
            live_capture_id: None,
            recording_policy: CaptureRecordingPolicy::EveryObservation,
            path: PathBuf::new(),
            ingress: CaptureWriterIngress {
                sender,
                records: Arc::new(CaptureRecordPool::new(1)),
                state: Arc::clone(&state),
                accepting: Arc::new(Mutex::new(true)),
            },
            state,
            join: None,
        };
        (writer, receiver)
    }

    #[test]
    fn accepted_flush_reply_loss_marks_writer_fatal() {
        let (writer, receiver) = controlled_flush_writer();
        let state = Arc::clone(&writer.state);
        let worker = thread::spawn(move || {
            let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
            state.queued_messages.fetch_sub(1, Ordering::AcqRel);
            let CaptureWriterMessage::Barrier(CaptureBarrier::Flush, reply) = message else {
                panic!("flush did not enqueue a barrier");
            };
            drop(reply);
        });
        assert!(writer.flush().is_err());
        worker.join().unwrap();
        assert!(
            writer.monitor().status().failed,
            "an accepted barrier without a durable receipt must fail closed"
        );
        assert_eq!(
            writer.monitor().status().last_error.as_deref(),
            Some("capture writer stopped before flush")
        );
    }

    #[test]
    fn accepted_flush_storage_error_marks_writer_fatal() {
        let (writer, receiver) = controlled_flush_writer();
        let state = Arc::clone(&writer.state);
        let worker = thread::spawn(move || {
            let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
            state.queued_messages.fetch_sub(1, Ordering::AcqRel);
            let CaptureWriterMessage::Barrier(CaptureBarrier::Flush, reply) = message else {
                panic!("flush did not enqueue a barrier");
            };
            reply.send(Err("test durable flush failed".into())).unwrap();
        });
        assert_eq!(writer.flush().unwrap_err(), "test durable flush failed");
        worker.join().unwrap();
        assert!(
            writer.monitor().status().failed,
            "storage rejected an accepted durability barrier"
        );
        assert_eq!(
            writer.monitor().status().last_error.as_deref(),
            Some("test durable flush failed")
        );
    }

    #[test]
    fn full_flush_queue_is_retryable_without_capture_loss() {
        let (sender, receiver) = sync_channel(1);
        let state = Arc::new(CaptureWriterState::default());
        let records = Arc::new(CaptureRecordPool::new(1));
        let ingress = CaptureWriterIngress {
            sender: sender.clone(),
            records,
            state: Arc::clone(&state),
            accepting: Arc::new(Mutex::new(true)),
        };
        let writer = CaptureWriter {
            artifact_id: CaptureArtifactId(Uuid::new_v4()),
            live_capture_id: None,
            recording_policy: CaptureRecordingPolicy::EveryObservation,
            path: PathBuf::new(),
            ingress,
            state: Arc::clone(&state),
            join: None,
        };

        let (reply, _result) = sync_channel(0);
        assert_eq!(
            writer.try_send(CaptureWriterMessage::Barrier(CaptureBarrier::Flush, reply)),
            CaptureWriteOutcome::Accepted
        );
        assert_eq!(writer.flush_outcome(), CaptureFlushOutcome::Rejected);
        let status = state.status();
        assert_eq!(status.queued_messages, 1);
        assert_eq!(status.peak_queued_messages, 1);
        assert_eq!(status.dropped_messages, 1);
        assert_eq!(state.incomplete_messages.load(Ordering::Acquire), 0);
        assert!(
            !status.failed,
            "a rejected control request does not stop the worker"
        );
        assert_eq!(status.last_error, None);

        drop(receiver.recv().unwrap());
        state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        let worker_state = Arc::clone(&state);
        let worker = thread::spawn(move || {
            let message = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
            worker_state.queued_messages.fetch_sub(1, Ordering::AcqRel);
            let CaptureWriterMessage::Barrier(CaptureBarrier::Flush, reply) = message else {
                panic!("retry did not enqueue a flush barrier");
            };
            reply.send(Ok(CaptureWriterBarrierResult::Flushed)).unwrap();
        });
        assert_eq!(writer.flush_outcome(), CaptureFlushOutcome::Flushed);
        worker.join().unwrap();
        assert_eq!(state.status().queued_messages, 0);
        assert!(!state.status().failed);
    }

    #[test]
    fn disconnected_finish_preserves_the_first_worker_error() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("ride.sqlite");
        let database = RideDatabase::open(&database_path).unwrap();
        let mut writer = CaptureWriter::start_with_database(
            directory.path().join("capture.jsonl"),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "test",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
            database.clone(),
        )
        .unwrap();
        writer.flush().unwrap();
        assert_eq!(
            writer.ingress().reject_location_batch(1),
            CaptureWriteOutcome::AdmissionLost {
                dropped_messages: 1
            }
        );
        assert!(!writer.monitor().status().failed);
        let connection = rusqlite::Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_capture_event BEFORE INSERT ON live_capture_events
                 BEGIN SELECT RAISE(ABORT, 'injected capture append failure'); END;",
            )
            .unwrap();
        assert_eq!(
            writer.record_location(database_test_location()),
            CaptureWriteOutcome::Accepted
        );
        // Join the known worker so the failure and channel closure are deterministic.
        writer.join.take().unwrap().join().unwrap();
        let monitor = writer.monitor();
        let initial_error = monitor.status().last_error.unwrap();
        assert!(initial_error.contains("injected capture append failure"));
        let error = writer.finish().unwrap_err();
        assert_eq!(error, initial_error);
        assert_eq!(
            monitor.status().last_error.as_deref(),
            Some(initial_error.as_str())
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn terminal_message_waits_for_queue_capacity() {
        let (sender, receiver) = sync_channel(1);
        let state = Arc::new(CaptureWriterState::default());
        let metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        };
        assert_eq!(
            try_send_message(&sender, &state, CaptureWriterMessage::Metadata(metadata)),
            CaptureWriteOutcome::Accepted
        );

        let (reply, _reply_receiver) = sync_channel(0);
        let finish_state = Arc::clone(&state);
        let finish = thread::spawn(move || {
            send_terminal_message(
                &sender,
                &finish_state,
                CaptureWriterMessage::Barrier(CaptureBarrier::Finish, reply),
            )
        });

        assert!(matches!(
            receiver.recv().unwrap(),
            CaptureWriterMessage::Metadata(_)
        ));
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            CaptureWriterMessage::Barrier(CaptureBarrier::Finish, _)
        ));
        assert_eq!(finish.join().unwrap(), CaptureWriteOutcome::Accepted);
        assert_eq!(state.status().dropped_messages, 0);
    }

    #[test]
    fn capture_writer_status_retains_peak_accepted_queue_depth() {
        let (sender, _receiver) = sync_channel(1);
        let state = Arc::new(CaptureWriterState::default());
        let records = Arc::new(CaptureRecordPool::new(1));
        let ingress = CaptureWriterIngress {
            sender: sender.clone(),
            records,
            state: Arc::clone(&state),
            accepting: Arc::new(Mutex::new(true)),
        };
        let writer = CaptureWriter {
            artifact_id: CaptureArtifactId(Uuid::new_v4()),
            live_capture_id: None,
            recording_policy: CaptureRecordingPolicy::EveryObservation,
            path: PathBuf::new(),
            ingress,
            state: Arc::clone(&state),
            join: None,
        };

        let (reply, _result) = sync_channel(0);
        assert_eq!(
            writer.try_send(CaptureWriterMessage::Barrier(CaptureBarrier::Flush, reply)),
            CaptureWriteOutcome::Accepted
        );
        let status = state.status();
        assert_eq!(status.queued_messages, 1);
        assert_eq!(status.peak_queued_messages, 1);
        assert_eq!(status.dropped_messages, 0);
    }
}
