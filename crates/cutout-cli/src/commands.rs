use std::time::Duration;

use cutout_btle::{
    BtleError, ConnectedPeripheral, SessionBridgeReport, SessionCapture, SessionEndpoints,
    capture_session, connect_and_discover, drive_session, scan_peripherals,
};
use cutout_protocols::{AERO_WRITE_CHANNEL, AeroReadOnlySession};

use crate::cli::{Cli, Command, TargetedScanArgs};

/// Executes a parsed CLI invocation.
///
/// # Errors
///
/// Returns the underlying Bluetooth transport error when scanning, connecting,
/// discovery, or protocol session bridging fails.
pub async fn run(cli: Cli) -> Result<(), BtleError> {
    match cli.command {
        Command::Scan(args) => scan(args.seconds()).await,
        Command::Connect(args) => connect(args, SessionMode::Drive).await,
        Command::CaptureAero(args) => connect(args, SessionMode::Capture).await,
    }
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
        let mut session = AeroReadOnlySession::default();
        match self {
            Self::Drive => {
                let report = drive_session(
                    &connection.peripheral,
                    &mut session,
                    AERO_WRITE_CHANNEL,
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
                    AERO_WRITE_CHANNEL,
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
}
