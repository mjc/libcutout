//! Retained charging synchronization runtime and its durable operation policy.

use super::{account, reports, reports::charging_account_token_is_well_formed};
use super::{
    sync::{SyncError, SyncOutcome, SyncResult, SyncSummary, result_for_error},
    transport::{ChargingHttpClient, ChargingTransportError},
};
use libcutout_persistence as persistence;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::PoisonError, time::Duration};
use uuid::Uuid;

const SYNC_TIMEOUT: Duration = Duration::from_secs(20);
const CHANGE_PAGE_SIZE: u32 = 10;
const MAX_UPLOADS_PER_SYNC: u32 = 8;

/// Portable result from a retained synchronization runtime.
#[derive(Debug, thiserror::Error)]
pub enum SyncRuntimeError {
    /// Transport setup or required synchronization work failed before a summary existed.
    #[error("charging synchronization failed: {0:?}")]
    Sync(SyncError),
}

/// Failure while running a direct durable charging operation.
#[derive(Debug, thiserror::Error)]
pub enum ChargingPersistenceError {
    /// A persistence worker stopped before returning the operation result.
    #[error("charging persistence worker stopped")]
    WorkerStopped,
    /// The durable charging operation failed.
    #[error(transparent)]
    Storage(#[from] persistence::StorageError),
}

impl From<SyncError> for SyncRuntimeError {
    fn from(error: SyncError) -> Self {
        Self::Sync(error)
    }
}

/// Long-lived charging database runtime shared by sync, refresh, account deletion, and reports.
///
/// The internal gate serializes sync and public refresh operations across calls. Keep one instance
/// beside the database handle for the full native handle lifetime.
#[derive(Debug)]
pub struct ChargingSyncService {
    database: persistence::RideDatabase,
    gate: tokio::sync::Mutex<()>,
}

impl ChargingSyncService {
    /// Creates the retained runtime around the database handle's existing database identity.
    #[must_use]
    pub fn new(database: persistence::RideDatabase) -> Self {
        Self {
            database,
            gate: tokio::sync::Mutex::new(()),
        }
    }

    /// Registers a directory identity and validates the returned credential before publishing it.
    ///
    /// # Errors
    /// Returns `SyncRuntimeError::Sync` with `Network`, `Unauthorized`, `Conflict`, `Rejected`,
    /// or `Throttled` when the request fails or its identity response is malformed.
    pub async fn register(client: &ChargingHttpClient) -> Result<(Uuid, String), SyncRuntimeError> {
        let response: IdentityResponse = client
            .request(Method::POST, "v1/identities", None, Some(Vec::new()))
            .await
            .map_err(map_transport)?;
        let id = Uuid::parse_str(&response.id).map_err(|_| SyncError::Network)?;
        if response.token.len() != 64
            || !response
                .token
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(SyncError::Network.into());
        }
        Ok((id, response.token))
    }

    /// Deletes the contributor account while retaining the sync fence through durable settlement.
    ///
    /// # Errors
    /// Returns `AccountDeletionError::Unauthorized`, `Network`, `Storage`, `Unsettled`, or
    /// `Remote` when the credential, service, durable state, or completion receipt fails. The
    /// durable deletion fence remains in place when the remote outcome is uncertain.
    pub async fn delete_community_account(
        &self,
        client: &ChargingHttpClient,
        token: &str,
    ) -> Result<u64, account::AccountDeletionError> {
        let port = RuntimeAccountDeletionPort {
            service: self,
            client,
            guard: std::sync::Mutex::new(None),
        };
        account::delete_community_account(&port, token).await
    }

    /// Submits a prepared abuse report through the shared charging service transport.
    ///
    /// # Errors
    /// Returns `ReportSubmitError::Unauthorized`, `InvalidReceipt`, or `Remote` when the token,
    /// service, or returned receipt fails validation.
    pub async fn submit_report(
        client: &ChargingHttpClient,
        report: &reports::ChargingContentReport,
        token: &str,
    ) -> Result<reports::ChargingReportReceipt, reports::ReportSubmitError> {
        reports::submit_charging_report(report, token, &RuntimeReportTransport { client }).await
    }

