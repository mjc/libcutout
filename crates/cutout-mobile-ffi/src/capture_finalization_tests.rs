//! Native admission loss must remain distinct from a stopped capture writer.

use super::*;
use std::{fs, path::PathBuf};

fn location() -> MobilePhoneLocationSampleDto {
    MobilePhoneLocationSampleDto {
        wall_clock_unix_ms: 1_700_000_000_010,
        source_timestamp_unix_seconds: None,
        latitude_degrees: 39.7,
        longitude_degrees: -104.9,
        altitude_meters: 1.0,
        horizontal_accuracy_meters: Some(1.0),
        vertical_accuracy_meters: None,
        speed_meters_per_second: Some(0.0),
        speed_accuracy_meters_per_second: None,
        course_degrees: None,
        course_accuracy_degrees: None,
    }
}

fn start_capture() -> (
    PathBuf,
    Arc<RideDatabaseHandle>,
    Arc<MobilePevcapCaptureBuilder>,
) {
    let directory = std::env::temp_dir().join(format!("cutout-finalization-{}", Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let database =
        open_ride_database(directory.join("ride.sqlite").to_string_lossy().into_owned()).unwrap();
    let builder = MobilePevcapCaptureBuilder::new(
        MobileWallClockUnixMillisDto {
            milliseconds: 1_700_000_000_000,
        },
        "test".into(),
        None,
    );
    assert!(builder.set_database(Arc::clone(&database)));
    assert!(builder.set_capture_start_monotonic_ms(100));
    assert!(
        builder.start_writer(
            directory
                .join("capture.jsonl")
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(
        builder.flush_writer_outcome(),
        MobileCaptureFlushOutcomeDto::Flushed
    );
    (directory, database, builder)
}

fn started_lifecycle() -> (Arc<CutoutSessionStateHandle>, MobileCaptureGenerationDto) {
    let lifecycle = CutoutSessionStateHandle::new();
    let generation = lifecycle
        .begin_capture(MobileCaptureOriginDto::Manual)
        .unwrap();
    assert!(lifecycle.capture_writer_started(generation));
    (lifecycle, generation)
}

#[test]
fn oversized_mobile_location_batch_reports_loss_and_replays_same_durable_receipt() {
    let (directory, database, builder) = start_capture();
    let (lifecycle, generation) = started_lifecycle();
    let outcome = builder.record_location_samples(
        MobileMonotonicMillisDto { milliseconds: 120 },
        MobileWallClockUnixMillisDto {
            milliseconds: 1_700_000_000_020,
        },
        vec![location(); persistence::CAPTURE_LOCATION_BATCH_CAPACITY + 1],
    );
    assert_ne!(outcome, MobileCaptureWriteOutcomeDto::Accepted);
    assert_ne!(
        lifecycle.apply_capture_write_outcome(Some(generation), outcome),
        MobileCaptureWriteDecisionDto::FailWriter,
        "known admission loss does not stop a usable writer"
    );
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::Recording
    );
    assert!(
        builder.flush_writer_outcome() == MobileCaptureFlushOutcomeDto::Flushed,
        "admission loss does not poison a later flush"
    );
    assert_eq!(
        builder.record_link_up(MobileMonotonicMillisDto { milliseconds: 130 }, None),
        MobileCaptureWriteOutcomeDto::Accepted,
        "data admitted after the lost batch must still be retained"
    );
    let first = builder.finish_writer_outcome();
    let MobileCaptureFinishOutcomeDto::DatabaseFinished {
        live_capture_id,
        integrity:
            MobileCaptureIntegrityDto::Incomplete {
                dropped_messages: 65,
            },
        jsonl_export: MobileCaptureJsonlExportDto::NotAttempted,
        status,
    } = &first
    else {
        panic!("lost location batch must finalize as incomplete: {first:?}");
    };
    assert_eq!(status.dropped_messages, 65);
    assert_eq!(status.queued_messages, 0);
    assert!(!status.failed);
    assert_eq!(builder.finish_writer_outcome(), first);
    assert_eq!(
        builder.finish_writer_and_publish_capture(None).finish,
        first
    );
    assert!(!directory.join("capture.jsonl").exists());

    let connection = rusqlite::Connection::open(directory.join("ride.sqlite")).unwrap();
    let snapshot: (String, String, u64, u64) = connection
        .query_row(
            "SELECT state, integrity, dropped_messages, next_sequence
             FROM live_capture_sessions WHERE capture_id = ?1",
            [live_capture_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(snapshot, ("finished".into(), "incomplete".into(), 65, 1));
    drop(connection);
    database.shutdown().unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn native_admission_loss_does_not_fail_usable_capture_lifecycle() {
    let (directory, database, builder) = start_capture();
    let (lifecycle, generation) = started_lifecycle();
    let locations = vec![
        PevcapLocationSample::from_raw_observation(
            MonotonicTimestamp::new(20),
            location().pevcap_location(),
            None,
            None,
        );
        persistence::CAPTURE_LOCATION_BATCH_CAPACITY + 1
    ];
    let outcome = builder
        .writer_ingress
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .record_location_batch(&locations);
    assert_ne!(outcome, CaptureWriteOutcome::Accepted);
    assert_ne!(
        lifecycle.apply_capture_write_outcome(Some(generation), outcome.into()),
        MobileCaptureWriteDecisionDto::FailWriter,
        "native admission rejection is not a worker failure"
    );
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::Recording
    );
    assert_eq!(
        builder.flush_writer_outcome(),
        MobileCaptureFlushOutcomeDto::Flushed
    );
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::DatabaseFinished {
            integrity: MobileCaptureIntegrityDto::Incomplete {
                dropped_messages: 65
            },
            ..
        }
    ));
    database.shutdown().unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn capture_writer_status_uses_fatal_state_without_inventing_failure_from_text_or_loss() {
    let (lifecycle, generation) = started_lifecycle();
    let diagnostic = MobileCaptureWriterStatusDto {
        dropped_messages: 65,
        last_error: Some("diagnostic text is not fatal evidence".into()),
        ..MobileCaptureWriterStatusDto::default()
    };
    assert_eq!(
        lifecycle.apply_capture_writer_status(Some(generation), diagnostic),
        MobileCaptureWriteDecisionDto::Continue
    );
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::Recording
    );
    assert_eq!(
        lifecycle.apply_capture_writer_status(
            Some(generation),
            MobileCaptureWriterStatusDto {
                failed: true,
                last_error: None,
                ..MobileCaptureWriterStatusDto::default()
            }
        ),
        MobileCaptureWriteDecisionDto::FailWriter
    );
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::SaveFailed
    );
}

#[test]
fn capture_flush_rejection_is_retryable_and_only_a_durable_receipt_releases_transport() {
    let (lifecycle, generation) = started_lifecycle();
    assert_eq!(
        lifecycle
            .apply_capture_flush_outcome(Some(generation), MobileCaptureFlushOutcomeDto::Rejected),
        MobileCaptureWriteDecisionDto::Rejected
    );
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::Recording
    );
    let rejected = lifecycle.begin_capture_finish(generation).unwrap();
    assert_eq!(
        lifecycle
            .apply_capture_flush_outcome(Some(generation), MobileCaptureFlushOutcomeDto::Rejected),
        MobileCaptureWriteDecisionDto::Rejected
    );
    assert!(!lifecycle.finish_capture_flush(rejected, MobileCaptureFlushOutcomeDto::Rejected));
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::SaveFailed
    );
    let retry = lifecycle.begin_capture_finish(generation).unwrap();
    assert_eq!(
        lifecycle
            .apply_capture_flush_outcome(Some(generation), MobileCaptureFlushOutcomeDto::Flushed),
        MobileCaptureWriteDecisionDto::Continue
    );
    assert!(lifecycle.finish_capture_flush(retry, MobileCaptureFlushOutcomeDto::Flushed));
    let finalizing = lifecycle.capture_lifecycle_snapshot();
    assert!(!lifecycle.finish_capture_flush(rejected, MobileCaptureFlushOutcomeDto::Flushed));
    assert_eq!(lifecycle.capture_lifecycle_snapshot(), finalizing);
}

