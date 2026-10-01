use super::{RecordingError, RideRecordingSession};
use crate::RideDatabase;

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
