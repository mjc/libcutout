import CutoutMobile
import CutoutMobileFFI
import SwiftUI

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

struct CameraRouteContainerView: View {
    let close: (() -> Void)?
    let annotateCapture: ((String, String) -> Void)?
    let recordMediaReference: ((CameraSourceKind, CameraMediaEvidence, URL) -> Void)?
    @State private var adapter = CameraLocalNetworkAdapter()
    @State private var previewRenderer = CameraPreviewRenderer()
    @State private var address = "192.168.1.254"
    @State private var port = "80"
    @State private var isReading = false
    @State private var readErrorKey: String?
    @State private var downloadedMediaURL: URL?
    @State private var downloadingMediaPath: String?
    @State private var mediaDownloadTask: Task<Void, Never>?
    @State private var mediaErrorKey: String?
    @State private var thumbnailTask: Task<Void, Never>?
    @State private var thumbnailDataByPath: [String: Data] = [:]
    @State private var thumbnailPathInFlight: String?
    @State private var thumbnailErrorKey: String?
    @State private var recordingRequestKey: String?
    @State private var isRequestingRecording = false
    @State private var stillRequestKey: String?
    @State private var isRequestingStill = false

    init(
        close: (() -> Void)? = nil,
        annotateCapture: ((String, String) -> Void)? = nil,
        recordMediaReference: ((CameraSourceKind, CameraMediaEvidence, URL) -> Void)? = nil,
        sessionState: CutoutSessionStateHandle = CutoutSessionStateHandle()
    ) {
        self.close = close
        self.annotateCapture = annotateCapture
        self.recordMediaReference = recordMediaReference
        _adapter = State(initialValue: CameraLocalNetworkAdapter(sessionState: sessionState))
    }

    var body: some View {
        CameraRouteView(
            onClose: close,
            presentation: adapter.presentation,
            readOnlyEvidence: adapter.readOnlyEvidence,
            movieRTSPURI: adapter.readOnlyEvidence?.movieRTSPURI,
            address: $address,
            port: $port,
            isReading: isReading,
            readErrorKey: readErrorKey,
            downloadedMediaURL: downloadedMediaURL,
            downloadingMediaPath: downloadingMediaPath,
            mediaErrorKey: mediaErrorKey,
            cancelDownload: cancelMediaDownload,
            thumbnailDataByPath: thumbnailDataByPath,
            thumbnailPathInFlight: thumbnailPathInFlight,
            thumbnailErrorKey: thumbnailErrorKey,
            supportsMediaThumbnails: adapter.readOnlyEvidence?.supportsMediaThumbnails ?? false,
            fetchThumbnail: fetchThumbnail,
            recordingRequestKey: recordingRequestKey,
            isRequestingRecording: isRequestingRecording,
            supportsOnboardRecording: adapter.readOnlyEvidence?.supportsOnboardRecording ?? false,
            stillRequestKey: stillRequestKey,
            isRequestingStill: isRequestingStill,
            supportsStillCapture: adapter.readOnlyEvidence?.supportsStillCapture ?? false,
            savedFileURL: adapter.savedPreviewFileURL,
            previewRenderer: previewRenderer,
            loadEvidence: readCamera,
            downloadMedia: downloadMedia,
            requestRecording: requestRecording,
            requestStillCapture: requestStillCapture,
            startPreview: { startPreview(saveTo: nil) },
            savePreview: { startPreview(saveTo: previewOutputURL()) },
            stopPreview: adapter.stopPreview
        )
            .task {
                adapter.setPreviewFrameHandler { frame in
                    try await previewRenderer.enqueue(frame)
                }
                adapter.start()
            }
            .onChange(of: adapter.savedPreviewFileURL) { _, url in
                guard let url else { return }
                annotateCapture?("camera_preview_file", url.lastPathComponent)
            }
            .onDisappear {
                mediaDownloadTask?.cancel()
                thumbnailTask?.cancel()
                adapter.stop()
                adapter.setPreviewFrameHandler(nil)
                previewRenderer.reset()
            }
    }

