use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use cutout_core::{
    MonotonicTimestamp, PevcapCapture, PevcapEncoding, PevcapHeader, PevcapLocationSample,
    PevcapPhoneLocation, PevcapRecord, RawTelemetryReadback, WallClockUnixTimestamp,
};
use cutout_ride_maps::{
    Coordinate, LatitudeE7, LocationAdmission, LocationSample, LocationSource, LongitudeE7,
    MAX_LIVE_ROUTE_POINTS, MonotonicMilliseconds, RideEvent, RideMapSegmentId, RidePointSequence,
    RideSegmentStartReason, RouteDisplayBudget, RoutePrivacyGridE7, RoutePrivacyPolicy,
    RouteTelemetryState, RouteViewport, WallClockUnixMilliseconds,
};
use rusqlite::Connection;

use cutout_ride_maps::RideLifecycleState;

use super::{
    GeoBounds, PevcapImportOutcome, QueryLimit, RideDatabase, RideHistoryQuery, RideId, RideRecord,
    RideSource, RouteProjectionCancellation, StorageError, VoltageSagModelRecord,
};

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn pevcap_location(
    monotonic_ms: u64,
    latitude_degrees: f64,
    horizontal_accuracy_meters: f64,
) -> PevcapPhoneLocation {
    PevcapPhoneLocation {
        wall_clock_unix_ms: 1_700_000_000_000 + monotonic_ms,
        latitude_degrees,
        longitude_degrees: -105.0,
        altitude_meters: 1_600.0,
        horizontal_accuracy_meters: Some(horizontal_accuracy_meters),
        vertical_accuracy_meters: Some(4.0),
        speed_meters_per_second: Some(0.0),
        speed_accuracy_meters_per_second: Some(1.0),
        course_degrees: Some(0.0),
        course_accuracy_degrees: Some(1.0),
    }
}

fn pevcap_location_record(
    monotonic_ms: u64,
    latitude_degrees: f64,
    horizontal_accuracy_meters: f64,
) -> PevcapRecord {
    PevcapRecord::link_up(MonotonicTimestamp::new(monotonic_ms), None).with_phone_location(
        pevcap_location(monotonic_ms, latitude_degrees, horizontal_accuracy_meters),
    )
}

fn create_legacy_schema(path: &std::path::Path, version: i64) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "
            CREATE TABLE rides (
                id TEXT PRIMARY KEY NOT NULL,
                source TEXT NOT NULL,
                state TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                point_count INTEGER NOT NULL,
                distance_mm INTEGER NOT NULL
            );
            CREATE TABLE ride_points (
                ride_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                monotonic_ms INTEGER NOT NULL,
                wall_clock_ms INTEGER NOT NULL,
                latitude_e7 INTEGER NOT NULL,
                longitude_e7 INTEGER NOT NULL,
                horizontal_accuracy_mm INTEGER,
                source TEXT NOT NULL,
                PRIMARY KEY (ride_id, sequence)
            );
            ",
        )
        .unwrap();
    if version >= 2 {
        connection
            .execute_batch(
                "
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
                    hardware_verified INTEGER NOT NULL,
                    last_learned_wall_clock_ms INTEGER NOT NULL
                );
                CREATE TABLE ride_session_marker (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    marker BLOB NOT NULL
                );
                ",
            )
            .unwrap();
    }
    connection
        .pragma_update(None, "user_version", version)
        .unwrap();
}

