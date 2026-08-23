use core::ops::RangeInclusive;

use cutout_core::{
    BatteryCurrent, BatteryLevel, Capacity, DiagnosticDetail, DiagnosticReadback,
    DiagnosticSeverity, Distance, Duration, DutyCycle, Energy, LightState, Measured,
    MonotonicTimestamp, ParallelCount, PhaseCurrent, Power, ProtocolTag, RawFieldValue,
    ReadOnlyResponse, SeriesCount, SettingsEntry, SettingsReadback, Speed, TelemetryDelta,
    Temperature, ValueQuality, ValueSource, VerificationStatus, Voltage, WireVoltage,
};
use thiserror::Error;

use crate::{
    BegodeFrame,
    parser::{ParserCursor, ParserOffset},
    util::u64_to_i64_saturating,
};

const SERIES_CELLS_20: SeriesCount = SeriesCount::new(20);
const SERIES_CELLS_24: SeriesCount = SeriesCount::new(24);
#[cfg(test)]
const PARALLEL_PACKS_1: ParallelCount = ParallelCount::new(1);
const PARALLEL_PACKS_2: ParallelCount = ParallelCount::new(2);

/// Begode speed/distance unit mode inferred from Live B settings bit 0.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BegodeUnitMode {
    /// Wheel wire values are already metric.
    #[default]
    Metric,

    /// Wheel wire values are imperial-scaled and must be converted to metric.
    Imperial,
}

/// Raw Begode settings bitfield.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeSettingsBits(u16);

impl BegodeSettingsBits {
    /// Creates a raw Begode settings bitfield.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the raw settings bitfield.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Raw Begode LED mode.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeLedMode(u8);

impl BegodeLedMode {
    /// Creates a raw Begode LED mode.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the raw LED mode.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Raw Begode alert bitfield.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeAlertFlags(u8);

impl BegodeAlertFlags {
    /// Creates a raw Begode alert bitfield.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the raw alert bitfield.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Begode light mode stored in the low two bits of the source byte.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeLightMode(u8);

impl BegodeLightMode {
    /// Creates a Begode light mode, preserving only the encoded low two bits.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value & 0x03)
    }

    /// Returns the low two light-mode bits.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Maps the documented light-mode values to the shared typed state.
    #[must_use]
    pub const fn light_state(self) -> Option<LightState> {
        match self.0 {
            0 => Some(LightState::Off),
            1 => Some(LightState::On),
            2 => Some(LightState::Strobe),
            _ => None,
        }
    }
}

impl BegodeUnitMode {
    const fn from_settings_bits(settings_bits: BegodeSettingsBits) -> Self {
        if settings_bits.get() & 0x0001 == 0 {
            Self::Metric
        } else {
            Self::Imperial
        }
    }

    fn distance_from_wire(self, distance_m: u32) -> Distance {
        match self {
            Self::Metric => Distance::from_metres(u64::from(distance_m)),
            Self::Imperial => Distance::from_milli_miles(distance_m),
        }
    }

    fn speed(self, metric_speed: Speed) -> Speed {
        match self {
            Self::Metric => metric_speed,
            Self::Imperial => Speed::from_milli_kmh_scaled(metric_speed.as_milli_kmh(), 1_609_344),
        }
    }

    fn speed_limit(self, raw_speed: u16) -> Speed {
        match self {
            Self::Metric => Speed::from_kmh(u64::from(raw_speed)),
            Self::Imperial => Speed::from_mph_floor_kmh(u64::from(raw_speed)),
        }
    }

    fn distance(self, metric_distance: Distance) -> Distance {
        self.distance_from_wire(u32::try_from(metric_distance.as_metres()).unwrap_or(u32::MAX))
    }
}

/// Stateful Begode telemetry normalizer for cross-frame unit-mode evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeTelemetryContext {
    unit_mode: BegodeUnitMode,
}

impl BegodeTelemetryContext {
    /// Returns the currently inferred unit mode.
    #[must_use]
    pub const fn unit_mode(self) -> BegodeUnitMode {
        self.unit_mode
    }

    /// Resets cross-frame evidence to the conservative metric default.
    pub const fn reset(&mut self) {
        self.unit_mode = BegodeUnitMode::Metric;
    }

    /// Updates cross-frame evidence from a decoded Live B frame.
    pub fn observe_live_b(&mut self, telemetry: BegodeLiveBTelemetry) {
        self.unit_mode = telemetry.unit_mode();
    }

    /// Converts decoded Live A fields into a normalized telemetry delta.
    #[must_use]
    pub fn live_a_to_delta(
        self,
        telemetry: BegodeLiveATelemetry,
        at_ms: MonotonicTimestamp,
    ) -> TelemetryDelta {
        telemetry.to_delta_with_units(at_ms, self.unit_mode)
    }

    /// Converts decoded Live B fields into a normalized telemetry delta.
    #[must_use]
    pub fn live_b_to_delta(
        self,
        telemetry: BegodeLiveBTelemetry,
        at_ms: MonotonicTimestamp,
    ) -> TelemetryDelta {
        telemetry.to_delta_with_units(at_ms)
    }

    /// Converts decoded Live B settings into normalized read-only settings.
    #[must_use]
    pub fn live_b_to_settings_response(self, telemetry: BegodeLiveBTelemetry) -> ReadOnlyResponse {
        telemetry.to_settings_response_with_units()
    }
}

/// Begode/Gotway pack voltage profile used to scale raw 67.2 V-equivalent telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodePackVoltageProfile {
    /// Generic 84 V full-charge profile for 20S Begode-family packs.
    Begode84VFullCharge,

    /// Generic 100.8 V full-charge profile for 24S Begode-family packs.
    Begode100VFullCharge,
}

/// Explicit voltage profile for the current Begode Falcon hardware target.
///
/// This is not generic Falcon identity evidence and does not imply capacity.
pub const BEGODE_FALCON_TARGET_VOLTAGE_PROFILE: BegodePackVoltageProfile =
    BegodePackVoltageProfile::Begode100VFullCharge;

/// Explicit evidence used to select a Begode pack voltage profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeVoltageEvidence {
    /// A capture/app/label explicitly identifies an 84 V class pack.
    VoltageClass84V,

    /// A capture/app/label explicitly identifies a 100.8 V class pack.
    VoltageClass100V,

    /// A capture/app/BMS value reports an observed pack voltage.
    ObservedPackVoltage(Voltage),
}

/// Result of selecting a Begode pack voltage profile from explicit evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeVoltageProfileSelection {
    /// No evidence selected a profile.
    Missing,

    /// Evidence selected more than one profile.
    Conflicting,

    /// Evidence selected exactly one profile.
    Selected(BegodePackVoltageProfile),
}

/// Explicit Begode pack capacity evidence from capture/app/pack labels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodeCapacityEvidence {
    /// Nominal pack capacity, when explicitly reported.
    pub nominal_capacity: Option<Capacity>,

    /// Pack energy, when explicitly reported.
    pub reported_energy: Option<Energy>,
}

/// Result of selecting Begode pack capacity evidence from explicit inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeCapacitySelection {
    /// No explicit capacity evidence was present.
    Missing,

    /// Evidence contained conflicting capacity values.
    Conflicting,

    /// Evidence selected a non-conflicting capacity record.
    Selected(BegodeCapacityEvidence),
}

/// Explicit Begode cell model evidence from capture/app/pack labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeCellModel {
    /// Samsung INR21700-50S cells.
    Samsung50S,
}

impl BegodeCellModel {
    /// Stable display label for this cell model.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Samsung50S => "Samsung 50S",
        }
    }
}

/// Explicit Begode pack-layout evidence from capture/app/pack labels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BegodePackLayoutEvidence {
    /// Cell model, when explicitly reported.
    pub cell_model: Option<BegodeCellModel>,

    /// Series cell count, when explicitly reported.
    pub series_cells: Option<SeriesCount>,

    /// Parallel cell count, when explicitly reported.
    pub parallel_count: Option<ParallelCount>,
}

/// Result of selecting Begode pack-layout evidence from explicit inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodePackLayoutSelection {
    /// No explicit layout evidence was present.
    Missing,

    /// Evidence contained conflicting layout values.
    Conflicting,

    /// Evidence selected a non-conflicting layout record.
    Selected(BegodePackLayoutEvidence),
}

/// Result of cross-checking explicit Begode pack evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodePackEvidenceConsistency {
    /// Evidence is self-consistent.
    Consistent,

    /// More explicit evidence is required before consistency can be proven.
    Incomplete,

    /// Evidence contradicts another explicit field.
    Inconsistent,
}

/// Explicit Falcon battery variant selected from voltage/capacity/layout evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeFalconBatteryVariant {
    /// Current live target hardware: 100.8 V / 24S / 900 Wh Samsung 50S.
    Current100V24S900WhSamsung50S,
}

impl BegodeFalconBatteryVariant {
    /// Voltage profile selected for this Falcon variant.
    #[must_use]
    pub const fn voltage_profile(self) -> BegodePackVoltageProfile {
        match self {
            Self::Current100V24S900WhSamsung50S => BegodePackVoltageProfile::Begode100VFullCharge,
        }
    }

