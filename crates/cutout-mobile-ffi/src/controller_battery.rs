//! Native adapters for the shared controller-only battery presentation.

use crate::{BatteryCurrent, BatteryLevel, Voltage, VoltageDelta};

/// Observed values from one live controller or retained battery snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobileControllerBatteryReadingsDto {
    /// Battery percentage reported by the device.
    pub reported_level: Option<BatteryLevel>,
    /// Battery percentage produced by an estimator.
    pub estimated_level: Option<BatteryLevel>,
    /// Pack voltage.
    pub voltage: Option<Voltage>,
    /// Battery current with normalized physical direction.
    pub current: Option<BatteryCurrent>,
    /// Produced voltage sag; a present zero remains a valid observation.
    pub voltage_sag: Option<VoltageDelta>,
}

impl From<MobileControllerBatteryReadingsDto> for cutout_core::ControllerBatteryReadings {
    fn from(value: MobileControllerBatteryReadingsDto) -> Self {
        Self {
            reported_level: value
                .reported_level
                .map(|level| cutout_core::BatteryLevel::from_percent(level.value)),
            estimated_level: value
                .estimated_level
                .map(|level| cutout_core::BatteryLevel::from_percent(level.value)),
            voltage: value
                .voltage
                .map(|voltage| cutout_core::Voltage::from_millivolts(voltage.value)),
            current: value.current.map(Into::into),
            voltage_sag: value
                .voltage_sag
                .map(|sag| cutout_core::VoltageDelta::from_millivolts(sag.value)),
        }
    }
}

/// A live context remains distinct from a snapshot even when live readings are absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileControllerBatteryInputDto {
    /// No live ride context is available.
    Snapshot {
        readings: MobileControllerBatteryReadingsDto,
    },
    /// Live evidence determines confidence; retained readings may fill displayed values.
    Live {
        /// Current telemetry, including explicit absence.
        telemetry: MobileControllerBatteryReadingsDto,
        /// Retained battery readings used only as the shared projection permits.
        snapshot: MobileControllerBatteryReadingsDto,
    },
}

/// Basis of the controller-only battery estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileControllerBatteryEstimateDetailDto {
    /// Live voltage and sag, or snapshot voltage and current, are available.
    RecentSag,
    /// Voltage is available without sag evidence.
    VoltageCurve,
    /// No estimate basis is available.
    Unavailable,
}

/// Confidence in a controller-only estimate, never proof of individual cell health.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileControllerBatteryConfidenceDto {
    /// The controller evidence includes recent load/sag information.
    Medium,
    /// Only limited controller evidence is available.
    Low,
    /// No usable controller estimate evidence is available.
    Unknown,
}

/// Rust-selected readings and evidence classification for both native Pack screens.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileControllerBatteryProjectionDto {
    /// Selected controller percentage, falling back to retained battery energy.
    pub level: Option<BatteryLevel>,
    /// Selected pack voltage.
    pub voltage: Option<Voltage>,
    /// Selected current, preserving zero and unknown direction.
    pub current: Option<BatteryCurrent>,
    /// Live sag evidence only.
    pub voltage_sag: Option<VoltageDelta>,
    /// Evidence basis to localize.
    pub detail: MobileControllerBatteryEstimateDetailDto,
    /// Evidence confidence to localize.
    pub confidence: MobileControllerBatteryConfidenceDto,
}

/// Projects controller-only battery readings without native fallback/confidence policy.
#[uniffi::export]
#[must_use]
pub fn mobile_controller_battery_projection(
    input: MobileControllerBatteryInputDto,
) -> MobileControllerBatteryProjectionDto {
    let input = match input {
        MobileControllerBatteryInputDto::Snapshot { readings } => {
            cutout_core::ControllerBatteryInput::Snapshot(readings.into())
        }
        MobileControllerBatteryInputDto::Live {
            telemetry,
            snapshot,
        } => cutout_core::ControllerBatteryInput::Live {
            telemetry: telemetry.into(),
            snapshot: snapshot.into(),
        },
    };
    let projection = cutout_core::ControllerBatteryProjection::from_input(input);
    MobileControllerBatteryProjectionDto {
        level: projection.level.map(|value| BatteryLevel {
            value: value.as_percent(),
        }),
        voltage: projection.voltage.map(|value| Voltage {
            value: value.as_millivolts(),
        }),
        current: projection.current.map(Into::into),
        voltage_sag: projection.voltage_sag.map(|value| VoltageDelta {
            value: value.as_millivolts(),
        }),
        detail: match projection.detail {
            cutout_core::ControllerBatteryEstimateDetail::RecentSag => {
                MobileControllerBatteryEstimateDetailDto::RecentSag
            }
            cutout_core::ControllerBatteryEstimateDetail::VoltageCurve => {
                MobileControllerBatteryEstimateDetailDto::VoltageCurve
            }
            cutout_core::ControllerBatteryEstimateDetail::Unavailable => {
                MobileControllerBatteryEstimateDetailDto::Unavailable
            }
        },
        confidence: match projection.confidence {
            cutout_core::ControllerBatteryConfidence::Medium => {
                MobileControllerBatteryConfidenceDto::Medium
            }
            cutout_core::ControllerBatteryConfidence::Low => {
                MobileControllerBatteryConfidenceDto::Low
            }
            cutout_core::ControllerBatteryConfidence::Unknown => {
                MobileControllerBatteryConfidenceDto::Unknown
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_projection_preserves_zero_and_live_absence() {
        let snapshot = MobileControllerBatteryReadingsDto {
            reported_level: Some(BatteryLevel { value: 45 }),
            voltage: Some(Voltage { value: 96_000 }),
            current: Some(BatteryCurrent::Zero),
            ..Default::default()
        };
        let live = mobile_controller_battery_projection(MobileControllerBatteryInputDto::Live {
            telemetry: MobileControllerBatteryReadingsDto::default(),
            snapshot,
        });
        assert_eq!(live.level, snapshot.reported_level);
        assert_eq!(live.current, Some(BatteryCurrent::Zero));
        assert_eq!(
            live.detail,
            MobileControllerBatteryEstimateDetailDto::Unavailable
        );
        assert_eq!(
            live.confidence,
            MobileControllerBatteryConfidenceDto::Unknown
        );
        let fallback =
            mobile_controller_battery_projection(MobileControllerBatteryInputDto::Snapshot {
                readings: snapshot,
            });
        assert_eq!(
            fallback.confidence,
            MobileControllerBatteryConfidenceDto::Medium
        );
    }
}
