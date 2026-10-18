use super::*;
use cutout_core::{
    CaptureObservationSource, CaptureObservedValue, CaptureResearchProfile,
    CaptureResearchProfileSelection, CaptureSettingObservationValue, CaptureSettingsObservation,
    CaptureSettingsObservationError, PevcapCapture, PevcapEncoding,
};

pub(crate) fn observation(capture_id: String, recorded_at_ms: u64) -> CaptureSettingsObservation {
    CaptureSettingsObservation {
        capture_id,
        profile: CaptureResearchProfile {
            profile_id: "nosfet_aero".into(),
            pev_category: Some("euc".into()),
            model: Some("Unsupported research device".into()),
            firmware: None,
            selection: CaptureResearchProfileSelection::Manual,
        },
        observed_at_ms: recorded_at_ms.saturating_sub(20),
        recorded_at_ms,
        source: CaptureObservationSource::ManufacturerApp,
        values: vec![
            CaptureSettingObservationValue {
                setting_id: "pwm_tiltback".into(),
                value: CaptureObservedValue::Off,
                unit: Some("%".into()),
                note: Some("Physical screen readback".into()),
            },
            CaptureSettingObservationValue {
                setting_id: "display_brightness".into(),
                value: CaptureObservedValue::Entered("0".into()),
                unit: None,
                note: None,
            },
            CaptureSettingObservationValue {
                setting_id: "future_unknown_setting".into(),
                value: CaptureObservedValue::Unobserved,
                unit: None,
                note: None,
            },
        ],
        note: Some("Before changing display units".into()),
    }
}

fn metadata() -> CaptureMetadata {
    CaptureMetadata {
        advertised_services: vec![],
        gatt_fingerprints: vec![],
        resolved_identity: None,
        annotations: vec![],
    }
}

#[test]
fn settings_observations_survive_database_relaunch_export_and_history() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let writer = CaptureWriter::start_with_database(
        directory.path().join("capture.jsonl"),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "unsupported-device",
        None,
        &metadata(),
        database.clone(),
    )
    .unwrap();
    let first = observation(writer.capture_identity(), 100);
    let mut second = observation(writer.capture_identity(), 300);
    second.values[0].value = CaptureObservedValue::Entered("125.00".into());
    second.note = Some("After changing one setting".into());
    for value in [&first, &second] {
        assert_eq!(
            writer.try_send_settings_observation(value.clone()).unwrap(),
            CaptureWriteOutcome::Accepted
        );
    }
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id,
        receipt,
        ..
    } = writer.finish_without_export().unwrap()
    else {
        panic!("database receipt required")
    };
    database
        .retain_finished_database_capture(
            &receipt,
            cutout_core::CaptureOrigin::Manual,
            None,
            WallClockUnixTimestamp::new(1_700_000_001_000),
        )
        .unwrap();
    drop(database);
    let reopened = RideDatabase::open(&database_path).unwrap();
    assert_eq!(
        reopened
            .live_capture_settings_observations(live_capture_id)
            .unwrap(),
        vec![first.clone(), second.clone()]
    );
    assert_eq!(
        reopened
            .capture_settings_observations(crate::CaptureSubmissionSource::RecordedArtifact(
                receipt.artifact_id()
            ))
            .unwrap(),
        vec![first.clone(), second.clone()]
    );
    let destination = directory.path().join("export.jsonl");
    reopened
        .export_live_capture(live_capture_id, &destination)
        .unwrap();
    let capture =
        PevcapCapture::decode(&fs::read(destination).unwrap(), PevcapEncoding::Jsonl).unwrap();
    assert_eq!(capture.settings_observations, vec![first, second]);
}

