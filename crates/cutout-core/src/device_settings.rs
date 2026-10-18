//! Semantic settings state shared by device sessions and native clients.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

// Keep identities distinct even when a connection replaces its settings owner.
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

use crate::{
    ControlRefusalReason, Duration, Measured, MonotonicTimestamp, SETTING_CONFIRMATION_TIMEOUT,
    SettingCommandStatus, SettingCompletionStrategy, SettingState, SettingTransportStatus,
    SettingValue, SettingValueSource, ValueQuality, ValueSource,
};

/// Stable semantic identity, independent of protocol fields or model names.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SettingId {
    /// Main headlight state.
    Headlight,
    /// Independently controlled high beam.
    HighBeam,
    /// Speed that triggers tilt-back.
    TiltbackSpeed,
    /// PWM utilization that triggers tilt-back, or explicitly disabled.
    PwmTiltback,
    /// Continuous pedal hardness.
    PedalHardness,
    /// Wheel display brightness.
    DisplayBrightness,
    /// Units used by the wheel display.
    DisplayUnits,
    /// Beeper loudness measured as a percentage.
    BeeperVolumePercent,
    /// Dynamic assist strength.
    DynamicAssist,
    /// Pedal-dip compensation strength.
    PedalDipCompensation,
    /// Maximum permitted lateral tilt.
    LateralTiltLimit,
    /// Reported-voltage correction.
    VoltageCorrection,
    /// Raw charge-related diagnostic with unresolved physical meaning.
    ChargeLimitDiagnostic,
    /// High-speed operating mode.
    HighSpeedMode,
    /// Low-battery operating mode.
    LowBatteryMode,
    /// Transport operating mode.
    TransportMode,
    /// Audible speed-alarm threshold.
    SpeedAlarmThreshold,
    /// Fore/aft pedal angle.
    PedalAngle,
    /// Discrete riding response preset.
    RidingPreset,
    /// Brake overpressure alarm threshold.
    BrakeOverpressureAlarm,
    /// Discrete pedal mode; not assumed equivalent to a riding preset.
    PedalMode,
    /// Discrete roll-angle policy.
    RollAngleMode,
    /// Enabled speed alarm stages or alternative alarm policy.
    SpeedAlarmMode,
    /// Wheel maximum speed setting.
    MaximumSpeed,
    /// Discrete beeper level; not a percentage.
    BeeperVolumeLevel,
    /// Device-defined lighting pattern.
    LightingPattern,
    /// Acceleration-assist enablement.
    AccelerationAssist,
    /// Independently controlled taillight.
    Taillight,
    /// Time until automatic shutdown, reported in seconds.
    AutoShutdownRemaining,
    /// Configured idle delay before power-off, reported in minutes.
    PowerOffDelay,
    /// Refloat ordinary setting `tiltback_duty_angle`.
    PushbackDutyAngle,
    /// Refloat ordinary setting `tiltback_duty_speed`.
    PushbackDutySpeed,
    /// Refloat ordinary setting `tiltback_return_speed`.
    PushbackReturnSpeed,
    /// Refloat ordinary setting `is_dutybeep_enabled`.
    PushbackDutyBeepEnabled,
    /// Refloat ordinary setting `persistent_fatal_error`.
    PushbackPersistentFatalError,
    /// Refloat ordinary setting `tiltback_hv_angle`.
    PushbackHighVoltageAngle,
    /// Refloat ordinary setting `tiltback_hv_speed`.
    PushbackHighVoltageSpeed,
    /// Refloat ordinary setting `tiltback_lv_angle`.
    PushbackLowVoltageAngle,
    /// Refloat ordinary setting `tiltback_lv_speed`.
    PushbackLowVoltageSpeed,
    /// Refloat ordinary setting `haptic.duty.frequency`.
    HapticsDutyFrequency,
    /// Refloat ordinary setting `haptic.duty.strength`.
    HapticsDutyStrength,
    /// Refloat ordinary setting `haptic.error.frequency`.
    HapticsErrorFrequency,
    /// Refloat ordinary setting `haptic.error.strength`.
    HapticsErrorStrength,
    /// Refloat ordinary setting `haptic.vibrate.frequency`.
    HapticsVibrateFrequency,
    /// Refloat ordinary setting `haptic.vibrate.strength`.
    HapticsVibrateStrength,
    /// Refloat ordinary setting `haptic.duty_solid_offset`.
    HapticsDutySolidOffset,
    /// Refloat ordinary setting `haptic.current_threshold`.
    HapticsCurrentThreshold,
    /// Refloat ordinary setting `haptic.min_strength`.
    HapticsMinStrength,
    /// Refloat ordinary setting `haptic.max_strength_speed`.
    HapticsMaxStrengthSpeed,
    /// Refloat ordinary setting `haptic.strength_curvature`.
    HapticsStrengthCurvature,
    /// Refloat ordinary setting `inputtilt_remote_type`.
    RemoteReceiverType,
    /// Refloat ordinary setting `inputtilt_deadband`.
    RemoteInputDeadband,
    /// Refloat ordinary setting `leds.on`.
    LightingOn,
    /// Refloat ordinary setting `leds.headlights_on`.
    LightingHeadlightsOn,
    /// Refloat ordinary setting `leds.headlights_transition`.
    LightingHeadlightsTransition,
    /// Refloat ordinary setting `leds.direction_transition`.
    LightingDirectionTransition,
    /// Refloat ordinary setting `leds.lights_off_when_lifted`.
    LightingLightsOffWhenLifted,
    /// Refloat ordinary setting `leds.status_on_front_when_lifted`.
    LightingStatusOnFrontWhenLifted,
    /// Refloat ordinary setting `leds.front.mode`.
    LightingFrontMode,
    /// Refloat ordinary setting `leds.front.brightness`.
    LightingFrontBrightness,
    /// Refloat ordinary setting `leds.front.color1`.
    LightingFrontColor1,
    /// Refloat ordinary setting `leds.front.color2`.
    LightingFrontColor2,
    /// Refloat ordinary setting `leds.front.speed`.
    LightingFrontSpeed,
    /// Refloat ordinary setting `leds.rear.mode`.
    LightingRearMode,
    /// Refloat ordinary setting `leds.rear.brightness`.
    LightingRearBrightness,
    /// Refloat ordinary setting `leds.rear.color1`.
    LightingRearColor1,
    /// Refloat ordinary setting `leds.rear.color2`.
    LightingRearColor2,
    /// Refloat ordinary setting `leds.rear.speed`.
    LightingRearSpeed,
    /// Refloat ordinary setting `leds.headlights.mode`.
    LightingHeadlightsMode,
    /// Refloat ordinary setting `leds.headlights.brightness`.
    LightingHeadlightsBrightness,
    /// Refloat ordinary setting `leds.headlights.color1`.
    LightingHeadlightsColor1,
    /// Refloat ordinary setting `leds.headlights.color2`.
    LightingHeadlightsColor2,
    /// Refloat ordinary setting `leds.headlights.speed`.
    LightingHeadlightsSpeed,
    /// Refloat ordinary setting `leds.taillights.mode`.
    LightingTaillightsMode,
    /// Refloat ordinary setting `leds.taillights.brightness`.
    LightingTaillightsBrightness,
    /// Refloat ordinary setting `leds.taillights.color1`.
    LightingTaillightsColor1,
    /// Refloat ordinary setting `leds.taillights.color2`.
    LightingTaillightsColor2,
    /// Refloat ordinary setting `leds.taillights.speed`.
    LightingTaillightsSpeed,
    /// Refloat ordinary setting `leds.status.brightness_headlights_off`.
    LightingStatusBrightnessHeadlightsOff,
    /// Refloat ordinary setting `leds.status.brightness_headlights_on`.
    LightingStatusBrightnessHeadlightsOn,
    /// Refloat ordinary setting `leds.status.show_sensors_while_running`.
    LightingStatusShowSensorsWhileRunning,
    /// Refloat ordinary setting `leds.status.motor_utilization_threshold`.
    LightingStatusMotorUtilizationThreshold,
    /// Refloat ordinary setting `leds.status.red_bar_percentage`.
    LightingStatusRedBarPercentage,
    /// Refloat ordinary setting `leds.status.idle_timeout`.
    LightingStatusIdleTimeout,
    /// Refloat ordinary setting `leds.status_idle.mode`.
    LightingStatusIdleMode,
    /// Refloat ordinary setting `leds.status_idle.brightness`.
    LightingStatusIdleBrightness,
    /// Refloat ordinary setting `leds.status_idle.color1`.
    LightingStatusIdleColor1,
    /// Refloat ordinary setting `leds.status_idle.color2`.
    LightingStatusIdleColor2,
    /// Refloat ordinary setting `leds.status_idle.speed`.
    LightingStatusIdleSpeed,
    /// Refloat ordinary setting `disabled`.
    PackageDisabled,
    /// Refloat ordinary setting `tiltback_hv`.
    PushbackHv,
    /// Refloat ordinary setting `tiltback_lv`.
    PushbackLv,
    /// Refloat ordinary setting `fault_adc1`.
    SensorsAdcchannel1,
    /// Refloat ordinary setting `fault_adc2`.
    SensorsAdcchannel2,
    /// Refloat ordinary setting `hardware.swap_footpad_adcs`.
    SensorsSwapFootpadAdcs,
    /// Refloat ordinary setting `is_footbeep_enabled`.
    SensorsFootFaultBeepEnabled,
    /// Refloat ordinary setting `is_beeper_enabled`.
    HardwareBeeperEnabled,
    /// Refloat ordinary setting `hardware.leds.mode`.
    LightingHardwareMode,
    /// Refloat ordinary setting `hardware.leds.pin`.
    LightingHardwarePin,
    /// Refloat ordinary setting `hardware.leds.pin_config`.
    LightingHardwarePinConfig,
    /// Refloat ordinary setting `hardware.leds.status.order`.
    LightingHardwareStatusOrder,
    /// Refloat ordinary setting `hardware.leds.status.count`.
    LightingHardwareStatusCount,
    /// Refloat ordinary setting `hardware.leds.status.color_order`.
    LightingHardwareStatusColorOrder,
    /// Refloat ordinary setting `hardware.leds.status.reverse`.
    LightingHardwareStatusReverse,
    /// Refloat ordinary setting `hardware.leds.front.order`.
    LightingHardwareFrontOrder,
    /// Refloat ordinary setting `hardware.leds.front.count`.
    LightingHardwareFrontCount,
    /// Refloat ordinary setting `hardware.leds.front.color_order`.
    LightingHardwareFrontColorOrder,
    /// Refloat ordinary setting `hardware.leds.front.reverse`.
    LightingHardwareFrontReverse,
    /// Refloat ordinary setting `hardware.leds.rear.order`.
    LightingHardwareRearOrder,
    /// Refloat ordinary setting `hardware.leds.rear.count`.
    LightingHardwareRearCount,
    /// Refloat ordinary setting `hardware.leds.rear.color_order`.
    LightingHardwareRearColorOrder,
    /// Refloat ordinary setting `hardware.leds.rear.reverse`.
    LightingHardwareRearReverse,
    /// Refloat ordinary setting `bms.enabled`.
    BmsEnabled,
    /// Refloat ordinary setting `bms.cell_lv_threshold`.
    BmsCellLvThreshold,
    /// Refloat ordinary setting `bms.cell_hv_threshold`.
    BmsCellHvThreshold,
    /// Refloat ordinary setting `bms.cell_balance_threshold`.
    BmsCellBalanceThreshold,
    /// Refloat ordinary setting `bms.cell_ht_threshold`.
    BmsCellHtThreshold,
    /// Refloat ordinary setting `bms.cell_lt_threshold`.
    BmsCellLtThreshold,
    /// Refloat ordinary setting `bms.bms_ht_threshold`.
    BmsBmsHtThreshold,
    /// VESC configuration field `l_current_max`.
    MotorDriveCurrentLimit,
    /// VESC configuration field `l_current_min`.
    MotorBrakingCurrentLimit,
    /// VESC configuration field `l_in_current_max`.
    BatteryCurrentOutLimit,
    /// VESC configuration field `l_in_current_min`.
    BatteryCurrentInLimit,
    /// VESC configuration field `l_abs_current_max`.
    AbsoluteMotorCurrentLimit,
    /// VESC configuration field `l_min_vin`.
    InputVoltageMinimum,
    /// VESC configuration field `l_max_vin`.
    InputVoltageMaximum,
    /// VESC configuration field `l_battery_cut_start`.
    BatteryCutbackStart,
    /// VESC configuration field `l_battery_cut_end`.
    BatteryCutbackEnd,
    /// VESC configuration field `l_battery_regen_cut_start`.
    RegenerativeBatteryCutbackStart,
    /// VESC configuration field `l_battery_regen_cut_end`.
    RegenerativeBatteryCutbackEnd,
    /// VESC configuration field `l_temp_fet_start`.
    FetTemperatureCutbackStart,
    /// VESC configuration field `l_temp_fet_end`.
    FetTemperatureCutbackEnd,
    /// VESC configuration field `l_temp_motor_start`.
    MotorTemperatureCutbackStart,
    /// VESC configuration field `l_temp_motor_end`.
    MotorTemperatureCutbackEnd,
    /// VESC configuration field `l_watt_max`.
    MotorPowerOutLimit,
    /// VESC configuration field `l_watt_min`.
    RegenerativeMotorPowerLimit,
    /// VESC configuration field `l_current_max_scale`.
    MotorDriveCurrentLimitScale,
    /// VESC configuration field `l_current_min_scale`.
    MotorBrakingCurrentLimitScale,
    /// VESC configuration field `si_motor_poles`.
    MotorPoleCount,
    /// VESC configuration field `si_gear_ratio`.
    MotorGearRatio,
    /// VESC configuration field `si_wheel_diameter`.
    WheelDiameter,
    /// VESC configuration field `si_battery_type`.
    BatteryChemistry,
    /// VESC configuration field `si_battery_cells`.
    BatterySeriesCellCount,
    /// VESC configuration field `si_battery_ah`.
    BatteryCapacity,
    /// VESC configuration field `bms.type`.
    BmsType,
    /// VESC configuration field `bms.limit_mode`.
    BmsLimitMode,
    /// VESC BMS temperature limiting flag projected from `bms.limit_mode` bit 0.
    BmsTemperatureLimitingEnabled,
    /// VESC BMS state-of-charge limiting flag projected from `bms.limit_mode` bit 1.
    BmsSocLimitingEnabled,
    /// VESC BMS minimum-cell-voltage limiting flag projected from `bms.limit_mode` bit 2.
    BmsMinimumCellVoltageLimitingEnabled,
    /// VESC BMS maximum-cell-voltage limiting flag projected from `bms.limit_mode` bit 3.
    BmsMaximumCellVoltageLimitingEnabled,
    /// VESC configuration field `bms.t_limit_start`.
    BmsTemperatureLimitStart,
    /// VESC configuration field `bms.t_limit_end`.
    BmsTemperatureLimitEnd,
    /// VESC configuration field `bms.soc_limit_start`.
    BmsSocLimitStart,
    /// VESC configuration field `bms.soc_limit_end`.
    BmsSocLimitEnd,
    /// VESC configuration field `bms.vmin_limit_start`.
    BmsCellMinimumVoltageLimitStart,
    /// VESC configuration field `bms.vmin_limit_end`.
    BmsCellMinimumVoltageLimitEnd,
    /// VESC configuration field `bms.vmax_limit_start`.
    BmsCellMaximumVoltageLimitStart,
    /// VESC configuration field `bms.vmax_limit_end`.
    BmsCellMaximumVoltageLimitEnd,
    /// VESC configuration field `bms.fwd_can_mode`.
    BmsCanForwardMode,
    /// VESC configuration field `controller_id`.
    ControllerId,
    /// VESC configuration field `timeout_msec`.
    CommandTimeout,
    /// VESC configuration field `timeout_brake_current`.
    CommandTimeoutBrakeCurrent,
    /// VESC configuration field `can_status_rate_1`.
    CanStatusRate1,
    /// VESC configuration field `can_status_rate_2`.
    CanStatusRate2,
    /// VESC configuration field `can_baud_rate`.
    CanBaudRate,
    /// VESC configuration field `can_mode`.
    CanMode,
    /// VESC configuration field `app_to_use`.
    ApplicationToUse,
    /// VESC configuration field `kill_sw_mode`.
    KillSwitchMode,
    /// VESC configuration field `app_ppm_conf.ctrl_type`.
    PpmControlMode,
    /// VESC configuration field `app_ppm_conf.hyst`.
    PpmHysteresis,
    /// VESC configuration field `app_ppm_conf.pulse_start`.
    PpmPulseStart,
    /// VESC configuration field `app_ppm_conf.pulse_end`.
    PpmPulseEnd,
    /// VESC configuration field `app_ppm_conf.pulse_center`.
    PpmPulseCenter,
    /// VESC configuration field `app_ppm_conf.safe_start`.
    PpmSafeStart,
    /// VESC configuration field `app_ppm_conf.ramp_time_pos`.
    PpmRampTimePositive,
    /// VESC configuration field `app_ppm_conf.ramp_time_neg`.
    PpmRampTimeNegative,
    /// VESC configuration field `app_adc_conf.ctrl_type`.
    AdcControlMode,
    /// VESC configuration field `app_adc_conf.hyst`.
    AdcHysteresis,
    /// VESC configuration field `app_adc_conf.voltage_start`.
    AdcVoltageStart,
    /// VESC configuration field `app_adc_conf.voltage_end`.
    AdcVoltageEnd,
    /// VESC configuration field `app_adc_conf.voltage_min`.
    AdcVoltageMinimum,
    /// VESC configuration field `app_adc_conf.voltage_max`.
    AdcVoltageMaximum,
    /// VESC configuration field `app_adc_conf.safe_start`.
    AdcSafeStart,
    /// VESC configuration field `app_adc_conf.ramp_time_pos`.
    AdcRampTimePositive,
    /// VESC configuration field `app_adc_conf.ramp_time_neg`.
    AdcRampTimeNegative,
    /// VESC configuration field `app_chuk_conf.ctrl_type`.
    NunchukControlMode,
    /// VESC configuration field `app_chuk_conf.hyst`.
    NunchukHysteresis,
    /// VESC configuration field `app_chuk_conf.ramp_time_pos`.
    NunchukRampTimePositive,
    /// VESC configuration field `app_chuk_conf.ramp_time_neg`.
    NunchukRampTimeNegative,
    /// VESC configuration field `imu_conf.type`.
    ImuType,
    /// VESC configuration field `imu_conf.mode`.
    ImuMode,
    /// VESC configuration field `imu_conf.filter`.
    ImuFilter,
    /// VESC configuration field `imu_conf.sample_rate_hz`.
    ImuSampleRate,
    /// VESC configuration field `imu_conf.use_magnetometer`.
    ImuUseMagnetometer,
    /// Whether the wheel reports that it is charging.
    ChargeMode,
}

