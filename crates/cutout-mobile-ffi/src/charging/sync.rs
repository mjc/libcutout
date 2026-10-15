use super::sync_policy::outcome_for_error;
use super::{
    MobileChargingBlockReasonDto, MobileChargingError, MobileChargingIdentityDto,
    MobileChargingInputDto, MobileChargingPendingActionDto, MobileChargingResolutionDto,
    MobileChargingSyncConflictDto, MobileChargingSyncOutcome, MobileChargingSyncSummaryDto,
    MobileChargingWriteReceiptDto,
    conversion::{input, input_dto, receipt_dto, storage_error},
};
use crate::{RideDatabaseHandle, persistence, ride_maps};
use reqwest::{Client, Method, StatusCode, Url, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(test)]
use std::net::IpAddr;
use std::time::Duration;
use uuid::Uuid;

const DIRECTORY_URL: &str = "https://rides.cutout.lol/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const SYNC_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REQUEST_BODY_BYTES: usize = 32 * 1024;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const CHANGE_PAGE_SIZE: u32 = 10;
const MAX_UPLOADS_PER_SYNC: u32 = 8;

struct ChargingHttpClient {
    client: Client,
    base_url: Url,
}

impl ChargingHttpClient {
    fn production() -> Result<Self, MobileChargingError> {
        Self::build(DIRECTORY_URL)
    }

    fn build(base_url: &str) -> Result<Self, MobileChargingError> {
        let base_url = Url::parse(base_url).map_err(|_| MobileChargingError::Network)?;
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| MobileChargingError::Network)?;
        Ok(Self { client, base_url })
    }

    #[cfg(test)]
    fn loopback(base_url: &str) -> Result<Self, MobileChargingError> {
        let url = Url::parse(base_url).map_err(|_| MobileChargingError::Network)?;
        let loopback = url.scheme() == "http"
            && url
                .host_str()
                .and_then(|host| {
                    host.trim_start_matches('[')
                        .trim_end_matches(']')
                        .parse::<IpAddr>()
                        .ok()
                })
                .is_some_and(|address| address.is_loopback());
        if !loopback {
            return Err(MobileChargingError::Network);
        }
        Self::build(base_url)
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<T, MobileChargingError> {
        let url = self
            .base_url
            .join(path)
            .map_err(|_| MobileChargingError::Network)?;
        let mut request = self.client.request(method, url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            if body.len() > MAX_REQUEST_BODY_BYTES {
                return Err(MobileChargingError::Network);
            }
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| MobileChargingError::Network)?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status));
        }
        let mut bytes = Vec::new();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(MobileChargingError::Network);
        }
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| MobileChargingError::Network)?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(MobileChargingError::Network);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| MobileChargingError::Network)
    }
}

fn status_error(status: StatusCode) -> MobileChargingError {
    match status.as_u16() {
        401 => MobileChargingError::Unauthorized,
        409 => MobileChargingError::Conflict,
        422 | 403 | 404 => MobileChargingError::Rejected,
        429 => MobileChargingError::Throttled,
        _ => MobileChargingError::Network,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityResponse {
    id: String,
    token: String,
}

#[derive(Deserialize)]
struct CountsResponse {
    up: u64,
    down: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ServerStatus {
    Published,
    Hidden,
    Withdrawn,
    Removed,
}

#[derive(Deserialize)]
struct SpotInputResponse {
    coordinate: CoordinateResponse,
    access_note: String,
    cost: ride_maps::ChargingCost,
    name: Option<String>,
    venue: Option<ride_maps::ChargingVenue>,
    hours: Option<String>,
    paid_terms: Option<ride_maps::ChargingPaidTerms>,
    price: Option<ride_maps::ChargingPrice>,
    #[serde(default)]
    supplies: Vec<ride_maps::ChargingSupply>,
}

#[derive(Deserialize)]
struct CoordinateResponse {
    latitude_e7: i32,
    longitude_e7: i32,
}

#[derive(Deserialize)]
struct PublicSpotResponse {
    id: String,
    revision: u64,
    created_at_ms: u64,
    updated_at_ms: u64,
    change_sequence: u64,
    last_verified_at_ms: Option<u64>,
    #[serde(flatten)]
    input: SpotInputResponse,
    votes: CountsResponse,
}

#[derive(Deserialize)]
struct SpotStateResponse {
    id: String,
    revision: u64,
    status: ServerStatus,
    change_sequence: u64,
    spot: Option<PublicSpotResponse>,
    duplicate_of: Option<String>,
}

#[derive(Deserialize)]
struct ChangesPageResponse {
    items: Vec<SpotStateResponse>,
    next_cursor: u64,
    has_more: bool,
}

#[derive(Deserialize)]
struct MutationReceiptResponse {
    operation_id: String,
    applied_revision: u64,
    state: SpotStateResponse,
}

#[derive(Deserialize)]
struct VoteStateResponse {
    revision: u64,
    vote: Option<ride_maps::ChargingVote>,
    votes: CountsResponse,
    spot_status: ServerStatus,
    change_sequence: u64,
}

#[derive(Deserialize)]
struct VoteReceiptResponse {
    operation_id: String,
    applied_revision: u64,
    revision: u64,
    vote: Option<ride_maps::ChargingVote>,
    votes: CountsResponse,
    #[allow(
        dead_code,
        reason = "vote receipts have no body; persistence preserves local spot status"
    )]
    #[serde(rename = "spot_status")]
    _spot_status: ServerStatus,
    change_sequence: u64,
}

#[derive(Serialize)]
struct RevisionRequest {
    operation_id: Uuid,
    expected_revision: u64,
}

#[derive(Serialize)]
struct VoteRequest {
    operation_id: Uuid,
    expected_revision: u64,
    vote: Option<ride_maps::ChargingVote>,
}

fn json_body<T: Serialize>(value: &T) -> Result<Vec<u8>, MobileChargingError> {
    serde_json::to_vec(value).map_err(|_| MobileChargingError::StorageFailure)
}

fn server_status(value: ServerStatus) -> persistence::ChargingStatus {
    match value {
        ServerStatus::Published => persistence::ChargingStatus::Published,
        ServerStatus::Hidden => persistence::ChargingStatus::Hidden,
        ServerStatus::Withdrawn => persistence::ChargingStatus::Withdrawn,
        ServerStatus::Removed => persistence::ChargingStatus::Removed,
    }
}

fn server_input(
    value: SpotInputResponse,
) -> Result<ride_maps::NewChargingSpot, MobileChargingError> {
    let coordinate = ride_maps::Coordinate::from_fixed_parts(
        value.coordinate.latitude_e7,
        value.coordinate.longitude_e7,
    )
    .map_err(|_| MobileChargingError::Network)?;
    let details = ride_maps::ChargingDetails::new(
        value.name.as_deref(),
        value.venue,
        value.hours.as_deref(),
        value.paid_terms,
        value.price,
        value.supplies,
    )
    .map_err(|_| MobileChargingError::Network)?;
    let input = ride_maps::NewChargingSpot::new(coordinate, &value.access_note, value.cost)
        .map_err(|_| MobileChargingError::Network)?;
    input
        .with_details(details)
        .map_err(|_| MobileChargingError::Network)
}

fn public_spot(
    value: PublicSpotResponse,
) -> Result<
    (
        ride_maps::ChargingSpotId,
        persistence::ChargingDirectoryEntry,
    ),
    MobileChargingError,
> {
    let id =
        ride_maps::ChargingSpotId::parse(&value.id).map_err(|_| MobileChargingError::Network)?;
    if value.created_at_ms > value.updated_at_ms {
        return Err(MobileChargingError::Network);
    }
    let entry = persistence::ChargingDirectoryEntry {
        id,
        input: Some(server_input(value.input)?),
        revision: persistence::ChargingRevision::new(value.revision)
            .map_err(|_| MobileChargingError::Network)?,
        change_sequence: persistence::ChargingRevision::new(value.change_sequence)
            .map_err(|_| MobileChargingError::Network)?,
        status: persistence::ChargingStatus::Published,
        verified_at_ms: value.last_verified_at_ms,
        vote: None,
        vote_revision: persistence::ChargingRevision::new(0)
            .map_err(|_| MobileChargingError::Network)?,
        votes_up: value.votes.up,
        votes_down: value.votes.down,
    };
    Ok((id, entry))
}

fn state_entry(
    value: SpotStateResponse,
) -> Result<persistence::ChargingDirectoryEntry, MobileChargingError> {
    let id =
        ride_maps::ChargingSpotId::parse(&value.id).map_err(|_| MobileChargingError::Network)?;
    let status = server_status(value.status);
    let (input, verified_at_ms, votes_up, votes_down) = if let Some(spot) = value.spot {
        if spot.id != value.id
            || spot.revision != value.revision
            || spot.change_sequence != value.change_sequence
            || status != persistence::ChargingStatus::Published
        {
            return Err(MobileChargingError::Network);
        }
        let (_, entry) = public_spot(spot)?;
        (
            entry.input,
            entry.verified_at_ms,
            entry.votes_up,
            entry.votes_down,
        )
    } else {
        let valid_duplicate = value.duplicate_of.as_deref().is_some_and(|duplicate| {
            ride_maps::ChargingSpotId::parse(duplicate).is_ok()
                && status == persistence::ChargingStatus::Removed
        });
        if status == persistence::ChargingStatus::Published
            || (value.duplicate_of.is_some() && !valid_duplicate)
        {
            return Err(MobileChargingError::Network);
        }
        (None, None, 0, 0)
    };
    Ok(persistence::ChargingDirectoryEntry {
        id,
        input,
        revision: persistence::ChargingRevision::new(value.revision)
            .map_err(|_| MobileChargingError::Network)?,
        change_sequence: persistence::ChargingRevision::new(value.change_sequence)
            .map_err(|_| MobileChargingError::Network)?,
        status,
        verified_at_ms,
        vote: None,
        vote_revision: persistence::ChargingRevision::new(0)
            .map_err(|_| MobileChargingError::Network)?,
        votes_up,
        votes_down,
    })
}

fn pending_action(action: &persistence::ChargingAction) -> MobileChargingPendingActionDto {
    match action {
        persistence::ChargingAction::Create(_) => MobileChargingPendingActionDto::Create,
        persistence::ChargingAction::Edit(_) => MobileChargingPendingActionDto::Edit,
        persistence::ChargingAction::Withdraw => MobileChargingPendingActionDto::Withdraw,
        persistence::ChargingAction::Verify => MobileChargingPendingActionDto::Verify,
        persistence::ChargingAction::Vote(selection) => MobileChargingPendingActionDto::Vote {
            selection: selection.map(Into::into),
        },
    }
}

fn blocked_operation_dto(
    value: &persistence::ChargingBlockedOperation,
) -> MobileChargingSyncConflictDto {
    let reason = match value.kind {
        persistence::ChargingOperationBlockKind::Conflict => MobileChargingBlockReasonDto::Conflict,
        persistence::ChargingOperationBlockKind::Invalid => MobileChargingBlockReasonDto::Invalid,
    };
    let remote_status = value.remote_entry.as_ref().map(|entry| entry.status.into());
    let remote_input = value
        .remote_entry
        .as_ref()
        .and_then(|entry| entry.input.as_ref())
        .map(input_dto);
    MobileChargingSyncConflictDto {
        operation_id: value.operation.id.uuid().to_string(),
        action: pending_action(&value.operation.action),
        id: value.operation.point_id.uuid().to_string(),
        local_revision: value.local_revision.get(),
        input: value.local_draft.as_ref().map(input_dto),
        remote_input,
        remote_status,
        reason,
        can_rebase: value.can_rebase,
    }
}

async fn database<T, F>(
    database: persistence::RideDatabase,
    action: F,
) -> Result<T, MobileChargingError>
where
    T: Send + 'static,
    F: FnOnce(persistence::RideDatabase) -> Result<T, persistence::StorageError> + Send + 'static,
{
    let result = tokio::task::spawn_blocking(move || action(database))
        .await
        .map_err(|_| MobileChargingError::StorageFailure)?;
    result.map_err(storage_error)
}

async fn register(
    client: &ChargingHttpClient,
) -> Result<MobileChargingIdentityDto, MobileChargingError> {
    let response: IdentityResponse = client
        .request(Method::POST, "v1/identities", None, Some(Vec::new()))
        .await?;
    let id = Uuid::parse_str(&response.id).map_err(|_| MobileChargingError::Network)?;
    if response.token.len() != 64
        || !response
            .token
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err(MobileChargingError::Network);
    }
    Ok(MobileChargingIdentityDto {
        identity_id: id.to_string(),
        token: response.token,
    })
}

