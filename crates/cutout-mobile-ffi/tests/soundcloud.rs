#![allow(clippy::disallowed_macros)]

use cutout_mobile_ffi::{
    MobileMusicCommandDto, MobileMusicProviderDto, MobileMusicProviderInterface,
    MobileMusicProviderLifecycle, MobileMusicTransportOutcome, MobileSoundCloudCommandAdmission,
    music_provider_from_storage, music_provider_profile, soundcloud_unavailable_snapshot,
};

#[test]
fn soundcloud_profile_and_selection_are_rust_owned() {
    let profile = music_provider_profile(MobileMusicProviderDto::SoundCloud);
    assert_eq!(
        profile.interface,
        MobileMusicProviderInterface::AppHandoffOnly
    );
    assert_eq!(profile.storage_key, "soundcloud");
    assert_eq!(profile.title_key, "music.provider.soundcloud");
    assert_eq!(
        profile.unavailable_key.as_deref(),
        Some("music.soundcloud.unavailable")
    );
    assert_eq!(
        music_provider_from_storage(Some(profile.storage_key)),
        MobileMusicProviderDto::SoundCloud
    );
    assert_eq!(
        music_provider_from_storage(Some("spotify".to_owned())),
        MobileMusicProviderDto::Spotify
    );
    assert_eq!(
        music_provider_from_storage(None),
        MobileMusicProviderDto::AppleMusic
    );
    assert_eq!(
        music_provider_from_storage(Some("unknown".to_owned())),
        MobileMusicProviderDto::AppleMusic
    );
    let snapshot = soundcloud_unavailable_snapshot(20);
    assert_eq!(snapshot.provider, MobileMusicProviderDto::SoundCloud);
    assert!(snapshot.capabilities.open_provider);
    assert!(!snapshot.capabilities.play);
    assert_eq!(snapshot.item, None);
}

#[test]
fn handoff_completion_is_owned_by_the_shared_rust_session() {
    let lifecycle = MobileMusicProviderLifecycle::new();
    let generation = lifecycle
        .begin_provider_session()
        .expect("provider generation");
    for command in [
        MobileMusicCommandDto::Play,
        MobileMusicCommandDto::Pause,
        MobileMusicCommandDto::Next,
        MobileMusicCommandDto::Previous,
    ] {
        assert_eq!(
            lifecycle.begin_soundcloud_command(generation, command, 10),
            MobileSoundCloudCommandAdmission::Refused
        );
    }
    let MobileSoundCloudCommandAdmission::Handoff { url, effect } =
        lifecycle.begin_soundcloud_command(generation, MobileMusicCommandDto::OpenProvider, 10)
    else {
        panic!("explicit local handoff");
    };
    assert_eq!(url, "soundcloud://");
    assert_eq!(effect.deadline_ms, 10_010);
    assert_eq!(
        lifecycle
            .finish_transport(generation, effect.id, MobileMusicTransportOutcome::Accepted)
            .outcome,
        Some(MobileMusicTransportOutcome::Accepted)
    );
    assert_eq!(
        lifecycle
            .finish_transport(generation, effect.id, MobileMusicTransportOutcome::Accepted)
            .outcome,
        None
    );
    let replacement = lifecycle.begin_provider_session().expect("replacement");
    assert_ne!(generation, replacement);
    assert_eq!(
        lifecycle.begin_soundcloud_command(generation, MobileMusicCommandDto::OpenProvider, 20),
        MobileSoundCloudCommandAdmission::Unavailable
    );
}

