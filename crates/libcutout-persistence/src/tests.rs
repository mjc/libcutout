use std::sync::Mutex;

use cutout_core::{
    MonotonicTimestamp, PevcapCapture, PevcapEncoding, PevcapHeader, PevcapPhoneLocation,
    PevcapRecord, WallClockUnixTimestamp,
};
use cutout_ride_maps::{Coordinate, LocationAdmission, LocationSample, LocationSource, RideEvent};
use rusqlite::Connection;

use cutout_ride_maps::RideLifecycleState;

use super::{
    GeoBounds, PevcapImportOutcome, QueryLimit, RideDatabase, RideSource, StorageError,
    VoltageSagModelRecord,
};

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        1_000,
        1_700_000_000_001,
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
        first.append_location(ride, sample).unwrap(),
        LocationAdmission::Duplicate
    );
    first.transition(ride, RideEvent::Stop).unwrap();
    first.transition(ride, RideEvent::Save).unwrap();
    first.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    let summary = reopened.summary(ride).unwrap();
    assert_eq!(summary.point_count(), 2);
    reopened.shutdown().unwrap();
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
                    horizontal_accuracy_meters: 3.0,
                    vertical_accuracy_meters: 4.0,
                    speed_meters_per_second: 0.0,
                    speed_accuracy_meters_per_second: 1.0,
                    course_degrees: 0.0,
                    course_accuracy_degrees: 1.0,
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
    assert_eq!(database.summary(ride_id).unwrap().point_count(), 1);
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
        assert_eq!(current_version, 5);
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
                    "SELECT platform_identifier FROM selected_device WHERE id = 1",
                    [],
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
