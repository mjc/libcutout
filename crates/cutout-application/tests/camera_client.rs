use cutout_application::camera::{
    CameraCommand, CameraCommandCompletionError, CameraHttpOrigin,
    CameraMediaDownloadCompletionError, NovatekCameraSessionState,
};
use cutout_protocols::{NovatekHttpOrigin, NovatekR3V1Session, parse_read_only_snapshot};

fn configure(state: &mut NovatekCameraSessionState) {
    let firmware =
        b"<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>";
    let live = b"<LIST><MovieLiveViewLink>rtsp://192.168.1.254/movie</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/photo</PhotoLiveViewLink></LIST>";
    let configuration = b"<Function><Cmd>2001</Cmd><Status>0</Status></Function>";
    let storage = b"<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>";
    let media = br"<LIST><File><NAME>clip.TS</NAME><FPATH>A:\Novatek\Movie\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>";
    let snapshot = parse_read_only_snapshot(firmware, live, configuration, storage, media).unwrap();
    let profile = NovatekR3V1Session::new(
        NovatekHttpOrigin::r3_pro_access_point(),
        snapshot.firmware().as_str(),
        [(2001, 0)],
    )
    .unwrap();
    state.replace_read_only(profile, snapshot);
}

#[test]
fn replacing_evidence_retires_commands_and_downloads_together() {
    let mut state = NovatekCameraSessionState::default();
    configure(&mut state);
    let command = state
        .authorize_command(
            &CameraHttpOrigin {
                address: "192.168.1.254".into(),
                port: 80,
            },
            CameraCommand::StartRecording,
        )
        .unwrap();
    let download = state
        .authorize_media_download(r"A:\Novatek\Movie\clip.TS")
        .unwrap();
    configure(&mut state);
    assert_eq!(
        state.complete_command(&command, b""),
        Err(CameraCommandCompletionError::StaleSession)
    );
    assert_eq!(
        state.complete_media_download(download.operation_id, 42),
        Err(CameraMediaDownloadCompletionError::StaleOperation)
    );
    assert!(state.media_is_current(r"A:\Novatek\Movie\clip.TS", 42));
}

#[test]
fn wrong_length_consumes_download_authorization_without_publishing_media() {
    let mut state = NovatekCameraSessionState::default();
    configure(&mut state);
    let download = state
        .authorize_media_download(r"A:\Novatek\Movie\clip.TS")
        .unwrap();
    assert_eq!(
        state.complete_media_download(download.operation_id, 41),
        Err(CameraMediaDownloadCompletionError::SizeMismatch {
            expected: 42,
            actual: 41
        })
    );
    assert_eq!(
        state.complete_media_download(download.operation_id, 42),
        Err(CameraMediaDownloadCompletionError::StaleOperation)
    );
}
