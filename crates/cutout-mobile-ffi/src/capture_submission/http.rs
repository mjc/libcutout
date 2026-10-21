//! Bounded HTTP effects for Rust's retained private capture snapshot.

use crate::persistence::{
    CaptureSubmissionFuture, CaptureSubmissionPayload, CaptureSubmissionTransferError,
    CaptureSubmissionTransport,
};
use cutout_core::{CaptureSubmissionMetadata, CaptureSubmissionReceipt, PevcapEncoding};
use reqwest::{Client, Method, Response, StatusCode, Url, header};
use serde::Deserialize;
use std::{
    io::{self, Read},
    time::Duration,
};
use uuid::Uuid;

const SERVICE_URL: &str = "https://rides.cutout.lol/";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_METADATA_BYTES: usize = 4096;
const UPLOAD_CHUNK_BYTES: usize = 128 * 1024;

pub(super) struct HttpCaptureTransport {
    client: Client,
    base_url: Url,
}

impl HttpCaptureTransport {
    pub(super) fn production() -> Result<Self, CaptureSubmissionTransferError> {
        Self::build(SERVICE_URL)
    }

    fn build(base_url: &str) -> Result<Self, CaptureSubmissionTransferError> {
        let base_url = Url::parse(base_url).map_err(|_| CaptureSubmissionTransferError::Network)?;
        let mut builder = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());
        if base_url.scheme() == "http" {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|_| CaptureSubmissionTransferError::Network)?;
        Ok(Self { client, base_url })
    }

    #[cfg(test)]
    pub(super) fn loopback(base_url: &str) -> Result<Self, CaptureSubmissionTransferError> {
        let url = Url::parse(base_url).map_err(|_| CaptureSubmissionTransferError::Network)?;
        let loopback = url.scheme() == "http"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/"
            && url
                .host_str()
                .and_then(|host| {
                    host.trim_start_matches('[')
                        .trim_end_matches(']')
                        .parse::<std::net::IpAddr>()
                        .ok()
                })
                .is_some_and(|address| address.is_loopback());
        if !loopback {
            return Err(CaptureSubmissionTransferError::Network);
        }
        Self::build(base_url)
    }

    fn request(
        &self,
        method: Method,
        operation_id: &str,
        token: &str,
    ) -> Result<reqwest::RequestBuilder, CaptureSubmissionTransferError> {
        let operation =
            Uuid::parse_str(operation_id).map_err(|_| CaptureSubmissionTransferError::Rejected)?;
        let url = self
            .base_url
            .join(&format!("v1/capture-submissions/{operation}"))
            .map_err(|_| CaptureSubmissionTransferError::Network)?;
        Ok(self.client.request(method, url).bearer_auth(token))
    }
}

