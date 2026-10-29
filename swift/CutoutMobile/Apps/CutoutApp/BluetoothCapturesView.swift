import CutoutMobile
import CutoutMobileFFI
import Observation
import SwiftUI

/// Navigation and presentation only. Recording survives the lifetime of every view here.
struct BluetoothCapturesView: View {
    let model: CutoutAppModel
    @State private var library = CaptureLibraryModel()
    @State private var isNewCapturePresented = false
    @State private var didStartCapture = false
    @State private var showsRecording = false
    @State private var showsBatchReview = false

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
                Button {
                    _ = model.captureSubmissionBatchReview {
                        Task {
                            await RustPersistenceStore.deduplicateManagedCaptureFiles()
                            await library.refresh()
                        }
                    }
                    showsBatchReview = true
                } label: {
                    Label(localizedAppText("capture.batch.review"), systemImage: "paperplane")
                }
                .accessibilityIdentifier("captures.batch.review")
                ForEach(library.captures, id: \.liveCaptureId) { capture in
                    NavigationLink {
                        DatabaseCaptureDetailView(capture: capture) {
                            Task { await library.refresh() }
                        }
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
        .sheet(isPresented: $showsBatchReview) {
            if let batchModel = model.captureSubmissionBatch {
                CaptureSubmissionBatchReviewView(model: batchModel)
            }
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

private struct CaptureSubmissionBatchReviewView: View {
    @Bindable var model: CaptureSubmissionBatchModel
    @Environment(\.dismiss) private var dismiss
    @State private var showsSendConfirmation = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Text(localizedAppText("capture.batch.explanation"))
                        .font(.subheadline).foregroundStyle(.secondary)
                    if model.isReviewing { ProgressView(localizedAppText("capture.batch.reviewing")) }
                    if let error = model.error {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(PevColors.warningText)
                        if !model.isSending {
                            Button(localizedAppText("capture.batch.retry_review")) { Task { await model.review() } }
                                .disabled(model.isReviewing)
                        }
                    }
                }

                Section("Private Send review") {
                    LabeledContent(localizedAppText("capture.batch.eligible"), value: model.eligibleCount.formatted())
                    LabeledContent(localizedAppText("capture.batch.skipped"), value: model.skippedCount.formatted())
                    LabeledContent(localizedAppText("capture.batch.selected"), value: model.selectedCount.formatted())
                    if model.isSending || model.didFinishSend {
                        LabeledContent(
                            localizedAppText("capture.batch.sent"),
                            value: batchCount(\.sent, terminal: model.summary?.sent))
                        LabeledContent(
                            localizedAppText("capture.batch.local_removed"),
                            value: batchCount(\.cleaned, terminal: model.summary?.cleaned))
                        LabeledContent(
                            localizedAppText("capture.batch.failed"),
                            value: batchCount(\.failed, terminal: model.summary?.failed))
                        LabeledContent(
                            localizedAppText("capture.batch.skipped"),
                            value: batchCount(\.skipped, terminal: model.summary?.skipped))
                        LabeledContent(
                            localizedAppText("capture.batch.cancelled"),
                            value: batchCount(\.cancelled, terminal: model.summary?.cancelled))
                        if let summary = model.summary, !summary.pendingCleanupSourceArtifactIDs.isEmpty {
                            LabeledContent(
                                localizedAppText("capture.batch.pending_cleanup"),
                                value: summary.pendingCleanupSourceArtifactIDs.count.formatted())
                        }
                        if model.isSending, let progress = model.progress {
                            LabeledContent(
                                localizedAppText("capture.batch.position"),
                                value: "\(progress.current.formatted()) of \(progress.total.formatted())")
                        }
                    }
                }

                Section(localizedAppText("capture.batch.recordings")) {
                    ForEach(model.candidates) { candidate in
                        VStack(alignment: .leading, spacing: 8) {
                            Toggle(
                                isOn: Binding(
                                    get: { model.selectedSourceArtifactIDs.contains(candidate.sourceArtifactID) },
                                    set: { model.setSelected($0, candidate: candidate) }
                                )
                            ) {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(candidate.displayName ?? candidate.sourceKind)
                                        .font(.headline)
                                    Text(
                                        candidate.isEligible
                                            ? candidate.sourceKind : candidate.skipReason ?? "Unavailable"
                                    )
                                    .font(.subheadline)
                                    .foregroundStyle(
                                        candidate.isEligible || candidate.isChecking
                                            ? .secondary : PevColors.warningText)
                                }
                            }
                            .disabled(!candidate.isEligible || model.isSending)
                            candidateDisclosure(candidate)
                            if let item = model.itemProgress[candidate.sourceArtifactID] {
                                Label(
                                    item.message,
                                    systemImage: item.isFailure ? "exclamationmark.triangle" : "checkmark.circle"
                                )
                                .font(.subheadline)
                                .foregroundStyle(item.isFailure ? PevColors.warningText : PevColors.green)
                            }
                        }
                        .padding(.vertical, 4)
                        .accessibilityIdentifier("captures.batch.candidate.\(candidate.sourceArtifactID)")
                    }
                }

                if !model.failedSourceArtifactIDs.isEmpty {
                    Section(localizedAppText("capture.batch.failed_section")) {
                        Text(localizedAppText("capture.batch.retained"))
                            .foregroundStyle(.secondary)
                    }
                }
                if model.didFinishSend, model.requiresFreshReview {
                    Section {
                        Button(localizedAppText("capture.batch.review_again")) { Task { await model.review() } }
                            .disabled(model.isReviewing || model.isSending)
                            .accessibilityIdentifier("captures.batch.reviewAgain")
                    }
                }
                if model.didFinishSend, !model.requiresFreshReview, !model.failedSourceArtifactIDs.isEmpty {
                    Section {
                        Button(localizedAppText("capture.batch.retry_failed")) { Task { await model.reviewFailed() } }
                            .disabled(model.isSending)
                            .accessibilityIdentifier("captures.batch.reviewFailedAgain")
                    }
                }
            }
            .navigationTitle(localizedAppText("capture.batch.title"))
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(localizedAppText("capture.batch.close")) { dismiss() }
                }
            }
            .safeAreaInset(edge: .bottom) {
                VStack(spacing: 10) {
                    if model.isSending {
                        ProgressView(localizedAppText("capture.batch.sending"))
                            .accessibilityIdentifier("captures.batch.progress")
                        Button(localizedAppText("capture.batch.stop"), role: .cancel) {
                            model.requestStopAfterCurrent()
                        }
                        .disabled(model.didRequestStop)
                        .accessibilityIdentifier("captures.batch.stop")
                    } else if !model.didFinishSend {
                        Button(
                            localizedAppText(
                                model.selectedCount == model.eligibleCount
                                    ? "capture.batch.send" : "capture.batch.send_selected")
                        ) { showsSendConfirmation = true }
                        .buttonStyle(.borderedProminent)
                        .tint(PevColors.yellow)
                        .foregroundStyle(.black)
                        .disabled(!model.canSend)
                        .accessibilityIdentifier("captures.batch.confirm")
                    }
                }
                .padding()
                .background(.bar)
            }
            .confirmationDialog(
                model.isReviewing && model.selectedCount > 0 && model.selectedCount == model.eligibleCount
                    ? localizedAppText("capture.batch.confirm_all")
                    : localizedAppText("capture.batch.confirm", model.selectedCount),
                isPresented: $showsSendConfirmation,
                titleVisibility: .visible
            ) {
                Button(
                    localizedAppText(
                        model.selectedCount == model.eligibleCount
                            ? "capture.batch.send_local" : "capture.batch.send_selected_local"), role: .destructive
                ) {
                    model.beginSendSelected()
                }
                Button(localizedAppText("capture.batch.not_now"), role: .cancel) {}
            }
        }
    }

    private func batchCount(
        _ keyPath: KeyPath<CaptureSubmissionBatchProgress, UInt64>,
        terminal: UInt64?
    ) -> String {
        if !model.isSending, let terminal { return terminal.formatted() }
        return model.progress?[keyPath: keyPath].formatted() ?? terminal?.formatted() ?? "0"
    }

    @ViewBuilder
    private func candidateDisclosure(_ candidate: CaptureSubmissionBatchCandidate) -> some View {
        LabeledContent(localizedAppText("capture.batch.origin_label"), value: candidate.origin)
        LabeledContent(localizedAppText("capture.batch.integrity_label"), value: candidate.integrity)
        LabeledContent(
            localizedAppText("capture.batch.source_label"),
            value: localizedAppText(
                "capture.batch.source", candidate.sourceBytes.formatted(),
                candidate.sourceSHA256 ?? localizedAppText("capture.batch.unavailable")))
        LabeledContent(
            localizedAppText("capture.batch.transfer_label"),
            value: candidate.transferBytes.flatMap { bytes in
                candidate.transferSHA256.map { localizedAppText("capture.batch.transfer", bytes.formatted(), $0) }
            } ?? localizedAppText("capture.batch.unavailable"))
        if !candidate.categories.isEmpty {
            Text(localizedAppText("capture.batch.includes", candidate.categories.joined(separator: ", ")))
                .font(.subheadline).foregroundStyle(.secondary)
        }
        LabeledContent(
            localizedAppText("capture.batch.evidence_label"),
            value: localizedAppText(
                "capture.batch.evidence", candidate.noteBytes.formatted(), candidate.screenshotCount))
        if let receipt = candidate.receipt {
            Label(localizedAppText("capture.batch.sent_receipt", receipt), systemImage: "checkmark.circle.fill")
                .foregroundStyle(PevColors.green)
        }
    }
}

