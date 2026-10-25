//! Strict host dispatch for the combined public server.

use axum::{
    Router,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderName, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, LazyLock},
};
use tower::ServiceExt;

use crate::TrustedProxy;

/// Canonical DNS name accepted by the public host dispatcher.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PublicHost(String);

impl PublicHost {
    /// Parse a lowercase ASCII DNS name. Ports, trailing dots, and aliases are rejected.
    ///
    /// # Errors
    /// Returns an error when the value is not a canonical ASCII DNS name.
    pub fn parse(value: impl Into<String>) -> Result<Self, HostConfigError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 253
            || !value.is_ascii()
            || value.bytes().any(|byte| byte.is_ascii_uppercase())
            || value.ends_with('.')
        {
            return Err(HostConfigError);
        }
        for label in value.split('.') {
            if label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err(HostConfigError);
            }
        }
        Ok(Self(value))
    }

    fn matches(&self, value: &str) -> bool {
        value == self.0
    }
}

enum HostedApp {
    Music,
    Rides,
    Site,
}

/// Invalid static host configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid public host configuration")]
pub struct HostConfigError;

/// The three independent public applications served by the combined listener.
#[derive(Clone, Debug)]
pub struct PublicVhosts {
    music_host: PublicHost,
    rides_host: PublicHost,
    site_host: PublicHost,
    trusted_proxy: TrustedProxy,
}

impl PublicVhosts {
    /// Construct routing policy with three distinct canonical hostnames.
    ///
    /// # Errors
    /// Returns an error when two services use the same hostname.
    pub fn new(
        music_host: PublicHost,
        rides_host: PublicHost,
        site_host: PublicHost,
        trusted_proxy: TrustedProxy,
    ) -> Result<Self, HostConfigError> {
        if music_host == rides_host || music_host == site_host || rides_host == site_host {
            return Err(HostConfigError);
        }
        Ok(Self {
            music_host,
            rides_host,
            site_host,
            trusted_proxy,
        })
    }

    fn hosted_app(&self, peer: SocketAddr, headers: &HeaderMap, uri: &Uri) -> Option<HostedApp> {
        if !same_ip(peer.ip(), self.trusted_proxy.0) {
            return None;
        }
        let host = request_host(headers, uri)?;
        if self.music_host.matches(host) {
            Some(HostedApp::Music)
        } else if self.rides_host.matches(host) {
            Some(HostedApp::Rides)
        } else if self.site_host.matches(host) {
            Some(HostedApp::Site)
        } else {
            None
        }
    }
}

/// Build a host-dispatching listener from already configured application routers.
/// The transport peer must be supplied by `into_make_service_with_connect_info`;
/// the configured proxy must overwrite `Host` with the canonical public host
/// and strip forwarding or internal routing metadata.
pub fn router(vhosts: PublicVhosts, music: Router, rides: Router, site: Router) -> Router {
    Router::new()
        .fallback(dispatch)
        .with_state(Arc::new(DispatchState {
            vhosts,
            music,
            rides,
            site,
        }))
}

struct DispatchState {
    vhosts: PublicVhosts,
    music: Router,
    rides: Router,
    site: Router,
}

async fn dispatch(State(state): State<Arc<DispatchState>>, request: Request) -> Response {
    let Some(peer) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0)
    else {
        return invalid_host();
    };
    let Some(app) = state
        .vhosts
        .hosted_app(peer, request.headers(), request.uri())
    else {
        return invalid_host();
    };
    let service = match app {
        HostedApp::Music => &state.music,
        HostedApp::Rides => &state.rides,
        HostedApp::Site => &state.site,
    };
    let mut request = request;
    request.extensions_mut().insert(state.vhosts.trusted_proxy);
    match service.clone().oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

fn invalid_host() -> Response {
    StatusCode::MISDIRECTED_REQUEST.into_response()
}

fn same_ip(left: IpAddr, right: IpAddr) -> bool {
    left.to_canonical() == right.to_canonical()
}

static REJECTED_METADATA: LazyLock<[HeaderName; 6]> = LazyLock::new(|| {
    [
        HeaderName::from_static("forwarded"),
        HeaderName::from_static("x-forwarded-host"),
        HeaderName::from_static("x-original-host"),
        HeaderName::from_static("x-cutout-host"),
        HeaderName::from_static("x-cutout-vhost"),
        HeaderName::from_static("x-cutout-route"),
    ]
});

fn request_host<'a>(headers: &'a HeaderMap, uri: &Uri) -> Option<&'a str> {
    if uri.authority().is_some() {
        return None;
    }
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts.next()?.to_str().ok()?;
    if hosts.next().is_some() {
        return None;
    }
    for name in &*REJECTED_METADATA {
        if headers.contains_key(name) {
            return None;
        }
    }
    Some(host)
}

#[cfg(test)]
#[path = "vhosts/tests.rs"]
mod tests;
