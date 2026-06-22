#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Core types and setup scaffolding for Cutout.

use std::ops::RangeInclusive;

use arrayvec::ArrayVec;
use thiserror::Error;

mod pevcap;
pub use pevcap::*;
mod battery_page;
pub use battery_page::*;
mod ffi;
pub use ffi::*;

/// Monotonic timestamp in milliseconds, supplied by the host.
pub type MonotonicMillis = u64;

/// Maximum payload bytes accepted for a single GATT write value.
pub const MAX_TRANSPORT_WRITE_LEN: usize = 512;

/// Payload bytes stored inline before falling back to an explicit large write.
pub const MAX_INLINE_TRANSPORT_WRITE_LEN: usize = 32;

/// Transport-independent identifier for a GATT characteristic or endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GattChannel([u8; 16]);

impl GattChannel {
    /// Creates a channel identifier from its 16-byte representation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the channel identifier as raw bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Host-observed link details supplied when a transport connects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkInfo {
    /// Host monotonic connection timestamp.
    pub monotonic_ms: MonotonicMillis,

    /// Maximum write payload length reported by the host, when known.
    pub max_write_len: Option<u16>,
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

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights(LightState),

    /// Sound a device horn or alert.
    SoundHorn,

    /// Set raw motor current in milliamps.
    SetRawMotorCurrent {
        /// Target motor/phase current in milliamps.
        current_ma: i32,
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
            Self::RequestSettings => CommandKind::RequestSettings,
            Self::SetLights(_) => CommandKind::SetLights,
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

    /// Request current settings without changing device state.
    RequestSettings,

    /// Set the device lights.
    SetLights,

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
            | Self::RequestSettings => SafetyClass::ReadOnly,
            Self::SetLights | Self::SoundHorn => SafetyClass::BenignControl,
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

/// Protocol family identifier used by registry data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    pub series_cells: u8,

    /// Nominal pack capacity in milliamp-hours, when known.
    pub nominal_capacity_mah: Option<u32>,

    /// Expected pack voltage range in millivolts.
    pub voltage_range_mv: RangeInclusive<u32>,

    /// Verification status for the battery metadata.
    pub verification: VerificationStatus,
}

/// Static BMS selector interpretation for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsPageSelectorSpec {
    /// BMS page selector value.
    pub selector: u8,

    /// Current interpretation of the selector.
    pub kind: BatteryPageKind,

    /// Verification status for this selector interpretation.
    pub verification: VerificationStatus,
}

/// Static BMS layout metadata for a registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BmsLayoutSpec {
    /// Series-connected cell count covered by this BMS layout.
    pub series_cells: u8,

    /// Parallel pack count for this model.
    pub parallel_packs: u8,

    /// Cell-voltage values decoded from a full cell-voltage page.
    pub cell_values_per_page: u8,

    /// Temperature values decoded from a full temperature page.
    pub temperature_values_per_page: u8,

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

