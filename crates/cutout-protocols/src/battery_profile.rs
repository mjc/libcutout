/// A measured voltage/percentage point for a battery profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryVoltagePoint {
    /// Pack voltage in millivolts.
    pub voltage_mv: i32,

    /// Battery percentage associated with the voltage.
    pub percent: u8,
}

/// Voltage-to-percentage profile for a known battery pack configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryVoltageProfile {
    /// Human-readable cell model.
    pub cell_model: &'static str,

    /// Series cell count.
    pub series_cells: u8,

    /// Parallel cell count.
    pub parallel_cells: u8,

    /// Empty calibration point.
    pub empty: BatteryVoltagePoint,

    /// Mid-curve calibration point.
    pub midpoint: BatteryVoltagePoint,

    /// Full calibration point.
    pub full: BatteryVoltagePoint,
}

impl BatteryVoltageProfile {
    /// Estimates battery percentage from pack voltage.
    #[must_use]
    pub fn estimate_percent(self, voltage_mv: i32) -> u8 {
        if voltage_mv <= self.empty.voltage_mv {
            return self.empty.percent;
        }
        if voltage_mv >= self.full.voltage_mv {
            return self.full.percent;
        }
        if voltage_mv <= self.midpoint.voltage_mv {
            return interpolate_percent(voltage_mv, self.empty, self.midpoint);
        }

        interpolate_percent(voltage_mv, self.midpoint, self.full)
    }
}

/// Samsung 50S pack profile used by NOSFET Aero and other 30s2p EUCs.
pub const SAMSUNG_50S_30S2P_PROFILE: BatteryVoltageProfile = BatteryVoltageProfile {
    cell_model: "Samsung 50S",
    series_cells: 30,
    parallel_cells: 2,
    empty: BatteryVoltagePoint {
        voltage_mv: 99_180,
        percent: 0,
    },
    midpoint: BatteryVoltagePoint {
        voltage_mv: 107_950,
        percent: 45,
    },
    full: BatteryVoltagePoint {
        voltage_mv: 123_370,
        percent: 100,
    },
};

fn interpolate_percent(voltage_mv: i32, low: BatteryVoltagePoint, high: BatteryVoltagePoint) -> u8 {
    let voltage_span = high.voltage_mv - low.voltage_mv;
    if voltage_span <= 0 {
        return low.percent;
    }

    let percent_span = i32::from(high.percent) - i32::from(low.percent);
    let numerator = (voltage_mv - low.voltage_mv) * percent_span;
    u8::try_from(i32::from(low.percent) + (numerator / voltage_span)).unwrap_or(high.percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samsung_50s_30s2p_profile_uses_hardware_observed_midpoint() {
        assert_eq!(
            SAMSUNG_50S_30S2P_PROFILE
                .estimate_percent(SAMSUNG_50S_30S2P_PROFILE.midpoint.voltage_mv),
            45
        );
    }

    #[test]
    fn samsung_50s_30s2p_profile_clamps_to_voltage_range() {
        assert_eq!(
            SAMSUNG_50S_30S2P_PROFILE
                .estimate_percent(SAMSUNG_50S_30S2P_PROFILE.empty.voltage_mv - 1),
            0
        );
        assert_eq!(
            SAMSUNG_50S_30S2P_PROFILE
                .estimate_percent(SAMSUNG_50S_30S2P_PROFILE.full.voltage_mv + 1),
            100
        );
    }
}
