//! Semantic device actions and their reported progress.

use std::collections::BTreeMap;

use crate::{ControlRefusalReason, Duration, Measured, MonotonicTimestamp};

/// Stable action identity independent of protocol commands and model names.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceActionId {
    /// Sound the wheel's horn once.
    Horn,
    /// Clear the wheel's trip-distance counter.
    ResetTripMeter,
    /// Enter or exit the wheel's gyro-calibration procedure.
    GyroCalibration,
}

/// Device-reported progress for an action with readable phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionProgress {
    /// The official app is waiting before it enables the calibration step.
    AdjustingAttitude,
    /// The official app permits the rider to start calibration.
    ReadyToCalibrate,
}

/// Semantic step selected by the action lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionStep {
    /// Invoke a single-step action.
    Invoke,
    /// Ask the wheel to enter the gyro-calibration preparation phase.
    PrepareGyroCalibration,
    /// Start gyro calibration after the wheel reports readiness.
    StartGyroCalibration,
}

/// One action invocation resolved from current device progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceActionRequest {
    /// Stable action identity.
    pub id: DeviceActionId,
    /// Procedure step selected by the Rust lifecycle owner.
    pub step: DeviceActionStep,
}

/// Lifecycle refusal before protocol authorization or transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionStateError {
    /// A procedure step is already waiting for device progress.
    Busy,
    /// The manufacturer procedure requires a wheel restart after this step.
    RestartRequired,
}

/// Result of submitting an action through the device session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionSubmissionOutcome {
    /// The request reached the transport queue.
    Accepted,
    /// The session refused the request before transport.
    Refused(ControlRefusalReason),
    /// Session or transport failure prevented submission.
    Failed,
}

/// Host-facing status of a device action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceActionStatus {
    /// No action request or usable progress is known.
    Idle,
    /// The first procedure step awaits further device progress.
    WaitingForProgress,
    /// Device progress permits the next procedure step.
    ReadyForNextStep,
    /// The command was sent but the protocol has no completion acknowledgement.
    SentWithoutConfirmation,
    /// The session refused the request before transport.
    Refused,
    /// Session or transport failure prevented submission.
    Failed,
}

/// Immutable lifecycle and evidence projection for one action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceActionSnapshot {
    /// Stable action identity.
    pub id: DeviceActionId,
    /// Most recent usable device-reported progress with original evidence.
    pub progress: Option<Measured<DeviceActionProgress>>,
    /// Rust-owned action state.
    pub status: DeviceActionStatus,
    /// Age of the current progress observation.
    pub age: Option<Duration>,
    /// Last procedure step submitted, when one exists.
    pub requested_step: Option<DeviceActionStep>,
    /// Next permitted step, or the Rust-owned reason another step is unavailable.
    pub next_step: Result<DeviceActionStep, DeviceActionStateError>,
    /// Reason for the last refusal.
    pub refusal: Option<ControlRefusalReason>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActionRecord {
    progress: Option<Measured<DeviceActionProgress>>,
    observed_at: Option<MonotonicTimestamp>,
    submitted_at: Option<MonotonicTimestamp>,
    requested_step: Option<DeviceActionStep>,
    status: DeviceActionStatus,
    refusal: Option<ControlRefusalReason>,
}

impl Default for ActionRecord {
    fn default() -> Self {
        Self {
            progress: None,
            observed_at: None,
            submitted_at: None,
            requested_step: None,
            status: DeviceActionStatus::Idle,
            refusal: None,
        }
    }
}

/// One semantic action lifecycle owner per connected device session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceActionsState {
    records: BTreeMap<DeviceActionId, ActionRecord>,
}

