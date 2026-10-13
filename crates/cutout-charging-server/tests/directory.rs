use cutout_charging_server::{Store, TrustedProxy, router};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use uuid::Uuid;

struct Server {
    address: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start(path: &std::path::Path) -> Self {
        Self::start_on(path, "127.0.0.1:0").await
    }

    async fn start_on(path: &std::path::Path, listen: &str) -> Self {
        Self::start_configured(path, listen, None).await
    }

    async fn start_configured(
        path: &std::path::Path,
        listen: &str,
        proxy: Option<std::net::IpAddr>,
    ) -> Self {
        let store = Store::open(path).expect("open database");
        let listener = tokio::net::TcpListener::bind(listen)
            .await
            .expect("bind listener");
        let address = format!("http://{}", listener.local_addr().expect("local address"));
        let app = match proxy {
            Some(proxy) => router(store).layer(axum::Extension(TrustedProxy(proxy))),
            None => router(store),
        };
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .expect("serve requests");
        });
        Self {
            address,
            client: Client::builder().no_proxy().build().expect("client"),
            task,
        }
    }

    async fn identity(&self) -> String {
        let response = self
            .client
            .post(format!("{}/v1/identities", self.address))
            .send()
            .await
            .expect("identity request");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.expect("identity response");
        body["token"].as_str().expect("token").to_owned()
    }

    async fn create(&self, token: &str, input: Value) -> Value {
        let response = self
            .client
            .post(format!("{}/v1/charging-spots", self.address))
            .bearer_auth(token)
            .json(&json!({"operation_id":Uuid::new_v4().to_string(),"spot":input}))
            .send()
            .await
            .expect("create");
        assert_eq!(response.status(), StatusCode::CREATED);
        response.json().await.expect("create receipt")
    }

    async fn read(&self, suffix: &str, token: Option<&str>) -> Value {
        let request = self.client.get(format!("{}{suffix}", self.address));
        let request = if let Some(token) = token {
            request.bearer_auth(token)
        } else {
            request
        };
        let response = request.send().await.expect("read request");
        assert_eq!(response.status(), StatusCode::OK);
        response.json().await.expect("read body")
    }

    async fn write(
        &self,
        method: reqwest::Method,
        suffix: &str,
        token: &str,
        body: &Value,
    ) -> reqwest::Response {
        self.client
            .request(method, format!("{}{suffix}", self.address))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .expect("write request")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn minimum(cost: &str) -> Value {
    json!({"coordinate":{"latitude_e7":400_000_000,"longitude_e7":-1_050_000_000},"access_note":"Outlet by the patio.","cost":cost})
}

fn provision_moderator(path: &std::path::Path) -> String {
    let provision = std::process::Command::new(env!("CARGO_BIN_EXE_cutout-charging-server"))
        .arg("--database")
        .arg(path)
        .args(["identity", "--moderator"])
        .output()
        .expect("provision");
    assert!(provision.status.success());
    let credential: Value = serde_json::from_slice(&provision.stdout).expect("credential");
    credential["token"].as_str().expect("token").to_owned()
}

#[tokio::test]
async fn one_registration_source_cannot_exhaust_another_sources_budget() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let source = Server::start(&path).await;
    let other = Server::start_on(&path, "[::1]:0").await;
    let mut throttled = false;
    for _ in 0..125 {
        let response = source
            .client
            .post(format!("{}/v1/identities", source.address))
            .send()
            .await
            .expect("registration");
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            throttled = true;
            break;
        }
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    assert!(
        throttled,
        "one source must have a bounded registration budget"
    );
    let forged = source
        .client
        .post(format!("{}/v1/identities", source.address))
        .header("x-real-ip", "192.0.2.123")
        .send()
        .await
        .expect("forged source");
    assert_eq!(forged.status(), StatusCode::TOO_MANY_REQUESTS);
    other.identity().await;
}

