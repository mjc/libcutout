//! Checked local VESC drivetrain configuration, independent of controller writes.

use crate::Distance;

/// Number of motor pole pairs in electrical-to-mechanical RPM conversion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MotorPolePairs(u8);

impl MotorPolePairs {
    /// Creates a motor pole-pair count.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Converts an even, nonzero motor pole count to pole pairs.
    #[must_use]
    pub const fn from_motor_poles(poles: u8) -> Option<Self> {
        if poles == 0 || !poles.is_multiple_of(2) {
            None
        } else {
            Some(Self(poles / 2))
        }
    }

    /// Returns the motor pole-pair count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Mechanical gear-reduction denominator stored in thousandths.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GearRatioDenominator(u32);

impl GearRatioDenominator {
    /// Creates an exact integer reduction; zero leaves speed unavailable.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self((value as u32) * 1_000)
    }

    /// Creates a positive reduction in thousandths of motor revolutions per wheel revolution.
    #[must_use]
    pub const fn from_milli(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Creates a reduction representable in thousandths by the VESC float.
    ///
    /// A thousandths round trip must reproduce the input float, so ordinary
    /// controller values such as 2.7 are accepted without rounding finer inputs.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_cmp,
        reason = "Range checks precede conversion; float equality checks exact wire representability."
    )]
    pub fn from_ratio(value: f32) -> Option<Self> {
        if !value.is_finite() || value <= 0.0 {
            return None;
        }
        let scaled = (f64::from(value) * 1_000.0).round();
        if !(1.0..=f64::from(u32::MAX)).contains(&scaled) {
            return None;
        }
        let milli = scaled as u32;
        if (f64::from(milli) / 1_000.0) as f32 != value {
            return None;
        }
        Self::from_milli(milli)
    }

    /// Returns motor revolutions per wheel revolution without integer truncation.
    #[must_use]
    pub fn get(self) -> f64 {
        f64::from(self.0) / 1_000.0
    }

    /// Returns the ratio in thousandths.
    #[must_use]
    pub const fn as_milli(self) -> u32 {
        self.0
    }
}

/// VESC speed geometry from motor setup config.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescSpeedGeometry {
    /// Motor pole pairs used to convert electrical RPM to mechanical RPM.
    pub motor_pole_pairs: MotorPolePairs,

    /// Mechanical gear reduction denominator.
    pub gear_ratio_denominator: GearRatioDenominator,

    /// Wheel circumference.
    pub wheel_circumference: Distance,
}

impl VescSpeedGeometry {
    /// Checks complete geometry before it can produce physical speed.
    #[must_use]
    pub const fn new(
        motor_pole_pairs: MotorPolePairs,
        gear_ratio_denominator: GearRatioDenominator,
        wheel_circumference: Distance,
    ) -> Option<Self> {
        if motor_pole_pairs.get() == 0
            || gear_ratio_denominator.as_milli() == 0
            || wheel_circumference.as_millimetres() == 0
            || wheel_circumference.as_millimetres() > u64::MAX / 2
        {
            None
        } else {
            Some(Self {
                motor_pole_pairs,
                gear_ratio_denominator,
                wheel_circumference,
            })
        }
    }
}

/// Field rejected by drivetrain parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum VescDrivetrainError {
    /// Diameter must be positive millimetres with up to three decimal places.
    #[error("invalid wheel diameter")]
    WheelDiameter,
    /// Pole pairs must be a positive u8 integer.
    #[error("invalid motor pole pairs")]
    MotorPolePairs,
    /// Reduction must be positive with up to three decimal places.
    #[error("invalid motor to wheel reduction")]
    Reduction,
}

/// Checked user configuration; stores diameter to preserve editable input precision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescDrivetrain {
    diameter_micrometres: u32,
    geometry: VescSpeedGeometry,
}

impl VescDrivetrain {
    /// Parses the three user fields without admitting nonfinite or rounded ratios.
    ///
    /// # Errors
    /// Returns the first invalid field, including unsupported precision or range.
    pub fn parse(
        diameter_mm: &str,
        pole_pairs: &str,
        reduction: &str,
    ) -> Result<Self, VescDrivetrainError> {
        let diameter = parse_thousandths(diameter_mm).ok_or(VescDrivetrainError::WheelDiameter)?;
        let poles = pole_pairs
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(VescDrivetrainError::MotorPolePairs)?;
        let ratio = parse_thousandths(reduction).ok_or(VescDrivetrainError::Reduction)?;
        Self::from_parts(diameter, poles, ratio)
    }

