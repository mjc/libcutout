//! Source-backed requests for the Novatek HTTP API targeted by R3 Pro support.

use std::{collections::HashSet, net::Ipv4Addr, num::NonZeroU16, str};

use arrayvec::{ArrayString, ArrayVec};
use quick_xml::{Reader, events::Event};
use thiserror::Error;

/// Maximum XML response size accepted by the small Novatek read parsers.
pub const NOVATEK_MAX_RESPONSE_BYTES: usize = 4 * 1024;

/// Maximum XML response size accepted by the Novatek media-list parser.
pub const NOVATEK_MAX_MEDIA_RESPONSE_BYTES: usize = 512 * 1024;

const NOVATEK_MAX_XML_DEPTH: usize = 32;
const NOVATEK_MAX_FIRMWARE_VERSION_BYTES: usize = 64;
const NOVATEK_MAX_RTSP_URI_BYTES: usize = 256;
const NOVATEK_MAX_COMMAND_STATUS_ENTRIES: usize = 32;
const NOVATEK_MAX_MEDIA_ENTRIES: usize = 2_048;
const NOVATEK_MAX_MEDIA_NAME_BYTES: usize = 128;
const NOVATEK_MAX_MEDIA_PATH_BYTES: usize = 256;
const NOVATEK_MAX_MEDIA_TIME_BYTES: usize = 32;
const NOVATEK_MEDIA_PATH_PREFIX: &str = r"A:\Novatek\";
/// Firmware family whose command and capability behavior is verified for the
/// R3 Pro integration.
pub const NOVATEK_VERIFIED_R3_PRO_FIRMWARE_PREFIX: &str = "R3V1";

/// Error returned when a bounded Novatek XML response is malformed.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekResponseError {
    /// The response exceeded the parser's fixed byte bound.
    #[error("Novatek response exceeds {max} bytes")]
    ResponseTooLarge {
        /// Maximum accepted response size.
        max: usize,
    },
    /// The response was not valid UTF-8.
    #[error("Novatek response is not UTF-8")]
    InvalidUtf8,
    /// The XML was incomplete, outside the protocol subset, or had ambiguous fields.
    #[error("Novatek response is not a complete XML document")]
    MalformedXml,
    /// A required XML element was absent.
    #[error("Novatek response is missing <{tag}>")]
    MissingTag {
        /// Required element name.
        tag: &'static str,
    },
    /// The camera returned a non-numeric status element.
    #[error("Novatek response has an invalid status")]
    InvalidStatus,
    /// The camera rejected the read request.
    #[error("Novatek response returned status {status}")]
    StatusFailure {
        /// Camera-reported status value.
        status: u16,
    },
    /// A tagged value exceeded its fixed storage bound.
    #[error("Novatek <{tag}> exceeds {max} bytes")]
    ValueTooLong {
        /// Element whose value was too large.
        tag: &'static str,
        /// Maximum accepted value size.
        max: usize,
    },
    /// A live-view tag did not contain an RTSP URI.
    #[error("Novatek <{tag}> is not a valid RTSP URI")]
    InvalidRtspUri {
        /// Element whose value was invalid.
        tag: &'static str,
    },
    /// The storage response did not contain `0` or `1`.
    #[error("Novatek storage response has an invalid value")]
    InvalidStorageValue,
    /// A command id was not numeric.
    #[error("Novatek response has an invalid command id")]
    InvalidCommand,
    /// A command response was returned for a different request.
    #[error("Novatek response command {actual} does not match expected command {expected}")]
    UnexpectedCommand {
        /// Command id reported by the camera.
        actual: NovatekCommandId,
        /// Command id associated with the request.
        expected: NovatekCommandId,
    },
    /// The configuration repeated a command identifier, making its evidence
    /// ambiguous.
    #[error("Novatek configuration repeats command {command_id}")]
    DuplicateCommand {
        /// Repeated command identifier.
        command_id: NovatekCommandId,
    },
    /// The response contained more repeated entries than the parser stores.
    #[error("Novatek response contains more than {max} <{tag}> entries")]
    TooManyEntries {
        /// Repeated element whose bound was exceeded.
        tag: &'static str,
        /// Maximum number of stored entries.
        max: usize,
    },
    /// The media list repeated a path, which would alias UI and download state.
    #[error("Novatek media list repeats a file path")]
    DuplicateMediaPath,
    /// A media metadata value was malformed.
    #[error("Novatek media <{tag}> is invalid")]
    InvalidMediaValue {
        /// Element whose value was invalid.
        tag: &'static str,
    },
}

/// Error returned when a Novatek local-network origin is unsafe or malformed.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekOriginError {
    /// The origin is not in the private, link-local, or loopback IPv4 ranges.
    #[error("Novatek origin is not a local IPv4 address")]
    NonLocalAddress,
    /// TCP port zero cannot be used as a camera origin.
    #[error("Novatek origin has an invalid TCP port")]
    InvalidPort,
}

/// Failure while proving that a Novatek device belongs to the verified R3V1
/// profile before a mutating request is constructed.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekProfileError {
    /// The reported firmware is outside the verified R3V1 family.
    #[error("Novatek firmware is outside the verified R3V1 profile")]
    UnsupportedFirmware,
    /// The reported firmware exceeds the bounded identity representation.
    #[error("Novatek firmware version is too long")]
    FirmwareVersionTooLong,
}

/// Failure while proving that a verified profile advertises a mutating
/// command before its target can be constructed.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekCapabilityError {
    /// The read-only configuration did not advertise the requested command.
    #[error("Novatek command {command_id} is not advertised by the profile")]
    NotAdvertised {
        /// Command identifier that was not advertised.
        command_id: NovatekCommandId,
    },
}

/// Failure while constructing a thumbnail target from retained profile proof.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekMediaThumbnailError {
    /// The read-only configuration did not advertise command `4001`.
    #[error(transparent)]
    Capability(#[from] NovatekCapabilityError),
    /// The supplied camera media path is invalid.
    #[error(transparent)]
    Path(#[from] NovatekMediaPathError),
}

/// Failure while constructing bounded command/status evidence at an adapter
/// boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekConfigurationError {
    /// The supplied status pairs exceeded the fixed configuration bound.
    #[error("Novatek configuration contains more than {max} status entries")]
    TooManyStatuses {
        /// Maximum number of retained status pairs.
        max: usize,
    },
    /// The supplied evidence repeated a command identifier.
    #[error("Novatek configuration repeats command {command_id}")]
    DuplicateCommand {
        /// Repeated command identifier.
        command_id: NovatekCommandId,
    },
    /// The supplied evidence contained the reserved zero command identifier.
    #[error("Novatek configuration contains an invalid command id")]
    InvalidCommand {
        /// Invalid command identifier.
        command_id: u16,
    },
}

/// Error returned when a camera-reported media path cannot become a safe HTTP
/// download target.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekMediaPathError {
    /// The path was not rooted at the camera's `A:\Novatek\` volume.
    #[error("Novatek media path is not camera-rooted")]
    InvalidRoot,
    /// The path contained a traversal, URL delimiter, or empty component.
    #[error("Novatek media path contains an unsafe component")]
    UnsafeComponent,
    /// The mapped HTTP target exceeded the protocol's fixed bound.
    #[error("Novatek media path exceeds {max} bytes")]
    ValueTooLong {
        /// Maximum target size.
        max: usize,
    },
}

/// Validated local IPv4 origin for Novatek HTTP control.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NovatekHttpOrigin {
    address: Ipv4Addr,
    port: NonZeroU16,
}

/// A nonzero Novatek command identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NovatekCommandId(NonZeroU16);

impl NovatekCommandId {
    /// Command identifier used by the verified recording capability proof.
    pub const RECORDING: Self = Self(NonZeroU16::new(2001).unwrap());

    /// Command identifier used by the verified still-capture capability proof.
    pub const STILL_CAPTURE: Self = Self(NonZeroU16::new(1001).unwrap());

    /// Command identifier used by the verified media-thumbnail capability.
    pub const MEDIA_THUMBNAIL: Self = Self(NonZeroU16::new(4001).unwrap());

    /// Creates a command identifier, rejecting the reserved zero value.
    #[must_use]
    pub const fn new(value: u16) -> Option<Self> {
        match NonZeroU16::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the numeric command identifier.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl std::fmt::Display for NovatekCommandId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(formatter)
    }
}

impl NovatekHttpOrigin {
    const R3_PRO_ACCESS_POINT: Self = Self {
        address: Ipv4Addr::new(192, 168, 1, 254),
        port: NonZeroU16::new(80).unwrap(),
    };

    /// Returns the captured HTTP endpoint of the `FreedConn` R3 Pro access point.
    #[must_use]
    pub const fn r3_pro_access_point() -> Self {
        Self::R3_PRO_ACCESS_POINT
    }

    /// Creates an origin only for a local IPv4 address and nonzero port.
    ///
    /// The camera's gateway is selected by the platform adapter; this type
    /// prevents arbitrary Internet or public-LAN targets from crossing the
    /// protocol boundary.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekOriginError::NonLocalAddress`] for public addresses or
    /// [`NovatekOriginError::InvalidPort`] for port zero.
    pub fn new(address: Ipv4Addr, port: u16) -> Result<Self, NovatekOriginError> {
        if !is_local_ipv4(address) {
            return Err(NovatekOriginError::NonLocalAddress);
        }
        let Some(port) = NonZeroU16::new(port) else {
            return Err(NovatekOriginError::InvalidPort);
        };
        Ok(Self { address, port })
    }

    /// Returns the validated IPv4 address.
    #[must_use]
    pub const fn address(self) -> Ipv4Addr {
        self.address
    }

    /// Returns the validated TCP port.
    #[must_use]
    pub const fn port(self) -> u16 {
        self.port.get()
    }
}

/// Firmware version reported by Novatek command `3012`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekFirmwareVersion(ArrayString<NOVATEK_MAX_FIRMWARE_VERSION_BYTES>);

impl NovatekFirmwareVersion {
    /// Returns the firmware version text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Proof that a firmware identity belongs to the verified R3V1 R3 Pro family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekR3V1Profile(NovatekFirmwareVersion);

/// Proof that verified R3V1 read-only evidence advertises recording control.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekRecordingCapability(NovatekR3V1Profile);

/// Proof that verified R3V1 read-only evidence advertises still capture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekStillCaptureCapability(NovatekR3V1Profile);

/// Proof that verified R3V1 read-only evidence advertises thumbnails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekMediaThumbnailCapability(NovatekR3V1Profile);

/// A recording request target that can only be constructed from recording
/// capability proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NovatekRecordingCommandTarget(&'static str);

impl NovatekRecordingCommandTarget {
    /// Returns the fixed relative request target for the validated command.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// A still-capture request target that can only be constructed from still
/// capture capability proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NovatekStillCaptureCommandTarget(&'static str);

impl NovatekStillCaptureCommandTarget {
    /// Returns the fixed relative request target for the validated command.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl NovatekR3V1Profile {
    /// Parses a bounded firmware identity and retains it as an R3V1 profile
    /// proof. Command-specific capabilities are derived separately from the
    /// profile and the camera's read-only configuration evidence.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekProfileError::UnsupportedFirmware`] when the firmware
    /// is outside the verified R3V1 family, or
    /// [`NovatekProfileError::FirmwareVersionTooLong`] when it cannot be held
    /// by the bounded firmware identity.
    pub fn parse(version: &str) -> Result<Self, NovatekProfileError> {
        if !is_r3_pro_firmware(version) {
            return Err(NovatekProfileError::UnsupportedFirmware);
        }
        let version = ArrayString::try_from(version)
            .map_err(|_| NovatekProfileError::FirmwareVersionTooLong)?;
        Ok(Self(NovatekFirmwareVersion(version)))
    }

