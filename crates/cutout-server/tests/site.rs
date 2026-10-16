use cutout_server::{Store, router};
use reqwest::{Client, StatusCode, header};
use tempfile::TempDir;

struct Server {
    address: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start(database: &std::path::Path) -> Self {
        let store = Store::open(database).expect("open database");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let address = format!("http://{}", listener.local_addr().expect("local address"));
        let app = router(store);
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

    async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{path}", self.address))
            .send()
            .await
            .expect("GET request")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn serves_the_mockup_assets_without_turning_unknown_api_paths_into_html() {
    let directory = TempDir::new().expect("temporary directory");
    let server = Server::start(&directory.path().join("site.sqlite")).await;

    let homepage = server.get("/").await;
    assert_eq!(homepage.status(), StatusCode::OK);
    assert_eq!(
        homepage.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert!(
        homepage
            .text()
            .await
            .expect("homepage body")
            .contains("CutOut — Know your ride.")
    );

    for (path, content_type) in [
        ("/site.css", "text/css; charset=utf-8"),
        ("/site.js", "text/javascript; charset=utf-8"),
        ("/signup-contract.js", "text/javascript; charset=utf-8"),
        ("/assets/app-icon.png", "image/png"),
        ("/locales/en.json", "application/json; charset=utf-8"),
        ("/locales/es.json", "application/json; charset=utf-8"),
    ] {
        let response = server.get(path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            content_type,
            "{path}"
        );
        let body = response.bytes().await.expect("asset body");
        assert!(!body.is_empty(), "{path}");
        if path == "/site.js" {
            let body = String::from_utf8(body.to_vec()).expect("site JavaScript is UTF-8");
            assert!(body.contains("/v1/alpha-signup/config"));
            assert!(body.contains("/v1/alpha-signup/requests"));
            assert!(body.contains("/v1/alpha-signup-removals"));
            assert!(body.contains("interaction-only"));
            assert!(body.contains("fetchWithTimeout"));
            assert!(body.contains("signupWidgetGeneration"));
            assert!(!body.contains("trail@example.com"));
            assert!(!body.contains("data-view"));
        }
        if path == "/locales/en.json" || path == "/locales/es.json" {
            let locale: serde_json::Value =
                serde_json::from_slice(&body).expect("valid locale JSON");
            assert!(locale["signup"]["privacyTitle"].is_string(), "{path}");
            assert!(locale["signup"]["privacyRetention"].is_string(), "{path}");
            assert!(locale["signup"]["removeRequestLink"].is_string(), "{path}");
            assert!(
                locale["signup"]["types"]
                    .as_array()
                    .is_some_and(|types| !types.is_empty()),
                "{path}"
            );
        }
        if path == "/signup-contract.js" {
            let body = String::from_utf8(body.to_vec()).expect("signup helper is UTF-8");
            assert!(body.contains("AbortController"));
            assert!(body.contains("signupPayload"));
            assert!(body.contains("isCurrentWidgetGeneration"));
        }
    }

    let missing_api = server.get("/v1/not-a-route").await;
    assert_eq!(missing_api.status(), StatusCode::NOT_FOUND);
    assert!(
        !missing_api
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/html"))
    );
}
