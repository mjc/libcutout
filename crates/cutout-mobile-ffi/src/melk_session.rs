//! Rust-owned state machine for the standalone MELK CoreBluetooth adapter.

use std::collections::{BTreeMap, VecDeque};

use super::{
    MobileMelkLightingError, MobileMelkLightingWriteDto, MobileMelkLightingWriteModeDto,
    mobile_melk_transport_action,
};
use cutout_core::{
    LightingBrightness, LightingPowerState, MelkClock, MelkControl, MelkSchedule, RgbColor,
    RgbLightingCommand,
};
use cutout_protocols::{MelkGattEvidence, MelkLightingProfile};

const MAX_CANDIDATES: usize = 32;
const CONNECTION_TIMEOUT_MS: u64 = 15_000;
const INITIALIZATION_DELAY_MS: u64 = 1_000;
const MAX_RECONNECT_ATTEMPTS: u8 = 3;
const FALLBACK_WRITE_INTERVAL_MS: u16 = 50;

/// Rust-owned lifecycle state for the standalone MELK transport.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingSessionStateDto {
    /// No transport has been started.
    Idle,
    /// The adapter is discovering candidates.
    Scanning,
    /// A candidate connection is in flight.
    Connecting,
    /// A retry is waiting for its timer.
    Retrying {
        attempt: u8,
        delay_milliseconds: u64,
    },
    /// GATT roles and notifications are being prepared.
    Discovering,
    /// The verified profile is ready for commands.
    Ready,
    /// The adapter was explicitly stopped or cannot reconnect.
    Disconnected,
    /// A terminal error with a user-visible explanation.
    Failed { reason: String },
}

/// CoreBluetooth facts submitted to the Rust MELK reducer.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingSessionEventDto {
    /// CoreBluetooth changed power state.
    BluetoothState { powered_on: bool, state_code: i32 },
    /// A peripheral was observed while scanning.
    Discovered {
        name: Option<String>,
        platform_identifier: String,
        rssi: i32,
    },
    /// The platform selected or restored a peripheral.
    Restored {
        name: Option<String>,
        platform_identifier: String,
        connected: bool,
    },
    /// No remembered peripheral was available for the selected identity.
    RestoreUnavailable,
    /// A connection completed.
    Connected {
        name: Option<String>,
        platform_identifier: String,
    },
    /// A connection failed.
    ConnectFailed { reason: String },
    /// The connection attempt timer fired.
    ConnectTimeout,
    /// Services were discovered and the required service was present.
    ServicesDiscovered {
        service_uuids: Vec<Vec<u8>>,
        error: Option<String>,
    },
    /// Characteristics were discovered with their typed GATT roles.
    CharacteristicsDiscovered {
        name: Option<String>,
        service_uuid: Vec<u8>,
        characteristics: Vec<MobileMelkLightingCharacteristicEvidenceDto>,
        error: Option<String>,
    },
    /// Notification subscription state changed.
    NotificationState {
        characteristic: Vec<u8>,
        ready: bool,
        can_send: bool,
        error: Option<String>,
    },
    /// A notification arrived on the verified FFF4 channel.
    Notification {
        characteristic: Vec<u8>,
        bytes: Vec<u8>,
    },
    /// CoreBluetooth can accept another no-response write.
    WriteReady { can_send: bool },
    /// A reducer timer fired.
    TimerFired {
        timer: MobileMelkLightingTimerDto,
        can_send: bool,
    },
    /// The selected peripheral disconnected.
    Disconnected { reason: String, powered_on: bool },
}

/// Timers requested by the Rust reducer and scheduled by the platform adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingTimerDto {
    /// Bounds a pending connection attempt.
    ConnectionAttempt,
    /// Starts the next reconnect attempt.
    Reconnect,
    /// Advances the initialization sequence.
    Initialization,
    /// Advances the bounded user-write queue.
    WriteDrain,
}

