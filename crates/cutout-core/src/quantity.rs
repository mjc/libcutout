//! Dimension, unit, and typed quantity definitions.

use std::{cmp::Ordering, fmt, marker::PhantomData, ops::RangeInclusive};

/// Marker trait for zero-sized quantity dimensions.
pub trait Dimension: Copy + Eq {}

/// Electrical potential dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricPotential;

impl Dimension for ElectricPotential {}

/// Electrical current dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricCurrent;

impl Dimension for ElectricCurrent {}

/// Electrical power dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricPower;

impl Dimension for ElectricPower {}

/// Electrical energy dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricEnergy;

impl Dimension for ElectricEnergy {}

/// Electrical charge capacity dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricCharge;

impl Dimension for ElectricCharge {}

/// Linear velocity dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Velocity;

impl Dimension for Velocity {}

/// Length dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Length;

impl Dimension for Length {}

/// Information size dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Information;

impl Dimension for Information {}

/// Thermodynamic temperature dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThermodynamicTemperature;

impl Dimension for ThermodynamicTemperature {}

/// Plane angle dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneAngle;

impl Dimension for PlaneAngle {}

/// Rotational speed dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AngularVelocity;

impl Dimension for AngularVelocity {}

/// Rotation-count dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rotation;

impl Dimension for Rotation {}

/// Dimensionless ratio dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ratio;

impl Dimension for Ratio {}

/// Radio signal power dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalPower;

impl Dimension for SignalPower {}

/// Time duration dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Time;

impl Dimension for Time {}

/// Discrete count dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Count;

impl Dimension for Count {}

/// Marker trait for zero-sized quantity units.
pub trait Unit: Copy + Eq {
    /// Dimension measured by this unit.
    type Dimension: Dimension;
}

/// Millivolt storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MilliVolt;

impl Unit for MilliVolt {
    type Dimension = ElectricPotential;
}

/// Centivolt storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CentiVolt;

impl Unit for CentiVolt {
    type Dimension = ElectricPotential;
}

/// Microvolt storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MicroVolt;

impl Unit for MicroVolt {
    type Dimension = ElectricPotential;
}

/// Milliamp storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MilliAmp;

impl Unit for MilliAmp {
    type Dimension = ElectricCurrent;
}

/// Milliwatt storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MilliWatt;

impl Unit for MilliWatt {
    type Dimension = ElectricPower;
}

/// Watt-hour storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WattHour;

impl Unit for WattHour {
    type Dimension = ElectricEnergy;
}

/// Milliamp-hour storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MilliAmpHour;

impl Unit for MilliAmpHour {
    type Dimension = ElectricCharge;
}

/// Millimetres per second storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MillimetrePerSecond;

impl Unit for MillimetrePerSecond {
    type Dimension = Velocity;
}

/// Millimetre storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Millimetre;

impl Unit for Millimetre {
    type Dimension = Length;
}

/// Byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Byte;

impl Unit for Byte {
    type Dimension = Information;
}

/// Parser frame byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserFrameByte;

impl Unit for ParserFrameByte {
    type Dimension = Information;
}

/// Parser buffer byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserBufferByte;

impl Unit for ParserBufferByte {
    type Dimension = Information;
}

/// Notification payload byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationPayloadByte;

impl Unit for NotificationPayloadByte {
    type Dimension = Information;
}

/// Notification chunk byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationChunkByte;

impl Unit for NotificationChunkByte {
    type Dimension = Information;
}

/// Protocol payload body byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadBodyByte;

impl Unit for PayloadBodyByte {
    type Dimension = Information;
}

/// Parser-dropped byte storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserDroppedByte;

impl Unit for ParserDroppedByte {
    type Dimension = Information;
}

/// Millisecond storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Millisecond;

impl Unit for Millisecond {
    type Dimension = Time;
}

/// Millicelsius storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MilliCelsius;

impl Unit for MilliCelsius {
    type Dimension = ThermodynamicTemperature;
}

/// Millidegree storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MilliDegree;

impl Unit for MilliDegree {
    type Dimension = PlaneAngle;
}

/// Electrical revolutions per minute storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricalRevolutionPerMinute;

impl Unit for ElectricalRevolutionPerMinute {
    type Dimension = AngularVelocity;
}

/// Relative tachometer count storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TachometerCountUnit;

impl Unit for TachometerCountUnit {
    type Dimension = Rotation;
}

/// Permille storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Permille;

impl Unit for Permille {
    type Dimension = Ratio;
}

/// Percent storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PercentUnit;

impl Unit for PercentUnit {
    type Dimension = Ratio;
}

/// Decibel-milliwatt storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecibelMilliwatt;

impl Unit for DecibelMilliwatt {
    type Dimension = SignalPower;
}

/// Cell-count storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell;

impl Unit for Cell {
    type Dimension = Count;
}

/// Pack-count storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pack;

impl Unit for Pack {
    type Dimension = Count;
}

/// Parser diagnostic event count storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserDiagnosticEvent;

impl Unit for ParserDiagnosticEvent {
    type Dimension = Count;
}

/// Parser queued-output count storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserQueuedOutput;

impl Unit for ParserQueuedOutput {
    type Dimension = Count;
}

/// Semantic event count storage unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticEvent;

impl Unit for SemanticEvent {
    type Dimension = Count;
}

/// Fixed-point quantity tagged by zero-sized dimension and unit markers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Quantity<D, U, T>
where
    D: Dimension,
    U: Unit<Dimension = D>,
{
    value: T,
    dimension: PhantomData<D>,
    unit: PhantomData<U>,
}

