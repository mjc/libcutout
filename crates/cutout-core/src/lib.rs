#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Core types and setup scaffolding for Cutout.

use std::{cmp::Ordering, fmt, marker::PhantomData, ops::RangeInclusive};

use arrayvec::ArrayVec;
use thiserror::Error;
use uuid::Uuid;

mod pevcap;
pub use pevcap::*;
mod battery_page;
pub use battery_page::*;
mod ffi;
pub use ffi::*;
mod session_state;
pub use session_state::*;
mod ride_lifecycle;
pub use ride_lifecycle::*;
mod energy_estimate;
pub use energy_estimate::*;
mod settings;
pub use settings::*;

#[cfg(test)]
mod gatt_channel_tests;

/// Monotonic timestamp supplied by the host.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicTimestamp(u64);

impl MonotonicTimestamp {
    /// Creates a monotonic timestamp from milliseconds.
    #[must_use]
    pub const fn from_milliseconds(value: u64) -> Self {
        Self(value)
    }

    /// Creates a monotonic timestamp from milliseconds.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self::from_milliseconds(value)
    }

    /// Returns the timestamp as milliseconds.
    #[must_use]
    pub const fn as_milliseconds(self) -> u64 {
        self.0
    }

    /// Returns the timestamp as milliseconds.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.as_milliseconds()
    }

    /// Adds a duration to this timestamp, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_add_duration(self, duration: Duration) -> Self {
        Self::from_milliseconds(self.0.saturating_add(duration.as_milliseconds()))
    }

    /// Returns the elapsed duration between this timestamp and an earlier one.
    #[must_use]
    pub const fn saturating_duration_since(self, earlier: Self) -> Duration {
        Duration::from_milliseconds(self.0.saturating_sub(earlier.0))
    }
}

impl fmt::Display for MonotonicTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Wall-clock timestamp represented as Unix epoch milliseconds.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WallClockUnixTimestamp(u64);

impl WallClockUnixTimestamp {
    /// Creates a wall-clock timestamp from Unix epoch milliseconds.
    #[must_use]
    pub const fn from_milliseconds(value: u64) -> Self {
        Self(value)
    }

    /// Creates a wall-clock timestamp from Unix epoch milliseconds.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self::from_milliseconds(value)
    }

    /// Returns the timestamp as Unix epoch milliseconds.
    #[must_use]
    pub const fn as_milliseconds(self) -> u64 {
        self.0
    }

    /// Returns the timestamp as Unix epoch milliseconds.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.as_milliseconds()
    }
}

impl fmt::Display for WallClockUnixTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Maximum payload bytes accepted for a single GATT write value.
pub const MAX_TRANSPORT_WRITE_LEN: usize = 512;

/// Payload bytes stored inline before falling back to an explicit large write.
pub const MAX_INLINE_TRANSPORT_WRITE_LEN: usize = 32;

/// Maximum payload accepted by a transport write.
pub type TransportWriteLimit = Quantity<Information, Byte, u16>;

/// Transport-independent identifier for a GATT characteristic or endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GattChannel(Uuid);

impl GattChannel {
    /// Creates a channel identifier from its 16-byte representation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Creates a channel identifier from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the channel identifier as raw bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 16] {
        *self.0.as_bytes()
    }

    /// Returns the channel identifier as a UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Host-observed link details supplied when a transport connects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkInfo {
    /// Host monotonic connection timestamp.
    pub monotonic_ms: MonotonicTimestamp,

    /// Maximum write payload length reported by the host, when known.
    pub max_write_len: Option<TransportWriteLimit>,
}

/// Documented pedal stiffness setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PedalMode {
    /// Firm pedal response.
    Hard,

    /// Mid-range pedal response.
    Medium,

    /// Soft pedal response.
    Soft,
}

impl PedalMode {
    /// Decodes the documented Veteran/NOSFET pedal-mode field.
    #[must_use]
    pub const fn from_veteran_raw(raw: u16) -> Option<Self> {
        match raw {
            0 => Some(Self::Hard),
            1 => Some(Self::Medium),
            2 => Some(Self::Soft),
            _ => None,
        }
    }

    /// Decodes the documented Begode Live-B pedal-mode bitfield.
    #[must_use]
    pub const fn from_begode_settings_bits(raw: u16) -> Option<Self> {
        match (raw >> 13) & 0x03 {
            0 => Some(Self::Soft),
            1 => Some(Self::Medium),
            2 => Some(Self::Hard),
            _ => None,
        }
    }
}

/// Command requested by the host application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceCommand {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request a telemetry update.
    RequestTelemetry,

    /// Request firmware or protocol version information.
    RequestFirmwareInfo,

    /// Request battery or BMS information.
    RequestBatteryInfo,

    /// Request device diagnostics.
    RequestDiagnostics,

    /// Request historical fault information.
    RequestFaultHistory,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights(LightState),

    /// Set pedal stiffness; this command is stationary-only.
    SetPedalMode(PedalMode),

    /// Enable or disable acceleration assist; this command is stationary-only.
    SetAccelerationAssist(AccelerationAssistState),

    /// Set the taillight state independently of the existing light control.
    SetTaillight(LightState),

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current in milliamps.
    SetRawMotorCurrent {
        /// Target motor/phase current in milliamps.
        current: PhaseCurrent,
    },
}

impl DeviceCommand {
    /// Returns the stable command kind, excluding command payload values.
    #[must_use]
    pub const fn kind(self) -> CommandKind {
        match self {
            Self::RequestIdentity => CommandKind::RequestIdentity,
            Self::RequestTelemetry => CommandKind::RequestTelemetry,
            Self::RequestFirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::RequestBatteryInfo => CommandKind::RequestBatteryInfo,
            Self::RequestDiagnostics => CommandKind::RequestDiagnostics,
            Self::RequestFaultHistory => CommandKind::RequestFaultHistory,
            Self::RequestSettings => CommandKind::RequestSettings,
            Self::SetLights(_) => CommandKind::SetLights,
            Self::SetPedalMode(_) => CommandKind::SetPedalMode,
            Self::SetAccelerationAssist(_) => CommandKind::SetAccelerationAssist,
            Self::SetTaillight(_) => CommandKind::SetTaillight,
            Self::SoundHorn => CommandKind::SoundHorn,
            Self::SetRawMotorCurrent { .. } => CommandKind::SetRawMotorCurrent,
        }
    }

    /// Returns the safety class for this command.
    #[must_use]
    pub const fn safety_class(self) -> SafetyClass {
        self.kind().safety_class()
    }

    /// Returns command metadata.
    #[must_use]
    pub const fn metadata(self) -> CommandMetadata {
        CommandMetadata {
            kind: self.kind(),
            safety_class: self.safety_class(),
        }
    }
}

/// Device light state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightState {
    /// Lights off.
    Off,

    /// Lights on.
    On,
}

/// User-facing acceleration-assist state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccelerationAssistState {
    /// Acceleration assist is disabled.
    Disabled,

    /// Acceleration assist is enabled.
    Enabled,
}

/// Stable command discriminator, excluding command payload values.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CommandKind {
    /// Request protocol or device identity.
    RequestIdentity,

    /// Request a telemetry update.
    RequestTelemetry,

    /// Request firmware or protocol version information.
    RequestFirmwareInfo,

    /// Request battery or BMS information.
    RequestBatteryInfo,

    /// Request device diagnostics.
    RequestDiagnostics,

    /// Request historical fault information.
    RequestFaultHistory,

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights,

    /// Set pedal stiffness.
    SetPedalMode,

    /// Enable or disable acceleration assist.
    SetAccelerationAssist,

    /// Set the taillight state.
    SetTaillight,

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current.
    SetRawMotorCurrent,
}

impl CommandKind {
    /// Returns the safety class for this command kind.
    #[must_use]
    pub const fn safety_class(self) -> SafetyClass {
        match self {
            Self::RequestIdentity
            | Self::RequestTelemetry
            | Self::RequestFirmwareInfo
            | Self::RequestBatteryInfo
            | Self::RequestDiagnostics
            | Self::RequestFaultHistory
            | Self::RequestSettings => SafetyClass::ReadOnly,
            Self::SetLights | Self::SetTaillight | Self::SoundHorn => SafetyClass::BenignControl,
            Self::SetPedalMode | Self::SetAccelerationAssist => SafetyClass::StationaryOnly,
            Self::SetRawMotorCurrent => SafetyClass::Actuation,
        }
    }
}

/// Safety class for a device command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyClass {
    /// Read-only request with no state change expected.
    ReadOnly,

    /// Benign control such as lights or horn.
    BenignControl,

    /// Setting that should only be changed while stationary.
    StationaryOnly,

    /// Direct actuation or motion-affecting control.
    Actuation,

    /// Firmware update or firmware mutation operation.
    Firmware,
}

/// Command metadata available before transport writes are generated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandMetadata {
    /// Stable command kind.
    pub kind: CommandKind,

    /// Safety class for this command.
    pub safety_class: SafetyClass,
}

/// Reason a command is unavailable in the current context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedReason {
    /// The command kind is not reported as supported.
    CommandNotSupported(CommandKind),
}

/// Short-lived authorization token for dangerous actuation commands.
///
/// Arm tokens are issued by [`DangerousActuationPolicy::arm`]. External
/// callers can inspect the derived model and expiry values, but cannot forge a
/// valid-looking token with a struct literal.
///
/// ```compile_fail
/// # use cutout_core::{DangerousActuationArm, MonotonicTimestamp};
/// let _forged = DangerousActuationArm {
///     model: "Begode Falcon",
///     expires_at_ms: MonotonicTimestamp::new(u64::MAX),
/// };
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DangerousActuationArm {
    model: &'static str,
    expires_at_ms: MonotonicTimestamp,
}

impl DangerousActuationArm {
    /// Returns the model this token was issued for.
    #[must_use]
    pub const fn model(self) -> &'static str {
        self.model
    }

    /// Returns the monotonic expiry time for this token.
    #[must_use]
    pub const fn expires_at_ms(self) -> MonotonicTimestamp {
        self.expires_at_ms
    }

    const fn is_for_model(self, model: &str) -> bool {
        str_eq(self.model, model)
    }

    const fn is_expired_at(self, monotonic_ms: MonotonicTimestamp) -> bool {
        monotonic_ms.get() > self.expires_at_ms.get()
    }
}

/// Dangerous actuation policy for a single model/session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DangerousActuationPolicy {
    /// Model this policy allows.
    pub model: &'static str,

    /// Maximum absolute motor/phase current allowed by this policy.
    pub max_current: PhaseCurrent,

    /// Duration of newly issued arming tokens.
    pub arm_duration: Duration,
}

impl DangerousActuationPolicy {
    /// Creates an expiring arm token for this policy's model.
    #[must_use]
    pub const fn arm(self, monotonic_ms: MonotonicTimestamp) -> DangerousActuationArm {
        DangerousActuationArm {
            model: self.model,
            expires_at_ms: monotonic_ms.saturating_add_duration(self.arm_duration),
        }
    }

    /// Authorizes a dangerous actuation command if the policy and token allow it.
    ///
    /// # Errors
    ///
    /// Returns [`DangerousActuationRefusal`] when the command is not dangerous
    /// actuation, the token is missing/expired/wrong-model, or the requested
    /// current exceeds this policy's absolute limit.
    pub const fn authorize(
        self,
        command: DeviceCommand,
        monotonic_ms: MonotonicTimestamp,
        arm: Option<DangerousActuationArm>,
    ) -> Result<CommandMetadata, DangerousActuationRefusal> {
        if !matches!(command.safety_class(), SafetyClass::Actuation) {
            return Err(DangerousActuationRefusal::WrongSafetyClass);
        }

        let Some(arm) = arm else {
            return Err(DangerousActuationRefusal::MissingArm);
        };

        if !arm.is_for_model(self.model) {
            return Err(DangerousActuationRefusal::WrongModel);
        }
        if arm.is_expired_at(monotonic_ms) {
            return Err(DangerousActuationRefusal::ExpiredArm);
        }
        if let DeviceCommand::SetRawMotorCurrent { current } = command
            && current.as_milliamps().saturating_abs() > self.max_current.as_milliamps()
        {
            return Err(DangerousActuationRefusal::CurrentLimitExceeded);
        }

        Ok(command.metadata())
    }
}

/// Short-lived authorization for a settings write while the vehicle is stationary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StationarySettingsArm {
    model: &'static str,
    expires_at_ms: MonotonicTimestamp,
}

impl StationarySettingsArm {
    /// Returns the model this token was issued for.
    #[must_use]
    pub const fn model(self) -> &'static str {
        self.model
    }

    /// Returns the monotonic expiry timestamp for this token.
    #[must_use]
    pub const fn expires_at_ms(self) -> MonotonicTimestamp {
        self.expires_at_ms
    }

    /// Returns whether this token is still valid for the model and timestamp.
    #[must_use]
    pub const fn is_valid_for(self, model: &str, monotonic_ms: MonotonicTimestamp) -> bool {
        str_eq(self.model, model) && monotonic_ms.get() <= self.expires_at_ms.get()
    }
}

/// Policy for issuing a short-lived stationary settings authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StationarySettingsPolicy {
    /// Model this policy allows.
    pub model: &'static str,

    /// Duration of newly issued authorizations.
    pub arm_duration: Duration,
}

impl StationarySettingsPolicy {
    /// Issues an authorization only from an explicitly stationary ride state.
    #[must_use]
    pub const fn arm(
        self,
        state: RideOperatingState,
        monotonic_ms: MonotonicTimestamp,
    ) -> Option<StationarySettingsArm> {
        match state {
            RideOperatingState::Parked | RideOperatingState::Standing => {
                Some(StationarySettingsArm {
                    model: self.model,
                    expires_at_ms: monotonic_ms.saturating_add_duration(self.arm_duration),
                })
            }
            RideOperatingState::Unknown
            | RideOperatingState::Riding
            | RideOperatingState::Charging => None,
        }
    }
}

/// Refusal reason for dangerous actuation authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DangerousActuationRefusal {
    /// Command is not classified as dangerous actuation.
    WrongSafetyClass,

    /// No arm token was supplied.
    MissingArm,

    /// Arm token was issued for another model.
    WrongModel,

    /// Arm token has expired.
    ExpiredArm,

    /// Requested current exceeds the policy limit.
    CurrentLimitExceeded,
}

/// Host-facing control refusal details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlRefusal {
    /// Command that was refused.
    pub command: CommandKind,

    /// Safety class of the refused command.
    pub safety_class: SafetyClass,

    /// Refusal reason.
    pub reason: ControlRefusalReason,
}

/// Reason a control command was refused before transport writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRefusalReason {
    /// Command is not classified for this control shell.
    WrongSafetyClass,

    /// No required arming token was supplied.
    MissingArm,

    /// Arming token was issued for another model.
    WrongModel,

    /// Arming token has expired.
    ExpiredArm,

    /// Requested value exceeds the configured current limit.
    CurrentLimitExceeded,

    /// Command is not supported by this model/session.
    UnsupportedCommand,
}

impl From<DangerousActuationRefusal> for ControlRefusalReason {
    fn from(value: DangerousActuationRefusal) -> Self {
        match value {
            DangerousActuationRefusal::WrongSafetyClass => Self::WrongSafetyClass,
            DangerousActuationRefusal::MissingArm => Self::MissingArm,
            DangerousActuationRefusal::WrongModel => Self::WrongModel,
            DangerousActuationRefusal::ExpiredArm => Self::ExpiredArm,
            DangerousActuationRefusal::CurrentLimitExceeded => Self::CurrentLimitExceeded,
        }
    }
}

const fn str_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }

    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Protocol family identifier used by registry data.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProtocolFamily {
    /// Veteran/LeaperKim/NOSFET `dc5a5c` frame family.
    VeteranLeaperkimNosfet,

    /// Begode/Gotway `55aa` frame family.
    BegodeGotway,

    /// VESC UART/CAN-derived family used by Refloat-style controllers.
    Vesc,
}

/// Verification state for registry fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationStatus {
    /// Not yet verified.
    Unverified,

    /// Inferred from partial evidence.
    Inferred,

    /// Verified against source-attributed protocol documentation.
    SourceVerified,

    /// Verified against actual Bluetooth hardware.
    HardwareVerified,

    /// Verified against both source-attributed documentation and hardware.
    SourceAndHardwareVerified,
}

/// A registry value plus its verification state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedValue<T> {
    /// Data value.
    pub value: T,

    /// Verification status for this value.
    pub verification: VerificationStatus,
}

/// Battery metadata for a registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatterySpec {
    /// Series cell count.
    pub series_cells: SeriesCount,

    /// Nominal pack capacity, when known.
    pub nominal_capacity: Option<Capacity>,

    /// Expected pack voltage range.
    pub voltage_range: RangeInclusive<Voltage>,

    /// Verification status for the battery metadata.
    pub verification: VerificationStatus,
}

/// Static BMS selector interpretation for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsPageSelectorSpec {
    /// BMS page selector value.
    pub selector: ProtocolSelector,

    /// Current interpretation of the selector.
    pub kind: BatteryPageKind,

    /// Verification status for this selector interpretation.
    pub verification: VerificationStatus,
}

/// Static BMS layout metadata for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsLayoutSpec {
    /// Series-connected cell count covered by this BMS layout.
    pub series_cells: SeriesCount,

    /// Parallel pack count for this model.
    pub parallel_packs: ParallelCount,

    /// Cell-voltage values decoded from a full cell-voltage page.
    pub cell_values_per_page: BmsCellValuesPerPage,

    /// Temperature values decoded from a full temperature page.
    pub temperature_values_per_page: BmsTemperatureValuesPerPage,

    /// Static selector interpretation table.
    pub selectors: &'static [BmsPageSelectorSpec],

    /// Verification status for the layout geometry.
    pub verification: VerificationStatus,
}

/// Observed roles for a GATT characteristic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GattRoles(u8);

impl GattRoles {
    const READ: u8 = 1 << 0;
    const WRITE: u8 = 1 << 1;
    const WRITE_WITHOUT_RESPONSE: u8 = 1 << 2;
    const NOTIFY: u8 = 1 << 3;
    const INDICATE: u8 = 1 << 4;

    /// Empty role set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns whether no roles are set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Adds read support.
    #[must_use]
    pub const fn with_read(self) -> Self {
        Self(self.0 | Self::READ)
    }

    /// Adds write-with-response support.
    #[must_use]
    pub const fn with_write(self) -> Self {
        Self(self.0 | Self::WRITE)
    }

    /// Adds write-without-response support.
    #[must_use]
    pub const fn with_write_without_response(self) -> Self {
        Self(self.0 | Self::WRITE_WITHOUT_RESPONSE)
    }

    /// Adds notification support.
    #[must_use]
    pub const fn with_notify(self) -> Self {
        Self(self.0 | Self::NOTIFY)
    }

    /// Adds indication support.
    #[must_use]
    pub const fn with_indicate(self) -> Self {
        Self(self.0 | Self::INDICATE)
    }

    /// Returns whether read is supported.
    #[must_use]
    pub const fn supports_read(self) -> bool {
        self.0 & Self::READ != 0
    }

    /// Returns whether write with response is supported.
    #[must_use]
    pub const fn supports_write(self) -> bool {
        self.0 & Self::WRITE != 0
    }

    /// Returns whether write without response is supported.
    #[must_use]
    pub const fn supports_write_without_response(self) -> bool {
        self.0 & Self::WRITE_WITHOUT_RESPONSE != 0
    }

    /// Returns whether notify is supported.
    #[must_use]
    pub const fn supports_notify(self) -> bool {
        self.0 & Self::NOTIFY != 0
    }

    /// Returns whether indicate is supported.
    #[must_use]
    pub const fn supports_indicate(self) -> bool {
        self.0 & Self::INDICATE != 0
    }
}

/// GATT service/characteristic fingerprint for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GattFingerprint {
    /// Observed service UUID.
    pub service: GattChannel,

    /// Observed characteristic UUID.
    pub characteristic: GattChannel,

    /// Observed characteristic roles.
    pub roles: GattRoles,

    /// Verification status for this fingerprint.
    pub verification: VerificationStatus,
}

/// Platform namespace for an installed-device identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstalledDevicePlatform {
    /// Apple `CoreBluetooth` peripheral identifier.
    CoreBluetooth,

    /// Android Bluetooth stack identifier.
    Android,

    /// Other host platform namespace.
    Other,
}

/// Opaque platform-scoped identifier for a remembered device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstalledDevicePlatformId<'a> {
    /// Platform namespace for this identifier.
    pub platform: InstalledDevicePlatform,

    /// Opaque identifier value as reported by the platform.
    pub value: &'a str,
}

/// Protocol-reported device serial number.
pub type ProtocolSerial<'a> = VerifiedValue<&'a str>;

/// Resolved installed-device model identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstalledDeviceModel<'a> {
    /// Resolved manufacturer or brand.
    pub manufacturer: &'a str,

    /// Resolved model name.
    pub model: &'a str,

    /// Resolved protocol family.
    pub protocol_family: ProtocolFamily,

    /// Verification status for this model resolution.
    pub verification: VerificationStatus,
}

/// Persistable installed-device identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledDeviceIdentity<'a> {
    /// Platform-scoped primary identifier. This is opaque and must not be
    /// assumed to be a stable public Bluetooth MAC address.
    pub platform_id: InstalledDevicePlatformId<'a>,

    /// Optional protocol-reported serial number.
    pub protocol_serial: Option<ProtocolSerial<'a>>,

    /// Optional user-facing alias.
    pub user_alias: Option<&'a str>,

    /// Optional resolved model identity.
    pub resolved_model: Option<InstalledDeviceModel<'a>>,

    /// Observed model/GATT fingerprints.
    pub gatt_fingerprints: &'a [GattFingerprint],
}

/// Data-only model registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRegistryEntry {
    /// Manufacturer or brand.
    pub manufacturer: ManufacturerKey,

    /// Model name.
    pub model: ModelKey,

    /// Protocol family.
    pub protocol_family: ProtocolFamily,

    /// Advertised-name hints. These are hints only, not identity truth.
    pub advertised_name_hints: &'static [&'static str],

    /// Passive wire model id when known.
    pub wire_model_id: Option<VerifiedValue<u16>>,

    /// Battery metadata when known.
    pub battery: Option<BatterySpec>,

    /// BMS layout metadata when known.
    pub bms: Option<BmsLayoutSpec>,

    /// Observed GATT fingerprints.
    pub gatt: &'static [GattFingerprint],

    /// Supported command capabilities.
    pub capabilities: Capabilities,

    /// Overall entry verification status.
    pub verification: VerificationStatus,
}

/// Stable manufacturer key used by catalog lookup and validation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ManufacturerKey(&'static str);

impl ManufacturerKey {
    /// Builds a manufacturer key from static registry data.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl core::ops::Deref for ManufacturerKey {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl PartialEq<&str> for ManufacturerKey {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ManufacturerKey> for &str {
    fn eq(&self, other: &ManufacturerKey) -> bool {
        *self == other.as_str()
    }
}

impl core::fmt::Display for ManufacturerKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable model key used by catalog lookup and validation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ModelKey(&'static str);

impl ModelKey {
    /// Builds a model key from static registry data.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl core::ops::Deref for ModelKey {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl PartialEq<&str> for ModelKey {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ModelKey> for &str {
    fn eq(&self, other: &ModelKey) -> bool {
        *self == other.as_str()
    }
}

impl core::fmt::Display for ModelKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable protocol-family key used by catalog lookup and validation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FamilyKey(ProtocolFamily);

impl FamilyKey {
    /// Builds a family key.
    #[must_use]
    pub const fn new(value: ProtocolFamily) -> Self {
        Self(value)
    }

    /// Returns the protocol family.
    #[must_use]
    pub const fn protocol_family(self) -> ProtocolFamily {
        self.0
    }
}

/// Opaque parser registration key for a registered model.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ParserKey(&'static str);

impl ParserKey {
    /// Builds a parser registration key.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Opaque session registration key for a registered model.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionKey(&'static str);

impl SessionKey {
    /// Builds a session registration key.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Returns the key text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Runtime registrations attached to an active catalog model.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModelRuntimeRegistration {
    /// Parser registration for model notifications/responses.
    pub parser: Option<ParserKey>,

    /// Session registration for model command/session handling.
    pub session: Option<SessionKey>,
}

impl ModelRuntimeRegistration {
    /// Builds an active parser/session registration pair.
    #[must_use]
    pub const fn active(parser: ParserKey, session: SessionKey) -> Self {
        Self {
            parser: Some(parser),
            session: Some(session),
        }
    }
}

/// Type-state marker for a missing required model-authoring field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MissingAuthoringField;

/// Type-state marker for a present required model-authoring field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PresentAuthoringField;

/// Type-state model authoring helper for static registry/catalog data.
///
/// This keeps the scalable model path as compile-time Rust data while making
/// the required fields explicit in the type signature. Optional metadata can be
/// layered in functionally, and only a fully-authored model can produce registry
/// and catalog entries.
#[derive(Clone, Debug)]
pub struct ModelAuthoring<
    Manufacturer = MissingAuthoringField,
    Model = MissingAuthoringField,
    Family = MissingAuthoringField,
    Gatt = MissingAuthoringField,
    CapabilitiesState = MissingAuthoringField,
    Runtime = MissingAuthoringField,
> {
    manufacturer: ManufacturerKey,
    model: ModelKey,
    family: FamilyKey,
    advertised_name_hints: &'static [&'static str],
    wire_model_id: Option<VerifiedValue<u16>>,
    battery: Option<BatterySpec>,
    bms: Option<BmsLayoutSpec>,
    gatt: &'static [GattFingerprint],
    capabilities: Capabilities,
    verification: VerificationStatus,
    runtime: ModelRuntimeRegistration,
    _state: PhantomData<(
        Manufacturer,
        Model,
        Family,
        Gatt,
        CapabilitiesState,
        Runtime,
    )>,
}

/// Fully-authored model state that can emit registry and catalog entries.
pub type CompleteModelAuthoring = ModelAuthoring<
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
    PresentAuthoringField,
>;

impl ModelAuthoring {
    /// Starts authoring a static model definition.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            manufacturer: ManufacturerKey::new(""),
            model: ModelKey::new(""),
            family: FamilyKey::new(ProtocolFamily::VeteranLeaperkimNosfet),
            advertised_name_hints: &[],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &[],
            capabilities: Capabilities::from_supported_commands([]),
            verification: VerificationStatus::Unverified,
            runtime: ModelRuntimeRegistration {
                parser: None,
                session: None,
            },
            _state: PhantomData,
        }
    }
}

impl Default for ModelAuthoring {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, N, F, G, C, R> ModelAuthoring<M, N, F, G, C, R> {
    /// Sets the manufacturer key.
    #[must_use]
    pub const fn manufacturer(
        self,
        manufacturer: ManufacturerKey,
    ) -> ModelAuthoring<PresentAuthoringField, N, F, G, C, R> {
        ModelAuthoring {
            manufacturer,
            model: self.model,
            family: self.family,
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt: self.gatt,
            capabilities: self.capabilities,
            verification: self.verification,
            runtime: self.runtime,
            _state: PhantomData,
        }
    }

    /// Sets the model key.
    #[must_use]
    pub const fn model(
        self,
        model: ModelKey,
    ) -> ModelAuthoring<M, PresentAuthoringField, F, G, C, R> {
        ModelAuthoring {
            manufacturer: self.manufacturer,
            model,
            family: self.family,
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt: self.gatt,
            capabilities: self.capabilities,
            verification: self.verification,
            runtime: self.runtime,
            _state: PhantomData,
        }
    }

    /// Sets the protocol family key.
    #[must_use]
    pub const fn family(
        self,
        family: FamilyKey,
    ) -> ModelAuthoring<M, N, PresentAuthoringField, G, C, R> {
        ModelAuthoring {
            manufacturer: self.manufacturer,
            model: self.model,
            family,
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt: self.gatt,
            capabilities: self.capabilities,
            verification: self.verification,
            runtime: self.runtime,
            _state: PhantomData,
        }
    }

    /// Sets advertised-name hints. These remain hints, not identity truth.
    #[must_use]
    pub const fn advertised_name_hints(self, hints: &'static [&'static str]) -> Self {
        Self {
            advertised_name_hints: hints,
            ..self
        }
    }

    /// Sets the passive wire model id.
    #[must_use]
    pub const fn wire_model_id(self, wire_model_id: VerifiedValue<u16>) -> Self {
        Self {
            wire_model_id: Some(wire_model_id),
            ..self
        }
    }

    /// Sets battery metadata.
    #[must_use]
    pub const fn battery(self, battery: BatterySpec) -> Self {
        Self {
            battery: Some(battery),
            ..self
        }
    }

    /// Sets BMS layout metadata.
    #[must_use]
    pub const fn bms(self, bms: BmsLayoutSpec) -> Self {
        Self {
            bms: Some(bms),
            ..self
        }
    }

    /// Sets observed GATT fingerprints.
    #[must_use]
    pub const fn gatt(
        self,
        gatt: &'static [GattFingerprint],
    ) -> ModelAuthoring<M, N, F, PresentAuthoringField, C, R> {
        ModelAuthoring {
            manufacturer: self.manufacturer,
            model: self.model,
            family: self.family,
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt,
            capabilities: self.capabilities,
            verification: self.verification,
            runtime: self.runtime,
            _state: PhantomData,
        }
    }

    /// Sets supported command capabilities.
    #[must_use]
    pub const fn capabilities(
        self,
        capabilities: Capabilities,
    ) -> ModelAuthoring<M, N, F, G, PresentAuthoringField, R> {
        ModelAuthoring {
            manufacturer: self.manufacturer,
            model: self.model,
            family: self.family,
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt: self.gatt,
            capabilities,
            verification: self.verification,
            runtime: self.runtime,
            _state: PhantomData,
        }
    }

    /// Sets the overall verification status.
    #[must_use]
    pub const fn verification(self, verification: VerificationStatus) -> Self {
        Self {
            verification,
            ..self
        }
    }

    /// Sets active parser and session runtime registrations.
    #[must_use]
    pub const fn active_runtime(
        self,
        parser: ParserKey,
        session: SessionKey,
    ) -> ModelAuthoring<M, N, F, G, C, PresentAuthoringField> {
        ModelAuthoring {
            manufacturer: self.manufacturer,
            model: self.model,
            family: self.family,
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt: self.gatt,
            capabilities: self.capabilities,
            verification: self.verification,
            runtime: ModelRuntimeRegistration::active(parser, session),
            _state: PhantomData,
        }
    }
}

impl CompleteModelAuthoring {
    /// Builds a data-only registry entry from a fully-authored model.
    #[must_use]
    pub const fn registry_entry(self) -> ModelRegistryEntry {
        ModelRegistryEntry {
            manufacturer: self.manufacturer,
            model: self.model,
            protocol_family: self.family.protocol_family(),
            advertised_name_hints: self.advertised_name_hints,
            wire_model_id: self.wire_model_id,
            battery: self.battery,
            bms: self.bms,
            gatt: self.gatt,
            capabilities: self.capabilities,
            verification: self.verification,
        }
    }

    /// Builds a catalog entry from a fully-authored model and static registry entry.
    #[must_use]
    pub const fn catalog_entry(self, registry: &'static ModelRegistryEntry) -> ModelCatalogEntry {
        ModelCatalogEntry::new(registry, self.runtime)
    }
}

/// Static catalog entry combining data-only metadata with runtime registration.
#[derive(Clone, Copy, Debug)]
pub struct ModelCatalogEntry {
    /// Data-only registry entry.
    pub registry: &'static ModelRegistryEntry,

    /// Runtime registrations used by hosts/protocol adapters.
    pub registration: ModelRuntimeRegistration,
}

impl ModelCatalogEntry {
    /// Builds a catalog entry from registry metadata and runtime registrations.
    #[must_use]
    pub const fn new(
        registry: &'static ModelRegistryEntry,
        registration: ModelRuntimeRegistration,
    ) -> Self {
        Self {
            registry,
            registration,
        }
    }

    /// Manufacturer key for this catalog entry.
    #[must_use]
    pub const fn manufacturer_key(self) -> ManufacturerKey {
        self.registry.manufacturer
    }

    /// Model key for this catalog entry.
    #[must_use]
    pub const fn model_key(self) -> ModelKey {
        self.registry.model
    }

    /// Family key for this catalog entry.
    #[must_use]
    pub const fn family_key(self) -> FamilyKey {
        FamilyKey::new(self.registry.protocol_family)
    }
}

/// Borrowed model catalog for allocation-free lookup over static entries.
#[derive(Clone, Copy, Debug)]
pub struct ModelCatalog<'a> {
    entries: &'a [ModelCatalogEntry],
}

impl<'a> ModelCatalog<'a> {
    /// Builds a borrowed model catalog.
    #[must_use]
    pub const fn new(entries: &'a [ModelCatalogEntry]) -> Self {
        Self { entries }
    }