    /// Series cell count selected for this Falcon variant.
    #[must_use]
    pub const fn series_cells(self) -> SeriesCount {
        self.voltage_profile().series_cells()
    }

    /// Cell model selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn cell_model(self) -> Option<BegodeCellModel> {
        match self {
            Self::Current100V24S900WhSamsung50S => Some(BegodeCellModel::Samsung50S),
        }
    }

    /// Parallel pack count selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn parallel_count(self) -> Option<ParallelCount> {
        match self {
            Self::Current100V24S900WhSamsung50S => Some(PARALLEL_PACKS_2),
        }
    }

    /// Nominal pack capacity selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn nominal_capacity(self) -> Option<Capacity> {
        match self {
            Self::Current100V24S900WhSamsung50S => Some(Capacity::from_milliamp_hours(10_000)),
        }
    }

    /// Pack energy selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn reported_energy(self) -> Option<Energy> {
        match self {
            Self::Current100V24S900WhSamsung50S => Some(Energy::from_watt_hours(900)),
        }
    }
}

/// Result of selecting a Falcon-specific battery variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeFalconBatteryVariantSelection {
    /// No variant could be selected from evidence.
    Missing,

    /// Evidence contradicted itself or selected no known Falcon variant.
    Conflicting,

    /// Evidence selected exactly one Falcon battery variant.
    Selected(BegodeFalconBatteryVariant),
}

impl BegodePackVoltageProfile {
    const fn scaler_milli(self) -> i32 {
        match self {
            Self::Begode84VFullCharge => 1_250,
            Self::Begode100VFullCharge => 1_500,
        }
    }

    /// Series cell count for this pack profile.
    #[must_use]
    pub const fn series_cells(self) -> SeriesCount {
        match self {
            Self::Begode84VFullCharge => SERIES_CELLS_20,
            Self::Begode100VFullCharge => SERIES_CELLS_24,
        }
    }

    /// Nominal pack capacity in milliamp-hours, when known.
    #[must_use]
    pub const fn nominal_capacity(self) -> Option<Capacity> {
        match self {
            Self::Begode84VFullCharge | Self::Begode100VFullCharge => None,
        }
    }

    /// Expected pack voltage range.
    #[must_use]
    pub fn voltage_range(self) -> RangeInclusive<Voltage> {
        match self {
            Self::Begode84VFullCharge => {
                Voltage::from_millivolts(60_000)..=Voltage::from_millivolts(84_000)
            }
            Self::Begode100VFullCharge => {
                Voltage::from_millivolts(72_000)..=Voltage::from_millivolts(100_800)
            }
        }
    }
}

/// Returns the explicit voltage profile for the current Begode Falcon target.
///
/// The generic registry entry remains battery-agnostic until live evidence
/// selects a device-specific voltage/capacity profile.
#[must_use]
pub const fn begode_falcon_target_voltage_profile() -> BegodePackVoltageProfile {
    BEGODE_FALCON_TARGET_VOLTAGE_PROFILE
}

/// Selects a Falcon-specific battery variant from already parsed evidence.
#[must_use]
pub fn select_begode_falcon_battery_variant(
    profile: BegodeVoltageProfileSelection,
    capacity: BegodeCapacitySelection,
    layout: BegodePackLayoutSelection,
) -> BegodeFalconBatteryVariantSelection {
    match validate_begode_pack_evidence(profile, capacity, layout) {
        BegodePackEvidenceConsistency::Inconsistent => {
            BegodeFalconBatteryVariantSelection::Conflicting
        }
        BegodePackEvidenceConsistency::Incomplete => BegodeFalconBatteryVariantSelection::Missing,
        BegodePackEvidenceConsistency::Consistent => {
            select_consistent_begode_falcon_battery_variant(profile, capacity, layout)
        }
    }
}

/// Selects a Begode pack voltage profile from explicit evidence.
#[must_use]
pub fn select_begode_pack_voltage_profile<I>(evidence: I) -> BegodeVoltageProfileSelection
where
    I: IntoIterator<Item = BegodeVoltageEvidence>,
{
    evidence
        .into_iter()
        .filter_map(evidence_profile)
        .try_fold(None, merge_profile_selection)
        .map_or(
            BegodeVoltageProfileSelection::Conflicting,
            |selected| match selected {
                Some(profile) => BegodeVoltageProfileSelection::Selected(profile),
                None => BegodeVoltageProfileSelection::Missing,
            },
        )
}

/// Selects a Begode pack voltage profile from capture/app annotations.
#[must_use]
pub fn select_begode_pack_voltage_profile_from_annotations<I, A>(
    annotations: I,
) -> BegodeVoltageProfileSelection
where
    I: IntoIterator<Item = A>,
    A: AsRef<str>,
{
    annotations
        .into_iter()
        .filter_map(|annotation| voltage_evidence_from_annotation(annotation.as_ref()))
        .try_fold(None, |selected, evidence| {
            evidence_profile(evidence).map_or(Ok(selected), |profile| {
                merge_profile_selection(selected, profile)
            })
        })
        .map_or(
            BegodeVoltageProfileSelection::Conflicting,
            |selected| match selected {
                Some(profile) => BegodeVoltageProfileSelection::Selected(profile),
                None => BegodeVoltageProfileSelection::Missing,
            },
        )
}

/// Selects Begode pack capacity evidence from capture/app annotations.
#[must_use]
pub fn select_begode_pack_capacity_from_annotations<I, A>(annotations: I) -> BegodeCapacitySelection
where
    I: IntoIterator<Item = A>,
    A: AsRef<str>,
{
    annotations
        .into_iter()
        .filter_map(|annotation| capacity_evidence_from_annotation(annotation.as_ref()))
        .try_fold(BegodeCapacityEvidence::default(), merge_capacity_evidence)
        .map_or(BegodeCapacitySelection::Conflicting, |evidence| {
            if evidence.nominal_capacity.is_some() || evidence.reported_energy.is_some() {
                BegodeCapacitySelection::Selected(evidence)
            } else {
                BegodeCapacitySelection::Missing
            }
        })
}

/// Selects Begode pack-layout evidence from capture/app annotations.
#[must_use]
pub fn select_begode_pack_layout_from_annotations<I, A>(annotations: I) -> BegodePackLayoutSelection
where
    I: IntoIterator<Item = A>,
    A: AsRef<str>,
{
    annotations
        .into_iter()
        .filter_map(|annotation| layout_evidence_from_annotation(annotation.as_ref()))
        .try_fold(BegodePackLayoutEvidence::default(), merge_layout_evidence)
        .map_or(BegodePackLayoutSelection::Conflicting, |evidence| {
            if evidence.cell_model.is_some()
                || evidence.series_cells.is_some()
                || evidence.parallel_count.is_some()
            {
                BegodePackLayoutSelection::Selected(evidence)
            } else {
                BegodePackLayoutSelection::Missing
            }
        })
}

/// Cross-checks selected Begode pack evidence for internal consistency.
#[must_use]
pub fn validate_begode_pack_evidence(
    profile: BegodeVoltageProfileSelection,
    capacity: BegodeCapacitySelection,
    layout: BegodePackLayoutSelection,
) -> BegodePackEvidenceConsistency {
    match (profile, capacity, layout) {
        (BegodeVoltageProfileSelection::Conflicting, _, _)
        | (_, BegodeCapacitySelection::Conflicting, _)
        | (_, _, BegodePackLayoutSelection::Conflicting) => {
            BegodePackEvidenceConsistency::Inconsistent
        }
        (
            BegodeVoltageProfileSelection::Selected(profile),
            capacity,
            BegodePackLayoutSelection::Selected(layout),
        ) => validate_selected_begode_pack_evidence(profile, capacity, layout),
        (BegodeVoltageProfileSelection::Missing, _, _)
        | (_, _, BegodePackLayoutSelection::Missing) => BegodePackEvidenceConsistency::Incomplete,
    }
}

fn validate_selected_begode_pack_evidence(
    profile: BegodePackVoltageProfile,
    capacity: BegodeCapacitySelection,
    layout: BegodePackLayoutEvidence,
) -> BegodePackEvidenceConsistency {
    if let Some(series_cells) = layout.series_cells
        && series_cells != profile.series_cells()
    {
        return BegodePackEvidenceConsistency::Inconsistent;
    }

    match capacity {
        BegodeCapacitySelection::Missing => BegodePackEvidenceConsistency::Consistent,
        BegodeCapacitySelection::Conflicting => BegodePackEvidenceConsistency::Inconsistent,
        BegodeCapacitySelection::Selected(capacity) => {
            validate_selected_begode_pack_capacity(capacity, layout)
        }
    }
}

fn validate_selected_begode_pack_capacity(
    capacity: BegodeCapacityEvidence,
    layout: BegodePackLayoutEvidence,
) -> BegodePackEvidenceConsistency {
    match (
        layout.cell_model,
        layout.series_cells,
        layout.parallel_count,
    ) {
        (Some(BegodeCellModel::Samsung50S), Some(series_cells), Some(parallel_count)) => {
            validate_samsung_50s_capacity(capacity, series_cells, parallel_count)
        }
        _ => BegodePackEvidenceConsistency::Incomplete,
    }
}

