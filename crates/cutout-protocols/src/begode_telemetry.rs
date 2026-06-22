use core::ops::RangeInclusive;

use cutout_core::{
    DiagnosticDetail, DiagnosticReadback, DiagnosticSeverity, Measured, MonotonicMillis,
    RawFieldValue, ReadOnlyResponse, SettingsEntry, SettingsReadback, TelemetryDelta, ValueQuality,
    ValueSource, VerificationStatus,
};
use thiserror::Error;

use crate::BegodeFrame;

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
    /// 84 V / 20S profile confirmed for Begode Falcon.
    Falcon84V,
}

impl BegodePackVoltageProfile {
    const fn scaler_milli(self) -> i32 {
        match self {
            Self::Falcon84V => 1_250,
        }
    }

    /// Series cell count for this pack profile.
    #[must_use]
    pub const fn series_cells(self) -> u8 {
        match self {
            Self::Falcon84V => 20,
        }
    }

    /// Nominal pack capacity in milliamp-hours, when known.
    #[must_use]
    pub const fn nominal_capacity_mah(self) -> Option<u32> {
        match self {
            Self::Falcon84V => Some(3_750),
        }
    }

    /// Expected pack voltage range in millivolts.
    #[must_use]
    pub fn voltage_range_mv(self) -> RangeInclusive<u32> {
        match self {
            Self::Falcon84V => 60_000..=84_000,
        }
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
        let bytes = frame.as_slice();
        let raw_voltage_centivolts = read_be_u16(bytes, 2);
        Ok(Self {
            raw_voltage_centivolts,
            voltage_mv: scaled_voltage_mv(raw_voltage_centivolts, profile),
            speed_milli_kmh: raw_speed_to_milli_kmh(read_be_i16(bytes, 4)),
            trip_distance_m: read_be_u32(bytes, 6),
            trip_distance_low_m: read_be_u16(bytes, 8),
            phase_current_ma: i32::from(read_be_i16(bytes, 10)) * 10,
            imu_temperature_mc: mpu6050_temperature_mc(read_be_i16(bytes, 12)),
            hardware_pwm_raw: read_be_i16(bytes, 14),
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
        let bytes = frame.as_slice();
        Ok(Self {
            total_distance_m: read_be_u32(bytes, 2),
            settings_bits: read_be_u16(bytes, 6),
            power_off_timer_minutes: read_be_u16(bytes, 8),
            tiltback_speed_kmh: read_be_u16(bytes, 10),
            led_mode: read_u8(bytes, 13),
            alert_flags: read_u8(bytes, 14),
            light_mode: read_u8(bytes, 15) & 0x03,
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
        let bytes = frame.as_slice();
        Ok(Self {
            battery_current_ma: i32::from(read_be_i16(bytes, 2)) * 10,
            motor_temperature_mc: i32::from(read_be_i16(bytes, 6)) * 1_000,
            true_pwm_raw: read_be_i16(bytes, 8),
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
    if actual == expected {
        Ok(())
    } else {
        Err(BegodeTelemetryError::UnexpectedFrameTag { expected, actual })
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

fn read_u8(bytes: &[u8; 24], offset: usize) -> u8 {
    bytes.get(offset).copied().unwrap_or_default()
}

fn read_be_u16(bytes: &[u8; 24], offset: usize) -> u16 {
    match bytes.get(offset..offset + 2) {
        Some([b0, b1]) => u16::from_be_bytes([*b0, *b1]),
        _ => 0,
    }
}

fn read_be_i16(bytes: &[u8; 24], offset: usize) -> i16 {
    match bytes.get(offset..offset + 2) {
        Some([b0, b1]) => i16::from_be_bytes([*b0, *b1]),
        _ => 0,
    }
}

fn read_be_u32(bytes: &[u8; 24], offset: usize) -> u32 {
    match bytes.get(offset..offset + 4) {
        Some([b0, b1, b2, b3]) => u32::from_be_bytes([*b0, *b1, *b2, *b3]),
        _ => 0,
    }
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
    use crate::{
        BEGODE_FIELD_ALERT_FLAGS, BEGODE_FIELD_LED_AND_LIGHT_MODE,
        BEGODE_FIELD_POWER_OFF_TIMER_MINUTES, BEGODE_FIELD_SETTINGS_BITS,
        BEGODE_FIELD_TILTBACK_SPEED_KMH, BegodeExtraTelemetry, BegodeFrame, BegodeLiveATelemetry,
        BegodeLiveBTelemetry, BegodePackVoltageProfile, BegodeTelemetryContext,
        BegodeTelemetryError, BegodeUnitMode, estimate_begode_battery_percent,
    };
    use cutout_core::{
        DiagnosticSeverity, Measured, RawFieldValue, ReadOnlyResponse, TelemetryDelta,
        ValueQuality, ValueSource, VerificationStatus,
    };
    use proptest::prelude::*;

    const LIVE_A: [u8; 24] = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
    const LIVE_B: [u8; 24] = hex_literal::hex!("55aa000000320000000f003200030502000004185a5a5a5a");
    const LIVE_B_IMPERIAL: [u8; 24] =
        hex_literal::hex!("55aa000000320001000f003200030502000004185a5a5a5a");
    const EXTRA: [u8; 24] = hex_literal::hex!("55aaff9c0000002affd8000000000000000007185a5a5a5a");

    #[test]
    fn live_a_decodes_source_backed_primary_fields_for_falcon_84v() {
        let frame = BegodeFrame::try_from_slice(&LIVE_A).expect("fixture frame is valid");
        let telemetry = BegodeLiveATelemetry::decode(&frame, BegodePackVoltageProfile::Falcon84V)
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
        let telemetry = BegodeLiveATelemetry::decode(&frame, BegodePackVoltageProfile::Falcon84V)
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
        let telemetry = BegodeLiveATelemetry::decode(&live_a, BegodePackVoltageProfile::Falcon84V)
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
            BegodeLiveATelemetry::decode(&live_b, BegodePackVoltageProfile::Falcon84V),
            Err(BegodeTelemetryError::UnexpectedFrameTag {
                expected: 0,
                actual: 4
            })
        );
    }

    #[test]
    fn falcon_84v_battery_percent_uses_better_begode_curve() {
        assert_eq!(
            estimate_begode_battery_percent(75_000, BegodePackVoltageProfile::Falcon84V),
            50
        );
    }

    #[test]
    fn falcon_84v_profile_exposes_pack_geometry_and_capacity() {
        let profile = BegodePackVoltageProfile::Falcon84V;

        assert_eq!(profile.series_cells(), 20);
        assert_eq!(profile.voltage_range_mv(), 60_000..=84_000);
        assert_eq!(profile.nominal_capacity_mah(), Some(3_750));
    }

    proptest! {
        #[test]
        fn falcon_battery_percent_is_monotonic(first_mv in 60_000i32..=84_000, second_mv in 60_000i32..=84_000) {
            let low = first_mv.min(second_mv);
            let high = first_mv.max(second_mv);

            prop_assert!(
                estimate_begode_battery_percent(low, BegodePackVoltageProfile::Falcon84V)
                    <= estimate_begode_battery_percent(high, BegodePackVoltageProfile::Falcon84V)
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
