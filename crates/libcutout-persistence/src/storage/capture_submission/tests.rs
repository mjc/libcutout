use super::*;
use crate::{
    CaptureMetadata, CaptureWriteOutcome, CaptureWriter, CaptureWriterFinish, LiveCaptureId,
    LiveCaptureIntegrity, QueryLimit, RideDatabase, SavedDatabaseCapture,
};
use cutout_core::{
    CaptureOrigin, GattChannel, MonotonicTimestamp, PevcapHeader, PevcapRecord,
    WallClockUnixTimestamp,
};
use std::sync::{
    Mutex,
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
            payload
                .read_to_end(&mut bytes)
                .map_err(|_| CaptureSubmissionTransferError::InvalidCapture)?;
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
            if let Some(error) = self.failure.lock().unwrap().take() {
                Err(error)
            } else {
                Ok(())
            }
        })
    }
}

const ACCOUNT: &str = "3a61d30c-669a-4a98-8b76-ddbb2b766e14";

fn recording(database: &RideDatabase, directory: &std::path::Path) -> CaptureWriter {
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

fn gps_recording(database: &RideDatabase, directory: &std::path::Path) -> LiveCaptureId {
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
    let receipt = database
        .confirm_pevcap_publication(
            &preview,
            1_700_000_001_000,
            &CapturePublication::SubmissionSnapshot,
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
    assert_eq!(prepared.metadata.origin, CaptureOrigin::Manual);
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

#[test]
fn capture_submission_preserves_automatic_origin_as_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Automatic);
    let prepared = database
        .prepare_capture_submission(CaptureSubmissionSource::LiveCapture(id))
        .unwrap();
    assert_eq!(prepared.metadata.origin, CaptureOrigin::Automatic);
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
fn capture_submission_freezes_payload_before_later_source_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let (id, receipt) = finished(recording(&database, directory.path()));
    retain(&database, &receipt, CaptureOrigin::Manual);
    let source = CaptureSubmissionSource::LiveCapture(id);
    let prepared = database.prepare_capture_submission(source).unwrap();
    database.purge_capture_music_metadata(&[]).unwrap();
    assert_eq!(
        database.capture_submission(source).unwrap(),
        Some(prepared.clone())
    );
    assert_eq!(
        database.prepare_capture_submission(source).unwrap(),
        prepared
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
    assert_eq!(
        capture.music_events,
        [] as [cutout_core::PevcapMusicEvent; 0]
    );
    assert!(String::from_utf8_lossy(payload).contains("capture_label=ride_stop"));
    assert_eq!(
        sent.receipt.as_ref().unwrap().sha256,
        prepared.metadata.sha256
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
fn capture_submission_sent_delete_requires_owner_and_preserves_local_capture() {
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
    let path = directory.path().join("original-still-exportable.jsonl");
    database.export_live_capture(id, &path).unwrap();
    assert_ne!(std::fs::read(path).unwrap(), [] as [u8; 0]);
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
fn capture_submission_file_snapshot_preserves_original_and_records_music_derivative() {
    for legacy_music in [false, true] {
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
        let mut record = PevcapRecord::inbound_notification(
            MonotonicTimestamp::new(1),
            GattChannel::from_bytes([0x11; 16]),
            GattChannel::from_bytes([0x22; 16]),
            vec![0xff, 0x00, 0xa5],
        );
        if legacy_music {
            let music = cutout_core::PevcapMusicEvent::new(
                cutout_core::MusicProvider::Spotify,
                "spotify:track:private",
                MonotonicTimestamp::new(1),
                WallClockUnixTimestamp::new(1_700_000_000_001),
                1,
                None,
            )
            .unwrap();
            record.music = Some(music.clone());
            assert_eq!(writer.record_music(music), CaptureWriteOutcome::Accepted);
        }
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
            if legacy_music {
                CaptureSubmissionTransformation::MusicRemoved
            } else {
                CaptureSubmissionTransformation::Unmodified
            }
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
        if legacy_music {
            assert_ne!(uploads[0].1, original);
        } else {
            assert_eq!(uploads[0].1, original);
        }
        let decoded =
            cutout_core::PevcapCapture::decode(&uploads[0].1, PevcapEncoding::Jsonl).unwrap();
        assert_eq!(
            decoded.music_events,
            [] as [cutout_core::PevcapMusicEvent; 0]
        );
        assert!(decoded.records.iter().all(|record| record.music.is_none()));
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
    database.purge_capture_music_metadata(&[]).unwrap();
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
    assert!(capture.music_events.is_empty());
    assert_eq!(sent.metadata, prepared.metadata);
}