struct CaptureSubmissionBatchCandidate: Identifiable, Equatable, Sendable {
    var id: String { sourceArtifactID }
    let sourceArtifactID: String
    let sourceKind: String
    let displayName: String?
    let isEligible: Bool
    let isChecking: Bool
    let skipReason: String?
    let origin: String
    let integrity: String
    let sourceBytes: UInt64
    let sourceSHA256: String?
    let transferBytes: UInt64?
    let transferSHA256: String?
    let categories: [String]
    let noteBytes: UInt64
    let screenshotCount: UInt64
    let receipt: String?
}

struct CaptureSubmissionBatchProgress: Equatable, Sendable {
    enum Phase: Equatable, Sendable { case current, sent, cleaned, failed, skipped, cancelled }
    let sourceArtifactID: String
    let phase: Phase
    let message: String
    let total: UInt64
    let sent: UInt64
    let cleaned: UInt64
    let failed: UInt64
    let skipped: UInt64
    let cancelled: UInt64
    let current: UInt64
}

struct CaptureSubmissionBatchSummary: Equatable, Sendable {
    let sent: UInt64
    let cleaned: UInt64
    let failed: UInt64
    let skipped: UInt64
    let cancelled: UInt64
    let failedSourceArtifactIDs: [String]
    let pendingCleanupSourceArtifactIDs: [String]
}