#[tokio::test]
async fn registration_trusts_only_one_valid_address_from_the_configured_proxy() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let trusted = Server::start_configured(
        &path,
        "127.0.0.1:0",
        Some("127.0.0.1".parse().expect("proxy")),
    )
    .await;
    let endpoint = format!("{}/v1/identities", trusted.address);
    for value in [None, Some("bad address"), Some("192.0.2.1, 192.0.2.2")] {
        let request = trusted.client.post(&endpoint);
        let request = match value {
            Some(value) => request.header("x-real-ip", value),
            None => request,
        };
        assert_eq!(
            request
                .send()
                .await
                .expect("invalid proxy address")
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let duplicate = trusted
        .client
        .post(&endpoint)
        .header("x-real-ip", "192.0.2.1")
        .header("x-real-ip", "192.0.2.2")
        .send()
        .await
        .expect("duplicate header");
    assert_eq!(duplicate.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn registration_proxy_sources_are_canonical_independent_and_durable() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let trusted = Server::start_configured(
        &path,
        "127.0.0.1:0",
        Some("127.0.0.1".parse().expect("proxy")),
    )
    .await;
    let endpoint = format!("{}/v1/identities", trusted.address);
    let mut throttled = false;
    for _ in 0..125 {
        let response = trusted
            .client
            .post(&endpoint)
            .header("x-real-ip", "192.0.2.1")
            .send()
            .await
            .expect("registration");
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            throttled = true;
            break;
        }
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    assert!(throttled);
    assert_eq!(
        trusted
            .client
            .post(&endpoint)
            .header("x-real-ip", "::ffff:192.0.2.1")
            .send()
            .await
            .expect("canonical source")
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        trusted
            .client
            .post(&endpoint)
            .header("x-real-ip", "192.0.2.2")
            .send()
            .await
            .expect("independent client")
            .status(),
        StatusCode::CREATED
    );
    let untrusted =
        Server::start_configured(&path, "[::1]:0", Some("127.0.0.1".parse().expect("proxy"))).await;
    // A peer outside the configured proxy is counted by transport address.
    assert_eq!(
        untrusted
            .client
            .post(format!("{}/v1/identities", untrusted.address))
            .header("x-real-ip", "bad address")
            .send()
            .await
            .expect("untrusted header ignored")
            .status(),
        StatusCode::CREATED
    );
    let budgets = registration_budgets(&path);
    drop(trusted);
    let _reopened = Server::start_configured(
        &path,
        "127.0.0.1:0",
        Some("127.0.0.1".parse().expect("proxy")),
    )
    .await;
    assert_eq!(registration_budgets(&path), budgets);
}

#[derive(Debug, Eq, PartialEq)]
struct RegistrationBudget {
    principal: String,
    period_ms: u64,
    window_start_ms: u64,
    count: u32,
}

fn registration_budgets(path: &std::path::Path) -> Vec<RegistrationBudget> {
    let connection = rusqlite::Connection::open(path).expect("read budgets");
    connection
        .prepare(
            "SELECT principal, period_ms, window_start_ms, count FROM quotas
             WHERE principal LIKE 'registration-%' ORDER BY principal, period_ms",
        )
        .expect("budget query")
        .query_map([], |row| {
            Ok(RegistrationBudget {
                principal: row.get(0)?,
                period_ms: row.get(1)?,
                window_start_ms: row.get(2)?,
                count: row.get(3)?,
            })
        })
        .expect("budget records")
        .collect::<Result<_, _>>()
        .expect("decoded budgets")
}

#[tokio::test]
async fn minimum_spots_are_public_durable_and_create_retries_do_not_duplicate() {
    let directory = TempDir::new().expect("temporary directory");
    let path = directory.path().join("spots.sqlite");
    let server = Server::start(&path).await;
    let token = server.identity().await;
    let mut ids = Vec::new();
    for cost in ["free", "paid"] {
        let operation = Uuid::new_v4().to_string();
        let payload = json!({"operation_id":operation,"spot":minimum(cost)});
        let response = server
            .client
            .post(format!("{}/v1/charging-spots", server.address))
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .expect("create");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.expect("receipt");
        assert_eq!(body["state"]["spot"]["cost"], cost);
        assert_eq!(body["state"]["spot"]["supplies"], json!([]));
        assert!(body["state"]["spot"].get("owner_id").is_none());
        let id = body["state"]["id"].as_str().expect("id").to_owned();
        let retry: Value = server
            .client
            .post(format!("{}/v1/charging-spots", server.address))
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .expect("retry")
            .json()
            .await
            .expect("retry receipt");
        assert_eq!(retry["state"]["id"], id);
        ids.push(id);
    }
    drop(server);
    let reopened = Server::start(&path).await;
    let list: Value = reopened
        .client
        .get(format!("{}/v1/charging-spots", reopened.address))
        .send()
        .await
        .expect("browse")
        .json()
        .await
        .expect("page");
    assert_eq!(list["items"].as_array().expect("items").len(), 2);
    for id in ids {
        let response = reopened
            .client
            .get(format!("{}/v1/charging-spots/{id}", reopened.address))
            .send()
            .await
            .expect("detail");
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn only_the_owner_can_edit_and_concurrent_revisions_conflict() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let owner = server.identity().await;
    let other = server.identity().await;
    let created = server.create(&owner, minimum("paid")).await;
    let id = created["state"]["id"].as_str().expect("id");
    let mut updated = minimum("free");
    updated["access_note"] = json!("Now outside.");
    let request =
        json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1,"spot":updated});
    let suffix = format!("/v1/charging-spots/{id}");
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, &other, &request)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let second = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1,"spot":minimum("paid")});
    let (first, second) = tokio::join!(
        server.write(reqwest::Method::PUT, &suffix, &owner, &request),
        server.write(reqwest::Method::PUT, &suffix, &owner, &second)
    );
    let mut statuses = vec![first.status().as_u16(), second.status().as_u16()];
    statuses.sort_unstable();
    assert_eq!(statuses, vec![200, 409]);
}

