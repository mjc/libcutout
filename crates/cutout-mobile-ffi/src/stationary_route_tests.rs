use super::*;

fn connected_ride() -> (
    Arc<CutoutSessionStateHandle>,
    Arc<MobileRideMapCore>,
    MobileConnectionAttemptTokenDto,
) {
    connect_core(MobileRideMapCore::new())
}

fn connect_core(
    core: Arc<MobileRideMapCore>,
) -> (
    Arc<CutoutSessionStateHandle>,
    Arc<MobileRideMapCore>,
    MobileConnectionAttemptTokenDto,
) {
    let session = CutoutSessionStateHandle::new();
    core.start_gps_only(1_000).unwrap();
    let token = session
        .begin_connection_attempt("wheel".into(), 1_000)
        .token
        .unwrap();
    session.connection_link_established(token.clone());
    assert!(
        session
            .lock_inner()
            .session_state_mut()
            .connection
            .finish_detection(&token.clone().into(), true)
    );
    let admission = session
        .begin_ride_recording_for_verified_connection(
            core.clone(),
            token.clone(),
            1_100,
            MobileMusicHistoryPolicyDto::Disabled,
        )
        .unwrap();
    admission.wait().unwrap();
    (session, core, token)
}

fn wheel_speed(
    session: &CutoutSessionStateHandle,
    core: Arc<MobileRideMapCore>,
    token: &MobileConnectionAttemptTokenDto,
    at: u64,
    speed: i32,
) {
    session
        .observe_ride_telemetry_for_verified_connection(
            core,
            token.clone(),
            at,
            Some(MobileRideMapSpeedObservationDto {
                millimetres_per_second: speed,
                observed_at_ms: at,
            }),
        )
        .unwrap();
}

fn location(core: &MobileRideMapCore, at: u64, latitude: f64) -> MobileRideMapCoreSnapshotDto {
    source_location(core, at, at, latitude).snapshot
}

fn source_location(
    core: &MobileRideMapCore,
    source_at: u64,
    receipt_at: u64,
    latitude: f64,
) -> MobileRideMapCoreOutcomeDto {
    let recording = core.current_snapshot(receipt_at).unwrap().recording_token;
    let outcomes = core
        .ingest_location_callback_with_outcomes(
            recording,
            receipt_at,
            1_700_000_000_000 + receipt_at,
            vec![MobilePhoneLocationSampleDto {
                wall_clock_unix_ms: 1_700_000_000_000 + source_at,
                source_timestamp_unix_seconds: None,
                latitude_degrees: latitude,
                longitude_degrees: -105.0,
                altitude_meters: 1_600.0,
                horizontal_accuracy_meters: Some(4.0),
                vertical_accuracy_meters: None,
                speed_meters_per_second: Some(3.0),
                speed_accuracy_meters_per_second: None,
                course_degrees: None,
                course_accuracy_degrees: None,
            }],
        )
        .unwrap();
    assert!(
        outcomes.iter().all(|outcome| !matches!(
            outcome.decision,
            MobileRideMapCoreDecisionDto::StorageError { .. }
        )),
        "{outcomes:?}"
    );
    outcomes.last().unwrap().clone()
}

#[test]
fn stationary_connected_wheel_does_not_record_gps_drift() {
    for speed in [0, 500, -500] {
        let (session, core, token) = connected_ride();
        wheel_speed(&session, core.clone(), &token, 1_200, speed);
        location(&core, 1_300, 40.0);
        let snapshot = location(&core, 1_400, 40.000_02);
        assert!(
            snapshot.summary.distance_meters.abs() < f64::EPSILON,
            "stationary speed {speed} must veto GPS drift; recorded {} m",
            snapshot.summary.distance_meters
        );
        assert!(
            snapshot.summary.point_count <= 1,
            "stationary fixes must not grow route geometry"
        );
    }
}

#[test]
fn parked_link_loss_does_not_record_drive_home_but_riding_link_loss_keeps_gps() {
    for initially_parked in [true, false] {
        let (session, core, token) = connected_ride();
        wheel_speed(
            &session,
            core.clone(),
            &token,
            1_200,
            if initially_parked { 0 } else { 3_000 },
        );
        location(&core, 1_300, 40.0);
        session.connection_link_down(token);
        let snapshot = location(&core, 10_000, 40.000_5);
        if initially_parked {
            assert_eq!(snapshot.summary.point_count, 0);
            assert!(snapshot.summary.distance_meters.abs() < f64::EPSILON);
        } else {
            assert_eq!(snapshot.summary.point_count, 2);
            assert!(snapshot.summary.distance_meters > 50.0);
        }
    }
}

