use super::*;
use crate::{
    CaptureMetadata, CaptureWriteOutcome, CaptureWriter, CaptureWriterFinish, LiveCaptureId,
    LiveCaptureIntegrity, QueryLimit, RideDatabase, SavedDatabaseCapture,
};
use cutout_core::{
    CaptureOrigin, GattChannel, MonotonicTimestamp, PevcapHeader, PevcapRecord,
    WallClockUnixTimestamp,
};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::task::{Context, Poll, Waker};

fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    let waker = Waker::noop();
    future.poll(&mut Context::from_waker(waker))
}
fn ready<F: Future>(future: F) -> F::Output {
    match poll_once(Box::pin(future).as_mut()) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test transport unexpectedly pending"),
    }
}

#[derive(Default)]
struct Transport {
    uploads: Mutex<Vec<(String, Vec<u8>)>>,
    lookup: Mutex<Option<CaptureSubmissionReceipt>>,
    failure: Mutex<Option<CaptureSubmissionTransferError>>,
    deleted: Mutex<Vec<String>>,
    pause: AtomicBool,
    pause_lookup: AtomicBool,
    pause_delete: AtomicBool,
}
impl CaptureSubmissionTransport for Transport {
    fn upload<'a>(
        &'a self,
        operation_id: &'a str,
        _token: &'a str,
        metadata: &'a CaptureSubmissionMetadata,
        mut payload: CaptureSubmissionPayload,
    ) -> CaptureSubmissionFuture<'a, CaptureSubmissionReceipt> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 64];
                let count = payload
                    .read(&mut chunk)
                    .map_err(|_| CaptureSubmissionTransferError::InvalidCapture)?;
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..count]);
                payload.report_upload_progress();
            }
            self.uploads
                .lock()
                .unwrap()
                .push((operation_id.to_owned(), bytes));
            std::future::poll_fn(|_| {
                if self.pause.load(Ordering::Acquire) {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            if let Some(error) = self.failure.lock().unwrap().take() {
                return Err(error);
            }
            Ok(CaptureSubmissionReceipt {
                operation_id: operation_id.to_owned(),
                sha256: metadata.sha256.clone(),
                bytes: metadata.bytes,
                received_at_milliseconds: 1_700_000_002_000,
            })
        })
    }
    fn receipt<'a>(
        &'a self,
        _operation_id: &'a str,
        _token: &'a str,
    ) -> CaptureSubmissionFuture<'a, Option<CaptureSubmissionReceipt>> {
        Box::pin(async move {
            std::future::poll_fn(|_| {
                if self.pause_lookup.load(Ordering::Acquire) {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            Ok(self.lookup.lock().unwrap().clone())
        })
    }
    fn delete<'a>(
        &'a self,
        operation_id: &'a str,
        _token: &'a str,
    ) -> CaptureSubmissionFuture<'a, ()> {
        Box::pin(async move {
            self.deleted.lock().unwrap().push(operation_id.to_owned());
            std::future::poll_fn(|_| {
                if self.pause_delete.load(Ordering::Acquire) {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            if let Some(error) = self.failure.lock().unwrap().take() {
                Err(error)
            } else {
                Ok(())
            }
        })
    }
}

const ACCOUNT: &str = "3a61d30c-669a-4a98-8b76-ddbb2b766e14";

fn recording(database: &RideDatabase, directory: &Path) -> CaptureWriter {
    let writer = CaptureWriter::start_with_database(
        directory.join("private-original.jsonl"),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "unsupported-wheel",
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
    let record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    assert_eq!(
        writer.try_send_record(record),
        CaptureWriteOutcome::Accepted
    );
    writer
}

fn finished(writer: CaptureWriter) -> (LiveCaptureId, Box<SavedDatabaseCapture>) {
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id,
        receipt,
        ..
    } = writer.finish_without_export().unwrap()
    else {
        panic!("database completion required");
    };
    (live_capture_id, receipt)
}

fn retain(database: &RideDatabase, receipt: &SavedDatabaseCapture, origin: CaptureOrigin) {
    database
        .retain_finished_database_capture(
            receipt,
            origin,
            Some("Unverified advertisement"),
            WallClockUnixTimestamp::new(1_700_000_001_000),
        )
        .unwrap();
}

fn gps_recording(database: &RideDatabase, directory: &Path) -> LiveCaptureId {
    let writer = recording(database, directory);
    assert_eq!(
        writer.record_location(
            cutout_core::PevcapLocationSample::new(
                MonotonicTimestamp::new(10),
                cutout_core::PevcapPhoneLocation {
                    wall_clock_unix_ms: 1_700_000_000_010,
                    latitude_degrees: 39.7,
                    longitude_degrees: -104.9,
                    altitude_meters: 1.0,
                    horizontal_accuracy_meters: Some(3.0),
                    vertical_accuracy_meters: None,
                    speed_meters_per_second: None,
                    speed_accuracy_meters_per_second: None,
                    course_degrees: None,
                    course_accuracy_degrees: None,
                },
                None,
                None,
            )
            .unwrap()
        ),
        CaptureWriteOutcome::Accepted
    );
    let (id, receipt) = finished(writer);
    retain(database, &receipt, CaptureOrigin::Manual);
    id
}

#[test]
fn capture_submission_gps_snapshot_prepare_recovers_after_publication_before_status_insert() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let id = gps_recording(&database, directory.path());
    let source = CaptureSubmissionSource::LiveCapture(id);
    let snapshot = directory.path().join("snapshot.jsonl");
    database.export_live_capture(id, &snapshot).unwrap();
    let bytes = std::fs::read(&snapshot).unwrap();
    let preview = database
        .preflight_pevcap(&snapshot, PevcapEncoding::Jsonl)
        .unwrap();
    assert_eq!(preview.location_count(), 1);
    let source_artifact_id =
        source_record_artifact_id(&database.submission_source(source).unwrap()).unwrap();
    let receipt = database
        .confirm_pevcap_publication(
            &preview,
            1_700_000_001_000,
            &CapturePublication::SubmissionSnapshot { source_artifact_id },
        )
        .unwrap();
    assert!(receipt.ride_id.is_none());
    assert_eq!(receipt.location_count, 0);
    assert!(receipt.recording.is_none());
    assert!(database.capture_submission(source).unwrap().is_none());
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&database_path).unwrap();
    // The derivative committed, but its Send status did not: retry must reuse those bytes.
    let prepared = reopened.prepare_capture_submission(source).unwrap();
    assert_eq!(prepared.phase, CaptureSubmissionPhase::Prepared);
    assert!(prepared.categories.location);
    assert_eq!(prepared.metadata.sha256, receipt.artifact_digest);
    assert_eq!(
        reopened.prepare_capture_submission(source).unwrap(),
        prepared
    );
    assert_eq!(
        reopened
            .list_rides(None, QueryLimit::new(10).unwrap())
            .unwrap()
            .rides(),
        []
    );
    let recovered = directory.path().join("recovered.jsonl");
    reopened
        .export_pevcap_capture(&prepared.metadata.sha256, &recovered)
        .unwrap();
    assert_eq!(std::fs::read(recovered).unwrap(), bytes);
    // Ordinary GPS import validation must not reinterpret this derivative as a derived ride.
    assert!(matches!(
        reopened.confirm_pevcap_import(&preview, 1_700_000_002_000),
        Err(StorageError::PevcapPreviewChanged)
    ));
}

#[test]
fn capture_submission_reuses_existing_gps_import_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let id = gps_recording(&database, directory.path());
    let snapshot = directory.path().join("snapshot.jsonl");
    database.export_live_capture(id, &snapshot).unwrap();
    let preview = database
        .preflight_pevcap(&snapshot, PevcapEncoding::Jsonl)
        .unwrap();
    let imported = database
        .confirm_pevcap_import(&preview, 1_700_000_001_000)
        .unwrap();
    let ride_id = imported
        .ride_id
        .expect("GPS import must retain its derived ride");
    assert_eq!(imported.location_count, 1);

    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    assert!(prepared.categories.location);
    assert_eq!(prepared.metadata.sha256, imported.artifact_digest);
    let rides = database
        .list_rides(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(rides.rides().len(), 1);
    assert_eq!(rides.rides()[0].id(), ride_id);
    let duplicate = database
        .confirm_pevcap_import(&preview, 1_700_000_002_000)
        .unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.ride_id, Some(ride_id));
    assert_eq!(duplicate.location_count, 1);
}

#[test]
fn capture_submission_prepares_unknown_decoder_without_gps_and_reuses_durable_identity() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    assert!(database.capture_submission(source).unwrap().is_none());
    let prepared = database.prepare_capture_submission(source).unwrap();
    assert_eq!(prepared.phase, CaptureSubmissionPhase::Prepared);
    assert_eq!(prepared.metadata.origin, CaptureSubmissionOrigin::Manual);
    assert_eq!(
        prepared.metadata.provenance.source_artifact_id,
        receipt.artifact_id().to_string()
    );
    assert_eq!(
        prepared.metadata.provenance.live_capture_id,
        Some(id.to_string())
    );
    assert_eq!(prepared.metadata.provenance.source_sha256, None);
    assert!(prepared.metadata.bytes > 0);
    assert_eq!(prepared.metadata.sha256.len(), 64);
    assert!(prepared.account_id.is_none());
    assert!(prepared.receipt.is_none());
    assert_eq!(
        database.prepare_capture_submission(source).unwrap(),
        prepared
    );
    assert_eq!(
        database
            .capture_submission(CaptureSubmissionSource::RecordedArtifact(
                receipt.artifact_id()
            ))
            .unwrap(),
        Some(prepared.clone())
    );
    database.shutdown().unwrap();
    let reopened = RideDatabase::open(&database_path).unwrap();
    assert_eq!(reopened.capture_submission(source).unwrap(), Some(prepared));
    assert!(
        !directory.path().join("private-original.jsonl").exists(),
        "preparation never invents an original export"
    );
}

