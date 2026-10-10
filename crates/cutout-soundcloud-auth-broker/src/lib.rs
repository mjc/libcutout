#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::{
    fmt,
    future::Future,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response as HttpResponse},
    routing::get,
};
use reqwest::{Client, Response, redirect::Policy};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};

const TOKEN_ENDPOINT: &str = "https://secure.soundcloud.com/oauth/token";
const REFRESH_EARLY_SECONDS: u64 = 60;
const RETRY_DELAY_SECONDS: u64 = 120;
const CLIENT_TOKEN_HOURLY_LIMIT: usize = 30;
const CLIENT_TOKEN_TWELVE_HOUR_LIMIT: usize = 50;
const TOKEN_RESPONSE_MAX_BYTES: usize = 16 * 1024;
const ACCESS_TOKEN_MAX_BYTES: usize = 8 * 1024;
const MINIMUM_RETURNED_TTL_SECONDS: u64 = 16;
const MAXIMUM_RETURNED_TTL_SECONDS: u64 = 3_600;

#[derive(Clone, Serialize, Deserialize)]
struct SecretToken(String);

impl SecretToken {
    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretToken([REDACTED])")
    }
}

#[derive(Deserialize)]
pub(crate) struct SoundCloudCredentials {
    client_id: String,
    client_secret: SecretToken,
}

impl fmt::Debug for SoundCloudCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SoundCloudCredentials")
            .field("client_id", &"[REDACTED]")
            .field("client_secret", &"[REDACTED]")
            .finish()
    }
}

impl SoundCloudCredentials {
    async fn read(path: &Path) -> Result<Self, BrokerError> {
        let bytes = fs::read(path)
            .await
            .map_err(|_| BrokerError::CredentialsUnavailable)?;
        serde_json::from_slice(&bytes).map_err(|_| BrokerError::CredentialsUnavailable)
    }
}

#[derive(Serialize, Deserialize)]
struct CachedToken {
    access_token: SecretToken,
    refresh_token: Option<SecretToken>,
    expires_at: u64,
}

impl fmt::Debug for CachedToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CachedToken")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl CachedToken {
    fn from_provider(token: ProviderToken, now: u64) -> Result<Self, BrokerError> {
        if !valid_access_token(token.access_token.expose())
            || token.expires_in == 0
            || token.expires_in > MAXIMUM_RETURNED_TTL_SECONDS
        {
            return Err(BrokerError::ProviderUnavailable);
        }

        let expires_at = now
            .checked_add(token.expires_in)
            .ok_or(BrokerError::ProviderUnavailable)?;

        Ok(Self {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at,
        })
    }

    fn response(&self, now: u64) -> Option<TokenResponse> {
        let remaining = remaining_lifetime(self.expires_at, now);
        if remaining < MINIMUM_RETURNED_TTL_SECONDS {
            return None;
        }

        Some(TokenResponse {
            access_token: self.access_token.expose().to_owned(),
            token_type: "Bearer".to_owned(),
            expires_in: remaining.min(MAXIMUM_RETURNED_TTL_SECONDS),
        })
    }
}

#[must_use]
pub const fn remaining_lifetime(expires_at: u64, now: u64) -> u64 {
    expires_at.saturating_sub(now)
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

impl fmt::Debug for TokenResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenResponse")
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

#[derive(Serialize, Deserialize, Default)]
struct BrokerState {
    cached: Option<CachedToken>,
    #[serde(default)]
    upstream_attempts: Vec<u64>,
    #[serde(default)]
    client_token_attempts: Vec<u64>,
    #[serde(default)]
    next_attempt_at: u64,
}

impl fmt::Debug for BrokerState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BrokerState")
            .field("cached", &self.cached)
            .field("upstream_attempt_count", &self.upstream_attempts.len())
            .field(
                "client_token_attempt_count",
                &self.client_token_attempts.len(),
            )
            .field("next_attempt_at", &self.next_attempt_at)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderFailure {
    InvalidGrant,
    Rejected,
    Unavailable,
}

trait TokenProvider: Send + Sync {
    #[allow(clippy::manual_async_fn)]
    fn client_credentials(
        &self,
    ) -> impl Future<Output = Result<ProviderToken, ProviderFailure>> + Send;
    #[allow(clippy::manual_async_fn)]
    fn refresh<'a>(
        &'a self,
        token: &'a SecretToken,
    ) -> impl Future<Output = Result<ProviderToken, ProviderFailure>> + Send + 'a;
}

