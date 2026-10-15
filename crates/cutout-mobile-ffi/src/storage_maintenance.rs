//! Explicit, lossless ride database maintenance for mobile clients.

use crate::{RideDatabaseHandle, persistence};

/// Summary of a completed storage maintenance pass.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileStorageMaintenanceReportDto {
    /// Database bytes before maintenance.
    pub bytes_before: u64,
    /// Database bytes after maintenance.
    pub bytes_after: u64,
    /// Legacy event payloads recompressed without changing their contents.
    pub compressed_event_count: u64,
}

/// Typed result of an explicitly requested storage maintenance pass.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileStorageMaintenanceOutcomeDto {
    /// The database maintenance pass completed.
    Completed {
        report: MobileStorageMaintenanceReportDto,
    },
    /// A ride is active or paused.
    ActiveRide,
    /// A live Bluetooth capture is active.
    ActiveCapture,
    /// A PEVCAP capture import is active or has not yet published its receipt.
    PevcapImportInProgress,
    /// The filesystem does not have enough free space for safe compaction.
    InsufficientSpace {
        required_bytes: u64,
        available_bytes: u64,
    },
    /// Maintenance could not complete for another storage reason.
    Failed,
}

#[uniffi::export]
impl RideDatabaseHandle {
    /// Queues lossless storage maintenance on the serialized database worker and waits for its
    /// durable result. Mobile callers must invoke this blocking method off the main actor.
    pub fn optimize_storage(&self) -> MobileStorageMaintenanceOutcomeDto {
        match self.inner.queue_storage_maintenance() {
            Ok(pending) => storage_maintenance_outcome(pending.wait_result()),
            Err(error) => storage_maintenance_error(&error),
        }
    }
}

fn storage_maintenance_outcome(
    result: Result<persistence::DatabaseMaintenanceReport, persistence::StorageError>,
) -> MobileStorageMaintenanceOutcomeDto {
    match result {
        Ok(report) => MobileStorageMaintenanceOutcomeDto::Completed {
            report: MobileStorageMaintenanceReportDto {
                bytes_before: report.bytes_before,
                bytes_after: report.bytes_after,
                compressed_event_count: report.compressed_event_count,
            },
        },
        Err(error) => storage_maintenance_error(&error),
    }
}

fn storage_maintenance_error(
    error: &persistence::StorageError,
) -> MobileStorageMaintenanceOutcomeDto {
    match error {
        persistence::StorageError::MaintenanceActiveRide => {
            MobileStorageMaintenanceOutcomeDto::ActiveRide
        }
        persistence::StorageError::MaintenanceActiveCapture => {
            MobileStorageMaintenanceOutcomeDto::ActiveCapture
        }
        persistence::StorageError::MaintenancePevcapImportInProgress => {
            MobileStorageMaintenanceOutcomeDto::PevcapImportInProgress
        }
        persistence::StorageError::MaintenanceInsufficientSpace {
            required_bytes,
            available_bytes,
        } => MobileStorageMaintenanceOutcomeDto::InsufficientSpace {
            required_bytes: *required_bytes,
            available_bytes: *available_bytes,
        },
        _ => MobileStorageMaintenanceOutcomeDto::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintenance_refusals_keep_their_typed_reason() {
        assert_eq!(
            storage_maintenance_error(&persistence::StorageError::MaintenanceActiveRide),
            MobileStorageMaintenanceOutcomeDto::ActiveRide
        );
        assert_eq!(
            storage_maintenance_error(&persistence::StorageError::MaintenanceActiveCapture),
            MobileStorageMaintenanceOutcomeDto::ActiveCapture
        );
        assert_eq!(
            storage_maintenance_error(
                &persistence::StorageError::MaintenancePevcapImportInProgress
            ),
            MobileStorageMaintenanceOutcomeDto::PevcapImportInProgress
        );
        assert_eq!(
            storage_maintenance_error(&persistence::StorageError::MaintenanceInsufficientSpace {
                required_bytes: 16_384,
                available_bytes: 8_192,
            }),
            MobileStorageMaintenanceOutcomeDto::InsufficientSpace {
                required_bytes: 16_384,
                available_bytes: 8_192,
            }
        );
    }
}
