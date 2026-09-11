//! Typed mobile controls for the MELK accessory; no raw-payload API.

use crate::{
    MobileMelkLightingError, MobileMelkLightingProfile, MobileMelkLightingRestoreStateDto,
    MobileMelkLightingWriteDto, MobileRgbLightingRecordError,
};
use cutout_core::{LightingPlayback, MelkClock, MelkControl, MelkSchedule};
use cutout_protocols::MelkLightingProfile;

/// Controller-local playback. Music uses the accessory microphone, not the phone.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileLightingPlaybackDto {
    /// Solid RGB.
    Solid,
    /// Reference pattern ID and native speed.
    Effect {
        /// Capture-backed MELK-OC21 pattern ID. Reference entries fail closed until verified.
        pattern: u8,
        /// Native speed (0..=255).
        speed: u8,
    },
    /// Accessory microphone effect and gain.
    Music {
        /// Microphone effect index (0..=7).
        effect: u8,
        /// Gain (0..=100 percent).
        sensitivity: u8,
    },
}

impl TryFrom<MobileLightingPlaybackDto> for LightingPlayback {
    type Error = MobileRgbLightingRecordError;
    fn try_from(value: MobileLightingPlaybackDto) -> Result<Self, Self::Error> {
        Ok(match value {
            MobileLightingPlaybackDto::Solid => Self::Solid,
            MobileLightingPlaybackDto::Effect { pattern, speed } => {
                let pattern: cutout_core::MelkPattern =
                    pattern.try_into().map_err(|_| Self::Error::InvalidState)?;
                if !MelkLightingProfile::capabilities().supports_effect(pattern.value()) {
                    return Err(Self::Error::InvalidState);
                }
                Self::Effect { pattern, speed }
            }
            MobileLightingPlaybackDto::Music {
                effect,
                sensitivity,
            } => {
                if !MelkLightingProfile::capabilities().controller_microphone {
                    return Err(Self::Error::InvalidState);
                }
                let effect: cutout_core::MelkMusicEffect =
                    effect.try_into().map_err(|_| Self::Error::InvalidState)?;
                let sensitivity: cutout_core::MelkSensitivity = sensitivity
                    .try_into()
                    .map_err(|_| Self::Error::InvalidState)?;
                Self::Music {
                    effect,
                    sensitivity,
                }
            }
        })
    }
}

impl From<LightingPlayback> for MobileLightingPlaybackDto {
    fn from(value: LightingPlayback) -> Self {
        match value {
            LightingPlayback::Solid => Self::Solid,
            LightingPlayback::Effect { pattern, speed } => Self::Effect {
                pattern: pattern.value(),
                speed,
            },
            LightingPlayback::Music {
                effect,
                sensitivity,
            } => Self::Music {
                effect: effect.value(),
                sensitivity: sensitivity.value(),
            },
        }
    }
}

/// One on/off scheduler slot. Repeat mask: Monday=1 through Sunday=64; zero is one-shot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkScheduleDto {
    /// The on slot when true; the off slot otherwise.
    pub power_on: bool,
    /// Local hour (0..=23).
    pub hour: u8,
    /// Local minute (0..=59).
    pub minute: u8,
    /// Repeating weekday mask (0..=127).
    pub days: u8,
    /// Whether this slot is enabled.
    pub enabled: bool,
}

/// Current local clock time used before writing a schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkClockDto {
    /// Local hour (0..=23).
    pub hour: u8,
    /// Local minute (0..=59).
    pub minute: u8,
    /// Local second (0..=59).
    pub second: u8,
    /// ISO weekday: Monday=1, Sunday=7.
    pub weekday: u8,
}

fn power(on: bool) -> cutout_core::LightingPowerState {
    if on {
        cutout_core::LightingPowerState::On
    } else {
        cutout_core::LightingPowerState::Off
    }
}

pub(crate) fn plan_schedule(
    schedule: MobileMelkScheduleDto,
    clock: MobileMelkClockDto,
) -> Result<Vec<MobileMelkLightingWriteDto>, MobileMelkLightingError> {
    let schedule = MelkSchedule::new(
        power(schedule.power_on),
        schedule.hour,
        schedule.minute,
        schedule.days,
        schedule.enabled,
    )
    .map_err(|_| MobileMelkLightingError::InvalidSchedule)?;
    let clock = MelkClock::new(clock.hour, clock.minute, clock.second, clock.weekday)
        .map_err(|_| MobileMelkLightingError::InvalidClock)?;
    if !MelkLightingProfile::capabilities().schedules {
        return Err(MobileMelkLightingError::UnsupportedCapability);
    }
    Ok([
        MelkLightingProfile::control_action(MelkControl::Clock(clock)),
        MelkLightingProfile::control_action(MelkControl::Schedule(schedule)),
    ]
    .into_iter()
    .map(crate::mobile_melk_transport_action)
    .collect())
}

#[uniffi::export]
impl MobileMelkLightingProfile {
    /// Plans a complete state atomically. Solid/effect playback does not claim microphone state
    /// because OC21 readback is unavailable.
    ///
    /// # Errors
    /// Returns `InvalidState` without any writes for invalid brightness, pattern, or microphone settings.
    pub fn apply_state(
        &self,
        state: MobileMelkLightingRestoreStateDto,
    ) -> Result<Vec<MobileMelkLightingWriteDto>, MobileRgbLightingRecordError> {
        let typed = cutout_core::RgbLightingRequestedState::try_from(state)?;
        Ok(MelkLightingProfile::plan_state(typed)
            .into_iter()
            .map(crate::mobile_melk_transport_action)
            .collect())
    }

    /// Updates only the native effect speed, without restarting the pattern or changing power.
    #[must_use]
    pub fn set_effect_speed(&self, speed: u8) -> MobileMelkLightingWriteDto {
        crate::mobile_melk_control(MelkControl::Speed(speed))
    }