struct CaptureSubmissionBatchAdapter {
    typealias ProgressUpdate = @MainActor @Sendable (CaptureSubmissionBatchProgress) -> Void
    typealias CandidateUpdate = @MainActor @Sendable (CaptureSubmissionBatchCandidate) -> Void
    var review: @MainActor (@escaping CandidateUpdate) async throws -> [CaptureSubmissionBatchCandidate]
    var sendSelected: @MainActor ([String], @escaping ProgressUpdate) async throws -> CaptureSubmissionBatchSummary
    var requestCancel: @MainActor () -> Void
    var commitSendAll: @MainActor (@escaping ProgressUpdate) async throws -> Bool
    var takeCommittedSummary: @MainActor () -> CaptureSubmissionBatchSummary?

    init(
        review: @escaping @MainActor (@escaping CandidateUpdate) async throws -> [CaptureSubmissionBatchCandidate],
        sendSelected:
            @escaping @MainActor ([String], @escaping ProgressUpdate) async throws -> CaptureSubmissionBatchSummary,
        requestCancel: @escaping @MainActor () -> Void,
        commitSendAll: @escaping @MainActor (@escaping ProgressUpdate) async throws -> Bool = { _ in false },
        takeCommittedSummary: @escaping @MainActor () -> CaptureSubmissionBatchSummary? = { nil }
    ) {
        self.review = review
        self.sendSelected = sendSelected
        self.requestCancel = requestCancel
        self.commitSendAll = commitSendAll
        self.takeCommittedSummary = takeCommittedSummary
    }

    static var live: CaptureSubmissionBatchAdapter {
        let state = CaptureSubmissionBatchFFIState()
        return CaptureSubmissionBatchAdapter(
            review: { onCandidate in try await state.review(onCandidate: onCandidate) },
            sendSelected: { ids, update in try await state.sendSelected(ids, update: update) },
            requestCancel: { state.requestCancel() },
            commitSendAll: { update in try await state.commitSendAll(update: update) },
            takeCommittedSummary: { state.takeCommittedSummary() }
        )
    }
}

@MainActor
private final class CaptureSubmissionBatchFFIState {
    private var client: CaptureSubmissionClientHandle?
    private var plan: CaptureSubmissionBatchPlanHandle?

    func review(onCandidate: @escaping CaptureSubmissionBatchAdapter.CandidateUpdate) async throws
        -> [CaptureSubmissionBatchCandidate]
    {
        let database = try await RustPersistenceStore.open()
        let client = try CaptureSubmissionClientHandle(database: database)
        let fileManager = FileManager.default
        guard let documents = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first else {
            throw CocoaError(.fileNoSuchFile)
        }
        let roots = [
            documents.path,
            fileManager.temporaryDirectory.appendingPathComponent("CaptureExports", isDirectory: true).path,
        ]
        self.client = client
        let listener = CaptureSubmissionBatchReviewListenerAdapter(update: onCandidate)
        return try await withTaskCancellationHandler {
            let plan = try await client.reviewAll(managedCaptureDirectories: roots, listener: listener)
            self.plan = plan
            return projectedCaptureSubmissionBatchCandidates(plan.candidates())
        } onCancel: {
            client.cancelReview()
        }
    }

