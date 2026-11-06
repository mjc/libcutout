//! Bounded HTTP transport shared by charging application workflows.

use reqwest::{Client, Method, StatusCode, Url, header};
use serde::de::DeserializeOwned;
use std::{net::IpAddr, time::Duration};

const DIRECTORY_URL: &str = "https://rides.cutout.lol/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUEST_BODY_BYTES: usize = 32 * 1024;
/// Maximum accepted decoded response body size.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Transport and service failure without binding-layer types.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ChargingTransportError {
    /// Connection, URL, body-size, or response decoding failed.
    #[error("charging service transport failed")]
    Network,
    /// The service rejected the contributor credential.
    #[error("charging service authorization failed")]
    Unauthorized,
    /// The local operation no longer matches server state.
    #[error("charging service reported a conflict")]
    Conflict,
    /// The service rejected the submitted operation.
    #[error("charging service rejected the operation")]
    Rejected,
    /// The service asked the client to slow down.
    #[error("charging service throttled the operation")]
    Throttled,
}

impl ChargingTransportError {
    /// Maps one HTTP status to the stable charging-service failure category.
    #[must_use]
    pub fn from_status(status: StatusCode) -> Self {
        match status.as_u16() {
            401 => Self::Unauthorized,
            409 => Self::Conflict,
            422 | 403 | 404 => Self::Rejected,
            429 => Self::Throttled,
            _ => Self::Network,
        }
    }
}

/// HTTP client with bounded bodies, redirects disabled, and fixed production origin policy.
#[derive(Debug)]
pub struct ChargingHttpClient {
    client: Client,
    base_url: Url,
}

impl ChargingHttpClient {
    /// Connects to the production charging directory.
    ///
    /// # Errors
    /// Returns `Network` when the HTTP client cannot be configured.
    pub fn production() -> Result<Self, ChargingTransportError> {
        Self::build(DIRECTORY_URL)
    }

    /// Creates a development client only for an HTTP loopback origin.
    ///
    /// # Errors
    /// Returns `Network` for malformed or non-loopback origins, or client configuration failure.
    pub fn loopback(base_url: &str) -> Result<Self, ChargingTransportError> {
        let url = Url::parse(base_url).map_err(|_| ChargingTransportError::Network)?;
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
                        .parse::<IpAddr>()
                        .ok()
                })
                .is_some_and(|address| address.is_loopback());
        if !loopback {
            return Err(ChargingTransportError::Network);
        }
        Self::build(base_url)
    }

    fn build(base_url: &str) -> Result<Self, ChargingTransportError> {
        let base_url = Url::parse(base_url).map_err(|_| ChargingTransportError::Network)?;
        let mut builder = cutout_core::http::client_builder()
            .map_err(|_| ChargingTransportError::Network)?
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());
        if base_url.scheme() == "http" {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|_| ChargingTransportError::Network)?;
        Ok(Self { client, base_url })
    }

    fn resolve_path(&self, path: &str) -> Result<Url, ChargingTransportError> {
        let url = self
            .base_url
            .join(path)
            .map_err(|_| ChargingTransportError::Network)?;
        if url.origin() != self.base_url.origin() {
            return Err(ChargingTransportError::Network);
        }
        Ok(url)
    }

    /// Sends one bounded request and decodes its JSON response.
    ///
    /// # Errors
    /// Returns the mapped service status or `Network` when the request, size bound, or JSON
    /// contract fails.
    pub async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<T, ChargingTransportError> {
        let url = self.resolve_path(path)?;
        let mut request = self.client.request(method, url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            if body.len() > MAX_REQUEST_BODY_BYTES {
                return Err(ChargingTransportError::Network);
            }
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| ChargingTransportError::Network)?;
        let status = response.status();
        if !status.is_success() {
            return Err(ChargingTransportError::from_status(status));
        }
        let mut bytes = Vec::new();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ChargingTransportError::Network);
        }
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| ChargingTransportError::Network)?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(ChargingTransportError::Network);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| ChargingTransportError::Network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_statuses_keep_auth_conflict_rejection_and_throttling_distinct() {
        assert_eq!(
            ChargingTransportError::from_status(StatusCode::UNAUTHORIZED),
            ChargingTransportError::Unauthorized
        );
        assert_eq!(
            ChargingTransportError::from_status(StatusCode::CONFLICT),
            ChargingTransportError::Conflict
        );
        assert_eq!(
            ChargingTransportError::from_status(StatusCode::UNPROCESSABLE_ENTITY),
            ChargingTransportError::Rejected
        );
        assert_eq!(
            ChargingTransportError::from_status(StatusCode::TOO_MANY_REQUESTS),
            ChargingTransportError::Throttled
        );
        assert_eq!(
            ChargingTransportError::from_status(StatusCode::SERVICE_UNAVAILABLE),
            ChargingTransportError::Network
        );
    }

    #[test]
    fn loopback_transport_rejects_non_loopback_and_url_decorations() {
        assert!(ChargingHttpClient::loopback("http://127.0.0.1:8080/").is_ok());
        assert!(ChargingHttpClient::loopback("http://[::1]:8080/").is_ok());
        for origin in [
            "http://example.test/",
            "https://127.0.0.1/",
            "http://127.0.0.1/path",
            "http://user@127.0.0.1/",
            "http://127.0.0.1/?token=x",
        ] {
            assert!(ChargingHttpClient::loopback(origin).is_err(), "{origin}");
        }
    }

    #[test]
    fn request_paths_cannot_redirect_the_application_client_to_another_origin() {
        let client = ChargingHttpClient::loopback("http://127.0.0.1:8080/").expect("client");
        assert!(client.resolve_path("v1/charging-spots").is_ok());
        assert!(client.resolve_path("//example.test/private").is_err());
        assert!(client.resolve_path("https://example.test/private").is_err());
    }
}