    /// Proves that read-only configuration advertises onboard recording.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekCapabilityError::NotAdvertised`] when the bounded
    /// configuration does not acknowledge the recording command.
    pub fn recording_capability(
        &self,
        configuration: &NovatekConfiguration,
    ) -> Result<NovatekRecordingCapability, NovatekCapabilityError> {
        (configuration.status_for_command_id(NovatekCommandId::RECORDING)
            == Some(NovatekStatusCode::ACKNOWLEDGED))
        .then(|| NovatekRecordingCapability(self.clone()))
        .ok_or(NovatekCapabilityError::NotAdvertised {
            command_id: NovatekCommandId::RECORDING,
        })
    }

    /// Proves that read-only configuration advertises still capture.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekCapabilityError::NotAdvertised`] when the bounded
    /// configuration does not acknowledge the still-capture command.
    pub fn still_capture_capability(
        &self,
        configuration: &NovatekConfiguration,
    ) -> Result<NovatekStillCaptureCapability, NovatekCapabilityError> {
        (configuration.status_for_command_id(NovatekCommandId::STILL_CAPTURE)
            == Some(NovatekStatusCode::ACKNOWLEDGED))
        .then(|| NovatekStillCaptureCapability(self.clone()))
        .ok_or(NovatekCapabilityError::NotAdvertised {
            command_id: NovatekCommandId::STILL_CAPTURE,
        })
    }

    /// Proves that read-only configuration advertises media thumbnails.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekCapabilityError::NotAdvertised`] unless command `4001`
    /// has an acknowledged status.
    pub fn media_thumbnail_capability(
        &self,
        configuration: &NovatekConfiguration,
    ) -> Result<NovatekMediaThumbnailCapability, NovatekCapabilityError> {
        (configuration.status_for_command_id(NovatekCommandId::MEDIA_THUMBNAIL)
            == Some(NovatekStatusCode::ACKNOWLEDGED))
        .then(|| NovatekMediaThumbnailCapability(self.clone()))
        .ok_or(NovatekCapabilityError::NotAdvertised {
            command_id: NovatekCommandId::MEDIA_THUMBNAIL,
        })
    }

    /// Returns the verified firmware identity.
    #[must_use]
    pub fn firmware_version(&self) -> &NovatekFirmwareVersion {
        &self.0
    }
}

/// Returns whether a firmware string is in the verified R3V1 R3 Pro family.
///
/// The R3V1 family is the stable identity gate used by the R3 Pro integration;
/// other R3 revisions remain unsupported until separately validated.
#[must_use]
pub fn is_r3_pro_firmware(version: &str) -> bool {
    version.starts_with(NOVATEK_VERIFIED_R3_PRO_FIRMWARE_PREFIX)
}

/// RTSP URI returned by a Novatek live-view response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekRtspUri(ArrayString<NOVATEK_MAX_RTSP_URI_BYTES>);

impl NovatekRtspUri {
    /// Returns the validated RTSP URI text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Live-view links returned by Novatek command `2019`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekLiveViewLinks {
    movie: NovatekRtspUri,
    photo: NovatekRtspUri,
}

impl NovatekLiveViewLinks {
    /// Returns the movie-preview RTSP URI.
    #[must_use]
    pub const fn movie(&self) -> &NovatekRtspUri {
        &self.movie
    }

    /// Returns the photo-preview RTSP URI.
    #[must_use]
    pub const fn photo(&self) -> &NovatekRtspUri {
        &self.photo
    }
}

/// Storage presence reported by Novatek command `3024`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NovatekStoragePresence {
    /// The camera reports no SD card.
    Absent,
    /// The camera reports an inserted SD card.
    Present,
}

/// One command/status pair reported by Novatek command `3014`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NovatekCommandStatus {
    command_id: NovatekCommandId,
    status: NovatekStatusCode,
}

/// Camera-reported Novatek command status.
///
/// A status is intentionally distinct from a command identifier even though
/// both are encoded as unsigned integers on the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NovatekStatusCode(u16);

impl NovatekStatusCode {
    /// The camera acknowledged the requested command.
    pub const ACKNOWLEDGED: Self = Self(0);

    /// Wraps a camera-reported status value without assigning semantics to
    /// unknown vendor-specific values.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the numeric status for an FFI or error boundary.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl NovatekCommandStatus {
    /// Returns the reported command id.
    #[must_use]
    pub const fn command_id(self) -> NovatekCommandId {
        self.command_id
    }

    /// Returns the reported status value.
    #[must_use]
    pub const fn status(self) -> NovatekStatusCode {
        self.status
    }
}

/// Outcome represented by a Novatek command response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NovatekCommandOutcome {
    /// The response reported status zero.
    Acknowledged,
    /// The response reported a nonzero status.
    Refused {
        /// Camera-reported refusal status.
        status: NovatekStatusCode,
    },
    /// The bounded response did not contain a status.
    Unknown,
}

/// Bounded command/status configuration returned by Novatek command `3014`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekConfiguration {
    statuses: ArrayVec<NovatekCommandStatus, NOVATEK_MAX_COMMAND_STATUS_ENTRIES>,
}

impl NovatekConfiguration {
    /// Builds bounded command/status evidence from an adapter-owned sequence.
    ///
    /// The iterator is consumed directly into fixed storage so an untrusted
    /// mobile vector cannot allocate an unbounded intermediate collection.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekConfigurationError::TooManyStatuses`] when the fixed
    /// status bound is exceeded, or a command validation error for zero or
    /// duplicate command identifiers.
    pub fn from_status_pairs(
        pairs: impl IntoIterator<Item = (u16, u16)>,
    ) -> Result<Self, NovatekConfigurationError> {
        let mut statuses: ArrayVec<NovatekCommandStatus, NOVATEK_MAX_COMMAND_STATUS_ENTRIES> =
            ArrayVec::new();
        for (command_id, status) in pairs {
            let command_id = NovatekCommandId::new(command_id)
                .ok_or(NovatekConfigurationError::InvalidCommand { command_id })?;
            if statuses.is_full() {
                return Err(NovatekConfigurationError::TooManyStatuses {
                    max: NOVATEK_MAX_COMMAND_STATUS_ENTRIES,
                });
            }
            if statuses
                .iter()
                .any(|existing| existing.command_id == command_id)
            {
                return Err(NovatekConfigurationError::DuplicateCommand { command_id });
            }
            statuses.push(NovatekCommandStatus {
                command_id,
                status: NovatekStatusCode::new(status),
            });
        }
        Ok(Self { statuses })
    }

    /// Returns all command/status pairs in response order.
    #[must_use]
    pub fn statuses(&self) -> &[NovatekCommandStatus] {
        &self.statuses
    }

    /// Returns the status for a source-backed read command, when reported.
    #[must_use]
    pub fn status_for(&self, command: NovatekReadCommand) -> Option<NovatekStatusCode> {
        self.status_for_command_id(command.command_id())
    }

    /// Returns the status for a validated command id, when present.
    #[must_use]
    pub fn status_for_command_id(&self, command_id: NovatekCommandId) -> Option<NovatekStatusCode> {
        self.statuses
            .iter()
            .find(|entry| entry.command_id == command_id)
            .map(|entry| entry.status)
    }
}

/// Failure while establishing one validated R3V1 Novatek session.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NovatekSessionError {
    /// The firmware identity did not prove the verified R3V1 profile.
    #[error(transparent)]
    Profile(#[from] NovatekProfileError),
    /// The supplied command/status evidence was malformed or unbounded.
    #[error(transparent)]
    Configuration(#[from] NovatekConfigurationError),
}

/// Rust-owned validated Novatek R3V1 session evidence.
///
/// This object deliberately owns only protocol proof and the selected local
/// origin. Apple/Android adapters remain responsible for URL loading and
/// lifecycle effects, while command-target construction cannot reconstruct a
/// weaker firmware/configuration proof per request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekR3V1Session {
    origin: NovatekHttpOrigin,
    profile: NovatekR3V1Profile,
    configuration: NovatekConfiguration,
}

impl NovatekR3V1Session {
    /// Establishes a session from one validated origin and bounded read-only evidence.
    ///
    /// # Errors
    ///
    /// Returns an error when the firmware is outside R3V1 or the command/status
    /// evidence contains a duplicate, zero, or excessive command identifier.
    pub fn new(
        origin: NovatekHttpOrigin,
        firmware_version: &str,
        configuration: impl IntoIterator<Item = (u16, u16)>,
    ) -> Result<Self, NovatekSessionError> {
        Ok(Self {
            origin,
            profile: NovatekR3V1Profile::parse(firmware_version)?,
            configuration: NovatekConfiguration::from_status_pairs(configuration)?,
        })
    }

    /// Returns the validated local HTTP origin.
    #[must_use]
    pub const fn origin(&self) -> NovatekHttpOrigin {
        self.origin
    }

    /// Returns the verified firmware identity.
    #[must_use]
    pub fn firmware_version(&self) -> &str {
        self.profile.firmware_version().as_str()
    }

    /// Returns the bounded command/status evidence retained by the session.
    #[must_use]
    pub fn configuration(&self) -> &NovatekConfiguration {
        &self.configuration
    }

    /// Builds a recording target from the retained R3V1 and `3014` proofs.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekCapabilityError::NotAdvertised`] when the selected
    /// recording command was not acknowledged by the retained configuration.
    pub fn recording_command_target(
        &self,
        command: NovatekRecordingCommand,
    ) -> Result<NovatekRecordingCommandTarget, NovatekCapabilityError> {
        let capability = self.profile.recording_capability(&self.configuration)?;
        Ok(command.request_target_for_capability(&capability))
    }

    /// Builds a still-capture target from the retained R3V1 and `3014` proofs.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekCapabilityError::NotAdvertised`] when still capture was
    /// not acknowledged by the retained configuration.
    pub fn still_capture_command_target(
        &self,
        command: NovatekStillCaptureCommand,
    ) -> Result<NovatekStillCaptureCommandTarget, NovatekCapabilityError> {
        let capability = self.profile.still_capture_capability(&self.configuration)?;
        Ok(command.request_target_for_capability(&capability))
    }

    /// Builds a thumbnail target from retained R3V1, `3014`, and media-path proof.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekMediaThumbnailError::Capability`] when command `4001`
    /// was not acknowledged, or [`NovatekMediaThumbnailError::Path`] when the
    /// camera-reported media path is invalid.
    pub fn media_thumbnail_target(
        &self,
        path: &str,
    ) -> Result<NovatekMediaThumbnailTarget, NovatekMediaThumbnailError> {
        self.profile
            .media_thumbnail_capability(&self.configuration)?
            .request_target(path)
            .map_err(Into::into)
    }

    /// Whether this retained session advertises the thumbnail command.
    #[must_use]
    pub fn supports_media_thumbnails(&self) -> bool {
        self.profile
            .media_thumbnail_capability(&self.configuration)
            .is_ok()
    }
}

/// One bounded file record returned by Novatek command `3015`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekMediaEntry {
    name: ArrayString<NOVATEK_MAX_MEDIA_NAME_BYTES>,
    path: ArrayString<NOVATEK_MAX_MEDIA_PATH_BYTES>,
    size_bytes: u64,
    timecode: u64,
    time: ArrayString<NOVATEK_MAX_MEDIA_TIME_BYTES>,
    attributes: u32,
}

/// Rust-issued identity for one pending Novatek media download.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NovatekMediaDownloadOperationId(u64);

impl NovatekMediaDownloadOperationId {
    /// Returns the opaque numeric identity for binding adapters.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One Rust-authorized transfer target and its exact retained inventory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekMediaDownloadAuthorization {
    id: NovatekMediaDownloadOperationId,
    target: NovatekMediaDownloadTarget,
    entry: NovatekMediaEntry,
}

