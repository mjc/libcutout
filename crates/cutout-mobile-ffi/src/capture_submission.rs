//! Explicit private diagnostic Send through Rust's retained snapshot owner.

mod http;

use crate::{RideDatabaseHandle, persistence};
use cutout_core::{CaptureSubmissionIntegrity, CaptureSubmissionReceipt};
use http::HttpCaptureTransport;
use std::sync::Arc;

/// A retained capture identity, never a foreign path or eligibility decision.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionSourceDto {
    /// Canonical SQLite capture UUID from durable writer completion or history.
    LiveCapture { id: String },
    /// Consumed writer's retained artifact UUID, independent of its file path.
    RecordedArtifact { id: String },
}

impl TryFrom<MobileCaptureSubmissionSourceDto> for persistence::CaptureSubmissionSource {
    type Error = MobileCaptureSubmissionError;

    fn try_from(source: MobileCaptureSubmissionSourceDto) -> Result<Self, Self::Error> {
        match source {
            MobileCaptureSubmissionSourceDto::LiveCapture { id } => {
                persistence::LiveCaptureId::parse(&id)
                    .map(Self::LiveCapture)
                    .map_err(|_| MobileCaptureSubmissionError::InvalidSource)
            }
            MobileCaptureSubmissionSourceDto::RecordedArtifact { id } => {
                persistence::CaptureArtifactId::parse(&id)
                    .map(Self::RecordedArtifact)
                    .map_err(|_| MobileCaptureSubmissionError::InvalidSource)
            }
        }
    }
}

/// Native presentation of the persisted operation; it does not authorize a transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionPhaseDto {
    /// Local immutable snapshot, with no Send effect.
    Prepared,
    /// A user-authorized transfer has not received a durable receipt yet.
    Sending,
    /// Rust retained a classified transfer failure.
    Failed,
    /// The service rejected the immutable snapshot; this operation cannot be retried.
    Rejected,
    /// A matching durable service receipt has been committed locally.
    Sent,
    /// Remote cancellation has not been confirmed.
    Cancelling,
    /// Cancellation has been confirmed, preserving the original recording.
    Cancelled,
    /// Remote deletion has not been confirmed.
    Deleting,
    /// Deletion has been confirmed, preserving the original recording.
    Deleted,
}

impl From<persistence::CaptureSubmissionPhase> for MobileCaptureSubmissionPhaseDto {
    fn from(phase: persistence::CaptureSubmissionPhase) -> Self {
        use persistence::CaptureSubmissionPhase as Phase;
        match phase {
            Phase::Prepared => Self::Prepared,
            Phase::Sending => Self::Sending,
            Phase::Retryable => Self::Failed,
            Phase::Rejected => Self::Rejected,
            Phase::Sent => Self::Sent,
            Phase::CancellationPending => Self::Cancelling,
            Phase::Cancelled => Self::Cancelled,
            Phase::DeletionPending => Self::Deleting,
            Phase::Deleted => Self::Deleted,
        }
    }
}

/// Categories found in the actual validated snapshot for the Send disclosure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent payload categories can be present in any combination"
)]
pub struct MobileCaptureSubmissionCategoriesDto {
    /// Known device/platform identifiers are included; raw BLE can contain others.
    pub device_identifiers: bool,
    /// Location observations are included.
    pub location: bool,
    /// User descriptions or notes are included.
    pub notes: bool,
    /// Accepted annotations or physical-action label boundaries are included.
    pub annotations: bool,
}

impl From<cutout_core::CaptureSubmissionCategories> for MobileCaptureSubmissionCategoriesDto {
    fn from(categories: cutout_core::CaptureSubmissionCategories) -> Self {
        Self {
            device_identifiers: categories.device_identifiers,
            location: categories.location,
            notes: categories.notes,
            annotations: categories.annotations,
        }
    }
}

/// A receipt already validated and committed by the Rust operation owner.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionReceiptDto {
    /// Retry-stable operation UUID.
    pub operation_id: String,
    /// Digest of the exact submitted snapshot.
    pub sha256: String,
    /// Durable remote byte length.
    pub bytes: u64,
    /// Service commit time in Unix milliseconds.
    pub received_at_milliseconds: u64,
}

