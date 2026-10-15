//! Explicit, serialized in-place SQLite storage maintenance.

use super::{DATABASE_PAGE_SIZE_BYTES, DatabaseMaintenanceReport, StorageError};
use rusqlite::Connection;
use std::{fs, path::Path};

const VACUUM_HEADROOM_BYTES: u64 = 64 * 1024 * 1024;

pub(super) fn run(
    connection: &mut Connection,
    database_path: &Path,
) -> Result<DatabaseMaintenanceReport, StorageError> {
    reject_active_work(connection)?;

    let database_bytes = file_size_if_present(database_path)?;
    let bytes_before = database_files_size(database_path)?;
    let wal_path = sidecar_path(database_path, "-wal");
    let shm_path = sidecar_path(database_path, "-shm");
    let wal_and_shm_bytes = file_size_if_present(&wal_path)?
        .checked_add(file_size_if_present(&shm_path)?)
        .ok_or(StorageError::MaintenanceSpaceCalculationOverflow)?;
    let required_bytes = required_temporary_space(database_bytes, wal_and_shm_bytes)?;
    let available_bytes =
        fs2::available_space(database_path.parent().ok_or(StorageError::InvalidPath)?)?;
    ensure_temporary_space(required_bytes, available_bytes)?;

    let compressed_event_count = super::live_capture::compress_legacy_payloads(connection)?;
    checkpoint_wal(connection)?;

    compact_and_restore(connection, compact)?;

    Ok(DatabaseMaintenanceReport {
        bytes_before,
        bytes_after: database_files_size(database_path)?,
        compressed_event_count,
    })
}

fn compact_and_restore(
    connection: &Connection,
    operation: impl FnOnce(&Connection) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let original_journal: String =
        connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let original_synchronous: u32 =
        connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    let operation_result = operation(connection);
    let restore_result =
        restore_operational_mode(connection, &original_journal, original_synchronous);
    match (operation_result, restore_result) {
        (_, Err(error)) | (Err(error), Ok(())) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn reject_active_work(connection: &Connection) -> Result<(), StorageError> {
    let active_ride: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM rides WHERE state IN ('active', 'paused'))",
        [],
        |row| row.get(0),
    )?;
    if active_ride {
        return Err(StorageError::MaintenanceActiveRide);
    }

    let active_capture: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM live_capture_sessions WHERE state = 'active')",
        [],
        |row| row.get(0),
    )?;
    if active_capture {
        return Err(StorageError::MaintenanceActiveCapture);
    }

    let pevcap_import_in_progress: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pevcap_import_work)
             OR EXISTS(SELECT 1 FROM pevcap_captures WHERE receipt_digest IS NULL)",
        [],
        |row| row.get(0),
    )?;
    if pevcap_import_in_progress {
        return Err(StorageError::MaintenancePevcapImportInProgress);
    }
    Ok(())
}

fn required_temporary_space(database_bytes: u64, wal_bytes: u64) -> Result<u64, StorageError> {
    database_bytes
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(wal_bytes))
        .and_then(|bytes| bytes.checked_add(VACUUM_HEADROOM_BYTES))
        .ok_or(StorageError::MaintenanceSpaceCalculationOverflow)
}

fn ensure_temporary_space(required_bytes: u64, available_bytes: u64) -> Result<(), StorageError> {
    if available_bytes < required_bytes {
        return Err(StorageError::MaintenanceInsufficientSpace {
            required_bytes,
            available_bytes,
        });
    }
    Ok(())
}

fn checkpoint_wal(connection: &Connection) -> Result<(), StorageError> {
    let (busy, _log_frames, _checkpointed_frames): (i64, i64, i64) =
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 {
        return Err(StorageError::MaintenanceCheckpointBusy);
    }
    Ok(())
}

fn compact(connection: &Connection) -> Result<(), StorageError> {
    let journal: String =
        connection.pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))?;
    if journal != "delete" {
        return Err(StorageError::MaintenanceJournalModeUnavailable);
    }
    connection.pragma_update(None, "page_size", DATABASE_PAGE_SIZE_BYTES)?;
    connection.execute_batch("VACUUM")?;
    let actual_page_size: u32 =
        connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
    if actual_page_size != DATABASE_PAGE_SIZE_BYTES {
        return Err(StorageError::MaintenancePageSizeMismatch {
            expected_bytes: DATABASE_PAGE_SIZE_BYTES,
            actual_bytes: actual_page_size,
        });
    }
    super::integrity_check(connection)?;
    Ok(())
}

fn restore_operational_mode(
    connection: &Connection,
    journal: &str,
    synchronous: u32,
) -> Result<(), StorageError> {
    let restored: String = connection
        .pragma_update_and_check(None, "journal_mode", journal, |row| row.get(0))
        .map_err(|source| StorageError::MaintenanceRestoreFailed { source })?;
    if restored != journal {
        return Err(StorageError::MaintenanceRestoreFailed {
            source: rusqlite::Error::InvalidQuery,
        });
    }
    connection
        .pragma_update(None, "synchronous", synchronous)
        .map_err(|source| StorageError::MaintenanceRestoreFailed { source })?;
    Ok(())
}

fn database_files_size(database_path: &Path) -> Result<u64, StorageError> {
    let wal_path = sidecar_path(database_path, "-wal");
    let shm_path = sidecar_path(database_path, "-shm");
    [database_path, wal_path.as_path(), shm_path.as_path()]
        .into_iter()
        .try_fold(0_u64, |total, path| {
            total
                .checked_add(file_size_if_present(path)?)
                .ok_or(StorageError::MaintenanceSpaceCalculationOverflow)
        })
}

fn file_size_if_present(path: &Path) -> Result<u64, StorageError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(StorageError::Io(error)),
    }
}

fn sidecar_path(database_path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut path = database_path.as_os_str().to_owned();
    path.push(suffix);
    path.into()
}

#[cfg(test)]
mod tests;