fn validate_samsung_50s_capacity(
    capacity: BegodeCapacityEvidence,
    series_cells: SeriesCount,
    parallel_count: ParallelCount,
) -> BegodePackEvidenceConsistency {
    let expected_capacity = Capacity::from_parallel_packs(5_000, parallel_count);
    if let Some(nominal_capacity) = capacity.nominal_capacity
        && nominal_capacity != expected_capacity
    {
        return BegodePackEvidenceConsistency::Inconsistent;
    }

    if let Some(reported_energy) = capacity.reported_energy {
        let expected_wh = Energy::from_cell_geometry(18, series_cells, parallel_count);
        if !within_percent(
            reported_energy.as_watt_hours(),
            expected_wh.as_watt_hours(),
            5,
        ) {
            return BegodePackEvidenceConsistency::Inconsistent;
        }
    }

    BegodePackEvidenceConsistency::Consistent
}

fn select_consistent_begode_falcon_battery_variant(
    profile: BegodeVoltageProfileSelection,
    capacity: BegodeCapacitySelection,
    layout: BegodePackLayoutSelection,
) -> BegodeFalconBatteryVariantSelection {
    match (profile, capacity, layout) {
        (
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge),
            BegodeCapacitySelection::Selected(capacity),
            BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                cell_model: Some(BegodeCellModel::Samsung50S),
                series_cells: Some(SERIES_CELLS_24),
                parallel_count: Some(PARALLEL_PACKS_2),
                ..
            }),
        ) if capacity.nominal_capacity == Some(Capacity::from_milliamp_hours(10_000))
            && capacity.reported_energy == Some(Energy::from_watt_hours(900)) =>
        {
            BegodeFalconBatteryVariantSelection::Selected(
                BegodeFalconBatteryVariant::Current100V24S900WhSamsung50S,
            )
        }
        _ => BegodeFalconBatteryVariantSelection::Missing,
    }
}

const fn within_percent(value: u32, expected: u32, percent: u32) -> bool {
    let delta = value.abs_diff(expected);
    delta * 100 <= expected * percent
}

fn voltage_evidence_from_annotation(annotation: &str) -> Option<BegodeVoltageEvidence> {
    let (key, value) = annotation.split_once('=')?;
    match key.trim() {
        "battery" | "app_voltage_class" | "charger_voltage_class" => voltage_class_evidence(value),
        "charger_voltage" | "observed_pack_voltage" | "bms_voltage" | "app_voltage" => {
            parse_mv_evidence(value)
        }
        _ => None,
    }
}

fn capacity_evidence_from_annotation(annotation: &str) -> Option<BegodeCapacityEvidence> {
    let (key, value) = annotation.split_once('=')?;
    let parsed = value.trim().parse::<u32>().ok()?;
    match key.trim() {
        "nominal_capacity_mah" | "nominal_capacity" | "capacity_mah" | "pack_capacity_mah" => {
            Some(BegodeCapacityEvidence {
                nominal_capacity: Some(Capacity::from_milliamp_hours(parsed)),
                reported_energy: None,
            })
        }
        "reported_wh" | "reported_energy" | "pack_wh" => Some(BegodeCapacityEvidence {
            nominal_capacity: None,
            reported_energy: Some(Energy::from_watt_hours(parsed)),
        }),
        _ => None,
    }
}

fn layout_evidence_from_annotation(annotation: &str) -> Option<BegodePackLayoutEvidence> {
    let (key, value) = annotation.split_once('=')?;
    match key.trim() {
        "cell_model" | "pack_cell_model" => cell_model_evidence(value),
        "series_cells" | "pack_series_cells" => {
            parse_u8_evidence(value).map(|series_cells| BegodePackLayoutEvidence {
                cell_model: None,
                series_cells: Some(SeriesCount::new(series_cells)),
                parallel_count: None,
            })
        }
        "parallel_count" | "parallel_cells" | "parallel_packs" | "pack_parallel_count" => {
            parse_u8_evidence(value).map(|parallel_count| BegodePackLayoutEvidence {
                cell_model: None,
                series_cells: None,
                parallel_count: Some(ParallelCount::new(parallel_count)),
            })
        }
        _ => None,
    }
}

fn cell_model_evidence(value: &str) -> Option<BegodePackLayoutEvidence> {
    let value = value.trim();
    if eq_ignore_ascii_case(value, "samsung 50s")
        || eq_ignore_ascii_case(value, "samsung50s")
        || eq_ignore_ascii_case(value, "50s")
    {
        Some(BegodePackLayoutEvidence {
            cell_model: Some(BegodeCellModel::Samsung50S),
            series_cells: None,
            parallel_count: None,
        })
    } else {
        None
    }
}

fn voltage_class_evidence(value: &str) -> Option<BegodeVoltageEvidence> {
    let value = value
        .trim()
        .split_once('-')
        .map_or(value.trim(), |(head, _)| head);
    if eq_ignore_ascii_case(value, "84v")
        || eq_ignore_ascii_case(value, "84.0v")
        || eq_ignore_ascii_case(value, "20s")
    {
        Some(BegodeVoltageEvidence::VoltageClass84V)
    } else if eq_ignore_ascii_case(value, "100v")
        || eq_ignore_ascii_case(value, "100.8v")
        || eq_ignore_ascii_case(value, "24s")
    {
        Some(BegodeVoltageEvidence::VoltageClass100V)
    } else {
        None
    }
}

fn parse_mv_evidence(value: &str) -> Option<BegodeVoltageEvidence> {
    value
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|millivolts| *millivolts >= 0)
        .map(Voltage::from_millivolts)
        .map(BegodeVoltageEvidence::ObservedPackVoltage)
}

fn parse_u8_evidence(value: &str) -> Option<u8> {
    value.trim().parse::<u8>().ok()
}

fn eq_ignore_ascii_case(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .all(|(left, right)| left.eq_ignore_ascii_case(&right))
}

const fn evidence_profile(evidence: BegodeVoltageEvidence) -> Option<BegodePackVoltageProfile> {
    match evidence {
        BegodeVoltageEvidence::VoltageClass84V => {
            Some(BegodePackVoltageProfile::Begode84VFullCharge)
        }
        BegodeVoltageEvidence::VoltageClass100V => {
            Some(BegodePackVoltageProfile::Begode100VFullCharge)
        }
        BegodeVoltageEvidence::ObservedPackVoltage(voltage) if voltage.as_millivolts() < 72_000 => {
            Some(BegodePackVoltageProfile::Begode84VFullCharge)
        }
        BegodeVoltageEvidence::ObservedPackVoltage(voltage)
            if voltage.as_millivolts() > 84_000 && voltage.as_millivolts() <= 100_800 =>
        {
            Some(BegodePackVoltageProfile::Begode100VFullCharge)
        }
        BegodeVoltageEvidence::ObservedPackVoltage(_) => None,
    }
}

fn merge_profile_selection(
    selected: Option<BegodePackVoltageProfile>,
    candidate: BegodePackVoltageProfile,
) -> Result<Option<BegodePackVoltageProfile>, ()> {
    match selected {
        Some(previous) if previous != candidate => Err(()),
        Some(previous) => Ok(Some(previous)),
        None => Ok(Some(candidate)),
    }
}

fn merge_capacity_evidence(
    selected: BegodeCapacityEvidence,
    evidence: BegodeCapacityEvidence,
) -> Result<BegodeCapacityEvidence, ()> {
    Ok(BegodeCapacityEvidence {
        nominal_capacity: merge_optional_quantity(
            selected.nominal_capacity,
            evidence.nominal_capacity,
        )?,
        reported_energy: merge_optional_quantity(
            selected.reported_energy,
            evidence.reported_energy,
        )?,
    })
}

fn merge_layout_evidence(
    selected: BegodePackLayoutEvidence,
    evidence: BegodePackLayoutEvidence,
) -> Result<BegodePackLayoutEvidence, ()> {
    Ok(BegodePackLayoutEvidence {
        cell_model: merge_optional_cell_model(selected.cell_model, evidence.cell_model)?,
        series_cells: merge_optional_quantity(selected.series_cells, evidence.series_cells)?,
        parallel_count: merge_optional_quantity(selected.parallel_count, evidence.parallel_count)?,
    })
}

const fn merge_optional_cell_model(
    left: Option<BegodeCellModel>,
    right: Option<BegodeCellModel>,
) -> Result<Option<BegodeCellModel>, ()> {
    match (left, right) {
        (None, None) => Ok(None),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (Some(left), Some(right)) if left as u8 == right as u8 => Ok(Some(left)),
        (Some(_), Some(_)) => Err(()),
    }
}

fn merge_optional_quantity<T: Copy + Eq>(
    left: Option<T>,
    right: Option<T>,
) -> Result<Option<T>, ()> {
    match (left, right) {
        (None, None) => Ok(None),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (Some(left), Some(right)) if left == right => Ok(Some(left)),
        (Some(_), Some(_)) => Err(()),
    }
}

