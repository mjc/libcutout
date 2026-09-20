//! Protocol-owned setting values and wire-value validation.

/// NOSFET/Veteran modern binary T riding mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VeteranRidingMode {
    /// Firm riding response.
    Hard,
    /// Medium riding response.
    Medium,
    /// Soft riding response.
    Soft,
}

impl VeteranRidingMode {
    /// Decodes the modern binary T wire value documented by EUC World.
    #[must_use]
    pub const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Soft),
            2 => Some(Self::Medium),
            3 => Some(Self::Hard),
            _ => None,
        }
    }

    /// Returns the modern binary T wire value documented by EUC World.
    #[must_use]
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Hard => 3,
            Self::Medium => 2,
            Self::Soft => 1,
        }
    }
}

/// NOSFET/Veteran speed setting accepted by the documented binary frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranSpeedSetting(u8);

impl VeteranSpeedSetting {
    /// Creates a speed setting in the wheel's documented 10..=200 km/h range.
    #[must_use]
    pub const fn new(kilometres_per_hour: u8) -> Option<Self> {
        match kilometres_per_hour {
            10..=200 => Some(Self(kilometres_per_hour)),
            _ => None,
        }
    }

    /// Returns the whole-kilometres-per-hour wire value.
    #[must_use]
    pub const fn kilometres_per_hour(self) -> u8 {
        self.0
    }
}

/// NOSFET Aero gyro-calibration lifecycle reported by page-8 settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VeteranGyroCalibrationState {
    /// The wheel is ready to start calibration.
    Idle,
    /// Calibration has started and the wheel is waiting for completion.
    Waiting,
    /// Calibration completed successfully.
    Complete,
}

impl VeteranGyroCalibrationState {
    /// Decodes the page-8 state byte documented by the official app.
    #[must_use]
    pub const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Idle),
            1 => Some(Self::Waiting),
            2 => Some(Self::Complete),
            _ => None,
        }
    }

    /// Returns the page-8 state byte.
    #[must_use]
    pub const fn wire_value(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Waiting => 1,
            Self::Complete => 2,
        }
    }
}

/// NOSFET/Veteran brake overpressure alarm threshold, in percent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranBrakeOverpressureAlarm(u8);

impl VeteranBrakeOverpressureAlarm {
    /// Creates a brake overpressure threshold in the source-backed range.
    #[must_use]
    pub const fn new(percent: u8) -> Option<Self> {
        match percent {
            90..=125 => Some(Self(percent)),
            _ => None,
        }
    }

    /// Returns the threshold percentage written to the wheel.
    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }
}

/// NOSFET/Veteran PWT warning margin, measured as unused PWM percentage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranPwmPercent(u8);

impl VeteranPwmPercent {
    /// Creates a warning margin from the PWM utilization shown to the rider.
    #[must_use]
    pub const fn from_duty_percent(duty: u8) -> Option<Self> {
        match 100_u8.checked_sub(duty) {
            Some(margin) => Self::new(margin),
            None => None,
        }
    }

    /// Returns PWM utilization at the warning threshold.
    #[must_use]
    pub const fn duty_percent(self) -> u8 {
        100 - self.0
    }

    /// Creates a PWM warning margin in EUC World's documented 0..=70 range.
    #[must_use]
    pub const fn new(percent: u8) -> Option<Self> {
        if percent <= 70 {
            Some(Self(percent))
        } else {
            None
        }
    }

    /// Returns the unused PWM margin percentage.
    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }
}

/// NOSFET/Veteran PWT warning configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VeteranPwmSetting {
    /// Disable the PWM warning.
    Off,
    /// Warn when the unused PWM percentage reaches this margin.
    Margin(VeteranPwmPercent),
}

impl From<VeteranPwmPercent> for VeteranPwmSetting {
    fn from(percent: VeteranPwmPercent) -> Self {
        Self::Margin(percent)
    }
}

/// NOSFET/Veteran MD pedal hardness, independently of the three pedal modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranPedalHardness(u8);

impl VeteranPedalHardness {
    /// Creates the documented 0..=100 percent ride-mode setting.
    #[must_use]
    pub const fn new(percent: u8) -> Option<Self> {
        if percent <= 100 {
            Some(Self(percent))
        } else {
            None
        }
    }

