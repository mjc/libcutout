use cutout_mobile_ffi::{
    MobileMusicMonitor, MobileMusicMonitorStart, MobileMusicPlaybackStateDto,
    music_callback_path_matches, music_playback_title_key,
};

#[test]
fn callback_path_binding_accepts_root_slash_without_relaxing_other_paths() {
    assert!(music_callback_path_matches(String::new(), "/".to_owned()));
    assert!(music_callback_path_matches("/".to_owned(), String::new()));
    assert!(!music_callback_path_matches(
        "/callback".to_owned(),
        "/callback/".to_owned()
    ));
}

#[test]
fn missing_title_binding_preserves_playing_and_unknown_status() {
    assert_eq!(
        music_playback_title_key(MobileMusicPlaybackStateDto::Playing),
        "music.state.playing"
    );
    assert_eq!(
        music_playback_title_key(MobileMusicPlaybackStateDto::Stale),
        "music.state.stale"
    );
}

#[test]
fn music_monitor_binding_resumes_passively_without_ride_state() {
    let monitor = MobileMusicMonitor::new();
    assert!(monitor.is_scene_active());
    assert_eq!(monitor.take_start(), None);
    monitor.request(true);
    assert_eq!(
        monitor.take_start(),
        Some(MobileMusicMonitorStart {
            allow_authorization: true,
        })
    );
    monitor.suspend();
    assert!(!monitor.is_scene_active());
    assert_eq!(monitor.take_start(), None);
    assert!(monitor.resume());
    assert_eq!(
        monitor.take_start(),
        Some(MobileMusicMonitorStart {
            allow_authorization: false,
        })
    );
    assert!(!monitor.resume());
    monitor.cancel();
    assert_eq!(monitor.take_start(), None);
}