/// Primary Begode live telemetry decoded from frame tag `0x00`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeLiveATelemetry {
    /// Wire-encoded voltage before profile scaling.
    pub wire_voltage: WireVoltage,

    /// Scaled pack voltage.
    pub voltage: Voltage,

    /// Signed speed.
    pub speed: Speed,

    /// Full four-byte trip distance candidate.
    pub trip_distance: Distance,

    /// Low-word trip distance for firmwares that do not populate the high word.
    pub trip_distance_low: Distance,

    /// Signed phase current.
    pub phase_current: PhaseCurrent,

    /// Default MPU6050 IMU temperature.
    pub imu_temperature: Temperature,

    /// Hardware PWM as a signed duty-cycle percentage.
    pub hardware_pwm: DutyCycle,

    /// Estimated battery percent derived from voltage.
    pub battery_level: BatteryLevel,
}

impl BegodeLiveATelemetry {
    /// Decodes a source-backed Begode live-A frame.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeTelemetryError::UnexpectedFrameTag`] when the frame tag
    /// is not `0x00`.
    pub fn decode(
        frame: &BegodeFrame,
        profile: BegodePackVoltageProfile,
    ) -> Result<Self, BegodeTelemetryError> {
        require_tag(frame, 0x00)?;
        let cursor = ParserCursor::new(frame.as_slice());
        let wire_voltage =
            WireVoltage::from_centivolts(be_u16(cursor, ParserOffset::from_bytes(2)));
        Ok(Self {
            wire_voltage,
            voltage: wire_voltage.as_scaled_voltage(profile.scaler_milli()),
            speed: Speed::from_centimetres_per_second(i32::from(be_i16(
                cursor,
                ParserOffset::from_bytes(4),
            ))),
            trip_distance: Distance::from_metres(u64::from(be_u32(
                cursor,
                ParserOffset::from_bytes(6),
            ))),
            trip_distance_low: Distance::from_metres(u64::from(be_u16(
                cursor,
                ParserOffset::from_bytes(8),
            ))),
            phase_current: PhaseCurrent::from_centiamps(i32::from(be_i16(
                cursor,
                ParserOffset::from_bytes(10),
            ))),
            imu_temperature: Temperature::from_mpu6050_counts(be_i16(
                cursor,
                ParserOffset::from_bytes(12),
            )),
            hardware_pwm: DutyCycle::from_decipermille(be_i16(
                cursor,
                ParserOffset::from_bytes(14),
            )),
            battery_level: estimate_begode_battery_level(
                wire_voltage.as_scaled_voltage(profile.scaler_milli()),
                profile,
            ),
        })
    }

    /// Converts decoded Live A fields into a transport-independent telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicTimestamp) -> TelemetryDelta {
        self.to_delta_with_units(at_ms, BegodeUnitMode::Metric)
    }

    /// Converts decoded Live A fields into a transport-independent telemetry delta.
    #[must_use]
    pub fn to_delta_with_units(
        self,
        at_ms: MonotonicTimestamp,
        unit_mode: BegodeUnitMode,
    ) -> TelemetryDelta {
        TelemetryDelta {
            speed: Some(source_reported(unit_mode.speed(self.speed))),
            voltage: Some(source_reported(self.voltage)),
            motor_current: Some(source_reported(PhaseCurrent::from_milliamps(
                self.phase_current.as_milliamps(),
            ))),
            power: Some(source_calculated(Power::from_voltage_current(
                self.voltage,
                self.phase_current,
            ))),
            controller_temperature: Some(source_reported(self.imu_temperature)),
            pwm: Some(source_reported(DutyCycle::from_permille(
                self.hardware_pwm.as_permille(),
            ))),
            distance: Some(source_reported(unit_mode.distance(self.trip_distance_low))),
            battery_level_estimated: Some(source_estimated(self.battery_level)),
            ..TelemetryDelta::empty(at_ms)
        }
    }
}

/// Secondary Begode live telemetry decoded from frame tag `0x04`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeLiveBTelemetry {
    /// Lifetime total distance in meters.
    pub total_distance: Distance,

    /// Raw settings bitfield.
    pub settings_bits: BegodeSettingsBits,

    /// Power-off timer.
    pub power_off_timer: Duration,

    /// Tiltback / max-speed field in km/h.
    pub tiltback_speed: Speed,

    /// LED mode.
    pub led_mode: BegodeLedMode,

    /// Raw alert bitfield.
    pub alert_flags: BegodeAlertFlags,

    /// Low two bits of the light-mode byte.
    pub light_mode: BegodeLightMode,
}

impl BegodeLiveBTelemetry {
    /// Decodes a source-backed Begode live-B frame.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeTelemetryError::UnexpectedFrameTag`] when the frame tag
    /// is not `0x04`.
    pub fn decode(frame: &BegodeFrame) -> Result<Self, BegodeTelemetryError> {
        require_tag(frame, 0x04)?;
        let cursor = ParserCursor::new(frame.as_slice());
        let settings_bits = BegodeSettingsBits::new(be_u16(cursor, ParserOffset::from_bytes(6)));
        let unit_mode = BegodeUnitMode::from_settings_bits(settings_bits);
        Ok(Self {
            total_distance: unit_mode
                .distance_from_wire(be_u32(cursor, ParserOffset::from_bytes(2))),
            settings_bits,
            power_off_timer: Duration::from_minutes(u64::from(be_u16(
                cursor,
                ParserOffset::from_bytes(8),
            ))),
            tiltback_speed: unit_mode.speed_limit(be_u16(cursor, ParserOffset::from_bytes(10))),
            led_mode: BegodeLedMode::new(byte(cursor, ParserOffset::from_bytes(13))),
            alert_flags: BegodeAlertFlags::new(byte(cursor, ParserOffset::from_bytes(14))),
            light_mode: BegodeLightMode::new(byte(cursor, ParserOffset::from_bytes(15))),
        })
    }

    /// Converts decoded Live B fields into a telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicTimestamp) -> TelemetryDelta {
        self.to_delta_with_units(at_ms)
    }

    /// Converts decoded Live B fields into a telemetry delta with unit normalization.
    #[must_use]
    pub fn to_delta_with_units(self, at_ms: MonotonicTimestamp) -> TelemetryDelta {
        TelemetryDelta {
            distance: Some(source_reported(Distance::from_millimetres(
                self.total_distance.as_millimetres(),
            ))),
            ..TelemetryDelta::empty(at_ms)
        }
    }

    /// Converts decoded Live B settings into a generic read-only response.
    #[must_use]
    pub fn to_settings_response(self) -> ReadOnlyResponse {
        self.to_settings_response_with_units()
    }

    /// Converts decoded Live B settings into a generic read-only response.
    #[must_use]
    pub fn to_settings_response_with_units(self) -> ReadOnlyResponse {
        ReadOnlyResponse::Settings(SettingsReadback::available([
            Some(settings_entry(
                BEGODE_FIELD_SETTINGS_BITS,
                i64::from(self.settings_bits.get()),
            )),
            Some(settings_entry(
                BEGODE_FIELD_POWER_OFF_TIMER_MINUTES,
                u64_to_i64_saturating(self.power_off_timer.as_minutes()),
            )),
            Some(settings_entry(
                BEGODE_FIELD_TILTBACK_SPEED_KMH,
                i64::from(
                    self.tiltback_speed
                        .as_kmh_rounded()
                        .clamp(0, i32::from(u16::MAX)),
                ),
            )),
            Some(settings_entry(
                BEGODE_FIELD_LED_AND_LIGHT_MODE,
                i64::from((u16::from(self.led_mode.get()) << 8) | u16::from(self.light_mode.get())),
            )),
        ]))
    }

    /// Returns the unit mode encoded by Live B settings bit 0.
    #[must_use]
    pub const fn unit_mode(self) -> BegodeUnitMode {
        BegodeUnitMode::from_settings_bits(self.settings_bits)
    }

    /// Converts decoded Live B alert flags into a diagnostic readback response.
    #[must_use]
    pub fn to_diagnostics_response(self) -> ReadOnlyResponse {
        ReadOnlyResponse::Diagnostics(DiagnosticReadback {
            details: [
                Some(DiagnosticDetail {
                    field: RawFieldValue::new(
                        BEGODE_FIELD_ALERT_FLAGS,
                        i64::from(self.alert_flags.get()),
                    ),
                    severity: if self.alert_flags.get() == 0 {
                        DiagnosticSeverity::Info
                    } else {
                        DiagnosticSeverity::Warning
                    },
                    quality: ValueQuality::Known,
                    verification: VerificationStatus::SourceVerified,
                }),
                None,
                None,
                None,
            ],
        })
    }
}

/// Extra Begode telemetry decoded from frame tag `0x07`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeExtraTelemetry {
    /// True battery current in milliamps.
    pub battery_current: BatteryCurrent,

    /// Motor temperature.
    pub motor_temperature: Temperature,

    /// True PWM as a signed duty-cycle percentage.
    pub true_pwm: DutyCycle,
}

