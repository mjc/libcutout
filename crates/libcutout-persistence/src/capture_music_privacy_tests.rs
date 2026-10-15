//! End-to-end cleanup of a normally imported route, including durable reopen.
use super::*;

#[test]
fn capture_music_cleanup_preserves_real_derived_ride_and_apple_history_after_reopen() {
    use cutout_core::{MusicProvider as CaptureProvider, PevcapMusicEvent};
    let _guard = test_guard();
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("rides.sqlite");
    let source = directory.path().join("user-import.jsonl");
    let music = PevcapMusicEvent::new(
        CaptureProvider::Spotify,
        "spotify:track:legacy",
        MonotonicTimestamp::new(1),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        0,
        None,
    )
    .unwrap();
    let mut record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![255, 0, 165],
    )
    .with_phone_location(pevcap_phone_location(1, 40.0, Some(3.0)));
    record.music = Some(music.clone());
    let capture = PevcapCapture::new_with_locations_and_music(
        pevcap_header(),
        vec![record],
        vec![],
        vec![music],
    );
    let original = capture.to_jsonl().unwrap();
    std::fs::write(&source, &original).unwrap();
    let database = RideDatabase::open(&database_path).unwrap();
    let preview = database
        .preflight_pevcap(&source, PevcapEncoding::Jsonl)
        .unwrap();
    let receipt = database
        .confirm_pevcap_import(&preview, 1_700_000_000_500)
        .unwrap();
    let ride = receipt.ride_id.unwrap();
    let history_ride = database
        .create_started_live_ride(1_700_000_000_000, 100, None)
        .unwrap();
    database
        .save_music_event(
            history_ride,
            MusicHistoryPolicy::OpaqueItem,
            0,
            music_event(),
        )
        .unwrap();
    let history = database.music_history(history_ride).unwrap();
    let summary = database.summary(ride).unwrap();
    assert_eq!(summary.point_count().as_u64(), 1);
    database.purge_capture_music_metadata(&[]).unwrap();
    let stored = database
        .list_pevcap_captures(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(stored.captures.len(), 1);
    let derivative = stored.captures[0].clone();
    assert_ne!(derivative.artifact_digest, receipt.artifact_digest);
    assert_eq!(derivative.ride_id, Some(ride));
    assert_eq!(derivative.imported_at_milliseconds, 1_700_000_000_500);
    assert_eq!(derivative.record_count, receipt.record_count);
    assert_eq!(derivative.location_count, receipt.location_count);
    assert_eq!(database.music_history(history_ride).unwrap(), history);
    assert!(!receipt.managed_artifact_path.exists());
    assert_eq!(std::fs::read(&source).unwrap(), original.as_bytes());
    database.shutdown().unwrap();
    let database = RideDatabase::open(&database_path).unwrap();
    assert_eq!(
        database
            .list_pevcap_captures(None, QueryLimit::new(10).unwrap())
            .unwrap()
            .captures,
        [derivative]
    );
    assert_eq!(
        database.summary(ride).unwrap().point_count(),
        summary.point_count()
    );
    assert_eq!(
        database.summary(ride).unwrap().distance(),
        summary.distance()
    );
    assert_eq!(database.music_history(history_ride).unwrap(), history);
    database.purge_capture_music_metadata(&[]).unwrap();
    database.shutdown().unwrap();
}
