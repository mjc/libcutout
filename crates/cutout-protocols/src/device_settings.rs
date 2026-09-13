//! Shared device settings definitions and semantic command validation.

mod readback;
pub use readback::SettingObservation;

use cutout_core::{
    AccelerationAssistState, AeroAngleAdjustment, AeroBeeperVolume, AeroBrakeOverpressureAlarm,
    AeroDisplayBacklight, AeroDynamicAssist, AeroHighSpeedMode, AeroLateralTiltLimit,
    AeroLowBatteryMode, AeroPedalDipCompensation, AeroPedalHardness, AeroPwmPercent,
    AeroPwmSetting, AeroRidingMode, AeroSpeedSetting, AeroTransportMode, AeroVoltageCorrection,
    AeroWheelUnits, BegodeBeeperVolume, BegodeLedModeSetting, BegodeMaxSpeed, Capabilities,
    CommandKind, DeviceCommand, DeviceSettingValue, LightState, PedalMode, RollAngle, SettingId,
    SpeedAlarmMode,
};

use crate::{BegodeFalconModel, SupportsBenignControls, SupportsSettingsWrites};

/// Meaning of a fixed-point numeric value before native display-unit conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingUnit {
    /// Percentage, including PWM utilization (never remaining headroom).
    Percent,
    /// Speed in kilometres per hour.
    KilometresPerHour,
    /// Angle in degrees.
    Degrees,
    /// Device-defined ordinal level, with no invented physical units.
    Level,
}

/// Semantic grouping and stable ordering independent of device names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingGroup {
    /// Alarms and operating limits.
    Limits,
    /// Pedal response and assistance.
    Riding,
    /// Lights, sound, and wheel display.
    Interface,
    /// Special operating modes.
    Modes,
    /// Values whose physical meaning is not established.
    Diagnostics,
}

/// A readable option may be unavailable for writing on the current protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingChoice {
    /// Stable semantic option identifier, not a wire byte.
    pub id: u16,
    /// Shared localization catalog key.
    pub label_key: &'static str,
    /// Whether a submission can select this option.
    pub writable: bool,
}

/// Native-neutral control definition. Unknown current values are separate state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingControl {
    /// Explicit on/off selection.
    Boolean,
    /// A bounded fixed-point numeric quantity.
    Number {
        /// Inclusive minimum in fixed-point units.
        minimum: i32,
        /// Inclusive maximum in fixed-point units.
        maximum: i32,
        /// Increment in fixed-point units.
        step: i32,
        /// Decimal places in the physical value.
        precision: u8,
        /// Physical or ordinal quantity.
        unit: SettingUnit,
        /// A separate disabled selection is supported.
        can_disable: bool,
    },
    /// Closed readable option set with individual write availability.
    Choices(Vec<SettingChoice>),
    /// Diagnostic value with no permitted submission.
    ReadOnly,
}

impl SettingControl {
    fn accepts(&self, value: DeviceSettingValue) -> bool {
        match (self, value) {
            (Self::Boolean, DeviceSettingValue::Boolean(_)) => true,
            (
                Self::Number {
                    minimum,
                    maximum,
                    step,
                    ..
                },
                DeviceSettingValue::Number(value),
            ) => (*minimum..=*maximum).contains(&value) && (value - minimum) % step == 0,
            (Self::Number { can_disable, .. }, DeviceSettingValue::Disabled) => *can_disable,
            (Self::Choices(choices), DeviceSettingValue::Choice(id)) => choices
                .iter()
                .any(|choice| choice.id == id && choice.writable),
            _ => false,
        }
    }
}

/// Static write availability. The session also checks live conditions at execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingAccess {
    /// Protocol write is verified, or explicit validation mode allows testing it.
    Writable,
    /// A candidate encoder exists but has not been verified.
    Unverified,
    /// Physical meaning or write semantics are unresolved.
    ReadOnly,
}