    private func readCamera() {
        guard let portNumber = UInt16(port) else {
            readErrorKey = "camera.error.invalid_port"
            return
        }

        isReading = true
        readErrorKey = nil
        Task { @MainActor in
            defer { isReading = false }
            do {
                let evidence = try await adapter.loadReadOnlyEvidence(address: address, port: portNumber)
                annotateCapture?("camera_profile", "novatek_r3_pro")
                annotateCapture?("camera_firmware", evidence.firmwareVersion)
            } catch CameraReadOnlyRequestError.pathUnavailable {
                readErrorKey = "camera.connection.detail.wifi_required"
            } catch CameraReadOnlyRequestError.unsupportedProfile {
                readErrorKey = "camera.error.unsupported_profile"
            } catch {
                readErrorKey = "camera.error.read_failed"
            }
        }
    }

    private func startPreview(saveTo url: URL?) {
        guard let uri = adapter.readOnlyEvidence?.movieRTSPURI else { return }

        Task { @MainActor in
            do {
                if let url {
                    try await adapter.startPreview(
                        uri: uri,
                        expectedAddress: address,
                        saveTo: url
                    )
                } else {
                    try await adapter.startPreview(uri: uri, expectedAddress: address)
                }
            } catch CameraReadOnlyRequestError.originMismatch {
                readErrorKey = "camera.error.origin_mismatch"
            } catch {
                readErrorKey = "camera.error.read_failed"
            }
        }
    }

    private func downloadMedia(_ media: CameraMediaEvidence) {
        guard let portNumber = UInt16(port) else {
            mediaErrorKey = "camera.error.invalid_port"
            return
        }

        let destination = mediaOutputURL(for: media)
        mediaDownloadTask?.cancel()
        downloadingMediaPath = media.path
        mediaErrorKey = nil
        mediaDownloadTask = Task { @MainActor in
            defer {
                downloadingMediaPath = nil
                mediaDownloadTask = nil
            }
            do {
                try await adapter.downloadMedia(
                    address: address,
                    port: portNumber,
                    media: media,
                    to: destination
                )
                downloadedMediaURL = destination
                annotateCapture?("camera_media_file", media.name)
                recordMediaReference?(.novatekR3Pro, media, destination)
            } catch is CancellationError {
                // Cancellation is an expected user action, not a transfer error.
            } catch CameraMediaDownloadError.originMismatch {
                mediaErrorKey = "camera.error.origin_mismatch"
            } catch {
                mediaErrorKey = "camera.error.media_download_failed"
            }
        }
    }

    private func cancelMediaDownload() {
        mediaDownloadTask?.cancel()
        mediaDownloadTask = nil
        downloadingMediaPath = nil
    }

    private func fetchThumbnail(_ media: CameraMediaEvidence) {
        guard let portNumber = UInt16(port) else {
            thumbnailErrorKey = "camera.error.invalid_port"
            return
        }

        thumbnailTask?.cancel()
        thumbnailPathInFlight = media.path
        thumbnailErrorKey = nil
        thumbnailTask = Task { @MainActor in
            defer {
                thumbnailPathInFlight = nil
                thumbnailTask = nil
            }
            do {
                let data = try await adapter.fetchMediaThumbnail(
                    address: address,
                    port: portNumber,
                    media: media
                )
                guard !data.isEmpty else {
                    thumbnailErrorKey = "camera.error.thumbnail_empty"
                    return
                }
                thumbnailDataByPath[media.path] = data
            } catch is CancellationError {
                // Cancellation is an expected user action.
            } catch CameraReadOnlyRequestError.originMismatch {
                thumbnailErrorKey = "camera.error.origin_mismatch"
            } catch {
                thumbnailErrorKey = "camera.error.thumbnail_failed"
            }
        }
    }