#[test]
fn capture_fatal_flush_during_save_preserves_finish_token_until_typed_rejection() {
    let (lifecycle, generation) = started_lifecycle();
    let token = lifecycle.begin_capture_finish(generation).unwrap();
    let failed = MobileCaptureFlushOutcomeDto::Failed {
        message: "accepted barrier lost its reply".into(),
    };
    assert_eq!(
        lifecycle.apply_capture_flush_outcome(Some(generation), failed.clone()),
        MobileCaptureWriteDecisionDto::FailWriter
    );
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::Saving
    );
    assert!(!lifecycle.finish_capture_flush(token, failed));
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::SaveFailed
    );
}

#[test]
fn capture_health_receipts_from_a_retired_writer_cannot_affect_its_replacement() {
    let (lifecycle, old) = started_lifecycle();
    assert!(lifecycle.retire_capture_writer(old));
    let replacement = lifecycle
        .begin_capture(MobileCaptureOriginDto::Manual)
        .unwrap();
    assert!(lifecycle.capture_writer_started(replacement));
    let current = lifecycle.capture_lifecycle_snapshot();
    assert_eq!(
        lifecycle.apply_capture_flush_outcome(Some(old), MobileCaptureFlushOutcomeDto::Flushed),
        MobileCaptureWriteDecisionDto::StaleFailure
    );
    assert_eq!(
        lifecycle.apply_capture_writer_status(
            Some(old),
            MobileCaptureWriterStatusDto {
                failed: true,
                ..MobileCaptureWriterStatusDto::default()
            }
        ),
        MobileCaptureWriteDecisionDto::StaleFailure
    );
    assert_eq!(
        lifecycle.apply_capture_writer_status(None, MobileCaptureWriterStatusDto::default()),
        MobileCaptureWriteDecisionDto::StaleFailure
    );
    assert_eq!(lifecycle.capture_lifecycle_snapshot(), current);
}