#[tokio::test]
async fn votes_are_desired_states_with_replay_fences_and_authoritative_counts() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let server = Server::start(&path).await;
    let owner = server.identity().await;
    let voter = server.identity().await;
    let created = server.create(&owner, minimum("free")).await;
    let id = created["state"]["id"].as_str().expect("id");
    let suffix = format!("/v1/charging-spots/{id}/vote");
    let up = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":0,"vote":"up"});
    let response = server
        .write(reqwest::Method::PUT, &suffix, &voter, &up)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let first: Value = response.json().await.expect("vote receipt");
    assert_eq!(first["votes"], json!({"up":1,"down":0}));
    let duplicate: Value = server
        .write(reqwest::Method::PUT, &suffix, &voter, &up)
        .await
        .json()
        .await
        .expect("duplicate");
    assert_eq!(duplicate["revision"], 1);
    assert_eq!(duplicate["votes"], first["votes"]);
    let down =
        json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1,"vote":"down"});
    let replacement: Value = server
        .write(reqwest::Method::PUT, &suffix, &voter, &down)
        .await
        .json()
        .await
        .expect("replace");
    assert_eq!(replacement["votes"], json!({"up":0,"down":1}));
    let stale_retry: Value = server
        .write(reqwest::Method::PUT, &suffix, &voter, &up)
        .await
        .json()
        .await
        .expect("old retry");
    assert_eq!(stale_retry["applied_revision"], 1);
    assert_eq!(stale_retry["revision"], 2);
    assert_eq!(stale_retry["vote"], "down");
    let stale_write =
        json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1,"vote":"up"});
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, &voter, &stale_write)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let clear =
        json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":2,"vote":null});
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, &voter, &clear)
            .await
            .status(),
        StatusCode::OK
    );
    drop(server);
    let reopened = Server::start(&path).await;
    let receipt: Value = reopened
        .write(reqwest::Method::PUT, &suffix, &voter, &clear)
        .await
        .json()
        .await
        .expect("durable replay");
    assert_eq!(receipt["revision"], 3);
    assert_eq!(receipt["votes"], json!({"up":0,"down":0}));
}

