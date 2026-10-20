#![cfg_attr(test, allow(clippy::disallowed_macros))]

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use cutout_application::capture::{
    CaptureStartBackend, CaptureStartError, CaptureStartMetadata, CaptureStartRequest,
    prepare_capture_writer,
};
use cutout_core::CaptureOrigin;

#[derive(Clone, Default)]
struct Backend {
    consent: Arc<AtomicBool>,
    starts: Arc<AtomicUsize>,
    fail_start: bool,
}

impl CaptureStartBackend for Backend {
    type Writer = CaptureStartMetadata;

    fn prepare_writer(
        &self,
        metadata: CaptureStartMetadata,
    ) -> Result<Self::Writer, CaptureStartError> {
        Ok(metadata)
    }
    fn automatic_recording_enabled(&self) -> bool {
        self.consent.load(Ordering::SeqCst)
    }
    fn attach_database(&self, _: &Self::Writer) -> bool {
        true
    }
    fn start_writer(&self, _: &Self::Writer, _: &str, _: u64) -> bool {
        self.starts.fetch_add(1, Ordering::SeqCst);
        !self.fail_start
    }
}

fn request(origin: CaptureOrigin) -> CaptureStartRequest {
    CaptureStartRequest {
        wall_clock_unix_seconds: 1_700_000_000.125,
        platform_id: "wheel".into(),
        advertised_services: vec![vec![1; 16]],
        directory_path: std::env::temp_dir().to_string_lossy().into_owned(),
        filename_nonce: "3F2504E0-4F89-41D3-9A0C-0305E82C3301".into(),
        source: "rust-client".into(),
        reason: "ride".into(),
        evidence: "verified".into(),
        annotations: vec!["user_note=one=two\nthree".into()],
        origin,
    }
}

#[test]
fn rust_client_gets_normalized_metadata_and_one_start_attempt() {
    let backend = Backend::default();
    let starts = backend.starts.clone();
    let prepared = prepare_capture_writer(request(CaptureOrigin::Manual), backend).unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    let started = prepared.start(100).unwrap();
    assert_eq!(started.writer.wall_clock_unix_ms, 1_700_000_000_125);
    assert_eq!(
        started.writer.annotations.last().unwrap(),
        "user_note=one two three"
    );
    assert!(
        started
            .path
            .ends_with("cutout-btle-capture-1700000000-3F2504E0-4F89-41D3-9A0C-0305E82C3301.jsonl")
    );
    assert_eq!(
        prepared.start(101).unwrap_err(),
        CaptureStartError::AlreadyStarted
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[test]
fn consent_withdrawal_consumes_preparation_without_starting_writer() {
    let backend = Backend::default();
    backend.consent.store(true, Ordering::SeqCst);
    let prepared =
        prepare_capture_writer(request(CaptureOrigin::Automatic), backend.clone()).unwrap();
    backend.consent.store(false, Ordering::SeqCst);
    assert_eq!(
        prepared.start(100).unwrap_err(),
        CaptureStartError::AutomaticRecordingDisabled
    );
    backend.consent.store(true, Ordering::SeqCst);
    assert_eq!(
        prepared.start(101).unwrap_err(),
        CaptureStartError::AlreadyStarted
    );
    assert_eq!(backend.starts.load(Ordering::SeqCst), 0);
}

#[test]
fn failed_effect_consumes_preparation_and_cannot_be_retried() {
    let backend = Backend {
        fail_start: true,
        ..Backend::default()
    };
    let prepared = prepare_capture_writer(request(CaptureOrigin::Manual), backend.clone()).unwrap();
    assert_eq!(
        prepared.start(100).unwrap_err(),
        CaptureStartError::WriterStartFailed
    );
    assert_eq!(
        prepared.start(101).unwrap_err(),
        CaptureStartError::AlreadyStarted
    );
    assert_eq!(backend.starts.load(Ordering::SeqCst), 1);
}

#[test]
fn contradictory_annotations_are_rejected_before_writer_effects() {
    let backend = Backend::default();
    let mut input = request(CaptureOrigin::Manual);
    input.annotations.push("capture_privacy=public".into());
    assert_eq!(
        prepare_capture_writer(input, backend.clone()).unwrap_err(),
        CaptureStartError::InvalidMetadata
    );
    assert_eq!(backend.starts.load(Ordering::SeqCst), 0);
}

#[test]
fn default_startup_uses_the_shared_rust_writer_without_native_adapters() {
    let directory =
        std::env::temp_dir().join(format!("cutout-start-client-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let mut input = request(CaptureOrigin::Manual);
    input.directory_path = directory.to_string_lossy().into_owned();
    let prepared = cutout_application::capture::prepare_capture_start(input, None).unwrap();
    let started = prepared.start(100).unwrap();
    assert!(matches!(
        started.writer.finish(true),
        cutout_application::capture_writer::CaptureFinishState::Complete(Ok(_))
    ));
    assert_eq!(
        started.writer.completed_artifact().unwrap().path(),
        std::path::Path::new(&started.path)
    );
    let capture = cutout_core::PevcapCapture::decode(
        &std::fs::read(&started.path).unwrap(),
        cutout_core::PevcapEncoding::Jsonl,
    )
    .unwrap();
    assert_eq!(
        capture.header.wall_clock_start_unix_ms.as_milliseconds(),
        1_700_000_000_125
    );
    assert!(
        capture
            .header
            .annotations
            .iter()
            .any(|annotation| annotation == "source=rust-client")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
