//! Pure Rust session reducer for the MELK `CoreBluetooth` adapter.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Write as _,
};

use cutout_protocols::{MelkGattEvidence, MelkLightingProfile};

use super::contract::{
    MobileMelkLightingSessionActionDto, MobileMelkLightingSessionCandidateDto,
    MobileMelkLightingSessionEventDto, MobileMelkLightingSessionSnapshotDto,
    MobileMelkLightingSessionStateDto, MobileMelkLightingTimerDto,
};
use crate::{
    MobileMelkLightingWriteDto, MobileMelkLightingWriteModeDto, mobile_melk_transport_action,
};

const MAX_CANDIDATES: usize = 32;
const CONNECTION_TIMEOUT_MS: u64 = 15_000;
const INITIALIZATION_DELAY_MS: u64 = 1_000;
const MAX_RECONNECT_ATTEMPTS: u8 = 3;
const FALLBACK_WRITE_INTERVAL_MS: u16 = 50;

#[derive(Clone, Debug)]
pub(crate) struct SessionReducer {
    state: MobileMelkLightingSessionStateDto,
    preferred_identifier: Option<String>,
    invalid_preferred_identifier: bool,
    selected_identifier: Option<String>,
    selected_name: Option<String>,
    candidates: BTreeMap<String, MobileMelkLightingSessionCandidateDto>,
    reconnect_enabled: bool,
    reconnect_attempt: u8,
    notification_ready: bool,
    initialization: VecDeque<MobileMelkLightingWriteDto>,
    writes: VecDeque<MobileMelkLightingWriteDto>,
    timer: Option<MobileMelkLightingTimerDto>,
    command_status: u8,
    actions: VecDeque<MobileMelkLightingSessionActionDto>,
    records: VecDeque<String>,
    notifications: VecDeque<Vec<u8>>,
    candidates_out: VecDeque<MobileMelkLightingSessionCandidateDto>,
}

impl Default for SessionReducer {
    fn default() -> Self {
        Self {
            state: MobileMelkLightingSessionStateDto::Idle,
            preferred_identifier: None,
            invalid_preferred_identifier: false,
            selected_identifier: None,
            selected_name: None,
            candidates: BTreeMap::new(),
            reconnect_enabled: true,
            reconnect_attempt: 0,
            notification_ready: false,
            initialization: VecDeque::new(),
            writes: VecDeque::new(),
            timer: None,
            command_status: 0,
            actions: VecDeque::new(),
            records: VecDeque::new(),
            notifications: VecDeque::new(),
            candidates_out: VecDeque::new(),
        }
    }
}

impl SessionReducer {
    pub(crate) fn start(&mut self, preferred_identifier: Option<&str>) {
        self.actions.clear();
        self.records.clear();
        self.notifications.clear();
        self.candidates_out.clear();
        self.preferred_identifier = preferred_identifier.map(str::to_owned);
        self.invalid_preferred_identifier =
            preferred_identifier.is_some_and(|value| uuid::Uuid::parse_str(value).is_err());
        self.reconnect_enabled = true;
        self.reconnect_attempt = 0;
        self.candidates.clear();
        self.selected_identifier = None;
        self.selected_name = None;
        self.timer = None;
        self.command_status = 0;
        self.transition(MobileMelkLightingSessionStateDto::Idle);
    }

    pub(crate) fn stop(&mut self) {
        self.reconnect_enabled = false;
        self.timer = None;
        self.actions.clear();
        self.transition(MobileMelkLightingSessionStateDto::Disconnected);
    }

    pub(crate) fn snapshot(&self) -> MobileMelkLightingSessionSnapshotDto {
        MobileMelkLightingSessionSnapshotDto {
            state: self.state.clone(),
            platform_identifier: self.selected_identifier.clone(),
            name: self.selected_name.clone(),
            command_status: self.command_status,
            notification_ready: self.notification_ready,
        }
    }

    fn record(&mut self, value: impl Into<String>) {
        self.records.push_back(value.into());
    }

