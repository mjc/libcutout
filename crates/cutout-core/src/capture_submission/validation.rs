use super::{
    CaptureSubmissionCategories, CaptureSubmissionMetadata, CaptureSubmissionPayloadSummary,
    CaptureSubmissionReceipt, CaptureSubmissionTransformation, MAX_CAPTURE_SUBMISSION_BYTES,
    MAX_CAPTURE_SUBMISSION_DURATION_MILLISECONDS, MAX_CAPTURE_SUBMISSION_EVENTS,
};
use crate::{
    PevcapBinaryError, PevcapEncoding, PevcapEvent, PevcapJsonlError, PevcapReader,
    PevcapStreamError,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    io::{self, Read},
    rc::Rc,
};
use thiserror::Error;
use uuid::Uuid;

/// Classified rejection without echoing identifiers, notes or raw parse input.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum CaptureSubmissionValidationError {
    /// Transfer identity or provenance is inconsistent.
    #[error("capture metadata is invalid")]
    InvalidMetadata,
    /// The complete byte stream is not valid PEVCAP.
    #[error("capture data is invalid")]
    InvalidPevcap,
    /// The container version is unsupported, independently of device support.
    #[error("capture format is unsupported")]
    UnsupportedFormat,
    /// A known independent or embedded music field is present.
    #[error("music metadata is prohibited in diagnostic submissions")]
    MusicPresent,
    /// The input exceeds the bounded byte length.
    #[error("capture byte limit exceeded")]
    TooLarge,
    /// The input exceeds the bounded event count.
    #[error("capture event limit exceeded")]
    TooManyEvents,
    /// The event timeline exceeds the supported capture span.
    #[error("capture duration limit exceeded")]
    DurationTooLong,
    /// The underlying source could not be read.
    #[error("capture source could not be read")]
    ReadFailure,
}

impl CaptureSubmissionMetadata {
    /// Validates the immutable envelope without asserting local writer authority.
    ///
    /// # Errors
    /// Returns a classified metadata or resource-limit error.
    pub fn validate(&self) -> Result<(), CaptureSubmissionValidationError> {
        use CaptureSubmissionValidationError::{InvalidMetadata, TooLarge};
        if self.bytes > MAX_CAPTURE_SUBMISSION_BYTES {
            return Err(TooLarge);
        }
        if self.bytes == 0
            || !digest_is_valid(&self.sha256)
            || !canonical_uuid(&self.provenance.source_artifact_id)
            || self
                .provenance
                .live_capture_id
                .as_deref()
                .is_some_and(|id| !canonical_uuid(id))
            || self
                .provenance
                .source_sha256
                .as_deref()
                .is_some_and(|s| !digest_is_valid(s))
        {
            return Err(InvalidMetadata);
        }
        let source = self.provenance.source_sha256.as_deref();
        let valid = match self.provenance.transformation {
            CaptureSubmissionTransformation::Unmodified => source == Some(self.sha256.as_str()),
            CaptureSubmissionTransformation::MusicRemoved => {
                source.is_some_and(|s| s != self.sha256)
            }
            CaptureSubmissionTransformation::DatabaseExportMusicExcluded => {
                self.provenance.live_capture_id.is_some() && source.is_none()
            }
        };
        if valid { Ok(()) } else { Err(InvalidMetadata) }
    }
}

impl CaptureSubmissionReceipt {
    /// Checks the receipt against the operation and exact approved bytes.
    ///
    /// # Errors
    /// Returns invalid metadata for a mismatched or malformed receipt.
    pub fn validate_for(
        &self,
        operation_id: &str,
        metadata: &CaptureSubmissionMetadata,
    ) -> Result<(), CaptureSubmissionValidationError> {
        metadata.validate()?;
        if canonical_uuid(operation_id)
            && self.operation_id == operation_id
            && self.sha256 == metadata.sha256
            && self.bytes == metadata.bytes
            && self.received_at_milliseconds > 0
            && i64::try_from(self.received_at_milliseconds).is_ok()
        {
            Ok(())
        } else {
            Err(CaptureSubmissionValidationError::InvalidMetadata)
        }
    }
}

fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

fn digest_is_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Default)]
struct ByteCounter {
    bytes: u64,
    digest: Sha256,
    too_large: bool,
    read_failed: bool,
}

struct CountedReader<R> {
    inner: R,
    counter: Rc<RefCell<ByteCounter>>,
}

impl<R: Read> Read for CountedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let remaining = MAX_CAPTURE_SUBMISSION_BYTES + 1 - self.counter.borrow().bytes;
        if remaining == 0 {
            self.counter.borrow_mut().too_large = true;
            return Err(io::Error::other("capture byte limit exceeded"));
        }
        let length = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = match self.inner.read(&mut buffer[..length]) {
            Ok(read) => read,
            Err(error) => {
                self.counter.borrow_mut().read_failed = true;
                return Err(error);
            }
        };
        let mut counter = self.counter.borrow_mut();
        counter.bytes += u64::try_from(read).unwrap_or(u64::MAX);
        if counter.bytes > MAX_CAPTURE_SUBMISSION_BYTES {
            counter.too_large = true;
            return Err(io::Error::other("capture byte limit exceeded"));
        }
        counter.digest.update(&buffer[..read]);
        Ok(read)
    }
}