impl CaptureSubmissionTransport for HttpCaptureTransport {
    fn upload<'a>(
        &'a self,
        operation_id: &'a str,
        token: &'a str,
        metadata: &'a CaptureSubmissionMetadata,
        payload: CaptureSubmissionPayload,
    ) -> CaptureSubmissionFuture<'a, CaptureSubmissionReceipt> {
        Box::pin(async move {
            let metadata_header = serde_json::to_string(metadata)
                .map_err(|_| CaptureSubmissionTransferError::Rejected)?;
            if metadata_header.len() > MAX_METADATA_BYTES || !metadata_header.is_ascii() {
                return Err(CaptureSubmissionTransferError::Rejected);
            }
            let content_type = if metadata.evidence.is_some() {
                "application/vnd.cutout.capture-evidence"
            } else {
                match metadata.encoding {
                    PevcapEncoding::Jsonl => "application/x-ndjson",
                    PevcapEncoding::Binary => "application/octet-stream",
                }
            };
            let length = payload.byte_count();
            let stream = futures_util::stream::try_unfold(payload, |mut payload| async move {
                tokio::task::spawn_blocking(move || {
                    let mut chunk = vec![0; UPLOAD_CHUNK_BYTES];
                    let count = payload.read(&mut chunk)?;
                    if count == 0 {
                        return Ok::<_, io::Error>(None);
                    }
                    chunk.truncate(count);
                    payload.report_upload_progress();
                    Ok(Some((chunk, payload)))
                })
                .await
                .map_err(io::Error::other)?
            });
            let response = self
                .request(Method::PUT, operation_id, token)?
                .header("x-cutout-capture", metadata_header)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_LENGTH, length)
                .body(reqwest::Body::wrap_stream(stream))
                .send()
                .await
                .map_err(|_| CaptureSubmissionTransferError::Network)?;
            let (status, body) = bounded_response(response).await?;
            if !status.is_success() {
                return Err(status_error(status, &body));
            }
            serde_json::from_slice(&body).map_err(|_| CaptureSubmissionTransferError::Network)
        })
    }

    fn receipt<'a>(
        &'a self,
        operation_id: &'a str,
        token: &'a str,
    ) -> CaptureSubmissionFuture<'a, Option<CaptureSubmissionReceipt>> {
        Box::pin(async move {
            let response = self
                .request(Method::GET, operation_id, token)?
                .send()
                .await
                .map_err(|_| CaptureSubmissionTransferError::Network)?;
            let (status, body) = bounded_response(response).await?;
            if status == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !status.is_success() {
                return Err(status_error(status, &body));
            }
            serde_json::from_slice(&body)
                .map(Some)
                .map_err(|_| CaptureSubmissionTransferError::Network)
        })
    }

    fn delete<'a>(
        &'a self,
        operation_id: &'a str,
        token: &'a str,
    ) -> CaptureSubmissionFuture<'a, ()> {
        Box::pin(async move {
            let response = self
                .request(Method::DELETE, operation_id, token)?
                .send()
                .await
                .map_err(|_| CaptureSubmissionTransferError::Network)?;
            let (status, body) = bounded_response(response).await?;
            if !status.is_success() {
                return Err(status_error(status, &body));
            }
            Ok(())
        })
    }
}

async fn bounded_response(
    mut response: Response,
) -> Result<(StatusCode, Vec<u8>), CaptureSubmissionTransferError> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(CaptureSubmissionTransferError::Network);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| CaptureSubmissionTransferError::Network)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(CaptureSubmissionTransferError::Network);
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}

#[derive(Deserialize)]
struct ServerError {
    error: String,
}

