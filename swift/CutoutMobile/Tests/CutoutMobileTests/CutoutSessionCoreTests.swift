import CoreLocation
import CutoutMobileFFI
import Synchronization
import XCTest

@testable import CutoutMobile

#if canImport(CoreBluetooth)
    import CoreBluetooth
#endif

private final class CaptureRecorderSpy: CutoutSessionCaptureRecording {
    var currentGeneration: CaptureGeneration? = .legacy
    var activeFileURL: URL?
    var hasWriter = true
    var currentMusicObservation: MobilePevcapMusicEventDto?
    var recordLinkUpResult: MobileCaptureWriteOutcomeDto = .accepted
    #if DEBUG
        var finishWriterGate: (() -> Void)?
    #endif
    private(set) var musicObservations: [MobilePevcapMusicEventDto?] = []
    private(set) var finishCount = 0
    private(set) var publishFailureCount = 0
    private(set) var publishProgressCount = 0
    var flushOutcome: MobileCaptureFlushOutcomeDto = .flushed
    var currentWriterStatus: MobileCaptureWriterStatusDto?
    var onPublishFailure: (() -> Void)?

    func start(
        generation: CaptureGeneration,
        platformIdentifier _: String,
        advertisedServices _: [BluetoothUuid],
        directory _: URL,
        reason _: String,
        annotations _: [String],
        evidence _: String,
        origin _: MobileCaptureOriginDto,
        advertisedName _: String?
    ) -> Bool {
        currentGeneration = generation
        hasWriter = true
        return true
    }

    func startSynthetic(generation: CaptureGeneration, fileURL: URL, progress _: CaptureProgress) {
        currentGeneration = generation
        activeFileURL = fileURL
    }

    func finishSynthetic() { activeFileURL = nil }
    func publishStarted() {}
    func resetMusicContext() { currentMusicObservation = nil }
    func addAnnotation(_: String) -> MobileCaptureWriteOutcomeDto { .accepted }
    func changeLabel(_: MobileCaptureLabelActionDto) throws -> [MobileCaptureLabelDto] { [] }
    func flushWriter() -> MobileCaptureFlushOutcomeDto { flushOutcome }

    func publishProgress() -> CaptureProgress {
        publishProgressCount += 1
        return CaptureProgress(
            elapsedMilliseconds: 0,
            notificationCount: 0,
            fileSizeBytes: 0,
            queuedMessageCount: 0,
            writerError: currentWriterStatus?.lastError,
            writerFailed: currentWriterStatus?.failed ?? false,
            droppedMessageCount: currentWriterStatus?.droppedMessages ?? 0
        )
    }

    func publishFailure() {
        publishFailureCount += 1
        onPublishFailure?()
    }
    func writerStatus() -> MobileCaptureWriterStatusDto? { currentWriterStatus }

    private(set) var notificationReceipts: [MonotonicMilliseconds] = []
    private(set) var notificationEvidence: [MobileCaptureNotificationEvidenceDto] = []
    private(set) var resolvedIdentities: [MobileResolvedIdentityDto] = []

    func recordNotification(
        characteristic _: BluetoothUuid,
        service _: BluetoothUuid,
        bytes _: Data,
        telemetry _: RawTelemetryReadback?,
        semanticTelemetry _: MobileTelemetrySnapshotDto?,
        receivedAt: MonotonicMilliseconds,
        evidence: MobileCaptureNotificationEvidenceDto
    ) -> MobileCaptureWriteOutcomeDto {
        notificationReceipts.append(receivedAt)
        notificationEvidence.append(evidence)
        return .accepted
    }

    func recordLocationUpdate(_: PhoneLocationUpdate) -> CaptureLocationWriteResult {
        CaptureLocationWriteResult(generation: currentGeneration, outcome: .accepted)
    }

    func recordLinkUp(maxWriteLength _: UInt16?) -> MobileCaptureWriteOutcomeDto { recordLinkUpResult }
    func recordLinkDown() -> MobileCaptureWriteOutcomeDto { .accepted }
    func recordMusicObservation(_ observation: MobilePevcapMusicEventDto?) -> MobileCaptureWriteOutcomeDto {
        musicObservations.append(observation)
        currentMusicObservation = observation
        return .accepted
    }
    func updateMusicPolicy(_: MobileMusicHistoryPolicyDto) -> MobileCaptureWriteOutcomeDto { .accepted }

    func setResolvedIdentity(
        _ identity: MobileResolvedIdentityDto,
        evidence _: String?,
        detail _: String?
    ) -> MobileCaptureWriteOutcomeDto {
        resolvedIdentities.append(identity)
        return .accepted
    }

    func addGattFingerprint(_: MobileGattFingerprintDto) -> MobileCaptureWriteOutcomeDto { .accepted }

    func makeWriteReceiptRecorder(
        channel _: BluetoothUuid,
        bytes _: Data,
        writeID _: UInt64
    ) -> (CoreBluetoothWriteDisposition) -> MobileCaptureWriteOutcomeDto { { _ in .accepted } }

    func finish(publishesResult _: Bool, priorWriteOutcome _: MobileCaptureWriteOutcomeDto) {
        finishCount += 1
        hasWriter = false
        currentGeneration = nil
    }

    func elapsedMilliseconds() -> UInt64 { 0 }
    func elapsedMilliseconds(since _: MonotonicMilliseconds) -> UInt64 { 0 }
}

final class CutoutSessionCoreTests: XCTestCase {
    func testRideMapPublishedSnapshotRejectsOlderRevisionButAcceptsNewRide() {
        let summary = MobileRideMapSummaryDto(
            pointCount: 0,
            distanceMeters: 0,
            durationMilliseconds: 0
        )
        let current = MobileRideMapSnapshotDto(
            rideID: "ride-a",
            revision: 4,
            state: .active,
            summary: summary,
            segmentCount: 0,
            associatedVehicle: nil
        )
        let older = MobileRideMapSnapshotDto(
            rideID: "ride-a",
            revision: 3,
            state: .paused,
            summary: summary,
            segmentCount: 0,
            associatedVehicle: nil
        )
        let replacement = MobileRideMapSnapshotDto(
            rideID: "ride-b",
            revision: 1,
            state: .active,
            summary: summary,
            segmentCount: 0,
            associatedVehicle: nil
        )

        XCTAssertFalse(
            CutoutSessionRideMapPresentation.shouldPublishSnapshot(
                current: current,
                incoming: older
            )
        )
        XCTAssertTrue(
            CutoutSessionRideMapPresentation.shouldPublishSnapshot(
                current: current,
                incoming: replacement
            )
        )
    }

    func testLocationEffectsRouteUnchangedUpdateInOrderAndIsolateCaptureFailures() {
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_025,
            sourceTimestampUnixSeconds: nil,
            latitudeDegrees: 39.739_235_8,
            longitudeDegrees: -104.990_251,
            altitudeMeters: 1_609.344,
            horizontalAccuracyMeters: 0.8,
            verticalAccuracyMeters: 1.2,
            speedMetersPerSecond: 4.470_400_25,
            speedAccuracyMetersPerSecond: 0.25,
            courseDegrees: 271.5,
            courseAccuracyDegrees: 3.0
        )
        let update = PhoneLocationUpdate(
            receiptMonotonic: MonotonicMilliseconds(125),
            receiptWallClock: Date(timeIntervalSince1970: 1_700_000_000.125),
            samples: [sample]
        )
        let expectedOutcomes: [MobileCaptureWriteOutcomeDto] = [.accepted, .rejected, .failed]

