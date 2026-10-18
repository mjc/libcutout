//! Protocol-selected mobile session; clients never construct a model-specific reactor.

use cutout_core::{
    Capabilities, ControlRefusal, ControlRefusalReason, DeviceCommand, DeviceEvent, HostSession,
    ModelRegistryEntry, MonotonicTimestamp, ParserDiagnosticsDto, ProtocolFamily, ProtocolSession,
    RideOperatingStateDto, SafetyClass, SessionInputDto, SessionOutput, TelemetrySnapshotDto,
};

use crate::vesc_settings_session::VescSettingsSession;
use crate::{
    ConcreteAeroBenignControlSession, ConcreteFalconBenignControlSession,
    ConcreteSessionStepResultDto, DeviceDetectionResolution, IdentityConfidence,
    ProtocolFamilyState,
};
pub use cutout_core::VehicleKind;
use cutout_core::{VehicleClassification, VehicleClassificationRequirement};

/// Original protocol outputs retained until shared state owners consume their evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSessionStep {
    /// Ordered domain outputs, including measured settings readbacks.
    pub outputs: Vec<SessionOutput>,
    /// First refusal from the protocol or unsupported command capability.
    pub error: Option<ControlRefusal>,
    /// Retained BMS observations after applying this step.
    pub bms_observation_summary: cutout_core::BmsObservationSummary,
    /// Retained BMS temperatures after applying this step.
    pub bms_temperature_summary: cutout_core::BmsTemperatureSummary,
    /// Retained BMS currents after applying this step.
    pub bms_current_summary: cutout_core::BmsCurrentSummary,
}

impl DeviceSessionStep {
    pub(crate) fn drain<S: ProtocolSession>(
        host: &mut HostSession<S>,
        input: &SessionInputDto,
        capabilities: Capabilities,
    ) -> Self {
        let outputs = host.drain_outputs();
        let error = outputs
            .iter()
            .find_map(|output| match output {
                SessionOutput::Event(DeviceEvent::ControlRefusal(refusal)) => Some(*refusal),
                _ => None,
            })
            .or_else(|| {
                let (SessionInputDto::Command(command)
                | SessionInputDto::CommandAt { command, .. }) = input
                else {
                    return None;
                };
                let command = DeviceCommand::from(*command);
                (!capabilities.supports_command_kind(command.kind())).then(|| ControlRefusal {
                    command: command.kind(),
                    safety_class: command.safety_class(),
                    reason: ControlRefusalReason::UnsupportedCommand,
                })
            });
        Self {
            outputs,
            error,
            bms_observation_summary: host.session_state().telemetry().bms.observation_summary(),
            bms_temperature_summary: host.session_state().telemetry().bms.temperature_summary(),
            bms_current_summary: host.session_state().telemetry().bms.current_summary(),
        }
    }
}

/// Verified protocol identity with independently optional model and vehicle kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceSessionIdentity {
    /// Protocol established by validated binary evidence.
    pub protocol: ProtocolFamily,
    /// Physical vehicle category established independently from generic UART.
    pub classification: VehicleClassification,
    /// Rust-owned admission to a physical vehicle presentation.
    pub classification_requirement: VehicleClassificationRequirement,
    /// Application discovery for VESC, independent from physical type and exact model.
    pub vesc_application: Option<crate::VescApplicationDiscovery>,
    /// Native query progress and failure evidence, independent from application identity.
    pub vesc_application_query: Option<crate::VescApplicationQueryState>,
    /// Exact registry identity, never a closest-family substitute.
    pub model: Option<&'static ModelRegistryEntry>,
}

impl DeviceSessionIdentity {
    /// Physical type, derived from the classification and its retained provenance.
    #[must_use]
    pub const fn vehicle_kind(self) -> VehicleKind {
        self.classification.kind()
    }
}

/// One protocol-owned mobile device session.
#[derive(Debug)]
pub struct DeviceSession {
    identity: DeviceSessionIdentity,
    engine: DeviceSessionEngine,
    user_classification: Option<VehicleClassification>,
}

#[derive(Debug)]
enum DeviceSessionEngine {
    Veteran(Box<ConcreteAeroBenignControlSession>),
    Begode(Box<ConcreteFalconBenignControlSession>),
    Vesc(Box<VescSettingsSession>),
}

