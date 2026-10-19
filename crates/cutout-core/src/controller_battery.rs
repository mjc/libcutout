//! Shared projection for controller battery readings shown when cell-level BMS data is absent.

use crate::{BatteryCurrent, BatteryLevel, Voltage, VoltageDelta};

/// Battery readings retained for a live controller or a pack snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControllerBatteryReadings {
    /// Battery percentage reported by the controller.
    pub reported_level: Option<BatteryLevel>,
    /// Battery percentage estimated by the controller or protocol.
    pub estimated_level: Option<BatteryLevel>,
    /// Pack voltage.
    pub voltage: Option<Voltage>,
    /// Pack current with its physical direction preserved.
    pub current: Option<BatteryCurrent>,
    /// Loaded-versus-reference pack voltage delta.
    pub voltage_sag: Option<VoltageDelta>,
}

/// Source readings used to project a controller-only battery presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControllerBatteryInput {
    /// A BMS/pack snapshot without current live ride telemetry.
    Snapshot(ControllerBatteryReadings),
    /// Live telemetry, with the pack snapshot available as a fallback for readings.
    Live {
        /// Current controller telemetry.
        telemetry: ControllerBatteryReadings,
        /// Latest BMS/pack snapshot.
        snapshot: ControllerBatteryReadings,
    },
}

/// Detail shown beneath a controller-only battery estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControllerBatteryEstimateDetail {
    /// Live voltage and sag, or snapshot voltage and current, are available.
    RecentSag,
    /// Voltage is available, but no sag evidence is available.
    VoltageCurve,
    /// No voltage is available to support an estimate detail.
    Unavailable,
}

/// Confidence shown alongside a controller-only battery estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControllerBatteryConfidence {
    /// Live estimate and sag evidence, or snapshot voltage and current, are available.
    Medium,
    /// Some battery estimate or voltage evidence is available.
    Low,
    /// No controller estimate or voltage evidence is available.
    Unknown,
}

/// Selected readings and presentation confidence for the no-cell-data screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControllerBatteryProjection {
    /// Selected reported-or-estimated battery percentage.
    pub level: Option<BatteryLevel>,
    /// Selected live voltage, falling back to the snapshot voltage.
    pub voltage: Option<Voltage>,
    /// Selected live current, falling back to the snapshot current.
    pub current: Option<BatteryCurrent>,
    /// Sag reported by the active input source.
    pub voltage_sag: Option<VoltageDelta>,
    /// Detail describing the available estimate evidence.
    pub detail: ControllerBatteryEstimateDetail,
    /// Confidence classification for the available estimate evidence.
    pub confidence: ControllerBatteryConfidence,
}

impl ControllerBatteryProjection {
    /// Selects controller and snapshot readings without retaining values from an earlier input.
    #[must_use]
    pub fn from_input(input: ControllerBatteryInput) -> Self {
        match input {
            ControllerBatteryInput::Snapshot(snapshot) => Self::from_snapshot(snapshot),
            ControllerBatteryInput::Live {
                telemetry,
                snapshot,
            } => Self::from_live(telemetry, snapshot),
        }
    }

    fn from_snapshot(snapshot: ControllerBatteryReadings) -> Self {
        let level = preferred_level(snapshot);
        let (detail, confidence) = if snapshot.voltage.is_some() && snapshot.current.is_some() {
            (
                ControllerBatteryEstimateDetail::RecentSag,
                ControllerBatteryConfidence::Medium,
            )
        } else if snapshot.voltage.is_some() {
            (
                ControllerBatteryEstimateDetail::VoltageCurve,
                ControllerBatteryConfidence::Low,
            )
        } else {
            (
                ControllerBatteryEstimateDetail::Unavailable,
                if level.is_some() {
                    ControllerBatteryConfidence::Low
                } else {
                    ControllerBatteryConfidence::Unknown
                },
            )
        };

        Self {
            level,
            voltage: snapshot.voltage,
            current: snapshot.current,
            // Snapshot-only Swift presentation does not display sag; it only uses voltage
            // and current to select the legacy detail/confidence labels above.
            voltage_sag: None,
            detail,
            confidence,
        }
    }