    /// Runs one bounded authenticated upload/reconciliation pass under the retained gate.
    ///
    /// # Errors
    /// Returns `SyncRuntimeError::Sync` with `Network`, `Unauthorized`, `Conflict`, `Rejected`,
    /// `Throttled`, `Unavailable`, or `StorageFailure` if setup fails before progress exists.
    pub async fn synchronize(
        &self,
        client: &ChargingHttpClient,
        token: &str,
    ) -> Result<SyncSummary, SyncRuntimeError> {
        self.synchronize_with_timeout(client, token, SYNC_TIMEOUT)
            .await
    }

    /// Runs a bounded synchronization pass with an explicit total lock and request budget.
    ///
    /// # Errors
    /// Returns `SyncRuntimeError::Sync` with `Network`, `Unauthorized`, `Conflict`, `Rejected`,
    /// `Throttled`, `Unavailable`, or `StorageFailure` if setup fails before progress exists.
    pub async fn synchronize_with_timeout(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        timeout: Duration,
    ) -> Result<SyncSummary, SyncRuntimeError> {
        let started = tokio::time::Instant::now();
        let _guard = tokio::time::timeout(timeout, self.gate.lock())
            .await
            .map_err(|_| SyncError::Network)?;
        self.synchronize_locked(client, token, timeout.saturating_sub(started.elapsed()))
            .await
    }

    async fn synchronize_locked(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        timeout: Duration,
    ) -> Result<SyncSummary, SyncRuntimeError> {
        let mut progress = None;
        let result = tokio::time::timeout(timeout, async {
            let pending_count = self
                .database(|database| database.charging_outbox_count())
                .await?;
            let progress = progress.insert(SyncProgress {
                pending_count,
                downloaded: 0,
                uploaded: 0,
            });
            let mut outcome = match self.sync_pass(client, token, progress).await {
                Ok(outcome) => outcome,
                Err(error) => SyncOutcome::Finished {
                    result: result_for_error(error),
                },
            };
            match self
                .database(|database| database.charging_outbox_count())
                .await
            {
                Ok(pending_count) => progress.pending_count = pending_count,
                Err(error) if !is_terminal_block(outcome) => {
                    outcome = SyncOutcome::Finished {
                        result: result_for_error(error),
                    };
                }
                Err(_) => {}
            }
            Ok::<_, SyncError>(outcome)
        })
        .await;
        match result {
            Ok(Ok(outcome)) => progress
                .map(|progress| progress.summary(outcome))
                .ok_or(SyncRuntimeError::Sync(SyncError::Network)),
            Ok(Err(error)) => Err(error.into()),
            Err(_) => Ok(progress
                .map(|progress| {
                    progress.summary(SyncOutcome::Finished {
                        result: SyncResult::Offline,
                    })
                })
                .ok_or(SyncRuntimeError::Sync(SyncError::Network))?),
        }
    }

    async fn sync_pass(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        progress: &mut SyncProgress,
    ) -> Result<SyncOutcome, SyncError> {
        if !charging_account_token_is_well_formed(token) {
            return Ok(SyncOutcome::Finished {
                result: SyncResult::AuthenticationRequired,
            });
        }
        let account = self
            .database(|database| database.community_account_state())
            .await?;
        if account != persistence::CommunityAccountState::Active {
            return Ok(SyncOutcome::Finished {
                result: SyncResult::AuthenticationRequired,
            });
        }
        let blocked = self
            .upload_pending_operations(client, token, progress)
            .await?;
        let more_changes = self.reconcile_directory(client, token, progress).await?;
        Ok(match blocked {
            Some(result) => SyncOutcome::Finished { result },
            None if more_changes || progress.pending_count > 0 => SyncOutcome::MoreWork,
            None => SyncOutcome::Finished {
                result: SyncResult::Synced,
            },
        })
    }