    /// Returns the underlying catalog entries.
    #[must_use]
    pub const fn entries(self) -> &'a [ModelCatalogEntry] {
        self.entries
    }

    /// Finds an entry by typed manufacturer/model keys.
    #[must_use]
    pub fn find_model(
        self,
        manufacturer: ManufacturerKey,
        model: ModelKey,
    ) -> Option<&'a ModelCatalogEntry> {
        self.find_model_names(manufacturer.as_str(), model.as_str())
    }

    /// Finds an entry by borrowed manufacturer/model names.
    #[must_use]
    pub fn find_model_names(
        self,
        manufacturer: &str,
        model: &str,
    ) -> Option<&'a ModelCatalogEntry> {
        self.entries.iter().find(|entry| {
            entry.registry.manufacturer == manufacturer && entry.registry.model == model
        })
    }

    /// Resolves a display model name to a catalog entry within a protocol family.
    #[must_use]
    pub fn resolve_display_model(
        self,
        family: ProtocolFamily,
        display_model: &str,
    ) -> CatalogModelResolution<'a> {
        let mut matches = self
            .entries
            .iter()
            .filter(|entry| entry.registry.protocol_family == family)
            .filter(|entry| registry_entry_matches_display_model(entry.registry, display_model));
        let Some(first) = matches.next() else {
            return CatalogModelResolution::NoMatch;
        };
        if matches.next().is_some() {
            CatalogModelResolution::Ambiguous
        } else {
            CatalogModelResolution::Matched(first)
        }
    }

    /// Resolves a BLE advertised name against catalog model hints.
    #[must_use]
    pub fn resolve_advertised_name(self, name: &str) -> CatalogModelResolution<'a> {
        let mut matches = self
            .entries
            .iter()
            .filter(|entry| registry_entry_matches_advertised_name(entry.registry, name));
        let Some(first) = matches.next() else {
            return CatalogModelResolution::NoMatch;
        };
        if matches.next().is_some() {
            CatalogModelResolution::Ambiguous
        } else {
            CatalogModelResolution::Matched(first)
        }
    }

    /// Finds the first catalog entry registered for a parser key.
    #[must_use]
    pub fn find_parser(self, parser: ParserKey) -> Option<&'a ModelCatalogEntry> {
        self.entries
            .iter()
            .find(|entry| entry.registration.parser == Some(parser))
    }

    /// Finds the first catalog entry registered for a session key.
    #[must_use]
    pub fn find_session(self, session: SessionKey) -> Option<&'a ModelCatalogEntry> {
        self.entries
            .iter()
            .find(|entry| entry.registration.session == Some(session))
    }

    /// Iterates entries for a protocol family without allocating.
    pub fn family_entries(
        self,
        family: FamilyKey,
    ) -> impl Clone + Iterator<Item = &'a ModelCatalogEntry> {
        self.entries
            .iter()
            .filter(move |entry| entry.family_key() == family)
    }
}

/// Result of resolving identity metadata against a model catalog.
#[derive(Clone, Copy, Debug)]
pub enum CatalogModelResolution<'a> {
    /// Exactly one catalog entry matched.
    Matched(&'a ModelCatalogEntry),

    /// No catalog entry matched.
    NoMatch,

    /// More than one catalog entry matched.
    Ambiguous,
}

fn registry_entry_matches_display_model(entry: &ModelRegistryEntry, display_model: &str) -> bool {
    entry.model == display_model
        || display_model
            .strip_prefix(entry.manufacturer.as_str())
            .and_then(|suffix| suffix.strip_prefix(' '))
            == Some(entry.model.as_str())
}

fn registry_entry_matches_advertised_name(entry: &ModelRegistryEntry, name: &str) -> bool {
    contains_ascii_ignore_case(name, entry.manufacturer.as_str())
        || contains_ascii_ignore_case(name, entry.model.as_str())
        || entry
            .advertised_name_hints
            .iter()
            .copied()
            .any(|hint| contains_ascii_ignore_case(name, hint))
}

fn contains_ascii_ignore_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();

    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| ascii_eq_ignore_case(window, needle))
}

fn ascii_eq_ignore_case(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

/// Registry data validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RegistryValidationError {
    /// Registry entry has an empty manufacturer.
    #[error("registry entry at index {index} has an empty manufacturer")]
    EmptyManufacturer {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Registry entry has an empty model.
    #[error("registry entry at index {index} has an empty model")]
    EmptyModel {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Registry entry duplicates an earlier manufacturer/model key.
    #[error("registry entry at index {index} duplicates entry at index {first_index}")]
    DuplicateModel {
        /// Duplicate entry index.
        index: usize,

        /// First entry index with the same manufacturer/model key.
        first_index: usize,
    },

    /// Registry entry duplicates a wire model id in the same protocol family.
    #[error(
        "registry entry at index {index} conflicts with entry at index {first_index} for a protocol wire model id"
    )]
    ConflictingWireModelId {
        /// Conflicting entry index.
        index: usize,

        /// First entry index with the same family and wire model id.
        first_index: usize,
    },

    /// Registry entry has no observed GATT fingerprints.
    #[error("registry entry at index {index} has no GATT fingerprints")]
    MissingGattFingerprint {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Registry entry has a GATT fingerprint with no characteristic roles.
    #[error(
        "registry entry at index {index} has invalid GATT fingerprint at index {fingerprint_index}"
    )]
    InvalidGattFingerprint {
        /// Entry index in the validated slice.
        index: usize,

        /// GATT fingerprint index in the entry.
        fingerprint_index: usize,
    },

    /// Registry entry exposes no supported commands.
    #[error("registry entry at index {index} exposes no command capabilities")]
    EmptyCapabilities {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Active catalog entry has no parser registration.
    #[error("catalog entry at index {index} has active capabilities but no parser registration")]
    MissingParserRegistration {
        /// Entry index in the validated slice.
        index: usize,
    },

    /// Active catalog entry has no session registration.
    #[error("catalog entry at index {index} has active capabilities but no session registration")]
    MissingSessionRegistration {
        /// Entry index in the validated slice.
        index: usize,
    },
}

/// Validates registry entries as data before they are bundled, hashed, or used
/// for model identification.
///
/// # Errors
///
/// Returns [`RegistryValidationError`] for the first structural inconsistency
/// found in the supplied entries.
pub fn validate_registry_entries(
    entries: &[&ModelRegistryEntry],
) -> Result<(), RegistryValidationError> {
    for (index, entry) in entries.iter().enumerate() {
        validate_registry_entry(index, entry)?;
        if let Some(first_index) = first_duplicate_model_index(entries, index, entry) {
            return Err(RegistryValidationError::DuplicateModel { index, first_index });
        }
        if let Some(first_index) = first_conflicting_wire_model_id_index(entries, index, entry) {
            return Err(RegistryValidationError::ConflictingWireModelId { index, first_index });
        }
    }
    Ok(())
}

/// Validates catalog entries before hosts use registry metadata or factories.
///
/// # Errors
///
/// Returns [`RegistryValidationError`] for the first structural inconsistency
/// found in the supplied entries.
pub fn validate_model_catalog(
    entries: &[ModelCatalogEntry],
) -> Result<(), RegistryValidationError> {
    for (index, entry) in entries.iter().enumerate() {
        validate_registry_entry(index, entry.registry)?;
        if entry.registration.parser.is_none() {
            return Err(RegistryValidationError::MissingParserRegistration { index });
        }
        if entry.registration.session.is_none() {
            return Err(RegistryValidationError::MissingSessionRegistration { index });
        }
        if let Some(first_index) = first_duplicate_catalog_model_index(entries, index, entry) {
            return Err(RegistryValidationError::DuplicateModel { index, first_index });
        }
        if let Some(first_index) =
            first_conflicting_catalog_wire_model_id_index(entries, index, entry)
        {
            return Err(RegistryValidationError::ConflictingWireModelId { index, first_index });
        }
    }
    Ok(())
}

/// Deterministic fingerprint for a registry snapshot.
///
/// This is intended for capture provenance and replay compatibility checks. It
/// is not a cryptographic authenticity mechanism.
#[must_use]
pub fn registry_entries_hash(entries: &[&ModelRegistryEntry]) -> [u8; 32] {
    let mut hasher = RegistryHashBuilder::new();
    hasher.write_bytes(b"cutout-registry-v1");
    hasher.write_usize(entries.len());
    for entry in entries {
        hasher.write_registry_entry(entry);
    }
    hasher.finish()
}

fn validate_registry_entry(
    index: usize,
    entry: &ModelRegistryEntry,
) -> Result<(), RegistryValidationError> {
    if entry.manufacturer.as_str().is_empty() {
        return Err(RegistryValidationError::EmptyManufacturer { index });
    }
    if entry.model.as_str().is_empty() {
        return Err(RegistryValidationError::EmptyModel { index });
    }
    if entry.gatt.is_empty() {
        return Err(RegistryValidationError::MissingGattFingerprint { index });
    }
    if let Some(fingerprint_index) = first_invalid_gatt_fingerprint_index(entry.gatt) {
        return Err(RegistryValidationError::InvalidGattFingerprint {
            index,
            fingerprint_index,
        });
    }
    if capabilities_are_empty(entry.capabilities) {
        return Err(RegistryValidationError::EmptyCapabilities { index });
    }
    Ok(())
}

fn first_duplicate_model_index(
    entries: &[&ModelRegistryEntry],
    index: usize,
    entry: &ModelRegistryEntry,
) -> Option<usize> {
    entries[..index].iter().position(|candidate| {
        candidate.manufacturer == entry.manufacturer && candidate.model == entry.model
    })
}

fn first_conflicting_wire_model_id_index(
    entries: &[&ModelRegistryEntry],
    index: usize,
    entry: &ModelRegistryEntry,
) -> Option<usize> {
    let wire_model_id = entry.wire_model_id?.value;
    entries[..index].iter().position(|candidate| {
        candidate.protocol_family == entry.protocol_family
            && candidate
                .wire_model_id
                .is_some_and(|candidate_id| candidate_id.value == wire_model_id)
    })
}

fn first_duplicate_catalog_model_index(
    entries: &[ModelCatalogEntry],
    index: usize,
    entry: &ModelCatalogEntry,
) -> Option<usize> {
    entries[..index].iter().position(|candidate| {
        candidate.registry.manufacturer == entry.registry.manufacturer
            && candidate.registry.model == entry.registry.model
    })
}

fn first_conflicting_catalog_wire_model_id_index(
    entries: &[ModelCatalogEntry],
    index: usize,
    entry: &ModelCatalogEntry,
) -> Option<usize> {
    let wire_model_id = entry.registry.wire_model_id?.value;
    entries[..index].iter().position(|candidate| {
        candidate.registry.protocol_family == entry.registry.protocol_family
            && candidate
                .registry
                .wire_model_id
                .is_some_and(|candidate_id| candidate_id.value == wire_model_id)
    })
}

fn capabilities_are_empty(capabilities: Capabilities) -> bool {
    ALL_COMMAND_KINDS
        .iter()
        .all(|command| !capabilities.supports_command_kind(*command))
}

fn first_invalid_gatt_fingerprint_index(gatt: &[GattFingerprint]) -> Option<usize> {
    gatt.iter()
        .position(|fingerprint| fingerprint.roles.is_empty())
}

struct RegistryHashBuilder {
    lanes: [u64; 4],
}

impl RegistryHashBuilder {
    const fn new() -> Self {
        Self {
            lanes: [
                0xcbf2_9ce4_8422_2325,
                0x9e37_79b9_7f4a_7c15,
                0x517c_c1b7_2722_0a95,
                0x94d0_49bb_1331_11eb,
            ],
        }
    }

    fn finish(self) -> [u8; 32] {
        let mut output = [0u8; 32];
        for (index, lane) in self.lanes.into_iter().enumerate() {
            let start = index * 8;
            output[start..start + 8].copy_from_slice(&lane.to_le_bytes());
        }
        output
    }

    fn write_registry_entry(&mut self, entry: &ModelRegistryEntry) {
        self.write_str(entry.manufacturer.as_str());
        self.write_str(entry.model.as_str());
        self.write_u8(protocol_family_code(entry.protocol_family));
        self.write_strs(entry.advertised_name_hints);
        self.write_verified_u16(entry.wire_model_id);
        self.write_battery(entry.battery.as_ref());
        self.write_bms(entry.bms.as_ref());
        self.write_gatt(entry.gatt);
        self.write_capabilities(entry.capabilities);
        self.write_u8(verification_code(entry.verification));
    }

    fn write_strs(&mut self, values: &[&str]) {
        self.write_usize(values.len());
        for value in values {
            self.write_str(value);
        }
    }

    fn write_verified_u16(&mut self, value: Option<VerifiedValue<u16>>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_u16(value.value);
                self.write_u8(verification_code(value.verification));
            }
            None => self.write_u8(0),
        }
    }

    fn write_battery(&mut self, battery: Option<&BatterySpec>) {
        match battery {
            Some(battery) => {
                self.write_u8(1);
                self.write_u8(battery.series_cells.get());
                self.write_optional_u32(battery.nominal_capacity.map(Capacity::as_milliamp_hours));
                self.write_i32(battery.voltage_range.start().as_millivolts());
                self.write_i32(battery.voltage_range.end().as_millivolts());
                self.write_u8(verification_code(battery.verification));
            }
            None => self.write_u8(0),
        }
    }

    fn write_bms(&mut self, bms: Option<&BmsLayoutSpec>) {
        match bms {
            Some(bms) => {
                self.write_u8(1);
                self.write_u8(bms.series_cells.get());
                self.write_u8(bms.parallel_packs.get());
                self.write_u8(bms.cell_values_per_page.get());
                self.write_u8(bms.temperature_values_per_page.get());
                self.write_usize(bms.selectors.len());
                for selector in bms.selectors {
                    self.write_u8(selector.selector.get());
                    self.write_u8(battery_page_kind_code(selector.kind));
                    self.write_u8(verification_code(selector.verification));
                }
                self.write_u8(verification_code(bms.verification));
            }
            None => self.write_u8(0),
        }
    }

    fn write_gatt(&mut self, fingerprints: &[GattFingerprint]) {
        self.write_usize(fingerprints.len());
        for fingerprint in fingerprints {
            self.write_bytes(&fingerprint.service.as_bytes());
            self.write_bytes(&fingerprint.characteristic.as_bytes());
            self.write_u8(gatt_roles_code(fingerprint.roles));
            self.write_u8(verification_code(fingerprint.verification));
        }
    }

    fn write_capabilities(&mut self, capabilities: Capabilities) {
        for command in ALL_COMMAND_KINDS {
            self.write_u8(u8::from(capabilities.supports_command_kind(command)));
        }
    }

    fn write_optional_u32(&mut self, value: Option<u32>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_u32(value);
            }
            None => self.write_u8(0),
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_usize(value.len());
        self.write_bytes(value.as_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u16(&mut self, value: u16) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            for ((lane_index_u64, lane_index_u32), lane) in
                (0_u64..).zip(0_u32..).zip(self.lanes.iter_mut())
            {
                *lane ^= u64::from(*byte).wrapping_add(lane_index_u64 << 8);
                *lane = lane.wrapping_mul(0x0000_0100_0000_01b3 + lane_index_u64);
                *lane ^= lane.rotate_left(17 + lane_index_u32);
            }
        }
    }
}

const ALL_COMMAND_KINDS: [CommandKind; 10] = [
    CommandKind::RequestIdentity,
    CommandKind::RequestTelemetry,
    CommandKind::RequestFirmwareInfo,
    CommandKind::RequestBatteryInfo,
    CommandKind::RequestDiagnostics,
    CommandKind::RequestFaultHistory,
    CommandKind::RequestSettings,
    CommandKind::SetLights,
    CommandKind::SoundHorn,
    CommandKind::SetRawMotorCurrent,
];

const fn protocol_family_code(family: ProtocolFamily) -> u8 {
    match family {
        ProtocolFamily::VeteranLeaperkimNosfet => 1,
        ProtocolFamily::BegodeGotway => 2,
        ProtocolFamily::Vesc => 3,
    }
}

const fn verification_code(verification: VerificationStatus) -> u8 {
    match verification {
        VerificationStatus::Unverified => 0,
        VerificationStatus::Inferred => 1,
        VerificationStatus::SourceVerified => 2,
        VerificationStatus::HardwareVerified => 3,
        VerificationStatus::SourceAndHardwareVerified => 4,
    }
}

const fn gatt_roles_code(roles: GattRoles) -> u8 {
    let mut bits = 0u8;
    if roles.supports_read() {
        bits |= 1 << 0;
    }
    if roles.supports_write() {
        bits |= 1 << 1;
    }
    if roles.supports_write_without_response() {
        bits |= 1 << 2;
    }
    if roles.supports_notify() {
        bits |= 1 << 3;
    }
    if roles.supports_indicate() {
        bits |= 1 << 4;
    }
    bits
}

const fn battery_page_kind_code(kind: BatteryPageKind) -> u8 {
    match kind {
        BatteryPageKind::Metadata => 1,
        BatteryPageKind::CellVoltage => 2,
        BatteryPageKind::Temperature => 3,
        BatteryPageKind::Raw => 4,
    }
}

/// Current command capabilities for a resolved device/session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    supported_commands: CommandSet,
}

impl Capabilities {
    /// Creates capabilities from supported command kinds.
    #[must_use]
    pub const fn from_supported_commands<const N: usize>(commands: [CommandKind; N]) -> Self {
        Self {
            supported_commands: CommandSet::from_commands(commands),
        }
    }

    /// Returns whether the command kind is supported.
    #[must_use]
    pub const fn supports_command_kind(self, kind: CommandKind) -> bool {
        self.supported_commands.contains(kind)
    }

    /// Combines two capability sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            supported_commands: CommandSet(self.supported_commands.0 | other.supported_commands.0),
        }
    }

    /// Checks whether a command is supported and returns metadata for it.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedReason::CommandNotSupported`] when the command kind
    /// is absent from this capability set.
    pub const fn check_command(
        self,
        command: DeviceCommand,
    ) -> Result<CommandMetadata, UnsupportedReason> {
        let kind = command.kind();
        if self.supports_command_kind(kind) {
            Ok(command.metadata())
        } else {
            Err(UnsupportedReason::CommandNotSupported(kind))
        }
    }
}

/// Compact command-kind set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CommandSet(u16);

impl CommandSet {
    const fn from_commands<const N: usize>(commands: [CommandKind; N]) -> Self {
        let mut set = Self(0);
        let mut index = 0;
        while index < N {
            set = set.insert(commands[index]);
            index += 1;
        }
        set
    }

    const fn insert(self, kind: CommandKind) -> Self {
        Self(self.0 | kind.bit())
    }

    const fn contains(self, kind: CommandKind) -> bool {
        self.0 & kind.bit() != 0
    }
}

impl CommandKind {
    const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// Transport-independent parser resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParserLimits {
    /// Maximum accepted logical frame length in bytes.
    pub max_frame_len: ParserFrameLen,

    /// Maximum buffered input length in bytes before a parser should shed data.
    pub max_buffered_len: ParserBufferedLen,

    /// Maximum queued outputs a parser should retain before yielding to host code.
    pub max_queued_outputs: ParserQueuedOutputCount,

    /// Parser timeout threshold in host monotonic milliseconds.
    pub timeout_ms: MonotonicTimestamp,
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self {
            max_frame_len: ParserFrameLen::from_bytes(4_096),
            max_buffered_len: ParserBufferedLen::from_bytes(8_192),
            max_queued_outputs: ParserQueuedOutputCount::from_outputs(128),
            timeout_ms: MonotonicTimestamp::new(1_000),
        }
    }
}

impl ParserLimits {
    /// Validates that a claimed frame length is within the configured limit.
    ///
    /// # Errors
    ///
    /// Returns [`ParserError::OversizedFrame`] when `claimed` exceeds
    /// [`Self::max_frame_len`].
    pub const fn validate_frame_len(self, claimed: ParserFrameLen) -> Result<(), ParserError> {
        if claimed.is_at_most(self.max_frame_len) {
            Ok(())
        } else {
            Err(ParserError::OversizedFrame {
                claimed,
                max: self.max_frame_len,
            })
        }
    }
}

enum ProtocolSelectorUnit {}
enum ProtocolTagUnit {}
enum VescControllerIdUnit {}
enum BmsCellValuesPerPageUnit {}
enum BmsTemperatureValuesPerPageUnit {}
enum BmsPackIndexUnit {}
enum BmsHalfIndexUnit {}
enum BmsCellIndexUnit {}

macro_rules! typed_protocol_value {
    ($name:ident, $unit:ident, $inner:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name {
            value: $inner,
            _unit: PhantomData<fn() -> $unit>,
        }

        impl $name {
            /// Creates the typed protocol value.
            #[must_use]
            pub const fn new(value: $inner) -> Self {
                Self {
                    value,
                    _unit: PhantomData,
                }
            }

            /// Returns the underlying primitive value for FFI/rendering edges.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.value
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.value).finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.value.fmt(f)
            }
        }
    };
}

/// Bytes dropped while recovering from malformed or excessive parser input.
pub type ParserDroppedBytes = Quantity<Information, ParserDroppedByte, u64>;

impl ParserDroppedBytes {
    /// Creates a dropped parser byte count from bytes.
    #[must_use]
    pub const fn from_bytes(value: u64) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this dropped parser byte count in bytes.
    #[must_use]
    pub const fn as_bytes(self) -> u64 {
        self.unit_value()
    }

    /// Adds dropped bytes, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::from_bytes(self.as_bytes().saturating_add(other.as_bytes()))
    }
}

/// Saturating count for one class of parser diagnostic event.
pub type ParserDiagnosticCount = Quantity<Count, ParserDiagnosticEvent, u64>;

impl ParserDiagnosticCount {
    /// Creates a parser diagnostic count from event count.
    #[must_use]
    pub const fn from_events(value: u64) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this parser diagnostic count as event count.
    #[must_use]
    pub const fn as_events(self) -> u64 {
        self.unit_value()
    }

    /// Adds one diagnostic event, saturating at `u64::MAX`.
    #[must_use]
    pub const fn next(self) -> Self {
        Self::from_events(self.as_events().saturating_add(1))
    }

    /// Adds diagnostic events, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::from_events(self.as_events().saturating_add(other.as_events()))
    }
}

/// Size of one parser frame or claimed parser frame.
pub type ParserFrameLen = Quantity<Information, ParserFrameByte, usize>;

impl ParserFrameLen {
    /// Returns true when this frame length is less than or equal to another.
    #[must_use]
    pub const fn is_at_most(self, other: Self) -> bool {
        self.as_bytes() <= other.as_bytes()
    }
}

/// Maximum buffered parser input size.
pub type ParserBufferedLen = Quantity<Information, ParserBufferByte, usize>;

/// Maximum queued parser output count.
pub type ParserQueuedOutputCount = Quantity<Count, ParserQueuedOutput, usize>;

/// Default maximum outputs retained by whole-capture replay helpers.
pub const DEFAULT_REPLAY_OUTPUT_LIMIT: ParserQueuedOutputCount =
    ParserQueuedOutputCount::from_outputs(16_384);

impl ParserQueuedOutputCount {
    /// Creates a parser queued-output count from output count.
    #[must_use]
    pub const fn from_outputs(value: usize) -> Self {
        Self::from_unit_value(value)
    }

    /// Returns this parser queued-output count as output count.
    #[must_use]
    pub const fn as_outputs(self) -> usize {
        self.unit_value()
    }
}

/// Parser failure reason that can be counted without tying core to a protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParserError {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame {
        /// Claimed or observed frame length.
        claimed: ParserFrameLen,

        /// Configured maximum accepted frame length.
        max: ParserFrameLen,
    },

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before the expected data arrived.
    Timeout {
        /// Elapsed monotonic milliseconds.
        elapsed_ms: MonotonicTimestamp,

        /// Timeout threshold in monotonic milliseconds.
        timeout_ms: MonotonicTimestamp,
    },

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

/// Saturating parser diagnostics counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParserDiagnostics {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: ParserDroppedBytes,

    /// Parser resynchronization attempts.
    pub resyncs: ParserDiagnosticCount,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: ParserDiagnosticCount,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: ParserDiagnosticCount,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: ParserDiagnosticCount,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: ParserDiagnosticCount,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: ParserDiagnosticCount,
}

impl ParserDiagnostics {
    /// Adds dropped bytes using saturating arithmetic.
    pub const fn add_dropped_bytes(&mut self, count: ParserDroppedBytes) {
        self.dropped_bytes = self.dropped_bytes.saturating_add(count);
    }

    /// Records one parser resynchronization attempt.
    pub const fn record_resync(&mut self) {
        self.resyncs = self.resyncs.next();
    }

    /// Records one parser error in the corresponding diagnostics counter.
    pub const fn record_error(&mut self, error: ParserError) {
        match error {
            ParserError::OversizedFrame { .. } => {
                self.oversized_frames = self.oversized_frames.next();
            }
            ParserError::BadChecksum => {
                self.bad_checksums = self.bad_checksums.next();
            }
            ParserError::MalformedFrame => {
                self.malformed_frames = self.malformed_frames.next();
            }
            ParserError::Timeout { .. } => {
                self.timeouts = self.timeouts.next();
            }
            ParserError::UnmatchedReply => {
                self.unmatched_replies = self.unmatched_replies.next();
            }
        }
    }

    /// Merges another diagnostics snapshot using saturating arithmetic.
    pub const fn merge(&mut self, other: Self) {
        self.dropped_bytes = self.dropped_bytes.saturating_add(other.dropped_bytes);
        self.resyncs = self.resyncs.saturating_add(other.resyncs);
        self.bad_checksums = self.bad_checksums.saturating_add(other.bad_checksums);
        self.timeouts = self.timeouts.saturating_add(other.timeouts);
        self.oversized_frames = self.oversized_frames.saturating_add(other.oversized_frames);
        self.malformed_frames = self.malformed_frames.saturating_add(other.malformed_frames);
        self.unmatched_replies = self
            .unmatched_replies
            .saturating_add(other.unmatched_replies);
    }
}

/// Stable host-facing diagnostic counter snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticSnapshot {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: ParserDroppedBytes,

    /// Parser resynchronization attempts.
    pub resyncs: ParserDiagnosticCount,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: ParserDiagnosticCount,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: ParserDiagnosticCount,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: ParserDiagnosticCount,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: ParserDiagnosticCount,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: ParserDiagnosticCount,
}

impl DiagnosticSnapshot {
    /// Creates a stable host-facing snapshot from parser diagnostics.
    #[must_use]
    pub const fn from_parser_diagnostics(diagnostics: ParserDiagnostics) -> Self {
        Self {
            dropped_bytes: diagnostics.dropped_bytes,
            resyncs: diagnostics.resyncs,
            bad_checksums: diagnostics.bad_checksums,
            timeouts: diagnostics.timeouts,
            oversized_frames: diagnostics.oversized_frames,
            malformed_frames: diagnostics.malformed_frames,
            unmatched_replies: diagnostics.unmatched_replies,
        }
    }

    /// Creates a diagnostic snapshot when the event carries diagnostics.
    #[must_use]
    pub const fn from_device_event(event: &DeviceEvent) -> Option<Self> {
        match event {
            DeviceEvent::Diagnostics(diagnostics) => {
                Some(Self::from_parser_diagnostics(*diagnostics))
            }
            DeviceEvent::LinkUp(_)
            | DeviceEvent::LinkDown
            | DeviceEvent::Tick { .. }
            | DeviceEvent::Telemetry(_)
            | DeviceEvent::ReadOnlyResponse(_)
            | DeviceEvent::ControlRefusal(_)
            | DeviceEvent::DiagnosticError(_) => None,
        }
    }
}

/// Stable host-facing parser error kind.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticErrorKind {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame,

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before expected data arrived.
    Timeout,

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

/// Stable host-facing parser error details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticError {
    /// Stable diagnostic error discriminator.
    pub kind: DiagnosticErrorKind,

    /// Claimed or observed frame length for oversized-frame errors.
    pub claimed_len: Option<ParserFrameLen>,

    /// Configured maximum frame length for oversized-frame errors.
    pub max_len: Option<ParserFrameLen>,

    /// Elapsed monotonic milliseconds for timeout errors.
    pub elapsed_ms: Option<MonotonicTimestamp>,

    /// Timeout threshold in monotonic milliseconds for timeout errors.
    pub timeout_ms: Option<MonotonicTimestamp>,
}

impl DiagnosticError {
    /// Creates stable host-facing error details from a parser error.
    #[must_use]
    pub const fn from_parser_error(error: ParserError) -> Self {
        match error {
            ParserError::OversizedFrame { claimed, max } => Self {
                kind: DiagnosticErrorKind::OversizedFrame,
                claimed_len: Some(claimed),
                max_len: Some(max),
                elapsed_ms: None,
                timeout_ms: None,
            },
            ParserError::BadChecksum => Self::without_details(DiagnosticErrorKind::BadChecksum),
            ParserError::MalformedFrame => {
                Self::without_details(DiagnosticErrorKind::MalformedFrame)
            }
            ParserError::Timeout {
                elapsed_ms,
                timeout_ms,
            } => Self {
                kind: DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed_ms: Some(elapsed_ms),
                timeout_ms: Some(timeout_ms),
            },
            ParserError::UnmatchedReply => {
                Self::without_details(DiagnosticErrorKind::UnmatchedReply)
            }
        }
    }

    const fn without_details(kind: DiagnosticErrorKind) -> Self {
        Self {
            kind,
            claimed_len: None,
            max_len: None,
            elapsed_ms: None,
            timeout_ms: None,
        }
    }
}

/// Size of one transport notification payload after capture/parser admission.
pub type NotificationByteLen = Quantity<Information, NotificationPayloadByte, usize>;

/// Replay split size for one notification chunk; zero preserves whole-notification replay.
pub type NotificationChunkLen = Quantity<Information, NotificationChunkByte, usize>;

impl NotificationChunkLen {
    /// Returns true when replay should preserve whole notifications.
    #[must_use]
    pub const fn is_whole(self) -> bool {
        self.as_bytes() == 0
    }
}

/// Size of a protocol payload body after selector/tag framing bytes are removed.
pub type PayloadBodyLen = Quantity<Information, PayloadBodyByte, usize>;

/// Number of semantic events emitted from one protocol ingest operation.
pub type SemanticEventCount = Quantity<Count, SemanticEvent, usize>;

typed_protocol_value!(
    ProtocolSelector,
    ProtocolSelectorUnit,
    u8,
    "Protocol selector or page identifier carried by a parsed family payload."
);

typed_protocol_value!(
    ProtocolTag,
    ProtocolTagUnit,
    u16,
    "Protocol tag or opcode carried by a parsed family payload."
);

typed_protocol_value!(
    VescControllerId,
    VescControllerIdUnit,
    u8,
    "VESC CAN controller identifier used for forwarded read-only requests."
);

typed_protocol_value!(
    BmsCellValuesPerPage,
    BmsCellValuesPerPageUnit,
    u8,
    "Cell-voltage value count decoded from a full BMS cell page."
);

typed_protocol_value!(
    BmsTemperatureValuesPerPage,
    BmsTemperatureValuesPerPageUnit,
    u8,
    "Temperature value count decoded from a full BMS temperature page."
);

typed_protocol_value!(
    BmsPackIndex,
    BmsPackIndexUnit,
    u8,
    "Zero-based BMS pack index inferred from protocol page metadata."
);

typed_protocol_value!(
    BmsHalfIndex,
    BmsHalfIndexUnit,
    u8,
    "Zero-based BMS half-pack index inferred from protocol page metadata."
);

typed_protocol_value!(
    BmsCellIndex,
    BmsCellIndexUnit,
    u16,
    "Zero-based BMS cell index represented by a decoded cell page."
);

/// Bounded notification evidence shared by protocol ingest outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationEvidence {
    /// Protocol family that accepted or classified the bytes.
    pub family: ProtocolFamily,

    /// Logical protocol channel used for session ingest.
    pub channel: GattChannel,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MonotonicTimestamp,

    /// Number of notification bytes observed.
    pub len: NotificationByteLen,
}

impl NotificationEvidence {
    /// Creates notification evidence for outcomes whose semantic variant owns
    /// any retained payload separately.
    #[must_use]
    pub const fn new(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self {
            family,
            channel,
            monotonic_ms,
            len,
        }
    }
}

/// Maximum raw notification bytes retained for unknown or partially understood
/// protocol evidence.
pub const MAX_RETAINED_NOTIFICATION_PAYLOAD_BYTES: usize = 4_096;

const INLINE_RETAINED_NOTIFICATION_PAYLOAD_BYTES: usize = 36;

/// Small retained payload storage for parser paths that should not allocate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineRetainedNotificationPayload {
    len: u8,
    bytes: [u8; INLINE_RETAINED_NOTIFICATION_PAYLOAD_BYTES],
}

