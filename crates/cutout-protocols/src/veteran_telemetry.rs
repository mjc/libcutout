use core::ops::RangeInclusive;
use cutout_core::{
    Angle, BmsCellValuesPerPage, BmsLayoutSpec, BmsPageSelectorSpec, BmsParallelPacks,
    BmsTemperatureValuesPerPage, ChargeMode, Distance, Duration, DutyCycle, FirmwareInfo, Measured,
    MonotonicMillis, PackSeriesCells, Percent, PhaseCurrent, Power, ProtocolSelector,
    RawFieldValue, ReadOnlyResponse, SettingsEntry, SettingsReadback, Speed, TelemetryDelta,
    Temperature, ValueQuality, ValueSource, VerificationStatus, Voltage,
};
use thiserror::Error;

use crate::{
    BatteryVoltageProfile, SAMSUNG_50S_PROFILE, VeteranFrame,
    parser::{ByteCursor, ByteOffset},
};
use crate::{VETERAN_BMS_CELL_VALUES_PER_PAGE, classify_veteran_bms_selector};

/// Samsung 50S profile minimum pack voltage for a NOSFET Aero 30s pack.
pub const NOSFET_AERO_MIN_VOLTAGE_MV: i32 = 91_000;

/// Samsung 50S profile maximum pack voltage for a NOSFET Aero 30s pack.
pub const NOSFET_AERO_MAX_VOLTAGE_MV: i32 = 126_000;

const VETERAN_BMS_LAYOUT_VERIFICATION: VerificationStatus = VerificationStatus::Inferred;

const VETERAN_BMS_SELECTOR_VERIFICATION: VerificationStatus = VerificationStatus::SourceVerified;

const VETERAN_BMS_TEMPERATURE_VALUES_PER_PAGE_U8: u8 = 6;

const VETERAN_BMS_SELECTORS: [BmsPageSelectorSpec; 9] = [
    bms_page_selector(0),
    bms_page_selector(1),
    bms_page_selector(2),
    bms_page_selector(3),
    bms_page_selector(4),
    bms_page_selector(5),
    bms_page_selector(6),
    bms_page_selector(7),
    bms_page_selector(8),
];

const VETERAN_BMS_30S_2P_LAYOUT: BmsLayoutSpec = veteran_bms_layout(30, 2);
const VETERAN_BMS_36S_2P_LAYOUT: BmsLayoutSpec = veteran_bms_layout(36, 2);
const VETERAN_BMS_36S_4P_LAYOUT: BmsLayoutSpec = veteran_bms_layout(36, 4);
const VETERAN_BMS_36S_6P_LAYOUT: BmsLayoutSpec = veteran_bms_layout(36, 6);
const VETERAN_BMS_42S_6P_LAYOUT: BmsLayoutSpec = veteran_bms_layout(42, 6);

const fn bms_page_selector(selector: u8) -> BmsPageSelectorSpec {
    BmsPageSelectorSpec {
        selector: ProtocolSelector::new(selector),
        kind: classify_veteran_bms_selector(ProtocolSelector::new(selector)),
        verification: VETERAN_BMS_SELECTOR_VERIFICATION,
    }
}

const fn veteran_bms_layout(series_cells: u8, parallel_packs: u8) -> BmsLayoutSpec {
    BmsLayoutSpec {
        series_cells: PackSeriesCells::new(series_cells),
        parallel_packs: BmsParallelPacks::new(parallel_packs),
        cell_values_per_page: BmsCellValuesPerPage::new(VETERAN_BMS_CELL_VALUES_PER_PAGE),
        temperature_values_per_page: BmsTemperatureValuesPerPage::new(
            VETERAN_BMS_TEMPERATURE_VALUES_PER_PAGE_U8,
        ),
        selectors: &VETERAN_BMS_SELECTORS,
        verification: VETERAN_BMS_LAYOUT_VERIFICATION,
    }
}

/// Minimal read-only telemetry decoded from a Veteran/LeaperKim/NOSFET frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranTelemetry {
    /// Firmware/model version fields.
    pub firmware: VeteranFirmwareVersion,

    /// Pack voltage.
    pub voltage: Voltage,

    /// Speed.
    pub speed: Speed,

    /// Trip distance.
    pub trip_distance: Distance,

    /// Total distance.
    pub total_distance: Distance,

    /// Phase current reported by the realtime frame.
    pub phase_current: PhaseCurrent,

    /// MOSFET/controller temperature.
    pub mosfet_temperature: Temperature,

    /// Auto shutdown time remaining.
    pub auto_shutdown_time_remaining: Duration,

    /// Raw charge-mode field retained for protocol evidence and settings readback.
    pub raw_charge_mode: VeteranRawChargeMode,

    /// Decoded charging state.
    pub charge_mode: ChargeMode,

    /// Speed alert threshold.
    pub speed_alert: Speed,

    /// Speed tiltback threshold.
    pub speed_tiltback: Speed,

    /// Raw pedals-mode field.
    pub pedals_mode: u16,

    /// Pitch.
    pub pitch: Angle,

    /// Hardware PWM duty cycle.
    pub hardware_pwm: DutyCycle,

    /// Cutout-estimated battery percentage from capture-backed pack range.
    pub battery_percent_estimated: Percent,
}

/// Raw Veteran charge-mode field retained for protocol evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranRawChargeMode(u16);

impl VeteranRawChargeMode {
    /// Creates a raw charge-mode evidence value from the wire field.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the protocol-native raw field value.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Static Veteran/LeaperKim/NOSFET model mapping derived from firmware model id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VeteranModelProfile {
    /// Firmware model id from the version word.
    pub model_id: u16,

    /// User-facing model family name.
    pub name: &'static str,

