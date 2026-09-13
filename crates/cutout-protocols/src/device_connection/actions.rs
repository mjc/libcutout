use crate::{ActionDescriptor, ActionRequestError, DeviceConnectionSession, DeviceConnectionStep};
use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, DeviceActionId, DeviceActionSnapshot,
    MonotonicTimestamp,
};

/// Semantic action refusal before protocol execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DeviceActionSubmissionError {
    /// The request no longer owns a verified connection.
    #[error("connection unavailable")]
    ConnectionUnavailable,
    /// The procedure is waiting for measured progress.
    #[error("action busy")]
    Busy,
    /// The prior procedure requires a wheel restart.
    #[error("wheel restart required")]
    RestartRequired,
    /// The profile refuses the requested action.
    #[error(transparent)]
    Profile(#[from] ActionRequestError),
}

/// Immutable action state paired with its producing connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceActionsSnapshot {
    /// Current connection token and readiness.
    pub connection: ConnectionAttemptSnapshot,
    /// Latest accepted monotonic input timestamp.
    pub at: MonotonicTimestamp,
    /// Shared lifecycle and measured progress.
    pub actions: Vec<DeviceActionSnapshot>,
}

impl DeviceConnectionSession {
    /// Returns the current device's semantic action catalog.
    #[must_use]
    pub fn action_descriptors(&self, validation_mode: bool) -> Vec<ActionDescriptor> {
        self.device.as_ref().map_or_else(Vec::new, |device| {
            device.control_profile().action_descriptors(validation_mode)
        })
    }

    /// Pairs action state and connection identity without cross-device reads.
    #[must_use]
    pub fn actions_snapshot(&self) -> DeviceActionsSnapshot {
        DeviceActionsSnapshot {
            connection: self.state.connection.snapshot().clone(),
            at: self.last_input_at,
            actions: self
                .action_descriptors(false)
                .into_iter()
                .map(|descriptor| {
                    self.state
                        .actions
                        .snapshot_for(descriptor.id, self.last_input_at)
                })
                .collect(),
        }
    }

    /// Submits the next legal procedure step through existing live protocol guards.
    ///
    /// # Errors
    /// Refuses stale identity, unavailable actions or a procedure that cannot advance.
    pub fn submit_action(
        &mut self,
        token: &ConnectionAttemptToken,
        id: DeviceActionId,
        validation_mode: bool,
        at: MonotonicTimestamp,
    ) -> Result<DeviceConnectionStep, DeviceActionSubmissionError> {
        if !self.state.connection.is_verified(token) {
            return Err(DeviceActionSubmissionError::ConnectionUnavailable);
        }
        let profile = self
            .device
            .as_ref()
            .ok_or(DeviceActionSubmissionError::ConnectionUnavailable)?
            .control_profile();
        let request = self
            .state
            .actions
            .next_request(id)
            .map_err(|error| match error {
                cutout_core::DeviceActionStateError::Busy => DeviceActionSubmissionError::Busy,
                cutout_core::DeviceActionStateError::RestartRequired => {
                    DeviceActionSubmissionError::RestartRequired
                }
            })?;
        let command = profile.action_command(request, validation_mode)?;
        let step = self
            .ingest_validated(
                token,
                &cutout_core::SessionInputDto::CommandAt {
                    command: command.into(),
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: at.get(),
                    },
                },
            )
            .ok_or(DeviceActionSubmissionError::ConnectionUnavailable)?;
        let outcome = step.result.error.map_or(
            cutout_core::DeviceActionSubmissionOutcome::Accepted,
            |refusal| cutout_core::DeviceActionSubmissionOutcome::Refused(refusal.reason),
        );
        self.state.actions.submission(request, outcome, at);
        Ok(step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{DeviceActionStatus, SessionInputDto, SessionOutput, TransportAction};

    #[test]
    fn action_catalog_publishes_initial_next_step_without_a_request() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        let snapshot = owner.actions_snapshot();
        let gyro = snapshot
            .actions
            .iter()
            .find(|action| action.id == DeviceActionId::GyroCalibration)
            .expect("initial procedure state is visible before pressing a control");
        assert_eq!(gyro.status, DeviceActionStatus::Idle);
        assert_eq!(
            gyro.next_step,
            Ok(cutout_core::DeviceActionStep::PrepareGyroCalibration)
        );
        assert!(gyro.requested_step.is_none());
        assert_eq!(snapshot.connection.token, Some(token));
    }

