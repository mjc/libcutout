import CutoutMobileFFI
import Foundation
import Synchronization

struct CaptureWriterCompletion {
    let generation: CaptureGeneration
    let completion: MobileCaptureCompletionDto
    let priorWriteOutcome: MobileCaptureWriteOutcomeDto
    var outcome: MobileCaptureFinishOutcomeDto { completion.finish }
    var databasePublicationSucceeded: Bool? { completion.databasePublicationSucceeded }

    var artifact: MobileSavedCaptureArtifactDto? {
        switch outcome {
        case let .artifactAvailable(artifact): artifact
        case let .databaseFinished(_, _, .available(artifact: artifact), _): artifact
        case .notStarted, .finalizing, .databaseFinished, .failed: nil
        }
    }

    var fileURL: URL? { artifact.map { URL(fileURLWithPath: $0.path) } }

    var succeeded: Bool {
        captureCompletionSucceeded(completion: completion, priorWriteOutcome: priorWriteOutcome)
    }
}

struct CaptureLocationWriteResult: Sendable {
    let generation: CaptureGeneration?
    let outcome: MobileCaptureWriteOutcomeDto
}

private struct ActiveCaptureLocationWriter: Sendable {
    let generation: CaptureGeneration
    let builder: MobilePevcapCaptureBuilder
}

protocol CutoutSessionCaptureRecording: AnyObject {
    var currentGeneration: CaptureGeneration? { get }
    var activeFileURL: URL? { get }
    var hasWriter: Bool { get }
    #if DEBUG
        var finishWriterGate: (() -> Void)? { get set }
    #endif
    func start(
        generation: CaptureGeneration,
        platformIdentifier: String,
        advertisedServices: [BluetoothUuid],
        directory: URL,
        reason: String,
        annotations: [String],
        evidence: String,
        origin: MobileCaptureOriginDto,
        advertisedName: String?
    ) -> Bool
    func startSynthetic(generation: CaptureGeneration, fileURL: URL, progress: CaptureProgress)
    func finishSynthetic()
    func publishStarted()
    func addAnnotation(_ annotation: String) -> MobileCaptureWriteOutcomeDto
    func changeLabel(_ action: MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto]
    func settingsObservationWriter() throws -> @Sendable (MobileCaptureSettingsDraftDto) throws ->
        MobileCaptureSettingsObservationDto
    func settingsObservations() throws -> [MobileCaptureSettingsObservationDto]
    func settingsReadbackReader() throws -> @Sendable (String, UInt64) throws -> [MobileCaptureResearchDecodedValueDto]
    func flushWriter() -> MobileCaptureFlushOutcomeDto
    func musicMetadataCleanupBarrier() -> @Sendable () -> Bool
    @discardableResult func publishProgress() -> CaptureProgress
    func publishFailure()
    func writerStatus() -> MobileCaptureWriterStatusDto?
    func recordNotification(
        characteristic: BluetoothUuid,
        service: BluetoothUuid,
        bytes: Data,
        telemetry: RawTelemetryReadback?,
        semanticTelemetry: MobileTelemetrySnapshotDto?,
        receivedAt: MonotonicMilliseconds,
        evidence: MobileCaptureNotificationEvidenceDto
    ) -> MobileCaptureWriteOutcomeDto
    func recordLocationUpdate(_ update: PhoneLocationUpdate) -> CaptureLocationWriteResult
    func recordLinkUp(maxWriteLength: UInt16?) -> MobileCaptureWriteOutcomeDto
    func recordLinkDown() -> MobileCaptureWriteOutcomeDto
    func setResolvedIdentity(
        _ identity: MobileResolvedIdentityDto,
        evidence: String?,
        detail: String?
    ) -> MobileCaptureWriteOutcomeDto
    func addGattFingerprint(_ fingerprint: MobileGattFingerprintDto) -> MobileCaptureWriteOutcomeDto
    func makeWriteReceiptRecorder(
        channel: BluetoothUuid,
        bytes: Data,
        writeID: UInt64
    ) -> (CoreBluetoothWriteDisposition) -> MobileCaptureWriteOutcomeDto
    func finish(publishesResult: Bool, priorWriteOutcome: MobileCaptureWriteOutcomeDto)
    func elapsedMilliseconds() -> UInt64
    func elapsedMilliseconds(since startedAt: MonotonicMilliseconds) -> UInt64
}

