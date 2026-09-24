import CutoutMobile
import CutoutMobileFFI
import ImageIO
import SwiftUI

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

struct CameraRouteContainerView: View {
    let close: (() -> Void)?
    let annotateCapture: ((String, String) -> Void)?
    let recordMediaReference: ((String, CameraSourceKind, CameraMediaEvidence, URL) -> Void)?
    let currentCaptureFileName: (() -> String?)?
    @State private var adapter = CameraLocalNetworkAdapter()
    @State private var previewRenderer = CameraPreviewRenderer()
    @State private var mediaModel: CameraMediaModel
    @State private var discoveryModel: CameraDiscoveryModel
    @State private var address = "192.168.1.254"
    @State private var port = "80"
    @State private var previewErrorKey: String?
    @State private var recordingRequestKey: String?
    @State private var isRequestingRecording = false
    @State private var stillRequestKey: String?
    @State private var isRequestingStill = false
    @State private var previewStartTask: Task<Void, Never>?
    @State private var recordingTask: Task<Void, Never>?
    @State private var stillTask: Task<Void, Never>?
    @State private var recordingGeneration: UInt64 = 0
    @State private var stillGeneration: UInt64 = 0
    @Environment(\.scenePhase) private var scenePhase

    init(
        close: (() -> Void)? = nil,
        annotateCapture: ((String, String) -> Void)? = nil,
        recordMediaReference: ((String, CameraSourceKind, CameraMediaEvidence, URL) -> Void)? = nil,
        currentCaptureFileName: (() -> String?)? = nil,
        sessionState: CutoutSessionStateHandle = CutoutSessionStateHandle()
    ) {
        self.close = close
        self.annotateCapture = annotateCapture
        self.recordMediaReference = recordMediaReference
        self.currentCaptureFileName = currentCaptureFileName
        let adapter = CameraLocalNetworkAdapter(sessionState: sessionState)
        _adapter = State(initialValue: adapter)
        let mediaModel = CameraMediaModel(
            adapter: adapter,
            annotateCapture: annotateCapture,
            recordMediaReference: recordMediaReference,
            currentCaptureFileName: currentCaptureFileName
        )
        _mediaModel = State(initialValue: mediaModel)
        _discoveryModel = State(
            initialValue: CameraDiscoveryModel(
                adapter: adapter,
                prepareForEvidenceRefresh: mediaModel.prepareForEvidenceRefresh,
                annotateCapture: annotateCapture
            )
        )
    }

    var body: some View {
        CameraRouteView(
            onClose: close,
            presentation: adapter.presentation,
            readOnlyEvidence: adapter.readOnlyEvidence,
            movieRTSPURI: adapter.readOnlyEvidence?.movieRTSPURI,
            mediaModel: mediaModel,
            address: $address,
            port: $port,
            isReading: discoveryModel.isReading,
            readErrorKey: discoveryModel.readErrorKey ?? previewErrorKey,
            recordingRequestKey: recordingRequestKey,
            isRequestingRecording: isRequestingRecording,
            supportsOnboardRecording: adapter.readOnlyEvidence?.supportsOnboardRecording ?? false,
            stillRequestKey: stillRequestKey,
            isRequestingStill: isRequestingStill,
            supportsStillCapture: adapter.readOnlyEvidence?.supportsStillCapture ?? false,
            savedFileURL: adapter.savedPreviewFileURL,
            previewRenderer: previewRenderer,
            loadEvidence: {
                previewErrorKey = nil
                _ = discoveryModel.load(address: address, port: port)
            },
            requestRecording: requestRecording,
            requestStillCapture: requestStillCapture,
            startPreview: { startPreview(saveTo: nil) },
            savePreview: { startPreview(saveTo: previewOutputURL()) },
            stopPreview: adapter.stopPreview
        )
            .task {
                adapter.setPreviewConfigurationHandler { configuration in
                    try previewRenderer.configure(configuration)
                }
                adapter.setPreviewFrameHandler { frame in
                    do {
                        try await previewRenderer.enqueue(frame)
                        return true
                    } catch CameraPreviewRendererError.missingParameterSets {
                        // The RTSP stream may begin with inter frames. Keep
                        // buffering until a random-access frame carries the
                        // codec configuration instead of killing the session.
                        return false
                    }
                }
                adapter.start()
            }
            .onChange(of: adapter.savedPreviewFileURL) { _, url in
                guard let url else { return }
                annotateCapture?("camera_preview_file", url.lastPathComponent)
            }
            .onDisappear {
                stopCameraWork()
            }
            .onChange(of: scenePhase) { _, phase in
                if phase == .background {
                    stopCameraWork()
                } else if phase == .active {
                    adapter.start()
                }
            }
    }