impl<D, U, T> Quantity<D, U, T>
where
    D: Dimension,
    U: Unit<Dimension = D>,
{
    /// Creates a quantity from a value already expressed in its storage unit.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self {
            value,
            dimension: PhantomData,
            unit: PhantomData,
        }
    }

    /// Creates a quantity from a value already expressed in its storage unit.
    #[must_use]
    pub const fn from_unit_value(value: T) -> Self {
        Self::new(value)
    }

    /// Returns the value in this quantity's storage unit.
    #[must_use]
    pub const fn unit_value(self) -> T
    where
        T: Copy,
    {
        self.value
    }

    /// Returns the value in this quantity's storage unit.
    #[must_use]
    pub const fn get(self) -> T
    where
        T: Copy,
    {
        self.unit_value()
    }
}

impl<U> Quantity<Information, U, usize>
where
    U: Unit<Dimension = Information>,
{
    /// Creates an information quantity from bytes.
    #[must_use]
    pub const fn from_bytes(value: usize) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this information quantity in bytes.
    #[must_use]
    pub const fn as_bytes(self) -> usize {
        self.unit_value()
    }

    /// Adds another information quantity, saturating at `usize::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::from_bytes(self.as_bytes().saturating_add(other.as_bytes()))
    }
}

impl<U> Quantity<Information, U, u16>
where
    U: Unit<Dimension = Information>,
{
    /// Creates an information quantity from bytes.
    #[must_use]
    pub const fn from_bytes(value: u16) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this information quantity in bytes.
    #[must_use]
    pub const fn as_bytes(self) -> u16 {
        self.unit_value()
    }

    /// Returns a non-zero chunk length suitable for transport writes.
    #[must_use]
    pub fn chunk_len(self) -> usize {
        usize::from(self.as_bytes()).max(1)
    }
}

impl<U> Quantity<Count, U, usize>
where
    U: Unit<Dimension = Count>,
{
    /// Creates a count quantity from event count.
    #[must_use]
    pub const fn from_events(value: usize) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this count quantity as event count.
    #[must_use]
    pub const fn as_events(self) -> usize {
        self.unit_value()
    }

    /// Returns true when this count has no observed events.
    #[must_use]
    pub const fn has_no_events(self) -> bool {
        self.as_events() == 0
    }

    /// Returns true when this count has at least one observed event.
    #[must_use]
    pub const fn has_events(self) -> bool {
        !self.has_no_events()
    }

    /// Adds one counted event, saturating at `usize::MAX`.
    #[must_use]
    pub const fn next(self) -> Self {
        Self::from_events(self.as_events().saturating_add(1))
    }

    /// Adds one counted event, saturating at `usize::MAX`.
    #[must_use]
    pub const fn increment(self) -> Self {
        self.next()
    }

    /// Adds another count quantity, saturating at `usize::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::from_events(self.as_events().saturating_add(other.as_events()))
    }
}

impl<D, U, T> Default for Quantity<D, U, T>
where
    D: Dimension,
    U: Unit<Dimension = D>,
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<D, U, T> fmt::Display for Quantity<D, U, T>
where
    D: Dimension,
    U: Unit<Dimension = D>,
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<D, U, T> Ord for Quantity<D, U, T>
where
    D: Dimension,
    U: Unit<Dimension = D>,
    T: Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<D, U, T> PartialOrd for Quantity<D, U, T>
where
    D: Dimension,
    U: Unit<Dimension = D>,
    T: Ord,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Shared conversion surface for quantity-backed values with a canonical
/// presentation scalar.
pub trait QuantityDisplayValue {
    /// Canonical scalar used by the display conversion.
    type DisplayValue: Copy + fmt::Display;

    /// Returns the canonical presentation scalar for this quantity.
    fn display_value(self) -> Self::DisplayValue;
}

/// Electrical voltage stored in millivolts.
pub type Voltage = Quantity<ElectricPotential, MilliVolt, i32>;

const fn saturating_u64_to_i32(value: u64) -> i32 {
    if value > i32::MAX as u64 {
        i32::MAX
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            value as i32
        }
    }
}

const fn saturating_i64_to_i32(value: i64) -> i32 {
    const I32_MAX_I64: i64 = 2_147_483_647;
    const I32_MIN_I64: i64 = -2_147_483_648;

    if value > I32_MAX_I64 {
        i32::MAX
    } else if value < I32_MIN_I64 {
        i32::MIN
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            value as i32
        }
    }
}

/// Divides two signed integers and rounds the quotient to the nearest integer.
#[must_use]
pub fn round_div_i32(numerator: i32, denominator: i32) -> i32 {
    if denominator == 0 {
        return 0;
    }

    let numerator = i64::from(numerator);
    let denominator = i64::from(denominator);
    let sign = if (numerator < 0) ^ (denominator < 0) {
        -1
    } else {
        1
    };
    let rounded = (numerator.abs() + denominator.abs() / 2) / denominator.abs();
    saturating_i64_to_i32(rounded.saturating_mul(sign))
}