#[test]
fn capture_submission_rejects_active_and_missing_consumed_writer_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "active-wheel",
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
    let active = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    assert!(matches!(
        database.prepare_capture_submission(CaptureSubmissionSource::LiveCapture(active)),
        Err(CaptureSubmissionError::NotFinished)
    ));
    assert!(
        database
            .capture_submission(CaptureSubmissionSource::LiveCapture(active))
            .unwrap()
            .is_none()
    );
    let writer = recording(&database, directory.path());
    writer.flush().unwrap();
    let (id, receipt) = finished(writer);
    assert!(matches!(
        database.prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id)),
        Err(CaptureSubmissionError::ProvenanceUnavailable)
    ));
    assert!(
        database
            .capture_submission(CaptureSubmissionSource::LiveCapture(id))
            .unwrap()
            .is_none()
    );
    retain(&database, &receipt, CaptureOrigin::Manual);
    assert!(
        database
            .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
            .is_ok(),
        "retry uses actual consumed-writer publication"
    );
}

#[test]
fn capture_submission_rejects_recovered_interruption_without_promoting_integrity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "interrupted-wheel",
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
    let id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    database.shutdown().unwrap();
    let reopened = RideDatabase::open(&path).unwrap();
    assert!(matches!(
        reopened.prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id)),
        Err(CaptureSubmissionError::NotFinished)
    ));
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(history.captures[0].integrity, LiveCaptureIntegrity::Unknown);
    assert!(history.captures[0].recording.is_none());
}

fn recovered_nonempty_interruption(directory: &Path) -> (PathBuf, RideDatabase, LiveCaptureId) {
    let path = directory.join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "interrupted-wheel",
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
    let id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    let record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    database
        .append_live_capture_event(
            id,
            crate::LiveCaptureEventKind::Notification,
            1,
            None,
            None,
            record.to_jsonl_line().unwrap().into_bytes(),
        )
        .unwrap();
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    (path, reopened, id)
}

#[test]
fn capture_submission_can_prepare_nonempty_recovered_interruption_with_stable_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let (path, reopened, id) = recovered_nonempty_interruption(directory.path());
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = reopened.prepare_capture_submission(source).unwrap();
    assert_eq!(prepared.phase, CaptureSubmissionPhase::Prepared);
    assert_eq!(prepared.metadata.origin, CaptureSubmissionOrigin::Unknown);
    assert_eq!(
        prepared.metadata.integrity,
        CaptureSubmissionIntegrity::Unknown
    );
    let live_capture_id = id.to_string();
    assert_eq!(
        prepared.metadata.provenance.live_capture_id.as_deref(),
        Some(live_capture_id.as_str())
    );
    assert_eq!(
        prepared.metadata.provenance.source_artifact_id, live_capture_id,
        "an unpublished interruption uses its retained durable capture identity"
    );
    assert!(
        prepared.receipt.is_none(),
        "preparation is not transmission"
    );
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(history.captures.len(), 1);
    assert_eq!(
        history.captures[0].submission.source_artifact_id,
        live_capture_id
    );
    assert_eq!(
        history.captures[0].submission.source,
        CaptureSubmissionSource::LiveCapture(id)
    );
    assert_eq!(
        history.captures[0]
            .submission
            .status
            .as_ref()
            .map(|status| status.operation_id.as_str()),
        Some(prepared.operation_id.as_str())
    );
    let published_history = reopened
        .list_pevcap_captures(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(published_history.captures.len(), 1);
    assert_eq!(
        published_history.captures[0].submission.source_artifact_id,
        prepared.metadata.provenance.source_artifact_id,
        "published history advertises the identity used by the durable Send status"
    );
    assert_eq!(
        published_history.captures[0]
            .submission
            .status
            .as_ref()
            .map(|status| status.operation_id.as_str()),
        Some(prepared.operation_id.as_str()),
        "status lookup through the advertised ID must reach this operation"
    );
    let operation = prepared.operation_id;
    reopened.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    let prepared_again = reopened.prepare_capture_submission(source).unwrap();
    assert_eq!(prepared_again.operation_id, operation);
    assert_eq!(
        prepared_again.metadata.provenance,
        prepared.metadata.provenance
    );
}

#[test]
fn recovered_interruption_history_alias_round_trips_evidence_and_prepares_same_operation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "interrupted-wheel",
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
    let id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    let record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    database
        .append_live_capture_event(
            id,
            crate::LiveCaptureEventKind::Notification,
            1,
            None,
            None,
            record.to_jsonl_line().unwrap().into_bytes(),
        )
        .unwrap();
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(10).unwrap())
        .unwrap();
    let alias = CaptureSubmissionSource::RecordedArtifact(
        CaptureArtifactId::parse(&history.captures[0].submission.source_artifact_id).unwrap(),
    );
    assert_eq!(
        reopened.capture_evidence(alias).unwrap(),
        CaptureAttachments::default(),
        "history identity must resolve the retained interruption after reopen"
    );
    let edited = CaptureAttachments {
        note: "reviewed after recovery".into(),
        screenshots: Vec::new(),
    };
    assert_eq!(
        reopened
            .save_capture_evidence(alias, edited.clone())
            .unwrap(),
        edited
    );

    let from_history = reopened.prepare_capture_submission(alias).unwrap();
    let from_live = reopened
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    assert_eq!(from_history.operation_id, from_live.operation_id);
    assert_eq!(
        from_history.metadata.provenance,
        from_live.metadata.provenance
    );
    assert_eq!(
        from_history
            .metadata
            .evidence
            .as_ref()
            .map(|manifest| manifest.note_bytes),
        Some(u32::try_from("reviewed after recovery".len()).unwrap())
    );
}

#[test]
fn recorded_artifact_file_resolution_precedes_live_id_alias_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "interrupted-wheel",
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
    let interrupted_id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    let record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    database
        .append_live_capture_event(
            interrupted_id,
            crate::LiveCaptureEventKind::Notification,
            1,
            None,
            None,
            record.to_jsonl_line().unwrap().into_bytes(),
        )
        .unwrap();

    let (recorded_id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared_recording = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(recorded_id))
        .unwrap();
    database.shutdown().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO pevcap_recordings
                (artifact_id, artifact_digest, origin, advertised_name,
                 platform_identifier, started_at_ms, model, model_verification)
             VALUES (?1, ?2, 'manual', NULL, 'published-wheel', 1234, NULL, NULL)",
            rusqlite::params![
                interrupted_id.to_string(),
                prepared_recording.metadata.sha256
            ],
        )
        .unwrap();

    let alias = CaptureSubmissionSource::RecordedArtifact(
        CaptureArtifactId::parse(&interrupted_id.to_string()).unwrap(),
    );
    assert!(matches!(
        source_record(&connection, alias).unwrap(),
        Some(SourceRecord::File(_))
    ));
}

#[test]
fn imported_capture_without_recording_keeps_evidence_and_send_receipt_after_retirement() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let source_path = directory.path().join("imported.jsonl");
    let (database, artifact_digest) =
        import_capture_without_recording(&database_path, &source_path);
    let digest = PevcapDigest::parse(&artifact_digest).unwrap();
    let source = CaptureSubmissionSource::PevcapDigest(digest);

    let history = database
        .list_pevcap_captures(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(history.captures.len(), 1);
    assert!(history.captures[0].recording.is_none());
    assert_eq!(
        history.captures[0].submission.source,
        CaptureSubmissionSource::PevcapDigest(digest)
    );

    assert_eq!(
        database.capture_evidence(source).unwrap(),
        CaptureAttachments::default()
    );
    let evidence = CaptureAttachments {
        note: "legacy imported evidence".into(),
        screenshots: Vec::new(),
    };
    let expected_note_bytes = u32::try_from(evidence.note.len()).unwrap();
    assert_eq!(
        database
            .save_capture_evidence(source, evidence.clone())
            .unwrap(),
        evidence
    );
    let prepared = database.prepare_capture_submission(source).unwrap();
    assert_eq!(prepared.metadata.origin, CaptureSubmissionOrigin::Unknown);
    assert_eq!(
        prepared.metadata.integrity,
        CaptureSubmissionIntegrity::Unknown
    );
    assert_eq!(
        prepared.metadata.provenance.source_artifact_id,
        legacy_capture_artifact_id(&artifact_digest)
            .unwrap()
            .to_string()
    );
    assert_eq!(
        prepared.metadata.provenance.source_sha256.as_deref(),
        Some(artifact_digest.as_str())
    );
    assert_eq!(
        prepared
            .metadata
            .evidence
            .as_ref()
            .map(|manifest| manifest.note_bytes),
        Some(expected_note_bytes)
    );
    let transport = Transport::default();
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    assert!(sent.receipt.is_some());
    assert!(
        String::from_utf8_lossy(&transport.uploads.lock().unwrap()[0].1)
            .contains("legacy imported evidence"),
        "saved evidence must be part of the frozen transfer before local retirement"
    );
    database.shutdown().unwrap();

    let reopened = retire_capture_submission(&database_path);
    assert_retained_import_receipt(&reopened, source, &prepared, &sent, expected_note_bytes);
}

fn import_capture_without_recording(
    database_path: &Path,
    source_path: &Path,
) -> (RideDatabase, String) {
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "legacy-wheel",
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
    let record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    let mut bytes = header.to_jsonl_line().unwrap().into_bytes();
    bytes.push(b'\n');
    bytes.extend_from_slice(&record.to_jsonl_line().unwrap().into_bytes());
    bytes.push(b'\n');
    std::fs::write(source_path, &bytes).unwrap();

    let database = RideDatabase::open(database_path).unwrap();
    let preview = database
        .preflight_pevcap(source_path, PevcapEncoding::Jsonl)
        .unwrap();
    let imported = database
        .confirm_pevcap_import(&preview, 1_700_000_001_000)
        .unwrap();
    (database, imported.artifact_digest)
}