#[test]
fn database_owns_one_service_and_reopens_persisted_rides() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let first = RideDatabase::open(&path).unwrap();
    let second = RideDatabase::open(&path).unwrap();
    assert_eq!(first.service_id(), second.service_id());
    assert!(first.capabilities().unwrap().sqlite_version().major() >= 3);

    let ride = first
        .create_ride(RideSource::Live, 1_600_000_000_000)
        .unwrap();
    first.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_000,
        1_700_000_000_000,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        first.append_location(ride, sample).unwrap(),
        LocationAdmission::Accepted
    );
    let second_sample = LocationSample::new(
        Coordinate::from_degrees(40.001, -105.001).unwrap(),
        2_000,
        1_700_000_002_000,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        first.append_location(ride, second_sample).unwrap(),
        LocationAdmission::Accepted
    );
    let export_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-timestamp-{}.json",
        uuid::Uuid::new_v4()
    ));
    first.export_ride_json(ride, &export_path).unwrap();
    let export = std::fs::read_to_string(&export_path).unwrap();
    assert!(export.contains("\"created_at_ms\":1600000000000"));
    assert!(!export.contains("\"updated_at_ms\":1600000000000"));
    let _ = std::fs::remove_file(export_path);
    assert_eq!(
        first.append_location(ride, second_sample).unwrap(),
        LocationAdmission::Duplicate
    );
    first.transition(ride, RideEvent::Stop).unwrap();
    first.transition(ride, RideEvent::Save).unwrap();
    first.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    let summary = reopened.summary(ride).unwrap();
    assert_eq!(summary.point_count().as_u64(), 2);
    reopened.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn current_database_repairs_monotonic_ride_creation_times() {
    let _guard = test_guard();
    let mut connection = Connection::open_in_memory().unwrap();
    crate::storage::create_current_schema(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO rides
             (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
             VALUES (?1, 'live', 'saved', 1_000, 1_700_000_002_000, 2, 1)",
            ["00000000-0000-0000-0000-000000000001"],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO ride_segments
             (ride_id, segment_id, sequence, start_reason, source,
              started_monotonic_ms, ended_monotonic_ms, started_wall_clock_ms, ended_wall_clock_ms)
             VALUES (?1, 0, 0, 'initial', 'live', 1_000, 1_000,
                     1_700_000_001_000, 1_700_000_001_000)",
            ["00000000-0000-0000-0000-000000000001"],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO ride_points
             (ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms,
              latitude_e7, longitude_e7, horizontal_accuracy_mm, source)
             VALUES (?1, 0, 0, 0, 1_000, 1_700_000_001_000, 400000000, -105000000, NULL, 'live')",
            ["00000000-0000-0000-0000-000000000001"],
        )
        .unwrap();

    crate::storage::repair_legacy_ride_creation_times(&mut connection).unwrap();

    let created_at_ms: u64 = connection
        .query_row(
            "SELECT created_at_ms FROM rides WHERE id = ?1",
            ["00000000-0000-0000-0000-000000000001"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(created_at_ms, 1_700_000_001_000);
}

#[test]
fn ride_updates_follow_domain_timestamps_without_regressing() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-domain-time-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let created_at_ms = 10_000_000_000_000;
    let sample_at_ms = created_at_ms + 1_000;
    let ride = database
        .create_ride(RideSource::Live, created_at_ms)
        .unwrap();

    database.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_000,
        sample_at_ms,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        database.append_location(ride, sample).unwrap(),
        LocationAdmission::Accepted
    );
    database.transition(ride, RideEvent::Stop).unwrap();
    database.transition(ride, RideEvent::Save).unwrap();

    let page = database
        .list_rides(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(page.rides()[0].updated_at_milliseconds(), sample_at_ms);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn queued_location_reports_worker_rejection() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-queued-location-error-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );

    assert!(matches!(
        database.enqueue_location_with_segment_and_telemetry(
            ride,
            sample,
            0,
            RouteTelemetryState::GpsOnly,
        ),
        Err(StorageError::InvalidRideState(RideLifecycleState::Draft))
    ));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn async_location_write_reports_completion_without_waiting() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-async-location-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );

    let pending = database
        .enqueue_location_async(ride, sample, 0, RouteTelemetryState::GpsOnly)
        .unwrap();
    let result = loop {
        if let Some(result) = pending.try_result().unwrap() {
            break result;
        }
        std::thread::yield_now();
    };
    assert!(matches!(result, Ok(LocationAdmission::Accepted)));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn async_location_write_can_bound_wait_for_a_delayed_worker() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-async-location-deadline-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();

    let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(0);
    let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(0);
    database
        .install_route_projection_test_gate(entered_sender, release_receiver)
        .unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );
    let pending = database
        .enqueue_location_async(ride, sample, 0, RouteTelemetryState::GpsOnly)
        .unwrap();
    entered_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("location write reaches the deliberate worker gate");

    assert!(matches!(
        pending.wait_result_until(Instant::now()),
        Ok(None)
    ));
    release_sender.send(()).unwrap();
    assert!(matches!(
        pending.wait_result(),
        Ok(Ok(LocationAdmission::Accepted))
    ));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn async_location_write_can_bound_wait_for_a_worker_gate() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-async-location-worker-gate-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();

    let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(0);
    let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(0);
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );
    let pending = database
        .enqueue_location_with_worker_gate_for_test(
            ride,
            sample,
            0,
            RouteTelemetryState::GpsOnly,
            entered_sender,
            release_receiver,
        )
        .unwrap();
    entered_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("location write reaches the worker gate before SQLite");

    assert!(matches!(
        pending.wait_result_until(Instant::now()),
        Ok(None)
    ));
    release_sender.send(()).unwrap();
    assert!(matches!(
        pending.wait_result(),
        Ok(Ok(LocationAdmission::Accepted))
    ));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn consumed_location_write_reports_worker_failure_and_recovers() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-worker-failure-location-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();

    let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(0);
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );
    let pending = database
        .enqueue_location_with_worker_failure_for_test(
            ride,
            sample,
            0,
            RouteTelemetryState::GpsOnly,
            entered_sender,
        )
        .unwrap();
    entered_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("worker consumed the location before failing");

    assert!(matches!(
        pending.wait_result_until(Instant::now() + Duration::from_secs(1)),
        Err(StorageError::ResponseDropped)
    ));
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut recovery = Err(StorageError::WorkerStopped);
    while Instant::now() < deadline {
        recovery = database
            .reopen()
            .and_then(|database| database.capabilities());
        if recovery.is_ok() {
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        recovery.is_ok(),
        "worker should recover in place: {recovery:?}"
    );
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn consumed_location_write_reconciles_after_worker_drops_response() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-worker-response-loss-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();

    let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(0);
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );
    let pending = database
        .enqueue_location_with_worker_failure_after_write_for_test(
            ride,
            sample,
            0,
            RouteTelemetryState::GpsOnly,
            entered_sender,
        )
        .unwrap();
    entered_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("worker commits the location before dropping its response");

    assert!(matches!(
        pending.wait_result_until(Instant::now() + Duration::from_secs(1)),
        Err(StorageError::ResponseDropped)
    ));

    let deadline = Instant::now() + Duration::from_secs(1);
    let mut summary = Err(StorageError::WorkerStopped);
    while Instant::now() < deadline {
        summary = database
            .reopen()
            .and_then(|database| database.summary(ride));
        if summary.is_ok() {
            break;
        }
        std::thread::yield_now();
    }
    assert_eq!(summary.unwrap().point_count(), 1.into());

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_reopens_after_an_unexpected_worker_exit() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-worker-recovery-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let stale_handle = database.clone();

    database.stop_worker_for_test().unwrap();
    // Commands on a stale handle transparently reacquire the process-wide service.
    assert!(stale_handle.capabilities().is_ok());

    let recovered = stale_handle.reopen().unwrap();
    assert_eq!(recovered.service_id(), stale_handle.service_id());
    assert!(recovered.capabilities().is_ok());

    recovered.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_commands_restart_the_worker_after_an_unexpected_exit() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-worker-auto-recovery-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let service_id = database.service_id();

    database.stop_worker_for_test().unwrap();
    assert!(database.capabilities().is_ok());
    assert_eq!(database.service_id(), service_id);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_history_duration_is_derived_from_rust_monotonic_state() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-duration-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        3_000,
        1_700_000_003_000,
        None,
        LocationSource::Live,
    );
    database.append_location(ride, sample).unwrap();

    let page = database
        .list_rides(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(page.rides()[0].duration_milliseconds(), 2_000);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_history_separates_wall_clock_order_from_monotonic_duration() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-dual-clock-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database
        .create_ride_with_monotonic_start(RideSource::Live, 1_700_000_000_000, Some(10_000))
        .unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    database
        .append_location(
            ride,
            LocationSample::new(
                Coordinate::from_degrees(40.0, -105.0).unwrap(),
                12_500,
                1_700_000_002_500,
                None,
                LocationSource::Live,
            ),
        )
        .unwrap();

    let record = database.find_ride(ride).unwrap().unwrap();
    assert_eq!(record.created_at_milliseconds(), 1_700_000_000_000);
    assert_eq!(record.monotonic_created_at_milliseconds(), Some(10_000));
    assert_eq!(record.duration_milliseconds(), 2_500);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_history_duration_is_persisted_from_lifecycle_clock() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-lifecycle-duration-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database
        .create_ride_with_monotonic_start(RideSource::Live, 1_700_000_000_000, Some(1_000))
        .unwrap();
    database
        .transition_at(ride, RideEvent::Start, 1_000)
        .unwrap();
    database
        .transition_at(ride, RideEvent::Pause, 5_000)
        .unwrap();
    database
        .transition_at(ride, RideEvent::Resume, 7_000)
        .unwrap();
    database
        .transition_at(ride, RideEvent::Stop, 12_000)
        .unwrap();

    let record = database.find_ride(ride).unwrap().unwrap();
    assert_eq!(record.duration_milliseconds(), 9_000);
    assert_eq!(record.paused_at_milliseconds(), None);
    assert_eq!(record.paused_duration_milliseconds(), 2_000);
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    assert_eq!(
        reopened
            .find_ride(ride)
            .unwrap()
            .unwrap()
            .duration_milliseconds(),
        9_000
    );
    let reopened_record = reopened.find_ride(ride).unwrap().unwrap();
    assert_eq!(reopened_record.paused_at_milliseconds(), None);
    assert_eq!(reopened_record.paused_duration_milliseconds(), 2_000);
    reopened.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn find_and_list_ride_projections_agree_for_all_route_shapes() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-ride-projection-parity-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();

    let empty_ride = database
        .create_ride_with_monotonic_start(RideSource::Live, 1_700_000_000_000, Some(1_000))
        .unwrap();
    database.transition(empty_ride, RideEvent::Start).unwrap();
    database.transition(empty_ride, RideEvent::Stop).unwrap();
    database.transition(empty_ride, RideEvent::Save).unwrap();

    let one_point_ride = database
        .create_ride_with_monotonic_start(RideSource::Live, 1_700_000_001_000, Some(10_000))
        .unwrap();
    database
        .transition(one_point_ride, RideEvent::Start)
        .unwrap();
    database
        .append_location(
            one_point_ride,
            LocationSample::new(
                Coordinate::from_degrees(40.0, -105.0).unwrap(),
                12_500,
                1_700_000_012_500,
                Some(3_000),
                LocationSource::Live,
            ),
        )
        .unwrap();
    database
        .transition(one_point_ride, RideEvent::Stop)
        .unwrap();
    database
        .transition(one_point_ride, RideEvent::Save)
        .unwrap();

    let multi_segment_ride = database
        .create_ride_with_monotonic_start(RideSource::Live, 1_700_000_002_000, Some(20_000))
        .unwrap();
    database
        .transition(multi_segment_ride, RideEvent::Start)
        .unwrap();
    database
        .append_location_with_segment_and_telemetry(
            multi_segment_ride,
            LocationSample::new(
                Coordinate::from_degrees(40.0, -105.0).unwrap(),
                21_000,
                1_700_000_021_000,
                Some(3_000),
                LocationSource::Live,
            ),
            0,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();
    database
        .append_location_with_segment_and_telemetry(
            multi_segment_ride,
            LocationSample::new(
                Coordinate::from_degrees(40.001, -105.0).unwrap(),
                52_000,
                1_700_000_052_000,
                Some(3_000),
                LocationSource::Live,
            ),
            1,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();
    database
        .transition(multi_segment_ride, RideEvent::Stop)
        .unwrap();
    database
        .transition(multi_segment_ride, RideEvent::Save)
        .unwrap();

    let listed = database
        .list_rides(None, QueryLimit::new(10).unwrap())
        .unwrap();
    for ride_id in [empty_ride, one_point_ride, multi_segment_ride] {
        let listed_record = listed
            .rides()
            .iter()
            .find(|record| record.id() == ride_id)
            .unwrap();
        let found_record = database.find_ride(ride_id).unwrap().unwrap();
        assert_eq!(
            found_record.duration_milliseconds(),
            listed_record.duration_milliseconds()
        );
        assert_eq!(found_record.summary(), listed_record.summary());
        assert_eq!(found_record.segment_count(), listed_record.segment_count());
    }

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn queued_location_preserves_the_durable_admission_outcome() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-queued-location-admission-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1_000).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        database
            .enqueue_location_with_segment_and_telemetry(
                ride,
                sample,
                0,
                RouteTelemetryState::GpsOnly,
            )
            .unwrap(),
        LocationAdmission::Accepted
    );
    assert_eq!(
        database
            .enqueue_location_with_segment_and_telemetry(
                ride,
                sample,
                0,
                RouteTelemetryState::GpsOnly,
            )
            .unwrap(),
        LocationAdmission::Duplicate
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_persists_migrated_mobile_state() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-mobile-state-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    database.save_selected_device("ios-local-aero", 42).unwrap();
    assert_eq!(
        database.selected_device().unwrap().as_deref(),
        Some("ios-local-aero")
    );
    database
        .save_device_name("ios-local-aero", "NF2557", 43)
        .unwrap();
    assert_eq!(
        database.device_name("ios-local-aero").unwrap().as_deref(),
        Some("NF2557")
    );
    let model = VoltageSagModelRecord {
        schema_version: 1,
        effective_resistance_milliohms: 37,
        observations: 4,
        hardware_verified: true,
        last_learned_wall_clock_milliseconds: 43,
    };
    database.save_voltage_sag_model("device-1", model).unwrap();
    assert_eq!(database.voltage_sag_model("device-1").unwrap(), Some(model));
    database.save_ride_session_marker(&[1, 2, 3]).unwrap();
    assert_eq!(database.ride_session_marker().unwrap(), Some(vec![1, 2, 3]));
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    assert_eq!(
        reopened.selected_device().unwrap().as_deref(),
        Some("ios-local-aero")
    );
    assert_eq!(
        reopened.device_name("ios-local-aero").unwrap().as_deref(),
        Some("NF2557")
    );
    assert_eq!(reopened.voltage_sag_model("device-1").unwrap(), Some(model));
    assert_eq!(reopened.ride_session_marker().unwrap(), Some(vec![1, 2, 3]));
    reopened.clear_selected_device().unwrap();
    reopened.remove_voltage_sag_model("device-1").unwrap();
    reopened.clear_ride_session_marker().unwrap();
    assert_eq!(reopened.selected_device().unwrap(), None);
    assert_eq!(reopened.voltage_sag_model("device-1").unwrap(), None);
    assert_eq!(reopened.ride_session_marker().unwrap(), None);
    reopened.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_preflights_confirms_and_deduplicates_managed_pevcap_artifacts() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let capture = PevcapCapture::new(
        header,
        vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(1), None).with_phone_location(
                PevcapPhoneLocation {
                    wall_clock_unix_ms: 1_700_000_000_000,
                    latitude_degrees: 40.0,
                    longitude_degrees: -105.0,
                    altitude_meters: 1_600.0,
                    horizontal_accuracy_meters: Some(3.0),
                    vertical_accuracy_meters: Some(4.0),
                    speed_meters_per_second: Some(0.0),
                    speed_accuracy_meters_per_second: Some(1.0),
                    course_degrees: Some(0.0),
                    course_accuracy_degrees: Some(1.0),
                },
            ),
        ],
    );
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.outcome(), PevcapImportOutcome::RideAndCapture);
    assert_eq!(preview.record_count(), 1);
    assert_eq!(preview.location_count(), 1);
    let first = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    assert!(!first.duplicate);
    assert_eq!(first.record_count, 1);
    assert_eq!(first.location_count, 1);
    assert_eq!(first.outcome, PevcapImportOutcome::RideAndCapture);
    let ride_id = first.ride_id.unwrap();
    assert_eq!(database.summary(ride_id).unwrap().point_count().as_u64(), 1);
    assert!(first.managed_artifact_path.exists());
    assert!(
        first
            .managed_artifact_path
            .metadata()
            .unwrap()
            .permissions()
            .readonly()
    );
    assert_ne!(first.managed_artifact_path, artifact_path);
    assert!(matches!(
        database.append_location(
            ride_id,
            LocationSample::new(
                Coordinate::from_degrees(40.1, -105.1).unwrap(),
                2,
                1_700_000_000_001,
                None,
                LocationSource::PevcapImport,
            )
        ),
        Err(StorageError::InvalidRideState(RideLifecycleState::Imported))
    ));
    let second = database
        .confirm_pevcap_import(&preview, 1_700_000_000_001)
        .unwrap();
    assert!(second.duplicate);
    assert_eq!(second.ride_id, first.ride_id);
    assert_eq!(second.managed_artifact_path, first.managed_artifact_path);
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&first.managed_artifact_path);
    let _ = std::fs::remove_dir(first.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[allow(
    clippy::too_many_lines,
    reason = "integration test covers the complete canonical import contract"
)]
#[test]
fn pevcap_import_uses_live_admission_and_segmentation_policy() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-policy-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-policy-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let capture = PevcapCapture::new(
        header,
        vec![
            pevcap_location_record(1_000, 40.0, 3.0)
                .with_telemetry(RawTelemetryReadback::default()),
            pevcap_location_record(1_000, 40.0, 3.0),
            pevcap_location_record(900, 40.0001, 3.0),
            pevcap_location_record(2_000, 41.0, 3.0),
            pevcap_location_record(3_000, 40.000_001, 200.0),
            pevcap_location_record(4_000, 40.000_001, 3.0),
            pevcap_location_record(5_000, 91.0, 3.0),
            pevcap_location_record(6_000, 40.000_001, -1.0),
            {
                let mut location = pevcap_location(7_000, 40.000_001, 3.0);
                location.wall_clock_unix_ms = 0;
                PevcapRecord::link_up(MonotonicTimestamp::new(7_000), None)
                    .with_phone_location(location)
            },
            pevcap_location_record(40_000, 40.001, 3.0),
        ],
    );
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.record_count(), 10);
    assert_eq!(preview.location_count(), 3);
    assert_eq!(preview.outcome(), PevcapImportOutcome::RideAndCapture);

    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    assert_eq!(receipt.location_count, 3);
    let ride_id = receipt.ride_id.unwrap();
    let points = database
        .route_points(ride_id, None, QueryLimit::new(10).unwrap())
        .unwrap()
        .points()
        .to_vec();
    assert_eq!(points.len(), 3);
    assert_eq!(
        points
            .iter()
            .map(|point| point.segment_id())
            .collect::<Vec<_>>(),
        vec![
            RideMapSegmentId::new(0),
            RideMapSegmentId::new(0),
            RideMapSegmentId::new(1),
        ]
    );
    assert!(points.iter().all(|point| {
        point.sample().source() == LocationSource::PevcapImport
            && point.telemetry_state() == RouteTelemetryState::GpsOnly
    }));
    assert_eq!(
        database
            .find_ride(ride_id)
            .unwrap()
            .unwrap()
            .segment_count(),
        2
    );
    let managed_capture = PevcapCapture::decode(
        &std::fs::read(&receipt.managed_artifact_path).unwrap(),
        PevcapEncoding::Jsonl,
    )
    .unwrap();
    assert_eq!(
        managed_capture.records[0].telemetry,
        Some(RawTelemetryReadback::default())
    );

    let live_ride = database
        .create_ride(RideSource::Live, 1_700_000_000_000)
        .unwrap();
    database.transition(live_ride, RideEvent::Start).unwrap();
    for (monotonic_ms, latitude_degrees, segment_id) in [
        (1_000, 40.0, 0),
        (4_000, 40.000_001, 0),
        (40_000, 40.001, 1),
    ] {
        let sample = LocationSample::new(
            Coordinate::from_degrees(latitude_degrees, -105.0).unwrap(),
            monotonic_ms,
            1_700_000_000_000 + monotonic_ms,
            Some(3_000),
            LocationSource::Live,
        );
        assert!(matches!(
            database.append_location_with_segment_and_telemetry(
                live_ride,
                sample,
                segment_id,
                RouteTelemetryState::GpsOnly,
            ),
            Ok(LocationAdmission::Accepted)
        ));
    }
    assert_eq!(
        database.summary(live_ride).unwrap(),
        database.summary(ride_id).unwrap()
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&receipt.managed_artifact_path);
    let _ = std::fs::remove_dir(receipt.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_imports_independent_location_samples_into_the_route() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-independent-location-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-independent-location-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let location = PevcapLocationSample::new(
        MonotonicTimestamp::new(1_000),
        pevcap_location(1_000, 40.0, 3.0),
        Some(false),
        Some(false),
    )
    .unwrap();
    let capture = PevcapCapture::new_with_locations(header, vec![], vec![location]);
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.record_count(), 0);
    assert_eq!(preview.location_count(), 1);
    assert_eq!(preview.outcome(), PevcapImportOutcome::RideAndCapture);

    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    let ride_id = receipt.ride_id.unwrap();
    assert_eq!(receipt.location_count, 1);
    let points = database
        .route_points(ride_id, None, QueryLimit::new(10).unwrap())
        .unwrap()
        .points()
        .to_vec();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].sample().source(), LocationSource::PevcapImport);

    let managed_capture = PevcapCapture::decode(
        &std::fs::read(&receipt.managed_artifact_path).unwrap(),
        PevcapEncoding::Jsonl,
    )
    .unwrap();
    assert_eq!(managed_capture.records.len(), 0);
    assert_eq!(managed_capture.locations, vec![location]);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&receipt.managed_artifact_path);
    let _ = std::fs::remove_dir(receipt.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_import_skips_invalid_location_with_decoder_reason() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-rejected-location-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-rejected-location-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let valid_location = PevcapLocationSample::new(
        MonotonicTimestamp::new(1_000),
        pevcap_location(1_000, 40.0, 3.0),
        None,
        None,
    )
    .unwrap();
    let invalid_location = valid_location
        .to_jsonl_line()
        .unwrap()
        .replace("40.0", "91.0");
    let input = format!(
        "{}\n{}\n",
        header.to_jsonl_line().unwrap(),
        invalid_location
    );
    std::fs::write(&artifact_path, input).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.location_count(), 0);
    assert_eq!(preview.outcome(), PevcapImportOutcome::CaptureOnly);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_import_preserves_interleaved_location_and_record_order() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-interleaved-location-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-interleaved-location-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let first_location = PevcapLocationSample::new(
        MonotonicTimestamp::new(1_000),
        pevcap_location(1_000, 40.0, 3.0),
        None,
        None,
    )
    .unwrap();
    let record = PevcapRecord::link_up(MonotonicTimestamp::new(2_000), None)
        .with_phone_location(pevcap_location(2_000, 40.0001, 3.0));
    let input = format!(
        "{}\n{}\n{}\n",
        header.to_jsonl_line().unwrap(),
        first_location.to_jsonl_line().unwrap(),
        record.to_jsonl_line().unwrap()
    );
    std::fs::write(&artifact_path, input).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.record_count(), 1);
    assert_eq!(preview.location_count(), 2);

    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    let ride_id = receipt.ride_id.unwrap();
    let points = database
        .route_points(ride_id, None, QueryLimit::new(10).unwrap())
        .unwrap()
        .points()
        .to_vec();
    assert_eq!(points.len(), 2);
    assert_eq!(
        points
            .iter()
            .map(|point| point.sample().monotonic_milliseconds())
            .collect::<Vec<_>>(),
        vec![
            MonotonicMilliseconds::new(1_000),
            MonotonicMilliseconds::new(2_000)
        ]
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&receipt.managed_artifact_path);
    let _ = std::fs::remove_dir(receipt.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_with_no_admitted_locations_is_capture_only() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-invalid-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-invalid-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let capture = PevcapCapture::new(header, vec![pevcap_location_record(1_000, 40.0, 200.0)]);
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.location_count(), 0);
    assert_eq!(preview.outcome(), PevcapImportOutcome::CaptureOnly);
    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    assert_eq!(receipt.ride_id, None);
    assert!(
        database
            .list_rides(None, QueryLimit::new(10).unwrap())
            .unwrap()
            .rides()
            .is_empty()
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&receipt.managed_artifact_path);
    let _ = std::fs::remove_dir(receipt.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn malformed_pevcap_import_does_not_publish_an_orphan_ride() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-malformed-pevcap-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-malformed-pevcap-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let backup_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-malformed-pevcap-backup-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&artifact_path, b"not a PEVCAP document\n").unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    assert!(
        database
            .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
            .is_err()
    );
    database.backup_to(&backup_path).unwrap();
    database.shutdown().unwrap();

    let connection = Connection::open(&backup_path).unwrap();
    let ride_count: u64 = connection
        .query_row("SELECT COUNT(*) FROM rides", [], |row| row.get(0))
        .unwrap();
    assert_eq!(ride_count, 0);

    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
    let _ = std::fs::remove_file(backup_path);
}

