import Foundation
import Network
import Observation
import CutoutMobileFFI

/// Coarse local-network path state needed before camera discovery.
public enum CameraLocalNetworkPathStatus: Equatable, Sendable {
    case unavailable
    case satisfied
}

/// Error building a request URL after Rust has validated the camera origin.
public enum CameraReadOnlyRequestError: Error, Equatable, Sendable {
    case invalidURL
}

/// Injectable transport used by the read-only camera loader and its tests.
public typealias CameraReadOnlyFetcher = @Sendable (URL) async throws -> Data

/// Failure while downloading one camera-reported media file.
public enum CameraMediaDownloadError: Error, Equatable, Sendable {
    case invalidPath
    case invalidURL
    case destinationExists
    case moveFailed
}

/// Fetches a streamed temporary file for a camera media URL.
public typealias CameraMediaDownloadFetcher = @Sendable (URL) async throws -> URL

/// Receives encoded preview frames for a platform-native renderer.
///
/// A throwing handler reports that the consumer could not use the frame. The
/// adapter treats that as an interrupted preview instead of claiming that a
/// transport-delivered but undisplayable stream is live.
public typealias CameraPreviewFrameHandler = @MainActor @Sendable (MobileCameraVideoFrameDto) async throws -> Void

/// Maps Apple path evidence to a conservative camera connection state.
public func cameraConnectionPresentation(
    pathStatus: CameraLocalNetworkPathStatus,
    usesWiFi: Bool
) -> CameraConnectionPresentation {
    guard pathStatus == .satisfied, usesWiFi else { return .wifiRequired }
    return .notConfigured
}

/// Apple-owned local-network readiness monitor for the selected camera path.
///
/// This monitor does not scan the LAN or infer a camera connection. It only
/// reports whether the phone currently has a usable Wi-Fi path; a future
/// selected-origin connection will supply the read-only Novatek evidence.
@MainActor
@Observable
public final class CameraLocalNetworkAdapter {
    public private(set) var presentation: CameraPresentation
    public private(set) var readOnlyEvidence: CameraReadOnlyEvidence?

    private let sessionState: CutoutSessionStateHandle
    private var monitor: NWPathMonitor?
    private var previewSession: MobileCameraPreviewSession?
    private var previewFileSink: MobileCameraPreviewFileSink?
    private var previewTask: Task<Void, Never>?
    private var previewFrameHandler: CameraPreviewFrameHandler?
    private let monitorQueue = DispatchQueue(label: "org.cutout.camera-local-network")

    public init(
        presentation: CameraPresentation = .initial,
        sessionState: CutoutSessionStateHandle = CutoutSessionStateHandle()
    ) {
        self.presentation = presentation
        self.readOnlyEvidence = nil
        self.sessionState = sessionState
        self.previewFrameHandler = nil
    }

    /// Starts observing the Wi-Fi path without adding a timeout or scanner.
    public func start() {
        guard monitor == nil else { return }

        let monitor = NWPathMonitor(requiredInterfaceType: .wifi)
        monitor.pathUpdateHandler = { [weak self] path in
            let status: CameraLocalNetworkPathStatus = path.status == .satisfied
                ? .satisfied
                : .unavailable
            let usesWiFi = path.usesInterfaceType(.wifi)
            Task { @MainActor [weak self] in
                self?.apply(pathStatus: status, usesWiFi: usesWiFi)
            }
        }
        monitor.start(queue: monitorQueue)
        self.monitor = monitor
    }

    /// Stops observing the path and returns to a non-optimistic initial state.
    public func stop() {
        stopPreview()
        monitor?.cancel()
        monitor = nil
        readOnlyEvidence = nil
        presentation = .initial
    }

    /// Applies a successful, read-only Novatek response set to the UI state.
    ///
    /// The response proves a local camera connection and storage state. It
    /// does not prove that a foreground RTSP preview or onboard recording is
    /// active, so those presentation values remain unchanged.
    public func apply(readOnlyEvidence evidence: CameraReadOnlyEvidence) {
        readOnlyEvidence = evidence
        presentation.connection = .connected
        presentation.profileName = "Novatek R3 Pro"
        presentation.storage = evidence.storagePresent ? .present : .missing
        refreshCameraState()
    }

    /// Records foreground preview truth in Rust and refreshes the UI snapshot.
    public func observePreview(_ preview: CameraPreviewPresentation) {
        sessionState.observeCameraPreview(preview: preview.dto)
        refreshCameraState()
    }

