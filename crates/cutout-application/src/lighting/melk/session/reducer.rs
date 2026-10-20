//! Pure Rust session reducer for the MELK `CoreBluetooth` adapter.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cutout_protocols::MelkLightingProfile;

use super::super::{LightingWrite, LightingWriteMode, transport_action};
use super::contract::{
    LightingSessionAction, LightingSessionCandidate, LightingSessionEvent, LightingSessionSnapshot,
    LightingSessionState, LightingTimer,
};

const MAX_CANDIDATES: usize = 32;
const CONNECTION_TIMEOUT_MS: u64 = 15_000;
const PROBE_TIMEOUT_MS: u64 = 3_000;
const INITIALIZATION_DELAY_MS: u64 = 1_000;
const MAX_RECONNECT_ATTEMPTS: u8 = 3;
const FALLBACK_WRITE_INTERVAL_MS: u16 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GattState {
    Unverified,
    Verified,
    Subscribed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionPurpose {
    Probe,
    Selected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProbeOutcome {
    Supported,
    Unsupported,
    Unavailable,
}

#[derive(Clone, Copy, Debug)]
enum RestoredConnection {
    Connected,
    Pending,
    Disconnected,
}

#[derive(Clone, Debug)]
pub struct SessionReducer {
    state: LightingSessionState,
    preferred_identifier: Option<String>,
    invalid_preferred_identifier: bool,
    selected_identifier: Option<String>,
    selected_name: Option<String>,
    candidates: BTreeMap<String, LightingSessionCandidate>,
    purpose: ConnectionPurpose,
    verified: BTreeSet<String>,
    rejected: BTreeSet<String>,
    probe_queue: VecDeque<String>,
    reconnect_enabled: bool,
    reconnect_attempt: u8,
    bluetooth_powered_on: bool,
    pending_restoration: Option<RestoredConnection>,
    gatt: GattState,
    initialization: VecDeque<LightingWrite>,
    writes: VecDeque<LightingWrite>,
    timer: Option<LightingTimer>,
    command_status: u8,
    actions: VecDeque<LightingSessionAction>,
    records: VecDeque<String>,
    notifications: VecDeque<Vec<u8>>,
    candidates_out: VecDeque<LightingSessionCandidate>,
    candidate_removals_out: VecDeque<String>,
}

impl Default for SessionReducer {
    fn default() -> Self {
        Self {
            state: LightingSessionState::Idle,
            preferred_identifier: None,
            invalid_preferred_identifier: false,
            selected_identifier: None,
            selected_name: None,
            candidates: BTreeMap::new(),
            purpose: ConnectionPurpose::Selected,
            verified: BTreeSet::new(),
            rejected: BTreeSet::new(),
            probe_queue: VecDeque::new(),
            reconnect_enabled: true,
            reconnect_attempt: 0,
            bluetooth_powered_on: false,
            pending_restoration: None,
            gatt: GattState::Unverified,
            initialization: VecDeque::new(),
            writes: VecDeque::new(),
            timer: None,
            command_status: 0,
            actions: VecDeque::new(),
            records: VecDeque::new(),
            notifications: VecDeque::new(),
            candidates_out: VecDeque::new(),
            candidate_removals_out: VecDeque::new(),
        }
    }
}

impl SessionReducer {
    pub fn start(&mut self, preferred_identifier: Option<&str>) {
        self.actions.clear();
        self.records.clear();
        self.notifications.clear();
        self.candidates_out.clear();
        self.candidate_removals_out.clear();
        self.preferred_identifier = preferred_identifier.map(str::to_owned);
        self.invalid_preferred_identifier =
            preferred_identifier.is_some_and(|value| uuid::Uuid::parse_str(value).is_err());
        self.reconnect_enabled = true;
        self.reconnect_attempt = 0;
        self.bluetooth_powered_on = false;
        self.pending_restoration = None;
        self.candidates.clear();
        self.verified.clear();
        self.rejected.clear();
        self.probe_queue.clear();
        self.purpose = ConnectionPurpose::Selected;
        self.selected_identifier = None;
        self.selected_name = None;
        self.timer = None;
        self.command_status = 0;
        self.transition(LightingSessionState::Idle);
    }

    pub fn stop(&mut self) {
        self.reconnect_enabled = false;
        self.purpose = ConnectionPurpose::Selected;
        self.timer = None;
        self.actions.clear();
        self.transition(LightingSessionState::Disconnected);
    }

    #[must_use]
    pub fn snapshot(&self) -> LightingSessionSnapshot {
        LightingSessionSnapshot {
            state: if self.is_probing() {
                LightingSessionState::Scanning
            } else {
                self.state.clone()
            },
            platform_identifier: self.selected_identifier.clone(),
            name: self.selected_name.clone(),
            command_status: self.command_status,
            notification_ready: self.gatt == GattState::Subscribed,
        }
    }

    fn record(&mut self, value: impl Into<String>) {
        self.records.push_back(value.into());
    }

    fn transition(&mut self, state: LightingSessionState) {
        if match state {
            LightingSessionState::Ready => true,
            _ => false,
        } {
            self.reconnect_attempt = 0;
            if self.preferred_identifier.is_none() {
                self.preferred_identifier
                    .clone_from(&self.selected_identifier);
            }
        } else {
            self.timer = None;
            if self.command_status == 1 {
                self.command_status = 3;
            }
            self.writes.clear();
            self.reset_initialization();
        }
        self.state = state;
    }

    fn forget_selected_connection(&mut self) {
        self.timer = None;
        self.pending_restoration = None;
        self.purpose = ConnectionPurpose::Selected;
        self.probe_queue.clear();
        self.selected_identifier = None;
        self.selected_name = None;
    }

    fn reset_initialization(&mut self) {
        self.initialization.clear();
        self.gatt = GattState::Unverified;
        if match self.timer {
            Some(LightingTimer::Initialization) => true,
            _ => false,
        } {
            self.timer = None;
        }
    }

    fn accepts(&self, identifier: &str) -> bool {
        let Ok(identifier) = uuid::Uuid::parse_str(identifier) else {
            return false;
        };
        !self.invalid_preferred_identifier
            && self
                .preferred_identifier
                .as_deref()
                .is_none_or(|preferred| uuid::Uuid::parse_str(preferred).ok() == Some(identifier))
    }

    fn is_probing(&self) -> bool {
        self.purpose == ConnectionPurpose::Probe
    }

    fn probe_next(&mut self) {
        if self.selected_identifier.is_some() || !self.bluetooth_powered_on {
            return;
        }
        while let Some(identifier) = self.probe_queue.pop_front() {
            let Some(candidate) = self.candidates.get(&identifier) else {
                continue;
            };
            self.selected_name.clone_from(&candidate.name);
            self.selected_identifier = Some(identifier.clone());
            self.purpose = ConnectionPurpose::Probe;
            self.transition(LightingSessionState::Connecting);
            self.actions.push_back(LightingSessionAction::Connect {
                platform_identifier: identifier,
            });
            self.arm(LightingTimer::ConnectionAttempt, PROBE_TIMEOUT_MS);
            break;
        }
    }

    fn finish_probe(&mut self, outcome: ProbeOutcome) {
        let Some(identifier) = self.selected_identifier.take() else {
            return;
        };
        self.actions
            .push_back(LightingSessionAction::CancelConnect {
                platform_identifier: identifier.clone(),
            });
        if outcome == ProbeOutcome::Supported {
            self.verified.insert(identifier.clone());
            if let Some(candidate) = self.candidates.get(&identifier) {
                self.candidates_out.push_back(candidate.clone());
            }
        } else {
            // Bound the negative cache while avoiding immediate repeated probes of noise.
            if outcome == ProbeOutcome::Unsupported {
                if self.rejected.len() >= 256 {
                    self.rejected.clear();
                }
                self.rejected.insert(identifier.clone());
            }
            self.discard_candidate(identifier);
        }
        self.selected_name = None;
        self.purpose = ConnectionPurpose::Selected;
        self.transition(LightingSessionState::Scanning);
        self.probe_next();
    }

    fn discard_candidate(&mut self, identifier: String) {
        self.candidates.remove(&identifier);
        self.verified.remove(&identifier);
        self.probe_queue.retain(|pending| pending != &identifier);
        self.candidates_out
            .retain(|candidate| candidate.platform_identifier != identifier);
        self.candidate_removals_out.push_back(identifier);
    }

    fn reject_candidate(&mut self, reason: String) {
        if self.is_probing() {
            self.record(format!("probe_rejected reason={reason}"));
            self.finish_probe(ProbeOutcome::Unsupported);
            return;
        }
        if let Some(identifier) = self.selected_identifier.clone() {
            self.actions
                .push_back(LightingSessionAction::CancelConnect {
                    platform_identifier: identifier.clone(),
                });
            self.discard_candidate(identifier);
        }
        if self.preferred_identifier.is_none() {
            self.selected_identifier = None;
            self.selected_name = None;
            self.transition(LightingSessionState::Scanning);
            self.record(format!("candidate_rejected reason={reason}"));
            self.actions.push_back(LightingSessionAction::Scan);
        } else {
            self.transition(LightingSessionState::Failed { reason });
        }
    }

    pub fn select(&mut self, identifier: &str) {
        if (!self.is_probing() && self.state != LightingSessionState::Scanning)
            || self.preferred_identifier.is_some()
            || self.invalid_preferred_identifier
            || !self.verified.contains(identifier)
        {
            return;
        }
        let Some(candidate) = self.candidates.get(identifier).cloned() else {
            return;
        };
        if self.is_probing() {
            if let Some(probe) = self.selected_identifier.take() {
                self.actions
                    .push_back(LightingSessionAction::CancelConnect {
                        platform_identifier: probe.clone(),
                    });
                self.probe_queue.push_front(probe);
            }
            self.purpose = ConnectionPurpose::Selected;
        }
        self.selected_identifier = Some(identifier.to_owned());
        self.selected_name.clone_from(&candidate.name);
        self.actions.push_back(LightingSessionAction::StopScan);
        self.actions.push_back(LightingSessionAction::Connect {
            platform_identifier: identifier.to_owned(),
        });
        self.transition(LightingSessionState::Connecting);
        self.arm(LightingTimer::ConnectionAttempt, CONNECTION_TIMEOUT_MS);
        self.record(format!("selected=melk id={identifier}"));
    }

    fn arm(&mut self, timer: LightingTimer, delay_milliseconds: u64) {
        self.timer = Some(timer);
        self.actions.push_back(LightingSessionAction::ArmTimer {
            timer,
            delay_milliseconds,
        });
    }

    fn connect_selected(&mut self) {
        let Some(identifier) = self.selected_identifier.clone() else {
            return;
        };
        if !self.bluetooth_powered_on {
            self.pending_restoration = Some(RestoredConnection::Disconnected);
            return;
        }
        self.transition(LightingSessionState::Connecting);
        self.actions.push_back(LightingSessionAction::Connect {
            platform_identifier: identifier,
        });
        self.arm(LightingTimer::ConnectionAttempt, CONNECTION_TIMEOUT_MS);
    }

    fn resume_restoration(&mut self, connection: RestoredConnection) {
        if !self.bluetooth_powered_on {
            self.pending_restoration = Some(connection);
            return;
        }
        self.pending_restoration = None;
        match connection {
            RestoredConnection::Connected => {
                let Some(identifier) = self.selected_identifier.clone() else {
                    return;
                };
                self.transition(LightingSessionState::Discovering);
                self.arm(
                    LightingTimer::Discovery,
                    if self.is_probing() {
                        PROBE_TIMEOUT_MS
                    } else {
                        CONNECTION_TIMEOUT_MS
                    },
                );
                self.actions
                    .push_back(LightingSessionAction::DiscoverServices {
                        platform_identifier: identifier,
                        service: cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid(),
                    });
            }
            RestoredConnection::Pending => {
                self.transition(LightingSessionState::Connecting);
                self.arm(LightingTimer::ConnectionAttempt, CONNECTION_TIMEOUT_MS);
            }
            RestoredConnection::Disconnected => self.connect_selected(),
        }
    }

    fn schedule_reconnect(&mut self, reason: &str) {
        self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
        if self.reconnect_attempt > MAX_RECONNECT_ATTEMPTS {
            self.transition(LightingSessionState::Failed {
                reason: format!(
                    "Accessory reconnect exhausted after {MAX_RECONNECT_ATTEMPTS} attempts"
                ),
            });
            self.record(format!("reconnect_exhausted reason={reason}"));
            return;
        }
        let delay = 250_u64 * (1_u64 << (self.reconnect_attempt - 1));
        self.transition(LightingSessionState::Retrying {
            attempt: self.reconnect_attempt,
            delay_milliseconds: delay,
        });
        self.record(format!(
            "reconnect_attempt={} delay_ms={delay} reason={reason}",
            self.reconnect_attempt
        ));
        self.arm(LightingTimer::Reconnect, delay);
    }

    fn connection_timed_out(&mut self) {
        if self.is_probing() {
            self.finish_probe(ProbeOutcome::Unavailable);
            return;
        }
        self.timer = None;
        if let Some(identifier) = self.selected_identifier.take() {
            self.actions
                .push_back(LightingSessionAction::CancelConnect {
                    platform_identifier: identifier,
                });
        }
        self.selected_name = None;
        if self.preferred_identifier.is_some() {
            self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
            if self.reconnect_attempt > MAX_RECONNECT_ATTEMPTS {
                self.transition(LightingSessionState::Failed {
                    reason: format!(
                        "Accessory reconnect exhausted after {MAX_RECONNECT_ATTEMPTS} attempts"
                    ),
                });
                self.record("connect_timeout reconnect_exhausted");
                return;
            }
        }
        self.transition(LightingSessionState::Scanning);
        self.actions.push_back(LightingSessionAction::Scan);
        self.record("connect_timeout");
    }

    pub fn queue_writes<I>(&mut self, writes: I) -> bool
    where
        I: IntoIterator<Item = LightingWrite>,
    {
        let writes: Vec<_> = writes.into_iter().collect();
        if writes
            .iter()
            .any(|write| write.mode != LightingWriteMode::WithoutResponse)
        {
            return false;
        }
        let coalescible = writes.iter().any(is_coalescible_color_write);
        let retained = if coalescible {
            self.writes.len().saturating_sub(
                self.writes
                    .iter()
                    .filter(|item| is_coalescible_color_write(item))
                    .count(),
            )
        } else {
            self.writes.len()
        };
        if retained + writes.len() > 32 {
            return false;
        }
        if coalescible {
            self.writes.retain(|item| !is_coalescible_color_write(item));
        }
        self.writes.extend(writes);
        self.command_status = 1;
        true
    }

    pub fn queue_write(&mut self, write: LightingWrite) -> bool {
        self.queue_writes([write])
    }

    fn drain_initialization(&mut self, can_send: bool) {
        if !can_send
            || self.gatt != GattState::Subscribed
            || !(match self.state {
                LightingSessionState::Discovering => true,
                _ => false,
            })
            || self.timer.is_some()
        {
            return;
        }
        let Some(write) = self.initialization.pop_front() else {
            self.transition(LightingSessionState::Ready);
            self.record("notify_state=true");
            return;
        };
        let interval = write
            .minimum_interval_ms
            .unwrap_or(FALLBACK_WRITE_INTERVAL_MS);
        self.emit_write(write);
        if self.initialization.is_empty() {
            self.arm(LightingTimer::Initialization, u64::from(interval));
        } else {
            self.arm(LightingTimer::Initialization, INITIALIZATION_DELAY_MS);
        }
    }

    pub fn drain_writes(&mut self, can_send: bool) {
        if !can_send
            || !(match self.state {
                LightingSessionState::Ready => true,
                _ => false,
            })
            || self.timer.is_some()
        {
            return;
        }
        let Some(write) = self.writes.pop_front() else {
            return;
        };
        self.record(format!("requested={}", hex::encode(&write.payload)));
        let interval = write
            .minimum_interval_ms
            .unwrap_or(FALLBACK_WRITE_INTERVAL_MS);
        self.emit_write(write);
        self.arm(LightingTimer::WriteDrain, u64::from(interval));
    }

    fn emit_write(&mut self, write: LightingWrite) {
        let Some(identifier) = self.selected_identifier.clone() else {
            return;
        };
        self.actions.push_back(LightingSessionAction::Write {
            platform_identifier: identifier,
            write,
        });
    }

    #[allow(
        clippy::too_many_lines,
        reason = "The reducer keeps event transitions explicit and auditable."
    )]
    pub fn handle(&mut self, event: LightingSessionEvent) {
        if !self.reconnect_enabled {
            return;
        }
        match event {
            LightingSessionEvent::Resume => {
                if !self.reconnect_enabled || self.invalid_preferred_identifier {
                    return;
                }
                match self.state {
                    LightingSessionState::Failed { .. } | LightingSessionState::Disconnected => {
                        let preferred_identifier = self.preferred_identifier.clone();
                        self.start(preferred_identifier.as_deref());
                        self.actions
                            .push_back(LightingSessionAction::RestartTransport);
                    }
                    _ => {}
                }
            }
            LightingSessionEvent::BluetoothState {
                powered_on,
                state_code,
            } => {
                self.bluetooth_powered_on = powered_on;
                if self.invalid_preferred_identifier {
                    self.transition(LightingSessionState::Failed {
                        reason: "Remembered lighting identity is invalid".into(),
                    });
                    self.record("scan=refused invalid remembered identity");
                } else if !powered_on {
                    self.forget_selected_connection();
                    self.transition(LightingSessionState::Failed {
                        reason: format!("Bluetooth unavailable: {state_code}"),
                    });
                } else if let Some(connection) = self.pending_restoration.take() {
                    self.resume_restoration(connection);
                } else if self.selected_identifier.is_none() {
                    if let Some(identifier) = self.preferred_identifier.clone() {
                        self.actions
                            .push_back(LightingSessionAction::RestorePeripheral {
                                platform_identifier: identifier,
                            });
                    } else {
                        self.actions.push_back(LightingSessionAction::Scan);
                        self.transition(LightingSessionState::Scanning);
                        self.record("scan=melk services=all; gatt=FFF0 post-connect");
                    }
                }
            }
            LightingSessionEvent::Discovered {
                name,
                platform_identifier,
                rssi,
            } => {
                if (!self.is_probing() && self.state != LightingSessionState::Scanning)
                    || !self.accepts(&platform_identifier)
                    || self.rejected.contains(&platform_identifier)
                {
                    self.discard_candidate(platform_identifier);
                    return;
                }
                if self.candidates.len() >= MAX_CANDIDATES
                    && !self.candidates.contains_key(&platform_identifier)
                {
                    self.discard_candidate(platform_identifier);
                    return;
                }
                let candidate = LightingSessionCandidate {
                    platform_identifier: platform_identifier.clone(),
                    name: name.clone(),
                    rssi,
                };
                let is_new = !self.candidates.contains_key(&platform_identifier);
                self.candidates
                    .insert(platform_identifier.clone(), candidate.clone());
                self.record(format!(
                    "candidate={} id={platform_identifier} rssi={rssi}",
                    name.as_deref().unwrap_or_default()
                ));
                if self.preferred_identifier.is_some() {
                    self.selected_identifier = Some(platform_identifier.clone());
                    self.selected_name = name;
                    self.actions.push_back(LightingSessionAction::StopScan);
                    self.connect_selected();
                } else if self.verified.contains(&platform_identifier) {
                    self.candidates_out.push_back(candidate);
                } else if is_new
                    || (self.selected_identifier.as_deref() != Some(platform_identifier.as_str())
                        && !self.probe_queue.contains(&platform_identifier))
                {
                    self.probe_queue.push_back(platform_identifier);
                    self.probe_next();
                }
            }
            LightingSessionEvent::RestoreUnavailable => {
                if self.bluetooth_powered_on
                    && self.preferred_identifier.is_some()
                    && self.selected_identifier.is_none()
                {
                    self.actions.push_back(LightingSessionAction::Scan);
                    self.transition(LightingSessionState::Scanning);
                    self.record("scan=melk services=all; gatt=FFF0 post-connect");
                }
            }
            LightingSessionEvent::Restored {
                name,
                platform_identifier,
                connected,
                pending,
            } => {
                if self.preferred_identifier.is_none() || !self.accepts(&platform_identifier) {
                    self.record("restore=melk ignored different identity");
                    return;
                }
                self.selected_identifier = Some(platform_identifier.clone());
                self.selected_name = name;
                self.record(format!("restore=melk id={platform_identifier}"));
                let connection = if connected {
                    RestoredConnection::Connected
                } else if pending {
                    RestoredConnection::Pending
                } else {
                    RestoredConnection::Disconnected
                };
                self.resume_restoration(connection);
            }
            LightingSessionEvent::Connected {
                name,
                platform_identifier,
            } => {
                if self.selected_identifier.as_deref() != Some(platform_identifier.as_str()) {
                    return;
                }
                if name.is_some() {
                    self.selected_name = name;
                }
                self.timer = None;
                self.resume_restoration(RestoredConnection::Connected);
            }
            LightingSessionEvent::ConnectFailed { reason } => {
                if self.state != LightingSessionState::Connecting
                    && self.pending_restoration.is_none()
                {
                    return;
                }
                if self.is_probing() {
                    self.finish_probe(ProbeOutcome::Unavailable);
                    return;
                }
                self.timer = None;
                self.pending_restoration = None;
                if self.reconnect_enabled {
                    self.schedule_reconnect(&reason);
                } else {
                    self.transition(LightingSessionState::Failed { reason });
                }
            }
            LightingSessionEvent::ConnectTimeout => {
                if match self.timer {
                    Some(LightingTimer::ConnectionAttempt) => true,
                    _ => false,
                } {
                    self.connection_timed_out();
                }
            }
            LightingSessionEvent::ServicesDiscovered {
                service_uuids,
                error,
            } => {
                if self.state != LightingSessionState::Discovering {
                    return;
                }
                if let Some(reason) = error {
                    if self.is_probing() {
                        self.finish_probe(ProbeOutcome::Unavailable);
                    } else {
                        self.reject_candidate(reason);
                    }
                } else if !service_uuids
                    .iter()
                    .any(|uuid| *uuid == cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid())
                {
                    self.record("gatt=missing FFF0 service");
                    self.reject_candidate("missing FFF0 service".into());
                } else if let Some(identifier) = self.selected_identifier.clone() {
                    self.actions
                        .push_back(LightingSessionAction::DiscoverCharacteristics {
                            platform_identifier: identifier,
                            service: cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid(),
                        });
                }
            }
            LightingSessionEvent::CharacteristicsDiscovered {
                name,
                service_uuid,
                characteristics,
                error,
            } => {
                if self.state != LightingSessionState::Discovering {
                    return;
                }
                if name.is_some() {
                    self.selected_name = name;
                }
                if let Some(reason) = error {
                    if self.is_probing() {
                        self.finish_probe(ProbeOutcome::Unavailable);
                    } else {
                        self.reject_candidate(reason);
                    }
                } else if service_uuid != cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid() {
                    self.reject_candidate("missing FFF0 service".into());
                } else if !characteristics.iter().any(|characteristic| {
                    characteristic.uuid == cutout_protocols::MELK_WRITE_CHANNEL.as_uuid()
                        && characteristic.write_without_response
                }) {
                    self.reject_candidate("missing FFF3 write characteristic".into());
                } else if !characteristics.iter().any(|characteristic| {
                    characteristic.uuid == cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid()
                        && characteristic.notify_or_indicate
                }) {
                    self.reject_candidate("missing FFF4 notification characteristic".into());
                } else if self.is_probing() {
                    self.finish_probe(ProbeOutcome::Supported);
                } else {
                    self.gatt = GattState::Verified;
                    self.initialization = MelkLightingProfile::initialization_actions()
                        .into_iter()
                        .map(transport_action)
                        .collect();
                    self.record("gatt=FFF0 write=FFF3 notify=FFF4");
                    if let Some(identifier) = self.selected_identifier.clone() {
                        self.actions.push_back(LightingSessionAction::Subscribe {
                            platform_identifier: identifier,
                            characteristic: MelkLightingProfile::write_policy()
                                .confirmation_channel
                                .as_uuid(),
                        });
                    }
                }
            }
            LightingSessionEvent::NotificationState {
                characteristic,
                ready,
                can_send,
                error,
            } => {
                if self.state != LightingSessionState::Discovering
                    && self.state != LightingSessionState::Ready
                {
                    return;
                }
                if characteristic != cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid() {
                    return;
                }
                if let Some(reason) = error {
                    self.transition(LightingSessionState::Failed { reason });
                } else if !ready {
                    self.transition(LightingSessionState::Failed {
                        reason: "FFF4 notify unavailable".into(),
                    });
                } else {
                    if self.gatt == GattState::Unverified {
                        return;
                    }
                    if self.timer == Some(LightingTimer::Discovery) {
                        self.timer = None;
                    }
                    self.gatt = GattState::Subscribed;
                    self.drain_initialization(can_send);
                }
            }
            LightingSessionEvent::Notification {
                characteristic,
                bytes,
            } => {
                if characteristic == cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid() {
                    self.notifications.push_back(bytes);
                }
            }
            LightingSessionEvent::WriteReady { can_send } => {
                self.drain_initialization(can_send);
                self.drain_writes(can_send);
            }
            LightingSessionEvent::TimerFired { timer, can_send } => {
                if self.timer != Some(timer) {
                    return;
                }
                match timer {
                    LightingTimer::ConnectionAttempt => self.connection_timed_out(),
                    LightingTimer::Discovery => {
                        self.record("gatt_timeout");
                        self.connection_timed_out();
                    }
                    LightingTimer::Reconnect => {
                        self.timer = None;
                        self.connect_selected();
                    }
                    LightingTimer::Initialization => {
                        self.timer = None;
                        self.drain_initialization(can_send);
                    }
                    LightingTimer::WriteDrain => {
                        self.timer = None;
                        self.drain_writes(can_send);
                    }
                }
            }
            LightingSessionEvent::Disconnected { reason, powered_on } => {
                if powered_on
                    && self.pending_restoration.is_none()
                    && match self.state {
                        LightingSessionState::Connecting
                        | LightingSessionState::Discovering
                        | LightingSessionState::Ready => false,
                        _ => true,
                    }
                {
                    return;
                }
                self.bluetooth_powered_on = powered_on;
                if self.is_probing() {
                    self.finish_probe(ProbeOutcome::Unavailable);
                    return;
                }
                self.pending_restoration = None;
                self.record(format!("disconnected error={reason}"));
                if self.reconnect_enabled && powered_on {
                    self.schedule_reconnect(&reason);
                } else {
                    self.forget_selected_connection();
                    self.transition(LightingSessionState::Disconnected);
                }
            }
        }
    }

    pub fn mark_last_command_confirmed(&mut self) -> bool {
        if self.command_status != 1 || !self.writes.is_empty() {
            return false;
        }
        self.command_status = 2;
        true
    }

    pub fn mark_last_command_unconfirmed(&mut self) {
        if self.command_status == 1 {
            self.command_status = 3;
        }
    }

    pub fn drain_actions(&mut self) -> Vec<LightingSessionAction> {
        self.actions.drain(..).collect()
    }

    pub fn drain_records(&mut self) -> Vec<String> {
        self.records.drain(..).collect()
    }

    pub fn drain_candidates(&mut self) -> Vec<LightingSessionCandidate> {
        self.candidates_out.drain(..).collect()
    }

    pub fn drain_candidate_removals(&mut self) -> Vec<String> {
        self.candidate_removals_out.drain(..).collect()
    }

    pub fn drain_notifications(&mut self) -> Vec<Vec<u8>> {
        self.notifications.drain(..).collect()
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        match self.state {
            LightingSessionState::Ready => true,
            _ => false,
        }
    }
}

fn is_coalescible_color_write(write: &LightingWrite) -> bool {
    write.confirmation_characteristic
        == MelkLightingProfile::write_policy()
            .confirmation_channel
            .as_uuid()
        && write.characteristic == MelkLightingProfile::write_policy().channel.as_uuid()
        && write.payload.len() == 9
        && write.payload[0] == 0x7e
        && write.payload[1] == 0
        && write.payload[2] == 5
        && write.payload[3] == 3
        && write.payload[8] == 0xef
}