fn retire_capture_submission(database_path: &Path) -> RideDatabase {
    let mut connection = Connection::open(database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let mut cursor = None;
    for _ in 0..4 {
        if cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap()
            == crate::storage::normalization::BatchState::Finished
        {
            break;
        }
    }
    drop(connection);

    RideDatabase::open(database_path).unwrap()
}

fn assert_retained_import_receipt(
    reopened: &RideDatabase,
    source: CaptureSubmissionSource,
    prepared: &CaptureSubmissionStatus,
    sent: &CaptureSubmissionStatus,
    expected_note_bytes: u32,
) {
    let status = reopened.capture_submission(source).unwrap().unwrap();
    assert_eq!(status.operation_id, prepared.operation_id);
    assert_eq!(status.phase, CaptureSubmissionPhase::Sent);
    assert_eq!(
        status.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
    assert_eq!(status.receipt, sent.receipt);
    assert_eq!(
        status
            .metadata
            .evidence
            .as_ref()
            .map(|manifest| manifest.note_bytes),
        Some(expected_note_bytes),
        "the retained receipt keeps the disclosure manifest after payload retirement"
    );
    let history = reopened
        .list_pevcap_captures(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(history.captures.len(), 1);
    assert_eq!(
        history.captures[0].submission.source_artifact_id,
        status.metadata.provenance.source_artifact_id
    );
    assert_eq!(
        history.captures[0]
            .submission
            .status
            .as_ref()
            .and_then(|status| status.receipt.as_ref()),
        sent.receipt.as_ref()
    );
}

#[test]
fn pevcap_digest_resolves_the_prepared_live_owner_and_retained_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "interrupted-wheel",
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
    let id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    let record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    database
        .append_live_capture_event(
            id,
            crate::LiveCaptureEventKind::Notification,
            1,
            None,
            None,
            record.to_jsonl_line().unwrap().into_bytes(),
        )
        .unwrap();
    database.shutdown().unwrap();
    let database = RideDatabase::open(&database_path).unwrap();
    let evidence = CaptureAttachments {
        note: "interrupted capture evidence".into(),
        screenshots: Vec::new(),
    };
    let live_source = CaptureSubmissionSource::LiveCapture(id);
    database
        .save_capture_evidence(live_source, evidence.clone())
        .unwrap();
    let prepared = database.prepare_capture_submission(live_source).unwrap();
    let operation_id = prepared.operation_id.clone();
    let digest = PevcapDigest::parse(
        &prepared
            .metadata
            .evidence
            .as_ref()
            .expect("saved evidence records the canonical source digest")
            .capture_sha256,
    )
    .unwrap();
    let digest_source = CaptureSubmissionSource::PevcapDigest(digest);
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&database_path).unwrap();
    assert_eq!(reopened.capture_evidence(digest_source).unwrap(), evidence);
    let status = reopened.capture_submission(digest_source).unwrap().unwrap();
    assert_eq!(status.operation_id, operation_id);
    let prepared_again = reopened.prepare_capture_submission(digest_source).unwrap();
    assert_eq!(prepared_again.operation_id, operation_id);
    assert_eq!(prepared_again.metadata.evidence, prepared.metadata.evidence);

    let transport = Transport::default();
    let sent =
        ready(reopened.send_capture_submission(&operation_id, ACCOUNT, "secret", &transport))
            .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    reopened.shutdown().unwrap();

    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let mut cursor = None;
    for _ in 0..4 {
        if cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap()
            == crate::storage::normalization::BatchState::Finished
        {
            break;
        }
    }
    drop(connection);

    let retired = RideDatabase::open(&database_path).unwrap();
    let retained = retired.capture_submission(digest_source).unwrap().unwrap();
    assert_eq!(retained.operation_id, operation_id);
    assert_eq!(retained.receipt, sent.receipt);
    assert_eq!(
        retained.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
}

#[test]
fn pevcap_digest_rejects_ambiguous_durable_submission_owners() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    database.shutdown().unwrap();

    let connection = Connection::open(&database_path).unwrap();
    let mut status: serde_json::Value = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&prepared.operation_id],
            |row| row.get::<_, String>(0),
        )
        .map(|json| serde_json::from_str(&json).unwrap())
        .unwrap();
    let second_source = "00000000-0000-0000-0000-0000000000f1";
    let second_operation = "00000000-0000-0000-0000-0000000000f2";
    status["operation_id"] = serde_json::json!(second_operation);
    status["metadata"]["provenance"]["source_artifact_id"] = serde_json::json!(second_source);
    connection
        .execute(
            "INSERT INTO capture_submissions
                (source_artifact_id, operation_id, snapshot_digest, evidence_json, status_json)
             SELECT ?1, ?2, snapshot_digest, evidence_json, ?3
             FROM capture_submissions WHERE operation_id = ?4",
            rusqlite::params![
                second_source,
                second_operation,
                status.to_string(),
                prepared.operation_id
            ],
        )
        .unwrap();

    let digest = PevcapDigest::parse(&prepared.metadata.sha256).unwrap();
    assert!(matches!(
        source_record(&connection, CaptureSubmissionSource::PevcapDigest(digest)),
        Err(StorageError::InvalidStoredValue {
            field: "ambiguous capture submission source identity",
            ..
        })
    ));
}

#[test]
fn digest_alias_keeps_live_owner_and_evidence_after_prepared_status_is_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "interrupted-wheel",
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
    let id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    let record = PevcapRecord::inbound_notification(
        MonotonicTimestamp::new(1),
        GattChannel::from_bytes([0x11; 16]),
        GattChannel::from_bytes([0x22; 16]),
        vec![0xff, 0x00, 0xa5],
    );
    database
        .append_live_capture_event(
            id,
            crate::LiveCaptureEventKind::Notification,
            1,
            None,
            None,
            record.to_jsonl_line().unwrap().into_bytes(),
        )
        .unwrap();
    database.shutdown().unwrap();

    let database = RideDatabase::open(&database_path).unwrap();
    let live_source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(live_source).unwrap();
    let digest = PevcapDigest::parse(
        prepared
            .metadata
            .evidence
            .as_ref()
            .map_or(&prepared.metadata.sha256, |manifest| {
                &manifest.capture_sha256
            }),
    )
    .unwrap();
    let digest_source = CaptureSubmissionSource::PevcapDigest(digest);
    let edited = CaptureAttachments {
        note: "edited through canonical digest".into(),
        screenshots: Vec::new(),
    };
    assert_eq!(
        database
            .save_capture_evidence(digest_source, edited.clone())
            .unwrap(),
        edited
    );
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&database_path).unwrap();
    assert_eq!(reopened.capture_evidence(digest_source).unwrap(), edited);
    let prepared_again = reopened.prepare_capture_submission(digest_source).unwrap();
    assert_eq!(
        prepared_again.metadata.provenance.source_artifact_id,
        id.to_string(),
        "re-preparation must keep the original live identity after replacing Prepared status"
    );
    assert_eq!(
        prepared_again
            .metadata
            .provenance
            .live_capture_id
            .as_deref(),
        Some(id.to_string().as_str())
    );
    assert_eq!(
        prepared_again
            .metadata
            .evidence
            .as_ref()
            .map(|manifest| manifest.note_bytes),
        Some(u32::try_from(edited.note.len()).unwrap())
    );
}

#[test]
fn digest_alias_keeps_published_live_context_through_receipt_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    let source_artifact_id = receipt.artifact_id();
    retain(&database, &receipt, CaptureOrigin::Manual);
    let initial = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let digest = PevcapDigest::parse(&initial.metadata.sha256).unwrap();
    let source = CaptureSubmissionSource::PevcapDigest(digest);
    let evidence = CaptureAttachments {
        note: "published context reviewed through digest".into(),
        screenshots: Vec::new(),
    };
    database
        .save_capture_evidence(source, evidence.clone())
        .unwrap();

    let prepared = database.prepare_capture_submission(source).unwrap();
    assert_eq!(
        prepared.metadata.provenance.source_artifact_id,
        source_artifact_id.to_string()
    );
    assert_eq!(
        prepared.metadata.provenance.live_capture_id.as_deref(),
        Some(id.to_string().as_str())
    );
    assert_eq!(prepared.metadata.origin, CaptureSubmissionOrigin::Manual);
    assert_eq!(
        prepared.metadata.integrity,
        CaptureSubmissionIntegrity::Complete
    );
    assert_eq!(
        prepared
            .metadata
            .evidence
            .as_ref()
            .map(|manifest| manifest.note_bytes),
        Some(u32::try_from(evidence.note.len()).unwrap())
    );

    let transport = Transport::default();
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    database.shutdown().unwrap();

    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let mut cursor = None;
    for _ in 0..4 {
        if cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap()
            == crate::storage::normalization::BatchState::Finished
        {
            break;
        }
    }
    let retained_events: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM live_capture_events WHERE capture_id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        retained_events, 0,
        "receipt cleanup retires the live event payload"
    );
    drop(connection);

    let reopened = RideDatabase::open(&database_path).unwrap();
    let retained = reopened.capture_submission(source).unwrap().unwrap();
    assert_eq!(retained.operation_id, prepared.operation_id);
    assert_eq!(retained.receipt, sent.receipt);
    assert_eq!(
        retained.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
}

