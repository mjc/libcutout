//! Direct Rust music operations use the same ride identity and lifecycle owner.
use cutout_application::ride_recording::{
    RideRecordingApplication,
    music::{RideMusicError, RideMusicEventOutcome, RideMusicObservation},
};
use cutout_core::MonotonicTimestamp;
use cutout_music::{
    MusicCapabilities, MusicPlaybackPosition, MusicPlaybackState, MusicProvider,
    MusicRideEventKind, MusicSnapshot,
};
use cutout_ride_maps::RideEvent;

#[test]
fn ride_lifecycle_admission_precedes_music_storage_effects() {
    let application = RideRecordingApplication::in_memory();
    let started = application
        .finish_start(
            application
                .begin_start(100, 1_700_000_000_000, None, None)
                .expect("start admitted"),
        )
        .expect("start completed");
    let observation = RideMusicObservation {
        snapshot: MusicSnapshot::new(
            MusicProvider::AppleMusic,
            "session",
            MusicPlaybackState::Playing,
            None,
            MusicPlaybackPosition::new(None, None).expect("valid position"),
            MonotonicTimestamp::new(200),
            MusicCapabilities::new(),
        )
        .expect("valid provider observation"),
        kind: MusicRideEventKind::Play,
        monotonic_at_ms: 200,
        wall_clock_at_ms: 1_700_000_000_100,
        clock_uncertainty_ms: 5,
    };
    assert!(match application.record_music_event(&observation) {
        Err(RideMusicError::Unavailable) => true,
        _ => false,
    });
    application
        .finish_transition(
            application
                .begin_transition(
                    started.command_token.expect("token exists"),
                    RideEvent::Stop,
                    200,
                    None,
                )
                .expect("stop admitted"),
        )
        .expect("stop completed");
    assert_eq!(
        application
            .record_music_event(&observation)
            .expect("terminal lifecycle is classified before storage")
            .outcome,
        RideMusicEventOutcome::RideNotOpen
    );
}
