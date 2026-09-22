//! Connection, detector and protocol-session ownership shared by native clients.

mod actions;
pub use actions::{DeviceActionSubmissionError, DeviceActionsSnapshot};
mod settings;
pub use settings::{DeviceSettingRequestError, DeviceSettingsSnapshot};

use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, ConnectionReadiness, CutoutSessionState,
    DeviceEvent, DeviceSettingsState, GattFingerprint, MonotonicTimestamp, ParserDiagnosticsDto,
    ProtocolFamily, ReadOnlyResponse, SessionInputDto, SessionOutput, TelemetrySnapshotDto,
};

use crate::{
    DeviceDetectionEvent, DeviceDetectionResolution, DeviceDetectionSession, DeviceSession,
    DeviceSessionIdentity, DeviceSessionStep,
};

/// Connection admission and exact identity from one owner observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceConnectionSnapshot {
    /// Current attempt and transport availability.
    pub connection: ConnectionAttemptSnapshot,
    /// Identity of the admitted decoder, if present.
    pub identity: Option<DeviceSessionIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ValidationGrant {
    generation: u64,
}

impl ValidationGrant {
    fn for_token(token: &ConnectionAttemptToken) -> Self {
        Self {
            generation: token.generation(),
        }
    }

    fn matches(self, token: &ConnectionAttemptToken) -> bool {
        self.generation == token.generation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VESC_REPLY: &[u8] = &[
        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
        38, 208, 3,
    ];

    fn begin(owner: &mut DeviceConnectionSession, name: &str) -> ConnectionAttemptToken {
        owner.begin_attempt(name.into(), MonotonicTimestamp::new(0));
        let token = owner.snapshot().connection.token.unwrap();
        owner.state.connection.connected(&token);
        token
    }

    #[test]
    fn replacement_owns_selection_and_rejects_previous_wire_evidence() {
        let mut owner = DeviceConnectionSession::default();
        let previous = begin(&mut owner, "A");
        let current = begin(&mut owner, "B");
        assert_eq!(
            owner
                .state
                .discovery()
                .selected_platform_identifier
                .as_deref(),
            Some("B")
        );
        assert!(
            owner
                .observe_for_attempt(
                    &previous,
                    DeviceDetectionEvent::Notification { bytes: VESC_REPLY }
                )
                .is_none()
        );
        owner.resolve(&current, false, MonotonicTimestamp::new(1));
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Pending
        );
        assert!(owner.snapshot().identity.is_none());
    }

    #[test]
    fn stale_attempt_cannot_mutate_detector_or_probe_state() {
        let mut owner = DeviceConnectionSession::default();
        let previous = begin(&mut owner, "A");
        let current = begin(&mut owner, "B");

        assert!(
            owner
                .begin_identification_probes(&previous, MonotonicTimestamp::new(1))
                .is_none()
        );
        assert!(
            owner
                .observe_detection(
                    &previous,
                    DeviceDetectionEvent::Notification { bytes: VESC_REPLY }
                )
                .is_none()
        );
        assert!(!owner.reset_detector(&previous));

        assert!(
            owner
                .begin_identification_probes(&current, MonotonicTimestamp::new(1))
                .is_some()
        );
    }

    #[test]
    fn standalone_probes_cannot_mutate_an_active_attempt() {
        let mut owner = DeviceConnectionSession::default();
        let _token = begin(&mut owner, "A");

        assert_eq!(
            owner.begin_identification_probes_unscoped(MonotonicTimestamp::new(1)),
            crate::IdentificationProbePlan::Unsupported
        );
    }