struct ProviderToken {
    access_token: SecretToken,
    refresh_token: Option<SecretToken>,
    expires_in: u64,
}

impl fmt::Debug for ProviderToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderToken")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

struct SoundCloudClient {
    http: Client,
    credentials: SoundCloudCredentials,
}

impl SoundCloudClient {
    fn new(credentials: SoundCloudCredentials) -> Result<Self, BrokerError> {
        if credentials.client_id.is_empty() || credentials.client_secret.expose().is_empty() {
            return Err(BrokerError::CredentialsUnavailable);
        }

        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(Policy::none())
            .build()
            .map_err(|_| BrokerError::ProviderUnavailable)?;

        Ok(Self { http, credentials })
    }

    async fn read_token_response(response: Response) -> Result<ProviderToken, ProviderFailure> {
        let status = response.status();
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| ProviderFailure::Unavailable)?
        {
            if bytes.len().saturating_add(chunk.len()) > TOKEN_RESPONSE_MAX_BYTES {
                return Err(ProviderFailure::Rejected);
            }
            bytes.extend_from_slice(&chunk);
        }

        if !status.is_success() {
            let invalid_grant = status == StatusCode::BAD_REQUEST
                && serde_json::from_slice::<ProviderErrorBody>(&bytes)
                    .is_ok_and(|body| body.error.as_deref() == Some("invalid_grant"));
            return Err(if invalid_grant {
                ProviderFailure::InvalidGrant
            } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                ProviderFailure::Unavailable
            } else {
                ProviderFailure::Rejected
            });
        }

        let wire = serde_json::from_slice::<ProviderTokenBody>(&bytes)
            .map_err(|_| ProviderFailure::Rejected)?;
        if wire.access_token.is_empty() || wire.expires_in == 0 {
            return Err(ProviderFailure::Rejected);
        }

        Ok(ProviderToken {
            access_token: SecretToken(wire.access_token),
            refresh_token: wire
                .refresh_token
                .filter(|value| !value.is_empty())
                .map(SecretToken),
            expires_in: wire.expires_in,
        })
    }
}

impl TokenProvider for SoundCloudClient {
    #[allow(clippy::manual_async_fn)]
    fn client_credentials(
        &self,
    ) -> impl Future<Output = Result<ProviderToken, ProviderFailure>> + Send {
        async {
            let response = self
                .http
                .post(TOKEN_ENDPOINT)
                .basic_auth(
                    &self.credentials.client_id,
                    Some(self.credentials.client_secret.expose()),
                )
                .form(&[("grant_type", "client_credentials")])
                .send()
                .await
                .map_err(|_| ProviderFailure::Unavailable)?;

            Self::read_token_response(response).await
        }
    }

    #[allow(clippy::manual_async_fn)]
    fn refresh<'a>(
        &'a self,
        token: &'a SecretToken,
    ) -> impl Future<Output = Result<ProviderToken, ProviderFailure>> + Send + 'a {
        async {
            let response = self
                .http
                .post(TOKEN_ENDPOINT)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", self.credentials.client_id.as_str()),
                    ("client_secret", self.credentials.client_secret.expose()),
                    ("refresh_token", token.expose()),
                ])
                .send()
                .await
                .map_err(|_| ProviderFailure::Unavailable)?;

            Self::read_token_response(response).await
        }
    }
}

#[derive(Deserialize)]
struct ProviderTokenBody {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Deserialize)]
struct ProviderErrorBody {
    error: Option<String>,
}

#[derive(Debug, Error)]
enum BrokerError {
    #[error("SoundCloud credentials are unavailable")]
    CredentialsUnavailable,
    #[error("SoundCloud token state is unavailable")]
    StateUnavailable,
    #[error("SoundCloud token provider is temporarily unavailable")]
    ProviderUnavailable,
    #[error("SoundCloud token requests are temporarily rate limited")]
    RetryLater,
}