impl BegodeExtraTelemetry {
    /// Decodes a source-backed Begode extra-telemetry frame.
    ///
    /// # Errors
    ///
    /// Returns [`BegodeTelemetryError::UnexpectedFrameTag`] when the frame tag
    /// is not `0x07`.
    pub fn decode(frame: &BegodeFrame) -> Result<Self, BegodeTelemetryError> {
        require_tag(frame, 0x07)?;
        let cursor = ParserCursor::new(frame.as_slice());
        Ok(Self {
            battery_current: BatteryCurrent::from_centiamps(i32::from(be_i16(
                cursor,
                ParserOffset::from_bytes(2),
            ))),
            motor_temperature: Temperature::from_celsius(i64::from(be_i16(
                cursor,
                ParserOffset::from_bytes(6),
            ))),
            true_pwm: DutyCycle::from_decipermille(be_i16(cursor, ParserOffset::from_bytes(8))),
        })
    }

    /// Converts decoded extra telemetry into a transport-independent delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicTimestamp) -> TelemetryDelta {
        TelemetryDelta {
            battery_current: Some(source_reported(self.battery_current)),
            motor_temperature: Some(source_reported(self.motor_temperature)),
            pwm: Some(source_reported(self.true_pwm)),
            ..TelemetryDelta::empty(at_ms)
        }
    }
}

/// Begode Live B raw field id for the settings bitfield.
pub const BEGODE_FIELD_SETTINGS_BITS: u16 = 0x0406;

/// Begode Live B raw field id for the power-off timer in minutes.
pub const BEGODE_FIELD_POWER_OFF_TIMER_MINUTES: u16 = 0x0408;

/// Begode Live B raw field id for tiltback/max-speed km/h.
pub const BEGODE_FIELD_TILTBACK_SPEED_KMH: u16 = 0x040a;

/// Begode Live B packed raw field id for LED mode and light mode.
pub const BEGODE_FIELD_LED_AND_LIGHT_MODE: u16 = 0x040d;

/// Begode Live B raw field id for alert flags.
pub const BEGODE_FIELD_ALERT_FLAGS: u16 = 0x040e;

/// Begode telemetry decode failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BegodeTelemetryError {
    /// Frame tag did not match the typed decoder.
    #[error("unexpected Begode frame tag: expected {expected:#04x}, got {actual:#04x}")]
    UnexpectedFrameTag {
        /// Expected tag for the typed decoder.
        expected: u8,

        /// Actual frame tag.
        actual: u8,
    },
}

/// Estimates Begode battery percent from scaled pack voltage and profile.
#[must_use]
pub fn estimate_begode_battery_level(
    voltage: Voltage,
    profile: BegodePackVoltageProfile,
) -> BatteryLevel {
    let wire_centivolts = i32::from(
        WireVoltage::from_scaled_voltage(voltage, profile.scaler_milli()).as_centivolts(),
    );
    BatteryLevel::from_piecewise_linear(
        i64::from(wire_centivolts),
        &[
            (5_120, BatteryLevel::from_percent(0)),
            (5_440, BatteryLevel::from_percent(9)),
            (6_680, BatteryLevel::from_percent(100)),
        ],
    )
}

fn require_tag(frame: &BegodeFrame, expected: u8) -> Result<(), BegodeTelemetryError> {
    let actual = frame.tag();
    if actual.get() == u16::from(expected) {
        Ok(())
    } else {
        Err(BegodeTelemetryError::UnexpectedFrameTag {
            expected,
            actual: tag_byte(actual),
        })
    }
}

const fn source_reported<T>(value: T) -> Measured<T> {
    Measured {
        value,
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    }
}

const fn source_calculated<T>(value: T) -> Measured<T> {
    Measured {
        value,
        source: ValueSource::Calculated,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    }
}

const fn source_estimated<T>(value: T) -> Measured<T> {
    Measured {
        value,
        source: ValueSource::Estimated,
        quality: ValueQuality::Inferred,
        verification: VerificationStatus::SourceVerified,
    }
}

const fn settings_entry(id: u16, value: i64) -> SettingsEntry {
    SettingsEntry {
        field: RawFieldValue::new(id, value),
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::SourceVerified,
    }
}

fn byte(cursor: ParserCursor<'_>, offset: ParserOffset) -> u8 {
    cursor.byte(offset).unwrap_or(0)
}

fn be_u16(cursor: ParserCursor<'_>, offset: ParserOffset) -> u16 {
    cursor.be_u16(offset).unwrap_or(0)
}

fn be_i16(cursor: ParserCursor<'_>, offset: ParserOffset) -> i16 {
    cursor.be_i16(offset).unwrap_or(0)
}

fn be_u32(cursor: ParserCursor<'_>, offset: ParserOffset) -> u32 {
    cursor.be_u32(offset).unwrap_or(0)
}

