import CutoutMobileFFI
import Synchronization
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

final class LiveRideModelTests: XCTestCase {
    @MainActor
    func testNewRideClearsThePreviousRideProjection() async throws {
        let state = MobileRideMapState()
        let connectionState = CutoutSessionStateHandle()
        let model = LiveRideModel(
            state: state,
            storageError: nil,
            availability: .ready,
            now: { 200 }
        )
        model.setSceneActive(true)
        model.setMapVisible(true)
        _ = try state.startGpsOnly(atMs: 100)

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
            return token
        }

        let firstAttempt = try verify("pev-1", atMs: 200)
        let firstAdmission = try state.beginVerifiedConnectionAdmission(
            connectionState: connectionState,
            token: firstAttempt,
            atMs: 200
        )
        let firstResult = try await Self.settleAdmission(state, firstAdmission)
        let first = try XCTUnwrap(firstResult)
        model.applySnapshot(first)

        let decision = try state.ingestLocation(
            monotonicMs: 200,
            wallClockUnixMs: 1_700_000_000_200,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        )
        var settledDecision = decision
        if case .pending = decision {
            let deadline = ContinuousClock.now + .seconds(10)
            while ContinuousClock.now < deadline, !Task.isCancelled {
                if let terminal = state.pollLocationWrites().first {
                    settledDecision = terminal
                    break
                }
                try await Task.sleep(for: .milliseconds(1))
            }
        }
        guard case .accepted = settledDecision else {
            return XCTFail("expected the first vehicle's route point to be accepted")
        }
        model.applyDecision(snapshot: first, decision: settledDecision)

        let projectionDeadline = ContinuousClock.now + .seconds(10)
        while model.displayPoints.isEmpty && ContinuousClock.now < projectionDeadline {
            await Task.yield()
        }
        XCTAssertFalse(model.displayPoints.isEmpty)
        XCTAssertNotNil(model.cameraRegion)
        XCTAssertNotNil(model.lastDecision)

        _ = try state.stop(atMs: 300)
        let second = try state.startGpsOnly(atMs: 400)
        XCTAssertNotEqual(first.rideID, second.rideID)
        model.applySnapshot(second)

