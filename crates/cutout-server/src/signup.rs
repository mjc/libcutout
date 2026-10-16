use crate::{
    Error, Store,
    store::{begin_write, db_error, hash, now_ms},
};
use axum::{
    Json,
    extract::{ConnectInfo, Extension, State},
    http::{HeaderMap, StatusCode, header},
};
use reqwest::Client;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use uuid::Uuid;

const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
const SIGNUP_ACTION: &str = "alpha_signup";
const DISCLOSURE_REVISION: &str = "alpha-signup-2026-10-16";

#[derive(Clone)]
pub(crate) struct SignupState {
    config: Option<Config>,
    client: Option<Client>,
    #[cfg(test)]
    siteverify_url: String,
}

#[derive(Clone)]
struct Config {
    site_key: String,
    secret: String,
    hostname: String,
}

impl SignupState {
    pub(crate) fn from_env() -> Self {
        // Collection stays unavailable until the Lific outbox worker and privacy gates exist.
        Self {
            config: None,
            client: Client::builder()
                .timeout(Duration::from_millis(1_100))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .ok(),
            #[cfg(test)]
            siteverify_url: SITEVERIFY_URL.to_owned(),
        }
    }

    #[cfg(test)]
    fn for_test(siteverify_url: String) -> Self {
        Self {
            config: Some(Config {
                site_key: "test-site-key".into(),
                secret: "test-secret".into(),
                hostname: "cutout.lol".into(),
            }),
            client: Client::builder()
                .timeout(Duration::from_millis(1_100))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .ok(),
            siteverify_url,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PublicConfig {
    enabled: bool,
    site_key: Option<String>,
    action: &'static str,
}

pub(crate) async fn public_config(Extension(state): Extension<SignupState>) -> Json<PublicConfig> {
    Json(PublicConfig {
        enabled: state.config.is_some(),
        site_key: state.config.as_ref().map(|config| config.site_key.clone()),
        action: SIGNUP_ACTION,
    })
}

#[derive(Deserialize)]
pub(crate) struct Request {
    idempotency_key: String,
    email: String,
    pev_type: Option<String>,
    pev_model: String,
    requested_accessories: String,
    locale: String,
    turnstile_token: String,
    website: String,
}

#[derive(Serialize)]
pub(crate) struct Receipt {
    status: &'static str,
    request_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct SiteverifyResponse {
    success: bool,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default, rename = "error-codes")]
    error_codes: Vec<String>,
}

#[derive(Serialize)]
struct SiteverifyRequest<'a> {
    secret: &'a str,
    response: &'a str,
    idempotency_key: &'a str,
}

pub(crate) async fn submit(
    State(store): State<Store>,
    Extension(state): Extension<SignupState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    trusted_proxy: Option<Extension<crate::TrustedProxy>>,
    headers: HeaderMap,
    body: Result<Json<Request>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<Receipt>), Error> {
    let request = body.map_err(|_| Error::Invalid)?.0;
    let request = ValidRequest::try_from(request)?;

    let config = state.config.as_ref().ok_or(Error::Unavailable)?;
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or(Error::Invalid)?;
    if origin != format!("https://{}", config.hostname) {
        return Err(Error::Invalid);
    }
    // A completed matching request may be retried after its one-use token is spent.
    if let Some(receipt) = store
        .signup_receipt(&request.idempotency_key, &request.payload_hash)
        .await?
    {
        return Ok((StatusCode::ACCEPTED, Json(receipt)));
    }
    let source = crate::http::registration::registration_source(
        peer.ip(),
        trusted_proxy.map(|Extension(proxy)| proxy),
        &headers,
    )?;
    store.check_signup_budget(source).await?;
    let client = state.client.as_ref().ok_or(Error::Unavailable)?;
    #[cfg(test)]
    verify_token_at(
        client,
        config,
        &request.turnstile_token,
        &request.idempotency_key,
        &state.siteverify_url,
    )
    .await?;
    #[cfg(not(test))]
    verify_token(
        client,
        config,
        &request.turnstile_token,
        &request.idempotency_key,
    )
    .await?;
    let receipt = store.accept_signup(request).await?;
    Ok((StatusCode::ACCEPTED, Json(receipt)))
}

struct ValidRequest {
    idempotency_key: String,
    email: String,
    email_hash: String,
    pev_type: Option<String>,
    pev_model: String,
    requested_accessories: String,
    locale: String,
    payload_hash: String,
    turnstile_token: String,
}

impl TryFrom<Request> for ValidRequest {
    type Error = Error;

