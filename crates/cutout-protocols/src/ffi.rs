use cutout_core::{
    ControlRefusalDto, HostSession, MonotonicTimestamp, ParserDiagnosticsDto, RideOperatingState,
    RideOperatingStateDto, SessionInput, SessionInputDto, SessionOutputDto, TelemetrySnapshotDto,
};

use crate::{
    BegodeFalconModel, NosfetAeroModel, ReadOnlySession, StationarySettingsWriteSession,
    SupportsBenignControls, SupportsSettingsWrites, VescBoardProfile, VescGenericModel,
    VescNotificationDecoder,
};

type AeroBenignControlHost = HostSession<StationarySettingsWriteSession<NosfetAeroModel, false>>;
type FalconBenignControlHost = HostSession<StationarySettingsWriteSession<BegodeFalconModel, true>>;
type VescReadOnlyHost = HostSession<ReadOnlySession<VescGenericModel, true>>;

fn output_is_telemetry(output: &cutout_core::SessionOutput) -> bool {
    matches!(
        output,
        cutout_core::SessionOutput::Event(cutout_core::DeviceEvent::Telemetry(_))
    )
}

/// Owned result of one concrete mobile session step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcreteSessionStepResultDto {
    /// Owned outputs emitted by the wrapped session.
    pub outputs: Vec<SessionOutputDto>,

    /// Stable error value surfaced from outputs or construction checks.
    pub error: Option<ConcreteSessionErrorDto>,
}

impl From<crate::DeviceSessionStep> for ConcreteSessionStepResultDto {
    fn from(value: crate::DeviceSessionStep) -> Self {
        Self {
            outputs: value.outputs.into_iter().map(Into::into).collect(),
            error: value
                .error
                .map(|refusal| ConcreteSessionErrorDto::CommandRefused {
                    refusal: refusal.into(),
                }),
        }
    }
}

/// Stable concrete mobile-wrapper error DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcreteSessionErrorDto {
    /// A command was refused by the read-only protocol shell.
    CommandRefused {
        /// Refusal details from the core domain model.
        refusal: ControlRefusalDto,
    },

    /// The requested Falcon construction profile is not supported yet.
    UnsupportedFalconProfile {
        /// Unsupported Falcon construction profile.
        profile: ConcreteFalconProfileDto,
    },
}

/// Concrete Falcon construction profile for mobile bindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcreteFalconProfileDto {
    /// Default known Falcon profile.
    Default,

    /// Deliberate unsupported sentinel used to keep binding errors typed.
    Unsupported,
}

/// Concrete NOSFET Aero telemetry wrapper with allow-listed headlight control.
#[derive(Clone, Debug)]
pub struct ConcreteAeroBenignControlSession {
    host: AeroBenignControlHost,
    last_telemetry_ms: Option<u64>,
}

