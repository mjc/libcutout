use arrayvec::ArrayVec;
use cutout_core::{CommandKind, GattChannel, WriteMode, WritePayload, WritePayloadTooLong};
use thiserror::Error;

const MAX_REQUEST_LEN: usize = 24;

/// Capture-backed FFE0 service UUID for NOSFET/Veteran-family sessions.
pub const VETERAN_SERVICE_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xff, 0xe0, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Capture-backed FFE1 data characteristic UUID for NOSFET/Veteran-family sessions.
pub const VETERAN_DATA_CHANNEL: GattChannel = GattChannel::from_bytes([
    0x00, 0x00, 0xff, 0xe1, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
]);

/// Placeholder write channel for Begode-family sessions.
pub const FALCON_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xB1; 16]);

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

    /// Bounded request bytes.
    pub payload: ArrayVec<u8, MAX_REQUEST_LEN>,

    /// GATT write mode required by this request.
    pub mode: WriteMode,
}

/// Request encoder for NOSFET Aero/Veteran-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AeroRequestEncoder;

impl AeroRequestEncoder {
    /// Encodes a supported Aero/Veteran-family probe.
    #[must_use]
    pub const fn encode(probe: AeroProbe) -> Option<EncodedRequest<AeroProbe>> {
        let _ = probe;
        None
    }

    /// Encodes a generic command if it belongs to the Aero/Veteran probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<AeroProbe>> {
        Self::encode(AeroProbe::from_command_kind(kind)?)
    }
}

/// Request encoder for source-backed Begode/Falcon-family probes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FalconRequestEncoder;

impl FalconRequestEncoder {
    /// Encodes a supported Begode/Falcon-family probe.
    #[must_use]
    pub fn encode(probe: FalconProbe) -> Option<EncodedRequest<FalconProbe>> {
        let payload = match probe {
            FalconProbe::Identity => Some(b"N".as_slice()),
            FalconProbe::FirmwareInfo => Some(b"V".as_slice()),
            FalconProbe::Telemetry | FalconProbe::BatteryInfo => None,
        }?;

        Some(EncodedRequest {
            probe,
            command: probe.command_kind(),
            payload: request_payload(payload),
            mode: WriteMode::WithoutResponse,
        })
    }

    /// Encodes a generic command if it belongs to the Begode/Falcon probe family.
    #[must_use]
    pub fn encode_command(kind: CommandKind) -> Option<EncodedRequest<FalconProbe>> {
        FalconProbe::from_command_kind(kind).and_then(Self::encode)
    }
}

/// Protocol device family used by capture-backed fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceFamily {
    /// NOSFET/Aero/Veteran family.
    NosfetAero,

    /// Begode/Falcon family.
    BegodeFalcon,
}

/// Classification of a notification stream by protocol family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolFamilyClassification {
    /// The bytes identified a known family.
    Known(DeviceFamily),

    /// The bytes were insufficient to make a decision.
    Pending,

    /// The bytes were definitely not a known family.
    Unknown,
}

/// Transport-independent stream family classifier.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProtocolFamilyClassifier;

impl ProtocolFamilyClassifier {
    /// Classifies a prefix of notification bytes by protocol family.
    #[must_use]
    pub fn classify(bytes: &[u8]) -> ProtocolFamilyClassification {
        if matches_prefix(bytes, &[0xdc, 0x5a, 0x5c]) {
            return ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero);
        }
        if matches_prefix(bytes, &[0x55, 0xaa, 0x19, 0xc1]) {
            return ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon);
        }
        if complete_or_pending(bytes, &[0xdc, 0x5a, 0x5c])
            || complete_or_pending(bytes, &[0x55, 0xaa, 0x19, 0xc1])
        {
            return ProtocolFamilyClassification::Pending;
        }
        ProtocolFamilyClassification::Unknown
    }
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

fn complete_or_pending(bytes: &[u8], prefix: &[u8]) -> bool {
    if bytes.len() >= prefix.len() {
        return false;
    }
    prefix.starts_with(bytes)
}

fn matches_prefix(bytes: &[u8], prefix: &[u8]) -> bool {
    bytes.len() >= prefix.len() && bytes.starts_with(prefix)
}

