use std::{
    collections::VecDeque,
    fmt::{self, Write as FmtWrite},
    io::{self, Read, Write},
    marker::PhantomData,
    ops::RangeInclusive,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use cutout_btle::{
    ConnectionSummary, ConnectionTarget, NotificationCount, NotificationPayloadTotal,
    ServiceSummary, SessionBridgeEvent, SessionBridgeReport, SubscribeCount,
};
use cutout_core::{
    Angle, BatteryCurrent, BatteryLevel, BatteryPagePayload, CatalogModelResolution, Current,
    DiagnosticReadback, Distance, DutyCycle, FirmwareInfo, Measured, ModelCatalog,
    NotificationByteLen, NotificationIngestOutcome, ParserDiagnostics, PhaseCurrent, Power,
    ProtocolFamily, RawTelemetryReadback, ReadOnlyResponse, SettingsEntry, SettingsReadback, Speed,
    TelemetryDelta, TelemetrySnapshot, Temperature, Voltage,
};
use cutout_protocols::{
    MODEL_CATALOG, NOSFET_AERO_SESSION_KEY, VETERAN_FIELD_CHARGE_MODE, VeteranModelProfile,
};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DashboardTab(usize);

impl DashboardTab {
    #[cfg(test)]
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    const fn first() -> Self {
        Self(0)
    }

    fn next(self) -> Self {
        Self((self.0 + 1) % TAB_COUNT)
    }

    fn previous(self) -> Self {
        Self(if self.0 == 0 {
            TAB_COUNT - 1
        } else {
            self.0 - 1
        })
    }

    fn bounded(self) -> usize {
        self.0.min(TAB_COUNT - 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanSelection(usize);

impl ScanSelection {
    const fn first() -> Self {
        Self(0)
    }

    const fn get(self) -> usize {
        self.0
    }

    fn shift_after_front_removal(&mut self) {
        self.0 = self.0.saturating_sub(1);
    }

    fn select_last_len(&mut self, len: usize) {
        self.0 = len.saturating_sub(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProfileSelection(usize);

impl ProfileSelection {
    const fn first() -> Self {
        Self(0)
    }

    const fn get(self) -> usize {
        self.0
    }

    fn move_down(&mut self, len: usize) {
        if len == 0 {
            self.0 = 0;
        } else {
            self.0 = (self.0 + 1).min(len - 1);
        }
    }

    fn move_up(&mut self) {
        self.0 = self.0.saturating_sub(1);
    }

    fn bounded(self, len: usize) -> Option<Self> {
        if len == 0 {
            None
        } else {
            Some(Self(self.0.min(len - 1)))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DashboardState {
    pub(crate) source: DashboardSource,
    pub(crate) active_tab: DashboardTab,
    pub(crate) active_profile_dashboard: Option<ProfileSelection>,
    pub(crate) provenance: Option<String>,
    pub(crate) device: DeviceSnapshot,
    pub(crate) scan_browser: ScanBrowser,
    pub(crate) telemetry: TelemetryWindow,
    pub(crate) read_only: ReadOnlyDashboardState,
    pub(crate) profiles: Vec<ProfileSnapshot>,
    pub(crate) profile_selection: ProfileSelection,
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
    pub(crate) selected: ScanSelection,
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
        current_limit: Option<String>,
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
            selected: ScanSelection::first(),
        }
    }

    fn selected(&self) -> Option<&ScanObservation> {
        self.observations.get(self.selected.get())
    }

    fn push_observation(&mut self, observation: ScanObservation, selected: bool) {
        if self.observations.len() == HISTORY_LIMIT {
            self.observations.remove(0);
            self.selected.shift_after_front_removal();
        }

        self.observations.push(observation);
        if selected {
            self.selected.select_last_len(self.observations.len());
        }
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DashboardCount<Tag> {
    value: u64,
    tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Clone for DashboardCount<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag> Copy for DashboardCount<Tag> {}

impl<Tag> Default for DashboardCount<Tag> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<Tag> DashboardCount<Tag> {
    const fn new(value: u64) -> Self {
        Self {
            value,
            tag: PhantomData,
        }
    }
}

impl<Tag> fmt::Display for DashboardCount<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReadOnlySummaryCount<Tag> {
    value: u64,
    tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Clone for ReadOnlySummaryCount<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag> Copy for ReadOnlySummaryCount<Tag> {}

impl<Tag> Default for ReadOnlySummaryCount<Tag> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<Tag> ReadOnlySummaryCount<Tag> {
    pub(crate) const fn new(value: u64) -> Self {
        Self {
            value,
            tag: PhantomData,
        }
    }

    const fn increment(self) -> Self {
        Self::new(self.value.saturating_add(1))
    }
}

impl<Tag> fmt::Display for ReadOnlySummaryCount<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DiscoveredDeviceCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConnectedDeviceCountTag;

pub(crate) type DiscoveredDeviceCount = DashboardCount<DiscoveredDeviceCountTag>;
pub(crate) type ConnectedDeviceCount = DashboardCount<ConnectedDeviceCountTag>;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct RawReadOnlyPageCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReadOnlyDiagnosticResponseCountTag;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct RawTelemetryResponseCountTag;

pub(crate) type RawReadOnlyPageCount = ReadOnlySummaryCount<RawReadOnlyPageCountTag>;
pub(crate) type ReadOnlyDiagnosticResponseCount =
    ReadOnlySummaryCount<ReadOnlyDiagnosticResponseCountTag>;
pub(crate) type RawTelemetryResponseCount = ReadOnlySummaryCount<RawTelemetryResponseCountTag>;

fn clamp_percent(value: u64) -> u8 {
    match u8::try_from(value) {
        Ok(value) if value <= 100 => value,
        _ => 100,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DashboardBatteryLevel(BatteryLevel);

impl DashboardBatteryLevel {
    fn new(value: u64) -> Self {
        Self(BatteryLevel::from_percent(clamp_percent(value)))
    }

    const fn decrement_for_demo(self) -> Self {
        let value = self.0.as_percent().saturating_sub(1);
        Self(BatteryLevel::from_percent(if value < 10 {
            10
        } else {
            value
        }))
    }

    fn ratio(self) -> f64 {
        f64::from(self.0.as_percent()) / 100.0
    }
}

impl fmt::Display for DashboardBatteryLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.as_percent().fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SignalQuality(BatteryLevel);

impl SignalQuality {
    fn new(value: u64) -> Self {
        Self(BatteryLevel::from_percent(clamp_percent(value)))
    }

    fn from_signal_strength(signal: cutout_core::SignalStrength) -> Self {
        Self::new(u64::from(signal.as_quality_percent()))
    }

    const fn increment(self) -> Self {
        let value = self.0.as_percent().saturating_add(1);
        Self(BatteryLevel::from_percent(if value > 100 {
            100
        } else {
            value
        }))
    }

    fn ratio(self) -> f64 {
        f64::from(self.0.as_percent()) / 100.0
    }
}

impl fmt::Display for SignalQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.as_percent().fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplaySpeed(Speed);

impl DisplaySpeed {
    fn from_speed(value: Speed) -> Self {
        Self(value)
    }

    pub(crate) fn get(self) -> u64 {
        self.0.as_mph()
    }

    const fn is_stationary(self) -> bool {
        self.0.as_millimetres_per_second() == 0
    }

    const fn is_moving(self) -> bool {
        self.0.as_millimetres_per_second() > 0
    }
}

impl fmt::Display for DisplaySpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} mph", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayVoltage(Voltage);

impl DisplayVoltage {
    fn from_voltage(value: Voltage) -> Self {
        Self(value)
    }

    pub(crate) fn get(self) -> u64 {
        self.0.as_whole_volts()
    }
}

impl fmt::Display for DisplayVoltage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} V", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WheelPitchDeg(Angle);

impl WheelPitchDeg {
    fn from_angle(value: Angle) -> Self {
        Self(value)
    }

    fn get(self) -> i64 {
        self.0.as_whole_degrees()
    }

    const fn is_lifted_or_tilted(self) -> bool {
        let millidegrees = self.0.as_millidegrees();
        millidegrees <= -45_000 || millidegrees >= 45_000
    }
}

impl fmt::Display for WheelPitchDeg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} deg", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayDutyCycle(DutyCycle);

impl DisplayDutyCycle {
    fn from_duty_cycle(value: DutyCycle) -> Self {
        Self(value)
    }

    fn get(self) -> i64 {
        self.0.as_whole_percent()
    }
}

impl fmt::Display for DisplayDutyCycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayTemperature(Temperature);

impl DisplayTemperature {
    fn from_temperature(value: Temperature) -> Self {
        Self(value)
    }

    fn get(self) -> i64 {
        self.0.as_whole_celsius()
    }
}

impl fmt::Display for DisplayTemperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} C", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayPhaseCurrent(PhaseCurrent);

impl DisplayPhaseCurrent {
    fn from_milliamps(value: i32) -> Self {
        Self(PhaseCurrent::from_milliamps(value))
    }

    fn get(self) -> i64 {
        self.0.as_whole_amps()
    }

    const fn is_idle(self) -> bool {
        let milliamps = self.0.as_milliamps();
        milliamps > -1_000 && milliamps < 1_000
    }

    const fn is_working(self) -> bool {
        !self.is_idle()
    }
}

impl fmt::Display for DisplayPhaseCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} A", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayBatteryCurrent(BatteryCurrent);

impl DisplayBatteryCurrent {
    fn from_current(value: BatteryCurrent) -> Self {
        Self(value)
    }

    fn from_milliamps(value: i32) -> Self {
        Self::from_current(BatteryCurrent::from_milliamps(value))
    }

    fn get(self) -> i64 {
        self.0.as_whole_amps()
    }

    const fn is_idle(self) -> bool {
        let milliamps = self.0.as_milliamps();
        milliamps > -1_000 && milliamps < 1_000
    }

    const fn is_working(self) -> bool {
        !self.is_idle()
    }
}

fn battery_current_from_current(value: Current) -> BatteryCurrent {
    BatteryCurrent::from_milliamps(value.as_milliamps())
}

impl fmt::Display for DisplayBatteryCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} A", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationalCurrent {
    Charge(DisplayBatteryCurrent),
    PhaseFallback(DisplayPhaseCurrent),
}

impl OperationalCurrent {
    const fn is_idle(self) -> bool {
        match self {
            Self::Charge(current) => current.is_idle(),
            Self::PhaseFallback(current) => current.is_idle(),
        }
    }

    const fn is_working(self) -> bool {
        match self {
            Self::Charge(current) => current.is_working(),
            Self::PhaseFallback(current) => current.is_working(),
        }
    }
}

impl fmt::Display for OperationalCurrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Charge(current) => current.fmt(f),
            Self::PhaseFallback(current) => current.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayPower(Power);

impl DisplayPower {
    fn from_power(value: Power) -> Self {
        Self(value)
    }

    #[cfg(test)]
    fn from_watts(value: i64) -> Self {
        Self::from_power(Power::from_watts(value))
    }

    fn from_milliwatts(value: i64) -> Self {
        Self::from_power(Power::from_milliwatts(value))
    }

    fn from_voltage_current(voltage: Voltage, current: BatteryCurrent) -> Self {
        Self::from_power(Power::from_voltage_current(voltage, current))
    }

    const fn get(self) -> i64 {
        self.0.as_whole_watts()
    }
}

impl fmt::Display for DisplayPower {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} W", self.get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationalChargeState {
    Charging,
    NotCharging,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationalWheelState {
    Charging,
    Riding,
    Balancing,
    Parked,
    Lifted,
    Unknown,
}

impl OperationalWheelState {
    const fn label(self) -> &'static str {
        match self {
            Self::Charging => "charging",
            Self::Riding => "riding",
            Self::Balancing => "balancing",
            Self::Parked => "parked",
            Self::Lifted => "lifted",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationalDutyCycle {
    Known(DisplayDutyCycle),
    Unknown,
}

impl fmt::Display for OperationalDutyCycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(value) => value.fmt(f),
            Self::Unknown => f.write_str("unknown"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PopulatedDiagnosticDetailCount(usize);

impl PopulatedDiagnosticDetailCount {
    const fn new(value: usize) -> Self {
        Self(value)
    }

    fn from_diagnostics(diagnostics: DiagnosticReadback) -> Self {
        Self::new(diagnostics.details.into_iter().flatten().count())
    }
}

impl fmt::Display for PopulatedDiagnosticDetailCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PopulatedRawTelemetryFieldCount(usize);

impl PopulatedRawTelemetryFieldCount {
    const fn new(value: usize) -> Self {
        Self(value)
    }

    fn from_raw_telemetry(raw: RawTelemetryReadback) -> Self {
        Self::new(raw.fields.into_iter().flatten().count())
    }
}

impl fmt::Display for PopulatedRawTelemetryFieldCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionCounters {
    pub(crate) discovered: DiscoveredDeviceCount,
    pub(crate) connected: ConnectedDeviceCount,
    pub(crate) subscriptions: SubscribeCount,
    pub(crate) notifications: NotificationCount,
    pub(crate) notification_bytes: NotificationPayloadTotal,
    pub(crate) latest_notification_len: Option<NotificationByteLen>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogEntry {
    pub(crate) level: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReadOnlyDashboardState {
    pub(crate) firmware: Option<FirmwareInfo>,
    pub(crate) settings: VecDeque<SettingsEntry>,
    pub(crate) bms_pages: VecDeque<BatteryPagePayload>,
    pub(crate) latest_bms_temperature: Option<BatteryPagePayload>,
    pub(crate) diagnostics: ReadOnlyDiagnosticResponseCount,
    pub(crate) raw_telemetry: RawTelemetryResponseCount,
    pub(crate) unknown_raw_pages: RawReadOnlyPageCount,
}

impl ReadOnlyDashboardState {
    fn apply_response(&mut self, response: ReadOnlyResponse) {
        match response {
            ReadOnlyResponse::Firmware(firmware) => {
                self.firmware = Some(firmware);
            }
            ReadOnlyResponse::Settings(settings) => {
                for entry in settings.entries.into_iter().flatten() {
                    push_bounded(&mut self.settings, entry);
                }
            }
            ReadOnlyResponse::Battery(payload) => {
                let page = payload.page();
                if matches!(
                    page.kind,
                    cutout_core::BatteryPageKind::Raw | cutout_core::BatteryPageKind::Metadata
                ) {
                    self.unknown_raw_pages = self.unknown_raw_pages.increment();
                }
                if BmsTemperatureValues(payload).has_values() {
                    self.latest_bms_temperature = Some(payload);
                }
                push_bounded(&mut self.bms_pages, payload);
            }
            ReadOnlyResponse::Diagnostics(_) => {
                self.diagnostics = self.diagnostics.increment();
            }
            ReadOnlyResponse::RawTelemetry(_) => {
                self.raw_telemetry = self.raw_telemetry.increment();
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DashboardUpdate {
    BatteryLevel(u8),
    SessionReport(Box<SessionBridgeReport>),
    Log { level: String, message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TelemetryWindow {
    pub(crate) battery_level: Option<DashboardBatteryLevel>,
    pub(crate) battery_source: BatterySource,
    pub(crate) signal_quality: SignalQuality,
    pub(crate) latest_speed: Option<DisplaySpeed>,
    pub(crate) latest_voltage: Option<DisplayVoltage>,
    pub(crate) latest_phase_current: Option<DisplayPhaseCurrent>,
    pub(crate) latest_battery_current: Option<DisplayBatteryCurrent>,
    pub(crate) latest_power: Option<DisplayPower>,
    pub(crate) latest_temperature: Option<DisplayTemperature>,
    pub(crate) latest_distance: Option<Distance>,
    pub(crate) latest_pitch: Option<WheelPitchDeg>,
    pub(crate) latest_pwm: Option<DisplayDutyCycle>,
    pub(crate) speed_samples: Vec<Speed>,
    pub(crate) voltage_samples: Vec<Voltage>,
    pub(crate) current_samples: Vec<Current>,
    pub(crate) temperature_samples: Vec<Temperature>,
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
    MoveDown,
    MoveUp,
    Enter,
    Back,
}

const DEMO_PROVENANCE: &str = "demo state: aero-nf2557.v1";
const DEMO_SPEED_MPH: &[u64] = &[0, 4, 9, 14, 18, 22, 24, 21, 19, 17, 16, 18];
const DEMO_VOLTAGE: &[Voltage] = &[
    Voltage::from_volts(52),
    Voltage::from_volts(52),
    Voltage::from_volts(53),
    Voltage::from_volts(53),
    Voltage::from_volts(54),
    Voltage::from_volts(54),
    Voltage::from_volts(55),
    Voltage::from_volts(55),
    Voltage::from_volts(55),
    Voltage::from_volts(54),
    Voltage::from_volts(54),
    Voltage::from_volts(53),
];
const DEMO_CURRENT: &[Current] = &[
    Current::from_amps(3),
    Current::from_amps(4),
    Current::from_amps(6),
    Current::from_amps(7),
    Current::from_amps(8),
    Current::from_amps(9),
    Current::from_amps(10),
    Current::from_amps(10),
    Current::from_amps(9),
    Current::from_amps(8),
    Current::from_amps(7),
    Current::from_amps(6),
];
const DEMO_TEMPERATURE: &[Temperature] = &[
    Temperature::from_celsius(30),
    Temperature::from_celsius(31),
    Temperature::from_celsius(31),
    Temperature::from_celsius(32),
    Temperature::from_celsius(32),
    Temperature::from_celsius(33),
    Temperature::from_celsius(34),
    Temperature::from_celsius(34),
    Temperature::from_celsius(35),
    Temperature::from_celsius(35),
    Temperature::from_celsius(34),
    Temperature::from_celsius(33),
];
static DEMO_SPEED: OnceLock<Box<[Speed]>> = OnceLock::new();

impl DashboardState {
    pub(crate) fn empty() -> Self {
        Self {
            source: DashboardSource::Live,
            active_tab: DashboardTab::first(),
            active_profile_dashboard: None,
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
            profile_selection: ProfileSelection::first(),
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
                state.scan_browser.selected = ScanSelection::first();
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
            discovered: DiscoveredDeviceCount::new(8),
            connected: ConnectedDeviceCount::new(1),
            subscriptions: SubscribeCount::from_events(4),
            notifications: NotificationCount::from_events(27),
            notification_bytes: NotificationPayloadTotal::default(),
            latest_notification_len: None,
        };
        self.telemetry.load_window(
            DashboardBatteryLevel::new(74),
            SignalQuality::new(81),
            demo_speed_samples(),
            DEMO_VOLTAGE,
            DEMO_CURRENT,
            DEMO_TEMPERATURE,
        );
        self.read_only.firmware = Some(FirmwareInfo {
            firmware_major: Some(Measured::reported(43)),
            firmware_minor: Some(Measured::reported(2)),
            firmware_patch: Some(Measured::reported(54)),
            ..FirmwareInfo::default()
        });
        self.profiles.push(ProfileSnapshot {
            name: "Primary drive".to_owned(),
            source: "probe".to_owned(),
            status: "ready".to_owned(),
            family: ProfileFamily::AeroVeteran {
                current_limit: Some("45A".to_owned()),
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
                current_limit: None,
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
        state.counters.discovered = DiscoveredDeviceCount::new(1);
        state.counters.connected = ConnectedDeviceCount::new(1);
        state.telemetry.signal_quality = observation
            .rssi
            .map_or_else(SignalQuality::default, SignalQuality::from_signal_strength);
        state.scan_browser.push_observation(
            ScanObservation {
                name: state.device.name.clone(),
                address: state.device.address.clone(),
                identifier: state.device.identifier.clone(),
                rssi: observation.rssi.map_or_else(
                    || "unknown".to_owned(),
                    |rssi| format!("{} dBm", rssi.as_dbm()),
                ),
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
                current_limit: None,
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
            .saturating_add(report.subscribes);
        self.counters.notifications = self
            .counters
            .notifications
            .saturating_add(report.notifications);
        self.counters.notification_bytes = self
            .counters
            .notification_bytes
            .saturating_add(report.notification_bytes);
        self.counters.latest_notification_len = report.latest_notification_len;
        self.push_log(
            "info",
            &format!(
                "session update protocol_writes={} writes={} subscribes={} notifications={} bytes={}",
                report.protocol_writes,
                report.writes,
                report.subscribes,
                report.notifications,
                report.notification_bytes.as_bytes()
            ),
        );

        if report.telemetry.has_no_events() {
            if report_has_no_parsed_events(report) {
                self.push_log(
                    "warn",
                    "notifications received but telemetry decoder produced no samples",
                );
            }
        } else {
            self.push_log("info", &format!("telemetry samples={}", report.telemetry));
        }

        if report.diagnostics.has_events() {
            self.push_log("warn", &format!("diagnostics={}", report.diagnostics));
        }

        if report.telemetry.has_events() {
            let snapshot = report.telemetry_snapshot;
            self.push_log_with_display("info", MappedTelemetryLog(snapshot));
            self.telemetry.apply_snapshot(snapshot);
        }
        for response in &report.read_only_response_events {
            self.read_only.apply_response(*response);
        }
        if report.read_only_responses.has_events() {
            self.push_log(
                "info",
                &format!("read-only responses={}", report.read_only_responses),
            );
        }
        if report_has_no_parsed_events(report) {
            self.push_log("trace", &format_unmapped_telemetry_event(report));
        }
        for event in &report.events {
            if let SessionBridgeEvent::NotificationIngest {
                monotonic_ms,
                outcome,
            } = event
            {
                self.push_notification_ingest_log(*monotonic_ms, *outcome);
            } else {
                let (level, message) = format_bridge_event(event);
                self.push_log(level, &message);
            }
        }
    }

    pub(crate) fn apply_battery_level(&mut self, level: u8) {
        let level = DashboardBatteryLevel::new(u64::from(level));
        self.telemetry.battery_level = Some(level);
        self.telemetry.battery_source = BatterySource::StandardBle;
        self.push_log("info", &format!("battery level {level}%"));
    }

    pub(crate) fn apply_update(&mut self, update: DashboardUpdate) {
        match update {
            DashboardUpdate::BatteryLevel(percent) => self.apply_battery_level(percent),
            DashboardUpdate::SessionReport(report) => self.apply_session_report(&report),
            DashboardUpdate::Log { level, message } => self.push_log_from_tracing(&level, &message),
        }
    }

    pub(crate) fn advance(&mut self) {
        if self.source == DashboardSource::Live {
            return;
        }

        let next_notification = self.counters.notifications.increment();
        self.counters.notifications = next_notification;
        self.telemetry.step();
        self.push_log("trace", "fixture heartbeat advanced");
    }

    fn next_tab(&mut self) {
        self.active_profile_dashboard = None;
        self.active_tab = self.active_tab.next();
    }

    fn previous_tab(&mut self) {
        self.active_profile_dashboard = None;
        self.active_tab = self.active_tab.previous();
    }

    fn handle_input(&mut self, input: DashboardInput) {
        match input {
            DashboardInput::Quit => {}
            DashboardInput::NextTab => self.next_tab(),
            DashboardInput::PreviousTab => self.previous_tab(),
            DashboardInput::MoveDown => {
                if self.active_tab.bounded() == 2 && self.active_profile_dashboard.is_none() {
                    self.profile_selection.move_down(self.profiles.len());
                }
            }
            DashboardInput::MoveUp => {
                if self.active_tab.bounded() == 2 && self.active_profile_dashboard.is_none() {
                    self.profile_selection.move_up();
                }
            }
            DashboardInput::Enter => {
                if self.active_tab.bounded() == 2 {
                    self.active_profile_dashboard =
                        self.profile_selection.bounded(self.profiles.len());
                }
            }
            DashboardInput::Back => {
                self.active_profile_dashboard = None;
            }
        }
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

    fn push_log_with_display(&mut self, level: &str, message: impl fmt::Display) {
        let rendered = message.to_string();
        log_dashboard_recent_event(level, &rendered);
        if !dashboard_log_sink_installed() {
            self.push_log_entry(level, &rendered);
        }
    }

    fn push_notification_ingest_log(
        &mut self,
        monotonic_ms: cutout_btle::MonotonicMs,
        outcome: NotificationIngestOutcome,
    ) {
        let message = NotificationIngestLog {
            monotonic_ms: monotonic_ms.get(),
            outcome,
        };
        let level = message.level();
        self.push_log_with_display(level, message);
    }

    fn push_log_from_tracing(&mut self, level: &str, message: &str) {
        self.push_log_entry(level, message);
    }

    fn push_log_entry(&mut self, level: &str, message: &str) {
        self.push_log_display(level, message);
    }

    fn push_log_display(&mut self, level: &str, message: impl fmt::Display) {
        if self.logs.len() == LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(LogEntry {
            level: level.to_owned(),
            message: message.to_string(),
        });
    }
}

impl TelemetryWindow {
    fn empty() -> Self {
        Self {
            battery_level: None,
            battery_source: BatterySource::Unknown,
            signal_quality: SignalQuality::default(),
            latest_speed: None,
            latest_voltage: None,
            latest_phase_current: None,
            latest_battery_current: None,
            latest_power: None,
            latest_temperature: None,
            latest_distance: None,
            latest_pitch: None,
            latest_pwm: None,
            speed_samples: Vec::with_capacity(HISTORY_LIMIT),
            voltage_samples: Vec::with_capacity(HISTORY_LIMIT),
            current_samples: Vec::with_capacity(HISTORY_LIMIT),
            temperature_samples: Vec::with_capacity(HISTORY_LIMIT),
            current_points: Vec::with_capacity(HISTORY_LIMIT),
            temperature_points: Vec::with_capacity(HISTORY_LIMIT),
        }
    }

    fn load_window(
        &mut self,
        battery_level: DashboardBatteryLevel,
        signal_quality: SignalQuality,
        speed_samples: &'static [Speed],
        voltage_samples: &'static [Voltage],
        current_samples: &'static [Current],
        temperature_samples: &'static [Temperature],
    ) {
        self.battery_level = Some(battery_level);
        self.battery_source = BatterySource::TelemetryReported;
        self.signal_quality = signal_quality;
        self.speed_samples.clear();
        self.voltage_samples.clear();
        self.current_samples.clear();
        self.temperature_samples.clear();
        self.speed_samples.extend(speed_samples.iter().copied());
        self.voltage_samples.extend(voltage_samples.iter().copied());
        self.current_samples.extend(current_samples.iter().copied());
        self.temperature_samples
            .extend(temperature_samples.iter().copied());
        self.latest_speed = self
            .speed_samples
            .last()
            .copied()
            .map(DisplaySpeed::from_speed);
        self.latest_voltage = self
            .voltage_samples
            .last()
            .copied()
            .map(DisplayVoltage::from_voltage);
        self.latest_battery_current = self
            .current_samples
            .last()
            .copied()
            .map(battery_current_from_current)
            .map(DisplayBatteryCurrent::from_current);
        self.latest_power = self
            .voltage_samples
            .last()
            .copied()
            .zip(self.current_samples.last().copied())
            .map(|(voltage, current)| {
                DisplayPower::from_voltage_current(voltage, battery_current_from_current(current))
            });
        self.latest_temperature = self
            .temperature_samples
            .last()
            .copied()
            .map(DisplayTemperature::from_temperature);
        self.sync_points();
    }

    fn step(&mut self) {
        let next_speed = (self
            .speed_samples
            .last()
            .copied()
            .map_or(0, |speed| DisplaySpeed::from_speed(speed).get())
            + 3)
            % 40;
        let next_voltage = 50
            + ((self
                .voltage_samples
                .last()
                .copied()
                .map_or(52, |voltage| DisplayVoltage::from_voltage(voltage).get())
                + 1)
                % 6);
        let next_current = 4
            + ((self
                .current_samples
                .last()
                .copied()
                .map_or(5, Current::as_abs_whole_amps)
                + 1)
                % 9);
        let next_temperature = 30
            + ((self
                .temperature_samples
                .last()
                .copied()
                .map_or(32, Temperature::as_abs_whole_celsius)
                + 1)
                % 9);

        if let Some(battery_level) = self.battery_level.as_mut() {
            *battery_level = battery_level.decrement_for_demo();
        }
        self.signal_quality = self.signal_quality.increment();
        push_sample(&mut self.speed_samples, Speed::from_mph(next_speed));
        push_sample(&mut self.voltage_samples, Voltage::from_volts(next_voltage));
        push_sample(
            &mut self.current_samples,
            Current::from_amps(i64::try_from(next_current).unwrap_or(i64::MAX)),
        );
        push_sample(
            &mut self.temperature_samples,
            Temperature::from_celsius(i64::try_from(next_temperature).unwrap_or(i64::MAX)),
        );
        self.latest_speed = self
            .speed_samples
            .last()
            .copied()
            .map(DisplaySpeed::from_speed);
        self.latest_voltage = self
            .voltage_samples
            .last()
            .copied()
            .map(DisplayVoltage::from_voltage);
        self.latest_battery_current = self
            .current_samples
            .last()
            .copied()
            .map(battery_current_from_current)
            .map(DisplayBatteryCurrent::from_current);
        self.latest_power = self
            .voltage_samples
            .last()
            .copied()
            .zip(self.current_samples.last().copied())
            .map(|(voltage, current)| {
                DisplayPower::from_voltage_current(voltage, battery_current_from_current(current))
            });
        self.latest_temperature = self
            .temperature_samples
            .last()
            .copied()
            .map(DisplayTemperature::from_temperature);
        self.sync_points();
    }

    fn sync_points(&mut self) {
        self.current_points.clear();
        self.temperature_points.clear();
        self.current_points.reserve(HISTORY_LIMIT);
        self.temperature_points.reserve(HISTORY_LIMIT);

        for (index, value) in self.current_samples.iter().enumerate() {
            self.current_points
                .push((index_to_f64(index), to_f64(value.as_abs_whole_amps())));
        }

        for (index, value) in self.temperature_samples.iter().enumerate() {
            self.temperature_points
                .push((index_to_f64(index), to_f64(value.as_abs_whole_celsius())));
        }
    }

    fn apply_snapshot(&mut self, snapshot: TelemetrySnapshot) {
        if let Some(percent) = snapshot.battery_level_reported {
            self.battery_level = Some(DashboardBatteryLevel::new(u64::from(percent.value.get())));
            self.battery_source = BatterySource::TelemetryReported;
        } else if let Some(percent) = snapshot.battery_level_estimated {
            self.battery_level = Some(DashboardBatteryLevel::new(u64::from(percent.value.get())));
            self.battery_source = BatterySource::TelemetryEstimated;
        }
        if let Some(speed) = snapshot.speed {
            let speed_value = speed.value;
            let speed = DisplaySpeed::from_speed(speed_value);
            self.latest_speed = Some(speed);
            push_sample(&mut self.speed_samples, speed_value);
        }
        if let Some(voltage) = snapshot.voltage {
            let voltage_value = voltage.value;
            let voltage = DisplayVoltage::from_voltage(voltage_value);
            self.latest_voltage = Some(voltage);
            seed_or_push_sample(&mut self.voltage_samples, voltage_value);
        }
        if let Some(current) = snapshot.battery_current {
            self.latest_battery_current =
                Some(DisplayBatteryCurrent::from_milliamps(current.value.get()));
        }
        if let Some(current) = snapshot.motor_current {
            let current = DisplayPhaseCurrent::from_milliamps(current.value.get());
            self.latest_phase_current = Some(current);
            push_sample(
                &mut self.current_samples,
                Current::from_milliamps(current.0.as_milliamps().saturating_abs()),
            );
        } else if let Some(current) = snapshot.battery_current {
            push_sample(
                &mut self.current_samples,
                Current::from_milliamps(current.value.get().saturating_abs()),
            );
        }
        if let Some(power) = snapshot.power {
            self.latest_power = Some(DisplayPower::from_milliwatts(power.value.get()));
        }
        if let Some(temperature) = snapshot
            .controller_temperature
            .or(snapshot.motor_temperature)
            .or(snapshot.battery_temperature)
        {
            self.latest_temperature = Some(DisplayTemperature::from_temperature(temperature.value));
            push_sample(&mut self.temperature_samples, temperature.value);
        }
        if let Some(distance) = snapshot.distance {
            self.latest_distance = Some(distance.value);
        }
        if let Some(pitch) = snapshot.pitch {
            self.latest_pitch = Some(WheelPitchDeg::from_angle(pitch.value));
        }
        if let Some(pwm) = snapshot.pwm {
            self.latest_pwm = Some(DisplayDutyCycle::from_duty_cycle(pwm.value));
        }
        self.sync_points();
    }

    fn has_decoded_samples(&self) -> bool {
        !(self.speed_samples.is_empty()
            && self.voltage_samples.is_empty()
            && self.current_samples.is_empty()
            && self.temperature_samples.is_empty())
    }
}

fn push_sample<T: Copy>(series: &mut Vec<T>, value: T) {
    if series.len() == HISTORY_LIMIT {
        series.remove(0);
    }
    series.push(value);
}

fn seed_or_push_sample<T: Copy>(series: &mut Vec<T>, value: T) {
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

struct FirmwareSummary(FirmwareInfo);

impl fmt::Display for FirmwareSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_optional_measured_u16(f, self.0.firmware_major)?;
        write!(f, ".")?;
        write_optional_measured_u16(f, self.0.firmware_minor)?;
        write!(f, ".")?;
        write_optional_measured_u16(f, self.0.firmware_patch)
    }
}

fn write_optional_measured_u16(
    f: &mut fmt::Formatter<'_>,
    value: Option<Measured<u16>>,
) -> fmt::Result {
    if let Some(value) = value {
        write!(f, "{}", value.value)
    } else {
        write!(f, "?")
    }
}

struct SettingsEntrySummary(SettingsEntry);

impl fmt::Display for SettingsEntrySummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entry = self.0;
        write!(
            f,
            "field={} value={} quality={} verification={}",
            entry.field.id,
            entry.field.value,
            quality_name(entry.quality),
            verification_name(entry.verification)
        )
    }
}

const fn battery_page_kind_name(kind: cutout_core::BatteryPageKind) -> &'static str {
    match kind {
        cutout_core::BatteryPageKind::Metadata => "metadata",
        cutout_core::BatteryPageKind::CellVoltage => "cell_voltage",
        cutout_core::BatteryPageKind::Temperature => "temperature",
        cutout_core::BatteryPageKind::Raw => "raw",
    }
}

struct BmsTemperatureValues(BatteryPagePayload);

impl BmsTemperatureValues {
    fn has_values(self) -> bool {
        matches!(self.0, BatteryPagePayload::Temperature(_))
            && self
                .0
                .temperatures()
                .into_iter()
                .any(|value| value.is_some())
    }
}

impl fmt::Display for BmsTemperatureValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut wrote = false;
        for temperature in self.0.temperatures().into_iter().flatten() {
            if wrote {
                write!(f, ",")?;
            } else {
                write!(f, " temps_c=")?;
            }
            wrote = true;
            write!(
                f,
                "{}",
                DisplayTemperature::from_temperature(Temperature::from_millicelsius(
                    temperature.value,
                ))
                .get()
            )?;
        }
        Ok(())
    }
}

struct BmsCurrentSummary(BatteryPagePayload);

impl fmt::Display for BmsCurrentSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let battery = self.0.battery();
        if let Some(current) = battery.current {
            write!(
                f,
                " current={}A",
                DisplayBatteryCurrent::from_milliamps(current.value.get()).get()
            )?;
        }
        if let Some(currents) = self.0.bms_pack_currents() {
            write!(
                f,
                " bms_current_0={}A bms_current_1={}A",
                DisplayBatteryCurrent(currents.current_0()).get(),
                DisplayBatteryCurrent(currents.current_1()).get()
            )?;
        }
        Ok(())
    }
}

struct BmsPageSummary(BatteryPagePayload);

impl fmt::Display for BmsPageSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let page = self.0.page();
        write!(
            f,
            "selector={} kind={} verification={}{}{}",
            page.selector,
            battery_page_kind_name(page.kind),
            verification_name(page.verification),
            BmsTemperatureValues(self.0),
            BmsCurrentSummary(self.0)
        )
    }
}

struct LatestBmsTemperatureSummary(BatteryPagePayload);

impl fmt::Display for LatestBmsTemperatureSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let page = self.0.page();
        write!(
            f,
            "selector={} verification={}{}",
            page.selector,
            verification_name(page.verification),
            BmsTemperatureValues(self.0)
        )
    }
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

fn demo_speed_samples() -> &'static [Speed] {
    DEMO_SPEED
        .get_or_init(|| {
            DEMO_SPEED_MPH
                .iter()
                .copied()
                .map(Speed::from_mph)
                .collect::<Vec<_>>()
                .into_boxed_slice()
        })
        .as_ref()
}

struct OptionalDisplayDistance(Option<Distance>);

impl fmt::Display for OptionalDisplayDistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => DisplayDistance(value).fmt(f),
            None => f.write_str("unknown"),
        }
    }
}

struct DisplayDistance(Distance);

impl DisplayDistance {
    #[cfg(test)]
    fn from_millimetres(value: u64) -> Self {
        Self(Distance::from_millimetres(value))
    }
}

impl fmt::Display for DisplayDistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meters = self.0.as_whole_metres();
        if meters < 1_000 {
            write!(f, "{meters} m")
        } else {
            write!(f, "{} km", TenthsDisplay(self.0.as_kilometre_tenths()))
        }
    }
}

struct TenthsDisplay(u64);

impl fmt::Display for TenthsDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.0 / 10, self.0 % 10)
    }
}

struct DeviceIdentity {
    make: String,
    model: String,
}

fn classify_device_identity(name: &str) -> DeviceIdentity {
    match ModelCatalog::new(&MODEL_CATALOG).resolve_advertised_name(name) {
        CatalogModelResolution::Matched(entry) => DeviceIdentity {
            make: entry.registry.manufacturer.to_owned(),
            model: entry.registry.model.to_owned(),
        },
        CatalogModelResolution::NoMatch | CatalogModelResolution::Ambiguous => DeviceIdentity {
            make: "unknown".to_owned(),
            model: "unknown".to_owned(),
        },
    }
}

struct MappedTelemetryLog(TelemetrySnapshot);

impl fmt::Display for MappedTelemetryLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fields = TelemetryFieldWriter::new(f, Some("telemetry mapped"), "none");
        let snapshot = self.0;

        if let Some(speed) = snapshot.speed {
            fields.write("speed", DisplaySpeed::from_speed(speed.value).get(), "mph")?;
        }
        if let Some(voltage) = snapshot.voltage {
            fields.write(
                "voltage",
                DisplayVoltage::from_voltage(voltage.value).get(),
                "V",
            )?;
        }
        if let Some(percent) = snapshot
            .battery_level_reported
            .or(snapshot.battery_level_estimated)
        {
            fields.write("battery", percent.value, "%")?;
        }
        if let Some(current) = snapshot.battery_current.or(snapshot.motor_current) {
            fields.write(
                "current",
                DisplayBatteryCurrent::from_milliamps(current.value.get()).get(),
                "A",
            )?;
        }
        if let Some(power) = snapshot.power {
            fields.write(
                "power",
                DisplayPower::from_milliwatts(power.value.get()).get(),
                "W",
            )?;
        }
        if let Some(temperature) = snapshot
            .controller_temperature
            .or(snapshot.motor_temperature)
            .or(snapshot.battery_temperature)
        {
            fields.write(
                "temperature",
                DisplayTemperature::from_temperature(temperature.value).get(),
                "C",
            )?;
        }
        if let Some(pwm) = snapshot.pwm {
            fields.write(
                "pwm",
                DisplayDutyCycle::from_duty_cycle(pwm.value).get(),
                "%",
            )?;
        }
        if let Some(distance) = snapshot.distance {
            fields.write_display("distance", DisplayTelemetryDistance(distance.value))?;
        }
        if let Some(pitch) = snapshot.pitch {
            fields.write("pitch", WheelPitchDeg::from_angle(pitch.value).get(), "deg")?;
        }
        if let Some(roll) = snapshot.roll {
            fields.write("roll", WheelPitchDeg::from_angle(roll.value).get(), "deg")?;
        }

        fields.finish()
    }
}

struct TelemetryDeltaLog(TelemetryDelta);

impl fmt::Display for TelemetryDeltaLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fields = TelemetryFieldWriter::new(f, None, "unmapped");
        let delta = self.0;

        if let Some(speed) = delta.speed {
            fields.write("speed", DisplaySpeed::from_speed(speed.value).get(), "mph")?;
        }
        if let Some(voltage) = delta.voltage {
            fields.write(
                "voltage",
                DisplayVoltage::from_voltage(voltage.value).get(),
                "V",
            )?;
        }
        if let Some(percent) = delta
            .battery_level_reported
            .or(delta.battery_level_estimated)
        {
            fields.write("battery", percent.value, "%")?;
        }
        if let Some(current) = delta.battery_current.or(delta.motor_current) {
            fields.write(
                "current",
                DisplayBatteryCurrent::from_milliamps(current.value.get()).get(),
                "A",
            )?;
        }
        if let Some(power) = delta.power {
            fields.write(
                "power",
                DisplayPower::from_milliwatts(power.value.get()).get(),
                "W",
            )?;
        }
        if let Some(temperature) = delta
            .controller_temperature
            .or(delta.motor_temperature)
            .or(delta.battery_temperature)
        {
            fields.write(
                "temperature",
                DisplayTemperature::from_temperature(temperature.value).get(),
                "C",
            )?;
        }
        if let Some(pwm) = delta.pwm {
            fields.write(
                "pwm",
                DisplayDutyCycle::from_duty_cycle(pwm.value).get(),
                "%",
            )?;
        }
        if let Some(distance) = delta.distance {
            fields.write_display("distance", DisplayTelemetryDistance(distance.value))?;
        }
        if let Some(pitch) = delta.pitch {
            fields.write("pitch", WheelPitchDeg::from_angle(pitch.value).get(), "deg")?;
        }

        fields.finish()
    }
}

