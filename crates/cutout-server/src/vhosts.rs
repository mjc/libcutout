//! Strict host dispatch for the combined public server.

use axum::{
    Router,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use std::net::{IpAddr, SocketAddr};
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

    fn matches_header(&self, value: &str) -> bool {
        value == self.0
    }
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
}

/// Build a host-dispatching listener from already configured application routers.
/// The transport peer must be supplied by `into_make_service_with_connect_info`;
/// the configured proxy must overwrite `Host` with the canonical public host
/// and strip forwarding or internal routing metadata.
pub fn router(vhosts: PublicVhosts, music: Router, rides: Router, site: Router) -> Router {
    Router::new().fallback(dispatch).with_state(DispatchState {
        vhosts,
        music,
        rides,
        site,
    })
}

#[derive(Clone)]
struct DispatchState {
    vhosts: PublicVhosts,
    music: Router,
    rides: Router,
    site: Router,
}

async fn dispatch(State(state): State<DispatchState>, request: Request) -> Response {
    let Some(peer) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0)
    else {
        return invalid_host();
    };
    if !same_ip(peer.ip(), state.vhosts.trusted_proxy.0) {
        return invalid_host();
    }
    let Some(host) = canonical_request_host(request.headers(), request.uri()) else {
        return invalid_host();
    };
    let service = if state.vhosts.music_host.matches_header(&host) {
        &state.music
    } else if state.vhosts.rides_host.matches_header(&host) {
        &state.rides
    } else if state.vhosts.site_host.matches_header(&host) {
        &state.site
    } else {
        return invalid_host();
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

fn canonical_request_host(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    if uri.authority().is_some() {
        return None;
    }
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts.next()?.to_str().ok()?;
    if hosts.next().is_some() || !is_canonical_host(host) {
        return None;
    }

    for name in [
        "forwarded",
        "x-forwarded-host",
        "x-original-host",
        "x-cutout-host",
        "x-cutout-vhost",
        "x-cutout-route",
    ] {
        if headers.contains_key(name) {
            return None;
        }
    }
    Some(host.to_owned())
}

fn is_canonical_host(host: &str) -> bool {
    PublicHost::parse(host.to_owned()).is_ok()
}

#[cfg(test)]
#[path = "vhosts/tests.rs"]
mod tests;
