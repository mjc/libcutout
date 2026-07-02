//! Telemetry, readback, and measured-value domain types.

use crate::{
    Angle, BatteryCurrent, BatteryLevel, BatteryPagePayload, CommandKind, Distance, DutyCycle,
    MonotonicTimestamp, PhaseCurrent, Power, Speed, Temperature, VerificationStatus, Voltage,
};
use thiserror::Error;

/// Where a measured value came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueSource {
    /// Value was reported directly by the device.
    Reported,

    /// Value was calculated by Cutout from other known values.
    Calculated,

    /// Value was estimated by Cutout from incomplete evidence.
    Estimated,
}

/// Confidence or usability of a measured value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueQuality {
    /// Value is directly supported by observed protocol data.
    Known,

    /// Value is inferred from partial, model-specific, or less direct evidence.
    Inferred,
}

/// A value with source and quality metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Measured<T> {
    /// Fixed-unit value.
    pub value: T,

    /// Source of the value.
    pub source: ValueSource,

    /// Quality of the value.
    pub quality: ValueQuality,

    /// Verification state for the decoded value.
    pub verification: VerificationStatus,
}

impl<T> Measured<T> {
    /// Creates a known value reported directly by the device.
    #[must_use]
    pub const fn reported(value: T) -> Self {
        Self {
            value,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        }
    }

    /// Creates a value calculated from other known values.
    #[must_use]
    pub const fn calculated(value: T) -> Self {
        Self {
            value,
            source: ValueSource::Calculated,
            quality: ValueQuality::Known,
            verification: VerificationStatus::Inferred,
        }
    }

    /// Creates a value estimated from incomplete evidence.
    #[must_use]
    pub const fn estimated(value: T) -> Self {
        Self {
            value,
            source: ValueSource::Estimated,
            quality: ValueQuality::Inferred,
            verification: VerificationStatus::Inferred,
        }
    }

    /// Transforms the underlying value while keeping provenance metadata.
    #[must_use]
    pub fn map_value<U>(self, f: impl FnOnce(T) -> U) -> Measured<U> {
        Measured {
            value: f(self.value),
            source: self.source,
            quality: self.quality,
            verification: self.verification,
        }
    }
}

/// Raw numeric field reported by a protocol-specific response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawFieldValue {
    /// Protocol-family field identifier.
    pub id: u16,

    /// Sign-extended raw field value.
    pub value: i64,
}

impl RawFieldValue {
    /// Creates a raw numeric field value.
    #[must_use]
    pub const fn new(id: u16, value: i64) -> Self {
        Self { id, value }
    }
}

/// Generic firmware or protocol version information.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FirmwareInfo {
    /// Protocol version, when reported.
    pub protocol_version: Option<Measured<u16>>,

    /// Firmware major version, when reported.
    pub firmware_major: Option<Measured<u16>>,

    /// Firmware minor version, when reported.
    pub firmware_minor: Option<Measured<u16>>,

    /// Firmware patch version, when reported.
    pub firmware_patch: Option<Measured<u16>>,

    /// Raw build identifier, when a protocol exposes one.
    pub build_id: Option<RawFieldValue>,
}

/// Paired page-specific BMS pack-current values with shared provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsPackCurrents {
    /// First page-specific BMS pack current.
    current_0: BatteryCurrent,

    /// Second page-specific BMS pack current.
    current_1: BatteryCurrent,

    /// Source of the current values.
    pub source: ValueSource,

    /// Confidence in the current values.
    pub quality: ValueQuality,

    /// Verification state for the current values.
    pub verification: VerificationStatus,
}

impl BmsPackCurrents {
    /// Creates known BMS pack current values reported directly by the device.
    #[must_use]
    pub const fn reported(current_0: BatteryCurrent, current_1: BatteryCurrent) -> Self {
        Self {
            current_0,
            current_1,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        }
    }

    /// Returns the first page-specific BMS pack current.
    #[must_use]
    pub const fn current_0(self) -> BatteryCurrent {
        self.current_0
    }

    /// Returns the second page-specific BMS pack current.
    #[must_use]
    pub const fn current_1(self) -> BatteryCurrent {
        self.current_1
    }
}

/// Device charging state decoded from protocol-specific status fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargeMode {
    /// The device reports that charging is not active.
    NotCharging,

    /// The device reports that charging is active.
    Charging,
}

impl ChargeMode {
    /// Converts a protocol-specific active/inactive charging flag.
    #[must_use]
    pub const fn from_active(active: bool) -> Self {
        if active {
            Self::Charging
        } else {
            Self::NotCharging
        }
    }

