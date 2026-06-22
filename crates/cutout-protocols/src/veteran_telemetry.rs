use cutout_core::{Measured, MonotonicMillis, TelemetryDelta};
use thiserror::Error;

use crate::VeteranFrame;

/// Capture-backed minimum pack voltage for a NOSFET Aero 30s pack.
pub const NOSFET_AERO_MIN_VOLTAGE_MV: i32 = 99_180;

/// Capture-backed maximum pack voltage for a NOSFET Aero 30s pack.
pub const NOSFET_AERO_MAX_VOLTAGE_MV: i32 = 123_370;

/// Minimal read-only telemetry decoded from a Veteran/LeaperKim/NOSFET frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranTelemetry {
    /// Pack voltage in millivolts.
    pub voltage_mv: i32,

    /// Cutout-estimated battery percentage from capture-backed pack range.
    pub battery_percent_estimated: u8,
}

impl VeteranTelemetry {
    /// Decodes the verified fixed voltage field from a complete Veteran frame.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranTelemetryError::FrameTooShort`] if the frame does not
    /// contain the fixed voltage field.
    pub fn decode(frame: &VeteranFrame) -> Result<Self, VeteranTelemetryError> {
        let voltage_mv = i32::from(
            read_be_u16(frame.as_slice(), 4).ok_or(VeteranTelemetryError::FrameTooShort)?,
        ) * 10;

        Ok(Self {
            voltage_mv,
            battery_percent_estimated: estimate_nosfet_aero_battery_percent(voltage_mv),
        })
    }

    /// Converts decoded telemetry into the transport-independent telemetry delta.
    #[must_use]
    pub const fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        TelemetryDelta {
            voltage_mv: Some(Measured::reported(self.voltage_mv)),
            battery_percent_estimated: Some(Measured::estimated(self.battery_percent_estimated)),
            ..TelemetryDelta::empty(at_ms)
        }
    }
}

/// Veteran telemetry decode failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VeteranTelemetryError {
    /// Frame ended before the decoded field.
    #[error("Veteran telemetry frame too short")]
    FrameTooShort,
}

/// Estimates Aero battery percent from the capture-backed 30s voltage range.
#[must_use]
pub fn estimate_nosfet_aero_battery_percent(voltage_mv: i32) -> u8 {
    if voltage_mv <= NOSFET_AERO_MIN_VOLTAGE_MV {
        return 0;
    }
    if voltage_mv >= NOSFET_AERO_MAX_VOLTAGE_MV {
        return 100;
    }

    let numerator = (voltage_mv - NOSFET_AERO_MIN_VOLTAGE_MV) * 100;
    let denominator = NOSFET_AERO_MAX_VOLTAGE_MV - NOSFET_AERO_MIN_VOLTAGE_MV;
    u8::try_from(numerator / denominator).unwrap_or(100)
}

fn read_be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let b0 = *bytes.get(offset)?;
    let b1 = *bytes.get(offset + 1)?;
    Some(u16::from_be_bytes([b0, b1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_aero_frame() -> VeteranFrame {
        VeteranFrame::try_from_slice(&hex_literal::hex!(
            "dc5a5c532a7c000000000000ab41001700000cff\
             000000000226021ca8f607801afa000080c80000\
             808080808080022880803080800e310e310e2f0e\
             2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
             310e2e9e05e3ad"
        ))
        .expect("fixture frame is valid")
    }

    #[test]
    fn veteran_telemetry_decodes_live_aero_voltage() {
        let telemetry = VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");

        assert_eq!(telemetry.voltage_mv, 108_760);
    }

    #[test]
    fn veteran_telemetry_estimates_live_aero_battery_percent() {
        let telemetry = VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");

        assert_eq!(telemetry.battery_percent_estimated, 39);
    }

    #[test]
    fn aero_battery_percent_estimate_clamps_to_pack_range() {
        assert_eq!(
            estimate_nosfet_aero_battery_percent(NOSFET_AERO_MIN_VOLTAGE_MV - 1),
            0
        );
        assert_eq!(
            estimate_nosfet_aero_battery_percent(NOSFET_AERO_MAX_VOLTAGE_MV + 1),
            100
        );
    }

    #[test]
    fn veteran_telemetry_maps_voltage_and_estimated_percent_to_delta() {
        let delta = VeteranTelemetry::decode(&live_aero_frame())
            .expect("telemetry decodes")
            .to_delta(42);

        assert_eq!(delta.at_ms, 42);
        assert_eq!(delta.voltage_mv, Some(Measured::reported(108_760)));
        assert_eq!(
            delta.battery_percent_estimated,
            Some(Measured::estimated(39))
        );
    }
}
