use crate::Store;
use axum::{
    Router,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
    routing::get,
};

#[derive(Clone, Copy)]
struct Asset {
    path: &'static str,
    body: &'static [u8],
    content_type: &'static str,
}

const ASSETS: &[Asset] = &[
    Asset {
        path: "/",
        body: include_bytes!("../../../docs/mockups/cutout-site/index.html"),
        content_type: "text/html; charset=utf-8",
    },
    Asset {
        path: "/site.css",
        body: include_bytes!("../../../docs/mockups/cutout-site/site.css"),
        content_type: "text/css; charset=utf-8",
    },
    Asset {
        path: "/site.js",
        body: include_bytes!("../../../docs/mockups/cutout-site/site.js"),
        content_type: "text/javascript; charset=utf-8",
    },
    Asset {
        path: "/signup-contract.js",
        body: include_bytes!("../../../docs/mockups/cutout-site/signup-contract.js"),
        content_type: "text/javascript; charset=utf-8",
    },
    Asset {
        path: "/assets/app-icon.png",
        body: include_bytes!("../../../docs/mockups/cutout-site/assets/app-icon.png"),
        content_type: "image/png",
    },
    Asset {
        path: "/assets/screenshots/en/euc-main.png",
        body: include_bytes!(
            "../../../docs/mockups/cutout-site/assets/screenshots/en/euc-main.png"
        ),
        content_type: "image/png",
    },
    Asset {
        path: "/assets/screenshots/en/ride-tracking.png",
        body: include_bytes!(
            "../../../docs/mockups/cutout-site/assets/screenshots/en/ride-tracking.png"
        ),
        content_type: "image/png",
    },
    Asset {
        path: "/assets/screenshots/en/ride-music-history.png",
        body: include_bytes!(
            "../../../docs/mockups/cutout-site/assets/screenshots/en/ride-music-history.png"
        ),
        content_type: "image/png",
    },
    Asset {
        path: "/assets/screenshots/en/charging-map.png",
        body: include_bytes!(
            "../../../docs/mockups/cutout-site/assets/screenshots/en/charging-map.png"
        ),
        content_type: "image/png",
    },
    Asset {
        path: "/assets/screenshots/en/tune.png",
        body: include_bytes!("../../../docs/mockups/cutout-site/assets/screenshots/en/tune.png"),
        content_type: "image/png",
    },
    Asset {
        path: "/assets/screenshots/en/rgb.jpg",
        body: include_bytes!("../../../docs/mockups/cutout-site/assets/screenshots/en/rgb.jpg"),
        content_type: "image/jpeg",
    },
    Asset {
        path: "/locales/en.json",
        body: include_bytes!("../../../docs/mockups/cutout-site/locales/en.json"),
        content_type: "application/json; charset=utf-8",
    },
    Asset {
        path: "/locales/es.json",
        body: include_bytes!("../../../docs/mockups/cutout-site/locales/es.json"),
        content_type: "application/json; charset=utf-8",
    },
];

pub(crate) fn router() -> Router<Store> {
    let mut router = Router::new();
    for asset in ASSETS {
        let asset = *asset;
        router = router.route(asset.path, get(move || async move { serve(asset) }));
    }
    router
}

fn serve(asset: Asset) -> Response {
    let mut response = asset.body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(asset.content_type),
    );
    response
}
