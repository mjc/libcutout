use super::MobileChargingError;
use super::sync::ChargingHttpClient;
use crate::{RideDatabaseHandle, persistence};
use reqwest::Method;
use serde::Deserialize;
use std::time::Duration;
use std::{future::Future, pin::Pin};

/// Confirmed service deletion, returned only after the local queue fence is settled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileCommunityAccountDeletionReceiptDto {
    /// The service's durable completion timestamp.
    pub completed_at_milliseconds: u64,
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

async fn delete_account(
    client: &ChargingHttpClient,
    local: persistence::RideDatabase,
    lock: &tokio::sync::Mutex<()>,
    token: &str,
) -> Result<MobileCommunityAccountDeletionReceiptDto, MobileChargingError> {
    let port = MobileAccountDeletionPort {
        client,
        local,
        lock,
        guard: std::sync::Mutex::new(None),
    };
    cutout_application::charging::account::delete_community_account(&port, token)
        .await
        .map(
            |completed_at_milliseconds| MobileCommunityAccountDeletionReceiptDto {
                completed_at_milliseconds,
            },
        )
        .map_err(map_deletion_error)
}

struct MobileAccountDeletionPort<'a> {
    client: &'a ChargingHttpClient,
    local: persistence::RideDatabase,
    lock: &'a tokio::sync::Mutex<()>,
    guard: std::sync::Mutex<Option<tokio::sync::MutexGuard<'a, ()>>>,
}

impl cutout_application::charging::account::AccountDeletionPort for MobileAccountDeletionPort<'_> {
    fn begin_deletion(
        &self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        persistence::CommunityAccountState,
                        cutout_application::charging::account::AccountStorageError,
                    >,
                > + Send
                + '_,
        >,
    > {
        let local = self.local.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || local.begin_community_account_deletion())
                .await
                .map_err(|_| {
                    cutout_application::charging::account::AccountStorageError::WorkerStopped
                })?
                .map_err(cutout_application::charging::account::AccountStorageError::Storage)
        })
    }

    fn delete_remote_exclusive<'a>(
        &'a self,
        token: &'a str,
        timeout: Duration,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<u64, cutout_application::charging::account::AccountRemoteError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let guard = tokio::time::timeout(timeout, self.lock.lock())
                .await
                .map_err(|_| cutout_application::charging::account::AccountRemoteError::Network)?;
            *self
                .guard
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(guard);
            let receipt: DeletionReceipt = self
                .client
                .request(Method::DELETE, "v1/identities/current", Some(token), None)
                .await
                .map_err(|error| match error {
                    MobileChargingError::Unauthorized => {
                        cutout_application::charging::account::AccountRemoteError::Unauthorized
                    }
                    MobileChargingError::Conflict => {
                        cutout_application::charging::account::AccountRemoteError::Conflict
                    }
                    MobileChargingError::Rejected => {
                        cutout_application::charging::account::AccountRemoteError::Rejected
                    }
                    MobileChargingError::Throttled => {
                        cutout_application::charging::account::AccountRemoteError::Throttled
                    }
                    _ => cutout_application::charging::account::AccountRemoteError::Network,
                })?;
            match receipt.status {
                DeletionStatus::Completed => {}
            }
            Ok(receipt.completed_at_ms)
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
                        cutout_application::charging::account::AccountStorageError,
                    >,
                > + Send
                + '_,
        >,
    > {
        let local = self.local.clone();
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || {
                local.complete_community_account_deletion(completed_at_ms)
            })
            .await
            .map_err(|_| cutout_application::charging::account::AccountStorageError::WorkerStopped)?
            .map_err(cutout_application::charging::account::AccountStorageError::Storage);
            self.guard
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            result
        })
    }
}