impl NovatekMediaDownloadAuthorization {
    /// Returns the one-shot operation identity.
    #[must_use]
    pub const fn id(&self) -> NovatekMediaDownloadOperationId {
        self.id
    }

    /// Returns the validated relative HTTP target.
    #[must_use]
    pub fn target(&self) -> &NovatekMediaDownloadTarget {
        &self.target
    }

    /// Returns the exact camera metadata authorized for this transfer.
    #[must_use]
    pub const fn entry(&self) -> &NovatekMediaEntry {
        &self.entry
    }
}

/// One-shot authorizations for media entries in a retained camera inventory.
#[derive(Debug, Default)]
pub struct NovatekMediaDownloadOperations {
    next_id: u64,
    pending: Vec<NovatekMediaDownloadAuthorization>,
}

impl NovatekMediaDownloadOperations {
    /// Authorizes an exact path from the supplied Rust-retained media list.
    /// Reauthorizing the same path replaces its prior pending operation.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is absent or cannot be mapped to a safe
    /// HTTP target, or when the operation identity space is exhausted.
    pub fn authorize(
        &mut self,
        media: &NovatekMediaList,
        path: &str,
    ) -> Result<NovatekMediaDownloadAuthorization, NovatekMediaDownloadAuthorizationError> {
        let entry = media
            .entries()
            .iter()
            .find(|entry| entry.path() == path)
            .ok_or(NovatekMediaDownloadAuthorizationError::MediaNotRetained)?;
        let target = media_download_target(entry.path())
            .map_err(NovatekMediaDownloadAuthorizationError::InvalidPath)?;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(NovatekMediaDownloadAuthorizationError::OperationIdExhausted)?;
        self.next_id = next_id;
        self.pending.retain(|pending| pending.entry.path() != path);
        let authorization = NovatekMediaDownloadAuthorization {
            id: NovatekMediaDownloadOperationId(next_id),
            target,
            entry: entry.clone(),
        };
        self.pending.push(authorization.clone());
        Ok(authorization)
    }

    /// Consumes an authorization exactly once and returns its retained entry.
    #[must_use]
    pub fn consume(
        &mut self,
        id: NovatekMediaDownloadOperationId,
    ) -> Option<NovatekMediaDownloadEntry> {
        let index = self.pending.iter().position(|pending| pending.id == id)?;
        let authorization = self.pending.remove(index);
        Some(NovatekMediaDownloadEntry(authorization.entry))
    }

    /// Consumes a binding-projected identity without exposing its constructor.
    #[must_use]
    pub fn consume_id(&mut self, id: u64) -> Option<NovatekMediaDownloadEntry> {
        self.consume(NovatekMediaDownloadOperationId(id))
    }

    /// Cancels one pending operation without consuming or returning its metadata.
    pub fn cancel(&mut self, id: NovatekMediaDownloadOperationId) -> bool {
        let Some(index) = self.pending.iter().position(|pending| pending.id == id) else {
            return false;
        };
        self.pending.remove(index);
        true
    }

    /// Cancels one binding-projected identity without exposing its constructor.
    pub fn cancel_id(&mut self, id: u64) -> bool {
        self.cancel(NovatekMediaDownloadOperationId(id))
    }

    /// Returns retained metadata for an active operation without consuming it.
    #[must_use]
    pub fn authorized_entry_id(&self, id: u64) -> Option<&NovatekMediaEntry> {
        self.pending
            .iter()
            .find(|authorization| authorization.id.get() == id)
            .map(NovatekMediaDownloadAuthorization::entry)
    }

    /// Invalidates all outstanding operations when the inventory is replaced.
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// Media metadata returned after a one-shot download authorization is consumed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekMediaDownloadEntry(NovatekMediaEntry);

impl NovatekMediaDownloadEntry {
    /// Returns the retained camera media metadata.
    #[must_use]
    pub const fn entry(&self) -> &NovatekMediaEntry {
        &self.0
    }
}

/// Why a Novatek media download could not be authorized.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NovatekMediaDownloadAuthorizationError {
    /// The requested path does not occur in the retained camera inventory.
    #[error("media is not in the retained camera inventory")]
    MediaNotRetained,
    /// The retained path cannot be converted to a safe HTTP target.
    #[error("media path is invalid")]
    InvalidPath(NovatekMediaPathError),
    /// The monotonic operation identifier cannot be advanced further.
    #[error("media download operation identity exhausted")]
    OperationIdExhausted,
}

/// Validated HTTP path for a file served by the camera's embedded web server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekMediaDownloadTarget(ArrayString<NOVATEK_MAX_MEDIA_PATH_BYTES>);

impl NovatekMediaDownloadTarget {
    /// Returns the relative HTTP target for the camera file.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

const NOVATEK_MEDIA_THUMBNAIL_QUERY: &str = "?custom=1&cmd=4001";
const NOVATEK_MAX_MEDIA_THUMBNAIL_TARGET_BYTES: usize =
    NOVATEK_MAX_MEDIA_PATH_BYTES + NOVATEK_MEDIA_THUMBNAIL_QUERY.len();

/// Validated HTTP target for a thumbnail served by the camera.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekMediaThumbnailTarget(ArrayString<NOVATEK_MAX_MEDIA_THUMBNAIL_TARGET_BYTES>);

impl NovatekMediaThumbnailTarget {
    /// Returns the relative HTTP target for the camera thumbnail.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl NovatekMediaThumbnailCapability {
    /// Builds a thumbnail request target from the acknowledged capability.
    ///
    /// # Errors
    ///
    /// Returns [`NovatekMediaPathError`] when the camera-reported path is
    /// invalid or exceeds the target bound.
    pub fn request_target(
        &self,
        path: &str,
    ) -> Result<NovatekMediaThumbnailTarget, NovatekMediaPathError> {
        media_thumbnail_target(path)
    }
}

/// Maps a camera-reported `A:\Novatek\...` path to a safe HTTP target.
///
/// The mapping follows the reference Novatek API: the drive prefix is removed
/// and Windows separators become URL path separators. Only the camera's
/// Novatek volume is accepted; components that could alter URL semantics are
/// rejected before a platform adapter constructs an origin-specific URL.
///
/// # Errors
///
/// Returns [`NovatekMediaPathError`] when the camera path is not an exact,
/// bounded Novatek media path.
pub fn media_download_target(
    path: &str,
) -> Result<NovatekMediaDownloadTarget, NovatekMediaPathError> {
    let Some(relative) = path.strip_prefix(NOVATEK_MEDIA_PATH_PREFIX) else {
        return Err(NovatekMediaPathError::InvalidRoot);
    };
    if relative.is_empty() {
        return Err(NovatekMediaPathError::UnsafeComponent);
    }

    let mut target = ArrayString::new();
    target
        .try_push_str("/Novatek/")
        .map_err(|_| NovatekMediaPathError::ValueTooLong {
            max: NOVATEK_MAX_MEDIA_PATH_BYTES,
        })?;
    for (index, component) in relative.split('\\').enumerate() {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.chars().any(|character| {
                character.is_ascii_control()
                    || character.is_ascii_whitespace()
                    || character == '?'
                    || character == '#'
                    || character == '%'
            })
            || component.contains('/')
        {
            return Err(NovatekMediaPathError::UnsafeComponent);
        }
        if index > 0 {
            target
                .try_push('/')
                .map_err(|_| NovatekMediaPathError::ValueTooLong {
                    max: NOVATEK_MAX_MEDIA_PATH_BYTES,
                })?;
        }
        target
            .try_push_str(component)
            .map_err(|_| NovatekMediaPathError::ValueTooLong {
                max: NOVATEK_MAX_MEDIA_PATH_BYTES,
            })?;
    }
    Ok(NovatekMediaDownloadTarget(target))
}

/// Maps a camera-reported media path to the source-backed thumbnail target.
///
/// The reference API requests command `4001` at the media path itself. Path
/// validation is shared with ordinary media downloads before the command is
/// appended.
///
/// # Errors
///
/// Returns [`NovatekMediaPathError`] when the camera path is not an exact,
/// bounded Novatek media path.
pub fn media_thumbnail_target(
    path: &str,
) -> Result<NovatekMediaThumbnailTarget, NovatekMediaPathError> {
    let target = media_download_target(path)?;
    let mut thumbnail = ArrayString::new();
    thumbnail
        .try_push_str(target.as_str())
        .and_then(|()| thumbnail.try_push_str(NOVATEK_MEDIA_THUMBNAIL_QUERY))
        .map_err(|_| NovatekMediaPathError::ValueTooLong {
            max: NOVATEK_MAX_MEDIA_THUMBNAIL_TARGET_BYTES,
        })?;
    Ok(NovatekMediaThumbnailTarget(thumbnail))
}

impl NovatekMediaEntry {
    /// Returns the camera-reported file name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the camera-reported file path.
    #[must_use]
    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    /// Returns the file size in bytes.
    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Returns the camera-reported timestamp code.
    #[must_use]
    pub const fn timecode(&self) -> u64 {
        self.timecode
    }

    /// Returns the camera-reported display time.
    #[must_use]
    pub fn time(&self) -> &str {
        self.time.as_str()
    }

    /// Returns the camera-reported attribute bits.
    #[must_use]
    pub const fn attributes(&self) -> u32 {
        self.attributes
    }
}

/// Bounded media metadata returned by Novatek command `3015`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekMediaList {
    entries: Vec<NovatekMediaEntry>,
}

impl NovatekMediaList {
    /// Returns media entries in camera response order.
    #[must_use]
    pub fn entries(&self) -> &[NovatekMediaEntry] {
        &self.entries
    }
}

/// Typed read-only observations collected from the verified R3 Pro profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekReadOnlySnapshot {
    firmware: NovatekFirmwareVersion,
    live_view: NovatekLiveViewLinks,
    configuration: NovatekConfiguration,
    storage: NovatekStoragePresence,
    media: Option<NovatekMediaList>,
}

impl NovatekReadOnlySnapshot {
    /// Returns the reported firmware version.
    #[must_use]
    pub const fn firmware(&self) -> &NovatekFirmwareVersion {
        &self.firmware
    }

    /// Returns the validated live-view links.
    #[must_use]
    pub const fn live_view(&self) -> &NovatekLiveViewLinks {
        &self.live_view
    }

    /// Returns the raw command/status configuration map.
    #[must_use]
    pub const fn configuration(&self) -> &NovatekConfiguration {
        &self.configuration
    }

    /// Returns the camera's SD-card presence readback.
    #[must_use]
    pub const fn storage(&self) -> NovatekStoragePresence {
        self.storage
    }

    /// Returns bounded media metadata if command `3015` has been loaded.
    #[must_use]
    pub const fn media(&self) -> Option<&NovatekMediaList> {
        self.media.as_ref()
    }
}

/// Parses the bounded XML response for Novatek command `3012`.
///
/// # Errors
///
/// Returns [`NovatekResponseError`] when the response is too large, malformed,
/// rejected by the camera, or contains an oversized firmware string.
pub fn parse_firmware_response(
    response: &[u8],
) -> Result<NovatekFirmwareVersion, NovatekResponseError> {
    let fields = parse_fields(response, "Function")?;
    parse_expected_command(&fields, NovatekReadCommand::FirmwareVersion.command_id())?;
    let status = parse_status(&fields)?;
    if status != 0 {
        return Err(NovatekResponseError::StatusFailure { status });
    }
    let version = fields.required("String")?;
    let version =
        ArrayString::try_from(version).map_err(|_| NovatekResponseError::ValueTooLong {
            tag: "String",
            max: NOVATEK_MAX_FIRMWARE_VERSION_BYTES,
        })?;
    Ok(NovatekFirmwareVersion(version))
}

