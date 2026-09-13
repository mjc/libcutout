use super::*;
use std::{
    fs,
    time::{Duration, Instant},
};
use uuid::Uuid;
#[test]
fn connection_telemetry_preserves_saved_intent_and_rejects_old_attempts() {
    let mut state = RideRecordingSession::new(None);
    let first = ConnectionAttemptToken {
        generation: 1,
        platform_identifier: "pev-1".to_owned(),
    };
    let second = ConnectionAttemptToken {
        generation: 2,
        platform_identifier: "pev-2".to_owned(),
    };
    let active = state.observe_connection_telemetry(&first, 1_000).unwrap();
    state
        .transition_at(ride_maps::RideEvent::Stop, 2_000)
        .unwrap();
    state.save().unwrap();
    let saved = state.observe_connection_telemetry(&first, 2_100).unwrap();
    assert_eq!(saved.ride_id, active.ride_id);
    assert_eq!(saved.state, ride_maps::RideLifecycleState::Saved);
    let next = state.observe_connection_telemetry(&second, 3_000).unwrap();
    assert_ne!(next.ride_id, saved.ride_id);
    assert_eq!(next.associated_vehicle.as_deref(), Some("pev-2"));
    let delayed = state.observe_connection_telemetry(&first, 4_000).unwrap();
    assert_eq!(delayed.ride_id, next.ride_id);
    assert_eq!(delayed.associated_vehicle, next.associated_vehicle);
}
#[test]
fn connection_telemetry_retries_deferred_association_without_resuming_pause() {
    let mut state = RideRecordingSession::new(None);
    state.start_gps_only(1_000, None).unwrap();
    state
        .ingest_location(5_000, 1_700_000_005_000, 40.0, -105.0, 3.0)
        .unwrap();
    let token = ConnectionAttemptToken {
        generation: 1,
        platform_identifier: "pev-1".to_owned(),
    };
    let deferred = state.observe_connection_telemetry(&token, 4_000).unwrap();
    assert!(deferred.associated_vehicle.is_none());
    let associated = state.observe_connection_telemetry(&token, 6_000).unwrap();
    assert_eq!(associated.associated_vehicle.as_deref(), Some("pev-1"));
    assert_eq!(
        associated.telemetry_state,
        ride_maps::RouteTelemetryState::AssociatedFresh
    );
    state
        .transition_at(ride_maps::RideEvent::Pause, 6_500)
        .unwrap();
    let paused = state.observe_connection_telemetry(&token, 6_600).unwrap();
    assert_eq!(paused.state, ride_maps::RideLifecycleState::Paused);
    assert_eq!(paused.ride_id, associated.ride_id);
}
#[test]
fn discarded_ride_retains_lifecycle_without_retaining_music_context() {
    let mut state = RideRecordingSession::new(None);
    state.start_gps_only(1_000, None).unwrap();
    state
        .transition_at(ride_maps::RideEvent::Stop, 2_000)
        .unwrap();
    state.discard().unwrap();
    assert!(state.current_music_history().is_none());
    assert_eq!(
        state.current_music_history_policy(),
        MusicHistoryPolicy::Disabled
    );
    assert_eq!(
        state.current_snapshot(2_000).unwrap().state,
        ride_maps::RideLifecycleState::Discarded
    );
}
#[test]
fn mobile_ride_map_core_retains_terminal_snapshot_and_explicit_stop() {
    let mut state = RideRecordingSession::new(None);
    let started = state.ensure_recording_for_vehicle("pev-1", 1_000).unwrap();
    state
        .transition_at(ride_maps::RideEvent::Stop, 2_000)
        .unwrap();
    let reconnected = state.ensure_recording_for_vehicle("pev-1", 3_000).unwrap();
    assert_eq!(reconnected.ride_id, started.ride_id);
    assert_eq!(reconnected.state, ride_maps::RideLifecycleState::Stopped);
    state.save().unwrap();
    let saved = state
        .current_snapshot(4_000)
        .expect("saved state remains authoritative");
    assert_eq!(saved.ride_id, started.ride_id);
    assert_eq!(saved.state, ride_maps::RideLifecycleState::Saved);
    let next = state.ensure_recording_for_vehicle("pev-1", 5_000).unwrap();
    assert_ne!(next.ride_id, saved.ride_id);
}
#[test]
fn mobile_ride_map_core_bounds_pending_location_backpressure() {
    let _guard = crate::tests::test_guard();
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-map-backpressure-{}-{}.sqlite3",
        std::process::id(),
        Uuid::new_v4()
    ));
    let _ = fs::remove_file(&path);
    let database = RideDatabase::open(&path).expect("database opens");
    let mut state = RideRecordingSession::new(Some(database.clone()));
    state.start_gps_only(1_000, None).expect("recording starts");

    for index in 0..MAX_PENDING_LOCATION_WRITES {
        let monotonic_ms = 1_001 + index as u64 * 1_000;
        let decision = state
            .ingest_location(
                monotonic_ms,
                1_700_000_000_000 + monotonic_ms,
                40.0 + f64::from(u32::try_from(index).expect("bounded index")) * 0.00001,
                -105.0,
                3.0,
            )
            .expect("location remains nonblocking under queue load");
        assert!(matches!(decision, RecordingDecision::Pending { .. }));
    }
    let started = Instant::now();
    let decision = state
        .ingest_location(
            1_001 + MAX_PENDING_LOCATION_WRITES as u64 * 1_000,
            1_700_000_000_000 + 1_001 + MAX_PENDING_LOCATION_WRITES as u64 * 1_000,
            40.00065,
            -105.0,
            3.0,
        )
        .expect("queue saturation is reported as a decision");
    assert!(started.elapsed() < Duration::from_millis(100));
    assert!(matches!(decision, RecordingDecision::StorageError { .. }));

    database.shutdown().expect("database shuts down");
    let _ = fs::remove_file(path);
}
#[test]
fn mobile_ride_map_core_starts_new_when_the_interrupted_ride_is_older_than_three_hours() {
    let _guard = crate::tests::test_guard();
    let path = std::env::temp_dir().join(format!(
        "cutout-mobile-map-auto-new-old-{}-{}.sqlite3",
        std::process::id(),
        Uuid::new_v4()
    ));
    let original_ride_id = {
        let database = RideDatabase::open(&path).expect("database opens");
        let mut state = RideRecordingSession::new(Some(database.clone()));
        let snapshot = state
            .ensure_recording_for_vehicle("pev-1", 1_000)
            .expect("connection starts ride");
        database.shutdown().expect("database shuts down");
        snapshot.ride_id
    };
    let now_milliseconds = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock follows epoch")
            .as_millis(),
    )
    .expect("wall clock fits u64");
    let stale_update = now_milliseconds
        .saturating_sub(AUTO_RESUME_RIDE_WINDOW_MILLISECONDS)
        .saturating_sub(1);
    rusqlite::Connection::open(&path)
        .expect("sqlite opens")
        .execute(
            "UPDATE rides SET created_at_ms = ?1, updated_at_ms = ?1",
            [stale_update],
        )
        .expect("ride timestamp ages");

    let database = RideDatabase::open(&path).expect("database reopens");
    let mut state = RideRecordingSession::new(Some(database.clone()));
    let started = state
        .ensure_recording_for_vehicle("pev-1", 2_000)
        .expect("stale ride is replaced");
    assert_eq!(started.state, ride_maps::RideLifecycleState::Active);
    assert_ne!(started.ride_id, original_ride_id);

    database.shutdown().expect("reopened database shuts down");
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use ride_maps::{RideEvent, RideLifecycleState, RideSegmentStartReason};

    #[test]
    fn durable_manual_resume_rebases_clock_and_keeps_route_and_pause_time() {
        let _guard = crate::tests::test_guard();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rides.sqlite");
        let database = RideDatabase::open(&path).unwrap();
        let mut recording = RideRecordingSession::new(Some(database.clone()));
        let original = recording.start_gps_only(999_000, None).unwrap();
        recording
            .ingest_location(1_000_000, 1_700_000_000_000, 40.0, -105.0, 3.0)
            .unwrap();
        database.summary(original.ride_id).unwrap();
        recording.poll_location_writes();
        database.shutdown().unwrap();
        drop(recording);
        let database = RideDatabase::open(&path).unwrap();
        let mut recording = RideRecordingSession::new(Some(database.clone()));
        let recovered = recording.current_snapshot(1_000).unwrap();
        assert_eq!(
            recovered.allowed_actions,
            vec![RideEvent::Resume, RideEvent::Save, RideEvent::Discard]
        );
        assert_eq!(recovered.state, RideLifecycleState::Interrupted);
        let resumed = recording.transition_at(RideEvent::Resume, 1_000).unwrap();
        assert_eq!(resumed.ride_id, original.ride_id);
        assert_eq!(resumed.summary.duration_milliseconds, 1_000);
        recording
            .ingest_location(1_500, 1_700_000_010_500, 40.000_01, -105.0, 3.0)
            .unwrap();
        database.summary(original.ride_id).unwrap();
        recording.poll_location_writes();
        let points = recording.points_after(None, 10).unwrap().points;
        assert_eq!(points.len(), 2);
        assert_eq!(
            points[1].point.sample().monotonic_milliseconds().as_u64(),
            1_000_500
        );
        assert_eq!(
            points[1].point.segment_start_reason(),
            RideSegmentStartReason::Resume
        );
        recording.transition_at(RideEvent::Pause, 1_600).unwrap();
        recording.transition_at(RideEvent::Resume, 2_600).unwrap();
        assert_eq!(
            recording
                .current_snapshot(2_700)
                .unwrap()
                .summary
                .duration_milliseconds,
            1_700
        );
        database.shutdown().unwrap();
    }

    #[test]
    fn queued_location_identity_cannot_cross_pause_or_new_recording() {
        let mut recording = RideRecordingSession::new(None);
        let first = recording.start_gps_only(1_000, None).unwrap();
        let input = PevcapPhoneLocation {
            wall_clock_unix_ms: 1_700_000_002_000,
            latitude_degrees: 40.0,
            longitude_degrees: -105.0,
            altitude_meters: 1_000.0,
            horizontal_accuracy_meters: Some(3.0),
            vertical_accuracy_meters: None,
            speed_meters_per_second: None,
            speed_accuracy_meters_per_second: None,
            course_degrees: None,
            course_accuracy_degrees: None,
        };
        recording.transition_at(RideEvent::Pause, 1_500).unwrap();
        recording.transition_at(RideEvent::Resume, 1_600).unwrap();
        assert!(
            recording
                .ingest_location_batch(
                    first.recording_token,
                    2_000,
                    input.wall_clock_unix_ms,
                    vec![input.clone()]
                )
                .unwrap()
                .is_empty()
        );
        recording.transition_at(RideEvent::Stop, 2_100).unwrap();
        recording.save().unwrap();
        let next = recording.start_gps_only(2_200, None).unwrap();
        assert!(
            recording
                .ingest_location_batch(
                    first.recording_token,
                    2_500,
                    input.wall_clock_unix_ms,
                    vec![input]
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            recording.current_snapshot(2_500).unwrap().ride_id,
            next.ride_id
        );
        assert_eq!(
            recording
                .current_snapshot(2_500)
                .unwrap()
                .summary
                .point_count,
            0
        );
    }
}