/// CoreBluetooth operation requested by the Rust reducer.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMelkLightingSessionActionDto {
    /// Scan without a service filter; MELK does not advertise FFF0.
    Scan,
    /// Stop scanning before connecting to a selected candidate.
    StopScan,
    /// Connect to a platform-local peripheral identifier.
    Connect { platform_identifier: String },
    /// Cancel a failed first-pairing candidate.
    CancelConnect { platform_identifier: String },
    /// Discover the verified FFF0 service.
    DiscoverServices {
        platform_identifier: String,
        service: Vec<u8>,
    },
    /// Discover all characteristics for FFF0.
    DiscoverCharacteristics {
        platform_identifier: String,
        service: Vec<u8>,
    },
    /// Subscribe to FFF4 notifications.
    Subscribe {
        platform_identifier: String,
        characteristic: Vec<u8>,
    },
    /// Execute one typed FFF3 write.
    Write {
        platform_identifier: String,
        write: MobileMelkLightingWriteDto,
    },
    /// Arm a reducer timer on the platform queue.
    ArmTimer {
        timer: MobileMelkLightingTimerDto,
        delay_milliseconds: u64,
    },
}

/// One characteristic's UUID and native CoreBluetooth properties.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingCharacteristicEvidenceDto {
    /// Full 128-bit UUID bytes in canonical CoreBluetooth order.
    pub uuid: Vec<u8>,
    /// Whether the characteristic supports write-without-response.
    pub write_without_response: bool,
    /// Whether the characteristic supports notifications or indications.
    pub notify_or_indicate: bool,
}

/// A bounded candidate surfaced during first pairing.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingSessionCandidateDto {
    /// Platform-local CoreBluetooth identifier.
    pub platform_identifier: String,
    /// Advertised local name, when present.
    pub name: Option<String>,
    /// Last observed RSSI.
    pub rssi: i32,
}

/// Snapshot of Rust-owned session state for the thin platform wrapper.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMelkLightingSessionSnapshotDto {
    /// Current lifecycle state.
    pub state: MobileMelkLightingSessionStateDto,
    /// Selected platform identifier, when one is active.
    pub platform_identifier: Option<String>,
    /// Selected advertised name, when one is known.
    pub name: Option<String>,
    /// Last command evidence state: 0 idle, 1 requested, 2 confirmed, 3 unconfirmed.
    pub command_status: u8,
    /// Whether FFF4 notifications are ready.
    pub notification_ready: bool,
}

#[derive(Clone, Debug)]
struct SessionCore {
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

impl Default for SessionCore {
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

impl SessionCore {
    fn snapshot(&self) -> MobileMelkLightingSessionSnapshotDto {
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
        if !matches!(state, MobileMelkLightingSessionStateDto::Ready) {
            self.writes.clear();
            self.reset_initialization();
        }
        self.state = state;
    }

    fn reset_initialization(&mut self) {
        self.initialization.clear();
        self.notification_ready = false;
        if matches!(self.timer, Some(MobileMelkLightingTimerDto::Initialization)) {
            self.timer = None;
        }
    }

    fn is_melk_name(name: Option<&str>) -> bool {
        name.map(str::trim)
            .map(|value| value.to_ascii_lowercase().starts_with("melk"))
            .unwrap_or(false)
    }