extension CutoutSessionCaptureRecording {
    func settingsObservationWriter() throws -> @Sendable (MobileCaptureSettingsDraftDto) throws ->
        MobileCaptureSettingsObservationDto
    {
        throw MobileCaptureResearchError.NotRecording
    }
    func settingsObservations() throws -> [MobileCaptureSettingsObservationDto] { [] }
    func settingsReadbackReader() throws -> @Sendable (String, UInt64) throws -> [MobileCaptureResearchDecodedValueDto]
    {
        throw MobileCaptureResearchError.NotRecording
    }
    func musicMetadataCleanupBarrier() -> @Sendable () -> Bool { { true } }
}

/// Owns PEVCAP writer state and capture-local timing; Rust/Core retain lifecycle and failure policy.
final class CutoutSessionCaptureRecorder: CutoutSessionCaptureRecording {
    private let clock: MonotonicClock
    private let wallClock: () -> Date
    private let database: RideDatabaseHandle?
    private let locationWriter = Mutex<ActiveCaptureLocationWriter?>(nil)
    private let publish: (CaptureEvent) -> Void
    private let onWriterCompletion: (CaptureWriterCompletion) -> Void
    private lazy var presentation = CutoutSessionCapturePresentation(publish: publish)
    private var startedAt: MonotonicMilliseconds?
    private var notificationCount: UInt64 = 0
    private var builder: MobilePevcapCaptureBuilder?
    private var fileURL: URL?
    private let writerCompletionGroup = DispatchGroup()
    private var captureOrigin = MobileCaptureOriginDto.automatic
    private var advertisedName: String?

    #if DEBUG
        var finishWriterGate: (() -> Void)?
    #endif

    var currentGeneration: CaptureGeneration? { presentation.currentGeneration }
    var activeFileURL: URL? { fileURL }
    var hasWriter: Bool { builder != nil }

    init(
        clock: MonotonicClock,
        wallClock: @escaping () -> Date,
        database: RideDatabaseHandle?,
        publish: @escaping (CaptureEvent) -> Void,
        onWriterCompletion: @escaping (CaptureWriterCompletion) -> Void
    ) {
        self.clock = clock
        self.wallClock = wallClock
        self.database = database
        self.publish = publish
        self.onWriterCompletion = onWriterCompletion
    }

    func start(
        generation: CaptureGeneration,
        platformIdentifier: String,
        advertisedServices: [BluetoothUuid],
        directory: URL,
        reason: String,
        annotations: [String],
        evidence: String,
        origin: MobileCaptureOriginDto,
        advertisedName: String?
    ) -> Bool {
        guard self.builder == nil else { return false }
        do {
            let prepared = try prepareCaptureWriter(
                request: MobileCaptureStartRequestDto(
                    wallClockUnixSeconds: wallClock().timeIntervalSince1970,
                    platformId: platformIdentifier,
                    advertisedServices: advertisedServices.map(\.bytes),
                    directoryPath: directory.path,
                    filenameNonce: UUID().uuidString,
                    source: "ios-app",
                    reason: reason,
                    evidence: evidence,
                    annotations: annotations,
                    origin: origin,
                    musicHistoryPolicy: .disabled
                ),
                database: database
            )
            let startedAt = clock.now()
            let admitted = try prepared.start(monotonicMs: startedAt.rawValue, musicContext: nil)
            let builder = admitted.builder
            let url = URL(fileURLWithPath: admitted.path)

            presentation.begin(generation: generation)
            captureOrigin = origin
            self.advertisedName = advertisedName
            self.startedAt = startedAt
            notificationCount = 0
            self.builder = builder
            fileURL = url
            locationWriter.withLock { $0 = ActiveCaptureLocationWriter(generation: generation, builder: builder) }
            return true
        } catch {
            return false
        }
    }

    func publishStarted() {
        guard let generation = currentGeneration, let fileURL else { return }
        publish(.started(generation: generation, fileURL: fileURL))
    }

    func startSynthetic(generation: CaptureGeneration, fileURL: URL, progress: CaptureProgress) {
        presentation.begin(generation: generation)
        self.fileURL = fileURL
        publish(.started(generation: generation, fileURL: fileURL))
        publish(.progress(generation: generation, progress))
    }

