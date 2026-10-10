use super::*;

#[test]
fn repeated_unchanged_database_flush_has_no_wal_writes() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let writer = CaptureWriter::start_with_database(
        directory.path().join("capture.jsonl"),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "wheel-a",
        None,
        &CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        },
        database,
    )
    .unwrap();
    writer.flush().unwrap();
    let observer = rusqlite::Connection::open(&database_path).unwrap();
    observer
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    for _ in 0..100 {
        writer.flush().unwrap();
    }
    let wal = database_path.with_extension("sqlite-wal");
    assert_eq!(
        fs::metadata(wal).unwrap().len(),
        0,
        "unchanged flushes must not rewrite already durable headers"
    );
    writer.finish().unwrap();
}

#[test]
fn database_completion_retains_exact_data_without_creating_an_export() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.jsonl");
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let writer = CaptureWriter::start_with_database(
        path.clone(),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "wheel-a",
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
    let finish = writer.finish_without_export().unwrap();
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id,
        integrity,
        jsonl_export,
        status,
        ..
    } = finish
    else {
        panic!("database writer must retain a database receipt");
    };
    assert_eq!(integrity, LiveCaptureIntegrity::Complete);
    assert_eq!(jsonl_export, CaptureJsonlExport::NotAttempted);
    assert!(!path.exists());
    assert_eq!(status.physical_bytes_written, 0);
    drop(database);
    let reopened = RideDatabase::open(&database_path).unwrap();
    let first = reopened
        .live_capture_page(live_capture_id, None, QueryLimit::new(32).unwrap())
        .unwrap();
    assert_eq!(first.state, LiveCaptureState::Finished);
    assert_eq!(first.integrity, LiveCaptureIntegrity::Complete);
    assert_eq!(first.next_sequence, 65);
    assert!(
        String::from_utf8(first.header_json)
            .unwrap()
            .contains("capture_label=ride_stop")
    );
    let final_page = reopened
        .live_capture_page(live_capture_id, Some(63), QueryLimit::new(32).unwrap())
        .unwrap();
    assert_eq!(final_page.events.len(), 1);
    assert_eq!(
        final_page.events[0].payload,
        tests::stationary_record(65)
            .to_jsonl_line()
            .unwrap()
            .into_bytes()
    );
}

#[test]
fn deferred_export_matches_original_bytes_and_history_survives_reopen() {
    use cutout_core::CaptureOrigin;
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let path = directory.path().join("automatic.jsonl");
    let writer = CaptureWriter::start_with_database(
        path.clone(),
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "wheel-a",
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
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id,
        receipt,
        status,
        ..
    } = writer.finish_without_export().unwrap()
    else {
        panic!("database completion required")
    };
    assert_eq!(status.physical_bytes_written, 0);
    assert!(!path.exists());
    assert_publication_retry_is_idempotent(&database, &receipt);
    drop(database);
    let reopened = RideDatabase::open(&database_path).unwrap();
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(history.captures.len(), 1);
    let capture = &history.captures[0];
    assert_eq!(capture.id, live_capture_id);
    assert_eq!(capture.event_count, 65);
    assert_eq!(capture.integrity, LiveCaptureIntegrity::Complete);
    let recording = capture.recording.as_ref().unwrap();
    assert_eq!(recording.artifact_id, receipt.artifact_id());
    assert_eq!(recording.advertised_name.as_deref(), Some("NF2557"));
    assert_eq!(recording.origin, CaptureOrigin::Automatic);
    let digest = reopened
        .export_live_capture(live_capture_id, &path)
        .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(digest, hex::encode(Sha256::digest(&bytes)));
    let first = reopened
        .live_capture_page(live_capture_id, None, QueryLimit::new(100).unwrap())
        .unwrap();
    let expected = std::iter::once(first.header_json)
        .chain(first.events.into_iter().map(|event| event.payload))
        .flat_map(|mut line| {
            line.push(b'\n');
            line
        })
        .collect::<Vec<_>>();
    assert_eq!(bytes, expected);
    assert_eq!(bytes.len() as u64, status.bytes_written);
    assert!(
        reopened
            .export_live_capture(live_capture_id, &path)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), expected);
    assert_eq!(
        reopened
            .list_live_capture_history(None, QueryLimit::new(1).unwrap())
            .unwrap(),
        history
    );
}