    func sendSelected(
        _ ids: [String],
        update: @escaping CaptureSubmissionBatchAdapter.ProgressUpdate
    ) async throws -> CaptureSubmissionBatchSummary {
        guard let client, let plan else { throw CocoaError(.fileReadNoSuchFile) }
        let credential = try await ChargingCommunitySync.shared.credentialForCaptureSubmission()
        let summary = try await client.sendSelected(
            plan: plan,
            selectedSourceArtifactIds: ids,
            accountId: credential.identityID,
            token: credential.token,
            listener: CaptureSubmissionBatchProgressListenerAdapter(update: update)
        )
        return CaptureSubmissionBatchSummary(
            sent: summary.sent, cleaned: summary.cleaned, failed: summary.failed,
            skipped: summary.skipped, cancelled: summary.cancelled,
            failedSourceArtifactIDs: summary.failedSourceArtifactIds,
            pendingCleanupSourceArtifactIDs: summary.pendingCleanupSourceArtifactIds
        )
    }

    func requestCancel() {
        client?.cancelReview()
        plan?.requestCancel()
    }

    func commitSendAll(update: @escaping CaptureSubmissionBatchAdapter.ProgressUpdate) async throws -> Bool {
        guard let client else { return false }
        let credential = try await ChargingCommunitySync.shared.credentialForCaptureSubmission()
        let listener = CaptureSubmissionBatchProgressListenerAdapter(update: update)
        return client.commitSendAll(
            accountId: credential.identityID, token: credential.token, listener: listener)
    }

    func takeCommittedSummary() -> CaptureSubmissionBatchSummary? {
        guard let summary = client?.takeCommittedSendSummary() else { return nil }
        return CaptureSubmissionBatchSummary(
            sent: summary.sent, cleaned: summary.cleaned, failed: summary.failed,
            skipped: summary.skipped, cancelled: summary.cancelled,
            failedSourceArtifactIDs: summary.failedSourceArtifactIds,
            pendingCleanupSourceArtifactIDs: summary.pendingCleanupSourceArtifactIds
        )
    }
}

func projectedCaptureSubmissionBatchCandidates(
    _ candidates: [MobileCaptureSubmissionBatchCandidateDto]
) -> [CaptureSubmissionBatchCandidate] {
    candidates.map(CaptureSubmissionBatchFFIState.project)
}

extension CaptureSubmissionBatchFFIState {
    nonisolated fileprivate static func project(_ candidate: MobileCaptureSubmissionBatchCandidateDto)
        -> CaptureSubmissionBatchCandidate
    {
        let categories: [String] =
            candidate.categories.map { value in
                var included: [String] = []
                if value.deviceIdentifiers { included.append(localizedAppText("capture.batch.category_identifiers")) }
                if value.location { included.append(localizedAppText("capture.batch.category_location")) }
                if value.notes { included.append(localizedAppText("capture.batch.category_notes")) }
                if value.annotations { included.append(localizedAppText("capture.batch.category_annotations")) }
                if value.settingsObservations { included.append(localizedAppText("capture.batch.category_settings")) }
                return included
            } ?? []
        let eligible = candidate.eligibility == .eligible
        let checking = candidate.eligibility == .checking
        return CaptureSubmissionBatchCandidate(
            sourceArtifactID: candidate.sourceArtifactId,
            sourceKind: sourceKindLabel(candidate.sourceKind),
            displayName: candidate.displayName.isEmpty ? nil : candidate.displayName,
            isEligible: eligible,
            isChecking: checking,
            skipReason: eligible ? nil : eligibilityLabel(candidate.skipReason ?? candidate.eligibility),
            origin: captureSubmissionOriginLabel(candidate.origin),
            integrity: captureSubmissionIntegrityLabel(candidate.integrity),
            sourceBytes: candidate.sourceBytes,
            sourceSHA256: candidate.sourceSha256,
            transferBytes: candidate.transferBytes,
            transferSHA256: candidate.transferSha256,
            categories: categories,
            noteBytes: UInt64(candidate.evidence?.noteBytes ?? 0),
            screenshotCount: UInt64(candidate.evidence?.screenshots ?? 0),
            receipt: candidate.status?.receipt?.operationId
        )
    }

    nonisolated private static func sourceKindLabel(_ kind: MobileCaptureSubmissionBatchSourceKindDto) -> String {
        switch kind {
        case .liveCapture: localizedAppText("capture.batch.kind_live")
        case .recordedArtifact: localizedAppText("capture.batch.kind_imported")
        case .managedFile: localizedAppText("capture.batch.kind_file")
        }
    }

    nonisolated fileprivate static func eligibilityLabel(_ eligibility: MobileCaptureSubmissionBatchEligibilityDto)
        -> String
    {
        switch eligibility {
        case .eligible: localizedAppText("capture.batch.eligible")
        case .alreadySubmitted: localizedAppText("capture.batch.reason_already_sent")
        case .active: localizedAppText("capture.batch.reason_active")
        case .empty: localizedAppText("capture.batch.reason_empty")
        case .tooLarge: localizedAppText("capture.batch.reason_too_large")
        case .corrupt: localizedAppText("capture.batch.reason_corrupt")
        case .unavailable: localizedAppText("capture.batch.reason_unavailable")
        case .checking: localizedAppText("capture.batch.reason_checking")
        }
    }

