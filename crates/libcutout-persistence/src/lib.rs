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
mod recording;
mod ride_session_marker;
mod storage;
pub use capture_writer::{
    CAPTURE_LOCATION_BATCH_CAPACITY, CaptureArtifactId, CaptureFlushOutcome, CaptureJsonlExport,
    CaptureMetadata, CaptureRecordAdmission, CaptureRecordingPolicy, CaptureWriteOutcome,
    CaptureWriter, CaptureWriterFinish, CaptureWriterIngress, CaptureWriterMonitor,
    CaptureWriterStatus, SavedCaptureArtifact, SavedDatabaseCapture,
};
pub use recording::{
    LocationAcquisition, LocationAcquisitionState, LocationAuthorization, LocationAvailability,
    LocationDemand, LocationEnvironment, LocationRecoveryAction, RecordingError, RecordingSnapshot,
    RecordingSpeed, RecordingSpeedSource, RecordingSpeedState, RecordingToken,
    RideMotionObservation, RideRecordingSession, location_acquisition_for,
};
pub use ride_session_marker::{RideSessionMarkerWriteStatus, RideSessionMarkerWriter};
pub use storage::*;

#[cfg(test)]
mod tests;
