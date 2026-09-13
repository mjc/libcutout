//! Connection, detector and protocol-session ownership shared by native clients.

use cutout_core::{
    ConnectionAttemptSnapshot, ConnectionAttemptToken, ConnectionReadiness, CutoutSessionState,
    DeviceSettingsState, MonotonicTimestamp, ParserDiagnosticsDto, ProtocolFamily, SessionInputDto,
    TelemetrySnapshotDto,
};

use crate::{
    ConcreteSessionStepResultDto, DeviceDetectionEvent, DeviceDetectionResolution,
    DeviceDetectionSession, DeviceSession, DeviceSessionIdentity,
};

/// Connection admission and exact identity from one owner observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceConnectionSnapshot {
    /// Current attempt and transport availability.
    pub connection: ConnectionAttemptSnapshot,
    /// Identity of the admitted decoder, if present.
    pub identity: Option<DeviceSessionIdentity>,
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
        owner.fail_capture();
        assert!(!owner.state.connection.is_current(&token));
        assert_eq!(
            owner.snapshot().connection.readiness,
            ConnectionReadiness::Failed
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
}

/// One decoded input paired with its producing connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceConnectionStep {
    /// Connection and identity that produced this result.
    pub session: DeviceConnectionSnapshot,
    /// Protocol outputs and typed error.
    pub result: ConcreteSessionStepResultDto,
    /// Latest normalized telemetry.
    pub telemetry: TelemetrySnapshotDto,
    /// Accumulated parser diagnostics.
    pub diagnostics: ParserDiagnosticsDto,
}

/// Authoritative device connection, backed by the existing state and protocol owners.
#[derive(Debug, Default)]
pub struct DeviceConnectionSession {
    /// Shared durable state, including ride, identity and settings slices.
    pub state: CutoutSessionState,
    /// Incremental detector retaining validated wire evidence.
    pub detector: DeviceDetectionSession,
    /// Protocol-selected decoder, never selected by native model dispatch.
    pub device: Option<DeviceSession>,
}

impl DeviceConnectionSession {
    /// Replaces all device-scoped state before native work for the new attempt.
    pub fn begin_attempt(&mut self, platform_identifier: String, at: MonotonicTimestamp) {
        self.state.reset_device_identity();
        self.state
            .select_discovered_platform(platform_identifier.clone());
        self.state.settings = DeviceSettingsState::default();
        self.detector = DeviceDetectionSession::default();
        self.device = None;
        self.state.connection.begin(platform_identifier, at);
    }

    /// Ends the attempt before native cancellation or explicit selection replacement.
    pub fn disconnect(&mut self) {
        self.state.connection.disconnect();
        self.state.settings.disconnect();
        self.device = None;
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
    pub fn fail_capture(&mut self) {
        self.state.connection.fail_capture();
        self.clear_failed_device();
    }

    fn clear_failed_device(&mut self) {
        if self.state.connection.snapshot().readiness == ConnectionReadiness::Failed {
            self.device = None;
            self.state.settings.disconnect();
        }
    }

    /// Accepts detector evidence only from the attempt owning the native callback.
    #[must_use]
    pub fn observe_for_attempt(
        &mut self,
        token: &ConnectionAttemptToken,
        event: DeviceDetectionEvent<'_>,
    ) -> Option<DeviceDetectionResolution> {
        self.state
            .connection
            .is_current(token)
            .then(|| self.detector.observe(&mut self.state, event))
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
        if let Some(device) = DeviceSession::from_detection(&resolution) {
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

    /// Decodes current verified input and pairs its identity before leaving this owner.
    #[must_use]
    pub fn ingest(
        &mut self,
        token: &ConnectionAttemptToken,
        input: &SessionInputDto,
    ) -> Option<DeviceConnectionStep> {
        if !self.state.connection.is_verified(token) {
            return None;
        }
        let device = self.device.as_mut()?;
        let result = device.ingest_checked(input);
        let telemetry = device.current_snapshot();
        let diagnostics = device.diagnostics();
        Some(DeviceConnectionStep {
            session: self.snapshot(),
            result,
            telemetry,
            diagnostics,
        })
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
