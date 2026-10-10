import CutoutMobileFFI
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

@MainActor
final class MusicFeatureModelTests: XCTestCase {
    func testBackgroundObservationDoesNotRetainMusicModelDuringSQLiteStall() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        var model: MusicFeatureModel? = makeModel(state: fixture.state, defaults: suite.defaults)
        weak let releasedModel = model
        _ = try XCTUnwrap(model?.providerLifecycle.beginProviderSession())
        try await fixture.holdWrite()
        model?.sceneDidEnterBackground()
        try await Task.sleep(for: .milliseconds(50))
        model = nil
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        XCTAssertNil(releasedModel, "A pending background observation must not retain music presentation")
        fixture.release.signal()
        try await fixture.finish()
    }

    func testStaleRideContextFailsStopBeforeOptionalErrorReadbackReturns() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        let retired = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        _ = try state.stop(atMs: 150)
        _ = try state.discard()
        let model = makeModel(
            state: state, defaults: suite.defaults,
            correlationRideIDReader: { retired.rideID })
        let current = try await state.startGpsOnlyCommand(atMs: 200, musicHistoryPolicy: .humanReadable)
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let readHeld = expectation(description: "optional current-ride history query is held after stale context fails")
        var releaseRead: CheckedContinuation<Void, Never>?
        defer { releaseRead?.resume() }
        let coordinator = model.coordinator
        let observation = try XCTUnwrap(
            model.submitObservation(
                self.observation(atMs: 250, identifier: "player-with-stale-history-context"),
                wallClockAtMs: 1_700_000_000_250, clockUncertaintyMs: 9,
                historyReadback: {
                    let events = try await coordinator.recordedEventsAsync()
                    readHeld.fulfill()
                    await withCheckedContinuation { releaseRead = $0 }
                    return events
                }))
        await fulfillment(of: [readHeld], timeout: 2)
        XCTAssertNotNil(model.historySaveError)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "player-with-stale-history-context")
        XCTAssertEqual(model.settingsNowPlaying?.isCommandAvailable(.pause), true)
        let stop = try state.beginLifecycleCommand(
            event: .stop, expected: XCTUnwrap(current.commandToken), atMs: 300)
        do {
            _ = try await settledMusicStop(state: state, command: stop)
            XCTFail("Stale required context must fail Stop before optional error readback is released")
        } catch let error as MobileRideMapError {
            XCTAssertEqual(error, .storageError("ride music observation is incomplete"))
            guard error == .storageError("ride music observation is incomplete") else { throw error }
        }
        XCTAssertEqual(state.currentSnapshot()?.rideID, current.rideID)
        XCTAssertNotEqual(state.currentSnapshot()?.state, .stopped)
        XCTAssertTrue(
            state.currentMusicEvents().isEmpty,
            "The stale original ride must not reassociate metadata with the current ride")
        let retry = try await state.performLifecycleCommand(
            event: .stop, expected: XCTUnwrap(state.currentSnapshot()?.commandToken), atMs: 301)
        XCTAssertEqual(retry.rideID, current.rideID)
        XCTAssertEqual(retry.state, .stopped)
        releaseRead?.resume()
        releaseRead = nil
        let accepted = await observation.value
        XCTAssertFalse(accepted)
        XCTAssertTrue(state.currentMusicEvents().isEmpty)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "player-with-stale-history-context")
        _ = try state.discard()
    }

    func testOptionalOldMusicReadbackCannotDelayStopOrOverwriteNewerPresentation() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        let ride = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let target = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 75))
        let readHeld = expectation(description: "older optional history result is held after required settlement")
        let newerCapture = expectation(description: "newer required capture progresses past the settled older request")
        var releaseRead: CheckedContinuation<Void, Never>?
        defer { releaseRead?.resume() }
        var deliveries = [MobilePevcapMusicEventDto]()
        let model = makeModel(
            state: state, defaults: suite.defaults,
            updateCaptureObservationAsync: { event, admittedTarget in
                XCTAssertEqual(admittedTarget, target)
                if let event {
                    deliveries.append(event)
                    if event.monotonicAtMs == 250 { newerCapture.fulfill() }
                }
                return .accepted
            }, captureTarget: { target })
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let coordinator = model.coordinator
        let older = try XCTUnwrap(
            model.submitObservation(
                observation(atMs: 200, identifier: "older"),
                wallClockAtMs: 1_700_000_000_200, clockUncertaintyMs: 7,
                historyReadback: {
                    let oldEvents = try await coordinator.recordedEventsAsync()
                    XCTAssertEqual(oldEvents.map(\.itemIdentifier), ["older"])
                    readHeld.fulfill()
                    await withCheckedContinuation { releaseRead = $0 }
                    return oldEvents
                }))
        await fulfillment(of: [readHeld], timeout: 2)
        let newer = try XCTUnwrap(
            model.submitObservation(
                observation(atMs: 250, identifier: "newer"),
                wallClockAtMs: 1_700_000_000_250, clockUncertaintyMs: 9))
        let stop = try state.beginLifecycleCommand(
            event: .stop, expected: XCTUnwrap(ride.commandToken), atMs: 300)
        await fulfillment(of: [newerCapture], timeout: 2)
        let stopped = try await settledMusicStop(state: state, command: stop)
        XCTAssertEqual(stopped.rideID, ride.rideID)
        XCTAssertEqual(stopped.state, .stopped)
        let newerAccepted = await newer.value
        XCTAssertTrue(newerAccepted)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "newer")
        XCTAssertEqual(model.timelineEvents.map(\.itemIdentifier), ["older", "newer"])
        XCTAssertEqual(deliveries.map(\.monotonicAtMs), [200, 250])
        XCTAssertEqual(deliveries.map(\.rideSequence), [0, 1])
        releaseRead?.resume()
        releaseRead = nil
        let olderPublished = await older.value
        XCTAssertFalse(olderPublished, "An old optional read cannot reclaim publication after newer classification")
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "newer")
        XCTAssertEqual(model.timelineEvents.map(\.itemIdentifier), ["older", "newer"])
        _ = try state.discard()
    }

    func testRejectedCaptureDiagnosticIsVisibleBeforeOptionalErrorReadbackReturns() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let target = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 76))
        let readHeld = expectation(description: "optional readback after required capture failure is held")
        var releaseRead: CheckedContinuation<Void, Never>?
        defer { releaseRead?.resume() }
        let model = makeModel(
            state: state, defaults: suite.defaults,
            updateCaptureObservationAsync: { _, admittedTarget in
                XCTAssertEqual(admittedTarget, target)
                return .rejected
            }, captureTarget: { target })
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let coordinator = model.coordinator
        let original = observation(atMs: 200, identifier: "failed-capture")
        let pending = try XCTUnwrap(
            model.submitObservation(
                original, wallClockAtMs: 1_700_000_000_200, clockUncertaintyMs: 11,
                historyReadback: {
                    let events = try await coordinator.recordedEventsAsync()
                    readHeld.fulfill()
                    await withCheckedContinuation { releaseRead = $0 }
                    return events
                }))
        await fulfillment(of: [readHeld], timeout: 2)
        let failure = try XCTUnwrap(model.captureFailureReceipt)
        XCTAssertEqual(failure.transition.snapshot, original.snapshot)
        XCTAssertEqual(failure.transition.wallClockAtMs, 1_700_000_000_200)
        XCTAssertEqual(failure.transition.clockUncertaintyMs, 11)
        XCTAssertEqual(failure.target, target)
        XCTAssertEqual(failure.outcome, .rejected)
        XCTAssertNotNil(
            model.historySaveError, "Required-effect failure must be visible before an optional query returns")
        releaseRead?.resume()
        releaseRead = nil
        let accepted = await pending.value
        XCTAssertFalse(accepted)
        try stopAfterExpectedRejectedMusicCapture(state: state, atMs: 300)
        _ = try state.discard()
    }

    func testStopWaitsForRequiredMusicCaptureReceipt() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        let ride = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let target = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 73))
        let sinkHeld = expectation(description: "required original-target capture sink is held after history commits")
        var releaseSink: CheckedContinuation<Void, Never>?
        defer { releaseSink?.resume() }
        var delivered: MobilePevcapMusicEventDto?
        let model = makeModel(
            state: state, defaults: suite.defaults,
            updateCaptureObservationAsync: { observation, admittedTarget in
                XCTAssertEqual(admittedTarget, target)
                delivered = observation
                sinkHeld.fulfill()
                await withCheckedContinuation { releaseSink = $0 }
                return .accepted
            }, captureTarget: { target })
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let observation = try XCTUnwrap(
            model.submitObservation(
                self.observation(atMs: 200, identifier: "captured-before-stop"),
                wallClockAtMs: 1_700_000_000_200, clockUncertaintyMs: 7))
        await fulfillment(of: [sinkHeld], timeout: 2)
        let stop = try state.beginLifecycleCommand(
            event: .stop, expected: XCTUnwrap(ride.commandToken), atMs: 300)
        if case .completed = try state.pollLifecycleCommand(stop) {
            XCTFail("SQL completion alone must not let Stop pass the held required capture sink")
        }
        releaseSink?.resume()
        releaseSink = nil
        let accepted = await observation.value
        XCTAssertTrue(accepted)
        let stopped = try await settledMusicStop(state: state, command: stop)
        XCTAssertEqual(stopped.rideID, ride.rideID)
        XCTAssertEqual(stopped.state, .stopped)
        XCTAssertEqual(delivered?.monotonicAtMs, 200)
        XCTAssertEqual(delivered?.wallClockUnixMs, 1_700_000_000_200)
        XCTAssertEqual(delivered?.clockUncertaintyMs, 7)
        XCTAssertEqual(delivered?.rideSequence, 0)
        let events = try await model.coordinator.recordedEventsAsync()
        XCTAssertEqual(events.map(\.itemIdentifier), ["captured-before-stop"])
        _ = try state.discard()
    }

    func testProviderResetMakesUnsettledAcceptedObservationFailStopExplicitly() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        let ride = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let target = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 74))
        let sinkHeld = expectation(description: "capture receipt is held across provider retirement")
        var releaseSink: CheckedContinuation<Void, Never>?
        defer { releaseSink?.resume() }
        let model = makeModel(
            state: state, defaults: suite.defaults,
            updateCaptureObservationAsync: { _, admittedTarget in
                XCTAssertEqual(admittedTarget, target)
                sinkHeld.fulfill()
                await withCheckedContinuation { releaseSink = $0 }
                return .accepted
            }, captureTarget: { target })
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let observation = try XCTUnwrap(
            model.submitObservation(
                self.observation(atMs: 200, identifier: "retired-before-stop")))
        await fulfillment(of: [sinkHeld], timeout: 2)
        let stop = try state.beginLifecycleCommand(
            event: .stop, expected: XCTUnwrap(ride.commandToken), atMs: 300)
        if case .completed = try state.pollLifecycleCommand(stop) {
            XCTFail("Stop must remain pending before the accepted capture obligation settles")
        }
        model.coordinator.resetProviderCorrelation()
        releaseSink?.resume()
        releaseSink = nil
        let accepted = await observation.value
        XCTAssertFalse(accepted)
        do {
            _ = try await settledMusicStop(state: state, command: stop)
            XCTFail("Provider retirement must not turn an unfinished accepted obligation into successful Stop")
        } catch let error as MobileRideMapError {
            XCTAssertEqual(error, .storageError("ride music observation is incomplete"))
        }
        XCTAssertEqual(state.currentSnapshot()?.rideID, ride.rideID)
        XCTAssertNotEqual(state.currentSnapshot()?.state, .stopped)
        let retry = try await state.performLifecycleCommand(
            event: .stop, expected: XCTUnwrap(state.currentSnapshot()?.commandToken), atMs: 301)
        XCTAssertEqual(retry.state, .stopped)
        _ = try state.discard()
    }

    private func settledMusicStop(
        state: MobileRideMapState, command: MobileRideMapLifecycleCommand
    ) async throws -> MobileRideMapSnapshotDto {
        let deadline = ContinuousClock.now + .seconds(3)
        while ContinuousClock.now < deadline {
            if case let .completed(snapshot) = try state.pollLifecycleCommand(command) { return snapshot }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Stop did not produce its Rust terminal receipt")
        throw MobileRideMapError.admissionPending
    }

    func testRejectedCaptureRetainsExactReceiptWhileLaterPlayerAndHistoryProgress() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let originalTarget = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 41))
        let replacementTarget = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 42))
        var target = originalTarget
        var deliveries = [(MobilePevcapMusicEventDto?, MobileMusicCaptureTarget)]()
        let model = makeModel(
            state: state, defaults: suite.defaults,
            updateCaptureObservationAsync: { observation, target in
                deliveries.append((observation, target))
                return deliveries.count == 1 ? .rejected : .accepted
            },
            captureTarget: { target }
        )
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let first = observation(atMs: 200, identifier: "first-track")
        let firstAccepted = await model.ingestObservationAsync(
            first, wallClockAtMs: 1_700_000_000_200, clockUncertaintyMs: 7)
        XCTAssertFalse(firstAccepted)
        let failure = try XCTUnwrap(model.captureFailureReceipt)
        XCTAssertEqual(failure.transition.snapshot, first.snapshot)
        XCTAssertEqual(failure.transition.wallClockAtMs, 1_700_000_000_200)
        XCTAssertEqual(failure.transition.clockUncertaintyMs, 7)
        XCTAssertEqual(failure.transition.captureTarget, originalTarget)
        XCTAssertEqual(failure.target, originalTarget)
        XCTAssertEqual(failure.outcome, .rejected)
        XCTAssertEqual(deliveries.first?.1, originalTarget)
        XCTAssertEqual(deliveries.first?.0?.monotonicAtMs, 200)
        XCTAssertEqual(deliveries.first?.0?.wallClockUnixMs, 1_700_000_000_200)
        XCTAssertEqual(deliveries.first?.0?.clockUncertaintyMs, 7)
        XCTAssertEqual(deliveries.first?.0?.rideSequence, 0)
        XCTAssertNotNil(model.historySaveError)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "first-track")

        target = replacementTarget
        let secondAccepted = await model.ingestObservationAsync(
            observation(atMs: 300, identifier: "replacement-track"),
            wallClockAtMs: 1_700_000_000_300, clockUncertaintyMs: 9)
        XCTAssertTrue(secondAccepted)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "replacement-track")
        let thirdAccepted = await model.ingestObservationAsync(
            observation(atMs: 400, identifier: "replacement-track"),
            wallClockAtMs: 1_700_000_000_400, clockUncertaintyMs: 11)
        XCTAssertTrue(thirdAccepted)
        XCTAssertEqual(model.captureFailureReceipt?.transition, failure.transition)
        XCTAssertEqual(model.captureFailureReceipt?.target, originalTarget)
        XCTAssertEqual(model.captureFailureReceipt?.outcome, .rejected)
        XCTAssertNotNil(model.historySaveError)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "replacement-track")
        XCTAssertTrue(deliveries.dropFirst().allSatisfy { $0.1 == replacementTarget })
        let rideID = try XCTUnwrap(state.currentSnapshot()?.rideID)
        let deleted = await model.forgetHistory(for: rideID)
        XCTAssertTrue(deleted)
        XCTAssertNil(model.captureFailureReceipt)
        XCTAssertNil(model.coordinator.previousRideHistoryFailure)
        XCTAssertNil(model.historySaveError)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "replacement-track")
        try stopAfterExpectedRejectedMusicCapture(state: state, atMs: 500)
        _ = try state.discard()
    }

    func testOrdinaryHistoryReadbackPreservesLateExactCaptureFailure() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        let ride = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let target = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 52))
        let captureHeld = expectation(description: "required capture admission is held during ordinary readback")
        var releaseCapture: CheckedContinuation<Void, Never>?
        let model = makeModel(
            state: state, defaults: suite.defaults,
            updateCaptureObservationAsync: { observation, _ in
                guard observation != nil else { return .accepted }
                captureHeld.fulfill()
                await withCheckedContinuation { releaseCapture = $0 }
                return .rejected
            },
            captureTarget: { target }
        )
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let original = observation(atMs: 200, identifier: "retained-track")
        let pending = try XCTUnwrap(
            model.submitObservation(
                original, wallClockAtMs: 1_700_000_000_200, clockUncertaintyMs: 7))
        await fulfillment(of: [captureHeld], timeout: 2)
        model.synchronizeHistory(try XCTUnwrap(state.currentMusicHistory()))
        releaseCapture?.resume()
        let admitted = await pending.value
        XCTAssertFalse(admitted)
        let failure = try XCTUnwrap(model.captureFailureReceipt)
        XCTAssertEqual(failure.transition.snapshot, original.snapshot)
        XCTAssertEqual(failure.transition.wallClockAtMs, 1_700_000_000_200)
        XCTAssertEqual(failure.transition.clockUncertaintyMs, 7)
        XCTAssertEqual(failure.target, target)
        XCTAssertEqual(failure.outcome, .rejected)
        XCTAssertNotNil(model.historySaveError)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "retained-track")
        XCTAssertEqual(model.settingsNowPlaying?.isCommandAvailable(.pause), true)
        let deleted = await model.forgetHistory(for: ride.rideID)
        XCTAssertTrue(deleted)
        XCTAssertNil(model.captureFailureReceipt)
        XCTAssertNil(model.historySaveError)
        try stopAfterExpectedRejectedMusicCapture(state: state, atMs: 300)
        _ = try state.discard()
    }

    func testHistoryDeletionPreventsLateCaptureFailureFromRestoringDiagnosticMetadata() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        let ride = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let target = MobileMusicCaptureTarget.capture(generation: MobileCaptureGenerationDto(value: 51))
        let captureHeld = expectation(description: "required capture admission is held")
        var releaseCapture: CheckedContinuation<Void, Never>?
        let model = makeModel(
            state: state, defaults: suite.defaults,
            updateCaptureObservationAsync: { observation, _ in
                guard observation != nil else { return .accepted }
                captureHeld.fulfill()
                await withCheckedContinuation { releaseCapture = $0 }
                return .rejected
            },
            captureTarget: { target }
        )
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let pending = try XCTUnwrap(
            model.submitObservation(
                observation(atMs: 200, identifier: "deleted-track"),
                wallClockAtMs: 1_700_000_000_200, clockUncertaintyMs: 7))
        await fulfillment(of: [captureHeld], timeout: 2)
        let deleted = await model.forgetHistory(for: ride.rideID)
        XCTAssertTrue(deleted)
        releaseCapture?.resume()
        let admitted = await pending.value
        XCTAssertFalse(admitted)
        XCTAssertNil(model.captureFailureReceipt)
        XCTAssertNil(model.coordinator.previousRideHistoryFailure)
        XCTAssertNil(model.historySaveError)
        XCTAssertTrue(model.timelineEvents.isEmpty)
        let playerAccepted = await model.ingestObservationAsync(
            observation(atMs: 300, identifier: "live-player"),
            wallClockAtMs: 1_700_000_000_300, clockUncertaintyMs: 9)
        XCTAssertTrue(playerAccepted)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "live-player")
        XCTAssertEqual(model.settingsNowPlaying?.isCommandAvailable(.pause), true)
        XCTAssertTrue(state.currentMusicEvents().isEmpty)
        try stopAfterExpectedRejectedMusicCapture(state: state, atMs: 400)
        _ = try state.discard()
    }

    private func stopAfterExpectedRejectedMusicCapture(state: MobileRideMapState, atMs: UInt64) throws {
        let originalRideID = try XCTUnwrap(state.currentSnapshot()?.rideID)
        do {
            _ = try state.stop(atMs: atMs)
            XCTFail("The known rejected required capture must produce an explicit incomplete Stop receipt")
            return
        } catch let error as MobileRideMapError {
            XCTAssertEqual(error, .storageError("ride music observation is incomplete"))
            guard error == .storageError("ride music observation is incomplete") else { throw error }
        }
        XCTAssertEqual(state.currentSnapshot()?.rideID, originalRideID)
        XCTAssertNotEqual(state.currentSnapshot()?.state, .stopped)
        let stopped = try state.stop(atMs: atMs)
        XCTAssertEqual(stopped.rideID, originalRideID)
        XCTAssertEqual(stopped.state, .stopped)
    }

    func testHistoryDeletionTargetsRequestedRideAfterAnAwaitedIdentityRead() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        let original = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        let readHeld = expectation(description: "deletion's old current-ride identity read held")
        var reads = 0
        var releaseRead: CheckedContinuation<Void, Never>?
        let model = makeModel(
            state: state, defaults: suite.defaults,
            historySnapshotReader: {
                let snapshot = await state.currentSnapshotAsync()
                reads += 1
                if reads == 1 {
                    readHeld.fulfill()
                    await withCheckedContinuation { releaseRead = $0 }
                }
                return snapshot
            })
        let deletion = Task { await model.forgetHistory(for: original.rideID) }
        await fulfillment(of: [readHeld], timeout: 2)
        _ = try state.stop(atMs: 200)
        _ = try state.save()
        let replacement = try await state.startGpsOnlyCommand(atMs: 300, musicHistoryPolicy: .humanReadable)
        _ = try state.recordMusicEvent(
            snapshot: observation(atMs: 400, identifier: "replacement-track").snapshot, kind: .play,
            monotonicAtMs: 400, wallClockAtMs: 1_700_000_000_400, clockUncertaintyMs: 5)
        let retained = try XCTUnwrap(state.currentMusicHistory())
        XCTAssertEqual(retained.events.count, 1)
        releaseRead?.resume()
        let deleted = await deletion.value
        XCTAssertTrue(deleted)
        let oldHistory = try state.storedMusicHistory(rideID: original.rideID)
        let replacementHistory = try state.storedMusicHistory(rideID: replacement.rideID)
        XCTAssertEqual(oldHistory.status, .deleted)
        XCTAssertEqual(replacementHistory.status, .available)
        XCTAssertEqual(replacementHistory.events, retained.events)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .humanReadable)
        _ = try state.stop(atMs: 500)
        _ = try state.discard()
    }

    func testOlderPolicyReadbackCannotReenableHistoryAfterANewerDisable() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .opaqueItem)
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        let readHeld = expectation(description: "older policy mutation awaiting authoritative readback")
        var reads = 0
        var releaseRead: CheckedContinuation<Void, Never>?
        let model = makeModel(
            state: state, defaults: suite.defaults, historyPolicyStore: store,
            historySnapshotReader: {
                let snapshot = await state.currentSnapshotAsync()
                reads += 1
                if reads == 2 {
                    readHeld.fulfill()
                    await withCheckedContinuation { releaseRead = $0 }
                }
                return snapshot
            })
        let older = Task { await model.setHistoryPolicyAsync(.humanReadable) }
        await fulfillment(of: [readHeld], timeout: 2)
        let latestAccepted = await model.setHistoryPolicyAsync(.disabled)
        XCTAssertTrue(latestAccepted)
        XCTAssertEqual(model.historyPolicy, .disabled)
        releaseRead?.resume()
        let olderAccepted = await older.value
        XCTAssertFalse(olderAccepted, "A superseded native publication must report that it was not applied")
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertEqual(model.preferredHistoryPolicy, .disabled)
        XCTAssertEqual(store.policy, .disabled)
        let durablePolicy = await state.currentMusicHistoryPolicyAsync()
        XCTAssertEqual(durablePolicy, .disabled)
        _ = try state.stop(atMs: 200)
        _ = try state.discard()
    }

    func testClosedHistoryReadDoesNotRetainMusicModelDuringSQLiteStall() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        var model: MusicFeatureModel? = makeModel(state: fixture.state, defaults: suite.defaults)
        weak let releasedModel = model
        try await fixture.holdWrite()
        model?.rideMapClosed()
        try await Task.sleep(for: .milliseconds(50))
        model = nil
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        XCTAssertNil(releasedModel, "A queued history read must not retain music presentation")
        try await fixture.finish()
    }

    func testSavedHistoryPreferenceSurvivesModelRecreationWithoutAnActiveRide() async throws {
        for policy in [MobileMusicHistoryPolicyDto.opaqueItem, .humanReadable] {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let store = MusicHistoryPolicyStore(defaults: suite.defaults)
            var capturePolicies = [MobileMusicHistoryPolicyDto]()
            var captureObservations = [MobilePevcapMusicEventDto?]()
            let initial = makeModel(
                state: MobileRideMapState(),
                defaults: suite.defaults,
                historyPolicyStore: store,
                updateCapturePolicy: { capturePolicies.append($0) },
                updateCaptureObservation: { captureObservations.append($0) }
            )
            let accepted = await initial.setHistoryPolicyAsync(policy)
            XCTAssertTrue(accepted)
            XCTAssertEqual(initial.preferredHistoryPolicy, policy)
            XCTAssertEqual(initial.historyPolicy, .disabled)
            XCTAssertEqual(capturePolicies.last, .disabled)
            XCTAssertTrue(initial.ingestObservation(observation(atMs: 50)))
            XCTAssertNil(initial.historySaveError)
            XCTAssertTrue(initial.timelineEvents.isEmpty)
            XCTAssertFalse(captureObservations.isEmpty)
            XCTAssertTrue(captureObservations.allSatisfy { $0 == nil })

            let restored = makeModel(state: MobileRideMapState(), defaults: suite.defaults, historyPolicyStore: store)
            restored.synchronizeHistory(nil)

            XCTAssertEqual(restored.preferredHistoryPolicy, policy)
            XCTAssertEqual(restored.historyPolicy, .disabled)
            XCTAssertEqual(store.policy, policy)
        }
    }

    func testRestoredRideRetentionDoesNotReplaceSavedHistoryPreference() async throws {
        for savedPolicy in [MobileMusicHistoryPolicyDto.disabled, .opaqueItem, .humanReadable] {
            for ridePolicy in [MobileMusicHistoryPolicyDto.disabled, .opaqueItem, .humanReadable] {
                let suite = try makeDefaults()
                defer { suite.defaults.removePersistentDomain(forName: suite.name) }
                let store = MusicHistoryPolicyStore(defaults: suite.defaults)
                store.set(savedPolicy)
                let state = MobileRideMapState()
                _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: ridePolicy)
                let model = makeModel(state: state, defaults: suite.defaults, historyPolicyStore: store)

                let error = await model.adoptHistoryForNewRideAsync()

                XCTAssertNil(error)
                XCTAssertEqual(model.preferredHistoryPolicy, savedPolicy)
                XCTAssertEqual(store.policy, savedPolicy)
                XCTAssertEqual(model.historyPolicy, ridePolicy)
                XCTAssertEqual(state.currentMusicHistoryPolicy(), ridePolicy)
            }
        }
    }

    func testClosingRideDisablesEffectivePolicyWithoutChangingPreference() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        store.set(.opaqueItem)
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        var capturePolicies = [MobileMusicHistoryPolicyDto]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: store,
            updateCapturePolicy: { capturePolicies.append($0) }
        )

        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        XCTAssertEqual(model.historyPolicy, .humanReadable)

        model.rideMapClosed()

        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertEqual(model.preferredHistoryPolicy, .opaqueItem)
        XCTAssertEqual(store.policy, .opaqueItem)
        XCTAssertEqual(capturePolicies.last, .disabled)
    }

    func testHistoryWriteFailurePreservesDistinctPreferenceAndEffectivePolicy() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        store.set(.opaqueItem)
        let state = MobileRideMapState(storageUnavailable: "history write unavailable")
        let model = makeModel(state: state, defaults: suite.defaults, historyPolicyStore: store)
        model.synchronizeHistory(nil)

        let accepted = await model.setHistoryPolicyAsync(.humanReadable)

        XCTAssertFalse(accepted)
        XCTAssertEqual(model.preferredHistoryPolicy, .opaqueItem)
        XCTAssertEqual(store.policy, .opaqueItem)
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertNotNil(model.historySaveError)
    }

    func testStaleHistoryReadbackCannotUndoPolicyChange() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        store.set(.disabled)
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        var capturedPolicies = [MobileMusicHistoryPolicyDto]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: store,
            updateCapturePolicy: { capturedPolicies.append($0) }
        )
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let staleHistory = try XCTUnwrap(state.currentMusicHistory())
        let staleRevision = model.historyReadbackRevision

        let didSetPolicy = await model.setHistoryPolicyAsync(.opaqueItem)
        XCTAssertTrue(didSetPolicy)
        XCTAssertFalse(model.synchronizeHistory(staleHistory, ifCurrentRevision: staleRevision))

        XCTAssertEqual(model.historyPolicy, .opaqueItem)
        XCTAssertEqual(model.preferredHistoryPolicy, .opaqueItem)
        XCTAssertEqual(store.policy, .opaqueItem)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .opaqueItem)
        XCTAssertEqual(capturedPolicies.last, .opaqueItem)
    }

    func testDeletingOlderHistoryPreservesPendingCurrentRideReadback() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        store.set(.opaqueItem)
        let state = MobileRideMapState()
        let olderRide = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        _ = try state.stop(atMs: 200)
        _ = try state.save()
        _ = try await state.startGpsOnlyCommand(atMs: 300, musicHistoryPolicy: .humanReadable)
        var capturedPolicies = [MobileMusicHistoryPolicyDto]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: store,
            updateCapturePolicy: { capturedPolicies.append($0) }
        )
        let currentHistory = try XCTUnwrap(state.currentMusicHistory())
        let readbackRevision = model.historyReadbackRevision

        let didForget = await model.forgetHistory(for: olderRide.rideID)
        XCTAssertTrue(didForget)
        XCTAssertTrue(model.synchronizeHistory(currentHistory, ifCurrentRevision: readbackRevision))

        XCTAssertEqual(try state.storedMusicHistory(rideID: olderRide.rideID).status, .deleted)
        XCTAssertEqual(model.historyPolicy, .humanReadable)
        XCTAssertEqual(model.preferredHistoryPolicy, .opaqueItem)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .humanReadable)
        XCTAssertEqual(capturedPolicies.last, .humanReadable)
    }

    func testOlderRideAdoptionPreservesReplacementRideReadback() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        store.set(.humanReadable)
        let state = MobileRideMapState()
        let originalRide = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        weak var modelReference: MusicFeatureModel?
        var replacementHistory: MobileMusicHistoryDto?
        var replacementRevision: UInt64?
        var replacementError: Error?
        var didReplace = false
        var capturePolicies = [MobileMusicHistoryPolicyDto]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: store,
            updateCapturePolicy: { policy in
                capturePolicies.append(policy)
                guard !didReplace else { return }
                didReplace = true
                do {
                    _ = try state.stop(atMs: 200)
                    _ = try state.save()
                    _ = try state.startGpsOnly(atMs: 300)
                    try state.setMusicHistoryPolicy(.opaqueItem)
                    replacementHistory = state.currentMusicHistory()
                    replacementRevision = modelReference?.historyReadbackRevision
                } catch {
                    replacementError = error
                }
            }
        )
        modelReference = model

        let adoptionError = await model.adoptHistoryForNewRideAsync()

        XCTAssertNil(adoptionError)
        XCTAssertNil(replacementError)
        XCTAssertTrue(didReplace)
        XCTAssertNotEqual(state.currentSnapshot()?.rideID, originalRide.rideID)
        XCTAssertTrue(
            model.synchronizeHistory(
                try XCTUnwrap(replacementHistory),
                ifCurrentRevision: try XCTUnwrap(replacementRevision)
            ))
        XCTAssertEqual(model.historyPolicy, .opaqueItem)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .opaqueItem)
        XCTAssertEqual(model.preferredHistoryPolicy, .humanReadable)
        XCTAssertEqual(store.policy, .humanReadable)
        XCTAssertEqual(capturePolicies.last, .opaqueItem)
    }

    func testOlderRidePolicyCompletionPreservesReplacementRideReadbackAndTimeline() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        store.set(.disabled)
        let state = MobileRideMapState()
        let originalRide = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .disabled)
        let replacementObservation = observation(atMs: 400)
        weak var modelReference: MusicFeatureModel?
        var replacementHistory: MobileMusicHistoryDto?
        var replacementRevision: UInt64?
        var replacementError: Error?
        var didReplace = false
        var capturePolicies = [MobileMusicHistoryPolicyDto]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: store,
            updateCapturePolicy: { policy in
                capturePolicies.append(policy)
                guard !didReplace else { return }
                didReplace = true
                do {
                    _ = try state.stop(atMs: 200)
                    _ = try state.save()
                    _ = try state.startGpsOnly(atMs: 300)
                    try state.setMusicHistoryPolicy(.opaqueItem)
                    _ = try state.recordMusicEvent(
                        snapshot: replacementObservation.snapshot,
                        kind: .play,
                        monotonicAtMs: 400,
                        wallClockAtMs: 1_700_000_000_400,
                        clockUncertaintyMs: 5
                    )
                    replacementHistory = state.currentMusicHistory()
                    replacementRevision = modelReference?.historyReadbackRevision
                } catch {
                    replacementError = error
                }
            }
        )
        modelReference = model

        let accepted = await model.setHistoryPolicyAsync(.humanReadable)

        XCTAssertTrue(accepted)
        XCTAssertNil(replacementError)
        XCTAssertTrue(didReplace)
        XCTAssertNotEqual(state.currentSnapshot()?.rideID, originalRide.rideID)
        XCTAssertTrue(model.timelineEvents.isEmpty, "the older completion must not publish the replacement timeline")
        let history = try XCTUnwrap(replacementHistory)
        XCTAssertEqual(history.events.count, 1)
        XCTAssertTrue(model.synchronizeHistory(history, ifCurrentRevision: try XCTUnwrap(replacementRevision)))
        XCTAssertEqual(model.timelineEvents.count, 1)
        XCTAssertNil(model.timelineEvents.first?.title)
        XCTAssertEqual(model.historyPolicy, .opaqueItem)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .opaqueItem)
        XCTAssertEqual(model.preferredHistoryPolicy, .humanReadable)
        XCTAssertEqual(store.policy, .humanReadable)
        XCTAssertEqual(capturePolicies.last, .opaqueItem)
    }

    func testStaleHistoryReadbackCannotRestoreDeletedCurrentHistory() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let store = MusicHistoryPolicyStore(defaults: suite.defaults)
        store.set(.humanReadable)
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        var capturedPolicies = [MobileMusicHistoryPolicyDto]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: store,
            updateCapturePolicy: { capturedPolicies.append($0) }
        )
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        let staleHistory = try XCTUnwrap(state.currentMusicHistory())
        let staleRevision = model.historyReadbackRevision
        let rideID = try XCTUnwrap(state.currentSnapshot()?.rideID)

        let didForget = await model.forgetHistory(for: rideID)
        XCTAssertTrue(didForget)
        XCTAssertFalse(model.synchronizeHistory(staleHistory, ifCurrentRevision: staleRevision))

        XCTAssertEqual(state.currentMusicHistory()?.status, .deleted)
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertEqual(model.preferredHistoryPolicy, .humanReadable)
        XCTAssertEqual(store.policy, .humanReadable)
        XCTAssertEqual(capturedPolicies.last, .disabled)
    }

    func testNewRideHistoryAdoptionReadsRustPolicyWithoutWritingSavedDefault() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let policyStore = MusicHistoryPolicyStore(defaults: suite.defaults)
        policyStore.set(.humanReadable)
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .opaqueItem)
        var captureWasCleared = false
        var capturePolicies = [MobileMusicHistoryPolicyDto]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: policyStore,
            updateCapturePolicy: { capturePolicies.append($0) },
            updateCaptureObservation: { captureWasCleared = $0 == nil }
        )
        model.setHistoryPersistenceError(.storageError("previous ride failure"))

        let error = await model.adoptHistoryForNewRideAsync()

        XCTAssertNil(error)
        XCTAssertTrue(captureWasCleared)
        XCTAssertEqual(model.historyPolicy, .opaqueItem)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .opaqueItem)
        XCTAssertEqual(policyStore.policy, .humanReadable)
        XCTAssertEqual(capturePolicies.last, .opaqueItem)
        XCTAssertTrue(model.timelineEvents.isEmpty)
        XCTAssertFalse(model.historyUnavailable)
        XCTAssertNil(model.historySaveError)

        XCTAssertTrue(model.ingestObservation(observation(atMs: 200), wallClockAtMs: 1_700_000_000_100))
        XCTAssertEqual(state.currentMusicHistory()?.status, .redacted)
        XCTAssertNil(state.currentMusicEvents().first?.title)
    }

    func testNewRideHistoryAdoptionPreservesDeletedHistory() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let policyStore = MusicHistoryPolicyStore(defaults: suite.defaults)
        policyStore.set(.humanReadable)
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .humanReadable)
        try await state.deleteCurrentMusicHistoryAsync()
        let model = makeModel(state: state, defaults: suite.defaults, historyPolicyStore: policyStore)

        let error = await model.adoptHistoryForNewRideAsync()

        XCTAssertNil(error)
        XCTAssertEqual(state.currentMusicHistory()?.status, .deleted)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .disabled)
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertEqual(model.preferredHistoryPolicy, .humanReadable)
        XCTAssertTrue(model.timelineEvents.isEmpty)
        XCTAssertEqual(policyStore.policy, .humanReadable)
    }

    func testNewRideHistoryReadbackFailureRemainsVisibleAndClearsTimeline() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let state = MobileRideMapState(storageUnavailable: "history storage unavailable")
        var reportedError: MobileRideMapError?
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            setRideHistoryError: { reportedError = $0 }
        )

        let error = await model.adoptHistoryForNewRideAsync()

        XCTAssertEqual(error, .storageError("history storage unavailable"))
        XCTAssertEqual(reportedError, error)
        XCTAssertEqual(model.historySaveError, error)
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertTrue(model.timelineEvents.isEmpty)
    }

    func testSpotifyPlayWithoutSnapshotStillDispatchesExplicitProviderCommand() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let providerStore = MusicProviderSelectionStore(defaults: suite.defaults)
        providerStore.set(.spotify)
        var commands = [(MobileMusicProviderDto, MobileMusicCommandDto)]()
        let music = makeModel(
            defaults: suite.defaults,
            providerSelectionStore: providerStore,
            providerCommandHandler: { provider, command in
                commands.append((provider, command))
                return .accepted
            }
        )

        XCTAssertNil(music.settingsNowPlaying)
        let result = await music.handleCommand(.play)

        XCTAssertEqual(result, .accepted)
        XCTAssertEqual(commands.count, 1)
        XCTAssertEqual(commands.first?.0, .spotify)
        XCTAssertEqual(commands.first?.1, .play)
    }

    func testOpeningHistoricalRideDetailDoesNotDispatchMusicTransport() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let rideID = "historical-ride"
        let historyQuery = HistoricalRideQuery(rideID: rideID)
        var providerCommands = [(MobileMusicProviderDto, MobileMusicCommandDto)]()
        let music = makeModel(
            defaults: suite.defaults,
            providerCommandHandler: { provider, command in
                providerCommands.append((provider, command))
                return .unavailable
            }
        )
        let history = RideHistoryModel(
            stateProvider: { historyQuery },
            storageErrorProvider: { nil }
        )
        XCTAssertTrue(music.ingestObservation(observation(atMs: 1_000)))
        let playingProjection = try XCTUnwrap(music.settingsNowPlaying)

        history.reload(selecting: rideID)
        await waitUntil("historical ride detail load") {
            history.selectedRideID == rideID
                && !history.isLoading
                && !history.routeLoading
                && !history.detailRouteLoading
        }

        XCTAssertEqual(music.selectedProvider, .appleMusic)
        XCTAssertEqual(music.settingsNowPlaying, playingProjection)
        XCTAssertNil(history.routeError)
        XCTAssertNil(history.detailRouteError)
        XCTAssertFalse(history.detailMusicTimelineUnavailable)
        XCTAssertTrue(
            providerCommands.isEmpty,
            "loading historical ride detail must not send music transport commands"
        )
    }

    #if canImport(SpotifyiOS) && os(iOS)
        func testSpotifyCallbackAdmissionSurvivesEitherSceneEventOrder() throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let redirectURL = try XCTUnwrap(URL(string: "cutout-spotify://spotify-login-callback"))
            let callbackURL = try XCTUnwrap(
                URL(string: "cutout-spotify://spotify-login-callback/#access_token=test")
            )

            for callbackBeforeResume in [true, false] {
                let providerStore = MusicProviderSelectionStore(defaults: suite.defaults)
                providerStore.set(.spotify)
                var callbackModel: MusicFeatureModel?
                var callbackAuthorizationID: MobileMusicAuthorizationId?
                var handedOff = [URL]()
                let music = makeModel(
                    defaults: suite.defaults,
                    providerSelectionStore: providerStore,
                    spotifyCallbackHandler: { url in
                        guard let model = callbackModel else { return false }
                        return SpotifyAuthorizationCallbackGate.dispatch(
                            url,
                            redirectURL: redirectURL,
                            authorizationID: callbackAuthorizationID,
                            lifecycle: model.providerLifecycle
                        ) { acceptedURL in
                            handedOff.append(acceptedURL)
                            return true
                        }
                    }
                )
                callbackModel = music
                let lifecycle = music.providerLifecycle
                lifecycle.requestMonitor(request: .authorize)
                XCTAssertEqual(lifecycle.beginMonitor()?.start, .authorize)
                let authorization = try XCTUnwrap(
                    lifecycle.beginAuthorizationEffect(
                        kind: .authorizing,
                        nowMs: 1_000
                    ))
                callbackAuthorizationID = authorization.id
                _ = try XCTUnwrap(lifecycle.beginProviderSession())
                XCTAssertTrue(lifecycle.suspend().observationGap)

                if callbackBeforeResume {
                    XCTAssertTrue(music.handleProviderURL(callbackURL))
                    XCTAssertEqual(
                        lifecycle.finishAuthorization(id: authorization.id),
                        .authorizing
                    )
                }

                XCTAssertEqual(lifecycle.resume(), .restored)
                XCTAssertEqual(lifecycle.beginMonitor()?.start, .observe)

                if !callbackBeforeResume {
                    XCTAssertTrue(music.handleProviderURL(callbackURL))
                    XCTAssertEqual(
                        lifecycle.finishAuthorization(id: authorization.id),
                        .authorizing
                    )
                }

                XCTAssertEqual(handedOff, [callbackURL])
                XCTAssertFalse(music.handleProviderURL(callbackURL))
                XCTAssertEqual(handedOff, [callbackURL], "duplicate callback must not reach the SDK handoff")
            }
        }
    #endif

    func testDeletingHistoryInvalidatesDetailBeforeRustDeleteAndClearsCaptureBeforeSelection() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let policyStore = MusicHistoryPolicyStore(defaults: suite.defaults)
        let selectedRideID = state.currentSnapshot()?.rideID
        var order = [String]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: policyStore,
            selectedRideID: { selectedRideID },
            invalidateHistory: {
                XCTAssertFalse(state.currentMusicHistory()?.events.isEmpty ?? true)
                order.append("invalidate")
            },
            updateCaptureObservation: { observation in
                if observation == nil { order.append("capture-clear") }
            },
            clearSelectedHistoryMusic: { order.append("selection-clear") }
        )
        XCTAssertTrue(model.setHistoryPolicy(.humanReadable))
        XCTAssertTrue(model.ingestObservation(observation(atMs: 200), wallClockAtMs: 1_700_000_000_000))
        let rideID = try XCTUnwrap(state.currentSnapshot()?.rideID)

        let didForget = await model.forgetHistory(for: rideID)
        XCTAssertTrue(didForget)

        XCTAssertEqual(order, ["invalidate", "capture-clear", "selection-clear"])
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertTrue(model.timelineEvents.isEmpty)
        XCTAssertEqual(policyStore.policy, .humanReadable)
        XCTAssertTrue(state.currentMusicEvents().isEmpty)
    }

    func testHiddenPlayerKeepsSettingsProjectionAndMonitoringIntentIndependent() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let visibility = MusicPlayerVisibilityStore()
        let wasHidden = visibility.isHidden
        visibility.setHidden(false)
        defer { visibility.setHidden(wasHidden) }
        let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
        monitoring.setEnabled(false)
        let model = makeModel(defaults: suite.defaults, monitoringPreferenceStore: monitoring)
        XCTAssertTrue(model.ingestObservation(observation(atMs: 10)))
        let settingsProjection = model.settingsNowPlaying

        model.dismissPlayer()

        XCTAssertTrue(model.isPlayerHidden)
        XCTAssertNil(model.nowPlaying)
        XCTAssertEqual(model.settingsNowPlaying, settingsProjection)
        XCTAssertFalse(monitoring.isEnabled)
    }

    func testLateCommandFeedbackCannotOverwriteAfterProviderSwitch() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)
        let requestID = try XCTUnwrap(model.beginCommandFeedback())
        XCTAssertNotNil(model.commandFeedback)

        model.selectProvider(.spotify)
        _ = model.finishCommand(.failed, provider: .appleMusic, requestID: requestID)

        XCTAssertNil(model.commandFeedback)
    }

    func testOpaqueSpotifyLocalIdentifierIsNotCopiedIntoPevcap() {
        XCTAssertNil(
            MusicFeatureModel.pevcapTrackIdentifier(
                policy: .opaqueItem,
                provider: .spotify,
                identifier: "spotify:local:artist:album:track"
            ))
        XCTAssertEqual(
            MusicFeatureModel.pevcapTrackIdentifier(
                policy: .opaqueItem,
                provider: .spotify,
                identifier: "spotify:track:catalog-id"
            ), "spotify:track:catalog-id")
        XCTAssertEqual(
            MusicFeatureModel.pevcapTrackIdentifier(
                policy: .humanReadable,
                provider: .spotify,
                identifier: "spotify:local:artist:album:track"
            ), "spotify:local:artist:album:track")
    }

    func testHistoryPolicyLoadsFromItsStoreAndRedactsTheRustTimeline() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let policyStore = MusicHistoryPolicyStore(defaults: suite.defaults)
        policyStore.set(.opaqueItem)
        let state = MobileRideMapState()
        _ = try await state.startGpsOnlyCommand(atMs: 100, musicHistoryPolicy: .opaqueItem)
        let model = makeModel(state: state, defaults: suite.defaults, historyPolicyStore: policyStore)

        XCTAssertEqual(model.historyPolicy, .disabled)
        let adoptionError = await model.adoptHistoryForNewRideAsync()
        XCTAssertNil(adoptionError)
        XCTAssertEqual(model.historyPolicy, .opaqueItem)
        XCTAssertEqual(model.preferredHistoryPolicy, .opaqueItem)
        let enabled = await model.setHistoryPolicyAsync(.humanReadable)
        XCTAssertTrue(enabled)
        let ingested = await model.ingestObservationAsync(observation(atMs: 200))
        XCTAssertTrue(ingested)
        XCTAssertEqual(model.timelineEvents.first?.title, "Track")

        let redacted = await model.setHistoryPolicyAsync(.opaqueItem)
        XCTAssertTrue(redacted)

        XCTAssertEqual(model.timelineEvents.count, 1)
        XCTAssertNil(model.timelineEvents.first?.title)
        XCTAssertNil(model.timelineEvents.first?.artist)
        XCTAssertNil(state.currentMusicEvents().first?.title)
        XCTAssertEqual(policyStore.policy, .opaqueItem)
    }

    func testRestoringPlayerAfterHiddenProviderSwitchDoesNotShowPreviousProvider() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let visibility = MusicPlayerVisibilityStore()
        let wasHidden = visibility.isHidden
        visibility.setHidden(false)
        defer { visibility.setHidden(wasHidden) }
        let model = makeModel(defaults: suite.defaults)
        model.restorePlayer()
        XCTAssertTrue(model.ingestObservation(observation(atMs: 10)))
        model.dismissPlayer()
        model.selectProvider(.spotify)

        model.restorePlayer()

        // The SDK may not have delivered its first observation yet.
        if let nowPlaying = model.nowPlaying {
            XCTAssertEqual(nowPlaying.provider, .spotify)
            XCTAssertEqual(nowPlaying.state, .unavailable)
        }
        XCTAssertNil(model.nowPlaying?.item)
    }

    #if canImport(MediaPlayer) && os(iOS)
        func testActualMonitorTaskUsesPassiveAuthorizationAndCancelsOnBackground() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
            monitoring.setEnabled(true)
            let monitor = TestAppleMusicMonitor()
            let pollWaiter = TestMusicMonitorPollWaiter()
            var captureObservations = [MobilePevcapMusicEventDto?]()
            let model = makeModel(
                defaults: suite.defaults,
                monitoringPreferenceStore: monitoring,
                updateCaptureObservation: { captureObservations.append($0) },
                appleMonitor: monitor,
                monitorPollWaiter: { deadlineMs, _ in
                    await pollWaiter.wait(until: deadlineMs)
                }
            )

            model.start(sceneIsActive: true)
            await waitUntil("first monitor poll") { pollWaiter.startedCount == 1 }
            await waitUntil("first monitor presentation") {
                model.settingsNowPlaying?.item?.identifier == "monitor-track"
            }

            XCTAssertEqual(monitor.authorizationPrompts, [false])
            XCTAssertEqual(monitor.startCount, 1)
            XCTAssertEqual(
                monitor.stopCount, 1, "starting a new generation first tears down any prior provider session")
            XCTAssertEqual(model.settingsNowPlaying?.state, MobileMusicPlaybackStateDto.playing)
            XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "monitor-track")
            XCTAssertTrue(model.timelineEvents.isEmpty)
            XCTAssertFalse(captureObservations.contains { $0 != nil })

            model.start(sceneIsActive: true)
            await waitUntil("replacement monitor poll") { pollWaiter.startedCount == 2 }
            XCTAssertEqual(monitor.startCount, 2)
            XCTAssertEqual(monitor.stopCount, 2)

            pollWaiter.releaseNext(returning: false)
            await waitUntil("superseded monitor poll") { pollWaiter.completedCount == 1 }
            XCTAssertEqual(monitor.stopCount, 2, "the superseded task must not stop the replacement monitor")

            model.sceneDidEnterBackground()
            XCTAssertEqual(monitor.stopCount, 3)
            pollWaiter.releaseNext(returning: false)
            await waitUntil("cancelled monitor poll") { pollWaiter.completedCount == 2 }
            await Task.yield()

            XCTAssertEqual(monitor.stopCount, 3, "a stale task must not tear down a later provider session")
            XCTAssertEqual(monitor.suspensionCount, 1)
            XCTAssertFalse(monitor.authorizationPrompts.contains(true))
        }

        func testExplicitShutdownInvalidatesLateMonitorObservationBeforeAdapterTeardown() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
            monitoring.setEnabled(true)
            let monitor = TestAppleMusicMonitor()
            let pollWaiter = TestMusicMonitorPollWaiter()
            let model = makeModel(
                defaults: suite.defaults,
                monitoringPreferenceStore: monitoring,
                appleMonitor: monitor,
                monitorPollWaiter: { deadlineMs, _ in
                    await pollWaiter.wait(until: deadlineMs)
                }
            )

            model.start(sceneIsActive: true)
            await waitUntil("monitor poll before shutdown") { pollWaiter.startedCount == 1 }
            await waitUntil("monitor presentation before shutdown") {
                model.settingsNowPlaying?.item?.identifier == "monitor-track"
            }
            let nowPlayingBeforeShutdown = try XCTUnwrap(model.settingsNowPlaying)

            monitor.emitOnNextStop(observation(atMs: 2_000, identifier: "teardown-track"))
            model.stopMonitoring()
            monitor.emit(observation(atMs: 2_000, identifier: "late-track"))
            pollWaiter.releaseNext(returning: false)
            await waitUntil("shutdown poll completion") { pollWaiter.completedCount == 1 }

            XCTAssertEqual(monitor.stopCount, 2)
            XCTAssertEqual(model.settingsNowPlaying, nowPlayingBeforeShutdown)
            XCTAssertTrue(model.timelineEvents.isEmpty)
        }

        func testAppModelDeallocationStopsMusicMonitor() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
            monitoring.setEnabled(true)
            let monitor = TestAppleMusicMonitor()
            var model: CutoutAppModel? = CutoutAppModel(
                core: MusicMonitorSessionDriver(),
                musicHistoryPolicyStore: MusicHistoryPolicyStore(defaults: suite.defaults),
                musicProviderSelectionStore: MusicProviderSelectionStore(defaults: suite.defaults),
                musicMonitoringPreferenceStore: monitoring,
                appleMusicMonitor: monitor
            )
            weak var weakModel = model

            model?.music.start(sceneIsActive: true)
            await waitUntil("app-owned music monitor start") { monitor.startCount == 1 }

            model = nil

            XCTAssertNil(weakModel)
            await waitUntil("app-model deallocation stops music adapter") { monitor.stopCount == 2 }
        }

        func testAppModelKeepsOneMusicMonitorAcrossStartupAndSceneResume() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
            monitoring.setEnabled(true)
            let monitor = TestAppleMusicMonitor()
            let model = CutoutAppModel(
                core: MusicMonitorSessionDriver(),
                musicHistoryPolicyStore: MusicHistoryPolicyStore(defaults: suite.defaults),
                musicProviderSelectionStore: MusicProviderSelectionStore(defaults: suite.defaults),
                musicMonitoringPreferenceStore: monitoring,
                appleMusicMonitor: monitor
            )

            model.start(sceneIsActive: true)
            await waitUntil("root-owned music monitor start") { monitor.startCount == 1 }

            model.start(sceneIsActive: true)
            model.appDidBecomeActive()
            await Task.yield()
            XCTAssertEqual(
                monitor.startCount, 1, "repeated startup and active notifications must not duplicate the monitor")

            model.appDidEnterBackground()
            await waitUntil("root music monitor suspension") { monitor.stopCount >= 2 }
            model.appDidBecomeActive()
            await waitUntil("root music monitor restoration") { monitor.startCount == 2 }

            model.music.stopMonitoring()
            await waitUntil("root music monitor shutdown") { monitor.stopCount >= 4 }
            XCTAssertEqual(monitor.startCount, 2)
        }

        func testExplicitConnectAllowsAuthorizationPrompt() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let monitor = TestAppleMusicMonitor()
            let pollWaiter = TestMusicMonitorPollWaiter()
            let model = makeModel(
                defaults: suite.defaults,
                appleMonitor: monitor,
                monitorPollWaiter: { deadlineMs, _ in
                    await pollWaiter.wait(until: deadlineMs)
                }
            )

            model.connect()
            await waitUntil("explicit authorization monitor poll") { pollWaiter.startedCount == 1 }

            XCTAssertEqual(monitor.authorizationPrompts, [true])
            XCTAssertEqual(monitor.startCount, 1)

            model.stopMonitoring()
            pollWaiter.releaseNext(returning: false)
            await waitUntil("explicit authorization monitor cancellation") { pollWaiter.completedCount == 1 }
        }

        func testConnectAllowsAuthorizationPromptForUnauthorizedSnapshot() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let monitor = TestAppleMusicMonitor()
            let pollWaiter = TestMusicMonitorPollWaiter()
            let model = makeModel(
                defaults: suite.defaults,
                appleMonitor: monitor,
                monitorPollWaiter: { deadlineMs, _ in
                    await pollWaiter.wait(until: deadlineMs)
                }
            )
            let unauthorized = MusicProviderObservation(
                snapshot: monitor.unauthorizedSnapshot(observedAtMs: 1_000)
            )
            XCTAssertTrue(model.ingestObservation(unauthorized))

            model.connect()
            await waitUntil("explicit authorization monitor poll after unauthorized snapshot") {
                pollWaiter.startedCount == 1
            }

            XCTAssertEqual(monitor.authorizationPrompts, [true])
            XCTAssertEqual(monitor.startCount, 1)

            model.stopMonitoring()
            pollWaiter.releaseNext(returning: false)
            await waitUntil("unauthorized authorization monitor cancellation") { pollWaiter.completedCount == 1 }
        }

        func testConnectReusesAuthorizationForExistingPlaybackSnapshots() async throws {
            let states: [MobileMusicPlaybackStateDto] = [
                .stale, .disconnected, .unavailable, .paused, .stopped, .playing, .buffering, .interrupted,
            ]

            for state in states {
                let suite = try makeDefaults()
                defer { suite.defaults.removePersistentDomain(forName: suite.name) }
                let monitor = TestAppleMusicMonitor()
                let pollWaiter = TestMusicMonitorPollWaiter()
                let model = makeModel(
                    defaults: suite.defaults,
                    appleMonitor: monitor,
                    monitorPollWaiter: { deadlineMs, _ in
                        await pollWaiter.wait(until: deadlineMs)
                    }
                )
                XCTAssertTrue(model.ingestObservation(observation(atMs: 1_000, state: state)), "seed \(state)")

                model.connect()
                await waitUntil("passive reconnect for \(state)") { pollWaiter.startedCount == 1 }

                XCTAssertEqual(monitor.authorizationPrompts, [false], "\(state) must reuse saved authorization")
                XCTAssertEqual(monitor.startCount, 1, "\(state) should reconnect the provider")

                model.stopMonitoring()
                pollWaiter.releaseNext(returning: false)
                await waitUntil("reconnect cancellation for \(state)") { pollWaiter.completedCount == 1 }
            }
        }

        func testColdStartRestoreUsesPassiveAuthorizationAndShowsPlayer() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let visibility = MusicPlayerVisibilityStore()
            let wasHidden = visibility.isHidden
            visibility.setHidden(true)
            defer { visibility.setHidden(wasHidden) }

            let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
            XCTAssertFalse(monitoring.isEnabled)
            let monitor = TestAppleMusicMonitor()
            let pollWaiter = TestMusicMonitorPollWaiter()
            let model = makeModel(
                defaults: suite.defaults,
                monitoringPreferenceStore: monitoring,
                appleMonitor: monitor,
                monitorPollWaiter: { deadlineMs, _ in
                    await pollWaiter.wait(until: deadlineMs)
                }
            )
            XCTAssertNil(model.settingsNowPlaying)
            XCTAssertTrue(model.isPlayerHidden)

            model.restorePlayer()
            await waitUntil("cold-start restore monitor poll") { pollWaiter.startedCount == 1 }
            await waitUntil("cold-start restore playback observation") {
                model.settingsNowPlaying?.item?.identifier == "monitor-track"
            }

            XCTAssertEqual(monitor.authorizationPrompts, [false])
            XCTAssertEqual(monitor.startCount, 1)
            XCTAssertTrue(monitoring.isEnabled)
            XCTAssertFalse(model.isPlayerHidden)
            XCTAssertFalse(visibility.isHidden)

            model.stopMonitoring()
            pollWaiter.releaseNext(returning: false)
            await waitUntil("cold-start restore monitor cancellation") { pollWaiter.completedCount == 1 }
        }

        func testRestorePlayerPreservesCurrentFeedbackWhenMonitorIsAlreadyActive() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let visibility = MusicPlayerVisibilityStore()
            let wasHidden = visibility.isHidden
            visibility.setHidden(true)
            defer { visibility.setHidden(wasHidden) }

            let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
            monitoring.setEnabled(true)
            let monitor = TestAppleMusicMonitor()
            let pollWaiter = TestMusicMonitorPollWaiter()
            let model = makeModel(
                defaults: suite.defaults,
                monitoringPreferenceStore: monitoring,
                appleMonitor: monitor,
                monitorPollWaiter: { deadlineMs, _ in
                    await pollWaiter.wait(until: deadlineMs)
                }
            )

            model.connect()
            await waitUntil("active music monitor poll") { pollWaiter.startedCount == 1 }
            await waitUntil("active music monitor observation") { monitor.startCount == 1 }
            let requestID = try XCTUnwrap(model.beginCommandFeedback())
            _ = model.finishCommand(.failed, provider: .appleMusic, requestID: requestID)
            let failedFeedback = try XCTUnwrap(model.commandFeedback)

            model.restorePlayer()

            XCTAssertEqual(model.commandFeedback, failedFeedback)
            XCTAssertEqual(monitor.startCount, 1, "restoring an active player must reuse its monitor")
            XCTAssertFalse(model.isPlayerHidden)

            model.stopMonitoring()
            pollWaiter.releaseNext(returning: false)
            await waitUntil("active music monitor cancellation") { pollWaiter.completedCount == 1 }
        }
    #endif

    #if !os(iOS)
        func testUnavailableMusicCommandPublishesVisibleFeedback() async throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let model = makeModel(defaults: suite.defaults)

            let outcome = await model.handleCommand(.play)

            XCTAssertEqual(outcome, .unavailable)
            XCTAssertEqual(model.commandStatusText, pevLocalizedText("music.command.unavailable"))
        }

        func testOlderSameProviderCompletionAndDismissalCannotReplaceNewerFeedback() throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let model = makeModel(defaults: suite.defaults)
            let first = try XCTUnwrap(model.beginCommandFeedback())
            let second = try XCTUnwrap(model.beginCommandFeedback())

            _ = model.finishCommand(.failed, provider: .appleMusic, requestID: first)
            XCTAssertNil(model.commandStatusText)

            _ = model.finishCommand(.refused, provider: .appleMusic, requestID: second)
            XCTAssertEqual(model.commandStatusText, pevLocalizedText("music.command.refused"))

            model.dismissCommandFeedback(requestID: first)
            XCTAssertEqual(model.commandStatusText, pevLocalizedText("music.command.refused"))
            model.dismissCommandFeedback(requestID: second)
            XCTAssertNil(model.commandStatusText)
        }

        func testSystemAlertDismissalClearsCurrentMusicCommandFeedback() throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let model = makeModel(defaults: suite.defaults)
            let requestID = try XCTUnwrap(model.beginCommandFeedback())
            _ = model.finishCommand(.failed, provider: .appleMusic, requestID: requestID)

            model.dismissCommandFeedback()

            XCTAssertNil(model.commandFeedback)
        }

        func testMusicSetupShowsUnavailableOnUnsupportedPlatform() throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let model = makeModel(defaults: suite.defaults)

            model.connect()

            XCTAssertEqual(model.nowPlaying?.state, .unavailable)
            XCTAssertEqual(model.nowPlaying?.provider, .appleMusic)
        }

        func testValidObservationClearsRecoveredValidationErrorWithoutATransition() throws {
            let suite = try makeDefaults()
            defer { suite.defaults.removePersistentDomain(forName: suite.name) }
            let model = makeModel(defaults: suite.defaults)
            let capabilities = MobileMusicCapabilitiesDto(
                previous: true,
                play: false,
                pause: true,
                next: true,
                openProvider: true
            )
            func observation(
                atMs: UInt64,
                positionMilliseconds: UInt64?,
                durationMilliseconds: UInt64?
            ) -> MusicProviderObservation {
                MusicProviderObservation(
                    snapshot: MobileMusicSnapshotDto(
                        provider: .appleMusic,
                        sessionId: "session",
                        state: .playing,
                        item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
                        positionMilliseconds: positionMilliseconds,
                        durationMilliseconds: durationMilliseconds,
                        observedAtMs: atMs,
                        capabilities: capabilities
                    ))
            }

            XCTAssertTrue(
                model.ingestObservation(
                    observation(
                        atMs: 1,
                        positionMilliseconds: 10,
                        durationMilliseconds: 100
                    )))
            XCTAssertFalse(
                model.ingestObservation(
                    observation(
                        atMs: 2,
                        positionMilliseconds: 101,
                        durationMilliseconds: 100
                    )))
            XCTAssertNotNil(model.historySaveError)
            XCTAssertEqual(model.settingsNowPlaying?.state, .stale)
            XCTAssertFalse(model.settingsNowPlaying?.capabilities.pause ?? true)

            XCTAssertTrue(
                model.ingestObservation(
                    observation(
                        atMs: 3,
                        positionMilliseconds: 10,
                        durationMilliseconds: 100
                    )))
            XCTAssertNil(model.historySaveError)
            XCTAssertEqual(model.settingsNowPlaying?.state, .playing)
            XCTAssertTrue(model.settingsNowPlaying?.capabilities.pause ?? false)
        }
    #endif

    private func makeModel(
        state: MobileRideMapState? = nil,
        defaults: UserDefaults,
        providerSelectionStore: MusicProviderSelectionStore? = nil,
        historyPolicyStore: MusicHistoryPolicyStore? = nil,
        monitoringPreferenceStore: MusicMonitoringPreferenceStore? = nil,
        updateCapturePolicy: @escaping @MainActor (MobileMusicHistoryPolicyDto) -> Void = { _ in },
        selectedRideID: @escaping @MainActor () -> String? = { nil },
        invalidateHistory: @escaping @MainActor () -> Void = {},
        updateCaptureObservation: @escaping @MainActor (MobilePevcapMusicEventDto?) -> Void = { _ in },
        updateCaptureObservationAsync: (
            @MainActor (
                MobilePevcapMusicEventDto?, MobileMusicCaptureTarget
            ) async -> MobileCaptureWriteOutcomeDto
        )? = nil,
        captureTarget: @escaping @MainActor () -> MobileMusicCaptureTarget = { .unavailable },
        clearSelectedHistoryMusic: @escaping @MainActor () -> Void = {},
        setRideHistoryError: @escaping @MainActor (MobileRideMapError) -> Void = { _ in },
        appleMonitor: (any AppleMusicMonitorDriving)? = nil,
        monitorPollWaiter: MusicMonitorPollWaiter? = nil,
        providerCommandHandler: MusicProviderCommandHandler? = nil,
        spotifyCallbackHandler: (@MainActor (URL) -> Bool)? = nil,
        historySnapshotReader: MusicHistorySnapshotReader? = nil,
        correlationRideIDReader: (@MainActor () async -> String?)? = nil
    ) -> MusicFeatureModel {
        MusicFeatureModel(
            providerSelectionStore: providerSelectionStore ?? MusicProviderSelectionStore(defaults: defaults),
            historyPolicyStore: historyPolicyStore ?? MusicHistoryPolicyStore(defaults: defaults),
            monitoringPreferenceStore: monitoringPreferenceStore ?? MusicMonitoringPreferenceStore(defaults: defaults),
            rideMapState: state,
            monotonicNow: { 1_000 },
            updateCapturePolicy: updateCapturePolicy,
            updateCaptureObservation: updateCaptureObservation,
            updateCaptureObservationAsync: updateCaptureObservationAsync,
            captureTarget: captureTarget,
            invalidateHistoryForDeletion: invalidateHistory,
            selectedHistoryRideID: selectedRideID,
            clearSelectedHistoryMusic: clearSelectedHistoryMusic,
            setRideHistoryError: setRideHistoryError,
            appleMonitor: appleMonitor,
            monitorPollWaiter: monitorPollWaiter,
            providerCommandHandler: providerCommandHandler,
            spotifyCallbackHandler: spotifyCallbackHandler,
            historySnapshotReader: historySnapshotReader,
            correlationRideIDReader: correlationRideIDReader
        )
    }

    private func waitUntil(
        _ description: String,
        maxTurns: Int = 10_000,
        file: StaticString = #filePath,
        line: UInt = #line,
        condition: @escaping @MainActor () -> Bool
    ) async {
        for _ in 0..<maxTurns {
            if condition() { return }
            await Task.yield()
        }
        XCTFail("timed out waiting for \(description)", file: file, line: line)
    }

    private func makeDefaults() throws -> (name: String, defaults: UserDefaults) {
        let name = "MusicFeatureModelTests-\(UUID().uuidString)"
        return (name, try XCTUnwrap(UserDefaults(suiteName: name)))
    }

    private func observation(
        atMs: UInt64,
        identifier: String = "track-1",
        state: MobileMusicPlaybackStateDto = .playing
    ) -> MusicProviderObservation {
        MusicProviderObservation(
            snapshot: MobileMusicSnapshotDto(
                provider: .appleMusic,
                sessionId: "feature-test",
                state: state,
                item: MobileMusicItemDto(identifier: identifier, title: "Track", artist: "Artist"),
                positionMilliseconds: nil,
                durationMilliseconds: nil,
                observedAtMs: atMs,
                capabilities: MobileMusicCapabilitiesDto(
                    previous: false,
                    play: true,
                    pause: true,
                    next: false,
                    openProvider: true
                )
            ))
    }
}