    /// Series-connected cell count for pack-voltage interpretation.
    pub cell_count: u8,

    /// Parallel cell count for model pack configuration.
    pub parallel_cells: u8,

    /// Nominal pack capacity in milliamp-hours, when known.
    pub nominal_capacity_mah: Option<u32>,

    /// Single-cell battery curve used for estimated battery percent, when known.
    pub battery_profile: Option<&'static BatteryVoltageProfile>,

    /// Pack-voltage range used for estimated battery percent.
    pub voltage_range_mv: RangeInclusive<i32>,

    /// Whether the fixed telemetry hardware-PWM field is valid.
    pub has_pwm_readback: bool,

    /// Whether the model family emits smart-BMS pages.
    pub has_smart_bms: bool,

    /// Static smart-BMS layout for model-specific page interpretation.
    pub bms_layout: Option<&'static BmsLayoutSpec>,

    /// Observed app-vs-device odometer display offset in meters, when known.
    ///
    /// This is model evidence, not a parser correction. Keep canonical decoded
    /// odometer fields unchanged until same-instant raw/app/LCD captures prove
    /// where the offset is applied.
    pub observed_app_odometer_offset_m: Option<i32>,

    /// Whether horn requires the newer binary `LeaperKim` command frame.
    pub requires_binary_horn: bool,
}

impl VeteranModelProfile {
    /// Returns the known profile for a Veteran firmware model id.
    #[must_use]
    pub fn from_model_id(model_id: u16) -> Option<Self> {
        let profile = match model_id {
            0 | 1 => Self::new_linear(model_id, "Veteran Sherman", 24, 79_350..=98_700),
            2 => Self::new_linear(model_id, "Veteran Abrams", 24, 79_350..=98_700),
            3 => Self::new_linear(model_id, "Veteran Sherman S", 24, 79_350..=98_700),
            4 => Self::new_with_battery_profile(
                model_id,
                "Veteran Patton",
                30,
                2,
                &SAMSUNG_50S_PROFILE,
            ),
            5 => Self::new_with_battery_profile(
                model_id,
                "Veteran Lynx",
                36,
                4,
                &SAMSUNG_50S_PROFILE,
            ),
            6 => Self::new_with_battery_profile(
                model_id,
                "Veteran Sherman L",
                36,
                6,
                &SAMSUNG_50S_PROFILE,
            ),
            7 => Self::new_with_battery_profile(
                model_id,
                "Veteran Patton S",
                30,
                2,
                &SAMSUNG_50S_PROFILE,
            ),
            8 => Self::new_with_battery_profile(
                model_id,
                "Veteran Oryx",
                42,
                6,
                &SAMSUNG_50S_PROFILE,
            ),
            9 => Self::new_with_battery_profile(
                model_id,
                "Veteran Lynx S",
                36,
                4,
                &SAMSUNG_50S_PROFILE,
            ),
            42 => {
                Self::new_with_battery_profile(model_id, "NOSFET Apex", 36, 4, &SAMSUNG_50S_PROFILE)
            }
            43 => {
                Self::new_with_battery_profile(model_id, "NOSFET Aero", 30, 2, &SAMSUNG_50S_PROFILE)
                    .with_observed_app_odometer_offset_m(805)
            }
            44 => {
                Self::new_with_battery_profile(model_id, "NOSFET Aeon", 36, 2, &SAMSUNG_50S_PROFILE)
            }
            _ => return None,
        };
        Some(profile)
    }

    const fn new_linear(
        model_id: u16,
        name: &'static str,
        cell_count: u8,
        voltage_range_mv: RangeInclusive<i32>,
    ) -> Self {
        Self {
            model_id,
            name,
            cell_count,
            parallel_cells: 1,
            nominal_capacity_mah: None,
            battery_profile: None,
            voltage_range_mv,
            has_pwm_readback: model_id >= 2,
            has_smart_bms: model_id >= 5 || matches!(model_id, 4 | 7 | 42..=44),
            bms_layout: None,
            observed_app_odometer_offset_m: None,
            requires_binary_horn: model_id >= 3,
        }
    }

    fn new_with_battery_profile(
        model_id: u16,
        name: &'static str,
        cell_count: u8,
        parallel_cells: u8,
        battery_profile: &'static BatteryVoltageProfile,
    ) -> Self {
        Self {
            model_id,
            name,
            cell_count,
            parallel_cells,
            nominal_capacity_mah: Some(
                u32::from(battery_profile.nominal_capacity_mah) * u32::from(parallel_cells),
            ),
            battery_profile: Some(battery_profile),
            voltage_range_mv: battery_profile_pack_range(battery_profile, cell_count),
            has_pwm_readback: model_id >= 2,
            has_smart_bms: model_id >= 5 || matches!(model_id, 4 | 7 | 42..=44),
            bms_layout: bms_layout_for_geometry(cell_count, parallel_cells),
            observed_app_odometer_offset_m: None,
            requires_binary_horn: model_id >= 3,
        }
    }

    const fn with_observed_app_odometer_offset_m(
        mut self,
        observed_app_odometer_offset_m: i32,
    ) -> Self {
        self.observed_app_odometer_offset_m = Some(observed_app_odometer_offset_m);
        self
    }

    /// Estimates battery percentage from this model's pack voltage.
    #[must_use]
    pub fn estimate_battery_percent(&self, voltage_mv: Voltage) -> u8 {
        if let Some(battery_profile) = self.battery_profile {
            return battery_profile.estimate_percent_from_pack_voltage(voltage_mv, self.cell_count);
        }

        let start = *self.voltage_range_mv.start();
        let end = *self.voltage_range_mv.end();
        let voltage_mv = voltage_mv.as_millivolts();
        if voltage_mv <= start {
            return 0;
        }
        if voltage_mv >= end {
            return 100;
        }

        let numerator = (voltage_mv - start) * 100;
        let denominator = end - start;
        u8::try_from((numerator + denominator / 2) / denominator).unwrap_or(100)
    }
}

