#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

//! Protocol-family scaffolding for Cutout.

use arrayvec::ArrayVec;
use cutout_core::{
    Capabilities, CommandKind, DeviceCommand, DeviceEvent, GattChannel, Measured, MonotonicMillis,
    ParserDiagnostics, ParserError, PollRequest, PollingPlan, ProtocolSession, RequestPolicy,
    RequestQueue, RequestUrgency, SessionInput, SessionOutput, TelemetryDelta, TransportAction,
    WriteMode, WritePayload, WritePayloadTooLong,
};
use thiserror::Error;

/// Placeholder write channel for NOSFET/Veteran-family sessions.
pub const AERO_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xA1; 16]);

/// Placeholder write channel for Begode-family sessions.
pub const FALCON_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xB1; 16]);

const DEFAULT_POLICY: RequestPolicy = RequestPolicy {
    timeout_ms: 1_000,
    max_retries: 1,
    min_interval_ms: 100,
};

const MAX_PROVISIONAL_REQUEST_LEN: usize = 24;
const MAX_VETERAN_FRAME_LEN: usize = 259;
const VETERAN_SHORT_FRAME_MAX_LEN: u8 = 38;

const AERO_POLL_PLAN: PollingPlan<5> = PollingPlan::new([
    PollRequest::new(
        CommandKind::RequestTelemetry,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestBatteryInfo,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestIdentity,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestFirmwareInfo,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestDiagnostics,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
]);

const FALCON_POLL_PLAN: PollingPlan<4> = PollingPlan::new([
    PollRequest::new(
        CommandKind::RequestTelemetry,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestIdentity,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestFirmwareInfo,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestBatteryInfo,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
]);

/// NOSFET/Veteran-family read-only probe identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AeroProbe {
    /// Request identity/model information.
    Identity,

    /// Request firmware or protocol version information.
    FirmwareInfo,

    /// Request live telemetry.
    Telemetry,

    /// Request battery or BMS information.
    BatteryInfo,

    /// Request diagnostic information.
    Diagnostics,
}

impl AeroProbe {
    /// Maps a generic command kind to an Aero/Veteran probe.
    #[must_use]
    pub const fn from_command_kind(kind: CommandKind) -> Option<Self> {
        match kind {
            CommandKind::RequestIdentity => Some(Self::Identity),
            CommandKind::RequestFirmwareInfo => Some(Self::FirmwareInfo),
            CommandKind::RequestTelemetry => Some(Self::Telemetry),
            CommandKind::RequestBatteryInfo => Some(Self::BatteryInfo),
            CommandKind::RequestDiagnostics => Some(Self::Diagnostics),
            CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => None,
        }
    }

    /// Returns the temporary placeholder label for this probe.
    #[must_use]
    pub const fn placeholder_label(self) -> &'static [u8] {
        match self {
            Self::Identity => b"aero:identity",
            Self::FirmwareInfo => b"aero:firmware",
            Self::Telemetry => b"aero:telemetry",
            Self::BatteryInfo => b"aero:battery",
            Self::Diagnostics => b"aero:diagnostics",
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Identity => CommandKind::RequestIdentity,
            Self::FirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::Telemetry => CommandKind::RequestTelemetry,
            Self::BatteryInfo => CommandKind::RequestBatteryInfo,
            Self::Diagnostics => CommandKind::RequestDiagnostics,
        }
    }
}

/// Begode-family read-only probe identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FalconProbe {
    /// Request identity/model information.
    Identity,

    /// Request firmware or protocol version information.
    FirmwareInfo,

    /// Request live telemetry.
    Telemetry,

    /// Request battery or BMS information.
    BatteryInfo,
}

impl FalconProbe {
    /// Maps a generic command kind to a Begode/Falcon probe.
    #[must_use]
    pub const fn from_command_kind(kind: CommandKind) -> Option<Self> {
        match kind {
            CommandKind::RequestIdentity => Some(Self::Identity),
            CommandKind::RequestFirmwareInfo => Some(Self::FirmwareInfo),
            CommandKind::RequestTelemetry => Some(Self::Telemetry),
            CommandKind::RequestBatteryInfo => Some(Self::BatteryInfo),
            CommandKind::RequestDiagnostics
            | CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent => None,
        }
    }

    /// Returns the temporary placeholder label for this probe.
    #[must_use]
    pub const fn placeholder_label(self) -> &'static [u8] {
        match self {
            Self::Identity => b"falcon:identity",
            Self::FirmwareInfo => b"falcon:firmware",
            Self::Telemetry => b"falcon:telemetry",
            Self::BatteryInfo => b"falcon:battery",
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Identity => CommandKind::RequestIdentity,
            Self::FirmwareInfo => CommandKind::RequestFirmwareInfo,
            Self::Telemetry => CommandKind::RequestTelemetry,
            Self::BatteryInfo => CommandKind::RequestBatteryInfo,
        }
    }
}

/// Bounded encoded request payload plus correlation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRequest<P> {
    /// Family-specific probe represented by this request.
    pub probe: P,

    /// Generic command kind used for scheduler and response correlation.
    pub command: CommandKind,

    /// Bounded provisional request bytes.
    pub payload: ArrayVec<u8, MAX_PROVISIONAL_REQUEST_LEN>,

    /// GATT write mode required by this request.
    pub mode: WriteMode,
}

/// Provisional request encoder for NOSFET Aero/Veteran-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AeroRequestEncoder;

impl AeroRequestEncoder {
    /// Encodes a supported Aero/Veteran-family probe.
    #[must_use]
    pub fn encode(probe: AeroProbe) -> EncodedRequest<AeroProbe> {
        EncodedRequest {
            probe,
            command: probe.command_kind(),
            payload: provisional_payload(probe.placeholder_label()),
            mode: WriteMode::WithResponse,
        }
    }

    /// Encodes a generic command if it belongs to the Aero/Veteran probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<AeroProbe>> {
        AeroProbe::from_command_kind(kind).map(Self::encode)
    }
}

