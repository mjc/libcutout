use std::{future::Future, time::Duration};

use cutout_core::{GattChannel, NotificationByteLen};
use thiserror::Error;

use crate::NegotiatedWriteLimit;

const BACKEND_OPERATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Errors surfaced while bridging protocol outputs to BTLE operations.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum SessionBridgeError {
    /// Session output referenced a channel that does not match the selected binding.
    #[error("bridge saw channel {observed:?} but expected {expected:?}")]
    UnexpectedChannel {
        /// Expected binding channel.
        expected: GattChannel,

        /// Observed channel from the session output.
        observed: GattChannel,
    },

    /// The session requested notification subscription but no notify endpoint was selected.
    #[error("bridge needs a notify-capable endpoint for channel {channel:?}")]
    MissingNotifyEndpoint {
        /// Abstract protocol channel requested for subscription.
        channel: GattChannel,
    },

    /// The discovered GATT tree did not contain a writable session endpoint.
    #[error("bridge needs a writable session endpoint")]
    MissingSessionEndpoint,

    /// A write chunk exceeded the negotiated BTLE write limit.
    #[error("bridge write chunk length {len} exceeded negotiated limit {limit}")]
    WriteChunkTooLong {
        /// Observed chunk length.
        len: NotificationByteLen,

        /// Negotiated write limit.
        limit: NegotiatedWriteLimit,
    },

    /// Session tried to emit transport work while handling an externally lost link.
    #[error("bridge cannot process transport actions after external link loss")]
    ExternalLinkDownTransportAction,
}

/// Errors surfaced by the BTLE adapter.
#[derive(Debug, Error)]
pub enum BtleError {
    /// No Bluetooth adapters were reported by the platform.
    #[error("no bluetooth adapters were found")]
    NoAdapterAvailable,

    /// No peripheral matched the requested target.
    #[error("no peripheral matched the requested target")]
    NoPeripheralMatched,

    /// Error reported by the underlying BTLE stack.
    #[error(transparent)]
    Backend(#[from] btleplug::Error),

    /// The underlying BTLE stack did not finish an operation in time.
    #[error("bluetooth operation timed out: {operation} after {after:?}")]
    OperationTimedOut {
        /// Operation that timed out.
        operation: &'static str,

        /// Timeout duration.
        after: Duration,
    },

    /// Scan cleanup failed after the scan operation otherwise succeeded.
    #[error("bluetooth scan cleanup failed after successful scan: {cleanup}")]
    ScanCleanupFailed {
        /// Failure reported while stopping the scan.
        cleanup: Box<BtleError>,
    },

    /// Scan cleanup failed after the scan operation also failed.
    #[error("bluetooth scan failed: {primary}; cleanup also failed: {cleanup}")]
    ScanFailedWithCleanup {
        /// Primary scan failure.
        primary: Box<BtleError>,

        /// Failure reported while stopping the scan.
        cleanup: Box<BtleError>,
    },

    /// Error reported by the session bridge.
    #[error(transparent)]
    Bridge(#[from] SessionBridgeError),
}

impl BtleError {
    /// Returns a concise user-facing hint for common desktop BLE failures.
    #[must_use]
    pub const fn diagnostic_hint(&self) -> &'static str {
        match self {
            Self::NoAdapterAvailable => {
                "enable Bluetooth, grant Bluetooth permission to this terminal, and verify the OS exposes an adapter"
            }
            Self::NoPeripheralMatched => {
                "power on the device, keep it nearby, increase --seconds, or use --name-contains/--identifier to narrow selection"
            }
            Self::OperationTimedOut { .. } => {
                "retry the operation, move closer to the device, and check whether another app is holding the BLE connection"
            }
            Self::ScanCleanupFailed { .. } | Self::ScanFailedWithCleanup { .. } => {
                "retry scanning, verify the adapter is still present, and include the cleanup failure in capture notes"
            }
            Self::Bridge(SessionBridgeError::MissingSessionEndpoint) => {
                "inspect GATT services and select a device exposing a writable and notify-capable session characteristic"
            }
            Self::Bridge(SessionBridgeError::MissingNotifyEndpoint { .. }) => {
                "inspect GATT services and select a notify-capable characteristic for the session channel"
            }
            Self::Bridge(SessionBridgeError::UnexpectedChannel { .. }) => {
                "report this protocol binding mismatch with the selected profile, GATT inventory, and capture logs"
            }
            Self::Bridge(SessionBridgeError::WriteChunkTooLong { .. }) => {
                "report this negotiated write chunking bug with the selected profile, MTU, and capture logs"
            }
            Self::Bridge(SessionBridgeError::ExternalLinkDownTransportAction) => {
                "report this session lifecycle bug with the selected profile and capture logs"
            }
            Self::Backend(_) => {
                "check OS Bluetooth permissions, adapter state, and whether another app is already connected"
            }
        }
    }
}

pub(crate) async fn backend_call<T, F>(operation: &'static str, future: F) -> Result<T, BtleError>
where
    F: Future<Output = Result<T, btleplug::Error>>,
{
    tokio::time::timeout(BACKEND_OPERATION_TIMEOUT, future)
        .await
        .map_err(|_| BtleError::OperationTimedOut {
            operation,
            after: BACKEND_OPERATION_TIMEOUT,
        })?
        .map_err(BtleError::Backend)
}