    func finishSynthetic() {
        guard let fileURL else { return }
        let generation = currentGeneration ?? .legacy
        self.fileURL = nil
        presentation.end()
        publish(.finished(generation: generation, fileURL: fileURL))
    }

    func addAnnotation(_ annotation: String) -> MobileCaptureWriteOutcomeDto {
        builder?.addAnnotation(annotation: annotation) ?? .accepted
    }

    func changeLabel(_ action: MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto] {
        guard let builder else { throw MobileCaptureAnnotationError.NotRecording }
        return try builder.changeLabel(action: action)
    }

    func settingsObservationWriter() throws -> @Sendable (MobileCaptureSettingsDraftDto) throws ->
        MobileCaptureSettingsObservationDto
    {
        guard let builder else { throw MobileCaptureResearchError.NotRecording }
        let recordedAt = clock.now().rawValue
        return { draft in try builder.recordSettingsObservation(draft: draft, monotonicMs: recordedAt) }
    }

    func settingsObservations() throws -> [MobileCaptureSettingsObservationDto] {
        guard let builder else { throw MobileCaptureResearchError.NotRecording }
        return try builder.settingsObservations()
    }

    func settingsReadbackReader() throws -> @Sendable (String, UInt64) throws -> [MobileCaptureResearchDecodedValueDto]
    {
        guard let builder else { throw MobileCaptureResearchError.NotRecording }
        return { profile, at in try builder.captureResearchReadback(profileId: profile, atMs: at) }
    }

    func flushWriter() -> MobileCaptureFlushOutcomeDto { builder?.flushWriterOutcome() ?? .rejected }

    /// Snapshot native writer ownership on the BLE queue; wait only on a background executor.
    func musicMetadataCleanupBarrier() -> @Sendable () -> Bool {
        let active = builder
        let completions = writerCompletionGroup
        return {
            completions.wait()
            guard active?.prepareMusicMetadataCleanup() ?? true else { return false }
            completions.wait()
            return true
        }
    }

    @discardableResult
    func publishProgress() -> CaptureProgress {
        let current = progress()
        presentation.publishProgress(current)
        return current
    }
    func publishFailure() { presentation.publishFailure() }
    func writerStatus() -> MobileCaptureWriterStatusDto? { builder?.writerStatus() }

    private func progress() -> CaptureProgress {
        let status = builder?.writerStatus()
        let attributes = fileURL.flatMap { try? FileManager.default.attributesOfItem(atPath: $0.path) }
        let fileSize = (attributes?[.size] as? NSNumber)?.uint64Value ?? 0
        let size = max(fileSize, status?.bytesWritten ?? 0)
        return CaptureProgress(
            elapsedMilliseconds: elapsedMilliseconds(),
            notificationCount: notificationCount,
            fileSizeBytes: size,
            queuedMessageCount: status?.queuedMessages ?? 0,
            writerError: status?.lastError,
            writerFailed: status?.failed ?? false,
            droppedMessageCount: status?.droppedMessages ?? 0
        )
    }

    func elapsedMilliseconds() -> UInt64 {
        guard let startedAt else { return 0 }
        return elapsedMilliseconds(since: startedAt)
    }

    func elapsedMilliseconds(since startedAt: MonotonicMilliseconds) -> UInt64 {
        clock.now().elapsed(since: startedAt).rawValue
    }