#[test]
fn settings_observations_survive_file_retention_without_the_original_file() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("capture.jsonl");
    let writer = CaptureWriter::start(
        original.clone(),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "unsupported",
        None,
        &metadata(),
    )
    .unwrap();
    let expected = observation(writer.capture_identity(), 100);
    assert_eq!(
        writer
            .try_send_settings_observation(expected.clone())
            .unwrap(),
        CaptureWriteOutcome::Accepted
    );
    let CaptureWriterFinish::FileSaved(saved) = writer.finish().unwrap() else {
        panic!("file receipt required")
    };
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    database
        .retain_finished_capture(
            &saved,
            cutout_core::CaptureOrigin::Manual,
            None,
            WallClockUnixTimestamp::new(1_700_000_001_000),
        )
        .unwrap();
    fs::remove_file(original).unwrap();
    drop(database);
    let reopened = RideDatabase::open(&database_path).unwrap();
    assert_eq!(
        reopened
            .capture_settings_observations(crate::CaptureSubmissionSource::RecordedArtifact(
                saved.id()
            ))
            .unwrap(),
        vec![expected]
    );
}

#[test]
fn settings_observation_writer_rejects_wrong_capture_and_oversized_input() {
    let directory = tempfile::tempdir().unwrap();
    let writer = CaptureWriter::start(
        directory.path().join("capture.jsonl"),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "unsupported",
        None,
        &metadata(),
    )
    .unwrap();
    let mut value = observation(Uuid::new_v4().to_string(), 100);
    assert!(matches!(
        writer.try_send_settings_observation(value.clone()),
        Err(CaptureSettingsObservationError::InvalidCaptureId)
    ));
    value.capture_id = writer.capture_identity();
    value.note = Some("x".repeat(1025));
    assert!(matches!(
        writer.try_send_settings_observation(value),
        Err(CaptureSettingsObservationError::TooLarge)
    ));
    writer.finish().unwrap();
}

#[test]
fn capture_transport_record_reader_keeps_order_across_pages_and_file_sources() {
    for use_database in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let path = directory.path().join("capture.jsonl");
        let writer = if use_database {
            CaptureWriter::start_with_database(
                path,
                WallClockUnixTimestamp::new(1_700_000_000_000),
                "unknown",
                None,
                &metadata(),
                database.clone(),
            )
            .unwrap()
        } else {
            CaptureWriter::start(
                path,
                WallClockUnixTimestamp::new(1_700_000_000_000),
                "unknown",
                None,
                &metadata(),
            )
            .unwrap()
        };
        let expected = (1..=70).map(tests::stationary_record).collect::<Vec<_>>();
        for record in &expected {
            assert_eq!(
                writer.try_send_record(record.clone()),
                CaptureWriteOutcome::Accepted
            );
        }
        assert_eq!(
            writer
                .try_send_settings_observation(observation(writer.capture_identity(), 100))
                .unwrap(),
            CaptureWriteOutcome::Accepted
        );
        let mut writer_records = Vec::new();
        writer
            .visit_transport_records(|record| writer_records.push(record))
            .unwrap();
        assert_eq!(writer_records, expected);
        if use_database {
            writer.flush().unwrap();
            let mut active_records = Vec::new();
            let id = LiveCaptureId::parse(&writer.capture_identity()).unwrap();
            database
                .visit_capture_transport_records(
                    crate::CaptureSubmissionSource::LiveCapture(id),
                    |record| active_records.push(record),
                )
                .unwrap();
            assert_eq!(active_records, expected);
            assert_eq!(
                database
                    .live_capture_settings_observations(id)
                    .unwrap()
                    .len(),
                1
            );
        }
        let source = match writer.finish_without_export().unwrap() {
            CaptureWriterFinish::FileSaved(saved) => {
                database
                    .retain_finished_capture(
                        &saved,
                        cutout_core::CaptureOrigin::Manual,
                        None,
                        WallClockUnixTimestamp::new(1_700_000_001_000),
                    )
                    .unwrap();
                crate::CaptureSubmissionSource::RecordedArtifact(saved.id())
            }
            CaptureWriterFinish::DatabaseFinished {
                live_capture_id, ..
            } => crate::CaptureSubmissionSource::LiveCapture(live_capture_id),
        };
        let mut records = Vec::new();
        database
            .visit_capture_transport_records(source, |record| records.push(record))
            .unwrap();
        assert_eq!(records, expected);
        database.shutdown().unwrap();
    }
}

