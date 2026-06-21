use std::{
    collections::VecDeque,
    io::{self, Read},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use ratatui::termina::{PlatformTerminal, Terminal as _};
use ratatui::{
    Frame, Terminal,
    backend::TerminaBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Axis, Block, Cell, Chart, Dataset, Gauge, Paragraph, Row, Sparkline, Table, Tabs},
};

const LOG_LIMIT: usize = 10;
const HISTORY_LIMIT: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardState {
    pub(crate) active_tab: usize,
    pub(crate) device: DeviceSnapshot,
    pub(crate) telemetry: TelemetryWindow,
    pub(crate) profiles: Vec<ProfileSnapshot>,
    pub(crate) counters: SessionCounters,
    pub(crate) logs: VecDeque<LogEntry>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionCounters {
    pub(crate) discovered: u64,
    pub(crate) connected: u64,
    pub(crate) subscriptions: u64,
    pub(crate) notifications: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogEntry {
    pub(crate) level: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TelemetryWindow {
    pub(crate) battery_pct: u64,
    pub(crate) signal_pct: u64,
    pub(crate) speed_mph: VecDeque<u64>,
    pub(crate) voltage_v: VecDeque<u64>,
    pub(crate) current_a: VecDeque<u64>,
    pub(crate) temperature_c: VecDeque<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardInput {
    Quit,
}

impl DashboardState {
    pub(crate) fn sample() -> Self {
        let mut logs = VecDeque::new();
        logs.push_back(LogEntry {
            level: "info".to_owned(),
            message: "dashboard booted in read-only mode".to_owned(),
        });
        logs.push_back(LogEntry {
            level: "info".to_owned(),
            message: "waiting for discovery and profile data".to_owned(),
        });
        logs.push_back(LogEntry {
            level: "debug".to_owned(),
            message: "Termina backend selected for terminal I/O".to_owned(),
        });

        Self {
            active_tab: 0,
            device: DeviceSnapshot {
                name: "Aero NF2557".to_owned(),
                address: "AA:BB:CC:DD:EE:FF".to_owned(),
                identifier: "platform-0001".to_owned(),
                firmware: "v3.8.12".to_owned(),
                connection_state: "connected".to_owned(),
            },
            telemetry: TelemetryWindow::sample(),
            profiles: vec![
                ProfileSnapshot {
                    name: "Primary drive".to_owned(),
                    source: "probe".to_owned(),
                    status: "ready".to_owned(),
                },
                ProfileSnapshot {
                    name: "Battery pack".to_owned(),
                    source: "capture".to_owned(),
                    status: "warming".to_owned(),
                },
                ProfileSnapshot {
                    name: "Motor controller".to_owned(),
                    source: "manual".to_owned(),
                    status: "partial".to_owned(),
                },
            ],
            counters: SessionCounters {
                discovered: 8,
                connected: 1,
                subscriptions: 4,
                notifications: 27,
            },
            logs,
        }
    }

    pub(crate) fn advance(&mut self) {
        self.counters.notifications = self.counters.notifications.saturating_add(1);
        self.telemetry.step();
        if self.logs.len() == LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(LogEntry {
            level: "trace".to_owned(),
            message: format!(
                "sample {} updated for {}",
                self.counters.notifications, self.device.name
            ),
        });
    }
}

impl TelemetryWindow {
    fn sample() -> Self {
        Self {
            battery_pct: 74,
            signal_pct: 81,
            speed_mph: VecDeque::from(vec![0, 4, 9, 14, 18, 22, 24, 21, 19, 17, 16, 18]),
            voltage_v: VecDeque::from(vec![52, 52, 53, 53, 54, 54, 55, 55, 55, 54, 54, 53]),
            current_a: VecDeque::from(vec![3, 4, 6, 7, 8, 9, 10, 10, 9, 8, 7, 6]),
            temperature_c: VecDeque::from(vec![30, 31, 31, 32, 32, 33, 34, 34, 35, 35, 34, 33]),
        }
    }

    fn step(&mut self) {
        let next_speed = (self.speed_mph.back().copied().unwrap_or(0) + 3) % 40;
        let next_voltage = 50 + ((self.voltage_v.back().copied().unwrap_or(52) + 1) % 6);
        let next_current = 4 + ((self.current_a.back().copied().unwrap_or(5) + 1) % 9);
        let next_temperature = 30 + ((self.temperature_c.back().copied().unwrap_or(32) + 1) % 9);

        self.battery_pct = self.battery_pct.saturating_sub(1).max(10);
        self.signal_pct = (self.signal_pct + 1).min(100);
        push_sample(&mut self.speed_mph, next_speed);
        push_sample(&mut self.voltage_v, next_voltage);
        push_sample(&mut self.current_a, next_current);
        push_sample(&mut self.temperature_c, next_temperature);
    }
}

fn push_sample(series: &mut VecDeque<u64>, value: u64) {
    if series.len() == HISTORY_LIMIT {
        series.pop_front();
    }
    series.push_back(value);
}

pub(crate) fn run_dashboard() -> Result<()> {
    let mut state = DashboardState::sample();
    let (tx, rx) = mpsc::channel::<DashboardInput>();
    let input_thread = spawn_input_thread(tx);

    let mut output = PlatformTerminal::new()?;
    output.enter_raw_mode()?;
    let backend = TerminaBackend::new(output);
    let mut terminal = Terminal::new(backend)?;
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| render_dashboard(frame, &state))?;

        if matches!(rx.try_recv(), Ok(DashboardInput::Quit)) {
            break;
        }

        if last_tick.elapsed() >= Duration::from_millis(250) {
            state.advance();
            last_tick = Instant::now();
        } else {
            thread::sleep(Duration::from_millis(25));
        }
    }

    drop(input_thread);
    Ok(())
}

fn spawn_input_thread(tx: mpsc::Sender<DashboardInput>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut locked = stdin.lock();
        let mut byte = [0_u8; 1];

        while locked.read_exact(&mut byte).is_ok() {
            match byte[0] {
                b'q' | b'Q' | 0x1b => {
                    let _ = tx.send(DashboardInput::Quit);
                    break;
                }
                _ => {}
            }
        }
    })
}

