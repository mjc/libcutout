use std::fmt::{self, Write as _};

use bytes::Bytes;
use cutout_core::{
    NotificationByteLen, PevcapCapture, PevcapHeader, PevcapHeaderError, PevcapRecord,
    TransportWriteLimit, WriteMode,
};
use futures_util::StreamExt;
use uuid::Uuid;

use crate::{
    BtleError, CharacteristicSummary, ConnectionSummary, MonotonicMs, NegotiatedWriteLimit,
    NotificationWindow, SessionBridgeReport, SessionPeripheral, WriteProvenance,
    gatt::gatt_channel_from_uuid, types::characteristic_from_summary,
};

const MAX_BTLE_ATTRIBUTE_VALUE_LEN: usize = 512;

/// Captured BTLE bytes tagged as either a valid attribute value or malformed input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapturedBtlePacket {
    /// Valid BTLE ATT attribute value.
    AttributeValue(BtleAttributeValue),

    /// Malformed BTLE bytes retained only for explicit capture/provenance.
    Malformed(MalformedBtlePacket),
}

impl CapturedBtlePacket {
    /// Classifies raw bytes from a BTLE attribute value while preserving them.
    #[must_use]
    pub fn from_raw_bytes(bytes: impl Into<Bytes>) -> Self {
        Self::from_bytes(bytes.into())
    }

    #[must_use]
    fn from_bytes(bytes: Bytes) -> Self {
        match BtleAttributeValue::try_from_bytes(bytes) {
            Ok(value) => Self::AttributeValue(value),
            Err(malformed) => Self::Malformed(malformed),
        }
    }

    /// Returns the original captured bytes for explicit capture/provenance edges.
    #[must_use]
    pub fn as_raw_bytes(&self) -> &[u8] {
        match self {
            Self::AttributeValue(value) => value.as_raw_bytes(),
            Self::Malformed(packet) => packet.as_raw_bytes(),
        }
    }

    /// Reconstructs the original raw byte buffer.
    #[must_use]
    pub fn into_raw_bytes(self) -> Bytes {
        match self {
            Self::AttributeValue(value) => value.into_raw_bytes(),
            Self::Malformed(packet) => packet.into_raw_bytes(),
        }
    }

    /// Returns the captured byte length.
    #[must_use]
    pub fn len(&self) -> NotificationByteLen {
        NotificationByteLen::from_bytes(self.as_raw_bytes().len())
    }

    /// Returns true when no bytes were captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_raw_bytes().is_empty()
    }

    /// Returns true when the capture boundary classified the bytes as malformed.
    #[must_use]
    pub const fn is_malformed(&self) -> bool {
        matches!(self, Self::Malformed(_))
    }

    /// Reconstructs the original raw bytes as a cheap shared buffer clone.
    #[must_use]
    pub fn to_raw_bytes(&self) -> Bytes {
        match self {
            Self::AttributeValue(value) => value.bytes.clone(),
            Self::Malformed(packet) => packet.bytes.clone(),
        }
    }
}

impl From<Bytes> for CapturedBtlePacket {
    fn from(bytes: Bytes) -> Self {
        Self::from_bytes(bytes)
    }
}

/// Valid BTLE ATT attribute-value bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BtleAttributeValue {
    bytes: Bytes,
}

impl BtleAttributeValue {
    fn try_from_bytes(bytes: Bytes) -> Result<Self, MalformedBtlePacket> {
        if bytes.len() <= MAX_BTLE_ATTRIBUTE_VALUE_LEN {
            Ok(Self { bytes })
        } else {
            Err(MalformedBtlePacket {
                bytes,
                reason: MalformedBtlePacketReason::OversizedAttributeValue {
                    max: NotificationByteLen::from_bytes(MAX_BTLE_ATTRIBUTE_VALUE_LEN),
                },
            })
        }
    }

    /// Returns the original captured bytes for explicit capture/provenance edges.
    #[must_use]
    pub fn as_raw_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// Reconstructs the original raw byte buffer.
    #[must_use]
    pub fn into_raw_bytes(self) -> Bytes {
        self.bytes
    }
}

/// Malformed BTLE bytes retained only for explicit capture/provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MalformedBtlePacket {
    bytes: Bytes,
    reason: MalformedBtlePacketReason,
}

impl MalformedBtlePacket {
    /// Returns why the bytes are not valid BTLE capture input.
    #[must_use]
    pub const fn reason(&self) -> MalformedBtlePacketReason {
        self.reason
    }

    /// Returns the original captured bytes for explicit capture/provenance edges.
    #[must_use]
    pub fn as_raw_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// Reconstructs the original raw byte buffer.
    #[must_use]
    pub fn into_raw_bytes(self) -> Bytes {
        self.bytes
    }
}

/// Malformed BTLE capture classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MalformedBtlePacketReason {
    /// Attribute value exceeded the BTLE ATT maximum of 512 bytes.
    OversizedAttributeValue {
        /// Maximum valid attribute value length.
        max: NotificationByteLen,
    },
}

