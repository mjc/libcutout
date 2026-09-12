use btleplug::api::Characteristic;
use cutout_core::{
    DeviceCommand, GattChannel, IgnoredNotificationEvidence, IgnoredNotificationReason, LinkInfo,
    NotificationEvidence, NotificationIngestOutcome, ProtocolSession, SessionInput, SessionOutput,
    TransportAction, TransportWriteLimit, WriteMode, WritePayload,
};
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::{
    BtleError, BtleNotification, BtleWriteChunk, ConnectionSummary, SessionBridgeError,
    SessionBridgeReport, SessionCapture, SessionCaptureRecord, SessionEndpoints, SessionPeripheral,
    identity::BridgeIdentityObserver,
    report::{process_device_event, process_notification_ingest_outcome},
    types::characteristic_from_summary,
    units::{MonotonicMs, NegotiatedWriteLimit, NotificationWindow, WriteProvenance},
};

/// Reusable notification stream for serialized protocol probes.
pub type BtleNotificationStream = Pin<Box<dyn Stream<Item = BtleNotification> + Send>>;

/// Write and notification channels used by a protocol session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionChannelPair {
    write: GattChannel,
    subscribe: GattChannel,
    admit_notifications: bool,
}

impl SessionChannelPair {
    /// Creates a pair with distinct write and notification channels.
    #[must_use]
    pub const fn new(write: GattChannel, subscribe: GattChannel) -> Self {
        Self {
            write,
            subscribe,
            admit_notifications: false,
        }
    }

    /// Requires incoming notifications to match the selected endpoint UUIDs.
    #[must_use]
    pub const fn with_endpoint_admission(mut self) -> Self {
        self.admit_notifications = true;
        self
    }

    const fn shared(channel: GattChannel) -> Self {
        Self::new(channel, channel)
    }
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
    notification_window: NotificationWindow,
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

/// Drives a protocol session while reporting identity from a host-supplied observer.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe, write, or
/// notification streaming fails.
pub async fn drive_session_with_identity_observer<P, S>(
    peripheral: &P,
    session: &mut S,
    channel: GattChannel,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: NotificationWindow,
    identity_observer: &mut dyn BridgeIdentityObserver,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_inner(
        peripheral,
        session,
        DriveSessionConfig {
            write_channel: channel,
            subscribe_channel: channel,
            admit_notifications: false,
            summary,
            endpoints,
            notification_window,
            commands: &[],
            write_provenance: WriteProvenance::Stable,
            monotonic_start: MonotonicMs::default(),
            stream_end_is_link_down: false,
            link_loss_idle_window: None,
        },
        None,
        Some(identity_observer),
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
    notification_window: NotificationWindow,
    commands: &[DeviceCommand],
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_with_channel_pair(
        peripheral,
        session,
        SessionChannelPair::shared(channel),
        summary,
        endpoints,
        notification_window,
        commands,
    )
    .await
}

/// Drives a protocol session whose write and notification channels differ.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe, write, or
/// notification streaming fails.
pub async fn drive_session_with_channel_pair<P, S>(
    peripheral: &P,
    session: &mut S,
    channels: SessionChannelPair,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: NotificationWindow,
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
            write_channel: channels.write,
            subscribe_channel: channels.subscribe,
            admit_notifications: channels.admit_notifications,
            summary,
            endpoints,
            notification_window,
            commands,
            write_provenance: WriteProvenance::Stable,
            monotonic_start: MonotonicMs::default(),
            stream_end_is_link_down: false,
            link_loss_idle_window: None,
        },
        None,
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
    notification_window: NotificationWindow,
    write_provenance: WriteProvenance,
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
            write_channel: channel,
            subscribe_channel: channel,
            admit_notifications: false,
            summary,
            endpoints,
            notification_window,
            commands: &[],
            write_provenance,
            monotonic_start: MonotonicMs::default(),
            stream_end_is_link_down: false,
            link_loss_idle_window: None,
        },
        Some(&mut records),
        None,
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
    notification_window: NotificationWindow,
    commands: &[DeviceCommand],
) -> Result<SessionCapture, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    capture_session_with_channel_pair(
        peripheral,
        session,
        SessionChannelPair::shared(channel),
        summary,
        endpoints,
        notification_window,
        commands,
    )
    .await
}