    /// Starts the foreground preview lifecycle and enters buffering state.
    public func startPreview() {
        reducePreviewEvent(.started)
    }

    /// Negotiates a local RTSP session and begins consuming encoded frames.
    ///
    /// The Retina session performs URI and local-origin validation before this
    /// adapter publishes buffering state. Encoded frame bytes are offered to
    /// the installed handler and optional file sink without changing state
    /// ownership.
    public func startPreview(uri: String) async throws {
        try await startPreview(uri: uri, destination: nil)
    }

    /// Negotiates a local RTSP session and saves its encoded H.264 frames.
    ///
    /// The destination is an Annex-B H.264 elementary stream. It is flushed
    /// when the stream ends or when this adapter is stopped; no application
    /// timeout is added.
    public func startPreview(uri: String, saveTo url: URL) async throws {
        try await startPreview(uri: uri, destination: url)
    }

    private func startPreview(uri: String, destination url: URL?) async throws {
        stopPreview()
        let session = try await MobileCameraPreviewSession.connect(uri: uri)
        let fileSink = try url.map {
            try MobileCameraPreviewFileSink.create(path: $0.path)
        }
        previewSession = session
        previewFileSink = fileSink
        startPreview()
        previewTask = Task { [weak self, session, fileSink] in
            defer { try? fileSink?.finish() }
            do {
                while !Task.isCancelled {
                    guard let frame = try await session.nextVideoFrame() else { break }
                    try fileSink?.writeFrame(frame: frame)
                    guard let self else { break }
                    try await self.previewFrameHandler?(frame)
                    self.recordPreviewFrame()
                }
                if !Task.isCancelled {
                    self?.interruptPreview()
                }
            } catch {
                if !Task.isCancelled {
                    self?.interruptPreview()
                }
            }
        }
    }

    /// Records that the RTSP transport delivered an encoded video frame.
    public func recordPreviewFrame() {
        reducePreviewEvent(.frameReceived)
    }

    /// Installs the consumer for encoded frames delivered by the preview.
    ///
    /// The adapter does not decode frames itself, allowing the app to choose
    /// a native renderer or a file-only consumer without changing transport.
    public func setPreviewFrameHandler(_ handler: CameraPreviewFrameHandler?) {
        previewFrameHandler = handler
    }

    /// Records an unexpected preview interruption without changing recording truth.
    public func interruptPreview() {
        reducePreviewEvent(.interrupted)
    }

    /// Stops the foreground preview lifecycle.
    public func stopPreview() {
        previewTask?.cancel()
        previewTask = nil
        previewSession?.stop()
        previewSession = nil
        try? previewFileSink?.finish()
        previewFileSink = nil
        reducePreviewEvent(.stopped)
    }

    /// Records onboard recording truth in Rust and refreshes the UI snapshot.
    public func observeOnboardRecording(_ recording: CameraRecordingPresentation) {
        sessionState.observeCameraOnboardRecording(onboardRecording: recording.dto)
        refreshCameraState()
    }

    /// Loads the verified R3 Pro read-only response set through an injected
    /// Apple transport and parses it through the Rust UniFFI boundary.
    ///
    /// The fetcher owns cancellation and transport policy. This method does
    /// not add a timeout, scanner, or write-capable camera command.
    public func loadReadOnlyEvidence(
        address: String,
        port: UInt16,
        fetch: @escaping CameraReadOnlyFetcher
    ) async throws -> CameraReadOnlyEvidence {
        clearReadOnlyEvidence()
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)

        let firmware = try await fetch(try requestURL(origin: origin, command: .firmwareVersion))
        let liveView = try await fetch(try requestURL(origin: origin, command: .liveViewFormat))
        let configuration = try await fetch(try requestURL(origin: origin, command: .configuration))
        let storage = try await fetch(try requestURL(origin: origin, command: .storagePresent))
        let media = try await fetch(try requestURL(origin: origin, command: .mediaList))

