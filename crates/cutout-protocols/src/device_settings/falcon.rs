use cutout_core::{
    AccelerationAssistState, BegodeBeeperVolume, BegodeLedModeSetting, BegodeMaxSpeed, CommandKind,
    DeviceCommand, DeviceSettingValue, LightState, PedalMode, RollAngle, SettingId, SpeedAlarmMode,
};

use super::{SettingControl, SettingUnit, checked_number, choices, number, speed_control};

pub(super) const fn command_kind(id: SettingId) -> Option<CommandKind> {
    Some(match id {
        SettingId::Headlight => CommandKind::SetLights,
        SettingId::PedalMode => CommandKind::SetPedalMode,
        SettingId::RollAngleMode => CommandKind::SetRollAngle,
        SettingId::SpeedAlarmMode => CommandKind::SetSpeedAlarmMode,
        SettingId::MaximumSpeed => CommandKind::SetBegodeMaxSpeed,
        SettingId::BeeperVolumeLevel => CommandKind::SetBegodeBeeperVolume,
        SettingId::LightingPattern => CommandKind::SetBegodeLedMode,
        SettingId::AccelerationAssist => CommandKind::SetAccelerationAssist,
        SettingId::Taillight => CommandKind::SetTaillight,
        _ => return None,
    })
}

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

pub(super) fn checked_command(id: SettingId, value: DeviceSettingValue) -> Option<DeviceCommand> {
    match value {
        DeviceSettingValue::Number(value) => numeric_command(id, value),
        DeviceSettingValue::Choice(value) => choice_command(id, value),
        DeviceSettingValue::Boolean(on) => boolean_command(id, on),
        DeviceSettingValue::Disabled => None,
    }
}

fn boolean_command(id: SettingId, on: bool) -> Option<DeviceCommand> {
    Some(match id {
        SettingId::Headlight => DeviceCommand::SetLights(light(on)),
        SettingId::AccelerationAssist => DeviceCommand::SetAccelerationAssist(if on {
            AccelerationAssistState::Enabled
        } else {
            AccelerationAssistState::Disabled
        }),
        SettingId::Taillight => DeviceCommand::SetTaillight(light(on)),
        _ => return None,
    })
}

fn numeric_command(id: SettingId, value: i32) -> Option<DeviceCommand> {
    let value = if id == SettingId::MaximumSpeed {
        if value % 10 != 0 {
            return None;
        }
        value / 10
    } else {
        value
    };
    match id {
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
        SettingId::LightingPattern => u8::try_from(value)
            .ok()
            .and_then(BegodeLedModeSetting::new)
            .map(DeviceCommand::SetBegodeLedMode),
        _ => None,
    }
}

const fn light(on: bool) -> LightState {
    if on { LightState::On } else { LightState::Off }
}
