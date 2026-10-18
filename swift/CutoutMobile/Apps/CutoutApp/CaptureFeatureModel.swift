import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Presentation of a writer-owned artifact, not a second capture store.
/// Relaunch history must come from Rust storage; this is only the live callback projection.
struct CaptureArtifact: Identifiable, Equatable {
    enum Outcome: Equatable {
        case saved
        case storedWithoutExport
        case incomplete(droppedMessages: UInt64)
        case integrityUnknown
        case failed
    }

    let id: CaptureGeneration
    let fileURL: URL?
    let startedAt: Date
    let device: CaptureDeviceIdentity?
    let isUserInitiated: Bool
    let progress: CaptureProgress?
    let outcome: Outcome
    var submissionSource: MobileCaptureSubmissionSourceDto? = nil
}

struct CaptureDeviceIdentity: Equatable {
    let platformIdentifier: String
    let title: String
    let advertisedName: String?

    init(row: DevicePickerRow) {
        platformIdentifier = row.id
        title = row.title
        advertisedName = row.advertisedName
    }
}

/// App-retained presentation. Only the session's typed writer events establish
/// recording and terminal outcomes; view navigation has no lifecycle effect.
@MainActor
@Observable
final class CaptureFeatureModel {
    @ObservationIgnored let sessionState: CutoutSessionStateHandle
    @ObservationIgnored private let flushOperation: @MainActor () async -> Bool
    @ObservationIgnored private let finishOperation: @MainActor () async -> Bool
    @ObservationIgnored private let changeCaptureLabel:
        (CaptureGeneration, MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto]
    @ObservationIgnored private let saveSettingsObservation:
        (CaptureGeneration, MobileCaptureSettingsDraftDto) async throws -> MobileCaptureSettingsObservationDto
    @ObservationIgnored private let readSettingsReadback:
        (CaptureGeneration, [MobileCaptureResearchReadbackRequestDto]) async throws ->
            [MobileCaptureResearchReadbackDto]
    @ObservationIgnored private let readSettingsObservations:
        (CaptureGeneration) throws -> [MobileCaptureSettingsObservationDto]
    let researchDraft = CaptureResearchDraft()
    private(set) var settingsObservations: [MobileCaptureSettingsObservationDto] = []
    private(set) var settingsError: String?
    @ObservationIgnored private var finishTask: Task<Bool, Never>?
    @ObservationIgnored private var finishRequestGeneration: CaptureGeneration?

    init(
        sessionState: CutoutSessionStateHandle,
        flush: @escaping @MainActor () async -> Bool = { false },
        finish: @escaping @MainActor () async -> Bool = { false },
        changeCaptureLabel: @escaping (CaptureGeneration, MobileCaptureLabelActionDto) throws ->
            [MobileCaptureLabelDto] = { _, _ in
                throw MobileCaptureAnnotationError.NotRecording
            },
        saveSettingsObservation: @escaping (CaptureGeneration, MobileCaptureSettingsDraftDto) async throws ->
            MobileCaptureSettingsObservationDto = { _, _ in throw MobileCaptureResearchError.NotRecording },
        readSettingsObservations: @escaping (CaptureGeneration) throws -> [MobileCaptureSettingsObservationDto] = { _ in
            []
        },
        readSettingsReadback: @escaping (CaptureGeneration, [MobileCaptureResearchReadbackRequestDto]) async throws ->
            [MobileCaptureResearchReadbackDto] = { _, _ in [] }
    ) {
        self.sessionState = sessionState
        flushOperation = flush
        finishOperation = finish
        self.changeCaptureLabel = changeCaptureLabel
        self.saveSettingsObservation = saveSettingsObservation
        self.readSettingsObservations = readSettingsObservations
        self.readSettingsReadback = readSettingsReadback
        lifecycle = sessionState.captureLifecycleSnapshot()
    }

    private(set) var lifecycle = MobileCaptureLifecycleSnapshotDto(attempt: nil, canStart: true, canPair: true)
    private var recordingStatus: CaptureStatus?
    private var presentedGeneration: CaptureGeneration?
    private(set) var progress: CaptureProgress?
    private(set) var activeLabels = Set<CaptureQuickLabel>()
    private(set) var annotationError: MobileCaptureAnnotationError?
    private(set) var isFinishRequested = false
    private(set) var deviceKind: String?
    private(set) var device: CaptureDeviceIdentity?
    private(set) var isUserInitiated = false
    private(set) var fileName: String?
    private(set) var notificationCount = 0
    private(set) var label: String?
    private(set) var completed: [CaptureArtifact] = []

