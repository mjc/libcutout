//! Capture startup validation and one-shot ownership through the existing writer.

use std::sync::Arc;

use cutout_application::capture as application;

use crate::{
    MobileCaptureOriginDto, MobilePevcapCaptureBuilder, MobileWallClockUnixMillisDto,
    RideDatabaseHandle,
};

/// Native observations for one capture startup; Rust owns normalization and admission.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileCaptureStartRequestDto {
    pub wall_clock_unix_seconds: f64,
    pub platform_id: String,
    pub advertised_services: Vec<Vec<u8>>,
    pub directory_path: String,
    pub filename_nonce: String,
    pub source: String,
    pub reason: String,
    pub evidence: String,
    pub annotations: Vec<String>,
    pub origin: MobileCaptureOriginDto,
}

/// Startup failure before a writer has been installed in the native adapter.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileCaptureStartError {
    #[error("capture wall clock is invalid")]
    InvalidWallClock,
    #[error("capture filename nonce is not a canonical UUID")]
    InvalidFilenameNonce,
    #[error("capture directory is not an absolute native path")]
    InvalidDirectory,
    #[error("capture startup metadata cannot be retained")]
    InvalidMetadata,
    #[error("capture startup has already been consumed")]
    AlreadyStarted,
    #[error("capture writer could not start")]
    WriterStartFailed,
    #[error("automatic diagnostic recording requires consent")]
    AutomaticRecordingDisabled,
}

impl From<application::CaptureStartError> for MobileCaptureStartError {
    fn from(error: application::CaptureStartError) -> Self {
        match error {
            application::CaptureStartError::InvalidWallClock => Self::InvalidWallClock,
            application::CaptureStartError::InvalidFilenameNonce => Self::InvalidFilenameNonce,
            application::CaptureStartError::InvalidDirectory => Self::InvalidDirectory,
            application::CaptureStartError::InvalidMetadata => Self::InvalidMetadata,
            application::CaptureStartError::AlreadyStarted => Self::AlreadyStarted,
            application::CaptureStartError::WriterStartFailed => Self::WriterStartFailed,
            application::CaptureStartError::AutomaticRecordingDisabled => {
                Self::AutomaticRecordingDisabled
            }
        }
    }
}

/// Admitted startup metadata. The writer remains untouched until `start` consumes it.
#[derive(Debug, uniffi::Object)]
pub struct MobilePreparedCaptureStart {
    inner: application::PreparedCaptureStart<application::CaptureWriterBackend>,
}

/// The existing writer and Rust-selected path for native presentation/ownership.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MobileStartedCaptureWriterDto {
    pub builder: Arc<MobilePevcapCaptureBuilder>,
    pub path: String,
}

/// Normalizes a native wall-clock observation without reading another clock.
#[must_use]
#[uniffi::export]
pub fn wall_clock_unix_milliseconds(seconds: f64) -> Option<MobileWallClockUnixMillisDto> {
    application::wall_clock_unix_milliseconds(seconds)
        .map(|milliseconds| MobileWallClockUnixMillisDto { milliseconds })
}

/// Formats one annotation without permitting a value to add delimiters or lines.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI string inputs are owned"
)]
pub fn format_pevcap_annotation(key: String, value: String) -> String {
    application::format_annotation(&key, &value)
}

/// Splits only the first delimiter, then normalizes each supplied component.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI string inputs are owned"
)]
pub fn sanitize_pevcap_annotation(annotation: String) -> String {
    application::sanitize_annotation(&annotation)
}

/// Validates all required startup evidence without creating files or database rows.
///
/// # Errors
/// Rejects an invalid clock/path/nonce, truncated service evidence, contradictory
/// startup annotations, missing automatic-diagnostic consent, or metadata that
/// consumes reserved label-closure space.
#[uniffi::export]
pub fn prepare_capture_writer(
    request: MobileCaptureStartRequestDto,
    database: Option<Arc<RideDatabaseHandle>>,
) -> Result<Arc<MobilePreparedCaptureStart>, MobileCaptureStartError> {
    let request = application::CaptureStartRequest {
        wall_clock_unix_seconds: request.wall_clock_unix_seconds,
        platform_id: request.platform_id,
        advertised_services: request.advertised_services,
        directory_path: request.directory_path,
        filename_nonce: request.filename_nonce,
        source: request.source,
        reason: request.reason,
        evidence: request.evidence,
        annotations: request.annotations,
        origin: request.origin.into(),
    };
    let inner = application::prepare_capture_start(
        request,
        database.map(|database| database.inner.clone()),
    )?;
    Ok(Arc::new(MobilePreparedCaptureStart { inner }))
}