        for (index, outcome) in expectedOutcomes.enumerated() {
            let observations = Mutex(
                (
                    events: [String](), capture: Optional<PhoneLocationUpdate>.none,
                    rideMap: Optional<PhoneLocationUpdate>.none
                ))
            let result = CaptureLocationWriteResult(
                generation: CaptureGeneration(rawValue: UInt64(index + 1)),
                outcome: outcome
            )
            let effects = CutoutSessionLocationEffects(
                recordCaptureUpdate: { received in
                    observations.withLock {
                        $0.events.append("capture")
                        $0.capture = received
                    }
                    return result
                },
                ingestRideMapUpdate: { received in
                    observations.withLock {
                        $0.events.append("ride-map")
                        $0.rideMap = received
                    }
                },
                handleCaptureResult: { received in
                    observations.withLock { $0.events.append("result") }
                    XCTAssertEqual(received.outcome, outcome)
                    XCTAssertEqual(received.generation, result.generation)
                }
            )
            let core = CutoutSessionCore(
                clock: MonotonicClock(),
                locationEffects: effects
            )

            core.handlePhoneLocationUpdate(update)

            let observed = observations.withLock { $0 }
            let captureUpdate = observed.capture
            let rideMapUpdate = observed.rideMap
            XCTAssertEqual(observed.events, ["capture", "result", "ride-map"])
            for received in [captureUpdate, rideMapUpdate].compactMap({ $0 }) {
                XCTAssertEqual(received.receiptMonotonic, update.receiptMonotonic)
                XCTAssertEqual(received.receiptWallClock, update.receiptWallClock)
                XCTAssertEqual(received.samples.count, update.samples.count)
                XCTAssertEqual(received.samples.first?.latitudeDegrees, sample.latitudeDegrees)
                XCTAssertEqual(received.samples.first?.longitudeDegrees, sample.longitudeDegrees)
            }
            XCTAssertNotNil(captureUpdate)
            XCTAssertNotNil(rideMapUpdate)
        }
    }

    func testCaptureLocationFailureDoesNotWaitForBleQueue() {
        let bleQueue = DispatchQueue(label: "io.cutout.test-blocked-ble")
        let core = CutoutSessionCore(clock: MonotonicClock(), bleQueue: bleQueue)
        let bleQueueEntered = DispatchSemaphore(value: 0)
        let releaseBleQueue = DispatchSemaphore(value: 0)
        let handlerReturned = DispatchSemaphore(value: 0)
        let bleQueueDrained = DispatchSemaphore(value: 0)
        let reference = WeakCutoutSessionCoreReference(core)

        bleQueue.async {
            bleQueueEntered.signal()
            releaseBleQueue.wait()
        }
        XCTAssertEqual(bleQueueEntered.wait(timeout: .now() + 5), .success)

        Thread.detachNewThread {
            guard let callbackCore = reference.value else {
                XCTFail("The recording callback owner must remain alive")
                handlerReturned.signal()
                return
            }
            callbackCore.handleCaptureLocationWriteResult(
                CaptureLocationWriteResult(generation: .legacy, outcome: .failed)
            )
            handlerReturned.signal()
        }
        XCTAssertEqual(
            handlerReturned.wait(timeout: .now() + 5),
            .success,
            "location callback handling must not wait for the BLE queue"
        )

        releaseBleQueue.signal()
        bleQueue.async { bleQueueDrained.signal() }
        XCTAssertEqual(bleQueueDrained.wait(timeout: .now() + 5), .success)
        withExtendedLifetime(core) {}
    }

    #if canImport(CoreBluetooth)
        func testStaleCharacteristicErrorIsIgnoredBeforeCurrentCallbackFailure() {
            let subscribed = NSObject()
            let stale = NSObject()
            let staleError = NSError(domain: "stale-characteristic", code: 1)
            let currentError = NSError(domain: "current-characteristic", code: 2)

            let staleResult = coreBluetoothCallbackDisposition(
                subscribed: subscribed,
                callback: stale,
                error: staleError
            )
            guard case .ignored = staleResult else {
                return XCTFail("a stale characteristic callback must be ignored before its error")
            }

            let currentErrorResult = coreBluetoothCallbackDisposition(
                subscribed: subscribed,
                callback: subscribed,
                error: currentError
            )
            guard case .failed(let receivedError) = currentErrorResult else {
                return XCTFail("an error from the subscribed characteristic must fail the callback")
            }
            XCTAssertEqual((receivedError as NSError).code, currentError.code)

            let acceptedResult = coreBluetoothCallbackDisposition(
                subscribed: subscribed,
                callback: subscribed,
                error: nil
            )
            guard case .accepted = acceptedResult else {
                XCTFail("an error-free callback from the subscribed characteristic must be accepted")
                return
            }
        }
    #endif

    func testCaptureForwardsOriginalNotificationReceiptDespiteProcessingDelay() throws {
        let capture = CaptureRecorderSpy()
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(999) }),
            captureRecorder: capture
        )
        let outcome = core.captureFrame(
            direction: "notify",
            characteristic: CBUUID(string: "0000FFE1-0000-1000-8000-00805F9B34FB"),
            service: CBUUID(string: "0000FFE0-0000-1000-8000-00805F9B34FB"),
            bytes: Data([0xaa]),
            captureNotificationEvidence: .stationaryTelemetry,
            receivedAt: MonotonicMilliseconds(150)
        )
        XCTAssertEqual(outcome, .accepted)
        XCTAssertEqual(capture.notificationReceipts, [MonotonicMilliseconds(150)])
        XCTAssertEqual(capture.notificationEvidence, [.stationaryTelemetry])
    }

    func testDefaultSessionProvidesCanonicalRideHistory() throws {
        let core = CutoutSessionCore()
        let state = try XCTUnwrap(core.rideMapStateHandle)

        if RustPersistenceStore.shared != nil {
            XCTAssertNil(state.initializationError)
            XCTAssertNoThrow(try state.storedSummaries(limit: 1))
        } else {
            XCTAssertEqual(
                state.initializationError,
                .storageError("Rust ride database is unavailable")
            )
        }
    }

    func testFinishedCaptureIsPublishedThroughTheExistingRustDatabase() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let platformIdentifier = "capture-test-\(UUID().uuidString)"

        let completed = expectation(description: "finished capture is published")
        let completion = Mutex<CaptureWriterCompletion?>(nil)
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
            database: database,
            publish: { _ in },
            onWriterCompletion: { value in
                completion.withLock { $0 = value }
                completed.fulfill()
            }
        )

        XCTAssertTrue(
            recorder.start(
                generation: CaptureGeneration(rawValue: 1),
                platformIdentifier: platformIdentifier,
                advertisedServices: [],
                directory: directory,
                reason: "manual_test",
                annotations: [],
                evidence: "simulator_fixture",
                origin: .manual,
                advertisedName: "VESC BLE UART"
            )
        )
        let location = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_025,
            sourceTimestampUnixSeconds: nil,
            latitudeDegrees: 39.739_235_8,
            longitudeDegrees: -104.990_251,
            altitudeMeters: 1_609.344,
            horizontalAccuracyMeters: 0.8,
            verticalAccuracyMeters: 1.2,
            speedMetersPerSecond: 4.470_400_25,
            speedAccuracyMetersPerSecond: 0.25,
            courseDegrees: 271.5,
            courseAccuracyDegrees: 3.0
        )
        let locationWrite = recorder.recordLocationUpdate(
            PhoneLocationUpdate(
                receiptMonotonic: MonotonicMilliseconds(125),
                receiptWallClock: Date(timeIntervalSince1970: 1_700_000_000.125),
                samples: [location]
            )
        )
        XCTAssertEqual(locationWrite.generation, CaptureGeneration(rawValue: 1))
        XCTAssertEqual(locationWrite.outcome, .accepted)
        recorder.finish(publishesResult: true, priorWriteOutcome: .accepted)

        await fulfillment(of: [completed], timeout: 5)
        let result = try XCTUnwrap(completion.withLock { $0 })
        XCTAssertTrue(result.succeeded)
        XCTAssertTrue(result.databasePublicationSucceeded == true)
        let captureID = try finishedDatabaseCaptureID(result)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
        let exportURL = try exportFinishedDatabaseCapture(result, database: database, directory: directory)
        let captureText = try String(contentsOf: exportURL, encoding: .utf8)
        XCTAssertTrue(captureText.contains("\"location\":"))
        XCTAssertFalse(captureText.contains("\"phone_location\":"))

        let page = try database.listLiveCaptureHistory(cursor: nil, limit: 500)
        let stored = try XCTUnwrap(page.captures.first { $0.liveCaptureId == captureID })
        XCTAssertEqual(stored.integrity, .complete)
        XCTAssertEqual(stored.eventCount, 1)
        let recording = try XCTUnwrap(stored.recording)
        XCTAssertEqual(recording.origin, .manual)
        XCTAssertEqual(recording.advertisedName, "VESC BLE UART")
        XCTAssertEqual(recording.platformIdentifier, platformIdentifier)
    }

    private func startRecorder(
        _ recorder: CutoutSessionCaptureRecorder,
        directory: URL,
        annotations: [String] = [],
        origin: MobileCaptureOriginDto = .manual,
        generation: UInt64 = 1
    ) -> Bool {
        recorder.start(
            generation: CaptureGeneration(rawValue: generation),
            platformIdentifier: "capture-test-\(UUID().uuidString)",
            advertisedServices: [],
            directory: directory,
            reason: "startup_invariant",
            annotations: annotations,
            evidence: "simulator_fixture",
            origin: origin,
            advertisedName: nil
        )
    }

    private func finishedDatabaseCaptureID(_ result: CaptureWriterCompletion) throws -> String {
        XCTAssertNil(result.fileURL, "durable completion must not require automatic export")
        if case .databaseFinished(let id, let integrity, let export, let status) = result.outcome {
            XCTAssertEqual(integrity, .complete)
            XCTAssertEqual(export, .notAttempted)
            XCTAssertEqual(status.physicalBytesWritten, 0)
            return id
        }
        XCTFail("Expected durable database completion")
        return try XCTUnwrap(nil as String?)
    }

    private func exportFinishedDatabaseCapture(
        _ result: CaptureWriterCompletion,
        database: RideDatabaseHandle,
        directory: URL
    ) throws -> URL {
        let id = try finishedDatabaseCaptureID(result)
        let url = directory.appendingPathComponent("explicit-export.jsonl")
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path))
        let export = try database.exportLiveCapture(liveCaptureId: id, path: url.path)
        XCTAssertEqual(export.path, url.path)
        return url
    }

    private func captureHeader(at url: URL) throws -> [String: Any] {
        let text = try String(contentsOf: url, encoding: .utf8)
        let data = try XCTUnwrap(try XCTUnwrap(text.split(separator: "\n").first).data(using: .utf8))
        let line = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        return try XCTUnwrap(line["header"] as? [String: Any])
    }

    func testInvalidCaptureStartDatesRejectWithoutStateAndPermitValidRetry() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let date = Mutex(Date(timeIntervalSince1970: -1))
        let clockReads = Mutex(0)
        let completed = expectation(description: "valid retry finishes")
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: {
                clockReads.withLock { $0 += 1 }
                return MonotonicMilliseconds(100)
            }),
            wallClock: { date.withLock { $0 } },
            database: try XCTUnwrap(MobileRideMapState.debugDatabase),
            publish: { _ in },
            onWriterCompletion: { value in
                XCTAssertTrue(value.succeeded)
                completed.fulfill()
            }
        )
        for seconds in [-1, Double.nan, .infinity, -.infinity, Double(UInt64.max) / 1_000] {
            date.withLock { $0 = Date(timeIntervalSince1970: seconds) }
            XCTAssertFalse(startRecorder(recorder, directory: directory))
            XCTAssertFalse(recorder.hasWriter)
            XCTAssertNil(recorder.currentGeneration)
            XCTAssertNil(recorder.activeFileURL)
            XCTAssertEqual(clockReads.withLock { $0 }, 0)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
        }
        date.withLock { $0 = Date(timeIntervalSince1970: 1_700_000_000) }
        XCTAssertTrue(startRecorder(recorder, directory: directory, generation: 2))
        XCTAssertEqual(clockReads.withLock { $0 }, 1)
        XCTAssertEqual(recorder.currentGeneration, CaptureGeneration(rawValue: 2))
        recorder.finish(publishesResult: true, priorWriteOutcome: .accepted)
        await fulfillment(of: [completed], timeout: 5)
    }

    func testCaptureStartupUsesOneWallClockSnapshotForFilenameAndHeader() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let reads = Mutex(0)
        let result = Mutex<CaptureWriterCompletion?>(nil)
        let completed = expectation(description: "capture finishes")
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            wallClock: {
                reads.withLock {
                    $0 += 1
                    return Date(timeIntervalSince1970: 1_700_000_000 + Double($0 - 1))
                }
            },
            database: try XCTUnwrap(MobileRideMapState.debugDatabase),
            publish: { _ in },
            onWriterCompletion: { value in
                result.withLock { $0 = value }
                completed.fulfill()
            }
        )
        XCTAssertTrue(startRecorder(recorder, directory: directory))
        XCTAssertEqual(reads.withLock { $0 }, 1)
        XCTAssertTrue(
            try XCTUnwrap(recorder.activeFileURL).lastPathComponent.hasPrefix("cutout-btle-capture-1700000000-"))
        recorder.finish(publishesResult: true, priorWriteOutcome: .accepted)
        await fulfillment(of: [completed], timeout: 5)
        let capture = try XCTUnwrap(result.withLock { $0 })
        XCTAssertTrue(capture.succeeded)
        let header = try captureHeader(
            at: exportFinishedDatabaseCapture(
                capture, database: XCTUnwrap(MobileRideMapState.debugDatabase), directory: directory
            ))
        XCTAssertEqual(header["wall_clock_start_unix_ms"] as? UInt64, 1_700_000_000_000)
    }

    func testDuplicateNativeCaptureStartPreservesTheAdmittedWriter() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let reads = Mutex(0)
        let completed = expectation(description: "original writer finishes")
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            wallClock: {
                reads.withLock { $0 += 1 }
                return Date(timeIntervalSince1970: 1_700_000_000)
            },
            database: try XCTUnwrap(MobileRideMapState.debugDatabase),
            publish: { _ in },
            onWriterCompletion: { value in
                XCTAssertEqual(value.generation, CaptureGeneration(rawValue: 1))
                XCTAssertTrue(value.succeeded)
                completed.fulfill()
            }
        )
        XCTAssertTrue(startRecorder(recorder, directory: directory))
        let originalURL = recorder.activeFileURL
        XCTAssertFalse(startRecorder(recorder, directory: directory, generation: 2))
        XCTAssertTrue(recorder.hasWriter)
        XCTAssertEqual(recorder.currentGeneration, CaptureGeneration(rawValue: 1))
        XCTAssertEqual(recorder.activeFileURL, originalURL)
        XCTAssertEqual(reads.withLock { $0 }, 1)
        recorder.finish(publishesResult: true, priorWriteOutcome: .accepted)
        await fulfillment(of: [completed], timeout: 5)
    }

    func testCaptureStartupReservesPolicyAndActiveLabelClosureAtExactCapacity() async throws {
        let cases: [(MobileCaptureOriginDto, [String])] = [
            (.manual, (0..<4).map { "note=\($0)" }),
            (.automatic, (0..<3).map { "note=\($0)" }),
            (.manual, ["note=0", "note=1", "capture_label=ride_start"]),
            (.automatic, ["note=0", "capture_label=ride_start"]),
        ]
        for (origin, annotations) in cases {
            let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: directory) }
            let result = Mutex<CaptureWriterCompletion?>(nil)
            let completed = expectation(description: "capacity boundary finishes")
            let recorder = CutoutSessionCaptureRecorder(
                clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
                wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
                database: try XCTUnwrap(MobileRideMapState.debugDatabase),
                publish: { _ in },
                onWriterCompletion: { value in
                    result.withLock { $0 = value }
                    completed.fulfill()
                }
            )
            XCTAssertTrue(startRecorder(recorder, directory: directory, annotations: annotations, origin: origin))
            recorder.finish(publishesResult: true, priorWriteOutcome: .accepted)
            await fulfillment(of: [completed], timeout: 5)
            let capture = try XCTUnwrap(result.withLock { $0 })
            XCTAssertTrue(capture.succeeded)
            let header = try captureHeader(
                at: exportFinishedDatabaseCapture(
                    capture, database: XCTUnwrap(MobileRideMapState.debugDatabase), directory: directory
                ))
            let retained = try XCTUnwrap(header["annotations"] as? [String])
            XCTAssertEqual(retained.count, 8)
            for annotation in annotations { XCTAssertTrue(retained.contains(annotation)) }
            if annotations.contains("capture_label=ride_start") {
                XCTAssertTrue(retained.contains("capture_label=ride_stop"))
            }
            XCTAssertEqual(retained.contains("capture_recording_policy=material_changes"), origin == .automatic)
        }
    }

    func testCaptureStartupRejectsActiveLabelClosureBeyondCapacity() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
            database: try XCTUnwrap(MobileRideMapState.debugDatabase),
            publish: { _ in },
            onWriterCompletion: { _ in XCTFail("rejected startup must not finish") }
        )
        XCTAssertFalse(
            startRecorder(
                recorder, directory: directory,
                annotations: ["note=0", "note=1", "capture_label=ride_start"], origin: .automatic
            ))
        XCTAssertFalse(recorder.hasWriter)
        XCTAssertNil(recorder.currentGeneration)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
    }

    func testStaleOptionalMusicContextDoesNotRejectCaptureStartup() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let result = Mutex<CaptureWriterCompletion?>(nil)
        let completed = expectation(description: "capture with stale optional context finishes")
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(10_000) }),
            wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
            database: try XCTUnwrap(MobileRideMapState.debugDatabase),
            publish: { _ in },
            onWriterCompletion: { value in
                result.withLock { $0 = value }
                completed.fulfill()
            }
        )
        XCTAssertEqual(recorder.updateMusicPolicy(.humanReadable), .accepted)
        XCTAssertEqual(
            recorder.recordMusicObservation(
                MobilePevcapMusicEventDto(
                    provider: .appleMusic, trackId: "stale-startup-track", monotonicAtMs: 10,
                    wallClockUnixMs: 1_700_000_000_010, clockUncertaintyMs: 5, rideSequence: nil
                )), .accepted)
        XCTAssertNotNil(recorder.currentMusicObservation)

        XCTAssertTrue(startRecorder(recorder, directory: directory))
        XCTAssertNil(recorder.currentMusicObservation)
        recorder.finish(publishesResult: true, priorWriteOutcome: .accepted)
        await fulfillment(of: [completed], timeout: 5)
        let capture = try XCTUnwrap(result.withLock { $0 })
        XCTAssertTrue(capture.succeeded)
        let exportURL = try exportFinishedDatabaseCapture(
            capture, database: XCTUnwrap(MobileRideMapState.debugDatabase), directory: directory
        )
        let text = try String(contentsOf: exportURL, encoding: .utf8)
        XCTAssertFalse(text.contains("stale-startup-track"))
    }

    func testCaptureStartupRejectsRequestedAnnotationsThatCannotBeRetained() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
            database: try XCTUnwrap(MobileRideMapState.debugDatabase),
            publish: { _ in },
            onWriterCompletion: { _ in XCTFail("rejected startup must not finish a writer") }
        )

        XCTAssertFalse(
            recorder.start(
                generation: CaptureGeneration(rawValue: 1),
                platformIdentifier: "capture-test-\(UUID().uuidString)",
                advertisedServices: [],
                directory: directory,
                reason: "annotation_overflow",
                annotations: (0..<6).map { "note=\($0)" },
                evidence: "simulator_fixture",
                origin: .automatic,
                advertisedName: nil
            )
        )
        XCTAssertFalse(recorder.hasWriter)
        XCTAssertNil(recorder.currentGeneration)
        XCTAssertNil(recorder.activeFileURL)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
    }

    func testFinishWithoutPublishingDoesNotReadPublicationClockOrNotifyCompletion() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let completion = expectation(description: "non-publishing finish does not notify")
        completion.isInverted = true
        let wallClockReadCount = Mutex(0)
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            wallClock: {
                wallClockReadCount.withLock { $0 += 1 }
                return Date(timeIntervalSince1970: 1_700_000_000)
            },
            database: database,
            publish: { _ in },
            onWriterCompletion: { _ in completion.fulfill() }
        )

        XCTAssertTrue(
            recorder.start(
                generation: CaptureGeneration(rawValue: 1),
                platformIdentifier: "capture-test-\(UUID().uuidString)",
                advertisedServices: [],
                directory: directory,
                reason: "manual_test",
                annotations: [],
                evidence: "simulator_fixture",
                origin: .manual,
                advertisedName: nil
            )
        )
        let readsBeforeFinish = wallClockReadCount.withLock { $0 }
        recorder.finish(publishesResult: false, priorWriteOutcome: .accepted)

        await fulfillment(of: [completion], timeout: 0.1)
        XCTAssertEqual(wallClockReadCount.withLock { $0 }, readsBeforeFinish)
    }

    func testDatabasePublicationFailureRetainsTheFinishedDatabaseCapture() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let platformIdentifier = "capture-test-\(UUID().uuidString)"
        let completed = expectation(description: "failed provenance publication retains durable database capture")
        let completion = Mutex<CaptureWriterCompletion?>(nil)
        let recorder = CutoutSessionCaptureRecorder(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
            database: database,
            publish: { _ in },
            onWriterCompletion: { value in
                completion.withLock { $0 = value }
                completed.fulfill()
            }
        )

        XCTAssertTrue(
            recorder.start(
                generation: CaptureGeneration(rawValue: 1),
                platformIdentifier: platformIdentifier,
                advertisedServices: [],
                directory: directory,
                reason: "manual_test",
                annotations: [],
                evidence: "simulator_fixture",
                origin: .manual,
                advertisedName: String(repeating: "x", count: 513)
            )
        )
        XCTAssertEqual(recorder.recordLinkUp(maxWriteLength: 64), .accepted)
        XCTAssertEqual(recorder.recordLinkDown(), .accepted)
        recorder.finish(publishesResult: true, priorWriteOutcome: .accepted)

        await fulfillment(of: [completed], timeout: 5)
        let result = try XCTUnwrap(completion.withLock { $0 })
        XCTAssertTrue(result.succeeded)
        XCTAssertFalse(result.databasePublicationSucceeded ?? true)
        let captureID = try finishedDatabaseCaptureID(result)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
        let page = try database.listLiveCaptureHistory(cursor: nil, limit: 500)
        let stored = try XCTUnwrap(page.captures.first { $0.liveCaptureId == captureID })
        XCTAssertEqual(stored.platformIdentifier, platformIdentifier)
        XCTAssertEqual(stored.integrity, .complete)
        XCTAssertEqual(stored.eventCount, 2)
        XCTAssertNil(stored.recording, "failed provenance must not hide durable capture data")
        let exportURL = try exportFinishedDatabaseCapture(result, database: database, directory: directory)
        let text = try String(contentsOf: exportURL, encoding: .utf8)
        let lines = try text.split(separator: "\n").map { line in
            try XCTUnwrap(JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any])
        }
        XCTAssertEqual(lines.count, 3)
        XCTAssertEqual(lines.first?["kind"] as? String, "header")
        let records = try lines.dropFirst().map { line in
            XCTAssertEqual(line["kind"] as? String, "record")
            return try XCTUnwrap(line["record"] as? [String: Any])
        }
        XCTAssertEqual(records.map { $0["direction"] as? String }, ["LinkUp", "LinkDown"])
        XCTAssertEqual(records.map { $0["monotonic_ms"] as? UInt64 }, [0, 0])
        XCTAssertEqual(records.first?["link_max_write_len"] as? UInt64, 64)
    }

    #if canImport(CoreBluetooth)
        func testCoreBluetoothRestorationPolicyOptsInAndSelectsOnlySavedDevice() {
            XCTAssertEqual(
                CoreBluetoothRestorationPolicy.centralManagerOptions[CBCentralManagerOptionRestoreIdentifierKey]
                    as? String,
                "io.cutout.central"
            )
            XCTAssertEqual(
                CoreBluetoothRestorationPolicy.selectedPlatformIdentifier(
                    savedPlatformIdentifier: "wheel-b",
                    restoredPlatformIdentifiers: ["wheel-a", "wheel-b"]
                ),
                "wheel-b"
            )
            XCTAssertNil(
                CoreBluetoothRestorationPolicy.selectedPlatformIdentifier(
                    savedPlatformIdentifier: "wheel-c",
                    restoredPlatformIdentifiers: ["wheel-a", "wheel-b"]
                )
            )
        }
    #endif

    func testBoundedDiagnosticLogRetainsNewestRecordsAndCountsDroppedHistory() {
        var log = BoundedDiagnosticLog(capacity: 3)

        ["one", "two", "three", "four"].forEach { log.append($0) }

        XCTAssertEqual(log.values, ["two", "three", "four"])
        XCTAssertEqual(log.droppedCount, 1)
    }

    func testRecoverableCaptureAdmissionLossAndFailedFlushPreserveWriterForRetry() async throws {
        let capture = CaptureRecorderSpy()
        capture.flushOutcome = .rejected
        capture.currentWriterStatus = MobileCaptureWriterStatusDto(
            queuedMessages: 0, peakQueuedMessages: 4, droppedMessages: 2,
            bytesWritten: 100, physicalBytesWritten: 100,
            failed: false, lastError: "queue admission lost"
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            captureRecorder: capture
        )
        let generation = try XCTUnwrap(core.rideSessionStateHandle.beginCapture(origin: .manual))
        capture.currentGeneration = CaptureGeneration(rawValue: generation.value)
        XCTAssertTrue(core.rideSessionStateHandle.captureWriterStarted(generation: generation))

        XCTAssertFalse(core.acceptCaptureWriteOutcomeForTesting(.admissionLost(droppedMessages: 2)))
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .recording)
        XCTAssertTrue(capture.hasWriter)
        XCTAssertEqual(capture.finishCount, 0)
        XCTAssertGreaterThan(capture.publishProgressCount, 0)
        let flushed = await core.flushCapture()
        XCTAssertFalse(flushed)
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .recording)
        XCTAssertTrue(capture.hasWriter)
        XCTAssertEqual(capture.publishProgress().writerError, "queue admission lost")
        XCTAssertFalse(capture.publishProgress().writerFailed)
        XCTAssertEqual(capture.publishProgress().writerHealth, .healthy)
        XCTAssertEqual(capture.publishProgress().droppedMessageCount, 2)

        XCTAssertTrue(core.acceptCaptureWriteOutcomeForTesting(.accepted))
        capture.flushOutcome = .flushed
        let retried = await core.flushCapture()
        XCTAssertTrue(retried)
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .recording)
        XCTAssertEqual(capture.finishCount, 0)
        XCTAssertEqual(capture.publishProgress().droppedMessageCount, 2)
    }

    func testCaptureAdmissionLossRejectsPhysicalWriteUntilItsQueuedReceiptIsAdmitted() throws {
        let capture = CaptureRecorderSpy()
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            captureRecorder: capture
        )
        let generation = try XCTUnwrap(core.rideSessionStateHandle.beginCapture(origin: .manual))
        capture.currentGeneration = CaptureGeneration(rawValue: generation.value)
        XCTAssertTrue(core.rideSessionStateHandle.captureWriterStarted(generation: generation))
        let channel = try XCTUnwrap(BluetoothUuid(coreBluetoothUuid: CBUUID(string: "FFE1")))
        var outcome = MobileCaptureWriteOutcomeDto.admissionLost(droppedMessages: 1)
        var captureReceipts: [CoreBluetoothWriteDisposition] = []
        var transportReceipts: [CoreBluetoothWriteDisposition] = []
        var physicalWrites = 0
        var recordedWrites = 0
        let adapter = CutoutSessionBluetoothWriteAdapter(
            makeCaptureReceipt: { _, _, _ in
                { disposition in
                    captureReceipts.append(disposition)
                    return core.acceptCaptureWriteOutcomeForTesting(outcome)
                }
            },
            recordWrite: { _, _ in recordedWrites += 1 }
        )
        let rejected = adapter.submit(
            channel: channel, bytes: Data([1]), canSend: { true }, isCurrent: { true },
            write: { physicalWrites += 1 }, onReceipt: { transportReceipts.append($0) }
        )
        XCTAssertEqual(rejected, .rejected)
        XCTAssertEqual(captureReceipts, [.queued])
        XCTAssertEqual(transportReceipts, [.rejected])
        XCTAssertEqual(physicalWrites, 0)
        XCTAssertEqual(recordedWrites, 0)
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .recording)
        XCTAssertEqual(capture.finishCount, 0)

        outcome = .accepted
        let admitted = adapter.submit(
            channel: channel, bytes: Data([2]), canSend: { true }, isCurrent: { true },
            write: { physicalWrites += 1 }, onReceipt: { transportReceipts.append($0) }
        )
        XCTAssertEqual(admitted, .submitted)
        XCTAssertEqual(captureReceipts, [.queued, .queued, .submitted])
        XCTAssertEqual(transportReceipts, [.rejected, .submitted])
        XCTAssertEqual(physicalWrites, 1)
        XCTAssertEqual(recordedWrites, 1)
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .recording)
    }

    func testFatalCaptureFlushFailureMarksOnlyItsWriterFailed() async throws {
        let capture = CaptureRecorderSpy()
        capture.flushOutcome = .failed(message: "database write failed")
        capture.currentWriterStatus = MobileCaptureWriterStatusDto(
            queuedMessages: 0, peakQueuedMessages: 0, droppedMessages: 0,
            bytesWritten: 100, physicalBytesWritten: 100,
            failed: true, lastError: "database write failed"
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            captureRecorder: capture
        )
        let generation = try XCTUnwrap(core.rideSessionStateHandle.beginCapture(origin: .manual))
        capture.currentGeneration = CaptureGeneration(rawValue: generation.value)
        XCTAssertTrue(core.rideSessionStateHandle.captureWriterStarted(generation: generation))

        let flushed = await core.flushCapture()
        XCTAssertFalse(flushed)
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .saveFailed)
        XCTAssertTrue(capture.hasWriter)
        XCTAssertEqual(capture.finishCount, 0)
        XCTAssertEqual(capture.publishProgress().writerError, "database write failed")
    }

    func testCaptureLocationAdmissionLossUsesRustDecisionAndPreservesWriter() throws {
        let capture = CaptureRecorderSpy()
        capture.currentWriterStatus = MobileCaptureWriterStatusDto(
            queuedMessages: 0, peakQueuedMessages: 2, droppedMessages: 1,
            bytesWritten: 100, physicalBytesWritten: 100,
            failed: false, lastError: "queue admission lost"
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            captureRecorder: capture
        )
        let generation = try XCTUnwrap(core.rideSessionStateHandle.beginCapture(origin: .manual))
        capture.currentGeneration = CaptureGeneration(rawValue: generation.value)
        XCTAssertTrue(core.rideSessionStateHandle.captureWriterStarted(generation: generation))

        core.handleCaptureLocationWriteResult(
            CaptureLocationWriteResult(
                generation: capture.currentGeneration, outcome: .admissionLost(droppedMessages: 1)
            ))
        XCTAssertTrue(core.acceptCaptureWriteOutcomeForTesting(.accepted))
        XCTAssertGreaterThan(capture.publishProgressCount, 0)
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .recording)
        XCTAssertTrue(capture.hasWriter)
        XCTAssertEqual(capture.finishCount, 0)
        XCTAssertEqual(capture.publishProgress().droppedMessageCount, 1)
        XCTAssertEqual(capture.publishProgress().writerHealth, .healthy)
    }

    func testCaptureProgressUsesRustFatalStatusEvenWithoutAnErrorMessage() throws {
        let capture = CaptureRecorderSpy()
        capture.currentWriterStatus = MobileCaptureWriterStatusDto(
            queuedMessages: 0, peakQueuedMessages: 0, droppedMessages: 1,
            bytesWritten: 100, physicalBytesWritten: 100,
            failed: true, lastError: nil
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            captureRecorder: capture
        )
        let generation = try XCTUnwrap(core.rideSessionStateHandle.beginCapture(origin: .manual))
        capture.currentGeneration = CaptureGeneration(rawValue: generation.value)
        XCTAssertTrue(core.rideSessionStateHandle.captureWriterStarted(generation: generation))

        XCTAssertFalse(core.acceptCaptureWriteOutcomeForTesting(.admissionLost(droppedMessages: 1)))
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.stage, .saveFailed)
        XCTAssertNil(capture.publishProgress().writerError)
        XCTAssertTrue(capture.publishProgress().writerFailed)
        XCTAssertEqual(capture.publishProgress().writerHealth, .failed)
        XCTAssertTrue(capture.hasWriter)
        XCTAssertEqual(capture.finishCount, 0)
    }

    func testLinkUpCaptureFailureDoesNotFailTheConnection() {
        let capture = CaptureRecorderSpy()
        capture.recordLinkUpResult = .failed
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(),
                connectionDelayMilliseconds: 0
            ),
            captureRecorder: capture
        )
        XCTAssertTrue(core.recordOnly(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        core.applyLinkUpStep(CoreBluetoothSessionStep(operations: [], snapshot: nil))

        XCTAssertEqual(core.phase, .subscribing)
        XCTAssertEqual(capture.publishFailureCount, 1)
        XCTAssertEqual(capture.finishCount, 1)
    }

    func testLinkUpCaptureFailureIsAppliedAfterDisplayReductionAndPhaseChange() {
        var events = [String]()
        let capture = CaptureRecorderSpy()
        capture.recordLinkUpResult = .failed
        let effects = CutoutSessionNotificationEffects(
            applyActions: { _ in
                events.append("actions")
                return []
            },
            observeRideMapConnection: { _, _, _ in events.append("map") },
            persistBmsSamples: { _ in events.append("bms") },
            reduceDisplayState: { state, snapshot, receivedAt, _ in
                events.append("display")
                return state.reducingLinkUpSnapshot(snapshot, receivedAt: receivedAt)
            }
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(),
                connectionDelayMilliseconds: 0
            ),
            notificationEffects: effects,
            captureRecorder: capture
        )
        XCTAssertTrue(core.recordOnly(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        var phaseWhenCaptureFailed: SessionConnectionPhase?
        capture.onPublishFailure = {
            events.append("capture-failure")
            phaseWhenCaptureFailed = core.phase
        }

        core.applyLinkUpStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(at: MonotonicMilliseconds(100), speed: speedValue(1_234))
            )
        )

        XCTAssertEqual(events, ["actions", "map", "bms", "display", "capture-failure"])
        XCTAssertEqual(phaseWhenCaptureFailed, .subscribing)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(capture.finishCount, 1)
    }

    func testLinkUpSnapshotIsReducedWhenCaptureRecordingFails() {
        let capture = CaptureRecorderSpy()
        capture.recordLinkUpResult = .failed
        let receivedAt = MonotonicMilliseconds(42)
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { receivedAt }),
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(),
                connectionDelayMilliseconds: 0
            ),
            captureRecorder: capture
        )
        XCTAssertTrue(core.recordOnly(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        let snapshot = TelemetrySnapshot(at: receivedAt, speed: speedValue(1_234))

        core.applyLinkUpStep(CoreBluetoothSessionStep(operations: [], snapshot: snapshot))

        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.telemetry, snapshot)
        XCTAssertEqual(core.displayState.notificationCount, 0)
        XCTAssertEqual(core.displayState.lastUpdate, receivedAt)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.phase, .subscribing)
        XCTAssertEqual(capture.publishFailureCount, 1)
        XCTAssertEqual(capture.finishCount, 1)
    }

    func testLinkUpWithoutSnapshotDoesNotRefreshStaleTelemetry() {
        let previousSnapshot = TelemetrySnapshot(speed: speedValue(1_234))
        let previous = RideDisplayState(
            speed: SpeedReadout(snapshot: previousSnapshot),
            telemetry: previousSnapshot,
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(12)
        )

        let updated = previous.reducingLinkUpSnapshot(
            nil,
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(updated, previous)
    }

    func testLinkUpWithoutTelemetryTimestampDoesNotRefreshStaleTelemetry() {
        let previousSnapshot = TelemetrySnapshot(speed: speedValue(8_000))
        let previous = RideDisplayState(
            speed: SpeedReadout(snapshot: previousSnapshot),
            telemetry: previousSnapshot,
            notificationCount: 3,
            lastUpdate: MonotonicMilliseconds(20)
        )

        let updated = previous.reducingLinkUpSnapshot(
            TelemetrySnapshot(),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(updated, previous)
    }

    @MainActor
    func testRestorationPublishesSelectionBeforeReplayingTheCurrentPhase() {
        let core = CutoutSessionCore()
        var events: [String] = []
        core.onBluetoothRestorationResolved = { identifier in
            events.append("restored=\(identifier ?? "none")")
        }
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            XCTAssertEqual(phase, core.phase)
            events.append("phase")
        }

        core.publishBluetoothRestoration("wheel-a")
        XCTAssertEqual(events, ["restored=wheel-a", "phase"])

        events.removeAll()
        core.publishBluetoothRestoration(nil)
        XCTAssertEqual(events, ["restored=none"])
    }

    func testMonotonicClockUsesItsInjectedUptimeSource() {
        let now = Mutex<MonotonicMilliseconds>(MonotonicMilliseconds(100))
        let clock = MonotonicClock(now: { now.withLock { $0 } })

        XCTAssertEqual(clock.now(), MonotonicMilliseconds(100))

        now.withLock { $0 = MonotonicMilliseconds(250) }
        XCTAssertEqual(clock.now(), MonotonicMilliseconds(250))
    }

    func testMonotonicElapsedSaturatesWhenTheClockMovesBackward() {
        XCTAssertEqual(
            MonotonicMilliseconds(1_333).elapsed(since: MonotonicMilliseconds(1_000)),
            MonotonicMilliseconds(333)
        )
        XCTAssertEqual(
            MonotonicMilliseconds(1_000).elapsed(since: MonotonicMilliseconds(1_333)),
            MonotonicMilliseconds(0)
        )
    }

    @MainActor
    func testPhoneLocationReadbackTracksAValidSampleWithoutAnActiveRide() async {
        let core = CutoutSessionCore()
        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 4,
            verticalAccuracy: 6,
            course: 90,
            courseAccuracy: 3,
            speed: 2,
            speedAccuracy: 0.2,
            timestamp: Date(timeIntervalSince1970: 1_700_000_000)
        )

        let published = expectation(description: "Phone sample delivered to Main")
        core.onPhoneLocationSnapshotChange = { snapshot, _ in
            if snapshot.latestSample != nil { published.fulfill() }
        }
        core.deliverPhoneLocationsForTesting([location])
        await fulfillment(of: [published], timeout: 2)

        XCTAssertEqual(core.phoneLocationSnapshot.latestSample?.latitudeDegrees, 39.7392)
        XCTAssertEqual(core.phoneLocationSnapshot.latestSample?.longitudeDegrees, -104.9903)
    }

    @MainActor
    func testAuthorizationChangePublishesRideMapAvailabilityWithoutLocationDemand() async {
        let core = CutoutSessionCore()
        let published = expectation(description: "Authorization availability is delivered")
        var publications = 0
        core.onRideMapAvailabilityChange = { _ in
            publications += 1
            if publications == 1 { published.fulfill() }
        }

        core.refreshPhoneLocationAuthorizationForTesting()

        await fulfillment(of: [published], timeout: 2)
        XCTAssertGreaterThanOrEqual(publications, 1)
    }

    @MainActor
    func testAuthorizationRefreshPreservesRustStorageFailure() async {
        let unavailable = expectation(description: "Rust storage failure is published")
        let core = CutoutSessionCore(
            rideMapState: MobileRideMapState(storageUnavailable: "map database unavailable")
        )
        var lastAvailability: MobileRideMapAvailability?
        var observedStorageFailure = false
        core.onRideMapAvailabilityChange = { availability in
            lastAvailability = availability
            if availability == .storageUnavailable && !observedStorageFailure {
                observedStorageFailure = true
                unavailable.fulfill()
            }
        }

        core.start()
        await fulfillment(of: [unavailable], timeout: 2)

        let refreshed = expectation(description: "Authorization refresh retains Rust storage failure")
        core.onRideMapAvailabilityChange = { availability in
            lastAvailability = availability
            refreshed.fulfill()
        }
        core.refreshPhoneLocationAuthorizationForTesting()
        await fulfillment(of: [refreshed], timeout: 2)
        XCTAssertEqual(lastAvailability, .storageUnavailable)
    }

    private static func location(timestamp: Date, latitude: CLLocationDegrees) -> CLLocation {
        CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: latitude, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 4,
            verticalAccuracy: 6,
            course: 90,
            courseAccuracy: 3,
            speed: 2,
            speedAccuracy: 0.2,
            timestamp: timestamp
        )
    }

    func testPhoneLocationStateClearPreventsCrossCaptureContext() {
        let state = MobilePhoneLocationState()
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_000,
            sourceTimestampUnixSeconds: nil,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )

        _ = state.ingest(sample: sample)
        XCTAssertEqual(state.currentSnapshot().latestSample, sample)

        state.clear()

        XCTAssertNil(state.currentSnapshot().latestSample)
    }

    func testDatabaseBackedRideMapReportsPendingThenDurablyAccepted() async throws {
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)

        let state = MobileRideMapState(database: database)
        _ = try await state.restoreCommand(atMs: 0)
        defer {
            _ = try? state.stop(atMs: 200)
            _ = try? state.discard()
        }
        _ = try state.startGpsOnly(atMs: 100)
        let decision = try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        )

        guard case .pending(let point) = decision else {
            return XCTFail("database-backed ingestion should return pending, got \(decision)")
        }

        var accepted: MobileRideMapDecisionDto?
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .seconds(10))
        while clock.now < deadline, !Task.isCancelled {
            if let outcome = state.pollLocationWrites().first {
                accepted = outcome
                break
            }
            try? await Task.sleep(for: .milliseconds(1))
        }

        guard case .accepted(let acceptedPoint) = accepted else {
            return XCTFail("pending location did not produce a durable acceptance")
        }
        XCTAssertEqual(acceptedPoint, point)
    }

    func testCaptureElapsedTimeUsesTheInjectedMonotonicClockAtExactBoundaries() {
        let now = Mutex<MonotonicMilliseconds>(MonotonicMilliseconds(2_999))
        let core = CutoutSessionCore(clock: MonotonicClock { now.withLock { $0 } })
        let startedAt = MonotonicMilliseconds(1_000)

        XCTAssertEqual(core.captureElapsedMilliseconds(since: startedAt), 1_999)

        now.withLock { $0 = MonotonicMilliseconds(3_000) }
        XCTAssertEqual(core.captureElapsedMilliseconds(since: startedAt), 2_000)

        now.withLock { $0 = MonotonicMilliseconds(3_001) }
        XCTAssertEqual(core.captureElapsedMilliseconds(since: startedAt), 2_001)
    }

    func testReconnectTimerCancelsSupersededAndExplicitWork() {
        let scheduler = RecordingReconnectScheduler()
        let timer = ConnectionReconnectController(scheduler: scheduler)
        var completed = [String]()

        XCTAssertEqual(
            timer.schedule(after: 200) { completed.append("first") },
            ConnectionReconnectSchedule(delayMilliseconds: 200)
        )
        XCTAssertEqual(
            timer.schedule(after: 500) { completed.append("second") },
            ConnectionReconnectSchedule(delayMilliseconds: 500)
        )

        scheduler.runAll()
        XCTAssertEqual(completed, ["second"])

        XCTAssertEqual(
            timer.schedule(after: 1_200) { completed.append("cancelled") },
            ConnectionReconnectSchedule(delayMilliseconds: 1_200)
        )
        timer.cancel()
        scheduler.runAll()

        XCTAssertEqual(completed, ["second"])
    }

    func testNordicNotificationUUIDsRemainFullWidthForPevcap() {
        let service = CBUUID(string: "6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
        let notify = CBUUID(string: "6E400003-B5A3-F393-E0A9-E50E24DCCA9E")

        XCTAssertEqual(BluetoothUuid(coreBluetoothUuid: service)?.bytes.count, 16)
        XCTAssertEqual(BluetoothUuid(coreBluetoothUuid: notify)?.bytes.count, 16)
        XCTAssertEqual(
            BluetoothUuid(coreBluetoothUuid: service)?.bytes,
            BluetoothUuid(
                Data([
                    0x6e, 0x40, 0x00, 0x01, 0xb5, 0xa3, 0xf3, 0x93,
                    0xe0, 0xa9, 0xe5, 0x0e, 0x24, 0xdc, 0xca, 0x9e,
                ]))?.bytes
        )
    }

    func testObservedAdvertisementsUpdatePickerScanState() {
        XCTAssertEqual(
            Set(CoreBluetoothScanPolicy.aeroFalcon.serviceUuids),
            Set([.eucSerialFfe0, .vescSerialFff0, .vescNordicUartService])
        )
        let core = CutoutSessionCore()
        var observedStates: [DevicePickerScanState] = []
        core.onScanStateChange = { observedStates.append($0) }

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "device-ffe0",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-unknown"),
                localName: "device-fff0",
                advertisedServiceUuids: [.bluetooth16(0xFFF0)]
            )
        )

        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["device-ffe0", "device-fff0"])
        XCTAssertEqual(core.scanState.rows.map(\.connectionRoute), [nil, nil])
        XCTAssertTrue(core.scanState.sections.supported.isEmpty)
        XCTAssertEqual(core.scanState.sections.probeRecommended.map(\.title), ["device-ffe0", "device-fff0"])
        XCTAssertTrue(core.scanState.sections.unsupported.isEmpty)
        XCTAssertEqual(observedStates.count, 2)
    }

    func testUnnamedNordicUartAdvertisementRemainsProvisional() {
        let core = CutoutSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc-unnamed"),
                localName: nil,
                advertisedServiceUuids: [.vescNordicUartService]
            )
        )

        XCTAssertEqual(core.scanState.rows.map(\.title), ["VESC device"])
        XCTAssertNil(core.scanState.rows.first?.connectionRoute)
    }

    func testObservedAdvertisementsHideNonPevRows() {
        let core = CutoutSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-keyboard"),
                localName: "Keyboard",
                advertisedServiceUuids: []
            )
        )

        XCTAssertTrue(core.scanState.rows.isEmpty)
    }

    func testPairUnknownCandidateReturnsFalse() {
        let core = CutoutSessionCore()

        XCTAssertFalse(core.pair(platformIdentifier: "ios-local-missing"))
    }

    @MainActor
    func testScriptedProbeTimeoutFailsWithoutPublishingLive() {
        let failed = expectation(description: "probe timeout is published")
        let live = expectation(description: "probe timeout never publishes live")
        live.isInverted = true
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedProbeCandidate,
                telemetry: nil,
                identificationProbeFailure: .timedOut,
                detectedSupport: .supported(
                    connectionRoute: .electricUnicycle,
                    electricUnicycleModel: .aero
                ),
                connectionDelayMilliseconds: 0
            ))
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == .failed(.identificationFailed(.timedOut)) {
                failed.fulfill()
            }
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertTrue(core.probe(platformIdentifier: scriptedProbeCandidate.platformIdentifier))

        wait(for: [failed, live], timeout: 0.2)
        XCTAssertEqual(core.phase, .failed(.identificationFailed(.timedOut)))
        XCTAssertNil(core.displayState.speed.millimetersPerSecond)
    }

    @MainActor
    func testScriptedSessionUsesTheCorePublicationPath() {
        let live = expectation(description: "scripted session reaches live")
        let core = CutoutSessionCore(
            clock: MonotonicClock(),
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                connectionDelayMilliseconds: 0
            )
        )
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        wait(for: [live], timeout: 1)
        XCTAssertEqual(core.phase, .live)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 8_000)
    }

    @MainActor
    func testLiveScriptRetainsItsPickerRowAfterPublishingIdentity() {
        let live = expectation(description: "scripted session reaches live")
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                startsLive: true,
                connectionDelayMilliseconds: 0
            ))
        var publishedRows = [[DevicePickerRow]]()
        core.onScanStateChange = { publishedRows.append($0.rows) }
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == .live { live.fulfill() }
        }

        core.start()
        wait(for: [live], timeout: 1)
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertTrue(publishedRows.allSatisfy { $0 == [scriptedVescCandidate.pickerRow] })
    }

    #if DEBUG
        @MainActor
        func testScriptedControlsUseProtocolEvidenceAndFenceReplacementRequests() throws {
            let live = expectation(description: "generic session reaches live twice")
            live.expectedFulfillmentCount = 2
            var frame = Data(repeating: 0, count: 42)
            frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
            frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
            let core = CutoutSessionCore(
                testScript: CutoutSessionTestScript(
                    candidate: scriptedAeroCandidate, telemetry: TelemetrySnapshot(speed: speedValue(0)),
                    protocolNotifications: [frame], connectionDelayMilliseconds: 0
                ))
            var oldToken: ConnectionAttemptToken?
            var newToken: ConnectionAttemptToken?
            core.onPhaseChange = { presentation in
                let phase = presentation.phase
                guard phase == .live else { return }
                if oldToken == nil {
                    oldToken = core.connectionSnapshot.token
                    core.disconnectAndScan()
                    XCTAssertTrue(core.pair(platformIdentifier: self.scriptedAeroCandidate.platformIdentifier))
                } else {
                    newToken = core.connectionSnapshot.token
                }
                live.fulfill()
            }
            core.start()
            XCTAssertTrue(core.pair(platformIdentifier: scriptedAeroCandidate.platformIdentifier))
            wait(for: [live], timeout: 2)
            let first = try XCTUnwrap(oldToken)
            let current = try XCTUnwrap(newToken)
            XCTAssertNotEqual(first.generation, current.generation)
            XCTAssertFalse(core.resetTripMeterForNewRide(token: first))
            let before = core.settings
            XCTAssertThrowsError(
                try core.submitDeviceSetting(token: first, id: .highBeam, value: .boolean(value: true))
            ) {
                XCTAssertEqual($0 as? DeviceSettingSubmissionError, .ConnectionUnavailable)
            }
            XCTAssertEqual(core.settings.settings, before.settings)
            XCTAssertThrowsError(
                try core.submitDeviceSetting(token: current, id: .pwmTiltback, value: .number(value: 101))
            ) {
                XCTAssertEqual($0 as? DeviceSettingSubmissionError, .InvalidValue)
            }
            try core.submitDeviceSetting(token: current, id: .pwmTiltback, value: .number(value: 80))
            XCTAssertFalse(core.settings.validationAuthorized)
            try core.submitDeviceSetting(token: current, id: .highBeam, value: .boolean(value: true))
            let highBeam = try XCTUnwrap(core.settings.setting(for: .highBeam))
            XCTAssertEqual(highBeam.requested, .boolean(value: true))
            XCTAssertEqual(highBeam.status, .sentWithoutConfirmation)
            XCTAssertNil(highBeam.current)
            core.disconnectAndScan()
        }
    #endif

    @MainActor
    func testScriptedBluetoothUnavailableSessionPublishesNoPickerRows() {
        assertScriptedInitialBluetoothState(
            .unavailable,
            phase: .bluetoothUnavailable(rawState: 4),
            scanState: .bluetoothUnavailable
        )
    }

    @MainActor
    func testScriptedBluetoothPermissionDeniedSessionPublishesNoPickerRows() {
        assertScriptedInitialBluetoothState(
            .permissionDenied,
            phase: .bluetoothPermissionDenied,
            scanState: .permissionDenied
        )
    }

    @MainActor
    private func assertScriptedInitialBluetoothState(
        _ initialBluetoothState: CutoutSessionTestInitialBluetoothState,
        phase expectedPhase: SessionConnectionPhase,
        scanState expectedScanState: DevicePickerScanState
    ) {
        let unavailable = expectation(description: "scripted session becomes unavailable")
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                initialBluetoothState: initialBluetoothState
            )
        )
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == expectedPhase {
                unavailable.fulfill()
            }
        }

        core.start()

        wait(for: [unavailable], timeout: 1)
        XCTAssertEqual(core.phase, expectedPhase)
        XCTAssertEqual(core.scanState, expectedScanState)
    }

    @MainActor
    func testExplicitDisconnectCancelsTheScriptedLateLiveCallback() {
        let live = expectation(description: "late scripted callback is ignored")
        live.isInverted = true
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                connectionDelayMilliseconds: 50
            )
        )
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        core.disconnectAndScan()

        wait(for: [live], timeout: 0.2)
        XCTAssertEqual(core.phase, .scanning)
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertNil(core.displayState.speed.millimetersPerSecond)
    }

    @MainActor
    func testScriptedLiveConnectionStartsRideMapAndPublishesSnapshot() throws {
        let live = expectation(description: "scripted session reaches live")
        let rideStarted = expectation(description: "ride-map recording starts")
        rideStarted.assertForOverFulfill = false
        let suiteName = "CutoutSessionCoreTests.rideMapAutoStart.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let selectedDeviceStore = DevicePickerSelectionStore(database: database, defaults: defaults)
        selectedDeviceStore.save(platformIdentifier: scriptedVescCandidate.platformIdentifier)
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                protocolNotifications: [
                    Data([
                        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101,
                        114, 104, 97, 115, 104, 0, 38, 208, 3,
                    ])
                ],
                startsLive: true,
                connectionDelayMilliseconds: 0
            ),
            rideMapState: MobileRideMapState(),
            selectedDeviceStore: selectedDeviceStore
        )
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == .live { live.fulfill() }
        }
        core.onRideMapSnapshotChange = { snapshot in
            if snapshot.state == .active { rideStarted.fulfill() }
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        wait(for: [live, rideStarted], timeout: 1)
        XCTAssertEqual(core.rideMapStateHandle?.currentSnapshot()?.state, .active)
    }

    @MainActor
    func testProductionLocationPathPublishesAcceptedRideMapPoint() throws {
        let live = expectation(description: "scripted session reaches live")
        let recording = expectation(description: "durable recording accepts locations")
        recording.assertForOverFulfill = false
        let pointAccepted = expectation(description: "ride-map point is accepted")
        let suiteName = "CutoutSessionCoreTests.rideMapLocation.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let selectedDeviceStore = DevicePickerSelectionStore(database: database, defaults: defaults)
        selectedDeviceStore.save(platformIdentifier: scriptedVescCandidate.platformIdentifier)
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                protocolNotifications: [
                    Data([
                        2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101,
                        114, 104, 97, 115, 104, 0, 38, 208, 3,
                    ])
                ],
                startsLive: true,
                connectionDelayMilliseconds: 0
            ),
            rideMapState: MobileRideMapState(),
            selectedDeviceStore: selectedDeviceStore
        )
        core.onPhaseChange = { presentation in
            let phase = presentation.phase
            if phase == .live { live.fulfill() }
        }
        core.onRideMapSnapshotChange = { snapshot in
            if snapshot.state == .active { recording.fulfill() }
        }
        core.onRideMapDecisionChange = { _, decision in
            if case .accepted = decision { pointAccepted.fulfill() }
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        wait(for: [live, recording], timeout: 1)
        core.onRideMapSnapshotChange = nil
        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 4,
            verticalAccuracy: 4,
            course: 0,
            speed: 8,
            timestamp: Date()
        )
        core.deliverPhoneLocationsForTesting([location])
        wait(for: [pointAccepted], timeout: 2)
    }

    @MainActor
    func testScriptedSessionPublishesReconnectAndReturnsLive() {
        let retry = expectation(description: "scripted session schedules reconnect")
        let live = expectation(description: "scripted session returns live after reconnect")
        let retryObserved = Mutex(false)
        let platformIdentifier = scriptedVescCandidate.platformIdentifier
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                reconnectsAfterFirstLive: true,
                reconnectDelayMilliseconds: 0,
                connectionDelayMilliseconds: 0
            )
        )
        let initialGeneration = core.connectionSnapshot.generation
        core.onPhaseChange = { [weak core] presentation in
            let phase = presentation.phase
            // A zero-delay reconnect can retire the first live generation before
            // Main delivers it. Only the current post-retry publication is required.
            if phase == .live, retryObserved.withLock({ $0 }),
                let core, core.connectionSnapshot.generation > initialGeneration + 1
            {
                live.fulfill()
            }
        }
        core.onReconnectScheduled = { scheduled in
            XCTAssertEqual(scheduled.platformIdentifier, platformIdentifier)
            XCTAssertEqual(scheduled.attempt, 1)
            retryObserved.withLock { $0 = true }
            retry.fulfill()
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: platformIdentifier))

        wait(for: [retry, live], timeout: 3)
        XCTAssertEqual(core.phase, .live)
        let connection = core.connectionSnapshot
        XCTAssertGreaterThan(connection.generation, initialGeneration + 1)
        XCTAssertEqual(connection.transport, .connected)
        XCTAssertEqual(connection.token?.platformIdentifier, platformIdentifier)
    }

    func testRecoverableGattFailureDoesNotPublishTerminalRideFailure() {
        let core = CutoutSessionCore()
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil),
            receivedAt: MonotonicMilliseconds(100)
        )
        XCTAssertEqual(core.phase, .live)
        var disconnects = 0

        core.recoverConnection(after: .serviceDiscoveryFailed("link lost")) {
            disconnects += 1
        }

        XCTAssertEqual(disconnects, 1)
        XCTAssertEqual(core.phase, .discoveringServices)
        XCTAssertFalse(core.isRecordOnlyConnection)
    }

    func testUnresolvedRestoredConnectionRetriesInsteadOfEnteringCaptureOnly() {
        let defaults = UserDefaults(suiteName: #function)!
        defer { defaults.removePersistentDomain(forName: #function) }
        let selectionStore = DevicePickerSelectionStore(defaults: defaults)
        selectionStore.save(platformIdentifier: "wheel-a")
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(1_000) },
            selectedDeviceStore: selectionStore
        )

        XCTAssertEqual(
            core.prepareRestoredConnection(from: ["wheel-a", "wheel-b"]),
            "wheel-a"
        )

        core.recordUnresolvedProtocolDetection(.timedOut, on: nil)

        XCTAssertEqual(core.phase, .discoveringServices)
        XCTAssertFalse(core.isRecordOnlyConnection)
        XCTAssertTrue(core.rideSessionStateHandle.shouldRetryIdentification())
    }

    func testUnresolvedFirstUseCanStillEnterCaptureOnly() {
        let core = CutoutSessionCore()
        core.rideSessionStateHandle.setDeviceConnectionIntent(intent: .use)

        core.recordUnresolvedProtocolDetection(.unsupported, on: nil)

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.isRecordOnlyConnection)
    }

    @MainActor
    func testEstablishedRecoveryIdentificationTimeoutsNeverBecomeManualCaptureOnly() throws {
        let scheduler = RecordingReconnectScheduler()
        let now = Mutex(MonotonicMilliseconds(1_000))
        let capture = CaptureRecorderSpy()
        capture.hasWriter = false
        capture.currentGeneration = nil
        let core = CutoutSessionCore(
            clock: MonotonicClock { now.withLock { $0 } },
            testScript: CutoutSessionTestScript(
                candidate: scriptedAeroCandidate, telemetry: nil, connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler, reconnectJitter: { 0 }, captureRecorder: capture
        )
        defer { core.disconnectAndScan() }
        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedAeroCandidate.platformIdentifier))
        let state = core.rideSessionStateHandle
        let verified = try XCTUnwrap(core.connectionSnapshot.token)
        var frame = Data(repeating: 0, count: 42)
        frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        _ = state.connectionLinkEstablished(token: verified)
        _ = state.observeConnectionNotification(token: verified, bytes: frame)
        XCTAssertEqual(
            state.resolveDeviceSession(token: verified, identificationComplete: true, nowMs: 1_001).connection
                .readiness, .verified)
        var reconnects = 0
        for index in 0..<12 {
            core.handleTransportTermination(
                platformIdentifier: scriptedAeroCandidate.platformIdentifier, error: nil,
                reconnect: { reconnects += 1 }
            )
            now.withLock { $0 = MonotonicMilliseconds(UInt64(index + 1) * 61_000) }
            scheduler.runAll()
            XCTAssertEqual(reconnects, index + 1)
            XCTAssertTrue(state.shouldRetryIdentification())
            let retry = try XCTUnwrap(core.connectionSnapshot.token)
            now.withLock { $0 = MonotonicMilliseconds(UInt64(index + 1) * 61_000 + 15_001) }
            XCTAssertEqual(
                state.expireConnectionAttempt(token: retry, nowMs: now.withLock { $0.rawValue }).readiness, .recordOnly)
            core.recordUnresolvedProtocolDetection(.timedOut, on: nil)
            XCTAssertEqual(core.phase, .discoveringServices)
            XCTAssertFalse(
                core.isRecordOnlyConnection,
                "An established recovery timeout cannot enter the manual capture-only branch")
        }
    }

    @MainActor
    func testTransportTerminationPreservesVerifiedWheelIdentityAcrossRetries() {
        let scheduler = RecordingReconnectScheduler()
        let now = Mutex(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(
            clock: MonotonicClock { now.withLock { $0 } },
            testScript: CutoutSessionTestScript(
                candidate: scriptedAeroCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )
        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedAeroCandidate.platformIdentifier))
        XCTAssertEqual(core.electricUnicycleModel, .aero)

        for deadline in [UInt64(1_200), 1_600] {
            core.handleTransportTermination(
                platformIdentifier: scriptedAeroCandidate.platformIdentifier,
                error: nil,
                reconnect: {}
            )
            now.withLock { $0 = MonotonicMilliseconds(deadline) }
            scheduler.runAll()
            XCTAssertEqual(core.electricUnicycleModel, .aero)
            XCTAssertFalse(core.isRecordOnlyConnection)
        }

        core.disconnectAndScan()
        XCTAssertNil(core.electricUnicycleModel)
    }

    @MainActor
    func testKnownRoutesRequireFreshProtocolDetectionAfterReconnect() throws {
        var aeroFrame = Data(repeating: 0, count: 42)
        aeroFrame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        aeroFrame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        let vescReply = Data([
            2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208, 3,
        ])
        for (candidate, reply) in [(scriptedAeroCandidate, aeroFrame), (scriptedVescCandidate, vescReply)] {
            let scheduler = RecordingReconnectScheduler()
            let now = Mutex(MonotonicMilliseconds(1_000))
            let core = CutoutSessionCore(
                clock: MonotonicClock { now.withLock { $0 } },
                testScript: CutoutSessionTestScript(
                    candidate: candidate, telemetry: nil, connectionDelayMilliseconds: 60_000
                ),
                reconnectScheduler: scheduler,
                reconnectJitter: { 0 }
            )
            defer { core.disconnectAndScan() }
            core.start()
            XCTAssertTrue(core.pair(platformIdentifier: candidate.platformIdentifier))
            let state = core.rideSessionStateHandle
            let previous = try XCTUnwrap(core.connectionSnapshot.token)
            core.handleTransportTermination(
                platformIdentifier: candidate.platformIdentifier, error: nil, reconnect: {}
            )
            now.withLock { $0 = MonotonicMilliseconds(1_200) }
            scheduler.runAll()

            let retry = try XCTUnwrap(core.connectionSnapshot.token)
            XCTAssertNotEqual(previous, retry)
            XCTAssertEqual(core.connectionSnapshot.readiness, .pending)
            XCTAssertTrue(core.isDetectingProtocol, "A known route must still subscribe for fresh identity evidence")
            XCTAssertFalse(core.isRecordOnlyConnection)
            XCTAssertEqual(core.electricUnicycleModel, candidate == scriptedAeroCandidate ? .aero : nil)
            XCTAssertNil(state.observeConnectionNotification(token: previous, bytes: reply))
            _ = state.connectionLinkEstablished(token: retry)
            XCTAssertNil(state.resolveDeviceSession(token: retry, identificationComplete: false, nowMs: 1_200).identity)
            _ = state.observeConnectionNotification(token: retry, bytes: reply)
            let resolved = state.resolveDeviceSession(token: retry, identificationComplete: true, nowMs: 1_201)
            XCTAssertEqual(resolved.connection.readiness, .verified)
            XCTAssertNotNil(resolved.identity)
            XCTAssertFalse(core.isDetectingProtocol)
        }
    }

    @MainActor
    func testTransportTerminationUsesTheSharedReconnectTransition() {
        let scheduler = RecordingReconnectScheduler()
        let now = Mutex(MonotonicMilliseconds(1_000))
        let retry = expectation(description: "transport termination schedules retry")
        var reconnectCount = 0
        let core = CutoutSessionCore(
            clock: MonotonicClock { now.withLock { $0 } },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )
        core.onReconnectScheduled = { scheduled in
            XCTAssertEqual(scheduled.platformIdentifier, self.scriptedVescCandidate.platformIdentifier)
            XCTAssertEqual(scheduled.attempt, 1)
            XCTAssertEqual(scheduled.deadline, MonotonicMilliseconds(1_200))
            now.withLock { $0 = MonotonicMilliseconds(scheduled.deadline.rawValue - 1) }
            retry.fulfill()
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        core.handleTransportTermination(
            platformIdentifier: scriptedVescCandidate.platformIdentifier,
            error: nil,
            reconnect: { reconnectCount += 1 }
        )

        wait(for: [retry], timeout: 1)
        XCTAssertEqual(core.phase, .discoveringServices)
        XCTAssertEqual(reconnectCount, 0)

        scheduler.runAll()
        XCTAssertEqual(reconnectCount, 0)

        now.withLock { $0 = MonotonicMilliseconds(1_200) }
        scheduler.runAll()
        XCTAssertEqual(reconnectCount, 1)
    }

    func testQueuedReconnectCannotReplaceNewAttemptEvenForSameDevice() {
        for replacement in ["A", "B"] {
            let scheduler = RecordingReconnectScheduler()
            let core = CutoutSessionCore(
                clock: MonotonicClock { MonotonicMilliseconds(1_000) },
                testScript: CutoutSessionTestScript(
                    candidate: scriptedVescCandidate, telemetry: nil, connectionDelayMilliseconds: 60_000),
                reconnectScheduler: scheduler,
                reconnectJitter: { 0 }
            )
            var reconnectCount = 0
            _ = core.rideSessionStateHandle.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0)
            core.handleTransportTermination(platformIdentifier: "A", error: nil, reconnect: { reconnectCount += 1 })
            _ = core.rideSessionStateHandle.beginConnectionAttempt(platformIdentifier: replacement, nowMs: 1)
            scheduler.runAll()
            XCTAssertEqual(reconnectCount, 0)
            XCTAssertEqual(core.connectionSnapshot.token?.platformIdentifier, replacement)
        }
    }

    func testExplicitDisconnectRejectsRetryEvenIfCancelledWorkIsDelivered() {
        let scheduler = RecordingReconnectScheduler()
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(1_000) },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate, telemetry: nil, connectionDelayMilliseconds: 60_000),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )
        var reconnectCount = 0
        _ = core.rideSessionStateHandle.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0)
        core.handleTransportTermination(platformIdentifier: "A", error: nil, reconnect: { reconnectCount += 1 })
        core.disconnectAndScan()
        scheduler.runAll(includingCancelled: true)
        XCTAssertEqual(reconnectCount, 0)
        XCTAssertEqual(core.connectionSnapshot.readiness, .disconnected)
    }

    @MainActor
    func testBluetoothStateChangesClearPickerCancelReconnectAndRestoreScanning() {
        let scheduler = RecordingReconnectScheduler()
        var reconnectCount = 0
        var scanCount = 0
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(1_000) },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        core.handleTransportTermination(
            platformIdentifier: scriptedVescCandidate.platformIdentifier,
            error: nil,
            reconnect: { reconnectCount += 1 }
        )

        core.handleCentralState(.poweredOff, startScan: {})
        scheduler.runAll()

        XCTAssertEqual(core.phase, .bluetoothUnavailable(rawState: CBManagerState.poweredOff.rawValue))
        XCTAssertEqual(core.scanState, DevicePickerScanState(status: .bluetoothUnavailable, rows: []))
        XCTAssertEqual(reconnectCount, 0)

        core.handleCentralState(.unauthorized, startScan: { scanCount += 1 })
        XCTAssertEqual(core.phase, .bluetoothPermissionDenied)
        XCTAssertEqual(core.scanState, .permissionDenied)
        XCTAssertEqual(scanCount, 0)

        core.handleCentralState(.poweredOn, startScan: { scanCount += 1 })
        XCTAssertEqual(core.phase, .scanning)
        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(scanCount, 1)
    }

    @MainActor
    func testTransportTerminationExhaustionCannotRunAnOlderReconnect() {
        let scheduler = RecordingReconnectScheduler()
        let now = Mutex(MonotonicMilliseconds(1_000))
        var reconnectCount = 0
        let core = CutoutSessionCore(
            clock: MonotonicClock { now.withLock { $0 } },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        for deadline in [UInt64(1_200), 1_600, 2_400, 2_400] {
            core.handleTransportTermination(
                platformIdentifier: scriptedVescCandidate.platformIdentifier,
                error: nil,
                reconnect: { reconnectCount += 1 }
            )
            now.withLock { $0 = MonotonicMilliseconds(deadline) }
            scheduler.runAll()
        }

        XCTAssertEqual(core.phase, .failed(.connectFailed("unknown error")))
        XCTAssertFalse(core.isRecordOnlyConnection)
        XCTAssertEqual(core.connectionSnapshot.readiness, .failed)
        XCTAssertEqual(reconnectCount, 3)
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
    }

    func testComparedDisconnectRejectsAReplacementConnectionGeneration() throws {
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate, telemetry: nil, connectionDelayMilliseconds: 60_000
            ))
        let first = core.rideSessionStateHandle.beginConnectionAttempt(platformIdentifier: "first", nowMs: 0)
        let replacement = core.rideSessionStateHandle.beginConnectionAttempt(
            platformIdentifier: "replacement", nowMs: 1)

        XCTAssertFalse(core.disconnectAndScan(expectedGeneration: first.generation))
        XCTAssertEqual(core.rideSessionStateHandle.connectionAttemptSnapshot().token, replacement.token)
        XCTAssertTrue(core.disconnectAndScan(expectedGeneration: replacement.generation))
        XCTAssertNil(core.rideSessionStateHandle.connectionAttemptSnapshot().token)
    }

    func testRecordOnlyMissingCandidateReturnsFalse() {
        let core = CutoutSessionCore()

        XCTAssertFalse(core.recordOnly(platformIdentifier: "ios-local-missing", note: "unknown wheel"))
    }

    func testWriterCreationFailureRejectsRecordOnlyWithoutAnnouncingStarted() async throws {
        let blockedDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data("not a directory".utf8).write(to: blockedDirectory)
        defer { try? FileManager.default.removeItem(at: blockedDirectory) }
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate, telemetry: nil, connectionDelayMilliseconds: 0
            ))
        core.captureDirectoryForTesting = blockedDirectory
        let failed = expectation(description: "writer constructor failure is published")
        core.onCaptureEvent = { event in
            if case .started = event { XCTFail("failed writer must never be announced as started") }
            if case .failed = event { failed.fulfill() }
        }

        XCTAssertFalse(core.recordOnly(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        await fulfillment(of: [failed], timeout: 2)
        XCTAssertFalse(core.isRecordOnlyConnection)
        let state = core.rideSessionStateHandle.captureLifecycleSnapshot()
        XCTAssertEqual(state.attempt?.stage, .failed)
        XCTAssertTrue(state.canStart)
        XCTAssertTrue(state.canPair)
        let flushed = await core.flushCapture()
        XCTAssertFalse(flushed)
    }

    func testNativePairingCannotReplaceManualCaptureWithoutAppGuard() async throws {
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate, telemetry: nil, connectionDelayMilliseconds: 0
            ))
        let started = expectation(description: "manual writer starts")
        let finished = expectation(description: "manual writer completes")
        var url: URL?
        core.onCaptureEvent = { event in
            if case .started(_, let fileURL) = event {
                url = fileURL
                started.fulfill()
            }
            if case .finished = event { finished.fulfill() }
        }
        XCTAssertTrue(core.recordOnly(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        await fulfillment(of: [started], timeout: 2)
        let generation = core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.generation
        XCTAssertFalse(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        XCTAssertFalse(core.probe(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        XCTAssertEqual(core.rideSessionStateHandle.captureLifecycleSnapshot().attempt?.generation, generation)
        XCTAssertTrue(core.isRecordOnlyConnection)
        let flushed = await core.flushCapture()
        XCTAssertTrue(flushed)
        core.disconnectAndScan()
        await fulfillment(of: [finished], timeout: 2)
        if let url { try FileManager.default.removeItem(at: url) }
    }

    func testSuccessfulScriptedRecordOnlyFlushUsesTheRealWriter() async throws {
        let started = expectation(description: "real capture writer starts")
        let finished = expectation(description: "diagnostic capture releases location demand")
        var captureURL: URL?
        let rideMapState = MobileRideMapState()
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 0
            ),
            rideMapState: rideMapState
        )
        core.onCaptureEvent = { event in
            if case .started(let generation, let fileURL) = event {
                XCTAssertGreaterThan(generation.rawValue, 0)
                captureURL = fileURL
                started.fulfill()
            }
            if case .finished = event { finished.fulfill() }
        }

        XCTAssertTrue(
            core.recordOnly(
                platformIdentifier: scriptedVescCandidate.platformIdentifier,
                note: "durability test",
                annotations: ["durability=background"]
            ))
        await fulfillment(of: [started], timeout: 1)
        let url = try XCTUnwrap(captureURL)
        defer { try? FileManager.default.removeItem(at: url) }

        let locationEnvironment = MobileRideMapLocationEnvironmentDto(
            authorization: .whenInUse,
            servicesEnabled: true,
            temporarilyUnavailable: false
        )
        XCTAssertNil(rideMapState.currentSnapshot())
        XCTAssertEqual(
            try rideMapState.observeLocationEnvironment(locationEnvironment).demand,
            .record,
            "diagnostic capture requests location even without an active ride"
        )

        let flushSucceeded = await core.flushCapture()
        XCTAssertTrue(flushSucceeded)
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        XCTAssertGreaterThan((attributes[.size] as? NSNumber)?.uint64Value ?? 0, 0)
        let capture = try String(contentsOf: url, encoding: .utf8)
        XCTAssertTrue(capture.contains("durability=background"))
        XCTAssertTrue(capture.contains("capture_evidence=simulator_fixture"))
        XCTAssertFalse(capture.contains("capture_evidence=hardware_tested"))

        core.disconnectAndScan()
        await fulfillment(of: [finished], timeout: 1)
        XCTAssertEqual(
            try rideMapState.observeLocationEnvironment(locationEnvironment).demand,
            .idle,
            "finishing the capture releases its location demand"
        )
    }

    func testRejectedLabelReplacementKeepsRealCaptureSavable() async throws {
        let started = expectation(description: "writer starts")
        let finished = expectation(description: "writer durably completes")
        var captureURL: URL?
        var captureGeneration: CaptureGeneration?
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate, telemetry: nil, connectionDelayMilliseconds: 0
            ))
        XCTAssertEqual(core.captureDirectoryForTesting, FileManager.default.temporaryDirectory)
        XCTAssertNil(CutoutSessionCore(clock: MonotonicClock()).captureDirectoryForTesting)
        core.onCaptureEvent = { event in
            switch event {
            case .started(let generation, let fileURL):
                captureGeneration = generation
                captureURL = fileURL
                started.fulfill()
            case .finished: finished.fulfill()
            case .failed: XCTFail("Label rejection must not fail the capture")
            default: break
            }
        }
        XCTAssertTrue(
            core.recordOnly(
                platformIdentifier: scriptedVescCandidate.platformIdentifier,
                annotations: ["note=first", "note=second"]
            ))
        await fulfillment(of: [started], timeout: 2)
        let generation = try XCTUnwrap(captureGeneration)
        let url = try XCTUnwrap(captureURL)
        defer { try? FileManager.default.removeItem(at: url) }
        XCTAssertEqual(
            url.deletingLastPathComponent().resolvingSymlinksInPath(),
            FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
        )
        XCTAssertThrowsError(
            try core.changeCaptureLabel(
                generation: .init(rawValue: generation.rawValue + 1), action: .start(label: .lowBeamOn)
            ))
        XCTAssertEqual(
            try core.changeCaptureLabel(generation: generation, action: .start(label: .lowBeamOn)), [.lowBeamOn])
        XCTAssertFalse(core.annotateCapture(key: "note", value: "no room"))
        XCTAssertThrowsError(try core.changeCaptureLabel(generation: generation, action: .start(label: .lowBeamOff))) {
            error in
            guard case MobileCaptureAnnotationError.CapacityReached = error else {
                return XCTFail("Unexpected error: \(error)")
            }
        }
        let saved = await core.finishCapture()
        XCTAssertTrue(saved)
        await fulfillment(of: [finished], timeout: 2)
        let contents = try String(contentsOf: url, encoding: .utf8)
        XCTAssertTrue(contents.contains("capture_label=low_beam_on_start"))
        XCTAssertTrue(contents.contains("capture_label=low_beam_on_stop"))
        XCTAssertFalse(contents.contains("low_beam_off"))
        XCTAssertThrowsError(try core.changeCaptureLabel(generation: generation, action: .stop(label: .lowBeamOn)))
    }

    func testMusicObservationBeforeCaptureIsRetainedUntilWriterStarts() async {
        let started = expectation(description: "real capture writer starts")
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 0
            ))
        core.onCaptureEvent = { event in
            if case .started = event {
                started.fulfill()
            }
        }
        let observation = MobilePevcapMusicEventDto(
            provider: .appleMusic,
            trackId: "pre-capture-track",
            monotonicAtMs: 10,
            wallClockUnixMs: 1_700_000_000_010,
            clockUncertaintyMs: 5,
            rideSequence: nil
        )

        core.updateMusicCaptureObservation(observation)
        XCTAssertEqual(core.musicCaptureObservationForTesting, observation)
        XCTAssertTrue(core.recordOnly(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        await fulfillment(of: [started], timeout: 1)
        XCTAssertNil(core.musicCaptureObservationForTesting)
        core.disconnectAndScan()
    }

    @MainActor
    func testMusicCaptureAdmissionLeavesMainResponsiveWhileBleOwnerIsBlocked() async {
        let queue = DispatchQueue(label: "io.cutout.test-music-blocked-ble")
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        let core = CutoutSessionCore(clock: MonotonicClock(), bleQueue: queue)
        let observation = MobilePevcapMusicEventDto(
            provider: .appleMusic, trackId: "blocked-capture-track", monotonicAtMs: 10,
            wallClockUnixMs: 1_700_000_000_010, clockUncertaintyMs: 5, rideSequence: 7)
        queue.async {
            entered.signal()
            release.wait()
        }
        XCTAssertEqual(entered.wait(timeout: .now() + 2), .success)
        defer { release.signal() }
        // A bounded release prevents a failing old synchronous implementation from hanging the runner.
        DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(500)) { release.signal() }
        let started = ContinuousClock.now
        let submitted = expectation(description: "Main starts capture admission")
        var admitted = false
        let admission = Task { @MainActor in
            submitted.fulfill()
            let outcome = await core.updateMusicCaptureObservationAsync(observation, target: .noCapture)
            admitted = true
            return outcome
        }
        await fulfillment(of: [submitted], timeout: 2)
        XCTAssertFalse(admitted, "the BLE owner is still held; admission must remain pending")
        XCTAssertLessThan(
            started.duration(to: .now), .milliseconds(150),
            "required music capture admission must suspend rather than wait on Main")
        release.signal()
        let outcome = await admission.value
        XCTAssertEqual(outcome, .accepted)
        XCTAssertEqual(core.musicCaptureObservationForTesting, observation)
    }

    @MainActor
    func testDelayedMusicCaptureCannotMutateReplacementWriterOrItsContext() async throws {
        let capture = CaptureRecorderSpy()
        let core = CutoutSessionCore(clock: MonotonicClock(), captureRecorder: capture)
        let lifecycle = core.rideSessionStateHandle
        let first = try XCTUnwrap(lifecycle.beginCapture(origin: .manual))
        XCTAssertTrue(lifecycle.captureWriterStarted(generation: first))
        let originalTarget = lifecycle.musicCaptureTarget()
        XCTAssertTrue(lifecycle.retireCaptureWriter(generation: first))
        let second = try XCTUnwrap(lifecycle.beginCapture(origin: .manual))
        XCTAssertTrue(lifecycle.captureWriterStarted(generation: second))
        capture.currentGeneration = CaptureGeneration(rawValue: second.value)
        let oldObservation = MobilePevcapMusicEventDto(
            provider: .appleMusic, trackId: "old-capture-track", monotonicAtMs: 10,
            wallClockUnixMs: 1_700_000_000_010, clockUncertaintyMs: 5, rideSequence: 7)
        let replacementObservation = MobilePevcapMusicEventDto(
            provider: .appleMusic, trackId: "replacement-capture-track", monotonicAtMs: 20,
            wallClockUnixMs: 1_700_000_000_020, clockUncertaintyMs: 3, rideSequence: 2)
        capture.currentMusicObservation = replacementObservation
        for target in [originalTarget, .noCapture, .unavailable] {
            let outcome = await core.updateMusicCaptureObservationAsync(oldObservation, target: target)
            XCTAssertEqual(outcome, .rejected)
            XCTAssertEqual(capture.currentMusicObservation, replacementObservation)
            XCTAssertTrue(capture.musicObservations.isEmpty)
            XCTAssertEqual(capture.finishCount, 0)
            XCTAssertEqual(lifecycle.captureLifecycleSnapshot().attempt?.stage, .recording)
        }
        let accepted = await core.updateMusicCaptureObservationAsync(
            replacementObservation, target: lifecycle.musicCaptureTarget())
        XCTAssertEqual(accepted, .accepted)
        XCTAssertEqual(capture.musicObservations, [replacementObservation])
    }

    func testCaptureMusicContextTakesAndResetsTheLatestObservation() {
        var context = CaptureMusicContext()
        let observation = MobilePevcapMusicEventDto(
            provider: .appleMusic,
            trackId: "track-1",
            monotonicAtMs: 10,
            wallClockUnixMs: 1_700_000_000_010,
            clockUncertaintyMs: 5,
            rideSequence: nil
        )

        context.update(observation)
        XCTAssertEqual(context.current, observation)
        XCTAssertEqual(context.take(), observation)
        XCTAssertNil(context.current)
        context.update(observation)
        context.reset()
        XCTAssertNil(context.current)
    }

    func testObservedAdvertisementsReplaceDuplicatePeripheralRows() {
        let core = CutoutSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "device-ffe0",
                advertisedServiceUuids: []
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "device-ffe0",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        XCTAssertEqual(core.scanState.rows.map(\.id), ["ios-local-falcon"])
        XCTAssertEqual(core.scanState.sections.probeRecommended.map(\.title), ["device-ffe0"])
    }

    func testNotificationAdmissionForwardsTheDecodedConnectionAttempt() throws {
        var observedAttempts = [ConnectionAttemptToken?]()
        var observedSpeeds = [MobileRideMapSpeedObservationDto?]()
        let effects = CutoutSessionNotificationEffects(
            applyActions: { _ in [] },
            observeRideMapConnection: { attempt, _, speed in
                observedAttempts.append(attempt)
                observedSpeeds.append(speed)
            },
            persistBmsSamples: { _ in },
            reduceDisplayState: { state, snapshot, receivedAt, _ in
                state.reducing(snapshot: snapshot, receivedAt: receivedAt)
            }
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            notificationEffects: effects
        )
        let capturedAttempt = try XCTUnwrap(
            core.rideSessionStateHandle
                .beginConnectionAttempt(platformIdentifier: "captured", nowMs: 1)
                .token
        )
        let replacementAttempt = try XCTUnwrap(
            core.rideSessionStateHandle
                .beginConnectionAttempt(platformIdentifier: "replacement", nowMs: 2)
                .token
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(speed: speedValue(2_468)),
                speedObservation: MobileRideMapSpeedObservationDto(
                    millimetresPerSecond: 2_468, observedAtMs: 2),
                connectionAttempt: capturedAttempt
            ),
            receivedAt: MonotonicMilliseconds(3)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil),
            receivedAt: MonotonicMilliseconds(4)
        )

        XCTAssertEqual(observedAttempts.count, 2)
        XCTAssertEqual(observedAttempts[0], capturedAttempt)
        XCTAssertNil(observedAttempts[1])
        XCTAssertEqual(
            observedSpeeds[0],
            MobileRideMapSpeedObservationDto(millimetresPerSecond: 2_468, observedAtMs: 2))
        XCTAssertNil(observedSpeeds[1])
        XCTAssertEqual(core.connectionSnapshot.token, replacementAttempt)
    }

    @MainActor
    func testRideAdmissionRejectsDecodedStepFromReplacedConnectionAttempt() async throws {
        let state = MobileRideMapState()
        let connectionState = CutoutSessionStateHandle()

        func verify(_ identifier: String, atMs: UInt64) throws -> ConnectionAttemptToken {
            let token = try XCTUnwrap(
                connectionState.beginConnectionAttempt(platformIdentifier: identifier, nowMs: atMs).token
            )
            _ = connectionState.connectionLinkEstablished(token: token)
            _ = connectionState.observeConnectionNotification(
                token: token,
                bytes: Data([
                    2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101,
                    114, 104, 97, 115, 104, 0, 38, 208, 3,
                ])
            )
            _ = connectionState.resolveDeviceSession(
                token: token,
                identificationComplete: false,
                nowMs: atMs + 1
            )
            XCTAssertTrue(connectionState.verifiedConnectionAttemptIsCurrent(token: token))
            return token
        }

        let firstAttempt = try verify("first", atMs: 1)
        let replacementAttempt = try verify("replacement", atMs: 3)
        XCTAssertThrowsError(
            try state.observeTelemetryForVerifiedConnection(
                connectionState: connectionState,
                token: firstAttempt,
                atMs: 4,
                speedObservation: MobileRideMapSpeedObservationDto(
                    millimetresPerSecond: 1_234,
                    observedAtMs: 4
                )
            )
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .staleConnection)
        }
        XCTAssertTrue(connectionState.verifiedConnectionAttemptIsCurrent(token: replacementAttempt))
    }

    func testApplyNotificationStepMarksLiveAndUpdatesDisplayState() {
        let core = CutoutSessionCore()
        let snapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            operatingState: .riding,
            voltage: voltageValue(117_000),
            powerFlow: .negativeUnknown,
            batteryLevelEstimated: batteryLevelValue(77)
        )
        let step = CoreBluetoothSessionStep(operations: [], snapshot: snapshot)
        let receivedAt = MonotonicMilliseconds(42)

        core.applyNotificationStep(step, receivedAt: receivedAt)

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(
            EucRideScreenState(phase: core.phase, displayState: core.displayState).operatingState,
            .riding
        )
        XCTAssertEqual(core.displayState.telemetry?.speed, Speed(value: 1_234))
        XCTAssertEqual(core.displayState.telemetry?.powerFlow, .negativeUnknown)
        XCTAssertEqual(core.displayState.notificationCount, 1)
        XCTAssertEqual(core.displayState.lastUpdate, receivedAt)
    }

    func testRideMapStorageFailureDoesNotSuppressDisplayReduction() {
        let core = CutoutSessionCore(
            rideMapState: MobileRideMapState(storageUnavailable: "map database unavailable")
        )
        let snapshot = TelemetrySnapshot(
            speed: speedValue(2_468),
            operatingState: .riding,
            voltage: voltageValue(50_400)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: snapshot),
            receivedAt: MonotonicMilliseconds(84)
        )

        XCTAssertEqual(core.phase, .live)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 2_468)
        XCTAssertEqual(core.displayState.telemetry?.voltage, Voltage(value: 50_400))
        XCTAssertEqual(core.displayState.notificationCount, 1)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(84))
    }

    func testNotificationEffectsRunBeforeDisplayPublicationInOneCanonicalOrder() {
        var events = [String]()
        let effects = CutoutSessionNotificationEffects(
            applyActions: { _ in
                events.append("actions")
                return []
            },
            observeRideMapConnection: { _, _, _ in events.append("map") },
            persistBmsSamples: { _ in events.append("bms") },
            reduceDisplayState: { state, snapshot, receivedAt, updateKind in
                events.append("display")
                switch updateKind {
                case .linkUp:
                    return state.reducingLinkUpSnapshot(snapshot, receivedAt: receivedAt)
                case .notification:
                    return state.reducing(snapshot: snapshot, receivedAt: receivedAt)
                }
            }
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            notificationEffects: effects
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(speed: speedValue(1_234)),
                actions: [.event()]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(events, ["actions", "map", "bms", "display"])
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.notificationCount, 1)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(42))
    }

    func testCaptureWriterFailureIsAppliedAfterNotificationDisplayReduction() {
        var events = [String]()
        let capture = CaptureRecorderSpy()
        capture.onPublishFailure = { events.append("capture-failure") }
        let effects = CutoutSessionNotificationEffects(
            applyActions: { _ in
                events.append("actions")
                return [.failed]
            },
            observeRideMapConnection: { _, _, _ in events.append("map") },
            persistBmsSamples: { _ in events.append("bms") },
            reduceDisplayState: { state, snapshot, receivedAt, _ in
                events.append("display")
                return state.reducing(snapshot: snapshot, receivedAt: receivedAt)
            }
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) }),
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(),
                connectionDelayMilliseconds: 0
            ),
            notificationEffects: effects,
            captureRecorder: capture
        )
        XCTAssertTrue(core.recordOnly(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot(speed: speedValue(1_234))),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(events, ["actions", "map", "bms", "display", "capture-failure"])
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.phase, .live)
        XCTAssertEqual(capture.publishFailureCount, 1)
        XCTAssertEqual(capture.finishCount, 1)
    }

    func testLinkUpRunsSharedEffectsBeforeOneNonNotificationDisplayReduction() {
        var events = [String]()
        let effects = CutoutSessionNotificationEffects(
            applyActions: { _ in
                events.append("actions")
                return []
            },
            observeRideMapConnection: { _, _, _ in events.append("map") },
            persistBmsSamples: { _ in events.append("bms") },
            reduceDisplayState: { state, snapshot, receivedAt, updateKind in
                events.append("display")
                switch updateKind {
                case .linkUp:
                    return state.reducingLinkUpSnapshot(snapshot, receivedAt: receivedAt)
                case .notification:
                    return state.reducing(snapshot: snapshot, receivedAt: receivedAt)
                }
            }
        )
        let capture = CaptureRecorderSpy()
        let receivedAt = MonotonicMilliseconds(100)
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { receivedAt }),
            notificationEffects: effects,
            captureRecorder: capture
        )
        let snapshot = TelemetrySnapshot(at: receivedAt, speed: speedValue(2_468))

        core.applyLinkUpStep(CoreBluetoothSessionStep(operations: [], snapshot: snapshot))

        XCTAssertEqual(events, ["actions", "map", "bms", "display"])
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 2_468)
        XCTAssertEqual(core.displayState.telemetry, snapshot)
        XCTAssertEqual(core.displayState.notificationCount, 0)
        XCTAssertEqual(core.displayState.lastUpdate, receivedAt)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.phase, .subscribing)
    }

    func testApplyNotificationStepPublishesDisplayStateOnMainThread() {
        nonisolated(unsafe) let core = CutoutSessionCore()
        let published = expectation(description: "display state published")
        core.onDisplayStateChange = { _ in
            XCTAssertTrue(Thread.isMainThread)
            published.fulfill()
        }

        DispatchQueue.global().async {
            core.applyNotificationStep(
                CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot()),
                receivedAt: MonotonicMilliseconds(42)
            )
        }

        wait(for: [published], timeout: 1.0)
    }

    @MainActor
    func testForegroundBmsBurstRetainsOnlyLatestPresentationWhileMainIsBlocked() {
        nonisolated(unsafe) let core = CutoutSessionCore()
        let topology = BmsTopology(
            layoutLabel: "unverified", seriesGroupCount: nil, parallelCount: nil,
            packCount: 1, bmsCount: 1, confidence: .unverified)
        let latest = expectation(description: "latest BMS projection reaches presentation")
        var published: [BmsSnapshot?] = []
        core.onBmsSnapshotChange = { snapshot in
            XCTAssertTrue(Thread.isMainThread)
            published.append(snapshot)
            if snapshot?.voltage?.value == 100 { latest.fulfill() }
        }
        let submitted = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            for index in 1...100 {
                core.applyNotificationStep(
                    CoreBluetoothSessionStep(
                        operations: [], snapshot: TelemetrySnapshot(),
                        actions: [
                            .withBmsSnapshot(BmsSnapshot(topology: topology, voltage: Voltage(value: Int32(index))))
                        ]
                    ),
                    receivedAt: MonotonicMilliseconds(UInt64(index))
                )
            }
            submitted.signal()
        }

        XCTAssertEqual(submitted.wait(timeout: .now() + 3), .success)
        XCTAssertTrue(published.isEmpty, "producer must complete while Main is unavailable")
        wait(for: [latest], timeout: 2)
        XCTAssertEqual(published.count, 1, "replaceable BMS presentation must not retain one Main closure per packet")
        XCTAssertEqual(published.last.flatMap { $0 }?.voltage?.value, 100)
        XCTAssertEqual(core.bmsSnapshot?.voltage?.value, 100)
        XCTAssertEqual(core.displayState.notificationCount, 100, "all material protocol effects still run")
        withExtendedLifetime(core) {}
    }

    @MainActor
    func testForegroundSettingsBurstRetainsLatestAndPreservesPhaseBeforeSettings() {
        nonisolated(unsafe) let core = CutoutSessionCore()
        var phases: [SessionConnectionPhase] = []
        var settingsPhases: [SessionConnectionPhase?] = []
        core.onPhaseChange = { phases.append($0.phase) }
        core.onSettingsChange = { _ in settingsPhases.append(phases.last) }
        let submitted = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            let step = CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot())
            for time in 1...50 {
                core.applyLinkUpStep(step)
                core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(UInt64(time)))
            }
            submitted.signal()
        }

        XCTAssertEqual(submitted.wait(timeout: .now() + 3), .success)
        XCTAssertTrue(settingsPhases.isEmpty)
        let drained = expectation(description: "foreground presentation mailboxes drain")
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(100)) { drained.fulfill() }
        wait(for: [drained], timeout: 2)
        XCTAssertEqual(phases.count, 100, "ordered connection phase effects remain lossless")
        XCTAssertEqual(phases.last, .live)
        XCTAssertEqual(settingsPhases, [.live], "latest settings follows the phase that admits it")
        XCTAssertEqual(core.displayState.notificationCount, 50)
        withExtendedLifetime(core) {}
    }

    func testUnchangedLivePhaseDoesNotRepublishSettingsForEachNotification() {
        let core = CutoutSessionCore()
        var phases: [SessionConnectionPhase] = []
        var settingsCount = 0
        core.onPhaseChange = { phases.append($0.phase) }
        core.onSettingsChange = { _ in settingsCount += 1 }
        let step = CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot())

        for time in 1...100 {
            core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(UInt64(time)))
        }

        XCTAssertEqual(phases, [.live])
        XCTAssertEqual(settingsCount, 1)
        XCTAssertEqual(core.displayState.notificationCount, 100, "all notification effects still run")

        core.applyLinkUpStep(step)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(101))
        XCTAssertEqual(phases, [.live, .subscribing, .live], "real phase transitions remain ordered")
        XCTAssertEqual(settingsCount, 3)
    }

    @MainActor
    func testInactiveNotificationProcessingRetainsDataWithoutForegroundPublications() async throws {
        let core = CutoutSessionCore()
        var displays: [RideDisplayState] = []
        var settingsCount = 0
        core.onDisplayStateChange = { displays.append($0) }
        core.onSettingsChange = { _ in settingsCount += 1 }
        core.setPresentationActive(false)
        let step = CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot())
        for index in 1...100 {
            core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(UInt64(index)))
        }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertTrue(displays.isEmpty)
        XCTAssertEqual(settingsCount, 0)
        XCTAssertEqual(core.displayState.notificationCount, 100)
        core.setPresentationActive(true)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(displays.last?.notificationCount, 100)
        XCTAssertEqual(settingsCount, 1)
    }

    @MainActor
    func testInactiveGpsDecisionsRemainDurableAndCatchUpWithLatestAcceptance() async throws {
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 1_000, musicHistoryPolicy: .disabled)
        let core = CutoutSessionCore(
            clock: MonotonicClock(now: { MonotonicMilliseconds(3_000) }),
            wallClock: { Date(timeIntervalSince1970: 1_700_000_003) },
            rideMapState: state
        )
        var decisions: [(MobileRideMapSnapshotDto, MobileRideMapDecisionDto)] = []
        core.onRideMapDecisionChange = { decisions.append(($0, $1)) }
        core.setPresentationActive(false)
        for index in 0..<2 {
            core.handlePhoneLocationUpdate(
                PhoneLocationUpdate(
                    receiptMonotonic: MonotonicMilliseconds(3_000),
                    receiptWallClock: Date(timeIntervalSince1970: 1_700_000_003),
                    samples: [
                        MobilePhoneLocationSampleDto(
                            wallClockUnixMs: 1_700_000_001_000 + UInt64(index) * 1_000,
                            sourceTimestampUnixSeconds: nil,
                            latitudeDegrees: 40.0 + Double(index) * 0.00002, longitudeDegrees: -105,
                            altitudeMeters: 1_600, horizontalAccuracyMeters: 3, verticalAccuracyMeters: nil,
                            speedMetersPerSecond: 2, speedAccuracyMetersPerSecond: nil,
                            courseDegrees: nil, courseAccuracyDegrees: nil
                        )
                    ]
                ))
        }
        let durableDeadline = ContinuousClock.now + .seconds(2)
        while state.currentSnapshot()?.summary.pointCount != 2 && ContinuousClock.now < durableDeadline {
            try await Task.sleep(for: .milliseconds(1))
        }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(state.currentSnapshot()?.summary.pointCount, 2, "hidden UI must not stop recording")
        XCTAssertTrue(decisions.isEmpty, "durable GPS decisions must not enqueue hidden presentation")
        decisions.removeAll()
        core.setPresentationActive(true)
        try await Task.sleep(for: .milliseconds(50))
        guard let latest = decisions.last, case .accepted(let point) = latest.1 else {
            return XCTFail("activation must deliver the latest Accepted decision for route reprojection")
        }
        XCTAssertEqual(latest.0.summary.pointCount, 2)
        XCTAssertEqual(point.sequence, 1)
        XCTAssertEqual(point.monotonicMs, 2_000)
    }

    @MainActor
    func testScenePresentationChangeDoesNotWaitForBlockedBleQueue() {
        let queue = DispatchQueue(label: "io.cutout.test-scene-blocked-ble")
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        let core = CutoutSessionCore(clock: MonotonicClock(), bleQueue: queue)
        queue.async {
            entered.signal()
            release.wait()
        }
        XCTAssertEqual(entered.wait(timeout: .now() + 2), .success)
        defer { release.signal() }
        DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(250)) { release.signal() }
        let clock = ContinuousClock()
        let started = clock.now
        core.setPresentationActive(false)
        XCTAssertLessThan(
            started.duration(to: clock.now), .milliseconds(100), "scene gating must not wait for recording backpressure"
        )
    }

    func testNewRustConnectionAttemptPublishesLivePhaseEvenWithoutAnIntermediateNativePhase() throws {
        let core = CutoutSessionCore()
        let state = core.rideSessionStateHandle
        var phases: [SessionConnectionPhase] = []
        var settingsGenerations: [UInt64] = []
        core.onPhaseChange = { phases.append($0.phase) }
        core.onSettingsChange = { settingsGenerations.append($0.connection.generation) }
        let step = CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot())

        let first = state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 1)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1))
        let second = state.beginConnectionAttempt(platformIdentifier: "B", nowMs: 2)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(2))
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(3))

        XCTAssertEqual(phases, [.live, .live])
        XCTAssertEqual(settingsGenerations, [first.generation, second.generation])
    }

    func testDisplayPublicationThrottleUsesMonotonicTime() {
        let clock = TestMonotonicClock(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(clock: MonotonicClock(now: { clock.now }))
        var publicationCount = 0
        core.onDisplayStateChange = { _ in publicationCount += 1 }

        let step = CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot())
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_000))

        clock.now = MonotonicMilliseconds(1_200)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_200))

        clock.now = MonotonicMilliseconds(1_333)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_333))

        XCTAssertEqual(publicationCount, 2)
    }

    func testDisplayPublicationThrottleDoesNotDelaySafetyWarningTransitions() throws {
        let clock = TestMonotonicClock(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(clock: MonotonicClock(now: { clock.now }))
        var publishedStates: [RideDisplayState] = []
        core.onDisplayStateChange = { publishedStates.append($0) }

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(500))
            ),
            receivedAt: MonotonicMilliseconds(1_000)
        )

        clock.now = MonotonicMilliseconds(1_200)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(800))
            ),
            receivedAt: MonotonicMilliseconds(1_200)
        )

        XCTAssertEqual(publishedStates.count, 2)
        XCTAssertEqual(
            EucRideScreenState(
                phase: .live,
                displayState: try XCTUnwrap(publishedStates.last)
            ).warningState.severity,
            .reduceAcceleration
        )

        clock.now = MonotonicMilliseconds(1_250)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(500))
            ),
            receivedAt: MonotonicMilliseconds(1_250)
        )

        XCTAssertEqual(publishedStates.count, 3)
        XCTAssertEqual(
            EucRideScreenState(
                phase: .live,
                displayState: try XCTUnwrap(publishedStates.last)
            ).warningState.severity,
            .normal
        )
    }

    func testVescRideSnapshotKeepsRideCriticalFieldsTyped() {
        let snapshot = VescRideSnapshot(
            title: "Fungineers X7",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            warning: .dutyPushback,
            boardSpeed: speedValue(19_000),
            dutyCycle: dutyCycle(820),
            dutyHeadroom: batteryLevelValue(18),
            batteryCurrent: batteryCurrentValue(38_000),
            powerFlow: .discharge,
            motorCurrent: phaseCurrentValue(71_000),
            boardAngle: angleValue(-18),
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(snapshot.vehicleKind, .float)
        XCTAssertEqual(snapshot.subProtocol, .refloat)
        XCTAssertEqual(snapshot.controllerState, .unknown)
        XCTAssertEqual(snapshot.warning, .dutyPushback)
        XCTAssertEqual(snapshot.boardSpeed, speedValue(19_000))
        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(820))
        XCTAssertEqual(snapshot.dutyHeadroom, batteryLevelValue(18))
        XCTAssertEqual(snapshot.batteryCurrent, batteryCurrentValue(38_000))
        XCTAssertEqual(snapshot.powerFlow, .discharge)
        XCTAssertEqual(snapshot.motorCurrent, phaseCurrentValue(71_000))
        XCTAssertEqual(snapshot.boardAngle, angleValue(-18))
        XCTAssertEqual(snapshot.controllerTemperature, temperatureValue(54_000))
        XCTAssertEqual(snapshot.motorTemperature, temperatureValue(49_000))
    }

    func testVescRideSnapshotUsesProtocolDecodedSafetyState() throws {
        let snapshot = try XCTUnwrap(
            VescRideSnapshot(
                displayState: RideDisplayState(
                    telemetry: TelemetrySnapshot(
                        speed: speedValue(19_000),
                        operatingState: .riding,
                        vescOperatingMode: .handtest,
                        vescWarning: .dutyPushback,
                        vescStopReason: .pitch
                    )
                ),
                title: nil
            ))

        XCTAssertEqual(snapshot.warning, .dutyPushback)
        XCTAssertEqual(snapshot.stopReason, .pitch)
        XCTAssertEqual(snapshot.operatingMode, .handtest)
    }

    func testVescRideSnapshotOwnsBatteryReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            batteryLevelReported: batteryLevelValue(72),
            batteryCurrent: batteryCurrentValue(38_000)
        )

        XCTAssertEqual(
            snapshot.batteryReadback,
            .reported(level: "72", current: "38.0")
        )
    }

    func testVescRideSnapshotOwnsBoardAngleReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            boardAngle: angleValue(-18_000),
            balanceAngle: angleValue(500)
        )

        XCTAssertEqual(
            snapshot.boardAngleReadback,
            .available(orientation: .noseDown, balanceAngle: "0.5")
        )
    }

    func testVescRideSnapshotOwnsMotorTemperatureReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(
            snapshot.controllerTemperatureReadback,
            .available(motorTemperature: "49.0")
        )
    }

    func testRideHeroReadoutOwnsVescSpeedFreshnessAndSeverity() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            warning: .dutyPushback,
            boardSpeed: speedValue(19_000),
            lastUpdate: MonotonicMilliseconds(1_000)
        )

        XCTAssertEqual(
            RideHeroReadout.vesc(
                snapshot: snapshot,
                now: MonotonicMilliseconds(4_000)
            ),
            .available(
                value: "42.5",
                unit: "mph",
                freshness: .stale,
                severity: .caution
            )
        )
    }

    func testTelemetryThermalReadbackOwnsSensorFormatting() {
        let snapshot = TelemetrySnapshot(
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(
            snapshot.thermalReadback,
            .controllerMotor(controller: "54", motor: "49")
        )
    }

    func testVescVehicleKindDoesNotImplySubProtocol() {
        let snapshot = VescRideSnapshot(
            title: "VESC Bike",
            vehicleKind: .bike,
            subProtocol: .generic,
            controllerState: .unknown
        )

        XCTAssertEqual(snapshot.vehicleKind, .bike)
        XCTAssertEqual(snapshot.subProtocol, .generic)

        let bike = VescRideSnapshot(
            title: "VESC Bike",
            vehicleKind: .bike,
            subProtocol: .bike,
            controllerState: .unknown
        )
        XCTAssertEqual(bike.vehicleKind, .bike)
        XCTAssertEqual(bike.subProtocol, .bike)

        let eskate = VescRideSnapshot(
            title: "VESC Skateboard",
            vehicleKind: .skateboard,
            subProtocol: .eskate,
            controllerState: .unknown
        )
        XCTAssertEqual(eskate.vehicleKind, .skateboard)
        XCTAssertEqual(eskate.subProtocol, .eskate)
    }

    func testVescRideSnapshotProjectsLiveDisplayTelemetryWithoutInventingSpeed() throws {
        let telemetry = TelemetrySnapshot(
            voltage: voltageValue(75_400),
            batteryCurrent: batteryCurrentValue(38_000),
            motorCurrent: phaseCurrentValue(71_000),
            powerFlow: .discharge,
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: "Little FOCer BT"))

        XCTAssertEqual(snapshot.title, "Little FOCer BT")
        XCTAssertEqual(snapshot.vehicleKind, .float)
        XCTAssertEqual(snapshot.subProtocol, .generic)
        XCTAssertEqual(snapshot.controllerState, .unknown)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertEqual(snapshot.batteryVoltage, voltageValue(75_400))
        XCTAssertEqual(snapshot.batteryCurrent, batteryCurrentValue(38_000))
        XCTAssertEqual(snapshot.powerFlow, .discharge)
        XCTAssertEqual(snapshot.motorCurrent, phaseCurrentValue(71_000))
        XCTAssertEqual(snapshot.controllerTemperature, temperatureValue(54_000))
        XCTAssertEqual(snapshot.motorTemperature, temperatureValue(49_000))
    }

    func testVescRideSnapshotProjectsBatteryLevelAndUpdateTime() throws {
        let telemetry = TelemetrySnapshot(
            operatingState: .parked,
            voltage: voltageValue(61_000),
            batteryLevelReported: batteryLevelValue(72),
            batteryLevelEstimated: batteryLevelValue(70)
        )
        let displayState = RideDisplayState(
            telemetry: telemetry,
            notificationCount: 1,
            lastUpdate: MonotonicMilliseconds(900)
        )

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.batteryLevelReported, batteryLevelValue(72))
        XCTAssertEqual(snapshot.batteryLevelEstimated, batteryLevelValue(70))
        XCTAssertEqual(snapshot.lastUpdate, MonotonicMilliseconds(900))
        XCTAssertEqual(
            snapshot.updateAge(
                at: MonotonicMilliseconds(1_000),
                staleAfter: MonotonicMilliseconds(250)
            ),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(100), freshness: .fresh)
        )
        XCTAssertEqual(
            snapshot.updateAge(
                at: MonotonicMilliseconds(1_300),
                staleAfter: MonotonicMilliseconds(250)
            ),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(400), freshness: .stale)
        )
    }

    func testVescRideSnapshotDerivesDutyHeadroomFromLiveDutyCycle() throws {
        let balancedTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(0))
        let idleNoiseTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(10))
        let loadedTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))

        let balanced = try XCTUnwrap(
            VescRideSnapshot(
                displayState: RideDisplayState(telemetry: balancedTelemetry, notificationCount: 1),
                title: nil
            ))
        let idleNoise = try XCTUnwrap(
            VescRideSnapshot(
                displayState: RideDisplayState(telemetry: idleNoiseTelemetry, notificationCount: 1),
                title: nil
            ))
        let loaded = try XCTUnwrap(
            VescRideSnapshot(
                displayState: RideDisplayState(telemetry: loadedTelemetry, notificationCount: 1),
                title: nil
            ))

        XCTAssertEqual(balanced.dutyCycle, dutyCycle(0))
        XCTAssertEqual(balanced.dutyHeadroom, batteryLevelValue(100))
        XCTAssertEqual(idleNoise.dutyCycle, dutyCycle(10))
        XCTAssertEqual(idleNoise.dutyHeadroom, batteryLevelValue(100))
        XCTAssertEqual(loaded.dutyCycle, dutyCycle(230))
        XCTAssertEqual(loaded.dutyHeadroom, batteryLevelValue(77))
        XCTAssertEqual(loaded.dutyHeadroomMetricValue, .available(display: "77", accessibility: "77"))
        XCTAssertEqual(
            loaded.dutyHeadroomProgressMetricValue,
            .available(display: "77%", accessibility: "77%")
        )
        XCTAssertEqual(try XCTUnwrap(loaded.dutyHeadroomProgress), 0.77, accuracy: 0.001)
    }

    func testVescRideSnapshotShowsParkedHeadroomWithProgress() throws {
        let telemetry = TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(10))

        let snapshot = try XCTUnwrap(
            VescRideSnapshot(
                displayState: RideDisplayState(telemetry: telemetry, notificationCount: 1),
                title: nil
            ))

        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(10))
        XCTAssertEqual(snapshot.dutyHeadroom, batteryLevelValue(100))
        XCTAssertEqual(snapshot.dutyHeadroomApplicability, .available)
        XCTAssertEqual(
            snapshot.dutyHeadroomMetricValue,
            .available(display: "100", accessibility: "100")
        )
        XCTAssertEqual(
            snapshot.dutyHeadroomProgressMetricValue,
            .available(display: "100%", accessibility: "100%")
        )
        XCTAssertEqual(snapshot.dutyHeadroomProgress, 1)
    }

    func testVescRideSnapshotKeepsMissingDutyHeadroomUnavailable() throws {
        let telemetry = TelemetrySnapshot(operatingState: .parked, voltage: voltageValue(62_800))

        let snapshot = try XCTUnwrap(
            VescRideSnapshot(
                displayState: RideDisplayState(telemetry: telemetry, notificationCount: 1),
                title: nil
            ))

        XCTAssertNil(snapshot.dutyCycle)
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertEqual(snapshot.dutyHeadroomApplicability, .unavailable)
        XCTAssertEqual(snapshot.dutyHeadroomMetricValue, .unavailable)
        XCTAssertEqual(snapshot.dutyHeadroomProgressMetricValue, .unavailable)
        XCTAssertNil(snapshot.dutyHeadroomProgress)
    }

    func testVescRideSnapshotProjectsFootpadFromSharedTelemetry() throws {
        let footpad = FootpadTelemetry(state: 3, adc1Milliunits: 1_250, adc2Milliunits: 875)
        let telemetry = TelemetrySnapshot(footpad: footpad)
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.footpad, footpad)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertNil(snapshot.boardAngle)
    }

    func testFootpadTelemetryExposesTypedAdcValues() {
        let footpad = FootpadTelemetry(state: 3, adc1Milliunits: 1_250, adc2Milliunits: nil)

        XCTAssertEqual(
            footpad.adc1MetricValue,
            .available(display: "1.25", accessibility: "1.25, available")
        )
        XCTAssertEqual(footpad.adc2MetricValue, .unavailable)
        XCTAssertEqual(footpad.stateDisplayText, "state 3")
        XCTAssertEqual(
            footpad.summaryText,
            "footpad state 3 · adc1 left 1.25 · adc2 right unavailable"
        )
    }

    func testFootpadTelemetryUsesTypedContactStateForDisplayAndAccessibility() {
        let cases: [(UInt8, FootpadContactState, String)] = [
            (0, .none, "not pressed"),
            (1, .left, "left pressed"),
            (2, .right, "right pressed"),
            (3, .both, "both pressed"),
        ]

        for (rawState, contactState, expectedDisplayText) in cases {
            let footpad = FootpadTelemetry(
                state: rawState,
                contactState: contactState,
                adc1Milliunits: 1_250,
                adc2Milliunits: 875
            )

            XCTAssertEqual(footpad.stateDisplayText, expectedDisplayText)
            XCTAssertEqual(
                footpad.accessibilityValue,
                "left / adc1, 1.25, available, right / adc2, 0.88, available, \(expectedDisplayText)"
            )
            XCTAssertEqual(
                footpad.summaryText,
                "footpad \(expectedDisplayText) · adc1 left 1.25 · adc2 right 0.88"
            )
        }
    }

    func testFootpadTelemetryKeepsZeroAdcAvailable() {
        let footpad = FootpadTelemetry(state: 0, adc1Milliunits: 0, adc2Milliunits: 0)

        XCTAssertEqual(
            footpad.adc1MetricValue,
            .available(display: "0.00", accessibility: "0.00, available")
        )
        XCTAssertEqual(
            footpad.adc2MetricValue,
            .available(display: "0.00", accessibility: "0.00, available")
        )
    }

    func testFootpadPresentationCopyResolvesFromThePackageCatalog() {
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.state", value: nil, table: "Localizable"),
            "state %lld"
        )
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.accessibility.summary", value: nil, table: "Localizable"),
            "%1$@, %2$@, %3$@, %4$@, %5$@"
        )
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.title", value: nil, table: "Localizable"),
            "Footpad"
        )
    }

    func testVescRideSnapshotProjectsAngleOnlyTelemetry() throws {
        let telemetry = TelemetrySnapshot(pitch: angleValue(14_200))
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.boardAngle, angleValue(14_200))
        XCTAssertNil(snapshot.batteryVoltage)
    }

    func testVescRideSnapshotDoesNotUseUnverifiedFactsForLiveDefaults() throws {
        let telemetry = TelemetrySnapshot(voltage: voltageValue(62_800))
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.title, VescRideSnapshot.defaultTitle)
        XCTAssertEqual(snapshot.subProtocol, .generic)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertNil(snapshot.boardAngle)
        XCTAssertNil(snapshot.controllerTemperature)
        XCTAssertNil(snapshot.motorTemperature)
        XCTAssertNotEqual(snapshot.title, "Fungineers X7")
    }

    func testSpeedObservationRemainsStickyAcrossTelemetryWithoutSpeed() {
        let core = CutoutSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            voltage: voltageValue(117_000),
            batteryLevelEstimated: batteryLevelValue(77)
        )
        let batteryOnlySnapshot = TelemetrySnapshot(
            voltage: voltageValue(116_500),
            batteryLevelEstimated: batteryLevelValue(76)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: speedSnapshot),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: batteryOnlySnapshot),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.telemetry?.voltage, Voltage(value: 116_500))
        XCTAssertEqual(core.displayState.notificationCount, 2)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(43))
    }

    func testNotificationWithoutSnapshotAdvancesLastUpdate() {
        let core = CutoutSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            voltage: voltageValue(117_000),
            batteryLevelEstimated: batteryLevelValue(77)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: speedSnapshot),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil),
            receivedAt: MonotonicMilliseconds(99)
        )

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.notificationCount, 2)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(99))
    }

    func testFaultHistoryReadbackUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let readback = FaultHistoryReadback.faultSince(
            FaultHistoryEntry(
                code: FaultCode.unknown(id: 0x0040, value: 1),
                source: .reported,
                quality: .known,
                verification: .hardwareVerified
            ),
            sinceDistance: Distance(value: 61_456_941)
        )
        var observedReadbacks: [FaultHistoryReadback?] = []
        core.onFaultHistoryReadbackChange = { observedReadbacks.append($0) }

        let action = SessionAction.withFaultHistoryReadback(readback)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.faultHistoryReadback, readback)
        XCTAssertEqual(core.faultHistoryReadback?.availability, .available)
        XCTAssertEqual(observedReadbacks, [readback])

        core.disconnectAndScan()

        XCTAssertNil(core.faultHistoryReadback)
        XCTAssertEqual(observedReadbacks, [readback, nil])
    }

    func testFaultHistoryReadbackConstructorsKeepNoFaultEvidenceExplicit() {
        let distance = Distance(value: 61_456_941)
        let noFault = FaultHistoryReadback.noFaultSince(distance)
        let unavailable = FaultHistoryReadback.unavailable()
        let unsupported = FaultHistoryReadback.unsupported()

        XCTAssertEqual(noFault.availability, .available)
        XCTAssertNil(noFault.lastFault)
        XCTAssertEqual(noFault.sinceDistance, distance)
        XCTAssertEqual(unavailable.availability, .unavailable)
        XCTAssertNil(unavailable.lastFault)
        XCTAssertNil(unavailable.sinceDistance)
        XCTAssertEqual(unsupported.availability, .unsupported)
        XCTAssertNil(unsupported.lastFault)
        XCTAssertNil(unsupported.sinceDistance)
    }

    func testFaultHistoryGeneratedReadbackStripsPayloadWhenUnavailable() {
        let distance = DistanceReading(
            value: Distance(value: 61_456_941),
            source: .reported,
            quality: .known,
            verification: .sourceVerified
        )
        let unavailable = FaultHistoryReadback(
            MobileFaultHistoryReadbackDto(
                availability: .unavailable,
                lastFault: nil,
                sinceDistance: distance
            )
        )
        let unsupported = FaultHistoryReadback(
            MobileFaultHistoryReadbackDto(
                availability: .unsupported,
                lastFault: nil,
                sinceDistance: distance
            )
        )

        XCTAssertEqual(unavailable, FaultHistoryReadback.unavailable())
        XCTAssertEqual(unsupported, FaultHistoryReadback.unsupported())
    }

    func testBmsSnapshotUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "unknown BMS topology",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            ),
            energyPercent: BatteryLevel(value: 72),
            voltage: Voltage(value: 81_600),
            current: BatteryCurrent(value: -1_250),
            highestTemperature: Temperature(value: 37_800)
        )
        var observedSnapshots: [BmsSnapshot?] = []
        core.onBmsSnapshotChange = { observedSnapshots.append($0) }

        let action = SessionAction.withBmsSnapshot(snapshot)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.bmsSnapshot, snapshot)
        XCTAssertEqual(core.bmsSnapshot?.topology.confidence, .unverified)
        XCTAssertEqual(observedSnapshots, [snapshot])

        core.disconnectAndScan()

        XCTAssertNil(core.bmsSnapshot)
        XCTAssertEqual(observedSnapshots, [snapshot, nil])
    }

    func testBmsStorageBatchUsesOnlyDecodedRawEvents() {
        let samples = bmsStorageSamples(
            observations: [
                BmsRawVoltageObservation(
                    eventSequence: 7,
                    observedAtMilliseconds: 1_000,
                    observationIndex: 45,
                    packIndex: 1,
                    packObservationIndex: 15,
                    voltage: Voltage(value: 4_209)
                )
            ],
            wallClockMilliseconds: 2_000,
            sessionIdentifier: "test-session"
        )

        XCTAssertEqual(samples.count, 1)
        XCTAssertEqual(samples[0].sessionIdentifier, "test-session")
        XCTAssertEqual(samples[0].eventSequence, 7)
        XCTAssertEqual(samples[0].monotonicMilliseconds, 1_000)
        XCTAssertEqual(samples[0].wallClockMilliseconds, 2_000)
        XCTAssertEqual(samples[0].observationIndex, 45)
        XCTAssertEqual(samples[0].packIndex, 1)
        XCTAssertEqual(samples[0].packObservationIndex, 15)
        XCTAssertEqual(samples[0].voltage, Voltage(value: 4_209))
    }

    func testBmsStorageSamplesKeepEqualReadingsFromDistinctDecodedEvents() {
        let samples = bmsStorageSamples(
            observations: [
                BmsRawVoltageObservation(
                    eventSequence: 1,
                    observedAtMilliseconds: 1_000,
                    observationIndex: 0,
                    packIndex: nil,
                    packObservationIndex: nil,
                    voltage: Voltage(value: 4_209)
                ),
                BmsRawVoltageObservation(
                    eventSequence: 2,
                    observedAtMilliseconds: 1_000,
                    observationIndex: 0,
                    packIndex: nil,
                    packObservationIndex: nil,
                    voltage: Voltage(value: 4_209)
                ),
            ],
            wallClockMilliseconds: 2_000,
            sessionIdentifier: "test-session"
        )

        XCTAssertEqual(samples.map(\.eventSequence), [1, 2])
        XCTAssertEqual(samples.map(\.voltage), [Voltage(value: 4_209), Voltage(value: 4_209)])
    }

    func testBmsSnapshotUsesRustOwnedProjectionWithoutReaggregation() {
        let core = CutoutSessionCore()
        let metadataPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 2,
            pageKind: "metadata",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            current: BatteryCurrent(value: 0)
        )
        let cellPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 3,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            cellDelta: VoltageDelta(value: 12),
            lowestGroupIndex: 1,
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090), alertLevel: .warning),
                BmsGroupSnapshot(index: 2, voltage: Voltage(value: 4_102)),
            ]
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(metadataPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(cellPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(core.bmsSnapshot?.pageSelector, 3)
        XCTAssertEqual(core.bmsSnapshot?.pageKind, "cell voltage")
        XCTAssertEqual(core.bmsSnapshot?.topology.layoutLabel, "8 observed BMS groups")
        XCTAssertNil(core.bmsSnapshot?.voltage)
        XCTAssertNil(core.bmsSnapshot?.current)
        XCTAssertEqual(core.bmsSnapshot?.cellDelta, VoltageDelta(value: 12))
        XCTAssertEqual(core.bmsSnapshot?.groups.count, 2)
    }

    func testBmsSnapshotReplacesPriorRustProjection() {
        let core = CutoutSessionCore()
        let topology = BmsTopology(
            layoutLabel: "unverified", seriesGroupCount: nil, parallelCount: nil, packCount: 1, bmsCount: 1,
            confidence: .unverified)
        func receive(_ snapshot: BmsSnapshot, at: UInt64) {
            core.applyNotificationStep(
                CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(snapshot)]),
                receivedAt: MonotonicMilliseconds(at)
            )
        }
        receive(
            BmsSnapshot(
                topology: topology, pageSelector: 6, cellDelta: VoltageDelta(value: 0), lowestGroupIndex: 46,
                observedGroupCount: 1, highestGroupIndex: 46,
                groups: [BmsGroupSnapshot(index: 46, voltage: Voltage(value: 4_200))]), at: 1)
        receive(
            BmsSnapshot(
                topology: topology, pageSelector: 2, cellDelta: VoltageDelta(value: 20), lowestGroupIndex: 16,
                observedGroupCount: 2, highestGroupIndex: 46,
                groups: [BmsGroupSnapshot(index: 16, voltage: Voltage(value: 4_180))]), at: 2)
        XCTAssertEqual(core.bmsSnapshot?.cellDelta, VoltageDelta(value: 20))
        XCTAssertEqual(core.bmsSnapshot?.lowestGroupIndex, 16)
        XCTAssertEqual(core.bmsSnapshot?.highestGroupIndex, 46)
        XCTAssertEqual(core.bmsSnapshot?.observedGroupCount, 2)
        receive(
            BmsSnapshot(
                topology: topology, pageSelector: 3, cellDelta: VoltageDelta(value: 20), lowestGroupIndex: 16,
                observedGroupCount: 2, highestGroupIndex: 46, highestTemperature: Temperature(value: 21_000)), at: 3)
        XCTAssertEqual(core.bmsSnapshot?.cellDelta, VoltageDelta(value: 20))
        XCTAssertEqual(core.bmsSnapshot?.groups.map(\.index), [])
    }

    func testBmsSnapshotPublishesChangedRustProjection() {
        let core = CutoutSessionCore()
        let firstPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 0,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090))
            ]
        )
        let cursorOnlyPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 1,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090))
            ]
        )
        var observedSnapshots: [BmsSnapshot?] = []
        core.onBmsSnapshotChange = { observedSnapshots.append($0) }

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(firstPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(cursorOnlyPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(observedSnapshots.count, 2)
        XCTAssertEqual(core.bmsSnapshot?.pageSelector, 1)
        XCTAssertEqual(core.bmsSnapshot?.pageKind, "cell voltage")
    }

    func testBmsSnapshotDoesNotMergeProtocolTaggedProjections() {
        let core = CutoutSessionCore()
        let topology = BmsTopology(
            layoutLabel: "64 observed BMS groups",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 2,
            confidence: .unverified
        )
        let firstBank = BmsSnapshot(
            topology: topology,
            pageSelector: 0,
            pageTag: 0x02,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 0))
            ]
        )
        let secondBank = BmsSnapshot(
            topology: topology,
            pageSelector: 0,
            pageTag: 0x03,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            groups: [
                BmsGroupSnapshot(index: 33, voltage: Voltage(value: 0))
            ]
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(firstBank)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(secondBank)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(core.bmsSnapshot?.groups.map(\.index), [33])
    }

    func testBmsSnapshotReplacesChangedRustProjection() {
        let core = CutoutSessionCore()
        let observedPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            voltage: Voltage(value: 95_800)
        )
        let unknownPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "unknown BMS topology",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            ),
            current: BatteryCurrent(value: 0)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(observedPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(unknownPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(core.bmsSnapshot?.topology.layoutLabel, "unknown BMS topology")
        XCTAssertEqual(core.bmsSnapshot?.topology.bmsCount, 0)
        XCTAssertNil(core.bmsSnapshot?.voltage)
        XCTAssertEqual(core.bmsSnapshot?.current, BatteryCurrent(value: 0))
    }

    func testProtocolIdentityCandidateUpdatesFromVeteranModelId() {
        let core = CutoutSessionCore()
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "NF2557")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "NOSFET Aero confirmed by model id 43")
        XCTAssertEqual(core.protocolIdentityCandidate?.support.electricUnicycleModel, .aero)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["NOSFET Aero confirmed by model id 43"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=NOSFET Aero confirmed by model id 43")
    }

    func testProtocolIdentityFallbackDisplayNameUsesDetectedFamily() {
        let cases: [(DeviceDetectionProtocolFamily?, String, String)] = [
            (.veteranLeaperkimNosfet, "protocol_identity.fallback.veteran_nosfet", "Veteran/NOSFET device"),
            (.begodeGotway, "protocol_identity.fallback.begode", "Begode device"),
            (.vesc, "protocol_identity.fallback.vesc", "VESC device"),
            (nil, "protocol_identity.fallback.unknown", "Detected rideable"),
        ]

        for (protocolFamily, key, expected) in cases {
            XCTAssertEqual(pevLocalizedText(key), expected)
            XCTAssertEqual(protocolIdentityFallbackDisplayName(protocolFamily: protocolFamily), pevLocalizedText(key))
        }
    }

    func testPevcapIdentityDoesNotUseProvisionalSelectedModel() {
        XCTAssertNil(captureResolvedIdentity(protocolIdentityCandidate: nil))
    }

    @MainActor
    func testCaptureIdentityRequiresFreshAttemptEvidenceAndRefreshesAnEqualCandidate() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let identifier = "fresh-capture-falcon"
        let candidate = DevicePickerDiscoveryCandidate(
            platformIdentifier: identifier, displayName: "Falcon", productCategory: "EUC", evidence: "fixture",
            detail: "fixture", support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .falcon),
            symbolName: "circle.hexagongrid.circle"
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(1_000) },
            testScript: CutoutSessionTestScript(candidate: candidate, telemetry: nil)
        )
        core.captureDirectoryForTesting = directory
        defer { core.disconnectAndScan() }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier(identifier), localName: "Falcon",
                advertisedServiceUuids: [.bluetooth16(0xffe0)]
            ))
        let completed = (0..<3).map { expectation(description: "capture segment \($0) durably closed") }
        var files: [URL] = []
        core.onCaptureEvent = { event in
            if case .finished(_, let url) = event {
                let index = files.count
                files.append(url)
                if index < completed.count { completed[index].fulfill() }
            } else if case .failed = event {
                XCTFail("Capture writer must preserve every segment")
            }
        }
        let state = core.rideSessionStateHandle
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        let frame = Data([
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ])
        let first = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: identifier, nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: first)
        XCTAssertTrue(core.recordOnly(platformIdentifier: identifier))
        _ = state.observeConnectionNotification(token: first, bytes: frame)
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        XCTAssertEqual(
            state.resolveDeviceSession(token: first, identificationComplete: true, nowMs: 1).connection.readiness,
            .verified)
        let remembered = try XCTUnwrap(core.protocolIdentityCandidate)
        core.finishCaptureForTesting()
        await fulfillment(of: [completed[0]], timeout: 3)
        let firstBytes = try Data(contentsOf: files[0])
        _ = state.connectionLinkDown(token: first)
        core.handleTransportTermination(platformIdentifier: identifier, error: nil, reconnect: {})

        _ = state.beginConnectionAttempt(platformIdentifier: identifier, nowMs: 2)
        XCTAssertTrue(core.recordOnly(platformIdentifier: identifier))
        XCTAssertEqual(core.protocolIdentityCandidate, remembered, "The picker retains remembered identity")
        core.finishCaptureForTesting()
        await fulfillment(of: [completed[1]], timeout: 3)

        let third = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: identifier, nowMs: 3).token)
        _ = state.connectionLinkEstablished(token: third)
        XCTAssertTrue(core.recordOnly(platformIdentifier: identifier))
        _ = state.observeConnectionNotification(token: third, bytes: frame)
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        XCTAssertEqual(
            core.protocolIdentityCandidate, remembered, "Equal UI identity still carries fresh capture proof")
        core.finishCaptureForTesting()
        await fulfillment(of: [completed[2]], timeout: 3)

        func header(_ file: URL) throws -> [String: Any] {
            let line = try XCTUnwrap(String(contentsOf: file, encoding: .utf8).split(separator: "\n").first)
            let object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any])
            return try XCTUnwrap(object["header"] as? [String: Any])
        }
        let firstHeader = try header(files[0])
        let emptyHeader = try header(files[1])
        let freshHeader = try header(files[2])
        let firstIdentity = try XCTUnwrap(firstHeader["resolved_identity"] as? [String: Any])
        let firstModel = try XCTUnwrap(firstIdentity["model"] as? [String: Any])
        XCTAssertEqual(firstModel["value"] as? String, "Begode Falcon")
        XCTAssertEqual(firstModel["verification"] as? String, "HardwareVerified")
        XCTAssertTrue(
            emptyHeader["resolved_identity"] is NSNull, "No-byte reconnect cannot claim previous hardware proof")
        XCTAssertFalse((emptyHeader["annotations"] as? [String] ?? []).contains { $0.hasPrefix("resolved_") })
        XCTAssertEqual(
            freshHeader["resolved_identity"] as? NSDictionary, firstHeader["resolved_identity"] as? NSDictionary)
        XCTAssertTrue((freshHeader["annotations"] as? [String] ?? []).contains { $0.hasPrefix("resolved_evidence=") })
        XCTAssertEqual(try Data(contentsOf: files[0]), firstBytes, "A later attempt cannot rewrite the closed segment")
    }

    func testEqualFreshIdentityRefreshesCaptureWithoutRepublishingPickerCandidate() throws {
        let capture = CaptureRecorderSpy()
        let core = CutoutSessionCore(clock: MonotonicClock(), captureRecorder: capture)
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("same-falcon-proof"), localName: "Falcon",
                advertisedServiceUuids: [.bluetooth16(0xffe0)]
            ))
        let state = core.rideSessionStateHandle
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "same-falcon-proof", nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: token)
        var publications = 0
        core.onProtocolIdentityCandidateChange = { _ in publications += 1 }
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        let first = try XCTUnwrap(capture.resolvedIdentities.last)
        let count = capture.resolvedIdentities.count
        let publicationCount = publications
        let fresh = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "same-falcon-proof", nowMs: 1).token)
        _ = state.connectionLinkEstablished(token: fresh)
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        XCTAssertEqual(
            capture.resolvedIdentities.count, count + 1,
            "Fresh proof belongs to the capture even when the picker row is unchanged")
        XCTAssertEqual(capture.resolvedIdentities.last, first)
        XCTAssertEqual(publications, publicationCount)
        let refreshedCount = capture.resolvedIdentities.count
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        XCTAssertEqual(
            capture.resolvedIdentities.count, refreshedCount,
            "An unchanged proof on the same attempt adds no writer work")
    }

    func testPevcapIdentityUsesProtocolConfirmedCandidate() {
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: mobileDiscoveryCandidateFromVeteranProtocolIdentity(
                platformIdentifier: "ios-local-aero",
                displayName: "NF2557",
                modelId: 43
            ))

        let identity = captureResolvedIdentity(protocolIdentityCandidate: candidate)

        XCTAssertEqual(identity?.protocolFamily, .veteranLeaperkimNosfet)
        XCTAssertEqual(identity?.model?.value, "NOSFET Aero")
        XCTAssertEqual(identity?.model?.verification, .hardwareVerified)
    }

    func testPevcapAnnotationSanitizesDelimiterCharacters() {
        XCTAssertEqual(
            pevcapAnnotation(key: "device_kind", value: "foo=bar\nbaz\rqux"),
            "device_kind=foo bar baz qux"
        )
        XCTAssertEqual(
            sanitizedPevcapAnnotation("user_note=one=two\nthree"),
            "user_note=one two three"
        )
    }

    func testBegodeProbeWritesAreLabeledForDetectionCapture() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))

        XCTAssertTrue(core.records.contains("begode_probe_write=model"))
        XCTAssertTrue(core.records.contains("begode_probe_write=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_write=imu"))
    }

    func testBegodeProbeWriteDoesNotUseSkippedWriteGuard() {
        let core = CutoutSessionCore()

        _ = core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("N".utf8))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertTrue(core.records.contains("begode_probe_write=model"))
    }

    func testVescTelemetryRequestDoesNotUseSkippedWriteGuard() {
        let core = CutoutSessionCore()

        _ = core.writeWithoutResponse(
            channel: .vescNordicUartWrite,
            bytes: Data([0x02, 0x01, 0x04, 0x40, 0x84, 0x03])
        )

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
    }

    func testVescRealtimeTelemetryRequestDoesNotUseReadOnlyWriteGuard() {
        let core = CutoutSessionCore()

        _ = core.writeWithoutResponse(
            channel: .vescNordicUartWrite,
            bytes: Data([0x02, 0x01, 0x0e, 0xe1, 0xce, 0x03])
        )

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
    }

    func testIdentificationProbeTransportSubscribesBeforeOrderedWrites() {
        let sink = RecordingOperationSink()
        let transport = IdentificationProbeTransportCoordinator(
            detectionSession: DeviceDetectionSession()
        )

        transport.subscribe(using: sink)
        XCTAssertEqual(sink.events, [.subscribe])
        XCTAssertEqual(transport.notificationsEnabled(at: MonotonicMilliseconds(1), using: sink), .unsupported)
        XCTAssertEqual(sink.events, [.subscribe])
        _ = transport.observeNotification(
            channel: .bluetooth16(0xffe1),
            bytes: Data([
                0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
                0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
                0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
            ]))

        let outcome = transport.notificationsEnabled(
            at: MonotonicMilliseconds(42),
            using: sink
        )

        XCTAssertEqual(sink.events, [.subscribe, .write])
        XCTAssertEqual(sink.writes, [Data("N".utf8)])
        XCTAssertEqual(transport.notificationsEnabled(at: MonotonicMilliseconds(43), using: sink), .alreadyPending)
        XCTAssertEqual(sink.writes.count, 1)
        guard case .writes = outcome else {
            return XCTFail("expected ordered probe writes")
        }

        let resolution = transport.observeNotification(
            channel: .bluetooth16(0xffe1),
            bytes: Data("NAME=Falcon".utf8)
        )
        XCTAssertEqual(resolution.modelBanner, Data("Falcon".utf8))
        _ = transport.notificationsEnabled(at: MonotonicMilliseconds(100), using: sink)
        XCTAssertEqual(sink.writes, [Data("N".utf8), Data("V".utf8)])
        _ = transport.observeNotification(channel: .bluetooth16(0xffe1), bytes: Data("GW1621003".utf8))
        _ = transport.notificationsEnabled(at: MonotonicMilliseconds(200), using: sink)
        XCTAssertEqual(sink.writes, [Data("N".utf8), Data("V".utf8), Data("M".utf8)])
    }

    func testUnrelatedWriteReachesNormalTransportValidation() {
        let core = CutoutSessionCore()

        _ = core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data([0x01]))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertFalse(core.records.contains("begode_probe_write=model"))
    }

    func testMultiBytePayloadStartingWithProbeByteReachesNormalTransportValidation() {
        let core = CutoutSessionCore()

        _ = core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("NAME".utf8))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertFalse(core.records.contains("begode_probe_write=model"))
    }

    func testBegodeProbeResponsesAreLabeledFromDetectionSession() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("GW FALCON 1.0".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("MPU6500".utf8))

        XCTAssertTrue(core.records.contains("begode_probe_response=model"))
        XCTAssertTrue(core.records.contains("begode_probe_response=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_response=imu"))
    }

    func testBegodeProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore(clock: MonotonicClock(now: { MonotonicMilliseconds(1_000) }))
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Typed Begode Falcon",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        _ = core.rideSessionStateHandle.beginConnectionAttempt(platformIdentifier: "ios-local-falcon", nowMs: 0)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Begode Falcon")
        let detail = "Begode/Falcon confirmed by reported model Falcon; advertised as Typed Begode Falcon"
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, detail)
        XCTAssertEqual(core.protocolIdentityCandidate?.support.electricUnicycleModel, .falcon)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            [detail]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=\(detail)")
        XCTAssertEqual(core.scanState.rows.first?.title, "Begode Falcon")
        XCTAssertEqual(core.scanState.rows.first?.id, "ios-local-falcon")
        XCTAssertTrue(core.scanState.rows.first?.detail.contains("Typed Begode Falcon") == true)

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Typed Begode Falcon",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            ))
        XCTAssertEqual(core.scanState.rows.first?.title, "Begode Falcon")
        let projected = core.scanState
        XCTAssertEqual(core.scanState, projected, "Reading scan state is a pure projection")
        core.disconnectAndScan()
        XCTAssertNil(core.protocolIdentityCandidate)
        XCTAssertEqual(core.scanState.rows.first?.id, "ios-local-falcon")
        XCTAssertEqual(core.scanState.rows.first?.title, "Begode Falcon")
    }

    func testBegodeFirmwareProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon-code"),
                localName: "GotWay_002441",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("GW-FALCON".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "GotWay_002441")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; code GW-FALCON")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Unresolved Begode code banner")
        )
        XCTAssertNil(core.protocolIdentityCandidate?.pickerRow.connectionRoute)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; code GW-FALCON"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/GotWay identity probe collected; code GW-FALCON")
    }

    func testBegodeImuProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon-imu"),
                localName: "GotWay_002441",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("MPU6500".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "GotWay_002441")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; imu MPU6500")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Begode model not confirmed")
        )
        XCTAssertNil(core.protocolIdentityCandidate?.pickerRow.connectionRoute)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; imu MPU6500"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/GotWay identity probe collected; imu MPU6500")
    }

    func testFragmentedBegodeFrameUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        let frame: [UInt8] = [
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ]
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-gotway"),
                localName: "Mystery Wheel",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionNotification(channel: channel, bytes: Data(Array(frame.prefix(20))))
        XCTAssertNil(core.protocolIdentityCandidate)

        core.observeDetectionNotification(channel: channel, bytes: Data(Array(frame.dropFirst(20))))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Mystery Wheel")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.detail, "Begode/Gotway protocol detected; model identity probe required")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .probeRecommended(disabledReason: "Begode/Gotway model identity probe required")
        )
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/Gotway protocol detected; model identity probe required"])
        XCTAssertEqual(
            core.records.last,
            "protocol_identity=Begode/Gotway protocol detected; model identity probe required"
        )
    }

    func testMixedProtocolFamiliesUpdateProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        let begodeFrame: [UInt8] = [
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ]
        var veteranFrame = Array(repeating: UInt8(0), count: 42)
        veteranFrame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        veteranFrame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-conflict"),
                localName: "Conflicting wheel",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionNotification(channel: channel, bytes: Data(veteranFrame))
        core.observeDetectionNotification(channel: channel, bytes: Data(begodeFrame))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Conflicting wheel")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Conflicting protocol family evidence")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .conflicting(disabledReason: "Conflicting identity evidence")
        )
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            [
                "NOSFET Aero confirmed by model id 43",
                "Conflicting protocol family evidence",
            ]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Conflicting protocol family evidence")
    }

    func testMalformedBegodeProbeResponseIsLabeledFromDetectionSession() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(
            channel: channel, bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00]))

        XCTAssertTrue(core.records.contains("begode_probe_malformed=model"))
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testMalformedBegodeModelResponseIsLabeledAfterQueuedProbeWrites() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(
            channel: channel, bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00]))

        XCTAssertTrue(core.records.contains("begode_probe_malformed=model"))
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testOutstandingBegodeProbeResponsesAreLabeledMissing() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.markOutstandingBegodeProbeResponsesMissing()

        XCTAssertTrue(core.records.contains("begode_probe_missing=model"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=imu"))
    }

    func testBegodeProbeResponsesExpireOnlyAfterMonotonicDeadline() {
        let now = Mutex<MonotonicMilliseconds>(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(clock: MonotonicClock(now: { now.withLock { $0 } }))
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))

        now.withLock { $0 = MonotonicMilliseconds(2_999) }
        core.expireOutstandingBegodeProbeResponses()
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))

        now.withLock { $0 = MonotonicMilliseconds(3_000) }
        core.expireOutstandingBegodeProbeResponses()
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))

        now.withLock { $0 = MonotonicMilliseconds(3_001) }
        core.expireOutstandingBegodeProbeResponses()
        XCTAssertTrue(core.records.contains("begode_probe_missing=model"))
    }

    func testAnsweredBegodeProbeIsNotLabeledMissing() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        core.markOutstandingBegodeProbeResponsesMissing()

        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testAnsweredBegodeProbeDoesNotHideOtherMissingResponses() {
        let now = Mutex<MonotonicMilliseconds>(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(clock: MonotonicClock(now: { now.withLock { $0 } }))
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))

        now.withLock { $0 = MonotonicMilliseconds(3_003) }
        core.expireOutstandingBegodeProbeResponses()

        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=imu"))
    }

    func testProtocolIdentityCandidatePrefersSelectedAdvertisement() {
        let core = CutoutSessionCore()
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-selected"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-last"),
                localName: "Later scan row",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        _ = core.rideSessionStateHandle.selectDiscoveredPlatform(
            platformIdentifier: "ios-local-selected"
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.protocolIdentityCandidate?.platformIdentifier, "ios-local-selected")
        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "NF2557")
    }

    func testDisconnectAndScanClearsProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        core.disconnectAndScan()

        XCTAssertEqual(core.protocolIdentityCandidate, nil)
        XCTAssertEqual(
            observedCandidates.map { $0?.detail },
            ["NOSFET Aero confirmed by model id 43", nil]
        )
    }

    func testDisconnectAndScanClearsRideStateAndReturnsPickerToScanning() {
        let core = CutoutSessionCore()
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NOSFET Aero",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(speed: speedValue(1_234))
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        core.disconnectAndScan()

        XCTAssertEqual(core.phase, .scanning)
        XCTAssertEqual(core.displayState, RideDisplayState())
        XCTAssertFalse(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["NOSFET Aero"])
    }

    func testRideStateCarriesPhaseAndTelemetrySnapshot() {
        let displayState = RideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            telemetry: TelemetrySnapshot(speed: speedValue(1_234), operatingState: .riding),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )
        let rideState = EucRideScreenState(phase: .subscribing, displayState: displayState)

        XCTAssertEqual(rideState.phaseText, "Subscribing...")
        XCTAssertEqual(rideState.speedText, "2.8")
        XCTAssertEqual(rideState.speedUnit, "mph")
        XCTAssertEqual(rideState.operatingState, .riding)
        XCTAssertEqual(rideState.telemetry?.speed, Speed(value: 1_234))
    }

    func testRideStateExposesPwmHeadroomWhileStandingOrRiding() {
        let riding = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))
            )
        )
        let standing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing, pwm: dutyCycle(230))
            )
        )

        XCTAssertEqual(riding.pwmHeadroomApplicability, .available)
        XCTAssertEqual(riding.pwmHeadroomPermille, 770)
        XCTAssertEqual(standing.pwmHeadroomApplicability, .available)
        XCTAssertEqual(standing.pwmHeadroomPermille, 770)
    }

    func testRideStateTreatsIdlePwmHeadroomAsFullHeadroom() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing, pwm: dutyCycle(10))
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .available)
        XCTAssertEqual(rideState.pwmHeadroomPermille, 1_000)
    }

    func testRideStateStatusUsesOperatingStateWhenLive() {
        let parked = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked)
            )
        )
        let riding = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding)
            )
        )
        let standing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing)
            )
        )
        let charging = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .charging)
            )
        )

        XCTAssertEqual(parked.statusText, "Parked")
        XCTAssertEqual(riding.statusText, "Riding")
        XCTAssertEqual(standing.statusText, "Parked")
        XCTAssertEqual(charging.statusText, "Charging")
    }

    func testRideStateDistinguishesEmptyLiveSnapshotFromPopulatedTelemetry() {
        let waiting = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let populated = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertEqual(waiting.telemetryAvailability, .waitingForValues)
        XCTAssertEqual(populated.telemetryAvailability, .populated)
    }

    func testRideStateCarriesTypedWarningSeverity() {
        let failed = EucRideScreenState(
            phase: .failed(.connectFailed("link dropped")),
            displayState: RideDisplayState()
        )
        let inactive = EucRideScreenState(
            phase: .scanning,
            displayState: RideDisplayState()
        )
        let waiting = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let populated = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertEqual(failed.warningState.severity, .failed)
        XCTAssertEqual(inactive.warningState.severity, .unavailable)
        XCTAssertEqual(waiting.warningState.severity, .caution)
        XCTAssertEqual(populated.warningState.severity, .normal)
        XCTAssertEqual(waiting.warningState.title, "Waiting for telemetry")
    }

    func testRideStateRecommendsReducingAccelerationForLowRidingPwmHeadroom() {
        let lowHeadroom = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(800))
            )
        )
        let healthyHeadroom = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(500))
            )
        )
        let parked = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(800))
            )
        )

        XCTAssertEqual(lowHeadroom.pwmHeadroomPermille, 200)
        XCTAssertEqual(lowHeadroom.warningState.severity, .reduceAcceleration)
        XCTAssertEqual(lowHeadroom.warningState.title, "Reduce acceleration")
        XCTAssertEqual(healthyHeadroom.warningState.severity, .normal)
        XCTAssertEqual(parked.warningState.severity, .normal)
    }

    func testRideStateTreatsMissingLiveSnapshotAsTelemetryUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState()
        )

        XCTAssertEqual(rideState.telemetryAvailability, .unavailable)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .unknown)
    }

    func testRideStateShowsParkedPwmHeadroom() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .available)
        XCTAssertEqual(rideState.pwmHeadroomPermille, 1_000)
    }

    func testRideStateOwnsTypedPwmHeadroomPresentation() throws {
        let available = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))
            )
        )
        let parked = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )
        let unavailable = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot(operatingState: .riding))
        )

        XCTAssertEqual(
            available.pwmHeadroomMetricValue,
            .available(display: "77%", accessibility: "77%")
        )
        XCTAssertEqual(try XCTUnwrap(available.pwmHeadroomProgress), 0.77, accuracy: 0.001)
        XCTAssertEqual(
            parked.pwmHeadroomMetricValue,
            .available(display: "100%", accessibility: "100%")
        )
        XCTAssertEqual(parked.pwmHeadroomProgress, 1)
        XCTAssertEqual(unavailable.pwmHeadroomMetricValue, .unavailable)
        XCTAssertNil(unavailable.pwmHeadroomProgress)
    }

    func testRideStateTreatsMissingPwmHeadroomAsUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding)
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .unavailable)
        XCTAssertNil(rideState.pwmHeadroomPermille)
    }

    func testRideStateAccountsForVisibleFieldsInPopulatedLiveSnapshot() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                speed: SpeedReadout(millimetersPerSecond: 1_234),
                telemetry: TelemetrySnapshot(
                    speed: speedValue(1_234),
                    operatingState: .riding,
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(2_000),
                    controllerTemperature: temperatureValue(31_000),
                    pwm: dutyCycle(230),
                    batteryLevelEstimated: batteryLevelValue(80)
                )
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .status), .sessionState)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .updateAge), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .pwmHeadroom), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .sagAdjustedEnergy), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .packVoltage), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .thermal), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .warningState), .sessionState)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .voltageSag), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .limpHomeRange), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .tabs), .staticNavigation)
    }

    func testRideStateRequiresRepresentativeLiveFieldsForValidation() {
        let ready = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: speedValue(1_234),
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(2_000),
                    controllerTemperature: temperatureValue(31_000),
                    pwm: dutyCycle(230)
                )
            )
        )
        let missing = EucRideScreenState(
            phase: .subscribing,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertTrue(ready.isLiveValidationReady)
        XCTAssertEqual(ready.liveValidationMissingFields, [])
        XCTAssertFalse(missing.isLiveValidationReady)
        XCTAssertEqual(
            missing.liveValidationMissingFields,
            [.livePhase, .updateAge, .speed, .power, .pwm, .thermal]
        )
    }

    func testRideStateAccountsForRegenerationPowerOnlyWhenFlowIsRegeneration() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    operatingState: .riding,
                    voltage: voltageValue(96_700),
                    batteryCurrent: batteryCurrentValue(-800),
                    powerFlow: .regeneration
                )
            )
        )

        XCTAssertEqual(rideState.regenerationPower, powerValue(-77_360))
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .regenPower), .derivedTelemetry)
    }

    func testRideStateDoesNotAccountForUnverifiedNegativePowerAsRegeneration() {
        let unknownFlowState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(96_700),
                    batteryCurrent: batteryCurrentValue(-800),
                    powerFlow: .negativeUnknown
                )
            )
        )
        let chargingState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(-2_000),
                    powerFlow: .charging
                )
            )
        )

        XCTAssertNil(unknownFlowState.regenerationPower)
        XCTAssertEqual(unknownFlowState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
        XCTAssertNil(chargingState.regenerationPower)
        XCTAssertEqual(chargingState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
    }

    func testRideStateAccountsForVoltageSagAndLimpHomeOnlyWhenTypedValuesExist() {
        let unavailableState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(96_700))
            )
        )
        let typedState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(96_700),
                    voltageSag: VoltageDelta(value: -1_200),
                    limpHomeRange: Distance(value: 22_852_500)
                )
            )
        )

        XCTAssertNil(unavailableState.voltageSag)
        XCTAssertNil(unavailableState.limpHomeRange)
        XCTAssertEqual(unavailableState.visibleFieldCoverage.source(for: .voltageSag), .explicitlyUnavailable)
        XCTAssertEqual(unavailableState.visibleFieldCoverage.source(for: .limpHomeRange), .explicitlyUnavailable)
        XCTAssertEqual(typedState.voltageSag, VoltageDelta(value: -1_200))
        XCTAssertEqual(typedState.limpHomeRange, Distance(value: 22_852_500))
        XCTAssertEqual(typedState.visibleFieldCoverage.source(for: .voltageSag), .derivedTelemetry)
        XCTAssertEqual(typedState.visibleFieldCoverage.source(for: .limpHomeRange), .derivedTelemetry)
    }

    func testRideStateBuildsControllerOnlyEstimateFromLiveTelemetry() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(117_600),
                    batteryCurrent: batteryCurrentValue(38_000),
                    voltageSag: VoltageDelta(value: 4_800),
                    batteryLevelEstimated: batteryLevelValue(71)
                )
            )
        )

        XCTAssertEqual(rideState.controllerOnlyEstimatePercent, batteryLevelValue(71))
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, .recentSag)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .medium)
    }

    func testRideStateLowersControllerOnlyEstimateConfidenceWhenSagIsUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(117_600),
                    batteryLevelReported: batteryLevelValue(68)
                )
            )
        )

        XCTAssertEqual(rideState.controllerOnlyEstimatePercent, batteryLevelValue(68))
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, .voltageCurve)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .low)
    }

    func testRideStateAccountsForParkedPwmAsDerivedTelemetry() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .pwmHeadroom), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .explicitlyUnavailable)
    }

    func testRideStateAccountsForEmptyLiveSnapshotAsExplicitlyUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .packVoltage), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .thermal), .explicitlyUnavailable)
    }

    func testRideStateClassifiesUpdateAgeFromMonotonicTimestamp() {
        let missing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let fresh = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000))
            )
        )
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(lastUpdate: MonotonicMilliseconds(1_000))
        )

        XCTAssertEqual(
            missing.updateAge(at: MonotonicMilliseconds(1_100), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: nil, freshness: .unavailable)
        )
        XCTAssertEqual(
            fresh.updateAge(at: MonotonicMilliseconds(1_100), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(100), freshness: .fresh)
        )
        XCTAssertEqual(
            stale.updateAge(at: MonotonicMilliseconds(1_300), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(300), freshness: .stale)
        )
        XCTAssertEqual(fresh.visibleFieldCoverage.source(for: .updateAge), .liveTelemetry)
    }

    func testRideStateUsesTypedStaleWarningWhenTelemetryIsOld() {
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000), voltage: voltageValue(118_000)),
                lastUpdate: MonotonicMilliseconds(4_000)
            )
        )
        let fresh = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(3_900), voltage: voltageValue(118_000)),
                lastUpdate: MonotonicMilliseconds(4_000)
            )
        )

        XCTAssertEqual(
            stale.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)),
            EucRideWarningState(severity: .caution, title: "Telemetry stale", detail: "Last update 3 seconds ago")
        )
        XCTAssertEqual(
            fresh.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)).severity,
            .normal
        )
    }

    func testRideStatePrefersStaleWarningOverLowPwmHeadroom() {
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    operatingState: .riding,
                    pwm: dutyCycle(800)
                )
            )
        )

        XCTAssertEqual(stale.warningState.severity, .reduceAcceleration)
        XCTAssertEqual(
            stale.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)),
            EucRideWarningState(severity: .caution, title: "Telemetry stale", detail: "Last update 3 seconds ago")
        )
    }

    func testRideStateClaimsDerivedPowerForZeroCurrent() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(0)
                )
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .derivedTelemetry)
    }

    func testDisplayStateProvidesDebugRowsForLiveValidation() {
        let displayState = RideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )

        XCTAssertEqual(
            displayState.debugRows,
            [
                SessionDebugRow(
                    id: "Notifications",
                    label: "Notifications",
                    metricValue: .status(display: "7", accessibility: "7")
                ),
                SessionDebugRow(
                    id: "Last update",
                    label: "Last update",
                    metricValue: .status(display: "9876 ms", accessibility: "9876 ms")
                ),
            ]
        )
    }

    private var scriptedVescCandidate: DevicePickerDiscoveryCandidate {
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "scripted-vesc",
            displayName: "Scripted VESC",
            productCategory: "VESC Onewheel",
            evidence: "test script",
            detail: "core callback fixture",
            support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
            symbolName: "circle.hexagongrid.circle"
        )
    }

    private var scriptedProbeCandidate: DevicePickerDiscoveryCandidate {
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "scripted-probe",
            displayName: "Unknown EUC",
            productCategory: "Electric unicycle",
            evidence: "test script",
            detail: "identification required",
            support: .probeRecommended(disabledReason: "Identity probe required"),
            symbolName: "magnifyingglass"
        )
    }

    private var scriptedAeroCandidate: DevicePickerDiscoveryCandidate {
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "scripted-aero",
            displayName: "Scripted Aero",
            productCategory: "Electric unicycle",
            evidence: "test script",
            detail: "settings crash fixture",
            support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
            symbolName: "circle.hexagongrid.circle"
        )
    }
    @MainActor
    func testCoreLocationSentinelsNormalizeInRustSnapshotAndForwardRawBatch() async throws {
        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_609,
            horizontalAccuracy: -1,
            verticalAccuracy: -1,
            course: -1,
            speed: -1,
            timestamp: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let observed = Mutex((capture: Optional<PhoneLocationUpdate>.none, rideMap: Optional<PhoneLocationUpdate>.none))
        let recorded = expectation(description: "Both recording sinks completed")
        let published = expectation(description: "Normalized phone sample published")
        let locationEffects = CutoutSessionLocationEffects(
            recordCaptureUpdate: { update in
                observed.withLock { $0.capture = update }
                return CaptureLocationWriteResult(generation: nil, outcome: .accepted)
            },
            ingestRideMapUpdate: { update in
                observed.withLock { $0.rideMap = update }
                recorded.fulfill()
            },
            handleCaptureResult: { _ in }
        )
        let core = CutoutSessionCore(
            clock: MonotonicClock(),
            locationEffects: locationEffects
        )

        core.onPhoneLocationSnapshotChange = { snapshot, _ in
            if snapshot.latestSample != nil { published.fulfill() }
        }
        core.deliverPhoneLocationsForTesting([location])
        await fulfillment(of: [recorded, published], timeout: 2)
        let capturedUpdate = observed.withLock { $0.capture }
        let rideMapUpdate = observed.withLock { $0.rideMap }

        let sample = try XCTUnwrap(core.phoneLocationSnapshot.latestSample)
        XCTAssertNil(sample.horizontalAccuracyMeters)
        XCTAssertNil(sample.verticalAccuracyMeters)
        XCTAssertNil(sample.speedMetersPerSecond)
        XCTAssertNil(sample.courseDegrees)
        XCTAssertNil(sample.speedAccuracyMetersPerSecond)
        XCTAssertNil(sample.courseAccuracyDegrees)

        for forwarded in [capturedUpdate, rideMapUpdate].compactMap({ $0?.samples.first }) {
            XCTAssertEqual(forwarded.horizontalAccuracyMeters, -1)
            XCTAssertEqual(forwarded.verticalAccuracyMeters, -1)
            XCTAssertEqual(forwarded.speedMetersPerSecond, -1)
            XCTAssertEqual(forwarded.speedAccuracyMetersPerSecond, -1)
            XCTAssertEqual(forwarded.courseDegrees, -1)
            XCTAssertEqual(forwarded.courseAccuracyDegrees, -1)
        }
        XCTAssertNotNil(capturedUpdate)
        XCTAssertNotNil(rideMapUpdate)
    }
}