#[must_use]
fn round_div_i64_to_i32(numerator: i64, denominator: i64) -> i32 {
    if denominator == 0 {
        return 0;
    }

    let sign = if (numerator < 0) ^ (denominator < 0) {
        -1
    } else {
        1
    };
    let rounded = (numerator.abs() + denominator.abs() / 2) / denominator.abs();
    saturating_i64_to_i32(rounded.saturating_mul(sign))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn round_f32_to_i32(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn round_f32_to_i64(value: f32) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(i64::MIN as f32, i64::MAX as f32) as i64
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn round_f32_to_u64(value: f32) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    value.round().clamp(0.0, u64::MAX as f32) as u64
}

impl Voltage {
    /// Creates a voltage from millivolts.
    #[must_use]
    pub const fn from_millivolts(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a voltage from centivolts.
    #[must_use]
    pub const fn from_centivolts(value: i32) -> Self {
        Self::from_millivolts(value.saturating_mul(10))
    }

    /// Creates a voltage from decivolts.
    #[must_use]
    pub const fn from_deci_volts(value: i32) -> Self {
        Self::from_millivolts(value.saturating_mul(100))
    }

    /// Returns this voltage in millivolts.
    #[must_use]
    pub const fn as_millivolts(self) -> i32 {
        self.unit_value()
    }

    /// Returns this voltage in volts.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_volts(self) -> f32 {
        self.as_millivolts() as f32 / 1_000.0
    }

    /// Returns this voltage in whole volts, rounded to the nearest volt.
    #[must_use]
    pub fn as_whole_volts(self) -> u64 {
        u64::from(self.as_millivolts().unsigned_abs()).saturating_add(500) / 1_000
    }

    /// Creates a voltage from whole volts.
    #[must_use]
    pub const fn from_volts(value: u64) -> Self {
        Self::from_millivolts(saturating_u64_to_i32(value.saturating_mul(1_000)))
    }

    /// Creates a voltage from volts represented as a floating-point number.
    #[must_use]
    pub fn from_volts_f32(value: f32) -> Self {
        Self::from_millivolts(round_f32_to_i32(value * 1_000.0))
    }

    /// Returns this pack voltage as a single-cell voltage for the given series count.
    #[must_use]
    pub fn as_cell_voltage(self, series_cells: SeriesCount) -> CellVoltage {
        let series_cells = i32::from(series_cells.get());
        if series_cells <= 0 {
            return CellVoltage::from_microvolts(0);
        }

        let numerator = i64::from(self.as_millivolts()).saturating_mul(1_000);
        let rounded = (numerator + i64::from(series_cells / 2)) / i64::from(series_cells);
        CellVoltage::from_microvolts(saturating_i64_to_i32(rounded))
    }

    /// Returns this voltage's position inside a voltage range as a whole percent.
    #[must_use]
    pub fn percent_of_range(self, voltage_range: &RangeInclusive<Self>) -> BatteryLevel {
        let voltage = i64::from(self.as_millivolts());
        let range_start = i64::from(voltage_range.start().as_millivolts());
        let range_end = i64::from(voltage_range.end().as_millivolts());
        if range_end <= range_start || voltage <= range_start {
            return BatteryLevel::from_percent(0);
        }
        if voltage >= range_end {
            return BatteryLevel::from_percent(100);
        }

        let percent = u64::try_from(
            (voltage - range_start)
                .saturating_mul(100)
                .saturating_add((range_end - range_start) / 2)
                / (range_end - range_start),
        )
        .unwrap_or(0);
        BatteryLevel::from_percent(u8::try_from(percent).unwrap_or(100))
    }

    /// Creates a pack voltage from a single-cell voltage and series cell count.
    #[must_use]
    pub fn from_cell_voltage(cell_voltage: CellVoltage, series_cells: i32) -> Self {
        if series_cells <= 0 {
            return Self::from_millivolts(0);
        }

        let numerator = i64::from(cell_voltage.as_microvolts()) * i64::from(series_cells);
        let rounded = (numerator + 500) / 1_000;
        Self::from_millivolts(saturating_i64_to_i32(rounded))
    }
}

impl QuantityDisplayValue for Voltage {
    type DisplayValue = u64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_whole_volts()
    }
}

/// Wire-encoded electrical voltage stored in centivolts.
pub type WireVoltage = Quantity<ElectricPotential, CentiVolt, u16>;

impl WireVoltage {
    /// Creates a voltage from centivolts.
    #[must_use]
    pub const fn from_centivolts(value: u16) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this voltage in centivolts.
    #[must_use]
    pub const fn as_centivolts(self) -> u16 {
        self.unit_value()
    }

    /// Converts this centivolt quantity into millivolts.
    #[must_use]
    pub const fn as_millivolts(self) -> i32 {
        self.as_centivolts() as i32 * 10
    }

    /// Scales this wire voltage into a pack voltage using a milli-ratio.
    #[must_use]
    pub fn as_scaled_voltage(self, scaler_milli: i32) -> Voltage {
        if scaler_milli <= 0 {
            return Voltage::from_millivolts(0);
        }

        let numerator = i64::from(self.as_millivolts()) * i64::from(scaler_milli);
        let scaled = (numerator + 500) / 1_000;
        Voltage::from_millivolts(saturating_i64_to_i32(scaled))
    }

    /// Converts a pack voltage back into wire centivolts using a milli-ratio.
    #[must_use]
    pub fn from_scaled_voltage(voltage: Voltage, scaler_milli: i32) -> Self {
        if scaler_milli <= 0 {
            return Self::from_centivolts(0);
        }

        let numerator = i64::from(voltage.as_millivolts()) * 100;
        let centivolts = (numerator + i64::from(scaler_milli / 2)) / i64::from(scaler_milli);
        Self::from_centivolts(u16::try_from(centivolts).unwrap_or(u16::MAX))
    }
}

/// Single-cell voltage stored in microvolts.
pub type CellVoltage = Quantity<ElectricPotential, MicroVolt, i32>;

impl CellVoltage {
    /// Creates a cell voltage from microvolts.
    #[must_use]
    pub const fn from_microvolts(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this cell voltage in microvolts.
    #[must_use]
    pub const fn as_microvolts(self) -> i32 {
        self.unit_value()
    }
}

/// Electrical current stored in milliamps.
pub type Current = Quantity<ElectricCurrent, MilliAmp, i32>;

/// Battery/input current stored in milliamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct BatteryCurrent(Current);

impl BatteryCurrent {
    /// Creates a battery current from milliamps.
    #[must_use]
    pub const fn from_milliamps(value: i32) -> Self {
        Self(Current::from_milliamps(value))
    }

    /// Creates a battery current from centiamps.
    #[must_use]
    pub const fn from_centiamps(value: i32) -> Self {
        Self(Current::from_centiamps(value))
    }