#[test]
fn lifecycle_command_cannot_mutate_a_replacement_ride() {
    let mut session = RideRecordingSession::new(None);
    let first = session.start_gps_only(1_000, None).unwrap();
    session
        .transition_at(ride_maps::RideEvent::Stop, 2_000)
        .unwrap();
    session.save().unwrap();
    let replacement = session.start_gps_only(3_000, None).unwrap();

    assert_eq!(
        session.apply_command(
            Some(first.command_token),
            ride_maps::RideEvent::Pause,
            4_000,
            None
        ),
        Err(RecordingError::StaleCommand)
    );
    assert_eq!(session.current_snapshot(3_000).unwrap(), replacement);
}

#[test]
fn lifecycle_command_generation_changes_on_transitions_but_not_telemetry() {
    let mut session = RideRecordingSession::new(None);
    let attempt = ConnectionAttemptToken {
        generation: 1,
        platform_identifier: "pev-1".into(),
    };
    let active = session
        .observe_connection_telemetry(&attempt, 1_000)
        .unwrap();
    let refreshed = session
        .observe_connection_telemetry(&attempt, 1_100)
        .unwrap();
    assert_eq!(active.command_token, refreshed.command_token);
    let paused = session
        .apply_command(
            Some(active.command_token),
            ride_maps::RideEvent::Pause,
            1_200,
            None,
        )
        .unwrap();
    assert_ne!(active.command_token, paused.command_token);
    let resumed = session
        .apply_command(
            Some(paused.command_token),
            ride_maps::RideEvent::Resume,
            1_300,
            None,
        )
        .unwrap();
    assert_eq!(resumed.ride_id, active.ride_id);
    assert_eq!(
        session.apply_command(
            Some(active.command_token),
            ride_maps::RideEvent::Stop,
            1_400,
            None
        ),
        Err(RecordingError::StaleCommand)
    );
    assert_eq!(session.current_snapshot(1_300).unwrap(), resumed);
}

