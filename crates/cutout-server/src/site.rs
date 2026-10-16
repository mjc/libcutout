use crate::Store;
use axum::{
    Router,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
    routing::get,
};

const INDEX: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/index.html");
const SITE_CSS: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/site.css");
const SITE_JS: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/site.js");
const SIGNUP_CONTRACT_JS: &[u8] =
    include_bytes!("../../../docs/mockups/cutout-site/signup-contract.js");
const APP_ICON: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/assets/app-icon.png");
const ENGLISH: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/locales/en.json");
const SPANISH: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/locales/es.json");

pub(crate) fn router() -> Router<Store> {
    Router::new()
        .route("/", get(index))
        .route("/site.css", get(site_css))
        .route("/site.js", get(site_js))
        .route("/signup-contract.js", get(signup_contract_js))
        .route("/assets/app-icon.png", get(app_icon))
        .route("/locales/en.json", get(english))
        .route("/locales/es.json", get(spanish))
}

async fn index() -> Response {
    asset(INDEX, "text/html; charset=utf-8")
}

async fn site_css() -> Response {
    asset(SITE_CSS, "text/css; charset=utf-8")
}

async fn site_js() -> Response {
    asset(SITE_JS, "text/javascript; charset=utf-8")
}

async fn signup_contract_js() -> Response {
    asset(SIGNUP_CONTRACT_JS, "text/javascript; charset=utf-8")
}

async fn app_icon() -> Response {
    asset(APP_ICON, "image/png")
}

async fn english() -> Response {
    asset(ENGLISH, "application/json; charset=utf-8")
}

async fn spanish() -> Response {
    asset(SPANISH, "application/json; charset=utf-8")
}

fn asset(contents: &'static [u8], content_type: &'static str) -> Response {
    let mut response = contents.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}
