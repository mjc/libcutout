use std::{
    net::{SocketAddr, TcpListener},
    process::{Child, Command, Output, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Response, StatusCode, header::HOST};
use serde_json::Value;
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Instant, sleep, timeout},
};

const MUSIC_HOST: &str = "music.cutout.lol";
const RIDES_HOST: &str = "rides.cutout.lol";
const SITE_HOST: &str = "cutout.lol";
const ACCESS_TOKEN: &str = "cached-mobile-access-token";

#[tokio::test]
async fn combined_runtime_routes_isolates_and_drains_with_cached_music_state() {
    let fixture = RuntimeFixture::new();
    let address = SocketAddr::from(([127, 0, 0, 1], reserve_port()));
    let mut server = start_server(
        address,
        &fixture.database_path,
        &fixture.credentials_directory,
        &fixture.state_path,
    );
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("HTTP client");

    wait_until_ready(&mut server, &client).await;
    assert_service_health_and_readiness(&client, address).await;
    assert_music_token_and_route_isolation(&client, address, &fixture).await;
    assert_charging_authentication(&client, address).await;
    assert_readiness_isolation(&client, address, &fixture).await;
    let drained_identity = finish_inflight_identity(&mut server, address).await;
    let output = server.wait_for_exit().await;
    assert!(
        output.status.success(),
        "server did not exit cleanly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(&fixture.state_path).expect("preserved token state"),
        fixture.initial_cache
    );
    assert_drained_identity_is_durable(&fixture.database_path, &drained_identity);
}

struct RuntimeFixture {
    _directory: TempDir,
    credentials_directory: std::path::PathBuf,
    database_path: std::path::PathBuf,
    initial_cache: Vec<u8>,
    state_path: std::path::PathBuf,
}

impl RuntimeFixture {
    fn new() -> Self {
        let directory = TempDir::new().expect("test directory");
        let credentials_directory = directory.path().join("credentials");
        std::fs::create_dir(&credentials_directory).expect("credential directory");
        std::fs::write(
            credentials_directory.join("soundcloud.json"),
            br#"{"client_id":"fixture-client","client_secret":"fixture-secret"}"#,
        )
        .expect("credentials");

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_secs();
        let state_path = directory.path().join("tokens.json");
        let initial_cache = serde_json::to_vec(&serde_json::json!({
            "cached": {
                "access_token": ACCESS_TOKEN,
                "refresh_token": "never-return-or-log-this",
                "expires_at": now + 86_400,
            },
            "upstream_attempts": [now - 30],
            "client_token_attempts": [now - 60],
            "next_attempt_at": now + 3_600,
        }))
        .expect("serialize fixture cache");
        std::fs::write(&state_path, &initial_cache).expect("cached token state");

        Self {
            database_path: directory.path().join("charging.sqlite"),
            _directory: directory,
            credentials_directory,
            initial_cache,
            state_path,
        }
    }
}

async fn assert_service_health_and_readiness(client: &Client, address: SocketAddr) {
    for (host, health_status) in [
        (MUSIC_HOST, StatusCode::NO_CONTENT),
        (RIDES_HOST, StatusCode::OK),
        (SITE_HOST, StatusCode::OK),
    ] {
        assert_eq!(
            request(client, address, host, "/healthz").await.status(),
            health_status
        );
        assert_eq!(
            request(client, address, host, "/readyz").await.status(),
            StatusCode::OK
        );
    }
}

