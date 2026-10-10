use super::{
    LocationAuthorization, LocationAvailability, LocationDemand, LocationEnvironment,
    RecordingError, RecordingSpeedSource, RecordingSpeedState, RideRecordingSession,
};
use crate::RideDatabase;
use cutout_music::MusicHistoryPolicy;
use cutout_ride_maps::{RideEvent, RideLifecycleState};

#[test]
fn wheel_motion_is_latched_and_fenced_by_source_time_and_fresh_verified_evidence() {
    let mut state = RecordingSpeedState::default();
    assert!(!state.parked_at(1_000));
    assert!(state.observe_verified_motion(499, 1_000, 1_000));
    assert!(!state.parked_at(999));
    assert!(state.parked_at(1_000));
    assert!(state.parked_at(90_000));
    assert!(!state.observe_verified_motion(501, 2_000, 4_001));
    assert!(state.parked_at(90_000));
    assert!(state.observe_verified_motion(-501, 3_000, 5_000));
    assert!(state.parked_at(2_999));
    assert!(!state.parked_at(3_000));
    assert_eq!(
        state
            .latest_motion()
            .unwrap()
            .last_stationary_at_milliseconds,
        Some(1_000)
    );
    assert!(!state.observe_verified_motion(0, 2_000, 5_000));
    let mut restored = RecordingSpeedState::default();
    restored.restore_motion(state.latest_motion().unwrap());
    assert!(
        restored.parked_at(2_999),
        "persisted prior Parked interval remains authoritative after restart"
    );
    assert!(!restored.parked_at(3_000));
    assert_eq!(
        restored.selected_at(RideLifecycleState::Active, 0, 3_000),
        None
    );
}

#[test]
fn bounded_motion_history_never_borrows_a_future_transition() {
    let mut state = RecordingSpeedState::default();
    for at in 1..=100 {
        assert!(state.observe_verified_motion(if at % 2 == 0 { 501 } else { 0 }, at, at));
    }
    assert_eq!(
        state.motion_at(1),
        None,
        "retired history cannot borrow a future wheel state"
    );
    assert!(state.motion_history_retired_at(1));
    assert!(
        !state.motion_history_retired_at(0),
        "true initial unknown remains independent GPS fallback"
    );
    assert!(state.parked_at(99));
    assert!(!state.parked_at(100));
}

#[test]
fn phone_gps_replayed_receipts_cannot_replace_speed_or_extend_freshness() {
    let mut speed = RecordingSpeedState::default();
    speed.observe_phone_gps(Some(7.0), 2_000, 7);
    speed.observe_phone_gps(Some(99.0), 2_000, 7);
    speed.observe_phone_gps(Some(99.0), 1_999, 7);
    assert_eq!(
        speed
            .selected_at(RideLifecycleState::Active, 7, 2_500)
            .unwrap()
            .millimetres_per_second,
        7_000
    );
    speed.observe_phone_gps(None, 3_000, 7);
    speed.observe_phone_gps(Some(99.0), 2_500, 7);
    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 4_001),
        None
    );
    speed.observe_phone_gps(Some(8.0), 4_002, 7);
    assert_eq!(
        speed
            .selected_at(RideLifecycleState::Active, 7, 4_003)
            .unwrap()
            .millimetres_per_second,
        8_000
    );
    speed.observe_phone_gps(Some(9.0), 1_000, 8);
    assert_eq!(
        speed
            .selected_at(RideLifecycleState::Active, 8, 1_001)
            .unwrap()
            .millimetres_per_second,
        9_000
    );
}

#[test]
fn rejected_start_preserves_the_published_recording_and_token() {
    let mut session = RideRecordingSession::new(None);
    assert_eq!(session.snapshot(), None);
    let first = session.start_gps_only(1_000, None).unwrap();

    assert_eq!(
        session.start_gps_only(2_000, None),
        Err(RecordingError::AlreadyRecording)
    );
    assert_eq!(session.snapshot(), Some(&first));
    assert_eq!(first.recording_token.unwrap().ride_id, first.ride_id);
}

#[test]
fn database_backed_start_requires_async_coordination_without_publishing_a_ride() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("recording.sqlite")).unwrap();
    let mut session = RideRecordingSession::new(Some(database.clone()));

    assert_eq!(session.snapshot(), None);
    assert_eq!(
        session.start_gps_only(1_000, None),
        Err(RecordingError::DatabaseCommandRequired)
    );
    assert_eq!(session.snapshot(), None);
    database.shutdown().unwrap();
}

#[test]
fn queued_ride_creation_commits_its_music_history_policy_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("recording.sqlite")).unwrap();

    let pending = database
        .queue_create_started_live_ride_with_music_policy(
            1_000,
            10,
            None,
            Some(MusicHistoryPolicy::HumanReadable),
        )
        .unwrap();
    let ride_id = pending.wait_result().unwrap();

    assert_eq!(
        database.music_history_policy(ride_id).unwrap(),
        MusicHistoryPolicy::HumanReadable
    );
    database.shutdown().unwrap();
}