/// Captured bridge records suitable for live BTLE evidence files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionCaptureRecord {
    /// Link-up metadata observed before protocol outputs were processed.
    Link {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Negotiated maximum write length, when known.
        max_write_len: Option<NegotiatedWriteLimit>,
    },

    /// Link-down event observed after the session requested disconnect.
    LinkDown {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,
    },

    /// Notification subscription issued by the session.
    Subscribe {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Concrete characteristic subscribed through BTLE.
        characteristic: Uuid,
    },

    /// Outbound write issued by the session.
    Write {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Concrete characteristic written through BTLE.
        characteristic: Uuid,

        /// Write mode requested by the protocol session.
        mode: WriteMode,

        /// Captured bytes sent to the characteristic.
        bytes: CapturedBtlePacket,

        /// Whether the bytes come from provisional protocol encoders.
        provenance: WriteProvenance,
    },

    /// Inbound notification observed from the device.
    Notification {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Characteristic that emitted the notification.
        characteristic: Uuid,

        /// Service associated with the notification.
        service: Uuid,

        /// Captured bytes received from the device.
        bytes: CapturedBtlePacket,
    },
}

impl fmt::Display for SessionCaptureRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Link {
                monotonic_ms,
                max_write_len: Some(max_write_len),
            } => write!(f, "link t_ms={monotonic_ms} max_write_len={max_write_len}"),
            Self::Link {
                monotonic_ms,
                max_write_len: None,
            } => write!(f, "link t_ms={monotonic_ms} max_write_len=<unknown>"),
            Self::LinkDown { monotonic_ms } => {
                write!(f, "link_down t_ms={monotonic_ms}")
            }
            Self::Subscribe {
                monotonic_ms,
                characteristic,
            } => write!(
                f,
                "subscribe t_ms={monotonic_ms} characteristic={characteristic}"
            ),
            Self::Write {
                monotonic_ms,
                characteristic,
                mode,
                bytes,
                provenance,
            } => {
                write!(
                    f,
                    "write t_ms={monotonic_ms} characteristic={characteristic} mode={} bytes=",
                    format_write_mode(*mode),
                )?;
                write_hex(f, bytes.as_raw_bytes())?;
                write_packet_status(f, bytes)?;
                write!(f, " provisional={}", provenance.is_provisional())
            }
            Self::Notification {
                monotonic_ms,
                characteristic,
                service,
                bytes,
            } => {
                write!(
                    f,
                    "notification t_ms={monotonic_ms} characteristic={characteristic} service={service} bytes="
                )?;
                write_hex(f, bytes.as_raw_bytes())?;
                write_packet_status(f, bytes)
            }
        }
    }
}

/// Captured records plus bridge counters from a session run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionCapture {
    /// Records emitted during the bridge run.
    pub records: Vec<SessionCaptureRecord>,

    /// Aggregate bridge counters.
    pub report: SessionBridgeReport,
}

/// Protocol-agnostic raw notification record captured from a subscribed
/// characteristic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawNotificationRecord {
    /// Monotonic capture time relative to the start of the raw subscription.
    pub monotonic_ms: MonotonicMs,

    /// Characteristic UUID that emitted the notification.
    pub characteristic: Uuid,

    /// Service UUID associated with the notification.
    pub service: Uuid,

    /// Captured notification bytes.
    pub bytes: CapturedBtlePacket,
}

/// Caller-supplied metadata for converting a live BTLE session capture into
/// PEVCAP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapSessionMetadata<'a> {
    /// Wall-clock capture start time in Unix milliseconds.
    pub wall_clock_start_unix_ms: cutout_core::WallClockUnixTimestamp,

    /// Platform identifier recorded by the capture producer.
    pub platform_id: &'a str,

    /// Version of the Cutout library or binary that produced the capture.
    pub library_version: &'a str,

    /// Registry hash used while producing the capture.
    pub registry_hash: [u8; 32],

    /// Resolved identity used while producing the capture, when known.
    pub resolved_identity: Option<cutout_core::PevcapResolvedIdentity>,

    /// Human annotations attached to the capture.
    pub annotations: &'a [&'a str],
}

/// Subscribes to a notify/indicate characteristic and records raw notification chunks.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe or
/// notification streaming fails.
pub async fn capture_raw_notifications<P>(
    peripheral: &P,
    characteristic: &CharacteristicSummary,
    notification_window: NotificationWindow,
) -> Result<Vec<RawNotificationRecord>, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
{
    let characteristic = characteristic_from_summary(characteristic);
    peripheral.subscribe(&characteristic).await?;
    if notification_window.is_zero() {
        return Ok(Vec::new());
    }

    let mut notifications = peripheral.notifications().await?;
    let started_at = tokio::time::Instant::now();
    let deadline = started_at + notification_window.as_duration();
    let mut records = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(notification)) if notification.characteristic == characteristic.uuid => {
                records.push(RawNotificationRecord {
                    monotonic_ms: MonotonicMs::from_elapsed_millis(
                        started_at.elapsed().as_millis(),
                    ),
                    characteristic: notification.characteristic,
                    service: notification.service,
                    bytes: notification.bytes,
                });
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }

    Ok(records)
}