const fn bms_layout_for_geometry(
    series_cells: u8,
    parallel_packs: u8,
) -> Option<&'static BmsLayoutSpec> {
    match (series_cells, parallel_packs) {
        (30, 2) => Some(&VETERAN_BMS_30S_2P_LAYOUT),
        (36, 2) => Some(&VETERAN_BMS_36S_2P_LAYOUT),
        (36, 4) => Some(&VETERAN_BMS_36S_4P_LAYOUT),
        (36, 6) => Some(&VETERAN_BMS_36S_6P_LAYOUT),
        (42, 6) => Some(&VETERAN_BMS_42S_6P_LAYOUT),
        _ => None,
    }
}

fn battery_profile_pack_range(
    battery_profile: &'static BatteryVoltageProfile,
    series_cells: u8,
) -> RangeInclusive<i32> {
    let series_cells = i32::from(series_cells);
    let start = battery_profile
        .points
        .first()
        .map_or(0, |point| pack_voltage_mv(point.cell_voltage, series_cells));
    let end = battery_profile.points.last().map_or(start, |point| {
        pack_voltage_mv(point.cell_voltage, series_cells)
    });
    start..=end
}

fn pack_voltage_mv(cell_voltage: cutout_core::CellVoltage, series_cells: i32) -> i32 {
    (cell_voltage.as_microvolts().saturating_mul(series_cells) + 500) / 1_000
}

const fn veteran_charge_mode(raw: VeteranRawChargeMode) -> ChargeMode {
    ChargeMode::from_active(raw.get() != 0)
}

impl VeteranTelemetry {
    /// Decodes the verified fixed telemetry header from a complete Veteran frame.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranTelemetryError::FrameTooShort`] if the frame does not
    /// contain the fixed voltage field.
    pub fn decode(frame: &VeteranFrame) -> Result<Self, VeteranTelemetryError> {
        let cursor = ByteCursor::new(frame.as_slice());
        let raw_version = cursor
            .be_u16(ByteOffset::new(28))
            .ok_or(VeteranTelemetryError::FrameTooShort)?;
        let firmware = VeteranFirmwareVersion::from_raw(raw_version);
        let voltage = Voltage::from_millivolts(
            i32::from(
                cursor
                    .be_u16(ByteOffset::new(4))
                    .ok_or(VeteranTelemetryError::FrameTooShort)?,
            ) * 10,
        );

        let raw_charge_mode = VeteranRawChargeMode::new(
            cursor
                .be_u16(ByteOffset::new(22))
                .ok_or(VeteranTelemetryError::FrameTooShort)?,
        );

        Ok(Self {
            firmware,
            voltage,
            speed: Speed::from_millimetres_per_second(deci_kmh_to_mm_s(i32::from(
                cursor
                    .be_i16(ByteOffset::new(6))
                    .ok_or(VeteranTelemetryError::FrameTooShort)?,
            ))),
            trip_distance: Distance::from_millimetres(
                u64::from(
                    cursor
                        .veteran_swapped_u32(ByteOffset::new(8))
                        .ok_or(VeteranTelemetryError::FrameTooShort)?,
                ) * 1_000,
            ),
            total_distance: Distance::from_millimetres(
                u64::from(
                    cursor
                        .veteran_swapped_u32(ByteOffset::new(12))
                        .ok_or(VeteranTelemetryError::FrameTooShort)?,
                ) * 1_000,
            ),
            phase_current: PhaseCurrent::from_milliamps(
                i32::from(
                    cursor
                        .be_i16(ByteOffset::new(16))
                        .ok_or(VeteranTelemetryError::FrameTooShort)?,
                ) * 100,
            ),
            mosfet_temperature: Temperature::from_millicelsius(
                i32::from(
                    cursor
                        .be_i16(ByteOffset::new(18))
                        .ok_or(VeteranTelemetryError::FrameTooShort)?,
                ) * 10,
            ),
            auto_shutdown_time_remaining: Duration::from_seconds(u64::from(
                cursor
                    .be_u16(ByteOffset::new(20))
                    .ok_or(VeteranTelemetryError::FrameTooShort)?,
            )),
            raw_charge_mode,
            charge_mode: veteran_charge_mode(raw_charge_mode),
            speed_alert: Speed::from_millimetres_per_second(deci_kmh_to_mm_s(i32::from(
                cursor
                    .be_u16(ByteOffset::new(24))
                    .ok_or(VeteranTelemetryError::FrameTooShort)?,
            ))),
            speed_tiltback: Speed::from_millimetres_per_second(deci_kmh_to_mm_s(i32::from(
                cursor
                    .be_u16(ByteOffset::new(26))
                    .ok_or(VeteranTelemetryError::FrameTooShort)?,
            ))),
            pedals_mode: cursor
                .be_u16(ByteOffset::new(30))
                .ok_or(VeteranTelemetryError::FrameTooShort)?,
            pitch: Angle::from_millidegrees(
                i32::from(
                    cursor
                        .be_i16(ByteOffset::new(32))
                        .ok_or(VeteranTelemetryError::FrameTooShort)?,
                ) * 10,
            ),
            hardware_pwm: DutyCycle::from_permille(veteran_pwm_permille(
                cursor
                    .be_u16(ByteOffset::new(34))
                    .ok_or(VeteranTelemetryError::FrameTooShort)?,
            )),
            battery_percent_estimated: Percent::from_percent(estimate_veteran_battery_percent(
                firmware.model_id,
                voltage,
            )),
        })
    }

