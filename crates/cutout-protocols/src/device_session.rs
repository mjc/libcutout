//! Protocol-selected mobile session; clients never construct a model-specific reactor.

use cutout_core::{
    Capabilities, ControlRefusal, ControlRefusalReason, DeviceCommand, DeviceEvent, HostSession,
    ModelRegistryEntry, ParserDiagnosticsDto, ProtocolFamily, ProtocolSession,
    RideOperatingStateDto, SafetyClass, SessionInputDto, SessionOutput, TelemetrySnapshotDto,
};

use crate::{
    ConcreteAeroBenignControlSession, ConcreteFalconBenignControlSession,
    ConcreteSessionStepResultDto, DeviceDetectionResolution, IdentityConfidence,
    ProtocolFamilyState, VescReadOnlySession,
};

/// Original protocol outputs retained until shared state owners consume their evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSessionStep {
    /// Ordered domain outputs, including measured settings readbacks.
    pub outputs: Vec<SessionOutput>,
    /// First refusal from the protocol or unsupported command capability.
    pub error: Option<ControlRefusal>,
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
        Self { outputs, error }
    }
}

/// Native presentation category, independent of controller protocol or model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VehicleKind {
    /// Protocol evidence does not establish the vehicle's physical kind.
    Unknown,
    /// Electric unicycle.
    ElectricUnicycle,
    /// Self-balancing board.
    Board,
    /// Scooter.
    Scooter,
    /// Bicycle.
    Bike,
}

/// Verified protocol identity with independently optional model and vehicle kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceSessionIdentity {
    /// Protocol established by validated binary evidence.
    pub protocol: ProtocolFamily,
    /// Physical vehicle category established independently from generic UART.
    pub vehicle_kind: VehicleKind,
    /// Exact registry identity, never a closest-family substitute.
    pub model: Option<&'static ModelRegistryEntry>,
}

/// One protocol-owned mobile device session.
#[derive(Debug)]
pub struct DeviceSession {
    identity: DeviceSessionIdentity,
    engine: DeviceSessionEngine,
}

#[derive(Debug)]
enum DeviceSessionEngine {
    Veteran(Box<ConcreteAeroBenignControlSession>),
    Begode(Box<ConcreteFalconBenignControlSession>),
    Vesc(Box<VescReadOnlySession>),
}

impl DeviceSession {
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
                DeviceSessionEngine::Vesc(Box::new(vesc_board_profile.map_or_else(
                    VescReadOnlySession::default,
                    VescReadOnlySession::with_board_profile,
                ))),
            ),
        };
        let model = resolution.staged.model.filter(|model| {
            resolution.staged.confidence == IdentityConfidence::Model
                && model.protocol_family == protocol
        });
        Some(Self {
            identity: DeviceSessionIdentity {
                protocol,
                vehicle_kind,
                model,
            },
            engine,
        })
    }

    /// Returns protocol, model and kind without inventing missing identity.
    #[must_use]
    pub const fn identity(&self) -> DeviceSessionIdentity {
        self.identity
    }

    /// Selects semantic settings only for an exactly identified supported model.
    #[must_use]
    pub fn control_profile(&self) -> crate::DeviceControlProfile {
        match (&self.engine, self.identity.model) {
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
                };
            }
        }
        match &mut self.engine {
            DeviceSessionEngine::Veteran(session) => session.ingest_typed(input),
            DeviceSessionEngine::Begode(session) => session.ingest_typed(input),
            DeviceSessionEngine::Vesc(session) => {
                let mut step = session.ingest_typed(input);
                if let SessionInputDto::LinkUp { monotonic_ms, .. } = input {
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

    /// Arms the existing stationary policy only for an exactly identified model.
    pub fn arm_settings_writes(
        &mut self,
        state: RideOperatingStateDto,
        speed: Option<i32>,
        at_ms: u64,
    ) -> bool {
        if self.identity.model.is_none() {
            return false;
        }
        match &mut self.engine {
            DeviceSessionEngine::Veteran(session) => {
                session.arm_settings_writes(state, speed, at_ms)
            }
            DeviceSessionEngine::Begode(session) => {
                session.arm_settings_writes(state, speed, at_ms)
            }
            DeviceSessionEngine::Vesc(_) => false,
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
        assert_eq!(session.identity().vehicle_kind, VehicleKind::Unknown);
        assert_eq!(session.identity().model, None);
        assert!(session.control_profile().descriptors(true).is_empty());
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
            cutout_core::DeviceCommandDto::ResetTripMeter,
        ));
        assert!(result.error.is_some());
        assert!(
            result
                .outputs
                .iter()
                .all(|output| !matches!(output, SessionOutputDto::Transport(_)))
        );
        assert!(!session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 1));
        assert!(session.control_profile().descriptors(true).is_empty());
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
            session.identity().vehicle_kind,
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
                let SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    cutout_core::ReadOnlyResponse::Settings(readback),
                )) = output
                else {
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