    /// Creates a battery current from deciamps.
    #[must_use]
    pub const fn from_deciamps(value: i32) -> Self {
        Self(Current::from_deciamps(value))
    }

    /// Creates a battery current from whole amps.
    #[must_use]
    pub const fn from_amps(value: i64) -> Self {
        Self(Current::from_amps(value))
    }

    /// Creates a battery current from amps represented as a floating-point number.
    #[must_use]
    pub fn from_amps_f32(value: f32) -> Self {
        Self(Current::from_amps_f32(value))
    }

    /// Returns this battery current in milliamps.
    #[must_use]
    pub const fn as_milliamps(self) -> i32 {
        self.0.as_milliamps()
    }

    /// Returns this battery current in amps.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_amps(self) -> f32 {
        self.0.as_amps()
    }
}

impl core::ops::Deref for BatteryCurrent {
    type Target = Current;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Current> for BatteryCurrent {
    fn from(value: Current) -> Self {
        Self(value)
    }
}

impl From<BatteryCurrent> for Current {
    fn from(value: BatteryCurrent) -> Self {
        value.0
    }
}

impl fmt::Display for BatteryCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Motor/phase current stored in milliamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct PhaseCurrent(Current);

impl PhaseCurrent {
    /// Creates a phase current from milliamps.
    #[must_use]
    pub const fn from_milliamps(value: i32) -> Self {
        Self(Current::from_milliamps(value))
    }

    /// Creates a phase current from centiamps.
    #[must_use]
    pub const fn from_centiamps(value: i32) -> Self {
        Self(Current::from_centiamps(value))
    }

    /// Creates a phase current from deciamps.
    #[must_use]
    pub const fn from_deciamps(value: i32) -> Self {
        Self(Current::from_deciamps(value))
    }

    /// Creates a phase current from whole amps.
    #[must_use]
    pub const fn from_amps(value: i64) -> Self {
        Self(Current::from_amps(value))
    }

    /// Creates a phase current from amps represented as a floating-point number.
    #[must_use]
    pub fn from_amps_f32(value: f32) -> Self {
        Self(Current::from_amps_f32(value))
    }

    /// Returns this phase current in milliamps.
    #[must_use]
    pub const fn as_milliamps(self) -> i32 {
        self.0.as_milliamps()
    }

    /// Returns this phase current in amps.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_amps(self) -> f32 {
        self.0.as_amps()
    }
}

impl core::ops::Deref for PhaseCurrent {
    type Target = Current;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Current> for PhaseCurrent {
    fn from(value: Current) -> Self {
        Self(value)
    }
}

impl From<PhaseCurrent> for Current {
    fn from(value: PhaseCurrent) -> Self {
        value.0
    }
}

impl fmt::Display for PhaseCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Current {
    /// Creates a current from milliamps.
    #[must_use]
    pub const fn from_milliamps(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a current from centiamps.
    #[must_use]
    pub const fn from_centiamps(value: i32) -> Self {
        Self::from_milliamps(value.saturating_mul(10))
    }

    /// Creates a current from deciamps.
    #[must_use]
    pub const fn from_deciamps(value: i32) -> Self {
        Self::from_milliamps(value.saturating_mul(100))
    }

    /// Returns this current in milliamps.
    #[must_use]
    pub const fn as_milliamps(self) -> i32 {
        self.unit_value()
    }

    /// Returns this current in amps.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_amps(self) -> f32 {
        self.as_milliamps() as f32 / 1_000.0
    }

    /// Returns this current magnitude as a current value.
    #[must_use]
    pub fn abs(self) -> Self {
        Self::from_milliamps(self.as_milliamps().saturating_abs())
    }

    /// Returns this current in whole amps, rounded toward zero.
    #[must_use]
    pub fn as_whole_amps(self) -> i64 {
        i64::from(self.as_milliamps() / 1_000)
    }

    /// Returns this current magnitude in whole amps, rounded to the nearest amp.
    #[must_use]
    pub fn as_abs_whole_amps(self) -> u64 {
        u64::from(self.as_milliamps().unsigned_abs()).saturating_add(500) / 1_000
    }

    /// Creates a current from whole amps.
    #[must_use]
    pub const fn from_amps(value: i64) -> Self {
        Self::from_milliamps(saturating_i64_to_i32(value.saturating_mul(1_000)))
    }

    /// Creates a current from amps represented as a floating-point number.
    #[must_use]
    pub fn from_amps_f32(value: f32) -> Self {
        Self::from_milliamps(round_f32_to_i32(value * 1_000.0))
    }
}

impl QuantityDisplayValue for Current {
    type DisplayValue = i64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_whole_amps()
    }
}

/// Peak current stored in milliamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct PeakCurrent(Current);

impl PeakCurrent {
    /// Creates a peak current from milliamps.
    #[must_use]
    pub const fn from_milliamps(value: i32) -> Self {
        Self(Current::from_milliamps(value))
    }

    /// Creates a peak current from amps represented as a floating-point number.
    #[must_use]
    pub fn from_amps_f32(value: f32) -> Self {
        Self(Current::from_amps_f32(value))
    }

    /// Creates a peak current from whole amps.
    #[must_use]
    pub const fn from_amps(value: i64) -> Self {
        Self(Current::from_amps(value))
    }

    /// Returns this peak current in milliamps.
    #[must_use]
    pub const fn as_milliamps(self) -> i32 {
        self.0.as_milliamps()
    }

    /// Returns this peak current in amps.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_amps(self) -> f32 {
        self.0.as_amps()
    }
}

