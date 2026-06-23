use async_trait::async_trait;
use cutout_core::{DeviceCommand, GattChannel, ProtocolSession};

use crate::{
    BtleError, ConnectionSummary, ConnectionTarget, MaxReconnectLinks, MonotonicMs,
    NotificationWindow, ReconnectAttempt, ScanWindow, SessionBridgeError, SessionBridgeReport,
    SessionCapture, SessionCaptureRecord, SessionPeripheral, WriteProvenance,
    bridge::{DriveSessionConfig, drive_session_inner},
    capture::session_record_monotonic_ms,
    connect_and_discover,
    report::merge_session_report,
};

const EXTERNAL_LINK_LOSS_IDLE_WINDOW: NotificationWindow = NotificationWindow::from_millis(1_500);

/// Host boundary that can create fresh connected peripherals for reconnecting sessions.
#[async_trait]
pub trait ReconnectingSessionHost: Send {
    /// Connected peripheral type returned for each link attempt.
    type Peripheral: SessionPeripheral + Sync;

    /// Connects and discovers the next link attempt.
    async fn connect(&mut self) -> Result<(Self::Peripheral, ConnectionSummary), BtleError>;
}

/// Production reconnect host backed by btleplug target scanning and discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BtleplugReconnectHost {
    target: ConnectionTarget,
    scan_for: ScanWindow,
}

impl BtleplugReconnectHost {
    /// Creates a reconnect host that reuses the same target and scan duration
    /// for every link attempt.
    #[must_use]
    pub const fn new(target: ConnectionTarget, scan_for: ScanWindow) -> Self {
        Self { target, scan_for }
    }

    /// Returns the target reused for each reconnect attempt.
    #[must_use]
    pub const fn target(&self) -> &ConnectionTarget {
        &self.target
    }

    /// Returns the scan duration reused for each reconnect attempt.
    #[must_use]
    pub const fn scan_for(&self) -> ScanWindow {
        self.scan_for
    }
}

#[async_trait]
impl ReconnectingSessionHost for BtleplugReconnectHost {
    type Peripheral = btleplug::platform::Peripheral;

    async fn connect(&mut self) -> Result<(Self::Peripheral, ConnectionSummary), BtleError> {
        let connected = connect_and_discover(&self.target, self.scan_for).await?;
        Ok((connected.peripheral, connected.summary))
    }
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
    pub attempt: ReconnectAttempt,

    /// Connection summary observed for this link attempt.
    pub summary: ConnectionSummary,

    /// Bridge counters and lifecycle events observed during this link attempt.
    pub report: SessionBridgeReport,
}

/// Captures a session across reconnect attempts supplied by a host boundary.
///
/// The host owns platform-specific connect/discover work. This bridge repeats
/// one bounded session run when the previous link intentionally disconnected or
/// the notification stream ended before the capture window elapsed.
///
/// # Errors
///
/// Returns the underlying host, transport, or bridge error from any link
/// attempt.
pub async fn capture_reconnecting_session<H, S>(
    host: &mut H,
    session: &mut S,
    channel: GattChannel,
    notification_window: NotificationWindow,
    max_links: MaxReconnectLinks,
    write_provenance: WriteProvenance,
) -> Result<SessionCapture, BtleError>
where
    H: ReconnectingSessionHost,
    S: ProtocolSession + Send,
{
    Ok(capture_reconnecting_session_with_commands(
        host,
        session,
        channel,
        notification_window,
        max_links,
        write_provenance,
        &[],
    )
    .await?
    .capture)
}

/// Captures a reconnecting session and preserves per-link connection summaries.
///
/// # Errors
///
/// Returns the underlying host, transport, or bridge error from any link
/// attempt.
pub async fn capture_reconnecting_session_with_summaries<H, S>(
    host: &mut H,
    session: &mut S,
    channel: GattChannel,
    notification_window: NotificationWindow,
    max_links: MaxReconnectLinks,
    write_provenance: WriteProvenance,
) -> Result<ReconnectingSessionCapture, BtleError>
where
    H: ReconnectingSessionHost,
    S: ProtocolSession + Send,
{
    capture_reconnecting_session_with_commands(
        host,
        session,
        channel,
        notification_window,
        max_links,
        write_provenance,
        &[],
    )
    .await
}

/// Captures a reconnecting session, sending explicit commands on the first link only.
///
/// Commands are intentionally not replayed after reconnect. A link loss cancels
/// any in-flight response expectation for those commands while allowing the
/// read-only session to resume passive subscription on a fresh link.
///
/// # Errors
///
/// Returns the underlying host, transport, or bridge error from any link
/// attempt.
pub async fn capture_reconnecting_session_with_commands<H, S>(
    host: &mut H,
    session: &mut S,
    channel: GattChannel,
    notification_window: NotificationWindow,
    max_links: MaxReconnectLinks,
    write_provenance: WriteProvenance,
    commands: &[DeviceCommand],
) -> Result<ReconnectingSessionCapture, BtleError>
where
    H: ReconnectingSessionHost,
    S: ProtocolSession + Send,
{
    let mut reconnecting_capture = ReconnectingSessionCapture::default();
    let mut monotonic_start = MonotonicMs::default();

    for attempt in max_links.attempts() {
        let (peripheral, summary) = match host.connect().await {
            Ok(connection) => connection,
            Err(error) => {
                if reconnecting_capture.capture.records.is_empty() {
                    return Err(error);
                }
                break;
            }
        };
        let endpoints = summary
            .select_session_endpoints()
            .ok_or(SessionBridgeError::MissingSessionEndpoint)?;
        let mut records = Vec::new();
        let report = drive_session_inner(
            &peripheral,
            session,
            DriveSessionConfig {
                channel,
                summary: &summary,
                endpoints,
                notification_window,
                commands: if attempt.is_first() { commands } else { &[] },
                write_provenance,
                monotonic_start,
                stream_end_is_link_down: true,
                link_loss_idle_window: Some(EXTERNAL_LINK_LOSS_IDLE_WINDOW),
            },
            Some(&mut records),
            None,
        )
        .await?;
        monotonic_start = records
            .iter()
            .map(session_record_monotonic_ms)
            .max()
            .unwrap_or(monotonic_start)
            .next();
        merge_session_report(&mut reconnecting_capture.capture.report, report.clone());
        let should_reconnect = report.disconnects.has_events()
            && records
                .iter()
                .any(|record| matches!(record, SessionCaptureRecord::LinkDown { .. }));
        reconnecting_capture.attempts.push(ReconnectAttemptReport {
            attempt,
            summary,
            report,
        });
        reconnecting_capture.capture.records.extend(records);
        if !should_reconnect {
            break;
        }
    }

    Ok(reconnecting_capture)
}
