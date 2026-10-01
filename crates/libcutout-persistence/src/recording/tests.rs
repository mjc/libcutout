use super::{RecordingError, RideRecordingSession};
use crate::RideDatabase;
use cutout_ride_maps::{RideEvent, RideLifecycleState};

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