#[test]
fn capture_only_pevcap_import_does_not_publish_an_empty_ride() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-capture-only-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-capture-only-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let capture = PevcapCapture::new(
        header,
        vec![PevcapRecord::link_up(MonotonicTimestamp::new(1), None)],
    );
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.outcome(), PevcapImportOutcome::CaptureOnly);
    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    assert_eq!(receipt.ride_id, None);
    assert_eq!(receipt.outcome, PevcapImportOutcome::CaptureOnly);
    assert!(
        database
            .list_rides(None, QueryLimit::new(10).unwrap())
            .unwrap()
            .rides()
            .is_empty()
    );
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&receipt.managed_artifact_path);
    let _ = std::fs::remove_dir(receipt.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_confirmation_rejects_a_source_changed_after_preflight() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-changed-pevcap-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-changed-pevcap-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let original = PevcapCapture::new(
        header.clone(),
        vec![PevcapRecord::link_up(MonotonicTimestamp::new(1), None)],
    );
    std::fs::write(&artifact_path, original.to_jsonl().unwrap()).unwrap();
    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();

    let changed = PevcapCapture::new(
        header,
        vec![PevcapRecord::link_up(MonotonicTimestamp::new(2), None)],
    );
    std::fs::write(&artifact_path, changed.to_jsonl().unwrap()).unwrap();
    assert!(matches!(
        database.confirm_pevcap_import(&preview, 1_700_000_000_000),
        Err(StorageError::PevcapPreviewChanged)
    ));
    assert!(
        database
            .list_rides(None, QueryLimit::new(10).unwrap())
            .unwrap()
            .rides()
            .is_empty()
    );
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_preflight_rejects_artifact_and_duration_limit_overruns() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-limits-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&database_path).unwrap();
    let oversized_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-oversized-pevcap-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    std::fs::File::create(&oversized_path)
        .unwrap()
        .set_len(512 * 1024 * 1024 + 1)
        .unwrap();
    assert!(matches!(
        database.preflight_pevcap(&oversized_path, PevcapEncoding::Jsonl),
        Err(StorageError::PevcapLimitExceeded {
            resource: "artifact bytes",
            ..
        })
    ));

    let long_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-long-pevcap-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "darwin",
        None,
        &[],
        &[],
        None,
        None,
        "test",
        [0; 32],
        &[],
    )
    .unwrap();
    let capture = PevcapCapture::new(
        header,
        vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(0), None),
            PevcapRecord::link_down(MonotonicTimestamp::new(86_400_001)),
        ],
    );
    std::fs::write(&long_path, capture.to_jsonl().unwrap()).unwrap();
    assert!(matches!(
        database.preflight_pevcap(&long_path, PevcapEncoding::Jsonl),
        Err(StorageError::PevcapLimitExceeded {
            resource: "duration milliseconds",
            ..
        })
    ));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(oversized_path);
    let _ = std::fs::remove_file(long_path);
}