/// Parses the bounded XML response for Novatek command `2019`.
///
/// # Errors
///
/// Returns [`NovatekResponseError`] when either link is absent, oversized, or
/// not an RTSP URI.
pub fn parse_live_view_response(
    response: &[u8],
) -> Result<NovatekLiveViewLinks, NovatekResponseError> {
    let fields = parse_fields(response, "LIST")?;
    Ok(NovatekLiveViewLinks {
        movie: parse_rtsp_uri(&fields, "MovieLiveViewLink")?,
        photo: parse_rtsp_uri(&fields, "PhotoLiveViewLink")?,
    })
}

/// Parses the bounded XML response for Novatek command `3024`.
///
/// # Errors
///
/// Returns [`NovatekResponseError`] when the response is malformed, rejected,
/// or contains a value other than `0` or `1`.
pub fn parse_storage_response(
    response: &[u8],
) -> Result<NovatekStoragePresence, NovatekResponseError> {
    let fields = parse_fields(response, "Function")?;
    parse_expected_command(&fields, NovatekReadCommand::StoragePresent.command_id())?;
    let status = parse_status(&fields)?;
    if status != 0 {
        return Err(NovatekResponseError::StatusFailure { status });
    }
    match fields.required("Value")? {
        "0" => Ok(NovatekStoragePresence::Absent),
        "1" => Ok(NovatekStoragePresence::Present),
        _ => Err(NovatekResponseError::InvalidStorageValue),
    }
}

/// Parses a bounded Novatek command response into a transport outcome.
///
/// A response without a status remains [`NovatekCommandOutcome::Unknown`]; it
/// is not treated as an acknowledgement. The command-specific caller remains
/// responsible for deciding whether the command was permitted by capability
/// evidence before sending it.
///
/// # Errors
///
/// Returns [`NovatekResponseError`] when the response is oversized, not UTF-8,
/// or contains a malformed status value.
pub fn parse_command_response(
    response: &[u8],
    expected_command_id: NovatekCommandId,
) -> Result<NovatekCommandOutcome, NovatekResponseError> {
    let fields = parse_fields(response, "Function")?;
    let command = match fields.required("Cmd") {
        Ok(value) => {
            let value = value
                .parse()
                .map_err(|_| NovatekResponseError::InvalidCommand)?;
            NovatekCommandId::new(value).ok_or(NovatekResponseError::InvalidCommand)?
        }
        Err(NovatekResponseError::MissingTag { tag: "Cmd" }) => {
            return Ok(NovatekCommandOutcome::Unknown);
        }
        Err(error) => return Err(error),
    };
    if command != expected_command_id {
        return Err(NovatekResponseError::UnexpectedCommand {
            actual: command,
            expected: expected_command_id,
        });
    }
    match parse_status(&fields) {
        Ok(0) => Ok(NovatekCommandOutcome::Acknowledged),
        Ok(status) => Ok(NovatekCommandOutcome::Refused {
            status: NovatekStatusCode::new(status),
        }),
        Err(NovatekResponseError::MissingTag { tag: "Status" }) => {
            Ok(NovatekCommandOutcome::Unknown)
        }
        Err(error) => Err(error),
    }
}

/// Parses a bounded command response after validating a raw expected ID.
///
/// This adapter is intended for FFI callers whose generated bindings represent
/// numeric command IDs. Rust callers should prefer [`parse_command_response`]
/// with a [`NovatekCommandId`] so an invalid zero ID cannot be represented.
///
/// # Errors
///
/// Returns [`NovatekResponseError::InvalidCommand`] for zero, or forwards the
/// bounded response/parser errors from [`parse_command_response`].
pub fn parse_command_response_for_id(
    response: &[u8],
    expected_command_id: u16,
) -> Result<NovatekCommandOutcome, NovatekResponseError> {
    let expected_command_id =
        NovatekCommandId::new(expected_command_id).ok_or(NovatekResponseError::InvalidCommand)?;
    parse_command_response(response, expected_command_id)
}

/// Parses the bounded XML response for Novatek command `3014`.
///
/// # Errors
///
/// Returns [`NovatekResponseError`] when command/status pairs are malformed or
/// exceed the fixed storage bound.
pub fn parse_configuration_response(
    response: &[u8],
) -> Result<NovatekConfiguration, NovatekResponseError> {
    let mut xml = NovatekXml::new(response, NOVATEK_MAX_RESPONSE_BYTES, "Function")?;
    let mut statuses: ArrayVec<NovatekCommandStatus, NOVATEK_MAX_COMMAND_STATUS_ENTRIES> =
        ArrayVec::new();
    let mut pending_command = None;
    let mut domain_error = None;
    while let Some((name, value)) = xml.field()? {
        if domain_error.is_some() {
            continue;
        }
        let outcome = (|| {
            match name {
                "Cmd" => {
                    if pending_command.is_some() {
                        return Err(NovatekResponseError::MissingTag { tag: "Status" });
                    }
                    pending_command = Some(parse_command_id(value)?);
                }
                "Status" => {
                    let Some(command_id) = pending_command.take() else {
                        return Ok(());
                    };
                    let status = value
                        .parse()
                        .map_err(|_| NovatekResponseError::InvalidStatus)?;
                    if statuses.is_full() {
                        return Err(NovatekResponseError::TooManyEntries {
                            tag: "Cmd",
                            max: NOVATEK_MAX_COMMAND_STATUS_ENTRIES,
                        });
                    }
                    if statuses
                        .iter()
                        .any(|existing| existing.command_id == command_id)
                    {
                        return Err(NovatekResponseError::DuplicateCommand { command_id });
                    }
                    statuses.push(NovatekCommandStatus {
                        command_id,
                        status: NovatekStatusCode::new(status),
                    });
                }
                _ => {}
            }
            Ok(())
        })();
        domain_error = outcome.err();
    }
    if let Some(error) = domain_error {
        return Err(error);
    }

    if pending_command.is_some() {
        return Err(NovatekResponseError::MissingTag { tag: "Status" });
    }
    if statuses.is_empty() {
        return Err(NovatekResponseError::MissingTag { tag: "Cmd" });
    }
    Ok(NovatekConfiguration { statuses })
}

/// Parses the bounded XML response for Novatek command `3015`.
///
/// # Errors
///
/// Returns [`NovatekResponseError`] when a file record is malformed, unsafe,
/// oversized, or exceeds the fixed entry bound.
pub fn parse_media_list_response(
    response: &[u8],
) -> Result<NovatekMediaList, NovatekResponseError> {
    let mut xml = NovatekXml::new(response, NOVATEK_MAX_MEDIA_RESPONSE_BYTES, "LIST")?;
    let mut entries = Vec::with_capacity(64);
    let mut paths = HashSet::with_capacity(64);
    let mut in_all_file = false;
    let mut domain_error = None;
    loop {
        match xml.next()? {
            Event::Start(start) if start.name().as_ref() == "ALLFile" && !in_all_file => {
                in_all_file = true;
            }
            Event::End(end) if end.name().as_ref() == "ALLFile" && in_all_file => {
                in_all_file = false;
            }
            Event::Start(start) if start.name().as_ref() == "File" => {
                let fields = xml.record_fields()?;
                if domain_error.is_some() {
                    continue;
                }
                let outcome = (|| {
                    if entries.len() >= NOVATEK_MAX_MEDIA_ENTRIES {
                        return Err(NovatekResponseError::TooManyEntries {
                            tag: "File",
                            max: NOVATEK_MAX_MEDIA_ENTRIES,
                        });
                    }
                    let entry = parse_media_entry(&fields)?;
                    if !paths.insert(entry.path) {
                        return Err(NovatekResponseError::DuplicateMediaPath);
                    }
                    entries.push(entry);
                    Ok(())
                })();
                domain_error = outcome.err();
            }
            Event::End(_) if xml.depth == 0 => {
                xml.finish()?;
                break;
            }
            Event::Empty(_) => {}
            Event::Text(text) if text.as_ref().trim().is_empty() => {}
            _ => return Err(NovatekResponseError::MalformedXml),
        }
    }
    if let Some(error) = domain_error {
        return Err(error);
    }
    Ok(NovatekMediaList { entries })
}

/// Parses the five bounded read-only responses captured from the R3 Pro.
///
/// The responses remain separate at the transport boundary; this helper only
/// composes their typed results after each parser has enforced its own limits.
///
/// # Errors
///
/// Returns the first [`NovatekResponseError`] raised by a component parser.
pub fn parse_read_only_snapshot(
    firmware_response: &[u8],
    live_view_response: &[u8],
    configuration_response: &[u8],
    storage_response: &[u8],
    media_response: &[u8],
) -> Result<NovatekReadOnlySnapshot, NovatekResponseError> {
    let mut snapshot = parse_connection_snapshot(
        firmware_response,
        live_view_response,
        configuration_response,
        storage_response,
    )?;
    snapshot.media = Some(parse_media_list_response(media_response)?);
    Ok(snapshot)
}

/// Parses the status responses needed for connection and live preview.
///
/// Media remains unobserved until the user requests the potentially large file list.
///
/// # Errors
///
/// Returns the first [`NovatekResponseError`] raised by a component parser.
pub fn parse_connection_snapshot(
    firmware_response: &[u8],
    live_view_response: &[u8],
    configuration_response: &[u8],
    storage_response: &[u8],
) -> Result<NovatekReadOnlySnapshot, NovatekResponseError> {
    Ok(NovatekReadOnlySnapshot {
        firmware: parse_firmware_response(firmware_response)?,
        live_view: parse_live_view_response(live_view_response)?,
        configuration: parse_configuration_response(configuration_response)?,
        storage: parse_storage_response(storage_response)?,
        media: None,
    })
}

fn parse_media_entry(file: &Fields<'_>) -> Result<NovatekMediaEntry, NovatekResponseError> {
    let name = media_string(file, "NAME", NOVATEK_MAX_MEDIA_NAME_BYTES)?;
    let path = media_string(file, "FPATH", NOVATEK_MAX_MEDIA_PATH_BYTES)?;
    let time = media_string(file, "TIME", NOVATEK_MAX_MEDIA_TIME_BYTES)?;
    let size_bytes = media_number(file, "SIZE")?;
    let timecode = media_number(file, "TIMECODE")?;
    let attributes = u32::try_from(media_number(file, "ATTR")?)
        .map_err(|_| NovatekResponseError::InvalidMediaValue { tag: "ATTR" })?;

    Ok(NovatekMediaEntry {
        name,
        path,
        size_bytes,
        timecode,
        time,
        attributes,
    })
}

fn media_string<const N: usize>(
    file: &Fields<'_>,
    tag: &'static str,
    max: usize,
) -> Result<ArrayString<N>, NovatekResponseError> {
    let value = file.required(tag)?;
    if value.is_empty()
        || value.chars().any(|character| character.is_ascii_control())
        || value.contains("..")
    {
        return Err(NovatekResponseError::InvalidMediaValue { tag });
    }
    ArrayString::try_from(value).map_err(|_| NovatekResponseError::ValueTooLong { tag, max })
}

fn media_number(file: &Fields<'_>, tag: &'static str) -> Result<u64, NovatekResponseError> {
    file.required(tag)?
        .parse()
        .map_err(|_| NovatekResponseError::InvalidMediaValue { tag })
}

fn parse_rtsp_uri(
    fields: &Fields<'_>,
    tag: &'static str,
) -> Result<NovatekRtspUri, NovatekResponseError> {
    let value = fields.required(tag)?;
    if !is_valid_rtsp_uri(value) {
        return Err(NovatekResponseError::InvalidRtspUri { tag });
    }
    let value = ArrayString::try_from(value).map_err(|_| NovatekResponseError::ValueTooLong {
        tag,
        max: NOVATEK_MAX_RTSP_URI_BYTES,
    })?;
    Ok(NovatekRtspUri(value))
}

