//! Application-owned device session orchestration over the protocol state owner.

#[cfg(feature = "test-support")]
use cutout_core::SettingCompletionStrategy;
use cutout_core::{
    CameraMediaProvenance, CameraOnboardRecordingState, CameraPreviewState, CaptureAttemptSnapshot,
    CaptureCompletionDisposition, CaptureFinishToken, CaptureGeneration, CaptureOrigin,
    CaptureStage, ConnectionAttemptToken, CutoutSessionState, RideSessionDecision,
    RideSessionEffect, RideSessionInput, RideSessionLifecycle, RideSessionMarker,
};
use cutout_core::{
    ConnectionRetryDecision, DeviceActionId, DeviceSettingValue, MonotonicTimestamp, SettingId,
    SettingTransportStatus,
};
use cutout_protocols::{
    DeviceActionSubmissionError, DeviceActionsSnapshot, DeviceConnectionSession,
    DeviceConnectionSnapshot, DeviceConnectionStep, DeviceDetectionEvent,
    DeviceDetectionResolution, DeviceSettingRequestError, DeviceSettingsSnapshot,
    RawSettingsSnapshot, SettingDescriptor,
};

/// Atomic read of capture state and writer admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureLifecycleSnapshot {
    pub attempt: Option<CaptureAttemptSnapshot>,
    pub can_start: bool,
    pub can_pair: bool,
}

/// Result category for an event offered to a capture writer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureWriteDecision {
    Continue,
    Rejected,
    AdmissionLost,
    FailWriter,
    StaleFailure,
}

/// Writer health classification used to update the authoritative capture lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureWriterOutcome {
    Accepted,
    Rejected,
    AdmissionLost,
    Failed,
}

/// Recovery result with the unchanged or restored lifecycle and its requested platform effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideSessionRecovery {
    pub state: RideSessionLifecycle,
    pub effect: RideSessionEffect,
}

/// Session-level operations shared by native interfaces.
///
/// Protocol identification and telemetry remain owned by the protocol state machine;
/// this service coordinates that owner with capture, camera and ride lifecycles.
#[derive(Debug, Default)]
pub struct ApplicationDeviceSession {
    protocol: DeviceConnectionSession,
}

impl ApplicationDeviceSession {
    /// Read-only state for native DTO projections.
    #[must_use]
    pub fn session_state(&self) -> &CutoutSessionState {
        self.protocol.session_state()
    }

    /// One atomic connection and identity snapshot.
    #[must_use]
    pub fn snapshot(&self) -> DeviceConnectionSnapshot {
        self.protocol.snapshot()
    }

    /// Current protocol detector resolution without exposing mutable detector state.
    #[must_use]
    pub fn detector_resolution(&self) -> DeviceDetectionResolution {
        self.protocol
            .detector()
            .resolution(self.protocol.session_state())
    }

    /// Retained picker evidence for a platform identifier, including the active attempt's detector.
    #[must_use]
    pub fn discovery_resolution(
        &self,
        platform_identifier: &str,
    ) -> Option<DeviceDetectionResolution> {
        self.protocol.discovery_resolution(platform_identifier)
    }

    /// Next strict timeout deadline for any pending identity probe.
    #[must_use]
    pub fn next_probe_expiry(&self, timeout: cutout_core::Duration) -> Option<MonotonicTimestamp> {
        self.protocol
            .detector()
            .next_probe_expiry(self.protocol.session_state(), timeout)
    }

