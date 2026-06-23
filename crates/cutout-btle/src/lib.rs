#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Bluetooth transport adapter scaffolding for Cutout.

use std::{collections::VecDeque, future::Future, pin::Pin, time::Duration};

use async_trait::async_trait;
use btleplug::{
    api::{Central, Characteristic, Manager as _, Peripheral as _, ScanFilter, ValueNotification},
    platform::{Adapter, Manager},
};
use cutout_core::{
    DeviceCommand, DeviceEvent, GattChannel, LinkInfo, NotificationIngestOutcome, ProtocolSession,
    ReadOnlyResponse, SessionInput, SessionOutput, TransportAction, WriteMode,
};
use futures_util::{StreamExt, stream::Stream};
use tracing::{debug, info};

mod capture;
mod error;
mod gatt;
mod identity;
mod observation;
mod types;

use capture::{merge_session_report, session_record_monotonic_ms};
use gatt::gatt_channel_from_uuid;
use identity::{IdentityContext, IdentityState, update_identity_report};
use types::characteristic_from_summary;

pub use capture::{
    BridgeIdentityResolution, PevcapSessionMetadata, RawNotificationRecord, ReconnectAttemptReport,
    ReconnectingSessionCapture, SessionBridgeEvent, SessionBridgeReport, SessionCapture,
    SessionCaptureRecord,
};
pub use error::{BtleError, SessionBridgeError};
pub use gatt::{
    GattUuid, KnownGattUuid, SharedFfe0Service, StandardBatteryLevelCharacteristic,
    StandardBatteryService,
};
pub use types::{
    AdvertisedServices, BluetoothAddress, CharacteristicSummary, ConnectedPeripheral,
    ConnectionSummary, ConnectionTarget, ManufacturerDataSummaries, ManufacturerDataSummary,
    NullBluetoothAddress, PeripheralIdentifier, PeripheralObservation, ServiceSummary,
    SessionEndpoints,
};

const TARGETED_SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const BACKEND_OPERATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-btle"
}

/// Minimal BTLE operations required by the protocol bridge.
#[async_trait]
pub trait SessionPeripheral: Send + Sync {
    /// Returns the negotiated MTU for the connected peripheral.
    fn mtu(&self) -> u16;

    /// Subscribes to notifications on the selected endpoint.
    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError>;

    /// Writes a payload to the selected endpoint.
    async fn write(
        &self,
        characteristic: &Characteristic,
        bytes: &[u8],
        mode: WriteMode,
    ) -> Result<(), BtleError>;

    /// Returns the notification stream for the connected peripheral.
    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>, BtleError>;

    /// Disconnects the peripheral.
    async fn disconnect(&self) -> Result<(), BtleError>;
}

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
    scan_for: Duration,
}

impl BtleplugReconnectHost {
    /// Creates a reconnect host that reuses the same target and scan duration
    /// for every link attempt.
    #[must_use]
    pub const fn new(target: ConnectionTarget, scan_for: Duration) -> Self {
        Self { target, scan_for }
    }

    /// Returns the target reused for each reconnect attempt.
    #[must_use]
    pub const fn target(&self) -> &ConnectionTarget {
        &self.target
    }

    /// Returns the scan duration reused for each reconnect attempt.
    #[must_use]
    pub const fn scan_for(&self) -> Duration {
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

/// Scans for peripherals and returns what was observed.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] when the platform exposes no
/// adapters, or [`BtleError::Backend`] when the BTLE backend reports a failure.
pub async fn scan_peripherals(scan_for: Duration) -> Result<Vec<PeripheralObservation>, BtleError> {
    let adapter = first_adapter().await?;
    backend_call("start scan", adapter.start_scan(ScanFilter::default())).await?;
    tokio::time::sleep(scan_for).await;
    let observations = collect_observations(&adapter).await?;
    let _ = backend_call("stop scan", adapter.stop_scan()).await;
    Ok(observations)
}

/// Connects to the first peripheral matching the target and returns a summary.
///
/// # Errors
///
/// Returns [`BtleError::NoAdapterAvailable`] if no adapter is present,
/// [`BtleError::NoPeripheralMatched`] if scan results do not satisfy the
/// target, or [`BtleError::Backend`] if the BTLE backend fails.
pub async fn connect_and_discover(
    target: &ConnectionTarget,
    scan_for: Duration,
) -> Result<ConnectedPeripheral, BtleError> {
    let adapter = first_adapter().await?;
    backend_call("start scan", adapter.start_scan(ScanFilter::default())).await?;

    let peripheral = wait_for_scan_match(scan_for, TARGETED_SCAN_POLL_INTERVAL, || {
        find_peripheral(&adapter, target)
    })
    .await;
    let _ = backend_call("stop scan", adapter.stop_scan()).await;
    let peripheral = peripheral?;

    backend_call("connect peripheral", peripheral.connect()).await?;
    backend_call("discover services", peripheral.discover_services()).await?;

    let observation = observation_from_peripheral(&peripheral).await?;
    let services = peripheral
        .services()
        .into_iter()
        .map(|service| ServiceSummary::from_service(&service))
        .collect();

    Ok(ConnectedPeripheral {
        peripheral,
        summary: ConnectionSummary {
            observation,
            services,
        },
    })
}

/// Reads the standard BLE Battery Level characteristic from a connected peripheral.
///
/// Returns `Ok(None)` when the device does not expose a readable Battery Level
/// characteristic or when the characteristic returns an empty payload.
///
/// # Errors
///
/// Returns [`BtleError::Backend`] if the underlying BTLE stack fails the read.
pub async fn read_battery_level(
    peripheral: &btleplug::platform::Peripheral,
    summary: &ConnectionSummary,
) -> Result<Option<u8>, BtleError> {
    let Some(characteristic) = summary.battery_level_characteristic() else {
        return Ok(None);
    };

    let value = backend_call(
        "read battery level",
        peripheral.read(&characteristic_from_summary(characteristic)),
    )
    .await?;
    Ok(value.first().copied().map(|percent| percent.min(100)))
}

/// Drives a protocol session against the selected BTLE endpoints.
///
/// # Errors
///
/// Returns [`BtleError::Bridge`] when a session output references a channel
/// that does not match the selected binding, or when the session asks for
/// subscription but no notify-capable endpoint was selected.
pub async fn drive_session<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_with_commands(
        peripheral,
        session,
        channel,
        summary,
        endpoints,
        notification_window,
        &[],
    )
    .await
}

/// Drives a protocol session against a connected peripheral and explicit commands.
///
/// Commands are injected after link setup/subscription processing and before
/// the passive notification window, so any resulting writes are captured as
/// ordinary session transport actions.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe, write, or
/// notification streaming fails.
pub async fn drive_session_with_commands<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
    commands: &[DeviceCommand],
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_inner(
        peripheral,
        session,
        DriveSessionConfig {
            channel,
            summary,
            endpoints,
            notification_window,
            commands,
            provisional_writes: false,
            monotonic_start: 0,
        },
        None,
    )
    .await
}

/// Captures a protocol session against the selected BTLE endpoints.
///
/// # Errors
///
/// Returns [`BtleError::Bridge`] when a session output references a channel
/// that does not match the selected binding, or when the session asks for
/// subscription but no notify-capable endpoint was selected.
pub async fn capture_session<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
    provisional_writes: bool,
) -> Result<SessionCapture, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut records = Vec::new();
    let report = drive_session_inner(
        peripheral,
        session,
        DriveSessionConfig {
            channel,
            summary,
            endpoints,
            notification_window,
            commands: &[],
            provisional_writes,
            monotonic_start: 0,
        },
        Some(&mut records),
    )
    .await?;

    Ok(SessionCapture { records, report })
}

/// Captures a protocol session while injecting explicit read-only commands.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe, write, or
/// notification streaming fails.
pub async fn capture_session_with_commands<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: Duration,
    commands: &[DeviceCommand],
) -> Result<SessionCapture, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut records = Vec::new();
    let report = drive_session_inner(
        peripheral,
        session,
        DriveSessionConfig {
            channel,
            summary,
            endpoints,
            notification_window,
            commands,
            provisional_writes: false,
            monotonic_start: 0,
        },
        Some(&mut records),
    )
    .await?;
    Ok(SessionCapture { records, report })
}

/// Captures a session across reconnect attempts supplied by a host boundary.
///
/// The host owns platform-specific connect/discover work. This bridge repeats
/// one bounded session run only when the previous link intentionally
/// disconnected.
///
/// # Errors
///
/// Returns the underlying host, transport, or bridge error from any link
/// attempt.
pub async fn capture_reconnecting_session<H, S>(
    host: &mut H,
    session: &mut S,
    channel: GattChannel,
    notification_window: Duration,
    max_links: usize,
    provisional_writes: bool,
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
        provisional_writes,
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
    notification_window: Duration,
    max_links: usize,
    provisional_writes: bool,
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
        provisional_writes,
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
    notification_window: Duration,
    max_links: usize,
    provisional_writes: bool,
    commands: &[DeviceCommand],
) -> Result<ReconnectingSessionCapture, BtleError>
where
    H: ReconnectingSessionHost,
    S: ProtocolSession + Send,
{
    let mut reconnecting_capture = ReconnectingSessionCapture::default();
    let mut monotonic_start = 0;

    for attempt in 1..=max_links {
        let (peripheral, summary) = host.connect().await?;
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
                commands: if attempt == 1 { commands } else { &[] },
                provisional_writes,
                monotonic_start,
            },
            Some(&mut records),
        )
        .await?;
        monotonic_start = records
            .iter()
            .map(session_record_monotonic_ms)
            .max()
            .unwrap_or(monotonic_start)
            .saturating_add(1);
        merge_session_report(&mut reconnecting_capture.capture.report, report.clone());
        let should_reconnect = report.disconnects > 0
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