    #[test]
    fn terminal_record_only_keeps_evidence_but_cannot_construct_decoder() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        owner.transport_failed(&token);
        let evidence = owner.observe_for_attempt(
            &token,
            DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
        );
        assert!(evidence.is_some());
        owner.resolve(&token, true, MonotonicTimestamp::new(1));
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::RecordOnly
        );
        assert!(owner.snapshot().identity.is_none());
        owner.fail_capture(&token);
        assert!(!owner.state.connection.is_current(&token));
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Failed
        );
    }

    #[test]
    fn saved_vesc_geometry_is_attempt_scoped_and_does_not_invent_vehicle_kind() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        let profile = crate::VescBoardProfile::new(
            crate::MotorPolePairs::new(1),
            crate::GearRatioDenominator::new(1),
            cutout_core::Distance::from_millimetres(60),
        );
        assert!(owner.configure_vesc_board_profile(&token, profile));
        let read = |owner: &mut DeviceConnectionSession, token: &ConnectionAttemptToken| {
            let _ = owner.observe_for_attempt(
                token,
                DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
            );
            owner.resolve(token, false, MonotonicTimestamp::new(1));
            let _ = owner.ingest(
                token,
                &SessionInputDto::LinkUp {
                    monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                },
            );
            owner
                .ingest(
                    token,
                    &SessionInputDto::Notification {
                        channel: crate::VESC_NOTIFY_CHANNEL.as_bytes(),
                        bytes: vec![
                            2, 23, 50, 0, 2, 161, 138, 0, 0, 0, 0, 0, 4, 0, 0, 3, 221, 1, 119, 255,
                            255, 170, 43, 0, 20, 45, 58, 3,
                        ],
                        monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
                    },
                )
                .unwrap()
        };
        let first = read(&mut owner, &token);
        assert_eq!(first.telemetry.speed.unwrap().value, 989);
        assert_eq!(
            first.session.identity.unwrap().vehicle_kind,
            crate::VehicleKind::Unknown
        );
        assert!(
            !owner.configure_vesc_board_profile(&token, profile),
            "live decoder configuration is immutable"
        );
        let replacement = begin(&mut owner, "B");
        assert!(!owner.configure_vesc_board_profile(&token, profile));
        assert!(read(&mut owner, &replacement).telemetry.speed.is_none());
    }

    #[test]
    fn verified_vesc_starts_protocol_owned_polling_after_subscription() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "VESC");
        let _ = owner.observe_for_attempt(
            &token,
            DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
        );
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        let step = owner
            .ingest(
                &token,
                &SessionInputDto::LinkUp {
                    monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                    max_write_len: None,
                },
            )
            .unwrap();
        let subscribe = step
            .result
            .outputs
            .iter()
            .position(|output| {
                matches!(
                    output,
                    SessionOutput::Transport(cutout_core::TransportAction::Subscribe { .. })
                )
            })
            .unwrap();
        let write = step
            .result
            .outputs
            .iter()
            .position(|output| {
                matches!(
                    output,
                    SessionOutput::Transport(cutout_core::TransportAction::Write { .. })
                )
            })
            .expect("generic link-up starts the existing VESC polling owner");
        assert!(
            subscribe < write,
            "native must enable notifications before initial requests"
        );
        let tick = owner
            .ingest(
                &token,
                &SessionInputDto::Tick {
                    monotonic_ms: cutout_core::MonotonicMillisDto {
                        milliseconds: 2_001,
                    },
                },
            )
            .unwrap();
        assert!(tick.result.outputs.iter().any(|output| matches!(
            output,
            SessionOutput::Transport(cutout_core::TransportAction::Write { .. })
        )));
        owner.link_down(&token);
        assert!(
            owner
                .ingest(
                    &token,
                    &SessionInputDto::Tick {
                        monotonic_ms: cutout_core::MonotonicMillisDto {
                            milliseconds: 4_001
                        },
                    }
                )
                .is_none()
        );
    }

    #[test]
    fn verified_link_loss_retires_decoder_before_queued_input() {
        let mut owner = DeviceConnectionSession::default();
        let token = begin(&mut owner, "A");
        let _ = owner.observe_for_attempt(
            &token,
            DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
        );
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        assert!(owner.state.connection.is_verified(&token));
        assert!(owner.snapshot().identity.is_some());
        owner.link_down(&token);
        assert!(!owner.state.connection.is_verified(&token));
        assert!(owner.snapshot().identity.is_none());
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Failed
        );
    }

    #[test]
    fn verified_decoder_is_retired_when_later_evidence_conflicts() {
        let mut owner = DeviceConnectionSession::default();
        let token = connected_aero(&mut owner);
        assert!(owner.state.connection.is_verified(&token));

        let resolution = owner
            .observe_detection(
                &token,
                DeviceDetectionEvent::Notification { bytes: VESC_REPLY },
            )
            .expect("current attempt evidence is retained");

        assert_eq!(resolution.protocol, crate::ProtocolFamilyState::Conflict);
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Conflicted
        );
        assert!(owner.snapshot().identity.is_none());
        assert!(!owner.state.connection.is_current(&token));
        assert!(
            owner
                .ingest(
                    &token,
                    &SessionInputDto::Tick {
                        monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
                    }
                )
                .is_none()
        );
    }

    pub(super) fn connected_aero(owner: &mut DeviceConnectionSession) -> ConnectionAttemptToken {
        let token = begin(owner, "A");
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        let _ =
            owner.observe_for_attempt(&token, DeviceDetectionEvent::Notification { bytes: &frame });
        owner.resolve(&token, false, MonotonicTimestamp::new(1));
        let _ = owner.ingest(
            &token,
            &SessionInputDto::LinkUp {
                monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
                max_write_len: None,
            },
        );
        token
    }

    pub(super) fn aero_mode_telemetry(at: u64) -> SessionInputDto {
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        SessionInputDto::Notification {
            channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: frame,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: at },
        }
    }

    fn pwm_duty_readback(pwm: u8, at: u64) -> SessionInputDto {
        let mut frame = vec![0x80; 58];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 54]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        frame[46] = 8;
        frame[53] = pwm;
        let checksum = crc32fast::hash(&frame[..54]);
        frame[54..].copy_from_slice(&checksum.to_be_bytes());
        SessionInputDto::Notification {
            channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: frame,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: at },
        }
    }

    #[test]
    fn decoded_settings_update_only_the_current_connections_semantic_owner() {
        let mut owner = DeviceConnectionSession::default();
        let token = connected_aero(&mut owner);
        assert!(owner.ingest(&token, &pwm_duty_readback(80, 10)).is_some());
        let snapshots = owner.state.settings.snapshot(MonotonicTimestamp::new(15));
        let pwm = snapshots
            .iter()
            .find(|snapshot| snapshot.id == cutout_core::SettingId::PwmTiltback)
            .expect("decoded PWM reaches shared settings owner");
        assert_eq!(
            pwm.current.unwrap().value,
            cutout_core::DeviceSettingValue::Number(80)
        );
        assert_eq!(pwm.age, Some(cutout_core::Duration::from_milliseconds(5)));
        let _ = begin(&mut owner, "B");
        assert!(owner.ingest(&token, &pwm_duty_readback(30, 20)).is_none());
        assert!(
            owner
                .state
                .settings
                .snapshot(MonotonicTimestamp::new(20))
                .is_empty()
        );
    }

    #[test]
    fn explicit_unknown_settings_frame_clears_current_but_preserves_request() {
        let mut owner = DeviceConnectionSession::default();
        let token = connected_aero(&mut owner);
        let _ = owner.ingest(&token, &pwm_duty_readback(80, 10));
        assert!(
            owner
                .state
                .settings
                .snapshot(MonotonicTimestamp::new(10))
                .iter()
                .any(
                    |snapshot| snapshot.id == cutout_core::SettingId::PwmTiltback
                        && snapshot.current.is_some()
                )
        );
        owner.state.settings.submission(
            cutout_core::SettingId::PwmTiltback,
            cutout_core::DeviceSettingValue::Number(85),
            cutout_core::SettingSubmissionOutcome::Accepted,
            cutout_core::SettingCompletionStrategy::MatchingReadback,
            MonotonicTimestamp::new(11),
        );
        let _ = owner.ingest(&token, &pwm_duty_readback(128, 12));
        let snapshots = owner.state.settings.snapshot(MonotonicTimestamp::new(12));
        let pwm = snapshots
            .iter()
            .find(|snapshot| snapshot.id == cutout_core::SettingId::PwmTiltback)
            .unwrap();
        assert!(pwm.current.is_none());
        assert_eq!(
            pwm.requested,
            Some(cutout_core::DeviceSettingValue::Number(85))
        );
    }
}

