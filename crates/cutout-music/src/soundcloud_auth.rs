//! Admission and expiry of public-playback tokens from `CutOut`'s token service.

use crate::soundcloud_player::PlayerError;

/// Official app-token service; confidential credentials stay off-device.
pub const TOKEN_ENDPOINT: &str = "https://soundcloud.cutout.lol/v1/token";

/// Validated, expiring authorization. Debug output never includes the token.
pub struct ApiAuthorization {
    token: String,
    admitted_at_ms: u64,
    valid_until_ms: u64,
}

impl std::fmt::Debug for ApiAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiAuthorization([redacted])")
    }
}

impl ApiAuthorization {
    /// Admits a bounded broker response against the host monotonic clock.
    ///
    /// # Errors
    /// Rejects malformed tokens, unsupported token types, or invalid expiry.
    pub fn parse(json: &[u8], now_ms: u64) -> Result<Self, PlayerError> {
        #[derive(serde::Deserialize)]
        struct Response {
            access_token: String,
            token_type: String,
            expires_in: u64,
        }
        if json.len() > 16 * 1024 {
            return Err(PlayerError::TooLarge);
        }
        let response: Response =
            serde_json::from_slice(json).map_err(|_| PlayerError::InvalidResponse)?;
        if !response.token_type.eq_ignore_ascii_case("bearer")
            || !(16..=3600).contains(&response.expires_in)
            || response.access_token.is_empty()
            || response.access_token.len() > 8192
            || !response
                .access_token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
        {
            return Err(PlayerError::InvalidResponse);
        }
        let valid_until_ms = now_ms
            .checked_add((response.expires_in - 15) * 1000)
            .ok_or(PlayerError::InvalidResponse)?;
        Ok(Self {
            token: response.access_token,
            admitted_at_ms: now_ms,
            valid_until_ms,
        })
    }

    /// Projects an API header only while its admitted lifetime remains valid.
    #[must_use]
    pub fn header(&self, now_ms: u64) -> Option<String> {
        if now_ms < self.admitted_at_ms || now_ms >= self.valid_until_ms {
            return None;
        }
        Some(format!("OAuth {}", self.token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admitted_token_expires_before_its_reported_provider_deadline() {
        let authorization = ApiAuthorization::parse(
            br#"{"access_token":"fixture-access","token_type":"bearer","expires_in":120}"#,
            1000,
        )
        .expect("valid broker response");
        assert_eq!(
            authorization.header(1000).as_deref(),
            Some("OAuth fixture-access")
        );
        assert!(authorization.header(106_000).is_none());
        assert!(!format!("{authorization:?}").contains("fixture-access"));
    }
    #[test]
    fn token_boundary_rejects_header_injection_and_impossible_lifetimes() {
        for json in [
            br#"{"access_token":"token\r\nInjected: yes","token_type":"bearer","expires_in":120}"#
                .as_slice(),
            br#"{"access_token":"","token_type":"bearer","expires_in":120}"#,
            br#"{"access_token":"fixture","token_type":"basic","expires_in":120}"#,
            br#"{"access_token":"fixture","token_type":"bearer","expires_in":0}"#,
            br#"{"access_token":"fixture","token_type":"bearer","expires_in":4000}"#,
        ] {
            assert_eq!(
                ApiAuthorization::parse(json, 1).expect_err("invalid authorization"),
                PlayerError::InvalidResponse
            );
        }
        assert!(
            ApiAuthorization::parse(
                br#"{"access_token":"fixture","token_type":"bearer","expires_in":120}"#,
                u64::MAX
            )
            .is_err()
        );
    }
}