/// Subscribes to a notify/indicate characteristic and records raw notification chunks.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe or
/// notification streaming fails.
pub async fn capture_raw_notifications<P>(
    peripheral: &P,
    characteristic: &CharacteristicSummary,
    notification_window: Duration,
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
    let deadline = started_at + notification_window;
    let mut records = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(notification)) if notification.uuid == characteristic.uuid => {
                records.push(RawNotificationRecord {
                    monotonic_ms: u64::try_from(started_at.elapsed().as_millis())
                        .unwrap_or(u64::MAX),
                    characteristic: notification.uuid,
                    service: notification.service_uuid,
                    bytes: notification.value,
                });
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }

    Ok(records)
}

struct DriveSessionConfig<'a> {
    channel: GattChannel,
    summary: &'a ConnectionSummary,
    endpoints: SessionEndpoints<'a>,
    notification_window: Duration,
    commands: &'a [DeviceCommand],
    provisional_writes: bool,
    monotonic_start: u64,
}

async fn drive_session_inner<P, S>(
    peripheral: &P,
    session: &mut S,
    config: DriveSessionConfig<'_>,
    mut capture: Option<&mut Vec<SessionCaptureRecord>>,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = config.notification_window.as_millis(),
        channel = ?config.channel,
        "session bridge drive inner entered"
    );
    let mut report = SessionBridgeReport::default();
    let identity_context = IdentityContext::new(config.summary);
    let mut identity_state = IdentityState::default();
    update_identity_report(&mut report, &identity_context, &identity_state);
    let bindings = BridgeBindings {
        write_characteristic: characteristic_from_summary(config.endpoints.write),
        notify_characteristic: config.endpoints.notify.map(characteristic_from_summary),
    };
    let mut outputs = Vec::new();
    let mut monotonic_ms = config.monotonic_start;

    process_link_up_outputs(
        LinkUpContext {
            peripheral,
            channel: config.channel,
            bindings: &bindings,
            report: &mut report,
            capture: capture.as_deref_mut(),
            provisional_writes: config.provisional_writes,
        },
        session,
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    for command in config.commands {
        monotonic_ms += 1;
        session.handle(SessionInput::Command(*command), &mut outputs);
        process_session_outputs(
            SessionOutputContext {
                peripheral,
                channel: config.channel,
                write_characteristic: &bindings.write_characteristic,
                notify_characteristic: bindings.notify_characteristic.as_ref(),
                report: &mut report,
                capture: capture.as_deref_mut(),
                provisional_writes: config.provisional_writes,
            },
            session,
            &mut outputs,
            monotonic_ms,
        )
        .await?;
    }

    monotonic_ms += 1;
    session.handle(SessionInput::Tick { monotonic_ms }, &mut outputs);
    process_session_outputs(
        SessionOutputContext {
            peripheral,
            channel: config.channel,
            write_characteristic: &bindings.write_characteristic,
            notify_characteristic: bindings.notify_characteristic.as_ref(),
            report: &mut report,
            capture: capture.as_deref_mut(),
            provisional_writes: config.provisional_writes,
        },
        session,
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    if config.notification_window.is_zero() || bindings.notify_characteristic.is_none() {
        return Ok(report);
    }

    process_notification_window(
        NotificationLoopContext {
            peripheral,
            channel: config.channel,
            bindings: &bindings,
            identity_context: &identity_context,
            identity_state: &mut identity_state,
            report: &mut report,
            capture,
            provisional_writes: config.provisional_writes,
        },
        session,
        &mut outputs,
        &mut monotonic_ms,
        config.notification_window,
    )
    .await?;

    Ok(report)
}

struct BridgeBindings {
    write_characteristic: Characteristic,
    notify_characteristic: Option<Characteristic>,
}

struct LinkUpContext<'a, P: ?Sized> {
    peripheral: &'a P,
    channel: GattChannel,
    bindings: &'a BridgeBindings,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    provisional_writes: bool,
}

async fn process_link_up_outputs<P, S>(
    mut context: LinkUpContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: u64,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let max_write_len = Some(context.peripheral.mtu());

    info!("session bridge link-up handling starting");
    session.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms,
            max_write_len,
        }),
        outputs,
    );
    info!(
        outputs = outputs.len(),
        "session bridge link-up handling completed"
    );
    if let Some(records) = context.capture.as_deref_mut() {
        records.push(SessionCaptureRecord::Link {
            monotonic_ms,
            max_write_len,
        });
    }
    info!(
        outputs = outputs.len(),
        "session bridge initial output processing starting"
    );
    process_session_outputs(
        SessionOutputContext {
            peripheral: context.peripheral,
            channel: context.channel,
            write_characteristic: &context.bindings.write_characteristic,
            notify_characteristic: context.bindings.notify_characteristic.as_ref(),
            report: context.report,
            capture: context.capture.as_deref_mut(),
            provisional_writes: context.provisional_writes,
        },
        session,
        outputs,
        monotonic_ms,
    )
    .await?;
    info!("session bridge initial output processing completed");

    Ok(())
}

struct NotificationLoopContext<'a, P: ?Sized> {
    peripheral: &'a P,
    channel: GattChannel,
    bindings: &'a BridgeBindings,
    identity_context: &'a IdentityContext<'a>,
    identity_state: &'a mut IdentityState,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    provisional_writes: bool,
}

async fn process_notification_window<P, S>(
    mut context: NotificationLoopContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: &mut u64,
    notification_window: Duration,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = notification_window.as_millis(),
        "session notification window starting"
    );
    info!("session notifications stream await starting");
    let mut notifications = context.peripheral.notifications().await?;
    info!("session notifications stream await completed");
    let deadline = tokio::time::Instant::now() + notification_window;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        debug!(
            remaining_ms = remaining.as_millis(),
            "session notification next await starting"
        );
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(notification)) => {
                *monotonic_ms += 1;
                if let Some(records) = context.capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::Notification {
                        monotonic_ms: *monotonic_ms,
                        characteristic: notification.uuid,
                        service: notification.service_uuid,
                        bytes: notification.value.clone(),
                    });
                }
                context.identity_state.observe(&notification.value);
                update_identity_report(
                    context.report,
                    context.identity_context,
                    context.identity_state,
                );
                session.handle(
                    SessionInput::Notification {
                        channel: context.channel,
                        bytes: &notification.value,
                        monotonic_ms: *monotonic_ms,
                    },
                    outputs,
                );
                let decode_outcome = notification_decode_outcome(outputs);
                process_session_outputs(
                    SessionOutputContext {
                        peripheral: context.peripheral,
                        channel: context.channel,
                        write_characteristic: &context.bindings.write_characteristic,
                        notify_characteristic: context.bindings.notify_characteristic.as_ref(),
                        report: context.report,
                        capture: context.capture.as_deref_mut(),
                        provisional_writes: context.provisional_writes,
                    },
                    session,
                    outputs,
                    *monotonic_ms,
                )
                .await?;
                log_notification_decode_outcome(decode_outcome, &notification, context.channel);
                context.report.notifications += 1;
                context.report.notification_bytes += notification.value.len();
                context.report.latest_notification_len = Some(notification.value.len());
            }
            Ok(None) => {
                debug!("session notification stream ended");
                break;
            }
            Err(_) => {
                debug!("session notification window elapsed");
                break;
            }
        }
    }
    debug!(
        notifications = context.report.notifications,
        notification_bytes = context.report.notification_bytes,
        latest_notification_len = ?context.report.latest_notification_len,
        "session notification window completed"
    );

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NotificationDecodeOutcome {
    Ignored,
    BufferedFragment,
    ParserGap,
    KnownReserved,
    ParserDiagnostic,
    SemanticEvents,
}

fn notification_decode_outcome(outputs: &[SessionOutput]) -> NotificationDecodeOutcome {
    outputs
        .iter()
        .filter_map(|output| match output {
            SessionOutput::NotificationIngest(outcome) => Some(outcome),
            SessionOutput::Transport(_) | SessionOutput::Event(_) => None,
        })
        .map(NotificationDecodeOutcome::from)
        .max()
        .unwrap_or(NotificationDecodeOutcome::Ignored)
}

impl From<&NotificationIngestOutcome> for NotificationDecodeOutcome {
    fn from(outcome: &NotificationIngestOutcome) -> Self {
        match outcome {
            NotificationIngestOutcome::SemanticEvents { .. } => {
                NotificationDecodeOutcome::SemanticEvents
            }
            NotificationIngestOutcome::ParserDiagnostic { .. } => {
                NotificationDecodeOutcome::ParserDiagnostic
            }
            NotificationIngestOutcome::KnownReserved { .. } => {
                NotificationDecodeOutcome::KnownReserved
            }
            NotificationIngestOutcome::ParserGap { .. } => NotificationDecodeOutcome::ParserGap,
            NotificationIngestOutcome::BufferedFragment(_) => {
                NotificationDecodeOutcome::BufferedFragment
            }
            NotificationIngestOutcome::Ignored(_) => NotificationDecodeOutcome::Ignored,
        }
    }
}

fn log_notification_decode_outcome(
    outcome: NotificationDecodeOutcome,
    notification: &ValueNotification,
    channel: GattChannel,
) {
    match outcome {
        NotificationDecodeOutcome::SemanticEvents => {}
        NotificationDecodeOutcome::BufferedFragment => {
            debug!(
                len = notification.value.len(),
                channel = ?channel,
                "session notification buffered by protocol decoder"
            );
        }
        NotificationDecodeOutcome::ParserDiagnostic => {
            debug!(
                len = notification.value.len(),
                channel = ?channel,
                "session notification produced parser diagnostic"
            );
        }
        NotificationDecodeOutcome::KnownReserved => {
            debug!(
                len = notification.value.len(),
                channel = ?channel,
                "session notification produced known reserved protocol evidence"
            );
        }
        NotificationDecodeOutcome::ParserGap => {
            debug!(
                len = notification.value.len(),
                channel = ?channel,
                "session notification produced parser gap evidence"
            );
        }
        NotificationDecodeOutcome::Ignored => {
            debug!(
                uuid = %notification.uuid,
                service = %notification.service_uuid,
                len = notification.value.len(),
                channel = ?channel,
                "session notification ignored by protocol session"
            );
        }
    }
}

