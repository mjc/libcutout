use cutout_core::{
    AeroAngleAdjustment, AeroBeeperVolume, AeroBrakeOverpressureAlarm, AeroDisplayBacklight,
    AeroDynamicAssist, AeroGyroCalibrationState, AeroHighSpeedMode, AeroLateralTiltLimit,
    AeroLowBatteryMode, AeroMaxChargeVoltageRaw, AeroPedalDipCompensation, AeroPedalHardness,
    AeroPwmPercent, AeroPwmSetting, AeroSpeedSetting, AeroTransportMode, AeroVoltageCorrection,
    DeviceCommand, DeviceEvent, Duration, GattChannel, GattFingerprint, HostSession, LightState,
    LinkInfo, ModelRegistryEntry, MonotonicTimestamp, PedalMode, RawFieldValue, ReadOnlyResponse,
    RideOperatingState, SessionOutput, SettingsEntry, SettingsReadback, Speed,
    StationarySettingsPolicy, TransportAction, TransportWriteLimit, ValueQuality, ValueSource,
    VerificationStatus, WriteMode, WritePayload,
};

use crate::{
    AeroControlEncoder, NOSFET_AERO_REGISTRY_ENTRY, NosfetAeroModel, ProtocolModelSpec,
    StationarySettingsWriteSession, SupportsSettingsWrites,
};

