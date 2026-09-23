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
    var status: CaptureStatus?
    var progress: CaptureProgress?
    var isFinishing = false
    var activeLabels = Set<CaptureQuickLabel>()
    var labelState = MobileCaptureLabels()
    var deviceKind: String?
    var device: CaptureDeviceIdentity?
    var isUserInitiated = false
    var fileName: String?
    var latestGeneration: CaptureGeneration?
    var activeGeneration: CaptureGeneration?
    var notificationCount = 0
    var label: String?
    private(set) var completed: [CaptureArtifact] = []

    var recordingSummary: String {
        if status == .failed { return localizedAppText("captures.save_failed") }
        if isFinishing { return localizedAppText("captures.saving") }
        return localizedAppText(notificationCount > 0 ? "captures.receiving" : "captures.waiting")
    }

    var canAnnotate: Bool { activeGeneration != nil && !isFinishing }

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

    func reset() {
        status = nil
        progress = nil
        isFinishing = false
        labelState = MobileCaptureLabels()
        activeLabels.removeAll()
        fileName = nil
        activeGeneration = nil
        notificationCount = 0
        label = nil
        deviceKind = nil
        device = nil
        isUserInitiated = false
    }

    func startLabel(_ next: CaptureQuickLabel, annotate: (String) -> Void) {
        guard canAnnotate else { return }
        let changes = labelState.start(label: next.domainLabel)
        guard !changes.isEmpty else { return }
        changes.forEach(annotate)
        refreshLabels()
        label = next.title
        status = .labelStarted(label: next.title, notificationCount: notificationCount, fileName: fileName)
    }

    func stopLabel(_ previous: CaptureQuickLabel, annotate: (String) -> Void) {
        guard canAnnotate, let change = labelState.stop(label: previous.domainLabel) else { return }
        annotate(change)
        refreshLabels()
        label = previous.title
        status = .labelStopped(label: previous.title, notificationCount: notificationCount, fileName: fileName)
    }

    func clearLabels() {
        labelState.clear()
        refreshLabels()
    }

    private func refreshLabels() {
        let active = labelState.active()
        activeLabels = Set(CaptureQuickLabel.allCases.filter { active.contains($0.domainLabel) })
    }

    func apply(_ event: CaptureEvent) {
        switch event {
        case let .started(generation, fileURL):
            guard latestGeneration.map({ generation >= $0 }) ?? true else { return }
            latestGeneration = generation
            activeGeneration = generation
            fileName = fileURL.lastPathComponent
            notificationCount = 0
            progress = nil
            label = nil
            clearLabels()
            isFinishing = false
            recordings[generation] = Recording(
                fileURL: fileURL, startedAt: .now, device: device,
                isUserInitiated: isUserInitiated
            )
            status = .recordingLocally(fileName: fileURL.lastPathComponent)
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
        status = .recording(label: label, notificationCount: notificationCount, fileName: fileName)
    }

    private func complete(_ generation: CaptureGeneration, outcome: CaptureArtifact.Outcome, fileURL: URL?) {
        if let recording = recordings.removeValue(forKey: generation) {
            completed.insert(CaptureArtifact(
                id: generation, fileURL: fileURL ?? recording.fileURL, startedAt: recording.startedAt,
                device: recording.device, isUserInitiated: recording.isUserInitiated,
                progress: recording.progress, outcome: outcome
            ), at: 0)
        }
        guard generation == activeGeneration else { return }
        if outcome == .saved, let fileURL {
            fileName = fileURL.lastPathComponent
            status = .saved(fileName: fileURL.lastPathComponent)
        } else {
            status = .failed
        }
        activeGeneration = nil
        // Finish admission stays closed until an accepted new recording starts.
        clearLabels()
    }
}