struct SessionOutputContext<'a, P: ?Sized> {
    peripheral: &'a P,
    channel: GattChannel,
    write_characteristic: &'a Characteristic,
    notify_characteristic: Option<&'a Characteristic>,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    provisional_writes: bool,
}

async fn process_session_outputs<P, S>(
    mut context: SessionOutputContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: u64,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut pending = outputs.drain(..).collect::<VecDeque<_>>();
    while let Some(output) = pending.pop_front() {
        match output {
            SessionOutput::Transport(TransportAction::Subscribe { channel: observed }) => {
                info!(
                    expected = ?context.channel,
                    observed = ?observed,
                    monotonic_ms,
                    "session bridge processing subscribe output"
                );
                if observed != context.channel {
                    return Err(SessionBridgeError::UnexpectedChannel {
                        expected: context.channel,
                        observed,
                    }
                    .into());
                }
                let Some(notify_characteristic) = context.notify_characteristic else {
                    return Err(SessionBridgeError::MissingNotifyEndpoint {
                        channel: context.channel,
                    }
                    .into());
                };
                info!(
                    characteristic = %notify_characteristic.uuid,
                    service = %notify_characteristic.service_uuid,
                    monotonic_ms,
                    "session subscribe await starting"
                );
                context.peripheral.subscribe(notify_characteristic).await?;
                info!(
                    characteristic = %notify_characteristic.uuid,
                    service = %notify_characteristic.service_uuid,
                    monotonic_ms,
                    "session subscribe await completed"
                );
                if let Some(records) = context.capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::Subscribe {
                        monotonic_ms,
                        characteristic: notify_characteristic.uuid,
                    });
                }
                context.report.subscribes += 1;
            }
            SessionOutput::Transport(TransportAction::Write {
                channel: observed,
                bytes,
                mode,
            }) => {
                if observed != context.channel {
                    return Err(SessionBridgeError::UnexpectedChannel {
                        expected: context.channel,
                        observed,
                    }
                    .into());
                }
                context.report.protocol_writes += 1;
                let write_limit = usize::from(context.peripheral.mtu()).max(1);
                for chunk in bytes.as_slice().chunks(write_limit) {
                    context
                        .peripheral
                        .write(context.write_characteristic, chunk, mode)
                        .await?;
                    if let Some(records) = context.capture.as_deref_mut() {
                        records.push(SessionCaptureRecord::Write {
                            monotonic_ms,
                            characteristic: context.write_characteristic.uuid,
                            mode,
                            bytes: chunk.to_vec(),
                            provisional: context.provisional_writes,
                        });
                    }
                    context.report.writes += 1;
                }
            }
            SessionOutput::Transport(TransportAction::Disconnect) => {
                context.peripheral.disconnect().await?;
                if let Some(records) = context.capture.as_deref_mut() {
                    records.push(SessionCaptureRecord::LinkDown { monotonic_ms });
                }
                context.report.disconnects += 1;
                session.handle(SessionInput::LinkDown, outputs);
                pending.extend(outputs.drain(..));
            }
            SessionOutput::Event(event) => {
                process_device_event(context.report, event, monotonic_ms);
            }
            SessionOutput::NotificationIngest(outcome) => {
                process_notification_ingest_outcome(context.report, outcome, monotonic_ms);
            }
        }
    }
    Ok(())
}

fn process_notification_ingest_outcome(
    report: &mut SessionBridgeReport,
    outcome: NotificationIngestOutcome,
    monotonic_ms: u64,
) {
    report.events.push(SessionBridgeEvent::NotificationIngest {
        monotonic_ms,
        outcome,
    });
}

