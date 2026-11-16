//! Portable capture decisions shared by the Swift and Kotlin sheets.

use super::{
    MobileCaptureSubmissionBatchEligibilityDto, MobileCaptureSubmissionCategoriesDto,
    MobileCaptureSubmissionError, MobileCaptureSubmissionFailureDto,
    MobileCaptureSubmissionLocalPayloadStateDto, MobileCaptureSubmissionPhaseDto,
    MobileCaptureSubmissionStatusDto,
};

/// History mark for a capture that has a durable private-send receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureHistoryBadge {
    /// The receipt is stored and the local payload is still present.
    Sent,
    /// The receipt is stored and the local payload was removed.
    LocalDeleted,
}

/// Sendable and checking flags for one reviewed row, plus the reason to show when it cannot be sent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureBatchAdmission {
    /// The row can be selected and sent.
    pub eligible: bool,
    /// Payload classification is still running.
    pub checking: bool,
    /// Reason to show when the row cannot be sent.
    pub skip_reason: Option<MobileCaptureSubmissionBatchEligibilityDto>,
}

/// One disclosed payload category, in display order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureDisclosureCategory {
    /// Header device or platform identifiers.
    DeviceIdentifiers,
    /// Location observations.
    Location,
    /// User descriptions or notes.
    Notes,
    /// Header annotations, including physical-action labels.
    Annotations,
    /// User-entered settings values.
    SettingsObservations,
}

impl From<cutout_core::CaptureDisclosureCategory> for MobileCaptureDisclosureCategory {
    fn from(category: cutout_core::CaptureDisclosureCategory) -> Self {
        match category {
            cutout_core::CaptureDisclosureCategory::DeviceIdentifiers => Self::DeviceIdentifiers,
            cutout_core::CaptureDisclosureCategory::Location => Self::Location,
            cutout_core::CaptureDisclosureCategory::Notes => Self::Notes,
            cutout_core::CaptureDisclosureCategory::Annotations => Self::Annotations,
            cutout_core::CaptureDisclosureCategory::SettingsObservations => {
                Self::SettingsObservations
            }
        }
    }
}

/// Categories present in this payload, in disclosure order.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns the record at this boundary"
)]
pub fn included_capture_categories(
    categories: MobileCaptureSubmissionCategoriesDto,
) -> Vec<MobileCaptureDisclosureCategory> {
    cutout_core::CaptureSubmissionCategories {
        device_identifiers: categories.device_identifiers,
        location: categories.location,
        notes: categories.notes,
        annotations: categories.annotations,
        settings_observations: categories.settings_observations,
    }
    .included()
    .into_iter()
    .map(MobileCaptureDisclosureCategory::from)
    .collect()
}

/// Whether this reviewed row can be sent, and which reason to show otherwise.
#[must_use]
#[uniffi::export]
pub fn capture_batch_admission(
    eligibility: MobileCaptureSubmissionBatchEligibilityDto,
    skip_reason: Option<MobileCaptureSubmissionBatchEligibilityDto>,
) -> MobileCaptureBatchAdmission {
    use MobileCaptureSubmissionBatchEligibilityDto as Eligibility;
    let eligible = match eligibility {
        Eligibility::Eligible => true,
        Eligibility::AlreadySubmitted
        | Eligibility::Active
        | Eligibility::Empty
        | Eligibility::TooLarge
        | Eligibility::Corrupt
        | Eligibility::Unavailable
        | Eligibility::Checking => false,
    };
    let checking = match eligibility {
        Eligibility::Checking => true,
        Eligibility::Eligible
        | Eligibility::AlreadySubmitted
        | Eligibility::Active
        | Eligibility::Empty
        | Eligibility::TooLarge
        | Eligibility::Corrupt
        | Eligibility::Unavailable => false,
    };
    let skip_reason = if eligible {
        None
    } else {
        Some(skip_reason.unwrap_or(eligibility))
    };
    MobileCaptureBatchAdmission {
        eligible,
        checking,
        skip_reason,
    }
}