/// A descriptor's semantic value, never a raw field identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceSettingValue {
    /// Explicit on/off value; absence of readback is represented separately.
    Boolean(bool),
    /// Fixed-point quantity using the descriptor's unit and decimal precision.
    Number(i32),
    /// Opaque option identifier from the descriptor's choices, not a wire byte.
    Choice(u16),
    /// Explicitly disabled functionality, distinct from zero and unknown.
    Disabled,
}

/// Result of submitting a validated setting through the device session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingSubmissionOutcome {
    /// The session accepted the request for transport.
    Accepted,
    /// The session refused the request before transport.
    Refused(ControlRefusalReason),
    /// The session or transport failed to submit the request.
    Failed,
}

/// Immutable lifecycle/evidence projection used by every setting presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceSettingSnapshot {
    /// Stable semantic identity.
    pub id: SettingId,
    /// Most recent observed value with its original provenance.
    pub current: Option<SettingValue<DeviceSettingValue>>,
    /// Original measurement evidence when the protocol supplied it.
    pub measured: Option<Measured<DeviceSettingValue>>,
    /// Last request, kept separate from device readback.
    pub requested: Option<DeviceSettingValue>,
    /// Rust-generated identity of the most recent submission.
    pub request_id: Option<u64>,
    /// Rust-owned confirmation state.
    pub status: SettingCommandStatus,
    /// Host transport evidence for the most recent request.
    pub transport: Option<SettingTransportStatus>,
    /// Age of the current observation; absent when no value has been observed.
    pub age: Option<Duration>,
    /// Reason for the last refused request.
    pub refusal: Option<ControlRefusalReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SettingRecord {
    state: SettingState<DeviceSettingValue>,
    observed_at: Option<MonotonicTimestamp>,
    evidence: Option<Measured<()>>,
    completion: SettingCompletionStrategy,
    transport: Option<SettingTransportStatus>,
    request_id: Option<u64>,
    transport_at: Option<MonotonicTimestamp>,
}

