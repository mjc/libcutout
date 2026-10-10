//! Ordered ride marker writes without waiting for the database worker.
use crate::{PendingRideSessionMarkerWrite, RideDatabase};
use std::sync::{Arc, Mutex, PoisonError};

const RETRY_AFTER_MS: u64 = 500;

/// Receipt state for the latest requested marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RideSessionMarkerWriteStatus {
    /// The latest request is durable.
    Committed,
    /// A worker command has not completed.
    Pending,
    /// Storage rejected the write; the desired value remains retryable.
    Retrying(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MarkerIntent {
    Save(Vec<u8>),
    Clear,
}

#[derive(Debug)]
struct InFlightMarker {
    marker: MarkerIntent,
    receipt: PendingRideSessionMarkerWrite,
}

/// One bounded writer for the process-owned opaque ride marker.
#[derive(Debug)]
pub struct RideSessionMarkerWriter {
    database: RideDatabase,
    state: Arc<Mutex<MarkerWriteState>>,
}

#[derive(Debug, Default)]
pub(crate) struct MarkerWriteState {
    desired: Option<MarkerIntent>,
    acknowledged: Option<MarkerIntent>,
    in_flight: Option<InFlightMarker>,
    retry_at_ms: u64,
    last_error: Option<String>,
}

impl RideSessionMarkerWriter {
    /// Constructs a writer using the existing database service.
    #[must_use]
    pub fn new(database: RideDatabase) -> Self {
        let state = Arc::clone(&database.marker_writes);
        Self { database, state }
    }

    /// Requests the latest opaque marker, or its removal, without waiting for SQLite.
    pub fn request(&self, marker: Option<Vec<u8>>, now_ms: u64) -> RideSessionMarkerWriteStatus {
        let marker = marker.map_or(MarkerIntent::Clear, MarkerIntent::Save);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.desired.as_ref() != Some(&marker) {
            state.desired = Some(marker);
            state.retry_at_ms = 0;
            state.last_error = None;
        }
        state.poll(&self.database, now_ms)
    }

    /// Imports an older native preference only before any current-process intent exists.
    pub fn migrate(&self, marker: Option<Vec<u8>>, now_ms: u64) -> RideSessionMarkerWriteStatus {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.desired.is_none() {
            state.desired = Some(marker.map_or(MarkerIntent::Clear, MarkerIntent::Save));
        }
        state.poll(&self.database, now_ms)
    }

    /// Reads receipt state without opening storage or admitting another command.
    #[must_use]
    pub fn status(&self) -> RideSessionMarkerWriteStatus {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .status()
    }

    /// Reads only a successful durable marker receipt; unknown and cleared markers return None.
    #[must_use]
    pub fn acknowledged_marker(&self) -> Option<Vec<u8>> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match &state.acknowledged {
            Some(MarkerIntent::Save(bytes)) => Some(bytes.clone()),
            Some(MarkerIntent::Clear) | None => None,
        }
    }

    /// Polls durable receipts on the owned background worker, including worker recovery.
    ///
    /// Worker restart can open SQLite, so call this method off the platform UI executor.
    pub fn poll(&self, now_ms: u64) -> RideSessionMarkerWriteStatus {
        // Recovery never owns the marker-state lock: callbacks can still replace desired intent.
        let database = self
            .database
            .recover_marker_worker()
            .unwrap_or_else(|_| self.database.clone());
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .poll(&database, now_ms)
    }
}

impl MarkerWriteState {
    fn status(&self) -> RideSessionMarkerWriteStatus {
        if self.in_flight.is_some() {
            RideSessionMarkerWriteStatus::Pending
        } else if self.desired.is_none() || self.desired == self.acknowledged {
            RideSessionMarkerWriteStatus::Committed
        } else if let Some(message) = &self.last_error {
            RideSessionMarkerWriteStatus::Retrying(message.clone())
        } else {
            RideSessionMarkerWriteStatus::Pending
        }
    }

    fn poll(&mut self, database: &RideDatabase, now_ms: u64) -> RideSessionMarkerWriteStatus {
        if let Some(mut in_flight) = self.in_flight.take() {
            match in_flight.receipt.try_result() {
                None => {
                    self.in_flight = Some(in_flight);
                    return RideSessionMarkerWriteStatus::Pending;
                }
                Some(Ok(())) => {
                    self.acknowledged = Some(in_flight.marker);
                    self.last_error = None;
                    self.retry_at_ms = 0;
                }
                Some(Err(error)) => {
                    if self.desired.as_ref() == Some(&in_flight.marker) {
                        self.last_error = Some(error.to_string());
                        self.retry_at_ms = now_ms.saturating_add(RETRY_AFTER_MS);
                    }
                }
            }
        }
        if self.desired.is_none() || self.desired == self.acknowledged {
            return RideSessionMarkerWriteStatus::Committed;
        }
        if now_ms < self.retry_at_ms {
            return RideSessionMarkerWriteStatus::Retrying(
                self.last_error.clone().unwrap_or_default(),
            );
        }
        let Some(marker) = self.desired.clone() else {
            return RideSessionMarkerWriteStatus::Committed;
        };
        let bytes = match &marker {
            MarkerIntent::Save(bytes) => Some(bytes.clone()),
            MarkerIntent::Clear => None,
        };
        match database.queue_ride_session_marker(bytes) {
            Ok(receipt) => {
                self.in_flight = Some(InFlightMarker { marker, receipt });
                RideSessionMarkerWriteStatus::Pending
            }
            Err(error) => {
                let message = error.to_string();
                self.last_error = Some(message.clone());
                self.retry_at_ms = now_ms.saturating_add(RETRY_AFTER_MS);
                RideSessionMarkerWriteStatus::Retrying(message)
            }
        }
    }
}
