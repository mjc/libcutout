use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use cutout_btle::{ConnectionSummary, ConnectionTarget, SessionBridgeEvent, SessionBridgeReport};
use cutout_core::{
    NotificationIngestOutcome, ParserDiagnostics, ProtocolFamily, ReadOnlyResponse, TelemetryDelta,
    TelemetrySnapshot,
};
use cutout_protocols::VeteranModelProfile;
use ratatui::termina::{
    PlatformTerminal, Terminal as _,
    escape::csi::{self},
};
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

use crate::logging::{dashboard_log_sink_installed, log_dashboard_recent_event};

const LOG_LIMIT: usize = 1_024;
const HISTORY_LIMIT: usize = 32;
const READ_ONLY_SUMMARY_LIMIT: usize = 16;
const TAB_COUNT: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DashboardState {
    pub(crate) source: DashboardSource,
    pub(crate) active_tab: usize,
    pub(crate) provenance: Option<String>,
    pub(crate) device: DeviceSnapshot,
    pub(crate) scan_browser: ScanBrowser,
    pub(crate) telemetry: TelemetryWindow,
    pub(crate) read_only: ReadOnlyDashboardState,
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
    pub(crate) make: String,
    pub(crate) model: String,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReadOnlyDashboardState {
    pub(crate) firmware: Option<String>,
    pub(crate) settings: VecDeque<String>,
    pub(crate) bms_pages: VecDeque<String>,
    pub(crate) latest_bms_temperature: Option<String>,
    pub(crate) diagnostics: u64,
    pub(crate) raw_telemetry: u64,
    pub(crate) unknown_raw_pages: u64,
}

impl ReadOnlyDashboardState {
    fn apply_response(&mut self, response: ReadOnlyResponse) {
        match response {
            ReadOnlyResponse::Firmware(firmware) => {
                self.firmware = Some(format_firmware_summary(firmware));
            }
            ReadOnlyResponse::Settings(settings) => {
                for entry in settings.entries.into_iter().flatten() {
                    push_bounded(
                        &mut self.settings,
                        format!(
                            "field={} value={} quality={} verification={}",
                            entry.field.id,
                            entry.field.value,
                            quality_name(entry.quality),
                            verification_name(entry.verification)
                        ),
                    );
                }
            }
            ReadOnlyResponse::Battery(payload) => {
                let page = payload.page();
                if matches!(
                    page.kind,
                    cutout_core::BatteryPageKind::Raw | cutout_core::BatteryPageKind::Metadata
                ) {
                    self.unknown_raw_pages = self.unknown_raw_pages.saturating_add(1);
                }
                let temperature_summary = bms_temperature_summary(payload);
                let current_summary = bms_current_summary(payload);
                if let Some(summary) = temperature_summary.as_ref() {
                    self.latest_bms_temperature = Some(format!(
                        "selector={} verification={}{}",
                        page.selector,
                        verification_name(page.verification),
                        summary
                    ));
                }
                push_bounded(
                    &mut self.bms_pages,
                    format!(
                        "selector={} kind={} verification={}{}{}",
                        page.selector,
                        battery_page_kind_name(page.kind),
                        verification_name(page.verification),
                        temperature_summary.as_deref().unwrap_or(""),
                        current_summary.as_deref().unwrap_or("")
                    ),
                );
            }
            ReadOnlyResponse::Diagnostics(_) => {
                self.diagnostics = self.diagnostics.saturating_add(1);
            }
            ReadOnlyResponse::RawTelemetry(_) => {
                self.raw_telemetry = self.raw_telemetry.saturating_add(1);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DashboardUpdate {
    BatteryPercent(u8),
    SessionReport(Box<SessionBridgeReport>),
    Log { level: String, message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TelemetryWindow {
    pub(crate) battery_pct: Option<u64>,
    pub(crate) battery_source: BatterySource,
    pub(crate) signal_pct: u64,
    pub(crate) latest_speed_mph: Option<u64>,
    pub(crate) latest_voltage_v: Option<u64>,
    pub(crate) latest_current_a: Option<i64>,
    pub(crate) latest_temperature_c: Option<i64>,
    pub(crate) latest_distance_m: Option<u64>,
    pub(crate) latest_pitch_deg: Option<i64>,
    pub(crate) latest_pwm_pct: Option<i64>,
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

const DEMO_PROVENANCE: &str = "demo state: aero-nf2557.v1";
const DEMO_SPEED_MPH: &[u64] = &[0, 4, 9, 14, 18, 22, 24, 21, 19, 17, 16, 18];
const DEMO_VOLTAGE_V: &[u64] = &[52, 52, 53, 53, 54, 54, 55, 55, 55, 54, 54, 53];
const DEMO_CURRENT_A: &[u64] = &[3, 4, 6, 7, 8, 9, 10, 10, 9, 8, 7, 6];
const DEMO_TEMPERATURE_C: &[u64] = &[30, 31, 31, 32, 32, 33, 34, 34, 35, 35, 34, 33];

impl DashboardState {
    pub(crate) fn empty() -> Self {
        Self {
            source: DashboardSource::Live,
            active_tab: 0,
            provenance: None,
            device: DeviceSnapshot {
                make: "unknown".to_owned(),
                model: "unknown".to_owned(),
                name: "unknown".to_owned(),
                address: "unknown".to_owned(),
                identifier: "unknown".to_owned(),
                firmware: "unknown".to_owned(),
                connection_state: "scanning".to_owned(),
            },
            scan_browser: ScanBrowser::empty(),
            telemetry: TelemetryWindow::empty(),
            read_only: ReadOnlyDashboardState::default(),
            profiles: Vec::new(),
            counters: SessionCounters::default(),
            logs: VecDeque::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn sample() -> Self {
        Self::demo(None)
    }

    pub(crate) fn demo(device: Option<&str>) -> Self {
        let mut state = Self::empty();
        state.source = DashboardSource::Demo;
        state.apply_demo_seed();

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

    fn apply_demo_seed(&mut self) {
        self.provenance = Some(DEMO_PROVENANCE.to_owned());
        self.apply_device_snapshot(
            "Aero NF2557",
            "AA:BB:CC:DD:EE:FF",
            "platform-0001",
            "v3.8.12",
            "connected",
        );
        self.scan_browser.filters = TargetFilterSummary {
            address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            identifier: Some("platform-0001".to_owned()),
            name_contains: Some("Aero".to_owned()),
        };
        self.scan_browser.push_observation(
            ScanObservation {
                name: "Aero NF2557".to_owned(),
                address: "AA:BB:CC:DD:EE:FF".to_owned(),
                identifier: "platform-0001".to_owned(),
                rssi: "-61 dBm".to_owned(),
                services: "battery, throttle, telemetry".to_owned(),
                real_device: true,
            },
            true,
        );
        self.scan_browser.push_observation(
            ScanObservation {
                name: "Begode X".to_owned(),
                address: "11:22:33:44:55:66".to_owned(),
                identifier: "platform-0202".to_owned(),
                rssi: "-77 dBm".to_owned(),
                services: "diagnostics, state".to_owned(),
                real_device: true,
            },
            false,
        );
        self.scan_browser.push_observation(
            ScanObservation {
                name: "Veteran V14".to_owned(),
                address: "AA:00:11:22:33:44".to_owned(),
                identifier: "platform-0303".to_owned(),
                rssi: "-84 dBm".to_owned(),
                services: "battery, control".to_owned(),
                real_device: true,
            },
            false,
        );
        self.counters = SessionCounters {
            discovered: 8,
            connected: 1,
            subscriptions: 4,
            notifications: 27,
            notification_bytes: 0,
            latest_notification_len: None,
        };
        self.telemetry.load_window(
            74,
            81,
            DEMO_SPEED_MPH,
            DEMO_VOLTAGE_V,
            DEMO_CURRENT_A,
            DEMO_TEMPERATURE_C,
        );
        self.profiles.push(ProfileSnapshot {
            name: "Primary drive".to_owned(),
            source: "probe".to_owned(),
            status: "ready".to_owned(),
            family: ProfileFamily::AeroVeteran {
                current_limit_a: Some("45A".to_owned()),
                tail_status: "raw tail preserved".to_owned(),
                summary: "Aero/Veteran current 45A / raw tail preserved".to_owned(),
            },
        });
        self.profiles.push(ProfileSnapshot {
            name: "Battery pack".to_owned(),
            source: "capture".to_owned(),
            status: "warming".to_owned(),
            family: ProfileFamily::Pending {
                family: "Begode/Falcon".to_owned(),
                note: "unsupported / pending".to_owned(),
                summary: "pending Begode/Falcon unsupported / pending".to_owned(),
            },
        });
        self.profiles.push(ProfileSnapshot {
            name: "Motor controller".to_owned(),
            source: "manual".to_owned(),
            status: "partial".to_owned(),
            family: ProfileFamily::AeroVeteran {
                current_limit_a: None,
                tail_status: "raw tail unknown".to_owned(),
                summary: "unknown Aero/Veteran current unknown / raw tail unknown".to_owned(),
            },
        });
        self.push_log("info", "demo state loaded from demo state: aero-nf2557.v1");
        self.push_log("debug", "dashboard booted in read-only mode");
    }

    #[cfg(test)]
    pub(crate) fn live_target(device: String) -> Self {
        let mut state = Self::empty();
        let identity = classify_device_identity(&device);
        identity.make.clone_into(&mut state.device.make);
        identity.model.clone_into(&mut state.device.model);
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
        let device_name = observation
            .name
            .clone()
            .unwrap_or_else(|| "unknown".to_owned());
        let identity = classify_device_identity(&device_name);
        state.device = DeviceSnapshot {
            make: identity.make,
            model: identity.model,
            name: device_name,
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
        state.telemetry.signal_pct = observation.rssi.map_or(0, rssi_to_signal_percent);
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
        self.counters.subscriptions = self
            .counters
            .subscriptions
            .saturating_add(usize_to_u64(report.subscribes));
        self.counters.notifications = self
            .counters
            .notifications
            .saturating_add(usize_to_u64(report.notifications));
        self.counters.notification_bytes = self
            .counters
            .notification_bytes
            .saturating_add(usize_to_u64(report.notification_bytes));
        self.counters.latest_notification_len = report.latest_notification_len.map(usize_to_u64);
        self.push_log(
            "info",
            &format!(
                "session update protocol_writes={} writes={} subscribes={} notifications={} bytes={}",
                report.protocol_writes,
                report.writes,
                report.subscribes,
                report.notifications,
                report.notification_bytes
            ),
        );

        if report.telemetry == 0 {
            if report_has_no_parsed_events(report) {
                self.push_log(
                    "warn",
                    "notifications received but telemetry decoder produced no samples",
                );
            }
        } else {
            self.push_log("info", &format!("telemetry samples={}", report.telemetry));
        }

        if report.diagnostics > 0 {
            self.push_log("warn", &format!("diagnostics={}", report.diagnostics));
        }

        if report.telemetry > 0 {
            let snapshot = report.telemetry_snapshot;
            self.push_log("info", &format_mapped_telemetry_event(snapshot));
            self.telemetry.apply_snapshot(snapshot);
        }
        for response in &report.read_only_response_events {
            self.read_only.apply_response(*response);
        }
        if report.read_only_responses > 0 {
            self.push_log(
                "info",
                &format!("read-only responses={}", report.read_only_responses),
            );
        }
        if report_has_no_parsed_events(report) {
            self.push_log("trace", &format_unmapped_telemetry_event(report));
        }
        for event in &report.events {
            let (level, message) = format_bridge_event(event);
            self.push_log(level, &message);
        }
    }

    pub(crate) fn apply_battery_percent(&mut self, percent: u8) {
        let percent = percent.min(100);
        self.telemetry.battery_pct = Some(u64::from(percent));
        self.telemetry.battery_source = BatterySource::StandardBle;
        self.push_log("info", &format!("battery level {percent}%"));
    }

    pub(crate) fn apply_update(&mut self, update: DashboardUpdate) {
        match update {
            DashboardUpdate::BatteryPercent(percent) => self.apply_battery_percent(percent),
            DashboardUpdate::SessionReport(report) => self.apply_session_report(&report),
            DashboardUpdate::Log { level, message } => self.push_log_from_tracing(&level, &message),
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

    fn apply_device_snapshot(
        &mut self,
        name: &str,
        address: &str,
        identifier: &str,
        firmware: &str,
        connection_state: &str,
    ) {
        let identity = classify_device_identity(name);
        self.device = DeviceSnapshot {
            make: identity.make,
            model: identity.model,
            name: name.to_owned(),
            address: address.to_owned(),
            identifier: identifier.to_owned(),
            firmware: firmware.to_owned(),
            connection_state: connection_state.to_owned(),
        };
    }

    fn push_log(&mut self, level: &str, message: &str) {
        log_dashboard_recent_event(level, message);
        if !dashboard_log_sink_installed() {
            self.push_log_entry(level, message);
        }
    }

    fn push_log_from_tracing(&mut self, level: &str, message: &str) {
        self.push_log_entry(level, message);
    }

    fn push_log_entry(&mut self, level: &str, message: &str) {
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
            latest_speed_mph: None,
            latest_voltage_v: None,
            latest_current_a: None,
            latest_temperature_c: None,
            latest_distance_m: None,
            latest_pitch_deg: None,
            latest_pwm_pct: None,
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
        self.latest_speed_mph = speed_mph.last().copied();
        self.latest_voltage_v = voltage_v.last().copied();
        self.latest_current_a = current_a.last().copied().and_then(u64_to_i64);
        self.latest_temperature_c = temperature_c.last().copied().and_then(u64_to_i64);
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
        self.latest_speed_mph = Some(next_speed);
        self.latest_voltage_v = Some(next_voltage);
        self.latest_current_a = u64_to_i64(next_current);
        self.latest_temperature_c = u64_to_i64(next_temperature);
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
            let speed_mph = mm_s_to_mph(speed.value);
            self.latest_speed_mph = Some(speed_mph);
            push_sample(&mut self.speed_mph, speed_mph);
        }
        if let Some(voltage) = snapshot.voltage_mv {
            let volts = millivolts_to_volts(voltage.value);
            self.latest_voltage_v = Some(volts);
            seed_or_push_sample(&mut self.voltage_v, volts);
        }
        if let Some(current) = snapshot.battery_current_ma.or(snapshot.motor_current_ma) {
            self.latest_current_a = Some(milliamps_to_amps(current.value));
            push_sample(&mut self.current_a, milliamps_to_amps_abs(current.value));
        }
        if let Some(temperature) = snapshot
            .controller_temperature_mc
            .or(snapshot.motor_temperature_mc)
            .or(snapshot.battery_temperature_mc)
        {
            self.latest_temperature_c = Some(millicelsius_to_celsius_signed(temperature.value));
            push_sample(
                &mut self.temperature_c,
                millicelsius_to_celsius(temperature.value),
            );
        }
        if let Some(distance) = snapshot.distance_mm {
            self.latest_distance_m = Some(distance.value / 1_000);
        }
        if let Some(pitch) = snapshot.pitch_mdeg {
            self.latest_pitch_deg = Some(millidegrees_to_degrees(pitch.value));
        }
        if let Some(pwm) = snapshot.pwm_permille {
            self.latest_pwm_pct = Some(permille_to_percent(pwm.value));
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

fn seed_or_push_sample(series: &mut Vec<u64>, value: u64) {
    if series.is_empty() {
        series.resize(HISTORY_LIMIT, value);
        return;
    }

    push_sample(series, value);
}

fn push_bounded<T>(items: &mut VecDeque<T>, item: T) {
    if items.len() == READ_ONLY_SUMMARY_LIMIT {
        items.pop_front();
    }
    items.push_back(item);
}

fn format_firmware_summary(firmware: cutout_core::FirmwareInfo) -> String {
    let major = firmware
        .firmware_major
        .map_or_else(|| "?".to_owned(), |value| value.value.to_string());
    let minor = firmware
        .firmware_minor
        .map_or_else(|| "?".to_owned(), |value| value.value.to_string());
    let patch = firmware
        .firmware_patch
        .map_or_else(|| "?".to_owned(), |value| value.value.to_string());
    format!("{major}.{minor}.{patch}")
}

const fn battery_page_kind_name(kind: cutout_core::BatteryPageKind) -> &'static str {
    match kind {
        cutout_core::BatteryPageKind::Metadata => "metadata",
        cutout_core::BatteryPageKind::CellVoltage => "cell_voltage",
        cutout_core::BatteryPageKind::Temperature => "temperature",
        cutout_core::BatteryPageKind::Raw => "raw",
    }
}

fn bms_temperature_summary(payload: cutout_core::BatteryPagePayload) -> Option<String> {
    let temperatures = payload.temperatures_mc();
    if !matches!(payload, cutout_core::BatteryPagePayload::Temperature(_)) {
        return None;
    }

    let mut summary = String::from(" temps_c=");
    let mut wrote = false;
    for temperature in temperatures.into_iter().flatten() {
        if wrote {
            summary.push(',');
        }
        wrote = true;
        summary.push_str(&millicelsius_to_celsius_signed(temperature.value).to_string());
    }

    wrote.then_some(summary)
}

fn bms_current_summary(payload: cutout_core::BatteryPagePayload) -> Option<String> {
    payload
        .battery()
        .current_ma
        .map(|current| format!(" current={}A", milliamps_to_amps(current.value)))
}

const fn quality_name(quality: cutout_core::ValueQuality) -> &'static str {
    match quality {
        cutout_core::ValueQuality::Known => "known",
        cutout_core::ValueQuality::Inferred => "inferred",
    }
}

const fn verification_name(verification: cutout_core::VerificationStatus) -> &'static str {
    match verification {
        cutout_core::VerificationStatus::Unverified => "unverified",
        cutout_core::VerificationStatus::Inferred => "inferred",
        cutout_core::VerificationStatus::SourceVerified => "source_verified",
        cutout_core::VerificationStatus::HardwareVerified => "hardware_verified",
        cutout_core::VerificationStatus::SourceAndHardwareVerified => {
            "source_and_hardware_verified"
        }
    }
}

fn mm_s_to_mph(value: i32) -> u64 {
    let numerator = u64::from(value.unsigned_abs()) * 1_000;
    numerator.saturating_add(223_694) / 447_388
}

fn millivolts_to_volts(value: i32) -> u64 {
    u64::from(value.unsigned_abs()).saturating_add(500) / 1_000
}

fn milliamps_to_amps(value: i32) -> i64 {
    i64::from(value) / 1_000
}

fn milliamps_to_amps_abs(value: i32) -> u64 {
    u64::from(value.unsigned_abs()).saturating_add(500) / 1_000
}

fn millicelsius_to_celsius_signed(value: i32) -> i64 {
    i64::from(value) / 1_000
}

fn millicelsius_to_celsius(value: i32) -> u64 {
    u64::from(value.unsigned_abs()).saturating_add(500) / 1_000
}

fn millidegrees_to_degrees(value: i32) -> i64 {
    i64::from(value) / 1_000
}

fn permille_to_percent(value: i16) -> i64 {
    i64::from(value) / 10
}

fn u64_to_i64(value: u64) -> Option<i64> {
    i64::try_from(value).ok()
}

fn format_distance_mm(value: u64) -> String {
    format_distance_m(value / 1_000)
}

fn format_optional_distance_m(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), format_distance_m)
}

fn format_distance_m(value: u64) -> String {
    if value < 1_000 {
        return format!("{value} m");
    }

    let km_tenths = value.saturating_mul(10).saturating_add(500) / 1_000;
    format!("{} km", format_tenths(km_tenths))
}

fn format_tenths(value: u64) -> String {
    format!("{}.{}", value / 10, value % 10)
}

struct DeviceIdentity {
    make: String,
    model: String,
}

fn classify_device_identity(name: &str) -> DeviceIdentity {
    let normalized = name.to_ascii_lowercase();
    if normalized.contains("aero") || normalized.starts_with("nf") {
        return DeviceIdentity {
            make: "NOSFET".to_owned(),
            model: "Aero".to_owned(),
        };
    }

    DeviceIdentity {
        make: "unknown".to_owned(),
        model: "unknown".to_owned(),
    }
}

fn format_mapped_telemetry_event(snapshot: TelemetrySnapshot) -> String {
    let mut fields = Vec::new();
    if let Some(speed) = snapshot.speed_mm_s {
        fields.push(format!("speed={}mph", mm_s_to_mph(speed.value)));
    }
    if let Some(voltage) = snapshot.voltage_mv {
        fields.push(format!("voltage={}V", millivolts_to_volts(voltage.value)));
    }
    if let Some(percent) = snapshot
        .battery_percent_reported
        .or(snapshot.battery_percent_estimated)
    {
        fields.push(format!("battery={}%", percent.value));
    }
    if let Some(current) = snapshot.battery_current_ma.or(snapshot.motor_current_ma) {
        fields.push(format!("current={}A", milliamps_to_amps(current.value)));
    }
    if let Some(temperature) = snapshot
        .controller_temperature_mc
        .or(snapshot.motor_temperature_mc)
        .or(snapshot.battery_temperature_mc)
    {
        fields.push(format!(
            "temperature={}C",
            millicelsius_to_celsius_signed(temperature.value)
        ));
    }
    if let Some(pwm) = snapshot.pwm_permille {
        fields.push(format!("pwm={}%", permille_to_percent(pwm.value)));
    }
    if let Some(distance) = snapshot.distance_mm {
        fields.push(format!("distance={}", format_distance_mm(distance.value)));
    }
    if let Some(pitch) = snapshot.pitch_mdeg {
        fields.push(format!("pitch={}deg", millidegrees_to_degrees(pitch.value)));
    }
    if let Some(roll) = snapshot.roll_mdeg {
        fields.push(format!("roll={}deg", millidegrees_to_degrees(roll.value)));
    }
    if fields.is_empty() {
        "telemetry mapped none".to_owned()
    } else {
        format!("telemetry mapped {}", fields.join(" "))
    }
}

fn format_bridge_event(event: &SessionBridgeEvent) -> (&'static str, String) {
    match event {
        SessionBridgeEvent::LinkDown { monotonic_ms } => {
            ("warn", format!("t={monotonic_ms}ms link down"))
        }
        SessionBridgeEvent::ProcessedTelemetry {
            monotonic_ms,
            delta,
        } => (
            "info",
            format!(
                "t={monotonic_ms}ms processed telemetry {}",
                format_telemetry_delta(*delta)
            ),
        ),
        SessionBridgeEvent::ReadOnlyResponse {
            monotonic_ms,
            response,
        } => (
            "info",
            format!(
                "t={monotonic_ms}ms {}",
                format_read_only_response(*response)
            ),
        ),
        SessionBridgeEvent::Diagnostics {
            monotonic_ms,
            diagnostics,
        } => (
            "warn",
            format!(
                "t={monotonic_ms}ms telemetry diagnostics {}",
                format_parser_diagnostics(*diagnostics)
            ),
        ),
        SessionBridgeEvent::DiagnosticError {
            monotonic_ms,
            error,
        } => (
            "warn",
            format!(
                "t={monotonic_ms}ms telemetry diagnostic_error {:?}",
                error.kind
            ),
        ),
        SessionBridgeEvent::NotificationIngest {
            monotonic_ms,
            outcome,
        } => format_notification_ingest_event(monotonic_ms.get(), *outcome),
    }
}

fn format_notification_ingest_event(
    monotonic_ms: u64,
    outcome: NotificationIngestOutcome,
) -> (&'static str, String) {
    match outcome {
        NotificationIngestOutcome::SemanticEvents {
            notification,
            event_count,
        } => (
            "info",
            format!(
                "t={monotonic_ms}ms protocol semantic events family={} events={} len={}",
                family_name(notification.family),
                event_count.get(),
                notification.len.get()
            ),
        ),
        NotificationIngestOutcome::BufferedFragment(notification) => (
            "trace",
            format!(
                "t={monotonic_ms}ms protocol buffered fragment family={} len={}",
                family_name(notification.family),
                notification.len.get()
            ),
        ),
        NotificationIngestOutcome::ParserDiagnostic {
            notification,
            error,
        } => (
            "warn",
            format!(
                "t={monotonic_ms}ms protocol parser diagnostic family={} len={} error={error:?}",
                family_name(notification.family),
                notification.len.get()
            ),
        ),
        NotificationIngestOutcome::KnownReserved {
            notification,
            payload,
        } => (
            "info",
            format!(
                "t={monotonic_ms}ms protocol known reserved family={} selector={} tag={} body_len={} verification={} len={}",
                family_name(notification.family),
                optional_u8(payload.selector.map(cutout_core::ProtocolSelector::get)),
                optional_u16(payload.tag.map(cutout_core::ProtocolTag::get)),
                payload.body_len.get(),
                verification_name(payload.verification),
                notification.len.get()
            ),
        ),
        NotificationIngestOutcome::ParserGap { notification, gap } => (
            "warn",
            format!(
                "t={monotonic_ms}ms protocol parser gap family={} selector={} tag={} body_len={} len={}",
                family_name(notification.family),
                optional_u8(gap.selector.map(cutout_core::ProtocolSelector::get)),
                optional_u16(gap.tag.map(cutout_core::ProtocolTag::get)),
                gap.body_len.get(),
                notification.len.get()
            ),
        ),
        NotificationIngestOutcome::Ignored(notification) => (
            "trace",
            format!(
                "t={monotonic_ms}ms protocol ignored notification family={} len={}",
                family_name(notification.family),
                notification.len.get()
            ),
        ),
    }
}

fn family_name(family: Option<ProtocolFamily>) -> &'static str {
    match family {
        Some(ProtocolFamily::VeteranLeaperkimNosfet) => "VeteranLeaperkimNosfet",
        Some(ProtocolFamily::BegodeGotway) => "BegodeGotway",
        Some(ProtocolFamily::Vesc) => "Vesc",
        None => "unknown",
    }
}

fn optional_u8(value: Option<u8>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

fn optional_u16(value: Option<u16>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

fn format_read_only_response(response: ReadOnlyResponse) -> String {
    match response {
        ReadOnlyResponse::Firmware(firmware) => {
            format!("read-only firmware {}", format_firmware_summary(firmware))
        }
        ReadOnlyResponse::Settings(settings) => {
            let mut entries = Vec::new();
            for entry in settings.entries.into_iter().flatten() {
                entries.push(format!(
                    "field={} value={} quality={} verification={}",
                    entry.field.id,
                    entry.field.value,
                    quality_name(entry.quality),
                    verification_name(entry.verification)
                ));
            }
            if entries.is_empty() {
                "read-only settings none observed".to_owned()
            } else {
                format!("read-only settings {}", entries.join(" "))
            }
        }
        ReadOnlyResponse::Battery(payload) => {
            let page = payload.page();
            let mut summary = format!(
                "read-only battery selector={} kind={} verification={}",
                page.selector,
                battery_page_kind_name(page.kind),
                verification_name(page.verification)
            );
            if let Some(temperature_summary) = bms_temperature_summary(payload) {
                summary.push_str(&temperature_summary);
            }
            if let Some(current_summary) = bms_current_summary(payload) {
                summary.push_str(&current_summary);
            }
            summary
        }
        ReadOnlyResponse::Diagnostics(diagnostics) => {
            let populated = diagnostics.details.into_iter().flatten().count();
            format!("read-only diagnostics details={populated}")
        }
        ReadOnlyResponse::RawTelemetry(raw) => {
            let populated = raw.fields.into_iter().flatten().count();
            format!("read-only raw telemetry fields={populated}")
        }
    }
}

fn report_has_no_parsed_events(report: &SessionBridgeReport) -> bool {
    report.telemetry == 0
        && report.read_only_responses == 0
        && report.diagnostics == 0
        && report.diagnostic_errors.is_empty()
        && report.read_only_response_events.is_empty()
}

fn format_telemetry_delta(delta: TelemetryDelta) -> String {
    let mut fields = Vec::new();
    if let Some(speed) = delta.speed_mm_s {
        fields.push(format!("speed={}mph", mm_s_to_mph(speed.value)));
    }
    if let Some(voltage) = delta.voltage_mv {
        fields.push(format!("voltage={}V", millivolts_to_volts(voltage.value)));
    }
    if let Some(percent) = delta
        .battery_percent_reported
        .or(delta.battery_percent_estimated)
    {
        fields.push(format!("battery={}%", percent.value));
    }
    if let Some(current) = delta.battery_current_ma.or(delta.motor_current_ma) {
        fields.push(format!("current={}A", milliamps_to_amps(current.value)));
    }
    if let Some(temperature) = delta
        .controller_temperature_mc
        .or(delta.motor_temperature_mc)
        .or(delta.battery_temperature_mc)
    {
        fields.push(format!(
            "temperature={}C",
            millicelsius_to_celsius_signed(temperature.value)
        ));
    }
    if let Some(pwm) = delta.pwm_permille {
        fields.push(format!("pwm={}%", permille_to_percent(pwm.value)));
    }
    if let Some(distance) = delta.distance_mm {
        fields.push(format!("distance={}", format_distance_mm(distance.value)));
    }
    if let Some(pitch) = delta.pitch_mdeg {
        fields.push(format!("pitch={}deg", millidegrees_to_degrees(pitch.value)));
    }
    if fields.is_empty() {
        "unmapped".to_owned()
    } else {
        fields.join(" ")
    }
}

fn format_unmapped_telemetry_event(report: &SessionBridgeReport) -> String {
    let latest = report
        .latest_notification_len
        .map_or_else(|| "none".to_owned(), |len| len.to_string());
    let diagnostics = format_parser_diagnostics(report.diagnostics_snapshot);
    format!(
        "telemetry unmapped notifications={} bytes={} latest_len={} diagnostics={} {}",
        report.notifications, report.notification_bytes, latest, report.diagnostics, diagnostics
    )
}

fn format_parser_diagnostics(diagnostics: ParserDiagnostics) -> String {
    format!(
        "dropped={} resyncs={} bad_checksums={} timeouts={} oversized={} malformed={} unmatched={}",
        diagnostics.dropped_bytes,
        diagnostics.resyncs,
        diagnostics.bad_checksums,
        diagnostics.timeouts,
        diagnostics.oversized_frames,
        diagnostics.malformed_frames,
        diagnostics.unmatched_replies
    )
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

pub(crate) fn run_dashboard_with_updates(
    state: DashboardState,
    updates: &mpsc::Receiver<DashboardUpdate>,
) -> Result<()> {
    run_dashboard_loop(state, Some(updates), dashboard_termination_requested())
}

fn run_dashboard_loop(
    mut state: DashboardState,
    updates: Option<&mpsc::Receiver<DashboardUpdate>>,
    termination_requested: &AtomicBool,
) -> Result<()> {
    let (tx, rx) = mpsc::channel::<DashboardInput>();
    let _input_thread = spawn_input_thread(tx);

    let mut terminal = init_dashboard_terminal()?;
    let mut last_tick = Instant::now();

    let result = 'dashboard: loop {
        if dashboard_should_exit(termination_requested) {
            break 'dashboard Ok(());
        }

        drain_dashboard_updates(&mut state, updates);

        if let Err(error) = terminal.draw(|frame| {
            frame.render_widget(Clear, frame.area());
            render_dashboard(frame, &state);
        }) {
            break 'dashboard Err(error.into());
        }

        while let Ok(input) = rx.try_recv() {
            match input {
                DashboardInput::Quit => break 'dashboard Ok(()),
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
    };

    let restore_result = restore_dashboard_terminal(&mut terminal);
    drop(terminal);
    ratatui::restore();
    restore_result?;
    result
}

type DashboardTerminal = Terminal<TerminaBackend<PlatformTerminal>>;

/// Installs signal flags that let the dashboard leave raw/alternate-screen mode on reload.
///
/// # Errors
///
/// Returns an error when process signal registration fails.
pub fn install_dashboard_signal_restore() -> Result<()> {
    let flag = dashboard_termination_requested().clone();
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(signal, flag.clone())?;
    }
    Ok(())
}

fn dashboard_termination_requested() -> &'static Arc<AtomicBool> {
    static TERMINATION_REQUESTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    TERMINATION_REQUESTED.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

fn dashboard_should_exit(termination_requested: &AtomicBool) -> bool {
    termination_requested.load(Ordering::Relaxed)
}

fn init_dashboard_terminal() -> Result<DashboardTerminal> {
    init_dashboard_terminal_inner().inspect_err(|_error| {
        ratatui::restore();
    })
}

fn init_dashboard_terminal_inner() -> Result<DashboardTerminal> {
    let mut output = PlatformTerminal::new()?;
    output.enter_raw_mode()?;
    write!(
        output,
        "{}{}",
        decset(csi::DecPrivateModeCode::ClearAndEnableAlternateScreen),
        decreset(csi::DecPrivateModeCode::ShowCursor)
    )?;
    output.flush()?;

    let backend = TerminaBackend::new(output);
    Ok(Terminal::new(backend)?)
}

fn restore_dashboard_terminal(terminal: &mut DashboardTerminal) -> Result<()> {
    let backend = terminal.backend_mut();
    write!(
        backend,
        "{}{}",
        decreset(csi::DecPrivateModeCode::ClearAndEnableAlternateScreen),
        decset(csi::DecPrivateModeCode::ShowCursor)
    )?;
    backend.flush()?;
    Ok(())
}

fn decset(code: csi::DecPrivateModeCode) -> csi::Csi {
    csi::Csi::Mode(csi::Mode::SetDecPrivateMode(csi::DecPrivateMode::Code(
        code,
    )))
}

fn decreset(code: csi::DecPrivateModeCode) -> csi::Csi {
    csi::Csi::Mode(csi::Mode::ResetDecPrivateMode(csi::DecPrivateMode::Code(
        code,
    )))
}

fn drain_dashboard_updates(
    state: &mut DashboardState,
    updates: Option<&mpsc::Receiver<DashboardUpdate>>,
) {
    let Some(updates) = updates else {
        return;
    };

    while let Ok(update) = updates.try_recv() {
        state.apply_update(update);
    }
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
    let active_tab = state.active_tab.min(TAB_COUNT - 1);
    let areas = if active_tab == 3 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Fill(1)])
            .split(frame.area())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(22),
                Constraint::Fill(1),
            ])
            .split(frame.area())
    };

    render_header(frame, areas[0], state);
    if active_tab == 3 {
        render_logs(frame, areas[1], state);
    } else {
        render_body(frame, areas[1], state);
        render_logs(frame, areas[2], state);
    }
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
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .split(area);

    let device = &state.device;
    let summary = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("make ", Style::new().fg(Color::Gray)),
            Span::raw(device.make.as_str()),
        ]),
        Line::from(vec![
            Span::styled("model ", Style::new().fg(Color::Gray)),
            Span::raw(device.model.as_str()),
        ]),
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
            Span::raw(state.provenance.as_deref().unwrap_or("live")),
        ]),
    ])
    .block(panel_block("Target"));
    frame.render_widget(summary, chunks[0]);

    let gauges = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    render_battery_cluster(frame, gauges[0], state);

    render_signal_gauge(frame, gauges[1], state);
    render_profiles(frame, chunks[2], state);
}

fn render_profiles(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_profile_table(frame, chunks[0], state);
    render_read_only_summary(frame, chunks[1], state);
}

fn render_profile_table(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
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

fn render_battery_cluster(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_battery_gauge(
        frame,
        chunks[0],
        state.telemetry.battery_pct,
        state.telemetry.battery_source,
        state.telemetry.latest_voltage_v,
    );
    render_voltage_sparkline(frame, chunks[1], state);
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

fn render_signal_gauge(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let rssi = state
        .scan_browser
        .selected()
        .map_or("unknown", |observation| observation.rssi.as_str());
    let signal = Gauge::default()
        .block(Block::bordered().title("Signal"))
        .gauge_style(Style::new().fg(Color::Cyan).bg(Color::Black))
        .label(format!("{}% / {rssi}", state.telemetry.signal_pct))
        .ratio(percent_ratio(state.telemetry.signal_pct));
    frame.render_widget(signal, area);
}

fn rssi_to_signal_percent(rssi_dbm: i16) -> u64 {
    let clamped = rssi_dbm.clamp(-100, -40);
    u64::try_from(i32::from(clamped) + 100)
        .unwrap_or(0)
        .saturating_mul(100)
        / 60
}

fn render_voltage_sparkline(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let title = voltage_sparkline_title(state);
    let (data, len, max) = voltage_sparkline_data(state);
    let spark = Sparkline::default()
        .block(panel_block(&title))
        .style(Style::new().fg(Color::Magenta))
        .data(&data[..len])
        .max(max);
    frame.render_widget(spark, area);
}

fn voltage_sparkline_title(state: &DashboardState) -> String {
    let voltage = state
        .telemetry
        .latest_voltage_v
        .map_or_else(|| "unknown".to_owned(), |voltage| format!("{voltage} V"));
    state.telemetry.battery_pct.map_or_else(
        || format!("Voltage {voltage}"),
        |percent| format!("Voltage {voltage} / {percent}%"),
    )
}

fn voltage_sparkline_max_v(state: &DashboardState) -> u64 {
    dashboard_voltage_range_mv(state)
        .map_or(100, |(_min_mv, max_mv)| millivolts_to_volts(max_mv))
        .max(state.telemetry.latest_voltage_v.unwrap_or(0))
        .max(1)
}

fn voltage_sparkline_data(state: &DashboardState) -> ([u64; HISTORY_LIMIT], usize, u64) {
    let mut data = [0; HISTORY_LIMIT];
    let len = state.telemetry.voltage_v.len().min(HISTORY_LIMIT);

    if let Some((min_mv, max_mv)) = dashboard_voltage_range_mv(state) {
        for (slot, voltage_v) in data.iter_mut().zip(state.telemetry.voltage_v.iter()) {
            *slot = voltage_range_percent(*voltage_v, min_mv, max_mv);
        }
        return (data, len, 100);
    }

    for (slot, voltage_v) in data.iter_mut().zip(state.telemetry.voltage_v.iter()) {
        *slot = *voltage_v;
    }
    (data, len, voltage_sparkline_max_v(state))
}

fn voltage_range_percent(sample_v: u64, min_mv: i32, max_mv: i32) -> u64 {
    let voltage_mv = i64::try_from(sample_v)
        .unwrap_or(i64::MAX / 1_000)
        .saturating_mul(1_000);
    let min_mv = i64::from(min_mv);
    let max_mv = i64::from(max_mv);
    if max_mv <= min_mv || voltage_mv <= min_mv {
        return 0;
    }
    if voltage_mv >= max_mv {
        return 100;
    }

    u64::try_from(((voltage_mv - min_mv) * 100 + (max_mv - min_mv) / 2) / (max_mv - min_mv))
        .unwrap_or(100)
}

fn dashboard_voltage_range_mv(state: &DashboardState) -> Option<(i32, i32)> {
    if state.device.make == "NOSFET"
        && state.device.model == "Aero"
        && let Some(profile) = VeteranModelProfile::from_model_id(43)
    {
        return Some((
            *profile.voltage_range_mv.start(),
            *profile.voltage_range_mv.end(),
        ));
    }

    None
}

fn render_telemetry(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    if state.telemetry.has_decoded_samples() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Length(7),
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
        render_telemetry_trend(frame, chunks[3], state);
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

fn render_read_only_summary(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let read_only = &state.read_only;
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("firmware ", Style::new().fg(Color::Gray)),
        Span::raw(read_only.firmware.as_deref().unwrap_or("unknown")),
    ]));

    if let Some(temperature) = read_only.latest_bms_temperature.as_deref() {
        lines.push(Line::from(vec![
            Span::styled("bms temp ", Style::new().fg(Color::Gray)),
            Span::raw(temperature),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("raw/unverified pages ", Style::new().fg(Color::Gray)),
        Span::raw(read_only.unknown_raw_pages.to_string()),
        Span::styled(" diagnostic responses ", Style::new().fg(Color::Gray)),
        Span::raw(read_only.diagnostics.to_string()),
        Span::styled(" raw telemetry ", Style::new().fg(Color::Gray)),
        Span::raw(read_only.raw_telemetry.to_string()),
    ]));

    lines.push(Line::from(vec![Span::styled(
        "settings",
        Style::new().fg(Color::Gray),
    )]));
    if read_only.settings.is_empty() {
        lines.push(Line::from(vec![Span::raw("none observed")]));
    } else {
        for setting in read_only.settings.iter().rev().take(4) {
            lines.push(Line::from(vec![Span::raw(setting.as_str())]));
        }
    }

    lines.push(Line::from(vec![Span::styled(
        "bms pages",
        Style::new().fg(Color::Gray),
    )]));
    for page in read_only.bms_pages.iter().rev().take(4) {
        lines.push(Line::from(vec![Span::raw(page.as_str())]));
    }

    let panel = Paragraph::new(lines).block(panel_block("Read-only responses"));
    frame.render_widget(panel, area);
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
            Span::styled("transport notifications ", Style::new().fg(Color::Gray)),
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
    .block(panel_block("Decoder input"));
    frame.render_widget(text, area);
}

fn render_pending_telemetry_wait(frame: &mut Frame<'_>, area: Rect) {
    let text = Paragraph::new(vec![
        Line::from("waiting for protocol decoder output"),
        Line::from("transport notifications are arriving from the connected device"),
        Line::from("decoded speed, voltage, current, and temperature will fill in here"),
    ])
    .block(panel_block("Decoder"));
    frame.render_widget(text, area);
}

fn render_session_summary(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let counters = &state.counters;
    let mut lines = vec![
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

    if state.telemetry.has_decoded_samples() {
        lines.extend([
            Line::from(vec![
                Span::styled("speed ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_u64(
                    state.telemetry.latest_speed_mph,
                    " mph",
                )),
                Span::styled(" voltage ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_u64(state.telemetry.latest_voltage_v, " V")),
                Span::styled(" battery ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_u64(state.telemetry.battery_pct, "%")),
            ]),
            Line::from(vec![
                Span::styled("current ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_i64(state.telemetry.latest_current_a, " A")),
                Span::styled(" temp ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_i64(
                    state.telemetry.latest_temperature_c,
                    " C",
                )),
                Span::styled(" pwm ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_i64(state.telemetry.latest_pwm_pct, "%")),
            ]),
            Line::from(vec![
                Span::styled("distance ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_distance_m(
                    state.telemetry.latest_distance_m,
                )),
                Span::styled(" pitch ", Style::new().fg(Color::Gray)),
                Span::raw(format_optional_i64(
                    state.telemetry.latest_pitch_deg,
                    " deg",
                )),
            ]),
        ]);
    }

    let panel = Paragraph::new(lines).block(panel_block("Session / telemetry"));
    frame.render_widget(panel, area);
}

fn format_optional_u64(value: Option<u64>, suffix: &str) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| format!("{value}{suffix}"))
}

fn format_optional_i64(value: Option<i64>, suffix: &str) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| format!("{value}{suffix}"))
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
    let visible_lines = usize::from(area.height.saturating_sub(2));
    let lines = state
        .logs
        .iter()
        .rev()
        .take(visible_lines)
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
    use cutout_core::{
        BatteryInfo, BatteryPageMetadata, BatteryPagePayload, DiagnosticReadback, FirmwareInfo,
        GattChannel, Measured, NotificationByteLen, NotificationIngestOutcome, ParserError,
        ParserGapEvidence, PayloadBodyLen, ProtocolFamily, ProtocolSelector, RawFieldValue,
        RawTelemetryReadback, ReadOnlyResponse, ReservedPayloadEvidence, SettingsEntry,
        SettingsReadback, TelemetrySnapshot, ValueQuality, ValueSource, VerificationStatus,
    };
    use cutout_protocols::{VeteranFrame, VeteranTelemetry};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn live_aero_telemetry_snapshot() -> TelemetrySnapshot {
        let frame = VeteranFrame::try_from_slice(&hex_literal::hex!(
            "dc5a5c532a7c000000000000ab41001700000cff\
             000000000226021ca8f607801afa000080c80000\
             808080808080022880803080800e310e310e2f0e\
             2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e\
             310e2e9e05e3ad"
        ))
        .expect("fixture frame is valid");
        let delta = VeteranTelemetry::decode(&frame)
            .expect("fixture telemetry decodes")
            .to_delta(42);
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(delta);
        snapshot
    }

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

    fn find_text_position(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
        for y in 0..buffer.area.height {
            let mut row = String::new();
            for x in 0..buffer.area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            if let Some(x) = row.find(needle) {
                return Some((u16::try_from(x).expect("x position fits u16"), y));
            }
        }
        None
    }

    fn empty_session_bridge_report() -> SessionBridgeReport {
        SessionBridgeReport::default()
    }

    #[test]
    fn sample_state_has_device_profiles_and_logs() {
        let state = DashboardState::sample();

        assert_eq!(state.source, DashboardSource::Demo);
        assert_eq!(state.active_tab, 0);
        assert_eq!(
            state.provenance.as_deref(),
            Some("demo state: aero-nf2557.v1")
        );
        assert_eq!(state.device.make, "NOSFET");
        assert_eq!(state.device.model, "Aero");
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
        assert_eq!(state.device.make, "NOSFET");
        assert_eq!(state.device.model, "Aero");
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
                advertised_services: vec![].into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };

        let state = DashboardState::live_connected(&target, &summary);

        assert_eq!(state.source, DashboardSource::Live);
        assert_eq!(state.device.make, "NOSFET");
        assert_eq!(state.device.model, "Aero");
        assert_eq!(state.device.name, "Aero NF2557");
        assert_eq!(state.device.address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(state.device.connection_state, "connected");
        assert_eq!(state.counters.connected, 1);
        assert_eq!(state.telemetry.battery_pct, None);
        assert_eq!(state.telemetry.battery_source, BatterySource::Unknown);
        assert_eq!(state.telemetry.signal_pct, 65);
        assert_eq!(state.scan_browser.observations.len(), 1);
        assert!(state.scan_browser.observations[0].real_device);
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.profiles[0].source, "gatt");

        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("65% / -61 dBm"));
        assert_eq!(dashboard_voltage_range_mv(&state), Some((91_000, 126_000)));
    }

    #[test]
    fn rssi_signal_percent_clamps_to_reasonable_ble_range() {
        assert_eq!(rssi_to_signal_percent(-40), 100);
        assert_eq!(rssi_to_signal_percent(-61), 65);
        assert_eq!(rssi_to_signal_percent(-74), 43);
        assert_eq!(rssi_to_signal_percent(-100), 0);
        assert_eq!(rssi_to_signal_percent(-120), 0);
        assert_eq!(rssi_to_signal_percent(-20), 100);
    }

    #[test]
    fn dashboard_terminal_modes_enter_and_restore_symmetrically() {
        assert_eq!(
            decset(csi::DecPrivateModeCode::ClearAndEnableAlternateScreen).to_string(),
            "\u{1b}[?1049h"
        );
        assert_eq!(
            decreset(csi::DecPrivateModeCode::ClearAndEnableAlternateScreen).to_string(),
            "\u{1b}[?1049l"
        );
        assert_eq!(
            decreset(csi::DecPrivateModeCode::ShowCursor).to_string(),
            "\u{1b}[?25l"
        );
        assert_eq!(
            decset(csi::DecPrivateModeCode::ShowCursor).to_string(),
            "\u{1b}[?25h"
        );
    }

    #[test]
    fn dashboard_loop_observes_termination_signal_flag() {
        let termination_requested = AtomicBool::new(false);

        assert!(!dashboard_should_exit(&termination_requested));

        termination_requested.store(true, Ordering::Relaxed);

        assert!(dashboard_should_exit(&termination_requested));
    }

    #[test]
    fn distance_formatter_uses_odometer_scale_for_large_distances() {
        assert_eq!(format_distance_m(999), "999 m");
        assert_eq!(format_distance_mm(1_551_169_000), "1551.2 km");
        assert_eq!(format_optional_distance_m(Some(1_551_169)), "1551.2 km");
        assert_eq!(format_distance_m(1_550_438), "1550.4 km");
        assert_eq!(format_optional_distance_m(None), "unknown");
    }

    #[test]
    fn voltage_sparkline_uses_connected_device_voltage_range() {
        let mut state = DashboardState::empty();
        state.device.make = "NOSFET".to_owned();
        state.device.model = "Aero".to_owned();
        state.telemetry.latest_voltage_v = Some(120);
        state.telemetry.voltage_v = vec![109, 120, 126];
        state.telemetry.battery_pct = Some(85);

        assert_eq!(dashboard_voltage_range_mv(&state), Some((91_000, 126_000)));
        assert_eq!(voltage_sparkline_max_v(&state), 126);
        assert_eq!(voltage_sparkline_title(&state), "Voltage 120 V / 85%");

        let (data, len, max) = voltage_sparkline_data(&state);
        assert_eq!(len, 3);
        assert_eq!(max, 100);
        assert_eq!(&data[..len], [51, 83, 100]);
    }

    #[test]
    fn voltage_sparkline_falls_back_to_observed_voltage_for_unknown_device() {
        let mut state = DashboardState::empty();
        state.telemetry.latest_voltage_v = Some(151);
        state.telemetry.voltage_v = vec![149, 151];

        assert_eq!(dashboard_voltage_range_mv(&state), None);
        assert_eq!(voltage_sparkline_max_v(&state), 151);
        assert_eq!(voltage_sparkline_title(&state), "Voltage 151 V");

        let (data, len, max) = voltage_sparkline_data(&state);
        assert_eq!(len, 2);
        assert_eq!(max, 151);
        assert_eq!(&data[..len], [149, 151]);
    }

    #[test]
    fn live_session_report_updates_real_counters_and_logs() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: 0,
            writes: 0,
            subscribes: 1,
            notifications: 184,
            notification_bytes: 18_400,
            latest_notification_len: Some(100),
            telemetry: 0,
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: 0,
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: 1,
            diagnostics_snapshot: ParserDiagnostics {
                malformed_frames: 2,
                unmatched_replies: 1,
                ..ParserDiagnostics::default()
            },
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![
                SessionBridgeEvent::Diagnostics {
                    monotonic_ms: cutout_btle::MonotonicMs::new(18),
                    diagnostics: ParserDiagnostics {
                        malformed_frames: 2,
                        unmatched_replies: 1,
                        ..ParserDiagnostics::default()
                    },
                },
                SessionBridgeEvent::LinkDown {
                    monotonic_ms: cutout_btle::MonotonicMs::new(19),
                },
            ],
            disconnects: 1,
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
        assert!(state.logs.iter().all(|entry| {
            !entry
                .message
                .contains("telemetry unmapped notifications=184")
        }));
        assert!(state.logs.iter().any(|entry| {
            entry.level == "warn"
                && entry.message.contains("t=18ms telemetry diagnostics")
                && entry.message.contains("malformed=2")
        }));
        assert!(
            state.logs.iter().any(|entry| {
                entry.level == "warn" && entry.message.contains("t=19ms link down")
            })
        );
    }

    #[test]
    fn live_session_report_applies_telemetry_snapshot() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: 0,
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
            read_only_responses: 0,
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: 0,
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(42),
                delta: TelemetryDelta {
                    speed_mm_s: Some(Measured::reported(4_470)),
                    voltage_mv: Some(Measured::reported(84_400)),
                    battery_current_ma: Some(Measured::reported(-12_400)),
                    controller_temperature_mc: Some(Measured::reported(36_600)),
                    battery_percent_reported: Some(Measured::reported(77)),
                    ..TelemetryDelta::empty(42)
                },
            }],
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.telemetry.battery_pct, Some(77));
        assert_eq!(
            state.telemetry.battery_source,
            BatterySource::TelemetryReported
        );
        assert_eq!(state.telemetry.latest_speed_mph, Some(10));
        assert_eq!(state.telemetry.latest_voltage_v, Some(84));
        assert_eq!(state.telemetry.latest_current_a, Some(-12));
        assert_eq!(state.telemetry.latest_temperature_c, Some(36));
        assert_eq!(state.telemetry.speed_mph, vec![10]);
        assert_eq!(state.telemetry.voltage_v.len(), HISTORY_LIMIT);
        assert!(
            state
                .telemetry
                .voltage_v
                .iter()
                .all(|voltage| *voltage == 84)
        );
        assert_eq!(state.telemetry.current_a, vec![12]);
        assert_eq!(state.telemetry.temperature_c, vec![37]);
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry.message.contains("telemetry mapped")
                && entry.message.contains("speed=10mph")
                && entry.message.contains("battery=77%")
                && entry.message.contains("current=-12A")
        }));
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry.message.contains("t=42ms processed telemetry")
                && entry.message.contains("voltage=84V")
        }));
    }

    #[test]
    fn parsed_session_report_suppresses_raw_notification_log_spam() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            notifications: 1,
            notification_bytes: 99,
            latest_notification_len: Some(99),
            telemetry: 1,
            telemetry_snapshot: TelemetrySnapshot {
                voltage_mv: Some(Measured::reported(117_600)),
                battery_percent_estimated: Some(Measured::estimated(78)),
                ..TelemetrySnapshot::default()
            },
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                delta: TelemetryDelta {
                    voltage_mv: Some(Measured::reported(117_600)),
                    battery_percent_estimated: Some(Measured::estimated(78)),
                    ..TelemetryDelta::empty(7)
                },
            }],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);

        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry
                    .message
                    .contains("processed telemetry voltage=118V battery=78%")
        }));
        assert!(
            state
                .logs
                .iter()
                .all(|entry| !entry.message.contains("raw notification len=99"))
        );
        assert!(
            state
                .logs
                .iter()
                .all(|entry| !entry.message.contains("telemetry unmapped notifications=1"))
        );

        state.active_tab = 3;
        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("processed telemetry voltage=118V battery=78%"));
        assert!(!text.contains("raw notification len=99"));
        assert!(!text.contains("telemetry unmapped notifications=1"));
    }

    #[test]
    fn unparsed_session_report_summarizes_transport_without_raw_event_spam() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            notifications: 3,
            notification_bytes: 57,
            latest_notification_len: Some(20),
            events: Vec::new(),
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);

        assert!(state.logs.iter().any(|entry| {
            entry.level == "trace"
                && entry.message.contains("telemetry unmapped notifications=3")
                && entry.message.contains("bytes=57")
                && entry.message.contains("latest_len=20")
        }));
        assert!(
            state
                .logs
                .iter()
                .all(|entry| !entry.message.contains("raw notification"))
        );

        state.active_tab = 3;
        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("telemetry unmapped notifications=3"));
        assert!(!text.contains("raw notification"));
    }

    #[test]
    fn ingest_outcome_events_render_each_typed_protocol_category() {
        let mut state = DashboardState::empty();
        let channel = GattChannel::from_bytes([0xA1; 16]);
        let report = SessionBridgeReport {
            notifications: 5,
            notification_bytes: 269,
            latest_notification_len: Some(77),
            events: vec![
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(3),
                    outcome: NotificationIngestOutcome::buffered_fragment(
                        ProtocolFamily::VeteranLeaperkimNosfet,
                        channel,
                        NotificationByteLen::new(20),
                        3,
                    ),
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(4),
                    outcome: NotificationIngestOutcome::known_reserved(
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
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(5),
                    outcome: NotificationIngestOutcome::parser_gap(
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
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(6),
                    outcome: NotificationIngestOutcome::parser_diagnostic(
                        ProtocolFamily::VeteranLeaperkimNosfet,
                        channel,
                        NotificationByteLen::new(77),
                        6,
                        ParserError::BadChecksum,
                    ),
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(7),
                    outcome: NotificationIngestOutcome::ignored_wrong_channel(
                        channel,
                        NotificationByteLen::new(20),
                        7,
                    ),
                },
            ],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = 3;

        let text = buffer_text(&render_buffer(&state, 140, 36));

        assert!(text.contains("protocol buffered fragment"));
        assert!(text.contains("protocol known reserved"));
        assert!(text.contains("selector=8"));
        assert!(text.contains("verification=hardware_verified"));
        assert!(text.contains("protocol parser gap"));
        assert!(text.contains("selector=9"));
        assert!(text.contains("protocol parser diagnostic"));
        assert!(text.contains("error=BadChecksum"));
        assert!(text.contains("protocol ignored notification"));
        assert!(!text.contains("raw notification"));
    }

    #[test]
    fn read_only_response_events_render_as_parsed_aero_events() {
        let mut state = DashboardState::empty();
        let read_only_response = ReadOnlyResponse::Battery(BatteryPagePayload::temperature_values(
            BatteryPageMetadata::temperature(3, VerificationStatus::HardwareVerified),
            BatteryInfo {
                temperature_mc: Some(Measured::reported(17_600)),
                ..BatteryInfo::default()
            },
            [
                Some(Measured::reported(17_600)),
                Some(Measured::reported(17_100)),
                Some(Measured::reported(17_700)),
                Some(Measured::reported(18_500)),
                Some(Measured::reported(19_000)),
                Some(Measured::reported(19_100)),
            ],
        ));
        let report = SessionBridgeReport {
            read_only_responses: 1,
            read_only_response_events: vec![read_only_response],
            events: vec![SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                response: read_only_response,
            }],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = 3;

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry
                    .message
                    .contains("read-only battery selector=3 kind=temperature")
                && entry.message.contains("temps_c=17,17,17,18,19,19")
        }));
        assert!(
            state
                .logs
                .iter()
                .all(|entry| !entry.message.contains("telemetry unmapped"))
        );
        assert!(text.contains("read-only battery selector=3 kind=temperature"));
    }

    #[test]
    fn read_only_metadata_current_renders_as_parsed_aero_event() {
        let mut state = DashboardState::empty();
        let read_only_response = ReadOnlyResponse::Battery(BatteryPagePayload::raw(
            BatteryPageMetadata::metadata(0, VerificationStatus::HardwareVerified),
            BatteryInfo {
                current_ma: Some(Measured::reported(2_010)),
                ..BatteryInfo::default()
            },
        ));
        let report = SessionBridgeReport {
            read_only_responses: 1,
            read_only_response_events: vec![read_only_response],
            events: vec![SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                response: read_only_response,
            }],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = 3;

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry
                    .message
                    .contains("read-only battery selector=0 kind=metadata")
                && entry.message.contains("current=2A")
        }));
        assert!(text.contains("current=2A"));
    }

    #[test]
    fn first_voltage_sample_seeds_the_sparkline_history() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: 0,
            writes: 0,
            subscribes: 1,
            notifications: 1,
            notification_bytes: 20,
            latest_notification_len: Some(20),
            telemetry: 1,
            telemetry_snapshot: live_aero_telemetry_snapshot(),
            read_only_responses: 0,
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: 0,
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                delta: TelemetryDelta {
                    voltage_mv: Some(Measured::reported(108_760)),
                    battery_percent_estimated: Some(Measured::estimated(47)),
                    ..TelemetryDelta::empty(7)
                },
            }],
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.telemetry.latest_voltage_v, Some(109));
        assert_eq!(state.telemetry.voltage_v.len(), HISTORY_LIMIT);
        assert!(
            state
                .telemetry
                .voltage_v
                .iter()
                .all(|voltage| *voltage == 109)
        );
    }

    #[test]
    fn live_session_report_summarizes_read_only_responses() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: 0,
            writes: 0,
            subscribes: 1,
            notifications: 3,
            notification_bytes: 300,
            latest_notification_len: Some(100),
            telemetry: 0,
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: 5,
            read_only_response_events: vec![
                ReadOnlyResponse::Firmware(FirmwareInfo {
                    firmware_major: Some(Measured::reported(43)),
                    firmware_minor: Some(Measured::reported(2)),
                    firmware_patch: Some(Measured::reported(54)),
                    ..FirmwareInfo::default()
                }),
                ReadOnlyResponse::Settings(SettingsReadback {
                    entries: [
                        Some(SettingsEntry {
                            field: RawFieldValue::new(0x20, 540),
                            source: ValueSource::Reported,
                            quality: ValueQuality::Known,
                            verification: VerificationStatus::HardwareVerified,
                        }),
                        None,
                        None,
                        None,
                    ],
                }),
                ReadOnlyResponse::Battery(BatteryPagePayload::cell_voltage(
                    BatteryPageMetadata::cell_voltage(2, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::temperature_values(
                    BatteryPageMetadata::temperature(3, VerificationStatus::HardwareVerified),
                    BatteryInfo {
                        temperature_mc: Some(Measured::reported(16_730)),
                        ..BatteryInfo::default()
                    },
                    [
                        Some(Measured::reported(16_730)),
                        Some(Measured::reported(17_840)),
                        Some(Measured::reported(18_100)),
                        Some(Measured::reported(17_800)),
                        Some(Measured::reported(17_700)),
                        Some(Measured::reported(19_100)),
                    ],
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::raw(8, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::RawTelemetry(RawTelemetryReadback::default()),
            ],
            firmware: None,
            settings: Vec::new(),
            diagnostics: 0,
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: Vec::new(),
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.read_only.firmware.as_deref(), Some("43.2.54"));
        assert_eq!(state.read_only.settings.len(), 1);
        assert!(
            state.read_only.settings[0].contains("field=32")
                && state.read_only.settings[0].contains("value=540")
                && state.read_only.settings[0].contains("hardware_verified")
        );
        assert_eq!(state.read_only.bms_pages.len(), 3);
        assert!(state.read_only.bms_pages[0].contains("selector=2 kind=cell_voltage"));
        assert!(state.read_only.bms_pages[1].contains(
            "selector=3 kind=temperature verification=hardware_verified temps_c=16,17,18,17,17,19"
        ));
        assert!(state.read_only.bms_pages[2].contains("selector=8 kind=raw"));
        assert_eq!(
            state.read_only.latest_bms_temperature.as_deref(),
            Some("selector=3 verification=hardware_verified temps_c=16,17,18,17,17,19")
        );
        assert_eq!(state.read_only.unknown_raw_pages, 1);
        assert_eq!(state.read_only.raw_telemetry, 1);
        assert!(
            state
                .logs
                .iter()
                .any(|entry| { entry.level == "info" && entry.message == "read-only responses=5" })
        );
    }

    #[test]
    fn read_only_session_report_does_not_warn_about_missing_telemetry_samples() {
        let mut state = DashboardState::empty();
        let read_only_response = ReadOnlyResponse::Battery(BatteryPagePayload::raw(
            BatteryPageMetadata::raw(8, VerificationStatus::HardwareVerified),
            BatteryInfo::default(),
        ));
        let report = SessionBridgeReport {
            telemetry: 0,
            read_only_responses: 1,
            read_only_response_events: vec![read_only_response],
            events: vec![SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                response: read_only_response,
            }],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);

        assert!(state.logs.iter().all(|entry| entry.message
            != "notifications received but telemetry decoder produced no samples"));
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info" && entry.message.contains("read-only battery selector=8")
        }));
    }

    #[test]
    fn live_session_report_wires_complete_aero_dashboard_state() {
        let mut state = DashboardState::empty();
        let telemetry = TelemetryDelta {
            speed_mm_s: Some(Measured::reported(4_470)),
            voltage_mv: Some(Measured::reported(108_760)),
            battery_percent_estimated: Some(Measured::estimated(47)),
            ..TelemetryDelta::empty(42)
        };
        let report = SessionBridgeReport {
            protocol_writes: 0,
            writes: 0,
            subscribes: 1,
            notifications: 4,
            notification_bytes: 400,
            latest_notification_len: Some(100),
            telemetry: 1,
            telemetry_snapshot: {
                let mut snapshot = TelemetrySnapshot::default();
                snapshot.apply_delta(telemetry);
                snapshot
            },
            read_only_responses: 5,
            read_only_response_events: vec![
                ReadOnlyResponse::Firmware(FirmwareInfo {
                    firmware_major: Some(Measured::reported(43)),
                    firmware_minor: Some(Measured::reported(2)),
                    firmware_patch: Some(Measured::reported(54)),
                    ..FirmwareInfo::default()
                }),
                ReadOnlyResponse::Settings(SettingsReadback {
                    entries: [
                        Some(SettingsEntry {
                            field: RawFieldValue::new(0x24, 1_920),
                            source: ValueSource::Reported,
                            quality: ValueQuality::Known,
                            verification: VerificationStatus::HardwareVerified,
                        }),
                        None,
                        None,
                        None,
                    ],
                }),
                ReadOnlyResponse::Battery(BatteryPagePayload::cell_voltage(
                    BatteryPageMetadata::cell_voltage(2, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::raw(8, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Diagnostics(DiagnosticReadback::default()),
            ],
            firmware: None,
            settings: Vec::new(),
            diagnostics: 1,
            diagnostics_snapshot: ParserDiagnostics {
                malformed_frames: 1,
                ..ParserDiagnostics::default()
            },
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![
                SessionBridgeEvent::ProcessedTelemetry {
                    monotonic_ms: cutout_btle::MonotonicMs::new(42),
                    delta: telemetry,
                },
                SessionBridgeEvent::Diagnostics {
                    monotonic_ms: cutout_btle::MonotonicMs::new(43),
                    diagnostics: ParserDiagnostics {
                        malformed_frames: 1,
                        ..ParserDiagnostics::default()
                    },
                },
            ],
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.counters.subscriptions, 1);
        assert_eq!(state.counters.notifications, 4);
        assert_eq!(state.telemetry.latest_speed_mph, Some(10));
        assert_eq!(state.telemetry.latest_voltage_v, Some(109));
        assert_eq!(state.telemetry.battery_pct, Some(47));
        assert_eq!(state.read_only.firmware.as_deref(), Some("43.2.54"));
        assert_eq!(state.read_only.settings.len(), 1);
        assert_eq!(state.read_only.bms_pages.len(), 2);
        assert_eq!(state.read_only.unknown_raw_pages, 1);
        assert_eq!(state.read_only.diagnostics, 1);
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info" && entry.message.contains("read-only responses=5")
        }));
        assert!(state.logs.iter().any(|entry| {
            entry.level == "warn"
                && entry.message.contains("t=43ms telemetry diagnostics")
                && entry.message.contains("malformed=1")
        }));
    }

    #[test]
    fn live_session_report_bounds_read_only_summaries() {
        let mut state = DashboardState::empty();
        let pages: Vec<_> = (0_u8..20)
            .map(|selector| {
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::raw(selector, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                ))
            })
            .collect();
        let report = SessionBridgeReport {
            protocol_writes: 0,
            writes: 0,
            subscribes: 0,
            notifications: 0,
            notification_bytes: 0,
            latest_notification_len: None,
            telemetry: 0,
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: pages.len(),
            read_only_response_events: pages,
            firmware: None,
            settings: Vec::new(),
            diagnostics: 0,
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: Vec::new(),
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.read_only.bms_pages.len(), READ_ONLY_SUMMARY_LIMIT);
        assert!(state.read_only.bms_pages[0].contains("selector=4"));
        assert!(state.read_only.bms_pages[READ_ONLY_SUMMARY_LIMIT - 1].contains("selector=19"));
        assert_eq!(state.read_only.unknown_raw_pages, 20);
    }

    #[test]
    fn read_only_temperature_summary_survives_later_bms_pages() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            read_only_responses: 4,
            read_only_response_events: vec![
                ReadOnlyResponse::Battery(BatteryPagePayload::temperature_values(
                    BatteryPageMetadata::temperature(3, VerificationStatus::HardwareVerified),
                    BatteryInfo {
                        temperature_mc: Some(Measured::reported(17_600)),
                        ..BatteryInfo::default()
                    },
                    [
                        Some(Measured::reported(17_600)),
                        Some(Measured::reported(17_100)),
                        Some(Measured::reported(17_700)),
                        Some(Measured::reported(18_500)),
                        Some(Measured::reported(19_000)),
                        Some(Measured::reported(19_100)),
                    ],
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::raw(8, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::metadata(0, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::cell_voltage(
                    BatteryPageMetadata::cell_voltage(2, VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
            ],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = 2;
        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert_eq!(
            state.read_only.latest_bms_temperature.as_deref(),
            Some("selector=3 verification=hardware_verified temps_c=17,17,17,18,19,19")
        );
        assert!(text.contains("bms temp"));
        assert!(text.contains("temps_c=17,17,17,18,19,19"));
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
    fn live_dashboard_update_applies_battery_and_log_events() {
        let mut state = DashboardState::live_target("NF2557".to_owned());

        state.apply_update(DashboardUpdate::BatteryPercent(45));
        state.apply_update(DashboardUpdate::Log {
            level: "info".to_owned(),
            message: "live dashboard update received".to_owned(),
        });

        assert_eq!(state.telemetry.battery_pct, Some(45));
        assert_eq!(state.telemetry.battery_source, BatterySource::StandardBle);
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info" && entry.message == "live dashboard update received"
        }));
    }

    #[test]
    fn live_session_reports_accumulate_transport_counters() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: 0,
            writes: 0,
            subscribes: 1,
            notifications: 2,
            notification_bytes: 40,
            latest_notification_len: Some(20),
            telemetry: 0,
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: 0,
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: 0,
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: Vec::new(),
            disconnects: 0,
        };

        state.apply_session_report(&report);
        state.apply_session_report(&report);

        assert_eq!(state.counters.subscriptions, 2);
        assert_eq!(state.counters.notifications, 4);
        assert_eq!(state.counters.notification_bytes, 80);
        assert_eq!(state.counters.latest_notification_len, Some(20));
    }

    #[test]
    fn live_session_report_uses_estimated_battery_from_voltage_telemetry() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: 0,
            writes: 0,
            subscribes: 1,
            notifications: 1,
            notification_bytes: 20,
            latest_notification_len: Some(20),
            telemetry: 1,
            telemetry_snapshot: live_aero_telemetry_snapshot(),
            read_only_responses: 0,
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: 0,
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(42),
                delta: TelemetryDelta {
                    speed_mm_s: Some(Measured::reported(0)),
                    voltage_mv: Some(Measured::reported(108_760)),
                    motor_current_ma: Some(Measured::reported(0)),
                    controller_temperature_mc: Some(Measured::reported(33_270)),
                    pwm_permille: Some(Measured::reported(-1_000)),
                    distance_mm: Some(Measured::reported(1_551_169_000)),
                    pitch_mdeg: Some(Measured::reported(69_060)),
                    battery_percent_estimated: Some(Measured::estimated(47)),
                    ..TelemetryDelta::empty(42)
                },
            }],
            disconnects: 0,
        };

        state.apply_session_report(&report);

        assert_eq!(state.telemetry.battery_pct, Some(47));
        assert_eq!(
            state.telemetry.battery_source,
            BatterySource::TelemetryEstimated
        );
        assert_eq!(state.telemetry.latest_speed_mph, Some(0));
        assert_eq!(state.telemetry.latest_voltage_v, Some(109));
        assert_eq!(state.telemetry.latest_current_a, Some(0));
        assert_eq!(state.telemetry.latest_temperature_c, Some(33));
        assert_eq!(state.telemetry.latest_distance_m, Some(1_551_169));
        assert_eq!(state.telemetry.latest_pitch_deg, Some(69));
        assert_eq!(state.telemetry.latest_pwm_pct, Some(-100));
        assert_eq!(state.telemetry.voltage_v.len(), HISTORY_LIMIT);
        assert!(
            state
                .telemetry
                .voltage_v
                .iter()
                .all(|voltage| *voltage == 109)
        );

        let overview_text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(overview_text.contains("47%"));
        assert!(overview_text.contains("Voltage"));

        state.active_tab = 1;
        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("voltage 109 V"));
        assert!(text.contains("109 V"));
        assert!(text.contains("0 A"));
        assert!(text.contains("33 C"));
        assert!(text.contains("-100%"));
        assert!(text.contains("1551.2 km"));
        assert!(!text.contains("1551169 m"));
        assert!(text.contains("69 deg"));
        assert!(text.contains("telemetry mapped"));
        assert!(text.contains("distance=1551.2 km"));
        assert!(text.contains("current=0A"));
        assert!(!text.contains("telemetry unmapped notifications=1"));
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
        assert!(text.contains("voltage"));
    }

    #[test]
    fn profiles_tab_renders_read_only_aero_state() {
        let mut state = DashboardState::empty();
        state.active_tab = 2;
        state
            .telemetry
            .apply_snapshot(live_aero_telemetry_snapshot());
        state.read_only.firmware = Some("43.2.54".to_owned());
        state.read_only.settings.push_back(
            "field=36 value=1920 quality=known verification=hardware_verified".to_owned(),
        );
        state.read_only.settings.push_back(
            "field=37 value=1940 quality=known verification=hardware_verified".to_owned(),
        );
        state
            .read_only
            .bms_pages
            .push_back("selector=2 kind=cell_voltage verification=hardware_verified".to_owned());
        state
            .read_only
            .bms_pages
            .push_back("selector=8 kind=raw verification=hardware_verified".to_owned());
        state
            .read_only
            .bms_pages
            .push_back("selector=47 kind=temperature verification=hardware_verified".to_owned());
        state.read_only.unknown_raw_pages = 1;
        state.read_only.diagnostics = 1;

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Read-only responses"));
        assert!(text.contains("firmware 43.2.54"));
        assert!(text.contains("settings"));
        assert!(text.contains("field=37 value=1940"));
        assert!(text.contains("field=36 value=1920"));
        assert!(text.contains("bms pages"));
        assert!(text.contains("selector=47 kind=temperature"));
        assert!(text.contains("selector=8 kind=raw"));
        assert!(text.contains("raw/unverified pages 1"));
        assert!(text.contains("diagnostic responses 1"));
    }

    #[test]
    fn live_telemetry_tab_renders_decoder_input_when_decoder_has_no_samples() {
        let mut state = DashboardState::empty();
        state.active_tab = 1;
        state.counters.notifications = 47;
        state.counters.notification_bytes = 4_700;
        state.counters.latest_notification_len = Some(100);

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Decoded telemetry"));
        assert!(text.contains("transport notifications"));
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
            "demo state loaded from demo state: aero-nf2557.v1",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
    }

    #[test]
    fn logs_tab_uses_recent_events_as_the_primary_panel() {
        let mut state = DashboardState::sample();
        state.active_tab = 3;

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert_eq!(text.matches("Recent events").count(), 1);
        assert!(text.contains("demo state loaded from demo state: aero-nf2557.v1"));
        assert!(text.contains("dashboard booted in read-only mode"));
    }

    #[test]
    fn taller_logs_tab_reveals_more_retained_events() {
        let mut state = DashboardState::empty();
        state.active_tab = 3;
        for index in 0..80 {
            state.push_log("info", &format!("event-{index:02}"));
        }

        let short_text = buffer_text(&render_buffer(&state, 80, 12));
        let tall_text = buffer_text(&render_buffer(&state, 80, 30));

        assert!(short_text.contains("Recent events"));
        assert!(tall_text.contains("Recent events"));
        assert!(short_text.contains("event-79"));
        assert!(tall_text.contains("event-79"));
        assert!(!short_text.contains("event-60"));
        assert!(tall_text.contains("event-60"));
        assert!(!tall_text.contains("event-50"));
    }

    #[test]
    fn log_history_keeps_a_large_recent_tail_for_big_screens() {
        let mut state = DashboardState::empty();
        for index in 0..1_100 {
            state.push_log("info", &format!("event-{index:04}"));
        }

        assert_eq!(state.logs.len(), LOG_LIMIT);
        assert_eq!(
            state.logs.front().map(|entry| entry.message.as_str()),
            Some("event-0076")
        );
        assert_eq!(
            state.logs.back().map(|entry| entry.message.as_str()),
            Some("event-1099")
        );
    }

    #[test]
    fn overview_renders_voltage_sparkline_as_compact_battery_companion() {
        let buffer = render_buffer(&DashboardState::sample(), 120, 36);
        let battery_position =
            find_text_position(&buffer, "Battery").expect("battery gauge renders");
        let voltage_position =
            find_text_position(&buffer, "Voltage").expect("voltage sparkline renders");
        let signal_position = find_text_position(&buffer, "Signal").expect("signal gauge renders");

        assert_eq!(voltage_position.1, battery_position.1);
        assert!(voltage_position.0 > battery_position.0);
        assert!(voltage_position.0 < signal_position.0);
        assert!(
            signal_position.0 - voltage_position.0 <= 18,
            "voltage panel should stay compact near battery, positions: battery={battery_position:?} voltage={voltage_position:?} signal={signal_position:?}"
        );
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
    fn demo_seed_hydrates_state_without_capture_replay() {
        let state = DashboardState::demo(None);

        assert_eq!(
            state.provenance.as_deref(),
            Some("demo state: aero-nf2557.v1")
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
                .any(|entry| entry.message.contains("demo state"))
        );
    }

    #[test]
    fn dashboard_render_shows_demo_provenance() {
        let buffer = render_buffer(&DashboardState::sample(), 120, 36);
        let text = buffer_text(&buffer);

        assert!(text.contains("demo state"));
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