/// Data-only model registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRegistryEntry {
    /// Manufacturer or brand.
    pub manufacturer: &'static str,

    /// Model name.
    pub model: &'static str,

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
        self.write_str(entry.manufacturer);
        self.write_str(entry.model);
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
                self.write_u8(battery.series_cells);
                self.write_optional_u32(battery.nominal_capacity_mah);
                self.write_u32(*battery.voltage_range_mv.start());
                self.write_u32(*battery.voltage_range_mv.end());
                self.write_u8(verification_code(battery.verification));
            }
            None => self.write_u8(0),
        }
    }

    fn write_bms(&mut self, bms: Option<&BmsLayoutSpec>) {
        match bms {
            Some(bms) => {
                self.write_u8(1);
                self.write_u8(bms.series_cells);
                self.write_u8(bms.parallel_packs);
                self.write_u8(bms.cell_values_per_page);
                self.write_u8(bms.temperature_values_per_page);
                self.write_usize(bms.selectors.len());
                for selector in bms.selectors {
                    self.write_u8(selector.selector);
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

    fn write_u16(&mut self, value: u16) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_u8(&mut self, value: u8) {
        self.write_bytes(&[value]);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            for (lane_index, lane) in self.lanes.iter_mut().enumerate() {
                let lane_index_u64 = u64::try_from(lane_index).unwrap_or_default();
                let lane_index_u32 = u32::try_from(lane_index).unwrap_or_default();
                *lane ^= u64::from(*byte).wrapping_add(lane_index_u64 << 8);
                *lane = lane.wrapping_mul(0x0000_0100_0000_01b3 + lane_index_u64);
                *lane ^= lane.rotate_left(17 + lane_index_u32);
            }
        }
    }
}

const ALL_COMMAND_KINDS: [CommandKind; 9] = [
    CommandKind::RequestIdentity,
    CommandKind::RequestTelemetry,
    CommandKind::RequestFirmwareInfo,
    CommandKind::RequestBatteryInfo,
    CommandKind::RequestDiagnostics,
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
    pub max_frame_len: usize,

    /// Maximum buffered input length in bytes before a parser should shed data.
    pub max_buffered_len: usize,

    /// Maximum queued outputs a parser should retain before yielding to host code.
    pub max_queued_outputs: usize,

    /// Parser timeout threshold in host monotonic milliseconds.
    pub timeout_ms: MonotonicMillis,
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self {
            max_frame_len: 4_096,
            max_buffered_len: 8_192,
            max_queued_outputs: 128,
            timeout_ms: 1_000,
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
    pub const fn validate_frame_len(self, claimed: usize) -> Result<(), ParserError> {
        if claimed <= self.max_frame_len {
            Ok(())
        } else {
            Err(ParserError::OversizedFrame {
                claimed,
                max: self.max_frame_len,
            })
        }
    }
}

/// Parser failure reason that can be counted without tying core to a protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParserError {
    /// A frame claimed or accumulated more bytes than allowed.
    OversizedFrame {
        /// Claimed or observed frame length.
        claimed: usize,

        /// Configured maximum accepted frame length.
        max: usize,
    },

    /// A frame checksum did not match its payload.
    BadChecksum,

    /// Input bytes could not form a valid frame.
    MalformedFrame,

    /// A parser deadline elapsed before the expected data arrived.
    Timeout {
        /// Elapsed monotonic milliseconds.
        elapsed_ms: MonotonicMillis,

        /// Timeout threshold in monotonic milliseconds.
        timeout_ms: MonotonicMillis,
    },

    /// A reply could not be matched to an in-flight request.
    UnmatchedReply,
}

/// Saturating parser diagnostics counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParserDiagnostics {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: u64,

    /// Parser resynchronization attempts.
    pub resyncs: u64,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: u64,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: u64,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: u64,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: u64,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: u64,
}

impl ParserDiagnostics {
    /// Adds dropped bytes using saturating arithmetic.
    pub const fn add_dropped_bytes(&mut self, count: u64) {
        self.dropped_bytes = self.dropped_bytes.saturating_add(count);
    }

    /// Records one parser resynchronization attempt.
    pub const fn record_resync(&mut self) {
        saturating_increment(&mut self.resyncs);
    }

    /// Records one parser error in the corresponding diagnostics counter.
    pub const fn record_error(&mut self, error: ParserError) {
        match error {
            ParserError::OversizedFrame { .. } => {
                saturating_increment(&mut self.oversized_frames);
            }
            ParserError::BadChecksum => {
                saturating_increment(&mut self.bad_checksums);
            }
            ParserError::MalformedFrame => {
                saturating_increment(&mut self.malformed_frames);
            }
            ParserError::Timeout { .. } => {
                saturating_increment(&mut self.timeouts);
            }
            ParserError::UnmatchedReply => {
                saturating_increment(&mut self.unmatched_replies);
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

const fn saturating_increment(counter: &mut u64) {
    *counter = counter.saturating_add(1);
}

/// Stable host-facing diagnostic counter snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticSnapshot {
    /// Bytes dropped while recovering from malformed or excessive input.
    pub dropped_bytes: u64,

    /// Parser resynchronization attempts.
    pub resyncs: u64,

    /// Frames rejected because their checksum did not match.
    pub bad_checksums: u64,

    /// Parser deadlines that elapsed before expected data arrived.
    pub timeouts: u64,

    /// Frames rejected because they exceeded parser limits.
    pub oversized_frames: u64,

    /// Frames rejected because their structure was invalid.
    pub malformed_frames: u64,

    /// Replies that could not be matched to an in-flight request.
    pub unmatched_replies: u64,
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
    pub const fn from_device_event(event: DeviceEvent) -> Option<Self> {
        match event {
            DeviceEvent::Diagnostics(diagnostics) => {
                Some(Self::from_parser_diagnostics(diagnostics))
            }
            DeviceEvent::LinkUp(_)
            | DeviceEvent::LinkDown
            | DeviceEvent::NotificationReceived { .. }
            | DeviceEvent::Tick { .. }
            | DeviceEvent::Telemetry(_)
            | DeviceEvent::ReadOnlyResponse(_)
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
    pub claimed_len: Option<usize>,

    /// Configured maximum frame length for oversized-frame errors.
    pub max_len: Option<usize>,

    /// Elapsed monotonic milliseconds for timeout errors.
    pub elapsed_ms: Option<MonotonicMillis>,

    /// Timeout threshold in monotonic milliseconds for timeout errors.
    pub timeout_ms: Option<MonotonicMillis>,
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

/// Transport-independent key used to correlate a scheduled request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestKey {
    /// Command kind represented by this request.
    pub command: CommandKind,
}

impl RequestKey {
    /// Creates a request key from a command kind.
    #[must_use]
    pub const fn new(command: CommandKind) -> Self {
        Self { command }
    }
}

/// Retry, timeout, and pacing policy for one scheduled request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestPolicy {
    /// Deadline for one attempt in monotonic milliseconds.
    pub timeout_ms: MonotonicMillis,

    /// Maximum retries after the first attempt.
    pub max_retries: u8,

    /// Minimum interval between starts for the same key.
    pub min_interval_ms: MonotonicMillis,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 1_000,
            max_retries: 0,
            min_interval_ms: 0,
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
    pub started_at_ms: MonotonicMillis,

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
        ready_at_ms: MonotonicMillis,
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
    last_started: Option<(RequestKey, MonotonicMillis)>,
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
        now_ms: MonotonicMillis,
    ) -> Result<(), RequestStartError> {
        if let Some(active) = self.in_flight {
            return Err(RequestStartError::Busy { key: active.key });
        }

        if let Some((last_key, started_at_ms)) = self.last_started {
            let ready_at_ms = started_at_ms.saturating_add(policy.min_interval_ms);
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
    pub const fn on_tick(self, now_ms: MonotonicMillis) -> RequestTick {
        let Some(active) = self.in_flight else {
            return RequestTick::Idle;
        };
        let deadline_ms = active
            .started_at_ms
            .saturating_add(active.policy.timeout_ms);
        if now_ms < deadline_ms {
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
        now_ms: MonotonicMillis,
    ) -> Result<(), RequestStartError> {
        let Some(mut active) = self.in_flight else {
            return Err(RequestStartError::NoActiveRequest);
        };
        if active.retries >= active.policy.max_retries {
            return Err(RequestStartError::Busy { key: active.key });
        }
        active.retries = active.retries.saturating_add(1);
        active.started_at_ms = now_ms;
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

/// Generic battery or BMS information.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatteryInfo {
    /// Pack or input voltage in millivolts.
    pub voltage_mv: Option<Measured<i32>>,

    /// Pack or battery current in milliamps.
    pub current_ma: Option<Measured<i32>>,

    /// Battery percentage reported by the device.
    pub percent_reported: Option<Measured<u8>>,

    /// Battery percentage estimated by Cutout.
    pub percent_estimated: Option<Measured<u8>>,

    /// Battery or BMS temperature in millicelsius.
    pub temperature_mc: Option<Measured<i32>>,

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

/// Bounded diagnostic readback response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticReadback {
    /// Diagnostic detail slots.
    pub details: [Option<DiagnosticDetail>; 4],
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
    pub entries: [Option<SettingsEntry>; 4],
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
            Self::Settings(_) => CommandKind::RequestSettings,
        }
    }
}

/// Partial telemetry update from a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryDelta {
    /// Host monotonic timestamp for this update.
    pub at_ms: MonotonicMillis,

    /// Reported or calculated speed in millimeters per second.
    pub speed_mm_s: Option<Measured<i32>>,

    /// Reported or measured input voltage in millivolts.
    pub voltage_mv: Option<Measured<i32>>,

    /// Battery/input current in milliamps.
    pub battery_current_ma: Option<Measured<i32>>,

    /// Motor/phase current in milliamps.
    pub motor_current_ma: Option<Measured<i32>>,

    /// Electrical power in milliwatts.
    pub power_mw: Option<Measured<i64>>,

    /// Controller temperature in millicelsius.
    pub controller_temperature_mc: Option<Measured<i32>>,

    /// Motor temperature in millicelsius.
    pub motor_temperature_mc: Option<Measured<i32>>,

    /// Battery temperature in millicelsius.
    pub battery_temperature_mc: Option<Measured<i32>>,

    /// PWM duty in permille.
    pub pwm_permille: Option<Measured<i16>>,

    /// Total or trip distance in millimeters.
    pub distance_mm: Option<Measured<u64>>,

    /// Pitch in millidegrees.
    pub pitch_mdeg: Option<Measured<i32>>,

    /// Roll in millidegrees.
    pub roll_mdeg: Option<Measured<i32>>,

    /// Battery percentage reported by the device.
    pub battery_percent_reported: Option<Measured<u8>>,

    /// Battery percentage estimated by Cutout.
    pub battery_percent_estimated: Option<Measured<u8>>,
}

impl TelemetryDelta {
    /// Creates an empty telemetry delta at a timestamp.
    #[must_use]
    pub const fn empty(at_ms: MonotonicMillis) -> Self {
        Self {
            at_ms,
            speed_mm_s: None,
            voltage_mv: None,
            battery_current_ma: None,
            motor_current_ma: None,
            power_mw: None,
            controller_temperature_mc: None,
            motor_temperature_mc: None,
            battery_temperature_mc: None,
            pwm_permille: None,
            distance_mm: None,
            pitch_mdeg: None,
            roll_mdeg: None,
            battery_percent_reported: None,
            battery_percent_estimated: None,
        }
    }
}

/// Aggregated latest-known telemetry snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TelemetrySnapshot {
    /// Timestamp of the latest applied delta.
    pub at_ms: Option<MonotonicMillis>,

    /// Latest known speed in millimeters per second.
    pub speed_mm_s: Option<Measured<i32>>,

    /// Latest known input voltage in millivolts.
    pub voltage_mv: Option<Measured<i32>>,

    /// Latest known battery/input current in milliamps.
    pub battery_current_ma: Option<Measured<i32>>,

    /// Latest known motor/phase current in milliamps.
    pub motor_current_ma: Option<Measured<i32>>,

    /// Latest known electrical power in milliwatts.
    pub power_mw: Option<Measured<i64>>,

    /// Latest known controller temperature in millicelsius.
    pub controller_temperature_mc: Option<Measured<i32>>,

    /// Latest known motor temperature in millicelsius.
    pub motor_temperature_mc: Option<Measured<i32>>,

    /// Latest known battery temperature in millicelsius.
    pub battery_temperature_mc: Option<Measured<i32>>,

    /// Latest known PWM duty in permille.
    pub pwm_permille: Option<Measured<i16>>,

    /// Latest known total or trip distance in millimeters.
    pub distance_mm: Option<Measured<u64>>,

    /// Latest known pitch in millidegrees.
    pub pitch_mdeg: Option<Measured<i32>>,

    /// Latest known roll in millidegrees.
    pub roll_mdeg: Option<Measured<i32>>,

    /// Latest known battery percentage reported by the device.
    pub battery_percent_reported: Option<Measured<u8>>,

    /// Latest known battery percentage estimated by Cutout.
    pub battery_percent_estimated: Option<Measured<u8>>,
}

impl TelemetrySnapshot {
    /// Applies a partial telemetry update, preserving fields absent from it.
    pub const fn apply_delta(&mut self, delta: TelemetryDelta) {
        self.at_ms = Some(delta.at_ms);

        if delta.speed_mm_s.is_some() {
            self.speed_mm_s = delta.speed_mm_s;
        }
        if delta.voltage_mv.is_some() {
            self.voltage_mv = delta.voltage_mv;
        }
        if delta.battery_current_ma.is_some() {
            self.battery_current_ma = delta.battery_current_ma;
        }
        if delta.motor_current_ma.is_some() {
            self.motor_current_ma = delta.motor_current_ma;
        }
        if delta.power_mw.is_some() {
            self.power_mw = delta.power_mw;
        }
        if delta.controller_temperature_mc.is_some() {
            self.controller_temperature_mc = delta.controller_temperature_mc;
        }
        if delta.motor_temperature_mc.is_some() {
            self.motor_temperature_mc = delta.motor_temperature_mc;
        }
        if delta.battery_temperature_mc.is_some() {
            self.battery_temperature_mc = delta.battery_temperature_mc;
        }
        if delta.pwm_permille.is_some() {
            self.pwm_permille = delta.pwm_permille;
        }
        if delta.distance_mm.is_some() {
            self.distance_mm = delta.distance_mm;
        }
        if delta.pitch_mdeg.is_some() {
            self.pitch_mdeg = delta.pitch_mdeg;
        }
        if delta.roll_mdeg.is_some() {
            self.roll_mdeg = delta.roll_mdeg;
        }
        if delta.battery_percent_reported.is_some() {
            self.battery_percent_reported = delta.battery_percent_reported;
        }
        if delta.battery_percent_estimated.is_some() {
            self.battery_percent_estimated = delta.battery_percent_estimated;
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
        monotonic_ms: MonotonicMillis,
    },

    /// Timer tick supplied by the host.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicMillis,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    /// Link-up event accepted by the session.
    LinkUp(LinkInfo),

    /// Link-down event accepted by the session.
    LinkDown,

    /// Notification metadata accepted by the session.
    NotificationReceived {
        /// Transport endpoint that produced the bytes.
        channel: GattChannel,

        /// Host monotonic receive timestamp.
        monotonic_ms: MonotonicMillis,

        /// Number of notification bytes observed.
        len: usize,
    },

    /// Tick event accepted by the session.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicMillis,
    },

    /// Telemetry update emitted by a protocol session.
    Telemetry(TelemetryDelta),

    /// Read-only response emitted by a protocol session.
    ReadOnlyResponse(ReadOnlyResponse),

    /// Parser diagnostics emitted by a protocol session.
    Diagnostics(ParserDiagnostics),

    /// Detailed parser diagnostic error emitted by a protocol session.
    DiagnosticError(DiagnosticError),
}

/// Output emitted by a protocol session for the host to drain.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "transport write payloads stay inline to avoid per-write heap allocation"
)]
pub enum SessionOutput {
    /// Transport action to execute outside the protocol engine.
    Transport(TransportAction),

    /// Semantic event to report to the application.
    Event(DeviceEvent),
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
    snapshot: TelemetrySnapshot,
    diagnostics: ParserDiagnostics,
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
            snapshot: TelemetrySnapshot {
                at_ms: None,
                speed_mm_s: None,
                voltage_mv: None,
                battery_current_ma: None,
                motor_current_ma: None,
                power_mw: None,
                controller_temperature_mc: None,
                motor_temperature_mc: None,
                battery_temperature_mc: None,
                pwm_permille: None,
                distance_mm: None,
                pitch_mdeg: None,
                roll_mdeg: None,
                battery_percent_reported: None,
                battery_percent_estimated: None,
            },
            diagnostics: ParserDiagnostics {
                dropped_bytes: 0,
                resyncs: 0,
                bad_checksums: 0,
                timeouts: 0,
                oversized_frames: 0,
                malformed_frames: 0,
                unmatched_replies: 0,
            },
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
        monotonic_ms: MonotonicMillis,
    ) {
        let bytes = bytes.into_boxed_slice();
        self.handle(SessionInput::Notification {
            channel,
            bytes: &bytes,
            monotonic_ms,
        });
    }

    /// Supplies a host timer tick to the protocol session.
    pub fn tick(&mut self, monotonic_ms: MonotonicMillis) {
        self.handle(SessionInput::Tick { monotonic_ms });
    }

    /// Supplies a host command to the protocol session.
    pub fn issue_command(&mut self, command: DeviceCommand) {
        self.handle(SessionInput::Command(command));
    }

    /// Drains owned session outputs accumulated so far.
    #[must_use]
    pub fn drain_outputs(&mut self) -> Vec<SessionOutput> {
        core::mem::take(&mut self.output)
    }

    /// Returns the latest telemetry snapshot.
    #[must_use]
    pub const fn current_snapshot(&self) -> TelemetrySnapshot {
        self.snapshot
    }

    /// Returns accumulated parser diagnostics.
    #[must_use]
    pub const fn diagnostics(&self) -> ParserDiagnostics {
        self.diagnostics
    }

    fn handle(&mut self, input: SessionInput<'_>) {
        let start = self.output.len();
        self.session.handle(input, &mut self.output);
        self.apply_state_from_outputs(start);
    }

    fn apply_state_from_outputs(&mut self, start: usize) {
        for output in &self.output[start..] {
            if let SessionOutput::Event(event) = output {
                match event {
                    DeviceEvent::Telemetry(delta) => {
                        self.snapshot.apply_delta(*delta);
                    }
                    DeviceEvent::Diagnostics(diagnostics) => {
                        self.diagnostics.merge(*diagnostics);
                    }
                    DeviceEvent::ReadOnlyResponse(_)
                    | DeviceEvent::DiagnosticError(_)
                    | DeviceEvent::LinkUp(_)
                    | DeviceEvent::LinkDown
                    | DeviceEvent::NotificationReceived { .. }
                    | DeviceEvent::Tick { .. } => {}
                }
            }
        }
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
        monotonic_ms: MonotonicMillis,
    },

    /// Captured timer tick.
    Tick {
        /// Host monotonic tick timestamp.
        monotonic_ms: MonotonicMillis,
    },

    /// Captured host command.
    Command(DeviceCommand),
}

impl CaptureRecord {
    /// Creates a notification capture record with owned bytes.
    #[must_use]
    pub const fn notification(
        channel: GattChannel,
        bytes: Vec<u8>,
        monotonic_ms: MonotonicMillis,
    ) -> Self {
        Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        }
    }

    /// Splits a notification record into chunks no larger than `chunk_len`.
    ///
    /// Non-notification records are returned unchanged. A zero `chunk_len`
    /// leaves the record unchanged.
    #[must_use]
    pub fn split_notification_bytes(self, chunk_len: usize) -> Vec<Self> {
        let Self::Notification {
            channel,
            bytes,
            monotonic_ms,
        } = self
        else {
            return vec![self];
        };

        if chunk_len == 0 {
            return vec![Self::notification(channel, bytes, monotonic_ms)];
        }

        bytes
            .chunks(chunk_len)
            .map(|chunk| Self::notification(channel, chunk.to_vec(), monotonic_ms))
            .collect()
    }

    /// Splits a notification record by requested chunk lengths.
    ///
    /// Extra bytes are appended as a final chunk. Non-notification records are
    /// returned unchanged.
    #[must_use]
    pub fn split_notification_by_lengths(self, lengths: &[usize]) -> Vec<Self> {
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
        for length in lengths.iter().copied().filter(|length| *length > 0) {
            if offset >= bytes.len() {
                break;
            }
            let end = offset.saturating_add(length).min(bytes.len());
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
    for record in records {
        match record {
            CaptureRecord::LinkUp(link) => host.ingest_link_up(*link),
            CaptureRecord::LinkDown => host.ingest_link_down(),
            CaptureRecord::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => host.ingest_notification_owned(*channel, bytes.clone(), *monotonic_ms),
            CaptureRecord::Tick { monotonic_ms } => host.tick(*monotonic_ms),
            CaptureRecord::Command(command) => host.issue_command(*command),
        }
        outputs.extend(host.drain_outputs());
    }
    outputs
}

/// Summary of deterministic replay equivalence across notification chunking
/// modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayChunkComparison {
    /// Semantic event count from whole-notification replay.
    pub whole_semantic_events: usize,

    /// Semantic event count from one-byte notification replay.
    pub one_byte_semantic_events: usize,

    /// Semantic event count from arbitrary notification chunk replay.
    pub arbitrary_semantic_events: usize,

    /// Whether one-byte replay produced the same semantic events as whole
    /// replay.
    pub one_byte_matches: bool,

    /// Whether arbitrary chunk replay produced the same semantic events as
    /// whole replay.
    pub arbitrary_matches: bool,
}

/// Replays a capture and returns semantic events only.
///
/// Raw [`DeviceEvent::NotificationReceived`] metadata is intentionally
/// excluded because notification lengths differ between chunking modes even
/// when decoded protocol behavior is equivalent.
#[must_use]
pub fn replay_capture_semantic_events<S>(
    host: &mut HostSession<S>,
    records: &[CaptureRecord],
) -> Vec<DeviceEvent>
where
    S: ProtocolSession,
{
    replay_capture(host, records)
        .into_iter()
        .filter_map(|output| match output {
            SessionOutput::Event(DeviceEvent::NotificationReceived { .. })
            | SessionOutput::Transport(_) => None,
            SessionOutput::Event(event) => Some(event),
        })
        .collect()
}

/// Compares whole-notification replay against one-byte and arbitrary
/// notification chunk replay.
#[must_use]
pub fn compare_replay_capture_chunks<S, F>(
    mut make_session: F,
    records: &[CaptureRecord],
    arbitrary_lengths: &[usize],
) -> ReplayChunkComparison
where
    S: ProtocolSession,
    F: FnMut() -> S,
{
    let whole = replay_capture_semantic_events(&mut HostSession::new(make_session()), records);
    let one_byte_records = split_capture_notifications_by_len(records, 1);
    let one_byte =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &one_byte_records);
    let arbitrary_records = split_capture_notifications_by_lengths(records, arbitrary_lengths);
    let arbitrary =
        replay_capture_semantic_events(&mut HostSession::new(make_session()), &arbitrary_records);

    ReplayChunkComparison {
        whole_semantic_events: whole.len(),
        one_byte_semantic_events: one_byte.len(),
        arbitrary_semantic_events: arbitrary.len(),
        one_byte_matches: one_byte == whole,
        arbitrary_matches: arbitrary == whole,
    }
}