private func assertVescTelemetryRequests(
    _ operations: [CoreBluetoothPlannedOperation],
    includesSubscribe: Bool,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    let expectedWriteCount = 3
    XCTAssertEqual(operations.count, expectedWriteCount + (includesSubscribe ? 1 : 0), file: file, line: line)
    if includesSubscribe {
        XCTAssertTrue(
            operations.contains(.subscribe(channel: .vescNordicUartNotify)),
            file: file,
            line: line
        )
    }
    let writes = operations.compactMap { operation -> Data? in
        guard case .writeWithoutResponse(channel: .vescNordicUartWrite, bytes: let bytes) = operation else {
            return nil
        }
        return bytes
    }
    XCTAssertEqual(writes.count, expectedWriteCount, file: file, line: line)
    XCTAssertTrue(writes.first.map { isRefloatRequest($0, command: 0) } ?? false, file: file, line: line)
    XCTAssertEqual(writes[1], Data([2, 1, 14, 225, 206, 3]), file: file, line: line)
    XCTAssertEqual(writes[2], Data([2, 1, 4, 64, 132, 3]), file: file, line: line)
}

private func isRefloatRequest(_ bytes: Data, command: UInt8) -> Bool {
    bytes.count >= 7
        && bytes.first == 0x02
        && bytes.last == 0x03
        && bytes[bytes.index(bytes.startIndex, offsetBy: 2)] == 36
        && bytes[bytes.index(bytes.startIndex, offsetBy: 3)] == 101
        && bytes[bytes.index(bytes.startIndex, offsetBy: 4)] == command
}