struct TelemetryFieldWriter<'formatter, 'output> {
    output: &'formatter mut fmt::Formatter<'output>,
    prefix: Option<&'static str>,
    empty: &'static str,
    fields: FieldCount,
}

impl<'formatter, 'output> TelemetryFieldWriter<'formatter, 'output> {
    const fn new(
        output: &'formatter mut fmt::Formatter<'output>,
        prefix: Option<&'static str>,
        empty: &'static str,
    ) -> Self {
        Self {
            output,
            prefix,
            empty,
            fields: FieldCount::empty(),
        }
    }

    fn write<T: fmt::Display>(&mut self, name: &str, value: T, unit: &str) -> fmt::Result {
        self.write_prefix(name)?;
        write!(self.output, "{value}{unit}")
    }

    fn write_display<T: fmt::Display>(&mut self, name: &str, value: T) -> fmt::Result {
        self.write_prefix(name)?;
        write!(self.output, "{value}")
    }

    fn write_prefix(&mut self, name: &str) -> fmt::Result {
        if self.fields.is_empty() {
            if let Some(prefix) = self.prefix {
                write!(self.output, "{prefix} {name}=")?;
            } else {
                write!(self.output, "{name}=")?;
            }
        } else {
            write!(self.output, " {name}=")?;
        }
        self.fields = self.fields.increment();
        Ok(())
    }

