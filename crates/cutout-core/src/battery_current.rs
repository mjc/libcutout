//! Physical battery-current direction, independent of wire polarity and charge mode.

use std::{fmt, num::NonZeroU32};

use crate::{ReportedBatteryCurrent, RideOperatingState};

/// Nonzero electrical current magnitude, in milliamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CurrentMagnitude(NonZeroU32);

impl CurrentMagnitude {
    /// Creates a nonzero magnitude; zero has its own battery-current state.
    #[must_use]
    pub const fn new(milliamps: u32) -> Option<Self> {
        match NonZeroU32::new(milliamps) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns this magnitude in milliamps.
    #[must_use]
    pub const fn as_milliamps(self) -> u32 {
        self.0.get()
    }
}

/// Direction across the battery pack boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatteryCurrentDirection {
    /// Electrical current entering the pack.
    In,
    /// Electrical current leaving the pack.
    Out,
    /// A valid measured zero.
    Zero,
    /// A measured magnitude whose physical direction is not established.
    Unknown,
}

/// Battery current with physical direction and nonnegative magnitude.
///
/// Wire signs belong to [`ReportedBatteryCurrent`]. Neither current in nor
/// current out implies a charger connection or a vehicle operating state.
/// Negative magnitudes cannot enter the semantic API:
///
/// ```compile_fail
/// let current = cutout_core::BatteryCurrent::current_in(-1);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatteryCurrent {
    /// A valid measured zero; missing data is represented separately.
    Zero,
    /// Current entering the battery pack.
    In(CurrentMagnitude),
    /// Current leaving the battery pack.
    Out(CurrentMagnitude),
    /// Magnitude is known but physical direction is not.
    Unknown(CurrentMagnitude),
}

impl BatteryCurrent {
    /// Creates current entering the pack, or measured zero.
    #[must_use]
    pub const fn current_in(milliamps: u32) -> Self {
        match CurrentMagnitude::new(milliamps) {
            Some(value) => Self::In(value),
            None => Self::Zero,
        }
    }

    /// Creates current leaving the pack, or measured zero.
    #[must_use]
    pub const fn current_out(milliamps: u32) -> Self {
        match CurrentMagnitude::new(milliamps) {
            Some(value) => Self::Out(value),
            None => Self::Zero,
        }
    }

    /// Creates a measured magnitude without claiming a physical direction.
    #[must_use]
    pub const fn unknown(milliamps: u32) -> Self {
        match CurrentMagnitude::new(milliamps) {
            Some(value) => Self::Unknown(value),
            None => Self::Zero,
        }
    }

    /// Returns the physical direction, without any charging inference.
    #[must_use]
    pub const fn direction(self) -> BatteryCurrentDirection {
        match self {
            Self::Zero => BatteryCurrentDirection::Zero,
            Self::In(_) => BatteryCurrentDirection::In,
            Self::Out(_) => BatteryCurrentDirection::Out,
            Self::Unknown(_) => BatteryCurrentDirection::Unknown,
        }
    }

    /// Returns the nonnegative magnitude in milliamps.
    #[must_use]
    pub const fn magnitude_milliamps(self) -> u32 {
        match self {
            Self::Zero => 0,
            Self::In(value) | Self::Out(value) | Self::Unknown(value) => value.as_milliamps(),
        }
    }

    /// Classifies flow only when the current observation is present and fresh.
    ///
    /// Unrelated telemetry cannot renew a retained current reading. Future-dated
    /// observations and missing clocks are unknown rather than fresh.
    #[must_use]
    pub fn power_flow_fresh(
        self,
        operating_state: RideOperatingState,
        observed_at: Option<crate::MonotonicTimestamp>,
        now: Option<crate::MonotonicTimestamp>,
    ) -> PowerFlowDirection {
        match (observed_at, now) {
            (Some(observed_at), Some(now))
                if now >= observed_at
                    && now.saturating_duration_since(observed_at).as_milliseconds()
                        <= crate::CONNECTED_WHEEL_MOTION_FRESHNESS_MS =>
            {
                self.power_flow(operating_state)
            }
            _ => PowerFlowDirection::Unknown,
        }
    }

    /// Classifies physical flow using an independently resolved, fresh ride state.
    ///
    /// Callers must resolve contradictory motion/charge evidence before calling;
    /// [`RideOperatingState::resolve_fresh`] gives motion priority. Current in
    /// while riding is regeneration, never charging. Unknown direction remains
    /// unknown even when a charger is connected.
    #[must_use]
    pub const fn power_flow(self, operating_state: RideOperatingState) -> PowerFlowDirection {
        match self {
            Self::Zero => PowerFlowDirection::Zero,
            Self::Out(_) => PowerFlowDirection::Discharge,
            Self::Unknown(_) => PowerFlowDirection::Unknown,
            Self::In(_) => match operating_state {
                RideOperatingState::Riding => PowerFlowDirection::Regeneration,
                RideOperatingState::Charging => PowerFlowDirection::Charging,
                RideOperatingState::Parked
                | RideOperatingState::Standing
                | RideOperatingState::Unknown => PowerFlowDirection::Unknown,
            },
        }
    }
}

impl fmt::Display for BatteryCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => f.write_str("0"),
            Self::In(value) => write!(f, "{} in", value.as_milliamps()),
            Self::Out(value) => write!(f, "{} out", value.as_milliamps()),
            Self::Unknown(value) => write!(f, "{} direction unknown", value.as_milliamps()),
        }
    }
}

/// Evidence-backed relationship between a wire sign and physical pack direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatteryCurrentPolarity {
    /// Positive wire current leaves the pack.
    PositiveIsOut,
    /// Positive wire current enters the pack.
    PositiveIsIn,
    /// The protocol/model's polarity has not been established.
    Unknown,
}

impl ReportedBatteryCurrent {
    /// Normalizes a raw reading once at the protocol/model adapter boundary.
    ///
    /// `unsigned_abs` preserves `i32::MIN` without overflow or saturation.
    #[must_use]
    pub const fn normalize(self, polarity: BatteryCurrentPolarity) -> BatteryCurrent {
        let value = self.as_milliamps();
        let magnitude = value.unsigned_abs();
        match polarity {
            BatteryCurrentPolarity::Unknown => BatteryCurrent::unknown(magnitude),
            BatteryCurrentPolarity::PositiveIsOut if value >= 0 => {
                BatteryCurrent::current_out(magnitude)
            }
            BatteryCurrentPolarity::PositiveIsIn if value < 0 => {
                BatteryCurrent::current_out(magnitude)
            }
            BatteryCurrentPolarity::PositiveIsOut | BatteryCurrentPolarity::PositiveIsIn => {
                BatteryCurrent::current_in(magnitude)
            }
        }
    }
}

/// Contextual interpretation of battery flow; separate from electrical direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerFlowDirection {
    /// Current leaving the battery pack, including parked consumption.
    Discharge,
    /// A valid measured zero current.
    Zero,
    /// Current entering the battery with independently admitted charging state.
    Charging,
    /// Current entering the battery while riding.
    Regeneration,
    /// Insufficient direction or operating-state evidence.
    Unknown,
}