/// Captures a protocol session whose write and notification channels differ.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error if subscribe, write, or
/// notification streaming fails.
pub async fn capture_session_with_channel_pair<P, S>(
    peripheral: &P,
    session: &mut S,
    channels: SessionChannelPair,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: NotificationWindow,
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
            write_channel: channels.write,
            subscribe_channel: channels.subscribe,
            admit_notifications: channels.admit_notifications,
            summary,
            endpoints,
            notification_window,
            commands,
            write_provenance: WriteProvenance::Stable,
            monotonic_start: MonotonicMs::default(),
            stream_end_is_link_down: false,
            link_loss_idle_window: None,
        },
        Some(&mut records),
        None,
    )
    .await?;
    Ok(SessionCapture { records, report })
}

/// Captures one session window using an already-open notification stream.
///
/// Returning the stream allows callers to serialize multiple request windows
/// without losing notifications between probes.
///
/// # Errors
///
/// Returns [`BtleError`] when the session cannot be driven or the notification
/// endpoint is unavailable.
#[allow(
    clippy::too_many_arguments,
    reason = "This public capture helper mirrors the transport API."
)]
pub async fn capture_session_with_channel_pair_and_stream<P, S>(
    peripheral: &P,
    session: &mut S,
    channels: SessionChannelPair,
    summary: &ConnectionSummary,
    endpoints: SessionEndpoints<'_>,
    notification_window: NotificationWindow,
    commands: &[DeviceCommand],
    notifications: BtleNotificationStream,
) -> Result<(SessionCapture, BtleNotificationStream), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let mut records = Vec::new();
    let (report, notifications) = drive_session_inner_with_stream(
        peripheral,
        session,
        DriveSessionConfig {
            write_channel: channels.write,
            subscribe_channel: channels.subscribe,
            admit_notifications: channels.admit_notifications,
            summary,
            endpoints,
            notification_window,
            commands,
            write_provenance: WriteProvenance::Stable,
            monotonic_start: MonotonicMs::default(),
            stream_end_is_link_down: false,
            link_loss_idle_window: None,
        },
        Some(&mut records),
        None,
        Some(notifications),
    )
    .await?;
    Ok((
        SessionCapture { records, report },
        notifications.ok_or_else(|| {
            BtleError::from(SessionBridgeError::MissingNotifyEndpoint {
                channel: channels.subscribe,
            })
        })?,
    ))
}

pub(crate) struct DriveSessionConfig<'a> {
    pub(crate) write_channel: GattChannel,
    pub(crate) subscribe_channel: GattChannel,
    pub(crate) admit_notifications: bool,
    pub(crate) summary: &'a ConnectionSummary,
    pub(crate) endpoints: SessionEndpoints<'a>,
    pub(crate) notification_window: NotificationWindow,
    pub(crate) commands: &'a [DeviceCommand],
    pub(crate) write_provenance: WriteProvenance,
    pub(crate) monotonic_start: MonotonicMs,
    pub(crate) stream_end_is_link_down: bool,
    pub(crate) link_loss_idle_window: Option<NotificationWindow>,
}

pub(crate) async fn drive_session_inner<P, S>(
    peripheral: &P,
    session: &mut S,
    config: DriveSessionConfig<'_>,
    capture: Option<&mut Vec<SessionCaptureRecord>>,
    identity_observer: Option<&mut dyn BridgeIdentityObserver>,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    drive_session_inner_with_stream(
        peripheral,
        session,
        config,
        capture,
        identity_observer,
        None,
    )
    .await
    .map(|(report, _)| report)
}