    #[test]
    fn horn_is_available_without_a_speed_sample_or_stationary_arm() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        assert!(
            owner
                .device
                .as_ref()
                .unwrap()
                .current_snapshot()
                .speed
                .is_none()
        );
        let result = owner
            .submit_action(
                &token,
                DeviceActionId::Horn,
                false,
                MonotonicTimestamp::new(2),
            )
            .unwrap();
        assert_eq!(result.result.error, None);
        assert!(result.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )));
        let snapshot = owner.actions_snapshot();
        assert_eq!(snapshot.actions[0].id, DeviceActionId::Horn);
        assert_eq!(
            snapshot.actions[0].status,
            DeviceActionStatus::SentWithoutConfirmation
        );
    }

    fn gyro_readback(phase: u8, at: u64) -> SessionInputDto {
        let mut frame = vec![0_u8; 75];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 71]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        frame[46] = 8;
        frame[56] = phase;
        let checksum = crc32fast::hash(&frame[..71]);
        frame[71..].copy_from_slice(&checksum.to_be_bytes());
        SessionInputDto::Notification {
            channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: frame,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: at },
        }
    }

    #[test]
    fn actual_gyro_progress_advances_only_current_attempt_and_never_claims_completion() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        let _ = owner.ingest(&token, &gyro_readback(128, 2));
        let step = owner
            .submit_action(
                &token,
                DeviceActionId::GyroCalibration,
                true,
                MonotonicTimestamp::new(3),
            )
            .unwrap();
        assert_eq!(step.result.error, None);
        assert_eq!(
            owner
                .actions_snapshot()
                .actions
                .into_iter()
                .find(|action| action.id == DeviceActionId::GyroCalibration)
                .unwrap()
                .status,
            DeviceActionStatus::WaitingForProgress
        );
        assert_eq!(
            owner.submit_action(
                &token,
                DeviceActionId::GyroCalibration,
                true,
                MonotonicTimestamp::new(4)
            ),
            Err(DeviceActionSubmissionError::Busy)
        );
        let _ = owner.ingest(&token, &gyro_readback(2, 5));
        assert_eq!(
            owner
                .actions_snapshot()
                .actions
                .into_iter()
                .find(|action| action.id == DeviceActionId::GyroCalibration)
                .unwrap()
                .status,
            DeviceActionStatus::ReadyForNextStep
        );
        let step = owner
            .submit_action(
                &token,
                DeviceActionId::GyroCalibration,
                true,
                MonotonicTimestamp::new(6),
            )
            .unwrap();
        assert_eq!(step.result.error, None);
        assert_eq!(
            owner
                .actions_snapshot()
                .actions
                .into_iter()
                .find(|action| action.id == DeviceActionId::GyroCalibration)
                .unwrap()
                .status,
            DeviceActionStatus::SentWithoutConfirmation
        );
        let _ = owner.ingest(&token, &gyro_readback(2, 7));
        assert_eq!(
            owner.submit_action(
                &token,
                DeviceActionId::GyroCalibration,
                true,
                MonotonicTimestamp::new(8)
            ),
            Err(DeviceActionSubmissionError::RestartRequired)
        );
        owner.begin_attempt("B".into(), MonotonicTimestamp::new(9));
        assert!(owner.ingest(&token, &gyro_readback(2, 10)).is_none());
        assert!(owner.actions_snapshot().actions.is_empty());
    }

    #[test]
    fn semantic_reset_is_unconfirmed_and_rejects_replacement_identity() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        let _ = owner.ingest(
            &token,
            &SessionInputDto::Notification {
                channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
                bytes: frame,
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
            },
        );
        let before_refusal = owner.actions_snapshot();
        assert_eq!(
            owner.submit_action(
                &token,
                DeviceActionId::ResetTripMeter,
                false,
                MonotonicTimestamp::new(3)
            ),
            Err(DeviceActionSubmissionError::Profile(
                ActionRequestError::Unverified
            ))
        );
        assert_eq!(owner.actions_snapshot(), before_refusal);
        let result = owner
            .submit_action(
                &token,
                DeviceActionId::ResetTripMeter,
                true,
                MonotonicTimestamp::new(3),
            )
            .unwrap();
        assert!(result.result.error.is_none());
        assert!(result.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )));
        let snapshot = owner.actions_snapshot();
        assert_eq!(snapshot.connection.token, Some(token.clone()));
        assert_eq!(
            snapshot
                .actions
                .iter()
                .find(|action| action.id == DeviceActionId::ResetTripMeter)
                .unwrap()
                .status,
            DeviceActionStatus::SentWithoutConfirmation
        );
        owner.begin_attempt("B".into(), MonotonicTimestamp::new(4));
        assert!(owner.actions_snapshot().actions.is_empty());
        assert_eq!(
            owner.submit_action(
                &token,
                DeviceActionId::ResetTripMeter,
                false,
                MonotonicTimestamp::new(5)
            ),
            Err(DeviceActionSubmissionError::ConnectionUnavailable)
        );
    }
}