async fn assert_music_token_and_route_isolation(
    client: &Client,
    address: SocketAddr,
    fixture: &RuntimeFixture,
) {
    let response = request(client, address, MUSIC_HOST, "/v1/token").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let token: Value = response.json().await.expect("music token JSON");
    assert_eq!(token["access_token"], ACCESS_TOKEN);
    assert_eq!(token["token_type"], "Bearer");
    assert!(token["expires_in"].as_u64().is_some_and(|ttl| ttl > 0));
    assert_eq!(token.as_object().expect("token object").len(), 3);
    assert!(token.get("refresh_token").is_none());

    assert_eq!(
        request(client, address, RIDES_HOST, "/v1/charging-spots")
            .await
            .status(),
        StatusCode::OK
    );
    for (host, path) in [
        (RIDES_HOST, "/v1/token"),
        (MUSIC_HOST, "/v1/charging-spots"),
        (SITE_HOST, "/v1/token"),
    ] {
        assert_eq!(
            request(client, address, host, path).await.status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        std::fs::read(&fixture.state_path).expect("unchanged token state"),
        fixture.initial_cache
    );
}

async fn assert_charging_authentication(client: &Client, address: SocketAddr) {
    let identity = client
        .post(format!("http://{address}/v1/identities"))
        .header(HOST, RIDES_HOST)
        .header("x-real-ip", "192.0.2.1")
        .body("")
        .send()
        .await
        .expect("create charging identity");
    assert_eq!(identity.status(), StatusCode::CREATED);
    let identity: Value = identity.json().await.expect("charging identity JSON");
    let token = identity["token"].as_str().expect("identity token");

    let deleted = client
        .delete(format!("http://{address}/v1/identities/current"))
        .header(HOST, RIDES_HOST)
        .header("x-real-ip", "192.0.2.1")
        .bearer_auth(token)
        .body("")
        .send()
        .await
        .expect("delete charging identity");
    assert_eq!(deleted.status(), StatusCode::OK);
    let receipt: Value = deleted.json().await.expect("deletion receipt JSON");
    let repeated = client
        .delete(format!("http://{address}/v1/identities/current"))
        .header(HOST, RIDES_HOST)
        .header("x-real-ip", "192.0.2.1")
        .bearer_auth(token)
        .body("")
        .send()
        .await
        .expect("retry charging identity deletion");
    assert_eq!(repeated.status(), StatusCode::OK);
    let repeated_receipt: Value = repeated
        .json()
        .await
        .expect("replayed deletion receipt JSON");
    assert_eq!(repeated_receipt, receipt);

    let revoked = client
        .get(format!(
            "http://{address}/v1/charging-spots/00000000-0000-4000-8000-000000000111/vote"
        ))
        .header(HOST, RIDES_HOST)
        .header("x-real-ip", "192.0.2.1")
        .bearer_auth(token)
        .send()
        .await
        .expect("use revoked charging identity");
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}

async fn assert_readiness_isolation(
    client: &Client,
    address: SocketAddr,
    fixture: &RuntimeFixture,
) {
    let database =
        rusqlite::Connection::open(&fixture.database_path).expect("open readiness database");
    let schema_version: u32 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read charging database schema version");
    database
        .pragma_update(None, "user_version", 0)
        .expect("make charging database unready");
    assert_eq!(
        request(client, address, MUSIC_HOST, "/readyz")
            .await
            .status(),
        StatusCode::OK
    );
    for host in [RIDES_HOST, SITE_HOST] {
        assert_eq!(
            request(client, address, host, "/readyz").await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
    database
        .pragma_update(None, "user_version", schema_version)
        .expect("restore charging database version");
    assert_eq!(
        request(client, address, RIDES_HOST, "/readyz")
            .await
            .status(),
        StatusCode::OK
    );
    drop(database);

    std::fs::remove_file(fixture.credentials_directory.join("soundcloud.json"))
        .expect("remove credentials to exercise readiness");
    assert_eq!(
        request(client, address, MUSIC_HOST, "/readyz")
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    for host in [RIDES_HOST, SITE_HOST] {
        assert_eq!(
            request(client, address, host, "/readyz").await.status(),
            StatusCode::OK
        );
    }
}

async fn finish_inflight_identity(server: &mut RunningServer, address: SocketAddr) -> Value {
    let mut stream = TcpStream::connect(address)
        .await
        .expect("connect in-flight request");
    stream
        .write_all(
            b"POST /v1/identities HTTP/1.1\r\nHost: rides.cutout.lol\r\nX-Real-IP: 192.0.2.1\r\nTransfer-Encoding: chunked\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
        )
        .await
        .expect("send request headers");
    let interim = timeout(Duration::from_secs(5), read_http_headers(&mut stream))
        .await
        .expect("timed out waiting for 100 Continue");
    assert!(
        interim.starts_with(b"HTTP/1.1 100 Continue\r\n"),
        "handler did not request the body: {}",
        String::from_utf8_lossy(&interim)
    );

    server.send_terminate();
    stream
        .write_all(b"0\r\n\r\n")
        .await
        .expect("complete in-flight empty body");
    let mut response = Vec::new();
    timeout(Duration::from_secs(10), stream.read_to_end(&mut response))
        .await
        .expect("timed out waiting for in-flight response")
        .expect("read in-flight response");
    assert!(
        response.starts_with(b"HTTP/1.1 201 Created\r\n"),
        "in-flight request did not finish successfully: {}",
        String::from_utf8_lossy(&response)
    );
    let offset = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|offset| offset + 4)
        .expect("response header terminator");
    let identity: Value =
        serde_json::from_slice(&response[offset..]).expect("in-flight identity JSON");
    assert!(identity["id"].as_str().is_some());
    identity
}

fn assert_drained_identity_is_durable(database_path: &std::path::Path, identity: &Value) {
    let database = rusqlite::Connection::open(database_path).expect("reopen SQLite database");
    let durable_identity_count: i64 = database
        .query_row(
            "SELECT COUNT(*) FROM contributors WHERE id = ?1 AND role = 'contributor' AND revoked_at_ms IS NULL",
            [identity["id"].as_str().expect("drained identity id")],
            |row| row.get(0),
        )
        .expect("read durable in-flight identity");
    assert_eq!(durable_identity_count, 1);
}

fn reserve_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve local port");
    listener.local_addr().expect("reserved address").port()
}

fn start_server(
    address: SocketAddr,
    database_path: &std::path::Path,
    credentials_directory: &std::path::Path,
    state_path: &std::path::Path,
) -> RunningServer {
    let child = Command::new(env!("CARGO_BIN_EXE_cutout-server"))
        .arg("--database")
        .arg(database_path)
        .arg("serve")
        .arg("--music")
        .arg("--listen")
        .arg(address.to_string())
        .arg("--trusted-proxy")
        .arg("127.0.0.1")
        .env("CREDENTIALS_DIRECTORY", credentials_directory)
        .env("SOUNDCLOUD_TOKEN_STATE_FILE", state_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start combined server");
    RunningServer { child, address }
}

async fn wait_until_ready(server: &mut RunningServer, client: &Client) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(status) = server.child.try_wait().expect("check server process") {
            let output = server.take_output(status);
            panic!(
                "combined server exited before readiness: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if let Ok(response) = client
            .get(format!("http://{}/readyz", server.address))
            .header(HOST, MUSIC_HOST)
            .send()
            .await
            && response.status() == StatusCode::OK
        {
            return;
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("combined server did not become ready before the deadline");
}

async fn request(client: &Client, address: SocketAddr, host: &str, path: &str) -> Response {
    client
        .get(format!("http://{address}{path}"))
        .header(HOST, host)
        .send()
        .await
        .unwrap_or_else(|error| panic!("request to {host}{path} failed: {error}"))
}

async fn read_http_headers(stream: &mut TcpStream) -> Vec<u8> {
    let mut headers = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        stream
            .read_exact(&mut byte)
            .await
            .expect("read HTTP response headers");
        headers.push(byte[0]);
        if headers.ends_with(b"\r\n\r\n") {
            return headers;
        }
    }
}

struct RunningServer {
    child: Child,
    address: SocketAddr,
}

impl RunningServer {
    fn send_terminate(&self) {
        let signal = Command::new("kill")
            .arg("-TERM")
            .arg(self.child.id().to_string())
            .status()
            .expect("send SIGTERM");
        assert!(signal.success(), "could not send SIGTERM to server");
    }

    async fn wait_for_exit(&mut self) -> Output {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().expect("wait for server shutdown") {
                return self.take_output(status);
            }
            assert!(
                Instant::now() < deadline,
                "server did not stop after SIGTERM"
            );
            sleep(Duration::from_millis(50)).await;
        }
    }

    fn take_output(&mut self, status: std::process::ExitStatus) -> Output {
        use std::io::Read;

        let mut stderr = Vec::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_end(&mut stderr);
        }
        Output {
            status,
            stdout: Vec::new(),
            stderr,
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