#[allow(
    clippy::too_many_lines,
    reason = "The bridge loop keeps transport ordering in one place."
)]
async fn drive_session_inner_with_stream<P, S>(
    peripheral: &P,
    session: &mut S,
    config: DriveSessionConfig<'_>,
    mut capture: Option<&mut Vec<SessionCaptureRecord>>,
    mut identity_observer: Option<&mut dyn BridgeIdentityObserver>,
    mut notification_stream: Option<BtleNotificationStream>,
) -> Result<(SessionBridgeReport, Option<BtleNotificationStream>), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = config.notification_window.as_duration().as_millis(),
        write_channel = ?config.write_channel,
        subscribe_channel = ?config.subscribe_channel,
        "session bridge drive inner entered"
    );
    let mut report = SessionBridgeReport::default();
    let monotonic_origin = Instant::now();
    if let Some(observer) = identity_observer.as_deref_mut() {
        observer.observe_connection(config.summary);
        report.identity = observer.resolution();
    }
    let bindings = BridgeBindings {
        write_characteristic: characteristic_from_summary(config.endpoints.write),
        notify_characteristic: config.endpoints.notify.map(characteristic_from_summary),
    };
    let mut outputs = Vec::new();
    let mut monotonic_ms = config.monotonic_start;

    process_link_up_outputs(
        LinkUpContext {
            peripheral,
            write_channel: config.write_channel,
            subscribe_channel: config.subscribe_channel,
            bindings: &bindings,
            report: &mut report,
            capture: capture.as_deref_mut(),
            write_provenance: config.write_provenance,
        },
        session,
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    let mut notifications =
        if config.notification_window.is_zero() || bindings.notify_characteristic.is_none() {
            notification_stream.take()
        } else if let Some(stream) = notification_stream.take() {
            Some(stream)
        } else {
            Some(peripheral.notifications().await?)
        };

    for command in config.commands {
        monotonic_ms = elapsed_or_next(monotonic_ms, monotonic_origin);
        session.handle(SessionInput::Command(*command), &mut outputs);
        process_session_outputs(
            SessionOutputContext {
                peripheral,
                write_channel: config.write_channel,
                subscribe_channel: config.subscribe_channel,
                write_characteristic: &bindings.write_characteristic,
                notify_characteristic: bindings.notify_characteristic.as_ref(),
                report: &mut report,
                capture: capture.as_deref_mut(),
                write_provenance: config.write_provenance,
            },
            session,
            &mut outputs,
            monotonic_ms,
        )
        .await?;
    }

    monotonic_ms = elapsed_or_next(monotonic_ms, monotonic_origin);
    session.handle(
        SessionInput::Tick {
            monotonic_ms: monotonic_ms.into_core(),
        },
        &mut outputs,
    );
    process_session_outputs(
        SessionOutputContext {
            peripheral,
            write_channel: config.write_channel,
            subscribe_channel: config.subscribe_channel,
            write_characteristic: &bindings.write_characteristic,
            notify_characteristic: bindings.notify_characteristic.as_ref(),
            report: &mut report,
            capture: capture.as_deref_mut(),
            write_provenance: config.write_provenance,
        },
        session,
        &mut outputs,
        monotonic_ms,
    )
    .await?;

    let Some(notifications) = notifications.take() else {
        return Ok((report, None));
    };

    let notifications = process_notification_window(
        NotificationLoopContext {
            peripheral,
            write_channel: config.write_channel,
            subscribe_channel: config.subscribe_channel,
            admit_notifications: config.admit_notifications,
            bindings: &bindings,
            identity_observer,
            report: &mut report,
            capture,
            write_provenance: config.write_provenance,
            stream_end_is_link_down: config.stream_end_is_link_down,
            link_loss_idle_window: config.link_loss_idle_window,
            monotonic_origin,
        },
        session,
        &mut outputs,
        &mut monotonic_ms,
        config.notification_window,
        notifications,
    )
    .await?;

    Ok((report, Some(notifications)))
}

