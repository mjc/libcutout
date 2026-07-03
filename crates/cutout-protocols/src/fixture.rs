use cutout_core::{CommandKind, GattChannel, WriteMode, WritePayload, WritePayloadTooLong};
use thiserror::Error;

use crate::{AeroProbe, DeviceFamily, FalconProbe, ProtocolProbe};

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

/// Loads request fixtures from deterministic checked-in fixture text.
///
/// The format is one fixture per non-empty, non-comment line. Each record uses
/// whitespace-separated `key=value` fields:
///
/// `family`, `probe`, `command`, `mode`, `bytes`, `provenance`, and
/// `verification` are required. `service` and `characteristic` are optional
/// 32-hex-byte GATT channels.
///
/// # Errors
///
/// Returns [`RequestFixtureLoadError`] when any record is malformed or violates
/// the validated [`RequestFixture`] schema.
pub fn load_request_fixtures(input: &str) -> Result<Vec<RequestFixture>, RequestFixtureLoadError> {
    let mut fixtures = Vec::new();
    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        fixtures.push(parse_request_fixture_line(line_number, line)?);
    }
    Ok(fixtures)
}

fn parse_request_fixture_line(
    line_number: usize,
    line: &str,
) -> Result<RequestFixture, RequestFixtureLoadError> {
    let mut family = None;
    let mut probe = None;
    let mut command = None;
    let mut mode = None;
    let mut bytes = None;
    let mut service = None;
    let mut characteristic = None;
    let mut provenance = None;
    let mut verification = None;

    for field in line.split_whitespace() {
        let Some((key, value)) = field.split_once('=') else {
            return Err(RequestFixtureLoadError::MalformedField { line: line_number });
        };
        match key {
            "family" => family = Some(parse_family(line_number, value)?),
            "probe" => probe = Some(parse_probe(line_number, value)?),
            "command" => command = Some(parse_command(line_number, value)?),
            "mode" => mode = Some(parse_mode(line_number, value)?),
            "bytes" => bytes = Some(parse_hex_bytes(line_number, value)?),
            "service" => service = Some(parse_channel(line_number, value)?),
            "characteristic" => characteristic = Some(parse_channel(line_number, value)?),
            "provenance" => provenance = Some(parse_provenance(line_number, value)?),
            "verification" => verification = Some(parse_verification(line_number, value)?),
            _ => return Err(RequestFixtureLoadError::UnknownField { line: line_number }),
        }
    }

    let family = family.ok_or(RequestFixtureLoadError::MissingField {
        line: line_number,
        field: "family",
    })?;
    let probe = probe.ok_or(RequestFixtureLoadError::MissingField {
        line: line_number,
        field: "probe",
    })?;
    let command = command.ok_or(RequestFixtureLoadError::MissingField {
        line: line_number,
        field: "command",
    })?;
    if probe.command_kind() != command {
        return Err(RequestFixtureLoadError::CommandMismatch {
            line: line_number,
            command,
            probe_command: probe.command_kind(),
        });
    }
    let mode = mode.ok_or(RequestFixtureLoadError::MissingField {
        line: line_number,
        field: "mode",
    })?;
    let bytes = bytes.ok_or(RequestFixtureLoadError::MissingField {
        line: line_number,
        field: "bytes",
    })?;
    let provenance = provenance.ok_or(RequestFixtureLoadError::MissingField {
        line: line_number,
        field: "provenance",
    })?;
    let verification = verification.ok_or(RequestFixtureLoadError::MissingField {
        line: line_number,
        field: "verification",
    })?;

    RequestFixture::new(
        family,
        probe,
        mode,
        &bytes,
        FixtureChannels {
            service,
            characteristic,
        },
        provenance,
        verification,
    )
    .map_err(|source| RequestFixtureLoadError::Fixture {
        line: line_number,
        source,
    })
}

