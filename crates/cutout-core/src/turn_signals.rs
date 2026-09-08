//! Intent classification for future automatic turn signals.

use crate::{Angle, Speed};

/// Side selected by an automatic lean-based turn signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnSignalSide {
    /// Left-side signal.
    Left,
    /// Right-side signal.
    Right,
}

/// Which roll sign represents a left turn on the installed wheel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnSignalPolarity {
    /// Negative roll is a left turn (the provisional default).
    NegativeIsLeft,
    /// Positive roll is a left turn.
    PositiveIsLeft,
}

/// Thresholds for converting a sustained wheel lean into a turn signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnSignalConfig {
    /// Minimum forward speed in millimetres per second.
    pub minimum_speed_mm_s: i32,
    /// Roll magnitude required for activation, in millidegrees.
    pub engage_roll_mdeg: i32,
    /// Roll magnitude below which an active signal is released, in millidegrees.
    pub release_roll_mdeg: i32,
    /// Time the lean must remain active before selecting a side.
    pub engage_duration_ms: u64,
}

impl Default for TurnSignalConfig {
    fn default() -> Self {
        Self {
            // Five mph is the first conservative speed gate; this avoids using
            // low-speed balance corrections as turn intent.
            minimum_speed_mm_s: 2_235,
            engage_roll_mdeg: 8_000,
            release_roll_mdeg: 4_000,
            engage_duration_ms: 250,
        }
    }
}

/// Stateful, hysteretic lean detector for future left/right lighting output.
///
/// This classifies intent only. It does not select a controller zone or emit a
/// Bluetooth write; those require an independently verified wiring topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnSignalDetector {
    config: TurnSignalConfig,
    polarity: TurnSignalPolarity,
    candidate: Option<(TurnSignalSide, u64)>,
    active: Option<TurnSignalSide>,
}

impl Default for TurnSignalDetector {
    fn default() -> Self {
        Self::new(
            TurnSignalConfig::default(),
            TurnSignalPolarity::NegativeIsLeft,
        )
    }
}

impl TurnSignalDetector {
    /// Creates a detector with explicit thresholds and roll polarity.
    #[must_use]
    pub const fn new(config: TurnSignalConfig, polarity: TurnSignalPolarity) -> Self {
        Self {
            config,
            polarity,
            candidate: None,
            active: None,
        }
    }

    /// Consumes one telemetry sample and returns the currently selected side.
    ///
    /// Missing roll, low speed, or a lean below the release threshold clears an
    /// active signal. A new signal requires the engage threshold to remain
    /// crossed for the configured duration.
    #[must_use]
    pub fn update(
        &mut self,
        at_ms: u64,
        speed: Speed,
        roll: Option<Angle>,
    ) -> Option<TurnSignalSide> {
        let Some(roll) = roll else {
            return self.clear();
        };
        let magnitude = i64::from(roll.as_millidegrees()).abs();
        let speed_is_sufficient =
            speed.as_millimetres_per_second() >= self.config.minimum_speed_mm_s;

        if !speed_is_sufficient || magnitude < i64::from(self.config.release_roll_mdeg) {
            return self.clear();
        }

        let side = self.side(roll);
        if self.active == Some(side) {
            return self.active;
        }

        if magnitude < i64::from(self.config.engage_roll_mdeg) {
            self.candidate = None;
            return self.active;
        }

        match self.candidate {
            Some((candidate, started_at)) if candidate == side => {
                if at_ms.saturating_sub(started_at) >= self.config.engage_duration_ms {
                    self.active = Some(side);
                    self.candidate = None;
                }
            }
            _ => self.candidate = Some((side, at_ms)),
        }
        self.active
    }

    fn clear(&mut self) -> Option<TurnSignalSide> {
        self.candidate = None;
        self.active = None;
        None
    }

    fn side(self, roll: Angle) -> TurnSignalSide {
        let is_negative = roll.as_millidegrees() < 0;
        match (self.polarity, is_negative) {
            (TurnSignalPolarity::NegativeIsLeft, true)
            | (TurnSignalPolarity::PositiveIsLeft, false) => TurnSignalSide::Left,
            _ => TurnSignalSide::Right,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TurnSignalConfig, TurnSignalDetector, TurnSignalSide};
    use crate::{Angle, Speed};

    #[test]
    fn sustained_lean_at_speed_selects_a_side_and_release_clears_it() {
        let mut detector = TurnSignalDetector::default();

        assert_eq!(
            detector.update(0, Speed::from_kmh(10), Some(Angle::from_degrees(-10))),
            None
        );
        assert_eq!(
            detector.update(300, Speed::from_kmh(10), Some(Angle::from_degrees(-10))),
            Some(TurnSignalSide::Left)
        );
        assert_eq!(
            detector.update(400, Speed::from_kmh(10), Some(Angle::from_degrees(0))),
            None
        );
    }

    #[test]
    fn low_speed_lean_never_becomes_a_signal() {
        let mut detector = TurnSignalDetector::default();
        let speed = Speed::from_kmh(5);
        assert_eq!(
            detector.update(0, speed, Some(Angle::from_degrees(-12))),
            None
        );
        assert_eq!(
            detector.update(1_000, speed, Some(Angle::from_degrees(-12))),
            None
        );
    }

    #[test]
    fn polarity_and_hysteresis_keep_the_selected_side_stable() {
        let mut detector = TurnSignalDetector::new(
            TurnSignalConfig::default(),
            super::TurnSignalPolarity::PositiveIsLeft,
        );
        let speed = Speed::from_kmh(10);
        assert_eq!(
            detector.update(0, speed, Some(Angle::from_degrees(10))),
            None
        );
        assert_eq!(
            detector.update(250, speed, Some(Angle::from_degrees(10))),
            Some(TurnSignalSide::Left)
        );
        assert_eq!(
            detector.update(300, speed, Some(Angle::from_degrees(5))),
            Some(TurnSignalSide::Left)
        );
    }
}