impl Default for SettingRecord {
    fn default() -> Self {
        Self {
            state: SettingState::unknown(),
            observed_at: None,
            evidence: None,
            completion: SettingCompletionStrategy::MatchingReadback,
            transport: None,
            request_id: None,
            transport_at: None,
        }
    }
}

/// One semantic settings state owner per device session.
///
/// Protocol adapters normalize observations before updating this owner. The
/// device session must validate identity, capabilities and authorization before
/// transport; recording an outcome here does not authorize a command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceSettingsState {
    records: BTreeMap<SettingId, SettingRecord>,
    managed_transport: bool,
}

impl DeviceSettingsState {
    /// Requires explicit host handoff for subsequent submissions.
    ///
    /// Native FFI callers opt in; direct protocol callers retain immediate timing.
    pub fn require_managed_transport(&mut self) {
        self.managed_transport = true;
    }

    /// Records ordered readback without treating old values as confirmation.
    pub fn observe(
        &mut self,
        id: SettingId,
        value: DeviceSettingValue,
        source: SettingValueSource,
        observed_at: MonotonicTimestamp,
    ) {
        self.observe_readback(id, SettingValue { value, source }, None, observed_at);
    }

    /// Retains decoded evidence without upgrading inferred values to confirmation.
    pub fn observe_measured(
        &mut self,
        id: SettingId,
        measured: Measured<DeviceSettingValue>,
        observed_at: MonotonicTimestamp,
    ) {
        let source = if measured.source == ValueSource::Reported {
            SettingValueSource::LiveReadback
        } else {
            SettingValueSource::Unknown
        };
        self.observe_readback(
            id,
            SettingValue {
                value: measured.value,
                source,
            },
            Some(measured.map_value(|_| ())),
            observed_at,
        );
    }

