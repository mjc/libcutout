use arrayvec::ArrayVec;
use bytes::Bytes;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(any(feature = "serde", test))]
use crate::GattRoles;
#[cfg(feature = "serde")]
use crate::VerificationStatus;
#[cfg(any(feature = "serde", test))]
use crate::VescControllerId;
use crate::{
    DeviceEvent, GattChannel, GattFingerprint, HostSession, LinkInfo, MonotonicMillis,
    NotificationChunkLen, ProtocolFamily, ProtocolSession, ReplayChunkComparison, RequestTarget,
    SemanticEventCount, SessionInput, SessionOutput, TransportWriteLen, VerifiedValue,
    WallClockUnixMillis, WriteMode,
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
pub const PEVCAP_MAX_GATT_FINGERPRINTS: usize = 16;

/// Maximum annotations stored in the PEVCAP header.
pub const PEVCAP_MAX_ANNOTATIONS: usize = 8;

/// Stable annotation key used for labeled capture sessions.
pub const PEVCAP_CAPTURE_LABEL_ANNOTATION_KEY: &str = "capture_label";

/// Stable annotation key used for capture privacy class.
pub const PEVCAP_CAPTURE_PRIVACY_ANNOTATION_KEY: &str = "capture_privacy";

/// Stable annotation key used for capture redistribution permission.
pub const PEVCAP_CAPTURE_DISTRIBUTION_ANNOTATION_KEY: &str = "capture_distribution";

/// Stable annotation key used for capture-level evidence class.
pub const PEVCAP_CAPTURE_EVIDENCE_ANNOTATION_KEY: &str = "capture_evidence";

#[cfg(feature = "serde")]
const PEVCAP_BINARY_LENGTH_PREFIX_BYTES: usize = 4;

/// Standard hardware capture session labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSessionLabel {
    /// Device powered on and stationary.
    PoweredOnStationary,

    /// Device rolling forward.
    RollingForward,

    /// Device rolling backward.
    RollingBackward,

    /// Wheel lifted safely off the ground.
    LiftedWheel,

    /// Device connected to a charger.
    Charging,

    /// Headlight toggled during capture.
    HeadlightToggled,

    /// Horn or alert sound triggered during capture.
    Horn,

    /// Ride mode changed during capture.
    RideModeChange,

    /// Alarm threshold changed during capture.
    AlarmChange,

    /// BMS screen or app BMS view observed during capture.
    BmsScreen,

    /// Disconnect and reconnect path observed.
    DisconnectReconnect,

    /// Device power-cycled around the capture.
    PowerCycle,
}

impl CaptureSessionLabel {
    /// All standard capture labels in stable taxonomy order.
    pub const ALL: [Self; 12] = [
        Self::PoweredOnStationary,
        Self::RollingForward,
        Self::RollingBackward,
        Self::LiftedWheel,
        Self::Charging,
        Self::HeadlightToggled,
        Self::Horn,
        Self::RideModeChange,
        Self::AlarmChange,
        Self::BmsScreen,
        Self::DisconnectReconnect,
        Self::PowerCycle,
    ];

    /// Stable lowercase slug used in PEVCAP annotations and Beads issue notes.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::PoweredOnStationary => "powered_on_stationary",
            Self::RollingForward => "rolling_forward",
            Self::RollingBackward => "rolling_backward",
            Self::LiftedWheel => "lifted_wheel",
            Self::Charging => "charging",
            Self::HeadlightToggled => "headlight_toggled",
            Self::Horn => "horn",
            Self::RideModeChange => "ride_mode_change",
            Self::AlarmChange => "alarm_change",
            Self::BmsScreen => "bms_screen",
            Self::DisconnectReconnect => "disconnect_reconnect",
            Self::PowerCycle => "power_cycle",
        }
    }

    /// Stable PEVCAP annotation string for this label.
    #[must_use]
    pub fn annotation(self) -> String {
        format!("{}={}", PEVCAP_CAPTURE_LABEL_ANNOTATION_KEY, self.slug())
    }
}

/// Capture privacy state for long-lived PEVCAP files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturePrivacy {
    /// Capture contains private identifiers, timing, location-adjacent data, or
    /// user annotations and should not be redistributed.
    Private,

    /// Sensitive data has been intentionally redacted.
    Redacted,
}

impl CapturePrivacy {
    /// Stable lowercase slug used in PEVCAP annotations and Beads issue notes.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Redacted => "redacted",
        }
    }

    /// Stable PEVCAP annotation string for this privacy marker.
    #[must_use]
    pub fn annotation(self) -> String {
        format!("{}={}", PEVCAP_CAPTURE_PRIVACY_ANNOTATION_KEY, self.slug())
    }
}

/// Capture redistribution permission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureDistribution {
    /// Capture can be redistributed as a checked-in fixture or shared corpus
    /// item.
    Redistributable,
}

impl CaptureDistribution {
    /// Stable lowercase slug used in PEVCAP annotations and Beads issue notes.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Redistributable => "redistributable",
        }
    }

    /// Stable PEVCAP annotation string for this redistribution marker.
    #[must_use]
    pub fn annotation(self) -> String {
        format!(
            "{}={}",
            PEVCAP_CAPTURE_DISTRIBUTION_ANNOTATION_KEY,
            self.slug()
        )
    }
}

/// Capture-level evidence class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureEvidence {
    /// Evidence was recorded from actual Bluetooth hardware.
    HardwareTested,

    /// Evidence is inferred from indirect source, model, or fixture data.
    Inferred,

    /// Evidence has not been verified.
    Unverified,
}

impl CaptureEvidence {
    /// Stable lowercase slug used in PEVCAP annotations and Beads issue notes.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::HardwareTested => "hardware_tested",
            Self::Inferred => "inferred",
            Self::Unverified => "unverified",
        }
    }

    /// Stable PEVCAP annotation string for this evidence marker.
    #[must_use]
    pub fn annotation(self) -> String {
        format!("{}={}", PEVCAP_CAPTURE_EVIDENCE_ANNOTATION_KEY, self.slug())
    }
}

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

/// Supported PEVCAP byte encodings for file and tooling surfaces.
#[cfg(feature = "serde")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapEncoding {
    /// Line-delimited JSON representation for review.
    Jsonl,

    /// Binary PEVCAP container representation for storage and replay tooling.
    Binary,
}

/// Direction of a captured transport record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapDirection {
    /// Link became available to the host.
    LinkUp,

    /// Link became unavailable to the host.
    LinkDown,

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
    pub wall_clock_start_unix_ms: WallClockUnixMillis,

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
        wall_clock_start_unix_ms: WallClockUnixMillis,
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

    /// Negotiated maximum write length, when this is a link-up record.
    pub link_max_write_len: Option<u16>,

    /// Optional request target metadata for outbound correlation.
    pub target: Option<RequestTarget>,

    /// Exact bytes captured for the record.
    pub bytes: Bytes,
}

