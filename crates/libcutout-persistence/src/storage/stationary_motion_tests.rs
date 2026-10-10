use super::*;

fn fixture() -> (tempfile::TempDir, RideDatabase, RideId) {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("rides.sqlite3")).unwrap();
    let id = database
        .create_started_live_ride(1_700_000_000_000, 1_000, Some("wheel"))
        .unwrap();
    (directory, database, id)
}

fn sample(at: u64, latitude: f64) -> LocationSample {
    LocationSample::new(
        Coordinate::from_degrees(latitude, -105.0).unwrap(),
        at,
        1_700_000_000_000 + at,
        Some(3_000),
        LocationSource::Live,
    )
}

#[test]
fn parked_motion_is_durable_before_any_gps_observation() {
    let (directory, database, id) = fixture();
    let motion = RideMotionObservation {
        parked: true,
        observed_at_milliseconds: 2_000,
        last_stationary_at_milliseconds: Some(2_000),
    };
    database
        .update_ride_map_metadata_with_motion(
            id,
            Some("wheel"),
            Some("wheel"),
            Some(1_000),
            Some(2_000),
            Some(motion),
        )
        .unwrap();
    assert_eq!(
        database
            .find_ride(id)
            .unwrap()
            .unwrap()
            .motion_observation(),
        Some(motion)
    );
    database.shutdown().unwrap();
    let reopened = RideDatabase::open(&directory.path().join("rides.sqlite3")).unwrap();
    assert_eq!(
        reopened
            .find_ride(id)
            .unwrap()
            .unwrap()
            .motion_observation(),
        Some(motion)
    );
    assert_eq!(
        reopened
            .find_ride(id)
            .unwrap()
            .unwrap()
            .summary()
            .point_count()
            .as_u64(),
        0
    );
}

#[test]
fn stationary_checkpoint_replaces_geometry_with_exact_durable_evidence() {
    let (directory, database, id) = fixture();
    let observation = sample(3_000, 40.0001);
    database
        .update_ride_map_metadata_with_motion(
            id,
            Some("wheel"),
            Some("wheel"),
            Some(1_000),
            Some(2_000),
            Some(RideMotionObservation {
                parked: true,
                observed_at_milliseconds: 2_000,
                last_stationary_at_milliseconds: Some(2_000),
            }),
        )
        .unwrap();
    assert_eq!(
        database
            .checkpoint_stationary_location_observation(RideStationaryLocationCheckpoint {
                ride_id: id,
                expected_point_count: 0,
                observation
            })
            .unwrap(),
        RideLocationObservationCheckpointOutcome::Applied
    );
    let ride = database.find_ride(id).unwrap().unwrap();
    assert_eq!(ride.stationary_location_observation(), Some(observation));
    assert_eq!(ride.summary().point_count().as_u64(), 0);
    assert_eq!(ride.summary().distance_millimetres(), 0);
    database.shutdown().unwrap();
    let reopened = RideDatabase::open(&directory.path().join("rides.sqlite3")).unwrap();
    assert_eq!(
        reopened
            .find_ride(id)
            .unwrap()
            .unwrap()
            .stationary_location_observation(),
        Some(observation)
    );
}

fn parked_connection() -> (Connection, RideId) {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let id = create_started_live_ride(
        &mut connection,
        1_700_000_000_000,
        1_000,
        Some("wheel"),
        None,
    )
    .unwrap();
    update_ride_map_metadata_with_motion(
        &connection,
        id,
        Some("wheel"),
        Some("wheel"),
        Some(1_000),
        Some(2_000),
        Some(RideMotionObservation {
            parked: true,
            observed_at_milliseconds: 2_000,
            last_stationary_at_milliseconds: Some(2_000),
        }),
    )
    .unwrap();
    (connection, id)
}

fn stationary_route_connection() -> (Connection, RideId) {
    let (mut connection, id) = parked_connection();
    assert_eq!(
        append_location(
            &mut connection,
            id,
            sample(1_000, 40.0),
            RideMapSegmentId::new(0),
            RouteTelemetryState::GpsOnly,
        )
        .unwrap(),
        LocationAdmission::Accepted
    );
    assert_eq!(
        checkpoint_stationary_location_observation(
            &connection,
            RideStationaryLocationCheckpoint {
                ride_id: id,
                expected_point_count: 1,
                observation: sample(3_000, 40.0001),
            },
        )
        .unwrap(),
        RideLocationObservationCheckpointOutcome::Applied
    );
    (connection, id)
}