    fn accepts(&self, identifier: &str) -> bool {
        !self.invalid_preferred_identifier
            && self
                .preferred_identifier
                .as_deref()
                .map(|preferred| {
                    uuid::Uuid::parse_str(preferred).ok() == uuid::Uuid::parse_str(identifier).ok()
                })
                .unwrap_or(true)
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

    fn select(&mut self, identifier: String) {
        if self.preferred_identifier.is_some()
            || self.invalid_preferred_identifier
            || self.selected_identifier.is_some()
        {
            return;
        }
        let Some(candidate) = self.candidates.get(&identifier).cloned() else {
            return;
        };
        self.selected_identifier = Some(identifier.clone());
        self.selected_name = candidate.name.clone();
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::StopScan);
        self.actions
            .push_back(MobileMelkLightingSessionActionDto::Connect {
                platform_identifier: identifier.clone(),
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

    fn schedule_reconnect(&mut self, reason: String) {
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

    fn queue_writes<I>(&mut self, writes: I) -> bool
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
        let retained = self.writes.len().saturating_sub(
            self.writes
                .iter()
                .filter(|item| is_coalescible_color_write(item))
                .count(),
        );
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

    fn queue_write(&mut self, write: MobileMelkLightingWriteDto) -> bool {
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
        self.emit_write(write);
        if self.initialization.is_empty() {
            self.transition(MobileMelkLightingSessionStateDto::Ready);
            self.record("notify_state=true");
        } else {
            self.arm(
                MobileMelkLightingTimerDto::Initialization,
                INITIALIZATION_DELAY_MS,
            );
        }
    }

    fn drain_writes(&mut self, can_send: bool) {
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

    fn handle(&mut self, event: MobileMelkLightingSessionEventDto) {
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
                            service: cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes().to_vec(),
                        });
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
                self.reconnect_attempt = 0;
                self.timer = None;
                self.transition(MobileMelkLightingSessionStateDto::Discovering);
                self.actions
                    .push_back(MobileMelkLightingSessionActionDto::DiscoverServices {
                        platform_identifier,
                        service: cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes().to_vec(),
                    });
            }
            MobileMelkLightingSessionEventDto::ConnectFailed { reason } => {
                self.timer = None;
                if self.reconnect_enabled {
                    self.schedule_reconnect(reason);
                } else {
                    self.transition(MobileMelkLightingSessionStateDto::Failed { reason });
                }
            }
            MobileMelkLightingSessionEventDto::ConnectTimeout => {
                if matches!(
                    self.timer,
                    Some(MobileMelkLightingTimerDto::ConnectionAttempt)
                ) {
                    self.timer = None;
                    if let Some(identifier) = self.selected_identifier.clone() {
                        self.actions
                            .push_back(MobileMelkLightingSessionActionDto::CancelConnect {
                                platform_identifier: identifier,
                            });
                    }
                    self.transition(MobileMelkLightingSessionStateDto::Scanning);
                    self.actions
                        .push_back(MobileMelkLightingSessionActionDto::Scan);
                    self.record("connect_timeout");
                }
            }
            MobileMelkLightingSessionEventDto::ServicesDiscovered {
                service_uuids,
                error,
            } => {
                if let Some(reason) = error {
                    self.reject_candidate(reason);
                } else if !service_uuids.iter().any(|uuid| {
                    uuid.as_slice() == cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes()
                }) {
                    self.record("gatt=missing FFF0 service");
                    self.reject_candidate("missing FFF0 service".into());
                } else if let Some(identifier) = self.selected_identifier.clone() {
                    self.actions.push_back(
                        MobileMelkLightingSessionActionDto::DiscoverCharacteristics {
                            platform_identifier: identifier,
                            service: cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes().to_vec(),
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
                } else if service_uuid != cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes() {
                    self.reject_candidate("missing FFF0 service".into());
                } else if !characteristics.iter().any(|characteristic| {
                    characteristic.uuid.as_slice()
                        == cutout_protocols::MELK_WRITE_CHANNEL.as_bytes()
                        && characteristic.write_without_response
                }) {
                    self.reject_candidate("missing FFF3 write characteristic".into());
                } else if !characteristics.iter().any(|characteristic| {
                    characteristic.uuid.as_slice()
                        == cutout_protocols::MELK_NOTIFY_CHANNEL.as_bytes()
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
                                    .as_bytes()
                                    .to_vec(),
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
                if characteristic.as_slice() != cutout_protocols::MELK_NOTIFY_CHANNEL.as_bytes() {
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
                if characteristic.as_slice() == cutout_protocols::MELK_NOTIFY_CHANNEL.as_bytes() {
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
                self.timer = None;
                match timer {
                    MobileMelkLightingTimerDto::ConnectionAttempt => {
                        self.handle(MobileMelkLightingSessionEventDto::ConnectTimeout);
                    }
                    MobileMelkLightingTimerDto::Reconnect => self.connect_selected(),
                    MobileMelkLightingTimerDto::Initialization => {
                        self.drain_initialization(can_send)
                    }
                    MobileMelkLightingTimerDto::WriteDrain => self.drain_writes(can_send),
                }
            }
            MobileMelkLightingSessionEventDto::Disconnected { reason, powered_on } => {
                self.notification_ready = false;
                self.initialization.clear();
                self.writes.clear();
                self.record(format!("disconnected error={reason}"));
                if self.reconnect_enabled && powered_on {
                    self.schedule_reconnect(reason);
                } else {
                    self.transition(MobileMelkLightingSessionStateDto::Disconnected);
                }
            }
        }
    }
}

fn is_coalescible_color_write(write: &MobileMelkLightingWriteDto) -> bool {
    write.confirmation_characteristic
        == MelkLightingProfile::write_policy()
            .confirmation_channel
            .as_bytes()
        && write.characteristic == MelkLightingProfile::write_policy().channel.as_bytes()
        && write.payload.len() == 9
        && write.payload[0] == 0x7e
        && write.payload[1] == 0
        && write.payload[2] == 5
        && write.payload[3] == 3
        && write.payload[8] == 0xef
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Rust-owned MELK session core exposed to the thin CoreBluetooth wrapper.
#[derive(Debug, uniffi::Object)]
pub struct MobileMelkLightingSessionCore {
    inner: std::sync::Mutex<SessionCore>,
}

#[uniffi::export]
impl MobileMelkLightingSessionCore {
    /// Creates an idle session core.
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            inner: std::sync::Mutex::new(SessionCore::default()),
        })
    }

    /// Starts the session with an optional remembered platform identifier.
    pub fn start(&self, preferred_platform_identifier: Option<String>) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.preferred_identifier = preferred_platform_identifier.clone();
        inner.invalid_preferred_identifier = preferred_platform_identifier
            .as_deref()
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_err());
        inner.reconnect_enabled = true;
        inner.reconnect_attempt = 0;
        inner.candidates.clear();
        inner.selected_identifier = None;
        inner.selected_name = None;
        inner.transition(MobileMelkLightingSessionStateDto::Idle);
    }

    /// Stops the session and prevents future reconnects.
    pub fn stop(&self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.reconnect_enabled = false;
        inner.timer = None;
        if inner.command_status == 1 {
            inner.command_status = 3;
        }
        inner.transition(MobileMelkLightingSessionStateDto::Disconnected);
    }

    /// Submits one CoreBluetooth event.
    pub fn handle(&self, event: MobileMelkLightingSessionEventDto) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .handle(event);
    }

    /// Selects a first-pairing candidate.
    pub fn select_candidate(&self, platform_identifier: String) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .select(platform_identifier);
    }

    /// Enqueues a power command if the verified profile is ready.
    pub fn set_power(&self, on: bool) -> bool {
        self.command(mobile_melk_transport_action(
            MelkLightingProfile::write_action(RgbLightingCommand::SetPower(if on {
                LightingPowerState::On
            } else {
                LightingPowerState::Off
            })),
        ))
    }

    /// Enqueues a solid-color command if the verified profile is ready.
    pub fn set_solid_color(&self, red: u8, green: u8, blue: u8) -> bool {
        self.command(mobile_melk_transport_action(
            MelkLightingProfile::write_action(RgbLightingCommand::SetSolidColor(RgbColor::new(
                red, green, blue,
            ))),
        ))
    }

    /// Enqueues a brightness command if the value is valid and the profile is ready.
    pub fn set_brightness(&self, percentage: u8) -> Result<bool, MobileMelkLightingError> {
        let brightness = LightingBrightness::try_from_percent(percentage)
            .map_err(|_| MobileMelkLightingError::InvalidBrightness)?;
        let write = mobile_melk_transport_action(MelkLightingProfile::write_action(
            RgbLightingCommand::SetBrightness(brightness),
        ));
        Ok(self.command(write))
    }

    /// Enqueues an effect-speed command.
    pub fn set_effect_speed(&self, speed: u8) -> bool {
        self.command(super::mobile_melk_control(MelkControl::Speed(speed)))
    }

    /// Enqueues a complete restore state.
    pub fn apply_state(
        &self,
        state: super::MobileMelkLightingRestoreStateDto,
    ) -> Result<bool, MobileMelkLightingError> {
        let state = state
            .try_into()
            .map_err(|_| MobileMelkLightingError::InvalidPlayback)?;
        let writes = MelkLightingProfile::plan_state(state)
            .into_iter()
            .map(mobile_melk_transport_action)
            .collect::<Vec<_>>();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !matches!(inner.state, MobileMelkLightingSessionStateDto::Ready)
            || writes
                .iter()
                .any(|write| write.mode != MobileMelkLightingWriteModeDto::WithoutResponse)
        {
            return Ok(false);
        }
        Ok(inner.queue_writes(writes))
    }

    /// Enqueues a controller-local schedule after synchronizing its local clock.
    pub fn set_schedule(
        &self,
        schedule: super::MobileMelkScheduleDto,
        clock: super::MobileMelkClockDto,
    ) -> Result<bool, MobileMelkLightingError> {
        let schedule = MelkSchedule::new(
            if schedule.power_on {
                LightingPowerState::On
            } else {
                LightingPowerState::Off
            },
            schedule.hour,
            schedule.minute,
            schedule.days,
            schedule.enabled,
        )
        .map_err(|_| MobileMelkLightingError::InvalidSchedule)?;
        let clock = MelkClock::new(clock.hour, clock.minute, clock.second, clock.weekday)
            .map_err(|_| MobileMelkLightingError::InvalidClock)?;
        if !MelkLightingProfile::capabilities().schedules {
            return Err(MobileMelkLightingError::UnsupportedCapability);
        }
        let writes = [
            mobile_melk_transport_action(MelkLightingProfile::control_action(MelkControl::Clock(
                clock,
            ))),
            mobile_melk_transport_action(MelkLightingProfile::control_action(
                MelkControl::Schedule(schedule),
            )),
        ];
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !matches!(inner.state, MobileMelkLightingSessionStateDto::Ready) {
            return Ok(false);
        }
        Ok(inner.queue_writes(writes))
    }

    /// Marks the latest requested command as physically confirmed.
    pub fn mark_last_command_confirmed(&self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.command_status == 1 {
            inner.command_status = 2;
        }
    }

    /// Marks the latest requested command as explicitly unconfirmed.
    pub fn mark_last_command_unconfirmed(&self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.command_status == 1 {
            inner.command_status = 3;
        }
    }

    /// Returns the current Rust-owned snapshot.
    pub fn snapshot(&self) -> MobileMelkLightingSessionSnapshotDto {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot()
    }

    /// Drains requested platform operations.
    pub fn drain_actions(&self) -> Vec<MobileMelkLightingSessionActionDto> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .actions
            .drain(..)
            .collect()
    }

