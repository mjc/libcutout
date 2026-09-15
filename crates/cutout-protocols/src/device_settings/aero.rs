use cutout_core::{
    AeroAngleAdjustment, AeroBeeperVolume, AeroBrakeOverpressureAlarm, AeroDisplayBacklight,
    AeroDynamicAssist, AeroHighSpeedMode, AeroLateralTiltLimit, AeroLowBatteryMode,
    AeroPedalDipCompensation, AeroPedalHardness, AeroPwmPercent, AeroPwmSetting, AeroRidingMode,
    AeroSpeedSetting, AeroTransportMode, AeroVoltageCorrection, AeroWheelUnits, CommandKind,
    DeviceCommand, DeviceSettingValue, LightState, PedalMode, SettingId, SettingsEntry,
};

use super::{
    SettingControl, SettingUnit, checked_number, choices, number, readback::SettingObservation,
    speed_control,
};

pub(super) const fn command_kind(id: SettingId) -> Option<CommandKind> {
    Some(match id {
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
        _ => return None,
    })
}

pub(super) fn control(id: SettingId) -> Option<SettingControl> {
    Some(match id {
        SettingId::HighBeam
        | SettingId::HighSpeedMode
        | SettingId::LowBatteryMode
        | SettingId::TransportMode => SettingControl::Boolean,
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold => speed_control(10, 200),
        SettingId::PwmTiltback => number(30, 100, 0, SettingUnit::PwmDutyPercent),
        SettingId::PedalHardness
        | SettingId::DisplayBrightness
        | SettingId::BeeperVolumePercent
        | SettingId::DynamicAssist
        | SettingId::PedalDipCompensation => number(0, 100, 0, SettingUnit::Percent),
        SettingId::LateralTiltLimit => number(35, 75, 0, SettingUnit::Degrees),
        SettingId::VoltageCorrection => number(-15, 15, 1, SettingUnit::Percent),
        SettingId::PedalAngle => number(-80, 80, 1, SettingUnit::Degrees),
        SettingId::BrakeOverpressureAlarm => number(90, 125, 0, SettingUnit::Percent),
        SettingId::DisplayUnits => choices(&[
            (0, "settings.choice.metric", true),
            (1, "settings.choice.imperial", true),
        ]),
        SettingId::RidingPreset => choices(&[
            (0, "settings.choice.hard", true),
            (1, "settings.choice.medium", true),
            (2, "settings.choice.soft", true),
        ]),
        SettingId::ChargeLimitDiagnostic => SettingControl::ReadOnly,
        SettingId::AutoShutdownRemaining => number(0, i32::MAX, 0, SettingUnit::Seconds),
        SettingId::ChargeMode => choices(&[
            (0, "settings.choice.not_charging", false),
            (1, "settings.choice.charging", false),
        ]),
        _ => return None,
    })
}

pub(super) const fn is_read_only(id: SettingId) -> bool {
    matches!(
        id,
        SettingId::ChargeLimitDiagnostic | SettingId::AutoShutdownRemaining | SettingId::ChargeMode
    )
}

pub(super) fn checked_command(id: SettingId, value: DeviceSettingValue) -> Option<DeviceCommand> {
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
    Some(match id {
        SettingId::HighBeam => DeviceCommand::SetAeroHighBeam(light(on)),
        SettingId::HighSpeedMode => DeviceCommand::SetAeroHighSpeedMode(AeroHighSpeedMode::new(on)),
        SettingId::LowBatteryMode => {
            DeviceCommand::SetAeroLowBatteryMode(AeroLowBatteryMode::new(on))
        }
        SettingId::TransportMode => DeviceCommand::SetAeroTransportMode(AeroTransportMode::new(on)),
        _ => return None,
    })
}

fn numeric_command(id: SettingId, value: i32) -> Option<DeviceCommand> {
    let value = if matches!(
        id,
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold
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
        _ => None,
    }
}

fn choice_command(id: SettingId, value: u16) -> Option<DeviceCommand> {
    match id {
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
        _ => None,
    }
}

const fn light(on: bool) -> LightState {
    if on { LightState::On } else { LightState::Off }
}