    fn from_live(
        telemetry: ControllerBatteryReadings,
        snapshot: ControllerBatteryReadings,
    ) -> Self {
        let telemetry_level = preferred_level(telemetry);
        let level = telemetry_level.or(preferred_level(snapshot));
        let voltage = telemetry.voltage.or(snapshot.voltage);
        let current = telemetry.current.or(snapshot.current);
        let detail = if telemetry.voltage.is_some() && telemetry.voltage_sag.is_some() {
            ControllerBatteryEstimateDetail::RecentSag
        } else if telemetry.voltage.is_some() {
            ControllerBatteryEstimateDetail::VoltageCurve
        } else {
            ControllerBatteryEstimateDetail::Unavailable
        };
        let confidence = if telemetry_level.is_some() && telemetry.voltage_sag.is_some() {
            ControllerBatteryConfidence::Medium
        } else if telemetry_level.is_some() || telemetry.voltage.is_some() {
            ControllerBatteryConfidence::Low
        } else {
            ControllerBatteryConfidence::Unknown
        };

        Self {
            level,
            voltage,
            current,
            voltage_sag: telemetry.voltage_sag,
            detail,
            confidence,
        }
    }
}

fn preferred_level(readings: ControllerBatteryReadings) -> Option<BatteryLevel> {
    readings.reported_level.or(readings.estimated_level)
}

#[cfg(test)]
mod tests {
    use super::{
        ControllerBatteryConfidence, ControllerBatteryEstimateDetail, ControllerBatteryInput,
        ControllerBatteryProjection, ControllerBatteryReadings,
    };
    use crate::{BatteryCurrent, BatteryLevel, Voltage, VoltageDelta};

    #[test]
    fn snapshot_prefers_reported_level_and_classifies_present_readings() {
        let projection = ControllerBatteryProjection::from_input(ControllerBatteryInput::Snapshot(
            ControllerBatteryReadings {
                reported_level: Some(BatteryLevel::from_percent(0)),
                estimated_level: Some(BatteryLevel::from_percent(50)),
                voltage: Some(Voltage::from_millivolts(0)),
                current: Some(BatteryCurrent::Zero),
                voltage_sag: Some(VoltageDelta::from_millivolts(0)),
            },
        ));

        assert_eq!(projection.level, Some(BatteryLevel::from_percent(0)));
        assert_eq!(projection.voltage, Some(Voltage::from_millivolts(0)));
        assert_eq!(projection.current, Some(BatteryCurrent::Zero));
        assert_eq!(projection.voltage_sag, None);
        assert_eq!(
            projection.detail,
            ControllerBatteryEstimateDetail::RecentSag
        );
        assert_eq!(projection.confidence, ControllerBatteryConfidence::Medium);
    }

    #[test]
    fn snapshot_estimate_and_confidence_fallbacks_match_existing_presentation() {
        let estimated = ControllerBatteryProjection::from_input(ControllerBatteryInput::Snapshot(
            ControllerBatteryReadings {
                estimated_level: Some(BatteryLevel::from_percent(42)),
                voltage: Some(Voltage::from_millivolts(84_000)),
                ..ControllerBatteryReadings::default()
            },
        ));
        assert_eq!(estimated.level, Some(BatteryLevel::from_percent(42)));
        assert_eq!(
            estimated.detail,
            ControllerBatteryEstimateDetail::VoltageCurve
        );
        assert_eq!(estimated.confidence, ControllerBatteryConfidence::Low);

        let estimate_without_voltage = ControllerBatteryProjection::from_input(
            ControllerBatteryInput::Snapshot(ControllerBatteryReadings {
                estimated_level: Some(BatteryLevel::from_percent(42)),
                ..ControllerBatteryReadings::default()
            }),
        );
        assert_eq!(
            estimate_without_voltage.detail,
            ControllerBatteryEstimateDetail::Unavailable
        );
        assert_eq!(
            estimate_without_voltage.confidence,
            ControllerBatteryConfidence::Low
        );

        let empty = ControllerBatteryProjection::from_input(ControllerBatteryInput::Snapshot(
            ControllerBatteryReadings::default(),
        ));
        assert_eq!(empty.detail, ControllerBatteryEstimateDetail::Unavailable);
        assert_eq!(empty.confidence, ControllerBatteryConfidence::Unknown);
    }

