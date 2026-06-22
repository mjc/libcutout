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

    /// Ordered voltage curve from empty to full.
    pub points: &'static [BatteryVoltagePoint],
}

impl BatteryVoltageProfile {
    /// Estimates battery percentage from pack voltage.
    #[must_use]
    pub fn estimate_percent(self, voltage_mv: i32) -> u8 {
        let Some(first) = self.points.first() else {
            return 0;
        };
        if voltage_mv <= first.voltage_mv {
            return first.percent;
        }

        for window in self.points.windows(2) {
            let [low, high] = window else {
                continue;
            };
            if voltage_mv <= high.voltage_mv {
                return interpolate_percent(voltage_mv, *low, *high);
            }
        }

        self.points.last().map_or(0, |point| point.percent)
    }
}

/// Sticker-backed Samsung 50S 30s pack voltage curve.
pub const SAMSUNG_50S_30S_POINTS: [BatteryVoltagePoint; 8] = [
    BatteryVoltagePoint {
        voltage_mv: 91_000,
        percent: 0,
    },
    BatteryVoltagePoint {
        voltage_mv: 96_000,
        percent: 7,
    },
    BatteryVoltagePoint {
        voltage_mv: 100_000,
        percent: 15,
    },
    BatteryVoltagePoint {
        voltage_mv: 103_000,
        percent: 25,
    },
    BatteryVoltagePoint {
        voltage_mv: 107_000,
        percent: 40,
    },
    BatteryVoltagePoint {
        voltage_mv: 112_000,
        percent: 60,
    },
    BatteryVoltagePoint {
        voltage_mv: 116_000,
        percent: 75,
    },
    BatteryVoltagePoint {
        voltage_mv: 126_000,
        percent: 100,
    },
];

/// Samsung 50S pack profile used by NOSFET Aero and other 30s2p EUCs.
pub const SAMSUNG_50S_30S2P_PROFILE: BatteryVoltageProfile = BatteryVoltageProfile {
    cell_model: "Samsung 50S",
    series_cells: 30,
    parallel_cells: 2,
    points: &SAMSUNG_50S_30S_POINTS,
};

fn interpolate_percent(voltage_mv: i32, low: BatteryVoltagePoint, high: BatteryVoltagePoint) -> u8 {
    let voltage_span = high.voltage_mv - low.voltage_mv;
    if voltage_span <= 0 {
        return low.percent;
    }

    let percent_span = i32::from(high.percent) - i32::from(low.percent);
    let numerator = (voltage_mv - low.voltage_mv) * percent_span;
    u8::try_from(i32::from(low.percent) + ((numerator + (voltage_span / 2)) / voltage_span))
        .unwrap_or(high.percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samsung_50s_30s2p_profile_uses_sticker_curve_points() {
        for point in SAMSUNG_50S_30S_POINTS {
            assert_eq!(
                SAMSUNG_50S_30S2P_PROFILE.estimate_percent(point.voltage_mv),
                point.percent
            );
        }
    }

    #[test]
    fn samsung_50s_30s2p_profile_interpolates_between_sticker_points() {
        assert_eq!(SAMSUNG_50S_30S2P_PROFILE.estimate_percent(107_950), 44);
    }

    #[test]
    fn samsung_50s_30s2p_profile_clamps_to_voltage_range() {
        assert_eq!(SAMSUNG_50S_30S2P_PROFILE.estimate_percent(90_999), 0);
        assert_eq!(SAMSUNG_50S_30S2P_PROFILE.estimate_percent(126_001), 100);
    }
}
