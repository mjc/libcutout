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

    use super::{
        Coordinate, LatitudeE7, LocationAdmission, LocationSample, LocationSource, LongitudeE7,
        RideDatabase, RideEvent, RideLifecycleState, RideSource, RideSummary, TransitionError,
    };

    static TEST_LOCK: Mutex<()> = Mutex::new(());

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
        assert!(summary.distance_millimeters() > 80_000);
        assert!(summary.distance_millimeters() < 100_000);
    }

    #[test]
    fn database_owns_one_service_and_reopens_persisted_rides() {
        let _guard = TEST_LOCK.lock().unwrap();
        let path =
            std::env::temp_dir().join(format!("cutout-ride-maps-{}.sqlite", uuid::Uuid::new_v4()));
        let first = RideDatabase::open(&path).unwrap();
        let second = RideDatabase::open(&path).unwrap();
        assert_eq!(first.service_id(), second.service_id());
        assert!(first.capabilities().unwrap().sqlite_version().major() >= 3);

        let ride = first
            .create_ride(RideSource::Live, 1_700_000_000_000)
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
        assert_eq!(
            first.append_location(ride, sample).unwrap(),
            LocationAdmission::Duplicate
        );
        first.transition(ride, RideEvent::Stop).unwrap();
        first.transition(ride, RideEvent::Save).unwrap();
        drop(second);
        first.shutdown().unwrap();

        let reopened = RideDatabase::open(&path).unwrap();
        let summary = reopened.summary(ride).unwrap();
        assert_eq!(summary.point_count(), 1);
        reopened.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn database_persists_migrated_mobile_state() {
        let _guard = TEST_LOCK.lock().unwrap();
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
        let _guard = TEST_LOCK.lock().unwrap();
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
    fn database_indexes_trails_and_map_points_with_rtree() {
        let _guard = TEST_LOCK.lock().unwrap();
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
