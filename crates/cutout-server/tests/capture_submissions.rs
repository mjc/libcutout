use cutout_server::{Store, router};
use reqwest::{Client, Method, Response, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

struct Server {
    address: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start(database: &Path) -> Self {
        let store = Store::open(database).expect("database");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = format!("http://{}", listener.local_addr().expect("address"));
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                router(store).into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .expect("server");
        });
        Self {
            address,
            client: cutout_core::http::client_builder()
                .expect("platform verifier")
                .no_proxy()
                .build()
                .expect("client"),
            task,
        }
    }

    async fn identity(&self) -> String {
        let response = self
            .client
            .post(format!("{}/v1/identities", self.address))
            .send()
            .await
            .expect("register");
        assert_eq!(response.status(), StatusCode::CREATED);
        let identity: Value = response.json().await.expect("identity");
        identity["token"].as_str().expect("token").to_owned()
    }

    async fn request(&self, method: Method, suffix: &str, token: &str) -> Response {
        self.client
            .request(method, format!("{}{suffix}", self.address))
            .bearer_auth(token)
            .send()
            .await
            .expect("request")
    }

    async fn put(&self, operation: &str, token: &str, bytes: &[u8], origin: &str) -> Response {
        self.put_metadata(operation, token, bytes, metadata(bytes, origin))
            .await
    }

    async fn put_metadata(
        &self,
        operation: &str,
        token: &str,
        bytes: &[u8],
        metadata: Value,
    ) -> Response {
        self.client
            .put(format!(
                "{}/v1/capture-submissions/{operation}",
                self.address
            ))
            .bearer_auth(token)
            .header("content-type", "application/x-ndjson")
            .header(
                "x-cutout-capture",
                serde_json::to_string(&metadata).expect("metadata"),
            )
            .body(bytes.to_vec())
            .send()
            .await
            .expect("capture upload")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn expect_status(response: Response, expected: StatusCode) -> Response {
    let actual = response.status();
    if actual == expected {
        return response;
    }
    let rejection = response.text().await.expect("HTTP rejection body");
    panic!("expected HTTP {expected}, got {actual}; server response: {rejection}");
}

fn metadata(bytes: &[u8], origin: &str) -> Value {
    json!({
        "sha256": hex::encode(Sha256::digest(bytes)),
        "bytes": bytes.len(),
        "encoding": "jsonl",
        "origin": origin,
        "integrity": "complete",
        "provenance": {
            "source_artifact_id": "00000000-0000-4000-8000-000000000099",
            "live_capture_id": null,
            "source_sha256": hex::encode(Sha256::digest(bytes)),
            "transformation": "unmodified"
        }
    })
}

/// Unknown device, no decoder or GPS, with both an observation and an inert captured write.
fn capture() -> Vec<u8> {
    let template = include_str!("../../cutout-cli/fixtures/pevcap/falcon-begode-banner.jsonl");
    let mut header: Value =
        serde_json::from_str(template.lines().next().expect("fixture header")).expect("header");
    header["version"]["minor"] = json!(cutout_core::PevcapFormatVersion::current().minor);
    header["header"]["platform_id"] = json!("unverified-peripheral");
    header["header"]["selected_session_key"] = Value::Null;
    header["header"]["resolved_identity"] = Value::Null;
    header["header"]["annotations"] = json!([
        "capture_privacy=private",
        "capture_evidence=unverified",
        "description=Pressed the physical lamp button"
    ]);
    let outbound = json!({"kind":"record","record":{
        "monotonic_ms":0,"direction":"Outbound",
        "characteristic":[0,0,255,225,0,0,16,0,128,0,0,128,95,155,52,251],
        "service":null,"write_mode":"WithoutResponse","bytes":[78]
    }});
    let inbound = json!({"kind":"record","record":{
        "monotonic_ms":1,"direction":"Inbound",
        "characteristic":[0,0,255,225,0,0,16,0,128,0,0,128,95,155,52,251],
        "service":[0,0,255,224,0,0,16,0,128,0,0,128,95,155,52,251],
        "write_mode":null,"bytes":[255,7,0,128]
    }});
    let encoded = format!("{header}\n{outbound}\n{inbound}\n");
    cutout_core::PevcapCapture::from_jsonl(&encoded)
        .expect("HTTP fixture must satisfy the actual PEVCAP schema");
    encoded.into_bytes()
}

fn provision(database: &Path, flag: &str) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cutout-server"))
        .arg("--database")
        .arg(database)
        .args(["identity", flag])
        .output()
        .expect("operator provisioning");
    assert!(output.status.success(), "operator capability provisioning");
    let identity: Value = serde_json::from_slice(&output.stdout).expect("operator identity");
    identity["token"].as_str().expect("token").to_owned()
}