    /// Replaces detector evidence for one current connection attempt.
    pub fn reset_detector(&mut self, token: &ConnectionAttemptToken) -> bool {
        self.protocol.reset_detector(token)
    }
    /// Replaces standalone detector evidence only while no attempt is selected.
    pub fn reset_detector_unscoped(&mut self) -> bool {
        self.protocol.reset_detector_unscoped()
    }
    /// Starts identification probes for a current attempt.
    pub fn begin_identification_probes(
        &mut self,
        token: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
    ) -> Option<cutout_protocols::IdentificationProbePlan> {
        self.protocol.begin_identification_probes(token, at)
    }
    /// Starts standalone identification probes before connection admission.
    pub fn begin_identification_probes_unscoped(
        &mut self,
        at: MonotonicTimestamp,
    ) -> cutout_protocols::IdentificationProbePlan {
        self.protocol.begin_identification_probes_unscoped(at)
    }
    /// Observes detector evidence scoped to one attempt.
    pub fn observe_for_attempt(
        &mut self,
        token: &ConnectionAttemptToken,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        self.protocol.observe_for_attempt(token, event)
    }
    /// Observes detector evidence scoped to one current connection attempt.
    pub fn observe_detection(
        &mut self,
        token: &ConnectionAttemptToken,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        self.observe_for_attempt(token, event)
    }
    /// Observes standalone detector evidence when no connection attempt is selected.
    pub fn observe_detection_unscoped(
        &mut self,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        self.protocol.observe_detection_unscoped(event)
    }
    /// Records a standalone GATT inventory.
    pub fn observe_gatt_unscoped(
        &mut self,
        fingerprints: &[cutout_core::GattFingerprint],
    ) -> Option<DeviceDetectionResolution> {
        self.protocol.observe_gatt_unscoped(fingerprints)
    }
    /// Records a probe write for the current connection.
    pub fn observe_probe_write_at(
        &mut self,
        token: &ConnectionAttemptToken,
        probe: cutout_core::PendingProbe,
        at: MonotonicTimestamp,
    ) -> Option<DeviceDetectionResolution> {
        self.protocol.observe_probe_write_at(token, probe, at)
    }
    /// Records a standalone probe write.
    pub fn observe_probe_write_at_unscoped(
        &mut self,
        probe: cutout_core::PendingProbe,
        at: MonotonicTimestamp,
    ) -> Option<DeviceDetectionResolution> {
        self.protocol.observe_probe_write_at_unscoped(probe, at)
    }
    /// Expires probes for a current connection.
    pub fn expire_pending_probes(
        &mut self,
        token: &ConnectionAttemptToken,
        now: MonotonicTimestamp,
        timeout: cutout_core::Duration,
    ) -> Vec<cutout_core::PendingProbe> {
        self.protocol.expire_pending_probes(token, now, timeout)
    }
    /// Marks outstanding probes missing for a current connection.
    pub fn mark_pending_probes_missing(
        &mut self,
        token: &ConnectionAttemptToken,
    ) -> Vec<cutout_core::PendingProbe> {
        self.protocol.mark_pending_probes_missing(token)
    }
    /// Expires standalone probes.
    pub fn expire_pending_probes_unscoped(
        &mut self,
        now: MonotonicTimestamp,
        timeout: cutout_core::Duration,
    ) -> Vec<cutout_core::PendingProbe> {
        self.protocol.expire_pending_probes_unscoped(now, timeout)
    }
    /// Marks standalone probes missing.
    pub fn mark_pending_probes_missing_unscoped(&mut self) -> Vec<cutout_core::PendingProbe> {
        self.protocol.mark_pending_probes_missing_unscoped()
    }

