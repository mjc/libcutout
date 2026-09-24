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
    case unsupportedProfile
    case pathUnavailable
    case originMismatch
    case responseTooLarge
    case unsupportedCapability
    case unexpectedHTTPStatus(Int)
}

/// Local rejection that prevents overlapping camera control requests.
public enum CameraCommandRequestError: Error, Equatable, Sendable {
    case inFlight
    case pathUnavailable
    case originMismatch
    case unsupported
}

/// Injectable transport used by the read-only camera loader and its tests.
public typealias CameraReadOnlyFetcher = @Sendable (URL) async throws -> Data

private let maximumCameraThumbnailBytes = 2 * 1024 * 1024
private let maximumCameraReadResponseBytes = 512 * 1024
private let cameraReadTimeoutSeconds: TimeInterval = 10

private final class CameraURLSessionDelegate: NSObject, URLSessionTaskDelegate, URLSessionDownloadDelegate {
    let maximumDownloadBytes: Int64?

    init(maximumDownloadBytes: Int64? = nil) {
        self.maximumDownloadBytes = maximumDownloadBytes
    }

    func urlSession(
        _: URLSession,
        task _: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest,
        completionHandler: @escaping (URLRequest?) -> Void
    ) {
        _ = response
        _ = request
        // These are fixed command targets. Even a same-origin redirect could
        // turn a read into a mutating GET, so no redirect is followed.
        completionHandler(nil)
    }

    func urlSession(
        _: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData _: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite _: Int64
    ) {
        if let maximumDownloadBytes, totalBytesWritten > maximumDownloadBytes {
            downloadTask.cancel()
        }
    }

    func urlSession(_: URLSession, downloadTask _: URLSessionDownloadTask, didFinishDownloadingTo _: URL) {}
}

private func cameraURLMatchesOrigin(
    _ url: URL?,
    origin: MobileNovatekHttpOriginDto
) -> Bool {
    guard let url,
          url.scheme?.lowercased() == "http",
          url.host == origin.address,
          (url.port ?? 80) == Int(origin.port)
    else {
        return false
    }
    return true
}

private func cameraSession(maximumDownloadBytes: Int64? = nil) -> URLSession {
    URLSession(
        configuration: .ephemeral,
        delegate: CameraURLSessionDelegate(maximumDownloadBytes: maximumDownloadBytes),
        delegateQueue: nil
    )
}

private func invalidateCameraSession(_ session: URLSession) {
    session.finishTasksAndInvalidate()
}

private func cameraData(
    from url: URL,
    origin: MobileNovatekHttpOriginDto,
    maximumBytes: Int = maximumCameraReadResponseBytes
) async throws -> Data {
    let session = cameraSession()
    defer { invalidateCameraSession(session) }
    var request = URLRequest(url: url)
    request.timeoutInterval = cameraReadTimeoutSeconds
    let (bytes, response) = try await session.bytes(for: request)
    guard cameraResponseMatchesOrigin(response, origin: origin) else {
        throw CameraReadOnlyRequestError.originMismatch
    }
    if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
        throw CameraReadOnlyRequestError.unexpectedHTTPStatus(http.statusCode)
    }
    if response.expectedContentLength > Int64(maximumBytes) {
        throw CameraReadOnlyRequestError.responseTooLarge
    }
    var data = Data()
    data.reserveCapacity(min(maximumBytes, max(0, Int(response.expectedContentLength))))
    for try await byte in bytes {
        data.append(byte)
        if data.count > maximumBytes {
            throw CameraReadOnlyRequestError.responseTooLarge
        }
    }
    return data
}

