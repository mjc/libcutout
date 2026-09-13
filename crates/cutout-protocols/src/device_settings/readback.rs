//! Protocol fields become semantic observations before crossing a native boundary.

use cutout_core::{
    DeviceSettingValue, LightState, Measured, PedalMode, RollAngle, SettingId, SettingsEntry,
    SettingsReadback, SpeedAlarmMode,
};

use super::{DeviceControlProfile, SettingControl, command_kind, control};
use crate::{
    AERO_FIELD_BEEPER_VOLUME_PERCENT, AERO_FIELD_BRAKE_OVERPRESSURE_ALARM_PERCENT,
    AERO_FIELD_DISPLAY_BACKLIGHT_PERCENT, AERO_FIELD_DYNAMIC_ASSIST_PERCENT,
    AERO_FIELD_HIGH_SPEED_MODE, AERO_FIELD_LATERAL_TILT_LIMIT_DEGREES, AERO_FIELD_LOW_BATTERY_MODE,
    AERO_FIELD_MAX_CHARGE_VOLTAGE_RAW, AERO_FIELD_PEDAL_DIP_COMPENSATION_PERCENT,
    AERO_FIELD_PEDAL_HARDNESS_PERCENT, AERO_FIELD_PWM_PERCENT, AERO_FIELD_TRANSPORT_MODE,
    AERO_FIELD_VOLTAGE_CORRECTION_TENTHS_PERCENT, AERO_FIELD_WHEEL_UNITS,
    BEGODE_FIELD_LED_AND_LIGHT_MODE, BEGODE_FIELD_SETTINGS_BITS, BEGODE_FIELD_TILTBACK_SPEED_KMH,
    BegodeLightMode, VETERAN_FIELD_PEDALS_MODE, VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
    VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
};

/// A field present in a protocol response, with its semantic meaning and evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingObservation {
    /// Stable setting identity.
    pub id: SettingId,
    /// Original evidence, or explicit unknown when the present field is unusable.
    /// A field absent from the response produces no observation at all.
    pub value: Option<Measured<DeviceSettingValue>>,
}

impl DeviceControlProfile {
    /// Converts decoded protocol fields without granting writes or fabricating evidence.
    #[must_use]
    pub fn normalize_readback(self, readback: SettingsReadback) -> Vec<SettingObservation> {
        let mut observations = Vec::new();
        for entry in readback.entries().into_iter().flatten() {
            normalize_entry(entry, &mut observations);
        }
        observations.retain(|entry| self.available.supports_command_kind(command_kind(entry.id)));
        observations
    }
}

fn push(
    observations: &mut Vec<SettingObservation>,
    entry: SettingsEntry,
    id: SettingId,
    value: Option<DeviceSettingValue>,
) {
    observations.push(SettingObservation {
        id,
        value: value.map(|value| Measured {
            value,
            source: entry.source,
            quality: entry.quality,
            verification: entry.verification,
        }),
    });
}

fn normalize_entry(entry: SettingsEntry, observations: &mut Vec<SettingObservation>) {
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
        VETERAN_FIELD_PEDALS_MODE => {
            let value = u16::try_from(raw)
                .ok()
                .and_then(PedalMode::from_veteran_raw)
                .map(pedal_choice);
            push(observations, entry, SettingId::PedalMode, value);
            return;
        }
        BEGODE_FIELD_TILTBACK_SPEED_KMH => {
            // The Live-B decoder already applied the wheel's unit mode.
            let value = u8::try_from(raw)
                .ok()
                .filter(|speed| *speed <= 99)
                .map(|speed| DeviceSettingValue::Number(i32::from(speed) * 10));
            push(observations, entry, SettingId::MaximumSpeed, value);
            return;
        }
        BEGODE_FIELD_SETTINGS_BITS => {
            normalize_begode_modes(entry, observations);
            return;
        }
        BEGODE_FIELD_LED_AND_LIGHT_MODE => {
            normalize_begode_lighting(entry, observations);
            return;
        }
        _ => return,
    };
    push(observations, entry, id, semantic_value(id, raw));
}

fn semantic_value(id: SettingId, raw: i64) -> Option<DeviceSettingValue> {
    match id {
        SettingId::PwmTiltback => match raw {
            0..=100 => Some(DeviceSettingValue::Number(i32::try_from(raw).ok()?)),
            200 => Some(DeviceSettingValue::Disabled),
            _ => None,
        },
        SettingId::TiltbackSpeed | SettingId::SpeedAlarmThreshold => {
            // Readable fractional values are preserved even when the encoder requires whole km/h.
            u16::try_from(raw)
                .ok()
                .filter(|value| (100..=2_000).contains(value))
                .map(|value| DeviceSettingValue::Number(i32::from(value)))
        }
        SettingId::ChargeLimitDiagnostic => (0..=70)
            .contains(&raw)
            .then(|| i32::try_from(raw).ok().map(DeviceSettingValue::Number))
            .flatten(),
        _ => match control(id) {
            SettingControl::Boolean => match raw {
                0 => Some(DeviceSettingValue::Boolean(false)),
                1 => Some(DeviceSettingValue::Boolean(true)),
                _ => None,
            },
            SettingControl::Choices(choices) => {
                let value = u16::try_from(raw).ok()?;
                choices
                    .iter()
                    .any(|choice| choice.id == value)
                    .then_some(DeviceSettingValue::Choice(value))
            }
            SettingControl::Number {
                minimum, maximum, ..
            } => {
                let value = i32::try_from(raw).ok()?;
                (minimum..=maximum)
                    .contains(&value)
                    .then_some(DeviceSettingValue::Number(value))
            }
            SettingControl::ReadOnly => None,
        },
    }
}

fn pedal_choice(value: PedalMode) -> DeviceSettingValue {
    DeviceSettingValue::Choice(match value {
        PedalMode::Hard => 0,
        PedalMode::Medium => 1,
        PedalMode::Soft => 2,
    })
}

fn normalize_begode_modes(entry: SettingsEntry, observations: &mut Vec<SettingObservation>) {
    let bits = u16::try_from(entry.field.value).ok();
    let pedal = bits
        .and_then(PedalMode::from_begode_settings_bits)
        .map(pedal_choice);
    let roll = bits
        .and_then(RollAngle::from_begode_settings_bits)
        .map(|value| {
            DeviceSettingValue::Choice(match value {
                RollAngle::Low => 0,
                RollAngle::Medium => 1,
                RollAngle::High => 2,
            })
        });
    let alarm = bits
        .and_then(SpeedAlarmMode::from_begode_settings_bits)
        .map(|value| {
            DeviceSettingValue::Choice(match value {
                SpeedAlarmMode::Both => 0,
                SpeedAlarmMode::StageOneOnly => 1,
                SpeedAlarmMode::Off => 2,
                SpeedAlarmMode::PwmTiltback => 3,
            })
        });
    push(observations, entry, SettingId::PedalMode, pedal);
    push(observations, entry, SettingId::RollAngleMode, roll);
    push(observations, entry, SettingId::SpeedAlarmMode, alarm);
}

fn normalize_begode_lighting(entry: SettingsEntry, observations: &mut Vec<SettingObservation>) {
    let packed = u16::try_from(entry.field.value).ok();
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
    push(observations, entry, SettingId::LightingPattern, pattern);
    push(observations, entry, SettingId::Headlight, light);
}