fn elapsed_or_next(previous: MonotonicMs, origin: Instant) -> MonotonicMs {
    previous.next().max(MonotonicMs::from_elapsed_millis(
        origin.elapsed().as_millis(),
    ))
}

struct BridgeBindings {
    write_characteristic: Characteristic,
    notify_characteristic: Option<Characteristic>,
}

struct LinkUpContext<'a, P: ?Sized> {
    peripheral: &'a P,
    write_channel: GattChannel,
    subscribe_channel: GattChannel,
    bindings: &'a BridgeBindings,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    write_provenance: WriteProvenance,
}

async fn process_link_up_outputs<P, S>(
    mut context: LinkUpContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: MonotonicMs,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    let max_write_len = Some(NegotiatedWriteLimit::from_bytes(context.peripheral.mtu()));

    info!("session bridge link-up handling starting");
    session.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms: monotonic_ms.into_core(),
            max_write_len: max_write_len.map(|len| TransportWriteLimit::from_bytes(len.as_bytes())),
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
            write_channel: context.write_channel,
            subscribe_channel: context.subscribe_channel,
            write_characteristic: &context.bindings.write_characteristic,
            notify_characteristic: context.bindings.notify_characteristic.as_ref(),
            report: context.report,
            capture: context.capture.as_deref_mut(),
            write_provenance: context.write_provenance,
        },
        session,
        outputs,
        monotonic_ms,
    )
    .await?;
    info!("session bridge initial output processing completed");

    Ok(())
}

struct NotificationLoopContext<'a, 'observer, P: ?Sized> {
    peripheral: &'a P,
    write_channel: GattChannel,
    subscribe_channel: GattChannel,
    admit_notifications: bool,
    bindings: &'a BridgeBindings,
    identity_observer: Option<&'observer mut dyn BridgeIdentityObserver>,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    write_provenance: WriteProvenance,
    stream_end_is_link_down: bool,
    link_loss_idle_window: Option<NotificationWindow>,
    monotonic_origin: Instant,
}

const SESSION_DEADLINE_TICK: Duration = Duration::from_millis(100);

