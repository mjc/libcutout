//! Truthful state for an external ride camera.

/// Foreground preview state, independent of the camera's onboard recording state.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CameraPreviewState {
    /// No preview is active.
    #[default]
    Stopped,
    /// The preview is waiting for usable media.
    Buffering,
    /// The preview is receiving current media.
    Live,
    /// The most recent preview media is no longer current.
    Stale,
    /// An active preview was interrupted.
    Interrupted,
    /// The configured camera cannot provide a preview.
    Unavailable,
}

/// Authoritative onboard recording state reported by the camera.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CameraOnboardRecordingState {
    /// Recording truth has not been read back from the camera.
    #[default]
    Unknown,
    /// The camera confirmed that onboard recording is stopped.
    Stopped,
    /// The camera confirmed that onboard recording is active.
    Recording,
}

/// Verified or test-only source that can feed a camera session.
///
/// This identifies the source family without owning sockets, video decoders,
/// or platform FFI. Those details stay with the adapter that supplies the
/// source observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraSourceKind {
    /// The hardware-verified `FreedConn` R3 Pro Novatek profile.
    NovatekR3Pro,
    /// A camera source whose preview transport is standard RTSP.
    Rtsp,
    /// A deterministic captured source used by tests and replay.
    Fixture,
}

/// Rust-owned state for one external ride-camera session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CameraSessionState {
    preview: CameraPreviewState,
    onboard_recording: CameraOnboardRecordingState,
}

/// Rust-owned camera source and state boundary.
///
/// The platform adapter owns connection and decoding details; this type keeps
/// only the selected source identity and truthful camera state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CameraSourceSession {
    source: CameraSourceKind,
    state: CameraSessionState,
}

impl CameraSourceSession {
    /// Creates a session for a selected camera source.
    #[must_use]
    pub const fn new(source: CameraSourceKind) -> Self {
        Self {
            source,
            state: CameraSessionState {
                preview: CameraPreviewState::Stopped,
                onboard_recording: CameraOnboardRecordingState::Unknown,
            },
        }
    }

    /// Returns the source identity.
    #[must_use]
    pub const fn source(self) -> CameraSourceKind {
        self.source
    }

    /// Returns the current truthful camera state.
    #[must_use]
    pub const fn state(self) -> CameraSessionState {
        self.state
    }

    /// Records a foreground preview observation.
    pub const fn observe_preview(&mut self, preview: CameraPreviewState) {
        self.state.observe_preview(preview);
    }

    /// Records authoritative onboard recording truth.
    pub const fn observe_onboard_recording(
        &mut self,
        onboard_recording: CameraOnboardRecordingState,
    ) {
        self.state.observe_onboard_recording(onboard_recording);
    }
}

impl CameraSessionState {
    /// Returns the current foreground preview state.
    #[must_use]
    pub const fn preview(self) -> CameraPreviewState {
        self.preview
    }

    /// Returns the latest authoritative onboard recording state.
    #[must_use]
    pub const fn onboard_recording(self) -> CameraOnboardRecordingState {
        self.onboard_recording
    }

    /// Records a foreground preview observation without inferring recording state.
    pub const fn observe_preview(&mut self, preview: CameraPreviewState) {
        self.preview = preview;
    }

    /// Records authoritative onboard recording truth without changing preview state.
    pub const fn observe_onboard_recording(
        &mut self,
        onboard_recording: CameraOnboardRecordingState,
    ) {
        self.onboard_recording = onboard_recording;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_preview_preserves_onboard_recording_truth() {
        let mut state = CameraSessionState::default();

        assert_eq!(state.preview(), CameraPreviewState::Stopped);
        assert_eq!(
            state.onboard_recording(),
            CameraOnboardRecordingState::Unknown
        );

        state.observe_onboard_recording(CameraOnboardRecordingState::Recording);
        state.observe_preview(CameraPreviewState::Live);
        state.observe_preview(CameraPreviewState::Stopped);

        assert_eq!(state.preview(), CameraPreviewState::Stopped);
        assert_eq!(
            state.onboard_recording(),
            CameraOnboardRecordingState::Recording
        );
    }

    #[test]
    fn source_session_keeps_transport_identity_separate_from_state() {
        let mut session = CameraSourceSession::new(CameraSourceKind::NovatekR3Pro);

        session.observe_preview(CameraPreviewState::Live);
        session.observe_onboard_recording(CameraOnboardRecordingState::Unknown);

        assert_eq!(session.source(), CameraSourceKind::NovatekR3Pro);
        assert_eq!(session.state().preview(), CameraPreviewState::Live);
        assert_eq!(
            session.state().onboard_recording(),
            CameraOnboardRecordingState::Unknown
        );
    }
}
