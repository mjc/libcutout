//! Capture admission shared by transport ownership and native presentation.
//!
//! The persistence writer remains the authority for durable completion. This
//! session slice correlates its outcomes and admits native effects; counters and
//! view navigation cannot advance the lifecycle.

/// Identity of one writer attempt, including a failed start.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CaptureGeneration(u64);

impl CaptureGeneration {
    /// Reconstructs a retained identity at a binding boundary; does not admit it.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Monotonic identity within the session owner.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Why a capture owns its transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureOrigin {
    /// Evidence collected while connecting or riding normally.
    Automatic,
    /// An explicit recording that must not be replaced by picker navigation.
    Manual,
}

/// Read-only lifecycle vocabulary for native presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureStage {
    /// Writer creation is pending; no recording has been claimed.
    Starting,
    /// The writer is accepting evidence.
    Recording,
    /// One save operation owns finish admission.
    Saving,
    /// Saving failed; progress must not erase this outcome.
    SaveFailed,
    /// Transport ownership has been released; durable finalization is pending.
    Finalizing,
    /// The consumed writer supplied a durable completion receipt.
    Saved,
    /// Startup or terminal writer failure.
    Failed,
}

impl CaptureStage {
    /// Whether this attempt still reserves the active writer/transport slot.
    #[must_use]
    pub fn reserves_writer(self) -> bool {
        self == Self::Starting
            || self == Self::Recording
            || self == Self::Saving
            || self == Self::SaveFailed
    }
}

/// An immutable projection of one capture attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureAttemptSnapshot {
    /// Identity supplied with every asynchronous callback.
    pub generation: CaptureGeneration,
    /// Explicit recording versus automatic connection evidence.
    pub origin: CaptureOrigin,
    /// Writer-owned lifecycle, independent of progress counters.
    pub stage: CaptureStage,
}

/// Admission for one asynchronous save, scoped to both capture and retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureFinishToken {
    generation: CaptureGeneration,
    operation: u64,
}

impl CaptureFinishToken {
    /// Reconstructs a token at a binding boundary; acceptance still requires a match.
    #[must_use]
    pub const fn new(generation: CaptureGeneration, operation: u64) -> Self {
        Self {
            generation,
            operation,
        }
    }
    /// Writer identity of this operation.
    #[must_use]
    pub const fn generation(self) -> CaptureGeneration {
        self.generation
    }
    /// Retry identity within the capture owner.
    #[must_use]
    pub const fn operation(self) -> u64 {
        self.operation
    }
}

/// One session's capture lifecycle; native and UI layers share this owner.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptureSessionLifecycle {
    next_generation: u64,
    next_operation: u64,
    current: Option<CaptureAttemptSnapshot>,
    finishing: Option<CaptureFinishToken>,
}

impl CaptureSessionLifecycle {
    /// Current attempt, including a startup or completion failure.
    #[must_use]
    pub const fn snapshot(&self) -> Option<CaptureAttemptSnapshot> {
        self.current
    }

    /// New recordings may start only after the previous writer has been retired.
    #[must_use]
    pub fn can_start(&self) -> bool {
        self.current
            .is_none_or(|attempt| !attempt.stage.reserves_writer())
    }

    /// Ordinary pairing cannot displace an explicit recording.
    /// Automatic evidence must still be retired before starting its replacement.
    #[must_use]
    pub fn can_pair(&self) -> bool {
        self.current.is_none_or(|attempt| {
            attempt.origin == CaptureOrigin::Automatic || !attempt.stage.reserves_writer()
        })
    }

    /// Reserves a generation before attempting writer creation. Never wraps identity.
    pub fn begin(&mut self, origin: CaptureOrigin) -> Option<CaptureGeneration> {
        if !self.can_start() {
            return None;
        }
        self.next_generation = self.next_generation.checked_add(1)?;
        let generation = CaptureGeneration(self.next_generation);
        self.current = Some(CaptureAttemptSnapshot {
            generation,
            origin,
            stage: CaptureStage::Starting,
        });
        self.finishing = None;
        Some(generation)
    }

    /// Admits recording only after the real writer starts.
    pub fn writer_started(&mut self, generation: CaptureGeneration) -> bool {
        self.transition(generation, CaptureStage::Starting, CaptureStage::Recording)
    }

    /// Retains failed startup without inventing a preceding recording.
    pub fn writer_start_failed(&mut self, generation: CaptureGeneration) -> bool {
        self.transition(generation, CaptureStage::Starting, CaptureStage::Failed)
    }

    /// Claims one finish operation before native code suspends for a flush.
    pub fn begin_finish(&mut self, generation: CaptureGeneration) -> Option<CaptureFinishToken> {
        let attempt = self.current.as_mut()?;
        if attempt.generation != generation
            || attempt.origin != CaptureOrigin::Manual
            || (attempt.stage != CaptureStage::Recording
                && attempt.stage != CaptureStage::SaveFailed)
        {
            return None;
        }
        self.next_operation = self.next_operation.checked_add(1)?;
        let token = CaptureFinishToken {
            generation,
            operation: self.next_operation,
        };
        attempt.stage = CaptureStage::Saving;
        self.finishing = Some(token);
        Some(token)
    }

