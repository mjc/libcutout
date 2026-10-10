//! Capture startup validation and one-shot ownership through the existing writer.

use std::{
    path::Path,
    sync::{Arc, Mutex, PoisonError},
};

use crate::{
    MobileCaptureOriginDto, MobileCaptureWriteOutcomeDto, MobileMusicHistoryPolicyDto,
    MobilePevcapCaptureBuilder, MobilePevcapMusicEventDto, MobileTransportWriteLimitDto,
    MobileWallClockUnixMillisDto, RideDatabaseHandle,
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
    pub music_history_policy: MobileMusicHistoryPolicyDto,
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
}

#[derive(Debug)]
struct PreparedCaptureWriter {
    builder: Arc<MobilePevcapCaptureBuilder>,
    path: String,
}

/// Admitted startup metadata. The writer remains untouched until `start` consumes it.
#[derive(Debug, uniffi::Object)]
pub struct MobilePreparedCaptureStart {
    prepared: Mutex<Option<PreparedCaptureWriter>>,
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite nonnegative milliseconds below 2^64 are checked before truncation"
)]
pub fn wall_clock_unix_milliseconds(seconds: f64) -> Option<MobileWallClockUnixMillisDto> {
    let milliseconds = seconds * 1_000.0;
    if !milliseconds.is_finite() || !(0.0..18_446_744_073_709_551_616.0).contains(&milliseconds) {
        return None;
    }
    Some(MobileWallClockUnixMillisDto {
        milliseconds: milliseconds.floor() as u64,
    })
}

fn annotation_component(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '=' | '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

/// Formats one annotation without permitting a value to add delimiters or lines.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI string inputs are owned"
)]
pub fn format_pevcap_annotation(key: String, value: String) -> String {
    format!(
        "{}={}",
        annotation_component(&key),
        annotation_component(&value)
    )
}

/// Splits only the first delimiter, then normalizes each supplied component.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI string inputs are owned"
)]
pub fn sanitize_pevcap_annotation(annotation: String) -> String {
    match annotation.split_once('=') {
        Some((key, value)) => format_pevcap_annotation(key.to_owned(), value.to_owned()),
        None => annotation_component(&annotation),
    }
}

fn startup_owned_annotation(annotation: &str) -> bool {
    let Some((key, _)) = annotation.split_once('=') else {
        return false;
    };
    match key {
        "source"
        | "capture_reason"
        | "capture_privacy"
        | "capture_evidence"
        | "capture_recording_policy" => true,
        _ => false,
    }
}

/// Validates all required startup evidence without creating files or database rows.
///
/// # Errors
/// Rejects an invalid clock/path/nonce, truncated service evidence, contradictory
/// startup annotations, or metadata that consumes reserved label-closure space.
#[uniffi::export]
pub fn prepare_capture_writer(
    request: MobileCaptureStartRequestDto,
    database: Option<Arc<RideDatabaseHandle>>,
) -> Result<Arc<MobilePreparedCaptureStart>, MobileCaptureStartError> {
    let wall_clock = wall_clock_unix_milliseconds(request.wall_clock_unix_seconds)
        .ok_or(MobileCaptureStartError::InvalidWallClock)?;
    let nonce = uuid::Uuid::parse_str(&request.filename_nonce)
        .map_err(|_| MobileCaptureStartError::InvalidFilenameNonce)?;
    if !nonce
        .hyphenated()
        .to_string()
        .eq_ignore_ascii_case(&request.filename_nonce)
    {
        return Err(MobileCaptureStartError::InvalidFilenameNonce);
    }
    let directory = Path::new(&request.directory_path);
    if !directory.is_absolute() || request.directory_path.contains('\0') {
        return Err(MobileCaptureStartError::InvalidDirectory);
    }
    if request.advertised_services.len() > cutout_core::PEVCAP_MAX_ADVERTISED_SERVICES
        || request
            .advertised_services
            .iter()
            .any(|service| service.len() > 16)
    {
        return Err(MobileCaptureStartError::InvalidMetadata);
    }
    let path = directory.join(format!(
        "cutout-btle-capture-{}-{}.jsonl",
        wall_clock.milliseconds / 1_000,
        request.filename_nonce
    ));
    let builder = MobilePevcapCaptureBuilder::new(
        wall_clock,
        request.platform_id,
        Some(MobileTransportWriteLimitDto { bytes: 23 }),
    );
    if !builder.set_recording_origin(request.origin)
        || !builder.set_music_history_policy(request.music_history_policy)
    {
        return Err(MobileCaptureStartError::InvalidMetadata);
    }
    for service in request.advertised_services {
        if builder.add_advertised_service(service) != MobileCaptureWriteOutcomeDto::Accepted {
            return Err(MobileCaptureStartError::InvalidMetadata);
        }
    }
    for annotation in [
        format_pevcap_annotation("source".to_owned(), request.source),
        format_pevcap_annotation("capture_reason".to_owned(), request.reason),
        "capture_privacy=private".to_owned(),
        format_pevcap_annotation("capture_evidence".to_owned(), request.evidence),
    ] {
        if builder.add_annotation(annotation) != MobileCaptureWriteOutcomeDto::Accepted {
            return Err(MobileCaptureStartError::InvalidMetadata);
        }
    }
    for annotation in request.annotations {
        let annotation = sanitize_pevcap_annotation(annotation);
        if startup_owned_annotation(&annotation)
            || builder.add_annotation(annotation) != MobileCaptureWriteOutcomeDto::Accepted
        {
            return Err(MobileCaptureStartError::InvalidMetadata);
        }
    }
    if let Some(database) = database
        && !builder.set_database(database)
    {
        return Err(MobileCaptureStartError::InvalidMetadata);
    }
    Ok(Arc::new(MobilePreparedCaptureStart {
        prepared: Mutex::new(Some(PreparedCaptureWriter {
            builder,
            path: path.to_string_lossy().into_owned(),
        })),
    }))
}

