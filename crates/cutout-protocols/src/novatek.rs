//! Source-backed requests for the Novatek HTTP API targeted by R3 Pro support.

use std::{net::Ipv4Addr, num::NonZeroU16, str};

use arrayvec::{ArrayString, ArrayVec};
use thiserror::Error;

/// Maximum XML response size accepted by the small Novatek read parsers.
pub const NOVATEK_MAX_RESPONSE_BYTES: usize = 4 * 1024;

/// Maximum XML response size accepted by the Novatek media-list parser.
pub const NOVATEK_MAX_MEDIA_RESPONSE_BYTES: usize = 512 * 1024;

const NOVATEK_MAX_FIRMWARE_VERSION_BYTES: usize = 64;
const NOVATEK_MAX_RTSP_URI_BYTES: usize = 256;
const NOVATEK_MAX_COMMAND_STATUS_ENTRIES: usize = 32;
const NOVATEK_MAX_MEDIA_ENTRIES: usize = 2_048;
const NOVATEK_MAX_MEDIA_NAME_BYTES: usize = 128;
const NOVATEK_MAX_MEDIA_PATH_BYTES: usize = 256;
const NOVATEK_MAX_MEDIA_TIME_BYTES: usize = 32;

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
    /// The response contained more repeated entries than the parser stores.
    #[error("Novatek response contains more than {max} <{tag}> entries")]
    TooManyEntries {
        /// Repeated element whose bound was exceeded.
        tag: &'static str,
        /// Maximum number of stored entries.
        max: usize,
    },
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

/// Validated local IPv4 origin for Novatek HTTP control.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NovatekHttpOrigin {
    address: Ipv4Addr,
    port: NonZeroU16,
}

impl NovatekHttpOrigin {
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
    command_id: u16,
    status: u16,
}

impl NovatekCommandStatus {
    /// Returns the reported command id.
    #[must_use]
    pub const fn command_id(self) -> u16 {
        self.command_id
    }

    /// Returns the reported status value.
    #[must_use]
    pub const fn status(self) -> u16 {
        self.status
    }
}

/// Bounded command/status configuration returned by Novatek command `3014`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NovatekConfiguration {
    statuses: ArrayVec<NovatekCommandStatus, NOVATEK_MAX_COMMAND_STATUS_ENTRIES>,
}

impl NovatekConfiguration {
    /// Returns all command/status pairs in response order.
    #[must_use]
    pub fn statuses(&self) -> &[NovatekCommandStatus] {
        &self.statuses
    }

    /// Returns the status for a source-backed read command, when reported.
    #[must_use]
    pub fn status_for(&self, command: NovatekReadCommand) -> Option<u16> {
        self.status_for_command_id(command.command_id())
    }

