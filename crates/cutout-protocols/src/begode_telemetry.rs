use core::ops::RangeInclusive;

use cutout_core::{
    DiagnosticDetail, DiagnosticReadback, DiagnosticSeverity, Measured, MonotonicMillis,
    RawFieldValue, ReadOnlyResponse, SettingsEntry, SettingsReadback, TelemetryDelta, ValueQuality,
    ValueSource, VerificationStatus,
};
use thiserror::Error;

use crate::{
    BegodeFrame,
    parser::{ByteCursor, ByteOffset},
};

/// Begode speed/distance unit mode inferred from Live B settings bit 0.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BegodeUnitMode {
    /// Wheel wire values are already metric.
    #[default]
    Metric,

    /// Wheel wire values are imperial-scaled and must be converted to metric.
    Imperial,
}

impl BegodeUnitMode {
    const fn from_settings_bits(settings_bits: u16) -> Self {
        if settings_bits & 0x0001 == 0 {
            Self::Metric
        } else {
            Self::Imperial
        }
    }

    fn distance_m_to_mm(self, distance_m: u32) -> u64 {
        match self {
            Self::Metric => u64::from(distance_m) * 1_000,
            Self::Imperial => miles_milli_to_metric_mm(distance_m),
        }
    }

    fn speed_milli_kmh(self, raw_metric_milli_kmh: i32) -> i32 {
        match self {
            Self::Metric => raw_metric_milli_kmh,
            Self::Imperial => mph_milli_to_kmh_milli(raw_metric_milli_kmh),
        }
    }

    fn speed_kmh_u16(self, raw_speed: u16) -> u16 {
        match self {
            Self::Metric => raw_speed,
            Self::Imperial => mph_to_kmh_u16(raw_speed),
        }
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
        at_ms: MonotonicMillis,
    ) -> TelemetryDelta {
        telemetry.to_delta_with_units(at_ms, self.unit_mode)
    }

    /// Converts decoded Live B fields into a normalized telemetry delta.
    #[must_use]
    pub fn live_b_to_delta(
        self,
        telemetry: BegodeLiveBTelemetry,
        at_ms: MonotonicMillis,
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
    BegodePackVoltageProfile::Begode84VFullCharge;

/// Explicit evidence used to select a Begode pack voltage profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BegodeVoltageEvidence {
    /// A capture/app/label explicitly identifies an 84 V class pack.
    VoltageClass84V,

    /// A capture/app/label explicitly identifies a 100.8 V class pack.
    VoltageClass100V,

    /// A capture/app/BMS value reports an observed pack voltage.
    ObservedPackVoltageMv(u32),
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
    /// Nominal pack capacity in milliamp-hours, when explicitly reported.
    pub nominal_capacity_mah: Option<u32>,

    /// Pack energy in watt-hours, when explicitly reported.
    pub reported_wh: Option<u32>,
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
    pub series_cells: Option<u8>,

    /// Parallel cell count, when explicitly reported.
    pub parallel_count: Option<u8>,
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
    /// Current live target hardware: 84 V / 20S, capacity still evidence-gated.
    Target84V20S,

    /// Planned/high-voltage Falcon mapping: 100.8 V / 24S / 900 Wh Samsung 50S.
    Planned100V24S900WhSamsung50S,
}

impl BegodeFalconBatteryVariant {
    /// Voltage profile selected for this Falcon variant.
    #[must_use]
    pub const fn voltage_profile(self) -> BegodePackVoltageProfile {
        match self {
            Self::Target84V20S => BegodePackVoltageProfile::Begode84VFullCharge,
            Self::Planned100V24S900WhSamsung50S => BegodePackVoltageProfile::Begode100VFullCharge,
        }
    }

    /// Series cell count selected for this Falcon variant.
    #[must_use]
    pub const fn series_cells(self) -> u8 {
        self.voltage_profile().series_cells()
    }

    /// Cell model selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn cell_model(self) -> Option<BegodeCellModel> {
        match self {
            Self::Target84V20S => None,
            Self::Planned100V24S900WhSamsung50S => Some(BegodeCellModel::Samsung50S),
        }
    }