    #[test]
    fn live_values_prefer_reported_then_estimated_then_snapshot_and_keep_direction() {
        let snapshot = ControllerBatteryReadings {
            reported_level: Some(BatteryLevel::from_percent(30)),
            estimated_level: Some(BatteryLevel::from_percent(31)),
            voltage: Some(Voltage::from_millivolts(80_000)),
            current: Some(BatteryCurrent::current_out(1_200)),
            voltage_sag: Some(VoltageDelta::from_millivolts(-800)),
        };
        let telemetry = ControllerBatteryReadings {
            estimated_level: Some(BatteryLevel::from_percent(60)),
            voltage: Some(Voltage::from_millivolts(0)),
            current: Some(BatteryCurrent::Zero),
            voltage_sag: Some(VoltageDelta::from_millivolts(0)),
            ..ControllerBatteryReadings::default()
        };
        let live = ControllerBatteryProjection::from_input(ControllerBatteryInput::Live {
            telemetry,
            snapshot,
        });

        assert_eq!(live.level, Some(BatteryLevel::from_percent(60)));
        assert_eq!(live.voltage, Some(Voltage::from_millivolts(0)));
        assert_eq!(live.current, Some(BatteryCurrent::Zero));
        assert_eq!(live.voltage_sag, Some(VoltageDelta::from_millivolts(0)));
        assert_eq!(live.detail, ControllerBatteryEstimateDetail::RecentSag);
        assert_eq!(live.confidence, ControllerBatteryConfidence::Medium);

        let reported = ControllerBatteryProjection::from_input(ControllerBatteryInput::Live {
            telemetry: ControllerBatteryReadings {
                reported_level: Some(BatteryLevel::from_percent(55)),
                estimated_level: Some(BatteryLevel::from_percent(56)),
                ..ControllerBatteryReadings::default()
            },
            snapshot,
        });
        assert_eq!(reported.level, Some(BatteryLevel::from_percent(55)));

        let snapshot_fallback =
            ControllerBatteryProjection::from_input(ControllerBatteryInput::Live {
                telemetry: ControllerBatteryReadings::default(),
                snapshot,
            });
        assert_eq!(
            snapshot_fallback.level,
            Some(BatteryLevel::from_percent(30))
        );
        assert_eq!(snapshot_fallback.voltage, snapshot.voltage);
        assert_eq!(snapshot_fallback.current, snapshot.current);
        assert_eq!(snapshot_fallback.voltage_sag, None);
        assert_eq!(
            snapshot_fallback.detail,
            ControllerBatteryEstimateDetail::Unavailable
        );
        assert_eq!(
            snapshot_fallback.confidence,
            ControllerBatteryConfidence::Unknown
        );

        let snapshot_level_with_live_sag =
            ControllerBatteryProjection::from_input(ControllerBatteryInput::Live {
                telemetry: ControllerBatteryReadings {
                    voltage_sag: Some(VoltageDelta::from_millivolts(-500)),
                    ..ControllerBatteryReadings::default()
                },
                snapshot,
            });
        assert_eq!(
            snapshot_level_with_live_sag.level,
            Some(BatteryLevel::from_percent(30))
        );
        assert_eq!(
            snapshot_level_with_live_sag.confidence,
            ControllerBatteryConfidence::Unknown
        );
    }

    #[test]
    fn each_projection_uses_only_its_current_input() {
        let with_values = ControllerBatteryProjection::from_input(
            ControllerBatteryInput::Snapshot(ControllerBatteryReadings {
                reported_level: Some(BatteryLevel::from_percent(77)),
                voltage: Some(Voltage::from_millivolts(117_000)),
                current: Some(BatteryCurrent::current_in(900)),
                voltage_sag: Some(VoltageDelta::from_millivolts(-1_200)),
                ..ControllerBatteryReadings::default()
            }),
        );
        let empty = ControllerBatteryProjection::from_input(ControllerBatteryInput::Snapshot(
            ControllerBatteryReadings::default(),
        ));

        assert_eq!(with_values.level, Some(BatteryLevel::from_percent(77)));
        assert_eq!(with_values.current, Some(BatteryCurrent::current_in(900)));
        assert_eq!(empty.level, None);
        assert_eq!(empty.voltage, None);
        assert_eq!(empty.current, None);
        assert_eq!(empty.voltage_sag, None);
        assert_eq!(empty.confidence, ControllerBatteryConfidence::Unknown);
    }
}
