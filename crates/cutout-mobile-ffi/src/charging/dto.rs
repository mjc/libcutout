use crate::{MobileMapCoordinateDto, MobileMapPointCursorDto, persistence, ride_maps};

/// Typed cost value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingCostDto {
    /// `Free` value.
    Free,
    /// `Paid` value.
    Paid,
}
impl From<ride_maps::ChargingCost> for MobileChargingCostDto {
    fn from(value: ride_maps::ChargingCost) -> Self {
        match value {
            ride_maps::ChargingCost::Free => Self::Free,
            ride_maps::ChargingCost::Paid => Self::Paid,
        }
    }
}
impl From<MobileChargingCostDto> for ride_maps::ChargingCost {
    fn from(value: MobileChargingCostDto) -> Self {
        match value {
            MobileChargingCostDto::Free => Self::Free,
            MobileChargingCostDto::Paid => Self::Paid,
        }
    }
}
/// Typed venue value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingVenueDto {
    /// `Restaurant` value.
    Restaurant,
    /// `Cafe` value.
    Cafe,
    /// `Bar` value.
    Bar,
    /// `EvChargingStation` value.
    EvChargingStation,
    /// `Other` value.
    Other,
}
impl From<ride_maps::ChargingVenue> for MobileChargingVenueDto {
    fn from(value: ride_maps::ChargingVenue) -> Self {
        match value {
            ride_maps::ChargingVenue::Restaurant => Self::Restaurant,
            ride_maps::ChargingVenue::Cafe => Self::Cafe,
            ride_maps::ChargingVenue::Bar => Self::Bar,
            ride_maps::ChargingVenue::EvChargingStation => Self::EvChargingStation,
            ride_maps::ChargingVenue::Other => Self::Other,
        }
    }
}
impl From<MobileChargingVenueDto> for ride_maps::ChargingVenue {
    fn from(value: MobileChargingVenueDto) -> Self {
        match value {
            MobileChargingVenueDto::Restaurant => Self::Restaurant,
            MobileChargingVenueDto::Cafe => Self::Cafe,
            MobileChargingVenueDto::Bar => Self::Bar,
            MobileChargingVenueDto::EvChargingStation => Self::EvChargingStation,
            MobileChargingVenueDto::Other => Self::Other,
        }
    }
}
/// Typed terms value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingTermsDto {
    /// `DirectFee` value.
    DirectFee,
    /// `PurchaseRequired` value.
    PurchaseRequired,
    /// `Both` value.
    Both,
}
impl From<ride_maps::ChargingPaidTerms> for MobileChargingTermsDto {
    fn from(value: ride_maps::ChargingPaidTerms) -> Self {
        match value {
            ride_maps::ChargingPaidTerms::DirectFee => Self::DirectFee,
            ride_maps::ChargingPaidTerms::PurchaseRequired => Self::PurchaseRequired,
            ride_maps::ChargingPaidTerms::Both => Self::Both,
        }
    }
}
impl From<MobileChargingTermsDto> for ride_maps::ChargingPaidTerms {
    fn from(value: MobileChargingTermsDto) -> Self {
        match value {
            MobileChargingTermsDto::DirectFee => Self::DirectFee,
            MobileChargingTermsDto::PurchaseRequired => Self::PurchaseRequired,
            MobileChargingTermsDto::Both => Self::Both,
        }
    }
}
/// Typed pricebasis value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingPriceBasisDto {
    /// `PerCharge` value.
    PerCharge,
    /// `PerHour` value.
    PerHour,
    /// `PerKwh` value.
    PerKwh,
    /// `Purchase` value.
    Purchase,
}
impl From<ride_maps::ChargingPriceBasis> for MobileChargingPriceBasisDto {
    fn from(value: ride_maps::ChargingPriceBasis) -> Self {
        match value {
            ride_maps::ChargingPriceBasis::PerCharge => Self::PerCharge,
            ride_maps::ChargingPriceBasis::PerHour => Self::PerHour,
            ride_maps::ChargingPriceBasis::PerKwh => Self::PerKwh,
            ride_maps::ChargingPriceBasis::Purchase => Self::Purchase,
        }
    }
}
impl From<MobileChargingPriceBasisDto> for ride_maps::ChargingPriceBasis {
    fn from(value: MobileChargingPriceBasisDto) -> Self {
        match value {
            MobileChargingPriceBasisDto::PerCharge => Self::PerCharge,
            MobileChargingPriceBasisDto::PerHour => Self::PerHour,
            MobileChargingPriceBasisDto::PerKwh => Self::PerKwh,
            MobileChargingPriceBasisDto::Purchase => Self::Purchase,
        }
    }
}
/// Typed vote value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingVoteDto {
    /// `Up` value.
    Up,
    /// `Down` value.
    Down,
}
impl From<ride_maps::ChargingVote> for MobileChargingVoteDto {
    fn from(value: ride_maps::ChargingVote) -> Self {
        match value {
            ride_maps::ChargingVote::Up => Self::Up,
            ride_maps::ChargingVote::Down => Self::Down,
        }
    }
}
impl From<MobileChargingVoteDto> for ride_maps::ChargingVote {
    fn from(value: MobileChargingVoteDto) -> Self {
        match value {
            MobileChargingVoteDto::Up => Self::Up,
            MobileChargingVoteDto::Down => Self::Down,
        }
    }
}
/// Typed status value at the charging boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingStatusDto {
    /// `Queued` value.
    Queued,
    /// `Published` value.
    Published,
    /// `Hidden` value.
    Hidden,
    /// `Withdrawn` value.
    Withdrawn,
    /// `Removed` value.
    Removed,
}
impl From<persistence::ChargingStatus> for MobileChargingStatusDto {
    fn from(value: persistence::ChargingStatus) -> Self {
        match value {
            persistence::ChargingStatus::Queued => Self::Queued,
            persistence::ChargingStatus::Published => Self::Published,
            persistence::ChargingStatus::Hidden => Self::Hidden,
            persistence::ChargingStatus::Withdrawn => Self::Withdrawn,
            persistence::ChargingStatus::Removed => Self::Removed,
        }
    }
}
impl From<MobileChargingStatusDto> for persistence::ChargingStatus {
    fn from(value: MobileChargingStatusDto) -> Self {
        match value {
            MobileChargingStatusDto::Queued => Self::Queued,
            MobileChargingStatusDto::Published => Self::Published,
            MobileChargingStatusDto::Hidden => Self::Hidden,
            MobileChargingStatusDto::Withdrawn => Self::Withdrawn,
            MobileChargingStatusDto::Removed => Self::Removed,
        }
    }
}
/// Confirmed connector selection, including a bounded custom label.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingConnectorDto {
    /// `TypeA` connector.
    TypeA,
    /// `TypeB` connector.
    TypeB,
    /// `TypeC` connector.
    TypeC,
    /// `TypeD` connector.
    TypeD,
    /// `TypeE` connector.
    TypeE,
    /// `TypeF` connector.
    TypeF,
    /// `TypeG` connector.
    TypeG,
    /// `TypeH` connector.
    TypeH,
    /// `TypeI` connector.
    TypeI,
    /// `TypeJ` connector.
    TypeJ,
    /// `TypeK` connector.
    TypeK,
    /// `TypeL` connector.
    TypeL,
    /// `TypeM` connector.
    TypeM,
    /// `TypeN` connector.
    TypeN,
    /// `TypeO` connector.
    TypeO,
    /// `Nema5_15` connector.
    Nema5_15,
    /// `Nema5_20` connector.
    Nema5_20,
    /// `Nema14_50` connector.
    Nema14_50,
    /// `J1772` connector.
    J1772,
    /// `Type2` connector.
    Type2,
    /// `Ccs1` connector.
    Ccs1,
    /// `Ccs2` connector.
    Ccs2,
    /// `Chademo` connector.
    Chademo,
    /// `Nacs` connector.
    Nacs,
    /// `Unknown` connector.
    Unknown,
    /// Custom connector label.
    Other { label: String },
}
impl TryFrom<MobileChargingConnectorDto> for ride_maps::ChargingConnector {
    type Error = MobileChargingError;
    fn try_from(value: MobileChargingConnectorDto) -> Result<Self, Self::Error> {
        Ok(match value {
            MobileChargingConnectorDto::TypeA => Self::TypeA,
            MobileChargingConnectorDto::TypeB => Self::TypeB,
            MobileChargingConnectorDto::TypeC => Self::TypeC,
            MobileChargingConnectorDto::TypeD => Self::TypeD,
            MobileChargingConnectorDto::TypeE => Self::TypeE,
            MobileChargingConnectorDto::TypeF => Self::TypeF,
            MobileChargingConnectorDto::TypeG => Self::TypeG,
            MobileChargingConnectorDto::TypeH => Self::TypeH,
            MobileChargingConnectorDto::TypeI => Self::TypeI,
            MobileChargingConnectorDto::TypeJ => Self::TypeJ,
            MobileChargingConnectorDto::TypeK => Self::TypeK,
            MobileChargingConnectorDto::TypeL => Self::TypeL,
            MobileChargingConnectorDto::TypeM => Self::TypeM,
            MobileChargingConnectorDto::TypeN => Self::TypeN,
            MobileChargingConnectorDto::TypeO => Self::TypeO,
            MobileChargingConnectorDto::Nema5_15 => Self::Nema5_15,
            MobileChargingConnectorDto::Nema5_20 => Self::Nema5_20,
            MobileChargingConnectorDto::Nema14_50 => Self::Nema14_50,
            MobileChargingConnectorDto::J1772 => Self::J1772,
            MobileChargingConnectorDto::Type2 => Self::Type2,
            MobileChargingConnectorDto::Ccs1 => Self::Ccs1,
            MobileChargingConnectorDto::Ccs2 => Self::Ccs2,
            MobileChargingConnectorDto::Chademo => Self::Chademo,
            MobileChargingConnectorDto::Nacs => Self::Nacs,
            MobileChargingConnectorDto::Unknown => Self::Unknown,
            MobileChargingConnectorDto::Other { label } => Self::Other(
                ride_maps::ConnectorLabel::new(&label)
                    .map_err(|_| MobileChargingError::InvalidMetadata)?,
            ),
        })
    }
}
impl From<ride_maps::ChargingConnector> for MobileChargingConnectorDto {
    fn from(value: ride_maps::ChargingConnector) -> Self {
        match value {
            ride_maps::ChargingConnector::TypeA => Self::TypeA,
            ride_maps::ChargingConnector::TypeB => Self::TypeB,
            ride_maps::ChargingConnector::TypeC => Self::TypeC,
            ride_maps::ChargingConnector::TypeD => Self::TypeD,
            ride_maps::ChargingConnector::TypeE => Self::TypeE,
            ride_maps::ChargingConnector::TypeF => Self::TypeF,
            ride_maps::ChargingConnector::TypeG => Self::TypeG,
            ride_maps::ChargingConnector::TypeH => Self::TypeH,
            ride_maps::ChargingConnector::TypeI => Self::TypeI,
            ride_maps::ChargingConnector::TypeJ => Self::TypeJ,
            ride_maps::ChargingConnector::TypeK => Self::TypeK,
            ride_maps::ChargingConnector::TypeL => Self::TypeL,
            ride_maps::ChargingConnector::TypeM => Self::TypeM,
            ride_maps::ChargingConnector::TypeN => Self::TypeN,
            ride_maps::ChargingConnector::TypeO => Self::TypeO,
            ride_maps::ChargingConnector::Nema5_15 => Self::Nema5_15,
            ride_maps::ChargingConnector::Nema5_20 => Self::Nema5_20,
            ride_maps::ChargingConnector::Nema14_50 => Self::Nema14_50,
            ride_maps::ChargingConnector::J1772 => Self::J1772,
            ride_maps::ChargingConnector::Type2 => Self::Type2,
            ride_maps::ChargingConnector::Ccs1 => Self::Ccs1,
            ride_maps::ChargingConnector::Ccs2 => Self::Ccs2,
            ride_maps::ChargingConnector::Chademo => Self::Chademo,
            ride_maps::ChargingConnector::Nacs => Self::Nacs,
            ride_maps::ChargingConnector::Unknown => Self::Unknown,
            ride_maps::ChargingConnector::Other(label) => Self::Other {
                label: label.as_str().to_owned(),
            },
        }
    }
}