    /// Replaces the selected connection attempt and its protocol-owned state.
    pub fn begin_attempt(&mut self, platform_identifier: String, at: MonotonicTimestamp) {
        self.protocol.begin_attempt(platform_identifier, at);
    }
    /// Admits a Rust-approved retry and returns its new attempt identity.
    pub fn admit_retry(
        &mut self,
        retry: cutout_core::ConnectionRetryToken,
        at: MonotonicTimestamp,
    ) -> Option<ConnectionAttemptToken> {
        self.protocol.admit_retry(retry, at)
    }
    /// Invalidates the current connection and decoder.
    pub fn disconnect(&mut self) {
        self.protocol.disconnect();
    }
    /// Records loss of the matching physical link.
    pub fn link_down(&mut self, token: &ConnectionAttemptToken) {
        self.protocol.link_down(token);
    }
    /// Records failure for the matching connection attempt.
    pub fn transport_failed(&mut self, token: &ConnectionAttemptToken) {
        self.protocol.transport_failed(token);
    }
    /// Resolves identification for the current attempt.
    pub fn resolve(
        &mut self,
        token: &ConnectionAttemptToken,
        identification_complete: bool,
        at: MonotonicTimestamp,
    ) {
        self.protocol.resolve(token, identification_complete, at);
    }
    /// Ingests one protocol input through the protocol owner.
    pub fn ingest(
        &mut self,
        token: &ConnectionAttemptToken,
        input: &cutout_core::SessionInputDto,
    ) -> Option<DeviceConnectionStep> {
        self.protocol.ingest(token, input)
    }
    /// Configures a VESC board profile for the matching pending attempt.
    pub fn configure_vesc_board_profile(
        &mut self,
        token: &ConnectionAttemptToken,
        profile: cutout_protocols::VescBoardProfile,
    ) -> bool {
        self.protocol.configure_vesc_board_profile(token, profile)
    }
    /// Returns the profile-selected default charge information.
    #[must_use]
    pub fn default_charge_profile(&self) -> Option<cutout_core::ChargeProfile> {
        self.protocol.default_charge_profile()
    }
    /// Current protocol validation authorization.
    #[must_use]
    pub fn validation_authorized(&self) -> bool {
        self.protocol.validation_authorized()
    }
    /// Grants protocol validation controls to a verified attempt.
    pub fn authorize_validation(&mut self, token: &ConnectionAttemptToken) -> bool {
        self.protocol.authorize_validation(token)
    }
    /// Revokes protocol validation controls for an attempt.
    pub fn revoke_validation(&mut self, token: &ConnectionAttemptToken) -> bool {
        self.protocol.revoke_validation(token)
    }
    /// Protocol setting descriptors paired with current validation authorization.
    #[must_use]
    pub fn settings_descriptors(&self) -> Vec<SettingDescriptor> {
        self.protocol.settings_descriptors()
    }
    /// Semantic setting snapshot.
    #[must_use]
    pub fn settings_snapshot(&self) -> DeviceSettingsSnapshot {
        self.protocol.settings_snapshot()
    }
    /// Semantic action snapshot.
    #[must_use]
    pub fn actions_snapshot(&self) -> DeviceActionsSnapshot {
        self.protocol.actions_snapshot()
    }
    /// Semantic action catalog.
    #[must_use]
    pub fn action_descriptors(&self) -> Vec<cutout_protocols::ActionDescriptor> {
        self.protocol.action_descriptors()
    }
    /// Submits one legal action step.
    ///
    /// # Errors
    /// Returns the protocol request error when the token or action is not admitted.
    pub fn submit_action(
        &mut self,
        token: &ConnectionAttemptToken,
        id: DeviceActionId,
        at: MonotonicTimestamp,
    ) -> Result<DeviceConnectionStep, DeviceActionSubmissionError> {
        self.protocol.submit_action(token, id, at)
    }
    /// Submits one validated semantic setting value.
    ///
    /// # Errors
    /// Returns the protocol request error when transport policy, token, or setting validation
    /// rejects the command.
    pub fn submit_setting(
        &mut self,
        token: &ConnectionAttemptToken,
        id: SettingId,
        value: DeviceSettingValue,
        at: MonotonicTimestamp,
    ) -> Result<DeviceConnectionStep, DeviceSettingRequestError> {
        self.protocol
            .session_state_mut()
            .settings
            .require_managed_transport();
        self.protocol.submit_setting(token, id, value, at)
    }
    /// Next setting or polling deadline for this verified connection.
    #[must_use]
    pub fn next_wakeup_at(
        &self,
        token: &ConnectionAttemptToken,
        can_poll: bool,
    ) -> Option<MonotonicTimestamp> {
        self.protocol.next_wakeup_at(token, can_poll)
    }
    /// Advances setting transport deadlines without polling.
    pub fn tick_setting_transport(
        &mut self,
        token: &ConnectionAttemptToken,
        at: MonotonicTimestamp,
    ) -> bool {
        self.protocol.tick_setting_transport(token, at)
    }
    /// Whether a queued setting write remains current at the native write boundary.
    #[must_use]
    pub fn setting_transport_is_current(
        &self,
        token: &ConnectionAttemptToken,
        request_id: u64,
        at: MonotonicTimestamp,
    ) -> bool {
        self.protocol
            .setting_transport_is_current(token, request_id, at)
    }
    /// Whether a queued settings read remains current at the native write boundary.
    #[must_use]
    pub fn setting_read_transport_is_current(
        &self,
        token: &ConnectionAttemptToken,
        request_id: u64,
        at: MonotonicTimestamp,
    ) -> bool {
        self.protocol
            .setting_read_transport_is_current(token, request_id, at)
    }
    /// Marks an admitted setting write's native transport state.
    pub fn mark_setting_transport(
        &mut self,
        token: &ConnectionAttemptToken,
        id: SettingId,
        request_id: u64,
        status: SettingTransportStatus,
        at: MonotonicTimestamp,
    ) -> bool {
        self.protocol
            .mark_setting_transport(token, id, request_id, status, at)
    }
    /// Marks an admitted settings read's native transport state.
    pub fn mark_setting_read_transport(
        &mut self,
        token: &ConnectionAttemptToken,
        request_id: u64,
        status: SettingTransportStatus,
        at: MonotonicTimestamp,
    ) -> bool {
        self.protocol
            .mark_setting_read_transport(token, request_id, status, at)
    }
    /// Raw settings pages retained for an explicit read-only projection.
    #[must_use]
    pub fn raw_settings_snapshot(&self) -> RawSettingsSnapshot {
        self.protocol.raw_settings_snapshot()
    }
    /// Latest protocol telemetry snapshot.
    #[must_use]
    pub fn telemetry_snapshot(&self) -> Option<cutout_core::TelemetrySnapshotDto> {
        self.protocol.telemetry_snapshot()
    }