    /// Restores canonical stored values through the same checked constructor.
    ///
    /// # Errors
    /// Returns the invalid field when values cannot form complete speed geometry.
    pub fn from_parts(
        diameter_micrometres: u32,
        pole_pairs: u8,
        ratio_milli: u32,
    ) -> Result<Self, VescDrivetrainError> {
        if diameter_micrometres == 0 {
            return Err(VescDrivetrainError::WheelDiameter);
        }
        if pole_pairs == 0 {
            return Err(VescDrivetrainError::MotorPolePairs);
        }
        let reduction =
            GearRatioDenominator::from_milli(ratio_milli).ok_or(VescDrivetrainError::Reduction)?;
        // The checked diameter is at most u32 micrometres; the rounded circumference fits u64.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "positive u32 diameter bounds the finite circumference"
        )]
        let circumference =
            (f64::from(diameter_micrometres) / 1_000.0 * core::f64::consts::PI).round() as u64;
        let geometry = VescSpeedGeometry::new(
            MotorPolePairs::new(pole_pairs),
            reduction,
            Distance::from_millimetres(circumference),
        )
        .ok_or(VescDrivetrainError::WheelDiameter)?;
        Ok(Self {
            diameter_micrometres,
            geometry,
        })
    }

    /// Original diameter in micrometres, preserving the editor's thousandth-mm precision.
    #[must_use]
    pub const fn diameter_micrometres(self) -> u32 {
        self.diameter_micrometres
    }
    /// Canonical checked speed geometry.
    #[must_use]
    pub const fn geometry(self) -> VescSpeedGeometry {
        self.geometry
    }
    /// Diameter formatted for the native editor without another conversion.
    #[must_use]
    pub fn diameter_text(self) -> String {
        format_thousandths(self.diameter_micrometres)
    }
    /// Pole pairs formatted for the native editor.
    #[must_use]
    pub fn pole_pairs_text(self) -> String {
        self.geometry.motor_pole_pairs.get().to_string()
    }
    /// Reduction formatted for the native editor.
    #[must_use]
    pub fn reduction_text(self) -> String {
        format_thousandths(self.geometry.gear_ratio_denominator.as_milli())
    }
}

/// Persisted identity of a device's retained user drivetrain configuration.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VescDrivetrainRevision(u64);

impl VescDrivetrainRevision {
    /// Checks the nonzero revision identity supplied by a durable owner.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    /// Returns the persisted revision identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
    /// Advances the revision without wrapping an exhausted identity.
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Self::new(value),
            None => None,
        }
    }
}

/// Evidence behind a saved drivetrain profile; never a controller verification claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VescDrivetrainSource {
    /// An explicit local user configuration, independent of device-reported geometry.
    UserConfigured,
}

/// A checked local drivetrain together with its committed revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VescDrivetrainProfile {
    value: VescDrivetrain,
    revision: VescDrivetrainRevision,
}
impl VescDrivetrainProfile {
    /// Retains the durable owner's revision without promoting user input to device proof.
    #[must_use]
    pub const fn user_configured(value: VescDrivetrain, revision: VescDrivetrainRevision) -> Self {
        Self { value, revision }
    }
    /// Canonical geometry and original editable diameter.
    #[must_use]
    pub const fn value(self) -> VescDrivetrain {
        self.value
    }
    /// Revision committed for this configuration by its durable device owner.
    #[must_use]
    pub const fn revision(self) -> VescDrivetrainRevision {
        self.revision
    }
    /// Source of these saved values, separate from protocol and capability evidence.
    #[must_use]
    pub const fn source(self) -> VescDrivetrainSource {
        VescDrivetrainSource::UserConfigured
    }
}

fn parse_thousandths(text: &str) -> Option<u32> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = fraction.trim_end_matches('0');
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().ok()?
    };
    let value = whole.parse::<u32>().ok()?.checked_mul(1_000)?.checked_add(
        fraction_value.checked_mul(10_u32.pow(u32::try_from(3 - fraction.len()).ok()?))?,
    )?;
    (value > 0).then_some(value)
}

fn format_thousandths(value: u32) -> String {
    let whole = value / 1_000;
    let fraction = value % 1_000;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:03}")
            .trim_end_matches('0')
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_drivetrain_revision_is_nonzero_and_never_wraps() {
        assert_eq!(VescDrivetrainRevision::new(0), None);
        assert_eq!(
            VescDrivetrainRevision::new(1)
                .unwrap()
                .checked_next()
                .unwrap()
                .get(),
            2
        );
        assert_eq!(
            VescDrivetrainRevision::new(u64::MAX)
                .unwrap()
                .checked_next(),
            None
        );
        let value = VescDrivetrain::parse("90", "7", "2.5").unwrap();
        let profile =
            VescDrivetrainProfile::user_configured(value, VescDrivetrainRevision::new(1).unwrap());
        assert_eq!(profile.value(), value);
        assert_eq!(profile.source(), VescDrivetrainSource::UserConfigured);
    }

    #[test]
    fn user_drivetrain_parses_fractional_geometry_without_rounding() {
        let value = VescDrivetrain::parse("90", "7", "2.5").expect("valid draft");
        assert_eq!(value.diameter_micrometres, 90_000);
        assert_eq!(value.geometry.gear_ratio_denominator.as_milli(), 2_500);
        assert_eq!(value.geometry.wheel_circumference.as_millimetres(), 283);
    }

    #[test]
    fn user_drivetrain_rejects_invalid_fields() {
        for value in ["", "0", "-1", "NaN", "inf", "1e3", "0.0001", "4294967.296"] {
            assert_eq!(
                VescDrivetrain::parse(value, "7", "2.5"),
                Err(VescDrivetrainError::WheelDiameter)
            );
        }
        for value in ["", "0", "7.5", "256", "NaN"] {
            assert_eq!(
                VescDrivetrain::parse("90", value, "2.5"),
                Err(VescDrivetrainError::MotorPolePairs)
            );
        }
        for value in ["", "0", "NaN", "inf", "0.0001", "2.5001", "4294967.296"] {
            assert_eq!(
                VescDrivetrain::parse("90", "7", value),
                Err(VescDrivetrainError::Reduction)
            );
        }
    }
}