    var latestGeneration: CaptureGeneration? {
        lifecycle.attempt.map { CaptureGeneration(rawValue: $0.generation.value) }
    }
    var activeGeneration: CaptureGeneration? {
        switch lifecycle.attempt?.stage {
        case .recording, .saving, .saveFailed, .finalizing: latestGeneration
        default: nil
        }
    }
    var isFinishing: Bool {
        isFinishRequested || lifecycle.attempt?.stage == .saving || lifecycle.attempt?.stage == .finalizing
    }
    var isManualCapture: Bool { lifecycle.attempt?.origin == .manual && activeGeneration != nil }
    var status: CaptureStatus? {
        // A rejected startup belongs to the setup error, not to the previous artifact.
        if lifecycle.attempt?.stage == .failed, latestGeneration != presentedGeneration,
            let recordingStatus
        {
            return recordingStatus
        }
        switch lifecycle.attempt?.stage {
        case .failed, .saveFailed: return .failed
        case .saved: return fileName.map { .saved(fileName: $0) } ?? .stored
        default: return recordingStatus
        }
    }

    var recordingSummary: String {
        if status == .failed { return localizedAppText("captures.save_failed") }
        if isFinishing { return localizedAppText("captures.saving") }
        return localizedAppText(notificationCount > 0 ? "captures.receiving" : "captures.waiting")
    }

    var canAnnotate: Bool { activeGeneration != nil && !isFinishing }

    private struct StartContext {
        let device: CaptureDeviceIdentity?
        let description: String?
    }
    private var pendingStart: StartContext?
    @ObservationIgnored private var acceptedStarts: [CaptureGeneration: StartContext] = [:]

    /// Stages presentation metadata only. The session's Rust owner admits the operation.
    func requestStart(device: CaptureDeviceIdentity?, description: String?, operation: () -> Bool) -> Bool {
        pendingStart = StartContext(device: device, description: description)
        let accepted = operation()
        if accepted, let context = pendingStart,
            let attempt = sessionState.captureLifecycleSnapshot().attempt
        {
            acceptedStarts[CaptureGeneration(rawValue: attempt.generation.value)] = context
        }
        pendingStart = nil
        return accepted
    }

    @ObservationIgnored private var recordings: [CaptureGeneration: Recording] = [:]

    private struct Recording {
        let requestedFileURL: URL
        let startedAt: Date
        var device: CaptureDeviceIdentity?
        let isUserInitiated: Bool
        var progress: CaptureProgress?
    }

    func updateDevice(_ row: DevicePickerRow) {
        guard device?.platformIdentifier == row.id else { return }
        device = CaptureDeviceIdentity(row: row)
        if let activeGeneration { recordings[activeGeneration]?.device = device }
    }

    private func resetRecordingPresentation() {
        recordingStatus = nil
        progress = nil
        annotationError = nil
        activeLabels.removeAll()
        settingsObservations.removeAll()
        researchDraft.reset()
        settingsError = nil
        fileName = nil
        notificationCount = 0
        label = nil
        deviceKind = nil
        device = nil
        isUserInitiated = false
    }

    func refreshSettingsObservations() {
        guard let generation = activeGeneration else { return }
        do {
            settingsObservations = try readSettingsObservations(generation)
            settingsError = nil
        } catch { settingsError = localizedAppText("capture.settings.load_error") }
    }

    func settingsReadback(requests: [MobileCaptureResearchReadbackRequestDto]) async throws
        -> [MobileCaptureResearchReadbackDto]
    {
        guard let generation = activeGeneration else { throw MobileCaptureResearchError.NotRecording }
        let result = try await readSettingsReadback(generation, requests)
        guard activeGeneration == generation else { throw MobileCaptureResearchError.NotRecording }
        return result
    }

    func recordSettingsObservation(_ draft: MobileCaptureSettingsDraftDto) async throws {
        guard canAnnotate, let generation = activeGeneration else { throw MobileCaptureResearchError.NotRecording }
        let saved = try await saveSettingsObservation(generation, draft)
        guard activeGeneration == generation else { return }
        settingsObservations.append(saved)
        settingsError = nil
    }

    func startLabel(_ next: CaptureQuickLabel) {
        guard !activeLabels.contains(next), changeLabel(.start(label: next.domainLabel)) else { return }
        label = next.title
        recordingStatus = .labelStarted(label: next.title, notificationCount: notificationCount, fileName: fileName)
    }

    func stopLabel(_ previous: CaptureQuickLabel) {
        guard activeLabels.contains(previous), changeLabel(.stop(label: previous.domainLabel)) else { return }
        label = previous.title
        recordingStatus = .labelStopped(label: previous.title, notificationCount: notificationCount, fileName: fileName)
    }

