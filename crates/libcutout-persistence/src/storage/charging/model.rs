use super::super::{MapPointCursor, MapPointId, StorageError};
use cutout_ride_maps::{
    ChargingConnector, ChargingCost, ChargingSpotId, ChargingVote, NewChargingSpot,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A checked revision, including zero for a contributor who has never voted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct ChargingRevision(u64);
impl ChargingRevision {
    pub(super) const ZERO: Self = Self(0);
    pub(super) const INITIAL: Self = Self(1);
    /// Checks that SQLite can retain the revision exactly.
    /// # Errors
    /// Rejects values exceeding SQLite's signed integer range.
    pub fn new(value: u64) -> Result<Self, StorageError> {
        i64::try_from(value).map_err(|_| StorageError::ChargingInvalidInput)?;
        Ok(Self(value))
    }
    /// Returns the exact revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
    pub(super) fn next(self) -> Result<Self, StorageError> {
        Self::new(
            self.0
                .checked_add(1)
                .ok_or(StorageError::ChargingConflict)?,
        )
    }
}
impl TryFrom<u64> for ChargingRevision {
    type Error = StorageError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<ChargingRevision> for u64 {
    fn from(value: ChargingRevision) -> Self {
        value.0
    }
}

/// An immutable identity used for server idempotency across retries and reopen.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ChargingOperationId(Uuid);
impl ChargingOperationId {
    /// Restores an existing operation identity.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    /// Returns the operation UUID.
    #[must_use]
    pub const fn uuid(self) -> Uuid {
        self.0
    }
}

/// Local queue state or an acknowledged server state; legacy points have no charging row.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingStatus {
    /// A new spot awaits server acceptance.
    Queued,
    /// The server accepted the spot for discovery.
    Published,
    /// Moderation hid the spot.
    Hidden,
    /// The contributor withdrew the spot.
    Withdrawn,
    /// Moderation removed the spot.
    Removed,
}
impl ChargingStatus {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Published => "published",
            Self::Hidden => "hidden",
            Self::Withdrawn => "withdrawn",
            Self::Removed => "removed",
        }
    }
    pub(super) fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "queued" => Ok(Self::Queued),
            "published" => Ok(Self::Published),
            "hidden" => Ok(Self::Hidden),
            "withdrawn" => Ok(Self::Withdrawn),
            "removed" => Ok(Self::Removed),
            _ => Err(StorageError::ChargingInvalidInput),
        }
    }
    pub(super) const fn visible(self) -> bool {
        match self {
            Self::Queued | Self::Published => true,
            Self::Hidden | Self::Withdrawn | Self::Removed => false,
        }
    }
}

/// Durable local detail, using the existing map-point identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredChargingSpot {
    /// Existing local map-point UUID.
    pub point_id: MapPointId,
    /// Optional public fields; tombstones have no retained public body.
    pub input: Option<NewChargingSpot>,
    /// True only for spots created by this installation.
    pub owned: bool,
    /// Durable local state, independent of unacknowledged requests.
    pub status: ChargingStatus,
    /// Optimistic local write revision.
    pub local_revision: ChargingRevision,
    /// Server identity, assigned on publication.
    pub remote_id: Option<ChargingSpotId>,
    /// Last acknowledged server spot revision.
    pub remote_revision: ChargingRevision,
    /// Directory change sequence, independent of spot revision.
    pub change_sequence: ChargingRevision,
    /// Server-confirmed access verification time.
    pub verified_at_ms: Option<u64>,
    /// Requested contributor vote; pending requests do not alter counts.
    pub vote: Option<ChargingVote>,
    /// Server-confirmed contributor vote revision.
    pub vote_revision: ChargingRevision,
    /// Last server-confirmed positive count.
    pub votes_up: u64,
    /// Last server-confirmed negative count.
    pub votes_down: u64,
}

/// Result returned only after local data and its outbox operation commit together.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChargingWriteReceipt {
    /// Retained map-point identity.
    pub point_id: MapPointId,
    /// Revision committed by this command.
    pub revision: ChargingRevision,
    /// Durable community operation identity.
    pub operation_id: ChargingOperationId,
}