#[tokio::test]
async fn private_capture_unknown_protocol_round_trips_exactly_after_restart() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let bytes = capture();
    // Automatic is provenance, not an automatic transfer or a reason to reject finished evidence.
    let response = expect_status(
        server.put(&operation, &owner, &bytes, "automatic").await,
        StatusCode::CREATED,
    )
    .await;
    let receipt: Value = response.json().await.expect("receipt");
    assert_eq!(receipt["operation_id"], operation);
    assert_eq!(receipt["sha256"], metadata(&bytes, "automatic")["sha256"]);
    assert_eq!(receipt["bytes"], bytes.len());
    assert!(receipt["received_at_milliseconds"].as_u64().is_some());
    drop(server);

    let restarted = Server::start(&database).await;
    let path = format!("/v1/capture-submissions/{operation}");
    let readback = restarted.request(Method::GET, &path, &owner).await;
    assert_eq!(readback.status(), StatusCode::OK);
    assert_eq!(readback.json::<Value>().await.expect("readback"), receipt);
    let content = restarted
        .request(Method::GET, &format!("{path}/content"), &owner)
        .await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(
        content.bytes().await.expect("capture bytes").as_ref(),
        bytes
    );
}

#[tokio::test]
async fn private_capture_retries_recover_one_receipt_and_changed_content_conflicts() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("server.sqlite")).await;
    let owner = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let bytes = capture();
    let first = server.put(&operation, &owner, &bytes, "manual").await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let receipt: Value = first.json().await.expect("receipt");
    let retry = server.put(&operation, &owner, &bytes, "manual").await;
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(retry.json::<Value>().await.expect("retry receipt"), receipt);
    let mut changed = bytes;
    changed.extend_from_slice(b"\n");
    assert_eq!(
        server
            .put(&operation, &owner, &changed, "manual")
            .await
            .status(),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn private_capture_wrong_owner_and_charging_moderator_cannot_read_evidence() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let stranger = server.identity().await;
    let moderator = provision(&database, "--moderator");
    let operation = Uuid::new_v4().to_string();
    assert_eq!(
        server
            .put(&operation, &owner, &capture(), "manual")
            .await
            .status(),
        StatusCode::CREATED
    );
    let path = format!("/v1/capture-submissions/{operation}");
    for token in [&stranger, &moderator] {
        for (method, suffix) in [
            (Method::GET, path.clone()),
            (Method::GET, format!("{path}/content")),
            (Method::DELETE, path.clone()),
        ] {
            assert_eq!(
                server.request(method, &suffix, token).await.status(),
                StatusCode::NOT_FOUND
            );
        }
        assert_eq!(
            server
                .request(Method::GET, "/v1/capture-submissions", token)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let reviewer = provision(&database, "--private-capture-reviewer");
    assert_eq!(
        server
            .request(Method::GET, "/v1/capture-submissions", &reviewer)
            .await
            .status(),
        StatusCode::OK
    );
    let content = server
        .request(Method::GET, &format!("{path}/content"), &reviewer)
        .await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(
        content.bytes().await.expect("review bytes").as_ref(),
        capture()
    );
}

#[tokio::test]
async fn private_capture_preupload_cancellation_prevents_late_publication() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let path = format!("/v1/capture-submissions/{operation}");
    assert!(
        server
            .request(Method::DELETE, &path, &owner)
            .await
            .status()
            .is_success()
    );
    drop(server);
    let restarted = Server::start(&database).await;
    assert_eq!(
        restarted
            .put(&operation, &owner, &capture(), "manual")
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        restarted
            .request(Method::GET, &format!("{path}/content"), &owner)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn private_capture_account_deletion_removes_private_content() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    expect_status(
        server.put(&operation, &owner, &capture(), "manual").await,
        StatusCode::CREATED,
    )
    .await;
    let reviewer = provision(&database, "--private-capture-reviewer");
    assert_eq!(
        server
            .request(Method::DELETE, "/v1/identities/current", &owner)
            .await
            .status(),
        StatusCode::OK
    );
    let path = format!("/v1/capture-submissions/{operation}/content");
    assert_eq!(
        server.request(Method::GET, &path, &reviewer).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        server.request(Method::GET, &path, &owner).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn private_capture_rejects_music_and_corruption_without_publishing() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("server.sqlite")).await;
    let owner = server.identity().await;
    let music = json!({"provider":"apple_music","track_id":"private-track",
        "monotonic_at_ms":1,"wall_clock_unix_ms":1,"clock_uncertainty_milliseconds":0,"ride_sequence":null});
    let original = String::from_utf8(capture()).expect("fixture text");
    let mut lines = original
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("line"))
        .collect::<Vec<_>>();
    lines[2]["record"]["music"] = music.clone();
    let embedded = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let independent = format!("{original}{}\n", json!({"kind":"music","music":music})).into_bytes();
    for (bytes, error_code) in [
        (embedded, "capture_music_prohibited"),
        (independent, "capture_invalid"),
    ] {
        let operation = Uuid::new_v4().to_string();
        let response = server.put(&operation, &owner, &bytes, "manual").await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.json::<Value>().await.expect("error")["error"],
            error_code
        );
        assert_eq!(
            server
                .request(
                    Method::GET,
                    &format!("/v1/capture-submissions/{operation}"),
                    &owner
                )
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    let bytes = capture();
    let mut metadata = metadata(&bytes, "manual");
    metadata["sha256"] = json!("a".repeat(64));
    metadata["provenance"]["source_sha256"] = metadata["sha256"].clone();
    let operation = Uuid::new_v4().to_string();
    let response = server
        .put_metadata(&operation, &owner, &bytes, metadata)
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<Value>().await.expect("error")["error"],
        "capture_integrity_failed"
    );
}

#[tokio::test]
async fn private_capture_has_separate_body_limit_and_retains_disclosed_loss() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("server.sqlite")).await;
    let owner = server.identity().await;
    let mut bytes = capture();
    let record = String::from_utf8(bytes.clone())
        .expect("fixture")
        .lines()
        .last()
        .expect("record")
        .to_owned();
    for _ in 0..300 {
        bytes.extend_from_slice(record.as_bytes());
        bytes.push(b'\n');
    }
    assert!(bytes.len() > 32 * 1024);
    let mut metadata = metadata(&bytes, "manual");
    metadata["integrity"] = json!({"incomplete":{"dropped_messages":7}});
    let operation = Uuid::new_v4().to_string();
    assert_eq!(
        server
            .put_metadata(&operation, &owner, &bytes, metadata.clone())
            .await
            .status(),
        StatusCode::CREATED
    );
    let retry = server
        .put_metadata(&operation, &owner, &bytes, metadata)
        .await;
    assert_eq!(retry.status(), StatusCode::OK);
}

