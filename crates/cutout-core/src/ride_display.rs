//! Shared ride display quantities. Platforms render the returned number and unit.

/// Tenths of a mile per hour for one millimetre per second.
///
/// `10 * 3_600 / 1_609_344` reduces to `125 / 5_588`. The reduced ratio keeps a
/// value that is exactly halfway on the displayed digit on the away-from-zero side.
const SPEED_TENTHS_NUMERATOR: i128 = 125;
const SPEED_TENTHS_DENOMINATOR: i128 = 5_588;

/// Millimetres in one mile. One fractional digit is `millimetres * 10 / MILES_IN_MILLIMETRES`.
const MILES_IN_MILLIMETRES: i128 = 1_609_344;

/// Milliwatts in one kilowatt. Below this absolute value, power shows two fractional digits.
const KILOWATT_MILLIWATTS: u64 = 1_000_000;

/// Live Activity PWM usage above this permille is critical. Exactly this value stays nominal.
pub const LIVE_ACTIVITY_PWM_CRITICAL_PERMILLE: u16 = 800;

/// Unit selected by the display policy. Platforms may localize the word, not the choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideDisplayUnit {
    /// Statute miles per hour.
    MilesPerHour,
    /// Statute miles.
    Miles,
    /// Kilowatts.
    Kilowatts,
    /// Percent from permille, after truncating division by ten.
    Percent,
}

/// A rounded display quantity. `scaled` is the integer of the last shown digit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RideQuantityDisplay {
    /// Canonical decimal text. The point is `.` and the sign is preserved.
    pub text: String,
    /// Signed integer of the last displayed digit. Speed and distance use tenths.
    pub scaled: i64,
    /// Digits after the decimal point. Zero means `text` is an integer.
    pub fraction_digits: u8,
    /// Unit that `text` and `scaled` already use.
    pub unit: RideDisplayUnit,
}

/// Speed in miles per hour, one fractional digit, from millimetres per second.
#[must_use]
pub fn ride_speed_display(millimetres_per_second: i64) -> RideQuantityDisplay {
    let scaled = fit_i64(round_half_away_from_zero(
        i128::from(millimetres_per_second).saturating_mul(SPEED_TENTHS_NUMERATOR),
        SPEED_TENTHS_DENOMINATOR,
    ));
    quantity(scaled, 1, RideDisplayUnit::MilesPerHour)
}

/// Distance in miles, one fractional digit, from millimetres.
#[must_use]
pub fn ride_distance_display(millimetres: i64) -> RideQuantityDisplay {
    let scaled = fit_i64(round_half_away_from_zero(
        i128::from(millimetres).saturating_mul(10),
        MILES_IN_MILLIMETRES,
    ));
    quantity(scaled, 1, RideDisplayUnit::Miles)
}

/// Power in kilowatts. Two fractional digits below one kilowatt, one digit at or above it.
#[must_use]
pub fn ride_power_display(milliwatts: i64) -> RideQuantityDisplay {
    let fine = milliwatts.unsigned_abs() < KILOWATT_MILLIWATTS;
    let (divisor, places) = if fine {
        (10_000_i128, 2_u8)
    } else {
        (100_000_i128, 1_u8)
    };
    let scaled = fit_i64(round_half_away_from_zero(i128::from(milliwatts), divisor));
    quantity(scaled, places, RideDisplayUnit::Kilowatts)
}

/// Percent label for a permille value. Integer division truncates toward zero.
#[must_use]
pub fn ride_permille_percent_label(permille: i64) -> RideQuantityDisplay {
    let percent = permille / 10;
    RideQuantityDisplay {
        text: percent.to_string(),
        scaled: percent,
        fraction_digits: 0,
        unit: RideDisplayUnit::Percent,
    }
}

/// Duration from whole seconds: `m:ss` under one hour and `h:mm:ss` at or above it.
#[must_use]
pub fn ride_duration_text(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remainder = seconds % 60;
    if hours == 0 {
        format!("{minutes}:{remainder:02}")
    } else {
        format!("{hours}:{minutes:02}:{remainder:02}")
    }
}

/// Live Activity PWM is critical only when usage is strictly above 800 permille.
#[must_use]
pub fn live_activity_pwm_is_critical(usage_permille: u16) -> bool {
    usage_permille > LIVE_ACTIVITY_PWM_CRITICAL_PERMILLE
}

fn quantity(scaled: i64, fraction_digits: u8, unit: RideDisplayUnit) -> RideQuantityDisplay {
    RideQuantityDisplay {
        text: decimal_text(scaled, fraction_digits),
        scaled,
        fraction_digits,
        unit,
    }
}

