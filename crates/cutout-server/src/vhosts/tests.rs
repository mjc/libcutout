use super::{PublicHost, PublicVhosts, router};
use crate::TrustedProxy;
use axum::{
    Router,
    http::{HeaderMap, HeaderValue, StatusCode},
    routing::get,
};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use tokio::task::JoinHandle;

const MUSIC: &str = "music.cutout.lol";
const RIDES: &str = "rides.cutout.lol";
const SITE: &str = "cutout.lol";

fn app(trusted_proxy: IpAddr) -> Router {
    let vhosts = PublicVhosts::new(
        PublicHost::parse(MUSIC).expect("music host"),
        PublicHost::parse(RIDES).expect("rides host"),
        PublicHost::parse(SITE).expect("site host"),
        TrustedProxy(trusted_proxy),
    )
    .expect("vhost config");
    router(
        vhosts,
        Router::new()
            .route("/v1/token", get(|| async { "music" }))
            .route("/readyz", get(|| async { StatusCode::OK })),
        Router::new()
            .route("/v1/charging-spots", get(|| async { "rides" }))
            .route("/readyz", get(|| async { StatusCode::SERVICE_UNAVAILABLE })),
        Router::new()
            .route("/", get(|| async { "site" }))
            .route("/readyz", get(|| async { StatusCode::OK })),
    )
}

async fn serve(app: Router) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("local address");
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("serve test app");
    });
    (format!("http://{address}"), task)
}

async fn request(
    client: &reqwest::Client,
    base: &str,
    host: &str,
    path: &str,
) -> reqwest::Response {
    client
        .get(format!("{base}{path}"))
        .header(reqwest::header::HOST, host)
        .send()
        .await
        .expect("send request")
}

#[tokio::test]
async fn canonical_hosts_dispatch_to_three_isolated_routers_and_keep_readiness_local() {
    let (base, task) = serve(app("127.0.0.1".parse().expect("proxy ip"))).await;
    let client = cutout_core::http::client_builder()
        .expect("platform verifier")
        .build()
        .expect("HTTP client");

    let response = request(&client, &base, MUSIC, "/v1/token").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.expect("music body"), "music");

    let response = request(&client, &base, RIDES, "/v1/charging-spots").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.expect("rides body"), "rides");

    let response = request(&client, &base, SITE, "/").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.expect("site body"), "site");

    assert_eq!(
        request(&client, &base, MUSIC, "/v1/charging-spots")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&client, &base, RIDES, "/v1/token").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&client, &base, SITE, "/v1/token").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&client, &base, SITE, "/v1/charging-spots")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&client, &base, RIDES, "/readyz").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        request(&client, &base, MUSIC, "/readyz").await.status(),
        StatusCode::OK
    );
    task.abort();
}

#[tokio::test]
async fn unknown_malformed_duplicate_or_conflicting_host_metadata_never_dispatches() {
    let (base, task) = serve(app("127.0.0.1".parse().expect("proxy ip"))).await;
    let client = cutout_core::http::client_builder()
        .expect("platform verifier")
        .build()
        .expect("HTTP client");
    for host in [
        "unknown.cutout.lol",
        "Music.cutout.lol",
        "music.cutout.lol:443",
        "music.cutout.lol.",
        "music.cutout.lol/extra",
    ] {
        assert_eq!(
            request(&client, &base, host, "/v1/token").await.status(),
            StatusCode::MISDIRECTED_REQUEST
        );
    }

    let response = client
        .get(format!("{base}/v1/token"))
        .header(reqwest::header::HOST, MUSIC)
        .header("x-forwarded-host", RIDES)
        .send()
        .await
        .expect("conflicting forwarded host");
    assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);

    let response = client
        .get(format!("{base}/v1/token"))
        .header(reqwest::header::HOST, MUSIC)
        .header("forwarded", format!("for=192.0.2.1;host={RIDES}"))
        .send()
        .await
        .expect("conflicting Forwarded host");
    assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);

    let response = client
        .get(format!("{base}/v1/token"))
        .header(reqwest::header::HOST, MUSIC)
        .header("x-cutout-vhost", RIDES)
        .send()
        .await
        .expect("conflicting internal route");
    assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);

    let response = client
        .get(format!("{base}/v1/token"))
        .header(reqwest::header::HOST, MUSIC)
        .header("forwarded", "for=192.0.2.1;host=bad:443")
        .send()
        .await
        .expect("malformed Forwarded host");
    assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);

    let mut headers = HeaderMap::new();
    headers.append(reqwest::header::HOST, HeaderValue::from_static(MUSIC));
    headers.append(reqwest::header::HOST, HeaderValue::from_static(RIDES));
    let response = client
        .get(format!("{base}/v1/token"))
        .headers(headers)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .expect("request with duplicate Host headers");
    assert_ne!(response.status(), StatusCode::OK);
    task.abort();
}

#[tokio::test]
async fn untrusted_transport_peer_cannot_select_a_service_with_claimed_host_headers() {
    let (base, task) = serve(app("192.0.2.9".parse().expect("different proxy ip"))).await;
    let client = cutout_core::http::client_builder()
        .expect("platform verifier")
        .build()
        .expect("HTTP client");
    let response = client
        .get(format!("{base}/v1/token"))
        .header(reqwest::header::HOST, MUSIC)
        .header("x-forwarded-host", MUSIC)
        .header("x-cutout-vhost", MUSIC)
        .send()
        .await
        .expect("untrusted host request");
    assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);
    task.abort();
}

#[test]
fn host_configuration_requires_distinct_canonical_ascii_names_without_ports() {
    for value in [
        "Music.cutout.lol",
        "music.cutout.lol.",
        "music.cutout.lol:443",
        "müsic.cutout.lol",
        "-music.cutout.lol",
    ] {
        assert!(PublicHost::parse(value).is_err(), "accepted {value}");
    }
    let music = PublicHost::parse(MUSIC).expect("music host");
    let rides = PublicHost::parse(RIDES).expect("rides host");
    assert!(
        PublicVhosts::new(
            music.clone(),
            rides.clone(),
            music,
            TrustedProxy("127.0.0.1".parse().expect("proxy"))
        )
        .is_err()
    );
}

#[test]
fn duplicate_host_headers_absolute_uri_authority_and_malformed_forwarded_are_rejected() {
    let mut headers = HeaderMap::new();
    headers.append(reqwest::header::HOST, HeaderValue::from_static(MUSIC));
    headers.append(reqwest::header::HOST, HeaderValue::from_static(RIDES));
    assert!(super::request_host(&headers, &"/v1/token".parse().expect("origin uri")).is_none());

    headers.clear();
    headers.insert(reqwest::header::HOST, HeaderValue::from_static(MUSIC));
    headers.insert("forwarded", HeaderValue::from_static("not-a-parameter"));
    assert!(super::request_host(&headers, &"/v1/token".parse().expect("origin uri")).is_none());

    headers.remove("forwarded");
    assert!(
        super::request_host(
            &headers,
            &"http://music.cutout.lol/v1/token"
                .parse()
                .expect("absolute uri")
        )
        .is_none()
    );

    headers.clear();
    headers.insert(reqwest::header::HOST, HeaderValue::from_static(MUSIC));
    headers.append("x-forwarded-host", HeaderValue::from_static(MUSIC));
    headers.append("x-forwarded-host", HeaderValue::from_static(MUSIC));
    assert!(super::request_host(&headers, &"/v1/token".parse().expect("origin uri")).is_none());
}
