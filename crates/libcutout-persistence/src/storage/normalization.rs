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
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let semantic_complete: bool = transaction.query_row(
        "SELECT semantic_complete FROM storage_normalization_progress WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if !semantic_complete {
        return Ok(ArchiveState::Deferred);
    }
    if has_active_work(
        &transaction,
        scan.as_ref().and_then(LegacyArtifactScan::backfill_digest),
    )? {
        return Ok(ArchiveState::Deferred);
    }
    let (cursor, complete): (Option<String>, bool) = transaction.query_row(
        "SELECT artifact_digest, artifact_complete
         FROM storage_normalization_progress WHERE singleton = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if complete {
        return Ok(ArchiveState::Finished);
    }
    drop(transaction);

    if scan.is_none() {
        let candidate: Option<(String, String, u64, String, bool, Option<u64>)> = connection
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
                [cursor.as_deref()],
                |row| {
                    let size: i64 = row.get(2)?;
                    let encoding: Option<String> = row.get(3)?;
                    let canonical_size: Option<i64> = row.get(5)?;
                    let size = if size == 0 {
                        canonical_size.unwrap_or(0)
                    } else {
                        size
                    };
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        u64::try_from(size).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Integer,
                                Box::new(error),
                            )
                        })?,
                        encoding.unwrap_or_default(),
                        row.get(4)?,
                        canonical_size
                            .map(u64::try_from)
                            .transpose()
                            .map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    5,
                                    rusqlite::types::Type::Integer,
                                    Box::new(error),
                                )
                            })?,
                    ))
                },
            )
            .optional()?;
        let Some((digest, stored_path, size, stored_encoding, backfill, _canonical_size)) =
            candidate
        else {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "UPDATE storage_normalization_progress
                 SET artifact_complete = 1, compaction_pending = 1
                 WHERE singleton = 1",
                [],
            )?;
            transaction.commit()?;
            return Ok(ArchiveState::Finished);
        };
        let Some((path, encoding)) = expected_legacy_path(database_path, &digest, &stored_path)
        else {
            advance_artifact_cursor(connection, &digest)?;
            return Ok(ArchiveState::Pending);
        };
        let unknown_size = size == 0;
        let source =
            open_verified_managed_file(database_path, &path, (!unknown_size).then_some(size))?;
        let (file, size, original_file) = match source {
            ManagedArtifactSource::Present(file, actual_size, fingerprint) => {
                if actual_size == 0 || actual_size > 536_870_912 {
                    advance_artifact_cursor(connection, &digest)?;
                    return Ok(ArchiveState::Pending);
                }
                (Some(file), actual_size, Some(fingerprint))
            }
            ManagedArtifactSource::Missing if !backfill => (None, size, None),
            ManagedArtifactSource::Missing | ManagedArtifactSource::Unsafe => {
                advance_artifact_cursor(connection, &digest)?;
                return Ok(ArchiveState::Pending);
            }
        };
        if backfill && file.is_some() {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            super::capture_data::begin(&transaction, &digest, encoding, size)?;
            transaction.commit()?;
        } else if !backfill && !matches!(stored_encoding.as_str(), "jsonl" | "binary") {
            advance_artifact_cursor(connection, &digest)?;
            return Ok(ArchiveState::Pending);
        } else if !backfill && !verify_stored_metadata(connection, &digest, size, &stored_encoding)?
        {
            advance_artifact_cursor(connection, &digest)?;
            return Ok(ArchiveState::Pending);
        }
        let phase = if backfill && file.is_some() {
            LegacyArtifactPhase::CopyLegacy
        } else {
            LegacyArtifactPhase::VerifyExisting
        };
        *scan = Some(LegacyArtifactScan {
            digest,
            path,
            file,
            hasher: Sha256::new(),
            offset: 0,
            sequence: 0,
            size,
            phase,
            original_file,
        });
    }

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
        let mut extra = [0_u8; 1];
        if let Some(file) = &mut active.file {
            file.seek(SeekFrom::Start(active.offset))?;
            match file.read(&mut extra) {
                Ok(0) => {}
                Ok(_) => {
                    reject_artifact(connection, scan)?;
                    return Ok(ArchiveState::Pending);
                }
                Err(error) => return Err(StorageError::Io(error)),
            }
        }
        if active.offset != active.size || !digest_matches(&active.hasher, &active.digest) {
            reject_artifact(connection, scan)?;
            return Ok(ArchiveState::Pending);
        }
        let already_unlinked = if active.file.is_some()
            && !managed_file_still_matches(database_path, active)?
        {
            let missing_after_verified_unlink = active.phase != LegacyArtifactPhase::CopyLegacy
                && matches!(fs::symlink_metadata(&active.path), Err(error) if error.kind() == std::io::ErrorKind::NotFound);
            if !missing_after_verified_unlink {
                reject_artifact(connection, scan)?;
                return Ok(ArchiveState::Pending);
            }
            true
        } else {
            false
        };
        if active.phase == LegacyArtifactPhase::CopyLegacy && active.file.is_some() {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            super::capture_data::publish(&transaction, &active.digest)?;
            transaction.commit()?;
            active.phase = LegacyArtifactPhase::VerifyCopied;
            active.hasher = Sha256::new();
            active.offset = 0;
            active.sequence = 0;
            if let Some(file) = &mut active.file {
                file.seek(SeekFrom::Start(0))?;
            }
            return Ok(ArchiveState::Pending);
        }
        let digest = active.digest.clone();
        let path = active.path.clone();
        let size = active.size;
        if active.file.is_some() && !already_unlinked {
            fs::remove_file(&path)?;
        }
        if active.file.is_some()
            && let Some(parent) = path.parent()
        {
            if !managed_directory(database_path, parent)? {
                return Ok(ArchiveState::Deferred);
            }
            File::open(parent)?.sync_all()?;
        } else if let Some(parent) = path.parent() {
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
        return Ok(ArchiveState::Pending);
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
        if expected_chunk != file_chunk {
            if active.file.is_some() {
                reject_artifact(connection, scan)?;
                return Ok(ArchiveState::Pending);
            }
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
