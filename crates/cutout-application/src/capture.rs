//! Capture startup admission and one-shot writer ownership.

use std::{
    path::Path,
    sync::{Mutex, PoisonError},
};

use cutout_core::{CaptureOrigin, GattChannel, TransportWriteLimit, WallClockUnixTimestamp};
use libcutout_persistence::{CaptureWriteOutcome, RideDatabase};
use std::sync::Arc;

use crate::capture_writer::CaptureWriterService;

/// Observations supplied by the host for one capture startup.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureStartRequest {
    pub wall_clock_unix_seconds: f64,
    pub platform_id: String,
    pub advertised_services: Vec<Vec<u8>>,
    pub directory_path: String,
    pub filename_nonce: String,
    pub source: String,
    pub reason: String,
    pub evidence: String,
    pub annotations: Vec<String>,
    pub origin: CaptureOrigin,
}

/// Metadata passed to a writer implementation after startup validation.
#[derive(Clone, Debug)]
pub struct CaptureStartMetadata {
    pub wall_clock_unix_ms: u64,
    pub platform_id: String,
    pub advertised_services: Vec<Vec<u8>>,
    pub annotations: Vec<String>,
    pub origin: CaptureOrigin,
}

/// Failure before a capture writer has been installed in its consumer.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CaptureStartError {
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

/// Writer effects used by the application operation. Implementations retain native
/// handles; admission, consent revalidation and one-shot sequencing stay here.
pub trait CaptureStartBackend {
    type Writer;

    /// Creates metadata without starting I/O.
    ///
    /// # Errors
    /// Returns an error when the bounded metadata cannot be retained.
    fn prepare_writer(
        &self,
        metadata: CaptureStartMetadata,
    ) -> Result<Self::Writer, CaptureStartError>;
    fn automatic_recording_enabled(&self) -> bool;
    fn attach_database(&self, writer: &Self::Writer) -> bool;
    fn start_writer(&self, writer: &Self::Writer, path: &str, monotonic_ms: u64) -> bool;
}

/// Default capture effects backed by the shared Rust writer and persistence owner.
#[derive(Clone, Debug)]
pub struct CaptureWriterBackend {
    database: Option<RideDatabase>,
}

impl CaptureWriterBackend {
    #[must_use]
    pub const fn new(database: Option<RideDatabase>) -> Self {
        Self { database }
    }
}

impl CaptureStartBackend for CaptureWriterBackend {
    type Writer = Arc<CaptureWriterService>;

    fn prepare_writer(
        &self,
        metadata: CaptureStartMetadata,
    ) -> Result<Self::Writer, CaptureStartError> {
        let writer = Arc::new(CaptureWriterService::new(
            WallClockUnixTimestamp::new(metadata.wall_clock_unix_ms),
            metadata.platform_id,
            Some(TransportWriteLimit::from_bytes(23)),
        ));
        if !writer.set_origin(metadata.origin) {
            return Err(CaptureStartError::InvalidMetadata);
        }
        for service in metadata.advertised_services {
            let mut bytes = [0; 16];
            let length = service.len().min(bytes.len());
            bytes[..length].copy_from_slice(&service[..length]);
            if writer.add_advertised_service(GattChannel::from_bytes(bytes))
                != CaptureWriteOutcome::Accepted
            {
                return Err(CaptureStartError::InvalidMetadata);
            }
        }
        for annotation in metadata.annotations {
            writer
                .update_annotations(false, |annotations| annotations.try_append([annotation]))
                .map_err(|_| CaptureStartError::InvalidMetadata)?;
        }
        Ok(writer)
    }

    fn automatic_recording_enabled(&self) -> bool {
        self.database.as_ref().is_some_and(|database| {
            database
                .automatic_diagnostic_recording_enabled()
                .unwrap_or(false)
        })
    }

    fn attach_database(&self, writer: &Self::Writer) -> bool {
        self.database
            .as_ref()
            .is_none_or(|database| writer.set_database(database.clone()))
    }

    fn start_writer(&self, writer: &Self::Writer, path: &str, monotonic_ms: u64) -> bool {
        writer.set_start_monotonic_ms(monotonic_ms) && writer.start(path.into())
    }
}

/// Prepares the shared writer directly for a Rust application or native adapter.
///
/// # Errors
/// Rejects invalid metadata or missing automatic diagnostic consent.
pub fn prepare_capture_start(
    request: CaptureStartRequest,
    database: Option<RideDatabase>,
) -> Result<PreparedCaptureStart<CaptureWriterBackend>, CaptureStartError> {
    prepare_capture_writer(request, CaptureWriterBackend::new(database))
}

struct Prepared<B: CaptureStartBackend> {
    backend: B,
    writer: B::Writer,
    path: String,
    origin: CaptureOrigin,
}

/// Admitted capture startup; every start attempt consumes ownership, even failure.
pub struct PreparedCaptureStart<B: CaptureStartBackend> {
    prepared: Mutex<Option<Prepared<B>>>,
}

impl<B: CaptureStartBackend> std::fmt::Debug for PreparedCaptureStart<B> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedCaptureStart")
            .finish_non_exhaustive()
    }
}