private struct HistoricalRideQuery: RideHistoryQuerying {
    private let summary: MobileRideMapHistorySummaryDto

    init(rideID: String) {
        summary = MobileRideMapHistorySummaryDto(
            rideID: rideID,
            state: .saved,
            summary: MobileRideMapSummaryDto(
                pointCount: 0,
                distanceMeters: 0,
                durationMilliseconds: 0
            ),
            segmentCount: 0,
            createdAtMilliseconds: 0,
            candidateVehicle: nil,
            associatedVehicle: nil,
            associatedVehicleName: nil,
            telemetryState: .associatedNoTelemetry
        )
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        _ = (rideID, budget, viewport, privacy, cancellation)
        return MobileRideMapRouteProjection(
            points: [],
            segments: [],
            sourcePointCount: 0,
            sourceSegmentCount: 0,
            candidatePointCount: 0,
            candidateSegmentCount: 0,
            displayedSegmentCount: 0,
            backgroundGapCount: 0,
            presence: .emptyRide
        )
    }

    func storedMusicHistoryAsync(rideID: String) async throws -> MobileMusicHistoryDto {
        _ = rideID
        return MobileMusicHistoryDto(status: .unavailable, events: [])
    }

    func storedHistoryVehicleOptions() throws -> [MobileRideMapHistoryVehicleOptionDto] {
        []
    }