pub(super) fn normalize_readback(entry: SettingsEntry, observations: &mut Vec<SettingObservation>) {
    use crate::{
        AERO_FIELD_BEEPER_VOLUME_PERCENT, AERO_FIELD_BRAKE_OVERPRESSURE_ALARM_PERCENT,
        AERO_FIELD_DISPLAY_BACKLIGHT_PERCENT, AERO_FIELD_DYNAMIC_ASSIST_PERCENT,
        AERO_FIELD_HIGH_SPEED_MODE, AERO_FIELD_LATERAL_TILT_LIMIT_DEGREES,
        AERO_FIELD_LOW_BATTERY_MODE, AERO_FIELD_MAX_CHARGE_VOLTAGE_RAW,
        AERO_FIELD_PEDAL_DIP_COMPENSATION_PERCENT, AERO_FIELD_PEDAL_HARDNESS_PERCENT,
        AERO_FIELD_PWM_PERCENT, AERO_FIELD_TRANSPORT_MODE,
        AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT, AERO_FIELD_WHEEL_UNITS,
        VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS, VETERAN_FIELD_CHARGE_MODE,
        VETERAN_FIELD_PEDALS_MODE, VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
        VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
    };
    let raw = entry.field.value;
    let id = match entry.field.id {
        AERO_FIELD_PWM_PERCENT => SettingId::PwmTiltback,
        AERO_FIELD_PEDAL_HARDNESS_PERCENT => SettingId::PedalHardness,
        AERO_FIELD_DISPLAY_BACKLIGHT_PERCENT => SettingId::DisplayBrightness,
        AERO_FIELD_BEEPER_VOLUME_PERCENT => SettingId::BeeperVolumePercent,
        AERO_FIELD_DYNAMIC_ASSIST_PERCENT => SettingId::DynamicAssist,
        AERO_FIELD_PEDAL_DIP_COMPENSATION_PERCENT => SettingId::PedalDipCompensation,
        AERO_FIELD_LATERAL_TILT_LIMIT_DEGREES => SettingId::LateralTiltLimit,
        AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT => SettingId::VoltageCorrection,
        AERO_FIELD_MAX_CHARGE_VOLTAGE_RAW => SettingId::ChargeLimitDiagnostic,
        AERO_FIELD_BRAKE_OVERPRESSURE_ALARM_PERCENT => SettingId::BrakeOverpressureAlarm,
        AERO_FIELD_TRANSPORT_MODE => SettingId::TransportMode,
        AERO_FIELD_HIGH_SPEED_MODE => SettingId::HighSpeedMode,
        AERO_FIELD_LOW_BATTERY_MODE => SettingId::LowBatteryMode,
        AERO_FIELD_WHEEL_UNITS => SettingId::DisplayUnits,
        VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH => SettingId::TiltbackSpeed,
        VETERAN_FIELD_SPEED_ALERT_DECI_KMH => SettingId::SpeedAlarmThreshold,
        VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS => SettingId::AutoShutdownRemaining,
        VETERAN_FIELD_CHARGE_MODE => SettingId::ChargeMode,
        VETERAN_FIELD_PEDALS_MODE => {
            let value = u16::try_from(raw)
                .ok()
                .and_then(PedalMode::from_veteran_raw)
                .map(pedal_choice);
            super::readback::push(observations, entry, SettingId::PedalMode, value);
            return;
        }
        _ => return,
    };
    super::readback::push(observations, entry, id, semantic_value(id, raw));
}

fn semantic_value(id: SettingId, raw: i64) -> Option<DeviceSettingValue> {
    match id {
        SettingId::PwmTiltback => match raw {
            0..=100 => Some(DeviceSettingValue::Number(i32::try_from(raw).ok()?)),
            200 => Some(DeviceSettingValue::Disabled),
            _ => None,
        },
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold => u16::try_from(raw)
            .ok()
            .filter(|value| (100..=2_000).contains(value))
            .map(|value| DeviceSettingValue::Number(i32::from(value))),
        SettingId::ChargeLimitDiagnostic => (0..=70)
            .contains(&raw)
            .then(|| i32::try_from(raw).ok().map(DeviceSettingValue::Number))
            .flatten(),
        _ => super::control_value(control(id)?, raw),
    }
}

fn pedal_choice(value: PedalMode) -> DeviceSettingValue {
    DeviceSettingValue::Choice(match value {
        PedalMode::Hard => 0,
        PedalMode::Medium => 1,
        PedalMode::Soft => 2,
    })
}