fn decimal_text(scaled: i64, places: u8) -> String {
    if places == 0 {
        return scaled.to_string();
    }
    let negative = scaled < 0;
    let magnitude = scaled.unsigned_abs();
    let factor = 10_u64.saturating_pow(u32::from(places));
    let whole = magnitude / factor;
    let fraction = magnitude % factor;
    let width = usize::from(places);
    if negative {
        format!("-{whole}.{fraction:0width$}")
    } else {
        format!("{whole}.{fraction:0width$}")
    }
}

fn round_half_away_from_zero(numerator: i128, denominator: i128) -> i128 {
    if denominator <= 0 {
        return 0;
    }
    let negative = numerator < 0;
    let magnitude = numerator.unsigned_abs();
    let divisor = denominator.unsigned_abs();
    let quotient = magnitude / divisor;
    let remainder = magnitude % divisor;
    let rounded = if remainder.saturating_mul(2) >= divisor {
        quotient.saturating_add(1)
    } else {
        quotient
    };
    let Some(positive) = i128::try_from(rounded).ok() else {
        return if negative { i128::MIN } else { i128::MAX };
    };
    if negative {
        positive.checked_neg().unwrap_or(i128::MIN)
    } else {
        positive
    }
}

fn fit_i64(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_policy_formats_speed_with_one_mile_per_hour_digit() {
        let stopped = ride_speed_display(0);
        assert_eq!(stopped.text, "0.0");
        assert_eq!(stopped.scaled, 0);
        assert_eq!(stopped.unit, RideDisplayUnit::MilesPerHour);

        let known = ride_speed_display(1_234);
        assert_eq!(known.text, "2.8");
        assert_eq!(known.scaled, 28);

        let ten = ride_speed_display(4_470);
        assert_eq!(ten.text, "10.0");
        assert_eq!(ten.scaled, 100);

        assert_eq!(ride_speed_display(-1_234).text, "-2.8");

        // 2_794 mm/s is exactly 6.25 mph, so one fractional digit rounds away from zero.
        assert_eq!(ride_speed_display(2_794).text, "6.3");
        assert_eq!(ride_speed_display(2_794).scaled, 63);
        assert_eq!(ride_speed_display(-2_794).text, "-6.3");
        assert_eq!(ride_speed_display(-2_794).scaled, -63);
    }

    #[test]
    fn display_policy_formats_distance_as_miles() {
        assert_eq!(ride_distance_display(0).text, "0.0");
        let mile = ride_distance_display(1_609_344);
        assert_eq!(mile.text, "1.0");
        assert_eq!(mile.scaled, 10);
        assert_eq!(mile.unit, RideDisplayUnit::Miles);
        assert_eq!(ride_distance_display(402_336).text, "0.3");
        assert_eq!(ride_distance_display(-1_609_344).text, "-1.0");
    }

    #[test]
    fn display_policy_formats_duration_power_and_permille() {
        assert_eq!(ride_duration_text(0), "0:00");
        assert_eq!(ride_duration_text(65_000), "1:05");
        assert_eq!(ride_duration_text(3_599_000), "59:59");
        assert_eq!(ride_duration_text(3_600_000), "1:00:00");
        assert_eq!(ride_duration_text(3_661_000), "1:01:01");

        let small = ride_power_display(673_320);
        assert_eq!(small.text, "0.67");
        assert_eq!(small.fraction_digits, 2);
        assert_eq!(small.unit, RideDisplayUnit::Kilowatts);
        assert_eq!(ride_power_display(999_999).text, "1.00");
        let one = ride_power_display(1_000_000);
        assert_eq!(one.text, "1.0");
        assert_eq!(one.fraction_digits, 1);
        assert_eq!(ride_power_display(1_500_000).text, "1.5");
        assert_eq!(ride_power_display(1_250_000).text, "1.3");
        assert_eq!(ride_power_display(-673_320).text, "-0.67");
        assert_eq!(ride_power_display(-1_250_000).text, "-1.3");

        assert_eq!(ride_permille_percent_label(0).text, "0");
        assert_eq!(ride_permille_percent_label(80).text, "8");
        assert_eq!(ride_permille_percent_label(1_000).text, "100");
        assert_eq!(ride_permille_percent_label(9).text, "0");
        assert_eq!(ride_permille_percent_label(-25).text, "-2");
        assert_eq!(
            ride_permille_percent_label(80).unit,
            RideDisplayUnit::Percent
        );
    }

    #[test]
    fn display_policy_pwm_is_critical_only_above_800_permille() {
        assert!(!live_activity_pwm_is_critical(0));
        assert!(!live_activity_pwm_is_critical(
            LIVE_ACTIVITY_PWM_CRITICAL_PERMILLE
        ));
        assert!(live_activity_pwm_is_critical(801));
        assert!(live_activity_pwm_is_critical(1_000));
    }
}