#[test]
fn live_settings_query_rejects_an_observation_for_another_capture() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let writer = CaptureWriter::start_with_database(
        directory.path().join("capture.jsonl"),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "unknown",
        None,
        &metadata(),
        database.clone(),
    )
    .unwrap();
    writer.flush().unwrap();
    let id = LiveCaptureId::parse(&writer.capture_identity()).unwrap();
    let foreign = observation(Uuid::new_v4().to_string(), 100);
    database
        .append_live_capture_event(
            id,
            LiveCaptureEventKind::Metadata,
            100,
            None,
            None,
            foreign.to_jsonl_line().unwrap().into_bytes(),
        )
        .unwrap();
    assert!(database.live_capture_settings_observations(id).is_err());
    assert!(
        database
            .capture_settings_observations(crate::CaptureSubmissionSource::LiveCapture(id))
            .is_err()
    );
    writer.finish_without_export().unwrap();
}

#[test]
fn pinned_read_scan_allows_live_admission_flush_and_finish_without_extending_its_prefix() {
    use std::sync::mpsc::sync_channel;
    for database_backed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let path = directory.path().join("capture.jsonl");
        let writer = if database_backed {
            CaptureWriter::start_with_database(
                path,
                WallClockUnixTimestamp::new(1_700_000_000_000),
                "unsupported",
                None,
                &metadata(),
                database.clone(),
            )
        } else {
            CaptureWriter::start(
                path,
                WallClockUnixTimestamp::new(1_700_000_000_000),
                "unsupported",
                None,
                &metadata(),
            )
        }
        .unwrap();
        for timestamp in 0..40 {
            assert_eq!(
                writer.try_send_record(PevcapRecord::link_down(
                    cutout_core::MonotonicTimestamp::new(timestamp)
                )),
                CaptureWriteOutcome::Accepted
            );
        }
        // Match the native ownership boundary: acquire only the admitted read receipt
        // while holding the writer mutex, then scan after releasing it.
        let writer = Mutex::new(writer);
        let read = writer.lock().unwrap().begin_transport_read().unwrap();
        let (entered, entered_receiver) = sync_channel(1);
        let (release, release_receiver) = sync_channel(1);
        let scan = thread::spawn(move || {
            let mut records = Vec::new();
            read.visit_transport_records(|record| {
                if records.is_empty() {
                    entered.send(()).unwrap();
                    release_receiver.recv().unwrap();
                }
                records.push(record.monotonic_ms.get());
            })
            .unwrap();
            records
        });
        entered_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let flush = {
            let writer = writer
                .try_lock()
                .expect("held comparison must not own writer admission");
            assert_eq!(
                writer.try_send_record(PevcapRecord::link_down(
                    cutout_core::MonotonicTimestamp::new(100)
                )),
                CaptureWriteOutcome::Accepted
            );
            let mut changed = metadata();
            changed
                .annotations
                .push("note=header changed while comparison is held".into());
            assert_eq!(
                writer.update_metadata(changed),
                CaptureWriteOutcome::Accepted
            );
            writer.begin_flush().unwrap()
        };
        flush.wait().unwrap();
        assert_eq!(
            writer.lock().unwrap().monitor().status().dropped_messages,
            0
        );
        // File finalization replaces the header by atomic rename; the opened prefix
        // still owns its old inode. SQLite keeps its already-pinned sequence limit.
        writer
            .into_inner()
            .unwrap()
            .finish_without_export()
            .unwrap();
        release.send(()).unwrap();
        assert_eq!(scan.join().unwrap(), (0..40).collect::<Vec<_>>());
        database.shutdown().unwrap();
    }
}

