import CutoutMobile
import SwiftUI

/// Navigation and presentation only. Recording survives the lifetime of every view here.
struct BluetoothCapturesView: View {
    let model: CutoutAppModel
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
                ForEach(model.capture.completed) { artifact in
                    NavigationLink {
                        CaptureArtifactDetailView(artifact: artifact)
                    } label: {
                        CaptureArtifactRow(artifact: artifact)
                    }
                    .accessibilityIdentifier("captures.artifact.\(artifact.id.rawValue)")
                }
                if model.capture.completed.isEmpty {
                    Text(localizedAppText("captures.empty"))
                        .foregroundStyle(.secondary)
                }
            } header: {
                Text(localizedAppText("captures.recent"))
            } footer: {
                // Until the canonical Rust history query lands, don't present this
                // callback projection as a complete or persistent capture library.
                Text(localizedAppText("captures.session_history"))
            }
        }
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .navigationTitle(localizedAppText("captures.title"))
        .tint(PevColors.yellow)
        .accessibilityIdentifier("captures.home")
        .sheet(isPresented: $isNewCapturePresented, onDismiss: openStartedCapture) {
            CaptureDeviceSelectionView(
                sections: (model.devicePickerScanState ?? DevicePickerScanState(status: .idle, rows: [])).sections,
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
            if artifact.outcome == .failed {
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

    private var fileExists: Bool { FileManager.default.fileExists(atPath: artifact.fileURL.path) }

    var body: some View {
        List {
            Section {
                CaptureArtifactRow(artifact: artifact)
                LabeledContent(localizedAppText("captures.source"), value: localizedAppText(
                    artifact.isUserInitiated ? "captures.manual" : "captures.automatic"
                ))
                if let progress = artifact.progress {
                    LabeledContent(localizedAppText("capture.detail.elapsed"), value: progress.elapsedMetricValue.displayText)
                }
            }
            Section {
                if fileExists {
                    ShareLink(item: artifact.fileURL) {
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
                    Text(artifact.fileURL.lastPathComponent)
                        .font(.footnote.monospaced())
                        .textSelection(.enabled)
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
        .navigationTitle(localizedAppText(artifact.outcome == .saved ? "captures.saved" : "captures.incomplete"))
        .accessibilityIdentifier("captures.detail")
    }
}