#[test]
fn capture_submission_still_rejects_an_active_live_capture() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "active-wheel",
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
    let id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    assert!(matches!(
        database.prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id)),
        Err(CaptureSubmissionError::NotFinished)
    ));
}

#[test]
fn capture_submission_rejects_corrupt_interrupted_bytes_without_removing_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let header = PevcapHeader::new(
        WallClockUnixTimestamp::new(1234),
        "corrupt-wheel",
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
    let id = database
        .begin_live_capture(header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    database
        .append_live_capture_event(
            id,
            crate::LiveCaptureEventKind::Notification,
            1,
            None,
            None,
            b"not a PEVCAP event".to_vec(),
        )
        .unwrap();
    database.shutdown().unwrap();

    let reopened = RideDatabase::open(&path).unwrap();
    assert!(matches!(
        reopened.prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id)),
        Err(CaptureSubmissionError::InvalidCapture)
    ));
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(history.captures[0].event_count, 1);
}

#[test]
fn capture_submission_preserves_automatic_origin_as_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Automatic);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    assert_eq!(prepared.metadata.origin, CaptureSubmissionOrigin::Automatic);
    assert_eq!(prepared.phase, CaptureSubmissionPhase::Prepared);
    assert!(
        prepared.receipt.is_none(),
        "preparation is never transmission"
    );
    let history = database
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(
        history.captures[0].integrity,
        LiveCaptureIntegrity::Complete
    );
}

#[test]
fn capture_submission_explicit_send_retains_raw_bytes_and_validates_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    assert!(prepared.categories.device_identifiers);
    assert!(prepared.categories.annotations);
    assert!(!prepared.categories.location);
    let transport = Transport::default();
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    assert_eq!(sent.account_id.as_deref(), Some(ACCOUNT));
    let uploads = transport.uploads.lock().unwrap();
    assert_eq!(uploads.len(), 1);
    let payload = &uploads[0].1;
    let capture = cutout_core::PevcapCapture::decode(payload, PevcapEncoding::Jsonl).unwrap();
    assert_eq!(capture.records[0].bytes, vec![0xff, 0x00, 0xa5]);

    assert!(String::from_utf8_lossy(payload).contains("capture_label=ride_stop"));
    assert_eq!(
        sent.receipt.as_ref().unwrap().sha256,
        prepared.metadata.sha256
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep receipt validation, bounded cleanup, reopen, and remote deletion in one lifecycle regression."
)]
fn matching_receipt_authorizes_bounded_reopenable_local_payload_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    let source_artifact_id = receipt.artifact_id();
    retain(&database, &receipt, CaptureOrigin::Manual);
    database
        .save_capture_evidence(
            CaptureSubmissionSource::LiveCapture(id),
            CaptureAttachments {
                note: "private attachment note".into(),
                screenshots: vec![cutout_core::CaptureScreenshot {
                    jpeg: vec![0xff, 0xd8, 0xff, 0xd9],
                }],
            },
        )
        .unwrap();
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(
        sent.local_payload_state,
        CaptureSubmissionLocalPayloadState::RemovalPending
    );
    database.shutdown().unwrap();

    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let mut cursor = None;
    for _ in 0..4 {
        if cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap()
            == crate::storage::normalization::BatchState::Finished
        {
            break;
        }
    }
    let status_json: String = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&prepared.operation_id],
            |row| row.get(0),
        )
        .unwrap();
    let status = decode_status(&status_json).unwrap();
    assert_cleanup_retains_receipt_and_catalog(
        &connection,
        &status,
        id,
        &source_artifact_id,
        &prepared,
    );
    drop(connection);

    let reopened = RideDatabase::open(&database_path).unwrap();
    let recovered = reopened
        .capture_submission(CaptureSubmissionSource::RecordedArtifact(
            source_artifact_id,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(
        recovered.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
    reopened
        .capture_evidence(CaptureSubmissionSource::RecordedArtifact(
            source_artifact_id,
        ))
        .expect("evidence lookup retains receipt provenance after payload cleanup");
    let deleted = ready(reopened.delete_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(deleted.phase, CaptureSubmissionPhase::Deleted);
    assert!(
        deleted.receipt.is_some(),
        "server deletion keeps the receipt detail reachable"
    );

    let late_edit = CaptureAttachments {
        note: "late native evidence edit".into(),
        screenshots: Vec::new(),
    };
    assert!(matches!(
        reopened.save_capture_evidence(CaptureSubmissionSource::LiveCapture(id), late_edit),
        Err(CaptureSubmissionError::InvalidState)
    ));
    let retained = reopened
        .capture_submission(CaptureSubmissionSource::RecordedArtifact(
            source_artifact_id,
        ))
        .unwrap()
        .unwrap();
    assert_eq!(retained.phase, CaptureSubmissionPhase::Deleted);
    assert_eq!(
        retained.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
    assert_eq!(retained.receipt, deleted.receipt);
    assert_eq!(retained.metadata.evidence, deleted.metadata.evidence);
}

#[test]
fn deleted_submission_with_retained_payload_can_be_edited_and_reprepared() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let original = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    database.shutdown().unwrap();

    let connection = Connection::open(&database_path).unwrap();
    let mut status: serde_json::Value = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&original.operation_id],
            |row| row.get::<_, String>(0),
        )
        .map(|json| serde_json::from_str(&json).unwrap())
        .unwrap();
    status["phase"] = serde_json::json!("deleted");
    assert_eq!(status["local_payload_state"], "retained");
    connection
        .execute(
            "UPDATE capture_submissions SET status_json = ?2 WHERE operation_id = ?1",
            rusqlite::params![original.operation_id, status.to_string()],
        )
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&database_path).unwrap();
    let edited = CaptureAttachments {
        note: "edit while canonical payload remains".into(),
        screenshots: Vec::new(),
    };
    assert_eq!(
        database
            .save_capture_evidence(CaptureSubmissionSource::LiveCapture(id), edited.clone())
            .unwrap(),
        edited
    );
    let reprepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    assert_ne!(reprepared.operation_id, original.operation_id);
    assert_eq!(reprepared.phase, CaptureSubmissionPhase::Prepared);
    assert_eq!(
        reprepared.metadata.evidence.as_ref().unwrap().note_bytes,
        u32::try_from(edited.note.len()).unwrap()
    );
}

fn assert_cleanup_retains_receipt_and_catalog(
    connection: &Connection,
    status: &CaptureSubmissionStatus,
    id: LiveCaptureId,
    source_artifact_id: &CaptureArtifactId,
    prepared: &CaptureSubmissionStatus,
) {
    assert_eq!(
        status.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
    assert_eq!(status.phase, CaptureSubmissionPhase::Sent);
    let capture_id = id.to_string();
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM live_capture_sessions WHERE capture_id = ?1",
                [&capture_id],
                |row| row.get(0),
            )
            .unwrap(),
        1,
        "the recording catalog row remains"
    );
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM live_capture_events WHERE capture_id = ?1",
                [&capture_id],
                |row| row.get(0),
            )
            .unwrap(),
        0,
        "event payload rows are removed"
    );
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM pevcap_capture_chunks", [], |row| row
                .get(0),)
            .unwrap(),
        0,
        "snapshot and original archive chunks are removed"
    );
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM capture_evidence WHERE source_artifact_id = ?1",
                [source_artifact_id.to_string()],
                |row| row.get(0),
            )
            .unwrap(),
        0,
        "source evidence JSON is removed"
    );
    assert_eq!(
        connection
            .query_row::<Option<String>, _, _>(
                "SELECT evidence_json FROM capture_submissions WHERE operation_id = ?1",
                [&prepared.operation_id],
                |row| row.get(0),
            )
            .unwrap(),
        None,
        "submission evidence JSON copy is removed"
    );
    assert!(
        status.metadata.evidence.is_some(),
        "receipt metadata retains the evidence manifest"
    );
    assert!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM pevcap_imports WHERE artifact_digest = ?1",
                [prepared.metadata.capture_sha256()],
                |row| row.get(0),
            )
            .unwrap()
            > 0,
        "import metadata remains for the submission foreign key"
    );
}

#[test]
fn confirmed_send_makes_local_capture_export_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    ready(database.send_capture_submission(&prepared.operation_id, ACCOUNT, "secret", &transport))
        .unwrap();
    let recorded_destination = directory.path().join("after-send-recorded.jsonl");
    assert!(matches!(
        database.export_recorded_capture(receipt.artifact_id(), &recorded_destination),
        Err(StorageError::CapturePayloadUnavailable)
    ));
    assert!(!recorded_destination.exists());
    let destination = directory.path().join("after-send.jsonl");
    assert!(database.export_live_capture(id, &destination).is_err());
    assert!(
        !destination.exists(),
        "unavailable data never publishes a partial export"
    );
    database.shutdown().unwrap();
}

#[test]
fn confirmed_send_wakes_finished_worker_maintenance() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let mut connection = Connection::open(&path).unwrap();
    super::super::configure_connection(&mut connection, &path).unwrap();
    connection.execute_batch(
        "UPDATE storage_normalization_progress SET semantic_complete = 1, artifact_complete = 1,
            compaction_pending = 0, compaction_complete = 1;
         UPDATE bms_voltage_backfill_progress SET complete = 1;"
    ).unwrap();
    drop(connection);
    let database = RideDatabase::open(&path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &Transport::default(),
    ))
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let status = database
            .capture_submission(CaptureSubmissionSource::LiveCapture(id))
            .unwrap()
            .unwrap();
        if status.local_payload_state == CaptureSubmissionLocalPayloadState::Removed {
            break;
        }
        if std::time::Instant::now() >= deadline {
            database.shutdown().unwrap();
            panic!("confirmed Send did not wake completed worker maintenance");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(
        database
            .capture_submission(CaptureSubmissionSource::LiveCapture(id))
            .unwrap()
            .unwrap()
            .receipt
            .is_some()
    );
    database.shutdown().unwrap();
}