impl DeviceActionsState {
    /// Resolves the next legal semantic step from reported device progress.
    ///
    /// # Errors
    /// Returns [`DeviceActionStateError::Busy`] while the first calibration step is pending, or
    /// [`DeviceActionStateError::RestartRequired`] after calibration was started without an
    /// acknowledgement.
    pub fn next_request(
        &self,
        id: DeviceActionId,
    ) -> Result<DeviceActionRequest, DeviceActionStateError> {
        let record = self.records.get(&id);
        let step = match id {
            DeviceActionId::Horn | DeviceActionId::ResetTripMeter => DeviceActionStep::Invoke,
            DeviceActionId::GyroCalibration => match record {
                Some(ActionRecord {
                    status: DeviceActionStatus::SentWithoutConfirmation,
                    ..
                }) => return Err(DeviceActionStateError::RestartRequired),
                Some(ActionRecord {
                    status: DeviceActionStatus::WaitingForProgress,
                    ..
                }) => return Err(DeviceActionStateError::Busy),
                Some(ActionRecord {
                    progress: Some(progress),
                    ..
                }) if progress.value == DeviceActionProgress::ReadyToCalibrate
                    && usable_action_progress(*progress) =>
                {
                    DeviceActionStep::StartGyroCalibration
                }
                _ => DeviceActionStep::PrepareGyroCalibration,
            },
        };
        Ok(DeviceActionRequest { id, step })
    }

    /// Records the result of submitting one resolved action step.
    pub fn submission(
        &mut self,
        request: DeviceActionRequest,
        outcome: DeviceActionSubmissionOutcome,
        submitted_at: MonotonicTimestamp,
    ) {
        let record = self.records.entry(request.id).or_default();
        record.submitted_at = Some(submitted_at);
        record.requested_step = Some(request.step);
        record.refusal = None;
        record.status = match outcome {
            DeviceActionSubmissionOutcome::Accepted => match request.step {
                DeviceActionStep::PrepareGyroCalibration => DeviceActionStatus::WaitingForProgress,
                DeviceActionStep::Invoke | DeviceActionStep::StartGyroCalibration => {
                    DeviceActionStatus::SentWithoutConfirmation
                }
            },
            DeviceActionSubmissionOutcome::Refused(reason) => {
                record.refusal = Some(reason);
                DeviceActionStatus::Refused
            }
            DeviceActionSubmissionOutcome::Failed => DeviceActionStatus::Failed,
        };
    }

    /// Applies ordered, evidence-bearing procedure progress.
    pub fn observe_measured(
        &mut self,
        id: DeviceActionId,
        measured: Measured<DeviceActionProgress>,
        observed_at: MonotonicTimestamp,
    ) {
        let record = self.records.entry(id).or_default();
        if record
            .observed_at
            .is_some_and(|latest| observed_at < latest)
            || record
                .submitted_at
                .is_some_and(|submitted| observed_at < submitted)
        {
            return;
        }
        let usable = usable_action_progress(measured);
        record.progress = Some(measured);
        record.observed_at = Some(observed_at);
        record.refusal = None;
        if record.status == DeviceActionStatus::SentWithoutConfirmation
            && record.requested_step == Some(DeviceActionStep::StartGyroCalibration)
        {
            return;
        }
        record.status = match (usable, measured.value) {
            (true, DeviceActionProgress::AdjustingAttitude) => {
                DeviceActionStatus::WaitingForProgress
            }
            (true, DeviceActionProgress::ReadyToCalibrate) => DeviceActionStatus::ReadyForNextStep,
            (false, _)
                if record.requested_step == Some(DeviceActionStep::PrepareGyroCalibration) =>
            {
                DeviceActionStatus::WaitingForProgress
            }
            (false, _) => DeviceActionStatus::Idle,
        };
        if usable && measured.value == DeviceActionProgress::ReadyToCalibrate {
            record.requested_step = None;
            record.submitted_at = None;
        }
    }

    /// Removes explicitly unknown progress without inventing a procedure phase.
    pub fn invalidate_progress(&mut self, id: DeviceActionId, observed_at: MonotonicTimestamp) {
        let record = self.records.entry(id).or_default();
        if record
            .observed_at
            .is_some_and(|latest| observed_at < latest)
            || record
                .submitted_at
                .is_some_and(|submitted| observed_at < submitted)
        {
            return;
        }
        record.progress = None;
        record.observed_at = Some(observed_at);
        if record.status != DeviceActionStatus::SentWithoutConfirmation {
            record.status =
                if record.requested_step == Some(DeviceActionStep::PrepareGyroCalibration) {
                    DeviceActionStatus::WaitingForProgress
                } else {
                    DeviceActionStatus::Idle
                };
        }
    }

    /// Invalidates all action progress and requests at a device boundary.
    pub fn disconnect(&mut self) {
        self.records.clear();
    }

