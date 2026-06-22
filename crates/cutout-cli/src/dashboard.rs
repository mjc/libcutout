use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use cutout_btle::{ConnectionSummary, ConnectionTarget, SessionBridgeReport};
use cutout_core::TelemetrySnapshot;
use ratatui::termina::{PlatformTerminal, Terminal as _};
use ratatui::{
    Frame, Terminal,
    backend::TerminaBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Axis, Block, Cell, Chart, Clear, Dataset, Gauge, Paragraph, Row, Sparkline, Table, Tabs,
    },
};

const LOG_LIMIT: usize = 10;
const HISTORY_LIMIT: usize = 32;
const TAB_COUNT: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DashboardState {
    pub(crate) source: DashboardSource,
    pub(crate) active_tab: usize,
    pub(crate) provenance: Option<&'static str>,
    pub(crate) device: DeviceSnapshot,
    pub(crate) scan_browser: ScanBrowser,
    pub(crate) telemetry: TelemetryWindow,
    pub(crate) profiles: Vec<ProfileSnapshot>,
    pub(crate) counters: SessionCounters,
    pub(crate) logs: VecDeque<LogEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DashboardSource {
    Demo,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceSnapshot {
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) identifier: String,
    pub(crate) firmware: String,
    pub(crate) connection_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileSnapshot {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) status: String,
    pub(crate) family: ProfileFamily,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScanBrowser {
    pub(crate) filters: TargetFilterSummary,
    pub(crate) observations: Vec<ScanObservation>,
    pub(crate) selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TargetFilterSummary {
    pub(crate) address: Option<String>,
    pub(crate) identifier: Option<String>,
    pub(crate) name_contains: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScanObservation {
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) identifier: String,
    pub(crate) rssi: String,
    pub(crate) services: String,
    pub(crate) real_device: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileFamily {
    AeroVeteran {
        current_limit_a: Option<String>,
        tail_status: String,
        summary: String,
    },
    Pending {
        family: String,
        note: String,
        summary: String,
    },
}

impl ProfileFamily {
    fn summary(&self) -> &str {
        match self {
            Self::AeroVeteran { summary, .. } | Self::Pending { summary, .. } => summary.as_str(),
        }
    }
}

impl From<FixtureProfileFamily> for ProfileFamily {
    fn from(value: FixtureProfileFamily) -> Self {
        match value {
            FixtureProfileFamily::AeroVeteran {
                current_limit_a,
                tail_status,
            } => Self::AeroVeteran {
                current_limit_a: current_limit_a.map(ToOwned::to_owned),
                tail_status: tail_status.to_owned(),
                summary: match current_limit_a {
                    Some(current_limit) => {
                        format!("Aero/Veteran current {current_limit} / {tail_status}")
                    }
                    None => format!("unknown Aero/Veteran current unknown / {tail_status}"),
                },
            },
            FixtureProfileFamily::Pending { family, note } => Self::Pending {
                family: family.to_owned(),
                note: note.to_owned(),
                summary: format!("pending {family} {note}"),
            },
        }
    }
}

impl ScanBrowser {
    fn empty() -> Self {
        Self {
            filters: TargetFilterSummary::default(),
            observations: Vec::new(),
            selected: 0,
        }
    }

    fn selected(&self) -> Option<&ScanObservation> {
        self.observations.get(self.selected)
    }

    fn push_observation(&mut self, observation: ScanObservation, selected: bool) {
        if self.observations.len() == HISTORY_LIMIT {
            self.observations.remove(0);
            if self.selected > 0 {
                self.selected -= 1;
            }
        }

        self.observations.push(observation);
        if selected {
            self.selected = self.observations.len().saturating_sub(1);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionCounters {
    pub(crate) discovered: u64,
    pub(crate) connected: u64,
    pub(crate) subscriptions: u64,
    pub(crate) notifications: u64,
    pub(crate) notification_bytes: u64,
    pub(crate) latest_notification_len: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogEntry {
    pub(crate) level: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TelemetryWindow {
    pub(crate) battery_pct: Option<u64>,
    pub(crate) battery_source: BatterySource,
    pub(crate) signal_pct: u64,
    pub(crate) latest_voltage_v: Option<u64>,
    pub(crate) speed_mph: Vec<u64>,
    pub(crate) voltage_v: Vec<u64>,
    pub(crate) current_a: Vec<u64>,
    pub(crate) temperature_c: Vec<u64>,
    pub(crate) current_points: Vec<(f64, f64)>,
    pub(crate) temperature_points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BatterySource {
    #[default]
    Unknown,
    StandardBle,
    TelemetryReported,
    TelemetryEstimated,
}

impl BatterySource {
    const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Battery",
            Self::StandardBle => "Battery BLE",
            Self::TelemetryReported => "Battery telemetry",
            Self::TelemetryEstimated => "Battery estimated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardInput {
    Quit,
    NextTab,
    PreviousTab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixtureEvent {
    Provenance {
        source: &'static str,
    },
    Device {
        name: &'static str,
        address: &'static str,
        identifier: &'static str,
        firmware: &'static str,
        connection_state: &'static str,
    },
    ScanFilters {
        address: Option<&'static str>,
        identifier: Option<&'static str>,
        name_contains: Option<&'static str>,
    },
    ScanObservation {
        name: &'static str,
        address: &'static str,
        identifier: &'static str,
        rssi: &'static str,
        services: &'static str,
        real_device: bool,
        selected: bool,
    },
    Counters {
        discovered: u64,
        connected: u64,
        subscriptions: u64,
        notifications: u64,
    },
    Telemetry {
        battery_pct: u64,
        signal_pct: u64,
        speed_mph: &'static [u64],
        voltage_v: &'static [u64],
        current_a: &'static [u64],
        temperature_c: &'static [u64],
    },
    Profile {
        name: &'static str,
        source: &'static str,
        status: &'static str,
        family: FixtureProfileFamily,
    },
    Log {
        level: &'static str,
        message: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FixtureReplay {
    steps: &'static [FixtureEvent],
    index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixtureProfileFamily {
    AeroVeteran {
        current_limit_a: Option<&'static str>,
        tail_status: &'static str,
    },
    Pending {
        family: &'static str,
        note: &'static str,
    },
}

const FIXTURE_DEMO_STEPS: &[FixtureEvent] = &[
    FixtureEvent::Provenance {
        source: "fixture/demo replay: aero-nf2557.v1",
    },
    FixtureEvent::Device {
        name: "Aero NF2557",
        address: "AA:BB:CC:DD:EE:FF",
        identifier: "platform-0001",
        firmware: "v3.8.12",
        connection_state: "connected",
    },
    FixtureEvent::ScanFilters {
        address: Some("AA:BB:CC:DD:EE:FF"),
        identifier: Some("platform-0001"),
        name_contains: Some("Aero"),
    },
    FixtureEvent::ScanObservation {
        name: "Aero NF2557",
        address: "AA:BB:CC:DD:EE:FF",
        identifier: "platform-0001",
        rssi: "-61 dBm",
        services: "battery, throttle, telemetry",
        real_device: true,
        selected: true,
    },
    FixtureEvent::ScanObservation {
        name: "Begode X",
        address: "11:22:33:44:55:66",
        identifier: "platform-0202",
        rssi: "-77 dBm",
        services: "diagnostics, state",
        real_device: true,
        selected: false,
    },
    FixtureEvent::ScanObservation {
        name: "Veteran V14",
        address: "AA:00:11:22:33:44",
        identifier: "platform-0303",
        rssi: "-84 dBm",
        services: "battery, control",
        real_device: true,
        selected: false,
    },
    FixtureEvent::Counters {
        discovered: 8,
        connected: 1,
        subscriptions: 4,
        notifications: 27,
    },
    FixtureEvent::Telemetry {
        battery_pct: 74,
        signal_pct: 81,
        speed_mph: &[0, 4, 9, 14, 18, 22, 24, 21, 19, 17, 16, 18],
        voltage_v: &[52, 52, 53, 53, 54, 54, 55, 55, 55, 54, 54, 53],
        current_a: &[3, 4, 6, 7, 8, 9, 10, 10, 9, 8, 7, 6],
        temperature_c: &[30, 31, 31, 32, 32, 33, 34, 34, 35, 35, 34, 33],
    },
    FixtureEvent::Profile {
        name: "Primary drive",
        source: "probe",
        status: "ready",
        family: FixtureProfileFamily::AeroVeteran {
            current_limit_a: Some("45A"),
            tail_status: "raw tail preserved",
        },
    },
    FixtureEvent::Profile {
        name: "Battery pack",
        source: "capture",
        status: "warming",
        family: FixtureProfileFamily::Pending {
            family: "Begode/Falcon",
            note: "unsupported / pending",
        },
    },
    FixtureEvent::Profile {
        name: "Motor controller",
        source: "manual",
        status: "partial",
        family: FixtureProfileFamily::AeroVeteran {
            current_limit_a: None,
            tail_status: "raw tail unknown",
        },
    },
    FixtureEvent::Log {
        level: "info",
        message: "fixture replay loaded from fixture/demo replay: aero-nf2557.v1",
    },
    FixtureEvent::Log {
        level: "debug",
        message: "dashboard booted in read-only mode",
    },
];

impl FixtureReplay {
    pub(crate) const fn demo() -> Self {
        Self {
            steps: FIXTURE_DEMO_STEPS,
            index: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn provenance(self) -> Option<&'static str> {
        self.steps.iter().find_map(|step| match step {
            FixtureEvent::Provenance { source } => Some(*source),
            _ => None,
        })
    }

    pub(crate) fn apply_next(&mut self, state: &mut DashboardState) -> bool {
        let Some(step) = self.steps.get(self.index).copied() else {
            return false;
        };
        self.index += 1;
        state.apply_fixture_event(step);
        true
    }

    pub(crate) fn apply_all(&mut self, state: &mut DashboardState) {
        while self.apply_next(state) {}
    }
}

impl DashboardState {
    pub(crate) fn empty() -> Self {
        Self {
            source: DashboardSource::Live,
            active_tab: 0,
            provenance: None,
            device: DeviceSnapshot {
                name: "unknown".to_owned(),
                address: "unknown".to_owned(),
                identifier: "unknown".to_owned(),
                firmware: "unknown".to_owned(),
                connection_state: "scanning".to_owned(),
            },
            scan_browser: ScanBrowser::empty(),
            telemetry: TelemetryWindow::empty(),
            profiles: Vec::new(),
            counters: SessionCounters::default(),
            logs: VecDeque::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn sample() -> Self {
        let mut state = Self::empty();
        state.source = DashboardSource::Demo;
        let mut replay = FixtureReplay::demo();
        replay.apply_all(&mut state);
        state
    }

    pub(crate) fn demo(device: Option<&str>) -> Self {
        let mut state = Self::empty();
        state.source = DashboardSource::Demo;
        let mut replay = FixtureReplay::demo();
        replay.apply_all(&mut state);

        if let Some(device) = device {
            if state.device.name != device {
                state
                    .scan_browser
                    .observations
                    .retain(|observation| observation.real_device && observation.name == device);
                state.scan_browser.selected = 0;
            }
        }

        state
    }

    #[cfg(test)]
    pub(crate) fn live_target(device: String) -> Self {
        let mut state = Self::empty();
        state.device.name.clone_from(&device);
        "target selected".clone_into(&mut state.device.connection_state);
        state.scan_browser.filters.name_contains = Some(device);
        state
    }

    pub(crate) fn live_connected(target: &ConnectionTarget, summary: &ConnectionSummary) -> Self {
        let mut state = Self::empty();
        state.scan_browser.filters = TargetFilterSummary {
            address: target.address.clone(),
            identifier: target.identifier.clone(),
            name_contains: target.name_contains.clone(),
        };

        let observation = &summary.observation;
        state.device = DeviceSnapshot {
            name: observation
                .name
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
            address: observation
                .address
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
            identifier: observation.identifier.clone(),
            firmware: "unknown".to_owned(),
            connection_state: "connected".to_owned(),
        };
        state.counters.discovered = 1;
        state.counters.connected = 1;
        state.scan_browser.push_observation(
            ScanObservation {
                name: state.device.name.clone(),
                address: state.device.address.clone(),
                identifier: state.device.identifier.clone(),
                rssi: observation
                    .rssi
                    .map_or_else(|| "unknown".to_owned(), |rssi| format!("{rssi} dBm")),
                services: services_summary(summary),
                real_device: true,
            },
            true,
        );
        state.profiles.push(ProfileSnapshot {
            name: "Veteran data".to_owned(),
            source: "gatt".to_owned(),
            status: "connected".to_owned(),
            family: ProfileFamily::AeroVeteran {
                current_limit_a: None,
                tail_status: "live channel observed".to_owned(),
                summary: "Aero/Veteran live notification channel observed".to_owned(),
            },
        });
        state.push_log("info", "connected dashboard target");
        state.push_log("info", "waiting for telemetry decoder data");
        state
    }

    pub(crate) fn apply_session_report(&mut self, report: &SessionBridgeReport) {
        self.counters.subscriptions = usize_to_u64(report.subscribes);
        self.counters.notifications = usize_to_u64(report.notifications);
        self.counters.notification_bytes = usize_to_u64(report.notification_bytes);
        self.counters.latest_notification_len = report.latest_notification_len.map(usize_to_u64);
        self.push_log(
            "info",
            &format!(
                "session probe writes={} subscribes={} notifications={} bytes={}",
                report.writes, report.subscribes, report.notifications, report.notification_bytes
            ),
        );

        if report.telemetry == 0 {
            self.push_log(
                "warn",
                "notifications received but telemetry decoder produced no samples",
            );
        } else {
            self.push_log("info", &format!("telemetry samples={}", report.telemetry));
        }

        if report.diagnostics > 0 {
            self.push_log("warn", &format!("diagnostics={}", report.diagnostics));
        }

        if report.telemetry > 0 {
            let snapshot = report.telemetry_snapshot;
            if let Some(voltage) = snapshot.voltage_mv {
                let volts = millivolts_to_volts(voltage.value);
                self.push_log("info", &format!("telemetry voltage {volts} V"));
            }
            if let Some(percent) = snapshot
                .battery_percent_reported
                .or(snapshot.battery_percent_estimated)
            {
                self.push_log("info", &format!("telemetry battery {}%", percent.value));
            }
            self.telemetry.apply_snapshot(snapshot);
        }
    }

    pub(crate) fn apply_battery_percent(&mut self, percent: u8) {
        let percent = percent.min(100);
        self.telemetry.battery_pct = Some(u64::from(percent));
        self.telemetry.battery_source = BatterySource::StandardBle;
        self.push_log("info", &format!("battery level {percent}%"));
    }

    pub(crate) fn apply_fixture_event(&mut self, event: FixtureEvent) {
        match event {
            FixtureEvent::Provenance { source } => {
                self.provenance = Some(source);
            }
            FixtureEvent::Device {
                name,
                address,
                identifier,
                firmware,
                connection_state,
            } => {
                self.device = DeviceSnapshot {
                    name: name.to_owned(),
                    address: address.to_owned(),
                    identifier: identifier.to_owned(),
                    firmware: firmware.to_owned(),
                    connection_state: connection_state.to_owned(),
                };
            }
            FixtureEvent::ScanFilters {
                address,
                identifier,
                name_contains,
            } => {
                self.scan_browser.filters = TargetFilterSummary {
                    address: address.map(ToOwned::to_owned),
                    identifier: identifier.map(ToOwned::to_owned),
                    name_contains: name_contains.map(ToOwned::to_owned),
                };
            }
            FixtureEvent::ScanObservation {
                name,
                address,
                identifier,
                rssi,
                services,
                real_device,
                selected,
            } => {
                let observation = ScanObservation {
                    name: name.to_owned(),
                    address: address.to_owned(),
                    identifier: identifier.to_owned(),
                    rssi: rssi.to_owned(),
                    services: services.to_owned(),
                    real_device,
                };
                self.scan_browser.push_observation(observation, selected);
            }
            FixtureEvent::Counters {
                discovered,
                connected,
                subscriptions,
                notifications,
            } => {
                self.counters = SessionCounters {
                    discovered,
                    connected,
                    subscriptions,
                    notifications,
                    notification_bytes: 0,
                    latest_notification_len: None,
                };
            }
            FixtureEvent::Telemetry {
                battery_pct,
                signal_pct,
                speed_mph,
                voltage_v,
                current_a,
                temperature_c,
            } => {
                self.telemetry.load_window(
                    battery_pct,
                    signal_pct,
                    speed_mph,
                    voltage_v,
                    current_a,
                    temperature_c,
                );
            }
            FixtureEvent::Profile {
                name,
                source,
                status,
                family,
            } => {
                self.profiles.push(ProfileSnapshot {
                    name: name.to_owned(),
                    source: source.to_owned(),
                    status: status.to_owned(),
                    family: family.into(),
                });
            }
            FixtureEvent::Log { level, message } => {
                self.push_log(level, message);
            }
        }
    }

    pub(crate) fn advance(&mut self) {
        if self.source == DashboardSource::Live {
            return;
        }

        let next_notification = self.counters.notifications.saturating_add(1);
        self.counters.notifications = next_notification;
        self.telemetry.step();
        self.push_log("trace", "fixture heartbeat advanced");
    }

    fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % TAB_COUNT;
    }

    fn previous_tab(&mut self) {
        self.active_tab = if self.active_tab == 0 {
            TAB_COUNT - 1
        } else {
            self.active_tab - 1
        };
    }

    fn push_log(&mut self, level: &str, message: &str) {
        if self.logs.len() == LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(LogEntry {
            level: level.to_owned(),
            message: message.to_owned(),
        });
    }
}

impl TelemetryWindow {
    fn empty() -> Self {
        Self {
            battery_pct: None,
            battery_source: BatterySource::Unknown,
            signal_pct: 0,
            latest_voltage_v: None,
            speed_mph: Vec::with_capacity(HISTORY_LIMIT),
            voltage_v: Vec::with_capacity(HISTORY_LIMIT),
            current_a: Vec::with_capacity(HISTORY_LIMIT),
            temperature_c: Vec::with_capacity(HISTORY_LIMIT),
            current_points: Vec::with_capacity(HISTORY_LIMIT),
            temperature_points: Vec::with_capacity(HISTORY_LIMIT),
        }
    }

    fn load_window(
        &mut self,
        battery_pct: u64,
        signal_pct: u64,
        speed_mph: &'static [u64],
        voltage_v: &'static [u64],
        current_a: &'static [u64],
        temperature_c: &'static [u64],
    ) {
        self.battery_pct = Some(battery_pct);
        self.battery_source = BatterySource::TelemetryReported;
        self.signal_pct = signal_pct;
        self.latest_voltage_v = voltage_v.last().copied();
        self.speed_mph.clear();
        self.voltage_v.clear();
        self.current_a.clear();
        self.temperature_c.clear();
        self.speed_mph.extend_from_slice(speed_mph);
        self.voltage_v.extend_from_slice(voltage_v);
        self.current_a.extend_from_slice(current_a);
        self.temperature_c.extend_from_slice(temperature_c);
        self.sync_points();
    }

    fn step(&mut self) {
        let next_speed = (self.speed_mph.last().copied().unwrap_or(0) + 3) % 40;
        let next_voltage = 50 + ((self.voltage_v.last().copied().unwrap_or(52) + 1) % 6);
        let next_current = 4 + ((self.current_a.last().copied().unwrap_or(5) + 1) % 9);
        let next_temperature = 30 + ((self.temperature_c.last().copied().unwrap_or(32) + 1) % 9);

        if let Some(battery_pct) = self.battery_pct.as_mut() {
            *battery_pct = battery_pct.saturating_sub(1).max(10);
        }
        self.signal_pct = (self.signal_pct + 1).min(100);
        push_sample(&mut self.speed_mph, next_speed);
        push_sample(&mut self.voltage_v, next_voltage);
        push_sample(&mut self.current_a, next_current);
        push_sample(&mut self.temperature_c, next_temperature);
        self.sync_points();
    }

    fn sync_points(&mut self) {
        self.current_points.clear();
        self.temperature_points.clear();
        self.current_points.reserve(HISTORY_LIMIT);
        self.temperature_points.reserve(HISTORY_LIMIT);

        for (index, value) in self.current_a.iter().enumerate() {
            self.current_points
                .push((index_to_f64(index), to_f64(*value)));
        }

        for (index, value) in self.temperature_c.iter().enumerate() {
            self.temperature_points
                .push((index_to_f64(index), to_f64(*value)));
        }
    }

    fn apply_snapshot(&mut self, snapshot: TelemetrySnapshot) {
        if let Some(percent) = snapshot.battery_percent_reported {
            self.battery_pct = Some(u64::from(percent.value));
            self.battery_source = BatterySource::TelemetryReported;
        } else if let Some(percent) = snapshot.battery_percent_estimated {
            self.battery_pct = Some(u64::from(percent.value));
            self.battery_source = BatterySource::TelemetryEstimated;
        }
        if let Some(speed) = snapshot.speed_mm_s {
            push_sample(&mut self.speed_mph, mm_s_to_mph(speed.value));
        }
        if let Some(voltage) = snapshot.voltage_mv {
            let volts = millivolts_to_volts(voltage.value);
            self.latest_voltage_v = Some(volts);
            push_sample(&mut self.voltage_v, volts);
        }
        if let Some(current) = snapshot.battery_current_ma.or(snapshot.motor_current_ma) {
            push_sample(&mut self.current_a, milliamps_to_amps_abs(current.value));
        }
        if let Some(temperature) = snapshot
            .controller_temperature_mc
            .or(snapshot.motor_temperature_mc)
            .or(snapshot.battery_temperature_mc)
        {
            push_sample(
                &mut self.temperature_c,
                millicelsius_to_celsius(temperature.value),
            );
        }
        self.sync_points();
    }

    fn has_decoded_samples(&self) -> bool {
        !(self.speed_mph.is_empty()
            && self.voltage_v.is_empty()
            && self.current_a.is_empty()
            && self.temperature_c.is_empty())
    }
}

fn push_sample(series: &mut Vec<u64>, value: u64) {
    if series.len() == HISTORY_LIMIT {
        series.remove(0);
    }
    series.push(value);
}

fn mm_s_to_mph(value: i32) -> u64 {
    let numerator = u64::from(value.unsigned_abs()) * 1_000;
    numerator.saturating_add(223_694) / 447_388
}

fn millivolts_to_volts(value: i32) -> u64 {
    u64::from(value.unsigned_abs()).saturating_add(500) / 1_000
}

fn milliamps_to_amps_abs(value: i32) -> u64 {
    u64::from(value.unsigned_abs()).saturating_add(500) / 1_000
}

fn millicelsius_to_celsius(value: i32) -> u64 {
    u64::from(value.unsigned_abs()).saturating_add(500) / 1_000
}

fn services_summary(summary: &ConnectionSummary) -> String {
    if summary.services.is_empty() {
        return "none discovered".to_owned();
    }

    summary
        .services
        .iter()
        .map(|service| format!("{}:{} chars", service.uuid, service.characteristics.len()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn run_dashboard(mut state: DashboardState) -> Result<()> {
    let (tx, rx) = mpsc::channel::<DashboardInput>();
    let _input_thread = spawn_input_thread(tx);

    let mut stdout = io::stdout().lock();
    stdout.write_all(b"\x1b[2J\x1b[H")?;
    stdout.flush()?;

    let mut output = PlatformTerminal::new()?;
    output.enter_raw_mode()?;
    let backend = TerminaBackend::new(output);
    let mut terminal = Terminal::new(backend)?;
    let mut last_tick = Instant::now();

    'dashboard: loop {
        terminal.draw(|frame| {
            frame.render_widget(Clear, frame.area());
            render_dashboard(frame, &state);
        })?;

        while let Ok(input) = rx.try_recv() {
            match input {
                DashboardInput::Quit => break 'dashboard,
                DashboardInput::NextTab => state.next_tab(),
                DashboardInput::PreviousTab => state.previous_tab(),
            }
        }

        if last_tick.elapsed() >= Duration::from_millis(250) {
            state.advance();
            last_tick = Instant::now();
        } else {
            thread::sleep(Duration::from_millis(25));
        }
    }

    Ok(())
}

fn spawn_input_thread(tx: mpsc::Sender<DashboardInput>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut locked = stdin.lock();
        let mut byte = [0_u8; 1];

        while locked.read_exact(&mut byte).is_ok() {
            match byte[0] {
                b'q' | b'Q' => {
                    let _ = tx.send(DashboardInput::Quit);
                    break;
                }
                b'\t' => {
                    let _ = tx.send(DashboardInput::NextTab);
                }
                0x1b => handle_escape_sequence(&mut locked, &tx),
                _ => {}
            }
        }
    })
}

fn handle_escape_sequence<R: Read>(input: &mut R, tx: &mpsc::Sender<DashboardInput>) {
    let mut sequence = [0_u8; 2];
    if input.read_exact(&mut sequence).is_err() {
        return;
    }

    match sequence {
        [b'[', b'C'] => {
            let _ = tx.send(DashboardInput::NextTab);
        }
        [b'[', b'D'] => {
            let _ = tx.send(DashboardInput::PreviousTab);
        }
        _ => {}
    }
}

pub(crate) fn render_dashboard(frame: &mut Frame<'_>, state: &DashboardState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(17),
            Constraint::Fill(1),
        ])
        .split(frame.area());

    render_header(frame, areas[0], state);
    render_body(frame, areas[1], state);
    render_logs(frame, areas[2], state);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let tabs = Tabs::new([
        Line::from("Overview"),
        Line::from("Telemetry"),
        Line::from("Profiles"),
        Line::from("Logs"),
    ])
    .select(state.active_tab.min(3))
    .block(Block::bordered().title("Cutout dashboard"))
    .highlight_style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    frame.render_widget(tabs, area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    match state.active_tab.min(TAB_COUNT - 1) {
        0 => render_overview_tab(frame, area, state),
        1 => render_telemetry(frame, area, state),
        2 => render_profiles(frame, area, state),
        3 => render_logs(frame, area, state),
        _ => unreachable!("active tab is clamped to known dashboard tabs"),
    }
}

fn render_overview_tab(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let rows = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    render_overview(frame, rows[0], state);
    render_telemetry(frame, rows[1], state);
}

fn render_overview(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .split(area);

    let device = &state.device;
    let summary = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("device ", Style::new().fg(Color::Gray)),
            Span::raw(device.name.as_str()),
        ]),
        Line::from(vec![
            Span::styled("address ", Style::new().fg(Color::Gray)),
            Span::raw(device.address.as_str()),
        ]),
        Line::from(vec![
            Span::styled("firmware/state ", Style::new().fg(Color::Gray)),
            Span::raw(device.firmware.as_str()),
            Span::styled(" / ", Style::new().fg(Color::Gray)),
            Span::raw(device.connection_state.as_str()),
        ]),
        Line::from(vec![
            Span::styled("source ", Style::new().fg(Color::Gray)),
            Span::raw(state.provenance.unwrap_or("live")),
        ]),
    ])
    .block(panel_block("Target"));
    frame.render_widget(summary, chunks[0]);

    let gauges = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    render_battery_gauge(
        frame,
        gauges[0],
        state.telemetry.battery_pct,
        state.telemetry.battery_source,
        state.telemetry.latest_voltage_v,
    );

    let signal = Gauge::default()
        .block(Block::bordered().title("Signal"))
        .gauge_style(Style::new().fg(Color::Cyan).bg(Color::Black))
        .ratio(percent_ratio(state.telemetry.signal_pct));
    frame.render_widget(signal, gauges[1]);
    render_profiles(frame, chunks[2], state);
}

fn render_profiles(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let rows = state
        .profiles
        .iter()
        .map(|profile| {
            Row::new(vec![
                Cell::from(profile.name.as_str()),
                Cell::from(profile.source.as_str()),
                Cell::from(profile.status.as_str()),
                Cell::from(profile.family.summary()),
            ])
        })
        .collect::<Vec<_>>();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(10),
            Constraint::Percentage(50),
        ],
    )
    .header(Row::new(vec!["Profile", "Source", "Status", "Family"]).style(Style::new().bold()))
    .block(panel_block("Profiles"))
    .column_spacing(1);
    frame.render_widget(table, area);
}

fn render_battery_gauge(
    frame: &mut Frame<'_>,
    area: Rect,
    battery_pct: Option<u64>,
    source: BatterySource,
    latest_voltage_v: Option<u64>,
) {
    if let Some(battery_pct) = battery_pct {
        let battery = Gauge::default()
            .block(Block::bordered().title(source.label()))
            .gauge_style(Style::new().fg(Color::Green).bg(Color::Black))
            .ratio(percent_ratio(battery_pct));
        frame.render_widget(battery, area);
    } else {
        let message = latest_voltage_v.map_or_else(
            || "unknown".to_owned(),
            |voltage| format!("voltage {voltage} V / battery unknown"),
        );
        let battery = Paragraph::new(message).block(Block::bordered().title("Battery"));
        frame.render_widget(battery, area);
    }
}

fn render_telemetry(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    if state.telemetry.has_decoded_samples() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10),
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .split(area);

        render_device_browser(frame, chunks[0], state);
        render_session_summary(frame, chunks[1], state);
        render_sparkline(
            frame,
            chunks[2],
            "Speed",
            &state.telemetry.speed_mph,
            Color::Yellow,
        );
        render_sparkline(
            frame,
            chunks[3],
            "Voltage",
            &state.telemetry.voltage_v,
            Color::Magenta,
        );
        render_telemetry_trend(frame, chunks[4], state);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10),
                Constraint::Length(4),
                Constraint::Length(4),
                Constraint::Length(4),
                Constraint::Fill(1),
            ])
            .split(area);

        render_device_browser(frame, chunks[0], state);
        render_session_summary(frame, chunks[1], state);
        render_pending_telemetry(frame, chunks[2], state);
        render_pending_telemetry_detail(frame, chunks[3], state);
        render_pending_telemetry_wait(frame, chunks[4]);
    }
}

fn render_telemetry_trend(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let chart = Chart::new(vec![
        Dataset::default()
            .name("current")
            .marker(ratatui::symbols::Marker::Dot)
            .style(Style::new().fg(Color::LightBlue))
            .data(state.telemetry.current_points.as_slice()),
        Dataset::default()
            .name("temperature")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::new().fg(Color::Red))
            .data(state.telemetry.temperature_points.as_slice()),
    ])
    .block(panel_block("Trend"))
    .x_axis(Axis::default().title("samples").bounds([
        0.0,
        f64::from(u32::try_from(HISTORY_LIMIT).unwrap_or(u32::MAX)),
    ]))
    .y_axis(Axis::default().title("value").bounds([0.0, 100.0]));
    frame.render_widget(chart, area);
}

fn render_pending_telemetry(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let text = Paragraph::new(vec![
        Line::from("decoded telemetry samples: 0"),
        Line::from(vec![
            Span::styled("raw notifications ", Style::new().fg(Color::Gray)),
            Span::raw(state.counters.notifications.to_string()),
            Span::styled(" bytes ", Style::new().fg(Color::Gray)),
            Span::raw(state.counters.notification_bytes.to_string()),
        ]),
    ])
    .block(panel_block("Decoded telemetry"));
    frame.render_widget(text, area);
}

fn render_pending_telemetry_detail(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let latest = state
        .counters
        .latest_notification_len
        .map_or_else(|| "none".to_owned(), |len| len.to_string());
    let text = Paragraph::new(vec![
        Line::from("waiting for protocol decoder output"),
        Line::from(vec![
            Span::styled("latest notification bytes ", Style::new().fg(Color::Gray)),
            Span::raw(latest),
        ]),
    ])
    .block(panel_block("Raw stream"));
    frame.render_widget(text, area);
}

fn render_pending_telemetry_wait(frame: &mut Frame<'_>, area: Rect) {
    let text = Paragraph::new(vec![
        Line::from("waiting for protocol decoder output"),
        Line::from("raw notifications are arriving from the connected device"),
        Line::from("decoded speed, voltage, current, and temperature will fill in here"),
    ])
    .block(panel_block("Decoder"));
    frame.render_widget(text, area);
}

fn render_session_summary(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let counters = &state.counters;
    let lines = vec![
        Line::from(vec![
            Span::styled("devices ", Style::new().fg(Color::Gray)),
            Span::raw(counters.discovered.to_string()),
            Span::styled(" connected ", Style::new().fg(Color::Gray)),
            Span::raw(counters.connected.to_string()),
        ]),
        Line::from(vec![
            Span::styled("subscriptions ", Style::new().fg(Color::Gray)),
            Span::raw(counters.subscriptions.to_string()),
            Span::styled(" notifications ", Style::new().fg(Color::Gray)),
            Span::raw(counters.notifications.to_string()),
        ]),
    ];

    let panel = Paragraph::new(lines).block(panel_block("Session"));
    frame.render_widget(panel, area);
}

fn render_device_browser(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let browser = &state.scan_browser;
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("filters ", Style::new().fg(Color::Gray)),
        Span::raw("address "),
        Span::raw(browser.filters.address.as_deref().unwrap_or("none")),
        Span::raw(" id "),
        Span::raw(browser.filters.identifier.as_deref().unwrap_or("none")),
        Span::raw(" name "),
        Span::raw(browser.filters.name_contains.as_deref().unwrap_or("none")),
    ]));

    if let Some(selected) = browser.selected() {
        lines.push(Line::from(vec![
            Span::styled("selected ", Style::new().fg(Color::Gray)),
            Span::raw(selected.name.as_str()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("address ", Style::new().fg(Color::Gray)),
            Span::raw(selected.address.as_str()),
            Span::raw(" id "),
            Span::raw(selected.identifier.as_str()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("rssi ", Style::new().fg(Color::Gray)),
            Span::raw(selected.rssi.as_str()),
            Span::raw(" services "),
            Span::raw(selected.services.as_str()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("selected ", Style::new().fg(Color::Gray)),
            Span::raw("none"),
        ]));
        lines.push(Line::from(vec![
            Span::styled("detail ", Style::new().fg(Color::Gray)),
            Span::raw("scan observations empty"),
        ]));
    }

    lines.push(Line::from(vec![Span::styled(
        "observations",
        Style::new().fg(Color::Gray),
    )]));

    let real_observations = browser
        .observations
        .iter()
        .filter(|observation| observation.real_device);

    if real_observations.clone().next().is_none() {
        lines.push(Line::from(vec![Span::raw("no scan observations")]));
    } else {
        for observation in real_observations {
            lines.push(Line::from(vec![
                Span::raw(observation.name.as_str()),
                Span::raw(" | "),
                Span::raw(observation.address.as_str()),
                Span::raw(" | "),
                Span::raw(observation.identifier.as_str()),
                Span::raw(" | "),
                Span::raw(observation.rssi.as_str()),
                Span::raw(" | "),
                Span::raw(observation.services.as_str()),
            ]));
        }
    }

    let panel = Paragraph::new(lines)
        .block(panel_block("Device browser"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn render_sparkline(frame: &mut Frame<'_>, area: Rect, title: &str, samples: &[u64], color: Color) {
    let spark = Sparkline::default()
        .block(panel_block(title))
        .style(Style::new().fg(color))
        .data(samples)
        .max(100);
    frame.render_widget(spark, area);
}

fn render_logs(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let lines = state
        .logs
        .iter()
        .rev()
        .map(|entry| {
            Line::from(vec![
                Span::styled("[", Style::new().fg(Color::Gray)),
                Span::raw(entry.level.as_str()),
                Span::styled("] ", Style::new().fg(Color::Gray)),
                Span::raw(entry.message.as_str()),
            ])
        })
        .collect::<Vec<_>>();

    let log_panel = Paragraph::new(lines)
        .block(panel_block("Recent events"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(log_panel, area);
}

fn panel_block(title: &str) -> Block<'_> {
    Block::bordered().title(title)
}

fn percent_ratio(value: u64) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX)) / 100.0
}

fn to_f64(value: u64) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn index_to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use cutout_btle::{ConnectionTarget, PeripheralObservation};
    use cutout_core::{Measured, TelemetrySnapshot};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn render_buffer(state: &DashboardState, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal creates");
        terminal
            .draw(|frame| render_dashboard(frame, state))
            .expect("dashboard renders");
        terminal.backend().buffer().clone()
    }

    fn buffer_text(buffer: &Buffer) -> String {
        let mut text = String::new();
        for y in 0..buffer.area.height {
            if y > 0 {
                text.push('\n');
            }
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
        }
        text
    }

    #[test]
    fn sample_state_has_device_profiles_and_logs() {
        let state = DashboardState::sample();

        assert_eq!(state.source, DashboardSource::Demo);
        assert_eq!(state.active_tab, 0);
        assert_eq!(
            state.provenance,
            Some("fixture/demo replay: aero-nf2557.v1")
        );
        assert_eq!(state.device.name, "Aero NF2557");
        assert_eq!(state.scan_browser.selected, 0);
        assert_eq!(state.scan_browser.observations.len(), 3);
        assert_eq!(
            state.scan_browser.filters.address.as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );
        assert_eq!(
            state.scan_browser.filters.identifier.as_deref(),
            Some("platform-0001")
        );
        assert_eq!(
            state.scan_browser.filters.name_contains.as_deref(),
            Some("Aero")
        );
        assert_eq!(state.profiles.len(), 3);
        assert_eq!(state.logs.len(), 2);
        assert_eq!(
            state.profiles[0].family,
            ProfileFamily::AeroVeteran {
                current_limit_a: Some("45A".to_owned()),
                tail_status: "raw tail preserved".to_owned(),
                summary: "Aero/Veteran current 45A / raw tail preserved".to_owned(),
            }
        );
        assert_eq!(
            state.profiles[1].family,
            ProfileFamily::Pending {
                family: "Begode/Falcon".to_owned(),
                note: "unsupported / pending".to_owned(),
                summary: "pending Begode/Falcon unsupported / pending".to_owned(),
            }
        );
        assert_eq!(
            state.telemetry.current_points.len(),
            state.telemetry.current_a.len()
        );
        assert_eq!(
            state.telemetry.temperature_points.len(),
            state.telemetry.temperature_c.len()
        );
    }

    #[test]
    fn live_target_state_never_uses_demo_fixture_data() {
        let state = DashboardState::live_target("NF2557".to_owned());

        assert_eq!(state.source, DashboardSource::Live);
        assert_eq!(state.device.name, "NF2557");
        assert_eq!(state.device.connection_state, "target selected");
        assert_eq!(
            state.scan_browser.filters.name_contains.as_deref(),
            Some("NF2557")
        );
        assert_eq!(state.provenance, None);
        assert!(state.scan_browser.observations.is_empty());
        assert!(state.profiles.is_empty());
        assert!(state.logs.is_empty());
    }

    #[test]
    fn live_connected_state_uses_connection_summary_data() {
        let target = ConnectionTarget {
            address: None,
            identifier: None,
            name_contains: Some("NF2557".to_owned()),
        };
        let summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "platform-0001".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("Aero NF2557".to_owned()),
                rssi: Some(-61),
                advertised_services: vec![],
            },
            services: Vec::new(),
        };

        let state = DashboardState::live_connected(&target, &summary);

        assert_eq!(state.source, DashboardSource::Live);
        assert_eq!(state.device.name, "Aero NF2557");
        assert_eq!(state.device.address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(state.device.connection_state, "connected");
        assert_eq!(state.counters.connected, 1);
        assert_eq!(state.telemetry.battery_pct, None);
        assert_eq!(state.telemetry.battery_source, BatterySource::Unknown);
        assert_eq!(state.scan_browser.observations.len(), 1);
        assert!(state.scan_browser.observations[0].real_device);
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.profiles[0].source, "gatt");
    }

    #[test]
    fn live_session_report_updates_real_counters_and_logs() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            writes: 0,
            subscribes: 1,
            notifications: 184,
            notification_bytes: 18_400,
            latest_notification_len: Some(100),
            telemetry: 0,
            telemetry_snapshot: TelemetrySnapshot::default(),
            diagnostics: 0,
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.counters.subscriptions, 1);
        assert_eq!(state.counters.notifications, 184);
        assert_eq!(state.counters.notification_bytes, 18_400);
        assert_eq!(state.counters.latest_notification_len, Some(100));
        assert!(
            state
                .logs
                .iter()
                .any(|entry| entry.message.contains("notifications=184"))
        );
    }

    #[test]
    fn live_session_report_applies_telemetry_snapshot() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            writes: 0,
            subscribes: 1,
            notifications: 2,
            notification_bytes: 200,
            latest_notification_len: Some(100),
            telemetry: 1,
            telemetry_snapshot: TelemetrySnapshot {
                speed_mm_s: Some(Measured::reported(4_470)),
                voltage_mv: Some(Measured::reported(84_400)),
                battery_current_ma: Some(Measured::reported(-12_400)),
                controller_temperature_mc: Some(Measured::reported(36_600)),
                battery_percent_reported: Some(Measured::reported(77)),
                ..TelemetrySnapshot::default()
            },
            diagnostics: 0,
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.telemetry.battery_pct, Some(77));
        assert_eq!(
            state.telemetry.battery_source,
            BatterySource::TelemetryReported
        );
        assert_eq!(state.telemetry.latest_voltage_v, Some(84));
        assert_eq!(state.telemetry.speed_mph, vec![10]);
        assert_eq!(state.telemetry.voltage_v, vec![84]);
        assert_eq!(state.telemetry.current_a, vec![12]);
        assert_eq!(state.telemetry.temperature_c, vec![37]);
    }

    #[test]
    fn live_battery_level_updates_battery_gauge_from_real_reading() {
        let mut state = DashboardState::empty();

        state.apply_battery_percent(88);

        assert_eq!(state.telemetry.battery_pct, Some(88));
        assert_eq!(state.telemetry.battery_source, BatterySource::StandardBle);
        assert!(
            state
                .logs
                .iter()
                .any(|entry| entry.message == "battery level 88%")
        );

        state.apply_battery_percent(150);

        assert_eq!(state.telemetry.battery_pct, Some(100));
        assert_eq!(state.telemetry.battery_source, BatterySource::StandardBle);
        assert!(
            state
                .logs
                .iter()
                .any(|entry| entry.message == "battery level 100%")
        );
    }

    #[test]
    fn live_session_report_uses_estimated_battery_from_voltage_telemetry() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            writes: 0,
            subscribes: 1,
            notifications: 1,
            notification_bytes: 20,
            latest_notification_len: Some(20),
            telemetry: 1,
            telemetry_snapshot: TelemetrySnapshot {
                voltage_mv: Some(Measured::reported(108_760)),
                battery_percent_estimated: Some(Measured::estimated(39)),
                ..TelemetrySnapshot::default()
            },
            diagnostics: 0,
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.telemetry.battery_pct, Some(39));
        assert_eq!(
            state.telemetry.battery_source,
            BatterySource::TelemetryEstimated
        );
        assert_eq!(state.telemetry.latest_voltage_v, Some(109));
        assert_eq!(state.telemetry.voltage_v, vec![109]);

        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("Battery estimated"));
        assert!(text.contains("39%"));
        assert!(text.contains("Voltage"));
    }

    #[test]
    fn live_dashboard_renders_unknown_battery_until_real_reading_arrives() {
        let state = DashboardState::empty();

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Battery"));
        assert!(text.contains("unknown"));
    }

    #[test]
    fn live_target_advance_does_not_emit_fixture_heartbeat() {
        let mut state = DashboardState::live_target("NF2557".to_owned());

        state.advance();

        assert!(state.logs.is_empty());
        assert_eq!(state.counters.notifications, 0);
    }

    #[test]
    fn tab_navigation_wraps_forward_and_backward() {
        let mut state = DashboardState::empty();

        state.next_tab();
        assert_eq!(state.active_tab, 1);

        state.previous_tab();
        assert_eq!(state.active_tab, 0);

        state.previous_tab();
        assert_eq!(state.active_tab, 3);
    }

    #[test]
    fn arrow_escape_sequences_emit_tab_navigation() {
        let (tx, rx) = mpsc::channel();

        handle_escape_sequence(&mut Cursor::new([b'[', b'C']), &tx);
        assert_eq!(rx.try_recv(), Ok(DashboardInput::NextTab));

        handle_escape_sequence(&mut Cursor::new([b'[', b'D']), &tx);
        assert_eq!(rx.try_recv(), Ok(DashboardInput::PreviousTab));
    }

    #[test]
    fn telemetry_tab_renders_telemetry_page() {
        let mut state = DashboardState::sample();
        state.active_tab = 1;

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Device browser"));
        assert!(text.contains("Session"));
        assert!(text.contains("notifications"));
        assert!(text.contains("Speed"));
        assert!(text.contains("Voltage"));
    }

    #[test]
    fn live_telemetry_tab_renders_raw_stream_when_decoder_has_no_samples() {
        let mut state = DashboardState::empty();
        state.active_tab = 1;
        state.counters.notifications = 47;
        state.counters.notification_bytes = 4_700;
        state.counters.latest_notification_len = Some(100);

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Decoded telemetry"));
        assert!(text.contains("raw notifications"));
        assert!(text.contains("47"));
        assert!(text.contains("bytes"));
        assert!(text.contains("4700"));
        assert!(text.contains("latest notification bytes 100"));
        assert!(!text.contains("Speed"));
        assert!(!text.contains("Voltage"));
    }

    #[test]
    fn dashboard_render_leaves_background_transparent() {
        let buffer = render_buffer(&DashboardState::empty(), 80, 24);

        assert_eq!(buffer[(79, 23)].bg, Color::Reset);
    }

    #[test]
    fn advance_trims_logs_and_updates_counters() {
        let mut state = DashboardState::sample();
        let notifications = state.counters.notifications;

        for _ in 0..20 {
            state.advance();
        }

        assert!(state.counters.notifications > notifications);
        assert!(state.logs.len() <= LOG_LIMIT);
        assert_eq!(state.telemetry.speed_mph.len(), HISTORY_LIMIT);
    }

    #[test]
    fn dashboard_render_contains_the_expected_panels() {
        let buffer = render_buffer(&DashboardState::sample(), 120, 36);
        let text = buffer_text(&buffer);

        for needle in [
            "Cutout dashboard",
            "Target",
            "Battery",
            "Signal",
            "Device browser",
            "selected Aero NF2557",
            "observations",
            "Profiles",
            "Aero/Veteran",
            "Speed",
            "Voltage",
            "Recent events",
            "Aero NF2557",
            "fixture replay loaded from fixture/demo replay: aero-nf2557.v1",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
    }

    #[test]
    fn dashboard_render_keeps_log_lines_visible() {
        let mut state = DashboardState::sample();
        state.logs.push_back(LogEntry {
            level: "warn".to_owned(),
            message: "profile payload still missing battery chemistry".to_owned(),
        });

        let buffer = render_buffer(&state, 100, 30);
        let text = buffer_text(&buffer);

        assert!(text.contains("battery chemistry"));
    }

    #[test]
    fn device_browser_shows_multiple_scan_observations() {
        let state = DashboardState::sample();
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).expect("terminal creates");
        terminal
            .draw(|frame| render_device_browser(frame, frame.area(), &state))
            .expect("device browser renders");

        let text = buffer_text(terminal.backend().buffer());

        assert!(text.contains("Begode X"));
        assert!(text.contains("-77 dBm"));
    }

    #[test]
    fn fixture_replay_hydrates_state_from_checked_in_demo_steps() {
        let mut state = DashboardState::empty();
        let mut replay = FixtureReplay::demo();

        assert_eq!(
            replay.provenance(),
            Some("fixture/demo replay: aero-nf2557.v1")
        );
        replay.apply_all(&mut state);

        assert_eq!(
            state.provenance,
            Some("fixture/demo replay: aero-nf2557.v1")
        );
        assert_eq!(state.device.name, "Aero NF2557");
        assert_eq!(state.counters.notifications, 27);
        assert_eq!(state.profiles.len(), 3);
        assert_eq!(state.telemetry.speed_mph.len(), 12);
        assert_eq!(state.telemetry.current_points.len(), 12);
        assert_eq!(state.telemetry.temperature_points.len(), 12);
        assert!(
            state
                .logs
                .iter()
                .any(|entry| entry.message.contains("fixture replay"))
        );
        assert!(!replay.apply_next(&mut state));
    }

    #[test]
    fn dashboard_render_shows_fixture_provenance() {
        let buffer = render_buffer(&DashboardState::sample(), 120, 36);
        let text = buffer_text(&buffer);

        assert!(text.contains("fixture/demo replay"));
    }

    #[test]
    fn device_browser_shows_selected_device_details() {
        let state = DashboardState::sample();

        let buffer = render_buffer(&state, 120, 36);
        let text = buffer_text(&buffer);

        assert!(text.contains("selected Aero NF2557"));
        assert!(text.contains("platform-0001"));
        assert!(text.contains("-61 dBm"));
        assert!(text.contains("battery, throttle, telemetry"));
    }

    #[test]
    fn empty_device_browser_renders_no_scan_state() {
        let state = DashboardState::empty();

        let buffer = render_buffer(&state, 120, 36);
        let text = buffer_text(&buffer);

        assert!(text.contains("Device browser"));
        assert!(text.contains("scan observations empty"));
    }
}
