//! Capture lifecycle through the existing session-state owner.

use crate::{CutoutSessionStateHandle, MobileCaptureWriteOutcomeDto, MobileCaptureWriterStatusDto};
use cutout_application::session::{CaptureWriteDecision, CaptureWriterOutcome};
use cutout_core::{
    CaptureAttemptSnapshot, CaptureCompletionDisposition, CaptureFinishToken, CaptureGeneration,
    CaptureOrigin, CaptureStage,
};

macro_rules! capture_enum {
    ($mobile:ident, $core:ident, $($variant:ident),+ $(,)?) => {
        /// Binding representation of the shared capture lifecycle vocabulary.
        #[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
        pub enum $mobile { $($variant),+ }
        impl From<$core> for $mobile {
            fn from(value: $core) -> Self { match value { $($core::$variant => Self::$variant),+ } }
        }
        impl From<$mobile> for $core {
            fn from(value: $mobile) -> Self { match value { $($mobile::$variant => Self::$variant),+ } }
        }
    };
}

capture_enum!(MobileCaptureOriginDto, CaptureOrigin, Automatic, Manual);
capture_enum!(
    MobileCaptureStageDto,
    CaptureStage,
    Starting,
    Recording,
    Saving,
    SaveFailed,
    Finalizing,
    Saved,
    Failed
);

/// Rust's decision for how the session should handle one capture-writer result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureWriteDecisionDto {
    /// Keep processing the session; the writer accepted the event.
    Continue,
    /// Metadata admission rejected the event, but the writer remains usable.
    Rejected,
    /// Evidence was lost at bounded admission, but the writer remains usable.
    AdmissionLost,
    /// The current writer failed and native failure effects should run.
    FailWriter,
    /// The result did not belong to an active writer of the current capture attempt.
    StaleFailure,
}

impl From<CaptureWriteDecision> for MobileCaptureWriteDecisionDto {
    fn from(value: CaptureWriteDecision) -> Self {
        match value {
            CaptureWriteDecision::Continue => Self::Continue,
            CaptureWriteDecision::Rejected => Self::Rejected,
            CaptureWriteDecision::AdmissionLost => Self::AdmissionLost,
            CaptureWriteDecision::FailWriter => Self::FailWriter,
            CaptureWriteDecision::StaleFailure => Self::StaleFailure,
        }
    }
}
impl From<MobileCaptureWriteDecisionDto> for CaptureWriteDecision {
    fn from(value: MobileCaptureWriteDecisionDto) -> Self {
        match value {
            MobileCaptureWriteDecisionDto::Continue => Self::Continue,
            MobileCaptureWriteDecisionDto::Rejected => Self::Rejected,
            MobileCaptureWriteDecisionDto::AdmissionLost => Self::AdmissionLost,
            MobileCaptureWriteDecisionDto::FailWriter => Self::FailWriter,
            MobileCaptureWriteDecisionDto::StaleFailure => Self::StaleFailure,
        }
    }
}

/// Durability receipt for one flush; observation loss is tracked separately.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureFlushOutcomeDto {
    /// All evidence preceding the admitted barrier is durable.
    Flushed,
    /// The healthy writer rejected this control request; retry is possible.
    Rejected,
    /// The writer failed or an admitted barrier has no valid durable receipt.
    Failed {
        /// First fatal writer cause.
        message: String,
    },
}

impl From<libcutout_persistence::CaptureFlushOutcome> for MobileCaptureFlushOutcomeDto {
    fn from(outcome: libcutout_persistence::CaptureFlushOutcome) -> Self {
        match outcome {
            libcutout_persistence::CaptureFlushOutcome::Flushed => Self::Flushed,
            libcutout_persistence::CaptureFlushOutcome::Rejected => Self::Rejected,
            libcutout_persistence::CaptureFlushOutcome::Failed { message } => {
                Self::Failed { message }
            }
        }
    }
}