        XCTAssertEqual(model.snapshot?.rideID, second.rideID)
        XCTAssertTrue(model.displayPoints.isEmpty)
        XCTAssertNil(model.cameraRegion)
        XCTAssertNil(model.lastDecision)
    }

    private static func settleAdmission(
        _ state: MobileRideMapState,
        _ admission: MobileRideMapConnectionAdmission
    ) async throws -> MobileRideMapSnapshotDto? {
        let deadline = ContinuousClock.now + .seconds(10)
        while ContinuousClock.now < deadline, !Task.isCancelled {
            switch try state.pollVerifiedConnectionAdmission(admission) {
            case .pending:
                try await Task.sleep(for: .milliseconds(1))
            case let .completed(snapshot):
                return snapshot
            }
        }
        XCTFail("timed out waiting for verified ride-map admission")
        return nil
    }

    @MainActor
    func testHeldRestoreCannotReplaceNewRecordingAndCancelsRustQuery() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        _ = try state.stop(atMs: 2_000)
        let saved = try state.save()
        let restoreStarted = expectation(description: "stored projection entered")
        let restoreFinished = expectation(description: "stored projection released")
        let query = HeldRestoreRideQuery(
            state: state,
            snapshot: saved,
            restoreStarted: restoreStarted,
            restoreFinished: restoreFinished
        )
        defer { query.releaseRestore.signal() }
        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { 3_000 }
        )
        model.setSceneActive(true)
        model.setMapVisible(true)

        let restoration = model.restore()
        await fulfillment(of: [restoreStarted], timeout: 3)
        XCTAssertEqual(model.snapshot?.rideID, saved.rideID)
        // Start the replacement only after the saved projection is held. Foreground
        // catchup must observe the same saved state until this command boundary.
        let newRecording = try state.startGpsOnly(atMs: 3_000)
        XCTAssertNotEqual(newRecording.rideID, saved.rideID)
        model.applyCommandSnapshot(newRecording, resetPoints: true)
        let clearedVersion = model.projectionVersion

        query.releaseRestore.signal()
        await fulfillment(of: [restoreFinished], timeout: 3)
        await restoration?.value
        XCTAssertTrue(query.tokensCancelled.withLock { $0.live })
        XCTAssertTrue(query.tokensCancelled.withLock { $0.durable })
        XCTAssertEqual(model.snapshot?.rideID, newRecording.rideID)
        XCTAssertEqual(model.projectionVersion, clearedVersion)
        XCTAssertTrue(model.displayPoints.isEmpty)
    }

    @MainActor
    func testRestoreDoesNotRetainLiveRideWhileItsProjectionIsBlocked() async throws {
        let state = MobileRideMapState()
        let started = try state.startGpsOnly(atMs: 1_000)
        let firstStarted = expectation(description: "restored live projection entered")
        let laterProjection = expectation(description: "released presentation must not start another projection")
        laterProjection.isInverted = true
        let query = HeldLiveRideQuery(
            state: state,
            firstStarted: firstStarted,
            secondFinished: laterProjection,
            ignoreTimedSnapshots: true
        )
        defer { query.releaseFirst.signal() }
        var model: LiveRideModel? = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { 1_000 }
        )
        weak let releasedModel = model
        model?.setSceneActive(true)
        model?.setMapVisible(true)
        let restoration = model?.restore()

        await fulfillment(of: [firstStarted], timeout: 3)
        XCTAssertEqual(model?.snapshot?.rideID, started.rideID)
        XCTAssertEqual(query.stats.withLock { $0.active }, 1)
        model = nil
        XCTAssertNil(releasedModel, "Restoration must not retain live presentation while a route query waits")

        query.releaseFirst.signal()
        await restoration?.value
        await fulfillment(of: [laterProjection], timeout: 0.05)
        XCTAssertTrue(query.stats.withLock { $0.liveTokenCancelled })
        XCTAssertTrue(query.stats.withLock { $0.durableTokenCancelled })
        _ = try state.stop(atMs: 2_000)
        _ = try state.discard()
    }

    @MainActor
    func testHeldProjectionCoalescesAcceptedOutcomesAndRejectsStaleResult() async throws {
        let state = MobileRideMapState()
        let started = try state.startGpsOnly(atMs: 1_000)
        let firstStarted = expectation(description: "first projection entered")
        let secondFinished = expectation(description: "coalesced projection finished")
        let query = HeldLiveRideQuery(
            state: state,
            firstStarted: firstStarted,
            secondFinished: secondFinished
        )
        defer { query.releaseFirst.signal() }
        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { 1_000 }
        )
        model.setSceneActive(true)
        model.setMapVisible(true)
        model.applyCommandSnapshot(started, resetPoints: true)

        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 1)))
        await fulfillment(of: [firstStarted], timeout: 3)
        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 2)))
        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 3)))
        XCTAssertEqual(query.stats.withLock { $0.calls }, 1)

        query.releaseFirst.signal()
        await fulfillment(of: [secondFinished], timeout: 3)
        let deadline = ContinuousClock.now.advanced(by: .seconds(2))
        while model.displayPoints.first?.sequence != 2 && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(model.displayPoints.map(\.sequence), [2])
        XCTAssertEqual(query.stats.withLock { $0.calls }, 2)
        XCTAssertEqual(query.stats.withLock { $0.maxConcurrent }, 1)
        XCTAssertEqual(
            query.stats.withLock { $0.budgets },
            [MobileRideMapLimits.rustOwned.liveTailPointLimit, MobileRideMapLimits.rustOwned.liveTailPointLimit]
        )
        XCTAssertTrue(query.stats.withLock { $0.liveTokenCancelled })
        XCTAssertTrue(query.stats.withLock { $0.durableTokenCancelled })
    }

    @MainActor
    func testProjectionWaitsForMapVisibilityAndForegroundWhileDecisionsContinue() async throws {
        let state = MobileRideMapState()
        let started = try state.startGpsOnly(atMs: 1_000)
        let query = CountingLiveRideQuery(state: state)
        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { 1_000 }
        )
        model.applyCommandSnapshot(started, resetPoints: true)

        for sequence in 1...3 {
            model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: UInt64(sequence))))
        }
        XCTAssertEqual(query.calls.withLock { $0 }, 0)
        XCTAssertEqual(model.snapshot?.rideID, started.rideID)
        guard case .accepted(let point)? = model.lastDecision else {
            return XCTFail("latest accepted decision should remain available while projection is disabled")
        }
        XCTAssertEqual(point.sequence, 3)

        model.setSceneActive(true)
        XCTAssertEqual(query.calls.withLock { $0 }, 0, "foreground alone must not project a hidden map")
        model.setMapVisible(true)
        await Self.waitUntilProjectionCount(1, query: query)
        XCTAssertEqual(query.calls.withLock { $0 }, 1)
    }

    @MainActor
    func testDurableRouteProgressReprojectsWhenAcceptedDecisionWasReplacedByIgnored() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        let query = CountingLiveRideQuery(state: state)
        let model = LiveRideModel(state: query, storageError: nil, availability: .ready, now: { 2_000 })
        model.setSceneActive(true)
        model.setMapVisible(true)
        await model.restore()?.value
        // Foreground duration catchup can win the snapshot-generation fence and
        // start projection independently. A retired restore read does not await
        // that worker; settle its published projection before advancing the route.
        let initialDeadline = ContinuousClock.now + .seconds(3)
        while model.projectionVersion == 0 && ContinuousClock.now < initialDeadline {
            try await Task.sleep(for: .milliseconds(1))
        }
        XCTAssertGreaterThan(model.projectionVersion, 0)
        XCTAssertEqual(model.snapshot?.rideID, state.currentSnapshot()?.rideID)
        XCTAssertTrue(model.displayPoints.isEmpty)
        XCTAssertEqual(query.calls.withLock { $0 }, 1)

        _ = try state.ingestLocation(
            monotonicMs: 2_000, wallClockUnixMs: 1_700_000_002_000,
            latitudeDegrees: 40, longitudeDegrees: -105, horizontalAccuracyMeters: 3
        )
        _ = try await state.checkpoint()
        let ignored = try state.ingestLocation(
            monotonicMs: 2_000, wallClockUnixMs: 1_700_000_002_000,
            latitudeDegrees: 40, longitudeDegrees: -105, horizontalAccuracyMeters: 3
        )
        XCTAssertEqual(ignored, .ignored(reason: .duplicateLocation))
        let latest = try XCTUnwrap(state.currentSnapshot())
        XCTAssertEqual(latest.summary.pointCount, 1)
        model.applyDecision(snapshot: latest, decision: ignored)
        await Self.waitUntilProjectionCount(2, query: query)
        XCTAssertEqual(
            query.calls.withLock { $0 }, 2,
            "latest durable route progress must invalidate geometry even when Accepted was coalesced away")
        let firstDeadline = ContinuousClock.now + .seconds(3)
        while model.displayPoints.count != 1 && ContinuousClock.now < firstDeadline {
            try await Task.sleep(for: .milliseconds(1))
        }
        XCTAssertEqual(model.displayPoints.count, 1)

        model.setSceneActive(false)
        _ = try state.ingestLocation(
            monotonicMs: 3_000, wallClockUnixMs: 1_700_000_003_000,
            latitudeDegrees: 40.0001, longitudeDegrees: -105, horizontalAccuracyMeters: 3
        )
        _ = try await state.checkpoint()
        XCTAssertEqual(state.currentSnapshot()?.summary.pointCount, 2)
        XCTAssertEqual(query.calls.withLock { $0 }, 2, "route progress must not query a hidden scene")
        model.setSceneActive(true)
        await Self.waitUntilProjectionCount(3, query: query)
        let secondDeadline = ContinuousClock.now + .seconds(3)
        while model.displayPoints.count != 2 && ContinuousClock.now < secondDeadline {
            try await Task.sleep(for: .milliseconds(1))
        }
        XCTAssertEqual(query.calls.withLock { $0 }, 3)
        XCTAssertEqual(model.displayPoints.count, 2)
        model.setSceneActive(false)
    }

    @MainActor
    func testHiddenNewRideErrorUsesAuthoritativeRustIdentityAndRejectsRetiredGeneration() async throws {
        let state = MobileRideMapState()
        let first = try state.startGpsOnly(atMs: 1_000)
        let model = LiveRideModel(state: state, storageError: nil, availability: .ready, now: { 1_000 })
        await model.restore()?.value
        XCTAssertFalse(model.isSceneActive)
        _ = try state.stop(atMs: 2_000)
        let latest = try state.startGpsOnly(atMs: 3_000)
        XCTAssertNotEqual(first.rideID, latest.rideID)
        XCTAssertEqual(
            model.snapshot?.rideID, first.rideID, "hidden presentation intentionally retains its prior snapshot")

        await model.applyError(
            MobileRideMapErrorEvent(
                context: MobileRideMapErrorContext(snapshot: latest),
                error: .storageError("current recording failed")
            ))?.value
        XCTAssertEqual(model.error, .storageError("current recording failed"))
        await model.applyError(
            MobileRideMapErrorEvent(
                context: MobileRideMapErrorContext(snapshot: first),
                error: .storageError("retired recording failed")
            ))?.value
        XCTAssertEqual(
            model.error, .storageError("current recording failed"),
            "late errors must not replace the active Rust ride's failure")
        let retiredGeneration = try XCTUnwrap(first.recordingToken).generation
        XCTAssertNotEqual(retiredGeneration, latest.recordingToken?.generation)
        await model.applyError(
            MobileRideMapErrorEvent(
                context: MobileRideMapErrorContext(
                    recordingToken: MobileRideMapRecordingTokenDto(
                        rideId: latest.rideID, generation: retiredGeneration
                    )),
                error: .storageError("retired generation failed")
            ))?.value
        XCTAssertEqual(
            model.error, .storageError("current recording failed"),
            "a matching ride ID must not admit a retired recording generation")
        _ = try state.stop(atMs: 4_000)
        _ = try state.discard()
        XCTAssertNil(state.currentSnapshot())
        await model.applyError(
            MobileRideMapErrorEvent(
                context: MobileRideMapErrorContext(snapshot: latest),
                error: .storageError("discarded recording failed")
            ))?.value
        XCTAssertEqual(
            model.error, .storageError("current recording failed"),
            "authoritative idle must not fall back to retained ride presentation")
    }

    @MainActor
    func testEmptyRestoreProjectsLaterPausedRideWhenMapReturns() async throws {
        let state = MobileRideMapState()
        let query = CountingLiveRideQuery(state: state)
        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { 1_000 }
        )
        await model.restore()?.value
        XCTAssertNil(model.snapshot)

        _ = try state.startGpsOnly(atMs: 1_000)
        let paused = try state.pause(atMs: 2_000)
        model.applySnapshot(paused)
        XCTAssertEqual(query.calls.withLock { $0 }, 0)

        model.setSceneActive(true)
        model.setMapVisible(true)
        await Self.waitUntilProjectionCount(1, query: query)
        XCTAssertEqual(query.calls.withLock { $0 }, 1)
        XCTAssertEqual(model.snapshot?.state, .paused)
    }

    @MainActor
    func testBackgroundCancelsAndFencesProjectionThenProjectsLatestRouteOnResume() async throws {
        let state = MobileRideMapState()
        let started = try state.startGpsOnly(atMs: 1_000)
        let firstStarted = expectation(description: "projection entered before background")
        let secondFinished = expectation(description: "latest projection completed after foreground")
        let query = HeldLiveRideQuery(
            state: state,
            firstStarted: firstStarted,
            secondFinished: secondFinished
        )
        defer { query.releaseFirst.signal() }
        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { 1_000 }
        )
        model.setSceneActive(true)
        model.setMapVisible(true)
        model.applyCommandSnapshot(started, resetPoints: true)
        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 1)))
        await fulfillment(of: [firstStarted], timeout: 3)

        model.setSceneActive(false)
        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 2)))
        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 3)))
        XCTAssertEqual(query.stats.withLock { $0.calls }, 1)
        query.releaseFirst.signal()
        await Self.waitUntilProjectionIdle(query)

        XCTAssertTrue(query.stats.withLock { $0.liveTokenCancelled })
        XCTAssertTrue(query.stats.withLock { $0.durableTokenCancelled })
        XCTAssertTrue(model.displayPoints.isEmpty, "cancelled background result must not replace the route")
        XCTAssertEqual(query.stats.withLock { $0.calls }, 1)

        model.setSceneActive(true)
        await fulfillment(of: [secondFinished], timeout: 3)
        let deadline = ContinuousClock.now.advanced(by: .seconds(3))
        while model.displayPoints.first?.sequence != 2 && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(query.stats.withLock { $0.calls }, 2)
        XCTAssertEqual(model.displayPoints.map(\.sequence), [2])
        XCTAssertEqual(query.stats.withLock { $0.maxConcurrent }, 1)
    }

    private static func waitUntilProjectionCount(
        _ expected: Int,
        query: CountingLiveRideQuery
    ) async {
        let deadline = ContinuousClock.now.advanced(by: .seconds(3))
        while query.calls.withLock({ $0 }) < expected && ContinuousClock.now < deadline {
            await Task.yield()
        }
    }

    private static func waitUntilProjectionIdle(_ query: HeldLiveRideQuery) async {
        let deadline = ContinuousClock.now.advanced(by: .seconds(3))
        while query.stats.withLock({ $0.active }) != 0 && ContinuousClock.now < deadline {
            await Task.yield()
        }
    }

    @MainActor
    func testDurationTickerSleepsWhileBackgroundedAndCatchesUpWithoutOpeningMap() async throws {
        let state = MobileRideMapState()
        let started = try state.startGpsOnly(atMs: 1_000)
        let now = Mutex<UInt64>(3_000)
        let query = CountingLiveRideQuery(state: state)
        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { now.withLock { $0 } }
        )
        _ = model.restore()
        model.applyCommandSnapshot(started, resetPoints: true)

        try await Task.sleep(for: .milliseconds(1_100))
        XCTAssertEqual(query.durationCalls.withLock { $0 }, 0)
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 0)
        XCTAssertEqual(state.currentSnapshot(atMs: 3_000)?.state, .active)

        model.setSceneActive(true)
        await model.refreshDuration()?.value
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 2_000)
        XCTAssertFalse(model.isMapVisible)
        XCTAssertEqual(query.calls.withLock { $0 }, 0)

        model.setSceneActive(false)
        let callsAtBackground = query.durationCalls.withLock { $0 }
        now.withLock { $0 = 7_000 }
        let latest = try XCTUnwrap(state.currentSnapshot(atMs: 7_000))
        model.applySnapshot(latest)
        try await Task.sleep(for: .milliseconds(1_100))
        XCTAssertEqual(query.durationCalls.withLock { $0 }, callsAtBackground)
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 6_000)
        XCTAssertEqual(state.currentSnapshot(atMs: 7_000)?.state, .active)

        now.withLock { $0 = 9_000 }
        model.setSceneActive(true)
        await model.refreshDuration()?.value
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 8_000)
        XCTAssertEqual(query.calls.withLock { $0 }, 0)
        model.setSceneActive(false)
    }

    @MainActor
    func testDurationUsesRustSnapshotAcrossPauseResumeAndStop() async throws {
        let state = MobileRideMapState()
        let now = Mutex<UInt64>(1_000)
        let model = LiveRideModel(
            state: state,
            storageError: nil,
            availability: .ready,
            now: { now.withLock { $0 } }
        )
        model.setSceneActive(true)
        model.setMapVisible(true)

        let started = try state.startGpsOnly(atMs: now.withLock { $0 })
        model.applyCommandSnapshot(started, resetPoints: true)
        now.withLock { $0 = 4_000 }
        await model.refreshDuration()?.value
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 3_000)

        let paused = try state.pause(atMs: now.withLock { $0 })
        model.applyCommandSnapshot(paused, resetPoints: false)
        now.withLock { $0 = 9_000 }
        await model.refreshDuration()?.value
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 3_000)

        let resumed = try state.resume(atMs: 10_000)
        model.applyCommandSnapshot(resumed, resetPoints: false)
        now.withLock { $0 = 15_000 }
        await model.refreshDuration()?.value
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 8_000)

        let stopped = try state.stop(atMs: 16_000)
        model.applyCommandSnapshot(stopped, resetPoints: false)
        now.withLock { $0 = 20_000 }
        await model.refreshDuration()?.value
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 9_000)
    }

    private func point(sequence: UInt64) -> MobileRideMapPointDto {
        MobileRideMapPointDto(
            sequence: sequence,
            segmentId: 1,
            startReason: .initial,
            latitudeDegrees: 40,
            longitudeDegrees: -105,
            wallClockUnixMs: 1_000,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: 1,
            telemetryState: .gpsOnly
        )
    }
}