/// Shared setting definition consumed by every native settings renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingDescriptor {
    /// Stable semantic identity.
    pub id: SettingId,
    /// Shared localization catalog key for the label.
    pub label_key: &'static str,
    /// Semantic section.
    pub group: SettingGroup,
    /// Stable display order within the catalog.
    pub order: u16,
    /// Control and allowed value shape.
    pub control: SettingControl,
    /// Static availability in the selected validation mode.
    pub access: SettingAccess,
    /// Whether device readback can confirm a submitted value.
    pub confirmation_supported: bool,
}

/// A semantic request rejected before it reaches the live session checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SettingsRequestError {
    /// No setting exists in this profile for the requested identifier.
    #[error("setting unavailable")]
    Unavailable,
    /// The descriptor is diagnostic-only, even in validation mode.
    #[error("setting is read-only")]
    ReadOnly,
    /// The candidate write requires explicit validation mode.
    #[error("setting write is unverified")]
    Unverified,
    /// Wrong value shape, out-of-range number, or non-writable choice.
    #[error("invalid setting value")]
    InvalidValue,
}

/// Capability selection over shared settings and actions, without client model switches.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceControlProfile {
    pub(crate) available: Capabilities,
    pub(crate) verified: Capabilities,
    confirmation: Capabilities,
}

impl DeviceControlProfile {
    /// Selects available encoders, verified writes, and writes with usable confirmation.
    #[must_use]
    pub const fn new(
        available: Capabilities,
        verified: Capabilities,
        confirmation: Capabilities,
    ) -> Self {
        Self {
            available,
            verified,
            confirmation,
        }
    }

    /// Returns supported controls, retaining diagnostic-only and unverified definitions.
    #[must_use]
    pub fn descriptors(self, validation_mode: bool) -> Vec<SettingDescriptor> {
        CATALOG
            .iter()
            .filter_map(|&(id, label_key, group, order)| {
                let kind = command_kind(id);
                if !self.available.supports_command_kind(kind) {
                    return None;
                }
                let mut control = control(id);
                if let SettingControl::Number { can_disable, .. } = &mut control {
                    *can_disable = id == SettingId::PwmTiltback
                        && self
                            .available
                            .supports_command_kind(CommandKind::SetAeroPwmOff)
                        && (validation_mode
                            || self
                                .verified
                                .supports_command_kind(CommandKind::SetAeroPwmOff));
                }
                Some(SettingDescriptor {
                    id,
                    label_key,
                    group,
                    order,
                    control,
                    access: if id == SettingId::ChargeLimitDiagnostic {
                        SettingAccess::ReadOnly
                    } else if validation_mode || self.verified.supports_command_kind(kind) {
                        SettingAccess::Writable
                    } else {
                        SettingAccess::Unverified
                    },
                    confirmation_supported: self.confirmation.supports_command_kind(kind)
                        && id != SettingId::ChargeLimitDiagnostic,
                })
            })
            .collect()
    }

    /// Resolves a semantic request into an existing checked domain command.
    ///
    /// # Errors
    /// Rejects unavailable, read-only, unverified, or invalid values. This does not
    /// authorize transport: the session must revalidate identity and live conditions.
    pub fn command(
        self,
        id: SettingId,
        value: DeviceSettingValue,
        validation_mode: bool,
    ) -> Result<DeviceCommand, SettingsRequestError> {
        let descriptor = self
            .descriptors(validation_mode)
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or(SettingsRequestError::Unavailable)?;
        match descriptor.access {
            SettingAccess::ReadOnly => return Err(SettingsRequestError::ReadOnly),
            SettingAccess::Unverified => return Err(SettingsRequestError::Unverified),
            SettingAccess::Writable => {}
        }
        if !descriptor.control.accepts(value) {
            return Err(SettingsRequestError::InvalidValue);
        }
        checked_command(id, value).ok_or(SettingsRequestError::InvalidValue)
    }
}