/// Builds a deterministic arbitrary notification chunk plan from replay
/// records.
///
/// The plan is sized to split the longest notification in the capture using a
/// repeating 2/3/5 byte pattern. Shorter notifications ignore extra chunk
/// lengths during replay.
#[must_use]
pub fn replay_arbitrary_chunk_lengths(records: &[CaptureRecord]) -> Vec<usize> {
    let max_notification_len = records
        .iter()
        .filter_map(|record| match record {
            CaptureRecord::Notification { bytes, .. } => Some(bytes.len()),
            CaptureRecord::LinkUp(_)
            | CaptureRecord::LinkDown
            | CaptureRecord::Tick { .. }
            | CaptureRecord::Command(_) => None,
        })
        .max()
        .unwrap_or_default();

    let mut lengths = Vec::new();
    let mut covered = 0usize;
    for chunk_len in [2usize, 3, 5].into_iter().cycle() {
        if covered >= max_notification_len {
            break;
        }
        let remaining = max_notification_len - covered;
        let next = chunk_len.min(remaining);
        lengths.push(next);
        covered += next;
    }
    lengths
}

fn split_capture_notifications_by_len(
    records: &[CaptureRecord],
    chunk_len: usize,
) -> Vec<CaptureRecord> {
    records
        .iter()
        .cloned()
        .flat_map(|record| record.split_notification_bytes(chunk_len))
        .collect()
}

