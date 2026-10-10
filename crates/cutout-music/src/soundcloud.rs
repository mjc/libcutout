//! Local `SoundCloud` capabilities. No HTTP or embedded playback is involved.
//!
//! Public iOS APIs do not expose the `SoundCloud` app's playback or metadata.
//! Opening the app is an explicit user action and never confirms playback.

use cutout_core::MonotonicTimestamp;

use crate::{MusicCommand, MusicSnapshot};

/// Proof that an explicit app-opening command passed `SoundCloud` admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoundCloudHandoff {
    url: &'static str,
}

impl SoundCloudHandoff {
    /// Local app URL. It contains no track selection or playback command.
    #[must_use]
    pub const fn url(&self) -> &'static str {
        self.url
    }
}

/// The only effect available through the `SoundCloud` local integration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use]
pub enum SoundCloudCommandAdmission {
    /// Execute an explicitly requested local app handoff.
    Handoff(SoundCloudHandoff),
    /// Public iOS APIs do not provide this `SoundCloud` command.
    Refused,
}

/// Admits local app opening and refuses unavailable playback operations.
pub const fn admit_command(command: MusicCommand) -> SoundCloudCommandAdmission {
    match command {
        MusicCommand::OpenProvider => SoundCloudCommandAdmission::Handoff(SoundCloudHandoff {
            url: "soundcloud://",
        }),
        MusicCommand::Play | MusicCommand::Pause | MusicCommand::Previous | MusicCommand::Next => {
            SoundCloudCommandAdmission::Refused
        }
    }
}

/// Projects truthful unavailability without a synthetic track or history event.
#[must_use]
pub fn unavailable_snapshot(observed_at: MonotonicTimestamp) -> MusicSnapshot {
    MusicSnapshot::soundcloud_unavailable(observed_at)
}
