use cutout_core::MonotonicTimestamp;
use cutout_music::{MusicCommand, MusicPlaybackState, spotify::playback::*};

const TRACK: &[u8] = br#"{"device":{"id":"phone","is_restricted":false},"is_playing":true,"progress_ms":1234,"item":{"type":"track","uri":"spotify:track:abc","name":"A track","artists":[{"name":"An artist"}],"duration_ms":90000},"actions":{"disallows":{"skipping_prev":true}}}"#;

#[test]
fn playback_retains_metadata_and_only_advertised_controls() {
    let playback = SpotifyPlayback::parse(TRACK).expect("valid playback");
    assert_eq!(playback.device_id(), Some("phone"));
    let snapshot = playback.snapshot(MusicPlaybackState::Playing, MonotonicTimestamp::new(10));
    let item = snapshot.item().expect("known track");
    assert_eq!(item.title(), Some("A track"));
    assert_eq!(item.artist(), Some("An artist"));
    assert_eq!(snapshot.position_milliseconds(), Some(1234));
    assert!(!snapshot.capabilities().supports(MusicCommand::Previous));
    assert!(snapshot.capabilities().supports(MusicCommand::Next));
    assert!(snapshot.capabilities().supports(MusicCommand::Pause));
    assert!(!snapshot.capabilities().supports(MusicCommand::Play));
    let stale = playback.snapshot(MusicPlaybackState::Stale, MonotonicTimestamp::new(20));
    assert!(!stale.capabilities().supports(MusicCommand::Next));
    assert!(stale.capabilities().supports(MusicCommand::OpenProvider));
}

#[test]
fn no_active_device_preserves_metadata_without_transport() {
    let playback = SpotifyPlayback::parse(TRACK)
        .expect("valid playback")
        .without_active_device();
    assert_eq!(playback.device_id(), None);
    assert!(!playback.is_playing());
    let snapshot = playback.snapshot(MusicPlaybackState::Stopped, MonotonicTimestamp::new(30));
    assert_eq!(
        snapshot.item().expect("retained track").title(),
        Some("A track")
    );
    assert!(!snapshot.capabilities().supports(MusicCommand::Play));
    assert!(!snapshot.capabilities().supports(MusicCommand::Next));
}

#[test]
fn restricted_and_unknown_content_never_invents_track_or_transport() {
    let playback = SpotifyPlayback::parse(br#"{"device":{"id":"phone","is_restricted":true},"is_playing":true,"item":{"type":"ad","uri":"spotify:ad:abc","name":"Ad"}}"#).expect("ad state");
    let snapshot = playback.snapshot(MusicPlaybackState::Playing, MonotonicTimestamp::new(0));
    assert!(snapshot.item().is_none());
    assert!(!snapshot.capabilities().supports(MusicCommand::Pause));
}

#[test]
fn episode_has_show_and_unknown_progress_and_validated_artwork() {
    let playback = SpotifyPlayback::parse(br#"{"device":{"id":"phone"},"is_playing":false,"progress_ms":null,"item":{"type":"episode","uri":"spotify:episode:abc","show":{"name":"Podcast"},"duration_ms":7200000,"images":[{"url":"https://images.example/large","width":640},{"url":"https://images.example/small","width":300}]}}"#).expect("episode");
    assert_eq!(playback.artwork_url(), Some("https://images.example/small"));
    let snapshot = playback.snapshot(MusicPlaybackState::Paused, MonotonicTimestamp::new(0));
    assert_eq!(snapshot.item().expect("episode").artist(), Some("Podcast"));
    assert_eq!(snapshot.duration_milliseconds(), Some(7_200_000));
    assert_eq!(snapshot.position_milliseconds(), None);
    assert!(snapshot.capabilities().supports(MusicCommand::Play));
    for body in [
        br#"{"is_playing":true,"item":{"type":"track","uri":"spotify:track:abc","images":[{"url":"http://evil.test/a"}]}}"#.as_slice(),
        br#"{"is_playing":true,"item":{"type":"track","uri":"spotify:track:abc","images":[{"url":"https://user:password@evil.test/a"}]}}"#.as_slice(),
    ] {
        assert_eq!(SpotifyPlayback::parse(body).expect("metadata").artwork_url(), None);
    }
}

#[test]
fn malformed_and_oversize_payloads_are_rejected_without_raw_error_text() {
    for payload in [
        b"{}".as_slice(),
        b"null",
        b"not JSON",
        br#"{"is_playing":true,"progress_ms":200,"item":{"duration_ms":100}}"#,
    ] {
        assert_eq!(
            SpotifyPlayback::parse(payload).expect_err("invalid"),
            SpotifyPlaybackError::InvalidPayload
        );
    }
    assert_eq!(
        SpotifyPlayback::parse(&vec![b' '; 1_048_577]).expect_err("bounded"),
        SpotifyPlaybackError::InvalidPayload
    );
}

#[test]
fn request_policy_targets_observed_device_and_never_transfers_playback() {
    let session = SpotifyHttpSession::default();
    let read = session.playback_request(10).expect("read");
    assert_eq!(read.method, "GET");
    assert_eq!(
        url::Url::parse(&read.url)
            .expect("url")
            .query_pairs()
            .collect::<Vec<_>>(),
        [("additional_types".into(), "track,episode".into())]
    );
    for (command, method, suffix) in [
        (MusicCommand::Play, "PUT", "play"),
        (MusicCommand::Pause, "PUT", "pause"),
        (MusicCommand::Next, "POST", "next"),
        (MusicCommand::Previous, "POST", "previous"),
    ] {
        let request = session
            .command_request(command, "phone&space value", 10)
            .expect("command");
        let url = url::Url::parse(&request.url).expect("url");
        assert_eq!(request.method, method);
        assert_eq!(url.path(), format!("/v1/me/player/{suffix}"));
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            [("device_id".into(), "phone&space value".into())]
        );
    }
    assert_eq!(
        session.command_request(MusicCommand::OpenProvider, "phone", 10),
        Err(SpotifyPlaybackError::Unavailable)
    );
    assert_eq!(
        session.command_request(MusicCommand::Play, "", 10),
        Err(SpotifyPlaybackError::Unavailable)
    );
}

#[test]
fn request_query_preserves_spaces_pluses_and_percent_escapes() {
    let session = SpotifyHttpSession::default();
    for (device, encoded) in [
        ("phone&space value", "phone%26space%20value"),
        ("phone+value", "phone%2Bvalue"),
        ("phone%2Bvalue", "phone%252Bvalue"),
        ("phone/?#=+% value", "phone%2F%3F%23%3D%2B%25%20value"),
    ] {
        let request = session
            .command_request(MusicCommand::Play, device, 10)
            .expect("command");
        let url = url::Url::parse(&request.url).expect("url");
        assert_eq!(url.query(), Some(format!("device_id={encoded}").as_str()));
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            [("device_id".into(), device.into())]
        );
    }
}

