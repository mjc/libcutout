use cutout_core::{
    FirmwareInfo, Measured, MonotonicMillis, RawFieldValue, ReadOnlyResponse, SettingsEntry,
    SettingsReadback, TelemetryDelta, ValueQuality, ValueSource, VerificationStatus,
};
use thiserror::Error;

use crate::VeteranFrame;

/// Capture-backed minimum pack voltage for a NOSFET Aero 30s pack.
pub const NOSFET_AERO_MIN_VOLTAGE_MV: i32 = 99_180;

/// Capture-backed maximum pack voltage for a NOSFET Aero 30s pack.
pub const NOSFET_AERO_MAX_VOLTAGE_MV: i32 = 123_370;

/// Minimal read-only telemetry decoded from a Veteran/LeaperKim/NOSFET frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranTelemetry {
    /// Firmware/model version fields.
    pub firmware: VeteranFirmwareVersion,

    /// Pack voltage in millivolts.
    pub voltage_mv: i32,

    /// Speed in protocol-native deci-km/h.
    pub speed_deci_kmh: i16,

    /// Trip distance in meters.
    pub trip_distance_m: u32,

    /// Total distance in meters.
    pub total_distance_m: u32,

    /// Phase current in protocol-native deci-amps.
    pub phase_current_deci_a: i16,

    /// MOSFET/controller temperature in millicelsius.
    pub mosfet_temperature_mc: i32,

    /// Auto-off setting in seconds.
    pub auto_off_seconds: u16,

    /// Raw charge-mode field.
    pub charge_mode: u16,

    /// Speed alert threshold in protocol-native deci-km/h.
    pub speed_alert_deci_kmh: u16,

    /// Speed tiltback threshold in protocol-native deci-km/h.
    pub speed_tiltback_deci_kmh: u16,

    /// Raw pedals-mode field.
    pub pedals_mode: u16,

    /// Pitch in millidegrees.
    pub pitch_mdeg: i32,

    /// Raw hardware PWM field.
    pub hardware_pwm_raw: u16,

    /// Cutout-estimated battery percentage from capture-backed pack range.
    pub battery_percent_estimated: u8,
}

impl VeteranTelemetry {
    /// Decodes the verified fixed telemetry header from a complete Veteran frame.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranTelemetryError::FrameTooShort`] if the frame does not
    /// contain the fixed voltage field.
    pub fn decode(frame: &VeteranFrame) -> Result<Self, VeteranTelemetryError> {
        let bytes = frame.as_slice();
        let raw_version = read_be_u16(bytes, 28).ok_or(VeteranTelemetryError::FrameTooShort)?;
        let firmware = VeteranFirmwareVersion::from_raw(raw_version);
        let voltage_mv =
            i32::from(read_be_u16(bytes, 4).ok_or(VeteranTelemetryError::FrameTooShort)?) * 10;

        Ok(Self {
            firmware,
            voltage_mv,
            speed_deci_kmh: read_be_i16(bytes, 6).ok_or(VeteranTelemetryError::FrameTooShort)?,
            trip_distance_m: read_veteran_swapped_u32(bytes, 8)
                .ok_or(VeteranTelemetryError::FrameTooShort)?,
            total_distance_m: read_veteran_swapped_u32(bytes, 12)
                .ok_or(VeteranTelemetryError::FrameTooShort)?,
            phase_current_deci_a: read_be_i16(bytes, 16)
                .ok_or(VeteranTelemetryError::FrameTooShort)?,
            mosfet_temperature_mc: i32::from(
                read_be_i16(bytes, 18).ok_or(VeteranTelemetryError::FrameTooShort)?,
            ) * 10,
            auto_off_seconds: read_be_u16(bytes, 20).ok_or(VeteranTelemetryError::FrameTooShort)?,
            charge_mode: read_be_u16(bytes, 22).ok_or(VeteranTelemetryError::FrameTooShort)?,
            speed_alert_deci_kmh: read_be_u16(bytes, 24)
                .ok_or(VeteranTelemetryError::FrameTooShort)?,
            speed_tiltback_deci_kmh: read_be_u16(bytes, 26)
                .ok_or(VeteranTelemetryError::FrameTooShort)?,
            pedals_mode: read_be_u16(bytes, 30).ok_or(VeteranTelemetryError::FrameTooShort)?,
            pitch_mdeg: i32::from(
                read_be_i16(bytes, 32).ok_or(VeteranTelemetryError::FrameTooShort)?,
            ) * 10,
            hardware_pwm_raw: read_be_u16(bytes, 34).ok_or(VeteranTelemetryError::FrameTooShort)?,
            battery_percent_estimated: estimate_nosfet_aero_battery_percent(voltage_mv),
        })
    }