    private func startPreview(saveTo url: URL?) {
        guard let uri = adapter.readOnlyEvidence?.movieRTSPURI else { return }

        discoveryModel.clearError()
        previewErrorKey = nil
        previewRenderer.reset()
        previewStartTask?.cancel()
        previewStartTask = Task { @MainActor in
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
                previewErrorKey = "camera.error.origin_mismatch"
            } catch {
                if !Task.isCancelled { previewErrorKey = "camera.error.read_failed" }
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
        recordingTask?.cancel()
        recordingGeneration &+= 1
        let generation = recordingGeneration
        recordingTask = Task { @MainActor in
            defer {
                if generation == recordingGeneration {
                    isRequestingRecording = false
                    recordingTask = nil
                }
            }
            do {
                let outcome = try await adapter.requestOnboardRecording(
                    address: address,
                    port: portNumber,
                    start: start
                )
                guard generation == recordingGeneration, !Task.isCancelled else { return }
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
                if generation == recordingGeneration { recordingRequestKey = "camera.connection.detail.wifi_required" }
            } catch CameraCommandRequestError.unsupported {
                if generation == recordingGeneration { recordingRequestKey = "camera.recording.unavailable" }
            } catch CameraCommandRequestError.inFlight {
                if generation == recordingGeneration { recordingRequestKey = "camera.command.busy" }
            } catch CameraCommandRequestError.originMismatch {
                if generation == recordingGeneration { recordingRequestKey = "camera.error.origin_mismatch" }
            } catch {
                if generation == recordingGeneration, !Task.isCancelled { recordingRequestKey = "camera.error.recording_request_failed" }
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
        stillTask?.cancel()
        stillGeneration &+= 1
        let generation = stillGeneration
        stillTask = Task { @MainActor in
            defer {
                if generation == stillGeneration {
                    isRequestingStill = false
                    stillTask = nil
                }
            }
            do {
                let outcome = try await adapter.requestStillCapture(
                    address: address,
                    port: portNumber
                )
                guard generation == stillGeneration, !Task.isCancelled else { return }
                stillRequestKey = cameraCommandOutcomeKey(
                    outcome,
                    acknowledged: "camera.still.request_sent"
                )
                annotateCapture?("camera_still_request", outcome.annotationValue)
            } catch is CancellationError {
                // Cancellation is an expected lifecycle event.
            } catch CameraCommandRequestError.pathUnavailable {
                if generation == stillGeneration { stillRequestKey = "camera.connection.detail.wifi_required" }
            } catch CameraCommandRequestError.unsupported {
                if generation == stillGeneration { stillRequestKey = "camera.still.unavailable" }
            } catch CameraCommandRequestError.inFlight {
                if generation == stillGeneration { stillRequestKey = "camera.command.busy" }
            } catch CameraCommandRequestError.originMismatch {
                if generation == stillGeneration { stillRequestKey = "camera.error.origin_mismatch" }
            } catch {
                if generation == stillGeneration, !Task.isCancelled { stillRequestKey = "camera.error.still_request_failed" }
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

    private func stopCameraWork() {
        recordingGeneration &+= 1
        stillGeneration &+= 1
        discoveryModel.cancel()
        previewStartTask?.cancel()
        recordingTask?.cancel()
        stillTask?.cancel()
        previewStartTask = nil
        recordingTask = nil
        stillTask = nil
        isRequestingRecording = false
        isRequestingStill = false
        mediaModel.cancelOutstandingWork()
        adapter.stop()
        previewRenderer.reset()
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
    let mediaModel: CameraMediaModel
    let hasPreviewSource: Bool
    @Binding var address: String
    @Binding var port: String
    let isReading: Bool
    let readErrorKey: String?
    let recordingRequestKey: String?
    let isRequestingRecording: Bool
    let supportsOnboardRecording: Bool
    let stillRequestKey: String?
    let isRequestingStill: Bool
    let supportsStillCapture: Bool
    let savedFileURL: URL?
    let previewRenderer: CameraPreviewRenderer?
    let loadEvidence: (() -> Void)?
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
        mediaModel: CameraMediaModel = CameraMediaModel(),
        address: Binding<String> = .constant("192.168.1.254"),
        port: Binding<String> = .constant("80"),
        isReading: Bool = false,
        readErrorKey: String? = nil,
        recordingRequestKey: String? = nil,
        isRequestingRecording: Bool = false,
        supportsOnboardRecording: Bool = false,
        stillRequestKey: String? = nil,
        isRequestingStill: Bool = false,
        supportsStillCapture: Bool = false,
        savedFileURL: URL? = nil,
        previewRenderer: CameraPreviewRenderer? = nil,
        loadEvidence: (() -> Void)? = nil,
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
        self.mediaModel = mediaModel
        self.hasPreviewSource = movieRTSPURI != nil
        self._address = address
        self._port = port
        self.isReading = isReading
        self.readErrorKey = readErrorKey
        self.recordingRequestKey = recordingRequestKey
        self.isRequestingRecording = isRequestingRecording
        self.supportsOnboardRecording = supportsOnboardRecording
        self.stillRequestKey = stillRequestKey
        self.isRequestingStill = isRequestingStill
        self.supportsStillCapture = supportsStillCapture
        self.savedFileURL = savedFileURL
        self.previewRenderer = previewRenderer
        self.loadEvidence = loadEvidence
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
            mediaModel: mediaModel,
            address: $address,
            port: $port,
            isReading: isReading,
            readErrorKey: readErrorKey,
            loadEvidence: loadEvidence
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
    let mediaModel: CameraMediaModel
    @Binding var address: String
    @Binding var port: String
    let isReading: Bool
    let readErrorKey: String?
    let loadEvidence: (() -> Void)?

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
                CameraMediaEvidenceList(
                    media: readOnlyEvidence.media,
                    address: address,
                    port: port,
                    supportsMediaThumbnails: readOnlyEvidence.supportsMediaThumbnails,
                    model: mediaModel
                )
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
        .accessibilityElement(children: .contain)
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

private struct CameraMediaEvidenceList: View {
    private static let pageSize = 100
    @State private var page = 1

    let media: [CameraMediaEvidence]
    let address: String
    let port: String
    let supportsMediaThumbnails: Bool
    let model: CameraMediaModel

    var body: some View {
        Group {
            if media.isEmpty {
                Text(localizedAppText("camera.evidence.metadata_only"))
                    .font(.footnote)
                    .foregroundStyle(PevColors.muted)
            } else {
                Divider()
                Text(localizedAppText("camera.evidence.media_title"))
                    .font(.subheadline.weight(.semibold))
                LazyVStack(alignment: .leading, spacing: 12) {
                    ForEach(media.prefix(page * Self.pageSize)) { media in
                        mediaRow(media)
                    }
                    if page * Self.pageSize < media.count {
                        Button {
                            page += 1
                        } label: {
                            Label(
                                localizedAppText("camera.evidence.media_load_more"),
                                systemImage: "ellipsis"
                            )
                        }
                        .buttonStyle(.bordered)
                    }
                }
                Text(localizedAppText("camera.evidence.media_local_only"))
                    .font(.footnote)
                    .foregroundStyle(PevColors.muted)
            }

            if let mediaErrorKey = model.mediaErrorKey {
                Text(localizedAppText(mediaErrorKey))
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
            if let thumbnailErrorKey = model.thumbnailErrorKey {
                Text(localizedAppText(thumbnailErrorKey))
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
        }
        .onChange(of: media) { _, _ in
            page = 1
        }
    }

    private func mediaRow(_ media: CameraMediaEvidence) -> some View {
        VStack(alignment: .leading, spacing: 8) {
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
                if model.downloadingMediaPath == media.path {
                    HStack(spacing: 8) {
                        ProgressView()
                            .controlSize(.small)
                        Button(action: model.cancelMediaDownload) {
                            Label(
                                localizedAppText("camera.evidence.media_cancel"),
                                systemImage: "xmark.circle"
                            )
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("camera.media.cancel.\(media.name)")
                    }
                } else {
                    Button {
                        model.downloadMedia(media, address: address, port: port)
                    } label: {
                        Label(
                            localizedAppText("camera.evidence.media_download"),
                            systemImage: "arrow.down.circle"
                        )
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("camera.media.download.\(media.name)")
                }
            }
            if let downloadedMediaURL = model.downloadedMediaURL,
               model.downloadedMediaPath == media.path {
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
            if supportsMediaThumbnails {
                HStack(spacing: 10) {
                    if let data = model.thumbnailDataByPath[media.path] {
                        CameraThumbnailView(data: data)
                            .accessibilityLabel(localizedAppText("camera.evidence.thumbnail"))
                    }
                    if model.thumbnailPathInFlight == media.path {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Button {
                            model.fetchThumbnail(media, address: address, port: port)
                        } label: {
                            Label(
                                localizedAppText("camera.evidence.thumbnail_request"),
                                systemImage: "photo"
                            )
                        }
                        .buttonStyle(.bordered)
                        .disabled(model.thumbnailPathInFlight != nil)
                        .accessibilityIdentifier("camera.media.thumbnail.\(media.name)")
                    }
                }
            }
        }
    }
}

private struct CameraThumbnailView: View {
    let data: Data

    var body: some View {
        if let image = cameraThumbnailImage(data: data) {
            Image(decorative: image, scale: 1, orientation: .up)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: 160, maxHeight: 100)
                .clipShape(RoundedRectangle(cornerRadius: 8))
        } else {
            Image(systemName: "photo")
        }
    }
}

private func cameraThumbnailImage(data: Data) -> CGImage? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
    let options: [CFString: Any] = [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceThumbnailMaxPixelSize: 320,
    ]
    return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
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

            CameraRecordingControls(
                isConnected: presentation.connection == .connected,
                recordingRequestKey: recordingRequestKey,
                isRequestingRecording: isRequestingRecording,
                supportsOnboardRecording: supportsOnboardRecording,
                stillRequestKey: stillRequestKey,
                isRequestingStill: isRequestingStill,
                supportsStillCapture: supportsStillCapture,
                requestRecording: requestRecording,
                requestStillCapture: requestStillCapture
            )

            if let previewRenderer, showsPreviewSurface {
                CameraPreviewSurface(renderer: previewRenderer)
                    .accessibilityIdentifier("camera.preview.surface")
            }

            CameraPreviewControls(
                hasPreviewSource: hasPreviewSource,
                preview: presentation.preview,
                savedFileURL: savedFileURL,
                startPreview: startPreview,
                savePreview: savePreview,
                stopPreview: stopPreview
            )
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

private struct CameraRecordingControls: View {
    let isConnected: Bool
    let recordingRequestKey: String?
    let isRequestingRecording: Bool
    let supportsOnboardRecording: Bool
    let stillRequestKey: String?
    let isRequestingStill: Bool
    let supportsStillCapture: Bool
    let requestRecording: ((Bool) -> Void)?
    let requestStillCapture: (() -> Void)?

    var body: some View {
        if isConnected, let requestRecording {
            VStack(alignment: .leading, spacing: 8) {
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
        }
    }
}

private struct CameraPreviewControls: View {
    let hasPreviewSource: Bool
    let preview: CameraPreviewPresentation
    let savedFileURL: URL?
    let startPreview: (() -> Void)?
    let savePreview: (() -> Void)?
    let stopPreview: (() -> Void)?

    var body: some View {
        if hasPreviewSource, let startPreview, let savePreview {
            VStack(alignment: .leading, spacing: 8) {
                Divider()
                HStack {
                    if preview == .stopped || preview == .interrupted {
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