fn assert_publication_retry_is_idempotent(database: &RideDatabase, receipt: &SavedDatabaseCapture) {
    use cutout_core::CaptureOrigin;
    for published_at in [1_700_000_001_000, 1_700_000_002_000] {
        database
            .retain_finished_database_capture(
                receipt,
                CaptureOrigin::Automatic,
                Some("NF2557"),
                WallClockUnixTimestamp::new(published_at),
            )
            .unwrap();
    }
    assert!(matches!(
        database.retain_finished_database_capture(
            receipt,
            CaptureOrigin::Manual,
            Some("NF2557"),
            WallClockUnixTimestamp::new(1_700_000_001_000)
        ),
        Err(crate::StorageError::CaptureIdentityConflict)
    ));
}

#[test]
fn interrupted_capture_is_visible_and_exports_without_completeness_promotion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&path).unwrap();
    let header = capture_header(
        WallClockUnixTimestamp::new(1234),
        "wheel-a",
        None,
        &CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec![],
        },
    )
    .unwrap();
    let id = LiveCaptureId::new();
    database
        .begin_live_capture_with_id(id, header.to_jsonl_line().unwrap().into_bytes(), 1234)
        .unwrap();
    database
        .append_live_capture_record(
            id,
            tests::stationary_record(1),
            tests::stationary_record(1)
                .to_jsonl_line()
                .unwrap()
                .into_bytes(),
        )
        .unwrap();
    database.shutdown().unwrap();
    let reopened = RideDatabase::open(&path).unwrap();
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(history.captures[0].state, LiveCaptureState::Interrupted);
    assert_eq!(history.captures[0].integrity, LiveCaptureIntegrity::Unknown);
    assert_eq!(history.captures[0].recording, None);
    let output = directory.path().join("interrupted.jsonl");
    let digest = reopened.export_live_capture(id, &output).unwrap();
    assert_eq!(
        digest,
        hex::encode(Sha256::digest(fs::read(output).unwrap()))
    );
    assert_eq!(
        reopened
            .list_live_capture_history(None, QueryLimit::new(1).unwrap())
            .unwrap(),
        history
    );
}