    fn finish(self) -> fmt::Result {
        if self.fields.is_empty() {
            if let Some(prefix) = self.prefix {
                write!(self.output, "{prefix} {}", self.empty)
            } else {
                write!(self.output, "{}", self.empty)
            }
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct FieldCount(usize);

impl FieldCount {
    const fn empty() -> Self {
        Self(0)
    }

    const fn is_empty(self) -> bool {
        self.0 == 0
    }

    const fn increment(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

struct OptionalPowerDisplay(Option<DisplayPower>);

impl fmt::Display for OptionalPowerDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => value.fmt(f),
            None => f.write_str("unknown"),
        }
    }
}

struct OptionalOperationalCurrentDisplay(Option<OperationalCurrent>);

impl fmt::Display for OptionalOperationalCurrentDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => value.fmt(f),
            None => f.write_str("unknown"),
        }
    }
}

struct DisplayTelemetryDistance(Distance);

impl DisplayTelemetryDistance {
    #[cfg(test)]
    fn from_millimetres(value: u64) -> Self {
        Self(Distance::from_millimetres(value))
    }
}

impl fmt::Display for DisplayTelemetryDistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meters = self.0.as_whole_metres();
        if meters < 1_000 {
            return write!(f, "{meters} m");
        }

        let km_tenths = self.0.as_kilometre_tenths();
        write!(f, "{}.{} km", km_tenths / 10, km_tenths % 10)
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
        SessionBridgeEvent::NotificationIngest { .. } => {
            unreachable!("notification ingest events are routed through NotificationIngestLog")
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NotificationIngestLog {
    monotonic_ms: u64,
    outcome: NotificationIngestOutcome,
}

impl NotificationIngestLog {
    const fn level(self) -> &'static str {
        match self.outcome {
            NotificationIngestOutcome::SemanticEvents { .. }
            | NotificationIngestOutcome::KnownReserved { .. } => "info",
            NotificationIngestOutcome::ParserDiagnostic { .. }
            | NotificationIngestOutcome::ParserGap { .. } => "warn",
            NotificationIngestOutcome::BufferedFragment(_)
            | NotificationIngestOutcome::Ignored(_) => "trace",
        }
    }
}

impl fmt::Display for NotificationIngestLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.outcome {
            NotificationIngestOutcome::SemanticEvents {
                notification,
                event_count,
            } => write!(
                f,
                "t={}ms protocol semantic events family={} events={} len={}",
                self.monotonic_ms,
                family_name(notification.family),
                event_count.as_events(),
                notification.len.as_bytes()
            ),
            NotificationIngestOutcome::BufferedFragment(notification) => write!(
                f,
                "t={}ms protocol buffered fragment family={} len={}",
                self.monotonic_ms,
                family_name(notification.family),
                notification.len.as_bytes()
            ),
            NotificationIngestOutcome::ParserDiagnostic {
                notification,
                error,
            } => write!(
                f,
                "t={}ms protocol parser diagnostic family={} len={} error={error:?}",
                self.monotonic_ms,
                family_name(notification.family),
                notification.len.as_bytes()
            ),
            NotificationIngestOutcome::KnownReserved {
                notification,
                payload,
            } => write!(
                f,
                "t={}ms protocol known reserved family={} selector={} tag={} body_len={} verification={} len={}",
                self.monotonic_ms,
                family_name(notification.family),
                OptionalU8(
                    payload
                        .classifier
                        .selector_value()
                        .map(cutout_core::ProtocolSelector::get)
                ),
                OptionalU16(
                    payload
                        .classifier
                        .tag_value()
                        .map(cutout_core::ProtocolTag::get)
                ),
                payload.body_len.as_bytes(),
                verification_name(payload.verification),
                notification.len.as_bytes()
            ),
            NotificationIngestOutcome::ParserGap { notification, gap } => write!(
                f,
                "t={}ms protocol parser gap family={} selector={} tag={} body_len={} len={}",
                self.monotonic_ms,
                family_name(notification.family),
                OptionalU8(
                    gap.classifier
                        .selector_value()
                        .map(cutout_core::ProtocolSelector::get)
                ),
                OptionalU16(
                    gap.classifier
                        .tag_value()
                        .map(cutout_core::ProtocolTag::get)
                ),
                gap.body_len.as_bytes(),
                notification.len.as_bytes()
            ),
            NotificationIngestOutcome::Ignored(notification) => write!(
                f,
                "t={}ms protocol ignored notification family={} len={}",
                self.monotonic_ms,
                family_name(notification.family),
                notification.len.as_bytes()
            ),
        }
    }
}

