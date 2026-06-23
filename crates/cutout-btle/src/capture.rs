use std::fmt::{self, Write as _};

use cutout_core::{
    DiagnosticError, FirmwareInfo, NotificationIngestOutcome, ParserDiagnostics, PevcapCapture,
    PevcapHeader, PevcapHeaderError, PevcapRecord, ReadOnlyResponse, SettingsReadback,
    TelemetryDelta, TelemetrySnapshot, WriteMode,
};
use cutout_protocols::{IdentityConfidence, IdentityEvidence};
use uuid::Uuid;

use crate::{ConnectionSummary, gatt_channel_from_uuid};

/// Report produced by a protocol bridge run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionBridgeReport {
    /// Protocol write actions emitted by the session before transport chunking.
    pub protocol_writes: usize,

    /// Transport writes executed through the bridge.
    pub writes: usize,

    /// Transport subscribe operations executed through the bridge.
    pub subscribes: usize,

    /// Notification payloads relayed into the session.
    pub notifications: usize,

    /// Total notification payload bytes relayed into the session.
    pub notification_bytes: usize,

    /// Length of the latest notification payload, if any were observed.
    pub latest_notification_len: Option<usize>,

    /// Semantic telemetry events emitted by the session.
    pub telemetry: usize,

    /// Latest semantic telemetry values emitted by the session.
    pub telemetry_snapshot: TelemetrySnapshot,

    /// Semantic read-only response events emitted by the session.
    pub read_only_responses: usize,

    /// Full read-only response payloads emitted by the session.
    pub read_only_response_events: Vec<ReadOnlyResponse>,

    /// Latest firmware readback emitted by the session.
    pub firmware: Option<FirmwareInfo>,

    /// Settings readbacks emitted by the session.
    pub settings: Vec<SettingsReadback>,

    /// Parser diagnostics events emitted by the session.
    pub diagnostics: usize,

    /// Aggregated parser diagnostic counters emitted by the session.
    pub diagnostics_snapshot: ParserDiagnostics,

    /// Detailed parser diagnostic errors emitted by the session.
    pub diagnostic_errors: Vec<DiagnosticError>,

    /// Staged identity resolution from non-actuating evidence.
    pub identity: Option<BridgeIdentityResolution>,

    /// Timestamped semantic events observed during the run.
    pub events: Vec<SessionBridgeEvent>,

    /// Transport disconnect operations executed through the bridge.
    pub disconnects: usize,
}

/// Staged identity resolution surfaced by a BTLE bridge run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeIdentityResolution {
    /// Resolved manufacturer, when model confidence was reached.
    pub manufacturer: Option<&'static str>,

    /// Resolved model, when model confidence was reached.
    pub model: Option<&'static str>,

    /// Confidence reported by staged identification.
    pub confidence: IdentityConfidence,

    /// Evidence that contributed to the decision.
    pub evidence: IdentityEvidence,
}

/// Timestamped semantic event emitted by the bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionBridgeEvent {
    /// Link-down event emitted by the protocol session after transport disconnect.
    LinkDown {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,
    },

    /// Decoded telemetry emitted by the protocol session.
    ProcessedTelemetry {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Telemetry delta emitted by the protocol session.
        delta: TelemetryDelta,
    },

    /// Read-only response emitted by the protocol session.
    ReadOnlyResponse {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Read-only response emitted by the protocol session.
        response: ReadOnlyResponse,
    },

    /// Parser diagnostics emitted by the protocol session.
    Diagnostics {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Parser diagnostic counters emitted at this timestamp.
        diagnostics: ParserDiagnostics,
    },

    /// Detailed parser diagnostic error emitted by the protocol session.
    DiagnosticError {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Detailed parser error emitted at this timestamp.
        error: DiagnosticError,
    },

    /// Typed protocol notification ingest outcome emitted by the session.
    NotificationIngest {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Protocol ingest outcome emitted at this timestamp.
        outcome: NotificationIngestOutcome,
    },
}