fn split_capture_notifications_by_lengths(
    records: &[CaptureRecord],
    lengths: &[usize],
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
    use crate::{
        DeviceCommand, DeviceEvent, GattChannel, LinkInfo, Measured, ProtocolSession, SessionInput,
        SessionOutput, TelemetryDelta, TelemetrySnapshot, TransportAction, UnsupportedReason,
        ValueQuality, ValueSource, VerificationStatus, WriteMode, WritePayload,
    };
    use core::mem::size_of;
    use proptest::prelude::*;

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-core");
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
    fn battery_page_types_remain_small() {
        assert_eq!(size_of::<crate::BatteryPageKind>(), 1);
        assert_eq!(size_of::<crate::BatteryPageMetadata>(), 3);
        assert!(size_of::<crate::BatteryInfo>() <= 64);
        assert!(size_of::<crate::BatteryPagePayload>() <= 128);
        assert!(size_of::<crate::ReadOnlyResponse>() <= 104);
        assert_eq!(size_of::<SessionOutput>(), 128);
        assert_eq!(size_of::<TransportAction>(), 64);
    }

    #[test]
    fn inline_write_capacity_size_snapshot_quantifies_transport_cost() {
        assert_eq!(crate::MAX_TRANSPORT_WRITE_LEN, 512);
        assert_eq!(crate::MAX_INLINE_TRANSPORT_WRITE_LEN, 32);
        assert_eq!(size_of::<WritePayload>(), 40);
        assert_eq!(size_of::<TransportAction>(), 64);
        assert_eq!(size_of::<SessionOutput>(), 128);
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
        assert_eq!(size_of::<crate::ParserDiagnostics>(), 56);
        assert_eq!(size_of::<crate::DiagnosticSnapshot>(), 56);
        assert!(size_of::<crate::DiagnosticError>() <= 80);
        assert!(size_of::<TelemetrySnapshot>() <= 256);
        assert!(size_of::<crate::CaptureRecord>() <= 48);
        assert!(size_of::<crate::HostSession<EchoSession>>() <= 352);
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
                    output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                        channel,
                        monotonic_ms,
                        len: bytes.len(),
                    }));
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
                    | DeviceCommand::RequestSettings
                    | DeviceCommand::SetLights(_)
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
            monotonic_ms: 10,
            max_write_len: Some(185),
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
                monotonic_ms: 20,
            },
            &mut output,
        );

        assert_eq!(session.last_notification_len, 3);
        assert_eq!(
            output.as_slice(),
            &[SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms: 20,
                len: 3
            })]
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
    fn telemetry_delta_updates_only_present_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        let first = TelemetryDelta {
            at_ms: 100,
            speed_mm_s: Some(Measured::reported(1_500)),
            voltage_mv: Some(Measured::reported(81_000)),
            battery_current_ma: Some(Measured::reported(-2_000)),
            ..TelemetryDelta::empty(100)
        };
        let second = TelemetryDelta {
            at_ms: 150,
            motor_temperature_mc: Some(Measured::reported(42_500)),
            ..TelemetryDelta::empty(150)
        };

        snapshot.apply_delta(first);
        snapshot.apply_delta(second);

        assert_eq!(snapshot.at_ms, Some(150));
        assert_eq!(snapshot.speed_mm_s, Some(Measured::reported(1_500)));
        assert_eq!(snapshot.voltage_mv, Some(Measured::reported(81_000)));
        assert_eq!(
            snapshot.motor_temperature_mc,
            Some(Measured::reported(42_500))
        );
    }

    #[test]
    fn zero_measurement_is_not_unknown() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(TelemetryDelta {
            at_ms: 200,
            speed_mm_s: Some(Measured::reported(0)),
            battery_current_ma: Some(Measured::reported(0)),
            ..TelemetryDelta::empty(200)
        });

        assert_eq!(snapshot.speed_mm_s, Some(Measured::reported(0)));
        assert_eq!(snapshot.battery_current_ma, Some(Measured::reported(0)));
        assert_eq!(snapshot.motor_current_ma, None);
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
        let estimated_percent = Measured::estimated(76);

        snapshot.apply_delta(TelemetryDelta {
            at_ms: 300,
            battery_current_ma: Some(Measured::reported(-1_200)),
            motor_current_ma: Some(Measured::reported(3_400)),
            controller_temperature_mc: Some(Measured::reported(35_000)),
            motor_temperature_mc: Some(Measured::reported(45_000)),
            battery_temperature_mc: Some(Measured::reported(31_000)),
            battery_percent_reported: Some(Measured::reported(80)),
            battery_percent_estimated: Some(estimated_percent),
            ..TelemetryDelta::empty(300)
        });

        assert_eq!(
            snapshot.battery_current_ma,
            Some(Measured::reported(-1_200))
        );
        assert_eq!(snapshot.motor_current_ma, Some(Measured::reported(3_400)));
        assert_eq!(
            snapshot.controller_temperature_mc,
            Some(Measured::reported(35_000))
        );
        assert_eq!(
            snapshot.motor_temperature_mc,
            Some(Measured::reported(45_000))
        );
        assert_eq!(
            snapshot.battery_temperature_mc,
            Some(Measured::reported(31_000))
        );
        assert_eq!(
            snapshot.battery_percent_reported,
            Some(Measured::reported(80))
        );
        assert_eq!(snapshot.battery_percent_estimated, Some(estimated_percent));
        assert_eq!(
            snapshot
                .battery_percent_estimated
                .map(|value| value.verification),
            Some(VerificationStatus::Inferred)
        );
    }

    #[test]
    fn telemetry_delta_can_be_emitted_as_device_event() {
        let delta = TelemetryDelta {
            at_ms: 400,
            distance_mm: Some(Measured::reported(12_345)),
            ..TelemetryDelta::empty(400)
        };

        assert_eq!(
            DeviceEvent::Telemetry(delta),
            DeviceEvent::Telemetry(TelemetryDelta {
                at_ms: 400,
                distance_mm: Some(Measured::reported(12_345)),
                ..TelemetryDelta::empty(400)
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
            voltage_mv: Some(Measured::reported(80_400)),
            current_ma: Some(Measured::reported(0)),
            percent_reported: Some(Measured::reported(0)),
            percent_estimated: Some(Measured::estimated(42)),
            temperature_mc: None,
            raw_state: None,
        };
        let response = crate::BatteryPagePayload::Raw(crate::BatteryRawPage::new(
            crate::BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified),
            battery,
        ));

        assert_eq!(
            response.page(),
            crate::BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified)
        );
        assert_eq!(response.battery().current_ma, Some(Measured::reported(0)));
        assert_eq!(
            response.battery().percent_reported,
            Some(Measured::reported(0))
        );
        assert_eq!(
            response
                .battery()
                .voltage_mv
                .map(|value| value.verification),
            Some(VerificationStatus::HardwareVerified)
        );
        assert_eq!(
            response
                .battery()
                .percent_estimated
                .map(|value| value.verification),
            Some(VerificationStatus::Inferred)
        );
        assert_eq!(response.battery().temperature_mc, None);
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
        let response = crate::SettingsReadback {
            entries: [Some(entry), None, None, None],
        };

        assert_eq!(response.entries[0], Some(entry));
        assert_eq!(response.entries[1], None);
        assert_eq!(
            response.entries[0].map(|entry| entry.verification),
            Some(VerificationStatus::HardwareVerified)
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
        let entry = crate::ModelRegistryEntry {
            manufacturer: "NOSFET",
            model: "Aero",
            protocol_family: crate::ProtocolFamily::VeteranLeaperkimNosfet,
            advertised_name_hints: &["NF2557"],
            wire_model_id: Some(crate::VerifiedValue {
                value: 43_u16,
                verification: VerificationStatus::HardwareVerified,
            }),
            battery: Some(crate::BatterySpec {
                series_cells: 30,
                nominal_capacity_mah: Some(10_000),
                voltage_range_mv: 99_180..=123_370,
                verification: VerificationStatus::SourceAndHardwareVerified,
            }),
            bms: Some(crate::BmsLayoutSpec {
                series_cells: 30,
                parallel_packs: 2,
                cell_values_per_page: 15,
                temperature_values_per_page: 6,
                selectors: &[
                    crate::BmsPageSelectorSpec {
                        selector: 0,
                        kind: crate::BatteryPageKind::Metadata,
                        verification: VerificationStatus::HardwareVerified,
                    },
                    crate::BmsPageSelectorSpec {
                        selector: 1,
                        kind: crate::BatteryPageKind::CellVoltage,
                        verification: VerificationStatus::HardwareVerified,
                    },
                ],
                verification: VerificationStatus::HardwareVerified,
            }),
            gatt: &AERO_GATT,
            capabilities: crate::Capabilities::from_supported_commands([
                crate::CommandKind::RequestIdentity,
                crate::CommandKind::RequestTelemetry,
                crate::CommandKind::RequestFirmwareInfo,
                crate::CommandKind::RequestBatteryInfo,
                crate::CommandKind::RequestDiagnostics,
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
        assert_eq!(bms.series_cells, 30);
        assert_eq!(bms.parallel_packs, 2);
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
    fn bms_layout_spec_preserves_static_selector_map() {
        const SELECTORS: [crate::BmsPageSelectorSpec; 4] = [
            crate::BmsPageSelectorSpec {
                selector: 0,
                kind: crate::BatteryPageKind::Metadata,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: 1,
                kind: crate::BatteryPageKind::CellVoltage,
                verification: VerificationStatus::HardwareVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: 3,
                kind: crate::BatteryPageKind::Raw,
                verification: VerificationStatus::SourceVerified,
            },
            crate::BmsPageSelectorSpec {
                selector: 8,
                kind: crate::BatteryPageKind::Raw,
                verification: VerificationStatus::SourceVerified,
            },
        ];
        let layout = crate::BmsLayoutSpec {
            series_cells: 30,
            parallel_packs: 2,
            cell_values_per_page: 15,
            temperature_values_per_page: 6,
            selectors: &SELECTORS,
            verification: VerificationStatus::HardwareVerified,
        };

        assert_eq!(layout.selectors.len(), 4);
        assert_eq!(layout.selectors[2].selector, 3);
        assert_eq!(layout.selectors[2].kind, crate::BatteryPageKind::Raw);
        assert_eq!(
            layout.selectors[2].verification,
            VerificationStatus::SourceVerified
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
            manufacturer,
            model,
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
            selector: 1,
            kind: crate::BatteryPageKind::CellVoltage,
            verification: VerificationStatus::SourceVerified,
        }];
        let mut entry = sample_registry_entry(manufacturer, model);
        entry.bms = Some(crate::BmsLayoutSpec {
            series_cells,
            parallel_packs,
            cell_values_per_page: 15,
            temperature_values_per_page: 6,
            selectors: &SELECTORS,
            verification: VerificationStatus::Inferred,
        });
        entry
    }

    #[test]
    fn read_only_response_reports_matching_command_kind() {
        let firmware = crate::ReadOnlyResponse::Firmware(crate::FirmwareInfo::default());
        let battery = crate::ReadOnlyResponse::Battery(crate::BatteryPagePayload::Raw(
            crate::BatteryRawPage::new(
                crate::BatteryPageMetadata::raw(8, VerificationStatus::SourceVerified),
                crate::BatteryInfo::default(),
            ),
        ));
        let diagnostics = crate::ReadOnlyResponse::Diagnostics(crate::DiagnosticReadback {
            details: [None, None, None, None],
        });
        let settings = crate::ReadOnlyResponse::Settings(crate::SettingsReadback {
            entries: [None, None, None, None],
        });

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
            crate::RequestKey::new(crate::CommandKind::RequestSettings),
        ];

        for (index, key) in keys.into_iter().enumerate() {
            assert!(!keys[index + 1..].contains(&key));
        }
    }

    #[test]
    fn benign_controls_are_distinct_from_read_only_requests() {
        let lights = DeviceCommand::SetLights(crate::LightState::On);
        let horn = DeviceCommand::SoundHorn;

        assert_eq!(lights.kind(), crate::CommandKind::SetLights);
        assert_eq!(horn.kind(), crate::CommandKind::SoundHorn);
        assert_eq!(lights.safety_class(), crate::SafetyClass::BenignControl);
        assert_eq!(horn.safety_class(), crate::SafetyClass::BenignControl);
    }

    #[test]
    fn actuation_commands_are_not_supported_without_capability() {
        let capabilities = crate::Capabilities::default();
        let command = DeviceCommand::SetRawMotorCurrent { current_ma: 1_000 };

        assert_eq!(command.safety_class(), crate::SafetyClass::Actuation);
        assert_eq!(
            capabilities.check_command(command),
            Err(UnsupportedReason::CommandNotSupported(command.kind()))
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
    fn parser_limits_reject_oversized_frame_lengths() {
        let limits = crate::ParserLimits {
            max_frame_len: 24,
            ..crate::ParserLimits::default()
        };

        assert_eq!(limits.validate_frame_len(24), Ok(()));
        assert_eq!(
            limits.validate_frame_len(25),
            Err(crate::ParserError::OversizedFrame {
                claimed: 25,
                max: 24,
            })
        );
    }

    #[test]
    fn parser_diagnostics_saturate_counters() {
        let mut diagnostics = crate::ParserDiagnostics {
            dropped_bytes: u64::MAX,
            ..crate::ParserDiagnostics::default()
        };

        diagnostics.add_dropped_bytes(10);
        diagnostics.record_resync();
        diagnostics.record_error(crate::ParserError::BadChecksum);

        assert_eq!(diagnostics.dropped_bytes, u64::MAX);
        assert_eq!(diagnostics.resyncs, 1);
        assert_eq!(diagnostics.bad_checksums, 1);
    }

    #[test]
    fn parser_diagnostics_merge_with_saturating_counts() {
        let mut left = crate::ParserDiagnostics {
            timeouts: u64::MAX,
            malformed_frames: 2,
            ..crate::ParserDiagnostics::default()
        };
        let right = crate::ParserDiagnostics {
            timeouts: 1,
            unmatched_replies: 3,
            ..crate::ParserDiagnostics::default()
        };

        left.merge(right);

        assert_eq!(left.timeouts, u64::MAX);
        assert_eq!(left.malformed_frames, 2);
        assert_eq!(left.unmatched_replies, 3);
    }

    #[test]
    fn parser_errors_map_to_expected_diagnostic_counters() {
        let mut diagnostics = crate::ParserDiagnostics::default();

        diagnostics.record_error(crate::ParserError::OversizedFrame {
            claimed: 4_097,
            max: 4_096,
        });
        diagnostics.record_error(crate::ParserError::MalformedFrame);
        diagnostics.record_error(crate::ParserError::Timeout {
            elapsed_ms: 1_500,
            timeout_ms: 1_000,
        });
        diagnostics.record_error(crate::ParserError::UnmatchedReply);

        assert_eq!(diagnostics.oversized_frames, 1);
        assert_eq!(diagnostics.malformed_frames, 1);
        assert_eq!(diagnostics.timeouts, 1);
        assert_eq!(diagnostics.unmatched_replies, 1);
    }

    #[test]
    fn parser_diagnostics_can_be_emitted_as_device_event() {
        let diagnostics = crate::ParserDiagnostics {
            bad_checksums: 2,
            resyncs: 1,
            ..crate::ParserDiagnostics::default()
        };

        assert_eq!(
            DeviceEvent::Diagnostics(diagnostics),
            DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                bad_checksums: 2,
                resyncs: 1,
                ..crate::ParserDiagnostics::default()
            })
        );
    }

    #[test]
    fn diagnostic_error_can_be_emitted_as_device_event() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::Timeout {
            elapsed_ms: 1_500,
            timeout_ms: 1_000,
        });

        assert_eq!(
            DeviceEvent::DiagnosticError(error),
            DeviceEvent::DiagnosticError(crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed_ms: Some(1_500),
                timeout_ms: Some(1_000),
            })
        );
    }

    #[test]
    fn diagnostic_snapshot_preserves_counter_fields() {
        let diagnostics = crate::ParserDiagnostics {
            dropped_bytes: 1,
            resyncs: 2,
            bad_checksums: 3,
            timeouts: 4,
            oversized_frames: 5,
            malformed_frames: 6,
            unmatched_replies: 7,
        };

        assert_eq!(
            crate::DiagnosticSnapshot::from_parser_diagnostics(diagnostics),
            crate::DiagnosticSnapshot {
                dropped_bytes: 1,
                resyncs: 2,
                bad_checksums: 3,
                timeouts: 4,
                oversized_frames: 5,
                malformed_frames: 6,
                unmatched_replies: 7,
            }
        );
    }

    #[test]
    fn diagnostic_error_preserves_oversized_frame_details() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::OversizedFrame {
            claimed: 4_097,
            max: 4_096,
        });

        assert_eq!(
            error,
            crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::OversizedFrame,
                claimed_len: Some(4_097),
                max_len: Some(4_096),
                elapsed_ms: None,
                timeout_ms: None,
            }
        );
    }

    #[test]
    fn diagnostic_error_preserves_timeout_details() {
        let error = crate::DiagnosticError::from_parser_error(crate::ParserError::Timeout {
            elapsed_ms: 1_500,
            timeout_ms: 1_000,
        });

        assert_eq!(
            error,
            crate::DiagnosticError {
                kind: crate::DiagnosticErrorKind::Timeout,
                claimed_len: None,
                max_len: None,
                elapsed_ms: Some(1_500),
                timeout_ms: Some(1_000),
            }
        );
    }

    #[test]
    fn diagnostic_snapshot_maps_from_device_event() {
        let diagnostics = crate::ParserDiagnostics {
            bad_checksums: 2,
            ..crate::ParserDiagnostics::default()
        };

        assert_eq!(
            crate::DiagnosticSnapshot::from_device_event(DeviceEvent::Diagnostics(diagnostics)),
            Some(crate::DiagnosticSnapshot {
                bad_checksums: 2,
                ..crate::DiagnosticSnapshot::default()
            })
        );
        assert_eq!(
            crate::DiagnosticSnapshot::from_device_event(DeviceEvent::LinkDown),
            None
        );
    }

    #[test]
    fn request_tracker_enforces_write_pacing() {
        let policy = crate::RequestPolicy {
            min_interval_ms: 100,
            ..crate::RequestPolicy::default()
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        assert_eq!(tracker.start(key, policy, 1_000), Ok(()));
        assert_eq!(
            tracker.correlate_reply(key, &mut crate::ParserDiagnostics::default()),
            crate::CorrelationResult::Matched { key, attempts: 1 }
        );
        assert_eq!(
            tracker.start(key, policy, 1_050),
            Err(crate::RequestStartError::Pacing { ready_at_ms: 1_100 })
        );
        assert_eq!(tracker.start(key, policy, 1_100), Ok(()));
    }

    #[test]
    fn request_tracker_reports_retry_after_timeout() {
        let policy = crate::RequestPolicy {
            timeout_ms: 250,
            max_retries: 2,
            ..crate::RequestPolicy::default()
        };
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        tracker.start(key, policy, 10).unwrap();

        assert_eq!(tracker.on_tick(259), crate::RequestTick::Waiting);
        assert_eq!(
            tracker.on_tick(260),
            crate::RequestTick::Retry { key, attempt: 1 }
        );
        assert_eq!(tracker.retry_started(260), Ok(()));
        assert_eq!(
            tracker.on_tick(510),
            crate::RequestTick::Retry { key, attempt: 2 }
        );
        assert_eq!(tracker.retry_started(510), Ok(()));
        assert_eq!(
            tracker.on_tick(760),
            crate::RequestTick::TimedOut { key, attempts: 3 }
        );
    }

    #[test]
    fn request_tracker_correlates_reply_and_clears_slot() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let key = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);

        tracker.start(key, policy, 20).unwrap();

        assert_eq!(
            tracker.correlate_reply(key, &mut crate::ParserDiagnostics::default()),
            crate::CorrelationResult::Matched { key, attempts: 1 }
        );
        assert_eq!(tracker.in_flight(), None);
        assert_eq!(tracker.start(key, policy, 21), Ok(()));
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
        assert_eq!(diagnostics.unmatched_replies, 1);
    }

    #[test]
    fn request_tracker_serializes_ambiguous_overlaps() {
        let policy = crate::RequestPolicy::default();
        let mut tracker = crate::RequestTracker::default();
        let telemetry = crate::RequestKey::new(crate::CommandKind::RequestTelemetry);
        let identity = crate::RequestKey::new(crate::CommandKind::RequestIdentity);

        tracker.start(telemetry, policy, 20).unwrap();

        assert_eq!(
            tracker.start(identity, policy, 21),
            Err(crate::RequestStartError::Busy { key: telemetry })
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
            timeout_ms: 250,
            max_retries: 2,
            min_interval_ms: 50,
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
        fn poll_request_accepts_read_only_commands(value in 0u8..6) {
            let kind = match value {
                0 => crate::CommandKind::RequestIdentity,
                1 => crate::CommandKind::RequestTelemetry,
                2 => crate::CommandKind::RequestFirmwareInfo,
                3 => crate::CommandKind::RequestBatteryInfo,
                4 => crate::CommandKind::RequestDiagnostics,
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
            let percent_reported = include_zero.then_some(Measured::reported(0));
            let response = crate::BatteryInfo {
                percent_reported,
                ..crate::BatteryInfo::default()
            };

            if include_zero {
                prop_assert_eq!(response.percent_reported, Some(Measured::reported(0)));
            } else {
                prop_assert_eq!(response.percent_reported, None);
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
            monotonic_ms: 10,
            max_write_len: Some(185),
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

        host.ingest_notification_owned(channel, vec![0xdc, 0x5a, 0x5c], 20);

        assert_eq!(
            host.drain_outputs().as_slice(),
            &[SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms: 20,
                len: 3,
            })]
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
    struct StateSession;

    impl ProtocolSession for StateSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::Command(DeviceCommand::RequestTelemetry) => {
                    output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                        TelemetryDelta {
                            at_ms: 40,
                            speed_mm_s: Some(Measured::reported(1_200)),
                            ..TelemetryDelta::empty(40)
                        },
                    )));
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        crate::ParserDiagnostics {
                            timeouts: monotonic_ms,
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
    fn host_session_updates_current_snapshot_from_events() {
        let mut host = crate::HostSession::new(StateSession);

        host.issue_command(DeviceCommand::RequestTelemetry);

        assert_eq!(host.current_snapshot().at_ms, Some(40));
        assert_eq!(
            host.current_snapshot().speed_mm_s,
            Some(Measured::reported(1_200))
        );
    }

    #[test]
    fn host_session_merges_diagnostics_from_events() {
        let mut host = crate::HostSession::new(StateSession);

        host.tick(2);
        host.tick(3);

        assert_eq!(host.diagnostics().timeouts, 5);
    }

    #[test]
    fn diagnostic_snapshot_maps_from_host_session_diagnostics() {
        let mut host = crate::HostSession::new(StateSession);

        host.tick(2);

        assert_eq!(
            crate::DiagnosticSnapshot::from_parser_diagnostics(host.diagnostics()),
            crate::DiagnosticSnapshot {
                timeouts: 2,
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
                                    at_ms: 90,
                                    speed_mm_s: Some(Measured::reported(self.sum)),
                                    ..TelemetryDelta::empty(90)
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
                            unmatched_replies: command.kind() as u64,
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
                SessionOutput::Transport(_) => None,
            })
            .collect()
    }

    #[test]
    fn capture_record_owns_notification_payloads() {
        let channel = GattChannel::from_bytes([0x11; 16]);
        let source = vec![1, 2, 0xff];
        let record = crate::CaptureRecord::notification(channel, source.clone(), 10);

        assert_eq!(
            record,
            crate::CaptureRecord::Notification {
                channel,
                bytes: source,
                monotonic_ms: 10,
            }
        );
    }

    #[test]
    fn replay_capture_drives_link_tick_command_and_notification_records() {
        let channel = GattChannel::from_bytes([0x22; 16]);
        let link = LinkInfo {
            monotonic_ms: 1,
            max_write_len: Some(185),
        };
        let records = [
            crate::CaptureRecord::LinkUp(link),
            crate::CaptureRecord::Tick { monotonic_ms: 2 },
            crate::CaptureRecord::Command(DeviceCommand::RequestIdentity),
            crate::CaptureRecord::notification(channel, vec![4, 5, 0xff], 3),
            crate::CaptureRecord::LinkDown,
        ];

        assert_eq!(
            replay_events(&records).as_slice(),
            &[
                DeviceEvent::LinkUp(link),
                DeviceEvent::Tick { monotonic_ms: 2 },
                DeviceEvent::Diagnostics(crate::ParserDiagnostics {
                    unmatched_replies: crate::CommandKind::RequestIdentity as u64,
                    ..crate::ParserDiagnostics::default()
                }),
                DeviceEvent::Telemetry(TelemetryDelta {
                    at_ms: 90,
                    speed_mm_s: Some(Measured::reported(9)),
                    ..TelemetryDelta::empty(90)
                }),
                DeviceEvent::LinkDown,
            ]
        );
    }

    #[test]
    fn one_byte_notification_replay_matches_whole_notification_replay() {
        let channel = GattChannel::from_bytes([0x33; 16]);
        let whole = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 0xff],
            10,
        )];
        let one_byte = crate::CaptureRecord::notification(channel, vec![1, 2, 3, 0xff], 10)
            .split_notification_bytes(1);

        assert_eq!(replay_events(&one_byte), replay_events(&whole));
    }

    #[test]
    fn replay_chunk_comparison_ignores_notification_boundaries() {
        let channel = GattChannel::from_bytes([0x66; 16]);
        let records = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 0xff],
            10,
        )];

        let comparison =
            crate::compare_replay_capture_chunks(FramedCaptureSession::default, &records, &[2, 1]);

        assert_eq!(
            comparison,
            crate::ReplayChunkComparison {
                whole_semantic_events: 1,
                one_byte_semantic_events: 1,
                arbitrary_semantic_events: 1,
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
                        speed_mm_s: Some(Measured::reported(
                            i32::try_from(bytes.len()).unwrap_or(0),
                        )),
                        ..TelemetryDelta::empty(monotonic_ms)
                    },
                )));
            }
        }

        let channel = GattChannel::from_bytes([0x77; 16]);
        let records = [crate::CaptureRecord::notification(
            channel,
            vec![1, 2, 3, 4],
            10,
        )];

        let comparison =
            crate::compare_replay_capture_chunks(|| NotificationLengthSession, &records, &[2, 2]);

        assert!(!comparison.one_byte_matches);
        assert!(!comparison.arbitrary_matches);
    }

    #[test]
    fn replay_arbitrary_chunk_lengths_are_derived_from_capture_notifications() {
        let channel = GattChannel::from_bytes([0x78; 16]);
        let records = [
            crate::CaptureRecord::Tick { monotonic_ms: 1 },
            crate::CaptureRecord::notification(channel, vec![0; 4], 2),
            crate::CaptureRecord::notification(channel, vec![0; 10], 3),
            crate::CaptureRecord::LinkDown,
        ];

        assert_eq!(
            crate::replay_arbitrary_chunk_lengths(&records),
            vec![2, 3, 5]
        );
    }

    #[test]
    fn replay_arbitrary_chunk_lengths_are_empty_without_notifications() {
        assert_eq!(
            crate::replay_arbitrary_chunk_lengths(&[crate::CaptureRecord::Tick {
                monotonic_ms: 1
            }]),
            Vec::<usize>::new()
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
            let whole = [crate::CaptureRecord::notification(channel, payload.clone(), 20)];
            let chunks = crate::CaptureRecord::notification(channel, payload, 20)
                .split_notification_by_lengths(&lengths);

            prop_assert_eq!(replay_events(&chunks), replay_events(&whole));
        }
    }

    #[test]
    fn replay_summary_preserves_output_order() {
        let channel = GattChannel::from_bytes([0x55; 16]);
        let records = [
            crate::CaptureRecord::Tick { monotonic_ms: 1 },
            crate::CaptureRecord::notification(channel, vec![9, 0xff], 2),
            crate::CaptureRecord::Tick { monotonic_ms: 3 },
        ];
        let mut host = crate::HostSession::new(FramedCaptureSession::default());

        assert_eq!(
            crate::replay_capture(&mut host, &records).as_slice(),
            &[
                SessionOutput::Event(DeviceEvent::Tick { monotonic_ms: 1 }),
                SessionOutput::Event(DeviceEvent::Telemetry(TelemetryDelta {
                    at_ms: 90,
                    speed_mm_s: Some(Measured::reported(9)),
                    ..TelemetryDelta::empty(90)
                })),
                SessionOutput::Event(DeviceEvent::Tick { monotonic_ms: 3 }),
            ]
        );
    }
}