#[test]
fn handoff_timeout_cancellation_and_provider_switch_reject_late_completion() {
    for terminal in [0, 1, 2] {
        let lifecycle = MobileMusicProviderLifecycle::new();
        let generation = lifecycle.begin_provider_session().expect("provider");
        let MobileSoundCloudCommandAdmission::Handoff { effect, .. } =
            lifecycle.begin_soundcloud_command(generation, MobileMusicCommandDto::OpenProvider, 10)
        else {
            panic!("handoff")
        };
        assert_eq!(
            lifecycle.begin_soundcloud_command(generation, MobileMusicCommandDto::OpenProvider, 10),
            MobileSoundCloudCommandAdmission::Unavailable
        );
        match terminal {
            0 => {
                assert_eq!(
                    lifecycle
                        .expire_transport(generation, effect.deadline_ms - 1)
                        .outcome,
                    None
                );
                assert_eq!(
                    lifecycle
                        .expire_transport(generation, effect.deadline_ms)
                        .outcome,
                    Some(MobileMusicTransportOutcome::TimedOut)
                );
            }
            1 => {
                assert_eq!(
                    lifecycle.cancel_transport(generation, effect.id).outcome,
                    Some(MobileMusicTransportOutcome::Cancelled)
                );
            }
            _ => {
                lifecycle.begin_provider_session().expect("replacement");
            }
        }
        assert_eq!(
            lifecycle
                .finish_transport(generation, effect.id, MobileMusicTransportOutcome::Accepted)
                .outcome,
            None
        );
    }
}

#[test]
fn soundcloud_cannot_supply_track_identifiers_to_capture() {
    for policy in [
        cutout_mobile_ffi::MobileMusicHistoryPolicyDto::Disabled,
        cutout_mobile_ffi::MobileMusicHistoryPolicyDto::OpaqueItem,
        cutout_mobile_ffi::MobileMusicHistoryPolicyDto::HumanReadable,
    ] {
        assert_eq!(
            cutout_mobile_ffi::pevcap_music_track_identifier(
                policy,
                MobileMusicProviderDto::SoundCloud,
                "invented-track".to_owned()
            ),
            None
        );
    }
}

#[test]
fn enabled_capture_history_rejects_fabricated_soundcloud_observations() {
    use cutout_mobile_ffi::{
        MobileCaptureWriteOutcomeDto, MobileMusicHistoryPolicyDto, MobilePevcapCaptureBuilder,
        MobilePevcapMusicEventDto, MobileWallClockUnixMillisDto,
    };

    for policy in [
        MobileMusicHistoryPolicyDto::OpaqueItem,
        MobileMusicHistoryPolicyDto::HumanReadable,
    ] {
        let builder = MobilePevcapCaptureBuilder::new(
            MobileWallClockUnixMillisDto { milliseconds: 100 },
            "ios-corebluetooth".to_owned(),
            None,
        );
        assert!(builder.set_music_history_policy(policy));
        let observation = MobilePevcapMusicEventDto {
            provider: MobileMusicProviderDto::SoundCloud,
            track_id: "invented-track".to_owned(),
            monotonic_at_ms: 10,
            wall_clock_unix_ms: 110,
            clock_uncertainty_ms: 0,
            ride_sequence: None,
        };
        assert!(!builder.set_music_context(Some(observation.clone())));
        assert_eq!(
            builder.record_music_event(observation),
            MobileCaptureWriteOutcomeDto::Failed
        );
    }
}

#[test]
fn admitted_soundcloud_player_projection_stays_unavailable_and_ordered() {
    use cutout_mobile_ffi::MobileMusicObservationAdmission;
    use std::sync::Arc;

    let lifecycle = MobileMusicProviderLifecycle::new();
    let request = lifecycle.begin_music_observation();
    let MobileMusicObservationAdmission::Admitted { lease } = request.admission() else {
        panic!("admitted observation");
    };
    let snapshot = soundcloud_unavailable_snapshot(20);
    assert_eq!(
        lifecycle.observe_admitted_player(Arc::clone(&lease), snapshot.clone()),
        Ok(Some(snapshot))
    );
    assert_eq!(
        lifecycle.observe_admitted_player(lease, soundcloud_unavailable_snapshot(19)),
        Ok(None)
    );
    request.release();
}
