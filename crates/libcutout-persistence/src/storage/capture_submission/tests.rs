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
    assert_eq!(database.capture_evidence(source).unwrap(), edited);
}

#[test]
fn capture_evidence_v49_migration_preserves_existing_receipt_and_raw_capture() {
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
    assert_eq!(reopened.capture_submission(source).unwrap().unwrap(), sent);
    assert_eq!(
        reopened.capture_evidence(source).unwrap(),
        CaptureAttachments::default()
    );
    let raw = directory.path().join("original.jsonl");
    reopened
        .export_pevcap_capture(&sent.metadata.sha256, &raw)
        .unwrap();
    assert_eq!(
        cutout_core::capture_evidence_digest(&std::fs::read(raw).unwrap()),
        sent.metadata.sha256
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
    let sent = ready(database.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        Arc::new(move |event| observed.lock().unwrap().push(event)),
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
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
    let sent = ready(database.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        callback,
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
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
    let sent = ready(reopened.send_capture_submission_with_progress(
        &prepared.operation_id,
        ACCOUNT,
        "secret",
        &transport,
        Arc::new(move |event| observed.lock().unwrap().push(event)),
    ))
    .unwrap();
    assert_eq!(sent.phase, CaptureSubmissionPhase::Sent);
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