#[tokio::test]
async fn withdrawal_yields_a_tombstone_and_old_creates_cannot_resurrect_it() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let owner = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let create = json!({"operation_id":operation,"spot":minimum("free")});
    let receipt: Value = server
        .write(reqwest::Method::POST, "/v1/charging-spots", &owner, &create)
        .await
        .json()
        .await
        .expect("create");
    let id = receipt["state"]["id"].as_str().expect("id");
    let withdraw = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1});
    assert_eq!(
        server
            .write(
                reqwest::Method::DELETE,
                &format!("/v1/charging-spots/{id}"),
                &owner,
                &withdraw
            )
            .await
            .status(),
        StatusCode::OK
    );
    let old_retry: Value = server
        .write(reqwest::Method::POST, "/v1/charging-spots", &owner, &create)
        .await
        .json()
        .await
        .expect("retry");
    assert_eq!(old_retry["state"]["status"], "withdrawn");
    assert!(old_retry["state"].get("spot").is_none());
    assert_eq!(
        server
            .client
            .get(format!("{}/v1/charging-spots/{id}", server.address))
            .send()
            .await
            .expect("detail")
            .status(),
        StatusCode::NOT_FOUND
    );
    let changes: Value = server
        .client
        .get(format!(
            "{}/v1/charging-spots/changes?after=0",
            server.address
        ))
        .send()
        .await
        .expect("changes")
        .json()
        .await
        .expect("change page");
    assert_eq!(changes["items"][0]["status"], "withdrawn");
    assert!(changes["items"][0].get("spot").is_none());
    let vote = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":0,"vote":"up"});
    assert_eq!(
        server
            .write(
                reqwest::Method::PUT,
                &format!("/v1/charging-spots/{id}/vote"),
                &owner,
                &vote
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn optional_venue_price_and_paired_supplies_round_trip_without_power_defaults() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let token = server.identity().await;
    let mut input = minimum("paid");
    input["name"] = json!("  Patio Cafe  ");
    input["venue"] = json!("restaurant");
    input["paid_terms"] = json!("purchase_required");
    input["price"] =
        json!({"amount_minor":500,"currency":"USD","minor_unit_exponent":2,"basis":"purchase"});
    input["supplies"] = json!([
        {"voltage_volts":120,"connectors":["nema_5_15","nema_5_20"]},
        {"voltage_volts":null,"connectors":["type_2","ccs2"]}
    ]);
    let receipt = server.create(&token, input.clone()).await;
    assert_eq!(receipt["state"]["spot"]["name"], "Patio Cafe");
    assert_eq!(receipt["state"]["spot"]["price"], input["price"]);
    assert_eq!(receipt["state"]["spot"]["supplies"], input["supplies"]);
    assert!(receipt["state"]["spot"]["last_verified_at_ms"].is_null());
}

#[tokio::test]
async fn invalid_payloads_and_untrusted_public_fields_are_rejected() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let token = server.identity().await;
    let mut examples = Vec::new();
    for (field, value) in [
        ("access_note", json!(" \t\n ")),
        ("cost", json!("unknown")),
        (
            "coordinate",
            json!({"latitude_e7":900_000_001,"longitude_e7":0}),
        ),
        ("supplies", json!([{"voltage_volts":0,"connectors":[]}])),
        (
            "supplies",
            json!([{"voltage_volts":120,"connectors":["invented_connector"]}]),
        ),
        ("private_ride", json!({"route":"private"})),
        ("votes", json!({"up":999})),
    ] {
        let mut input = minimum("free");
        input[field] = value;
        examples.push(input);
    }
    for input in examples {
        let response = server
            .write(
                reqwest::Method::POST,
                "/v1/charging-spots",
                &token,
                &json!({"operation_id":Uuid::new_v4().to_string(),"spot":input}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let error: Value = response.json().await.expect("classified error");
        assert_eq!(error, json!({"error":"invalid"}));
    }
    let response = server
        .client
        .post(format!("{}/v1/charging-spots", server.address))
        .json(&json!({"operation_id":Uuid::new_v4().to_string(),"spot":minimum("free")}))
        .send()
        .await
        .expect("unauthorized create");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}

#[tokio::test]
async fn discovery_pages_are_bounded_filtered_and_antimeridian_safe() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let token = server.identity().await;
    for (longitude, cost) in [
        (1_790_000_000, "free"),
        (-1_790_000_000, "paid"),
        (0, "free"),
    ] {
        let mut input = minimum(cost);
        input["coordinate"] = json!({"latitude_e7":0,"longitude_e7":longitude});
        server.create(&token, input).await;
    }
    let url = format!(
        "{}/v1/charging-spots?south_e7=-10000000&north_e7=10000000&west_e7=1700000000&east_e7=-1700000000",
        server.address
    );
    let page: Value = server
        .client
        .get(&url)
        .send()
        .await
        .expect("crossing query")
        .json()
        .await
        .expect("page");
    assert_eq!(page["items"].as_array().expect("items").len(), 2);
    let free: Value = server
        .client
        .get(format!("{url}&cost=free"))
        .send()
        .await
        .expect("cost query")
        .json()
        .await
        .expect("page");
    assert_eq!(free["items"].as_array().expect("items").len(), 1);
    let first: Value = server
        .client
        .get(format!("{}/v1/charging-spots?limit=1", server.address))
        .send()
        .await
        .expect("first page")
        .json()
        .await
        .expect("page");
    let cursor = first["next_cursor"].as_str().expect("next cursor");
    let second: Value = server
        .client
        .get(format!(
            "{}/v1/charging-spots?limit=1&after={cursor}",
            server.address
        ))
        .send()
        .await
        .expect("second page")
        .json()
        .await
        .expect("page");
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    for query in ["limit=0", "limit=101", "south_e7=1", "after=not-a-uuid"] {
        assert_eq!(
            server
                .client
                .get(format!("{}/v1/charging-spots?{query}", server.address))
                .send()
                .await
                .expect("invalid query")
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[tokio::test]
async fn reports_do_not_hide_pins_and_moderators_can_correct_hide_restore_and_resolve() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let moderator_token = provision_moderator(&path);
    let moderator = moderator_token.as_str();
    let server = Server::start(&path).await;
    let owner = server.identity().await;
    let reporter = server.identity().await;
    let created = server.create(&owner, minimum("free")).await;
    let id = created["state"]["id"].as_str().expect("id");
    let report = json!({"operation_id":Uuid::new_v4().to_string(),"reason":"wrong_location","message":"Outlet is on the other side."});
    let response = server
        .write(
            reqwest::Method::POST,
            &format!("/v1/charging-spots/{id}/reports"),
            &reporter,
            &report,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let report_receipt: Value = response.json().await.expect("report receipt");
    let report_id = report_receipt["id"].as_str().expect("report id");
    let suffix = format!("/v1/moderation/charging-spots/{id}");
    let hide = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1,"note":"Check location.","action":{"type":"hide"}});
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, &reporter, &hide)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        server
            .client
            .get(format!("{}/v1/charging-spots/{id}", server.address))
            .send()
            .await
            .expect("public spot")
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, moderator, &hide)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .client
            .get(format!("{}/v1/charging-spots/{id}", server.address))
            .send()
            .await
            .expect("hidden spot")
            .status(),
        StatusCode::NOT_FOUND
    );
    let mut corrected = minimum("free");
    corrected["access_note"] = json!("Outlet on the east patio.");
    let correction = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":2,"note":"Corrected access instructions.","action":{"type":"correct","spot":corrected}});
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, moderator, &correction)
            .await
            .status(),
        StatusCode::OK
    );
    let restore = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":3,"note":"Location corrected.","action":{"type":"restore"}});
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, moderator, &restore)
            .await
            .status(),
        StatusCode::OK
    );
    let published = server.read(&format!("/v1/charging-spots/{id}"), None).await;
    assert_eq!(published["revision"], 4);
    assert_eq!(published["access_note"], "Outlet on the east patio.");
    let resolve = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1,"note":"Corrected and restored."});
    assert_eq!(
        server
            .write(
                reqwest::Method::PUT,
                &format!("/v1/moderation/reports/{report_id}"),
                moderator,
                &resolve
            )
            .await
            .status(),
        StatusCode::OK
    );
    let reports = server
        .read("/v1/moderation/reports?status=open", Some(moderator))
        .await;
    assert_eq!(reports["items"], json!([]));
    let audit = server.read("/v1/moderation/audit", Some(moderator)).await;
    assert_eq!(audit["items"].as_array().expect("entries").len(), 4);
}