    func recordNotification(
        characteristic: BluetoothUuid,
        service: BluetoothUuid,
        bytes: Data,
        telemetry: RawTelemetryReadback?,
        semanticTelemetry: MobileTelemetrySnapshotDto?,
        receivedAt: MonotonicMilliseconds,
        evidence: MobileCaptureNotificationEvidenceDto
    ) -> MobileCaptureWriteOutcomeDto {
        if let builder {
            let outcome = builder.recordDecodedNotification(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: receivedAt.rawValue),
                characteristic: characteristic.bytes,
                service: service.bytes,
                bytes: bytes,
                telemetry: telemetry?.dto,
                semanticTelemetry: semanticTelemetry,
                evidence: evidence
            )
            if case .accepted = outcome { notificationCount += 1 }
            return outcome
        }
        notificationCount += 1
        return .accepted
    }

    func recordLocationUpdate(_ update: PhoneLocationUpdate) -> CaptureLocationWriteResult {
        locationWriter.withLock { active in
            guard let active else {
                return CaptureLocationWriteResult(generation: nil, outcome: .accepted)
            }
            guard let receiptWallClockUnixMs = unixMilliseconds(for: update.receiptWallClock) else {
                return CaptureLocationWriteResult(
                    generation: active.generation,
                    outcome: .rejected
                )
            }
            let outcome = active.builder.recordLocationSamples(
                receiptMonotonicMs: MobileMonotonicMillisDto(
                    milliseconds: update.receiptMonotonic.rawValue
                ),
                receiptWallClockUnixMs: MobileWallClockUnixMillisDto(
                    milliseconds: receiptWallClockUnixMs
                ),
                samples: update.samples
            )
            return CaptureLocationWriteResult(generation: active.generation, outcome: outcome)
        }
    }

    func recordLinkUp(maxWriteLength: UInt16?) -> MobileCaptureWriteOutcomeDto {
        guard let builder else { return .accepted }
        return builder.recordLinkUp(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: clock.now().rawValue),
            maxWriteLen: maxWriteLength.map(MobileTransportWriteLimitDto.init(bytes:))
        )
    }

    func recordLinkDown() -> MobileCaptureWriteOutcomeDto {
        builder?.recordLinkDown(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: clock.now().rawValue)
        ) ?? .accepted
    }

    func setResolvedIdentity(
        _ identity: MobileResolvedIdentityDto,
        evidence: String?,
        detail: String?
    ) -> MobileCaptureWriteOutcomeDto {
        builder?.updateResolvedIdentity(identity: identity, evidence: evidence, detail: detail) ?? .accepted
    }

    func addGattFingerprint(_ fingerprint: MobileGattFingerprintDto) -> MobileCaptureWriteOutcomeDto {
        builder?.addGattFingerprint(fingerprint: fingerprint) ?? .accepted
    }

    func makeWriteReceiptRecorder(
        channel: BluetoothUuid,
        bytes: Data,
        writeID: UInt64
    ) -> (CoreBluetoothWriteDisposition) -> MobileCaptureWriteOutcomeDto {
        guard let builder else { return { _ in .accepted } }
        let clock = self.clock
        return { [builder, clock] disposition in
            let captured: MobilePevcapWriteDispositionDto
            switch disposition {
            case .queued: captured = .queued
            case .submitted: captured = .submitted
            case .rejected: captured = .rejected
            case .cancelled: captured = .cancelled
            }
            return builder.recordWriteWithoutResponseReceipt(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: clock.now().rawValue),
                characteristic: channel.bytes,
                bytes: bytes,
                writeId: writeID,
                disposition: captured
            )
        }
    }

    func finish(publishesResult: Bool, priorWriteOutcome: MobileCaptureWriteOutcomeDto) {
        guard let builder else { return }
        let generation = currentGeneration ?? .legacy
        let origin = captureOrigin
        let advertisedName = self.advertisedName
        locationWriter.withLock { $0 = nil }
        self.builder = nil
        self.fileURL = nil
        startedAt = nil
        self.advertisedName = nil
        presentation.end()

        let completionHandler = onWriterCompletion
        let completions = writerCompletionGroup
        completions.enter()
        let wallClock = wallClock
        #if DEBUG
            let finishWriterGate = finishWriterGate
        #endif
        let finish = DispatchWorkItem {
            defer { completions.leave() }
            #if DEBUG
                finishWriterGate?()
            #endif
            let publication =
                publishesResult
                ? MobileCaptureHistoryPublicationDto(
                    origin: origin,
                    advertisedName: advertisedName,
                    publishedAtUnixMs: unixMilliseconds(for: wallClock()).map {
                        MobileWallClockUnixMillisDto(milliseconds: $0)
                    }
                )
                : nil
            let completion = builder.finishWriterAndPublishCapture(publication: publication)
            guard publishesResult else { return }
            completionHandler(
                CaptureWriterCompletion(
                    generation: generation,
                    completion: completion,
                    priorWriteOutcome: priorWriteOutcome
                ))
        }
        DispatchQueue.global(qos: .utility).async(execute: finish)
    }
}