impl DeviceSession {
    pub(crate) fn configure_user_drivetrain(&mut self, value: cutout_core::VescDrivetrain) {
        if let DeviceSessionEngine::Vesc(session) = &mut self.engine {
            session.configure_user_drivetrain(value);
        }
    }
    pub(crate) fn application_query_deadline(&self) -> Option<MonotonicTimestamp> {
        match &self.engine {
            DeviceSessionEngine::Vesc(session) => session.application_query_deadline(),
            _ => None,
        }
    }
    pub(crate) fn advance_application_query_deadlines(&mut self, at: MonotonicTimestamp) {
        if let DeviceSessionEngine::Vesc(session) = &mut self.engine {
            session.advance_application_query_deadlines(at);
        }
    }
    pub(crate) fn application_query_is_current(
        &self,
        id: cutout_core::ApplicationQueryId,
        at: MonotonicTimestamp,
    ) -> bool {
        match &self.engine {
            DeviceSessionEngine::Vesc(session) => session.application_query_is_current(id, at),
            _ => false,
        }
    }
    pub(crate) fn record_application_query_transport(
        &mut self,
        id: cutout_core::ApplicationQueryId,
        index: u16,
        count: u16,
        status: cutout_core::ApplicationQueryTransportStatus,
        at: MonotonicTimestamp,
    ) -> bool {
        match &mut self.engine {
            DeviceSessionEngine::Vesc(session) => {
                session.record_application_query_transport(id, index, count, status, at)
            }
            _ => false,
        }
    }

    pub(crate) fn take_failed_setting_operation(&mut self) -> Option<u64> {
        match &mut self.engine {
            DeviceSessionEngine::Vesc(session) => session.take_failed_setting_operation(),
            DeviceSessionEngine::Veteran(_) | DeviceSessionEngine::Begode(_) => None,
        }
    }
    pub(crate) fn record_setting_transport(
        &mut self,
        operation_id: u64,
        status: cutout_core::SettingTransportStatus,
        at: cutout_core::MonotonicTimestamp,
    ) {
        if let DeviceSessionEngine::Vesc(session) = &mut self.engine {
            session.record_setting_transport(operation_id, status, at);
        }
    }
    pub(crate) fn setting_read_transport_is_current(
        &self,
        operation_id: u64,
        at: cutout_core::MonotonicTimestamp,
    ) -> bool {
        match &self.engine {
            DeviceSessionEngine::Vesc(session) => {
                session.setting_read_transport_is_current(operation_id, at)
            }
            DeviceSessionEngine::Veteran(_) | DeviceSessionEngine::Begode(_) => false,
        }
    }
    pub(crate) fn record_setting_read_transport(
        &mut self,
        operation_id: u64,
        status: cutout_core::SettingTransportStatus,
        at: cutout_core::MonotonicTimestamp,
    ) -> bool {
        match &mut self.engine {
            DeviceSessionEngine::Vesc(session) => {
                session.record_setting_read_transport(operation_id, status, at)
            }
            DeviceSessionEngine::Veteran(_) | DeviceSessionEngine::Begode(_) => false,
        }
    }
    pub(crate) fn setting_mutation_submitted_at(
        &self,
        operation_id: u64,
    ) -> Option<cutout_core::MonotonicTimestamp> {
        match &self.engine {
            DeviceSessionEngine::Vesc(session) => {
                session.setting_mutation_submitted_at(operation_id)
            }
            DeviceSessionEngine::Veteran(_) | DeviceSessionEngine::Begode(_) => None,
        }
    }
    pub(crate) fn next_tick_at(&self) -> Option<cutout_core::MonotonicTimestamp> {
        match &self.engine {
            DeviceSessionEngine::Veteran(session) => session.next_tick_at(),
            DeviceSessionEngine::Begode(session) => session.next_tick_at(),
            DeviceSessionEngine::Vesc(session) => session.next_tick_at(),
        }
    }