#[allow(
    clippy::too_many_lines,
    clippy::single_match_else,
    reason = "The notification loop must serialize stream, tick, and link-loss events."
)]
async fn process_notification_window<P, S>(
    mut context: NotificationLoopContext<'_, '_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: &mut MonotonicMs,
    notification_window: NotificationWindow,
    mut notifications: BtleNotificationStream,
) -> Result<BtleNotificationStream, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = notification_window.as_duration().as_millis(),
        "session notification window starting"
    );
    info!("session notifications stream await starting");
    info!("session notifications stream ready before protocol polling");
    let deadline = tokio::time::Instant::now() + notification_window.as_duration();
    let mut next_tick = tokio::time::Instant::now() + SESSION_DEADLINE_TICK;
    let mut last_notification_at = tokio::time::Instant::now();
    let mut link_down_recorded = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let wait = link_loss_next_wait(remaining, context.link_loss_idle_window);
        let wait_deadline = tokio::time::Instant::now() + wait;
        let tick_due = next_tick <= wait_deadline;
        let tick_deadline = if tick_due { next_tick } else { wait_deadline };
        debug!(
            remaining_ms = remaining.as_millis(),
            wait_ms = wait.as_millis(),
            "session notification next await starting"
        );
        tokio::select! {
            result = notifications.next() => match result {
                Some(notification) => {
                last_notification_at = tokio::time::Instant::now();
                *monotonic_ms = elapsed_or_next(*monotonic_ms, context.monotonic_origin);
                let decode_outcome = ingest_notification(
                    &mut context,
                    session,
                    outputs,
                    &notification,
                    *monotonic_ms,
                );
                process_session_outputs(
                    SessionOutputContext {
                        peripheral: context.peripheral,
                        write_channel: context.write_channel,
                        subscribe_channel: context.subscribe_channel,
                        write_characteristic: &context.bindings.write_characteristic,
                        notify_characteristic: context.bindings.notify_characteristic.as_ref(),
                        report: context.report,
                        capture: context.capture.as_deref_mut(),
                        write_provenance: context.write_provenance,
                    },
                    session,
                    outputs,
                    *monotonic_ms,
                )
                .await?;
                log_notification_decode_outcome(
                    decode_outcome.as_ref(),
                    &notification,
                    context.subscribe_channel,
                );
                context.report.notifications = context.report.notifications.increment();
                let notification_len = notification.len();
                context.report.notification_bytes =
                    context.report.notification_bytes.saturating_add(
                        crate::NotificationPayloadTotal::from_bytes(notification_len.as_bytes()),
                    );
                context.report.latest_notification_len = Some(notification_len);
            }
                None => {
                debug!("session notification stream ended");
                if context.stream_end_is_link_down {
                    *monotonic_ms = monotonic_ms.next();
                    record_external_link_down(
                        context.report,
                        context.capture.as_deref_mut(),
                        session,
                        outputs,
                        *monotonic_ms,
                    )?;
                    link_down_recorded = true;
                }
                break;
                }
            },
            () = tokio::time::sleep_until(tick_deadline), if tick_due => {
                *monotonic_ms = elapsed_or_next(*monotonic_ms, context.monotonic_origin);
                session.handle(
                    SessionInput::Tick {
                        monotonic_ms: monotonic_ms.into_core(),
                    },
                    outputs,
                );
                process_session_outputs(
                    SessionOutputContext {
                        peripheral: context.peripheral,
                        write_channel: context.write_channel,
                        subscribe_channel: context.subscribe_channel,
                        write_characteristic: &context.bindings.write_characteristic,
                        notify_characteristic: context.bindings.notify_characteristic.as_ref(),
                        report: context.report,
                        capture: context.capture.as_deref_mut(),
                        write_provenance: context.write_provenance,
                    },
                    session,
                    outputs,
                    *monotonic_ms,
                )
                .await?;
                // A tick can produce a write that waits for the transport to
                // accept it (for example while a no-response BLE queue is
                // full). Rebase the next response deadline after that write
                // completes instead of catching up from the old timer slot.
                // This keeps requests serialized at the transport boundary.
                next_tick = tokio::time::Instant::now() + SESSION_DEADLINE_TICK;
            },
            () = tokio::time::sleep_until(wait_deadline) => {
                if link_loss_idle_elapsed(last_notification_at, context.link_loss_idle_window) {
                    debug!("session notification idle window elapsed; recording link down");
                    *monotonic_ms = monotonic_ms.next();
                    record_external_link_down(
                        context.report,
                        context.capture.as_deref_mut(),
                        session,
                        outputs,
                        *monotonic_ms,
                    )?;
                    link_down_recorded = true;
                } else {
                    debug!("session notification window elapsed");
                }
                break;
            }
        }
    }
    if !link_down_recorded
        && context.stream_end_is_link_down
        && link_loss_idle_elapsed(last_notification_at, context.link_loss_idle_window)
    {
        *monotonic_ms = monotonic_ms.next();
        record_external_link_down(
            context.report,
            context.capture.as_deref_mut(),
            session,
            outputs,
            *monotonic_ms,
        )?;
    }
    debug!(
        notifications = context.report.notifications.as_events(),
        notification_bytes = context.report.notification_bytes.as_bytes(),
        latest_notification_len = ?context.report.latest_notification_len,
        "session notification window completed"
    );

    Ok(notifications)
}

