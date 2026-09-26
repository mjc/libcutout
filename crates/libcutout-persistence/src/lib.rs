#![cfg_attr(test, allow(clippy::disallowed_macros))]
#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]

//! Rust-owned `SQLite` persistence for rides, maps, and mobile state.

mod capture_writer;
mod pevcap_limits;
mod storage;
pub use capture_writer::{
    CAPTURE_LOCATION_BATCH_CAPACITY, CaptureArtifactId, CaptureMetadata, CaptureWriteOutcome,
    CaptureWriter, CaptureWriterIngress, CaptureWriterMonitor, CaptureWriterStatus,
    SavedCaptureArtifact,
};
pub use storage::*;

#[cfg(test)]
mod tests;