    pub(crate) fn setting_authorization_expires_at(
        &self,
        authorization: crate::session::SettingWriteAuthorization,
    ) -> Option<cutout_core::MonotonicTimestamp> {
        match &self.engine {
            DeviceSessionEngine::Veteran(session) => {
                Some(session.setting_authorization_expires_at(authorization))
            }
            DeviceSessionEngine::Begode(session) => {
                Some(session.setting_authorization_expires_at(authorization))
            }
            DeviceSessionEngine::Vesc(session) => {
                session.setting_authorization_expires_at(authorization)
            }
        }
    }

    pub(crate) fn notification_buffer_state(&self) -> crate::NotificationBufferState {
        match &self.engine {
            DeviceSessionEngine::Veteran(session) => session.notification_buffer_state(),
            DeviceSessionEngine::Begode(_) | DeviceSessionEngine::Vesc(_) => {
                crate::NotificationBufferState::Unknown
            }
        }
    }

    /// Latest validated raw settings pages exposed without client protocol parsing.
    #[must_use]
    pub fn raw_settings_pages(&self) -> &[crate::RawSettingsPage] {
        match &self.engine {
            DeviceSessionEngine::Veteran(session) => session.raw_settings_pages(),
            DeviceSessionEngine::Begode(_) | DeviceSessionEngine::Vesc(_) => &[],
        }
    }

    /// Constructs only from validated protocol evidence retained by the detector.
    #[must_use]
    pub fn from_detection(resolution: &DeviceDetectionResolution) -> Option<Self> {
        Self::from_detection_with_vesc_profile(resolution, None)
    }

    pub(crate) fn from_detection_with_vesc_profile(
        resolution: &DeviceDetectionResolution,
        vesc_board_profile: Option<crate::VescBoardProfile>,
    ) -> Option<Self> {
        let (protocol, vehicle_kind, engine) = match resolution.protocol {
            ProtocolFamilyState::Unknown | ProtocolFamilyState::Conflict => return None,
            ProtocolFamilyState::VeteranLeaperkimNosfet => (
                ProtocolFamily::VeteranLeaperkimNosfet,
                VehicleKind::ElectricUnicycle,
                DeviceSessionEngine::Veteran(Box::default()),
            ),
            ProtocolFamilyState::BegodeGotway => (
                ProtocolFamily::BegodeGotway,
                VehicleKind::ElectricUnicycle,
                DeviceSessionEngine::Begode(Box::default()),
            ),
            ProtocolFamilyState::Vesc => (
                ProtocolFamily::Vesc,
                VehicleKind::Unknown,
                DeviceSessionEngine::Vesc(Box::new(VescSettingsSession::new(vesc_board_profile))),
            ),
        };
        let model = resolution.staged.model.filter(|model| {
            resolution.staged.confidence == IdentityConfidence::Model
                && model.protocol_family == protocol
        });
        Some(Self {
            identity: DeviceSessionIdentity {
                protocol,
                classification: VehicleClassification::from_protocol(vehicle_kind),
                classification_requirement: if vehicle_kind == VehicleKind::Unknown {
                    VehicleClassificationRequirement::Pending
                } else {
                    VehicleClassificationRequirement::Resolved
                },
                vesc_application: None,
                vesc_application_query: None,
                model,
            },
            engine,
            user_classification: None,
        })
    }

    /// Returns protocol, model and kind without inventing missing identity.
    #[must_use]
    pub fn identity(&self) -> DeviceSessionIdentity {
        let mut identity = self.identity;
        if let DeviceSessionEngine::Vesc(session) = &self.engine {
            let application = session.application_discovery();
            identity.vesc_application = Some(application);
            identity.vesc_application_query = Some(session.application_query_state());
            let (classification, requirement) = match application {
                crate::VescApplicationDiscovery::Pending => (
                    VehicleClassification::UNKNOWN,
                    VehicleClassificationRequirement::Pending,
                ),
                crate::VescApplicationDiscovery::Refloat => (
                    VehicleClassification::from_protocol(VehicleKind::Onewheel),
                    VehicleClassificationRequirement::Resolved,
                ),
                crate::VescApplicationDiscovery::Absent => match self.user_classification {
                    Some(classification) => {
                        (classification, VehicleClassificationRequirement::Resolved)
                    }
                    None => (
                        VehicleClassification::UNKNOWN,
                        VehicleClassificationRequirement::UserSelectionRequired,
                    ),
                },
            };
            identity.classification = classification;
            identity.classification_requirement = requirement;
        }
        identity
    }