    /// Returns the percentage wire value.
    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }
}

/// Units shown by the NOSFET Aero wheel display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VeteranWheelUnits {
    /// Kilometres and kilometres per hour.
    Metric,
    /// Miles and miles per hour.
    Imperial,
}

impl VeteranWheelUnits {
    /// Decodes the documented wheel-display mode; other values are unavailable.
    #[must_use]
    pub const fn from_display_mode(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Metric),
            1 => Some(Self::Imperial),
            _ => None,
        }
    }

    /// Returns the documented display-mode value.
    #[must_use]
    pub const fn display_mode(self) -> u8 {
        match self {
            Self::Metric => 0,
            Self::Imperial => 1,
        }
    }
}

macro_rules! aero_toggle_setting {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name(bool);

        impl $name {
            /// Creates a toggle setting from its documented binary value.
            #[must_use]
            pub const fn new(enabled: bool) -> Self {
                Self(enabled)
            }

            /// Returns the binary value sent to the wheel.
            #[must_use]
            pub const fn enabled(self) -> bool {
                self.0
            }
        }
    };
}

aero_toggle_setting!(
    /// Aero high-speed mode, which changes the wheel's high-speed behavior.
    VeteranHighSpeedMode
);
aero_toggle_setting!(
    /// Aero low-battery mode, which changes the wheel's low-voltage behavior.
    VeteranLowBatteryMode
);
aero_toggle_setting!(
    /// Aero transportation mode, which prevents normal motor startup.
    VeteranTransportMode
);