fn settings_test_writer(
    path: &Path,
    database: &RideDatabase,
    database_backed: bool,
) -> CaptureWriter {
    if database_backed {
        CaptureWriter::start_with_database(
            path.to_path_buf(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "unsupported",
            None,
            &metadata(),
            database.clone(),
        )
    } else {
        CaptureWriter::start(
            path.to_path_buf(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "unsupported",
            None,
            &metadata(),
        )
    }
    .unwrap()
}

#[test]
fn settings_snapshot_receipt_uses_the_last_queue_slot_and_rejection_keeps_no_hidden_value() {
    for database_backed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
        let path = directory.path().join("capture.jsonl");
        let writer = settings_test_writer(&path, &database, database_backed);
        writer.flush().unwrap();
        // An existing rendezvous barrier holds the real worker after durability.
        // Once dequeued it cannot consume another queue entry until released below.
        let (hold, release) = sync_channel(0);
        assert_eq!(
            writer.try_send(CaptureWriterMessage::Barrier(CaptureBarrier::Flush, hold)),
            CaptureWriteOutcome::Accepted
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while writer.monitor().status().queued_messages != 0 {
            assert!(
                Instant::now() < deadline,
                "writer did not reach held barrier"
            );
            thread::yield_now();
        }
        for _ in 0..CAPTURE_WRITER_QUEUE_CAPACITY - 1 {
            assert_eq!(
                writer.update_metadata(metadata()),
                CaptureWriteOutcome::Accepted
            );
        }
        let expected = observation(writer.capture_identity(), 100);
        let CaptureSettingsObservationAdmission::Accepted(receipt) =
            writer.begin_settings_observation(expected.clone()).unwrap()
        else {
            panic!("last queue slot must admit observation and receipt together");
        };
        assert_eq!(
            writer.monitor().status().queued_messages,
            CAPTURE_WRITER_QUEUE_CAPACITY as u64
        );
        assert!(
            writer.begin_flush().is_err(),
            "there is deliberately no second slot for a separate barrier"
        );
        let rejected = observation(writer.capture_identity(), 200);
        assert!(matches!(
            writer.begin_settings_observation(rejected).unwrap(),
            CaptureSettingsObservationAdmission::AdmissionLost {
                dropped_messages: 1
            }
        ));
        assert_eq!(
            writer.state.settings_observations.load(Ordering::Acquire),
            1,
            "rejected admission must release its observation quota"
        );
        assert!(matches!(
            release.recv().unwrap(),
            Ok(CaptureWriterBarrierResult::Flushed)
        ));
        receipt.wait().unwrap();
        assert!(
            !writer.monitor().status().failed,
            "queue rejection must not report a failed writer"
        );
        if database_backed {
            assert_eq!(
                database
                    .live_capture_settings_observations(writer.live_capture_id.unwrap())
                    .unwrap(),
                [expected]
            );
            assert!(matches!(
                writer.finish_without_export().unwrap(),
                CaptureWriterFinish::DatabaseFinished {
                    integrity: LiveCaptureIntegrity::Incomplete {
                        dropped_messages: 1
                    },
                    ..
                }
            ));
        } else {
            let capture =
                PevcapCapture::decode(&fs::read(path).unwrap(), PevcapEncoding::Jsonl).unwrap();
            assert_eq!(capture.settings_observations, [expected]);

            assert!(
                writer.finish_without_export().is_err(),
                "file completion still reports the deliberately rejected second observation"
            );
        }
        database.shutdown().unwrap();
    }
}

#[test]
fn admitted_settings_snapshot_never_acknowledges_database_write_failure() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let writer = CaptureWriter::start_with_database(
        directory.path().join("capture.jsonl"),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "unsupported",
        None,
        &metadata(),
        database.clone(),
    )
    .unwrap();
    writer.flush().unwrap();
    database.shutdown().unwrap();
    let CaptureSettingsObservationAdmission::Accepted(receipt) = writer
        .begin_settings_observation(observation(writer.capture_identity(), 100))
        .unwrap()
    else {
        panic!("live queue admission should succeed before the failed database write");
    };
    let error = receipt.wait().unwrap_err();
    assert!(error.contains("could not persist live capture event"));
    assert!(writer.monitor().status().failed);
    assert!(writer.finish_without_export().is_err());
}