fn map_deletion_error(
    error: cutout_application::charging::account::AccountDeletionError,
) -> MobileChargingError {
    use cutout_application::charging::account::{
        AccountDeletionError as Error, AccountRemoteError as Remote, AccountStorageError as Storage,
    };
    match error {
        Error::Unauthorized | Error::Remote(Remote::Unauthorized) => {
            MobileChargingError::Unauthorized
        }
        Error::Network | Error::Remote(Remote::Network) => MobileChargingError::Network,
        Error::Storage(Storage::Storage(error)) => super::conversion::storage_error(error),
        Error::Storage(Storage::WorkerStopped) | Error::Unsettled => {
            MobileChargingError::StorageFailure
        }
        Error::Remote(Remote::Conflict) => MobileChargingError::Conflict,
        Error::Remote(Remote::Rejected) => MobileChargingError::Rejected,
        Error::Remote(Remote::Throttled) => MobileChargingError::Throttled,
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RideDatabaseHandle {
    /// Deletes the current service account without deleting local rides or public contributions.
    /// # Errors
    /// Leaves the durable fence pending on network, authentication or storage failure. Keep the
    /// native credential until this returns a confirmed receipt so a lost response can be retried.
    pub async fn delete_community_account(
        &self,
        token: String,
    ) -> Result<MobileCommunityAccountDeletionReceiptDto, MobileChargingError> {
        delete_account(
            &ChargingHttpClient::production()?,
            self.inner.clone(),
            &self.charging_sync_lock,
            &token,
        )
        .await
    }

    /// Exercises the same operation against a local development service, never an arbitrary host.
    /// # Errors
    /// Rejects origins that are not HTTP loopback, or returns the normal deletion failure.
    pub async fn delete_community_account_at_loopback(
        &self,
        token: String,
        base_url: String,
    ) -> Result<MobileCommunityAccountDeletionReceiptDto, MobileChargingError> {
        delete_account(
            &ChargingHttpClient::loopback(&base_url)?,
            self.inner.clone(),
            &self.charging_sync_lock,
            &token,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        extract::State,
        http::{Request, StatusCode},
        middleware::{self, Next},
        response::{IntoResponse, Response},
        routing::delete,
    };
    use serde_json::{Value, json};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use uuid::Uuid;

    async fn serve(
        router: Router,
    ) -> Result<(String, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await;
        });
        Ok((format!("http://{address}/"), task))
    }

    async fn lose_first_receipt(
        State(lost): State<Arc<AtomicBool>>,
        request: Request<axum::body::Body>,
        next: Next,
    ) -> Response {
        let is_deletion =
            request.method() == Method::DELETE && request.uri().path() == "/v1/identities/current";
        let response = next.run(request).await;
        if is_deletion && response.status() == StatusCode::OK && !lost.swap(true, Ordering::SeqCst)
        {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else {
            response
        }
    }

    fn saved_ride(
        local: &persistence::RideDatabase,
    ) -> Result<persistence::RideId, persistence::StorageError> {
        let ride = local.create_ride(persistence::RideSource::Live, 1_000)?;
        local.transition_at(ride, crate::ride_maps::RideEvent::Start, 1_000)?;
        local.transition_at(ride, crate::ride_maps::RideEvent::Stop, 2_000)?;
        local.transition_at(ride, crate::ride_maps::RideEvent::Save, 2_000)?;
        assert!(local.find_ride(ride)?.is_some());
        Ok(ride)
    }

    async fn contribute(
        transport: &ChargingHttpClient,
    ) -> Result<(Value, Value), Box<dyn std::error::Error>> {
        let identity: Value = transport
            .request(Method::POST, "v1/identities", None, None)
            .await?;
        let token = identity["token"].as_str().ok_or("missing token")?;
        let body = serde_json::to_vec(&json!({
            "operation_id": Uuid::new_v4(), "spot": {
                "coordinate":{"latitude_e7":397_392_000,"longitude_e7":-1_049_903_000},
                "access_note":"Outside outlet","cost":"free","name":null,"venue":null,
                "hours":null,"paid_terms":null,"price":null,"supplies":[]
            }
        }))?;
        let public: Value = transport
            .request(Method::POST, "v1/charging-spots", Some(token), Some(body))
            .await?;
        Ok((identity, public))
    }

    #[tokio::test]
    async fn community_account_deletion_reconciles_lost_reply_and_preserves_public_and_local_data()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory =
            std::env::temp_dir().join(format!("cutout-account-deletion-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        let server_path = directory.join("server.sqlite");
        let local_path = directory.join("local.sqlite");
        let store = cutout_server::Store::open(&server_path)?;
        let lost = Arc::new(AtomicBool::new(false));
        let app = cutout_server::router(store)
            .layer(middleware::from_fn_with_state(lost, lose_first_receipt));
        let (origin, task) = serve(app).await?;
        let transport = ChargingHttpClient::loopback(&origin)?;
        let (identity, public) = contribute(&transport).await?;
        let token = identity["token"].as_str().ok_or("missing token")?;
        let public_id = public["state"]["id"].as_str().ok_or("missing spot")?;
        let local = persistence::RideDatabase::open(&local_path)?;
        let ride = saved_ride(&local)?;
        let point = local.save_charging_spot(crate::ride_maps::NewChargingSpot::new(
            crate::ride_maps::Coordinate::from_degrees(39.7, -104.9)?,
            "Local unsubmitted draft",
            crate::ride_maps::ChargingCost::Free,
        )?)?;
        let lock = tokio::sync::Mutex::new(());
        assert_eq!(
            delete_account(&transport, local.clone(), &lock, token).await,
            Err(MobileChargingError::Network)
        );
        assert_eq!(
            local.community_account_state()?,
            persistence::CommunityAccountState::Deleting
        );
        assert!(local.next_charging_operation().is_err());
        assert!(local.find_ride(ride)?.is_some());
        let completed = delete_account(&transport, local.clone(), &lock, token).await?;
        assert!(completed.completed_at_milliseconds > 0);
        assert_eq!(local.charging_outbox_count()?, 0);
        assert!(local.charging_spot(point.point_id)?.is_some());
        assert!(local.find_ride(ride)?.is_some());
        let retained: Value = transport
            .request(
                Method::GET,
                &format!("v1/charging-spots/{public_id}"),
                None,
                None,
            )
            .await?;
        assert_eq!(retained["access_note"], "Outside outlet");
        assert_eq!(
            transport
                .request::<Value>(
                    Method::GET,
                    &format!("v1/charging-spots/{public_id}/vote"),
                    Some(token),
                    None
                )
                .await,
            Err(MobileChargingError::Unauthorized)
        );
        let connection = rusqlite::Connection::open(&server_path)?;
        let owner: String = connection.query_row(
            "SELECT owner_id FROM spots WHERE id=?1",
            [public_id],
            |row| row.get(0),
        )?;
        assert_eq!(owner, "00000000-0000-4000-8000-000000000001");
        assert_ne!(owner, identity["id"].as_str().ok_or("missing identity")?);
        drop(connection);
        local.shutdown()?;
        let reopened = persistence::RideDatabase::open(&local_path)?;
        assert_eq!(
            reopened.community_account_state()?,
            persistence::CommunityAccountState::Deleted {
                completed_at_milliseconds: completed.completed_at_milliseconds
            }
        );
        assert_eq!(
            delete_account(&transport, reopened.clone(), &lock, token).await?,
            completed
        );
        assert!(reopened.find_ride(ride)?.is_some());
        reopened.shutdown()?;
        task.abort();
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[tokio::test]
    async fn community_account_deletion_offline_keeps_a_retryable_fence()
    -> Result<(), Box<dyn std::error::Error>> {
        let path =
            std::env::temp_dir().join(format!("cutout-account-offline-{}.sqlite", Uuid::new_v4()));
        let local = persistence::RideDatabase::open(&path)?;
        let (origin, task) = serve(Router::new().route(
            "/v1/identities/current",
            delete(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        ))
        .await?;
        let lock = tokio::sync::Mutex::new(());
        let result = delete_account(
            &ChargingHttpClient::loopback(&origin)?,
            local.clone(),
            &lock,
            &"c".repeat(64),
        )
        .await;
        assert_eq!(result, Err(MobileChargingError::Network));
        assert_eq!(
            local.community_account_state()?,
            persistence::CommunityAccountState::Deleting
        );
        local.shutdown()?;
        let reopened = persistence::RideDatabase::open(&path)?;
        assert_eq!(
            reopened.community_account_state()?,
            persistence::CommunityAccountState::Deleting
        );
        reopened.shutdown()?;
        task.abort();
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn community_account_test_transport_rejects_remote_and_decorated_origins() {
        for origin in [
            "http://example.com/",
            "https://127.0.0.1/",
            "http://user@127.0.0.1/",
            "http://127.0.0.1/?token=secret",
            "http://127.0.0.1/path/",
            "http://127.0.0.1/#fragment",
        ] {
            assert!(ChargingHttpClient::loopback(origin).is_err());
        }
    }
}