impl PevcapRecord {
    /// Creates a link-up lifecycle record.
    #[must_use]
    pub fn link_up(monotonic_ms: MonotonicMillis, max_write_len: Option<u16>) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::LinkUp,
            characteristic: GattChannel::from_bytes([0; 16]),
            service: None,
            write_mode: None,
            link_max_write_len: max_write_len,
            target: None,
            bytes: Bytes::new(),
        }
    }

    /// Creates a link-down lifecycle record.
    #[must_use]
    pub fn link_down(monotonic_ms: MonotonicMillis) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::LinkDown,
            characteristic: GattChannel::from_bytes([0; 16]),
            service: None,
            write_mode: None,
            link_max_write_len: None,
            target: None,
            bytes: Bytes::new(),
        }
    }

    /// Creates an outbound write record.
    #[must_use]
    pub fn outbound_write(
        monotonic_ms: MonotonicMillis,
        characteristic: GattChannel,
        write_mode: WriteMode,
        bytes: impl Into<Bytes>,
    ) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::Outbound,
            characteristic,
            service: None,
            write_mode: Some(write_mode),
            link_max_write_len: None,
            target: None,
            bytes: bytes.into(),
        }
    }

    /// Creates an outbound write record with explicit request target metadata.
    #[must_use]
    pub fn targeted_outbound_write(
        monotonic_ms: MonotonicMillis,
        characteristic: GattChannel,
        write_mode: WriteMode,
        bytes: impl Into<Bytes>,
        target: RequestTarget,
    ) -> Self {
        Self {
            target: Some(target),
            ..Self::outbound_write(monotonic_ms, characteristic, write_mode, bytes)
        }
    }

    /// Creates an inbound notification record.
    #[must_use]
    pub fn inbound_notification(
        monotonic_ms: MonotonicMillis,
        characteristic: GattChannel,
        service: GattChannel,
        bytes: impl Into<Bytes>,
    ) -> Self {
        Self {
            monotonic_ms,
            direction: PevcapDirection::Inbound,
            characteristic,
            service: Some(service),
            write_mode: None,
            link_max_write_len: None,
            target: None,
            bytes: bytes.into(),
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

    /// Replays PEVCAP records directly through a host session using borrowed
    /// payload slices and caller-provided output storage.
    ///
    /// Outbound writes are preserved in PEVCAP for audit but are intentionally
    /// not replayed as host inputs.
    pub fn replay_into_host<S>(&self, host: &mut HostSession<S>, outputs: &mut Vec<SessionOutput>)
    where
        S: ProtocolSession,
    {
        self.replay_mode_into_host(PevcapReplayMode::Whole, host, outputs);
    }

    /// Replays PEVCAP records directly through a host session, splitting each
    /// inbound notification into one-byte chunks.
    pub fn replay_one_byte_notifications_into_host<S>(
        &self,
        host: &mut HostSession<S>,
        outputs: &mut Vec<SessionOutput>,
    ) where
        S: ProtocolSession,
    {
        self.replay_mode_into_host(PevcapReplayMode::OneByte, host, outputs);
    }

    /// Replays PEVCAP records directly through a host session, splitting each
    /// inbound notification by the provided chunk lengths.
    pub fn replay_notification_chunks_into_host<S>(
        &self,
        lengths: &[NotificationChunkLen],
        host: &mut HostSession<S>,
        outputs: &mut Vec<SessionOutput>,
    ) where
        S: ProtocolSession,
    {
        self.replay_mode_into_host(PevcapReplayMode::Lengths(lengths), host, outputs);
    }

    /// Counts host inputs represented by this capture's replay path.
    #[must_use]
    pub fn replay_input_count(&self) -> usize {
        usize::from(
            !self
                .records
                .iter()
                .any(|record| record.direction == PevcapDirection::LinkUp),
        )
        .saturating_add(
            self.records
                .iter()
                .filter(|record| record.direction != PevcapDirection::Outbound)
                .count(),
        )
    }

    /// Builds the deterministic arbitrary notification chunk plan for this
    /// capture without materializing owned replay records.
    #[must_use]
    pub fn arbitrary_notification_chunk_lengths(&self) -> Vec<NotificationChunkLen> {
        let max_notification_len = self
            .records
            .iter()
            .filter_map(|record| {
                (record.direction == PevcapDirection::Inbound).then_some(record.bytes.len())
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
            lengths.push(NotificationChunkLen::new(next));
            covered += next;
        }
        lengths
    }

    /// Compares whole-notification PEVCAP replay against one-byte and
    /// arbitrary notification chunk replay without materializing owned replay
    /// records.
    #[must_use]
    pub fn compare_replay_chunks<S, F>(
        &self,
        mut make_session: F,
        arbitrary_lengths: &[NotificationChunkLen],
    ) -> ReplayChunkComparison
    where
        S: ProtocolSession,
        F: FnMut() -> S,
    {
        let whole =
            self.replay_semantic_events(HostSession::new(make_session()), PevcapReplayMode::Whole);
        let one_byte = self
            .replay_semantic_events(HostSession::new(make_session()), PevcapReplayMode::OneByte);
        let arbitrary = self.replay_semantic_events(
            HostSession::new(make_session()),
            PevcapReplayMode::Lengths(arbitrary_lengths),
        );

        ReplayChunkComparison {
            whole_semantic_events: SemanticEventCount::new(whole.len()),
            one_byte_semantic_events: SemanticEventCount::new(one_byte.len()),
            arbitrary_semantic_events: SemanticEventCount::new(arbitrary.len()),
            one_byte_matches: one_byte == whole,
            arbitrary_matches: arbitrary == whole,
        }
    }

    fn replay_semantic_events<S>(
        &self,
        mut host: HostSession<S>,
        mode: PevcapReplayMode<'_>,
    ) -> Vec<DeviceEvent>
    where
        S: ProtocolSession,
    {
        let mut outputs = Vec::new();
        self.replay_mode_into_host(mode, &mut host, &mut outputs);
        outputs
            .into_iter()
            .filter_map(|output| match output {
                SessionOutput::Event(event) => Some(event),
                SessionOutput::Transport(_) | SessionOutput::NotificationIngest(_) => None,
            })
            .collect()
    }

    fn replay_mode_into_host<S>(
        &self,
        mode: PevcapReplayMode<'_>,
        host: &mut HostSession<S>,
        outputs: &mut Vec<SessionOutput>,
    ) where
        S: ProtocolSession,
    {
        if !self
            .records
            .iter()
            .any(|record| record.direction == PevcapDirection::LinkUp)
        {
            host.ingest_link_up(LinkInfo {
                monotonic_ms: MonotonicMillis::new(0),
                max_write_len: transport_write_len(self.header.write_limit),
            });
            host.drain_outputs_into(outputs);
        }

        for record in &self.records {
            match record.direction {
                PevcapDirection::LinkUp => {
                    host.ingest_link_up(LinkInfo {
                        monotonic_ms: record.monotonic_ms,
                        max_write_len: transport_write_len(record.link_max_write_len),
                    });
                }
                PevcapDirection::LinkDown => host.ingest_link_down(),
                PevcapDirection::Inbound => {
                    replay_pevcap_notification(record, mode, host, outputs);
                    continue;
                }
                PevcapDirection::Outbound => {}
            }
            host.drain_outputs_into(outputs);
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
                    records.push(record.try_into_record().map_err(|source| {
                        PevcapJsonlError::Record {
                            line: line_number,
                            source,
                        }
                    })?);
                }
            }
        }

        Ok(Self {
            version: version.ok_or(PevcapJsonlError::MissingHeader)?,
            header: header.ok_or(PevcapJsonlError::MissingHeader)?,
            records,
        })
    }

    /// Serializes this capture as a binary PEVCAP container.
    ///
    /// The container starts with PEVCAP magic and version bytes, followed by a
    /// length-prefixed header payload and ordered length-prefixed record
    /// payloads.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapBinaryError::Serialize`] when payload serialization
    /// fails, or [`PevcapBinaryError::LengthTooLarge`] when a payload cannot be
    /// represented by the v1 length prefix.
    #[cfg(feature = "serde")]
    pub fn to_binary(&self) -> Result<Vec<u8>, PevcapBinaryError> {
        let header = serde_json::to_vec(&PevcapHeaderJson::from(&self.header))
            .map_err(PevcapBinaryError::Serialize)?;

        let mut output = Vec::new();
        output.extend_from_slice(&PEVCAP_MAGIC);
        write_u16_le(&mut output, self.version.major);
        write_u16_le(&mut output, self.version.minor);
        write_len_prefixed(&mut output, PevcapBinarySection::Header, &header)?;
        write_u32_le(
            &mut output,
            u32::try_from(self.records.len()).map_err(|_| PevcapBinaryError::LengthTooLarge {
                section: PevcapBinarySection::RecordCount,
                len: self.records.len(),
            })?,
        );

        for record in &self.records {
            let payload = serde_json::to_vec(&PevcapRecordJson::from(record))
                .map_err(PevcapBinaryError::Serialize)?;
            write_len_prefixed(&mut output, PevcapBinarySection::Record, &payload)?;
        }

        Ok(output)
    }

    /// Deserializes a capture from a binary PEVCAP container.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapBinaryError`] when the container is malformed, has the
    /// wrong magic/version, is truncated, has trailing bytes, or violates PEVCAP
    /// header bounds.
    #[cfg(feature = "serde")]
    pub fn from_binary(input: &[u8]) -> Result<Self, PevcapBinaryError> {
        let mut remaining = input;
        let magic = read_exact(
            &mut remaining,
            PEVCAP_MAGIC.len(),
            PevcapBinarySection::Magic,
        )?;
        if magic != PEVCAP_MAGIC {
            return Err(PevcapBinaryError::InvalidMagic);
        }

        let version = PevcapFormatVersion {
            major: read_u16_le(&mut remaining, PevcapBinarySection::Version)?,
            minor: read_u16_le(&mut remaining, PevcapBinarySection::Version)?,
        };
        if version != PevcapFormatVersion::current() {
            return Err(PevcapBinaryError::UnsupportedVersion { version });
        }

        let header = read_len_prefixed(&mut remaining, PevcapBinarySection::Header)?;
        let header = serde_json::from_slice::<PevcapHeaderJson>(header)
            .map_err(|source| PevcapBinaryError::Deserialize {
                section: PevcapBinarySection::Header,
                source,
            })?
            .try_into_header()?;

        let record_count = read_u32_le(&mut remaining, PevcapBinarySection::RecordCount)?;
        let mut records = Vec::with_capacity(record_count as usize);
        for _ in 0..record_count {
            let payload = read_len_prefixed(&mut remaining, PevcapBinarySection::Record)?;
            records.push(
                serde_json::from_slice::<PevcapRecordJson>(payload)
                    .map_err(|source| PevcapBinaryError::Deserialize {
                        section: PevcapBinarySection::Record,
                        source,
                    })?
                    .try_into_record()
                    .map_err(PevcapBinaryError::Record)?,
            );
        }

        if !remaining.is_empty() {
            return Err(PevcapBinaryError::TrailingBytes {
                len: remaining.len(),
            });
        }

        Ok(Self {
            version,
            header,
            records,
        })
    }

    /// Serializes this capture using the requested PEVCAP encoding.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapCodecError`] when the selected encoder cannot serialize
    /// the capture.
    #[cfg(feature = "serde")]
    pub fn encode(&self, encoding: PevcapEncoding) -> Result<Vec<u8>, PevcapCodecError> {
        match encoding {
            PevcapEncoding::Jsonl => Ok(self.to_jsonl()?.into_bytes()),
            PevcapEncoding::Binary => Ok(self.to_binary()?),
        }
    }

    /// Deserializes a capture using the requested PEVCAP encoding.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapCodecError`] when the selected decoder cannot parse the
    /// input bytes.
    #[cfg(feature = "serde")]
    pub fn decode(input: &[u8], encoding: PevcapEncoding) -> Result<Self, PevcapCodecError> {
        match encoding {
            PevcapEncoding::Jsonl => Ok(Self::from_jsonl(std::str::from_utf8(input)?)?),
            PevcapEncoding::Binary => Ok(Self::from_binary(input)?),
        }
    }
}