impl InlineRetainedNotificationPayload {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut retained = Self {
            len: u8::try_from(bytes.len()).ok()?,
            bytes: [0; INLINE_RETAINED_NOTIFICATION_PAYLOAD_BYTES],
        };
        retained.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(retained)
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// Bounded raw payload retained when protocol bytes are not fully understood.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetainedNotificationPayload {
    /// No raw payload was available to retain.
    Empty,

    /// Small bounded raw payload retained inline on hot parser paths.
    Inline(InlineRetainedNotificationPayload),

    /// Bounded raw payload retained for later investigation.
    Bytes(Box<ArrayVec<u8, MAX_RETAINED_NOTIFICATION_PAYLOAD_BYTES>>),
}

impl RetainedNotificationPayload {
    /// Creates an empty retained payload.
    #[must_use]
    pub fn empty() -> Self {
        Self::Empty
    }

    /// Copies bounded raw payload bytes for later protocol investigation.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.is_empty() {
            return Self::Empty;
        }

        if bytes.len() <= INLINE_RETAINED_NOTIFICATION_PAYLOAD_BYTES {
            if let Some(retained) = InlineRetainedNotificationPayload::from_bytes(bytes) {
                return Self::Inline(retained);
            }
        }

        Self::Bytes(Box::new(
            bytes
                .iter()
                .copied()
                .take(MAX_RETAINED_NOTIFICATION_PAYLOAD_BYTES)
                .collect(),
        ))
    }

    /// Returns the retained payload bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Empty => &[],
            Self::Inline(bytes) => bytes.as_slice(),
            Self::Bytes(bytes) => bytes.as_slice(),
        }
    }

    /// Returns the number of raw bytes retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Returns whether no raw bytes were retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

impl Default for RetainedNotificationPayload {
    fn default() -> Self {
        Self::empty()
    }
}

/// Reason a notification did not enter a family-owned decoder path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoredNotificationReason {
    /// Notification arrived on a channel the selected protocol does not consume.
    WrongChannel,

    /// Notification could not be associated with a supported protocol family.
    UnsupportedFamily,

    /// Notification was classified to a family but not to a supported channel.
    UnsupportedChannel,

    /// Notification was accepted by a known family but no semantic mapping exists yet.
    AcceptedButUnmapped,

    /// Notification advanced frame-boundary search without completing a frame.
    SeekingFrameBoundary,

    /// Notification was classified and intentionally dropped by policy.
    IntentionallyDropped,
}

/// Bounded evidence for notifications that were explicitly ignored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IgnoredNotificationEvidence {
    /// Protocol family when classification got that far.
    pub family: Option<ProtocolFamily>,

    /// Logical protocol channel used for session ingest.
    pub channel: GattChannel,

    /// Host monotonic receive timestamp.
    pub monotonic_ms: MonotonicTimestamp,

    /// Number of notification bytes observed.
    pub len: NotificationByteLen,

    /// Bounded raw payload retained to identify ignored bytes in captures.
    pub retained_payload: RetainedNotificationPayload,
}

impl IgnoredNotificationEvidence {
    /// Creates ignored-notification evidence when the caller only has a
    /// previously measured byte length.
    #[must_use]
    pub fn new(
        family: Option<ProtocolFamily>,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self {
            family,
            channel,
            monotonic_ms,
            len,
            retained_payload: RetainedNotificationPayload::empty(),
        }
    }

    /// Creates bounded ignored-notification evidence with retained raw bytes.
    #[must_use]
    pub fn with_retained_payload(
        family: Option<ProtocolFamily>,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self {
            family,
            channel,
            monotonic_ms,
            len: NotificationByteLen::from_bytes(bytes.len()),
            retained_payload: RetainedNotificationPayload::from_bytes(bytes),
        }
    }
}

/// Bounded evidence for protocol payloads that are known but intentionally not
/// decoded as stable telemetry yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadClassifier {
    /// Protocol selector/page id.
    Selector(ProtocolSelector),

    /// Protocol tag/opcode.
    Tag(ProtocolTag),
}

impl PayloadClassifier {
    /// Creates selector-based payload evidence.
    #[must_use]
    pub const fn selector(selector: ProtocolSelector) -> Self {
        Self::Selector(selector)
    }

    /// Creates tag-based payload evidence.
    #[must_use]
    pub const fn tag(tag: ProtocolTag) -> Self {
        Self::Tag(tag)
    }

    /// Returns the selector value when this evidence is selector-based.
    #[must_use]
    pub const fn selector_value(self) -> Option<ProtocolSelector> {
        match self {
            Self::Selector(selector) => Some(selector),
            Self::Tag(_) => None,
        }
    }

    /// Returns the tag value when this evidence is tag-based.
    #[must_use]
    pub const fn tag_value(self) -> Option<ProtocolTag> {
        match self {
            Self::Selector(_) => None,
            Self::Tag(tag) => Some(tag),
        }
    }
}

/// Bounded evidence for protocol payloads that are known but intentionally not
/// decoded as stable telemetry yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReservedPayloadEvidence {
    /// Typed payload classifier for the family.
    pub classifier: PayloadClassifier,

    /// Length of the classified body.
    pub body_len: PayloadBodyLen,

    /// Bounded raw payload bytes retained for later semantic mapping.
    pub retained_payload: RetainedNotificationPayload,

    /// Verification status for this reserved-payload classification.
    pub verification: VerificationStatus,
}

/// Bounded evidence for a known-family payload that still has no stable parser
/// mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParserGapEvidence {
    /// Typed payload classifier for the family.
    pub classifier: PayloadClassifier,

    /// Length of the unparsed body.
    pub body_len: PayloadBodyLen,

    /// Bounded raw payload bytes retained for later semantic mapping.
    pub retained_payload: RetainedNotificationPayload,
}

/// Typed result of feeding one transport notification into a protocol decoder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationIngestOutcome {
    /// The notification produced one or more semantic session events.
    SemanticEvents {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Number of semantic events produced by this ingest step.
        event_count: SemanticEventCount,
    },

    /// The protocol accepted the bytes but is still waiting for a complete
    /// frame/message.
    BufferedFragment(NotificationEvidence),

    /// The protocol produced parser diagnostics for the notification.
    ParserDiagnostic {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Parser error emitted for this ingest step.
        error: ParserError,
    },

    /// The protocol recognized the payload as known/reserved evidence.
    KnownReserved {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Reserved payload evidence with any retained bytes needed for later mapping.
        payload: ReservedPayloadEvidence,
    },

    /// The protocol family accepted the notification but lacks a stable mapping
    /// for the payload.
    ParserGap {
        /// Bounded notification evidence.
        notification: NotificationEvidence,

        /// Parser-gap evidence with retained bytes needed for later mapping.
        gap: ParserGapEvidence,
    },

    /// The session explicitly ignored the notification.
    Ignored {
        /// Bounded ignored-notification evidence.
        evidence: IgnoredNotificationEvidence,

        /// Reason the notification did not enter a decoder path.
        reason: IgnoredNotificationReason,
    },
}

impl NotificationIngestOutcome {
    /// Creates a semantic-events outcome.
    #[must_use]
    pub const fn semantic_events(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        event_count: SemanticEventCount,
    ) -> Self {
        Self::SemanticEvents {
            notification: NotificationEvidence::new(family, channel, len, monotonic_ms),
            event_count,
        }
    }

    /// Creates an accepted buffered-fragment outcome.
    #[must_use]
    pub const fn buffered_fragment(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::BufferedFragment(NotificationEvidence::new(
            family,
            channel,
            len,
            monotonic_ms,
        ))
    }

    /// Creates a parser-diagnostic outcome.
    #[must_use]
    pub const fn parser_diagnostic(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        error: ParserError,
    ) -> Self {
        Self::ParserDiagnostic {
            notification: NotificationEvidence::new(family, channel, len, monotonic_ms),
            error,
        }
    }

    /// Creates a known-reserved payload outcome.
    #[must_use]
    pub const fn known_reserved(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        payload: ReservedPayloadEvidence,
    ) -> Self {
        Self::KnownReserved {
            notification: NotificationEvidence::new(family, channel, len, monotonic_ms),
            payload,
        }
    }

    /// Creates a parser-gap outcome.
    #[must_use]
    pub const fn parser_gap(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
        gap: ParserGapEvidence,
    ) -> Self {
        Self::ParserGap {
            notification: NotificationEvidence::new(family, channel, len, monotonic_ms),
            gap,
        }
    }

    /// Creates an ignored wrong-channel notification outcome.
    #[must_use]
    pub fn ignored_wrong_channel(
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::new(None, channel, len, monotonic_ms),
            reason: IgnoredNotificationReason::WrongChannel,
        }
    }

    /// Creates an ignored wrong-channel notification outcome for a known family.
    #[must_use]
    pub fn wrong_channel_for_family(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            reason: IgnoredNotificationReason::WrongChannel,
        }
    }

    /// Creates a known-family wrong-channel outcome with retained raw bytes.
    #[must_use]
    pub fn wrong_channel_for_family_bytes(
        family: ProtocolFamily,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::with_retained_payload(
                Some(family),
                channel,
                bytes,
                monotonic_ms,
            ),
            reason: IgnoredNotificationReason::WrongChannel,
        }
    }

    /// Creates an ignored unsupported-family notification outcome.
    #[must_use]
    pub fn unsupported_family(
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::new(None, channel, len, monotonic_ms),
            reason: IgnoredNotificationReason::UnsupportedFamily,
        }
    }

    /// Creates an ignored unsupported-channel notification outcome.
    #[must_use]
    pub fn unsupported_channel(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            reason: IgnoredNotificationReason::UnsupportedChannel,
        }
    }

    /// Creates an accepted-but-unmapped notification outcome.
    #[must_use]
    pub fn accepted_but_unmapped(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            reason: IgnoredNotificationReason::AcceptedButUnmapped,
        }
    }

    /// Creates an accepted-but-unmapped outcome with retained raw bytes.
    #[must_use]
    pub fn accepted_but_unmapped_bytes(
        family: ProtocolFamily,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::with_retained_payload(
                Some(family),
                channel,
                bytes,
                monotonic_ms,
            ),
            reason: IgnoredNotificationReason::AcceptedButUnmapped,
        }
    }

    /// Creates a frame-boundary-search outcome with retained raw bytes.
    #[must_use]
    pub fn seeking_frame_boundary(
        family: ProtocolFamily,
        channel: GattChannel,
        bytes: &[u8],
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::with_retained_payload(
                Some(family),
                channel,
                bytes,
                monotonic_ms,
            ),
            reason: IgnoredNotificationReason::SeekingFrameBoundary,
        }
    }

    /// Creates an intentionally dropped notification outcome.
    #[must_use]
    pub fn intentionally_dropped(
        family: ProtocolFamily,
        channel: GattChannel,
        len: NotificationByteLen,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Ignored {
            evidence: IgnoredNotificationEvidence::new(Some(family), channel, len, monotonic_ms),
            reason: IgnoredNotificationReason::IntentionallyDropped,
        }
    }
}

/// Transport-independent request target used for correlation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequestTarget {
    /// Direct request to the connected controller/device.
    #[default]
    Local,

    /// Request forwarded to a VESC CAN controller id.
    VescCanController {
        /// VESC CAN controller id.
        controller_id: VescControllerId,
    },
}

/// Transport-independent key used to correlate a scheduled request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestKey {
    /// Command kind represented by this request.
    pub command: CommandKind,

    /// Transport-independent target represented by this request.
    pub target: RequestTarget,
}

impl RequestKey {
    /// Creates a request key from a command kind.
    #[must_use]
    pub const fn new(command: CommandKind) -> Self {
        Self::for_target(command, RequestTarget::Local)
    }

    /// Creates a request key from a command kind and explicit target.
    #[must_use]
    pub const fn for_target(command: CommandKind, target: RequestTarget) -> Self {
        Self { command, target }
    }
}

/// Retry, timeout, and pacing policy for one scheduled request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestPolicy {
    /// Deadline duration for one attempt.
    pub timeout: Duration,

    /// Maximum retries after the first attempt.
    pub max_retries: u8,

    /// Minimum interval between starts for the same key.
    pub min_interval: Duration,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_milliseconds(1_000),
            max_retries: 0,
            min_interval: Duration::from_milliseconds(0),
        }
    }
}

/// Active scheduled request state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledRequest {
    /// Request correlation key.
    pub key: RequestKey,

    /// Request scheduling policy.
    pub policy: RequestPolicy,

    /// Monotonic start time for the current attempt.
    pub started_at_ms: MonotonicTimestamp,

    /// Zero-based retry count for the current attempt.
    pub retries: u8,
}

impl ScheduledRequest {
    const fn attempts(self) -> u8 {
        self.retries.saturating_add(1)
    }
}

/// Reason a request could not be started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestStartError {
    /// Another ambiguous request is already awaiting a reply.
    Busy {
        /// Key for the active request.
        key: RequestKey,
    },

    /// The request key is still inside its pacing interval.
    Pacing {
        /// Earliest monotonic time when the request can be started.
        ready_at_ms: MonotonicTimestamp,
    },

    /// No active request can be retried.
    NoActiveRequest,
}

/// Decision returned when advancing request time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestTick {
    /// No request is currently active.
    Idle,

    /// The active request has not reached its deadline.
    Waiting,

    /// The active request reached its deadline and may be retried.
    Retry {
        /// Request key eligible for retry.
        key: RequestKey,

        /// One-based retry attempt number.
        attempt: u8,
    },

    /// The active request reached its deadline and has no retries remaining.
    TimedOut {
        /// Request key that timed out.
        key: RequestKey,

        /// Total attempts including the initial attempt.
        attempts: u8,
    },
}

/// Result of correlating a reply with the active request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorrelationResult {
    /// Reply matched the active request and cleared the slot.
    Matched {
        /// Matched request key.
        key: RequestKey,

        /// Total attempts including the initial attempt.
        attempts: u8,
    },

    /// Reply did not match the active request, or no request was active.
    Unmatched {
        /// Reply key that could not be matched.
        key: RequestKey,
    },
}

/// One-slot request tracker for ambiguous protocol replies.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestTracker {
    in_flight: Option<ScheduledRequest>,
    last_started: Option<(RequestKey, MonotonicTimestamp)>,
}

impl RequestTracker {
    /// Returns the active in-flight request, if any.
    #[must_use]
    pub const fn in_flight(self) -> Option<ScheduledRequest> {
        self.in_flight
    }

    /// Starts a request if no ambiguous request is active and pacing allows it.
    ///
    /// # Errors
    ///
    /// Returns [`RequestStartError::Busy`] when an earlier request is still
    /// active, or [`RequestStartError::Pacing`] when the key is inside its
    /// minimum start interval.
    pub fn start(
        &mut self,
        key: RequestKey,
        policy: RequestPolicy,
        now_ms: MonotonicTimestamp,
    ) -> Result<(), RequestStartError> {
        if let Some(active) = self.in_flight {
            return Err(RequestStartError::Busy { key: active.key });
        }

        if let Some((last_key, started_at_ms)) = self.last_started {
            let ready_at_ms = started_at_ms.saturating_add_duration(policy.min_interval);
            if last_key == key && now_ms < ready_at_ms {
                return Err(RequestStartError::Pacing { ready_at_ms });
            }
        }

        self.in_flight = Some(ScheduledRequest {
            key,
            policy,
            started_at_ms: now_ms,
            retries: 0,
        });
        self.last_started = Some((key, now_ms));
        Ok(())
    }

    /// Advances scheduler time and reports timeout or retry eligibility.
    #[must_use]
    pub const fn on_tick(self, now_ms: MonotonicTimestamp) -> RequestTick {
        let Some(active) = self.in_flight else {
            return RequestTick::Idle;
        };
        let deadline_ms = active
            .started_at_ms
            .saturating_add_duration(active.policy.timeout);
        if now_ms.get() < deadline_ms.get() {
            RequestTick::Waiting
        } else if active.retries < active.policy.max_retries {
            RequestTick::Retry {
                key: active.key,
                attempt: active.retries.saturating_add(1),
            }
        } else {
            RequestTick::TimedOut {
                key: active.key,
                attempts: active.attempts(),
            }
        }
    }

    /// Marks the active request retry as started at `now_ms`.
    ///
    /// # Errors
    ///
    /// Returns [`RequestStartError::NoActiveRequest`] when no request is
    /// active, or [`RequestStartError::Busy`] when no retries remain.
    pub const fn retry_started(
        &mut self,
        now_ms: MonotonicTimestamp,
    ) -> Result<(), RequestStartError> {
        let Some(mut active) = self.in_flight else {
            return Err(RequestStartError::NoActiveRequest);
        };
        if active.retries >= active.policy.max_retries {
            return Err(RequestStartError::Busy { key: active.key });
        }
        active.retries = active.retries.saturating_add(1);
        active.started_at_ms = now_ms;
        self.last_started = Some((active.key, now_ms));
        self.in_flight = Some(active);
        Ok(())
    }

    /// Correlates a reply key with the active request and updates diagnostics.
    pub fn correlate_reply(
        &mut self,
        key: RequestKey,
        diagnostics: &mut ParserDiagnostics,
    ) -> CorrelationResult {
        let Some(active) = self.in_flight else {
            diagnostics.record_error(ParserError::UnmatchedReply);
            return CorrelationResult::Unmatched { key };
        };

        if active.key == key {
            self.in_flight = None;
            CorrelationResult::Matched {
                key,
                attempts: active.attempts(),
            }
        } else {
            diagnostics.record_error(ParserError::UnmatchedReply);
            CorrelationResult::Unmatched { key }
        }
    }
}

/// Relative scheduling urgency for queued read-only requests.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RequestUrgency {
    /// Routine polling work such as regular telemetry refreshes.
    Routine,

    /// Higher-value probes such as identity or capability refreshes.
    High,

    /// Critical read-only probes that should be sent before other queued work.
    Critical,
}

/// Number of higher-priority pops allowed before older queued work can age ahead.
pub const REQUEST_STARVATION_SKIP_THRESHOLD: u8 = 2;

/// Request staged in a bounded scheduler queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuedRequest {
    /// Request correlation key.
    pub key: RequestKey,

    /// Request scheduling policy.
    pub policy: RequestPolicy,

    /// Relative scheduling urgency.
    pub urgency: RequestUrgency,
}

impl QueuedRequest {
    /// Creates a queued request with routine urgency.
    #[must_use]
    pub const fn new(key: RequestKey, policy: RequestPolicy) -> Self {
        Self::with_urgency(key, policy, RequestUrgency::Routine)
    }

    /// Creates a queued request with explicit urgency.
    #[must_use]
    pub const fn with_urgency(
        key: RequestKey,
        policy: RequestPolicy,
        urgency: RequestUrgency,
    ) -> Self {
        Self {
            key,
            policy,
            urgency,
        }
    }
}

/// Reason a request could not be queued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestQueueError {
    /// The queue has no free slots.
    Full {
        /// Queue capacity in requests.
        capacity: usize,
    },

    /// A request with the same key is already queued.
    DuplicateKey {
        /// Duplicate request key.
        key: RequestKey,
    },
}

/// Per-urgency scheduler counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestUrgencyCounters {
    /// Routine request count.
    pub routine: u64,

    /// High-priority request count.
    pub high: u64,

    /// Critical request count.
    pub critical: u64,
}

impl RequestUrgencyCounters {
    fn increment(&mut self, urgency: RequestUrgency) {
        match urgency {
            RequestUrgency::Routine => self.routine = self.routine.saturating_add(1),
            RequestUrgency::High => self.high = self.high.saturating_add(1),
            RequestUrgency::Critical => self.critical = self.critical.saturating_add(1),
        }
    }
}

/// Structured scheduler diagnostics for bounded request queues.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RequestSchedulerDiagnostics {
    /// Requests refused because a matching key was already queued.
    pub duplicate_refusals: u64,

    /// Requests refused because the queue was full.
    pub overflow_refusals: u64,

    /// Requests accepted by urgency.
    pub enqueued: RequestUrgencyCounters,

    /// Requests popped by urgency.
    pub dequeued: RequestUrgencyCounters,

    /// Starvation-aging promotions or interventions.
    pub starvation_aging_events: u64,
}

/// Fixed-capacity FIFO request queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestQueue<const N: usize> {
    entries: [Option<QueuedRequest>; N],
    len: usize,
}

impl<const N: usize> Default for RequestQueue<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> RequestQueue<N> {
    /// Creates an empty request queue.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            len: 0,
        }
    }

    /// Returns the queue capacity.
    #[must_use]
    pub const fn capacity(self) -> usize {
        N
    }

    /// Returns the number of queued requests.
    #[must_use]
    pub const fn len(self) -> usize {
        self.len
    }

    /// Returns whether the queue is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    /// Returns whether a request key is already queued.
    #[must_use]
    pub fn contains_key(self, key: RequestKey) -> bool {
        let mut index = 0;
        while index < self.len {
            if let Some(request) = self.entries[index]
                && request.key == key
            {
                return true;
            }
            index += 1;
        }
        false
    }

    /// Enqueues a request at the back of the queue.
    ///
    /// # Errors
    ///
    /// Returns [`RequestQueueError::DuplicateKey`] when the same key is already
    /// queued, or [`RequestQueueError::Full`] when the fixed capacity is
    /// exhausted.
    pub fn enqueue(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        if self.contains_key(request.key) {
            return Err(RequestQueueError::DuplicateKey { key: request.key });
        }

        if self.len == N {
            return Err(RequestQueueError::Full { capacity: N });
        }

        self.entries[self.len] = Some(request);
        self.len += 1;
        Ok(())
    }

    /// Enqueues a request ahead of lower-urgency work.
    ///
    /// Requests with the same urgency retain FIFO order.
    ///
    /// # Errors
    ///
    /// Returns [`RequestQueueError::DuplicateKey`] when the same key is already
    /// queued, or [`RequestQueueError::Full`] when the fixed capacity is
    /// exhausted.
    pub fn enqueue_by_urgency(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        self.enqueue_by_urgency_with_index(request).map(|_| ())
    }

    fn enqueue_by_urgency_with_index(
        &mut self,
        request: QueuedRequest,
    ) -> Result<usize, RequestQueueError> {
        if self.contains_key(request.key) {
            return Err(RequestQueueError::DuplicateKey { key: request.key });
        }

        if self.len == N {
            return Err(RequestQueueError::Full { capacity: N });
        }

        let insert_at = self
            .entries
            .iter()
            .take(self.len)
            .position(|entry| entry.is_some_and(|queued| request.urgency > queued.urgency))
            .unwrap_or(self.len);

        let mut move_from = self.len;
        while move_from > insert_at {
            self.entries[move_from] = self.entries[move_from - 1];
            move_from -= 1;
        }
        self.entries[insert_at] = Some(request);
        self.len += 1;
        Ok(insert_at)
    }

    /// Removes and returns the front request.
    pub const fn pop_next(&mut self) -> Option<QueuedRequest> {
        if self.len == 0 {
            return None;
        }

        let next = self.entries[0];
        let mut index = 1;
        while index < self.len {
            self.entries[index - 1] = self.entries[index];
            index += 1;
        }
        self.len -= 1;
        self.entries[self.len] = None;
        next
    }
}

/// Fixed-capacity request scheduler with observable diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestScheduler<const N: usize> {
    queue: RequestQueue<N>,
    skip_counts: [u8; N],
    diagnostics: RequestSchedulerDiagnostics,
}

impl<const N: usize> Default for RequestScheduler<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> RequestScheduler<N> {
    /// Creates an empty request scheduler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            queue: RequestQueue::new(),
            skip_counts: [0; N],
            diagnostics: RequestSchedulerDiagnostics {
                duplicate_refusals: 0,
                overflow_refusals: 0,
                enqueued: RequestUrgencyCounters {
                    routine: 0,
                    high: 0,
                    critical: 0,
                },
                dequeued: RequestUrgencyCounters {
                    routine: 0,
                    high: 0,
                    critical: 0,
                },
                starvation_aging_events: 0,
            },
        }
    }

    /// Returns scheduler diagnostics accumulated so far.
    #[must_use]
    pub const fn diagnostics(self) -> RequestSchedulerDiagnostics {
        self.diagnostics
    }

    /// Returns the number of queued requests.
    #[must_use]
    pub const fn len(self) -> usize {
        self.queue.len()
    }

    /// Returns whether the scheduler queue is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.queue.is_empty()
    }

    /// Enqueues a request at FIFO priority while updating diagnostics.
    ///
    /// # Errors
    ///
    /// Returns the same refusal reason as [`RequestQueue::enqueue`].
    pub fn enqueue(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        let previous_len = self.queue.len();
        let result = self.queue.enqueue(request);
        if result.is_ok() {
            self.skip_counts[previous_len] = 0;
        }
        self.record_enqueue_result(request, result)
    }

    /// Enqueues by urgency while updating diagnostics.
    ///
    /// # Errors
    ///
    /// Returns the same refusal reason as [`RequestQueue::enqueue_by_urgency`].
    pub fn enqueue_by_urgency(&mut self, request: QueuedRequest) -> Result<(), RequestQueueError> {
        let result = self.queue.enqueue_by_urgency_with_index(request);
        if let Ok(insert_at) = result {
            self.insert_skip_count(insert_at);
        }
        self.record_enqueue_result(request, result.map(|_| ()))
    }

    /// Removes and returns the next request while updating diagnostics.
    pub fn pop_next(&mut self) -> Option<QueuedRequest> {
        let selected = self.aged_pop_index()?;
        let request = self.remove_at(selected)?;
        if selected > 0 {
            self.diagnostics.starvation_aging_events =
                self.diagnostics.starvation_aging_events.saturating_add(1);
        }
        self.age_skipped_after_pop(selected);
        self.diagnostics.dequeued.increment(request.urgency);
        Some(request)
    }

    fn insert_skip_count(&mut self, insert_at: usize) {
        let mut move_from = self.queue.len().saturating_sub(1);
        while move_from > insert_at {
            self.skip_counts[move_from] = self.skip_counts[move_from - 1];
            move_from -= 1;
        }
        self.skip_counts[insert_at] = 0;
    }

    fn aged_pop_index(&self) -> Option<usize> {
        if self.queue.is_empty() {
            return None;
        }
        let mut index = 1;
        while index < self.queue.len() {
            if self.skip_counts[index] >= REQUEST_STARVATION_SKIP_THRESHOLD {
                return Some(index);
            }
            index += 1;
        }
        Some(0)
    }

    fn remove_at(&mut self, selected: usize) -> Option<QueuedRequest> {
        let request = self.queue.entries[selected]?;
        let mut index = selected + 1;
        while index < self.queue.len() {
            self.queue.entries[index - 1] = self.queue.entries[index];
            self.skip_counts[index - 1] = self.skip_counts[index];
            index += 1;
        }
        self.queue.len -= 1;
        self.queue.entries[self.queue.len] = None;
        self.skip_counts[self.queue.len] = 0;
        Some(request)
    }

    fn age_skipped_after_pop(&mut self, selected: usize) {
        for (index, skip_count) in self
            .skip_counts
            .iter_mut()
            .take(self.queue.len())
            .enumerate()
        {
            if index >= selected {
                *skip_count = skip_count.saturating_add(1);
            }
        }
    }

    fn record_enqueue_result(
        &mut self,
        request: QueuedRequest,
        result: Result<(), RequestQueueError>,
    ) -> Result<(), RequestQueueError> {
        match result {
            Ok(()) => {
                self.diagnostics.enqueued.increment(request.urgency);
                Ok(())
            }
            Err(RequestQueueError::DuplicateKey { key }) => {
                self.diagnostics.duplicate_refusals =
                    self.diagnostics.duplicate_refusals.saturating_add(1);
                Err(RequestQueueError::DuplicateKey { key })
            }
            Err(RequestQueueError::Full { capacity }) => {
                self.diagnostics.overflow_refusals =
                    self.diagnostics.overflow_refusals.saturating_add(1);
                Err(RequestQueueError::Full { capacity })
            }
        }
    }
}

/// One read-only request entry in a protocol polling plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PollRequest {
    /// Command kind to request.
    pub kind: CommandKind,

    /// Request scheduling policy.
    pub policy: RequestPolicy,

    /// Relative scheduling urgency.
    pub urgency: RequestUrgency,
}

impl PollRequest {
    /// Creates a poll request entry.
    #[must_use]
    pub const fn new(kind: CommandKind, policy: RequestPolicy, urgency: RequestUrgency) -> Self {
        Self {
            kind,
            policy,
            urgency,
        }
    }

    /// Converts this poll entry to a queued request.
    ///
    /// # Errors
    ///
    /// Returns [`PollingPlanError::UnsupportedCommand`] when the command is not
    /// read-only.
    pub const fn to_queued_request(self) -> Result<QueuedRequest, PollingPlanError> {
        let safety_class = self.kind.safety_class();
        if matches!(safety_class, SafetyClass::ReadOnly) {
            Ok(QueuedRequest::with_urgency(
                RequestKey::new(self.kind),
                self.policy,
                self.urgency,
            ))
        } else {
            Err(PollingPlanError::UnsupportedCommand {
                kind: self.kind,
                safety_class,
            })
        }
    }
}

/// Reason a polling plan could not be enqueued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PollingPlanError {
    /// Polling plans may only contain read-only commands.
    UnsupportedCommand {
        /// Rejected command kind.
        kind: CommandKind,

        /// Safety class that made the command unsupported for polling.
        safety_class: SafetyClass,
    },

    /// The destination queue refused a request.
    Queue(RequestQueueError),
}

/// Fixed protocol polling plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PollingPlan<const N: usize> {
    items: [PollRequest; N],
}

impl<const N: usize> PollingPlan<N> {
    /// Creates a polling plan from fixed poll entries.
    #[must_use]
    pub const fn new(items: [PollRequest; N]) -> Self {
        Self { items }
    }

    /// Returns the plan entries.
    #[must_use]
    pub const fn items(self) -> [PollRequest; N] {
        self.items
    }

    /// Enqueues the plan into a bounded request queue.
    ///
    /// # Errors
    ///
    /// Returns [`PollingPlanError::UnsupportedCommand`] for non-read-only plan
    /// entries, or [`PollingPlanError::Queue`] when the destination queue
    /// refuses a converted request.
    pub fn enqueue_into<const Q: usize>(
        self,
        queue: &mut RequestQueue<Q>,
    ) -> Result<(), PollingPlanError> {
        for item in self.items {
            queue
                .enqueue_by_urgency(item.to_queued_request()?)
                .map_err(PollingPlanError::Queue)?;
        }
        Ok(())
    }
}

/// Transport write behavior requested by a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    /// Write with transport-level acknowledgement.
    WithResponse,

    /// Write without transport-level acknowledgement.
    WithoutResponse,
}

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
        let scaled = (numerator + 500_000) / 1_000_000;
        Self::from_milli_kmh(saturating_i64_to_i32(scaled))
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

/// Raw numeric field reported by a protocol-specific response.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawFieldValue {
    /// Protocol-family field identifier.
    pub id: u16,

    /// Sign-extended raw field value.
    pub value: i64,
}

/// Protocol-native IEEE-754 single-precision field.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawFloatFieldValue {
    /// Protocol-family field identifier.
    pub id: u16,
    /// Exact IEEE-754 bits received or decoded from the protocol.
    pub value_bits: u32,
}

impl RawFloatFieldValue {
    /// Creates a field while retaining the exact `f32` bit pattern.
    #[must_use]
    pub const fn new(id: u16, value: f32) -> Self {
        Self {
            id,
            value_bits: value.to_bits(),
        }
    }

    /// Returns the retained floating-point value.
    #[must_use]
    pub const fn value(self) -> f32 {
        f32::from_bits(self.value_bits)
    }
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

/// Conservative ride operating state decoded from protocol-specific status fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideOperatingState {
    /// No live evidence has established whether the vehicle is parked, riding, or charging.
    Unknown,

    /// Explicit telemetry indicates the vehicle is parked or ready but not balancing.
    Parked,

    /// Live telemetry indicates the vehicle is stationary without explicit parked/off evidence.
    Standing,

    /// Telemetry indicates the vehicle is moving or balancing under ride context.
    Riding,

    /// Telemetry indicates charger-connected/charging state.
    Charging,
}

/// Protocol-decoded controller operating mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideOperatingMode {
    /// The protocol reported an unsupported mode value.
    Unknown,
    /// Normal upright balancing mode.
    Normal,
    /// Upside-down darkride mode.
    Darkride,
    /// Hand-test mode.
    Handtest,
    /// Flywheel test mode.
    Flywheel,
}