fn append_stationary_exit(
    connection: &mut Connection,
    id: RideId,
    observation: LocationSample,
) -> LocationWriteResult {
    append_location_with_result(
        connection,
        id,
        observation,
        RideMapSegmentId::new(1),
        RouteTelemetryState::AssociatedFresh,
        SegmentStartReasonInput::Recorded(RideSegmentStartReason::Stationary),
    )
    .unwrap()
}

#[test]
fn stationary_worker_rejects_material_callback_before_latest_provider_observation() {
    let (mut connection, id) = stationary_route_connection();
    let changes = connection.total_changes();
    let rejected = append_stationary_exit(&mut connection, id, sample(2_500, 40.00008));
    assert_eq!(rejected.admission(), LocationAdmission::OutOfOrder);
    assert_eq!(rejected.sequence(), None);
    assert_eq!(connection.total_changes(), changes);
    assert_eq!(
        find_ride(&connection, id)
            .unwrap()
            .unwrap()
            .summary()
            .point_count()
            .as_u64(),
        1
    );
}

#[test]
fn stationary_worker_checks_jump_against_latest_provider_observation() {
    let (mut connection, id) = stationary_route_connection();
    let changes = connection.total_changes();
    let rejected = append_stationary_exit(&mut connection, id, sample(3_001, 40.00011));
    assert_eq!(rejected.admission(), LocationAdmission::UnrealisticJump);
    assert_eq!(rejected.sequence(), None);
    assert_eq!(connection.total_changes(), changes);
    assert_eq!(
        find_ride(&connection, id)
            .unwrap()
            .unwrap()
            .summary()
            .point_count()
            .as_u64(),
        1
    );
}

#[test]
fn stationary_worker_preserves_route_sequence_and_zero_bridge_on_valid_exit() {
    let (mut connection, id) = stationary_route_connection();
    let first = sample(4_000, 40.00011);
    let second = sample(5_000, 40.00012);
    let accepted = append_stationary_exit(&mut connection, id, first);
    assert_eq!(accepted.admission(), LocationAdmission::Accepted);
    assert_eq!(accepted.sequence(), Some(1));
    let ride = find_ride(&connection, id).unwrap().unwrap();
    assert_eq!(ride.summary().distance_millimetres(), 0);
    assert_eq!(ride.stationary_location_observation(), None);
    let accepted = append_stationary_exit(&mut connection, id, second);
    assert_eq!(accepted.admission(), LocationAdmission::Accepted);
    assert_eq!(accepted.sequence(), Some(2));
    let ride = find_ride(&connection, id).unwrap().unwrap();
    assert_eq!(ride.summary().point_count().as_u64(), 3);
    assert_eq!(
        ride.summary().distance_millimetres(),
        cutout_ride_maps::distance_between(first, second).as_u64()
    );
    let changes = connection.total_changes();
    assert_eq!(
        append_stationary_exit(&mut connection, id, first).admission(),
        LocationAdmission::OutOfOrder
    );
    assert_eq!(connection.total_changes(), changes);
}

#[test]
fn stationary_checkpoint_is_one_update_and_rejects_stale_geometry_or_clocks() {
    let (connection, id) = parked_connection();
    let observation = sample(3_000, 40.0);
    let checkpoint = RideStationaryLocationCheckpoint {
        ride_id: id,
        expected_point_count: 0,
        observation,
    };
    let changes = connection.total_changes();
    assert_eq!(
        checkpoint_stationary_location_observation(&connection, checkpoint).unwrap(),
        RideLocationObservationCheckpointOutcome::Applied
    );
    assert_eq!(connection.total_changes(), changes + 1);
    assert_eq!(
        checkpoint_stationary_location_observation(&connection, checkpoint).unwrap(),
        RideLocationObservationCheckpointOutcome::AlreadyObserved
    );
    assert_eq!(connection.total_changes(), changes + 1);
    for rejected in [
        RideStationaryLocationCheckpoint {
            expected_point_count: 1,
            ..checkpoint
        },
        RideStationaryLocationCheckpoint {
            observation: sample(2_500, 40.0),
            ..checkpoint
        },
        RideStationaryLocationCheckpoint {
            observation: LocationSample::new(
                observation.coordinate(),
                4_000,
                1_700_000_004_000,
                Some(500_000),
                LocationSource::Live,
            ),
            ..checkpoint
        },
    ] {
        assert_eq!(
            checkpoint_stationary_location_observation(&connection, rejected).unwrap(),
            RideLocationObservationCheckpointOutcome::PointChanged
        );
    }
    assert_eq!(connection.total_changes(), changes + 1);
    assert_eq!(
        find_ride(&connection, id)
            .unwrap()
            .unwrap()
            .stationary_location_observation(),
        Some(observation)
    );
}

