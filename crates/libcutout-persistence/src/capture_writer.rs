//! Bounded PEVCAP file writer and durable completion evidence.
//! This is the existing streaming writer, independent of any mobile binding or UI.

use cutout_core::{
    CaptureLabelState, GattChannel, GattFingerprint, PEVCAP_CAPTURE_LABEL_ANNOTATION_KEY,
    PevcapHeader, PevcapLocationSample, PevcapMusicEvent, PevcapRecord, PevcapResolvedIdentity,
    TransportWriteLimit, WallClockUnixTimestamp,
};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Rust-generated identity of one capture artifact, independent of filenames and devices.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CaptureArtifactId(Uuid);

impl std::fmt::Display for CaptureArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
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
    path: PathBuf,
    status: CaptureWriterStatus,
}

impl SavedCaptureArtifact {
    /// Identity assigned when the writer was created.
    #[must_use]
    pub const fn id(&self) -> CaptureArtifactId {
        self.id
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
const CAPTURE_WRITER_BUFFER_BYTES: u64 = 128 * 1024;
const CAPTURE_WRITER_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
const CAPTURE_WRITER_SYNC_INTERVAL: Duration = Duration::from_secs(3);

/// Rust-owned status for the bounded capture writer queue.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptureWriterStatus {
    /// Messages accepted by the queue and not yet written.
    pub queued_messages: u64,
    /// Highest number of accepted messages waiting to be written.
    pub peak_queued_messages: u64,
    /// Messages rejected because the queue was full or closed.
    pub dropped_messages: u64,
    /// Bytes written to the capture file.
    pub bytes_written: u64,
    /// Total successful write payload bytes, including header rewrites.
    pub physical_bytes_written: u64,
    /// Whether the writer has encountered an unrecoverable error.
    pub failed: bool,
    /// Last writer error, if one exists.
    pub last_error: Option<String>,
}

#[derive(Debug)]
struct CaptureWriterState {
    queued_messages: AtomicU64,
    peak_queued_messages: AtomicU64,
    dropped_messages: AtomicU64,
    bytes_written: AtomicU64,
    physical_bytes_written: AtomicU64,
    failed: AtomicBool,
    last_error: Mutex<Option<String>>,
}

impl Default for CaptureWriterState {
    fn default() -> Self {
        Self {
            queued_messages: AtomicU64::new(0),
            peak_queued_messages: AtomicU64::new(0),
            dropped_messages: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            physical_bytes_written: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            last_error: Mutex::new(None),
        }
    }
}

impl CaptureWriterState {
    fn fail(&self, error: impl Into<String>) {
        self.failed.store(true, Ordering::Release);
        *self
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(error.into());
    }