/// Protocol-decoded ride warning whose meaning is independent of presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideWarning {
    /// No ride warning is active.
    None,

    /// Controller input voltage is below its configured warning threshold.
    LowVoltage,
    /// Controller input voltage is above its configured warning threshold.
    HighVoltage,
    /// Controller MOSFET temperature reached its warning threshold.
    MosfetTemperature,
    /// Motor temperature reached its warning threshold.
    MotorTemperature,
    /// Motor current reached its configured warning threshold.
    Current,

    /// The controller is applying duty-based pushback.
    DutyPushback,

    /// The controller is applying temperature-based pushback.
    TemperaturePushback,

    /// The controller reports active wheel slip.
    Wheelslip,

    /// The controller reports a sensor warning.
    Sensors,
    /// The package reports a low battery warning.
    LowBattery,
    /// The package reports an error warning.
    Error,
}

/// Protocol-decoded reason that the controller stopped balancing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RideStopReason {
    /// No stop condition is active.
    None,
    /// Board pitch exceeded the allowed range.
    Pitch,
    /// Board roll exceeded the allowed range.
    Roll,
    /// One half of the footpad switch caused the stop.
    SwitchHalf,
    /// The full footpad switch caused the stop.
    SwitchFull,
    /// Reverse-stop logic caused the stop.
    Reverse,
    /// Quick-stop logic caused the stop.
    QuickStop,
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

/// Availability of read-only battery or BMS data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BatteryReadbackAvailability {
    /// Battery or BMS data was reported by the device.
    Available,

    /// Battery or BMS data is expected for this device/profile but was not reported.
    #[default]
    Unavailable,

    /// Battery or BMS data is not supported for this device/profile.
    Unsupported,
}

/// Read-only battery or BMS page readback.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BatteryReadback {
    /// Whether battery or BMS data is available for display.
    availability: BatteryReadbackAvailability,

    /// Battery/BMS page payload, when available.
    page: Option<BatteryPagePayload>,
}

impl BatteryReadback {
    /// Creates an available battery/BMS readback.
    #[must_use]
    pub const fn available(page: BatteryPagePayload) -> Self {
        Self {
            availability: BatteryReadbackAvailability::Available,
            page: Some(page),
        }
    }

    /// Creates an unavailable battery/BMS readback.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            availability: BatteryReadbackAvailability::Unavailable,
            page: None,
        }
    }

    /// Creates an unsupported battery/BMS readback.
    #[must_use]
    pub const fn unsupported() -> Self {
        Self {
            availability: BatteryReadbackAvailability::Unsupported,
            page: None,
        }
    }

    /// Returns whether battery or BMS data is available for display.
    #[must_use]
    pub const fn availability(&self) -> BatteryReadbackAvailability {
        self.availability
    }

    /// Returns the battery/BMS page payload, when available.
    #[must_use]
    pub const fn page(&self) -> Option<&BatteryPagePayload> {
        self.page.as_ref()
    }
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

/// Bounded diagnostic readback response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticReadback {
    /// Diagnostic detail slots.
    pub details: [Option<DiagnosticDetail>; 4],
}

/// Bounded protocol-native raw telemetry readback.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RawTelemetryReadback {
    /// Present raw telemetry fields.
    #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_raw_fields"))]
    pub fields: ArrayVec<RawFieldValue, 8>,

    /// Present protocol-native float fields, retained without narrowing.
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "deserialize_raw_float_fields")
    )]
    pub float_fields: ArrayVec<RawFloatFieldValue, 19>,
}

#[cfg(feature = "serde")]
fn deserialize_raw_fields<'de, D>(deserializer: D) -> Result<ArrayVec<RawFieldValue, 8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_sparse_slots(deserializer)
}

#[cfg(feature = "serde")]
fn deserialize_raw_float_fields<'de, D>(
    deserializer: D,
) -> Result<ArrayVec<RawFloatFieldValue, 19>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_sparse_slots(deserializer)
}

#[cfg(feature = "serde")]
fn deserialize_sparse_slots<'de, T, D, const CAPACITY: usize>(
    deserializer: D,
) -> Result<ArrayVec<T, CAPACITY>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    let slots = <Vec<Option<T>> as serde::Deserialize>::deserialize(deserializer)?;
    let mut fields = ArrayVec::new();
    for field in slots.into_iter().flatten().take(CAPACITY) {
        if fields.try_push(field).is_err() {
            break;
        }
    }
    Ok(fields)
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

/// Availability of a read-only settings response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SettingsReadbackAvailability {
    /// Settings were reported by the device.
    Available,

    /// Settings are expected for this device/profile but were not reported.
    #[default]
    Unavailable,

    /// Settings are not supported for this device/profile.
    Unsupported,
}

/// Bounded settings readback response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SettingsReadback {
    /// Whether settings are available for display.
    availability: SettingsReadbackAvailability,

    /// Settings entries.
    entries: [Option<SettingsEntry>; 4],
}

impl SettingsReadback {
    /// Creates an available settings readback.
    #[must_use]
    pub const fn available(entries: [Option<SettingsEntry>; 4]) -> Self {
        Self {
            availability: SettingsReadbackAvailability::Available,
            entries,
        }
    }

    /// Creates an unavailable settings readback.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            availability: SettingsReadbackAvailability::Unavailable,
            entries: [None, None, None, None],
        }
    }

    /// Creates an unsupported settings readback.
    #[must_use]
    pub const fn unsupported() -> Self {
        Self {
            availability: SettingsReadbackAvailability::Unsupported,
            entries: [None, None, None, None],
        }
    }

    /// Returns whether settings are available for display.
    #[must_use]
    pub const fn availability(self) -> SettingsReadbackAvailability {
        self.availability
    }

    /// Returns the bounded settings entries.
    #[must_use]
    pub const fn entries(self) -> [Option<SettingsEntry>; 4] {
        self.entries
    }
}

/// Availability of read-only fault-history data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FaultHistoryAvailability {
    /// Fault history was reported by the device.
    Available,

    /// Fault history is expected for this device/profile but was not reported.
    #[default]
    Unavailable,

    /// Fault history is not supported for this device/profile.
    Unsupported,
}

/// Protocol-specific fault code without proven semantic mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultCode {
    /// Raw protocol field/value pair for an unknown fault code.
    pub raw: RawFieldValue,
}

impl FaultCode {
    /// Creates a structured unknown fault code from a raw protocol field/value pair.
    #[must_use]
    pub const fn unknown(raw: RawFieldValue) -> Self {
        Self { raw }
    }
}

/// Last reported fault, preserving fault identity separately from provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultHistoryEntry {
    /// Protocol-specific fault code without proven semantic mapping.
    pub code: FaultCode,

    /// Source of the fault code.
    pub source: ValueSource,

    /// Confidence in the fault-code interpretation.
    pub quality: ValueQuality,

    /// Verification state for the fault-code interpretation.
    pub verification: VerificationStatus,
}

impl FaultHistoryEntry {
    /// Creates a structured unknown fault code reported directly by the device.
    #[must_use]
    pub const fn reported_unknown(code: FaultCode) -> Self {
        Self {
            code,
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        }
    }
}

/// Read-only last-fault history.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FaultHistoryReadback {
    /// Whether fault history is available for display.
    availability: FaultHistoryAvailability,

    /// Last reported fault, if the device reports one.
    last_fault: Option<FaultHistoryEntry>,

    /// Distance since the last fault, if reported separately.
    since_distance: Option<Measured<Distance>>,
}

impl FaultHistoryReadback {
    /// Creates an unavailable fault-history readback.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            availability: FaultHistoryAvailability::Unavailable,
            last_fault: None,
            since_distance: None,
        }
    }

    /// Creates an unsupported fault-history readback.
    #[must_use]
    pub const fn unsupported() -> Self {
        Self {
            availability: FaultHistoryAvailability::Unsupported,
            last_fault: None,
            since_distance: None,
        }
    }

    /// Creates an available fault-history readback proving no fault at a reported distance.
    #[must_use]
    pub const fn no_fault_since(since_distance: Measured<Distance>) -> Self {
        Self {
            availability: FaultHistoryAvailability::Available,
            last_fault: None,
            since_distance: Some(since_distance),
        }
    }

    /// Creates an available fault-history readback with a last-fault code.
    #[must_use]
    pub const fn fault_since(
        last_fault: FaultHistoryEntry,
        since_distance: Option<Measured<Distance>>,
    ) -> Self {
        Self {
            availability: FaultHistoryAvailability::Available,
            last_fault: Some(last_fault),
            since_distance,
        }
    }

    /// Returns whether fault-history data is available for display.
    #[must_use]
    pub const fn availability(self) -> FaultHistoryAvailability {
        self.availability
    }

    /// Returns the last reported fault, if one was reported.
    #[must_use]
    pub const fn last_fault(self) -> Option<FaultHistoryEntry> {
        self.last_fault
    }

    /// Returns distance since the last fault, if reported separately.
    #[must_use]
    pub const fn since_distance(self) -> Option<Measured<Distance>> {
        self.since_distance
    }
}

/// Generic read-only response payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadOnlyResponse {
    /// Firmware or protocol version response.
    Firmware(FirmwareInfo),

    /// Battery or BMS response.
    Battery(BatteryReadback),

    /// Diagnostic response.
    Diagnostics(DiagnosticReadback),

    /// Protocol-native raw telemetry response.
    RawTelemetry(RawTelemetryReadback),

    /// Settings readback response.
    Settings(SettingsReadback),

    /// Fault-history readback response.
    FaultHistory(FaultHistoryReadback),
}

impl ReadOnlyResponse {
    /// Returns the command kind that requested this response.
    #[must_use]
    pub const fn command_kind(&self) -> CommandKind {
        match self {
            Self::Firmware(_) => CommandKind::RequestFirmwareInfo,
            Self::Battery(_) => CommandKind::RequestBatteryInfo,
            Self::Diagnostics(_) => CommandKind::RequestDiagnostics,
            Self::FaultHistory(_) => CommandKind::RequestFaultHistory,
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

    /// Device charging state decoded from protocol-specific status fields.
    pub charge_mode: Option<Measured<ChargeMode>>,

    /// Ride operating state decoded from protocol-specific status fields.
    pub operating_state: Option<RideOperatingState>,

    /// Controller operating mode decoded from protocol-specific status fields.
    pub operating_mode: Option<RideOperatingMode>,

    /// Ride warning decoded from a protocol-owned status field.
    pub ride_warning: Option<RideWarning>,

    /// Reason the controller stopped balancing.
    pub ride_stop_reason: Option<RideStopReason>,

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

    /// Balance-loop target angle in millidegrees.
    pub balance_angle: Option<Measured<Angle>>,

    /// Roll in millidegrees.
    pub roll: Option<Measured<Angle>>,

    /// Footpad/sensor state for single-wheel boards.
    pub footpad: Option<FootpadTelemetry>,

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
            charge_mode: None,
            operating_state: None,
            operating_mode: None,
            ride_warning: None,
            ride_stop_reason: None,
            motor_current: None,
            power: None,
            controller_temperature: None,
            motor_temperature: None,
            battery_temperature: None,
            pwm: None,
            distance: None,
            pitch: None,
            balance_angle: None,
            roll: None,
            footpad: None,
            battery_level_reported: None,
            battery_level_estimated: None,
        }
    }
}

/// Semantically decoded footpad contact state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FootpadContactState {
    /// Neither footpad contact is active.
    None,

    /// Only the left footpad contact is active.
    Left,

    /// Only the right footpad contact is active.
    Right,

    /// Both footpad contacts are active.
    Both,
}

/// Latest known footpad sensor state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FootpadTelemetry {
    /// Protocol-specific footpad state bitfield/nibble.
    pub state: u8,

    /// Semantically decoded contact state when the protocol defines one.
    pub contact_state: Option<FootpadContactState>,

    /// First footpad ADC reading in protocol units, scaled by 1000.
    pub adc1_milliunits: Option<i32>,

    /// Second footpad ADC reading in protocol units, scaled by 1000.
    pub adc2_milliunits: Option<i32>,
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

    /// Latest known device charging state.
    pub charge_mode: Option<Measured<ChargeMode>>,

    /// Latest known ride operating state.
    pub operating_state: Option<RideOperatingState>,

    /// Latest known controller operating mode.
    pub operating_mode: Option<RideOperatingMode>,

    /// Latest protocol-decoded ride warning.
    pub ride_warning: Option<RideWarning>,

    /// Latest reason the controller stopped balancing.
    pub ride_stop_reason: Option<RideStopReason>,

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

    /// Latest known balance-loop target angle in millidegrees.
    pub balance_angle: Option<Measured<Angle>>,

    /// Latest known roll in millidegrees.
    pub roll: Option<Measured<Angle>>,

    /// Latest known footpad/sensor state.
    pub footpad: Option<FootpadTelemetry>,

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
        if delta.charge_mode.is_some() {
            self.charge_mode = delta.charge_mode;
        }
        if delta.operating_state.is_some() {
            self.operating_state = delta.operating_state;
        }
        if delta.operating_mode.is_some() {
            self.operating_mode = delta.operating_mode;
        }
        if delta.ride_warning.is_some() {
            self.ride_warning = delta.ride_warning;
        }
        if delta.ride_stop_reason.is_some() {
            self.ride_stop_reason = delta.ride_stop_reason;
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
        if delta.balance_angle.is_some() {
            self.balance_angle = delta.balance_angle;
        }
        if delta.roll.is_some() {
            self.roll = delta.roll;
        }
        if delta.footpad.is_some() {
            self.footpad = delta.footpad;
        }
        if delta.battery_level_reported.is_some() {
            self.battery_level_reported = delta.battery_level_reported;
        }
        if delta.battery_level_estimated.is_some() {
            self.battery_level_estimated = delta.battery_level_estimated;
        }
    }
}

/// Input supplied to a protocol session by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionInput<'a> {
    /// The underlying transport link is available.
    LinkUp(LinkInfo),

    /// The underlying transport link is no longer available.
    LinkDown,

    /// Notification bytes received from a transport endpoint.
    Notification {
        /// Transport endpoint that produced the bytes.
        channel: GattChannel,

        /// Borrowed notification payload for this reactor step.
        bytes: &'a [u8],

        /// Host monotonic receive timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Timer tick supplied by the host.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Command requested by the host application.
    Command(DeviceCommand),
}

/// Bounded transport write payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WritePayload(WritePayloadStorage);

#[derive(Clone, Debug, Eq, PartialEq)]
enum WritePayloadStorage {
    Inline(ArrayVec<u8, MAX_INLINE_TRANSPORT_WRITE_LEN>),
    Large(Box<ArrayVec<u8, MAX_TRANSPORT_WRITE_LEN>>),
}

impl WritePayload {
    /// Creates a bounded write payload by copying bytes from a slice.
    ///
    /// # Errors
    ///
    /// Returns [`WritePayloadTooLong`] when `bytes` exceeds
    /// [`MAX_TRANSPORT_WRITE_LEN`].
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, WritePayloadTooLong> {
        if bytes.len() > MAX_TRANSPORT_WRITE_LEN {
            return Err(WritePayloadTooLong {
                len: bytes.len(),
                max: MAX_TRANSPORT_WRITE_LEN,
            });
        }

        if bytes.len() <= MAX_INLINE_TRANSPORT_WRITE_LEN {
            return Ok(Self(WritePayloadStorage::Inline(
                ArrayVec::<u8, MAX_INLINE_TRANSPORT_WRITE_LEN>::try_from(bytes).map_err(|_| {
                    WritePayloadTooLong {
                        len: bytes.len(),
                        max: MAX_TRANSPORT_WRITE_LEN,
                    }
                })?,
            )));
        }

        Ok(Self(WritePayloadStorage::Large(Box::new(
            ArrayVec::<u8, MAX_TRANSPORT_WRITE_LEN>::try_from(bytes).map_err(|_| {
                WritePayloadTooLong {
                    len: bytes.len(),
                    max: MAX_TRANSPORT_WRITE_LEN,
                }
            })?,
        ))))
    }

    /// Returns the write payload as bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        match &self.0 {
            WritePayloadStorage::Inline(bytes) => bytes.as_slice(),
            WritePayloadStorage::Large(bytes) => bytes.as_slice(),
        }
    }

    /// Returns the payload length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Returns whether the payload is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Returns whether this payload uses the common inline representation.
    #[must_use]
    pub const fn is_inline(&self) -> bool {
        matches!(self.0, WritePayloadStorage::Inline(_))
    }
}

/// Error returned when constructing an oversized write payload.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("write payload length {len} exceeds maximum {max}")]
pub struct WritePayloadTooLong {
    /// Attempted payload length.
    pub len: usize,

    /// Maximum accepted payload length.
    pub max: usize,
}

/// Action a host transport must perform for a protocol session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportAction {
    /// Subscribe to notifications from a transport endpoint.
    Subscribe {
        /// Transport endpoint to subscribe to.
        channel: GattChannel,
    },

    /// Write bytes to a transport endpoint.
    Write {
        /// Transport endpoint to write to.
        channel: GattChannel,

        /// Bounded bytes to write after this reactor step.
        bytes: WritePayload,

        /// Transport write behavior.
        mode: WriteMode,
    },

    /// Disconnect the underlying transport.
    Disconnect,
}

/// Semantic event emitted by a protocol session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    /// Link-up event accepted by the session.
    LinkUp(LinkInfo),

    /// Link-down event accepted by the session.
    LinkDown,

    /// Tick event accepted by the session.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Telemetry update emitted by a protocol session.
    Telemetry(TelemetryDelta),

    /// Read-only response emitted by a protocol session.
    ReadOnlyResponse(ReadOnlyResponse),

    /// Control command refused before transport writes.
    ControlRefusal(ControlRefusal),

    /// Parser diagnostics emitted by a protocol session.
    Diagnostics(ParserDiagnostics),

    /// Detailed parser diagnostic error emitted by a protocol session.
    DiagnosticError(DiagnosticError),
}

/// Output emitted by a protocol session for the host to drain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionOutput {
    /// Transport action to execute outside the protocol engine.
    Transport(TransportAction),

    /// Semantic event to report to the application.
    Event(DeviceEvent),

    /// Parser-level notification ingest outcome.
    NotificationIngest(NotificationIngestOutcome),
}

/// Error emitted when a session produces more outputs than a checked replay
/// path is willing to retain.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SessionOutputError {
    /// The session produced more outputs than the configured replay limit.
    #[error("session output count {actual:?} exceeds checked replay limit {limit:?}")]
    OutputOverflow {
        /// Configured output limit.
        limit: ParserQueuedOutputCount,

        /// Outputs that would be retained.
        actual: ParserQueuedOutputCount,
    },
}

/// Synchronous protocol reactor.
pub trait ProtocolSession {
    /// Handles one input and appends any resulting outputs.
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>);
}

/// Host-facing synchronous session facade.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostSession<S> {
    session: S,
    output: Vec<SessionOutput>,
    state: Box<CutoutSessionState>,
}

impl<S> HostSession<S>
where
    S: ProtocolSession,
{
    /// Creates a host session around a protocol session.
    #[must_use]
    pub fn new(session: S) -> Self {
        Self {
            session,
            output: Vec::with_capacity(4),
            state: Box::default(),
        }
    }

    /// Supplies a link-up event to the protocol session.
    pub fn ingest_link_up(&mut self, link: LinkInfo) {
        self.handle(SessionInput::LinkUp(link));
    }

    /// Supplies a link-down event to the protocol session.
    pub fn ingest_link_down(&mut self) {
        self.handle(SessionInput::LinkDown);
    }

    /// Supplies owned notification bytes to the protocol session.
    pub fn ingest_notification_owned(
        &mut self,
        channel: GattChannel,
        bytes: Vec<u8>,
        monotonic_ms: MonotonicTimestamp,
    ) {
        let bytes = bytes.into_boxed_slice();
        self.handle(SessionInput::Notification {
            channel,
            bytes: &bytes,
            monotonic_ms,
        });
    }

    /// Supplies a host timer tick to the protocol session.
    pub fn tick(&mut self, monotonic_ms: MonotonicTimestamp) {
        self.handle(SessionInput::Tick { monotonic_ms });
    }

    /// Supplies a host command to the protocol session.
    pub fn issue_command(&mut self, command: DeviceCommand) {
        self.handle(SessionInput::Command(command));
    }

    /// Supplies one borrowed host input to the protocol session.
    pub fn ingest(&mut self, input: SessionInput<'_>) {
        self.handle(input);
    }

    /// Returns mutable access to the protocol session for typed host-side setup.
    pub fn session_mut(&mut self) -> &mut S {
        &mut self.session
    }

    /// Drains owned session outputs accumulated so far.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutput> {
        core::mem::take(&mut self.output)
    }

    /// Moves accumulated session outputs into an existing buffer.
    pub fn drain_outputs_into(&mut self, output: &mut Vec<SessionOutput>) {
        output.append(&mut self.output);
    }

    /// Moves accumulated outputs into an existing buffer while enforcing a
    /// typed replay output limit.
    ///
    /// # Errors
    ///
    /// Returns [`SessionOutputError::OutputOverflow`] when retaining this drain
    /// would exceed `limit`.
    pub fn drain_outputs_checked_into(
        &mut self,
        output: &mut Vec<SessionOutput>,
        limit: ParserQueuedOutputCount,
    ) -> Result<(), SessionOutputError> {
        let actual = output.len().saturating_add(self.output.len());
        (actual <= limit.as_outputs())
            .then(|| self.drain_outputs_into(output))
            .ok_or_else(|| SessionOutputError::OutputOverflow {
                limit,
                actual: ParserQueuedOutputCount::from_outputs(actual),
            })
    }

    /// Returns the latest telemetry snapshot.
    #[must_use]
    pub fn current_snapshot(&self) -> TelemetrySnapshot {
        self.state.current_telemetry()
    }

    /// Returns accumulated parser diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> ParserDiagnostics {
        self.state.parser_diagnostics()
    }

    /// Returns the borrowed Rust-owned session state.
    #[must_use]
    pub fn session_state(&self) -> &CutoutSessionState {
        self.state.as_ref()
    }

    fn handle(&mut self, input: SessionInput<'_>) {
        let start = self.output.len();
        self.session.handle(input, &mut self.output);
        self.state.observe_outputs(&self.output[start..]);
    }
}

/// Owned host input captured for deterministic replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureRecord {
    /// Captured link-up input.
    LinkUp(LinkInfo),

    /// Captured link-down input.
    LinkDown,

    /// Captured notification input with owned bytes.
    Notification {
        /// Transport endpoint that produced the bytes.
        channel: GattChannel,

        /// Owned notification payload.
        bytes: Vec<u8>,

        /// Host monotonic receive timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Captured timer tick.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicTimestamp,
    },

    /// Captured host command.
    Command(DeviceCommand),

    /// Captured host command with target metadata for correlation.
    TargetedCommand {
        /// Captured command.
        command: DeviceCommand,

        /// Captured request target.
        target: RequestTarget,
    },
}

impl CaptureRecord {
    /// Creates a notification capture record with owned bytes.
    #[must_use]
    pub const fn notification(
        channel: GattChannel,
        bytes: Vec<u8>,
        monotonic_ms: MonotonicTimestamp,
    ) -> Self {
        Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        }
    }

    /// Creates a captured host command with explicit target metadata.
    #[must_use]
    pub const fn targeted_command(command: DeviceCommand, target: RequestTarget) -> Self {
        Self::TargetedCommand { command, target }
    }

    /// Splits a notification record into chunks no larger than `chunk_len`.
    ///
    /// Non-notification records are returned unchanged. A zero `chunk_len`
    /// leaves the record unchanged.
    #[must_use]
    pub fn split_notification_bytes(self, chunk_len: NotificationChunkLen) -> Vec<Self> {
        let Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        } = self
        else {
            return vec![self];
        };

        if chunk_len.is_whole() {
            return vec![Self::notification(channel, bytes, monotonic_ms)];
        }

        bytes
            .chunks(chunk_len.as_bytes())
            .map(|chunk| Self::notification(channel, chunk.to_vec(), monotonic_ms))
            .collect()
    }

    /// Splits a notification record by requested chunk lengths.
    ///
    /// Extra bytes are appended as a final chunk. Non-notification records are
    /// returned unchanged.
    #[must_use]
    pub fn split_notification_by_lengths(self, lengths: &[NotificationChunkLen]) -> Vec<Self> {
        let Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        } = self
        else {
            return vec![self];
        };

        let mut records = Vec::new();
        let mut offset = 0;
        for length in lengths.iter().copied().filter(|length| !length.is_whole()) {
            if offset >= bytes.len() {
                break;
            }
            let end = offset.saturating_add(length.as_bytes()).min(bytes.len());
            records.push(Self::notification(
                channel,
                bytes[offset..end].to_vec(),
                monotonic_ms,
            ));
            offset = end;
        }
        if offset < bytes.len() {
            records.push(Self::notification(
                channel,
                bytes[offset..].to_vec(),
                monotonic_ms,
            ));
        }
        records
    }
}

/// Replays captured host inputs through a host session and returns outputs.
#[must_use]
pub fn replay_capture<S>(host: &mut HostSession<S>, records: &[CaptureRecord]) -> Vec<SessionOutput>
where
    S: ProtocolSession,
{
    let mut outputs = Vec::new();
    replay_capture_into(host, records, &mut outputs);
    outputs
}

/// Replays captured host inputs through a host session and returns outputs,
/// enforcing a typed retained-output limit.
///
/// # Errors
///
/// Returns [`SessionOutputError::OutputOverflow`] when the replay would retain
/// more outputs than `output_limit`.
pub fn replay_capture_checked<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
    output_limit: ParserQueuedOutputCount,
) -> Result<Vec<SessionOutput>, SessionOutputError>
where
    S: ProtocolSession,
{
    let mut outputs = Vec::new();
    replay_capture_checked_into(host, records, &mut outputs, output_limit)?;
    Ok(outputs)
}

/// Replays captured host inputs through a host session into an existing buffer.
pub fn replay_capture_into<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
    outputs: &mut Vec<SessionOutput>,
) where
    S: ProtocolSession,
{
    for record in records {
        match record {
            CaptureRecord::LinkUp(link) => host.ingest_link_up(*link),
            CaptureRecord::LinkDown => host.ingest_link_down(),
            CaptureRecord::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => host.ingest(SessionInput::Notification {
                channel: *channel,
                bytes,
                monotonic_ms: *monotonic_ms,
            }),
            CaptureRecord::Tick { monotonic_ms } => host.tick(*monotonic_ms),
            CaptureRecord::Command(command) | CaptureRecord::TargetedCommand { command, .. } => {
                host.issue_command(*command);
            }
        }
        host.drain_outputs_into(outputs);
    }
}

/// Replays captured host inputs through a host session into an existing buffer,
/// enforcing a typed retained-output limit.
///
/// # Errors
///
/// Returns [`SessionOutputError::OutputOverflow`] when a drain would exceed
/// `output_limit`.
pub fn replay_capture_checked_into<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
    outputs: &mut Vec<SessionOutput>,
    output_limit: ParserQueuedOutputCount,
) -> Result<(), SessionOutputError>
where
    S: ProtocolSession,
{
    for record in records {
        match record {
            CaptureRecord::LinkUp(link) => host.ingest_link_up(*link),
            CaptureRecord::LinkDown => host.ingest_link_down(),
            CaptureRecord::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => host.ingest(SessionInput::Notification {
                channel: *channel,
                bytes,
                monotonic_ms: *monotonic_ms,
            }),
            CaptureRecord::Tick { monotonic_ms } => host.tick(*monotonic_ms),
            CaptureRecord::Command(command) | CaptureRecord::TargetedCommand { command, .. } => {
                host.issue_command(*command);
            }
        }
        host.drain_outputs_checked_into(outputs, output_limit)?;
    }
    Ok(())
}

pub(crate) fn drain_semantic_events_checked<S>(
    host: &mut HostSession<S>,
    outputs: &mut Vec<SessionOutput>,
    events: &mut Vec<DeviceEvent>,
    output_limit: ParserQueuedOutputCount,
) -> Result<(), SessionOutputError>
where
    S: ProtocolSession,
{
    host.drain_outputs_checked_into(outputs, output_limit)?;
    events.extend(outputs.drain(..).filter_map(|output| match output {
        SessionOutput::Event(event) => Some(event),
        SessionOutput::Transport(_) | SessionOutput::NotificationIngest(_) => None,
    }));
    Ok(())
}

/// Summary of deterministic replay equivalence across notification chunking
/// modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayChunkComparison {
    /// Semantic event count from whole-notification replay.
    pub whole_semantic_events: SemanticEventCount,

    /// Semantic event count from one-byte notification replay.
    pub one_byte_semantic_events: SemanticEventCount,

    /// Semantic event count from arbitrary notification chunk replay.
    pub arbitrary_semantic_events: SemanticEventCount,

    /// Whether one-byte replay produced the same semantic events as whole
    /// replay.
    pub one_byte_matches: bool,

    /// Whether arbitrary chunk replay produced the same semantic events as
    /// whole replay.
    pub arbitrary_matches: bool,
}

/// Named replay case for testing parser behavior across notification
/// boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationBoundaryReplayCase {
    /// Stable case name for assertion diagnostics.
    pub name: &'static str,

    /// Replay records for this notification boundary layout.
    pub records: Vec<CaptureRecord>,
}

/// Named replay case for malformed or lossy notification streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationImpairmentReplayCase {
    /// Stable case name for assertion diagnostics.
    pub name: &'static str,

    /// Replay records for this impaired notification stream.
    pub records: Vec<CaptureRecord>,
}

/// Replays a capture and returns semantic events only.
///
/// Typed ingest outcomes are intentionally excluded because notification
/// boundaries differ between chunking modes even when decoded protocol behavior
/// is equivalent.
///
/// # Errors
///
/// Returns [`SessionOutputError`] when replay produces more outputs than the
/// default replay retention limit.
pub fn replay_capture_semantic_events<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
) -> Result<Vec<DeviceEvent>, SessionOutputError>
where
    S: ProtocolSession,
{
    let mut outputs = Vec::new();
    let mut events = Vec::new();
    for record in records {
        match record {
            CaptureRecord::LinkUp(link) => host.ingest_link_up(*link),
            CaptureRecord::LinkDown => host.ingest_link_down(),
            CaptureRecord::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => host.ingest(SessionInput::Notification {
                channel: *channel,
                bytes,
                monotonic_ms: *monotonic_ms,
            }),
            CaptureRecord::Tick { monotonic_ms } => host.tick(*monotonic_ms),
            CaptureRecord::Command(command) | CaptureRecord::TargetedCommand { command, .. } => {
                host.issue_command(*command);
            }
        }
        drain_semantic_events_checked(
            host,
            &mut outputs,
            &mut events,
            DEFAULT_REPLAY_OUTPUT_LIMIT,
        )?;
    }
    Ok(events)
}

/// Compares whole-notification replay against one-byte and arbitrary
/// notification chunk replay.
///
/// # Errors
///
/// Returns [`SessionOutputError`] when any replay mode produces more outputs
/// than the default replay retention limit.
pub fn compare_replay_capture_chunks<S, F>(
    mut make_session: F,
    records: &[CaptureRecord],
    arbitrary_lengths: &[NotificationChunkLen],
) -> Result<ReplayChunkComparison, SessionOutputError>
where
    S: ProtocolSession,
    F: FnMut() -> S,
{
    let whole = replay_capture_semantic_events(&mut HostSession::new(make_session()), records)?;
    let one_byte_records =
        split_capture_notifications_by_len(records, NotificationChunkLen::from_bytes(1));
    let one_byte =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &one_byte_records)?;
    let arbitrary_records = split_capture_notifications_by_lengths(records, arbitrary_lengths);
    let arbitrary =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &arbitrary_records)?;

    Ok(ReplayChunkComparison {
        whole_semantic_events: SemanticEventCount::from_events(whole.len()),
        one_byte_semantic_events: SemanticEventCount::from_events(one_byte.len()),
        arbitrary_semantic_events: SemanticEventCount::from_events(arbitrary.len()),
        one_byte_matches: one_byte == whole,
        arbitrary_matches: arbitrary == whole,
    })
}

/// Builds a deterministic arbitrary notification chunk plan from replay
/// records.
///
/// The plan is sized to split the longest notification in the capture using a
/// repeating 2/3/5 byte pattern. Shorter notifications ignore extra chunk
/// lengths during replay.
#[must_use]
pub fn replay_arbitrary_chunk_lengths(records: &[CaptureRecord]) -> Vec<NotificationChunkLen> {
    let max_notification_len = records
        .iter()
        .filter_map(|record| match record {
            CaptureRecord::Notification { bytes, .. } => Some(bytes.len()),
            CaptureRecord::LinkUp(_)
            | CaptureRecord::LinkDown
            | CaptureRecord::Tick { .. }
            | CaptureRecord::Command(_)
            | CaptureRecord::TargetedCommand { .. } => None,
        })
        .max()
        .unwrap_or(0);

    let mut lengths = Vec::new();
    let mut covered = 0usize;
    for chunk_len in [2usize, 3, 5].into_iter().cycle() {
        if covered >= max_notification_len {
            break;
        }
        let remaining = max_notification_len - covered;
        let next = chunk_len.min(remaining);
        lengths.push(NotificationChunkLen::from_bytes(next));
        covered += next;
    }
    lengths
}