/// Registers one contributor identity. Persist the returned token securely before contribution.
///
/// # Errors
/// Returns a typed network or server error when registration fails.
#[uniffi::export(async_runtime = "tokio")]
pub async fn mobile_register_charging_identity()
-> Result<MobileChargingIdentityDto, MobileChargingError> {
    register(&ChargingHttpClient::production()?).await
}

async fn download_page(
    client: &ChargingHttpClient,
    database_handle: &persistence::RideDatabase,
    token: Option<&str>,
) -> Result<(u32, bool), MobileChargingError> {
    let cursor = database(database_handle.clone(), |database| {
        database.charging_changes_cursor()
    })
    .await?;
    let path = format!(
        "v1/charging-spots/changes?after={}&limit={CHANGE_PAGE_SIZE}",
        cursor.get()
    );
    let page: ChangesPageResponse = client.request(Method::GET, &path, token, None).await?;
    let next_cursor = persistence::ChargingChangeCursor::new(page.next_cursor)
        .map_err(|_| MobileChargingError::Network)?;
    if next_cursor < cursor || page.items.len() > CHANGE_PAGE_SIZE as usize {
        return Err(MobileChargingError::Network);
    }
    let has_more = page.has_more;
    let mut entries = Vec::with_capacity(page.items.len());
    for item in page.items {
        let mut entry = state_entry(item)?;
        if entry.status == persistence::ChargingStatus::Published
            && let Some(token) = token
        {
            let path = format!("v1/charging-spots/{}/vote", entry.id.as_uuid());
            let vote: VoteStateResponse = client
                .request(Method::GET, &path, Some(token), None)
                .await?;
            if server_status(vote.spot_status) != persistence::ChargingStatus::Published
                || vote.change_sequence != entry.change_sequence.get()
            {
                return Err(MobileChargingError::Network);
            }
            entry.vote = vote.vote;
            entry.vote_revision = persistence::ChargingRevision::new(vote.revision)
                .map_err(|_| MobileChargingError::Network)?;
            entry.votes_up = vote.votes.up;
            entry.votes_down = vote.votes.down;
        }
        entries.push(entry);
    }
    if token.is_none() {
        let pending = database(database_handle.clone(), |database| {
            database.charging_outbox_count()
        })
        .await?;
        if pending > 0 {
            return Ok((0, true));
        }
    }
    let result = database(database_handle.clone(), move |database| {
        database
            .apply_charging_changes_page(entries, next_cursor)
            .map(|result| result.applied_entries)
    })
    .await?;
    Ok((result, has_more))
}