#[test]
fn start_command_observing_no_ride_cannot_replace_a_new_automatic_ride() {
    let mut session = RideRecordingSession::new(None);
    let automatic = session
        .ensure_recording_for_vehicle("pev-1", 1_000)
        .unwrap();
    assert_eq!(
        session.apply_command(None, ride_maps::RideEvent::Start, 1_100, None),
        Err(RecordingError::StaleCommand)
    );
    assert_eq!(session.current_snapshot(1_000).unwrap(), automatic);
}

#[test]
fn saved_listening_default_is_committed_for_manual_and_automatic_new_rides() {
    let _guard = crate::tests::test_guard();
    let path = std::env::temp_dir().join(format!(
        "cutout-ride-music-default-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));
    session.set_default_music_history_policy(MusicHistoryPolicy::HumanReadable);

    let automatic = session
        .ensure_recording_for_vehicle("pev-1", 1_000)
        .unwrap();
    assert_eq!(
        database.music_history_policy(automatic.ride_id).unwrap(),
        MusicHistoryPolicy::HumanReadable
    );
    assert_eq!(
        automatic.music_history_policy,
        MusicHistoryPolicy::HumanReadable
    );
    session
        .transition_at(ride_maps::RideEvent::Stop, 2_000)
        .unwrap();
    session.save().unwrap();
    let manual = session.start_gps_only(3_000, None).unwrap();
    assert_ne!(automatic.ride_id, manual.ride_id);
    assert_eq!(
        database.music_history_policy(manual.ride_id).unwrap(),
        MusicHistoryPolicy::HumanReadable
    );

    database.shutdown().unwrap();
    let _ = fs::remove_file(path);
}

#[test]
fn saved_listening_default_does_not_restore_deleted_history_on_reconnect_or_recovery() {
    let _guard = crate::tests::test_guard();
    let path = std::env::temp_dir().join(format!(
        "cutout-ride-music-deleted-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));
    session.set_default_music_history_policy(MusicHistoryPolicy::HumanReadable);
    let first = session
        .ensure_recording_for_vehicle("pev-1", 1_000)
        .unwrap();
    session.delete_current_music_history().unwrap();
    session
        .transition_at(ride_maps::RideEvent::Pause, 2_000)
        .unwrap();
    session.set_default_music_history_policy(MusicHistoryPolicy::OpaqueItem);
    let reconnected = session
        .ensure_recording_for_vehicle("pev-1", 3_000)
        .unwrap();
    assert_eq!(reconnected.ride_id, first.ride_id);
    assert_eq!(
        session.current_music_history_policy(),
        MusicHistoryPolicy::Disabled
    );
    assert_eq!(
        database.music_history(first.ride_id).unwrap().status,
        crate::MusicHistoryStatus::Deleted
    );
    drop(session);
    database.shutdown().unwrap();
    let database = RideDatabase::open(&path).unwrap();
    let mut recovered = RideRecordingSession::new(Some(database.clone()));
    recovered.set_default_music_history_policy(MusicHistoryPolicy::HumanReadable);
    assert_eq!(
        recovered.current_music_history_policy(),
        MusicHistoryPolicy::Disabled
    );
    assert_eq!(
        recovered.current_music_history().unwrap().unwrap().status,
        crate::MusicHistoryStatus::Deleted
    );
    database.shutdown().unwrap();
    let _ = fs::remove_file(path);
}

#[test]
fn saved_listening_default_failure_rolls_back_new_ride_and_preserves_retry() {
    let _guard = crate::tests::test_guard();
    let path = std::env::temp_dir().join(format!(
        "cutout-ride-music-atomic-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_music_policy BEFORE INSERT ON ride_music_history BEGIN SELECT RAISE(FAIL, 'injected retention failure'); END;").unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));
    session.set_default_music_history_policy(MusicHistoryPolicy::HumanReadable);
    assert!(
        session
            .ensure_recording_for_vehicle("pev-1", 1_000)
            .is_err()
    );
    assert!(session.current_snapshot(1_000).is_none());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM rides", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    connection
        .execute_batch("DROP TRIGGER reject_music_policy;")
        .unwrap();
    let retried = session
        .ensure_recording_for_vehicle("pev-1", 2_000)
        .unwrap();
    assert_eq!(
        database.music_history_policy(retried.ride_id).unwrap(),
        MusicHistoryPolicy::HumanReadable
    );
    database.shutdown().unwrap();
    let _ = fs::remove_file(path);
}

#[test]
fn location_acquisition_is_owned_by_the_recording_lifecycle() {
    let mut session = RideRecordingSession::new(None);
    session.observe_location_environment(LocationEnvironment {
        authorization: LocationAuthorization::Always,
        services_enabled: true,
        temporarily_unavailable: false,
    });
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
    session.start_gps_only(1_000, None).unwrap();
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );
    session
        .transition_at(ride_maps::RideEvent::Pause, 2_000)
        .unwrap();
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
    session
        .transition_at(ride_maps::RideEvent::Resume, 3_000)
        .unwrap();
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );
    session
        .transition_at(ride_maps::RideEvent::Stop, 4_000)
        .unwrap();
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
    session.save().unwrap();
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
}

#[test]
fn location_acquisition_distinguishes_permission_services_and_temporary_failure() {
    let mut session = RideRecordingSession::new(None);
    let cases = [
        (
            LocationAuthorization::NotDetermined,
            true,
            false,
            LocationAvailability::PermissionRequired,
            LocationDemand::RequestPermission,
        ),
        (
            LocationAuthorization::Denied,
            true,
            false,
            LocationAvailability::Denied,
            LocationDemand::Idle,
        ),
        (
            LocationAuthorization::Restricted,
            true,
            false,
            LocationAvailability::Restricted,
            LocationDemand::Idle,
        ),
        (
            LocationAuthorization::Always,
            false,
            false,
            LocationAvailability::ServicesDisabled,
            LocationDemand::Idle,
        ),
        (
            LocationAuthorization::WhenInUse,
            true,
            false,
            LocationAvailability::Ready,
            LocationDemand::Record,
        ),
        (
            LocationAuthorization::Always,
            true,
            true,
            LocationAvailability::TemporarilyUnavailable,
            LocationDemand::Record,
        ),
    ];
    session.start_gps_only(1_000, None).unwrap();
    for (authorization, services_enabled, temporarily_unavailable, availability, demand) in cases {
        session.observe_location_environment(LocationEnvironment {
            authorization,
            services_enabled,
            temporarily_unavailable,
        });
        let snapshot = session.location_acquisition();
        assert_eq!(snapshot.availability, availability);
        assert_eq!(snapshot.demand, demand);
        assert_eq!(
            session.current_snapshot(1_000).unwrap().state,
            ride_maps::RideLifecycleState::Active
        );
    }
}

#[test]
fn diagnostic_capture_location_demand_is_explicit_and_generation_scoped() {
    let mut session = RideRecordingSession::new(None);
    session.observe_location_environment(LocationEnvironment {
        authorization: LocationAuthorization::Always,
        services_enabled: true,
        temporarily_unavailable: false,
    });
    // An ordinary telemetry capture has no independent acquisition input.
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
    session.observe_diagnostic_capture_location(1, true);
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );
    assert!(session.current_snapshot(1_000).is_none());
    session.observe_diagnostic_capture_location(2, true);
    session.observe_diagnostic_capture_location(1, false);
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );
    session.start_gps_only(1_000, None).unwrap();
    session
        .transition_at(ride_maps::RideEvent::Pause, 2_000)
        .unwrap();
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );
    session.observe_diagnostic_capture_location(2, false);
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
    let closed_revision = session.location_acquisition().revision;
    session.observe_diagnostic_capture_location(2, true);
    session.observe_diagnostic_capture_location(1, true);
    assert_eq!(session.location_acquisition().revision, closed_revision);
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
    assert_eq!(
        session.current_snapshot(2_000).unwrap().state,
        ride_maps::RideLifecycleState::Paused
    );
}

