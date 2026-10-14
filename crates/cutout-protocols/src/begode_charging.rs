use cutout_core::{ChargeMode, Duration, Measured, MonotonicTimestamp, TelemetryDelta, Voltage};

use crate::{BegodeBmsCellPage, BegodeBmsSummary, BegodeFrame};

// Retain telemetry only while speed, selected voltage, and BMS evidence are fresh.
const FRESH: Duration = Duration::from_seconds(5);
// Require one minute without movement before considering charge voltage behavior.
const SETTLE: Duration = Duration::from_minutes(1);
// The net rise must span at least two minutes.
const TREND: Duration = Duration::from_minutes(2);
// Distinct rises closer than this cannot increment the trend evidence.
const INCREASE_SPACING: Duration = Duration::from_seconds(20);
const REQUIRED_RISE_MV: i32 = 200;
const MATERIAL_RISE_MV: i32 = 50;
const STOP_DROP_MV: i32 = 100;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
// ponytail: bank zero represents charge voltage; current aggregation stays in BMS normalization.
enum Source {
    #[default]
    Unknown,
    Placeholders,
    Usable,
}

/// Conservative, estimated Begode charging inference from sustained voltage rise.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BegodeCharging {
    source: Source,
    placeholder_summary_at: Option<MonotonicTimestamp>,
    placeholder_cells_at: Option<MonotonicTimestamp>,
    valid_summary_at: Option<MonotonicTimestamp>,
    real_cells_at: Option<MonotonicTimestamp>,
    last_input_at: Option<MonotonicTimestamp>,
    last_speed_at: Option<MonotonicTimestamp>,
    stationary_since: Option<MonotonicTimestamp>,
    last_voltage_at: Option<MonotonicTimestamp>,
    trend_started_at: Option<MonotonicTimestamp>,
    baseline_mv: Option<i32>,
    peak_mv: Option<i32>,
    last_rise_at: Option<MonotonicTimestamp>,
    rises: u8,
    charging: bool,
}

impl BegodeCharging {
    /// Updates inference from one frame and its corresponding partial telemetry.
    #[must_use]
    pub(crate) fn observe(
        &mut self,
        frame: &BegodeFrame,
        delta: &TelemetryDelta,
        at: MonotonicTimestamp,
    ) -> Measured<ChargeMode> {
        if self.last_input_at.is_some_and(|last| at < last) {
            self.reset();
            return Measured::estimated(ChargeMode::Unknown);
        }
        if self.evidence_stale(at) {
            self.reset_evidence();
        }
        self.expire_trend(at);
        self.last_input_at = Some(at);

        if let Some(speed) = delta.speed {
            self.last_speed_at = Some(at);
            if speed.value.is_moving() {
                self.reset_evidence();
                self.last_input_at = Some(at);
                self.last_speed_at = Some(at);
                return Measured::estimated(ChargeMode::NotCharging);
            }
            if self.stationary_since.is_none() {
                self.stationary_since = Some(at);
            }
        }

        match frame.tag().get() {
            0x00 => {
                if self.source == Source::Placeholders
                    && let Some(voltage) = delta.voltage
                {
                    self.observe_voltage(voltage.value, at);
                }
            }
            0x01 => {
                if let Ok(summary) = BegodeBmsSummary::decode(frame) {
                    self.observe_summary(summary, at);
                }
            }
            0x02 | 0x03 => {
                if let Ok(page) = BegodeBmsCellPage::decode(frame) {
                    self.observe_cells(&page, at);
                }
            }
            _ => {}
        }

        self.refresh_source(at);
        if self.ready(at) {
            self.result(at)
        } else {
            Measured::estimated(ChargeMode::Unknown)
        }
    }

