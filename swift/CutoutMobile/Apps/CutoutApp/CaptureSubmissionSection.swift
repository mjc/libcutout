import CutoutMobileFFI
import ImageIO
import Observation
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The same private Send presentation is embedded in both existing details.
struct CaptureSubmissionSection: View {
    @State private var model: CaptureSubmissionModel
    @State private var evidence: CaptureEvidenceEditorModel
    @State private var showsSendConfirmation = false
    @State private var showsDeleteConfirmation = false

    init(source: MobileCaptureSubmissionSourceDto) {
        _model = State(initialValue: CaptureSubmissionModel(source: source))
        _evidence = State(initialValue: CaptureEvidenceEditorModel(source: source))
    }

    var body: some View {
        Group {
            CaptureEvidenceSection(model: evidence, editable: evidenceEditable) { await model.refresh() }
            submissionSection
        }
        .task {
            await model.refresh()
            await evidence.refresh()
        }
    }

    private var evidenceEditable: Bool {
        switch model.status?.phase {
        case nil, .prepared, .cancelled, .deleted: true
        default: false
        }
    }

    private var submissionSection: some View {
        Section {
            Text("Send this recording privately to the CutOut maintainer to add or verify device and protocol support.")
                .confirmationDialog(
                    "Send this capture privately?", isPresented: $showsSendConfirmation, titleVisibility: .visible
                ) {
                    Button("Send to CutOut maintainer") { Task { await model.send() } }
                        .accessibilityIdentifier("captures.send.confirm")
                    Button("Not now", role: .cancel) {}
                } message: {
                    Text(confirmationText)
                }
                .confirmationDialog(
                    "Delete the sent copy?", isPresented: $showsDeleteConfirmation, titleVisibility: .visible
                ) {
                    Button("Delete sent copy", role: .destructive) { Task { await model.delete() } }
                    Button("Keep sent copy", role: .cancel) {}
                } message: {
                    Text(
                        "Remove the private server copy. Your original recording stays on this device. Operator backups and copies already downloaded may remain."
                    )
                }
            if let status = model.status {
                disclosure(status)
                actions(status)
            } else {
                Button {
                    Task {
                        await model.prepare()
                        if model.status?.phase == .prepared { showsSendConfirmation = true }
                    }
                } label: {
                    Label("Review private Send", systemImage: "paperplane")
                }
                .disabled(model.isWorking || !evidence.isLoaded || evidence.isWorking || evidence.isDirty)
                .accessibilityIdentifier("captures.send.review")
            }
            if model.isWorking { ProgressView(model.isSending ? "Sending privately…" : "Preparing…") }
            if let error = model.error {
                Text(error).foregroundStyle(PevColors.warningText)
                    .accessibilityIdentifier("captures.send.error")
            }
            if let failure = model.failureMessage {
                Text(failure).foregroundStyle(PevColors.warningText)
            }
        } header: {
            Text("Private protocol support")
        } footer: {
            Text(
                "Sending is optional. Your original recording stays on this device. Music and provider listening metadata are excluded. Raw Bluetooth bytes can contain identifiers that CutOut cannot recognize."
            )
        }
    }

    @ViewBuilder
    private func disclosure(_ status: MobileCaptureSubmissionStatusDto) -> some View {
        LabeledContent(
            "Snapshot size",
            value: ByteCountFormatter.string(fromByteCount: Int64(clamping: status.byteCount), countStyle: .file))
        Text("Includes raw Bluetooth evidence, event order and timing.")
        if !includedCategories(status).isEmpty {
            LabeledContent("Also includes", value: includedCategories(status).joined(separator: ", "))
        }
        if status.droppedMessages > 0 {
            Label(
                "This recording lost \(status.droppedMessages.formatted()) messages. The maintainer will receive that loss information.",
                systemImage: "exclamationmark.triangle"
            )
            .foregroundStyle(PevColors.warningText)
        }
        if let receipt = status.receipt {
            LabeledContent("Receipt", value: receipt.operationId)
                .textSelection(.enabled)
                .accessibilityIdentifier("captures.send.receipt")
            DisclosureGroup("Sent snapshot") {
                Text(receipt.sha256).font(.footnote.monospaced()).textSelection(.enabled)
            }
        }
    }