/// Exact optional amount; no floating-point currency conversion.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingPriceDto {
    pub amount_minor: u64,
    pub currency: String,
    pub minor_unit_exponent: u8,
    pub basis: MobileChargingPriceBasisDto,
}
/// Confirmed voltage and connectors from the same supply group.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingSupplyDto {
    pub voltage_volts: Option<u16>,
    pub connectors: Vec<MobileChargingConnectorDto>,
}
/// Public charging fields; only pin, access note and cost are required.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingInputDto {
    pub coordinate: MobileMapCoordinateDto,
    pub access_note: String,
    pub cost: MobileChargingCostDto,
    pub name: Option<String>,
    pub venue: Option<MobileChargingVenueDto>,
    pub hours: Option<String>,
    pub terms: Option<MobileChargingTermsDto>,
    pub price: Option<MobileChargingPriceDto>,
    pub supplies: Vec<MobileChargingSupplyDto>,
}
/// Detail read from the existing Rust database owner.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingSpotDto {
    pub id: String,
    pub input: Option<MobileChargingInputDto>,
    pub owned: bool,
    pub status: MobileChargingStatusDto,
    pub local_revision: u64,
    pub remote_id: Option<String>,
    pub remote_revision: u64,
    pub change_sequence: u64,
    pub verified_at_ms: Option<u64>,
    pub vote: Option<MobileChargingVoteDto>,
    pub vote_revision: u64,
    pub votes_up: u64,
    pub votes_down: u64,
    pub pending_operations: u32,
    pub pending_vote: bool,
}
/// Durable local receipt; publication requires a separate server receipt.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingWriteReceiptDto {
    pub id: String,
    pub local_revision: u64,
    pub operation_id: String,
}