/// Builds reusable replay cases for parser tests from protocol frames.
///
/// The returned cases cover one frame per notification, one byte per
/// notification, caller-supplied arbitrary chunk lengths, and all frames
/// coalesced into one notification. Parser tests can state canonical protocol
/// frames once, then compare expected semantic events across these boundary
/// layouts.
#[must_use]
pub fn notification_boundary_replay_cases(
    channel: GattChannel,
    frames: &[&[u8]],
    monotonic_ms: MonotonicTimestamp,
    arbitrary_lengths: &[NotificationChunkLen],
) -> Vec<NotificationBoundaryReplayCase> {
    let whole_records = notification_records(channel, frames, monotonic_ms);
    let one_byte_records =
        split_capture_notifications_by_len(&whole_records, NotificationChunkLen::from_bytes(1));
    let arbitrary_records =
        split_capture_notifications_by_lengths(&whole_records, arbitrary_lengths);
    let coalesced_records = coalesced_notification_record(channel, frames, monotonic_ms);

    vec![
        NotificationBoundaryReplayCase {
            name: "whole",
            records: whole_records,
        },
        NotificationBoundaryReplayCase {
            name: "one-byte",
            records: one_byte_records,
        },
        NotificationBoundaryReplayCase {
            name: "arbitrary",
            records: arbitrary_records,
        },
        NotificationBoundaryReplayCase {
            name: "coalesced",
            records: coalesced_records,
        },
    ]
}

/// Builds reusable replay cases for parser tests that exercise malformed
/// streams.
///
/// The returned cases include noise bytes before a valid frame, duplicate first
/// chunks, missing final bytes, and a timeout tick after a partial frame.
/// Parser tests should state the expected behavior for each named case because
/// some protocols recover while others intentionally reject or wait.
#[must_use]
pub fn notification_impairment_replay_cases(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
    noise_prefix: &[u8],
    timeout_ms: MonotonicTimestamp,
) -> Vec<NotificationImpairmentReplayCase> {
    vec![
        NotificationImpairmentReplayCase {
            name: "noise-prefix",
            records: vec![CaptureRecord::notification(
                channel,
                prefixed_bytes(noise_prefix, frame),
                monotonic_ms,
            )],
        },
        NotificationImpairmentReplayCase {
            name: "duplicate-first-chunk",
            records: duplicate_first_chunk_records(channel, frame, monotonic_ms),
        },
        NotificationImpairmentReplayCase {
            name: "missing-final-byte",
            records: missing_final_byte_record(channel, frame, monotonic_ms),
        },
        NotificationImpairmentReplayCase {
            name: "timeout-after-partial",
            records: timeout_after_partial_records(channel, frame, monotonic_ms, timeout_ms),
        },
    ]
}

fn notification_records(
    channel: GattChannel,
    frames: &[&[u8]],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    frames
        .iter()
        .map(|frame| CaptureRecord::notification(channel, (*frame).to_vec(), monotonic_ms))
        .collect()
}

fn coalesced_notification_record(
    channel: GattChannel,
    frames: &[&[u8]],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    let len = frames.iter().map(|frame| frame.len()).sum();
    let mut bytes = Vec::with_capacity(len);
    for frame in frames {
        bytes.extend_from_slice(frame);
    }

    if bytes.is_empty() {
        Vec::new()
    } else {
        vec![CaptureRecord::notification(channel, bytes, monotonic_ms)]
    }
}

fn prefixed_bytes(prefix: &[u8], bytes: &[u8]) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(prefix.len().saturating_add(bytes.len()));
    prefixed.extend_from_slice(prefix);
    prefixed.extend_from_slice(bytes);
    prefixed
}

fn duplicate_first_chunk_records(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    if frame.is_empty() {
        return Vec::new();
    }

    let split = frame.len().clamp(1, 4);
    let first = frame[..split].to_vec();
    let mut records = vec![
        CaptureRecord::notification(channel, first.clone(), monotonic_ms),
        CaptureRecord::notification(channel, first, monotonic_ms),
    ];

    if split < frame.len() {
        records.push(CaptureRecord::notification(
            channel,
            frame[split..].to_vec(),
            monotonic_ms,
        ));
    }

    records
}

fn missing_final_byte_record(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    let Some(truncated_len) = frame.len().checked_sub(1) else {
        return Vec::new();
    };

    vec![CaptureRecord::notification(
        channel,
        frame[..truncated_len].to_vec(),
        monotonic_ms,
    )]
}

fn timeout_after_partial_records(
    channel: GattChannel,
    frame: &[u8],
    monotonic_ms: MonotonicTimestamp,
    timeout_ms: MonotonicTimestamp,
) -> Vec<CaptureRecord> {
    let split = frame.len().saturating_sub(1);
    vec![
        CaptureRecord::notification(channel, frame[..split].to_vec(), monotonic_ms),
        CaptureRecord::Tick {
            monotonic_ms: timeout_ms,
        },
    ]
}

fn split_capture_notifications_by_len(
    records: &[CaptureRecord],
    chunk_len: NotificationChunkLen,
) -> Vec<CaptureRecord> {
    records
        .iter()
        .cloned()
        .flat_map(|record| record.split_notification_bytes(chunk_len))
        .collect()
}

fn split_capture_notifications_by_lengths(
    records: &[CaptureRecord],
    lengths: &[NotificationChunkLen],
) -> Vec<CaptureRecord> {
    records
        .iter()
        .cloned()
        .flat_map(|record| record.split_notification_by_lengths(lengths))
        .collect()
}

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-core"
}

#[cfg(test)]
mod tests {
    use super::crate_name;
    use crate::round_div_i32;
    use crate::{
        Angle, BatteryCurrent, BatteryLevel, Capacity, CellVoltage, ControlRefusalReason, Current,
        DeviceCommand, DeviceEvent, Distance, Duration, DutyCycle, Energy, FootpadTelemetry,
        GattChannel, LightState, LinkInfo, Measured, MonotonicTimestamp, ParallelCount,
        PeakCurrent, PhaseCurrent, Power, ProtocolSession, SETTING_WRITE_CONFIRMATION_TIMEOUT,
        SeriesCount, SessionInput, SessionOutput, SettingState, SettingValue, SettingValueSource,
        Speed, TelemetryDelta, TelemetrySnapshot, Temperature, TransportAction, UnsupportedReason,
        ValueQuality, ValueSource, VerificationStatus, Voltage, WriteMode, WritePayload,
    };
    use core::mem::size_of;
    use proptest::prelude::*;

    const TEST_CHANNEL: GattChannel = GattChannel::from_bytes([0xA1; 16]);

    const fn ms(value: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::new(value)
    }

    const fn dropped_bytes(value: u64) -> crate::ParserDroppedBytes {
        crate::ParserDroppedBytes::from_bytes(value)
    }

    const fn diag_count(value: u64) -> crate::ParserDiagnosticCount {
        crate::ParserDiagnosticCount::from_events(value)
    }

    const fn write_len(value: u16) -> crate::TransportWriteLimit {
        crate::TransportWriteLimit::from_bytes(value)
    }