#[uniffi::export]
impl MobilePreparedCaptureStart {
    /// Consumes admitted preparation once, including when the actual writer cannot start.
    ///
    /// # Errors
    /// Returns `AlreadyStarted` for reuse, `AutomaticRecordingDisabled` after consent
    /// withdrawal, and `WriterStartFailed` for writer admission failure.
    pub fn start(
        &self,
        monotonic_ms: u64,
    ) -> Result<MobileStartedCaptureWriterDto, MobileCaptureStartError> {
        let started = self.inner.start(monotonic_ms)?;
        Ok(MobileStartedCaptureWriterDto {
            builder: Arc::new(MobilePevcapCaptureBuilder {
                writer: started.writer,
            }),
            path: started.path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        thread,
    };

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("cutout-start-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn request(directory: &Directory) -> MobileCaptureStartRequestDto {
        MobileCaptureStartRequestDto {
            wall_clock_unix_seconds: 1_700_000_000.125,
            platform_id: "wheel".to_owned(),
            advertised_services: vec![vec![1; 16]],
            directory_path: directory.0.to_string_lossy().into_owned(),
            filename_nonce: "3F2504E0-4F89-41D3-9A0C-0305E82C3301".to_owned(),
            source: "ios-app".to_owned(),
            reason: "ride".to_owned(),
            evidence: "verified".to_owned(),
            annotations: vec!["user_note=one=two\nthree\r四".to_owned()],
            origin: MobileCaptureOriginDto::Manual,
        }
    }

    fn finished_capture(started: &MobileStartedCaptureWriterDto) -> cutout_core::PevcapCapture {
        assert!(matches!(
            started.builder.finish_writer_outcome(),
            crate::MobileCaptureFinishOutcomeDto::ArtifactAvailable { .. }
                | crate::MobileCaptureFinishOutcomeDto::DatabaseFinished {
                    jsonl_export: crate::MobileCaptureJsonlExportDto::Available { .. },
                    ..
                }
        ));
        cutout_core::PevcapCapture::decode(
            &fs::read(&started.path).unwrap(),
            cutout_core::PevcapEncoding::Jsonl,
        )
        .unwrap()
    }

    #[test]
    fn automatic_capture_requires_consent_before_writer_preparation() {
        let directory = Directory::new();
        let mut request = request(&directory);
        request.origin = MobileCaptureOriginDto::Automatic;
        assert!(prepare_capture_writer(request, None).is_err());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn automatic_capture_rechecks_persisted_consent_at_writer_start_and_fails_closed() {
        let directory = Directory::new();
        let database_path = directory.0.join("rides.sqlite");
        let database =
            crate::open_ride_database(database_path.to_string_lossy().into_owned()).unwrap();
        let mut request = request(&directory);
        request.origin = MobileCaptureOriginDto::Automatic;
        assert_eq!(
            prepare_capture_writer(request.clone(), Some(Arc::clone(&database))).err(),
            Some(MobileCaptureStartError::AutomaticRecordingDisabled)
        );
        database
            .inner
            .save_automatic_diagnostic_recording_enabled(true)
            .unwrap();
        let prepared =
            prepare_capture_writer(request.clone(), Some(Arc::clone(&database))).unwrap();
        database
            .inner
            .save_automatic_diagnostic_recording_enabled(false)
            .unwrap();
        assert_eq!(
            prepared.start(100).err(),
            Some(MobileCaptureStartError::AutomaticRecordingDisabled)
        );
        let sql = rusqlite::Connection::open(&database_path).unwrap();
        assert_eq!(
            sql.query_row("SELECT COUNT(*) FROM live_capture_sessions", [], |row| row
                .get::<_, u64>(
                0
            ))
            .unwrap(),
            0
        );
        drop(sql);
        database.shutdown().unwrap();
        assert_eq!(
            prepare_capture_writer(request, Some(database)).err(),
            Some(MobileCaptureStartError::AutomaticRecordingDisabled)
        );
    }

    #[test]
    fn capture_start_rejects_advertised_service_truncation() {
        let directory = Directory::new();
        let mut request = request(&directory);
        request.advertised_services =
            vec![vec![1; 16]; cutout_core::PEVCAP_MAX_ADVERTISED_SERVICES + 1];
        assert_eq!(
            prepare_capture_writer(request, None).err(),
            Some(MobileCaptureStartError::InvalidMetadata)
        );
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
        let mut request = self::request(&directory);
        request.advertised_services = vec![vec![1; 17]];
        assert_eq!(
            prepare_capture_writer(request, None).err(),
            Some(MobileCaptureStartError::InvalidMetadata)
        );
    }

    #[test]
    fn capture_start_rejects_requested_recording_policy_override() {
        let directory = Directory::new();
        let mut request = request(&directory);
        request.annotations = vec!["capture_recording_policy=every_observation".to_owned()];
        assert_eq!(
            prepare_capture_writer(request, None).err(),
            Some(MobileCaptureStartError::InvalidMetadata)
        );
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn capture_start_wall_clock_normalization_rejects_invalid_and_truncates_once() {
        for seconds in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.001,
            f64::MAX,
            18_446_744_073_709_551_616.0 / 1_000.0,
        ] {
            assert_eq!(wall_clock_unix_milliseconds(seconds), None);
            let directory = Directory::new();
            let mut request = request(&directory);
            request.wall_clock_unix_seconds = seconds;
            assert_eq!(
                prepare_capture_writer(request, None).err(),
                Some(MobileCaptureStartError::InvalidWallClock)
            );
            assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
        }
        assert_eq!(
            wall_clock_unix_milliseconds(0.0),
            Some(MobileWallClockUnixMillisDto { milliseconds: 0 })
        );
        assert_eq!(
            wall_clock_unix_milliseconds(1.2349),
            Some(MobileWallClockUnixMillisDto {
                milliseconds: 1_234
            })
        );
    }

    #[test]
    fn capture_start_formats_native_metadata_and_preserves_filename_header_clock() {
        let directory = Directory::new();
        let mut request = request(&directory);
        request.reason = "manual=note\nnext\rline".to_owned();
        request.evidence = "user=action\nconfirmed".to_owned();
        let prepared = prepare_capture_writer(request, None).unwrap();
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
        let started = prepared.start(10_000).unwrap();
        assert!(started.path.ends_with(
            "cutout-btle-capture-1700000000-3F2504E0-4F89-41D3-9A0C-0305E82C3301.jsonl"
        ));
        assert_eq!(
            started.builder.metadata().annotations,
            [
                "source=ios-app",
                "capture_reason=manual note next line",
                "capture_privacy=private",
                "capture_evidence=user action confirmed",
                "user_note=one two three 四",
            ]
        );
        let capture = finished_capture(&started);
        assert_eq!(
            capture.header.wall_clock_start_unix_ms.as_milliseconds(),
            1_700_000_000_125
        );
        assert!(Path::new(&started.path).is_file());
    }

    #[test]
    fn capture_start_reserves_policy_and_active_label_closure_at_exact_capacity() {
        let directory = Directory::new();
        let database_path = directory.0.join("rides.sqlite");
        let database =
            crate::open_ride_database(database_path.to_string_lossy().into_owned()).unwrap();
        database
            .inner
            .save_automatic_diagnostic_recording_enabled(true)
            .unwrap();
        let mut request = request(&directory);
        request.origin = MobileCaptureOriginDto::Automatic;
        request
            .annotations
            .push("capture_label=ride_start".to_owned());
        // Four startup annotations, one automatic policy and one reserved stop.
        let padding = cutout_core::PEVCAP_MAX_ANNOTATIONS - 6 - request.annotations.len();
        request
            .annotations
            .extend((0..padding).map(|index| format!("note={index}")));
        let prepared =
            prepare_capture_writer(request.clone(), Some(Arc::clone(&database))).unwrap();
        request.annotations.push("note=overflow".to_owned());
        assert_eq!(
            prepare_capture_writer(request, Some(Arc::clone(&database))).err(),
            Some(MobileCaptureStartError::InvalidMetadata)
        );
        let sql = rusqlite::Connection::open(&database_path).unwrap();
        assert_eq!(
            sql.query_row("SELECT COUNT(*) FROM live_capture_sessions", [], |row| row
                .get::<_, u64>(
                0
            ))
            .unwrap(),
            0
        );
        let started = prepared.start(1_000).unwrap();
        let capture = finished_capture(&started);
        let annotations = capture.header.annotations;
        assert_eq!(annotations.len(), cutout_core::PEVCAP_MAX_ANNOTATIONS);
        assert!(
            annotations
                .iter()
                .any(|annotation| annotation == "capture_label=ride_stop")
        );
        assert_eq!(
            cutout_core::CaptureLabelState::from_annotations(
                annotations.iter().map(String::as_str)
            )
            .active()
            .len(),
            0
        );
        assert!(
            annotations
                .iter()
                .any(|annotation| annotation == "capture_recording_policy=material_changes")
        );
        assert_eq!(
            sql.query_row("SELECT COUNT(*) FROM live_capture_sessions", [], |row| row
                .get::<_, u64>(
                0
            ))
            .unwrap(),
            1
        );
        drop(sql);
        database.shutdown().unwrap();
    }

    #[test]
    fn capture_start_rejects_invalid_path_nonce_and_preserves_existing_file() {
        let directory = Directory::new();
        for nonce in [
            "../escape",
            "not-a-uuid",
            "3f2504e04f8941d39a0c0305e82c3301",
        ] {
            let mut request = request(&directory);
            request.filename_nonce = nonce.to_owned();
            assert_eq!(
                prepare_capture_writer(request, None).err(),
                Some(MobileCaptureStartError::InvalidFilenameNonce)
            );
        }
        let mut invalid = request(&directory);
        invalid.directory_path = "relative".to_owned();
        assert_eq!(
            prepare_capture_writer(invalid, None).err(),
            Some(MobileCaptureStartError::InvalidDirectory)
        );
        let prepared = prepare_capture_writer(request(&directory), None).unwrap();
        let path = directory
            .0
            .join("cutout-btle-capture-1700000000-3F2504E0-4F89-41D3-9A0C-0305E82C3301.jsonl");
        fs::write(&path, b"original capture").unwrap();
        assert_eq!(
            prepared.start(1_000).err(),
            Some(MobileCaptureStartError::WriterStartFailed)
        );
        assert_eq!(
            prepared.start(1_001).err(),
            Some(MobileCaptureStartError::AlreadyStarted)
        );
        assert_eq!(fs::read(path).unwrap(), b"original capture");
    }

    #[test]
    fn capture_start_preparation_is_consumed_once_across_concurrent_callers() {
        let directory = Directory::new();
        let prepared = prepare_capture_writer(request(&directory), None).unwrap();
        let other = Arc::clone(&prepared);
        let first = thread::spawn(move || prepared.start(1_000));
        let second = thread::spawn(move || other.start(1_000));
        let results = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| result.as_ref().err()
                    == Some(&MobileCaptureStartError::AlreadyStarted))
                .count(),
            1
        );
        let started = results.into_iter().find_map(Result::ok).unwrap();
        let _ = finished_capture(&started);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn capture_start_annotation_helpers_preserve_first_delimiter_and_unicode() {
        assert_eq!(
            format_pevcap_annotation("key=\n".to_owned(), "one=two\r\n三".to_owned()),
            "key  =one two  三"
        );
        assert_eq!(
            sanitize_pevcap_annotation("=one=two\n三".to_owned()),
            "=one two 三"
        );
        assert_eq!(
            sanitize_pevcap_annotation("plain\r\n三".to_owned()),
            "plain  三"
        );
    }
}
