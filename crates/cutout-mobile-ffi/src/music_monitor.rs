use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_music::{MusicMonitor, MusicMonitorStart};

use crate::{CoreMusicPlaybackState, MobileMusicPlaybackStateDto};

/// Matches callback paths with only empty/slash root equivalence.
#[uniffi::export]
#[must_use]
#[allow(clippy::needless_pass_by_value)] // UniFFI owns strings at this boundary.
pub fn music_callback_path_matches(expected: String, actual: String) -> bool {
    cutout_music::music_callback_path_matches(&expected, &actual)
}

/// Localized title key preserving playback state when track metadata is absent.
#[uniffi::export]
#[must_use]
pub fn music_playback_title_key(state: MobileMusicPlaybackStateDto) -> String {
    CoreMusicPlaybackState::from(state)
        .fallback_title_key()
        .to_owned()
}

/// Permission for a single foreground provider-monitor start.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileMusicMonitorStart {
    /// Whether this start may consume an explicit user authorization request.
    pub allow_authorization: bool,
}

/// Rust-owned music observation intent, unrelated to whether a ride is open.
#[derive(Debug, Default, uniffi::Object)]
pub struct MobileMusicMonitor {
    inner: Mutex<MusicMonitor>,
}

#[uniffi::export]
impl MobileMusicMonitor {
    /// Creates an unrequested foreground monitor.
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Requests passive monitoring or one explicit authorization attempt.
    pub fn request(&self, allow_authorization: bool) {
        self.lock_inner().request(allow_authorization);
    }

    /// Cancels monitoring intent and any unused authorization permission.
    pub fn cancel(&self) {
        self.lock_inner().cancel();
    }

    /// Preserves monitoring intent while suspending foreground work.
    pub fn suspend(&self) {
        self.lock_inner().suspend();
    }

    /// Whether entering the foreground should restore observation.
    #[must_use]
    pub fn resume(&self) -> bool {
        self.lock_inner().resume()
    }

    /// Whether the scene permits foreground observation.
    #[must_use]
    pub fn is_scene_active(&self) -> bool {
        self.lock_inner().is_scene_active()
    }

    /// Admits a start and consumes the one-shot authorization permission, if any.
    #[must_use]
    pub fn take_start(&self) -> Option<MobileMusicMonitorStart> {
        self.lock_inner()
            .take_start()
            .map(|start| MobileMusicMonitorStart {
                allow_authorization: start == MusicMonitorStart::Authorize,
            })
    }
}

impl MobileMusicMonitor {
    fn lock_inner(&self) -> MutexGuard<'_, MusicMonitor> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