private final class RecordingOperationSink: CoreBluetoothOperationSink {
    enum Event: Equatable {
        case subscribe
        case write
    }

    private let writesChanged = NSCondition()
    private var recordedWrites: [Data] = []
    private var recordedEvents: [Event] = []

    var writes: [Data] {
        writesChanged.lock()
        defer { writesChanged.unlock() }
        return recordedWrites
    }

    var events: [Event] {
        writesChanged.lock()
        defer { writesChanged.unlock() }
        return recordedEvents
    }

    func subscribe(channel: BluetoothUuid) {
        writesChanged.lock()
        defer { writesChanged.unlock() }
        recordedEvents.append(.subscribe)
    }

    func writeWithoutResponse(
        channel: BluetoothUuid, bytes: Data, isCurrent: @escaping () -> Bool,
        onReceipt: @escaping (CoreBluetoothWriteDisposition) -> Void
    ) -> CoreBluetoothWriteDisposition {
        guard isCurrent() else {
            onReceipt(.cancelled)
            return .cancelled
        }
        writesChanged.lock()
        recordedWrites.append(bytes)
        recordedEvents.append(.write)
        writesChanged.broadcast()
        writesChanged.unlock()
        onReceipt(.submitted)
        return .submitted
    }

    func disconnect() {}