#[test]
fn retry_after_blocks_reads_and_commands_until_monotonic_deadline() {
    let mut session = SpotifyHttpSession::default();
    assert_eq!(
        session.receive_response(429, Some("45"), 0, 100),
        Err(SpotifyPlaybackError::RateLimited {
            retry_after_milliseconds: 45_000
        })
    );
    assert_eq!(
        session.playback_request(200),
        Err(SpotifyPlaybackError::RateLimited {
            retry_after_milliseconds: 44_900
        })
    );
    assert_eq!(
        session.command_request(MusicCommand::Next, "phone", 200),
        Err(SpotifyPlaybackError::RateLimited {
            retry_after_milliseconds: 44_900
        })
    );
    assert!(session.playback_request(45_100).is_ok());
    for (header, delay) in [
        ("NaN", 30_000),
        ("-5", 1_000),
        ("0.125", 1_000),
        ("999999", 86_400_000),
        ("broken", 30_000),
    ] {
        let mut session = SpotifyHttpSession::default();
        assert_eq!(
            session.receive_response(429, Some(header), 0, 0),
            Err(SpotifyPlaybackError::RateLimited {
                retry_after_milliseconds: delay
            })
        );
    }
    assert_eq!(
        session.receive_response(401, None, 0, 0),
        Err(SpotifyPlaybackError::Unauthorized)
    );
    assert_eq!(
        session.receive_response(403, None, 0, 0),
        Err(SpotifyPlaybackError::Forbidden)
    );
    assert_eq!(session.receive_response(204, None, 0, 0), Ok(false));
    assert_eq!(
        session.receive_response(200, None, 1_048_577, 0),
        Err(SpotifyPlaybackError::Unavailable)
    );
}

#[test]
fn overlapping_rate_limits_cannot_shorten_an_existing_provider_deadline() {
    let mut session = SpotifyHttpSession::default();
    assert_eq!(
        session.receive_response(429, Some("90"), 0, 100),
        Err(SpotifyPlaybackError::RateLimited {
            retry_after_milliseconds: 90_000
        })
    );
    assert_eq!(
        session.receive_response(429, Some("1"), 0, 200),
        Err(SpotifyPlaybackError::RateLimited {
            retry_after_milliseconds: 89_900
        })
    );
    assert!(session.playback_request(90_099).is_err());
    assert!(session.playback_request(90_100).is_ok());
}
