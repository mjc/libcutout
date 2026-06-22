use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Parser;
use cutout_cli::{Cli, run};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_DEBUG_LOG_PATH: &str = "debug.log";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let _log_guard = init_logging();

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

fn init_logging() -> Option<WorkerGuard> {
    let rust_log = std::env::var_os("RUST_LOG");
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new("cutout_cli=info"),
    };

    if let Some(path) = debug_log_path(std::env::var_os("CUTOUT_DEBUG_LOG"), rust_log.as_ref()) {
        let (directory, file_name) = debug_log_location(&path);
        let file_appender = tracing_appender::rolling::never(directory, file_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let subscriber = fmt()
            .compact()
            .with_target(true)
            .with_env_filter(filter)
            .with_writer(non_blocking)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
        return Some(guard);
    }

    let subscriber = fmt()
        .compact()
        .without_time()
        .with_target(false)
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
    None
}

fn debug_log_path(explicit_path: Option<OsString>, rust_log: Option<&OsString>) -> Option<PathBuf> {
    match explicit_path {
        Some(value) if value.is_empty() => None,
        Some(value) => Some(PathBuf::from(value)),
        None if rust_log_requests_debug_file(rust_log) => {
            Some(PathBuf::from(DEFAULT_DEBUG_LOG_PATH))
        }
        None => None,
    }
}

fn debug_log_location(path: &Path) -> (PathBuf, OsString) {
    let directory = path.parent().map_or_else(
        || PathBuf::from("."),
        |parent| {
            if parent.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                parent.to_path_buf()
            }
        },
    );
    let file_name = path
        .file_name()
        .map_or_else(|| OsString::from(DEFAULT_DEBUG_LOG_PATH), OsString::from);
    (directory, file_name)
}

fn rust_log_requests_debug_file(value: Option<&OsString>) -> bool {
    value
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.split(',').any(directive_requests_debug_file))
}

fn directive_requests_debug_file(directive: &str) -> bool {
    let level = directive
        .rsplit_once('=')
        .map_or(directive, |(_target, level)| level);
    matches!(
        level.trim().to_ascii_lowercase().as_str(),
        "debug" | "trace"
    )
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf, time::Duration};

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
        assert_eq!(
            super::debug_log_path(Some(OsString::new()), Some(&OsString::from("debug"))),
            None
        );
    }

    #[test]
    fn debug_log_env_selects_file_logging_path() {
        assert_eq!(
            super::debug_log_path(
                Some(OsString::from("target/custom-cutout-debug.log")),
                Some(&OsString::from("info"))
            ),
            Some(PathBuf::from("target/custom-cutout-debug.log"))
        );
    }

    #[test]
    fn rust_log_debug_selects_default_file_logging_path() {
        assert_eq!(
            super::debug_log_path(None, Some(&OsString::from("debug"))),
            Some(PathBuf::from("debug.log"))
        );
    }

    #[test]
    fn rust_log_module_debug_selects_default_file_logging_path() {
        assert_eq!(
            super::debug_log_path(None, Some(&OsString::from("cutout_cli=debug,info"))),
            Some(PathBuf::from("debug.log"))
        );
    }

    #[test]
    fn rust_log_info_keeps_stderr_logging() {
        assert_eq!(
            super::debug_log_path(None, Some(&OsString::from("cutout_cli=info"))),
            None
        );
    }

    #[test]
    fn debug_log_location_uses_working_directory_for_plain_file_name() {
        assert_eq!(
            super::debug_log_location(&PathBuf::from("debug.log")),
            (PathBuf::from("."), OsString::from("debug.log"))
        );
    }

    #[test]
    fn debug_log_location_splits_directory_and_file_name() {
        assert_eq!(
            super::debug_log_location(&PathBuf::from("target/custom-cutout-debug.log")),
            (
                PathBuf::from("target"),
                OsString::from("custom-cutout-debug.log")
            )
        );
    }
}