/// The history row's send mark. A phase without a receipt has no mark.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns the record at this boundary"
)]
pub fn capture_history_send_badge(
    status: Option<MobileCaptureSubmissionStatusDto>,
) -> Option<MobileCaptureHistoryBadge> {
    let status = status?;
    status.receipt.as_ref()?;
    let removed = match status.local_payload_state {
        MobileCaptureSubmissionLocalPayloadStateDto::Removed => true,
        MobileCaptureSubmissionLocalPayloadStateDto::Retained
        | MobileCaptureSubmissionLocalPayloadStateDto::RemovalPending => false,
    };
    match status.phase {
        MobileCaptureSubmissionPhaseDto::Sent
        | MobileCaptureSubmissionPhaseDto::Deleting
        | MobileCaptureSubmissionPhaseDto::Deleted => Some(if removed {
            MobileCaptureHistoryBadge::LocalDeleted
        } else {
            MobileCaptureHistoryBadge::Sent
        }),
        MobileCaptureSubmissionPhaseDto::Prepared
        | MobileCaptureSubmissionPhaseDto::Sending
        | MobileCaptureSubmissionPhaseDto::Failed
        | MobileCaptureSubmissionPhaseDto::Rejected
        | MobileCaptureSubmissionPhaseDto::Cancelling
        | MobileCaptureSubmissionPhaseDto::Cancelled => None,
    }
}

/// Saved notes and screenshots can change in this phase. No status is still editable.
#[must_use]
#[uniffi::export]
pub fn capture_evidence_is_editable(phase: Option<MobileCaptureSubmissionPhaseDto>) -> bool {
    match phase {
        None
        | Some(
            MobileCaptureSubmissionPhaseDto::Prepared
            | MobileCaptureSubmissionPhaseDto::Cancelled
            | MobileCaptureSubmissionPhaseDto::Deleted,
        ) => true,
        Some(
            MobileCaptureSubmissionPhaseDto::Sending
            | MobileCaptureSubmissionPhaseDto::Failed
            | MobileCaptureSubmissionPhaseDto::Rejected
            | MobileCaptureSubmissionPhaseDto::Sent
            | MobileCaptureSubmissionPhaseDto::Cancelling
            | MobileCaptureSubmissionPhaseDto::Deleting,
        ) => false,
    }
}

/// Private Send can be reviewed when the note and screenshots are loaded and saved.
#[must_use]
#[uniffi::export]
pub fn capture_submission_evidence_is_reviewable(loaded: bool, working: bool, dirty: bool) -> bool {
    loaded && !working && !dirty
}

/// Sentence for a classified transfer failure.
#[must_use]
#[uniffi::export]
pub fn capture_submission_failure_message(failure: MobileCaptureSubmissionFailureDto) -> String {
    match failure {
        MobileCaptureSubmissionFailureDto::Network => {
            "The network failed. This capture is still on this phone."
        }
        MobileCaptureSubmissionFailureDto::Unauthorized => "This account could not send the capture.",
        MobileCaptureSubmissionFailureDto::Forbidden => "This account cannot send the capture.",
        MobileCaptureSubmissionFailureDto::Conflict => {
            "The server already has different information for this capture. It is still on this phone."
        }
        MobileCaptureSubmissionFailureDto::Rejected => {
            "The server rejected this capture. It is still on this phone."
        }
        MobileCaptureSubmissionFailureDto::Throttled => "Waiting. The server asked to slow down.",
        MobileCaptureSubmissionFailureDto::Unavailable => {
            "The server is unavailable. This capture is still on this phone."
        }
        MobileCaptureSubmissionFailureDto::TooLarge => "This capture is too large to send.",
        MobileCaptureSubmissionFailureDto::InvalidCapture => "This capture could not be read.",
        MobileCaptureSubmissionFailureDto::MusicPresent => {
            "This capture includes music and was not sent."
        }
        MobileCaptureSubmissionFailureDto::QuotaExceeded => {
            "The server has no room for more captures."
        }
        MobileCaptureSubmissionFailureDto::Cancelled => "The send was cancelled.",
    }
    .to_owned()
}

/// Shared UTF-8 budget for one capture note.
#[must_use]
#[uniffi::export]
pub fn capture_note_limit_bytes() -> u32 {
    u32::try_from(cutout_core::MAX_CAPTURE_NOTE_BYTES).unwrap_or(u32::MAX)
}

/// Shared screenshot count for one capture.
#[must_use]
#[uniffi::export]
pub fn capture_screenshot_limit() -> u32 {
    u32::try_from(cutout_core::MAX_CAPTURE_SCREENSHOTS).unwrap_or(u32::MAX)
}