    private func requestRecording(start: Bool) {
        guard let portNumber = UInt16(port) else {
            recordingRequestKey = "camera.error.invalid_port"
            return
        }

        isRequestingRecording = true
        recordingRequestKey = nil
        Task { @MainActor in
            defer { isRequestingRecording = false }
            do {
                let outcome = try await adapter.requestOnboardRecording(
                    address: address,
                    port: portNumber,
                    start: start
                )
                recordingRequestKey = cameraCommandOutcomeKey(
                    outcome,
                    acknowledged: start
                        ? "camera.recording.request_start_sent"
                        : "camera.recording.request_stop_sent"
                )
                annotateCapture?(
                    "camera_recording_request",
                    "\(start ? "start" : "stop"):\(outcome.annotationValue)"
                )
            } catch is CancellationError {
                // Cancellation is an expected lifecycle event.
            } catch CameraCommandRequestError.pathUnavailable {
                recordingRequestKey = "camera.connection.detail.wifi_required"
            } catch CameraCommandRequestError.unsupported {
                recordingRequestKey = "camera.recording.unavailable"
            } catch CameraCommandRequestError.inFlight {
                recordingRequestKey = "camera.command.busy"
            } catch CameraCommandRequestError.originMismatch {
                recordingRequestKey = "camera.error.origin_mismatch"
            } catch {
                recordingRequestKey = "camera.error.recording_request_failed"
            }
        }
    }

    private func requestStillCapture() {
        guard let portNumber = UInt16(port) else {
            stillRequestKey = "camera.error.invalid_port"
            return
        }

        isRequestingStill = true
        stillRequestKey = nil
        Task { @MainActor in
            defer { isRequestingStill = false }
            do {
                let outcome = try await adapter.requestStillCapture(
                    address: address,
                    port: portNumber
                )
                stillRequestKey = cameraCommandOutcomeKey(
                    outcome,
                    acknowledged: "camera.still.request_sent"
                )
                annotateCapture?("camera_still_request", outcome.annotationValue)
            } catch is CancellationError {
                // Cancellation is an expected lifecycle event.
            } catch CameraCommandRequestError.pathUnavailable {
                stillRequestKey = "camera.connection.detail.wifi_required"
            } catch CameraCommandRequestError.unsupported {
                stillRequestKey = "camera.still.unavailable"
            } catch CameraCommandRequestError.inFlight {
                stillRequestKey = "camera.command.busy"
            } catch CameraCommandRequestError.originMismatch {
                stillRequestKey = "camera.error.origin_mismatch"
            } catch {
                stillRequestKey = "camera.error.still_request_failed"
            }
        }
    }

    private func previewOutputURL() -> URL {
        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return directory.appendingPathComponent(
            "camera-preview-" + UUID().uuidString + ".h264"
        )
    }

    private func mediaOutputURL(for media: CameraMediaEvidence) -> URL {
        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return directory.appendingPathComponent(
            "camera-media-" + UUID().uuidString + "-" + cameraMediaLocalFileComponent(media.name)
        )
    }
}

func cameraMediaLocalFileComponent(_ name: String) -> String {
    let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_."))
    let sanitized = name.unicodeScalars
        .map { allowed.contains($0) ? String($0) : "_" }
        .joined()
        .trimmingCharacters(in: .whitespacesAndNewlines)
    return sanitized.isEmpty || sanitized == "." || sanitized == ".." ? "media" : sanitized
}

private func cameraCommandOutcomeKey(
    _ outcome: CameraCommandOutcome,
    acknowledged: String
) -> String {
    switch outcome {
    case .acknowledged:
        acknowledged
    case .refused:
        "camera.command.refused"
    case .timedOut:
        "camera.command.timed_out"
    case .unknown:
        "camera.command.unknown"
    case .failed:
        "camera.command.failed"
    }
}

private extension CameraCommandOutcome {
    var annotationValue: String {
        switch self {
        case .acknowledged: "acknowledged"
        case .refused: "refused"
        case .timedOut: "timed_out"
        case .unknown: "unknown"
        case .failed: "failed"
        }
    }
}

struct CameraRouteView: View {
    let onClose: (() -> Void)?
    let presentation: CameraPresentation
    let readOnlyEvidence: CameraReadOnlyEvidence?
    let movieRTSPURI: String?
    let hasPreviewSource: Bool
    @Binding var address: String
    @Binding var port: String
    let isReading: Bool
    let readErrorKey: String?
    let downloadedMediaURL: URL?
    let downloadingMediaPath: String?
    let mediaErrorKey: String?
    let cancelDownload: (() -> Void)?
    let thumbnailDataByPath: [String: Data]
    let thumbnailPathInFlight: String?
    let thumbnailErrorKey: String?
    let supportsMediaThumbnails: Bool
    let fetchThumbnail: ((CameraMediaEvidence) -> Void)?
    let recordingRequestKey: String?
    let isRequestingRecording: Bool
    let supportsOnboardRecording: Bool
    let stillRequestKey: String?
    let isRequestingStill: Bool
    let supportsStillCapture: Bool
    let savedFileURL: URL?
    let previewRenderer: CameraPreviewRenderer?
    let loadEvidence: (() -> Void)?
    let downloadMedia: ((CameraMediaEvidence) -> Void)?
    let requestRecording: ((Bool) -> Void)?
    let requestStillCapture: (() -> Void)?
    let startPreview: (() -> Void)?
    let savePreview: (() -> Void)?
    let stopPreview: (() -> Void)?

