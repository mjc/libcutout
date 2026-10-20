#![cfg_attr(test, allow(clippy::disallowed_macros))]

use std::{fs, path::PathBuf};

use cutout_application::capture_writer::{CaptureFinishState, CaptureWriterService};
use cutout_core::{CaptureOrigin, MonotonicTimestamp, PevcapRecord, WallClockUnixTimestamp};
use libcutout_persistence::{CaptureRecordAdmission, CaptureWriteOutcome, CaptureWriterFinish};

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