#[tokio::test]
async fn private_capture_deleted_submission_cannot_be_recreated_or_read() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("server.sqlite")).await;
    let owner = server.identity().await;
    let stranger = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let bytes = capture();
    assert_eq!(
        server
            .put(&operation, &owner, &bytes, "manual")
            .await
            .status(),
        StatusCode::CREATED
    );
    let path = format!("/v1/capture-submissions/{operation}");
    assert_eq!(
        server
            .put(&operation, &stranger, &bytes, "manual")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert!(
        server
            .request(Method::DELETE, &path, &owner)
            .await
            .status()
            .is_success()
    );
    assert!(
        server
            .request(Method::DELETE, &path, &owner)
            .await
            .status()
            .is_success()
    );
    assert_eq!(
        server
            .put(&operation, &owner, &bytes, "manual")
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        server.request(Method::GET, &path, &owner).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        server
            .request(Method::GET, &format!("{path}/content"), &owner)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn private_capture_authentication_limits_and_format_errors_are_classified() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("server.sqlite")).await;
    let owner = server.identity().await;
    assert_eq!(
        server
            .put(
                &Uuid::new_v4().to_string(),
                "revoked-or-invalid",
                &capture(),
                "manual"
            )
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let mut too_large = metadata(&capture(), "manual");
    too_large["bytes"] = json!(512_u64 * 1024 * 1024 + 1);
    assert_eq!(
        server
            .put_metadata(&Uuid::new_v4().to_string(), &owner, &capture(), too_large)
            .await
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut lines = String::from_utf8(capture())
        .expect("fixture")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("line"))
        .collect::<Vec<_>>();
    lines[0]["version"]["major"] = json!(999);
    let unsupported = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let response = server
        .put(&Uuid::new_v4().to_string(), &owner, &unsupported, "manual")
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<Value>().await.expect("error")["error"],
        "capture_unsupported_format"
    );
    let malformed = b"not a PEVCAP file\n";
    let response = server
        .put(&Uuid::new_v4().to_string(), &owner, malformed, "manual")
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<Value>().await.expect("error")["error"],
        "capture_invalid"
    );
}

