#![cfg_attr(test, allow(clippy::disallowed_macros))]

use std::{fs, path::PathBuf};

use cutout_application::capture_writer::{CaptureFinishState, CaptureWriterService};
use cutout_core::{
    CaptureOrigin, MonotonicTimestamp, PevcapLocationSample, PevcapPhoneLocation, PevcapRecord,
    WallClockUnixTimestamp,
};
use libcutout_persistence::{
    CaptureFlushOutcome, CaptureRecordAdmission, CaptureWriteOutcome, CaptureWriterFinish,
};

#[test]
fn rust_client_owns_capture_timestamps_admission_and_durable_finish() {
    let service = CaptureWriterService::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "rust-client".into(),
        None,
    );
    let base = std::env::temp_dir().join(format!("cutout-app-capture-{}", uuid::Uuid::new_v4()));
    let path = PathBuf::from(format!("{}.jsonl", base.display()));

    assert!(service.set_start_monotonic_ms(100));
    assert!(!service.set_start_monotonic_ms(101));
    assert!(service.set_origin(CaptureOrigin::Manual));
    assert!(service.start(path.clone()));
    assert_eq!(
        service.relative_timestamp(150),
        Ok(MonotonicTimestamp::new(50))
    );
    assert!(service.relative_timestamp(99).is_err());
    assert_eq!(
        service.record(
            PevcapRecord::link_up(MonotonicTimestamp::new(50), None),
            CaptureRecordAdmission::EveryObservation,
        ),
        CaptureWriteOutcome::Accepted,
    );
    assert!(matches!(
        service.finish(true),
        CaptureFinishState::Complete(Ok(_))
    ));
    let artifact = service
        .completed_artifact()
        .expect("finished capture artifact");
    assert_eq!(artifact.path(), path);
    assert!(matches!(
        service.finish(true),
        CaptureFinishState::Complete(Ok(CaptureWriterFinish::FileSaved(_)))
    ));
    fs::remove_file(path).expect("remove generated artifact");
}

#[test]
fn location_batch_uses_the_live_ingress_and_failed_start_stays_failed() {
    let service = CaptureWriterService::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "rust-client".into(),
        None,
    );
    assert!(service.set_start_monotonic_ms(100));
    let missing = std::env::temp_dir().join(format!(
        "cutout-missing-parent-{}/capture.jsonl",
        uuid::Uuid::new_v4()
    ));
    assert!(!service.start(missing));
    match service.flush() {
        CaptureFlushOutcome::Failed { message } => assert!(!message.is_empty()),
        outcome => panic!("failed start was reported as {outcome:?}"),
    }

    let service = CaptureWriterService::new(
        WallClockUnixTimestamp::new(1_700_000_000_000),
        "rust-client".into(),
        None,
    );
    let path = std::env::temp_dir().join(format!(
        "cutout-app-location-{}.jsonl",
        uuid::Uuid::new_v4()
    ));
    assert!(service.set_start_monotonic_ms(100));
    assert!(service.start(path.clone()));
    let location = PevcapLocationSample::new(
        MonotonicTimestamp::new(50),
        PevcapPhoneLocation {
            wall_clock_unix_ms: 1_700_000_000_050,
            latitude_degrees: 39.7,
            longitude_degrees: -104.9,
            altitude_meters: 1_600.0,
            horizontal_accuracy_meters: Some(2.0),
            vertical_accuracy_meters: None,
            speed_meters_per_second: None,
            speed_accuracy_meters_per_second: None,
            course_degrees: None,
            course_accuracy_degrees: None,
        },
        None,
        None,
    )
    .expect("location sample");
    assert_eq!(
        service.record_location_batch(&[location]),
        CaptureWriteOutcome::Accepted
    );
    assert_eq!(
        service.reject_location_batch(0),
        CaptureWriteOutcome::Accepted
    );
    let _ = service.finish(true);
    fs::remove_file(&path).ok();
}
