use cutout_core::{
    AeroAngleAdjustment, AeroPwmPercent, AeroSpeedSetting, DeviceCommand, Duration, GattChannel,
    GattFingerprint, HostSession, LightState, LinkInfo, ModelRegistryEntry, MonotonicTimestamp,
    PedalMode, RideOperatingState, SessionOutput, Speed, StationarySettingsPolicy, TransportAction,
    TransportWriteLimit, WriteMode, WritePayload,
};

use crate::{
    NOSFET_AERO_REGISTRY_ENTRY, NosfetAeroModel, ProtocolModelSpec, StationarySettingsWriteSession,
    SupportsSettingsWrites,
};

/// Typed settings readback held by the simulated NOSFET Aero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AeroSettingsReadback {
    /// Current TLT speed, when the simulator has a value.
    pub tiltback_speed: Option<AeroSpeedSetting>,

    /// Current PWT percentage, when the simulator has a value.
    pub pwm_percent: Option<AeroPwmPercent>,

    /// Current ALM speed, when the simulator has a value.
    pub alarm_speed: Option<AeroSpeedSetting>,

    /// Current ANG adjustment, when the simulator has a value.
    pub angle_adjustment: Option<AeroAngleAdjustment>,

    /// Current pedal mode, when the simulator has a value.
    pub pedal_mode: Option<PedalMode>,

    /// Current high-beam state, when the simulator has a value.
    pub high_beam: Option<LightState>,

    /// Current single-frame headlight state, when the simulator has a value.
    pub headlight: Option<LightState>,

    /// Number of accepted trip-meter reset writes.
    pub trip_meter_reset_count: u32,
}

impl AeroSettingsReadback {
    /// Creates a readback with no initial setting values.
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            tiltback_speed: None,
            pwm_percent: None,
            alarm_speed: None,
            angle_adjustment: None,
            pedal_mode: None,
            high_beam: None,
            headlight: None,
            trip_meter_reset_count: 0,
        }
    }

    /// Creates a useful stationary test fixture using typed default values.
    #[must_use]
    pub fn with_defaults() -> Self {
        let Some(tiltback_speed) = AeroSpeedSetting::new(20) else {
            return Self::unknown();
        };
        let Some(pwm_percent) = AeroPwmPercent::new(60) else {
            return Self::unknown();
        };
        let Some(alarm_speed) = AeroSpeedSetting::new(20) else {
            return Self::unknown();
        };
        let Some(angle_adjustment) = AeroAngleAdjustment::new(0) else {
            return Self::unknown();
        };
        Self {
            tiltback_speed: Some(tiltback_speed),
            pwm_percent: Some(pwm_percent),
            alarm_speed: Some(alarm_speed),
            angle_adjustment: Some(angle_adjustment),
            pedal_mode: Some(PedalMode::Medium),
            high_beam: Some(LightState::Off),
            headlight: Some(LightState::Off),
            trip_meter_reset_count: 0,
        }
    }
}

impl Default for AeroSettingsReadback {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// One transport write observed by the simulated device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AeroSimulatorWrite {
    /// GATT channel selected by the real session.
    pub channel: GattChannel,

    /// Exact bounded payload emitted by the real encoder.
    pub payload: WritePayload,

    /// Transport write mode selected by the real session.
    pub mode: WriteMode,
}

/// Deterministic, Rust-owned NOSFET Aero device simulator.
///
/// The simulator wraps the production stationary-settings session and records
/// its transport actions. It changes readback only after that session emits a
/// write, so tests exercise the same capability gate and encoder boundary as a
/// live BLE connection without claiming hardware effects.
#[derive(Clone, Debug)]
pub struct AeroSettingsSimulator {
    session: HostSession<StationarySettingsWriteSession<NosfetAeroModel, false>>,
    readback: AeroSettingsReadback,
    writes: Vec<AeroSimulatorWrite>,
}

impl Default for AeroSettingsSimulator {
    fn default() -> Self {
        Self::new(AeroSettingsReadback::default())
    }
}

impl AeroSettingsSimulator {
    /// Creates a simulator with an explicit typed settings snapshot.
    #[must_use]
    pub fn new(readback: AeroSettingsReadback) -> Self {
        let mut simulator = Self {
            session: HostSession::new(StationarySettingsWriteSession::default()),
            readback,
            writes: Vec::new(),
        };
        let _ = simulator.connect(MonotonicTimestamp::new(0));
        simulator
    }

