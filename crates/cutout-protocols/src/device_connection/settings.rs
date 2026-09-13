use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, DeviceSettingSnapshot, DeviceSettingValue,
    MonotonicTimestamp, SettingId,
};

use crate::{
    DeviceConnectionSession, DeviceConnectionStep, SettingDescriptor, SettingsRequestError,
};

/// A semantic request rejected before the protocol accepted it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DeviceSettingRequestError {
    /// The supplied connection no longer permits device writes.
    #[error("connection unavailable")]
    ConnectionUnavailable,
    /// Descriptor validation refused the request.
    #[error(transparent)]
    Profile(#[from] SettingsRequestError),
}

/// Settings observations paired with the immutable connection that owns them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSettingsSnapshot {
    /// Connection identity and admission state.
    pub connection: ConnectionAttemptSnapshot,
    /// Latest accepted host input time used to calculate observation age.
    pub at: MonotonicTimestamp,
    /// Observations and pending requests from the sole shared settings owner.
    pub settings: Vec<DeviceSettingSnapshot>,
}

impl DeviceConnectionSession {
    /// Returns semantic controls available on the selected exact protocol model.
    #[must_use]
    pub fn settings_descriptors(&self, validation_mode: bool) -> Vec<SettingDescriptor> {
        self.device.as_ref().map_or_else(Vec::new, |device| {
            device.control_profile().descriptors(validation_mode)
        })
    }

    /// Returns immutable observed/requested state for the current connection.
    #[must_use]
    pub fn settings_snapshot(&self) -> DeviceSettingsSnapshot {
        DeviceSettingsSnapshot {
            connection: self.state.connection.snapshot().clone(),
            at: self.last_input_at,
            settings: self.state.settings.snapshot(self.last_input_at),
        }
    }

    /// Resolves and submits one semantic request through existing protocol guards.
    ///
    /// # Errors
    /// Returns an identity or descriptor refusal without replacing an earlier request.
    pub fn submit_setting(
        &mut self,
        token: &ConnectionAttemptToken,
        id: SettingId,
        value: DeviceSettingValue,
        validation_mode: bool,
        at: MonotonicTimestamp,
    ) -> Result<DeviceConnectionStep, DeviceSettingRequestError> {
        if !self.state.connection.is_verified(token) {
            return Err(DeviceSettingRequestError::ConnectionUnavailable);
        }
        let device = self
            .device
            .as_mut()
            .ok_or(DeviceSettingRequestError::ConnectionUnavailable)?;
        let profile = device.control_profile();
        let command = profile.command(id, value, validation_mode)?;
        let confirmation_supported = profile
            .descriptors(validation_mode)
            .iter()
            .any(|descriptor| descriptor.id == id && descriptor.confirmation_supported);
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
            .ok_or(DeviceSettingRequestError::ConnectionUnavailable)?;
        let outcome = step
            .result
            .error
            .map_or(cutout_core::SettingSubmissionOutcome::Accepted, |refusal| {
                cutout_core::SettingSubmissionOutcome::Refused(refusal.reason)
            });
        self.state
            .settings
            .submission(id, value, outcome, confirmation_supported, at);
        Ok(step)
    }
}

#[cfg(test)]
mod tests {
    use cutout_core::{SessionOutput, SettingCommandStatus, TransportAction};

    use super::*;

    #[test]
    fn transport_ingress_cannot_bypass_semantic_write_validation() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        let before = owner.settings_snapshot();
        for command in [
            cutout_core::DeviceCommand::SetLights(cutout_core::LightState::On),
            cutout_core::DeviceCommand::ResetTripMeter,
        ] {
            assert!(
                owner
                    .ingest(
                        &token,
                        &cutout_core::SessionInputDto::Command(command.into())
                    )
                    .is_none()
            );
            assert!(
                owner
                    .ingest(
                        &token,
                        &cutout_core::SessionInputDto::CommandAt {
                            command: command.into(),
                            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
                        }
                    )
                    .is_none()
            );
        }
        assert_eq!(owner.settings_snapshot(), before);
        assert!(
            owner
                .ingest(
                    &token,
                    &cutout_core::SessionInputDto::Command(
                        cutout_core::DeviceCommand::RequestTelemetry.into()
                    )
                )
                .is_some()
        );
    }

    #[test]
    fn semantic_submission_uses_live_session_and_records_unconfirmed_outcome() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        let _ = owner.ingest(&token, &super::super::tests::aero_mode_telemetry(1));
        let result = owner
            .submit_setting(
                &token,
                SettingId::HighBeam,
                DeviceSettingValue::Boolean(true),
                false,
                MonotonicTimestamp::new(2),
            )
            .unwrap();
        assert!(result.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )));
        let snapshot = owner.settings_snapshot();
        assert_eq!(snapshot.connection.token, Some(token));
        let headlight = snapshot
            .settings
            .iter()
            .find(|setting| setting.id == SettingId::HighBeam)
            .unwrap();
        assert_eq!(headlight.requested, Some(DeviceSettingValue::Boolean(true)));
        assert_eq!(
            headlight.status,
            SettingCommandStatus::SentWithoutConfirmation
        );
    }

    #[test]
    fn invalid_or_stale_requests_do_not_replace_a_pending_request() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        let _ = owner.ingest(&token, &super::super::tests::aero_mode_telemetry(1));
        owner
            .submit_setting(
                &token,
                SettingId::HighBeam,
                DeviceSettingValue::Boolean(true),
                false,
                MonotonicTimestamp::new(2),
            )
            .unwrap();
        let before = owner.settings_snapshot();
        assert_eq!(
            owner.submit_setting(
                &token,
                SettingId::HighBeam,
                DeviceSettingValue::Number(7),
                false,
                MonotonicTimestamp::new(3)
            ),
            Err(DeviceSettingRequestError::Profile(
                SettingsRequestError::InvalidValue
            ))
        );
        assert_eq!(owner.settings_snapshot(), before);
        owner.begin_attempt("B".into(), MonotonicTimestamp::new(4));
        assert_eq!(
            owner.submit_setting(
                &token,
                SettingId::HighBeam,
                DeviceSettingValue::Boolean(false),
                false,
                MonotonicTimestamp::new(5)
            ),
            Err(DeviceSettingRequestError::ConnectionUnavailable)
        );
        assert!(owner.settings_snapshot().settings.is_empty());
    }

    #[test]
    fn validation_mode_still_requires_fresh_stationary_protocol_evidence() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        assert_eq!(
            owner.submit_setting(
                &token,
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(80),
                false,
                MonotonicTimestamp::new(2)
            ),
            Err(DeviceSettingRequestError::Profile(
                SettingsRequestError::Unverified
            ))
        );
        let refused = owner
            .submit_setting(
                &token,
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(80),
                true,
                MonotonicTimestamp::new(2),
            )
            .unwrap();
        assert_eq!(
            refused.result.error.unwrap().reason,
            cutout_core::ControlRefusalReason::MissingArm
        );
        assert!(!refused.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )));
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        let _ = owner.ingest(
            &token,
            &cutout_core::SessionInputDto::Notification {
                channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
                bytes: frame,
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 10 },
            },
        );
        let accepted = owner
            .submit_setting(
                &token,
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(80),
                true,
                MonotonicTimestamp::new(11),
            )
            .unwrap();
        assert!(accepted.result.error.is_none());
        assert!(accepted.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )));
        let expired = owner
            .submit_setting(
                &token,
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(85),
                true,
                MonotonicTimestamp::new(10_000),
            )
            .unwrap();
        assert!(expired.result.error.is_some());
        assert!(!expired.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )));
    }
}