#[test]
fn sent_sidecar_is_archived_before_cleanup_can_remove_its_backfill_source() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();

    let exported = directory.path().join("canonical-before-send.jsonl");
    database
        .export_pevcap_capture(prepared.metadata.capture_sha256(), &exported)
        .unwrap();
    let canonical_bytes = std::fs::read(&exported).unwrap();
    database.shutdown().unwrap();

    let digest = prepared.metadata.capture_sha256();
    let sidecar = seed_sent_managed_sidecar(&database_path, &prepared, &canonical_bytes);

    let database = RideDatabase::open(&database_path).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let status = database
            .capture_submission(CaptureSubmissionSource::LiveCapture(id))
            .unwrap()
            .unwrap();
        let connection = Connection::open(&database_path).unwrap();
        let artifact_complete: bool = connection
            .query_row(
                "SELECT artifact_complete FROM storage_normalization_progress WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if status.local_payload_state == CaptureSubmissionLocalPayloadState::Removed
            && artifact_complete
            && !sidecar.exists()
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            database.shutdown().unwrap();
            panic!("Sent sidecar was not normalized before local payload cleanup");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    database.shutdown().unwrap();

    let connection = Connection::open(&database_path).unwrap();
    assert_eq!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM pevcap_capture_chunks WHERE artifact_digest = ?1",
                [&digest],
                |row| row.get(0),
            )
            .unwrap(),
        0,
        "archive normalization must not resurrect removed Sent payload"
    );
    let status_json: String = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&prepared.operation_id],
            |row| row.get(0),
        )
        .unwrap();
    let status = decode_status(&status_json).unwrap();
    assert_eq!(status.phase, CaptureSubmissionPhase::Sent);
    assert_eq!(
        status.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
    assert!(
        status.receipt.is_some(),
        "the receipt remains available after cleanup"
    );
}

fn seed_sent_managed_sidecar(
    database_path: &Path,
    prepared: &CaptureSubmissionStatus,
    canonical_bytes: &[u8],
) -> PathBuf {
    let digest = prepared.metadata.capture_sha256();
    let mut managed_directory = database_path.as_os_str().to_owned();
    managed_directory.push(".pevcap-imports");
    let managed_directory = PathBuf::from(managed_directory);
    std::fs::create_dir_all(&managed_directory).unwrap();
    let sidecar = managed_directory.join(format!("{digest}.jsonl"));
    std::fs::write(&sidecar, canonical_bytes).unwrap();

    let mut status = serde_json::to_value(prepared).unwrap();
    status["phase"] = serde_json::json!("sent");
    status["local_payload_state"] = serde_json::json!("removal_pending");
    status["account_id"] = serde_json::json!(ACCOUNT);
    status["receipt"] = serde_json::json!({
        "operation_id": prepared.operation_id,
        "sha256": prepared.metadata.sha256,
        "bytes": prepared.metadata.bytes,
        "received_at_milliseconds": 1_700_000_002_000_u64,
    });

    let connection = Connection::open(database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    connection
        .execute(
            "UPDATE capture_submissions SET status_json = ?2 WHERE operation_id = ?1",
            rusqlite::params![prepared.operation_id, status.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE pevcap_imports SET artifact_path = ?2, artifact_size = ?3
         WHERE artifact_digest = ?1",
            rusqlite::params![
                digest,
                sidecar.to_string_lossy(),
                i64::try_from(canonical_bytes.len()).unwrap(),
            ],
        )
        .unwrap();
    connection
        .execute_batch(
            "UPDATE storage_normalization_progress SET semantic_complete = 1,
            artifact_complete = 0, compaction_pending = 0, compaction_complete = 1;
         UPDATE bms_voltage_backfill_progress SET complete = 1;",
        )
        .unwrap();
    sidecar
}

#[test]
fn invalid_persisted_receipt_never_authorizes_local_payload_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    database.shutdown().unwrap();

    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let mut invalid_sent = serde_json::to_value(&prepared).unwrap();
    invalid_sent["phase"] = serde_json::json!("sent");
    invalid_sent["local_payload_state"] = serde_json::json!("removal_pending");
    invalid_sent["account_id"] = serde_json::json!(ACCOUNT);
    invalid_sent["receipt"] = serde_json::json!({
        "operation_id": prepared.operation_id,
        "sha256": "f".repeat(64),
        "bytes": prepared.metadata.bytes,
        "received_at_milliseconds": 1_700_000_002_000_u64,
    });
    connection
        .execute(
            "UPDATE capture_submissions SET status_json = ?2 WHERE operation_id = ?1",
            rusqlite::params![prepared.operation_id, invalid_sent.to_string()],
        )
        .unwrap();
    let mut cursor = None;
    assert_eq!(
        cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap(),
        crate::storage::normalization::BatchState::Finished,
        "an invalid receipt cannot authorize cleanup or keep maintenance pending",
    );
    assert!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM live_capture_events WHERE capture_id = ?1",
                [id.to_string()],
                |row| row.get(0)
            )
            .unwrap()
            > 0
    );
}

#[test]
fn unsent_submission_sharing_capture_keeps_shared_local_payload() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    database.shutdown().unwrap();

    let mut shared = prepared.clone();
    shared.operation_id = Uuid::new_v4().to_string();
    shared.metadata.provenance.source_artifact_id = Uuid::new_v4().to_string();
    let connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let shared_json = serde_json::to_string(&shared).unwrap();
    connection
        .execute(
            "INSERT INTO capture_submissions
         (source_artifact_id, operation_id, snapshot_digest, status_json)
         VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                shared.metadata.provenance.source_artifact_id,
                shared.operation_id,
                shared.metadata.capture_sha256(),
                shared_json,
            ],
        )
        .unwrap();
    drop(connection);

    let database = RideDatabase::open(&database_path).unwrap();
    let transport = Transport::default();
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    database.shutdown().unwrap();

    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();

    let mut cursor = None;
    assert_eq!(
        cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap(),
        crate::storage::normalization::BatchState::Finished,
    );
    assert!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM live_capture_events WHERE capture_id = ?1",
                [id.to_string()],
                |row| row.get(0)
            )
            .unwrap()
            > 0
    );
    assert!(
        connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM pevcap_capture_chunks WHERE artifact_digest = ?1",
                [&prepared.metadata.capture_sha256()],
                |row| row.get(0)
            )
            .unwrap()
            > 0
    );
    let shared_json: String = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&shared.operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        decode_status(&shared_json).unwrap().phase,
        CaptureSubmissionPhase::Prepared
    );
    let json: String = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&prepared.operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        decode_status(&json).unwrap().local_payload_state,
        CaptureSubmissionLocalPayloadState::RemovalPending,
    );
}

#[test]
fn local_cleanup_does_not_invalidate_in_flight_remote_delete() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    ready(database.send_capture_submission(&prepared.operation_id, ACCOUNT, "secret", &transport))
        .unwrap();

    transport.pause_delete.store(true, Ordering::Release);
    let mut deletion = Box::pin(database.delete_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ));
    assert!(poll_once(deletion.as_mut()).is_pending());
    assert_eq!(
        transport.deleted.lock().unwrap().as_slice(),
        std::slice::from_ref(&prepared.operation_id),
        "Delete has been submitted and is waiting for its remote result",
    );

    let deletion_pending_json: String = {
        let connection = Connection::open(&database_path).unwrap();
        connection
            .query_row(
                "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
                [&prepared.operation_id],
                |row| row.get(0),
            )
            .unwrap()
    };
    let deletion_pending = decode_status(&deletion_pending_json).unwrap();
    assert_eq!(
        deletion_pending.phase,
        CaptureSubmissionPhase::DeletionPending
    );

    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    // Reproduce a historical cleanup candidate after Delete has captured its
    // revision. Maintenance changes only local payload state, so it must not
    // invalidate the in-flight remote deletion.
    connection
        .execute(
            "UPDATE capture_submissions
             SET status_json = json_set(status_json, '$.local_payload_state', 'removal_pending')
             WHERE operation_id = ?1",
            [&prepared.operation_id],
        )
        .unwrap();
    let mut cursor = None;
    assert_eq!(
        cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap(),
        crate::storage::normalization::BatchState::Pending,
    );
    let after_maintenance_json: String = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&prepared.operation_id],
            |row| row.get(0),
        )
        .unwrap();
    let after_maintenance = decode_status(&after_maintenance_json).unwrap();
    assert_eq!(
        after_maintenance.phase,
        CaptureSubmissionPhase::DeletionPending
    );
    assert_eq!(
        after_maintenance.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
    assert_eq!(after_maintenance.revision, deletion_pending.revision);
    drop(connection);

    transport.pause_delete.store(false, Ordering::Release);
    let deleted = ready(deletion).unwrap();
    assert_eq!(deleted.phase, CaptureSubmissionPhase::Deleted);
    assert_eq!(
        transport.deleted.lock().unwrap().as_slice(),
        std::slice::from_ref(&prepared.operation_id)
    );
    assert_eq!(
        deleted.local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed
    );
    assert!(deleted.receipt.is_some());
}