macro_rules! aero_bounded_setting {
    ($(#[$doc:meta])* $name:ident($primitive:ty), $minimum:literal..=$maximum:literal, $value:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name($primitive);

        impl $name {
            /// Creates a setting within the source-backed inclusive bounds.
            #[must_use]
            pub const fn new(value: $primitive) -> Option<Self> {
                match value {
                    $minimum..=$maximum => Some(Self(value)),
                    _ => None,
                }
            }

            /// Returns the setting value in its documented units.
            #[must_use]
            pub const fn $value(self) -> $primitive {
                self.0
            }
        }
    };
}

aero_bounded_setting!(
    /// Aero wheel display backlight brightness, from 0 through 100 percent.
    VeteranDisplayBacklight(u8), 0..=100, percent
);
aero_bounded_setting!(
    /// Aero wheel beeper volume, from 0 through 100 percent.
    VeteranBeeperVolume(u8), 0..=100, percent
);
aero_bounded_setting!(
    /// Aero dynamic assist, from 0 through 100 percent.
    VeteranDynamicAssist(u8), 0..=100, percent
);
aero_bounded_setting!(
    /// Aero pedal-dip compensation, from 0 through 100 percent.
    VeteranPedalDipCompensation(u8), 0..=100, percent
);
aero_bounded_setting!(
    /// Aero lateral tilt limit, from 35 through 75 degrees.
    VeteranLateralTiltLimit(u8), 35..=75, degrees
);
aero_bounded_setting!(
    /// Aero voltage correction, from -1.5 through +1.5 percent.
    VeteranVoltageCorrection(i8), -15..=15, tenths_of_percent
);

/// NOSFET/Veteran ANG (vertical angle) adjustment in tenths of a degree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranAngleAdjustment(i8);

impl VeteranAngleAdjustment {
    /// Creates an angle adjustment in the documented -8.0..=8.0 degree range.
    #[must_use]
    pub const fn new(tenths_of_degree: i8) -> Option<Self> {
        if tenths_of_degree < -80 || tenths_of_degree > 80 {
            None
        } else {
            Some(Self(tenths_of_degree))
        }
    }

    /// Returns the signed tenths-of-a-degree wire value.
    #[must_use]
    pub const fn tenths_of_degree(self) -> i8 {
        self.0
    }
}

/// NOSFET Aero maximum-charge voltage control in the official raw range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranMaxChargeVoltageRaw(u8);

impl VeteranMaxChargeVoltageRaw {
    /// Creates a maximum-charge setting in the official 0..=70 raw range.
    #[must_use]
    pub const fn new(raw: u8) -> Option<Self> {
        if raw <= 70 { Some(Self(raw)) } else { None }
    }

    /// Returns the raw `MxV` value written to the wheel.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// Begode max-speed setting accepted by the documented `W` submenu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeMaxSpeed(u8);

impl BegodeMaxSpeed {
    /// Creates a max-speed setting in the protocol's two-digit range.
    #[must_use]
    pub const fn new(kilometres_per_hour: u8) -> Option<Self> {
        if kilometres_per_hour <= 99 {
            Some(Self(kilometres_per_hour))
        } else {
            None
        }
    }

    /// Returns the whole-kilometres-per-hour wire value.
    #[must_use]
    pub const fn kilometres_per_hour(self) -> u8 {
        self.0
    }
}

/// Begode beeper volume accepted by the documented `W` submenu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeBeeperVolume(u8);

impl BegodeBeeperVolume {
    /// Creates a beeper volume in the documented 1..=9 range.
    #[must_use]
    pub const fn new(level: u8) -> Option<Self> {
        if level >= 1 && level <= 9 {
            Some(Self(level))
        } else {
            None
        }
    }

    /// Returns the protocol volume level.
    #[must_use]
    pub const fn level(self) -> u8 {
        self.0
    }
}

/// Begode LED mode accepted by the documented `W` submenu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeLedModeSetting(u8);

impl BegodeLedModeSetting {
    /// Creates an LED mode in the documented 0..=9 range.
    #[must_use]
    pub const fn new(mode: u8) -> Option<Self> {
        if mode <= 9 { Some(Self(mode)) } else { None }
    }

    /// Returns the protocol LED mode.
    #[must_use]
    pub const fn mode(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_aero_ranges_are_enforced() {
        assert!(VeteranSpeedSetting::new(9).is_none());
        assert!(VeteranSpeedSetting::new(10).is_some());
        assert!(VeteranSpeedSetting::new(200).is_some());
        assert!(VeteranSpeedSetting::new(201).is_none());
        assert!(VeteranPwmPercent::new(0).is_some());
        assert!(VeteranPwmPercent::new(70).is_some());
        assert!(VeteranPwmPercent::new(71).is_none());
        assert!(VeteranMaxChargeVoltageRaw::new(70).is_some());
        assert!(VeteranMaxChargeVoltageRaw::new(71).is_none());
        assert!(VeteranLateralTiltLimit::new(35).is_some());
        assert!(VeteranLateralTiltLimit::new(75).is_some());
        assert!(VeteranLateralTiltLimit::new(76).is_none());
        assert!(VeteranVoltageCorrection::new(-15).is_some());
        assert!(VeteranVoltageCorrection::new(15).is_some());
        assert!(VeteranVoltageCorrection::new(16).is_none());
        assert!(VeteranAngleAdjustment::new(-80).is_some());
        assert!(VeteranAngleAdjustment::new(80).is_some());
        assert!(VeteranAngleAdjustment::new(81).is_none());
        assert!(VeteranBrakeOverpressureAlarm::new(90).is_some());
        assert!(VeteranBrakeOverpressureAlarm::new(125).is_some());
        assert!(VeteranBrakeOverpressureAlarm::new(126).is_none());
    }

    #[test]
    fn documented_aero_wire_enums_round_trip() {
        assert_eq!(
            VeteranRidingMode::from_wire(1),
            Some(VeteranRidingMode::Soft)
        );
        assert_eq!(
            VeteranRidingMode::from_wire(2),
            Some(VeteranRidingMode::Medium)
        );
        assert_eq!(
            VeteranRidingMode::from_wire(3),
            Some(VeteranRidingMode::Hard)
        );
        assert_eq!(VeteranRidingMode::from_wire(0), None);
        assert_eq!(VeteranRidingMode::Hard.wire_value(), 3);
        assert_eq!(VeteranRidingMode::Medium.wire_value(), 2);
        assert_eq!(VeteranRidingMode::Soft.wire_value(), 1);
        assert_eq!(
            VeteranGyroCalibrationState::from_wire(0),
            Some(VeteranGyroCalibrationState::Idle)
        );
        assert_eq!(
            VeteranGyroCalibrationState::from_wire(1),
            Some(VeteranGyroCalibrationState::Waiting)
        );
        assert_eq!(
            VeteranGyroCalibrationState::from_wire(2),
            Some(VeteranGyroCalibrationState::Complete)
        );
        assert_eq!(VeteranGyroCalibrationState::from_wire(3), None);
    }
}