/// Provisional request encoder for Begode/Falcon-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FalconRequestEncoder;

impl FalconRequestEncoder {
    /// Encodes a supported Begode/Falcon-family probe.
    #[must_use]
    pub fn encode(probe: FalconProbe) -> EncodedRequest<FalconProbe> {
        EncodedRequest {
            probe,
            command: probe.command_kind(),
            payload: provisional_payload(probe.placeholder_label()),
            mode: WriteMode::WithResponse,
        }
    }

    /// Encodes a generic command if it belongs to the Begode/Falcon probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<FalconProbe>> {
        FalconProbe::from_command_kind(kind).map(Self::encode)
    }
}

/// Protocol device family used by capture-backed fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceFamily {
    /// NOSFET Aero or Veteran-family protocol.
    NosfetAero,

    /// Begode Falcon or Begode-family protocol.
    BegodeFalcon,
}

/// Classification result for an observed notification byte prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolFamilyClassification {
    /// Enough bytes were observed to identify a supported protocol family.
    Known(DeviceFamily),

    /// The observed prefix can still become a known family when more bytes arrive.
    Pending,

    /// The observed prefix cannot match a supported protocol family.
    Unknown,
}

/// Classifies protocol families that share the generic FFE0/FFE1 GATT profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProtocolFamilyClassifier;

impl ProtocolFamilyClassifier {
    /// Returns the protocol family indicated by the first notification bytes.
    #[must_use]
    pub fn classify(bytes: &[u8]) -> ProtocolFamilyClassification {
        if matches_prefix(bytes, &[0xdc, 0x5a, 0x5c]) {
            return complete_or_pending(bytes, 3, DeviceFamily::NosfetAero);
        }
        if matches_prefix(bytes, &[0x55, 0xaa]) {
            return complete_or_pending(bytes, 2, DeviceFamily::BegodeFalcon);
        }
        ProtocolFamilyClassification::Unknown
    }
}

fn complete_or_pending(
    bytes: &[u8],
    required_len: usize,
    family: DeviceFamily,
) -> ProtocolFamilyClassification {
    if bytes.len() >= required_len {
        ProtocolFamilyClassification::Known(family)
    } else {
        ProtocolFamilyClassification::Pending
    }
}

fn matches_prefix(bytes: &[u8], prefix: &[u8]) -> bool {
    if bytes.len() <= prefix.len() {
        prefix.starts_with(bytes)
    } else {
        bytes.starts_with(prefix)
    }
}

/// Complete Veteran/LeaperKim/NOSFET frame reassembled from BLE notifications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VeteranFrame {
    bytes: ArrayVec<u8, MAX_VETERAN_FRAME_LEN>,
}

impl VeteranFrame {
    /// Builds a frame from already-reassembled bytes.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranReassemblyError::InvalidFrame`] when the bytes do not
    /// contain the Veteran magic, length byte, and declared frame length.
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, VeteranReassemblyError> {
        if !bytes.starts_with(&[0xdc, 0x5a, 0x5c]) {
            return Err(VeteranReassemblyError::InvalidFrame);
        }
        let Some(len) = bytes.get(3) else {
            return Err(VeteranReassemblyError::InvalidFrame);
        };
        if bytes.len() != usize::from(*len) + 4 {
            return Err(VeteranReassemblyError::InvalidFrame);
        }
        let Ok(bytes) = ArrayVec::try_from(bytes) else {
            return Err(VeteranReassemblyError::InvalidFrame);
        };
        Ok(Self { bytes })
    }

    /// Returns the complete frame bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

/// Known Veteran/LeaperKim/NOSFET model identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VeteranModel {
    /// NOSFET Aero, reported as Veteran model id 43.
    NosfetAero,
}

impl VeteranModel {
    /// Resolves a raw Veteran model id.
    #[must_use]
    pub const fn from_model_id(model_id: u16) -> Option<Self> {
        match model_id {
            43 => Some(Self::NosfetAero),
            _ => None,
        }
    }
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

/// Read-only telemetry decoded from a Veteran/LeaperKim/NOSFET frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VeteranTelemetry {
    /// Known model, when Cutout recognizes the reported model id.
    pub model: Option<VeteranModel>,

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
}

impl VeteranTelemetry {
    /// Decodes telemetry fields from a complete Veteran frame.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranDecodeError::FrameTooShort`] if the frame does not
    /// contain the fixed telemetry header fields.
    pub fn decode(frame: &VeteranFrame) -> Result<Self, VeteranDecodeError> {
        let bytes = frame.as_slice();
        let raw_version = read_be_u16(bytes, 28).ok_or(VeteranDecodeError::FrameTooShort)?;
        let firmware = VeteranFirmwareVersion::from_raw(raw_version);

        Ok(Self {
            model: VeteranModel::from_model_id(firmware.model_id),
            firmware,
            voltage_mv: i32::from(read_be_u16(bytes, 4).ok_or(VeteranDecodeError::FrameTooShort)?)
                * 10,
            speed_deci_kmh: read_be_i16(bytes, 6).ok_or(VeteranDecodeError::FrameTooShort)?,
            trip_distance_m: read_veteran_swapped_u32(bytes, 8)
                .ok_or(VeteranDecodeError::FrameTooShort)?,
            total_distance_m: read_veteran_swapped_u32(bytes, 12)
                .ok_or(VeteranDecodeError::FrameTooShort)?,
            phase_current_deci_a: read_be_i16(bytes, 16)
                .ok_or(VeteranDecodeError::FrameTooShort)?,
            mosfet_temperature_mc: i32::from(
                read_be_i16(bytes, 18).ok_or(VeteranDecodeError::FrameTooShort)?,
            ) * 10,
            auto_off_seconds: read_be_u16(bytes, 20).ok_or(VeteranDecodeError::FrameTooShort)?,
            charge_mode: read_be_u16(bytes, 22).ok_or(VeteranDecodeError::FrameTooShort)?,
            speed_alert_deci_kmh: read_be_u16(bytes, 24)
                .ok_or(VeteranDecodeError::FrameTooShort)?,
            speed_tiltback_deci_kmh: read_be_u16(bytes, 26)
                .ok_or(VeteranDecodeError::FrameTooShort)?,
            pedals_mode: read_be_u16(bytes, 30).ok_or(VeteranDecodeError::FrameTooShort)?,
            pitch_mdeg: i32::from(read_be_i16(bytes, 32).ok_or(VeteranDecodeError::FrameTooShort)?)
                * 10,
            hardware_pwm_raw: read_be_u16(bytes, 34).ok_or(VeteranDecodeError::FrameTooShort)?,
        })
    }