impl core::ops::Deref for PeakCurrent {
    type Target = Current;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Current> for PeakCurrent {
    fn from(value: Current) -> Self {
        Self(value)
    }
}

impl From<PeakCurrent> for Current {
    fn from(value: PeakCurrent) -> Self {
        value.0
    }
}

impl fmt::Display for PeakCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Rotational speed stored in electrical revolutions per minute.
pub type RotationalSpeed = Quantity<AngularVelocity, ElectricalRevolutionPerMinute, i32>;

impl RotationalSpeed {
    /// Creates a rotational speed from electrical revolutions per minute.
    #[must_use]
    pub const fn from_erpm(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this rotational speed in electrical revolutions per minute.
    #[must_use]
    pub const fn as_erpm(self) -> i32 {
        self.unit_value()
    }

    /// Creates a rotational speed from electrical RPM represented as a floating-point number.
    #[must_use]
    pub fn from_erpm_f32(value: f32) -> Self {
        Self::from_erpm(round_f32_to_i32(value))
    }

    /// Converts this electrical rotational speed to linear speed using drive geometry.
    #[must_use]
    pub fn as_speed(
        self,
        motor_pole_pairs: u8,
        gear_ratio_denominator: u8,
        wheel_circumference: Distance,
    ) -> Option<Speed> {
        let denominator = i64::from(motor_pole_pairs) * i64::from(gear_ratio_denominator) * 60;
        if denominator == 0 {
            return None;
        }

        let wheel_circumference_mm = i64::try_from(wheel_circumference.as_millimetres()).ok()?;
        let numerator = i64::from(self.as_erpm()) * wheel_circumference_mm;
        Some(Speed::from_millimetres_per_second(round_div_i64_to_i32(
            numerator,
            denominator,
        )))
    }
}

/// Relative tachometer reading stored as signed counts.
pub type TachometerReading = Quantity<Rotation, TachometerCountUnit, i32>;

impl TachometerReading {
    /// Creates a relative tachometer reading from signed counts.
    #[must_use]
    pub const fn from_counts(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this relative tachometer reading as signed counts.
    #[must_use]
    pub const fn as_counts(self) -> i32 {
        self.unit_value()
    }
}

/// Electrical power stored in milliwatts.
pub type Power = Quantity<ElectricPower, MilliWatt, i64>;

impl Power {
    /// Creates a power value from milliwatts.
    #[must_use]
    pub const fn from_milliwatts(value: i64) -> Self {
        Self::from_unit_value(value)
    }

    /// Calculates electrical power from voltage and current.
    #[must_use]
    pub fn from_voltage_current(voltage: Voltage, current: impl Into<Current>) -> Self {
        let current = current.into();
        Self::from_milliwatts(
            i64::from(voltage.as_millivolts()) * i64::from(current.as_milliamps()) / 1_000,
        )
    }

    /// Returns this power in milliwatts.
    #[must_use]
    pub const fn as_milliwatts(self) -> i64 {
        self.unit_value()
    }

    /// Returns this power in watts.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_watts(self) -> f32 {
        self.as_milliwatts() as f32 / 1_000.0
    }

    /// Returns this power in whole watts, rounded toward zero.
    #[must_use]
    pub const fn as_whole_watts(self) -> i64 {
        self.as_milliwatts() / 1_000
    }

    /// Creates a power from whole watts.
    #[must_use]
    pub const fn from_watts(value: i64) -> Self {
        Self::from_milliwatts(value.saturating_mul(1_000))
    }

    /// Creates a power from watts represented as a floating-point number.
    #[must_use]
    pub fn from_watts_f32(value: f32) -> Self {
        Self::from_milliwatts(round_f32_to_i64(value * 1_000.0))
    }
}

impl QuantityDisplayValue for Power {
    type DisplayValue = i64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_whole_watts()
    }
}

/// Electrical energy stored in watt-hours.
pub type Energy = Quantity<ElectricEnergy, WattHour, u32>;

impl Energy {
    /// Creates an energy value from watt-hours.
    #[must_use]
    pub const fn from_watt_hours(value: u32) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates an energy value from per-cell watt-hours and pack geometry.
    #[must_use]
    pub const fn from_cell_geometry(
        cell_watt_hours: u32,
        series_cells: SeriesCount,
        parallel_packs: ParallelCount,
    ) -> Self {
        let value = cell_watt_hours
            .saturating_mul(series_cells.get() as u32)
            .saturating_mul(parallel_packs.get() as u32);
        Self::from_watt_hours(value)
    }

    /// Returns this energy in watt-hours.
    #[must_use]
    pub const fn as_watt_hours(self) -> u32 {
        self.unit_value()
    }
}

/// Electrical charge capacity stored in milliamp-hours.
pub type Capacity = Quantity<ElectricCharge, MilliAmpHour, u32>;

impl Capacity {
    /// Creates a capacity value from milliamp-hours.
    #[must_use]
    pub const fn from_milliamp_hours(value: u32) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a capacity value from per-cell milliamp-hours and parallel packs.
    #[must_use]
    pub const fn from_parallel_packs(
        cell_capacity_milliamp_hours: u32,
        parallel_packs: ParallelCount,
    ) -> Self {
        Self::from_milliamp_hours(
            cell_capacity_milliamp_hours.saturating_mul(parallel_packs.get() as u32),
        )
    }

