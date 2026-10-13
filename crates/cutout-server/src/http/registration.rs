use super::TrustedProxy;
use crate::{
    Error, Store,
    store::{Identity, Registration},
};
use axum::{
    Json,
    extract::{ConnectInfo, Extension, State},
    http::{HeaderMap, StatusCode},
};
use std::net::{IpAddr, SocketAddr};

pub(super) async fn identity(
    State(store): State<Store>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    trusted_proxy: Option<Extension<TrustedProxy>>,
    headers: HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<(StatusCode, Json<Identity>), Error> {
    if !body.map_err(|_| Error::Invalid)?.is_empty() {
        return Err(Error::Invalid);
    }
    let source = registration_source(
        peer.ip(),
        trusted_proxy.map(|Extension(proxy)| proxy),
        &headers,
    )?;
    Ok((
        StatusCode::CREATED,
        Json(store.identity(Registration::Public { source }).await?),
    ))
}

fn registration_source(
    peer: IpAddr,
    trusted_proxy: Option<TrustedProxy>,
    headers: &HeaderMap,
) -> Result<IpAddr, Error> {
    match trusted_proxy {
        Some(TrustedProxy(proxy)) if proxy.to_canonical() == peer.to_canonical() => {
            forwarded_client_address(headers)
        }
        Some(_) | None => Ok(peer),
    }
}

fn forwarded_client_address(headers: &HeaderMap) -> Result<IpAddr, Error> {
    let mut addresses = headers.get_all("x-real-ip").iter();
    let address = addresses.next().ok_or(Error::Invalid)?;
    if addresses.next().is_some() {
        return Err(Error::Invalid);
    }
    address
        .to_str()
        .map_err(|_| Error::Invalid)?
        .parse()
        .map_err(|_| Error::Invalid)
}

#[cfg(test)]
mod tests {
    use super::registration_source;
    use crate::{Error, TrustedProxy};
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn canonical_transport_addresses_match_the_configured_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("192.0.2.1"));
        for peer in ["127.0.0.1", "::ffff:127.0.0.1"] {
            for proxy in ["127.0.0.1", "::ffff:127.0.0.1"] {
                assert_eq!(
                    registration_source(
                        peer.parse().expect("peer"),
                        Some(TrustedProxy(proxy.parse().expect("proxy"))),
                        &headers
                    )
                    .expect("trusted client"),
                    "192.0.2.1".parse::<std::net::IpAddr>().expect("client")
                );
            }
        }
    }

    #[test]
    fn non_text_proxy_headers_are_rejected_only_for_a_trusted_peer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-real-ip",
            HeaderValue::from_bytes(&[0xff]).expect("opaque header"),
        );
        let peer = "127.0.0.1".parse().expect("peer");
        assert_eq!(
            registration_source(peer, Some(TrustedProxy(peer)), &headers),
            Err(Error::Invalid)
        );
        let other = "192.0.2.2".parse().expect("other proxy");
        assert_eq!(
            registration_source(peer, Some(TrustedProxy(other)), &headers),
            Ok(peer)
        );
        assert_eq!(registration_source(peer, None, &headers), Ok(peer));
    }
}
