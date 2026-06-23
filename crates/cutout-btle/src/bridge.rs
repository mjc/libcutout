use btleplug::api::{Characteristic, ValueNotification};
use cutout_core::{
    DeviceCommand, GattChannel, LinkInfo, NotificationIngestOutcome, ProtocolSession, SessionInput,
    SessionOutput, TransportAction,
};
use futures_util::StreamExt;
use tracing::{debug, info};

use crate::{
    BtleError, ConnectionSummary, SessionBridgeError, SessionBridgeReport, SessionCapture,
    SessionCaptureRecord, SessionEndpoints, SessionPeripheral,
    identity::{IdentityContext, IdentityState, update_identity_report},
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
        },
        Some(&mut records),
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
}

pub(crate) async fn drive_session_inner<P, S>(
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
        window_ms = config.notification_window.as_duration().as_millis(),
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
            monotonic_ms: monotonic_ms.get(),
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
            identity_context: &identity_context,
            identity_state: &mut identity_state,
            report: &mut report,
            capture,
            write_provenance: config.write_provenance,
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
            monotonic_ms: monotonic_ms.get(),
            max_write_len: max_write_len.map(NegotiatedWriteLen::get),
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

struct NotificationLoopContext<'a, P: ?Sized> {
    peripheral: &'a P,
    channel: GattChannel,
    bindings: &'a BridgeBindings,
    identity_context: &'a IdentityContext<'a>,
    identity_state: &'a mut IdentityState,
    report: &'a mut SessionBridgeReport,
    capture: Option<&'a mut Vec<SessionCaptureRecord>>,
    write_provenance: WriteProvenance,
}

async fn process_notification_window<P, S>(
    mut context: NotificationLoopContext<'_, P>,
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
        debug!(
            remaining_ms = remaining.as_millis(),
            "session notification next await starting"
        );
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(notification)) => {
                *monotonic_ms = monotonic_ms.next();
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
                        monotonic_ms: monotonic_ms.get(),
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
                        write_provenance: context.write_provenance,
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
pub(crate) enum NotificationDecodeOutcome {
    Ignored,
    BufferedFragment,
    ParserGap,
    KnownReserved,
    ParserDiagnostic,
    SemanticEvents,
}

pub(crate) fn notification_decode_outcome(outputs: &[SessionOutput]) -> NotificationDecodeOutcome {
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
                    let write_limit = NegotiatedWriteLen::from_mtu(context.peripheral.mtu());
                    for chunk in bytes.as_slice().chunks(write_limit.chunk_len()) {
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
                                provenance: context.write_provenance,
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