#[derive(Clone, Copy)]
enum PevcapReplayMode<'a> {
    Whole,
    OneByte,
    Lengths(&'a [NotificationChunkLen]),
}

fn replay_pevcap_notification<S>(
    record: &PevcapRecord,
    mode: PevcapReplayMode<'_>,
    host: &mut HostSession<S>,
    outputs: &mut Vec<SessionOutput>,
) where
    S: ProtocolSession,
{
    match mode {
        PevcapReplayMode::Whole => {
            host.ingest(SessionInput::Notification {
                channel: record.characteristic,
                bytes: record.bytes.as_ref(),
                monotonic_ms: record.monotonic_ms,
            });
            host.drain_outputs_into(outputs);
        }
        PevcapReplayMode::OneByte => {
            for chunk in record.bytes.as_ref().chunks(1) {
                host.ingest(SessionInput::Notification {
                    channel: record.characteristic,
                    bytes: chunk,
                    monotonic_ms: record.monotonic_ms,
                });
                host.drain_outputs_into(outputs);
            }
        }
        PevcapReplayMode::Lengths(lengths) => {
            let mut offset = 0usize;
            for length in lengths.iter().copied().filter(|length| !length.is_whole()) {
                if offset >= record.bytes.len() {
                    break;
                }
                let end = offset.saturating_add(length.get()).min(record.bytes.len());
                host.ingest(SessionInput::Notification {
                    channel: record.characteristic,
                    bytes: &record.bytes[offset..end],
                    monotonic_ms: record.monotonic_ms,
                });
                host.drain_outputs_into(outputs);
                offset = end;
            }
            if offset < record.bytes.len() {
                host.ingest(SessionInput::Notification {
                    channel: record.characteristic,
                    bytes: &record.bytes[offset..],
                    monotonic_ms: record.monotonic_ms,
                });
                host.drain_outputs_into(outputs);
            }
        }
    }
}

fn transport_write_len(value: Option<u16>) -> Option<TransportWriteLen> {
    value.map(TransportWriteLen::new)
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

    /// A record line decoded as JSON but violated PEVCAP record invariants.
    #[error("malformed PEVCAP JSONL record at line {line}: {source}")]
    Record {
        /// One-based line number.
        line: usize,

        /// Record invariant failure.
        source: PevcapRecordError,
    },
}