#[test]
fn recording_checkpoint_settles_pending_points_without_ending_the_active_ride() {
    let _guard = crate::tests::test_guard();
    let path =
        std::env::temp_dir().join(format!("cutout-ride-checkpoint-{}.sqlite3", Uuid::new_v4()));
    let database = RideDatabase::open(&path).unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));
    let active = session.start_gps_only(1_000, None).unwrap();
    assert!(matches!(
        session
            .ingest_location(2_000, 1_700_000_002_000, 40.0, -105.0, 3.0)
            .unwrap(),
        RecordingDecision::Pending { .. }
    ));
    let decisions = session.checkpoint().unwrap();
    assert!(matches!(
        decisions.as_slice(),
        [RecordingDecision::Accepted { .. }]
    ));
    let checkpoint = session.current_snapshot(2_000).unwrap();
    assert_eq!(checkpoint.ride_id, active.ride_id);
    assert_eq!(checkpoint.state, ride_maps::RideLifecycleState::Active);
    assert_eq!(checkpoint.summary.point_count, 1);
    assert!(session.checkpoint().unwrap().is_empty());
    drop(session);
    database.shutdown().unwrap();
    let database = RideDatabase::open(&path).unwrap();
    let recovered = RideRecordingSession::new(Some(database.clone()))
        .current_snapshot(2_000)
        .unwrap();
    assert_eq!(recovered.ride_id, active.ride_id);
    assert_eq!(recovered.state, ride_maps::RideLifecycleState::Interrupted);
    assert_eq!(recovered.summary.point_count, 1);
    database.shutdown().unwrap();
    let _ = fs::remove_file(path);
}

