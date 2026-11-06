use super::MobileChargingError;
use super::sync::ChargingHttpClient;
use cutout_application::charging::{
    reports::{ChargingContentReport, ReportSubmitError},
    sync_runtime::ChargingSyncService,
};
use std::sync::Arc;
use uuid::Uuid;

/// A confirmed private moderation report, separate from public pin votes.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileChargingReportReceiptDto {
    /// Identifier returned by the moderation service.
    pub report_id: String,
}

/// One immutable report operation. Explicit retries reuse its identifier and text.
#[derive(uniffi::Object)]
pub struct MobileChargingContentReport {
    policy: ChargingContentReport,
}

impl std::fmt::Debug for MobileChargingContentReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MobileChargingContentReport")
            .finish_non_exhaustive()
    }
}

#[uniffi::export(async_runtime = "tokio")]
#[allow(
    clippy::needless_pass_by_value,
    reason = "UniFFI owns boundary strings"
)]
impl MobileChargingContentReport {
    /// Prepares an offensive-content report for a published community pin.
    /// # Errors
    /// Rejects invalid identifiers and details over 1,000 UTF-8 bytes or containing controls.
    #[uniffi::constructor]
    pub fn new(spot_id: String, message: String) -> Result<Arc<Self>, MobileChargingError> {
        let spot_id =
            Uuid::parse_str(&spot_id).map_err(|_| MobileChargingError::InvalidIdentifier)?;
        let operation_id = Uuid::new_v4();
        let policy = ChargingContentReport::new(spot_id, operation_id, &message)
            .map_err(|_| MobileChargingError::InvalidMetadata)?;
        Ok(Arc::new(Self { policy }))
    }

    /// Submits this report to the fixed HTTPS service and returns only a validated receipt.
    /// # Errors
    /// Returns authentication, service, or invalid-receipt errors without claiming success.
    pub async fn submit(
        &self,
        token: String,
    ) -> Result<MobileChargingReportReceiptDto, MobileChargingError> {
        self.submit_with(&ChargingHttpClient::production()?, &token)
            .await
    }
}

impl MobileChargingContentReport {
    async fn submit_with(
        &self,
        client: &ChargingHttpClient,
        token: &str,
    ) -> Result<MobileChargingReportReceiptDto, MobileChargingError> {
        ChargingSyncService::submit_report(&client.inner, &self.policy, token)
            .await
            .map(|receipt| MobileChargingReportReceiptDto {
                report_id: receipt.report_id.to_string(),
            })
            .map_err(MobileChargingError::from)
    }
}

impl From<ReportSubmitError> for MobileChargingError {
    fn from(error: ReportSubmitError) -> Self {
        use cutout_application::charging::reports::{
            ReportRemoteError as Remote, ReportSubmitError as Error,
        };
        match error {
            Error::Unauthorized | Error::Remote(Remote::Unauthorized) => {
                MobileChargingError::Unauthorized
            }
            Error::InvalidReceipt | Error::Remote(Remote::Network) => MobileChargingError::Network,
            Error::Remote(Remote::Conflict) => MobileChargingError::Conflict,
            Error::Remote(Remote::Rejected) => MobileChargingError::Rejected,
            Error::Remote(Remote::Throttled) => MobileChargingError::Throttled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        extract::State,
        http::Request,
        middleware::{self, Next},
        response::{IntoResponse, Response},
    };
    use serde_json::{Value, json};
    use std::sync::Mutex;

    #[test]
    fn report_preparation_rejects_invalid_identifiers_and_unbounded_details() {
        assert!(MobileChargingContentReport::new("not-a-pin".into(), String::new()).is_err());
        let id = Uuid::new_v4().to_string();
        assert!(MobileChargingContentReport::new(id.clone(), "é".repeat(501)).is_err());
        assert!(MobileChargingContentReport::new(id.clone(), "text\0private".into()).is_err());
        let report = MobileChargingContentReport::new(id, "  Details\nfor the moderator  ".into())
            .expect("report");
        assert_eq!(report.policy.message(), Some("Details\nfor the moderator"));
    }

    async fn start(
        router: Router,
    ) -> Result<(String, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = format!("http://{}/", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await;
        });
        Ok((address, task))
    }