    private func changeLabel(_ action: MobileCaptureLabelActionDto) -> Bool {
        guard canAnnotate, let generation = activeGeneration else { return false }
        do {
            let active = try changeCaptureLabel(generation, action)
            activeLabels = Set(CaptureQuickLabel.allCases.filter { active.contains($0.domainLabel) })
            annotationError = nil
            return true
        } catch {
            annotationError = error as? MobileCaptureAnnotationError ?? .WriterFailed
            return false
        }
    }

    var annotationErrorText: String? {
        switch annotationError {
        case .CapacityReached: localizedAppText("captures.labels_full")
        case .NotRecording: localizedAppText("captures.labels_not_recording")
        case .AdmissionLost, .WriterFailed: localizedAppText("captures.labels_failed")
        case nil: nil
        }
    }

    func dismissAnnotationError() {
        annotationError = nil
    }

    func clearLabels() {
        // The writer closes persisted intervals at finalization. This resets only the projection.
        activeLabels.removeAll()
        annotationError = nil
    }

    func flush() async -> Bool {
        let flushed = await flushOperation()
        apply(.lifecycle(sessionState.captureLifecycleSnapshot()))
        return flushed
    }

    func finish() async -> Bool {
        guard let generation = activeGeneration, isManualCapture else { return false }
        if finishRequestGeneration == generation, let finishTask { return await finishTask.value }

        finishRequestGeneration = generation
        isFinishRequested = true
        let task = Task { @MainActor in await finishOperation() }
        finishTask = task
        let succeeded = await task.value
        let snapshot = sessionState.captureLifecycleSnapshot()
        lifecycle = snapshot

        guard finishRequestGeneration == generation else { return false }
        finishTask = nil
        finishRequestGeneration = nil
        isFinishRequested = false
        return succeeded && snapshot.attempt?.generation.value == generation.rawValue
    }

    func apply(_ event: CaptureEvent) {
        switch event {
        case let .lifecycle(snapshot):
            lifecycle = snapshot
        case let .started(generation, fileURL):
            guard generation == activeGeneration else { return }
            if finishRequestGeneration != generation { isFinishRequested = false }
            let context = acceptedStarts.removeValue(forKey: generation) ?? pendingStart
            resetRecordingPresentation()
            presentedGeneration = generation
            device = context?.device
            deviceKind = context?.description
            isUserInitiated = lifecycle.attempt?.origin == .manual
            pendingStart = nil
            fileName = fileURL.lastPathComponent
            notificationCount = 0
            progress = nil
            label = nil
            clearLabels()
            recordings[generation] = Recording(
                requestedFileURL: fileURL, startedAt: .now, device: device,
                isUserInitiated: isUserInitiated
            )
            recordingStatus = .recordingLocally(fileName: fileURL.lastPathComponent)
        case let .notificationRecorded(generation):
            guard generation == activeGeneration else { return }
            notificationCount += 1
            updateRecordingStatus()
        case let .progress(generation, progress):
            guard generation == activeGeneration else { return }
            self.progress = progress
            recordings[generation]?.progress = progress
            recordings[generation]?.device = device
            notificationCount = Int(clamping: progress.notificationCount)
            updateRecordingStatus()
        case let .finished(generation, fileURL, artifactId):
            complete(
                generation, outcome: .saved, fileURL: fileURL,
                submissionSource: artifactId.map { .recordedArtifact(id: $0) }
            )
        case let .databaseFinished(generation, outcome):
            guard case let .databaseFinished(liveCaptureId, integrity, jsonlExport, _) = outcome else { return }
            let artifact: MobileSavedCaptureArtifactDto?
            switch jsonlExport {
            case let .available(saved): artifact = saved
            case .notAttempted, .failed: artifact = nil
            }
            let captureOutcome: CaptureArtifact.Outcome
            switch integrity {
            case .complete:
                captureOutcome = artifact == nil ? .storedWithoutExport : .saved
            case let .incomplete(droppedMessages):
                captureOutcome = .incomplete(droppedMessages: droppedMessages)
            case .unknown:
                captureOutcome = .integrityUnknown
            }
            complete(
                generation,
                outcome: captureOutcome,
                fileURL: artifact.map { URL(fileURLWithPath: $0.path) },
                submissionSource: .liveCapture(id: liveCaptureId)
            )
        case let .failed(generation):
            complete(generation, outcome: .failed, fileURL: nil, preserveRequestedFile: true)
        }
    }

    private func updateRecordingStatus() {
        recordingStatus = .recording(label: label, notificationCount: notificationCount, fileName: fileName)
    }