#[test]
fn recording_checkpoint_reports_a_closed_worker_without_claiming_durability() {
    let _guard = crate::tests::test_guard();
    let path = std::env::temp_dir().join(format!(
        "cutout-ride-checkpoint-closed-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));
    let active = session.start_gps_only(1_000, None).unwrap();
    database.shutdown().unwrap();
    assert!(session.checkpoint().is_err());
    let snapshot = session.current_snapshot(1_000).unwrap();
    assert_eq!(snapshot.ride_id, active.ride_id);
    assert_eq!(snapshot.state, ride_maps::RideLifecycleState::Active);
    let _ = fs::remove_file(path);
}

#[test]
fn explicit_disconnect_pauses_durably_and_does_not_resume_on_reconnect() {
    let _guard = crate::tests::test_guard();
    let path =
        std::env::temp_dir().join(format!("cutout-ride-disconnect-{}.sqlite3", Uuid::new_v4()));
    let database = RideDatabase::open(&path).unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));
    let active = session
        .ensure_recording_for_vehicle("pev-1", 1_000)
        .unwrap();
    let paused = session
        .prepare_disconnect(Some(active.command_token), 2_000)
        .unwrap()
        .unwrap();
    assert_eq!(paused.state, ride_maps::RideLifecycleState::Paused);
    assert_eq!(paused.ride_id, active.ride_id);
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
    let reconnected = session
        .ensure_recording_for_vehicle("pev-1", 3_000)
        .unwrap();
    assert_eq!(reconnected.state, ride_maps::RideLifecycleState::Paused);
    database.shutdown().unwrap();
    let database = RideDatabase::open(&path).unwrap();
    let recovered = RideRecordingSession::new(Some(database.clone()))
        .current_snapshot(3_000)
        .unwrap();
    assert_eq!(recovered.state, ride_maps::RideLifecycleState::Interrupted);
    assert_eq!(recovered.summary.duration_milliseconds, 1_000);
    database.shutdown().unwrap();
    let _ = fs::remove_file(path);
}

