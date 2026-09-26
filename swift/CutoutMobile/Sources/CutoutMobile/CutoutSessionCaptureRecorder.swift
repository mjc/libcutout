import CutoutMobileFFI
import Foundation
import Synchronization

struct CaptureMusicContext: Equatable {
    private(set) var current: MobilePevcapMusicEventDto?

    mutating func update(_ observation: MobilePevcapMusicEventDto?) {
        current = observation
    }

    mutating func take() -> MobilePevcapMusicEventDto? {
        defer { current = nil }
        return current
    }

    mutating func reset() {
        current = nil
    }
}

struct CaptureWriterCompletion {
    let generation: CaptureGeneration
    let fileURL: URL?
    let succeeded: Bool
    let databasePublicationSucceeded: Bool?
}

struct CaptureLocationWriteResult {
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
    var currentMusicObservation: MobilePevcapMusicEventDto? { get }
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
    func resetMusicContext()
    func addAnnotation(_ annotation: String) -> MobileCaptureWriteOutcomeDto
    func changeLabel(_ action: MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto]
    func flushWriter() -> Bool
    @discardableResult func publishProgress() -> CaptureProgress
    func publishFailure()
    func writerStatus() -> MobileCaptureWriterStatusDto?
    func recordNotification(
        characteristic: BluetoothUuid,
        service: BluetoothUuid,
        bytes: Data,
        telemetry: RawTelemetryReadback?
    ) -> MobileCaptureWriteOutcomeDto
    func recordLocationUpdate(_ update: PhoneLocationUpdate) -> CaptureLocationWriteResult
    func recordLinkUp(maxWriteLength: UInt16?) -> MobileCaptureWriteOutcomeDto
    func recordLinkDown() -> MobileCaptureWriteOutcomeDto
    func recordMusicObservation(_ observation: MobilePevcapMusicEventDto?) -> MobileCaptureWriteOutcomeDto
    func updateMusicPolicy(_ policy: MobileMusicHistoryPolicyDto) -> MobileCaptureWriteOutcomeDto
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
    func finish(publishesResult: Bool, priorWriteSucceeded: Bool)
    func elapsedMilliseconds() -> UInt64
    func elapsedMilliseconds(since startedAt: MonotonicMilliseconds) -> UInt64
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
    private var musicContext = CaptureMusicContext()
    private var musicHistoryPolicy = MobileMusicHistoryPolicyDto.disabled
    private var captureOrigin = MobileCaptureOriginDto.automatic
    private var advertisedName: String?

    #if DEBUG
        var finishWriterGate: (() -> Void)?
    #endif

    var currentGeneration: CaptureGeneration? { presentation.currentGeneration }
    var activeFileURL: URL? { fileURL }
    var hasWriter: Bool { builder != nil }
    var currentMusicObservation: MobilePevcapMusicEventDto? { musicContext.current }

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
        locationWriter.withLock { $0 = nil }
        presentation.begin(generation: generation)
        captureOrigin = origin
        self.advertisedName = advertisedName
        let startedAt = clock.now()
        self.startedAt = startedAt
        notificationCount = 0

        let url = directory.appendingPathComponent(
            "cutout-btle-capture-\(Int(wallClock().timeIntervalSince1970))-\(UUID().uuidString).jsonl"
        )
        let builder = MobilePevcapCaptureBuilder(
            wallClockStartUnixMs: MobileWallClockUnixMillisDto(
                milliseconds: UInt64(wallClock().timeIntervalSince1970 * 1_000)
            ),
            platformId: platformIdentifier,
            writeLimit: MobileTransportWriteLimitDto(bytes: 23)
        )
        if let database {
            builder.setDatabase(database: database)
        }
        _ = builder.setMusicHistoryPolicy(policy: musicHistoryPolicy)
        _ = builder.setCaptureStartMonotonicMs(monotonicMs: startedAt.rawValue)
        advertisedServices.forEach { _ = builder.addAdvertisedService(service: $0.bytes) }
        [
            "source=ios-app",
            "capture_reason=\(reason)",
            "capture_privacy=private",
            "capture_evidence=\(evidence)",
        ].forEach { _ = builder.addAnnotation(annotation: $0) }
        annotations.forEach { _ = builder.addAnnotation(annotation: sanitizedPevcapAnnotation($0)) }
        _ = builder.setMusicContext(music: musicContext.current)
        self.builder = builder
        fileURL = url
        guard builder.startWriter(path: url.path) else {
            self.builder = nil
            fileURL = nil
            self.startedAt = nil
            presentation.end()
            return false
        }
        locationWriter.withLock { $0 = ActiveCaptureLocationWriter(generation: generation, builder: builder) }
        musicContext.reset()
        return true
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

    func resetMusicContext() { musicContext.reset() }

    func addAnnotation(_ annotation: String) -> MobileCaptureWriteOutcomeDto {
        builder?.addAnnotation(annotation: annotation) ?? .accepted
    }

    func changeLabel(_ action: MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto] {
        guard let builder else { throw MobileCaptureAnnotationError.NotRecording }
        return try builder.changeLabel(action: action)
    }

