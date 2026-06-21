#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

//! Protocol-family scaffolding for Cutout.

use cutout_core::{
    Capabilities, CommandKind, DeviceCommand, DeviceEvent, GattChannel, PollRequest, PollingPlan,
    ProtocolSession, RequestPolicy, RequestQueue, RequestUrgency, SessionInput, SessionOutput,
    TransportAction, WriteMode,
};

/// Placeholder write channel for NOSFET/Veteran-family sessions.
pub const AERO_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xA1; 16]);

/// Placeholder write channel for Begode-family sessions.
pub const FALCON_WRITE_CHANNEL: GattChannel = GattChannel::from_bytes([0xB1; 16]);

const DEFAULT_POLICY: RequestPolicy = RequestPolicy {
    timeout_ms: 1_000,
    max_retries: 1,
    min_interval_ms: 100,
};

const AERO_POLL_PLAN: PollingPlan<5> = PollingPlan::new([
    PollRequest::new(
        CommandKind::RequestTelemetry,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestBatteryInfo,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestIdentity,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestFirmwareInfo,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestDiagnostics,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
]);

const FALCON_POLL_PLAN: PollingPlan<4> = PollingPlan::new([
    PollRequest::new(
        CommandKind::RequestTelemetry,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
    PollRequest::new(
        CommandKind::RequestIdentity,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestFirmwareInfo,
        DEFAULT_POLICY,
        RequestUrgency::High,
    ),
    PollRequest::new(
        CommandKind::RequestBatteryInfo,
        DEFAULT_POLICY,
        RequestUrgency::Routine,
    ),
]);

/// Initial read-only session shell for NOSFET Aero/Veteran-family devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AeroReadOnlySession {
    connected: bool,
    queue: RequestQueue<5>,
}

impl AeroReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub fn capabilities() -> Capabilities {
        Capabilities::from_supported_commands([
            CommandKind::RequestIdentity,
            CommandKind::RequestFirmwareInfo,
            CommandKind::RequestTelemetry,
            CommandKind::RequestBatteryInfo,
            CommandKind::RequestDiagnostics,
        ])
    }
}

impl ProtocolSession for AeroReadOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(info) => {
                self.connected = true;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            }
            SessionInput::LinkDown => {
                self.connected = false;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                if self.connected {
                    enqueue_aero_plan(&mut self.queue);
                    drain_queue(AERO_WRITE_CHANNEL, "aero", &mut self.queue, output);
                }
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms,
                len: bytes.len(),
            })),
            SessionInput::Command(
                DeviceCommand::RequestIdentity
                | DeviceCommand::RequestTelemetry
                | DeviceCommand::RequestFirmwareInfo
                | DeviceCommand::RequestBatteryInfo
                | DeviceCommand::RequestDiagnostics
                | DeviceCommand::RequestSettings
                | DeviceCommand::SetLights(_)
                | DeviceCommand::SoundHorn
                | DeviceCommand::SetRawMotorCurrent { .. },
            ) => {}
        }
    }
}

/// Initial read-only session shell for Begode Falcon devices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FalconReadOnlySession {
    connected: bool,
    queue: RequestQueue<4>,
}

impl FalconReadOnlySession {
    /// Returns the commands this session shell can schedule.
    #[must_use]
    pub fn capabilities() -> Capabilities {
        Capabilities::from_supported_commands([
            CommandKind::RequestIdentity,
            CommandKind::RequestFirmwareInfo,
            CommandKind::RequestTelemetry,
            CommandKind::RequestBatteryInfo,
        ])
    }
}

impl ProtocolSession for FalconReadOnlySession {
    fn handle(&mut self, input: SessionInput<'_>, output: &mut Vec<SessionOutput>) {
        match input {
            SessionInput::LinkUp(info) => {
                self.connected = true;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkUp(info)));
            }
            SessionInput::LinkDown => {
                self.connected = false;
                self.queue = RequestQueue::new();
                output.push(SessionOutput::Event(DeviceEvent::LinkDown));
            }
            SessionInput::Tick { monotonic_ms } => {
                output.push(SessionOutput::Event(DeviceEvent::Tick { monotonic_ms }));
                if self.connected {
                    enqueue_falcon_plan(&mut self.queue);
                    drain_queue(FALCON_WRITE_CHANNEL, "falcon", &mut self.queue, output);
                }
            }
            SessionInput::Notification {
                channel,
                bytes,
                monotonic_ms,
            } => output.push(SessionOutput::Event(DeviceEvent::NotificationReceived {
                channel,
                monotonic_ms,
                len: bytes.len(),
            })),
            SessionInput::Command(
                DeviceCommand::RequestIdentity
                | DeviceCommand::RequestTelemetry
                | DeviceCommand::RequestFirmwareInfo
                | DeviceCommand::RequestBatteryInfo
                | DeviceCommand::RequestDiagnostics
                | DeviceCommand::RequestSettings
                | DeviceCommand::SetLights(_)
                | DeviceCommand::SoundHorn
                | DeviceCommand::SetRawMotorCurrent { .. },
            ) => {}
        }
    }
}

fn enqueue_aero_plan(queue: &mut RequestQueue<5>) {
    let _result = AERO_POLL_PLAN.enqueue_into(queue);
}