    /// Converts decoded telemetry into the transport-independent telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        let motor_current_ma = i32::from(self.phase_current_deci_a) * 100;
        TelemetryDelta {
            speed_mm_s: Some(Measured::reported(deci_kmh_to_mm_s(self.speed_deci_kmh))),
            voltage_mv: Some(Measured::reported(self.voltage_mv)),
            motor_current_ma: Some(Measured::reported(motor_current_ma)),
            power_mw: Some(Measured::calculated(veteran_power_mw(
                self.voltage_mv,
                motor_current_ma,
            ))),
            controller_temperature_mc: Some(Measured::reported(self.mosfet_temperature_mc)),
            pwm_permille: Some(Measured::reported(veteran_pwm_permille(
                self.hardware_pwm_raw,
            ))),
            distance_mm: Some(Measured::reported(u64::from(self.total_distance_m) * 1_000)),
            pitch_mdeg: Some(Measured::reported(self.pitch_mdeg)),
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
                        VETERAN_FIELD_AUTO_OFF_SECONDS,
                        i64::from(self.auto_off_seconds),
                    )),
                    Some(settings_entry(
                        VETERAN_FIELD_CHARGE_MODE,
                        i64::from(self.charge_mode),
                    )),
                    Some(settings_entry(
                        VETERAN_FIELD_SPEED_ALERT_DECI_KMH,
                        i64::from(self.speed_alert_deci_kmh),
                    )),
                    Some(settings_entry(
                        VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH,
                        i64::from(self.speed_tiltback_deci_kmh),
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

/// Veteran fixed-header raw field id for auto-off seconds.
pub const VETERAN_FIELD_AUTO_OFF_SECONDS: u16 = 0x0014;

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

fn read_be_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    let b0 = *bytes.get(offset)?;
    let b1 = *bytes.get(offset + 1)?;
    Some(i16::from_be_bytes([b0, b1]))
}

fn read_veteran_swapped_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let b0 = u32::from(*bytes.get(offset)?);
    let b1 = u32::from(*bytes.get(offset + 1)?);
    let b2 = u32::from(*bytes.get(offset + 2)?);
    let b3 = u32::from(*bytes.get(offset + 3)?);
    Some((b2 << 24) | (b3 << 16) | (b0 << 8) | b1)
}

fn deci_kmh_to_mm_s(value: i16) -> i32 {
    i32::from(value) * 250 / 9
}

fn veteran_power_mw(voltage_mv: i32, current_ma: i32) -> i64 {
    i64::from(voltage_mv) * i64::from(current_ma) / 1_000
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

    #[test]
    fn veteran_telemetry_decodes_live_aero_voltage() {
        let telemetry = VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");

        assert_eq!(telemetry.voltage_mv, 108_760);
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
                voltage_mv: 108_760,
                speed_deci_kmh: 0,
                trip_distance_m: 0,
                total_distance_m: 1_551_169,
                phase_current_deci_a: 0,
                mosfet_temperature_mc: 33_270,
                auto_off_seconds: 0,
                charge_mode: 0,
                speed_alert_deci_kmh: 550,
                speed_tiltback_deci_kmh: 540,
                pedals_mode: 1_920,
                pitch_mdeg: 69_060,
                hardware_pwm_raw: 0,
                battery_percent_estimated: 39,
            }
        );
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
        assert_eq!(delta.speed_mm_s, Some(Measured::reported(0)));
        assert_eq!(delta.voltage_mv, Some(Measured::reported(108_760)));
        assert_eq!(delta.motor_current_ma, Some(Measured::reported(0)));
        assert_eq!(
            delta.controller_temperature_mc,
            Some(Measured::reported(33_270))
        );
        assert_eq!(delta.power_mw, Some(Measured::calculated(0)));
        assert_eq!(delta.pwm_permille, Some(Measured::reported(-1_000)));
        assert_eq!(delta.distance_mm, Some(Measured::reported(1_551_169_000)));
        assert_eq!(delta.pitch_mdeg, Some(Measured::reported(69_060)));
        assert_eq!(
            delta.battery_percent_estimated,
            Some(Measured::estimated(39))
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
        let responses = VeteranTelemetry::decode(&live_aero_frame())
            .expect("telemetry decodes")
            .to_settings_responses();

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
                RawFieldValue::new(VETERAN_FIELD_AUTO_OFF_SECONDS, 0),
                RawFieldValue::new(VETERAN_FIELD_CHARGE_MODE, 0),
                RawFieldValue::new(VETERAN_FIELD_SPEED_ALERT_DECI_KMH, 550),
                RawFieldValue::new(VETERAN_FIELD_SPEED_TILTBACK_DECI_KMH, 540),
                RawFieldValue::new(VETERAN_FIELD_PEDALS_MODE, 1_920),
            ]
        );
    }

    #[test]
    fn veteran_telemetry_delta_scales_nonzero_speed_and_current() {
        let mut telemetry =
            VeteranTelemetry::decode(&live_aero_frame()).expect("telemetry decodes");
        telemetry.speed_deci_kmh = 36;
        telemetry.phase_current_deci_a = -17;

        let delta = telemetry.to_delta(42);

        assert_eq!(delta.speed_mm_s, Some(Measured::reported(1_000)));
        assert_eq!(delta.motor_current_ma, Some(Measured::reported(-1_700)));
        assert_eq!(delta.power_mw, Some(Measured::calculated(-184_892)));
    }

    #[test]
    fn veteran_swapped_u32_uses_veteran_byte_order_for_all_bytes() {
        assert_eq!(
            read_veteran_swapped_u32(&[0x12, 0x34, 0x56, 0x78], 0),
            Some(0x5678_1234)
        );
    }

    #[test]
    fn veteran_speed_conversion_scales_nonzero_deci_kmh() {
        assert_eq!(deci_kmh_to_mm_s(36), 1_000);
    }

    #[test]
    fn veteran_power_conversion_uses_millivolts_and_milliamps() {
        assert_eq!(veteran_power_mw(108_760, -1_700), -184_892);
    }

    #[test]
    fn veteran_pwm_conversion_maps_raw_bounds_and_midpoint() {
        assert_eq!(veteran_pwm_permille(0), -1_000);
        assert_eq!(veteran_pwm_permille(0x8000), 0);
        assert_eq!(veteran_pwm_permille(u16::MAX), 999);
    }
}
