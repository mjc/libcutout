#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Rust-owned ride recording and geospatial domain primitives.

mod coordinate;
pub use coordinate::*;
mod lifecycle;
pub use lifecycle::*;
mod location;
pub use location::*;
mod summary;
pub use summary::*;
mod storage;
pub use storage::*;

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use cutout_core::{
        MonotonicTimestamp, PevcapCapture, PevcapEncoding, PevcapHeader, PevcapPhoneLocation,
        PevcapRecord, WallClockUnixTimestamp,
    };
    use rusqlite::Connection;

    use super::{
        Coordinate, LatitudeE7, LocationAdmission, LocationSample, LocationSource, LongitudeE7,
        RideDatabase, RideEvent, RideLifecycleState, RideSource, RideSummary, StorageError,
        TransitionError,
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
    fn coordinate_rejects_non_finite_and_out_of_range_values() {
        assert!(Coordinate::from_degrees(40.0, -105.0).is_ok());
        assert!(Coordinate::from_degrees(f64::NAN, -105.0).is_err());
        assert!(Coordinate::from_degrees(91.0, -105.0).is_err());
        assert!(Coordinate::from_degrees(40.0, 181.0).is_err());
    }

    #[test]
    fn coordinate_uses_fixed_point_degree_values() {
        let coordinate = Coordinate::from_degrees(40.123_456_7, -105.765_432_1).unwrap();
        assert_eq!(coordinate.latitude(), LatitudeE7::new(401_234_567));
        assert_eq!(coordinate.longitude(), LongitudeE7::new(-1_057_654_321));
    }

    #[test]
    fn ride_lifecycle_accepts_only_valid_transitions() {
        let state = RideLifecycleState::Draft;
        let state = state.apply(RideEvent::Start).unwrap();
        let state = state.apply(RideEvent::Pause).unwrap();
        let state = state.apply(RideEvent::Resume).unwrap();
        let state = state.apply(RideEvent::Stop).unwrap();
        assert_eq!(state.apply(RideEvent::Save), Ok(RideLifecycleState::Saved));
        assert_eq!(state.apply(RideEvent::Pause), Err(TransitionError::Invalid));
    }

    #[test]
    fn location_admission_deduplicates_repeated_samples() {
        let coordinate = Coordinate::from_degrees(40.0, -105.0).unwrap();
        let sample = LocationSample::new(
            coordinate,
            1_000,
            1_700_000_000_000,
            Some(3_000),
            LocationSource::PevcapImport,
        );
        assert_eq!(sample.admission(None), LocationAdmission::Accepted);
        assert_eq!(
            sample.admission(Some(&sample)),
            LocationAdmission::Duplicate
        );
    }

    #[test]
    fn ride_summary_counts_points_and_distance() {
        let first = LocationSample::new(
            Coordinate::from_degrees(40.0, -105.0).unwrap(),
            1_000,
            1_700_000_000_000,
            None,
            LocationSource::Live,
        );
        let second = LocationSample::new(
            Coordinate::from_degrees(40.0, -104.999).unwrap(),
            2_000,
            1_700_000_001_000,
            None,
            LocationSource::Live,
        );
        let summary = RideSummary::from_samples(&[first, second]);
        assert_eq!(summary.point_count(), 2);
        assert!(summary.distance_millimetres() > 80_000);
        assert!(summary.distance_millimetres() < 100_000);
    }

    #[test]
    fn database_owns_one_service_and_reopens_persisted_rides() {
        let _guard = test_guard();
        let path =
            std::env::temp_dir().join(format!("cutout-ride-maps-{}.sqlite", uuid::Uuid::new_v4()));
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
        let second = LocationSample::new(
            Coordinate::from_degrees(40.001, -105.001).unwrap(),
            1_000,
            1_700_000_000_001,
            None,
            LocationSource::Live,
        );
        assert_eq!(
            first.append_location(ride, second).unwrap(),
            LocationAdmission::Accepted
        );
        let export_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-timestamp-{}.json",
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
            "cutout-ride-maps-mobile-state-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = RideDatabase::open(&path).unwrap();
        database.save_selected_device("ios-local-aero", 42).unwrap();
        assert_eq!(
            database.selected_device().unwrap().as_deref(),
            Some("ios-local-aero")
        );
        let model = crate::VoltageSagModelRecord {
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
    fn database_streams_pevcap_and_deduplicates_artifact_digest() {
        let _guard = test_guard();
        let database_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-pevcap-db-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-pevcap-{}.jsonl",
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
        let first = database
            .import_pevcap(&artifact_path, PevcapEncoding::Jsonl, 1_700_000_000_000)
            .unwrap();
        assert!(!first.duplicate);
        assert_eq!(first.record_count, 1);
        assert_eq!(first.location_count, 1);
        assert_eq!(database.summary(first.ride_id).unwrap().point_count(), 1);
        let second = database
            .import_pevcap(&artifact_path, PevcapEncoding::Jsonl, 1_700_000_000_001)
            .unwrap();
        assert!(second.duplicate);
        assert_eq!(second.ride_id, first.ride_id);
        database.shutdown().unwrap();
        let _ = std::fs::remove_file(database_path);
        let _ = std::fs::remove_file(artifact_path);
    }

    #[test]
    fn malformed_pevcap_import_does_not_publish_an_orphan_ride() {
        let _guard = test_guard();
        let database_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-malformed-pevcap-db-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let artifact_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-malformed-pevcap-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        let backup_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-malformed-pevcap-backup-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&artifact_path, b"not a PEVCAP document\n").unwrap();

        let database = RideDatabase::open(&database_path).unwrap();
        assert!(
            database
                .import_pevcap(&artifact_path, PevcapEncoding::Jsonl, 1_700_000_000_000)
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
    fn legacy_schema_versions_migrate_to_the_current_schema() {
        let _guard = test_guard();
        for version in [1_i64, 2_i64] {
            let path = std::env::temp_dir().join(format!(
                "cutout-ride-maps-legacy-v{version}-{}.sqlite",
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
            assert_eq!(current_version, 3);
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
            "cutout-ride-maps-newer-schema-{}.sqlite",
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
            "cutout-ride-maps-spatial-{}.sqlite",
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
        assert_eq!(
            database
                .trail_segments_in_bounds(39.9, 40.1, -105.1, -104.9)
                .unwrap()
                .len(),
            1
        );
        let point = database.create_map_point("Charge", start).unwrap();
        let points = database
            .map_points_in_bounds(39.9, 40.1, -105.1, -104.9)
            .unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].id, point);
        let backup_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-backup-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        database.backup_to(&backup_path).unwrap();
        assert!(backup_path.is_file());
        let ride = database.create_ride(RideSource::Live, 1).unwrap();
        let export_path = std::env::temp_dir().join(format!(
            "cutout-ride-maps-export-{}.json",
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
}