#[test]
fn delayed_gps_uses_source_motion_and_movement_resumes_without_stationary_bridge() {
    let (session, core, token) = connected_ride();
    wheel_speed(&session, core.clone(), &token, 1_200, 501);
    location(&core, 1_300, 40.0);
    wheel_speed(&session, core.clone(), &token, 1_800, 0);
    let before_parked = source_location(&core, 1_600, 4_000, 40.000_01);
    assert!(matches!(
        before_parked.decision,
        MobileRideMapCoreDecisionDto::Accepted { .. }
    ));
    let baseline = before_parked.snapshot.summary.distance_meters;
    assert!(baseline > 1.0);
    let parked = source_location(&core, 2_000, 4_100, 40.000_02);
    assert!(matches!(
        parked.decision,
        MobileRideMapCoreDecisionDto::Ignored {
            reason: MobileRideMapDecisionReasonDto::Parked
        }
    ));
    // A later decoded frame does not retroactively change this historical GPS callback.
    session
        .observe_ride_telemetry_for_verified_connection(
            core.clone(),
            token,
            4_200,
            Some(MobileRideMapSpeedObservationDto {
                millimetres_per_second: -501,
                observed_at_ms: 2_500,
            }),
        )
        .unwrap();
    let still_parked = source_location(&core, 2_300, 4_300, 40.000_03);
    assert!(matches!(
        still_parked.decision,
        MobileRideMapCoreDecisionDto::Ignored {
            reason: MobileRideMapDecisionReasonDto::Parked
        }
    ));
    let replay = source_location(&core, 2_200, 4_400, 40.000_02);
    assert!(matches!(
        replay.decision,
        MobileRideMapCoreDecisionDto::Rejected {
            reason: MobileRideMapDecisionReasonDto::TimestampOutOfOrder
        }
    ));
    let resumed = source_location(&core, 2_600, 4_500, 40.000_2);
    let MobileRideMapCoreDecisionDto::Accepted { point } = resumed.decision else {
        panic!("movement was not admitted: {resumed:?}");
    };
    assert_eq!(
        point.start_reason,
        MobileRideSegmentStartReasonDto::Stationary
    );
    assert!((resumed.snapshot.summary.distance_meters - baseline).abs() < f64::EPSILON);
    let moving = source_location(&core, 2_700, 4_600, 40.000_21);
    assert!(moving.snapshot.summary.distance_meters > baseline + 1.0);
}

#[test]
fn stale_or_unverified_speed_cannot_release_parked_and_gps_only_stays_independent() {
    let (session, core, token) = connected_ride();
    wheel_speed(&session, core.clone(), &token, 1_200, 0);
    session
        .observe_ride_telemetry_for_verified_connection(
            core.clone(),
            token.clone(),
            5_000,
            Some(MobileRideMapSpeedObservationDto {
                millimetres_per_second: 10_000,
                observed_at_ms: 1_500,
            }),
        )
        .unwrap();
    session.connection_link_down(token.clone());
    assert!(
        session
            .observe_ride_telemetry_for_verified_connection(
                core.clone(),
                token,
                6_000,
                Some(MobileRideMapSpeedObservationDto {
                    millimetres_per_second: 10_000,
                    observed_at_ms: 6_000
                })
            )
            .is_err()
    );
    assert_eq!(location(&core, 7_000, 40.0).summary.point_count, 0);
    let independent = MobileRideMapCore::new();
    independent.start_gps_only(1_000).unwrap();
    location(&independent, 1_300, 40.0);
    assert!(
        location(&independent, 1_400, 40.000_01)
            .summary
            .distance_meters
            > 1.0
    );
}

#[test]
fn durable_parked_before_first_gps_blocks_drive_home_after_owner_restart() {
    let _guard = tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-stationary-before-gps-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
    let initial = MobileRideMapCore::with_database(database.clone());
    initial.restore(1_000).unwrap();
    let (session, core, token) = connect_core(initial);
    wheel_speed(&session, core.clone(), &token, 1_200, 0);
    // The restore only reads the database: the new owner has no session or speed cache.
    drop(core);
    let restored = MobileRideMapCore::with_database(database.clone());
    restored.restore(1_200).unwrap();
    let snapshot = location(&restored, 10_000, 40.01);
    assert_eq!(snapshot.summary.point_count, 0);
    assert!(snapshot.summary.distance_meters.abs() < f64::EPSILON);
    let ride_id = restored.inner.lock().unwrap().ride_id.clone().unwrap();
    let stored = database
        .inner
        .find_ride(parse_mobile_ride_id(&ride_id).unwrap())
        .unwrap()
        .unwrap();
    assert!(stored.motion_observation().unwrap().parked);
    assert!(stored.stationary_location_observation().is_some());
}