    const fn frame_len(value: usize) -> crate::ParserFrameLen {
        crate::ParserFrameLen::from_bytes(value)
    }

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-core");
    }

    #[test]
    fn setting_state_requires_matching_readback_before_confirmation() {
        let mut state = SettingState::current(LightState::Off, SettingValueSource::LiveReadback);

        state.submit(LightState::On, ms(10));
        assert_eq!(
            state,
            SettingState::Pending {
                current: Some(SettingValue {
                    value: LightState::Off,
                    source: SettingValueSource::LiveReadback,
                }),
                requested: LightState::On,
                submitted_at: ms(10),
            }
        );
        assert!(!state.confirm(LightState::Off, ms(11)));
        assert!(state.confirm(LightState::On, ms(12)));
        assert_eq!(
            state,
            SettingState::Confirmed {
                value: SettingValue {
                    value: LightState::On,
                    source: SettingValueSource::LiveReadback,
                },
                confirmed_at: ms(12),
            }
        );

        state.submit(LightState::Off, ms(20));
        state.timeout();
        assert_eq!(
            state,
            SettingState::TimedOut {
                current: Some(SettingValue {
                    value: LightState::On,
                    source: SettingValueSource::LiveReadback,
                }),
                requested: LightState::Off,
            }
        );

        let mut observed = SettingState::<LightState>::unknown();
        assert!(!observed.observe(LightState::Off, SettingValueSource::LiveReadback, ms(40)));
        assert_eq!(
            observed,
            SettingState::Current(SettingValue {
                value: LightState::Off,
                source: SettingValueSource::LiveReadback,
            })
        );
    }

    #[test]
    fn setting_state_timeout_waits_for_deadline() {
        let mut state = SettingState::<LightState>::unknown();
        state.submit(LightState::On, ms(10));
        let timeout = SETTING_WRITE_CONFIRMATION_TIMEOUT;

        assert!(!state.timeout_if_elapsed(ms(2_009), timeout));
        assert!(matches!(state, SettingState::Pending { .. }));

        assert!(state.timeout_if_elapsed(ms(2_010), timeout));
        assert!(matches!(
            state,
            SettingState::TimedOut {
                requested: LightState::On,
                ..
            }
        ));
    }

    #[test]
    fn setting_state_preserves_refusal_without_a_transport_write() {
        let mut state = SettingState::<LightState>::unknown();
        state.submit(LightState::On, ms(30));
        state.refuse(ControlRefusalReason::UnsupportedCommand);

        assert_eq!(
            state,
            SettingState::Refused {
                current: None,
                requested: Some(LightState::On),
                reason: ControlRefusalReason::UnsupportedCommand,
            }
        );
    }

    #[test]
    fn write_payload_preserves_bytes_without_vec_storage() {
        let payload = WritePayload::try_from_slice(b"telemetry").expect("payload fits");

        assert_eq!(payload.as_slice(), b"telemetry");
        assert_eq!(payload.len(), 9);
        assert!(payload.is_inline());
    }

    #[test]
    fn write_payload_uses_explicit_large_variant_for_rare_max_size_writes() {
        let bytes = [0xa5; crate::MAX_TRANSPORT_WRITE_LEN];
        let payload = WritePayload::try_from_slice(&bytes).expect("max payload fits");

        assert_eq!(payload.as_slice(), bytes);
        assert_eq!(payload.len(), crate::MAX_TRANSPORT_WRITE_LEN);
        assert!(!payload.is_inline());
    }

    #[test]
    fn write_payload_rejects_oversized_writes() {
        let bytes = vec![0; crate::MAX_TRANSPORT_WRITE_LEN + 1];

        assert_eq!(
            WritePayload::try_from_slice(&bytes),
            Err(crate::WritePayloadTooLong {
                len: crate::MAX_TRANSPORT_WRITE_LEN + 1,
                max: crate::MAX_TRANSPORT_WRITE_LEN,
            })
        );
    }

    #[test]
    fn wire_voltage_keeps_protocol_voltage_units_explicit() {
        let voltage = crate::WireVoltage::from_centivolts(6_005);

        assert_eq!(voltage.as_centivolts(), 6_005);
        assert_eq!(voltage.as_millivolts(), 60_050);
        assert_eq!(
            voltage.as_scaled_voltage(1_000),
            Voltage::from_millivolts(60_050)
        );
        assert_eq!(
            crate::WireVoltage::from_scaled_voltage(Voltage::from_millivolts(60_050), 1_000),
            voltage
        );
    }

    #[test]
    fn battery_page_types_remain_small() {
        assert_eq!(size_of::<crate::BatteryPageKind>(), 1);
        assert_eq!(size_of::<crate::BatteryPageMetadata>(), 8);
        assert!(size_of::<crate::BatteryInfo>() <= 64);
        assert!(size_of::<crate::BatteryPagePayload>() <= 128);
        // Eight raw integer slots plus thirty-two lossless float slots keep this
        // bounded while allowing the VESC read-only decoder to retain native data.
        assert!(
            size_of::<crate::RawTelemetryReadback>() <= 1_024,
            "RawTelemetryReadback size={}",
            size_of::<crate::RawTelemetryReadback>()
        );
        assert!(
            size_of::<crate::ReadOnlyResponse>() <= 1_024,
            "ReadOnlyResponse size={}",
            size_of::<crate::ReadOnlyResponse>()
        );
        assert!(size_of::<SessionOutput>() <= 1024);
        assert_eq!(size_of::<TransportAction>(), 64);
    }

    #[test]
    fn inline_write_capacity_size_snapshot_quantifies_transport_cost() {
        assert_eq!(crate::MAX_TRANSPORT_WRITE_LEN, 512);
        assert_eq!(crate::MAX_INLINE_TRANSPORT_WRITE_LEN, 32);
        assert_eq!(size_of::<WritePayload>(), 40);
        assert_eq!(size_of::<TransportAction>(), 64);
        assert!(size_of::<SessionOutput>() <= 1024);
    }

    #[test]
    fn raw_telemetry_response_preserves_protocol_native_fields() {
        let response = crate::ReadOnlyResponse::RawTelemetry(crate::RawTelemetryReadback {
            fields: [
                crate::RawFieldValue::new(0x8001, 989),
                crate::RawFieldValue::new(0x8002, -21_973),
                crate::RawFieldValue::new(0x8003, 20),
                crate::RawFieldValue::new(0x8004, 0),
            ]
            .into_iter()
            .collect(),
            float_fields: arrayvec::ArrayVec::new(),
        });

        assert_eq!(
            response.command_kind(),
            crate::CommandKind::RequestTelemetry
        );
        let crate::ReadOnlyResponse::RawTelemetry(raw) = response else {
            panic!("expected raw telemetry");
        };
        assert_eq!(raw.fields[0], crate::RawFieldValue::new(0x8001, 989));
        assert_eq!(raw.fields[1], crate::RawFieldValue::new(0x8002, -21_973));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn raw_telemetry_serde_reads_sparse_slots_and_writes_packed_fields() {
        let raw: crate::RawTelemetryReadback = serde_json::from_value(serde_json::json!({
            "fields": [null, { "id": 0x8002, "value": -21_973 }],
            "float_fields": [null, null, { "id": 0x8010, "value_bits": 0x3f80_0001 }]
        }))
        .expect("legacy sparse telemetry should deserialize");

        assert_eq!(
            raw.fields.as_slice(),
            &[crate::RawFieldValue::new(0x8002, -21_973)]
        );
        assert_eq!(
            raw.float_fields.as_slice(),
            &[crate::RawFloatFieldValue {
                id: 0x8010,
                value_bits: 0x3f80_0001,
            }]
        );
        assert_eq!(
            serde_json::to_value(raw).expect("packed telemetry should serialize"),
            serde_json::json!({
                "fields": [{ "id": 0x8002, "value": -21_973 }],
                "float_fields": [{ "id": 0x8010, "value_bits": 0x3f80_0001 }]
            })
        );
    }

    #[test]
    fn request_scheduler_size_snapshot_separates_queue_and_diagnostics_cost() {
        assert_eq!(size_of::<crate::QueuedRequest>(), 32);
        assert_eq!(size_of::<crate::RequestSchedulerDiagnostics>(), 72);
        assert_eq!(size_of::<crate::RequestQueue<3>>(), 104);
        assert_eq!(size_of::<crate::RequestScheduler<3>>(), 184);
        assert_eq!(size_of::<crate::RequestQueue<8>>(), 264);
        assert_eq!(size_of::<crate::RequestScheduler<8>>(), 344);
    }

    #[test]
    fn request_hot_path_types_remain_small() {
        assert!(size_of::<crate::RequestKey>() <= 16);
        assert!(size_of::<crate::RequestPolicy>() <= 24);
        assert!(size_of::<crate::QueuedRequest>() <= 32);
        assert!(size_of::<crate::RequestTracker>() <= 56);
        assert!(size_of::<crate::PollRequest>() <= 32);
        assert!(size_of::<crate::RequestQueue<3>>() <= 104);
        assert!(size_of::<crate::RequestScheduler<3>>() <= 184);
        assert!(size_of::<crate::PollingPlan<4>>() <= 128);
    }

    #[test]
    fn parser_hot_path_types_remain_small() {
        assert!(size_of::<Measured<u16>>() <= 8);
        assert!(size_of::<Measured<i32>>() <= 16);
        assert!(size_of::<Measured<u64>>() <= 24);
        assert_eq!(size_of::<crate::NotificationByteLen>(), size_of::<usize>());
        assert_eq!(size_of::<crate::NotificationChunkLen>(), size_of::<usize>());
        assert_eq!(size_of::<crate::PayloadBodyLen>(), size_of::<usize>());
        assert_eq!(size_of::<crate::SemanticEventCount>(), size_of::<usize>());
        assert_eq!(size_of::<crate::ProtocolSelector>(), size_of::<u8>());
        assert_eq!(size_of::<crate::ProtocolTag>(), size_of::<u16>());
        assert_eq!(size_of::<SeriesCount>(), size_of::<u8>());
        assert_eq!(size_of::<ParallelCount>(), size_of::<u8>());
        assert_eq!(size_of::<crate::BmsCellValuesPerPage>(), size_of::<u8>());
        assert_eq!(
            size_of::<crate::BmsTemperatureValuesPerPage>(),
            size_of::<u8>()
        );
        assert_eq!(size_of::<crate::BmsPackIndex>(), size_of::<u8>());
        assert_eq!(size_of::<crate::BmsHalfIndex>(), size_of::<u8>());
        assert_eq!(size_of::<crate::BmsCellIndex>(), size_of::<u16>());
        assert_eq!(size_of::<crate::ParserDiagnostics>(), 56);
        assert_eq!(size_of::<crate::DiagnosticSnapshot>(), 56);
        assert!(size_of::<crate::DiagnosticError>() <= 80);
        assert!(size_of::<crate::NotificationIngestOutcome>() <= 128);
        assert!(size_of::<crate::NotificationEvidence>() <= 64);
        assert!(size_of::<crate::PayloadClassifier>() <= 4);
        assert!(size_of::<crate::ReservedPayloadEvidence>() <= 64);
        assert!(size_of::<TelemetrySnapshot>() <= 256);
        assert!(size_of::<crate::CaptureRecord>() <= 48);
        assert!(size_of::<crate::HostSession<EchoSession>>() <= 352);
    }

    #[test]
    fn notification_ingest_evidence_uses_distinct_typed_protocol_values() {
        let notification_len = crate::NotificationByteLen::from_bytes(77);
        let body_len = crate::PayloadBodyLen::from_bytes(24);
        let event_count = crate::SemanticEventCount::from_events(3);
        let selector = crate::ProtocolSelector::new(8);
        let tag = crate::ProtocolTag::new(0x5c);

        assert_eq!(notification_len.as_bytes(), 77);
        assert_eq!(body_len.as_bytes(), 24);
        assert_eq!(event_count.as_events(), 3);
        assert_eq!(selector.get(), 8);
        assert_eq!(tag.get(), 0x5c);
    }

    #[test]
    fn notification_ingest_outcome_distinguishes_buffered_fragments_from_ignored_traffic() {
        let buffered = crate::NotificationIngestOutcome::buffered_fragment(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(20),
            ms(7),
        );
        let ignored = crate::NotificationIngestOutcome::ignored_wrong_channel(
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(20),
            ms(7),
        );
        let wrong_channel = crate::NotificationIngestOutcome::wrong_channel_for_family(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(9),
            ms(8),
        );
        let unmapped = crate::NotificationIngestOutcome::accepted_but_unmapped(
            crate::ProtocolFamily::Vesc,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(10),
            ms(9),
        );
        let dropped = crate::NotificationIngestOutcome::intentionally_dropped(
            crate::ProtocolFamily::BegodeGotway,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(12),
            ms(10),
        );

        assert!(matches!(
            buffered,
            crate::NotificationIngestOutcome::BufferedFragment(evidence)
                if evidence.family == crate::ProtocolFamily::VeteranLeaperkimNosfet
                    && evidence.channel == TEST_CHANNEL
                    && evidence.len == crate::NotificationByteLen::from_bytes(20)
                    && evidence.monotonic_ms == ms(7)
        ));
        assert!(matches!(
            ignored,
            crate::NotificationIngestOutcome::Ignored { evidence, reason }
                if reason == crate::IgnoredNotificationReason::WrongChannel
                    && evidence.family.is_none()
                    && evidence.channel == TEST_CHANNEL
                    && evidence.len == crate::NotificationByteLen::from_bytes(20)
                    && evidence.monotonic_ms == ms(7)
        ));
        assert!(matches!(
            wrong_channel,
            crate::NotificationIngestOutcome::Ignored { evidence, reason }
                if reason == crate::IgnoredNotificationReason::WrongChannel
                    && evidence.family == Some(crate::ProtocolFamily::VeteranLeaperkimNosfet)
                    && evidence.channel == TEST_CHANNEL
                    && evidence.len == crate::NotificationByteLen::from_bytes(9)
                    && evidence.monotonic_ms == ms(8)
        ));
        assert!(matches!(
            unmapped,
            crate::NotificationIngestOutcome::Ignored { evidence, reason }
                if reason == crate::IgnoredNotificationReason::AcceptedButUnmapped
                    && evidence.family == Some(crate::ProtocolFamily::Vesc)
                    && evidence.channel == TEST_CHANNEL
                    && evidence.len == crate::NotificationByteLen::from_bytes(10)
                    && evidence.monotonic_ms == ms(9)
        ));
        assert!(matches!(
            dropped,
            crate::NotificationIngestOutcome::Ignored { evidence, reason }
                if reason == crate::IgnoredNotificationReason::IntentionallyDropped
                    && evidence.family == Some(crate::ProtocolFamily::BegodeGotway)
                    && evidence.channel == TEST_CHANNEL
                    && evidence.len == crate::NotificationByteLen::from_bytes(12)
                    && evidence.monotonic_ms == ms(10)
        ));
    }

    #[test]
    fn notification_ingest_outcome_carries_known_reserved_payload_evidence() {
        let outcome = crate::NotificationIngestOutcome::known_reserved(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(75),
            ms(12),
            crate::ReservedPayloadEvidence {
                classifier: crate::PayloadClassifier::selector(crate::ProtocolSelector::new(8)),
                body_len: crate::PayloadBodyLen::from_bytes(68),
                retained_payload: crate::RetainedNotificationPayload::from_bytes(&[0x08, 0xaa]),
                verification: VerificationStatus::HardwareVerified,
            },
        );

        assert!(matches!(
            outcome,
            crate::NotificationIngestOutcome::KnownReserved {
                notification,
                payload,
            } if notification.family == crate::ProtocolFamily::VeteranLeaperkimNosfet
                && notification.channel == TEST_CHANNEL
                && notification.len == crate::NotificationByteLen::from_bytes(75)
                && notification.monotonic_ms == ms(12)
                && payload.classifier.selector_value() == Some(crate::ProtocolSelector::new(8))
                && payload.classifier.tag_value().is_none()
                && payload.body_len == crate::PayloadBodyLen::from_bytes(68)
                && payload.retained_payload.as_slice() == [0x08, 0xaa]
                && payload.verification == VerificationStatus::HardwareVerified
        ));
    }

    #[test]
    fn notification_ingest_outcome_counts_semantic_events_without_storing_them() {
        let outcome = crate::NotificationIngestOutcome::semantic_events(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(77),
            ms(21),
            crate::SemanticEventCount::from_events(3),
        );

        assert!(matches!(
            outcome,
            crate::NotificationIngestOutcome::SemanticEvents {
                notification,
                event_count,
            } if notification.family == crate::ProtocolFamily::VeteranLeaperkimNosfet
                && notification.channel == TEST_CHANNEL
                && notification.len == crate::NotificationByteLen::from_bytes(77)
                && notification.monotonic_ms == ms(21)
                && event_count == crate::SemanticEventCount::from_events(3)
        ));
    }

    #[test]
    fn notification_ingest_outcome_carries_parser_diagnostics_without_raw_bytes() {
        let outcome = crate::NotificationIngestOutcome::parser_diagnostic(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(77),
            ms(22),
            crate::ParserError::BadChecksum,
        );

        assert!(matches!(
            outcome,
            crate::NotificationIngestOutcome::ParserDiagnostic {
                notification,
                error: crate::ParserError::BadChecksum,
            } if notification.family == crate::ProtocolFamily::VeteranLeaperkimNosfet
                && notification.channel == TEST_CHANNEL
        ));
    }

    #[test]
    fn notification_ingest_debug_keeps_retained_payload_evidence() {
        let outcome = crate::NotificationIngestOutcome::parser_gap(
            crate::ProtocolFamily::VeteranLeaperkimNosfet,
            TEST_CHANNEL,
            crate::NotificationByteLen::from_bytes(77),
            ms(15),
            crate::ParserGapEvidence {
                classifier: crate::PayloadClassifier::tag(crate::ProtocolTag::new(0x5c)),
                body_len: crate::PayloadBodyLen::from_bytes(70),
                retained_payload: crate::RetainedNotificationPayload::from_bytes(&[
                    0x5c, 0xde, 0xad, 0xbe, 0xef,
                ]),
            },
        );
        let debug = format!("{outcome:?}");

        assert!(debug.contains("ParserGap"));
        assert!(debug.contains("body_len"));
        assert!(debug.contains("value: 70"));
        assert!(debug.contains("retained_payload"));
        assert!(debug.contains("222"));
    }

    #[derive(Default)]
    struct EchoSession {
        last_notification_len: usize,
        link_is_up: bool,
    }

    impl ProtocolSession for EchoSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(info) => {
                    self.link_is_up = true;
                    output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
                }
                SessionInput::LinkDown => {
                    self.link_is_up = false;
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::Notification {
                    bytes,
                    channel,
                    monotonic_ms,
                } => {
                    self.last_notification_len = bytes.len();
                    output.push(SessionOutput::NotificationIngest(
                        crate::NotificationIngestOutcome::ignored_wrong_channel(
                            channel,
                            crate::NotificationByteLen::from_bytes(bytes.len()),
                            monotonic_ms,
                        ),
                    ));
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                }
                SessionInput::Command(DeviceCommand::RequestTelemetry) => {
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes([1; 16]),
                        bytes: WritePayload::try_from_slice(b"telemetry")
                            .expect("test write payload fits"),
                        mode: WriteMode::WithResponse,
                    }));
                }
                SessionInput::Command(DeviceCommand::RequestIdentity) => {
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([2; 16]),
                    }));
                }
                SessionInput::Command(
                    DeviceCommand::RequestFirmwareInfo
                    | DeviceCommand::RequestBatteryInfo
                    | DeviceCommand::RequestDiagnostics
                    | DeviceCommand::RequestFaultHistory
                    | DeviceCommand::RequestSettings
                    | DeviceCommand::SetLights(_)
                    | DeviceCommand::SetPedalMode(_)
                    | DeviceCommand::SetAccelerationAssist(_)
                    | DeviceCommand::SetTaillight(_)
                    | DeviceCommand::SoundHorn
                    | DeviceCommand::SetRawMotorCurrent { .. },
                ) => {}
            }
        }
    }

    #[test]
    fn drives_a_session_without_runtime_or_ble_stack() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();
        let link = LinkInfo {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len(185)),
        };

        session.handle(SessionInput::LinkUp(link), &mut output);

        assert!(session.link_is_up);
        assert_eq!(
            output.as_slice(),
            &[SessionOutput::Event(DeviceEvent::LinkUp(link))]
        );
    }

    #[test]
    fn passes_notification_bytes_through_borrowed_input() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();
        let channel = GattChannel::from_bytes([0xfe; 16]);

        session.handle(
            SessionInput::Notification {
                channel,
                bytes: &[0xdc, 0x5a, 0x5c],
                monotonic_ms: ms(20),
            },
            &mut output,
        );

        assert_eq!(session.last_notification_len, 3);
        assert_eq!(
            output.as_slice(),
            &[SessionOutput::NotificationIngest(
                crate::NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    crate::NotificationByteLen::from_bytes(3),
                    ms(20)
                )
            )]
        );
    }

    #[test]
    fn hosts_can_drain_owned_actions_after_each_input() {
        let mut session = EchoSession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::Command(DeviceCommand::RequestTelemetry),
            &mut output,
        );
        let drained = core::mem::take(&mut output);

        assert!(output.is_empty());
        assert_eq!(
            drained.as_slice(),
            &[SessionOutput::Transport(TransportAction::Write {
                channel: GattChannel::from_bytes([1; 16]),
                bytes: WritePayload::try_from_slice(b"telemetry").expect("test write payload fits"),
                mode: WriteMode::WithResponse,
            })]
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn quantity_conversions_keep_unit_math_in_core() {
        assert_eq!(Speed::from_mph(10).as_millimetres_per_second(), 4_474);
        assert_eq!(Speed::from_millimetres_per_second(4_470).as_mph(), 10);
        assert_eq!(Speed::from_kmh(50).as_kmh_rounded(), 50);
        assert_eq!(
            Speed::from_centimetres_per_second(1_336).as_millimetres_per_second(),
            13_360
        );
        assert_eq!(
            Speed::from_millimetres_per_second(22_222).as_kmh_rounded(),
            80
        );
        assert_eq!(
            Speed::from_millimetres_per_second(15_277).as_deci_kmh_rounded(),
            550
        );
        assert_eq!(
            Speed::from_milli_kmh_scaled(10_000, 1_609_344).as_millimetres_per_second(),
            4_470
        );
        assert_eq!(round_div_i32(5, 2), 3);
        assert_eq!(round_div_i32(-5, 2), -3);

        assert_eq!(Voltage::from_volts(126).as_millivolts(), 126_000);
        assert_eq!(Voltage::from_millivolts(84_400).as_whole_volts(), 84);
        assert_eq!(
            <Voltage as crate::QuantityDisplayValue>::display_value(Voltage::from_millivolts(
                84_400,
            )),
            84
        );
        assert_eq!(Voltage::from_deci_volts(915).as_millivolts(), 91_500);
        assert_eq!(
            Current::from_milliamps(-1_700).abs(),
            Current::from_milliamps(1_700)
        );
        assert_eq!(
            Temperature::from_mpu6050_counts(0).as_millicelsius(),
            36_530
        );
        assert_eq!(
            Voltage::from_millivolts(91_000).as_cell_voltage(SeriesCount::new(30)),
            CellVoltage::from_microvolts(3_033_333)
        );
        let voltage_range = Voltage::from_volts(91)..=Voltage::from_volts(126);
        assert_eq!(
            Voltage::from_millivolts(108_500)
                .percent_of_range(&voltage_range)
                .as_percent(),
            50
        );
        assert_eq!(Voltage::from_centivolts(9_150).as_millivolts(), 91_500);
        assert_eq!(
            Voltage::from_cell_voltage(CellVoltage::from_microvolts(3_050_000), 30).as_millivolts(),
            91_500
        );
        assert_eq!(
            Capacity::from_parallel_packs(5_000, ParallelCount::new(2)).as_milliamp_hours(),
            10_000
        );
        assert_eq!(
            Energy::from_cell_geometry(18, SeriesCount::new(20), ParallelCount::new(2))
                .as_watt_hours(),
            720
        );

        assert_eq!(Current::from_amps(-12).as_milliamps(), -12_000);
        assert_eq!(Current::from_centiamps(-1_240).as_milliamps(), -12_400);
        assert_eq!(Current::from_deciamps(-124).as_milliamps(), -12_400);
        assert_eq!(Current::from_milliamps(-12_400).as_whole_amps(), -12);
        assert_eq!(Current::from_milliamps(-12_400).as_abs_whole_amps(), 12);
        assert_eq!(BatteryLevel::from_percent_i32(-1).as_percent(), 0);
        assert_eq!(BatteryLevel::from_percent_i32(120).as_percent(), 100);
        assert_eq!(
            <BatteryLevel as crate::PercentQuantity>::as_percent(BatteryLevel::from_percent(75)),
            75
        );
        assert_eq!(
            <BatteryLevel as crate::QuantityDisplayValue>::display_value(
                BatteryLevel::from_percent(42)
            ),
            42
        );
        assert!((BatteryLevel::from_percent(75).as_ratio() - 0.75).abs() < f64::EPSILON);
        assert_eq!(
            BatteryLevel::interpolate(
                BatteryLevel::from_percent(20),
                BatteryLevel::from_percent(80),
                50,
                0,
                100,
            )
            .as_percent(),
            50
        );
        assert_eq!(
            BatteryLevel::from_piecewise_linear(
                5_440,
                &[
                    (5_120, BatteryLevel::from_percent(0)),
                    (5_440, BatteryLevel::from_percent(9)),
                    (6_680, BatteryLevel::from_percent(100)),
                ],
            )
            .as_percent(),
            9
        );
    }

    #[test]
    fn quantity_conversions_cover_angles_ratios_and_power() {
        assert_eq!(Temperature::from_celsius(36).as_millicelsius(), 36_000);
        assert_eq!(
            Temperature::from_centi_celsius(-3_660).as_millicelsius(),
            -36_600
        );
        assert_eq!(
            Temperature::from_millicelsius(-36_600).as_abs_whole_celsius(),
            37
        );
        assert_eq!(Duration::from_deciseconds(12).as_milliseconds(), 1_200);

        assert_eq!(Angle::from_degrees(69).as_millidegrees(), 69_000);
        assert_eq!(Angle::from_deci_degrees(690).as_millidegrees(), 6_900);
        assert_eq!(Angle::from_millidegrees(69_060).as_whole_degrees(), 69);

        assert_eq!(DutyCycle::from_decipermille(524).as_permille(), 52);
        assert_eq!(DutyCycle::from_centered_pwm(0).as_permille(), -1_000);
        assert_eq!(DutyCycle::from_centered_pwm(0x8000).as_permille(), 0);
        assert_eq!(DutyCycle::from_centered_pwm(u16::MAX).as_permille(), 999);

        assert_eq!(
            Power::from_voltage_current(Voltage::from_volts(53), Current::from_amps(-6)),
            Power::from_watts(-318)
        );
        assert_eq!(
            Power::from_voltage_current(
                Voltage::from_millivolts(i32::MAX),
                Current::from_milliamps(i32::MAX),
            ),
            Power::from_milliwatts(4_611_686_014_132_420)
        );
        assert_eq!(DutyCycle::from_centipercent(755).as_permille(), 75);
    }

    #[test]
    fn whole_unit_constructors_saturate_at_storage_bounds() {
        assert_eq!(Voltage::from_volts(u64::MAX).as_millivolts(), i32::MAX);
        assert_eq!(Current::from_amps(i64::MAX).as_milliamps(), i32::MAX);
        assert_eq!(Current::from_amps(i64::MIN).as_milliamps(), i32::MIN);
        assert_eq!(
            Temperature::from_celsius(i64::MAX).as_millicelsius(),
            i32::MAX
        );
        assert_eq!(Angle::from_degrees(i64::MIN).as_millidegrees(), i32::MIN);
    }

    #[test]
    fn telemetry_delta_updates_only_present_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        let first = TelemetryDelta {
            at_ms: ms(100),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                1_500,
            ))),
            voltage: Some(Measured::reported(Voltage::from_millivolts(81_000))),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(-2_000))),
            ..TelemetryDelta::empty(ms(100))
        };
        let second = TelemetryDelta {
            at_ms: ms(150),
            motor_temperature: Some(Measured::reported(Temperature::from_millicelsius(42_500))),
            ..TelemetryDelta::empty(ms(150))
        };

        snapshot.apply_delta(first);
        snapshot.apply_delta(second);

        assert_eq!(snapshot.at_ms, Some(ms(150)));
        assert_eq!(
            snapshot.speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(
                1_500
            )))
        );
        assert_eq!(
            snapshot.voltage,
            Some(Measured::reported(Voltage::from_millivolts(81_000)))
        );
        assert_eq!(
            snapshot.motor_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(42_500)))
        );
    }

    #[test]
    fn telemetry_delta_updates_footpad_without_clearing_other_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(TelemetryDelta {
            at_ms: ms(100),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                1_500,
            ))),
            ..TelemetryDelta::empty(ms(100))
        });
        snapshot.apply_delta(TelemetryDelta {
            at_ms: ms(150),
            footpad: Some(FootpadTelemetry {
                state: 3,
                contact_state: None,
                adc1_milliunits: Some(1_250),
                adc2_milliunits: Some(875),
            }),
            ..TelemetryDelta::empty(ms(150))
        });

        assert_eq!(snapshot.at_ms, Some(ms(150)));
        assert_eq!(
            snapshot.speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(
                1_500
            )))
        );
        assert_eq!(
            snapshot.footpad,
            Some(FootpadTelemetry {
                state: 3,
                contact_state: None,
                adc1_milliunits: Some(1_250),
                adc2_milliunits: Some(875),
            })
        );
    }

    #[test]
    fn zero_measurement_is_not_unknown() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(TelemetryDelta {
            at_ms: ms(200),
            speed: Some(Measured::reported(Speed::from_millimetres_per_second(0))),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(0))),
            ..TelemetryDelta::empty(ms(200))
        });

        assert_eq!(
            snapshot.speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(0)))
        );
        assert_eq!(
            snapshot.battery_current,
            Some(Measured::reported(BatteryCurrent::from_milliamps(0)))
        );
        assert_eq!(snapshot.motor_current, None);
    }

    #[test]
    fn duration_quantity_converts_protocol_time_units_to_milliseconds() {
        assert_eq!(Duration::from_milliseconds(750).as_milliseconds(), 750);
        assert_eq!(Duration::from_seconds(11).as_milliseconds(), 11_000);
        assert_eq!(Duration::from_minutes(15).as_milliseconds(), 900_000);
        assert_eq!(Duration::from_minutes(15).as_seconds(), 900);
        assert_eq!(Duration::from_minutes(15).as_minutes(), 15);
    }

    #[test]
    fn battery_quantity_types_preserve_capacity_and_energy_units() {
        assert_eq!(
            Capacity::from_milliamp_hours(10_000).as_milliamp_hours(),
            10_000
        );
        assert_eq!(Energy::from_watt_hours(900).as_watt_hours(), 900);
        assert_eq!(BatteryCurrent::from_milliamps(1_250).as_milliamps(), 1_250);
        assert_eq!(PhaseCurrent::from_amps_f32(-1.25).as_milliamps(), -1_250);
        assert_eq!(PeakCurrent::from_milliamps(1_250).as_milliamps(), 1_250);
        assert_eq!(PeakCurrent::from_amps_f32(-1.25).as_milliamps(), -1_250);
    }

    #[test]
    fn signal_strength_quantity_preserves_dbm_unit() {
        let signal = crate::SignalStrength::from_dbm(-61);
        assert_eq!(signal.as_dbm(), -61);
        assert_eq!(signal.as_quality_percent(), 78);
        assert_eq!(
            <crate::SignalStrength as crate::QuantityDisplayValue>::display_value(signal),
            78
        );
        assert_eq!(
            crate::SignalStrength::from_dbm(-120).as_quality_percent(),
            0
        );
    }

    #[test]
    fn rotational_speed_quantity_preserves_erpm_unit() {
        assert_eq!(crate::RotationalSpeed::from_erpm(4_500).as_erpm(), 4_500);
    }

    #[test]
    fn rotational_speed_quantity_converts_to_linear_speed_with_drive_geometry() {
        let wheel = Distance::from_millimetres(2_100);

        assert_eq!(
            crate::RotationalSpeed::from_erpm(4_500).as_speed(15, 1, wheel),
            Some(Speed::from_millimetres_per_second(10_500))
        );
        assert_eq!(
            crate::RotationalSpeed::from_erpm(4_500).as_speed(15, 2, wheel),
            Some(Speed::from_millimetres_per_second(5_250))
        );
        assert_eq!(
            crate::RotationalSpeed::from_erpm(4_500).as_speed(0, 1, wheel),
            None
        );
    }

    #[test]
    fn tachometer_reading_quantity_preserves_signed_counts() {
        assert_eq!(
            crate::TachometerReading::from_counts(-21_973).as_counts(),
            -21_973
        );
    }

    #[test]
    fn distance_offset_quantity_preserves_signed_length_unit() {
        assert_eq!(
            crate::DistanceOffset::from_metres(805).as_millimetres(),
            805_000
        );
        assert_eq!(
            crate::DistanceOffset::from_metres(-2).as_millimetres(),
            -2_000
        );
    }

    #[test]
    fn measured_constructors_preserve_provenance_and_verification() {
        let reported = Measured::reported(7);
        let calculated = Measured::calculated(11);
        let estimated = Measured::estimated(13);

        assert_eq!(reported.source, ValueSource::Reported);
        assert_eq!(reported.quality, ValueQuality::Known);
        assert_eq!(reported.verification, VerificationStatus::HardwareVerified);

        assert_eq!(calculated.source, ValueSource::Calculated);
        assert_eq!(calculated.quality, ValueQuality::Known);
        assert_eq!(calculated.verification, VerificationStatus::Inferred);

        assert_eq!(estimated.source, ValueSource::Estimated);
        assert_eq!(estimated.quality, ValueQuality::Inferred);
        assert_eq!(estimated.verification, VerificationStatus::Inferred);
    }

    #[test]
    fn telemetry_keeps_distinct_current_temperature_and_estimate_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        let estimated_level = Measured::estimated(BatteryLevel::from_percent(76));

        snapshot.apply_delta(TelemetryDelta {
            at_ms: ms(300),
            battery_current: Some(Measured::reported(BatteryCurrent::from_milliamps(-1_200))),
            motor_current: Some(Measured::reported(PhaseCurrent::from_milliamps(3_400))),
            controller_temperature: Some(Measured::reported(Temperature::from_millicelsius(
                35_000,
            ))),
            motor_temperature: Some(Measured::reported(Temperature::from_millicelsius(45_000))),
            battery_temperature: Some(Measured::reported(Temperature::from_millicelsius(31_000))),
            battery_level_reported: Some(Measured::reported(BatteryLevel::from_percent(80))),
            battery_level_estimated: Some(estimated_level),
            ..TelemetryDelta::empty(ms(300))
        });

        assert_eq!(
            snapshot.battery_current,
            Some(Measured::reported(BatteryCurrent::from_milliamps(-1_200)))
        );
        assert_eq!(
            snapshot.motor_current,
            Some(Measured::reported(PhaseCurrent::from_milliamps(3_400)))
        );
        assert_eq!(
            snapshot.controller_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(35_000)))
        );
        assert_eq!(
            snapshot.motor_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(45_000)))
        );
        assert_eq!(
            snapshot.battery_temperature,
            Some(Measured::reported(Temperature::from_millicelsius(31_000)))
        );
        assert_eq!(
            snapshot.battery_level_reported,
            Some(Measured::reported(BatteryLevel::from_percent(80)))
        );
        assert_eq!(snapshot.battery_level_estimated, Some(estimated_level));
        assert_eq!(
            snapshot
                .battery_level_estimated
                .map(|value| value.verification),
            Some(VerificationStatus::Inferred)
        );
    }

    #[test]
    fn telemetry_delta_can_be_emitted_as_device_event() {
        let delta = TelemetryDelta {
            at_ms: ms(400),
            distance: Some(Measured::reported(Distance::from_millimetres(12_345))),
            ..TelemetryDelta::empty(ms(400))
        };

        assert_eq!(
            DeviceEvent::Telemetry(delta),
            DeviceEvent::Telemetry(TelemetryDelta {
                at_ms: ms(400),
                distance: Some(Measured::reported(Distance::from_millimetres(12_345))),
                ..TelemetryDelta::empty(ms(400))
            })
        );
    }

    #[test]
    fn firmware_response_preserves_version_fields_and_evidence() {
        let response = crate::FirmwareInfo {
            protocol_version: Some(Measured::reported(3)),
            firmware_major: Some(Measured::reported(1)),
            firmware_minor: Some(Measured::reported(14)),
            firmware_patch: None,
            build_id: Some(crate::RawFieldValue::new(0x20, 0x0000_1234)),
        };

        assert_eq!(response.protocol_version, Some(Measured::reported(3)));
        assert_eq!(response.firmware_patch, None);
        assert_eq!(
            response.build_id,
            Some(crate::RawFieldValue::new(0x20, 0x0000_1234))
        );
    }

    #[test]
    fn battery_response_distinguishes_reported_estimated_and_unknown_percent() {
        let battery = crate::BatteryInfo {
            voltage: Some(Measured::reported(Voltage::from_millivolts(80_400))),
            current: Some(Measured::reported(BatteryCurrent::from_milliamps(0))),
            level_reported: Some(Measured::reported(BatteryLevel::from_percent(0))),
            level_estimated: Some(Measured::estimated(BatteryLevel::from_percent(42))),
            temperature: None,
            raw_state: None,
        };
        let response = crate::BatteryPagePayload::Raw(crate::BatteryRawPage::new(
            crate::BatteryPageMetadata::raw(
                crate::ProtocolSelector::new(8),
                VerificationStatus::SourceVerified,
            ),
            battery,
        ));

        assert_eq!(
            response.page(),
            crate::BatteryPageMetadata::raw(
                crate::ProtocolSelector::new(8),
                VerificationStatus::SourceVerified,
            )
        );
        assert_eq!(
            response.battery().current,
            Some(Measured::reported(BatteryCurrent::from_milliamps(0)))
        );
        assert_eq!(
            response.battery().level_reported,
            Some(Measured::reported(BatteryLevel::from_percent(0)))
        );
        assert_eq!(
            response.battery().voltage.map(|value| value.verification),
            Some(VerificationStatus::HardwareVerified)
        );
        assert_eq!(
            response
                .battery()
                .level_estimated
                .map(|value| value.verification),
            Some(VerificationStatus::Inferred)
        );
        assert_eq!(response.battery().temperature, None);
    }

    #[test]
    fn diagnostic_detail_preserves_raw_field_identifier_and_severity() {
        let detail = crate::DiagnosticDetail {
            field: crate::RawFieldValue::new(0x55, -7),
            severity: crate::DiagnosticSeverity::Warning,
            quality: ValueQuality::Inferred,
            verification: VerificationStatus::Inferred,
        };

        assert_eq!(detail.field.id, 0x55);
        assert_eq!(detail.field.value, -7);
        assert_eq!(detail.severity, crate::DiagnosticSeverity::Warning);
        assert_eq!(detail.quality, ValueQuality::Inferred);
        assert_eq!(detail.verification, VerificationStatus::Inferred);
    }

    #[test]
    fn settings_readback_entry_carries_numeric_values_without_writes() {
        let entry = crate::SettingsEntry {
            field: crate::RawFieldValue::new(0x10, 2),
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        };
        let response = crate::SettingsReadback::available([Some(entry), None, None, None]);

        assert_eq!(
            response.availability(),
            crate::SettingsReadbackAvailability::Available
        );
        assert_eq!(response.entries()[0], Some(entry));
        assert_eq!(response.entries()[1], None);
        assert_eq!(
            response.entries()[0].map(|entry| entry.verification),
            Some(VerificationStatus::HardwareVerified)
        );
    }

    #[test]
    fn fault_history_readback_separates_unknown_code_from_since_distance() {
        let code = crate::FaultCode::unknown(crate::RawFieldValue::new(0x0040, 1));
        let last_fault = crate::FaultHistoryEntry::reported_unknown(code);
        let distance = Measured::reported(Distance::from_millimetres(61_456_941));
        let readback = crate::FaultHistoryReadback::fault_since(last_fault, Some(distance));

        assert_eq!(
            readback.availability(),
            crate::FaultHistoryAvailability::Available
        );
        assert_eq!(readback.last_fault(), Some(last_fault));
        assert_eq!(readback.last_fault().expect("fault").code, code);
        assert_eq!(readback.since_distance(), Some(distance));
        assert_eq!(
            crate::FaultHistoryReadback::no_fault_since(distance).last_fault(),
            None
        );
        assert_eq!(
            crate::FaultHistoryReadback::unavailable().availability(),
            crate::FaultHistoryAvailability::Unavailable
        );
        assert_eq!(
            crate::FaultHistoryReadback::unsupported().availability(),
            crate::FaultHistoryAvailability::Unsupported
        );
    }

    #[test]
    fn registry_entry_represents_capture_backed_aero_metadata() {
        const AERO_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            characteristic: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            roles: crate::GattRoles::empty()
                .with_read()
                .with_write()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::HardwareVerified,
        }];
        const AERO_BMS_SELECTORS: [crate::BmsPageSelectorSpec; 2] = [
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(0),
                kind: crate::BatteryPageKind::Metadata,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(1),
                kind: crate::BatteryPageKind::CellVoltage,
                verification: VerificationStatus::HardwareVerified,
            },
        ];
        let entry = crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("NOSFET"),
            model: crate::ModelKey::new("Aero"),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["NF2557"],
            wire_model_id: Some(crate::VerifiedValue {
                value: 43_u16,
                verification: VerificationStatus::HardwareVerified,
            }),
            battery: Some(crate::BatterySpec {
                series_cells: SeriesCount::new(30),
                nominal_capacity: Some(Capacity::from_milliamp_hours(10_000)),
                voltage_range: Voltage::from_millivolts(99_180)..=Voltage::from_millivolts(123_370),
                verification: VerificationStatus::SourceAndHardwareVerified,
            }),
            bms: Some(crate::BmsLayoutSpec {
                series_cells: SeriesCount::new(30),
                parallel_packs: ParallelCount::new(2),
                cell_values_per_page: crate::BmsCellValuesPerPage::new(15),
                temperature_values_per_page: crate::BmsTemperatureValuesPerPage::new(6),
                selectors: &AERO_BMS_SELECTORS,
                verification: VerificationStatus::HardwareVerified,
            }),
            gatt: &AERO_GATT,
            capabilities: crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
                crate::CommandKind::RequestFirmwareInfo,
                crate::CommandKind::RequestBatteryInfo,
                crate::CommandKind::RequestFaultHistory,
            ]),
            verification: VerificationStatus::HardwareVerified,
        };

        assert_eq!(entry.manufacturer, "NOSFET");
        assert_eq!(
            entry.protocol_family,
            crate::ProtocolFamily::VeteranLeaperkimNosfet
        );
        assert!(
            entry
                .capabilities
                .supports_command_kind(crate::CommandKind::RequestTelemetry)
        );
        assert!(
            !entry
                .capabilities
                .supports_command_kind(crate::CommandKind::SetRawMotorCurrent)
        );
        assert_eq!(entry.wire_model_id.map(|model_id| model_id.value), Some(43));
        assert!(entry.gatt[0].roles.supports_read());
        assert!(entry.gatt[0].roles.supports_write());
        assert!(entry.gatt[0].roles.supports_write_without_response());
        assert!(entry.gatt[0].roles.supports_notify());
        assert!(!entry.gatt[0].roles.supports_indicate());
        let bms = entry
            .bms
            .expect("Aero registry entry should carry BMS layout");
        assert_eq!(bms.series_cells, SeriesCount::new(30));
        assert_eq!(bms.parallel_packs, ParallelCount::new(2));
        assert_eq!(bms.selectors[1].kind, crate::BatteryPageKind::CellVoltage);
    }

    #[test]
    fn registry_hash_is_stable_for_same_entries() {
        let entry = sample_registry_entry("NOSFET", "Aero");

        assert_eq!(
            crate::registry_entries_hash(&[&entry]),
            crate::registry_entries_hash(&[&entry])
        );
    }

    #[test]
    fn registry_hash_changes_when_entry_metadata_changes() {
        let aero = sample_registry_entry("NOSFET", "Aero");
        let aeon = sample_registry_entry("NOSFET", "Aeon");

        assert_ne!(
            crate::registry_entries_hash(&[&aero]),
            crate::registry_entries_hash(&[&aeon])
        );
    }

    #[test]
    fn registry_hash_changes_when_bms_layout_changes() {
        let without_bms = sample_registry_entry("NOSFET", "Aero");
        let with_bms = sample_registry_entry_with_bms("NOSFET", "Aero", 30, 2);

        assert_ne!(
            crate::registry_entries_hash(&[&without_bms]),
            crate::registry_entries_hash(&[&with_bms])
        );
    }

    #[test]
    fn registry_validation_accepts_well_formed_entries() {
        let aero = sample_registry_entry("NOSFET", "Aero");
        let falcon = sample_registry_entry("Begode", "Falcon");

        assert_eq!(crate::validate_registry_entries(&[&aero, &falcon]), Ok(()));
    }

    #[test]
    fn registry_validation_rejects_empty_manufacturer_or_model() {
        let empty_manufacturer = sample_registry_entry("", "Aero");
        let empty_model = sample_registry_entry("NOSFET", "");

        assert_eq!(
            crate::validate_registry_entries(&[&empty_manufacturer]),
            Err(crate::RegistryValidationError::EmptyManufacturer { index: 0 })
        );
        assert_eq!(
            crate::validate_registry_entries(&[&empty_model]),
            Err(crate::RegistryValidationError::EmptyModel { index: 0 })
        );
    }

    #[test]
    fn registry_validation_rejects_duplicate_model_keys() {
        let first = sample_registry_entry("NOSFET", "Aero");
        let duplicate = sample_registry_entry("NOSFET", "Aero");

        assert_eq!(
            crate::validate_registry_entries(&[&first, &duplicate]),
            Err(crate::RegistryValidationError::DuplicateModel {
                index: 1,
                first_index: 0,
            })
        );
    }

    #[test]
    fn registry_validation_rejects_conflicting_wire_model_claims() {
        let mut first = sample_registry_entry("NOSFET", "Aero");
        first.wire_model_id = Some(crate::VerifiedValue {
            value: 43,
            verification: VerificationStatus::HardwareVerified,
        });
        let mut duplicate_wire_id = sample_registry_entry("NOSFET", "Aero Pro");
        duplicate_wire_id.wire_model_id = Some(crate::VerifiedValue {
            value: 43,
            verification: VerificationStatus::Inferred,
        });

        assert_eq!(
            crate::validate_registry_entries(&[&first, &duplicate_wire_id]),
            Err(crate::RegistryValidationError::ConflictingWireModelId {
                index: 1,
                first_index: 0,
            })
        );
    }

    #[test]
    fn registry_validation_rejects_missing_gatt_fingerprint() {
        let mut entry = sample_registry_entry("NOSFET", "Aero");
        entry.gatt = &[];

        assert_eq!(
            crate::validate_registry_entries(&[&entry]),
            Err(crate::RegistryValidationError::MissingGattFingerprint { index: 0 })
        );
    }

    #[test]
    fn registry_validation_rejects_empty_capabilities() {
        let mut entry = sample_registry_entry("NOSFET", "Aero");
        entry.capabilities = crate::Capabilities::default();

        assert_eq!(
            crate::validate_registry_entries(&[&entry]),
            Err(crate::RegistryValidationError::EmptyCapabilities { index: 0 })
        );
    }

    #[test]
    fn model_authoring_emits_static_registry_and_catalog_entries() {
        const AUTHORING: crate::CompleteModelAuthoring = crate::ModelAuthoring::new()
            .manufacturer(crate::ManufacturerKey::new("TypedCo"))
            .model(crate::ModelKey::new("Typed Model"))
            .family(crate::FamilyKey::new(
                crate::ProtocolFamily::VeteranLeaperkimNosfet,
            ))
            .advertised_name_hints(&["TypedHint"])
            .gatt(&STATIC_SAMPLE_GATT)
            .capabilities(crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
            ]))
            .verification(VerificationStatus::Inferred)
            .active_runtime(
                crate::ParserKey::new("typed-parser"),
                crate::SessionKey::new("typed-session"),
            );
        static AUTHORED_MODEL: crate::ModelRegistryEntry = AUTHORING.registry_entry();
        const CATALOG: [crate::ModelCatalogEntry; 1] = [AUTHORING.catalog_entry(&AUTHORED_MODEL)];

        assert_eq!(crate::validate_model_catalog(&CATALOG), Ok(()));
        assert_eq!(
            crate::ModelCatalog::new(&CATALOG)
                .find_model(
                    crate::ManufacturerKey::new("TypedCo"),
                    crate::ModelKey::new("Typed Model")
                )
                .map(|entry| entry.registration.parser),
            Some(Some(crate::ParserKey::new("typed-parser")))
        );
    }

    #[test]
    fn catalog_entry_exposes_typed_keys_for_common_model_path() {
        let catalog = crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        };

        assert_eq!(catalog.manufacturer_key().as_str(), "NOSFET");
        assert_eq!(catalog.model_key().as_str(), "Aero");
        assert_eq!(
            catalog.family_key().protocol_family(),
            crate::ProtocolFamily::VeteranLeaperkimNosfet
        );
        assert_eq!(crate::validate_model_catalog(&[catalog]), Ok(()));
    }

    #[test]
    fn catalog_validation_rejects_missing_active_registrations() {
        let missing_parser = crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: None,
                session: Some(crate::SessionKey::new("test-session")),
            },
        };
        let missing_session = crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: None,
            },
        };

        assert_eq!(
            crate::validate_model_catalog(&[missing_parser]),
            Err(crate::RegistryValidationError::MissingParserRegistration { index: 0 })
        );
        assert_eq!(
            crate::validate_model_catalog(&[missing_session]),
            Err(crate::RegistryValidationError::MissingSessionRegistration { index: 0 })
        );
    }

    #[test]
    fn catalog_lookup_uses_typed_keys_over_static_entries() {
        const CATALOG: [crate::ModelCatalogEntry; 1] = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&CATALOG);

        assert_eq!(
            catalog
                .find_model(
                    crate::ManufacturerKey::new("NOSFET"),
                    crate::ModelKey::new("Aero")
                )
                .map(|entry| entry.registry.model),
            Some(crate::ModelKey::new("Aero"))
        );
        assert_eq!(
            catalog
                .family_entries(crate::FamilyKey::new(
                    crate::ProtocolFamily::VeteranLeaperkimNosfet
                ))
                .count(),
            1
        );
    }

    #[test]
    fn catalog_lookup_finds_registered_parser_and_session_keys() {
        const CATALOG: [crate::ModelCatalogEntry; 1] = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&CATALOG);

        assert_eq!(
            catalog
                .find_parser(crate::ParserKey::new("test-parser"))
                .map(|entry| entry.model_key()),
            Some(crate::ModelKey::new("Aero"))
        );
        assert_eq!(
            catalog
                .find_session(crate::SessionKey::new("test-session"))
                .map(|entry| entry.model_key()),
            Some(crate::ModelKey::new("Aero"))
        );
        assert!(
            catalog
                .find_parser(crate::ParserKey::new("missing"))
                .is_none()
        );
        assert!(
            catalog
                .find_session(crate::SessionKey::new("missing"))
                .is_none()
        );
    }

    #[test]
    fn catalog_resolves_borrowed_display_model_without_allocating_keys() {
        let entries = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&entries);

        assert_eq!(
            catalog
                .find_model_names("NOSFET", "Aero")
                .map(|entry| entry.model_key()),
            Some(crate::ModelKey::new("Aero"))
        );
        let crate::CatalogModelResolution::Matched(entry) = catalog
            .resolve_display_model(crate::ProtocolFamily::VeteranLeaperkimNosfet, "NOSFET Aero")
        else {
            panic!("display model should resolve");
        };
        assert_eq!(entry.model_key(), crate::ModelKey::new("Aero"));

        assert!(matches!(
            catalog.resolve_display_model(crate::ProtocolFamily::BegodeGotway, "NOSFET Aero"),
            crate::CatalogModelResolution::NoMatch
        ));
    }

    #[test]
    fn catalog_resolves_advertised_name_hints_without_allocating_keys() {
        let entries = [crate::ModelCatalogEntry {
            registry: &STATIC_AERO_REGISTRY_ENTRY,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new("test-parser")),
                session: Some(crate::SessionKey::new("test-session")),
            },
        }];
        let catalog = crate::ModelCatalog::new(&entries);

        let crate::CatalogModelResolution::Matched(entry) =
            catalog.resolve_advertised_name("Aero NF2557")
        else {
            panic!("advertised name should resolve through registry hints");
        };

        assert_eq!(
            entry.manufacturer_key(),
            crate::ManufacturerKey::new("NOSFET")
        );
        assert_eq!(entry.model_key(), crate::ModelKey::new("Aero"));
        assert!(matches!(
            catalog.resolve_advertised_name("mystery device"),
            crate::CatalogModelResolution::NoMatch
        ));
    }

    #[test]
    fn catalog_display_model_resolution_reports_ambiguity() {
        static OTHER_AERO_REGISTRY_ENTRY: crate::ModelRegistryEntry = crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("Other"),
            model: crate::ModelKey::new("Aero"),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &[],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: STATIC_AERO_REGISTRY_ENTRY.gatt,
            capabilities: STATIC_AERO_REGISTRY_ENTRY.capabilities,
            verification: VerificationStatus::Inferred,
        };
        let entries = [
            crate::ModelCatalogEntry {
                registry: &STATIC_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("test-parser")),
                    session: Some(crate::SessionKey::new("test-session")),
                },
            },
            crate::ModelCatalogEntry {
                registry: &OTHER_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("other-parser")),
                    session: Some(crate::SessionKey::new("other-session")),
                },
            },
        ];
        let catalog = crate::ModelCatalog::new(&entries);

        assert!(matches!(
            catalog.resolve_display_model(crate::ProtocolFamily::VeteranLeaperkimNosfet, "Aero"),
            crate::CatalogModelResolution::Ambiguous
        ));
    }

    #[test]
    fn catalog_advertised_name_resolution_reports_ambiguity() {
        static OTHER_AERO_REGISTRY_ENTRY: crate::ModelRegistryEntry = crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("Other"),
            model: crate::ModelKey::new("Shared"),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["NF2557"],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: STATIC_AERO_REGISTRY_ENTRY.gatt,
            capabilities: STATIC_AERO_REGISTRY_ENTRY.capabilities,
            verification: VerificationStatus::Inferred,
        };
        let entries = [
            crate::ModelCatalogEntry {
                registry: &STATIC_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("test-parser")),
                    session: Some(crate::SessionKey::new("test-session")),
                },
            },
            crate::ModelCatalogEntry {
                registry: &OTHER_AERO_REGISTRY_ENTRY,
                registration: crate::ModelRuntimeRegistration {
                    parser: Some(crate::ParserKey::new("other-parser")),
                    session: Some(crate::SessionKey::new("other-session")),
                },
            },
        ];
        let catalog = crate::ModelCatalog::new(&entries);

        assert!(matches!(
            catalog.resolve_advertised_name("NF2557"),
            crate::CatalogModelResolution::Ambiguous
        ));
    }

    #[test]
    fn synthetic_catalog_scales_to_one_thousand_models() {
        const MODEL_COUNT: usize = 1_000;
        let entries: Vec<_> = (0..MODEL_COUNT).map(synthetic_catalog_entry).collect();
        let registry_entries: Vec<_> = entries.iter().map(|entry| entry.registry).collect();
        let catalog = crate::ModelCatalog::new(&entries);

        assert_eq!(crate::validate_registry_entries(&registry_entries), Ok(()));
        assert_eq!(crate::validate_model_catalog(&entries), Ok(()));
        assert_eq!(
            catalog
                .find_model_names("Synthetic", "Model0999")
                .map(|entry| entry.registration.session),
            Some(Some(crate::SessionKey::new("synthetic-session-0999")))
        );
        let crate::CatalogModelResolution::Matched(entry) =
            catalog.resolve_advertised_name("PEV-0999")
        else {
            panic!("unique synthetic advertised hint should resolve");
        };
        assert_eq!(entry.registry.model, "Model0999");
    }

    #[test]
    fn catalog_ambiguity_is_independent_of_registration_order() {
        let mut forward = vec![
            synthetic_catalog_entry_with_hint(1, "shared-hint"),
            synthetic_catalog_entry_with_hint(2, "shared-hint"),
        ];
        let mut reversed = forward.clone();
        reversed.reverse();

        assert!(matches!(
            crate::ModelCatalog::new(&forward).resolve_advertised_name("shared-hint"),
            crate::CatalogModelResolution::Ambiguous
        ));
        assert!(matches!(
            crate::ModelCatalog::new(&reversed).resolve_advertised_name("shared-hint"),
            crate::CatalogModelResolution::Ambiguous
        ));

        forward.reverse();
        assert_eq!(
            crate::validate_model_catalog(&forward),
            Ok(()),
            "shared advertised hints are ambiguous identity evidence, not invalid metadata"
        );
    }

    #[test]
    fn registry_validation_rejects_invalid_gatt_fingerprints() {
        const INVALID_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([0x11; 16]),
            characteristic: GattChannel::from_bytes([0x22; 16]),
            roles: crate::GattRoles::empty(),
            verification: VerificationStatus::SourceVerified,
        }];
        let mut entry = sample_registry_entry("NOSFET", "Aero");
        entry.gatt = &INVALID_GATT;

        assert_eq!(
            crate::validate_registry_entries(&[&entry]),
            Err(crate::RegistryValidationError::InvalidGattFingerprint {
                index: 0,
                fingerprint_index: 0,
            })
        );
    }

    #[test]
    fn bms_layout_spec_preserves_static_selector_map() {
        const SELECTORS: [crate::BmsPageSelectorSpec; 4] = [
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(0),
                kind: crate::BatteryPageKind::Metadata,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(1),
                kind: crate::BatteryPageKind::CellVoltage,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(3),
                kind: crate::BatteryPageKind::Raw,
                verification: VerificationStatus::SourceVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: crate::ProtocolSelector::new(8),
                kind: crate::BatteryPageKind::Raw,
                verification: VerificationStatus::SourceVerified,
            },
        ];
        let layout = crate::BmsLayoutSpec {
            series_cells: SeriesCount::new(30),
            parallel_packs: ParallelCount::new(2),
            cell_values_per_page: crate::BmsCellValuesPerPage::new(15),
            temperature_values_per_page: crate::BmsTemperatureValuesPerPage::new(6),
            selectors: &SELECTORS,
            verification: VerificationStatus::HardwareVerified,
        };

        assert_eq!(layout.selectors.len(), 4);
        assert_eq!(
            layout.selectors[2].selector,
            crate::ProtocolSelector::new(3)
        );
        assert_eq!(layout.selectors[2].kind, crate::BatteryPageKind::Raw);
        assert_eq!(
            layout.selectors[2].verification,
            VerificationStatus::SourceVerified
        );
    }

    #[test]
    fn installed_device_identity_uses_core_bluetooth_id_as_opaque_primary_key() {
        const GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            characteristic: GattChannel::from_bytes([
                0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b,
                0x34, 0xfb,
            ]),
            roles: crate::GattRoles::empty()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::HardwareVerified,
        }];
        let identity = crate::InstalledDeviceIdentity {
            platform_id: crate::InstalledDevicePlatformId {
                platform: crate::InstalledDevicePlatform::CoreBluetooth,
                value: "8de871ff-6aa1-a767-34dd-608e584b610e",
            },
            protocol_serial: Some(crate::VerifiedValue {
                value: "NF2557",
                verification: VerificationStatus::HardwareVerified,
            }),
            user_alias: Some("shop Aero"),
            resolved_model: Some(crate::InstalledDeviceModel {
                manufacturer: "NOSFET",
                model: "Aero",
                protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
                verification: VerificationStatus::HardwareVerified,
            }),
            gatt_fingerprints: &GATT,
        };

        assert_eq!(
            identity.platform_id.platform,
            crate::InstalledDevicePlatform::CoreBluetooth
        );
        assert_eq!(
            identity.platform_id.value,
            "8de871ff-6aa1-a767-34dd-608e584b610e"
        );
        assert_eq!(
            identity.protocol_serial.map(|serial| serial.value),
            Some("NF2557")
        );
        assert_eq!(identity.user_alias, Some("shop Aero"));
        assert_eq!(
            identity
                .resolved_model
                .map(|model| (model.manufacturer, model.model)),
            Some(("NOSFET", "Aero"))
        );
        assert!(identity.gatt_fingerprints[0].roles.supports_notify());
    }

    #[test]
    fn installed_device_identity_treats_android_identifier_as_platform_scoped_opaque_value() {
        let identity = crate::InstalledDeviceIdentity {
            platform_id: crate::InstalledDevicePlatformId {
                platform: crate::InstalledDevicePlatform::Android,
                value: "00:00:00:00:00:00",
            },
            protocol_serial: None,
            user_alias: None,
            resolved_model: Some(crate::InstalledDeviceModel {
                manufacturer: "Begode",
                model: "Falcon",
                protocol_family: crate::ProtocolFamily::BegodeGotway,
                verification: VerificationStatus::Inferred,
            }),
            gatt_fingerprints: &[],
        };

        assert_eq!(
            identity.platform_id.platform,
            crate::InstalledDevicePlatform::Android
        );
        assert_eq!(identity.platform_id.value, "00:00:00:00:00:00");
        assert_eq!(identity.protocol_serial, None);
        assert_eq!(identity.user_alias, None);
        assert_eq!(identity.gatt_fingerprints, &[]);
        assert_eq!(
            identity
                .resolved_model
                .map(|model| (model.protocol_family, model.verification)),
            Some((
                crate::ProtocolFamily::BegodeGotway,
                VerificationStatus::Inferred
            ))
        );
    }

    fn sample_registry_entry(
        manufacturer: &'static str,
        model: &'static str,
    ) -> crate::ModelRegistryEntry {
        const SAMPLE_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
            service: GattChannel::from_bytes([0x11; 16]),
            characteristic: GattChannel::from_bytes([0x22; 16]),
            roles: crate::GattRoles::empty()
                .with_write_without_response()
                .with_notify(),
            verification: VerificationStatus::SourceVerified,
        }];

        crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new(manufacturer),
            model: crate::ModelKey::new(model),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["NF"],
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &SAMPLE_GATT,
            capabilities: crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
            ]),
            verification: VerificationStatus::Inferred,
        }
    }

    fn sample_registry_entry_with_bms(
        manufacturer: &'static str,
        model: &'static str,
        series_cells: u8,
        parallel_packs: u8,
    ) -> crate::ModelRegistryEntry {
        const SELECTORS: [crate::BmsPageSelectorSpec; 1] = [crate::BmsPageSelectorSpec {
            selector: crate::ProtocolSelector::new(1),
            kind: crate::BatteryPageKind::CellVoltage,
            verification: VerificationStatus::SourceVerified,
        }];
        let mut entry = sample_registry_entry(manufacturer, model);
        entry.bms = Some(crate::BmsLayoutSpec {
            series_cells: SeriesCount::new(series_cells),
            parallel_packs: ParallelCount::new(parallel_packs),
            cell_values_per_page: crate::BmsCellValuesPerPage::new(15),
            temperature_values_per_page: crate::BmsTemperatureValuesPerPage::new(6),
            selectors: &SELECTORS,
            verification: VerificationStatus::Inferred,
        });
        entry
    }

    fn synthetic_catalog_entry(index: usize) -> crate::ModelCatalogEntry {
        let hint = leak_static_str(format!("PEV-{index:04}"));
        synthetic_catalog_entry_with_hint(index, hint)
    }

    fn synthetic_catalog_entry_with_hint(
        index: usize,
        hint: &'static str,
    ) -> crate::ModelCatalogEntry {
        let model = leak_static_str(format!("Model{index:04}"));
        let hints = Box::leak(Box::new([hint]));
        let registry = Box::leak(Box::new(crate::ModelRegistryEntry {
            manufacturer: crate::ManufacturerKey::new("Synthetic"),
            model: crate::ModelKey::new(model),
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: hints,
            wire_model_id: None,
            battery: None,
            bms: None,
            gatt: &STATIC_SAMPLE_GATT,
            capabilities: crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
            ]),
            verification: VerificationStatus::Inferred,
        }));

        crate::ModelCatalogEntry {
            registry,
            registration: crate::ModelRuntimeRegistration {
                parser: Some(crate::ParserKey::new(leak_static_str(format!(
                    "synthetic-parser-{index:04}"
                )))),
                session: Some(crate::SessionKey::new(leak_static_str(format!(
                    "synthetic-session-{index:04}"
                )))),
            },
        }
    }

    fn leak_static_str(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }

    const STATIC_SAMPLE_GATT: [crate::GattFingerprint; 1] = [crate::GattFingerprint {
        service: GattChannel::from_bytes([0x11; 16]),
        characteristic: GattChannel::from_bytes([0x22; 16]),
        roles: crate::GattRoles::empty()
            .with_write_without_response()
            .with_notify(),
        verification: VerificationStatus::SourceVerified,
    }];

    static STATIC_AERO_REGISTRY_ENTRY: crate::ModelRegistryEntry = crate::ModelRegistryEntry {
        manufacturer: crate::ManufacturerKey::new("NOSFET"),
        model: crate::ModelKey::new("Aero"),
        protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
        advertised_name_hints: &["NF"],
        wire_model_id: None,
        battery: None,
        bms: None,
        gatt: &STATIC_SAMPLE_GATT,
        capabilities: crate::Capabilities::from_supported_commands([
            crate::CommandKind::RequestIdentity,
            crate::CommandKind::RequestTelemetry,
        ]),
        verification: VerificationStatus::Inferred,
    };

    #[test]
    fn read_only_response_reports_matching_command_kind() {
        let firmware = crate::ReadOnlyResponse::Firmware(crate::FirmwareInfo::default());
        let battery = crate::ReadOnlyResponse::Battery(crate::BatteryReadback::available(
            crate::BatteryPagePayload::Raw(crate::BatteryRawPage::new(
                crate::BatteryPageMetadata::raw(
                    crate::ProtocolSelector::new(8),
                    VerificationStatus::SourceVerified,
                ),
                crate::BatteryInfo::default(),
            )),
        ));
        let diagnostics = crate::ReadOnlyResponse::Diagnostics(crate::DiagnosticReadback {
            details: [None, None, None, None],
        });
        let settings =
            crate::ReadOnlyResponse::Settings(crate::SettingsReadback::available([None; 4]));
        let fault_history =
            crate::ReadOnlyResponse::FaultHistory(crate::FaultHistoryReadback::unavailable());

        assert_eq!(
            firmware.command_kind(),
            crate::CommandKind::RequestFirmwareInfo
        );
        assert_eq!(
            battery.command_kind(),
            crate::CommandKind::RequestBatteryInfo
        );
        assert_eq!(
            diagnostics.command_kind(),
            crate::CommandKind::RequestDiagnostics
        );
        assert_eq!(settings.command_kind(), crate::CommandKind::RequestSettings);
        assert_eq!(
            fault_history.command_kind(),
            crate::CommandKind::RequestFaultHistory
        );
    }

    #[test]
    fn read_only_response_can_be_emitted_as_device_event() {
        let firmware = crate::FirmwareInfo {
            firmware_major: Some(Measured::reported(43)),
            ..crate::FirmwareInfo::default()
        };

        assert_eq!(
            DeviceEvent::ReadOnlyResponse(crate::ReadOnlyResponse::Firmware(firmware)),
            DeviceEvent::ReadOnlyResponse(crate::ReadOnlyResponse::Firmware(crate::FirmwareInfo {
                firmware_major: Some(Measured::reported(43)),
                ..crate::FirmwareInfo::default()
            }))
        );
    }

    #[test]
    fn read_only_commands_have_queryable_metadata() {
        let command = DeviceCommand::RequestTelemetry;
        let metadata = command.metadata();

        assert_eq!(metadata.kind, command.kind());
        assert_eq!(metadata.safety_class, command.safety_class());
        assert_eq!(metadata.kind, crate::CommandKind::RequestTelemetry);
        assert_eq!(metadata.safety_class, crate::SafetyClass::ReadOnly);
    }

    #[test]
    fn read_only_probe_commands_have_distinct_metadata() {
        let probes = [
            (
                DeviceCommand::RequestFirmwareInfo,
                crate::CommandKind::RequestFirmwareInfo,
            ),
            (
                DeviceCommand::RequestBatteryInfo,
                crate::CommandKind::RequestBatteryInfo,
            ),
            (
                DeviceCommand::RequestDiagnostics,
                crate::CommandKind::RequestDiagnostics,
            ),
            (
                DeviceCommand::RequestFaultHistory,
                crate::CommandKind::RequestFaultHistory,
            ),
            (
                DeviceCommand::RequestSettings,
                crate::CommandKind::RequestSettings,
            ),
        ];

        for (command, kind) in probes {
            assert_eq!(
                command.metadata(),
                crate::CommandMetadata {
                    kind,
                    safety_class: crate::SafetyClass::ReadOnly,
                }
            );
        }
    }

    #[test]
    fn capabilities_accept_new_read_only_probe_commands() {
        let capabilities = crate::Capabilities::from_supported_commands([
            crate::CommandKind::RequestFirmwareInfo,
            crate::CommandKind::RequestBatteryInfo,
            crate::CommandKind::RequestDiagnostics,
            crate::CommandKind::RequestFaultHistory,
            crate::CommandKind::RequestSettings,
        ]);

        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestFirmwareInfo),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestFirmwareInfo,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestBatteryInfo),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestBatteryInfo,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestDiagnostics),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestDiagnostics,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestFaultHistory),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestFaultHistory,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestSettings),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestSettings,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
    }

    #[test]
    fn read_only_probe_request_keys_are_distinct() {
        let keys = [
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestKey::new(crate::CommandKind::RequestBatteryInfo),
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestKey::new(crate::CommandKind::RequestFaultHistory),
            crate::RequestKey::new(crate::CommandKind::RequestSettings),
        ];

        for (index, key) in keys.into_iter().enumerate() {
            assert!(!keys[index + 1..].contains(&key));
        }
    }

    #[test]
    fn request_key_preserves_optional_transport_target() {
        let local = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let can = crate::RequestKey::for_target(
            crate::CommandKind::RequestTelemetry,
            crate::RequestTarget::VescCanController {
                controller_id: crate::VescControllerId::new(42),
            },
        );

        assert_eq!(local.command, crate::CommandKind::RequestTelemetry);
        assert_eq!(local.target, crate::RequestTarget::Local);
        assert_eq!(can.command, crate::CommandKind::RequestTelemetry);
        assert_eq!(
            can.target,
            crate::RequestTarget::VescCanController {
                controller_id: crate::VescControllerId::new(42),
            }
        );
        assert_ne!(local, can);
    }

    #[test]
    fn benign_controls_are_distinct_from_read_only_requests() {
        let lights = DeviceCommand::SetLights(LightState::On);
        let horn = DeviceCommand::SoundHorn;

        assert_eq!(lights.kind(), crate::CommandKind::SetLights);
        assert_eq!(horn.kind(), crate::CommandKind::SoundHorn);
        assert_eq!(lights.safety_class(), crate::SafetyClass::BenignControl);
        assert_eq!(horn.safety_class(), crate::SafetyClass::BenignControl);
    }

    #[test]
    fn remaining_euc_setting_intents_are_typed_and_safety_classified() {
        let acceleration =
            DeviceCommand::SetAccelerationAssist(crate::AccelerationAssistState::Enabled);
        let taillight = DeviceCommand::SetTaillight(LightState::On);

        assert_eq!(
            acceleration.kind(),
            crate::CommandKind::SetAccelerationAssist
        );
        assert_eq!(
            acceleration.safety_class(),
            crate::SafetyClass::StationaryOnly
        );
        assert_eq!(taillight.kind(), crate::CommandKind::SetTaillight);
        assert_eq!(taillight.safety_class(), crate::SafetyClass::BenignControl);
    }

    #[test]
    fn veteran_pedal_mode_raw_values_use_the_documented_mapping() {
        assert_eq!(
            crate::PedalMode::from_veteran_raw(0),
            Some(crate::PedalMode::Hard)
        );
        assert_eq!(
            crate::PedalMode::from_veteran_raw(1),
            Some(crate::PedalMode::Medium)
        );
        assert_eq!(
            crate::PedalMode::from_veteran_raw(2),
            Some(crate::PedalMode::Soft)
        );
        assert_eq!(crate::PedalMode::from_veteran_raw(1920), None);
    }

    #[test]
    fn begode_pedal_mode_settings_bits_use_documented_inverted_mapping() {
        assert_eq!(
            crate::PedalMode::from_begode_settings_bits(0x0000),
            Some(crate::PedalMode::Soft)
        );
        assert_eq!(
            crate::PedalMode::from_begode_settings_bits(0x2000),
            Some(crate::PedalMode::Medium)
        );
        assert_eq!(
            crate::PedalMode::from_begode_settings_bits(0x4000),
            Some(crate::PedalMode::Hard)
        );
        assert_eq!(crate::PedalMode::from_begode_settings_bits(0x6000), None);
    }

    #[test]
    fn command_safety_classes_match_control_matrix() {
        let matrix = [
            (
                crate::SafetyClass::ReadOnly,
                &[
                    crate::CommandKind::RequestIdentity,
                    crate::CommandKind::RequestTelemetry,
                    crate::CommandKind::RequestFirmwareInfo,
                    crate::CommandKind::RequestBatteryInfo,
                    crate::CommandKind::RequestDiagnostics,
                    crate::CommandKind::RequestFaultHistory,
                    crate::CommandKind::RequestSettings,
                ][..],
            ),
            (
                crate::SafetyClass::BenignControl,
                &[crate::CommandKind::SetLights, crate::CommandKind::SoundHorn][..],
            ),
            (
                crate::SafetyClass::StationaryOnly,
                &[crate::CommandKind::SetPedalMode][..],
            ),
            (
                crate::SafetyClass::Actuation,
                &[crate::CommandKind::SetRawMotorCurrent][..],
            ),
        ];

        for (safety_class, commands) in matrix {
            for command in commands {
                assert_eq!(command.safety_class(), safety_class);
            }
        }
    }

    #[test]
    fn stationary_settings_policy_only_arms_stationary_states() {
        let policy = crate::StationarySettingsPolicy {
            model: "NOSFET Aero",
            arm_duration: Duration::from_milliseconds(100),
        };

        assert!(
            policy
                .arm(crate::RideOperatingState::Unknown, ms(10))
                .is_none()
        );
        assert!(
            policy
                .arm(crate::RideOperatingState::Riding, ms(10))
                .is_none()
        );
        assert!(
            policy
                .arm(crate::RideOperatingState::Charging, ms(10))
                .is_none()
        );
        assert!(
            policy
                .arm(crate::RideOperatingState::Standing, ms(10))
                .is_some()
        );
        assert!(
            policy
                .arm(crate::RideOperatingState::Parked, ms(10))
                .is_some()
        );
    }

    #[test]
    fn actuation_commands_are_not_supported_without_capability() {
        let capabilities = crate::Capabilities::default();
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(1_000),
        };

        assert_eq!(command.safety_class(), crate::SafetyClass::Actuation);
        assert_eq!(
            capabilities.check_command(command),
            Err(UnsupportedReason::CommandNotSupported(command.kind()))
        );
    }

    #[test]
    fn dangerous_actuation_policy_requires_arm_token() {
        let policy = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(1_000),
        };

        assert_eq!(
            policy.authorize(command, ms(42), None),
            Err(crate::DangerousActuationRefusal::MissingArm)
        );
    }

    #[test]
    fn dangerous_actuation_policy_rejects_expired_or_wrong_model_arms() {
        let falcon = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let aero = crate::DangerousActuationPolicy {
            model: "NOSFET Aero",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(1_000),
        };
        let falcon_arm = falcon.arm(ms(10));
        let aero_arm = aero.arm(ms(10));

        assert_eq!(
            falcon.authorize(command, ms(1_011), Some(falcon_arm)),
            Err(crate::DangerousActuationRefusal::ExpiredArm)
        );
        assert_eq!(
            falcon.authorize(command, ms(42), Some(aero_arm)),
            Err(crate::DangerousActuationRefusal::WrongModel)
        );
    }

    #[test]
    fn dangerous_actuation_policy_rejects_non_actuation_and_over_limit_commands() {
        let policy = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let arm = policy.arm(ms(10));

        assert_eq!(
            policy.authorize(DeviceCommand::SoundHorn, ms(42), Some(arm)),
            Err(crate::DangerousActuationRefusal::WrongSafetyClass)
        );
        assert_eq!(
            policy.authorize(
                DeviceCommand::SetRawMotorCurrent {
                    current: PhaseCurrent::from_milliamps(5_001)
                },
                ms(42),
                Some(arm)
            ),
            Err(crate::DangerousActuationRefusal::CurrentLimitExceeded)
        );
    }

    #[test]
    fn dangerous_actuation_policy_accepts_armed_in_limit_actuation() {
        let policy = crate::DangerousActuationPolicy {
            model: "Begode Falcon",
            max_current: PhaseCurrent::from_milliamps(5_000),
            arm_duration: Duration::from_milliseconds(1_000),
        };
        let command = DeviceCommand::SetRawMotorCurrent {
            current: PhaseCurrent::from_milliamps(-5_000),
        };
        let arm = policy.arm(ms(10));

        assert_eq!(
            policy.authorize(command, ms(1_010), Some(arm)),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::SetRawMotorCurrent,
                safety_class: crate::SafetyClass::Actuation,
            })
        );
    }

    #[test]
    fn hosts_can_query_support_before_writes() {
        let capabilities = crate::Capabilities::from_supported_commands([
            crate::CommandKind::RequestTelemetry,
            crate::CommandKind::SetLights,
        ]);

        assert_eq!(
            capabilities.check_command(DeviceCommand::RequestTelemetry),
            Ok(crate::CommandMetadata {
                kind: crate::CommandKind::RequestTelemetry,
                safety_class: crate::SafetyClass::ReadOnly,
            })
        );
        assert_eq!(
            capabilities.check_command(DeviceCommand::SoundHorn),
            Err(UnsupportedReason::CommandNotSupported(
                crate::CommandKind::SoundHorn
            ))
        );
    }

    #[test]
    fn capability_union_combines_read_and_control_commands() {
        let read =
            crate::Capabilities::from_supported_commands([crate::CommandKind::RequestTelemetry]);
        let control = crate::Capabilities::from_supported_commands([crate::CommandKind::SetLights]);

        let combined = read.union(control);

        assert!(combined.supports_command_kind(crate::CommandKind::RequestTelemetry));
        assert!(combined.supports_command_kind(crate::CommandKind::SetLights));
        assert!(!combined.supports_command_kind(crate::CommandKind::SoundHorn));
    }

    #[test]
    fn parser_limits_reject_oversized_frame_lengths() {
        let limits = crate::ParserLimits {
            max_frame_len: frame_len(24),
            ..crate::ParserLimits::default()
        };

        assert_eq!(limits.validate_frame_len(frame_len(24)), Ok(()));
        assert_eq!(
            limits.validate_frame_len(frame_len(25)),
            Err(crate::ParserError::OversizedFrame {
                claimed: frame_len(25),
                max: frame_len(24),
            })
        );
    }

    #[test]
    fn parser_diagnostics_saturate_counters() {
        let mut diagnostics = crate::ParserDiagnostics {
            dropped_bytes: dropped_bytes(u64::MAX),
            ..crate::ParserDiagnostics::default()
        };

        diagnostics.add_dropped_bytes(dropped_bytes(10));
        diagnostics.record_resync();
        diagnostics.record_error(crate::ParserError::BadChecksum);

        assert_eq!(diagnostics.dropped_bytes, dropped_bytes(u64::MAX));
        assert_eq!(diagnostics.resyncs, diag_count(1));
        assert_eq!(diagnostics.bad_checksums, diag_count(1));
    }

    #[test]
    fn parser_diagnostics_merge_with_saturating_counts() {
        let mut left = crate::ParserDiagnostics {
            timeouts: diag_count(u64::MAX),
            malformed_frames: diag_count(2),
            ..crate::ParserDiagnostics::default()
        };
        let right = crate::ParserDiagnostics {
            timeouts: diag_count(1),
            unmatched_replies: diag_count(3),
            ..crate::ParserDiagnostics::default()
        };

        left.merge(right);

        assert_eq!(left.timeouts, diag_count(u64::MAX));
        assert_eq!(left.malformed_frames, diag_count(2));
        assert_eq!(left.unmatched_replies, diag_count(3));
    }

    #[test]
    fn parser_errors_map_to_expected_diagnostic_counters() {
        let mut diagnostics = crate::ParserDiagnostics::default();

        diagnostics.record_error(crate::ParserError::OversizedFrame {
            claimed: frame_len(4_097),
            max: frame_len(4_096),
        });
        diagnostics.record_error(crate::ParserError::MalformedFrame);
        diagnostics.record_error(crate::ParserError::Timeout {
            elapsed_ms: ms(1_500),
            timeout_ms: ms(1_000),
        });
        diagnostics.record_error(crate::ParserError::UnmatchedReply);

        assert_eq!(diagnostics.oversized_frames, diag_count(1));
        assert_eq!(diagnostics.malformed_frames, diag_count(1));
        assert_eq!(diagnostics.timeouts, diag_count(1));
        assert_eq!(diagnostics.unmatched_replies, diag_count(1));
    }

    #[test]
    fn parser_diagnostics_can_be_emitted_as_device_event() {
        let diagnostics = crate::ParserDiagnostics {
            bad_checksums: diag_count(2),
            resyncs: diag_count(1),
            ..crate::ParserDiagnostics::default()
        };

        assert_eq!(
            DeviceEvent::Diagnostics(diagnostics),
            DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                bad_checksums: diag_count(2),
                resyncs: diag_count(1),
                ..crate::ParserDiagnostics::default()
            })
        );
    }

    #[test]
    fn diagnostic_error_can_be_emitted_as_device_event() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::Timeout {
            elapsed_ms: ms(1_500),
            timeout_ms: ms(1_000),
        });

        assert_eq!(
            DeviceEvent::DiagnosticError(error),
            DeviceEvent::DiagnosticError(crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed_ms: Some(ms(1_500)),
                timeout_ms: Some(ms(1_000)),
            })
        );
    }

    #[test]
    fn diagnostic_snapshot_preserves_counter_fields() {
        let diagnostics = crate::ParserDiagnostics {
            dropped_bytes: dropped_bytes(1),
            resyncs: diag_count(2),
            bad_checksums: diag_count(3),
            timeouts: diag_count(4),
            oversized_frames: diag_count(5),
            malformed_frames: diag_count(6),
            unmatched_replies: diag_count(7),
        };

        assert_eq!(
            crate::DiagnosticSnapshot::from_parser_diagnostics(diagnostics),
            crate::DiagnosticSnapshot {
                dropped_bytes: dropped_bytes(1),
                resyncs: diag_count(2),
                bad_checksums: diag_count(3),
                timeouts: diag_count(4),
                oversized_frames: diag_count(5),
                malformed_frames: diag_count(6),
                unmatched_replies: diag_count(7),
            }
        );
    }

    #[test]
    fn diagnostic_error_preserves_oversized_frame_details() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::OversizedFrame {
            claimed: frame_len(4_097),
            max: frame_len(4_096),
        });

        assert_eq!(
            error,
            crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::OversizedFrame,
                claimed_len: Some(frame_len(4_097)),
                max_len: Some(frame_len(4_096)),
                elapsed_ms: None,
                timeout_ms: None,
            }
        );
    }

    #[test]
    fn diagnostic_error_preserves_timeout_details() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::Timeout {
            elapsed_ms: ms(1_500),
            timeout_ms: ms(1_000),
        });

        assert_eq!(
            error,
            crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed_ms: Some(ms(1_500)),
                timeout_ms: Some(ms(1_000)),
            }
        );
    }

    #[test]
    fn diagnostic_snapshot_maps_from_device_event() {
        let diagnostics = crate::ParserDiagnostics {
            bad_checksums: diag_count(2),
            ..crate::ParserDiagnostics::default()
        };

        assert_eq!(
            crate::DiagnosticSnapshot::from_device_event(&DeviceEvent::Diagnostics(diagnostics)),
            Some(crate::DiagnosticSnapshot {
                bad_checksums: diag_count(2),
                ..crate::DiagnosticSnapshot::default()
            })
        );
        assert_eq!(
            crate::DiagnosticSnapshot::from_device_event(&DeviceEvent::LinkDown),
            None
        );
    }

    #[test]
    fn request_tracker_enforces_write_pacing() {
        let policy = crate::RequestPolicy {
            min_interval: Duration::from_milliseconds(100),
            ..crate::RequestPolicy::default()
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        assert_eq!(tracker.start(key, policy, ms(1_000)), Ok(()));
        assert_eq!(
            tracker.correlate_reply(key, &mut crate::ParserDiagnostics::default()),
            crate::CorrelationResult::Matched { key, attempts: 1 }
        );
        assert_eq!(
            tracker.start(key, policy, ms(1_050)),
            Err(crate::RequestStartError::Pacing {
                ready_at_ms: ms(1_100)
            })
        );
        assert_eq!(tracker.start(key, policy, ms(1_100)), Ok(()));
    }

    #[test]
    fn request_tracker_reports_retry_after_timeout() {
        let policy = crate::RequestPolicy {
            timeout: Duration::from_milliseconds(250),
            max_retries: 2,
            ..crate::RequestPolicy::default()
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        tracker.start(key, policy, ms(10)).unwrap();

        assert_eq!(tracker.on_tick(ms(259)), crate::RequestTick::Waiting);
        assert_eq!(
            tracker.on_tick(ms(260)),
            crate::RequestTick::Retry { key, attempt: 1 }
        );
        assert_eq!(tracker.retry_started(ms(260)), Ok(()));
        assert_eq!(
            tracker.on_tick(ms(510)),
            crate::RequestTick::Retry { key, attempt: 2 }
        );
        assert_eq!(tracker.retry_started(ms(510)), Ok(()));
        assert_eq!(
            tracker.on_tick(ms(760)),
            crate::RequestTick::TimedOut { key, attempts: 3 }
        );
    }

    #[test]
    fn request_tracker_retry_start_updates_same_key_pacing_watermark() {
        let policy = crate::RequestPolicy {
            min_interval: Duration::from_milliseconds(100),
            timeout: Duration::from_milliseconds(250),
            max_retries: 1,
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        assert_eq!(tracker.start(key, policy, ms(10)), Ok(()));
        assert_eq!(
            tracker.on_tick(ms(260)),
            crate::RequestTick::Retry { key, attempt: 1 }
        );
        assert_eq!(tracker.retry_started(ms(260)), Ok(()));
        assert_eq!(
            tracker.correlate_reply(key, &mut crate::ParserDiagnostics::default()),
            crate::CorrelationResult::Matched { key, attempts: 2 }
        );

        assert_eq!(
            tracker.start(key, policy, ms(300)),
            Err(crate::RequestStartError::Pacing {
                ready_at_ms: ms(360)
            })
        );
    }

    #[test]
    fn request_tracker_rejects_retry_without_active_request_or_retry_budget() {
        let policy = crate::RequestPolicy {
            timeout: Duration::from_milliseconds(250),
            max_retries: 0,
            ..crate::RequestPolicy::default()
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        assert_eq!(
            tracker.retry_started(ms(10)),
            Err(crate::RequestStartError::NoActiveRequest)
        );
        assert_eq!(tracker.start(key, policy, ms(10)), Ok(()));
        assert_eq!(
            tracker.retry_started(ms(260)),
            Err(crate::RequestStartError::Busy { key })
        );
    }

    #[test]
    fn request_tracker_correlates_reply_and_clears_slot() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        tracker.start(key, policy, ms(20)).unwrap();

        assert_eq!(
            tracker.correlate_reply(key, &mut crate::ParserDiagnostics::default()),
            crate::CorrelationResult::Matched { key, attempts: 1 }
        );
        assert_eq!(tracker.in_flight(), None);
        assert_eq!(tracker.start(key, policy, ms(21)), Ok(()));
    }

    #[test]
    fn request_tracker_counts_unmatched_replies() {
        let mut diagnostics = crate::ParserDiagnostics::default();
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        assert_eq!(
            tracker.correlate_reply(key, &mut diagnostics),
            crate::CorrelationResult::Unmatched { key }
        );
        assert_eq!(diagnostics.unmatched_replies, diag_count(1));
    }

    #[test]
    fn request_tracker_serializes_ambiguous_overlaps() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let telemetry = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let identity = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        tracker.start(telemetry, policy, ms(20)).unwrap();

        assert_eq!(
            tracker.start(identity, policy, ms(21)),
            Err(crate::RequestStartError::Busy { key: telemetry })
        );
    }

    #[test]
    fn request_tracker_correlates_can_target_separately_from_local_command() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let local = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let can = crate::RequestKey::for_target(
            crate::CommandKind::RequestTelemetry,
            crate::RequestTarget::VescCanController {
                controller_id: crate::VescControllerId::new(7),
            },
        );
        let mut diagnostics = crate::ParserDiagnostics::default();

        tracker.start(can, policy, ms(20)).unwrap();

        assert_eq!(
            tracker.correlate_reply(local, &mut diagnostics),
            crate::CorrelationResult::Unmatched { key: local }
        );
        assert_eq!(diagnostics.unmatched_replies, diag_count(1));
        assert_eq!(
            tracker.correlate_reply(can, &mut diagnostics),
            crate::CorrelationResult::Matched {
                key: can,
                attempts: 1
            }
        );
    }

    #[test]
    fn request_queue_pops_in_fifo_order() {
        let mut queue = crate::RequestQueue::<3>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(queue.enqueue(telemetry), Ok(()));
        assert_eq!(queue.enqueue(identity), Ok(()));

        assert_eq!(queue.pop_next(), Some(telemetry));
        assert_eq!(queue.pop_next(), Some(identity));
        assert_eq!(queue.pop_next(), None);
    }

    #[test]
    fn request_queue_rejects_overflow() {
        let mut queue = crate::RequestQueue::<1>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(queue.enqueue(telemetry), Ok(()));
        assert_eq!(
            queue.enqueue(identity),
            Err(crate::RequestQueueError::Full { capacity: 1 })
        );
    }

    #[test]
    fn request_queue_rejects_duplicate_keys() {
        let mut queue = crate::RequestQueue::<2>::new();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let request = crate::QueuedRequest::new(key, crate::RequestPolicy::default());

        assert_eq!(queue.enqueue(request), Ok(()));
        assert_eq!(
            queue.enqueue(request),
            Err(crate::RequestQueueError::DuplicateKey { key })
        );
    }

    #[test]
    fn request_queue_allows_reenqueue_after_dequeue() {
        let mut queue = crate::RequestQueue::<1>::new();
        let request = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );

        assert_eq!(queue.enqueue(request), Ok(()));
        assert_eq!(queue.pop_next(), Some(request));
        assert_eq!(queue.enqueue(request), Ok(()));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn request_queue_inserts_higher_urgency_before_routine_work() {
        let mut queue = crate::RequestQueue::<3>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(queue.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(queue.enqueue_by_urgency(identity), Ok(()));

        assert_eq!(queue.pop_next(), Some(identity));
        assert_eq!(queue.pop_next(), Some(telemetry));
    }

    #[test]
    fn request_queue_preserves_fifo_within_same_urgency() {
        let mut queue = crate::RequestQueue::<3>::new();
        let telemetry = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(queue.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(queue.enqueue_by_urgency(identity), Ok(()));

        assert_eq!(queue.pop_next(), Some(telemetry));
        assert_eq!(queue.pop_next(), Some(identity));
    }

    #[test]
    fn request_queue_refuses_duplicate_before_priority_insertion() {
        let mut queue = crate::RequestQueue::<2>::new();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let routine = crate::QueuedRequest::new(key, crate::RequestPolicy::default());
        let urgent = crate::QueuedRequest::with_urgency(
            key,
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(queue.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(
            queue.enqueue_by_urgency(urgent),
            Err(crate::RequestQueueError::DuplicateKey { key })
        );
        assert_eq!(queue.pop_next(), Some(routine));
    }

    #[test]
    fn request_queue_refuses_full_before_priority_insertion() {
        let mut queue = crate::RequestQueue::<1>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(queue.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(
            queue.enqueue_by_urgency(identity),
            Err(crate::RequestQueueError::Full { capacity: 1 })
        );
        assert_eq!(queue.pop_next(), Some(telemetry));
    }

    #[test]
    fn request_scheduler_counts_enqueue_and_dequeue_by_urgency() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(scheduler.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));

        assert_eq!(
            scheduler.diagnostics().enqueued,
            crate::RequestUrgencyCounters {
                routine: 1,
                high: 1,
                critical: 1,
            }
        );
        assert_eq!(scheduler.pop_next(), Some(diagnostics));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.pop_next(), Some(telemetry));
        assert_eq!(
            scheduler.diagnostics().dequeued,
            crate::RequestUrgencyCounters {
                routine: 1,
                high: 1,
                critical: 1,
            }
        );
    }

    #[test]
    fn request_scheduler_preserves_fifo_within_same_urgency() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let telemetry = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(scheduler.enqueue_by_urgency(telemetry), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(telemetry));
        assert_eq!(scheduler.pop_next(), Some(identity));
    }

    #[test]
    fn request_scheduler_inserts_between_higher_and_lower_urgency_work() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let critical = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let high = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(scheduler.enqueue_by_urgency(critical), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(high), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(critical));
        assert_eq!(scheduler.pop_next(), Some(high));
        assert_eq!(scheduler.pop_next(), Some(routine));
    }

    #[test]
    fn request_scheduler_counts_duplicate_and_overflow_refusals() {
        let mut scheduler = crate::RequestScheduler::<1>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(scheduler.enqueue(telemetry), Ok(()));
        assert_eq!(
            scheduler.enqueue(telemetry),
            Err(crate::RequestQueueError::DuplicateKey { key: telemetry.key })
        );
        assert_eq!(
            scheduler.enqueue(identity),
            Err(crate::RequestQueueError::Full { capacity: 1 })
        );

        assert_eq!(scheduler.diagnostics().duplicate_refusals, 1);
        assert_eq!(scheduler.diagnostics().overflow_refusals, 1);
        assert_eq!(
            scheduler.diagnostics().enqueued,
            crate::RequestUrgencyCounters {
                routine: 1,
                high: 0,
                critical: 0,
            }
        );
    }

    #[test]
    fn request_scheduler_exposes_queue_len_and_empty_state() {
        let mut scheduler = crate::RequestScheduler::<1>::new();
        let request = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );

        assert!(scheduler.is_empty());
        assert_eq!(scheduler.len(), 0);
        assert_eq!(scheduler.enqueue(request), Ok(()));
        assert!(!scheduler.is_empty());
        assert_eq!(scheduler.len(), 1);
        assert_eq!(scheduler.pop_next(), Some(request));
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.len(), 0);
    }

    #[test]
    fn request_scheduler_ages_skipped_routine_work_ahead_of_repeated_critical_work() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let firmware = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(firmware), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(firmware));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(routine));
        assert_eq!(scheduler.diagnostics().starvation_aging_events, 1);
        assert_eq!(scheduler.pop_next(), Some(diagnostics));
    }

    #[test]
    fn request_scheduler_continues_after_aged_promotion_without_stale_skip_counts() {
        let mut scheduler = crate::RequestScheduler::<3>::new();
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let firmware = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );

        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.enqueue_by_urgency(firmware), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(firmware));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(routine));

        assert_eq!(scheduler.pop_next(), Some(diagnostics));
        assert_eq!(scheduler.pop_next(), None);
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.diagnostics().starvation_aging_events, 1);
    }

    #[test]
    fn request_scheduler_does_not_age_new_middle_insert_after_promotion() {
        let mut scheduler = crate::RequestScheduler::<4>::new();
        let routine = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let firmware = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestFirmwareInfo),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let identity = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let diagnostics = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::RequestDiagnostics),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::Critical,
        );
        let set_lights = crate::QueuedRequest::with_urgency(
            crate::RequestKey::new(crate::CommandKind::SetLights),
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(scheduler.enqueue_by_urgency(routine), Ok(()));
        assert_eq!(scheduler.enqueue_by_urgency(firmware), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(firmware));
        assert_eq!(scheduler.enqueue_by_urgency(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(identity));
        assert_eq!(scheduler.enqueue_by_urgency(diagnostics), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(routine));
        assert_eq!(scheduler.enqueue_by_urgency(set_lights), Ok(()));

        assert_eq!(scheduler.pop_next(), Some(diagnostics));
        assert_eq!(scheduler.pop_next(), Some(set_lights));
        assert_eq!(scheduler.diagnostics().starvation_aging_events, 1);
    }

    #[test]
    fn request_scheduler_does_not_count_aging_when_fifo_front_is_selected() {
        let mut scheduler = crate::RequestScheduler::<2>::new();
        let telemetry = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );
        let identity = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestIdentity),
            crate::RequestPolicy::default(),
        );

        assert_eq!(scheduler.enqueue(telemetry), Ok(()));
        assert_eq!(scheduler.enqueue(identity), Ok(()));
        assert_eq!(scheduler.pop_next(), Some(telemetry));
        assert_eq!(scheduler.pop_next(), Some(identity));

        assert_eq!(scheduler.diagnostics().starvation_aging_events, 0);
    }

    #[test]
    fn poll_request_converts_read_only_command_to_queued_request() {
        let policy = crate::RequestPolicy {
            timeout: Duration::from_milliseconds(250),
            max_retries: 2,
            min_interval: Duration::from_milliseconds(50),
        };
        let request = crate::PollRequest::new(
            crate::CommandKind::RequestIdentity,
            policy,
            crate::RequestUrgency::High,
        );

        assert_eq!(
            request.to_queued_request(),
            Ok(crate::QueuedRequest::with_urgency(
                crate::RequestKey::new(crate::CommandKind::RequestIdentity),
                policy,
                crate::RequestUrgency::High
            ))
        );
    }

    #[test]
    fn poll_request_rejects_non_read_only_command() {
        let request = crate::PollRequest::new(
            crate::CommandKind::SetLights,
            crate::RequestPolicy::default(),
            crate::RequestUrgency::High,
        );

        assert_eq!(
            request.to_queued_request(),
            Err(crate::PollingPlanError::UnsupportedCommand {
                kind: crate::CommandKind::SetLights,
                safety_class: crate::SafetyClass::BenignControl,
            })
        );
    }

    #[test]
    fn polling_plan_enqueues_requests_by_urgency() {
        let plan = crate::PollingPlan::new([
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            ),
            crate::PollRequest::new(
                crate::CommandKind::RequestIdentity,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High,
            ),
        ]);
        let mut queue = crate::RequestQueue::<2>::new();

        assert_eq!(plan.enqueue_into(&mut queue), Ok(()));

        assert_eq!(
            queue.pop_next(),
            Some(crate::QueuedRequest::with_urgency(
                crate::RequestKey::new(crate::CommandKind::RequestIdentity),
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High
            ))
        );
        assert_eq!(
            queue.pop_next(),
            Some(crate::QueuedRequest::new(
                crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
                crate::RequestPolicy::default()
            ))
        );
    }

    #[test]
    fn polling_plan_propagates_duplicate_queue_errors() {
        let plan = crate::PollingPlan::new([
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            ),
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High,
            ),
        ]);
        let mut queue = crate::RequestQueue::<2>::new();

        assert_eq!(
            plan.enqueue_into(&mut queue),
            Err(crate::PollingPlanError::Queue(
                crate::RequestQueueError::DuplicateKey {
                    key: crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
                }
            ))
        );
    }

    #[test]
    fn polling_plan_propagates_capacity_errors() {
        let plan = crate::PollingPlan::new([
            crate::PollRequest::new(
                crate::CommandKind::RequestTelemetry,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            ),
            crate::PollRequest::new(
                crate::CommandKind::RequestIdentity,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::High,
            ),
        ]);
        let mut queue = crate::RequestQueue::<1>::new();

        assert_eq!(
            plan.enqueue_into(&mut queue),
            Err(crate::PollingPlanError::Queue(
                crate::RequestQueueError::Full { capacity: 1 }
            ))
        );
    }

    #[test]
    fn zero_capacity_request_queue_refuses_enqueue() {
        let mut queue = crate::RequestQueue::<0>::new();
        let request = crate::QueuedRequest::new(
            crate::RequestKey::new(crate::CommandKind::RequestTelemetry),
            crate::RequestPolicy::default(),
        );

        assert_eq!(
            queue.enqueue(request),
            Err(crate::RequestQueueError::Full { capacity: 0 })
        );
        assert!(queue.is_empty());
    }

    proptest! {
        #[test]
        fn request_queue_preserves_order_up_to_capacity(input in proptest::collection::vec(0u8..5, 0..8)) {
            let mut queue = crate::RequestQueue::<3>::new();
            let mut expected = Vec::new();

            for value in input {
                let command = if value % 2 == 0 {
                    crate::CommandKind::RequestTelemetry
                } else {
                    crate::CommandKind::RequestIdentity
                };
                let request = crate::QueuedRequest::new(
                    crate::RequestKey::new(command),
                    crate::RequestPolicy::default(),
                );
                if expected.iter().any(|queued: &crate::QueuedRequest| queued.key == request.key) {
                    prop_assert_eq!(
                        queue.enqueue(request),
                        Err(crate::RequestQueueError::DuplicateKey { key: request.key })
                    );
                } else if expected.len() == 3 {
                    prop_assert_eq!(
                        queue.enqueue(request),
                        Err(crate::RequestQueueError::Full { capacity: 3 })
                    );
                } else {
                    prop_assert_eq!(queue.enqueue(request), Ok(()));
                    expected.push(request);
                }
            }

            let mut observed = Vec::new();
            while let Some(request) = queue.pop_next() {
                observed.push(request);
            }
            prop_assert_eq!(observed, expected);
        }
    }

    proptest! {
        #[test]
        fn poll_request_accepts_read_only_commands(value in 0u8..7) {
            let kind = match value {
                0 => crate::CommandKind::RequestIdentity,
                1 => crate::CommandKind::RequestTelemetry,
                2 => crate::CommandKind::RequestFirmwareInfo,
                3 => crate::CommandKind::RequestBatteryInfo,
                4 => crate::CommandKind::RequestDiagnostics,
                5 => crate::CommandKind::RequestFaultHistory,
                _ => crate::CommandKind::RequestSettings,
            };
            let request = crate::PollRequest::new(
                kind,
                crate::RequestPolicy::default(),
                crate::RequestUrgency::Routine,
            );

            prop_assert_eq!(
                request.to_queued_request(),
                Ok(crate::QueuedRequest::new(
                    crate::RequestKey::new(kind),
                    crate::RequestPolicy::default()
                ))
            );
        }
    }

    proptest! {
        #[test]
        fn battery_response_keeps_unknown_distinct_from_zero(include_zero in any::<bool>()) {
            let level_reported = include_zero.then_some(Measured::reported(BatteryLevel::from_percent(0)));
            let response = crate::BatteryInfo {
                level_reported,
                ..crate::BatteryInfo::default()
            };

            if include_zero {
                prop_assert_eq!(
                    response.level_reported,
                    Some(Measured::reported(BatteryLevel::from_percent(0)))
                );
            } else {
                prop_assert_eq!(response.level_reported, None);
            }
        }
    }

    proptest! {
        #[test]
        fn request_queue_priority_order_is_monotonic(urgencies in proptest::collection::vec(0u8..3, 0..3)) {
            let commands = [
                crate::CommandKind::RequestTelemetry,
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::SetLights,
            ];
            let mut queue = crate::RequestQueue::<3>::new();

            for (index, urgency) in urgencies.into_iter().enumerate() {
                let request = crate::QueuedRequest::with_urgency(
                    crate::RequestKey::new(commands[index]),
                    crate::RequestPolicy::default(),
                    match urgency {
                        0 => crate::RequestUrgency::Routine,
                        1 => crate::RequestUrgency::High,
                        _ => crate::RequestUrgency::Critical,
                    },
                );
                prop_assert_eq!(queue.enqueue_by_urgency(request), Ok(()));
            }

            let mut last = crate::RequestUrgency::Critical;
            while let Some(request) = queue.pop_next() {
                prop_assert!(request.urgency <= last);
                last = request.urgency;
            }
        }
    }

    #[test]
    fn host_session_drives_link_events_and_drains_outputs() {
        let mut host = crate::HostSession::new(EchoSession::default());
        let link = LinkInfo {
            monotonic_ms: ms(10),
            max_write_len: Some(write_len(185)),
        };

        host.ingest_link_up(link);
        let drained = host.drain_outputs();

        assert_eq!(
            drained.as_slice(),
            &[SessionOutput::Event(DeviceEvent::LinkUp(link))]
        );
        assert!(host.drain_outputs().is_empty());
    }

    #[test]
    fn host_session_ingests_owned_notifications_without_retaining_bytes() {
        let mut host = crate::HostSession::new(EchoSession::default());
        let channel = GattChannel::from_bytes([0xfe; 16]);

        host.ingest_notification_owned(channel, vec![0xdc, 0x5a, 0x5c], ms(20));

        assert_eq!(
            host.drain_outputs().as_slice(),
            &[SessionOutput::NotificationIngest(
                crate::NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    crate::NotificationByteLen::from_bytes(3),
                    ms(20)
                )
            )]
        );
    }

    #[test]
    fn host_session_ingests_borrowed_session_input_for_ffi_wrappers() {
        let mut host = crate::HostSession::new(EchoSession::default());
        let channel = GattChannel::from_bytes([0xa1; 16]);
        let bytes = [0xde, 0xad, 0xbe, 0xef];

        host.ingest(SessionInput::Notification {
            channel,
            bytes: &bytes,
            monotonic_ms: ms(42),
        });

        assert_eq!(
            host.drain_outputs().as_slice(),
            &[SessionOutput::NotificationIngest(
                crate::NotificationIngestOutcome::ignored_wrong_channel(
                    channel,
                    crate::NotificationByteLen::from_bytes(4),
                    ms(42)
                )
            )]
        );
    }

    #[test]
    fn host_session_issues_commands_through_facade() {
        let mut host = crate::HostSession::new(EchoSession::default());

        host.issue_command(DeviceCommand::RequestTelemetry);

        assert_eq!(
            host.drain_outputs().as_slice(),
            &[SessionOutput::Transport(TransportAction::Write {
                channel: GattChannel::from_bytes([1; 16]),
                bytes: WritePayload::try_from_slice(b"telemetry").expect("test write payload fits"),
                mode: WriteMode::WithResponse,
            })]
        );
    }

    #[derive(Default)]
    struct StateSession {
        bms_step: u8,
        telemetry_step: u8,
    }

    impl ProtocolSession for StateSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::Command(DeviceCommand::RequestTelemetry) => {
                    let delta = match self.telemetry_step {
                        0 => TelemetryDelta {
                            at_ms: ms(40),
                            speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                                1_200,
                            ))),
                            ..TelemetryDelta::empty(ms(40))
                        },
                        _ => TelemetryDelta {
                            at_ms: ms(41),
                            voltage: Some(Measured::reported(Voltage::from_millivolts(80_400))),
                            ..TelemetryDelta::empty(ms(41))
                        },
                    };
                    self.telemetry_step = self.telemetry_step.saturating_add(1);
                    output.push(SessionOutput::Event(DeviceEvent::Telemetry(delta)));
                }
                SessionInput::Command(DeviceCommand::RequestBatteryInfo) => {
                    let selector = match self.bms_step {
                        0 => 8,
                        _ => 9,
                    };
                    self.bms_step = self.bms_step.saturating_add(1);
                    output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                        TelemetryDelta::empty(ms(42)),
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                        crate::ReadOnlyResponse::Battery(crate::BatteryReadback::available(
                            crate::BatteryPagePayload::Raw(crate::BatteryRawPage::new(
                                crate::BatteryPageMetadata::raw(
                                    crate::ProtocolSelector::new(selector),
                                    VerificationStatus::SourceVerified,
                                ),
                                crate::BatteryInfo {
                                    voltage: Some(Measured::reported(Voltage::from_millivolts(
                                        80_400,
                                    ))),
                                    ..crate::BatteryInfo::default()
                                },
                            )),
                        )),
                    )));
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        crate::ParserDiagnostics {
                            timeouts: diag_count(monotonic_ms.get()),
                            ..crate::ParserDiagnostics::default()
                        },
                    )));
                }
                SessionInput::LinkUp(_)
                | SessionInput::LinkDown
                | SessionInput::Notification { .. }
                | SessionInput::Command(_) => {}
            }
        }
    }

    #[test]
    fn host_session_aggregates_current_snapshot_from_multiple_events() {
        let mut host = crate::HostSession::new(StateSession::default());

        host.issue_command(DeviceCommand::RequestTelemetry);
        host.issue_command(DeviceCommand::RequestTelemetry);

        assert_eq!(host.current_snapshot().at_ms, Some(ms(41)));
        assert_eq!(
            host.current_snapshot().speed,
            Some(Measured::reported(Speed::from_millimetres_per_second(
                1_200
            )))
        );
        assert_eq!(
            host.current_snapshot().voltage,
            Some(Measured::reported(Voltage::from_millivolts(80_400)))
        );
    }

    #[test]
    fn host_session_aggregates_bms_pages_under_telemetry_state() {
        let mut host = crate::HostSession::new(StateSession::default());

        host.issue_command(DeviceCommand::RequestBatteryInfo);
        host.issue_command(DeviceCommand::RequestBatteryInfo);

        assert_eq!(
            host.session_state().telemetry().bms.latest.availability(),
            crate::BatteryReadbackAvailability::Available
        );
        assert_eq!(
            host.session_state()
                .telemetry()
                .bms
                .latest
                .page()
                .map(crate::BatteryPagePayload::page)
                .map(|page| page.selector.get()),
            Some(9)
        );
        assert_eq!(
            host.session_state()
                .telemetry()
                .bms
                .pages
                .iter()
                .map(crate::BatteryPagePayload::page)
                .map(|page| page.selector.get())
                .collect::<Vec<_>>(),
            vec![8, 9]
        );
    }

    #[test]
    fn host_session_merges_diagnostics_from_events() {
        let mut host = crate::HostSession::new(StateSession::default());

        host.tick(ms(2));
        host.tick(ms(3));

        assert_eq!(
            host.session_state().diagnostics.parser.timeouts,
            diag_count(5)
        );
    }

    #[test]
    fn diagnostic_snapshot_maps_from_host_session_diagnostics() {
        let mut host = crate::HostSession::new(StateSession::default());

        host.tick(ms(2));

        assert_eq!(
            crate::DiagnosticSnapshot::from_parser_diagnostics(host.diagnostics()),
            crate::DiagnosticSnapshot {
                timeouts: diag_count(2),
                ..crate::DiagnosticSnapshot::default()
            }
        );
    }

    #[derive(Default)]
    struct FramedCaptureSession {
        sum: i32,
    }

    impl ProtocolSession for FramedCaptureSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(info) => {
                    output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
                }
                SessionInput::LinkDown => {
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::Notification { bytes, .. } => {
                    for byte in bytes {
                        if *byte == 0xff {
                            output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                                TelemetryDelta {
                                    at_ms: ms(90),
                                    speed: Some(Measured::reported(
                                        Speed::from_millimetres_per_second(self.sum),
                                    )),
                                    ..TelemetryDelta::empty(ms(90))
                                },
                            )));
                            self.sum = 0;
                        } else {
                            self.sum += i32::from(*byte);
                        }
                    }
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                }
                SessionInput::Command(command) => {
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        crate::ParserDiagnostics {
                            unmatched_replies: diag_count(command.kind() as u64),
                            ..crate::ParserDiagnostics::default()
                        },
                    )));
                }
            }
        }
    }

    fn replay_events(records: &[crate::CaptureRecord]) -> Vec<DeviceEvent> {
        let mut host = crate::HostSession::new(FramedCaptureSession::default());
        crate::replay_capture(&mut host, records)
            .into_iter()
            .filter_map(|output| match output {
                SessionOutput::Event(event) => Some(event),
                SessionOutput::Transport(_) | SessionOutput::NotificationIngest(_) => None,
            })
            .collect()
    }

    #[test]
    fn capture_record_owns_notification_payloads() {
        let channel = GattChannel::from_bytes([0x11; 16]);
        let source = vec![1, 2, 0xff];
        let record = crate::CaptureRecord::notification(channel, source.clone(), ms(10));

        assert_eq!(
            record,
            crate::CaptureRecord::Notification {
                channel,
                bytes: source,
                monotonic_ms: ms(10),
            }
        );
    }

    #[test]
    fn capture_record_preserves_targeted_command_metadata() {
        let target = crate::RequestTarget::VescCanController {
            controller_id: crate::VescControllerId::new(7),
        };
        let record =
            crate::CaptureRecord::targeted_command(DeviceCommand::RequestTelemetry, target);

        assert_eq!(
            record,
            crate::CaptureRecord::TargetedCommand {
                command: DeviceCommand::RequestTelemetry,
                target,
            }
        );
    }

    #[test]
    fn replay_capture_drives_link_tick_command_and_notification_records() {
        let channel = GattChannel::from_bytes([0x22; 16]);
        let link = LinkInfo {
            monotonic_ms: ms(1),
            max_write_len: Some(write_len(185)),
        };
        let records = [
            crate::CaptureRecord::LinkUp(link),
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(2),
            },
            crate::CaptureRecord::Command(DeviceCommand::RequestIdentity),
            crate::CaptureRecord::notification(channel, vec![4, 5, 0xff], ms(3)),
            crate::CaptureRecord::LinkDown,
        ];

        assert_eq!(
            replay_events(&records).as_slice(),
            &[
                DeviceEvent::LinkUp(link),
                DeviceEvent::Tick {
                    monotonic_ms: ms(2)
                },
                DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                    unmatched_replies: diag_count(crate::CommandKind::RequestIdentity as u64),
                    ..crate::ParserDiagnostics::default()
                }),
                DeviceEvent::Telemetry(TelemetryDelta {
                    at_ms: ms(90),
                    speed: Some(Measured::reported(Speed::from_millimetres_per_second(9),)),
                    ..TelemetryDelta::empty(ms(90))
                }),
                DeviceEvent::LinkDown,
            ]
        );
    }

    #[test]
    fn replay_capture_drives_targeted_command_as_underlying_command() {
        let target = crate::RequestTarget::VescCanController {
            controller_id: crate::VescControllerId::new(7),
        };
        let records = [crate::CaptureRecord::targeted_command(
            DeviceCommand::RequestTelemetry,
            target,
        )];

        assert_eq!(
            replay_events(&records).as_slice(),
            &[DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                unmatched_replies: diag_count(crate::CommandKind::RequestTelemetry as u64),
                ..crate::ParserDiagnostics::default()
            })]
        );
    }

    #[test]
    fn targeted_command_survives_notification_chunking_helpers() {
        let target = crate::RequestTarget::VescCanController {
            controller_id: crate::VescControllerId::new(7),
        };
        let record =
            crate::CaptureRecord::targeted_command(DeviceCommand::RequestTelemetry, target);

        assert_eq!(
            record
                .clone()
                .split_notification_bytes(crate::NotificationChunkLen::from_bytes(1)),
            vec![record.clone()]
        );
        assert_eq!(
            record.clone().split_notification_by_lengths(&[
                crate::NotificationChunkLen::from_bytes(1),
                crate::NotificationChunkLen::from_bytes(2),
            ]),
            vec![record]
        );
    }

    #[test]
    fn one_byte_notification_replay_matches_whole_notification_replay() {
        let channel = GattChannel::from_bytes([0x33; 16]);
        let whole = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 0xff],
            ms(10),
        )];
        let one_byte = crate::CaptureRecord::notification(channel, vec![1, 2, 3, 0xff], ms(10))
            .split_notification_bytes(crate::NotificationChunkLen::from_bytes(1));

        assert_eq!(replay_events(&one_byte), replay_events(&whole));
    }

    #[test]
    fn replay_chunk_comparison_ignores_notification_boundaries() {
        let channel = GattChannel::from_bytes([0x66; 16]);
        let records = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 0xff],
            ms(10),
        )];

        let comparison = crate::compare_replay_capture_chunks(
            FramedCaptureSession::default,
            &records,
            &[
                crate::NotificationChunkLen::from_bytes(2),
                crate::NotificationChunkLen::from_bytes(1),
            ],
        )
        .expect("bounded replay comparison should fit");

        assert_eq!(
            comparison,
            crate::ReplayChunkComparison {
                whole_semantic_events: crate::SemanticEventCount::from_events(1),
                one_byte_semantic_events: crate::SemanticEventCount::from_events(1),
                arbitrary_semantic_events: crate::SemanticEventCount::from_events(1),
                one_byte_matches: true,
                arbitrary_matches: true,
            }
        );
    }

    #[test]
    fn replay_chunk_comparison_reports_semantic_mismatch() {
        #[derive(Default)]
        struct NotificationLengthSession;

        impl ProtocolSession for NotificationLengthSession {
            fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
                let SessionInput::Notification {
                    bytes,
                    monotonic_ms,
                    ..
                } = input
                else {
                    return;
                };
                output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                    TelemetryDelta {
                        at_ms: monotonic_ms,
                        speed: Some(Measured::reported(Speed::from_millimetres_per_second(
                            i32::try_from(bytes.len()).unwrap_or(0),
                        ))),
                        ..TelemetryDelta::empty(monotonic_ms)
                    },
                )));
            }
        }

        let channel = GattChannel::from_bytes([0x77; 16]);
        let records = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 4],
            ms(10),
        )];

        let comparison = crate::compare_replay_capture_chunks(
            || NotificationLengthSession,
            &records,
            &[
                crate::NotificationChunkLen::from_bytes(2),
                crate::NotificationChunkLen::from_bytes(2),
            ],
        )
        .expect("bounded replay comparison should fit");

        assert!(!comparison.one_byte_matches);
        assert!(!comparison.arbitrary_matches);
    }

    #[test]
    fn replay_chunk_comparison_reports_output_overflow() {
        #[derive(Default)]
        struct NoisySession;

        impl ProtocolSession for NoisySession {
            fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
                let SessionInput::Notification { monotonic_ms, .. } = input else {
                    return;
                };

                output.extend((0..=128).map(|offset| {
                    SessionOutput::Event(DeviceEvent::Tick {
                        monotonic_ms: ms(monotonic_ms.get().saturating_add(offset)),
                    })
                }));
            }
        }

        let channel = GattChannel::from_bytes([0x79; 16]);
        let records = [crate::CaptureRecord::notification(channel, vec![1], ms(10))];

        let error = crate::replay_capture_checked(
            &mut crate::HostSession::new(NoisySession),
            &records,
            crate::ParserLimits::default().max_queued_outputs,
        )
        .expect_err("overflow must not be collapsed into an empty replay");

        assert_eq!(
            error,
            crate::SessionOutputError::OutputOverflow {
                limit: crate::ParserLimits::default().max_queued_outputs,
                actual: crate::ParserQueuedOutputCount::from_outputs(129),
            }
        );
    }

    #[test]
    fn replay_arbitrary_chunk_lengths_are_derived_from_capture_notifications() {
        let channel = GattChannel::from_bytes([0x78; 16]);
        let records = [
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(1),
            },
            crate::CaptureRecord::notification(channel, vec![0; 4], ms(2)),
            crate::CaptureRecord::notification(channel, vec![0; 10], ms(3)),
            crate::CaptureRecord::LinkDown,
        ];

        assert_eq!(
            crate::replay_arbitrary_chunk_lengths(&records),
            vec![
                crate::NotificationChunkLen::from_bytes(2),
                crate::NotificationChunkLen::from_bytes(3),
                crate::NotificationChunkLen::from_bytes(5),
            ]
        );
    }

    #[test]
    fn replay_arbitrary_chunk_lengths_are_empty_without_notifications() {
        assert_eq!(
            crate::replay_arbitrary_chunk_lengths(&[crate::CaptureRecord::Tick {
                monotonic_ms: ms(1)
            }]),
            Vec::<crate::NotificationChunkLen>::new()
        );
    }

    #[test]
    fn notification_boundary_cases_cover_whole_bytewise_arbitrary_and_coalesced_replay() {
        let channel = GattChannel::from_bytes([0x79; 16]);
        let frame_a = [0xaa, 0xbb, 0xcc];
        let frame_b = [0xdd, 0xee];

        let cases = crate::notification_boundary_replay_cases(
            channel,
            &[frame_a.as_slice(), frame_b.as_slice()],
            ms(10),
            &[crate::NotificationChunkLen::from_bytes(2)],
        );

        assert_eq!(
            cases.iter().map(|case| case.name).collect::<Vec<_>>(),
            vec!["whole", "one-byte", "arbitrary", "coalesced"]
        );
        assert_eq!(
            cases
                .iter()
                .map(|case| case.records.len())
                .collect::<Vec<_>>(),
            vec![2, 5, 3, 1]
        );
    }

    #[test]
    fn notification_impairment_cases_cover_noisy_duplicate_missing_and_timeout_replay() {
        let channel = GattChannel::from_bytes([0x7a; 16]);
        let frame = [0xaa, 0xbb, 0xcc, 0xdd, 0xee];

        let cases = crate::notification_impairment_replay_cases(
            channel,
            frame.as_slice(),
            ms(10),
            &[0x00, 0x01],
            ms(99),
        );

        assert_eq!(
            cases.iter().map(|case| case.name).collect::<Vec<_>>(),
            vec![
                "noise-prefix",
                "duplicate-first-chunk",
                "missing-final-byte",
                "timeout-after-partial",
            ]
        );
        assert_eq!(
            cases
                .iter()
                .map(|case| case.records.len())
                .collect::<Vec<_>>(),
            vec![1, 3, 1, 2]
        );
    }

    proptest! {
        #[test]
        fn arbitrary_chunk_notification_replay_matches_whole_notification_replay(
            payload_prefix in proptest::collection::vec(0u8..0xff, 0..16),
            lengths in proptest::collection::vec(0usize..6, 0..8),
        ) {
            let channel = GattChannel::from_bytes([0x44; 16]);
            let mut payload = payload_prefix;
            payload.push(0xff);
            let whole = [crate::CaptureRecord::notification(channel, payload.clone(), ms(20))];
            let chunk_lengths = lengths
                .into_iter()
                .map(crate::NotificationChunkLen::from_bytes)
                .collect::<Vec<_>>();
            let chunks = crate::CaptureRecord::notification(channel, payload, ms(20))
                .split_notification_by_lengths(&chunk_lengths);

            prop_assert_eq!(replay_events(&chunks), replay_events(&whole));
        }
    }

    #[test]
    fn replay_summary_preserves_output_order() {
        let channel = GattChannel::from_bytes([0x55; 16]);
        let records = [
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(1),
            },
            crate::CaptureRecord::notification(channel, vec![9, 0xff], ms(2)),
            crate::CaptureRecord::Tick {
                monotonic_ms: ms(3),
            },
        ];
        let mut host = crate::HostSession::new(FramedCaptureSession::default());

        assert_eq!(
            crate::replay_capture(&mut host, &records).as_slice(),
            &[
                SessionOutput::Event(DeviceEvent::Tick {
                    monotonic_ms: ms(1)
                }),
                SessionOutput::Event(DeviceEvent::Telemetry(TelemetryDelta {
                    at_ms: ms(90),
                    speed: Some(Measured::reported(Speed::from_millimetres_per_second(9),)),
                    ..TelemetryDelta::empty(ms(90))
                })),
                SessionOutput::Event(DeviceEvent::Tick {
                    monotonic_ms: ms(3)
                }),
            ]
        );
    }
}