fn enqueue_falcon_plan(queue: &mut RequestQueue<4>) {
    let _result = FALCON_POLL_PLAN.enqueue_into(queue);
}

fn drain_queue<const N: usize>(
    channel: GattChannel,
    family: &'static str,
    queue: &mut RequestQueue<N>,
    output: &mut Vec<SessionOutput>,
) {
    while let Some(request) = queue.pop_next() {
        output.push(SessionOutput::Transport(TransportAction::Write {
            channel,
            bytes: placeholder_payload(family, request.key.command).to_vec(),
            mode: WriteMode::WithResponse,
        }));
    }
}

fn placeholder_payload(family: &'static str, command: CommandKind) -> &'static [u8] {
    match (family, command) {
        ("aero", CommandKind::RequestIdentity) => b"aero:identity",
        ("aero", CommandKind::RequestFirmwareInfo) => b"aero:firmware",
        ("aero", CommandKind::RequestTelemetry) => b"aero:telemetry",
        ("aero", CommandKind::RequestBatteryInfo) => b"aero:battery",
        ("aero", CommandKind::RequestDiagnostics) => b"aero:diagnostics",
        ("falcon", CommandKind::RequestIdentity) => b"falcon:identity",
        ("falcon", CommandKind::RequestFirmwareInfo) => b"falcon:firmware",
        ("falcon", CommandKind::RequestTelemetry) => b"falcon:telemetry",
        ("falcon", CommandKind::RequestBatteryInfo) => b"falcon:battery",
        (
            _,
            CommandKind::RequestIdentity
            | CommandKind::RequestTelemetry
            | CommandKind::RequestFirmwareInfo
            | CommandKind::RequestBatteryInfo
            | CommandKind::RequestDiagnostics
            | CommandKind::RequestSettings
            | CommandKind::SetLights
            | CommandKind::SoundHorn
            | CommandKind::SetRawMotorCurrent,
        ) => b"",
    }
}

/// Returns the crate name used by setup smoke tests.
#[must_use]
pub const fn crate_name() -> &'static str {
    "cutout-protocols"
}

#[cfg(test)]
mod tests {
    use super::crate_name;
    use cutout_core::{
        Capabilities, CommandKind, LinkInfo, ProtocolSession, SessionInput, SessionOutput,
        TransportAction, WriteMode,
    };

    #[test]
    fn exposes_the_expected_name() {
        assert_eq!(crate_name(), "cutout-protocols");
    }

    #[test]
    fn aero_session_exposes_read_only_capabilities() {
        let capabilities = crate::AeroReadOnlySession::capabilities();

        assert_eq!(
            capabilities,
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
                CommandKind::RequestDiagnostics,
            ])
        );
    }

    #[test]
    fn falcon_session_exposes_read_only_capabilities() {
        let capabilities = crate::FalconReadOnlySession::capabilities();

        assert_eq!(
            capabilities,
            Capabilities::from_supported_commands([
                CommandKind::RequestIdentity,
                CommandKind::RequestFirmwareInfo,
                CommandKind::RequestTelemetry,
                CommandKind::RequestBatteryInfo,
            ])
        );
    }

    #[test]
    fn aero_session_does_not_poll_while_disconnected() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(SessionInput::Tick { monotonic_ms: 10 }, &mut output);

        assert!(transport_writes(&output).is_empty());
    }

    #[test]
    fn aero_session_emits_polls_in_scheduler_order_after_link_up() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert_eq!(
            transport_writes(&output),
            vec![
                (crate::AERO_WRITE_CHANNEL, b"aero:identity".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:firmware".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:telemetry".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:battery".to_vec()),
                (crate::AERO_WRITE_CHANNEL, b"aero:diagnostics".to_vec()),
            ]
        );
    }

    #[test]
    fn falcon_session_emits_polls_in_scheduler_order_after_link_up() {
        let mut session = crate::FalconReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(20),
            }),
            &mut output,
        );
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert_eq!(
            transport_writes(&output),
            vec![
                (crate::FALCON_WRITE_CHANNEL, b"falcon:identity".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:firmware".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:telemetry".to_vec()),
                (crate::FALCON_WRITE_CHANNEL, b"falcon:battery".to_vec()),
            ]
        );
    }

    #[test]
    fn aero_session_clears_queued_polls_on_link_down() {
        let mut session = crate::AeroReadOnlySession::default();
        let mut output = Vec::new();

        session.handle(
            SessionInput::LinkUp(LinkInfo {
                monotonic_ms: 1,
                max_write_len: Some(185),
            }),
            &mut output,
        );
        session.handle(SessionInput::LinkDown, &mut output);
        output.clear();
        session.handle(SessionInput::Tick { monotonic_ms: 2 }, &mut output);

        assert!(transport_writes(&output).is_empty());
    }

    fn transport_writes(output: &[SessionOutput]) -> Vec<(cutout_core::GattChannel, Vec<u8>)> {
        output
            .iter()
            .filter_map(|item| {
                let SessionOutput::Transport(TransportAction::Write {
                    channel,
                    bytes,
                    mode,
                }) = item
                else {
                    return None;
                };
                assert_eq!(*mode, WriteMode::WithResponse);
                Some((*channel, bytes.clone()))
            })
            .collect()
    }
}