/// One decoded input paired with its producing connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceConnectionStep {
    /// Connection and identity that produced this result.
    pub session: DeviceConnectionSnapshot,
    /// Protocol outputs and typed error.
    pub result: DeviceSessionStep,
    /// Latest normalized telemetry.
    pub telemetry: TelemetrySnapshotDto,
    /// Accumulated parser diagnostics.
    pub diagnostics: ParserDiagnosticsDto,
}

/// Authoritative device connection, backed by the existing state and protocol owners.
#[derive(Debug, Default)]
pub struct DeviceConnectionSession {
    /// Shared durable state, including ride, identity and settings slices.
    state: CutoutSessionState,
    /// Incremental detector retaining validated wire evidence.
    detector: DeviceDetectionSession,
    /// Protocol-selected decoder, never selected by native model dispatch.
    device: Option<DeviceSession>,
    last_input_at: MonotonicTimestamp,
    vesc_board_profile: Option<crate::VescBoardProfile>,
    validation_grant: Option<ValidationGrant>,
    setting_operations:
        std::collections::BTreeMap<cutout_core::SettingId, settings::SettingTransportOperation>,
}

impl DeviceConnectionSession {
    /// Borrows the shared session state without exposing connection internals.
    #[must_use]
    pub fn session_state(&self) -> &CutoutSessionState {
        &self.state
    }

