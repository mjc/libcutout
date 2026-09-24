//! Capture lifecycle through the existing session-state owner.

use crate::CutoutSessionStateHandle;
use cutout_core::{
    CaptureAttemptSnapshot, CaptureFinishToken, CaptureGeneration, CaptureOrigin, CaptureStage,
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
        let inner = self.lock_inner();
        let capture = &inner.session_state().capture;
        MobileCaptureLifecycleSnapshotDto {
            attempt: capture.snapshot().map(Into::into),
            can_start: capture.can_start(),
            can_pair: capture.can_pair(),
        }
    }

    /// Reserves an attempt before invoking the existing writer constructor.
    pub fn begin_capture(
        &self,
        origin: MobileCaptureOriginDto,
    ) -> Option<MobileCaptureGenerationDto> {
        self.lock_inner()
            .session_state_mut()
            .capture
            .begin(origin.into())
            .map(Into::into)
    }

    /// Reports successful writer creation, never just the user's intent to start.
    pub fn capture_writer_started(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner()
            .session_state_mut()
            .capture
            .writer_started(generation.into())
    }

    /// Reports a failed constructor for its reserved attempt.
    pub fn capture_writer_start_failed(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner()
            .session_state_mut()
            .capture
            .writer_start_failed(generation.into())
    }

    /// Claims a save operation before native asynchronous work.
    pub fn begin_capture_finish(
        &self,
        generation: MobileCaptureGenerationDto,
    ) -> Option<MobileCaptureFinishTokenDto> {
        self.lock_inner()
            .session_state_mut()
            .capture
            .begin_finish(generation.into())
            .map(Into::into)
    }

    /// Authorizes transport release only for this capture's successful save flush.
    pub fn finish_capture_flush(
        &self,
        token: MobileCaptureFinishTokenDto,
        succeeded: bool,
    ) -> bool {
        self.lock_inner()
            .session_state_mut()
            .capture
            .finish_flush(token.into(), succeeded)
    }

    /// Marks failed evidence without letting a later counter update clear it.
    pub fn capture_writer_failed(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner()
            .session_state_mut()
            .capture
            .writer_failed(generation.into())
    }

    /// Detaches active ownership while the consumed writer finishes.
    pub fn retire_capture_writer(&self, generation: MobileCaptureGenerationDto) -> bool {
        self.lock_inner()
            .session_state_mut()
            .capture
            .retire(generation.into())
    }

    /// Applies a real terminal writer result to the matching attempt only.
    pub fn complete_capture_writer(
        &self,
        generation: MobileCaptureGenerationDto,
        succeeded: bool,
    ) -> bool {
        self.lock_inner()
            .session_state_mut()
            .capture
            .complete(generation.into(), succeeded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        owner.complete_capture_writer(generation, false);
        assert!(owner.capture_lifecycle_snapshot().can_start);
    }
}