    /// Returns true when charging is active.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Charging)
    }
}

impl core::fmt::Display for ChargeMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotCharging => f.write_str("not_charging"),
            Self::Charging => f.write_str("charging"),
        }
    }
}

/// Generic battery or BMS information.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatteryInfo {
    /// Pack or input voltage in millivolts.
    pub voltage: Option<Measured<Voltage>>,

    /// Pack or battery current in milliamps.
    pub current: Option<Measured<BatteryCurrent>>,

    /// Battery level reported by the device.
    pub level_reported: Option<Measured<BatteryLevel>>,

    /// Battery level estimated by Cutout.
    pub level_estimated: Option<Measured<BatteryLevel>>,

    /// Battery or BMS temperature in millicelsius.
    pub temperature: Option<Measured<Temperature>>,

    /// Raw battery/BMS state field, when present.
    pub raw_state: Option<RawFieldValue>,
}

/// Severity for a diagnostic detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    /// Informational diagnostic.
    Info,

    /// Warning diagnostic.
    Warning,

    /// Error diagnostic.
    Error,
}

/// Generic diagnostic detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticDetail {
    /// Raw diagnostic field.
    pub field: RawFieldValue,

    /// Diagnostic severity.
    pub severity: DiagnosticSeverity,

    /// Confidence in the diagnostic interpretation.
    pub quality: ValueQuality,

    /// Verification state for the diagnostic interpretation.
    pub verification: VerificationStatus,
}

/// Number of diagnostic detail slots retained in a diagnostic readback.
///
/// This is a temporary implementation capacity for compact read-only responses,
/// not a protocol-level limit.
pub const DIAGNOSTIC_READBACK_CAPACITY: usize = 4;

/// Number of protocol-native fields retained in a raw telemetry readback.
///
/// This is a temporary implementation capacity for compact read-only responses,
/// not a protocol-level limit.
pub const RAW_TELEMETRY_READBACK_CAPACITY: usize = 4;

/// Number of settings entries retained in a settings readback.
///
/// This is a temporary implementation capacity for compact read-only responses,
/// not a protocol-level limit.
pub const SETTINGS_READBACK_CAPACITY: usize = 4;

/// Error returned when a fixed readback cannot store every provided item.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ReadbackCapacityError {
    /// More items were provided than the readback can store.
    #[error("readback capacity {capacity} exceeded by {requested} requested items")]
    TooManyItems {
        /// Fixed readback capacity.
        capacity: usize,

        /// Requested item count.
        requested: usize,
    },
}

/// Bounded diagnostic readback response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticReadback {
    /// Diagnostic detail slots.
    pub details: [Option<DiagnosticDetail>; DIAGNOSTIC_READBACK_CAPACITY],
}

impl DiagnosticReadback {
    /// Builds a diagnostic readback from exactly one diagnostic detail.
    #[must_use]
    pub const fn from_detail(detail: DiagnosticDetail) -> Self {
        Self {
            details: [Some(detail), None, None, None],
        }
    }

    /// Builds a full diagnostic readback from exact-capacity diagnostic details.
    #[must_use]
    pub fn from_details(details: [DiagnosticDetail; DIAGNOSTIC_READBACK_CAPACITY]) -> Self {
        Self {
            details: details.map(Some),
        }
    }

    /// Builds a diagnostic readback from diagnostic details.
    ///
    /// # Errors
    ///
    /// Returns [`ReadbackCapacityError::TooManyItems`] when `details` exceeds
    /// [`DIAGNOSTIC_READBACK_CAPACITY`].
    pub fn try_from_details(details: &[DiagnosticDetail]) -> Result<Self, ReadbackCapacityError> {
        if details.len() > DIAGNOSTIC_READBACK_CAPACITY {
            return Err(ReadbackCapacityError::TooManyItems {
                capacity: DIAGNOSTIC_READBACK_CAPACITY,
                requested: details.len(),
            });
        }

        let mut readback = Self::default();
        for (slot, detail) in readback.details.iter_mut().zip(details.iter().copied()) {
            *slot = Some(detail);
        }
        Ok(readback)
    }
}

/// Bounded protocol-native raw telemetry readback.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawTelemetryReadback {
    /// Raw telemetry field slots.
    pub fields: [Option<RawFieldValue>; RAW_TELEMETRY_READBACK_CAPACITY],
}

