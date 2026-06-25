use cutout_core::{
    DeviceEvent, DiagnosticError, FirmwareInfo, NotificationByteLen, NotificationIngestOutcome,
    ParserDiagnostics, ReadOnlyResponse, SettingsReadback, TelemetryDelta, TelemetrySnapshot,
};

use crate::{
    BridgeIdentityResolution, DiagnosticEventCount, DisconnectCount, MonotonicMs,
    NotificationCount, NotificationPayloadTotal, ProtocolWriteCount, ReadOnlyResponseCount,
    SubscribeCount, TelemetryEventCount, TransportWriteCount,
};

/// Report produced by a protocol bridge run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionBridgeReport {
    /// Protocol write actions emitted by the session before transport chunking.
    pub protocol_writes: ProtocolWriteCount,

    /// Transport writes executed through the bridge.
    pub writes: TransportWriteCount,

    /// Transport subscribe operations executed through the bridge.
    pub subscribes: SubscribeCount,

    /// Notification payloads relayed into the session.
    pub notifications: NotificationCount,

    /// Total notification payload bytes relayed into the session.
    pub notification_bytes: NotificationPayloadTotal,

    /// Length of the latest notification payload, if any were observed.
    pub latest_notification_len: Option<NotificationByteLen>,

    /// Semantic telemetry events emitted by the session.
    pub telemetry: TelemetryEventCount,

    /// Latest semantic telemetry values emitted by the session.
    pub telemetry_snapshot: TelemetrySnapshot,

    /// Semantic read-only response events emitted by the session.
    pub read_only_responses: ReadOnlyResponseCount,

    /// Full read-only response payloads emitted by the session.
    pub read_only_response_events: Vec<ReadOnlyResponse>,

    /// Latest firmware readback emitted by the session.
    pub firmware: Option<FirmwareInfo>,

    /// Settings readbacks emitted by the session.
    pub settings: Vec<SettingsReadback>,

    /// Parser diagnostics events emitted by the session.
    pub diagnostics: DiagnosticEventCount,

    /// Aggregated parser diagnostic counters emitted by the session.
    pub diagnostics_snapshot: ParserDiagnostics,

    /// Detailed parser diagnostic errors emitted by the session.
    pub diagnostic_errors: Vec<DiagnosticError>,

    /// Staged identity resolution from non-actuating evidence.
    pub identity: Option<BridgeIdentityResolution>,

    /// Timestamped semantic events observed during the run.
    pub events: Vec<SessionBridgeEvent>,

    /// Transport disconnect operations executed through the bridge.
    pub disconnects: DisconnectCount,
}

impl SessionBridgeReport {
    /// Records a typed notification ingest outcome at the report edge.
    pub fn record_notification_ingest(
        &mut self,
        outcome: NotificationIngestOutcome,
        monotonic_ms: MonotonicMs,
    ) {
        self.events.push(SessionBridgeEvent::NotificationIngest {
            monotonic_ms,
            outcome,
        });
    }
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
    into.protocol_writes = into.protocol_writes.saturating_add(report.protocol_writes);
    into.writes = into.writes.saturating_add(report.writes);
    into.subscribes = into.subscribes.saturating_add(report.subscribes);
    into.notifications = into.notifications.saturating_add(report.notifications);
    into.notification_bytes = into
        .notification_bytes
        .saturating_add(report.notification_bytes);
    if report.latest_notification_len.is_some() {
        into.latest_notification_len = report.latest_notification_len;
    }
    into.telemetry = into.telemetry.saturating_add(report.telemetry);
    into.telemetry_snapshot = report.telemetry_snapshot;
    into.read_only_responses = into
        .read_only_responses
        .saturating_add(report.read_only_responses);
    into.read_only_response_events
        .extend(report.read_only_response_events);
    into.firmware = report.firmware.or(into.firmware.take());
    into.settings.extend(report.settings);
    into.diagnostics = into.diagnostics.saturating_add(report.diagnostics);
    into.diagnostics_snapshot.merge(report.diagnostics_snapshot);
    into.diagnostic_errors.extend(report.diagnostic_errors);
    if report.identity.is_some() {
        into.identity = report.identity;
    }
    into.events.extend(report.events);
    into.disconnects = into.disconnects.saturating_add(report.disconnects);
}

pub(crate) fn process_notification_ingest_outcome(
    report: &mut SessionBridgeReport,
    outcome: NotificationIngestOutcome,
    monotonic_ms: MonotonicMs,
) {
    report.record_notification_ingest(outcome, monotonic_ms);
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
            report.telemetry = report.telemetry.increment();
            report.telemetry_snapshot.apply_delta(delta);
            report.events.push(SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms,
                delta,
            });
        }
        DeviceEvent::ReadOnlyResponse(response) => {
            report.read_only_responses = report.read_only_responses.increment();
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
            report.diagnostics = report.diagnostics.increment();
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