    func flushWriter() -> Bool { builder?.flushWriter() ?? false }

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
            writerError: status?.failed == true ? status?.lastError : nil
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
        telemetry: RawTelemetryReadback?
    ) -> MobileCaptureWriteOutcomeDto {
        if let builder {
            let outcome = builder.recordNotificationWithContext(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: elapsedMilliseconds()),
                characteristic: characteristic.bytes,
                service: service.bytes,
                bytes: bytes,
                telemetry: telemetry?.dto,
                phoneLocation: nil
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
            monotonicMs: MobileMonotonicMillisDto(milliseconds: elapsedMilliseconds()),
            maxWriteLen: maxWriteLength.map(MobileTransportWriteLimitDto.init(bytes:))
        )
    }

    func recordLinkDown() -> MobileCaptureWriteOutcomeDto {
        builder?.recordLinkDown(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: elapsedMilliseconds())
        ) ?? .accepted
    }

    func recordMusicObservation(_ observation: MobilePevcapMusicEventDto?) -> MobileCaptureWriteOutcomeDto {
        musicContext.update(observation)
        guard let builder else { return .accepted }
        guard let observation else {
            _ = builder.setMusicContext(music: nil)
            return .accepted
        }
        return builder.recordMusicEvent(music: observation)
    }

    func updateMusicPolicy(_ policy: MobileMusicHistoryPolicyDto) -> MobileCaptureWriteOutcomeDto {
        musicHistoryPolicy = policy
        _ = builder?.setMusicHistoryPolicy(policy: policy)
        return .accepted
    }

    func setResolvedIdentity(
        _ identity: MobileResolvedIdentityDto,
        evidence: String?,
        detail: String?
    ) -> MobileCaptureWriteOutcomeDto {
        guard let builder else { return .accepted }
        let identityOutcome = builder.setResolvedIdentity(identity: identity)
        guard case .accepted = identityOutcome else { return identityOutcome }
        if let evidence {
            let outcome = builder.addAnnotation(annotation: pevcapAnnotation(key: "resolved_evidence", value: evidence))
            guard case .accepted = outcome else { return outcome }
        }
        if let detail {
            let outcome = builder.addAnnotation(annotation: pevcapAnnotation(key: "resolved_detail", value: detail))
            guard case .accepted = outcome else { return outcome }
        }
        return .accepted
    }

    func addGattFingerprint(_ fingerprint: MobileGattFingerprintDto) -> MobileCaptureWriteOutcomeDto {
        builder?.addGattFingerprint(fingerprint: fingerprint) ?? .accepted
    }

    func makeWriteReceiptRecorder(
        channel: BluetoothUuid,
        bytes: Data,
        writeID: UInt64
    ) -> (CoreBluetoothWriteDisposition) -> MobileCaptureWriteOutcomeDto {
        guard let builder, let startedAt else { return { _ in .accepted } }
        return { [weak self, builder] disposition in
            let captured: MobilePevcapWriteDispositionDto
            switch disposition {
            case .queued: captured = .queued
            case .submitted: captured = .submitted
            case .rejected: captured = .rejected
            case .cancelled: captured = .cancelled
            }
            let elapsed = self?.elapsedMilliseconds(since: startedAt) ?? 0
            return builder.recordWriteWithoutResponseReceipt(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: elapsed),
                characteristic: channel.bytes,
                bytes: bytes,
                writeId: writeID,
                disposition: captured
            )
        }
    }

    func finish(publishesResult: Bool, priorWriteSucceeded: Bool) {
        musicContext.reset()
        guard let builder else { return }
        let generation = currentGeneration ?? .legacy
        let fileURL = self.fileURL
        let origin = captureOrigin
        let advertisedName = self.advertisedName
        locationWriter.withLock { $0 = nil }
        self.builder = nil
        self.fileURL = nil
        startedAt = nil
        self.advertisedName = nil
        presentation.end()

        let completionHandler = onWriterCompletion
        let database = database
        let wallClock = wallClock
        #if DEBUG
            let finishWriterGate = finishWriterGate
        #endif
        let finish = DispatchWorkItem {
            #if DEBUG
                finishWriterGate?()
            #endif
            let writerSucceeded = builder.finishWriter()
            let artifact = writerSucceeded ? builder.completedArtifact() : nil
            guard publishesResult else { return }
            let databasePublicationSucceeded: Bool?
            if let database, artifact != nil {
                let milliseconds = wallClock().timeIntervalSince1970 * 1_000
                if milliseconds.isFinite, milliseconds > 0, milliseconds < Double(UInt64.max) {
                    do {
                        _ = try database.retainFinishedCapture(
                            builder: builder,
                            origin: origin,
                            advertisedName: advertisedName,
                            publishedAt: MobileWallClockUnixMillisDto(
                                milliseconds: UInt64(milliseconds.rounded(.down))
                            )
                        )
                        databasePublicationSucceeded = true
                    } catch {
                        databasePublicationSucceeded = false
                    }
                } else {
                    databasePublicationSucceeded = false
                }
            } else {
                databasePublicationSucceeded = nil
            }
            completionHandler(
                CaptureWriterCompletion(
                    generation: generation,
                    fileURL: artifact.map { URL(fileURLWithPath: $0.path) } ?? fileURL,
                    succeeded: priorWriteSucceeded && artifact != nil,
                    databasePublicationSucceeded: databasePublicationSucceeded
                ))
        }
        DispatchQueue.global(qos: .utility).async(execute: finish)
    }
}
