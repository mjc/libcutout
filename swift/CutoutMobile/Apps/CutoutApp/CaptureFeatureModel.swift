import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Presentation of a writer-owned artifact, not a second capture store.
/// Relaunch history must come from Rust storage; this is only the live callback projection.
struct CaptureArtifact: Identifiable, Equatable {
    enum Outcome { case saved, failed }

    let id: CaptureGeneration
    let fileURL: URL
    let startedAt: Date
    let device: CaptureDeviceIdentity?
    let isUserInitiated: Bool
    let progress: CaptureProgress?
    let outcome: Outcome
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

    init(sessionState: CutoutSessionStateHandle) {
        self.sessionState = sessionState
        lifecycle = sessionState.captureLifecycleSnapshot()
    }

    private(set) var lifecycle = MobileCaptureLifecycleSnapshotDto(attempt: nil, canStart: true, canPair: true)
    private var recordingStatus: CaptureStatus?
    private var presentedGeneration: CaptureGeneration?
    private(set) var progress: CaptureProgress?
    private(set) var activeLabels = Set<CaptureQuickLabel>()
    private(set) var annotationError: MobileCaptureAnnotationError?
    private(set) var deviceKind: String?
    private(set) var device: CaptureDeviceIdentity?
    private(set) var isUserInitiated = false
    private(set) var fileName: String?
    private(set) var notificationCount = 0
    private(set) var label: String?
    private(set) var completed: [CaptureArtifact] = []

    var latestGeneration: CaptureGeneration? { lifecycle.attempt.map { CaptureGeneration(rawValue: $0.generation.value) } }
    var activeGeneration: CaptureGeneration? {
        switch lifecycle.attempt?.stage {
        case .recording, .saving, .saveFailed, .finalizing: latestGeneration
        default: nil
        }
    }
    var isFinishing: Bool { lifecycle.attempt?.stage == .saving || lifecycle.attempt?.stage == .finalizing }
    var isManualCapture: Bool { lifecycle.attempt?.origin == .manual && activeGeneration != nil }
    var status: CaptureStatus? {
        // A rejected startup belongs to the setup error, not to the previous artifact.
        if lifecycle.attempt?.stage == .failed, latestGeneration != presentedGeneration,
           let recordingStatus { return recordingStatus }
        switch lifecycle.attempt?.stage {
        case .failed, .saveFailed: return .failed
        case .saved: return fileName.map { .saved(fileName: $0) }
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
           let attempt = sessionState.captureLifecycleSnapshot().attempt {
            acceptedStarts[CaptureGeneration(rawValue: attempt.generation.value)] = context
        }
        pendingStart = nil
        return accepted
    }

    @ObservationIgnored private var recordings: [CaptureGeneration: Recording] = [:]

    private struct Recording {
        let fileURL: URL
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
        fileName = nil
        notificationCount = 0
        label = nil
        deviceKind = nil
        device = nil
        isUserInitiated = false
    }

    func startLabel(
        _ next: CaptureQuickLabel,
        record: (CaptureGeneration, MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto]
    ) {
        guard !activeLabels.contains(next), changeLabel(.start(label: next.domainLabel), record: record) else { return }
        label = next.title
        recordingStatus = .labelStarted(label: next.title, notificationCount: notificationCount, fileName: fileName)
    }

    func stopLabel(
        _ previous: CaptureQuickLabel,
        record: (CaptureGeneration, MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto]
    ) {
        guard activeLabels.contains(previous), changeLabel(.stop(label: previous.domainLabel), record: record) else { return }
        label = previous.title
        recordingStatus = .labelStopped(label: previous.title, notificationCount: notificationCount, fileName: fileName)
    }

    private func changeLabel(
        _ action: MobileCaptureLabelActionDto,
        record: (CaptureGeneration, MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto]
    ) -> Bool {
        guard canAnnotate, let generation = activeGeneration else { return false }
        do {
            let active = try record(generation, action)
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
        case .WriterFailed: localizedAppText("captures.labels_failed")
        case .CaptureLimitReached: localizedAppText("captures.labels_limit_reached")
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

    func apply(_ event: CaptureEvent) {
        switch event {
        case let .lifecycle(snapshot):
            lifecycle = snapshot
        case let .started(generation, fileURL):
            guard generation == activeGeneration else { return }
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
                fileURL: fileURL, startedAt: .now, device: device,
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
        case let .finished(generation, fileURL):
            complete(generation, outcome: .saved, fileURL: fileURL)
        case let .failed(generation):
            complete(generation, outcome: .failed, fileURL: nil)
        }
    }

    private func updateRecordingStatus() {
        recordingStatus = .recording(label: label, notificationCount: notificationCount, fileName: fileName)
    }

    private func complete(_ generation: CaptureGeneration, outcome: CaptureArtifact.Outcome, fileURL: URL?) {
        if let recording = recordings.removeValue(forKey: generation) {
            completed.insert(CaptureArtifact(
                id: generation, fileURL: fileURL ?? recording.fileURL, startedAt: recording.startedAt,
                device: recording.device, isUserInitiated: recording.isUserInitiated,
                progress: recording.progress, outcome: outcome
            ), at: 0)
        }
        guard generation == latestGeneration else { return }
        guard generation == presentedGeneration else { return }
        if outcome == .saved, let fileURL {
            fileName = fileURL.lastPathComponent
            recordingStatus = .saved(fileName: fileURL.lastPathComponent)
        } else {
            recordingStatus = .failed
        }
        // Finish admission stays closed until an accepted new recording starts.
        clearLabels()
    }
}