/// PEVCAP encoding/decoding error for format-dispatched tooling.
#[cfg(feature = "serde")]
#[derive(Debug, Error)]
pub enum PevcapCodecError {
    /// JSONL text was not valid UTF-8.
    #[error("PEVCAP JSONL input is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// JSONL import or export failed.
    #[error(transparent)]
    Jsonl(#[from] PevcapJsonlError),

    /// Binary import or export failed.
    #[error(transparent)]
    Binary(#[from] PevcapBinaryError),
}

/// PEVCAP binary container import/export error.
#[cfg(feature = "serde")]
#[derive(Debug, Error)]
pub enum PevcapBinaryError {
    /// Payload serialization failed.
    #[error("failed to serialize PEVCAP binary payload: {0}")]
    Serialize(serde_json::Error),

    /// Payload deserialization failed.
    #[error("failed to deserialize PEVCAP binary {section:?} payload: {source}")]
    Deserialize {
        /// Container section being decoded.
        section: PevcapBinarySection,

        /// Underlying JSON payload error.
        source: serde_json::Error,
    },

    /// Header magic bytes did not match PEVCAP.
    #[error("invalid PEVCAP binary magic")]
    InvalidMagic,

    /// Header version is not supported by this reader.
    #[error("unsupported PEVCAP binary version {version:?}")]
    UnsupportedVersion {
        /// Decoded version.
        version: PevcapFormatVersion,
    },

    /// The container ended before a section could be read completely.
    #[error("truncated PEVCAP binary {section:?} section")]
    Truncated {
        /// Container section being decoded.
        section: PevcapBinarySection,
    },

    /// The container had bytes after the expected final record.
    #[error("PEVCAP binary has {len} trailing bytes")]
    TrailingBytes {
        /// Number of unexpected trailing bytes.
        len: usize,
    },

    /// A payload length cannot be represented by the v1 framing prefix.
    #[error("PEVCAP binary {section:?} payload length {len} is too large")]
    LengthTooLarge {
        /// Container section being encoded.
        section: PevcapBinarySection,

        /// Payload length.
        len: usize,
    },

    /// Header metadata violated bounded PEVCAP limits.
    #[error(transparent)]
    Header(#[from] PevcapHeaderError),

    /// A record payload decoded as JSON but violated PEVCAP record invariants.
    #[error("malformed PEVCAP binary record payload: {0}")]
    Record(PevcapRecordError),
}

/// PEVCAP binary container section identifier for error reporting.
#[cfg(feature = "serde")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PevcapBinarySection {
    /// Magic bytes at the start of the container.
    Magic,

    /// Format version fields.
    Version,

    /// Capture header payload.
    Header,

    /// Number of ordered records in the container.
    RecordCount,

    /// Capture record payload.
    Record,
}

/// PEVCAP record-level invariant failure after raw file decoding.
#[cfg(feature = "serde")]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PevcapRecordError {
    /// An inbound notification record did not carry the required service UUID.
    #[error("inbound PEVCAP record is missing service UUID")]
    MissingInboundService,

    /// A non-inbound record carried service metadata.
    #[error("non-inbound PEVCAP record carried service UUID")]
    UnexpectedService,

    /// An outbound write record did not carry the required write mode.
    #[error("outbound PEVCAP record is missing write mode")]
    MissingOutboundWriteMode,

    /// A non-outbound record carried write-mode metadata.
    #[error("non-outbound PEVCAP record carried write mode")]
    UnexpectedWriteMode,

    /// A non-link-up record carried link maximum write length metadata.
    #[error("non-link-up PEVCAP record carried link max write length")]
    UnexpectedLinkMaxWriteLen,

    /// A non-outbound record carried request-target metadata.
    #[error("non-outbound PEVCAP record carried request target metadata")]
    UnexpectedTarget,

    /// A link lifecycle record carried payload bytes.
    #[error("link lifecycle PEVCAP record carried payload bytes")]
    UnexpectedLinkBytes,
}

#[cfg(feature = "serde")]
fn write_u16_le(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(feature = "serde")]
fn write_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(feature = "serde")]
fn write_len_prefixed(
    output: &mut Vec<u8>,
    section: PevcapBinarySection,
    payload: &[u8],
) -> Result<(), PevcapBinaryError> {
    write_u32_le(
        output,
        u32::try_from(payload.len()).map_err(|_| PevcapBinaryError::LengthTooLarge {
            section,
            len: payload.len(),
        })?,
    );
    output.extend_from_slice(payload);
    Ok(())
}

#[cfg(feature = "serde")]
fn read_exact<'input>(
    input: &mut &'input [u8],
    len: usize,
    section: PevcapBinarySection,
) -> Result<&'input [u8], PevcapBinaryError> {
    if input.len() < len {
        return Err(PevcapBinaryError::Truncated { section });
    }
    let (head, tail) = input.split_at(len);
    *input = tail;
    Ok(head)
}

#[cfg(feature = "serde")]
fn read_u16_le(input: &mut &[u8], section: PevcapBinarySection) -> Result<u16, PevcapBinaryError> {
    let bytes = read_exact(input, 2, section)?;
    let array =
        <[u8; 2]>::try_from(bytes).map_err(|_err| PevcapBinaryError::Truncated { section })?;
    Ok(u16::from_le_bytes(array))
}

#[cfg(feature = "serde")]
fn read_u32_le(input: &mut &[u8], section: PevcapBinarySection) -> Result<u32, PevcapBinaryError> {
    let bytes = read_exact(input, PEVCAP_BINARY_LENGTH_PREFIX_BYTES, section)?;
    let array = <[u8; PEVCAP_BINARY_LENGTH_PREFIX_BYTES]>::try_from(bytes)
        .map_err(|_err| PevcapBinaryError::Truncated { section })?;
    Ok(u32::from_le_bytes(array))
}

#[cfg(feature = "serde")]
fn read_len_prefixed<'input>(
    input: &mut &'input [u8],
    section: PevcapBinarySection,
) -> Result<&'input [u8], PevcapBinaryError> {
    let len = read_u32_le(input, section)? as usize;
    read_exact(input, len, section)
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
            wall_clock_start_unix_ms: header.wall_clock_start_unix_ms.as_milliseconds(),
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
            WallClockUnixMillis::from_milliseconds(self.wall_clock_start_unix_ms),
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
    monotonic_ms: u64,
    direction: PevcapDirectionJson,
    characteristic: [u8; 16],
    service: Option<[u8; 16]>,
    write_mode: Option<WriteModeJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link_max_write_len: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<PevcapRequestTargetJson>,
    bytes: Bytes,
}

#[cfg(feature = "serde")]
impl From<&PevcapRecord> for PevcapRecordJson {
    fn from(record: &PevcapRecord) -> Self {
        Self {
            monotonic_ms: record.monotonic_ms.get(),
            direction: PevcapDirectionJson::from(record.direction),
            characteristic: record.characteristic.as_bytes(),
            service: record.service.map(GattChannel::as_bytes),
            write_mode: record.write_mode.map(WriteModeJson::from),
            link_max_write_len: record.link_max_write_len,
            target: record.target.map(PevcapRequestTargetJson::from),
            bytes: record.bytes.clone(),
        }
    }
}

#[cfg(feature = "serde")]
impl PevcapRecordJson {
    fn try_into_record(self) -> Result<PevcapRecord, PevcapRecordError> {
        self.validate()?;
        Ok(PevcapRecord {
            monotonic_ms: MonotonicMillis::new(self.monotonic_ms),
            direction: self.direction.into_direction(),
            characteristic: GattChannel::from_bytes(self.characteristic),
            service: self.service.map(GattChannel::from_bytes),
            write_mode: self.write_mode.map(WriteModeJson::into_mode),
            link_max_write_len: self.link_max_write_len,
            target: self.target.map(PevcapRequestTargetJson::into_target),
            bytes: self.bytes,
        })
    }

