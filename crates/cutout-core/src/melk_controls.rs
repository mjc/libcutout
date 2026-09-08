//! Bounded controller-local MELK lighting controls.
//! Protocol research: dave-code-ruiz/elkbledom at f41b8a27838f5b76f2b8b2c628b61cd700fe5fc3.
//! Pattern IDs describe the reference catalog, not proof of every firmware's visual mapping.

use crate::LightingPowerState;

/// A rejected controller-local setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MelkControlError {
    /// Pattern is outside the reference's 0..=227 catalog.
    #[error("MELK pattern must be between 0 and 227")]
    Pattern,
    /// The controller has eight reference microphone modes.
    #[error("MELK music effect must be between 0 and 7")]
    MusicEffect,
    /// Sensitivity is a percentage.
    #[error("MELK sensitivity must be between 0 and 100")]
    Sensitivity,
    /// Invalid 24-hour time, ISO weekday, or weekday mask.
    #[error("invalid MELK clock or schedule")]
    Time,
}

macro_rules! bounded_setting {
    ($name:ident, $max:literal, $error:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
        #[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
        pub struct $name(u8);

        impl $name {
            /// Returns the validated setting.
            #[must_use]
            pub const fn value(self) -> u8 {
                self.0
            }
        }
        impl TryFrom<u8> for $name {
            type Error = MelkControlError;
            fn try_from(value: u8) -> Result<Self, Self::Error> {
                if value <= $max {
                    Ok(Self(value))
                } else {
                    Err(MelkControlError::$error)
                }
            }
        }
        impl From<$name> for u8 {
            fn from(value: $name) -> u8 {
                value.0
            }
        }
    };
}

bounded_setting!(
    MelkPattern,
    227,
    Pattern,
    "Reference MELK pattern ID; appearance depends on controller firmware."
);
bounded_setting!(
    MelkMusicEffect,
    7,
    MusicEffect,
    "Controller microphone effect index (0..=7), independent of phone audio."
);
bounded_setting!(
    MelkSensitivity,
    100,
    Sensitivity,
    "Controller microphone sensitivity percentage."
);

/// Playback saved with a lighting preset; solid RGB remains the backward-compatible default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum LightingPlayback {
    /// One solid RGB color.
    #[default]
    Solid,
    /// Controller-local pattern and native 0..=255 speed.
    Effect {
        /// Reference pattern ID.
        pattern: MelkPattern,
        /// Native speed (not a percentage).
        speed: u8,
    },
    /// Controller-local microphone mode; no phone recording or audio analysis.
    Music {
        /// Microphone effect index.
        effect: MelkMusicEffect,
        /// Microphone gain percentage.
        sensitivity: MelkSensitivity,
    },
}

/// A validated controller-local on/off schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MelkSchedule {
    power: LightingPowerState,
    hour: u8,
    minute: u8,
    days: u8,
    enabled: bool,
}

impl MelkSchedule {
    /// Creates a schedule. Weekday bits are Monday=1 through Sunday=64; zero is one-shot.
    ///
    /// # Errors
    /// Rejects hours above 23, minutes above 59, and weekday masks above 127.
    pub const fn new(
        power: LightingPowerState,
        hour: u8,
        minute: u8,
        days: u8,
        enabled: bool,
    ) -> Result<Self, MelkControlError> {
        if hour > 23 || minute > 59 || days > 127 {
            return Err(MelkControlError::Time);
        }
        Ok(Self {
            power,
            hour,
            minute,
            days,
            enabled,
        })
    }

    /// Returns power, hour, minute, weekday mask, and enabled state.
    #[must_use]
    pub const fn components(self) -> (LightingPowerState, u8, u8, u8, bool) {
        (self.power, self.hour, self.minute, self.days, self.enabled)
    }
}

/// Validated local clock time with ISO weekday (Monday=1, Sunday=7).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MelkClock {
    hour: u8,
    minute: u8,
    second: u8,
    weekday: u8,
}

impl MelkClock {
    /// Creates a clock synchronization value.
    ///
    /// # Errors
    /// Rejects invalid 24-hour time or ISO weekdays outside 1..=7.
    pub const fn new(
        hour: u8,
        minute: u8,
        second: u8,
        weekday: u8,
    ) -> Result<Self, MelkControlError> {
        if hour > 23 || minute > 59 || second > 59 || weekday < 1 || weekday > 7 {
            return Err(MelkControlError::Time);
        }
        Ok(Self {
            hour,
            minute,
            second,
            weekday,
        })
    }

    /// Returns hour, minute, second, and ISO weekday.
    #[must_use]
    pub const fn components(self) -> (u8, u8, u8, u8) {
        (self.hour, self.minute, self.second, self.weekday)
    }
}

/// Controller-local commands, separate from vehicle lighting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MelkControl {
    /// Select a built-in pattern.
    Pattern(MelkPattern),
    /// Set native effect speed.
    Speed(u8),
    /// Choose one of eight microphone effects.
    MusicEffect(MelkMusicEffect),
    /// Enable or disable the controller microphone.
    Microphone(bool),
    /// Set microphone sensitivity.
    Sensitivity(MelkSensitivity),
    /// Set the on/off scheduler slot.
    Schedule(MelkSchedule),
    /// Synchronize the controller's local clock.
    Clock(MelkClock),
}