    /// Attaches a saved user choice without changing protocol, model or command capabilities.
    pub(crate) fn configure_vehicle_kind(&mut self, kind: VehicleKind) -> bool {
        let Some(classification) = VehicleClassification::user_provided(kind) else {
            return false;
        };
        match &self.engine {
            DeviceSessionEngine::Vesc(_) => {
                self.user_classification = Some(classification);
                true
            }
            DeviceSessionEngine::Veteran(_) | DeviceSessionEngine::Begode(_) => false,
        }
    }

    /// Selects semantic settings only for an exactly identified supported model.
    #[must_use]
    pub fn control_profile(&self) -> crate::DeviceControlProfile {
        match (&self.engine, self.identity.model) {
            (DeviceSessionEngine::Vesc(session), _) => session.control_profile(),
            (DeviceSessionEngine::Veteran(_), Some(model))
                if model == &crate::NOSFET_AERO_REGISTRY_ENTRY =>
            {
                crate::aero_control_profile()
            }
            (DeviceSessionEngine::Begode(_), Some(model))
                if model == &crate::BEGODE_FALCON_REGISTRY_ENTRY =>
            {
                crate::falcon_control_profile()
            }
            _ => crate::DeviceControlProfile::default(),
        }
    }

    /// Drives a typed input through the selected existing protocol implementation.
    #[must_use]
    pub fn ingest_checked(&mut self, input: &SessionInputDto) -> ConcreteSessionStepResultDto {
        self.ingest_typed(input).into()
    }

    /// Preserves measured domain outputs for the shared device state owner.
    #[must_use]
    pub fn ingest_typed(&mut self, input: &SessionInputDto) -> DeviceSessionStep {
        if self.identity.model.is_none()
            && let SessionInputDto::Command(command) | SessionInputDto::CommandAt { command, .. } =
                input
            && self.control_profile().descriptors(false).is_empty()
        {
            let command = DeviceCommand::from(*command);
            if command.safety_class() != SafetyClass::ReadOnly {
                let refusal = ControlRefusal {
                    command: command.kind(),
                    safety_class: command.safety_class(),
                    reason: ControlRefusalReason::UnsupportedCommand,
                };
                return DeviceSessionStep {
                    outputs: vec![SessionOutput::Event(DeviceEvent::ControlRefusal(refusal))],
                    error: Some(refusal),
                    bms_observation_summary: cutout_core::BmsObservationSummary::default(),
                    bms_temperature_summary: cutout_core::BmsTemperatureSummary::default(),
                    bms_current_summary: cutout_core::BmsCurrentSummary::default(),
                };
            }
        }
        match &mut self.engine {
            DeviceSessionEngine::Veteran(session) => session.ingest_typed(input),
            DeviceSessionEngine::Begode(session) => session.ingest_typed(input),
            DeviceSessionEngine::Vesc(session) => {
                let mut step = session.ingest_typed(input);
                if let SessionInputDto::LinkUp { monotonic_ms, .. } = input {
                    let firmware = session.ingest_typed(&SessionInputDto::CommandAt {
                        command: cutout_core::DeviceCommandDto::RequestFirmwareInfo,
                        monotonic_ms: *monotonic_ms,
                    });
                    step.outputs.extend(firmware.outputs);
                    step.error = step.error.or(firmware.error);
                    let startup = session.ingest_typed(&SessionInputDto::CommandAt {
                        command: cutout_core::DeviceCommandDto::RequestTelemetry,
                        monotonic_ms: *monotonic_ms,
                    });
                    step.outputs.extend(startup.outputs);
                    step.error = step.error.or(startup.error);
                }
                step
            }
        }
    }

    /// Projects normalized telemetry without exposing the protocol implementation.
    #[must_use]
    pub fn current_snapshot(&self) -> TelemetrySnapshotDto {
        match &self.engine {
            DeviceSessionEngine::Veteran(session) => session.current_snapshot(),
            DeviceSessionEngine::Begode(session) => session.current_snapshot(),
            DeviceSessionEngine::Vesc(session) => session.current_snapshot(),
        }
    }