    nonisolated fileprivate static func failureText(_ failure: MobileCaptureSubmissionFailureDto) -> String {
        switch failure {
        case .network: "The network failed. This capture is still on this phone."
        case .unauthorized: "This account could not send the capture."
        case .forbidden: "This account cannot send the capture."
        case .conflict: "The server already has different information for this capture. It is still on this phone."
        case .rejected: "The server rejected this capture. It is still on this phone."
        case .throttled: "Waiting. The server asked to slow down."
        case .unavailable: "The server is unavailable. This capture is still on this phone."
        case .tooLarge: "This capture is too large to send."
        case .invalidCapture: "This capture could not be read."
        case .musicPresent: "This capture includes music and was not sent."
        case .quotaExceeded: "The server has no room for more captures."
        case .cancelled: "The send was cancelled."
        }
    }
}

private final class CaptureSubmissionBatchReviewListenerAdapter: MobileCaptureSubmissionBatchReviewListener,
    Sendable
{
    private let update: CaptureSubmissionBatchAdapter.CandidateUpdate

    init(update: @escaping CaptureSubmissionBatchAdapter.CandidateUpdate) { self.update = update }

    func onCandidate(candidate: MobileCaptureSubmissionBatchCandidateDto) {
        let projected = CaptureSubmissionBatchFFIState.project(candidate)
        let update = self.update
        Task { @MainActor in update(projected) }
    }
}

private final class CaptureSubmissionBatchProgressListenerAdapter: MobileCaptureSubmissionBatchProgressListener,
    Sendable
{
    private let update: CaptureSubmissionBatchAdapter.ProgressUpdate

    init(update: @escaping CaptureSubmissionBatchAdapter.ProgressUpdate) { self.update = update }

    func onProgress(progress: MobileCaptureSubmissionBatchProgressDto) {
        let event = Self.project(progress)
        Task { @MainActor [update] in update(event) }
    }

    nonisolated private static func project(_ progress: MobileCaptureSubmissionBatchProgressDto)
        -> CaptureSubmissionBatchProgress
    {
        let phase: CaptureSubmissionBatchProgress.Phase
        let message: String
        switch progress.phase {
        case .current:
            phase = .current
            if progress.status?.failure == .throttled {
                message = CaptureSubmissionBatchFFIState.failureText(.throttled)
            } else if let sent = progress.bytesSent, let total = progress.totalBytes {
                message = localizedAppText("capture.batch.phase_bytes", sent.formatted(), total.formatted())
            } else {
                message = localizedAppText("capture.batch.phase_current")
            }
        case .sent:
            phase = .sent
            message = localizedAppText("capture.batch.phase_receipt")
        case .cleaned:
            phase = .cleaned
            message = localizedAppText("capture.batch.phase_cleaned")
        case .failed:
            phase = .failed
            let retained = localizedAppText("capture.batch.phase_failed")
            if let failure = progress.status?.failure {
                message = "\(CaptureSubmissionBatchFFIState.failureText(failure)) \(retained)"
            } else {
                message =
                    progress.reason.map {
                        localizedAppText(
                            "capture.batch.phase_failed_reason",
                            CaptureSubmissionBatchFFIState.eligibilityLabel($0),
                            retained)
                    } ?? retained
            }
        case .skipped:
            phase = .skipped
            message =
                progress.reason.map(CaptureSubmissionBatchFFIState.eligibilityLabel)
                ?? localizedAppText("capture.batch.skipped")
        case .cancelled:
            phase = .cancelled
            message = localizedAppText("capture.batch.phase_cancelled")
        }
        return CaptureSubmissionBatchProgress(
            sourceArtifactID: progress.sourceArtifactId,
            phase: phase,
            message: message,
            total: progress.total,
            sent: progress.sent,
            cleaned: progress.cleaned,
            failed: progress.failed,
            skipped: progress.skipped,
            cancelled: progress.cancelled,
            current: progress.current
        )
    }
}

@MainActor
@Observable
final class CaptureSubmissionBatchModel: Identifiable {
    let id = UUID()
    private(set) var candidates: [CaptureSubmissionBatchCandidate] = []
    private(set) var selectedSourceArtifactIDs: Set<String> = []
    private(set) var isReviewing = false
    private(set) var isWaitingToSend = false
    private(set) var isSending = false
    private(set) var didFinishSend = false
    private(set) var didRequestStop = false
    private(set) var requiresFreshReview = false
    private(set) var error: String?
    private(set) var progress: CaptureSubmissionBatchProgress?
    private(set) var itemProgress: [String: ItemProgress] = [:]
    private(set) var summary: CaptureSubmissionBatchSummary?
    private(set) var failedSourceArtifactIDs: [String] = []
    private var sendAttempt: UInt64 = 0
    private var userAdjustedSelection = false
    private var reviewWaiters: [CheckedContinuation<Void, Never>] = []
    private var reviewTask: Task<Void, Never>?
    @ObservationIgnored private let adapter: CaptureSubmissionBatchAdapter
    @ObservationIgnored private var onTerminalUpdate: @MainActor () -> Void