impl From<CaptureSubmissionReceipt> for MobileCaptureSubmissionReceiptDto {
    fn from(receipt: CaptureSubmissionReceipt) -> Self {
        Self {
            operation_id: receipt.operation_id,
            sha256: receipt.sha256,
            bytes: receipt.bytes,
            received_at_milliseconds: receipt.received_at_milliseconds,
        }
    }
}

/// Bounded failure classification, without service body, payload or credentials.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileCaptureSubmissionFailureDto {
    /// No complete network result was obtained.
    Network,
    /// The service could not authenticate the retained credential.
    Unauthorized,
    /// The authenticated account cannot access this operation.
    Forbidden,
    /// The operation or receipt conflicts with the fixed snapshot.
    Conflict,
    /// The service rejected the request.
    Rejected,
    /// The request budget was exhausted.
    Throttled,
    /// The service could not complete the request.
    Unavailable,
    /// The payload exceeded a service resource limit.
    TooLarge,
    /// The payload failed capture validation.
    InvalidCapture,
    /// The payload contains excluded music metadata.
    MusicPresent,
    /// Private capture storage is full.
    QuotaExceeded,
    /// The remote operation was already cancelled.
    Cancelled,
}

impl From<persistence::CaptureSubmissionTransferError> for MobileCaptureSubmissionFailureDto {
    fn from(failure: persistence::CaptureSubmissionTransferError) -> Self {
        use persistence::CaptureSubmissionTransferError as Failure;
        match failure {
            Failure::Network => Self::Network,
            Failure::Unauthorized => Self::Unauthorized,
            Failure::Forbidden => Self::Forbidden,
            Failure::Conflict => Self::Conflict,
            Failure::Rejected => Self::Rejected,
            Failure::Throttled => Self::Throttled,
            Failure::Unavailable => Self::Unavailable,
            Failure::TooLarge => Self::TooLarge,
            Failure::InvalidCapture => Self::InvalidCapture,
            Failure::MusicPresent => Self::MusicPresent,
            Failure::QuotaExceeded => Self::QuotaExceeded,
            Failure::Cancelled => Self::Cancelled,
        }
    }
}

/// Current Rust-owned operation, safe to display after relaunch without sending.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCaptureSubmissionStatusDto {
    /// Stable identity for this selected snapshot and every explicit retry.
    pub operation_id: String,
    /// Current persisted phase, including unconfirmed remote removal.
    pub phase: MobileCaptureSubmissionPhaseDto,
    /// Actual included categories, independent of native presentation flags.
    pub categories: MobileCaptureSubmissionCategoriesDto,
    /// Known observation loss disclosed with incomplete durable recordings.
    pub dropped_messages: u64,
    /// Exact immutable submitted byte count.
    pub byte_count: u64,
    /// Matching durable receipt, when one has been committed.
    pub receipt: Option<MobileCaptureSubmissionReceiptDto>,
    /// Last classified failure without raw response or evidence bytes.
    pub failure: Option<MobileCaptureSubmissionFailureDto>,
}

impl From<persistence::CaptureSubmissionStatus> for MobileCaptureSubmissionStatusDto {
    fn from(status: persistence::CaptureSubmissionStatus) -> Self {
        let dropped_messages = match status.metadata.integrity {
            CaptureSubmissionIntegrity::Complete => 0,
            CaptureSubmissionIntegrity::Incomplete { dropped_messages } => dropped_messages,
        };
        Self {
            operation_id: status.operation_id,
            phase: status.phase.into(),
            categories: status.categories.into(),
            dropped_messages,
            byte_count: status.metadata.bytes,
            receipt: status.receipt.map(Into::into),
            failure: status.failure.map(Into::into),
        }
    }
}