fn ingest_notification<P, S>(
    context: &mut NotificationLoopContext<'_, '_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    notification: &BtleNotification,
    monotonic_ms: MonotonicMs,
) -> Option<NotificationDecodeOutcome>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    if let Some(records) = context.capture.as_deref_mut() {
        records.push(SessionCaptureRecord::Notification {
            monotonic_ms,
            characteristic: notification.characteristic,
            service: notification.service,
            bytes: notification.bytes.clone(),
        });
    }
    if let Some(observer) = context.identity_observer.as_deref_mut() {
        observer.observe_notification(notification);
        context.report.identity = observer.resolution();
    }
    let admitted = !context.admit_notifications
        || context
            .bindings
            .notify_characteristic
            .as_ref()
            .is_some_and(|expected| {
                notification.characteristic == expected.uuid
                    && notification.service == expected.service_uuid
            });
    if !admitted {
        let outcome = NotificationIngestOutcome::Ignored {
            evidence: IgnoredNotificationEvidence::with_retained_payload(
                None,
                context.subscribe_channel,
                notification.as_raw_bytes(),
                monotonic_ms.into_core(),
            ),
            reason: IgnoredNotificationReason::WrongChannel,
        };
        outputs.push(SessionOutput::NotificationIngest(outcome));
        return notification_decode_outcome(outputs);
    }
    session.handle(
        SessionInput::Notification {
            channel: context.subscribe_channel,
            bytes: notification.as_raw_bytes(),
            monotonic_ms: monotonic_ms.into_core(),
        },
        outputs,
    );
    notification_decode_outcome(outputs)
}

fn link_loss_next_wait(
    remaining: Duration,
    link_loss_idle_window: Option<NotificationWindow>,
) -> Duration {
    link_loss_idle_window.map_or(remaining, |idle_window| {
        remaining.min(idle_window.as_duration())
    })
}

fn link_loss_idle_elapsed(
    last_notification_at: tokio::time::Instant,
    link_loss_idle_window: Option<NotificationWindow>,
) -> bool {
    link_loss_idle_window.is_some_and(|idle_window| {
        tokio::time::Instant::now().saturating_duration_since(last_notification_at)
            >= idle_window.as_duration()
    })
}

