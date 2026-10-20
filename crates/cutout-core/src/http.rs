//! Shared HTTPS client configuration for Cutout's network consumers.

use std::sync::Arc;

use rustls_platform_verifier::BuilderVerifierExt;

/// Starts an HTTP client builder with Cutout's TLS configuration.
///
/// Uses Ring and the platform certificate verifier without changing the
/// process-global Rustls provider. Callers configure request policy on the
/// returned builder.
///
/// # Errors
///
/// Returns an error if the platform certificate verifier cannot be initialized.
pub fn client_builder() -> Result<reqwest::ClientBuilder, rustls::Error> {
    Ok(reqwest::Client::builder().tls_backend_preconfigured(tls_config()?))
}

fn tls_config() -> Result<rustls::ClientConfig, rustls::Error> {
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_platform_verifier()?
    .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

#[cfg(test)]
mod tests {
    #[test]
    fn tls_configuration_uses_the_ring_cipher_implementation() {
        let config = super::tls_config().expect("platform verifier");
        let ring_suite = rustls::crypto::ring::cipher_suite::TLS13_AES_256_GCM_SHA384;
        let actual = config
            .crypto_provider()
            .cipher_suites
            .iter()
            .find(|suite| suite.suite() == ring_suite.suite())
            .and_then(rustls::SupportedCipherSuite::tls13)
            .expect("configured TLS 1.3 AES-256 suite");
        let expected = ring_suite.tls13().expect("Ring TLS 1.3 suite");
        assert!(std::ptr::eq(actual, expected));
    }
}