/// Typed settings readback held by the simulated NOSFET Aero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AeroSettingsReadback {
    /// Current TLT speed, when the simulator has a value.
    pub tiltback_speed: Option<AeroSpeedSetting>,

    /// Current PWT percentage, when the simulator has a value.
    pub pwm_percent: Option<AeroPwmSetting>,

    /// Current simulated wheel display backlight brightness, when explicitly set.
    pub display_backlight: Option<AeroDisplayBacklight>,

    /// Current simulated wheel beeper volume, when explicitly set.
    pub beeper_volume: Option<AeroBeeperVolume>,

    /// Current simulated dynamic assist, when explicitly set.
    pub dynamic_assist: Option<AeroDynamicAssist>,

    /// Current simulated pedal-dip compensation, when explicitly set.
    pub pedal_dip_compensation: Option<AeroPedalDipCompensation>,

    /// Current simulated lateral tilt limit, when explicitly set.
    pub lateral_tilt_limit: Option<AeroLateralTiltLimit>,

    /// Current simulated voltage correction, when explicitly set.
    pub voltage_correction: Option<AeroVoltageCorrection>,

    /// Current official MxV raw maximum-charge value.
    pub max_charge_voltage_raw: Option<AeroMaxChargeVoltageRaw>,

    /// Current numeric MD pedal hardness, when the simulator has a value.
    pub pedal_hardness: Option<AeroPedalHardness>,

    /// Simulated wheel display units, independent of host formatting preferences.
    pub wheel_units: Option<cutout_core::AeroWheelUnits>,

    /// Current high-speed mode, when explicitly set.
    pub high_speed_mode: Option<AeroHighSpeedMode>,

    /// Current low-battery mode, when explicitly set.
    pub low_battery_mode: Option<AeroLowBatteryMode>,

    /// Current transportation mode, when explicitly set.
    pub transport_mode: Option<AeroTransportMode>,

    /// Current ALM speed, when the simulator has a value.
    pub alarm_speed: Option<AeroSpeedSetting>,

    /// Current ANG adjustment, when the simulator has a value.
    pub angle_adjustment: Option<AeroAngleAdjustment>,

    /// Current page-8 gyro-calibration phase.
    pub gyro_calibration_state: Option<AeroGyroCalibrationState>,

    /// Current NOSFET brake overpressure alarm threshold.
    pub brake_overpressure_alarm: Option<AeroBrakeOverpressureAlarm>,

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
            display_backlight: None,
            beeper_volume: None,
            dynamic_assist: None,
            pedal_dip_compensation: None,
            lateral_tilt_limit: None,
            voltage_correction: None,
            max_charge_voltage_raw: None,
            pedal_hardness: None,
            wheel_units: None,
            high_speed_mode: None,
            low_battery_mode: None,
            transport_mode: None,
            alarm_speed: None,
            angle_adjustment: None,
            gyro_calibration_state: None,
            brake_overpressure_alarm: None,
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
        let Some(brake_overpressure_alarm) = AeroBrakeOverpressureAlarm::new(100) else {
            return Self::unknown();
        };
        Self {
            tiltback_speed: Some(tiltback_speed),
            pwm_percent: Some(pwm_percent.into()),
            display_backlight: None,
            beeper_volume: None,
            dynamic_assist: None,
            pedal_dip_compensation: None,
            lateral_tilt_limit: None,
            voltage_correction: None,
            max_charge_voltage_raw: None,
            pedal_hardness: None,
            wheel_units: None,
            high_speed_mode: None,
            low_battery_mode: None,
            transport_mode: None,
            alarm_speed: Some(alarm_speed),
            angle_adjustment: Some(angle_adjustment),
            gyro_calibration_state: Some(AeroGyroCalibrationState::Idle),
            brake_overpressure_alarm: Some(brake_overpressure_alarm),
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
/// its transport actions. It emits typed settings readback events only after
/// the corresponding write sequence completes, so mobile tests consume the
/// same output conversion path as a live BLE connection without claiming
/// hardware effects.
#[derive(Clone, Debug)]
pub struct AeroSettingsSimulator {
    session: HostSession<StationarySettingsWriteSession<NosfetAeroModel, false>>,
    readback: AeroSettingsReadback,
    writes: Vec<AeroSimulatorWrite>,
    pending_readback: Option<DeviceCommand>,
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
            pending_readback: None,
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
        } else {
            self.session.session_mut().clear_arm();
            self.pending_readback = None;
        }
        self.session.tick(monotonic_ms);
        let mut outputs = self.drain_outputs();
        self.complete_pending_readback(&mut outputs);
        self.session.issue_command(command);
        let command_outputs = self.drain_outputs();
        let command_wrote = command_outputs.iter().any(has_transport_write);
        outputs.extend(command_outputs);
        if command_wrote {
            if Self::is_delayed_sequence(command) {
                self.pending_readback = Some(command);
            } else {
                outputs.push(self.apply_readback(command));
            }
        }
        outputs
    }

    /// Advances the simulated session and returns newly emitted outputs.
    #[must_use]
    pub fn tick(&mut self, monotonic_ms: MonotonicTimestamp) -> Vec<SessionOutput> {
        self.session.tick(monotonic_ms);
        let mut outputs = self.drain_outputs();
        self.complete_pending_readback(&mut outputs);
        outputs
    }

    fn complete_pending_readback(&mut self, outputs: &mut Vec<SessionOutput>) {
        if self.pending_readback.is_some() && outputs.iter().any(has_transport_write) {
            if let Some(command) = self.pending_readback.take() {
                outputs.push(self.apply_readback(command));
            }
        }
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

    fn apply_readback(&mut self, command: DeviceCommand) -> SessionOutput {
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
            DeviceCommand::SetAeroPwmOff => {
                self.readback.pwm_percent = Some(AeroPwmSetting::Off);
            }
            DeviceCommand::SetAeroGyroCalibration => {
                let current = self
                    .readback
                    .gyro_calibration_state
                    .unwrap_or(AeroGyroCalibrationState::Idle);
                self.readback.gyro_calibration_state = Some(match current {
                    AeroGyroCalibrationState::Waiting => AeroGyroCalibrationState::Complete,
                    AeroGyroCalibrationState::Idle => AeroGyroCalibrationState::Waiting,
                    AeroGyroCalibrationState::Complete => AeroGyroCalibrationState::Idle,
                });
            }
            DeviceCommand::SetAeroBrakeOverpressureAlarm(value) => {
                self.readback.brake_overpressure_alarm = Some(value);
            }
            DeviceCommand::SetAeroDisplayBacklight(value) => {
                self.readback.display_backlight = Some(value);
            }
            DeviceCommand::SetAeroBeeperVolume(value) => {
                self.readback.beeper_volume = Some(value);
            }
            DeviceCommand::SetAeroDynamicAssist(value) => {
                self.readback.dynamic_assist = Some(value);
            }
            DeviceCommand::SetAeroPedalDipCompensation(value) => {
                self.readback.pedal_dip_compensation = Some(value);
            }
            DeviceCommand::SetAeroLateralTiltLimit(value) => {
                self.readback.lateral_tilt_limit = Some(value);
            }
            DeviceCommand::SetAeroVoltageCorrection(value) => {
                self.readback.voltage_correction = Some(value);
            }
            DeviceCommand::SetAeroMaxChargeVoltageRaw(value) => {
                self.readback.max_charge_voltage_raw = Some(value);
            }
            DeviceCommand::SetAeroPedalHardness(value) => {
                self.readback.pedal_hardness = Some(value);
            }
            DeviceCommand::SetAeroWheelUnits(value) => {
                self.readback.wheel_units = Some(value);
            }
            DeviceCommand::SetAeroHighSpeedMode(value) => {
                self.readback.high_speed_mode = Some(value);
            }
            DeviceCommand::SetAeroLowBatteryMode(value) => {
                self.readback.low_battery_mode = Some(value);
            }
            DeviceCommand::SetAeroTransportMode(value) => {
                self.readback.transport_mode = Some(value);
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
        SessionOutput::Event(DeviceEvent::ReadOnlyResponse(ReadOnlyResponse::Settings(
            self.settings_readback(),
        )))
    }

    fn is_delayed_sequence(command: DeviceCommand) -> bool {
        AeroControlEncoder::encode_settings_sequence(command)
            .is_some_and(|sequence| sequence.steps.len() > 1)
    }

    fn settings_readback(&self) -> SettingsReadback {
        SettingsReadback::available([
            self.readback.tiltback_speed.map(|value| {
                settings_entry(
                    crate::VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                    i64::from(value.kilometres_per_hour()) * 10,
                )
            }),
            self.readback.alarm_speed.map(|value| {
                settings_entry(
                    crate::VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
                    i64::from(value.kilometres_per_hour()) * 10,
                )
            }),
            self.readback.gyro_calibration_state.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_GYRO_CALIBRATION_STATE,
                    i64::from(value.wire_value()),
                )
            }),
            self.readback.brake_overpressure_alarm.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_BRAKE_OVERPRESSURE_ALARM_PERCENT,
                    i64::from(value.percent()),
                )
            }),
            self.readback.pedal_mode.map(|value| {
                settings_entry(
                    crate::VETERAN_FIELD_PEDALS_MODE,
                    i64::from(pedal_mode_raw(value)),
                )
            }),
            self.readback.display_backlight.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_DISPLAY_BACKLIGHT_PERCENT,
                    i64::from(value.percent()),
                )
            }),
            self.readback.beeper_volume.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_BEEPER_VOLUME_PERCENT,
                    i64::from(value.percent()),
                )
            }),
            self.readback.dynamic_assist.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_DYNAMIC_ASSIST_PERCENT,
                    i64::from(value.percent()),
                )
            }),
            self.readback.pedal_dip_compensation.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_PEDAL_DIP_COMPENSATION_PERCENT,
                    i64::from(value.percent()),
                )
            }),
            self.readback.lateral_tilt_limit.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_LATERAL_TILT_LIMIT_DEGREES,
                    i64::from(value.degrees()),
                )
            }),
            self.readback.voltage_correction.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT,
                    i64::from(value.tenths_of_percent()),
                )
            }),
            self.readback.max_charge_voltage_raw.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_MAX_CHARGE_VOLTAGE_RAW,
                    i64::from(value.raw()),
                )
            }),
            self.readback.pedal_hardness.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_PEDAL_HARDNESS_PERCENT,
                    i64::from(value.percent()),
                )
            }),
            self.readback.wheel_units.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_WHEEL_UNITS,
                    i64::from(value.display_mode()),
                )
            }),
            self.readback.high_speed_mode.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_HIGH_SPEED_MODE,
                    i64::from(u8::from(value.enabled())),
                )
            }),
            self.readback.low_battery_mode.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_LOW_BATTERY_MODE,
                    i64::from(u8::from(value.enabled())),
                )
            }),
            self.readback.transport_mode.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_TRANSPORT_MODE,
                    i64::from(u8::from(value.enabled())),
                )
            }),
        ])
    }
}

