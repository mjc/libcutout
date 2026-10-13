//! Charging commands executed by the existing database owner.
use super::{
    Command as DatabaseCommand, GeoBounds, MapPointCursor, MapPointId, QueryLimit, Reply,
    RideDatabase, SpatialSchemaState, StorageError,
};
use cutout_ride_maps::{ChargingVote, Coordinate, NewChargingSpot};
use rusqlite::Connection;

mod codec;
mod model;
mod mutations;
mod queries;
mod records;
mod sync;

pub use model::{
    ChargingAction, ChargingDirectoryEntry, ChargingFilter, ChargingOperation, ChargingOperationId,
    ChargingPage, ChargingRevision, ChargingStatus, ChargingTarget, ChargingWriteReceipt,
    StoredChargingSpot,
};
use mutations::{change, save};
use queries::{nearby, page};
use records::detail;
use sync::{acknowledge_operation, cache_directory_entry, count, next_operation};

/// Maximum number of pending community writes retained by an installation.
pub const CHARGING_OUTBOX_CAPACITY: u32 = 1_024;
pub(super) const SCHEMA: &str = "
CREATE TABLE charging_spots (
 point_id BLOB PRIMARY KEY NOT NULL REFERENCES map_points(id) ON DELETE CASCADE,
 input TEXT CHECK(input IS NULL OR (json_valid(input) AND length(CAST(input AS BLOB)) <= 32768)),
 owned INTEGER NOT NULL CHECK(owned IN (0,1)),
 status TEXT NOT NULL CHECK(status IN ('queued','published','hidden','withdrawn','removed')),
 local_revision INTEGER NOT NULL CHECK(local_revision > 0),
 remote_id TEXT UNIQUE, remote_revision INTEGER NOT NULL DEFAULT 0 CHECK(remote_revision >= 0),
 change_sequence INTEGER NOT NULL DEFAULT 0 CHECK(change_sequence >= 0),
 verified_at_ms INTEGER CHECK(verified_at_ms IS NULL OR verified_at_ms >= 0),
 vote TEXT CHECK(vote IS NULL OR vote IN ('up','down')),
 vote_revision INTEGER NOT NULL DEFAULT 0 CHECK(vote_revision >= 0),
 votes_up INTEGER NOT NULL DEFAULT 0 CHECK(votes_up >= 0),
 votes_down INTEGER NOT NULL DEFAULT 0 CHECK(votes_down >= 0),
 last_ack TEXT
);
CREATE TABLE charging_outbox (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 operation_id TEXT NOT NULL UNIQUE,
 point_id BLOB NOT NULL REFERENCES charging_spots(point_id),
 action TEXT NOT NULL CHECK(json_valid(action) AND length(CAST(action AS BLOB)) <= 32768),
 local_revision INTEGER NOT NULL CHECK(local_revision > 0),
 target TEXT CHECK(target IS NULL OR json_valid(target))
);
CREATE INDEX charging_outbox_point ON charging_outbox(point_id,sequence);
";

