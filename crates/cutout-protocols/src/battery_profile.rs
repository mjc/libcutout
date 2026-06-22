/// A measured voltage/percentage point for a battery profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryVoltagePoint {
    /// Single-cell voltage in microvolts.
    pub cell_uv: i32,

    /// Battery percentage associated with the voltage.
    pub percent: u8,
}

/// Single-cell voltage-to-percentage profile for a known battery chemistry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryVoltageProfile {
    /// Human-readable cell model.
    pub cell_model: &'static str,

    /// Nominal cell capacity in milliamp-hours.
    pub nominal_capacity_mah: u16,

    /// Ordered single-cell voltage curve from empty to full.
    pub points: &'static [BatteryVoltagePoint],
}

impl BatteryVoltageProfile {
    /// Estimates battery percentage from pack voltage and series cell count.
    #[must_use]
    pub fn estimate_percent_from_pack_voltage(self, pack_voltage_mv: i32, series_cells: u8) -> u8 {
        self.estimate_percent_from_cell_voltage(normalize_cell_voltage_uv(
            pack_voltage_mv,
            series_cells,
        ))
    }

    /// Estimates battery percentage from a single-cell voltage.
    #[must_use]
    pub fn estimate_percent_from_cell_voltage(self, cell_voltage_uv: i32) -> u8 {
        let Some(first) = self.points.first() else {
            return 0;
        };
        if cell_voltage_uv <= first.cell_uv {
            return first.percent;
        }

        for window in self.points.windows(2) {
            let [low, high] = window else {
                continue;
            };
            if cell_voltage_uv <= high.cell_uv {
                return interpolate_percent(cell_voltage_uv, *low, *high);
            }
        }

        self.points.last().map_or(0, |point| point.percent)
    }
}

/// Sticker-backed Samsung 50S single-cell voltage curve.
pub const SAMSUNG_50S_CELL_POINTS: [BatteryVoltagePoint; 8] = [
    BatteryVoltagePoint {
        cell_uv: 3_033_333,
        percent: 0,
    },
    BatteryVoltagePoint {
        cell_uv: 3_200_000,
        percent: 7,
    },
    BatteryVoltagePoint {
        cell_uv: 3_333_333,
        percent: 15,
    },
    BatteryVoltagePoint {
        cell_uv: 3_433_333,
        percent: 25,
    },
    BatteryVoltagePoint {
        cell_uv: 3_566_667,
        percent: 40,
    },
    BatteryVoltagePoint {
        cell_uv: 3_733_333,
        percent: 60,
    },
    BatteryVoltagePoint {
        cell_uv: 3_866_667,
        percent: 75,
    },
    BatteryVoltagePoint {
        cell_uv: 4_200_000,
        percent: 100,
    },
];

/// Samsung 50S single-cell profile.
pub const SAMSUNG_50S_PROFILE: BatteryVoltageProfile = BatteryVoltageProfile {
    cell_model: "Samsung 50S",
    nominal_capacity_mah: 5_000,
    points: &SAMSUNG_50S_CELL_POINTS,
};

fn normalize_cell_voltage_uv(pack_voltage_mv: i32, series_cells: u8) -> i32 {
    let series_cells = i32::from(series_cells);
    if series_cells <= 0 {
        return 0;
    }

    ((pack_voltage_mv * 1_000) + (series_cells / 2)) / series_cells
}

fn interpolate_percent(
    cell_voltage_uv: i32,
    low: BatteryVoltagePoint,
    high: BatteryVoltagePoint,
) -> u8 {
    let voltage_span = high.cell_uv - low.cell_uv;
    if voltage_span <= 0 {
        return low.percent;
    }

    let percent_span = i32::from(high.percent) - i32::from(low.percent);
    let numerator = (cell_voltage_uv - low.cell_uv) * percent_span;
    u8::try_from(i32::from(low.percent) + ((numerator + (voltage_span / 2)) / voltage_span))
        .unwrap_or(high.percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samsung_50s_profile_uses_single_cell_curve_points() {
        for point in SAMSUNG_50S_CELL_POINTS {
            assert_eq!(
                SAMSUNG_50S_PROFILE.estimate_percent_from_cell_voltage(point.cell_uv),
                point.percent
            );
        }
    }

    #[test]
    fn samsung_50s_cell_curve_is_strictly_increasing() {
        for window in SAMSUNG_50S_CELL_POINTS.windows(2) {
            let [low, high] = window else {
                continue;
            };

            assert!(low.cell_uv < high.cell_uv);
            assert!(low.percent < high.percent);
        }
    }

    #[test]
    fn samsung_50s_cell_curve_matches_sticker_voltage_points_for_30s_pack() {
        let sticker_points = [
            (91_000, 0),
            (96_000, 7),
            (100_000, 15),
            (103_000, 25),
            (107_000, 40),
            (112_000, 60),
            (116_000, 75),
            (126_000, 100),
        ];

        for (pack_mv, percent) in sticker_points {
            assert_eq!(
                SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(pack_mv, 30),
                percent
            );
        }
    }

    #[test]
    fn samsung_50s_cell_curve_stores_cell_voltages_not_pack_voltages() {
        assert!(
            SAMSUNG_50S_CELL_POINTS
                .iter()
                .all(|point| (3_000_000..=4_300_000).contains(&point.cell_uv))
        );
    }

    #[test]
    fn samsung_50s_profile_normalizes_pack_voltage_to_cell_voltage() {
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(91_000, 30),
            0
        );
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(126_000, 30),
            100
        );
    }

    #[test]
    fn samsung_50s_profile_uses_series_cells_as_voltage_multiplier() {
        let thirty_series = SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(108_000, 30);
        let thirty_six_series = SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(108_000, 36);

        assert!(thirty_series > thirty_six_series);
    }

    #[test]
    fn samsung_50s_profile_estimates_same_percent_for_equivalent_series_packs() {
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(107_950, 30),
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(129_540, 36)
        );
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(151_130, 42),
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(107_950, 30)
        );
    }

    #[test]
    fn samsung_50s_profile_interpolates_midpoint_between_adjacent_cell_points() {
        let low = SAMSUNG_50S_CELL_POINTS[3];
        let high = SAMSUNG_50S_CELL_POINTS[4];
        let midpoint_uv = (low.cell_uv + high.cell_uv) / 2;

        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_cell_voltage(midpoint_uv),
            33
        );
    }

    #[test]
    fn samsung_50s_profile_estimates_are_monotonic_across_pack_range() {
        let mut previous = 0;
        for pack_mv in (91_000..=126_000).step_by(250) {
            let percent = SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(pack_mv, 30);
            assert!(percent >= previous);
            previous = percent;
        }
    }

    #[test]
    fn samsung_50s_profile_does_not_encode_parallel_count() {
        assert_eq!(SAMSUNG_50S_PROFILE.cell_model, "Samsung 50S");
        assert_eq!(SAMSUNG_50S_PROFILE.nominal_capacity_mah, 5_000);
        assert_eq!(SAMSUNG_50S_PROFILE.points.len(), 8);
    }

    #[test]
    fn samsung_50s_profile_interpolates_between_sticker_points() {
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(107_950, 30),
            44
        );
    }

    #[test]
    fn samsung_50s_profile_interpolates_single_cell_voltages() {
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_cell_voltage(3_598_333),
            44
        );
    }

    #[test]
    fn samsung_50s_profile_treats_zero_series_as_empty() {
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(107_950, 0),
            0
        );
    }

    #[test]
    fn samsung_50s_profile_clamps_to_voltage_range() {
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(90_999, 30),
            0
        );
        assert_eq!(
            SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(126_001, 30),
            100
        );
    }
}