struct OptionalU8(Option<u8>);

impl fmt::Display for OptionalU8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(f, "{value}"),
            None => write!(f, "none"),
        }
    }
}

struct OptionalU16(Option<u16>);

impl fmt::Display for OptionalU16 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(f, "{value}"),
            None => write!(f, "none"),
        }
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

fn format_read_only_response(response: ReadOnlyResponse) -> String {
    match response {
        ReadOnlyResponse::Firmware(firmware) => {
            format!("read-only firmware {}", FirmwareSummary(firmware))
        }
        ReadOnlyResponse::Settings(settings) => SettingsReadbackLog(settings).to_string(),
        ReadOnlyResponse::Battery(payload) => {
            let page = payload.page();
            let mut summary = format!(
                "read-only battery selector={} kind={} verification={}",
                page.selector,
                battery_page_kind_name(page.kind),
                verification_name(page.verification)
            );
            let _ = write!(
                summary,
                "{}{}",
                BmsTemperatureValues(payload),
                BmsCurrentSummary(payload)
            );
            summary
        }
        ReadOnlyResponse::Diagnostics(diagnostics) => {
            let populated = PopulatedDiagnosticDetailCount::from_diagnostics(diagnostics);
            format!("read-only diagnostics details={populated}")
        }
        ReadOnlyResponse::RawTelemetry(raw) => {
            let populated = PopulatedRawTelemetryFieldCount::from_raw_telemetry(raw);
            format!("read-only raw telemetry fields={populated}")
        }
    }
}

struct SettingsReadbackLog(SettingsReadback);

impl fmt::Display for SettingsReadbackLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut wrote = false;
        for entry in self.0.entries.into_iter().flatten() {
            if wrote {
                write!(f, " ")?;
            } else {
                write!(f, "read-only settings ")?;
            }
            wrote = true;
            write!(
                f,
                "field={} value={} quality={} verification={}",
                entry.field.id,
                entry.field.value,
                quality_name(entry.quality),
                verification_name(entry.verification)
            )?;
        }

        if wrote {
            Ok(())
        } else {
            write!(f, "read-only settings none observed")
        }
    }
}

fn report_has_no_parsed_events(report: &SessionBridgeReport) -> bool {
    report.telemetry.has_no_events()
        && report.read_only_responses.has_no_events()
        && report.diagnostics.has_no_events()
        && report.diagnostic_errors.is_empty()
        && report.read_only_response_events.is_empty()
}

fn format_telemetry_delta(delta: TelemetryDelta) -> String {
    TelemetryDeltaLog(delta).to_string()
}

fn format_unmapped_telemetry_event(report: &SessionBridgeReport) -> String {
    let diagnostics = format_parser_diagnostics(report.diagnostics_snapshot);
    format!(
        "telemetry unmapped notifications={} bytes={} latest_len={} diagnostics={} {}",
        report.notifications,
        report.notification_bytes.as_bytes(),
        OptionalNotificationLen(report.latest_notification_len),
        report.diagnostics,
        diagnostics
    )
}

struct OptionalNotificationLen(Option<NotificationByteLen>);

impl fmt::Display for OptionalNotificationLen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(len) => len.fmt(f),
            None => f.write_str("none"),
        }
    }
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
    CompactServicesSummary(summary.services.as_slice()).to_string()
}

struct CompactServicesSummary<'services>(&'services [ServiceSummary]);

impl fmt::Display for CompactServicesSummary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut services = self.0.iter();
        let Some(first) = services.next() else {
            return write!(f, "none discovered");
        };

        write_service_summary(f, first)?;
        for service in services {
            write!(f, ", ")?;
            write_service_summary(f, service)?;
        }
        Ok(())
    }
}

fn write_service_summary(f: &mut fmt::Formatter<'_>, service: &ServiceSummary) -> fmt::Result {
    write!(
        f,
        "{}:{} chars",
        service.uuid,
        service.characteristics.len()
    )
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
                input => state.handle_input(input),
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
                b'\r' | b'\n' => {
                    let _ = tx.send(DashboardInput::Enter);
                }
                b'\t' => {
                    let _ = tx.send(DashboardInput::NextTab);
                }
                b'j' | b'J' => {
                    let _ = tx.send(DashboardInput::MoveDown);
                }
                b'k' | b'K' => {
                    let _ = tx.send(DashboardInput::MoveUp);
                }
                b'b' | b'B' | 0x7f => {
                    let _ = tx.send(DashboardInput::Back);
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
        [b'[', b'B'] => {
            let _ = tx.send(DashboardInput::MoveDown);
        }
        [b'[', b'A'] => {
            let _ = tx.send(DashboardInput::MoveUp);
        }
        _ => {}
    }
}

pub(crate) fn render_dashboard(frame: &mut Frame<'_>, state: &DashboardState) {
    let active_tab = state.active_tab.bounded();
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
    .select(state.active_tab.bounded())
    .block(Block::bordered().title("Cutout dashboard"))
    .highlight_style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    frame.render_widget(tabs, area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    if let Some(selection) = state.active_profile_dashboard
        && state.active_tab.bounded() == 2
    {
        render_operational_dashboard(frame, area, state, selection);
        return;
    }

    match state.active_tab.bounded() {
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
        .enumerate()
        .map(|(index, profile)| {
            let row = Row::new(vec![
                Cell::from(profile.name.as_str()),
                Cell::from(profile.source.as_str()),
                Cell::from(profile.status.as_str()),
                Cell::from(profile.family.summary()),
            ]);
            if index == state.profile_selection.get() {
                row.style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            } else {
                row
            }
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

fn render_operational_dashboard(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &DashboardState,
    selection: ProfileSelection,
) {
    let profile = selection
        .bounded(state.profiles.len())
        .and_then(|selection| state.profiles.get(selection.get()));
    let profile_heading = operational_profile_heading(state, profile);
    let firmware = state.read_only.firmware.map_or_else(
        || state.device.firmware.clone(),
        |firmware| FirmwareSummary(firmware).to_string(),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(12), Constraint::Fill(1)])
        .split(area);

    let dashboard = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Fill(1)])
        .split(chunks[0]);
    let summary = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(dashboard[1]);

    render_operational_heading(frame, dashboard[0], &profile_heading);
    render_operational_speed_panel(frame, summary[0], state, &firmware);
    render_operational_metric_grid(frame, summary[1], state);
    render_read_only_summary(frame, chunks[1], state);
}

fn render_operational_heading(frame: &mut Frame<'_>, area: Rect, profile_heading: &str) {
    let heading = Paragraph::new(Line::from(Span::styled(
        profile_heading.to_owned(),
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )))
    .block(panel_block("Operational dashboard"))
    .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(heading, area);
}

fn render_operational_speed_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &DashboardState,
    firmware: &str,
) {
    let speed = OptionalDisplay(state.telemetry.latest_speed).to_string();
    let lines = vec![
        Line::from(vec![Span::styled(
            speed,
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled("device ", Style::new().fg(Color::Gray)),
            Span::raw(state.device.name.as_str()),
        ]),
        Line::from(vec![
            Span::styled("state ", Style::new().fg(Color::Gray)),
            Span::raw(state.device.connection_state.as_str()),
        ]),
        Line::from(vec![
            Span::styled("source ", Style::new().fg(Color::Gray)),
            Span::raw(state.provenance.as_deref().unwrap_or("live")),
        ]),
        Line::from(vec![
            Span::styled("firmware ", Style::new().fg(Color::Gray)),
            Span::raw(firmware.to_owned()),
        ]),
    ];

    let panel = Paragraph::new(lines)
        .block(panel_block("Speed"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn render_operational_metric_grid(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    render_operational_top_metrics(frame, rows[0], state);
    render_operational_middle_metrics(frame, rows[1], state);
    render_operational_bottom_metrics(frame, rows[2], state);
}

fn render_operational_top_metrics(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    render_metric_tile(
        frame,
        top[0],
        "Battery",
        OptionalBatteryLevelDisplay(state.telemetry.battery_level).to_string(),
        state
            .telemetry
            .battery_level
            .map(DashboardBatteryLevel::ratio),
        Color::Green,
    );
    render_metric_tile(
        frame,
        top[1],
        "Voltage",
        OptionalDisplay(state.telemetry.latest_voltage).to_string(),
        operational_voltage_ratio(state),
        Color::Magenta,
    );
    render_metric_tile(
        frame,
        top[2],
        "Signal",
        format!("{}%", state.telemetry.signal_quality),
        Some(state.telemetry.signal_quality.ratio()),
        Color::Cyan,
    );
}

fn render_operational_middle_metrics(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    render_metric_tile(
        frame,
        middle[0],
        "Wheel state",
        operational_wheel_state(state).label().to_owned(),
        None,
        Color::Green,
    );
    render_metric_tile(
        frame,
        middle[1],
        "Power",
        OptionalPowerDisplay(state.telemetry.latest_power).to_string(),
        None,
        Color::Yellow,
    );
    render_metric_tile(
        frame,
        middle[2],
        "Amps",
        OptionalOperationalCurrentDisplay(operational_current(state)).to_string(),
        None,
        Color::LightBlue,
    );
    render_metric_tile(
        frame,
        middle[3],
        "Temp",
        OptionalDisplay(state.telemetry.latest_temperature).to_string(),
        None,
        Color::Red,
    );
}

fn render_operational_bottom_metrics(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    render_metric_tile(
        frame,
        bottom[0],
        "PWM",
        operational_pwm(state).to_string(),
        None,
        Color::Yellow,
    );
    render_metric_tile(
        frame,
        bottom[1],
        "Trip",
        OptionalDisplayDistance(state.telemetry.latest_distance).to_string(),
        None,
        Color::Cyan,
    );
    render_metric_tile(
        frame,
        bottom[2],
        "Read-only mode",
        "active".to_owned(),
        None,
        Color::DarkGray,
    );
}

fn render_metric_tile(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    value: String,
    ratio: Option<f64>,
    color: Color,
) {
    if let Some(ratio) = ratio {
        let gauge = Gauge::default()
            .block(panel_block(title))
            .gauge_style(Style::new().fg(color).bg(Color::Black))
            .label(value)
            .ratio(ratio.clamp(0.0, 1.0));
        frame.render_widget(gauge, area);
        return;
    }

    let panel = Paragraph::new(vec![Line::from(Span::styled(
        value,
        Style::new().fg(color).add_modifier(Modifier::BOLD),
    ))])
    .block(panel_block(title))
    .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn operational_voltage_ratio(state: &DashboardState) -> Option<f64> {
    let voltage = state.telemetry.latest_voltage?;
    let voltage_range = dashboard_voltage_range(state)?;
    let percent = voltage.0.percent_of_range(&voltage_range);
    Some(to_f64(percent) / 100.0)
}

fn operational_wheel_state(state: &DashboardState) -> OperationalWheelState {
    match operational_charge_state(state) {
        OperationalChargeState::Charging => return OperationalWheelState::Charging,
        OperationalChargeState::NotCharging | OperationalChargeState::Unknown => {}
    }

    let speed = state.telemetry.latest_speed;
    let current = operational_current(state);
    let pitch = state.telemetry.latest_pitch;

    if speed.is_some_and(DisplaySpeed::is_moving) {
        return OperationalWheelState::Riding;
    }

    if pitch.is_some_and(WheelPitchDeg::is_lifted_or_tilted) {
        return OperationalWheelState::Lifted;
    }

    match (speed, current) {
        (Some(speed), Some(current)) if speed.is_stationary() && current.is_working() => {
            OperationalWheelState::Balancing
        }
        (Some(speed), Some(current)) if speed.is_stationary() && current.is_idle() => {
            OperationalWheelState::Parked
        }
        _ => OperationalWheelState::Unknown,
    }
}

fn operational_charge_state(state: &DashboardState) -> OperationalChargeState {
    state
        .read_only
        .settings
        .iter()
        .find_map(|setting| {
            (setting.field.id == VETERAN_FIELD_CHARGE_MODE).then_some({
                if setting.field.value == 0 {
                    OperationalChargeState::NotCharging
                } else {
                    OperationalChargeState::Charging
                }
            })
        })
        .unwrap_or(OperationalChargeState::Unknown)
}

fn operational_current(state: &DashboardState) -> Option<OperationalCurrent> {
    state
        .telemetry
        .latest_battery_current
        .map(OperationalCurrent::Charge)
        .or_else(|| {
            state
                .telemetry
                .latest_phase_current
                .map(OperationalCurrent::PhaseFallback)
        })
}

fn operational_pwm(state: &DashboardState) -> OperationalDutyCycle {
    state
        .telemetry
        .latest_pwm
        .map_or(OperationalDutyCycle::Unknown, OperationalDutyCycle::Known)
}

fn operational_profile_heading(
    state: &DashboardState,
    profile: Option<&ProfileSnapshot>,
) -> String {
    let identity = operational_device_identity(state);
    let Some(profile) = profile else {
        return format!("{identity} Profile Not Selected");
    };
    let protocol = operational_protocol_label(&profile.family);
    let status = operational_profile_status(profile);
    format!("{identity} ({protocol}) {status}")
}

fn operational_device_identity(state: &DashboardState) -> String {
    if state.device.make == "unknown" || state.device.model == "unknown" {
        return state.device.name.clone();
    }
    if state
        .device
        .model
        .to_ascii_lowercase()
        .starts_with(&state.device.make.to_ascii_lowercase())
    {
        return state.device.model.clone();
    }
    format!("{} {}", state.device.make, state.device.model)
}

fn operational_protocol_label(family: &ProfileFamily) -> &'static str {
    match family {
        ProfileFamily::AeroVeteran { .. } => "via Veteran Protocol",
        ProfileFamily::Pending { .. } => "profile pending",
    }
}

fn operational_profile_status(profile: &ProfileSnapshot) -> String {
    match &profile.family {
        ProfileFamily::AeroVeteran { summary, .. } => summary
            .strip_prefix("Aero/Veteran ")
            .unwrap_or(summary)
            .split_whitespace()
            .map(title_case_word)
            .collect::<Vec<_>>()
            .join(" "),
        ProfileFamily::Pending { summary, .. } => summary
            .split_whitespace()
            .map(title_case_word)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn title_case_word(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut output = String::new();
    output.extend(first.to_uppercase());
    output.push_str(chars.as_str());
    output
}

fn render_battery_cluster(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_battery_gauge(
        frame,
        chunks[0],
        state.telemetry.battery_level,
        state.telemetry.battery_source,
        state.telemetry.latest_voltage,
    );
    render_voltage_sparkline(frame, chunks[1], state);
}

fn render_battery_gauge(
    frame: &mut Frame<'_>,
    area: Rect,
    battery_level: Option<DashboardBatteryLevel>,
    source: BatterySource,
    latest_voltage: Option<DisplayVoltage>,
) {
    if let Some(battery_level) = battery_level {
        let battery = Gauge::default()
            .block(Block::bordered().title(source.label()))
            .gauge_style(Style::new().fg(Color::Green).bg(Color::Black))
            .ratio(battery_level.ratio());
        frame.render_widget(battery, area);
    } else {
        let message = latest_voltage.map_or_else(
            || "unknown".to_owned(),
            |voltage| format!("voltage {voltage} / battery unknown"),
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
        .label(format!("{}% / {rssi}", state.telemetry.signal_quality))
        .ratio(state.telemetry.signal_quality.ratio());
    frame.render_widget(signal, area);
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
        .latest_voltage
        .map_or_else(|| "unknown".to_owned(), |voltage| voltage.to_string());
    state.telemetry.battery_level.map_or_else(
        || format!("Voltage {voltage}"),
        |percent| format!("Voltage {voltage} / {percent}%"),
    )
}

fn voltage_sparkline_max(state: &DashboardState) -> u64 {
    dashboard_voltage_range(state)
        .map_or(100, |range| range.end().as_whole_volts())
        .max(
            state
                .telemetry
                .latest_voltage
                .map_or(0, DisplayVoltage::get),
        )
        .max(1)
}

fn voltage_sparkline_data(state: &DashboardState) -> ([u64; HISTORY_LIMIT], usize, u64) {
    let mut data = [0; HISTORY_LIMIT];
    let len = state.telemetry.voltage_samples.len().min(HISTORY_LIMIT);

    if let Some(voltage_range) = dashboard_voltage_range(state) {
        for (slot, voltage_samples) in data.iter_mut().zip(state.telemetry.voltage_samples.iter()) {
            *slot = voltage_samples.percent_of_range(&voltage_range);
        }
        return (data, len, 100);
    }

    for (slot, voltage_samples) in data.iter_mut().zip(state.telemetry.voltage_samples.iter()) {
        *slot = voltage_samples.as_whole_volts();
    }
    (data, len, voltage_sparkline_max(state))
}

fn speed_sparkline_data(state: &DashboardState) -> ([u64; HISTORY_LIMIT], usize) {
    let mut data = [0; HISTORY_LIMIT];
    let len = state.telemetry.speed_samples.len().min(HISTORY_LIMIT);

    for (slot, speed_sample) in data.iter_mut().zip(state.telemetry.speed_samples.iter()) {
        *slot = speed_sample.as_mph();
    }

    (data, len)
}

fn dashboard_voltage_range(state: &DashboardState) -> Option<RangeInclusive<Voltage>> {
    if ModelCatalog::new(&MODEL_CATALOG)
        .find_model_names(&state.device.make, &state.device.model)
        .is_some_and(|entry| entry.registration.session == Some(NOSFET_AERO_SESSION_KEY))
        && let Some(profile) = VeteranModelProfile::from_model_id(43)
    {
        return Some(profile.voltage_range);
    }

    None
}

fn render_telemetry(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    if state.telemetry.has_decoded_samples() {
        let (speed_samples, speed_len) = speed_sparkline_data(state);
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
            &speed_samples[..speed_len],
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
        read_only.firmware.map_or_else(
            || Span::raw("unknown"),
            |firmware| Span::raw(FirmwareSummary(firmware).to_string()),
        ),
    ]));

    if let Some(temperature) = read_only.latest_bms_temperature {
        lines.push(Line::from(vec![
            Span::styled("bms temp ", Style::new().fg(Color::Gray)),
            Span::raw(LatestBmsTemperatureSummary(temperature).to_string()),
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
            lines.push(Line::from(vec![Span::raw(
                SettingsEntrySummary(*setting).to_string(),
            )]));
        }
    }

    lines.push(Line::from(vec![Span::styled(
        "bms pages",
        Style::new().fg(Color::Gray),
    )]));
    for page in read_only.bms_pages.iter().rev().take(4) {
        lines.push(Line::from(vec![Span::raw(
            BmsPageSummary(*page).to_string(),
        )]));
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
            Span::raw(state.counters.notifications.as_events().to_string()),
            Span::styled(" bytes ", Style::new().fg(Color::Gray)),
            Span::raw(state.counters.notification_bytes.as_bytes().to_string()),
        ]),
    ])
    .block(panel_block("Decoded telemetry"));
    frame.render_widget(text, area);
}

fn render_pending_telemetry_detail(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let text = Paragraph::new(vec![
        Line::from("waiting for protocol decoder output"),
        Line::from(vec![
            Span::styled("latest notification bytes ", Style::new().fg(Color::Gray)),
            Span::raw(OptionalNotificationLen(state.counters.latest_notification_len).to_string()),
        ]),
    ])
    .block(panel_block("Decoder input"));
    frame.render_widget(text, area);
}

fn render_pending_telemetry_wait(frame: &mut Frame<'_>, area: Rect) {
    let text = Paragraph::new(vec![
        Line::from("waiting for protocol decoder output"),
        Line::from("transport notifications are arriving from the connected device"),
        Line::from("decoded speed, voltage, current, power, and temperature will fill in here"),
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
                Span::raw(OptionalDisplay(state.telemetry.latest_speed).to_string()),
                Span::styled(" voltage ", Style::new().fg(Color::Gray)),
                Span::raw(OptionalDisplay(state.telemetry.latest_voltage).to_string()),
                Span::styled(" battery ", Style::new().fg(Color::Gray)),
                Span::raw(OptionalBatteryLevelDisplay(state.telemetry.battery_level).to_string()),
            ]),
            Line::from(vec![
                Span::styled("amps ", Style::new().fg(Color::Gray)),
                Span::raw(
                    OptionalOperationalCurrentDisplay(operational_current(state)).to_string(),
                ),
                Span::styled(" power ", Style::new().fg(Color::Gray)),
                Span::raw(OptionalPowerDisplay(state.telemetry.latest_power).to_string()),
            ]),
            Line::from(vec![
                Span::styled(" temp ", Style::new().fg(Color::Gray)),
                Span::raw(OptionalDisplay(state.telemetry.latest_temperature).to_string()),
                Span::styled(" pwm ", Style::new().fg(Color::Gray)),
                Span::raw(OptionalDisplay(state.telemetry.latest_pwm).to_string()),
                Span::styled(" distance ", Style::new().fg(Color::Gray)),
                Span::raw(OptionalDisplayDistance(state.telemetry.latest_distance).to_string()),
                Span::styled(" pitch ", Style::new().fg(Color::Gray)),
                Span::raw(OptionalDisplay(state.telemetry.latest_pitch).to_string()),
            ]),
        ]);
    }

    let panel = Paragraph::new(lines).block(panel_block("Session / telemetry"));
    frame.render_widget(panel, area);
}

struct OptionalDisplay<T>(Option<T>);

impl<T> fmt::Display for OptionalDisplay<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(value) => value.fmt(f),
            None => f.write_str("unknown"),
        }
    }
}