    @ViewBuilder
    private func actions(_ status: MobileCaptureSubmissionStatusDto) -> some View {
        switch status.phase {
        case .prepared:
            sendButton("Send privately")
            cancelButton
        case .sending:
            Text("The transfer has not received a durable receipt yet.")
            if !model.isSending { sendButton("Retry Send") }
            cancelButton
        case .failed:
            sendButton("Retry Send")
            cancelButton
        case .rejected:
            Text("The service rejected this snapshot. This Send cannot be retried.")
            cancelButton
        case .sent:
            if status.receipt != nil {
                Label("Received privately", systemImage: "checkmark.circle")
            }
            Button("Delete sent copy", role: .destructive) { showsDeleteConfirmation = true }
                .disabled(model.isWorking)
                .accessibilityIdentifier("captures.send.delete")
        case .cancelling:
            Text("Cancellation is pending.")
            cancelButton
        case .cancelled:
            Label("Send cancelled", systemImage: "xmark.circle")
        case .deleting:
            Text("Deletion is pending. The sent copy has not been confirmed removed.")
            Button("Retry deletion", role: .destructive) { showsDeleteConfirmation = true }
                .disabled(model.isWorking)
        case .deleted:
            Label("Sent copy deleted", systemImage: "checkmark.circle")
        }
    }

    private func sendButton(_ title: String) -> some View {
        Button {
            showsSendConfirmation = true
        } label: {
            Label(title, systemImage: "paperplane")
        }
        .disabled(model.isWorking || model.isSending || evidence.isWorking || evidence.isDirty)
        .accessibilityIdentifier("captures.send")
    }

    private var cancelButton: some View {
        Button("Cancel Send", role: .cancel) { Task { await model.cancel() } }
            .disabled(model.isCancelling || (model.isWorking && !model.isSending))
            .accessibilityIdentifier("captures.send.cancel")
    }

    private var confirmationText: String {
        guard let status = model.status else { return "Review the saved snapshot before sending." }
        let categories = (["raw Bluetooth evidence", "event order and timing"] + includedCategories(status)).joined(
            separator: ", ")
        return
            "Send \(categories) to the CutOut maintainer for protocol support. Music is excluded. Your original recording stays on this device."
    }

    private func includedCategories(_ status: MobileCaptureSubmissionStatusDto) -> [String] {
        var categories: [String] = []
        if status.categories.deviceIdentifiers { categories.append("device and platform identifiers") }
        if status.categories.location { categories.append("location") }
        if status.categories.notes { categories.append("descriptions and notes") }
        if !evidence.screenshots.isEmpty { categories.append("\(evidence.screenshots.count) selected screenshots") }
        if status.categories.annotations { categories.append("annotations and label boundaries") }
        if status.categories.settingsObservations { categories.append("user-entered settings values") }
        return categories
    }
}

@MainActor
@Observable
final class CaptureEvidenceEditorModel {
    let source: MobileCaptureSubmissionSourceDto
    var note = "" { didSet { if !isWorking { isDirty = true } } }
    private(set) var screenshots: [Data] = []
    private(set) var isLoaded = false
    private(set) var isWorking = false
    private(set) var isDirty = false
    private(set) var error: String?
    private(set) var noteLimit = 1024
    private(set) var screenshotLimit = 4

    init(source: MobileCaptureSubmissionSourceDto) { self.source = source }

    func refresh() async {
        guard !isWorking, !isDirty else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            let client = try CaptureSubmissionClientHandle(database: await RustPersistenceStore.open())
            let saved = try await client.evidence(source: source)
            note = saved.note
            screenshots = saved.screenshots
            noteLimit = Int(saved.noteLimitBytes)
            screenshotLimit = Int(saved.screenshotLimit)
            isLoaded = true
            error = nil
        } catch { self.error = CaptureSubmissionModel.message(for: error) }
    }

    func save() async -> Bool {
        guard !isWorking, isLoaded else { return false }
        isWorking = true
        defer { isWorking = false }
        do {
            let client = try CaptureSubmissionClientHandle(database: await RustPersistenceStore.open())
            let saved = try await client.saveEvidence(source: source, note: note, screenshots: screenshots)
            note = saved.note
            screenshots = saved.screenshots
            isDirty = false
            error = nil
            return true
        } catch {
            self.error = CaptureSubmissionModel.message(for: error)
            return false
        }
    }

    func removeScreenshot(at index: Int) {
        guard !isWorking, screenshots.indices.contains(index) else { return }
        screenshots.remove(at: index)
        isDirty = true
    }

    func importScreenshots(_ items: [PhotosPickerItem]) async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            var imported: [Data] = []
            for item in items {
                guard let data = try await item.loadTransferable(type: Data.self) else {
                    throw MobileCaptureSubmissionError.InvalidScreenshot
                }
                let jpeg = try await Task.detached(priority: .userInitiated) { try captureScreenshotJPEG(data) }.value
                imported.append(jpeg)
            }
            guard screenshots.count + imported.count <= screenshotLimit else {
                throw MobileCaptureSubmissionError.TooManyScreenshots
            }
            screenshots += imported
            isDirty = true
            error = nil
        } catch { self.error = CaptureSubmissionModel.message(for: error) }
    }
}

