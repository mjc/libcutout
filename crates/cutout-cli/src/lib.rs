#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Application layer for the Cutout command-line interface.

mod cli;
mod commands;
mod dashboard;
mod logging;
mod validation;

pub use cli::Cli;
pub use commands::run;
pub use dashboard::install_dashboard_signal_restore;
pub use logging::init_logging;