private func cameraDownload(
    from url: URL,
    origin: MobileNovatekHttpOriginDto,
    maximumBytes: UInt64
) async throws -> URL {
    let session = cameraSession(maximumDownloadBytes: Int64(clamping: maximumBytes))
    defer { invalidateCameraSession(session) }
    let (downloadedURL, response) = try await session.download(from: url)
    guard cameraURLMatchesOrigin(response.url, origin: origin) else {
        throw CameraMediaDownloadError.originMismatch
    }
    if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
        throw CameraMediaDownloadError.unexpectedHTTPStatus(http.statusCode)
    }
    if response.expectedContentLength >= 0,
       UInt64(response.expectedContentLength) > maximumBytes {
        throw CameraMediaDownloadError.responseTooLarge
    }
    let temporaryURL = FileManager.default.temporaryDirectory
        .appendingPathComponent("cutout-camera-download-\(UUID().uuidString).tmp")
    do {
        let attributes = try FileManager.default.attributesOfItem(atPath: downloadedURL.path)
        let count = (attributes[.size] as? NSNumber)?.uint64Value ?? 0
        guard count <= maximumBytes else {
            throw CameraMediaDownloadError.responseTooLarge
        }
        try FileManager.default.moveItem(at: downloadedURL, to: temporaryURL)
        return temporaryURL
    } catch {
        try? FileManager.default.removeItem(at: temporaryURL)
        throw error
    }
}

func cameraResponseMatchesOrigin(
    _ response: URLResponse?,
    origin: MobileNovatekHttpOriginDto
) -> Bool {
    cameraURLMatchesOrigin(response?.url, origin: origin)
}

/// Failure while downloading one camera-reported media file.
public enum CameraMediaDownloadError: Error, Equatable, Sendable {
    case invalidPath
    case invalidURL
    case pathUnavailable
    case originMismatch
    case destinationExists
    case sizeMismatch(expected: UInt64, actual: UInt64)
    case responseTooLarge
    case unexpectedHTTPStatus(Int)
    case moveFailed
}

/// Fetches a streamed temporary file for a camera media URL.
///
/// Ownership of the returned file transfers to the adapter, which removes it
/// unless the file is moved to the requested destination.
public typealias CameraMediaDownloadFetcher = @Sendable (URL) async throws -> URL

/// Receives encoded preview frames for a platform-native renderer.
///
/// A throwing handler reports that the consumer could not use the frame. The
/// adapter treats that as an interrupted preview instead of claiming that a
/// transport-delivered but undisplayable stream is live.
public typealias CameraPreviewFrameHandler = @MainActor @Sendable (MobileCameraVideoFrameDto) async throws -> Bool
public typealias CameraPreviewConfigurationHandler = @MainActor @Sendable (MobileCameraVideoConfigurationDto) throws -> Void

/// Maps Apple path evidence to a conservative camera connection state.
public func cameraConnectionPresentation(
    pathStatus: CameraLocalNetworkPathStatus,
    usesWiFi: Bool,
    hasReadOnlyEvidence: Bool = false
) -> CameraConnectionPresentation {
    guard pathStatus == .satisfied, usesWiFi else { return .wifiRequired }
    return hasReadOnlyEvidence ? .connected : .notConfigured
}

/// Apple-owned local-network readiness monitor for the selected camera path.
///
/// This monitor does not scan the LAN or infer a camera connection. It only
/// reports whether the phone currently has a usable Wi-Fi path; the caller
/// supplies the selected origin used to load read-only Novatek evidence.
@MainActor
@Observable
public final class CameraLocalNetworkAdapter {
    public private(set) var presentation: CameraPresentation
    public private(set) var readOnlyEvidence: CameraReadOnlyEvidence?
    public private(set) var savedPreviewFileURL: URL?

    private let sessionState: CutoutSessionStateHandle
    private var monitor: NWPathMonitor?
    private var previewSession: MobileCameraPreviewSession?
    private var previewFileSink: MobileCameraPreviewFileSink?
    private var previewFileState = CameraPreviewFileState(destination: nil)
    private var previewTask: Task<Void, Never>?
    private var previewFrameHandler: CameraPreviewFrameHandler?
    private var previewConfigurationHandler: CameraPreviewConfigurationHandler?
    private var commandInFlight = false
    private var pathObservationGeneration: UInt64 = 0
    private var evidencePathObservationGeneration: UInt64 = 0
    private let monitorQueue = DispatchQueue(label: "org.cutout.camera-local-network")

