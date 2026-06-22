use arrayvec::ArrayVec;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(test)]
use crate::GattRoles;
#[cfg(feature = "serde")]
use crate::VerificationStatus;
use crate::{
    GattChannel, GattFingerprint, MonotonicMillis, ProtocolFamily, VerifiedValue, WriteMode,
};

/// PEVCAP file format magic bytes.
pub const PEVCAP_MAGIC: [u8; 8] = *b"PEVCAP\0\0";

/// Current major PEVCAP format version.
pub const PEVCAP_VERSION_MAJOR: u16 = 1;

/// Current minor PEVCAP format version.
pub const PEVCAP_VERSION_MINOR: u16 = 0;

/// Maximum captured advertisement service UUIDs stored in the PEVCAP header.
pub const PEVCAP_MAX_ADVERTISED_SERVICES: usize = 8;

/// Maximum GATT fingerprints stored in the PEVCAP header.
pub const PEVCAP_MAX_GATT_FINGERPRINTS: usize = 8;

/// Maximum annotations stored in the PEVCAP header.
pub const PEVCAP_MAX_ANNOTATIONS: usize = 8;

/// Current PEVCAP format version.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PevcapFormatVersion {
    /// Major version number.
    pub major: u16,

    /// Minor version number.
    pub minor: u16,
}

impl PevcapFormatVersion {
    /// Returns the format version currently produced by Cutout.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            major: PEVCAP_VERSION_MAJOR,
            minor: PEVCAP_VERSION_MINOR,
        }
    }
}

/// Direction of a captured transport record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapDirection {
    /// Data flowed from the peripheral to the host.
    Inbound,

    /// Data flowed from the host to the peripheral.
    Outbound,
}

/// Resolved model and firmware metadata embedded in the capture header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapResolvedIdentity {
    /// Resolved protocol family, when known.
    pub protocol_family: Option<ProtocolFamily>,

    /// Resolved model name, when known.
    pub model: Option<VerifiedValue<String>>,

    /// Resolved firmware string, when known.
    pub firmware: Option<VerifiedValue<String>>,
}

/// PEVCAP capture header metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapHeader {
    /// Wall-clock start time in Unix milliseconds.
    pub wall_clock_start_unix_ms: u64,

    /// Platform identifier recorded by the capture producer.
    pub platform_id: String,

    /// Maximum transport write length observed at capture time.
    pub write_limit: Option<u16>,

    /// Advertised service UUIDs observed during discovery.
    pub advertised_services: ArrayVec<GattChannel, PEVCAP_MAX_ADVERTISED_SERVICES>,

    /// GATT fingerprints observed during discovery.
    pub gatt_fingerprints: ArrayVec<GattFingerprint, PEVCAP_MAX_GATT_FINGERPRINTS>,

    /// Resolved device identity, when known.
    pub resolved_identity: Option<PevcapResolvedIdentity>,

    /// Version of the Cutout library that produced the capture.
    pub library_version: String,

    /// Registry hash used to resolve the capture.
    pub registry_hash: [u8; 32],

    /// Human annotations attached to the capture.
    pub annotations: ArrayVec<String, PEVCAP_MAX_ANNOTATIONS>,
}

impl PevcapHeader {
    /// Creates a header after validating the bounded metadata fields.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapHeaderError::TooManyAdvertisedServices`] when the
    /// observed service list exceeds the format bound, or similarly for GATT
    /// fingerprints and annotations.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wall_clock_start_unix_ms: u64,
        platform_id: impl Into<String>,
        write_limit: Option<u16>,
        advertised_services: &[GattChannel],
        gatt_fingerprints: &[GattFingerprint],
        resolved_identity: Option<PevcapResolvedIdentity>,
        library_version: impl Into<String>,
        registry_hash: [u8; 32],
        annotations: &[&str],
    ) -> Result<Self, PevcapHeaderError> {
        Ok(Self {
            wall_clock_start_unix_ms,
            platform_id: platform_id.into(),
            write_limit,
            advertised_services: collect_bounded(
                advertised_services,
                PEVCAP_MAX_ADVERTISED_SERVICES,
                PevcapHeaderField::AdvertisedServices,
            )?,
            gatt_fingerprints: collect_bounded(
                gatt_fingerprints,
                PEVCAP_MAX_GATT_FINGERPRINTS,
                PevcapHeaderField::GattFingerprints,
            )?,
            resolved_identity,
            library_version: library_version.into(),
            registry_hash,
            annotations: collect_bounded_strings(
                annotations,
                PEVCAP_MAX_ANNOTATIONS,
                PevcapHeaderField::Annotations,
            )?,
        })
    }
}