    private func complete(
        _ generation: CaptureGeneration,
        outcome: CaptureArtifact.Outcome,
        fileURL: URL?,
        submissionSource: MobileCaptureSubmissionSourceDto? = nil,
        preserveRequestedFile: Bool = false
    ) {
        if let recording = recordings.removeValue(forKey: generation) {
            completed.insert(
                CaptureArtifact(
                    id: generation,
                    fileURL: fileURL ?? (preserveRequestedFile ? recording.requestedFileURL : nil),
                    startedAt: recording.startedAt,
                    device: recording.device, isUserInitiated: recording.isUserInitiated,
                    progress: recording.progress, outcome: outcome, submissionSource: submissionSource
                ), at: 0)
        }
        guard generation == latestGeneration else { return }
        guard generation == presentedGeneration else { return }
        if outcome == .failed {
            recordingStatus = .failed
        } else {
            fileName = fileURL?.lastPathComponent
            recordingStatus = fileURL.map { .saved(fileName: $0.lastPathComponent) } ?? .stored
        }
        // Finish admission stays closed until an accepted new recording starts.
        clearLabels()
    }
}

/// Presentation and off-main calls only; Rust owns history, completeness and export contents.
@MainActor
@Observable
final class CaptureLibraryModel {
    typealias Query = @Sendable (MobileLiveCaptureHistoryCursorDto?) async throws -> MobileLiveCaptureHistoryPageDto
    @ObservationIgnored private let query: Query

    init(
        query: @escaping Query = { cursor in
            let database = try await RustPersistenceStore.open()
            return try await Task.detached(priority: .userInitiated) {
                try database.listLiveCaptureHistory(cursor: cursor, limit: 50)
            }.value
        }
    ) {
        self.query = query
    }

    private(set) var captures: [MobileLiveCaptureHistoryEntryDto] = []
    private(set) var isLoading = false
    private(set) var error: String?
    private(set) var hasMore = false
    @ObservationIgnored private var cursor: MobileLiveCaptureHistoryCursorDto?
    @ObservationIgnored private var refreshPending = false

    func refresh() async {
        guard !isLoading else {
            refreshPending = true
            return
        }
        await load(reset: true)
    }

    func loadMore() async {
        guard !isLoading, hasMore else { return }
        await load(reset: false)
    }

    private func load(reset: Bool) async {
        isLoading = true
        defer { isLoading = false }
        var reset = reset
        repeat {
            refreshPending = false
            if reset { cursor = nil }
            do {
                let page = try await query(cursor)
                captures = reset ? page.captures : captures + page.captures
                cursor = page.nextCursor
                hasMore = page.nextCursor != nil
                error = nil
            } catch {
                self.error = error.localizedDescription
            }
            // One pending native refresh replaces any number of requests during this query.
            reset = true
        } while refreshPending
    }
}

@MainActor
@Observable
final class CaptureExportModel {
    typealias SourceExport = @MainActor (URL) async throws -> URL
    typealias CaptureExport = @MainActor (String) async throws -> URL

    private(set) var fileURL: URL?
    private(set) var isExporting = false
    private(set) var error: String?
    private let sourceExport: SourceExport
    private let captureExport: CaptureExport

    init(
        sourceExport: SourceExport? = nil,
        captureExport: CaptureExport? = nil
    ) {
        self.sourceExport = sourceExport ?? Self.exportSourceFile
        self.captureExport = captureExport ?? Self.exportDatabaseCapture
    }

    func export(source: URL) async {
        guard !isExporting, fileURL == nil else { return }
        isExporting = true
        error = nil
        defer { isExporting = false }
        do {
            fileURL = try await sourceExport(source)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func export(id: String) async {
        guard !isExporting, fileURL == nil else { return }
        isExporting = true
        error = nil
        defer { isExporting = false }
        do {
            fileURL = try await captureExport(id)
        } catch {
            self.error = error.localizedDescription
        }
    }

    private static func exportSourceFile(_ source: URL) async throws -> URL {
        let result = try await Task.detached(priority: .userInitiated) {
            let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
                "CaptureExports", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let ext = source.pathExtension == "pevcap" ? "pevcap" : "jsonl"
            let destination = directory.appendingPathComponent("capture-\(UUID().uuidString).\(ext)")
            try FileManager.default.copyItem(at: source, to: destination)
            return destination
        }.value
        return result
    }

    private static func exportDatabaseCapture(_ id: String) async throws -> URL {
        let database = try await RustPersistenceStore.open()
        let result = try await Task.detached(priority: .userInitiated) {
            let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
                "CaptureExports", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let path = directory.appendingPathComponent("capture-\(UUID().uuidString).jsonl").path
            return try database.exportLiveCapture(liveCaptureId: id, path: path)
        }.value
        return URL(fileURLWithPath: result.path)
    }
}