/// Captured bridge records suitable for live BTLE evidence files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionCaptureRecord {
    /// Link-up metadata observed before protocol outputs were processed.
    Link {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Negotiated maximum write length, when known.
        max_write_len: Option<u16>,
    },

    /// Link-down event observed after the session requested disconnect.
    LinkDown {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,
    },

    /// Notification subscription issued by the session.
    Subscribe {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Concrete characteristic subscribed through BTLE.
        characteristic: Uuid,
    },

    /// Outbound write issued by the session.
    Write {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Concrete characteristic written through BTLE.
        characteristic: Uuid,

        /// Write mode requested by the protocol session.
        mode: WriteMode,

        /// Exact bytes sent to the characteristic.
        bytes: Vec<u8>,

        /// Whether the bytes come from provisional protocol encoders.
        provisional: bool,
    },

    /// Inbound notification observed from the device.
    Notification {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: u64,

        /// Characteristic that emitted the notification.
        characteristic: Uuid,

        /// Service associated with the notification.
        service: Uuid,

        /// Exact bytes received from the device.
        bytes: Vec<u8>,
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
                provisional,
            } => {
                write!(
                    f,
                    "write t_ms={monotonic_ms} characteristic={characteristic} mode={} bytes=",
                    format_write_mode(*mode),
                )?;
                write_hex(f, bytes)?;
                write!(f, " provisional={provisional}")
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
                write_hex(f, bytes)
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

/// Reconnect capture plus per-link connection metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReconnectingSessionCapture {
    /// Aggregate records and report across connected links.
    pub capture: SessionCapture,

    /// Ordered diagnostics for each connected link attempt.
    pub attempts: Vec<ReconnectAttemptReport>,
}

/// Diagnostics captured for one reconnect link attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectAttemptReport {
    /// One-based link attempt number.
    pub attempt: usize,

    /// Connection summary observed for this link attempt.
    pub summary: ConnectionSummary,

    /// Bridge counters and lifecycle events observed during this link attempt.
    pub report: SessionBridgeReport,
}

/// Protocol-agnostic raw notification record captured from a subscribed
/// characteristic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawNotificationRecord {
    /// Monotonic capture time relative to the start of the raw subscription.
    pub monotonic_ms: u64,

    /// Characteristic UUID that emitted the notification.
    pub characteristic: Uuid,

    /// Service UUID associated with the notification.
    pub service: Uuid,

    /// Exact notification bytes.
    pub bytes: Vec<u8>,
}

/// Caller-supplied metadata for converting a live BTLE session capture into
/// PEVCAP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PevcapSessionMetadata<'a> {
    /// Wall-clock capture start time in Unix milliseconds.
    pub wall_clock_start_unix_ms: u64,

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
            write_limit,
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

pub(crate) fn session_record_monotonic_ms(record: &SessionCaptureRecord) -> u64 {
    match record {
        SessionCaptureRecord::Link { monotonic_ms, .. }
        | SessionCaptureRecord::LinkDown { monotonic_ms }
        | SessionCaptureRecord::Subscribe { monotonic_ms, .. }
        | SessionCaptureRecord::Write { monotonic_ms, .. }
        | SessionCaptureRecord::Notification { monotonic_ms, .. } => *monotonic_ms,
    }
}

pub(crate) fn merge_session_report(into: &mut SessionBridgeReport, report: SessionBridgeReport) {
    into.protocol_writes += report.protocol_writes;
    into.writes += report.writes;
    into.subscribes += report.subscribes;
    into.notifications += report.notifications;
    into.notification_bytes += report.notification_bytes;
    if report.latest_notification_len.is_some() {
        into.latest_notification_len = report.latest_notification_len;
    }
    into.telemetry += report.telemetry;
    into.telemetry_snapshot = report.telemetry_snapshot;
    into.read_only_responses += report.read_only_responses;
    into.read_only_response_events
        .extend(report.read_only_response_events);
    into.firmware = report.firmware.or(into.firmware.take());
    into.settings.extend(report.settings);
    into.diagnostics += report.diagnostics;
    into.diagnostics_snapshot.merge(report.diagnostics_snapshot);
    into.diagnostic_errors.extend(report.diagnostic_errors);
    if report.identity.is_some() {
        into.identity = report.identity;
    }
    into.events.extend(report.events);
    into.disconnects += report.disconnects;
}

fn session_record_to_pevcap_record(record: &SessionCaptureRecord) -> Option<PevcapRecord> {
    match record {
        SessionCaptureRecord::Link {
            monotonic_ms,
            max_write_len,
        } => Some(PevcapRecord::link_up(*monotonic_ms, *max_write_len)),
        SessionCaptureRecord::LinkDown { monotonic_ms } => {
            Some(PevcapRecord::link_down(*monotonic_ms))
        }
        SessionCaptureRecord::Write {
            monotonic_ms,
            characteristic,
            mode,
            bytes,
            provisional: _,
        } => Some(PevcapRecord::outbound_write(
            *monotonic_ms,
            gatt_channel_from_uuid(*characteristic),
            *mode,
            bytes.clone(),
        )),
        SessionCaptureRecord::Notification {
            monotonic_ms,
            characteristic,
            service,
            bytes,
        } => Some(PevcapRecord::inbound_notification(
            *monotonic_ms,
            gatt_channel_from_uuid(*characteristic),
            gatt_channel_from_uuid(*service),
            bytes.clone(),
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