/// PEVCAP header validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PevcapHeaderError {
    /// A bounded header list exceeded the allowed capacity.
    #[error("{field:?} has {len} items but the format only allows {max}")]
    TooManyItems {
        /// Header field that exceeded its bound.
        field: PevcapHeaderField,

        /// Number of items observed.
        len: usize,

        /// Maximum allowed items.
        max: usize,
    },
}

/// Bounded header field identifier for error reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapHeaderField {
    /// Advertised services observed during discovery.
    AdvertisedServices,

    /// GATT fingerprints observed during discovery.
    GattFingerprints,

    /// Free-form capture annotations.
    Annotations,
}

fn collect_bounded<T: Clone, const N: usize>(
    items: &[T],
    max: usize,
    field: PevcapHeaderField,
) -> Result<ArrayVec<T, N>, PevcapHeaderError> {
    if items.len() > max {
        return Err(PevcapHeaderError::TooManyItems {
            field,
            len: items.len(),
            max,
        });
    }

    let mut collected = ArrayVec::new();
    for item in items {
        let pushed = collected.try_push(item.clone());
        debug_assert!(pushed.is_ok());
    }
    Ok(collected)
}

fn collect_bounded_strings<const N: usize>(
    items: &[&str],
    max: usize,
    field: PevcapHeaderField,
) -> Result<ArrayVec<String, N>, PevcapHeaderError> {
    if items.len() > max {
        return Err(PevcapHeaderError::TooManyItems {
            field,
            len: items.len(),
            max,
        });
    }

    let mut collected = ArrayVec::new();
    for item in items {
        let pushed = collected.try_push((*item).to_owned());
        debug_assert!(pushed.is_ok());
    }
    Ok(collected)
}

/// Owned capture record for PEVCAP files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapRecord {
    /// Relative monotonic timestamp in milliseconds.
    pub monotonic_ms: MonotonicMillis,

    /// Transport direction for this record.
    pub direction: PevcapDirection,

    /// Characteristic that produced or consumed the bytes.
    pub characteristic: GattChannel,

    /// Optional service UUID associated with the record.
    pub service: Option<GattChannel>,

    /// Write mode, when the record is an outbound write.
    pub write_mode: Option<WriteMode>,

    /// Exact bytes captured for the record.
    pub bytes: Vec<u8>,
}

impl PevcapRecord {
    /// Creates an outbound write record.
    #[must_use]
    pub fn outbound_write(
        monotonic_ms: MonotonicMillis,
        characteristic: GattChannel,
        write_mode: WriteMode,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::Outbound,
            characteristic,
            service: None,
            write_mode: Some(write_mode),
            bytes,
        }
    }

    /// Creates an inbound notification record.
    #[must_use]
    pub fn inbound_notification(
        monotonic_ms: MonotonicMillis,
        characteristic: GattChannel,
        service: GattChannel,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::Inbound,
            characteristic,
            service: Some(service),
            write_mode: None,
            bytes,
        }
    }
}

/// Versioned PEVCAP capture envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapCapture {
    /// File format version.
    pub version: PevcapFormatVersion,

    /// Header metadata for the capture.
    pub header: PevcapHeader,

    /// Ordered transport records.
    pub records: Vec<PevcapRecord>,
}