    /// Returns the status for a raw reported command id, when present.
    #[must_use]
    pub fn status_for_command_id(&self, command_id: u16) -> Option<u16> {
        self.statuses
            .iter()
            .find(|entry| entry.command_id == command_id)
            .map(|entry| entry.status)
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
    media: NovatekMediaList,
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

    /// Returns the bounded media metadata.
    #[must_use]
    pub const fn media(&self) -> &NovatekMediaList {
        &self.media
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
    let xml = bounded_xml(response)?;
    let status = parse_status(xml)?;
    if status != 0 {
        return Err(NovatekResponseError::StatusFailure { status });
    }
    let version = extract_tag(xml, "String", "<String>", "</String>")?;
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
    let xml = bounded_xml(response)?;
    Ok(NovatekLiveViewLinks {
        movie: parse_rtsp_uri(xml, "MovieLiveViewLink")?,
        photo: parse_rtsp_uri(xml, "PhotoLiveViewLink")?,
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
    let xml = bounded_xml(response)?;
    let status = parse_status(xml)?;
    if status != 0 {
        return Err(NovatekResponseError::StatusFailure { status });
    }
    match extract_tag(xml, "Value", "<Value>", "</Value>")? {
        "0" => Ok(NovatekStoragePresence::Absent),
        "1" => Ok(NovatekStoragePresence::Present),
        _ => Err(NovatekResponseError::InvalidStorageValue),
    }
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
    let xml = bounded_xml(response)?;
    let mut cursor = 0;
    let mut statuses = ArrayVec::new();

    while let Some(command_offset) = xml
        .get(cursor..)
        .and_then(|remaining| remaining.find("<Cmd>"))
    {
        let command_start = cursor + command_offset + "<Cmd>".len();
        let command_end = xml
            .get(command_start..)
            .and_then(|remaining| remaining.find("</Cmd>").map(|index| command_start + index))
            .ok_or(NovatekResponseError::MissingTag { tag: "Cmd" })?;
        let command_id = xml
            .get(command_start..command_end)
            .ok_or(NovatekResponseError::MissingTag { tag: "Cmd" })?
            .trim()
            .parse()
            .map_err(|_| NovatekResponseError::InvalidCommand)?;

        let after_command = xml
            .get(command_end + "</Cmd>".len()..)
            .ok_or(NovatekResponseError::MissingTag { tag: "Status" })?;
        let status_offset = after_command
            .find("<Status>")
            .ok_or(NovatekResponseError::MissingTag { tag: "Status" })?;
        if after_command
            .find("<Cmd>")
            .is_some_and(|next_command| next_command < status_offset)
        {
            return Err(NovatekResponseError::MissingTag { tag: "Status" });
        }
        let status_start = command_end + "</Cmd>".len() + status_offset + "<Status>".len();
        let status_end = xml
            .get(status_start..)
            .and_then(|remaining| {
                remaining
                    .find("</Status>")
                    .map(|index| status_start + index)
            })
            .ok_or(NovatekResponseError::MissingTag { tag: "Status" })?;
        let status = xml
            .get(status_start..status_end)
            .ok_or(NovatekResponseError::MissingTag { tag: "Status" })?
            .trim()
            .parse()
            .map_err(|_| NovatekResponseError::InvalidStatus)?;

        if statuses.is_full() {
            return Err(NovatekResponseError::TooManyEntries {
                tag: "Cmd",
                max: NOVATEK_MAX_COMMAND_STATUS_ENTRIES,
            });
        }
        statuses.push(NovatekCommandStatus { command_id, status });
        cursor = status_end + "</Status>".len();
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
    let xml = bounded_xml_with_limit(response, NOVATEK_MAX_MEDIA_RESPONSE_BYTES)?;
    let mut cursor = 0;
    let mut entries = Vec::with_capacity(64);

    while let Some(file_offset) = xml
        .get(cursor..)
        .and_then(|remaining| remaining.find("<File>"))
    {
        if entries.len() >= NOVATEK_MAX_MEDIA_ENTRIES {
            return Err(NovatekResponseError::TooManyEntries {
                tag: "File",
                max: NOVATEK_MAX_MEDIA_ENTRIES,
            });
        }
        let file_start = cursor + file_offset + "<File>".len();
        let file_end = xml
            .get(file_start..)
            .and_then(|remaining| remaining.find("</File>").map(|index| file_start + index))
            .ok_or(NovatekResponseError::MissingTag { tag: "File" })?;
        let file = xml
            .get(file_start..file_end)
            .ok_or(NovatekResponseError::MissingTag { tag: "File" })?;
        entries.push(parse_media_entry(file)?);
        cursor = file_end + "</File>".len();
    }

    if entries.is_empty() {
        return Err(NovatekResponseError::MissingTag { tag: "File" });
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
    Ok(NovatekReadOnlySnapshot {
        firmware: parse_firmware_response(firmware_response)?,
        live_view: parse_live_view_response(live_view_response)?,
        configuration: parse_configuration_response(configuration_response)?,
        storage: parse_storage_response(storage_response)?,
        media: parse_media_list_response(media_response)?,
    })
}

fn parse_media_entry(file: &str) -> Result<NovatekMediaEntry, NovatekResponseError> {
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
    file: &str,
    tag: &'static str,
    max: usize,
) -> Result<ArrayString<N>, NovatekResponseError> {
    let (open, close) = match tag {
        "NAME" => ("<NAME>", "</NAME>"),
        "FPATH" => ("<FPATH>", "</FPATH>"),
        "TIME" => ("<TIME>", "</TIME>"),
        _ => return Err(NovatekResponseError::MissingTag { tag }),
    };
    let value = extract_tag(file, tag, open, close)?;
    if value.is_empty()
        || value.chars().any(|character| character.is_ascii_control())
        || value.contains("..")
    {
        return Err(NovatekResponseError::InvalidMediaValue { tag });
    }
    ArrayString::try_from(value).map_err(|_| NovatekResponseError::ValueTooLong { tag, max })
}

fn media_number(file: &str, tag: &'static str) -> Result<u64, NovatekResponseError> {
    let (open, close) = match tag {
        "SIZE" => ("<SIZE>", "</SIZE>"),
        "TIMECODE" => ("<TIMECODE>", "</TIMECODE>"),
        "ATTR" => ("<ATTR>", "</ATTR>"),
        _ => return Err(NovatekResponseError::MissingTag { tag }),
    };
    extract_tag(file, tag, open, close)?
        .parse()
        .map_err(|_| NovatekResponseError::InvalidMediaValue { tag })
}

fn parse_rtsp_uri(xml: &str, tag: &'static str) -> Result<NovatekRtspUri, NovatekResponseError> {
    let (open, close) = match tag {
        "MovieLiveViewLink" => ("<MovieLiveViewLink>", "</MovieLiveViewLink>"),
        "PhotoLiveViewLink" => ("<PhotoLiveViewLink>", "</PhotoLiveViewLink>"),
        _ => return Err(NovatekResponseError::MissingTag { tag }),
    };
    let value = extract_tag(xml, tag, open, close)?;
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
        && !path.is_empty()
        && !value.chars().any(|character| character.is_ascii_control())
        && !value.chars().any(char::is_whitespace)
}

fn bounded_xml(response: &[u8]) -> Result<&str, NovatekResponseError> {
    bounded_xml_with_limit(response, NOVATEK_MAX_RESPONSE_BYTES)
}

fn bounded_xml_with_limit(response: &[u8], max: usize) -> Result<&str, NovatekResponseError> {
    if response.len() > max {
        return Err(NovatekResponseError::ResponseTooLarge { max });
    }
    str::from_utf8(response).map_err(|_| NovatekResponseError::InvalidUtf8)
}

fn extract_tag<'a>(
    xml: &'a str,
    tag: &'static str,
    open: &str,
    close: &str,
) -> Result<&'a str, NovatekResponseError> {
    let start = xml
        .find(open)
        .map(|index| index + open.len())
        .ok_or(NovatekResponseError::MissingTag { tag })?;
    let end = xml
        .get(start..)
        .and_then(|remaining| remaining.find(close).map(|index| start + index))
        .ok_or(NovatekResponseError::MissingTag { tag })?;
    xml.get(start..end)
        .map(str::trim)
        .ok_or(NovatekResponseError::MissingTag { tag })
}

fn parse_status(xml: &str) -> Result<u16, NovatekResponseError> {
    extract_tag(xml, "Status", "<Status>", "</Status>")?
        .parse()
        .map_err(|_| NovatekResponseError::InvalidStatus)
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
    /// Returns the source-reported numeric command ID.
    #[must_use]
    pub const fn command_id(self) -> u16 {
        self as u16
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

#[cfg(test)]
mod tests {
    use super::*;

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
            configuration.status_for(NovatekReadCommand::Command2016),
            Some(0)
        );
        assert_eq!(configuration.status_for_command_id(2002), Some(11));
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
            snapshot.configuration().status_for_command_id(2016),
            Some(0)
        );
        assert_eq!(snapshot.storage(), NovatekStoragePresence::Present);
        assert_eq!(snapshot.media().entries()[0].size_bytes(), 42);
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