impl SessionCapture {
    /// Converts this live BTLE bridge capture into a PEVCAP envelope.
    ///
    /// Link and subscribe records are represented in the PEVCAP header or GATT
    /// metadata; outbound writes and inbound notifications become ordered
    /// PEVCAP transport records.
    ///
    /// # Errors
    ///
    /// Returns [`PevcapHeaderError`] if observed metadata exceeds PEVCAP
    /// bounds.
    pub fn to_pevcap(
        &self,
        summary: &ConnectionSummary,
        metadata: PevcapSessionMetadata<'_>,
    ) -> Result<PevcapCapture, PevcapHeaderError> {
        let advertised_services = summary
            .observation
            .advertised_services
            .iter()
            .copied()
            .map(gatt_channel_from_uuid)
            .collect::<Vec<_>>();
        let gatt_fingerprints = summary.gatt_fingerprints();
        let write_limit = self.records.iter().find_map(|record| match record {
            SessionCaptureRecord::Link { max_write_len, .. } => *max_write_len,
            SessionCaptureRecord::LinkDown { .. }
            | SessionCaptureRecord::Subscribe { .. }
            | SessionCaptureRecord::Write { .. }
            | SessionCaptureRecord::Notification { .. } => None,
        });
        let header = PevcapHeader::new(
            metadata.wall_clock_start_unix_ms,
            metadata.platform_id,
            write_limit.map(|len| TransportWriteLimit::from_bytes(len.as_bytes())),
            &advertised_services,
            &gatt_fingerprints,
            metadata.resolved_identity,
            metadata.library_version,
            metadata.registry_hash,
            metadata.annotations,
        )?;
        let records = self
            .records
            .iter()
            .filter_map(session_record_to_pevcap_record)
            .collect();

        Ok(PevcapCapture::new(header, records))
    }
}

pub(crate) fn session_record_monotonic_ms(record: &SessionCaptureRecord) -> MonotonicMs {
    match record {
        SessionCaptureRecord::Link { monotonic_ms, .. }
        | SessionCaptureRecord::LinkDown { monotonic_ms }
        | SessionCaptureRecord::Subscribe { monotonic_ms, .. }
        | SessionCaptureRecord::Write { monotonic_ms, .. }
        | SessionCaptureRecord::Notification { monotonic_ms, .. } => *monotonic_ms,
    }
}

fn session_record_to_pevcap_record(record: &SessionCaptureRecord) -> Option<PevcapRecord> {
    match record {
        SessionCaptureRecord::Link {
            monotonic_ms,
            max_write_len,
        } => Some(PevcapRecord::link_up(
            monotonic_ms.into_core(),
            max_write_len.map(|len| TransportWriteLimit::from_bytes(len.as_bytes())),
        )),
        SessionCaptureRecord::LinkDown { monotonic_ms } => {
            Some(PevcapRecord::link_down(monotonic_ms.into_core()))
        }
        SessionCaptureRecord::Write {
            monotonic_ms,
            characteristic,
            mode,
            bytes,
            provenance: _,
        } => Some(PevcapRecord::outbound_write(
            monotonic_ms.into_core(),
            gatt_channel_from_uuid(*characteristic),
            *mode,
            bytes.to_raw_bytes(),
        )),
        SessionCaptureRecord::Notification {
            monotonic_ms,
            characteristic,
            service,
            bytes,
        } => Some(PevcapRecord::inbound_notification(
            monotonic_ms.into_core(),
            gatt_channel_from_uuid(*characteristic),
            gatt_channel_from_uuid(*service),
            bytes.to_raw_bytes(),
        )),
        SessionCaptureRecord::Subscribe { .. } => None,
    }
}

fn format_write_mode(mode: WriteMode) -> &'static str {
    match mode {
        WriteMode::WithResponse => "with-response",
        WriteMode::WithoutResponse => "without-response",
    }
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes.iter().try_for_each(|byte| {
        f.write_char(char::from(HEX[usize::from(byte >> 4)]))?;
        f.write_char(char::from(HEX[usize::from(byte & 0x0f)]))
    })
}

fn write_packet_status(f: &mut fmt::Formatter<'_>, packet: &CapturedBtlePacket) -> fmt::Result {
    match packet {
        CapturedBtlePacket::AttributeValue(_) => Ok(()),
        CapturedBtlePacket::Malformed(malformed) => match malformed.reason() {
            MalformedBtlePacketReason::OversizedAttributeValue { max } => {
                write!(f, " btle=malformed_oversized_attribute_value max={max}")
            }
        },
    }
}