    func storedHistoryRide(rideID: String) throws -> MobileRideMapHistorySummaryDto? {
        summary.rideID == rideID ? summary : nil
    }

    func storedHistoryPage(
        cursor: MobileRideCursorDto?,
        limit: UInt32,
        filter: MobileRideHistoryFilterDto?
    ) throws -> MobileRideMapHistoryPageDto {
        _ = (cursor, limit, filter)
        return MobileRideMapHistoryPageDto(summaries: [summary], nextCursor: nil)
    }
}

#if canImport(MediaPlayer) && os(iOS)
    @MainActor
    final class TestAppleMusicMonitor: AppleMusicMonitorDriving {
        private(set) var authorizationPrompts = [Bool]()
        private(set) var startCount = 0
        private(set) var stopCount = 0
        private(set) var suspensionCount = 0
        private var onObservation: (@MainActor (MusicProviderObservation) -> Void)?
        private var onNextStopObservation: MusicProviderObservation?

        func requestAuthorization(allowPrompt: Bool) async -> Bool {
            authorizationPrompts.append(allowPrompt)
            return true
        }

        func unauthorizedSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
            MobileMusicSnapshotDto(
                provider: .appleMusic,
                sessionId: "test-monitor",
                state: .unauthorized,
                item: nil,
                positionMilliseconds: nil,
                durationMilliseconds: nil,
                observedAtMs: observedAtMs,
                capabilities: .init(previous: false, play: false, pause: false, next: false, openProvider: true)
            )
        }

        func startMonitoring(
            observedAtMs: @escaping @MainActor () -> UInt64,
            onObservation: @escaping @MainActor (MusicProviderObservation) -> Void
        ) async {
            startCount += 1
            self.onObservation = onObservation
            onObservation(
                MusicProviderObservation(
                    snapshot: MobileMusicSnapshotDto(
                        provider: .appleMusic,
                        sessionId: "test-monitor",
                        state: .playing,
                        item: .init(identifier: "monitor-track", title: "Track", artist: "Artist"),
                        positionMilliseconds: nil,
                        durationMilliseconds: nil,
                        observedAtMs: observedAtMs(),
                        capabilities: .init(previous: false, play: true, pause: true, next: false, openProvider: true)
                    )))
        }

        func emit(_ observation: MusicProviderObservation) {
            onObservation?(observation)
        }

        func emitOnNextStop(_ observation: MusicProviderObservation) {
            onNextStopObservation = observation
        }

        func stopMonitoring() {
            stopCount += 1
            if let onNextStopObservation {
                self.onNextStopObservation = nil
                onObservation?(onNextStopObservation)
            }
        }

        func applySuspension(_ suspension: MobileMusicProviderSuspension) {
            _ = suspension
            suspensionCount += 1
        }

        func refreshObservation(observedAtMs: UInt64) {
            _ = observedAtMs
        }
    }

    @MainActor
    private final class MusicMonitorSessionDriver: CutoutSessionDriving {
        let rideSessionStateHandle = CutoutSessionStateHandle()
        var onDisplayStateChange: ((RideDisplayState) -> Void)?
        var onPhaseChange: ((SessionConnectionPresentation) -> Void)?
        var onReconnectScheduled: ((SessionConnectionRetry) -> Void)?
        var onCaptureEvent: ((CaptureEvent) -> Void)?
        var onScanStateChange: ((DevicePickerScanState) -> Void)?
        var onSettingsChange: ((DeviceSettings) -> Void)?
        var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
        var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
        var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)?
        var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)?
        var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)?
        var onRideMapErrorChange: ((MobileRideMapErrorEvent) -> Void)?
        var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)?
        var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
        var onBluetoothRestorationResolved: ((String?) -> Void)?
        var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { nil }
        var electricUnicycleModel: ElectricUnicycleModel? { nil }
        var settings: DeviceSettings { rideSessionStateHandle.settings() }

        func start() {}
        func pair(platformIdentifier: String) -> Bool { false }
        func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool { false }
        func probe(platformIdentifier: String) -> Bool { false }
        func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool { false }
        func changeCaptureLabel(
            generation: CaptureGeneration,
            action: MobileCaptureLabelActionDto
        ) throws -> [MobileCaptureLabelDto] { [] }
        func annotateCapture(key: String, value: String) -> Bool { false }
        func updateMusicCapturePolicy(_ policy: MobileMusicHistoryPolicyDto) {}
        func updateMusicCaptureObservation(_ observation: MobilePevcapMusicEventDto?) {}
        func updateMusicCaptureObservationAsync(
            _ observation: MobilePevcapMusicEventDto?, target: MobileMusicCaptureTarget
        ) async -> MobileCaptureWriteOutcomeDto { .accepted }
        func flushCapture() async -> Bool { true }
        func finishCapture() async -> Bool { true }
        func disconnectAndScan() {}
        func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue) throws {
        }
        func submitDeviceAction(token: ConnectionAttemptToken, id: DeviceActionID) throws {}
        func now() -> MonotonicMilliseconds { MonotonicMilliseconds(0) }
        func startRideMapGpsOnly(atMs: UInt64, musicHistoryPolicy: MobileMusicHistoryPolicyDto) async throws
            -> MobileRideMapSnapshotDto
        {
            throw MobileRideMapError.storageError("ride map unavailable")
        }
        func pauseRideMap(expected _: MobileRideMapCommandTokenDto, atMs _: UInt64) async throws
            -> MobileRideMapSnapshotDto
        {
            throw MobileRideMapError.storageError("ride map unavailable")
        }
        func resumeRideMap(expected _: MobileRideMapCommandTokenDto, atMs _: UInt64) async throws
            -> MobileRideMapSnapshotDto
        {
            throw MobileRideMapError.storageError("ride map unavailable")
        }
        func stopRideMap(expected _: MobileRideMapCommandTokenDto, atMs _: UInt64) async throws
            -> MobileRideMapSnapshotDto
        {
            throw MobileRideMapError.storageError("ride map unavailable")
        }
        func saveRideMap(expected _: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto {
            throw MobileRideMapError.storageError("ride map unavailable")
        }
        func discardRideMap(expected _: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto {
            throw MobileRideMapError.storageError("ride map unavailable")
        }
    }

    @MainActor
    private final class TestMusicMonitorPollWaiter {
        private var continuations = [CheckedContinuation<Bool, Never>]()
        private(set) var startedCount = 0
        private(set) var completedCount = 0

        func wait(until _: UInt64) async -> Bool {
            startedCount += 1
            let result = await withCheckedContinuation { continuations.append($0) }
            completedCount += 1
            return result
        }

        func releaseNext(returning result: Bool) {
            guard !continuations.isEmpty else { return }
            continuations.removeFirst().resume(returning: result)
        }
    }
#endif