#[tokio::test]
async fn access_verification_is_explicit_and_is_invalidated_by_an_edit() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let owner = server.identity().await;
    let created = server.create(&owner, minimum("free")).await;
    let id = created["state"]["id"].as_str().expect("id");
    assert!(created["state"]["spot"]["last_verified_at_ms"].is_null());
    let request = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1});
    let suffix = format!("/v1/charging-spots/{id}/verification");
    let response = server
        .write(reqwest::Method::POST, &suffix, &owner, &request)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let verified: Value = response.json().await.expect("verification receipt");
    assert!(
        verified["state"]["spot"]["last_verified_at_ms"]
            .as_u64()
            .expect("time")
            > 0
    );
    let edit = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":2,"spot":minimum("paid")});
    let edited: Value = server
        .write(
            reqwest::Method::PUT,
            &format!("/v1/charging-spots/{id}"),
            &owner,
            &edit,
        )
        .await
        .json()
        .await
        .expect("edit receipt");
    assert!(edited["state"]["spot"]["last_verified_at_ms"].is_null());
    let old: Value = server
        .write(reqwest::Method::POST, &suffix, &owner, &request)
        .await
        .json()
        .await
        .expect("verification replay");
    assert!(old["state"]["spot"]["last_verified_at_ms"].is_null());
    assert_eq!(old["state"]["revision"], 3);
}

