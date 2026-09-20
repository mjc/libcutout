use cutout_core::{
    DeviceActionId, DeviceActionRequest, DeviceCommand, DeviceEvent, DeviceSettingValue, Duration,
    GattChannel, GattFingerprint, HostSession, LightState, LinkInfo, ModelRegistryEntry,
    MonotonicTimestamp, PedalMode, RawFieldValue, ReadOnlyResponse, RideOperatingState,
    SessionOutput, SettingId, SettingsEntry, SettingsReadback, Speed, StationarySettingsPolicy,
    TransportAction, TransportWriteLimit, ValueQuality, ValueSource, VerificationStatus, WriteMode,
    WritePayload,
};

use crate::settings_wire::*;
use crate::{
    NOSFET_AERO_REGISTRY_ENTRY, NosfetAeroModel, ProtocolModelSpec, StationarySettingsWriteSession,
    SupportsSettingsWrites,
};

/// Typed settings readback held by the simulated NOSFET Aero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AeroSettingsReadback {
    /// Current TLT speed, when the simulator has a value.
    pub tiltback_speed: Option<VeteranSpeedSetting>,

    /// Current PWT percentage, when the simulator has a value.
    pub pwm_percent: Option<VeteranPwmSetting>,

    /// Current simulated wheel display backlight brightness, when explicitly set.
    pub display_backlight: Option<VeteranDisplayBacklight>,

    /// Current simulated wheel beeper volume, when explicitly set.
    pub beeper_volume: Option<VeteranBeeperVolume>,

    /// Current simulated dynamic assist, when explicitly set.
    pub dynamic_assist: Option<VeteranDynamicAssist>,

    /// Current simulated pedal-dip compensation, when explicitly set.
    pub pedal_dip_compensation: Option<VeteranPedalDipCompensation>,

    /// Current simulated lateral tilt limit, when explicitly set.
    pub lateral_tilt_limit: Option<VeteranLateralTiltLimit>,

    /// Current simulated voltage correction, when explicitly set.
    pub voltage_correction: Option<VeteranVoltageCorrection>,

    /// Current official MxV raw maximum-charge value.
    pub max_charge_voltage_raw: Option<VeteranMaxChargeVoltageRaw>,

    /// Current numeric MD pedal hardness, when the simulator has a value.
    pub pedal_hardness: Option<VeteranPedalHardness>,

    /// Simulated wheel display units, independent of host formatting preferences.
    pub wheel_units: Option<VeteranWheelUnits>,

    /// Current high-speed mode, when explicitly set.
    pub high_speed_mode: Option<VeteranHighSpeedMode>,

    /// Current low-battery mode, when explicitly set.
    pub low_battery_mode: Option<VeteranLowBatteryMode>,

    /// Current transportation mode, when explicitly set.
    pub transport_mode: Option<VeteranTransportMode>,

    /// Current ALM speed, when the simulator has a value.
    pub alarm_speed: Option<VeteranSpeedSetting>,

    /// Current ANG adjustment, when the simulator has a value.
    pub angle_adjustment: Option<VeteranAngleAdjustment>,

    /// Current page-8 gyro-calibration phase.
    pub gyro_calibration_state: Option<VeteranGyroCalibrationState>,

    /// Current NOSFET brake overpressure alarm threshold.
    pub brake_overpressure_alarm: Option<VeteranBrakeOverpressureAlarm>,

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
        let Some(tiltback_speed) = VeteranSpeedSetting::new(20) else {
            return Self::unknown();
        };
        let Some(pwm_percent) = VeteranPwmPercent::new(60) else {
            return Self::unknown();
        };
        let Some(alarm_speed) = VeteranSpeedSetting::new(20) else {
            return Self::unknown();
        };
        let Some(angle_adjustment) = VeteranAngleAdjustment::new(0) else {
            return Self::unknown();
        };
        let Some(brake_overpressure_alarm) = VeteranBrakeOverpressureAlarm::new(100) else {
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
            gyro_calibration_state: Some(VeteranGyroCalibrationState::Idle),
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
    session: HostSession<StationarySettingsWriteSession<NosfetAeroModel>>,
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
        }
        self.session.tick(monotonic_ms);
        let mut outputs = self.drain_outputs();
        self.session.issue_command(command);
        let command_outputs = self.drain_outputs();
        let command_wrote = command_outputs.iter().any(has_transport_write);
        outputs.extend(command_outputs);
        if command_wrote {
            outputs.push(self.apply_readback(command));
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

    fn apply_readback(&mut self, command: DeviceCommand) -> SessionOutput {
        match command {
            DeviceCommand::InvokeAction(DeviceActionRequest { id, .. }) => match id {
                DeviceActionId::ResetTripMeter => {
                    self.readback.trip_meter_reset_count =
                        self.readback.trip_meter_reset_count.saturating_add(1);
                }
                DeviceActionId::GyroCalibration => {
                    let current = self
                        .readback
                        .gyro_calibration_state
                        .unwrap_or(VeteranGyroCalibrationState::Idle);
                    self.readback.gyro_calibration_state = Some(match current {
                        VeteranGyroCalibrationState::Waiting => {
                            VeteranGyroCalibrationState::Complete
                        }
                        VeteranGyroCalibrationState::Idle => VeteranGyroCalibrationState::Waiting,
                        VeteranGyroCalibrationState::Complete => VeteranGyroCalibrationState::Idle,
                    });
                }
                DeviceActionId::Horn => {}
            },
            DeviceCommand::SetSetting { id, value } => {
                self.apply_setting_readback(id, value);
            }
            DeviceCommand::SetLights(value) => {
                self.readback.headlight = Some(value);
            }
            _ => {}
        }
        SessionOutput::Event(DeviceEvent::ReadOnlyResponse(ReadOnlyResponse::Settings(
            self.settings_readback(),
        )))
    }

    fn apply_setting_readback(&mut self, id: SettingId, value: DeviceSettingValue) {
        match (id, value) {
            (SettingId::TiltbackSpeed, DeviceSettingValue::Number(value)) => {
                self.readback.tiltback_speed = u8::try_from(value / 10)
                    .ok()
                    .and_then(VeteranSpeedSetting::new);
            }
            (SettingId::SpeedAlarmThreshold, DeviceSettingValue::Number(value)) => {
                self.readback.alarm_speed = u8::try_from(value / 10)
                    .ok()
                    .and_then(VeteranSpeedSetting::new);
            }
            (SettingId::PedalAngle, DeviceSettingValue::Number(value)) => {
                self.readback.angle_adjustment = i8::try_from(value)
                    .ok()
                    .and_then(VeteranAngleAdjustment::new);
            }
            (SettingId::PwmTiltback, DeviceSettingValue::Disabled) => {
                self.readback.pwm_percent = Some(VeteranPwmSetting::Off);
            }
            (SettingId::PwmTiltback, DeviceSettingValue::Number(value)) => {
                self.readback.pwm_percent = u8::try_from(value)
                    .ok()
                    .and_then(|duty| 100_u8.checked_sub(duty))
                    .and_then(VeteranPwmPercent::new)
                    .map(VeteranPwmSetting::Margin);
            }
            (SettingId::BrakeOverpressureAlarm, DeviceSettingValue::Number(value)) => {
                self.readback.brake_overpressure_alarm = u8::try_from(value)
                    .ok()
                    .and_then(VeteranBrakeOverpressureAlarm::new);
            }
            (SettingId::DisplayBrightness, DeviceSettingValue::Number(value)) => {
                self.readback.display_backlight = u8::try_from(value)
                    .ok()
                    .and_then(VeteranDisplayBacklight::new);
            }
            (SettingId::BeeperVolumePercent, DeviceSettingValue::Number(value)) => {
                self.readback.beeper_volume =
                    u8::try_from(value).ok().and_then(VeteranBeeperVolume::new);
            }
            (SettingId::DynamicAssist, DeviceSettingValue::Number(value)) => {
                self.readback.dynamic_assist =
                    u8::try_from(value).ok().and_then(VeteranDynamicAssist::new);
            }
            (SettingId::PedalDipCompensation, DeviceSettingValue::Number(value)) => {
                self.readback.pedal_dip_compensation = u8::try_from(value)
                    .ok()
                    .and_then(VeteranPedalDipCompensation::new);
            }
            (SettingId::LateralTiltLimit, DeviceSettingValue::Number(value)) => {
                self.readback.lateral_tilt_limit = u8::try_from(value)
                    .ok()
                    .and_then(VeteranLateralTiltLimit::new);
            }
            (SettingId::VoltageCorrection, DeviceSettingValue::Number(value)) => {
                self.readback.voltage_correction = i8::try_from(value)
                    .ok()
                    .and_then(VeteranVoltageCorrection::new);
            }
            (SettingId::ChargeLimitDiagnostic, DeviceSettingValue::Number(value)) => {
                self.readback.max_charge_voltage_raw = u8::try_from(value)
                    .ok()
                    .and_then(VeteranMaxChargeVoltageRaw::new);
            }
            (SettingId::PedalHardness, DeviceSettingValue::Number(value)) => {
                self.readback.pedal_hardness =
                    u8::try_from(value).ok().and_then(VeteranPedalHardness::new);
            }
            (SettingId::DisplayUnits, DeviceSettingValue::Choice(value)) => {
                self.readback.wheel_units = u8::try_from(value)
                    .ok()
                    .and_then(VeteranWheelUnits::from_display_mode);
            }
            (SettingId::HighSpeedMode, DeviceSettingValue::Boolean(value)) => {
                self.readback.high_speed_mode = Some(VeteranHighSpeedMode::new(value));
            }
            (SettingId::LowBatteryMode, DeviceSettingValue::Boolean(value)) => {
                self.readback.low_battery_mode = Some(VeteranLowBatteryMode::new(value));
            }
            (SettingId::TransportMode, DeviceSettingValue::Boolean(value)) => {
                self.readback.transport_mode = Some(VeteranTransportMode::new(value));
            }
            (SettingId::RidingPreset, DeviceSettingValue::Choice(value)) => {
                self.readback.pedal_mode = match value {
                    0 => Some(PedalMode::Hard),
                    1 => Some(PedalMode::Medium),
                    2 => Some(PedalMode::Soft),
                    _ => None,
                };
            }
            (SettingId::HighBeam, DeviceSettingValue::Boolean(value)) => {
                self.readback.high_beam = Some(if value {
                    LightState::On
                } else {
                    LightState::Off
                });
            }
            _ => {}
        }
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
            self.readback.pwm_percent.map(|value| {
                settings_entry(
                    crate::AERO_FIELD_PWM_PERCENT,
                    i64::from(pwm_wire_value(value)),
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

const fn pwm_wire_value(value: VeteranPwmSetting) -> u8 {
    match value {
        VeteranPwmSetting::Off => 200,
        VeteranPwmSetting::Margin(percent) => 100 - percent.percent(),
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
    use crate::NosfetDialect;
    use cutout_core::{ControlRefusalReason, DeviceActionStep, DeviceEvent};

    fn speed(value: u8) -> VeteranSpeedSetting {
        VeteranSpeedSetting::new(value).expect("test speed is in range")
    }

    fn pwm(value: u8) -> VeteranPwmSetting {
        VeteranPwmPercent::new(value)
            .expect("test pwm is in range")
            .into()
    }

    fn angle(value: i8) -> VeteranAngleAdjustment {
        VeteranAngleAdjustment::new(value).expect("test angle is in range")
    }

    const fn setting(id: SettingId, value: DeviceSettingValue) -> DeviceCommand {
        DeviceCommand::SetSetting { id, value }
    }

    const fn action(id: DeviceActionId) -> DeviceCommand {
        DeviceCommand::InvokeAction(DeviceActionRequest {
            id,
            step: DeviceActionStep::Invoke,
        })
    }

    const fn parked() -> RideOperatingState {
        RideOperatingState::Parked
    }

    #[test]
    fn simulator_applies_aero_tune_writes_and_records_transport() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);
        let commands = [
            setting(SettingId::TiltbackSpeed, DeviceSettingValue::Number(530)),
            setting(SettingId::PwmTiltback, DeviceSettingValue::Number(64)),
            setting(
                SettingId::SpeedAlarmThreshold,
                DeviceSettingValue::Number(560),
            ),
            setting(SettingId::PedalAngle, DeviceSettingValue::Number(-12)),
            setting(SettingId::RidingPreset, DeviceSettingValue::Choice(0)),
            setting(SettingId::HighSpeedMode, DeviceSettingValue::Boolean(true)),
            setting(
                SettingId::LowBatteryMode,
                DeviceSettingValue::Boolean(false),
            ),
            setting(SettingId::TransportMode, DeviceSettingValue::Boolean(true)),
        ];

        for command in commands {
            let outputs = simulator.issue(command, parked(), None, now);
            assert!(outputs.iter().any(has_transport_write));
            let expected = NosfetDialect::encode(command).map(|encoded| encoded.payload);
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
        assert_eq!(readback.pwm_percent, Some(pwm(36)));
        assert_eq!(readback.alarm_speed, Some(speed(56)));
        assert_eq!(readback.angle_adjustment, Some(angle(-12)));
        assert_eq!(readback.pedal_mode, Some(PedalMode::Hard));
        assert_eq!(
            readback.high_speed_mode,
            Some(VeteranHighSpeedMode::new(true))
        );
        assert_eq!(
            readback.low_battery_mode,
            Some(VeteranLowBatteryMode::new(false))
        );
        assert_eq!(
            readback.transport_mode,
            Some(VeteranTransportMode::new(true))
        );
        assert_eq!(simulator.writes().len(), commands.len());
    }

    #[test]
    fn simulator_toggles_gyro_calibration_off_after_completion() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);

        let _ = simulator.issue(action(DeviceActionId::GyroCalibration), parked(), None, now);
        assert_eq!(
            simulator.readback().gyro_calibration_state,
            Some(VeteranGyroCalibrationState::Waiting)
        );

        let _ = simulator.issue(
            action(DeviceActionId::GyroCalibration),
            parked(),
            None,
            MonotonicTimestamp::new(1_210),
        );
        assert_eq!(
            simulator.readback().gyro_calibration_state,
            Some(VeteranGyroCalibrationState::Complete)
        );

        let _ = simulator.issue(
            action(DeviceActionId::GyroCalibration),
            parked(),
            None,
            MonotonicTimestamp::new(1_220),
        );
        assert_eq!(
            simulator.readback().gyro_calibration_state,
            Some(VeteranGyroCalibrationState::Idle)
        );
    }

    #[test]
    fn simulator_emits_the_typed_settings_readback_event() {
        let mut simulator = AeroSettingsSimulator::default();
        let outputs = simulator.issue(
            setting(SettingId::TiltbackSpeed, DeviceSettingValue::Number(530)),
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
            setting(SettingId::PwmTiltback, DeviceSettingValue::Number(64)),
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
                crate::AERO_FIELD_PWM_PERCENT,
                crate::AERO_FIELD_GYRO_CALIBRATION_STATE,
                crate::AERO_FIELD_BRAKE_OVERPRESSURE_ALARM_PERCENT,
                crate::VETERAN_FIELD_PEDALS_MODE,
            ]
        );
        assert_eq!(simulator.readback().pwm_percent, Some(pwm(36)));
    }

    #[test]
    fn simulator_readback_event_includes_page_eight_settings() {
        let mut initial = AeroSettingsReadback::default();
        initial.max_charge_voltage_raw = VeteranMaxChargeVoltageRaw::new(46);
        let mut simulator = AeroSettingsSimulator::new(initial);
        let now = MonotonicTimestamp::new(10);
        let commands = [
            setting(SettingId::PwmTiltback, DeviceSettingValue::Number(64)),
            setting(SettingId::DisplayBrightness, DeviceSettingValue::Number(80)),
            setting(
                SettingId::BeeperVolumePercent,
                DeviceSettingValue::Number(40),
            ),
            setting(SettingId::DynamicAssist, DeviceSettingValue::Number(35)),
            setting(
                SettingId::PedalDipCompensation,
                DeviceSettingValue::Number(25),
            ),
            setting(SettingId::LateralTiltLimit, DeviceSettingValue::Number(55)),
            setting(SettingId::VoltageCorrection, DeviceSettingValue::Number(-5)),
            setting(SettingId::PedalHardness, DeviceSettingValue::Number(70)),
            setting(SettingId::DisplayUnits, DeviceSettingValue::Choice(1)),
            setting(SettingId::HighSpeedMode, DeviceSettingValue::Boolean(true)),
            setting(
                SettingId::LowBatteryMode,
                DeviceSettingValue::Boolean(false),
            ),
            setting(SettingId::TransportMode, DeviceSettingValue::Boolean(true)),
        ];

        for (index, command) in commands.into_iter().enumerate() {
            let monotonic_ms = now.saturating_add_duration(Duration::from_milliseconds(
                u64::try_from(index).expect("test index fits in a timestamp"),
            ));
            let _ = simulator.issue(command, parked(), None, monotonic_ms);
        }

        let settings = simulator.settings_readback();
        let fields: Vec<_> = settings
            .entries()
            .iter()
            .flatten()
            .map(|entry| (entry.field.id, entry.field.value))
            .collect();

        assert!(fields.contains(&(crate::AERO_FIELD_PWM_PERCENT, 64)));
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
    fn simulator_readback_maps_disabled_pwm_to_the_wire_sentinel() {
        let mut simulator = AeroSettingsSimulator::default();
        let settings = simulator
            .issue(
                setting(SettingId::PwmTiltback, DeviceSettingValue::Disabled),
                parked(),
                None,
                MonotonicTimestamp::new(10),
            )
            .into_iter()
            .find_map(|output| match output {
                SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                    ReadOnlyResponse::Settings(settings),
                )) => Some(settings),
                _ => None,
            })
            .expect("PWM writes emit a settings readback");

        assert!(settings.entries().into_iter().flatten().any(|entry| {
            entry.field == RawFieldValue::new(crate::AERO_FIELD_PWM_PERCENT, 200)
        }));
    }

    #[test]
    fn simulator_accepts_500_mm_per_second_and_refuses_above_it() {
        let mut simulator = AeroSettingsSimulator::default();
        let accepted = simulator.issue(
            setting(SettingId::PwmTiltback, DeviceSettingValue::Number(70)),
            RideOperatingState::Riding,
            Some(Speed::from_millimetres_per_second(500)),
            MonotonicTimestamp::new(10),
        );
        assert!(accepted.iter().any(has_transport_write));
        let write_count = simulator.writes().len();

        let refused = simulator.issue(
            setting(SettingId::PwmTiltback, DeviceSettingValue::Number(69)),
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
        assert_eq!(simulator.readback().pwm_percent, Some(pwm(30)));
    }

    #[test]
    fn simulator_keeps_unsupported_commands_write_free() {
        let mut simulator = AeroSettingsSimulator::default();
        let outputs = simulator.issue(
            setting(SettingId::RollAngleMode, DeviceSettingValue::Choice(2)),
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
        let command = setting(SettingId::TiltbackSpeed, DeviceSettingValue::Number(420));
        let _ = simulator.issue(command, parked(), None, MonotonicTimestamp::new(10));
        let _ = simulator.issue(command, parked(), None, MonotonicTimestamp::new(11));
        let _ = simulator.issue(
            action(DeviceActionId::ResetTripMeter),
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
            let hardness = VeteranPedalHardness::new(percent).expect("documented bound");
            let _ = simulator.issue(
                setting(
                    SettingId::PedalHardness,
                    DeviceSettingValue::Number(i32::from(percent)),
                ),
                parked(),
                None,
                MonotonicTimestamp::new(10),
            );
            assert_eq!(simulator.readback().pedal_hardness, Some(hardness));
            assert_eq!(simulator.readback().pedal_mode, mode);
        }
        assert!(VeteranPedalHardness::new(101).is_none());
        assert!(VeteranPedalHardness::new(u8::MAX).is_none());
        simulator.clear_writes();
        let _ = simulator.issue(
            setting(SettingId::PedalHardness, DeviceSettingValue::Number(50)),
            RideOperatingState::Riding,
            Some(Speed::from_millimetres_per_second(501)),
            MonotonicTimestamp::new(11),
        );
        assert!(simulator.writes().is_empty());
        assert_eq!(
            simulator.readback().pedal_hardness,
            VeteranPedalHardness::new(100)
        );
    }

    #[test]
    fn simulator_records_the_aero_high_beam_frame() {
        let mut simulator = AeroSettingsSimulator::default();
        let now = MonotonicTimestamp::new(10);
        let first_outputs = simulator.issue(
            setting(SettingId::HighBeam, DeviceSettingValue::Boolean(true)),
            parked(),
            None,
            now,
        );
        assert!(first_outputs.iter().any(is_settings_readback));
        let second_outputs = simulator.tick(now);
        assert!(!second_outputs.iter().any(is_settings_readback));

        let expected = NosfetDialect::encode(setting(
            SettingId::HighBeam,
            DeviceSettingValue::Boolean(true),
        ))
        .expect("high beam has a source-backed frame");
        assert_eq!(simulator.writes().len(), 1);
        assert_eq!(simulator.writes()[0].payload, expected.payload);
        assert_eq!(simulator.writes()[0].mode, expected.mode);
        assert_eq!(simulator.readback().high_beam, Some(LightState::On));
    }

    #[test]
    fn simulator_keeps_completed_high_beam_write_after_later_command() {
        let mut simulator = AeroSettingsSimulator::default();
        let _ = simulator.issue(
            setting(SettingId::HighBeam, DeviceSettingValue::Boolean(true)),
            parked(),
            None,
            MonotonicTimestamp::new(10),
        );
        simulator.clear_writes();
        let _ = simulator.issue(
            setting(SettingId::TiltbackSpeed, DeviceSettingValue::Number(530)),
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
    fn simulator_refuses_unverified_generic_lights() {
        let mut simulator = AeroSettingsSimulator::default();
        let outputs = simulator.issue(
            DeviceCommand::SetLights(LightState::On),
            RideOperatingState::Riding,
            Some(Speed::from_millimetres_per_second(2_000)),
            MonotonicTimestamp::new(10),
        );

        assert!(!outputs.iter().any(has_transport_write));
        assert_eq!(simulator.readback().headlight, Some(LightState::Off));
        assert!(simulator.writes().is_empty());
    }
}