    /// Clears all speed, BMS, and voltage-trend evidence.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Expires stale evidence and returns the current estimate.
    #[must_use]
    pub(crate) fn tick(&mut self, at: MonotonicTimestamp) -> Measured<ChargeMode> {
        if self.last_input_at.is_some_and(|last| at < last) {
            self.reset();
            return Measured::estimated(ChargeMode::Unknown);
        }
        if self.evidence_stale(at) {
            self.reset_evidence();
            self.last_input_at = Some(at);
            return Measured::estimated(ChargeMode::Unknown);
        }
        self.last_input_at = Some(at);
        self.expire_trend(at);
        if self.ready(at) {
            self.result(at)
        } else {
            Measured::estimated(ChargeMode::Unknown)
        }
    }

    /// Returns the first timestamp at which active inferred charging evidence expires.
    pub(crate) fn next_tick_at(&self) -> Option<MonotonicTimestamp> {
        if !self.charging {
            return None;
        }
        let mut oldest = self.last_speed_at?;
        oldest = oldest.min(self.last_voltage_at?);
        match self.source {
            Source::Unknown => return None,
            Source::Placeholders => {
                oldest = oldest.min(self.placeholder_summary_at?);
                oldest = oldest.min(self.placeholder_cells_at?);
            }
            Source::Usable => {
                oldest = oldest.min(self.valid_summary_at?);
                oldest = oldest.min(self.real_cells_at?);
            }
        }
        let evidence_expiry = oldest.saturating_add_duration(Duration::from_milliseconds(
            FRESH.as_milliseconds().saturating_add(1),
        ));
        let trend_expiry = self
            .last_rise_at?
            .saturating_add_duration(Duration::from_milliseconds(
                TREND.as_milliseconds().saturating_add(1),
            ));
        Some(evidence_expiry.min(trend_expiry))
    }

    fn observe_summary(&mut self, summary: BegodeBmsSummary, at: MonotonicTimestamp) {
        if summary.bms_index.get() != 0 {
            return;
        }
        if valid_summary(summary) {
            if self.source != Source::Usable {
                self.reset_trend();
                self.last_voltage_at = None;
            }
            self.valid_summary_at = Some(at);
            self.placeholder_summary_at = None;
            self.source = Source::Usable;
            if self.real_cells_at.is_some_and(|seen| fresh(seen, at)) {
                self.observe_voltage(summary.pack_voltage, at);
            }
        } else if is_placeholder(summary) {
            if self.source == Source::Usable {
                self.reset_trend();
                self.last_voltage_at = None;
                self.real_cells_at = None;
                self.source = Source::Unknown;
            }
            self.placeholder_summary_at = Some(at);
            self.valid_summary_at = None;
        } else {
            self.clear_bms_evidence();
        }
    }

    fn observe_cells(&mut self, page: &BegodeBmsCellPage, at: MonotonicTimestamp) {
        if page.bms_index.get() != 0 {
            return;
        }
        let plausible = page.cell_voltage.iter().all(|voltage| {
            let mv = voltage.as_millivolts();
            mv == 0 || (2_500..=4_500).contains(&mv)
        });
        let has_cells = page
            .cell_voltage
            .iter()
            .any(|voltage| (2_500..=4_500).contains(&voltage.as_millivolts()));
        if plausible && has_cells {
            self.real_cells_at = Some(at);
            if self.source != Source::Usable {
                self.reset_trend();
                self.last_voltage_at = None;
            }
            self.source = Source::Usable;
        } else if page
            .cell_voltage
            .iter()
            .all(|voltage| voltage.as_millivolts() == 0)
        {
            self.placeholder_cells_at = Some(at);
        } else {
            self.clear_bms_evidence();
        }
    }

    fn refresh_source(&mut self, at: MonotonicTimestamp) {
        if self.source == Source::Usable {
            return;
        }
        let placeholder_summary_fresh = self
            .placeholder_summary_at
            .is_some_and(|seen| fresh(seen, at));
        let placeholder_cells_fresh = self
            .placeholder_cells_at
            .is_some_and(|seen| fresh(seen, at));
        if placeholder_summary_fresh && placeholder_cells_fresh {
            self.source = Source::Placeholders;
        } else if self.source == Source::Placeholders {
            self.source = Source::Unknown;
            self.last_voltage_at = None;
            self.reset_trend();
        }
    }