#[test]
fn lifecycle_transition_invalidates_the_previous_recording_token() {
    let mut session = RideRecordingSession::new(None);
    let started = session.start_gps_only(1_000, None).unwrap();
    let started_token = started.recording_token.unwrap();

    let paused = session.transition(RideEvent::Pause, 2_000).unwrap();

    assert_eq!(paused.state, RideLifecycleState::Paused);
    assert_eq!(paused.revision, started.revision + 1);
    assert_eq!(paused.recording_token, None);
    assert_eq!(session.snapshot(), Some(&paused));

    let resumed = session.transition(RideEvent::Resume, 3_000).unwrap();

    assert_eq!(resumed.state, RideLifecycleState::Active);
    assert_eq!(resumed.revision, paused.revision + 1);
    assert_eq!(
        resumed.recording_token.map(|token| token.ride_id),
        Some(started_token.ride_id)
    );
    assert_eq!(
        resumed.recording_token.map(|token| token.generation),
        Some(started_token.generation + 2)
    );
}

#[test]
fn invalid_transition_preserves_the_published_recording() {
    let mut session = RideRecordingSession::new(None);
    let started = session.start_gps_only(1_000, None).unwrap();

    assert_eq!(
        session.transition(RideEvent::Save, 2_000),
        Err(RecordingError::InvalidTransition)
    );
    assert_eq!(session.snapshot(), Some(&started));
}

#[test]
fn location_demand_tracks_recording_lifecycle() {
    let mut session = RideRecordingSession::new(None);
    let environment = LocationEnvironment {
        authorization: LocationAuthorization::Always,
        services_enabled: true,
        temporarily_unavailable: false,
    };
    session.observe_location_environment(environment);
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);

    session.start_gps_only(1_000, None).unwrap();
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );

    session.transition(RideEvent::Pause, 2_000).unwrap();
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);

    session.transition(RideEvent::Resume, 3_000).unwrap();
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );

    session.transition(RideEvent::Stop, 4_000).unwrap();
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);
}

#[test]
fn location_availability_separates_permission_services_and_provider_failure() {
    let mut session = RideRecordingSession::new(None);
    session.start_gps_only(1_000, None).unwrap();
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

    for (authorization, services_enabled, temporarily_unavailable, availability, demand) in cases {
        session.observe_location_environment(LocationEnvironment {
            authorization,
            services_enabled,
            temporarily_unavailable,
        });
        let acquisition = session.location_acquisition();
        assert_eq!(acquisition.availability, availability);
        assert_eq!(acquisition.demand, demand);
        assert_eq!(acquisition.revision, session.snapshot().unwrap().revision);
    }
}

#[test]
fn location_recovery_action_is_selected_by_rust_for_active_recording() {
    let mut session = RideRecordingSession::new(None);
    session.start_gps_only(1_000, None).unwrap();

    let cases = [
        (
            LocationEnvironment {
                authorization: LocationAuthorization::NotDetermined,
                services_enabled: true,
                temporarily_unavailable: false,
            },
            super::LocationRecoveryAction::RequestPermission,
        ),
        (
            LocationEnvironment {
                authorization: LocationAuthorization::Denied,
                services_enabled: true,
                temporarily_unavailable: false,
            },
            super::LocationRecoveryAction::OpenSettings,
        ),
        (
            LocationEnvironment {
                authorization: LocationAuthorization::Always,
                services_enabled: false,
                temporarily_unavailable: false,
            },
            super::LocationRecoveryAction::OpenSettings,
        ),
        (
            LocationEnvironment {
                authorization: LocationAuthorization::Restricted,
                services_enabled: true,
                temporarily_unavailable: false,
            },
            super::LocationRecoveryAction::None,
        ),
    ];

    for (environment, expected_action) in cases {
        session.observe_location_environment(environment);
        assert_eq!(
            session.location_acquisition().recovery_action,
            expected_action
        );
    }

    session.transition(RideEvent::Pause, 2_000).unwrap();
    assert_eq!(
        session.location_acquisition().recovery_action,
        super::LocationRecoveryAction::None
    );
}

#[test]
fn diagnostic_location_generation_rejects_stale_and_reopened_captures() {
    let mut session = RideRecordingSession::new(None);
    session.observe_location_environment(LocationEnvironment {
        authorization: LocationAuthorization::Always,
        services_enabled: true,
        temporarily_unavailable: false,
    });

    session.observe_diagnostic_capture_location(1, true);
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );

    session.observe_diagnostic_capture_location(0, true);
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );

    session.observe_diagnostic_capture_location(1, false);
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);

    session.observe_diagnostic_capture_location(1, true);
    assert_eq!(session.location_acquisition().demand, LocationDemand::Idle);

    session.observe_diagnostic_capture_location(2, true);
    assert_eq!(
        session.location_acquisition().demand,
        LocationDemand::Record
    );
}