impl ConcreteAeroBenignControlSession {
    /// Creates a telemetry session wrapper with allow-listed headlight control.
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: HostSession::new(
                StationarySettingsWriteSession::<NosfetAeroModel, false>::default(),
            ),
            last_telemetry_ms: None,
        }
    }

    /// Arms stationary settings writes from current, explicitly classified ride state.
    pub fn arm_settings_writes(
        &mut self,
        state: RideOperatingStateDto,
        speed_mm_per_second: Option<i32>,
        monotonic_ms: u64,
    ) -> bool {
        if matches!(
            state,
            RideOperatingStateDto::Standing | RideOperatingStateDto::Riding
        ) && self
            .last_telemetry_ms
            .is_some_and(|observed| monotonic_ms.saturating_sub(observed) > 5_000)
        {
            return false;
        }
        arm_stationary_settings::<NosfetAeroModel, false>(
            &mut self.host,
            state,
            speed_mm_per_second,
            monotonic_ms,
        )
    }

    /// Drives one owned DTO input through the wrapped protocol reactor.
    pub fn ingest(&mut self, input: &SessionInputDto) {
        ingest_timestamped_command(&mut self.host, input);
    }

    /// Sets the host timestamp before a command input that carries no core timestamp.
    pub fn set_monotonic(&mut self, monotonic_ms: u64) {
        self.host
            .session_mut()
            .set_monotonic(cutout_core::MonotonicTimestamp::new(monotonic_ms));
    }

    /// Drives one DTO input and returns owned outputs plus any stable error DTO.
    #[must_use]
    pub fn ingest_checked(&mut self, input: &SessionInputDto) -> ConcreteSessionStepResultDto {
        self.ingest_typed(input).into()
    }

    /// Drains original domain outputs before conversion for shared state owners.
    pub(crate) fn ingest_typed(&mut self, input: &SessionInputDto) -> crate::DeviceSessionStep {
        self.ingest(input);
        let result = crate::DeviceSessionStep::drain(
            &mut self.host,
            input,
            StationarySettingsWriteSession::<NosfetAeroModel, false>::capabilities(),
        );
        if matches!(
            input,
            SessionInputDto::LinkUp { .. } | SessionInputDto::LinkDown
        ) {
            self.last_telemetry_ms = None;
        }
        if let SessionInputDto::Notification { monotonic_ms, .. } = input {
            if result.outputs.iter().any(output_is_telemetry) {
                self.last_telemetry_ms = Some(monotonic_ms.milliseconds);
            }
        }
        result
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutputDto> {
        drain_host_outputs(&mut self.host)
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    #[must_use]
    pub fn current_snapshot(&self) -> TelemetrySnapshotDto {
        self.host.current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    #[must_use]
    pub fn diagnostics(&self) -> ParserDiagnosticsDto {
        self.host.diagnostics().into()
    }
}

impl Default for ConcreteAeroBenignControlSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Concrete Begode Falcon telemetry wrapper with allow-listed headlight control.
#[derive(Clone, Debug)]
pub struct ConcreteFalconBenignControlSession {
    host: FalconBenignControlHost,
    last_telemetry_ms: Option<u64>,
}

impl ConcreteFalconBenignControlSession {
    /// Creates a telemetry session wrapper with allow-listed headlight control.
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: HostSession::new(
                StationarySettingsWriteSession::<BegodeFalconModel, true>::default(),
            ),
            last_telemetry_ms: None,
        }
    }

    /// Arms stationary settings writes from current, explicitly classified ride state.
    pub fn arm_settings_writes(
        &mut self,
        state: RideOperatingStateDto,
        speed_mm_per_second: Option<i32>,
        monotonic_ms: u64,
    ) -> bool {
        if matches!(
            state,
            RideOperatingStateDto::Standing | RideOperatingStateDto::Riding
        ) && self
            .last_telemetry_ms
            .is_some_and(|observed| monotonic_ms.saturating_sub(observed) > 5_000)
        {
            return false;
        }
        arm_stationary_settings::<BegodeFalconModel, true>(
            &mut self.host,
            state,
            speed_mm_per_second,
            monotonic_ms,
        )
    }

    /// Creates a telemetry and headlight-control session for a selected Falcon profile.
    ///
    /// # Errors
    ///
    /// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
    /// selected profile is not supported by the concrete wrapper.
    pub fn try_new(profile: ConcreteFalconProfileDto) -> Result<Self, ConcreteSessionErrorDto> {
        match profile {
            ConcreteFalconProfileDto::Default => Ok(Self::new()),
            ConcreteFalconProfileDto::Unsupported => {
                Err(ConcreteSessionErrorDto::UnsupportedFalconProfile { profile })
            }
        }
    }

    /// Drives one owned DTO input through the wrapped protocol reactor.
    pub fn ingest(&mut self, input: &SessionInputDto) {
        ingest_timestamped_command(&mut self.host, input);
    }

    /// Sets the host timestamp before a command input that carries no core timestamp.
    pub fn set_monotonic(&mut self, monotonic_ms: u64) {
        self.host
            .session_mut()
            .set_monotonic(cutout_core::MonotonicTimestamp::new(monotonic_ms));
    }

    /// Drives one DTO input and returns owned outputs plus any stable error DTO.
    #[must_use]
    pub fn ingest_checked(&mut self, input: &SessionInputDto) -> ConcreteSessionStepResultDto {
        self.ingest_typed(input).into()
    }

    /// Drains original domain outputs before conversion for shared state owners.
    pub(crate) fn ingest_typed(&mut self, input: &SessionInputDto) -> crate::DeviceSessionStep {
        self.ingest(input);
        let result = crate::DeviceSessionStep::drain(
            &mut self.host,
            input,
            StationarySettingsWriteSession::<BegodeFalconModel, true>::capabilities(),
        );
        if matches!(
            input,
            SessionInputDto::LinkUp { .. } | SessionInputDto::LinkDown
        ) {
            self.last_telemetry_ms = None;
        }
        if let SessionInputDto::Notification { monotonic_ms, .. } = input {
            if result.outputs.iter().any(output_is_telemetry) {
                self.last_telemetry_ms = Some(monotonic_ms.milliseconds);
            }
        }
        result
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutputDto> {
        drain_host_outputs(&mut self.host)
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    #[must_use]
    pub fn current_snapshot(&self) -> TelemetrySnapshotDto {
        self.host.current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    #[must_use]
    pub fn diagnostics(&self) -> ParserDiagnosticsDto {
        self.host.diagnostics().into()
    }
}

impl Default for ConcreteFalconBenignControlSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Concrete mobile-binding read-only session wrapper for generic VESC telemetry.
#[derive(Debug)]
pub struct VescReadOnlySession {
    host: VescReadOnlyHost,
}

impl VescReadOnlySession {
    /// Creates a read-only session wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: HostSession::new(ReadOnlySession::<VescGenericModel, true>::default()),
        }
    }

    /// Creates a read-only session wrapper with known VESC board facts.
    #[must_use]
    pub fn with_board_profile(board_profile: VescBoardProfile) -> Self {
        Self {
            host: HostSession::new(ReadOnlySession::<VescGenericModel, true>::with_decoder(
                VescNotificationDecoder::with_board_profile(board_profile),
            )),
        }
    }

    /// Drives one owned DTO input through the wrapped protocol reactor.
    pub fn ingest(&mut self, input: &SessionInputDto) {
        ingest_timestamped_command(&mut self.host, input);
    }

    /// Drives one DTO input and returns owned outputs plus any stable error DTO.
    #[must_use]
    pub fn ingest_checked(&mut self, input: &SessionInputDto) -> ConcreteSessionStepResultDto {
        self.ingest_typed(input).into()
    }

    /// Drains original domain outputs before conversion for shared state owners.
    pub(crate) fn ingest_typed(&mut self, input: &SessionInputDto) -> crate::DeviceSessionStep {
        self.ingest(input);
        crate::DeviceSessionStep::drain(
            &mut self.host,
            input,
            ReadOnlySession::<VescGenericModel, true>::capabilities(),
        )
    }

    /// Drains owned output DTOs accumulated since the previous drain.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutputDto> {
        drain_host_outputs(&mut self.host)
    }

    /// Returns the latest telemetry snapshot as an owned DTO.
    #[must_use]
    pub fn current_snapshot(&self) -> TelemetrySnapshotDto {
        self.host.current_snapshot().into()
    }

    /// Returns accumulated parser diagnostics as an owned DTO.
    #[must_use]
    pub fn diagnostics(&self) -> ParserDiagnosticsDto {
        self.host.diagnostics().into()
    }
}