    /// Converts decoded telemetry into the transport-independent telemetry delta.
    #[must_use]
    pub fn to_delta(self, at_ms: MonotonicMillis) -> TelemetryDelta {
        TelemetryDelta {
            at_ms,
            speed_mm_s: Some(Measured::reported(deci_kmh_to_mm_s(self.speed_deci_kmh))),
            voltage_mv: Some(Measured::reported(self.voltage_mv)),
            motor_current_ma: Some(Measured::reported(
                i32::from(self.phase_current_deci_a) * 100,
            )),
            controller_temperature_mc: Some(Measured::reported(self.mosfet_temperature_mc)),
            pwm_permille: Some(Measured::reported(veteran_pwm_permille(
                self.hardware_pwm_raw,
            ))),
            distance_mm: Some(Measured::reported(u64::from(self.total_distance_m) * 1_000)),
            pitch_mdeg: Some(Measured::reported(self.pitch_mdeg)),
            ..TelemetryDelta::empty(at_ms)
        }
    }
}

/// Error emitted while decoding a complete Veteran frame.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VeteranDecodeError {
    /// The frame is too short to contain fixed telemetry fields.
    #[error("Veteran telemetry frame too short")]
    FrameTooShort,
}

/// Error emitted while reassembling Veteran/LeaperKim/NOSFET frames.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VeteranReassemblyError {
    /// A complete long frame failed CRC32 validation.
    #[error("Veteran frame CRC mismatch")]
    CrcMismatch,

    /// A complete frame was structurally invalid.
    #[error("invalid Veteran frame")]
    InvalidFrame,
}

/// Sync reassembler for Veteran/LeaperKim/NOSFET `dc5a5c` notification streams.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VeteranFrameReassembler {
    buffer: ArrayVec<u8, MAX_VETERAN_FRAME_LEN>,
}

impl VeteranFrameReassembler {
    /// Feeds one notification byte into the reassembler.
    ///
    /// # Errors
    ///
    /// Returns [`VeteranReassemblyError::CrcMismatch`] when a long frame's
    /// CRC32 trailer does not match the frame contents.
    pub fn feed_byte(&mut self, byte: u8) -> Result<Option<VeteranFrame>, VeteranReassemblyError> {
        self.push_resyncing(byte);
        let Some(expected_len) = self.expected_len() else {
            return Ok(None);
        };
        if self.buffer.len() < expected_len {
            return Ok(None);
        }

        if self.uses_crc() && !self.crc_matches() {
            self.buffer.clear();
            return Err(VeteranReassemblyError::CrcMismatch);
        }

        let frame = VeteranFrame::try_from_slice(self.buffer.as_slice())?;
        self.buffer.clear();
        Ok(Some(frame))
    }

    fn push_resyncing(&mut self, byte: u8) {
        match self.buffer.len() {
            0 => {
                if byte == 0xdc {
                    self.buffer.push(byte);
                }
            }
            1 => {
                if byte == 0x5a {
                    self.buffer.push(byte);
                } else {
                    self.buffer.clear();
                    if byte == 0xdc {
                        self.buffer.push(byte);
                    }
                }
            }
            2 => {
                if byte == 0x5c {
                    self.buffer.push(byte);
                } else {
                    self.buffer.clear();
                    if byte == 0xdc {
                        self.buffer.push(byte);
                    }
                }
            }
            _ => self.buffer.push(byte),
        }
    }

    fn expected_len(&self) -> Option<usize> {
        self.buffer.get(3).map(|len| usize::from(*len) + 4)
    }

    fn uses_crc(&self) -> bool {
        self.buffer
            .get(3)
            .is_some_and(|len| *len > VETERAN_SHORT_FRAME_MAX_LEN)
    }

    fn crc_matches(&self) -> bool {
        let Some(declared_len) = self.buffer.get(3).copied().map(usize::from) else {
            return false;
        };
        let Some(expected_crc) = read_be_u32(self.buffer.as_slice(), declared_len) else {
            return false;
        };
        let Some(crc_bytes) = self.buffer.as_slice().get(..declared_len) else {
            return false;
        };
        crc32fast::hash(crc_bytes) == expected_crc
    }
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let b0 = *bytes.get(offset)?;
    let b1 = *bytes.get(offset + 1)?;
    let b2 = *bytes.get(offset + 2)?;
    let b3 = *bytes.get(offset + 3)?;
    Some(u32::from_be_bytes([b0, b1, b2, b3]))
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

fn veteran_pwm_permille(raw: u16) -> i16 {
    i16::try_from(raw / 10).unwrap_or(i16::MAX)
}

/// Family-specific request probe used by fixture records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolProbe {
    /// NOSFET/Veteran-family probe.
    Aero(AeroProbe),

    /// Begode/Falcon-family probe.
    Falcon(FalconProbe),
}