pub(super) enum Command {
    Save {
        input: NewChargingSpot,
        reply: Reply<ChargingWriteReceipt>,
    },
    Change {
        id: MapPointId,
        expected: ChargingRevision,
        action: ChargingAction,
        reply: Reply<ChargingWriteReceipt>,
    },
    Detail {
        id: MapPointId,
        reply: Reply<Option<StoredChargingSpot>>,
    },
    Page {
        bounds: GeoBounds,
        filter: ChargingFilter,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
        reply: Reply<ChargingPage>,
    },
    Nearby {
        bounds: GeoBounds,
        origin: Coordinate,
        filter: ChargingFilter,
        limit: QueryLimit,
        reply: Reply<Vec<StoredChargingSpot>>,
    },
    Next {
        reply: Reply<Option<ChargingOperation>>,
    },
    Cache {
        entry: ChargingDirectoryEntry,
        reply: Reply<MapPointId>,
    },
    Acknowledge {
        id: ChargingOperationId,
        applied: ChargingRevision,
        entry: ChargingDirectoryEntry,
        reply: Reply<MapPointId>,
    },
    Count {
        reply: Reply<u32>,
    },
}
impl RideDatabase {
    /// Atomically stores a new minimum-field spot and queues its community submission.
    /// # Errors
    /// Returns storage failures or a full outbox; neither leaves a partial pin.
    pub fn save_charging_spot(
        &self,
        input: NewChargingSpot,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Save { input, reply }))
    }
    /// Edits/moves one owned spot without changing its UUID.
    /// # Errors
    /// Rejects stale revisions, tombstones, directory-owned pins and full queues.
    pub fn edit_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
        input: NewChargingSpot,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Edit(input))
    }
    /// Withdraws an owned pin and queues the server withdrawal, including before first publication.
    /// # Errors
    /// Rejects stale revisions, tombstones, non-owned pins and full queues.
    pub fn withdraw_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Withdraw)
    }
    /// Queues explicit access verification without fabricating a confirmation timestamp.
    /// # Errors
    /// Rejects stale revisions and spots not currently available for verification.
    pub fn verify_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Verify)
    }
    /// Queues the requested vote state without changing confirmed counts.
    /// # Errors
    /// Rejects stale revisions and spots not currently available for voting.
    pub fn vote_charging_spot(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
        vote: Option<ChargingVote>,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.charging_change(id, expected, ChargingAction::Vote(vote))
    }
    fn charging_change(
        &self,
        id: MapPointId,
        expected: ChargingRevision,
        action: ChargingAction,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Change {
                id,
                expected,
                action,
                reply,
            })
        })
    }
    /// Reads charging detail; legacy points have no charging row.
    /// # Errors
    /// Returns worker/storage failures, never invented defaults for absent metadata.
    pub fn charging_spot(
        &self,
        id: MapPointId,
    ) -> Result<Option<StoredChargingSpot>, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Detail { id, reply }))
    }
    /// Queries visible charging spots through the existing `RTree` with bounded pagination.
    /// # Errors
    /// Returns worker or storage failures.
    pub fn charging_spots_in_bounds(
        &self,
        bounds: GeoBounds,
        filter: ChargingFilter,
        cursor: Option<MapPointCursor>,
        limit: QueryLimit,
    ) -> Result<ChargingPage, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Page {
                bounds,
                filter,
                cursor,
                limit,
                reply,
            })
        })
    }
    /// Ranks all matching spots in the bounds by exact great-circle distance with bounded memory.
    /// # Errors
    /// Returns worker or storage failures.
    pub fn nearby_charging_spots(
        &self,
        bounds: GeoBounds,
        origin: Coordinate,
        filter: ChargingFilter,
        limit: QueryLimit,
    ) -> Result<Vec<StoredChargingSpot>, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Nearby {
                bounds,
                origin,
                filter,
                limit,
                reply,
            })
        })
    }
    /// Freezes and reads the oldest pending command; repeated reads return identical retry data.
    /// # Errors
    /// Returns worker/storage failures or inconsistent retained queue data.
    pub fn next_charging_operation(&self) -> Result<Option<ChargingOperation>, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Next { reply }))
    }
    /// Returns the bounded pending queue size.
    /// # Errors
    /// Returns worker/storage failures.
    pub fn charging_outbox_count(&self) -> Result<u32, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Count { reply }))
    }
    /// Caches a server snapshot using the same map-point owner and spatial index.
    /// # Errors
    /// Rejects malformed/stale snapshots and refuses to overwrite pending local changes.
    pub fn cache_charging_spot(
        &self,
        entry: ChargingDirectoryEntry,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Cache { entry, reply }))
    }
    /// Atomically accepts a matching server receipt and advances the pending queue.
    /// # Errors
    /// Rejects incorrect identities/revisions and receipts for an unclaimed or later operation.
    pub fn acknowledge_charging_operation(
        &self,
        id: ChargingOperationId,
        applied: ChargingRevision,
        entry: ChargingDirectoryEntry,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::Acknowledge {
                id,
                applied,
                entry,
                reply,
            })
        })
    }
}

pub(super) fn execute(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    command: Command,
) {
    match command {
        Command::Save { input, reply } => {
            let _ = reply.send(save(connection, spatial, &input));
        }
        Command::Change {
            id,
            expected,
            action,
            reply,
        } => {
            let _ = reply.send(change(connection, spatial, id, expected, &action));
        }
        Command::Detail { id, reply } => {
            let _ = reply.send(detail(connection, id));
        }
        Command::Page {
            bounds,
            filter,
            cursor,
            limit,
            reply,
        } => {
            let _ = reply.send(page(connection, spatial, bounds, &filter, cursor, limit));
        }
        Command::Nearby {
            bounds,
            origin,
            filter,
            limit,
            reply,
        } => {
            let _ = reply.send(nearby(connection, spatial, bounds, origin, &filter, limit));
        }
        Command::Next { reply } => {
            let _ = reply.send(next_operation(connection));
        }
        Command::Cache { entry, reply } => {
            let _ = reply.send(cache_directory_entry(connection, spatial, &entry));
        }
        Command::Acknowledge {
            id,
            applied,
            entry,
            reply,
        } => {
            let _ = reply.send(acknowledge_operation(
                connection, spatial, id, applied, &entry,
            ));
        }
        Command::Count { reply } => {
            let _ = reply.send(count(connection));
        }
    }
}

#[cfg(test)]
mod tests;