    /// Converts decoded telemetry into the transport-independent telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        let power_mw = Power::from_voltage_current(self.voltage, self.phase_current);
        TelemetryDelta {
            speed_mm_s: Some(Measured::reported(Speed::from_millimetres_per_second(
                self.speed.as_millimetres_per_second(),
            ))),
            voltage_mv: Some(Measured::reported(self.voltage)),
            motor_current_ma: Some(Measured::reported(self.phase_current)),
            power_mw: Some(Measured::calculated(power_mw)),
            controller_temperature_mc: Some(Measured::reported(self.mosfet_temperature)),
            pwm_permille: Some(Measured::reported(self.hardware_pwm)),
            distance_mm: Some(Measured::reported(Distance::from_millimetres(
                self.total_distance.as_millimetres(),
            ))),
            pitch_mdeg: Some(Measured::reported(self.pitch)),
            battery_percent_estimated: Some(Measured::estimated(self.battery_percent_estimated)),
            ..TelemetryDelta::empty(at_ms)
        }
    }

    /// Converts decoded firmware fields into a generic read-only response.
    #[must_use]
    pub fn to_firmware_response(self) -> ReadOnlyResponse {
        ReadOnlyResponse::Firmware(FirmwareInfo {
            protocol_version: None,
            firmware_major: Some(Measured::reported(self.firmware.model_id)),
            firmware_minor: Some(Measured::reported(self.firmware.minor)),
            firmware_patch: Some(Measured::reported(self.firmware.revision)),
            build_id: Some(RawFieldValue::new(
                VETERAN_FIELD_FIRMWARE_VERSION,
                i64::from(self.firmware.raw_version),
            )),
        })
    }

    /// Converts decoded fixed-header settings fields into generic readback slots.
    #[must_use]
    pub fn to_settings_responses(self) -> [ReadOnlyResponse; 2] {
        [
            ReadOnlyResponse::Settings(SettingsReadback {
                entries: [
                    Some(settings_entry(
                        VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS,
                        i64::try_from(self.auto_shutdown_time_remaining.as_seconds())
                            .unwrap_or(i64::MAX),
                    )),
                    Some(settings_entry(
                        VETERAN_FIELD_CHARGE_MODE,
                        i64::from(self.raw_charge_mode.get()),
                    )),
                    Some(settings_entry(
                        VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
                        i64::from(speed_to_deci_kmh(
                            self.speed_alert.as_millimetres_per_second(),
                        )),
                    )),
                    Some(settings_entry(
                        VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                        i64::from(speed_to_deci_kmh(
                            self.speed_tiltback.as_millimetres_per_second(),
                        )),
                    )),
                ],
            }),
            ReadOnlyResponse::Settings(SettingsReadback {
                entries: [
                    Some(settings_entry(
                        VETERAN_FIELD_PEDALS_MODE,
                        i64::from(self.pedals_mode),
                    )),
                    None,
                    None,
                    None,
                ],
            }),
        ]
    }
}

/// Veteran fixed-header raw field id for firmware version.
pub const VETERAN_FIELD_FIRMWARE_VERSION: u16 = 0x001c;

/// Veteran fixed-header raw field id for auto shutdown time remaining seconds.
pub const VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS: u16 = 0x0014;

/// Veteran fixed-header raw field id for charge mode.
pub const VETERAN_FIELD_CHARGE_MODE: u16 = 0x0016;

/// Veteran fixed-header raw field id for speed alert threshold.
pub const VETERAN_FIELD_SPEED_ALERT_DECI_KMH: u16 = 0x0018;

/// Veteran fixed-header raw field id for speed tiltback threshold.
pub const VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH: u16 = 0x001a;

/// Veteran fixed-header raw field id for pedals mode.
pub const VETERAN_FIELD_PEDALS_MODE: u16 = 0x001e;

const fn settings_entry(id: u16, value: i64) -> SettingsEntry {
    SettingsEntry {
        field: RawFieldValue::new(id, value),
        source: ValueSource::Reported,
        quality: ValueQuality::Known,
        verification: VerificationStatus::HardwareVerified,
    }
}

/// Veteran telemetry decode failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VeteranTelemetryError {
    /// Frame ended before the decoded field.
    #[error("Veteran telemetry frame too short")]
    FrameTooShort,
}

/// Firmware version fields embedded in a Veteran telemetry frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranFirmwareVersion {
    /// Raw firmware version word.
    pub raw_version: u16,

    /// Veteran model id extracted from the version word.
    pub model_id: u16,

    /// Minor version digit extracted from the version word.
    pub minor: u16,

    /// Revision number extracted from the version word.
    pub revision: u16,
}

impl VeteranFirmwareVersion {
    #[must_use]
    const fn from_raw(raw_version: u16) -> Self {
        Self {
            raw_version,
            model_id: raw_version / 1_000,
            minor: (raw_version % 1_000) / 100,
            revision: raw_version % 100,
        }
    }
}

/// Estimates Aero battery percent from its model's Samsung 50S battery profile.
#[must_use]
pub fn estimate_nosfet_aero_battery_percent(voltage_mv: Voltage) -> u8 {
    estimate_veteran_battery_percent(43, voltage_mv)
}

/// Estimates Veteran battery percent using the known model profile when possible.
#[must_use]
pub fn estimate_veteran_battery_percent(model_id: u16, voltage_mv: Voltage) -> u8 {
    VeteranModelProfile::from_model_id(model_id)
        .map_or(0, |profile| profile.estimate_battery_percent(voltage_mv))
}

fn deci_kmh_to_mm_s(value: i32) -> i32 {
    value * 250 / 9
}

