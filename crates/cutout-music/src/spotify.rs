//! Spotify content links admitted from actual provider item identities.

/// Returns a content-specific HTTPS link for a supported Spotify item URI.
#[must_use]
pub fn content_url(identifier: &str) -> Option<String> {
    let mut parts = identifier.split(':');
    if parts.next()? != "spotify" {
        return None;
    }
    let kind = parts.next()?;
    if kind != "track" && kind != "episode" {
        return None;
    }
    let id = parts.next()?;
    if parts.next().is_some()
        || id.len() != 22
        || !id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(format!("https://open.spotify.com/{kind}/{id}"))
}

#[cfg(test)]
mod tests {
    use super::content_url;

    #[test]
    fn spotify_links_use_the_actual_track_or_episode_identity() {
        assert_eq!(
            content_url("spotify:track:4uLU6hMCjMI75M1A2tKUQC"),
            Some("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC".into())
        );
        assert_eq!(
            content_url("spotify:episode:512ojhOuo1ktJprKbVcKyQ"),
            Some("https://open.spotify.com/episode/512ojhOuo1ktJprKbVcKyQ".into())
        );
    }

    #[test]
    fn spotify_links_reject_local_unknown_and_url_injection_identifiers() {
        for identifier in [
            "",
            "spotify:local:Artist:Album:Song:100",
            "spotify:track:short",
            "spotify:track:4uLU6hMCjMI75M1A2tKUQC?evil",
            "https://evil.example/track",
            "spotify:track:4uLU6hMCjMI75M1A2tKUQ/",
            "spotify:album:4uLU6hMCjMI75M1A2tKUQC",
        ] {
            assert_eq!(content_url(identifier), None, "{identifier}");
        }
    }
}