impl PevcapCapture {
    /// Creates a capture envelope using the current format version.
    #[must_use]
    pub fn new(header: PevcapHeader, records: Vec<PevcapRecord>) -> Self {
        Self {
            version: PevcapFormatVersion::current(),
            header,
            records,
        }
    }

    /// Serializes this capture as line-delimited JSON for review tooling.
    ///
    /// The first line is a PEVCAP header line, followed by one transport
    /// record per line in replay order.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapJsonlError::Serialize`] when JSON serialization fails.
    #[cfg(feature = "serde")]
    pub fn to_jsonl(&self) -> Result<String, PevcapJsonlError> {
        let mut output = serde_json::to_string(&PevcapJsonlLine::Header {
            magic: PEVCAP_MAGIC,
            version: self.version,
            header: PevcapHeaderJson::from(&self.header),
        })
        .map_err(PevcapJsonlError::Serialize)?;
        output.push('\n');

        for record in &self.records {
            output.push_str(
                &serde_json::to_string(&PevcapJsonlLine::Record {
                    record: PevcapRecordJson::from(record),
                })
                .map_err(PevcapJsonlError::Serialize)?,
            );
            output.push('\n');
        }

        Ok(output)
    }

    /// Deserializes a capture from line-delimited JSON.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapJsonlError`] when the stream is malformed, missing a
    /// header, has the wrong magic/version, or violates PEVCAP header bounds.
    #[cfg(feature = "serde")]
    pub fn from_jsonl(input: &str) -> Result<Self, PevcapJsonlError> {
        let mut header = None;
        let mut version = None;
        let mut records = Vec::new();

        for (index, raw_line) in input.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            let line_number = index + 1;
            match serde_json::from_str::<PevcapJsonlLine>(line).map_err(|source| {
                PevcapJsonlError::Deserialize {
                    line: line_number,
                    source,
                }
            })? {
                PevcapJsonlLine::Header {
                    magic,
                    version: decoded_version,
                    header: decoded_header,
                } => {
                    if header.is_some() {
                        return Err(PevcapJsonlError::DuplicateHeader { line: line_number });
                    }
                    if magic != PEVCAP_MAGIC {
                        return Err(PevcapJsonlError::InvalidMagic { line: line_number });
                    }
                    if decoded_version != PevcapFormatVersion::current() {
                        return Err(PevcapJsonlError::UnsupportedVersion {
                            line: line_number,
                            version: decoded_version,
                        });
                    }
                    version = Some(decoded_version);
                    header = Some(decoded_header.try_into_header()?);
                }
                PevcapJsonlLine::Record { record } => {
                    if header.is_none() {
                        return Err(PevcapJsonlError::MissingHeader);
                    }
                    records.push(record.into_record());
                }
            }
        }

        Ok(Self {
            version: version.ok_or(PevcapJsonlError::MissingHeader)?,
            header: header.ok_or(PevcapJsonlError::MissingHeader)?,
            records,
        })
    }
}

/// JSONL PEVCAP import/export error.
#[cfg(feature = "serde")]
#[derive(Debug, Error)]
pub enum PevcapJsonlError {
    /// JSON serialization failed.
    #[error("failed to serialize PEVCAP JSONL: {0}")]
    Serialize(serde_json::Error),

    /// JSON deserialization failed for a specific line.
    #[error("failed to deserialize PEVCAP JSONL line {line}: {source}")]
    Deserialize {
        /// One-based line number.
        line: usize,

        /// Underlying JSON error.
        source: serde_json::Error,
    },

    /// The JSONL stream did not start with a capture header.
    #[error("PEVCAP JSONL is missing a header line")]
    MissingHeader,

    /// More than one header line was encountered.
    #[error("duplicate PEVCAP JSONL header at line {line}")]
    DuplicateHeader {
        /// One-based line number.
        line: usize,
    },

