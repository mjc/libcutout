use super::{
    VACUUM_HEADROOM_BYTES, compact_and_restore, ensure_temporary_space, reject_active_work,
    required_temporary_space, run,
};
use crate::storage::StorageError;
use crate::storage::{DATABASE_PAGE_SIZE_BYTES, configure_connection};
use rusqlite::Connection;

#[test]
fn vacuum_space_includes_database_copy_wal_and_headroom() {
    assert_eq!(
        required_temporary_space(1_024, 300).unwrap(),
        2 * 1_024 + 300 + VACUUM_HEADROOM_BYTES
    );
}

#[test]
fn vacuum_space_rejects_a_shortfall_with_typed_sizes() {
    let error = ensure_temporary_space(900, 899).unwrap_err();
    assert!(matches!(
        error,
        StorageError::MaintenanceInsufficientSpace {
            required_bytes: 900,
            available_bytes: 899
        }
    ));
}

#[test]
fn vacuum_space_rejects_integer_overflow() {
    assert!(matches!(
        required_temporary_space(u64::MAX, 0),
        Err(StorageError::MaintenanceSpaceCalculationOverflow)
    ));
}

#[test]
fn failed_compaction_restores_wal_and_full_synchronous_mode() {
    let directory = tempfile::tempdir().unwrap();
    let connection = Connection::open(directory.path().join("maintenance.sqlite")).unwrap();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    connection
        .pragma_update(None, "synchronous", "FULL")
        .unwrap();

    let result = compact_and_restore(&connection, |connection| {
        connection
            .pragma_update(None, "journal_mode", "DELETE")
            .unwrap();
        Err(StorageError::MaintenanceJournalModeUnavailable)
    });

    assert!(matches!(
        result,
        Err(StorageError::MaintenanceJournalModeUnavailable)
    ));
    let journal: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    let synchronous: u32 = connection
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
    assert_eq!(synchronous, 2);
    assert!(matches!(
        super::restore_operational_mode(&connection, "not-a-journal-mode", 2),
        Err(StorageError::MaintenanceRestoreFailed { .. })
    ));
    connection
        .execute_batch(
            "CREATE TABLE after_failure (value INTEGER); INSERT INTO after_failure VALUES (7);",
        )
        .unwrap();
    drop(connection);
    let reopened = Connection::open(directory.path().join("maintenance.sqlite")).unwrap();
    let value: i64 = reopened
        .query_row("SELECT value FROM after_failure", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, 7);
}

#[test]
fn maintenance_converts_a_migrated_database_to_the_target_page_size() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.sqlite");
    let mut connection = Connection::open(&path).unwrap();
    connection.pragma_update(None, "page_size", 4_096).unwrap();
    crate::storage::migrations::migrate(&mut connection).unwrap();
    connection
        .execute(
            "INSERT INTO rides (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
             VALUES ('maintenance-test-ride', 'live', 'draft', 1, 1, 0, 0)",
            [],
        )
        .unwrap();
    drop(connection);

    let mut connection = Connection::open(&path).unwrap();
    configure_connection(&mut connection, &path).unwrap();
    let existing_page_size: u32 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    assert_eq!(existing_page_size, 4_096);

    run(&mut connection, &path).unwrap();

    let page_size: u32 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    assert_eq!(page_size, DATABASE_PAGE_SIZE_BYTES);
    let retained_rides: u64 = connection
        .query_row("SELECT COUNT(*) FROM rides", [], |row| row.get(0))
        .unwrap();
    assert_eq!(retained_rides, 1);
}

#[test]
fn maintenance_refuses_unpublished_pevcap_import_state() {
    let mut connection = Connection::open_in_memory().unwrap();
    crate::storage::migrations::migrate(&mut connection).unwrap();
    let digest = "0".repeat(64);
    connection
        .execute(
            "INSERT INTO pevcap_import_work (artifact_digest, artifact_path, ride_id)
             VALUES (?1, 'staged.pevcap', NULL)",
            [&digest],
        )
        .unwrap();
    assert!(matches!(
        reject_active_work(&connection),
        Err(StorageError::MaintenancePevcapImportInProgress)
    ));
    connection
        .execute("DELETE FROM pevcap_import_work", [])
        .unwrap();
    connection
        .execute(
            "INSERT INTO pevcap_captures (artifact_digest, encoding, artifact_size)
             VALUES (?1, 'jsonl', 1)",
            [&digest],
        )
        .unwrap();
    assert!(matches!(
        reject_active_work(&connection),
        Err(StorageError::MaintenancePevcapImportInProgress)
    ));
}