    /// Returns the registry entry used for protocol/model identity.
    #[must_use]
    pub const fn registry_entry() -> &'static ModelRegistryEntry {
        &NOSFET_AERO_REGISTRY_ENTRY
    }

    /// Returns the GATT fingerprint used by the simulated model.
    #[must_use]
    pub fn gatt_fingerprints() -> &'static [GattFingerprint] {
        Self::registry_entry().gatt
    }

    /// Connects the simulated transport.
    #[must_use]
    pub fn connect(&mut self, monotonic_ms: MonotonicTimestamp) -> Vec<SessionOutput> {
        self.session.ingest_link_up(LinkInfo {
            monotonic_ms,
            max_write_len: Some(TransportWriteLimit::from_bytes(185)),
        });
        self.drain_outputs()
    }

    /// Issues a typed command with the same stationary/500-mm/s gate as Aero.
    #[must_use]
    pub fn issue(
        &mut self,
        command: DeviceCommand,
        state: RideOperatingState,
        speed: Option<Speed>,
        monotonic_ms: MonotonicTimestamp,
    ) -> Vec<SessionOutput> {
        self.session.session_mut().clear_arm();
        let policy = StationarySettingsPolicy {
            model: NosfetAeroModel::MODEL,
            arm_duration: Duration::from_milliseconds(5_000),
        };
        if let Some(arm) = policy.arm_with_speed(
            state,
            speed,
            <NosfetAeroModel as SupportsSettingsWrites>::MAX_SETTINGS_SPEED,
            monotonic_ms,
        ) {
            self.session.session_mut().arm(arm);
        }
        self.session.tick(monotonic_ms);
        let mut outputs = self.drain_outputs();
        self.session.issue_command(command);
        let command_outputs = self.drain_outputs();
        let command_wrote = command_outputs.iter().any(has_transport_write);
        outputs.extend(command_outputs);
        if command_wrote {
            self.apply_readback(command);
        }
        outputs
    }

    /// Advances the simulated session and returns newly emitted outputs.
    #[must_use]
    pub fn tick(&mut self, monotonic_ms: MonotonicTimestamp) -> Vec<SessionOutput> {
        self.session.tick(monotonic_ms);
        self.drain_outputs()
    }

    /// Returns the latest simulated typed settings readback.
    #[must_use]
    pub const fn readback(&self) -> AeroSettingsReadback {
        self.readback
    }

    /// Returns all transport writes observed since construction or the last clear.
    #[must_use]
    pub fn writes(&self) -> &[AeroSimulatorWrite] {
        &self.writes
    }

    /// Clears recorded transport writes without changing simulated settings.
    pub fn clear_writes(&mut self) {
        self.writes.clear();
    }

    fn drain_outputs(&mut self) -> Vec<SessionOutput> {
        let outputs = self.session.drain_outputs();
        self.writes
            .extend(outputs.iter().filter_map(simulator_write));
        outputs
    }

    fn apply_readback(&mut self, command: DeviceCommand) {
        match command {
            DeviceCommand::ResetTripMeter => {
                self.readback.trip_meter_reset_count =
                    self.readback.trip_meter_reset_count.saturating_add(1);
            }
            DeviceCommand::SetAeroTiltbackSpeed(value) => {
                self.readback.tiltback_speed = Some(value);
            }
            DeviceCommand::SetAeroPwmPercent(value) => {
                self.readback.pwm_percent = Some(value);
            }
            DeviceCommand::SetAeroAlarmSpeed(value) => {
                self.readback.alarm_speed = Some(value);
            }
            DeviceCommand::SetAeroAngleAdjustment(value) => {
                self.readback.angle_adjustment = Some(value);
            }
            DeviceCommand::SetAeroHighBeam(value) => {
                self.readback.high_beam = Some(value);
            }
            DeviceCommand::SetLights(value) => {
                self.readback.headlight = Some(value);
            }
            DeviceCommand::SetPedalMode(value) => {
                self.readback.pedal_mode = Some(value);
            }
            _ => {}
        }
    }
}

fn has_transport_write(output: &SessionOutput) -> bool {
    matches!(
        output,
        SessionOutput::Transport(TransportAction::Write { .. })
    )
}