    /// Header magic bytes did not match PEVCAP.
    #[error("invalid PEVCAP JSONL magic at line {line}")]
    InvalidMagic {
        /// One-based line number.
        line: usize,
    },

    /// Header version is not supported by this reader.
    #[error("unsupported PEVCAP JSONL version {version:?} at line {line}")]
    UnsupportedVersion {
        /// One-based line number.
        line: usize,

        /// Decoded version.
        version: PevcapFormatVersion,
    },

    /// Header metadata violated bounded PEVCAP limits.
    #[error(transparent)]
    Header(#[from] PevcapHeaderError),
}

#[cfg(feature = "serde")]
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PevcapJsonlLine {
    Header {
        magic: [u8; 8],
        version: PevcapFormatVersion,
        header: PevcapHeaderJson,
    },
    Record {
        record: PevcapRecordJson,
    },
}

#[cfg(feature = "serde")]
#[derive(Deserialize, Serialize)]
struct PevcapHeaderJson {
    wall_clock_start_unix_ms: u64,
    platform_id: String,
    write_limit: Option<u16>,
    advertised_services: Vec<[u8; 16]>,
    gatt_fingerprints: Vec<GattFingerprintJson>,
    resolved_identity: Option<PevcapResolvedIdentityJson>,
    library_version: String,
    registry_hash: [u8; 32],
    annotations: Vec<String>,
}

#[cfg(feature = "serde")]
impl From<&PevcapHeader> for PevcapHeaderJson {
    fn from(header: &PevcapHeader) -> Self {
        Self {
            wall_clock_start_unix_ms: header.wall_clock_start_unix_ms,
            platform_id: header.platform_id.clone(),
            write_limit: header.write_limit,
            advertised_services: header
                .advertised_services
                .iter()
                .map(|channel| channel.as_bytes())
                .collect(),
            gatt_fingerprints: header
                .gatt_fingerprints
                .iter()
                .map(GattFingerprintJson::from)
                .collect(),
            resolved_identity: header
                .resolved_identity
                .as_ref()
                .map(PevcapResolvedIdentityJson::from),
            library_version: header.library_version.clone(),
            registry_hash: header.registry_hash,
            annotations: header.annotations.iter().cloned().collect(),
        }
    }
}

#[cfg(feature = "serde")]
impl PevcapHeaderJson {
    fn try_into_header(self) -> Result<PevcapHeader, PevcapHeaderError> {
        let advertised_services = self
            .advertised_services
            .into_iter()
            .map(GattChannel::from_bytes)
            .collect::<Vec<_>>();
        let gatt_fingerprints = self
            .gatt_fingerprints
            .into_iter()
            .map(GattFingerprintJson::into_fingerprint)
            .collect::<Vec<_>>();
        let annotations = self
            .annotations
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        PevcapHeader::new(
            self.wall_clock_start_unix_ms,
            self.platform_id,
            self.write_limit,
            &advertised_services,
            &gatt_fingerprints,
            self.resolved_identity
                .map(PevcapResolvedIdentityJson::into_identity),
            self.library_version,
            self.registry_hash,
            &annotations,
        )
    }
}

#[cfg(feature = "serde")]
#[derive(Deserialize, Serialize)]
struct PevcapResolvedIdentityJson {
    protocol_family: Option<ProtocolFamilyJson>,
    model: Option<VerifiedStringJson>,
    firmware: Option<VerifiedStringJson>,
}

#[cfg(feature = "serde")]
impl From<&PevcapResolvedIdentity> for PevcapResolvedIdentityJson {
    fn from(identity: &PevcapResolvedIdentity) -> Self {
        Self {
            protocol_family: identity.protocol_family.map(ProtocolFamilyJson::from),
            model: identity.model.as_ref().map(VerifiedStringJson::from),
            firmware: identity.firmware.as_ref().map(VerifiedStringJson::from),
        }
    }
}

#[cfg(feature = "serde")]
impl PevcapResolvedIdentityJson {
    fn into_identity(self) -> PevcapResolvedIdentity {
        PevcapResolvedIdentity {
            protocol_family: self.protocol_family.map(ProtocolFamilyJson::into_family),
            model: self.model.map(VerifiedStringJson::into_verified_value),
            firmware: self.firmware.map(VerifiedStringJson::into_verified_value),
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Deserialize, Serialize)]
struct VerifiedStringJson {
    value: String,
    verification: VerificationStatusJson,
}

#[cfg(feature = "serde")]
impl From<&VerifiedValue<String>> for VerifiedStringJson {
    fn from(value: &VerifiedValue<String>) -> Self {
        Self {
            value: value.value.clone(),
            verification: VerificationStatusJson::from(value.verification),
        }
    }
}

#[cfg(feature = "serde")]
impl VerifiedStringJson {
    fn into_verified_value(self) -> VerifiedValue<String> {
        VerifiedValue {
            value: self.value,
            verification: self.verification.into_status(),
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Clone, Copy, Deserialize, Serialize)]
enum ProtocolFamilyJson {
    VeteranLeaperkimNosfet,
    BegodeGotway,
    Vesc,
}

#[cfg(feature = "serde")]
impl From<ProtocolFamily> for ProtocolFamilyJson {
    fn from(family: ProtocolFamily) -> Self {
        match family {
            ProtocolFamily::VeteranLeaperkimNosfet => Self::VeteranLeaperkimNosfet,
            ProtocolFamily::BegodeGotway => Self::BegodeGotway,
            ProtocolFamily::Vesc => Self::Vesc,
        }
    }
}

#[cfg(feature = "serde")]
impl ProtocolFamilyJson {
    const fn into_family(self) -> ProtocolFamily {
        match self {
            Self::VeteranLeaperkimNosfet => ProtocolFamily::VeteranLeaperkimNosfet,
            Self::BegodeGotway => ProtocolFamily::BegodeGotway,
            Self::Vesc => ProtocolFamily::Vesc,
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Clone, Copy, Deserialize, Serialize)]
enum VerificationStatusJson {
    Unverified,
    Inferred,
    SourceVerified,
    HardwareVerified,
    SourceAndHardwareVerified,
}

#[cfg(feature = "serde")]
impl From<VerificationStatus> for VerificationStatusJson {
    fn from(status: VerificationStatus) -> Self {
        match status {
            VerificationStatus::Unverified => Self::Unverified,
            VerificationStatus::Inferred => Self::Inferred,
            VerificationStatus::SourceVerified => Self::SourceVerified,
            VerificationStatus::HardwareVerified => Self::HardwareVerified,
            VerificationStatus::SourceAndHardwareVerified => Self::SourceAndHardwareVerified,
        }
    }
}

#[cfg(feature = "serde")]
impl VerificationStatusJson {
    const fn into_status(self) -> VerificationStatus {
        match self {
            Self::Unverified => VerificationStatus::Unverified,
            Self::Inferred => VerificationStatus::Inferred,
            Self::SourceVerified => VerificationStatus::SourceVerified,
            Self::HardwareVerified => VerificationStatus::HardwareVerified,
            Self::SourceAndHardwareVerified => VerificationStatus::SourceAndHardwareVerified,
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Deserialize, Serialize)]
struct GattFingerprintJson {
    service: [u8; 16],
    characteristic: [u8; 16],
    roles: GattRolesJson,
    verification: VerificationStatusJson,
}

#[cfg(feature = "serde")]
impl From<&GattFingerprint> for GattFingerprintJson {
    fn from(fingerprint: &GattFingerprint) -> Self {
        Self {
            service: fingerprint.service.as_bytes(),
            characteristic: fingerprint.characteristic.as_bytes(),
            roles: GattRolesJson::from(fingerprint.roles),
            verification: VerificationStatusJson::from(fingerprint.verification),
        }
    }
}

#[cfg(feature = "serde")]
impl GattFingerprintJson {
    fn into_fingerprint(self) -> GattFingerprint {
        GattFingerprint {
            service: GattChannel::from_bytes(self.service),
            characteristic: GattChannel::from_bytes(self.characteristic),
            roles: self.roles.into_roles(),
            verification: self.verification.into_status(),
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Deserialize, Serialize)]
struct GattRolesJson {
    roles: Vec<GattRoleJson>,
}

#[cfg(feature = "serde")]
impl From<GattRoles> for GattRolesJson {
    fn from(roles: GattRoles) -> Self {
        let mut serialized = Vec::with_capacity(5);
        if roles.supports_read() {
            serialized.push(GattRoleJson::Read);
        }
        if roles.supports_write() {
            serialized.push(GattRoleJson::Write);
        }
        if roles.supports_write_without_response() {
            serialized.push(GattRoleJson::WriteWithoutResponse);
        }
        if roles.supports_notify() {
            serialized.push(GattRoleJson::Notify);
        }
        if roles.supports_indicate() {
            serialized.push(GattRoleJson::Indicate);
        }
        Self { roles: serialized }
    }
}

#[cfg(feature = "serde")]
impl GattRolesJson {
    fn into_roles(self) -> GattRoles {
        self.roles
            .into_iter()
            .fold(GattRoles::empty(), |roles, role| role.apply(roles))
    }
}

#[cfg(feature = "serde")]
#[derive(Clone, Copy, Deserialize, Serialize)]
enum GattRoleJson {
    Read,
    Write,
    WriteWithoutResponse,
    Notify,
    Indicate,
}

#[cfg(feature = "serde")]
impl GattRoleJson {
    const fn apply(self, roles: GattRoles) -> GattRoles {
        match self {
            Self::Read => roles.with_read(),
            Self::Write => roles.with_write(),
            Self::WriteWithoutResponse => roles.with_write_without_response(),
            Self::Notify => roles.with_notify(),
            Self::Indicate => roles.with_indicate(),
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Deserialize, Serialize)]
struct PevcapRecordJson {
    monotonic_ms: MonotonicMillis,
    direction: PevcapDirectionJson,
    characteristic: [u8; 16],
    service: Option<[u8; 16]>,
    write_mode: Option<WriteModeJson>,
    bytes: Vec<u8>,
}

#[cfg(feature = "serde")]
impl From<&PevcapRecord> for PevcapRecordJson {
    fn from(record: &PevcapRecord) -> Self {
        Self {
            monotonic_ms: record.monotonic_ms,
            direction: PevcapDirectionJson::from(record.direction),
            characteristic: record.characteristic.as_bytes(),
            service: record.service.map(GattChannel::as_bytes),
            write_mode: record.write_mode.map(WriteModeJson::from),
            bytes: record.bytes.clone(),
        }
    }
}

#[cfg(feature = "serde")]
impl PevcapRecordJson {
    fn into_record(self) -> PevcapRecord {
        PevcapRecord {
            monotonic_ms: self.monotonic_ms,
            direction: self.direction.into_direction(),
            characteristic: GattChannel::from_bytes(self.characteristic),
            service: self.service.map(GattChannel::from_bytes),
            write_mode: self.write_mode.map(WriteModeJson::into_mode),
            bytes: self.bytes,
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Clone, Copy, Deserialize, Serialize)]
enum PevcapDirectionJson {
    Inbound,
    Outbound,
}

#[cfg(feature = "serde")]
impl From<PevcapDirection> for PevcapDirectionJson {
    fn from(direction: PevcapDirection) -> Self {
        match direction {
            PevcapDirection::Inbound => Self::Inbound,
            PevcapDirection::Outbound => Self::Outbound,
        }
    }
}

#[cfg(feature = "serde")]
impl PevcapDirectionJson {
    const fn into_direction(self) -> PevcapDirection {
        match self {
            Self::Inbound => PevcapDirection::Inbound,
            Self::Outbound => PevcapDirection::Outbound,
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Clone, Copy, Deserialize, Serialize)]
enum WriteModeJson {
    WithResponse,
    WithoutResponse,
}

#[cfg(feature = "serde")]
impl From<WriteMode> for WriteModeJson {
    fn from(mode: WriteMode) -> Self {
        match mode {
            WriteMode::WithResponse => Self::WithResponse,
            WriteMode::WithoutResponse => Self::WithoutResponse,
        }
    }
}

#[cfg(feature = "serde")]
impl WriteModeJson {
    const fn into_mode(self) -> WriteMode {
        match self {
            Self::WithResponse => WriteMode::WithResponse,
            Self::WithoutResponse => WriteMode::WithoutResponse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerificationStatus;

    #[test]
    fn pevcap_current_version_and_magic_are_stable() {
        assert_eq!(PEVCAP_MAGIC, *b"PEVCAP\0\0");
        assert_eq!(
            PevcapFormatVersion::current(),
            PevcapFormatVersion { major: 1, minor: 0 }
        );
    }

    #[test]
    fn pevcap_header_preserves_metadata_and_bounded_evidence() {
        let service = GattChannel::from_bytes([0x11; 16]);
        let fingerprint = GattFingerprint {
            service,
            characteristic: GattChannel::from_bytes([0x22; 16]),
            roles: GattRoles::empty().with_read().with_write().with_notify(),
            verification: VerificationStatus::HardwareVerified,
        };
        let header = PevcapHeader::new(
            1_725_000_000_000,
            "darwin",
            Some(185),
            &[service],
            &[fingerprint],
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
                model: Some(VerifiedValue {
                    value: "NOSFET Aero".to_owned(),
                    verification: VerificationStatus::HardwareVerified,
                }),
                firmware: Some(VerifiedValue {
                    value: "3.8.12".to_owned(),
                    verification: VerificationStatus::SourceAndHardwareVerified,
                }),
            }),
            "0.1.0",
            [0xAB; 32],
            &["capture", "demo"],
        )
        .expect("header should validate");

        assert_eq!(header.wall_clock_start_unix_ms, 1_725_000_000_000);
        assert_eq!(header.platform_id, "darwin");
        assert_eq!(header.write_limit, Some(185));
        assert_eq!(header.advertised_services.as_slice(), &[service]);
        assert_eq!(header.gatt_fingerprints.as_slice(), &[fingerprint]);
        assert_eq!(
            header
                .resolved_identity
                .as_ref()
                .map(|resolved| resolved.model.as_ref().map(|model| model.value.as_str())),
            Some(Some("NOSFET Aero"))
        );
        assert_eq!(
            header
                .resolved_identity
                .as_ref()
                .and_then(|resolved| resolved.model.as_ref().map(|model| model.verification)),
            Some(VerificationStatus::HardwareVerified)
        );
        assert_eq!(
            header
                .resolved_identity
                .as_ref()
                .and_then(|resolved| resolved
                    .firmware
                    .as_ref()
                    .map(|firmware| firmware.verification)),
            Some(VerificationStatus::SourceAndHardwareVerified)
        );
        assert_eq!(header.library_version, "0.1.0");
        assert_eq!(header.registry_hash, [0xAB; 32]);
        assert_eq!(
            header.annotations.as_slice(),
            &["capture".to_owned(), "demo".to_owned()]
        );
    }