impl Default for VescReadOnlySession {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates the NOSFET Aero telemetry wrapper with allow-listed headlight control.
#[must_use]
pub fn new_nosfet_aero_benign_control_session() -> ConcreteAeroBenignControlSession {
    ConcreteAeroBenignControlSession {
        host: HostSession::new(StationarySettingsWriteSession::<NosfetAeroModel, false>::default()),
        last_telemetry_ms: None,
    }
}

/// Creates the Begode Falcon telemetry wrapper with allow-listed headlight control.
#[must_use]
pub fn new_begode_falcon_benign_control_session() -> ConcreteFalconBenignControlSession {
    ConcreteFalconBenignControlSession {
        host: HostSession::new(
            StationarySettingsWriteSession::<BegodeFalconModel, true>::default(),
        ),
        last_telemetry_ms: None,
    }
}

fn arm_stationary_settings<
    M: crate::ReadOnlyModelSpec + SupportsSettingsWrites + SupportsBenignControls,
    const ACCEPT_ANY_NOTIFICATION: bool,
>(
    host: &mut HostSession<StationarySettingsWriteSession<M, ACCEPT_ANY_NOTIFICATION>>,
    state: RideOperatingStateDto,
    speed_mm_per_second: Option<i32>,
    monotonic_ms: u64,
) -> bool {
    let now = MonotonicTimestamp::new(monotonic_ms);
    let snapshot = host.current_snapshot();
    let Some(speed) = host.session_mut().fresh_settings_speed(now) else {
        host.session_mut().clear_arm();
        return false;
    };
    if snapshot
        .charge_mode
        .is_some_and(|mode| mode.value.is_active())
        || snapshot.operating_state == Some(RideOperatingState::Charging)
        || speed_mm_per_second.is_some_and(|reported| reported != speed.as_millimetres_per_second())
    {
        host.session_mut().clear_arm();
        return false;
    }
    let state = match state {
        RideOperatingStateDto::Unknown => RideOperatingState::Unknown,
        RideOperatingStateDto::Parked => RideOperatingState::Parked,
        RideOperatingStateDto::Standing => RideOperatingState::Standing,
        RideOperatingStateDto::Riding => RideOperatingState::Riding,
        RideOperatingStateDto::Charging => RideOperatingState::Charging,
    };
    let Some(arm) = M::arm_settings_write(state, Some(speed), now) else {
        // Failed rearming also cancels work authorized by older ride evidence.
        host.session_mut().clear_arm();
        return false;
    };
    host.session_mut().arm(arm);
    true
}

/// Creates the Begode Falcon telemetry wrapper for a selected profile.
///
/// # Errors
///
/// Returns [`ConcreteSessionErrorDto::UnsupportedFalconProfile`] when the
/// selected profile is not supported by the concrete wrapper.
pub fn try_new_begode_falcon_benign_control_session(
    profile: ConcreteFalconProfileDto,
) -> Result<ConcreteFalconBenignControlSession, ConcreteSessionErrorDto> {
    ConcreteFalconBenignControlSession::try_new(profile)
}

/// Creates a generic VESC read-only session wrapper.
#[must_use]
pub fn new_vesc_read_only_session() -> VescReadOnlySession {
    VescReadOnlySession::new()
}

fn ingest_timestamped_command<S>(host: &mut HostSession<S>, input: &SessionInputDto)
where
    S: cutout_core::ProtocolSession,
{
    if let SessionInputDto::CommandAt { monotonic_ms, .. } = input {
        host.ingest(SessionInput::Tick {
            monotonic_ms: MonotonicTimestamp::new(monotonic_ms.milliseconds),
        });
    }
    host.ingest(input.as_session_input());
}

fn drain_host_outputs<S>(host: &mut HostSession<S>) -> Vec<SessionOutputDto>
where
    S: cutout_core::ProtocolSession,
{
    let mut summary = None;
    host.drain_outputs()
        .into_iter()
        .map(|output| {
            let mut output = SessionOutputDto::from(output);
            if let SessionOutputDto::ReadOnly(response) = &mut output
                && let cutout_core::ReadOnlyOutputPayload::Battery(readback) = &mut response.payload
                && let Some(page) = &mut readback.page
            {
                page.observation_summary = summary
                    .get_or_insert_with(|| {
                        host.session_state().telemetry().bms.observation_summary()
                    })
                    .clone();
            }
            output
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use cutout_core::{
        CommandKindDto, ControlRefusalDto, ControlRefusalReasonDto, DeviceCommandDto, LinkInfo,
        MonotonicMillisDto, MonotonicTimestamp, ParserDiagnosticCountDto, RideOperatingStateDto,
        SafetyClassDto, SessionEventDto, SessionInputDto, SessionOutputDto, TransportActionDto,
        TransportWriteLimit, TransportWriteLimitDto,
    };

    use crate::{BEGODE_DATA_CHANNEL, VESC_NOTIFY_CHANNEL, VETERAN_DATA_CHANNEL};

    use super::{
        ConcreteFalconProfileDto, ConcreteSessionErrorDto,
        new_begode_falcon_benign_control_session, new_nosfet_aero_benign_control_session,
        new_vesc_read_only_session, try_new_begode_falcon_benign_control_session,
    };

    const fn ms(value: u64) -> MonotonicMillisDto {
        MonotonicMillisDto {
            milliseconds: value,
        }
    }

    #[test]
    fn drained_bms_pages_carry_the_retained_core_summary() {
        struct Pages;
        impl cutout_core::ProtocolSession for Pages {
            fn handle(
                &mut self,
                input: cutout_core::SessionInput<'_>,
                output: &mut Vec<cutout_core::SessionOutput>,
            ) {
                let cutout_core::SessionInput::Tick { monotonic_ms } = input else {
                    return;
                };
                let (selector, voltage) = if monotonic_ms.get() == 1 {
                    (6, 4_200)
                } else {
                    (2, 4_180)
                };
                let readback = crate::decode_veteran_bms_page(
                    cutout_core::ProtocolSelector::new(selector),
                    (0..15)
                        .map(|_| cutout_core::Voltage::from_millivolts(voltage))
                        .collect(),
                    cutout_core::BatteryInfo::default(),
                    cutout_core::VerificationStatus::Unverified,
                )
                .expect("typed cell page");
                output.push(cutout_core::SessionOutput::Event(
                    cutout_core::DeviceEvent::ReadOnlyResponse(
                        cutout_core::ReadOnlyResponse::Battery(readback),
                    ),
                ));
            }
        }
        let mut host = cutout_core::HostSession::new(Pages);
        host.tick(MonotonicTimestamp::new(1));
        let _ = super::drain_host_outputs(&mut host);
        host.tick(MonotonicTimestamp::new(2));
        let outputs = super::drain_host_outputs(&mut host);
        let SessionOutputDto::ReadOnly(response) = &outputs[0] else {
            panic!("battery output")
        };
        let cutout_core::ReadOnlyOutputPayload::Battery(readback) = &response.payload else {
            panic!("battery readback")
        };
        let page = readback.page.as_ref().expect("page");
        assert_eq!(page.cell_voltages.len(), 15);
        assert_eq!(page.observation_summary.observed_count, 30);
        assert_eq!(
            page.observation_summary.lowest_index,
            Some(cutout_core::BmsObservationIndex::new(15))
        );
        assert_eq!(
            page.observation_summary.highest_index,
            Some(cutout_core::BmsObservationIndex::new(45))
        );
        assert_eq!(
            page.observation_summary
                .voltage_spread
                .map(cutout_core::VoltageDelta::as_millivolts),
            Some(20)
        );
    }

    const fn write_len(value: u16) -> TransportWriteLimit {
        TransportWriteLimit::from_bytes(value)
    }

    const fn write_len_dto(value: u16) -> TransportWriteLimitDto {
        TransportWriteLimitDto { bytes: value }
    }

    #[test]
    fn concrete_aero_session_drives_link_up_and_drains_owned_outputs() {
        let mut session = new_nosfet_aero_benign_control_session();

        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });

        assert!(session.drain_outputs().iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel })
                if *channel == VETERAN_DATA_CHANNEL.as_bytes()
        )));
    }

    #[test]
    fn concrete_falcon_session_maps_command_dto_to_write_output() {
        let mut session = new_begode_falcon_benign_control_session();
        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });
        let _ = session.drain_outputs();

        session.ingest(&SessionInputDto::Command(DeviceCommandDto::RequestIdentity));

        assert!(session.drain_outputs().iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { channel, bytes, .. })
                if *channel == BEGODE_DATA_CHANNEL.as_bytes() && bytes == b"N"
        )));
    }

    #[test]
    fn concrete_aero_session_maps_set_lights_to_control_write() {
        let mut session = new_nosfet_aero_benign_control_session();

        let result = session.ingest_checked(&SessionInputDto::Command(
            DeviceCommandDto::SetLights(cutout_core::LightStateDto::On),
        ));

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { channel, bytes, .. })
                if *channel == VETERAN_DATA_CHANNEL.as_bytes()
                    && bytes == b"SetLightON"
        )));
    }

    #[test]
    fn failed_rearm_revokes_the_previous_stationary_write_authorization() {
        let mut session = new_nosfet_aero_benign_control_session();
        ingest_stationary_aero(&mut session, 10);
        assert!(session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 10));
        assert!(!session.arm_settings_writes(RideOperatingStateDto::Riding, Some(501), 11));

        let result =
            session.ingest_checked(&SessionInputDto::Command(DeviceCommandDto::ResetTripMeter));

        assert_eq!(
            result.error,
            Some(ConcreteSessionErrorDto::CommandRefused {
                refusal: ControlRefusalDto {
                    command: CommandKindDto::ResetTripMeter,
                    safety_class: SafetyClassDto::StationaryOnly,
                    reason: ControlRefusalReasonDto::MissingArm,
                }
            })
        );
        assert!(result.outputs.iter().all(|output| !matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { .. })
        )));
    }

    fn ingest_stationary_aero(session: &mut super::ConcreteAeroBenignControlSession, at: u64) {
        let _ = session.ingest_checked(&SessionInputDto::LinkUp {
            monotonic_ms: ms(at),
            max_write_len: Some(write_len_dto(185)),
        });
        let frame = hex_literal::hex!(
            "dc5a5c532a7c000000000000ab41001700000cff\
             000000000226021ca8f607801afa000080c80000\
             808080808080022880803080800e310e310e2f0e\
             2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
             310e2e9e05e3ad"
        );
        let _ = session.ingest_checked(&SessionInputDto::Notification {
            channel: VETERAN_DATA_CHANNEL.as_bytes(),
            bytes: frame.to_vec(),
            monotonic_ms: ms(at),
        });
    }

    #[test]
    fn concrete_settings_arm_requires_fresh_observed_speed() {
        let mut session = new_nosfet_aero_benign_control_session();
        assert!(!session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 10));
        ingest_stationary_aero(&mut session, 20);
        assert!(session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 20));
        let _ = session.ingest_checked(&SessionInputDto::Tick {
            monotonic_ms: ms(60_000),
        });
        assert!(!session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 60_000));
        let result =
            session.ingest_checked(&SessionInputDto::Command(DeviceCommandDto::ResetTripMeter));
        assert!(result.error.is_some());
        assert!(result.outputs.iter().all(|output| !matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { .. })
        )));
    }

    #[test]
    fn bms_refresh_does_not_renew_settings_speed_evidence() {
        let mut session = new_begode_falcon_benign_control_session();
        let _ = session.ingest_checked(&SessionInputDto::LinkUp {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len_dto(185)),
        });
        let mut ride = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
        ride[4..6].copy_from_slice(&[0, 0]);
        let _ = session.ingest_checked(&SessionInputDto::Notification {
            channel: BEGODE_DATA_CHANNEL.as_bytes(),
            bytes: ride.to_vec(),
            monotonic_ms: ms(20),
        });
        assert!(session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 20));
        let bms = hex_literal::hex!("55aa271000000320ff9c0019001a0190000001035a5a5a5a");
        let _ = session.ingest_checked(&SessionInputDto::Notification {
            channel: BEGODE_DATA_CHANNEL.as_bytes(),
            bytes: bms.to_vec(),
            monotonic_ms: ms(60_000),
        });
        assert!(!session.arm_settings_writes(RideOperatingStateDto::Parked, Some(0), 60_000));
    }

    #[test]
    fn concrete_falcon_session_maps_set_lights_to_control_write() {
        let mut session = new_begode_falcon_benign_control_session();

        let result = session.ingest_checked(&SessionInputDto::Command(
            DeviceCommandDto::SetLights(cutout_core::LightStateDto::Off),
        ));

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { bytes, .. })
                if bytes == b"E"
        )));
    }

    #[test]
    fn vesc_read_only_session_subscribes_on_link_up() {
        let mut session = new_vesc_read_only_session();

        let result = session.ingest_checked(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel })
                if *channel == VESC_NOTIFY_CHANNEL.as_bytes()
        )));
    }

    #[test]
    fn checked_ingest_surfaces_unsupported_command_as_error_dto() {
        let mut session = new_begode_falcon_benign_control_session();
        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });
        let _ = session.drain_outputs();

        let result = session.ingest_checked(&SessionInputDto::Command(DeviceCommandDto::SoundHorn));

        let expected_refusal = ControlRefusalDto {
            command: CommandKindDto::SoundHorn,
            safety_class: SafetyClassDto::BenignControl,
            reason: ControlRefusalReasonDto::UnsupportedCommand,
        };
        assert_eq!(
            result.error,
            Some(ConcreteSessionErrorDto::CommandRefused {
                refusal: expected_refusal
            })
        );
        assert!(result.outputs.iter().all(|output| !matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Write { .. })
        )));
    }

    #[test]
    fn falcon_profile_constructor_rejects_unsupported_profile_with_error_dto() {
        assert_eq!(
            try_new_begode_falcon_benign_control_session(ConcreteFalconProfileDto::Unsupported)
                .expect_err("unsupported profile should return typed error"),
            ConcreteSessionErrorDto::UnsupportedFalconProfile {
                profile: ConcreteFalconProfileDto::Unsupported
            }
        );
    }

    #[test]
    fn falcon_profile_constructor_accepts_default_profile() {
        let mut session =
            try_new_begode_falcon_benign_control_session(ConcreteFalconProfileDto::Default)
                .expect("default Falcon profile should construct");

        let result = session.ingest_checked(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });

        assert_eq!(result.error, None);
        assert!(result.outputs.iter().any(|output| matches!(
            output,
            SessionOutputDto::Transport(TransportActionDto::Subscribe { channel })
                if *channel == BEGODE_DATA_CHANNEL.as_bytes()
        )));
    }

    #[test]
    fn concrete_session_exposes_snapshot_and_diagnostics_dtos() {
        let mut session = new_begode_falcon_benign_control_session();
        let channel = BEGODE_DATA_CHANNEL.as_bytes();
        let mut malformed = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
        malformed[20] = 0;

        session.ingest(&SessionInputDto::LinkUp {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len_dto(185)),
        });
        let _ = session.drain_outputs();

        session.ingest(&SessionInputDto::Notification {
            channel,
            bytes: malformed.to_vec(),
            monotonic_ms: ms(42),
        });

        assert_eq!(session.current_snapshot().at_ms, None);
        assert_eq!(
            session.diagnostics().malformed_frames,
            ParserDiagnosticCountDto { count: 1 }
        );
        assert!(session.drain_outputs().iter().any(|output| {
            matches!(
                output,
                SessionOutputDto::Event(SessionEventDto::DiagnosticError(_))
            )
        }));
    }

    #[test]
    fn concrete_session_accepts_core_link_info_roundtrip_inputs() {
        let mut session = new_begode_falcon_benign_control_session();
        let link = LinkInfo {
            monotonic_ms: MonotonicTimestamp::new(7),
            max_write_len: Some(write_len(20)),
        };

        session.ingest(&SessionInputDto::from(cutout_core::SessionInput::LinkUp(
            link,
        )));

        assert!(!session.drain_outputs().is_empty());
    }
}