    async fn upload_pending_operations(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        progress: &mut SyncProgress,
    ) -> Result<Option<SyncResult>, SyncError> {
        let mut blocked_result = None;
        for _ in 0..MAX_UPLOADS_PER_SYNC {
            let operation = self
                .database(|database| database.next_charging_operation())
                .await?;
            let Some(operation) = operation else { break };
            match self
                .upload_operation_locked(client, token, operation.clone())
                .await
            {
                Ok(()) => {
                    progress.uploaded = progress.uploaded.saturating_add(1);
                    progress.pending_count = progress.pending_count.saturating_sub(1);
                }
                Err(error @ (SyncError::Conflict | SyncError::Rejected)) => {
                    self.block_rejected_operation(client, token, operation, error, progress)
                        .await?;
                    if blocked_result.is_none() {
                        blocked_result = Some(result_for_error(error));
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(blocked_result)
    }

    async fn reconcile_directory(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        progress: &mut SyncProgress,
    ) -> Result<bool, SyncError> {
        let (count, more_changes) = self.download_page_locked(client, Some(token)).await?;
        progress.downloaded = progress.downloaded.saturating_add(count);
        progress.pending_count = self
            .database(|database| database.charging_outbox_count())
            .await?;
        Ok(more_changes)
    }

    /// Refreshes public directory pages only when no local contribution awaits acknowledgement.
    ///
    /// # Errors
    /// Returns `SyncRuntimeError::Sync` with `Network`, `Unauthorized`, `Conflict`, `Rejected`,
    /// `Throttled`, `Unavailable`, or `StorageFailure` if the database, transport, or retained gate
    /// fails before a refresh summary can be returned.
    pub async fn refresh(
        &self,
        client: &ChargingHttpClient,
    ) -> Result<SyncSummary, SyncRuntimeError> {
        tokio::time::timeout(SYNC_TIMEOUT, async {
            let _guard = self.gate.lock().await;
            let pending_count = self
                .database(|database| database.charging_outbox_count())
                .await?;
            if pending_count > 0 {
                return Ok(SyncSummary {
                    pending_count,
                    downloaded: 0,
                    uploaded: 0,
                    outcome: SyncOutcome::Finished {
                        result: SyncResult::Pending,
                    },
                });
            }
            let (downloaded, more_changes) = self.download_page_locked(client, None).await?;
            let pending_count = self
                .database(|database| database.charging_outbox_count())
                .await?;
            Ok(SyncSummary {
                pending_count,
                downloaded,
                uploaded: 0,
                outcome: if more_changes {
                    SyncOutcome::MoreWork
                } else if pending_count > 0 {
                    SyncOutcome::Finished {
                        result: SyncResult::Pending,
                    }
                } else {
                    SyncOutcome::Finished {
                        result: SyncResult::Synced,
                    }
                },
            })
        })
        .await
        .map_err(|_| SyncError::Network)?
        .map_err(SyncRuntimeError::Sync)
    }

    /// Returns the persisted blocked operations without converting them to a native DTO.
    ///
    /// # Errors
    /// Returns `ChargingPersistenceError::WorkerStopped` if the persistence worker stops, or
    /// `ChargingPersistenceError::Storage` with the database error when the query fails.
    pub async fn blocked_operations(
        &self,
    ) -> Result<Vec<persistence::ChargingBlockedOperation>, ChargingPersistenceError> {
        self.database_raw(|database| database.list_blocked_charging_operations())
            .await
    }

    /// Resolves an operation against its expected local revision.
    ///
    /// # Errors
    /// Returns `ChargingPersistenceError::WorkerStopped` if the worker stops, or
    /// `ChargingPersistenceError::Storage` with the database error (including `ChargingConflict`)
    /// when the expected revision is invalid or stale.
    pub async fn resolve_operation(
        &self,
        operation_id: persistence::ChargingOperationId,
        expected: persistence::ChargingRevision,
        resolution: persistence::ChargingOperationResolution,
    ) -> Result<(), ChargingPersistenceError> {
        self.database_raw(move |database| {
            database.resolve_charging_operation(operation_id, expected, resolution)
        })
        .await
    }

    /// Replaces a rejected draft and queues it with a fresh durable operation identity.
    ///
    /// # Errors
    /// Returns `ChargingPersistenceError::WorkerStopped` if the worker stops, or
    /// `ChargingPersistenceError::Storage` with the database error when the operation conflicts,
    /// is rejected, or cannot be persisted.
    pub async fn replace_blocked_operation(
        &self,
        operation_id: persistence::ChargingOperationId,
        expected: persistence::ChargingRevision,
        fields: cutout_ride_maps::NewChargingSpot,
    ) -> Result<persistence::ChargingWriteReceipt, ChargingPersistenceError> {
        self.database_raw(move |database| {
            database.replace_blocked_charging_operation(operation_id, expected, fields)
        })
        .await
    }

    /// Returns the last successfully committed public directory page timestamp.
    ///
    /// # Errors
    /// Returns `ChargingPersistenceError::WorkerStopped` if the worker stops, or
    /// `ChargingPersistenceError::Storage` with the database error if the query fails.
    pub async fn last_updated_ms(&self) -> Result<Option<u64>, ChargingPersistenceError> {
        self.database_raw(|database| database.charging_directory_last_updated_ms())
            .await
    }

    /// Downloads and durably applies one bounded directory page under the service gate.
    ///
    /// # Errors
    /// Returns `SyncRuntimeError::Sync` with `Network`, `Unauthorized`, `Conflict`, `Rejected`,
    /// `Throttled`, `Unavailable`, or `StorageFailure` if transport, gate admission, receipt
    /// validation, or persistence fails.
    pub async fn download_page(
        &self,
        client: &ChargingHttpClient,
        token: Option<&str>,
    ) -> Result<(u32, bool), SyncRuntimeError> {
        tokio::time::timeout(SYNC_TIMEOUT, async {
            let _guard = self.gate.lock().await;
            self.download_page_locked(client, token).await
        })
        .await
        .map_err(|_| SyncError::Network)?
        .map_err(SyncRuntimeError::Sync)
    }

    async fn download_page_locked(
        &self,
        client: &ChargingHttpClient,
        token: Option<&str>,
    ) -> Result<(u32, bool), SyncError> {
        let cursor = self
            .database(|database| database.charging_changes_cursor())
            .await?;
        let path = format!(
            "v1/charging-spots/changes?after={}&limit={CHANGE_PAGE_SIZE}",
            cursor.get()
        );
        let page: ChangesPageResponse = client
            .request(Method::GET, &path, token, None)
            .await
            .map_err(map_transport)?;
        let next_cursor = persistence::ChargingChangeCursor::new(page.next_cursor)
            .map_err(|_| SyncError::Network)?;
        if next_cursor < cursor || page.items.len() > CHANGE_PAGE_SIZE as usize {
            return Err(SyncError::Network);
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
                    .await
                    .map_err(map_transport)?;
                if server_status(vote.spot_status) != persistence::ChargingStatus::Published
                    || vote.change_sequence != entry.change_sequence.get()
                {
                    return Err(SyncError::Network);
                }
                entry.vote = vote.vote;
                entry.vote_revision = persistence::ChargingRevision::new(vote.revision)
                    .map_err(|_| SyncError::Network)?;
                entry.votes_up = vote.votes.up;
                entry.votes_down = vote.votes.down;
            }
            entries.push(entry);
        }
        if token.is_none()
            && self
                .database(|database| database.charging_outbox_count())
                .await?
                > 0
        {
            return Ok((0, true));
        }
        let result = self
            .database(move |database| {
                database
                    .apply_charging_changes_page(entries, next_cursor)
                    .map(|result| result.applied_entries)
            })
            .await?;
        Ok((result, has_more))
    }

    /// Uploads one queued operation and acknowledges only a validated durable receipt.
    ///
    /// # Errors
    /// Returns `SyncRuntimeError::Sync` with `Network`, `Unauthorized`, `Conflict`, `Rejected`,
    /// `Throttled`, `Unavailable`, or `StorageFailure` when the upload or durable acknowledgement
    /// fails.
    pub async fn upload_operation(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        operation: persistence::ChargingOperation,
    ) -> Result<(), SyncRuntimeError> {
        tokio::time::timeout(SYNC_TIMEOUT, async {
            let _guard = self.gate.lock().await;
            self.upload_operation_locked(client, token, operation).await
        })
        .await
        .map_err(|_| SyncError::Network)?
        .map_err(SyncRuntimeError::Sync)
    }

    async fn upload_operation_locked(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        operation: persistence::ChargingOperation,
    ) -> Result<(), SyncError> {
        use persistence::ChargingAction;
        let operation_id = operation.id.uuid();
        let request = operation_request(&operation)?;
        let point_id = operation.point_id;
        if let ChargingAction::Vote(_) = operation.action {
            let receipt: VoteReceiptResponse = client
                .request(request.0, &request.1, Some(token), Some(request.2))
                .await
                .map_err(map_transport)?;
            if receipt.operation_id != operation_id.to_string()
                || receipt.revision < receipt.applied_revision
            {
                return Err(SyncError::Network);
            }
            self.database(move |database| {
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
            .await
        } else {
            let receipt: MutationReceiptResponse = client
                .request(request.0, &request.1, Some(token), Some(request.2))
                .await
                .map_err(map_transport)?;
            if receipt.operation_id != operation_id.to_string()
                || receipt.state.revision < receipt.applied_revision
            {
                return Err(SyncError::Network);
            }
            let mut entry = state_entry(receipt.state)?;
            let current = self
                .database(move |database| database.charging_spot(point_id))
                .await?
                .ok_or(SyncError::StorageFailure)?;
            entry.vote = current.vote;
            entry.vote_revision = current.vote_revision;
            self.database(move |database| {
                database
                    .acknowledge_charging_operation(
                        persistence::ChargingOperationId::from_uuid(operation_id),
                        persistence::ChargingRevision::new(receipt.applied_revision)?,
                        entry,
                    )
                    .map(|_| ())
            })
            .await
        }
    }

    async fn block_rejected_operation(
        &self,
        client: &ChargingHttpClient,
        token: &str,
        operation: persistence::ChargingOperation,
        error: SyncError,
        progress: &mut SyncProgress,
    ) -> Result<(), SyncError> {
        let kind = match error {
            SyncError::Conflict => persistence::ChargingOperationBlockKind::Conflict,
            SyncError::Rejected => persistence::ChargingOperationBlockKind::Invalid,
            _ => return Err(error),
        };
        let already_blocked = if kind == persistence::ChargingOperationBlockKind::Conflict {
            let mut more_changes = true;
            for _ in 0..5 {
                if !more_changes {
                    break;
                }
                let (count, remaining) = self.download_page_locked(client, Some(token)).await?;
                progress.downloaded = progress.downloaded.saturating_add(count);
                more_changes = remaining;
            }
            let operation_id = operation.id;
            self.database(move |database| {
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
            self.database(move |database| database.block_charging_operation(operation.id, kind))
                .await?;
        }
        Ok(())
    }

    async fn database<T, F>(&self, action: F) -> Result<T, SyncError>
    where
        T: Send + 'static,
        F: FnOnce(persistence::RideDatabase) -> Result<T, persistence::StorageError>
            + Send
            + 'static,
    {
        self.database_raw(action)
            .await
            .map_err(|error| match error {
                ChargingPersistenceError::WorkerStopped => SyncError::StorageFailure,
                ChargingPersistenceError::Storage(error) => map_storage(&error),
            })
    }

    async fn database_raw<T, F>(&self, action: F) -> Result<T, ChargingPersistenceError>
    where
        T: Send + 'static,
        F: FnOnce(persistence::RideDatabase) -> Result<T, persistence::StorageError>
            + Send
            + 'static,
    {
        tokio::task::spawn_blocking({
            let database = self.database.clone();
            move || action(database)
        })
        .await
        .map_err(|_| ChargingPersistenceError::WorkerStopped)?
        .map_err(ChargingPersistenceError::Storage)
    }
}

struct RuntimeAccountDeletionPort<'a> {
    service: &'a ChargingSyncService,
    client: &'a ChargingHttpClient,
    guard: std::sync::Mutex<Option<tokio::sync::MutexGuard<'a, ()>>>,
}

impl account::AccountDeletionPort for RuntimeAccountDeletionPort<'_> {
    fn begin_deletion(
        &self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        persistence::CommunityAccountState,
                        account::AccountStorageError,
                    >,
                > + Send
                + '_,
        >,
    > {
        let database = self.service.database.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || database.begin_community_account_deletion())
                .await
                .map_err(|_| account::AccountStorageError::WorkerStopped)?
                .map_err(account::AccountStorageError::Storage)
        })
    }

    fn delete_remote_exclusive<'a>(
        &'a self,
        token: &'a str,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<u64, account::AccountRemoteError>> + Send + 'a>> {
        Box::pin(async move {
            let guard = tokio::time::timeout(timeout, self.service.gate.lock())
                .await
                .map_err(|_| account::AccountRemoteError::Network)?;
            *self.guard.lock().unwrap_or_else(PoisonError::into_inner) = Some(guard);
            let receipt: DeletionReceipt = self
                .client
                .request(Method::DELETE, "v1/identities/current", Some(token), None)
                .await
                .map_err(|error| match error {
                    ChargingTransportError::Unauthorized => {
                        account::AccountRemoteError::Unauthorized
                    }
                    ChargingTransportError::Conflict => account::AccountRemoteError::Conflict,
                    ChargingTransportError::Rejected => account::AccountRemoteError::Rejected,
                    ChargingTransportError::Throttled => account::AccountRemoteError::Throttled,
                    ChargingTransportError::Network => account::AccountRemoteError::Network,
                })?;
            match receipt.status {
                DeletionStatus::Completed => Ok(receipt.completed_at_ms),
            }
        })
    }

    fn complete_deletion(
        &self,
        completed_at_ms: u64,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        persistence::CommunityAccountState,
                        account::AccountStorageError,
                    >,
                > + Send
                + '_,
        >,
    > {
        let database = self.service.database.clone();
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || {
                database.complete_community_account_deletion(completed_at_ms)
            })
            .await
            .map_err(|_| account::AccountStorageError::WorkerStopped)?
            .map_err(account::AccountStorageError::Storage);
            self.guard
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            result
        })
    }
}

struct RuntimeReportTransport<'a> {
    client: &'a ChargingHttpClient,
}