    func waitForWrites(_ expectedCount: Int, timeout: TimeInterval) -> Bool {
        writesChanged.lock()
        defer { writesChanged.unlock() }

        let deadline = Date().addingTimeInterval(timeout)
        while recordedWrites.count < expectedCount {
            if !writesChanged.wait(until: deadline) {
                return recordedWrites.count >= expectedCount
            }
        }
        return true
    }
}

private func waitForWrites(_ expectedCount: Int, in sink: RecordingOperationSink) {
    XCTAssertTrue(
        sink.waitForWrites(expectedCount, timeout: 1.5),
        "operation sink did not reach \(expectedCount) writes"
    )
}

private func speedValue(_ value: Int32) -> Speed {
    Speed(value: value)
}

private func voltageValue(_ value: Int32) -> Voltage {
    Voltage(value: value)
}

private func batteryCurrentValue(_ value: Int32) -> BatteryCurrent {
    BatteryCurrent(value: value)
}

private func phaseCurrentValue(_ value: Int32) -> PhaseCurrent {
    PhaseCurrent(value: value)
}

private func powerValue(_ value: Int64) -> Power {
    Power(value: value)
}

private func temperatureValue(_ value: Int32) -> Temperature {
    Temperature(value: value)
}

private func angleValue(_ value: Int32) -> Angle {
    Angle(value: value)
}