fn apply_writer_health(
    handle: &CutoutSessionStateHandle,
    generation: Option<MobileCaptureGenerationDto>,
    fatal: bool,
    healthy: MobileCaptureWriteDecisionDto,
) -> MobileCaptureWriteDecisionDto {
    handle
        .lock_inner()
        .apply_capture_writer_health(generation.map(Into::into), fatal, healthy.into())
        .into()
}

/// One Rust-issued writer attempt, not a connection or device identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureGenerationDto {
    /// Monotonic session-local generation.
    pub value: u64,
}

impl From<MobileCaptureGenerationDto> for CaptureGeneration {
    fn from(value: MobileCaptureGenerationDto) -> Self {
        Self::new(value.value)
    }
}
impl From<CaptureGeneration> for MobileCaptureGenerationDto {
    fn from(value: CaptureGeneration) -> Self {
        Self { value: value.get() }
    }
}

/// Generation-fenced projection, including failures before recording begins.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureAttemptDto {
    /// Writer identity.
    pub generation: MobileCaptureGenerationDto,
    /// Connection ownership intent.
    pub origin: MobileCaptureOriginDto,
    /// Current lifecycle stage.
    pub stage: MobileCaptureStageDto,
}

impl From<CaptureAttemptSnapshot> for MobileCaptureAttemptDto {
    fn from(value: CaptureAttemptSnapshot) -> Self {
        Self {
            generation: value.generation.into(),
            origin: value.origin.into(),
            stage: value.stage.into(),
        }
    }
}

/// One atomic lifecycle/admission publication for the native session and UI.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureLifecycleSnapshotDto {
    /// Most recent attempt, absent before the first request.
    pub attempt: Option<MobileCaptureAttemptDto>,
    /// Whether a writer slot can be reserved.
    pub can_start: bool,
    /// Whether ordinary pairing may retire automatic evidence and connect.
    pub can_pair: bool,
}

/// One admitted asynchronous save; stale captures and stale retries are rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureFinishTokenDto {
    /// Capture identity.
    pub generation: MobileCaptureGenerationDto,
    /// Save retry identity.
    pub operation: u64,
}

impl From<CaptureFinishToken> for MobileCaptureFinishTokenDto {
    fn from(value: CaptureFinishToken) -> Self {
        Self {
            generation: value.generation().into(),
            operation: value.operation(),
        }
    }
}
impl From<MobileCaptureFinishTokenDto> for CaptureFinishToken {
    fn from(value: MobileCaptureFinishTokenDto) -> Self {
        Self::new(value.generation.into(), value.operation)
    }
}

#[uniffi::export]
impl CutoutSessionStateHandle {
    /// Reads admission without copying unrelated telemetry or settings.
    pub fn capture_lifecycle_snapshot(&self) -> MobileCaptureLifecycleSnapshotDto {
        let snapshot = self.lock_inner().capture_snapshot();
        MobileCaptureLifecycleSnapshotDto {
            attempt: snapshot.attempt.map(Into::into),
            can_start: snapshot.can_start,
            can_pair: snapshot.can_pair,
        }
    }

    /// Reserves an attempt before invoking the existing writer constructor.
    pub fn begin_capture(
        &self,
        origin: MobileCaptureOriginDto,
    ) -> Option<MobileCaptureGenerationDto> {
        self.lock_inner()
            .begin_capture(origin.into())
            .map(Into::into)
    }

    /// Reports successful writer creation, never just the user's intent to start.
    pub fn capture_writer_started(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner().capture_writer_started(generation.into())
    }

    /// Reports a failed constructor for its reserved attempt.
    pub fn capture_writer_start_failed(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner()
            .capture_writer_start_failed(generation.into())
    }

    /// Claims a save operation before native asynchronous work.
    pub fn begin_capture_finish(
        &self,
        generation: MobileCaptureGenerationDto,
    ) -> Option<MobileCaptureFinishTokenDto> {
        self.lock_inner()
            .begin_capture_finish(generation.into())
            .map(Into::into)
    }