#[test]
fn explicit_disconnect_preserves_absent_terminal_and_replacement_recordings() {
    let mut session = RideRecordingSession::new(None);
    assert!(session.prepare_disconnect(None, 0).unwrap().is_none());
    let first = session.start_gps_only(1_000, None).unwrap();
    session
        .transition_at(ride_maps::RideEvent::Stop, 2_000)
        .unwrap();
    let saved = session.save().unwrap();
    assert_eq!(
        session
            .prepare_disconnect(Some(saved.command_token), 3_000)
            .unwrap()
            .unwrap()
            .state,
        ride_maps::RideLifecycleState::Saved
    );
    let replacement = session.start_gps_only(4_000, None).unwrap();
    assert!(matches!(
        session.prepare_disconnect(Some(first.command_token), 5_000),
        Err(RecordingError::StaleCommand)
    ));
    assert_eq!(
        session.current_snapshot(5_000).unwrap().recording_token,
        replacement.recording_token
    );
    assert_eq!(
        session.current_snapshot(5_000).unwrap().state,
        ride_maps::RideLifecycleState::Active
    );
}

#[test]
fn explicit_disconnect_failure_preserves_active_recording_for_retry() {
    let _guard = crate::tests::test_guard();
    let path = std::env::temp_dir().join(format!(
        "cutout-ride-disconnect-failure-{}.sqlite3",
        Uuid::new_v4()
    ));
    let database = RideDatabase::open(&path).unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));
    let active = session.start_gps_only(1_000, None).unwrap();
    database.shutdown().unwrap();
    assert!(
        session
            .prepare_disconnect(Some(active.command_token), 2_000)
            .is_err()
    );
    let current = session.current_snapshot(2_000).unwrap();
    assert_eq!(current.state, ride_maps::RideLifecycleState::Active);
    assert_eq!(current.command_token, active.command_token);
    let _ = fs::remove_file(path);
}