    fn try_from(request: Request) -> Result<Self, Self::Error> {
        if !request.website.trim().is_empty()
            || request.idempotency_key.len() > 64
            || Uuid::parse_str(&request.idempotency_key).is_err()
            || request.turnstile_token.is_empty()
            || request.turnstile_token.len() > 2048
            || request.email.len() > 254
            || !request.email.contains('@')
            || request.pev_model.len() > 120
            || request.requested_accessories.len() > 600
            || (request.locale != "en" && request.locale != "es")
        {
            return Err(Error::Invalid);
        }
        let email = request.email.trim().to_owned();
        let idempotency_key = Uuid::parse_str(&request.idempotency_key)
            .map_err(|_| Error::Invalid)?
            .to_string();
        let (local, domain) = email.rsplit_once('@').ok_or(Error::Invalid)?;
        if local.is_empty() || domain.is_empty() || email.chars().any(char::is_whitespace) {
            return Err(Error::Invalid);
        }
        let email_hash = hash(format!("{}@{}", local, domain.to_ascii_lowercase()).as_bytes());
        let normalized = NormalizedRequest {
            email,
            pev_type: request.pev_type.filter(|value| !value.trim().is_empty()),
            pev_model: request.pev_model.trim().to_owned(),
            requested_accessories: request.requested_accessories.trim().to_owned(),
            locale: request.locale,
        };
        if normalized
            .pev_type
            .as_ref()
            .is_some_and(|value| value.len() > 32)
        {
            return Err(Error::Invalid);
        }
        let payload_hash = hash(&serde_json::to_vec(&normalized).map_err(|_| Error::Invalid)?);
        Ok(Self {
            idempotency_key,
            email_hash,
            email: normalized.email,
            pev_type: normalized.pev_type,
            pev_model: normalized.pev_model,
            requested_accessories: normalized.requested_accessories,
            locale: normalized.locale,
            payload_hash,
            turnstile_token: request.turnstile_token,
        })
    }
}

#[derive(Serialize)]
struct NormalizedRequest {
    email: String,
    pev_type: Option<String>,
    pev_model: String,
    requested_accessories: String,
    locale: String,
}

#[cfg(not(test))]
async fn verify_token(
    client: &Client,
    config: &Config,
    token: &str,
    idempotency_key: &str,
) -> Result<(), Error> {
    verify_token_at(client, config, token, idempotency_key, SITEVERIFY_URL).await
}

async fn verify_token_at(
    client: &Client,
    config: &Config,
    token: &str,
    idempotency_key: &str,
    url: &str,
) -> Result<(), Error> {
    let payload = SiteverifyRequest {
        secret: &config.secret,
        response: token,
        idempotency_key,
    };
    let response = tokio::time::timeout(Duration::from_millis(2_400), async {
        let mut result = None;
        for _ in 0..2 {
            match client.post(url).json(&payload).send().await {
                Ok(response) if response.status().is_success() => {
                    let parsed = response
                        .json::<SiteverifyResponse>()
                        .await
                        .map_err(|_| Error::Unavailable)?;
                    let retryable = !parsed.success
                        && parsed
                            .error_codes
                            .iter()
                            .any(|code| code == "internal-error");
                    result = Some(parsed);
                    if !retryable {
                        break;
                    }
                }
                Ok(response) if response.status().is_server_error() => {}
                Ok(_) => return Err(Error::Unavailable),
                Err(_) => {}
            }
        }
        result.ok_or(Error::Unavailable)
    })
    .await
    .map_err(|_| Error::Unavailable)??;
    classify_response(&response, config)
}

fn classify_response(response: &SiteverifyResponse, config: &Config) -> Result<(), Error> {
    if response.success {
        return if response.hostname.as_deref() == Some(config.hostname.as_str())
            && response.action.as_deref() == Some(SIGNUP_ACTION)
        {
            Ok(())
        } else {
            Err(Error::Invalid)
        };
    }
    if response.error_codes.iter().any(|code| {
        code.as_str() == "timeout-or-duplicate"
            || code.as_str() == "invalid-input-response"
            || code.as_str() == "missing-input-response"
    }) {
        return Err(Error::Invalid);
    }
    Err(Error::Unavailable)
}

impl Store {
    async fn check_signup_budget(&self, source: IpAddr) -> Result<(), Error> {
        let principal = format!("alpha-signup:{}", hash(source_key(source).as_bytes()));
        self.call(move |connection| {
            let now = now_ms()?;
            let transaction = begin_write(connection)?;
            crate::store::quota(
                &transaction,
                &principal,
                crate::store::QuotaWindow::Minute,
                5,
                now,
            )?;
            transaction.commit().map_err(db_error)
        })
        .await
    }