    /// Removes an explicitly unusable readback, preserving any outstanding request.
    pub fn invalidate_readback(&mut self, id: SettingId, observed_at: MonotonicTimestamp) {
        let record = self.records.entry(id).or_default();
        if record
            .observed_at
            .is_some_and(|latest| observed_at < latest)
            || (match record.state {
                SettingState::Pending {
                    submitted_at: Some(at),
                    ..
                } if observed_at < at => true,
                _ => false,
            })
        {
            return;
        }
        match &mut record.state {
            SettingState::Pending { current, .. }
            | SettingState::Refused { current, .. }
            | SettingState::TimedOut { current, .. }
            | SettingState::Failed { current, .. } => *current = None,
            _ => record.state = SettingState::Unknown,
        }
        record.evidence = None;
        record.observed_at = Some(observed_at);
    }

    fn observe_readback(
        &mut self,
        id: SettingId,
        value: SettingValue<DeviceSettingValue>,
        evidence: Option<Measured<()>>,
        observed_at: MonotonicTimestamp,
    ) {
        let record = self.records.entry(id).or_default();
        if record
            .observed_at
            .is_some_and(|latest| observed_at < latest)
            || (match record.state {
                SettingState::Pending {
                    submitted_at: Some(at),
                    ..
                } if observed_at < at => true,
                _ => false,
            })
        {
            return;
        }
        let usable_confirmation = value.source == SettingValueSource::LiveReadback
            && evidence.is_none_or(|evidence| {
                evidence.source == ValueSource::Reported
                    && evidence.quality == ValueQuality::Known
                    && evidence.verification.is_trusted()
            });
        if (!record.completion.supports_readback() || !usable_confirmation)
            && let SettingState::Pending { current, .. } = &mut record.state
        {
            *current = Some(value);
        } else {
            record.state.observe(value.value, value.source, observed_at);
        }
        record.observed_at = Some(observed_at);
        record.evidence = evidence;
    }