    init(
        onClose: (() -> Void)? = nil,
        presentation: CameraPresentation = .initial,
        readOnlyEvidence: CameraReadOnlyEvidence? = nil,
        movieRTSPURI: String? = nil,
        address: Binding<String> = .constant("192.168.1.254"),
        port: Binding<String> = .constant("80"),
        isReading: Bool = false,
        readErrorKey: String? = nil,
        downloadedMediaURL: URL? = nil,
        downloadingMediaPath: String? = nil,
        mediaErrorKey: String? = nil,
        cancelDownload: (() -> Void)? = nil,
        thumbnailDataByPath: [String: Data] = [:],
        thumbnailPathInFlight: String? = nil,
        thumbnailErrorKey: String? = nil,
        supportsMediaThumbnails: Bool = false,
        fetchThumbnail: ((CameraMediaEvidence) -> Void)? = nil,
        recordingRequestKey: String? = nil,
        isRequestingRecording: Bool = false,
        supportsOnboardRecording: Bool = false,
        stillRequestKey: String? = nil,
        isRequestingStill: Bool = false,
        supportsStillCapture: Bool = false,
        savedFileURL: URL? = nil,
        previewRenderer: CameraPreviewRenderer? = nil,
        loadEvidence: (() -> Void)? = nil,
        downloadMedia: ((CameraMediaEvidence) -> Void)? = nil,
        requestRecording: ((Bool) -> Void)? = nil,
        requestStillCapture: (() -> Void)? = nil,
        startPreview: (() -> Void)? = nil,
        savePreview: (() -> Void)? = nil,
        stopPreview: (() -> Void)? = nil
    ) {
        self.onClose = onClose
        self.presentation = presentation
        self.readOnlyEvidence = readOnlyEvidence
        self.movieRTSPURI = movieRTSPURI
        self.hasPreviewSource = movieRTSPURI != nil
        self._address = address
        self._port = port
        self.isReading = isReading
        self.readErrorKey = readErrorKey
        self.downloadedMediaURL = downloadedMediaURL
        self.downloadingMediaPath = downloadingMediaPath
        self.mediaErrorKey = mediaErrorKey
        self.cancelDownload = cancelDownload
        self.thumbnailDataByPath = thumbnailDataByPath
        self.thumbnailPathInFlight = thumbnailPathInFlight
        self.thumbnailErrorKey = thumbnailErrorKey
        self.supportsMediaThumbnails = supportsMediaThumbnails
        self.fetchThumbnail = fetchThumbnail
        self.recordingRequestKey = recordingRequestKey
        self.isRequestingRecording = isRequestingRecording
        self.supportsOnboardRecording = supportsOnboardRecording
        self.stillRequestKey = stillRequestKey
        self.isRequestingStill = isRequestingStill
        self.supportsStillCapture = supportsStillCapture
        self.savedFileURL = savedFileURL
        self.previewRenderer = previewRenderer
        self.loadEvidence = loadEvidence
        self.downloadMedia = downloadMedia
        self.requestRecording = requestRecording
        self.requestStillCapture = requestStillCapture
        self.startPreview = startPreview
        self.savePreview = savePreview
        self.stopPreview = stopPreview
    }