    fn transition(&mut self, state: MobileMelkLightingSessionStateDto) {
        if matches!(state, MobileMelkLightingSessionStateDto::Ready) {
            self.reconnect_attempt = 0;
        } else {
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
        self.selected_identifier = None;
        self.selected_name = None;
    }

    fn reset_initialization(&mut self) {
        self.initialization.clear();
        self.notification_ready = false;
        if matches!(self.timer, Some(MobileMelkLightingTimerDto::Initialization)) {
            self.timer = None;
        }
    }

    fn is_melk_name(name: Option<&str>) -> bool {
        name.is_some_and(|value| value.trim().to_ascii_lowercase().starts_with("melk"))
    }

    fn accepts(&self, identifier: &str) -> bool {
        !self.invalid_preferred_identifier
            && self
                .preferred_identifier
                .as_deref()
                .is_none_or(|preferred| {
                    uuid::Uuid::parse_str(preferred).ok() == uuid::Uuid::parse_str(identifier).ok()
                })
    }

    fn accepts_discovery(&self, name: Option<&str>, identifier: &str) -> bool {
        self.accepts(identifier)
            && (self.preferred_identifier.is_some()
                || name.map(str::trim).is_some_and(|v| !v.is_empty()))
    }

    fn selected_id(&self) -> Option<&str> {
        self.selected_identifier.as_deref()
    }

    fn reject_candidate(&mut self, reason: String) {
        if self.preferred_identifier.is_none() {
            if let Some(identifier) = self.selected_id().map(str::to_owned) {
                self.actions
                    .push_back(MobileMelkLightingSessionActionDto::CancelConnect {
                        platform_identifier: identifier,
                    });
            }
            self.selected_identifier = None;
            self.selected_name = None;
            self.transition(MobileMelkLightingSessionStateDto::Scanning);
            self.record(format!("candidate_rejected reason={reason}"));
            self.actions
                .push_back(MobileMelkLightingSessionActionDto::Scan);
        } else {
            self.transition(MobileMelkLightingSessionStateDto::Failed { reason });
        }
    }

    pub(crate) fn select(&mut self, identifier: &str) {
        if self.preferred_identifier.is_some()
            || self.invalid_preferred_identifier
            || self.selected_identifier.is_some()
        {
            return;
        }
        let Some(candidate) = self.candidates.get(identifier).cloned() else {
            return;
        };
        self.selected_identifier = Some(identifier.to_owned());
        self.selected_name.clone_from(&candidate.name);
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::StopScan);
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::Connect {
                platform_identifier: identifier.to_owned(),
            });
        self.transition(MobileMelkLightingSessionStateDto::Connecting);
        self.arm(
            MobileMelkLightingTimerDto::ConnectionAttempt,
            CONNECTION_TIMEOUT_MS,
        );
        self.record(format!("selected=melk id={identifier}"));
    }

    fn arm(&mut self, timer: MobileMelkLightingTimerDto, delay_milliseconds: u64) {
        self.timer = Some(timer);
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::ArmTimer {
                timer,
                delay_milliseconds,
            });
    }

    fn connect_selected(&mut self) {
        let Some(identifier) = self.selected_identifier.clone() else {
            return;
        };
        self.transition(MobileMelkLightingSessionStateDto::Connecting);
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::Connect {
                platform_identifier: identifier,
            });
        self.arm(
            MobileMelkLightingTimerDto::ConnectionAttempt,
            CONNECTION_TIMEOUT_MS,
        );
    }

    fn schedule_reconnect(&mut self, reason: &str) {
        self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
        if self.reconnect_attempt > MAX_RECONNECT_ATTEMPTS {
            self.transition(MobileMelkLightingSessionStateDto::Failed {
                reason: format!(
                    "Accessory reconnect exhausted after {MAX_RECONNECT_ATTEMPTS} attempts"
                ),
            });
            self.record(format!("reconnect_exhausted reason={reason}"));
            return;
        }
        let delay = 250_u64 * (1_u64 << (self.reconnect_attempt - 1));
        self.transition(MobileMelkLightingSessionStateDto::Retrying {
            attempt: self.reconnect_attempt,
            delay_milliseconds: delay,
        });
        self.record(format!(
            "reconnect_attempt={} delay_ms={delay} reason={reason}",
            self.reconnect_attempt
        ));
        self.arm(MobileMelkLightingTimerDto::Reconnect, delay);
    }

    fn connection_timed_out(&mut self) {
        self.timer = None;
        if let Some(identifier) = self.selected_identifier.take() {
            self.actions
                .push_back(MobileMelkLightingSessionActionDto::CancelConnect {
                    platform_identifier: identifier,
                });
        }
        self.selected_name = None;
        if self.preferred_identifier.is_some() {
            self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
            if self.reconnect_attempt > MAX_RECONNECT_ATTEMPTS {
                self.transition(MobileMelkLightingSessionStateDto::Failed {
                    reason: format!(
                        "Accessory reconnect exhausted after {MAX_RECONNECT_ATTEMPTS} attempts"
                    ),
                });
                self.record("connect_timeout reconnect_exhausted");
                return;
            }
        }
        self.transition(MobileMelkLightingSessionStateDto::Scanning);
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::Scan);
        self.record("connect_timeout");
    }

    pub(crate) fn queue_writes<I>(&mut self, writes: I) -> bool
    where
        I: IntoIterator<Item = MobileMelkLightingWriteDto>,
    {
        let writes: Vec<_> = writes.into_iter().collect();
        if writes
            .iter()
            .any(|write| write.mode != MobileMelkLightingWriteModeDto::WithoutResponse)
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

    pub(crate) fn queue_write(&mut self, write: MobileMelkLightingWriteDto) -> bool {
        self.queue_writes([write])
    }

    fn drain_initialization(&mut self, can_send: bool) {
        if !can_send
            || !self.notification_ready
            || !matches!(self.state, MobileMelkLightingSessionStateDto::Discovering)
            || self.timer.is_some()
        {
            return;
        }
        let Some(write) = self.initialization.pop_front() else {
            self.transition(MobileMelkLightingSessionStateDto::Ready);
            self.record("notify_state=true");
            return;
        };
        let interval = write
            .minimum_interval_ms
            .unwrap_or(FALLBACK_WRITE_INTERVAL_MS);
        self.emit_write(write);
        if self.initialization.is_empty() {
            self.arm(
                MobileMelkLightingTimerDto::Initialization,
                u64::from(interval),
            );
        } else {
            self.arm(
                MobileMelkLightingTimerDto::Initialization,
                INITIALIZATION_DELAY_MS,
            );
        }
    }

    pub(crate) fn drain_writes(&mut self, can_send: bool) {
        if !can_send
            || !matches!(self.state, MobileMelkLightingSessionStateDto::Ready)
            || self.timer.is_some()
        {
            return;
        }
        let Some(write) = self.writes.pop_front() else {
            return;
        };
        self.record(format!("requested={}", hex(&write.payload)));
        let interval = write
            .minimum_interval_ms
            .unwrap_or(FALLBACK_WRITE_INTERVAL_MS);
        self.emit_write(write);
        self.arm(MobileMelkLightingTimerDto::WriteDrain, u64::from(interval));
    }

    fn emit_write(&mut self, write: MobileMelkLightingWriteDto) {
        let Some(identifier) = self.selected_identifier.clone() else {
            return;
        };
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::Write {
                platform_identifier: identifier,
                write,
            });
    }

    #[allow(
        clippy::too_many_lines,
        reason = "The reducer keeps event transitions explicit and auditable."
    )]
    pub(crate) fn handle(&mut self, event: MobileMelkLightingSessionEventDto) {
        match event {
            MobileMelkLightingSessionEventDto::BluetoothState {
                powered_on,
                state_code,
            } => {
                if self.invalid_preferred_identifier {
                    self.transition(MobileMelkLightingSessionStateDto::Failed {
                        reason: "Remembered lighting identity is invalid".into(),
                    });
                    self.record("scan=refused invalid remembered identity");
                } else if !powered_on {
                    self.forget_selected_connection();
                    self.transition(MobileMelkLightingSessionStateDto::Failed {
                        reason: format!("Bluetooth unavailable: {state_code}"),
                    });
                } else if self.selected_identifier.is_none() && self.preferred_identifier.is_none()
                {
                    self.actions
                        .push_back(MobileMelkLightingSessionActionDto::Scan);
                    self.transition(MobileMelkLightingSessionStateDto::Scanning);
                    self.record("scan=melk services=all; gatt=FFF0 post-connect");
                }
            }
            MobileMelkLightingSessionEventDto::Discovered {
                name,
                platform_identifier,
                rssi,
            } => {
                if self.selected_identifier.is_some()
                    || !self.accepts_discovery(name.as_deref(), &platform_identifier)
                {
                    return;
                }
                if self.candidates.len() >= MAX_CANDIDATES
                    && !self.candidates.contains_key(&platform_identifier)
                {
                    let Some(evicted) = self
                        .candidates
                        .iter()
                        .find(|(_, candidate)| !Self::is_melk_name(candidate.name.as_deref()))
                        .map(|(id, _)| id.clone())
                    else {
                        return;
                    };
                    self.candidates.remove(&evicted);
                }
                let candidate = MobileMelkLightingSessionCandidateDto {
                    platform_identifier: platform_identifier.clone(),
                    name: name.clone(),
                    rssi,
                };
                self.candidates
                    .insert(platform_identifier.clone(), candidate.clone());
                self.record(format!(
                    "candidate={} id={platform_identifier} rssi={rssi}",
                    name.as_deref().unwrap_or_default()
                ));
                if self.preferred_identifier.is_some() {
                    self.selected_identifier = Some(platform_identifier.clone());
                    self.selected_name = name;
                    self.actions
                        .push_back(MobileMelkLightingSessionActionDto::StopScan);
                    self.connect_selected();
                } else {
                    self.candidates_out.push_back(candidate);
                }
            }
            MobileMelkLightingSessionEventDto::RestoreUnavailable => {
                if self.preferred_identifier.is_some() && self.selected_identifier.is_none() {
                    self.actions
                        .push_back(MobileMelkLightingSessionActionDto::Scan);
                    self.transition(MobileMelkLightingSessionStateDto::Scanning);
                    self.record("scan=melk services=all; gatt=FFF0 post-connect");
                }
            }
            MobileMelkLightingSessionEventDto::Restored {
                name,
                platform_identifier,
                connected,
                pending,
            } => {
                if !self.accepts(&platform_identifier) {
                    self.record("restore=melk ignored different identity");
                    return;
                }
                self.selected_identifier = Some(platform_identifier.clone());
                self.selected_name = name;
                self.record(format!("restore=melk id={platform_identifier}"));
                if connected {
                    self.transition(MobileMelkLightingSessionStateDto::Discovering);
                    self.actions
                        .push_back(MobileMelkLightingSessionActionDto::DiscoverServices {
                            platform_identifier,
                            service: cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid().into(),
                        });
                } else if pending {
                    self.transition(MobileMelkLightingSessionStateDto::Connecting);
                    self.arm(
                        MobileMelkLightingTimerDto::ConnectionAttempt,
                        CONNECTION_TIMEOUT_MS,
                    );
                } else {
                    self.connect_selected();
                }
            }
            MobileMelkLightingSessionEventDto::Connected {
                name,
                platform_identifier,
            } => {
                if self.selected_identifier.as_deref() != Some(platform_identifier.as_str()) {
                    return;
                }
                self.selected_name = name;
                self.timer = None;
                self.transition(MobileMelkLightingSessionStateDto::Discovering);
                self.actions
                    .push_back(MobileMelkLightingSessionActionDto::DiscoverServices {
                        platform_identifier,
                        service: cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid().into(),
                    });
            }
            MobileMelkLightingSessionEventDto::ConnectFailed { reason } => {
                self.timer = None;
                if self.reconnect_enabled {
                    self.schedule_reconnect(&reason);
                } else {
                    self.transition(MobileMelkLightingSessionStateDto::Failed { reason });
                }
            }
            MobileMelkLightingSessionEventDto::ConnectTimeout => {
                if matches!(
                    self.timer,
                    Some(MobileMelkLightingTimerDto::ConnectionAttempt)
                ) {
                    self.connection_timed_out();
                }
            }
            MobileMelkLightingSessionEventDto::ServicesDiscovered {
                service_uuids,
                error,
            } => {
                if let Some(reason) = error {
                    self.reject_candidate(reason);
                } else if !service_uuids.iter().any(|uuid| {
                    uuid::Uuid::from(*uuid) == cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid()
                }) {
                    self.record("gatt=missing FFF0 service");
                    self.reject_candidate("missing FFF0 service".into());
                } else if let Some(identifier) = self.selected_identifier.clone() {
                    self.actions.push_back(
                        MobileMelkLightingSessionActionDto::DiscoverCharacteristics {
                            platform_identifier: identifier,
                            service: cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid().into(),
                        },
                    );
                }
            }
            MobileMelkLightingSessionEventDto::CharacteristicsDiscovered {
                name,
                service_uuid,
                characteristics,
                error,
            } => {
                let name = name.or_else(|| {
                    self.preferred_identifier
                        .as_ref()
                        .map(|_| "MELK-OC21".into())
                });
                if let Some(reason) = error {
                    self.reject_candidate(reason);
                } else if uuid::Uuid::from(service_uuid)
                    != cutout_protocols::MELK_SERVICE_CHANNEL.as_uuid()
                {
                    self.reject_candidate("missing FFF0 service".into());
                } else if !characteristics.iter().any(|characteristic| {
                    uuid::Uuid::from(characteristic.uuid)
                        == cutout_protocols::MELK_WRITE_CHANNEL.as_uuid()
                        && characteristic.write_without_response
                }) {
                    self.reject_candidate("missing FFF3 write characteristic".into());
                } else if !characteristics.iter().any(|characteristic| {
                    uuid::Uuid::from(characteristic.uuid)
                        == cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid()
                        && characteristic.notify_or_indicate
                }) {
                    self.reject_candidate("missing FFF4 notification characteristic".into());
                } else if MelkLightingProfile::identify(
                    name.as_deref().unwrap_or_default(),
                    MelkGattEvidence::observed(),
                )
                .is_none()
                {
                    self.reject_candidate("invalid MELK profile".into());
                } else {
                    self.initialization = MelkLightingProfile::initialization_actions()
                        .into_iter()
                        .map(mobile_melk_transport_action)
                        .collect();
                    self.notification_ready = false;
                    self.record("gatt=FFF0 write=FFF3 notify=FFF4");
                    if let Some(identifier) = self.selected_identifier.clone() {
                        self.actions
                            .push_back(MobileMelkLightingSessionActionDto::Subscribe {
                                platform_identifier: identifier,
                                characteristic: MelkLightingProfile::write_policy()
                                    .confirmation_channel
                                    .as_uuid()
                                    .into(),
                            });
                    }
                }
            }
            MobileMelkLightingSessionEventDto::NotificationState {
                characteristic,
                ready,
                can_send,
                error,
            } => {
                if uuid::Uuid::from(characteristic)
                    != cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid()
                {
                    return;
                }
                if let Some(reason) = error {
                    self.transition(MobileMelkLightingSessionStateDto::Failed { reason });
                } else if !ready {
                    self.transition(MobileMelkLightingSessionStateDto::Failed {
                        reason: "FFF4 notify unavailable".into(),
                    });
                } else {
                    self.notification_ready = true;
                    self.drain_initialization(can_send);
                }
            }
            MobileMelkLightingSessionEventDto::Notification {
                characteristic,
                bytes,
            } => {
                if uuid::Uuid::from(characteristic)
                    == cutout_protocols::MELK_NOTIFY_CHANNEL.as_uuid()
                {
                    self.notifications.push_back(bytes);
                }
            }
            MobileMelkLightingSessionEventDto::WriteReady { can_send } => {
                self.drain_initialization(can_send);
                self.drain_writes(can_send);
            }
            MobileMelkLightingSessionEventDto::TimerFired { timer, can_send } => {
                if self.timer != Some(timer) {
                    return;
                }
                match timer {
                    MobileMelkLightingTimerDto::ConnectionAttempt => self.connection_timed_out(),
                    MobileMelkLightingTimerDto::Reconnect => {
                        self.timer = None;
                        self.connect_selected();
                    }
                    MobileMelkLightingTimerDto::Initialization => {
                        self.timer = None;
                        self.drain_initialization(can_send);
                    }
                    MobileMelkLightingTimerDto::WriteDrain => {
                        self.timer = None;
                        self.drain_writes(can_send);
                    }
                }
            }
            MobileMelkLightingSessionEventDto::Disconnected { reason, powered_on } => {
                self.record(format!("disconnected error={reason}"));
                if self.reconnect_enabled && powered_on {
                    self.schedule_reconnect(&reason);
                } else {
                    self.forget_selected_connection();
                    self.transition(MobileMelkLightingSessionStateDto::Disconnected);
                }
            }
        }
    }

    pub(crate) fn mark_last_command_confirmed(&mut self) {
        if self.command_status == 1 {
            self.command_status = 2;
        }
    }

    pub(crate) fn mark_last_command_unconfirmed(&mut self) {
        if self.command_status == 1 {
            self.command_status = 3;
        }
    }

    pub(crate) fn drain_actions(&mut self) -> Vec<MobileMelkLightingSessionActionDto> {
        self.actions.drain(..).collect()
    }

    pub(crate) fn drain_records(&mut self) -> Vec<String> {
        self.records.drain(..).collect()
    }

    pub(crate) fn drain_candidates(&mut self) -> Vec<MobileMelkLightingSessionCandidateDto> {
        self.candidates_out.drain(..).collect()
    }

    pub(crate) fn drain_notifications(&mut self) -> Vec<Vec<u8>> {
        self.notifications.drain(..).collect()
    }

    pub(crate) fn is_ready(&self) -> bool {
        matches!(self.state, MobileMelkLightingSessionStateDto::Ready)
    }
}

fn is_coalescible_color_write(write: &MobileMelkLightingWriteDto) -> bool {
    uuid::Uuid::from(write.confirmation_characteristic)
        == MelkLightingProfile::write_policy()
            .confirmation_channel
            .as_uuid()
        && uuid::Uuid::from(write.characteristic)
            == MelkLightingProfile::write_policy().channel.as_uuid()
        && write.payload.len() == 9
        && write.payload[0] == 0x7e
        && write.payload[1] == 0
        && write.payload[2] == 5
        && write.payload[3] == 3
        && write.payload[8] == 0xef
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
