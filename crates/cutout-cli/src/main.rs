use std::process::ExitCode;

use clap::Parser;
use cutout_cli::{Cli, run};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    init_logging();

    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", render_error(&error));
            ExitCode::FAILURE
        }
    }
}

fn render_error(error: &anyhow::Error) -> String {
    if let Some(btle_error) = error.downcast_ref::<cutout_btle::BtleError>() {
        format!("{error}\nhint: {}", btle_error.diagnostic_hint())
    } else {
        error.to_string()
    }
}

fn init_logging() {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new("cutout_cli=info"),
    };

    let subscriber = fmt()
        .compact()
        .without_time()
        .with_target(false)
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    fn rendered_btle_errors_include_actionable_hint() {
        let error = anyhow::Error::from(cutout_btle::BtleError::OperationTimedOut {
            operation: "connect peripheral",
            after: Duration::from_secs(10),
        });

        let rendered = super::render_error(&error);

        assert!(rendered.contains("bluetooth operation timed out: connect peripheral after 10s"));
        assert!(rendered.contains("hint: retry the operation"));
    }

    #[test]
    fn rendered_non_btle_errors_do_not_gain_ble_hint() {
        let error = anyhow::anyhow!("usage failed");

        assert_eq!(super::render_error(&error), "usage failed");
    }
}
