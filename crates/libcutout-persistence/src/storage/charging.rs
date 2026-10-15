//! Charging commands executed by the existing database owner.
use super::{
    Command as DatabaseCommand, GeoBounds, MapPointCursor, MapPointId, QueryLimit, Reply,
    RideDatabase, SpatialSchemaState, StorageError,
};
use cutout_ride_maps::{ChargingVote, Coordinate, NewChargingSpot};
use rusqlite::Connection;

mod account;
mod codec;
mod model;
mod mutations;
mod queries;
mod records;
mod sync;

use account::AccountEvent;
pub use account::CommunityAccountState;
pub use model::{
    ChargingAction, ChargingBlockedOperation, ChargingChangeCursor, ChargingDirectoryEntry,
    ChargingFilter, ChargingOperation, ChargingOperationBlockKind, ChargingOperationId,
    ChargingOperationResolution, ChargingPage, ChargingPageApplyResult, ChargingRevision,
    ChargingStatus, ChargingTarget, ChargingVoteReceipt, ChargingWriteReceipt, StoredChargingSpot,
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
 blocked_kind TEXT CHECK(blocked_kind IS NULL OR blocked_kind IN ('conflict','invalid')),
 target TEXT CHECK(target IS NULL OR json_valid(target))
);
CREATE INDEX charging_outbox_point ON charging_outbox(point_id,sequence);
";

/// Maximum changes accepted in one durable directory transaction.
pub const CHARGING_SYNC_PAGE_LIMIT: usize = 100;