pub(crate) fn render_dashboard(frame: &mut Frame<'_>, state: &DashboardState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(10),
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
            Constraint::Length(4),
            Constraint::Length(5),
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
            Span::styled("firmware ", Style::new().fg(Color::Gray)),
            Span::raw(device.firmware.as_str()),
        ]),
        Line::from(vec![
            Span::styled("state ", Style::new().fg(Color::Gray)),
            Span::raw(device.connection_state.as_str()),
        ]),
    ])
    .block(Block::bordered().title("Target"));
    frame.render_widget(summary, chunks[0]);

    let gauges = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let battery = Gauge::default()
        .block(Block::bordered().title("Battery"))
        .gauge_style(Style::new().fg(Color::Green).bg(Color::Black))
        .ratio(percent_ratio(state.telemetry.battery_pct))
        .label(format!("{}%", state.telemetry.battery_pct));
    frame.render_widget(battery, gauges[0]);

    let signal = Gauge::default()
        .block(Block::bordered().title("Signal"))
        .gauge_style(Style::new().fg(Color::Cyan).bg(Color::Black))
        .ratio(percent_ratio(state.telemetry.signal_pct))
        .label(format!("{}%", state.telemetry.signal_pct));
    frame.render_widget(signal, gauges[1]);

    let rows = state
        .profiles
        .iter()
        .map(|profile| {
            Row::new(vec![
                Cell::from(profile.name.clone()),
                Cell::from(profile.source.clone()),
                Cell::from(profile.status.clone()),
            ])
        })
        .collect::<Vec<_>>();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
    )
    .header(Row::new(vec!["Profile", "Source", "Status"]).style(Style::new().bold()))
    .block(Block::bordered().title("Profiles"))
    .column_spacing(1);
    frame.render_widget(table, chunks[2]);
}

fn render_telemetry(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .split(area);

    render_sparkline(
        frame,
        chunks[0],
        "Speed",
        &state.telemetry.speed_mph,
        Color::Yellow,
    );
    render_sparkline(
        frame,
        chunks[1],
        "Voltage",
        &state.telemetry.voltage_v,
        Color::Magenta,
    );

    let current_points = series_points(&state.telemetry.current_a);
    let temperature_points = series_points(&state.telemetry.temperature_c);
    let counters = Chart::new(vec![
        Dataset::default()
            .name("current")
            .marker(ratatui::symbols::Marker::Dot)
            .style(Style::new().fg(Color::LightBlue))
            .data(current_points.as_slice()),
        Dataset::default()
            .name("temperature")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::new().fg(Color::Red))
            .data(temperature_points.as_slice()),
    ])
    .block(Block::bordered().title("Trend"))
    .x_axis(Axis::default().title("samples").bounds([
        0.0,
        f64::from(u32::try_from(HISTORY_LIMIT).unwrap_or(u32::MAX)),
    ]))
    .y_axis(Axis::default().title("value").bounds([0.0, 100.0]));
    frame.render_widget(counters, chunks[2]);
}

fn render_sparkline(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    samples: &VecDeque<u64>,
    color: Color,
) {
    let spark = Sparkline::default()
        .block(Block::bordered().title(title))
        .style(Style::new().fg(color))
        .data(series_values(samples))
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
                Span::styled(format!("[{}] ", entry.level), Style::new().fg(Color::Gray)),
                Span::raw(entry.message.clone()),
            ])
        })
        .collect::<Vec<_>>();

    let log_panel = Paragraph::new(lines)
        .block(Block::bordered().title("Recent events"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(log_panel, area);
}

fn series_values(samples: &VecDeque<u64>) -> Vec<u64> {
    samples.iter().copied().collect()
}

fn series_points(samples: &VecDeque<u64>) -> Vec<(f64, f64)> {
    samples
        .iter()
        .enumerate()
        .map(|(index, value)| (index_to_f64(index), to_f64(*value)))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
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

        assert_eq!(state.active_tab, 0);
        assert_eq!(state.device.name, "Aero NF2557");
        assert_eq!(state.profiles.len(), 3);
        assert_eq!(state.logs.len(), 3);
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
            "Profiles",
            "Speed",
            "Voltage",
            "Trend",
            "Recent events",
            "Aero NF2557",
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
}