impl ProtocolProbe {
    /// Maps a device family and generic command kind into a protocol probe.
    ///
    /// # Errors
    ///
    /// Returns [`RequestFixtureError::UnsupportedCommand`] when the command is
    /// unsupported by the selected family.
    pub fn from_family_command(
        family: DeviceFamily,
        command: CommandKind,
    ) -> Result<Self, RequestFixtureError> {
        match family {
            DeviceFamily::NosfetAero => AeroProbe::from_command_kind(command)
                .map(Self::Aero)
                .ok_or(RequestFixtureError::UnsupportedCommand { family, command }),
            DeviceFamily::BegodeFalcon => FalconProbe::from_command_kind(command)
                .map(Self::Falcon)
                .ok_or(RequestFixtureError::UnsupportedCommand { family, command }),
        }
    }

    /// Returns the family that owns this probe.
    #[must_use]
    pub const fn family(self) -> DeviceFamily {
        match self {
            Self::Aero(_) => DeviceFamily::NosfetAero,
            Self::Falcon(_) => DeviceFamily::BegodeFalcon,
        }
    }

    /// Returns the generic command kind correlated with this probe.
    #[must_use]
    pub const fn command_kind(self) -> CommandKind {
        match self {
            Self::Aero(probe) => probe.command_kind(),
            Self::Falcon(probe) => probe.command_kind(),
        }
    }
}

/// Optional service/characteristic channels observed for a fixture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FixtureChannels {
    /// Optional GATT service or endpoint group identifier.
    pub service: Option<GattChannel>,

    /// Optional GATT characteristic or write endpoint identifier.
    pub characteristic: Option<GattChannel>,
}

/// Provenance category for capture-backed request fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureProvenance {
    /// Observed from a Bluetooth capture.
    BluetoothCapture,

    /// Observed from an application trace.
    AppTrace,

    /// Taken from source-attributed vendor or protocol documentation.
    VendorDocumentation,
}

/// Whether request fixture bytes have been verified against real hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareVerification {
    /// Fixture bytes have not been verified against real Bluetooth hardware.
    Unverified,

    /// Fixture bytes have been verified against real Bluetooth hardware.
    VerifiedOnBluetooth,
}

/// Capture/spec-backed request fixture record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestFixture {
    /// Device family the fixture applies to.
    pub family: DeviceFamily,

    /// Family-specific probe encoded by this fixture.
    pub probe: ProtocolProbe,

    /// Generic command kind used for scheduler and response correlation.
    pub command: CommandKind,

    /// Transport write behavior observed for the request.
    pub mode: WriteMode,

    /// Bounded request bytes.
    pub bytes: WritePayload,

    /// Optional service/characteristic evidence.
    pub channels: FixtureChannels,

    /// Source category for the fixture evidence.
    pub provenance: FixtureProvenance,

    /// Hardware verification state for the fixture.
    pub hardware_verification: HardwareVerification,
}

impl RequestFixture {
    /// Creates a request fixture after validating family/probe and byte bounds.
    ///
    /// # Errors
    ///
    /// Returns [`RequestFixtureError::FamilyMismatch`] when the probe belongs
    /// to a different family, or [`RequestFixtureError::PayloadTooLong`] when
    /// the request bytes exceed the core transport write bound.
    pub fn new(
        family: DeviceFamily,
        probe: ProtocolProbe,
        mode: WriteMode,
        bytes: &[u8],
        channels: FixtureChannels,
        provenance: FixtureProvenance,
        hardware_verification: HardwareVerification,
    ) -> Result<Self, RequestFixtureError> {
        let probe_family = probe.family();
        if family != probe_family {
            return Err(RequestFixtureError::FamilyMismatch {
                family,
                probe_family,
            });
        }
        Ok(Self {
            family,
            probe,
            command: probe.command_kind(),
            mode,
            bytes: WritePayload::try_from_slice(bytes)
                .map_err(RequestFixtureError::PayloadTooLong)?,
            channels,
            provenance,
            hardware_verification,
        })
    }
}

/// Request fixture validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RequestFixtureError {
    /// Probe belongs to a different protocol family.
    #[error("fixture family {family:?} does not match probe family {probe_family:?}")]
    FamilyMismatch {
        /// Fixture device family.
        family: DeviceFamily,

        /// Family implied by the probe.
        probe_family: DeviceFamily,
    },

    /// Command is unsupported by a protocol family.
    #[error("command {command:?} is unsupported by fixture family {family:?}")]
    UnsupportedCommand {
        /// Device family requested for mapping.
        family: DeviceFamily,

        /// Unsupported command.
        command: CommandKind,
    },

    /// Request bytes exceed the transport payload bound.
    #[error(transparent)]
    PayloadTooLong(WritePayloadTooLong),
}

/// Initial read-only session shell for NOSFET Aero/Veteran-family devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AeroReadOnlySession {
    connected: bool,
    queue: RequestQueue<5>,
    reassembler: VeteranFrameReassembler,
}

impl AeroReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub const fn capabilities() -> Capabilities {
        Capabilities::from_supported_commands([
            CommandKind::RequestIdentity,
            CommandKind::RequestFirmwareInfo,
            CommandKind::RequestTelemetry,
            CommandKind::RequestBatteryInfo,
            CommandKind::RequestDiagnostics,
        ])
    }
}

impl ProtocolSession for AeroReadOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(info) => {
                self.connected = true;
                self.queue = RequestQueue::new();
                self.reassembler = VeteranFrameReassembler::default();
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: AERO_WRITE_CHANNEL,
                }));
            }
            SessionInput::LinkDown => {
                self.connected = false;
                self.queue = RequestQueue::new();
                self.reassembler = VeteranFrameReassembler::default();
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                if self.connected {
                    enqueue_aero_plan(&mut self.queue);
                    drain_aero_queue(&mut self.queue, output);
                }
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => {
                output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                    channel,
                    monotonic_ms,
                    len: bytes.len(),
                }));
                handle_aero_notification(&mut self.reassembler, bytes, monotonic_ms, output);
            }
            SessionInput::Command(
                DeviceCommand::RequestIdentity
                | DeviceCommand::RequestTelemetry
                | DeviceCommand::RequestFirmwareInfo
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

fn handle_aero_notification(
    reassembler: &mut VeteranFrameReassembler,
    bytes: &[u8],
    monotonic_ms: MonotonicMillis,
    output: &mut Vec<SessionOutput>,
) {
    for byte in bytes {
        match reassembler.feed_byte(*byte) {
            Ok(Some(frame)) => push_aero_frame(&frame, monotonic_ms, output),
            Ok(None) => {}
            Err(VeteranReassemblyError::CrcMismatch) => {
                output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                    diagnostics_for(ParserError::BadChecksum),
                )));
            }
            Err(VeteranReassemblyError::InvalidFrame) => {
                output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                    diagnostics_for(ParserError::MalformedFrame),
                )));
            }
        }
    }
}