fn record_external_link_down<S>(
    report: &mut SessionBridgeReport,
    capture: Option<&mut Vec<SessionCaptureRecord>>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: MonotonicMs,
) -> Result<(), BtleError>
where
    S: ProtocolSession + Send,
{
    if let Some(records) = capture {
        records.push(SessionCaptureRecord::LinkDown { monotonic_ms });
    }
    report.disconnects = report.disconnects.increment();
    session.handle(SessionInput::LinkDown, outputs);
    while !outputs.is_empty() {
        for output in std::mem::take(outputs) {
            match output {
                SessionOutput::Event(event) => process_device_event(report, event, monotonic_ms),
                SessionOutput::NotificationIngest(outcome) => {
                    process_notification_ingest_outcome(report, outcome, monotonic_ms);
                }
                SessionOutput::Transport(_) => {
                    return Err(SessionBridgeError::ExternalLinkDownTransportAction.into());
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NotificationDecodeOutcome {
    Ignored {
        evidence: IgnoredNotificationEvidence,
        reason: IgnoredNotificationReason,
    },
    BufferedFragment(NotificationEvidence),
    ParserGap(NotificationEvidence),
    KnownReserved(NotificationEvidence),
    ParserDiagnostic(NotificationEvidence),
    SemanticEvents(NotificationEvidence),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum NotificationDecodeKind {
    Ignored,
    BufferedFragment,
    ParserGap,
    KnownReserved,
    ParserDiagnostic,
    SemanticEvents,
}

pub(crate) fn notification_decode_outcome(
    outputs: &[SessionOutput],
) -> Option<NotificationDecodeOutcome> {
    outputs
        .iter()
        .filter_map(|output| match output {
            SessionOutput::NotificationIngest(outcome) => Some(outcome),
            SessionOutput::Transport(_) | SessionOutput::Event(_) => None,
        })
        .map(NotificationDecodeOutcome::from)
        .max_by_key(NotificationDecodeOutcome::kind)
}

impl From<&NotificationIngestOutcome> for NotificationDecodeOutcome {
    fn from(outcome: &NotificationIngestOutcome) -> Self {
        match outcome {
            NotificationIngestOutcome::SemanticEvents { notification, .. } => {
                NotificationDecodeOutcome::SemanticEvents(*notification)
            }
            NotificationIngestOutcome::ParserDiagnostic { notification, .. } => {
                NotificationDecodeOutcome::ParserDiagnostic(*notification)
            }
            NotificationIngestOutcome::KnownReserved { notification, .. } => {
                NotificationDecodeOutcome::KnownReserved(*notification)
            }
            NotificationIngestOutcome::ParserGap { notification, .. } => {
                NotificationDecodeOutcome::ParserGap(*notification)
            }
            NotificationIngestOutcome::BufferedFragment(notification) => {
                NotificationDecodeOutcome::BufferedFragment(*notification)
            }
            NotificationIngestOutcome::Ignored { evidence, reason } => {
                NotificationDecodeOutcome::Ignored {
                    evidence: evidence.clone(),
                    reason: *reason,
                }
            }
        }
    }
}

impl NotificationDecodeOutcome {
    pub(crate) const fn kind(&self) -> NotificationDecodeKind {
        match self {
            Self::Ignored { .. } => NotificationDecodeKind::Ignored,
            Self::BufferedFragment(_) => NotificationDecodeKind::BufferedFragment,
            Self::ParserGap(_) => NotificationDecodeKind::ParserGap,
            Self::KnownReserved(_) => NotificationDecodeKind::KnownReserved,
            Self::ParserDiagnostic(_) => NotificationDecodeKind::ParserDiagnostic,
            Self::SemanticEvents(_) => NotificationDecodeKind::SemanticEvents,
        }
    }
}

fn log_notification_decode_outcome(
    outcome: Option<&NotificationDecodeOutcome>,
    notification: &BtleNotification,
    channel: GattChannel,
) {
    match outcome {
        Some(NotificationDecodeOutcome::SemanticEvents(_)) => {}
        Some(NotificationDecodeOutcome::BufferedFragment(evidence)) => {
            debug!(
                len = evidence.len.as_bytes(),
                channel = ?evidence.channel,
                "session notification buffered by protocol decoder"
            );
        }
        Some(NotificationDecodeOutcome::ParserDiagnostic(evidence)) => {
            debug!(
                len = evidence.len.as_bytes(),
                channel = ?evidence.channel,
                "session notification produced parser diagnostic"
            );
        }
        Some(NotificationDecodeOutcome::KnownReserved(evidence)) => {
            debug!(
                len = evidence.len.as_bytes(),
                channel = ?evidence.channel,
                "session notification produced known reserved protocol evidence"
            );
        }
        Some(NotificationDecodeOutcome::ParserGap(evidence)) => {
            debug!(
                len = evidence.len.as_bytes(),
                channel = ?evidence.channel,
                "session notification produced parser gap evidence"
            );
        }
        Some(NotificationDecodeOutcome::Ignored { reason, .. }) => {
            debug!(
                uuid = %notification.characteristic,
                service = %notification.service,
                len = notification.len().as_bytes(),
                channel = ?channel,
                reason = ?reason,
                "session notification ignored by protocol session"
            );
        }
        None => {
            debug!(
                uuid = %notification.characteristic,
                service = %notification.service,
                len = notification.len().as_bytes(),
                channel = ?channel,
                "session notification ignored by protocol session"
            );
        }
    }
}

struct SessionOutputContext<'a, P: ?Sized> {
    peripheral: &'a P,
    write_channel: GattChannel,
    subscribe_channel: GattChannel,
    write_characteristic: &'a Characteristic,
    notify_characteristic: Option<&'a Characteristic>,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    write_provenance: WriteProvenance,
}

async fn process_session_outputs<P, S>(
    mut context: SessionOutputContext<'_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: MonotonicMs,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    while !outputs.is_empty() {
        for output in std::mem::take(outputs) {
            match output {
                SessionOutput::Transport(TransportAction::Subscribe { channel: observed }) => {
                    info!(
                        expected = ?context.subscribe_channel,
                        observed = ?observed,
                        monotonic_ms = monotonic_ms.get(),
                        "session bridge processing subscribe output"
                    );
                    if observed != context.subscribe_channel {
                        return Err(SessionBridgeError::UnexpectedChannel {
                            expected: context.subscribe_channel,
                            observed,
                        }
                        .into());
                    }
                    let Some(notify_characteristic) = context.notify_characteristic else {
                        return Err(SessionBridgeError::MissingNotifyEndpoint {
                            channel: context.subscribe_channel,
                        }
                        .into());
                    };
                    info!(
                        characteristic = %notify_characteristic.uuid,
                        service = %notify_characteristic.service_uuid,
                        monotonic_ms = monotonic_ms.get(),
                        "session subscribe await starting"
                    );
                    context.peripheral.subscribe(notify_characteristic).await?;
                    info!(
                        characteristic = %notify_characteristic.uuid,
                        service = %notify_characteristic.service_uuid,
                        monotonic_ms = monotonic_ms.get(),
                        "session subscribe await completed"
                    );
                    if let Some(records) = context.capture.as_deref_mut() {
                        records.push(SessionCaptureRecord::Subscribe {
                            monotonic_ms,
                            characteristic: notify_characteristic.uuid,
                        });
                    }
                    context.report.subscribes = context.report.subscribes.increment();
                }
                SessionOutput::Transport(TransportAction::Write {
                    channel: observed,
                    bytes,
                    mode,
                }) => {
                    process_transport_write(&mut context, observed, &bytes, mode, monotonic_ms)
                        .await?;
                }
                SessionOutput::Transport(TransportAction::Disconnect) => {
                    context.peripheral.disconnect().await?;
                    if let Some(records) = context.capture.as_deref_mut() {
                        records.push(SessionCaptureRecord::LinkDown { monotonic_ms });
                    }
                    context.report.disconnects = context.report.disconnects.increment();
                    session.handle(SessionInput::LinkDown, outputs);
                }
                SessionOutput::Event(event) => {
                    process_device_event(context.report, event, monotonic_ms);
                }
                SessionOutput::NotificationIngest(outcome) => {
                    process_notification_ingest_outcome(context.report, outcome, monotonic_ms);
                }
            }
        }
    }
    Ok(())
}

async fn process_transport_write<P>(
    context: &mut SessionOutputContext<'_, P>,
    observed: GattChannel,
    bytes: &WritePayload,
    mode: WriteMode,
    monotonic_ms: MonotonicMs,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
{
    if observed != context.write_channel {
        return Err(SessionBridgeError::UnexpectedChannel {
            expected: context.write_channel,
            observed,
        }
        .into());
    }

    context.report.protocol_writes = context.report.protocol_writes.increment();
    let write_limit = NegotiatedWriteLimit::from_bytes(context.peripheral.mtu());
    for chunk in bytes.as_slice().chunks(write_limit.chunk_len()) {
        let chunk = BtleWriteChunk::new(chunk, write_limit).ok_or(
            SessionBridgeError::WriteChunkTooLong {
                len: cutout_core::NotificationByteLen::from_bytes(chunk.len()),
                limit: write_limit,
            },
        )?;
        context
            .peripheral
            .write(context.write_characteristic, chunk, mode)
            .await?;
        if let Some(records) = context.capture.as_deref_mut() {
            records.push(SessionCaptureRecord::Write {
                monotonic_ms,
                characteristic: context.write_characteristic.uuid,
                mode,
                bytes: bytes::Bytes::copy_from_slice(chunk.as_slice()).into(),
                provenance: context.write_provenance,
            });
        }
        context.report.writes = context.report.writes.increment();
    }
    Ok(())
}