fn request_payload(bytes: &[u8]) -> ArrayVec<u8, MAX_REQUEST_LEN> {
    let mut payload = ArrayVec::new();
    for byte in bytes {
        let pushed = payload.try_push(*byte);
        debug_assert!(pushed.is_ok());
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AeroReadOnlySession, FalconReadOnlySession};
    use cutout_core::Capabilities;

    #[test]
    fn classifier_distinguishes_partial_and_complete_prefixes() {
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0xdc, 0x5a]),
            ProtocolFamilyClassification::Pending
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0xdc, 0x5a, 0x5c]),
            ProtocolFamilyClassification::Known(DeviceFamily::NosfetAero)
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0x55, 0xaa, 0x19]),
            ProtocolFamilyClassification::Pending
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0x55, 0xaa, 0x19, 0xc1]),
            ProtocolFamilyClassification::Known(DeviceFamily::BegodeFalcon)
        );
        assert_eq!(
            ProtocolFamilyClassifier::classify(&[0x01, 0x02, 0x03]),
            ProtocolFamilyClassification::Unknown
        );
    }

    #[test]
    fn protocol_probe_maps_supported_commands_and_rejects_unsupported_ones() {
        assert_eq!(
            ProtocolProbe::from_family_command(
                DeviceFamily::NosfetAero,
                CommandKind::RequestTelemetry,
            ),
            Ok(ProtocolProbe::Aero(AeroProbe::Telemetry))
        );
        assert_eq!(
            ProtocolProbe::from_family_command(
                DeviceFamily::BegodeFalcon,
                CommandKind::RequestIdentity
            ),
            Ok(ProtocolProbe::Falcon(FalconProbe::Identity))
        );
        assert!(matches!(
            ProtocolProbe::from_family_command(DeviceFamily::NosfetAero, CommandKind::SetLights),
            Err(RequestFixtureError::UnsupportedCommand {
                family: DeviceFamily::NosfetAero,
                command: CommandKind::SetLights,
            })
        ));
        assert!(matches!(
            ProtocolProbe::from_family_command(
                DeviceFamily::BegodeFalcon,
                CommandKind::RequestDiagnostics
            ),
            Err(RequestFixtureError::UnsupportedCommand {
                family: DeviceFamily::BegodeFalcon,
                command: CommandKind::RequestDiagnostics,
            })
        ));
    }

    #[test]
    fn request_fixture_keeps_evidence_metadata() {
        let channels = FixtureChannels {
            service: Some(VETERAN_SERVICE_CHANNEL),
            characteristic: Some(VETERAN_DATA_CHANNEL),
        };
        let fixture = RequestFixture::new(
            DeviceFamily::NosfetAero,
            ProtocolProbe::Aero(AeroProbe::Diagnostics),
            WriteMode::WithResponse,
            &[0x10, 0x20, 0x30],
            channels,
            FixtureProvenance::AppTrace,
            HardwareVerification::VerifiedOnBluetooth,
        )
        .expect("fixture should validate");

        assert_eq!(fixture.family, DeviceFamily::NosfetAero);
        assert_eq!(fixture.probe, ProtocolProbe::Aero(AeroProbe::Diagnostics));
        assert_eq!(fixture.command, CommandKind::RequestDiagnostics);
        assert_eq!(fixture.mode, WriteMode::WithResponse);
        assert_eq!(fixture.bytes.as_slice(), &[0x10, 0x20, 0x30]);
        assert_eq!(fixture.channels, channels);
        assert_eq!(fixture.provenance, FixtureProvenance::AppTrace);
        assert_eq!(
            fixture.hardware_verification,
            HardwareVerification::VerifiedOnBluetooth
        );
    }

    #[test]
    fn request_fixture_rejects_family_mismatch() {
        assert!(matches!(
            RequestFixture::new(
                DeviceFamily::BegodeFalcon,
                ProtocolProbe::Aero(AeroProbe::Identity),
                WriteMode::WithoutResponse,
                b"N",
                FixtureChannels::default(),
                FixtureProvenance::VendorDocumentation,
                HardwareVerification::Unverified,
            ),
            Err(RequestFixtureError::FamilyMismatch {
                family: DeviceFamily::BegodeFalcon,
                probe_family: DeviceFamily::NosfetAero,
            })
        ));
    }

    #[test]
    fn aero_and_falcon_capabilities_are_family_specific() {
        assert_eq!(
            AeroReadOnlySession::capabilities(),
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
                CommandKind::RequestDiagnostics,
            ])
        );
        assert_eq!(
            FalconReadOnlySession::capabilities(),
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
            ])
        );
    }
}