    /// Authorizes transport release only for this capture's successful save flush.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI enum inputs are owned"
    )]
    pub fn finish_capture_flush(
        &self,
        token: MobileCaptureFinishTokenDto,
        outcome: MobileCaptureFlushOutcomeDto,
    ) -> bool {
        let succeeded = match outcome {
            MobileCaptureFlushOutcomeDto::Flushed => true,
            MobileCaptureFlushOutcomeDto::Rejected
            | MobileCaptureFlushOutcomeDto::Failed { .. } => false,
        };
        self.lock_inner()
            .finish_capture_flush(token.into(), succeeded)
    }

    /// Interprets one flush receipt without native queue or writer-health policy.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI enum inputs are owned"
    )]
    pub fn apply_capture_flush_outcome(
        &self,
        generation: Option<MobileCaptureGenerationDto>,
        outcome: MobileCaptureFlushOutcomeDto,
    ) -> MobileCaptureWriteDecisionDto {
        match outcome {
            MobileCaptureFlushOutcomeDto::Flushed => apply_writer_health(
                self,
                generation,
                false,
                MobileCaptureWriteDecisionDto::Continue,
            ),
            MobileCaptureFlushOutcomeDto::Rejected => apply_writer_health(
                self,
                generation,
                false,
                MobileCaptureWriteDecisionDto::Rejected,
            ),
            MobileCaptureFlushOutcomeDto::Failed { .. } => apply_writer_health(
                self,
                generation,
                true,
                MobileCaptureWriteDecisionDto::Continue,
            ),
        }
    }

    /// Applies authoritative writer health; counters and diagnostic text do not prove failure.
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply_capture_writer_status(
        &self,
        generation: Option<MobileCaptureGenerationDto>,
        status: MobileCaptureWriterStatusDto,
    ) -> MobileCaptureWriteDecisionDto {
        apply_writer_health(
            self,
            generation,
            status.failed,
            MobileCaptureWriteDecisionDto::Continue,
        )
    }

    /// Marks failed evidence without letting a later counter update clear it.
    pub fn capture_writer_failed(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner().capture_writer_failed(generation.into())
    }

    /// Applies a writer result to the Rust-owned capture lifecycle.
    pub fn apply_capture_write_outcome(
        &self,
        generation: Option<MobileCaptureGenerationDto>,
        outcome: MobileCaptureWriteOutcomeDto,
    ) -> MobileCaptureWriteDecisionDto {
        let outcome = match outcome {
            MobileCaptureWriteOutcomeDto::Accepted => CaptureWriterOutcome::Accepted,
            MobileCaptureWriteOutcomeDto::Rejected => CaptureWriterOutcome::Rejected,
            MobileCaptureWriteOutcomeDto::AdmissionLost { .. } => {
                CaptureWriterOutcome::AdmissionLost
            }
            MobileCaptureWriteOutcomeDto::Failed => CaptureWriterOutcome::Failed,
        };
        self.lock_inner()
            .apply_capture_write_outcome(generation.map(Into::into), outcome)
            .into()
    }

    /// Detaches active ownership while the consumed writer finishes.
    pub fn retire_capture_writer(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner().retire_capture_writer(generation.into())
    }

    /// Consumes one owned terminal receipt, preserving current state for retired writers.
    pub fn complete_capture_writer(
        &self,
        generation: MobileCaptureGenerationDto,
        completion: crate::MobileCaptureCompletionDto,
        prior_write_outcome: MobileCaptureWriteOutcomeDto,
    ) -> crate::MobileCaptureCompletionDecisionDto {
        match completion.finish {
            crate::MobileCaptureFinishOutcomeDto::NotStarted
            | crate::MobileCaptureFinishOutcomeDto::Finalizing => {
                return crate::MobileCaptureCompletionDecisionDto::Rejected;
            }
            crate::MobileCaptureFinishOutcomeDto::ArtifactAvailable { .. }
            | crate::MobileCaptureFinishOutcomeDto::DatabaseFinished { .. }
            | crate::MobileCaptureFinishOutcomeDto::Failed { .. } => {}
        }
        let succeeded =
            crate::capture_completion::completion_succeeded(&completion, prior_write_outcome);
        let disposition = self
            .lock_inner()
            .complete_capture_writer(generation.into(), succeeded);
        match disposition {
            CaptureCompletionDisposition::Rejected => {
                crate::MobileCaptureCompletionDecisionDto::Rejected
            }
            CaptureCompletionDisposition::CurrentAttempt
            | CaptureCompletionDisposition::HistoricalAttempt => {
                crate::capture_completion::publication_for_completion(
                    completion,
                    prior_write_outcome,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_failure_does_not_invalidate_a_verified_connection() {
        let handle = CutoutSessionStateHandle::new();
        let generation = handle
            .begin_capture(MobileCaptureOriginDto::Automatic)
            .unwrap();
        assert!(handle.capture_writer_started(generation));

        let snapshot = handle.begin_connection_attempt("A".into(), 10);
        let token = snapshot.token.unwrap();
        handle.connection_link_established(token.clone());
        {
            let mut inner = handle.lock_inner();
            assert!(inner.finish_detection(&token.clone().into(), true));
        }
        let connection_before = handle.connection_attempt_snapshot();

        assert!(handle.capture_writer_failed(generation));

        assert_eq!(handle.connection_attempt_snapshot(), connection_before);
        assert!(handle.verified_connection_attempt_is_current(token));
        assert_eq!(
            handle.capture_lifecycle_snapshot().attempt.unwrap().stage,
            MobileCaptureStageDto::SaveFailed
        );
    }

    #[test]
    fn capture_write_outcomes_use_the_rust_owned_writer_lifecycle() {
        let handle = CutoutSessionStateHandle::new();
        let generation = handle
            .begin_capture(MobileCaptureOriginDto::Manual)
            .unwrap();
        assert!(handle.capture_writer_started(generation));

        assert_eq!(
            handle.apply_capture_write_outcome(
                Some(generation),
                MobileCaptureWriteOutcomeDto::Accepted
            ),
            MobileCaptureWriteDecisionDto::Continue
        );
        assert_eq!(
            handle.apply_capture_write_outcome(
                Some(generation),
                MobileCaptureWriteOutcomeDto::Rejected
            ),
            MobileCaptureWriteDecisionDto::Rejected
        );
        assert_eq!(
            handle.apply_capture_write_outcome(
                Some(generation),
                MobileCaptureWriteOutcomeDto::AdmissionLost {
                    dropped_messages: 65
                }
            ),
            MobileCaptureWriteDecisionDto::AdmissionLost
        );
        assert_eq!(
            handle.capture_lifecycle_snapshot().attempt.unwrap().stage,
            MobileCaptureStageDto::Recording
        );
        assert_eq!(
            handle.apply_capture_write_outcome(
                Some(generation),
                MobileCaptureWriteOutcomeDto::Failed
            ),
            MobileCaptureWriteDecisionDto::FailWriter
        );
        assert_eq!(
            handle.capture_lifecycle_snapshot().attempt.unwrap().stage,
            MobileCaptureStageDto::SaveFailed
        );
    }

    #[test]
    fn capture_write_failure_without_its_generation_is_stale() {
        let handle = CutoutSessionStateHandle::new();

        assert_eq!(
            handle.apply_capture_write_outcome(None, MobileCaptureWriteOutcomeDto::Failed),
            MobileCaptureWriteDecisionDto::StaleFailure
        );
        assert!(handle.capture_lifecycle_snapshot().attempt.is_none());
    }

    #[test]
    fn shared_handle_keeps_capture_admission_across_connection_changes() {
        let owner = CutoutSessionStateHandle::new();
        let generation = owner.begin_capture(MobileCaptureOriginDto::Manual).unwrap();
        assert!(owner.capture_writer_started(generation));
        owner.begin_connection_attempt("wheel-a".into(), 0);
        let snapshot = owner.capture_lifecycle_snapshot();
        assert!(!snapshot.can_start);
        assert!(!snapshot.can_pair);
        assert_eq!(snapshot.attempt.unwrap().generation, generation);
        owner.retire_capture_writer(generation);
        owner.complete_capture_writer(
            generation,
            crate::MobileCaptureCompletionDto {
                finish: crate::MobileCaptureFinishOutcomeDto::Failed {
                    message: "writer failed".into(),
                },
                database_publication_succeeded: None,
            },
            MobileCaptureWriteOutcomeDto::Accepted,
        );
        assert!(owner.capture_lifecycle_snapshot().can_start);
    }

    #[test]
    fn camera_media_provenance_requires_the_generation_that_is_current_at_completion() {
        let owner = CutoutSessionStateHandle::new();
        let first = owner.begin_capture(MobileCaptureOriginDto::Manual).unwrap();
        assert!(owner.capture_writer_started(first));

        assert!(
            owner
                .record_camera_media_provenance(camera_media_provenance(first))
                .is_ok()
        );
        assert_eq!(owner.camera_media_provenance().len(), 1);

        assert!(owner.retire_capture_writer(first));
        assert_eq!(
            owner.complete_capture_writer(
                first,
                crate::MobileCaptureCompletionDto {
                    finish: crate::MobileCaptureFinishOutcomeDto::DatabaseFinished {
                        live_capture_id: "capture-a".into(),
                        integrity: crate::MobileCaptureIntegrityDto::Complete,
                        jsonl_export: crate::MobileCaptureJsonlExportDto::NotAttempted,
                        status: MobileCaptureWriterStatusDto::default(),
                    },
                    database_publication_succeeded: None,
                },
                MobileCaptureWriteOutcomeDto::Accepted,
            ),
            crate::MobileCaptureCompletionDecisionDto::PublishDatabase {
                outcome: crate::MobileCaptureFinishOutcomeDto::DatabaseFinished {
                    live_capture_id: "capture-a".into(),
                    integrity: crate::MobileCaptureIntegrityDto::Complete,
                    jsonl_export: crate::MobileCaptureJsonlExportDto::NotAttempted,
                    status: MobileCaptureWriterStatusDto::default(),
                },
                database_publication_failed: false,
            }
        );
        let current = owner.begin_capture(MobileCaptureOriginDto::Manual).unwrap();
        assert!(owner.capture_writer_started(current));

        assert_eq!(
            owner.record_camera_media_provenance(camera_media_provenance(first)),
            Err(crate::MobileCameraMediaProvenanceError::StaleCaptureGeneration)
        );
        assert_eq!(owner.camera_media_provenance().len(), 1);
        assert!(
            owner
                .record_camera_media_provenance(camera_media_provenance(current))
                .is_ok()
        );
        assert_eq!(owner.camera_media_provenance().len(), 2);
    }

    fn camera_media_provenance(
        generation: MobileCaptureGenerationDto,
    ) -> crate::MobileCameraMediaProvenanceInput {
        crate::MobileCameraMediaProvenanceInput {
            capture_generation: generation,
            source: crate::MobileCameraSourceKindDto::Fixture,
            camera_path: "A:\\Novatek\\Movie\\clip.TS".into(),
            size_bytes: 42,
            camera_timecode: 7,
            camera_time: "2025/01/01 00:00:00".into(),
            ride_capture_file_name: format!("ride-{}.pevcap", generation.value),
            captured_at_monotonic_ms: 100,
            captured_at_wall_clock_ms: 200,
            clock_uncertainty: crate::MobileCameraClockUncertaintyDto::Unknown,
        }
    }
}
