use std::time::Duration;

use anyhow::Result;
use cutout_btle::{
    BtleError, ConnectedPeripheral, SessionBridgeReport, SessionCapture, SessionEndpoints,
    capture_session, connect_and_discover, drive_session, scan_peripherals,
};
use cutout_core::{FirmwareInfo, Measured, SettingsReadback, TelemetrySnapshot};
use cutout_protocols::{NosfetAeroModel, ReadOnlySession, VETERAN_DATA_CHANNEL};

use crate::cli::{Cli, Command, TargetedScanArgs};
use crate::dashboard::run_dashboard;
use crate::validation::render_validation_report;

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
        Command::Dashboard(args) => run_dashboard(args)?,
    }

    Ok(())
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
        "session writes={} subscribes={} notifications={} telemetry={} read_only_responses={} diagnostics={} disconnects={}",
        report.writes,
        report.subscribes,
        report.notifications,
        report.telemetry,
        report.read_only_responses,
        report.diagnostics,
        report.disconnects
    );
    if let Some(telemetry) = render_telemetry_snapshot(&report.telemetry_snapshot) {
        println!("{telemetry}");
    }
    if let Some(firmware) = render_firmware_info(report.firmware) {
        println!("{firmware}");
    }
    for settings in render_settings_readbacks(&report.settings) {
        println!("{settings}");
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

fn render_firmware_info(firmware: Option<FirmwareInfo>) -> Option<String> {
    let firmware = firmware?;
    let mut fields = Vec::new();
    push_measured_u16(&mut fields, "firmware_major", firmware.firmware_major);
    push_measured_u16(&mut fields, "firmware_minor", firmware.firmware_minor);
    push_measured_u16(&mut fields, "firmware_patch", firmware.firmware_patch);
    if let Some(build_id) = firmware.build_id {
        fields.push(format!("raw_{:04x}={}", build_id.id, build_id.value));
    }

    (!fields.is_empty()).then(|| format!("firmware {}", fields.join(" ")))
}

fn render_settings_readbacks(settings: &[SettingsReadback]) -> Vec<String> {
    settings
        .iter()
        .filter_map(|settings| {
            let mut fields = Vec::new();
            for entry in settings.entries.into_iter().flatten() {
                fields.push(format!("raw_{:04x}={}", entry.field.id, entry.field.value));
            }

            (!fields.is_empty()).then(|| format!("settings {}", fields.join(" ")))
        })
        .collect()
}

fn push_measured_u16(
    fields: &mut Vec<String>,
    name: &'static str,
    measured: Option<Measured<u16>>,
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
            power_mw: Some(Measured::calculated(-184_892)),
            controller_temperature_mc: Some(Measured::reported(33_270)),
            pwm_permille: Some(Measured::reported(-1_000)),
            distance_mm: Some(Measured::reported(1_551_169_000)),
            pitch_mdeg: Some(Measured::reported(69_060)),
            battery_percent_estimated: Some(Measured::estimated(47)),
            ..cutout_core::TelemetryDelta::empty(42)
        });

        assert_eq!(
            render_telemetry_snapshot(&snapshot).as_deref(),
            Some(
                "telemetry speed_mm_s=1200 voltage_mv=108760 motor_current_ma=-1700 power_mw=-184892 controller_temperature_mc=33270 pwm_permille=-1000 distance_mm=1551169000 pitch_mdeg=69060 battery_percent_estimated=47"
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

    #[test]
    fn firmware_renderer_includes_fixed_header_version_fields() {
        let firmware = FirmwareInfo {
            firmware_major: Some(Measured::reported(43)),
            firmware_minor: Some(Measured::reported(2)),
            firmware_patch: Some(Measured::reported(54)),
            build_id: Some(cutout_core::RawFieldValue::new(0x001c, 43_254)),
            ..FirmwareInfo::default()
        };

        assert_eq!(
            render_firmware_info(Some(firmware)).as_deref(),
            Some("firmware firmware_major=43 firmware_minor=2 firmware_patch=54 raw_001c=43254")
        );
    }

    #[test]
    fn settings_renderer_includes_fixed_header_raw_fields() {
        let entry = |id, value| cutout_core::SettingsEntry {
            field: cutout_core::RawFieldValue::new(id, value),
            source: cutout_core::ValueSource::Reported,
            quality: cutout_core::ValueQuality::Known,
            verification: cutout_core::VerificationStatus::HardwareVerified,
        };
        let settings = SettingsReadback {
            entries: [
                Some(entry(0x0014, 0)),
                Some(entry(0x0016, 0)),
                Some(entry(0x0018, 550)),
                Some(entry(0x001a, 540)),
            ],
        };
        let more_settings = SettingsReadback {
            entries: [Some(entry(0x001e, 1_920)), None, None, None],
        };

        assert_eq!(
            render_settings_readbacks(&[settings, more_settings]),
            vec![
                "settings raw_0014=0 raw_0016=0 raw_0018=550 raw_001a=540",
                "settings raw_001e=1920",
            ]
        );
    }
}