impl RawTelemetryReadback {
    /// Builds a full raw telemetry readback from exact-capacity fields.
    #[must_use]
    pub fn from_fields(fields: [RawFieldValue; RAW_TELEMETRY_READBACK_CAPACITY]) -> Self {
        Self {
            fields: fields.map(Some),
        }
    }

    /// Builds a raw telemetry readback from protocol-native fields.
    ///
    /// # Errors
    ///
    /// Returns [`ReadbackCapacityError::TooManyItems`] when `fields` exceeds
    /// [`RAW_TELEMETRY_READBACK_CAPACITY`].
    pub fn try_from_fields(fields: &[RawFieldValue]) -> Result<Self, ReadbackCapacityError> {
        if fields.len() > RAW_TELEMETRY_READBACK_CAPACITY {
            return Err(ReadbackCapacityError::TooManyItems {
                capacity: RAW_TELEMETRY_READBACK_CAPACITY,
                requested: fields.len(),
            });
        }

        let mut readback = Self::default();
        for (slot, field) in readback.fields.iter_mut().zip(fields.iter().copied()) {
            *slot = Some(field);
        }
        Ok(readback)
    }
}

/// Generic read-only settings entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsEntry {
    /// Raw settings field.
    pub field: RawFieldValue,

    /// Source of the settings value.
    pub source: ValueSource,

    /// Confidence in the settings value.
    pub quality: ValueQuality,

    /// Verification state for the settings value.
    pub verification: VerificationStatus,
}

/// Bounded settings readback response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SettingsReadback {
    /// Settings entries.
    pub entries: [Option<SettingsEntry>; SETTINGS_READBACK_CAPACITY],
}

impl SettingsReadback {
    /// Builds a settings readback from exactly one settings entry.
    #[must_use]
    pub const fn from_entry(entry: SettingsEntry) -> Self {
        Self {
            entries: [Some(entry), None, None, None],
        }
    }

    /// Builds a full settings readback from exact-capacity settings entries.
    #[must_use]
    pub fn from_entries(entries: [SettingsEntry; SETTINGS_READBACK_CAPACITY]) -> Self {
        Self {
            entries: entries.map(Some),
        }
    }

    /// Builds a settings readback from settings entries.
    ///
    /// # Errors
    ///
    /// Returns [`ReadbackCapacityError::TooManyItems`] when `entries` exceeds
    /// [`SETTINGS_READBACK_CAPACITY`].
    pub fn try_from_entries(entries: &[SettingsEntry]) -> Result<Self, ReadbackCapacityError> {
        if entries.len() > SETTINGS_READBACK_CAPACITY {
            return Err(ReadbackCapacityError::TooManyItems {
                capacity: SETTINGS_READBACK_CAPACITY,
                requested: entries.len(),
            });
        }

        let mut readback = Self::default();
        for (slot, entry) in readback.entries.iter_mut().zip(entries.iter().copied()) {
            *slot = Some(entry);
        }
        Ok(readback)
    }
}

/// Generic read-only response payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadOnlyResponse {
    /// Firmware or protocol version response.
    Firmware(FirmwareInfo),

    /// Battery or BMS response.
    Battery(BatteryPagePayload),

    /// Diagnostic response.
    Diagnostics(DiagnosticReadback),

    /// Protocol-native raw telemetry response.
    RawTelemetry(RawTelemetryReadback),

    /// Settings readback response.
    Settings(SettingsReadback),
}

impl ReadOnlyResponse {
    /// Returns the command kind that requested this response.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Firmware(_) => CommandKind::RequestFirmwareInfo,
            Self::Battery(_) => CommandKind::RequestBatteryInfo,
            Self::Diagnostics(_) => CommandKind::RequestDiagnostics,
            Self::RawTelemetry(_) => CommandKind::RequestTelemetry,
            Self::Settings(_) => CommandKind::RequestSettings,
        }
    }
}

/// Partial telemetry update from a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryDelta {
    /// Host monotonic timestamp for this update.
    pub at_ms: MonotonicTimestamp,

    /// Reported or calculated speed in millimeters per second.
    pub speed: Option<Measured<Speed>>,

    /// Reported or measured input voltage in millivolts.
    pub voltage: Option<Measured<Voltage>>,

    /// Battery/input current in milliamps.
    pub battery_current: Option<Measured<BatteryCurrent>>,

    /// Motor/phase current in milliamps.
    pub motor_current: Option<Measured<PhaseCurrent>>,

    /// Electrical power in milliwatts.
    pub power: Option<Measured<Power>>,

    /// Controller temperature in millicelsius.
    pub controller_temperature: Option<Measured<Temperature>>,

    /// Motor temperature in millicelsius.
    pub motor_temperature: Option<Measured<Temperature>>,

    /// Battery temperature in millicelsius.
    pub battery_temperature: Option<Measured<Temperature>>,

    /// PWM duty in permille.
    pub pwm: Option<Measured<DutyCycle>>,

    /// Total or trip distance in millimeters.
    pub distance: Option<Measured<Distance>>,

    /// Pitch in millidegrees.
    pub pitch: Option<Measured<Angle>>,

    /// Roll in millidegrees.
    pub roll: Option<Measured<Angle>>,

    /// Battery level reported by the device.
    pub battery_level_reported: Option<Measured<BatteryLevel>>,

    /// Battery level estimated by Cutout.
    pub battery_level_estimated: Option<Measured<BatteryLevel>>,
}