    struct ItemProgress: Equatable, Sendable {
        let phase: CaptureSubmissionBatchProgress.Phase
        let message: String
        let isFailure: Bool
    }

    init(
        adapter: CaptureSubmissionBatchAdapter,
        onTerminalUpdate: @escaping @MainActor () -> Void = {}
    ) {
        self.adapter = adapter
        self.onTerminalUpdate = onTerminalUpdate
    }

    var eligibleCount: Int { candidates.filter(\.isEligible).count }
    var skippedCount: Int { candidates.filter { !$0.isEligible && !$0.isChecking }.count }
    var selectedCount: Int { selectedSourceArtifactIDs.count }
    var canSend: Bool {
        if isSending || isWaitingToSend || didFinishSend || error != nil || selectedCount == 0 {
            return false
        }
        return isReviewing || !requiresFreshReview
    }
    func replaceTerminalUpdate(_ update: @escaping @MainActor () -> Void) {
        onTerminalUpdate = update
    }

    /// Starts the check once. Closing the sheet does not cancel this task.
    func ensureReviewStarted() {
        guard reviewTask == nil else { return }
        reviewTask = Task { await review() }
    }

    func review() async {
        await review(preselecting: nil)
    }

    func reviewFailed() async {
        guard didFinishSend, !failedSourceArtifactIDs.isEmpty else { return }
        await review(preselecting: Set(failedSourceArtifactIDs))
    }

    func cancelReview() {
        adapter.requestCancel()
    }

    private func review(preselecting previousFailures: Set<String>?) async {
        guard !isReviewing, !isSending else { return }
        isReviewing = true
        requiresFreshReview = true
        error = nil
        candidates = []
        selectedSourceArtifactIDs = []
        userAdjustedSelection = false
        progress = nil
        itemProgress = [:]
        summary = nil
        didFinishSend = false
        defer {
            isReviewing = false
            let waiters = reviewWaiters
            reviewWaiters.removeAll()
            for waiter in waiters {
                waiter.resume()
            }
        }
        do {
            let reviewed = try await adapter.review { [weak self] candidate in
                self?.upsertReviewed(candidate, preselecting: previousFailures)
            }
            let finished = !Task.isCancelled && !reviewed.contains(where: \.isChecking)
            candidates = reviewed
            let eligible = Set(reviewed.filter(\.isEligible).map(\.sourceArtifactID))
            if finished && !userAdjustedSelection {
                selectedSourceArtifactIDs = Set(
                    reviewed.filter { candidate in
                        guard candidate.isEligible else { return false }
                        return previousFailures?.contains(candidate.sourceArtifactID) ?? true
                    }.map(\.sourceArtifactID))
            } else {
                selectedSourceArtifactIDs.formIntersection(eligible)
            }
            requiresFreshReview = !finished
            if finished {
                didRequestStop = false
                failedSourceArtifactIDs = []
            }
        } catch is CancellationError {
            requiresFreshReview = true
        } catch {
            self.error = localizedAppText("capture.batch.review_error")
            requiresFreshReview = true
        }
    }

    private func upsertReviewed(
        _ candidate: CaptureSubmissionBatchCandidate,
        preselecting previousFailures: Set<String>?
    ) {
        if let index = candidates.firstIndex(where: { $0.sourceArtifactID == candidate.sourceArtifactID }) {
            if candidates[index].isEligible && candidate.isChecking { return }
            candidates[index] = candidate
        } else {
            candidates.append(candidate)
        }
        guard candidate.isEligible else {
            selectedSourceArtifactIDs.remove(candidate.sourceArtifactID)
            return
        }
        guard !userAdjustedSelection else { return }
        let allowed = previousFailures?.contains(candidate.sourceArtifactID) ?? true
        if allowed {
            selectedSourceArtifactIDs.insert(candidate.sourceArtifactID)
        }
    }

    func setSelected(_ selected: Bool, candidate: CaptureSubmissionBatchCandidate) {
        guard candidate.isEligible, !isSending else { return }
        userAdjustedSelection = true
        if selected {
            selectedSourceArtifactIDs.insert(candidate.sourceArtifactID)
        } else {
            selectedSourceArtifactIDs.remove(candidate.sourceArtifactID)
        }
    }

