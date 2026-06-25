use cutout_core::{Capacity, CellVoltage, PackSeriesCells, Percent, Voltage};

/// A measured voltage/percentage point for a battery profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryVoltagePoint {
    /// Single-cell voltage.
    pub cell_voltage: CellVoltage,

    /// Battery percentage associated with the voltage.
    pub percent: Percent,
}

/// Single-cell voltage-to-percentage profile for a known battery chemistry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryVoltageProfile {
    /// Human-readable cell model.
    pub cell_model: &'static str,

    /// Nominal cell capacity.
    pub nominal_capacity: Capacity,

    /// Ordered single-cell voltage curve from empty to full.
    pub points: &'static [BatteryVoltagePoint],
}

impl BatteryVoltageProfile {
    /// Estimates battery percentage from pack voltage and series cell count.
    #[must_use]
    pub fn estimate_percent_from_pack_voltage(
        self,
        pack_voltage: Voltage,
        series_cells: PackSeriesCells,
    ) -> Percent {
        self.estimate_percent_from_cell_voltage(normalize_cell_voltage(pack_voltage, series_cells))
    }

    /// Estimates battery percentage from a single-cell voltage.
    #[must_use]
    pub fn estimate_percent_from_cell_voltage(self, cell_voltage: CellVoltage) -> Percent {
        let Some(first) = self.points.first() else {
            return Percent::from_percent(0);
        };
        if cell_voltage.as_microvolts() <= first.cell_voltage.as_microvolts() {
            return first.percent;
        }

        for window in self.points.windows(2) {
            let [low, high] = window else {
                continue;
            };
            if cell_voltage.as_microvolts() <= high.cell_voltage.as_microvolts() {
                return interpolate_percent(cell_voltage, *low, *high);
            }
        }

        self.points
            .last()
            .map_or(Percent::from_percent(0), |point| point.percent)
    }
}

/// Sticker-backed Samsung 50S single-cell voltage curve.
pub const SAMSUNG_50S_CELL_POINTS: [BatteryVoltagePoint; 8] = [
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(3_033_333),
        percent: Percent::from_percent(0),
    },
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(3_200_000),
        percent: Percent::from_percent(7),
    },
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(3_333_333),
        percent: Percent::from_percent(15),
    },
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(3_433_333),
        percent: Percent::from_percent(25),
    },
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(3_566_667),
        percent: Percent::from_percent(40),
    },
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(3_733_333),
        percent: Percent::from_percent(60),
    },
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(3_866_667),
        percent: Percent::from_percent(75),
    },
    BatteryVoltagePoint {
        cell_voltage: CellVoltage::from_microvolts(4_200_000),
        percent: Percent::from_percent(100),
    },
];

/// Samsung 50S single-cell profile.
pub const SAMSUNG_50S_PROFILE: BatteryVoltageProfile = BatteryVoltageProfile {
    cell_model: "Samsung 50S",
    nominal_capacity: Capacity::from_milliamp_hours(5_000),
    points: &SAMSUNG_50S_CELL_POINTS,
};

fn normalize_cell_voltage(pack_voltage: Voltage, series_cells: PackSeriesCells) -> CellVoltage {
    let series_cells = i32::from(series_cells.get());
    if series_cells <= 0 {
        return CellVoltage::from_microvolts(0);
    }

    CellVoltage::from_microvolts(
        ((pack_voltage.as_millivolts() * 1_000) + (series_cells / 2)) / series_cells,
    )
}