    async fn lose_first_receipt(
        State(first): State<Arc<Mutex<Option<Value>>>>,
        request: Request<axum::body::Body>,
        next: Next,
    ) -> Response {
        let reporting = request.uri().path().ends_with("/reports");
        let response = next.run(request).await;
        if !reporting {
            return response;
        }
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, 32 * 1024)
            .await
            .expect("small receipt");
        let mut saved = first.lock().expect("receipt lock");
        if saved.is_none() {
            *saved = Some(serde_json::from_slice(&bytes).expect("actual server receipt"));
            reqwest::StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else {
            Response::from_parts(parts, axum::body::Body::from(bytes))
        }
    }

    #[tokio::test]
    async fn lost_report_response_retries_the_same_authenticated_server_operation()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!("cutout-report-{}.sqlite", Uuid::new_v4()));
        let first = Arc::new(Mutex::new(None));
        let router = cutout_server::router(cutout_server::Store::open(&path)?).layer(
            middleware::from_fn_with_state(first.clone(), lose_first_receipt),
        );
        let (base_url, task) = start(router).await?;
        let http = cutout_core::http::client_builder()
            .expect("platform verifier")
            .build()
            .expect("HTTP client");
        let identity: Value = http
            .post(format!("{base_url}v1/identities"))
            .send()
            .await?
            .json()
            .await?;
        let token = identity["token"].as_str().ok_or("missing token")?;
        let created: Value = http
            .post(format!("{base_url}v1/charging-spots"))
            .bearer_auth(token)
            .json(&json!({"operation_id":Uuid::new_v4(),"spot": {
                "coordinate":{"latitude_e7":400_000_000,"longitude_e7":-1_050_000_000},
                "access_note":"Outlet by the patio.","cost":"free"
            }}))
            .send()
            .await?
            .json()
            .await?;
        let id = created["state"]["id"].as_str().ok_or("missing spot")?;
        let report =
            MobileChargingContentReport::new(id.into(), "Offensive text in this pin.".into())?;
        let client = ChargingHttpClient::loopback(&base_url)?;
        assert_eq!(
            report.submit_with(&client, "malformed-token").await,
            Err(MobileChargingError::Unauthorized)
        );
        assert_eq!(
            report.submit_with(&client, token).await,
            Err(MobileChargingError::Network)
        );
        let receipt = report.submit_with(&client, token).await?;
        let original = first
            .lock()
            .expect("receipt lock")
            .clone()
            .ok_or("missing original")?;
        assert_eq!(original["id"], receipt.report_id);
        assert_eq!(
            original["operation_id"],
            report.policy.operation_id().to_string()
        );
        let unauthenticated = "0".repeat(64);
        assert_eq!(
            report.submit_with(&client, &unauthenticated).await,
            Err(MobileChargingError::Unauthorized)
        );
        task.abort();
        Ok(())
    }

    #[tokio::test]
    async fn unrelated_report_receipt_cannot_be_shown_as_sent()
    -> Result<(), Box<dyn std::error::Error>> {
        let router = Router::new().route("/v1/charging-spots/{id}/reports", axum::routing::post(|| async {
            axum::Json(json!({"operation_id":Uuid::new_v4(),"id":Uuid::new_v4(),"applied_revision":1,"revision":1,"status":"open"}))
        }));
        let (base_url, task) = start(router).await?;
        let report = MobileChargingContentReport::new(Uuid::new_v4().to_string(), String::new())?;
        assert_eq!(
            report
                .submit_with(&ChargingHttpClient::loopback(&base_url)?, &"1".repeat(64))
                .await,
            Err(MobileChargingError::Network)
        );
        task.abort();
        Ok(())
    }
}