#[allow(
    clippy::too_many_lines,
    reason = "keeps one frozen operation upload and receipt validation atomic"
)]
async fn upload_operation(
    client: &ChargingHttpClient,
    database_handle: &persistence::RideDatabase,
    token: &str,
    operation: persistence::ChargingOperation,
) -> Result<(), MobileChargingError> {
    use persistence::{ChargingAction, ChargingTarget};
    let operation_id = operation.id.uuid();
    let body_and_path = match (&operation.action, &operation.target) {
        (ChargingAction::Create(input), ChargingTarget::Create) => {
            let body = json_body(&serde_json::json!({
                "operation_id": operation_id,
                "spot": input.community_submission(),
            }))?;
            (Method::POST, "v1/charging-spots".to_owned(), body)
        }
        (
            ChargingAction::Edit(input),
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => {
            let body = json_body(&serde_json::json!({
                "operation_id": operation_id,
                "expected_revision": expected_revision.get(),
                "spot": input.community_submission(),
            }))?;
            (
                Method::PUT,
                format!("v1/charging-spots/{}", id.as_uuid()),
                body,
            )
        }
        (
            ChargingAction::Withdraw,
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => {
            let body = json_body(&RevisionRequest {
                operation_id,
                expected_revision: expected_revision.get(),
            })?;
            (
                Method::DELETE,
                format!("v1/charging-spots/{}", id.as_uuid()),
                body,
            )
        }
        (
            ChargingAction::Verify,
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => {
            let body = json_body(&RevisionRequest {
                operation_id,
                expected_revision: expected_revision.get(),
            })?;
            (
                Method::POST,
                format!("v1/charging-spots/{}/verification", id.as_uuid()),
                body,
            )
        }
        (
            ChargingAction::Vote(vote),
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => {
            let body = json_body(&VoteRequest {
                operation_id,
                expected_revision: expected_revision.get(),
                vote: *vote,
            })?;
            (
                Method::PUT,
                format!("v1/charging-spots/{}/vote", id.as_uuid()),
                body,
            )
        }
        _ => return Err(MobileChargingError::StorageFailure),
    };

    let is_vote = match &operation.action {
        ChargingAction::Vote(_) => true,
        ChargingAction::Create(_)
        | ChargingAction::Edit(_)
        | ChargingAction::Withdraw
        | ChargingAction::Verify => false,
    };
    let point_id = operation.point_id;
    if is_vote {
        let receipt: VoteReceiptResponse = client
            .request(
                body_and_path.0,
                &body_and_path.1,
                Some(token),
                Some(body_and_path.2),
            )
            .await?;
        if receipt.operation_id != operation_id.to_string() {
            return Err(MobileChargingError::Network);
        }
        if receipt.revision < receipt.applied_revision {
            return Err(MobileChargingError::Network);
        }
        database(database_handle.clone(), move |database| {
            database
                .acknowledge_charging_vote_operation(
                    persistence::ChargingOperationId::from_uuid(operation_id),
                    persistence::ChargingVoteReceipt {
                        applied_revision: persistence::ChargingRevision::new(
                            receipt.applied_revision,
                        )?,
                        vote_revision: persistence::ChargingRevision::new(receipt.revision)?,
                        vote: receipt.vote,
                        votes_up: receipt.votes.up,
                        votes_down: receipt.votes.down,
                        change_sequence: persistence::ChargingChangeCursor::new(
                            receipt.change_sequence,
                        )?,
                    },
                )
                .map(|_| ())
        })
        .await?;
    } else {
        let receipt: MutationReceiptResponse = client
            .request(
                body_and_path.0,
                &body_and_path.1,
                Some(token),
                Some(body_and_path.2),
            )
            .await?;
        if receipt.operation_id != operation_id.to_string() {
            return Err(MobileChargingError::Network);
        }
        let current_revision = receipt.state.revision;
        let mut entry = state_entry(receipt.state)?;
        if current_revision < receipt.applied_revision {
            return Err(MobileChargingError::Network);
        }
        let current = database(database_handle.clone(), move |database| {
            database.charging_spot(point_id)
        })
        .await?
        .ok_or(MobileChargingError::StorageFailure)?;
        // Public mutation receipts do not contain the contributor's vote.
        entry.vote = current.vote;
        entry.vote_revision = current.vote_revision;
        database(database_handle.clone(), move |database| {
            database
                .acknowledge_charging_operation(
                    persistence::ChargingOperationId::from_uuid(operation_id),
                    persistence::ChargingRevision::new(receipt.applied_revision)?,
                    entry,
                )
                .map(|_| ())
        })
        .await?;
    }
    Ok(())
}

#[cfg(test)]
async fn synchronize(
    client: &ChargingHttpClient,
    database_handle: persistence::RideDatabase,
    token: String,
) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
    synchronize_once(client, database_handle, token).await
}

#[cfg(test)]
async fn synchronize_once(
    client: &ChargingHttpClient,
    database_handle: persistence::RideDatabase,
    token: String,
) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
    synchronize_once_with_timeout(client, database_handle, token, SYNC_TIMEOUT).await
}

async fn synchronize_once_with_timeout(
    client: &ChargingHttpClient,
    database_handle: persistence::RideDatabase,
    token: String,
    timeout: Duration,
) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
    let mut progress = None;
    let result = tokio::time::timeout(timeout, async {
        let pending_count = database(database_handle.clone(), |database| {
            database.charging_outbox_count()
        })
        .await?;
        let progress = progress.insert(MobileChargingSyncProgress {
            pending_count,
            downloaded: 0,
            uploaded: 0,
        });
        let mut outcome =
            match synchronize_once_pass(client, &database_handle, &token, progress).await {
                Ok(outcome) => outcome,
                Err(error) => sync_outcome(error),
            };
        match database(database_handle, |database| database.charging_outbox_count()).await {
            Ok(pending_count) => progress.pending_count = pending_count,
            Err(error) if !is_terminal_block(outcome) => outcome = sync_outcome(error),
            Err(_) => {}
        }
        Ok::<_, MobileChargingError>(outcome)
    })
    .await;
    match result {
        Ok(Ok(outcome)) => sync_summary(progress, outcome),
        Ok(Err(error)) => Err(error),
        Err(_) => sync_summary(progress, MobileChargingSyncOutcome::Offline),
    }
}

struct MobileChargingSyncProgress {
    pending_count: u32,
    downloaded: u32,
    uploaded: u32,
}

fn sync_summary(
    progress: Option<MobileChargingSyncProgress>,
    outcome: MobileChargingSyncOutcome,
) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
    let Some(progress) = progress else {
        return Err(MobileChargingError::Network);
    };
    Ok(MobileChargingSyncSummaryDto {
        pending_count: progress.pending_count,
        downloaded: progress.downloaded,
        uploaded: progress.uploaded,
        outcome,
    })
}

async fn synchronize_once_pass(
    client: &ChargingHttpClient,
    database_handle: &persistence::RideDatabase,
    token: &str,
    progress: &mut MobileChargingSyncProgress,
) -> Result<MobileChargingSyncOutcome, MobileChargingError> {
    if token.len() != 64 || !token.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Ok(MobileChargingSyncOutcome::AuthenticationRequired);
    }
    let blocked_outcome =
        upload_pending_operations(client, database_handle, token, progress).await?;
    let more_changes = reconcile_directory(client, database_handle, token, progress).await?;
    Ok(make_sync_outcome(
        blocked_outcome,
        more_changes,
        progress.pending_count,
    ))
}

async fn upload_pending_operations(
    client: &ChargingHttpClient,
    database_handle: &persistence::RideDatabase,
    token: &str,
    progress: &mut MobileChargingSyncProgress,
) -> Result<Option<MobileChargingSyncOutcome>, MobileChargingError> {
    let mut blocked_outcome = None;
    for _ in 0..MAX_UPLOADS_PER_SYNC {
        let operation = database(database_handle.clone(), |database| {
            database.next_charging_operation()
        })
        .await?;
        let Some(operation) = operation else { break };
        match upload_operation(client, database_handle, token, operation.clone()).await {
            Ok(()) => {
                progress.uploaded = progress.uploaded.saturating_add(1);
                progress.pending_count = progress.pending_count.saturating_sub(1);
            }
            Err(error) => {
                if let Some(kind) = block_kind(error) {
                    block_rejected_operation(
                        client,
                        database_handle,
                        token,
                        operation,
                        kind,
                        progress,
                    )
                    .await?;
                    if blocked_outcome.is_none() {
                        blocked_outcome = Some(sync_outcome(error));
                    }
                } else {
                    return Err(error);
                }
            }
        }
    }
    Ok(blocked_outcome)
}

