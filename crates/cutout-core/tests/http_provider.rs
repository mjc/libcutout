#![cfg(feature = "http")]
#![allow(clippy::disallowed_macros)]

#[test]
fn builds_client_without_installing_a_global_crypto_provider() {
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    let client = cutout_core::http::client_builder()
        .expect("platform verifier")
        .no_proxy()
        .build()
        .expect("client must construct without a global crypto provider");
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    drop(client);
}
