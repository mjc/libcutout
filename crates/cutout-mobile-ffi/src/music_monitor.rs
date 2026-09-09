use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cutout_music::{MusicMonitor, MusicMonitorRequest, MusicMonitorResume, MusicMonitorStart};

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

/// User intent for a foreground provider-monitor request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicMonitorRequest {
    /// Observe or reconnect using existing authorization.
    Observe,
    /// Consume one explicit user request to launch authorization if necessary.
    Authorize,
}

/// Permission for a single foreground provider-monitor start.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicMonitorStart {
    /// Observe or reconnect using existing authorization.
    Observe,
    /// Launch provider authorization for an explicit user request.
    Authorize,
}

/// Result of bringing a monitor back to the foreground.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileMusicMonitorResume {
    /// The monitor was already in the foreground.
    AlreadyActive,
    /// No monitor request was waiting when the scene resumed.
    NoRequest,
    /// A requested monitor was restored after suspension.
    Restored,
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
    pub fn request(&self, request: MobileMusicMonitorRequest) {
        self.lock_inner().request(match request {
            MobileMusicMonitorRequest::Observe => MusicMonitorRequest::Observe,
            MobileMusicMonitorRequest::Authorize => MusicMonitorRequest::Authorize,
        });
    }

    /// Cancels monitoring intent and any unused authorization permission.
    pub fn cancel(&self) {
        self.lock_inner().cancel();
    }

    /// Preserves monitoring intent while suspending foreground work.
    pub fn suspend(&self) {
        self.lock_inner().suspend();
    }

    /// Returns the foreground transition outcome and restores active observation.
    #[must_use]
    pub fn resume(&self) -> MobileMusicMonitorResume {
        match self.lock_inner().resume() {
            MusicMonitorResume::AlreadyActive => MobileMusicMonitorResume::AlreadyActive,
            MusicMonitorResume::NoRequest => MobileMusicMonitorResume::NoRequest,
            MusicMonitorResume::Restored => MobileMusicMonitorResume::Restored,
        }
    }

    /// Whether the scene permits foreground observation.
    #[must_use]
    pub fn is_scene_active(&self) -> bool {
        self.lock_inner().is_scene_active()
    }

    /// Admits a start and consumes the one-shot authorization permission, if any.
    #[must_use]
    pub fn take_start(&self) -> Option<MobileMusicMonitorStart> {
        self.lock_inner().take_start().map(|start| match start {
            MusicMonitorStart::Observe => MobileMusicMonitorStart::Observe,
            MusicMonitorStart::Authorize => MobileMusicMonitorStart::Authorize,
        })
    }
}

impl MobileMusicMonitor {
    fn lock_inner(&self) -> MutexGuard<'_, MusicMonitor> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