    async fn signup_receipt(
        &self,
        key: &str,
        payload_hash: &str,
    ) -> Result<Option<Receipt>, Error> {
        let key = key.to_owned();
        let payload_hash = payload_hash.to_owned();
        self.call(move |connection| {
            let found = connection.query_row(
                "SELECT payload_hash, receipt_id FROM alpha_signup_idempotency WHERE idempotency_key=?1",
                [&key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            ).optional().map_err(db_error)?;
            match found {
                Some((stored, request_id)) if stored == payload_hash => Ok(Some(Receipt { status: "received", request_id })),
                Some(_) => Err(Error::Conflict),
                None => Ok(None),
            }
        }).await
    }

    async fn accept_signup(&self, request: ValidRequest) -> Result<Receipt, Error> {
        self.call(move |connection| {
            let transaction = begin_write(connection)?;
            if let Some((stored, request_id)) = transaction.query_row(
                "SELECT payload_hash, receipt_id FROM alpha_signup_idempotency WHERE idempotency_key=?1",
                [&request.idempotency_key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            ).optional().map_err(db_error)? {
                if stored != request.payload_hash { return Err(Error::Conflict); }
                transaction.commit().map_err(db_error)?;
                return Ok(Receipt { status: "received", request_id });
            }
            let request_id = transaction.query_row(
                "SELECT id FROM alpha_signup_requests WHERE email_hash=?1", [&request.email_hash], |row| row.get::<_, String>(0),
            ).optional().map_err(db_error)?.unwrap_or_else(|| Uuid::new_v4().to_string());
            let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM alpha_signup_requests WHERE id=?1)", [&request_id], |row| row.get(0)).map_err(db_error)?;
            if !exists {
                transaction.execute(
                    "INSERT INTO alpha_signup_requests (id,email,email_hash,pev_type,pev_model,requested_accessories,locale,disclosure_revision,received_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    rusqlite::params![request_id, request.email, request.email_hash, request.pev_type, request.pev_model, request.requested_accessories, request.locale, DISCLOSURE_REVISION, now_ms()?],
                ).map_err(db_error)?;
                transaction.execute("INSERT INTO alpha_signup_outbox(request_id,state) VALUES (?1,'pending')", [&request_id]).map_err(db_error)?;
            }
            let receipt_id = Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO alpha_signup_idempotency(idempotency_key,payload_hash,request_id,receipt_id) VALUES (?1,?2,?3,?4)",
                rusqlite::params![request.idempotency_key, request.payload_hash, request_id, receipt_id],
            ).map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            Ok(Receipt { status: "received", request_id: receipt_id })
        }).await
    }
}

fn source_key(source: IpAddr) -> String {
    match source.to_canonical() {
        IpAddr::V4(address) => format!("v4:{address}"),
        IpAddr::V6(address) => format!(
            "v6:{:x}:{:x}:{:x}:{:x}",
            u16::from_be_bytes([address.octets()[0], address.octets()[1]]),
            u16::from_be_bytes([address.octets()[2], address.octets()[3]]),
            u16::from_be_bytes([address.octets()[4], address.octets()[5]]),
            u16::from_be_bytes([address.octets()[6], address.octets()[7]])
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::post};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    async fn test_server(
        reply: serde_json::Value,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let app = Router::new().route(
            "/siteverify",
            post(move |Json(_body): Json<serde_json::Value>| {
                seen.fetch_add(1, Ordering::SeqCst);
                let reply = reply.clone();
                async move { Json(reply) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let address = format!("http://{}", listener.local_addr().expect("mock addr"));
        let task =
            tokio::spawn(async move { axum::serve(listener, app).await.expect("mock serve") });
        (format!("{address}/siteverify"), calls, task)
    }

    async fn signup_server(
        siteverify_url: String,
    ) -> (
        String,
        Store,
        tokio::task::JoinHandle<()>,
        tempfile::TempDir,
    ) {
        signup_server_with_state(SignupState::for_test(siteverify_url)).await
    }

    async fn signup_server_with_state(
        state: SignupState,
    ) -> (
        String,
        Store,
        tokio::task::JoinHandle<()>,
        tempfile::TempDir,
    ) {
        let directory = tempfile::TempDir::new().expect("database tempdir");
        let store = Store::open(&directory.path().join("alpha.sqlite")).expect("store");
        let app = crate::http::router_with_signup_state(store.clone(), state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("signup bind");
        let address = format!("http://{}", listener.local_addr().expect("signup addr"));
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .expect("signup serve");
        });
        (address, store, task, directory)
    }

    fn body(key: &str, token: &str, email: &str) -> serde_json::Value {
        serde_json::json!({
            "idempotency_key": key, "email": email, "pev_type": "euc",
            "pev_model": "Example model", "requested_accessories": "Example accessory",
            "locale": "en", "turnstile_token": token, "website": ""
        })
    }

    async fn submit(
        client: &Client,
        address: &str,
        body: serde_json::Value,
        origin: &str,
    ) -> reqwest::Response {
        client
            .post(format!("{address}/v1/alpha-signup/requests"))
            .header("origin", origin)
            .json(&body)
            .send()
            .await
            .expect("signup request")
    }

    fn valid_request() -> Request {
        Request {
            idempotency_key: Uuid::new_v4().to_string(),
            email: "abc@example.com".into(),
            pev_type: None,
            pev_model: String::new(),
            requested_accessories: String::new(),
            locale: "en".into(),
            turnstile_token: "opaque-token".into(),
            website: String::new(),
        }
    }

    #[test]
    fn validates_bounds_honeypot_and_locale_without_touching_turnstile() {
        let request = valid_request();
        assert!(ValidRequest::try_from(request).is_ok());
        let mut request = valid_request();
        request.locale = "fr".into();
        assert_eq!(ValidRequest::try_from(request).err(), Some(Error::Invalid));
        let mut request = valid_request();
        request.website = "https://spam.example".into();
        assert_eq!(ValidRequest::try_from(request).err(), Some(Error::Invalid));
        let mut request = valid_request();
        request.pev_model = "x".repeat(121);
        assert_eq!(ValidRequest::try_from(request).err(), Some(Error::Invalid));
        let mut request = valid_request();
        request.requested_accessories = "x".repeat(601);
        assert_eq!(ValidRequest::try_from(request).err(), Some(Error::Invalid));
        let mut request = valid_request();
        request.turnstile_token = "x".repeat(2049);
        assert_eq!(ValidRequest::try_from(request).err(), Some(Error::Invalid));
    }

    #[test]
    fn siteverify_success_requires_configured_hostname_and_action() {
        let config = Config {
            site_key: "public".into(),
            secret: "private".into(),
            hostname: "cutout.lol".into(),
        };
        let response = SiteverifyResponse {
            success: true,
            hostname: Some("cutout.lol".into()),
            action: Some(SIGNUP_ACTION.into()),
            error_codes: Vec::new(),
        };
        assert_eq!(classify_response(&response, &config), Ok(()));
        let wrong_hostname = SiteverifyResponse {
            hostname: Some("attacker.example".into()),
            ..response.clone()
        };
        assert_eq!(
            classify_response(&wrong_hostname, &config),
            Err(Error::Invalid)
        );
        let wrong_action = SiteverifyResponse {
            action: Some("login".into()),
            ..response.clone()
        };
        assert_eq!(
            classify_response(&wrong_action, &config),
            Err(Error::Invalid)
        );
        let unsuccessful = SiteverifyResponse {
            success: false,
            ..response
        };
        assert_eq!(
            classify_response(&unsuccessful, &config),
            Err(Error::Unavailable)
        );
        let expired = SiteverifyResponse {
            success: false,
            hostname: None,
            action: None,
            error_codes: vec!["timeout-or-duplicate".into()],
        };
        assert_eq!(classify_response(&expired, &config), Err(Error::Invalid));
        let outage = SiteverifyResponse {
            success: false,
            hostname: None,
            action: None,
            error_codes: vec!["internal-error".into()],
        };
        assert_eq!(classify_response(&outage, &config), Err(Error::Unavailable));
    }

    #[tokio::test]
    async fn accepted_request_retry_does_not_redeem_token_twice_or_duplicate_outbox() {
        let (verify_url, verify_calls, verify_task) = test_server(serde_json::json!({
            "success": true, "hostname": "cutout.lol", "action": SIGNUP_ACTION
        }))
        .await;
        let (address, store, signup_task, _directory) = signup_server(verify_url).await;
        let client = Client::builder().no_proxy().build().expect("client");
        let key = Uuid::new_v4().to_string();
        let payload = body(&key, "opaque-token", "rider@example.com");
        let first = submit(&client, &address, payload.clone(), "https://cutout.lol").await;
        assert_eq!(first.status(), StatusCode::ACCEPTED);
        let first_receipt: serde_json::Value = first.json().await.expect("first receipt");
        let retry = submit(&client, &address, payload.clone(), "https://cutout.lol").await;
        assert_eq!(retry.status(), StatusCode::ACCEPTED);
        let retry_receipt: serde_json::Value = retry.json().await.expect("retry receipt");
        assert_eq!(first_receipt, retry_receipt);
        assert_eq!(verify_calls.load(Ordering::SeqCst), 1);
        let mut changed = payload.clone();
        changed["email"] = serde_json::json!("other@example.com");
        assert_eq!(
            submit(&client, &address, changed, "https://cutout.lol")
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(verify_calls.load(Ordering::SeqCst), 1);
        let duplicate_email = body(
            &Uuid::new_v4().to_string(),
            "another-token",
            "rider@example.com",
        );
        let duplicate = submit(&client, &address, duplicate_email, "https://cutout.lol").await;
        assert_eq!(duplicate.status(), StatusCode::ACCEPTED);
        let duplicate_receipt: serde_json::Value =
            duplicate.json().await.expect("duplicate receipt");
        assert_ne!(first_receipt["request_id"], duplicate_receipt["request_id"]);
        assert_eq!(verify_calls.load(Ordering::SeqCst), 2);
        let (requests, outbox): (u32, u32) = store
            .call(|connection| {
                Ok((
                    connection
                        .query_row("SELECT count(*) FROM alpha_signup_requests", [], |row| {
                            row.get(0)
                        })
                        .map_err(db_error)?,
                    connection
                        .query_row("SELECT count(*) FROM alpha_signup_outbox", [], |row| {
                            row.get(0)
                        })
                        .map_err(db_error)?,
                ))
            })
            .await
            .expect("durable request");
        assert_eq!((requests, outbox), (1, 1));
        signup_task.abort();
        verify_task.abort();
    }

    #[tokio::test]
    async fn siteverify_mismatch_expiry_and_provider_failure_fail_closed() {
        for (reply, expected) in [
            (
                serde_json::json!({"success": true, "hostname": "wrong.example", "action": SIGNUP_ACTION}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                serde_json::json!({"success": false, "error-codes": ["timeout-or-duplicate"]}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                serde_json::json!({"success": false, "error-codes": ["internal-error"]}),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ] {
            let (verify_url, verify_calls, verify_task) = test_server(reply).await;
            let (address, _store, signup_task, _directory) = signup_server(verify_url).await;
            let client = Client::builder().no_proxy().build().expect("client");
            let response = submit(
                &client,
                &address,
                body(
                    &Uuid::new_v4().to_string(),
                    "opaque-token",
                    "rider@example.com",
                ),
                "https://cutout.lol",
            )
            .await;
            assert_eq!(response.status(), expected);
            assert_eq!(
                verify_calls.load(Ordering::SeqCst),
                if expected == StatusCode::SERVICE_UNAVAILABLE {
                    2
                } else {
                    1
                }
            );
            signup_task.abort();
            verify_task.abort();
        }
    }

    #[tokio::test]
    async fn provider_retry_reuses_siteverify_idempotency_and_sends_only_token_metadata() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen_calls = calls.clone();
        let sent = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let seen_payloads = sent.clone();
        let mock = Router::new().route(
            "/siteverify",
            post(move |Json(body): Json<serde_json::Value>| {
                let attempt = seen_calls.fetch_add(1, Ordering::SeqCst);
                seen_payloads.lock().expect("payload mutex").push(body);
                async move {
                    if attempt == 0 {
                        Json(serde_json::json!({ "success": false, "error-codes": ["internal-error"] }))
                    } else {
                        Json(serde_json::json!({ "success": true, "hostname": "cutout.lol", "action": SIGNUP_ACTION }))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let verify_url = format!(
            "http://{}/siteverify",
            listener.local_addr().expect("mock address")
        );
        let verify_task =
            tokio::spawn(async move { axum::serve(listener, mock).await.expect("mock serve") });
        let (address, _store, signup_task, _directory) = signup_server(verify_url).await;
        let client = Client::builder().no_proxy().build().expect("client");
        let key = Uuid::new_v4().to_string();
        let response = submit(
            &client,
            &address,
            body(&key, "challenge-token", "private@example.com"),
            "https://cutout.lol",
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let payloads = sent.lock().expect("payload mutex");
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0]["idempotency_key"], key);
        assert_eq!(payloads[0], payloads[1]);
        assert_eq!(payloads[0].as_object().expect("siteverify fields").len(), 3);
        assert!(!payloads[0].to_string().contains("private@example.com"));
        assert!(!payloads[0].to_string().contains("Example accessory"));
        signup_task.abort();
        verify_task.abort();
    }

    #[tokio::test]
    async fn default_runtime_config_is_disabled_and_wrong_origin_never_calls_siteverify() {
        let (verify_url, verify_calls, verify_task) = test_server(
            serde_json::json!({"success": true, "hostname": "cutout.lol", "action": SIGNUP_ACTION}),
        )
        .await;
        let (address, _store, signup_task, _directory) = signup_server(verify_url).await;
        let client = Client::builder().no_proxy().build().expect("client");
        let wrong_origin = submit(
            &client,
            &address,
            body(
                &Uuid::new_v4().to_string(),
                "opaque-token",
                "rider@example.com",
            ),
            "https://attacker.example",
        )
        .await;
        assert_eq!(wrong_origin.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(verify_calls.load(Ordering::SeqCst), 0);
        let (disabled_address, disabled_store, disabled_task, _disabled_directory) =
            signup_server_with_state(SignupState::from_env()).await;
        let disabled = public_config(Extension(SignupState::from_env())).await.0;
        assert!(!disabled.enabled);
        assert!(disabled.site_key.is_none());
        let disabled = submit(
            &client,
            &disabled_address,
            body(
                &Uuid::new_v4().to_string(),
                "opaque-token",
                "rider@example.com",
            ),
            "https://cutout.lol",
        )
        .await;
        assert_eq!(disabled.status(), StatusCode::SERVICE_UNAVAILABLE);
        let requests: u32 = disabled_store
            .call(|connection| {
                connection
                    .query_row("SELECT count(*) FROM alpha_signup_requests", [], |row| {
                        row.get(0)
                    })
                    .map_err(db_error)
            })
            .await
            .expect("disabled database read");
        assert_eq!(requests, 0);
        signup_task.abort();
        disabled_task.abort();
        verify_task.abort();
    }

    #[tokio::test]
    async fn signup_source_limit_stops_before_sixth_siteverify_call() {
        let (verify_url, verify_calls, verify_task) = test_server(serde_json::json!({
            "success": true, "hostname": "cutout.lol", "action": SIGNUP_ACTION
        }))
        .await;
        let (address, _store, signup_task, _directory) = signup_server(verify_url).await;
        let client = Client::builder().no_proxy().build().expect("client");
        for index in 0..5 {
            let body = body(
                &Uuid::new_v4().to_string(),
                "opaque-token",
                &format!("rider{index}@example.com"),
            );
            assert_eq!(
                submit(&client, &address, body, "https://cutout.lol")
                    .await
                    .status(),
                StatusCode::ACCEPTED
            );
        }
        let sixth = body(
            &Uuid::new_v4().to_string(),
            "opaque-token",
            "sixth@example.com",
        );
        assert_eq!(
            submit(&client, &address, sixth, "https://cutout.lol")
                .await
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(verify_calls.load(Ordering::SeqCst), 5);
        signup_task.abort();
        verify_task.abort();
    }
}
