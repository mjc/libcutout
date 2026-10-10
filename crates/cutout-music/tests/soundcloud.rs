#![allow(clippy::disallowed_macros)]

use cutout_core::{MonotonicTimestamp, WallClockUnixTimestamp};
use cutout_music::soundcloud::{SoundCloudCommandAdmission, admit_command, unavailable_snapshot};
use cutout_music::{
    MusicCapabilities, MusicCommand, MusicHistoryPolicy, MusicItem, MusicPlaybackPosition,
    MusicPlaybackState, MusicProvider, MusicRideEvent, MusicRideEventKind, MusicSnapshot,
    MusicValidationError,
};

#[test]
fn soundcloud_exposes_only_local_app_handoff_without_inventing_playback() {
    let snapshot = unavailable_snapshot(MonotonicTimestamp::new(10));
    assert_eq!(snapshot.provider(), MusicProvider::SoundCloud);
    assert_eq!(snapshot.state(), MusicPlaybackState::Unavailable);
    assert_eq!(snapshot.item(), None);
    assert_eq!(snapshot.position_milliseconds(), None);
    assert_eq!(snapshot.duration_milliseconds(), None);
    assert!(snapshot.capabilities().supports(MusicCommand::OpenProvider));
    for command in [
        MusicCommand::Play,
        MusicCommand::Pause,
        MusicCommand::Previous,
        MusicCommand::Next,
    ] {
        assert!(!snapshot.capabilities().supports(command));
        assert_eq!(admit_command(command), SoundCloudCommandAdmission::Refused);
    }
    let SoundCloudCommandAdmission::Handoff(handoff) = admit_command(MusicCommand::OpenProvider)
    else {
        panic!("explicit app handoff must be admitted");
    };
    assert_eq!(handoff.url(), "soundcloud://");
    for policy in [
        MusicHistoryPolicy::Disabled,
        MusicHistoryPolicy::OpaqueItem,
        MusicHistoryPolicy::HumanReadable,
    ] {
        assert_eq!(
            MusicRideEvent::from_snapshot(
                &snapshot,
                MusicRideEventKind::ItemChanged,
                MonotonicTimestamp::new(10),
                WallClockUnixTimestamp::new(100),
                0,
                policy
            ),
            None
        );
    }
}

#[test]
fn soundcloud_rejects_invented_metadata_state_position_and_controls() {
    let invalid = [
        (
            MusicPlaybackState::Playing,
            None,
            MusicPlaybackPosition::default(),
            MusicCapabilities::new(),
        ),
        (
            MusicPlaybackState::Unavailable,
            Some(MusicItem::new("track", None, None).expect("item")),
            MusicPlaybackPosition::default(),
            MusicCapabilities::new(),
        ),
        (
            MusicPlaybackState::Unavailable,
            None,
            MusicPlaybackPosition::new(Some(1), None).expect("position"),
            MusicCapabilities::new(),
        ),
        (
            MusicPlaybackState::Unavailable,
            None,
            MusicPlaybackPosition::default(),
            MusicCapabilities::new().with(MusicCommand::Play),
        ),
    ];
    for (state, item, position, capabilities) in invalid {
        assert_eq!(
            MusicSnapshot::new(
                MusicProvider::SoundCloud,
                "session",
                state,
                item,
                position,
                MonotonicTimestamp::new(10),
                capabilities
            ),
            Err(MusicValidationError::ProviderObservationUnsupported)
        );
    }
}

#[test]
fn handoff_only_provider_cannot_create_stored_listening_events() {
    assert_eq!(
        MusicRideEvent::new(
            MusicProvider::SoundCloud,
            Some("invented-track".to_owned()),
            None,
            None,
            MusicRideEventKind::Play,
            cutout_music::MusicEventTiming {
                observed_at: None,
                monotonic_at: MonotonicTimestamp::new(10),
                wall_clock_at: WallClockUnixTimestamp::new(100),
                clock_uncertainty_milliseconds: 0,
            }
        ),
        Err(MusicValidationError::ProviderObservationUnsupported)
    );
}