private func captureScreenshotJPEG(_ data: Data) throws -> Data {
    guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
        let image = CGImageSourceCreateThumbnailAtIndex(
            source, 0,
            [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: 2048,
                kCGImageSourceShouldCacheImmediately: true,
            ] as CFDictionary)
    else { throw MobileCaptureSubmissionError.InvalidScreenshot }
    let result = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(result, UTType.jpeg.identifier as CFString, 1, nil) else {
        throw MobileCaptureSubmissionError.InvalidScreenshot
    }
    CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.85] as CFDictionary)
    guard CGImageDestinationFinalize(destination) else { throw MobileCaptureSubmissionError.InvalidScreenshot }
    return result as Data
}

private struct CaptureEvidenceSection: View {
    @Bindable var model: CaptureEvidenceEditorModel
    let editable: Bool
    let onSaved: @MainActor () async -> Void
    @State private var selectedItems: [PhotosPickerItem] = []

    var body: some View {
        Section {
            TextField("Add a note", text: $model.note, axis: .vertical)
                .lineLimit(3...6)
                .disabled(!editable || !model.isLoaded || model.isWorking)
                .accessibilityIdentifier("captures.evidence.note")
            Text("\(model.note.utf8.count) / \(model.noteLimit) bytes")
                .font(.caption.monospacedDigit())
                .foregroundStyle(model.note.utf8.count > model.noteLimit ? PevColors.warningText : .secondary)
                .accessibilityIdentifier("captures.evidence.noteCount")
            ForEach(model.screenshots.indices, id: \.self) { index in
                HStack {
                    if let source = CGImageSourceCreateWithData(model.screenshots[index] as CFData, nil),
                        let image = CGImageSourceCreateThumbnailAtIndex(
                            source, 0,
                            [
                                kCGImageSourceCreateThumbnailFromImageAlways: true,
                                kCGImageSourceThumbnailMaxPixelSize: 320,
                            ] as CFDictionary)
                    {
                        Image(decorative: image, scale: 1).resizable().scaledToFit().frame(maxHeight: 160)
                            .accessibilityLabel("Screenshot \(index + 1)")
                    }
                    if editable {
                        Button(role: .destructive) {
                            model.removeScreenshot(at: index)
                        } label: {
                            Label("Remove screenshot \(index + 1)", systemImage: "trash")
                        }.buttonStyle(.borderless).disabled(model.isWorking)
                    }
                }
            }
            if editable {
                PhotosPicker(
                    selection: $selectedItems,
                    maxSelectionCount: max(1, model.screenshotLimit - model.screenshots.count),
                    matching: .images, preferredItemEncoding: .compatible
                ) {
                    Label(
                        "Add screenshots (\(model.screenshots.count)/\(model.screenshotLimit))",
                        systemImage: "photo.badge.plus")
                }
                .disabled(!model.isLoaded || model.isWorking || model.screenshots.count >= model.screenshotLimit)
                .accessibilityIdentifier("captures.evidence.add")
                Button("Save note and screenshots") {
                    Task { if await model.save() { await onSaved() } }
                }
                .disabled(
                    !model.isLoaded || !model.isDirty || model.isWorking || model.note.utf8.count > model.noteLimit
                )
                .accessibilityIdentifier("captures.evidence.save")
            }
            if model.isWorking { ProgressView() }
            if let error = model.error { Text(error).foregroundStyle(PevColors.warningText) }
        } header: {
            Text("Recording notes")
        }
        .onChange(of: selectedItems) { _, items in
            guard !items.isEmpty else { return }
            Task {
                await model.importScreenshots(items)
                selectedItems = []
            }
        }
    }
}
