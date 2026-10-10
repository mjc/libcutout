use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use cutout_ride_maps::{
    Coordinate, LocationSample, LocationSource, RideMapSegmentId, RouteTelemetryState,
};
use rusqlite::Connection;

use super::{music_test_database, test_guard};
use crate::{RideDatabase, RideSessionMarkerWriteStatus, RideSessionMarkerWriter, StorageError};

fn hold_worker(database: &RideDatabase) -> (SyncSender<()>, Receiver<()>) {
    let ride = database.create_started_live_ride(1_000, 100, None).unwrap();
    let (entered_tx, entered_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let pending = database
        .enqueue_location_with_worker_gate_for_test(
            ride,
            LocationSample::new(
                Coordinate::from_degrees(40.0, -105.0).unwrap(),
                1_001,
                1_700_000_000_001,
                None,
                LocationSource::Live,
            ),
            RideMapSegmentId::new(0),
            RouteTelemetryState::GpsOnly,
            entered_tx,
            release_rx,
        )
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(pending);
    (release_tx, entered_rx)
}

fn settle(writer: &mut RideSessionMarkerWriter, first_now_ms: u64) {
    let started = Instant::now();
    loop {
        if writer.poll(first_now_ms + u64::try_from(started.elapsed().as_millis()).unwrap())
            == RideSessionMarkerWriteStatus::Committed
        {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "marker never reached durable success"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn marker_receipt_is_pending_until_busy_worker_commits_exact_bytes() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-pending");
    let (release, _) = hold_worker(&database);
    let mut writer = RideSessionMarkerWriter::new(database.clone());
    let expected = vec![0, 255, 13, 10, 42];
    assert_eq!(
        writer.request(Some(expected.clone()), 100),
        RideSessionMarkerWriteStatus::Pending
    );
    assert_eq!(writer.poll(101), RideSessionMarkerWriteStatus::Pending);
    release.send(()).unwrap();
    settle(&mut writer, 102);
    assert_eq!(database.ride_session_marker().unwrap(), Some(expected));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_full_queue_retains_intent_without_waiting_and_retries_after_capacity_returns() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-full");
    let (release, _) = hold_worker(&database);
    let mut queued = Vec::new();
    loop {
        match database.queue_ride_checkpoint() {
            Ok(ticket) => queued.push(ticket),
            Err(StorageError::QueueFull) => break,
            Err(error) => panic!("unexpected queue result: {error}"),
        }
    }
    let mut writer = RideSessionMarkerWriter::new(database.clone());
    assert!(matches!(
        writer.request(Some(vec![7, 8, 9]), 100),
        RideSessionMarkerWriteStatus::Retrying(_)
    ));
    release.send(()).unwrap();
    for ticket in queued {
        ticket.wait_result().unwrap();
    }
    settle(&mut writer, 1_000);
    assert_eq!(database.ride_session_marker().unwrap(), Some(vec![7, 8, 9]));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_unchanged_durable_intent_needs_no_worker_capacity() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-dedupe");
    let mut writer = RideSessionMarkerWriter::new(database.clone());
    writer.request(Some(vec![1, 2, 3]), 100);
    settle(&mut writer, 101);
    let (release, _) = hold_worker(&database);
    let mut queued = Vec::new();
    loop {
        match database.queue_ride_checkpoint() {
            Ok(ticket) => queued.push(ticket),
            Err(StorageError::QueueFull) => break,
            Err(error) => panic!("unexpected queue result: {error}"),
        }
    }
    let repeated = writer.request(Some(vec![1, 2, 3]), 200);
    release.send(()).unwrap();
    for ticket in queued {
        ticket.wait_result().unwrap();
    }
    assert_eq!(repeated, RideSessionMarkerWriteStatus::Committed);
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_replaces_only_latest_pending_intent_and_orders_clear_after_inflight_save() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-replace-clear");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE marker_audit (kind TEXT NOT NULL);
        CREATE TRIGGER audit_marker_insert AFTER INSERT ON ride_session_marker BEGIN
            INSERT INTO marker_audit VALUES ('save'); END;
        CREATE TRIGGER audit_marker_update AFTER UPDATE ON ride_session_marker BEGIN
            INSERT INTO marker_audit VALUES ('save'); END;
        CREATE TRIGGER audit_marker_clear AFTER DELETE ON ride_session_marker BEGIN
            INSERT INTO marker_audit VALUES ('clear'); END;",
        )
        .unwrap();
    let (release, _) = hold_worker(&database);
    let mut writer = RideSessionMarkerWriter::new(database.clone());
    writer.request(Some(vec![1]), 100);
    for index in 2..100 {
        writer.request(Some(vec![index]), 101);
    }
    writer.request(None, 102);
    release.send(()).unwrap();
    settle(&mut writer, 103);
    assert_eq!(database.ride_session_marker().unwrap(), None);
    let events: Vec<String> = connection
        .prepare("SELECT kind FROM marker_audit ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(events, vec!["save", "clear"]);
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_failure_is_not_acknowledged_and_identical_intent_retries_after_deadline() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-failure");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_marker BEFORE INSERT ON ride_session_marker
        BEGIN SELECT RAISE(ABORT, 'marker test rejection'); END;",
        )
        .unwrap();
    let mut writer = RideSessionMarkerWriter::new(database.clone());
    writer.request(Some(vec![55]), 100);
    let started = Instant::now();
    loop {
        if matches!(writer.poll(101), RideSessionMarkerWriteStatus::Retrying(_)) {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(database.ride_session_marker().unwrap(), None);
    assert!(matches!(
        writer.request(Some(vec![55]), 102),
        RideSessionMarkerWriteStatus::Retrying(_)
    ));
    connection
        .execute_batch("DROP TRIGGER reject_marker")
        .unwrap();
    settle(&mut writer, 1_000);
    assert_eq!(database.ride_session_marker().unwrap(), Some(vec![55]));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_multiple_database_handles_share_one_ordered_latest_intent() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-shared");
    let other = RideDatabase::open(&path).unwrap();
    let (release, _) = hold_worker(&database);
    let mut first = RideSessionMarkerWriter::new(database.clone());
    let mut second = RideSessionMarkerWriter::new(other);
    first.request(Some(vec![10]), 100);
    second.request(None, 101);
    assert_eq!(first.acknowledged_marker(), None);
    release.send(()).unwrap();
    settle(&mut first, 102);
    settle(&mut second, 103);
    assert_eq!(database.ride_session_marker().unwrap(), None);
    assert_eq!(first.status(), RideSessionMarkerWriteStatus::Committed);
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_delayed_preference_migration_cannot_replace_current_clear() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-migration-fence");
    let mut writer = RideSessionMarkerWriter::new(database.clone());
    writer.request(None, 100);
    settle(&mut writer, 101);
    assert_eq!(
        writer.migrate(Some(vec![99]), 102),
        RideSessionMarkerWriteStatus::Committed
    );
    settle(&mut writer, 103);
    assert_eq!(database.ride_session_marker().unwrap(), None);
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn marker_retry_after_worker_restart_uses_recovered_sender_for_every_later_write() {
    let _guard = test_guard();
    let (database, path) = music_test_database("marker-worker-restart");
    let mut writer = RideSessionMarkerWriter::new(database.clone());
    writer.request(Some(vec![1]), 100);
    settle(&mut writer, 101);
    database.stop_worker_for_test().unwrap();
    assert!(matches!(
        writer.request(Some(vec![2]), 200),
        RideSessionMarkerWriteStatus::Retrying(_)
    ));
    settle(&mut writer, 1_000);
    assert_eq!(database.ride_session_marker().unwrap(), Some(vec![2]));
    writer.request(Some(vec![3]), 2_000);
    settle(&mut writer, 3_000);
    assert_eq!(database.ride_session_marker().unwrap(), Some(vec![3]));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}
