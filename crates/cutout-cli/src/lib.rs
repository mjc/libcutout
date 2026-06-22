#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

//! Application layer for the Cutout command-line interface.

mod cli;
mod commands;
mod dashboard;
mod logging;
mod validation;

pub use cli::Cli;
pub use commands::run;
pub use logging::init_logging;