/// A writer whose startup succeeded, with its application-selected artifact path.
#[derive(Debug)]
pub struct StartedCaptureWriter<W> {
    pub writer: W,
    pub path: String,
}

/// Normalizes the supplied observation without reading a different clock.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite nonnegative milliseconds below 2^64 are checked before truncation"
)]
pub fn wall_clock_unix_milliseconds(seconds: f64) -> Option<u64> {
    let milliseconds = seconds * 1_000.0;
    if !milliseconds.is_finite() || !(0.0..18_446_744_073_709_551_616.0).contains(&milliseconds) {
        return None;
    }
    Some(milliseconds.floor() as u64)
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

/// Formats one annotation without allowing additional delimiters or lines.
#[must_use]
pub fn format_annotation(key: &str, value: &str) -> String {
    format!(
        "{}={}",
        annotation_component(key),
        annotation_component(value)
    )
}

/// Splits only the first delimiter, then normalizes supplied components.
#[must_use]
pub fn sanitize_annotation(annotation: &str) -> String {
    match annotation.split_once('=') {
        Some((key, value)) => format_annotation(key, value),
        None => annotation_component(annotation),
    }
}

fn startup_owned_annotation(annotation: &str) -> bool {
    annotation
        .split_once('=')
        .is_some_and(|(key, _)| match key {
            "source"
            | "capture_reason"
            | "capture_privacy"
            | "capture_evidence"
            | "capture_recording_policy" => true,
            _ => false,
        })
}

/// Validates all evidence and prepares metadata without starting the writer.
///
/// # Errors
/// Rejects malformed observations, contradictory metadata or absent consent.
pub fn prepare_capture_writer<B: CaptureStartBackend>(
    request: CaptureStartRequest,
    backend: B,
) -> Result<PreparedCaptureStart<B>, CaptureStartError> {
    let wall_clock_unix_ms = wall_clock_unix_milliseconds(request.wall_clock_unix_seconds)
        .ok_or(CaptureStartError::InvalidWallClock)?;
    let nonce = uuid::Uuid::parse_str(&request.filename_nonce)
        .map_err(|_| CaptureStartError::InvalidFilenameNonce)?;
    if !nonce
        .hyphenated()
        .to_string()
        .eq_ignore_ascii_case(&request.filename_nonce)
    {
        return Err(CaptureStartError::InvalidFilenameNonce);
    }
    let directory = Path::new(&request.directory_path);
    if !directory.is_absolute() || request.directory_path.contains('\0') {
        return Err(CaptureStartError::InvalidDirectory);
    }
    if request.advertised_services.len() > cutout_core::PEVCAP_MAX_ADVERTISED_SERVICES
        || request
            .advertised_services
            .iter()
            .any(|service| service.len() > 16)
    {
        return Err(CaptureStartError::InvalidMetadata);
    }
    let path = directory
        .join(format!(
            "cutout-btle-capture-{}-{}.jsonl",
            wall_clock_unix_ms / 1_000,
            request.filename_nonce
        ))
        .to_string_lossy()
        .into_owned();
    let mut annotations = vec![
        format_annotation("source", &request.source),
        format_annotation("capture_reason", &request.reason),
        "capture_privacy=private".to_owned(),
        format_annotation("capture_evidence", &request.evidence),
    ];
    for annotation in request.annotations {
        let annotation = sanitize_annotation(&annotation);
        if startup_owned_annotation(&annotation) {
            return Err(CaptureStartError::InvalidMetadata);
        }
        annotations.push(annotation);
    }
    let writer = backend.prepare_writer(CaptureStartMetadata {
        wall_clock_unix_ms,
        platform_id: request.platform_id,
        advertised_services: request.advertised_services,
        annotations,
        origin: request.origin,
    })?;
    if request.origin == CaptureOrigin::Automatic && !backend.automatic_recording_enabled() {
        return Err(CaptureStartError::AutomaticRecordingDisabled);
    }
    if !backend.attach_database(&writer) {
        return Err(CaptureStartError::InvalidMetadata);
    }
    Ok(PreparedCaptureStart {
        prepared: Mutex::new(Some(Prepared {
            backend,
            writer,
            path,
            origin: request.origin,
        })),
    })
}

impl<B: CaptureStartBackend> PreparedCaptureStart<B> {
    /// Consumes preparation once and rechecks automatic recording consent.
    ///
    /// # Errors
    /// Rejects reuse, revoked consent and failed writer admission.
    pub fn start(
        &self,
        monotonic_ms: u64,
    ) -> Result<StartedCaptureWriter<B::Writer>, CaptureStartError> {
        let prepared = self
            .prepared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or(CaptureStartError::AlreadyStarted)?;
        if prepared.origin == CaptureOrigin::Automatic
            && !prepared.backend.automatic_recording_enabled()
        {
            return Err(CaptureStartError::AutomaticRecordingDisabled);
        }
        if !prepared
            .backend
            .start_writer(&prepared.writer, &prepared.path, monotonic_ms)
        {
            return Err(CaptureStartError::WriterStartFailed);
        }
        Ok(StartedCaptureWriter {
            writer: prepared.writer,
            path: prepared.path,
        })
    }
}
