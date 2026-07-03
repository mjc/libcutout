use clap::Parser;
use cutout_cli::{Cli, init_logging, install_dashboard_signal_restore, run};
use std::process::ExitCode;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let dashboard_mode = dashboard_mode_requested();
    let _log_guard = init_logging(dashboard_mode);
    if dashboard_mode {
        if let Err(error) = install_dashboard_signal_restore() {
            eprintln!("failed to install dashboard signal restore: {error}");
            return ExitCode::FAILURE;
        }
    }
    let cli = Cli::parse();

    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", render_error(&error));
            ExitCode::FAILURE
        }
    }
}

fn dashboard_mode_requested() -> bool {
    std::env::args_os().skip(1).any(|arg| arg == "dashboard")
}

fn render_error(error: &anyhow::Error) -> String {
    if let Some(btle_error) = error.downcast_ref::<cutout_btle::BtleError>() {
        format!("{error}\nhint: {}", btle_error.diagnostic_hint())
    } else {
        error.to_string()
    }
}