    fn ready(&self, at: MonotonicTimestamp) -> bool {
        self.last_speed_at.is_some_and(|seen| fresh(seen, at))
            && self.last_voltage_at.is_some_and(|seen| fresh(seen, at))
            && match self.source {
                Source::Unknown => false,
                Source::Placeholders => {
                    self.placeholder_summary_at
                        .is_some_and(|seen| fresh(seen, at))
                        && self
                            .placeholder_cells_at
                            .is_some_and(|seen| fresh(seen, at))
                }
                Source::Usable => {
                    self.valid_summary_at.is_some_and(|seen| fresh(seen, at))
                        && self.real_cells_at.is_some_and(|seen| fresh(seen, at))
                }
            }
    }

    fn evidence_stale(&self, at: MonotonicTimestamp) -> bool {
        stale(self.last_speed_at, at)
            || stale(self.last_voltage_at, at)
            || match self.source {
                Source::Unknown => false,
                Source::Placeholders => {
                    stale(self.placeholder_summary_at, at) || stale(self.placeholder_cells_at, at)
                }
                Source::Usable => stale(self.valid_summary_at, at) || stale(self.real_cells_at, at),
            }
    }

    fn observe_voltage(&mut self, voltage: Voltage, at: MonotonicTimestamp) {
        if self.last_voltage_at == Some(at) {
            return;
        }
        let mv = voltage.as_millivolts();
        if mv <= 0 {
            self.reset_trend();
            self.last_voltage_at = Some(at);
            return;
        }
        if self.last_voltage_at.is_some_and(|last| at < last) {
            self.reset_trend();
            return;
        }
        if self
            .stationary_since
            .is_none_or(|start| elapsed(start, at) < SETTLE)
        {
            self.last_voltage_at = Some(at);
            return;
        }
        if self
            .peak_mv
            .is_some_and(|peak| peak.saturating_sub(mv) >= STOP_DROP_MV)
        {
            self.reset_trend();
        }
        if self.baseline_mv.is_none() {
            self.baseline_mv = Some(mv);
            self.trend_started_at = Some(at);
        }
        let peak = self.peak_mv.get_or_insert(mv);
        if mv.saturating_sub(*peak) >= MATERIAL_RISE_MV {
            if self
                .last_rise_at
                .is_none_or(|last| elapsed(last, at) >= INCREASE_SPACING)
            {
                self.rises = self.rises.saturating_add(1);
                self.last_rise_at = Some(at);
            }
            *peak = mv;
        }
        self.last_voltage_at = Some(at);
        if self.rises >= 3
            && self
                .baseline_mv
                .is_some_and(|base| mv.saturating_sub(base) >= REQUIRED_RISE_MV)
            && self
                .trend_started_at
                .is_some_and(|start| elapsed(start, at) >= TREND)
        {
            self.charging = true;
        }
    }

    fn expire_trend(&mut self, at: MonotonicTimestamp) {
        if self.charging
            && self
                .last_rise_at
                .is_some_and(|last| elapsed(last, at) > TREND)
        {
            self.reset_trend();
        }
    }

    fn result(&mut self, at: MonotonicTimestamp) -> Measured<ChargeMode> {
        let settled = self
            .stationary_since
            .is_some_and(|start| elapsed(start, at) >= SETTLE);
        if !settled {
            self.charging = false;
            self.reset_trend();
            return Measured::estimated(ChargeMode::Unknown);
        }
        if self.charging {
            Measured::estimated(ChargeMode::Charging)
        } else {
            Measured::estimated(ChargeMode::Unknown)
        }
    }

    fn reset_trend(&mut self) {
        self.baseline_mv = None;
        self.peak_mv = None;
        self.trend_started_at = None;
        self.last_rise_at = None;
        self.rises = 0;
        self.charging = false;
    }