        let snapshot = try mobileParseNovatekReadOnlySnapshot(
            firmwareResponse: firmware,
            liveViewResponse: liveView,
            configurationResponse: configuration,
            storageResponse: storage,
            mediaResponse: media
        )
        let evidence = CameraReadOnlyEvidence(snapshot)
        apply(readOnlyEvidence: evidence)
        return evidence
    }

    /// Loads read-only evidence using Apple's URL loading stack.
    ///
    /// The camera origin is still supplied by the caller; no LAN scan or
    /// guessed destination is performed. URL loading cancellation propagates
    /// through the async call, and no custom request timeout is installed.
    public func loadReadOnlyEvidence(
        address: String,
        port: UInt16
    ) async throws -> CameraReadOnlyEvidence {
        try await loadReadOnlyEvidence(address: address, port: port) { url in
            try await URLSession.shared.data(from: url).0
        }
    }

    /// Downloads one media entry to a new local file without adding a timeout.
    ///
    /// The camera path is converted by Rust before the caller-owned local
    /// origin is applied. The destination must not already exist; this keeps a
    /// failed or cancelled transfer from replacing an existing capture.
    public func downloadMedia(
        address: String,
        port: UInt16,
        media: CameraMediaEvidence,
        to destination: URL,
        fetch: @escaping CameraMediaDownloadFetcher
    ) async throws {
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        let target: String
        do {
            target = try mobileNovatekMediaDownloadTarget(path: media.path)
        } catch {
            throw CameraMediaDownloadError.invalidPath
        }
        guard let url = URL(string: "http://\(origin.address):\(origin.port)\(target)") else {
            throw CameraMediaDownloadError.invalidURL
        }
        guard !FileManager.default.fileExists(atPath: destination.path) else {
            throw CameraMediaDownloadError.destinationExists
        }

        let temporaryURL = try await fetch(url)
        do {
            try FileManager.default.moveItem(at: temporaryURL, to: destination)
        } catch {
            throw CameraMediaDownloadError.moveFailed
        }
    }

    /// Downloads one media entry with Apple's streaming URL-loading stack.
    ///
    /// URL loading owns cancellation and transfer lifetime; no application
    /// timeout is installed for this potentially large SD-card file.
    public func downloadMedia(
        address: String,
        port: UInt16,
        media: CameraMediaEvidence,
        to destination: URL
    ) async throws {
        try await downloadMedia(
            address: address,
            port: port,
            media: media,
            to: destination
        ) { url in
            try await URLSession.shared.download(from: url).0
        }
    }

    private func apply(pathStatus: CameraLocalNetworkPathStatus, usesWiFi: Bool) {
        presentation.connection = cameraConnectionPresentation(
            pathStatus: pathStatus,
            usesWiFi: usesWiFi
        )
    }

    private func clearReadOnlyEvidence() {
        readOnlyEvidence = nil
        presentation.connection = .notConfigured
        presentation.profileName = nil
        presentation.storage = .unknown
    }

    private func requestURL(
        origin: MobileNovatekHttpOriginDto,
        command: MobileNovatekReadCommandDto
    ) throws -> URL {
        guard let url = URL(string: "http://\(origin.address):\(origin.port)\(mobileNovatekReadCommandTarget(command: command))") else {
            throw CameraReadOnlyRequestError.invalidURL
        }
        return url
    }

    private func refreshCameraState() {
        let snapshot = sessionState.cameraSnapshot()
        presentation.preview = snapshot.preview.presentation
        presentation.recording = snapshot.onboardRecording.presentation
    }

    private func reducePreviewEvent(_ event: MobileCameraPreviewEventDto) {
        sessionState.reduceCameraPreview(event: event)
        refreshCameraState()
    }
}

private extension CameraPreviewPresentation {
    var dto: MobileCameraPreviewStateDto {
        switch self {
        case .stopped: .stopped
        case .buffering: .buffering
        case .live: .live
        case .stale: .stale
        case .interrupted: .interrupted
        case .unavailable: .unavailable
        }
    }
}

private extension CameraRecordingPresentation {
    var dto: MobileCameraOnboardRecordingStateDto {
        switch self {
        case .unknown: .unknown
        case .stopped: .stopped
        case .recording: .recording
        }
    }
}

private extension MobileCameraPreviewStateDto {
    var presentation: CameraPreviewPresentation {
        switch self {
        case .stopped: .stopped
        case .buffering: .buffering
        case .live: .live
        case .stale: .stale
        case .interrupted: .interrupted
        case .unavailable: .unavailable
        }
    }
}

private extension MobileCameraOnboardRecordingStateDto {
    var presentation: CameraRecordingPresentation {
        switch self {
        case .unknown: .unknown
        case .stopped: .stopped
        case .recording: .recording
        }
    }
}