fn push_aero_frame(
    frame: &VeteranFrame,
    monotonic_ms: MonotonicMillis,
    output: &mut Vec<SessionOutput>,
) {
    match VeteranTelemetry::decode(frame) {
        Ok(telemetry) => output.push(SessionOutput::Event(DeviceEvent::Telemetry(
            telemetry.to_delta(monotonic_ms),
        ))),
        Err(VeteranDecodeError::FrameTooShort) => {
            output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                diagnostics_for(ParserError::MalformedFrame),
            )));
        }
    }
}

fn diagnostics_for(error: ParserError) -> ParserDiagnostics {
    let mut diagnostics = ParserDiagnostics::default();
    diagnostics.record_error(error);
    diagnostics
}

/// Initial read-only session shell for Begode Falcon devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FalconReadOnlySession {
    connected: bool,
    queue: RequestQueue<4>,
}

impl FalconReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub const fn capabilities() -> Capabilities {
        Capabilities::from_supported_commands([
            CommandKind::RequestIdentity,
            CommandKind::RequestFirmwareInfo,
            CommandKind::RequestTelemetry,
            CommandKind::RequestBatteryInfo,
        ])
    }
}

impl ProtocolSession for FalconReadOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(info) => {
                self.connected = true;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: FALCON_WRITE_CHANNEL,
                }));
            }
            SessionInput::LinkDown => {
                self.connected = false;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                if self.connected {
                    enqueue_falcon_plan(&mut self.queue);
                    drain_falcon_queue(&mut self.queue, output);
                }
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms,
                len: bytes.len(),
            })),
            SessionInput::Command(
                DeviceCommand::RequestIdentity
                | DeviceCommand::RequestTelemetry
                | DeviceCommand::RequestFirmwareInfo
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

fn enqueue_aero_plan(queue: &mut RequestQueue<5>) {
    let _result = AERO_POLL_PLAN.enqueue_into(queue);
}

fn enqueue_falcon_plan(queue: &mut RequestQueue<4>) {
    let _result = FALCON_POLL_PLAN.enqueue_into(queue);
}

fn drain_aero_queue(queue: &mut RequestQueue<5>, output: &mut Vec<SessionOutput>) {
    while let Some(request) = queue.pop_next() {
        let Some(encoded) = AeroRequestEncoder::encode_command(request.key.command) else {
            continue;
        };
        let Ok(bytes) = WritePayload::try_from_slice(encoded.payload.as_slice()) else {
            continue;
        };
        output.push(SessionOutput::Transport(TransportAction::Write {
            channel: AERO_WRITE_CHANNEL,
            bytes,
            mode: encoded.mode,
        }));
    }
}

fn drain_falcon_queue(queue: &mut RequestQueue<4>, output: &mut Vec<SessionOutput>) {
    while let Some(request) = queue.pop_next() {
        let Some(encoded) = FalconRequestEncoder::encode_command(request.key.command) else {
            continue;
        };
        let Ok(bytes) = WritePayload::try_from_slice(encoded.payload.as_slice()) else {
            continue;
        };
        output.push(SessionOutput::Transport(TransportAction::Write {
            channel: FALCON_WRITE_CHANNEL,
            bytes,
            mode: encoded.mode,
        }));
    }
}

fn provisional_payload(bytes: &[u8]) -> ArrayVec<u8, MAX_PROVISIONAL_REQUEST_LEN> {
    let mut payload = ArrayVec::new();
    for byte in bytes {
        let pushed = payload.try_push(*byte);
        debug_assert!(pushed.is_ok());
    }
    payload
}

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-protocols"
}

