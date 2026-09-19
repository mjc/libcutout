use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, DeviceSettingSnapshot, DeviceSettingValue,
    MonotonicTimestamp, SettingId,
};

use crate::{
    DeviceConnectionSession, DeviceConnectionStep, SettingCompletionStrategy, SettingDescriptor,
    SettingsRequestError,
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
    pub fn settings_descriptors(&self) -> Vec<SettingDescriptor> {
        self.device.as_ref().map_or_else(Vec::new, |device| {
            device
                .control_profile()
                .descriptors(self.validation_authorized())
        })
    }

    /// Grants protocol validation controls only to a verified decoder.
    pub fn authorize_validation(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.state.connection.is_verified(token) || self.device.is_none() {
            return false;
        }
        self.validation_grant = Some(super::ValidationGrant::for_token(token));
        true
    }

    /// Revokes protocol validation controls for the current attempt.
    pub fn revoke_validation(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.state.connection.is_current(token) {
            return false;
        }
        self.validation_grant = None;
        true
    }

    /// Returns the current attempt's validation authorization.
    #[must_use]
    pub fn validation_authorized(&self) -> bool {
        self.validation_grant.is_some_and(|grant| {
            self.state
                .connection
                .snapshot()
                .token
                .as_ref()
                .is_some_and(|token| grant.matches(token))
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
        at: MonotonicTimestamp,
    ) -> Result<DeviceConnectionStep, DeviceSettingRequestError> {
        if !self.state.connection.is_verified(token) {
            return Err(DeviceSettingRequestError::ConnectionUnavailable);
        }
        let validation_authorized = self.validation_authorized();
        let device = self
            .device
            .as_mut()
            .ok_or(DeviceSettingRequestError::ConnectionUnavailable)?;
        let profile = device.control_profile();
        let command = profile.command(id, value, validation_authorized)?;
        let completion = profile
            .descriptors(validation_authorized)
            .iter()
            .find(|descriptor| descriptor.id == id)
            .map_or(SettingCompletionStrategy::SubmissionOnly, |descriptor| {
                descriptor.completion
            });
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
            .submission(id, value, outcome, completion.supports_readback(), at);
        Ok(step)
    }
}

#[cfg(test)]
mod tests {
    use cutout_core::{SessionOutput, SettingCommandStatus, TransportAction};

    use super::*;

    fn nf2557_notifications() -> Vec<Vec<u8>> {
        include_str!("../../fixtures/nosfet-aero/nf2557-2026-06-21-notifications.hex")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                (0..line.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&line[offset..offset + 2], 16).unwrap())
                    .collect()
            })
            .collect()
    }

    fn connected_nf2557(
        notifications: &[Vec<u8>],
    ) -> (DeviceConnectionSession, ConnectionAttemptToken) {
        let mut owner = DeviceConnectionSession::default();
        owner.begin_attempt("NF2557".into(), MonotonicTimestamp::new(0));
        let token = owner.snapshot().connection.token.unwrap();
        owner.state.connection.connected(&token);
        for bytes in notifications {
            let _ = owner
                .observe_for_attempt(&token, crate::DeviceDetectionEvent::Notification { bytes });
        }
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        assert_eq!(
            owner.snapshot().identity.unwrap().model,
            Some(&crate::NOSFET_AERO_REGISTRY_ENTRY)
        );
        owner
            .ingest(
                &token,
                &cutout_core::SessionInputDto::LinkUp {
                    monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                },
            )
            .unwrap();
        (owner, token)
    }

    fn replay_nf2557(
        owner: &mut DeviceConnectionSession,
        token: &ConnectionAttemptToken,
        notifications: &[Vec<u8>],
        at: u64,
    ) {
        for bytes in notifications {
            owner
                .ingest(
                    token,
                    &cutout_core::SessionInputDto::Notification {
                        channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
                        bytes: bytes.clone(),
                        monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: at },
                    },
                )
                .unwrap();
        }
    }

    #[test]
    fn nf2557_reported_tune_values_send_when_stopped_and_refuse_while_moving() {
        // Complete CRC-protected packets from the 2026-09-18 phone capture
        // covering the reported 15:15 Tune failures. The first reports 0 km/h;
        // the second reports 3.4 km/h. No device write is performed by this test.
        let stopped = hex_literal::hex!("dc5a5c532f3b000036350001f3f5001e001a123604af000002260230a8f607800081011280c80000808080808080022880805a80800fed0fec0fed0fed0fed0fe70fec0fed0fed0fed0fed0fe70fec0fed0fe80e425347").to_vec();
        let moving = hex_literal::hex!("dc5a5c532f35002236470001f407001efffb124f04b0000002260226a8f607800064032080c80000808080808080022880805a80800fed0fed0fed0fed0fed0fe70fec0fed0fed0fed0fed0fe70fec0fed0fe945e6b8ce").to_vec();
        for (id, value, magic, offset, wire_value) in [
            (SettingId::TiltbackSpeed, 550, *b"LdAp", 12, 55),
            (SettingId::PwmTiltback, 80, *b"LdAp", 13, 80),
            (SettingId::LateralTiltLimit, 45, *b"LkAp", 17, 45),
            (SettingId::SpeedAlarmThreshold, 560, *b"LkAp", 12, 56),
        ] {
            let (mut owner, token) = connected_nf2557(&[stopped.clone(), moving.clone()]);
            for (at, frame, accepted) in [
                (10, &stopped, true),
                (20, &moving, false),
                (30, &stopped, true),
            ] {
                replay_nf2557(&mut owner, &token, std::slice::from_ref(frame), at);
                let step = owner
                    .submit_setting(
                        &token,
                        id,
                        DeviceSettingValue::Number(value),
                        MonotonicTimestamp::new(at + 1),
                    )
                    .unwrap();
                let writes: Vec<_> = step
                    .result
                    .outputs
                    .iter()
                    .filter_map(|output| match output {
                        SessionOutput::Transport(TransportAction::Write { bytes, .. }) => {
                            Some(bytes.as_slice())
                        }
                        _ => None,
                    })
                    .collect();
                if accepted {
                    assert_eq!(step.result.error, None, "{id:?}");
                    assert_eq!(writes.len(), 1, "{id:?}");
                    assert_eq!(&writes[0][..4], &magic, "{id:?}");
                    assert_eq!(writes[0][offset], wire_value, "{id:?}");
                } else {
                    assert_eq!(
                        step.result.error.unwrap().reason,
                        cutout_core::ControlRefusalReason::MissingArm,
                        "{id:?}"
                    );
                    assert!(writes.is_empty(), "{id:?}");
                }
            }
        }
    }

    #[test]
    fn nf2557_high_beam_on_off_emit_once_and_preserve_unknown_readback() {
        let notifications = nf2557_notifications();
        let (mut owner, token) = connected_nf2557(&notifications);
        for (at, enabled, expected) in [
            (10, true, &b"SetLightON"[..]),
            (5_010, false, &b"SetLightOFF"[..]),
        ] {
            replay_nf2557(&mut owner, &token, &notifications, at);
            let step = owner
                .submit_setting(
                    &token,
                    SettingId::HighBeam,
                    DeviceSettingValue::Boolean(enabled),
                    MonotonicTimestamp::new(at + 1),
                )
                .unwrap();
            assert!(step.result.error.is_none());
            let writes: Vec<_> = step
                .result
                .outputs
                .iter()
                .filter_map(|output| match output {
                    SessionOutput::Transport(TransportAction::Write {
                        channel,
                        bytes,
                        mode,
                    }) => Some((*channel, bytes.as_slice(), *mode)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                writes,
                [(
                    crate::VETERAN_DATA_CHANNEL,
                    expected,
                    cutout_core::WriteMode::WithoutResponse
                )]
            );

            // Further real telemetry must not turn a request into a measured light state.
            replay_nf2557(&mut owner, &token, &notifications, at + 2);
            for milliseconds in [at + 3, at + 102, at + 3_002] {
                let tick = owner
                    .ingest(
                        &token,
                        &cutout_core::SessionInputDto::Tick {
                            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds },
                        },
                    )
                    .unwrap();
                assert!(!tick.result.outputs.iter().any(|output| matches!(
                    output,
                    SessionOutput::Transport(TransportAction::Write { .. })
                )));
                let snapshot = owner.settings_snapshot();
                let light = snapshot
                    .settings
                    .iter()
                    .find(|setting| setting.id == SettingId::HighBeam)
                    .unwrap();
                assert_eq!(light.current, None);
                assert_eq!(light.measured, None);
                assert_eq!(light.requested, Some(DeviceSettingValue::Boolean(enabled)));
                assert_eq!(light.status, SettingCommandStatus::SentWithoutConfirmation);
            }
        }
    }

    #[test]
    fn nf2557_high_beam_requires_fresh_session_speed_even_after_a_previous_write() {
        let notifications = nf2557_notifications();
        for enabled in [false, true] {
            let (mut owner, token) = connected_nf2557(&notifications);
            for at in [2, 2_011] {
                let refused = owner
                    .submit_setting(
                        &token,
                        SettingId::HighBeam,
                        DeviceSettingValue::Boolean(enabled),
                        MonotonicTimestamp::new(at),
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
                let snapshot = owner.settings_snapshot();
                let light = snapshot
                    .settings
                    .iter()
                    .find(|setting| setting.id == SettingId::HighBeam)
                    .unwrap();
                assert_eq!(light.current, None);
                assert_eq!(light.requested, Some(DeviceSettingValue::Boolean(enabled)));
                assert_eq!(light.status, SettingCommandStatus::Refused);
                if at == 2 {
                    replay_nf2557(&mut owner, &token, &notifications, 10);
                    let accepted = owner
                        .submit_setting(
                            &token,
                            SettingId::HighBeam,
                            DeviceSettingValue::Boolean(!enabled),
                            MonotonicTimestamp::new(11),
                        )
                        .unwrap();
                    assert!(accepted.result.error.is_none());
                    assert!(accepted.result.outputs.iter().any(|output| matches!(
                        output,
                        SessionOutput::Transport(TransportAction::Write { .. })
                    )));
                }
            }
        }
    }

    #[test]
    fn transport_ingress_cannot_bypass_semantic_write_validation() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        let before = owner.settings_snapshot();
        for command in [
            cutout_core::DeviceCommand::SetLights(cutout_core::LightState::On),
            cutout_core::DeviceCommand::InvokeAction(cutout_core::DeviceActionRequest {
                id: cutout_core::DeviceActionId::ResetTripMeter,
                step: cutout_core::DeviceActionStep::Invoke,
            }),
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
                MonotonicTimestamp::new(2),
            )
            .unwrap();
        let before = owner.settings_snapshot();
        assert_eq!(
            owner.submit_setting(
                &token,
                SettingId::HighBeam,
                DeviceSettingValue::Number(7),
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
                MonotonicTimestamp::new(5)
            ),
            Err(DeviceSettingRequestError::ConnectionUnavailable)
        );
        assert!(owner.settings_snapshot().settings.is_empty());
    }

    #[test]
    fn ordinary_aero_settings_emit_checked_frames_only_with_current_stationary_evidence() {
        let profile = crate::aero_control_profile();
        for descriptor in profile
            .descriptors(false)
            .into_iter()
            .filter(|item| item.access == crate::SettingAccess::Writable)
        {
            let value = match descriptor.control {
                crate::SettingControl::Boolean => DeviceSettingValue::Boolean(true),
                crate::SettingControl::Number { minimum, .. } => {
                    DeviceSettingValue::Number(minimum)
                }
                crate::SettingControl::Choices(choices) => {
                    DeviceSettingValue::Choice(choices[0].id)
                }
                crate::SettingControl::ReadOnly => unreachable!(),
            };
            let id = descriptor.id;
            for (speed, age, accepted) in [
                (None, 0, false),
                (Some(20_u16), 1, false),
                (Some(0), 2_001, false),
                (Some(0), 1, true),
            ] {
                let mut owner = DeviceConnectionSession::default();
                let token = super::super::tests::connected_aero(&mut owner);
                if let Some(speed) = speed {
                    let mut frame = vec![0_u8; 42];
                    frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
                    frame[6..8].copy_from_slice(&speed.to_be_bytes());
                    frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
                    owner
                        .ingest(
                            &token,
                            &cutout_core::SessionInputDto::Notification {
                                channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
                                bytes: frame,
                                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 10 },
                            },
                        )
                        .unwrap();
                }
                assert!(!owner.validation_authorized());
                let step = owner
                    .submit_setting(&token, id, value, MonotonicTimestamp::new(10 + age))
                    .unwrap();
                let mut writes: Vec<Vec<u8>> = step
                    .result
                    .outputs
                    .iter()
                    .filter_map(|output| match output {
                        SessionOutput::Transport(TransportAction::Write { bytes, .. }) => {
                            Some(bytes.as_slice().to_vec())
                        }
                        _ => None,
                    })
                    .collect();
                if id == SettingId::LateralTiltLimit && accepted {
                    let follow_up = owner
                        .ingest(
                            &token,
                            &cutout_core::SessionInputDto::Tick {
                                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 11 },
                            },
                        )
                        .unwrap();
                    writes.extend(follow_up.result.outputs.iter().filter_map(
                        |output| match output {
                            SessionOutput::Transport(TransportAction::Write { bytes, .. }) => {
                                Some(bytes.as_slice().to_vec())
                            }
                            _ => None,
                        },
                    ));
                }
                assert_eq!(
                    step.result.error.is_none(),
                    accepted,
                    "{id:?} speed={speed:?} age={age}"
                );
                if accepted {
                    let command = profile.command(id, value, false).unwrap();
                    let expected: Vec<_> = crate::NosfetDialect::encode_settings_sequence(command)
                        .map(|sequence| {
                            sequence
                                .steps
                                .into_iter()
                                .map(|step| step.payload)
                                .collect()
                        })
                        .or_else(|| {
                            crate::NosfetDialect::encode(command)
                                .map(|encoded| vec![encoded.payload])
                        })
                        .unwrap();
                    assert_eq!(
                        writes,
                        expected
                            .iter()
                            .map(|payload| payload.as_slice().to_vec())
                            .collect::<Vec<_>>(),
                        "{id:?}"
                    );
                    let state = owner
                        .settings_snapshot()
                        .settings
                        .into_iter()
                        .find(|item| item.id == id)
                        .unwrap();
                    assert_eq!(state.requested, Some(value));
                    assert_eq!(
                        state.status,
                        if descriptor.completion.supports_readback() {
                            SettingCommandStatus::WaitingForConfirmation
                        } else {
                            SettingCommandStatus::SentWithoutConfirmation
                        }
                    );
                } else {
                    assert!(writes.is_empty(), "{id:?}");
                }
                owner.begin_attempt("replacement".into(), MonotonicTimestamp::new(3_000));
                assert_eq!(
                    owner.submit_setting(&token, id, value, MonotonicTimestamp::new(3_001)),
                    Err(DeviceSettingRequestError::ConnectionUnavailable)
                );
            }
        }
    }

    #[test]
    fn ordinary_settings_require_fresh_stationary_protocol_evidence() {
        let mut owner = DeviceConnectionSession::default();
        let token = super::super::tests::connected_aero(&mut owner);
        assert!(!owner.validation_authorized());
        let refused = owner
            .submit_setting(
                &token,
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(80),
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
                MonotonicTimestamp::new(10_000),
            )
            .unwrap();
        assert!(expired.result.error.is_some());
        assert!(!expired.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(TransportAction::Write { .. })
        )));
    }
    #[test]
    fn validation_authorization_is_owned_by_one_connection_attempt() {
        let mut owner = DeviceConnectionSession::default();
        let first = super::super::tests::connected_aero(&mut owner);
        assert_eq!(
            owner
                .settings_descriptors()
                .into_iter()
                .find(|item| item.id == SettingId::PwmTiltback)
                .unwrap()
                .access,
            crate::SettingAccess::Writable
        );
        assert!(owner.authorize_validation(&first));
        assert!(owner.validation_authorized());
        assert_eq!(
            owner
                .settings_descriptors()
                .into_iter()
                .find(|item| item.id == SettingId::PwmTiltback)
                .unwrap()
                .access,
            crate::SettingAccess::Writable
        );

        owner.begin_attempt("B".into(), MonotonicTimestamp::new(3));
        let replacement = super::super::tests::connected_aero(&mut owner);
        assert!(!owner.validation_authorized());
        assert!(!owner.authorize_validation(&first));
        assert!(owner.authorize_validation(&replacement));
    }
}