#[test]
fn live_speed_prefers_vehicle_zero_then_falls_back_to_fresh_gps() {
    let mut speed = RecordingSpeedState::default();
    speed.observe_vehicle(Some(0), 1_000, 7);
    speed.observe_phone_gps(Some(8.5), 1_100, 7);

    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 1_500),
        Some(super::RecordingSpeed {
            millimetres_per_second: 0,
            source: RecordingSpeedSource::Vehicle,
        })
    );
    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 3_100),
        Some(super::RecordingSpeed {
            millimetres_per_second: 8_500,
            source: RecordingSpeedSource::PhoneGps,
        })
    );
}

#[test]
fn wheel_motion_threshold_is_inclusive_and_uses_signed_speed_magnitude() {
    for (speed, parked) in [
        (499, true),
        (500, false),
        (501, false),
        (-499, true),
        (-500, false),
        (-501, false),
    ] {
        let mut state = RecordingSpeedState::default();
        assert!(state.observe_verified_motion(speed, 1_000, 1_000));
        assert_eq!(state.parked_at(1_000), parked, "speed {speed}");
    }
}

#[test]
fn repeated_vehicle_speed_timestamp_keeps_first_zero_and_does_not_renew_freshness() {
    let mut speed = RecordingSpeedState::default();
    speed.observe_vehicle(Some(0), 1_000, 7);
    speed.observe_vehicle(Some(7_000), 1_000, 7);
    speed.observe_vehicle(None, 1_000, 7);
    speed.observe_phone_gps(Some(8.0), 2_000, 7);

    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 1_001),
        Some(super::RecordingSpeed {
            millimetres_per_second: 0,
            source: RecordingSpeedSource::Vehicle,
        })
    );
    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 3_001),
        Some(super::RecordingSpeed {
            millimetres_per_second: 8_000,
            source: RecordingSpeedSource::PhoneGps,
        }),
        "a duplicate wheel timestamp cannot extend the original observation"
    );
}

#[test]
fn repeated_vehicle_speed_timestamp_keeps_first_moving_value_and_newer_evidence_wins() {
    let mut speed = RecordingSpeedState::default();
    speed.observe_vehicle(Some(7_000), 1_000, 7);
    speed.observe_vehicle(Some(0), 1_000, 7);
    speed.observe_vehicle(None, 1_000, 7);

    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 1_001),
        Some(super::RecordingSpeed {
            millimetres_per_second: 7_000,
            source: RecordingSpeedSource::Vehicle,
        })
    );

    speed.observe_vehicle(Some(9_000), 2_000, 7);
    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 2_001),
        Some(super::RecordingSpeed {
            millimetres_per_second: 9_000,
            source: RecordingSpeedSource::Vehicle,
        })
    );
    speed.observe_vehicle(Some(0), 2_000, 8);
    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 8, 2_001),
        Some(super::RecordingSpeed {
            millimetres_per_second: 0,
            source: RecordingSpeedSource::Vehicle,
        }),
        "a new ride generation may restart its source clock"
    );

    speed.observe_vehicle(Some(3_000), 100, 9);
    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 9, 101),
        Some(super::RecordingSpeed {
            millimetres_per_second: 3_000,
            source: RecordingSpeedSource::Vehicle,
        }),
        "a new ride generation may restart its source clock at a lower timestamp"
    );
}

#[test]
fn live_speed_is_unavailable_when_paused_or_after_ride_generation_changes() {
    let mut speed = RecordingSpeedState::default();
    speed.observe_vehicle(Some(4_000), 1_000, 7);
    speed.observe_phone_gps(Some(2.0), 1_000, 7);

    assert_eq!(
        speed.selected_at(RideLifecycleState::Paused, 7, 1_100),
        None
    );
    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 8, 1_100),
        None
    );
}

#[test]
fn live_speed_preserves_signed_vehicle_direction() {
    let mut speed = RecordingSpeedState::default();
    speed.observe_vehicle(Some(-4_000), 1_000, 7);

    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 1_100),
        Some(super::RecordingSpeed {
            millimetres_per_second: -4_000,
            source: RecordingSpeedSource::Vehicle,
        })
    );
}

#[test]
fn live_speed_ignores_invalid_observations() {
    let mut speed = RecordingSpeedState::default();
    speed.observe_phone_gps(Some(f64::NAN), 1_000, 7);

    assert_eq!(
        speed.selected_at(RideLifecycleState::Active, 7, 1_100),
        None
    );
}