/// Aero controls retain explicit per-command verification; charge raw stays diagnostic-only.
#[must_use]
pub const fn aero_control_profile() -> DeviceControlProfile {
    DeviceControlProfile::new(
        Capabilities::from_supported_commands([
            CommandKind::SetAeroHighBeam,
            CommandKind::SetAeroTiltbackSpeed,
            CommandKind::SetAeroPwmPercent,
            CommandKind::SetAeroPwmOff,
            CommandKind::SetAeroRidingMode,
            CommandKind::SetAeroBrakeOverpressureAlarm,
            CommandKind::SetAeroPedalHardness,
            CommandKind::SetAeroDisplayBacklight,
            CommandKind::SetAeroBeeperVolume,
            CommandKind::SetAeroDynamicAssist,
            CommandKind::SetAeroPedalDipCompensation,
            CommandKind::SetAeroLateralTiltLimit,
            CommandKind::SetAeroVoltageCorrection,
            CommandKind::SetAeroMaxChargeVoltageRaw,
            CommandKind::SetAeroWheelUnits,
            CommandKind::SetAeroHighSpeedMode,
            CommandKind::SetAeroLowBatteryMode,
            CommandKind::SetAeroTransportMode,
            CommandKind::SetAeroAlarmSpeed,
            CommandKind::SetAeroAngleAdjustment,
            CommandKind::SoundHorn,
            CommandKind::ResetTripMeter,
            CommandKind::SetAeroGyroCalibration,
        ]),
        Capabilities::from_supported_commands([
            CommandKind::SetAeroHighBeam,
            CommandKind::SoundHorn,
        ]),
        Capabilities::from_supported_commands([
            CommandKind::SetAeroTiltbackSpeed,
            CommandKind::SetAeroAlarmSpeed,
            CommandKind::SetAeroPwmPercent,
            CommandKind::SetAeroPedalHardness,
            CommandKind::SetAeroDisplayBacklight,
            CommandKind::SetAeroWheelUnits,
            CommandKind::SetAeroBeeperVolume,
            CommandKind::SetAeroDynamicAssist,
            CommandKind::SetAeroPedalDipCompensation,
            CommandKind::SetAeroLateralTiltLimit,
            CommandKind::SetAeroVoltageCorrection,
            CommandKind::SetAeroHighSpeedMode,
            CommandKind::SetAeroLowBatteryMode,
            CommandKind::SetAeroTransportMode,
            CommandKind::SetAeroBrakeOverpressureAlarm,
        ]),
    )
}

/// Falcon writes with no readable acknowledgement remain explicitly unconfirmed.
#[must_use]
pub const fn falcon_control_profile() -> DeviceControlProfile {
    let available =
        BegodeFalconModel::WRITE_CAPABILITIES.union(BegodeFalconModel::CONTROL_CAPABILITIES);
    DeviceControlProfile::new(
        available,
        available,
        Capabilities::from_supported_commands([
            CommandKind::SetPedalMode,
            CommandKind::SetRollAngle,
            CommandKind::SetSpeedAlarmMode,
        ]),
    )
}

