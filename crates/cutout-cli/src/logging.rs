use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, mpsc},
};

use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    EnvFilter, fmt, layer::Context, layer::SubscriberExt, registry, util::SubscriberInitExt,
};

use crate::dashboard::DashboardUpdate;

const DEFAULT_DEBUG_LOG_PATH: &str = "debug.log";

static DASHBOARD_LOG_SINK: OnceLock<Mutex<Option<mpsc::Sender<DashboardUpdate>>>> = OnceLock::new();

pub fn init_logging(dashboard_mode: bool) -> Option<WorkerGuard> {
    let rust_log = std::env::var_os("RUST_LOG");
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new("cutout_cli=info"),
    };

    if let Some(path) = debug_log_path(std::env::var_os("CUTOUT_DEBUG_LOG"), rust_log.as_ref()) {
        let (directory, file_name) = debug_log_location(&path);
        let file_appender = tracing_appender::rolling::never(directory, file_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let layer = fmt::layer()
            .compact()
            .with_target(true)
            .with_writer(non_blocking);
        let subscriber = registry().with(filter).with(DashboardLogLayer).with(layer);
        let _ = subscriber.try_init();
        return Some(guard);
    } else if dashboard_mode {
        let subscriber = registry().with(filter).with(DashboardLogLayer).with(
            fmt::layer()
                .compact()
                .with_target(true)
                .with_writer(io::sink),
        );
        let _ = subscriber.try_init();
    } else {
        let subscriber = registry().with(filter).with(DashboardLogLayer).with(
            fmt::layer()
                .compact()
                .without_time()
                .with_target(false)
                .with_writer(io::stderr),
        );
        let _ = subscriber.try_init();
    }
    None
}

pub(crate) fn install_dashboard_log_sink(
    sender: mpsc::Sender<DashboardUpdate>,
) -> DashboardLogSinkGuard {
    let sink = dashboard_log_sink();
    let mut guard = sink
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = Some(sender);
    DashboardLogSinkGuard
}

fn dashboard_log_sink() -> &'static Mutex<Option<mpsc::Sender<DashboardUpdate>>> {
    DASHBOARD_LOG_SINK.get_or_init(|| Mutex::new(None))
}

pub(crate) struct DashboardLogSinkGuard;

impl Drop for DashboardLogSinkGuard {
    fn drop(&mut self) {
        let mut guard = dashboard_log_sink()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = None;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DashboardLogLayer;

impl<S> tracing_subscriber::Layer<S> for DashboardLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let target = event.metadata().target();
        if !target.starts_with("cutout_") {
            return;
        }

        let rendered = render_dashboard_log_message(render_event_parts(event));
        if rendered.is_empty() {
            return;
        }

        let level = event.metadata().level().as_str().to_ascii_lowercase();
        let sink = dashboard_log_sink();
        let guard = sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = guard.as_ref() {
            let _ = sender.send(DashboardUpdate::Log {
                level,
                message: rendered,
            });
        }
    }
}

fn render_event_parts(event: &Event<'_>) -> DashboardEventParts {
    let mut visitor = DashboardEventVisitor::default();
    event.record(&mut visitor);
    visitor.finish()
}

fn render_dashboard_log_message(parts: DashboardEventParts) -> String {
    let mut rendered = String::new();
    if let Some(message) = parts.message.filter(|message| !message.is_empty()) {
        rendered.push_str(&message);
    }
    for field in parts.fields {
        if !rendered.is_empty() {
            rendered.push(' ');
        }
        rendered.push_str(&field);
    }
    rendered
}

#[derive(Default)]
struct DashboardEventVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl DashboardEventVisitor {
    fn finish(self) -> DashboardEventParts {
        DashboardEventParts {
            message: self.message,
            fields: self.fields,
        }
    }
}

#[derive(Debug, Default)]
struct DashboardEventParts {
    message: Option<String>,
    fields: Vec<String>,
}

impl tracing_subscriber::field::Visit for DashboardEventVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(strip_quotes(rendered));
        } else {
            self.fields
                .push(format!("{}={}", field.name(), strip_quotes(rendered)));
        }
    }
}

fn strip_quotes(value: String) -> String {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value[1..value.len() - 1].to_owned()
    } else {
        value
    }
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
    use super::*;

    #[test]
    fn debug_log_path_defaults_to_working_directory_file() {
        assert_eq!(
            debug_log_path(None, Some(&OsString::from("debug"))),
            Some(PathBuf::from("debug.log"))
        );
    }

    #[test]
    fn debug_log_location_splits_directory_and_file_name() {
        assert_eq!(
            debug_log_location(&PathBuf::from("target/custom-cutout-debug.log")),
            (
                PathBuf::from("target"),
                OsString::from("custom-cutout-debug.log")
            )
        );
    }

    #[test]
    fn render_dashboard_log_message_combines_message_and_fields() {
        let rendered = render_dashboard_log_message(DashboardEventParts {
            message: Some("dashboard live update".to_owned()),
            fields: vec!["iteration=12".to_owned(), "note=armed".to_owned()],
        });

        assert_eq!(rendered, "dashboard live update iteration=12 note=armed");
    }

    #[test]
    fn strip_quotes_removes_matching_delimiters_only() {
        assert_eq!(strip_quotes("\"quoted\"".to_owned()), "quoted");
        assert_eq!(strip_quotes("not quoted".to_owned()), "not quoted");
    }
}