#[test]
fn metadata_invalid_rows_before_page_limit_do_not_starve_later_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    let mut target = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    database.shutdown().unwrap();

    let mut connection = Connection::open(&database_path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let invalid_digest = format!("{:064x}", 1);
    connection.execute(
        "INSERT INTO pevcap_imports
         (artifact_digest, artifact_path, ride_id, outcome, artifact_size, record_count, location_count, imported_at_ms)
         VALUES (?1, NULL, NULL, 'capture_only', 0, 0, 0, 1)",
        [&invalid_digest],
    ).unwrap();

    // Put the only valid cleanup after more metadata-invalid, otherwise valid
    // receipt rows than the helper's bounded candidate page.
    target.operation_id = "ffffffff-ffff-4fff-bfff-ffffffffffff".into();
    target.receipt.as_mut().unwrap().operation_id = target.operation_id.clone();
    let target_json = serde_json::to_string(&target).unwrap();
    connection.execute(
        "UPDATE capture_submissions SET operation_id = ?2, status_json = ?3 WHERE operation_id = ?1",
        rusqlite::params![prepared.operation_id, target.operation_id, target_json],
    ).unwrap();

    for n in 1..=70_u128 {
        let operation_id = format!("00000000-0000-4000-8000-{n:012x}");
        let source_artifact_id = Uuid::new_v4().to_string();
        let mut invalid = target.clone();
        invalid.operation_id.clone_from(&operation_id);
        invalid.metadata.sha256.clone_from(&invalid_digest);
        invalid.metadata.bytes = 0;
        invalid
            .metadata
            .provenance
            .source_artifact_id
            .clone_from(&source_artifact_id);
        invalid.metadata.provenance.source_sha256 = None;
        invalid.metadata.provenance.live_capture_id = None;
        invalid.receipt.as_mut().unwrap().operation_id = operation_id.clone();
        invalid
            .receipt
            .as_mut()
            .unwrap()
            .sha256
            .clone_from(&invalid_digest);
        invalid.receipt.as_mut().unwrap().bytes = 0;
        // Receipt fields match, but zero-byte metadata is invalid under the
        // typed Rust validator. These rows have unrelated payload identities.
        let status_json = serde_json::to_string(&invalid).unwrap();
        connection
            .execute(
                "INSERT INTO capture_submissions
             (source_artifact_id, operation_id, snapshot_digest, status_json)
             VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    source_artifact_id,
                    operation_id,
                    invalid_digest,
                    status_json
                ],
            )
            .unwrap();
    }

    let mut cursor = None;
    for _ in 0..4 {
        cleanup_sent_capture_batch(&mut connection, &mut cursor).unwrap();
        let removed: bool = connection
            .query_row(
                "SELECT json_extract(status_json, '$.local_payload_state') = 'removed'
             FROM capture_submissions WHERE operation_id = ?1",
                [&target.operation_id],
                |row| row.get(0),
            )
            .unwrap();
        if removed {
            break;
        }
    }
    let cleaned_json: String = connection
        .query_row(
            "SELECT status_json FROM capture_submissions WHERE operation_id = ?1",
            [&target.operation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        decode_status(&cleaned_json).unwrap().local_payload_state,
        CaptureSubmissionLocalPayloadState::Removed,
    );
}

#[test]
fn capture_submission_lost_response_retry_recovers_same_receipt_without_reupload() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let transport = Transport::default();
    *transport.failure.lock().unwrap() = Some(CaptureSubmissionTransferError::Network);
    let failed = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(failed.phase, CaptureSubmissionPhase::Retryable);
    *transport.lookup.lock().unwrap() = Some(CaptureSubmissionReceipt {
        operation_id: prepared.operation_id.clone(),
        sha256: prepared.metadata.sha256.clone(),
        bytes: prepared.metadata.bytes,
        received_at_milliseconds: 1_700_000_002_000,
    });
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    assert_eq!(sent.metadata, prepared.metadata);
    assert_eq!(transport.uploads.lock().unwrap().len(), 1);
    assert!(matches!(
        ready(database.send_capture_submission(
            &prepared.operation_id,
            "09ecaa84-3ca8-4363-9184-547a156ed5aa",
            "other",
            &transport
        )),
        Err(CaptureSubmissionError::WrongAccount)
    ));
}

#[test]
fn capture_submission_cancel_fences_delayed_upload_and_retries_remote_tombstone() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let transport = Transport::default();
    transport.pause.store(true, Ordering::Release);
    let mut send = Box::pin(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ));
    assert!(poll_once(send.as_mut()).is_pending());
    assert!(matches!(
        ready(database.cancel_capture_submission(&prepared.operation_id, None, &transport)),
        Err(CaptureSubmissionError::WrongAccount)
    ));
    *transport.failure.lock().unwrap() = Some(CaptureSubmissionTransferError::Network);
    let credential = Some(CaptureSubmissionCredential {
        account_id: ACCOUNT,
        token: "secret",
    });
    let pending =
        ready(database.cancel_capture_submission(&prepared.operation_id, credential, &transport))
            .unwrap();
    assert_eq!(pending.phase, CaptureSubmissionPhase::CancellationPending);
    transport.pause.store(false, Ordering::Release);
    let stale = ready(send).unwrap();
    assert_eq!(stale.phase, CaptureSubmissionPhase::CancellationPending);
    assert!(stale.receipt.is_none());
    let cancelled =
        ready(database.cancel_capture_submission(&prepared.operation_id, credential, &transport))
            .unwrap();
    assert_eq!(cancelled.phase, CaptureSubmissionPhase::Cancelled);
    assert_eq!(transport.deleted.lock().unwrap().len(), 2);
    assert!(matches!(
        ready(database.send_capture_submission(
            &prepared.operation_id,
            ACCOUNT,
            "secret",
            &transport
        )),
        Err(CaptureSubmissionError::InvalidState)
    ));
}

#[test]
fn capture_submission_prepared_cancel_is_local_and_preserves_original() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    let cancelled =
        ready(database.cancel_capture_submission(&prepared.operation_id, None, &transport))
            .unwrap();
    assert_eq!(cancelled.phase, CaptureSubmissionPhase::Cancelled);
    assert!(transport.deleted.lock().unwrap().is_empty());
    let exported = directory.path().join("still-local.jsonl");
    database.export_live_capture(id, &exported).unwrap();
    assert_ne!(std::fs::read(exported).unwrap(), [] as [u8; 0]);
}

#[test]
fn transfer_admission_requires_retryable_or_prepared_and_retained_payload() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let mut status = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let phases = [
        CaptureSubmissionPhase::Prepared,
        CaptureSubmissionPhase::Sending,
        CaptureSubmissionPhase::Retryable,
        CaptureSubmissionPhase::Rejected,
        CaptureSubmissionPhase::CancellationPending,
        CaptureSubmissionPhase::Cancelled,
        CaptureSubmissionPhase::Sent,
        CaptureSubmissionPhase::DeletionPending,
        CaptureSubmissionPhase::Deleted,
    ];
    let payload_states = [
        CaptureSubmissionLocalPayloadState::Retained,
        CaptureSubmissionLocalPayloadState::RemovalPending,
        CaptureSubmissionLocalPayloadState::Removed,
    ];
    for phase in phases {
        for payload_state in payload_states {
            status.phase = phase;
            status.local_payload_state = payload_state;
            let expected = matches!(
                phase,
                CaptureSubmissionPhase::Prepared | CaptureSubmissionPhase::Retryable
            ) && payload_state == CaptureSubmissionLocalPayloadState::Retained;
            assert_eq!(
                status.can_start_transfer(),
                expected,
                "phase {phase:?} with payload {payload_state:?}"
            );
        }
    }
}

#[test]
fn capture_submission_cancel_during_receipt_lookup_does_not_start_upload() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    transport.pause_lookup.store(true, Ordering::Release);
    let mut send = Box::pin(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ));
    assert!(poll_once(send.as_mut()).is_pending());
    let credential = Some(CaptureSubmissionCredential {
        account_id: ACCOUNT,
        token: "secret",
    });
    let cancelled =
        ready(database.cancel_capture_submission(&prepared.operation_id, credential, &transport))
            .unwrap();
    assert_eq!(cancelled.phase, CaptureSubmissionPhase::Cancelled);
    transport.pause_lookup.store(false, Ordering::Release);
    assert_eq!(
        ready(send).unwrap().phase,
        CaptureSubmissionPhase::Cancelled
    );
    assert!(transport.uploads.lock().unwrap().is_empty());
    assert_eq!(transport.deleted.lock().unwrap().len(), 1);
}

#[test]
fn capture_submission_sent_delete_requires_owner_and_preserves_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    assert_eq!(
        ready(database.send_capture_submission(
            &prepared.operation_id,
            ACCOUNT,
            "secret",
            &transport
        ))
        .unwrap()
        .phase,
        CaptureSubmissionPhase::Sent
    );
    assert!(matches!(
        ready(database.delete_capture_submission(
            &prepared.operation_id,
            "09ecaa84-3ca8-4363-9184-547a156ed5aa",
            "other",
            &transport
        )),
        Err(CaptureSubmissionError::WrongAccount)
    ));
    assert!(transport.deleted.lock().unwrap().is_empty());
    *transport.failure.lock().unwrap() = Some(CaptureSubmissionTransferError::Unavailable);
    assert_eq!(
        ready(database.delete_capture_submission(
            &prepared.operation_id,
            ACCOUNT,
            "secret",
            &transport
        ))
        .unwrap()
        .phase,
        CaptureSubmissionPhase::DeletionPending
    );
    assert_eq!(
        ready(database.delete_capture_submission(
            &prepared.operation_id,
            ACCOUNT,
            "secret",
            &transport
        ))
        .unwrap()
        .phase,
        CaptureSubmissionPhase::Deleted
    );
    let path = directory.path().join("removed-after-confirmed-send.jsonl");
    assert!(database.export_live_capture(id, &path).is_err());
    assert!(!path.exists());
    let status = database
        .capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap()
        .unwrap();
    assert_eq!(status.phase, CaptureSubmissionPhase::Deleted);
    assert!(status.receipt.is_some());
    database.shutdown().unwrap();
}