/// Admission or local storage failure; transfer failures remain in persisted status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, uniffi::Error)]
pub enum MobileCaptureSubmissionError {
    /// The supplied source is absent or is not a capture UUID.
    #[error("capture source is invalid")]
    InvalidSource,
    /// The Rust owner has no valid durable completion for this action.
    #[error("capture is not ready for private submission")]
    NotReady,
    /// The fixed service transport could not be configured.
    #[error("capture submission service is unavailable")]
    Unavailable,
    /// The operation cannot use the provided account/credential.
    #[error("capture submission requires its existing account")]
    Unauthorized,
    /// The request cannot currently fit the service's rate budget.
    #[error("capture submission request budget is exhausted")]
    RateLimited,
    /// The service has no remaining private capture storage.
    #[error("private capture storage quota is exhausted")]
    QuotaExceeded,
    /// The snapshot failed its full-stream validation.
    #[error("capture submission is invalid")]
    InvalidCapture,
    /// Local persistence or its worker could not complete the operation.
    #[error("capture submission storage failed")]
    StorageFailure,
    /// Cancellation prevented completion.
    #[error("capture submission was cancelled")]
    Cancelled,
    /// Receipt identity did not match the selected immutable snapshot.
    #[error("capture submission receipt did not match")]
    ReceiptMismatch,
}

impl From<persistence::CaptureSubmissionError> for MobileCaptureSubmissionError {
    fn from(error: persistence::CaptureSubmissionError) -> Self {
        use persistence::CaptureSubmissionError as Error;
        match error {
            Error::NotFound => Self::InvalidSource,
            Error::NotFinished | Error::ProvenanceUnavailable | Error::InvalidState => {
                Self::NotReady
            }
            Error::InvalidCapture => Self::InvalidCapture,
            Error::WrongAccount => Self::Unauthorized,
            Error::Storage(_) => Self::StorageFailure,
        }
    }
}

/// Executes explicit effects for one caller-selected Rust source/operation.
/// Constructing or reading this handle never starts a transfer.
#[derive(uniffi::Object)]
pub struct CaptureSubmissionClientHandle {
    database: persistence::RideDatabase,
    transport: HttpCaptureTransport,
}

impl std::fmt::Debug for CaptureSubmissionClientHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CaptureSubmissionClientHandle")
            .finish_non_exhaustive()
    }
}

#[uniffi::export(async_runtime = "tokio")]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary handles and strings"
)]
impl CaptureSubmissionClientHandle {
    /// Uses the existing database and the fixed HTTPS service, without source paths.
    /// # Errors
    /// Returns `Unavailable` when the bounded HTTP client cannot be configured.
    #[uniffi::constructor]
    pub fn new(
        database: Arc<RideDatabaseHandle>,
    ) -> Result<Arc<Self>, MobileCaptureSubmissionError> {
        let transport = HttpCaptureTransport::production()
            .map_err(|_| MobileCaptureSubmissionError::Unavailable)?;
        Ok(Arc::new(Self {
            database: database.inner.clone(),
            transport,
        }))
    }