/// The sheet message when the screenshot count is above the shared limit.
#[must_use]
#[uniffi::export]
pub fn capture_screenshot_limit_message() -> String {
    MobileCaptureSubmissionError::TooManyScreenshots.to_string()
}

/// Sentence shown when private Send fails for a reason outside the classified errors.
#[must_use]
#[uniffi::export]
pub fn capture_submission_unexpected_error_message() -> String {
    "The private transfer could not be completed. Your original recording is still on this device."
        .to_owned()
}

/// Sentence shown when private Send cannot save or admit the recording.
#[must_use]
#[uniffi::export]
pub fn capture_submission_error_message(error: MobileCaptureSubmissionError) -> String {
    match error {
        MobileCaptureSubmissionError::NoteTooLong => "The note must be 1,024 bytes or less.",
        MobileCaptureSubmissionError::TooManyScreenshots => "Choose up to four screenshots.",
        MobileCaptureSubmissionError::ScreenshotTooLarge => "Each screenshot must be 5 MiB or less.",
        MobileCaptureSubmissionError::InvalidScreenshot => "The screenshot could not be read.",
        MobileCaptureSubmissionError::InvalidSource => {
            "This recording has no verified source for private Send."
        }
        MobileCaptureSubmissionError::NotReady => "This recording is not ready for private Send.",
        MobileCaptureSubmissionError::Unavailable => "Private Send is unavailable. Try again later.",
        MobileCaptureSubmissionError::Unauthorized => "The account cannot authorize this transfer.",
        MobileCaptureSubmissionError::RateLimited => "Too many requests. Try again later.",
        MobileCaptureSubmissionError::QuotaExceeded => {
            "Private capture storage is full. Try again later."
        }
        MobileCaptureSubmissionError::InvalidCapture => {
            "The recording could not be validated for private Send."
        }
        MobileCaptureSubmissionError::StorageFailure => {
            "The transfer status could not be saved. Your original recording is still on this device."
        }
        MobileCaptureSubmissionError::Cancelled => "The transfer was cancelled.",
        MobileCaptureSubmissionError::ReceiptMismatch => {
            "The server receipt did not match this snapshot. Receipt confirmation is still required."
        }
    }
    .to_owned()
}

/// English confirmation name for one disclosed category.
#[must_use]
#[uniffi::export]
pub fn capture_disclosure_label(category: MobileCaptureDisclosureCategory) -> String {
    match category {
        MobileCaptureDisclosureCategory::DeviceIdentifiers => "device and platform identifiers",
        MobileCaptureDisclosureCategory::Location => "location",
        MobileCaptureDisclosureCategory::Notes => "descriptions and notes",
        MobileCaptureDisclosureCategory::Annotations => "annotations and label boundaries",
        MobileCaptureDisclosureCategory::SettingsObservations => "user-entered settings values",
    }
    .to_owned()
}

/// Confirmation phrases for the payload, with the screenshot count last.
#[must_use]
#[uniffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns the record at this boundary"
)]
pub fn capture_disclosure_phrases(
    categories: MobileCaptureSubmissionCategoriesDto,
    screenshot_count: u32,
) -> Vec<String> {
    let mut phrases = included_capture_categories(categories)
        .into_iter()
        .map(capture_disclosure_label)
        .collect::<Vec<_>>();
    if screenshot_count > 0 {
        phrases.push(format!("{screenshot_count} selected screenshots"));
    }
    phrases
}

/// A missing status can still be reviewed. Sent, rejected, and in-flight removal cannot.
#[must_use]
#[uniffi::export]
pub fn capture_submission_send_offered(phase: Option<MobileCaptureSubmissionPhaseDto>) -> bool {
    match phase {
        None
        | Some(
            MobileCaptureSubmissionPhaseDto::Prepared
            | MobileCaptureSubmissionPhaseDto::Sending
            | MobileCaptureSubmissionPhaseDto::Failed
            | MobileCaptureSubmissionPhaseDto::Cancelled,
        ) => true,
        Some(
            MobileCaptureSubmissionPhaseDto::Rejected
            | MobileCaptureSubmissionPhaseDto::Sent
            | MobileCaptureSubmissionPhaseDto::Deleting
            | MobileCaptureSubmissionPhaseDto::Deleted
            | MobileCaptureSubmissionPhaseDto::Cancelling,
        ) => false,
    }
}