#[tokio::test]
async fn a_vote_requires_an_explicit_requested_state() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let token = server.identity().await;
    let created = server.create(&token, minimum("free")).await;
    let id = created["state"]["id"].as_str().expect("id");
    let missing = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":0});
    assert_eq!(
        server
            .write(
                reqwest::Method::PUT,
                &format!("/v1/charging-spots/{id}/vote"),
                &token,
                &missing
            )
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn registration_accepts_no_role_or_oversized_payload_and_errors_stay_classified() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let response = server
        .client
        .post(format!("{}/v1/identities", server.address))
        .json(&json!({"moderator":true}))
        .send()
        .await
        .expect("registration");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let token = server.identity().await;
    for body in ["not-json".to_owned(), "x".repeat(40_000)] {
        let response = server
            .client
            .post(format!("{}/v1/charging-spots", server.address))
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .expect("malformed request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.json::<Value>().await.expect("error"),
            json!({"error":"invalid"})
        );
    }
}

#[tokio::test]
async fn moderator_duplicate_removal_is_terminal_and_replay_safe() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let moderator_token = provision_moderator(&path);
    let moderator = moderator_token.as_str();
    let server = Server::start(&path).await;
    let owner = server.identity().await;
    let canonical = server.create(&owner, minimum("free")).await;
    let duplicate = server.create(&owner, minimum("free")).await;
    let canonical_id = canonical["state"]["id"].as_str().expect("id");
    let duplicate_id = duplicate["state"]["id"].as_str().expect("id");
    let suffix = format!("/v1/moderation/charging-spots/{duplicate_id}");
    let request = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":1,"note":"Same outlet.","action":{"type":"duplicate","canonical_id":canonical_id}});
    let removed: Value = server
        .write(reqwest::Method::PUT, &suffix, moderator, &request)
        .await
        .json()
        .await
        .expect("removed");
    assert_eq!(removed["state"]["status"], "removed");
    assert_eq!(removed["state"]["duplicate_of"], canonical_id);
    assert!(removed["state"].get("spot").is_none());
    let replay: Value = server
        .write(reqwest::Method::PUT, &suffix, moderator, &request)
        .await
        .json()
        .await
        .expect("replay");
    assert_eq!(replay["state"]["revision"], 2);
    let restore = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":2,"note":"Attempt restore.","action":{"type":"restore"}});
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, moderator, &restore)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let public: Value = server
        .client
        .get(format!("{}/v1/charging-spots", server.address))
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("page");
    assert_eq!(public["items"].as_array().expect("items").len(), 1);
}