/// One-time public identity credential; store it in Keychain before contributing.
#[allow(
    missing_debug_implementations,
    reason = "The bearer token must not appear in debug output"
)]
#[derive(Clone, uniffi::Record)]
pub struct MobileChargingIdentityDto {
    pub identity_id: String,
    pub token: String,
}

/// Terminal result of synchronization.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncResult {
    /// The directory and local outbox are both caught up.
    Synced,
    /// The bounded run finished with work still pending.
    Pending,
    /// A local operation was blocked by a concurrent server revision.
    Conflict,
    /// A local operation was rejected by server validation.
    Rejected,
    /// The caller is not authorized to sync with the stored identity.
    AuthenticationRequired,
    /// The request budget is temporarily exhausted.
    Throttled,
    /// The directory could not be reached.
    Offline,
    /// Local storage could not complete the pass.
    StorageUnavailable,
}

/// Result of one bounded synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingSyncOutcome {
    /// Directory pages or runnable outbox work remain for another round.
    MoreWork,
    /// The pass is finished with its terminal result.
    Finished { result: MobileChargingSyncResult },
}

/// Result and committed progress of one bounded synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingSyncSummaryDto {
    pub pending_count: u32,
    pub downloaded: u32,
    pub uploaded: u32,
    pub outcome: MobileChargingSyncOutcome,
}