#[test]
fn capture_durable_flush_after_observation_loss_preserves_incomplete_integrity() {
    let (directory, database, builder) = start_capture();
    let (lifecycle, generation) = started_lifecycle();
    assert!(matches!(
        builder.record_location_samples(
            MobileMonotonicMillisDto { milliseconds: 120 },
            MobileWallClockUnixMillisDto {
                milliseconds: 1_700_000_000_020
            },
            vec![location(); persistence::CAPTURE_LOCATION_BATCH_CAPACITY + 1],
        ),
        MobileCaptureWriteOutcomeDto::AdmissionLost {
            dropped_messages: 65
        }
    ));
    let token = lifecycle.begin_capture_finish(generation).unwrap();
    let outcome = builder.flush_writer_outcome();
    assert_eq!(outcome, MobileCaptureFlushOutcomeDto::Flushed);
    assert_eq!(
        lifecycle.apply_capture_flush_outcome(Some(generation), outcome.clone()),
        MobileCaptureWriteDecisionDto::Continue
    );
    assert!(lifecycle.finish_capture_flush(token, outcome));
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::DatabaseFinished {
            integrity: MobileCaptureIntegrityDto::Incomplete {
                dropped_messages: 65
            },
            ..
        }
    ));
    assert_eq!(
        builder.flush_writer_outcome(),
        MobileCaptureFlushOutcomeDto::Rejected
    );
    database.shutdown().unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn capture_sql_flush_failure_is_classified_in_rust_and_retains_first_cause() {
    let (directory, database, builder) = start_capture();
    let (lifecycle, generation) = started_lifecycle();
    let sql = rusqlite::Connection::open(directory.join("ride.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER deny_capture_flush BEFORE INSERT ON live_capture_events BEGIN SELECT RAISE(ABORT, 'test capture append failed'); END;").unwrap();
    assert_eq!(
        builder.record_link_up(MobileMonotonicMillisDto { milliseconds: 130 }, None),
        MobileCaptureWriteOutcomeDto::Accepted
    );
    let failure = builder.flush_writer_outcome();
    let MobileCaptureFlushOutcomeDto::Failed { message } = &failure else {
        panic!("SQL failure requires a fatal flush receipt: {failure:?}");
    };
    assert!(message.contains("test capture append failed"));
    assert_eq!(
        lifecycle.apply_capture_flush_outcome(Some(generation), failure.clone()),
        MobileCaptureWriteDecisionDto::FailWriter
    );
    assert_eq!(
        lifecycle
            .capture_lifecycle_snapshot()
            .attempt
            .unwrap()
            .stage,
        MobileCaptureStageDto::SaveFailed
    );
    assert_eq!(builder.flush_writer_outcome(), failure);
    assert_eq!(
        lifecycle.apply_capture_writer_status(Some(generation), builder.writer_status()),
        MobileCaptureWriteDecisionDto::FailWriter
    );
    assert!(matches!(
        builder.finish_writer_outcome(),
        MobileCaptureFinishOutcomeDto::Failed { .. }
    ));
    drop(sql);
    database.shutdown().unwrap();
    fs::remove_dir_all(directory).unwrap();
}
