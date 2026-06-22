use std::time::Duration;

use anyhow::Result;
use cutout_btle::{
    BtleError, ConnectedPeripheral, ConnectionTarget, SessionBridgeReport, SessionCapture,
    SessionEndpoints, capture_session, connect_and_discover, drive_session, scan_peripherals,
};
use cutout_core::{Measured, TelemetrySnapshot};
use cutout_protocols::{NosfetAeroModel, ReadOnlySession, VETERAN_DATA_CHANNEL};
use tracing::info;

use crate::cli::{Cli, Command, DashboardArgs, TargetedScanArgs};
use crate::dashboard::{DashboardState, run_dashboard};
use crate::validation::render_validation_report;

const DASHBOARD_PROBE_WINDOW: Duration = Duration::from_secs(2);

/// Executes a parsed CLI invocation.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error when scanning, connecting,
/// discovery, or protocol session bridging fails.
pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Scan(args) => scan(args.seconds()).await?,
        Command::Connect(args) => connect(args, SessionMode::Drive).await?,
        Command::CaptureAero(args) => connect(args, SessionMode::Capture).await?,
        Command::Validation => print!("{}", render_validation_report()),
        Command::Dashboard(args) => dashboard(args).await?,
    }

    Ok(())
}

async fn dashboard(args: DashboardArgs) -> Result<()> {
    if args.demo {
        return run_dashboard(DashboardState::demo(args.device.as_deref()));
    }

    let target = dashboard_live_target(&args)?;
    info!(
        device = target.name_contains.as_deref().unwrap_or("<none>"),
        seconds = args.seconds(),
        "scanning for dashboard device"
    );
    let connection = connect_and_discover(&target, Duration::from_secs(args.seconds())).await?;
    info!(
        observation = %connection.summary.observation,
        "connected dashboard device"
    );
    let mut state = DashboardState::live_connected(&target, &connection.summary);
    if let Some(endpoints) = connection.summary.select_session_endpoints() {
        info!(
            seconds = DASHBOARD_PROBE_WINDOW.as_secs(),
            "probing dashboard session"
        );
        let mut session = AeroReadOnlySession::default();
        let report = drive_session(
            &connection.peripheral,
            &mut session,
            VETERAN_DATA_CHANNEL,
            endpoints,
            DASHBOARD_PROBE_WINDOW,
        )
        .await?;
        state.apply_session_report(&report);
    }
    run_dashboard(state)
}

fn dashboard_live_target(args: &DashboardArgs) -> Result<ConnectionTarget> {
    let Some(device) = args.device.clone() else {
        anyhow::bail!("dashboard requires --demo or --device to start");
    };

    Ok(ConnectionTarget {
        address: None,
        identifier: None,
        name_contains: Some(device),
    })
}

async fn scan(seconds: u64) -> Result<(), BtleError> {
    for observation in scan_peripherals(Duration::from_secs(seconds)).await? {
        println!("{observation}");
    }
    Ok(())
}