#[test]
fn capture_submission_preserves_finished_loss_and_rejects_corrupt_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let writer = recording(&database, directory.path());
    let _ = writer.ingress().reject_location_batch(3);
    let (id, receipt) = finished(writer);
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    assert_eq!(
        prepared.metadata.integrity,
        CaptureSubmissionIntegrity::Incomplete {
            dropped_messages: 3
        }
    );
    let observer = Connection::open(&path).unwrap();
    observer.execute("UPDATE pevcap_capture_chunks SET payload = zeroblob(length(payload)) WHERE artifact_digest = ?1", [&prepared.metadata.sha256]).unwrap();
    let transport = Transport::default();
    let rejected = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(rejected.phase, CaptureSubmissionPhase::Rejected);
    assert!(rejected.receipt.is_none());
}

#[test]
fn capture_submission_file_snapshot_preserves_original_and_exact_bytes() {
    {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let original_path = directory.path().join("original.jsonl");
        let writer = CaptureWriter::start(
            original_path.clone(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "file-wheel",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec![],
            },
        )
        .unwrap();
        let record = PevcapRecord::inbound_notification(
            MonotonicTimestamp::new(1),
            GattChannel::from_bytes([0x11; 16]),
            GattChannel::from_bytes([0x22; 16]),
            vec![0xff, 0x00, 0xa5],
        );

        assert_eq!(
            writer.try_send_record(record),
            CaptureWriteOutcome::Accepted
        );
        let CaptureWriterFinish::FileSaved(artifact) = writer.finish().unwrap() else {
            panic!("file receipt required");
        };
        database
            .retain_finished_capture(
                &artifact,
                CaptureOrigin::Manual,
                None,
                WallClockUnixTimestamp::new(1_700_000_001_000),
            )
            .unwrap();
        let original = std::fs::read(&original_path).unwrap();
        let prepared = database
            .prepare_capture_submission(CaptureSubmissionSource::RecordedArtifact(artifact.id()))
            .unwrap();
        assert_eq!(
            prepared.metadata.provenance.source_sha256.as_deref(),
            Some(artifact.content_digest())
        );
        assert_eq!(prepared.metadata.provenance.live_capture_id, None);
        assert_eq!(
            prepared.metadata.provenance.transformation,
            CaptureSubmissionTransformation::Unmodified
        );
        assert_eq!(std::fs::read(&original_path).unwrap(), original);
        let transport = Transport::default();
        let sent = ready(database.send_capture_submission(
            &prepared.operation_id,
            ACCOUNT,
            "secret",
            &transport,
        ))
        .unwrap();
        assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
        let uploads = transport.uploads.lock().unwrap();
        assert_eq!(uploads[0].1, original);
        let decoded =
            cutout_core::PevcapCapture::decode(&uploads[0].1, PevcapEncoding::Jsonl).unwrap();

        assert_eq!(decoded.records[0].bytes, vec![0xff, 0x00, 0xa5]);
        drop(uploads);
        database.shutdown().unwrap();
    }
}

#[test]
fn capture_submission_interrupted_send_reopens_retryable_without_network() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let transport = Transport::default();
    transport.pause.store(true, Ordering::Release);
    let mut send = Box::pin(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ));
    assert!(poll_once(send.as_mut()).is_pending());
    drop(send);
    database.shutdown().unwrap();
    let reopened = RideDatabase::open(&path).unwrap();
    let status = reopened.capture_submission(source).unwrap().unwrap();
    assert_eq!(status.phase, CaptureSubmissionPhase::Retryable);
    assert_eq!(status.operation_id, prepared.operation_id);
    assert_eq!(status.metadata, prepared.metadata);
    assert_eq!(status.account_id.as_deref(), Some(ACCOUNT));
    assert_eq!(transport.uploads.lock().unwrap().len(), 1);
    assert!(transport.deleted.lock().unwrap().is_empty());
}

#[test]
fn capture_submission_rejects_receipt_for_different_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    let transport = Transport::default();
    *transport.lookup.lock().unwrap() = Some(CaptureSubmissionReceipt {
        operation_id: prepared.operation_id.clone(),
        sha256: "a".repeat(64),
        bytes: prepared.metadata.bytes,
        received_at_milliseconds: 1_700_000_002_000,
    });
    let rejected = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(rejected.phase, CaptureSubmissionPhase::Rejected);
    assert_eq!(
        rejected.failure,
        Some(CaptureSubmissionTransferError::Conflict)
    );
    assert!(rejected.receipt.is_none());
    assert!(transport.uploads.lock().unwrap().is_empty());
}

#[test]
fn capture_submission_v48_migration_preserves_recording_and_starts_without_send() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    database.shutdown().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE capture_submissions; PRAGMA user_version = 48;")
        .unwrap();
    drop(connection);
    let reopened = RideDatabase::open(&path).unwrap();
    let source = CaptureSubmissionSource::LiveCapture(id);
    assert!(reopened.capture_submission(source).unwrap().is_none());
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(
        history.captures[0].recording.as_ref().unwrap().artifact_id,
        receipt.artifact_id()
    );
    let prepared = reopened.prepare_capture_submission(source).unwrap();
    assert_eq!(prepared.phase, CaptureSubmissionPhase::Prepared);
    assert!(prepared.account_id.is_none());
    assert!(prepared.receipt.is_none());
}

#[test]
fn capture_submission_account_deletion_retires_late_receipt_and_preserves_original() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let transport = Transport::default();
    transport.pause.store(true, Ordering::Release);
    let mut send = Box::pin(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ));
    assert!(poll_once(send.as_mut()).is_pending());
    database.begin_community_account_deletion().unwrap();
    database
        .complete_community_account_deletion(1_700_000_002_000)
        .unwrap();
    transport.pause.store(false, Ordering::Release);
    let stale = ready(send).unwrap();
    assert_eq!(stale.phase, CaptureSubmissionPhase::Deleted);
    assert!(stale.receipt.is_none());
    assert_eq!(
        database.capture_submission(source).unwrap().unwrap().phase,
        CaptureSubmissionPhase::Deleted
    );
    assert!(matches!(
        ready(database.send_capture_submission(
            &prepared.operation_id,
            ACCOUNT,
            "secret",
            &transport
        )),
        Err(CaptureSubmissionError::WrongAccount)
    ));
    let history = database
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(
        history.captures[0].recording.as_ref().unwrap().artifact_id,
        receipt.artifact_id()
    );
}

#[test]
fn capture_submission_freezes_and_sends_reported_settings_after_relaunch() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let writer = recording(&database, directory.path());
    let first = crate::capture_writer::settings_observation_tests::observation(
        writer.capture_identity(),
        100,
    );
    let mut second = first.clone();
    second.observed_at_ms = 180;
    second.recorded_at_ms = 200;
    second.values[0].value = cutout_core::CaptureObservedValue::Entered("125.00".into());
    for value in [&first, &second] {
        assert_eq!(
            writer.try_send_settings_observation(value.clone()).unwrap(),
            CaptureWriteOutcome::Accepted
        );
    }
    let (id, receipt) = finished(writer);
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    assert!(prepared.categories.annotations);
    let original_snapshot = directory.path().join("snapshot.jsonl");
    database
        .export_pevcap_capture(&prepared.metadata.sha256, &original_snapshot)
        .unwrap();
    let original_bytes = std::fs::read(original_snapshot).unwrap();
    drop(database);
    let reopened = RideDatabase::open(&database_path).unwrap();
    assert_eq!(
        reopened.prepare_capture_submission(source).unwrap(),
        prepared
    );
    let transport = Transport::default();
    let sent = ready(reopened.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    let uploads = transport.uploads.lock().unwrap();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].1, original_bytes);
    let capture = cutout_core::PevcapCapture::decode(&uploads[0].1, PevcapEncoding::Jsonl).unwrap();
    assert_eq!(capture.settings_observations, vec![first, second]);

    assert_eq!(sent.metadata, prepared.metadata);
}