    fn validate(&self) -> Result<(), PevcapRecordError> {
        match self.direction {
            PevcapDirectionJson::LinkUp | PevcapDirectionJson::LinkDown => {
                if self.service.is_some() {
                    return Err(PevcapRecordError::UnexpectedService);
                }
                if self.write_mode.is_some() {
                    return Err(PevcapRecordError::UnexpectedWriteMode);
                }
                if self.target.is_some() {
                    return Err(PevcapRecordError::UnexpectedTarget);
                }
                if !self.bytes.is_empty() {
                    return Err(PevcapRecordError::UnexpectedLinkBytes);
                }
                if matches!(self.direction, PevcapDirectionJson::LinkDown)
                    && self.link_max_write_len.is_some()
                {
                    return Err(PevcapRecordError::UnexpectedLinkMaxWriteLen);
                }
            }
            PevcapDirectionJson::Inbound => {
                if self.service.is_none() {
                    return Err(PevcapRecordError::MissingInboundService);
                }
                if self.write_mode.is_some() {
                    return Err(PevcapRecordError::UnexpectedWriteMode);
                }
                if self.link_max_write_len.is_some() {
                    return Err(PevcapRecordError::UnexpectedLinkMaxWriteLen);
                }
                if self.target.is_some() {
                    return Err(PevcapRecordError::UnexpectedTarget);
                }
            }
            PevcapDirectionJson::Outbound => {
                if self.service.is_some() {
                    return Err(PevcapRecordError::UnexpectedService);
                }
                if self.write_mode.is_none() {
                    return Err(PevcapRecordError::MissingOutboundWriteMode);
                }
                if self.link_max_write_len.is_some() {
                    return Err(PevcapRecordError::UnexpectedLinkMaxWriteLen);
                }
            }
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PevcapRequestTargetJson {
    Local,
    VescCanController { controller_id: u8 },
}

#[cfg(feature = "serde")]
impl From<RequestTarget> for PevcapRequestTargetJson {
    fn from(target: RequestTarget) -> Self {
        match target {
            RequestTarget::Local => Self::Local,
            RequestTarget::VescCanController { controller_id } => Self::VescCanController {
                controller_id: controller_id.get(),
            },
        }
    }
}

#[cfg(feature = "serde")]
impl PevcapRequestTargetJson {
    const fn into_target(self) -> RequestTarget {
        match self {
            Self::Local => RequestTarget::Local,
            Self::VescCanController { controller_id } => RequestTarget::VescCanController {
                controller_id: VescControllerId::new(controller_id),
            },
        }
    }
}

#[cfg(feature = "serde")]
#[derive(Clone, Copy, Deserialize, Serialize)]
enum PevcapDirectionJson {
    LinkUp,
    LinkDown,
    Inbound,
    Outbound,
}

#[cfg(feature = "serde")]
impl From<PevcapDirection> for PevcapDirectionJson {
    fn from(direction: PevcapDirection) -> Self {
        match direction {
            PevcapDirection::LinkUp => Self::LinkUp,
            PevcapDirection::LinkDown => Self::LinkDown,
            PevcapDirection::Inbound => Self::Inbound,
            PevcapDirection::Outbound => Self::Outbound,
        }
    }
}

#[cfg(feature = "serde")]
impl PevcapDirectionJson {
    const fn into_direction(self) -> PevcapDirection {
        match self {
            Self::LinkUp => PevcapDirection::LinkUp,
            Self::LinkDown => PevcapDirection::LinkDown,
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
    use crate::{NotificationByteLen, VerificationStatus};
    use proptest::prelude::*;
    use std::{cell::RefCell, rc::Rc};

    const fn ms(value: u64) -> MonotonicMillis {
        MonotonicMillis::new(value)
    }

    const fn wc(value: u64) -> WallClockUnixMillis {
        WallClockUnixMillis::new(value)
    }

    const fn write_len(value: u16) -> TransportWriteLen {
        TransportWriteLen::new(value)
    }

    #[derive(Clone, Default)]
    struct RecordingSession {
        bytes: Rc<RefCell<Vec<u8>>>,
    }

    impl ProtocolSession for RecordingSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(link) => {
                    output.push(SessionOutput::Event(DeviceEvent::LinkUp(link)));
                }
                SessionInput::LinkDown => output.push(SessionOutput::Event(DeviceEvent::LinkDown)),
                SessionInput::Notification {
                    channel,
                    bytes,
                    monotonic_ms,
                } => {
                    self.bytes.borrow_mut().extend_from_slice(bytes);
                    output.push(SessionOutput::NotificationIngest(
                        crate::NotificationIngestOutcome::ignored_wrong_channel(
                            channel,
                            NotificationByteLen::new(bytes.len()),
                            monotonic_ms,
                        ),
                    ));
                }
                SessionInput::Tick { monotonic_ms } => {
                    output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                }
                SessionInput::Command(_) => {}
            }
        }
    }

    fn replay_outputs(capture: &PevcapCapture, mode: PevcapReplayMode<'_>) -> Vec<SessionOutput> {
        let recorder = RecordingSession::default();
        let mut host = HostSession::new(recorder);
        let mut outputs = Vec::new();
        capture.replay_mode_into_host(mode, &mut host, &mut outputs);
        outputs
    }

    fn replayed_bytes(capture: &PevcapCapture, mode: PevcapReplayMode<'_>) -> Vec<u8> {
        let recorder = RecordingSession::default();
        let bytes = Rc::clone(&recorder.bytes);
        let mut host = HostSession::new(recorder);
        let mut outputs = Vec::new();
        capture.replay_mode_into_host(mode, &mut host, &mut outputs);
        bytes.borrow().clone()
    }

    #[test]
    fn pevcap_current_version_and_magic_are_stable() {
        assert_eq!(PEVCAP_MAGIC, *b"PEVCAP\0\0");
        assert_eq!(
            PevcapFormatVersion::current(),
            PevcapFormatVersion { major: 1, minor: 0 }
        );
    }

    #[test]
    fn capture_session_labels_match_hardware_corpus_taxonomy() {
        assert_eq!(
            CaptureSessionLabel::ALL.map(CaptureSessionLabel::slug),
            [
                "powered_on_stationary",
                "rolling_forward",
                "rolling_backward",
                "lifted_wheel",
                "charging",
                "headlight_toggled",
                "horn",
                "ride_mode_change",
                "alarm_change",
                "bms_screen",
                "disconnect_reconnect",
                "power_cycle",
            ]
        );
    }

    #[test]
    fn capture_session_label_annotations_are_stable_pevcap_metadata() {
        let label = CaptureSessionLabel::Charging.annotation();
        let header = PevcapHeader::new(
            wc(1_725_000_000_000),
            "darwin",
            None,
            &[],
            &[],
            None,
            "0.1.0",
            [0; 32],
            &[label.as_str()],
        )
        .expect("header should validate");

        assert_eq!(label, "capture_label=charging");
        assert_eq!(header.annotations.as_slice(), &[label]);
    }

    #[test]
    fn capture_privacy_distribution_and_evidence_annotations_are_stable() {
        let annotations = [
            CapturePrivacy::Private.annotation(),
            CapturePrivacy::Redacted.annotation(),
            CaptureDistribution::Redistributable.annotation(),
            CaptureEvidence::HardwareTested.annotation(),
            CaptureEvidence::Inferred.annotation(),
            CaptureEvidence::Unverified.annotation(),
        ];

        assert_eq!(
            annotations,
            [
                "capture_privacy=private",
                "capture_privacy=redacted",
                "capture_distribution=redistributable",
                "capture_evidence=hardware_tested",
                "capture_evidence=inferred",
                "capture_evidence=unverified",
            ]
        );
    }

    #[test]
    fn capture_privacy_provenance_annotations_are_pevcap_metadata() {
        let label = CaptureSessionLabel::PoweredOnStationary.annotation();
        let privacy = CapturePrivacy::Redacted.annotation();
        let distribution = CaptureDistribution::Redistributable.annotation();
        let evidence = CaptureEvidence::HardwareTested.annotation();
        let header = PevcapHeader::new(
            wc(1_725_000_000_000),
            "darwin",
            None,
            &[],
            &[],
            None,
            "0.1.0",
            [0; 32],
            &[
                label.as_str(),
                privacy.as_str(),
                distribution.as_str(),
                evidence.as_str(),
            ],
        )
        .expect("header should validate");

        assert_eq!(
            header.annotations.as_slice(),
            &[label, privacy, distribution, evidence]
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
            wc(1_725_000_000_000),
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

        assert_eq!(header.wall_clock_start_unix_ms, wc(1_725_000_000_000));
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
    fn pevcap_header_accepts_live_aero_gatt_inventory_size() {
        let service = GattChannel::from_bytes([0xFE; 16]);
        let mut gatt = Vec::new();
        for index in 0_u8..10 {
            gatt.push(GattFingerprint {
                service,
                characteristic: GattChannel::from_bytes([index; 16]),
                roles: GattRoles::empty().with_read(),
                verification: VerificationStatus::HardwareVerified,
            });
        }

        let header = PevcapHeader::new(
            wc(1_725_000_000_000),
            "8de871ff-6aa1-a767-34dd-608e584b610e",
            Some(185),
            &[service],
            &gatt,
            Some(PevcapResolvedIdentity {
                protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
                model: Some(VerifiedValue {
                    value: "NF2557".to_owned(),
                    verification: VerificationStatus::HardwareVerified,
                }),
                firmware: None,
            }),
            "0.1.0",
            [0xAB; 32],
            &["capture_label=powered_on_stationary"],
        )
        .expect("live Aero GATT inventory should fit in PEVCAP");

        assert_eq!(header.gatt_fingerprints.len(), 10);
    }

    #[test]
    fn pevcap_header_rejects_oversized_annotations() {
        let annotations = ["note"; PEVCAP_MAX_ANNOTATIONS + 1];
        let error = PevcapHeader::new(
            wc(0),
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
            ms(7),
            characteristic,
            WriteMode::WithoutResponse,
            Bytes::from_static(&[0x01, 0x23, 0xab]),
        );
        let notification = PevcapRecord::inbound_notification(
            ms(9),
            characteristic,
            service,
            Bytes::from_static(&[0xde, 0xad, 0xbe, 0xef]),
        );

        assert_eq!(write.direction, PevcapDirection::Outbound);
        assert_eq!(write.characteristic, characteristic);
        assert_eq!(write.service, None);
        assert_eq!(write.write_mode, Some(WriteMode::WithoutResponse));
        assert_eq!(write.bytes.as_ref(), &[0x01, 0x23, 0xab]);
        assert_eq!(notification.direction, PevcapDirection::Inbound);
        assert_eq!(notification.characteristic, characteristic);
        assert_eq!(notification.service, Some(service));
        assert_eq!(notification.write_mode, None);
        assert_eq!(notification.bytes.as_ref(), &[0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn pevcap_records_preserve_optional_request_target() {
        let characteristic = GattChannel::from_bytes([0x33; 16]);
        let target = RequestTarget::VescCanController {
            controller_id: VescControllerId::new(7),
        };
        let write = PevcapRecord::targeted_outbound_write(
            ms(7),
            characteristic,
            WriteMode::WithoutResponse,
            Bytes::from_static(&[0x01, 0x23]),
            target,
        );

        assert_eq!(write.target, Some(target));
        assert_eq!(write.direction, PevcapDirection::Outbound);
        assert_eq!(write.write_mode, Some(WriteMode::WithoutResponse));
    }

    #[test]
    fn pevcap_capture_wraps_header_and_records() {
        let header = PevcapHeader::new(
            wc(1),
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
            ms(1),
            GattChannel::from_bytes([0x55; 16]),
            WriteMode::WithResponse,
            Bytes::from_static(&[0x10]),
        )];

        let capture = PevcapCapture::new(header.clone(), records.clone());

        assert_eq!(capture.version, PevcapFormatVersion::current());
        assert_eq!(capture.header, header);
        assert_eq!(capture.records, records);
    }

    #[test]
    fn pevcap_capture_replays_inbound_notifications_into_host() {
        let service = GattChannel::from_bytes([0x44; 16]);
        let characteristic = GattChannel::from_bytes([0x55; 16]);
        let header = PevcapHeader::new(
            wc(1),
            "darwin",
            Some(23),
            &[service],
            &[],
            None,
            "0.1.0",
            [0x11; 32],
            &[],
        )
        .expect("header should validate");
        let capture = PevcapCapture::new(
            header,
            vec![
                PevcapRecord::outbound_write(
                    ms(7),
                    characteristic,
                    WriteMode::WithoutResponse,
                    Bytes::from_static(b"N"),
                ),
                PevcapRecord::inbound_notification(
                    ms(9),
                    characteristic,
                    service,
                    Bytes::from_static(b"NAME=Falcon"),
                ),
                PevcapRecord::inbound_notification(
                    ms(11),
                    characteristic,
                    service,
                    Bytes::from_static(b"55aa"),
                ),
            ],
        );

        let outputs = replay_outputs(&capture, PevcapReplayMode::Whole);
        assert_eq!(capture.replay_input_count(), 3);
        assert!(matches!(
            outputs[0],
            SessionOutput::Event(DeviceEvent::LinkUp(LinkInfo {
                monotonic_ms,
                max_write_len: Some(max_write_len),
            })) if monotonic_ms == ms(0) && max_write_len == write_len(23)
        ));
        assert!(matches!(
            outputs[1],
            SessionOutput::NotificationIngest(crate::NotificationIngestOutcome::Ignored(evidence))
                if evidence.monotonic_ms == ms(9)
        ));
        assert!(matches!(
            outputs[2],
            SessionOutput::NotificationIngest(crate::NotificationIngestOutcome::Ignored(evidence))
                if evidence.monotonic_ms == ms(11)
        ));
        assert_eq!(
            replayed_bytes(&capture, PevcapReplayMode::Whole),
            b"NAME=Falcon55aa"
        );
    }

    #[test]
    fn pevcap_capture_preserves_explicit_link_lifecycle_replay_order() {
        let service = GattChannel::from_bytes([0x44; 16]);
        let characteristic = GattChannel::from_bytes([0x55; 16]);
        let header = PevcapHeader::new(
            wc(1),
            "darwin",
            Some(23),
            &[service],
            &[],
            None,
            "0.1.0",
            [0x11; 32],
            &[],
        )
        .expect("header should validate");
        let capture = PevcapCapture::new(
            header,
            vec![
                PevcapRecord::link_up(ms(5), Some(23)),
                PevcapRecord::inbound_notification(
                    ms(9),
                    characteristic,
                    service,
                    Bytes::from_static(b"NAME=Falcon"),
                ),
                PevcapRecord::link_down(ms(12)),
                PevcapRecord::link_up(ms(20), Some(23)),
                PevcapRecord::inbound_notification(
                    ms(21),
                    characteristic,
                    service,
                    Bytes::from_static(b"55aa"),
                ),
            ],
        );

        let outputs = replay_outputs(&capture, PevcapReplayMode::Whole);
        assert_eq!(capture.replay_input_count(), 5);
        assert!(matches!(
            outputs[0],
            SessionOutput::Event(DeviceEvent::LinkUp(LinkInfo {
                monotonic_ms,
                max_write_len: Some(max_write_len),
            })) if monotonic_ms == ms(5) && max_write_len == write_len(23)
        ));
        assert!(matches!(
            outputs[2],
            SessionOutput::Event(DeviceEvent::LinkDown)
        ));
        assert!(matches!(
            outputs[3],
            SessionOutput::Event(DeviceEvent::LinkUp(LinkInfo {
                monotonic_ms,
                max_write_len: Some(max_write_len),
            })) if monotonic_ms == ms(20) && max_write_len == write_len(23)
        ));
        assert_eq!(
            replayed_bytes(&capture, PevcapReplayMode::Whole),
            b"NAME=Falcon55aa"
        );
    }

    #[test]
    fn pevcap_capture_without_inbound_records_has_only_link_replay_input() {
        let characteristic = GattChannel::from_bytes([0x55; 16]);
        let header =
            PevcapHeader::new(wc(1), "darwin", None, &[], &[], None, "0.1.0", [0; 32], &[])
                .expect("header should validate");
        let capture = PevcapCapture::new(
            header,
            vec![PevcapRecord::outbound_write(
                ms(7),
                characteristic,
                WriteMode::WithoutResponse,
                Bytes::from_static(b"N"),
            )],
        );

        let outputs = replay_outputs(&capture, PevcapReplayMode::Whole);
        assert_eq!(capture.replay_input_count(), 1);
        assert!(matches!(
            outputs.as_slice(),
            [SessionOutput::Event(DeviceEvent::LinkUp(LinkInfo {
                monotonic_ms,
                max_write_len: None,
            }))] if *monotonic_ms == ms(0)
        ));
    }

    #[test]
    fn pevcap_capture_replay_records_can_split_notifications_to_single_bytes() {
        let characteristic = GattChannel::from_bytes([0x55; 16]);
        let header = PevcapHeader::new(
            wc(1),
            "darwin",
            Some(128),
            &[],
            &[],
            None,
            "0.1.0",
            [0; 32],
            &[],
        )
        .expect("header should validate");
        let capture = PevcapCapture::new(
            header,
            vec![PevcapRecord::inbound_notification(
                ms(9),
                characteristic,
                characteristic,
                Bytes::from_static(b"abc"),
            )],
        );

        let outputs = replay_outputs(&capture, PevcapReplayMode::OneByte);
        assert_eq!(outputs.len(), 4);
        assert_eq!(replayed_bytes(&capture, PevcapReplayMode::OneByte), b"abc");
        assert!(outputs[1..].iter().all(|output| {
            matches!(
                output,
                SessionOutput::NotificationIngest(
                    crate::NotificationIngestOutcome::Ignored(evidence)
                ) if evidence.len == NotificationByteLen::new(1)
            )
        }));
    }

    #[test]
    fn pevcap_capture_replay_records_can_apply_arbitrary_notification_chunks() {
        let characteristic = GattChannel::from_bytes([0x66; 16]);
        let header = PevcapHeader::new(
            wc(1),
            "darwin",
            Some(128),
            &[],
            &[],
            None,
            "0.1.0",
            [0; 32],
            &[],
        )
        .expect("header should validate");
        let capture = PevcapCapture::new(
            header,
            vec![PevcapRecord::inbound_notification(
                ms(9),
                characteristic,
                characteristic,
                Bytes::from_static(b"abcd"),
            )],
        );

        let lengths = [NotificationChunkLen::new(2), NotificationChunkLen::new(1)];
        let outputs = replay_outputs(&capture, PevcapReplayMode::Lengths(&lengths));
        assert_eq!(outputs.len(), 4);
        assert_eq!(
            replayed_bytes(&capture, PevcapReplayMode::Lengths(&lengths)),
            b"abcd"
        );
        assert!(matches!(
            outputs[1],
            SessionOutput::NotificationIngest(crate::NotificationIngestOutcome::Ignored(evidence))
                if evidence.len == NotificationByteLen::new(2)
        ));
    }

    proptest! {
        #[test]
        fn pevcap_arbitrary_notification_chunks_preserve_replay_payloads(
            payload in proptest::collection::vec(any::<u8>(), 1..64),
            lengths in proptest::collection::vec(0usize..12, 0..16),
        ) {
            let characteristic = GattChannel::from_bytes([0x77; 16]);
            let header = PevcapHeader::new(
            wc(1),
                "darwin",
                Some(128),
                &[],
                &[],
                None,
                "0.1.0",
                [0; 32],
                &[],
            )
            .expect("header should validate");
            let capture = PevcapCapture::new(
                header,
                vec![PevcapRecord::inbound_notification(ms(9),
                    characteristic,
                    characteristic,
                    payload.clone(),
                )],
            );

            let chunk_lengths = lengths
                .into_iter()
                .map(NotificationChunkLen::new)
                .collect::<Vec<_>>();

            prop_assert_eq!(
                replayed_bytes(&capture, PevcapReplayMode::Lengths(&chunk_lengths)),
                payload,
            );
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_jsonl_round_trips_header_and_ordered_records() {
        let service = GattChannel::from_bytes([0xFE; 16]);
        let characteristic = GattChannel::from_bytes([0xE1; 16]);
        let can_target = RequestTarget::VescCanController {
            controller_id: VescControllerId::new(7),
        };
        let header = PevcapHeader::new(
            wc(1_725_000_123_456),
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
                PevcapRecord::targeted_outbound_write(
                    ms(7),
                    characteristic,
                    WriteMode::WithoutResponse,
                    Bytes::from_static(b"N"),
                    can_target,
                ),
                PevcapRecord::inbound_notification(
                    ms(9),
                    characteristic,
                    service,
                    Bytes::from_static(b"NAME=Falcon"),
                ),
            ],
        );

        let jsonl = capture.to_jsonl().expect("capture serializes");
        let decoded = PevcapCapture::from_jsonl(&jsonl).expect("capture deserializes");

        assert_eq!(decoded, capture);
        assert_eq!(decoded.records[0].target, Some(can_target));
        assert_eq!(jsonl.lines().count(), 3);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_jsonl_round_trips_link_lifecycle_records() {
        let header = PevcapHeader::new(
            wc(1),
            "darwin",
            Some(23),
            &[],
            &[],
            None,
            "0.1.0",
            [0x42; 32],
            &[],
        )
        .expect("header should validate");
        let capture = PevcapCapture::new(
            header,
            vec![
                PevcapRecord::link_up(ms(5), Some(23)),
                PevcapRecord::link_down(ms(12)),
            ],
        );

        let jsonl = capture.to_jsonl().expect("capture serializes");
        let decoded = PevcapCapture::from_jsonl(&jsonl).expect("capture deserializes");

        assert_eq!(decoded, capture);
        assert!(jsonl.contains(r#""direction":"LinkUp""#));
        assert!(jsonl.contains(r#""direction":"LinkDown""#));
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

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_jsonl_rejects_malformed_inbound_record_before_replay() {
        let source = sample_pevcap_capture()
            .to_jsonl()
            .expect("capture serializes");
        let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
        lines[2] = lines[2].replace(
            r#""service":[254,254,254,254,254,254,254,254,254,254,254,254,254,254,254,254]"#,
            r#""service":null"#,
        );
        let jsonl = lines.join("\n");

        let err = PevcapCapture::from_jsonl(&jsonl)
            .expect_err("inbound notification without service is malformed");

        assert!(matches!(
            err,
            PevcapJsonlError::Record {
                line: 3,
                source: PevcapRecordError::MissingInboundService,
            }
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_round_trips_header_and_ordered_records() {
        let mut capture = sample_pevcap_capture();
        let target = RequestTarget::VescCanController {
            controller_id: VescControllerId::new(9),
        };
        capture.records[0].target = Some(target);

        let binary = capture.to_binary().expect("capture serializes");
        let decoded = PevcapCapture::from_binary(&binary).expect("capture deserializes");

        assert!(binary.starts_with(&PEVCAP_MAGIC));
        assert_eq!(decoded, capture);
        assert_eq!(decoded.records[0].target, Some(target));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_rejects_malformed_record_payload_before_replay() {
        let mut capture = sample_pevcap_capture();
        capture.records[1].service = None;

        let binary = capture.to_binary().expect("malformed capture serializes");
        let error = PevcapCapture::from_binary(&binary).expect_err("record should be rejected");

        assert!(matches!(
            error,
            PevcapBinaryError::Record(PevcapRecordError::MissingInboundService)
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_rejects_bad_magic() {
        let mut binary = sample_pevcap_capture()
            .to_binary()
            .expect("capture serializes");
        if let Some(first) = binary.first_mut() {
            *first = b'X';
        }

        let error = PevcapCapture::from_binary(&binary).expect_err("magic should be rejected");

        assert!(matches!(error, PevcapBinaryError::InvalidMagic));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_rejects_unsupported_version() {
        let mut binary = sample_pevcap_capture()
            .to_binary()
            .expect("capture serializes");
        let major_start = PEVCAP_MAGIC.len();
        let major_end = major_start + 2;
        binary
            .splice(major_start..major_end, 2_u16.to_le_bytes())
            .for_each(drop);

        let error = PevcapCapture::from_binary(&binary).expect_err("version should be rejected");

        assert!(matches!(
            error,
            PevcapBinaryError::UnsupportedVersion {
                version: PevcapFormatVersion { major: 2, minor: 0 }
            }
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_rejects_truncated_header() {
        let mut binary = sample_pevcap_capture()
            .to_binary()
            .expect("capture serializes");
        let header_len_start = PEVCAP_MAGIC.len() + 4;
        let header_len_end = header_len_start + PEVCAP_BINARY_LENGTH_PREFIX_BYTES;
        binary
            .splice(header_len_start..header_len_end, 4_u32.to_le_bytes())
            .for_each(drop);
        binary.truncate(header_len_end + 2);

        let error = PevcapCapture::from_binary(&binary).expect_err("header should be truncated");

        assert!(matches!(
            error,
            PevcapBinaryError::Truncated {
                section: PevcapBinarySection::Header
            }
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_rejects_truncated_record() {
        let mut binary = sample_pevcap_capture()
            .to_binary()
            .expect("capture serializes");
        let _last = binary.pop();

        let error = PevcapCapture::from_binary(&binary).expect_err("record should be truncated");

        assert!(matches!(
            error,
            PevcapBinaryError::Truncated {
                section: PevcapBinarySection::Record
            }
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_rejects_trailing_bytes() {
        let mut binary = sample_pevcap_capture()
            .to_binary()
            .expect("capture serializes");
        binary.push(0xAA);

        let error = PevcapCapture::from_binary(&binary).expect_err("trailing byte should fail");

        assert!(matches!(error, PevcapBinaryError::TrailingBytes { len: 1 }));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_binary_rejects_oversized_bounded_header_data() {
        let mut output = Vec::new();
        output.extend_from_slice(&PEVCAP_MAGIC);
        write_u16_le(&mut output, PEVCAP_VERSION_MAJOR);
        write_u16_le(&mut output, PEVCAP_VERSION_MINOR);
        let header = PevcapHeaderJson {
            wall_clock_start_unix_ms: 1,
            platform_id: "darwin".to_owned(),
            write_limit: None,
            advertised_services: Vec::new(),
            gatt_fingerprints: Vec::new(),
            resolved_identity: None,
            library_version: "0.1.0".to_owned(),
            registry_hash: [0x00; 32],
            annotations: vec!["note".to_owned(); PEVCAP_MAX_ANNOTATIONS + 1],
        };
        let payload = serde_json::to_vec(&header).expect("header serializes");
        write_len_prefixed(&mut output, PevcapBinarySection::Header, &payload)
            .expect("length should fit");
        write_u32_le(&mut output, 0);

        let error = PevcapCapture::from_binary(&output).expect_err("header bounds should fail");

        assert!(matches!(
            error,
            PevcapBinaryError::Header(PevcapHeaderError::TooManyItems {
                field: PevcapHeaderField::Annotations,
                len,
                max: PEVCAP_MAX_ANNOTATIONS
            }) if len == PEVCAP_MAX_ANNOTATIONS + 1
        ));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_codec_dispatches_jsonl_round_trip() {
        let capture = sample_pevcap_capture();

        let encoded = capture
            .encode(PevcapEncoding::Jsonl)
            .expect("JSONL encodes");
        let decoded =
            PevcapCapture::decode(&encoded, PevcapEncoding::Jsonl).expect("JSONL decodes");

        assert_eq!(decoded, capture);
        assert!(encoded.starts_with(br#"{"kind":"header""#));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_codec_dispatches_binary_round_trip() {
        let capture = sample_pevcap_capture();

        let encoded = capture
            .encode(PevcapEncoding::Binary)
            .expect("binary encodes");
        let decoded =
            PevcapCapture::decode(&encoded, PevcapEncoding::Binary).expect("binary decodes");

        assert_eq!(decoded, capture);
        assert!(encoded.starts_with(&PEVCAP_MAGIC));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pevcap_codec_rejects_non_utf8_jsonl_bytes() {
        let error = PevcapCapture::decode(&[0xff], PevcapEncoding::Jsonl)
            .expect_err("JSONL input must be UTF-8");

        assert!(matches!(error, PevcapCodecError::Utf8(_)));
    }

    #[cfg(feature = "serde")]
    fn sample_pevcap_capture() -> PevcapCapture {
        let service = GattChannel::from_bytes([0xFE; 16]);
        let characteristic = GattChannel::from_bytes([0xE1; 16]);
        let header = PevcapHeader::new(
            wc(1_725_000_123_456),
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
        PevcapCapture::new(
            header,
            vec![
                PevcapRecord::outbound_write(
                    ms(7),
                    characteristic,
                    WriteMode::WithoutResponse,
                    Bytes::from_static(b"N"),
                ),
                PevcapRecord::inbound_notification(
                    ms(9),
                    characteristic,
                    service,
                    Bytes::from_static(b"NAME=Falcon"),
                ),
            ],
        )
    }
}
