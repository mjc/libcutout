#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Rust-owned `SQLite` persistence for rides, maps, and mobile state.

mod storage;
pub use storage::*;

#[cfg(test)]
mod tests;