/// Request fixture loading error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RequestFixtureLoadError {
    /// A required key was absent.
    #[error("line {line}: missing required field {field}")]
    MissingField {
        /// One-based line number.
        line: usize,

        /// Missing field name.
        field: &'static str,
    },

    /// A field did not use `key=value` syntax.
    #[error("line {line}: malformed fixture field")]
    MalformedField {
        /// One-based line number.
        line: usize,
    },

    /// A field key is not part of the fixture format.
    #[error("line {line}: unknown fixture field")]
    UnknownField {
        /// One-based line number.
        line: usize,
    },

    /// A field value is not accepted for its key.
    #[error("line {line}: invalid value for {field}")]
    InvalidValue {
        /// One-based line number.
        line: usize,

        /// Field name.
        field: &'static str,
    },

    /// A fixture command did not match its probe.
    #[error("line {line}: command {command:?} does not match probe command {probe_command:?}")]
    CommandMismatch {
        /// One-based line number.
        line: usize,

        /// Parsed command.
        command: CommandKind,

        /// Command implied by the probe.
        probe_command: CommandKind,
    },

    /// Parsed fields failed schema validation.
    #[error("line {line}: {source}")]
    Fixture {
        /// One-based line number.
        line: usize,

        /// Schema validation error.
        source: RequestFixtureError,
    },
}

fn parse_family(line: usize, value: &str) -> Result<DeviceFamily, RequestFixtureLoadError> {
    match value {
        "nosfet-aero" => Ok(DeviceFamily::NosfetAero),
        "begode-falcon" => Ok(DeviceFamily::BegodeFalcon),
        _ => Err(RequestFixtureLoadError::InvalidValue {
            line,
            field: "family",
        }),
    }
}

fn parse_probe(line: usize, value: &str) -> Result<ProtocolProbe, RequestFixtureLoadError> {
    match value {
        "aero.identity" => Ok(ProtocolProbe::Aero(AeroProbe::Identity)),
        "aero.firmware-info" => Ok(ProtocolProbe::Aero(AeroProbe::FirmwareInfo)),
        "aero.telemetry" => Ok(ProtocolProbe::Aero(AeroProbe::Telemetry)),
        "aero.battery-info" => Ok(ProtocolProbe::Aero(AeroProbe::BatteryInfo)),
        "aero.fault-history" => Ok(ProtocolProbe::Aero(AeroProbe::FaultHistory)),
        "falcon.identity" => Ok(ProtocolProbe::Falcon(FalconProbe::Identity)),
        "falcon.firmware-info" => Ok(ProtocolProbe::Falcon(FalconProbe::FirmwareInfo)),
        "falcon.telemetry" => Ok(ProtocolProbe::Falcon(FalconProbe::Telemetry)),
        "falcon.battery-info" => Ok(ProtocolProbe::Falcon(FalconProbe::BatteryInfo)),
        _ => Err(RequestFixtureLoadError::InvalidValue {
            line,
            field: "probe",
        }),
    }
}

fn parse_command(line: usize, value: &str) -> Result<CommandKind, RequestFixtureLoadError> {
    match value {
        "request-identity" => Ok(CommandKind::RequestIdentity),
        "request-firmware-info" => Ok(CommandKind::RequestFirmwareInfo),
        "request-telemetry" => Ok(CommandKind::RequestTelemetry),
        "request-battery-info" => Ok(CommandKind::RequestBatteryInfo),
        "request-fault-history" => Ok(CommandKind::RequestFaultHistory),
        "request-diagnostics" => Ok(CommandKind::RequestDiagnostics),
        _ => Err(RequestFixtureLoadError::InvalidValue {
            line,
            field: "command",
        }),
    }
}

fn parse_mode(line: usize, value: &str) -> Result<WriteMode, RequestFixtureLoadError> {
    match value {
        "with-response" => Ok(WriteMode::WithResponse),
        "without-response" => Ok(WriteMode::WithoutResponse),
        _ => Err(RequestFixtureLoadError::InvalidValue {
            line,
            field: "mode",
        }),
    }
}

fn parse_provenance(
    line: usize,
    value: &str,
) -> Result<FixtureProvenance, RequestFixtureLoadError> {
    match value {
        "bluetooth-capture" => Ok(FixtureProvenance::BluetoothCapture),
        "app-trace" => Ok(FixtureProvenance::AppTrace),
        "vendor-documentation" => Ok(FixtureProvenance::VendorDocumentation),
        _ => Err(RequestFixtureLoadError::InvalidValue {
            line,
            field: "provenance",
        }),
    }
}

fn parse_verification(
    line: usize,
    value: &str,
) -> Result<HardwareVerification, RequestFixtureLoadError> {
    match value {
        "unverified" => Ok(HardwareVerification::Unverified),
        "verified-on-bluetooth" => Ok(HardwareVerification::VerifiedOnBluetooth),
        _ => Err(RequestFixtureLoadError::InvalidValue {
            line,
            field: "verification",
        }),
    }
}

fn parse_channel(line: usize, value: &str) -> Result<GattChannel, RequestFixtureLoadError> {
    let bytes = parse_hex_array::<16>(line, "channel", value)?;
    Ok(GattChannel::from_bytes(bytes))
}