async fn reconcile_directory(
    client: &ChargingHttpClient,
    database_handle: &persistence::RideDatabase,
    token: &str,
    progress: &mut MobileChargingSyncProgress,
) -> Result<bool, MobileChargingError> {
    let (count, more_changes) = download_page(client, database_handle, Some(token)).await?;
    progress.downloaded = progress.downloaded.saturating_add(count);
    progress.pending_count = database(database_handle.clone(), |database| {
        database.charging_outbox_count()
    })
    .await?;
    Ok(more_changes)
}

fn make_sync_outcome(
    blocked: Option<MobileChargingSyncOutcome>,
    more_changes: bool,
    pending_count: u32,
) -> MobileChargingSyncOutcome {
    match blocked {
        Some(outcome) => outcome,
        None if more_changes || pending_count > 0 => MobileChargingSyncOutcome::MoreWork,
        None => MobileChargingSyncOutcome::Synced,
    }
}

fn is_terminal_block(outcome: MobileChargingSyncOutcome) -> bool {
    outcome == MobileChargingSyncOutcome::Conflict || outcome == MobileChargingSyncOutcome::Rejected
}

fn block_kind(error: MobileChargingError) -> Option<persistence::ChargingOperationBlockKind> {
    match error {
        MobileChargingError::Conflict => Some(persistence::ChargingOperationBlockKind::Conflict),
        MobileChargingError::Rejected => Some(persistence::ChargingOperationBlockKind::Invalid),
        _ => None,
    }
}

async fn block_rejected_operation(
    client: &ChargingHttpClient,
    database_handle: &persistence::RideDatabase,
    token: &str,
    operation: persistence::ChargingOperation,
    kind: persistence::ChargingOperationBlockKind,
    progress: &mut MobileChargingSyncProgress,
) -> Result<(), MobileChargingError> {
    let already_blocked = if kind == persistence::ChargingOperationBlockKind::Conflict {
        let mut more_changes = true;
        for _ in 0..5 {
            if !more_changes {
                break;
            }
            let (count, remaining) = download_page(client, database_handle, Some(token)).await?;
            progress.downloaded = progress.downloaded.saturating_add(count);
            more_changes = remaining;
        }
        let operation_id = operation.id;
        database(database_handle.clone(), move |database| {
            database.list_blocked_charging_operations().map(|blocked| {
                blocked
                    .iter()
                    .any(|value| value.operation.id == operation_id)
            })
        })
        .await?
    } else {
        false
    };
    if !already_blocked {
        database(database_handle.clone(), move |database| {
            database.block_charging_operation(operation.id, kind)
        })
        .await?;
    }
    Ok(())
}

fn sync_outcome(error: MobileChargingError) -> MobileChargingSyncOutcome {
    outcome_for_error(error)
}

#[cfg(test)]
async fn refresh_directory(
    client: &ChargingHttpClient,
    database_handle: persistence::RideDatabase,
) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
    tokio::time::timeout(
        SYNC_TIMEOUT,
        refresh_directory_once(client, database_handle),
    )
    .await
    .map_err(|_| MobileChargingError::Network)?
}

async fn refresh_directory_once(
    client: &ChargingHttpClient,
    database_handle: persistence::RideDatabase,
) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
    let pending_count = database(database_handle.clone(), |database| {
        database.charging_outbox_count()
    })
    .await?;
    if pending_count > 0 {
        return Ok(MobileChargingSyncSummaryDto {
            pending_count,
            downloaded: 0,
            uploaded: 0,
            outcome: MobileChargingSyncOutcome::Pending,
        });
    }
    let (downloaded, more_changes) = download_page(client, &database_handle, None).await?;
    let pending_count =
        database(database_handle, |database| database.charging_outbox_count()).await?;
    Ok(MobileChargingSyncSummaryDto {
        pending_count,
        downloaded,
        uploaded: 0,
        outcome: if more_changes {
            MobileChargingSyncOutcome::MoreWork
        } else if pending_count > 0 {
            MobileChargingSyncOutcome::Pending
        } else {
            MobileChargingSyncOutcome::Synced
        },
    })
}