    fn reset_evidence(&mut self) {
        *self = Self::default();
    }

    fn clear_bms_evidence(&mut self) {
        self.source = Source::Unknown;
        self.placeholder_summary_at = None;
        self.placeholder_cells_at = None;
        self.valid_summary_at = None;
        self.real_cells_at = None;
        self.reset_trend();
    }
}

fn valid_summary(summary: BegodeBmsSummary) -> bool {
    let pack = summary.pack_voltage.as_millivolts();
    let half = summary.half_pack_voltage.as_millivolts();
    (-20_000..=100_000).contains(&summary.temperature_0.as_millicelsius())
        && (-20_000..=100_000).contains(&summary.temperature_1.as_millicelsius())
        && (40_000..=250_000).contains(&pack)
        && half > 0
        && half <= pack
}

fn is_placeholder(summary: BegodeBmsSummary) -> bool {
    summary.current.as_milliamps() == 0
        && summary.temperature_0.as_millicelsius() == 0
        && summary.temperature_1.as_millicelsius() == 5_000_000
        && summary.half_pack_voltage.as_millivolts() == 0
}

fn fresh(seen: MonotonicTimestamp, now: MonotonicTimestamp) -> bool {
    seen <= now && elapsed(seen, now) <= FRESH
}

fn stale(seen: Option<MonotonicTimestamp>, now: MonotonicTimestamp) -> bool {
    seen.is_some_and(|at| !fresh(at, now))
}

