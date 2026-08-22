import Foundation

/// Connection truth for the optional local-network ride camera.
public enum CameraConnectionPresentation: Equatable, Sendable {
    case notConfigured
    case permissionRequired
    case wifiRequired
    case discovering
    case connected
    case unsupported
}

/// Foreground preview truth, kept independent from onboard recording.
public enum CameraPreviewPresentation: Equatable, Sendable {
    case stopped
    case buffering
    case live
    case stale
    case interrupted
    case unavailable
}

/// Authoritative camera recording truth when available.
public enum CameraRecordingPresentation: Equatable, Sendable {
    case unknown
    case stopped
    case recording
}

/// SD-card/storage truth when available.
public enum CameraStoragePresentation: Equatable, Sendable {
    case unknown
    case present
    case missing
    case error
}

/// Presentation-only camera state supplied by the future Apple network adapter.
///
/// The initial value is deliberately non-optimistic: it does not claim a
/// connection, preview, recording, or storage status before read-only evidence
/// arrives from the camera.
public struct CameraPresentation: Equatable, Sendable {
    public var connection: CameraConnectionPresentation
    public var preview: CameraPreviewPresentation
    public var recording: CameraRecordingPresentation
    public var storage: CameraStoragePresentation
    public var profileName: String?

    public init(
        connection: CameraConnectionPresentation = .notConfigured,
        preview: CameraPreviewPresentation = .stopped,
        recording: CameraRecordingPresentation = .unknown,
        storage: CameraStoragePresentation = .unknown,
        profileName: String? = nil
    ) {
        self.connection = connection
        self.preview = preview
        self.recording = recording
        self.storage = storage
        self.profileName = profileName
    }

    public static let initial = Self()
}