    /// Projects every action with known lifecycle state.
    #[must_use]
    pub fn snapshot(&self, now: MonotonicTimestamp) -> Vec<DeviceActionSnapshot> {
        self.records
            .iter()
            .map(|(&id, _)| self.snapshot_for(id, now))
            .collect()
    }

    /// Projects one action, including its initial next step before any request exists.
    #[must_use]
    pub fn snapshot_for(
        &self,
        id: DeviceActionId,
        now: MonotonicTimestamp,
    ) -> DeviceActionSnapshot {
        let record = self.records.get(&id).copied().unwrap_or_default();
        DeviceActionSnapshot {
            id,
            progress: record.progress,
            status: record.status,
            age: record.progress.and_then(|_| {
                record
                    .observed_at
                    .map(|observed| now.saturating_duration_since(observed))
            }),
            requested_step: record.requested_step,
            next_step: self.next_request(id).map(|request| request.step),
            refusal: record.refusal,
        }
    }
}

fn usable_action_progress(progress: Measured<DeviceActionProgress>) -> bool {
    progress.source == crate::ValueSource::Reported
        && progress.quality == crate::ValueQuality::Known
        && matches!(
            progress.verification,
            crate::VerificationStatus::SourceVerified
                | crate::VerificationStatus::HardwareVerified
                | crate::VerificationStatus::SourceAndHardwareVerified
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ControlRefusalReason, Measured, MonotonicTimestamp, ValueQuality, ValueSource,
        VerificationStatus,
    };

    fn time(value: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::new(value)
    }

    #[test]
    fn gyro_procedure_advances_only_from_observed_progress() {
        let mut actions = DeviceActionsState::default();
        let initial = actions.snapshot_for(DeviceActionId::GyroCalibration, time(0));
        assert_eq!(initial.status, DeviceActionStatus::Idle);
        assert_eq!(
            initial.next_step,
            Ok(DeviceActionStep::PrepareGyroCalibration)
        );
        let prepare = actions
            .next_request(DeviceActionId::GyroCalibration)
            .expect("initial procedure step is available");
        assert_eq!(prepare.step, DeviceActionStep::PrepareGyroCalibration);

        actions.submission(prepare, DeviceActionSubmissionOutcome::Accepted, time(10));
        assert_eq!(
            actions
                .next_request(DeviceActionId::GyroCalibration)
                .unwrap_err(),
            DeviceActionStateError::Busy
        );
        assert_eq!(
            actions.snapshot(time(11))[0].status,
            DeviceActionStatus::WaitingForProgress
        );
        assert_eq!(
            actions.snapshot(time(11))[0].next_step,
            Err(DeviceActionStateError::Busy)
        );

        actions.observe_measured(
            DeviceActionId::GyroCalibration,
            Measured {
                value: DeviceActionProgress::AdjustingAttitude,
                source: ValueSource::Reported,
                quality: ValueQuality::Known,
                verification: VerificationStatus::SourceVerified,
            },
            time(12),
        );
        assert_eq!(
            actions
                .next_request(DeviceActionId::GyroCalibration)
                .unwrap_err(),
            DeviceActionStateError::Busy
        );

        actions.observe_measured(
            DeviceActionId::GyroCalibration,
            Measured::reported(DeviceActionProgress::ReadyToCalibrate),
            time(13),
        );
        let start = actions
            .next_request(DeviceActionId::GyroCalibration)
            .expect("reported readiness permits the second step");
        assert_eq!(start.step, DeviceActionStep::StartGyroCalibration);
        let snapshot = actions.snapshot(time(14));
        assert_eq!(snapshot[0].status, DeviceActionStatus::ReadyForNextStep);
        assert_eq!(
            snapshot[0].next_step,
            Ok(DeviceActionStep::StartGyroCalibration)
        );
        assert_eq!(
            snapshot[0].progress.map(|entry| entry.value),
            Some(DeviceActionProgress::ReadyToCalibrate)
        );

        actions.submission(start, DeviceActionSubmissionOutcome::Accepted, time(15));
        actions.observe_measured(
            DeviceActionId::GyroCalibration,
            Measured::reported(DeviceActionProgress::ReadyToCalibrate),
            time(16),
        );
        assert_eq!(
            actions.snapshot(time(17))[0].status,
            DeviceActionStatus::SentWithoutConfirmation
        );
        assert_eq!(
            actions.snapshot(time(17))[0].next_step,
            Err(DeviceActionStateError::RestartRequired)
        );
        assert_eq!(
            actions
                .next_request(DeviceActionId::GyroCalibration)
                .unwrap_err(),
            DeviceActionStateError::RestartRequired
        );
    }

    #[test]
    fn reset_submission_is_terminal_without_timeout_or_completion_claim() {
        let mut actions = DeviceActionsState::default();
        let request = actions
            .next_request(DeviceActionId::ResetTripMeter)
            .expect("reset is available");
        assert_eq!(request.step, DeviceActionStep::Invoke);
        actions.submission(request, DeviceActionSubmissionOutcome::Accepted, time(10));
        assert_eq!(
            actions.snapshot(time(u64::MAX))[0].status,
            DeviceActionStatus::SentWithoutConfirmation
        );
    }

    #[test]
    fn horn_is_a_single_step_action_without_a_completion_claim() {
        let mut actions = DeviceActionsState::default();
        let request = actions
            .next_request(DeviceActionId::Horn)
            .expect("horn is available");
        assert_eq!(request.step, DeviceActionStep::Invoke);
        actions.submission(request, DeviceActionSubmissionOutcome::Accepted, time(10));
        assert_eq!(
            actions.snapshot(time(u64::MAX))[0].status,
            DeviceActionStatus::SentWithoutConfirmation
        );
    }

    #[test]
    fn action_progress_preserves_evidence_order_and_explicit_unknown() {
        let mut actions = DeviceActionsState::default();
        let current = Measured {
            value: DeviceActionProgress::ReadyToCalibrate,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        };
        actions.observe_measured(DeviceActionId::GyroCalibration, current, time(20));
        actions.observe_measured(
            DeviceActionId::GyroCalibration,
            Measured::estimated(DeviceActionProgress::AdjustingAttitude),
            time(19),
        );
        assert_eq!(actions.snapshot(time(30))[0].progress, Some(current));

        actions.invalidate_progress(DeviceActionId::GyroCalibration, time(21));
        actions.observe_measured(
            DeviceActionId::GyroCalibration,
            Measured::reported(DeviceActionProgress::AdjustingAttitude),
            time(20),
        );
        let snapshot = actions.snapshot(time(30));
        assert_eq!(snapshot[0].progress, None);
        assert_eq!(snapshot[0].age, None);
        assert_eq!(snapshot[0].status, DeviceActionStatus::Idle);
    }

    #[test]
    fn inferred_readiness_never_enables_calibration() {
        let mut actions = DeviceActionsState::default();
        actions.observe_measured(
            DeviceActionId::GyroCalibration,
            Measured::estimated(DeviceActionProgress::ReadyToCalibrate),
            time(10),
        );
        assert_eq!(
            actions
                .next_request(DeviceActionId::GyroCalibration)
                .expect("unverified readiness falls back to preparation")
                .step,
            DeviceActionStep::PrepareGyroCalibration
        );
        assert_eq!(
            actions.snapshot(time(11))[0].status,
            DeviceActionStatus::Idle
        );
        assert_eq!(
            actions.snapshot(time(11))[0].next_step,
            Ok(DeviceActionStep::PrepareGyroCalibration)
        );
    }

    #[test]
    fn refusal_and_failure_are_explicit_and_disconnect_clears_state() {
        let mut actions = DeviceActionsState::default();
        let reset = actions
            .next_request(DeviceActionId::ResetTripMeter)
            .expect("reset is available");
        actions.submission(
            reset,
            DeviceActionSubmissionOutcome::Refused(ControlRefusalReason::MissingArm),
            time(10),
        );
        let refused = actions.snapshot(time(11));
        assert_eq!(refused[0].status, DeviceActionStatus::Refused);
        assert_eq!(refused[0].refusal, Some(ControlRefusalReason::MissingArm));

        actions.submission(reset, DeviceActionSubmissionOutcome::Failed, time(12));
        assert_eq!(
            actions.snapshot(time(13))[0].status,
            DeviceActionStatus::Failed
        );
        actions.disconnect();
        assert!(actions.snapshot(time(14)).is_empty());
    }
}