fn simulator_write(output: &SessionOutput) -> Option<AeroSimulatorWrite> {
    let SessionOutput::Transport(TransportAction::Write {
        channel,
        bytes,
        mode,
    }) = output
    else {
        return None;
    };
    Some(AeroSimulatorWrite {
        channel: *channel,
        payload: bytes.clone(),
        mode: *mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AeroControlEncoder;
    use cutout_core::{ControlRefusalReason, DeviceEvent, RollAngle};

    fn speed(value: u8) -> AeroSpeedSetting {
        AeroSpeedSetting::new(value).expect("test speed is in range")
    }

    fn pwm(value: u8) -> AeroPwmPercent {
        AeroPwmPercent::new(value).expect("test pwm is in range")
    }

    fn angle(value: i8) -> AeroAngleAdjustment {
        AeroAngleAdjustment::new(value).expect("test angle is in range")
    }

    const fn parked() -> RideOperatingState {
        RideOperatingState::Parked
    }

    #[test]
    fn simulator_applies_aero_tune_writes_and_records_transport() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);
        let commands = [
            DeviceCommand::SetAeroTiltbackSpeed(speed(53)),
            DeviceCommand::SetAeroPwmPercent(pwm(64)),
            DeviceCommand::SetAeroAlarmSpeed(speed(56)),
            DeviceCommand::SetAeroAngleAdjustment(angle(-12)),
            DeviceCommand::SetPedalMode(PedalMode::Hard),
        ];

        for command in commands {
            let outputs = simulator.issue(command, parked(), None, now);
            assert!(outputs.iter().any(has_transport_write));
            let expected = AeroControlEncoder::encode(command).map(|encoded| encoded.payload);
            assert_eq!(
                simulator
                    .writes()
                    .last()
                    .map(|write| write.payload.as_slice()),
                expected.as_ref().map(|payload| payload.as_slice())
            );
        }

        let readback = simulator.readback();
        assert_eq!(readback.tiltback_speed, Some(speed(53)));
        assert_eq!(readback.pwm_percent, Some(pwm(64)));
        assert_eq!(readback.alarm_speed, Some(speed(56)));
        assert_eq!(readback.angle_adjustment, Some(angle(-12)));
        assert_eq!(readback.pedal_mode, Some(PedalMode::Hard));
        assert_eq!(simulator.writes().len(), commands.len());
    }

    #[test]
    fn simulator_accepts_500_mm_per_second_and_refuses_above_it() {
        let mut simulator = AeroSettingsSimulator::default();
        let accepted = simulator.issue(
            DeviceCommand::SetAeroPwmPercent(pwm(70)),
            RideOperatingState::Riding,
            Some(Speed::from_millimetres_per_second(500)),
            MonotonicTimestamp::new(10),
        );
        assert!(accepted.iter().any(has_transport_write));
        let write_count = simulator.writes().len();

        let refused = simulator.issue(
            DeviceCommand::SetAeroPwmPercent(pwm(71)),
            RideOperatingState::Riding,
            Some(Speed::from_millimetres_per_second(501)),
            MonotonicTimestamp::new(20),
        );
        assert_eq!(simulator.writes().len(), write_count);
        assert!(refused.iter().any(|output| {
            matches!(
                output,
                SessionOutput::Event(DeviceEvent::ControlRefusal(refusal))
                    if refusal.reason == ControlRefusalReason::MissingArm
            )
        }));
        assert_eq!(simulator.readback().pwm_percent, Some(pwm(70)));
    }

    #[test]
    fn simulator_keeps_unsupported_commands_write_free() {
        let mut simulator = AeroSettingsSimulator::default();
        let outputs = simulator.issue(
            DeviceCommand::SetRollAngle(RollAngle::High),
            parked(),
            None,
            MonotonicTimestamp::new(10),
        );
        assert!(simulator.writes().is_empty());
        assert!(outputs.iter().any(|output| {
            matches!(
                output,
                SessionOutput::Event(DeviceEvent::ControlRefusal(refusal))
                    if refusal.reason == ControlRefusalReason::UnsupportedCommand
            )
        }));
    }

    #[test]
    fn simulator_records_idempotent_writes_and_trip_resets() {
        let mut simulator = AeroSettingsSimulator::default();
        let command = DeviceCommand::SetAeroTiltbackSpeed(speed(42));
        let _ = simulator.issue(command, parked(), None, MonotonicTimestamp::new(10));
        let _ = simulator.issue(command, parked(), None, MonotonicTimestamp::new(11));
        let _ = simulator.issue(
            DeviceCommand::ResetTripMeter,
            parked(),
            None,
            MonotonicTimestamp::new(12),
        );

        assert_eq!(simulator.writes().len(), 3);
        assert_eq!(simulator.readback().tiltback_speed, Some(speed(42)));
        assert_eq!(simulator.readback().trip_meter_reset_count, 1);
    }

    #[test]
    fn simulator_records_both_aero_high_beam_frames() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);
        let _ = simulator.issue(
            DeviceCommand::SetAeroHighBeam(LightState::On),
            parked(),
            None,
            now,
        );
        let _ = simulator.tick(now);

        let expected = AeroControlEncoder::encode_settings_sequence(
            DeviceCommand::SetAeroHighBeam(LightState::On),
        )
        .expect("high beam has a paired frame sequence");
        assert_eq!(simulator.writes().len(), expected.steps.len());
        for (write, step) in simulator.writes().iter().zip(expected.steps) {
            assert_eq!(write.payload, step.payload);
            assert_eq!(write.mode, step.mode);
        }
        assert_eq!(simulator.readback().high_beam, Some(LightState::On));
    }

    #[test]
    fn simulator_tracks_single_frame_headlight_writes() {
        let mut simulator = AeroSettingsSimulator::default();
        let outputs = simulator.issue(
            DeviceCommand::SetLights(LightState::On),
            RideOperatingState::Riding,
            Some(Speed::from_millimetres_per_second(2_000)),
            MonotonicTimestamp::new(10),
        );

        assert!(outputs.iter().any(has_transport_write));
        assert_eq!(simulator.readback().headlight, Some(LightState::On));
        assert_eq!(simulator.writes().len(), 1);
        assert_eq!(simulator.writes()[0].payload.as_slice(), b"SetLightON");
    }
}