    var body: some View {
        PevDashboardScaffold(
            sectionTitle: localizedAppText("navigation.section.camera"),
            bottomPadding: 32,
            horizontalPadding: 20
        ) {
            if let onClose {
                Button(action: onClose) {
                    Label(localizedAppText("camera.action.back"), systemImage: "chevron.left")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("camera.back")
            }

            if #available(iOS 26, macOS 26, *) {
                GlassEffectContainer(spacing: 16) {
                    cameraCards
                }
            } else {
                cameraCards
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("camera.screen")
    }

    @ViewBuilder
    private var cameraCards: some View {
        CameraStatusCard(
            presentation: presentation,
            readOnlyEvidence: readOnlyEvidence,
            movieRTSPURI: movieRTSPURI,
            address: $address,
            port: $port,
            isReading: isReading,
            readErrorKey: readErrorKey,
            downloadedMediaURL: downloadedMediaURL,
            downloadingMediaPath: downloadingMediaPath,
            mediaErrorKey: mediaErrorKey,
            cancelDownload: cancelDownload,
            thumbnailDataByPath: thumbnailDataByPath,
            thumbnailPathInFlight: thumbnailPathInFlight,
            thumbnailErrorKey: thumbnailErrorKey,
            supportsMediaThumbnails: supportsMediaThumbnails,
            fetchThumbnail: fetchThumbnail,
            loadEvidence: loadEvidence,
            downloadMedia: downloadMedia
        )
        CameraTruthCard(
            presentation: presentation,
            readOnlyEvidence: readOnlyEvidence,
            savedFileURL: savedFileURL,
            previewRenderer: previewRenderer,
            hasPreviewSource: hasPreviewSource,
            recordingRequestKey: recordingRequestKey,
            isRequestingRecording: isRequestingRecording,
            supportsOnboardRecording: supportsOnboardRecording,
            stillRequestKey: stillRequestKey,
            isRequestingStill: isRequestingStill,
            supportsStillCapture: supportsStillCapture,
            requestRecording: requestRecording,
            requestStillCapture: requestStillCapture,
            startPreview: startPreview,
            savePreview: savePreview,
            stopPreview: stopPreview
        )
    }
}

private struct CameraStatusCard: View {
    let presentation: CameraPresentation
    let readOnlyEvidence: CameraReadOnlyEvidence?
    let movieRTSPURI: String?
    @Binding var address: String
    @Binding var port: String
    let isReading: Bool
    let readErrorKey: String?
    let downloadedMediaURL: URL?
    let downloadingMediaPath: String?
    let mediaErrorKey: String?
    let cancelDownload: (() -> Void)?
    let thumbnailDataByPath: [String: Data]
    let thumbnailPathInFlight: String?
    let thumbnailErrorKey: String?
    let supportsMediaThumbnails: Bool
    let fetchThumbnail: ((CameraMediaEvidence) -> Void)?
    let loadEvidence: (() -> Void)?
    let downloadMedia: ((CameraMediaEvidence) -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(localizedAppText("camera.setup.title"), systemImage: "video.fill")
                .font(.title3.weight(.bold))

            Text(connectionTitle)
                .font(.headline)

            Text(connectionDetail)
                .font(.body)
                .foregroundStyle(PevColors.muted)

            if let profileName = presentation.profileName {
                Label(profileName, systemImage: "checkmark.seal")
                    .font(.subheadline.weight(.semibold))
            } else {
                Label(localizedAppText("camera.profile.pending"), systemImage: "questionmark.diamond")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
            }

            if let readOnlyEvidence {
                Divider()
                CameraTruthRow(
                    title: localizedAppText("camera.evidence.firmware"),
                    value: readOnlyEvidence.firmwareVersion,
                    systemImage: "cpu"
                )
                CameraTruthRow(
                    title: localizedAppText("camera.evidence.media_count"),
                    value: String(readOnlyEvidence.mediaCount),
                    systemImage: "film.stack"
                )
                if !readOnlyEvidence.media.isEmpty {
                    Divider()
                    Text(localizedAppText("camera.evidence.media_title"))
                        .font(.subheadline.weight(.semibold))
                    ForEach(readOnlyEvidence.media, id: \.path) { media in
                        HStack(alignment: .top, spacing: 10) {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(media.name)
                                    .font(.subheadline.monospaced())
                                    .lineLimit(1)
                                Text(media.time)
                                    .font(.caption)
                                    .foregroundStyle(PevColors.muted)
                            }
                            Spacer(minLength: 8)
                            if downloadingMediaPath == media.path {
                                HStack(spacing: 8) {
                                    ProgressView()
                                        .controlSize(.small)
                                    if let cancelDownload {
                                        Button(action: cancelDownload) {
                                            Label(
                                                localizedAppText("camera.evidence.media_cancel"),
                                                systemImage: "xmark.circle"
                                            )
                                        }
                                        .buttonStyle(.bordered)
                                        .accessibilityIdentifier("camera.media.cancel.\(media.name)")
                                    }
                                }
                            } else if let downloadMedia {
                                Button(action: { downloadMedia(media) }) {
                                    Label(
                                        localizedAppText("camera.evidence.media_download"),
                                        systemImage: "arrow.down.circle"
                                    )
                                }
                                .buttonStyle(.bordered)
                                .accessibilityIdentifier("camera.media.download.\(media.name)")
                            }
                        }
                        if let downloadedMediaURL,
                           downloadedMediaURL.lastPathComponent.hasSuffix("-" + media.name) {
                            HStack(spacing: 12) {
                                Text(localizedAppText("camera.evidence.media_saved"))
                                    .font(.caption)
                                    .foregroundStyle(PevColors.muted)
                                ShareLink(item: downloadedMediaURL) {
                                    Label(
                                        localizedAppText("camera.evidence.media_export"),
                                        systemImage: "square.and.arrow.up"
                                    )
                                }
                                .buttonStyle(.bordered)
                                .accessibilityIdentifier("camera.media.export.\(media.name)")
                            }
                        }
                        if supportsMediaThumbnails, let fetchThumbnail {
                            HStack(spacing: 10) {
                                if let data = thumbnailDataByPath[media.path] {
                                    CameraThumbnailView(data: data)
                                        .accessibilityLabel(localizedAppText("camera.evidence.thumbnail"))
                                }
                                if thumbnailPathInFlight == media.path {
                                    ProgressView()
                                        .controlSize(.small)
                                } else {
                                    Button(action: { fetchThumbnail(media) }) {
                                        Label(
                                            localizedAppText("camera.evidence.thumbnail_request"),
                                            systemImage: "photo"
                                        )
                                    }
                                    .buttonStyle(.bordered)
                                    .disabled(thumbnailPathInFlight != nil)
                                    .accessibilityIdentifier("camera.media.thumbnail.\(media.name)")
                                }
                            }
                        }
                    }
                    Text(localizedAppText("camera.evidence.media_local_only"))
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)
                } else {
                    Text(localizedAppText("camera.evidence.metadata_only"))
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)
                }
                if let mediaErrorKey {
                    Text(localizedAppText(mediaErrorKey))
                        .font(.footnote)
                        .foregroundStyle(.red)
                }
                if let thumbnailErrorKey {
                    Text(localizedAppText(thumbnailErrorKey))
                        .font(.footnote)
                        .foregroundStyle(.red)
                }
            }