    /// Drains bounded diagnostic records.
    pub fn drain_records(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .records
            .drain(..)
            .collect()
    }

    /// Drains first-pairing candidates.
    pub fn drain_candidates(&self) -> Vec<MobileMelkLightingSessionCandidateDto> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .candidates_out
            .drain(..)
            .collect()
    }

    /// Drains raw FFF4 notifications for the app-level observer.
    pub fn drain_notifications(&self) -> Vec<Vec<u8>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .notifications
            .drain(..)
            .collect()
    }

    /// Drains one pending user write when CoreBluetooth reports available capacity.
    pub fn flush_writes(&self, can_send: bool) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain_writes(can_send);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-1111-1111-1111-111111111111";

    fn ready_core() -> std::sync::Arc<MobileMelkLightingSessionCore> {
        let core = MobileMelkLightingSessionCore::new();
        core.start(Some(ID.into()));
        core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
            powered_on: true,
            state_code: 5,
        });
        core.handle(MobileMelkLightingSessionEventDto::Discovered {
            name: Some("MELK-OC21  6A".into()),
            platform_identifier: ID.into(),
            rssi: -60,
        });
        core.drain_actions();
        core.handle(MobileMelkLightingSessionEventDto::Connected {
            name: Some("MELK-OC21  6A".into()),
            platform_identifier: ID.into(),
        });
        core.drain_actions();
        core.handle(MobileMelkLightingSessionEventDto::ServicesDiscovered {
            service_uuids: vec![cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes().to_vec()],
            error: None,
        });
        core.drain_actions();
        core.handle(
            MobileMelkLightingSessionEventDto::CharacteristicsDiscovered {
                name: Some("MELK-OC21  6A".into()),
                service_uuid: cutout_protocols::MELK_SERVICE_CHANNEL.as_bytes().to_vec(),
                characteristics: vec![
                    MobileMelkLightingCharacteristicEvidenceDto {
                        uuid: cutout_protocols::MELK_WRITE_CHANNEL.as_bytes().to_vec(),
                        write_without_response: true,
                        notify_or_indicate: false,
                    },
                    MobileMelkLightingCharacteristicEvidenceDto {
                        uuid: cutout_protocols::MELK_NOTIFY_CHANNEL.as_bytes().to_vec(),
                        write_without_response: false,
                        notify_or_indicate: true,
                    },
                ],
                error: None,
            },
        );
        core.drain_actions();
        core.handle(MobileMelkLightingSessionEventDto::NotificationState {
            characteristic: cutout_protocols::MELK_NOTIFY_CHANNEL.as_bytes().to_vec(),
            ready: true,
            can_send: true,
            error: None,
        });
        core.drain_actions();
        core.handle(MobileMelkLightingSessionEventDto::TimerFired {
            timer: MobileMelkLightingTimerDto::Initialization,
            can_send: true,
        });
        core.drain_actions();
        assert_eq!(
            core.snapshot().state,
            MobileMelkLightingSessionStateDto::Ready
        );
        core
    }

    #[test]
    fn reducer_owns_gatt_initialization_and_ready_gate() {
        let core = ready_core();
        let snapshot = core.snapshot();
        assert_eq!(snapshot.platform_identifier.as_deref(), Some(ID));
        assert!(snapshot.notification_ready);
    }

    #[test]
    fn reducer_coalesces_color_preview_writes_and_waits_for_capacity() {
        let core = ready_core();
        assert!(core.set_solid_color(255, 0, 0));
        assert!(core.set_solid_color(0, 255, 0));
        assert!(core.drain_actions().is_empty());
        core.flush_writes(true);
        let actions = core.drain_actions();
        assert_eq!(actions.len(), 2);
        let MobileMelkLightingSessionActionDto::Write { write, .. } = &actions[0] else {
            panic!("expected a color write")
        };
        assert_eq!(write.payload, [0x7e, 0, 5, 3, 0, 255, 0, 0, 0xef]);
        assert!(matches!(
            actions[1],
            MobileMelkLightingSessionActionDto::ArmTimer {
                timer: MobileMelkLightingTimerDto::WriteDrain,
                ..
            }
        ));
    }

    #[test]
    fn reducer_rejects_malformed_remembered_identity_before_scanning() {
        let core = MobileMelkLightingSessionCore::new();
        core.start(Some("not-a-uuid".into()));
        core.handle(MobileMelkLightingSessionEventDto::BluetoothState {
            powered_on: true,
            state_code: 5,
        });
        assert_eq!(
            core.snapshot().state,
            MobileMelkLightingSessionStateDto::Failed {
                reason: "Remembered lighting identity is invalid".into()
            }
        );
        assert!(core.drain_actions().is_empty());
    }
}

impl MobileMelkLightingSessionCore {
    fn command(&self, write: MobileMelkLightingWriteDto) -> bool {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !matches!(inner.state, MobileMelkLightingSessionStateDto::Ready) {
            return false;
        }
        inner.queue_write(write)
    }
}