#[test]
fn stationary_checkpoint_failure_rolls_back_all_evidence() {
    let (connection, id) = parked_connection();
    connection.execute_batch("CREATE TRIGGER deny_stationary BEFORE UPDATE OF stationary_latitude_e7 ON rides BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    assert!(
        checkpoint_stationary_location_observation(
            &connection,
            RideStationaryLocationCheckpoint {
                ride_id: id,
                expected_point_count: 0,
                observation: sample(3_000, 40.0)
            }
        )
        .is_err()
    );
    let ride = find_ride(&connection, id).unwrap().unwrap();
    assert_eq!(ride.stationary_location_observation(), None);
    assert_eq!(ride.last_location_observed_monotonic_milliseconds(), None);
    assert_eq!(ride.summary().point_count().as_u64(), 0);
    assert_eq!(
        ride.motion_observation().unwrap().observed_at_milliseconds,
        2_000
    );
}

#[test]
fn future_riding_receipt_does_not_reclassify_earlier_stationary_gps() {
    let (mut connection, id) = parked_connection();
    let motion = RideMotionObservation {
        parked: false,
        observed_at_milliseconds: 4_000,
        last_stationary_at_milliseconds: Some(2_000),
    };
    update_ride_map_metadata_with_motion(
        &connection,
        id,
        Some("wheel"),
        Some("wheel"),
        Some(1_000),
        Some(4_000),
        Some(motion),
    )
    .unwrap();
    assert_eq!(
        checkpoint_stationary_location_observation(
            &connection,
            RideStationaryLocationCheckpoint {
                ride_id: id,
                expected_point_count: 0,
                observation: sample(3_000, 40.0)
            }
        )
        .unwrap(),
        RideLocationObservationCheckpointOutcome::Applied
    );
    assert_eq!(
        checkpoint_stationary_location_observation(
            &connection,
            RideStationaryLocationCheckpoint {
                ride_id: id,
                expected_point_count: 0,
                observation: sample(5_000, 40.0)
            }
        )
        .unwrap(),
        RideLocationObservationCheckpointOutcome::PointChanged
    );
    assert_eq!(
        append_location(
            &mut connection,
            id,
            sample(5_000, 40.0001),
            RideMapSegmentId::new(0),
            RouteTelemetryState::AssociatedFresh
        )
        .unwrap(),
        LocationAdmission::Accepted
    );
    let ride = find_ride(&connection, id).unwrap().unwrap();
    assert_eq!(ride.stationary_location_observation(), None);
    assert_eq!(ride.motion_observation(), Some(motion));
    assert_eq!(ride.last_stationary_motion_at_milliseconds(), Some(2_000));
}

#[test]
fn conflicting_motion_rejects_the_entire_metadata_update() {
    let (connection, id) = parked_connection();
    let conflicting = RideMotionObservation {
        parked: false,
        observed_at_milliseconds: 2_000,
        last_stationary_at_milliseconds: Some(2_000),
    };
    assert!(
        update_ride_map_metadata_with_motion(
            &connection,
            id,
            Some("changed"),
            Some("wheel"),
            Some(1_000),
            Some(2_000),
            Some(conflicting)
        )
        .is_err()
    );
    let ride = find_ride(&connection, id).unwrap().unwrap();
    assert_eq!(ride.candidate_vehicle(), Some("wheel"));
    assert!(ride.motion_observation().unwrap().parked);
}

fn old_schema_fixture() -> (Connection, RideId, Vec<u8>) {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let id = create_started_live_ride(
        &mut connection,
        1_700_000_000_000,
        1_000,
        Some("wheel"),
        None,
    )
    .unwrap();
    append_location(
        &mut connection,
        id,
        sample(2_000, 40.0),
        RideMapSegmentId::new(0),
        RouteTelemetryState::GpsOnly,
    )
    .unwrap();
    let segment_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='ride_segments'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .pragma_update(None, "foreign_keys", false)
        .unwrap();
    connection
        .execute_batch(
            "CREATE TABLE old_segments AS SELECT * FROM ride_segments; DROP TABLE ride_segments;",
        )
        .unwrap();
    connection
        .execute_batch(&segment_sql.replace(", 'stationary'", ""))
        .unwrap();
    connection
        .execute_batch(
            "INSERT INTO ride_segments SELECT * FROM old_segments; DROP TABLE old_segments;",
        )
        .unwrap();
    for column in [
        "motion_parked",
        "motion_observed_monotonic_ms",
        "last_stationary_motion_monotonic_ms",
        "stationary_latitude_e7",
        "stationary_longitude_e7",
        "stationary_horizontal_accuracy_mm",
    ] {
        connection
            .execute_batch(&format!("ALTER TABLE rides DROP COLUMN {column};"))
            .unwrap();
    }
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    connection.pragma_update(None, "user_version", 39).unwrap();
    let before: Vec<u8> = connection.query_row("SELECT CAST(monotonic_ms || ':' || wall_clock_ms || ':' || latitude_e7 || ':' || longitude_e7 AS BLOB) FROM ride_points", [], |row| row.get(0)).unwrap();
    (connection, id, before)
}