            if let loadEvidence {
                Divider()
                TextField(localizedAppText("camera.origin.address"), text: $address)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("camera.origin.address")
                TextField(localizedAppText("camera.origin.port"), text: $port)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("camera.origin.port")
                Button(action: loadEvidence) {
                    Label(
                        localizedAppText(isReading ? "camera.origin.reading" : "camera.origin.read"),
                        systemImage: isReading ? "arrow.triangle.2.circlepath" : "checkmark.shield"
                    )
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    isReading
                        || address.isEmpty
                        || port.isEmpty
                        || presentation.connection.blocksReadOnlyDiscovery
                )
                .accessibilityIdentifier("camera.origin.read")

                if let readErrorKey {
                    Text(localizedAppText(readErrorKey))
                        .font(.footnote)
                        .foregroundStyle(.red)
                }
            }

            if movieRTSPURI != nil {
                Label(localizedAppText("camera.preview.source_ready"), systemImage: "checkmark.circle")
                    .font(.footnote.weight(.semibold))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
        .cameraSurface(tint: PevColors.cyan)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("camera.setup.card")
    }

    private var connectionTitle: String {
        switch presentation.connection {
        case .notConfigured: localizedAppText("camera.connection.not_configured")
        case .permissionRequired: localizedAppText("camera.connection.permission_required")
        case .wifiRequired: localizedAppText("camera.connection.wifi_required")
        case .discovering: localizedAppText("camera.connection.discovering")
        case .connected: localizedAppText("camera.connection.connected")
        case .unsupported: localizedAppText("camera.connection.unsupported")
        }
    }