struct OptionalBatteryLevelDisplay(Option<DashboardBatteryLevel>);

impl fmt::Display for OptionalBatteryLevelDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(percent) => write!(f, "{percent}%"),
            None => f.write_str("unknown"),
        }
    }
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

fn to_f64(value: u64) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn index_to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use std::{fmt::Write as _, io::Cursor};

    use super::*;
    use cutout_btle::{
        ConnectionTarget, DiagnosticEventCount, DisconnectCount, NotificationCount,
        NotificationPayloadTotal, PeripheralObservation, ProtocolWriteCount, ReadOnlyResponseCount,
        SubscribeCount, TelemetryEventCount, TransportWriteCount,
    };
    use cutout_core::{
        BatteryInfo, BatteryPageKind, BatteryPageMetadata, BatteryPagePayload, DiagnosticDetail,
        DiagnosticSeverity, FirmwareInfo, GattChannel, Measured, MonotonicTimestamp,
        NotificationByteLen, NotificationIngestOutcome, ParserDiagnosticCount, ParserError,
        ParserGapEvidence, PayloadBodyLen, ProtocolFamily, ProtocolSelector, RawFieldValue,
        ReadOnlyResponse, ReservedPayloadEvidence, SettingsEntry, SettingsReadback, SignalStrength,
        TelemetrySnapshot, ValueQuality, ValueSource, VerificationStatus,
    };
    use cutout_protocols::{VeteranFrame, VeteranTelemetry};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    const fn ms(value: u64) -> MonotonicTimestamp {
        MonotonicTimestamp::new(value)
    }

    const fn rssi(value: i16) -> SignalStrength {
        SignalStrength::from_dbm(value)
    }

    const fn protocol_writes(value: usize) -> ProtocolWriteCount {
        ProtocolWriteCount::from_events(value)
    }

    const fn writes(value: usize) -> TransportWriteCount {
        TransportWriteCount::from_events(value)
    }

    const fn subscribes(value: usize) -> SubscribeCount {
        SubscribeCount::from_events(value)
    }

    const fn notifications(value: usize) -> NotificationCount {
        NotificationCount::from_events(value)
    }

    const fn telemetry_events(value: usize) -> TelemetryEventCount {
        TelemetryEventCount::from_events(value)
    }

    const fn read_only_responses(value: usize) -> ReadOnlyResponseCount {
        ReadOnlyResponseCount::from_events(value)
    }

    const fn diagnostic_events(value: usize) -> DiagnosticEventCount {
        DiagnosticEventCount::from_events(value)
    }

    const fn parser_diag_count(value: u64) -> ParserDiagnosticCount {
        ParserDiagnosticCount::from_events(value)
    }

    const fn disconnects(value: usize) -> DisconnectCount {
        DisconnectCount::from_events(value)
    }

    const fn sel(value: u8) -> ProtocolSelector {
        ProtocolSelector::new(value)
    }

    fn speed(value: i32) -> Measured<Speed> {
        Measured::reported(Speed::from_millimetres_per_second(value))
    }

    fn display_speed_mph(value: u64) -> DisplaySpeed {
        DisplaySpeed::from_speed(Speed::from_mph(value))
    }

    fn voltage(value: i32) -> Measured<Voltage> {
        Measured::reported(Voltage::from_millivolts(value))
    }

    fn display_voltage_volts(value: u64) -> DisplayVoltage {
        DisplayVoltage::from_voltage(Voltage::from_volts(value))
    }

    fn battery_current(value: i32) -> Measured<BatteryCurrent> {
        Measured::reported(BatteryCurrent::from_milliamps(value))
    }

    fn power(value: i64) -> Measured<Power> {
        Measured::calculated(Power::from_milliwatts(value))
    }

    fn temperature(value: i32) -> Measured<Temperature> {
        Measured::reported(Temperature::from_millicelsius(value))
    }

    fn display_temperature_celsius(value: i64) -> DisplayTemperature {
        DisplayTemperature::from_temperature(Temperature::from_celsius(value))
    }

    fn assert_typed_telemetry_history(
        state: &DashboardState,
        current: Current,
        temperature: Temperature,
    ) {
        assert_eq!(state.telemetry.current_samples, vec![current]);
        assert_eq!(state.telemetry.temperature_samples, vec![temperature]);
    }

    fn telemetry_snapshot_report() -> SessionBridgeReport {
        SessionBridgeReport {
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(1),
            notifications: notifications(2),
            notification_bytes: NotificationPayloadTotal::from_bytes(200),
            latest_notification_len: Some(NotificationByteLen::from_bytes(100)),
            telemetry: telemetry_events(1),
            telemetry_snapshot: TelemetrySnapshot {
                speed: Some(speed(4_470)),
                voltage: Some(voltage(84_400)),
                battery_current: Some(battery_current(-12_400)),
                power: Some(power(-1_046_560)),
                controller_temperature: Some(temperature(36_600)),
                battery_level_reported: Some(level_reported(77)),
                ..TelemetrySnapshot::default()
            },
            read_only_responses: read_only_responses(0),
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(0),
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(42),
                delta: TelemetryDelta {
                    speed: Some(speed(4_470)),
                    voltage: Some(voltage(84_400)),
                    battery_current: Some(battery_current(-12_400)),
                    power: Some(power(-1_046_560)),
                    controller_temperature: Some(temperature(36_600)),
                    battery_level_reported: Some(level_reported(77)),
                    ..TelemetryDelta::empty(ms(42))
                },
            }],
            disconnects: disconnects(0),
        }
    }

    fn estimated_battery_report() -> SessionBridgeReport {
        SessionBridgeReport {
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(1),
            notifications: notifications(1),
            notification_bytes: NotificationPayloadTotal::from_bytes(20),
            latest_notification_len: Some(NotificationByteLen::from_bytes(20)),
            telemetry: telemetry_events(1),
            telemetry_snapshot: live_aero_telemetry_snapshot(),
            read_only_responses: read_only_responses(0),
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(0),
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(42),
                delta: TelemetryDelta {
                    speed: Some(speed(0)),
                    voltage: Some(voltage(108_760)),
                    motor_current: Some(battery_current(0)),
                    controller_temperature: Some(temperature(33_270)),
                    pwm: Some(duty_cycle_permille(-1_000)),
                    distance: Some(distance(1_551_169_000)),
                    pitch: Some(angle_mdeg(69_060)),
                    battery_level_estimated: Some(level_estimated(47)),
                    ..TelemetryDelta::empty(ms(42))
                },
            }],
            disconnects: disconnects(0),
        }
    }

    fn duty_cycle_permille(value: i16) -> Measured<DutyCycle> {
        Measured::reported(DutyCycle::from_permille(value))
    }

    fn distance(value: u64) -> Measured<Distance> {
        Measured::reported(Distance::from_millimetres(value))
    }

    fn angle_mdeg(value: i32) -> Measured<Angle> {
        Measured::reported(Angle::from_millidegrees(value))
    }

    fn level_reported(value: u8) -> Measured<BatteryLevel> {
        Measured::reported(BatteryLevel::from_percent(value))
    }

    fn level_estimated(value: u8) -> Measured<BatteryLevel> {
        Measured::estimated(BatteryLevel::from_percent(value))
    }

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
            .to_delta(ms(42));
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(delta);
        snapshot
    }

    fn snapshot_from_delta(delta: TelemetryDelta) -> TelemetrySnapshot {
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

    fn firmware_43_2_54() -> FirmwareInfo {
        FirmwareInfo {
            firmware_major: Some(Measured::reported(43)),
            firmware_minor: Some(Measured::reported(2)),
            firmware_patch: Some(Measured::reported(54)),
            ..FirmwareInfo::default()
        }
    }

    fn firmware_summary_text(state: &DashboardState) -> Option<String> {
        state
            .read_only
            .firmware
            .map(FirmwareSummary)
            .map(|firmware| firmware.to_string())
    }

    fn hardware_setting(field: u16, value: i64) -> SettingsEntry {
        SettingsEntry {
            field: RawFieldValue::new(field, value),
            source: ValueSource::Reported,
            quality: ValueQuality::Known,
            verification: VerificationStatus::HardwareVerified,
        }
    }

    fn sample_aero_read_only_responses() -> Vec<ReadOnlyResponse> {
        vec![
            ReadOnlyResponse::Firmware(firmware_43_2_54()),
            ReadOnlyResponse::Settings(SettingsReadback {
                entries: [Some(hardware_setting(0x20, 540)), None, None, None],
            }),
            ReadOnlyResponse::Battery(BatteryPagePayload::cell_voltage(
                BatteryPageMetadata::cell_voltage(sel(2), VerificationStatus::HardwareVerified),
                BatteryInfo::default(),
            )),
            ReadOnlyResponse::Battery(BatteryPagePayload::temperature_values(
                BatteryPageMetadata::temperature(sel(3), VerificationStatus::HardwareVerified),
                BatteryInfo {
                    temperature: Some(temperature(16_730)),
                    ..BatteryInfo::default()
                },
                [
                    Some(temperature(16_730)),
                    Some(temperature(17_840)),
                    Some(temperature(18_100)),
                    Some(temperature(17_800)),
                    Some(temperature(17_700)),
                    Some(temperature(19_100)),
                ],
            )),
            ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                BatteryPageMetadata::raw(sel(8), VerificationStatus::HardwareVerified),
                BatteryInfo::default(),
            )),
            ReadOnlyResponse::RawTelemetry(RawTelemetryReadback::default()),
        ]
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

    fn assert_display_preserves_capacity(value: impl fmt::Display, expected: &str) {
        let mut output = String::with_capacity(512);
        let capacity = output.capacity();

        write!(&mut output, "{value}").expect("display writes to string");

        assert_eq!(output, expected);
        assert_eq!(output.capacity(), capacity);
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
        assert_eq!(state.active_tab, DashboardTab::first());
        assert_eq!(
            state.provenance.as_deref(),
            Some("demo state: aero-nf2557.v1")
        );
        assert_eq!(state.device.make, "NOSFET");
        assert_eq!(state.device.model, "NOSFET Aero");
        assert_eq!(state.device.name, "Aero NF2557");
        assert_eq!(state.scan_browser.selected, ScanSelection::first());
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
                current_limit: Some("45A".to_owned()),
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
            state.telemetry.current_samples.len()
        );
        assert_eq!(
            state.telemetry.temperature_points.len(),
            state.telemetry.temperature_samples.len()
        );
    }

    #[test]
    fn live_target_state_never_uses_demo_fixture_data() {
        let state = DashboardState::live_target("NF2557".to_owned());

        assert_eq!(state.source, DashboardSource::Live);
        assert_eq!(state.device.make, "NOSFET");
        assert_eq!(state.device.model, "NOSFET Aero");
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
    fn live_target_identity_comes_from_model_catalog_hints() {
        let state = DashboardState::live_target("Begode Falcon".to_owned());

        assert_eq!(state.source, DashboardSource::Live);
        assert_eq!(state.device.make, "Begode");
        assert_eq!(state.device.model, "Falcon");
        assert_eq!(state.device.name, "Begode Falcon");
        assert_eq!(state.device.connection_state, "target selected");
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
                rssi: Some(rssi(-61)),
                advertised_services: vec![].into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };

        let state = DashboardState::live_connected(&target, &summary);

        assert_eq!(state.source, DashboardSource::Live);
        assert_eq!(state.device.make, "NOSFET");
        assert_eq!(state.device.model, "NOSFET Aero");
        assert_eq!(state.device.name, "Aero NF2557");
        assert_eq!(state.device.address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(state.device.connection_state, "connected");
        assert_eq!(state.counters.connected, ConnectedDeviceCount::new(1));
        assert_eq!(state.telemetry.battery_level, None);
        assert_eq!(state.telemetry.battery_source, BatterySource::Unknown);
        assert_eq!(state.telemetry.signal_quality, SignalQuality::new(78));
        assert_eq!(state.scan_browser.observations.len(), 1);
        assert!(state.scan_browser.observations[0].real_device);
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.profiles[0].source, "gatt");

        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("78% / -61 dBm"));
        assert_eq!(
            dashboard_voltage_range(&state),
            Some(Voltage::from_millivolts(91_000)..=Voltage::from_millivolts(126_000))
        );
    }

    #[test]
    fn compact_services_summary_streams_service_entries() {
        let services = vec![
            ServiceSummary {
                uuid: uuid::Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb),
                primary: true,
                characteristics: Vec::new().into(),
            },
            ServiceSummary {
                uuid: uuid::Uuid::from_u128(0x0000ffe0_0000_1000_8000_00805f9b34fb),
                primary: true,
                characteristics: Vec::new().into(),
            },
        ];

        assert_eq!(
            CompactServicesSummary(&services).to_string(),
            "0000180f-0000-1000-8000-00805f9b34fb:0 chars, 0000ffe0-0000-1000-8000-00805f9b34fb:0 chars"
        );
        assert_eq!(CompactServicesSummary(&[]).to_string(), "none discovered");
    }

    #[test]
    fn signal_quality_clamps_to_reasonable_ble_range() {
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-40)),
            SignalQuality::new(100)
        );
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-50)),
            SignalQuality::new(100)
        );
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-61)),
            SignalQuality::new(78)
        );
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-74)),
            SignalQuality::new(52)
        );
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-90)),
            SignalQuality::new(20)
        );
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-100)),
            SignalQuality::new(0)
        );
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-120)),
            SignalQuality::new(0)
        );
        assert_eq!(
            SignalQuality::from_signal_strength(rssi(-20)),
            SignalQuality::new(100)
        );
    }

    #[test]
    fn operational_wheel_state_prefers_charge_mode_readback() {
        let mut state = DashboardState::empty();
        state.telemetry.latest_speed = Some(display_speed_mph(3));
        state
            .read_only
            .settings
            .push_back(hardware_setting(VETERAN_FIELD_CHARGE_MODE, 1));

        assert_eq!(
            operational_wheel_state(&state),
            OperationalWheelState::Charging
        );
    }

    #[test]
    fn operational_wheel_state_reports_riding_from_motion() {
        let mut state = DashboardState::empty();
        state.telemetry.latest_speed = Some(display_speed_mph(1));
        state.telemetry.latest_battery_current = Some(DisplayBatteryCurrent::from_milliamps(0));
        state
            .read_only
            .settings
            .push_back(hardware_setting(VETERAN_FIELD_CHARGE_MODE, 0));

        assert_eq!(
            operational_wheel_state(&state),
            OperationalWheelState::Riding
        );
    }

    #[test]
    fn operational_wheel_state_reports_lifted_from_stationary_pitch() {
        let mut state = DashboardState::empty();
        state.telemetry.latest_speed = Some(display_speed_mph(0));
        state.telemetry.latest_battery_current = Some(DisplayBatteryCurrent::from_milliamps(0));
        state.telemetry.latest_pitch =
            Some(WheelPitchDeg::from_angle(Angle::from_millidegrees(69_000)));

        assert_eq!(
            operational_wheel_state(&state),
            OperationalWheelState::Lifted
        );
    }

    #[test]
    fn operational_wheel_state_reports_balancing_from_stationary_current() {
        let mut state = DashboardState::empty();
        state.telemetry.latest_speed = Some(display_speed_mph(0));
        state.telemetry.latest_battery_current = Some(DisplayBatteryCurrent::from_milliamps(4_000));
        state.telemetry.latest_pitch = Some(WheelPitchDeg::from_angle(Angle::from_millidegrees(0)));

        assert_eq!(
            operational_wheel_state(&state),
            OperationalWheelState::Balancing
        );
    }

    #[test]
    fn operational_wheel_state_reports_parked_from_stationary_idle_current() {
        let mut state = DashboardState::empty();
        state.telemetry.latest_speed = Some(display_speed_mph(0));
        state.telemetry.latest_battery_current = Some(DisplayBatteryCurrent::from_milliamps(0));
        state.telemetry.latest_pitch = Some(WheelPitchDeg::from_angle(Angle::from_millidegrees(0)));

        assert_eq!(
            operational_wheel_state(&state),
            OperationalWheelState::Parked
        );
    }

    #[test]
    fn operational_pwm_preserves_signed_percent_without_load_semantics() {
        let mut state = DashboardState::empty();
        assert_eq!(operational_pwm(&state), OperationalDutyCycle::Unknown);

        state.telemetry.latest_speed = Some(display_speed_mph(0));
        state.telemetry.latest_battery_current = Some(DisplayBatteryCurrent::from_milliamps(0));
        state.telemetry.latest_pwm = Some(DisplayDutyCycle::from_duty_cycle(
            DutyCycle::from_permille(-1_000),
        ));

        assert_eq!(
            operational_pwm(&state),
            OperationalDutyCycle::Known(DisplayDutyCycle::from_duty_cycle(
                DutyCycle::from_permille(-1_000),
            ))
        );
        assert_eq!(operational_pwm(&state).to_string(), "-100%");
    }

    #[test]
    fn display_power_converts_from_milliwatts_without_unit_leaking() {
        assert_eq!(DisplayPower::from_milliwatts(-184_892).get(), -184);
        assert_eq!(
            DisplayPower::from_voltage_current(
                Voltage::from_millivolts(53_000),
                BatteryCurrent::from_milliamps(6_000),
            ),
            DisplayPower::from_watts(318)
        );
        assert_eq!(
            DisplayPower::from_milliwatts(-184_892).to_string(),
            "-184 W"
        );
        assert_eq!(OptionalPowerDisplay(None).to_string(), "unknown");
    }

    #[test]
    fn operational_current_display_preserves_signed_discharge_current() {
        let current = DisplayBatteryCurrent::from_milliamps(-12_400);

        assert_eq!(current.get(), -12);
        assert_eq!(current.to_string(), "-12 A");
        assert_eq!(
            OptionalOperationalCurrentDisplay(Some(OperationalCurrent::Charge(current)))
                .to_string(),
            "-12 A"
        );
        assert_eq!(
            OptionalOperationalCurrentDisplay(None).to_string(),
            "unknown"
        );
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
        assert_eq!(
            DisplayDistance::from_millimetres(999_000).to_string(),
            "999 m"
        );
        assert_eq!(
            DisplayTelemetryDistance::from_millimetres(1_551_169_000).to_string(),
            "1551.2 km"
        );
        assert_eq!(
            OptionalDisplayDistance(Some(Distance::from_millimetres(1_551_169_000))).to_string(),
            "1551.2 km"
        );
        assert_eq!(
            DisplayDistance::from_millimetres(1_550_438_000).to_string(),
            "1550.4 km"
        );
        assert_eq!(OptionalDisplayDistance(None).to_string(), "unknown");
    }

    #[test]
    fn optional_numeric_presenters_render_without_helper_strings() {
        assert_eq!(
            OptionalDisplay(Some(display_voltage_volts(47))).to_string(),
            "47 V"
        );
        assert_eq!(
            OptionalDisplay::<DisplayVoltage>(None).to_string(),
            "unknown"
        );
        assert_eq!(
            OptionalBatteryLevelDisplay(Some(DashboardBatteryLevel::new(47))).to_string(),
            "47%"
        );
        assert_eq!(OptionalBatteryLevelDisplay(None).to_string(), "unknown");
        assert_eq!(
            OptionalDisplay(Some(display_temperature_celsius(-18))).to_string(),
            "-18 C"
        );
        assert_eq!(
            OptionalDisplay::<DisplayTemperature>(None).to_string(),
            "unknown"
        );
    }

    #[test]
    fn voltage_sparkline_uses_connected_device_voltage_range() {
        let mut state = DashboardState::empty();
        state.device.make = "NOSFET".to_owned();
        state.device.model = "NOSFET Aero".to_owned();
        state.telemetry.latest_voltage = Some(display_voltage_volts(120));
        state.telemetry.voltage_samples = vec![
            Voltage::from_millivolts(109_000),
            Voltage::from_millivolts(120_000),
            Voltage::from_millivolts(126_000),
        ];
        state.telemetry.battery_level = Some(DashboardBatteryLevel::new(85));

        assert_eq!(
            dashboard_voltage_range(&state),
            Some(Voltage::from_millivolts(91_000)..=Voltage::from_millivolts(126_000))
        );
        assert_eq!(voltage_sparkline_max(&state), 126);
        assert_eq!(voltage_sparkline_title(&state), "Voltage 120 V / 85%");

        let (data, len, max) = voltage_sparkline_data(&state);
        assert_eq!(len, 3);
        assert_eq!(max, 100);
        assert_eq!(&data[..len], [51, 83, 100]);
    }

    #[test]
    fn voltage_sparkline_falls_back_to_observed_voltage_for_unknown_device() {
        let mut state = DashboardState::empty();
        state.telemetry.latest_voltage = Some(display_voltage_volts(151));
        state.telemetry.voltage_samples = vec![
            Voltage::from_millivolts(149_000),
            Voltage::from_millivolts(151_000),
        ];

        assert_eq!(dashboard_voltage_range(&state), None);
        assert_eq!(voltage_sparkline_max(&state), 151);
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
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(1),
            notifications: notifications(184),
            notification_bytes: NotificationPayloadTotal::from_bytes(18_400),
            latest_notification_len: Some(NotificationByteLen::from_bytes(100)),
            telemetry: telemetry_events(0),
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: read_only_responses(0),
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(1),
            diagnostics_snapshot: ParserDiagnostics {
                malformed_frames: parser_diag_count(2),
                unmatched_replies: parser_diag_count(1),
                ..ParserDiagnostics::default()
            },
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![
                SessionBridgeEvent::Diagnostics {
                    monotonic_ms: cutout_btle::MonotonicMs::new(18),
                    diagnostics: ParserDiagnostics {
                        malformed_frames: parser_diag_count(2),
                        unmatched_replies: parser_diag_count(1),
                        ..ParserDiagnostics::default()
                    },
                },
                SessionBridgeEvent::LinkDown {
                    monotonic_ms: cutout_btle::MonotonicMs::new(19),
                },
            ],
            disconnects: disconnects(1),
        };

        state.apply_session_report(&report);

        assert_eq!(state.counters.subscriptions, subscribes(1));
        assert_eq!(state.counters.notifications, notifications(184));
        assert_eq!(
            state.counters.notification_bytes,
            NotificationPayloadTotal::from_bytes(18_400)
        );
        assert_eq!(
            state.counters.latest_notification_len,
            Some(NotificationByteLen::from_bytes(100))
        );
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
        let report = telemetry_snapshot_report();

        state.apply_session_report(&report);

        assert_eq!(
            state.telemetry.battery_level,
            Some(DashboardBatteryLevel::new(77))
        );
        assert_eq!(
            state.telemetry.battery_source,
            BatterySource::TelemetryReported
        );
        assert_eq!(
            state.telemetry.latest_speed,
            Some(DisplaySpeed::from_speed(
                Speed::from_millimetres_per_second(4_470,)
            ))
        );
        assert_eq!(
            state.telemetry.latest_voltage,
            Some(DisplayVoltage::from_voltage(Voltage::from_millivolts(
                84_400
            )))
        );
        assert_eq!(
            state
                .telemetry
                .latest_battery_current
                .map(DisplayBatteryCurrent::get),
            Some(-12)
        );
        assert_eq!(state.telemetry.latest_phase_current, None);
        assert_eq!(
            state.telemetry.latest_power,
            Some(DisplayPower::from_milliwatts(-1_046_560))
        );
        assert_eq!(
            state.telemetry.latest_temperature,
            Some(DisplayTemperature::from_temperature(
                Temperature::from_millicelsius(36_600),
            ))
        );
        assert_eq!(
            state.telemetry.speed_samples,
            vec![Speed::from_millimetres_per_second(4_470)]
        );
        assert_eq!(state.telemetry.voltage_samples.len(), HISTORY_LIMIT);
        assert!(
            state
                .telemetry
                .voltage_samples
                .iter()
                .all(|voltage| *voltage == Voltage::from_millivolts(84_400))
        );
        assert_typed_telemetry_history(
            &state,
            Current::from_milliamps(12_400),
            Temperature::from_millicelsius(36_600),
        );
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry.message.contains("telemetry mapped")
                && entry.message.contains("speed=10mph")
                && entry.message.contains("battery=77%")
                && entry.message.contains("current=-12A")
                && entry.message.contains("power=-1046W")
        }));
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry.message.contains("t=42ms processed telemetry")
                && entry.message.contains("voltage=84V")
                && entry.message.contains("power=-1046W")
        }));
    }

    #[test]
    fn parsed_session_report_suppresses_raw_notification_log_spam() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            notifications: notifications(1),
            notification_bytes: NotificationPayloadTotal::from_bytes(99),
            latest_notification_len: Some(NotificationByteLen::from_bytes(99)),
            telemetry: telemetry_events(1),
            telemetry_snapshot: TelemetrySnapshot {
                voltage: Some(voltage(117_600)),
                battery_level_estimated: Some(level_estimated(78)),
                ..TelemetrySnapshot::default()
            },
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                delta: TelemetryDelta {
                    voltage: Some(voltage(117_600)),
                    battery_level_estimated: Some(level_estimated(78)),
                    ..TelemetryDelta::empty(ms(7))
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

        state.active_tab = DashboardTab::new(3);
        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("processed telemetry voltage=118V battery=78%"));
        assert!(!text.contains("raw notification len=99"));
        assert!(!text.contains("telemetry unmapped notifications=1"));
    }

    #[test]
    fn unparsed_session_report_summarizes_transport_without_raw_event_spam() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            notifications: notifications(3),
            notification_bytes: NotificationPayloadTotal::from_bytes(57),
            latest_notification_len: Some(NotificationByteLen::from_bytes(20)),
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

        state.active_tab = DashboardTab::new(3);
        let text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(text.contains("telemetry unmapped notifications=3"));
        assert!(!text.contains("raw notification"));
    }

    #[test]
    fn notification_ingest_log_keeps_level_structured_until_render_edge() {
        let channel = GattChannel::from_bytes([0xA1; 16]);
        let log = NotificationIngestLog {
            monotonic_ms: 4,
            outcome: NotificationIngestOutcome::known_reserved(
                ProtocolFamily::VeteranLeaperkimNosfet,
                channel,
                NotificationByteLen::from_bytes(75),
                ms(4),
                ReservedPayloadEvidence {
                    classifier: cutout_core::PayloadClassifier::selector(ProtocolSelector::new(8)),
                    body_len: PayloadBodyLen::from_bytes(24),
                    verification: VerificationStatus::HardwareVerified,
                },
            ),
        };

        assert_eq!(log.level(), "info");
        assert_eq!(
            log.to_string(),
            "t=4ms protocol known reserved family=VeteranLeaperkimNosfet selector=8 tag=none body_len=24 verification=hardware_verified len=75"
        );
    }

    #[test]
    fn mapped_telemetry_log_keeps_snapshot_structured_until_render_edge() {
        let snapshot = TelemetrySnapshot {
            speed: Some(speed(12_000)),
            voltage: Some(voltage(119_600)),
            battery_level_reported: Some(level_reported(87)),
            motor_current: Some(battery_current(-18_500)),
            power: Some(power(-2_212_600)),
            controller_temperature: Some(temperature(36_000)),
            pwm: Some(duty_cycle_permille(420)),
            distance: Some(distance(1_551_169_000)),
            pitch: Some(angle_mdeg(-2_000)),
            roll: Some(angle_mdeg(1_000)),
            ..TelemetrySnapshot::default()
        };

        assert_eq!(
            MappedTelemetryLog(snapshot).to_string(),
            "telemetry mapped speed=27mph voltage=120V battery=87% current=-18A power=-2212W temperature=36C pwm=42% distance=1551.2 km pitch=-2deg roll=1deg"
        );
        assert_eq!(
            MappedTelemetryLog(TelemetrySnapshot::default()).to_string(),
            "telemetry mapped none"
        );
    }

    #[test]
    fn settings_readback_log_streams_bounded_entries_at_render_edge() {
        let settings = SettingsReadback {
            entries: [
                Some(SettingsEntry {
                    field: RawFieldValue::new(0x20, 540),
                    source: ValueSource::Reported,
                    quality: ValueQuality::Known,
                    verification: VerificationStatus::HardwareVerified,
                }),
                None,
                Some(SettingsEntry {
                    field: RawFieldValue::new(0x21, -12),
                    source: ValueSource::Estimated,
                    quality: ValueQuality::Inferred,
                    verification: VerificationStatus::Inferred,
                }),
                None,
            ],
        };

        assert_eq!(
            SettingsReadbackLog(settings).to_string(),
            "read-only settings field=32 value=540 quality=known verification=hardware_verified field=33 value=-12 quality=inferred verification=inferred"
        );
        assert_eq!(
            SettingsReadbackLog(SettingsReadback::default()).to_string(),
            "read-only settings none observed"
        );
    }

    #[test]
    fn read_only_response_summary_counts_use_typed_units() {
        let diagnostics = DiagnosticReadback {
            details: [
                None,
                Some(DiagnosticDetail {
                    field: RawFieldValue::new(0x30, 7),
                    severity: DiagnosticSeverity::Info,
                    quality: ValueQuality::Known,
                    verification: VerificationStatus::HardwareVerified,
                }),
                None,
                None,
            ],
        };
        let raw = RawTelemetryReadback {
            fields: [
                Some(RawFieldValue::new(0x8001, 989)),
                None,
                Some(RawFieldValue::new(0x8002, -21_973)),
                None,
            ],
        };

        assert_eq!(
            PopulatedDiagnosticDetailCount::from_diagnostics(diagnostics),
            PopulatedDiagnosticDetailCount::new(1)
        );
        assert_eq!(
            PopulatedRawTelemetryFieldCount::from_raw_telemetry(raw),
            PopulatedRawTelemetryFieldCount::new(2)
        );
        assert_eq!(
            format_read_only_response(ReadOnlyResponse::Diagnostics(diagnostics)),
            "read-only diagnostics details=1"
        );
        assert_eq!(
            format_read_only_response(ReadOnlyResponse::RawTelemetry(raw)),
            "read-only raw telemetry fields=2"
        );
    }

    #[test]
    fn telemetry_delta_log_reuses_typed_field_rendering() {
        let delta = TelemetryDelta {
            speed: Some(speed(4_470)),
            voltage: Some(voltage(118_400)),
            battery_level_estimated: Some(level_estimated(78)),
            battery_current: Some(battery_current(-12_400)),
            power: Some(power(-1_468_160)),
            motor_temperature: Some(temperature(44_600)),
            distance: Some(distance(999_000)),
            pitch: Some(angle_mdeg(-3_000)),
            ..TelemetryDelta::empty(ms(42))
        };

        assert_eq!(
            TelemetryDeltaLog(delta).to_string(),
            "speed=10mph voltage=118V battery=78% current=-12A power=-1468W temperature=44C distance=999 m pitch=-3deg"
        );
        assert_eq!(
            TelemetryDeltaLog(TelemetryDelta::empty(ms(42))).to_string(),
            "unmapped"
        );
    }

    #[test]
    fn dashboard_presenters_write_into_preallocated_buffers_without_reallocating() {
        let channel = GattChannel::from_bytes([0xA1; 16]);
        let ingest = NotificationIngestLog {
            monotonic_ms: 4,
            outcome: NotificationIngestOutcome::known_reserved(
                ProtocolFamily::VeteranLeaperkimNosfet,
                channel,
                NotificationByteLen::from_bytes(75),
                ms(4),
                ReservedPayloadEvidence {
                    classifier: cutout_core::PayloadClassifier::selector(sel(8)),
                    body_len: PayloadBodyLen::from_bytes(24),
                    verification: VerificationStatus::HardwareVerified,
                },
            ),
        };
        assert_display_preserves_capacity(
            ingest,
            "t=4ms protocol known reserved family=VeteranLeaperkimNosfet selector=8 tag=none body_len=24 verification=hardware_verified len=75",
        );
        let parser_gap = NotificationIngestLog {
            monotonic_ms: 9,
            outcome: NotificationIngestOutcome::parser_gap(
                ProtocolFamily::VeteranLeaperkimNosfet,
                channel,
                NotificationByteLen::from_bytes(75),
                ms(9),
                ParserGapEvidence {
                    classifier: cutout_core::PayloadClassifier::tag(cutout_core::ProtocolTag::new(
                        0x1234,
                    )),
                    body_len: PayloadBodyLen::from_bytes(12),
                },
            ),
        };
        assert_display_preserves_capacity(
            parser_gap,
            "t=9ms protocol parser gap family=VeteranLeaperkimNosfet selector=none tag=4660 body_len=12 len=75",
        );

        assert_display_preserves_capacity(
            MappedTelemetryLog(TelemetrySnapshot {
                voltage: Some(voltage(119_600)),
                battery_level_reported: Some(level_reported(87)),
                ..TelemetrySnapshot::default()
            }),
            "telemetry mapped voltage=120V battery=87%",
        );

        assert_display_preserves_capacity(
            SettingsReadbackLog(SettingsReadback {
                entries: [Some(hardware_setting(0x20, 540)), None, None, None],
            }),
            "read-only settings field=32 value=540 quality=known verification=hardware_verified",
        );

        assert_display_preserves_capacity(
            BmsPageSummary(
                BatteryPagePayload::raw(
                    BatteryPageMetadata::metadata(sel(0), VerificationStatus::HardwareVerified),
                    BatteryInfo {
                        current: Some(battery_current(2_010)),
                        ..BatteryInfo::default()
                    },
                )
                .with_bms_pack_currents(cutout_core::BmsPackCurrents::reported(
                    BatteryCurrent::from_milliamps(-1_230),
                    BatteryCurrent::from_milliamps(450),
                )),
            ),
            "selector=0 kind=metadata verification=hardware_verified current=2A bms_current_0=-1A bms_current_1=0A",
        );
    }

    #[test]
    fn ingest_outcome_events_render_each_typed_protocol_category() {
        let mut state = DashboardState::empty();
        let channel = GattChannel::from_bytes([0xA1; 16]);
        let report = SessionBridgeReport {
            notifications: notifications(5),
            notification_bytes: NotificationPayloadTotal::from_bytes(269),
            latest_notification_len: Some(NotificationByteLen::from_bytes(77)),
            events: vec![
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(3),
                    outcome: NotificationIngestOutcome::buffered_fragment(
                        ProtocolFamily::VeteranLeaperkimNosfet,
                        channel,
                        NotificationByteLen::from_bytes(20),
                        ms(3),
                    ),
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(4),
                    outcome: NotificationIngestOutcome::known_reserved(
                        ProtocolFamily::VeteranLeaperkimNosfet,
                        channel,
                        NotificationByteLen::from_bytes(75),
                        ms(4),
                        ReservedPayloadEvidence {
                            classifier: cutout_core::PayloadClassifier::selector(
                                ProtocolSelector::new(8),
                            ),
                            body_len: PayloadBodyLen::from_bytes(24),
                            verification: VerificationStatus::HardwareVerified,
                        },
                    ),
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(5),
                    outcome: NotificationIngestOutcome::parser_gap(
                        ProtocolFamily::VeteranLeaperkimNosfet,
                        channel,
                        NotificationByteLen::from_bytes(77),
                        ms(5),
                        ParserGapEvidence {
                            classifier: cutout_core::PayloadClassifier::selector(
                                ProtocolSelector::new(9),
                            ),
                            body_len: PayloadBodyLen::from_bytes(26),
                        },
                    ),
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(6),
                    outcome: NotificationIngestOutcome::parser_diagnostic(
                        ProtocolFamily::VeteranLeaperkimNosfet,
                        channel,
                        NotificationByteLen::from_bytes(77),
                        ms(6),
                        ParserError::BadChecksum,
                    ),
                },
                SessionBridgeEvent::NotificationIngest {
                    monotonic_ms: cutout_btle::MonotonicMs::new(7),
                    outcome: NotificationIngestOutcome::ignored_wrong_channel(
                        channel,
                        NotificationByteLen::from_bytes(20),
                        ms(7),
                    ),
                },
            ],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = DashboardTab::new(3);

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
            BatteryPageMetadata::temperature(sel(3), VerificationStatus::HardwareVerified),
            BatteryInfo {
                temperature: Some(temperature(17_600)),
                ..BatteryInfo::default()
            },
            [
                Some(temperature(17_600)),
                Some(temperature(17_100)),
                Some(temperature(17_700)),
                Some(temperature(18_500)),
                Some(temperature(19_000)),
                Some(temperature(19_100)),
            ],
        ));
        let report = SessionBridgeReport {
            read_only_responses: read_only_responses(1),
            read_only_response_events: vec![read_only_response],
            events: vec![SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                response: read_only_response,
            }],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = DashboardTab::new(3);

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
        let read_only_response = ReadOnlyResponse::Battery(
            BatteryPagePayload::raw(
                BatteryPageMetadata::metadata(sel(0), VerificationStatus::HardwareVerified),
                BatteryInfo {
                    current: Some(battery_current(2_010)),
                    ..BatteryInfo::default()
                },
            )
            .with_bms_pack_currents(cutout_core::BmsPackCurrents::reported(
                BatteryCurrent::from_milliamps(-1_230),
                BatteryCurrent::from_milliamps(450),
            )),
        );
        let report = SessionBridgeReport {
            read_only_responses: read_only_responses(1),
            read_only_response_events: vec![read_only_response],
            events: vec![SessionBridgeEvent::ReadOnlyResponse {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                response: read_only_response,
            }],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = DashboardTab::new(3);

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(state.logs.iter().any(|entry| {
            entry.level == "info"
                && entry
                    .message
                    .contains("read-only battery selector=0 kind=metadata")
                && entry.message.contains("current=2A")
                && entry.message.contains("bms_current_0=-1A")
                && entry.message.contains("bms_current_1=0A")
        }));
        assert!(text.contains("current=2A"));
        assert!(text.contains("bms_current_0=-1A"));
        assert!(text.contains("bms_current_1=0A"));
    }

    #[test]
    fn first_voltage_sample_seeds_the_sparkline_history() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(1),
            notifications: notifications(1),
            notification_bytes: NotificationPayloadTotal::from_bytes(20),
            latest_notification_len: Some(NotificationByteLen::from_bytes(20)),
            telemetry: telemetry_events(1),
            telemetry_snapshot: live_aero_telemetry_snapshot(),
            read_only_responses: read_only_responses(0),
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(0),
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: vec![SessionBridgeEvent::ProcessedTelemetry {
                monotonic_ms: cutout_btle::MonotonicMs::new(7),
                delta: TelemetryDelta {
                    voltage: Some(voltage(108_760)),
                    battery_level_estimated: Some(level_estimated(47)),
                    ..TelemetryDelta::empty(ms(7))
                },
            }],
            disconnects: disconnects(0),
        };

        state.apply_session_report(&report);

        assert_eq!(
            state.telemetry.latest_voltage,
            Some(DisplayVoltage::from_voltage(Voltage::from_millivolts(
                108_760
            )))
        );
        assert_eq!(state.telemetry.voltage_samples.len(), HISTORY_LIMIT);
        assert!(
            state
                .telemetry
                .voltage_samples
                .iter()
                .all(|voltage| *voltage == Voltage::from_millivolts(108_760))
        );
    }

    #[test]
    fn live_session_report_summarizes_read_only_responses() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(1),
            notifications: notifications(3),
            notification_bytes: NotificationPayloadTotal::from_bytes(300),
            latest_notification_len: Some(NotificationByteLen::from_bytes(100)),
            telemetry: telemetry_events(0),
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: read_only_responses(5),
            read_only_response_events: sample_aero_read_only_responses(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(0),
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: Vec::new(),
            disconnects: disconnects(0),
        };

        state.apply_session_report(&report);

        assert_eq!(firmware_summary_text(&state), Some("43.2.54".to_owned()));
        assert_eq!(state.read_only.settings.len(), 1);
        assert_eq!(
            state.read_only.settings[0].field,
            RawFieldValue::new(0x20, 540)
        );
        assert_eq!(
            state.read_only.settings[0].verification,
            VerificationStatus::HardwareVerified
        );
        assert_eq!(state.read_only.bms_pages.len(), 3);
        assert_eq!(
            state.read_only.bms_pages[0].page().kind,
            BatteryPageKind::CellVoltage
        );
        assert_eq!(
            state.read_only.bms_pages[1].page().kind,
            BatteryPageKind::Temperature
        );
        assert_eq!(
            BmsPageSummary(state.read_only.bms_pages[1]).to_string(),
            "selector=3 kind=temperature verification=hardware_verified temps_c=16,17,18,17,17,19"
        );
        assert_eq!(
            state.read_only.bms_pages[2].page().kind,
            BatteryPageKind::Raw
        );
        assert_eq!(
            state
                .read_only
                .latest_bms_temperature
                .map(LatestBmsTemperatureSummary)
                .map(|summary| summary.to_string()),
            Some("selector=3 verification=hardware_verified temps_c=16,17,18,17,17,19".to_owned())
        );
        assert_eq!(
            state.read_only.unknown_raw_pages,
            RawReadOnlyPageCount::new(1)
        );
        assert_eq!(
            state.read_only.raw_telemetry,
            RawTelemetryResponseCount::new(1)
        );
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
            BatteryPageMetadata::raw(sel(8), VerificationStatus::HardwareVerified),
            BatteryInfo::default(),
        ));
        let report = SessionBridgeReport {
            telemetry: telemetry_events(0),
            read_only_responses: read_only_responses(1),
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
    #[allow(clippy::too_many_lines)]
    fn live_session_report_wires_complete_aero_dashboard_state() {
        let mut state = DashboardState::empty();
        let telemetry = TelemetryDelta {
            speed: Some(speed(4_470)),
            voltage: Some(voltage(108_760)),
            battery_level_estimated: Some(level_estimated(47)),
            ..TelemetryDelta::empty(ms(42))
        };
        let report = SessionBridgeReport {
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(1),
            notifications: notifications(4),
            notification_bytes: NotificationPayloadTotal::from_bytes(400),
            latest_notification_len: Some(NotificationByteLen::from_bytes(100)),
            telemetry: telemetry_events(1),
            telemetry_snapshot: snapshot_from_delta(telemetry),
            read_only_responses: read_only_responses(5),
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
                    BatteryPageMetadata::cell_voltage(sel(2), VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::raw(sel(8), VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Diagnostics(DiagnosticReadback::default()),
            ],
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(1),
            diagnostics_snapshot: ParserDiagnostics {
                malformed_frames: parser_diag_count(1),
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
                        malformed_frames: parser_diag_count(1),
                        ..ParserDiagnostics::default()
                    },
                },
            ],
            disconnects: disconnects(0),
        };

        state.apply_session_report(&report);

        assert_eq!(state.counters.subscriptions, subscribes(1));
        assert_eq!(state.counters.notifications, notifications(4));
        assert_eq!(
            state.telemetry.latest_speed,
            Some(DisplaySpeed::from_speed(
                Speed::from_millimetres_per_second(4_470,)
            ))
        );
        assert_eq!(
            state.telemetry.latest_voltage,
            Some(DisplayVoltage::from_voltage(Voltage::from_millivolts(
                108_760
            )))
        );
        assert_eq!(
            state.telemetry.battery_level,
            Some(DashboardBatteryLevel::new(47))
        );
        assert_eq!(firmware_summary_text(&state), Some("43.2.54".to_owned()));
        assert_eq!(state.read_only.settings.len(), 1);
        assert_eq!(state.read_only.bms_pages.len(), 2);
        assert_eq!(
            state.read_only.unknown_raw_pages,
            RawReadOnlyPageCount::new(1)
        );
        assert_eq!(
            state.read_only.diagnostics,
            ReadOnlyDiagnosticResponseCount::new(1)
        );
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
                    BatteryPageMetadata::raw(sel(selector), VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                ))
            })
            .collect();
        let report = SessionBridgeReport {
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(0),
            notifications: notifications(0),
            notification_bytes: NotificationPayloadTotal::default(),
            latest_notification_len: None,
            telemetry: telemetry_events(0),
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: read_only_responses(pages.len()),
            read_only_response_events: pages,
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(0),
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: Vec::new(),
            disconnects: disconnects(0),
        };

        state.apply_session_report(&report);

        assert_eq!(state.read_only.bms_pages.len(), READ_ONLY_SUMMARY_LIMIT);
        assert_eq!(state.read_only.bms_pages[0].page().selector, sel(4));
        assert_eq!(
            state.read_only.bms_pages[READ_ONLY_SUMMARY_LIMIT - 1]
                .page()
                .selector,
            sel(19)
        );
        assert_eq!(
            state.read_only.unknown_raw_pages,
            RawReadOnlyPageCount::new(20)
        );
    }

    #[test]
    fn read_only_temperature_summary_survives_later_bms_pages() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            read_only_responses: read_only_responses(4),
            read_only_response_events: vec![
                ReadOnlyResponse::Battery(BatteryPagePayload::temperature_values(
                    BatteryPageMetadata::temperature(sel(3), VerificationStatus::HardwareVerified),
                    BatteryInfo {
                        temperature: Some(temperature(17_600)),
                        ..BatteryInfo::default()
                    },
                    [
                        Some(temperature(17_600)),
                        Some(temperature(17_100)),
                        Some(temperature(17_700)),
                        Some(temperature(18_500)),
                        Some(temperature(19_000)),
                        Some(temperature(19_100)),
                    ],
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::raw(sel(8), VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::raw(
                    BatteryPageMetadata::metadata(sel(0), VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
                ReadOnlyResponse::Battery(BatteryPagePayload::cell_voltage(
                    BatteryPageMetadata::cell_voltage(sel(2), VerificationStatus::HardwareVerified),
                    BatteryInfo::default(),
                )),
            ],
            ..empty_session_bridge_report()
        };

        state.apply_session_report(&report);
        state.active_tab = DashboardTab::new(2);
        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert_eq!(
            state
                .read_only
                .latest_bms_temperature
                .map(LatestBmsTemperatureSummary)
                .map(|summary| summary.to_string()),
            Some("selector=3 verification=hardware_verified temps_c=17,17,17,18,19,19".to_owned())
        );
        assert!(text.contains("bms temp"));
        assert!(text.contains("temps_c=17,17,17,18,19,19"));
    }

    #[test]
    fn live_battery_level_updates_battery_gauge_from_real_reading() {
        let mut state = DashboardState::empty();

        state.apply_battery_level(88);

        assert_eq!(
            state.telemetry.battery_level,
            Some(DashboardBatteryLevel::new(88))
        );
        assert_eq!(state.telemetry.battery_source, BatterySource::StandardBle);
        assert!(
            state
                .logs
                .iter()
                .any(|entry| entry.message == "battery level 88%")
        );

        state.apply_battery_level(150);

        assert_eq!(
            state.telemetry.battery_level,
            Some(DashboardBatteryLevel::new(100))
        );
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

        state.apply_update(DashboardUpdate::BatteryLevel(45));
        state.apply_update(DashboardUpdate::Log {
            level: "info".to_owned(),
            message: "live dashboard update received".to_owned(),
        });

        assert_eq!(
            state.telemetry.battery_level,
            Some(DashboardBatteryLevel::new(45))
        );
        assert_eq!(state.telemetry.battery_source, BatterySource::StandardBle);
        assert!(state.logs.iter().any(|entry| {
            entry.level == "info" && entry.message == "live dashboard update received"
        }));
    }

    #[test]
    fn live_session_reports_accumulate_transport_counters() {
        let mut state = DashboardState::empty();
        let report = SessionBridgeReport {
            protocol_writes: protocol_writes(0),
            writes: writes(0),
            subscribes: subscribes(1),
            notifications: notifications(2),
            notification_bytes: NotificationPayloadTotal::from_bytes(40),
            latest_notification_len: Some(NotificationByteLen::from_bytes(20)),
            telemetry: telemetry_events(0),
            telemetry_snapshot: TelemetrySnapshot::default(),
            read_only_responses: read_only_responses(0),
            read_only_response_events: Vec::new(),
            firmware: None,
            settings: Vec::new(),
            diagnostics: diagnostic_events(0),
            diagnostics_snapshot: ParserDiagnostics::default(),
            diagnostic_errors: Vec::new(),
            identity: None,
            events: Vec::new(),
            disconnects: disconnects(0),
        };

        state.apply_session_report(&report);
        state.apply_session_report(&report);

        assert_eq!(state.counters.subscriptions, subscribes(2));
        assert_eq!(state.counters.notifications, notifications(4));
        assert_eq!(
            state.counters.notification_bytes,
            NotificationPayloadTotal::from_bytes(80)
        );
        assert_eq!(
            state.counters.latest_notification_len,
            Some(NotificationByteLen::from_bytes(20))
        );
    }

    #[test]
    fn live_session_report_uses_estimated_battery_from_voltage_telemetry() {
        let mut state = DashboardState::empty();
        let report = estimated_battery_report();

        state.apply_session_report(&report);

        assert_eq!(
            state.telemetry.battery_level,
            Some(DashboardBatteryLevel::new(47))
        );
        assert_eq!(
            state.telemetry.battery_source,
            BatterySource::TelemetryEstimated
        );
        assert_eq!(state.telemetry.latest_speed, Some(display_speed_mph(0)));
        assert_eq!(
            state.telemetry.latest_voltage,
            Some(DisplayVoltage::from_voltage(Voltage::from_millivolts(
                108_760
            )))
        );
        assert_eq!(
            state
                .telemetry
                .latest_phase_current
                .map(DisplayPhaseCurrent::get),
            None
        );
        assert_eq!(
            state.telemetry.latest_battery_current,
            Some(DisplayBatteryCurrent::from_milliamps(0))
        );
        assert_eq!(
            state.telemetry.latest_temperature,
            Some(DisplayTemperature::from_temperature(
                Temperature::from_millicelsius(33_270),
            ))
        );
        assert_eq!(
            state.telemetry.latest_distance,
            Some(Distance::from_millimetres(1_551_169_000))
        );
        assert_eq!(
            state.telemetry.latest_pitch,
            Some(WheelPitchDeg::from_angle(Angle::from_millidegrees(69_060)))
        );
        assert_eq!(
            state.telemetry.latest_pwm,
            Some(DisplayDutyCycle::from_duty_cycle(DutyCycle::from_permille(
                -1_000,
            )))
        );
        assert_eq!(state.telemetry.voltage_samples.len(), HISTORY_LIMIT);
        assert!(
            state
                .telemetry
                .voltage_samples
                .iter()
                .all(|voltage| *voltage == Voltage::from_millivolts(108_760))
        );

        let overview_text = buffer_text(&render_buffer(&state, 120, 36));
        assert!(overview_text.contains("47%"));
        assert!(overview_text.contains("Voltage"));

        state.active_tab = DashboardTab::new(1);
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
        assert_eq!(state.counters.notifications, NotificationCount::default());
    }

    #[test]
    fn tab_navigation_wraps_forward_and_backward() {
        let mut state = DashboardState::empty();

        state.next_tab();
        assert_eq!(state.active_tab, DashboardTab::new(1));

        state.previous_tab();
        assert_eq!(state.active_tab, DashboardTab::first());

        state.previous_tab();
        assert_eq!(state.active_tab, DashboardTab::new(3));
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
        state.active_tab = DashboardTab::new(1);

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
        state.active_tab = DashboardTab::new(2);
        state
            .telemetry
            .apply_snapshot(live_aero_telemetry_snapshot());
        state.read_only.firmware = Some(firmware_43_2_54());
        state
            .read_only
            .settings
            .push_back(hardware_setting(36, 1920));
        state
            .read_only
            .settings
            .push_back(hardware_setting(37, 1940));
        state
            .read_only
            .bms_pages
            .push_back(BatteryPagePayload::cell_voltage(
                BatteryPageMetadata::cell_voltage(sel(2), VerificationStatus::HardwareVerified),
                BatteryInfo::default(),
            ));
        state.read_only.bms_pages.push_back(BatteryPagePayload::raw(
            BatteryPageMetadata::raw(sel(8), VerificationStatus::HardwareVerified),
            BatteryInfo::default(),
        ));
        state
            .read_only
            .bms_pages
            .push_back(BatteryPagePayload::temperature(
                BatteryPageMetadata::temperature(sel(47), VerificationStatus::HardwareVerified),
                BatteryInfo::default(),
            ));
        state.read_only.unknown_raw_pages = RawReadOnlyPageCount::new(1);
        state.read_only.diagnostics = ReadOnlyDiagnosticResponseCount::new(1);

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
    fn profiles_tab_enter_opens_selected_operational_dashboard() {
        let mut state = DashboardState::sample();
        state.active_tab = DashboardTab::new(2);

        state.handle_input(DashboardInput::Enter);

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Operational dashboard"));
        assert!(
            text.contains("NOSFET Aero (via Veteran Protocol) Current 45A / Raw Tail Preserved")
        );
        assert!(text.contains("18 mph"));
        assert!(text.contains("Battery"));
        assert!(text.contains("74%"));
        assert!(text.contains("Voltage"));
        assert!(text.contains("53 V"));
        assert!(text.contains("Signal"));
        assert!(text.contains("81%"));
        assert!(text.contains("Wheel state"));
        assert!(text.contains("riding"));
        assert!(text.contains("Power"));
        assert!(text.contains("318 W"));
        assert!(text.contains("Amps"));
        assert!(text.contains("6 A"));
        assert!(text.contains("Temp"));
        assert!(text.contains("33 C"));
        assert!(text.contains("PWM"));
        assert!(text.contains("unknown"));
        assert!(text.contains("Trip"));
        assert!(text.contains("firmware 43.2.54"));
        assert!(text.contains("Read-only mode"));
        assert!(text.contains("active"));
        assert!(!text.contains("Load/PWM"));
        assert!(!text.contains("Controls"));
        assert!(!text.contains("disabled"));
    }

    #[test]
    fn escape_returns_from_operational_dashboard_to_profiles() {
        let mut state = DashboardState::sample();
        state.active_tab = DashboardTab::new(2);
        state.handle_input(DashboardInput::Enter);

        state.handle_input(DashboardInput::Back);

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Profiles"));
        assert!(text.contains("Read-only responses"));
        assert!(!text.contains("Operational dashboard"));
    }

    #[test]
    fn profile_selection_moves_before_opening_dashboard() {
        let mut state = DashboardState::sample();
        state.active_tab = DashboardTab::new(2);

        state.handle_input(DashboardInput::MoveDown);
        state.handle_input(DashboardInput::Enter);

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert!(text.contains("Operational dashboard"));
        assert!(
            text.contains(
                "NOSFET Aero (profile pending) Pending Begode/Falcon Unsupported / Pending"
            )
        );
    }

    #[test]
    fn live_profile_dashboard_uses_device_protocol_heading() {
        let target = ConnectionTarget {
            address: None,
            identifier: None,
            name_contains: Some("NF2557".to_owned()),
        };
        let summary = ConnectionSummary {
            observation: PeripheralObservation {
                identifier: "platform-0001".to_owned(),
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                name: Some("NF2557".to_owned()),
                rssi: Some(rssi(-61)),
                advertised_services: vec![].into(),
                manufacturer_data: Vec::new().into(),
            },
            services: Vec::new().into(),
        };
        let mut state = DashboardState::live_connected(&target, &summary);
        state.active_tab = DashboardTab::new(2);

        state.handle_input(DashboardInput::Enter);

        let text = buffer_text(&render_buffer(&state, 140, 36));

        assert!(
            text.contains("NOSFET Aero (via Veteran Protocol) Live Notification Channel Observed")
        );
    }

    #[test]
    fn live_telemetry_tab_renders_decoder_input_when_decoder_has_no_samples() {
        let mut state = DashboardState::empty();
        state.active_tab = DashboardTab::new(1);
        state.counters.notifications = notifications(47);
        state.counters.notification_bytes = NotificationPayloadTotal::from_bytes(4_700);
        state.counters.latest_notification_len = Some(NotificationByteLen::from_bytes(100));

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
        assert_eq!(state.telemetry.speed_samples.len(), HISTORY_LIMIT);
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
        state.active_tab = DashboardTab::new(3);

        let text = buffer_text(&render_buffer(&state, 120, 36));

        assert_eq!(text.matches("Recent events").count(), 1);
        assert!(text.contains("demo state loaded from demo state: aero-nf2557.v1"));
        assert!(text.contains("dashboard booted in read-only mode"));
    }

    #[test]
    fn taller_logs_tab_reveals_more_retained_events() {
        let mut state = DashboardState::empty();
        state.active_tab = DashboardTab::new(3);
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
        assert_eq!(state.counters.notifications, notifications(27));
        assert_eq!(state.profiles.len(), 3);
        assert_eq!(state.telemetry.speed_samples.len(), 12);
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