fn parse_hex_bytes(line: usize, value: &str) -> Result<Vec<u8>, RequestFixtureLoadError> {
    if value.len() % 2 != 0 {
        return Err(RequestFixtureLoadError::InvalidValue {
            line,
            field: "bytes",
        });
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks_exact(2) {
        bytes.push(parse_hex_byte(line, "bytes", chunk)?);
    }
    Ok(bytes)
}

fn parse_hex_array<const N: usize>(
    line: usize,
    field: &'static str,
    value: &str,
) -> Result<[u8; N], RequestFixtureLoadError> {
    if value.len() != N * 2 {
        return Err(RequestFixtureLoadError::InvalidValue { line, field });
    }
    let mut bytes = [0; N];
    for (slot, chunk) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *slot = parse_hex_byte(line, field, chunk)?;
    }
    Ok(bytes)
}

fn parse_hex_byte(
    line: usize,
    field: &'static str,
    byte: &[u8],
) -> Result<u8, RequestFixtureLoadError> {
    let [high, low] = byte else {
        return Err(RequestFixtureLoadError::InvalidValue { line, field });
    };
    let high =
        parse_hex_nibble(*high).ok_or(RequestFixtureLoadError::InvalidValue { line, field })?;
    let low =
        parse_hex_nibble(*low).ok_or(RequestFixtureLoadError::InvalidValue { line, field })?;
    Ok((high << 4) + low)
}