    /// Parallel cell count selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn parallel_count(self) -> Option<u8> {
        match self {
            Self::Target84V20S => None,
            Self::Planned100V24S900WhSamsung50S => Some(2),
        }
    }

    /// Nominal pack capacity selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn nominal_capacity_mah(self) -> Option<u32> {
        match self {
            Self::Target84V20S => None,
            Self::Planned100V24S900WhSamsung50S => Some(10_000),
        }
    }

    /// Pack energy selected for this Falcon variant, when evidence-backed.
    #[must_use]
    pub const fn reported_wh(self) -> Option<u32> {
        match self {
            Self::Target84V20S => None,
            Self::Planned100V24S900WhSamsung50S => Some(900),
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
    pub const fn series_cells(self) -> u8 {
        match self {
            Self::Begode84VFullCharge => 20,
            Self::Begode100VFullCharge => 24,
        }
    }

    /// Nominal pack capacity in milliamp-hours, when known.
    #[must_use]
    pub const fn nominal_capacity_mah(self) -> Option<u32> {
        match self {
            Self::Begode84VFullCharge | Self::Begode100VFullCharge => None,
        }
    }

    /// Expected pack voltage range in millivolts.
    #[must_use]
    pub fn voltage_range_mv(self) -> RangeInclusive<u32> {
        match self {
            Self::Begode84VFullCharge => 60_000..=84_000,
            Self::Begode100VFullCharge => 72_000..=100_800,
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
pub const fn select_begode_falcon_battery_variant(
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
pub fn select_begode_pack_voltage_profile(
    evidence: &[BegodeVoltageEvidence],
) -> BegodeVoltageProfileSelection {
    evidence
        .iter()
        .copied()
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
            if evidence.nominal_capacity_mah.is_some() || evidence.reported_wh.is_some() {
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
pub const fn validate_begode_pack_evidence(
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

const fn validate_selected_begode_pack_evidence(
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

const fn validate_selected_begode_pack_capacity(
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

const fn validate_samsung_50s_capacity(
    capacity: BegodeCapacityEvidence,
    series_cells: u8,
    parallel_count: u8,
) -> BegodePackEvidenceConsistency {
    let expected_mah = parallel_count as u32 * 5_000;
    if let Some(nominal_capacity_mah) = capacity.nominal_capacity_mah
        && nominal_capacity_mah != expected_mah
    {
        return BegodePackEvidenceConsistency::Inconsistent;
    }

    if let Some(reported_wh) = capacity.reported_wh {
        let expected_wh = series_cells as u32 * parallel_count as u32 * 18;
        if !within_percent(reported_wh, expected_wh, 5) {
            return BegodePackEvidenceConsistency::Inconsistent;
        }
    }

    BegodePackEvidenceConsistency::Consistent
}

const fn select_consistent_begode_falcon_battery_variant(
    profile: BegodeVoltageProfileSelection,
    capacity: BegodeCapacitySelection,
    layout: BegodePackLayoutSelection,
) -> BegodeFalconBatteryVariantSelection {
    match (profile, capacity, layout) {
        (
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge),
            BegodeCapacitySelection::Missing,
            BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                series_cells: Some(20),
                ..
            }),
        ) => {
            BegodeFalconBatteryVariantSelection::Selected(BegodeFalconBatteryVariant::Target84V20S)
        }
        (
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge),
            BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                nominal_capacity_mah: Some(10_000),
                reported_wh: Some(900),
            }),
            BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                cell_model: Some(BegodeCellModel::Samsung50S),
                series_cells: Some(24),
                parallel_count: Some(2),
            }),
        ) => BegodeFalconBatteryVariantSelection::Selected(
            BegodeFalconBatteryVariant::Planned100V24S900WhSamsung50S,
        ),
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
        "battery" | "app_voltage_class" | "charger_voltage" | "charger_voltage_class" => {
            voltage_class_evidence(value)
        }
        "charger_voltage_mv" | "observed_pack_voltage_mv" | "bms_voltage_mv" | "app_voltage_mv" => {
            parse_mv_evidence(value)
        }
        _ => None,
    }
}

fn capacity_evidence_from_annotation(annotation: &str) -> Option<BegodeCapacityEvidence> {
    let (key, value) = annotation.split_once('=')?;
    let parsed = value.trim().parse::<u32>().ok()?;
    match key.trim() {
        "nominal_capacity_mah" | "capacity_mah" | "pack_capacity_mah" => {
            Some(BegodeCapacityEvidence {
                nominal_capacity_mah: Some(parsed),
                reported_wh: None,
            })
        }
        "reported_wh" | "pack_wh" => Some(BegodeCapacityEvidence {
            nominal_capacity_mah: None,
            reported_wh: Some(parsed),
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
                series_cells: Some(series_cells),
                parallel_count: None,
            })
        }
        "parallel_count" | "parallel_cells" | "parallel_packs" | "pack_parallel_count" => {
            parse_u8_evidence(value).map(|parallel_count| BegodePackLayoutEvidence {
                cell_model: None,
                series_cells: None,
                parallel_count: Some(parallel_count),
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
        .parse::<u32>()
        .ok()
        .map(BegodeVoltageEvidence::ObservedPackVoltageMv)
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
        BegodeVoltageEvidence::ObservedPackVoltageMv(mv) if mv < 72_000 => {
            Some(BegodePackVoltageProfile::Begode84VFullCharge)
        }
        BegodeVoltageEvidence::ObservedPackVoltageMv(mv) if mv > 84_000 && mv <= 100_800 => {
            Some(BegodePackVoltageProfile::Begode100VFullCharge)
        }
        BegodeVoltageEvidence::ObservedPackVoltageMv(_) => None,
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
        nominal_capacity_mah: merge_optional_u32(
            selected.nominal_capacity_mah,
            evidence.nominal_capacity_mah,
        )?,
        reported_wh: merge_optional_u32(selected.reported_wh, evidence.reported_wh)?,
    })
}

fn merge_layout_evidence(
    selected: BegodePackLayoutEvidence,
    evidence: BegodePackLayoutEvidence,
) -> Result<BegodePackLayoutEvidence, ()> {
    Ok(BegodePackLayoutEvidence {
        cell_model: merge_optional_cell_model(selected.cell_model, evidence.cell_model)?,
        series_cells: merge_optional_u8(selected.series_cells, evidence.series_cells)?,
        parallel_count: merge_optional_u8(selected.parallel_count, evidence.parallel_count)?,
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

const fn merge_optional_u8(left: Option<u8>, right: Option<u8>) -> Result<Option<u8>, ()> {
    match (left, right) {
        (None, None) => Ok(None),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (Some(left), Some(right)) if left == right => Ok(Some(left)),
        (Some(_), Some(_)) => Err(()),
    }
}

const fn merge_optional_u32(left: Option<u32>, right: Option<u32>) -> Result<Option<u32>, ()> {
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
    /// Raw unscaled voltage in centivolts.
    pub raw_voltage_centivolts: u16,

    /// Scaled pack voltage in millivolts.
    pub voltage_mv: i32,

    /// Raw signed speed converted to milli-km/h.
    pub speed_milli_kmh: i32,

    /// Full four-byte trip distance candidate in meters.
    pub trip_distance_m: u32,

    /// Low-word trip distance in meters for firmwares that do not populate the high word.
    pub trip_distance_low_m: u16,

    /// Signed phase current in milliamps.
    pub phase_current_ma: i32,

    /// Default MPU6050 IMU temperature in millicelsius.
    pub imu_temperature_mc: i32,

    /// Raw hardware PWM field.
    pub hardware_pwm_raw: i16,

    /// Estimated battery percent derived from voltage.
    pub battery_percent_estimated: u8,
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
        let cursor = ByteCursor::new(frame.as_slice());
        let raw_voltage_centivolts = be_u16(cursor, 2);
        Ok(Self {
            raw_voltage_centivolts,
            voltage_mv: scaled_voltage_mv(raw_voltage_centivolts, profile),
            speed_milli_kmh: raw_speed_to_milli_kmh(be_i16(cursor, 4)),
            trip_distance_m: be_u32(cursor, 6),
            trip_distance_low_m: be_u16(cursor, 8),
            phase_current_ma: i32::from(be_i16(cursor, 10)) * 10,
            imu_temperature_mc: mpu6050_temperature_mc(be_i16(cursor, 12)),
            hardware_pwm_raw: be_i16(cursor, 14),
            battery_percent_estimated: estimate_begode_battery_percent(
                scaled_voltage_mv(raw_voltage_centivolts, profile),
                profile,
            ),
        })
    }

    /// Converts decoded Live A fields into a transport-independent telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        self.to_delta_with_units(at_ms, BegodeUnitMode::Metric)
    }

    /// Converts decoded Live A fields into a transport-independent telemetry delta.
    #[must_use]
    pub fn to_delta_with_units(
        self,
        at_ms: MonotonicMillis,
        unit_mode: BegodeUnitMode,
    ) -> TelemetryDelta {
        TelemetryDelta {
            speed_mm_s: Some(source_reported(milli_kmh_to_mm_s(
                unit_mode.speed_milli_kmh(self.speed_milli_kmh),
            ))),
            voltage_mv: Some(source_reported(self.voltage_mv)),
            motor_current_ma: Some(source_reported(self.phase_current_ma)),
            power_mw: Some(source_calculated(power_mw(
                self.voltage_mv,
                self.phase_current_ma,
            ))),
            controller_temperature_mc: Some(source_reported(self.imu_temperature_mc)),
            pwm_permille: Some(source_reported(raw_pwm_to_permille(self.hardware_pwm_raw))),
            distance_mm: Some(source_reported(
                unit_mode.distance_m_to_mm(u32::from(self.trip_distance_low_m)),
            )),
            battery_percent_estimated: Some(source_estimated(self.battery_percent_estimated)),
            ..TelemetryDelta::empty(at_ms)
        }
    }
}

/// Secondary Begode live telemetry decoded from frame tag `0x04`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BegodeLiveBTelemetry {
    /// Lifetime total distance in meters.
    pub total_distance_m: u32,

    /// Raw settings bitfield.
    pub settings_bits: u16,

    /// Power-off timer in minutes.
    pub power_off_timer_minutes: u16,

    /// Tiltback / max-speed field in km/h.
    pub tiltback_speed_kmh: u16,

    /// LED mode.
    pub led_mode: u8,

    /// Raw alert bitfield.
    pub alert_flags: u8,

    /// Low two bits of the light-mode byte.
    pub light_mode: u8,
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
        let cursor = ByteCursor::new(frame.as_slice());
        Ok(Self {
            total_distance_m: be_u32(cursor, 2),
            settings_bits: be_u16(cursor, 6),
            power_off_timer_minutes: be_u16(cursor, 8),
            tiltback_speed_kmh: be_u16(cursor, 10),
            led_mode: byte(cursor, 13),
            alert_flags: byte(cursor, 14),
            light_mode: byte(cursor, 15) & 0x03,
        })
    }

    /// Converts decoded Live B fields into a telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        self.to_delta_with_units(at_ms)
    }

    /// Converts decoded Live B fields into a telemetry delta with unit normalization.
    #[must_use]
    pub fn to_delta_with_units(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        TelemetryDelta {
            distance_mm: Some(source_reported(
                self.unit_mode().distance_m_to_mm(self.total_distance_m),
            )),
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
        ReadOnlyResponse::Settings(SettingsReadback {
            entries: [
                Some(settings_entry(
                    BEGODE_FIELD_SETTINGS_BITS,
                    i64::from(self.settings_bits),
                )),
                Some(settings_entry(
                    BEGODE_FIELD_POWER_OFF_TIMER_MINUTES,
                    i64::from(self.power_off_timer_minutes),
                )),
                Some(settings_entry(
                    BEGODE_FIELD_TILTBACK_SPEED_KMH,
                    i64::from(self.unit_mode().speed_kmh_u16(self.tiltback_speed_kmh)),
                )),
                Some(settings_entry(
                    BEGODE_FIELD_LED_AND_LIGHT_MODE,
                    i64::from((u16::from(self.led_mode) << 8) | u16::from(self.light_mode)),
                )),
            ],
        })
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
                        i64::from(self.alert_flags),
                    ),
                    severity: if self.alert_flags == 0 {
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
    pub battery_current_ma: i32,

    /// Motor temperature in millicelsius.
    pub motor_temperature_mc: i32,

    /// True PWM raw field.
    pub true_pwm_raw: i16,
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
        let cursor = ByteCursor::new(frame.as_slice());
        Ok(Self {
            battery_current_ma: i32::from(be_i16(cursor, 2)) * 10,
            motor_temperature_mc: i32::from(be_i16(cursor, 6)) * 1_000,
            true_pwm_raw: be_i16(cursor, 8),
        })
    }

    /// Converts decoded extra telemetry into a transport-independent delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        TelemetryDelta {
            battery_current_ma: Some(source_reported(self.battery_current_ma)),
            motor_temperature_mc: Some(source_reported(self.motor_temperature_mc)),
            pwm_permille: Some(source_reported(raw_pwm_to_permille(self.true_pwm_raw))),
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
pub fn estimate_begode_battery_percent(voltage_mv: i32, profile: BegodePackVoltageProfile) -> u8 {
    let raw_centivolts = unscaled_centivolts(voltage_mv, profile);
    if raw_centivolts <= 5_120 {
        return 0;
    }
    if raw_centivolts <= 5_440 {
        return percent_from_i32(div_round(raw_centivolts - 5_120, 36).clamp(0, 100));
    }
    if raw_centivolts <= 6_680 {
        return percent_from_i32(div_round((raw_centivolts - 5_320) * 10, 136).clamp(0, 100));
    }
    100
}

fn require_tag(frame: &BegodeFrame, expected: u8) -> Result<(), BegodeTelemetryError> {
    let actual = frame.tag();
    if actual.get() == u16::from(expected) {
        Ok(())
    } else {
        Err(BegodeTelemetryError::UnexpectedFrameTag {
            expected,
            actual: u8::try_from(actual.get()).unwrap_or_default(),
        })
    }
}

fn scaled_voltage_mv(raw_centivolts: u16, profile: BegodePackVoltageProfile) -> i32 {
    (i32::from(raw_centivolts) * 10 * profile.scaler_milli() + 500) / 1_000
}

fn unscaled_centivolts(voltage_mv: i32, profile: BegodePackVoltageProfile) -> i32 {
    (voltage_mv * 100 + profile.scaler_milli() / 2) / profile.scaler_milli()
}

fn raw_speed_to_milli_kmh(raw_speed: i16) -> i32 {
    i32::from(raw_speed) * 36
}

fn milli_kmh_to_mm_s(value: i32) -> i32 {
    value * 5 / 18
}

fn power_mw(voltage_mv: i32, current_ma: i32) -> i64 {
    i64::from(voltage_mv) * i64::from(current_ma) / 1_000
}

fn raw_pwm_to_permille(raw_pwm: i16) -> i16 {
    raw_pwm / 10
}

fn mpu6050_temperature_mc(raw_temperature: i16) -> i32 {
    36_530 + (i32::from(raw_temperature) * 1_000) / 340
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

fn byte(cursor: ByteCursor<'_>, offset: usize) -> u8 {
    cursor.byte(ByteOffset::new(offset)).unwrap_or_default()
}

fn be_u16(cursor: ByteCursor<'_>, offset: usize) -> u16 {
    cursor.be_u16(ByteOffset::new(offset)).unwrap_or_default()
}

fn be_i16(cursor: ByteCursor<'_>, offset: usize) -> i16 {
    cursor.be_i16(ByteOffset::new(offset)).unwrap_or_default()
}

fn be_u32(cursor: ByteCursor<'_>, offset: usize) -> u32 {
    cursor.be_u32(ByteOffset::new(offset)).unwrap_or_default()
}

const fn div_round(numerator: i32, denominator: i32) -> i32 {
    (numerator + denominator / 2) / denominator
}

fn mph_milli_to_kmh_milli(value: i32) -> i32 {
    let scaled = i64::from(value) * 1_609_344;
    match i32::try_from(scaled / 1_000_000) {
        Ok(value) => value,
        Err(_) => {
            if scaled.is_negative() {
                i32::MIN
            } else {
                i32::MAX
            }
        }
    }
}

fn miles_milli_to_metric_mm(value: u32) -> u64 {
    u64::from(value) * 1_609_344 / 1_000
}

fn mph_to_kmh_u16(value: u16) -> u16 {
    let scaled = u32::from(value) * 1_609_344;
    u16::try_from(scaled / 1_000_000).unwrap_or(u16::MAX)
}

fn percent_from_i32(percent: i32) -> u8 {
    match u8::try_from(percent) {
        Ok(value) => value,
        Err(_) => {
            if percent < 0 {
                0
            } else {
                100
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BEGODE_FALCON_TARGET_VOLTAGE_PROFILE, BegodeCapacityEvidence, BegodeCapacitySelection,
        BegodeCellModel, BegodeFalconBatteryVariant, BegodeFalconBatteryVariantSelection,
        BegodePackLayoutEvidence, BegodePackLayoutSelection, BegodeVoltageEvidence,
        BegodeVoltageProfileSelection, begode_falcon_target_voltage_profile,
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
        estimate_begode_battery_percent, validate_begode_pack_evidence,
    };
    use cutout_core::{
        DiagnosticSeverity, Measured, ProtocolTag, RawFieldValue, ReadOnlyResponse, TelemetryDelta,
        ValueQuality, ValueSource, VerificationStatus,
    };
    use proptest::prelude::*;

    const LIVE_A: [u8; 24] = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
    const LIVE_B: [u8; 24] = hex_literal::hex!("55aa000000320000000f003200030502000004185a5a5a5a");
    const LIVE_B_IMPERIAL: [u8; 24] =
        hex_literal::hex!("55aa000000320001000f003200030502000004185a5a5a5a");
    const EXTRA: [u8; 24] = hex_literal::hex!("55aaff9c0000002affd8000000000000000007185a5a5a5a");

    #[test]
    fn live_a_decodes_source_backed_primary_fields_for_falcon_84v_full_charge() {
        let frame = BegodeFrame::try_from_slice(&LIVE_A).expect("fixture frame is valid");
        assert_eq!(frame.tag(), ProtocolTag::new(0x00));
        let telemetry =
            BegodeLiveATelemetry::decode(&frame, BegodePackVoltageProfile::Begode84VFullCharge)
                .expect("live A frame decodes");

        assert_eq!(telemetry.raw_voltage_centivolts, 6005);
        assert_eq!(telemetry.voltage_mv, 75_063);
        assert_eq!(telemetry.speed_milli_kmh, 48_096);
        assert_eq!(telemetry.trip_distance_m, 0x0076_02ee);
        assert_eq!(telemetry.trip_distance_low_m, 750);
        assert_eq!(telemetry.phase_current_ma, -11_800);
        assert_eq!(telemetry.imu_temperature_mc, 27_930);
        assert_eq!(telemetry.hardware_pwm_raw, 0x1481);
        assert_eq!(telemetry.battery_percent_estimated, 50);
    }

    #[test]
    fn live_b_decodes_total_mileage_and_settings_fields() {
        let frame = BegodeFrame::try_from_slice(&LIVE_B).expect("fixture frame is valid");
        let telemetry = BegodeLiveBTelemetry::decode(&frame).expect("live B frame decodes");

        assert_eq!(telemetry.total_distance_m, 50);
        assert_eq!(telemetry.settings_bits, 0);
        assert_eq!(telemetry.power_off_timer_minutes, 15);
        assert_eq!(telemetry.tiltback_speed_kmh, 50);
        assert_eq!(telemetry.led_mode, 3);
        assert_eq!(telemetry.alert_flags, 5);
        assert_eq!(telemetry.light_mode, 2);
    }

    #[test]
    fn extra_telemetry_decodes_true_current_motor_temperature_and_pwm() {
        let frame = BegodeFrame::try_from_slice(&EXTRA).expect("fixture frame is valid");
        let telemetry = BegodeExtraTelemetry::decode(&frame).expect("extra frame decodes");

        assert_eq!(telemetry.battery_current_ma, -1_000);
        assert_eq!(telemetry.motor_temperature_mc, 42_000);
        assert_eq!(telemetry.true_pwm_raw, -40);
    }

    #[test]
    fn live_a_maps_source_backed_fields_to_canonical_delta() {
        let frame = BegodeFrame::try_from_slice(&LIVE_A).expect("fixture frame is valid");
        let telemetry =
            BegodeLiveATelemetry::decode(&frame, BegodePackVoltageProfile::Begode84VFullCharge)
                .expect("live A frame decodes");

        let delta = telemetry.to_delta(42);

        assert_eq!(
            delta,
            TelemetryDelta {
                at_ms: 42,
                speed_mm_s: Some(source_reported(13_360)),
                voltage_mv: Some(source_reported(75_063)),
                battery_current_ma: None,
                motor_current_ma: Some(source_reported(-11_800)),
                power_mw: Some(source_calculated(-885_743)),
                controller_temperature_mc: Some(source_reported(27_930)),
                motor_temperature_mc: None,
                battery_temperature_mc: None,
                pwm_permille: Some(source_reported(524)),
                distance_mm: Some(source_reported(750_000)),
                pitch_mdeg: None,
                roll_mdeg: None,
                battery_percent_reported: None,
                battery_percent_estimated: Some(source_estimated(50)),
            }
        );
    }

    #[test]
    fn live_b_maps_distance_and_settings_to_canonical_readbacks() {
        let frame = BegodeFrame::try_from_slice(&LIVE_B).expect("fixture frame is valid");
        let telemetry = BegodeLiveBTelemetry::decode(&frame).expect("live B frame decodes");

        assert_eq!(
            telemetry.to_delta(99).distance_mm,
            Some(source_reported(50_000))
        );
        let ReadOnlyResponse::Settings(settings) = telemetry.to_settings_response() else {
            panic!("expected settings response");
        };

        assert_eq!(
            settings.entries,
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
            BegodeLiveATelemetry::decode(&live_a, BegodePackVoltageProfile::Begode84VFullCharge)
                .expect("live A decodes");

        let delta = context.live_a_to_delta(telemetry, 42);

        assert_eq!(delta.speed_mm_s, Some(source_reported(21_500)));
        assert_eq!(delta.distance_mm, Some(source_reported(1_207_008)));
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
            telemetry.to_delta(7).distance_mm,
            Some(source_reported(80_467))
        );
        let ReadOnlyResponse::Settings(settings) = telemetry.to_settings_response() else {
            panic!("expected settings response");
        };

        assert_eq!(
            settings.entries[2],
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

        let delta = telemetry.to_delta(7);

        assert_eq!(delta.battery_current_ma, Some(source_reported(-1_000)));
        assert_eq!(delta.motor_temperature_mc, Some(source_reported(42_000)));
        assert_eq!(delta.pwm_permille, Some(source_reported(-4)));
    }

    #[test]
    fn typed_decoders_reject_wrong_frame_tags() {
        let live_b = BegodeFrame::try_from_slice(&LIVE_B).expect("fixture frame is valid");

        assert_eq!(
            BegodeLiveATelemetry::decode(&live_b, BegodePackVoltageProfile::Begode84VFullCharge),
            Err(BegodeTelemetryError::UnexpectedFrameTag {
                expected: 0,
                actual: 4
            })
        );
    }

    #[test]
    fn falcon_84v_full_charge_battery_percent_uses_better_begode_curve() {
        assert_eq!(
            estimate_begode_battery_percent(75_063, BegodePackVoltageProfile::Begode84VFullCharge),
            50
        );
    }

    #[test]
    fn falcon_84v_full_charge_profile_exposes_pack_geometry_without_capacity_guess() {
        let profile = BegodePackVoltageProfile::Begode84VFullCharge;

        assert_eq!(profile.series_cells(), 20);
        assert_eq!(profile.voltage_range_mv(), 60_000..=84_000);
        assert_eq!(profile.nominal_capacity_mah(), None);
    }

    #[test]
    fn begode_84v_profile_records_user_confirmed_falcon_target() {
        let profile = BegodePackVoltageProfile::Begode84VFullCharge;

        assert_eq!(profile.series_cells(), 20);
        assert_eq!(profile.voltage_range_mv(), 60_000..=84_000);
        assert_eq!(profile.nominal_capacity_mah(), None);
    }

    #[test]
    fn falcon_target_voltage_profile_is_explicit_84v_without_capacity() {
        let profile = begode_falcon_target_voltage_profile();

        assert_eq!(profile, BEGODE_FALCON_TARGET_VOLTAGE_PROFILE);
        assert_eq!(profile, BegodePackVoltageProfile::Begode84VFullCharge);
        assert_eq!(profile.series_cells(), 20);
        assert_eq!(profile.voltage_range_mv(), 60_000..=84_000);
        assert_eq!(profile.nominal_capacity_mah(), None);
    }

    #[test]
    fn begode_100v_profile_records_public_falcon_variant_evidence() {
        let profile = BegodePackVoltageProfile::Begode100VFullCharge;

        assert_eq!(profile.series_cells(), 24);
        assert_eq!(profile.voltage_range_mv(), 72_000..=100_800);
        assert_eq!(profile.nominal_capacity_mah(), None);
    }

    #[test]
    fn voltage_profile_selection_uses_explicit_84v_class_evidence() {
        assert_eq!(
            select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::VoltageClass84V]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge)
        );
    }

    #[test]
    fn voltage_profile_selection_uses_explicit_100v_class_evidence() {
        assert_eq!(
            select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::VoltageClass100V]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
        );
    }

    #[test]
    fn voltage_profile_selection_rejects_conflicting_classes() {
        assert_eq!(
            select_begode_pack_voltage_profile(&[
                BegodeVoltageEvidence::VoltageClass84V,
                BegodeVoltageEvidence::VoltageClass100V,
            ]),
            BegodeVoltageProfileSelection::Conflicting
        );
    }

    #[test]
    fn voltage_profile_selection_does_not_guess_from_overlap_voltage() {
        assert_eq!(
            select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::ObservedPackVoltageMv(
                80_000
            )]),
            BegodeVoltageProfileSelection::Missing
        );
    }

    #[test]
    fn voltage_profile_selection_uses_non_overlapping_observed_voltage() {
        assert_eq!(
            select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::ObservedPackVoltageMv(
                95_000
            )]),
            BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
        );
        assert_eq!(
            select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::ObservedPackVoltageMv(
                65_000
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
                "observed_pack_voltage_mv=95000",
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
                "observed_pack_voltage_mv=80000",
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
                nominal_capacity_mah: Some(10_000),
                reported_wh: None,
            })
        );
    }

    #[test]
    fn capacity_evidence_from_annotations_preserves_reported_wh_separately() {
        assert_eq!(
            select_begode_pack_capacity_from_annotations([
                "reported_wh=900",
                "nominal_capacity_mah=9000",
            ]),
            BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                nominal_capacity_mah: Some(9_000),
                reported_wh: Some(900),
            })
        );
    }

    #[test]
    fn capacity_evidence_from_annotations_rejects_conflicting_values() {
        assert_eq!(
            select_begode_pack_capacity_from_annotations([
                "nominal_capacity_mah=10000",
                "nominal_capacity_mah=9000",
            ]),
            BegodeCapacitySelection::Conflicting
        );
        assert_eq!(
            select_begode_pack_capacity_from_annotations(["reported_wh=672", "reported_wh=900",]),
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
                series_cells: Some(20),
                parallel_count: Some(1),
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
            select_begode_pack_layout_from_annotations(["battery=84v", "reported_wh=672"]),
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
                    series_cells: Some(24),
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
                    BegodePackVoltageProfile::Begode84VFullCharge,
                ),
                BegodeCapacitySelection::Missing,
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: None,
                    series_cells: Some(20),
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
                    nominal_capacity_mah: None,
                    reported_wh: Some(900),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(20),
                    parallel_count: Some(2),
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
                    nominal_capacity_mah: Some(10_000),
                    reported_wh: Some(900),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(24),
                    parallel_count: Some(2),
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
                    nominal_capacity_mah: None,
                    reported_wh: Some(900),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(20),
                    parallel_count: None,
                }),
            ),
            BegodePackEvidenceConsistency::Incomplete
        );
    }

    #[test]
    fn falcon_battery_variant_selects_current_84v_target_from_explicit_voltage_and_layout() {
        assert_eq!(
            select_begode_falcon_battery_variant(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode84VFullCharge,
                ),
                BegodeCapacitySelection::Missing,
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: None,
                    series_cells: Some(20),
                    parallel_count: None,
                }),
            ),
            BegodeFalconBatteryVariantSelection::Selected(BegodeFalconBatteryVariant::Target84V20S)
        );
    }

    #[test]
    fn falcon_battery_variant_selects_planned_100v_900wh_50s_mapping_from_full_evidence() {
        assert_eq!(
            select_begode_falcon_battery_variant(
                BegodeVoltageProfileSelection::Selected(
                    BegodePackVoltageProfile::Begode100VFullCharge,
                ),
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity_mah: Some(10_000),
                    reported_wh: Some(900),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(24),
                    parallel_count: Some(2),
                }),
            ),
            BegodeFalconBatteryVariantSelection::Selected(
                BegodeFalconBatteryVariant::Planned100V24S900WhSamsung50S,
            )
        );
    }

    #[test]
    fn falcon_battery_variant_does_not_select_from_model_name_or_capacity_only() {
        assert_eq!(
            select_begode_falcon_battery_variant(
                BegodeVoltageProfileSelection::Missing,
                BegodeCapacitySelection::Selected(BegodeCapacityEvidence {
                    nominal_capacity_mah: Some(10_000),
                    reported_wh: Some(900),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(24),
                    parallel_count: Some(2),
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
                    nominal_capacity_mah: Some(10_000),
                    reported_wh: Some(900),
                }),
                BegodePackLayoutSelection::Selected(BegodePackLayoutEvidence {
                    cell_model: Some(BegodeCellModel::Samsung50S),
                    series_cells: Some(24),
                    parallel_count: Some(2),
                }),
            ),
            BegodeFalconBatteryVariantSelection::Conflicting
        );
    }

    #[test]
    fn falcon_target_84v_variant_metadata_keeps_capacity_unknown() {
        let variant = BegodeFalconBatteryVariant::Target84V20S;

        assert_eq!(
            variant.voltage_profile(),
            BegodePackVoltageProfile::Begode84VFullCharge
        );
        assert_eq!(variant.series_cells(), 20);
        assert_eq!(variant.cell_model(), None);
        assert_eq!(variant.parallel_count(), None);
        assert_eq!(variant.nominal_capacity_mah(), None);
        assert_eq!(variant.reported_wh(), None);
    }

    #[test]
    fn falcon_planned_100v_variant_metadata_preserves_source_backed_shape() {
        let variant = BegodeFalconBatteryVariant::Planned100V24S900WhSamsung50S;

        assert_eq!(
            variant.voltage_profile(),
            BegodePackVoltageProfile::Begode100VFullCharge
        );
        assert_eq!(variant.series_cells(), 24);
        assert_eq!(variant.cell_model(), Some(BegodeCellModel::Samsung50S));
        assert_eq!(variant.parallel_count(), Some(2));
        assert_eq!(variant.nominal_capacity_mah(), Some(10_000));
        assert_eq!(variant.reported_wh(), Some(900));
    }

    proptest! {
        #[test]
        fn falcon_battery_percent_is_monotonic(first_mv in 60_000i32..=84_000, second_mv in 60_000i32..=84_000) {
            let low = first_mv.min(second_mv);
            let high = first_mv.max(second_mv);

            prop_assert!(
                estimate_begode_battery_percent(low, BegodePackVoltageProfile::Begode84VFullCharge)
                    <= estimate_begode_battery_percent(high, BegodePackVoltageProfile::Begode84VFullCharge)
            );
        }

        #[test]
        fn begode_100v_battery_percent_is_monotonic(first_mv in 72_000i32..=100_800, second_mv in 72_000i32..=100_800) {
            let low = first_mv.min(second_mv);
            let high = first_mv.max(second_mv);

            prop_assert!(
                estimate_begode_battery_percent(low, BegodePackVoltageProfile::Begode100VFullCharge)
                    <= estimate_begode_battery_percent(high, BegodePackVoltageProfile::Begode100VFullCharge)
            );
        }

        #[test]
        fn voltage_profile_selection_maps_low_non_overlap_voltage_to_84v(mv in 1u32..72_000) {
            prop_assert_eq!(
                select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::ObservedPackVoltageMv(mv)]),
                BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode84VFullCharge)
            );
        }

        #[test]
        fn voltage_profile_selection_maps_high_non_overlap_voltage_to_100v(mv in 84_001u32..=100_800) {
            prop_assert_eq!(
                select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::ObservedPackVoltageMv(mv)]),
                BegodeVoltageProfileSelection::Selected(BegodePackVoltageProfile::Begode100VFullCharge)
            );
        }

        #[test]
        fn voltage_profile_selection_keeps_overlap_voltage_ambiguous(mv in 72_000u32..=84_000) {
            prop_assert_eq!(
                select_begode_pack_voltage_profile(&[BegodeVoltageEvidence::ObservedPackVoltageMv(mv)]),
                BegodeVoltageProfileSelection::Missing
            );
        }

        #[test]
        fn live_b_unit_mode_follows_settings_bit_zero(settings_bits in any::<u16>()) {
            let telemetry = BegodeLiveBTelemetry {
                total_distance_m: 0,
                settings_bits,
                power_off_timer_minutes: 0,
                tiltback_speed_kmh: 0,
                led_mode: 0,
                alert_flags: 0,
                light_mode: 0,
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