#[test]
fn reopen_removes_abandoned_pevcap_work_and_draft_ride() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-abandoned-pevcap-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let managed_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-abandoned-pevcap-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&managed_path, b"abandoned").unwrap();
    let database = RideDatabase::open(&database_path).unwrap();
    database.shutdown().unwrap();
    let ride_id = uuid::Uuid::new_v4().to_string();
    let connection = Connection::open(&database_path).unwrap();
    connection
        .execute(
            "INSERT INTO rides
                (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
             VALUES (?1, 'pevcap_import', 'draft', 1, 1, 0, 0)",
            [&ride_id],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO pevcap_import_work (artifact_digest, artifact_path, ride_id)
             VALUES (?1, ?2, ?3)",
            rusqlite::params!["0".repeat(64), managed_path.to_string_lossy(), ride_id],
        )
        .unwrap();
    drop(connection);

    let reopened = RideDatabase::open(&database_path).unwrap();
    assert!(!managed_path.exists());
    assert!(
        reopened
            .list_rides(None, QueryLimit::new(10).unwrap())
            .unwrap()
            .rides()
            .is_empty()
    );
    reopened.shutdown().unwrap();
    let connection = Connection::open(&database_path).unwrap();
    let work_count: u64 = connection
        .query_row("SELECT COUNT(*) FROM pevcap_import_work", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(work_count, 0);
    drop(connection);
    let _ = std::fs::remove_file(database_path);
}

#[test]
fn legacy_schema_versions_migrate_to_the_current_schema() {
    let _guard = test_guard();
    for version in [1_i64, 2_i64] {
        let path = std::env::temp_dir().join(format!(
            "libcutout-persistence-legacy-v{version}-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        create_legacy_schema(&path, version);
        let ride_id = uuid::Uuid::new_v4().to_string();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "INSERT INTO rides
                    (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
                 VALUES (?1, 'live', 'saved', 10, 20, 1, 1234)",
                [&ride_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ride_points
                    (ride_id, sequence, monotonic_ms, wall_clock_ms, latitude_e7,
                     longitude_e7, horizontal_accuracy_mm, source)
                 VALUES (?1, 0, 1, 2, 400000000, -1050000000, NULL, 'live')",
                [&ride_id],
            )
            .unwrap();
        if version >= 2 {
            connection
                .execute(
                    "INSERT INTO selected_device (id, platform_identifier, updated_at_ms)
                     VALUES (1, 'legacy-device', 42)",
                    [],
                )
                .unwrap();
        }
        drop(connection);
        let database = RideDatabase::open(&path).unwrap();
        database.shutdown().unwrap();

        let connection = Connection::open(&path).unwrap();
        let current_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(current_version, 14);
        let devices_table: String = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'devices'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(devices_table, "devices");
        let pevcap_table: String = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'pevcap_imports'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pevcap_table, "pevcap_imports");
        let persisted_ride: (String, u64, u64) = connection
            .query_row(
                "SELECT id, point_count, distance_mm FROM rides WHERE id = ?1",
                [&ride_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(persisted_ride, (ride_id, 1, 1234));
        if version >= 2 {
            let selected: String = connection
                .query_row(
                    "SELECT platform_identifier FROM selected_device WHERE singleton_key = ?1",
                    [crate::storage::SelectedDeviceKey::VALUE.blob()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(selected, "legacy-device");
        }
        let _ = std::fs::remove_file(path);
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "migration fixture keeps both legacy schemas explicit"
)]
fn schema_ten_and_eleven_migrations_create_segment_rows_and_foreign_keys() {
    let _guard = test_guard();
    for version in [10_i64, 11_i64] {
        let path = std::env::temp_dir().join(format!(
            "libcutout-persistence-segment-migration-v{version}-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let connection = Connection::open(&path).unwrap();
        crate::storage::create_current_schema(&connection).unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = OFF;
                 DROP TABLE ride_points;
                 DROP TABLE ride_segments;
                 DROP TABLE rides;",
            )
            .unwrap();
        let duration_columns = if version >= 11 {
            ", duration_ms INTEGER NOT NULL DEFAULT 0,
               paused_at_ms INTEGER,
               paused_duration_ms INTEGER NOT NULL DEFAULT 0"
        } else {
            ""
        };
        connection
            .execute_batch(&format!(
                "CREATE TABLE rides (
                    id TEXT PRIMARY KEY NOT NULL,
                    source TEXT NOT NULL,
                    state TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    monotonic_created_at_ms INTEGER,
                    updated_at_ms INTEGER NOT NULL,
                    point_count INTEGER NOT NULL,
                    distance_mm INTEGER NOT NULL,
                    candidate_vehicle TEXT,
                    associated_vehicle TEXT,
                    associated_at_ms INTEGER,
                    last_telemetry_at_ms INTEGER{duration_columns}
                );
                CREATE TABLE ride_points (
                    ride_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    segment_id INTEGER NOT NULL DEFAULT 0,
                    telemetry_state INTEGER NOT NULL DEFAULT 0,
                    monotonic_ms INTEGER NOT NULL,
                    wall_clock_ms INTEGER NOT NULL,
                    latitude_e7 INTEGER NOT NULL,
                    longitude_e7 INTEGER NOT NULL,
                    horizontal_accuracy_mm INTEGER,
                    source TEXT NOT NULL,
                    PRIMARY KEY (ride_id, sequence)
                );
                PRAGMA user_version = {version};"
            ))
            .unwrap();
        let ride_id = uuid::Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO rides
                    (id, source, state, created_at_ms, monotonic_created_at_ms, updated_at_ms,
                     point_count, distance_mm)
                 VALUES (?1, 'live', 'saved', 10, 1, 20, 1, 1234)",
                [&ride_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ride_points
                    (ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms,
                     latitude_e7, longitude_e7, horizontal_accuracy_mm, source)
                 VALUES (?1, 0, 0, 0, 1, 2, 400000000, -1050000000, NULL, 'live')",
                [&ride_id],
            )
            .unwrap();
        drop(connection);
        let database = RideDatabase::open(&path).unwrap();
        database.shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();

        let current_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(current_version, 14);
        let segment_count: u64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ride_segments WHERE ride_id = ?1",
                [&ride_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(segment_count, 1);
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        let foreign_key_errors: Vec<String> = connection
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(foreign_key_errors.is_empty());
        drop(connection);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn newer_schema_is_rejected_without_resetting_the_database() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-newer-schema-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", 99_i64)
        .unwrap();
    drop(connection);

    assert!(matches!(
        RideDatabase::open(&path),
        Err(StorageError::UnsupportedSchemaVersion(99))
    ));
    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 99);
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_indexes_trails_and_map_points_with_rtree() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-spatial-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    if !database.capabilities().unwrap().has_rtree() {
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
        return;
    }
    let trail = database.create_trail("Front Range").unwrap();
    let start = Coordinate::from_degrees(40.0, -105.0).unwrap();
    let end = Coordinate::from_degrees(40.001, -105.001).unwrap();
    database.append_trail_segment(trail, 0, start, end).unwrap();
    let bounds = GeoBounds::new(39.9, 40.1, -105.1, -104.9).unwrap();
    let segments = database
        .trail_segments_in_bounds(bounds, None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(segments.segments().len(), 1);
    let point = database.create_map_point("Charge", start).unwrap();
    let points = database
        .map_points_in_bounds(bounds, None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(points.points().len(), 1);
    assert_eq!(points.points()[0].id, point);
    database.rebuild_spatial_indexes().unwrap();
    assert_eq!(
        database
            .map_points_in_bounds(bounds, None, QueryLimit::new(1).unwrap())
            .unwrap()
            .points(),
        points.points()
    );
    let backup_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-backup-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    database.backup_to(&backup_path).unwrap();
    assert!(backup_path.is_file());
    let ride = database.create_ride(RideSource::Live, 1).unwrap();
    let export_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-export-{}.json",
        uuid::Uuid::new_v4()
    ));
    database.export_ride_json(ride, &export_path).unwrap();
    assert!(
        std::fs::read_to_string(&export_path)
            .unwrap()
            .contains("schema_version")
    );
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(backup_path);
    let _ = std::fs::remove_file(export_path);
}

#[test]
fn spatial_queries_validate_page_and_cross_the_antimeridian() {
    let _guard = test_guard();
    assert!(matches!(
        GeoBounds::new(f64::NAN, 1.0, 2.0, 3.0),
        Err(StorageError::InvalidGeographicBounds)
    ));
    assert!(matches!(
        GeoBounds::new(2.0, 1.0, 2.0, 3.0),
        Err(StorageError::InvalidGeographicBounds)
    ));

    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-antimeridian-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    if !database.capabilities().unwrap().has_rtree() {
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
        return;
    }
    for (name, longitude) in [("west", -179.5), ("east", 179.5), ("outside", 0.0)] {
        database
            .create_map_point(name, Coordinate::from_degrees(0.0, longitude).unwrap())
            .unwrap();
    }
    let bounds = GeoBounds::new(-1.0, 1.0, 179.0, -179.0).unwrap();
    let first = database
        .map_points_in_bounds(bounds, None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(first.points().len(), 1);
    assert!(first.next_cursor().is_some());
    let second = database
        .map_points_in_bounds(bounds, first.next_cursor(), QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(second.points().len(), 1);
    assert!(second.next_cursor().is_none());
    let mut names = [
        first.points()[0].name.as_str(),
        second.points()[0].name.as_str(),
    ];
    names.sort_unstable();
    assert_eq!(names, ["east", "west"]);
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_rejects_an_unrelated_current_version_schema() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-unrelated-schema-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE unrelated (id INTEGER); PRAGMA user_version = 3;")
        .unwrap();
    drop(connection);

    assert!(matches!(
        RideDatabase::open(&path),
        Err(StorageError::InvalidDatabaseIdentity)
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn current_schema_uses_uuid_singleton_keys_and_keeps_device_names_non_unique() {
    let _guard = test_guard();
    let connection = Connection::open_in_memory().unwrap();
    crate::storage::create_current_schema(&connection).unwrap();

    for table in ["selected_device", "ride_session_marker"] {
        let columns: Vec<(String, String)> = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| Ok((row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(columns[0].0, "singleton_key");
        assert_eq!(columns[0].1, "BLOB");
        assert!(!columns.iter().any(|(name, _)| name == "id"));
    }

    let device_columns: Vec<(String, String)> = connection
        .prepare("PRAGMA table_info(devices)")
        .unwrap()
        .query_map([], |row| Ok((row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        device_columns[0],
        ("platform_identifier".to_owned(), "TEXT".to_owned())
    );
    assert_eq!(
        device_columns[1],
        ("display_name".to_owned(), "TEXT".to_owned())
    );

    connection
        .execute(
            "INSERT INTO devices (platform_identifier, display_name, updated_at_ms)
             VALUES ('corebluetooth-a', 'NF2557', 1), ('corebluetooth-b', 'NF2557', 2)",
            [],
        )
        .unwrap();
    let count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM devices WHERE display_name = 'NF2557'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    assert!(
        connection
            .execute(
                "INSERT INTO devices (platform_identifier, display_name, updated_at_ms)
             VALUES ('corebluetooth-a', 'Renamed NF2557', 3)",
                [],
            )
            .is_err()
    );
}

#[test]
fn spatial_domain_ids_are_uuid_backed_and_rtree_keys_are_internal() {
    let _guard = test_guard();
    let connection = Connection::open_in_memory().unwrap();
    crate::storage::create_current_schema(&connection).unwrap();

    let map_point_columns: Vec<(String, String, i64, i64)> = connection
        .prepare("PRAGMA table_info(map_points)")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(1)?, row.get(2)?, row.get(3)?, row.get(5)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(map_point_columns[0], ("id".into(), "BLOB".into(), 1, 1));
    assert_eq!(map_point_columns[1].0, "name");

    let trail_segment_columns: Vec<(String, String, i64, i64)> = connection
        .prepare("PRAGMA table_info(trail_segments)")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(1)?, row.get(2)?, row.get(3)?, row.get(5)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        !trail_segment_columns
            .iter()
            .any(|(name, _, _, _)| name == "id")
    );
    assert_eq!(
        trail_segment_columns
            .iter()
            .find(|(name, _, _, _)| name == "trail_id")
            .map(|(_, _, _, pk)| *pk),
        Some(1)
    );
    assert_eq!(
        trail_segment_columns
            .iter()
            .find(|(name, _, _, _)| name == "sequence")
            .map(|(_, _, _, pk)| *pk),
        Some(2)
    );

    for table in ["map_point_spatial_keys", "trail_segment_spatial_keys"] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "missing {table}");
    }
    let map_key_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name = 'map_point_spatial_keys'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(map_key_sql.contains("rtree_id"));
    assert!(!map_key_sql.contains(" id INTEGER PRIMARY KEY"));
}

#[test]
fn schema_v13_spatial_rows_migrate_without_integer_domain_ids() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-spatial-identity-migration-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let connection = Connection::open(&path).unwrap();
    crate::storage::create_current_schema(&connection).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE map_points_rtree;
             DROP TABLE map_point_spatial_keys;
             DROP TABLE map_points;
             DROP TABLE trail_segments_rtree;
             DROP TABLE trail_segment_spatial_keys;
             DROP TABLE trail_segments;
             CREATE TABLE trail_segments (
                 id INTEGER PRIMARY KEY,
                 trail_id TEXT NOT NULL REFERENCES trails(id) ON DELETE CASCADE,
                 sequence INTEGER NOT NULL,
                 start_lat_e7 INTEGER NOT NULL,
                 start_lon_e7 INTEGER NOT NULL,
                 end_lat_e7 INTEGER NOT NULL,
                 end_lon_e7 INTEGER NOT NULL,
                 UNIQUE (trail_id, sequence)
             );
             CREATE VIRTUAL TABLE trail_segments_rtree
                 USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
             CREATE TABLE map_points (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 latitude_e7 INTEGER NOT NULL,
                 longitude_e7 INTEGER NOT NULL
             );
             CREATE VIRTUAL TABLE map_points_rtree
                 USING rtree_i32(id, min_lat_e7, max_lat_e7, min_lon_e7, max_lon_e7);
             INSERT INTO trails (id, name)
             VALUES ('00000000-0000-0000-0000-000000000001', 'Legacy trail');
             INSERT INTO trail_segments
                 (id, trail_id, sequence, start_lat_e7, start_lon_e7, end_lat_e7, end_lon_e7)
             VALUES (7, '00000000-0000-0000-0000-000000000001', 0,
                     400000000, -1050000000, 400010000, -1050010000);
             INSERT INTO map_points (id, name, latitude_e7, longitude_e7)
             VALUES (11, 'Legacy point', 400000000, -1050000000);
             INSERT INTO trail_segments_rtree
                 VALUES (7, 400000000, 400010000, -1050010000, -1050000000);
             INSERT INTO map_points_rtree
                 VALUES (11, 400000000, 400000000, -1050000000, -1050000000);
             PRAGMA application_id = 1129665615;
             PRAGMA user_version = 13;",
        )
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    let bounds = GeoBounds::new(39.9, 40.1, -105.1, -104.9).unwrap();
    assert_eq!(
        database
            .trail_segments_in_bounds(bounds, None, QueryLimit::new(10).unwrap())
            .unwrap()
            .segments()
            .len(),
        1
    );
    let points = database
        .map_points_in_bounds(bounds, None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(points.points().len(), 1);
    assert_eq!(points.points()[0].name, "Legacy point");
    assert_ne!(points.points()[0].id.uuid(), uuid::Uuid::nil());
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 14);
    let rtree_id: i64 = connection
        .query_row(
            "SELECT rtree_id FROM trail_segment_spatial_keys",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rtree_id, 7);
    let point_id_type: String = connection
        .query_row(
            "SELECT type FROM pragma_table_info('map_points') WHERE name = 'id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(point_id_type, "BLOB");
    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn schema_v12_singleton_rows_migrate_to_uuid_keys_without_data_loss() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-singleton-key-migration-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let connection = Connection::open(&path).unwrap();
    crate::storage::create_current_schema(&connection).unwrap();
    connection
        .execute_batch(
            "DROP TABLE selected_device;
             DROP TABLE ride_session_marker;
             CREATE TABLE selected_device (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 platform_identifier TEXT NOT NULL,
                 updated_at_ms INTEGER NOT NULL
             );
             CREATE TABLE ride_session_marker (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 marker BLOB NOT NULL
             );
             INSERT INTO selected_device (id, platform_identifier, updated_at_ms)
             VALUES (1, 'corebluetooth-a', 42);
             INSERT INTO ride_session_marker (id, marker) VALUES (1, X'010203');
             PRAGMA application_id = 1129665615;
             PRAGMA user_version = 12;",
        )
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(
        database.selected_device().unwrap().as_deref(),
        Some("corebluetooth-a")
    );
    assert_eq!(database.ride_session_marker().unwrap(), Some(vec![1, 2, 3]));
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 14);
    let selected_key_length: u64 = connection
        .query_row(
            "SELECT length(singleton_key) FROM selected_device",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(selected_key_length, 16);
    assert!(
        !connection
            .prepare("PRAGMA table_info(selected_device)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .contains(&"id".to_owned())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn reopen_recovers_recording_rides_and_reports_them_in_bootstrap() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-recovery-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 10).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    assert_eq!(reopened.bootstrap().recovered_rides(), &[ride]);
    let page = reopened
        .list_rides(None, QueryLimit::new(10).unwrap())
        .unwrap();
    let recovered = page
        .rides()
        .iter()
        .find(|candidate| candidate.id() == ride)
        .unwrap();
    assert_eq!(recovered.state(), RideLifecycleState::Interrupted);
    reopened.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_history_and_route_queries_are_stably_bounded() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-bounded-history-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let mut rides = Vec::new();
    for created_at in [10_u64, 20, 30] {
        let ride = database.create_ride(RideSource::Live, created_at).unwrap();
        database.transition(ride, RideEvent::Start).unwrap();
        database.transition(ride, RideEvent::Stop).unwrap();
        database.transition(ride, RideEvent::Save).unwrap();
        rides.push(ride);
    }

    let first = database
        .list_rides(None, QueryLimit::new(2).unwrap())
        .unwrap();
    assert_eq!(first.rides().len(), 2);
    assert_eq!(first.rides()[0].id(), rides[2]);
    assert_eq!(first.rides()[1].id(), rides[1]);
    let second = database
        .list_rides(first.next_cursor(), QueryLimit::new(2).unwrap())
        .unwrap();
    assert_eq!(second.rides().len(), 1);
    assert_eq!(second.rides()[0].id(), rides[0]);
    let selected = database.find_ride(rides[0]).unwrap().expect("saved ride");
    assert_eq!(selected.id(), rides[0]);
    assert!(database.find_ride(RideId::new()).unwrap().is_none());

    let ride = database.create_ride(RideSource::Live, 40).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    for sequence in 0_u32..3 {
        let sample = LocationSample::new(
            Coordinate::from_degrees(40.0 + f64::from(sequence) / 10_000.0, -105.0).unwrap(),
            u64::from(sequence + 1),
            1_700_000_000_000 + u64::from(sequence),
            None,
            LocationSource::Live,
        );
        assert_eq!(
            database.append_location(ride, sample).unwrap(),
            LocationAdmission::Accepted
        );
    }
    let first = database
        .route_points(ride, None, QueryLimit::new(2).unwrap())
        .unwrap();
    assert_eq!(first.points().len(), 2);
    assert_eq!(first.points()[0].sequence(), RidePointSequence::new(0));
    assert_eq!(first.points()[1].sequence(), RidePointSequence::new(1));
    let second = database
        .route_points(ride, first.next_cursor(), QueryLimit::new(2).unwrap())
        .unwrap();
    assert_eq!(second.points().len(), 1);
    assert_eq!(second.points()[0].sequence(), RidePointSequence::new(2));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn latest_route_points_return_the_bounded_tail_in_ascending_order() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-tail-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 40).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    for sequence in 0..u64::try_from(MAX_LIVE_ROUTE_POINTS + 2).unwrap() {
        let offset = f64::from(u32::try_from(sequence).unwrap()) / 1_000_000.0;
        let sample = LocationSample::new(
            Coordinate::from_degrees(40.0 + offset, -105.0).unwrap(),
            sequence + 1,
            1_700_000_000_000 + sequence,
            None,
            LocationSource::Live,
        );
        assert_eq!(
            database.append_location(ride, sample).unwrap(),
            LocationAdmission::Accepted
        );
    }

    let points = database.latest_route_points(ride).unwrap();
    assert_eq!(points.len(), MAX_LIVE_ROUTE_POINTS);
    assert_eq!(
        points.first().unwrap().sequence(),
        RidePointSequence::new(2)
    );
    assert_eq!(
        points.last().unwrap().sequence(),
        RidePointSequence::new(u64::try_from(MAX_LIVE_ROUTE_POINTS + 1).unwrap())
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn durable_route_projection_is_bounded_and_viewport_filtered_in_rust() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-projection-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 40).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    for offset in 0..4_u64 {
        let sample = LocationSample::new(
            Coordinate::from_degrees(
                40.0 + f64::from(u32::try_from(offset).unwrap()) / 10_000.0,
                -105.0,
            )
            .unwrap(),
            offset + 1,
            1_700_000_000_000 + offset,
            None,
            LocationSource::Live,
        );
        assert_eq!(
            database.append_location(ride, sample).unwrap(),
            LocationAdmission::Accepted
        );
    }

    let viewport = RouteViewport::new(
        LatitudeE7::new(400_000_000),
        LatitudeE7::new(400_002_000),
        LongitudeE7::new(-1_050_000_000),
        LongitudeE7::new(-1_049_999_000),
    )
    .unwrap();
    let projection = database
        .project_route_points(
            ride,
            Some(viewport),
            RouteDisplayBudget::new(2).unwrap(),
            RoutePrivacyPolicy::grid(RoutePrivacyGridE7::new(1_000).unwrap()),
        )
        .unwrap();

    assert_eq!(projection.source_point_count(), 4);
    assert_eq!(projection.source_segment_count(), 1);
    assert_eq!(projection.candidate_segment_count(), 1);
    assert_eq!(projection.displayed_segment_count(), 1);
    assert_eq!(projection.points().len(), 2);
    assert_eq!(projection.points()[0].sequence().as_u64(), 0);
    assert_eq!(projection.points()[1].sequence().as_u64(), 2);
    assert!(projection.points().iter().all(|point| {
        point.privacy_class() == cutout_ride_maps::RoutePrivacyClass::GridRedacted
    }));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn durable_route_projection_reports_segments_omitted_by_display_budget() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-segment-projection-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 40).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    for (sample_number, (monotonic_ms, latitude_degrees, segment_id)) in [
        (1, 40.0, 0),
        (2_001, 40.0001, 0),
        (3_001, 40.0002, 0),
        (40_000, 40.0003, 1),
        (80_000, 40.0004, 2),
        (81_000, 40.0005, 2),
        (82_000, 40.0006, 2),
    ]
    .into_iter()
    .enumerate()
    {
        let sample = LocationSample::new(
            Coordinate::from_degrees(latitude_degrees, -105.0).unwrap(),
            monotonic_ms,
            1_700_000_000_000 + monotonic_ms,
            None,
            LocationSource::Live,
        );
        assert_eq!(
            database
                .append_location_with_segment_id(ride, sample, RideMapSegmentId::new(segment_id))
                .unwrap(),
            LocationAdmission::Accepted,
            "sample {sample_number} should be accepted"
        );
    }

    let projection = database
        .project_route_points(
            ride,
            None,
            RouteDisplayBudget::new(4).unwrap(),
            RoutePrivacyPolicy::Precise,
        )
        .unwrap();

    assert_eq!(projection.source_point_count(), 7);
    assert_eq!(projection.source_segment_count(), 3);
    assert_eq!(projection.candidate_segment_count(), 3);
    assert_eq!(projection.displayed_segment_count(), 2);
    assert_eq!(
        projection
            .points()
            .iter()
            .map(|point| point.segment_id().value())
            .collect::<Vec<_>>(),
        vec![0, 0, 2, 2]
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cancelled_durable_route_projection_returns_before_scanning() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-projection-cancel-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 40).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    for sequence in 0_u64..4_096 {
        let offset = f64::from(u32::try_from(sequence).unwrap()) / 1_000_000.0;
        let sample = LocationSample::new(
            Coordinate::from_degrees(40.0 + offset, -105.0).unwrap(),
            sequence + 1,
            1_700_000_000_000 + sequence,
            None,
            LocationSource::Live,
        );
        assert_eq!(
            database.append_location(ride, sample).unwrap(),
            LocationAdmission::Accepted
        );
    }
    let cancellation = RouteProjectionCancellation::new();
    cancellation.cancel();

    let error = database
        .project_route_points_cancellable(
            ride,
            None,
            RouteDisplayBudget::new(2).unwrap(),
            RoutePrivacyPolicy::Precise,
            cancellation,
        )
        .expect_err("a cancelled projection must not scan the route");

    assert!(matches!(error, StorageError::Cancelled));
    assert!(database.find_ride(ride).unwrap().is_some());
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn expired_durable_route_projection_returns_a_typed_deadline() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-projection-deadline-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 40).unwrap();
    let cancellation = RouteProjectionCancellation::with_deadline(
        Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap(),
    );

    let error = database
        .project_route_points_cancellable(
            ride,
            None,
            RouteDisplayBudget::new(2).unwrap(),
            RoutePrivacyPolicy::Precise,
            cancellation,
        )
        .expect_err("an expired projection must not enter the worker");

    assert!(matches!(error, StorageError::DeadlineExceeded));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn cancelled_in_flight_route_projection_leaves_worker_usable() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-projection-recovery-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 40).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    for sequence in 0_u64..4 {
        let sample = LocationSample::new(
            Coordinate::from_degrees(
                40.0 + f64::from(u32::try_from(sequence).unwrap()) / 10_000.0,
                -105.0,
            )
            .unwrap(),
            sequence + 1,
            1_700_000_000_000 + sequence,
            None,
            LocationSource::Live,
        );
        assert_eq!(
            database.append_location(ride, sample).unwrap(),
            LocationAdmission::Accepted
        );
    }

    let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(0);
    let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(0);
    database
        .install_route_projection_test_gate(entered_sender, release_receiver)
        .unwrap();

    let cancellation = RouteProjectionCancellation::new();
    let projection_database = database.clone();
    let projection_cancellation = cancellation.clone();
    let projection = std::thread::spawn(move || {
        projection_database.project_route_points_cancellable(
            ride,
            None,
            RouteDisplayBudget::new(2).unwrap(),
            RoutePrivacyPolicy::Precise,
            projection_cancellation,
        )
    });

    entered_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("projection reached a deliberately slow SQLite callback");
    cancellation.cancel();
    release_sender
        .send(())
        .expect("projection callback is released after cancellation");

    let error = projection
        .join()
        .expect("projection thread does not panic")
        .expect_err("an active projection reports typed cancellation");
    assert!(matches!(error, StorageError::Cancelled));

    assert!(database.find_ride(ride).unwrap().is_some());
    let next_sample = LocationSample::new(
        Coordinate::from_degrees(40.001, -105.0).unwrap(),
        5,
        1_700_000_000_005,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        database.append_location(ride, next_sample).unwrap(),
        LocationAdmission::Accepted
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn filtered_ride_history_queries_stay_rust_owned_and_bounded() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-filtered-history-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let first = database.create_ride(RideSource::Live, 10).unwrap();
    let second = database.create_ride(RideSource::Live, 20).unwrap();
    for ride in [first, second] {
        database.transition(ride, RideEvent::Start).unwrap();
        database.transition(ride, RideEvent::Stop).unwrap();
        database.transition(ride, RideEvent::Save).unwrap();
    }
    database.save_device_name("device-a", "NF2557", 30).unwrap();
    database
        .update_ride_map_metadata(first, None, Some("device-a"), None, None)
        .unwrap();

    let date_filtered = database
        .list_rides_filtered(
            None,
            QueryLimit::new(10).unwrap(),
            RideHistoryQuery::new(Some(15), None, None),
        )
        .unwrap();
    assert_eq!(
        date_filtered
            .rides()
            .iter()
            .map(RideRecord::id)
            .collect::<Vec<_>>(),
        vec![second]
    );

    let vehicle_filtered = database
        .list_rides_filtered(
            None,
            QueryLimit::new(10).unwrap(),
            RideHistoryQuery::new(None, Some("device-a"), None),
        )
        .unwrap();
    assert_eq!(
        vehicle_filtered
            .rides()
            .iter()
            .map(RideRecord::id)
            .collect::<Vec<_>>(),
        vec![first]
    );

    let name_searched = database
        .list_rides_filtered(
            None,
            QueryLimit::new(10).unwrap(),
            RideHistoryQuery::new(None, None, Some("nf2557")),
        )
        .unwrap();
    assert_eq!(
        name_searched
            .rides()
            .iter()
            .map(RideRecord::id)
            .collect::<Vec<_>>(),
        vec![first]
    );
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_history_queries_share_one_grouped_point_aggregation() {
    let find_sql = crate::storage::ride_records_sql(
        "WHERE rides.state NOT IN ('draft', 'discarded') AND rides.id = ?1",
    );
    let list_sql = crate::storage::ride_records_sql(
        "LEFT JOIN devices AS associated_device
             ON associated_device.platform_identifier = rides.associated_vehicle
         WHERE rides.state NOT IN ('draft', 'discarded')",
    );

    for sql in [&find_sql, &list_sql] {
        assert_eq!(sql.matches("GROUP BY ride_id").count(), 1);
        assert_eq!(sql.matches("LEFT JOIN ride_point_aggregates").count(), 1);
        assert_eq!(
            sql.matches("SELECT MAX(monotonic_ms) FROM ride_points")
                .count(),
            0
        );
        assert_eq!(
            sql.matches("SELECT MIN(monotonic_ms) FROM ride_points")
                .count(),
            0
        );
        assert_eq!(
            sql.matches("SELECT COUNT(DISTINCT segment_id) FROM ride_points")
                .count(),
            0
        );
    }
}

#[test]
fn version_eight_migration_adds_monotonic_ride_start_column() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-legacy-v8-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let connection = Connection::open(&path).unwrap();
    crate::storage::create_current_schema(&connection).unwrap();
    connection
        .execute_batch(
            "
            ALTER TABLE rides DROP COLUMN monotonic_created_at_ms;
            ALTER TABLE rides DROP COLUMN duration_ms;
            ALTER TABLE rides DROP COLUMN paused_at_ms;
            ALTER TABLE rides DROP COLUMN paused_duration_ms;
            DROP TABLE devices;
            PRAGMA application_id = 1129665615;
            PRAGMA user_version = 8;
            ",
        )
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let has_monotonic_start: bool = connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM pragma_table_info('rides')
                 WHERE name = 'monotonic_created_at_ms'
             )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 14);
    assert!(has_monotonic_start);

    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_history_excludes_explicitly_discarded_rides() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-discarded-history-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let discarded = database.create_ride(RideSource::Live, 10).unwrap();
    database.transition(discarded, RideEvent::Start).unwrap();
    database.transition(discarded, RideEvent::Stop).unwrap();
    database.transition(discarded, RideEvent::Discard).unwrap();

    let saved = database.create_ride(RideSource::Live, 20).unwrap();
    database.transition(saved, RideEvent::Start).unwrap();
    database.transition(saved, RideEvent::Stop).unwrap();
    database.transition(saved, RideEvent::Save).unwrap();

    let page = database
        .list_rides(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(page.rides().len(), 1);
    assert_eq!(page.rides()[0].id(), saved);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_history_persists_map_association_metadata() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-map-metadata-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 10).unwrap();
    database
        .update_ride_map_metadata(ride, Some("pev-1"), None, None, None)
        .unwrap();
    database
        .update_ride_map_metadata(ride, None, Some("pev-1"), Some(20), Some(21))
        .unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    database.transition(ride, RideEvent::Stop).unwrap();
    database.transition(ride, RideEvent::Save).unwrap();

    let page = database
        .list_rides(None, QueryLimit::new(10).unwrap())
        .unwrap();
    let record = page
        .rides()
        .iter()
        .find(|record| record.id() == ride)
        .unwrap();
    assert_eq!(record.candidate_vehicle(), None);
    assert_eq!(record.associated_vehicle(), Some("pev-1"));
    assert_eq!(record.associated_at_milliseconds(), Some(20));
    assert_eq!(record.last_telemetry_at_milliseconds(), Some(21));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn route_points_persist_telemetry_provenance() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-map-telemetry-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 10).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        11,
        1_700_000_000_011,
        Some(3_000),
        LocationSource::Live,
    );
    database
        .append_location_with_segment_and_telemetry(
            ride,
            sample,
            0,
            RouteTelemetryState::AssociatedFresh,
        )
        .unwrap();
    let page = database
        .route_points(ride, None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(
        page.points()[0].telemetry_state(),
        RouteTelemetryState::AssociatedFresh
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn route_points_reject_arbitrary_segment_id() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-segment-integrity-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 10).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = |monotonic_ms| {
        LocationSample::new(
            Coordinate::from_degrees(40.0, -105.0).unwrap(),
            monotonic_ms,
            1_700_000_000_000 + monotonic_ms,
            Some(3_000),
            LocationSource::Live,
        )
    };
    database
        .append_location_with_segment_and_telemetry(
            ride,
            sample(11),
            0,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();
    assert!(matches!(
        database.append_location_with_segment_and_telemetry(
            ride,
            sample(12),
            2,
            RouteTelemetryState::GpsOnly,
        ),
        Err(StorageError::InvalidSegmentId {
            expected: 0,
            actual: 2
        })
    ));
    database
        .append_location_with_segment_and_telemetry(
            ride,
            sample(30_012),
            1,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();
    assert!(matches!(
        database.append_location_with_segment_and_telemetry(
            ride,
            sample(30_013),
            3,
            RouteTelemetryState::GpsOnly,
        ),
        Err(StorageError::InvalidSegmentId {
            expected: 1,
            actual: 3
        })
    ));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn migrated_ride_tables_enforce_the_current_constraints() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-migrated-constraints-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    create_legacy_schema(&path, 1);
    let database = RideDatabase::open(&path).unwrap();
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    assert!(
        connection
            .execute(
                "INSERT INTO rides
                    (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
                 VALUES ('bad', 'not-a-source', 'active', 0, 0, -1, -1)",
                [],
            )
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO ride_points
                    (ride_id, sequence, monotonic_ms, wall_clock_ms, latitude_e7,
                     longitude_e7, horizontal_accuracy_mm, source)
                 VALUES ('missing', 0, 0, 0, 0, 0, NULL, 'live')",
                [],
            )
            .is_err()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_segments_persist_ordered_reason_and_source_metadata() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-ride-segments-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 10).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = |monotonic_ms| {
        LocationSample::new(
            Coordinate::from_degrees(40.0, -105.0).unwrap(),
            monotonic_ms,
            1_700_000_000_000 + monotonic_ms,
            Some(3_000),
            LocationSource::Live,
        )
    };
    database
        .append_location_with_segment_and_telemetry(
            ride,
            sample(11),
            0,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();
    database
        .append_location_with_segment_and_telemetry(
            ride,
            sample(40_012),
            1,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();

    let segments = database
        .ride_segments(ride, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].id(), RideMapSegmentId::new(0));
    assert_eq!(segments[0].sequence(), RidePointSequence::new(0));
    assert_eq!(segments[0].start_reason(), RideSegmentStartReason::Initial);
    assert_eq!(segments[0].source(), RideSource::Live);
    assert_eq!(
        segments[0].ended_monotonic_milliseconds(),
        Some(MonotonicMilliseconds::new(11))
    );
    assert_eq!(segments[1].id(), RideMapSegmentId::new(1));
    assert_eq!(segments[1].sequence(), RidePointSequence::new(1));
    assert_eq!(
        segments[1].start_reason(),
        RideSegmentStartReason::BackgroundGap
    );
    assert_eq!(
        segments[1].started_monotonic_milliseconds(),
        MonotonicMilliseconds::new(40_012)
    );
    assert_eq!(
        segments[1].ended_monotonic_milliseconds(),
        Some(MonotonicMilliseconds::new(40_012))
    );
    assert_eq!(
        segments[1].started_wall_clock_milliseconds(),
        WallClockUnixMilliseconds::new(1_700_000_040_012)
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_points_require_a_matching_segment_foreign_key() {
    let _guard = test_guard();
    let connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    crate::storage::create_current_schema(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO rides
             (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
             VALUES (?1, 'live', 'active', 10, 10, 0, 0)",
            ["00000000-0000-0000-0000-000000000001"],
        )
        .unwrap();
    let error = connection
        .execute(
            "INSERT INTO ride_points
             (ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms,
              latitude_e7, longitude_e7, horizontal_accuracy_mm, source)
             VALUES (?1, 0, 0, 0, 11, 1_700_000_000_011, 400000000, -105000000, NULL, 'live')",
            ["00000000-0000-0000-0000-000000000001"],
        )
        .expect_err("a point without its segment must be rejected");
    assert!(matches!(error, rusqlite::Error::SqliteFailure(_, _)));
}