async fn connect(args: TargetedScanArgs, mode: SessionMode) -> Result<(), BtleError> {
    let seconds = args.seconds();
    let connection =
        connect_and_discover(&args.into_target(), Duration::from_secs(seconds)).await?;

    println!("{}", connection.summary);
    if let Some(endpoints) = connection.summary.select_session_endpoints() {
        print_session_endpoints(endpoints);
        mode.run(&connection, endpoints, Duration::from_secs(seconds))
            .await?;
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionMode {
    Drive,
    Capture,
}

impl SessionMode {
    async fn run(
        self,
        connection: &ConnectedPeripheral,
        endpoints: SessionEndpoints<'_>,
        window: Duration,
    ) -> Result<(), BtleError> {
        let mut session = ReadOnlySession::<NosfetAeroModel, false>::default();
        match self {
            Self::Drive => {
                let report = drive_session(
                    &connection.peripheral,
                    &mut session,
                    VETERAN_DATA_CHANNEL,
                    endpoints,
                    window,
                )
                .await?;
                print_session_report(&report);
            }
            Self::Capture => {
                let capture = capture_session(
                    &connection.peripheral,
                    &mut session,
                    VETERAN_DATA_CHANNEL,
                    endpoints,
                    window,
                    true,
                )
                .await?;
                print_capture(capture);
            }
        }
        Ok(())
    }
}

fn print_session_endpoints(endpoints: SessionEndpoints<'_>) {
    println!(
        "session write={} notify={}",
        endpoints.write.uuid,
        endpoints
            .notify
            .map_or_else(|| "<none>".to_owned(), |notify| notify.uuid.to_string())
    );
}

fn print_capture(capture: SessionCapture) {
    for record in capture.records {
        println!("{record}");
    }
    print_session_report(&capture.report);
}

fn print_session_report(report: &SessionBridgeReport) {
    println!(
        "session writes={} subscribes={} notifications={} telemetry={} diagnostics={} disconnects={}",
        report.writes,
        report.subscribes,
        report.notifications,
        report.telemetry,
        report.diagnostics,
        report.disconnects
    );
    if let Some(telemetry) = render_telemetry_snapshot(&report.telemetry_snapshot) {
        println!("{telemetry}");
    }
}

fn render_telemetry_snapshot(snapshot: &TelemetrySnapshot) -> Option<String> {
    let mut fields = Vec::new();
    push_measured_i32(&mut fields, "speed_mm_s", snapshot.speed_mm_s);
    push_measured_i32(&mut fields, "voltage_mv", snapshot.voltage_mv);
    push_measured_i32(
        &mut fields,
        "battery_current_ma",
        snapshot.battery_current_ma,
    );
    push_measured_i32(&mut fields, "motor_current_ma", snapshot.motor_current_ma);
    push_measured_i64(&mut fields, "power_mw", snapshot.power_mw);
    push_measured_i32(
        &mut fields,
        "controller_temperature_mc",
        snapshot.controller_temperature_mc,
    );
    push_measured_i32(
        &mut fields,
        "motor_temperature_mc",
        snapshot.motor_temperature_mc,
    );
    push_measured_i32(
        &mut fields,
        "battery_temperature_mc",
        snapshot.battery_temperature_mc,
    );
    push_measured_i16(&mut fields, "pwm_permille", snapshot.pwm_permille);
    push_measured_u64(&mut fields, "distance_mm", snapshot.distance_mm);
    push_measured_i32(&mut fields, "pitch_mdeg", snapshot.pitch_mdeg);
    push_measured_i32(&mut fields, "roll_mdeg", snapshot.roll_mdeg);
    push_measured_u8(
        &mut fields,
        "battery_percent_reported",
        snapshot.battery_percent_reported,
    );
    push_measured_u8(
        &mut fields,
        "battery_percent_estimated",
        snapshot.battery_percent_estimated,
    );

    (!fields.is_empty()).then(|| format!("telemetry {}", fields.join(" ")))
}

fn push_measured_i16(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<i16>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_i32(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<i32>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_i64(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<i64>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_u8(fields: &mut Vec<String>, name: &'static str, measured: Option<Measured<u8>>) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

fn push_measured_u64(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<u64>>,
) {
    if let Some(measured) = measured {
        fields.push(format!("{name}={}", measured.value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_snapshot_renderer_includes_present_fields() {
        let mut snapshot = TelemetrySnapshot::default();
        snapshot.apply_delta(cutout_core::TelemetryDelta {
            speed_mm_s: Some(Measured::reported(1_200)),
            voltage_mv: Some(Measured::reported(108_760)),
            motor_current_ma: Some(Measured::reported(-1_700)),
            controller_temperature_mc: Some(Measured::reported(33_270)),
            pwm_permille: Some(Measured::reported(-1_000)),
            distance_mm: Some(Measured::reported(1_551_169_000)),
            pitch_mdeg: Some(Measured::reported(69_060)),
            battery_percent_estimated: Some(Measured::estimated(39)),
            ..cutout_core::TelemetryDelta::empty(42)
        });

        assert_eq!(
            render_telemetry_snapshot(&snapshot).as_deref(),
            Some(
                "telemetry speed_mm_s=1200 voltage_mv=108760 motor_current_ma=-1700 controller_temperature_mc=33270 pwm_permille=-1000 distance_mm=1551169000 pitch_mdeg=69060 battery_percent_estimated=39"
            )
        );
    }

    #[test]
    fn telemetry_snapshot_renderer_omits_empty_snapshot() {
        assert_eq!(
            render_telemetry_snapshot(&TelemetrySnapshot::default()),
            None
        );
    }
}

#[cfg(test)]
mod tests {
    use cutout_btle::ConnectionTarget;

    use super::*;
    use crate::cli::ScanArgs;

    fn dashboard_args(demo: bool, device: Option<&str>) -> DashboardArgs {
        DashboardArgs {
            demo,
            device: device.map(ToOwned::to_owned),
            scan: ScanArgs { seconds: 5 },
        }
    }

    #[test]
    fn dashboard_live_target_requires_device_outside_demo_mode() {
        let error = dashboard_live_target(&dashboard_args(false, None))
            .expect_err("live dashboard requires an explicit device");

        assert_eq!(
            error.to_string(),
            "dashboard requires --demo or --device to start"
        );
    }

    #[test]
    fn dashboard_live_target_maps_device_to_name_filter() {
        let target = dashboard_live_target(&dashboard_args(false, Some("NF2557")))
            .expect("device becomes a live target");

        assert_eq!(
            target,
            ConnectionTarget {
                address: None,
                identifier: None,
                name_contains: Some("NF2557".to_owned()),
            }
        );
    }
}