#[cfg(test)]
mod tests {
    use super::crate_name;
    use cutout_core::{
        Capabilities, CommandKind, DeviceEvent, LinkInfo, Measured, ProtocolSession, SessionInput,
        SessionOutput, TransportAction, WriteMode,
    };

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-protocols");
    }

    #[test]
    fn aero_session_exposes_read_only_capabilities() {
        let capabilities = crate::AeroReadOnlySession::capabilities();

        assert_eq!(
            capabilities,
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
                CommandKind::RequestDiagnostics,
            ])
        );
    }

    #[test]
    fn falcon_session_exposes_read_only_capabilities() {
        let capabilities = crate::FalconReadOnlySession::capabilities();

        assert_eq!(
            capabilities,
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
            ])
        );
    }

    #[test]
    fn aero_session_requests_subscription_on_link_up() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::Transport(TransportAction::Subscribe {
                channel: crate::AERO_WRITE_CHANNEL
            })
        )));
    }

    #[test]
    fn falcon_session_requests_subscription_on_link_up() {
        let mut session = crate::FalconReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );

        assert!(output.iter().any(|item| matches!(
            item,
            SessionOutput::Transport(TransportAction::Subscribe {
                channel: crate::FALCON_WRITE_CHANNEL
            })
        )));
    }

    #[test]
    fn aero_probe_mapping_accepts_supported_read_only_commands() {
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestIdentity),
            Some(crate::AeroProbe::Identity)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestFirmwareInfo),
            Some(crate::AeroProbe::FirmwareInfo)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestTelemetry),
            Some(crate::AeroProbe::Telemetry)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestBatteryInfo),
            Some(crate::AeroProbe::BatteryInfo)
        );
        assert_eq!(
            crate::AeroProbe::from_command_kind(CommandKind::RequestDiagnostics),
            Some(crate::AeroProbe::Diagnostics)
        );
    }

    #[test]
    fn falcon_probe_mapping_rejects_unsupported_read_only_commands() {
        assert_eq!(
            crate::FalconProbe::from_command_kind(CommandKind::RequestDiagnostics),
            None
        );
        assert_eq!(
            crate::FalconProbe::from_command_kind(CommandKind::RequestSettings),
            None
        );
    }

    #[test]
    fn probe_placeholder_labels_are_stable() {
        assert_eq!(
            crate::AeroProbe::Identity.placeholder_label(),
            b"aero:identity"
        );
        assert_eq!(
            crate::AeroProbe::Diagnostics.placeholder_label(),
            b"aero:diagnostics"
        );
        assert_eq!(
            crate::FalconProbe::FirmwareInfo.placeholder_label(),
            b"falcon:firmware"
        );
        assert_eq!(
            crate::FalconProbe::BatteryInfo.placeholder_label(),
            b"falcon:battery"
        );
    }

    #[test]
    fn aero_encoder_preserves_probe_command_write_mode_and_payload() {
        let request = crate::AeroRequestEncoder::encode(crate::AeroProbe::FirmwareInfo);

        assert_eq!(request.probe, crate::AeroProbe::FirmwareInfo);
        assert_eq!(request.command, CommandKind::RequestFirmwareInfo);
        assert_eq!(request.mode, WriteMode::WithResponse);
        assert_eq!(request.payload.as_slice(), b"aero:firmware");
    }

    #[test]
    fn falcon_encoder_preserves_probe_command_write_mode_and_payload() {
        let request = crate::FalconRequestEncoder::encode(crate::FalconProbe::BatteryInfo);

        assert_eq!(request.probe, crate::FalconProbe::BatteryInfo);
        assert_eq!(request.command, CommandKind::RequestBatteryInfo);
        assert_eq!(request.mode, WriteMode::WithResponse);
        assert_eq!(request.payload.as_slice(), b"falcon:battery");
    }

    #[test]
    fn aero_encoder_rejects_unsupported_command_family() {
        assert_eq!(
            crate::AeroRequestEncoder::encode_command(CommandKind::RequestSettings),
            None
        );
    }

    #[test]
    fn falcon_encoder_rejects_unsupported_diagnostics_probe() {
        assert_eq!(
            crate::FalconRequestEncoder::encode_command(CommandKind::RequestDiagnostics),
            None
        );
    }

    #[test]
    fn request_fixture_preserves_metadata_and_bounded_payload() {
        let fixture = crate::RequestFixture::new(
            crate::DeviceFamily::NosfetAero,
            crate::ProtocolProbe::Aero(crate::AeroProbe::Identity),
            WriteMode::WithResponse,
            b"\xdc\x5a\x5c",
            crate::FixtureChannels {
                service: Some(crate::AERO_WRITE_CHANNEL),
                characteristic: Some(crate::AERO_WRITE_CHANNEL),
            },
            crate::FixtureProvenance::BluetoothCapture,
            crate::HardwareVerification::VerifiedOnBluetooth,
        )
        .expect("fixture is valid");

        assert_eq!(fixture.family, crate::DeviceFamily::NosfetAero);
        assert_eq!(fixture.command, CommandKind::RequestIdentity);
        assert_eq!(fixture.bytes.as_slice(), b"\xdc\x5a\x5c");
        assert_eq!(
            fixture.provenance,
            crate::FixtureProvenance::BluetoothCapture
        );
        assert_eq!(
            fixture.hardware_verification,
            crate::HardwareVerification::VerifiedOnBluetooth
        );
    }

    #[test]
    fn request_fixture_rejects_oversized_request_bytes() {
        let bytes = vec![0; cutout_core::MAX_TRANSPORT_WRITE_LEN + 1];

        assert_eq!(
            crate::RequestFixture::new(
                crate::DeviceFamily::NosfetAero,
                crate::ProtocolProbe::Aero(crate::AeroProbe::Telemetry),
                WriteMode::WithResponse,
                &bytes,
                crate::FixtureChannels::default(),
                crate::FixtureProvenance::VendorDocumentation,
                crate::HardwareVerification::Unverified,
            ),
            Err(crate::RequestFixtureError::PayloadTooLong(
                cutout_core::WritePayloadTooLong {
                    len: cutout_core::MAX_TRANSPORT_WRITE_LEN + 1,
                    max: cutout_core::MAX_TRANSPORT_WRITE_LEN,
                }
            ))
        );
    }

    #[test]
    fn request_fixture_rejects_probe_from_wrong_family() {
        assert_eq!(
            crate::RequestFixture::new(
                crate::DeviceFamily::BegodeFalcon,
                crate::ProtocolProbe::Aero(crate::AeroProbe::Diagnostics),
                WriteMode::WithResponse,
                b"probe",
                crate::FixtureChannels::default(),
                crate::FixtureProvenance::AppTrace,
                crate::HardwareVerification::Unverified,
            ),
            Err(crate::RequestFixtureError::FamilyMismatch {
                family: crate::DeviceFamily::BegodeFalcon,
                probe_family: crate::DeviceFamily::NosfetAero,
            })
        );
    }

    #[test]
    fn protocol_probe_rejects_unsupported_family_command() {
        assert_eq!(
            crate::ProtocolProbe::from_family_command(
                crate::DeviceFamily::BegodeFalcon,
                CommandKind::RequestDiagnostics,
            ),
            Err(crate::RequestFixtureError::UnsupportedCommand {
                family: crate::DeviceFamily::BegodeFalcon,
                command: CommandKind::RequestDiagnostics,
            })
        );
    }

    #[test]
    fn protocol_family_classifier_matches_known_notification_magic() {
        assert_eq!(
            crate::ProtocolFamilyClassifier::classify(b"\xdc\x5a\x5c\x20"),
            crate::ProtocolFamilyClassification::Known(crate::DeviceFamily::NosfetAero)
        );
        assert_eq!(
            crate::ProtocolFamilyClassifier::classify(b"\x55\xaa\x19\xc1"),
            crate::ProtocolFamilyClassification::Known(crate::DeviceFamily::BegodeFalcon)
        );
    }

    #[test]
    fn protocol_family_classifier_distinguishes_pending_from_unknown() {
        assert_eq!(
            crate::ProtocolFamilyClassifier::classify(b""),
            crate::ProtocolFamilyClassification::Pending
        );
        assert_eq!(
            crate::ProtocolFamilyClassifier::classify(b"\xdc\x5a"),
            crate::ProtocolFamilyClassification::Pending
        );
        assert_eq!(
            crate::ProtocolFamilyClassifier::classify(b"\x55"),
            crate::ProtocolFamilyClassification::Pending
        );
        assert_eq!(
            crate::ProtocolFamilyClassifier::classify(b"\x00\x01\x02"),
            crate::ProtocolFamilyClassification::Unknown
        );
    }

    fn feed_chunk(
        reassembler: &mut crate::VeteranFrameReassembler,
        bytes: &[u8],
    ) -> Vec<crate::VeteranFrame> {
        feed_chunk_result(reassembler, bytes).expect("chunk reassembles without protocol error")
    }

    fn feed_chunk_result(
        reassembler: &mut crate::VeteranFrameReassembler,
        bytes: &[u8],
    ) -> Result<Vec<crate::VeteranFrame>, crate::VeteranReassemblyError> {
        let mut frames = Vec::new();
        for byte in bytes {
            if let Some(frame) = reassembler.feed_byte(*byte)? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }

    fn long_veteran_frame() -> Vec<u8> {
        let mut frame = vec![0xdc, 0x5a, 0x5c, 39];
        frame.extend(0_u8..35);
        let crc = crc32fast::hash(&frame);
        frame.extend(crc.to_be_bytes());
        frame
    }

    fn notification_fixture_chunks() -> Vec<Vec<u8>> {
        include_str!("../fixtures/nosfet-aero/nf2557-2026-06-21-notifications.hex")
            .lines()
            .filter_map(hex_fixture_line)
            .collect()
    }

    fn hex_fixture_line(line: &str) -> Option<Vec<u8>> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        Some(hex_to_bytes(trimmed))
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut nibbles = hex.bytes();
        while let Some(high) = nibbles.next() {
            let low = nibbles.next().expect("fixture hex has even length");
            bytes.push((hex_nibble(high) << 4) | hex_nibble(low));
        }
        bytes
    }

    fn hex_nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("fixture contains non-hex byte"),
        }
    }

    #[test]
    fn veteran_reassembler_reassembles_fragmented_short_frame() {
        let mut reassembler = crate::VeteranFrameReassembler::default();
        let mut frames = Vec::new();

        frames.extend(feed_chunk(&mut reassembler, b"\xdc\x5a\x5c\x04\x01"));
        frames.extend(feed_chunk(&mut reassembler, b"\x02\x03\x04"));

        assert_eq!(
            frames,
            vec![
                crate::VeteranFrame::try_from_slice(b"\xdc\x5a\x5c\x04\x01\x02\x03\x04")
                    .expect("fixture frame fits")
            ]
        );
    }

    #[test]
    fn veteran_reassembler_resyncs_before_magic() {
        let mut reassembler = crate::VeteranFrameReassembler::default();

        let frames = feed_chunk(
            &mut reassembler,
            b"\x00\xff\xdc\x5a\x00\xdc\x5a\x5c\x04\x01\x02\x03\x04",
        );

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_slice(), b"\xdc\x5a\x5c\x04\x01\x02\x03\x04");
    }

    #[test]
    fn veteran_reassembler_returns_multiple_frames_from_one_stream() {
        let mut reassembler = crate::VeteranFrameReassembler::default();

        let frames = feed_chunk(
            &mut reassembler,
            b"\xdc\x5a\x5c\x01\xaa\xdc\x5a\x5c\x02\xbb\xcc",
        );

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].as_slice(), b"\xdc\x5a\x5c\x01\xaa");
        assert_eq!(frames[1].as_slice(), b"\xdc\x5a\x5c\x02\xbb\xcc");
    }

    #[test]
    fn veteran_reassembler_rejects_long_frame_with_bad_crc() {
        let mut reassembler = crate::VeteranFrameReassembler::default();
        let mut frame = long_veteran_frame();
        let last = frame.last_mut().expect("fixture has a CRC trailer");
        *last ^= 0xff;

        let error = feed_chunk_result(&mut reassembler, &frame)
            .expect_err("bad CRC should reject the long frame");

        assert_eq!(error, crate::VeteranReassemblyError::CrcMismatch);
    }

    #[test]
    fn veteran_reassembler_accepts_long_frame_with_valid_crc() {
        let mut reassembler = crate::VeteranFrameReassembler::default();
        let frame = long_veteran_frame();

        let frames = feed_chunk_result(&mut reassembler, &frame).expect("valid CRC is accepted");

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_slice(), frame.as_slice());
    }

    #[test]
    fn veteran_reassembler_consumes_live_aero_fixture_chunks() {
        let mut reassembler = crate::VeteranFrameReassembler::default();
        let mut frames = Vec::new();

        for chunk in notification_fixture_chunks() {
            frames.extend(feed_chunk(&mut reassembler, &chunk));
        }

        assert_eq!(frames.len(), 4);
        assert_eq!(
            frames[0].as_slice().get(..4),
            Some(&[0xdc, 0x5a, 0x5c, 0x53][..])
        );
        assert_eq!(
            frames[1].as_slice().get(..4),
            Some(&[0xdc, 0x5a, 0x5c, 0x5f][..])
        );
        assert_eq!(
            frames[2].as_slice().get(..4),
            Some(&[0xdc, 0x5a, 0x5c, 0x49][..])
        );
    }

    #[test]
    fn veteran_telemetry_decodes_first_live_aero_fixture_frame() {
        let mut reassembler = crate::VeteranFrameReassembler::default();
        let mut frames = Vec::new();

        for chunk in notification_fixture_chunks() {
            frames.extend(feed_chunk(&mut reassembler, &chunk));
        }

        let telemetry =
            crate::VeteranTelemetry::decode(&frames[0]).expect("live fixture frame decodes");

        assert_eq!(telemetry.model, Some(crate::VeteranModel::NosfetAero));
        assert_eq!(telemetry.firmware.model_id, 43);
        assert_eq!(telemetry.firmware.raw_version, 43_254);
        assert_eq!(telemetry.voltage_mv, 108_760);
        assert_eq!(telemetry.speed_deci_kmh, 0);
        assert_eq!(telemetry.total_distance_m, 1_551_169);
        assert_eq!(telemetry.phase_current_deci_a, 0);
        assert_eq!(telemetry.mosfet_temperature_mc, 33_270);
        assert_eq!(telemetry.speed_alert_deci_kmh, 550);
        assert_eq!(telemetry.speed_tiltback_deci_kmh, 540);
    }

    #[test]
    fn veteran_telemetry_maps_live_aero_fixture_to_core_delta() {
        let mut reassembler = crate::VeteranFrameReassembler::default();
        let mut frames = Vec::new();

        for chunk in notification_fixture_chunks() {
            frames.extend(feed_chunk(&mut reassembler, &chunk));
        }

        let delta = crate::VeteranTelemetry::decode(&frames[0])
            .expect("live fixture frame decodes")
            .to_delta(42);

        assert_eq!(delta.at_ms, 42);
        assert_eq!(delta.speed_mm_s, Some(Measured::reported(0)));
        assert_eq!(delta.voltage_mv, Some(Measured::reported(108_760)));
        assert_eq!(delta.motor_current_ma, Some(Measured::reported(0)));
        assert_eq!(
            delta.controller_temperature_mc,
            Some(Measured::reported(33_270))
        );
        assert_eq!(delta.distance_mm, Some(Measured::reported(1_551_169_000)));
    }

    #[test]
    fn aero_session_emits_telemetry_from_live_fixture_notifications() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        for chunk in notification_fixture_chunks() {
            session.handle(
                SessionInput::Notification {
                    channel: crate::AERO_WRITE_CHANNEL,
                    bytes: chunk.as_slice(),
                    monotonic_ms: 42,
                },
                &mut output,
            );
        }

        let telemetry: Vec<_> = output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::Event(DeviceEvent::Telemetry(delta)) => Some(*delta),
                _ => None,
            })
            .collect();

        assert_eq!(telemetry.len(), 4);
        assert_eq!(telemetry[0].at_ms, 42);
        assert_eq!(telemetry[0].voltage_mv, Some(Measured::reported(108_760)));
        assert_eq!(telemetry[0].speed_mm_s, Some(Measured::reported(0)));
        assert_eq!(telemetry[0].motor_current_ma, Some(Measured::reported(0)));
        assert_eq!(
            telemetry[0].controller_temperature_mc,
            Some(Measured::reported(33_270))
        );
        assert_eq!(
            telemetry[0].distance_mm,
            Some(Measured::reported(1_551_169_000))
        );
    }

    #[test]
    fn aero_session_reports_bad_checksum_diagnostics() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();
        let mut frame = long_veteran_frame();
        let last = frame.last_mut().expect("fixture has a CRC trailer");
        *last ^= 0xff;

        session.handle(
            SessionInput::Notification {
                channel: crate::AERO_WRITE_CHANNEL,
                bytes: frame.as_slice(),
                monotonic_ms: 42,
            },
            &mut output,
        );

        let diagnostics: Vec<_> = output
            .iter()
            .filter_map(|item| match item {
                SessionOutput::Event(DeviceEvent::Diagnostics(diagnostics)) => Some(*diagnostics),
                _ => None,
            })
            .collect();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].bad_checksums, 1);
    }

    #[test]
    fn aero_session_does_not_poll_while_disconnected() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(SessionInput::Tick { monotonic_ms: 10 }, &mut output);

        assert!(transport_writes(&output).is_empty());
    }

    #[test]
    fn aero_session_emits_polls_in_scheduler_order_after_link_up() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert_eq!(
            transport_writes(&output),
            vec![
                (crate::AERO_WRITE_CHANNEL, b"aero:identity".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:firmware".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:telemetry".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:battery".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:diagnostics".to_vec()),
            ]
        );
    }

    #[test]
    fn falcon_session_emits_polls_in_scheduler_order_after_link_up() {
        let mut session = crate::FalconReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(20),
            }),
            &mut output,
        );
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert_eq!(
            transport_writes(&output),
            vec![
                (crate::FALCON_WRITE_CHANNEL, b"falcon:identity".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:firmware".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:telemetry".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:battery".to_vec()),
            ]
        );
    }

    #[test]
    fn aero_session_clears_queued_polls_on_link_down() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(SessionInput::LinkDown, &mut output);
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert!(transport_writes(&output).is_empty());
    }

    fn transport_writes(output: &[SessionOutput]) -> Vec<(cutout_core::GattChannel, Vec<u8>)> {
        output
            .iter()
            .filter_map(|item| {
                let SessionOutput::Transport(TransportAction::Write {
                    channel,
                    bytes,
                    mode,
                }) = item
                else {
                    return None;
                };
                assert_eq!(*mode, WriteMode::WithResponse);
                Some((*channel, bytes.as_slice().to_vec()))
            })
            .collect()
    }
}