const fn elapsed(earlier: MonotonicTimestamp, later: MonotonicTimestamp) -> Duration {
    later.saturating_duration_since(earlier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{Speed, ValueQuality, ValueSource, VerificationStatus};

    fn frame(tag: u8, fields: &[(usize, u16)]) -> BegodeFrame {
        frame_sub(tag, 0, fields)
    }

    fn frame_sub(tag: u8, sub_index: u8, fields: &[(usize, u16)]) -> BegodeFrame {
        let mut bytes = [0; 24];
        bytes[0] = 0x55;
        bytes[1] = 0xaa;
        bytes[18] = tag;
        bytes[19] = sub_index;
        for (offset, value) in fields {
            let [high, low] = value.to_be_bytes();
            bytes[*offset] = high;
            bytes[*offset + 1] = low;
        }
        bytes[20..24].fill(0x5a);
        BegodeFrame::try_from_slice(&bytes).expect("valid test frame")
    }

    fn placeholder_summary() -> BegodeFrame {
        placeholder_summary_bank(0)
    }

    fn placeholder_summary_bank(bank: u8) -> BegodeFrame {
        frame_sub(
            0x01,
            bank.saturating_mul(2).saturating_add(1),
            &[(6, 985), (8, 0), (10, 0), (12, 5_000), (14, 0)],
        )
    }

    fn usable_summary(bank: u8, pack_deci_volts: u16) -> BegodeFrame {
        frame_sub(
            0x01,
            bank.saturating_mul(2),
            &[(6, pack_deci_volts), (8, 0), (10, 25), (12, 25), (14, 420)],
        )
    }

    fn cells(zero: bool) -> BegodeFrame {
        cells_bank(zero, 0)
    }

    fn cells_bank(zero: bool, bank: u8) -> BegodeFrame {
        let mut fields = [(0, 0); 8];
        for (index, (offset, value)) in fields.iter_mut().enumerate() {
            *offset = 2 + index * 2;
            *value = if zero { 0 } else { 3_400 };
        }
        frame(if bank == 0 { 0x02 } else { 0x03 }, &fields)
    }

    fn observation(at: u64, voltage_mv: Option<i32>) -> TelemetryDelta {
        let mut delta = TelemetryDelta::empty(MonotonicTimestamp::new(at));
        delta.speed = Some(Measured::reported(Speed::default()));
        delta.voltage = voltage_mv.map(|mv| Measured::reported(Voltage::from_millivolts(mv)));
        delta
    }

    fn observed(
        state: &mut BegodeCharging,
        frame: &BegodeFrame,
        at: u64,
        voltage_mv: Option<i32>,
    ) -> Measured<ChargeMode> {
        state.observe(
            frame,
            &observation(at, voltage_mv),
            MonotonicTimestamp::new(at),
        )
    }

    fn observe_placeholder_voltage(
        state: &mut BegodeCharging,
        at: u64,
        voltage_mv: i32,
    ) -> Measured<ChargeMode> {
        observed(state, &placeholder_summary(), at, None);
        observed(state, &cells(true), at, None);
        observed(state, &frame(0, &[]), at, Some(voltage_mv))
    }

    #[test]
    fn estimated_modes_never_claim_hardware_verification() {
        let result = BegodeCharging::default().tick(MonotonicTimestamp::new(0));
        assert_eq!(result, Measured::estimated(ChargeMode::Unknown));
        assert_eq!(result.source, ValueSource::Estimated);
        assert_eq!(result.quality, ValueQuality::Inferred);
        assert_eq!(result.verification, VerificationStatus::Inferred);
    }

    #[test]
    fn placeholder_fallback_requires_the_summary_and_zero_cell_page() {
        let mut state = BegodeCharging::default();
        observed(&mut state, &placeholder_summary(), 0, None);
        let result = observed(&mut state, &frame(0, &[]), 1, Some(80_000));
        assert_eq!(result.value, ChargeMode::Unknown);

        observed(&mut state, &cells(true), 2, None);
        let result = observed(&mut state, &frame(0, &[]), 3, Some(80_000));
        assert_eq!(result.value, ChargeMode::Unknown);

        let mut different_bank = BegodeCharging::default();
        observed(&mut different_bank, &placeholder_summary(), 0, None);
        observed(&mut different_bank, &frame(0x03, &[]), 1, None);
        let result = observed(&mut different_bank, &frame(0, &[]), 2, Some(80_000));
        assert_eq!(result.value, ChargeMode::Unknown);
    }

    #[test]
    fn placeholder_voltage_requires_a_sustained_rise_after_stationary_settling() {
        let mut state = BegodeCharging::default();
        for at in (0_u64..=240_000).step_by(2_000) {
            let voltage = if at >= 120_000 {
                80_210
            } else if at >= 100_000 {
                80_130
            } else if at >= 80_000 {
                80_060
            } else {
                80_000
            };
            observed(&mut state, &placeholder_summary(), at, None);
            observed(&mut state, &placeholder_summary_bank(1), at, None);
            observed(&mut state, &cells_bank(true, 1), at, None);
            observed(&mut state, &cells(true), at, None);
            let result = observed(&mut state, &frame(0, &[]), at, Some(voltage));
            if at < 180_000 {
                assert_ne!(result.value, ChargeMode::Charging, "at {at} ms");
            }
            if at == 60_000 {
                assert_eq!(result.value, ChargeMode::Unknown);
            }
            if at == 180_000 {
                assert_eq!(result.value, ChargeMode::Charging);
                assert_eq!(state.next_tick_at(), Some(MonotonicTimestamp::new(185_001)));
                let mut dropped = state;
                observed(&mut dropped, &placeholder_summary(), 182_000, None);
                observed(&mut dropped, &cells(true), 182_000, None);
                assert_eq!(
                    observed(&mut dropped, &frame(0, &[]), 182_000, Some(80_100)).value,
                    ChargeMode::Unknown
                );
                let mut transitioned = state;
                observed(&mut transitioned, &usable_summary(0, 840), 182_000, None);
                assert_eq!(
                    observed(&mut transitioned, &cells(false), 182_000, None).value,
                    ChargeMode::Unknown
                );
            }
            if at == 240_000 {
                assert_eq!(result.value, ChargeMode::Charging);
                assert_eq!(state.next_tick_at(), Some(MonotonicTimestamp::new(240_001)));
                assert_eq!(
                    state.tick(MonotonicTimestamp::new(240_001)).value,
                    ChargeMode::Unknown
                );
                assert_eq!(state.next_tick_at(), None);
            }
        }
    }

    #[test]
    fn usable_bms_voltage_can_infer_only_from_its_matching_real_cell_bank() {
        let mut state = BegodeCharging::default();
        for at in (0_u64..=180_000).step_by(2_000) {
            let voltage = if at >= 120_000 {
                843
            } else if at >= 100_000 {
                842
            } else if at >= 80_000 {
                841
            } else {
                840
            };
            observed(&mut state, &usable_summary(0, voltage), at, None);
            observed(&mut state, &usable_summary(1, 1_000), at, None);
            observed(&mut state, &cells_bank(false, 1), at, None);
            let result = observed(&mut state, &cells(false), at, None);
            if at == 180_000 {
                assert_eq!(result.value, ChargeMode::Charging);
            }
        }

        let mut high_voltage = BegodeCharging::default();
        observed(&mut high_voltage, &usable_summary(0, 1_680), 0, None);
        observed(&mut high_voltage, &cells(false), 1, None);
        assert_eq!(
            high_voltage.valid_summary_at,
            Some(MonotonicTimestamp::new(0))
        );

        let mut other_bank = BegodeCharging::default();
        for at in (0_u64..=180_000).step_by(2_000) {
            observed(&mut other_bank, &usable_summary(1, 1_000), at, None);
            let result = observed(
                &mut other_bank,
                &frame_sub(0x03, 0, &[(2, 3_400)]),
                at,
                None,
            );
            assert_eq!(result.value, ChargeMode::Unknown);
        }
    }

    #[test]
    fn movement_and_stale_speed_clear_the_inference() {
        let mut state = BegodeCharging::default();
        let live = frame(0, &[]);
        let mut delta = observation(0, Some(80_000));
        delta.speed = Some(Measured::reported(Speed::from_millimetres_per_second(
            2_000,
        )));
        assert_eq!(
            state
                .observe(&live, &delta, MonotonicTimestamp::new(0))
                .value,
            ChargeMode::NotCharging
        );
        assert_eq!(
            state.tick(MonotonicTimestamp::new(5_001)).value,
            ChargeMode::Unknown
        );
    }

    #[test]
    fn one_rebound_or_small_voltage_noise_never_establishes_charging() {
        let mut rebound = BegodeCharging::default();
        for at in (0_u64..=180_000).step_by(2_000) {
            let voltage = if (80_000..180_000).contains(&at) {
                80_300
            } else {
                80_000
            };
            let result = observe_placeholder_voltage(&mut rebound, at, voltage);
            assert_ne!(result.value, ChargeMode::Charging, "single rise at {at} ms");
        }

        let mut noise = BegodeCharging::default();
        for at in (0_u64..=200_000).step_by(2_000) {
            let voltage = if at / 2_000 % 2 == 0 { 80_040 } else { 80_000 };
            let result = observe_placeholder_voltage(&mut noise, at, voltage);
            assert_ne!(result.value, ChargeMode::Charging, "small steps at {at} ms");
        }
    }

    #[test]
    fn live_a_before_first_bms_markers_does_not_erase_partial_marker_evidence() {
        let mut state = BegodeCharging::default();
        let live = frame(0, &[]);
        observed(&mut state, &live, 0, Some(80_000));
        observed(&mut state, &placeholder_summary(), 0, None);
        observed(&mut state, &cells(true), 0, None);
        let result = observed(&mut state, &live, 2_000, Some(80_000));
        assert_eq!(result.value, ChargeMode::Unknown);
        assert_eq!(state.source, Source::Placeholders);
        assert_eq!(state.last_voltage_at, Some(MonotonicTimestamp::new(2_000)));
    }
}