    private var connectionDetail: String {
        switch presentation.connection {
        case .notConfigured:
            localizedAppText("camera.connection.detail.not_configured")
        case .permissionRequired:
            localizedAppText("camera.connection.detail.permission_required")
        case .wifiRequired:
            localizedAppText("camera.connection.detail.wifi_required")
        case .discovering:
            localizedAppText("camera.connection.detail.discovering")
        case .connected:
            localizedAppText("camera.connection.detail.connected")
        case .unsupported:
            localizedAppText("camera.connection.detail.unsupported")
        }
    }
}

private struct CameraThumbnailView: View {
    let data: Data

    var body: some View {
#if canImport(UIKit)
        if let image = UIImage(data: data) {
            Image(uiImage: image)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: 160, maxHeight: 100)
                .clipShape(RoundedRectangle(cornerRadius: 8))
        } else {
            Image(systemName: "photo")
        }
#elseif canImport(AppKit)
        if let image = NSImage(data: data) {
            Image(nsImage: image)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: 160, maxHeight: 100)
                .clipShape(RoundedRectangle(cornerRadius: 8))
        } else {
            Image(systemName: "photo")
        }
#else
        Image(systemName: "photo")
#endif
    }
}

private struct CameraTruthCard: View {
    let presentation: CameraPresentation
    let readOnlyEvidence: CameraReadOnlyEvidence?
    let savedFileURL: URL?
    let previewRenderer: CameraPreviewRenderer?
    let hasPreviewSource: Bool
    let recordingRequestKey: String?
    let isRequestingRecording: Bool
    let supportsOnboardRecording: Bool
    let stillRequestKey: String?
    let isRequestingStill: Bool
    let supportsStillCapture: Bool
    let requestRecording: ((Bool) -> Void)?
    let requestStillCapture: (() -> Void)?
    let startPreview: (() -> Void)?
    let savePreview: (() -> Void)?
    let stopPreview: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(localizedAppText("camera.truth.title"))
                .font(.headline)

            CameraTruthRow(
                title: localizedAppText("camera.truth.preview"),
                value: previewText,
                systemImage: "play.rectangle"
            )
            CameraTruthRow(
                title: localizedAppText("camera.truth.recording"),
                value: recordingText,
                systemImage: "record.circle"
            )
            CameraTruthRow(
                title: localizedAppText("camera.truth.storage"),
                value: storageText,
                systemImage: "sdcard"
            )

            Text(localizedAppText("camera.privacy.local_only"))
                .font(.footnote)
                .foregroundStyle(PevColors.muted)

