use super::{LiveCaptureEventKind, QueryLimit, RideDatabase, RideEvent, StorageError, test_guard};
use rusqlite::Connection;

#[test]
fn queued_maintenance_preserves_rides_and_finished_captures_across_reopen() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("maintenance.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let initial_page_size: u32 = Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    assert_eq!(initial_page_size, 16 * 1024);

    let ride_id = database
        .create_started_live_ride(1_700_000_000_000, 1_000, Some("maintenance-test"))
        .unwrap();
    database
        .transition_at(ride_id, RideEvent::Stop, 1_001)
        .unwrap();
    let capture_id = database
        .begin_live_capture(b"header".to_vec(), 1_000)
        .unwrap();
    database
        .append_live_capture_event(
            capture_id,
            LiveCaptureEventKind::Notification,
            1_001,
            None,
            None,
            b"retained payload".to_vec(),
        )
        .unwrap();
    database.finish_live_capture(capture_id, 1_002).unwrap();

    let first = database.queue_storage_maintenance().unwrap();
    let duplicate = database.queue_storage_maintenance().unwrap();
    assert_eq!(first.wait_result().unwrap().compressed_event_count, 0);
    assert_eq!(duplicate.wait_result().unwrap().compressed_event_count, 0);
    assert!(database.find_ride(ride_id).unwrap().is_some());
    let capture = database
        .live_capture(capture_id, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(capture.events.len(), 1);
    assert_eq!(capture.events[0].payload, b"retained payload");
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    let page_size: u32 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    assert_eq!(page_size, 16 * 1024);
    let journal: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
    drop(connection);

    let reopened = RideDatabase::open(&path).unwrap();
    assert!(reopened.find_ride(ride_id).unwrap().is_some());
    assert_eq!(
        reopened
            .live_capture(capture_id, QueryLimit::new(10).unwrap())
            .unwrap()
            .events[0]
            .payload,
        b"retained payload"
    );
    reopened.shutdown().unwrap();
}

#[test]
fn maintenance_refuses_while_a_live_ride_is_active_or_paused() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("active-ride.sqlite")).unwrap();
    let ride = database
        .create_started_live_ride(1_700_000_000_000, 1_000, Some("bike"))
        .unwrap();
    let result = database.queue_storage_maintenance().unwrap().wait_result();
    assert!(matches!(result, Err(StorageError::MaintenanceActiveRide)));
    database
        .transition_at(ride, RideEvent::Pause, 1_001)
        .unwrap();
    let result = database.queue_storage_maintenance().unwrap().wait_result();
    assert!(matches!(result, Err(StorageError::MaintenanceActiveRide)));
    database.shutdown().unwrap();
}

#[test]
fn maintenance_refuses_while_a_live_capture_is_active() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("active-capture.sqlite")).unwrap();
    let _capture_id = database
        .begin_live_capture(b"active capture".to_vec(), 1_000)
        .unwrap();
    let result = database.queue_storage_maintenance().unwrap().wait_result();
    assert!(matches!(
        result,
        Err(StorageError::MaintenanceActiveCapture)
    ));
    database.shutdown().unwrap();
}

#[test]
fn checkpoint_busy_leaves_the_serial_worker_usable_for_retry() {
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkpoint-busy.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let ride_id = database
        .create_started_live_ride(1_700_000_000_000, 1_000, Some("checkpoint-test"))
        .unwrap();
    database
        .transition_at(ride_id, RideEvent::Stop, 1_001)
        .unwrap();

    let observer = Connection::open(&path).unwrap();
    let transaction = observer.unchecked_transaction().unwrap();
    transaction
        .query_row("SELECT COUNT(*) FROM rides", [], |row| row.get::<_, u64>(0))
        .unwrap();
    let first = database.queue_storage_maintenance().unwrap().wait_result();
    assert!(matches!(
        first,
        Err(StorageError::MaintenanceCheckpointBusy)
    ));
    drop(transaction);
    drop(observer);

    database
        .queue_storage_maintenance()
        .unwrap()
        .wait_result()
        .unwrap();
    assert!(database.find_ride(ride_id).unwrap().is_some());
    database.shutdown().unwrap();
}
