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
const SCREENSHOT_EUC_MAIN: &[u8] =
    include_bytes!("../../../docs/mockups/cutout-site/assets/screenshots/en/euc-main.png");
const SCREENSHOT_RIDE_HISTORY: &[u8] =
    include_bytes!("../../../docs/mockups/cutout-site/assets/screenshots/en/ride-tracking.png");
const SCREENSHOT_CHARGING_MAP: &[u8] =
    include_bytes!("../../../docs/mockups/cutout-site/assets/screenshots/en/charging-map.png");
const SCREENSHOT_TUNE: &[u8] =
    include_bytes!("../../../docs/mockups/cutout-site/assets/screenshots/en/tune.png");
const SCREENSHOT_RGB: &[u8] =
    include_bytes!("../../../docs/mockups/cutout-site/assets/screenshots/en/rgb.jpg");
const ENGLISH: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/locales/en.json");
const SPANISH: &[u8] = include_bytes!("../../../docs/mockups/cutout-site/locales/es.json");

pub(crate) fn router() -> Router<Store> {
    Router::new()
        .route("/", get(index))
        .route("/site.css", get(site_css))
        .route("/site.js", get(site_js))
        .route("/signup-contract.js", get(signup_contract_js))
        .route("/assets/app-icon.png", get(app_icon))
        .route(
            "/assets/screenshots/en/euc-main.png",
            get(screenshot_euc_main),
        )
        .route(
            "/assets/screenshots/en/ride-tracking.png",
            get(screenshot_ride_history),
        )
        .route(
            "/assets/screenshots/en/charging-map.png",
            get(screenshot_charging_map),
        )
        .route("/assets/screenshots/en/tune.png", get(screenshot_tune))
        .route("/assets/screenshots/en/rgb.jpg", get(screenshot_rgb))
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

async fn screenshot_euc_main() -> Response {
    asset(SCREENSHOT_EUC_MAIN, "image/png")
}

async fn screenshot_ride_history() -> Response {
    asset(SCREENSHOT_RIDE_HISTORY, "image/png")
}

async fn screenshot_charging_map() -> Response {
    asset(SCREENSHOT_CHARGING_MAP, "image/png")
}

async fn screenshot_tune() -> Response {
    asset(SCREENSHOT_TUNE, "image/png")
}

async fn screenshot_rgb() -> Response {
    asset(SCREENSHOT_RGB, "image/jpeg")
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