fn tag_byte(tag: ProtocolTag) -> u8 {
    u8::try_from(tag.get()).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    const fn ms(value: u64) -> cutout_core::MonotonicTimestamp {
        cutout_core::MonotonicTimestamp::new(value)
    }

    use super::{
        BEGODE_FALCON_TARGET_VOLTAGE_PROFILE, BegodeAlertFlags, BegodeCapacityEvidence,
        BegodeCapacitySelection, BegodeCellModel, BegodeFalconBatteryVariant,
        BegodeFalconBatteryVariantSelection, BegodeLedMode, BegodeLightMode,
        BegodePackLayoutEvidence, BegodePackLayoutSelection, BegodeSettingsBits,
        BegodeVoltageEvidence, BegodeVoltageProfileSelection, PARALLEL_PACKS_1, PARALLEL_PACKS_2,
        SERIES_CELLS_20, SERIES_CELLS_24, begode_falcon_target_voltage_profile,
        select_begode_falcon_battery_variant, select_begode_pack_capacity_from_annotations,
        select_begode_pack_layout_from_annotations, select_begode_pack_voltage_profile,
        select_begode_pack_voltage_profile_from_annotations,
    };
    use crate::{
        BEGODE_FIELD_ALERT_FLAGS, BEGODE_FIELD_LED_AND_LIGHT_MODE,
        BEGODE_FIELD_POWER_OFF_TIMER_MINUTES, BEGODE_FIELD_SETTINGS_BITS,
        BEGODE_FIELD_TILTBACK_SPEED_KMH, BegodeExtraTelemetry, BegodeFrame, BegodeLiveATelemetry,
        BegodeLiveBTelemetry, BegodePackEvidenceConsistency, BegodePackVoltageProfile,
        BegodeTelemetryContext, BegodeTelemetryError, BegodeUnitMode,
        estimate_begode_battery_level, validate_begode_pack_evidence,
    };
    use cutout_core::{Capacity, Duration, Energy, LightState};
    use cutout_core::{
        DiagnosticSeverity, Measured, ParallelCount, ProtocolTag, RawFieldValue, ReadOnlyResponse,
        SeriesCount, TelemetryDelta, ValueQuality, ValueSource, VerificationStatus, Voltage,
    };
    use proptest::prelude::*;

    const LIVE_A: [u8; 24] = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
    const LIVE_B: [u8; 24] = hex_literal::hex!("55aa000000320000000f003200030502000004185a5a5a5a");
    const LIVE_B_IMPERIAL: [u8; 24] =
        hex_literal::hex!("55aa000000320001000f003200030502000004185a5a5a5a");
    const EXTRA: [u8; 24] = hex_literal::hex!("55aaff9c0000002affd8000000000000000007185a5a5a5a");

    #[test]
    fn live_a_decodes_source_backed_primary_fields_for_falcon_100v_full_charge() {
        let frame = BegodeFrame::try_from_slice(&LIVE_A).expect("fixture frame is valid");
        assert_eq!(frame.tag(), ProtocolTag::new(0x00));
        let telemetry =
            BegodeLiveATelemetry::decode(&frame, BegodePackVoltageProfile::Begode100VFullCharge)
                .expect("live A frame decodes");

        assert_eq!(telemetry.wire_voltage.as_centivolts(), 6005);
        assert_eq!(telemetry.voltage.as_millivolts(), 90_075);
        assert_eq!(telemetry.speed.as_millimetres_per_second(), 13_360);
        assert_eq!(telemetry.trip_distance.as_millimetres(), 7_733_998_000);
        assert_eq!(telemetry.trip_distance_low.as_millimetres(), 750_000);
        assert_eq!(telemetry.phase_current.as_milliamps(), -11_800);
        assert_eq!(telemetry.imu_temperature.as_millicelsius(), 27_930);
        assert_eq!(telemetry.hardware_pwm.as_permille(), 0x1481 / 10);
        assert_eq!(telemetry.battery_level.get(), 50);
    }

    #[test]
    fn live_b_decodes_total_mileage_and_settings_fields() {
        let frame = BegodeFrame::try_from_slice(&LIVE_B).expect("fixture frame is valid");
        let telemetry = BegodeLiveBTelemetry::decode(&frame).expect("live B frame decodes");

        assert_eq!(telemetry.total_distance.as_millimetres(), 50_000);
        assert_eq!(telemetry.settings_bits, BegodeSettingsBits::new(0));
        assert_eq!(telemetry.power_off_timer, Duration::from_minutes(15));
        assert_eq!(telemetry.tiltback_speed.as_millimetres_per_second(), 13_888);
        assert_eq!(telemetry.led_mode, BegodeLedMode::new(3));
        assert_eq!(telemetry.alert_flags, BegodeAlertFlags::new(5));
        assert_eq!(telemetry.light_mode, BegodeLightMode::new(2));
    }

    #[test]
    fn light_mode_maps_documented_begode_states() {
        assert_eq!(BegodeLightMode::new(0).light_state(), Some(LightState::Off));
        assert_eq!(BegodeLightMode::new(1).light_state(), Some(LightState::On));
        assert_eq!(
            BegodeLightMode::new(2).light_state(),
            Some(LightState::Strobe)
        );
        assert_eq!(BegodeLightMode::new(3).light_state(), None);
    }

    #[test]
    fn extra_telemetry_decodes_true_current_motor_temperature_and_pwm() {
        let frame = BegodeFrame::try_from_slice(&EXTRA).expect("fixture frame is valid");
        let telemetry = BegodeExtraTelemetry::decode(&frame).expect("extra frame decodes");

        assert_eq!(telemetry.battery_current.as_milliamps(), -1_000);
        assert_eq!(telemetry.motor_temperature.as_millicelsius(), 42_000);
        assert_eq!(telemetry.true_pwm.as_permille(), -4);
    }

    #[test]
    fn live_a_maps_source_backed_fields_to_canonical_delta() {
        let frame = BegodeFrame::try_from_slice(&LIVE_A).expect("fixture frame is valid");
        let telemetry =
            BegodeLiveATelemetry::decode(&frame, BegodePackVoltageProfile::Begode100VFullCharge)
                .expect("live A frame decodes");

        let delta = telemetry.to_delta(ms(42));

        assert_eq!(
            delta,
            TelemetryDelta {
                at_ms: ms(42),
                speed: Some(source_reported(
                    cutout_core::Speed::from_millimetres_per_second(13_360,)
                )),
                voltage: Some(source_reported(Voltage::from_millivolts(90_075))),
                motor_current: Some(source_reported(cutout_core::PhaseCurrent::from_milliamps(
                    -11_800,
                ))),
                power: Some(source_calculated(cutout_core::Power::from_milliwatts(
                    -1_062_885
                ))),
                controller_temperature: Some(source_reported(
                    cutout_core::Temperature::from_millicelsius(27_930,)
                )),
                pwm: Some(source_reported(cutout_core::DutyCycle::from_permille(524))),
                distance: Some(source_reported(cutout_core::Distance::from_millimetres(
                    750_000
                ))),
                battery_level_estimated: Some(source_estimated(
                    cutout_core::BatteryLevel::from_percent(50)
                )),
                ..TelemetryDelta::empty(ms(42))
            }
        );
    }

    #[test]
    fn live_b_maps_distance_and_settings_to_canonical_readbacks() {
        let frame = BegodeFrame::try_from_slice(&LIVE_B).expect("fixture frame is valid");
        let telemetry = BegodeLiveBTelemetry::decode(&frame).expect("live B frame decodes");

        assert_eq!(
            telemetry.to_delta(ms(99)).distance,
            Some(source_reported(cutout_core::Distance::from_millimetres(
                50_000
            )))
        );
        let ReadOnlyResponse::Settings(settings) = telemetry.to_settings_response() else {
            panic!("expected settings response");
        };

        assert_eq!(
            settings.entries(),
            [
                Some(settings_entry(BEGODE_FIELD_SETTINGS_BITS, 0)),
                Some(settings_entry(BEGODE_FIELD_POWER_OFF_TIMER_MINUTES, 15)),
                Some(settings_entry(BEGODE_FIELD_TILTBACK_SPEED_KMH, 50)),
                Some(settings_entry(BEGODE_FIELD_LED_AND_LIGHT_MODE, 0x0302)),
            ]
        );
    }

    #[test]
    fn live_b_settings_bit_zero_selects_imperial_unit_mode() {
        let metric = BegodeFrame::try_from_slice(&LIVE_B).expect("metric frame is valid");
        let imperial =
            BegodeFrame::try_from_slice(&LIVE_B_IMPERIAL).expect("imperial frame is valid");

        assert_eq!(
            BegodeLiveBTelemetry::decode(&metric)
                .expect("metric live B decodes")
                .unit_mode(),
            BegodeUnitMode::Metric
        );
        assert_eq!(
            BegodeLiveBTelemetry::decode(&imperial)
                .expect("imperial live B decodes")
                .unit_mode(),
            BegodeUnitMode::Imperial
        );
    }

    #[test]
    fn telemetry_context_converts_imperial_live_a_values_to_metric_delta() {
        let live_b =
            BegodeFrame::try_from_slice(&LIVE_B_IMPERIAL).expect("imperial live B is valid");
        let live_a = BegodeFrame::try_from_slice(&LIVE_A).expect("live A frame is valid");
        let mut context = BegodeTelemetryContext::default();
        context.observe_live_b(BegodeLiveBTelemetry::decode(&live_b).expect("live B decodes"));
        let telemetry =
            BegodeLiveATelemetry::decode(&live_a, BegodePackVoltageProfile::Begode100VFullCharge)
                .expect("live A decodes");

        let delta = context.live_a_to_delta(telemetry, ms(42));

        assert_eq!(
            delta.speed,
            Some(source_reported(
                cutout_core::Speed::from_millimetres_per_second(21_500)
            ))
        );
        assert_eq!(
            delta.distance,
            Some(source_reported(cutout_core::Distance::from_millimetres(
                1_207_008
            )))
        );
    }

    #[test]
    fn telemetry_context_resets_to_metric_unit_mode() {
        let live_b =
            BegodeFrame::try_from_slice(&LIVE_B_IMPERIAL).expect("imperial live B is valid");
        let mut context = BegodeTelemetryContext::default();
        context.observe_live_b(BegodeLiveBTelemetry::decode(&live_b).expect("live B decodes"));

        context.reset();

        assert_eq!(context.unit_mode(), BegodeUnitMode::Metric);
    }

    #[test]
    fn live_b_imperial_mode_normalizes_total_distance_and_tiltback_speed() {
        let frame = BegodeFrame::try_from_slice(&LIVE_B_IMPERIAL).expect("imperial frame is valid");
        let telemetry = BegodeLiveBTelemetry::decode(&frame).expect("live B decodes");

        assert_eq!(
            telemetry.to_delta(ms(7)).distance,
            Some(source_reported(cutout_core::Distance::from_millimetres(
                80_467
            )))
        );
        let ReadOnlyResponse::Settings(settings) = telemetry.to_settings_response() else {
            panic!("expected settings response");
        };

        assert_eq!(
            settings.entries()[2],
            Some(settings_entry(BEGODE_FIELD_TILTBACK_SPEED_KMH, 80))
        );
    }

    #[test]
    fn live_b_maps_alert_flags_to_diagnostics() {
        let frame = BegodeFrame::try_from_slice(&LIVE_B).expect("fixture frame is valid");
        let telemetry = BegodeLiveBTelemetry::decode(&frame).expect("live B frame decodes");

        let ReadOnlyResponse::Diagnostics(diagnostics) = telemetry.to_diagnostics_response() else {
            panic!("expected diagnostics response");
        };

        let detail = diagnostics.details[0].expect("alert flags are present");
        assert_eq!(
            detail.field,
            RawFieldValue::new(BEGODE_FIELD_ALERT_FLAGS, 5)
        );
        assert_eq!(detail.severity, DiagnosticSeverity::Warning);
        assert_eq!(detail.quality, ValueQuality::Known);
        assert_eq!(detail.verification, VerificationStatus::SourceVerified);
    }

    #[test]
    fn extra_telemetry_maps_true_values_to_canonical_delta() {
        let frame = BegodeFrame::try_from_slice(&EXTRA).expect("fixture frame is valid");
        let telemetry = BegodeExtraTelemetry::decode(&frame).expect("extra frame decodes");

        let delta = telemetry.to_delta(ms(7));

        assert_eq!(
            delta.battery_current,
            Some(source_reported(
                cutout_core::BatteryCurrent::from_milliamps(-1_000)
            ))
        );
        assert_eq!(
            delta.motor_temperature,
            Some(source_reported(
                cutout_core::Temperature::from_millicelsius(42_000)
            ))
        );
        assert_eq!(
            delta.pwm,
            Some(source_reported(cutout_core::DutyCycle::from_permille(-4)))
        );
    }

    #[test]
    fn typed_decoders_reject_wrong_frame_tags() {
        let live_b = BegodeFrame::try_from_slice(&LIVE_B).expect("fixture frame is valid");

        assert_eq!(
            BegodeLiveATelemetry::decode(&live_b, BegodePackVoltageProfile::Begode100VFullCharge),
            Err(BegodeTelemetryError::UnexpectedFrameTag {
                expected: 0,
                actual: 4
            })
        );
    }

    #[test]
    fn falcon_100v_full_charge_battery_level_uses_better_begode_curve() {
        assert_eq!(
            estimate_begode_battery_level(
                Voltage::from_millivolts(75_063),
                BegodePackVoltageProfile::Begode100VFullCharge,
            ),
            cutout_core::BatteryLevel::from_percent(0)
        );
    }

    #[test]
    fn falcon_100v_full_charge_profile_exposes_pack_geometry_without_capacity_guess() {
        let profile = BegodePackVoltageProfile::Begode100VFullCharge;

        assert_eq!(profile.series_cells(), SeriesCount::new(24));
        assert_eq!(
            profile.voltage_range(),
            Voltage::from_millivolts(72_000)..=Voltage::from_millivolts(100_800)
        );
        assert_eq!(profile.nominal_capacity(), None);
    }

    #[test]
    fn begode_100v_profile_records_user_confirmed_falcon_target() {
        let profile = BegodePackVoltageProfile::Begode100VFullCharge;

        assert_eq!(profile.series_cells(), SeriesCount::new(24));
        assert_eq!(
            profile.voltage_range(),
            Voltage::from_millivolts(72_000)..=Voltage::from_millivolts(100_800)
        );
        assert_eq!(profile.nominal_capacity(), None);
    }

    #[test]
    fn falcon_target_voltage_profile_is_explicit_100v_without_capacity() {
        let profile = begode_falcon_target_voltage_profile();

        assert_eq!(profile, BEGODE_FALCON_TARGET_VOLTAGE_PROFILE);
        assert_eq!(profile, BegodePackVoltageProfile::Begode100VFullCharge);
        assert_eq!(profile.series_cells(), SeriesCount::new(24));
        assert_eq!(
            profile.voltage_range(),
            Voltage::from_millivolts(72_000)..=Voltage::from_millivolts(100_800)
        );
        assert_eq!(profile.nominal_capacity(), None);
    }

    #[test]
    fn begode_100v_profile_records_public_falcon_variant_evidence() {
        let profile = BegodePackVoltageProfile::Begode100VFullCharge;

        assert_eq!(profile.series_cells(), SeriesCount::new(24));
        assert_eq!(
            profile.voltage_range(),
            Voltage::from_millivolts(72_000)..=Voltage::from_millivolts(100_800)
        );
        assert_eq!(profile.nominal_capacity(), None);
    }

    #[test]
    fn voltage_profile_selection_uses_explicit_84v_class_evidence() {
        assert_eq!(
            select_begode_pack_voltage_profile([BegodeVoltageEvidence::VoltageClass84V]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge)
        );
    }

    #[test]
    fn voltage_profile_selection_uses_explicit_100v_class_evidence() {
        assert_eq!(
            select_begode_pack_voltage_profile([BegodeVoltageEvidence::VoltageClass100V]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
        );
    }

    #[test]
    fn voltage_profile_selection_rejects_conflicting_classes() {
        assert_eq!(
            select_begode_pack_voltage_profile([
                BegodeVoltageEvidence::VoltageClass84V,
                BegodeVoltageEvidence::VoltageClass100V,
            ]),
            BegodeVoltageProfileSelection::Conflicting
        );
    }

    #[test]
    fn voltage_profile_selection_does_not_guess_from_overlap_voltage() {
        assert_eq!(
            select_begode_pack_voltage_profile([BegodeVoltageEvidence::ObservedPackVoltage(
                Voltage::from_millivolts(80_000)
            )]),
            BegodeVoltageProfileSelection::Missing
        );
    }

    #[test]
    fn voltage_profile_selection_uses_non_overlapping_observed_voltage() {
        assert_eq!(
            select_begode_pack_voltage_profile([BegodeVoltageEvidence::ObservedPackVoltage(
                Voltage::from_millivolts(95_000)
            )]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
        );
        assert_eq!(
            select_begode_pack_voltage_profile([BegodeVoltageEvidence::ObservedPackVoltage(
                Voltage::from_millivolts(65_000)
            )]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge)
        );
    }

    #[test]
    fn voltage_evidence_from_annotations_uses_explicit_84v_capture_label() {
        assert_eq!(
            select_begode_pack_voltage_profile_from_annotations([
                "capture_label=powered_on_stationary",
                "battery=84v",
            ]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge)
        );
    }

    #[test]
    fn voltage_evidence_from_annotations_accepts_84v_label_with_provenance_suffix() {
        assert_eq!(
            select_begode_pack_voltage_profile_from_annotations([
                "battery=84v-user-confirmed-target"
            ]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge)
        );
    }

    #[test]
    fn voltage_evidence_from_annotations_uses_explicit_100v_capture_label() {
        assert_eq!(
            select_begode_pack_voltage_profile_from_annotations([
                "battery=100.8v",
                "cell_model=Samsung 50S",
            ]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
        );
    }

    #[test]
    fn voltage_evidence_from_annotations_uses_non_overlapping_observed_voltage() {
        assert_eq!(
            select_begode_pack_voltage_profile_from_annotations([
                "observed_pack_voltage=95000",
                "capture_label=rolling_forward",
            ]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
        );
    }

    #[test]
    fn voltage_evidence_from_annotations_rejects_conflicts() {
        assert_eq!(
            select_begode_pack_voltage_profile_from_annotations([
                "battery=84v",
                "app_voltage_class=100v",
            ]),
            BegodeVoltageProfileSelection::Conflicting
        );
    }

    #[test]
    fn voltage_evidence_from_annotations_ignores_unknown_or_ambiguous_evidence() {
        assert_eq!(
            select_begode_pack_voltage_profile_from_annotations([
                "model=Falcon",
                "cell_model=Samsung 50S",
                "observed_pack_voltage=80000",
            ]),
            BegodeVoltageProfileSelection::Missing
        );
    }

    #[test]
    fn capacity_evidence_from_annotations_requires_explicit_capacity_value() {
        assert_eq!(
            select_begode_pack_capacity_from_annotations([
                "battery=84v",
                "cell_model=Samsung 50S",
                "series_cells=20",
            ]),
            BegodeCapacitySelection::Missing
        );

        assert_eq!(
            select_begode_pack_capacity_from_annotations([
                "battery=84v",
                "nominal_capacity_mah=10000",
            ]),
            BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                reported_energy: None,
            })
        );
    }

    #[test]
    fn capacity_evidence_from_annotations_preserves_reported_energy_separately() {
        assert_eq!(
            select_begode_pack_capacity_from_annotations([
                "reported_wh=900",
                "nominal_capacity_mah=9000",
            ]),
            BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                nominal_capacity: Some(Capacity::from_milliamp_hours(9_000)),
                reported_energy: Some(Energy::from_watt_hours(900)),
            })
        );
    }

    #[test]
    fn capacity_evidence_from_annotations_rejects_conflicting_values() {
        assert_eq!(
            select_begode_pack_capacity_from_annotations([
                "nominal_capacity=10000",
                "nominal_capacity=9000",
            ]),
            BegodeCapacitySelection::Conflicting
        );
        assert_eq!(
            select_begode_pack_capacity_from_annotations([
                "reported_energy=672",
                "reported_energy=900",
            ]),
            BegodeCapacitySelection::Conflicting
        );
    }

    #[test]
    fn pack_layout_evidence_from_annotations_parses_explicit_layout_values() {
        assert_eq!(
            select_begode_pack_layout_from_annotations([
                "cell_model=Samsung 50S",
                "series_cells=20",
                "parallel_count=1",
            ]),
            BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                cell_model: Some(BegodeCellModel::Samsung50S),
                series_cells: Some(SERIES_CELLS_20),
                parallel_count: Some(PARALLEL_PACKS_1),
            })
        );
    }

    #[test]
    fn pack_layout_evidence_accepts_common_cell_model_spellings() {
        for annotation in ["cell_model=Samsung50S", "cell_model=50s"] {
            assert_eq!(
                select_begode_pack_layout_from_annotations([annotation]),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: None,
                    parallel_count: None,
                })
            );
        }
    }

    #[test]
    fn pack_layout_evidence_from_annotations_reports_missing_and_conflicts() {
        assert_eq!(
            select_begode_pack_layout_from_annotations(["battery=84v", "reported_energy=672"]),
            BegodePackLayoutSelection::Missing
        );
        assert_eq!(
            select_begode_pack_layout_from_annotations(["series_cells=20", "series_cells=24"]),
            BegodePackLayoutSelection::Conflicting
        );
        assert_eq!(
            select_begode_pack_layout_from_annotations(["parallel_count=1", "parallel_cells=2",]),
            BegodePackLayoutSelection::Conflicting
        );
    }

    #[test]
    fn pack_layout_evidence_does_not_select_voltage_or_capacity() {
        let annotations = [
            "cell_model=Samsung 50S",
            "series_cells=20",
            "parallel_count=1",
        ];

        assert_eq!(
            select_begode_pack_voltage_profile_from_annotations(annotations),
            BegodeVoltageProfileSelection::Missing
        );
        assert_eq!(
            select_begode_pack_capacity_from_annotations(annotations),
            BegodeCapacitySelection::Missing
        );
    }

    #[test]
    fn pack_evidence_consistency_rejects_profile_series_mismatch() {
        assert_eq!(
            validate_begode_pack_evidence(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode84VFullCharge,
                ),
                BegodeCapacitySelection::Missing,
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: None,
                    series_cells: Some(SERIES_CELLS_24),
                    parallel_count: None,
                }),
            ),
            BegodePackEvidenceConsistency::Inconsistent
        );
    }

    #[test]
    fn pack_evidence_consistency_accepts_matching_profile_series() {
        assert_eq!(
            validate_begode_pack_evidence(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode100VFullCharge,
                ),
                BegodeCapacitySelection::Missing,
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: None,
                    series_cells: Some(SERIES_CELLS_24),
                    parallel_count: None,
                }),
            ),
            BegodePackEvidenceConsistency::Consistent
        );
    }

    #[test]
    fn pack_evidence_consistency_rejects_84v_20s_2p_50s_as_900wh() {
        assert_eq!(
            validate_begode_pack_evidence(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode84VFullCharge,
                ),
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity: None,
                    reported_energy: Some(Energy::from_watt_hours(900)),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(SERIES_CELLS_20),
                    parallel_count: Some(PARALLEL_PACKS_2),
                }),
            ),
            BegodePackEvidenceConsistency::Inconsistent
        );
    }

    #[test]
    fn pack_evidence_consistency_accepts_100v_24s_2p_50s_as_900wh() {
        assert_eq!(
            validate_begode_pack_evidence(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode100VFullCharge,
                ),
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                    reported_energy: Some(Energy::from_watt_hours(900)),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(SERIES_CELLS_24),
                    parallel_count: Some(PARALLEL_PACKS_2),
                }),
            ),
            BegodePackEvidenceConsistency::Consistent
        );
    }

    #[test]
    fn pack_evidence_consistency_keeps_underconstrained_capacity_incomplete() {
        assert_eq!(
            validate_begode_pack_evidence(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode84VFullCharge,
                ),
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity: None,
                    reported_energy: Some(Energy::from_watt_hours(900)),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(SERIES_CELLS_20),
                    parallel_count: None,
                }),
            ),
            BegodePackEvidenceConsistency::Incomplete
        );
    }

    #[test]
    fn falcon_battery_variant_selects_current_100v_target_from_explicit_voltage_and_layout() {
        assert_eq!(
            select_begode_falcon_battery_variant(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode100VFullCharge,
                ),
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                    reported_energy: Some(Energy::from_watt_hours(900)),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(SERIES_CELLS_24),
                    parallel_count: Some(PARALLEL_PACKS_2),
                }),
            ),
            BegodeFalconBatteryVariantSelection::Selected(
                BegodeFalconBatteryVariant::Current100V24S900WhSamsung50S
            )
        );
    }

    #[test]
    fn falcon_battery_variant_selects_current_100v_900wh_50s_mapping_from_full_evidence() {
        assert_eq!(
            select_begode_falcon_battery_variant(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode100VFullCharge,
                ),
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                    reported_energy: Some(Energy::from_watt_hours(900)),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(SERIES_CELLS_24),
                    parallel_count: Some(PARALLEL_PACKS_2),
                }),
            ),
            BegodeFalconBatteryVariantSelection::Selected(
                BegodeFalconBatteryVariant::Current100V24S900WhSamsung50S,
            )
        );
    }

    #[test]
    fn falcon_battery_variant_does_not_select_from_model_name_or_capacity_only() {
        assert_eq!(
            select_begode_falcon_battery_variant(
                BegodeVoltageProfileSelection::Missing,
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                    reported_energy: Some(Energy::from_watt_hours(900)),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(SERIES_CELLS_24),
                    parallel_count: Some(PARALLEL_PACKS_2),
                }),
            ),
            BegodeFalconBatteryVariantSelection::Missing
        );
    }

    #[test]
    fn falcon_battery_variant_rejects_contradictory_evidence() {
        assert_eq!(
            select_begode_falcon_battery_variant(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode84VFullCharge,
                ),
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                    reported_energy: Some(Energy::from_watt_hours(900)),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(SERIES_CELLS_24),
                    parallel_count: Some(PARALLEL_PACKS_2),
                }),
            ),
            BegodeFalconBatteryVariantSelection::Conflicting
        );
    }

    #[test]
    fn falcon_target_100v_variant_metadata_keeps_capacity_known() {
        let variant = BegodeFalconBatteryVariant::Current100V24S900WhSamsung50S;

        assert_eq!(
            variant.voltage_profile(),
            BegodePackVoltageProfile::Begode100VFullCharge
        );
        assert_eq!(variant.series_cells(), SeriesCount::new(24));
        assert_eq!(variant.cell_model(), Some(BegodeCellModel::Samsung50S));
        assert_eq!(variant.parallel_count(), Some(ParallelCount::new(2)));
        assert_eq!(
            variant.nominal_capacity(),
            Some(Capacity::from_milliamp_hours(10_000))
        );
        assert_eq!(
            variant.reported_energy(),
            Some(Energy::from_watt_hours(900))
        );
    }

    #[test]
    fn falcon_current_100v_variant_metadata_preserves_source_backed_shape() {
        let variant = BegodeFalconBatteryVariant::Current100V24S900WhSamsung50S;

        assert_eq!(
            variant.voltage_profile(),
            BegodePackVoltageProfile::Begode100VFullCharge
        );
        assert_eq!(variant.series_cells(), SeriesCount::new(24));
        assert_eq!(variant.cell_model(), Some(BegodeCellModel::Samsung50S));
        assert_eq!(variant.parallel_count(), Some(ParallelCount::new(2)));
        assert_eq!(
            variant.nominal_capacity(),
            Some(Capacity::from_milliamp_hours(10_000))
        );
        assert_eq!(
            variant.reported_energy(),
            Some(Energy::from_watt_hours(900))
        );
    }

    proptest! {
        #[test]
        fn falcon_battery_level_is_monotonic(first_mv in 60_000i32..=84_000, second_mv in 60_000i32..=84_000) {
            let low = first_mv.min(second_mv);
            let high = first_mv.max(second_mv);

            prop_assert!(
                estimate_begode_battery_level(
                    Voltage::from_millivolts(low),
                    BegodePackVoltageProfile::Begode84VFullCharge,
                ) <= estimate_begode_battery_level(
                    Voltage::from_millivolts(high),
                    BegodePackVoltageProfile::Begode84VFullCharge,
                )
            );
        }

        #[test]
        fn begode_100v_battery_level_is_monotonic(first_mv in 72_000i32..=100_800, second_mv in 72_000i32..=100_800) {
            let low = first_mv.min(second_mv);
            let high = first_mv.max(second_mv);

            prop_assert!(
                estimate_begode_battery_level(
                    Voltage::from_millivolts(low),
                    BegodePackVoltageProfile::Begode100VFullCharge,
                ) <= estimate_begode_battery_level(
                    Voltage::from_millivolts(high),
                    BegodePackVoltageProfile::Begode100VFullCharge,
                )
            );
        }

        #[test]
        fn voltage_profile_selection_maps_low_non_overlap_voltage_to_84v(mv in 1i32..72_000) {
            prop_assert_eq!(
                select_begode_pack_voltage_profile([BegodeVoltageEvidence::ObservedPackVoltage(Voltage::from_millivolts(mv))]),
                BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge)
            );
        }

        #[test]
        fn voltage_profile_selection_maps_high_non_overlap_voltage_to_100v(mv in 84_001i32..=100_800) {
            prop_assert_eq!(
                select_begode_pack_voltage_profile([BegodeVoltageEvidence::ObservedPackVoltage(Voltage::from_millivolts(mv))]),
                BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
            );
        }

        #[test]
        fn voltage_profile_selection_keeps_overlap_voltage_ambiguous(mv in 72_000i32..=84_000) {
            prop_assert_eq!(
                select_begode_pack_voltage_profile([BegodeVoltageEvidence::ObservedPackVoltage(Voltage::from_millivolts(mv))]),
                BegodeVoltageProfileSelection::Missing
            );
        }

        #[test]
        fn live_b_unit_mode_follows_settings_bit_zero(settings_bits in any::<u16>()) {
            let telemetry = BegodeLiveBTelemetry {
                total_distance: cutout_core::Distance::from_millimetres(0),
                settings_bits: BegodeSettingsBits::new(settings_bits),
                power_off_timer: Duration::from_minutes(0),
                tiltback_speed: cutout_core::Speed::from_millimetres_per_second(0),
                led_mode: BegodeLedMode::new(0),
                alert_flags: BegodeAlertFlags::new(0),
                light_mode: BegodeLightMode::new(0),
            };

            prop_assert_eq!(
                telemetry.unit_mode(),
                if settings_bits & 1 == 0 {
                    BegodeUnitMode::Metric
                } else {
                    BegodeUnitMode::Imperial
                }
            );
        }
    }

    const fn source_reported<T>(value: T) -> Measured<T> {
        Measured {
            value,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::SourceVerified,
        }
    }

    const fn source_calculated<T>(value: T) -> Measured<T> {
        Measured {
            value,
            source: ValueSource::Calculated,
            quality: ValueQuality::Known,
            verification: VerificationStatus::SourceVerified,
        }
    }

    const fn source_estimated<T>(value: T) -> Measured<T> {
        Measured {
            value,
            source: ValueSource::Estimated,
            quality: ValueQuality::Inferred,
            verification: VerificationStatus::SourceVerified,
        }
    }

    const fn settings_entry(id: u16, value: i64) -> cutout_core::SettingsEntry {
        cutout_core::SettingsEntry {
            field: RawFieldValue::new(id, value),
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::SourceVerified,
        }
    }
}