private final class CountingLiveRideQuery: LiveRideQuerying {
    let calls = Mutex(0)
    let durationCalls = Mutex(0)
    private let state: MobileRideMapState

    init(state: MobileRideMapState) {
        self.state = state
    }

    func currentSnapshot() -> MobileRideMapSnapshotDto? { state.currentSnapshot() }

    func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        durationCalls.withLock { $0 += 1 }
        return state.currentSnapshot(atMs: atMs)
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        try state.projectStoredPoints(
            rideID: rideID,
            budget: budget,
            viewport: viewport,
            privacy: privacy,
            cancellation: cancellation
        )
    }

    func projectCurrentRoutePoints(
        budget: UInt32,
        rideID: String?,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        durableCancellation: MobileRideMapProjectionCancellation?,
        liveCancellation: MobileLiveRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        calls.withLock { $0 += 1 }
        return try state.projectCurrentRoutePoints(
            budget: budget,
            rideID: rideID,
            viewport: viewport,
            privacy: privacy,
            durableCancellation: durableCancellation,
            liveCancellation: liveCancellation
        )
    }
}

private final class HeldLiveRideQuery: LiveRideQuerying {
    struct Stats: Sendable {
        var calls = 0
        var active = 0
        var maxConcurrent = 0
        var budgets = [UInt32]()
        var liveTokenCancelled = false
        var durableTokenCancelled = false
    }