#[test]
fn capture_notes_screenshots_survive_save_reopen_and_exact_send() {
    use cutout_core::{
        CaptureAttachments, CaptureEvidenceError, CaptureScreenshot,
        validate_capture_submission_payload,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let writer = recording(&database, directory.path());
    let (id, receipt) = finished(writer);
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let evidence = CaptureAttachments {
        note: "é".repeat(512),
        screenshots: vec![
            CaptureScreenshot {
                jpeg: vec![0xff, 0xd8, 0xff, 0xd9]
            };
            4
        ],
    };
    assert_eq!(
        database
            .save_capture_evidence(source, evidence.clone())
            .unwrap(),
        evidence
    );
    let mut excessive = evidence.clone();
    excessive.note.push('a');
    assert!(matches!(
        database.save_capture_evidence(source, excessive),
        Err(CaptureSubmissionError::InvalidEvidence(
            CaptureEvidenceError::NoteTooLong
        ))
    ));
    assert_eq!(database.capture_evidence(source).unwrap(), evidence);
    drop(database);
    let database = RideDatabase::open(&path).unwrap();
    assert_eq!(database.capture_evidence(source).unwrap(), evidence);
    let first = database.prepare_capture_submission(source).unwrap();
    let mut edited = evidence.clone();
    edited.note = "saved correction".into();
    database
        .save_capture_evidence(source, edited.clone())
        .unwrap();
    let prepared = database.prepare_capture_submission(source).unwrap();
    assert_ne!(prepared.operation_id, first.operation_id);
    assert_eq!(prepared.metadata.evidence.as_ref().unwrap().screenshots, 4);
    let transport = Transport::default();
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
    assert!(matches!(
        database.save_capture_evidence(source, evidence),
        Err(CaptureSubmissionError::InvalidState)
    ));
    let uploaded = transport.uploads.lock().unwrap();
    let bytes = &uploaded[0].1;
    let summary = validate_capture_submission_payload(bytes.as_slice(), &sent.metadata).unwrap();
    assert_eq!(summary.sha256, sent.receipt.unwrap().sha256);
    let raw_length = usize::try_from(sent.metadata.capture_bytes()).unwrap();
    assert_eq!(
        cutout_core::capture_evidence_digest(&bytes[..raw_length]),
        sent.metadata.capture_sha256()
    );
    let decoded: CaptureAttachments = serde_json::from_slice(&bytes[raw_length..]).unwrap();
    assert_eq!(decoded, edited);
    assert!(sent.metadata.evidence.is_some());
    assert!(matches!(
        sent.local_payload_state,
        CaptureSubmissionLocalPayloadState::RemovalPending
            | CaptureSubmissionLocalPayloadState::Removed
    ));
}

#[test]
fn capture_evidence_v49_migration_preserves_receipt_metadata_and_removes_local_payload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let sent = ready(database.send_capture_submission(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &Transport::default(),
    ))
    .unwrap();
    database.shutdown().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("BEGIN; ALTER TABLE capture_submissions RENAME TO current_submissions; CREATE TABLE capture_submissions(source_artifact_id TEXT PRIMARY KEY NOT NULL, operation_id TEXT NOT NULL UNIQUE, snapshot_digest TEXT NOT NULL REFERENCES pevcap_imports(artifact_digest), status_json TEXT NOT NULL CHECK(json_valid(status_json)) CHECK(json_extract(status_json, '$.metadata.sha256') = snapshot_digest)); INSERT INTO capture_submissions SELECT source_artifact_id,operation_id,snapshot_digest,status_json FROM current_submissions; DROP TABLE current_submissions; DROP TABLE capture_evidence; PRAGMA user_version=49; COMMIT;").unwrap();
    drop(connection);
    let reopened = RideDatabase::open(&path).unwrap();
    let migrated = reopened.capture_submission(source).unwrap().unwrap();
    assert_eq!(migrated.phase, sent.phase);
    assert_eq!(migrated.metadata, sent.metadata);
    assert_eq!(migrated.account_id, sent.account_id);
    assert_eq!(migrated.receipt, sent.receipt);
    assert_eq!(migrated.revision, sent.revision);
    assert!(matches!(
        migrated.local_payload_state,
        CaptureSubmissionLocalPayloadState::RemovalPending
            | CaptureSubmissionLocalPayloadState::Removed
    ));
    assert_eq!(
        reopened.capture_evidence(source).unwrap(),
        CaptureAttachments::default()
    );
    let raw = directory.path().join("original.jsonl");
    assert!(
        reopened
            .export_pevcap_capture(&sent.metadata.sha256, &raw)
            .is_err()
    );
    assert!(
        !raw.exists(),
        "a sent capture is unavailable for local export"
    );
}

#[test]
fn capture_submission_progress_starts_at_zero_for_the_admitted_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    database
        .save_capture_evidence(
            source,
            CaptureAttachments {
                note: "Access note".into(),
                screenshots: vec![
                    cutout_core::CaptureScreenshot {
                        jpeg: vec![0xff, 0xd8, 0xff, 0xd9]
                    };
                    4
                ],
            },
        )
        .unwrap();
    let prepared = database.prepare_capture_submission(source).unwrap();
    let progress = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&progress);
    let transport = Transport::default();
    let completed = ready(database.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        Arc::new(move |event| observed.lock().unwrap().push(event)),
    ))
    .unwrap();
    assert_eq!(completed.phase, CaptureSubmissionPhase::Sent);
    let progress = progress.lock().unwrap();
    let first = progress
        .first()
        .expect("admitted send must report initial progress");
    assert_eq!(first.operation_id, prepared.operation_id);
    assert_eq!(first.bytes_sent, 0);
    assert_eq!(first.total_bytes, prepared.metadata.bytes);
    assert_eq!(first.phase, CaptureSubmissionProgressPhase::Uploading);
    assert!(first.attempt_id > prepared.revision);
}

#[test]
fn capture_submission_progress_waits_for_receipt_and_retry_restarts_at_zero() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&events);
    let callback: CaptureSubmissionProgressCallback =
        Arc::new(move |progress| observed.lock().unwrap().push(progress));
    let transport = Transport::default();
    transport.pause.store(true, Ordering::Release);
    *transport.failure.lock().unwrap() = Some(CaptureSubmissionTransferError::Network);
    let mut send = Box::pin(database.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        Arc::clone(&callback),
    ));
    assert!(poll_once(send.as_mut()).is_pending());
    let previous_attempt = {
        let progress = events.lock().unwrap();
        assert!(progress.len() > 2);
        assert!(
            progress
                .windows(2)
                .all(|pair| pair[0].bytes_sent < pair[1].bytes_sent)
        );
        let last = progress.last().unwrap();
        assert_eq!(last.bytes_sent, prepared.metadata.bytes);
        assert_eq!(last.phase, CaptureSubmissionProgressPhase::VerifyingReceipt);
        last.attempt_id
    };
    let pending = database.capture_submission(source).unwrap().unwrap();
    assert_eq!(pending.phase, CaptureSubmissionPhase::Sending);
    assert!(pending.receipt.is_none(), "100% is not a durable receipt");
    transport.pause.store(false, Ordering::Release);
    assert_eq!(
        ready(send).unwrap().phase,
        CaptureSubmissionPhase::Retryable
    );
    events.lock().unwrap().clear();
    let completed = ready(database.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        callback,
    ))
    .unwrap();
    assert_eq!(completed.phase, CaptureSubmissionPhase::Sent);
    let progress = events.lock().unwrap();
    assert_eq!(
        progress[0].bytes_sent, 0,
        "retry never claims a resumable offset"
    );
    assert!(progress[0].attempt_id > previous_attempt);
    assert!(
        progress
            .iter()
            .all(|event| event.attempt_id == progress[0].attempt_id
                && event.total_bytes == prepared.metadata.bytes)
    );
}

#[test]
fn capture_submission_progress_ignores_cancelled_retired_and_dropped_attempts() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let sending = database
        .submission_action(
            &prepared.operation_id,
            Action::Start {
                account_id: ACCOUNT.into(),
            },
        )
        .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&events);
    let reporter = CaptureSubmissionProgressReporter {
        database: database.clone(),
        operation_id: prepared.operation_id.clone(),
        attempt_id: sending.revision,
        total_bytes: prepared.metadata.bytes,
        callback: Arc::new(move |event| observed.lock().unwrap().push(event)),
        active: Arc::new(AtomicBool::new(true)),
    };
    reporter.report(0);
    assert_eq!(events.lock().unwrap().len(), 1);
    {
        let _guard = CaptureSubmissionProgressGuard(Some(reporter.clone()));
    }
    reporter.report(10);
    assert_eq!(
        events.lock().unwrap().len(),
        1,
        "dropped future cannot report a later blocking chunk"
    );
    reporter.active.store(true, Ordering::Release);
    database
        .submission_action(
            &prepared.operation_id,
            Action::Failure {
                revision: sending.revision,
                failure: CaptureSubmissionTransferError::Network,
            },
        )
        .unwrap();
    reporter.report(20);
    assert_eq!(
        events.lock().unwrap().len(),
        1,
        "failed attempt cannot update progress"
    );
    let retry = database
        .submission_action(
            &prepared.operation_id,
            Action::Start {
                account_id: ACCOUNT.into(),
            },
        )
        .unwrap();
    reporter.report(30);
    assert_eq!(
        events.lock().unwrap().len(),
        1,
        "previous attempt cannot update a retry"
    );
    let current = CaptureSubmissionProgressReporter {
        attempt_id: retry.revision,
        ..reporter.clone()
    };
    database
        .submission_action(
            &prepared.operation_id,
            Action::Removal {
                account_id: Some(ACCOUNT.into()),
                deleting: false,
            },
        )
        .unwrap();
    current.report(40);
    assert_eq!(
        events.lock().unwrap().len(),
        1,
        "cancelled attempt cannot update progress"
    );
}

#[test]
fn capture_submission_relaunch_retains_no_progress_and_receipt_recovery_needs_no_upload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&events);
    let transport = Transport::default();
    transport.pause.store(true, Ordering::Release);
    let mut send = Box::pin(database.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        Arc::new(move |event| observed.lock().unwrap().push(event)),
    ));
    assert!(poll_once(send.as_mut()).is_pending());
    drop(send);
    let count = events.lock().unwrap().len();
    database.shutdown().unwrap();
    let reopened = RideDatabase::open(&path).unwrap();
    assert_eq!(events.lock().unwrap().len(), count);
    assert_eq!(
        reopened.capture_submission(source).unwrap().unwrap().phase,
        CaptureSubmissionPhase::Retryable
    );
    let receipt = CaptureSubmissionReceipt {
        operation_id: prepared.operation_id.clone(),
        sha256: prepared.metadata.sha256.clone(),
        bytes: prepared.metadata.bytes,
        received_at_milliseconds: 123,
    };
    *transport.lookup.lock().unwrap() = Some(receipt);
    events.lock().unwrap().clear();
    let observed = Arc::clone(&events);
    let completed = ready(reopened.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        Arc::new(move |event| observed.lock().unwrap().push(event)),
    ))
    .unwrap();
    assert_eq!(completed.phase, CaptureSubmissionPhase::Sent);
    assert_eq!(
        transport.uploads.lock().unwrap().len(),
        1,
        "recovered receipt avoids a second upload"
    );
    let progress = events.lock().unwrap();
    assert_eq!(progress.len(), 1);
    assert_eq!(
        progress[0].bytes_sent, 0,
        "recovery does not invent uploaded bytes"
    );
}
