use btleplug::api::Characteristic;
use cutout_core::{
    DeviceCommand, GattChannel, LinkInfo, NotificationEvidence, NotificationIngestOutcome,
    ProtocolSession, SessionInput, SessionOutput, TransportAction, TransportWriteLimit, WriteMode,
    WritePayload,
};
use futures_util::StreamExt;
use tracing::{debug, info};

use crate::{
    BtleError, BtleNotification, BtleWriteChunk, ConnectionSummary, SessionBridgeError,
    SessionBridgeReport, SessionCapture, SessionCaptureRecord, SessionEndpoints, SessionPeripheral,
    identity::BridgeIdentityObserver,
    report::{process_device_event, process_notification_ingest_outcome},
    types::characteristic_from_summary,
    units::{MonotonicMs, NegotiatedWriteLen, NotificationWindow, WriteProvenance},
};

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
            channel,
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
    drive_session_inner(
        peripheral,
        session,
        DriveSessionConfig {
            channel,
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
            channel,
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

pub(crate) struct DriveSessionConfig<'a> {
    pub(crate) channel: GattChannel,
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
    mut capture: Option<&mut Vec<SessionCaptureRecord>>,
    mut identity_observer: Option<&mut dyn BridgeIdentityObserver>,
) -> Result<SessionBridgeReport, BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = config.notification_window.as_duration().as_millis(),
        channel = ?config.channel,
        "session bridge drive inner entered"
    );
    let mut report = SessionBridgeReport::default();
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
            channel: config.channel,
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

    for command in config.commands {
        monotonic_ms = monotonic_ms.next();
        session.handle(SessionInput::Command(*command), &mut outputs);
        process_session_outputs(
            SessionOutputContext {
                peripheral,
                channel: config.channel,
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

    monotonic_ms = monotonic_ms.next();
    session.handle(
        SessionInput::Tick {
            monotonic_ms: monotonic_ms.into_core(),
        },
        &mut outputs,
    );
    process_session_outputs(
        SessionOutputContext {
            peripheral,
            channel: config.channel,
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

    if config.notification_window.is_zero() || bindings.notify_characteristic.is_none() {
        return Ok(report);
    }

    process_notification_window(
        NotificationLoopContext {
            peripheral,
            channel: config.channel,
            bindings: &bindings,
            identity_observer,
            report: &mut report,
            capture,
            write_provenance: config.write_provenance,
            stream_end_is_link_down: config.stream_end_is_link_down,
            link_loss_idle_window: config.link_loss_idle_window,
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
    let max_write_len = Some(NegotiatedWriteLen::from_mtu(context.peripheral.mtu()));

    info!("session bridge link-up handling starting");
    session.handle(
        SessionInput::LinkUp(LinkInfo {
            monotonic_ms: monotonic_ms.into_core(),
            max_write_len: max_write_len.map(|len| TransportWriteLimit::from_bytes(len.get())),
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
    channel: GattChannel,
    bindings: &'a BridgeBindings,
    identity_observer: Option<&'observer mut dyn BridgeIdentityObserver>,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    write_provenance: WriteProvenance,
    stream_end_is_link_down: bool,
    link_loss_idle_window: Option<NotificationWindow>,
}

async fn process_notification_window<P, S>(
    mut context: NotificationLoopContext<'_, '_, P>,
    session: &mut S,
    outputs: &mut Vec<SessionOutput>,
    monotonic_ms: &mut MonotonicMs,
    notification_window: NotificationWindow,
) -> Result<(), BtleError>
where
    P: SessionPeripheral + Sync + ?Sized,
    S: ProtocolSession + Send,
{
    info!(
        window_ms = notification_window.as_duration().as_millis(),
        "session notification window starting"
    );
    info!("session notifications stream await starting");
    let mut notifications = context.peripheral.notifications().await?;
    info!("session notifications stream await completed");
    let deadline = tokio::time::Instant::now() + notification_window.as_duration();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let wait = link_loss_next_wait(remaining, context.link_loss_idle_window);
        debug!(
            remaining_ms = remaining.as_millis(),
            wait_ms = wait.as_millis(),
            "session notification next await starting"
        );
        match tokio::time::timeout(wait, notifications.next()).await {
            Ok(Some(notification)) => {
                *monotonic_ms = monotonic_ms.next();
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
                        channel: context.channel,
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
                log_notification_decode_outcome(decode_outcome, &notification, context.channel);
                context.report.notifications = context.report.notifications.increment();
                let notification_len = notification.len();
                context.report.notification_bytes = context
                    .report
                    .notification_bytes
                    .saturating_add_len(notification_len);
                context.report.latest_notification_len = Some(notification_len);
            }
            Ok(None) => {
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
                }
                break;
            }
            Err(_) => {
                if link_loss_idle_elapsed(remaining, context.link_loss_idle_window) {
                    debug!("session notification idle window elapsed; recording link down");
                    *monotonic_ms = monotonic_ms.next();
                    record_external_link_down(
                        context.report,
                        context.capture.as_deref_mut(),
                        session,
                        outputs,
                        *monotonic_ms,
                    )?;
                } else {
                    debug!("session notification window elapsed");
                }
                break;
            }
        }
    }
    debug!(
        notifications = context.report.notifications.get(),
        notification_bytes = context.report.notification_bytes.get(),
        latest_notification_len = ?context.report.latest_notification_len,
        "session notification window completed"
    );

    Ok(())
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
    session.handle(
        SessionInput::Notification {
            channel: context.channel,
            bytes: notification.as_raw_bytes(),
            monotonic_ms: monotonic_ms.into_core(),
        },
        outputs,
    );
    notification_decode_outcome(outputs)
}

fn link_loss_next_wait(
    remaining: std::time::Duration,
    link_loss_idle_window: Option<NotificationWindow>,
) -> std::time::Duration {
    link_loss_idle_window.map_or(remaining, |idle_window| {
        remaining.min(idle_window.as_duration())
    })
}

fn link_loss_idle_elapsed(
    remaining: std::time::Duration,
    link_loss_idle_window: Option<NotificationWindow>,
) -> bool {
    link_loss_idle_window.is_some_and(|idle_window| idle_window.as_duration() < remaining)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NotificationDecodeOutcome {
    Ignored(NotificationEvidence),
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
        .max_by_key(|outcome| outcome.kind())
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
            NotificationIngestOutcome::Ignored(notification) => {
                NotificationDecodeOutcome::Ignored(*notification)
            }
        }
    }
}

impl NotificationDecodeOutcome {
    pub(crate) const fn kind(self) -> NotificationDecodeKind {
        match self {
            Self::Ignored(_) => NotificationDecodeKind::Ignored,
            Self::BufferedFragment(_) => NotificationDecodeKind::BufferedFragment,
            Self::ParserGap(_) => NotificationDecodeKind::ParserGap,
            Self::KnownReserved(_) => NotificationDecodeKind::KnownReserved,
            Self::ParserDiagnostic(_) => NotificationDecodeKind::ParserDiagnostic,
            Self::SemanticEvents(_) => NotificationDecodeKind::SemanticEvents,
        }
    }
}

fn log_notification_decode_outcome(
    outcome: Option<NotificationDecodeOutcome>,
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
        Some(NotificationDecodeOutcome::Ignored(_)) | None => {
            debug!(
                uuid = %notification.characteristic,
                service = %notification.service,
                len = notification.len().get(),
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
                        expected = ?context.channel,
                        observed = ?observed,
                        monotonic_ms = monotonic_ms.get(),
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
    if observed != context.channel {
        return Err(SessionBridgeError::UnexpectedChannel {
            expected: context.channel,
            observed,
        }
        .into());
    }

    context.report.protocol_writes = context.report.protocol_writes.increment();
    let write_limit = NegotiatedWriteLen::from_mtu(context.peripheral.mtu());
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
