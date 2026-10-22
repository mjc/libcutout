//! Bounded, restartable consolidation of legacy duplicate semantic snapshots.

use super::{StorageError, live_capture};
use cutout_core::PevcapEncoding;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const SEMANTIC_BATCH_ROWS: usize = 64;
const CAPTURE_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BatchState {
    Pending,
    Deferred,
    Finished,
}

pub(super) struct LegacyArtifactScan {
    digest: String,
    path: PathBuf,
    file: Option<File>,
    hasher: Sha256,
    offset: u64,
    sequence: u64,
    size: u64,
    phase: LegacyArtifactPhase,
    original_file: Option<FileFingerprint>,
}

#[cfg(test)]
struct ArchiveScanPause {
    digest: String,
    entered: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
static ARCHIVE_SCAN_PAUSE: std::sync::Mutex<Option<ArchiveScanPause>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn pause_legacy_archive_for_test(
    digest: String,
    entered: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<()>,
) {
    *ARCHIVE_SCAN_PAUSE.lock().unwrap() = Some(ArchiveScanPause {
        digest,
        entered,
        resume,
    });
}

#[cfg(test)]
fn pause_if_requested(scan: &LegacyArtifactScan) {
    if scan.sequence == 0 {
        return;
    }
    let pause = {
        let mut configured = ARCHIVE_SCAN_PAUSE.lock().unwrap();
        if configured
            .as_ref()
            .is_some_and(|pause| pause.digest == scan.digest)
        {
            configured.take()
        } else {
            None
        }
    };
    if let Some(pause) = pause {
        pause.entered.send(()).unwrap();
        pause
            .resume
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LegacyArtifactPhase {
    VerifyExisting,
    CopyLegacy,
    VerifyCopied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileFingerprint {
    len: u64,
    #[cfg(not(unix))]
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    mtime: i64,
    #[cfg(unix)]
    mtime_nsec: i64,
    #[cfg(unix)]
    ctime: i64,
    #[cfg(unix)]
    ctime_nsec: i64,
}

impl FileFingerprint {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            len: metadata.len(),
            #[cfg(not(unix))]
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            dev: metadata.dev(),
            #[cfg(unix)]
            ino: metadata.ino(),
            #[cfg(unix)]
            mtime: metadata.mtime(),
            #[cfg(unix)]
            mtime_nsec: metadata.mtime_nsec(),
            #[cfg(unix)]
            ctime: metadata.ctime(),
            #[cfg(unix)]
            ctime_nsec: metadata.ctime_nsec(),
        }
    }
}

enum ManagedArtifactSource {
    Present(File, u64, FileFingerprint),
    Missing,
    Unsafe,
}

struct LegacyArtifactCandidate {
    digest: String,
    stored_path: String,
    size: u64,
    encoding: String,
    backfill: bool,
}

impl LegacyArtifactScan {
    pub(super) fn backfill_digest(&self) -> Option<&str> {
        (self.phase == LegacyArtifactPhase::CopyLegacy && self.file.is_some())
            .then_some(self.digest.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ArchiveState {
    Pending,
    Deferred,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CompactionState {
    Complete,
    Deferred,
}

/// Reclaims pages freed by semantic and archive normalization after a quiet period.
/// Failures keep the durable pending flag set for a later retry.
pub(super) fn compact_if_safe(
    connection: &mut Connection,
    database_path: &Path,
    demand: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    observed_demand: u64,
) -> Result<CompactionState, StorageError> {
    let mut busy_timeout = MaintenanceBusyTimeout::enter(connection)?;
    let result = compact_if_safe_nonblocking(
        busy_timeout.connection(),
        database_path,
        demand,
        observed_demand,
    );
    busy_timeout.restore()?;
    result
}

fn compact_if_safe_nonblocking(
    connection: &Connection,
    database_path: &Path,
    demand: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    observed_demand: u64,
) -> Result<CompactionState, StorageError> {
    if demand.load(std::sync::atomic::Ordering::Acquire) != observed_demand {
        return Ok(CompactionState::Deferred);
    }
    let parent = database_path
        .parent()
        .ok_or(StorageError::InvalidDatabaseIdentity)?;
    let filesystem = rustix::fs::statvfs(parent).map_err(std::io::Error::from)?;
    let available = filesystem
        .f_bavail
        .checked_mul(filesystem.f_frsize)
        .ok_or(StorageError::InvalidDatabaseIdentity)?;
    compact_with_interrupt_observer(
        connection,
        database_path,
        demand,
        observed_demand,
        available,
        || {},
        None,
    )
}

#[cfg(test)]
fn compact_with_available_space<F>(
    connection: &mut Connection,
    database_path: &Path,
    demand: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    observed_demand: u64,
    available: u64,
    on_vacuum_start: F,
) -> Result<CompactionState, StorageError>
where
    F: FnOnce(),
{
    compact_with_available_space_and_observer(
        connection,
        database_path,
        demand,
        observed_demand,
        available,
        on_vacuum_start,
        None,
    )
}

#[cfg(test)]
fn compact_with_available_space_and_observer<F>(
    connection: &mut Connection,
    database_path: &Path,
    demand: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    observed_demand: u64,
    available: u64,
    on_vacuum_start: F,
    interrupt_observed: Option<&std::sync::Arc<std::sync::atomic::AtomicU64>>,
) -> Result<CompactionState, StorageError>
where
    F: FnOnce(),
{
    let mut busy_timeout = MaintenanceBusyTimeout::enter(connection)?;
    let result = compact_with_interrupt_observer(
        busy_timeout.connection(),
        database_path,
        demand,
        observed_demand,
        available,
        on_vacuum_start,
        interrupt_observed,
    );
    busy_timeout.restore()?;
    result
}

fn compact_with_interrupt_observer<F>(
    connection: &Connection,
    database_path: &Path,
    demand: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    observed_demand: u64,
    available: u64,
    on_vacuum_start: F,
    interrupt_observed: Option<&std::sync::Arc<std::sync::atomic::AtomicU64>>,
) -> Result<CompactionState, StorageError>
where
    F: FnOnce(),
{
    #[cfg(not(test))]
    let _ = interrupt_observed;
    if demand.load(std::sync::atomic::Ordering::Acquire) != observed_demand {
        return Ok(CompactionState::Deferred);
    }
    let pending: bool = connection.query_row(
        "SELECT compaction_pending FROM storage_normalization_progress WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if !pending || has_active_work(connection, None)? {
        return Ok(CompactionState::Deferred);
    }
    if checkpoint_truncate(connection)?.0 != 0 {
        return Ok(CompactionState::Deferred);
    }
    let database_bytes = fs::metadata(database_path)?.len();
    let required = database_bytes
        .checked_mul(2)
        .and_then(|value| value.checked_add(32 * 1024 * 1024))
        .ok_or(StorageError::InvalidDatabaseIdentity)?;
    if available < required {
        return Ok(CompactionState::Deferred);
    }

    let interrupt = std::sync::Arc::new(connection.get_interrupt_handle());
    if demand.load(std::sync::atomic::Ordering::Acquire) != observed_demand {
        return Ok(CompactionState::Deferred);
    }
    let stop_watcher = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watcher_stop = std::sync::Arc::clone(&stop_watcher);
    let watcher_demand = std::sync::Arc::clone(demand);
    let watcher_interrupt = std::sync::Arc::clone(&interrupt);
    #[cfg(test)]
    let watcher_observer = interrupt_observed.map(std::sync::Arc::clone);
    #[cfg(not(test))]
    let _ = interrupt_observed;
    let Ok(watcher) = std::thread::Builder::new()
        .name("capture-vacuum-interrupt".to_owned())
        .spawn(move || {
            while !watcher_stop.load(std::sync::atomic::Ordering::Acquire) {
                if watcher_demand.load(std::sync::atomic::Ordering::Acquire) != observed_demand {
                    watcher_interrupt.interrupt();
                    #[cfg(test)]
                    if let Some(observed) = &watcher_observer {
                        observed.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        })
    else {
        return Ok(CompactionState::Deferred);
    };
    let mut watcher = InterruptWatcher::new(stop_watcher, watcher);
    on_vacuum_start();
    let vacuum = connection.execute_batch("VACUUM");
    let watcher_stopped = watcher.stop_and_join();
    if !watcher_stopped || vacuum.is_err() || checkpoint_truncate(connection)?.0 != 0 {
        return Ok(CompactionState::Deferred);
    }
    connection.execute(
        "UPDATE storage_normalization_progress
         SET compaction_pending = 0, compaction_complete = 1
         WHERE singleton = 1",
        [],
    )?;
    Ok(CompactionState::Complete)
}

struct InterruptWatcher {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

pub(super) struct MaintenanceBusyTimeout<'connection> {
    connection: &'connection mut Connection,
    original: Option<std::time::Duration>,
}

impl<'connection> MaintenanceBusyTimeout<'connection> {
    pub(super) fn enter(connection: &'connection mut Connection) -> Result<Self, StorageError> {
        let timeout_millis: i64 =
            connection.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
        let timeout_millis =
            u64::try_from(timeout_millis).map_err(|_| StorageError::InvalidDatabaseIdentity)?;
        let original = std::time::Duration::from_millis(timeout_millis);
        connection.busy_timeout(std::time::Duration::ZERO)?;
        Ok(Self {
            connection,
            original: Some(original),
        })
    }

    pub(super) fn restore(&mut self) -> Result<(), StorageError> {
        if let Some(original) = self.original.take() {
            self.connection.busy_timeout(original)?;
        }
        Ok(())
    }

    pub(super) fn connection(&self) -> &Connection {
        self.connection
    }

    pub(super) fn connection_mut(&mut self) -> &mut Connection {
        self.connection
    }
}

impl Drop for MaintenanceBusyTimeout<'_> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

impl InterruptWatcher {
    fn new(
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        join: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            stop,
            join: Some(join),
        }
    }

    fn stop_and_join(&mut self) -> bool {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        self.join.take().is_some_and(|join| join.join().is_ok())
    }
}

impl Drop for InterruptWatcher {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

fn checkpoint_truncate(connection: &Connection) -> Result<(i64, i64, i64), StorageError> {
    Ok(
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?,
    )
}

/// Removes at most one small batch of semantic rows already represented exactly by event bytes.
///
/// Progress and deletion share a transaction. A process stop therefore either retains the
/// complete batch or resumes strictly after the last committed row. Independent, malformed, and
/// oversized semantic rows remain available as explicit overrides.
pub(super) fn remove_redundant_semantic_batch(
    connection: &mut Connection,
    own_backfill: Option<&str>,
) -> Result<BatchState, StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let has_active_work: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE state IN ('active', 'paused'))
             OR EXISTS(SELECT 1 FROM live_capture_sessions WHERE state = 'active')
             OR EXISTS(SELECT 1 FROM pevcap_import_work)
             OR EXISTS(SELECT 1 FROM pevcap_captures
                       WHERE receipt_digest IS NULL AND artifact_digest IS NOT ?1)",
        [own_backfill],
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
             SET semantic_complete = 1, compaction_pending = 0
             WHERE singleton = 1",
            [],
        )?;
        transaction.commit()?;
        return Ok(BatchState::Finished);
    };
    for (capture_id, sequence, snapshot_size, payload, encoding, original_bytes) in &entries {
        let semantic_limit = i64::try_from(live_capture::LIVE_CAPTURE_EVENT_LIMIT_BYTES)
            .map_err(|_| StorageError::InvalidDatabaseIdentity)?;
        if !(1..=semantic_limit).contains(snapshot_size) {
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
             semantic_complete = ?3, compaction_pending = 0
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

/// Verifies and retires one bounded chunk of one old managed artifact per worker idle tick.
/// The active file handle and hash state are process-local; after restart the current artifact is
/// rechecked from byte zero, while the durable cursor advances only after a completed decision.
pub(super) fn normalize_legacy_artifact_batch(
    connection: &mut Connection,
    database_path: &Path,
    scan: &mut Option<LegacyArtifactScan>,
) -> Result<ArchiveState, StorageError> {
    let cursor = match archive_progress(connection, scan.as_ref())? {
        ArchiveProgress::Deferred => return Ok(ArchiveState::Deferred),
        ArchiveProgress::Finished => return Ok(ArchiveState::Finished),
        ArchiveProgress::Ready(cursor) => cursor,
    };
    if scan.is_none() {
        let result =
            start_legacy_artifact_scan(connection, database_path, scan, cursor.as_deref())?;
        if result == ArchiveState::Finished || scan.is_none() {
            return Ok(result);
        }
    }
    process_legacy_artifact_chunk(connection, database_path, scan)
}

enum ArchiveProgress {
    Deferred,
    Finished,
    Ready(Option<String>),
}

fn archive_progress(
    connection: &mut Connection,
    scan: Option<&LegacyArtifactScan>,
) -> Result<ArchiveProgress, StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let semantic_complete: bool = transaction.query_row(
        "SELECT semantic_complete FROM storage_normalization_progress WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if !semantic_complete {
        return Ok(ArchiveProgress::Deferred);
    }
    if has_active_work(
        &transaction,
        scan.and_then(LegacyArtifactScan::backfill_digest),
    )? {
        return Ok(ArchiveProgress::Deferred);
    }
    let (cursor, complete): (Option<String>, bool) = transaction.query_row(
        "SELECT artifact_digest, artifact_complete
         FROM storage_normalization_progress WHERE singleton = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if complete {
        return Ok(ArchiveProgress::Finished);
    }
    drop(transaction);
    Ok(ArchiveProgress::Ready(cursor))
}

fn start_legacy_artifact_scan(
    connection: &mut Connection,
    database_path: &Path,
    scan: &mut Option<LegacyArtifactScan>,
    cursor: Option<&str>,
) -> Result<ArchiveState, StorageError> {
    let Some(candidate) = next_legacy_artifact_candidate(connection, cursor)? else {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE storage_normalization_progress
             SET artifact_complete = 1, compaction_pending = 1
             WHERE singleton = 1",
            [],
        )?;
        transaction.commit()?;
        return Ok(ArchiveState::Finished);
    };
    let Some((path, encoding)) =
        expected_legacy_path(database_path, &candidate.digest, &candidate.stored_path)
    else {
        advance_artifact_cursor(connection, &candidate.digest)?;
        return Ok(ArchiveState::Pending);
    };
    let unknown_size = candidate.size == 0;
    let source = open_verified_managed_file(
        database_path,
        &path,
        (!unknown_size).then_some(candidate.size),
    )?;
    let (file, size, original_file) = match source {
        ManagedArtifactSource::Present(file, actual_size, fingerprint) => {
            if actual_size == 0 || actual_size > 536_870_912 {
                advance_artifact_cursor(connection, &candidate.digest)?;
                return Ok(ArchiveState::Pending);
            }
            (Some(file), actual_size, Some(fingerprint))
        }
        ManagedArtifactSource::Missing if !candidate.backfill => (None, candidate.size, None),
        ManagedArtifactSource::Missing | ManagedArtifactSource::Unsafe => {
            advance_artifact_cursor(connection, &candidate.digest)?;
            return Ok(ArchiveState::Pending);
        }
    };
    if candidate.backfill && file.is_some() {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::capture_data::begin(&transaction, &candidate.digest, encoding, size)?;
        transaction.commit()?;
    } else if !candidate.backfill
        && ((candidate.encoding != "jsonl" && candidate.encoding != "binary")
            || !verify_stored_metadata(connection, &candidate.digest, size, &candidate.encoding)?)
    {
        advance_artifact_cursor(connection, &candidate.digest)?;
        return Ok(ArchiveState::Pending);
    }
    let phase = if candidate.backfill && file.is_some() {
        LegacyArtifactPhase::CopyLegacy
    } else {
        LegacyArtifactPhase::VerifyExisting
    };
    *scan = Some(LegacyArtifactScan {
        digest: candidate.digest,
        path,
        file,
        hasher: Sha256::new(),
        offset: 0,
        sequence: 0,
        size,
        phase,
        original_file,
    });
    Ok(ArchiveState::Pending)
}

type LegacyArtifactCandidateRow = (String, String, i64, Option<String>, bool, Option<i64>);

fn next_legacy_artifact_candidate(
    connection: &mut Connection,
    cursor: Option<&str>,
) -> Result<Option<LegacyArtifactCandidate>, StorageError> {
    let candidate: Option<LegacyArtifactCandidateRow> = connection
        .query_row(
            "SELECT imports.artifact_digest, imports.artifact_path,
                    imports.artifact_size, captures.encoding,
                    captures.artifact_digest IS NULL, captures.artifact_size
             FROM pevcap_imports AS imports
             LEFT JOIN pevcap_captures AS captures
               ON captures.artifact_digest = imports.artifact_digest
             WHERE imports.artifact_path IS NOT NULL
               AND (?1 IS NULL OR imports.artifact_digest > ?1)
             ORDER BY imports.artifact_digest LIMIT 1",
            [cursor],
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
        .optional()?;
    let Some((digest, stored_path, imported_size, encoding, backfill, canonical_size)) = candidate
    else {
        return Ok(None);
    };
    let stored_size = if imported_size == 0 {
        canonical_size.unwrap_or(0)
    } else {
        imported_size
    };
    Ok(Some(LegacyArtifactCandidate {
        digest,
        stored_path,
        size: u64::try_from(stored_size).map_err(|_| StorageError::InvalidDatabaseIdentity)?,
        encoding: encoding.unwrap_or_default(),
        backfill,
    }))
}

fn process_legacy_artifact_chunk(
    connection: &mut Connection,
    database_path: &Path,
    scan: &mut Option<LegacyArtifactScan>,
) -> Result<ArchiveState, StorageError> {
    let Some(active) = scan.as_mut() else {
        return Ok(ArchiveState::Pending);
    };
    let expected_chunk = super::capture_data::chunk(connection, &active.digest, active.sequence)?;
    let chunk_limit = if let Some(expected_chunk) = &expected_chunk {
        expected_chunk.len()
    } else if active.phase == LegacyArtifactPhase::CopyLegacy && active.file.is_some() {
        usize::try_from(active.size.saturating_sub(active.offset))
            .unwrap_or(CAPTURE_CHUNK_BYTES)
            .min(CAPTURE_CHUNK_BYTES)
    } else {
        0
    };
    if chunk_limit == 0 {
        return complete_legacy_artifact_scan(connection, database_path, scan);
    }
    let mut file_chunk = vec![0; chunk_limit];
    if let Some(file) = &mut active.file {
        file.seek(SeekFrom::Start(active.offset))?;
        match file.read_exact(&mut file_chunk) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                reject_artifact(connection, scan)?;
                return Ok(ArchiveState::Pending);
            }
            Err(error) => return Err(StorageError::Io(error)),
        }
    }
    if let Some(expected_chunk) = expected_chunk {
        if expected_chunk != file_chunk && active.file.is_some() {
            reject_artifact(connection, scan)?;
            return Ok(ArchiveState::Pending);
        }
        if active.file.is_none() {
            file_chunk = expected_chunk;
        }
    } else if active.phase == LegacyArtifactPhase::CopyLegacy && active.file.is_some() {
        super::capture_data::append_legacy_backfill_chunk(
            connection,
            &active.digest,
            active.sequence,
            &file_chunk,
        )?;
    } else {
        reject_artifact(connection, scan)?;
        return Ok(ArchiveState::Pending);
    }
    active.hasher.update(&file_chunk);
    active.offset = active
        .offset
        .saturating_add(u64::try_from(file_chunk.len()).unwrap_or(u64::MAX));
    active.sequence = active.sequence.saturating_add(1);
    if active.offset > active.size {
        reject_artifact(connection, scan)?;
        return Ok(ArchiveState::Pending);
    }
    #[cfg(test)]
    if let Some(active) = scan.as_ref() {
        pause_if_requested(active);
    }
    Ok(ArchiveState::Pending)
}

fn complete_legacy_artifact_scan(
    connection: &mut Connection,
    database_path: &Path,
    scan: &mut Option<LegacyArtifactScan>,
) -> Result<ArchiveState, StorageError> {
    if scan.is_none() {
        return Ok(ArchiveState::Pending);
    }
    verify_scan_end(connection, scan)?;
    let Some(active) = scan.as_mut() else {
        return Ok(ArchiveState::Pending);
    };
    if active.phase == LegacyArtifactPhase::CopyLegacy && active.file.is_some() {
        publish_legacy_backfill(connection, active)?;
        return Ok(ArchiveState::Pending);
    }
    retire_verified_legacy_artifact(connection, database_path, scan)
}

fn verify_scan_end(
    connection: &mut Connection,
    scan: &mut Option<LegacyArtifactScan>,
) -> Result<(), StorageError> {
    let Some(active) = scan.as_mut() else {
        return Ok(());
    };
    let mut extra = [0_u8; 1];
    if let Some(file) = &mut active.file {
        file.seek(SeekFrom::Start(active.offset))?;
        match file.read(&mut extra) {
            Ok(0) => {}
            Ok(_) => {
                reject_artifact(connection, scan)?;
                return Ok(());
            }
            Err(error) => return Err(StorageError::Io(error)),
        }
    }
    if active.offset != active.size || !digest_matches(&active.hasher, &active.digest) {
        reject_artifact(connection, scan)?;
    }
    Ok(())
}

fn publish_legacy_backfill(
    connection: &mut Connection,
    active: &mut LegacyArtifactScan,
) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    super::capture_data::publish(&transaction, &active.digest)?;
    transaction.commit()?;
    active.phase = LegacyArtifactPhase::VerifyCopied;
    active.hasher = Sha256::new();
    active.offset = 0;
    active.sequence = 0;
    if let Some(file) = &mut active.file {
        file.seek(SeekFrom::Start(0))?;
    }
    Ok(())
}

fn retire_verified_legacy_artifact(
    connection: &mut Connection,
    database_path: &Path,
    scan: &mut Option<LegacyArtifactScan>,
) -> Result<ArchiveState, StorageError> {
    let Some(active) = scan.as_ref() else {
        return Ok(ArchiveState::Pending);
    };
    let already_unlinked =
        if active.file.is_some() && !managed_file_still_matches(database_path, active)? {
            let missing_after_verified_unlink = active.phase != LegacyArtifactPhase::CopyLegacy
                && match fs::symlink_metadata(&active.path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                    _ => false,
                };
            if !missing_after_verified_unlink {
                reject_artifact(connection, scan)?;
                return Ok(ArchiveState::Pending);
            }
            true
        } else {
            false
        };
    let digest = active.digest.clone();
    let path = active.path.clone();
    let size = active.size;
    if active.file.is_some() && !already_unlinked {
        fs::remove_file(&path)?;
    }
    if let Some(parent) = path.parent() {
        if !managed_directory(database_path, parent)? {
            return Ok(ArchiveState::Deferred);
        }
        File::open(parent)?.sync_all()?;
    }
    *scan = None;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "UPDATE pevcap_imports SET artifact_path = NULL, artifact_size = ?2
         WHERE artifact_digest = ?1",
        rusqlite::params![&digest, size],
    )?;
    transaction.execute(
        "UPDATE storage_normalization_progress SET artifact_digest = ?1
         WHERE singleton = 1",
        [&digest],
    )?;
    transaction.commit()?;
    Ok(ArchiveState::Pending)
}

fn has_active_work(
    connection: &Connection,
    own_backfill: Option<&str>,
) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE state IN ('active', 'paused'))
             OR EXISTS(SELECT 1 FROM live_capture_sessions WHERE state = 'active')
             OR EXISTS(SELECT 1 FROM pevcap_import_work)
             OR EXISTS(SELECT 1 FROM pevcap_captures
                       WHERE receipt_digest IS NULL AND artifact_digest IS NOT ?1)",
        [own_backfill],
        |row| row.get(0),
    )?)
}

