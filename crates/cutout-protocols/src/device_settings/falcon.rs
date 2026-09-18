use cutout_core::{
    DeviceSettingValue, LightState, PedalMode, RollAngle, SettingId, SettingsEntry, SpeedAlarmMode,
};

use super::{
    SettingControl, SettingUnit, choices, number, readback::SettingObservation, speed_control,
};

pub(super) fn control(id: SettingId) -> Option<SettingControl> {
    Some(match id {
        SettingId::Headlight | SettingId::AccelerationAssist | SettingId::Taillight => {
            SettingControl::Boolean
        }
        SettingId::MaximumSpeed => speed_control(0, 99),
        SettingId::BeeperVolumeLevel => number(1, 9, 0, SettingUnit::Level),
        SettingId::PowerOffDelay => number(0, 255, 0, SettingUnit::Minutes),
        SettingId::LightingPattern => choices(&[
            (0, "settings.choice.pattern_0", false),
            (1, "settings.choice.pattern_1", false),
            (2, "settings.choice.pattern_2", false),
            (3, "settings.choice.pattern_3", false),
            (4, "settings.choice.pattern_4", false),
            (5, "settings.choice.pattern_5", false),
            (6, "settings.choice.pattern_6", false),
            (7, "settings.choice.pattern_7", false),
            (8, "settings.choice.pattern_8", false),
            (9, "settings.choice.pattern_9", false),
        ]),
        SettingId::PedalMode => choices(&[
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
        _ => return None,
    })
}

pub(super) const fn is_read_only(id: SettingId) -> bool {
    matches!(id, SettingId::LightingPattern | SettingId::PowerOffDelay)
}

pub(super) fn normalize_readback(entry: SettingsEntry, observations: &mut Vec<SettingObservation>) {
    use crate::{
        BEGODE_FIELD_LED_AND_LIGHT_MODE, BEGODE_FIELD_POWER_OFF_TIMER_MINUTES,
        BEGODE_FIELD_SETTINGS_BITS, BEGODE_FIELD_TILTBACK_SPEED_KMH, BegodeLightMode,
    };

    let raw = entry.field.value;
    match entry.field.id {
        BEGODE_FIELD_POWER_OFF_TIMER_MINUTES => super::readback::push(
            observations,
            entry,
            SettingId::PowerOffDelay,
            semantic_value(SettingId::PowerOffDelay, raw),
        ),
        BEGODE_FIELD_TILTBACK_SPEED_KMH => {
            let value = u8::try_from(raw)
                .ok()
                .filter(|speed| *speed <= 99)
                .map(|speed| DeviceSettingValue::Number(i32::from(speed) * 10));
            super::readback::push(observations, entry, SettingId::MaximumSpeed, value);
        }
        BEGODE_FIELD_SETTINGS_BITS => {
            let bits = u16::try_from(raw).ok();
            super::readback::push(
                observations,
                entry,
                SettingId::PedalMode,
                bits.and_then(PedalMode::from_begode_settings_bits)
                    .map(pedal_choice),
            );
            super::readback::push(
                observations,
                entry,
                SettingId::RollAngleMode,
                bits.and_then(RollAngle::from_begode_settings_bits)
                    .map(roll_choice),
            );
            super::readback::push(
                observations,
                entry,
                SettingId::SpeedAlarmMode,
                bits.and_then(SpeedAlarmMode::from_begode_settings_bits)
                    .map(speed_alarm_choice),
            );
        }
        BEGODE_FIELD_LED_AND_LIGHT_MODE => {
            let packed = u16::try_from(raw).ok();
            let pattern = packed
                .map(|packed| packed >> 8)
                .filter(|pattern| *pattern <= 9)
                .map(DeviceSettingValue::Choice);
            let light = packed.and_then(|packed| {
                match BegodeLightMode::new(packed.to_le_bytes()[0]).light_state() {
                    Some(LightState::Off) => Some(DeviceSettingValue::Boolean(false)),
                    Some(LightState::On) => Some(DeviceSettingValue::Boolean(true)),
                    _ => None,
                }
            });
            super::readback::push(observations, entry, SettingId::LightingPattern, pattern);
            super::readback::push(observations, entry, SettingId::Headlight, light);
        }
        _ => {}
    }
}

fn semantic_value(id: SettingId, raw: i64) -> Option<DeviceSettingValue> {
    super::control_value(control(id)?, raw)
}

fn pedal_choice(value: PedalMode) -> DeviceSettingValue {
    DeviceSettingValue::Choice(match value {
        PedalMode::Hard => 0,
        PedalMode::Medium => 1,
        PedalMode::Soft => 2,
    })
}

fn roll_choice(value: RollAngle) -> DeviceSettingValue {
    DeviceSettingValue::Choice(match value {
        RollAngle::Low => 0,
        RollAngle::Medium => 1,
        RollAngle::High => 2,
    })
}

fn speed_alarm_choice(value: SpeedAlarmMode) -> DeviceSettingValue {
    DeviceSettingValue::Choice(match value {
        SpeedAlarmMode::Both => 0,
        SpeedAlarmMode::StageOneOnly => 1,
        SpeedAlarmMode::Off => 2,
        SpeedAlarmMode::PwmTiltback => 3,
    })
}
