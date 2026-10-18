import CutoutMobileFFI
import SwiftUI

/// The same private Send presentation is embedded in both existing details.
struct CaptureSubmissionSection: View {
    @State private var model: CaptureSubmissionModel
    @State private var showsSendConfirmation = false
    @State private var showsDeleteConfirmation = false

    init(source: MobileCaptureSubmissionSourceDto) {
        _model = State(initialValue: CaptureSubmissionModel(source: source))
    }

    var body: some View {
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
                .disabled(model.isWorking)
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
        .task { await model.refresh() }
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
        .disabled(model.isWorking || model.isSending)
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
        if status.categories.annotations { categories.append("annotations and label boundaries") }
        if status.categories.settingsObservations { categories.append("user-entered settings values") }
        return categories
    }
}