const fn parse_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AeroProbe, FalconProbe, VETERAN_DATA_CHANNEL, VETERAN_SERVICE_CHANNEL};

    #[test]
    fn request_fixture_keeps_evidence_metadata() {
        let channels = FixtureChannels {
            service: Some(VETERAN_SERVICE_CHANNEL),
            characteristic: Some(VETERAN_DATA_CHANNEL),
        };
        let fixture = RequestFixture::new(
            DeviceFamily::NosfetAero,
            ProtocolProbe::Aero(AeroProbe::FaultHistory),
            WriteMode::WithResponse,
            &[0x10, 0x20, 0x30],
            channels,
            FixtureProvenance::AppTrace,
            HardwareVerification::VerifiedOnBluetooth,
        )
        .expect("fixture should validate");

        assert_eq!(fixture.family, DeviceFamily::NosfetAero);
        assert_eq!(fixture.probe, ProtocolProbe::Aero(AeroProbe::FaultHistory));
        assert_eq!(fixture.command, CommandKind::RequestFaultHistory);
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
    fn loader_reads_checked_in_falcon_request_fixtures() {
        let fixtures = load_request_fixtures(include_str!(
            "../fixtures/requests/falcon-read-only.requests"
        ))
        .expect("checked-in fixture file loads");

        assert_eq!(fixtures.len(), 2);
        assert_eq!(fixtures[0].family, DeviceFamily::BegodeFalcon);
        assert_eq!(
            fixtures[0].probe,
            ProtocolProbe::Falcon(FalconProbe::Identity)
        );
        assert_eq!(fixtures[0].command, CommandKind::RequestIdentity);
        assert_eq!(fixtures[0].mode, WriteMode::WithoutResponse);
        assert_eq!(fixtures[0].bytes.as_slice(), b"N");
        assert_eq!(
            fixtures[0].provenance,
            FixtureProvenance::VendorDocumentation
        );
        assert_eq!(
            fixtures[0].hardware_verification,
            HardwareVerification::Unverified
        );
        assert_eq!(
            fixtures[1].probe,
            ProtocolProbe::Falcon(FalconProbe::FirmwareInfo)
        );
        assert_eq!(fixtures[1].bytes.as_slice(), b"V");
    }

    #[test]
    fn loader_rejects_family_probe_mismatch() {
        let err = load_request_fixtures(
            "family=nosfet-aero probe=falcon.identity command=request-identity mode=without-response bytes=4e provenance=vendor-documentation verification=unverified",
        )
        .expect_err("family/probe mismatch fails");

        assert_eq!(
            err,
            RequestFixtureLoadError::Fixture {
                line: 1,
                source: RequestFixtureError::FamilyMismatch {
                    family: DeviceFamily::NosfetAero,
                    probe_family: DeviceFamily::BegodeFalcon,
                },
            }
        );
    }

    #[test]
    fn loader_rejects_probe_command_mismatch() {
        let err = load_request_fixtures(
            "family=begode-falcon probe=falcon.identity command=request-firmware-info mode=without-response bytes=4e provenance=vendor-documentation verification=unverified",
        )
        .expect_err("probe/command mismatch fails");

        assert_eq!(
            err,
            RequestFixtureLoadError::CommandMismatch {
                line: 1,
                command: CommandKind::RequestFirmwareInfo,
                probe_command: CommandKind::RequestIdentity,
            }
        );
    }

    #[test]
    fn loader_rejects_malformed_hex_bytes() {
        let err = load_request_fixtures(
            "family=begode-falcon probe=falcon.identity command=request-identity mode=without-response bytes=4g provenance=vendor-documentation verification=unverified",
        )
        .expect_err("malformed bytes fail");

        assert_eq!(
            err,
            RequestFixtureLoadError::InvalidValue {
                line: 1,
                field: "bytes",
            }
        );
    }

    #[test]
    fn loader_rejects_oversized_request_bytes() {
        let oversized_hex = "00".repeat(cutout_core::MAX_TRANSPORT_WRITE_LEN + 1);
        let line = format!(
            "family=begode-falcon probe=falcon.identity command=request-identity mode=without-response bytes={oversized_hex} provenance=vendor-documentation verification=unverified"
        );

        let err = load_request_fixtures(&line).expect_err("oversized payload fails");

        assert!(matches!(
            err,
            RequestFixtureLoadError::Fixture {
                line: 1,
                source: RequestFixtureError::PayloadTooLong(_),
            }
        ));
    }

    #[test]
    fn loader_preserves_optional_channels_and_verification() {
        let fixtures = load_request_fixtures(
            "family=nosfet-aero probe=aero.fault-history command=request-fault-history mode=with-response bytes=0102 service=0000ffe000001000800000805f9b34fb characteristic=0000ffe100001000800000805f9b34fb provenance=bluetooth-capture verification=verified-on-bluetooth",
        )
        .expect("fixture with channels loads");

        assert_eq!(
            fixtures[0].channels,
            FixtureChannels {
                service: Some(VETERAN_SERVICE_CHANNEL),
                characteristic: Some(VETERAN_DATA_CHANNEL),
            }
        );
        assert_eq!(fixtures[0].provenance, FixtureProvenance::BluetoothCapture);
        assert_eq!(
            fixtures[0].hardware_verification,
            HardwareVerification::VerifiedOnBluetooth
        );
    }

    #[test]
    fn loader_accepts_all_supported_probe_spellings() {
        let input = "\
family=nosfet-aero probe=aero.identity command=request-identity mode=with-response bytes= provenance=app-trace verification=unverified
family=nosfet-aero probe=aero.firmware-info command=request-firmware-info mode=with-response bytes= provenance=app-trace verification=unverified
family=nosfet-aero probe=aero.telemetry command=request-telemetry mode=with-response bytes= provenance=app-trace verification=unverified
family=nosfet-aero probe=aero.battery-info command=request-battery-info mode=with-response bytes= provenance=app-trace verification=unverified
family=begode-falcon probe=falcon.telemetry command=request-telemetry mode=without-response bytes= provenance=app-trace verification=unverified
family=begode-falcon probe=falcon.battery-info command=request-battery-info mode=without-response bytes= provenance=app-trace verification=unverified";

        let fixtures = load_request_fixtures(input).expect("all supported spellings load");

        assert_eq!(
            fixtures
                .iter()
                .map(|fixture| fixture.probe)
                .collect::<Vec<_>>(),
            vec![
                ProtocolProbe::Aero(AeroProbe::Identity),
                ProtocolProbe::Aero(AeroProbe::FirmwareInfo),
                ProtocolProbe::Aero(AeroProbe::Telemetry),
                ProtocolProbe::Aero(AeroProbe::BatteryInfo),
                ProtocolProbe::Falcon(FalconProbe::Telemetry),
                ProtocolProbe::Falcon(FalconProbe::BatteryInfo),
            ]
        );
        assert!(
            fixtures
                .iter()
                .all(|fixture| fixture.provenance == FixtureProvenance::AppTrace)
        );
    }

    #[test]
    fn loader_accepts_uppercase_hex_bytes() {
        let fixtures = load_request_fixtures(
            "family=begode-falcon probe=falcon.identity command=request-identity mode=without-response bytes=4E provenance=vendor-documentation verification=unverified",
        )
        .expect("uppercase hex loads");

        assert_eq!(fixtures[0].bytes.as_slice(), b"N");
    }
}