fn speed_to_deci_kmh(value: i32) -> i32 {
    let numerator = i64::from(value) * 36;
    let rounded = if numerator >= 0 {
        (numerator + 500) / 1_000
    } else {
        (numerator - 500) / 1_000
    };
    #[allow(clippy::cast_possible_truncation)]
    {
        rounded as i32
    }
}

fn veteran_pwm_permille(raw_pwm: u16) -> i16 {
    let centered = i32::from(raw_pwm) - 0x8000;
    let permille = centered * 1_000 / 0x8000;
    #[allow(clippy::cast_possible_truncation)]
    {
        permille as i16
    }
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

    fn live_aero_2026_06_22_frame() -> VeteranFrame {
        VeteranFrame::try_from_slice(&hex_literal::hex!(
            "dc5a5c532a09000000170000ab6c001700000be6\
             045d00000226021ca8f607801b23000080c80000\
             8080808080800100000080801e0e050e020e020e\
             020e020e020e070e030e030e090e060e020e060e\
             050e04f5d81527"
        ))
        .expect("fixture frame is valid")
    }

    fn synthetic_short_frame(model_id: u16, voltage_mv: i32) -> VeteranFrame {
        let mut bytes = [0_u8; 42];
        bytes[0..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        let voltage_centivolts =
            u16::try_from(voltage_mv / 10).expect("synthetic voltage fits u16");
        bytes[4..6].copy_from_slice(&voltage_centivolts.to_be_bytes());
        let raw_version = model_id * 1_000;
        bytes[28..30].copy_from_slice(&raw_version.to_be_bytes());

        VeteranFrame::try_from_slice(&bytes).expect("synthetic short frame is valid")
    }

    #[test]
    fn veteran_telemetry_decodes_live_aero_voltage() {
        let telemetry = VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");

        assert_eq!(telemetry.voltage.as_millivolts(), 108_760);
    }

    #[test]
    fn veteran_telemetry_decodes_live_aero_fixed_header() {
        let telemetry = VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");

        assert_eq!(
            telemetry,
            VeteranTelemetry {
                firmware: VeteranFirmwareVersion {
                    raw_version: 43_254,
                    model_id: 43,
                    minor: 2,
                    revision: 54,
                },
                voltage: Voltage::from_millivolts(108_760),
                speed: Speed::from_millimetres_per_second(0),
                trip_distance: Distance::from_millimetres(0),
                total_distance: Distance::from_millimetres(1_551_169_000),
                phase_current: PhaseCurrent::from_milliamps(0),
                mosfet_temperature: Temperature::from_millicelsius(33_270),
                auto_shutdown_time_remaining: Duration::from_seconds(0),
                raw_charge_mode: VeteranRawChargeMode::new(0),
                charge_mode: ChargeMode::NotCharging,
                speed_alert: Speed::from_millimetres_per_second(15_277),
                speed_tiltback: Speed::from_millimetres_per_second(15_000),
                pedals_mode: 1_920,
                pitch: Angle::from_millidegrees(69_060),
                hardware_pwm: DutyCycle::from_permille(-1_000),
                battery_percent_estimated: Percent::from_percent(47),
            }
        );
    }

    #[test]
    fn veteran_telemetry_estimates_live_aero_battery_percent() {
        let telemetry = VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");

        assert_eq!(
            telemetry.battery_percent_estimated,
            Percent::from_percent(47)
        );
    }

    #[test]
    fn veteran_telemetry_decodes_charge_mode_from_raw_field() {
        let mut bytes = [0_u8; 42];
        bytes[0..4].copy_from_slice(&[0xdc, 0x5a, 0x5c, 38]);
        bytes[22..24].copy_from_slice(&1_u16.to_be_bytes());
        let frame = VeteranFrame::try_from_slice(&bytes).expect("synthetic frame is valid");

        let telemetry = VeteranTelemetry::decode(&frame).expect("telemetry decodes");

        assert_eq!(telemetry.raw_charge_mode, VeteranRawChargeMode::new(1));
        assert_eq!(telemetry.charge_mode, ChargeMode::Charging);
    }

    #[test]
    fn veteran_telemetry_decodes_second_live_aero_capture() {
        let telemetry =
            VeteranTelemetry::decode(&live_aero_2026_06_22_frame()).expect("telemetry decodes");

        assert_eq!(telemetry.voltage.as_millivolts(), 107_610);
        assert_eq!(
            telemetry.battery_percent_estimated,
            Percent::from_percent(42)
        );
        assert_eq!(
            telemetry.auto_shutdown_time_remaining,
            Duration::from_seconds(1_117)
        );
        assert_eq!(telemetry.firmware.model_id, 43);
        assert_eq!(telemetry.firmware.minor, 2);
        assert_eq!(telemetry.firmware.revision, 54);
    }

    #[test]
    fn aero_battery_percent_estimate_clamps_to_pack_range() {
        assert_eq!(
            estimate_nosfet_aero_battery_percent(Voltage::from_millivolts(
                NOSFET_AERO_MIN_VOLTAGE_MV - 1,
            )),
            0
        );
        assert_eq!(
            estimate_nosfet_aero_battery_percent(Voltage::from_millivolts(
                NOSFET_AERO_MAX_VOLTAGE_MV + 1,
            )),
            100
        );
    }

    #[test]
    fn veteran_model_profile_maps_known_model_ids() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let oryx = VeteranModelProfile::from_model_id(8).expect("Oryx profile is known");
        let sherman = VeteranModelProfile::from_model_id(0).expect("Sherman profile is known");

        assert_eq!(aero.name, "NOSFET Aero");
        assert_eq!(aero.cell_count, 30);
        assert_eq!(aero.parallel_cells, 2);
        assert_eq!(aero.nominal_capacity_mah, Some(10_000));
        assert_eq!(
            aero.battery_profile.map(|profile| profile.cell_model),
            Some("Samsung 50S")
        );
        assert_eq!(aero.voltage_range_mv, 91_000..=126_000);
        assert!(aero.has_pwm_readback);
        assert!(aero.requires_binary_horn);
        assert_eq!(aero.observed_app_odometer_offset_m, Some(805));

        assert_eq!(oryx.name, "Veteran Oryx");
        assert_eq!(oryx.cell_count, 42);
        assert_eq!(oryx.parallel_cells, 6);
        assert_eq!(oryx.nominal_capacity_mah, Some(30_000));
        assert_eq!(
            oryx.battery_profile.map(|profile| profile.cell_model),
            Some("Samsung 50S")
        );
        assert_eq!(oryx.voltage_range_mv, 127_400..=176_400);
        assert!(oryx.has_smart_bms);

        assert_eq!(sherman.cell_count, 24);
        assert_eq!(sherman.parallel_cells, 1);
        assert_eq!(sherman.nominal_capacity_mah, None);
        assert_eq!(sherman.battery_profile, None);
        assert_eq!(sherman.voltage_range_mv, 79_350..=98_700);
        assert!(!sherman.has_pwm_readback);
        assert!(!sherman.requires_binary_horn);
        assert_eq!(sherman.observed_app_odometer_offset_m, None);
    }

    #[test]
    fn aero_model_profile_records_observed_app_odometer_offset_without_parser_correction() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let telemetry =
            VeteranTelemetry::decode(&live_aero_2026_06_22_frame()).expect("telemetry decodes");
        let delta = telemetry.to_delta(42);

        assert_eq!(aero.observed_app_odometer_offset_m, Some(805));
        assert_eq!(
            delta.distance_mm.map(|distance| distance.value),
            Some(telemetry.total_distance)
        );
    }

    #[test]
    fn aero_model_profile_derives_pack_range_from_samsung_50s_cell_curve() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let profile = aero
            .battery_profile
            .expect("Aero has a cell battery profile");
        let first = profile.points.first().expect("profile has low point");
        let last = profile.points.last().expect("profile has high point");

        assert_eq!(
            aero.voltage_range_mv,
            pack_voltage_mv(first.cell_voltage, i32::from(aero.cell_count))
                ..=pack_voltage_mv(last.cell_voltage, i32::from(aero.cell_count))
        );
        assert_eq!(aero.voltage_range_mv, 91_000..=126_000);
    }

    #[test]
    fn aero_model_profile_points_at_samsung_50s_profile() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let profile = aero
            .battery_profile
            .expect("Aero has a cell battery profile");

        assert_eq!(profile, &SAMSUNG_50S_PROFILE);
    }

    #[test]
    fn aero_model_profile_keeps_pack_geometry_separate_from_cell_profile() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let profile = aero
            .battery_profile
            .expect("Aero has a cell battery profile");

        assert_eq!(aero.cell_count, 30);
        assert_eq!(aero.parallel_cells, 2);
        assert_eq!(profile.cell_model, "Samsung 50S");
        assert_eq!(
            profile
                .points
                .first()
                .expect("low point")
                .cell_voltage
                .as_microvolts(),
            3_033_333
        );
        assert_eq!(
            profile
                .points
                .last()
                .expect("high point")
                .cell_voltage
                .as_microvolts(),
            4_200_000
        );
    }

    #[test]
    fn aero_model_profile_estimates_sticker_points_by_scaling_single_cells() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
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
                aero.estimate_battery_percent(Voltage::from_millivolts(pack_mv)),
                percent
            );
        }
    }

    #[test]
    fn aero_parallel_count_does_not_change_voltage_percentage_estimation() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let profile = aero
            .battery_profile
            .expect("Aero has a cell battery profile");

        assert_eq!(
            aero.estimate_battery_percent(Voltage::from_millivolts(107_950)),
            profile.estimate_percent_from_pack_voltage(
                Voltage::from_millivolts(107_950),
                aero.cell_count,
            )
        );
    }

    #[test]
    fn aero_model_profile_uses_samsung_50s_curve_for_battery_percent() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");

        assert_eq!(
            aero.estimate_battery_percent(Voltage::from_millivolts(91_000)),
            0
        );
        assert_eq!(
            aero.estimate_battery_percent(Voltage::from_millivolts(107_950)),
            44
        );
        assert_eq!(
            aero.estimate_battery_percent(Voltage::from_millivolts(108_760)),
            47
        );
        assert_eq!(
            aero.estimate_battery_percent(Voltage::from_millivolts(126_000)),
            100
        );
    }

    #[test]
    fn known_samsung_50s_veteran_models_share_cell_curve_with_model_geometry() {
        let models = [
            (4, "Veteran Patton", 30, 2, Some(10_000), 91_000..=126_000),
            (5, "Veteran Lynx", 36, 4, Some(20_000), 109_200..=151_200),
            (
                6,
                "Veteran Sherman L",
                36,
                6,
                Some(30_000),
                109_200..=151_200,
            ),
            (7, "Veteran Patton S", 30, 2, Some(10_000), 91_000..=126_000),
            (8, "Veteran Oryx", 42, 6, Some(30_000), 127_400..=176_400),
            (9, "Veteran Lynx S", 36, 4, Some(20_000), 109_200..=151_200),
            (42, "NOSFET Apex", 36, 4, Some(20_000), 109_200..=151_200),
            (43, "NOSFET Aero", 30, 2, Some(10_000), 91_000..=126_000),
            (44, "NOSFET Aeon", 36, 2, Some(10_000), 109_200..=151_200),
        ];

        for (model_id, name, cell_count, parallel_cells, nominal_capacity_mah, voltage_range_mv) in
            models
        {
            let profile =
                VeteranModelProfile::from_model_id(model_id).expect("known profile exists");

            assert_eq!(profile.name, name);
            assert_eq!(profile.cell_count, cell_count);
            assert_eq!(profile.parallel_cells, parallel_cells);
            assert_eq!(profile.nominal_capacity_mah, nominal_capacity_mah);
            assert_eq!(profile.battery_profile, Some(&SAMSUNG_50S_PROFILE));
            assert_eq!(profile.voltage_range_mv, voltage_range_mv);
        }
    }

    #[test]
    fn known_veteran_smart_bms_models_expose_static_bms_layouts() {
        let models = [
            (4, "Veteran Patton", 30, 2),
            (5, "Veteran Lynx", 36, 4),
            (6, "Veteran Sherman L", 36, 6),
            (7, "Veteran Patton S", 30, 2),
            (8, "Veteran Oryx", 42, 6),
            (9, "Veteran Lynx S", 36, 4),
            (42, "NOSFET Apex", 36, 4),
            (43, "NOSFET Aero", 30, 2),
            (44, "NOSFET Aeon", 36, 2),
        ];

        for (model_id, name, series_cells, parallel_packs) in models {
            let profile =
                VeteranModelProfile::from_model_id(model_id).expect("known profile exists");
            let layout = profile.bms_layout.expect("smart-BMS layout is known");

            assert_eq!(profile.name, name);
            assert_eq!(layout.series_cells, PackSeriesCells::new(series_cells));
            assert_eq!(layout.parallel_packs, BmsParallelPacks::new(parallel_packs));
            assert_eq!(layout.cell_values_per_page, BmsCellValuesPerPage::new(15));
            assert_eq!(
                layout.temperature_values_per_page,
                BmsTemperatureValuesPerPage::new(6)
            );
            assert_eq!(layout.verification, VerificationStatus::Inferred);
        }
    }

    #[test]
    fn aero_bms_layout_preserves_documented_selector_map() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let layout = aero.bms_layout.expect("Aero smart-BMS layout is known");

        assert_eq!(layout.selectors, &VETERAN_BMS_SELECTORS);
        assert_eq!(
            layout.selectors[0].kind,
            cutout_core::BatteryPageKind::Metadata
        );
        assert_eq!(
            layout.selectors[1].kind,
            cutout_core::BatteryPageKind::CellVoltage
        );
        assert_eq!(
            layout.selectors[3].kind,
            cutout_core::BatteryPageKind::Temperature
        );
        assert_eq!(layout.selectors[8].kind, cutout_core::BatteryPageKind::Raw);
        assert_eq!(
            layout.selectors[3].verification,
            VerificationStatus::SourceVerified
        );
    }

    #[test]
    fn samsung_50s_veteran_models_estimate_equivalent_pack_voltages_equally() {
        let aero = VeteranModelProfile::from_model_id(43).expect("Aero profile is known");
        let lynx = VeteranModelProfile::from_model_id(5).expect("Lynx profile is known");
        let oryx = VeteranModelProfile::from_model_id(8).expect("Oryx profile is known");

        assert_eq!(
            aero.estimate_battery_percent(Voltage::from_millivolts(107_950)),
            44
        );
        assert_eq!(
            lynx.estimate_battery_percent(Voltage::from_millivolts(129_540)),
            44
        );
        assert_eq!(
            oryx.estimate_battery_percent(Voltage::from_millivolts(151_130)),
            44
        );
    }

    #[test]
    fn models_without_cell_profile_evidence_keep_linear_pack_estimation() {
        for model_id in [0, 1, 2, 3] {
            let profile =
                VeteranModelProfile::from_model_id(model_id).expect("known profile exists");

            assert_eq!(profile.battery_profile, None);
            assert_eq!(profile.parallel_cells, 1);
            assert_eq!(profile.nominal_capacity_mah, None);
        }
    }

    #[test]
    fn veteran_model_profile_returns_none_for_unknown_model_ids() {
        assert_eq!(VeteranModelProfile::from_model_id(99), None);
    }

    #[test]
    fn veteran_model_profile_estimates_battery_percent_from_profile_range() {
        let lynx = VeteranModelProfile::from_model_id(5).expect("Lynx profile is known");

        assert_eq!(
            lynx.estimate_battery_percent(Voltage::from_millivolts(109_200)),
            0
        );
        assert_eq!(
            lynx.estimate_battery_percent(Voltage::from_millivolts(151_200)),
            100
        );
        assert_eq!(
            lynx.estimate_battery_percent(Voltage::from_millivolts(129_540)),
            44
        );
    }

    #[test]
    fn veteran_battery_estimation_uses_model_specific_strategy() {
        assert_eq!(
            estimate_veteran_battery_percent(43, Voltage::from_millivolts(107_950)),
            44
        );
        assert_eq!(
            estimate_veteran_battery_percent(5, Voltage::from_millivolts(129_540)),
            44
        );
        assert_eq!(
            estimate_veteran_battery_percent(99, Voltage::from_millivolts(133_535)),
            0
        );
    }

    #[test]
    fn aero_battery_estimation_delegates_to_model_profile() {
        assert_eq!(
            estimate_nosfet_aero_battery_percent(Voltage::from_millivolts(107_950)),
            estimate_veteran_battery_percent(43, Voltage::from_millivolts(107_950))
        );
    }

    #[test]
    fn veteran_telemetry_uses_model_profile_for_battery_percent() {
        let telemetry = VeteranTelemetry::decode(&synthetic_short_frame(5, 129_540))
            .expect("synthetic Lynx frame decodes");

        assert_eq!(telemetry.firmware.model_id, 5);
        assert_eq!(
            telemetry.battery_percent_estimated,
            Percent::from_percent(44)
        );
    }

    #[test]
    fn veteran_telemetry_maps_voltage_and_estimated_percent_to_delta() {
        let delta = VeteranTelemetry::decode(&live_aero_frame())
            .expect("telemetry decodes")
            .to_delta(42);

        assert_eq!(delta.at_ms, 42);
        assert_eq!(
            delta.speed_mm_s,
            Some(Measured::reported(Speed::from_millimetres_per_second(0)))
        );
        assert_eq!(
            delta.voltage_mv,
            Some(Measured::reported(Voltage::from_millivolts(108_760)))
        );
        assert_eq!(
            delta.motor_current_ma,
            Some(Measured::reported(
                cutout_core::BatteryCurrent::from_milliamps(0)
            ))
        );
        assert_eq!(
            delta.controller_temperature_mc,
            Some(Measured::reported(Temperature::from_millicelsius(33_270)))
        );
        assert_eq!(
            delta.power_mw,
            Some(Measured::calculated(Power::from_milliwatts(0)))
        );
        assert_eq!(
            delta.pwm_permille,
            Some(Measured::reported(DutyCycle::from_permille(-1_000)))
        );
        assert_eq!(
            delta.distance_mm,
            Some(Measured::reported(Distance::from_millimetres(
                1_551_169_000
            )))
        );
        assert_eq!(
            delta.pitch_mdeg,
            Some(Measured::reported(Angle::from_millidegrees(69_060)))
        );
        assert_eq!(
            delta.battery_percent_estimated,
            Some(Measured::estimated(Percent::from_percent(47)))
        );
    }

    #[test]
    fn veteran_telemetry_maps_firmware_to_read_only_response() {
        let response = VeteranTelemetry::decode(&live_aero_frame())
            .expect("telemetry decodes")
            .to_firmware_response();

        let ReadOnlyResponse::Firmware(firmware) = response else {
            panic!("expected firmware response");
        };
        assert_eq!(firmware.firmware_major, Some(Measured::reported(43)));
        assert_eq!(firmware.firmware_minor, Some(Measured::reported(2)));
        assert_eq!(firmware.firmware_patch, Some(Measured::reported(54)));
        assert_eq!(
            firmware.build_id,
            Some(RawFieldValue::new(VETERAN_FIELD_FIRMWARE_VERSION, 43_254))
        );
    }

    #[test]
    fn veteran_telemetry_maps_fixed_header_settings_to_read_only_response() {
        let telemetry = VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");
        let responses = telemetry.to_settings_responses();

        let present: Vec<_> = responses
            .into_iter()
            .flat_map(|response| match response {
                ReadOnlyResponse::Settings(settings) => settings.entries,
                _ => [None, None, None, None],
            })
            .flatten()
            .map(|entry| entry.field)
            .collect();

        assert_eq!(
            present,
            vec![
                RawFieldValue::new(VETERAN_FIELD_AUTO_SHUTDOWN_TIME_REMAINING_SECONDS, 0),
                RawFieldValue::new(VETERAN_FIELD_CHARGE_MODE, 0),
                RawFieldValue::new(VETERAN_FIELD_SPEED_ALERT_DECI_KMH, 550),
                RawFieldValue::new(VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, 540),
                RawFieldValue::new(VETERAN_FIELD_PEDALS_MODE, 1_920),
            ]
        );

        assert_eq!(telemetry.speed_alert.as_millimetres_per_second(), 15_277);
        assert_eq!(telemetry.speed_tiltback.as_millimetres_per_second(), 15_000);
    }

    #[test]
    fn veteran_telemetry_delta_scales_nonzero_speed_and_current() {
        let mut telemetry =
            VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");
        telemetry.speed = Speed::from_millimetres_per_second(deci_kmh_to_mm_s(36));
        telemetry.phase_current = PhaseCurrent::from_milliamps(-1_700);

        let delta = telemetry.to_delta(42);

        assert_eq!(
            delta.speed_mm_s,
            Some(Measured::reported(Speed::from_millimetres_per_second(
                1_000
            )))
        );
        assert_eq!(
            delta.motor_current_ma,
            Some(Measured::reported(
                cutout_core::BatteryCurrent::from_milliamps(-1_700)
            ))
        );
        assert_eq!(
            delta.power_mw,
            Some(Measured::calculated(Power::from_milliwatts(-184_892)))
        );
    }

    #[test]
    fn veteran_swapped_u32_uses_veteran_byte_order_for_all_bytes() {
        let cursor = ByteCursor::new(&[0x12, 0x34, 0x56, 0x78]);

        assert_eq!(
            cursor.veteran_swapped_u32(ByteOffset::new(0)),
            Some(0x5678_1234)
        );
    }

    #[test]
    fn veteran_speed_conversion_scales_nonzero_deci_kmh() {
        assert_eq!(deci_kmh_to_mm_s(36), 1_000);
    }

    #[test]
    fn veteran_speed_conversion_round_trips_deci_kmh_thresholds() {
        assert_eq!(speed_to_deci_kmh(15_277), 550);
        assert_eq!(speed_to_deci_kmh(15_000), 540);
    }

    #[test]
    fn veteran_power_conversion_uses_typed_phase_current() {
        let current = PhaseCurrent::from_milliamps(-1_700);

        assert_eq!(current.as_milliamps(), -1_700);
        assert_eq!(
            Power::from_voltage_current(Voltage::from_millivolts(108_760), current).as_milliwatts(),
            -184_892
        );
    }

    #[test]
    fn veteran_pwm_conversion_maps_raw_bounds_and_midpoint() {
        assert_eq!(veteran_pwm_permille(0), -1_000);
        assert_eq!(veteran_pwm_permille(0x8000), 0);
        assert_eq!(veteran_pwm_permille(u16::MAX), 999);
    }
}