    /// Mutably borrows shared session state for FFI-owned projections.
    pub fn session_state_mut(&mut self) -> &mut CutoutSessionState {
        &mut self.state
    }

    /// Borrows the retained detector evidence.
    #[must_use]
    pub fn detector(&self) -> &DeviceDetectionSession {
        &self.detector
    }

    /// Replaces detector evidence when starting a new identification pass.
    pub fn reset_detector(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.state.connection.is_current(token) {
            return false;
        }
        self.detector = DeviceDetectionSession::default();
        true
    }

    /// Resets standalone detector evidence before connection admission.
    pub fn reset_detector_unscoped(&mut self) -> bool {
        if self.state.connection.snapshot().token.is_some() {
            return false;
        }
        self.detector = DeviceDetectionSession::default();
        true
    }

    /// Starts protocol-owned identification probes against the current session state.
    pub fn begin_identification_probes(
        &mut self,
        token: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
    ) -> Option<crate::IdentificationProbePlan> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        Some(
            self.detector
                .begin_identification_probes(&mut self.state, at),
        )
    }

    /// Begins probes for standalone discovery before connection admission.
    pub fn begin_identification_probes_unscoped(
        &mut self,
        at: MonotonicTimestamp,
    ) -> crate::IdentificationProbePlan {
        if self.state.connection.snapshot().token.is_some() {
            return crate::IdentificationProbePlan::Unsupported;
        }
        self.detector
            .begin_identification_probes(&mut self.state, at)
    }

    /// Expires protocol-owned probes and returns the missing probes.
    pub fn expire_pending_probes(
        &mut self,
        token: &ConnectionAttemptToken,
        now: MonotonicTimestamp,
        timeout: cutout_core::Duration,
    ) -> Vec<cutout_core::PendingProbe> {
        if !self.state.connection.is_current(token) {
            return Vec::new();
        }
        self.detector
            .expire_pending_probes(&mut self.state, now, timeout)
            .into_iter()
            .collect()
    }

    /// Marks all protocol-owned probes missing.
    pub fn mark_pending_probes_missing(
        &mut self,
        token: &ConnectionAttemptToken,
    ) -> Vec<cutout_core::PendingProbe> {
        if !self.state.connection.is_current(token) {
            return Vec::new();
        }
        self.detector
            .mark_pending_probes_missing(&mut self.state)
            .into_iter()
            .collect()
    }

    /// Observes detector evidence without exposing the detector's mutable state.
    pub fn observe_detection(
        &mut self,
        token: &ConnectionAttemptToken,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        self.observe_for_attempt(token, event)
    }

    /// Records an identification probe write at its monotonic start time.
    pub fn observe_probe_write_at(
        &mut self,
        token: &ConnectionAttemptToken,
        probe: cutout_core::PendingProbe,
        at: MonotonicTimestamp,
    ) -> Option<DeviceDetectionResolution> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        Some(
            self.detector
                .observe_probe_write_at(&mut self.state, probe, at),
        )
    }

    /// Applies detector evidence for standalone discovery before an attempt exists.
    pub fn observe_detection_unscoped(
        &mut self,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        if self.state.connection.snapshot().token.is_some() {
            return None;
        }
        Some(self.detector.observe(&mut self.state, event))
    }

    /// Applies a standalone GATT observation before connection admission.
    pub fn observe_gatt_unscoped(
        &mut self,
        fingerprints: &[GattFingerprint],
    ) -> Option<DeviceDetectionResolution> {
        self.observe_detection_unscoped(DeviceDetectionEvent::Gatt { gatt: fingerprints })
    }

    /// Records a standalone probe write before connection admission.
    pub fn observe_probe_write_at_unscoped(
        &mut self,
        probe: cutout_core::PendingProbe,
        at: MonotonicTimestamp,
    ) -> Option<DeviceDetectionResolution> {
        if self.state.connection.snapshot().token.is_some() {
            return None;
        }
        Some(
            self.detector
                .observe_probe_write_at(&mut self.state, probe, at),
        )
    }

    /// Expires standalone probe evidence before connection admission.
    pub fn expire_pending_probes_unscoped(
        &mut self,
        now: MonotonicTimestamp,
        timeout: cutout_core::Duration,
    ) -> Vec<cutout_core::PendingProbe> {
        if self.state.connection.snapshot().token.is_some() {
            return Vec::new();
        }
        self.detector
            .expire_pending_probes(&mut self.state, now, timeout)
            .into_iter()
            .collect()
    }

    /// Marks standalone probe evidence missing before connection admission.
    pub fn mark_pending_probes_missing_unscoped(&mut self) -> Vec<cutout_core::PendingProbe> {
        if self.state.connection.snapshot().token.is_some() {
            return Vec::new();
        }
        self.detector
            .mark_pending_probes_missing(&mut self.state)
            .into_iter()
            .collect()
    }

    /// Replaces all device-scoped state before native work for the new attempt.
    pub fn begin_attempt(&mut self, platform_identifier: String, at: MonotonicTimestamp) {
        self.last_input_at = at;
        self.vesc_board_profile = None;
        self.validation_grant = None;
        self.state.reset_device_identity();
        self.state
            .select_discovered_platform(platform_identifier.clone());
        self.state.settings = DeviceSettingsState::default();
        self.setting_operations.clear();
        self.state.actions.disconnect();
        self.detector = DeviceDetectionSession::default();
        self.device = None;
        self.state.connection.begin(platform_identifier, at);
    }

    /// Supplies saved controller geometry for this pending attempt without asserting identity.
    pub fn configure_vesc_board_profile(
        &mut self,
        token: &ConnectionAttemptToken,
        profile: crate::VescBoardProfile,
    ) -> bool {
        if !self.state.connection.is_current(token)
            || self.state.connection.snapshot().readiness != ConnectionReadiness::Pending
        {
            return false;
        }
        self.vesc_board_profile = Some(profile);
        true
    }

    /// Selects usable capacity only for the protocol established in this attempt.
    #[must_use]
    pub fn default_charge_profile(&self) -> Option<cutout_core::ChargeProfile> {
        let device = self.device.as_ref()?;
        match device.identity().protocol {
            ProtocolFamily::Vesc => self
                .vesc_board_profile
                .and_then(|profile| profile.charge_profile),
            _ => device.control_profile().default_charge_profile(),
        }
    }

    /// Ends the attempt before native cancellation or explicit selection replacement.
    pub fn disconnect(&mut self) {
        self.state.connection.disconnect();
        self.state.settings.disconnect();
        self.setting_operations.clear();
        self.state.actions.disconnect();
        self.device = None;
        self.validation_grant = None;
    }

    /// Records link availability and invalidates verified-session work after link loss.
    pub fn link_down(&mut self, token: &ConnectionAttemptToken) {
        self.state.connection.link_down(token);
        self.clear_failed_device();
    }

    /// Preserves pending detection as record-only; established-session failures are terminal.
    pub fn transport_failed(&mut self, token: &ConnectionAttemptToken) {
        self.state.connection.transport_failed(token);
        self.clear_failed_device();
    }

    /// Capture storage failure invalidates every device-dependent operation.
    pub fn fail_capture(&mut self, token: &ConnectionAttemptToken) {
        self.state.connection.fail_capture(token);
        self.clear_failed_device();
    }

    /// Invalidates a verified decoder when a later notification proves it wrong.
    pub fn protocol_conflict(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.state.connection.conflict(token) {
            return false;
        }
        self.device = None;
        self.validation_grant = None;
        self.state.settings.disconnect();
        self.state.actions.disconnect();
        true
    }

    fn clear_failed_device(&mut self) {
        if self.state.connection.snapshot().readiness == ConnectionReadiness::Failed {
            self.device = None;
            self.state.settings.disconnect();
            self.state.actions.disconnect();
        }
    }

    /// Accepts detector evidence only from the attempt owning the native callback.
    #[must_use]
    pub fn observe_for_attempt(
        &mut self,
        token: &ConnectionAttemptToken,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        if !self.state.connection.is_current(token) {
            return None;
        }
        let resolution = self.detector.observe(&mut self.state, event);
        if resolution.protocol == crate::ProtocolFamilyState::Conflict
            && self.state.connection.snapshot().readiness == ConnectionReadiness::Verified
        {
            self.protocol_conflict(token);
        }
        Some(resolution)
    }

    /// Resolves readiness from retained wire evidence, respecting the whole-attempt deadline.
    pub fn resolve(
        &mut self,
        token: &ConnectionAttemptToken,
        identification_complete: bool,
        at: MonotonicTimestamp,
    ) {
        if !self.state.connection.is_current(token)
            || self.state.connection.snapshot().readiness != ConnectionReadiness::Pending
            || self.state.connection.expire(token, at)
        {
            return;
        }
        let resolution = self.detector.resolution(&self.state);
        if let Some(device) =
            DeviceSession::from_detection_with_vesc_profile(&resolution, self.vesc_board_profile)
        {
            let identity = device.identity();
            if (identification_complete
                || identity.model.is_some()
                || identity.protocol == ProtocolFamily::Vesc)
                && self.state.connection.finish_detection(token, true)
            {
                self.device = Some(device);
            }
        } else if identification_complete {
            self.state.connection.finish_detection(token, false);
        }
    }

    /// Decodes transport input and read-only commands for the current verified attempt.
    /// Device writes enter through validated semantic settings or action submission.
    #[must_use]
    pub fn ingest(
        &mut self,
        token: &ConnectionAttemptToken,
        input: &SessionInputDto,
    ) -> Option<DeviceConnectionStep> {
        if let SessionInputDto::Command(command) | SessionInputDto::CommandAt { command, .. } =
            input
            && cutout_core::DeviceCommand::from(*command).safety_class()
                != cutout_core::SafetyClass::ReadOnly
        {
            return None;
        }
        self.ingest_validated(token, input)
    }

    fn ingest_validated(
        &mut self,
        token: &ConnectionAttemptToken,
        input: &SessionInputDto,
    ) -> Option<DeviceConnectionStep> {
        if !self.state.connection.is_verified(token) {
            return None;
        }
        if let SessionInputDto::Notification { monotonic_ms, .. }
        | SessionInputDto::Tick { monotonic_ms }
        | SessionInputDto::CommandAt { monotonic_ms, .. }
        | SessionInputDto::LinkUp { monotonic_ms, .. } = input
        {
            self.last_input_at = self
                .last_input_at
                .max(MonotonicTimestamp::new(monotonic_ms.milliseconds));
            self.state.settings.tick(self.last_input_at);
        }
        let device = self.device.as_mut()?;
        if let SessionInputDto::CommandAt {
            command,
            monotonic_ms,
        } = input
        {
            let command = cutout_core::DeviceCommand::from(*command);
            let at = MonotonicTimestamp::new(monotonic_ms.milliseconds);
            if command.safety_class() == cutout_core::SafetyClass::StationaryOnly {
                let snapshot = device.current_snapshot();
                let operating_state = cutout_core::RideOperatingState::resolve(
                    snapshot.operating_state.map(Into::into),
                    snapshot.charge_mode.map(|mode| mode.value.into()),
                    snapshot
                        .speed
                        .map(|speed| cutout_core::Speed::from_millimetres_per_second(speed.value)),
                );
                let _ = device.arm_settings_writes(
                    operating_state.into(),
                    snapshot.speed.map(|speed| speed.value),
                    at.get(),
                );
            }
        }
        let result = device.ingest_typed(input);
        let telemetry = device.current_snapshot();
        let diagnostics = device.diagnostics();
        if let SessionInputDto::Notification { monotonic_ms, .. } = input {
            let received_at = MonotonicTimestamp::new(monotonic_ms.milliseconds);
            let profile = device.control_profile();
            for output in &result.outputs {
                let SessionOutput::Event(DeviceEvent::ReadOnlyResponse(response)) = output else {
                    continue;
                };
                let ReadOnlyResponse::Settings(readback) = response else {
                    continue;
                };
                profile.apply_action_readback(&mut self.state.actions, *readback, received_at);
                for observation in profile.normalize_readback(*readback) {
                    if let Some(value) = observation.value {
                        self.state
                            .settings
                            .observe_measured(observation.id, value, received_at);
                    } else {
                        self.state
                            .settings
                            .invalidate_readback(observation.id, received_at);
                    }
                }
            }
        }
        Some(DeviceConnectionStep {
            session: self.snapshot(),
            result,
            telemetry,
            diagnostics,
        })
    }

    /// Reads bounded raw-page evidence with its current connection and model identity.
    #[must_use]
    pub fn raw_settings_snapshot(&self) -> crate::RawSettingsSnapshot {
        crate::RawSettingsSnapshot {
            session: self.snapshot(),
            pages: self
                .device
                .as_ref()
                .map_or_else(Vec::new, |device| device.raw_settings_pages().to_vec()),
        }
    }

    /// Returns an immutable connection and identity snapshot.
    #[must_use]
    pub fn snapshot(&self) -> DeviceConnectionSnapshot {
        DeviceConnectionSnapshot {
            connection: self.state.connection.snapshot().clone(),
            identity: self.device.as_ref().map(DeviceSession::identity),
        }
    }
}