#[tokio::test]
async fn live_backup_restores_spots_votes_and_tokens_and_revocation_clears_votes() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let server = Server::start(&path).await;
    let binary = env!("CARGO_BIN_EXE_cutout-charging-server");
    let owner = server.identity().await;
    let response: Value = server
        .client
        .post(format!("{}/v1/identities", server.address))
        .send()
        .await
        .expect("identity")
        .json()
        .await
        .expect("identity body");
    let voter = response["token"].as_str().expect("token");
    let voter_id = response["id"].as_str().expect("id");
    let created = server.create(&owner, minimum("paid")).await;
    let id = created["state"]["id"].as_str().expect("id");
    let suffix = format!("/v1/charging-spots/{id}/vote");
    let vote = json!({"operation_id":Uuid::new_v4().to_string(),"expected_revision":0,"vote":"up"});
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, voter, &vote)
            .await
            .status(),
        StatusCode::OK
    );
    let backup = directory.path().join("backup.sqlite");
    let snapshot = std::process::Command::new(binary)
        .arg("--database")
        .arg(&path)
        .arg("backup")
        .arg("--output")
        .arg(&backup)
        .output()
        .expect("snapshot");
    assert!(snapshot.status.success());
    let restored = Server::start(&backup).await;
    let replay: Value = restored
        .write(reqwest::Method::PUT, &suffix, voter, &vote)
        .await
        .json()
        .await
        .expect("restored vote receipt");
    assert_eq!(replay["votes"], json!({"up":1,"down":0}));
    let revoke = std::process::Command::new(binary)
        .arg("--database")
        .arg(&path)
        .arg("revoke")
        .arg("--identity")
        .arg(voter_id)
        .output()
        .expect("revoke");
    assert!(revoke.status.success());
    assert_eq!(
        server
            .write(reqwest::Method::PUT, &suffix, voter, &vote)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let public: Value = server
        .client
        .get(format!("{}/v1/charging-spots/{id}", server.address))
        .send()
        .await
        .expect("read")
        .json()
        .await
        .expect("spot");
    assert_eq!(public["votes"], json!({"up":0,"down":0}));
}