fn interpolate_percent(
    cell_voltage: CellVoltage,
    low: BatteryVoltagePoint,
    high: BatteryVoltagePoint,
) -> Percent {
    let cell_voltage_uv = cell_voltage.as_microvolts();
    let low_uv = low.cell_voltage.as_microvolts();
    let high_uv = high.cell_voltage.as_microvolts();
    let voltage_span = high_uv - low_uv;
    if voltage_span <= 0 {
        return low.percent;
    }

    let percent_span = i32::from(high.percent.get()) - i32::from(low.percent.get());
    let numerator = (cell_voltage_uv - low_uv) * percent_span;
    Percent::from_percent(
        u8::try_from(
            i32::from(low.percent.get()) + ((numerator + (voltage_span / 2)) / voltage_span),
        )
        .unwrap_or_else(|_| high.percent.get()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn pct(value: u8) -> Percent {
        Percent::from_percent(value)
    }

    const fn series(value: u8) -> PackSeriesCells {
        PackSeriesCells::new(value)
    }

    #[test]
    fn samsung_50s_profile_uses_single_cell_curve_points() {
        for point in SAMSUNG_50S_CELL_POINTS {
            assert_eq!(
                SAMSUNG_50S_PROFILE.estimate_percent_from_cell_voltage(point.cell_voltage),
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

            assert!(low.cell_voltage.as_microvolts() < high.cell_voltage.as_microvolts());
            assert!(low.percent.get() < high.percent.get());
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
                SAMSUNG_50S_PROFILE.estimate_percent_from_pack_voltage(
                    Voltage::from_millivolts(pack_mv),
                    series(30),
                ),
                pct(percent)
            );
        }
    }

    #[test]
    fn samsung_50s_cell_curve_stores_cell_voltages_not_pack_voltages() {
        assert!(
            SAMSUNG_50S_CELL_POINTS
                .iter()
                .all(|point| (3_000_000..=4_300_000).contains(&point.cell_voltage.as_microvolts()))
        );
    }

    #[test]
    fn samsung_50s_profile_normalizes_pack_voltage_to_cell_voltage() {
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(91_000), series(30),),
            pct(0)
        );
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(126_000), series(30),),
            pct(100)
        );
    }

    #[test]
    fn samsung_50s_profile_uses_series_cells_as_voltage_multiplier() {
        let thirty_series = SAMSUNG_50S_PROFILE
            .estimate_percent_from_pack_voltage(Voltage::from_millivolts(108_000), series(30));
        let thirty_six_series = SAMSUNG_50S_PROFILE
            .estimate_percent_from_pack_voltage(Voltage::from_millivolts(108_000), series(36));

        assert!(thirty_series > thirty_six_series);
    }

    #[test]
    fn samsung_50s_profile_estimates_same_percent_for_equivalent_series_packs() {
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(107_950), series(30),),
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(129_540), series(36),)
        );
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(151_130), series(42),),
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(107_950), series(30),)
        );
    }

    #[test]
    fn samsung_50s_profile_interpolates_midpoint_between_adjacent_cell_points() {
        let low = SAMSUNG_50S_CELL_POINTS[3];
        let high = SAMSUNG_50S_CELL_POINTS[4];
        let midpoint_uv =
            (low.cell_voltage.as_microvolts() + high.cell_voltage.as_microvolts()) / 2;

        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_cell_voltage(CellVoltage::from_microvolts(midpoint_uv)),
            pct(33)
        );
    }

    #[test]
    fn samsung_50s_profile_estimates_are_monotonic_across_pack_range() {
        let mut previous = pct(0);
        for pack_mv in (91_000..=126_000).step_by(250) {
            let percent = SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(pack_mv), series(30));
            assert!(percent >= previous);
            previous = percent;
        }
    }

    #[test]
    fn samsung_50s_profile_does_not_encode_parallel_count() {
        assert_eq!(SAMSUNG_50S_PROFILE.cell_model, "Samsung 50S");
        assert_eq!(
            SAMSUNG_50S_PROFILE.nominal_capacity,
            Capacity::from_milliamp_hours(5_000)
        );
        assert_eq!(SAMSUNG_50S_PROFILE.points.len(), 8);
    }

    #[test]
    fn samsung_50s_profile_interpolates_between_sticker_points() {
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(107_950), series(30),),
            pct(44)
        );
    }

    #[test]
    fn samsung_50s_profile_interpolates_single_cell_voltages() {
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_cell_voltage(CellVoltage::from_microvolts(3_598_333)),
            pct(44)
        );
    }

    #[test]
    fn samsung_50s_profile_treats_zero_series_as_empty() {
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(107_950), series(0),),
            pct(0)
        );
    }

    #[test]
    fn samsung_50s_profile_clamps_to_voltage_range() {
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(90_999), series(30),),
            pct(0)
        );
        assert_eq!(
            SAMSUNG_50S_PROFILE
                .estimate_percent_from_pack_voltage(Voltage::from_millivolts(126_001), series(30),),
            pct(100)
        );
    }
}