impl reports::ChargingReportTransport for RuntimeReportTransport<'_> {
    fn submit<'a>(
        &'a self,
        token: &'a str,
        command: reports::ChargingReportCommand,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<reports::ChargingReportReceipt, reports::ReportRemoteError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let body = serde_json::to_vec(&ReportRequest {
                operation_id: command.operation_id,
                reason: command.reason,
                message: command.message.as_deref(),
            })
            .map_err(|_| reports::ReportRemoteError::Network)?;
            let response: ReportReceipt = self
                .client
                .request(
                    Method::POST,
                    &format!("v1/charging-spots/{}/reports", command.spot_id),
                    Some(token),
                    Some(body),
                )
                .await
                .map_err(|error| match error {
                    ChargingTransportError::Unauthorized => {
                        reports::ReportRemoteError::Unauthorized
                    }
                    ChargingTransportError::Conflict => reports::ReportRemoteError::Conflict,
                    ChargingTransportError::Rejected => reports::ReportRemoteError::Rejected,
                    ChargingTransportError::Throttled => reports::ReportRemoteError::Throttled,
                    ChargingTransportError::Network => reports::ReportRemoteError::Network,
                })?;
            let _ = response.status;
            Ok(reports::ChargingReportReceipt {
                operation_id: response.operation_id,
                report_id: response.id,
                applied_revision: response.applied_revision,
                revision: response.revision,
            })
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum DeletionStatus {
    Completed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletionReceipt {
    status: DeletionStatus,
    completed_at_ms: u64,
}

#[derive(Serialize)]
struct ReportRequest<'a> {
    operation_id: Uuid,
    reason: &'static str,
    message: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportReceipt {
    operation_id: Uuid,
    id: Uuid,
    applied_revision: u64,
    revision: u64,
    status: ReportStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportStatus {
    Open,
    Resolved,
}

struct SyncProgress {
    pending_count: u32,
    downloaded: u32,
    uploaded: u32,
}

impl SyncProgress {
    fn summary(self, outcome: SyncOutcome) -> SyncSummary {
        SyncSummary {
            pending_count: self.pending_count,
            downloaded: self.downloaded,
            uploaded: self.uploaded,
            outcome,
        }
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
    cost: cutout_ride_maps::ChargingCost,
    name: Option<String>,
    venue: Option<cutout_ride_maps::ChargingVenue>,
    hours: Option<String>,
    paid_terms: Option<cutout_ride_maps::ChargingPaidTerms>,
    price: Option<cutout_ride_maps::ChargingPrice>,
    #[serde(default)]
    supplies: Vec<cutout_ride_maps::ChargingSupply>,
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
    vote: Option<cutout_ride_maps::ChargingVote>,
    votes: CountsResponse,
    spot_status: ServerStatus,
    change_sequence: u64,
}
#[derive(Deserialize)]
struct VoteReceiptResponse {
    operation_id: String,
    applied_revision: u64,
    revision: u64,
    vote: Option<cutout_ride_maps::ChargingVote>,
    votes: CountsResponse,
    #[allow(dead_code, reason = "public vote receipts preserve local status")]
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
    vote: Option<cutout_ride_maps::ChargingVote>,
}

fn json_body<T: Serialize>(value: &T) -> Result<Vec<u8>, SyncError> {
    serde_json::to_vec(value).map_err(|_| SyncError::StorageFailure)
}
fn server_status(value: ServerStatus) -> persistence::ChargingStatus {
    match value {
        ServerStatus::Published => persistence::ChargingStatus::Published,
        ServerStatus::Hidden => persistence::ChargingStatus::Hidden,
        ServerStatus::Withdrawn => persistence::ChargingStatus::Withdrawn,
        ServerStatus::Removed => persistence::ChargingStatus::Removed,
    }
}
fn server_input(value: SpotInputResponse) -> Result<cutout_ride_maps::NewChargingSpot, SyncError> {
    let coordinate = cutout_ride_maps::Coordinate::from_fixed_parts(
        value.coordinate.latitude_e7,
        value.coordinate.longitude_e7,
    )
    .map_err(|_| SyncError::Network)?;
    let details = cutout_ride_maps::ChargingDetails::new(
        value.name.as_deref(),
        value.venue,
        value.hours.as_deref(),
        value.paid_terms,
        value.price,
        value.supplies,
    )
    .map_err(|_| SyncError::Network)?;
    cutout_ride_maps::NewChargingSpot::new(coordinate, &value.access_note, value.cost)
        .map_err(|_| SyncError::Network)?
        .with_details(details)
        .map_err(|_| SyncError::Network)
}
fn public_spot(
    value: PublicSpotResponse,
) -> Result<
    (
        cutout_ride_maps::ChargingSpotId,
        persistence::ChargingDirectoryEntry,
    ),
    SyncError,
> {
    let id = cutout_ride_maps::ChargingSpotId::parse(&value.id).map_err(|_| SyncError::Network)?;
    if value.created_at_ms > value.updated_at_ms {
        return Err(SyncError::Network);
    }
    let entry = persistence::ChargingDirectoryEntry {
        id,
        input: Some(server_input(value.input)?),
        revision: persistence::ChargingRevision::new(value.revision)
            .map_err(|_| SyncError::Network)?,
        change_sequence: persistence::ChargingRevision::new(value.change_sequence)
            .map_err(|_| SyncError::Network)?,
        status: persistence::ChargingStatus::Published,
        verified_at_ms: value.last_verified_at_ms,
        vote: None,
        vote_revision: persistence::ChargingRevision::new(0).map_err(|_| SyncError::Network)?,
        votes_up: value.votes.up,
        votes_down: value.votes.down,
    };
    Ok((id, entry))
}
fn state_entry(value: SpotStateResponse) -> Result<persistence::ChargingDirectoryEntry, SyncError> {
    let id = cutout_ride_maps::ChargingSpotId::parse(&value.id).map_err(|_| SyncError::Network)?;
    let status = server_status(value.status);
    let (input, verified_at_ms, votes_up, votes_down) = if let Some(spot) = value.spot {
        if spot.id != value.id
            || spot.revision != value.revision
            || spot.change_sequence != value.change_sequence
            || status != persistence::ChargingStatus::Published
        {
            return Err(SyncError::Network);
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
            cutout_ride_maps::ChargingSpotId::parse(duplicate).is_ok()
                && status == persistence::ChargingStatus::Removed
        });
        if status == persistence::ChargingStatus::Published
            || (value.duplicate_of.is_some() && !valid_duplicate)
        {
            return Err(SyncError::Network);
        }
        (None, None, 0, 0)
    };
    Ok(persistence::ChargingDirectoryEntry {
        id,
        input,
        revision: persistence::ChargingRevision::new(value.revision)
            .map_err(|_| SyncError::Network)?,
        change_sequence: persistence::ChargingRevision::new(value.change_sequence)
            .map_err(|_| SyncError::Network)?,
        status,
        verified_at_ms,
        vote: None,
        vote_revision: persistence::ChargingRevision::new(0).map_err(|_| SyncError::Network)?,
        votes_up,
        votes_down,
    })
}
fn operation_request(
    operation: &persistence::ChargingOperation,
) -> Result<(Method, String, Vec<u8>), SyncError> {
    use persistence::{ChargingAction, ChargingTarget};
    let operation_id = operation.id.uuid();
    let request = match (&operation.action, &operation.target) {
        (ChargingAction::Create(input), ChargingTarget::Create) => (
            Method::POST,
            "v1/charging-spots".to_owned(),
            json_body(&serde_json::json!({
                "operation_id": operation_id,
                "spot": input.community_submission(),
            }))?,
        ),
        (
            ChargingAction::Edit(input),
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => (
            Method::PUT,
            format!("v1/charging-spots/{}", id.as_uuid()),
            json_body(&serde_json::json!({
                "operation_id": operation_id,
                "expected_revision": expected_revision.get(),
                "spot": input.community_submission(),
            }))?,
        ),
        (
            ChargingAction::Withdraw,
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => (
            Method::DELETE,
            format!("v1/charging-spots/{}", id.as_uuid()),
            json_body(&RevisionRequest {
                operation_id,
                expected_revision: expected_revision.get(),
            })?,
        ),
        (
            ChargingAction::Verify,
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => (
            Method::POST,
            format!("v1/charging-spots/{}/verification", id.as_uuid()),
            json_body(&RevisionRequest {
                operation_id,
                expected_revision: expected_revision.get(),
            })?,
        ),
        (
            ChargingAction::Vote(vote),
            ChargingTarget::Existing {
                id,
                expected_revision,
            },
        ) => (
            Method::PUT,
            format!("v1/charging-spots/{}/vote", id.as_uuid()),
            json_body(&VoteRequest {
                operation_id,
                expected_revision: expected_revision.get(),
                vote: *vote,
            })?,
        ),
        _ => return Err(SyncError::StorageFailure),
    };
    Ok(request)
}

fn map_transport(error: ChargingTransportError) -> SyncError {
    match error {
        ChargingTransportError::Network => SyncError::Network,
        ChargingTransportError::Unauthorized => SyncError::Unauthorized,
        ChargingTransportError::Conflict => SyncError::Conflict,
        ChargingTransportError::Rejected => SyncError::Rejected,
        ChargingTransportError::Throttled => SyncError::Throttled,
    }
}
fn map_storage(error: &persistence::StorageError) -> SyncError {
    match error {
        persistence::StorageError::ChargingConflict => SyncError::Conflict,
        persistence::StorageError::CommunityAccountUnavailable => SyncError::Unauthorized,
        persistence::StorageError::QueueFull
        | persistence::StorageError::WorkerStopped
        | persistence::StorageError::ResponseDropped
        | persistence::StorageError::DeadlineExceeded
        | persistence::StorageError::SpatialCapabilityUnavailable => SyncError::Unavailable,
        _ => SyncError::StorageFailure,
    }
}
fn is_terminal_block(outcome: SyncOutcome) -> bool {
    match outcome {
        SyncOutcome::MoreWork => false,
        SyncOutcome::Finished { result } => {
            result == SyncResult::Conflict || result == SyncResult::Rejected
        }
    }
}