    /// Reads only persisted local status. Relaunch does not resume an upload.
    /// # Errors
    /// Returns an invalid source or typed local storage failure.
    pub async fn status(
        &self,
        source: MobileCaptureSubmissionSourceDto,
    ) -> Result<Option<MobileCaptureSubmissionStatusDto>, MobileCaptureSubmissionError> {
        let source = source.try_into()?;
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.capture_submission(source))
            .await
            .map_err(|_| MobileCaptureSubmissionError::StorageFailure)?
            .map(|status| status.map(Into::into))
            .map_err(Into::into)
    }

    /// Freezes the Rust-approved immutable snapshot locally for disclosure.
    /// # Errors
    /// Rejects unfinalized, unproven or invalid evidence without transmitting anything.
    pub async fn prepare(
        &self,
        source: MobileCaptureSubmissionSourceDto,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        let source = source.try_into()?;
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.prepare_capture_submission(source))
            .await
            .map_err(|_| MobileCaptureSubmissionError::StorageFailure)?
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Runs exactly the user-confirmed operation, including receipt recovery for a retry.
    /// # Errors
    /// Returns typed admission/storage errors; bounded HTTP failures persist in status.
    pub async fn send(
        &self,
        operation_id: String,
        account_id: String,
        token: String,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        self.database
            .send_capture_submission(&operation_id, &account_id, &token, &self.transport)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Cancels the chosen operation; local prepared cancellation needs no account.
    /// # Errors
    /// Unconfirmed remote cancellation remains pending; partial credentials are refused.
    pub async fn cancel(
        &self,
        operation_id: String,
        account_id: Option<String>,
        token: Option<String>,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        let credential = match (account_id.as_deref(), token.as_deref()) {
            (Some(account_id), Some(token)) => {
                Some(persistence::CaptureSubmissionCredential { account_id, token })
            }
            (None, None) => None,
            _ => return Err(MobileCaptureSubmissionError::Unauthorized),
        };
        self.database
            .cancel_capture_submission(&operation_id, credential, &self.transport)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Deletes only the remote copy using the retained operation and existing account.
    /// # Errors
    /// Unconfirmed deletion remains pending and preserves the local original.
    pub async fn delete(
        &self,
        operation_id: String,
        account_id: String,
        token: String,
    ) -> Result<MobileCaptureSubmissionStatusDto, MobileCaptureSubmissionError> {
        self.database
            .delete_capture_submission(&operation_id, &account_id, &token, &self.transport)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutout_core::{
        CaptureOrigin, GattChannel, MonotonicTimestamp, PevcapRecord, WallClockUnixTimestamp,
    };
    use persistence::{CaptureMetadata, CaptureWriteOutcome, CaptureWriter, CaptureWriterFinish};
    use serde_json::Value;
    use std::{
        net::{Ipv4Addr, SocketAddr},
        path::{Path, PathBuf},
    };
    use uuid::Uuid;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Result<Self, std::io::Error> {
            let path =
                std::env::temp_dir().join(format!("cutout-capture-submission-{}", Uuid::new_v4()));
            std::fs::create_dir(&path)?;
            Ok(Self(path.canonicalize()?))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn finished_capture(
        directory: &Path,
    ) -> Result<
        (Arc<RideDatabaseHandle>, MobileCaptureSubmissionSourceDto),
        Box<dyn std::error::Error>,
    > {
        let database = crate::open_ride_database(
            directory
                .join("local.sqlite")
                .to_string_lossy()
                .into_owned(),
        )?;
        let writer = CaptureWriter::start_with_database(
            directory.join("private-original.jsonl"),
            WallClockUnixTimestamp::new(1_700_000_000_000),
            "unsupported-unverified-device",
            None,
            &CaptureMetadata {
                advertised_services: vec![],
                gatt_fingerprints: vec![],
                resolved_identity: None,
                annotations: vec!["capture_label=lamp_button_pressed".into()],
            },
            database.inner.clone(),
        )?;
        assert_eq!(
            writer.try_send_record(PevcapRecord::inbound_notification(
                MonotonicTimestamp::new(1),
                GattChannel::from_bytes([0x11; 16]),
                GattChannel::from_bytes([0x22; 16]),
                vec![0xff, 0x00, 0xa5],
            )),
            CaptureWriteOutcome::Accepted
        );
        let CaptureWriterFinish::DatabaseFinished {
            live_capture_id,
            receipt,
            ..
        } = writer.finish_without_export()?
        else {
            return Err("expected durable database completion".into());
        };
        database.inner.retain_finished_database_capture(
            &receipt,
            CaptureOrigin::Manual,
            Some("Unverified advertisement"),
            WallClockUnixTimestamp::new(1_700_000_001_000),
        )?;
        Ok((
            database,
            MobileCaptureSubmissionSourceDto::LiveCapture {
                id: live_capture_id.to_string(),
            },
        ))
    }

    #[test]
    fn source_requires_a_retained_uuid_instead_of_accepting_a_path() {
        for source in [
            MobileCaptureSubmissionSourceDto::LiveCapture {
                id: "/tmp/capture.jsonl".into(),
            },
            MobileCaptureSubmissionSourceDto::RecordedArtifact {
                id: "not-a-writer-receipt".into(),
            },
        ] {
            assert_eq!(
                persistence::CaptureSubmissionSource::try_from(source),
                Err(MobileCaptureSubmissionError::InvalidSource)
            );
        }
        let id = Uuid::new_v4().to_string();
        assert!(
            persistence::CaptureSubmissionSource::try_from(
                MobileCaptureSubmissionSourceDto::LiveCapture { id: id.clone() }
            )
            .is_ok()
        );
        assert!(
            persistence::CaptureSubmissionSource::try_from(
                MobileCaptureSubmissionSourceDto::RecordedArtifact { id }
            )
            .is_ok()
        );
    }

    #[test]
    fn status_projection_preserves_actual_loss_categories_and_pending_removal()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let mut status = database
            .inner
            .prepare_capture_submission(source.try_into()?)?;
        let operation = status.operation_id.clone();
        let byte_count = status.metadata.bytes;
        status.metadata.integrity = CaptureSubmissionIntegrity::Incomplete {
            dropped_messages: 7,
        };
        status.phase = persistence::CaptureSubmissionPhase::DeletionPending;
        status.failure = Some(persistence::CaptureSubmissionTransferError::Unavailable);
        let projected: MobileCaptureSubmissionStatusDto = status.into();
        assert_eq!(projected.operation_id, operation);
        assert_eq!(projected.byte_count, byte_count);
        assert_eq!(projected.dropped_messages, 7);
        assert!(projected.categories.device_identifiers);
        assert!(!projected.categories.location);
        assert!(projected.categories.annotations);
        assert_eq!(projected.phase, MobileCaptureSubmissionPhaseDto::Deleting);
        assert_eq!(
            projected.failure,
            Some(MobileCaptureSubmissionFailureDto::Unavailable)
        );
        assert!(projected.receipt.is_none());
        assert_eq!(
            MobileCaptureSubmissionPhaseDto::from(persistence::CaptureSubmissionPhase::Rejected),
            MobileCaptureSubmissionPhaseDto::Rejected
        );
        assert_eq!(
            MobileCaptureSubmissionPhaseDto::from(persistence::CaptureSubmissionPhase::Retryable),
            MobileCaptureSubmissionPhaseDto::Failed
        );
        assert_eq!(
            MobileCaptureSubmissionPhaseDto::from(
                persistence::CaptureSubmissionPhase::CancellationPending
            ),
            MobileCaptureSubmissionPhaseDto::Cancelling
        );
        database.inner.clone().shutdown()?;
        Ok(())
    }

    #[tokio::test]
    async fn persisted_service_rejection_is_projected_without_retry_admission()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct RejectingTransport {
            uploads: AtomicUsize,
        }

        impl persistence::CaptureSubmissionTransport for RejectingTransport {
            fn upload<'a>(
                &'a self,
                _operation_id: &'a str,
                _token: &'a str,
                _metadata: &'a cutout_core::CaptureSubmissionMetadata,
                _payload: persistence::CaptureSubmissionPayload,
            ) -> persistence::CaptureSubmissionFuture<'a, CaptureSubmissionReceipt> {
                self.uploads.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Err(persistence::CaptureSubmissionTransferError::Forbidden) })
            }

            fn receipt<'a>(
                &'a self,
                _operation_id: &'a str,
                _token: &'a str,
            ) -> persistence::CaptureSubmissionFuture<'a, Option<CaptureSubmissionReceipt>>
            {
                Box::pin(async { Ok(None) })
            }

            fn delete<'a>(
                &'a self,
                _operation_id: &'a str,
                _token: &'a str,
            ) -> persistence::CaptureSubmissionFuture<'a, ()> {
                Box::pin(async { Err(persistence::CaptureSubmissionTransferError::Unavailable) })
            }
        }

        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let transport = RejectingTransport {
            uploads: AtomicUsize::new(0),
        };
        let prepared = database
            .inner
            .prepare_capture_submission(source.clone().try_into()?)?;
        let account_id = Uuid::new_v4().to_string();
        let rejected = database
            .inner
            .send_capture_submission(
                &prepared.operation_id,
                &account_id,
                "test-token",
                &transport,
            )
            .await?;
        assert_eq!(
            rejected.phase,
            persistence::CaptureSubmissionPhase::Rejected
        );
        let client = CaptureSubmissionClientHandle {
            database: database.inner.clone(),
            transport: HttpCaptureTransport::loopback("http://127.0.0.1:1/")?,
        };
        let projected = client.status(source).await?.ok_or("status missing")?;
        assert_eq!(projected.phase, MobileCaptureSubmissionPhaseDto::Rejected);
        assert_eq!(
            projected.failure,
            Some(MobileCaptureSubmissionFailureDto::Forbidden)
        );
        assert!(projected.receipt.is_none());
        let retry = database
            .inner
            .send_capture_submission(
                &prepared.operation_id,
                &account_id,
                "test-token",
                &transport,
            )
            .await
            .map(MobileCaptureSubmissionStatusDto::from)
            .map_err(MobileCaptureSubmissionError::from);
        assert_eq!(retry, Err(MobileCaptureSubmissionError::NotReady));
        assert_eq!(transport.uploads.load(Ordering::Relaxed), 1);
        database.inner.clone().shutdown()?;
        Ok(())
    }

    #[tokio::test]
    async fn prepared_capture_can_be_cancelled_locally_without_credentials()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let client = CaptureSubmissionClientHandle {
            database: database.inner.clone(),
            transport: HttpCaptureTransport::loopback("http://127.0.0.1:1/")?,
        };
        assert!(client.status(source.clone()).await?.is_none());
        let prepared = client.prepare(source.clone()).await?;
        assert_eq!(prepared.phase, MobileCaptureSubmissionPhaseDto::Prepared);
        assert_eq!(
            client
                .cancel(
                    prepared.operation_id.clone(),
                    Some(Uuid::new_v4().to_string()),
                    None
                )
                .await,
            Err(MobileCaptureSubmissionError::Unauthorized)
        );
        let cancelled = client.cancel(prepared.operation_id, None, None).await?;
        assert_eq!(cancelled.phase, MobileCaptureSubmissionPhaseDto::Cancelled);
        assert!(cancelled.receipt.is_none());
        assert_eq!(client.status(source).await?, Some(cancelled));
        database.inner.clone().shutdown()?;
        Ok(())
    }

    #[tokio::test]
    async fn ffi_send_receives_an_authenticated_server_receipt_then_deletes_only_the_remote_copy()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = TestDirectory::new()?;
        let (database, source) = finished_capture(&directory.0)?;
        let store = cutout_server::Store::open(&directory.0.join("server.sqlite"))?;
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let base_url = format!("http://{}/", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                cutout_server::router(store).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        let http = reqwest::Client::builder().no_proxy().build()?;
        let identity: Value = http
            .post(format!("{base_url}v1/identities"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let account_id = identity["id"]
            .as_str()
            .ok_or("identity id missing")?
            .to_owned();
        let token = identity["token"]
            .as_str()
            .ok_or("identity token missing")?
            .to_owned();
        let client = CaptureSubmissionClientHandle {
            database: database.inner.clone(),
            transport: HttpCaptureTransport::loopback(&base_url)?,
        };
        assert!(client.status(source.clone()).await?.is_none());
        let prepared = client.prepare(source.clone()).await?;
        assert!(prepared.receipt.is_none());
        assert!(!prepared.categories.location);
        let sent = client
            .send(
                prepared.operation_id.clone(),
                account_id.clone(),
                token.clone(),
            )
            .await?;
        assert_eq!(sent.phase, MobileCaptureSubmissionPhaseDto::Sent);
        let receipt = sent.receipt.clone().ok_or("durable receipt missing")?;
        assert_eq!(receipt.operation_id, prepared.operation_id);
        assert_eq!(receipt.bytes, prepared.byte_count);
        assert_eq!(
            client
                .send(
                    prepared.operation_id.clone(),
                    account_id.clone(),
                    token.clone()
                )
                .await?,
            sent
        );
        let wrong_account = client
            .delete(
                prepared.operation_id.clone(),
                Uuid::new_v4().to_string(),
                token.clone(),
            )
            .await;
        assert_eq!(
            wrong_account,
            Err(MobileCaptureSubmissionError::Unauthorized)
        );
        let deleted = client
            .delete(prepared.operation_id.clone(), account_id, token.clone())
            .await?;
        assert_eq!(deleted.phase, MobileCaptureSubmissionPhaseDto::Deleted);
        assert_eq!(client.status(source.clone()).await?, Some(deleted));
        let remote = http
            .get(format!(
                "{base_url}v1/capture-submissions/{}",
                prepared.operation_id
            ))
            .bearer_auth(token)
            .send()
            .await?;
        assert_eq!(remote.status(), reqwest::StatusCode::NOT_FOUND);
        let persistence::CaptureSubmissionSource::LiveCapture(id) = source.try_into()? else {
            return Err("expected live source".into());
        };
        let exported = database
            .inner
            .export_live_capture(id, &directory.0.join("after-delete.jsonl"))?;
        assert_eq!(
            exported, receipt.sha256,
            "remote deletion must preserve every local capture byte"
        );
        task.abort();
        database.inner.clone().shutdown()?;
        Ok(())
    }
}