#[test]
fn stationary_motion_migration_preserves_points_and_foreign_keys_without_rebuilding_payloads() {
    let (mut connection, id, before) = old_schema_fixture();
    let roots: Vec<(String, i64)> = {
        let mut statement = connection.prepare("SELECT name, rootpage FROM sqlite_schema WHERE name IN ('ride_points', 'live_capture_events', 'pevcap_capture_chunks') ORDER BY name").unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    migrate(&mut connection).unwrap();
    assert!(
        connection
            .pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
            .unwrap()
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        CURRENT_SCHEMA_VERSION
    );
    let after: Vec<u8> = connection.query_row("SELECT CAST(monotonic_ms || ':' || wall_clock_ms || ':' || latitude_e7 || ':' || longitude_e7 AS BLOB) FROM ride_points", [], |row| row.get(0)).unwrap();
    assert_eq!(after, before);
    for (name, root) in roots {
        assert_eq!(
            connection
                .query_row(
                    "SELECT rootpage FROM sqlite_schema WHERE name=?1",
                    [name],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            root
        );
    }
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap(),
        0
    );
    let ride = find_ride(&connection, id).unwrap().unwrap();
    assert_eq!(ride.summary().point_count().as_u64(), 1);
    assert_eq!(ride.motion_observation(), None);
    assert_eq!(ride.stationary_location_observation(), None);
    connection.execute("INSERT INTO ride_segments (ride_id, segment_id, point_count, sequence, start_reason, source, started_monotonic_ms, started_wall_clock_ms) VALUES (?1, 1, 0, 1, 'stationary', 'live', 3000, 1700000003000)", [id.uuid().to_string()]).unwrap();
    assert!(
        connection
            .execute(
                "DELETE FROM ride_segments WHERE ride_id=?1 AND segment_id=0",
                [id.uuid().to_string()]
            )
            .is_err()
    );
}

#[test]
fn stationary_motion_migration_failure_restores_foreign_keys_and_rolls_back_columns() {
    let (mut connection, _, before) = old_schema_fixture();
    connection
        .execute_batch("CREATE TABLE ride_segments_v40(blocked INTEGER);")
        .unwrap();
    assert!(migrate(&mut connection).is_err());
    assert!(
        connection
            .pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
            .unwrap()
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        39
    );
    assert_eq!(connection.query_row("SELECT COUNT(*) FROM pragma_table_info('rides') WHERE name IN ('motion_parked', 'motion_observed_monotonic_ms', 'last_stationary_motion_monotonic_ms', 'stationary_latitude_e7', 'stationary_longitude_e7', 'stationary_horizontal_accuracy_mm')", [], |row| row.get::<_, u64>(0)).unwrap(), 0);
    let after: Vec<u8> = connection.query_row("SELECT CAST(monotonic_ms || ':' || wall_clock_ms || ':' || latitude_e7 || ':' || longitude_e7 AS BLOB) FROM ride_points", [], |row| row.get(0)).unwrap();
    assert_eq!(after, before);
    connection
        .execute_batch("DROP TABLE ride_segments_v40;")
        .unwrap();
    migrate(&mut connection).unwrap();
    assert!(
        connection
            .pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
            .unwrap()
    );
}

#[test]
fn unchanged_motion_transition_allows_newer_durable_telemetry_receipt() {
    let (connection, id) = parked_connection();
    let motion = find_ride(&connection, id)
        .unwrap()
        .unwrap()
        .motion_observation()
        .unwrap();
    update_ride_map_metadata_with_motion(
        &connection,
        id,
        Some("wheel"),
        Some("wheel"),
        Some(1_000),
        Some(5_000),
        Some(motion),
    )
    .unwrap();
    let ride = find_ride(&connection, id).unwrap().unwrap();
    assert_eq!(ride.last_telemetry_at_milliseconds(), Some(5_000));
    assert_eq!(ride.motion_observation(), Some(motion));
    assert!(
        update_ride_map_metadata_with_motion(
            &connection,
            id,
            Some("wheel"),
            Some("wheel"),
            Some(1_000),
            Some(1_500),
            Some(motion)
        )
        .is_err()
    );
    assert_eq!(
        find_ride(&connection, id)
            .unwrap()
            .unwrap()
            .last_telemetry_at_milliseconds(),
        Some(5_000)
    );
}
