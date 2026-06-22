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
            eprintln!("{error}");
            ExitCode::FAILURE
        }
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