struct TokenBroker<P> {
    provider: P,
    state_path: Option<PathBuf>,
    state: Mutex<BrokerState>,
}

impl<P: TokenProvider> TokenBroker<P> {
    async fn load(provider: P, state_path: PathBuf) -> Result<Self, BrokerError> {
        let state = match fs::read(&state_path).await {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| BrokerError::StateUnavailable)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BrokerState::default(),
            Err(_) => return Err(BrokerError::StateUnavailable),
        };

        Ok(Self {
            provider,
            state_path: Some(state_path),
            state: Mutex::new(state),
        })
    }

    #[cfg(test)]
    fn in_memory(provider: P, state: BrokerState) -> Self {
        Self {
            provider,
            state_path: None,
            state: Mutex::new(state),
        }
    }

    async fn get_token(&self, now: u64) -> Result<TokenResponse, BrokerError> {
        let request_started = Instant::now();
        let mut state = self.state.lock().await;
        let base_now = now;
        let now = elapsed_wall_time(base_now, request_started);

        if let Some(cached) = &state.cached
            && remaining_lifetime(cached.expires_at, now) > REFRESH_EARLY_SECONDS
        {
            return cached.response(now).ok_or(BrokerError::ProviderUnavailable);
        }

        if now < state.next_attempt_at {
            return state
                .cached
                .as_ref()
                .and_then(|cached| cached.response(now))
                .ok_or(BrokerError::RetryLater);
        }

        if let Some(refresh_token) = state
            .cached
            .as_ref()
            .and_then(|cached| cached.refresh_token.as_ref())
        {
            let refresh_token = refresh_token.clone();
            record_upstream_attempt(&mut state, now)?;
            self.persist(&state).await?;

            match self.provider.refresh(&refresh_token).await {
                Ok(token) => {
                    let completed_at = elapsed_wall_time(base_now, request_started);
                    return self.replace_with(token, completed_at, &mut state).await;
                }
                Err(ProviderFailure::InvalidGrant) => {
                    if let Some(cached) = &mut state.cached {
                        cached.refresh_token = None;
                    }
                    self.persist(&state).await?;
                    return self
                        .acquire_fresh_token(
                            elapsed_wall_time(base_now, request_started),
                            &mut state,
                        )
                        .await;
                }
                Err(_) => {
                    return state
                        .cached
                        .as_ref()
                        .and_then(|cached| {
                            cached.response(elapsed_wall_time(base_now, request_started))
                        })
                        .ok_or(BrokerError::ProviderUnavailable);
                }
            }
        }

        self.acquire_fresh_token(elapsed_wall_time(base_now, request_started), &mut state)
            .await
    }

    async fn acquire_fresh_token(
        &self,
        now: u64,
        state: &mut BrokerState,
    ) -> Result<TokenResponse, BrokerError> {
        state
            .client_token_attempts
            .retain(|attempt| now.saturating_sub(*attempt) < 12 * 60 * 60);
        if state.client_token_attempts.len() >= CLIENT_TOKEN_TWELVE_HOUR_LIMIT {
            return Err(BrokerError::RetryLater);
        }

        record_upstream_attempt(state, now)?;
        state.client_token_attempts.push(now);
        self.persist(state).await?;

        let upstream_started = Instant::now();
        match self.provider.client_credentials().await {
            Ok(token) => {
                let completed_at = elapsed_wall_time(now, upstream_started);
                self.replace_with(token, completed_at, state).await
            }
            Err(_) => Err(BrokerError::ProviderUnavailable),
        }
    }

    async fn replace_with(
        &self,
        token: ProviderToken,
        now: u64,
        state: &mut BrokerState,
    ) -> Result<TokenResponse, BrokerError> {
        let persist_started = Instant::now();
        let cached = CachedToken::from_provider(token, now)?;
        state.cached = Some(cached);
        state.next_attempt_at = 0;
        self.persist(state).await?;
        state
            .cached
            .as_ref()
            .and_then(|cached| cached.response(elapsed_wall_time(now, persist_started)))
            .ok_or(BrokerError::ProviderUnavailable)
    }

    async fn persist(&self, state: &BrokerState) -> Result<(), BrokerError> {
        let Some(path) = self.state_path.as_ref() else {
            return Ok(());
        };
        write_state(path, state).await
    }
}