impl TelemetryDelta {
    /// Creates an empty telemetry delta at a timestamp.
    #[must_use]
    pub const fn empty(at_ms: MonotonicTimestamp) -> Self {
        Self {
            at_ms,
            speed: None,
            voltage: None,
            battery_current: None,
            motor_current: None,
            power: None,
            controller_temperature: None,
            motor_temperature: None,
            battery_temperature: None,
            pwm: None,
            distance: None,
            pitch: None,
            roll: None,
            battery_level_reported: None,
            battery_level_estimated: None,
        }
    }
}

/// Aggregated latest-known telemetry snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TelemetrySnapshot {
    /// Timestamp of the latest applied delta.
    pub at_ms: Option<MonotonicTimestamp>,

    /// Latest known speed in millimeters per second.
    pub speed: Option<Measured<Speed>>,

    /// Latest known input voltage in millivolts.
    pub voltage: Option<Measured<Voltage>>,

    /// Latest known battery/input current in milliamps.
    pub battery_current: Option<Measured<BatteryCurrent>>,

    /// Latest known motor/phase current in milliamps.
    pub motor_current: Option<Measured<PhaseCurrent>>,

    /// Latest known electrical power in milliwatts.
    pub power: Option<Measured<Power>>,

    /// Latest known controller temperature in millicelsius.
    pub controller_temperature: Option<Measured<Temperature>>,

    /// Latest known motor temperature in millicelsius.
    pub motor_temperature: Option<Measured<Temperature>>,

    /// Latest known battery temperature in millicelsius.
    pub battery_temperature: Option<Measured<Temperature>>,

    /// Latest known PWM duty in permille.
    pub pwm: Option<Measured<DutyCycle>>,

    /// Latest known total or trip distance in millimeters.
    pub distance: Option<Measured<Distance>>,

    /// Latest known pitch in millidegrees.
    pub pitch: Option<Measured<Angle>>,

    /// Latest known roll in millidegrees.
    pub roll: Option<Measured<Angle>>,

    /// Latest known battery level reported by the device.
    pub battery_level_reported: Option<Measured<BatteryLevel>>,

    /// Latest known battery level estimated by Cutout.
    pub battery_level_estimated: Option<Measured<BatteryLevel>>,
}

impl TelemetrySnapshot {
    /// Applies a partial telemetry update, preserving fields absent from it.
    pub const fn apply_delta(&mut self, delta: TelemetryDelta) {
        self.at_ms = Some(delta.at_ms);

        if delta.speed.is_some() {
            self.speed = delta.speed;
        }
        if delta.voltage.is_some() {
            self.voltage = delta.voltage;
        }
        if delta.battery_current.is_some() {
            self.battery_current = delta.battery_current;
        }
        if delta.motor_current.is_some() {
            self.motor_current = delta.motor_current;
        }
        if delta.power.is_some() {
            self.power = delta.power;
        }
        if delta.controller_temperature.is_some() {
            self.controller_temperature = delta.controller_temperature;
        }
        if delta.motor_temperature.is_some() {
            self.motor_temperature = delta.motor_temperature;
        }
        if delta.battery_temperature.is_some() {
            self.battery_temperature = delta.battery_temperature;
        }
        if delta.pwm.is_some() {
            self.pwm = delta.pwm;
        }
        if delta.distance.is_some() {
            self.distance = delta.distance;
        }
        if delta.pitch.is_some() {
            self.pitch = delta.pitch;
        }
        if delta.roll.is_some() {
            self.roll = delta.roll;
        }
        if delta.battery_level_reported.is_some() {
            self.battery_level_reported = delta.battery_level_reported;
        }
        if delta.battery_level_estimated.is_some() {
            self.battery_level_estimated = delta.battery_level_estimated;
        }
    }
}
