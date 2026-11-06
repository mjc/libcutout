//! Retained camera evidence, command authorization and one-shot media operations.

use cutout_core::{
    CameraClockUncertainty, CameraMediaCaptureTiming as CoreCameraMediaCaptureTiming,
    CameraMediaProvenance as CoreCameraMediaProvenance, CameraSourceKind as CoreCameraSourceKind,
    MonotonicTimestamp, WallClockUnixTimestamp,
};
use cutout_protocols::{
    NovatekMediaDownloadAuthorizationError, NovatekMediaDownloadOperations,
    NovatekMediaThumbnailError, NovatekR3V1Session, NovatekReadOnlySnapshot,
    NovatekRecordingCommand, NovatekStillCaptureCommand,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraCommand {
    /// Request that the camera start recording.
    StartRecording,
    /// Request that the camera stop recording.
    StopRecording,
    /// Request a still image.
    StillCapture,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraCommandRequest {
    /// Validated local camera origin retained by Rust.
    pub origin: CameraHttpOrigin,
    /// Origin-relative command target.
    pub target: String,
    /// Semantic command authorized by the retained profile.
    pub command: CameraCommand,
    /// Retained-session generation that authorized this request.
    pub session_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CameraCommandAuthorizationError {
    /// No validated R3V1 camera session is retained or the command is not advertised.
    #[error("Novatek command is not supported by the retained session")]
    Unsupported,
    /// The requested origin differs from the origin that supplied retained evidence.
    #[error("Novatek command origin differs from retained evidence")]
    OriginMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CameraCommandCompletionError {
    /// The command was authorized by a session that has since been replaced or cleared.
    #[error("Novatek command belongs to a stale camera session")]
    StaleSession,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraCommandOutcome {
    /// The camera response reported status zero.
    Acknowledged,
    /// The camera response reported a nonzero status.
    Refused,
    /// The response did not provide a usable status.
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraMediaEntry {
    /// Camera-reported file name.
    pub name: String,
    /// Camera-reported path.
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Camera-reported timestamp code.
    pub timecode: u64,
    /// Camera-reported display time.
    pub time: String,
    /// Camera-reported attribute bits.
    pub attributes: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraMediaDownloadRequest {
    /// One-shot Rust operation identity.
    pub operation_id: u64,
    /// Validated relative camera URL target.
    pub target: String,
    /// Expected file size retained from command `3015`.
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CameraMediaDownloadAuthorizationError {
    /// No validated Novatek session is retained.
    #[error("no retained Novatek media session")]
    SessionUnavailable,
    /// The requested path is absent from the retained inventory.
    #[error("media is not in the retained Novatek inventory")]
    MediaNotRetained,
    /// The retained path cannot be mapped to a safe HTTP target.
    #[error("media path is invalid")]
    InvalidPath,
    /// The operation identifier space is exhausted.
    #[error("media download operation identity exhausted")]
    OperationIdExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CameraMediaDownloadCompletionError {
    /// The operation is unknown, stale, replaced, or already consumed.
    #[error("media download operation is stale or already consumed")]
    StaleOperation,
    /// The downloaded file length differs from retained camera evidence.
    #[error("downloaded media size differs from retained evidence")]
    SizeMismatch { expected: u64, actual: u64 },
    /// The capture changed while the native transfer was in flight.
    #[error("camera media result belongs to a stale capture generation")]
    StaleCaptureGeneration,
    /// Association data does not satisfy the Rust provenance bounds.
    #[error("invalid camera media association")]
    InvalidProvenance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CameraThumbnailTargetError {
    /// The retained R3V1 configuration does not advertise command `4001`.
    #[error("media thumbnails are not advertised")]
    CapabilityNotAdvertised,
    /// The media record is not present in the current retained read-only snapshot.
    #[error("media evidence is no longer current")]
    MediaNotCurrent,
    /// The camera-reported path is not a safe thumbnail path.
    #[error("invalid Novatek media path")]
    InvalidPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraHttpOrigin {
    /// Validated IPv4 address text.
    pub address: String,
    /// Validated TCP port.
    pub port: u16,
}

impl CameraCommand {
    const fn expected_response_command_id(self) -> u16 {
        match self {
            Self::StartRecording | Self::StopRecording => 2001,
            Self::StillCapture => 1001,
        }
    }
}

/// Capture association observed before a native transfer completes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraMediaAssociation {
    pub ride_capture_file_name: String,
    pub captured_at_monotonic_ms: u64,
    pub captured_at_wall_clock_ms: u64,
    pub clock_uncertainty: CameraClockUncertainty,
}

#[derive(Debug)]
struct RetainedNovatekSession {
    profile: NovatekR3V1Session,
    read_only_snapshot: NovatekReadOnlySnapshot,
}

#[derive(Debug, Default)]
pub struct NovatekCameraSessionState {
    generation: NovatekSessionGeneration,
    retained: Option<RetainedNovatekSession>,
    media_downloads: NovatekMediaDownloadOperations,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct NovatekSessionGeneration(u64);

impl NovatekSessionGeneration {
    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

impl NovatekCameraSessionState {
    pub fn replace_read_only(
        &mut self,
        profile: NovatekR3V1Session,
        read_only_snapshot: NovatekReadOnlySnapshot,
    ) {
        self.generation.advance();
        self.media_downloads.clear();
        self.retained = Some(RetainedNovatekSession {
            profile,
            read_only_snapshot,
        });
    }

    pub fn clear(&mut self) {
        self.generation.advance();
        self.media_downloads.clear();
        self.retained = None;
    }

    #[must_use]
    pub fn profile(&self) -> Option<&NovatekR3V1Session> {
        self.retained.as_ref().map(|session| &session.profile)
    }

    /// Authorizes a command against the retained origin and advertised capabilities.
    ///
    /// # Errors
    /// Rejects a different origin or a command unsupported by retained evidence.
    pub fn authorize_command(
        &self,
        requested_origin: &CameraHttpOrigin,
        command: CameraCommand,
    ) -> Result<CameraCommandRequest, CameraCommandAuthorizationError> {
        let profile = self
            .profile()
            .ok_or(CameraCommandAuthorizationError::Unsupported)?;
        let origin = CameraHttpOrigin {
            address: profile.origin().address().to_string(),
            port: profile.origin().port(),
        };
        if requested_origin != &origin {
            return Err(CameraCommandAuthorizationError::OriginMismatch);
        }

        let target = match command {
            CameraCommand::StartRecording => profile
                .recording_command_target(NovatekRecordingCommand::Start)
                .map(|target| target.as_str().to_owned()),
            CameraCommand::StopRecording => profile
                .recording_command_target(NovatekRecordingCommand::Stop)
                .map(|target| target.as_str().to_owned()),
            CameraCommand::StillCapture => profile
                .still_capture_command_target(NovatekStillCaptureCommand)
                .map(|target| target.as_str().to_owned()),
        };
        let target = target.map_err(|_| CameraCommandAuthorizationError::Unsupported)?;

        Ok(CameraCommandRequest {
            origin,
            target,
            command,
            session_generation: self.generation.0,
        })
    }

    /// Interprets a response only while its complete authorization remains current.
    ///
    /// # Errors
    /// Returns `StaleSession` if the profile, origin, command or generation changed.
    pub fn complete_command(
        &self,
        request: &CameraCommandRequest,
        response: &[u8],
    ) -> Result<CameraCommandOutcome, CameraCommandCompletionError> {
        let Ok(current_request) = self.authorize_command(&request.origin, request.command) else {
            return Err(CameraCommandCompletionError::StaleSession);
        };
        if current_request.origin != request.origin
            || current_request.target != request.target
            || current_request.command != request.command
            || current_request.session_generation != request.session_generation
        {
            return Err(CameraCommandCompletionError::StaleSession);
        }

        Ok(
            match cutout_protocols::parse_command_response_for_id(
                response,
                request.command.expected_response_command_id(),
            ) {
                Ok(cutout_protocols::NovatekCommandOutcome::Acknowledged) => {
                    CameraCommandOutcome::Acknowledged
                }
                Ok(cutout_protocols::NovatekCommandOutcome::Refused { .. }) => {
                    CameraCommandOutcome::Refused
                }
                Ok(cutout_protocols::NovatekCommandOutcome::Unknown) | Err(_) => {
                    CameraCommandOutcome::Unknown
                }
            },
        )
    }

    #[must_use]
    pub fn media_is_current(&self, path: &str, size_bytes: u64) -> bool {
        self.retained.as_ref().is_some_and(|session| {
            session.read_only_snapshot.media().is_some_and(|media| {
                media
                    .entries()
                    .iter()
                    .any(|entry| entry.path() == path && entry.size_bytes() == size_bytes)
            })
        })
    }

    /// Reserves a one-shot download using the exact retained inventory entry.
    ///
    /// # Errors
    /// Rejects absent session/media evidence, invalid paths or exhausted operation IDs.
    pub fn authorize_media_download(
        &mut self,
        path: &str,
    ) -> Result<CameraMediaDownloadRequest, CameraMediaDownloadAuthorizationError> {
        let session = self
            .retained
            .as_ref()
            .ok_or(CameraMediaDownloadAuthorizationError::SessionUnavailable)?;
        let authorization = self
            .media_downloads
            .authorize(
                session
                    .read_only_snapshot
                    .media()
                    .ok_or(CameraMediaDownloadAuthorizationError::MediaNotRetained)?,
                path,
            )
            .map_err(|error| match error {
                NovatekMediaDownloadAuthorizationError::MediaNotRetained => {
                    CameraMediaDownloadAuthorizationError::MediaNotRetained
                }
                NovatekMediaDownloadAuthorizationError::InvalidPath(_) => {
                    CameraMediaDownloadAuthorizationError::InvalidPath
                }
                NovatekMediaDownloadAuthorizationError::OperationIdExhausted => {
                    CameraMediaDownloadAuthorizationError::OperationIdExhausted
                }
            })?;
        Ok(CameraMediaDownloadRequest {
            operation_id: authorization.id().get(),
            target: authorization.target().as_str().to_owned(),
            size_bytes: authorization.entry().size_bytes(),
        })
    }

    /// Consumes an authorization and checks the actual downloaded length.
    ///
    /// # Errors
    /// Rejects stale operations or a length different from retained camera evidence.
    pub fn complete_media_download(
        &mut self,
        operation_id: u64,
        actual_size: u64,
    ) -> Result<CameraMediaEntry, CameraMediaDownloadCompletionError> {
        let entry = self
            .media_downloads
            .consume_id(operation_id)
            .ok_or(CameraMediaDownloadCompletionError::StaleOperation)?;
        let entry = entry.entry();
        if actual_size != entry.size_bytes() {
            return Err(CameraMediaDownloadCompletionError::SizeMismatch {
                expected: entry.size_bytes(),
                actual: actual_size,
            });
        }
        Ok(CameraMediaEntry {
            name: entry.name().to_owned(),
            path: entry.path().to_owned(),
            size_bytes: entry.size_bytes(),
            timecode: entry.timecode(),
            time: entry.time().to_owned(),
            attributes: entry.attributes(),
        })
    }

    pub fn cancel_media_download(&mut self, operation_id: u64) -> bool {
        self.media_downloads.cancel_id(operation_id)
    }

    /// Builds provenance from the authorized entry and observed capture timing.
    ///
    /// # Errors
    /// Rejects stale download authorization or invalid association metadata.
    pub fn media_provenance_for_download(
        &self,
        operation_id: u64,
        association: &CameraMediaAssociation,
    ) -> Result<CoreCameraMediaProvenance, CameraMediaDownloadCompletionError> {
        let entry = self
            .media_downloads
            .authorized_entry_id(operation_id)
            .ok_or(CameraMediaDownloadCompletionError::StaleOperation)?;
        CoreCameraMediaProvenance::new(
            CoreCameraSourceKind::NovatekR3Pro,
            entry.path(),
            entry.size_bytes(),
            entry.timecode(),
            entry.time(),
            &association.ride_capture_file_name,
            CoreCameraMediaCaptureTiming {
                captured_at_monotonic: MonotonicTimestamp::new(
                    association.captured_at_monotonic_ms,
                ),
                captured_at_wall_clock: WallClockUnixTimestamp::new(
                    association.captured_at_wall_clock_ms,
                ),
                clock_uncertainty: association.clock_uncertainty,
            },
        )
        .map_err(|_| CameraMediaDownloadCompletionError::InvalidProvenance)
    }

    /// Builds a thumbnail target for a current inventory entry.
    ///
    /// # Errors
    /// Rejects stale inventory, absent thumbnail capability or an invalid path.
    pub fn media_thumbnail_target(
        &self,
        path: &str,
        size_bytes: u64,
    ) -> Result<String, CameraThumbnailTargetError> {
        if !self.media_is_current(path, size_bytes) {
            return Err(CameraThumbnailTargetError::MediaNotCurrent);
        }
        self.profile()
            .ok_or(CameraThumbnailTargetError::CapabilityNotAdvertised)?
            .media_thumbnail_target(path)
            .map(|target| target.as_str().to_owned())
            .map_err(|error| match error {
                NovatekMediaThumbnailError::Capability(_) => {
                    CameraThumbnailTargetError::CapabilityNotAdvertised
                }
                NovatekMediaThumbnailError::Path(_) => CameraThumbnailTargetError::InvalidPath,
            })
    }
}