fn is_local_ipv4(address: Ipv4Addr) -> bool {
    let [first, second, ..] = address.octets();
    address.is_loopback()
        || first == 10
        || (first == 172 && (16..=31).contains(&second))
        || (first == 192 && second == 168)
        || (first == 169 && second == 254)
}

fn is_valid_rtsp_uri(value: &str) -> bool {
    let Some(authority_and_path) = value.strip_prefix("rtsp://") else {
        return false;
    };
    let Some((authority, path)) = authority_and_path.split_once('/') else {
        return false;
    };
    !authority.is_empty()
        && !authority.contains('@')
        && !path.is_empty()
        && !value.chars().any(|character| character.is_ascii_control())
        && !value.chars().any(char::is_whitespace)
}

// The camera protocol uses a restricted XML subset: no attributes, comments,
// CDATA, processing instructions, or DTD. Scalar text stays literal (including
// entity references) because URI/path validation must inspect camera evidence.
struct NovatekXml<'a> {
    reader: Reader<&'a [u8]>,
    source: &'a str,
    depth: usize,
}

impl<'a> NovatekXml<'a> {
    fn new(response: &'a [u8], max: usize, root: &str) -> Result<Self, NovatekResponseError> {
        if response.len() > max {
            return Err(NovatekResponseError::ResponseTooLarge { max });
        }
        let source = str::from_utf8(response).map_err(|_| NovatekResponseError::InvalidUtf8)?;
        if source.starts_with('\u{feff}') {
            return Err(NovatekResponseError::MalformedXml);
        }
        let mut reader = Reader::from_reader(response);
        reader.config_mut().allow_dangling_amp = true;
        reader.config_mut().trim_markup_names_in_closing_tags = false;
        let mut xml = Self {
            reader,
            source,
            depth: 0,
        };
        let mut declaration = false;
        loop {
            match xml.next()? {
                Event::Decl(value) if !declaration => {
                    value
                        .version()
                        .map_err(|_| NovatekResponseError::MalformedXml)?;
                    declaration = true;
                }
                Event::Text(text) if text.as_ref().trim().is_empty() => {}
                Event::Start(start) if start.name().as_ref() == root => return Ok(xml),
                _ => return Err(NovatekResponseError::MalformedXml),
            }
        }
    }

    fn next(&mut self) -> Result<Event<'a>, NovatekResponseError> {
        let event = self
            .reader
            .read_event()
            .map_err(|_| NovatekResponseError::MalformedXml)?;
        match &event {
            Event::Start(start) | Event::Empty(start) => {
                // Attribute parsing is unnecessary: any content beyond the exact
                // element name is outside the captured Novatek subset.
                if start.as_ref() != start.name().as_ref()
                    || !start.name().as_ref().bytes().all(|byte| match byte {
                        b'_' | b'-' | b':' => true,
                        _ => byte.is_ascii_alphanumeric(),
                    })
                {
                    return Err(NovatekResponseError::MalformedXml);
                }
                if let Event::Start(_) = &event {
                    if self.depth >= NOVATEK_MAX_XML_DEPTH {
                        return Err(NovatekResponseError::MalformedXml);
                    }
                    self.depth += 1;
                }
            }
            Event::End(_) => {
                self.depth = self
                    .depth
                    .checked_sub(1)
                    .ok_or(NovatekResponseError::MalformedXml)?;
            }
            Event::Decl(_) | Event::Eof if self.depth == 0 => {}
            Event::Text(_) | Event::GeneralRef(_) => {}
            _ => return Err(NovatekResponseError::MalformedXml),
        }
        Ok(event)
    }

    // Offsets delimit borrowed scalar text; quick-xml supplies token boundaries
    // and matching-end validation. No XML text is copied or entity-decoded.
    fn scalar(&mut self) -> Result<&'a str, NovatekResponseError> {
        let start = usize::try_from(self.reader.buffer_position())
            .map_err(|_| NovatekResponseError::MalformedXml)?;
        loop {
            let end = usize::try_from(self.reader.buffer_position())
                .map_err(|_| NovatekResponseError::MalformedXml)?;
            match self.next()? {
                Event::End(_) => {
                    return self
                        .source
                        .get(start..end)
                        .map(str::trim)
                        .ok_or(NovatekResponseError::MalformedXml);
                }
                Event::Text(_) | Event::GeneralRef(_) => {}
                _ => return Err(NovatekResponseError::MalformedXml),
            }
        }
    }

    fn field(&mut self) -> Result<Option<(&'static str, &'a str)>, NovatekResponseError> {
        loop {
            match self.next()? {
                Event::Start(start) => {
                    let name = FIELD_NAMES
                        .iter()
                        .find(|name| **name == start.name().as_ref())
                        .copied()
                        .unwrap_or("");
                    return Ok(Some((name, self.scalar()?)));
                }
                Event::End(_) => {
                    if self.depth == 0 {
                        self.finish()?;
                    }
                    return Ok(None);
                }
                Event::Empty(_) => {}
                Event::Text(text) if text.as_ref().trim().is_empty() => {}
                _ => return Err(NovatekResponseError::MalformedXml),
            }
        }
    }

    fn record_fields(&mut self) -> Result<Fields<'a>, NovatekResponseError> {
        let mut fields = Fields::default();
        while let Some((name, value)) = self.field()? {
            fields.insert(name, value)?;
        }
        Ok(fields)
    }

    fn finish(&mut self) -> Result<(), NovatekResponseError> {
        loop {
            match self.next()? {
                Event::Text(text) if text.as_ref().trim().is_empty() => {}
                Event::Eof => return Ok(()),
                _ => return Err(NovatekResponseError::MalformedXml),
            }
        }
    }
}

const FIELD_NAMES: [&str; 12] = [
    "Cmd",
    "Status",
    "String",
    "Value",
    "MovieLiveViewLink",
    "PhotoLiveViewLink",
    "NAME",
    "FPATH",
    "TIME",
    "SIZE",
    "TIMECODE",
    "ATTR",
];

#[derive(Default)]
struct Fields<'a> {
    values: [Option<&'a str>; FIELD_NAMES.len()],
}

impl<'a> Fields<'a> {
    fn insert(&mut self, name: &str, value: &'a str) -> Result<(), NovatekResponseError> {
        if let Some(slot) = FIELD_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .and_then(|index| self.values.get_mut(index))
            && slot.replace(value).is_some()
        {
            return Err(NovatekResponseError::MalformedXml);
        }
        Ok(())
    }
    fn required(&self, tag: &'static str) -> Result<&'a str, NovatekResponseError> {
        FIELD_NAMES
            .iter()
            .position(|candidate| *candidate == tag)
            .and_then(|index| self.values.get(index).copied().flatten())
            .ok_or(NovatekResponseError::MissingTag { tag })
    }
}

fn parse_fields<'a>(response: &'a [u8], root: &str) -> Result<Fields<'a>, NovatekResponseError> {
    NovatekXml::new(response, NOVATEK_MAX_RESPONSE_BYTES, root)?.record_fields()
}

fn parse_status(fields: &Fields<'_>) -> Result<u16, NovatekResponseError> {
    fields
        .required("Status")?
        .parse()
        .map_err(|_| NovatekResponseError::InvalidStatus)
}

fn parse_command_id(value: &str) -> Result<NovatekCommandId, NovatekResponseError> {
    let value = value
        .parse()
        .map_err(|_| NovatekResponseError::InvalidCommand)?;
    NovatekCommandId::new(value).ok_or(NovatekResponseError::InvalidCommand)
}

fn parse_expected_command(
    fields: &Fields<'_>,
    expected: NovatekCommandId,
) -> Result<(), NovatekResponseError> {
    let actual = parse_command_id(fields.required("Cmd")?)?;
    if actual != expected {
        return Err(NovatekResponseError::UnexpectedCommand { actual, expected });
    }
    Ok(())
}

/// Non-mutating Novatek HTTP commands reported by the reference API.
///
/// These command IDs come from the reverse-engineered
/// [`rgov/novatek-api`](https://github.com/rgov/novatek-api) wrapper and public
/// Novatek compatibility documentation. Their presence does not prove that a
/// camera or firmware is compatible; callers must still verify the selected
/// local device from bounded responses.
#[repr(u16)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NovatekReadCommand {
    /// Command `2016`; the reference source does not establish its semantics.
    Command2016 = 2016,
    /// Command `2019`, reported to return the live-view format/link.
    LiveViewFormat = 2019,
    /// Command `3012`, reported to return a firmware/version string.
    FirmwareVersion = 3012,
    /// Command `3014`, reported to return configuration/status values.
    Configuration = 3014,
    /// Command `3015`, reported to return camera media paths.
    MediaList = 3015,
    /// Command `3024`, reported to return SD-card presence.
    StoragePresent = 3024,
}

impl NovatekReadCommand {
    const COMMAND_2016: NovatekCommandId = NovatekCommandId::new(2016).unwrap();
    const LIVE_VIEW_FORMAT: NovatekCommandId = NovatekCommandId::new(2019).unwrap();
    const FIRMWARE_VERSION: NovatekCommandId = NovatekCommandId::new(3012).unwrap();
    const CONFIGURATION: NovatekCommandId = NovatekCommandId::new(3014).unwrap();
    const MEDIA_LIST: NovatekCommandId = NovatekCommandId::new(3015).unwrap();
    const STORAGE_PRESENT: NovatekCommandId = NovatekCommandId::new(3024).unwrap();

    /// Returns the validated source-reported command ID.
    #[must_use]
    pub const fn command_id(self) -> NovatekCommandId {
        match self {
            Self::Command2016 => Self::COMMAND_2016,
            Self::LiveViewFormat => Self::LIVE_VIEW_FORMAT,
            Self::FirmwareVersion => Self::FIRMWARE_VERSION,
            Self::Configuration => Self::CONFIGURATION,
            Self::MediaList => Self::MEDIA_LIST,
            Self::StoragePresent => Self::STORAGE_PRESENT,
        }
    }

    /// Returns the relative control target for a selected local camera origin.
    ///
    /// Returning only a fixed relative target keeps origin discovery and
    /// validation in the platform network adapter.
    #[must_use]
    pub const fn request_target(self) -> &'static str {
        match self {
            Self::Command2016 => "/?custom=1&cmd=2016",
            Self::LiveViewFormat => "/?custom=1&cmd=2019",
            Self::FirmwareVersion => "/?custom=1&cmd=3012",
            Self::Configuration => "/?custom=1&cmd=3014",
            Self::MediaList => "/?custom=1&cmd=3015",
            Self::StoragePresent => "/?custom=1&cmd=3024",
        }
    }
}

/// Explicit onboard-recording request reported by the reference Novatek API.
///
/// This only encodes a user-requested command. It does not establish that the
/// camera accepted the request or that recording state changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NovatekRecordingCommand {
    /// Request that the camera start onboard recording.
    Start,
    /// Request that the camera stop onboard recording.
    Stop,
}

/// Explicit still-capture request reported by the reference Novatek API.
///
/// This only encodes a user-requested command. It does not establish that the
/// camera accepted the request or that a new media entry exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NovatekStillCaptureCommand;

impl NovatekStillCaptureCommand {
    /// Returns the fixed relative target for this still-capture request after
    /// the caller proves the verified R3V1 capability.
    #[must_use]
    pub const fn request_target_for_capability(
        self,
        _capability: &NovatekStillCaptureCapability,
    ) -> NovatekStillCaptureCommandTarget {
        NovatekStillCaptureCommandTarget("/?custom=1&cmd=1001")
    }
}

