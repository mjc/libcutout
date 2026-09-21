use cutout_core::{DeviceSettingValue, PedalMode, SettingId, SettingsEntry};

use super::{
    SettingBinding, SettingControl, SettingEncoderBinding, SettingObservationBinding, SettingUnit,
    choices, number, readback::SettingObservation, speed_control,
};

pub(super) fn binding(id: SettingId) -> Option<SettingBinding> {
    let (control, observation, encoder) = match id {
        SettingId::HighBeam => (
            SettingControl::Boolean,
            SettingObservationBinding::None,
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::HighSpeedMode | SettingId::LowBatteryMode | SettingId::TransportMode => (
            SettingControl::Boolean,
            SettingObservationBinding::MatchingField(match id {
                SettingId::HighSpeedMode => crate::AERO_FIELD_HIGH_SPEED_MODE,
                SettingId::LowBatteryMode => crate::AERO_FIELD_LOW_BATTERY_MODE,
                SettingId::TransportMode => crate::AERO_FIELD_TRANSPORT_MODE,
                _ => unreachable!(),
            }),
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold => (
            speed_control(10, 200),
            SettingObservationBinding::MatchingField(match id {
                SettingId::TiltbackSpeed => crate::VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                SettingId::SpeedAlarmThreshold => crate::VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
                _ => unreachable!(),
            }),
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::PwmTiltback => (
            number(30, 100, 0, SettingUnit::PwmDutyPercent),
            SettingObservationBinding::MatchingField(crate::AERO_FIELD_PWM_PERCENT),
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::PedalHardness
        | SettingId::DisplayBrightness
        | SettingId::BeeperVolumePercent
        | SettingId::DynamicAssist
        | SettingId::PedalDipCompensation => (
            number(0, 100, 0, SettingUnit::Percent),
            SettingObservationBinding::MatchingField(match id {
                SettingId::PedalHardness => crate::AERO_FIELD_PEDAL_HARDNESS_PERCENT,
                SettingId::DisplayBrightness => crate::AERO_FIELD_DISPLAY_BACKLIGHT_PERCENT,
                SettingId::BeeperVolumePercent => crate::AERO_FIELD_BEEPER_VOLUME_PERCENT,
                SettingId::DynamicAssist => crate::AERO_FIELD_DYNAMIC_ASSIST_PERCENT,
                SettingId::PedalDipCompensation => crate::AERO_FIELD_PEDAL_DIP_COMPENSATION_PERCENT,
                _ => unreachable!(),
            }),
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::LateralTiltLimit => (
            number(35, 75, 0, SettingUnit::Degrees),
            SettingObservationBinding::MatchingField(crate::AERO_FIELD_LATERAL_TILT_LIMIT_DEGREES),
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::VoltageCorrection => (
            number(-15, 15, 1, SettingUnit::Percent),
            SettingObservationBinding::MatchingField(
                crate::AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT,
            ),
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::PedalAngle => (
            number(-80, 80, 1, SettingUnit::Degrees),
            SettingObservationBinding::None,
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::BrakeOverpressureAlarm => (
            number(90, 125, 0, SettingUnit::Percent),
            SettingObservationBinding::MatchingField(
                crate::AERO_FIELD_BRAKE_OVERPRESSURE_ALARM_PERCENT,
            ),
            SettingEncoderBinding::Nosfet,
        ),
        _ => return binding_remaining(id),
    };
    Some(SettingBinding::new(control, observation, encoder))
}

fn binding_remaining(id: SettingId) -> Option<SettingBinding> {
    let (control, observation, encoder) = match id {
        SettingId::DisplayUnits => (
            choices(&[
                (0, "settings.choice.metric", true),
                (1, "settings.choice.imperial", true),
            ]),
            SettingObservationBinding::MatchingField(crate::AERO_FIELD_WHEEL_UNITS),
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::RidingPreset => (
            choices(&[
                (0, "settings.choice.hard", true),
                (1, "settings.choice.medium", true),
                (2, "settings.choice.soft", true),
            ]),
            SettingObservationBinding::None,
            SettingEncoderBinding::Nosfet,
        ),
        SettingId::ChargeLimitDiagnostic => (
            SettingControl::ReadOnly,
            SettingObservationBinding::ReadOnlyField(crate::AERO_FIELD_MAX_CHARGE_VOLTAGE_RAW),
            SettingEncoderBinding::None,
        ),
        SettingId::AutoShutdownRemaining => (
            number(0, i32::MAX, 0, SettingUnit::Seconds),
            SettingObservationBinding::ReadOnlyField(
                crate::VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS,
            ),
            SettingEncoderBinding::None,
        ),
        SettingId::ChargeMode => (
            choices(&[
                (0, "settings.choice.not_charging", false),
                (1, "settings.choice.charging", false),
            ]),
            SettingObservationBinding::ReadOnlyField(crate::VETERAN_FIELD_CHARGE_MODE),
            SettingEncoderBinding::None,
        ),
        _ => return None,
    };
    Some(SettingBinding::new(control, observation, encoder))
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
                .and_then(pedal_mode_from_veteran_raw)
                .map(pedal_choice);
            super::readback::push(observations, entry, SettingId::PedalMode, value);
            return;
        }
        _ => return,
    };
    super::readback::push(observations, entry, id, semantic_value(id, raw));
}

const fn pedal_mode_from_veteran_raw(raw: u16) -> Option<PedalMode> {
    match raw {
        0 => Some(PedalMode::Hard),
        1 => Some(PedalMode::Medium),
        2 => Some(PedalMode::Soft),
        _ => None,
    }
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
        _ => super::control_value(binding(id)?.control, raw),
    }
}

fn pedal_choice(value: PedalMode) -> DeviceSettingValue {
    DeviceSettingValue::Choice(match value {
        PedalMode::Hard => 0,
        PedalMode::Medium => 1,
        PedalMode::Soft => 2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn veteran_pedal_mode_uses_the_documented_mapping() {
        assert_eq!(pedal_mode_from_veteran_raw(0), Some(PedalMode::Hard));
        assert_eq!(pedal_mode_from_veteran_raw(1), Some(PedalMode::Medium));
        assert_eq!(pedal_mode_from_veteran_raw(2), Some(PedalMode::Soft));
        assert_eq!(pedal_mode_from_veteran_raw(1920), None);
    }
}