#[uniffi::export]
impl MobilePreparedCaptureStart {
    /// Consumes admitted preparation once, including when the actual writer cannot start.
    /// Optional stale music context cannot reject otherwise valid startup.
    ///
    /// # Errors
    /// Returns `AlreadyStarted` for reuse and `WriterStartFailed` for writer admission failure.
    pub fn start(
        &self,
        monotonic_ms: u64,
        music_context: Option<MobilePevcapMusicEventDto>,
    ) -> Result<MobileStartedCaptureWriterDto, MobileCaptureStartError> {
        let prepared = self
            .prepared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or(MobileCaptureStartError::AlreadyStarted)?;
        if !prepared
            .builder
            .set_capture_start_monotonic_ms(monotonic_ms)
        {
            return Err(MobileCaptureStartError::WriterStartFailed);
        }
        let _ = prepared.builder.set_music_context(music_context);
        if !prepared.builder.start_writer(prepared.path.clone()) {
            return Err(MobileCaptureStartError::WriterStartFailed);
        }
        Ok(MobileStartedCaptureWriterDto {
            builder: prepared.builder,
            path: prepared.path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf, thread};

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
            origin: MobileCaptureOriginDto::Automatic,
            music_history_policy: MobileMusicHistoryPolicyDto::Disabled,
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
        let started = prepared.start(10_000, None).unwrap();
        assert!(started.path.ends_with(
            "cutout-btle-capture-1700000000-3F2504E0-4F89-41D3-9A0C-0305E82C3301.jsonl"
        ));
        assert_eq!(
            started.builder.wall_clock_start_unix_ms.as_milliseconds(),
            1_700_000_000_125
        );
        assert_eq!(
            started.builder.metadata().annotations,
            [
                "capture_recording_policy=material_changes",
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
        let mut request = request(&directory);
        request
            .annotations
            .push("capture_label=ride_start".to_owned());
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
        let started = prepared.start(1_000, None).unwrap();
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
        let path = prepared
            .prepared
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .path
            .clone();
        fs::write(&path, b"original capture").unwrap();
        assert_eq!(
            prepared.start(1_000, None).err(),
            Some(MobileCaptureStartError::WriterStartFailed)
        );
        assert_eq!(
            prepared.start(1_001, None).err(),
            Some(MobileCaptureStartError::AlreadyStarted)
        );
        assert_eq!(fs::read(path).unwrap(), b"original capture");
    }

    #[test]
    fn capture_start_preparation_is_consumed_once_across_concurrent_callers() {
        let directory = Directory::new();
        let prepared = prepare_capture_writer(request(&directory), None).unwrap();
        let other = Arc::clone(&prepared);
        let first = thread::spawn(move || prepared.start(1_000, None));
        let second = thread::spawn(move || other.start(1_000, None));
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
    fn capture_start_stale_optional_music_does_not_reject_writer() {
        let directory = Directory::new();
        let mut request = request(&directory);
        request.music_history_policy = MobileMusicHistoryPolicyDto::OpaqueItem;
        let prepared = prepare_capture_writer(request, None).unwrap();
        let music = MobilePevcapMusicEventDto {
            provider: crate::MobileMusicProviderDto::Spotify,
            track_id: "spotify:track:test".to_owned(),
            monotonic_at_ms: 1,
            wall_clock_unix_ms: 1_700_000_000_000,
            clock_uncertainty_ms: 1,
            ride_sequence: None,
        };
        let started = prepared.start(10_000, Some(music)).unwrap();
        assert_eq!(started.builder.music_context.lock().unwrap().len(), 0);
        let _ = finished_capture(&started);
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