const fn settings_entry(id: u16, value: i64) -> SettingsEntry {
    SettingsEntry {
        field: RawFieldValue::new(id, value),
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    }
}

const fn pedal_mode_raw(value: PedalMode) -> u16 {
    match value {
        PedalMode::Hard => 0,
        PedalMode::Medium => 1,
        PedalMode::Soft => 2,
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

    fn pwm(value: u8) -> AeroPwmSetting {
        AeroPwmPercent::new(value)
            .expect("test pwm is in range")
            .into()
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
            DeviceCommand::SetAeroHighSpeedMode(AeroHighSpeedMode::new(true)),
            DeviceCommand::SetAeroLowBatteryMode(AeroLowBatteryMode::new(false)),
            DeviceCommand::SetAeroTransportMode(AeroTransportMode::new(true)),
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
                expected.as_ref().map(WritePayload::as_slice)
            );
        }

        let readback = simulator.readback();
        assert_eq!(readback.tiltback_speed, Some(speed(53)));
        assert_eq!(readback.pwm_percent, Some(pwm(64)));
        assert_eq!(readback.alarm_speed, Some(speed(56)));
        assert_eq!(readback.angle_adjustment, Some(angle(-12)));
        assert_eq!(readback.pedal_mode, Some(PedalMode::Hard));
        assert_eq!(readback.high_speed_mode, Some(AeroHighSpeedMode::new(true)));
        assert_eq!(
            readback.low_battery_mode,
            Some(AeroLowBatteryMode::new(false))
        );
        assert_eq!(readback.transport_mode, Some(AeroTransportMode::new(true)));
        assert_eq!(simulator.writes().len(), commands.len());
    }

    #[test]
    fn simulator_toggles_gyro_calibration_off_after_completion() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);

        let _ = simulator.issue(DeviceCommand::SetAeroGyroCalibration, parked(), None, now);
        assert_eq!(
            simulator.readback().gyro_calibration_state,
            Some(AeroGyroCalibrationState::Waiting)
        );

        let _ = simulator.issue(
            DeviceCommand::SetAeroGyroCalibration,
            parked(),
            None,
            MonotonicTimestamp::new(1_210),
        );
        assert_eq!(
            simulator.readback().gyro_calibration_state,
            Some(AeroGyroCalibrationState::Complete)
        );

        let _ = simulator.issue(
            DeviceCommand::SetAeroGyroCalibration,
            parked(),
            None,
            MonotonicTimestamp::new(1_220),
        );
        assert_eq!(
            simulator.readback().gyro_calibration_state,
            Some(AeroGyroCalibrationState::Idle)
        );
    }

    #[test]
    fn simulator_emits_the_typed_settings_readback_event() {
        let mut simulator = AeroSettingsSimulator::default();
        let outputs = simulator.issue(
            DeviceCommand::SetAeroTiltbackSpeed(speed(53)),
            parked(),
            None,
            MonotonicTimestamp::new(10),
        );

        assert!(outputs.iter().any(|output| {
            matches!(
                output,
                SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    ReadOnlyResponse::Settings(settings)
                )) if settings.entries().iter().flatten().any(|entry| {
                    entry.field == RawFieldValue::new(crate::VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, 530)
                })
            )
        }));
    }

    #[test]
    fn simulator_readback_event_matches_production_settings_shape() {
        let mut simulator = AeroSettingsSimulator::default();
        let mut outputs = simulator.issue(
            DeviceCommand::SetAeroPwmPercent(pwm(64)),
            parked(),
            None,
            MonotonicTimestamp::new(10),
        );
        outputs.extend(simulator.tick(MonotonicTimestamp::new(17)));

        let settings = outputs.iter().find_map(|output| match output {
            SessionOutput::Event(DeviceEvent::ReadOnlyResponse(ReadOnlyResponse::Settings(
                settings,
            ))) => Some(settings),
            _ => None,
        });
        let settings = settings.expect("completed writes emit a settings event");
        let fields: Vec<_> = settings
            .entries()
            .iter()
            .flatten()
            .map(|entry| entry.field.id)
            .collect();
        assert_eq!(
            fields,
            vec![
                crate::VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                crate::VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
                crate::AERO_FIELD_GYRO_CALIBRATION_STATE,
                crate::AERO_FIELD_BRAKE_OVERPRESSURE_ALARM_PERCENT,
                crate::VETERAN_FIELD_PEDALS_MODE,
            ]
        );
        assert_eq!(simulator.readback().pwm_percent, Some(pwm(64)));
    }

    #[test]
    fn simulator_readback_event_includes_page_eight_settings() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);
        let commands = [
            DeviceCommand::SetAeroDisplayBacklight(
                AeroDisplayBacklight::new(80).expect("80 percent fits"),
            ),
            DeviceCommand::SetAeroBeeperVolume(AeroBeeperVolume::new(40).expect("40 percent fits")),
            DeviceCommand::SetAeroDynamicAssist(
                AeroDynamicAssist::new(35).expect("35 percent fits"),
            ),
            DeviceCommand::SetAeroPedalDipCompensation(
                AeroPedalDipCompensation::new(25).expect("25 percent fits"),
            ),
            DeviceCommand::SetAeroLateralTiltLimit(
                AeroLateralTiltLimit::new(55).expect("55 degrees fits"),
            ),
            DeviceCommand::SetAeroVoltageCorrection(
                AeroVoltageCorrection::new(-5).expect("-5 tenths fits"),
            ),
            DeviceCommand::SetAeroMaxChargeVoltageRaw(
                AeroMaxChargeVoltageRaw::new(46).expect("official raw MxV value fits"),
            ),
            DeviceCommand::SetAeroPedalHardness(
                AeroPedalHardness::new(70).expect("70 percent fits"),
            ),
            DeviceCommand::SetAeroWheelUnits(cutout_core::AeroWheelUnits::Imperial),
            DeviceCommand::SetAeroHighSpeedMode(AeroHighSpeedMode::new(true)),
            DeviceCommand::SetAeroLowBatteryMode(AeroLowBatteryMode::new(false)),
            DeviceCommand::SetAeroTransportMode(AeroTransportMode::new(true)),
        ];

        for command in commands {
            let _ = simulator.issue(command, parked(), None, now);
        }

        let settings = simulator.settings_readback();
        let fields: Vec<_> = settings
            .entries()
            .iter()
            .flatten()
            .map(|entry| (entry.field.id, entry.field.value))
            .collect();

        assert!(fields.contains(&(crate::AERO_FIELD_DISPLAY_BACKLIGHT_PERCENT, 80)));
        assert!(fields.contains(&(crate::AERO_FIELD_BEEPER_VOLUME_PERCENT, 40)));
        assert!(fields.contains(&(crate::AERO_FIELD_DYNAMIC_ASSIST_PERCENT, 35)));
        assert!(fields.contains(&(crate::AERO_FIELD_PEDAL_DIP_COMPENSATION_PERCENT, 25)));
        assert!(fields.contains(&(crate::AERO_FIELD_LATERAL_TILT_LIMIT_DEGREES, 55)));
        assert!(fields.contains(&(crate::AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT, -5)));
        assert!(fields.contains(&(crate::AERO_FIELD_MAX_CHARGE_VOLTAGE_RAW, 46)));
        assert!(fields.contains(&(crate::AERO_FIELD_PEDAL_HARDNESS_PERCENT, 70)));
        assert!(fields.contains(&(crate::AERO_FIELD_WHEEL_UNITS, 1)));
        assert!(fields.contains(&(crate::AERO_FIELD_HIGH_SPEED_MODE, 1)));
        assert!(fields.contains(&(crate::AERO_FIELD_LOW_BATTERY_MODE, 0)));
        assert!(fields.contains(&(crate::AERO_FIELD_TRANSPORT_MODE, 1)));
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
            DeviceCommand::SetAeroPwmPercent(pwm(69)),
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
    fn simulator_applies_md_independently_and_refuses_moving_writes() {
        let mut simulator = AeroSettingsSimulator::default();
        let mode = simulator.readback().pedal_mode;
        for percent in [0, 100] {
            let hardness = AeroPedalHardness::new(percent).expect("documented bound");
            let _ = simulator.issue(
                DeviceCommand::SetAeroPedalHardness(hardness),
                parked(),
                None,
                MonotonicTimestamp::new(10),
            );
            assert_eq!(simulator.readback().pedal_hardness, Some(hardness));
            assert_eq!(simulator.readback().pedal_mode, mode);
        }
        assert!(AeroPedalHardness::new(101).is_none());
        assert!(AeroPedalHardness::new(u8::MAX).is_none());
        simulator.clear_writes();
        let _ = simulator.issue(
            DeviceCommand::SetAeroPedalHardness(AeroPedalHardness::new(50).expect("in range")),
            RideOperatingState::Riding,
            Some(Speed::from_millimetres_per_second(501)),
            MonotonicTimestamp::new(11),
        );
        assert!(simulator.writes().is_empty());
        assert_eq!(
            simulator.readback().pedal_hardness,
            AeroPedalHardness::new(100)
        );
    }

    #[test]
    fn simulator_records_the_aero_high_beam_frame() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);
        let first_outputs = simulator.issue(
            DeviceCommand::SetAeroHighBeam(LightState::On),
            parked(),
            None,
            now,
        );
        assert!(first_outputs.iter().any(is_settings_readback));
        let second_outputs = simulator.tick(now);
        assert!(!second_outputs.iter().any(is_settings_readback));

        let expected = AeroControlEncoder::encode_settings_sequence(
            DeviceCommand::SetAeroHighBeam(LightState::On),
        )
        .expect("high beam has a source-backed frame sequence");
        assert_eq!(simulator.writes().len(), expected.steps.len());
        for (write, step) in simulator.writes().iter().zip(expected.steps) {
            assert_eq!(write.payload, step.payload);
            assert_eq!(write.mode, step.mode);
        }
        assert_eq!(simulator.readback().high_beam, Some(LightState::On));
    }

    #[test]
    fn simulator_keeps_completed_high_beam_write_after_later_command() {
        let mut simulator = AeroSettingsSimulator::default();
        let _ = simulator.issue(
            DeviceCommand::SetAeroHighBeam(LightState::On),
            parked(),
            None,
            MonotonicTimestamp::new(10),
        );
        simulator.clear_writes();
        let _ = simulator.issue(
            DeviceCommand::SetAeroTiltbackSpeed(speed(53)),
            parked(),
            None,
            MonotonicTimestamp::new(6_000),
        );
        assert_eq!(simulator.writes().len(), 1);
        assert_eq!(simulator.readback().high_beam, Some(LightState::On));
        assert_eq!(simulator.readback().tiltback_speed, Some(speed(53)));
    }

    fn is_settings_readback(output: &SessionOutput) -> bool {
        matches!(
            output,
            SessionOutput::Event(DeviceEvent::ReadOnlyResponse(ReadOnlyResponse::Settings(_)))
        )
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