    #[test]
    fn pevcap_header_rejects_oversized_annotations() {
        let annotations = ["note"; PEVCAP_MAX_ANNOTATIONS + 1];
        let error = PevcapHeader::new(
            0,
            "linux",
            None,
            &[],
            &[],
            None,
            "0.1.0",
            [0x00; 32],
            &annotations,
        )
        .expect_err("header should reject oversized annotations");

        assert_eq!(
            error,
            PevcapHeaderError::TooManyItems {
                field: PevcapHeaderField::Annotations,
                len: PEVCAP_MAX_ANNOTATIONS + 1,
                max: PEVCAP_MAX_ANNOTATIONS,
            }
        );
    }

    #[test]
    fn pevcap_records_preserve_direction_modes_and_bytes() {
        let characteristic = GattChannel::from_bytes([0x33; 16]);
        let service = GattChannel::from_bytes([0x44; 16]);
        let write = PevcapRecord::outbound_write(
            7,
            characteristic,
            WriteMode::WithoutResponse,
            vec![0x01, 0x23, 0xab],
        );
        let notification = PevcapRecord::inbound_notification(
            9,
            characteristic,
            service,
            vec![0xde, 0xad, 0xbe, 0xef],
        );

        assert_eq!(write.direction, PevcapDirection::Outbound);
        assert_eq!(write.characteristic, characteristic);
        assert_eq!(write.service, None);
        assert_eq!(write.write_mode, Some(WriteMode::WithoutResponse));
        assert_eq!(write.bytes.as_slice(), &[0x01, 0x23, 0xab]);
        assert_eq!(notification.direction, PevcapDirection::Inbound);
        assert_eq!(notification.characteristic, characteristic);
        assert_eq!(notification.service, Some(service));
        assert_eq!(notification.write_mode, None);
        assert_eq!(notification.bytes.as_slice(), &[0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn pevcap_capture_wraps_header_and_records() {
        let header = PevcapHeader::new(
            1,
            "darwin",
            Some(185),
            &[],
            &[],
            None,
            "0.1.0",
            [0x11; 32],
            &[],
        )
        .expect("header should validate");
        let records = vec![PevcapRecord::outbound_write(
            1,
            GattChannel::from_bytes([0x55; 16]),
            WriteMode::WithResponse,
            vec![0x10],
        )];

        let capture = PevcapCapture::new(header.clone(), records.clone());

        assert_eq!(capture.version, PevcapFormatVersion::current());
        assert_eq!(capture.header, header);
        assert_eq!(capture.records, records);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_jsonl_round_trips_header_and_ordered_records() {
        let service = GattChannel::from_bytes([0xFE; 16]);
        let characteristic = GattChannel::from_bytes([0xE1; 16]);
        let header = PevcapHeader::new(
            1_725_000_123_456,
            "darwin",
            Some(182),
            &[service],
            &[GattFingerprint {
                service,
                characteristic,
                roles: GattRoles::empty()
                    .with_write_without_response()
                    .with_notify(),
                verification: VerificationStatus::HardwareVerified,
            }],
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::BegodeGotway),
                model: Some(VerifiedValue {
                    value: "Begode Falcon".to_owned(),
                    verification: VerificationStatus::Inferred,
                }),
                firmware: None,
            }),
            "0.1.0",
            [0x42; 32],
            &["identity_confidence=Model", "battery=84v"],
        )
        .expect("header should validate");
        let capture = PevcapCapture::new(
            header,
            vec![
                PevcapRecord::outbound_write(
                    7,
                    characteristic,
                    WriteMode::WithoutResponse,
                    b"N".to_vec(),
                ),
                PevcapRecord::inbound_notification(
                    9,
                    characteristic,
                    service,
                    b"NAME=Falcon".to_vec(),
                ),
            ],
        );

        let jsonl = capture.to_jsonl().expect("capture serializes");
        let decoded = PevcapCapture::from_jsonl(&jsonl).expect("capture deserializes");

        assert_eq!(decoded, capture);
        assert_eq!(jsonl.lines().count(), 3);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_jsonl_requires_first_line_header() {
        let err = PevcapCapture::from_jsonl(
            r#"{"kind":"record","record":{"monotonic_ms":1,"direction":"Inbound","characteristic":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"service":null,"write_mode":null,"bytes":[]}}"#,
        )
        .expect_err("record before header is invalid");

        assert!(matches!(err, PevcapJsonlError::MissingHeader));
    }
}
