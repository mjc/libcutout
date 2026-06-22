use thiserror::Error;

use crate::BegodeFrame;

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
}

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

fn mpu6050_temperature_mc(raw_temperature: i16) -> i32 {
    36_530 + (i32::from(raw_temperature) * 1_000) / 340
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
        BegodeExtraTelemetry, BegodeFrame, BegodeLiveATelemetry, BegodeLiveBTelemetry,
        BegodePackVoltageProfile, BegodeTelemetryError, estimate_begode_battery_percent,
    };
    use proptest::prelude::*;

    const LIVE_A: [u8; 24] = hex_literal::hex!("55aa17750538007602eefb64f4941481000900185a5a5a5a");
    const LIVE_B: [u8; 24] = hex_literal::hex!("55aa000000320000000f003200030502000004185a5a5a5a");
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
    }
}