fn expected_legacy_path(
    database_path: &Path,
    digest: &str,
    stored_path: &str,
) -> Option<(PathBuf, PevcapEncoding)> {
    let directory = legacy_import_directory(database_path);
    for (extension, encoding) in [
        ("jsonl", PevcapEncoding::Jsonl),
        ("pevcap", PevcapEncoding::Binary),
    ] {
        let expected = directory.join(format!("{digest}.{extension}"));
        if Path::new(stored_path) == expected {
            return Some((expected, encoding));
        }
    }
    None
}

fn legacy_import_directory(database_path: &Path) -> PathBuf {
    let mut name = database_path.as_os_str().to_owned();
    name.push(".pevcap-imports");
    PathBuf::from(name)
}

fn managed_directory(database_path: &Path, directory: &Path) -> Result<bool, StorageError> {
    if directory != legacy_import_directory(database_path) {
        return Ok(false);
    }
    match fs::symlink_metadata(directory) {
        Ok(metadata) => Ok(metadata.file_type().is_dir() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(StorageError::Io(error)),
    }
}

fn open_verified_managed_file(
    database_path: &Path,
    path: &Path,
    expected_size: Option<u64>,
) -> Result<ManagedArtifactSource, StorageError> {
    let directory = legacy_import_directory(database_path);
    if !managed_directory(database_path, &directory)? || path.parent() != Some(directory.as_path())
    {
        return Ok(ManagedArtifactSource::Unsafe);
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ManagedArtifactSource::Missing);
        }
        Err(error) => return Err(StorageError::Io(error)),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Ok(ManagedArtifactSource::Unsafe);
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ManagedArtifactSource::Missing);
        }
        Err(error) => return Err(StorageError::Io(error)),
    };
    let opened_metadata = file.metadata()?;
    let opened_fingerprint = FileFingerprint::from_metadata(&opened_metadata);
    let path_fingerprint = FileFingerprint::from_metadata(&metadata);
    if (expected_size.is_some_and(|size| size != opened_metadata.len()))
        || opened_metadata.len() != path_fingerprint.len
        || !same_file_identity(path_fingerprint, opened_fingerprint)
    {
        return Ok(ManagedArtifactSource::Unsafe);
    }
    Ok(ManagedArtifactSource::Present(
        file,
        opened_metadata.len(),
        opened_fingerprint,
    ))
}