    let stats = Mutex(Stats())
    let releaseFirst = DispatchSemaphore(value: 0)
    private let state: MobileRideMapState
    private let firstStarted: XCTestExpectation
    private let secondFinished: XCTestExpectation
    private let ignoreTimedSnapshots: Bool

    init(
        state: MobileRideMapState,
        firstStarted: XCTestExpectation,
        secondFinished: XCTestExpectation,
        ignoreTimedSnapshots: Bool = false
    ) {
        self.state = state
        self.firstStarted = firstStarted
        self.secondFinished = secondFinished
        self.ignoreTimedSnapshots = ignoreTimedSnapshots
    }

    func currentSnapshot() -> MobileRideMapSnapshotDto? { state.currentSnapshot() }

    func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        guard !ignoreTimedSnapshots else { return nil }
        return state.currentSnapshot(atMs: atMs)
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        try state.projectStoredPoints(
            rideID: rideID,
            budget: budget,
            viewport: viewport,
            privacy: privacy,
            cancellation: cancellation
        )
    }

    func projectCurrentRoutePoints(
        budget: UInt32,
        rideID: String?,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        durableCancellation: MobileRideMapProjectionCancellation?,
        liveCancellation: MobileLiveRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        let call = stats.withLock { stats in
            stats.calls += 1
            stats.active += 1
            stats.maxConcurrent = max(stats.maxConcurrent, stats.active)
            stats.budgets.append(budget)
            return stats.calls
        }
        defer { stats.withLock { $0.active -= 1 } }
        if call == 1 {
            firstStarted.fulfill()
            releaseFirst.wait()
            let liveCancelled = cancellationObserved {
                try state.projectPoints(
                    budget: 1,
                    cancellation: liveCancellation
                )
            }
            let durableCancelled = cancellationObserved {
                try state.projectStoredPoints(
                    rideID: rideID ?? "",
                    budget: 1,
                    cancellation: durableCancellation
                )
            }
            stats.withLock {
                $0.liveTokenCancelled = liveCancelled
                $0.durableTokenCancelled = durableCancelled
            }
        } else {
            secondFinished.fulfill()
        }
        let sequence = UInt64(call)
        return MobileRideMapRouteProjection(
            points: [
                MobileRideMapRouteDisplayPoint(
                    sequence: sequence,
                    segmentId: 1,
                    latitudeDegrees: 40,
                    longitudeDegrees: -105,
                    privacyClass: .precise
                )
            ],
            segments: [],
            sourcePointCount: sequence,
            sourceSegmentCount: 1,
            candidatePointCount: 1,
            candidateSegmentCount: 0,
            displayedSegmentCount: 0,
            backgroundGapCount: 0,
            presence: .visible
        )
    }
}