async fn write_state(path: &Path, state: &BrokerState) -> Result<(), BrokerError> {
    let parent = path.parent().ok_or(BrokerError::StateUnavailable)?;
    fs::create_dir_all(parent)
        .await
        .map_err(|_| BrokerError::StateUnavailable)?;
    let bytes = serde_json::to_vec(state).map_err(|_| BrokerError::StateUnavailable)?;
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| BrokerError::StateUnavailable)?
            .as_nanos()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let write_result = async {
        let mut file = options
            .open(&temporary)
            .await
            .map_err(|_| BrokerError::StateUnavailable)?;
        file.write_all(&bytes)
            .await
            .map_err(|_| BrokerError::StateUnavailable)?;
        file.sync_all()
            .await
            .map_err(|_| BrokerError::StateUnavailable)?;
        drop(file);
        fs::rename(&temporary, path)
            .await
            .map_err(|_| BrokerError::StateUnavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .await
                .map_err(|_| BrokerError::StateUnavailable)?;
        }
        let directory = fs::File::open(parent)
            .await
            .map_err(|_| BrokerError::StateUnavailable)?;
        directory
            .sync_all()
            .await
            .map_err(|_| BrokerError::StateUnavailable)
    }
    .await;

    if write_result.is_err() {
        let _ = fs::remove_file(temporary).await;
    }
    write_result
}

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
}

async fn token_handler<P: TokenProvider>(
    State(broker): State<std::sync::Arc<TokenBroker<P>>>,
) -> HttpResponse {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let response = match broker.get_token(now).await {
        Ok(token) => (StatusCode::OK, Json(token)).into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "temporarily_unavailable",
            }),
        )
            .into_response(),
    };
    let mut response = response;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn health_handler() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn router<P: TokenProvider + 'static>(broker: std::sync::Arc<TokenBroker<P>>) -> Router {
    Router::new()
        .route("/healthz", get(health_handler))
        .route("/v1/token", get(token_handler::<P>))
        .with_state(broker)
}

fn elapsed_wall_time(base: u64, started: Instant) -> u64 {
    base.saturating_add(started.elapsed().as_secs())
}

fn valid_access_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= ACCESS_TOKEN_MAX_BYTES
        && token.bytes().all(|byte| byte.is_ascii_graphic())
}

fn record_upstream_attempt(state: &mut BrokerState, now: u64) -> Result<(), BrokerError> {
    state
        .upstream_attempts
        .retain(|attempt| now.saturating_sub(*attempt) < 60 * 60);
    if state.upstream_attempts.len() >= CLIENT_TOKEN_HOURLY_LIMIT {
        return Err(BrokerError::RetryLater);
    }
    state.upstream_attempts.push(now);
    state.next_attempt_at = now.saturating_add(RETRY_DELAY_SECONDS);
    Ok(())
}

