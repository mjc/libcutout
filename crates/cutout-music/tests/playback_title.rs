use cutout_music::MusicPlaybackState;

#[test]
fn missing_metadata_title_reports_playback_evidence_instead_of_not_playing() {
    for (state, key) in [
        (MusicPlaybackState::Playing, "music.state.playing"),
        (MusicPlaybackState::Paused, "music.state.paused"),
        (MusicPlaybackState::Stopped, "music.state.stopped"),
        (MusicPlaybackState::Buffering, "music.state.buffering"),
        (MusicPlaybackState::Interrupted, "music.state.interrupted"),
        (
            MusicPlaybackState::Unauthorized,
            "music.state.authorization_required",
        ),
        (MusicPlaybackState::Unavailable, "music.state.unavailable"),
        (MusicPlaybackState::Disconnected, "music.state.disconnected"),
        (MusicPlaybackState::Stale, "music.state.stale"),
    ] {
        assert_eq!(state.fallback_title_key(), key);
    }
}