    /// Requests retry policy from the shared connection lifecycle.
    pub fn request_retry(
        &mut self,
        token: &ConnectionAttemptToken,
        now: MonotonicTimestamp,
        jitter_permille: u16,
    ) -> ConnectionRetryDecision {
        self.protocol
            .session_state_mut()
            .connection
            .request_retry(token, now, jitter_permille)
    }
    /// Cancels a matching retry timer and its retry budget.
    pub fn cancel_retry(&mut self, token: cutout_core::ConnectionRetryToken) -> bool {
        self.protocol
            .session_state_mut()
            .connection
            .cancel_retry(token)
    }
    /// Accepts the native connected callback for the attempt.
    pub fn connected(&mut self, token: &ConnectionAttemptToken) {
        self.protocol
            .session_state_mut()
            .connection
            .connected(token);
    }
    /// Expires an attempt at the supplied monotonic time.
    pub fn expire_attempt(&mut self, token: &ConnectionAttemptToken, at: MonotonicTimestamp) {
        self.protocol
            .session_state_mut()
            .connection
            .expire(token, at);
    }
    /// Marks protocol detection complete or record-only for an attempt.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn finish_detection(&mut self, token: &ConnectionAttemptToken, supported: bool) -> bool {
        self.protocol
            .session_state_mut()
            .connection
            .finish_detection(token, supported)
    }
    /// Deadline for a pending retry token.
    #[must_use]
    pub fn retry_deadline(
        &self,
        token: cutout_core::ConnectionRetryToken,
    ) -> Option<MonotonicTimestamp> {
        self.protocol
            .session_state()
            .connection
            .retry_deadline(token)
    }
    /// Selects the session's connection intent.
    pub fn set_connection_intent(&mut self, intent: cutout_core::DeviceConnectionIntent) {
        self.protocol.session_state_mut().device_connection_intent = intent;
    }
    /// Records one setting submission in the shared observation state.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn record_setting_submission(
        &mut self,
        id: SettingId,
        value: DeviceSettingValue,
        outcome: cutout_core::SettingSubmissionOutcome,
        strategy: SettingCompletionStrategy,
        at: MonotonicTimestamp,
    ) {
        self.protocol
            .session_state_mut()
            .settings
            .submission(id, value, outcome, strategy, at);
    }
    /// Records one measured setting observation.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn observe_setting_measured(
        &mut self,
        id: SettingId,
        measured: cutout_core::Measured<DeviceSettingValue>,
        at: MonotonicTimestamp,
    ) {
        self.protocol
            .session_state_mut()
            .settings
            .observe_measured(id, measured, at);
    }

    /// Records one scan observation in the shared discovery inventory.
    pub fn observe_discovery(&mut self, observation: cutout_core::DiscoveryObservation) {
        self.protocol
            .session_state_mut()
            .observe_discovery(observation);
    }

    /// Selects a discovered platform identifier in the shared inventory.
    pub fn select_discovered_platform(&mut self, platform_identifier: String) {
        self.protocol
            .session_state_mut()
            .select_discovered_platform(platform_identifier);
    }

    /// Clears device evidence and detector state while retaining discovery inventory.
    pub fn reset_device_identity_and_detector_unscoped(&mut self) -> bool {
        if !self.protocol.reset_detector_unscoped() {
            return false;
        }
        self.protocol.session_state_mut().reset_device_identity();
        true
    }

    /// Clears device identity for one current attempt, retaining discovery inventory.
    pub fn reset_device_identity_and_detector(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.protocol.session_state().connection.is_current(token) {
            return false;
        }
        self.protocol.session_state_mut().reset_device_identity();
        self.protocol.reset_detector(token)
    }

    /// Clears standalone link-local probes while preserving identity and discovery facts.
    pub fn reset_link_probes_unscoped(&mut self) -> bool {
        if !self.protocol.reset_detector_unscoped() {
            return false;
        }
        self.protocol
            .session_state_mut()
            .identity_mut()
            .reset_link_probes();
        true
    }

    /// Clears link probes and detector state for the current attempt, in established order.
    pub fn reset_link_probes_and_detector(&mut self, token: &ConnectionAttemptToken) -> bool {
        if !self.protocol.session_state().connection.is_current(token) {
            return false;
        }
        self.protocol
            .session_state_mut()
            .identity_mut()
            .reset_link_probes();
        self.protocol.reset_detector(token)
    }

    /// Reads capture admission as one coherent value.
    #[must_use]
    pub fn capture_snapshot(&self) -> CaptureLifecycleSnapshot {
        let capture = &self.protocol.session_state().capture;
        CaptureLifecycleSnapshot {
            attempt: capture.snapshot(),
            can_start: capture.can_start(),
            can_pair: capture.can_pair(),
        }
    }

    /// Reserves a writer generation before native construction begins.
    pub fn begin_capture(&mut self, origin: CaptureOrigin) -> Option<CaptureGeneration> {
        self.protocol.session_state_mut().capture.begin(origin)
    }

    /// Admits recording after the native writer reports successful startup.
    pub fn capture_writer_started(&mut self, generation: CaptureGeneration) -> bool {
        self.protocol
            .session_state_mut()
            .capture
            .writer_started(generation)
    }

    /// Retains constructor failure for the matching writer generation.
    pub fn capture_writer_start_failed(&mut self, generation: CaptureGeneration) -> bool {
        self.protocol
            .session_state_mut()
            .capture
            .writer_start_failed(generation)
    }

    /// Claims one manual finish operation before native code starts an asynchronous flush.
    pub fn begin_capture_finish(
        &mut self,
        generation: CaptureGeneration,
    ) -> Option<CaptureFinishToken> {
        self.protocol
            .session_state_mut()
            .capture
            .begin_finish(generation)
    }

    /// Applies the durable flush receipt to its admitted finish operation.
    pub fn finish_capture_flush(&mut self, token: CaptureFinishToken, succeeded: bool) -> bool {
        self.protocol
            .session_state_mut()
            .capture
            .finish_flush(token, succeeded)
    }

    /// Marks failed evidence without allowing later counters to clear it.
    pub fn capture_writer_failed(&mut self, generation: CaptureGeneration) -> bool {
        self.protocol
            .session_state_mut()
            .capture
            .writer_failed(generation)
    }

    /// Detaches active capture ownership while the native writer completes.
    pub fn retire_capture_writer(&mut self, generation: CaptureGeneration) -> bool {
        self.protocol.session_state_mut().capture.retire(generation)
    }

    /// Consumes one generation-fenced terminal receipt.
    pub fn complete_capture_writer(
        &mut self,
        generation: CaptureGeneration,
        succeeded: bool,
    ) -> CaptureCompletionDisposition {
        self.protocol
            .session_state_mut()
            .capture
            .complete(generation, succeeded)
    }

    /// Applies authoritative writer health, preserving Saving until its finish token resolves.
    pub fn apply_capture_writer_health(
        &mut self,
        generation: Option<CaptureGeneration>,
        fatal: bool,
        healthy: CaptureWriteDecision,
    ) -> CaptureWriteDecision {
        let Some(generation) = generation else {
            return CaptureWriteDecision::StaleFailure;
        };
        let capture = &mut self.protocol.session_state_mut().capture;
        let Some(attempt) = capture.snapshot() else {
            return CaptureWriteDecision::StaleFailure;
        };
        if attempt.generation != generation {
            return CaptureWriteDecision::StaleFailure;
        }
        match attempt.stage {
            CaptureStage::Recording | CaptureStage::SaveFailed => {
                if fatal {
                    capture.writer_failed(generation);
                    CaptureWriteDecision::FailWriter
                } else {
                    healthy
                }
            }
            CaptureStage::Saving => {
                if fatal {
                    CaptureWriteDecision::FailWriter
                } else {
                    healthy
                }
            }
            CaptureStage::Starting
            | CaptureStage::Finalizing
            | CaptureStage::Saved
            | CaptureStage::Failed => CaptureWriteDecision::StaleFailure,
        }
    }

    /// Applies an individual event result to the writer lifecycle.
    pub fn apply_capture_write_outcome(
        &mut self,
        generation: Option<CaptureGeneration>,
        outcome: CaptureWriterOutcome,
    ) -> CaptureWriteDecision {
        match outcome {
            CaptureWriterOutcome::Accepted => CaptureWriteDecision::Continue,
            CaptureWriterOutcome::Rejected => CaptureWriteDecision::Rejected,
            CaptureWriterOutcome::AdmissionLost => CaptureWriteDecision::AdmissionLost,
            CaptureWriterOutcome::Failed => {
                if generation.is_some_and(|generation| self.capture_writer_failed(generation)) {
                    CaptureWriteDecision::FailWriter
                } else {
                    CaptureWriteDecision::StaleFailure
                }
            }
        }
    }

    /// Admits one camera-media association only while its capture generation accepts results.
    pub fn record_camera_media_provenance(
        &mut self,
        generation: CaptureGeneration,
        record: CameraMediaProvenance,
    ) -> bool {
        self.protocol
            .session_state_mut()
            .record_camera_media_provenance(generation, record)
    }

    /// Clears camera-media associations.
    pub fn clear_camera_media_provenance(&mut self) {
        self.protocol.session_state_mut().camera_provenance.clear();
    }

    /// Advances the camera scan generation.
    pub fn advance_camera_discovery_generation(&mut self) {
        self.protocol
            .session_state_mut()
            .camera_mut()
            .advance_discovery_generation();
    }
    /// Advances the camera preview generation.
    pub fn advance_camera_preview_generation(&mut self) {
        self.protocol
            .session_state_mut()
            .camera_mut()
            .advance_preview_generation();
    }
    /// Invalidates camera lifecycle tokens.
    pub fn invalidate_camera_lifecycle(&mut self) {
        self.protocol.session_state_mut().camera_mut().invalidate();
    }
    /// Records current preview state.
    pub fn observe_camera_preview(&mut self, state: CameraPreviewState) {
        self.protocol
            .session_state_mut()
            .camera_mut()
            .observe_preview(state);
    }
    /// Records current onboard recording state.
    pub fn observe_camera_onboard_recording(&mut self, state: CameraOnboardRecordingState) {
        self.protocol
            .session_state_mut()
            .camera_mut()
            .observe_onboard_recording(state);
    }

    /// Applies one ride-session event and commits its returned lifecycle state atomically.
    pub fn transition_ride_session(&mut self, input: RideSessionInput) -> RideSessionDecision {
        let decision = self.protocol.session_state().ride_session.transition(input);
        self.protocol.session_state_mut().ride_session = decision.state().clone();
        decision
    }

    /// Recovers a persisted ride marker without replacing a live identity or pending cleanup.
    pub fn recover_ride_session_marker(
        &mut self,
        marker: RideSessionMarker,
        restored_platform_identifier: Option<&str>,
    ) -> RideSessionRecovery {
        let current = &self.protocol.session_state().ride_session;
        if current.marker().is_some() {
            return RideSessionRecovery {
                state: current.clone(),
                effect: RideSessionEffect::None,
            };
        }
        let decision = RideSessionLifecycle::recover(
            marker,
            restored_platform_identifier,
            current.app_presence(),
        );
        let (state, effect) = decision.into_parts();
        self.protocol.session_state_mut().ride_session = state.clone();
        RideSessionRecovery { state, effect }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_health_does_not_bypass_an_admitted_saving_operation() {
        let mut session = ApplicationDeviceSession::default();
        let generation = session
            .begin_capture(CaptureOrigin::Manual)
            .expect("slot available");
        assert!(session.capture_writer_started(generation));
        let finish = session
            .begin_capture_finish(generation)
            .expect("recording can finish");
        assert_eq!(
            session
                .capture_snapshot()
                .attempt
                .map(|attempt| attempt.stage),
            Some(CaptureStage::Saving)
        );
        assert_eq!(
            session.apply_capture_writer_health(
                Some(generation),
                true,
                CaptureWriteDecision::Continue
            ),
            CaptureWriteDecision::FailWriter
        );
        assert_eq!(
            session
                .capture_snapshot()
                .attempt
                .map(|attempt| attempt.stage),
            Some(CaptureStage::Saving)
        );
        assert!(!session.finish_capture_flush(finish, false));
        assert_eq!(
            session
                .capture_snapshot()
                .attempt
                .map(|attempt| attempt.stage),
            Some(CaptureStage::SaveFailed)
        );
    }
}