            if readOnlyEvidence != nil {
                Label(localizedAppText("camera.connection.bluetooth_warning"), systemImage: "exclamationmark.triangle")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.orange)
                    .accessibilityElement(children: .combine)
            }

            if let requestRecording, presentation.connection == .connected {
                Divider()
                if supportsOnboardRecording {
                    HStack {
                        Button(action: { requestRecording(true) }) {
                            Label(
                                localizedAppText(isRequestingRecording ? "camera.recording.requesting" : "camera.recording.request_start"),
                                systemImage: isRequestingRecording ? "arrow.triangle.2.circlepath" : "record.circle"
                            )
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(isRequestingRecording || isRequestingStill)
                        .accessibilityIdentifier("camera.recording.request-start")
                        Button(action: { requestRecording(false) }) {
                            Label(localizedAppText("camera.recording.request_stop"), systemImage: "stop.circle")
                        }
                        .buttonStyle(.bordered)
                        .disabled(isRequestingRecording || isRequestingStill)
                        .accessibilityIdentifier("camera.recording.request-stop")
                    }
                    Text(localizedAppText("camera.recording.request_detail"))
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)
                    if let recordingRequestKey {
                        Text(localizedAppText(recordingRequestKey))
                            .font(.footnote)
                            .foregroundStyle(PevColors.muted)
                    }
                } else {
                    Text(localizedAppText("camera.recording.unavailable"))
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)
                }
                if let requestStillCapture {
                    if supportsStillCapture {
                        Button(action: requestStillCapture) {
                            Label(
                                localizedAppText(isRequestingStill ? "camera.still.requesting" : "camera.still.request"),
                                systemImage: isRequestingStill ? "arrow.triangle.2.circlepath" : "camera.shutter.button"
                            )
                        }
                        .buttonStyle(.bordered)
                        .disabled(isRequestingStill || isRequestingRecording)
                        .accessibilityIdentifier("camera.still.request")
                        if let stillRequestKey {
                            Text(localizedAppText(stillRequestKey))
                                .font(.footnote)
                                .foregroundStyle(PevColors.muted)
                        }
                    } else {
                        Text(localizedAppText("camera.still.unavailable"))
                            .font(.footnote)
                            .foregroundStyle(PevColors.muted)
                    }
                }
            }

            if let previewRenderer, showsPreviewSurface {
                CameraPreviewSurface(renderer: previewRenderer)
                    .accessibilityIdentifier("camera.preview.surface")
            }

            if hasPreviewSource, let startPreview, let savePreview {
                Divider()
                HStack {
                    if presentation.preview == .stopped || presentation.preview == .interrupted {
                        Button(action: startPreview) {
                            Label(localizedAppText("camera.preview.start"), systemImage: "play.fill")
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("camera.preview.start")
                        Button(action: savePreview) {
                            Label(localizedAppText("camera.preview.save"), systemImage: "arrow.down.to.line")
                        }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("camera.preview.save")
                    } else if let stopPreview {
                        Button(action: stopPreview) {
                            Label(localizedAppText("camera.preview.stop"), systemImage: "stop.fill")
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("camera.preview.stop")
                    }
                }

                Text(localizedAppText("camera.preview.saved_detail"))
                    .font(.footnote)
                    .foregroundStyle(PevColors.muted)

                if let savedFileURL {
                    HStack(spacing: 12) {
                        Text(savedFileURL.lastPathComponent)
                            .font(.footnote.monospaced())
                            .foregroundStyle(PevColors.muted)
                            .accessibilityIdentifier("camera.preview.saved-file")
                        ShareLink(item: savedFileURL) {
                            Label(
                                localizedAppText("camera.preview.export"),
                                systemImage: "square.and.arrow.up"
                            )
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("camera.preview.export")
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
        .cameraSurface(tint: PevColors.purple)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("camera.truth.card")
    }

    private var previewText: String {
        switch presentation.preview {
        case .stopped: localizedAppText("camera.preview.stopped")
        case .buffering: localizedAppText("camera.preview.buffering")
        case .live: localizedAppText("camera.preview.live")
        case .stale: localizedAppText("camera.preview.stale")
        case .interrupted: localizedAppText("camera.preview.interrupted")
        case .unavailable: localizedAppText("camera.preview.unavailable")
        }
    }

    private var recordingText: String {
        switch presentation.recording {
        case .unknown: localizedAppText("camera.recording.unknown")
        case .stopped: localizedAppText("camera.recording.stopped")
        case .recording: localizedAppText("camera.recording.active")
        }
    }

    private var storageText: String {
        switch presentation.storage {
        case .unknown: localizedAppText("camera.storage.unknown")
        case .present: localizedAppText("camera.storage.present")
        case .missing: localizedAppText("camera.storage.missing")
        case .error: localizedAppText("camera.storage.error")
        }
    }

    private var showsPreviewSurface: Bool {
        switch presentation.preview {
        case .stopped, .unavailable:
            false
        case .buffering, .live, .stale, .interrupted:
            true
        }
    }
}

private struct CameraTruthRow: View {
    let title: String
    let value: String
    let systemImage: String

    var body: some View {
        Label {
            HStack {
                Text(title)
                Spacer(minLength: 12)
                Text(value)
                    .foregroundStyle(PevColors.muted)
            }
        } icon: {
            Image(systemName: systemImage)
                .accessibilityHidden(true)
        }
        .font(.subheadline)
        .accessibilityElement(children: .combine)
    }
}

private extension View {
    @ViewBuilder
    func cameraSurface(tint: Color) -> some View {
        if #available(iOS 26, macOS 26, *) {
            self.glassEffect(.regular.tint(tint.opacity(0.18)), in: .rect(cornerRadius: 24))
        } else {
            self.background(PevDashboardCardBackground(cornerRadius: 24))
        }
    }
}

#Preview("Camera setup") {
    CameraRouteView()
}
