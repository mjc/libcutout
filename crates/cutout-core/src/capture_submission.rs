//! Shared wire values for an explicit private capture submission.
//!
//! These values describe bytes, not permission to obtain them. Local durable
//! receipt admission belongs to persistence; server access belongs to its actor.

use crate::{CaptureOrigin, PevcapEncoding};
use serde::{Deserialize, Serialize};

mod validation;
pub use validation::{
    CaptureSubmissionValidationError, validate_capture_submission,
    validate_capture_submission_payload,
};

/// Maximum capture body accepted for recording, import, review, and upload.
pub const MAX_CAPTURE_SUBMISSION_BYTES: u64 = 20_000_000_000;
/// Maximum events across all PEVCAP event kinds.
pub const MAX_CAPTURE_SUBMISSION_EVENTS: u64 = 10_000_000;
/// Maximum span of capture-relative event timestamps.
pub const MAX_CAPTURE_SUBMISSION_DURATION_MILLISECONDS: u64 = 24 * 60 * 60 * 1_000;

/// Durable completeness retained from the consumed writer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSubmissionIntegrity {
    /// No queue loss was reported by the writer.
    Complete,
    /// The durable capture honestly retains a known loss count.
    Incomplete {
        /// Observations the writer could not retain.
        dropped_messages: u64,
    },
    /// The durable source predates a trustworthy writer-loss receipt.
    Unknown,
}

/// Recording origin disclosed with a private submission.
///
/// Unlike [`CaptureOrigin`], this vocabulary includes old durable recordings whose
/// header does not contain enough information to establish how recording began.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSubmissionOrigin {
    /// Evidence collected while connecting or riding normally.
    Automatic,
    /// An explicit recording that must not be replaced by picker navigation.
    Manual,
    /// Legacy source metadata does not establish the recording origin.
    Unknown,
}

impl From<CaptureOrigin> for CaptureSubmissionOrigin {
    fn from(origin: CaptureOrigin) -> Self {
        match origin {
            CaptureOrigin::Automatic => Self::Automatic,
            CaptureOrigin::Manual => Self::Manual,
        }
    }
}

/// How submitted bytes relate to the retained original.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSubmissionTransformation {
    /// Exact bytes of an approved file original.
    Unmodified,
    /// A file derivative excludes legacy music metadata.
    MusicRemoved,
    /// A database export excludes music; no exact original file is asserted.
    DatabaseExportMusicExcluded,
}

/// Evidence identity retained separately from the submitted byte digest.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureSubmissionProvenance {
    /// Identity of the consumed writer, independent of a filename or export.
    pub source_artifact_id: String,
    /// Canonical database capture UUID, when SQLite was the durable source.
    pub live_capture_id: Option<String>,
    /// Exact original file digest, when such an original exists.
    pub source_sha256: Option<String>,
    /// A truthful account of any derivative, without modifying its header.
    pub transformation: CaptureSubmissionTransformation,
}

/// Immutable description of one approved transfer snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureSubmissionMetadata {
    /// Optional user-selected context; raw PEVCAP remains the unchanged prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<crate::CaptureEvidenceManifest>,
    /// Lowercase SHA-256 of the exact submitted bytes.
    pub sha256: String,
    /// Exact submitted byte length.
    pub bytes: u64,
    /// Existing PEVCAP representation; not a new capture format.
    pub encoding: PevcapEncoding,
    /// Recording provenance, never a hardware-support gate.
    pub origin: CaptureSubmissionOrigin,
    /// Durable completeness, including disclosed observation loss.
    pub integrity: CaptureSubmissionIntegrity,
    /// Retained original and derivative relationship.
    pub provenance: CaptureSubmissionProvenance,
}

/// Published only after the service's validated bytes and receipt commit.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureSubmissionReceipt {
    /// Stable user-initiated operation UUID.
    pub operation_id: String,
    /// SHA-256 of the durable submitted bytes.
    pub sha256: String,
    /// Durable submitted byte length.
    pub bytes: u64,
    /// Service commit time in Unix milliseconds.
    pub received_at_milliseconds: u64,
}

/// Categories observed in the selected payload for native disclosure.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent payload categories can be present in any combination"
)]
pub struct CaptureSubmissionCategories {
    /// Header device or platform identifiers are present; raw BLE may contain more.
    pub device_identifiers: bool,
    /// Independent or embedded location observations are present.
    pub location: bool,
    /// User description or note annotations are present.
    pub notes: bool,
    /// Header annotations, including physical-action labels, are present.
    pub annotations: bool,
    /// User-entered settings values and their optional context are present.
    #[serde(default)]
    pub settings_observations: bool,
}

/// A full-stream check without replaying or executing captured writes.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CaptureSubmissionPayloadSummary {
    /// Digest of the exact validated input bytes.
    pub sha256: String,
    /// Byte length of the exact validated input.
    pub bytes: u64,
    /// Count across every event kind.
    pub events: u64,
    /// Header annotation count, bounded by the existing format.
    pub annotation_count: u64,
    /// Actual payload categories used by the disclosure.
    pub categories: CaptureSubmissionCategories,
}