    func sendSelected() async {
        await send(Array(selectedSourceArtifactIDs))
    }

    /// The sheet can close while this runs. The model keeps the send.
    func beginSendSelected() {
        let ids = Array(selectedSourceArtifactIDs)
        Task { await self.send(ids) }
    }

    private func applySendProgress(_ event: CaptureSubmissionBatchProgress, attempt: UInt64) {
        guard isSending, sendAttempt == attempt else { return }
        progress = event
        let isFailure = event.phase == .failed
        itemProgress[event.sourceArtifactID] = ItemProgress(
            phase: event.phase, message: event.message, isFailure: isFailure)
        if isFailure, !failedSourceArtifactIDs.contains(event.sourceArtifactID) {
            failedSourceArtifactIDs.append(event.sourceArtifactID)
        } else if event.phase == .sent || event.phase == .cleaned {
            failedSourceArtifactIDs.removeAll { $0 == event.sourceArtifactID }
        }
    }

    func requestStopAfterCurrent() {
        guard isSending, !didRequestStop else { return }
        didRequestStop = true
        requiresFreshReview = true
        adapter.requestCancel()
    }

    private func waitForReviewToEnd() async {
        if !isReviewing { return }
        await withCheckedContinuation { continuation in
            reviewWaiters.append(continuation)
        }
    }

    private func send(_ sourceArtifactIDs: [String]) async {
        if isWaitingToSend || isSending { return }
        var acceptedPartialReview = false
        var ids = sourceArtifactIDs
        let eligibleNow = Set(candidates.filter(\.isEligible).map(\.sourceArtifactID))
        let sendingAll = !ids.isEmpty && Set(ids) == eligibleNow
        if isReviewing && sendingAll {
            isSending = true
            didRequestStop = false
            error = nil
            progress = nil
            failedSourceArtifactIDs = []
            let attempt = sendAttempt &+ 1
            sendAttempt = attempt
            do {
                var accepted = false
                for _ in 0..<5 {
                    accepted = try await adapter.commitSendAll { [weak self] event in
                        self?.applySendProgress(event, attempt: attempt)
                    }
                    if accepted || !isReviewing { break }
                    await Task.yield()
                }
                if isReviewing { await waitForReviewToEnd() }
                if accepted, let result = adapter.takeCommittedSummary(), sendAttempt == attempt {
                    summary = result
                    failedSourceArtifactIDs = result.failedSourceArtifactIDs
                    requiresFreshReview = requiresFreshReview || result.cancelled > 0
                    didFinishSend = true
                    isSending = false
                    onTerminalUpdate()
                    return
                }
            } catch {
                guard sendAttempt == attempt else { return }
                self.error = error.localizedDescription
                didFinishSend = true
                isSending = false
                onTerminalUpdate()
                return
            }
            guard sendAttempt == attempt else { return }
            isSending = false
            ids = candidates.filter(\.isEligible).map(\.sourceArtifactID)
            acceptedPartialReview = true
        } else if isReviewing {
            acceptedPartialReview = true
            isWaitingToSend = true
            adapter.requestCancel()
            await waitForReviewToEnd()
            isWaitingToSend = false
            let eligible = Set(candidates.filter(\.isEligible).map(\.sourceArtifactID))
            ids = ids.filter { eligible.contains($0) }
        }
        guard !isSending, !didFinishSend, error == nil, !ids.isEmpty else { return }
        guard acceptedPartialReview || !requiresFreshReview else { return }
        sendAttempt &+= 1
        let attempt = sendAttempt
        isSending = true
        didRequestStop = false
        error = nil
        progress = nil
        failedSourceArtifactIDs = []
        defer { if sendAttempt == attempt { isSending = false } }
        do {
            let result = try await adapter.sendSelected(ids) { [weak self] event in
                self?.applySendProgress(event, attempt: attempt)
            }
            guard sendAttempt == attempt else { return }
            summary = result
            failedSourceArtifactIDs = result.failedSourceArtifactIDs
            requiresFreshReview = requiresFreshReview || result.cancelled > 0
            didFinishSend = true
            onTerminalUpdate()
        } catch {
            guard sendAttempt == attempt else { return }
            self.error = error.localizedDescription
            didFinishSend = true
            for sourceArtifactID in ids where !failedSourceArtifactIDs.contains(sourceArtifactID) {
                failedSourceArtifactIDs.append(sourceArtifactID)
            }
            onTerminalUpdate()
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
    @State private var export = CaptureExportModel()
    @State private var localPayloadState: MobileCaptureSubmissionLocalPayloadStateDto?

    private var fileExists: Bool {
        artifact.fileURL.map { FileManager.default.fileExists(atPath: $0.path) } ?? false
    }

    private var localPayloadAvailable: Bool {
        localPayloadState != .removalPending && localPayloadState != .removed
    }

    private var recordedArtifactID: String? {
        guard case .recordedArtifact(id: let id) = artifact.submissionSource else { return nil }
        return id
    }

    private var hasExportSource: Bool {
        recordedArtifactID != nil || fileExists
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
                if !localPayloadAvailable {
                    Text(
                        localPayloadState == .removed
                            ? "Local capture payload was removed after receipt confirmation."
                            : "Local capture payload removal is in progress.")
                } else if let fileURL = export.fileURL {
                    ShareLink(item: fileURL) {
                        Label(localizedAppText("captures.share"), systemImage: "square.and.arrow.up")
                    }
                    .accessibilityIdentifier("captures.share.link")
                } else if export.isExporting {
                    ProgressView(localizedAppText("captures.preparing"))
                } else {
                    if let error = export.error {
                        Text(error).foregroundStyle(PevColors.warningText)
                    }
                    Button(localizedAppText("captures.share")) {
                        Task { await prepareExport() }
                    }
                    .disabled(export.isExporting || !hasExportSource)
                    .accessibilityIdentifier("captures.share")
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
            if let source = artifact.submissionSource {
                CaptureSavedSettingsSection(source: source)
                CaptureSubmissionSection(source: source) { state in
                    localPayloadState = state
                    if state == .removalPending || state == .removed {
                        export.invalidate()
                    }
                }
                .id(artifact.id)
            }
        }
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .navigationTitle(localizedAppText(detailTitleKey))
        .accessibilityIdentifier("captures.detail")
    }

    private func prepareExport() async {
        if let recordedArtifactID {
            await export.export(recordedArtifactID: recordedArtifactID, fallbackSource: artifact.fileURL)
        } else if fileExists, let source = artifact.fileURL {
            await export.export(source: source)
        }
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
            if let badge = captureHistorySendBadge(capture.submission.status) {
                Label(
                    badge == .localDeleted
                        ? localizedAppText("capture.history.local_deleted")
                        : localizedAppText("capture.history.sent"), systemImage: "checkmark.circle.fill"
                )
                .foregroundStyle(PevColors.green)
                .accessibilityIdentifier("captures.history.\(capture.liveCaptureId).sent")
            }
        }.padding(.vertical, 4).accessibilityElement(children: .combine)
    }
}

enum CaptureHistorySendBadge: Equatable {
    case sent
    case localDeleted
}

func captureHistorySendBadge(_ status: MobileCaptureSubmissionStatusDto?) -> CaptureHistorySendBadge? {
    guard let status, status.receipt != nil else { return nil }
    switch status.phase {
    case .sent, .deleting, .deleted:
        return status.localPayloadState == .removed ? .localDeleted : .sent
    default:
        return nil
    }
}

private struct DatabaseCaptureDetailView: View {
    let capture: MobileLiveCaptureHistoryEntryDto
    let onSubmissionUpdated: () -> Void
    @State private var export = CaptureExportModel()
    @State private var localPayloadState: MobileCaptureSubmissionLocalPayloadStateDto?