private func batteryLevelValue(_ value: UInt8) -> BatteryLevel {
    BatteryLevel(value: value)
}

private func dutyCycle(_ permille: Int16) -> DutyCycle {
    DutyCycle(permille: permille)
}

private final class TestMonotonicClock: Sendable {
    private let storage: Mutex<MonotonicMilliseconds>

    var now: MonotonicMilliseconds {
        get { storage.withLock { $0 } }
        set { storage.withLock { $0 = newValue } }
    }

    init(_ now: MonotonicMilliseconds) {
        storage = Mutex(now)
    }
}

extension [EucRideVisibleFieldCoverage] {
    fileprivate func source(for field: EucRideVisibleField) -> EucRideVisibleFieldSource? {
        first { $0.field == field }?.source
    }
}

private final class RecordingReconnectScheduler: ConnectionReconnectScheduling {
    private final class Token: ConnectionReconnectCancellable {
        var isCancelled = false

        func cancel() {
            isCancelled = true
        }
    }

    private var scheduled: [(token: Token, operation: () -> Void)] = []

    func schedule(after _: UInt64, operation: @escaping () -> Void) -> any ConnectionReconnectCancellable {
        let token = Token()
        scheduled.append((token, operation))
        return token
    }

    func runAll(includingCancelled: Bool = false) {
        let scheduled = scheduled
        self.scheduled.removeAll()
        for entry in scheduled where includingCancelled || !entry.token.isCancelled {
            entry.operation()
        }
    }
}
