import CutoutMobileFFI
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

/// Result of a camera control request, kept distinct from recording readback.
public enum CameraCommandOutcome: Equatable, Sendable {
    /// The camera response reported status zero.
    case acknowledged
    /// The camera response reported a nonzero status.
    case refused
    /// The transport reported a timeout.
    case timedOut
    /// The response did not provide a usable status.
    case unknown
    /// The transport failed for a non-timeout reason.
    case failed
}

/// SD-card/storage truth when available.
public enum CameraStoragePresentation: Equatable, Sendable {
    case unknown
    case present
    case missing
    case error
}

/// One command/status observation returned by the Rust Novatek parser.
public struct CameraCommandStatusEvidence: Equatable, Sendable {
    public let commandID: UInt16
    public let status: UInt16

    public init(commandID: UInt16, status: UInt16) {
        self.commandID = commandID
        self.status = status
    }
}

extension CameraCommandStatusEvidence {
    init(_ dto: MobileNovatekCommandStatusDto) {
        self.init(commandID: dto.commandId, status: dto.status)
    }
}

/// One bounded media record returned by the Rust Novatek parser.
public struct CameraMediaEvidence: Equatable, Sendable {
    public let name: String
    public let path: String
    public let sizeBytes: UInt64
    public let timecode: UInt64
    public let time: String
    public let attributes: UInt32

    public init(
        name: String,
        path: String,
        sizeBytes: UInt64,
        timecode: UInt64,
        time: String,
        attributes: UInt32
    ) {
        self.name = name
        self.path = path
        self.sizeBytes = sizeBytes
        self.timecode = timecode
        self.time = time
        self.attributes = attributes
    }
}

extension CameraMediaEvidence {
    init(_ dto: MobileNovatekMediaEntryDto) {
        self.init(
            name: dto.name,
            path: dto.path,
            sizeBytes: dto.sizeBytes,
            timecode: dto.timecode,
            time: dto.time,
            attributes: dto.attributes
        )
    }
}

/// The confidence available for the relationship between camera and phone
/// clocks. Unknown is retained instead of treating a camera timestamp as a
/// phone timestamp.
public enum CameraClockUncertainty: Equatable, Sendable {
    case unknown
    case milliseconds(UInt64)
}

/// A verified local media file associated with the active ride capture.
///
/// This reference carries metadata and a local URL only; camera video bytes
/// remain outside PEVCAP and SQLite.
public struct CameraMediaReference: Equatable, Sendable {
    public let cameraPath: String
    public let localURL: URL
    public let sizeBytes: UInt64
    public let cameraTimecode: UInt64
    public let cameraTime: String
    public let rideCaptureFileName: String
    public let clockUncertainty: CameraClockUncertainty

    public init(
        cameraPath: String,
        localURL: URL,
        sizeBytes: UInt64,
        cameraTimecode: UInt64,
        cameraTime: String,
        rideCaptureFileName: String,
        clockUncertainty: CameraClockUncertainty
    ) {
        self.cameraPath = cameraPath
        self.localURL = localURL
        self.sizeBytes = sizeBytes
        self.cameraTimecode = cameraTimecode
        self.cameraTime = cameraTime
        self.rideCaptureFileName = rideCaptureFileName
        self.clockUncertainty = clockUncertainty
    }
}

/// Read-only Novatek evidence parsed by Rust before reaching the Swift UI.
public struct CameraReadOnlyEvidence: Equatable, Sendable {
    public let firmwareVersion: String
    public let movieRTSPURI: String
    public let photoRTSPURI: String
    public let configuration: [CameraCommandStatusEvidence]
    public let storagePresent: Bool
    public let media: [CameraMediaEvidence]

    /// Number of bounded media records returned by the camera.
    public var mediaCount: Int { media.count }

    /// Whether the observed firmware belongs to the verified R3 Pro family.
    ///
    /// The adapter must not label another Novatek camera as an R3 Pro merely
    /// because its response shape is parseable.
    public var isR3ProProfile: Bool {
        mobileNovatekFirmwareIsR3Pro(firmwareVersion: firmwareVersion)
    }

    /// Whether read-only configuration evidence advertises the Novatek
    /// onboard-recording command. This is capability evidence, not recording
    /// state readback.
    public var supportsOnboardRecording: Bool {
        advertises(commandID: 2001)
    }

    /// Whether read-only configuration evidence advertises still capture.
    /// This is capability evidence, not a capture acknowledgement or media
    /// readback.
    public var supportsStillCapture: Bool {
        advertises(commandID: 1001)
    }

    /// Whether read-only configuration evidence advertises thumbnail requests.
    public var supportsMediaThumbnails: Bool {
        advertises(commandID: 4001)
    }

    public init(_ snapshot: MobileNovatekReadOnlySnapshotDto) {
        self.init(
            firmwareVersion: snapshot.firmwareVersion,
            movieRTSPURI: snapshot.movieRtspUri,
            photoRTSPURI: snapshot.photoRtspUri,
            configuration: snapshot.configuration.map(CameraCommandStatusEvidence.init),
            storagePresent: snapshot.storagePresent,
            media: snapshot.media.map(CameraMediaEvidence.init)
        )
    }

    public init(
        firmwareVersion: String,
        movieRTSPURI: String,
        photoRTSPURI: String,
        configuration: [CameraCommandStatusEvidence],
        storagePresent: Bool,
        media: [CameraMediaEvidence]
    ) {
        self.firmwareVersion = firmwareVersion
        self.movieRTSPURI = movieRTSPURI
        self.photoRTSPURI = photoRTSPURI
        self.configuration = configuration
        self.storagePresent = storagePresent
        self.media = media
    }

    private func advertises(commandID: UInt16) -> Bool {
        configuration.contains { $0.commandID == commandID && $0.status == 0 }
    }
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
