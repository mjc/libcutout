//! Application-owned orchestration for durable ride data.
//!
//! The persistence crate owns SQLite mechanics. This service owns the process-level workflows
//! that span calls or combine database operations, so interface crates only adapt their public
//! values and errors.

use libcutout_persistence::{
    BmsVoltageSampleRecord, PendingBmsVoltageWrite, RideDatabase, RideSessionMarkerWriteStatus,
    RideSessionMarkerWriter, StorageError,
};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use thiserror::Error;

const MAX_PENDING_BMS_WRITES: usize = 64;
/// Maximum number of raw BMS samples admitted in one asynchronous batch.
pub const MAX_BMS_SAMPLES_PER_BATCH: usize = 256;

/// Stable identity and terminal result of an asynchronously persisted BMS batch.
#[derive(Debug)]
pub struct BmsVoltageWriteOutcome {
    /// Identifier allocated when the batch was admitted.
    pub request_id: u64,
    /// Durable result, present only after storage finishes the batch.
    pub result: Result<(), StorageError>,
}

/// Failure admitting an asynchronous BMS write batch.
#[derive(Debug, Error)]
pub enum BmsVoltageWriteAdmissionError {
    /// The interface-level bounded batch size was exceeded.
    #[error("BMS voltage batch is too large")]
    BatchTooLarge,
    /// The process-local request identifier space was exhausted.
    #[error("BMS voltage request identifier space was exhausted")]
    RequestIdExhausted,
    /// The application-owned set of outstanding requests reached its bound.
    #[error("too many pending BMS voltage writes")]
    PendingQueueFull,
    /// The pending application queue or persistence worker rejected the batch.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Debug)]
struct PendingBmsVoltageWrites {
    next_request_id: u64,
    writes: VecDeque<(u64, PendingBmsVoltageWrite)>,
}

/// Process-scoped durable ride-data operations used by native and Slint front ends.
///
/// The service owns bounded BMS submission and completion across calls, and the ordered opaque
/// ride-marker write workflow. The underlying database remains independently usable through its
/// persistence API for ordinary single-call storage operations.
#[derive(Debug)]
pub struct RideDatabaseApplication {
    database: RideDatabase,
    pending_bms_writes: Mutex<PendingBmsVoltageWrites>,
    marker_writer: RideSessionMarkerWriter,
}

impl RideDatabaseApplication {
    /// Opens the process-scoped database service.
    ///
    /// # Errors
    /// Returns the persistence error when the database cannot be opened or initialized.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        RideDatabase::open(path).map(Self::new)
    }

    /// Creates the application operations around an already-open database handle.
    #[must_use]
    pub fn new(database: RideDatabase) -> Self {
        Self {
            marker_writer: RideSessionMarkerWriter::new(database.clone()),
            database,
            pending_bms_writes: Mutex::new(PendingBmsVoltageWrites {
                next_request_id: 1,
                writes: VecDeque::new(),
            }),
        }
    }

    /// Provides the underlying persistence handle for single-call database operations.
    #[must_use]
    pub fn persistence(&self) -> &RideDatabase {
        &self.database
    }

    /// Stores raw BMS samples independently of ride lifecycle.
    ///
    /// # Errors
    /// Returns the persistence error if the sample batch cannot be stored.
    pub fn record_bms_voltage_samples(
        &self,
        samples: &[BmsVoltageSampleRecord],
    ) -> Result<(), StorageError> {
        self.database.record_bms_voltage_samples(samples)
    }

    /// Admits one bounded BMS batch and retains its completion for a later poll.
    ///
    /// Empty batches are ignored. The operation checks the public batch bound and process-local
    /// outstanding bound before queue admission, and does not consume a request ID on failure.
    ///
    /// # Errors
    /// Returns an error when the batch exceeds its bound, request IDs are exhausted, the pending
    /// queue is full, or persistence rejects the write.
    pub fn queue_bms_voltage_samples(
        &self,
        samples: Vec<BmsVoltageSampleRecord>,
    ) -> Result<Option<u64>, BmsVoltageWriteAdmissionError> {
        if samples.len() > MAX_BMS_SAMPLES_PER_BATCH {
            return Err(BmsVoltageWriteAdmissionError::BatchTooLarge);
        }
        if samples.is_empty() {
            return Ok(None);
        }

        let mut pending = self
            .pending_bms_writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if pending.writes.len() >= MAX_PENDING_BMS_WRITES {
            return Err(BmsVoltageWriteAdmissionError::PendingQueueFull);
        }
        let next_request_id = pending
            .next_request_id
            .checked_add(1)
            .ok_or(BmsVoltageWriteAdmissionError::RequestIdExhausted)?;
        let write = self.database.queue_bms_voltage_samples(samples)?;
        let request_id = pending.next_request_id;
        pending.next_request_id = next_request_id;
        pending.writes.push_back((request_id, write));
        Ok(Some(request_id))
    }

