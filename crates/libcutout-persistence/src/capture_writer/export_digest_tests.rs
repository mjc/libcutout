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