const CATALOG: &[(SettingId, &str, SettingGroup, u16)] = &[
    (
        SettingId::Headlight,
        "settings.headlight.label",
        SettingGroup::Interface,
        0,
    ),
    (
        SettingId::HighBeam,
        "settings.high_beam.label",
        SettingGroup::Interface,
        1,
    ),
    (
        SettingId::TiltbackSpeed,
        "settings.tiltback_speed.label",
        SettingGroup::Limits,
        2,
    ),
    (
        SettingId::PwmTiltback,
        "settings.pwm_tiltback.label",
        SettingGroup::Limits,
        3,
    ),
    (
        SettingId::PedalHardness,
        "settings.pedal_hardness.label",
        SettingGroup::Riding,
        4,
    ),
    (
        SettingId::DisplayBrightness,
        "settings.display_brightness.label",
        SettingGroup::Interface,
        5,
    ),
    (
        SettingId::DisplayUnits,
        "settings.display_units.label",
        SettingGroup::Interface,
        6,
    ),
    (
        SettingId::BeeperVolumePercent,
        "settings.beeper_volume_percent.label",
        SettingGroup::Interface,
        7,
    ),
    (
        SettingId::DynamicAssist,
        "settings.dynamic_assist.label",
        SettingGroup::Riding,
        8,
    ),
    (
        SettingId::PedalDipCompensation,
        "settings.pedal_dip_compensation.label",
        SettingGroup::Riding,
        9,
    ),
    (
        SettingId::LateralTiltLimit,
        "settings.lateral_tilt_limit.label",
        SettingGroup::Limits,
        10,
    ),
    (
        SettingId::VoltageCorrection,
        "settings.voltage_correction.label",
        SettingGroup::Modes,
        11,
    ),
    (
        SettingId::ChargeLimitDiagnostic,
        "settings.charge_limit_diagnostic.label",
        SettingGroup::Diagnostics,
        12,
    ),
    (
        SettingId::HighSpeedMode,
        "settings.high_speed_mode.label",
        SettingGroup::Modes,
        13,
    ),
    (
        SettingId::LowBatteryMode,
        "settings.low_battery_mode.label",
        SettingGroup::Modes,
        14,
    ),
    (
        SettingId::TransportMode,
        "settings.transport_mode.label",
        SettingGroup::Modes,
        15,
    ),
    (
        SettingId::SpeedAlarmThreshold,
        "settings.speed_alarm_threshold.label",
        SettingGroup::Limits,
        16,
    ),
    (
        SettingId::PedalAngle,
        "settings.pedal_angle.label",
        SettingGroup::Riding,
        17,
    ),
    (
        SettingId::RidingPreset,
        "settings.riding_preset.label",
        SettingGroup::Riding,
        18,
    ),
    (
        SettingId::BrakeOverpressureAlarm,
        "settings.brake_overpressure_alarm.label",
        SettingGroup::Limits,
        19,
    ),
    (
        SettingId::PedalMode,
        "settings.pedal_mode.label",
        SettingGroup::Riding,
        20,
    ),
    (
        SettingId::RollAngleMode,
        "settings.roll_angle_mode.label",
        SettingGroup::Riding,
        21,
    ),
    (
        SettingId::SpeedAlarmMode,
        "settings.speed_alarm_mode.label",
        SettingGroup::Limits,
        22,
    ),
    (
        SettingId::MaximumSpeed,
        "settings.maximum_speed.label",
        SettingGroup::Limits,
        23,
    ),
    (
        SettingId::BeeperVolumeLevel,
        "settings.beeper_volume_level.label",
        SettingGroup::Interface,
        24,
    ),
    (
        SettingId::LightingPattern,
        "settings.lighting_pattern.label",
        SettingGroup::Interface,
        25,
    ),
    (
        SettingId::AccelerationAssist,
        "settings.acceleration_assist.label",
        SettingGroup::Riding,
        26,
    ),
    (
        SettingId::Taillight,
        "settings.taillight.label",
        SettingGroup::Interface,
        27,
    ),
];

const fn command_kind(id: SettingId) -> CommandKind {
    match id {
        SettingId::Headlight => CommandKind::SetLights,
        SettingId::HighBeam => CommandKind::SetAeroHighBeam,
        SettingId::TiltbackSpeed => CommandKind::SetAeroTiltbackSpeed,
        SettingId::PwmTiltback => CommandKind::SetAeroPwmPercent,
        SettingId::PedalHardness => CommandKind::SetAeroPedalHardness,
        SettingId::DisplayBrightness => CommandKind::SetAeroDisplayBacklight,
        SettingId::DisplayUnits => CommandKind::SetAeroWheelUnits,
        SettingId::BeeperVolumePercent => CommandKind::SetAeroBeeperVolume,
        SettingId::DynamicAssist => CommandKind::SetAeroDynamicAssist,
        SettingId::PedalDipCompensation => CommandKind::SetAeroPedalDipCompensation,
        SettingId::LateralTiltLimit => CommandKind::SetAeroLateralTiltLimit,
        SettingId::VoltageCorrection => CommandKind::SetAeroVoltageCorrection,
        SettingId::ChargeLimitDiagnostic => CommandKind::SetAeroMaxChargeVoltageRaw,
        SettingId::HighSpeedMode => CommandKind::SetAeroHighSpeedMode,
        SettingId::LowBatteryMode => CommandKind::SetAeroLowBatteryMode,
        SettingId::TransportMode => CommandKind::SetAeroTransportMode,
        SettingId::SpeedAlarmThreshold => CommandKind::SetAeroAlarmSpeed,
        SettingId::PedalAngle => CommandKind::SetAeroAngleAdjustment,
        SettingId::RidingPreset => CommandKind::SetAeroRidingMode,
        SettingId::BrakeOverpressureAlarm => CommandKind::SetAeroBrakeOverpressureAlarm,
        SettingId::PedalMode => CommandKind::SetPedalMode,
        SettingId::RollAngleMode => CommandKind::SetRollAngle,
        SettingId::SpeedAlarmMode => CommandKind::SetSpeedAlarmMode,
        SettingId::MaximumSpeed => CommandKind::SetBegodeMaxSpeed,
        SettingId::BeeperVolumeLevel => CommandKind::SetBegodeBeeperVolume,
        SettingId::LightingPattern => CommandKind::SetBegodeLedMode,
        SettingId::AccelerationAssist => CommandKind::SetAccelerationAssist,
        SettingId::Taillight => CommandKind::SetTaillight,
    }
}