    /// Returns this capacity in milliamp-hours.
    #[must_use]
    pub const fn as_milliamp_hours(self) -> u32 {
        self.unit_value()
    }
}

/// Linear speed stored in millimetres per second.
pub type Speed = Quantity<Velocity, MillimetrePerSecond, i32>;

impl Speed {
    /// Creates a speed from millimetres per second.
    #[must_use]
    pub const fn from_millimetres_per_second(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a speed from centimetres per second.
    #[must_use]
    pub const fn from_centimetres_per_second(value: i32) -> Self {
        Self::from_millimetres_per_second(value.saturating_mul(10))
    }

    /// Creates a speed from kilometres per hour.
    #[must_use]
    pub fn from_kmh(value: u64) -> Self {
        Self::from_milli_kmh(saturating_u64_to_i32(value.saturating_mul(1_000)))
    }

    /// Creates a speed from metres per second represented as a floating-point number.
    #[must_use]
    pub fn from_metres_per_second(value: f32) -> Self {
        Self::from_millimetres_per_second(round_f32_to_i32(value * 1_000.0))
    }

    /// Creates a speed from milli-kilometres per hour.
    #[must_use]
    pub const fn from_milli_kmh(value: i32) -> Self {
        Self::from_millimetres_per_second(value.saturating_mul(5) / 18)
    }

    /// Creates a speed from deci-kilometres per hour.
    #[must_use]
    pub const fn from_deci_kmh(value: i32) -> Self {
        Self::from_milli_kmh(value.saturating_mul(100))
    }

    /// Returns this speed in millimetres per second.
    #[must_use]
    pub const fn as_millimetres_per_second(self) -> i32 {
        self.unit_value()
    }

    /// Returns this speed in centimetres per second.
    #[must_use]
    pub const fn as_centimetres_per_second(self) -> i32 {
        self.as_millimetres_per_second() / 10
    }

    /// Returns this speed in metres per second.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_metres_per_second(self) -> f32 {
        self.as_millimetres_per_second() as f32 / 1_000.0
    }

    /// Returns this speed in kilometres per hour.
    #[must_use]
    pub fn as_kmh(self) -> i32 {
        self.as_milli_kmh() / 1_000
    }

    /// Returns this speed in whole kilometres per hour, rounded to the nearest km/h.
    #[must_use]
    pub fn as_kmh_rounded(self) -> i32 {
        let numerator = i64::from(self.as_millimetres_per_second()) * 18;
        if numerator >= 0 {
            i32::try_from((numerator + 2_500) / 5_000).unwrap_or(i32::MAX)
        } else {
            i32::try_from((numerator - 2_500) / 5_000).unwrap_or(i32::MIN)
        }
    }

    /// Returns this speed in milli-kilometres per hour.
    #[must_use]
    pub const fn as_milli_kmh(self) -> i32 {
        self.as_millimetres_per_second().saturating_mul(18) / 5
    }

    /// Returns this speed in deci-kilometres per hour.
    #[must_use]
    pub const fn as_deci_kmh(self) -> i32 {
        self.as_milli_kmh() / 100
    }

    /// Returns this speed in deci-kilometres per hour, rounded to the nearest tenth.
    #[must_use]
    pub fn as_deci_kmh_rounded(self) -> i32 {
        let numerator = i64::from(self.as_millimetres_per_second()) * 36;
        if numerator >= 0 {
            i32::try_from((numerator + 500) / 1_000).unwrap_or(i32::MAX)
        } else {
            i32::try_from((numerator - 500) / 1_000).unwrap_or(i32::MIN)
        }
    }

    /// Creates a speed from miles per hour.
    #[must_use]
    pub fn from_mph(value: u64) -> Self {
        let millimetres_per_second = value.saturating_mul(447_388).saturating_add(500) / 1_000;
        Self::from_millimetres_per_second(i32::try_from(millimetres_per_second).unwrap_or(i32::MAX))
    }

    /// Creates a speed from whole miles per hour, truncating the km/h result to a whole number.
    #[must_use]
    pub fn from_mph_floor_kmh(value: u64) -> Self {
        let kmh = value.saturating_mul(1_609_344) / 1_000_000;
        Self::from_kmh(kmh)
    }

    /// Scales a milli-km/h value by a milli-ratio.
    #[must_use]
    pub fn from_milli_kmh_scaled(value: i32, scale_milli: i32) -> Self {
        if scale_milli <= 0 {
            return Self::from_millimetres_per_second(0);
        }

        let numerator = i64::from(value) * i64::from(scale_milli);
        let scaled = round_div_i64_to_i32(numerator, 1_000_000);
        Self::from_milli_kmh(scaled)
    }

    /// Returns this speed in whole miles per hour, rounded to the nearest mph.
    #[must_use]
    pub fn as_mph(self) -> u64 {
        u64::from(self.as_millimetres_per_second().unsigned_abs())
            .saturating_mul(1_000)
            .saturating_add(223_694)
            / 447_388
    }
}

impl QuantityDisplayValue for Speed {
    type DisplayValue = u64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_mph()
    }
}

/// Linear distance stored in millimetres.
pub type Distance = Quantity<Length, Millimetre, u64>;

impl Distance {
    /// Creates a distance from millimetres.
    #[must_use]
    pub const fn from_millimetres(value: u64) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a distance from metres.
    #[must_use]
    pub const fn from_metres(value: u64) -> Self {
        Self::from_millimetres(value.saturating_mul(1_000))
    }

    /// Creates a distance from milli-miles.
    #[must_use]
    pub fn from_milli_miles(value: u32) -> Self {
        Self::from_millimetres(u64::from(value).saturating_mul(1_609_344) / 1_000)
    }

    /// Creates a distance from metres represented as a floating-point number.
    #[must_use]
    pub fn from_metres_f32(value: f32) -> Self {
        Self::from_millimetres(round_f32_to_u64(value * 1_000.0))
    }

    /// Returns this distance in millimetres.
    #[must_use]
    pub const fn as_millimetres(self) -> u64 {
        self.unit_value()
    }

    /// Returns this distance in metres.
    #[must_use]
    pub const fn as_metres(self) -> u64 {
        self.as_whole_metres()
    }

    /// Returns this distance in whole metres, rounded toward zero.
    #[must_use]
    pub const fn as_whole_metres(self) -> u64 {
        self.as_millimetres() / 1_000
    }

    /// Returns this distance in tenths of a kilometre, rounded to the nearest tenth.
    #[must_use]
    pub const fn as_kilometre_tenths(self) -> u64 {
        self.as_whole_metres()
            .saturating_mul(10)
            .saturating_add(500)
            / 1_000
    }
}