    /// Plans local clock sync followed by one scheduler-slot update.
    ///
    /// # Errors
    /// Returns `InvalidSchedule` for invalid schedule values or `InvalidClock` for invalid clock values.
    pub fn set_schedule(
        &self,
        schedule: MobileMelkScheduleDto,
        clock: MobileMelkClockDto,
    ) -> Result<Vec<MobileMelkLightingWriteDto>, MobileMelkLightingError> {
        plan_schedule(schedule, clock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MobileMelkLightingGattEvidence;

    fn profile() -> std::sync::Arc<MobileMelkLightingProfile> {
        MobileMelkLightingProfile::new(
            "MELK-OC21 6A".into(),
            MobileMelkLightingGattEvidence {
                service_present: true,
                write_without_response: true,
                notify_or_indicate: true,
            },
        )
        .unwrap()
    }

    #[test]
    fn speed_update_is_one_frame_and_complete_effect_applies_speed_after_pattern() {
        let profile = profile();
        for speed in [0, 128, 255] {
            let write = profile.set_effect_speed(speed);
            assert_eq!(write.payload, [0x7e, 4, 2, speed, 255, 255, 255, 0, 0xef]);
            let writes = profile
                .apply_state(MobileMelkLightingRestoreStateDto {
                    power_on: false,
                    red: 0,
                    green: 0,
                    blue: 0,
                    brightness: 50,
                    playback: Some(MobileLightingPlaybackDto::Effect { pattern: 16, speed }),
                })
                .unwrap();
            assert_eq!(writes[0].payload, [0x7e, 5, 3, 16, 6, 255, 255, 0, 0xef]);
            assert_eq!(writes[1], write);
            assert_eq!(
                writes.last().unwrap().payload,
                profile.set_power(false).payload
            );
        }
    }

    #[test]
    fn restore_marker_keeps_playback_and_rejects_unverified_effects() {
        let mut state = MobileMelkLightingRestoreStateDto {
            power_on: true,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 50,
            playback: Some(MobileLightingPlaybackDto::Effect {
                pattern: 75,
                speed: 255,
            }),
        };
        let marker =
            crate::MobileMelkLightingRestoreMarker::new("controller".into(), state).unwrap();
        assert_eq!(
            marker.recover("controller".into(), true).requested,
            Some(state)
        );
        for pattern in [0, 11, 212, 213, 227] {
            state.playback = Some(MobileLightingPlaybackDto::Effect {
                pattern,
                speed: 255,
            });
            assert!(matches!(
                crate::MobileMelkLightingRestoreMarker::new("controller".into(), state),
                Err(MobileMelkLightingError::InvalidPlayback)
            ));
        }
        state.playback = Some(MobileLightingPlaybackDto::Music {
            effect: 7,
            sensitivity: 50,
        });
        assert!(matches!(
            crate::MobileMelkLightingRestoreMarker::new("controller".into(), state),
            Err(MobileMelkLightingError::InvalidPlayback)
        ));
    }

    #[test]
    fn complete_playback_validates_before_planning_and_finishes_with_power() {
        let mut state = MobileMelkLightingRestoreStateDto {
            power_on: false,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 50,
            playback: Some(MobileLightingPlaybackDto::Effect {
                pattern: 16,
                speed: 50,
            }),
        };
        let writes = profile().apply_state(state).unwrap();
        assert_eq!(writes.len(), 4);
        assert_eq!(writes[0].payload, [0x7e, 5, 3, 16, 6, 255, 255, 0, 0xef]);
        assert_eq!(writes[1].payload, [0x7e, 4, 2, 50, 255, 255, 255, 0, 0xef]);
        assert_eq!(
            writes.last().unwrap().payload,
            profile().set_power(false).payload
        );
        for pattern in [0, 11, 212, 213, 227, 228] {
            state.playback = Some(MobileLightingPlaybackDto::Effect { pattern, speed: 50 });
            assert_eq!(
                profile().apply_state(state),
                Err(MobileRgbLightingRecordError::InvalidState)
            );
        }
        state.playback = Some(MobileLightingPlaybackDto::Music {
            effect: 7,
            sensitivity: 100,
        });
        assert_eq!(
            profile().apply_state(state),
            Err(MobileRgbLightingRecordError::InvalidState)
        );
        state.playback = None;
        let writes = profile().apply_state(state).unwrap();
        assert_eq!(writes[0].payload, [0x7e, 0, 5, 3, 1, 2, 3, 0, 0xef]);
    }
    #[test]
    fn schedule_and_clock_errors_remain_distinct() {
        let invalid_schedule = MobileMelkScheduleDto {
            power_on: true,
            hour: 24,
            minute: 0,
            days: 0,
            enabled: true,
        };
        let valid_schedule = MobileMelkScheduleDto {
            power_on: true,
            hour: 12,
            minute: 0,
            days: 0,
            enabled: true,
        };
        let invalid_clock = MobileMelkClockDto {
            hour: 12,
            minute: 0,
            second: 0,
            weekday: 0,
        };
        let valid_clock = MobileMelkClockDto {
            hour: 12,
            minute: 0,
            second: 0,
            weekday: 1,
        };

        assert_eq!(
            profile().set_schedule(invalid_schedule, valid_clock),
            Err(MobileMelkLightingError::InvalidSchedule)
        );
        assert_eq!(
            profile().set_schedule(valid_schedule, invalid_clock),
            Err(MobileMelkLightingError::InvalidClock)
        );
        assert_eq!(
            profile().set_schedule(valid_schedule, valid_clock),
            Err(MobileMelkLightingError::UnsupportedCapability)
        );
    }
}
