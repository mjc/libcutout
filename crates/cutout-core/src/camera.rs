//! Truthful state for an external ride camera.

use arrayvec::{ArrayString, ArrayVec};
use thiserror::Error;

use crate::{MonotonicTimestamp, WallClockUnixTimestamp};

/// Maximum UTF-8 bytes retained for a camera media path.
pub const CAMERA_MAX_MEDIA_PATH_BYTES: usize = 256;
/// Maximum UTF-8 bytes retained for a camera-reported display time.
pub const CAMERA_MAX_CAMERA_TIME_BYTES: usize = 32;
/// Maximum UTF-8 bytes retained for the associated ride capture name.
pub const CAMERA_MAX_RIDE_CAPTURE_NAME_BYTES: usize = 128;
/// Maximum number of media provenance records retained in one session.
pub const CAMERA_MAX_PROVENANCE_RECORDS: usize = 64;

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

/// Confidence available for the relationship between camera and phone clocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraClockUncertainty {
    /// No clock relationship was established.
    Unknown,
    /// The clocks may differ by at most this many milliseconds.
    Milliseconds(u64),
}

/// Failure while constructing bounded camera-media provenance.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CameraMediaProvenanceError {
    /// The camera path was empty.
    #[error("camera media path is empty")]
    EmptyCameraPath,
    /// The camera path exceeded the bounded storage budget.
    #[error("camera media path is too long")]
    CameraPathTooLong,
    /// The camera display time exceeded the bounded storage budget.
    #[error("camera media time is too long")]
    CameraTimeTooLong,
    /// The ride capture name was empty.
    #[error("ride capture file name is empty")]
    EmptyRideCaptureFileName,
    /// The ride capture name exceeded the bounded storage budget.
    #[error("ride capture file name is too long")]
    RideCaptureFileNameTooLong,
}

/// Host timing captured alongside a camera-media association.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CameraMediaCaptureTiming {
    /// Host monotonic time when the association was captured.
    pub captured_at_monotonic: MonotonicTimestamp,
    /// Host wall-clock time when the association was captured.
    pub captured_at_wall_clock: WallClockUnixTimestamp,
    /// Explicit camera/phone clock uncertainty.
    pub clock_uncertainty: CameraClockUncertainty,
}

/// Metadata linking one camera file to one ride capture without storing bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraMediaProvenance {
    source: CameraSourceKind,
    camera_path: ArrayString<CAMERA_MAX_MEDIA_PATH_BYTES>,
    size_bytes: u64,
    camera_timecode: u64,
    camera_time: ArrayString<CAMERA_MAX_CAMERA_TIME_BYTES>,
    ride_capture_file_name: ArrayString<CAMERA_MAX_RIDE_CAPTURE_NAME_BYTES>,
    captured_at_monotonic: MonotonicTimestamp,
    captured_at_wall_clock: WallClockUnixTimestamp,
    clock_uncertainty: CameraClockUncertainty,
}

impl CameraMediaProvenance {
    /// Creates bounded provenance for a downloaded camera media file.
    ///
    /// # Errors
    ///
    /// Returns an error when a required identifier is empty or any textual
    /// field exceeds its bounded storage budget.
    pub fn new(
        source: CameraSourceKind,
        camera_path: &str,
        size_bytes: u64,
        camera_timecode: u64,
        camera_time: &str,
        ride_capture_file_name: &str,
        timing: CameraMediaCaptureTiming,
    ) -> Result<Self, CameraMediaProvenanceError> {
        if camera_path.is_empty() {
            return Err(CameraMediaProvenanceError::EmptyCameraPath);
        }
        if ride_capture_file_name.is_empty() {
            return Err(CameraMediaProvenanceError::EmptyRideCaptureFileName);
        }
        Ok(Self {
            source,
            camera_path: ArrayString::try_from(camera_path)
                .map_err(|_| CameraMediaProvenanceError::CameraPathTooLong)?,
            size_bytes,
            camera_timecode,
            camera_time: ArrayString::try_from(camera_time)
                .map_err(|_| CameraMediaProvenanceError::CameraTimeTooLong)?,
            ride_capture_file_name: ArrayString::try_from(ride_capture_file_name)
                .map_err(|_| CameraMediaProvenanceError::RideCaptureFileNameTooLong)?,
            captured_at_monotonic: timing.captured_at_monotonic,
            captured_at_wall_clock: timing.captured_at_wall_clock,
            clock_uncertainty: timing.clock_uncertainty,
        })
    }

    /// Returns the camera source identity.
    #[must_use]
    pub const fn source(&self) -> CameraSourceKind {
        self.source
    }

    /// Returns the bounded camera media path.
    #[must_use]
    pub fn camera_path(&self) -> &str {
        self.camera_path.as_str()
    }

    /// Returns the camera-reported media size.
    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Returns the camera-reported media timecode.
    #[must_use]
    pub const fn camera_timecode(&self) -> u64 {
        self.camera_timecode
    }

    /// Returns the bounded camera display time.
    #[must_use]
    pub fn camera_time(&self) -> &str {
        self.camera_time.as_str()
    }

    /// Returns the bounded ride capture file name.
    #[must_use]
    pub fn ride_capture_file_name(&self) -> &str {
        self.ride_capture_file_name.as_str()
    }

    /// Returns the host monotonic time at which the association was captured.
    #[must_use]
    pub const fn captured_at_monotonic(&self) -> MonotonicTimestamp {
        self.captured_at_monotonic
    }

    /// Returns the host wall-clock time at which the association was captured.
    #[must_use]
    pub const fn captured_at_wall_clock(&self) -> WallClockUnixTimestamp {
        self.captured_at_wall_clock
    }