/// Why a durable charging operation is waiting for explicit resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingBlockReasonDto {
    /// The server revision or lifecycle changed before this operation could apply.
    Conflict,
    /// The server rejected the operation's public input.
    Invalid,
}

/// Frozen local operation that was rejected or conflicts with newer server state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingPendingActionDto {
    /// A new charging spot submission.
    Create,
    /// An edit to the submitted charging spot.
    Edit,
    /// Withdrawal of the submitted charging spot.
    Withdraw,
    /// Verification of the submitted charging spot.
    Verify,
    /// Set or clear the contributor's vote.
    Vote {
        /// `None` means the pending operation explicitly clears the vote.
        selection: Option<MobileChargingVoteDto>,
    },
}

/// Resolution selected for a blocked local charging operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileChargingResolutionDto {
    /// Keep its frozen server target and retry the same idempotent request.
    Retry,
    /// Keep the local draft and prepare it against the latest server revision.
    RebaseKeepingLocal,
    /// Accept the retained server state and discard the blocked local operation.
    AcceptServer,
}

/// A local contribution and the current server state that needs a rider decision.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingSyncConflictDto {
    pub operation_id: String,
    pub action: MobileChargingPendingActionDto,
    pub id: String,
    pub local_revision: u64,
    pub input: Option<MobileChargingInputDto>,
    pub remote_input: Option<MobileChargingInputDto>,
    pub remote_status: Option<MobileChargingStatusDto>,
    pub reason: MobileChargingBlockReasonDto,
    pub can_rebase: bool,
}
/// Discovery filters applied in Rust before pagination/ranking.
#[derive(Clone, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingFilterDto {
    pub cost: Option<MobileChargingCostDto>,
    pub connector: Option<MobileChargingConnectorDto>,
    pub voltage_volts: Option<u16>,
    pub venue: Option<MobileChargingVenueDto>,
}
/// Bounded discovery page using the existing map-point cursor.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MobileChargingPageDto {
    pub spots: Vec<MobileChargingSpotDto>,
    pub next_cursor: Option<MobileMapPointCursorDto>,
}
/// Stable error categories for field validation, conflicts and durable storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileChargingError {
    #[error("Invalid map pin")]
    InvalidPin,
    #[error("Access note is blank")]
    AccessNoteBlank,
    #[error("Access note is too long")]
    AccessNoteTooLong,
    #[error("Access note contains unsupported characters")]
    AccessNoteControlCharacter,
    #[error("Invalid charging details")]
    InvalidMetadata,
    #[error("Invalid identifier")]
    InvalidIdentifier,
    #[error("Invalid page limit")]
    InvalidLimit,
    #[error("Spot changed")]
    Conflict,
    #[error("Spot belongs to another contributor")]
    NotOwned,
    #[error("Spot not found")]
    NotFound,
    #[error("Submission queue is full")]
    OutboxFull,
    #[error("Storage is unavailable")]
    Unavailable,
    #[error("Could not save or read the spot")]
    StorageFailure,
    #[error("A contributor identity is required")]
    Unauthorized,
    #[error("The directory rejected this contribution")]
    Rejected,
    #[error("The directory request budget is temporarily exhausted")]
    Throttled,
    #[error("The charging directory could not be reached")]
    Network,
}