    /// Returns parser diagnostics for the active protocol.
    #[must_use]
    pub fn diagnostics(&self) -> ParserDiagnosticsDto {
        match &self.engine {
            DeviceSessionEngine::Veteran(session) => session.diagnostics(),
            DeviceSessionEngine::Begode(session) => session.diagnostics(),
            DeviceSessionEngine::Vesc(session) => session.diagnostics(),
        }
    }

    /// Whether a full-configuration operation already owns the device's write slot.
    pub(crate) fn settings_busy(&self) -> bool {
        match &self.engine {
            DeviceSessionEngine::Vesc(session) => session.settings_busy(),
            DeviceSessionEngine::Veteran(_) | DeviceSessionEngine::Begode(_) => false,
        }
    }

    pub(crate) fn settings_read_requires_reconnect(
        &self,
        at: cutout_core::MonotonicTimestamp,
    ) -> bool {
        match &self.engine {
            DeviceSessionEngine::Vesc(session) => session.settings_read_requires_reconnect(at),
            DeviceSessionEngine::Veteran(_) | DeviceSessionEngine::Begode(_) => false,
        }
    }

    /// Associates delayed settings stages with their original transport operation.
    pub(crate) fn bind_setting_operation(
        &mut self,
        id: cutout_core::TransportOperationId,
    ) -> (Option<crate::session::SettingWriteAuthorization>, usize) {
        match &mut self.engine {
            DeviceSessionEngine::Veteran(session) => session.bind_setting_operation(id),
            DeviceSessionEngine::Begode(session) => session.bind_setting_operation(id),
            DeviceSessionEngine::Vesc(session) => session.bind_setting_operation(id),
        }
    }

    pub(crate) fn setting_write_is_current(
        &self,
        authorization: crate::session::SettingWriteAuthorization,
        at: cutout_core::MonotonicTimestamp,
    ) -> bool {
        match &self.engine {
            DeviceSessionEngine::Veteran(session) => {
                session.setting_write_is_current(authorization, at)
            }
            DeviceSessionEngine::Begode(session) => {
                session.setting_write_is_current(authorization, at)
            }
            DeviceSessionEngine::Vesc(session) => {
                session.setting_write_is_current(authorization, at)
            }
        }
    }