    /// Returns the retained camera/phone clock uncertainty.
    #[must_use]
    pub const fn clock_uncertainty(&self) -> CameraClockUncertainty {
        self.clock_uncertainty
    }
}

/// Bounded Rust-owned camera-media provenance for one session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CameraProvenanceState {
    records: ArrayVec<CameraMediaProvenance, CAMERA_MAX_PROVENANCE_RECORDS>,
}

impl CameraProvenanceState {
    /// Records media provenance, replacing a duplicate association and evicting
    /// the oldest association when the bounded capacity is full.
    pub fn record_media(&mut self, record: CameraMediaProvenance) {
        if let Some(existing) = self.records.iter_mut().find(|existing| {
            existing.source == record.source
                && existing.camera_path == record.camera_path
                && existing.ride_capture_file_name == record.ride_capture_file_name
        }) {
            *existing = record;
            return;
        }
        if self.records.len() == CAMERA_MAX_PROVENANCE_RECORDS {
            self.records.remove(0);
        }
        let _ = self.records.try_push(record);
    }

    /// Returns retained media provenance in capture order.
    #[must_use]
    pub fn media(&self) -> &[CameraMediaProvenance] {
        &self.records
    }

    /// Clears all associations when a new ride capture begins.
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Returns the number of retained media associations.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether no media associations are retained.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
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
    fn camera_media_provenance_rejects_unbounded_metadata() {
        let long_path = "p".repeat(CAMERA_MAX_MEDIA_PATH_BYTES + 1);

        assert_eq!(
            CameraMediaProvenance::new(
                CameraSourceKind::NovatekR3Pro,
                &long_path,
                10,
                20,
                "2026-09-07T00:00:00Z",
                "ride.pevcap",
                CameraMediaCaptureTiming {
                    captured_at_monotonic: MonotonicTimestamp::new(30),
                    captured_at_wall_clock: WallClockUnixTimestamp::new(40),
                    clock_uncertainty: CameraClockUncertainty::Unknown,
                },
            ),
            Err(CameraMediaProvenanceError::CameraPathTooLong)
        );
    }

    #[test]
    fn camera_media_provenance_retains_typed_capture_timing_without_bytes() {
        let record = CameraMediaProvenance::new(
            CameraSourceKind::NovatekR3Pro,
            "/DCIM/0001.MP4",
            10,
            20,
            "2026-09-07T00:00:00Z",
            "ride.pevcap",
            CameraMediaCaptureTiming {
                captured_at_monotonic: MonotonicTimestamp::new(30),
                captured_at_wall_clock: WallClockUnixTimestamp::new(40),
                clock_uncertainty: CameraClockUncertainty::Milliseconds(500),
            },
        )
        .expect("bounded camera metadata");

        assert_eq!(record.source(), CameraSourceKind::NovatekR3Pro);
        assert_eq!(record.camera_path(), "/DCIM/0001.MP4");
        assert_eq!(record.size_bytes(), 10);
        assert_eq!(record.captured_at_monotonic(), MonotonicTimestamp::new(30));
        assert_eq!(
            record.captured_at_wall_clock(),
            WallClockUnixTimestamp::new(40)
        );
        assert_eq!(
            record.clock_uncertainty(),
            CameraClockUncertainty::Milliseconds(500)
        );
    }

    #[test]
    fn camera_media_provenance_is_bounded_and_deduplicated() {
        let mut state = CameraProvenanceState::default();
        for index in 0..CAMERA_MAX_PROVENANCE_RECORDS {
            state.record_media(
                CameraMediaProvenance::new(
                    CameraSourceKind::Fixture,
                    &format!("/clip-{index}"),
                    index as u64,
                    index as u64,
                    "",
                    "ride.pevcap",
                    CameraMediaCaptureTiming {
                        captured_at_monotonic: MonotonicTimestamp::new(index as u64),
                        captured_at_wall_clock: WallClockUnixTimestamp::new(index as u64),
                        clock_uncertainty: CameraClockUncertainty::Unknown,
                    },
                )
                .expect("bounded camera metadata"),
            );
        }

        state.record_media(
            CameraMediaProvenance::new(
                CameraSourceKind::Fixture,
                "/clip-0",
                99,
                99,
                "",
                "ride.pevcap",
                CameraMediaCaptureTiming {
                    captured_at_monotonic: MonotonicTimestamp::new(99),
                    captured_at_wall_clock: WallClockUnixTimestamp::new(99),
                    clock_uncertainty: CameraClockUncertainty::Unknown,
                },
            )
            .expect("bounded camera metadata"),
        );

        assert_eq!(state.len(), CAMERA_MAX_PROVENANCE_RECORDS);
        assert_eq!(state.media()[0].camera_path(), "/clip-0");
        assert_eq!(state.media()[0].size_bytes(), 99);
    }

    #[test]
    fn camera_media_provenance_can_be_reset_for_a_new_ride_capture() {
        let mut state = CameraProvenanceState::default();
        state.record_media(
            CameraMediaProvenance::new(
                CameraSourceKind::Fixture,
                "/clip",
                1,
                1,
                "",
                "ride.pevcap",
                CameraMediaCaptureTiming {
                    captured_at_monotonic: MonotonicTimestamp::new(1),
                    captured_at_wall_clock: WallClockUnixTimestamp::new(1),
                    clock_uncertainty: CameraClockUncertainty::Unknown,
                },
            )
            .expect("bounded camera metadata"),
        );

        state.clear();

        assert!(state.is_empty());
    }

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