    private var localPayloadAvailable: Bool {
        localPayloadState != .removalPending && localPayloadState != .removed
    }

    var body: some View {
        List {
            Section { DatabaseCaptureRow(capture: capture) }
            Section {
                LabeledContent("Events", value: capture.eventCount.formatted())
                LabeledContent("Stored bytes", value: capture.storedBytes.formatted())
                if let model = capture.recording?.model { LabeledContent("Model", value: model.value) }
            }
            Section {
                if !localPayloadAvailable {
                    Text(
                        localPayloadState == .removed
                            ? "Local capture payload was removed after receipt confirmation."
                            : "Local capture payload removal is in progress.")
                } else if let fileURL = export.fileURL {
                    ShareLink(item: fileURL) { Label("Share recording", systemImage: "square.and.arrow.up") }
                } else {
                    Button {
                        Task { await export.export(id: capture.liveCaptureId) }
                    } label: {
                        if export.isExporting {
                            ProgressView(localizedAppText("captures.preparing"))
                        } else {
                            Label("Export recording", systemImage: "square.and.arrow.up")
                        }
                    }.disabled(export.isExporting || !localPayloadAvailable)
                }
                if let error = export.error { Text(error).foregroundStyle(PevColors.warningText) }
            } footer: {
                Text("Export creates a shareable file from locally retained capture data.")
            }
            CaptureSavedSettingsSection(source: capture.submission.source)
            CaptureSubmissionSection(
                source: capture.submission.source,
                onLocalPayloadStateChange: { state in
                    localPayloadState = state
                    if state == .removalPending || state == .removed {
                        export.invalidate()
                    }
                },
                onStatusChange: { status in
                    if status?.receipt != nil { onSubmissionUpdated() }
                }
            )
            .id(capture.liveCaptureId)
        }
        .navigationTitle("Recording")
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .tint(PevColors.yellow)
    }
}