    /// Checks a flush completion before releasing this capture's transport.
    /// Success does not claim a saved file; the persistence receipt must follow.
    pub fn finish_flush(&mut self, token: CaptureFinishToken, succeeded: bool) -> bool {
        if self.finishing != Some(token) {
            return false;
        }
        let Some(attempt) = self.current.as_mut() else {
            return false;
        };
        if attempt.generation != token.generation || attempt.stage != CaptureStage::Saving {
            return false;
        }
        self.finishing = None;
        attempt.stage = if succeeded {
            CaptureStage::Finalizing
        } else {
            CaptureStage::SaveFailed
        };
        succeeded
    }

    /// Retains a background-flush or writer failure. Healthy progress never clears it.
    pub fn writer_failed(&mut self, generation: CaptureGeneration) -> bool {
        let Some(attempt) = self.current.as_mut() else {
            return false;
        };
        if attempt.generation != generation
            || (attempt.stage != CaptureStage::Recording
                && attempt.stage != CaptureStage::SaveFailed)
        {
            return false;
        }
        attempt.stage = CaptureStage::SaveFailed;
        true
    }

    /// Releases the active slot while an owned writer finalizes asynchronously.
    pub fn retire(&mut self, generation: CaptureGeneration) -> bool {
        let Some(attempt) = self.current.as_mut() else {
            return false;
        };
        if attempt.generation != generation
            || attempt.stage == CaptureStage::Starting
            || (!attempt.stage.reserves_writer() && attempt.stage != CaptureStage::Finalizing)
        {
            return false;
        }
        attempt.stage = CaptureStage::Finalizing;
        self.finishing = None;
        true
    }

    /// Applies a consumed writer's outcome only to its own current attempt.
    /// Old outcomes remain attributable through their event/artifact identity.
    pub fn complete(&mut self, generation: CaptureGeneration, succeeded: bool) -> bool {
        self.transition(
            generation,
            CaptureStage::Finalizing,
            if succeeded {
                CaptureStage::Saved
            } else {
                CaptureStage::Failed
            },
        )
    }

    fn transition(
        &mut self,
        generation: CaptureGeneration,
        from: CaptureStage,
        to: CaptureStage,
    ) -> bool {
        let Some(attempt) = self.current.as_mut() else {
            return false;
        };
        if attempt.generation != generation || attempt.stage != from {
            return false;
        }
        attempt.stage = to;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording(origin: CaptureOrigin) -> (CaptureSessionLifecycle, CaptureGeneration) {
        let mut owner = CaptureSessionLifecycle::default();
        let generation = owner.begin(origin).unwrap();
        assert!(owner.writer_started(generation));
        (owner, generation)
    }

    #[test]
    fn manual_capture_blocks_pairing_and_writer_replacement() {
        let (mut owner, generation) = recording(CaptureOrigin::Manual);
        assert!(!owner.can_pair());
        assert_eq!(owner.begin(CaptureOrigin::Automatic), None);
        assert_eq!(owner.snapshot().unwrap().generation, generation);
        assert!(owner.retire(generation));
        assert!(owner.can_pair());
        assert!(owner.begin(CaptureOrigin::Automatic).is_some());
    }

    #[test]
    fn failed_start_has_an_identity_but_never_claims_recording() {
        let mut owner = CaptureSessionLifecycle::default();
        let generation = owner.begin(CaptureOrigin::Manual).unwrap();
        assert!(!owner.can_start());
        assert!(!owner.can_pair());
        assert!(owner.writer_start_failed(generation));
        assert_eq!(owner.snapshot().unwrap().stage, CaptureStage::Failed);
        assert!(owner.can_start());
        assert!(!owner.writer_started(generation));
    }

    #[test]
    fn failed_save_survives_progress_and_retry_tokens_are_fenced() {
        let (mut owner, generation) = recording(CaptureOrigin::Manual);
        let first = owner.begin_finish(generation).unwrap();
        assert_eq!(owner.begin_finish(generation), None);
        assert!(!owner.finish_flush(first, false));
        for _ in 0..3 {
            owner.writer_failed(generation);
            assert_eq!(owner.snapshot().unwrap().stage, CaptureStage::SaveFailed);
        }
        let retry = owner.begin_finish(generation).unwrap();
        assert_ne!(first, retry);
        assert!(!owner.finish_flush(first, true));
        assert!(owner.finish_flush(retry, true));
        assert_eq!(owner.snapshot().unwrap().stage, CaptureStage::Finalizing);
        assert!(owner.complete(generation, true));
        assert_eq!(owner.snapshot().unwrap().stage, CaptureStage::Saved);
    }

    #[test]
    fn late_completion_cannot_retire_a_new_recording() {
        for succeeded in [false, true] {
            let (mut owner, first) = recording(CaptureOrigin::Manual);
            owner.retire(first);
            let second = owner.begin(CaptureOrigin::Manual).unwrap();
            owner.writer_started(second);
            assert!(!owner.complete(first, succeeded));
            assert_eq!(owner.snapshot().unwrap().generation, second);
            assert_eq!(owner.snapshot().unwrap().stage, CaptureStage::Recording);
            assert!(!owner.can_start());
        }
    }

    #[test]
    fn automatic_capture_cannot_authorize_a_manual_stop() {
        let (mut owner, generation) = recording(CaptureOrigin::Automatic);
        assert_eq!(owner.begin_finish(generation), None);
        assert!(owner.can_pair());
        assert!(!owner.can_start());
    }

    #[test]
    fn generation_exhaustion_does_not_reuse_an_old_identity() {
        let mut owner = CaptureSessionLifecycle {
            next_generation: u64::MAX,
            ..Default::default()
        };
        assert_eq!(owner.begin(CaptureOrigin::Manual), None);
        assert_eq!(owner.snapshot(), None);
    }
}
