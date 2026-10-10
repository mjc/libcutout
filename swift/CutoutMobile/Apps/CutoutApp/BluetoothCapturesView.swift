import CutoutMobile
import CutoutMobileFFI
import SwiftUI

/// Navigation and presentation only. Recording survives the lifetime of every view here.
struct BluetoothCapturesView: View {
    let model: CutoutAppModel
    @State private var library = CaptureLibraryModel()
    @State private var isNewCapturePresented = false
    @State private var didStartCapture = false
    @State private var showsRecording = false

    var body: some View {
        List {
            if model.capture.activeGeneration != nil {
                Section {
                    NavigationLink {
                        CaptureRouteView(capture: model.capture)
                    } label: {
                        Label {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(localizedAppText("captures.active"))
                                    .font(.headline)
                                Text(model.capture.device?.title ?? localizedAppText("captures.automatic"))
                                    .foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "record.circle")
                                .foregroundStyle(PevColors.yellow)
                        }
                    }
                    .accessibilityIdentifier("captures.active")
                }
            }

            Section {
                Button(action: { isNewCapturePresented = true }) {
                    Label(localizedAppText("captures.new"), systemImage: "plus")
                        .frame(maxWidth: .infinity, minHeight: 44)
                }
                .buttonStyle(.borderedProminent)
                .tint(PevColors.yellow)
                .foregroundStyle(.black)
                .disabled(!model.capture.lifecycle.canStart)
                .accessibilityIdentifier("captures.new")
            }

            Section {
                ForEach(library.captures, id: \.liveCaptureId) { capture in
                    NavigationLink {
                        DatabaseCaptureDetailView(capture: capture)
                    } label: {
                        DatabaseCaptureRow(capture: capture)
                    }
                    .accessibilityIdentifier("captures.database.\(capture.liveCaptureId)")
                }
                if library.captures.isEmpty && !library.isLoading {
                    Text(localizedAppText("captures.empty")).foregroundStyle(.secondary)
                }
                if library.isLoading { ProgressView() }
                if library.hasMore {
                    Button("Load more recordings") { Task { await library.loadMore() } }
                }
                if let error = library.error {
                    Text(error).foregroundStyle(PevColors.warningText)
                    Button("Retry") { Task { await library.refresh() } }
                }
            } header: {
                Text(localizedAppText("captures.recent"))
            }
            // Session failures and file-only recordings have no durable database row.
            let local = model.capture.completed.filter { artifact in
                switch artifact.outcome {
                case .failed, .saved: true
                case .storedWithoutExport, .incomplete, .integrityUnknown: false
                }
            }
            if !local.isEmpty {
                Section("This session") {
                    ForEach(local) { artifact in
                        NavigationLink {
                            CaptureArtifactDetailView(artifact: artifact)
                        } label: {
                            CaptureArtifactRow(artifact: artifact)
                        }
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .navigationTitle(localizedAppText("captures.title"))
        .tint(PevColors.yellow)
        .accessibilityIdentifier("captures.home")
        .task { await library.refresh() }
        .refreshable { await library.refresh() }
        .onChange(of: model.capture.completed.count) {
            Task { await library.refresh() }
        }
        .sheet(isPresented: $isNewCapturePresented, onDismiss: openStartedCapture) {
            CaptureDeviceSelectionView(
                sections: (model.device.scanState ?? DevicePickerScanState(status: .idle, rows: [])).sections,
                start: start
            )
        }
        .navigationDestination(isPresented: $showsRecording) {
            CaptureRouteView(capture: model.capture)
        }
    }

    private func start(_ row: DevicePickerRow, description: String) -> Bool {
        guard model.recordOnly(platformIdentifier: row.id, deviceKind: description) else { return false }
        didStartCapture = true
        isNewCapturePresented = false
        return true
    }

    private func openStartedCapture() {
        if didStartCapture {
            showsRecording = true
            didStartCapture = false
        }
    }

}

private struct CaptureArtifactRow: View {
    let artifact: CaptureArtifact

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(artifact.device?.title ?? localizedAppText("captures.untitled"))
                .font(.headline)
            if let name = artifact.device?.advertisedName {
                Text(name).font(.subheadline).foregroundStyle(.secondary)
            }
            Text(artifact.startedAt, format: .dateTime.month(.abbreviated).day().hour().minute())
                .font(.subheadline).foregroundStyle(.secondary)
            switch artifact.outcome {
            case .saved: EmptyView()
            case .storedWithoutExport:
                Label(localizedAppText("captures.stored_without_export"), systemImage: "internaldrive")
                    .foregroundStyle(.secondary)
            case .incomplete:
                Label(localizedAppText("captures.incomplete"), systemImage: "exclamationmark.triangle")
                    .foregroundStyle(PevColors.warningText)
            case .integrityUnknown:
                Label(localizedAppText("captures.integrity_unknown"), systemImage: "questionmark.folder")
                    .foregroundStyle(.secondary)
            case .failed:
                Label(localizedAppText("captures.incomplete"), systemImage: "exclamationmark.triangle")
                    .foregroundStyle(PevColors.warningText)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }
}

struct CaptureArtifactDetailView: View {
    let artifact: CaptureArtifact

    private var fileExists: Bool {
        artifact.fileURL.map { FileManager.default.fileExists(atPath: $0.path) } ?? false
    }

    var body: some View {
        List {
            Section {
                CaptureArtifactRow(artifact: artifact)
                LabeledContent(
                    localizedAppText("captures.source"),
                    value: localizedAppText(
                        artifact.isUserInitiated ? "captures.manual" : "captures.automatic"
                    ))
                if let progress = artifact.progress {
                    LabeledContent(
                        localizedAppText("capture.detail.elapsed"), value: progress.elapsedMetricValue.displayText)
                }
            }
            Section {
                if fileExists, let fileURL = artifact.fileURL {
                    ShareLink(item: fileURL) {
                        Label(localizedAppText("captures.share"), systemImage: "square.and.arrow.up")
                    }
                    .accessibilityIdentifier("captures.share")
                } else {
                    Label(localizedAppText("captures.file_unavailable"), systemImage: "doc.badge.ellipsis")
                }
            } footer: {
                Text(localizedAppText("captures.privacy"))
            }
            Section {
                DisclosureGroup(localizedAppText("captures.technical")) {
                    if let fileURL = artifact.fileURL {
                        Text(fileURL.lastPathComponent)
                            .font(.footnote.monospaced())
                            .textSelection(.enabled)
                    }
                    if let identifier = artifact.device?.platformIdentifier {
                        Text(identifier).font(.footnote.monospaced()).textSelection(.enabled)
                    }
                    if let progress = artifact.progress {
                        PevDashboardKeyValueRows(rows: captureSessionDetailRows(progress: progress))
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .navigationTitle(localizedAppText(detailTitleKey))
        .accessibilityIdentifier("captures.detail")
    }

    private var detailTitleKey: String {
        switch artifact.outcome {
        case .saved: "captures.saved"
        case .storedWithoutExport: "captures.stored_without_export"
        case .incomplete, .failed: "captures.incomplete"
        case .integrityUnknown: "captures.integrity_unknown"
        }
    }
}

private struct DatabaseCaptureRow: View {
    let capture: MobileLiveCaptureHistoryEntryDto
    private var startedAt: Date { Date(timeIntervalSince1970: Double(capture.startedAtMilliseconds) / 1_000) }
    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(capture.recording?.advertisedName ?? capture.platformIdentifier).font(.headline)
            Text(startedAt, format: .dateTime.month(.abbreviated).day().hour().minute())
                .font(.subheadline).foregroundStyle(.secondary)
            if capture.interrupted {
                Label("Recording interrupted", systemImage: "exclamationmark.triangle").foregroundStyle(
                    PevColors.warningText)
            } else {
                switch capture.integrity {
                case .complete: EmptyView()
                case .incomplete:
                    Label(localizedAppText("captures.incomplete"), systemImage: "exclamationmark.triangle")
                        .foregroundStyle(PevColors.warningText)
                case .unknown:
                    Label(localizedAppText("captures.integrity_unknown"), systemImage: "questionmark.folder")
                        .foregroundStyle(.secondary)
                }
            }
        }.padding(.vertical, 4).accessibilityElement(children: .combine)
    }
}

private struct DatabaseCaptureDetailView: View {
    let capture: MobileLiveCaptureHistoryEntryDto
    @State private var export = CaptureExportModel()
    var body: some View {
        List {
            Section { DatabaseCaptureRow(capture: capture) }
            Section {
                LabeledContent("Events", value: capture.eventCount.formatted())
                LabeledContent("Stored bytes", value: capture.storedBytes.formatted())
                if let model = capture.recording?.model { LabeledContent("Model", value: model.value) }
            }
            Section {
                if let fileURL = export.fileURL {
                    ShareLink(item: fileURL) { Label("Share recording", systemImage: "square.and.arrow.up") }
                } else {
                    Button {
                        Task { await export.export(id: capture.liveCaptureId) }
                    } label: {
                        if export.isExporting {
                            ProgressView("Preparing recording…")
                        } else {
                            Label("Export recording", systemImage: "square.and.arrow.up")
                        }
                    }.disabled(export.isExporting)
                }
                if let error = export.error { Text(error).foregroundStyle(PevColors.warningText) }
            } footer: {
                Text("The recording is saved on this device. Export creates a shareable file from its retained data.")
            }
        }
        .navigationTitle("Recording")
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .tint(PevColors.yellow)
    }
}