impl QuantityDisplayValue for Distance {
    type DisplayValue = u64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_whole_metres()
    }
}

/// Signed linear distance offset stored in millimetres.
pub type DistanceOffset = Quantity<Length, Millimetre, i64>;

impl DistanceOffset {
    /// Creates a signed distance offset from millimetres.
    #[must_use]
    pub const fn from_millimetres(value: i64) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a signed distance offset from metres.
    #[must_use]
    pub const fn from_metres(value: i64) -> Self {
        Self::from_millimetres(value.saturating_mul(1_000))
    }

    /// Returns this signed distance offset in millimetres.
    #[must_use]
    pub const fn as_millimetres(self) -> i64 {
        self.unit_value()
    }
}

/// Time duration stored in milliseconds.
pub type Duration = Quantity<Time, Millisecond, u64>;

impl Duration {
    /// Creates a duration from milliseconds.
    #[must_use]
    pub const fn from_milliseconds(value: u64) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a duration from deciseconds.
    #[must_use]
    pub const fn from_deciseconds(value: u64) -> Self {
        Self::from_milliseconds(value.saturating_mul(100))
    }

    /// Creates a duration from seconds.
    #[must_use]
    pub const fn from_seconds(value: u64) -> Self {
        Self::from_milliseconds(value.saturating_mul(1_000))
    }

    /// Creates a duration from minutes.
    #[must_use]
    pub const fn from_minutes(value: u64) -> Self {
        Self::from_seconds(value.saturating_mul(60))
    }

    /// Returns this duration in milliseconds.
    #[must_use]
    pub const fn as_milliseconds(self) -> u64 {
        self.unit_value()
    }

    /// Returns this duration in whole seconds.
    #[must_use]
    pub const fn as_seconds(self) -> u64 {
        self.as_milliseconds() / 1_000
    }

    /// Returns this duration in whole minutes.
    #[must_use]
    pub const fn as_minutes(self) -> u64 {
        self.as_seconds() / 60
    }

    /// Creates a duration from seconds represented as a floating-point number.
    #[must_use]
    pub fn from_seconds_f32(value: f32) -> Self {
        Self::from_milliseconds(round_f32_to_u64(value * 1_000.0))
    }
}

/// Temperature stored in millicelsius.
pub type Temperature = Quantity<ThermodynamicTemperature, MilliCelsius, i32>;

impl Temperature {
    /// Creates a temperature from millicelsius.
    #[must_use]
    pub const fn from_millicelsius(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a temperature from decicelsius.
    #[must_use]
    pub const fn from_deci_celsius(value: i32) -> Self {
        Self::from_millicelsius(value.saturating_mul(100))
    }

    /// Creates a temperature from centicelsius.
    #[must_use]
    pub const fn from_centi_celsius(value: i32) -> Self {
        Self::from_millicelsius(value.saturating_mul(10))
    }

    /// Returns this temperature in millicelsius.
    #[must_use]
    pub const fn as_millicelsius(self) -> i32 {
        self.unit_value()
    }

    /// Returns this temperature in celsius.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_celsius(self) -> f32 {
        self.as_millicelsius() as f32 / 1_000.0
    }

    /// Returns this temperature in whole celsius, rounded toward zero.
    #[must_use]
    pub fn as_whole_celsius(self) -> i64 {
        i64::from(self.as_millicelsius() / 1_000)
    }

    /// Returns this temperature magnitude in whole celsius, rounded to the nearest degree.
    #[must_use]
    pub fn as_abs_whole_celsius(self) -> u64 {
        u64::from(self.as_millicelsius().unsigned_abs()).saturating_add(500) / 1_000
    }

    /// Creates a temperature from whole celsius.
    #[must_use]
    pub const fn from_celsius(value: i64) -> Self {
        Self::from_millicelsius(saturating_i64_to_i32(value.saturating_mul(1_000)))
    }

    /// Creates a temperature from raw MPU6050 sensor counts.
    #[must_use]
    pub const fn from_mpu6050_counts(value: i16) -> Self {
        Self::from_millicelsius(36_530 + (value as i32 * 1_000) / 340)
    }
}

impl QuantityDisplayValue for Temperature {
    type DisplayValue = i64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_whole_celsius()
    }
}

/// Plane angle stored in millidegrees.
pub type Angle = Quantity<PlaneAngle, MilliDegree, i32>;

impl Angle {
    /// Creates an angle from millidegrees.
    #[must_use]
    pub const fn from_millidegrees(value: i32) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates an angle from decidegrees.
    #[must_use]
    pub const fn from_deci_degrees(value: i32) -> Self {
        Self::from_millidegrees(value.saturating_mul(10))
    }

    /// Returns this angle in millidegrees.
    #[must_use]
    pub const fn as_millidegrees(self) -> i32 {
        self.unit_value()
    }

    /// Returns this angle in degrees.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_degrees(self) -> f32 {
        self.as_millidegrees() as f32 / 1_000.0
    }

    /// Returns this angle in whole degrees, rounded toward zero.
    #[must_use]
    pub fn as_whole_degrees(self) -> i64 {
        i64::from(self.as_millidegrees() / 1_000)
    }

    /// Creates an angle from whole degrees.
    #[must_use]
    pub const fn from_degrees(value: i64) -> Self {
        Self::from_millidegrees(saturating_i64_to_i32(value.saturating_mul(1_000)))
    }
}

impl QuantityDisplayValue for Angle {
    type DisplayValue = i64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_whole_degrees()
    }
}

/// Ratio stored in permille.
pub type DutyCycle = Quantity<Ratio, Permille, i16>;

impl DutyCycle {
    /// Creates a duty cycle from permille.
    #[must_use]
    pub const fn from_permille(value: i16) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a duty cycle from centipercent.
    #[must_use]
    pub fn from_centipercent(value: u16) -> Self {
        Self::from_permille(i16::try_from(value / 10).unwrap_or(i16::MAX))
    }