#[test]
fn durable_parked_then_riding_before_gps_preserves_boundary_after_owner_restart() {
    let _guard = tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-stationary-moving-before-gps-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
    let initial = MobileRideMapCore::with_database(database.clone());
    initial.restore(1_000).unwrap();
    let (session, core, token) = connect_core(initial);
    wheel_speed(&session, core.clone(), &token, 1_200, 3_000);
    location(&core, 1_300, 40.0);
    let baseline = location(&core, 1_400, 40.000_01).summary.distance_meters;
    wheel_speed(&session, core.clone(), &token, 1_500, 0);
    wheel_speed(&session, core.clone(), &token, 1_600, 3_000);
    drop(core);
    let restored = MobileRideMapCore::with_database(database.clone());
    restored.restore(1_600).unwrap();
    let historical_parked = source_location(&restored, 1_550, 1_650, 40.000_02);
    assert!(
        matches!(
            historical_parked.decision,
            MobileRideMapCoreDecisionDto::Ignored {
                reason: MobileRideMapDecisionReasonDto::Parked,
            }
        ),
        "historical parked source must not become unknown after restore"
    );
    let resumed = source_location(&restored, 1_700, 1_700, 40.000_12);
    let MobileRideMapCoreDecisionDto::Accepted { point } = resumed.decision else {
        panic!("{resumed:?}");
    };
    assert_eq!(
        point.start_reason,
        MobileRideSegmentStartReasonDto::Stationary
    );
    assert!((resumed.snapshot.summary.distance_meters - baseline).abs() < f64::EPSILON);
    assert!(
        location(&restored, 1_800, 40.000_13)
            .summary
            .distance_meters
            > baseline + 1.0
    );
}

#[test]
fn retired_wheel_motion_history_cannot_turn_delayed_parked_fixes_into_distance() {
    let (session, core, token) = connected_ride();
    for at in 1_200..1_300 {
        wheel_speed(
            &session,
            core.clone(),
            &token,
            at,
            if at % 2 == 0 { 0 } else { 501 },
        );
    }
    let retired = source_location(&core, 1_220, 1_500, 40.0);
    assert!(matches!(
        retired.decision,
        MobileRideMapCoreDecisionDto::Rejected {
            reason: MobileRideMapDecisionReasonDto::MotionHistoryRetired,
        }
    ));
    assert_eq!(retired.snapshot.summary.point_count, 0);
    let initial_unknown = source_location(&core, 1_150, 1_600, 40.0);
    assert!(matches!(
        initial_unknown.decision,
        MobileRideMapCoreDecisionDto::Accepted { .. }
    ));
}

#[test]
fn repeated_parked_frames_keep_transition_time_and_lifecycle_checkpoints_succeed() {
    let _guard = tests::RIDE_DATABASE_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let path = std::env::temp_dir().join(format!(
        "cutout-stationary-repeated-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = open_ride_database(path.to_string_lossy().into_owned()).unwrap();
    let initial = MobileRideMapCore::with_database(database.clone());
    initial.restore(1_000).unwrap();
    let (session, core, token) = connect_core(initial);
    wheel_speed(&session, core.clone(), &token, 1_200, 0);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE motion_write_count (value INTEGER NOT NULL); INSERT INTO motion_write_count VALUES (0); CREATE TRIGGER motion_write_count_update AFTER UPDATE ON rides BEGIN UPDATE motion_write_count SET value = value + 1; END;").unwrap();
    wheel_speed(&session, core.clone(), &token, 1_300, 0);
    wheel_speed(&session, core.clone(), &token, 1_400, 500);
    let writes: u64 = connection
        .query_row("SELECT value FROM motion_write_count", [], |row| row.get(0))
        .unwrap();
    assert_eq!(writes, 0, "same-Parked frames do not add SQL writes");
    session
        .observe_ride_telemetry_for_verified_connection(
            core.clone(),
            token,
            1_450,
            Some(MobileRideMapSpeedObservationDto {
                millimetres_per_second: 3_000,
                observed_at_ms: 1_350,
            }),
        )
        .unwrap();
    core.pause_at(1_500).unwrap();
    core.resume_at(1_600).unwrap();
    assert_eq!(location(&core, 1_700, 40.0).summary.point_count, 0);
    core.stop_at(1_800).unwrap();
    let ride_id = core.inner.lock().unwrap().ride_id.clone().unwrap();
    let stored = database
        .inner
        .find_ride(parse_mobile_ride_id(&ride_id).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.motion_observation(),
        Some(persistence::RideMotionObservation {
            parked: true,
            observed_at_milliseconds: 1_200,
            last_stationary_at_milliseconds: Some(1_200),
        })
    );
    assert_eq!(stored.last_telemetry_at_milliseconds(), Some(1_450));
    assert_eq!(stored.state(), ride_maps::RideLifecycleState::Stopped);
}