fn process_device_event(report: &mut SessionBridgeReport, event: DeviceEvent, monotonic_ms: u64) {
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

#[async_trait]
impl SessionPeripheral for btleplug::platform::Peripheral {
    fn mtu(&self) -> u16 {
        btleplug::api::Peripheral::mtu(self)
    }

    async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), BtleError> {
        btleplug::api::Peripheral::subscribe(self, characteristic)
            .await
            .map_err(BtleError::from)
    }

    async fn write(
        &self,
        characteristic: &Characteristic,
        bytes: &[u8],
        mode: WriteMode,
    ) -> Result<(), BtleError> {
        btleplug::api::Peripheral::write(
            self,
            characteristic,
            bytes,
            match mode {
                WriteMode::WithResponse => btleplug::api::WriteType::WithResponse,
                WriteMode::WithoutResponse => btleplug::api::WriteType::WithoutResponse,
            },
        )
        .await
        .map_err(BtleError::from)
    }

    async fn notifications(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ValueNotification> + Send>>, BtleError> {
        btleplug::api::Peripheral::notifications(self)
            .await
            .map_err(BtleError::from)
    }

    async fn disconnect(&self) -> Result<(), BtleError> {
        btleplug::api::Peripheral::disconnect(self)
            .await
            .map_err(BtleError::from)
    }
}

async fn first_adapter() -> Result<Adapter, BtleError> {
    let manager = Manager::new().await?;
    let mut adapters = backend_call("list adapters", manager.adapters()).await?;
    adapters.pop().ok_or(BtleError::NoAdapterAvailable)
}

async fn collect_observations(adapter: &Adapter) -> Result<Vec<PeripheralObservation>, BtleError> {
    let mut observations = Vec::new();
    for peripheral in backend_call("list peripherals", adapter.peripherals()).await? {
        if let Some(properties) =
            backend_call("read peripheral properties", peripheral.properties()).await?
        {
            observations.push(PeripheralObservation::from_peripheral(
                &peripheral,
                properties,
            ));
        }
    }
    Ok(observations)
}

async fn observation_from_peripheral(
    peripheral: &btleplug::platform::Peripheral,
) -> Result<PeripheralObservation, BtleError> {
    let Some(properties) =
        backend_call("read peripheral properties", peripheral.properties()).await?
    else {
        return Ok(PeripheralObservation {
            identifier: peripheral.id().to_string(),
            address: None,
            name: None,
            rssi: None,
            advertised_services: AdvertisedServices::new(),
            manufacturer_data: ManufacturerDataSummaries::new(),
        });
    };
    Ok(PeripheralObservation::from_peripheral(
        peripheral, properties,
    ))
}

async fn find_peripheral(
    adapter: &Adapter,
    target: &ConnectionTarget,
) -> Result<btleplug::platform::Peripheral, BtleError> {
    for peripheral in backend_call("list peripherals", adapter.peripherals()).await? {
        let Some(properties) =
            backend_call("read peripheral properties", peripheral.properties()).await?
        else {
            continue;
        };
        let observation = PeripheralObservation::from_peripheral(&peripheral, properties);
        if target.matches(&observation) {
            return Ok(peripheral);
        }
    }
    Err(BtleError::NoPeripheralMatched)
}

async fn wait_for_scan_match<T, F, Fut>(
    scan_for: Duration,
    poll_interval: Duration,
    mut find: F,
) -> Result<T, BtleError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, BtleError>>,
{
    let started = tokio::time::Instant::now();
    let deadline = started + scan_for;

    loop {
        match find().await {
            Ok(value) => return Ok(value),
            Err(BtleError::NoPeripheralMatched) if tokio::time::Instant::now() < deadline => {
                let next_poll = (tokio::time::Instant::now() + poll_interval).min(deadline);
                tokio::time::sleep_until(next_poll).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn backend_call<T, F>(operation: &'static str, future: F) -> Result<T, BtleError>
where
    F: Future<Output = Result<T, btleplug::Error>>,
{
    tokio::time::timeout(BACKEND_OPERATION_TIMEOUT, future)
        .await
        .map_err(|_| BtleError::OperationTimedOut {
            operation,
            after: BACKEND_OPERATION_TIMEOUT,
        })?
        .map_err(BtleError::Backend)
}

#[cfg(test)]
mod tests {
    use std::{
        pin::Pin,
        sync::{Arc, Mutex},
        time::{Duration, Instant as StdInstant},
    };

    use btleplug::api::{CharPropFlags, Characteristic, ValueNotification};
    use cutout_core::{
        DeviceCommand, DeviceEvent, DiagnosticError, FirmwareInfo, GattChannel, Measured,
        NotificationByteLen, NotificationIngestOutcome, ParserDiagnostics, ParserError,
        ParserGapEvidence, PayloadBodyLen, PevcapDirection, PevcapResolvedIdentity, ProtocolFamily,
        ProtocolSelector, ProtocolSession, RawFieldValue, ReadOnlyResponse,
        ReservedPayloadEvidence, SemanticEventCount, SessionInput, SessionOutput, SettingsEntry,
        SettingsReadback, TelemetryDelta, TransportAction, ValueQuality, ValueSource,
        VerificationStatus, VerifiedValue, WriteMode,
    };
    use cutout_protocols::IdentityConfidence;
    use futures_util::stream;
    use smallvec::smallvec;
    use uuid::Uuid;

    use super::crate_name;

    type WriteRecord = (Uuid, Vec<u8>, WriteMode);
    type WriteLog = Arc<Mutex<Vec<WriteRecord>>>;
    type NotificationLog = Arc<Mutex<Vec<ValueNotification>>>;

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-btle");
    }

    #[test]
    fn peripheral_identifier_is_a_typed_backend_handle() {
        let identifier = crate::PeripheralIdentifier::new("platform-id-7");

        assert_eq!(identifier.as_str(), "platform-id-7");
        assert_eq!(identifier.to_string(), "platform-id-7");
        assert_eq!(identifier.into_inner(), "platform-id-7");
    }

    #[test]
    fn bluetooth_address_filters_platform_null_placeholder() {
        assert_eq!(crate::BluetoothAddress::new("00:00:00:00:00:00"), None);

        let address = crate::BluetoothAddress::new("AA:BB:CC:DD:EE:FF").expect("valid address");
        assert_eq!(address.as_str(), "AA:BB:CC:DD:EE:FF");
        assert_eq!(address.to_string(), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn known_uuid_fields_classify_to_zst_markers() {
        let battery_service = crate::ServiceSummary {
            uuid: <crate::StandardBatteryService as crate::KnownGattUuid>::UUID,
            primary: true,
            characteristics: Vec::new(),
        };
        let battery_level = crate::CharacteristicSummary {
            uuid: <crate::StandardBatteryLevelCharacteristic as crate::KnownGattUuid>::UUID,
            service_uuid: <crate::StandardBatteryService as crate::KnownGattUuid>::UUID,
            properties: CharPropFlags::READ,
        };

        assert_eq!(
            battery_service.gatt_uuid(),
            crate::GattUuid::StandardBatteryService(crate::StandardBatteryService)
        );
        assert_eq!(
            battery_level.gatt_uuid(),
            crate::GattUuid::StandardBatteryLevelCharacteristic(
                crate::StandardBatteryLevelCharacteristic
            )
        );
        assert_eq!(
            battery_level.service_gatt_uuid(),
            crate::GattUuid::StandardBatteryService(crate::StandardBatteryService)
        );
    }

    #[test]
    fn peripheral_observation_exposes_typed_identity_views() {
        let observation = crate::PeripheralObservation {
            identifier: "backend-42".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: None,
            rssi: None,
            advertised_services: crate::AdvertisedServices::new(),
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        };

        let identifier = observation.platform_identifier();
        assert_eq!(identifier.as_str(), "backend-42");
        assert_eq!(
            identifier.as_str().as_ptr(),
            observation.identifier.as_ptr()
        );

        let address = observation
            .bluetooth_address()
            .expect("address is normalized");
        assert_eq!(
            address.as_str().as_ptr(),
            observation.address.as_deref().expect("address").as_ptr()
        );
        assert_eq!(address.as_str(), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn peripheral_observation_classifies_advertised_services() {
        let unknown = Uuid::from_u128(0x6e40_0003_b5a3_f393_e0a9_e50e_24dc_ca9e);
        let observation = crate::PeripheralObservation {
            identifier: "backend-42".to_owned(),
            address: None,
            name: None,
            rssi: None,
            advertised_services: smallvec![
                <crate::SharedFfe0Service as crate::KnownGattUuid>::UUID,
                unknown,
            ],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        };

        let services = observation.advertised_service_uuids().collect::<Vec<_>>();

        assert_eq!(
            services,
            vec![
                crate::GattUuid::SharedFfe0Service(crate::SharedFfe0Service),
                crate::GattUuid::Other(unknown),
            ]
        );
        assert!(observation.advertises::<crate::SharedFfe0Service>());
        assert!(!observation.advertises::<crate::StandardBatteryService>());
    }

    #[test]
    fn connection_target_matches_on_address_and_name() {
        let target = crate::ConnectionTarget {
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            identifier: None,
            name_contains: Some("Aero".to_owned()),
        };
        let observation = crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            name: Some("NOSFET Aero".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        };

        assert!(target.matches(&observation));
    }

    #[test]
    fn btleplug_reconnect_host_reuses_target_and_scan_duration() {
        let target = crate::ConnectionTarget {
            address: None,
            identifier: Some("corebluetooth-id".to_owned()),
            name_contains: Some("NF2557".to_owned()),
        };

        let host = crate::BtleplugReconnectHost::new(target.clone(), Duration::from_secs(7));

        assert_eq!(host.target(), &target);
        assert_eq!(host.scan_for(), Duration::from_secs(7));
    }

    #[tokio::test]
    async fn targeted_scan_wait_returns_as_soon_as_match_is_found() {
        let started = StdInstant::now();
        let mut attempts = 0_u8;

        let result = crate::wait_for_scan_match(
            Duration::from_millis(200),
            Duration::from_millis(5),
            || {
                attempts += 1;
                async move {
                    if attempts == 1 {
                        Err(crate::BtleError::NoPeripheralMatched)
                    } else {
                        Ok("matched")
                    }
                }
            },
        )
        .await
        .expect("second poll matches");

        assert_eq!(result, "matched");
        assert_eq!(attempts, 2);
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "targeted scan should not wait for the full scan period after a match"
        );
    }

    #[tokio::test]
    async fn targeted_scan_wait_times_out_without_match() {
        let started = StdInstant::now();
        let mut attempts = 0_u8;

        let result = crate::wait_for_scan_match::<(), _, _>(
            Duration::from_millis(15),
            Duration::from_millis(5),
            || {
                attempts += 1;
                async { Err(crate::BtleError::NoPeripheralMatched) }
            },
        )
        .await;

        assert!(matches!(result, Err(crate::BtleError::NoPeripheralMatched)));
        assert!(attempts >= 2);
        assert!(started.elapsed() >= Duration::from_millis(15));
    }

    #[tokio::test]
    async fn targeted_scan_wait_returns_non_match_errors_immediately() {
        let started = StdInstant::now();
        let mut attempts = 0_u8;

        let result = crate::wait_for_scan_match::<(), _, _>(
            Duration::from_millis(200),
            Duration::from_millis(5),
            || {
                attempts += 1;
                async {
                    Err(crate::BtleError::Bridge(
                        crate::SessionBridgeError::MissingNotifyEndpoint {
                            channel: GattChannel::from_bytes([0x44; 16]),
                        },
                    ))
                }
            },
        )
        .await;

        assert!(matches!(result, Err(crate::BtleError::Bridge(_))));
        assert_eq!(attempts, 1);
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn connection_target_matches_on_platform_identifier() {
        let target = crate::ConnectionTarget {
            address: None,
            identifier: Some("cb-uuid-1234".to_owned()),
            name_contains: None,
        };
        let observation = crate::PeripheralObservation {
            identifier: "cb-uuid-1234".to_owned(),
            address: None,
            name: Some("NF2557".to_owned()),
            rssi: Some(-42),
            advertised_services: smallvec![],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        };

        assert!(target.matches(&observation));
    }

    #[test]
    fn peripheral_observation_renders_manufacturer_data_without_payload_bytes() {
        let observation = crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: None,
            name: Some("Generic".to_owned()),
            rssi: Some(-60),
            advertised_services: smallvec![],
            manufacturer_data: smallvec![
                crate::ManufacturerDataSummary {
                    company_id: 0x004c,
                    len: 6,
                },
                crate::ManufacturerDataSummary {
                    company_id: 0x000f,
                    len: 2,
                },
            ],
        };

        assert_eq!(
            observation.to_string(),
            "id=peripheral-id name=Generic rssi=-60 services=[] manufacturer_data=[004c:6b,000f:2b] family_hints=[]"
        );
    }

    #[test]
    fn peripheral_observation_renders_family_hints_as_non_final_evidence() {
        let observation = crate::PeripheralObservation {
            identifier: "peripheral-id".to_owned(),
            address: None,
            name: Some("NF2557".to_owned()),
            rssi: None,
            advertised_services: smallvec![Uuid::from_u128(
                0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb
            )],
            manufacturer_data: crate::ManufacturerDataSummaries::new(),
        };

        assert_eq!(
            observation.to_string(),
            "id=peripheral-id name=NF2557 services=[0000ffe0-0000-1000-8000-00805f9b34fb] manufacturer_data=[] family_hints=[shared-ffe0-ffe1,name-nosfet-aero]"
        );
    }

    #[test]
    fn operation_timeout_error_names_the_backend_operation() {
        let error = crate::BtleError::OperationTimedOut {
            operation: "start scan",
            after: Duration::from_secs(10),
        };

        assert_eq!(
            error.to_string(),
            "bluetooth operation timed out: start scan after 10s"
        );
    }

    #[test]
    fn btle_errors_expose_actionable_desktop_diagnostic_hints() {
        let no_adapter = crate::BtleError::NoAdapterAvailable;
        assert_eq!(
            no_adapter.diagnostic_hint(),
            "enable Bluetooth, grant Bluetooth permission to this terminal, and verify the OS exposes an adapter"
        );

        let no_match = crate::BtleError::NoPeripheralMatched;
        assert_eq!(
            no_match.diagnostic_hint(),
            "power on the device, keep it nearby, increase --seconds, or use --name-contains/--identifier to narrow selection"
        );

        let timed_out = crate::BtleError::OperationTimedOut {
            operation: "discover services",
            after: Duration::from_secs(5),
        };
        assert_eq!(
            timed_out.diagnostic_hint(),
            "retry the operation, move closer to the device, and check whether another app is holding the BLE connection"
        );

        let missing_endpoint =
            crate::BtleError::Bridge(crate::SessionBridgeError::MissingSessionEndpoint);
        assert_eq!(
            missing_endpoint.diagnostic_hint(),
            "inspect GATT services and select a device exposing a writable and notify-capable session characteristic"
        );
    }

    #[test]
    fn connection_summary_renders_services_and_characteristics() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                }],
            }],
        };

        assert!(summary.to_string().contains("AA:BB:CC:DD:EE:FF"));
        assert!(summary.to_string().contains("NOSFET Aero"));
        assert!(summary.to_string().contains("ffe0"));
        assert!(summary.to_string().contains("ffe1"));
    }

    #[test]
    fn connection_summary_selects_explicit_notify_characteristic() {
        let requested = Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb);
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: None,
                name: Some("Raw device".to_owned()),
                rssi: None,
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE_WITHOUT_RESPONSE,
                    },
                    crate::CharacteristicSummary {
                        uuid: requested,
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        assert_eq!(
            summary
                .select_notify_characteristic(Some(requested))
                .map(|characteristic| characteristic.uuid),
            Some(requested)
        );
    }

    #[test]
    fn connection_summary_rejects_explicit_non_notify_characteristic() {
        let requested = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: None,
                name: Some("Raw device".to_owned()),
                rssi: None,
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: requested,
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE_WITHOUT_RESPONSE,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        assert!(
            summary
                .select_notify_characteristic(Some(requested))
                .is_none()
        );
    }

    #[tokio::test]
    async fn raw_notification_capture_subscribes_and_filters_selected_characteristic() {
        let service = Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb);
        let selected = Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb);
        let other = Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb);
        let peripheral = RecordingPeripheral::with_notifications(vec![
            ValueNotification {
                uuid: other,
                service_uuid: service,
                value: vec![0x99],
            },
            ValueNotification {
                uuid: selected,
                service_uuid: service,
                value: vec![0x01, 0x02, 0x03],
            },
        ]);
        let characteristic = crate::CharacteristicSummary {
            uuid: selected,
            service_uuid: service,
            properties: CharPropFlags::NOTIFY,
        };

        let records = crate::capture_raw_notifications(
            &peripheral,
            &characteristic,
            Duration::from_millis(5),
        )
        .await
        .expect("raw notification capture succeeds");

        assert_eq!(
            peripheral
                .subscribes
                .lock()
                .expect("subscribe log")
                .as_slice(),
            &[selected]
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].characteristic, selected);
        assert_eq!(records[0].service, service);
        assert_eq!(records[0].bytes, [0x01, 0x02, 0x03]);
    }

    #[test]
    fn connection_summary_selects_standard_battery_level_characteristic() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_180f_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::READ,
                }],
            }],
        };

        assert_eq!(
            summary
                .battery_level_characteristic()
                .map(|characteristic| characteristic.uuid),
            Some(Uuid::from_u128(0x0000_2a19_0000_1000_8000_0080_5f9b_34fb))
        );
    }

    #[test]
    fn connection_summary_uses_identifier_when_address_is_unavailable() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "cb-uuid-1234".to_owned(),
                address: None,
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![],
        };

        assert!(summary.to_string().contains("id=cb-uuid-1234"));
        assert!(summary.to_string().contains("name=NOSFET Aero"));
    }

    #[test]
    fn capture_record_formats_write_bytes_with_provenance() {
        let record = crate::SessionCaptureRecord::Write {
            monotonic_ms: 7,
            characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            mode: WriteMode::WithoutResponse,
            bytes: vec![0x01, 0x23, 0xab, 0xcd],
            provisional: true,
        };

        assert_eq!(
            record.to_string(),
            "write t_ms=7 characteristic=0000ffe1-0000-1000-8000-00805f9b34fb mode=without-response bytes=0123abcd provisional=true"
        );
    }

    #[test]
    fn capture_record_formats_notification_bytes_with_service() {
        let record = crate::SessionCaptureRecord::Notification {
            monotonic_ms: 11,
            characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
            service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
        };

        assert_eq!(
            record.to_string(),
            "notification t_ms=11 characteristic=0000ffe1-0000-1000-8000-00805f9b34fb service=0000ffe0-0000-1000-8000-00805f9b34fb bytes=deadbeef"
        );
    }

    #[test]
    fn session_capture_converts_to_pevcap_with_summary_metadata() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "cb-uuid".to_owned(),
                address: None,
                name: Some("NF2557".to_owned()),
                rssi: Some(-67),
                advertised_services: smallvec![Uuid::from_u128(
                    0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
                )],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }],
            }],
        };
        let capture = crate::SessionCapture {
            records: vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(23),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 1,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteMode::WithoutResponse,
                    bytes: b"N".to_vec(),
                    provisional: false,
                },
                crate::SessionCaptureRecord::Notification {
                    monotonic_ms: 3,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    bytes: b"NAME=NF2557".to_vec(),
                },
                crate::SessionCaptureRecord::LinkDown { monotonic_ms: 4 },
            ],
            report: crate::SessionBridgeReport::default(),
        };

        let pevcap = capture
            .to_pevcap(
                &summary,
                crate::PevcapSessionMetadata {
                    wall_clock_start_unix_ms: 1_725_000_123_456,
                    platform_id: "darwin",
                    library_version: "0.1.0",
                    registry_hash: [0x42; 32],
                    resolved_identity: Some(PevcapResolvedIdentity {
                        protocol_family: Some(ProtocolFamily::VeteranLeaperkimNosfet),
                        model: Some(VerifiedValue {
                            value: "NOSFET Aero".to_owned(),
                            verification: VerificationStatus::Inferred,
                        }),
                        firmware: None,
                    }),
                    annotations: &["live aero"],
                },
            )
            .expect("session capture converts to PEVCAP");

        assert_eq!(pevcap.header.wall_clock_start_unix_ms, 1_725_000_123_456);
        assert_eq!(pevcap.header.platform_id, "darwin");
        assert_eq!(pevcap.header.write_limit, Some(23));
        assert_eq!(pevcap.header.advertised_services.len(), 1);
        assert_eq!(pevcap.header.gatt_fingerprints.len(), 1);
        assert_eq!(
            pevcap
                .header
                .resolved_identity
                .as_ref()
                .and_then(|identity| identity.model.as_ref().map(|model| model.value.as_str())),
            Some("NOSFET Aero")
        );
        assert_eq!(pevcap.records.len(), 4);
        assert_eq!(pevcap.records[0].direction, PevcapDirection::LinkUp);
        assert_eq!(pevcap.records[0].monotonic_ms, 0);
        assert_eq!(pevcap.records[0].link_max_write_len, Some(23));
        assert_eq!(pevcap.records[1].direction, PevcapDirection::Outbound);
        assert_eq!(
            pevcap.records[1].write_mode,
            Some(WriteMode::WithoutResponse)
        );
        assert_eq!(pevcap.records[1].bytes, b"N");
        assert_eq!(pevcap.records[2].direction, PevcapDirection::Inbound);
        assert_eq!(
            pevcap.records[2].service,
            pevcap.header.advertised_services.first().copied()
        );
        assert_eq!(pevcap.records[2].bytes, b"NAME=NF2557");
        assert_eq!(pevcap.records[3].direction, PevcapDirection::LinkDown);
        assert_eq!(pevcap.records[3].monotonic_ms, 4);
    }

    #[test]
    fn session_capture_pevcap_conversion_preserves_write_response_mode() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("Begode_Falcon".to_owned()),
                rssi: None,
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![],
        };
        let capture = crate::SessionCapture {
            records: vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: None,
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 1,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteMode::WithResponse,
                    bytes: vec![0x01, 0x02],
                    provisional: true,
                },
            ],
            report: crate::SessionBridgeReport::default(),
        };

        let pevcap = capture
            .to_pevcap(
                &summary,
                crate::PevcapSessionMetadata {
                    wall_clock_start_unix_ms: 1,
                    platform_id: "test",
                    library_version: "0.1.0",
                    registry_hash: [0; 32],
                    resolved_identity: None,
                    annotations: &[],
                },
            )
            .expect("session capture converts to PEVCAP");

        assert_eq!(pevcap.header.write_limit, None);
        assert_eq!(pevcap.records.len(), 2);
        assert_eq!(pevcap.records[0].direction, PevcapDirection::LinkUp);
        assert_eq!(pevcap.records[1].write_mode, Some(WriteMode::WithResponse));
        assert_eq!(pevcap.records[1].bytes, [0x01, 0x02]);
    }

    #[test]
    fn connection_summary_finds_write_and_notify_candidates() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::READ,
                    },
                ],
            }],
        };

        assert_eq!(summary.write_candidates().count(), 1);
        assert_eq!(summary.notify_candidates().count(), 1);
    }

    #[test]
    fn connection_summary_selects_session_endpoints() {
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::INDICATE,
                    },
                ],
            }],
        };

        let endpoints = summary
            .select_session_endpoints()
            .expect("summary has a writable characteristic");
        assert_eq!(
            endpoints.write.uuid,
            Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb)
        );
        assert_eq!(
            endpoints
                .notify
                .expect("summary has a notify-capable characteristic")
                .uuid,
            Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb)
        );
    }

    #[tokio::test]
    async fn drive_session_reports_hints_only_identity_from_name_and_shared_gatt() {
        let peripheral = RecordingPeripheral::default();
        let mut session = SubscribeOnlySession;
        let summary = begode_falcon_summary("Falcon");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes(
                *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
            ),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
        )
        .await
        .expect("bridge reports staged identity hints");

        let identity = report.identity.expect("identity hints are reported");
        assert_eq!(identity.confidence, IdentityConfidence::HintsOnly);
        assert_eq!(identity.model, None);
        assert_eq!(identity.manufacturer, None);
        assert!(identity.evidence.has_advertised_name_hint());
        assert!(identity.evidence.has_gatt_hint());
        assert_eq!(peripheral.writes.lock().expect("write log").len(), 0);
    }

    #[tokio::test]
    async fn drive_session_resolves_falcon_after_family_and_name_banner_notifications() {
        let peripheral = RecordingPeripheral::with_notifications(vec![
            ValueNotification {
                uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                value: vec![0x55, 0xaa, 0, 0],
            },
            ValueNotification {
                uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                value: b"NAME=Falcon".to_vec(),
            },
        ]);
        let mut session = SubscribeOnlySession;
        let summary = begode_falcon_summary("Begode_Falcon");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes(
                *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
            ),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
        )
        .await
        .expect("bridge resolves Falcon identity");

        let identity = report.identity.expect("model identity is reported");
        assert_eq!(identity.confidence, IdentityConfidence::Model);
        assert_eq!(identity.manufacturer, Some("Begode"));
        assert_eq!(identity.model, Some("Falcon"));
        assert!(identity.evidence.has_passive_family_match());
        assert!(identity.evidence.has_banner_model_match());
        assert_eq!(report.notifications, 2);
        assert_eq!(peripheral.writes.lock().expect("write log").len(), 0);
    }

    #[tokio::test]
    async fn drive_session_subscribes_and_writes_matching_transport_channels() {
        let peripheral = RecordingPeripheral::default();
        let mut session = BridgeSession::default();
        let summary = crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NOSFET Aero".to_owned()),
                rssi: Some(-42),
                advertised_services: smallvec![],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        };

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
        )
        .await
        .expect("bridge accepts matching transport outputs");

        assert_eq!(report.writes, 1);
        assert_eq!(report.subscribes, 1);
        assert_eq!(report.notifications, 0);
        assert_eq!(
            peripheral
                .subscribes
                .lock()
                .expect("subscribe log")
                .as_slice(),
            &[Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb)]
        );
        assert_eq!(
            peripheral.writes.lock().expect("write log").as_slice(),
            &[(
                Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                b"bridge:write".to_vec(),
                WriteMode::WithResponse,
            )]
        );
    }

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn drive_session_relays_notifications_back_into_session() {
        let peripheral = RecordingPeripheral::with_notification(ValueNotification {
            uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
            service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            value: vec![0x13, 0x37],
        });
        let mut session = BridgeSession::default();
        let summary = shared_write_notify_summary("NOSFET Aero");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
        )
        .await
        .expect("bridge consumes notifications");

        assert_eq!(report.notifications, 1);
        assert_eq!(report.notification_bytes, 2);
        assert_eq!(report.latest_notification_len, Some(2));
        assert_eq!(
            *session
                .last_notification_channel
                .lock()
                .expect("notification channel"),
            Some(GattChannel::from_bytes([0xA1; 16]))
        );
        assert_eq!(report.telemetry, 1);
        assert_eq!(report.read_only_responses, 2);
        assert_eq!(
            report.telemetry_snapshot.speed_mm_s,
            Some(Measured::reported(1_200))
        );
        assert_eq!(
            report.telemetry_snapshot.voltage_mv,
            Some(Measured::reported(84_200))
        );
        assert_eq!(
            report.telemetry_snapshot.battery_percent_estimated,
            Some(Measured::estimated(61))
        );
        assert_eq!(
            report.firmware.expect("firmware response").firmware_major,
            Some(Measured::reported(43))
        );
        assert_eq!(
            report.settings.first().expect("settings response").entries[0]
                .expect("settings entry")
                .field,
            RawFieldValue::new(0x0014, 30)
        );
        assert_eq!(report.diagnostics, 1);
        assert_eq!(report.diagnostics_snapshot.malformed_frames, 1);
        assert_eq!(
            report.diagnostic_errors.as_slice(),
            &[DiagnosticError::from_parser_error(
                ParserError::MalformedFrame
            )]
        );
        assert!(report.events.iter().all(|event| {
            matches!(
                event,
                crate::SessionBridgeEvent::ProcessedTelemetry { .. }
                    | crate::SessionBridgeEvent::ReadOnlyResponse { .. }
                    | crate::SessionBridgeEvent::Diagnostics { .. }
                    | crate::SessionBridgeEvent::DiagnosticError { .. }
                    | crate::SessionBridgeEvent::NotificationIngest { .. }
                    | crate::SessionBridgeEvent::LinkDown { .. }
            )
        }));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: 2,
                ..
            }
        )));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::Diagnostics {
                monotonic_ms: 2,
                diagnostics,
            } if diagnostics.malformed_frames == 1
        )));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms: 2,
                response: ReadOnlyResponse::Firmware(firmware),
            } if firmware.firmware_major == Some(Measured::reported(43))
        )));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms: 2,
                response: ReadOnlyResponse::Settings(settings),
            } if settings.entries[0].is_some()
        )));
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::SessionBridgeEvent::DiagnosticError {
                monotonic_ms: 2,
                error,
            } if error.kind == cutout_core::DiagnosticErrorKind::MalformedFrame
        )));
        assert_eq!(*session.notification_count.lock().expect("count"), 1);
    }

    #[test]
    fn parsed_notifications_are_not_eligible_for_raw_transport_logging() {
        let outputs = [SessionOutput::NotificationIngest(
            NotificationIngestOutcome::semantic_events(
                ProtocolFamily::VeteranLeaperkimNosfet,
                GattChannel::from_bytes([0xA1; 16]),
                NotificationByteLen::new(77),
                7,
                SemanticEventCount::new(5),
            ),
        )];

        assert_eq!(
            crate::notification_decode_outcome(&outputs),
            crate::NotificationDecodeOutcome::SemanticEvents
        );
    }

    #[test]
    fn accepted_fragment_notifications_are_reported_as_buffered_decoder_input() {
        let outputs = [SessionOutput::NotificationIngest(
            NotificationIngestOutcome::buffered_fragment(
                ProtocolFamily::VeteranLeaperkimNosfet,
                GattChannel::from_bytes([0xA1; 16]),
                NotificationByteLen::new(20),
                3,
            ),
        )];

        assert_eq!(
            crate::notification_decode_outcome(&outputs),
            crate::NotificationDecodeOutcome::BufferedFragment
        );
    }

    #[test]
    fn ignored_notifications_remain_eligible_for_debug_transport_logging() {
        let outputs = [SessionOutput::NotificationIngest(
            NotificationIngestOutcome::ignored_wrong_channel(
                GattChannel::from_bytes([0xA1; 16]),
                NotificationByteLen::new(20),
                3,
            ),
        )];

        assert_eq!(
            crate::notification_decode_outcome(&outputs),
            crate::NotificationDecodeOutcome::Ignored
        );
    }

    #[test]
    fn drive_session_reports_fragment_notifications_as_typed_ingest_events() {
        let mut report = crate::SessionBridgeReport::default();
        let outcome = NotificationIngestOutcome::buffered_fragment(
            ProtocolFamily::VeteranLeaperkimNosfet,
            GattChannel::from_bytes([0xA1; 16]),
            NotificationByteLen::new(20),
            3,
        );

        crate::process_notification_ingest_outcome(&mut report, outcome, 3);

        assert_eq!(
            report.events.as_slice(),
            &[crate::SessionBridgeEvent::NotificationIngest {
                monotonic_ms: 3,
                outcome,
            }]
        );
    }

    #[test]
    fn semantic_notifications_suppress_transport_logging_without_raw_notification_event() {
        let outputs = [
            SessionOutput::NotificationIngest(NotificationIngestOutcome::semantic_events(
                ProtocolFamily::VeteranLeaperkimNosfet,
                GattChannel::from_bytes([0xA1; 16]),
                NotificationByteLen::new(77),
                3,
                SemanticEventCount::new(5),
            )),
            SessionOutput::Event(DeviceEvent::Telemetry(TelemetryDelta {
                voltage_mv: Some(Measured::reported(126_000)),
                ..TelemetryDelta::empty(0)
            })),
        ];

        assert_eq!(
            crate::notification_decode_outcome(&outputs),
            crate::NotificationDecodeOutcome::SemanticEvents
        );
    }

    #[test]
    fn known_reserved_and_parser_gap_notifications_have_distinct_decode_outcomes() {
        let channel = GattChannel::from_bytes([0xA1; 16]);
        let reserved = [SessionOutput::NotificationIngest(
            NotificationIngestOutcome::known_reserved(
                ProtocolFamily::VeteranLeaperkimNosfet,
                channel,
                NotificationByteLen::new(75),
                4,
                ReservedPayloadEvidence {
                    selector: Some(ProtocolSelector::new(8)),
                    tag: None,
                    body_len: PayloadBodyLen::new(24),
                    verification: VerificationStatus::HardwareVerified,
                },
            ),
        )];
        let gap = [SessionOutput::NotificationIngest(
            NotificationIngestOutcome::parser_gap(
                ProtocolFamily::VeteranLeaperkimNosfet,
                channel,
                NotificationByteLen::new(77),
                5,
                ParserGapEvidence {
                    selector: Some(ProtocolSelector::new(9)),
                    tag: None,
                    body_len: PayloadBodyLen::new(26),
                },
            ),
        )];
        let diagnostic = [SessionOutput::NotificationIngest(
            NotificationIngestOutcome::parser_diagnostic(
                ProtocolFamily::VeteranLeaperkimNosfet,
                channel,
                NotificationByteLen::new(77),
                6,
                ParserError::BadChecksum,
            ),
        )];

        assert_eq!(
            crate::notification_decode_outcome(&reserved),
            crate::NotificationDecodeOutcome::KnownReserved
        );
        assert_eq!(
            crate::notification_decode_outcome(&gap),
            crate::NotificationDecodeOutcome::ParserGap
        );
        assert_eq!(
            crate::notification_decode_outcome(&diagnostic),
            crate::NotificationDecodeOutcome::ParserDiagnostic
        );
    }

    #[tokio::test]
    async fn capture_session_records_subscribe_write_and_notification_bytes() {
        let peripheral = RecordingPeripheral::with_notification(ValueNotification {
            uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
            service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
            value: vec![0x13, 0x37],
        });
        let mut session = BridgeSession::default();
        let summary = shared_write_notify_summary("NOSFET Aero");

        let capture = crate::capture_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::from_millis(10),
            true,
        )
        .await
        .expect("capture consumes bridge outputs");

        assert_eq!(capture.report.notifications, 1);
        assert_eq!(
            capture.records,
            vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 0,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 1,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteMode::WithResponse,
                    bytes: b"bridge:write".to_vec(),
                    provisional: true,
                },
                crate::SessionCaptureRecord::Notification {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                    service: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    bytes: vec![0x13, 0x37],
                },
            ]
        );
    }

    #[tokio::test]
    async fn capture_session_with_commands_records_command_writes_before_tick() {
        let peripheral = RecordingPeripheral::default();
        let mut session = CommandWriteSession;
        let summary = begode_falcon_summary("Begode_Falcon");

        let capture = crate::capture_session_with_commands(
            &peripheral,
            &mut session,
            GattChannel::from_bytes(
                *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
            ),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
            &[
                DeviceCommand::RequestIdentity,
                DeviceCommand::RequestFirmwareInfo,
            ],
        )
        .await
        .expect("capture records explicit command writes");

        assert_eq!(capture.report.writes, 2);
        assert_eq!(
            capture.records,
            vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 0,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 1,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteMode::WithoutResponse,
                    bytes: b"N".to_vec(),
                    provisional: false,
                },
                crate::SessionCaptureRecord::Write {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    mode: WriteMode::WithoutResponse,
                    bytes: b"V".to_vec(),
                    provisional: false,
                },
            ]
        );
    }

    #[tokio::test]
    async fn capture_session_chunks_writes_by_negotiated_write_limit() {
        let peripheral = RecordingPeripheral::with_mtu(4);
        let mut session = LargeWriteSession;
        let summary = shared_write_notify_summary("NOSFET Aero");

        let capture = crate::capture_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
            false,
        )
        .await
        .expect("capture chunks oversized bridge writes");

        assert_eq!(capture.report.protocol_writes, 1);
        assert_eq!(capture.report.writes, 3);
        assert_eq!(
            peripheral.writes.lock().expect("write log").as_slice(),
            &[
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"0123".to_vec(),
                    WriteMode::WithoutResponse,
                ),
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"4567".to_vec(),
                    WriteMode::WithoutResponse,
                ),
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"89".to_vec(),
                    WriteMode::WithoutResponse,
                ),
            ]
        );
        let writes: Vec<_> = capture
            .records
            .iter()
            .filter_map(|record| match record {
                crate::SessionCaptureRecord::Write { bytes, mode, .. } => {
                    Some((bytes.as_slice(), *mode))
                }
                crate::SessionCaptureRecord::Link { .. }
                | crate::SessionCaptureRecord::LinkDown { .. }
                | crate::SessionCaptureRecord::Subscribe { .. }
                | crate::SessionCaptureRecord::Notification { .. } => None,
            })
            .collect();
        assert_eq!(
            writes,
            vec![
                (b"0123".as_slice(), WriteMode::WithoutResponse),
                (b"4567".as_slice(), WriteMode::WithoutResponse),
                (b"89".as_slice(), WriteMode::WithoutResponse),
            ]
        );
    }

    #[tokio::test]
    async fn drive_session_feeds_link_down_after_intentional_disconnect() {
        let peripheral = RecordingPeripheral::default();
        let mut session = DisconnectOnTickSession::default();
        let summary = shared_write_notify_summary("NOSFET Aero");

        let report = crate::drive_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
        )
        .await
        .expect("bridge handles intentional disconnect");

        assert_eq!(report.disconnects, 1);
        assert_eq!(
            report.events.as_slice(),
            &[crate::SessionBridgeEvent::LinkDown { monotonic_ms: 1 }]
        );
        assert_eq!(*session.link_down_count.lock().expect("count"), 1);
        assert_eq!(*peripheral.disconnects.lock().expect("disconnect log"), 1);
    }

    #[tokio::test]
    async fn capture_session_records_link_down_after_intentional_disconnect() {
        let peripheral = RecordingPeripheral::default();
        let mut session = DisconnectOnTickSession::default();
        let summary = shared_write_notify_summary("NOSFET Aero");

        let capture = crate::capture_session(
            &peripheral,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            &summary,
            summary
                .select_session_endpoints()
                .expect("summary has session endpoints"),
            Duration::ZERO,
            false,
        )
        .await
        .expect("capture records intentional disconnect");

        assert_eq!(
            capture.records,
            vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::LinkDown { monotonic_ms: 1 },
            ]
        );
        assert_eq!(capture.report.disconnects, 1);
    }

    #[tokio::test]
    async fn capture_reconnecting_session_restores_subscription_after_disconnect() {
        let first = RecordingPeripheral::default();
        let second = RecordingPeripheral::default();
        let mut host = FakeReconnectHost::new(vec![first.clone(), second.clone()]);
        let mut session = ReconnectOnceSession::default();

        let reconnecting_capture = crate::capture_reconnecting_session_with_summaries(
            &mut host,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            Duration::ZERO,
            2,
            false,
        )
        .await
        .expect("fake host reconnects once");
        let capture = reconnecting_capture.capture;

        assert_eq!(host.connects, 2);
        assert_eq!(reconnecting_capture.attempts.len(), 2);
        assert_eq!(reconnecting_capture.attempts[0].attempt, 1);
        assert_eq!(reconnecting_capture.attempts[0].report.subscribes, 1);
        assert_eq!(reconnecting_capture.attempts[0].report.disconnects, 1);
        assert_eq!(reconnecting_capture.attempts[1].attempt, 2);
        assert_eq!(reconnecting_capture.attempts[1].report.subscribes, 1);
        assert_eq!(reconnecting_capture.attempts[1].report.disconnects, 0);
        assert_eq!(
            reconnecting_capture.attempts[0]
                .summary
                .observation
                .name
                .as_deref(),
            Some("NOSFET Aero")
        );
        assert_eq!(
            reconnecting_capture.attempts[1]
                .summary
                .observation
                .name
                .as_deref(),
            Some("NOSFET Aero")
        );
        assert_eq!(capture.report.subscribes, 2);
        assert_eq!(capture.report.disconnects, 1);
        assert_eq!(*session.link_ups.lock().expect("link ups"), 2);
        assert_eq!(*session.link_downs.lock().expect("link downs"), 1);
        assert_eq!(first.subscribes.lock().expect("first subscribes").len(), 1);
        assert_eq!(
            second.subscribes.lock().expect("second subscribes").len(),
            1
        );
        assert_eq!(*first.disconnects.lock().expect("first disconnects"), 1);
        assert_eq!(*second.disconnects.lock().expect("second disconnects"), 0);
        assert_eq!(
            capture.records,
            vec![
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 0,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 0,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
                crate::SessionCaptureRecord::LinkDown { monotonic_ms: 1 },
                crate::SessionCaptureRecord::Link {
                    monotonic_ms: 2,
                    max_write_len: Some(185),
                },
                crate::SessionCaptureRecord::Subscribe {
                    monotonic_ms: 2,
                    characteristic: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                },
            ]
        );
    }

    #[tokio::test]
    async fn capture_reconnecting_session_cancels_commands_after_reconnect() {
        let first = RecordingPeripheral::default();
        let second = RecordingPeripheral::default();
        let mut host = FakeReconnectHost::new(vec![first.clone(), second.clone()]);
        let mut session = CommandThenDisconnectSession::default();

        let capture = crate::capture_reconnecting_session_with_commands(
            &mut host,
            &mut session,
            GattChannel::from_bytes([0xA1; 16]),
            Duration::ZERO,
            2,
            false,
            &[
                DeviceCommand::RequestIdentity,
                DeviceCommand::RequestFirmwareInfo,
            ],
        )
        .await
        .expect("fake host reconnects after first-link commands");

        assert_eq!(capture.attempts.len(), 2);
        assert_eq!(capture.attempts[0].report.writes, 2);
        assert_eq!(capture.attempts[1].report.writes, 0);
        assert_eq!(
            first.writes.lock().expect("first writes").as_slice(),
            &[
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"N".to_vec(),
                    WriteMode::WithoutResponse,
                ),
                (
                    Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    b"V".to_vec(),
                    WriteMode::WithoutResponse,
                ),
            ]
        );
        assert!(second.writes.lock().expect("second writes").is_empty());
        assert_eq!(
            second.subscribes.lock().expect("second subscribes").len(),
            1
        );
    }

    #[derive(Default)]
    struct BridgeSession {
        notification_count: Arc<Mutex<usize>>,
        last_notification_channel: Arc<Mutex<Option<GattChannel>>>,
    }

    struct SubscribeOnlySession;

    impl ProtocolSession for SubscribeOnlySession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            if matches!(input, SessionInput::LinkUp(_)) {
                output.push(SessionOutput::Transport(TransportAction::Subscribe {
                    channel: GattChannel::from_bytes(
                        *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                    ),
                }));
            }
        }
    }

    struct CommandWriteSession;

    #[derive(Default)]
    struct CommandThenDisconnectSession {
        link_ups: usize,
    }

    struct LargeWriteSession;

    #[derive(Default)]
    struct DisconnectOnTickSession {
        link_down_count: Arc<Mutex<usize>>,
    }

    #[derive(Default)]
    struct ReconnectOnceSession {
        link_ups: Arc<Mutex<usize>>,
        link_downs: Arc<Mutex<usize>>,
    }

    struct FakeReconnectHost {
        peripherals: std::collections::VecDeque<RecordingPeripheral>,
        connects: usize,
    }

    impl FakeReconnectHost {
        fn new(peripherals: Vec<RecordingPeripheral>) -> Self {
            Self {
                peripherals: peripherals.into(),
                connects: 0,
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::ReconnectingSessionHost for FakeReconnectHost {
        type Peripheral = RecordingPeripheral;

        async fn connect(
            &mut self,
        ) -> Result<(Self::Peripheral, crate::ConnectionSummary), crate::BtleError> {
            self.connects += 1;
            self.peripherals
                .pop_front()
                .map(|peripheral| (peripheral, shared_write_notify_summary("NOSFET Aero")))
                .ok_or(crate::BtleError::NoPeripheralMatched)
        }
    }

    impl ProtocolSession for CommandWriteSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes(
                            *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                        ),
                    }));
                }
                SessionInput::Command(command) => {
                    let bytes = match command {
                        DeviceCommand::RequestIdentity => b"N".as_slice(),
                        DeviceCommand::RequestFirmwareInfo => b"V".as_slice(),
                        _ => return,
                    };
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes(
                            *Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb).as_bytes(),
                        ),
                        bytes: cutout_core::WritePayload::try_from_slice(bytes)
                            .expect("fixture payload fits"),
                        mode: WriteMode::WithoutResponse,
                    }));
                }
                SessionInput::LinkDown
                | SessionInput::Tick { .. }
                | SessionInput::Notification { .. } => {}
            }
        }
    }

    impl ProtocolSession for LargeWriteSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            if matches!(input, SessionInput::Tick { .. }) {
                output.push(SessionOutput::Transport(TransportAction::Write {
                    channel: GattChannel::from_bytes([0xA1; 16]),
                    bytes: cutout_core::WritePayload::try_from_slice(b"0123456789")
                        .expect("fixture payload fits"),
                    mode: WriteMode::WithoutResponse,
                }));
            }
        }
    }

    impl ProtocolSession for CommandThenDisconnectSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    self.link_ups += 1;
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                    }));
                }
                SessionInput::Command(command) => {
                    let bytes = match command {
                        DeviceCommand::RequestIdentity => b"N".as_slice(),
                        DeviceCommand::RequestFirmwareInfo => b"V".as_slice(),
                        _ => return,
                    };
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                        bytes: cutout_core::WritePayload::try_from_slice(bytes)
                            .expect("fixture payload fits"),
                        mode: WriteMode::WithoutResponse,
                    }));
                }
                SessionInput::Tick { .. } => {
                    if self.link_ups == 1 {
                        output.push(SessionOutput::Transport(TransportAction::Disconnect));
                    }
                }
                SessionInput::LinkDown | SessionInput::Notification { .. } => {}
            }
        }
    }

    impl ProtocolSession for DisconnectOnTickSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::Tick { .. } => {
                    output.push(SessionOutput::Transport(TransportAction::Disconnect));
                }
                SessionInput::LinkDown => {
                    *self.link_down_count.lock().expect("link down count") += 1;
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::LinkUp(_)
                | SessionInput::Command(_)
                | SessionInput::Notification { .. } => {}
            }
        }
    }

    impl ProtocolSession for ReconnectOnceSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    let mut link_ups = self.link_ups.lock().expect("link ups");
                    *link_ups += 1;
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                    }));
                }
                SessionInput::Tick { .. } => {
                    if *self.link_ups.lock().expect("link ups") == 1 {
                        output.push(SessionOutput::Transport(TransportAction::Disconnect));
                    }
                }
                SessionInput::LinkDown => {
                    *self.link_downs.lock().expect("link downs") += 1;
                    output.push(SessionOutput::Event(DeviceEvent::LinkDown));
                }
                SessionInput::Command(_) | SessionInput::Notification { .. } => {}
            }
        }
    }

    impl ProtocolSession for BridgeSession {
        fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
            match input {
                SessionInput::LinkUp(_) => {
                    output.push(SessionOutput::Transport(TransportAction::Subscribe {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                    }));
                }
                SessionInput::Tick { .. } => {
                    output.push(SessionOutput::Transport(TransportAction::Write {
                        channel: GattChannel::from_bytes([0xA1; 16]),
                        bytes: cutout_core::WritePayload::try_from_slice(b"bridge:write")
                            .expect("fixture payload fits"),
                        mode: WriteMode::WithResponse,
                    }));
                }
                SessionInput::Notification { channel, .. } => {
                    *self
                        .notification_count
                        .lock()
                        .expect("notification counter") += 1;
                    *self
                        .last_notification_channel
                        .lock()
                        .expect("notification channel") = Some(channel);
                    output.push(SessionOutput::NotificationIngest(
                        NotificationIngestOutcome::semantic_events(
                            ProtocolFamily::VeteranLeaperkimNosfet,
                            GattChannel::from_bytes([0xA1; 16]),
                            NotificationByteLen::new(2),
                            0,
                            SemanticEventCount::new(1),
                        ),
                    ));
                    output.push(SessionOutput::Event(DeviceEvent::Telemetry(
                        TelemetryDelta {
                            speed_mm_s: Some(Measured::reported(1_200)),
                            voltage_mv: Some(Measured::reported(84_200)),
                            battery_percent_estimated: Some(Measured::estimated(61)),
                            ..TelemetryDelta::empty(0)
                        },
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                        ReadOnlyResponse::Firmware(FirmwareInfo {
                            firmware_major: Some(Measured::reported(43)),
                            ..FirmwareInfo::default()
                        }),
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::ReadOnlyResponse(
                        ReadOnlyResponse::Settings(SettingsReadback {
                            entries: [
                                Some(SettingsEntry {
                                    field: RawFieldValue::new(0x0014, 30),
                                    source: ValueSource::Reported,
                                    quality: ValueQuality::Known,
                                    verification: VerificationStatus::HardwareVerified,
                                }),
                                None,
                                None,
                                None,
                            ],
                        }),
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::DiagnosticError(
                        DiagnosticError::from_parser_error(ParserError::MalformedFrame),
                    )));
                    output.push(SessionOutput::Event(DeviceEvent::Diagnostics(
                        ParserDiagnostics {
                            malformed_frames: 1,
                            ..ParserDiagnostics::default()
                        },
                    )));
                }
                SessionInput::LinkDown | SessionInput::Command(_) => {}
            }
        }
    }

    #[derive(Clone, Debug)]
    struct RecordingPeripheral {
        subscribes: Arc<Mutex<Vec<Uuid>>>,
        writes: WriteLog,
        notifications: NotificationLog,
        disconnects: Arc<Mutex<usize>>,
        mtu: u16,
    }

    impl Default for RecordingPeripheral {
        fn default() -> Self {
            Self {
                subscribes: Arc::new(Mutex::new(Vec::new())),
                writes: Arc::new(Mutex::new(Vec::new())),
                notifications: Arc::new(Mutex::new(Vec::new())),
                disconnects: Arc::new(Mutex::new(0)),
                mtu: 185,
            }
        }
    }

    impl RecordingPeripheral {
        fn with_mtu(mtu: u16) -> Self {
            Self {
                mtu,
                ..Self::default()
            }
        }

        fn with_notification(notification: ValueNotification) -> Self {
            Self::with_notifications(vec![notification])
        }

        fn with_notifications(notifications: Vec<ValueNotification>) -> Self {
            Self {
                notifications: Arc::new(Mutex::new(notifications)),
                ..Self::default()
            }
        }
    }

    fn begode_falcon_summary(name: &str) -> crate::ConnectionSummary {
        crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some(name.to_owned()),
                rssi: Some(-42),
                advertised_services: smallvec![Uuid::from_u128(
                    0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb,
                )],
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![crate::CharacteristicSummary {
                    uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                    service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                    properties: CharPropFlags::WRITE_WITHOUT_RESPONSE | CharPropFlags::NOTIFY,
                }],
            }],
        }
    }

    fn shared_write_notify_summary(name: &str) -> crate::ConnectionSummary {
        crate::ConnectionSummary {
            observation: crate::PeripheralObservation {
                identifier: "peripheral-id".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some(name.to_owned()),
                rssi: Some(-42),
                advertised_services: crate::AdvertisedServices::new(),
                manufacturer_data: crate::ManufacturerDataSummaries::new(),
            },
            services: vec![crate::ServiceSummary {
                uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                primary: true,
                characteristics: vec![
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe1_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::WRITE | CharPropFlags::NOTIFY,
                    },
                    crate::CharacteristicSummary {
                        uuid: Uuid::from_u128(0x0000_ffe2_0000_1000_8000_0080_5f9b_34fb),
                        service_uuid: Uuid::from_u128(0x0000_ffe0_0000_1000_8000_0080_5f9b_34fb),
                        properties: CharPropFlags::NOTIFY,
                    },
                ],
            }],
        }
    }

    #[async_trait::async_trait]
    impl crate::SessionPeripheral for RecordingPeripheral {
        fn mtu(&self) -> u16 {
            self.mtu
        }

        async fn subscribe(&self, characteristic: &Characteristic) -> Result<(), crate::BtleError> {
            self.subscribes
                .lock()
                .expect("subscribe log")
                .push(characteristic.uuid);
            Ok(())
        }

        async fn write(
            &self,
            characteristic: &Characteristic,
            bytes: &[u8],
            mode: WriteMode,
        ) -> Result<(), crate::BtleError> {
            self.writes.lock().expect("write log").push((
                characteristic.uuid,
                bytes.to_vec(),
                mode,
            ));
            Ok(())
        }

        async fn notifications(
            &self,
        ) -> Result<Pin<Box<dyn stream::Stream<Item = ValueNotification> + Send>>, crate::BtleError>
        {
            let notifications = self.notifications.lock().expect("notification log").clone();
            Ok(Box::pin(stream::iter(notifications)))
        }

        async fn disconnect(&self) -> Result<(), crate::BtleError> {
            *self.disconnects.lock().expect("disconnect log") += 1;
            Ok(())
        }
    }
}