pub(super) enum Command {
    Account {
        event: AccountEvent,
        reply: Reply<CommunityAccountState>,
    },
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
    AcknowledgeVote {
        id: ChargingOperationId,
        receipt: ChargingVoteReceipt,
        reply: Reply<MapPointId>,
    },
    Count {
        reply: Reply<u32>,
    },
    ChangesCursor {
        reply: Reply<ChargingChangeCursor>,
    },
    ApplyChangesPage {
        entries: Vec<ChargingDirectoryEntry>,
        next_cursor: ChargingChangeCursor,
        reply: Reply<ChargingPageApplyResult>,
    },
    CacheVote {
        remote_id: cutout_ride_maps::ChargingSpotId,
        vote: Option<ChargingVote>,
        vote_revision: ChargingRevision,
        votes_up: u64,
        votes_down: u64,
        change_sequence: ChargingChangeCursor,
        reply: Reply<MapPointId>,
    },
    BlockOperation {
        id: ChargingOperationId,
        kind: ChargingOperationBlockKind,
        reply: Reply<()>,
    },
    ListBlocked {
        reply: Reply<Vec<ChargingBlockedOperation>>,
    },
    ResolveOperation {
        id: ChargingOperationId,
        expected_local_revision: ChargingRevision,
        resolution: ChargingOperationResolution,
        reply: Reply<()>,
    },
    ReplaceBlockedOperation {
        id: ChargingOperationId,
        expected_local_revision: ChargingRevision,
        input: NewChargingSpot,
        reply: Reply<ChargingWriteReceipt>,
    },
    DirectoryLastUpdated {
        reply: Reply<Option<u64>>,
    },
}
impl RideDatabase {
    /// Reads the durable community deletion gate without opening an account.
    /// # Errors
    /// Returns storage or malformed-state errors.
    pub fn community_account_state(&self) -> Result<CommunityAccountState, StorageError> {
        self.account_event(AccountEvent::Read)
    }
    /// Fences community writes before a request can leave the process.
    /// # Errors
    /// Returns storage failures; no HTTP operation should follow a failed fence.
    pub fn begin_community_account_deletion(&self) -> Result<CommunityAccountState, StorageError> {
        self.account_event(AccountEvent::BeginDeletion)
    }
    /// Settles confirmed service deletion and removes obsolete community work atomically.
    /// Local rides, captures and point geometry are preserved.
    /// # Errors
    /// Rejects completion without a durable request, invalid times, or storage errors.
    pub fn complete_community_account_deletion(
        &self,
        at_ms: u64,
    ) -> Result<CommunityAccountState, StorageError> {
        self.account_event(AccountEvent::CompleteDeletion { at_ms })
    }
    fn account_event(&self, event: AccountEvent) -> Result<CommunityAccountState, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::Account { event, reply }))
    }

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
    /// Returns the durable incremental directory-feed cursor.
    /// # Errors
    /// Returns worker/storage failures or a corrupt retained cursor.
    pub fn charging_changes_cursor(&self) -> Result<ChargingChangeCursor, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::ChangesCursor { reply }))
    }
    /// Returns when a successful directory refresh last committed, if one has completed.
    /// # Errors
    /// Returns worker/storage failures or a corrupt retained timestamp.
    pub fn charging_directory_last_updated_ms(&self) -> Result<Option<u64>, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::DirectoryLastUpdated { reply })
        })
    }
    /// Applies one bounded directory page and advances its cursor in the same transaction.
    /// # Errors
    /// Rejects malformed, oversized, regressing or corrupt pages without partial writes.
    pub fn apply_charging_changes_page(
        &self,
        entries: Vec<ChargingDirectoryEntry>,
        next_cursor: ChargingChangeCursor,
    ) -> Result<ChargingPageApplyResult, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::ApplyChangesPage {
                entries,
                next_cursor,
                reply,
            })
        })
    }
    /// Refreshes confirmed vote state without replacing a queued local vote.
    /// # Errors
    /// Rejects missing spots, stale change state or invalid counters.
    pub fn cache_charging_vote(
        &self,
        remote_id: cutout_ride_maps::ChargingSpotId,
        vote: Option<ChargingVote>,
        vote_revision: ChargingRevision,
        votes_up: u64,
        votes_down: u64,
        change_sequence: ChargingChangeCursor,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::CacheVote {
                remote_id,
                vote,
                vote_revision,
                votes_up,
                votes_down,
                change_sequence,
                reply,
            })
        })
    }
    /// Persists a classified refusal for an exact frozen operation.
    /// # Errors
    /// Rejects unknown operations or inconsistent retained state.
    pub fn block_charging_operation(
        &self,
        id: ChargingOperationId,
        kind: ChargingOperationBlockKind,
    ) -> Result<(), StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::BlockOperation { id, kind, reply })
        })
    }
    /// Lists blocked operations and both versions required for conflict review.
    /// # Errors
    /// Returns worker/storage errors or malformed retained conflict state.
    pub fn list_blocked_charging_operations(
        &self,
    ) -> Result<Vec<ChargingBlockedOperation>, StorageError> {
        self.request(move |reply| DatabaseCommand::Charging(Command::ListBlocked { reply }))
    }
    /// Resolves a blocked operation only if its reviewed local revision is current.
    /// # Errors
    /// Rejects stale review, missing conflict data and unsafe tombstone rebases.
    pub fn resolve_charging_operation(
        &self,
        id: ChargingOperationId,
        expected_local_revision: ChargingRevision,
        resolution: ChargingOperationResolution,
    ) -> Result<(), StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::ResolveOperation {
                id,
                expected_local_revision,
                resolution,
                reply,
            })
        })
    }
    /// Replaces a rejected local edit with user-corrected fields and a fresh operation identity.
    /// # Errors
    /// Rejects stale review, terminal remote state, invalid fields, or a full outbox.
    pub fn replace_blocked_charging_operation(
        &self,
        id: ChargingOperationId,
        expected_local_revision: ChargingRevision,
        input: NewChargingSpot,
    ) -> Result<ChargingWriteReceipt, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::ReplaceBlockedOperation {
                id,
                expected_local_revision,
                input,
                reply,
            })
        })
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
    /// Atomically acknowledges a vote receipt without replacing cached body or feed state.
    /// # Errors
    /// Rejects stale, mismatched, or non-head vote receipts without partial writes.
    pub fn acknowledge_charging_vote_operation(
        &self,
        id: ChargingOperationId,
        receipt: ChargingVoteReceipt,
    ) -> Result<MapPointId, StorageError> {
        self.request(move |reply| {
            DatabaseCommand::Charging(Command::AcknowledgeVote { id, receipt, reply })
        })
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the database actor keeps each command reply adjacent to its storage operation"
)]
pub(super) fn execute(
    connection: &mut Connection,
    spatial: &mut SpatialSchemaState,
    command: Command,
) {
    match command {
        Command::Account { event, reply } => {
            let _ = reply.send(account::apply(connection, event));
        }
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
        Command::AcknowledgeVote { id, receipt, reply } => {
            let _ = reply.send(sync::acknowledge_vote_operation(connection, id, receipt));
        }
        Command::Count { reply } => {
            let _ = reply.send(count(connection));
        }
        Command::ChangesCursor { reply } => {
            let _ = reply.send(sync::changes_cursor(connection));
        }
        Command::ApplyChangesPage {
            entries,
            next_cursor,
            reply,
        } => {
            let _ = reply.send(sync::apply_changes_page(
                connection,
                spatial,
                &entries,
                next_cursor,
            ));
        }
        Command::CacheVote {
            remote_id,
            vote,
            vote_revision,
            votes_up,
            votes_down,
            change_sequence,
            reply,
        } => {
            let _ = reply.send(sync::cache_vote(
                connection,
                remote_id,
                vote,
                vote_revision,
                votes_up,
                votes_down,
                change_sequence,
            ));
        }
        Command::BlockOperation { id, kind, reply } => {
            let _ = reply.send(sync::block_operation(connection, id, kind));
        }
        Command::ListBlocked { reply } => {
            let _ = reply.send(sync::list_blocked_operations(connection));
        }
        Command::ResolveOperation {
            id,
            expected_local_revision,
            resolution,
            reply,
        } => {
            let _ = reply.send(sync::resolve_operation(
                connection,
                spatial,
                id,
                expected_local_revision,
                resolution,
            ));
        }
        Command::ReplaceBlockedOperation {
            id,
            expected_local_revision,
            input,
            reply,
        } => {
            let _ = reply.send(sync::replace_blocked_operation(
                connection,
                spatial,
                id,
                expected_local_revision,
                &input,
            ));
        }
        Command::DirectoryLastUpdated { reply } => {
            let _ = reply.send(sync::directory_last_updated(connection));
        }
    }
}

#[cfg(test)]
mod tests;