    fn status(&self) -> CaptureWriterStatus {
        CaptureWriterStatus {
            queued_messages: self.queued_messages.load(Ordering::Acquire),
            peak_queued_messages: self.peak_queued_messages.load(Ordering::Acquire),
            dropped_messages: self.dropped_messages.load(Ordering::Acquire),
            bytes_written: self.bytes_written.load(Ordering::Acquire),
            physical_bytes_written: self.physical_bytes_written.load(Ordering::Acquire),
            failed: self.failed.load(Ordering::Acquire),
            last_error: self
                .last_error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
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
    Barrier(CaptureBarrier, SyncSender<Result<(), String>>),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CaptureBarrier {
    Flush,
    Finish,
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
    records: Mutex<VecDeque<PevcapRecord>>,
}

impl CaptureRecordPool {
    fn new(capacity: usize) -> Self {
        Self {
            records: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn take(&self) -> Option<PevcapRecord> {
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
    path: PathBuf,
    sender: SyncSender<CaptureWriterMessage>,
    records: Arc<CaptureRecordPool>,
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
        let header = capture_header(wall_clock_start_unix_ms, platform_id, write_limit, metadata)?;
        let file = sync_parent_directory_after(&path, || {
            OpenOptions::new().create_new(true).write(true).open(&path)
        })?;
        let (sender, receiver) = sync_channel(CAPTURE_WRITER_QUEUE_CAPACITY);
        let records = Arc::new(CaptureRecordPool::new(CAPTURE_WRITER_QUEUE_CAPACITY));
        let state = Arc::new(CaptureWriterState::default());
        let thread_records = Arc::clone(&records);
        let thread_state = Arc::clone(&state);
        let artifact_path = path.clone();
        let join = thread::Builder::new()
            .name("cutout-pevcap-writer".into())
            .spawn(move || {
                run_capture_writer(
                    &path,
                    file,
                    header,
                    &receiver,
                    &thread_records,
                    &thread_state,
                );
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            artifact_id: CaptureArtifactId(Uuid::new_v4()),
            path: artifact_path,
            sender,
            records,
            state,
            join: Some(join),
        })
    }

    fn try_send(&self, message: CaptureWriterMessage) -> bool {
        let queued_messages = self.state.queued_messages.fetch_add(1, Ordering::AcqRel) + 1;
        match self.sender.try_send(message) {
            Ok(()) => {
                self.state
                    .peak_queued_messages
                    .fetch_max(queued_messages, Ordering::AcqRel);
                true
            }
            Err(TrySendError::Full(_)) => {
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer queue is full");
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer stopped");
                false
            }
        }
    }

    /// Admits a transport record without waiting for disk I/O.
    pub fn try_send_record(&self, record: PevcapRecord) -> bool {
        let mut records = self
            .records
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if records.len() == records.capacity() {
            self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
            self.state.fail("capture writer queue is full");
            return false;
        }
        records.push_back(record);
        let queued_messages = self.state.queued_messages.fetch_add(1, Ordering::AcqRel) + 1;
        match self.sender.try_send(CaptureWriterMessage::Record) {
            Ok(()) => {
                self.state
                    .peak_queued_messages
                    .fetch_max(queued_messages, Ordering::AcqRel);
                true
            }
            Err(TrySendError::Full(CaptureWriterMessage::Record)) => {
                records.pop_back();
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer queue is full");
                false
            }
            Err(TrySendError::Disconnected(CaptureWriterMessage::Record)) => {
                records.pop_back();
                self.state.queued_messages.fetch_sub(1, Ordering::AcqRel);
                self.state.dropped_messages.fetch_add(1, Ordering::AcqRel);
                self.state.fail("capture writer stopped");
                false
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                unreachable!("record send returned a different message")
            }
        }
    }

    /// Waits for all admitted data and metadata to become durable.
    ///
    /// # Errors
    /// Returns queue, worker, or storage failure.
    pub fn flush(&self) -> Result<(), String> {
        let (sender, receiver) = sync_channel(0);
        if !self.try_send(CaptureWriterMessage::Barrier(CaptureBarrier::Flush, sender)) {
            return Err(self
                .state
                .status()
                .last_error
                .unwrap_or_else(|| "capture writer flush failed".into()));
        }
        receiver
            .recv()
            .map_err(|_| "capture writer stopped before flush".to_string())?
    }

    /// Consumes the active writer and returns evidence of successful durable completion.
    ///
    /// # Errors
    /// Returns queue, worker, or storage failure; no saved artifact is produced.
    pub fn finish(mut self) -> Result<SavedCaptureArtifact, String> {
        let (sender, receiver) = sync_channel(0);
        if !self.try_send(CaptureWriterMessage::Barrier(
            CaptureBarrier::Finish,
            sender,
        )) {
            return Err(self
                .state
                .status()
                .last_error
                .unwrap_or_else(|| "capture writer finish failed".into()));
        }
        let result = receiver
            .recv()
            .map_err(|_| "capture writer stopped before finish".to_string())?;
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| "capture writer thread panicked".to_string())?;
        }
        result?;
        let status = self.state.status();
        if status.failed {
            return Err(status
                .last_error
                .unwrap_or_else(|| "capture writer failed".into()));
        }
        Ok(SavedCaptureArtifact {
            id: self.artifact_id,
            path: self.path,
            status,
        })
    }

    /// Returns a cheap monitor that remains usable after the writer is consumed.
    #[must_use]
    pub fn monitor(&self) -> CaptureWriterMonitor {
        CaptureWriterMonitor(Arc::clone(&self.state))
    }

    /// Admits a metadata update without waiting for disk I/O.
    #[must_use]
    pub fn update_metadata(&self, metadata: CaptureMetadata) -> bool {
        self.try_send(CaptureWriterMessage::Metadata(metadata))
    }

    /// Admits a location observation without waiting for disk I/O.
    #[must_use]
    pub fn record_location(&self, location: PevcapLocationSample) -> bool {
        self.try_send(CaptureWriterMessage::Location(location))
    }

    /// Admits an already privacy-filtered music event without waiting for disk I/O.
    #[must_use]
    pub fn record_music(&self, music: PevcapMusicEvent) -> bool {
        self.try_send(CaptureWriterMessage::Music(music))
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

fn run_capture_writer(
    path: &Path,
    file: File,
    mut header: PevcapHeader,
    receiver: &Receiver<CaptureWriterMessage>,
    records: &CaptureRecordPool,
    state: &CaptureWriterState,
) {
    let result = write_capture_stream(path, file, &mut header, receiver, records, state);
    if let Err(error) = result {
        state.fail(error);
    }
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
    let header_bytes = write_line(
        &mut writer,
        &header.to_jsonl_line().map_err(|error| error.to_string())?,
    )?;
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

    while let Ok(message) = receiver.recv() {
        state.queued_messages.fetch_sub(1, Ordering::AcqRel);
        let line = match message {
            CaptureWriterMessage::Record => Some(
                records
                    .take()
                    .ok_or_else(|| "capture record slot was empty".to_string())?
                    .to_jsonl_line()
                    .map_err(|error| error.to_string())?,
            ),
            CaptureWriterMessage::Location(location) => Some(
                location
                    .to_jsonl_line()
                    .map_err(|error| error.to_string())?,
            ),
            CaptureWriterMessage::Music(music) => {
                Some(music.to_jsonl_line().map_err(|error| error.to_string())?)
            }
            CaptureWriterMessage::Metadata(metadata) => {
                pending_metadata = Some(metadata);
                None
            }
            CaptureWriterMessage::Barrier(kind, reply) => {
                let result = flush_capture_barrier(
                    path,
                    &mut writer,
                    header,
                    &mut pending_metadata,
                    state,
                    &mut flush,
                    kind,
                );
                reply_capture_writer_result(result, &reply)?;
                if kind == CaptureBarrier::Finish {
                    return Ok(());
                }
                None
            }
        };
        if let Some(line) = line {
            write_capture_event_line(&mut writer, &line, state, &mut flush)?;
        }
    }
    flush_capture_barrier(
        path,
        &mut writer,
        header,
        &mut pending_metadata,
        state,
        &mut flush,
        CaptureBarrier::Finish,
    )
}

fn flush_capture_barrier(
    path: &Path,
    writer: &mut BufWriter<File>,
    header: &mut PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
    state: &CaptureWriterState,
    flush: &mut CaptureFlushState,
    kind: CaptureBarrier,
) -> Result<(), String> {
    if kind == CaptureBarrier::Finish {
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
    result: Result<(), String>,
    reply: &SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let failure = result.as_ref().err().cloned();
    let _ = reply.send(result);
    failure.map_or(Ok(()), Err)
}

fn rewrite_pending_capture_metadata(
    path: &Path,
    writer: &mut BufWriter<File>,
    header: &mut PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
    state: &CaptureWriterState,
) -> Result<bool, String> {
    let Some(metadata) = pending_metadata.take() else {
        return Ok(false);
    };
    *header = capture_header(
        header.wall_clock_start_unix_ms,
        header.platform_id.as_str(),
        header.write_limit,
        &metadata,
    )?;
    let bytes = rewrite_capture_header(path, writer, header)?;
    state
        .physical_bytes_written
        .fetch_add(bytes, Ordering::AcqRel);
    Ok(true)
}

/// Finalization owns interval closure, including transport loss and producer drop.
/// A background flush is not an interval boundary. Never claim a saved artifact
/// if its bounded header cannot retain the required closing evidence.
fn close_pending_capture_labels(
    header: &PevcapHeader,
    pending_metadata: &mut Option<CaptureMetadata>,
) -> Result<(), String> {
    let annotations = pending_metadata
        .as_ref()
        .map_or(header.annotations.as_slice(), |metadata| {
            metadata.annotations.as_slice()
        });
    let mut labels = CaptureLabelState::from_annotations(annotations.iter().map(String::as_str));
    if labels.active().is_empty() {
        return Ok(());
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
    Ok(())
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
    let mut reader = BufReader::new(input);
    let mut old_header = Vec::new();
    reader
        .read_until(b'\n', &mut old_header)
        .map_err(|error| error.to_string())?;
    let temp_path = path.with_extension("jsonl.tmp");
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp_path)
        .map_err(|error| error.to_string())?;
    let header_bytes = write_line_to_file(
        &mut output,
        &header.to_jsonl_line().map_err(|error| error.to_string())?,
    )?;
    let copied_bytes =
        std::io::copy(&mut reader, &mut output).map_err(|error| error.to_string())?;
    output.sync_data().map_err(|error| error.to_string())?;
    drop(output);
    sync_parent_directory_after(path, || fs::rename(&temp_path, path))?;
    let file = OpenOptions::new()
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
        let writer = CaptureWriter::start(
            path.clone(),
            WallClockUnixTimestamp::new(0),
            "test",
            None,
            &metadata,
        )
        .unwrap();
        let result = writer.finish();
        assert!(result.unwrap_err().contains("closing label boundaries"));
        assert!(path.exists(), "failed artifact remains recoverable");
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
        assert!(writer.update_metadata(metadata));
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
        let artifact = writer.finish().unwrap();
        let capture = fs::read_to_string(artifact.path()).unwrap();
        assert_eq!(capture.matches("capture_label=ride_stop").count(), 1);
        assert_eq!(capture.matches("capture_label=balancing_stop").count(), 1);
    }
    #[test]
    fn capture_writer_queue_overrun_is_nonblocking_and_instrumented() {
        let (sender, _receiver) = sync_channel(0);
        let state = Arc::new(CaptureWriterState::default());
        let writer = CaptureWriter {
            artifact_id: CaptureArtifactId(Uuid::new_v4()),
            path: PathBuf::new(),
            sender,
            records: Arc::new(CaptureRecordPool::new(1)),
            state: Arc::clone(&state),
            join: None,
        };

        let (reply, _result) = sync_channel(0);
        assert!(!writer.try_send(CaptureWriterMessage::Barrier(CaptureBarrier::Flush, reply)));
        let status = state.status();
        assert_eq!(status.queued_messages, 0);
        assert_eq!(status.peak_queued_messages, 0);
        assert_eq!(status.dropped_messages, 1);
        assert!(status.failed);
        assert_eq!(
            status.last_error.as_deref(),
            Some("capture writer queue is full")
        );
    }

    #[test]
    fn capture_writer_status_retains_peak_accepted_queue_depth() {
        let (sender, _receiver) = sync_channel(1);
        let state = Arc::new(CaptureWriterState::default());
        let writer = CaptureWriter {
            artifact_id: CaptureArtifactId(Uuid::new_v4()),
            path: PathBuf::new(),
            sender,
            records: Arc::new(CaptureRecordPool::new(1)),
            state: Arc::clone(&state),
            join: None,
        };

        let (reply, _result) = sync_channel(0);
        assert!(writer.try_send(CaptureWriterMessage::Barrier(CaptureBarrier::Flush, reply)));
        let status = state.status();
        assert_eq!(status.queued_messages, 1);
        assert_eq!(status.peak_queued_messages, 1);
        assert_eq!(status.dropped_messages, 0);
    }
}