    /// Records the outcome of this request, including failures before a write.
    pub fn submission(
        &mut self,
        id: SettingId,
        requested: DeviceSettingValue,
        outcome: SettingSubmissionOutcome,
        completion: SettingCompletionStrategy,
        submitted_at: MonotonicTimestamp,
    ) -> u64 {
        let record = self.records.entry(id).or_default();
        let request_id = NEXT_REQUEST_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .unwrap_or(u64::MAX);
        record.request_id = Some(request_id);
        record.transport_at = Some(submitted_at);
        record.completion = completion;
        record.transport = Some(match outcome {
            SettingSubmissionOutcome::Accepted => SettingTransportStatus::Accepted,
            SettingSubmissionOutcome::Refused(_) | SettingSubmissionOutcome::Failed => {
                SettingTransportStatus::Rejected
            }
        });
        record.state.submit(requested, submitted_at);
        if self.managed_transport {
            record.state.await_transport();
        }
        match outcome {
            SettingSubmissionOutcome::Accepted => {}
            SettingSubmissionOutcome::Refused(reason) => record.state.refuse(reason),
            SettingSubmissionOutcome::Failed => {
                record.state.fail();
            }
        }
        request_id
    }

    /// Advances only the latest request through the host transport lifecycle.
    /// Returns false for stale identities, timestamps, or invalid transitions.
    pub fn transport(
        &mut self,
        id: SettingId,
        request_id: u64,
        status: SettingTransportStatus,
        at: MonotonicTimestamp,
    ) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        if record.request_id != Some(request_id)
            || record.transport_at.is_some_and(|latest| at < latest)
            || !(match record.state {
                SettingState::Pending { .. } => true,
                _ => false,
            })
            || !(match (record.transport, status) {
                (
                    Some(SettingTransportStatus::Accepted),
                    SettingTransportStatus::Queued
                    | SettingTransportStatus::Submitted
                    | SettingTransportStatus::Rejected
                    | SettingTransportStatus::Cancelled,
                )
                | (
                    Some(SettingTransportStatus::Queued),
                    SettingTransportStatus::Submitted
                    | SettingTransportStatus::Rejected
                    | SettingTransportStatus::Cancelled,
                ) => true,
                _ => false,
            })
        {
            return false;
        }
        record.transport = Some(status);
        record.transport_at = Some(at);
        match status {
            SettingTransportStatus::Submitted => record.state.transport_submitted(at),
            SettingTransportStatus::Rejected | SettingTransportStatus::Cancelled => {
                record.state.fail();
            }
            SettingTransportStatus::Queued => {}
            SettingTransportStatus::Accepted => unreachable!(),
        }
        true
    }

    /// Fails a protocol procedure without rewriting its native transport evidence.
    ///
    /// A submitted write can still fail its subsequent verification. Stale or
    /// already terminal operations cannot change the current request.
    pub fn fail_operation(&mut self, request_id: u64, at: MonotonicTimestamp) -> bool {
        let Some(record) = self
            .records
            .values_mut()
            .find(|record| record.request_id == Some(request_id))
        else {
            return false;
        };
        if record.transport_at.is_some_and(|latest| at < latest)
            || !match record.state {
                SettingState::Pending { .. } => true,
                _ => false,
            }
        {
            return false;
        }
        record.state.fail();
        true
    }

    /// Earliest readable setting confirmation deadline; none while transport is pending.
    #[must_use]
    pub fn next_confirmation_at(&self) -> Option<MonotonicTimestamp> {
        self.records
            .values()
            .filter_map(|record| {
                if !record.completion.supports_readback() {
                    return None;
                }
                match record.state {
                    SettingState::Pending {
                        submitted_at: Some(at),
                        ..
                    } => Some(at.saturating_add_duration(SETTING_CONFIRMATION_TIMEOUT)),
                    _ => None,
                }
            })
            .min()
    }

    /// Whether this operation still requires a native transport receipt.
    #[must_use]
    pub fn awaits_transport(&self, id: SettingId, request_id: u64) -> bool {
        self.records.get(&id).is_some_and(|record| {
            if record.request_id != Some(request_id) {
                return false;
            }
            match record.transport {
                Some(SettingTransportStatus::Accepted | SettingTransportStatus::Queued) => true,
                Some(
                    SettingTransportStatus::Submitted
                    | SettingTransportStatus::Rejected
                    | SettingTransportStatus::Cancelled,
                )
                | None => false,
            }
        })
    }

    /// Advances confirmation deadlines for readable settings only.
    pub fn tick(&mut self, now: MonotonicTimestamp) {
        for record in self.records.values_mut() {
            if record.completion.supports_readback() {
                record
                    .state
                    .timeout_if_elapsed(now, SETTING_CONFIRMATION_TIMEOUT);
            }
        }
    }

    /// Invalidates all observations and pending work on a device boundary.
    pub fn disconnect(&mut self) {
        self.records.clear();
    }

    /// Projects observed and requested values without inventing defaults.
    #[must_use]
    pub fn snapshot(&self, now: MonotonicTimestamp) -> Vec<DeviceSettingSnapshot> {
        self.records
            .iter()
            .map(|(&id, record)| DeviceSettingSnapshot {
                id,
                current: record.state.current_readback(),
                measured: record
                    .state
                    .current_readback()
                    .zip(record.evidence)
                    .map(|(current, evidence)| evidence.map_value(|()| current.value)),
                requested: record.state.requested_value(),
                request_id: record.request_id,
                status: record.state.command_status(now, record.completion),
                transport: record.transport,
                age: record
                    .observed_at
                    .filter(|_| record.state.current_readback().is_some())
                    .map(|observed| now.saturating_duration_since(observed)),
                refusal: match record.state {
                    SettingState::Refused { reason, .. } => Some(reason),
                    _ => None,
                },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn procedure_failure_preserves_submission_and_cannot_confirm_from_later_readback() {
        let mut settings = DeviceSettingsState::default();
        settings.require_managed_transport();
        let id = SettingId::PwmTiltback;
        let desired = DeviceSettingValue::Number(81);
        let request = settings.submission(
            id,
            desired,
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::MatchingReadback,
            time(10),
        );
        assert!(settings.transport(id, request, SettingTransportStatus::Submitted, time(11)));
        assert!(!settings.fail_operation(request, time(9)));
        assert!(settings.fail_operation(request, time(12)));
        settings.observe(id, desired, SettingValueSource::LiveReadback, time(13));
        let record = settings.snapshot(time(14))[0];
        assert_eq!(record.status, SettingCommandStatus::Failed);
        assert_eq!(record.transport, Some(SettingTransportStatus::Submitted));
        assert_eq!(record.current.map(|reading| reading.value), Some(desired));
        assert!(!settings.fail_operation(request, time(15)));
    }
    use crate::{
        ControlRefusalReason, MonotonicTimestamp, SettingCommandStatus, SettingValueSource,
    };

    fn time(value: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::new(value)
    }

    #[test]
    fn measured_readback_retains_evidence_and_only_confirmable_evidence_acknowledges() {
        use crate::{Measured, ValueQuality, ValueSource, VerificationStatus};

        let id = SettingId::PwmTiltback;
        let value = DeviceSettingValue::Number(80);
        for source in [
            ValueSource::Reported,
            ValueSource::Calculated,
            ValueSource::Estimated,
        ] {
            for quality in [ValueQuality::Known, ValueQuality::Inferred] {
                for verification in [
                    VerificationStatus::Unverified,
                    VerificationStatus::Inferred,
                    VerificationStatus::SourceVerified,
                    VerificationStatus::HardwareVerified,
                    VerificationStatus::SourceAndHardwareVerified,
                ] {
                    let mut settings = DeviceSettingsState::default();
                    settings.submission(
                        id,
                        value,
                        SettingSubmissionOutcome::Accepted,
                        SettingCompletionStrategy::MatchingReadback,
                        time(10),
                    );
                    let measured = Measured {
                        value,
                        source,
                        quality,
                        verification,
                    };
                    settings.observe_measured(id, measured, time(20));
                    let snapshot = settings.snapshot(time(30));
                    assert_eq!(snapshot[0].measured, Some(measured));
                    assert_eq!(snapshot[0].age.unwrap().as_milliseconds(), 10);
                    let confirms = source == ValueSource::Reported
                        && quality == ValueQuality::Known
                        && matches!(
                            verification,
                            VerificationStatus::SourceVerified
                                | VerificationStatus::HardwareVerified
                                | VerificationStatus::SourceAndHardwareVerified
                        );
                    assert_eq!(
                        snapshot[0].status,
                        if confirms {
                            SettingCommandStatus::Confirmed
                        } else {
                            SettingCommandStatus::WaitingForConfirmation
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn stale_readback_cannot_replace_newer_evidence() {
        use crate::Measured;

        let mut settings = DeviceSettingsState::default();
        let id = SettingId::PwmTiltback;
        let current = Measured::reported(DeviceSettingValue::Number(80));
        settings.observe_measured(id, current, time(20));
        settings.observe_measured(
            id,
            Measured::estimated(DeviceSettingValue::Number(60)),
            time(19),
        );
        assert_eq!(settings.snapshot(time(30))[0].measured, Some(current));
        assert_eq!(
            settings.snapshot(time(30))[0]
                .age
                .unwrap()
                .as_milliseconds(),
            10
        );
        settings.disconnect();
        assert_eq!(settings.snapshot(time(30)).len(), 0);
    }

    #[test]
    fn explicit_unknown_invalidates_current_without_erasing_a_pending_request() {
        let mut settings = DeviceSettingsState::default();
        let id = SettingId::Headlight;
        settings.observe_measured(
            id,
            Measured::reported(DeviceSettingValue::Boolean(false)),
            time(10),
        );
        settings.submission(
            id,
            DeviceSettingValue::Boolean(true),
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::MatchingReadback,
            time(20),
        );
        settings.invalidate_readback(id, time(21));
        let snapshot = settings.snapshot(time(22));
        assert_eq!(snapshot[0].current, None);
        assert_eq!(snapshot[0].measured, None);
        assert_eq!(snapshot[0].age, None);
        assert_eq!(
            snapshot[0].requested,
            Some(DeviceSettingValue::Boolean(true))
        );
        assert_eq!(
            snapshot[0].status,
            SettingCommandStatus::WaitingForConfirmation
        );
        settings.observe_measured(
            id,
            Measured::reported(DeviceSettingValue::Boolean(true)),
            time(20),
        );
        assert_eq!(settings.snapshot(time(22))[0].current, None);
        settings.observe_measured(
            id,
            Measured::reported(DeviceSettingValue::Boolean(true)),
            time(23),
        );
        assert_eq!(
            settings.snapshot(time(23))[0].status,
            SettingCommandStatus::Confirmed
        );
        settings.invalidate_readback(id, time(24));
        assert_eq!(settings.snapshot(time(24))[0].current, None);
        assert_eq!(
            settings.snapshot(time(24))[0].status,
            SettingCommandStatus::Idle
        );
    }

    #[test]
    fn unknown_and_disabled_are_distinct_from_zero_and_false() {
        let mut settings = DeviceSettingsState::default();
        assert_eq!(settings.snapshot(time(0)).len(), 0);
        settings.observe(
            SettingId::PwmTiltback,
            DeviceSettingValue::Disabled,
            SettingValueSource::LiveReadback,
            time(1),
        );
        settings.observe(
            SettingId::Headlight,
            DeviceSettingValue::Boolean(false),
            SettingValueSource::LiveReadback,
            time(1),
        );
        let snapshot = settings.snapshot(time(11));
        assert_eq!(snapshot.len(), 2);
        let pwm = snapshot
            .iter()
            .find(|entry| entry.id == SettingId::PwmTiltback)
            .unwrap();
        assert_eq!(pwm.current.unwrap().value, DeviceSettingValue::Disabled);
        assert_eq!(pwm.age.unwrap().as_milliseconds(), 10);
        assert_eq!(pwm.status, SettingCommandStatus::Idle);
    }

    #[test]
    fn transport_evidence_is_separate_from_wheel_confirmation() {
        let mut settings = DeviceSettingsState::default();
        let id = SettingId::DisplayBrightness;
        settings.submission(
            id,
            DeviceSettingValue::Number(1),
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::MatchingReadback,
            time(10),
        );
        let accepted = settings.snapshot(time(10));
        assert_eq!(
            accepted[0].status,
            SettingCommandStatus::WaitingForConfirmation
        );
        assert_eq!(
            accepted[0].transport,
            Some(SettingTransportStatus::Accepted)
        );

        assert!(settings.transport(
            id,
            accepted[0].request_id.unwrap(),
            SettingTransportStatus::Submitted,
            time(20),
        ));
        let submitted = settings.snapshot(time(20));
        assert_eq!(
            submitted[0].status,
            SettingCommandStatus::WaitingForConfirmation
        );
        assert_eq!(
            submitted[0].transport,
            Some(SettingTransportStatus::Submitted)
        );
    }

    #[test]
    fn managed_transport_waits_for_handoff_before_confirmation_or_timeout() {
        for completion in [
            SettingCompletionStrategy::MatchingReadback,
            SettingCompletionStrategy::SubmissionOnly,
        ] {
            let mut settings = DeviceSettingsState::default();
            settings.require_managed_transport();
            let id = SettingId::DisplayBrightness;
            let value = DeviceSettingValue::Number(1);
            settings.submission(
                id,
                value,
                SettingSubmissionOutcome::Accepted,
                completion,
                time(10),
            );
            let request_id = settings.snapshot(time(10))[0].request_id.unwrap();
            for (at, status) in [
                (3_000, SettingTransportStatus::Accepted),
                (6_000, SettingTransportStatus::Queued),
            ] {
                if status == SettingTransportStatus::Queued {
                    assert!(settings.transport(id, request_id, status, time(at)));
                }
                settings.observe(id, value, SettingValueSource::LiveReadback, time(at));
                settings.tick(time(at));
                let snapshot = settings.snapshot(time(at))[0];
                assert_eq!(snapshot.current.unwrap().value, value);
                assert_eq!(snapshot.requested, Some(value));
                assert_eq!(
                    snapshot.status,
                    SettingCommandStatus::WaitingForConfirmation
                );
                assert_eq!(snapshot.transport, Some(status));
            }
            assert!(settings.transport(
                id,
                request_id,
                SettingTransportStatus::Submitted,
                time(10_000)
            ));
            settings.observe(id, value, SettingValueSource::LiveReadback, time(9_999));
            settings.tick(time(11_999));
            assert_eq!(
                settings.snapshot(time(11_999))[0].status,
                if completion.supports_readback() {
                    SettingCommandStatus::WaitingForConfirmation
                } else {
                    SettingCommandStatus::SentWithoutConfirmation
                }
            );
            let mut confirming = settings.clone();
            confirming.observe(id, value, SettingValueSource::LiveReadback, time(11_999));
            assert_eq!(
                confirming.snapshot(time(11_999))[0].status,
                if completion.supports_readback() {
                    SettingCommandStatus::Confirmed
                } else {
                    SettingCommandStatus::SentWithoutConfirmation
                }
            );
            settings.tick(time(12_000));
            assert_eq!(
                settings.snapshot(time(12_000))[0].status,
                if completion.supports_readback() {
                    SettingCommandStatus::TimedOut
                } else {
                    SettingCommandStatus::SentWithoutConfirmation
                }
            );
        }
    }

    #[test]
    fn transport_accepts_only_forward_transitions_for_the_latest_request() {
        use SettingTransportStatus::{Accepted, Cancelled, Queued, Rejected, Submitted};
        for from in [Accepted, Queued, Submitted, Rejected] {
            for to in [Accepted, Queued, Submitted, Rejected, Cancelled] {
                let mut settings = DeviceSettingsState::default();
                settings.require_managed_transport();
                let id = SettingId::Headlight;
                settings.submission(
                    id,
                    DeviceSettingValue::Boolean(true),
                    SettingSubmissionOutcome::Accepted,
                    SettingCompletionStrategy::MatchingReadback,
                    time(10),
                );
                let request_id = settings.snapshot(time(10))[0].request_id.unwrap();
                if from != Accepted {
                    assert!(settings.transport(id, request_id, from, time(20)));
                }
                let before = settings.snapshot(time(30));
                let allowed = matches!(
                    (from, to),
                    (Accepted, Queued | Submitted | Rejected | Cancelled)
                        | (Queued, Submitted | Rejected | Cancelled)
                );
                assert_eq!(
                    settings.transport(id, request_id, to, time(30)),
                    allowed,
                    "{from:?} -> {to:?}"
                );
                if !allowed {
                    assert_eq!(settings.snapshot(time(30)), before);
                } else if matches!(to, Rejected | Cancelled) {
                    assert_eq!(
                        settings.snapshot(time(30))[0].status,
                        SettingCommandStatus::Failed
                    );
                    assert_eq!(
                        settings.snapshot(time(30))[0].requested,
                        Some(DeviceSettingValue::Boolean(true))
                    );
                }
            }
        }
        let mut settings = DeviceSettingsState::default();
        settings.require_managed_transport();
        let id = SettingId::Headlight;
        let mut previous_id = 0;
        for reset in [false, false, true] {
            if reset {
                settings.disconnect();
            }
            settings.submission(
                id,
                DeviceSettingValue::Boolean(true),
                SettingSubmissionOutcome::Accepted,
                SettingCompletionStrategy::MatchingReadback,
                time(10),
            );
            let before = settings.snapshot(time(10));
            let request_id = before[0].request_id.unwrap();
            assert!(request_id > previous_id);
            assert!(!settings.transport(id, previous_id, Submitted, time(20)));
            assert!(!settings.transport(SettingId::HighBeam, request_id, Submitted, time(20)));
            assert!(!settings.transport(id, request_id, Submitted, time(9)));
            assert_eq!(settings.snapshot(time(10)), before);
            previous_id = request_id;
        }
    }

    #[test]
    fn a_refused_request_preserves_current_and_a_retry_can_confirm() {
        let mut settings = DeviceSettingsState::default();
        let id = SettingId::Headlight;
        settings.observe(
            id,
            DeviceSettingValue::Boolean(false),
            SettingValueSource::LiveReadback,
            time(10),
        );
        settings.submission(
            id,
            DeviceSettingValue::Boolean(true),
            SettingSubmissionOutcome::Refused(ControlRefusalReason::Busy),
            SettingCompletionStrategy::MatchingReadback,
            time(20),
        );
        let snapshot = settings.snapshot(time(25));
        assert_eq!(
            snapshot[0].current.unwrap().value,
            DeviceSettingValue::Boolean(false)
        );
        assert_eq!(
            snapshot[0].requested,
            Some(DeviceSettingValue::Boolean(true))
        );
        assert_eq!(snapshot[0].status, SettingCommandStatus::Refused);
        assert_eq!(snapshot[0].refusal, Some(ControlRefusalReason::Busy));
        settings.submission(
            id,
            DeviceSettingValue::Boolean(true),
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::MatchingReadback,
            time(30),
        );
        settings.observe(
            id,
            DeviceSettingValue::Boolean(true),
            SettingValueSource::LiveReadback,
            time(29),
        );
        assert_eq!(
            settings.snapshot(time(31))[0].status,
            SettingCommandStatus::WaitingForConfirmation
        );
        settings.observe(
            id,
            DeviceSettingValue::Boolean(true),
            SettingValueSource::LiveReadback,
            time(32),
        );
        assert_eq!(
            settings.snapshot(time(32))[0].status,
            SettingCommandStatus::Confirmed
        );
    }

    #[test]
    fn submission_only_readback_does_not_confirm_a_write() {
        let mut settings = DeviceSettingsState::default();
        let id = SettingId::Headlight;
        settings.submission(
            id,
            DeviceSettingValue::Boolean(true),
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::SubmissionOnly,
            time(10),
        );
        settings.observe(
            id,
            DeviceSettingValue::Boolean(true),
            SettingValueSource::LiveReadback,
            time(20),
        );
        let snapshot = settings.snapshot(time(25));
        assert_eq!(
            snapshot[0].current.unwrap().value,
            DeviceSettingValue::Boolean(true)
        );
        assert_eq!(snapshot[0].age, Some(Duration::from_milliseconds(5)));
        assert_eq!(
            snapshot[0].requested,
            Some(DeviceSettingValue::Boolean(true))
        );
        assert_eq!(
            snapshot[0].status,
            SettingCommandStatus::SentWithoutConfirmation
        );
        settings.tick(time(10_000));
        assert_eq!(
            settings.snapshot(time(10_000))[0].status,
            SettingCommandStatus::SentWithoutConfirmation
        );
    }

    #[test]
    fn unconfirmed_writes_do_not_become_current_or_time_out() {
        let mut settings = DeviceSettingsState::default();
        settings.submission(
            SettingId::MaximumSpeed,
            DeviceSettingValue::Number(30),
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::SubmissionOnly,
            time(10),
        );
        settings.tick(time(5_000));
        let snapshot = settings.snapshot(time(5_000));
        assert_eq!(
            snapshot[0].status,
            SettingCommandStatus::SentWithoutConfirmation
        );
        assert_eq!(snapshot[0].current, None);
        assert_eq!(snapshot[0].age, None);
        assert_eq!(snapshot[0].requested, Some(DeviceSettingValue::Number(30)));
    }

    #[test]
    fn failed_retry_records_the_new_request_and_disconnect_clears_everything() {
        let mut settings = DeviceSettingsState::default();
        settings.submission(
            SettingId::MaximumSpeed,
            DeviceSettingValue::Number(30),
            SettingSubmissionOutcome::Accepted,
            SettingCompletionStrategy::SubmissionOnly,
            time(10),
        );
        settings.submission(
            SettingId::MaximumSpeed,
            DeviceSettingValue::Number(40),
            SettingSubmissionOutcome::Failed,
            SettingCompletionStrategy::SubmissionOnly,
            time(20),
        );
        let snapshot = settings.snapshot(time(20));
        assert_eq!(snapshot[0].requested, Some(DeviceSettingValue::Number(40)));
        assert_eq!(snapshot[0].status, SettingCommandStatus::Failed);
        settings.disconnect();
        assert_eq!(settings.snapshot(time(30)).len(), 0);
    }

    #[test]
    fn out_of_order_readback_cannot_replace_newer_state_or_refresh_its_age() {
        let mut settings = DeviceSettingsState::default();
        settings.observe(
            SettingId::Headlight,
            DeviceSettingValue::Boolean(true),
            SettingValueSource::LiveReadback,
            time(20),
        );
        settings.observe(
            SettingId::Headlight,
            DeviceSettingValue::Boolean(false),
            SettingValueSource::LiveReadback,
            time(10),
        );
        let snapshot = settings.snapshot(time(30));
        assert_eq!(
            snapshot[0].current.unwrap().value,
            DeviceSettingValue::Boolean(true)
        );
        assert_eq!(snapshot[0].age.unwrap().as_milliseconds(), 10);
    }
}
