use super::*;

#[test]
fn database_export_preserves_exact_multipage_digest_data_and_closed_labels() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let artifact_path = directory.path().join("capture.jsonl");
    let writer = CaptureWriter::start_with_database(
        artifact_path.clone(),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "test",
        None,
        &CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec!["capture_label=ride_start".into()],
        },
        database.clone(),
    )
    .unwrap();
    for at in 1..=65 {
        assert_eq!(
            writer.try_send_record(tests::stationary_record(at)),
            CaptureWriteOutcome::Accepted
        );
    }
    let artifact = writer.finish_exported().unwrap();
    let exported = fs::read(&artifact_path).unwrap();
    assert_eq!(
        artifact.content_digest(),
        hex::encode(Sha256::digest(&exported))
    );
    assert_eq!(
        artifact.status().physical_bytes_written,
        exported.len() as u64
    );
    let decoded =
        cutout_core::PevcapCapture::decode(&exported, cutout_core::PevcapEncoding::Jsonl).unwrap();
    assert_eq!(decoded.records.len(), 65);
    assert!(
        String::from_utf8(exported)
            .unwrap()
            .contains("capture_label=ride_stop")
    );
}

#[test]
fn fresh_btle_export_omits_legacy_music_without_touching_raw_ble() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let writer = CaptureWriter::start_with_database(
        directory.path().join("not-exported.jsonl"),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "test",
        None,
        &CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        },
        database.clone(),
    )
    .unwrap();
    let music = PevcapMusicEvent::new(
        cutout_core::MusicProvider::Spotify,
        "spotify:track:private",
        cutout_core::MonotonicTimestamp::new(1),
        WallClockUnixTimestamp::new(1_700_000_000_001),
        1,
        None,
    )
    .unwrap();
    let mut record = tests::stationary_record(1);
    record.music = Some(music.clone());
    assert_eq!(writer.record_music(music), CaptureWriteOutcome::Accepted);
    assert_eq!(
        writer.try_send_record(record.clone()),
        CaptureWriteOutcome::Accepted
    );
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id, ..
    } = writer.finish_without_export().unwrap()
    else {
        panic!("database finish");
    };
    let path = directory.path().join("share.jsonl");
    let digest = database
        .export_live_capture(live_capture_id, &path)
        .unwrap();
    let bytes = fs::read(path).unwrap();
    let capture =
        cutout_core::PevcapCapture::decode(&bytes, cutout_core::PevcapEncoding::Jsonl).unwrap();
    assert_eq!(capture.music_events, Vec::new());
    record.music = None;
    assert_eq!(capture.records, vec![record]);
    assert_eq!(digest, hex::encode(Sha256::digest(bytes)));
}

#[test]
fn fresh_btle_file_share_has_its_own_digest_and_preserves_original() {
    for encoding in [
        cutout_core::PevcapEncoding::Jsonl,
        cutout_core::PevcapEncoding::Binary,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let music = PevcapMusicEvent::new(
            cutout_core::MusicProvider::AppleMusic,
            "legacy-item",
            cutout_core::MonotonicTimestamp::new(1),
            WallClockUnixTimestamp::new(1_700_000_000_001),
            1,
            None,
        )
        .unwrap();
        let header = PevcapHeader::new(
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "wheel",
            None,
            &[],
            &[],
            None,
            None,
            "test",
            [0; 32],
            &[],
        )
        .unwrap();
        let mut record = tests::stationary_record(1);
        record.music = Some(music.clone());
        let capture = cutout_core::PevcapCapture::new_with_locations_and_music(
            header,
            vec![record.clone()],
            vec![],
            vec![music],
        );
        let original = capture.encode(encoding).unwrap();
        let source = directory.path().join("source.jsonl");
        let destination = directory.path().join("share.jsonl");
        fs::write(&source, &original).unwrap();
        let digest = export_capture_file_without_music(&source, &destination).unwrap();
        let bytes = fs::read(&destination).unwrap();
        let retained = cutout_core::PevcapCapture::decode(&bytes, encoding).unwrap();
        assert_eq!(retained.music_events, Vec::new());
        record.music = None;
        assert_eq!(retained.records, vec![record]);
        assert_eq!(digest, hex::encode(Sha256::digest(&bytes)));
        assert_eq!(fs::read(&source).unwrap(), original);
        assert!(export_capture_file_without_music(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), bytes);
    }
}