/// Starts the loopback-only HTTP service using systemd credential and state paths.
/// Run the loopback-only token broker using systemd credential and state paths.
///
/// # Errors
/// Returns a static error when credentials, persisted state, listener setup, or service execution fails.
pub async fn run_from_environment() -> Result<(), &'static str> {
    use std::{env, net::SocketAddr, sync::Arc};

    let credentials_directory =
        env::var_os("CREDENTIALS_DIRECTORY").ok_or("systemd credentials are not available")?;
    let credentials_path = PathBuf::from(credentials_directory).join("soundcloud.json");
    let state_path = env::var_os("SOUNDCLOUD_TOKEN_STATE_FILE").map_or_else(
        || PathBuf::from("/var/lib/soundcloud-token-broker/tokens.json"),
        PathBuf::from,
    );
    let listen_address = env::var("SOUNDCLOUD_BROKER_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:8787".to_owned())
        .parse::<SocketAddr>()
        .map_err(|_| "invalid service listen address")?;
    if !listen_address.ip().is_loopback() {
        return Err("service listen address must be loopback");
    }

    let credentials = SoundCloudCredentials::read(&credentials_path)
        .await
        .map_err(|_| "SoundCloud credentials are unavailable")?;
    let client = SoundCloudClient::new(credentials)
        .map_err(|_| "SoundCloud client initialization failed")?;
    let broker = TokenBroker::load(client, state_path)
        .await
        .map_err(|_| "SoundCloud token state is unavailable")?;
    let listener = tokio::net::TcpListener::bind(listen_address)
        .await
        .map_err(|_| "service listener could not start")?;

    axum::serve(listener, router(Arc::new(broker)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|_| "service stopped unexpectedly")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Mutex as StdMutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::{
        BrokerState, CLIENT_TOKEN_HOURLY_LIMIT, CachedToken, Future, ProviderFailure,
        ProviderToken, SecretToken, TokenBroker, TokenProvider, remaining_lifetime,
    };

    struct FakeProvider {
        client_results: StdMutex<VecDeque<Result<ProviderToken, ProviderFailure>>>,
        refresh_results: StdMutex<VecDeque<Result<ProviderToken, ProviderFailure>>>,
        client_calls: AtomicUsize,
        refresh_calls: AtomicUsize,
    }

    impl FakeProvider {
        fn new(
            client_results: impl IntoIterator<Item = Result<ProviderToken, ProviderFailure>>,
            refresh_results: impl IntoIterator<Item = Result<ProviderToken, ProviderFailure>>,
        ) -> Self {
            Self {
                client_results: StdMutex::new(client_results.into_iter().collect()),
                refresh_results: StdMutex::new(refresh_results.into_iter().collect()),
                client_calls: AtomicUsize::new(0),
                refresh_calls: AtomicUsize::new(0),
            }
        }
    }

    impl TokenProvider for FakeProvider {
        #[allow(clippy::manual_async_fn)]
        fn client_credentials(
            &self,
        ) -> impl Future<Output = Result<ProviderToken, ProviderFailure>> + Send {
            async {
                self.client_calls.fetch_add(1, Ordering::SeqCst);
                self.client_results
                    .lock()
                    .expect("client result lock")
                    .pop_front()
                    .unwrap_or_else(|| Ok(provider_token("fresh", "refresh-fresh", 3_600)))
            }
        }

        #[allow(clippy::manual_async_fn)]
        fn refresh<'a>(
            &'a self,
            _token: &'a SecretToken,
        ) -> impl Future<Output = Result<ProviderToken, ProviderFailure>> + Send + 'a {
            async {
                self.refresh_calls.fetch_add(1, Ordering::SeqCst);
                self.refresh_results
                    .lock()
                    .expect("refresh result lock")
                    .pop_front()
                    .unwrap_or_else(|| Ok(provider_token("renewed", "refresh-renewed", 3_600)))
            }
        }
    }

    fn provider_token(access: &str, refresh: &str, expires_in: u64) -> ProviderToken {
        ProviderToken {
            access_token: SecretToken(access.to_owned()),
            refresh_token: Some(SecretToken(refresh.to_owned())),
            expires_in,
        }
    }

    fn expired_state(refresh_token: &str) -> BrokerState {
        BrokerState {
            cached: Some(CachedToken {
                access_token: SecretToken("expired-access".to_owned()),
                refresh_token: Some(SecretToken(refresh_token.to_owned())),
                expires_at: 900,
            }),
            upstream_attempts: Vec::new(),
            client_token_attempts: Vec::new(),
            next_attempt_at: 0,
        }
    }

    #[tokio::test]
    async fn cached_token_reports_remaining_lifetime_and_is_reused() {
        let provider = FakeProvider::new([], []);
        let broker = TokenBroker::in_memory(provider, BrokerState::default());

        let first = broker.get_token(1_000).await.expect("fresh token");
        let second = broker.get_token(1_060).await.expect("cached token");

        assert_eq!(first.expires_in, 3_600);
        assert_eq!(second.expires_in, 3_540);
        assert_eq!(broker.provider.client_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_token_requests_share_one_upstream_grant() {
        let provider = FakeProvider::new([], []);
        let broker = std::sync::Arc::new(TokenBroker::in_memory(provider, BrokerState::default()));
        let mut requests = Vec::new();
        for _ in 0..16 {
            let broker = std::sync::Arc::clone(&broker);
            requests.push(tokio::spawn(async move { broker.get_token(1_000).await }));
        }

        for request in requests {
            let token = request.await.expect("token request task").expect("token");
            assert_eq!(token.expires_in, 3_600);
        }
        assert_eq!(broker.provider.client_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn refresh_rotates_single_use_refresh_token_before_expiry() {
        let provider = FakeProvider::new(
            [],
            [Ok(provider_token("rotated", "refresh-rotated", 3_600))],
        );
        let broker = TokenBroker::in_memory(provider, expired_state("refresh-old"));

        let token = broker.get_token(1_000).await.expect("refreshed token");
        let cached = broker.state.lock().await;

        assert_eq!(token.access_token, "rotated");
        assert_eq!(
            cached
                .cached
                .as_ref()
                .and_then(|value| value.refresh_token.as_ref())
                .map(SecretToken::expose),
            Some("refresh-rotated")
        );
        assert_eq!(broker.provider.refresh_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.provider.client_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invalid_refresh_grant_uses_one_fresh_credentials_grant() {
        let provider = FakeProvider::new(
            [Ok(provider_token(
                "replacement",
                "refresh-replacement",
                3_600,
            ))],
            [Err(ProviderFailure::InvalidGrant)],
        );
        let broker = TokenBroker::in_memory(provider, expired_state("consumed-refresh"));

        let token = broker.get_token(1_000).await.expect("fallback token");
        let cached = broker.state.lock().await;

        assert_eq!(token.access_token, "replacement");
        assert_eq!(
            cached
                .cached
                .as_ref()
                .and_then(|value| value.refresh_token.as_ref())
                .map(SecretToken::expose),
            Some("refresh-replacement")
        );
        assert_eq!(broker.provider.refresh_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.provider.client_calls.load(Ordering::SeqCst), 1);
        assert_eq!(cached.upstream_attempts.len(), 2);
    }

    #[tokio::test]
    async fn refresh_and_fallback_are_separately_counted_against_upstream_budget() {
        let provider = FakeProvider::new(
            [Ok(provider_token("replacement", "refresh-new", 3_600))],
            [Err(ProviderFailure::InvalidGrant)],
        );
        let mut state = expired_state("refresh-old");
        state.upstream_attempts = vec![1_000; CLIENT_TOKEN_HOURLY_LIMIT - 1];
        let broker = TokenBroker::in_memory(provider, state);

        assert!(broker.get_token(1_000).await.is_err());
        let state = broker.state.lock().await;

        assert_eq!(broker.provider.refresh_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.provider.client_calls.load(Ordering::SeqCst), 0);
        assert_eq!(state.upstream_attempts.len(), CLIENT_TOKEN_HOURLY_LIMIT);
    }

    #[tokio::test]
    async fn failed_refresh_does_not_mint_or_retry_client_credentials() {
        let provider = FakeProvider::new([], [Err(ProviderFailure::Unavailable)]);
        let broker = TokenBroker::in_memory(provider, expired_state("refresh-current"));

        assert!(broker.get_token(1_000).await.is_err());
        assert!(broker.get_token(1_001).await.is_err());
        assert_eq!(broker.provider.refresh_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.provider.client_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cached_token_response_contains_only_mobile_fields() {
        let cached = CachedToken {
            access_token: SecretToken("test-secret-token".to_owned()),
            refresh_token: Some(SecretToken("test-refresh-token".to_owned())),
            expires_at: 1_100,
        };

        let value =
            serde_json::to_value(cached.response(1_000).expect("response")).expect("token JSON");

        assert_eq!(value.as_object().expect("object").len(), 3);
        assert_eq!(value["expires_in"], 100);
        assert!(value.get("refresh_token").is_none());
    }

    #[test]
    fn secret_token_debug_is_redacted() {
        let token = SecretToken("never-log-this-value".to_owned());
        assert!(!format!("{token:?}").contains("never-log-this-value"));
    }

    #[test]
    fn remaining_lifetime_saturates_when_clock_moves_past_expiry() {
        assert_eq!(remaining_lifetime(999, 1_000), 0);
    }
}