fn verify_stored_metadata(
    connection: &Connection,
    digest: &str,
    size: u64,
    encoding: &str,
) -> Result<bool, StorageError> {
    let metadata: Option<(i64, String)> = connection
        .query_row(
            "SELECT artifact_size, encoding FROM pevcap_captures
             WHERE artifact_digest = ?1 AND receipt_digest = ?1",
            [digest],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(metadata.is_some_and(|(stored_size, stored_encoding)| {
        u64::try_from(stored_size).ok() == Some(size) && stored_encoding == encoding
    }))
}

fn managed_file_still_matches(
    database_path: &Path,
    scan: &LegacyArtifactScan,
) -> Result<bool, StorageError> {
    let directory = legacy_import_directory(database_path);
    if !managed_directory(database_path, &directory)?
        || scan.path.parent() != Some(directory.as_path())
    {
        return Ok(false);
    }
    let path_metadata = match fs::symlink_metadata(&scan.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(StorageError::Io(error)),
    };
    let Some(file) = &scan.file else {
        return Ok(false);
    };
    let opened_metadata = file.metadata()?;
    if !path_metadata.file_type().is_file()
        || path_metadata.file_type().is_symlink()
        || path_metadata.len() != scan.size
        || opened_metadata.len() != scan.size
    {
        return Ok(false);
    }
    let Some(original) = scan.original_file else {
        return Ok(false);
    };
    Ok(
        same_file_identity(original, FileFingerprint::from_metadata(&path_metadata))
            && same_file_identity(original, FileFingerprint::from_metadata(&opened_metadata)),
    )
}

fn same_file_identity(expected: FileFingerprint, actual: FileFingerprint) -> bool {
    #[cfg(unix)]
    {
        expected.dev == actual.dev
            && expected.ino == actual.ino
            && expected.len == actual.len
            && expected.mtime == actual.mtime
            && expected.mtime_nsec == actual.mtime_nsec
            && expected.ctime == actual.ctime
            && expected.ctime_nsec == actual.ctime_nsec
    }
    #[cfg(not(unix))]
    {
        expected.len == actual.len && expected.modified == actual.modified
    }
}

fn digest_matches(hasher: &Sha256, digest: &str) -> bool {
    hex::encode(hasher.clone().finalize()) == digest
}

fn reject_artifact(
    connection: &mut Connection,
    scan: &mut Option<LegacyArtifactScan>,
) -> Result<(), StorageError> {
    if let Some(active) = scan.as_ref() {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if active.phase == LegacyArtifactPhase::CopyLegacy
            && active.file.is_some()
            && !super::capture_data::is_stored(&transaction, &active.digest)?
        {
            super::capture_data::abort(&transaction, &active.digest)?;
        }
        advance_artifact_cursor(&transaction, &active.digest)?;
        transaction.commit()?;
        scan.take();
    }
    Ok(())
}

fn advance_artifact_cursor(connection: &Connection, digest: &str) -> Result<(), StorageError> {
    connection.execute(
        "UPDATE storage_normalization_progress SET artifact_digest = ?1
         WHERE singleton = 1",
        [digest],
    )?;
    Ok(())
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
#[cfg(test)]
mod tests {
    use super::{
        CompactionState, LegacyArtifactPhase, LegacyArtifactScan, MaintenanceBusyTimeout,
        compact_with_available_space, compact_with_available_space_and_observer, reject_artifact,
    };
    use rusqlite::{
        Connection,
        hooks::{AuthAction, Authorization},
    };
    use sha2::{Digest, Sha256};
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        time::Duration,
    };

    fn pending_database(path: &std::path::Path) -> Connection {
        let mut connection = Connection::open(path).unwrap();
        super::super::configure_connection(&mut connection, path).unwrap();
        connection
            .execute_batch(
                "UPDATE storage_normalization_progress
                 SET semantic_complete = 1, artifact_complete = 1,
                     compaction_pending = 1, compaction_complete = 0
                 WHERE singleton = 1;
                 CREATE TABLE filler (payload BLOB NOT NULL);
                 INSERT INTO filler VALUES (zeroblob(16777216));",
            )
            .unwrap();
        connection
    }

    fn compaction_is_pending(connection: &Connection) -> bool {
        connection
            .query_row(
                "SELECT compaction_pending FROM storage_normalization_progress WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn failed_backfill_abort_retains_scan_until_retry_commits() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("backfill-abort.sqlite");
        let mut connection = Connection::open(&database_path).unwrap();
        connection.busy_timeout(Duration::ZERO).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE pevcap_captures (
                    artifact_digest TEXT PRIMARY KEY,
                    receipt_digest TEXT
                 );
                 CREATE TABLE storage_normalization_progress (
                    singleton INTEGER PRIMARY KEY,
                    artifact_digest TEXT
                 );
                 INSERT INTO storage_normalization_progress VALUES (1, 'previous');
                 INSERT INTO pevcap_captures VALUES (lower(hex(zeroblob(32))), NULL);",
            )
            .unwrap();
        let digest = "0".repeat(64);
        let mut scan = Some(LegacyArtifactScan {
            digest: digest.clone(),
            path: PathBuf::from("managed-sidecar.jsonl"),
            file: Some(tempfile::tempfile().unwrap()),
            hasher: Sha256::new(),
            offset: 0,
            sequence: 0,
            size: 1,
            phase: LegacyArtifactPhase::CopyLegacy,
            original_file: None,
        });
        let lock = Connection::open(&database_path).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();

        assert!(reject_artifact(&mut connection, &mut scan).is_err());
        assert!(scan.is_some(), "worker retains its source and digest owner");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM pevcap_captures WHERE artifact_digest = ?1",
                    [&digest],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            1,
            "the incomplete manifest remains until abort succeeds"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT artifact_digest FROM storage_normalization_progress WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "previous",
            "cursor is not advanced before cleanup commits"
        );

        lock.execute_batch("ROLLBACK").unwrap();
        reject_artifact(&mut connection, &mut scan).unwrap();
        assert!(scan.is_none());
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM pevcap_captures WHERE artifact_digest = ?1",
                    [&digest],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT artifact_digest FROM storage_normalization_progress WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            digest
        );
    }

    #[test]
    fn low_space_defers_compaction_and_keeps_database_usable() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("low-space.sqlite");
        let mut connection = pending_database(&database_path);
        let result = compact_with_available_space(
            &mut connection,
            &database_path,
            &Arc::new(AtomicU64::new(0)),
            0,
            0,
            || panic!("VACUUM must not start without required free space"),
        )
        .unwrap();
        assert_eq!(result, CompactionState::Deferred);
        assert!(compaction_is_pending(&connection));
        assert_eq!(
            connection
                .query_row("SELECT length(payload) FROM filler", [], |row| row
                    .get::<_, u64>(0))
                .unwrap(),
            16 * 1024 * 1024
        );
    }

    #[test]
    fn command_demand_interrupts_vacuum_and_later_retry_completes() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("interrupted-vacuum.sqlite");
        let mut connection = pending_database(&database_path);
        let demand = Arc::new(AtomicU64::new(0));
        let cancel = Arc::clone(&demand);
        let interruptions = Arc::new(AtomicU64::new(0));
        let observed_interruptions = Arc::clone(&interruptions);
        let gated = Arc::new(AtomicBool::new(false));
        let gate_once = Arc::clone(&gated);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        connection
            .authorizer(Some(move |context: rusqlite::hooks::AuthContext<'_>| {
                if let AuthAction::Attach { .. } = context.action
                    && !gate_once.swap(true, Ordering::AcqRel)
                {
                    entered_tx
                        .send(observed_interruptions.load(Ordering::Acquire))
                        .unwrap();
                    release_rx.recv().unwrap();
                }
                Authorization::Allow
            }))
            .unwrap();
        // Model a producer which has published intent and is paused just before enqueue.
        // The worker may take this value as its baseline, so a successful enqueue must publish
        // demand again after sending.
        demand.fetch_add(1, Ordering::AcqRel);
        let observed_demand = demand.load(Ordering::Acquire);
        let worker_demand = Arc::clone(&demand);
        let worker_interruptions = Arc::clone(&interruptions);
        let worker_database_path = database_path.clone();
        let worker = std::thread::spawn(move || {
            let result = compact_with_available_space_and_observer(
                &mut connection,
                &worker_database_path,
                &worker_demand,
                observed_demand,
                u64::MAX,
                move || {
                    // This is the post-enqueue publication, after the worker captured the
                    // pre-enqueue baseline but before VACUUM's VM begins.
                    cancel.fetch_add(1, Ordering::AcqRel);
                },
                Some(&worker_interruptions),
            )
            .unwrap();
            // The watcher is joined before regular database work resumes.
            assert_eq!(
                connection
                    .query_row("SELECT length(payload) FROM filler", [], |row| row
                        .get::<_, u64>(0))
                    .unwrap(),
                16 * 1024 * 1024
            );
            connection
                .execute("INSERT INTO filler VALUES (zeroblob(1))", [])
                .unwrap();
            (connection, result)
        });
        let pre_gate_interrupts = entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while interruptions.load(Ordering::Acquire) <= pre_gate_interrupts {
            assert!(
                std::time::Instant::now() < deadline,
                "watcher did not interrupt inside VACUUM's ATTACH gate"
            );
            std::thread::yield_now();
        }
        release_tx.send(()).unwrap();
        let (mut connection, result) = worker.join().unwrap();
        assert_eq!(result, CompactionState::Deferred);
        assert!(compaction_is_pending(&connection));
        assert_eq!(
            compact_with_available_space(
                &mut connection,
                &database_path,
                &demand,
                demand.load(Ordering::Acquire),
                u64::MAX,
                || {},
            )
            .unwrap(),
            CompactionState::Complete
        );
        assert!(!compaction_is_pending(&connection));
    }

    #[test]
    fn busy_checkpoint_defers_compaction_without_losing_pending_work() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("busy-checkpoint.sqlite");
        let mut connection = pending_database(&database_path);
        let reader = Connection::open(&database_path).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        reader
            .query_row("SELECT count(*) FROM filler", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap();
        connection
            .execute("INSERT INTO filler VALUES (zeroblob(1))", [])
            .unwrap();

        let original_busy_timeout: i64 = connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        let gated = Arc::new(AtomicBool::new(false));
        let gate_once = Arc::clone(&gated);
        let (checkpoint_entered_tx, checkpoint_entered_rx) = std::sync::mpsc::channel();
        let (checkpoint_release_tx, checkpoint_release_rx) = std::sync::mpsc::channel();
        connection
            .authorizer(Some(move |context: rusqlite::hooks::AuthContext<'_>| {
                if let AuthAction::Pragma { pragma_name, .. } = context.action
                    && pragma_name.eq_ignore_ascii_case("wal_checkpoint")
                    && !gate_once.swap(true, Ordering::AcqRel)
                {
                    checkpoint_entered_tx.send(()).unwrap();
                    checkpoint_release_rx.recv().unwrap();
                }
                Authorization::Allow
            }))
            .unwrap();
        let demand = Arc::new(AtomicU64::new(0));
        let worker_demand = Arc::clone(&demand);
        let worker_database_path = database_path.clone();
        let checkpoint_started = std::time::Instant::now();
        let worker = std::thread::spawn(move || {
            let result = compact_with_available_space(
                &mut connection,
                &worker_database_path,
                &worker_demand,
                0,
                u64::MAX,
                || panic!("VACUUM must wait for a successful WAL checkpoint"),
            )
            .unwrap();
            let timeout_after: i64 = connection
                .pragma_query_value(None, "busy_timeout", |row| row.get(0))
                .unwrap();
            let readable_rows = connection
                .query_row("SELECT count(*) FROM filler", [], |row| {
                    row.get::<_, u64>(0)
                })
                .unwrap();
            (connection, result, timeout_after, readable_rows)
        });
        checkpoint_entered_rx.recv().unwrap();
        demand.fetch_add(1, Ordering::AcqRel);
        checkpoint_release_tx.send(()).unwrap();
        let (mut connection, compacted, restored_busy_timeout, readable_rows) =
            worker.join().unwrap();
        assert_eq!(compacted, CompactionState::Deferred);
        assert!(
            checkpoint_started.elapsed() < Duration::from_millis(500),
            "a queued command must not wait through the connection's normal busy timeout"
        );
        assert_eq!(restored_busy_timeout, original_busy_timeout);
        assert!(compaction_is_pending(&connection));
        assert_eq!(readable_rows, 2);
        reader.execute_batch("ROLLBACK").unwrap();
        assert_eq!(
            compact_with_available_space(
                &mut connection,
                &database_path,
                &Arc::new(AtomicU64::new(0)),
                0,
                u64::MAX,
                || {},
            )
            .unwrap(),
            CompactionState::Complete
        );
        assert!(!compaction_is_pending(&connection));
    }

    #[test]
    fn idle_normalization_uses_nonblocking_busy_timeout_and_restores_connection_setting() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("busy-normalization.sqlite");
        let mut connection = Connection::open(&database_path).unwrap();
        super::super::configure_connection(&mut connection, &database_path).unwrap();
        let original_busy_timeout: i64 = connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(original_busy_timeout, 5_000);

        let lock = Connection::open(&database_path).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let maintenance_started = std::time::Instant::now();
        let mut busy_timeout = MaintenanceBusyTimeout::enter(&mut connection).unwrap();
        let result = super::remove_redundant_semantic_batch(busy_timeout.connection_mut(), None);
        busy_timeout.restore().unwrap();
        drop(busy_timeout);
        assert!(result.is_err(), "the competing writer keeps its lock");
        assert!(
            maintenance_started.elapsed() < Duration::from_millis(500),
            "idle normalization yields to a queued command instead of waiting five seconds"
        );
        let restored_busy_timeout: i64 = connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(restored_busy_timeout, original_busy_timeout);
        lock.execute_batch("ROLLBACK").unwrap();
    }
}