    /// Arms the existing stationary policy only for an exactly identified model.
    pub fn arm_settings_writes(
        &mut self,
        state: RideOperatingStateDto,
        speed: Option<i32>,
        at_ms: u64,
    ) -> bool {
        if self.identity.model.is_none() && self.identity.protocol != ProtocolFamily::Vesc {
            return false;
        }
        match &mut self.engine {
            DeviceSessionEngine::Veteran(session) => {
                session.arm_settings_writes(state, speed, at_ms)
            }
            DeviceSessionEngine::Begode(session) => {
                session.arm_settings_writes(state, speed, at_ms)
            }
            DeviceSessionEngine::Vesc(session) => session.arm_settings_writes(state, speed, at_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use cutout_core::{CutoutSessionState, SessionOutputDto};

    use crate::{DeviceDetectionEvent, DeviceDetectionSession};

    use super::*;

    #[test]
    fn banners_and_advertisements_never_construct_a_live_session() {
        let mut state = CutoutSessionState::default();
        let mut detector = DeviceDetectionSession::default();
        let _ = detector.observe(
            &mut state,
            DeviceDetectionEvent::Advertisement {
                name: Some(b"Falcon"),
            },
        );
        let resolution = detector.observe(
            &mut state,
            DeviceDetectionEvent::Notification {
                bytes: b"NAME=Falcon",
            },
        );
        assert!(DeviceSession::from_detection(&resolution).is_none());
    }

    #[test]
    fn validated_vesc_is_readable_without_inventing_board_or_model() {
        let mut state = CutoutSessionState::default();
        let mut detector = DeviceDetectionSession::default();
        let frame = [
            2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104,
            0, 38, 208, 3,
        ];
        let resolution = detector.observe(
            &mut state,
            DeviceDetectionEvent::Notification { bytes: &frame },
        );
        let session =
            DeviceSession::from_detection(&resolution).expect("validated VESC is supported");
        assert_eq!(session.identity().protocol, ProtocolFamily::Vesc);
        assert_eq!(session.identity().vehicle_kind(), VehicleKind::Unknown);
        assert_eq!(session.identity().model, None);
        assert_eq!(session.control_profile().descriptors(true).len(), 0);
    }

    #[test]
    fn unknown_veteran_model_keeps_read_only_protocol_without_model_writes() {
        let mut state = CutoutSessionState::default();
        let mut detector = DeviceDetectionSession::default();
        let mut frame = [0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&60_000_u16.to_be_bytes());
        let resolution = detector.observe(
            &mut state,
            DeviceDetectionEvent::Notification { bytes: &frame },
        );
        let mut session =
            DeviceSession::from_detection(&resolution).expect("validated family remains readable");
        assert_eq!(session.identity().model, None);
        let result = session.ingest_checked(&SessionInputDto::Command(
            cutout_core::DeviceCommandDto::InvokeAction(cutout_core::DeviceActionRequestDto {
                id: cutout_core::DeviceActionIdDto::ResetTripMeter,
                step: cutout_core::DeviceActionStepDto::Invoke,
            }),
        ));
        assert!(result.error.is_some());
        assert!(
            result
                .outputs
                .iter()
                .all(|output| !matches!(output, SessionOutputDto::Transport(_)))
        );
        assert!(!session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 1));
        assert_eq!(session.control_profile().descriptors(true).len(), 0);
    }

    #[test]
    fn exact_veteran_model_identity_is_retained_by_generic_session() {
        let mut state = CutoutSessionState::default();
        let mut detector = DeviceDetectionSession::default();
        let mut frame = [0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        let resolution = detector.observe(
            &mut state,
            DeviceDetectionEvent::Notification { bytes: &frame },
        );
        let session = DeviceSession::from_detection(&resolution).expect("exact model is supported");
        assert_eq!(
            session.identity().model,
            Some(&crate::NOSFET_AERO_REGISTRY_ENTRY)
        );
        assert_eq!(
            session.identity().vehicle_kind(),
            VehicleKind::ElectricUnicycle
        );
        assert!(
            session
                .control_profile()
                .descriptors(false)
                .iter()
                .any(|descriptor| descriptor.id == cutout_core::SettingId::PwmTiltback)
        );
    }

    #[test]
    fn typed_session_retains_reported_settings_evidence_before_dto_conversion() {
        let mut state = CutoutSessionState::default();
        let mut detector = DeviceDetectionSession::default();
        let mut frame = vec![0_u8; 42];
        frame[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        frame[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        let resolution = detector.observe(
            &mut state,
            DeviceDetectionEvent::Notification { bytes: &frame },
        );
        let mut session = DeviceSession::from_detection(&resolution).unwrap();
        let _ = session.ingest_typed(&SessionInputDto::LinkUp {
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 0 },
            max_write_len: None,
        });
        let _ = session.ingest_typed(&SessionInputDto::Notification {
            channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: frame,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 1 },
        });
        let mut settings = vec![0x80; 58];
        settings[..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 54]);
        settings[28..30].copy_from_slice(&43_000_u16.to_be_bytes());
        settings[46] = 8;
        settings[53] = 20;
        let checksum = crc32fast::hash(&settings[..54]);
        settings[54..].copy_from_slice(&checksum.to_be_bytes());
        let step = session.ingest_typed(&SessionInputDto::Notification {
            channel: crate::VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: settings,
            monotonic_ms: cutout_core::MonotonicMillisDto { milliseconds: 2 },
        });
        let entry = step
            .outputs
            .iter()
            .find_map(|output| {
                let SessionOutput::Event(DeviceEvent::ReadOnlyResponse(response)) = output else {
                    return None;
                };
                let cutout_core::ReadOnlyResponse::Settings(readback) = response else {
                    return None;
                };
                readback
                    .entries()
                    .into_iter()
                    .flatten()
                    .find(|entry| entry.field.id == crate::AERO_FIELD_PWM_PERCENT)
            })
            .expect("original measured PWM setting is retained");
        assert_eq!(entry.field.value, 20);
        assert_eq!(entry.source, cutout_core::ValueSource::Reported);
        assert_eq!(entry.quality, cutout_core::ValueQuality::Known);
        assert_eq!(
            entry.verification,
            cutout_core::VerificationStatus::SourceVerified
        );
    }
}