private final class HeldRestoreRideQuery: LiveRideQuerying {
    let releaseRestore = DispatchSemaphore(value: 0)
    let tokensCancelled = Mutex((live: false, durable: false))
    private let state: MobileRideMapState
    private let snapshot: MobileRideMapSnapshotDto
    private let restoreStarted: XCTestExpectation
    private let restoreFinished: XCTestExpectation

    init(
        state: MobileRideMapState,
        snapshot: MobileRideMapSnapshotDto,
        restoreStarted: XCTestExpectation,
        restoreFinished: XCTestExpectation
    ) {
        self.state = state
        self.snapshot = snapshot
        self.restoreStarted = restoreStarted
        self.restoreFinished = restoreFinished
    }

    func currentSnapshot() -> MobileRideMapSnapshotDto? { snapshot }

    func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        state.currentSnapshot(atMs: atMs)
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        restoreStarted.fulfill()
        releaseRestore.wait()
        let cancelled = cancellationObserved {
            try state.projectStoredPoints(
                rideID: rideID,
                budget: budget,
                viewport: viewport,
                privacy: privacy,
                cancellation: cancellation
            )
        }
        tokensCancelled.withLock { $0 = (live: false, durable: cancelled) }
        restoreFinished.fulfill()
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

    func projectCurrentRoutePoints(
        budget: UInt32,
        rideID: String?,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        durableCancellation: MobileRideMapProjectionCancellation?,
        liveCancellation: MobileLiveRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        restoreStarted.fulfill()
        releaseRestore.wait()
        let liveCancelled = cancellationObserved {
            try state.projectPoints(budget: 1, cancellation: liveCancellation)
        }
        let durableCancelled = cancellationObserved {
            try state.projectStoredPoints(
                rideID: rideID ?? "",
                budget: 1,
                cancellation: durableCancellation
            )
        }
        tokensCancelled.withLock {
            $0 = (live: liveCancelled, durable: durableCancelled)
        }
        restoreFinished.fulfill()
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
}

private func cancellationObserved(
    _ operation: () throws -> MobileRideMapRouteProjection
) -> Bool {
    do {
        _ = try operation()
        return false
    } catch {
        return error as? MobileRideMapError == .cancelled
    }
}
