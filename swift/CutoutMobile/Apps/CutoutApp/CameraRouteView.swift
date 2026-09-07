import CutoutMobile
import SwiftUI

struct CameraRouteContainerView: View {
    @State private var adapter = CameraLocalNetworkAdapter()
    @State private var previewRenderer = CameraPreviewRenderer()
    @State private var address = "192.168.1.254"
    @State private var port = "80"
    @State private var isReading = false
    @State private var readErrorKey: String?
    @State private var savedFileURL: URL?
    @State private var downloadedMediaURL: URL?
    @State private var downloadingMediaPath: String?
    @State private var mediaErrorKey: String?

    var body: some View {
        CameraRouteView(
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
            savedFileURL: savedFileURL,
            previewRenderer: previewRenderer,
            loadEvidence: readCamera,
            downloadMedia: downloadMedia,
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
            .onDisappear {
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
                _ = try await adapter.loadReadOnlyEvidence(address: address, port: portNumber)
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
                    try await adapter.startPreview(uri: uri, saveTo: url)
                    savedFileURL = url
                } else {
                    try await adapter.startPreview(uri: uri)
                }
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
        downloadingMediaPath = media.path
        mediaErrorKey = nil
        Task { @MainActor in
            defer { downloadingMediaPath = nil }
            do {
                try await adapter.downloadMedia(
                    address: address,
                    port: portNumber,
                    media: media,
                    to: destination
                )
                downloadedMediaURL = destination
            } catch {
                mediaErrorKey = "camera.error.media_download_failed"
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
            "camera-media-" + UUID().uuidString + "-" + media.name
        )
    }
}

struct CameraRouteView: View {
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
    let savedFileURL: URL?
    let previewRenderer: CameraPreviewRenderer?
    let loadEvidence: (() -> Void)?
    let downloadMedia: ((CameraMediaEvidence) -> Void)?
    let startPreview: (() -> Void)?
    let savePreview: (() -> Void)?
    let stopPreview: (() -> Void)?

    init(
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
        savedFileURL: URL? = nil,
        previewRenderer: CameraPreviewRenderer? = nil,
        loadEvidence: (() -> Void)? = nil,
        downloadMedia: ((CameraMediaEvidence) -> Void)? = nil,
        startPreview: (() -> Void)? = nil,
        savePreview: (() -> Void)? = nil,
        stopPreview: (() -> Void)? = nil
    ) {
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
        self.savedFileURL = savedFileURL
        self.previewRenderer = previewRenderer
        self.loadEvidence = loadEvidence
        self.downloadMedia = downloadMedia
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
            loadEvidence: loadEvidence,
            downloadMedia: downloadMedia
        )
        CameraTruthCard(
            presentation: presentation,
            savedFileURL: savedFileURL,
            previewRenderer: previewRenderer,
            hasPreviewSource: hasPreviewSource,
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
                                ProgressView()
                                    .controlSize(.small)
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
                        if downloadedMediaURL?.lastPathComponent.hasSuffix("-" + media.name) == true {
                            Text(localizedAppText("camera.evidence.media_saved"))
                                .font(.caption)
                                .foregroundStyle(PevColors.muted)
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
                .disabled(isReading || address.isEmpty || port.isEmpty)
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

private struct CameraTruthCard: View {
    let presentation: CameraPresentation
    let savedFileURL: URL?
    let previewRenderer: CameraPreviewRenderer?
    let hasPreviewSource: Bool
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
                    Text(savedFileURL.lastPathComponent)
                        .font(.footnote.monospaced())
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("camera.preview.saved-file")
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