    /// Returns completed BMS batches in submission order, retaining unfinished batches.
    pub fn poll_bms_voltage_writes(&self) -> Vec<BmsVoltageWriteOutcome> {
        let mut pending = self
            .pending_bms_writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut completed = Vec::new();
        let mut remaining = VecDeque::with_capacity(pending.writes.len());
        while let Some((request_id, mut write)) = pending.writes.pop_front() {
            if let Some(result) = write.try_result() {
                completed.push(BmsVoltageWriteOutcome { request_id, result });
            } else {
                remaining.push_back((request_id, write));
            }
        }
        pending.writes = remaining;
        completed
    }

    /// Waits for all queued BMS writes in submission order and consumes their outcomes.
    ///
    /// This is a lifecycle barrier and may block; platform callback paths should use
    /// [`Self::poll_bms_voltage_writes`] instead. Admission remains excluded until every
    /// currently queued result has been observed, matching the original handle behavior.
    pub fn finish_bms_voltage_writes(&self) -> Vec<BmsVoltageWriteOutcome> {
        let mut pending = self
            .pending_bms_writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        pending
            .writes
            .drain(..)
            .map(|(request_id, write)| BmsVoltageWriteOutcome {
                request_id,
                result: write.wait_result(),
            })
            .collect()
    }

    /// Admits marker replacement or removal without waiting for SQLite.
    pub fn request_ride_session_marker(
        &self,
        marker: Option<Vec<u8>>,
        now_milliseconds: u64,
    ) -> RideSessionMarkerWriteStatus {
        self.marker_writer.request(marker, now_milliseconds)
    }

    /// Fences preference migration behind any marker intent already accepted this process.
    pub fn migrate_ride_session_marker(
        &self,
        marker: Option<Vec<u8>>,
        now_milliseconds: u64,
    ) -> RideSessionMarkerWriteStatus {
        self.marker_writer.migrate(marker, now_milliseconds)
    }

    /// Polls durability and retry state. Call off the UI executor because recovery can open SQLite.
    pub fn poll_ride_session_marker(&self, now_milliseconds: u64) -> RideSessionMarkerWriteStatus {
        self.marker_writer.poll(now_milliseconds)
    }

    /// Reads the latest marker-write status without waiting on storage.
    #[must_use]
    pub fn ride_session_marker_write_status(&self) -> RideSessionMarkerWriteStatus {
        self.marker_writer.status()
    }

    /// Returns marker bytes only after durable acknowledgement.
    #[must_use]
    pub fn acknowledged_ride_session_marker(&self) -> Option<Vec<u8>> {
        self.marker_writer.acknowledged_marker()
    }
}

#[cfg(test)]
mod tests {
    use super::{BmsVoltageWriteAdmissionError, RideDatabaseApplication};
    use libcutout_persistence::BmsVoltageSampleRecord;
    use std::sync::Mutex;
    use std::sync::PoisonError;
    use uuid::Uuid;

    static DATABASE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn bms_queue_bounds_admission_and_correlates_one_terminal_result() {
        let _guard = DATABASE_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let path = std::env::temp_dir().join(format!(
            "cutout-application-bms-{}-{}.sqlite3",
            std::process::id(),
            Uuid::new_v4()
        ));
        let application = RideDatabaseApplication::open(&path).expect("database opens");
        let sample = BmsVoltageSampleRecord::new(
            "wheel-a",
            "session-a",
            1,
            1_000,
            1_700_000_000_000,
            0,
            4_193,
        )
        .expect("sample is valid");

        assert_eq!(
            application
                .queue_bms_voltage_samples(Vec::new())
                .expect("empty batch is ignored"),
            None
        );
        assert!(matches!(
            application.queue_bms_voltage_samples(vec![sample.clone(); 257]),
            Err(BmsVoltageWriteAdmissionError::BatchTooLarge)
        ));

        let request_id = application
            .queue_bms_voltage_samples(vec![sample])
            .expect("bounded write is admitted")
            .expect("non-empty batch receives a request ID");
        application
            .persistence()
            .integrity_check()
            .expect("FIFO barrier waits for the BMS write");
        assert_eq!(
            application
                .poll_bms_voltage_writes()
                .into_iter()
                .map(|outcome| (outcome.request_id, outcome.result.is_ok()))
                .collect::<Vec<_>>(),
            vec![(request_id, true)]
        );
        assert!(application.poll_bms_voltage_writes().is_empty());

        drop(application);
        let _ = std::fs::remove_file(path);
    }
}