#[tokio::test]
async fn private_capture_rejects_cloudflare_oversize_before_reading_body() {
    use tokio::io::AsyncWriteExt;

    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let mut metadata = metadata(&capture(), "manual");
    metadata["bytes"] = json!(100_000_001_u64);
    let encoded = serde_json::to_string(&metadata).expect("metadata");
    let mut connection =
        tokio::net::TcpStream::connect(server.address.trim_start_matches("http://"))
            .await
            .expect("raw upload");
    let headers = format!(
        "PUT /v1/capture-submissions/{operation} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {owner}\r\nContent-Type: application/x-ndjson\r\nx-cutout-capture: {encoded}\r\nContent-Length: 100000001\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n"
    );
    connection
        .write_all(headers.as_bytes())
        .await
        .expect("headers only; no capture body is sent");
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        combined_capture_headers(&mut connection),
    )
    .await
    .expect("oversized request must be refused without waiting for its body");
    assert!(
        response.starts_with(b"HTTP/1.1 413 "),
        "must reject the declared body above the actual ingress limit before 100 Continue: {}",
        String::from_utf8_lossy(&response)
    );
    drop(connection);
    let path = format!("/v1/capture-submissions/{operation}");
    expect_status(
        server.request(Method::GET, &path, &owner).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let inspector = rusqlite::Connection::open(&database).expect("inspector");
    let count: u64 = inspector
        .query_row(
            "SELECT count(*) FROM capture_submissions WHERE operation_id=?1",
            [&operation],
            |row| row.get(0),
        )
        .expect("no capture publication");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn private_capture_failed_storage_publishes_nothing_and_preserves_retry() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let connection = rusqlite::Connection::open(&database).expect("database");
    connection.execute_batch("CREATE TRIGGER fail_capture BEFORE INSERT ON capture_submissions BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").expect("failure trigger");
    assert_eq!(
        server
            .put(&operation, &owner, &capture(), "manual")
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let count: u64 = connection
        .query_row("SELECT count(*) FROM capture_submissions", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(count, 0);
    connection
        .execute_batch("DROP TRIGGER fail_capture")
        .expect("remove trigger");
    assert_eq!(
        server
            .put(&operation, &owner, &capture(), "manual")
            .await
            .status(),
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn private_capture_cancel_during_body_stream_blocks_finalize() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    for _ in 0..2 {
        expect_status(
            server
                .put(&Uuid::new_v4().to_string(), &owner, &capture(), "manual")
                .await,
            StatusCode::CREATED,
        )
        .await;
    }
    let inspector = rusqlite::Connection::open(&database).expect("inspector");
    let owner_id: String = inspector
        .query_row(
            "SELECT id FROM contributors WHERE token_hash=?1",
            [hex::encode(Sha256::digest(owner.as_bytes()))],
            |row| row.get(0),
        )
        .expect("owner identity");
    let principal = format!("capture-upload:{owner_id}");
    let (previous_window, previous_attempts): (u64, u64) = inspector
        .query_row(
            "SELECT window_start_ms,count FROM quotas WHERE principal=?1 AND period_ms=3600000",
            [&principal],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("two completed upload admissions");
    let operation = Uuid::new_v4().to_string();
    let bytes = capture();
    let metadata = serde_json::to_string(&metadata(&bytes, "manual")).expect("metadata");
    let mut connection =
        tokio::net::TcpStream::connect(server.address.trim_start_matches("http://"))
            .await
            .expect("raw upload");
    let header = format!(
        "PUT /v1/capture-submissions/{operation} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-ndjson\r\nAuthorization: Bearer {owner}\r\nx-cutout-capture: {metadata}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    connection
        .write_all(header.as_bytes())
        .await
        .expect("headers");
    let half = bytes.len() / 2;
    connection
        .write_all(&bytes[..half])
        .await
        .expect("partial body");
    // The durable attempt budget establishes that the request reached authenticated admission.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let (window,attempts):(u64,u64)=inspector.query_row(
                "SELECT window_start_ms,count FROM quotas WHERE principal=?1 AND period_ms=3600000",
                [&principal],|row|Ok((row.get(0)?,row.get(1)?))).expect("owner hourly budget");
            if window != previous_window || attempts > previous_attempts {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("upload admitted");
    let path = format!("/v1/capture-submissions/{operation}");
    assert_eq!(
        server.request(Method::DELETE, &path, &owner).await.status(),
        StatusCode::NO_CONTENT
    );
    connection
        .write_all(&bytes[half..])
        .await
        .expect("remaining body");
    let mut response = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        connection.read_to_string(&mut response),
    )
    .await
    .expect("response timeout")
    .expect("response");
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert_eq!(
        server
            .request(Method::GET, &format!("{path}/content"), &owner)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn private_capture_requires_matching_media_type_and_accepts_binary() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("server.sqlite")).await;
    let owner = server.identity().await;
    let bytes = capture();
    for content_type in [
        None,
        Some("application/json"),
        Some("application/octet-stream"),
    ] {
        let operation = Uuid::new_v4().to_string();
        let request = server
            .client
            .put(format!(
                "{}/v1/capture-submissions/{operation}",
                server.address
            ))
            .bearer_auth(&owner)
            .header(
                "x-cutout-capture",
                serde_json::to_string(&metadata(&bytes, "manual")).expect("metadata"),
            )
            .body(bytes.clone());
        let request = if let Some(content_type) = content_type {
            request.header("content-type", content_type)
        } else {
            request
        };
        assert_eq!(
            request.send().await.expect("request").status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let decoded =
        cutout_core::PevcapCapture::from_jsonl(std::str::from_utf8(&bytes).expect("fixture text"))
            .expect("capture");
    let binary = decoded.to_binary().expect("binary encoding");
    let mut metadata = metadata(&binary, "manual");
    metadata["encoding"] = json!("binary");
    let operation = Uuid::new_v4().to_string();
    let response = server
        .client
        .put(format!(
            "{}/v1/capture-submissions/{operation}",
            server.address
        ))
        .bearer_auth(&owner)
        .header("content-type", "application/octet-stream")
        .header(
            "x-cutout-capture",
            serde_json::to_string(&metadata).expect("metadata"),
        )
        .body(binary.clone())
        .send()
        .await
        .expect("binary upload");
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = server
        .request(
            Method::GET,
            &format!("/v1/capture-submissions/{operation}/content"),
            &owner,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.bytes().await.expect("readback").as_ref(), binary);
}

#[tokio::test]
async fn private_capture_review_capability_is_rechecked_after_revocation() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let reviewer = provision(&database, "--private-capture-reviewer");
    let operation = Uuid::new_v4().to_string();
    assert_eq!(
        server
            .put(&operation, &owner, &capture(), "manual")
            .await
            .status(),
        StatusCode::CREATED
    );
    let path = format!("/v1/capture-submissions/{operation}/content");
    assert_eq!(
        server.request(Method::GET, &path, &reviewer).await.status(),
        StatusCode::OK
    );
    let connection = rusqlite::Connection::open(&database).expect("database");
    connection
        .execute(
            "UPDATE contributors SET private_capture_review=0 WHERE token_hash=?1",
            [hex::encode(Sha256::digest(reviewer.as_bytes()))],
        )
        .expect("remove capability");
    assert_eq!(
        server.request(Method::GET, &path, &reviewer).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        server
            .request(Method::GET, "/v1/capture-submissions", &reviewer)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    connection
        .execute(
            "UPDATE contributors SET revoked_at_ms=1 WHERE token_hash=?1",
            [hex::encode(Sha256::digest(owner.as_bytes()))],
        )
        .expect("revoke owner");
    assert_eq!(
        server.request(Method::GET, &path, &owner).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn private_capture_and_signup_moderation_keep_independent_permissions() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("combined.sqlite");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let moderator = provision(&database, "--moderator");
    let reviewer = provision(&database, "--private-capture-reviewer");
    assert_eq!(
        server
            .client
            .get(format!("{}/readyz", server.address))
            .send()
            .await
            .expect("readiness")
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .client
            .get(format!("{}/v1/alpha-signup/config", server.address))
            .send()
            .await
            .expect("signup config")
            .status(),
        StatusCode::OK
    );
    let operation = Uuid::new_v4().to_string();
    let bytes = capture();
    expect_status(
        server.put(&operation, &owner, &bytes, "manual").await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        server
            .request(Method::GET, "/v1/moderation/alpha-signups", &moderator)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .request(Method::GET, "/v1/moderation/alpha-signups", &reviewer)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        server
            .request(Method::GET, "/v1/capture-submissions", &moderator)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        server
            .request(Method::GET, "/v1/capture-submissions", &reviewer)
            .await
            .status(),
        StatusCode::OK
    );
    let response = server
        .request(
            Method::GET,
            &format!("/v1/capture-submissions/{operation}/content"),
            &reviewer,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.bytes().await.expect("private content").as_ref(),
        bytes
    );
}

struct CombinedCaptureServer {
    child: std::process::Child,
    address: std::net::SocketAddr,
    client: Client,
}

fn combined_capture_music_fixture(directory: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let credentials = directory.join("credentials");
    std::fs::create_dir(&credentials).expect("combined credentials directory");
    std::fs::write(
        credentials.join("soundcloud.json"),
        br#"{"client_id":"fixture-client","client_secret":"fixture-secret"}"#,
    )
    .expect("combined credentials");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let state = directory.join("music-tokens.json");
    std::fs::write(
        &state,
        serde_json::to_vec(&json!({
            "cached": {
                "access_token": "fixture-cached-access-token",
                "refresh_token": "fixture-unused-refresh-token",
                "expires_at": now + 86_400
            },
            "upstream_attempts": [],
            "client_token_attempts": [],
            "next_attempt_at": now + 3_600
        }))
        .expect("cached music state"),
    )
    .expect("music state file");
    (credentials, state)
}

impl CombinedCaptureServer {
    async fn start(directory: &Path, database: &Path) -> Self {
        use std::{
            net::TcpListener,
            process::{Command, Stdio},
            time::Duration,
        };
        let (credentials, state) = combined_capture_music_fixture(directory);
        let reserved = TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let address = reserved.local_addr().expect("reserved address");
        drop(reserved);
        let child = Command::new(env!("CARGO_BIN_EXE_cutout-server"))
            .arg("--database")
            .arg(database)
            .args(["serve", "--ride-database"])
            .arg(directory.join("rides.sqlite"))
            .args(["--listen"])
            .arg(address.to_string())
            .args(["--trusted-proxy", "127.0.0.1"])
            .env("CREDENTIALS_DIRECTORY", credentials)
            .env("SOUNDCLOUD_TOKEN_STATE_FILE", state)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("combined capture server");
        let mut server = Self {
            child,
            address,
            client: cutout_core::http::client_builder()
                .expect("platform verifier")
                .no_proxy()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("combined client"),
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = server.child.try_wait().expect("server status") {
                use std::io::Read;
                let mut stderr = String::new();
                if let Some(mut pipe) = server.child.stderr.take() {
                    pipe.read_to_string(&mut stderr)
                        .expect("startup diagnostics");
                }
                panic!("combined server exited before readiness ({status}): {stderr}");
            }
            if let Ok(response) = server
                .request(Method::GET, "music.cutout.lol", "/readyz", "")
                .send()
                .await
                && response.status() == StatusCode::OK
            {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "combined server readiness deadline"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        for host in ["rides.cutout.lol", "cutout.lol"] {
            expect_status(
                server
                    .request(Method::GET, host, "/readyz", "")
                    .send()
                    .await
                    .expect("host readiness"),
                StatusCode::OK,
            )
            .await;
        }
        server
    }

    fn request(
        &self,
        method: Method,
        host: &str,
        path: &str,
        token: &str,
    ) -> reqwest::RequestBuilder {
        let request = self
            .client
            .request(method, format!("http://{}{path}", self.address))
            .header("host", host);
        if token.is_empty() {
            request
        } else {
            request.bearer_auth(token)
        }
    }
}

impl Drop for CombinedCaptureServer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

async fn combined_capture_headers(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let byte = stream.read_u8().await.expect("HTTP response header byte");
        bytes.push(byte);
        assert!(bytes.len() <= 16 * 1024, "bounded response headers");
    }
    bytes
}

async fn combined_capture_slow_put(
    server: &CombinedCaptureServer,
    operation: &str,
    owner: &str,
    bytes: &[u8],
) -> Value {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("slow upload connection");
    let encoded = serde_json::to_string(&metadata(bytes, "manual")).expect("slow upload metadata");
    let headers = format!(
        "PUT /v1/capture-submissions/{operation} HTTP/1.1\r\nHost: rides.cutout.lol\r\nAuthorization: Bearer {owner}\r\nContent-Type: application/x-ndjson\r\nx-cutout-capture: {encoded}\r\nContent-Length: {}\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream
        .write_all(headers.as_bytes())
        .await
        .expect("slow upload headers");
    let interim = tokio::time::timeout(
        Duration::from_secs(5),
        combined_capture_headers(&mut stream),
    )
    .await
    .expect("body admission response");
    assert!(
        interim.starts_with(b"HTTP/1.1 100 Continue\r\n"),
        "rides capture PUT must reach its body handler before publication: {}",
        String::from_utf8_lossy(&interim)
    );
    let half = bytes.len() / 2;
    stream
        .write_all(&bytes[..half])
        .await
        .expect("first capture body fragment");
    // Cross the directory's five-second deadline, within the capture idle budget.
    tokio::time::sleep(Duration::from_secs(6)).await;
    stream
        .write_all(&bytes[half..])
        .await
        .expect("final capture body fragment");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut response))
        .await
        .expect("capture publication deadline")
        .expect("capture publication response");
    assert!(
        response.starts_with(b"HTTP/1.1 201 Created\r\n"),
        "capture must outlive the directory deadline and publish: {}",
        String::from_utf8_lossy(&response)
    );
    let start = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .expect("publication header terminator")
        + 4;
    serde_json::from_slice(&response[start..]).expect("publication receipt")
}

#[tokio::test]
async fn private_capture_combined_rides_host_streams_past_directory_deadline_and_isolates_review() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let moderator = provision(&database, "--moderator");
    let reviewer = provision(&database, "--private-capture-reviewer");
    let server = CombinedCaptureServer::start(directory.path(), &database).await;
    let identity = expect_status(
        server
            .request(Method::POST, "rides.cutout.lol", "/v1/identities", "")
            .header("x-real-ip", "192.0.2.1")
            .send()
            .await
            .expect("rides registration"),
        StatusCode::CREATED,
    )
    .await
    .json::<Value>()
    .await
    .expect("owner identity");
    let owner = identity["token"].as_str().expect("owner bearer");
    let mut bytes = capture();
    let record = std::str::from_utf8(&bytes)
        .expect("capture text")
        .lines()
        .last()
        .expect("record")
        .to_owned();
    for _ in 0..300 {
        bytes.extend_from_slice(record.as_bytes());
        bytes.push(b'\n');
    }
    assert!(bytes.len() > 32 * 1024);
    cutout_core::validate_capture_submission(bytes.as_slice(), cutout_core::PevcapEncoding::Jsonl)
        .expect("valid music-free upload fixture");
    let operation = Uuid::new_v4().to_string();
    let receipt = combined_capture_slow_put(&server, &operation, owner, &bytes).await;
    assert_eq!(receipt["operation_id"], operation);
    assert_eq!(receipt["sha256"], metadata(&bytes, "manual")["sha256"]);
    assert_eq!(receipt["bytes"], bytes.len());
    assert!(
        receipt["received_at_milliseconds"]
            .as_u64()
            .is_some_and(|time| time > 0)
    );
    let path = format!("/v1/capture-submissions/{operation}");
    let recovered = expect_status(
        server
            .request(Method::GET, "rides.cutout.lol", &path, owner)
            .send()
            .await
            .expect("receipt lookup"),
        StatusCode::OK,
    )
    .await
    .json::<Value>()
    .await
    .expect("durable receipt");
    assert_eq!(recovered, receipt);
    let content_path = format!("{path}/content");
    assert_combined_capture_review(
        &server,
        owner,
        &reviewer,
        &moderator,
        &content_path,
        &bytes,
        &receipt,
    )
    .await;
    assert_combined_capture_host_isolation(&server, &path, &content_path, &reviewer).await;
}

async fn assert_combined_capture_review(
    server: &CombinedCaptureServer,
    owner: &str,
    reviewer: &str,
    moderator: &str,
    content_path: &str,
    bytes: &[u8],
    receipt: &Value,
) {
    for token in [owner, reviewer] {
        let response = expect_status(
            server
                .request(Method::GET, "rides.cutout.lol", content_path, token)
                .send()
                .await
                .expect("private content"),
            StatusCode::OK,
        )
        .await;
        assert_eq!(
            response.bytes().await.expect("exact payload").as_ref(),
            bytes
        );
    }
    expect_status(
        server
            .request(Method::GET, "rides.cutout.lol", content_path, moderator)
            .send()
            .await
            .expect("moderator content"),
        StatusCode::NOT_FOUND,
    )
    .await;
    expect_status(
        server
            .request(
                Method::GET,
                "rides.cutout.lol",
                "/v1/capture-submissions",
                moderator,
            )
            .send()
            .await
            .expect("moderator list"),
        StatusCode::FORBIDDEN,
    )
    .await;
    let page = expect_status(
        server
            .request(
                Method::GET,
                "rides.cutout.lol",
                "/v1/capture-submissions",
                reviewer,
            )
            .send()
            .await
            .expect("reviewer list"),
        StatusCode::OK,
    )
    .await
    .json::<Value>()
    .await
    .expect("private list");
    assert_eq!(
        page["submissions"].as_array().expect("submissions").len(),
        1
    );
    assert_eq!(&page["submissions"][0]["receipt"], receipt);
}

async fn assert_combined_capture_host_isolation(
    server: &CombinedCaptureServer,
    path: &str,
    content_path: &str,
    reviewer: &str,
) {
    for host in ["cutout.lol", "music.cutout.lol"] {
        for (method, route) in [
            (Method::GET, "/v1/capture-submissions"),
            (Method::GET, path),
            (Method::GET, content_path),
            (Method::PUT, path),
            (Method::DELETE, path),
        ] {
            expect_status(
                server
                    .request(method, host, route, reviewer)
                    .send()
                    .await
                    .expect("other host capture route"),
                StatusCode::NOT_FOUND,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn private_capture_reported_settings_round_trip_exactly_and_stay_private() {
    let directory = TempDir::new().expect("directory");
    let database = directory.path().join("server.sqlite");
    let mut capture =
        cutout_core::PevcapCapture::decode(&capture(), cutout_core::PevcapEncoding::Jsonl)
            .expect("capture");
    capture.version = cutout_core::PevcapFormatVersion::current();
    let first = cutout_core::CaptureSettingsObservation {
        capture_id: Uuid::new_v4().to_string(),
        profile: cutout_core::CaptureResearchProfile {
            profile_id: "nosfet_aero".into(),
            pev_category: Some("euc".into()),
            model: Some("Unverified manual profile".into()),
            firmware: None,
            selection: cutout_core::CaptureResearchProfileSelection::Manual,
        },
        observed_at_ms: 10,
        recorded_at_ms: 100,
        source: cutout_core::CaptureObservationSource::ManufacturerApp,
        values: vec![
            cutout_core::CaptureSettingObservationValue {
                setting_id: "pwm_tiltback".into(),
                value: cutout_core::CaptureObservedValue::Off,
                unit: Some("%".into()),
                note: None,
            },
            cutout_core::CaptureSettingObservationValue {
                setting_id: "display_brightness".into(),
                value: cutout_core::CaptureObservedValue::Entered("0".into()),
                unit: None,
                note: Some("Screen displayed zero".into()),
            },
        ],
        note: Some("Before switching display units".into()),
    };
    let mut second = first.clone();
    second.observed_at_ms = 150;
    second.recorded_at_ms = 200;
    second.values[0].value = cutout_core::CaptureObservedValue::Entered("125.00".into());
    capture.settings_observations = vec![first.clone(), second.clone()];
    let bytes = capture
        .encode(cutout_core::PevcapEncoding::Jsonl)
        .expect("encode observations");
    let server = Server::start(&database).await;
    let owner = server.identity().await;
    let stranger = server.identity().await;
    let reviewer = provision(&database, "--private-capture-reviewer");
    let operation = Uuid::new_v4().to_string();
    expect_status(
        server.put(&operation, &owner, &bytes, "manual").await,
        StatusCode::CREATED,
    )
    .await;
    drop(server);
    let restarted = Server::start(&database).await;
    let content_path = format!("/v1/capture-submissions/{operation}/content");
    for token in [&owner, &reviewer] {
        let response = restarted.request(Method::GET, &content_path, token).await;
        assert_eq!(response.status(), StatusCode::OK);
        let returned = response.bytes().await.expect("private capture");
        assert_eq!(returned.as_ref(), bytes);
        let decoded =
            cutout_core::PevcapCapture::decode(&returned, cutout_core::PevcapEncoding::Jsonl)
                .expect("decode settings");
        assert_eq!(
            decoded.settings_observations,
            vec![first.clone(), second.clone()]
        );
    }
    assert_eq!(
        restarted
            .request(Method::GET, &content_path, &stranger)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let mut rejected = bytes.clone();
    rejected.extend_from_slice(b"{\"kind\":\"music\",\"music\":{\"provider\":\"apple_music\",\"track_id\":\"prohibited\"}}\n");
    let response = restarted
        .put(&Uuid::new_v4().to_string(), &owner, &rejected, "manual")
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<Value>().await.expect("music rejection")["error"],
        "capture_invalid"
    );
}