#[test]
fn inactive_capture_history_pages_stably_and_export_rejects_active_sources() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let metadata = CaptureMetadata {
        advertised_services: vec![],
        gatt_fingerprints: vec![],
        resolved_identity: None,
        annotations: vec![],
    };
    for at in [1000, 1000, 2000] {
        CaptureWriter::start_with_database(
            directory.path().join(format!("{}.jsonl", Uuid::new_v4())),
            WallClockUnixTimestamp::new(at),
            "wheel-a",
            None,
            &metadata,
            database.clone(),
        )
        .unwrap()
        .finish_without_export()
        .unwrap();
    }
    let id = LiveCaptureId::new();
    let header = capture_header(
        WallClockUnixTimestamp::new(3000),
        "wheel-active",
        None,
        &metadata,
    )
    .unwrap();
    database
        .begin_live_capture_with_id(id, header.to_jsonl_line().unwrap().into_bytes(), 3000)
        .unwrap();
    let output = directory.path().join("active.jsonl");
    assert!(database.export_live_capture(id, &output).is_err());
    assert!(!output.exists());
    let all = database
        .list_live_capture_history(None, QueryLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(all.captures.len(), 3);
    assert_eq!(all.captures[0].started_at_ms, 2000);
    let mut cursor = None;
    let mut ids = Vec::new();
    loop {
        let page = database
            .list_live_capture_history(cursor, QueryLimit::new(1).unwrap())
            .unwrap();
        ids.extend(page.captures.iter().map(|capture| capture.id));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        ids,
        all.captures
            .iter()
            .map(|capture| capture.id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn incomplete_database_capture_remains_exportable_without_promoting_integrity() {
    let directory = tempfile::tempdir().unwrap();
    let database = RideDatabase::open(&directory.path().join("ride.sqlite")).unwrap();
    let output = directory.path().join("capture.jsonl");
    let writer = CaptureWriter::start_with_database(
        output.clone(),
        WallClockUnixTimestamp::new(1000),
        "wheel-a",
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
    writer
        .state
        .incomplete_messages
        .fetch_add(1, Ordering::AcqRel);
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id,
        integrity,
        ..
    } = writer.finish_without_export().unwrap()
    else {
        panic!("database receipt required")
    };
    assert_eq!(
        integrity,
        LiveCaptureIntegrity::Incomplete {
            dropped_messages: 1
        }
    );
    database
        .export_live_capture(live_capture_id, &output)
        .unwrap();
    let history = database
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(history.captures[0].integrity, integrity);
}

/// Reproducible process fixture: run the built test binary with --exact --nocapture. The two
/// policies ingest the same events; only finalization is timed and SHA/data equality is checked.
#[test]
fn database_only_finalization_avoids_export_bytes_with_identical_deferred_contents() {
    let directory = tempfile::tempdir().unwrap();
    let metadata = CaptureMetadata {
        advertised_services: vec![],
        gatt_fingerprints: vec![],
        resolved_identity: None,
        annotations: vec!["capture_label=ride_start".into()],
    };
    let mut exports = Vec::new();
    for deferred in [false, true] {
        let database =
            RideDatabase::open(&directory.path().join(format!("{deferred}.sqlite"))).unwrap();
        let output = directory.path().join(format!("{deferred}.jsonl"));
        let writer = CaptureWriter::start_with_database(
            output.clone(),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "wheel-a",
            None,
            &metadata,
            database.clone(),
        )
        .unwrap();
        for at in 1..=1024 {
            assert_eq!(
                writer.try_send_record(tests::stationary_record(at)),
                CaptureWriteOutcome::Accepted
            );
            if at % 32 == 0 {
                writer.flush().unwrap();
            }
        }
        writer.flush().unwrap();
        let start = Instant::now();
        let completion = if deferred {
            writer.finish_without_export()
        } else {
            writer.finish()
        }
        .unwrap();
        let elapsed = start.elapsed();
        let CaptureWriterFinish::DatabaseFinished {
            live_capture_id,
            status,
            ..
        } = completion
        else {
            panic!("database receipt required")
        };
        if deferred {
            assert_eq!(status.physical_bytes_written, 0);
            assert!(!output.exists());
            database
                .export_live_capture(live_capture_id, &output)
                .unwrap();
        } else {
            assert_eq!(
                status.physical_bytes_written,
                fs::metadata(&output).unwrap().len()
            );
        }
        let bytes = fs::read(output).unwrap();
        eprintln!(
            "finalize deferred={deferred} events=1024 physical_export_bytes={} elapsed_us={} jsonl_bytes={} digest={}",
            status.physical_bytes_written,
            elapsed.as_micros(),
            bytes.len(),
            hex::encode(Sha256::digest(&bytes))
        );
        exports.push(bytes);
        database.shutdown().unwrap();
    }
    assert_eq!(exports[0], exports[1]);
}

#[test]
fn v38_migration_preserves_raw_capture_data_and_history_does_not_scan_events() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let writer = CaptureWriter::start_with_database(
        directory.path().join("capture.jsonl"),
        WallClockUnixTimestamp::new(1000),
        "wheel-a",
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
    assert_eq!(
        writer.try_send_record(tests::stationary_record(1)),
        CaptureWriteOutcome::Accepted
    );
    let CaptureWriterFinish::DatabaseFinished {
        live_capture_id, ..
    } = writer.finish_without_export().unwrap()
    else {
        panic!("database receipt required")
    };
    database.shutdown().unwrap();
    let observer = rusqlite::Connection::open(&database_path).unwrap();
    observer.execute_batch("ALTER TABLE live_capture_sessions DROP COLUMN recording_context_json; PRAGMA user_version = 38;").unwrap();
    // Bad compressed bytes must not be decoded by a metadata-only migration or history query.
    observer.execute("UPDATE live_capture_events SET payload = X'00', payload_encoding = 1, payload_original_bytes = 512 WHERE capture_id = ?1",
        [live_capture_id.to_string()]).unwrap();
    drop(observer);
    let reopened = RideDatabase::open(&database_path).unwrap();
    let history = reopened
        .list_live_capture_history(None, QueryLimit::new(1).unwrap())
        .unwrap();
    assert_eq!(history.captures[0].id, live_capture_id);
    assert_eq!(history.captures[0].event_count, 1);
    let observer = rusqlite::Connection::open(&database_path).unwrap();
    let version: i64 = observer
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 39);
    let payload: Vec<u8> = observer
        .query_row(
            "SELECT payload FROM live_capture_events WHERE capture_id = ?1",
            [live_capture_id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(payload, vec![0]);
    let output = directory.path().join("corrupt.jsonl");
    assert!(
        reopened
            .export_live_capture(live_capture_id, &output)
            .is_err()
    );
    assert!(!output.exists());
    assert_eq!(
        reopened
            .list_live_capture_history(None, QueryLimit::new(1).unwrap())
            .unwrap(),
        history
    );
}

#[test]
fn database_terminal_failure_rolls_back_closed_labels_with_completion_state() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("ride.sqlite");
    let database = RideDatabase::open(&database_path).unwrap();
    let writer = CaptureWriter::start_with_database(
        directory.path().join("capture.jsonl"),
        WallClockUnixTimestamp::new(1000),
        "wheel-a",
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
    writer.flush().unwrap();
    let observer = rusqlite::Connection::open(&database_path).unwrap();
    let before: Vec<u8> = observer
        .query_row("SELECT header_json FROM live_capture_sessions", [], |row| {
            row.get(0)
        })
        .unwrap();
    observer.execute_batch("CREATE TRIGGER deny_finish BEFORE UPDATE OF state ON live_capture_sessions BEGIN SELECT RAISE(ABORT, 'deny terminal state'); END;").unwrap();
    assert!(
        writer
            .finish_without_export()
            .unwrap_err()
            .contains("deny terminal state")
    );
    let (header, state): (Vec<u8>, String) = observer
        .query_row(
            "SELECT header_json, state FROM live_capture_sessions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "active");
    assert_eq!(
        header, before,
        "a failed terminal transaction must not durably close labels without completing the capture"
    );
}

#[test]
fn atomic_terminal_update_writes_one_wal_frame_instead_of_two() {
    let directory = tempfile::tempdir().unwrap();
    let mut frame_counts = Vec::new();
    for atomic in [false, true] {
        let path = directory.path().join(format!("{atomic}.sqlite"));
        let database = RideDatabase::open(&path).unwrap();
        let id = LiveCaptureId::new();
        let mut metadata = CaptureMetadata {
            advertised_services: vec![],
            gatt_fingerprints: vec![],
            resolved_identity: None,
            annotations: vec!["capture_label=ride_start".into()],
        };
        let header = capture_header(
            WallClockUnixTimestamp::new(1000),
            "wheel-a",
            None,
            &metadata,
        )
        .unwrap();
        database
            .begin_live_capture_with_id(id, header.to_jsonl_line().unwrap().into_bytes(), 1000)
            .unwrap();
        metadata.annotations.push("capture_label=ride_stop".into());
        let closed_header = capture_header(
            WallClockUnixTimestamp::new(1000),
            "wheel-a",
            None,
            &metadata,
        )
        .unwrap()
        .to_jsonl_line()
        .unwrap()
        .into_bytes();
        let observer = rusqlite::Connection::open(&path).unwrap();
        observer
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        if atomic {
            database
                .finalize_live_capture(
                    id,
                    closed_header.clone(),
                    2000,
                    LiveCaptureIntegrity::Complete,
                )
                .unwrap();
        } else {
            // Exact previous writer sequence, without optional export or re-import noise.
            database
                .update_live_capture_header(id, closed_header.clone())
                .unwrap();
            database
                .finish_live_capture_with_integrity(id, 2000, LiveCaptureIntegrity::Complete)
                .unwrap();
        }
        let page_size: u64 = observer
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .unwrap();
        let wal_bytes = fs::metadata(path.with_extension("sqlite-wal"))
            .unwrap()
            .len();
        let frames = (wal_bytes - 32) / (page_size + 24);
        eprintln!(
            "terminal atomic={atomic} wal_bytes={wal_bytes} wal_frames={frames} page_size={page_size}"
        );
        let snapshot = database
            .live_capture_page(id, None, QueryLimit::new(1).unwrap())
            .unwrap();
        assert_eq!(snapshot.header_json, closed_header);
        assert_eq!(snapshot.state, LiveCaptureState::Finished);
        assert_eq!(snapshot.integrity, LiveCaptureIntegrity::Complete);
        assert_eq!(snapshot.finished_at_ms, Some(2000));
        frame_counts.push(frames);
        drop(observer);
        database.shutdown().unwrap();
    }
    assert_eq!(frame_counts, [2, 1]);
}
