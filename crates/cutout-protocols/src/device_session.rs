//! Protocol-selected mobile session; clients never construct a model-specific reactor.

use cutout_core::{
    ControlRefusal, ControlRefusalReason, DeviceCommand, ModelRegistryEntry, ParserDiagnosticsDto,
    ProtocolFamily, RideOperatingStateDto, SafetyClass, SessionEventDto, SessionInputDto,
    SessionOutputDto, TelemetrySnapshotDto,
};

use crate::{
    ConcreteAeroBenignControlSession, ConcreteFalconBenignControlSession, ConcreteSessionErrorDto,
    ConcreteSessionStepResultDto, DeviceDetectionResolution, IdentityConfidence,
    ProtocolFamilyState, VescReadOnlySession,
};

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
                DeviceSessionEngine::Vesc(Box::default()),
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

    /// Drives a typed input through the selected existing protocol implementation.
    #[must_use]
    pub fn ingest_checked(&mut self, input: &SessionInputDto) -> ConcreteSessionStepResultDto {
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
                }
                .into();
                return ConcreteSessionStepResultDto {
                    outputs: vec![SessionOutputDto::Event(SessionEventDto::ControlRefusal(
                        refusal,
                    ))],
                    error: Some(ConcreteSessionErrorDto::CommandRefused { refusal }),
                };
            }
        }
        match &mut self.engine {
            DeviceSessionEngine::Veteran(session) => session.ingest_checked(input),
            DeviceSessionEngine::Begode(session) => session.ingest_checked(input),
            DeviceSessionEngine::Vesc(session) => session.ingest_checked(input),
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
    use cutout_core::CutoutSessionState;

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
    }
}