fn classify(error: &PevcapStreamError, counter: &ByteCounter) -> CaptureSubmissionValidationError {
    use CaptureSubmissionValidationError::{
        InvalidPevcap, ReadFailure, TooLarge, UnsupportedFormat,
    };
    if counter.too_large {
        return TooLarge;
    }
    if counter.read_failed {
        return ReadFailure;
    }
    match error {
        PevcapStreamError::Jsonl(PevcapJsonlError::LineTooLong { .. })
        | PevcapStreamError::Binary(PevcapBinaryError::PayloadTooLarge { .. }) => TooLarge,
        PevcapStreamError::Jsonl(PevcapJsonlError::UnsupportedVersion { .. })
        | PevcapStreamError::Binary(PevcapBinaryError::UnsupportedVersion { .. }) => {
            UnsupportedFormat
        }
        _ => InvalidPevcap,
    }
}

#[derive(Default)]
struct EventCounter {
    count: u64,
    first: Option<u64>,
    last: Option<u64>,
}

impl EventCounter {
    fn observe(&mut self, timestamp: u64) -> Result<(), CaptureSubmissionValidationError> {
        self.count += 1;
        if self.count > MAX_CAPTURE_SUBMISSION_EVENTS {
            return Err(CaptureSubmissionValidationError::TooManyEvents);
        }
        let first = self.first.map_or(timestamp, |old| old.min(timestamp));
        let last = self.last.map_or(timestamp, |old| old.max(timestamp));
        self.first = Some(first);
        self.last = Some(last);
        if last - first > MAX_CAPTURE_SUBMISSION_DURATION_MILLISECONDS {
            return Err(CaptureSubmissionValidationError::DurationTooLong);
        }
        Ok(())
    }
}

/// Validates all event kinds, rejects music and computes the exact input digest.
/// Unknown device/protocol identities and GPS-free evidence remain valid. This
/// operation never creates a host session or executes captured writes.
///
/// # Errors
/// Returns a bounded format, resource, music or source-read classification.
pub fn validate_capture_submission<R: Read>(
    source: R,
    encoding: PevcapEncoding,
) -> Result<CaptureSubmissionPayloadSummary, CaptureSubmissionValidationError> {
    let counter = Rc::new(RefCell::new(ByteCounter::default()));
    let input = CountedReader {
        inner: source,
        counter: Rc::clone(&counter),
    };
    let mut reader =
        PevcapReader::new(input, encoding).map_err(|error| classify(&error, &counter.borrow()))?;
    let header = reader.header();
    let mut categories = CaptureSubmissionCategories {
        device_identifiers: !header.platform_id.is_empty()
            || !header.gatt_fingerprints.is_empty()
            || !header.advertised_services.is_empty()
            || header.resolved_identity.is_some(),
        location: false,
        notes: header.annotations.iter().any(|value| {
            value.starts_with("description=")
                || value.starts_with("note=")
                || value.starts_with("device_description=")
                || value.starts_with("capture_description=")
                || value.starts_with("user_note=")
        }),
        annotations: !header.annotations.is_empty(),
        settings_observations: false,
    };
    let annotation_count = u64::try_from(header.annotations.len()).unwrap_or(u64::MAX);
    let mut events = EventCounter::default();
    let mut settings_observations = 0;
    while let Some(event) = reader
        .next_event()
        .map_err(|error| classify(&error, &counter.borrow()))?
    {
        if matches!(&event, PevcapEvent::SettingsObservation(_)) {
            settings_observations += 1;
            if settings_observations > crate::MAX_CAPTURE_SETTINGS_OBSERVATIONS {
                return Err(CaptureSubmissionValidationError::TooManyEvents);
            }
        }
        let timestamp = inspect_event(event, &mut categories)?;
        events.observe(timestamp)?;
    }
    let counter = counter.borrow();
    Ok(CaptureSubmissionPayloadSummary {
        sha256: format!("{:x}", counter.digest.clone().finalize()),
        bytes: counter.bytes,
        events: events.count,
        annotation_count,
        categories,
    })
}

fn inspect_event(
    event: PevcapEvent,
    categories: &mut CaptureSubmissionCategories,
) -> Result<u64, CaptureSubmissionValidationError> {
    match event {
        PevcapEvent::SettingsObservation(observation) => {
            categories.notes = true;
            categories.annotations = true;
            categories.settings_observations = true;
            Ok(observation.recorded_at_ms)
        }
        PevcapEvent::Music(_) => Err(CaptureSubmissionValidationError::MusicPresent),
        PevcapEvent::Record(record) => {
            if record.music.is_some() {
                return Err(CaptureSubmissionValidationError::MusicPresent);
            }
            categories.location |= record.phone_location.is_some();
            Ok(record.monotonic_ms.get())
        }
        PevcapEvent::Location(sample) => {
            categories.location = true;
            Ok(sample.receipt_monotonic_ms.get())
        }
        PevcapEvent::LocationRejected(rejection) => {
            categories.location = true;
            Ok(rejection.observation.receipt_monotonic_ms.get())
        }
    }
}

#[cfg(test)]
mod tests;
