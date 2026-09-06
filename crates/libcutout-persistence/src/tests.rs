use std::sync::Mutex;
use std::time::{Duration, Instant};

use cutout_core::{
    MonotonicTimestamp, MusicEventTiming, MusicHistoryPolicy, MusicProvider, MusicRideEvent,
    MusicRideEventKind, PevcapCapture, PevcapEncoding, PevcapHeader, PevcapLocationSample,
    PevcapPhoneLocation, PevcapRecord, WallClockUnixTimestamp,
};
use cutout_ride_maps::{
    Coordinate, LocationAdmission, LocationSample, LocationSource, RideEvent, RouteDisplayBudget,
    RoutePrivacyGridE7, RoutePrivacyPolicy, RouteTelemetryState, RouteViewport, VehicleIdentity,
    WallClockUnixMilliseconds,
};
use rusqlite::Connection;

use cutout_ride_maps::{RideLifecycleState, RideMapSegmentId, RideSegmentStartReason};

use super::{
    GeoBounds, HistoryContextBudget, PevcapImportOutcome, PevcapImportPreview, PevcapImportWarning,
    QueryLimit, RideDatabase, RideHistoryQuery, RideId, RideRecord, RideSource,
    RouteProjectionCancellation, StorageError, VoltageSagModelRecord,
    normalize_device_display_name,
};

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn music_event() -> MusicRideEvent {
    MusicRideEvent::new(
        MusicProvider::AppleMusic,
        Some("opaque-track".to_owned()),
        None,
        None,
        MusicRideEventKind::ItemChanged,
        MusicEventTiming {
            monotonic_at: MonotonicTimestamp::new(110),
            wall_clock_at: WallClockUnixTimestamp::new(1_700_000_000_110),
            clock_uncertainty_milliseconds: 5,
        },
    )
    .expect("music event is valid")
}

fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn pevcap_header() -> PevcapHeader {
    PevcapHeader::new(
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
    .unwrap()
}

fn pevcap_phone_location(
    monotonic_ms: u64,
    latitude_degrees: f64,
    horizontal_accuracy_meters: Option<f64>,
) -> PevcapPhoneLocation {
    PevcapPhoneLocation {
        wall_clock_unix_ms: 1_700_000_000_000 + monotonic_ms,
        latitude_degrees,
        longitude_degrees: -105.0,
        altitude_meters: 1_600.0,
        horizontal_accuracy_meters,
        vertical_accuracy_meters: None,
        speed_meters_per_second: None,
        speed_accuracy_meters_per_second: None,
        course_degrees: None,
        course_accuracy_degrees: None,
    }
}

fn pevcap_location_record(
    monotonic_ms: u64,
    latitude_degrees: f64,
    horizontal_accuracy_meters: Option<f64>,
) -> PevcapRecord {
    PevcapRecord::link_up(MonotonicTimestamp::new(monotonic_ms), None).with_phone_location(
        pevcap_phone_location(monotonic_ms, latitude_degrees, horizontal_accuracy_meters),
    )
}

fn pevcap_preview_variant(
    preview: &PevcapImportPreview,
    artifact_size: u64,
    record_count: u64,
    location_count: u64,
    duration_milliseconds: u64,
    outcome: PevcapImportOutcome,
    warnings: Vec<PevcapImportWarning>,
) -> PevcapImportPreview {
    PevcapImportPreview::from_parts(
        preview.source_path().to_owned(),
        preview.encoding(),
        preview.artifact_digest().to_owned(),
        artifact_size,
        record_count,
        location_count,
        duration_milliseconds,
        outcome,
        warnings,
    )
}

fn assert_duplicate_preview_variants_rejected(
    database: &RideDatabase,
    preview: &PevcapImportPreview,
) {
    let variants = [
        pevcap_preview_variant(
            preview,
            preview.artifact_size(),
            preview.record_count() + 1,
            preview.location_count(),
            preview.duration_milliseconds(),
            preview.outcome(),
            preview.warnings().to_vec(),
        ),
        pevcap_preview_variant(
            preview,
            preview.artifact_size(),
            preview.record_count(),
            preview.location_count() + 1,
            preview.duration_milliseconds(),
            preview.outcome(),
            preview.warnings().to_vec(),
        ),
        pevcap_preview_variant(
            preview,
            preview.artifact_size(),
            preview.record_count(),
            preview.location_count(),
            preview.duration_milliseconds() + 1,
            preview.outcome(),
            preview.warnings().to_vec(),
        ),
        pevcap_preview_variant(
            preview,
            preview.artifact_size(),
            preview.record_count(),
            preview.location_count(),
            preview.duration_milliseconds(),
            PevcapImportOutcome::CaptureOnly,
            vec![PevcapImportWarning::NoRouteLocations],
        ),
        pevcap_preview_variant(
            preview,
            preview.artifact_size(),
            preview.record_count(),
            preview.location_count(),
            preview.duration_milliseconds(),
            preview.outcome(),
            vec![PevcapImportWarning::NoRouteLocations],
        ),
        pevcap_preview_variant(
            preview,
            preview.artifact_size() + 1,
            preview.record_count(),
            preview.location_count(),
            preview.duration_milliseconds(),
            preview.outcome(),
            preview.warnings().to_vec(),
        ),
    ];
    for preview in variants {
        assert!(matches!(
            database.confirm_pevcap_import(&preview, 1_700_000_000_001),
            Err(StorageError::PevcapPreviewChanged)
        ));
    }
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
        2_500,
        1_700_000_002_500,
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
                 started_monotonic_ms, started_wall_clock_ms)
             VALUES (?1, 0, 0, 'initial', 'live', 0, 1_700_000_000_000)",
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

    let (created_at_ms, monotonic_created_at_ms): (u64, Option<u64>) = connection
        .query_row(
            "SELECT created_at_ms, monotonic_created_at_ms FROM rides WHERE id = ?1",
            ["00000000-0000-0000-0000-000000000001"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(created_at_ms, 1_700_000_001_000);
    assert_eq!(monotonic_created_at_ms, Some(1_000));
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
        database.append_location_with_segment_and_telemetry(
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
fn queued_location_returns_durable_admission() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-queued-location-admission-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database
        .create_ride(RideSource::Live, 1_700_000_000_000)
        .unwrap();
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
            .append_location_with_segment_and_telemetry(
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
            .append_location_with_segment_and_telemetry(
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
fn durable_location_admission_matches_route_policy() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-policy-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database
        .create_ride(RideSource::Live, 1_700_000_000_000)
        .unwrap();
    database.transition(ride, RideEvent::Start).unwrap();

    let first = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_000,
        1_700_000_000_000,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        database.append_location(ride, first).unwrap(),
        LocationAdmission::Accepted
    );

    let low_accuracy = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        Some(100_001),
        LocationSource::Live,
    );
    assert_eq!(
        database.append_location(ride, low_accuracy).unwrap(),
        LocationAdmission::AccuracyTooLow
    );

    let unrealistic_jump = LocationSample::new(
        Coordinate::from_degrees(40.001, -105.0).unwrap(),
        1_002,
        1_700_000_000_002,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        database.append_location(ride, unrealistic_jump).unwrap(),
        LocationAdmission::UnrealisticJump
    );

    assert_eq!(database.summary(ride).unwrap().point_count().as_u64(), 1);
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn queued_location_write_returns_before_worker_completion_and_can_be_polled() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-queued-location-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database
        .create_ride(RideSource::Live, 1_700_000_000_000)
        .unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_001,
        1_700_000_000_001,
        None,
        LocationSource::Live,
    );

    let started = Instant::now();
    let mut pending = database
        .queue_location(
            ride,
            sample,
            RideMapSegmentId::new(0),
            RideSegmentStartReason::Initial,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();
    assert!(started.elapsed() < Duration::from_millis(100));
    let result = loop {
        if let Some(result) = pending.try_result() {
            break result;
        }
        std::thread::yield_now();
    };
    let result = result.unwrap();
    assert_eq!(result.admission(), LocationAdmission::Accepted);
    assert_eq!(result.sequence(), Some(0));
    assert!(pending.try_result().is_none());

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn queued_location_write_wait_consumes_and_does_not_lose_result() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-queued-location-consumption-{}.sqlite",
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
    let mut pending = database
        .queue_location(
            ride,
            sample,
            RideMapSegmentId::new(0),
            RideSegmentStartReason::Initial,
            RouteTelemetryState::GpsOnly,
        )
        .unwrap();

    let result = pending
        .wait_result_until(Instant::now() + Duration::from_secs(1))
        .expect("worker should return a result")
        .expect("deadline should include the result")
        .expect("location write should be accepted");
    assert_eq!(result.admission(), LocationAdmission::Accepted);
    assert!(pending.try_result().is_none());
    assert!(matches!(
        pending.wait_result_until(Instant::now()),
        Ok(None)
    ));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn queued_location_write_can_bound_wait_for_a_delayed_worker() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-queued-location-deadline-{}.sqlite",
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
    let mut pending = database
        .queue_location(
            ride,
            sample,
            RideMapSegmentId::new(0),
            RideSegmentStartReason::Initial,
            RouteTelemetryState::GpsOnly,
        )
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
        Ok(Ok(result)) if result.admission() == LocationAdmission::Accepted
    ));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn queued_location_write_can_bound_wait_for_a_worker_gate() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-queued-location-worker-gate-{}.sqlite",
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
    let mut pending = database
        .enqueue_location_with_worker_gate_for_test(
            ride,
            sample,
            RideMapSegmentId::new(0),
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
        Ok(Ok(result)) if result.admission() == LocationAdmission::Accepted
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
    let mut pending = database
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
fn shutdown_prevents_stale_handles_from_restarting_the_worker() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-worker-shutdown-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let stale_handle = database.clone();

    database.begin_shutdown_for_test().unwrap();
    database.stop_worker_for_test().unwrap();
    assert!(matches!(
        stale_handle.capabilities(),
        Err(StorageError::WorkerStopped)
    ));

    drop(stale_handle);
    drop(database);
    RideDatabase::open(&path).unwrap().shutdown().unwrap();
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
    let ride = database
        .create_ride(RideSource::Live, 1_700_000_000_000)
        .unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let first = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        3_000,
        1_700_000_003_000,
        None,
        LocationSource::Live,
    );
    let second = LocationSample::new(
        Coordinate::from_degrees(40.0001, -105.0).unwrap(),
        5_000,
        1_700_000_005_000,
        None,
        LocationSource::Live,
    );
    database.append_location(ride, first).unwrap();
    database.append_location(ride, second).unwrap();

    let page = database
        .list_rides(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(page.rides()[0].duration_milliseconds(), 2_000);
    assert_eq!(
        database
            .find_ride(ride)
            .unwrap()
            .unwrap()
            .duration_milliseconds(),
        2_000
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn monotonic_ride_start_is_persisted_separately_from_wall_clock_ordering() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-monotonic-ride-start-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database
        .create_ride_with_monotonic_start(RideSource::Live, 1_700_000_000_000, Some(1_000))
        .unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    database
        .append_location(
            ride,
            LocationSample::new(
                Coordinate::from_degrees(40.0, -105.0).unwrap(),
                2_000,
                1_700_000_001_000,
                None,
                LocationSource::Live,
            ),
        )
        .unwrap();
    database.shutdown().unwrap();

    let database = RideDatabase::open(&path).unwrap();
    let record = database.find_ride(ride).unwrap().unwrap();
    assert_eq!(record.created_at_milliseconds(), 1_700_000_000_000);
    assert_eq!(record.monotonic_created_at_milliseconds(), Some(1_000));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn lifecycle_transition_uses_latest_durable_location_timestamp() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-lifecycle-watermark-{}.sqlite",
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
        .append_location(
            ride,
            LocationSample::new(
                Coordinate::from_degrees(40.0, -105.0).unwrap(),
                5_000,
                1_700_000_004_000,
                None,
                LocationSource::Live,
            ),
        )
        .unwrap();
    database
        .transition_at(ride, RideEvent::Pause, 3_000)
        .unwrap();

    let record = database.find_ride(ride).unwrap().unwrap();
    assert_eq!(record.paused_at_milliseconds(), Some(5_000));
    assert_eq!(record.duration_milliseconds(), 4_000);

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}
#[test]
fn lifecycle_timing_is_persisted_across_pause_and_reopen() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-lifecycle-timing-{}.sqlite",
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
        .transition_at(ride, RideEvent::Pause, 3_000)
        .unwrap();
    database
        .transition_at(ride, RideEvent::Resume, 5_000)
        .unwrap();
    database
        .transition_at(ride, RideEvent::Stop, 7_000)
        .unwrap();
    let record = database.find_ride(ride).unwrap().unwrap();
    assert_eq!(record.paused_duration_milliseconds(), 2_000);
    assert_eq!(record.completed_duration_milliseconds(), 4_000);
    database.shutdown().unwrap();

    let database = RideDatabase::open(&path).unwrap();
    let restored = database.find_ride(ride).unwrap().unwrap();
    assert_eq!(restored.paused_duration_milliseconds(), 2_000);
    assert_eq!(restored.completed_duration_milliseconds(), 4_000);
    assert_eq!(restored.duration_milliseconds(), 4_000);
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
            .append_location_with_segment_and_telemetry(
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
            .append_location_with_segment_and_telemetry(
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
    database
        .save_selected_device("  ios-local-aero  ", 42)
        .unwrap();
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
fn remember_selected_device_normalizes_identity_and_name_together() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-device-selection-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();

    database
        .remember_selected_device("  corebluetooth-a  ", Some("  NF2557  "), 42)
        .unwrap();

    assert_eq!(
        database.selected_device().unwrap().as_deref(),
        Some("corebluetooth-a")
    );
    assert_eq!(
        database.device_name("corebluetooth-a").unwrap().as_deref(),
        Some("NF2557")
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn ride_map_metadata_normalizes_and_bounds_text() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-map-metadata-validation-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 1).unwrap();

    database
        .update_ride_map_metadata(
            ride,
            Some("  candidate  "),
            Some(" associated "),
            None,
            None,
        )
        .unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let ride_record = database.find_ride(ride).unwrap().expect("recording ride");
    assert_eq!(ride_record.candidate_vehicle(), Some("candidate"));
    assert_eq!(ride_record.associated_vehicle(), Some("associated"));

    let too_long = "x".repeat(513);
    let error = database
        .update_ride_map_metadata(ride, Some(&too_long), None, None, None)
        .unwrap_err();
    assert!(matches!(
        error,
        StorageError::InvalidStoredValue {
            field: "candidate vehicle",
            ..
        }
    ));
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn device_names_normalize_lookup_and_bound_text() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-device-name-validation-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();

    database
        .save_device_name("  device-a  ", "  NF2557  ", 1)
        .unwrap();
    assert_eq!(
        database.device_name(" device-a ").unwrap().as_deref(),
        Some("NF2557")
    );

    let too_long = "x".repeat(513);
    let error = database
        .save_device_name("device-a", &too_long, 2)
        .unwrap_err();
    assert!(matches!(
        error,
        StorageError::InvalidStoredValue {
            field: "display name",
            ..
        }
    ));
    let error = database
        .save_device_name(&too_long, "NF2557", 3)
        .unwrap_err();
    assert!(matches!(
        error,
        StorageError::InvalidStoredValue {
            field: "platform identifier",
            ..
        }
    ));
    assert_eq!(
        normalize_device_display_name("device-a", "  device-a  ").unwrap(),
        None
    );
    assert_eq!(
        normalize_device_display_name("device-a", "  NF2557  ").unwrap(),
        Some("NF2557".to_owned())
    );
    assert_eq!(
        database
            .migrate_device_name("device-a", "  NF2557  ", 4)
            .unwrap(),
        Some("NF2557".to_owned())
    );
    assert_eq!(
        database
            .migrate_device_name("device-a", " device-a ", 5)
            .unwrap(),
        None
    );
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one end-to-end test keeps managed-artifact integrity failures in a single audit trail"
)]
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
    let capture = PevcapCapture::new(
        pevcap_header(),
        vec![pevcap_location_record(1, 40.0, Some(3.0))],
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
    std::fs::remove_file(&first.managed_artifact_path).unwrap();
    assert!(matches!(
        database.confirm_pevcap_import(&preview, 1_700_000_000_002),
        Err(StorageError::PevcapPreviewChanged)
    ));
    std::fs::write(&first.managed_artifact_path, b"tampered").unwrap();
    assert!(matches!(
        database.confirm_pevcap_import(&preview, 1_700_000_000_003),
        Err(StorageError::PevcapPreviewChanged)
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let external_file = artifact_path.with_extension("external");
        std::fs::remove_file(&first.managed_artifact_path).unwrap();
        std::fs::write(&external_file, b"keep").unwrap();
        symlink(&external_file, &first.managed_artifact_path).unwrap();
        assert!(matches!(
            database.confirm_pevcap_import(&preview, 1_700_000_000_004),
            Err(StorageError::PevcapPreviewChanged)
        ));
        assert_eq!(std::fs::read(&external_file).unwrap(), b"keep");
        std::fs::remove_file(&first.managed_artifact_path).unwrap();

        let managed_directory = first.managed_artifact_path.parent().unwrap();
        let external_directory = artifact_path.with_extension("external-directory");
        std::fs::remove_dir(managed_directory).unwrap();
        std::fs::create_dir(&external_directory).unwrap();
        let sentinel = external_directory.join("sentinel");
        std::fs::write(&sentinel, b"keep").unwrap();
        symlink(&external_directory, managed_directory).unwrap();
        assert!(matches!(
            database.confirm_pevcap_import(&preview, 1_700_000_000_005),
            Err(StorageError::PevcapPreviewChanged)
        ));
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
        std::fs::remove_file(managed_directory).unwrap();
        std::fs::remove_file(sentinel).unwrap();
        std::fs::remove_dir(external_directory).unwrap();
        std::fs::remove_file(external_file).unwrap();
    }
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&first.managed_artifact_path);
    let _ = std::fs::remove_dir(first.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_import_keeps_a_valid_location_without_horizontal_accuracy() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-no-accuracy-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-no-accuracy-{}.jsonl",
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
                    horizontal_accuracy_meters: None,
                    vertical_accuracy_meters: None,
                    speed_meters_per_second: None,
                    speed_accuracy_meters_per_second: None,
                    course_degrees: None,
                    course_accuracy_degrees: None,
                },
            ),
        ],
    );
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.location_count(), 1);
    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    let ride_id = receipt.ride_id.unwrap();
    let connection = Connection::open(&database_path).unwrap();
    let accuracy: Option<i64> = connection
        .query_row(
            "SELECT horizontal_accuracy_mm FROM ride_points WHERE ride_id = ?1",
            [ride_id.uuid().to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(accuracy, None);
    database.shutdown().unwrap();
    let managed_artifact_path = receipt.managed_artifact_path;
    let _ = std::fs::remove_file(&managed_artifact_path);
    let _ = std::fs::remove_dir(managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_import_deduplicates_repeated_attached_location_context() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-context-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-context-{}.jsonl",
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
    let location = PevcapPhoneLocation {
        wall_clock_unix_ms: 1_700_000_000_000,
        latitude_degrees: 40.0,
        longitude_degrees: -105.0,
        altitude_meters: 1_600.0,
        horizontal_accuracy_meters: Some(3.0),
        vertical_accuracy_meters: None,
        speed_meters_per_second: None,
        speed_accuracy_meters_per_second: None,
        course_degrees: None,
        course_accuracy_degrees: None,
    };
    let capture = PevcapCapture::new(
        header,
        vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(1), None).with_phone_location(location),
            PevcapRecord::link_up(MonotonicTimestamp::new(2), None).with_phone_location(location),
        ],
    );
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.location_count(), 1);
    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    assert_eq!(
        database
            .summary(receipt.ride_id.unwrap())
            .unwrap()
            .point_count()
            .as_u64(),
        1
    );
    database.shutdown().unwrap();
    let managed_artifact_path = receipt.managed_artifact_path;
    let _ = std::fs::remove_file(&managed_artifact_path);
    let _ = std::fs::remove_dir(managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_jsonl_and_binary_imports_preserve_merged_route_order() {
    let _guard = test_guard();
    let independent = PevcapLocationSample::new(
        MonotonicTimestamp::new(2_000),
        pevcap_phone_location(2_000, 40.00001, Some(3.0)),
        None,
        None,
    )
    .unwrap();
    let capture = PevcapCapture::new_with_locations(
        pevcap_header(),
        vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(1_000), None)
                .with_phone_location(pevcap_phone_location(1_000, 40.0, Some(3.0))),
            PevcapRecord::link_up(MonotonicTimestamp::new(3_000), None)
                .with_phone_location(pevcap_phone_location(3_000, 40.0002, Some(3.0))),
        ],
        vec![independent],
    );
    let mut imported_routes = Vec::new();

    for encoding in [PevcapEncoding::Jsonl, PevcapEncoding::Binary] {
        let database_path = std::env::temp_dir().join(format!(
            "libcutout-persistence-pevcap-order-db-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "libcutout-persistence-pevcap-order-{}.artifact",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&artifact_path, capture.encode(encoding).unwrap()).unwrap();
        let database = RideDatabase::open(&database_path).unwrap();
        let preview = database.preflight_pevcap(&artifact_path, encoding).unwrap();
        assert_eq!(preview.location_count(), 3);
        assert_eq!(preview.duration_milliseconds(), 2_000);
        let receipt = database
            .confirm_pevcap_import(&preview, 1_700_000_000_000)
            .unwrap();
        let ride_id = receipt.ride_id.unwrap();
        let ride = database.find_ride(ride_id).unwrap().unwrap();
        let route = database
            .route_points(ride_id, None, QueryLimit::new(10).unwrap())
            .unwrap()
            .points()
            .iter()
            .map(|point| {
                (
                    point.sample().monotonic_milliseconds().as_u64(),
                    point.sample().coordinate().latitude().as_i32(),
                    point.sample().coordinate().longitude().as_i32(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(route.len(), 3);
        assert_eq!(
            route.iter().map(|point| point.0).collect::<Vec<_>>(),
            [1_000, 2_000, 3_000]
        );
        assert_eq!(ride.duration_milliseconds(), 2_000);
        assert!(ride.average_speed_millimetres_per_second().is_some());
        imported_routes.push((
            route,
            ride.summary(),
            ride.duration_milliseconds(),
            ride.average_speed_millimetres_per_second(),
            ride.segment_count(),
        ));
        database.shutdown().unwrap();
        let managed_artifact_path = receipt.managed_artifact_path;
        let _ = std::fs::remove_file(&managed_artifact_path);
        let _ = std::fs::remove_dir(managed_artifact_path.parent().unwrap());
        let _ = std::fs::remove_file(database_path);
        let _ = std::fs::remove_file(artifact_path);
    }

    assert_eq!(imported_routes[0], imported_routes[1]);
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
fn location_only_pevcap_preflight_applies_duration_limit() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-location-duration-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-location-duration-{}.jsonl",
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
    let location = |monotonic_ms, wall_clock_unix_ms| {
        PevcapLocationSample::new(
            MonotonicTimestamp::new(monotonic_ms),
            PevcapPhoneLocation {
                wall_clock_unix_ms,
                latitude_degrees: 40.0,
                longitude_degrees: -105.0,
                altitude_meters: 1_600.0,
                horizontal_accuracy_meters: Some(3.0),
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
    };
    let capture = PevcapCapture::new_with_locations(
        header,
        vec![],
        vec![
            location(0, 1_700_000_000_000),
            location(24 * 60 * 60 * 1_000 + 1, 1_700_086_400_001),
        ],
    );
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    assert!(matches!(
        database.preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl),
        Err(StorageError::PevcapLimitExceeded {
            resource: "duration milliseconds",
            ..
        })
    ));
    database.shutdown().unwrap();
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
fn pevcap_confirmation_reconciles_a_committed_finish_response_loss() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-finish-response-loss-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-finish-response-loss-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let capture = PevcapCapture::new(
        PevcapHeader::new(
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
        .unwrap(),
        vec![
            PevcapRecord::link_up(MonotonicTimestamp::new(1_000), None).with_phone_location(
                PevcapPhoneLocation {
                    wall_clock_unix_ms: 1_700_000_000_000,
                    latitude_degrees: 40.0,
                    longitude_degrees: -105.0,
                    altitude_meters: 1_600.0,
                    horizontal_accuracy_meters: Some(3.0),
                    vertical_accuracy_meters: None,
                    speed_meters_per_second: None,
                    speed_accuracy_meters_per_second: None,
                    course_degrees: None,
                    course_accuracy_degrees: None,
                },
            ),
        ],
    );
    let source_bytes = capture.to_jsonl().unwrap().into_bytes();
    std::fs::write(&artifact_path, &source_bytes).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    RideDatabase::fail_next_pevcap_finish_response_for_test();

    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    assert!(!receipt.duplicate);
    assert_eq!(receipt.location_count, 1);
    assert_eq!(
        std::fs::read(&receipt.managed_artifact_path).unwrap(),
        source_bytes
    );
    let duplicate = database
        .confirm_pevcap_import(&preview, 1_700_000_000_001)
        .unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(
        duplicate.managed_artifact_path,
        receipt.managed_artifact_path
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&receipt.managed_artifact_path);
    let _ = std::fs::remove_dir(receipt.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn duplicate_pevcap_confirmation_rejects_tampered_reviewed_facts() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-duplicate-tampered-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-duplicate-tampered-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    let capture = PevcapCapture::new(
        pevcap_header(),
        vec![pevcap_location_record(1_000, 40.0, Some(3.0))],
    );
    std::fs::write(&artifact_path, capture.to_jsonl().unwrap()).unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&artifact_path, PevcapEncoding::Jsonl)
        .unwrap();
    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_000)
        .unwrap();
    assert_duplicate_preview_variants_rejected(&database, &preview);
    assert_eq!(
        database
            .confirm_pevcap_import(&preview, 1_700_000_000_002)
            .unwrap()
            .ride_id,
        receipt.ride_id
    );

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(&receipt.managed_artifact_path);
    let _ = std::fs::remove_dir(receipt.managed_artifact_path.parent().unwrap());
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_file(artifact_path);
}

#[test]
fn pevcap_confirmation_cleans_managed_artifact_when_begin_fails() {
    let _guard = test_guard();
    let database_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-begin-failure-db-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let artifact_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-pevcap-begin-failure-{}.jsonl",
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
    assert!(database.confirm_pevcap_import(&preview, u64::MAX).is_err());
    let mut managed_directory = database_path.as_os_str().to_owned();
    managed_directory.push(".pevcap-imports");
    let managed_directory = std::path::PathBuf::from(managed_directory);
    let managed_entries = match std::fs::read_dir(&managed_directory) {
        Ok(entries) => entries.count(),
        Err(_) => 0,
    };
    assert_eq!(managed_entries, 0);
    database.shutdown().unwrap();
    let _ = std::fs::remove_dir(managed_directory);
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
    let canonical_database_path = database_path
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join(database_path.file_name().unwrap());
    let mut managed_directory = canonical_database_path.as_os_str().to_owned();
    managed_directory.push(".pevcap-imports");
    let managed_directory = std::path::PathBuf::from(managed_directory);
    std::fs::create_dir_all(&managed_directory).unwrap();
    let managed_path = managed_directory.join("abandoned.jsonl");
    let external_path = std::env::temp_dir().join(format!(
        "libcutout-persistence-unmanaged-pevcap-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&managed_path, b"abandoned").unwrap();
    std::fs::write(&external_path, b"outside managed storage").unwrap();
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
    connection
        .execute(
            "INSERT INTO pevcap_import_work (artifact_digest, artifact_path, ride_id)
             VALUES (?1, ?2, ?3)",
            rusqlite::params!["1".repeat(64), external_path.to_string_lossy(), ride_id],
        )
        .unwrap();
    drop(connection);

    let reopened = RideDatabase::open(&database_path).unwrap();
    assert!(!managed_path.exists());
    assert!(external_path.exists());
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
    let _ = std::fs::remove_file(external_path);
    let _ = std::fs::remove_dir(managed_directory);
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
        assert_eq!(current_version, 16);
        drop(connection);

        let reopened = RideDatabase::open(&path).unwrap();
        reopened.shutdown().unwrap();

        let connection = Connection::open(&path).unwrap();
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
    assert_eq!(version, 16);
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
    assert_eq!(version, 16);
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
            u64::from(sequence + 1) * 1_000,
            1_700_000_000_000 + u64::from(sequence + 1) * 1_000,
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
    assert_eq!(first.points()[0].sequence(), 0);
    assert_eq!(first.points()[1].sequence(), 1);
    let second = database
        .route_points(ride, first.next_cursor(), QueryLimit::new(2).unwrap())
        .unwrap();
    assert_eq!(second.points().len(), 1);
    assert_eq!(second.points()[0].sequence(), 2);
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
            &cancellation,
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
            (sequence + 1) * 1_000,
            1_700_000_000_000 + (sequence + 1) * 1_000,
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
            &projection_cancellation,
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
        5_000,
        1_700_000_005_000,
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
fn durable_history_context_projection_excludes_selected_and_bounds_each_route() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-history-context-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let mut rides = Vec::new();
    for ride_index in 0..3_u64 {
        let ride = database
            .create_ride(RideSource::Live, 1_700_000_000_000 + ride_index * 10_000)
            .unwrap();
        database.transition(ride, RideEvent::Start).unwrap();
        for point_index in 0..4_u64 {
            let sample = LocationSample::new(
                Coordinate::from_degrees(
                    40.0 + f64::from(u32::try_from(ride_index).unwrap()) / 100.0
                        + f64::from(u32::try_from(point_index).unwrap()) / 10_000.0,
                    -105.0,
                )
                .unwrap(),
                (point_index + 1) * 1_000,
                1_700_000_000_000 + ride_index * 10_000 + (point_index + 1) * 1_000,
                None,
                LocationSource::Live,
            );
            assert_eq!(
                database.append_location(ride, sample).unwrap(),
                LocationAdmission::Accepted
            );
        }
        rides.push(ride);
    }

    let projection = database
        .project_history_context(
            RideHistoryQuery::default(),
            Some(rides[1]),
            HistoryContextBudget::new(10, 2, 3, 4).unwrap(),
            None,
            RoutePrivacyPolicy::grid(RoutePrivacyGridE7::new(1_000).unwrap()),
        )
        .unwrap();

    assert_eq!(projection.source_history_route_count(), 3);
    assert_eq!(projection.context_route_count(), 2);
    assert_eq!(projection.routes().len(), 2);
    assert!(projection.routes_omitted_by_budget());
    assert!(!projection.history_page_has_more());
    assert_eq!(projection.total_display_point_count(), 4);
    assert!(projection.routes().iter().all(|route| {
        route.ride_id() != rides[1]
            && route.projection().points().len() <= 3
            && route.projection().points().iter().all(|point| {
                point.privacy_class() == cutout_ride_maps::RoutePrivacyClass::GridRedacted
            })
    }));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn durable_history_context_projection_reports_aggregate_budget_omissions() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-history-context-budget-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    for ride_index in 0..3_u64 {
        let ride = database
            .create_ride(RideSource::Live, 1_700_000_000_000 + ride_index * 10_000)
            .unwrap();
        database.transition(ride, RideEvent::Start).unwrap();
        for point_index in 0..2_u64 {
            let sample = LocationSample::new(
                Coordinate::from_degrees(
                    40.0 + f64::from(u32::try_from(ride_index).unwrap()) / 100.0
                        + f64::from(u32::try_from(point_index).unwrap()) / 10_000.0,
                    -105.0,
                )
                .unwrap(),
                (point_index + 1) * 1_000,
                1_700_000_000_000 + ride_index * 10_000 + (point_index + 1) * 1_000,
                None,
                LocationSource::Live,
            );
            assert_eq!(
                database.append_location(ride, sample).unwrap(),
                LocationAdmission::Accepted
            );
        }
    }

    let projection = database
        .project_history_context(
            RideHistoryQuery::default(),
            None,
            HistoryContextBudget::new(10, 3, 3, 2).unwrap(),
            None,
            RoutePrivacyPolicy::Precise,
        )
        .unwrap();

    assert_eq!(projection.context_route_count(), 3);
    assert_eq!(projection.routes().len(), 1);
    assert_eq!(projection.total_display_point_count(), 2);
    assert!(projection.routes_omitted_by_budget());

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
    assert_eq!(
        vehicle_filtered.rides()[0].associated_vehicle_name(),
        Some("NF2557")
    );
    assert_eq!(
        database
            .list_ride_history_vehicle_options()
            .unwrap()
            .into_iter()
            .map(|option| (
                option.platform_identifier().to_owned(),
                option.display_name().map(str::to_owned)
            ))
            .collect::<Vec<_>>(),
        vec![("device-a".to_owned(), Some("NF2557".to_owned()))]
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
fn filtered_history_escapes_like_wildcards() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-filtered-wildcards-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let wildcard_decoy = database.create_ride(RideSource::Live, 30).unwrap();
    let wildcard_literal = database.create_ride(RideSource::Live, 40).unwrap();
    for ride in [wildcard_decoy, wildcard_literal] {
        database.transition(ride, RideEvent::Start).unwrap();
        database.transition(ride, RideEvent::Stop).unwrap();
        database.transition(ride, RideEvent::Save).unwrap();
    }
    database
        .save_device_name("device-decoy", "name-Xliteral", 41)
        .unwrap();
    database
        .save_device_name("device_%\\literal", "name_%\\literal", 42)
        .unwrap();
    database
        .update_ride_map_metadata(wildcard_decoy, None, Some("device-decoy"), None, None)
        .unwrap();
    database
        .update_ride_map_metadata(
            wildcard_literal,
            None,
            Some("device_%\\literal"),
            None,
            None,
        )
        .unwrap();
    let literal_search = database
        .list_rides_filtered(
            None,
            QueryLimit::new(10).unwrap(),
            RideHistoryQuery::new(None, None, Some("name_%\\literal")),
        )
        .unwrap();
    assert_eq!(
        literal_search
            .rides()
            .iter()
            .map(RideRecord::id)
            .collect::<Vec<_>>(),
        vec![wildcard_literal]
    );
    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
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
            ALTER TABLE rides DROP COLUMN monotonic_last_event_ms;
            ALTER TABLE rides DROP COLUMN paused_at_ms;
            ALTER TABLE rides DROP COLUMN paused_duration_ms;
            ALTER TABLE rides DROP COLUMN completed_duration_ms;
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
    assert_eq!(version, 16);
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
fn route_projection_is_bounded_viewport_aware_and_cancellable() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-route-projection-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 10).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    for sequence in 0_u32..100 {
        let sequence_ms = u64::from(sequence);
        let sample = LocationSample::new(
            Coordinate::from_degrees(40.0 + f64::from(sequence) / 1_000_000.0, -105.0).unwrap(),
            1_000 + sequence_ms * 1_000,
            1_700_000_000_000 + sequence_ms * 1_000,
            None,
            LocationSource::Live,
        );
        assert_eq!(
            database.append_location(ride, sample).unwrap(),
            LocationAdmission::Accepted
        );
    }

    let projection = database
        .project_route_points(
            ride,
            None,
            RouteDisplayBudget::new(8).unwrap(),
            RoutePrivacyPolicy::Precise,
        )
        .unwrap();
    assert_eq!(projection.source_point_count(), 100);
    assert_eq!(projection.points().len(), 8);

    let viewport = RouteViewport::new(
        cutout_ride_maps::LatitudeE7::new(400_000_000),
        cutout_ride_maps::LatitudeE7::new(400_000_050),
        cutout_ride_maps::LongitudeE7::new(-1_050_000_000),
        cutout_ride_maps::LongitudeE7::new(-1_049_999_000),
    )
    .unwrap();
    let visible = database
        .project_route_points(
            ride,
            Some(viewport),
            RouteDisplayBudget::new(8).unwrap(),
            RoutePrivacyPolicy::Precise,
        )
        .unwrap();
    assert!(!visible.points().is_empty());
    assert!(visible.points().iter().all(|point| {
        viewport.minimum_latitude().as_i32() <= point.coordinate().latitude().as_i32()
            && point.coordinate().latitude().as_i32() <= viewport.maximum_latitude().as_i32()
    }));

    let antimeridian = RouteViewport::new(
        cutout_ride_maps::LatitudeE7::new(-900_000_000),
        cutout_ride_maps::LatitudeE7::new(900_000_000),
        cutout_ride_maps::LongitudeE7::new(1_790_000_000),
        cutout_ride_maps::LongitudeE7::new(-1_790_000_000),
    )
    .unwrap();
    assert!(antimeridian.crosses_antimeridian());
    let antimeridian_projection = database
        .project_route_points(
            ride,
            Some(antimeridian),
            RouteDisplayBudget::new(8).unwrap(),
            RoutePrivacyPolicy::Precise,
        )
        .unwrap();
    assert!(antimeridian_projection.points().is_empty());

    let cancellation = RouteProjectionCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        database.project_route_points_cancellable(
            ride,
            None,
            RouteDisplayBudget::new(8).unwrap(),
            RoutePrivacyPolicy::Precise,
            &cancellation,
        ),
        Err(StorageError::Cancelled)
    ));

    database.shutdown().unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn route_projection_ignores_empty_segment_rows() {
    let _guard = test_guard();
    let path = std::env::temp_dir().join(format!(
        "libcutout-persistence-empty-segment-projection-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let ride = database.create_ride(RideSource::Live, 10).unwrap();
    database.transition(ride, RideEvent::Start).unwrap();
    let sample = LocationSample::new(
        Coordinate::from_degrees(40.0, -105.0).unwrap(),
        1_000,
        1_700_000_000_000,
        None,
        LocationSource::Live,
    );
    assert_eq!(
        database.append_location(ride, sample).unwrap(),
        LocationAdmission::Accepted
    );
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO ride_segments
                (ride_id, segment_id, point_count, sequence, start_reason, source,
                 started_monotonic_ms, ended_monotonic_ms,
                 started_wall_clock_ms, ended_wall_clock_ms)
             VALUES (?1, 1, 0, 1, 'background_gap', 'live', 2_000, 2_000,
                     1_700_000_001_000, 1_700_000_001_000)",
            [ride.uuid().to_string()],
        )
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&path).unwrap();
    let projection = database
        .project_route_points(
            ride,
            None,
            RouteDisplayBudget::new(8).unwrap(),
            RoutePrivacyPolicy::Precise,
        )
        .unwrap();
    assert_eq!(projection.source_segment_count(), 1);
    assert_eq!(projection.background_gap_count(), 0);

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
fn fresh_ride_points_enforce_segment_foreign_keys() {
    let _guard = test_guard();
    let connection = Connection::open_in_memory().unwrap();
    crate::storage::create_current_schema(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO rides
                (id, source, state, created_at_ms, updated_at_ms, point_count, distance_mm)
             VALUES ('ride', 'live', 'active', 0, 0, 0, 0)",
            [],
        )
        .unwrap();
    assert!(
        connection
            .execute(
                "INSERT INTO ride_points
                (ride_id, sequence, segment_id, telemetry_state, monotonic_ms, wall_clock_ms,
                 latitude_e7, longitude_e7, horizontal_accuracy_mm, source)
             VALUES ('ride', 0, 99, 0, 0, 0, 0, 0, NULL, 'live')",
                [],
            )
            .is_err()
    );
}

#[test]
fn history_query_preserves_typed_timestamp_and_vehicle_identity() {
    let query = RideHistoryQuery::new(Some(15), Some(" device-a "), None);

    assert_eq!(
        query.created_after_timestamp(),
        Some(WallClockUnixMilliseconds::new(15))
    );
    assert_eq!(
        query.vehicle_identity(),
        Some(&VehicleIdentity::new("device-a").unwrap())
    );
}

#[test]
fn music_history_round_trips_and_can_be_deleted_without_deleting_ride() {
    let _guard = test_guard();
    let database = RideDatabase::open(std::path::Path::new(":memory:")).unwrap();
    let ride = database
        .create_started_live_ride(1_700_000_000_000, 100, None)
        .unwrap();
    let event = music_event();

    database
        .save_music_event(ride, MusicHistoryPolicy::OpaqueItem, 0, event.clone())
        .unwrap();
    assert_eq!(database.music_events(ride).unwrap(), vec![event]);

    database.delete_music_history(ride).unwrap();
    assert!(database.music_events(ride).unwrap().is_empty());
    assert!(database.find_ride(ride).unwrap().is_some());
    database.shutdown().unwrap();
}

#[test]
fn lowering_music_history_policy_redacts_existing_display_metadata() {
    let _guard = test_guard();
    let database = RideDatabase::open(std::path::Path::new(":memory:")).unwrap();
    let ride = database
        .create_started_live_ride(1_700_000_000_000, 100, None)
        .unwrap();
    let event = MusicRideEvent::new(
        MusicProvider::AppleMusic,
        Some("opaque-track".to_owned()),
        Some("Song".to_owned()),
        Some("Artist".to_owned()),
        MusicRideEventKind::ItemChanged,
        MusicEventTiming {
            monotonic_at: MonotonicTimestamp::new(110),
            wall_clock_at: WallClockUnixTimestamp::new(1_700_000_000_110),
            clock_uncertainty_milliseconds: 5,
        },
    )
    .unwrap();

    database
        .save_music_event(ride, MusicHistoryPolicy::HumanReadable, 0, event)
        .unwrap();
    database
        .save_music_history_policy(ride, MusicHistoryPolicy::OpaqueItem)
        .unwrap();

    let redacted = database.music_events(ride).unwrap();
    assert_eq!(redacted.len(), 1);
    assert_eq!(
        redacted[0]
            .item_identifier()
            .map(cutout_core::MusicIdentifier::as_str),
        Some("opaque-track")
    );
    assert_eq!(redacted[0].title(), None);
    assert_eq!(redacted[0].artist(), None);
    database.shutdown().unwrap();
}