    public init(
        presentation: CameraPresentation = .initial,
        sessionState: CutoutSessionStateHandle = CutoutSessionStateHandle()
    ) {
        self.presentation = presentation
        self.readOnlyEvidence = nil
        self.savedPreviewFileURL = nil
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
        invalidateCameraLifecycle()
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
    public func apply(
        readOnlyEvidence evidence: CameraReadOnlyEvidence,
        origin: MobileNovatekHttpOriginDto? = nil
    ) {
        guard evidence.isR3ProProfile else {
            invalidateCameraLifecycle()
            readOnlyEvidence = nil
            presentation.connection = .unsupported
            presentation.profileName = nil
            presentation.storage = .unknown
            refreshCameraState()
            return
        }
        readOnlyEvidence = evidence
        sessionState.clearNovatekSession()
        if let origin {
            try? sessionState.configureNovatekReadOnlySession(
                origin: origin,
                snapshot: evidence.dto
            )
        }
        evidencePathObservationGeneration = pathObservationGeneration
        presentation.connection = .connected
        presentation.profileName = "FreedConn R3 Pro · Novatek"
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
    /// The RTSP session performs URI and local-origin validation before this
    /// adapter publishes buffering state. Encoded frame bytes are offered to
    /// the installed handler and optional file sink without changing state
    /// ownership.
    public func startPreview(uri: String) async throws {
        try await startPreview(uri: uri, destination: nil, expectedAddress: nil)
    }

    /// Negotiates a preview only when its RTSP host matches the selected camera.
    public func startPreview(uri: String, expectedAddress: String) async throws {
        try await startPreview(uri: uri, destination: nil, expectedAddress: expectedAddress)
    }

    /// Negotiates a local RTSP session and saves its encoded H.264 frames.
    ///
    /// The destination is an Annex-B H.264 elementary stream. It is flushed
    /// when the stream ends or when this adapter is stopped; no application
    /// timeout is added.
    public func startPreview(uri: String, saveTo url: URL) async throws {
        try await startPreview(uri: uri, destination: url, expectedAddress: nil)
    }

    /// Negotiates and saves a preview bound to the selected camera address.
    public func startPreview(uri: String, expectedAddress: String, saveTo url: URL) async throws {
        try await startPreview(uri: uri, destination: url, expectedAddress: expectedAddress)
    }

    private func startPreview(
        uri: String,
        destination url: URL?,
        expectedAddress: String?
    ) async throws {
        stopPreview()
        let token = sessionState.cameraSessionToken()
        let session: MobileCameraPreviewSession
        guard let readOnlyOrigin = sessionState.novatekSessionOrigin() else {
            throw CameraReadOnlyRequestError.pathUnavailable
        }
        if let expectedAddress, expectedAddress != readOnlyOrigin.address {
            throw CameraReadOnlyRequestError.originMismatch
        }
        do {
            session = try await mobileCameraPreviewConnectForOrigin(
                uri: uri,
                expectedAddress: readOnlyOrigin.address
            )
        } catch MobileCameraPreviewError.OriginMismatch {
            throw CameraReadOnlyRequestError.originMismatch
        }
        try requireCurrentPreview(
            token: token,
            session: session,
            fileSink: nil
        )
        let fileSink = try url.map {
            try MobileCameraPreviewFileSink.create(path: $0.path)
        }
        try requireCurrentPreview(token: token, session: session, fileSink: fileSink)
        if let configuration = session.videoConfiguration() {
            do {
                try previewConfigurationHandler?(configuration)
            } catch {
                try? fileSink?.finish()
                session.stop()
                throw error
            }
        }
        try requireCurrentPreview(token: token, session: session, fileSink: fileSink)
        previewSession = session
        previewFileSink = fileSink
        previewFileState = CameraPreviewFileState(destination: url)
        if url != nil {
            savedPreviewFileURL = nil
        }
        startPreview()
        let frameHandler = previewFrameHandler
        previewTask = Task.detached { [weak self, session, fileSink, frameHandler] in
            do {
                while !Task.isCancelled {
                    guard let frame = try await session.nextVideoFrame() else { break }
                    guard let self else { break }
                    guard await MainActor.run(body: {
                        self.isCurrentPreview(token: token)
                    }) else { break }
                    try fileSink?.writeFrame(frame: frame)
                    let rendered = try await frameHandler?(frame) ?? true
                    guard !Task.isCancelled else { break }
                    let savedURL: URL? = await MainActor.run {
                        guard self.isCurrentPreview(token: token) else { return nil }
                        return self.previewFileState.recordFrame()
                    }
                    if let savedURL {
                        await MainActor.run {
                            guard self.isCurrentPreview(token: token) else { return }
                            self.savedPreviewFileURL = savedURL
                        }
                    }
                    guard rendered else { continue }
                    await MainActor.run {
                        guard self.isCurrentPreview(token: token) else { return }
                        self.recordPreviewFrame()
                    }
                }
                if !Task.isCancelled {
                    let terminated = await MainActor.run {
                        self?.terminatePreviewAfterTask(
                            token: token,
                            session: session,
                            fileSink: fileSink
                        ) ?? false
                    }
                    if !terminated {
                        session.stop()
                        try? fileSink?.finish()
                    }
                } else {
                    session.stop()
                }
                try? fileSink?.finish()
            } catch {
                if !Task.isCancelled {
                    let terminated = await MainActor.run {
                        self?.terminatePreviewAfterTask(
                            token: token,
                            session: session,
                            fileSink: fileSink
                        ) ?? false
                    }
                    if !terminated {
                        try? fileSink?.finish()
                        session.stop()
                    }
                } else {
                    session.stop()
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

    /// Installs the consumer for SDP-advertised codec configuration.
    public func setPreviewConfigurationHandler(_ handler: CameraPreviewConfigurationHandler?) {
        previewConfigurationHandler = handler
    }

    /// Records an unexpected preview interruption without changing recording truth.
    public func interruptPreview() {
        reducePreviewEvent(.interrupted)
    }

    private func terminatePreviewAfterTask(
        token: MobileCameraSessionTokenDto,
        session: MobileCameraPreviewSession,
        fileSink: MobileCameraPreviewFileSink?
    ) -> Bool {
        guard isCurrentPreview(token: token) else { return false }
        session.stop()
        try? fileSink?.finish()
        previewSession = nil
        previewFileSink = nil
        previewTask = nil
        reducePreviewEvent(.interrupted)
        return true
    }

    private func isCurrentPreview(token: MobileCameraSessionTokenDto) -> Bool {
        sessionState.cameraSessionTokenIsCurrent(token: token)
    }

    private func requireCurrentPreview(
        token: MobileCameraSessionTokenDto,
        session: MobileCameraPreviewSession,
        fileSink: MobileCameraPreviewFileSink?
    ) throws {
        guard isCurrentPreview(token: token), !Task.isCancelled else {
            try? fileSink?.finish()
            session.stop()
            throw CancellationError()
        }
    }

    /// Stops the foreground preview lifecycle.
    public func stopPreview() {
        sessionState.advanceCameraGeneration()
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
        guard !presentation.connection.blocksReadOnlyDiscovery else {
            throw CameraReadOnlyRequestError.pathUnavailable
        }
        clearReadOnlyEvidence()
        let requestToken = sessionState.cameraSessionToken()
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        observeDiscoveryStarted()

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
        guard isCurrentCameraRequest(requestToken) else {
            throw CameraReadOnlyRequestError.pathUnavailable
        }
        let evidence = CameraReadOnlyEvidence(snapshot)
        guard evidence.isR3ProProfile else {
            apply(readOnlyEvidence: evidence, origin: origin)
            throw CameraReadOnlyRequestError.unsupportedProfile
        }
        apply(readOnlyEvidence: evidence, origin: origin)
        return evidence
    }

    /// Loads read-only evidence using Apple's URL loading stack.
    ///
    /// The camera origin is still supplied by the caller; no LAN scan or
    /// guessed destination is performed. URL loading cancellation propagates
    /// through the async call; bounded metadata requests use a finite
    /// connection/read timeout while media downloads remain cancellable streams.
    public func loadReadOnlyEvidence(
        address: String,
        port: UInt16
    ) async throws -> CameraReadOnlyEvidence {
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        return try await loadReadOnlyEvidence(address: address, port: port) { url in
            try await cameraData(from: url, origin: origin)
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
        let requestToken = sessionState.cameraSessionToken()
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        guard readOnlyOriginMatches(origin) else {
            throw CameraMediaDownloadError.originMismatch
        }
        guard sessionState.novatekMediaIsCurrent(
            path: media.path,
            sizeBytes: media.sizeBytes
        ) else {
            throw CameraMediaDownloadError.pathUnavailable
        }
        let temporaryURL = try await Self.fetchMedia(
            origin: origin,
            media: media,
            destination: destination,
            fetch: fetch
        )
        defer { try? FileManager.default.removeItem(at: temporaryURL) }
        guard isCurrentCameraRequest(requestToken) else {
            throw CameraMediaDownloadError.pathUnavailable
        }
        try await Self.installMedia(from: temporaryURL, to: destination)
    }

    private nonisolated static func fetchMedia(
        origin: MobileNovatekHttpOriginDto,
        media: CameraMediaEvidence,
        destination: URL,
        fetch: @escaping CameraMediaDownloadFetcher
    ) async throws -> URL {
        let target: String
        do {
            target = try mobileNovatekMediaDownloadTarget(path: media.path)
        } catch {
            throw CameraMediaDownloadError.invalidPath
        }
        let url: URL
        do {
            guard let requestURL = URL(string: "http://\(origin.address):\(origin.port)\(target)") else {
                throw CameraMediaDownloadError.invalidURL
            }
            url = requestURL
        } catch {
            throw CameraMediaDownloadError.invalidURL
        }
        guard !FileManager.default.fileExists(atPath: destination.path) else {
            throw CameraMediaDownloadError.destinationExists
        }

        try Task.checkCancellation()
        let temporaryURL = try await fetch(url)
        var keepTemporaryFile = false
        defer {
            if !keepTemporaryFile {
                try? FileManager.default.removeItem(at: temporaryURL)
            }
        }
        try Task.checkCancellation()
        guard
            let attributes = try? FileManager.default.attributesOfItem(atPath: temporaryURL.path),
            let fileSize = attributes[.size] as? NSNumber
        else {
            throw CameraMediaDownloadError.moveFailed
        }
        let actualSize = fileSize.uint64Value
        guard actualSize == media.sizeBytes else {
            throw CameraMediaDownloadError.sizeMismatch(
                expected: media.sizeBytes,
                actual: actualSize
            )
        }
        keepTemporaryFile = true
        return temporaryURL
    }

    private nonisolated static func installMedia(
        from temporaryURL: URL,
        to destination: URL
    ) async throws {
        do {
            try FileManager.default.moveItem(at: temporaryURL, to: destination)
        } catch {
            throw CameraMediaDownloadError.moveFailed
        }
    }

    /// Fetches a camera-generated thumbnail for a media entry.
    ///
    /// Rust validates the retained media record and command `4001` capability
    /// before returning the request target.
    /// The response is returned as-is so the platform UI can decode it without
    /// moving image types across the Rust boundary.
    public func fetchMediaThumbnail(
        address: String,
        port: UInt16,
        media: CameraMediaEvidence,
        fetch: @escaping CameraReadOnlyFetcher
    ) async throws -> Data {
        let requestToken = sessionState.cameraSessionToken()
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        guard readOnlyOriginMatches(origin) else {
            throw CameraReadOnlyRequestError.originMismatch
        }
        let target: String
        do {
            target = try sessionState.novatekMediaThumbnailTarget(
                path: media.path,
                sizeBytes: media.sizeBytes
            )
        } catch MobileNovatekThumbnailTargetError.CapabilityNotAdvertised {
            throw CameraReadOnlyRequestError.unsupportedCapability
        } catch MobileNovatekThumbnailTargetError.MediaNotCurrent {
            throw CameraReadOnlyRequestError.pathUnavailable
        } catch MobileNovatekThumbnailTargetError.InvalidPath {
            throw CameraReadOnlyRequestError.invalidURL
        }
        let data = try await fetch(try requestURL(origin: origin, target: target))
        guard isCurrentCameraRequest(requestToken) else {
            throw CameraReadOnlyRequestError.pathUnavailable
        }
        guard data.count <= maximumCameraThumbnailBytes else {
            throw CameraReadOnlyRequestError.responseTooLarge
        }
        return data
    }

    /// Fetches a camera-generated thumbnail through Apple's URL-loading stack.
    public func fetchMediaThumbnail(
        address: String,
        port: UInt16,
        media: CameraMediaEvidence
    ) async throws -> Data {
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        return try await fetchMediaThumbnail(address: address, port: port, media: media) { url in
            try await cameraData(
                from: url,
                origin: origin,
                maximumBytes: maximumCameraThumbnailBytes
            )
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
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        return try await downloadMedia(
            address: address,
            port: port,
            media: media,
            to: destination
        ) { url in
            try await cameraDownload(
                from: url,
                origin: origin,
                maximumBytes: media.sizeBytes
            )
        }
    }

    /// Sends an explicit onboard-recording request to the camera.
    ///
    /// Acknowledged/refused response status is returned separately from
    /// recording readback, so this method leaves `presentation.recording`
    /// unchanged.
    public func requestOnboardRecording(
        address: String,
        port: UInt16,
        start: Bool,
        fetch: @escaping CameraReadOnlyFetcher
    ) async throws -> CameraCommandOutcome {
        let origin = try commandContext(address: address, port: port)
        let command: MobileNovatekRecordingCommandDto = start ? .start : .stop
        let target: String
        do {
            target = try sessionState.novatekRecordingCommandTarget(command: command)
        } catch {
            throw CameraCommandRequestError.unsupported
        }
        return try await requestCommand(
            url: requestURL(
                origin: origin,
                target: target
            ),
            expectedCommandID: 2001,
            fetch: fetch
        )
    }

    /// Sends an onboard-recording request through Apple's URL-loading stack.
    ///
    /// URL loading owns cancellation and transfer lifetime; no application
    /// timeout is installed.
    public func requestOnboardRecording(
        address: String,
        port: UInt16,
        start: Bool
    ) async throws -> CameraCommandOutcome {
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        return try await requestOnboardRecording(address: address, port: port, start: start) { url in
            do {
                return try await cameraData(from: url, origin: origin)
            } catch CameraReadOnlyRequestError.originMismatch {
                throw CameraCommandRequestError.originMismatch
            }
        }
    }

    /// Sends an explicit still-capture request when read-only evidence
    /// advertises command `1001`.
    ///
    /// The HTTP response only proves transport acceptance. Callers must wait
    /// for a later media-list response before presenting a captured file.
    public func requestStillCapture(
        address: String,
        port: UInt16,
        fetch: @escaping CameraReadOnlyFetcher
    ) async throws -> CameraCommandOutcome {
        let origin = try commandContext(address: address, port: port)
        let target: String
        do {
            target = try sessionState.novatekStillCaptureCommandTarget()
        } catch {
            throw CameraCommandRequestError.unsupported
        }
        return try await requestCommand(
            url: requestURL(
                origin: origin,
                target: target
            ),
            expectedCommandID: 1001,
            fetch: fetch
        )
    }

    /// Sends a still-capture request through Apple's URL-loading stack.
    ///
    /// URL loading owns cancellation and transfer lifetime; no application
    /// timeout is installed.
    public func requestStillCapture(
        address: String,
        port: UInt16
    ) async throws -> CameraCommandOutcome {
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        return try await requestStillCapture(address: address, port: port) { url in
            do {
                return try await cameraData(from: url, origin: origin)
            } catch CameraReadOnlyRequestError.originMismatch {
                throw CameraCommandRequestError.originMismatch
            }
        }
    }

    private func requestCommand(
        url: URL,
        expectedCommandID: UInt16,
        fetch: @escaping CameraReadOnlyFetcher
    ) async throws -> CameraCommandOutcome {
        guard !commandInFlight else {
            throw CameraCommandRequestError.inFlight
        }
        let requestToken = sessionState.cameraSessionToken()
        commandInFlight = true
        defer { commandInFlight = false }

        let response: Data
        do {
            response = try await fetch(url)
        } catch let error as CancellationError {
            throw error
        } catch let error as URLError where error.code == .timedOut {
            return .timedOut
        } catch let error as CameraCommandRequestError {
            throw error
        } catch {
            return .failed
        }

        guard isCurrentCameraRequest(requestToken) else {
            throw CameraCommandRequestError.pathUnavailable
        }

        do {
            return try mobileParseNovatekCommandOutcome(
                response: response,
                expectedCommandId: expectedCommandID
            ).cameraOutcome
        } catch {
            return .unknown
        }
    }

    private func commandContext(
        address: String,
        port: UInt16
    ) throws -> MobileNovatekHttpOriginDto {
        let origin = try mobileValidateNovatekHttpOrigin(address: address, port: port)
        guard let sessionOrigin = sessionState.novatekSessionOrigin() else {
            throw CameraCommandRequestError.unsupported
        }
        guard sessionOrigin == origin else {
            throw CameraCommandRequestError.originMismatch
        }
        return origin
    }

    func apply(pathStatus: CameraLocalNetworkPathStatus, usesWiFi: Bool) {
        pathObservationGeneration &+= 1
        if readOnlyEvidence != nil,
           evidencePathObservationGeneration != pathObservationGeneration {
            clearReadOnlyEvidence()
        }
        let nextConnection = cameraConnectionPresentation(
            pathStatus: pathStatus,
            usesWiFi: usesWiFi,
            hasReadOnlyEvidence: readOnlyEvidence != nil
        )
        guard pathStatus == .satisfied, usesWiFi else {
            clearReadOnlyEvidence()
            presentation.connection = nextConnection
            return
        }
        presentation.connection = nextConnection
    }

    /// Records that Local Network permission has not yet been granted.
    public func observePermissionRequired() {
        presentation.connection = .permissionRequired
    }

    /// Marks a read-only probe as in progress for the camera route.
    public func observeDiscoveryStarted() {
        presentation.connection = .discovering
    }

    private func clearReadOnlyEvidence() {
        invalidateCameraLifecycle()
        readOnlyEvidence = nil
        presentation.connection = .notConfigured
        presentation.profileName = nil
        presentation.storage = .unknown
    }

    private func invalidateCameraLifecycle() {
        stopPreview()
        sessionState.invalidateCameraLifecycle()
        refreshCameraState()
    }

    private func isCurrentCameraRequest(_ token: MobileCameraSessionTokenDto) -> Bool {
        sessionState.cameraSessionTokenIsCurrent(token: token)
    }

    private func readOnlyOriginMatches(_ origin: MobileNovatekHttpOriginDto) -> Bool {
        guard let retainedOrigin = sessionState.novatekSessionOrigin() else { return false }
        return retainedOrigin == origin
    }

    private func requestURL(
        origin: MobileNovatekHttpOriginDto,
        command: MobileNovatekReadCommandDto
    ) throws -> URL {
        try requestURL(
            origin: origin,
            target: mobileNovatekReadCommandTarget(command: command)
        )
    }

    private func requestURL(
        origin: MobileNovatekHttpOriginDto,
        target: String
    ) throws -> URL {
        guard let url = URL(string: "http://\(origin.address):\(origin.port)\(target)") else {
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

private extension MobileNovatekCommandOutcomeDto {
    var cameraOutcome: CameraCommandOutcome {
        switch self {
        case .acknowledged: .acknowledged
        case .refused: .refused
        case .unknown: .unknown
        }
    }
}
