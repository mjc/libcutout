#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

//! Application layer for the Cutout command-line interface.

mod cli;
mod commands;
mod dashboard;

pub use cli::Cli;
pub use commands::run;
