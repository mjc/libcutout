use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

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

    if let Some(path) = debug_log_path(std::env::var_os("CUTOUT_DEBUG_LOG")) {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(_) => {
                let log_path = path.clone();
                let subscriber = fmt()
                    .compact()
                    .with_target(true)
                    .with_env_filter(filter)
                    .with_writer(move || DebugLogWriter {
                        path: log_path.clone(),
                    })
                    .finish();
                let _ = tracing::subscriber::set_global_default(subscriber);
                return;
            }
            Err(error) => {
                eprintln!(
                    "failed to open CUTOUT_DEBUG_LOG {}: {error}",
                    path.display()
                );
            }
        }
    }

    let subscriber = fmt()
        .compact()
        .without_time()
        .with_target(false)
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[derive(Debug)]
struct DebugLogWriter {
    path: PathBuf,
}

impl Write for DebugLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn debug_log_path(value: Option<OsString>) -> Option<PathBuf> {
    value.and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    })
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

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

    #[test]
    fn empty_debug_log_env_keeps_stderr_logging() {
        assert_eq!(super::debug_log_path(Some(OsString::new())), None);
    }

    #[test]
    fn debug_log_env_selects_file_logging_path() {
        assert_eq!(
            super::debug_log_path(Some(OsString::from("target/cutout-debug.log"))),
            Some(PathBuf::from("target/cutout-debug.log"))
        );
    }
}