/// Cancel stays available until the transfer is finished or already cancelled.
#[must_use]
#[uniffi::export]
pub fn capture_submission_cancel_offered(phase: Option<MobileCaptureSubmissionPhaseDto>) -> bool {
    match phase {
        Some(
            MobileCaptureSubmissionPhaseDto::Prepared
            | MobileCaptureSubmissionPhaseDto::Sending
            | MobileCaptureSubmissionPhaseDto::Failed
            | MobileCaptureSubmissionPhaseDto::Rejected
            | MobileCaptureSubmissionPhaseDto::Cancelling,
        ) => true,
        None
        | Some(
            MobileCaptureSubmissionPhaseDto::Sent
            | MobileCaptureSubmissionPhaseDto::Deleting
            | MobileCaptureSubmissionPhaseDto::Deleted
            | MobileCaptureSubmissionPhaseDto::Cancelled,
        ) => false,
    }
}

/// Outcome line after a cancel or delete of a sent copy.
#[must_use]
#[uniffi::export]
pub fn capture_removal_result_message(phase: MobileCaptureSubmissionPhaseDto) -> String {
    match phase {
        MobileCaptureSubmissionPhaseDto::Deleted => "Sent copy deleted",
        MobileCaptureSubmissionPhaseDto::Cancelled => "Send cancelled",
        MobileCaptureSubmissionPhaseDto::Prepared
        | MobileCaptureSubmissionPhaseDto::Sending
        | MobileCaptureSubmissionPhaseDto::Failed
        | MobileCaptureSubmissionPhaseDto::Rejected
        | MobileCaptureSubmissionPhaseDto::Sent
        | MobileCaptureSubmissionPhaseDto::Deleting
        | MobileCaptureSubmissionPhaseDto::Cancelling => "Removal is pending. Retry to confirm it.",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        MobileCaptureSubmissionBatchEligibilityDto, MobileCaptureSubmissionCategoriesDto,
        MobileCaptureSubmissionError, MobileCaptureSubmissionFailureDto,
        MobileCaptureSubmissionLocalPayloadStateDto, MobileCaptureSubmissionPhaseDto,
        MobileCaptureSubmissionStatusDto, capture_batch_admission, capture_disclosure_phrases,
        capture_evidence_is_editable, capture_history_send_badge, capture_removal_result_message,
        capture_submission_cancel_offered, capture_submission_error_message,
        capture_submission_evidence_is_reviewable, capture_submission_failure_message,
        capture_submission_send_offered, capture_submission_unexpected_error_message,
        included_capture_categories,
    };
    use crate::MobileCaptureIntegrityDto;

    fn status(
        phase: MobileCaptureSubmissionPhaseDto,
        receipt: bool,
        removed: bool,
    ) -> MobileCaptureSubmissionStatusDto {
        MobileCaptureSubmissionStatusDto {
            operation_id: "fixed".to_owned(),
            phase,
            origin: super::super::MobileCaptureSubmissionOriginDto::Unknown,
            integrity: MobileCaptureIntegrityDto::Unknown,
            local_payload_state: if removed {
                MobileCaptureSubmissionLocalPayloadStateDto::Removed
            } else {
                MobileCaptureSubmissionLocalPayloadStateDto::Retained
            },
            categories: MobileCaptureSubmissionCategoriesDto {
                device_identifiers: false,
                location: false,
                notes: false,
                annotations: false,
                settings_observations: false,
            },
            dropped_messages: 0,
            byte_count: 1,
            receipt: receipt.then(|| crate::MobileCaptureSubmissionReceiptDto {
                operation_id: "fixed".to_owned(),
                sha256: "ab".repeat(32),
                bytes: 1,
                received_at_milliseconds: 1,
            }),
            failure: None,
        }
    }

    #[test]
    fn disclosure_order_follows_the_payload_fields() {
        let included = included_capture_categories(MobileCaptureSubmissionCategoriesDto {
            device_identifiers: true,
            location: false,
            notes: true,
            annotations: false,
            settings_observations: true,
        });
        assert_eq!(
            included,
            vec![
                super::MobileCaptureDisclosureCategory::DeviceIdentifiers,
                super::MobileCaptureDisclosureCategory::Notes,
                super::MobileCaptureDisclosureCategory::SettingsObservations,
            ]
        );
    }

    #[test]
    fn checking_admission_keeps_a_supplied_skip_reason() {
        let admission = capture_batch_admission(
            MobileCaptureSubmissionBatchEligibilityDto::Checking,
            Some(MobileCaptureSubmissionBatchEligibilityDto::Empty),
        );
        assert!(!admission.eligible);
        assert!(admission.checking);
        assert_eq!(
            admission.skip_reason,
            Some(MobileCaptureSubmissionBatchEligibilityDto::Empty)
        );
        let eligible = capture_batch_admission(
            MobileCaptureSubmissionBatchEligibilityDto::Eligible,
            Some(MobileCaptureSubmissionBatchEligibilityDto::Empty),
        );
        assert!(eligible.eligible);
        assert_eq!(eligible.skip_reason, None);
    }

    #[test]
    fn history_badge_needs_a_receipt_and_a_sent_phase() {
        assert_eq!(capture_history_send_badge(None), None);
        assert_eq!(
            capture_history_send_badge(Some(status(
                MobileCaptureSubmissionPhaseDto::Sent,
                false,
                false
            ))),
            None
        );
        assert_eq!(
            capture_history_send_badge(Some(status(
                MobileCaptureSubmissionPhaseDto::Sent,
                true,
                true
            ))),
            Some(super::MobileCaptureHistoryBadge::LocalDeleted)
        );
        assert_eq!(
            capture_history_send_badge(Some(status(
                MobileCaptureSubmissionPhaseDto::Prepared,
                true,
                false
            ))),
            None
        );
    }

    #[test]
    fn evidence_gates_match_the_sheet() {
        assert!(capture_evidence_is_editable(None));
        assert!(capture_evidence_is_editable(Some(
            MobileCaptureSubmissionPhaseDto::Deleted
        )));
        assert!(!capture_evidence_is_editable(Some(
            MobileCaptureSubmissionPhaseDto::Sending
        )));
        assert!(capture_submission_evidence_is_reviewable(
            true, false, false
        ));
        assert!(!capture_submission_evidence_is_reviewable(
            true, true, false
        ));
        assert_eq!(
            capture_submission_failure_message(MobileCaptureSubmissionFailureDto::Network),
            "The network failed. This capture is still on this phone."
        );
    }

    #[test]
    fn sheet_copy_and_phase_gates_follow_one_decision() {
        assert_eq!(
            capture_submission_unexpected_error_message(),
            "The private transfer could not be completed. Your original recording is still on this device."
        );
        assert_eq!(
            capture_submission_error_message(MobileCaptureSubmissionError::Unauthorized),
            "The account cannot authorize this transfer."
        );
        assert_eq!(
            capture_submission_error_message(MobileCaptureSubmissionError::TooManyScreenshots),
            "Choose up to four screenshots."
        );
        assert_eq!(
            capture_disclosure_phrases(
                MobileCaptureSubmissionCategoriesDto {
                    device_identifiers: true,
                    location: true,
                    notes: false,
                    annotations: false,
                    settings_observations: false,
                },
                2,
            ),
            vec![
                "device and platform identifiers".to_owned(),
                "location".to_owned(),
                "2 selected screenshots".to_owned(),
            ]
        );
        assert!(capture_submission_send_offered(None));
        assert!(capture_submission_send_offered(Some(
            MobileCaptureSubmissionPhaseDto::Cancelled
        )));
        assert!(!capture_submission_send_offered(Some(
            MobileCaptureSubmissionPhaseDto::Rejected
        )));
        assert!(capture_submission_cancel_offered(Some(
            MobileCaptureSubmissionPhaseDto::Cancelling
        )));
        assert!(!capture_submission_cancel_offered(None));
        assert!(!capture_submission_cancel_offered(Some(
            MobileCaptureSubmissionPhaseDto::Cancelled
        )));
        assert_eq!(
            capture_removal_result_message(MobileCaptureSubmissionPhaseDto::Deleted),
            "Sent copy deleted"
        );
        assert_eq!(
            capture_removal_result_message(MobileCaptureSubmissionPhaseDto::Sent),
            "Removal is pending. Retry to confirm it."
        );
    }
}