fn status_error(status: StatusCode, body: &[u8]) -> CaptureSubmissionTransferError {
    use CaptureSubmissionTransferError as Error;
    match status.as_u16() {
        401 => Error::Unauthorized,
        403 => Error::Forbidden,
        409 => Error::Conflict,
        413 => Error::TooLarge,
        422 => match serde_json::from_slice::<ServerError>(body)
            .ok()
            .map(|body| body.error)
            .as_deref()
        {
            Some("capture_music_prohibited") => Error::MusicPresent,
            Some("capture_invalid" | "capture_unsupported_format" | "capture_integrity_failed") => {
                Error::InvalidCapture
            }
            _ => Error::Rejected,
        },
        429 => Error::Throttled,
        507 => Error::QuotaExceeded,
        500..=599 => Error::Unavailable,
        _ => Error::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, Response as HttpResponse},
        routing::get,
    };
    use std::{
        convert::Infallible,
        net::Ipv4Addr,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    async fn server(
        router: Router,
    ) -> Result<(String, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok((format!("http://{address}/"), task))
    }

    #[test]
    fn loopback_constructor_rejects_nonliteral_hosts_and_extra_url_components() {
        for url in [
            "http://localhost:1234/",
            "https://127.0.0.1:1234/",
            "http://127.0.0.1:1234/path",
            "http://user@127.0.0.1:1234/",
            "http://127.0.0.1:1234/?query",
            "http://127.0.0.1:1234/#fragment",
        ] {
            assert!(HttpCaptureTransport::loopback(url).is_err(), "{url}");
        }
        assert!(HttpCaptureTransport::loopback("http://127.0.0.1:1234/").is_ok());
        assert!(HttpCaptureTransport::loopback("http://[::1]:1234/").is_ok());
    }

    #[tokio::test]
    async fn receipt_uses_the_canonical_path_and_bearer_auth_without_following_redirects()
    -> Result<(), Box<dyn std::error::Error>> {
        let operation = Uuid::new_v4().to_string();
        let path = format!("/v1/capture-submissions/{operation}");
        let redirected = Arc::new(AtomicUsize::new(0));
        let redirected_handler = Arc::clone(&redirected);
        let router = Router::new()
            .route(
                &path,
                get(|headers: HeaderMap| async move {
                    assert_eq!(
                        headers
                            .get(header::AUTHORIZATION)
                            .and_then(|value| value.to_str().ok()),
                        Some("Bearer test-token")
                    );
                    (
                        StatusCode::TEMPORARY_REDIRECT,
                        [(header::LOCATION, "/unexpected")],
                    )
                }),
            )
            .route(
                "/unexpected",
                get(move || {
                    let redirected = Arc::clone(&redirected_handler);
                    async move {
                        redirected.fetch_add(1, Ordering::SeqCst);
                        StatusCode::OK
                    }
                }),
            );
        let (url, task) = server(router).await?;
        let result = HttpCaptureTransport::loopback(&url)?
            .receipt(&operation, "test-token")
            .await;
        task.abort();
        assert_eq!(result, Err(CaptureSubmissionTransferError::Rejected));
        assert_eq!(redirected.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn missing_receipt_is_absent_and_auth_failure_stays_classified()
    -> Result<(), Box<dyn std::error::Error>> {
        for (status, expected) in [
            (StatusCode::NOT_FOUND, Ok(None)),
            (
                StatusCode::UNAUTHORIZED,
                Err(CaptureSubmissionTransferError::Unauthorized),
            ),
        ] {
            let router = Router::new().route(
                "/v1/capture-submissions/{operation}",
                get(move || async move { status }),
            );
            let (url, task) = server(router).await?;
            let result = HttpCaptureTransport::loopback(&url)?
                .receipt(&Uuid::new_v4().to_string(), "test-token")
                .await;
            task.abort();
            assert_eq!(result, expected);
        }
        Ok(())
    }

    #[tokio::test]
    async fn receipt_is_parsed_and_delete_accepts_an_empty_success()
    -> Result<(), Box<dyn std::error::Error>> {
        let operation = Uuid::new_v4().to_string();
        let receipt = CaptureSubmissionReceipt {
            operation_id: operation.clone(),
            sha256: "a".repeat(64),
            bytes: 77,
            received_at_milliseconds: 123,
        };
        let response_receipt = receipt.clone();
        let router = Router::new().route(
            "/v1/capture-submissions/{operation}",
            get(move || {
                let receipt = response_receipt.clone();
                async move { axum::Json(receipt) }
            })
            .delete(|| async { StatusCode::NO_CONTENT }),
        );
        let (url, task) = server(router).await?;
        let transport = HttpCaptureTransport::loopback(&url)?;
        let received = transport.receipt(&operation, "test-token").await?;
        let deleted = transport.delete(&operation, "test-token").await;
        task.abort();
        assert_eq!(received, Some(receipt));
        assert_eq!(deleted, Ok(()));
        Ok(())
    }

    #[tokio::test]
    async fn receipt_rejects_chunked_response_over_the_bound()
    -> Result<(), Box<dyn std::error::Error>> {
        let router = Router::new().route(
            "/v1/capture-submissions/{operation}",
            get(|| async {
                let chunks = futures_util::stream::iter(
                    (0..5).map(|_| Ok::<_, Infallible>(vec![b' '; 8192])),
                );
                HttpResponse::new(Body::from_stream(chunks))
            }),
        );
        let (url, task) = server(router).await?;
        let result = HttpCaptureTransport::loopback(&url)?
            .receipt(&Uuid::new_v4().to_string(), "test-token")
            .await;
        task.abort();
        assert_eq!(result, Err(CaptureSubmissionTransferError::Network));
        Ok(())
    }

    #[test]
    fn capture_error_codes_are_classified_without_exposing_the_body() {
        for code in [
            "capture_invalid",
            "capture_unsupported_format",
            "capture_integrity_failed",
        ] {
            assert_eq!(
                status_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("{{\"error\":\"{code}\"}}").as_bytes()
                ),
                CaptureSubmissionTransferError::InvalidCapture
            );
        }
        assert_eq!(
            status_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                br#"{"error":"capture_music_prohibited"}"#
            ),
            CaptureSubmissionTransferError::MusicPresent
        );
        assert_eq!(
            status_error(
                StatusCode::INSUFFICIENT_STORAGE,
                br#"{"error":"capture_storage_quota"}"#
            ),
            CaptureSubmissionTransferError::QuotaExceeded
        );
    }
}