fn number(minimum: i32, maximum: i32, precision: u8, unit: SettingUnit) -> SettingControl {
    SettingControl::Number {
        minimum,
        maximum,
        step: 1,
        precision,
        unit,
        can_disable: false,
    }
}

fn speed_control(minimum: i32, maximum: i32) -> SettingControl {
    SettingControl::Number {
        minimum: minimum * 10,
        maximum: maximum * 10,
        step: 10,
        precision: 1,
        unit: SettingUnit::KilometresPerHour,
        can_disable: false,
    }
}

fn choices(entries: &[(u16, &'static str, bool)]) -> SettingControl {
    SettingControl::Choices(
        entries
            .iter()
            .map(|&(id, label_key, writable)| SettingChoice {
                id,
                label_key,
                writable,
            })
            .collect(),
    )
}

fn control(id: SettingId) -> SettingControl {
    match id {
        SettingId::Headlight
        | SettingId::HighBeam
        | SettingId::HighSpeedMode
        | SettingId::LowBatteryMode
        | SettingId::TransportMode
        | SettingId::AccelerationAssist
        | SettingId::Taillight => SettingControl::Boolean,
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold => speed_control(10, 200),
        SettingId::PwmTiltback => number(30, 100, 0, SettingUnit::Percent),
        SettingId::PedalHardness
        | SettingId::DisplayBrightness
        | SettingId::BeeperVolumePercent
        | SettingId::DynamicAssist
        | SettingId::PedalDipCompensation => number(0, 100, 0, SettingUnit::Percent),
        SettingId::LateralTiltLimit => number(35, 75, 0, SettingUnit::Degrees),
        SettingId::VoltageCorrection => number(-15, 15, 1, SettingUnit::Percent),
        SettingId::PedalAngle => number(-80, 80, 1, SettingUnit::Degrees),
        SettingId::BrakeOverpressureAlarm => number(90, 125, 0, SettingUnit::Percent),
        SettingId::MaximumSpeed => speed_control(0, 99),
        SettingId::BeeperVolumeLevel => number(1, 9, 0, SettingUnit::Level),
        SettingId::LightingPattern => choices(&[
            (0, "settings.choice.pattern_0", true),
            (1, "settings.choice.pattern_1", true),
            (2, "settings.choice.pattern_2", true),
            (3, "settings.choice.pattern_3", true),
            (4, "settings.choice.pattern_4", true),
            (5, "settings.choice.pattern_5", true),
            (6, "settings.choice.pattern_6", true),
            (7, "settings.choice.pattern_7", true),
            (8, "settings.choice.pattern_8", true),
            (9, "settings.choice.pattern_9", true),
        ]),
        SettingId::DisplayUnits => choices(&[
            (0, "settings.choice.metric", true),
            (1, "settings.choice.imperial", true),
        ]),
        SettingId::RidingPreset | SettingId::PedalMode => choices(&[
            (0, "settings.choice.hard", true),
            (1, "settings.choice.medium", true),
            (2, "settings.choice.soft", true),
        ]),
        SettingId::RollAngleMode => choices(&[
            (0, "settings.choice.low", true),
            (1, "settings.choice.medium", true),
            (2, "settings.choice.high", true),
        ]),
        SettingId::SpeedAlarmMode => choices(&[
            (0, "settings.choice.both_alarm_stages", true),
            (1, "settings.choice.first_alarm_stage", true),
            (2, "settings.choice.off", false),
            (3, "settings.choice.pwm_tiltback", false),
        ]),
        SettingId::ChargeLimitDiagnostic => SettingControl::ReadOnly,
    }
}

fn checked_number<T, U: TryFrom<i32>>(
    value: i32,
    constructor: impl FnOnce(U) -> Option<T>,
) -> Option<T> {
    U::try_from(value).ok().and_then(constructor)
}

fn checked_command(id: SettingId, value: DeviceSettingValue) -> Option<DeviceCommand> {
    match value {
        DeviceSettingValue::Number(value) => numeric_command(id, value),
        DeviceSettingValue::Choice(value) => choice_command(id, value),
        DeviceSettingValue::Boolean(on) => boolean_command(id, on),
        DeviceSettingValue::Disabled if id == SettingId::PwmTiltback => {
            Some(DeviceCommand::SetAeroPwmPercent(AeroPwmSetting::Off))
        }
        DeviceSettingValue::Disabled => None,
    }
}

fn boolean_command(id: SettingId, on: bool) -> Option<DeviceCommand> {
    match id {
        SettingId::Headlight => Some(DeviceCommand::SetLights(light(on))),
        SettingId::HighBeam => Some(DeviceCommand::SetAeroHighBeam(light(on))),
        SettingId::Taillight => Some(DeviceCommand::SetTaillight(light(on))),
        SettingId::HighSpeedMode => Some(DeviceCommand::SetAeroHighSpeedMode(
            AeroHighSpeedMode::new(on),
        )),
        SettingId::LowBatteryMode => Some(DeviceCommand::SetAeroLowBatteryMode(
            AeroLowBatteryMode::new(on),
        )),
        SettingId::TransportMode => Some(DeviceCommand::SetAeroTransportMode(
            AeroTransportMode::new(on),
        )),
        SettingId::AccelerationAssist => Some(DeviceCommand::SetAccelerationAssist(if on {
            AccelerationAssistState::Enabled
        } else {
            AccelerationAssistState::Disabled
        })),
        _ => None,
    }
}

fn numeric_command(id: SettingId, value: i32) -> Option<DeviceCommand> {
    let value = if matches!(
        id,
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold | SettingId::MaximumSpeed
    ) {
        if value % 10 != 0 {
            return None;
        }
        value / 10
    } else {
        value
    };
    match id {
        SettingId::PwmTiltback => checked_number(value, AeroPwmPercent::from_duty_percent)
            .map(|pwm| DeviceCommand::SetAeroPwmPercent(pwm.into())),
        SettingId::TiltbackSpeed => {
            checked_number(value, AeroSpeedSetting::new).map(DeviceCommand::SetAeroTiltbackSpeed)
        }
        SettingId::SpeedAlarmThreshold => {
            checked_number(value, AeroSpeedSetting::new).map(DeviceCommand::SetAeroAlarmSpeed)
        }
        SettingId::PedalHardness => {
            checked_number(value, AeroPedalHardness::new).map(DeviceCommand::SetAeroPedalHardness)
        }
        SettingId::DisplayBrightness => checked_number(value, AeroDisplayBacklight::new)
            .map(DeviceCommand::SetAeroDisplayBacklight),
        SettingId::BeeperVolumePercent => {
            checked_number(value, AeroBeeperVolume::new).map(DeviceCommand::SetAeroBeeperVolume)
        }
        SettingId::DynamicAssist => {
            checked_number(value, AeroDynamicAssist::new).map(DeviceCommand::SetAeroDynamicAssist)
        }
        SettingId::PedalDipCompensation => checked_number(value, AeroPedalDipCompensation::new)
            .map(DeviceCommand::SetAeroPedalDipCompensation),
        SettingId::LateralTiltLimit => checked_number(value, AeroLateralTiltLimit::new)
            .map(DeviceCommand::SetAeroLateralTiltLimit),
        SettingId::VoltageCorrection => checked_number(value, AeroVoltageCorrection::new)
            .map(DeviceCommand::SetAeroVoltageCorrection),
        SettingId::PedalAngle => checked_number(value, AeroAngleAdjustment::new)
            .map(DeviceCommand::SetAeroAngleAdjustment),
        SettingId::BrakeOverpressureAlarm => checked_number(value, AeroBrakeOverpressureAlarm::new)
            .map(DeviceCommand::SetAeroBrakeOverpressureAlarm),
        SettingId::MaximumSpeed => {
            checked_number(value, BegodeMaxSpeed::new).map(DeviceCommand::SetBegodeMaxSpeed)
        }
        SettingId::BeeperVolumeLevel => {
            checked_number(value, BegodeBeeperVolume::new).map(DeviceCommand::SetBegodeBeeperVolume)
        }
        _ => None,
    }
}

fn choice_command(id: SettingId, value: u16) -> Option<DeviceCommand> {
    match id {
        SettingId::LightingPattern => u8::try_from(value)
            .ok()
            .and_then(BegodeLedModeSetting::new)
            .map(DeviceCommand::SetBegodeLedMode),
        SettingId::DisplayUnits => match value {
            0 => Some(DeviceCommand::SetAeroWheelUnits(AeroWheelUnits::Metric)),
            1 => Some(DeviceCommand::SetAeroWheelUnits(AeroWheelUnits::Imperial)),
            _ => None,
        },
        SettingId::RidingPreset => match value {
            0 => Some(DeviceCommand::SetAeroRidingMode(AeroRidingMode::Hard)),
            1 => Some(DeviceCommand::SetAeroRidingMode(AeroRidingMode::Medium)),
            2 => Some(DeviceCommand::SetAeroRidingMode(AeroRidingMode::Soft)),
            _ => None,
        },
        SettingId::PedalMode => match value {
            0 => Some(DeviceCommand::SetPedalMode(PedalMode::Hard)),
            1 => Some(DeviceCommand::SetPedalMode(PedalMode::Medium)),
            2 => Some(DeviceCommand::SetPedalMode(PedalMode::Soft)),
            _ => None,
        },
        SettingId::RollAngleMode => match value {
            0 => Some(DeviceCommand::SetRollAngle(RollAngle::Low)),
            1 => Some(DeviceCommand::SetRollAngle(RollAngle::Medium)),
            2 => Some(DeviceCommand::SetRollAngle(RollAngle::High)),
            _ => None,
        },
        SettingId::SpeedAlarmMode => match value {
            0 => Some(DeviceCommand::SetSpeedAlarmMode(SpeedAlarmMode::Both)),
            1 => Some(DeviceCommand::SetSpeedAlarmMode(
                SpeedAlarmMode::StageOneOnly,
            )),
            _ => None,
        },
        _ => None,
    }
}

const fn light(on: bool) -> LightState {
    if on { LightState::On } else { LightState::Off }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{
        AeroPwmSetting, Capabilities, CommandKind, DeviceCommand, DeviceSettingValue, SettingId,
    };

    #[test]
    fn aero_profile_exposes_one_descriptor_per_physical_control() {
        let descriptors = aero_control_profile().descriptors(true);
        assert!(
            descriptors
                .iter()
                .any(|item| item.id == SettingId::HighBeam)
        );
        assert!(
            descriptors
                .iter()
                .any(|item| item.id == SettingId::RidingPreset)
        );
        assert!(
            !descriptors
                .iter()
                .any(|item| item.id == SettingId::Headlight)
        );
        assert!(
            !descriptors
                .iter()
                .any(|item| item.id == SettingId::PedalMode)
        );
    }

    #[test]
    fn confirmation_requires_a_positive_profile_capability() {
        let available = Capabilities::from_supported_commands([CommandKind::SetLights]);
        let unknown = DeviceControlProfile::new(available, available, Capabilities::default());
        assert!(!unknown.descriptors(false)[0].confirmation_supported);
        let aero = aero_control_profile().descriptors(true);
        for id in [
            SettingId::HighBeam,
            SettingId::PedalAngle,
            SettingId::RidingPreset,
            SettingId::ChargeLimitDiagnostic,
        ] {
            assert!(
                !aero
                    .iter()
                    .find(|entry| entry.id == id)
                    .unwrap()
                    .confirmation_supported,
                "{id:?}"
            );
        }
        assert!(
            aero.iter()
                .find(|entry| entry.id == SettingId::PwmTiltback)
                .unwrap()
                .confirmation_supported
        );
    }

    #[test]
    fn a_new_profile_uses_the_same_semantic_descriptors_and_submission() {
        let capabilities = Capabilities::from_supported_commands([
            CommandKind::SetLights,
            CommandKind::SetAeroPwmPercent,
            CommandKind::SetAeroPwmOff,
        ]);
        let profile =
            DeviceControlProfile::new(capabilities, capabilities, Capabilities::default());
        let descriptors = profile.descriptors(false);
        assert_eq!(descriptors.len(), 2);
        let pwm = descriptors
            .iter()
            .find(|entry| entry.id == SettingId::PwmTiltback)
            .unwrap();
        assert_eq!(pwm.label_key, "settings.pwm_tiltback.label");
        assert_eq!(
            profile
                .command(SettingId::PwmTiltback, DeviceSettingValue::Disabled, false)
                .unwrap(),
            DeviceCommand::SetAeroPwmPercent(AeroPwmSetting::Off)
        );
        let DeviceCommand::SetAeroPwmPercent(AeroPwmSetting::Margin(margin)) = profile
            .command(
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(80),
                false,
            )
            .unwrap()
        else {
            panic!("wrong command")
        };
        assert_eq!(margin.percent(), 20);
        assert_eq!(
            profile.command(
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(20),
                false
            ),
            Err(SettingsRequestError::InvalidValue)
        );
    }

    #[test]
    fn readable_alarm_options_do_not_grant_unsupported_writes() {
        let profile = falcon_control_profile();
        let descriptors = profile.descriptors(false);
        let alarm = descriptors
            .iter()
            .find(|entry| entry.id == SettingId::SpeedAlarmMode)
            .unwrap();
        let SettingControl::Choices(choices) = &alarm.control else {
            panic!("not choices")
        };
        assert_eq!(choices.len(), 4);
        assert_eq!(choices.iter().filter(|choice| choice.writable).count(), 2);
        assert_eq!(
            profile.command(
                SettingId::SpeedAlarmMode,
                DeviceSettingValue::Choice(2),
                false
            ),
            Err(SettingsRequestError::InvalidValue)
        );
    }

    #[test]
    fn uncertain_charge_meaning_cannot_be_enabled_by_validation_mode() {
        let profile = aero_control_profile();
        let descriptors = profile.descriptors(true);
        let charge = descriptors
            .iter()
            .find(|entry| entry.id == SettingId::ChargeLimitDiagnostic)
            .unwrap();
        assert_eq!(charge.access, SettingAccess::ReadOnly);
        assert_eq!(charge.control, SettingControl::ReadOnly);
        assert_eq!(
            profile.command(
                SettingId::ChargeLimitDiagnostic,
                DeviceSettingValue::Number(46),
                true
            ),
            Err(SettingsRequestError::ReadOnly)
        );
        assert_eq!(
            profile.command(
                SettingId::PwmTiltback,
                DeviceSettingValue::Number(80),
                false
            ),
            Err(SettingsRequestError::Unverified)
        );
        assert!(
            profile
                .command(SettingId::PwmTiltback, DeviceSettingValue::Number(80), true)
                .is_ok()
        );
    }

    #[test]
    fn numeric_descriptors_and_checked_commands_agree_on_every_value() {
        for profile in [aero_control_profile(), falcon_control_profile()] {
            for descriptor in profile.descriptors(true) {
                let SettingControl::Number {
                    minimum,
                    maximum,
                    step,
                    ..
                } = descriptor.control
                else {
                    continue;
                };
                for value in -256..=256 {
                    let permitted =
                        (minimum..=maximum).contains(&value) && (value - minimum) % step == 0;
                    assert_eq!(
                        profile
                            .command(descriptor.id, DeviceSettingValue::Number(value), true)
                            .is_ok(),
                        permitted,
                        "{:?}: {value}",
                        descriptor.id
                    );
                }
            }
        }
    }
}