    /// Creates a duty cycle from a raw decipermille value.
    #[must_use]
    pub const fn from_decipermille(value: i16) -> Self {
        Self::from_permille(value / 10)
    }

    /// Creates a duty cycle from a centered raw PWM register.
    #[must_use]
    pub fn from_centered_pwm(value: u16) -> Self {
        let centered = i32::from(value) - 0x8000;
        let permille = centered * 1_000 / 0x8000;
        Self::from_permille(
            i16::try_from(permille).unwrap_or(if permille.is_negative() {
                i16::MIN
            } else {
                i16::MAX
            }),
        )
    }

    /// Returns this duty cycle in permille.
    #[must_use]
    pub const fn as_permille(self) -> i16 {
        self.unit_value()
    }

    /// Returns this duty cycle as percent.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_percent(self) -> f32 {
        f32::from(self.as_permille()) / 10.0
    }

    /// Returns this duty cycle as whole percent, rounded toward zero.
    #[must_use]
    pub fn as_whole_percent(self) -> i64 {
        i64::from(self.as_permille() / 10)
    }
}

impl QuantityDisplayValue for DutyCycle {
    type DisplayValue = i64;

    fn display_value(self) -> Self::DisplayValue {
        self.as_whole_percent()
    }
}

/// Battery state-of-charge stored as a percentage.
pub type BatteryLevel = Quantity<Ratio, PercentUnit, u8>;

/// Shared conversion surface for battery state-of-charge quantities.
#[allow(clippy::wrong_self_convention)]
pub trait PercentQuantity {
    /// Creates a battery level from a percent value.
    fn from_percent(value: u8) -> Self;

    /// Returns this battery level as a percent value.
    fn as_percent(self) -> u8;
}

impl BatteryLevel {
    /// Creates a battery level from a percent value.
    #[must_use]
    pub const fn from_percent(value: u8) -> Self {
        Self::from_unit_value(value)
    }

    /// Creates a battery level from a signed percent value, clamping to the stored range.
    #[must_use]
    pub fn from_percent_i32(value: i32) -> Self {
        Self::from_percent(u8::try_from(value.clamp(0, 100)).unwrap_or(100))
    }

    /// Returns this battery level as a percent value.
    #[must_use]
    pub const fn as_percent(self) -> u8 {
        self.unit_value()
    }

    /// Returns this battery level as a unitless fraction in the range `0.0..=1.0`.
    #[must_use]
    pub fn as_ratio(self) -> f64 {
        f64::from(self.as_percent()) / 100.0
    }

    /// Linearly interpolates a battery level between two reference points.
    #[must_use]
    pub fn interpolate(low: Self, high: Self, value: i64, low_value: i64, high_value: i64) -> Self {
        let value_span = high_value - low_value;
        if value_span <= 0 {
            return low;
        }

        let level_span = i32::from(high.as_percent()) - i32::from(low.as_percent());
        let value_offset = value - low_value;
        let numerator = value_offset.saturating_mul(i64::from(level_span));
        let level = i32::from(low.as_percent())
            + round_div_i32(
                i32::try_from(numerator).unwrap_or_else(|_| {
                    if numerator.is_negative() {
                        i32::MIN
                    } else {
                        i32::MAX
                    }
                }),
                i32::try_from(value_span).unwrap_or_else(|_| {
                    if value_span.is_negative() {
                        i32::MIN
                    } else {
                        i32::MAX
                    }
                }),
            );
        Self::from_percent_i32(level)
    }

    /// Evaluates a piecewise-linear battery curve over typed percentage points.
    #[must_use]
    pub fn from_piecewise_linear(value: i64, points: &[(i64, Self)]) -> Self {
        let Some((first_value, first_level)) = points.first().copied() else {
            return Self::from_percent(0);
        };
        if value <= first_value {
            return first_level;
        }

        for window in points.windows(2) {
            let [low, high] = window else {
                continue;
            };
            let (low_value, low_level) = *low;
            let (high_value, high_level) = *high;
            if value <= high_value {
                return Self::interpolate(low_level, high_level, value, low_value, high_value);
            }
        }

        points
            .last()
            .copied()
            .map_or_else(|| Self::from_percent(0), |(_, level)| level)
    }
}

impl PercentQuantity for BatteryLevel {
    fn from_percent(value: u8) -> Self {
        Self::from_unit_value(value)
    }

    fn as_percent(self) -> u8 {
        self.unit_value()
    }
}

impl QuantityDisplayValue for BatteryLevel {
    type DisplayValue = u8;

    fn display_value(self) -> Self::DisplayValue {
        self.as_percent()
    }
}

/// Radio signal strength stored in dBm.
pub type SignalStrength = Quantity<SignalPower, DecibelMilliwatt, i16>;

impl SignalStrength {
    /// Creates a signal-strength value from dBm.
    #[must_use]
    pub const fn from_dbm(value: i16) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this signal strength in dBm.
    #[must_use]
    pub const fn as_dbm(self) -> i16 {
        self.unit_value()
    }

    /// Returns this signal strength as a coarse UI quality percentage.
    #[must_use]
    pub fn as_quality_percent(self) -> u8 {
        let dbm = self.as_dbm().clamp(-100, -50);
        let offset = i32::from(dbm) + 100;
        let percent = (offset * 100) / 50;
        u8::try_from(percent).unwrap_or(100)
    }
}

impl QuantityDisplayValue for SignalStrength {
    type DisplayValue = u8;

    fn display_value(self) -> Self::DisplayValue {
        self.as_quality_percent()
    }
}

/// Series-connected cell count for model battery and BMS metadata.
pub type SeriesCount = Quantity<Count, Cell, u8>;

/// Parallel pack count for model BMS metadata.
pub type ParallelCount = Quantity<Count, Pack, u8>;