/// Validated public directory data received by the future sync client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingDirectoryEntry {
    /// Server-assigned identity; distinct from an existing local identity.
    pub id: ChargingSpotId,
    /// Current public body; present only for published spots.
    pub input: Option<NewChargingSpot>,
    /// Server spot revision.
    pub revision: ChargingRevision,
    /// Server change sequence.
    pub change_sequence: ChargingRevision,
    /// Server status; Queued is never a server state.
    pub status: ChargingStatus,
    /// Confirmed access verification timestamp.
    pub verified_at_ms: Option<u64>,
    /// Contributor vote returned by an authenticated read or receipt.
    pub vote: Option<ChargingVote>,
    /// Contributor vote revision.
    pub vote_revision: ChargingRevision,
    /// Aggregate positive votes.
    pub votes_up: u64,
    /// Aggregate negative votes.
    pub votes_down: u64,
}
impl ChargingDirectoryEntry {
    pub(super) fn validate(&self) -> Result<(), StorageError> {
        if self.status == ChargingStatus::Queued
            || self.revision.get() == 0
            || self.change_sequence.get() == 0
            || (self.status == ChargingStatus::Published) != self.input.is_some()
        {
            return Err(StorageError::ChargingInvalidInput);
        }
        for value in [self.votes_up, self.votes_down]
            .into_iter()
            .chain(self.verified_at_ms)
        {
            ChargingRevision::new(value)?;
        }
        Ok(())
    }
}

/// Frozen endpoint target and revision for an idempotent server retry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChargingTarget {
    /// The server assigns an identity on create.
    Create,
    /// Subsequent commands use the server identity and a frozen expected revision.
    Existing {
        /// Server identity.
        id: ChargingSpotId,
        /// Spot revision or, for votes, contributor vote revision.
        expected_revision: ChargingRevision,
    },
}

/// Public action retained in the outbox, without ride or credential data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChargingAction {
    /// Create a spot with validated fields.
    Create(NewChargingSpot),
    /// Replace public fields and move the existing pin.
    Edit(NewChargingSpot),
    /// Withdraw the contributor's spot.
    Withdraw,
    /// Confirm access without changing power facts.
    Verify,
    /// Set or clear a contributor vote; never increment aggregate counts locally.
    Vote(Option<ChargingVote>),
}

/// One retryable command; obtaining it freezes the target in SQLite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingOperation {
    /// Stable idempotency key.
    pub id: ChargingOperationId,
    /// Local point identity.
    pub point_id: MapPointId,
    /// Committed local revision.
    pub local_revision: ChargingRevision,
    /// Frozen server target.
    pub target: ChargingTarget,
    /// Validated public action.
    pub action: ChargingAction,
}

/// Optional discovery filters applied before pagination or distance ranking.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChargingFilter {
    /// Explicit access category, when selected.
    pub cost: Option<ChargingCost>,
    /// Confirmed connector only; an absent supply does not match.
    pub connector: Option<ChargingConnector>,
    /// Confirmed supply voltage only.
    pub voltage: Option<cutout_ride_maps::SupplyVoltage>,
}

/// Bounded page using the existing stable UUID cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargingPage {
    /// Matching detail rows.
    pub spots: Vec<StoredChargingSpot>,
    /// Cursor returned only when another matching row exists.
    pub next_cursor: Option<MapPointCursor>,
}

impl ChargingAction {
    pub(super) const fn is_vote(&self) -> bool {
        match self {
            Self::Vote(_) => true,
            Self::Create(_) | Self::Edit(_) | Self::Withdraw | Self::Verify => false,
        }
    }
}

/// Server data whose status, body and SQLite integer bounds have been checked.
pub(super) struct ValidatedDirectoryEntry<'a>(&'a ChargingDirectoryEntry);

impl<'a> ValidatedDirectoryEntry<'a> {
    pub(super) fn new(entry: &'a ChargingDirectoryEntry) -> Result<Self, StorageError> {
        entry.validate()?;
        Ok(Self(entry))
    }

    pub(super) fn as_entry(&self) -> &'a ChargingDirectoryEntry {
        self.0
    }
}