impl NovatekRecordingCommand {
    /// Returns the fixed relative target for this recording request after the
    /// caller proves the verified R3V1 capability.
    #[must_use]
    pub const fn request_target_for_capability(
        self,
        _capability: &NovatekRecordingCapability,
    ) -> NovatekRecordingCommandTarget {
        match self {
            Self::Start => NovatekRecordingCommandTarget("/?custom=1&cmd=2001&str=1"),
            Self::Stop => NovatekRecordingCommandTarget("/?custom=1&cmd=2001&str=0"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn command_id(value: u16) -> NovatekCommandId {
        NovatekCommandId::new(value).expect("test command id must be nonzero")
    }

    #[test]
    fn source_backed_read_commands_encode_exact_relative_targets() {
        let cases = [
            (NovatekReadCommand::Command2016, "/?custom=1&cmd=2016"),
            (NovatekReadCommand::LiveViewFormat, "/?custom=1&cmd=2019"),
            (NovatekReadCommand::FirmwareVersion, "/?custom=1&cmd=3012"),
            (NovatekReadCommand::Configuration, "/?custom=1&cmd=3014"),
            (NovatekReadCommand::MediaList, "/?custom=1&cmd=3015"),
            (NovatekReadCommand::StoragePresent, "/?custom=1&cmd=3024"),
        ];

        for (command, expected) in cases {
            assert_eq!(command.request_target(), expected);
        }
    }

    #[test]
    fn recording_commands_encode_fixed_user_requested_targets() {
        let profile = NovatekR3V1Profile::parse("R3V1.1_20240411").expect("verified profile");
        let configuration =
            NovatekConfiguration::from_status_pairs([(2001, 0)]).expect("bounded configuration");
        let capability = profile
            .recording_capability(&configuration)
            .expect("advertised recording capability");
        assert_eq!(
            NovatekRecordingCommand::Start
                .request_target_for_capability(&capability)
                .as_str(),
            "/?custom=1&cmd=2001&str=1"
        );
        assert_eq!(
            NovatekRecordingCommand::Stop
                .request_target_for_capability(&capability)
                .as_str(),
            "/?custom=1&cmd=2001&str=0"
        );
    }

    #[test]
    fn still_capture_command_encodes_fixed_user_requested_target() {
        let profile = NovatekR3V1Profile::parse("R3V1.1_20240411").expect("verified profile");
        let configuration =
            NovatekConfiguration::from_status_pairs([(1001, 0)]).expect("bounded configuration");
        let capability = profile
            .still_capture_capability(&configuration)
            .expect("advertised still capability");
        assert_eq!(
            NovatekStillCaptureCommand
                .request_target_for_capability(&capability)
                .as_str(),
            "/?custom=1&cmd=1001"
        );
    }

    #[test]
    fn validated_r3_session_retains_origin_and_reuses_capability_proofs() {
        let origin = NovatekHttpOrigin::new("192.168.1.254".parse().unwrap(), 80)
            .expect("camera origin is local");
        let session = NovatekR3V1Session::new(origin, "R3V1.1_20240411", [(2001, 0), (1001, 0)])
            .expect("captured R3V1 evidence establishes a session");

        assert_eq!(session.origin(), origin);
        assert_eq!(session.firmware_version(), "R3V1.1_20240411");
        assert_eq!(session.configuration().statuses().len(), 2);
        assert_eq!(
            session
                .recording_command_target(NovatekRecordingCommand::Start)
                .expect("recording proof is retained")
                .as_str(),
            "/?custom=1&cmd=2001&str=1"
        );
        assert_eq!(
            session
                .still_capture_command_target(NovatekStillCaptureCommand)
                .expect("still-capture proof is retained")
                .as_str(),
            "/?custom=1&cmd=1001"
        );
    }

    #[test]
    fn validated_r3_session_rejects_unverified_or_ambiguous_evidence() {
        let origin = NovatekHttpOrigin::new("192.168.1.254".parse().unwrap(), 80)
            .expect("camera origin is local");
        assert_eq!(
            NovatekR3V1Session::new(origin, "R3V2.0_20240411", [(2001, 0)]),
            Err(NovatekSessionError::Profile(
                NovatekProfileError::UnsupportedFirmware
            ))
        );
        assert_eq!(
            NovatekR3V1Session::new(origin, "R3V1.1_20240411", [(2001, 0), (2001, 7)]),
            Err(NovatekSessionError::Configuration(
                NovatekConfigurationError::DuplicateCommand {
                    command_id: NovatekCommandId::RECORDING,
                }
            ))
        );
    }

    #[test]
    fn mutating_commands_reject_an_unverified_firmware_family() {
        assert_eq!(
            NovatekR3V1Profile::parse("R4V2.0_20250101"),
            Err(NovatekProfileError::UnsupportedFirmware)
        );
    }

    #[test]
    fn mutating_commands_reject_missing_capability_evidence() {
        let profile = NovatekR3V1Profile::parse("R3V1.1_20240411").expect("verified profile");
        let configuration =
            NovatekConfiguration::from_status_pairs(std::iter::empty::<(u16, u16)>())
                .expect("empty adapter configuration is bounded");
        assert_eq!(
            profile.recording_capability(&configuration),
            Err(NovatekCapabilityError::NotAdvertised {
                command_id: command_id(2001),
            })
        );
        assert_eq!(
            profile.still_capture_capability(&configuration),
            Err(NovatekCapabilityError::NotAdvertised {
                command_id: NovatekCommandId::STILL_CAPTURE,
            })
        );
    }

    #[test]
    fn mutating_commands_reject_nonzero_capability_status() {
        let profile = NovatekR3V1Profile::parse("R3V1.1_20240411").expect("verified profile");
        let configuration = NovatekConfiguration::from_status_pairs([(2001, 7), (1001, 9)])
            .expect("bounded configuration");
        assert_eq!(
            profile.recording_capability(&configuration),
            Err(NovatekCapabilityError::NotAdvertised {
                command_id: NovatekCommandId::RECORDING,
            })
        );
        assert_eq!(
            profile.still_capture_capability(&configuration),
            Err(NovatekCapabilityError::NotAdvertised {
                command_id: NovatekCommandId::STILL_CAPTURE,
            })
        );
    }

    #[test]
    fn thumbnail_target_requires_acknowledged_r3v1_capability() {
        let origin = NovatekHttpOrigin::new("192.168.1.254".parse().unwrap(), 80)
            .expect("camera origin is local");
        let advertised = NovatekR3V1Session::new(origin, "R3V1.1_20240411", [(4001, 0)])
            .expect("verified profile retains configuration");

        assert!(advertised.supports_media_thumbnails());
        assert_eq!(
            advertised
                .media_thumbnail_target(r"A:\Novatek\Movie\clip.TS")
                .expect("advertised capability permits a safe media target")
                .as_str(),
            "/Novatek/Movie/clip.TS?custom=1&cmd=4001"
        );

        for statuses in [Vec::new(), vec![(4001, 7)]] {
            let unsupported = NovatekR3V1Session::new(origin, "R3V1.1_20240411", statuses)
                .expect("verified profile retains bounded configuration");
            assert!(!unsupported.supports_media_thumbnails());
            assert_eq!(
                unsupported.media_thumbnail_target(r"A:\Novatek\Movie\clip.TS"),
                Err(NovatekMediaThumbnailError::Capability(
                    NovatekCapabilityError::NotAdvertised {
                        command_id: NovatekCommandId::MEDIA_THUMBNAIL,
                    }
                ))
            );
        }
    }

    #[test]
    fn adapter_configuration_rejects_more_than_the_fixed_status_bound() {
        let statuses = (1..34).map(|command_id| (command_id, 0));
        assert_eq!(
            NovatekConfiguration::from_status_pairs(statuses),
            Err(NovatekConfigurationError::TooManyStatuses { max: 32 })
        );
    }

    #[test]
    fn adapter_configuration_rejects_duplicate_command_evidence() {
        assert_eq!(
            NovatekConfiguration::from_status_pairs([(2001, 0), (2001, 7)]),
            Err(NovatekConfigurationError::DuplicateCommand {
                command_id: NovatekCommandId::RECORDING,
            })
        );
    }

    #[test]
    fn adapter_configuration_rejects_zero_command_evidence() {
        assert_eq!(
            NovatekConfiguration::from_status_pairs([(0, 0)]),
            Err(NovatekConfigurationError::InvalidCommand { command_id: 0 })
        );
    }

    #[test]
    fn command_response_classifies_acknowledged_refused_and_unknown() {
        assert_eq!(
            parse_command_response(
                br"<Function><Cmd>2001</Cmd><Status>0</Status></Function>",
                command_id(2001)
            ),
            Ok(NovatekCommandOutcome::Acknowledged)
        );
        assert_eq!(
            parse_command_response(
                br"<Function><Cmd>2001</Cmd><Status>7</Status></Function>",
                command_id(2001)
            ),
            Ok(NovatekCommandOutcome::Refused {
                status: NovatekStatusCode::new(7),
            })
        );
        assert_eq!(
            parse_command_response(br"<Function><Cmd>2001</Cmd></Function>", command_id(2001),),
            Ok(NovatekCommandOutcome::Unknown)
        );
    }

    #[test]
    fn response_parsers_reject_incomplete_or_malformed_xml_documents() {
        let command = br"<Function><Cmd>2001</Cmd><Status>0</Status>";
        assert_eq!(
            parse_command_response(command, command_id(2001)),
            Err(NovatekResponseError::MalformedXml)
        );

        let mismatched_tags = br"<Function><Cmd>2001</Cmd><Status>0</Status></Cmd></Function>";
        assert_eq!(
            parse_command_response(mismatched_tags, command_id(2001)),
            Err(NovatekResponseError::MalformedXml)
        );

        let configuration = br"<Function><Cmd>2001</Cmd><Status>0</Status></Function>garbage";
        assert_eq!(
            parse_configuration_response(configuration),
            Err(NovatekResponseError::MalformedXml)
        );

        let live_view = br"<LIST><MovieLiveViewLink>rtsp://192.168.1.254/movie</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/photo</PhotoLiveViewLink>";
        assert_eq!(
            parse_live_view_response(live_view),
            Err(NovatekResponseError::MalformedXml)
        );

        let media = br"<LIST></LIST>garbage";
        assert_eq!(
            parse_media_list_response(media),
            Err(NovatekResponseError::MalformedXml)
        );
    }

    #[test]
    fn command_response_rejects_duplicate_and_nested_acknowledgement_fields() {
        for response in [
            b"<Function><Cmd>2001</Cmd><Status>0</Status><Status>7</Status></Function>".as_slice(),
            b"<Function><Wrapper><Cmd>2001</Cmd><Status>0</Status></Wrapper></Function>".as_slice(),
            b"<Function><Cmd>2001</Cmd><Status>0<Status>7</Status></Status></Function>".as_slice(),
        ] {
            assert_eq!(
                parse_command_response(response, command_id(2001)),
                Err(NovatekResponseError::MalformedXml),
            );
        }
    }

    #[test]
    fn response_parser_keeps_literal_entity_text_and_rejects_unsupported_markup() {
        let firmware = parse_firmware_response(
            br"<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1&amp;&#65;&unknown;literal</String></Function>",
        )
        .expect("literal XML references remain untransformed protocol evidence");
        assert_eq!(firmware.as_str(), "R3V1&amp;&#65;&unknown;literal");

        for markup in [
            "<!-- comment -->",
            "<![CDATA[text]]>",
            "<?camera ignored?>",
            "<!DOCTYPE Function>",
            "<Status extra=\"ignored\">0</Status>",
        ] {
            let response =
                format!("<Function><Cmd>2001</Cmd><Status>0</Status>{markup}</Function>");
            assert_eq!(
                parse_command_response(response.as_bytes(), command_id(2001)),
                Err(NovatekResponseError::MalformedXml),
            );
        }
    }

    #[test]
    fn response_parser_preserves_literal_ampersands_and_ignores_unknown_leaf_fields() {
        let response = br"<LIST><Unknown>literal&amp;text</Unknown><Empty/><MovieLiveViewLink>rtsp://192.168.1.254/live?one=1&two=2</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/photo</PhotoLiveViewLink></LIST>";
        let links = parse_live_view_response(response).expect("captured URI text stays literal");
        assert_eq!(
            links.movie().as_str(),
            "rtsp://192.168.1.254/live?one=1&two=2"
        );
        assert_eq!(
            parse_firmware_response(
                b"<Function><Cmd>3012</Cmd><Status>0</Status><String>\xff</String></Function>"
            ),
            Err(NovatekResponseError::InvalidUtf8),
        );
        assert_eq!(
            parse_firmware_response("\u{feff}<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1</String></Function>".as_bytes()),
            Err(NovatekResponseError::MalformedXml),
        );
    }

    #[test]
    fn media_list_rejects_nested_records_duplicate_fields_and_unvalidated_suffixes() {
        let file = "<File><NAME>clip.TS</NAME><FPATH>A:\\Novatek\\Movie\\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File>";
        for response in [
            format!(
                "<LIST>{}</LIST>",
                file.replace("<NAME>", "<File><NAME>")
                    .replace("</NAME>", "</NAME></File>")
            ),
            format!(
                "<LIST>{}</LIST>",
                file.replace("</NAME>", "</NAME><NAME>other.TS</NAME>")
            ),
            format!("<LIST>{file}</LIST><LIST></LIST>"),
            format!("<LIST>{file}<ALLFile>"),
            format!("<LIST>{file}</LIST>trailing"),
        ] {
            assert_eq!(
                parse_media_list_response(response.as_bytes()),
                Err(NovatekResponseError::MalformedXml)
            );
        }
    }

    #[test]
    fn media_list_enforces_entry_and_response_bounds() {
        let mut response = String::from("<LIST>");
        for index in 0..NOVATEK_MAX_MEDIA_ENTRIES {
            write!(response, "<File><NAME>{index}.TS</NAME><FPATH>A:\\Novatek\\Movie\\{index}.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File>").unwrap();
        }
        let prefix = response.clone();
        response.push_str("</LIST>");
        response.extend(std::iter::repeat_n(
            ' ',
            NOVATEK_MAX_MEDIA_RESPONSE_BYTES - response.len(),
        ));
        assert_eq!(
            parse_media_list_response(response.as_bytes())
                .unwrap()
                .entries()
                .len(),
            NOVATEK_MAX_MEDIA_ENTRIES
        );
        response.push(' ');
        assert_eq!(
            parse_media_list_response(response.as_bytes()),
            Err(NovatekResponseError::ResponseTooLarge {
                max: NOVATEK_MAX_MEDIA_RESPONSE_BYTES
            })
        );
        let extra = format!("{prefix}<File></File></LIST>");
        assert_eq!(
            parse_media_list_response(extra.as_bytes()),
            Err(NovatekResponseError::TooManyEntries {
                tag: "File",
                max: NOVATEK_MAX_MEDIA_ENTRIES
            })
        );
    }

    #[test]
    fn configuration_enforces_pair_order_and_fixed_entry_bound() {
        assert_eq!(
            parse_configuration_response(
                br"<Function><Cmd>1002</Cmd><Cmd>1003</Cmd><Status>0</Status></Function>"
            ),
            Err(NovatekResponseError::MissingTag { tag: "Status" })
        );
        let mut response = String::from("<Function>");
        for index in 1..=NOVATEK_MAX_COMMAND_STATUS_ENTRIES {
            write!(response, "<Cmd>{index}</Cmd><Status>0</Status>").unwrap();
        }
        let accepted = format!("{response}</Function>");
        assert_eq!(
            parse_configuration_response(accepted.as_bytes())
                .unwrap()
                .statuses()
                .len(),
            NOVATEK_MAX_COMMAND_STATUS_ENTRIES
        );
        response.push_str("<Cmd>1000</Cmd><Status>0</Status></Function>");
        assert_eq!(
            parse_configuration_response(response.as_bytes()),
            Err(NovatekResponseError::TooManyEntries {
                tag: "Cmd",
                max: NOVATEK_MAX_COMMAND_STATUS_ENTRIES
            })
        );
    }

    #[test]
    fn malformed_document_takes_precedence_over_streamed_domain_errors() {
        assert_eq!(
            parse_configuration_response(
                br"<Function><Cmd>0</Cmd><Status>bad</Status></Function>trailing"
            ),
            Err(NovatekResponseError::MalformedXml),
        );
        assert_eq!(
            parse_media_list_response(br"<LIST><File><NAME>bad..name</NAME></File></LIST>trailing"),
            Err(NovatekResponseError::MalformedXml),
        );
    }

    #[test]
    fn media_fields_keep_width_numeric_and_missing_field_errors() {
        let file = "<File><NAME>clip.TS</NAME><FPATH>A:\\Novatek\\Movie\\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File>";
        for (file, expected) in [
            (
                file.replace("<SIZE>42</SIZE>", "<SIZE>invalid</SIZE>"),
                NovatekResponseError::InvalidMediaValue { tag: "SIZE" },
            ),
            (
                file.replace("<ATTR>32</ATTR>", "<ATTR>4294967296</ATTR>"),
                NovatekResponseError::InvalidMediaValue { tag: "ATTR" },
            ),
            (
                file.replace("<NAME>clip.TS</NAME>", ""),
                NovatekResponseError::MissingTag { tag: "NAME" },
            ),
            (
                file.replace(
                    "clip.TS</NAME>",
                    &format!("{}</NAME>", "x".repeat(NOVATEK_MAX_MEDIA_NAME_BYTES + 1)),
                ),
                NovatekResponseError::ValueTooLong {
                    tag: "NAME",
                    max: NOVATEK_MAX_MEDIA_NAME_BYTES,
                },
            ),
        ] {
            let response = format!("<LIST>{file}</LIST>");
            assert_eq!(
                parse_media_list_response(response.as_bytes()),
                Err(expected)
            );
        }
    }

    #[test]
    fn command_response_rejects_a_different_command_acknowledgement() {
        assert_eq!(
            parse_command_response(
                br"<Function><Cmd>3024</Cmd><Status>0</Status></Function>",
                command_id(2001),
            ),
            Err(NovatekResponseError::UnexpectedCommand {
                actual: command_id(3024),
                expected: command_id(2001),
            })
        );
    }

    #[test]
    fn command_response_rejects_zero_command_ids() {
        assert_eq!(
            parse_command_response(
                br"<Function><Cmd>0</Cmd><Status>0</Status></Function>",
                command_id(2001),
            ),
            Err(NovatekResponseError::InvalidCommand)
        );
        assert_eq!(
            parse_command_response_for_id(
                br"<Function><Cmd>2001</Cmd><Status>0</Status></Function>",
                0,
            ),
            Err(NovatekResponseError::InvalidCommand)
        );
    }

    #[test]
    fn firmware_response_returns_the_reported_version() {
        let response = br#"<?xml version="1.0" encoding="UTF-8" ?>
<Function>
<Cmd>3012</Cmd>
<Status>0</Status>
<String>R3V1.1_20240411</String>
</Function>"#;

        let firmware = parse_firmware_response(response).expect("fixture is valid");

        assert_eq!(firmware.as_str(), "R3V1.1_20240411");
    }

    #[test]
    fn firmware_response_rejects_a_different_command() {
        let response = br"<Function><Cmd>3024</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>";

        assert_eq!(
            parse_firmware_response(response),
            Err(NovatekResponseError::UnexpectedCommand {
                actual: command_id(3024),
                expected: command_id(3012),
            })
        );
    }

    #[test]
    fn firmware_family_check_does_not_generalize_to_other_versions() {
        assert!(is_r3_pro_firmware("R3V1.1_20240411"));
        assert!(is_r3_pro_firmware("R3V1.2_20250101"));
        assert!(!is_r3_pro_firmware("R3V2.0_20250101"));
        assert!(!is_r3_pro_firmware("R4V2.0_20250101"));
        assert!(!is_r3_pro_firmware("R3-not-a-firmware-version"));
    }

    #[test]
    fn live_view_response_returns_valid_rtsp_links() {
        let response = br#"<?xml version="1.0" encoding="UTF-8" ?>
<LIST>
<MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink>
<PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink>
</LIST>"#;

        let links = parse_live_view_response(response).expect("fixture is valid");

        assert_eq!(links.movie().as_str(), "rtsp://192.168.1.254/xxx.mov");
        assert_eq!(links.photo().as_str(), "rtsp://192.168.1.254/xxx.mov");
    }

    #[test]
    fn storage_response_maps_one_to_present() {
        let response = br#"<?xml version="1.0" encoding="UTF-8" ?>
<Function>
<Cmd>3024</Cmd>
<Status>0</Status>
<Value>1</Value>
</Function>"#;

        assert_eq!(
            parse_storage_response(response).expect("fixture is valid"),
            NovatekStoragePresence::Present
        );
    }

    #[test]
    fn storage_response_rejects_a_different_command() {
        let response = br"<Function><Cmd>3012</Cmd><Status>0</Status><Value>1</Value></Function>";

        assert_eq!(
            parse_storage_response(response),
            Err(NovatekResponseError::UnexpectedCommand {
                actual: command_id(3012),
                expected: command_id(3024),
            })
        );
    }

    #[test]
    fn oversized_firmware_response_is_rejected_before_parsing() {
        let response = vec![b' '; NOVATEK_MAX_RESPONSE_BYTES + 1];

        assert_eq!(
            parse_firmware_response(&response),
            Err(NovatekResponseError::ResponseTooLarge {
                max: NOVATEK_MAX_RESPONSE_BYTES
            })
        );
    }

    #[test]
    fn live_view_rejects_non_rtsp_links() {
        let response = br"<LIST>
<MovieLiveViewLink>http://192.168.1.254/xxx.mov</MovieLiveViewLink>
<PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink>
</LIST>";

        assert_eq!(
            parse_live_view_response(response),
            Err(NovatekResponseError::InvalidRtspUri {
                tag: "MovieLiveViewLink"
            })
        );
    }

    #[test]
    fn live_view_rejects_uri_userinfo() {
        let response = br"<LIST>
<MovieLiveViewLink>rtsp://camera-user:camera-password@192.168.1.254/xxx.mov</MovieLiveViewLink>
<PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink>
</LIST>";

        assert_eq!(
            parse_live_view_response(response),
            Err(NovatekResponseError::InvalidRtspUri {
                tag: "MovieLiveViewLink"
            })
        );
    }

    #[test]
    fn storage_rejects_camera_failure_status() {
        let response = br"<Function><Cmd>3024</Cmd><Status>11</Status><Value>1</Value></Function>";

        assert_eq!(
            parse_storage_response(response),
            Err(NovatekResponseError::StatusFailure { status: 11 })
        );
    }

    #[test]
    fn configuration_response_preserves_command_status_pairs() {
        let response = br"<Function>
<Cmd>1002</Cmd><Status>0</Status>
<Cmd>2016</Cmd><Status>0</Status>
<Cmd>2002</Cmd><Status>11</Status>
</Function>";

        let configuration = parse_configuration_response(response).expect("fixture is valid");

        assert_eq!(configuration.statuses().len(), 3);
        assert_eq!(
            configuration.statuses()[0].command_id(),
            NovatekCommandId::new(1002).unwrap()
        );
        assert_eq!(
            configuration.status_for(NovatekReadCommand::Command2016),
            Some(NovatekStatusCode::ACKNOWLEDGED)
        );
        assert_eq!(
            configuration.status_for_command_id(NovatekCommandId::new(2002).unwrap()),
            Some(NovatekStatusCode::new(11))
        );
    }

    #[test]
    fn configuration_response_rejects_duplicate_command_evidence() {
        let response = br"<Function>
<Cmd>2001</Cmd><Status>0</Status>
<Cmd>2001</Cmd><Status>7</Status>
</Function>";

        assert_eq!(
            parse_configuration_response(response),
            Err(NovatekResponseError::DuplicateCommand {
                command_id: NovatekCommandId::RECORDING,
            })
        );
    }

    #[test]
    fn empty_media_list_is_valid_read_only_evidence() {
        let media = parse_media_list_response(br"<LIST></LIST>")
            .expect("an empty camera card listing is valid");

        assert_eq!(media.entries(), []);
    }

    #[test]
    fn media_list_response_returns_bounded_file_metadata() {
        let response = br"<LIST>
<ALLFile><File>
<NAME>20250619070156_001134.TS</NAME>
<FPATH>A:\Novatek\Movie\20250619070156_001134.TS</FPATH>
<SIZE>77531952</SIZE>
<TIMECODE>1523791964</TIMECODE>
<TIME>2025/06/19 07:02:56</TIME>
<ATTR>32</ATTR></File></ALLFile>
<ALLFile><File>
<NAME>20251021191727_001681.JPG</NAME>
<FPATH>A:\Novatek\Photo\20251021191727_001681.JPG</FPATH>
<SIZE>1234</SIZE>
<TIMECODE>1530000000</TIMECODE>
<TIME>2025/10/21 19:17:27</TIME>
<ATTR>16</ATTR></File></ALLFile>
</LIST>";

        let media = parse_media_list_response(response).expect("fixture is valid");

        assert_eq!(media.entries().len(), 2);
        assert_eq!(media.entries()[0].name(), "20250619070156_001134.TS");
        assert_eq!(media.entries()[0].size_bytes(), 77_531_952);
        assert_eq!(
            media.entries()[1].path(),
            r"A:\Novatek\Photo\20251021191727_001681.JPG"
        );
    }

    #[test]
    fn media_download_authorization_uses_retained_metadata_and_is_one_shot() {
        let media = parse_media_list_response(br"<LIST><File><NAME>clip.TS</NAME><FPATH>A:\Novatek\Movie\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>")
            .expect("fixture is valid");
        let mut operations = NovatekMediaDownloadOperations::default();
        let authorization = operations
            .authorize(&media, r"A:\Novatek\Movie\clip.TS")
            .expect("listed path is authorized");

        assert_eq!(authorization.target().as_str(), "/Novatek/Movie/clip.TS");
        assert_eq!(authorization.entry().timecode(), 7);
        let consumed = operations
            .consume(authorization.id())
            .expect("authorized operation can be consumed");
        assert_eq!(consumed.entry(), &media.entries()[0]);
        assert_eq!(operations.consume(authorization.id()), None);
    }

    #[test]
    fn media_download_reauthorization_replaces_only_the_same_pending_path() {
        let media = parse_media_list_response(br"<LIST><File><NAME>one.TS</NAME><FPATH>A:\Novatek\Movie\one.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File><File><NAME>two.TS</NAME><FPATH>A:\Novatek\Movie\two.TS</FPATH><SIZE>24</SIZE><TIMECODE>8</TIMECODE><TIME>2025/01/01 00:00:01</TIME><ATTR>32</ATTR></File></LIST>")
            .expect("fixture is valid");
        let mut operations = NovatekMediaDownloadOperations::default();
        let first = operations
            .authorize(&media, r"A:\Novatek\Movie\one.TS")
            .expect("first authorization");
        let unrelated = operations
            .authorize(&media, r"A:\Novatek\Movie\two.TS")
            .expect("unrelated authorization");
        let replacement = operations
            .authorize(&media, r"A:\Novatek\Movie\one.TS")
            .expect("reauthorization replaces the earlier operation");

        assert_eq!(operations.consume(first.id()), None);
        assert_eq!(
            operations.consume(unrelated.id()).unwrap().entry().path(),
            r"A:\Novatek\Movie\two.TS"
        );
        assert_eq!(
            operations.consume(replacement.id()).unwrap().entry().path(),
            r"A:\Novatek\Movie\one.TS"
        );
    }

    #[test]
    fn cancelling_media_download_removes_only_its_authorization() {
        let media = parse_media_list_response(br"<LIST><File><NAME>one.TS</NAME><FPATH>A:\Novatek\Movie\one.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File><File><NAME>two.TS</NAME><FPATH>A:\Novatek\Movie\two.TS</FPATH><SIZE>24</SIZE><TIMECODE>8</TIMECODE><TIME>2025/01/01 00:00:01</TIME><ATTR>32</ATTR></File></LIST>")
            .expect("fixture is valid");
        let mut operations = NovatekMediaDownloadOperations::default();
        let cancelled = operations
            .authorize(&media, r"A:\Novatek\Movie\one.TS")
            .expect("first authorization");
        let retained = operations
            .authorize(&media, r"A:\Novatek\Movie\two.TS")
            .expect("second authorization");

        assert!(operations.cancel(cancelled.id()));
        assert!(!operations.cancel(cancelled.id()));
        assert_eq!(
            operations
                .authorized_entry_id(retained.id().get())
                .map(NovatekMediaEntry::path),
            Some(r"A:\Novatek\Movie\two.TS")
        );
    }

    #[test]
    fn replacing_the_media_inventory_invalidates_pending_downloads() {
        let media = parse_media_list_response(br"<LIST><File><NAME>clip.TS</NAME><FPATH>A:\Novatek\Movie\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>")
            .expect("fixture is valid");
        let mut operations = NovatekMediaDownloadOperations::default();
        let authorization = operations
            .authorize(&media, r"A:\Novatek\Movie\clip.TS")
            .expect("listed path is authorized");

        operations.clear();

        assert_eq!(operations.consume(authorization.id()), None);
    }

    #[test]
    fn media_download_authorization_rejects_unlisted_paths() {
        let media =
            parse_media_list_response(br"<LIST></LIST>").expect("empty camera inventory is valid");
        let mut operations = NovatekMediaDownloadOperations::default();

        assert_eq!(
            operations.authorize(&media, r"A:\Novatek\Movie\clip.TS"),
            Err(NovatekMediaDownloadAuthorizationError::MediaNotRetained)
        );
    }

    #[test]
    fn media_list_parser_accepts_the_observed_r3_inventory_size() {
        const OBSERVED_ENTRY_COUNT: usize = 1_826;
        let mut response = String::from("<LIST>");
        for index in 0..OBSERVED_ENTRY_COUNT {
            write!(
                response,
                "<ALLFile><File><NAME>20260822112613_{index:06}.TS</NAME>\
<FPATH>A:\\Novatek\\Movie\\20260822112613_{index:06}.TS</FPATH>\
<SIZE>70827496</SIZE><TIMECODE>1561746275</TIMECODE>\
<TIME>2026/08/22 11:27:06</TIME><ATTR>32</ATTR></File></ALLFile>"
            )
            .expect("writing the bounded fixture");
        }
        response.push_str("</LIST>");

        let media = parse_media_list_response(response.as_bytes())
            .expect("the observed R3 inventory remains within parser bounds");

        assert_eq!(media.entries().len(), OBSERVED_ENTRY_COUNT);
        assert_eq!(media.entries()[0].name(), "20260822112613_000000.TS");
        assert_eq!(
            media.entries()[OBSERVED_ENTRY_COUNT - 1].path(),
            r"A:\Novatek\Movie\20260822112613_001825.TS"
        );
    }

    #[test]
    fn media_list_rejects_path_traversal() {
        let response = br"<LIST><File>
<NAME>clip.TS</NAME>
<FPATH>A:\Novatek\Movie\..\clip.TS</FPATH>
<SIZE>1</SIZE><TIMECODE>1</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>0</ATTR>
</File></LIST>";

        assert_eq!(
            parse_media_list_response(response),
            Err(NovatekResponseError::InvalidMediaValue { tag: "FPATH" })
        );
    }

    #[test]
    fn media_list_rejects_duplicate_paths() {
        let response = br"<LIST>
<File><NAME>one.TS</NAME><FPATH>A:\Novatek\Movie\one.TS</FPATH><SIZE>1</SIZE><TIMECODE>1</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>0</ATTR></File>
<File><NAME>duplicate.TS</NAME><FPATH>A:\Novatek\Movie\one.TS</FPATH><SIZE>2</SIZE><TIMECODE>2</TIMECODE><TIME>2025/01/01 00:00:01</TIME><ATTR>0</ATTR></File>
</LIST>";

        assert_eq!(
            parse_media_list_response(response),
            Err(NovatekResponseError::DuplicateMediaPath)
        );
    }

    #[test]
    fn media_path_maps_camera_drive_to_http_target() {
        let target = media_download_target(r"A:\Novatek\Movie\clip.TS")
            .expect("captured camera path is downloadable");

        assert_eq!(target.as_str(), "/Novatek/Movie/clip.TS");
    }

    #[test]
    fn media_path_maps_to_source_backed_thumbnail_target() {
        let target = media_thumbnail_target(r"A:\Novatek\Movie\clip.TS")
            .expect("camera media path is valid");
        assert_eq!(target.as_str(), "/Novatek/Movie/clip.TS?custom=1&cmd=4001");
    }

    #[test]
    fn media_path_rejects_unsafe_or_non_camera_paths() {
        for path in [
            r"/Novatek/Movie/clip.TS",
            r"A:\Novatek\Movie\..\clip.TS",
            r"A:\Novatek\Movie\clip?raw.TS",
        ] {
            assert!(
                media_download_target(path).is_err(),
                "path should be rejected: {path}"
            );
        }
    }

    #[test]
    fn read_only_snapshot_composes_the_verified_r3_responses() {
        let firmware = br"<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>";
        let live_view = br"<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>";
        let configuration = br"<Function><Cmd>2016</Cmd><Status>0</Status></Function>";
        let storage = br"<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>";
        let media = br"<LIST><File><NAME>clip.TS</NAME><FPATH>A:\Novatek\Movie\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>";

        let snapshot = parse_read_only_snapshot(firmware, live_view, configuration, storage, media)
            .expect("captured read-only responses are valid");

        assert_eq!(snapshot.firmware().as_str(), "R3V1.1_20240411");
        assert_eq!(
            snapshot.live_view().movie().as_str(),
            "rtsp://192.168.1.254/xxx.mov"
        );
        assert_eq!(
            snapshot
                .configuration()
                .status_for_command_id(NovatekCommandId::new(2016).unwrap()),
            Some(NovatekStatusCode::ACKNOWLEDGED)
        );
        assert_eq!(snapshot.storage(), NovatekStoragePresence::Present);
        assert_eq!(snapshot.media().unwrap().entries()[0].size_bytes(), 42);
    }

    #[test]
    fn r3_pro_access_point_origin_matches_the_captured_camera() {
        let origin = NovatekHttpOrigin::r3_pro_access_point();
        assert_eq!(origin.address(), std::net::Ipv4Addr::new(192, 168, 1, 254));
        assert_eq!(origin.port(), 80);
    }

    #[test]
    fn http_origin_accepts_captured_private_address() {
        let origin = NovatekHttpOrigin::new(
            "192.168.1.254".parse().expect("fixture address is valid"),
            80,
        )
        .expect("captured camera origin is local");

        assert_eq!(origin.address().to_string(), "192.168.1.254");
        assert_eq!(origin.port(), 80);
    }

    #[test]
    fn http_origin_rejects_public_address_and_zero_port() {
        assert_eq!(
            NovatekHttpOrigin::new("8.8.8.8".parse().expect("fixture address is valid"), 80,),
            Err(NovatekOriginError::NonLocalAddress)
        );
        assert_eq!(
            NovatekHttpOrigin::new(
                "192.168.1.254".parse().expect("fixture address is valid"),
                0,
            ),
            Err(NovatekOriginError::InvalidPort)
        );
    }
}