#[uniffi::export(async_runtime = "tokio")]
#[allow(clippy::needless_pass_by_value)]
impl RideDatabaseHandle {
    /// Synchronizes bounded directory changes and retryable local submissions.
    ///
    /// Progress and service failures are returned as a typed summary outcome. A storage setup,
    /// database read, or lock timeout can still return an error before a progress record exists.
    ///
    /// # Errors
    /// Returns an error when the database is unavailable, the sync lock times out, or request
    /// setup fails before the progress summary can be constructed.
    pub async fn synchronize_charging_spots(
        &self,
        token: String,
    ) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
        let started = tokio::time::Instant::now();
        let _guard = tokio::time::timeout(SYNC_TIMEOUT, self.charging_sync_lock.lock())
            .await
            .map_err(|_| MobileChargingError::Network)?;
        synchronize_once_with_timeout(
            &ChargingHttpClient::production()?,
            self.inner.clone(),
            token,
            SYNC_TIMEOUT.saturating_sub(started.elapsed()),
        )
        .await
    }

    /// Refreshes public directory pages when no local contribution is awaiting acknowledgement.
    ///
    /// # Errors
    /// Returns a typed network or storage error.
    pub async fn refresh_charging_directory(
        &self,
    ) -> Result<MobileChargingSyncSummaryDto, MobileChargingError> {
        tokio::time::timeout(SYNC_TIMEOUT, async {
            let _guard = self.charging_sync_lock.lock().await;
            refresh_directory_once(&ChargingHttpClient::production()?, self.inner.clone()).await
        })
        .await
        .map_err(|_| MobileChargingError::Network)?
    }

    /// Lists local drafts blocked by a server conflict or validation rejection.
    ///
    /// # Errors
    /// Returns a storage error if the local database cannot be read.
    pub async fn list_blocked_charging_operations(
        &self,
    ) -> Result<Vec<MobileChargingSyncConflictDto>, MobileChargingError> {
        let blocked = database(self.inner.clone(), |database| {
            database.list_blocked_charging_operations()
        })
        .await?;
        Ok(blocked.iter().map(blocked_operation_dto).collect())
    }

    /// Resolves a previously reviewed blocked contribution using its current local revision.
    ///
    /// # Errors
    /// Returns a conflict error for a stale review or a typed storage error.
    pub async fn resolve_charging_operation(
        &self,
        operation_id: String,
        expected_local_revision: u64,
        resolution: MobileChargingResolutionDto,
    ) -> Result<(), MobileChargingError> {
        let id =
            Uuid::parse_str(&operation_id).map_err(|_| MobileChargingError::InvalidIdentifier)?;
        let expected = persistence::ChargingRevision::new(expected_local_revision)
            .map_err(|_| MobileChargingError::Conflict)?;
        let resolution = match resolution {
            MobileChargingResolutionDto::Retry => persistence::ChargingOperationResolution::Retry,
            MobileChargingResolutionDto::RebaseKeepingLocal => {
                persistence::ChargingOperationResolution::RebaseKeepingLocal
            }
            MobileChargingResolutionDto::AcceptServer => {
                persistence::ChargingOperationResolution::AcceptServer
            }
        };
        database(self.inner.clone(), move |database| {
            database.resolve_charging_operation(
                persistence::ChargingOperationId::from_uuid(id),
                expected,
                resolution,
            )
        })
        .await
    }

    /// Replaces a rejected draft and queues it with a fresh durable operation identity.
    ///
    /// # Errors
    /// Returns a conflict, validation, or storage error.
    pub async fn replace_blocked_charging_operation(
        &self,
        operation_id: String,
        expected_local_revision: u64,
        fields: MobileChargingInputDto,
    ) -> Result<MobileChargingWriteReceiptDto, MobileChargingError> {
        let id =
            Uuid::parse_str(&operation_id).map_err(|_| MobileChargingError::InvalidIdentifier)?;
        let expected = persistence::ChargingRevision::new(expected_local_revision)
            .map_err(|_| MobileChargingError::Conflict)?;
        let fields = input(fields)?;
        let receipt = database(self.inner.clone(), move |database| {
            database.replace_blocked_charging_operation(
                persistence::ChargingOperationId::from_uuid(id),
                expected,
                fields,
            )
        })
        .await?;
        Ok(receipt_dto(receipt))
    }

    /// Returns the time of the last successfully committed public directory page.
    ///
    /// # Errors
    /// Returns a storage error if the local database cannot be read.
    pub async fn charging_directory_last_updated_ms(
        &self,
    ) -> Result<Option<u64>, MobileChargingError> {
        database(self.inner.clone(), |database| {
            database.charging_directory_last_updated_ms()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charging::MobileChargingVoteDto;
    use axum::{
        Json, Router,
        extract::State,
        http::Request,
        middleware::{self, Next},
        response::{IntoResponse, Response},
        routing::{get, post},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::{net::SocketAddr, path::PathBuf};

    async fn start_server(
        router: Router,
    ) -> Result<(String, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        Ok((format!("http://{address}/"), task))
    }

    fn database_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cutout-charging-{label}-{}.sqlite", Uuid::new_v4()))
    }

    #[test]
    fn sync_timeout_without_initial_progress_returns_network_error() {
        assert_eq!(
            sync_summary(None, MobileChargingSyncOutcome::Offline),
            Err(MobileChargingError::Network)
        );
    }

    fn test_input() -> Result<ride_maps::NewChargingSpot, Box<dyn std::error::Error>> {
        let coordinate = ride_maps::Coordinate::from_degrees(39.7392, -104.9903)?;
        Ok(ride_maps::NewChargingSpot::new(
            coordinate,
            "Ask at the front desk",
            ride_maps::ChargingCost::Free,
        )?)
    }

    #[test]
    fn blocked_operation_dto_preserves_action_vote_clear_and_local_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let local_id = persistence::MapPointId::from_uuid(Uuid::new_v4());
        let remote_id = ride_maps::ChargingSpotId::parse(&Uuid::new_v4().to_string())?;
        let operation = persistence::ChargingOperation {
            id: persistence::ChargingOperationId::from_uuid(Uuid::new_v4()),
            point_id: local_id,
            local_revision: persistence::ChargingRevision::new(2)?,
            target: persistence::ChargingTarget::Existing {
                id: remote_id,
                expected_revision: persistence::ChargingRevision::new(1)?,
            },
            action: persistence::ChargingAction::Vote(None),
        };
        let blocked = persistence::ChargingBlockedOperation {
            operation,
            kind: persistence::ChargingOperationBlockKind::Invalid,
            local_revision: persistence::ChargingRevision::new(2)?,
            local_draft: None,
            remote_entry: None,
            can_rebase: false,
        };

        let dto = blocked_operation_dto(&blocked);
        assert_eq!(dto.id, local_id.uuid().to_string());
        assert_eq!(
            dto.action,
            MobileChargingPendingActionDto::Vote { selection: None }
        );
        assert_eq!(
            pending_action(&persistence::ChargingAction::Create(test_input()?)),
            MobileChargingPendingActionDto::Create
        );
        assert_eq!(
            pending_action(&persistence::ChargingAction::Edit(test_input()?)),
            MobileChargingPendingActionDto::Edit
        );
        assert_eq!(
            pending_action(&persistence::ChargingAction::Withdraw),
            MobileChargingPendingActionDto::Withdraw
        );
        assert_eq!(
            pending_action(&persistence::ChargingAction::Verify),
            MobileChargingPendingActionDto::Verify
        );
        assert_eq!(
            pending_action(&persistence::ChargingAction::Vote(Some(
                ride_maps::ChargingVote::Up,
            ))),
            MobileChargingPendingActionDto::Vote {
                selection: Some(MobileChargingVoteDto::Up),
            }
        );
        Ok(())
    }

    async fn unavailable() -> StatusCode {
        StatusCode::SERVICE_UNAVAILABLE
    }

    async fn large_response() -> Json<serde_json::Value> {
        Json(serde_json::json!({ "payload": "x".repeat(40 * 1024) }))
    }

    async fn oversized_stream() -> Response {
        let body = axum::body::Body::from_stream(futures_util::stream::iter([Ok::<
            _,
            std::convert::Infallible,
        >(
            axum::body::Bytes::from(vec![b'x'; MAX_RESPONSE_BYTES + 1]),
        )]));
        Response::builder()
            .status(StatusCode::OK)
            .body(body)
            .expect("static response is valid")
    }

    async fn inconsistent_page(State(id): State<String>) -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "items": [{
                "id": id,
                "revision": 1,
                "status": "published",
                "change_sequence": 1,
                "spot": {
                    "id": id,
                    "revision": 1,
                    "created_at_ms": 100,
                    "updated_at_ms": 100,
                    "change_sequence": 1,
                    "last_verified_at_ms": null,
                    "coordinate": {"latitude_e7": 397_392_000, "longitude_e7": -1_049_903_000},
                    "access_note": "Ask at the front desk",
                    "cost": "free",
                    "name": null,
                    "venue": null,
                    "hours": null,
                    "paid_terms": null,
                    "price": null,
                    "supplies": [],
                    "votes": {"up": 0, "down": 0}
                },
                "duplicate_of": null
            }],
            "next_cursor": 1,
            "has_more": false
        }))
    }

    async fn newer_vote() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "revision": 1,
            "vote": null,
            "votes": {"up": 1, "down": 0},
            "spot_status": "published",
            "change_sequence": 2
        }))
    }

    async fn drop_first_create_response(
        State(dropped): State<Arc<AtomicBool>>,
        request: Request<axum::body::Body>,
        next: Next,
    ) -> Response {
        let is_create =
            request.method() == Method::POST && request.uri().path() == "/v1/charging-spots";
        let response = next.run(request).await;
        if is_create
            && response.status() == StatusCode::CREATED
            && !dropped.swap(true, Ordering::SeqCst)
        {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else {
            response
        }
    }

    async fn drop_first_vote_response(
        State(dropped): State<Arc<AtomicBool>>,
        request: Request<axum::body::Body>,
        next: Next,
    ) -> Response {
        let is_vote = request.method() == Method::PUT && request.uri().path().ends_with("/vote");
        let response = next.run(request).await;
        if is_vote && response.status() == StatusCode::OK && !dropped.swap(true, Ordering::SeqCst) {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else {
            response
        }
    }

    async fn fail_second_create_request(
        State(failed_second): State<Arc<AtomicBool>>,
        request: Request<axum::body::Body>,
        next: Next,
    ) -> Response {
        let is_create =
            request.method() == Method::POST && request.uri().path() == "/v1/charging-spots";
        if is_create && failed_second.swap(true, Ordering::SeqCst) {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else {
            next.run(request).await
        }
    }

    async fn delay_second_create_request(
        State(delayed_second): State<Arc<AtomicBool>>,
        request: Request<axum::body::Body>,
        next: Next,
    ) -> Response {
        let is_create =
            request.method() == Method::POST && request.uri().path() == "/v1/charging-spots";
        if is_create && delayed_second.swap(true, Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        next.run(request).await
    }

    #[test]
    fn removed_duplicate_state_accepts_only_a_valid_canonical_uuid() {
        let duplicate_id = Uuid::new_v4().to_string();
        let canonical_id = Uuid::new_v4().to_string();
        let removed = serde_json::json!({
            "id": duplicate_id,
            "revision": 2,
            "status": "removed",
            "change_sequence": 3,
            "duplicate_of": canonical_id,
        });
        let state: SpotStateResponse = serde_json::from_value(removed).expect("state JSON");
        let entry = state_entry(state).expect("valid duplicate tombstone");
        assert_eq!(entry.status, persistence::ChargingStatus::Removed);
        assert!(entry.input.is_none());

        let invalid_duplicate = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "revision": 2,
            "status": "removed",
            "change_sequence": 4,
            "duplicate_of": "not-a-uuid",
        });
        let state: SpotStateResponse =
            serde_json::from_value(invalid_duplicate).expect("state JSON");
        assert_eq!(state_entry(state), Err(MobileChargingError::Network));
    }

    #[tokio::test]
    async fn durable_retry_vote_clear_and_withdrawal_use_server_receipts()
    -> Result<(), Box<dyn std::error::Error>> {
        let server_path = database_path("server");
        let local_path = database_path("local");
        let server_store = cutout_server::Store::open(&server_path)?;
        let (base_url, task) = start_server(cutout_server::router(server_store)).await?;
        let local = persistence::RideDatabase::open(&local_path)?;
        let identity = register(&ChargingHttpClient::loopback(&base_url)?).await?;
        let receipt = local.save_charging_spot(test_input()?)?;
        let operation = local
            .next_charging_operation()?
            .ok_or("missing durable create")?;

        // A transient transport failure leaves the same operation ID and frozen request queued.
        let (failing_url, failing_task) =
            start_server(Router::new().route("/v1/charging-spots", post(unavailable))).await?;
        let failure = upload_operation(
            &ChargingHttpClient::loopback(&failing_url)?,
            &local,
            &identity.token,
            operation.clone(),
        )
        .await;
        assert_eq!(failure, Err(MobileChargingError::Network));
        failing_task.abort();
        assert_eq!(
            local.next_charging_operation()?.ok_or("retry lost")?.id,
            operation.id
        );

        let synchronized = synchronize(
            &ChargingHttpClient::loopback(&base_url)?,
            local.clone(),
            identity.token.clone(),
        )
        .await?;
        assert_eq!(synchronized.uploaded, 1);
        assert_eq!(synchronized.pending_count, 0);
        let spot = local
            .charging_spot(receipt.point_id)?
            .ok_or("published spot missing")?;
        assert_eq!(spot.status, persistence::ChargingStatus::Published);
        assert_eq!(spot.pending_operations, 0);

        local.vote_charging_spot(
            receipt.point_id,
            spot.local_revision,
            Some(ride_maps::ChargingVote::Up),
        )?;
        synchronize(
            &ChargingHttpClient::loopback(&base_url)?,
            local.clone(),
            identity.token.clone(),
        )
        .await?;
        let voted = local
            .charging_spot(receipt.point_id)?
            .ok_or("voted spot missing")?;
        assert_eq!(voted.vote, Some(ride_maps::ChargingVote::Up));
        assert!(!voted.pending_vote);
        assert_eq!(voted.votes_up, 1);

        local.vote_charging_spot(receipt.point_id, voted.local_revision, None)?;
        synchronize(
            &ChargingHttpClient::loopback(&base_url)?,
            local.clone(),
            identity.token.clone(),
        )
        .await?;
        let cleared = local
            .charging_spot(receipt.point_id)?
            .ok_or("cleared spot missing")?;
        assert_eq!(cleared.vote, None);
        assert!(!cleared.pending_vote);
        assert_eq!(cleared.votes_up, 0);

        local.withdraw_charging_spot(receipt.point_id, cleared.local_revision)?;
        synchronize(
            &ChargingHttpClient::loopback(&base_url)?,
            local.clone(),
            identity.token.clone(),
        )
        .await?;
        let withdrawn = local
            .charging_spot(receipt.point_id)?
            .ok_or("withdrawn spot missing")?;
        assert_eq!(withdrawn.status, persistence::ChargingStatus::Withdrawn);
        assert!(withdrawn.input.is_none());
        assert_eq!(withdrawn.pending_operations, 0);
        let visible = local.charging_spots_in_bounds(
            persistence::GeoBounds::new(39.0, 40.0, -105.5, -104.0)?,
            persistence::ChargingFilter::default(),
            None,
            persistence::QueryLimit::new(10)?,
        )?;
        assert_eq!(visible.spots.len(), 0);

        vote_then_withdraw_before_sync(&local, &base_url, &identity.token).await?;

        task.abort();
        drop(local);
        let _ = std::fs::remove_file(server_path);
        let _ = std::fs::remove_file(local_path);
        Ok(())
    }

    #[tokio::test]
    async fn conflict_summary_keeps_downloaded_and_uploaded_progress()
    -> Result<(), Box<dyn std::error::Error>> {
        let server_path = database_path("sync-conflict-progress-server");
        let local_path = database_path("sync-conflict-progress-local");
        let server_store = cutout_server::Store::open(&server_path)?;
        let (base_url, task) = start_server(cutout_server::router(server_store)).await?;
        let client = ChargingHttpClient::loopback(&base_url)?;
        let local = persistence::RideDatabase::open(&local_path)?;
        let identity = register(&client).await?;
        let receipt = local.save_charging_spot(test_input()?)?;
        let initial = synchronize(&client, local.clone(), identity.token.clone()).await?;
        assert_eq!(initial.uploaded, 1);

        let spot = local
            .charging_spot(receipt.point_id)?
            .ok_or("created spot missing")?;
        let remote_id = spot.remote_id.ok_or("remote identity missing")?;
        let verification = json_body(&RevisionRequest {
            operation_id: Uuid::new_v4(),
            expected_revision: 1,
        })?;
        let _: MutationReceiptResponse = client
            .request(
                Method::POST,
                &format!("v1/charging-spots/{}/verification", remote_id.as_uuid()),
                Some(&identity.token),
                Some(verification),
            )
            .await?;
        let edited_input = ride_maps::NewChargingSpot::new(
            ride_maps::Coordinate::from_degrees(39.7392, -104.9903)?,
            "Use the north entrance",
            ride_maps::ChargingCost::Free,
        )?;
        local.edit_charging_spot(receipt.point_id, spot.local_revision, edited_input)?;
        let second_input = ride_maps::NewChargingSpot::new(
            ride_maps::Coordinate::from_degrees(39.8, -104.9)?,
            "Ask at the front desk",
            ride_maps::ChargingCost::Free,
        )?;
        local.save_charging_spot(second_input)?;

        let summary = synchronize(&client, local.clone(), identity.token).await?;

        assert_eq!(summary.outcome, MobileChargingSyncOutcome::Conflict);
        assert_eq!(summary.pending_count, 1);
        assert_eq!(summary.uploaded, 1);
        assert_eq!(summary.downloaded, 2);
        assert_eq!(local.list_blocked_charging_operations()?.len(), 1);

        task.abort();
        drop(local);
        let _ = std::fs::remove_file(server_path);
        let _ = std::fs::remove_file(local_path);
        Ok(())
    }

    #[tokio::test]
    async fn midpass_network_failure_keeps_earlier_upload_progress()
    -> Result<(), Box<dyn std::error::Error>> {
        let server_path = database_path("sync-network-progress-server");
        let local_path = database_path("sync-network-progress-local");
        let server_store = cutout_server::Store::open(&server_path)?;
        let router = cutout_server::router(server_store).layer(middleware::from_fn_with_state(
            Arc::new(AtomicBool::new(false)),
            fail_second_create_request,
        ));
        let (base_url, task) = start_server(router).await?;
        let client = ChargingHttpClient::loopback(&base_url)?;
        let local = persistence::RideDatabase::open(&local_path)?;
        let identity = register(&client).await?;
        local.save_charging_spot(test_input()?)?;
        let second_input = ride_maps::NewChargingSpot::new(
            ride_maps::Coordinate::from_degrees(39.8, -104.9)?,
            "Ask at the front desk",
            ride_maps::ChargingCost::Free,
        )?;
        local.save_charging_spot(second_input)?;

        let summary = synchronize(&client, local.clone(), identity.token).await?;

        assert_eq!(summary.outcome, MobileChargingSyncOutcome::Offline);
        assert_eq!(summary.pending_count, 1);
        assert_eq!(summary.uploaded, 1);
        assert_eq!(summary.downloaded, 0);

        task.abort();
        drop(local);
        let _ = std::fs::remove_file(server_path);
        let _ = std::fs::remove_file(local_path);
        Ok(())
    }

    #[tokio::test]
    async fn sync_deadline_keeps_committed_upload_progress()
    -> Result<(), Box<dyn std::error::Error>> {
        let server_path = database_path("sync-deadline-progress-server");
        let local_path = database_path("sync-deadline-progress-local");
        let server_store = cutout_server::Store::open(&server_path)?;
        let router = cutout_server::router(server_store).layer(middleware::from_fn_with_state(
            Arc::new(AtomicBool::new(false)),
            delay_second_create_request,
        ));
        let (base_url, task) = start_server(router).await?;
        let client = ChargingHttpClient::loopback(&base_url)?;
        let local = persistence::RideDatabase::open(&local_path)?;
        let identity = register(&client).await?;
        local.save_charging_spot(test_input()?)?;
        let second_input = ride_maps::NewChargingSpot::new(
            ride_maps::Coordinate::from_degrees(39.8, -104.9)?,
            "Ask at the front desk",
            ride_maps::ChargingCost::Free,
        )?;
        local.save_charging_spot(second_input)?;

        let summary = synchronize_once_with_timeout(
            &client,
            local.clone(),
            identity.token,
            Duration::from_secs(2),
        )
        .await?;

        assert_eq!(summary.outcome, MobileChargingSyncOutcome::Offline);
        assert_eq!(summary.pending_count, 1);
        assert_eq!(summary.uploaded, 1);
        assert_eq!(summary.downloaded, 0);

        task.abort();
        drop(local);
        let _ = std::fs::remove_file(server_path);
        let _ = std::fs::remove_file(local_path);
        Ok(())
    }

    async fn vote_then_withdraw_before_sync(
        local: &persistence::RideDatabase,
        base_url: &str,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = ChargingHttpClient::loopback(base_url)?;
        let queued = local.save_charging_spot(test_input()?)?;
        synchronize(&client, local.clone(), token.to_owned()).await?;
        let published = local
            .charging_spot(queued.point_id)?
            .ok_or("second published spot missing")?;
        let remote_id = published.remote_id.ok_or("second remote ID missing")?;
        local.vote_charging_spot(
            queued.point_id,
            published.local_revision,
            Some(ride_maps::ChargingVote::Up),
        )?;
        let voted = local
            .charging_spot(queued.point_id)?
            .ok_or("second voted spot missing")?;
        local.withdraw_charging_spot(queued.point_id, voted.local_revision)?;

        let synchronized = synchronize(&client, local.clone(), token.to_owned()).await?;
        assert_eq!(synchronized.pending_count, 0);
        let withdrawn = local
            .charging_spot(queued.point_id)?
            .ok_or("withdrawn spot missing")?;
        assert_eq!(withdrawn.status, persistence::ChargingStatus::Withdrawn);
        assert!(withdrawn.input.is_none());
        assert_eq!(withdrawn.pending_operations, 0);

        let server_page: ChangesPageResponse = client
            .request(
                Method::GET,
                "v1/charging-spots/changes?after=0&limit=10",
                None,
                None,
            )
            .await?;
        let server_spot = server_page
            .items
            .iter()
            .find(|item| item.id == remote_id.as_uuid().to_string())
            .ok_or("withdrawn spot missing from server changes")?;
        assert_eq!(server_spot.status, ServerStatus::Withdrawn);
        Ok(())
    }

    #[tokio::test]
    async fn committed_create_with_lost_response_retries_after_reopen()
    -> Result<(), Box<dyn std::error::Error>> {
        let server_path = database_path("server-lost-response");
        let local_path = database_path("local-lost-response");
        let server_store = cutout_server::Store::open(&server_path)?;
        let router = cutout_server::router(server_store).layer(middleware::from_fn_with_state(
            Arc::new(AtomicBool::new(false)),
            drop_first_create_response,
        ));
        let (base_url, task) = start_server(router).await?;
        let local = persistence::RideDatabase::open(&local_path)?;
        let identity = register(&ChargingHttpClient::loopback(&base_url)?).await?;
        let receipt = local.save_charging_spot(test_input()?)?;
        let operation_id = local
            .next_charging_operation()?
            .ok_or("missing durable create")?
            .id;

        let first = synchronize(
            &ChargingHttpClient::loopback(&base_url)?,
            local.clone(),
            identity.token.clone(),
        )
        .await;
        let retained_failure_progress = match first {
            Ok(summary) => {
                summary.outcome == MobileChargingSyncOutcome::Offline && summary.uploaded == 0
            }
            Err(_) => false,
        };
        assert!(retained_failure_progress);

        // The operation replay keeps its original applied_revision, while the server
        // returns the current state after a later verification has advanced the spot.
        let client = ChargingHttpClient::loopback(&base_url)?;
        let page: ChangesPageResponse = client
            .request(
                Method::GET,
                "v1/charging-spots/changes?after=0&limit=10",
                None,
                None,
            )
            .await?;
        let remote_id = page
            .items
            .first()
            .ok_or("server create was not committed")?
            .id
            .clone();
        let verification = json_body(&RevisionRequest {
            operation_id: Uuid::new_v4(),
            expected_revision: 1,
        })?;
        let _: MutationReceiptResponse = client
            .request(
                Method::POST,
                &format!("v1/charging-spots/{remote_id}/verification"),
                Some(&identity.token),
                Some(verification),
            )
            .await?;
        drop(local);

        let reopened = persistence::RideDatabase::open(&local_path)?;
        let retry = reopened
            .next_charging_operation()?
            .ok_or("create was not durable after reopen")?;
        assert_eq!(retry.id, operation_id);
        let second = synchronize(
            &ChargingHttpClient::loopback(&base_url)?,
            reopened.clone(),
            identity.token,
        )
        .await?;
        assert_eq!(second.uploaded, 1);
        assert_eq!(second.pending_count, 0);
        let published = reopened
            .charging_spot(receipt.point_id)?
            .ok_or("server receipt was not applied")?;
        assert_eq!(published.status, persistence::ChargingStatus::Published);
        assert!(published.remote_id.is_some());
        assert_eq!(published.remote_revision.get(), 2);
        assert!(published.verified_at_ms.is_some());
        assert_eq!(published.pending_operations, 0);

        task.abort();
        drop(reopened);
        let _ = std::fs::remove_file(server_path);
        let _ = std::fs::remove_file(local_path);
        Ok(())
    }

    #[tokio::test]
    async fn replayed_vote_receipt_keeps_historical_applied_revision_and_current_vote()
    -> Result<(), Box<dyn std::error::Error>> {
        let server_path = database_path("server-vote-replay");
        let local_path = database_path("local-vote-replay");
        let server_store = cutout_server::Store::open(&server_path)?;
        let router = cutout_server::router(server_store).layer(middleware::from_fn_with_state(
            Arc::new(AtomicBool::new(false)),
            drop_first_vote_response,
        ));
        let (base_url, task) = start_server(router).await?;
        let client = ChargingHttpClient::loopback(&base_url)?;
        let local = persistence::RideDatabase::open(&local_path)?;
        let identity = register(&client).await?;
        let receipt = local.save_charging_spot(test_input()?)?;
        synchronize(&client, local.clone(), identity.token.clone()).await?;
        let spot = local
            .charging_spot(receipt.point_id)?
            .ok_or("created spot missing")?;

        let voted = local.vote_charging_spot(
            receipt.point_id,
            spot.local_revision,
            Some(ride_maps::ChargingVote::Up),
        )?;
        let operation = local
            .next_charging_operation()?
            .ok_or("missing vote operation")?;
        assert_eq!(operation.id.uuid(), voted.operation_id.uuid());
        assert_eq!(
            upload_operation(&client, &local, &identity.token, operation.clone()).await,
            Err(MobileChargingError::Network)
        );

        let remote_id = local
            .charging_spot(receipt.point_id)?
            .and_then(|spot| spot.remote_id)
            .ok_or("remote id missing")?;
        let replacement = VoteRequest {
            operation_id: Uuid::new_v4(),
            expected_revision: 1,
            vote: Some(ride_maps::ChargingVote::Down),
        };
        let _: VoteReceiptResponse = client
            .request(
                Method::PUT,
                &format!("v1/charging-spots/{}/vote", remote_id.as_uuid()),
                Some(&identity.token),
                Some(json_body(&replacement)?),
            )
            .await?;

        upload_operation(&client, &local, &identity.token, operation).await?;
        let acknowledged = local
            .charging_spot(receipt.point_id)?
            .ok_or("acknowledged vote missing")?;
        assert_eq!(acknowledged.vote, Some(ride_maps::ChargingVote::Down));
        assert_eq!(acknowledged.votes_down, 1);
        assert!(!acknowledged.pending_vote);
        assert_eq!(acknowledged.pending_operations, 0);

        synchronize(&client, local.clone(), identity.token).await?;
        let refreshed = local
            .charging_spot(receipt.point_id)?
            .ok_or("refreshed vote missing")?;
        assert!(refreshed.change_sequence.get() >= 3);
        assert_eq!(refreshed.vote, Some(ride_maps::ChargingVote::Down));

        task.abort();
        drop(local);
        let _ = std::fs::remove_file(server_path);
        let _ = std::fs::remove_file(local_path);
        Ok(())
    }

    #[tokio::test]
    async fn unauthenticated_refresh_caches_public_spots_and_skips_pending_outbox()
    -> Result<(), Box<dyn std::error::Error>> {
        let public_path = database_path("public-cache");
        let router = Router::new()
            .route("/v1/charging-spots/changes", get(inconsistent_page))
            .with_state(Uuid::new_v4().to_string());
        let (base_url, task) = start_server(router).await?;
        let client = ChargingHttpClient::loopback(&base_url)?;
        let public_cache = persistence::RideDatabase::open(&public_path)?;
        let refreshed = refresh_directory(&client, public_cache.clone()).await?;
        assert_eq!(refreshed.downloaded, 1);
        let bounds = persistence::GeoBounds::new(39.0, 40.0, -105.5, -104.0)?;
        let public_page = public_cache.charging_spots_in_bounds(
            bounds,
            persistence::ChargingFilter::default(),
            None,
            persistence::QueryLimit::new(10)?,
        )?;
        assert_eq!(public_page.spots.len(), 1);
        assert!(!public_page.spots[0].owned);

        let cursor = public_cache.charging_changes_cursor()?;
        public_cache.save_charging_spot(test_input()?)?;
        let skipped = refresh_directory(&client, public_cache.clone()).await?;
        assert_eq!(skipped.downloaded, 0);
        assert_eq!(skipped.pending_count, 1);
        assert_eq!(public_cache.charging_changes_cursor()?, cursor);

        task.abort();
        drop(public_cache);
        let _ = std::fs::remove_file(public_path);
        Ok(())
    }

    #[tokio::test]
    async fn malformed_success_payload_is_rejected_as_network_error()
    -> Result<(), Box<dyn std::error::Error>> {
        async fn malformed() -> (StatusCode, &'static str) {
            (StatusCode::OK, "not-json")
        }
        let (url, task) =
            start_server(Router::new().route("/v1/identities", post(malformed))).await?;
        let result = register(&ChargingHttpClient::loopback(&url)?).await;
        assert!(match result {
            Err(MobileChargingError::Network) => true,
            _ => false,
        });
        task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn response_bound_accepts_valid_large_json_and_rejects_oversized_stream()
    -> Result<(), Box<dyn std::error::Error>> {
        let (url, task) = start_server(
            Router::new()
                .route("/large", get(large_response))
                .route("/huge", get(oversized_stream)),
        )
        .await?;
        let client = ChargingHttpClient::loopback(&url)?;
        let large: serde_json::Value = client.request(Method::GET, "large", None, None).await?;
        assert_eq!(
            large["payload"].as_str().ok_or("missing payload")?.len(),
            40 * 1024
        );
        let huge: Result<serde_json::Value, _> =
            client.request(Method::GET, "huge", None, None).await;
        assert!(match huge {
            Err(MobileChargingError::Network) => true,
            _ => false,
        });
        task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn newer_own_vote_sequence_rejects_stale_directory_snapshot()
    -> Result<(), Box<dyn std::error::Error>> {
        let spot_id = Uuid::new_v4().to_string();
        let router = Router::new()
            .route("/v1/charging-spots/changes", get(inconsistent_page))
            .route("/v1/charging-spots/{id}/vote", get(newer_vote))
            .with_state(spot_id);
        let (url, task) = start_server(router).await?;
        let local_path = database_path("stale-page");
        let local = persistence::RideDatabase::open(&local_path)?;
        let token = "a".repeat(64);
        let result =
            download_page(&ChargingHttpClient::loopback(&url)?, &local, Some(&token)).await;
        assert!(match result {
            Err(MobileChargingError::Network) => true,
            _ => false,
        });
        assert_eq!(local.charging_changes_cursor()?.get(), 0);
        task.abort();
        drop(local);
        let _ = std::fs::remove_file(local_path);
        Ok(())
    }

    #[test]
    fn contribution_serializes_only_the_public_charging_submission()
    -> Result<(), Box<dyn std::error::Error>> {
        let submission = serde_json::to_value(test_input()?.community_submission())?;
        let fields = submission
            .as_object()
            .ok_or("submission must be an object")?;
        for field in fields.keys() {
            assert!(
                [
                    "coordinate",
                    "access_note",
                    "cost",
                    "name",
                    "venue",
                    "hours",
                    "paid_terms",
                    "price",
                    "supplies",
                ]
                .contains(&field.as_str()),
                "unexpected public request field: {field}"
            );
        }
        assert!(!fields.contains_key("token"));
        assert!(!fields.contains_key("identity_id"));
        assert!(!fields.contains_key("ride"));
        Ok(())
    }

    #[test]
    fn only_test_loopback_clients_can_use_plain_http() {
        assert!(ChargingHttpClient::loopback("http://127.0.0.1:8080/").is_ok());
        assert!(ChargingHttpClient::loopback("http://[::1]:8080/").is_ok());
        assert!(ChargingHttpClient::loopback("http://example.test/").is_err());
        assert!(ChargingHttpClient::loopback("https://example.test/").is_err());
    }

    #[test]
    fn status_codes_are_classified_without_reading_or_logging_tokens() {
        assert_eq!(
            status_error(StatusCode::UNAUTHORIZED),
            MobileChargingError::Unauthorized
        );
        assert_eq!(
            status_error(StatusCode::CONFLICT),
            MobileChargingError::Conflict
        );
        assert_eq!(
            status_error(StatusCode::UNPROCESSABLE_ENTITY),
            MobileChargingError::Rejected
        );
        assert_eq!(
            status_error(StatusCode::TOO_MANY_REQUESTS),
            MobileChargingError::Throttled
        );
        assert_eq!(
            status_error(StatusCode::SERVICE_UNAVAILABLE),
            MobileChargingError::Network
        );
    }
}