#[tokio::test]
async fn failed_creation_rolls_back_spot_operation_and_quota_and_can_be_retried() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let server = Server::start(&path).await;
    let token = server.identity().await;
    let connection = rusqlite::Connection::open(&path).expect("failure fixture");
    connection.execute_batch("CREATE TRIGGER reject_operation BEFORE INSERT ON operations BEGIN SELECT RAISE(ABORT,'test operation failure'); END;").expect("install failure fixture");
    let payload = json!({"operation_id":Uuid::new_v4().to_string(),"spot":minimum("free")});
    let failed = server
        .write(
            reqwest::Method::POST,
            "/v1/charging-spots",
            &token,
            &payload,
        )
        .await;
    assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        failed.json::<Value>().await.expect("error"),
        json!({"error":"unavailable"})
    );
    assert_eq!(
        server.read("/v1/charging-spots", None).await["items"],
        json!([])
    );
    let operations: u64 = connection
        .query_row("SELECT COUNT(*) FROM operations", [], |row| row.get(0))
        .expect("operations");
    assert_eq!(operations, 0);
    let write_budgets: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM quotas WHERE principal LIKE 'write:%' OR principal LIKE 'new-spots:%'",
            [],
            |row| row.get(0),
        )
        .expect("write budgets");
    assert_eq!(write_budgets, 0);
    let sequence: u64 = connection
        .query_row(
            "SELECT value FROM counters WHERE name='change_sequence'",
            [],
            |row| row.get(0),
        )
        .expect("sequence");
    assert_eq!(sequence, 0);
    connection
        .execute_batch("DROP TRIGGER reject_operation;")
        .expect("remove fixture");
    assert_eq!(
        server
            .write(
                reqwest::Method::POST,
                "/v1/charging-spots",
                &token,
                &payload
            )
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        server.read("/v1/charging-spots", None).await["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );
}

#[tokio::test]
async fn concurrent_create_retries_share_an_identity_and_changed_requests_conflict() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let token = server.identity().await;
    let operation = Uuid::new_v4().to_string();
    let payload = json!({"operation_id":operation,"spot":minimum("free")});
    let (a, b) = tokio::join!(
        server.write(
            reqwest::Method::POST,
            "/v1/charging-spots",
            &token,
            &payload
        ),
        server.write(
            reqwest::Method::POST,
            "/v1/charging-spots",
            &token,
            &payload
        )
    );
    assert_eq!(a.status(), StatusCode::CREATED);
    assert_eq!(b.status(), StatusCode::CREATED);
    let a: Value = a.json().await.expect("a receipt");
    let b: Value = b.json().await.expect("b receipt");
    assert_eq!(a["state"]["id"], b["state"]["id"]);
    let changed = json!({"operation_id":operation,"spot":minimum("paid")});
    assert_eq!(
        server
            .write(
                reqwest::Method::POST,
                "/v1/charging-spots",
                &token,
                &changed
            )
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        server.read("/v1/charging-spots", None).await["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );
}

#[tokio::test]
async fn a_change_cursor_ahead_of_the_restored_database_is_rejected() {
    let directory = TempDir::new().expect("directory");
    let server = Server::start(&directory.path().join("spots.sqlite")).await;
    let response = server
        .client
        .get(format!(
            "{}/v1/charging-spots/changes?after=1",
            server.address
        ))
        .send()
        .await
        .expect("cursor request");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn retry_after_a_client_timeout_cannot_duplicate_a_committed_creation() {
    let directory = TempDir::new().expect("directory");
    let path = directory.path().join("spots.sqlite");
    let server = Server::start(&path).await;
    let token = server.identity().await;
    let payload = json!({"operation_id":Uuid::new_v4().to_string(),"spot":minimum("free")});
    let connection = rusqlite::Connection::open(&path).expect("write lock fixture");
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .expect("hold write lock");
    let client = Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_millis(250))
        .build()
        .expect("timeout client");
    let error = client
        .post(format!("{}/v1/charging-spots", server.address))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .expect_err("client times out while SQLite write waits");
    assert!(error.is_timeout());
    connection
        .execute_batch("COMMIT;")
        .expect("release write lock");
    let response = server
        .write(
            reqwest::Method::POST,
            "/v1/charging-spots",
            &token,
            &payload,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt: Value = response.json().await.expect("retry receipt");
    let id = receipt["state"]["id"].clone();
    assert_eq!(
        server.read("/v1/charging-spots", None).await["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );
    drop(server);
    let reopened = Server::start(&path).await;
    let replay: Value = reopened
        .write(
            reqwest::Method::POST,
            "/v1/charging-spots",
            &token,
            &payload,
        )
        .await
        .json()
        .await
        .expect("restart replay");
    assert_eq!(replay["state"]["id"], id);
}
