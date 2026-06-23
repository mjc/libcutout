use cutout_core::{
    DeviceEvent, DiagnosticError, FirmwareInfo, NotificationIngestOutcome, ParserDiagnostics,
    ReadOnlyResponse, SettingsReadback, TelemetryDelta, TelemetrySnapshot,
};

use crate::{BridgeIdentityResolution, MonotonicMs};

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

/// Timestamped semantic event emitted by the bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionBridgeEvent {
    /// Link-down event emitted by the protocol session after transport disconnect.
    LinkDown {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,
    },

    /// Decoded telemetry emitted by the protocol session.
    ProcessedTelemetry {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Telemetry delta emitted by the protocol session.
        delta: TelemetryDelta,
    },

    /// Read-only response emitted by the protocol session.
    ReadOnlyResponse {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Read-only response emitted by the protocol session.
        response: ReadOnlyResponse,
    },

    /// Parser diagnostics emitted by the protocol session.
    Diagnostics {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Parser diagnostic counters emitted at this timestamp.
        diagnostics: ParserDiagnostics,
    },

    /// Detailed parser diagnostic error emitted by the protocol session.
    DiagnosticError {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Detailed parser error emitted at this timestamp.
        error: DiagnosticError,
    },

    /// Typed protocol notification ingest outcome emitted by the session.
    NotificationIngest {
        /// Relative monotonic timestamp in milliseconds.
        monotonic_ms: MonotonicMs,

        /// Protocol ingest outcome emitted at this timestamp.
        outcome: NotificationIngestOutcome,
    },
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

pub(crate) fn process_notification_ingest_outcome(
    report: &mut SessionBridgeReport,
    outcome: NotificationIngestOutcome,
    monotonic_ms: MonotonicMs,
) {
    report.events.push(SessionBridgeEvent::NotificationIngest {
        monotonic_ms,
        outcome,
    });
}

pub(crate) fn process_device_event(
    report: &mut SessionBridgeReport,
    event: DeviceEvent,
    monotonic_ms: MonotonicMs,
) {
    match event {
        DeviceEvent::LinkUp(_) | DeviceEvent::Tick { .. } | DeviceEvent::ControlRefusal(_) => {}
        DeviceEvent::LinkDown => {
            report
                .events
                .push(SessionBridgeEvent::LinkDown { monotonic_ms });
        }
        DeviceEvent::Telemetry(delta) => {
            report.telemetry += 1;
            report.telemetry_snapshot.apply_delta(delta);
            report.events.push(SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms,
                delta,
            });
        }
        DeviceEvent::ReadOnlyResponse(response) => {
            report.read_only_responses += 1;
            report.read_only_response_events.push(response);
            report.events.push(SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms,
                response,
            });
            match response {
                ReadOnlyResponse::Firmware(firmware) => {
                    report.firmware = Some(firmware);
                }
                ReadOnlyResponse::Settings(settings) => {
                    report.settings.push(settings);
                }
                ReadOnlyResponse::Battery(_)
                | ReadOnlyResponse::Diagnostics(_)
                | ReadOnlyResponse::RawTelemetry(_) => {}
            }
        }
        DeviceEvent::Diagnostics(diagnostics) => {
            report.diagnostics += 1;
            report.diagnostics_snapshot.merge(diagnostics);
            report.events.push(SessionBridgeEvent::Diagnostics {
                monotonic_ms,
                diagnostics,
            });
        }
        DeviceEvent::DiagnosticError(error) => {
            report.diagnostic_errors.push(error);
            report.events.push(SessionBridgeEvent::DiagnosticError {
                monotonic_ms,
                error,
            });
        }
    }
}
