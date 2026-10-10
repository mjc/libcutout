import CutoutMobileFFI
import Synchronization
import XCTest

@testable import CutoutMobile

final class LiveActivityRideLifecycleCoordinatorTests: XCTestCase {
    override func setUp() {
        super.setUp()
        XCTAssertNoThrow(try RideSessionMarkerStore().clear())
    }

    override func tearDown() {
        XCTAssertNoThrow(try RideSessionMarkerStore().clear())
        super.tearDown()
    }

    func testCoordinatorPersistsOnlyTheOpaqueRustMarkerUntilTheRideEnds() async throws {
        let suiteName = "LiveActivityRideLifecycleCoordinatorTests.marker.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            markerStore: markerStore
        )
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: snapshot,
            shouldBeActive: true
        )

        XCTAssertNotNil(markerStore.marker)

        await coordinator.end(requestID: 2, reason: .disconnected)

        XCTAssertNil(markerStore.marker)
    }

    func testPersistedRideRecoveryAdoptsTheSameRustIdentity() async throws {
        let suiteName = "LiveActivityRideLifecycleCoordinatorTests.restore.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let source = CutoutSessionStateHandle()
        let started = try source.reduceRideSession(
            input: .start(platformIdentifier: "vesc-platform-id")
        )
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let manager = RecordingLiveActivityRideLifecycleManager()
        let restoredState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: restoredState,
            markerStore: markerStore
        )
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        let recovered = await coordinator.recoverPersistedRide(
            requestID: 1,
            restoredPlatformIdentifier: "vesc-platform-id",
            snapshot: snapshot
        )

        XCTAssertEqual(recovered, .adopted)
        XCTAssertEqual(restoredState.rideSessionSnapshot().identity, started.snapshot.identity)
        XCTAssertEqual(restoredState.rideSessionSnapshot().phase, .active)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot)])
        XCTAssertNotNil(markerStore.marker)
    }

    func testPersistedRideRecoveryPreservesRideWithoutARestoredPeripheral() async throws {
        let suiteName = "LiveActivityRideLifecycleCoordinatorTests.orphan.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let source = CutoutSessionStateHandle()
        _ = try source.reduceRideSession(input: .start(platformIdentifier: "vesc-platform-id"))
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let manager = RecordingLiveActivityRideLifecycleManager()
        let restoredState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: restoredState,
            markerStore: markerStore
        )

        let recovered = await coordinator.recoverPersistedRide(
            requestID: 1,
            restoredPlatformIdentifier: nil,
            snapshot: nil
        )

        XCTAssertEqual(recovered, .reconnecting)
        XCTAssertEqual(restoredState.rideSessionSnapshot().phase, .reconnecting)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [])
        XCTAssertNotNil(markerStore.marker)
    }

    func testMissingPresentationAfterRecoveryPreservesRideUntilExplicitEnd() async throws {
        let restoredIdentifiers: [String?] = [nil, "different-wheel"]
        for restoredIdentifier in restoredIdentifiers {
            let suiteName = "LiveActivityRideLifecycleCoordinatorTests.noPresentation.\(UUID().uuidString)"
            let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
            defer { defaults.removePersistentDomain(forName: suiteName) }
            let markerStore = RideSessionMarkerStore(defaults: defaults)
            let source = CutoutSessionStateHandle()
            let started = try source.reduceRideSession(input: .start(platformIdentifier: "previous-wheel"))
            markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
            let restoredState = CutoutSessionStateHandle()
            let manager = RecordingLiveActivityRideLifecycleManager()
            let coordinator = LiveActivityRideLifecycleCoordinator(
                manager: manager,
                sessionState: restoredState,
                markerStore: markerStore
            )

            let recovered = await coordinator.recoverPersistedRide(
                requestID: 1,
                restoredPlatformIdentifier: restoredIdentifier,
                snapshot: nil
            )
            XCTAssertEqual(recovered, .reconnecting)
            await coordinator.reconcile(requestID: 2, snapshot: nil, shouldBeActive: false)

            let events = await manager.recordedEvents()
            XCTAssertEqual(events, [])
            XCTAssertEqual(restoredState.rideSessionSnapshot().identity, started.snapshot.identity)
            XCTAssertEqual(restoredState.rideSessionSnapshot().phase, .reconnecting)
            XCTAssertNotNil(markerStore.marker)

            await coordinator.end(requestID: 3, reason: .disconnected)
            let endedEvents = await manager.recordedEvents()
            XCTAssertEqual(endedEvents, [.end(.disconnected)])
            XCTAssertEqual(restoredState.rideSessionSnapshot().phase, .ended(reason: .userDisconnect))
            XCTAssertNil(markerStore.marker)
            await coordinator.end(requestID: 4, reason: .disconnected)
            let repeatedEndEvents = await manager.recordedEvents()
            XCTAssertEqual(repeatedEndEvents, endedEvents)
        }
    }

    func testCoordinatorPublishesActivityKitStartIntoSharedRustLifecycle() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let sessionState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: sessionState
        )
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: snapshot,
            shouldBeActive: true
        )

        let rustSnapshot = sessionState.rideSessionSnapshot()
        XCTAssertEqual(rustSnapshot.identity?.platformIdentifier, "vesc-platform-id")
        XCTAssertEqual(rustSnapshot.phase, .active)
        XCTAssertEqual(rustSnapshot.activity, .active(activityId: "activity-1"))
        let activityIdentity = await manager.startedRideSessionIdentity()
        XCTAssertEqual(activityIdentity?.platformIdentifier, rustSnapshot.identity?.platformIdentifier)
        XCTAssertEqual(activityIdentity?.sessionID, rustSnapshot.identity?.sessionId)
        let freshnessWindows = await manager.recordedFreshnessWindows()
        XCTAssertEqual(freshnessWindows, [2_000])
    }

    func testTransientDisconnectKeepsRustIdentityAndResumesWithoutDuplicateStart() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let sessionState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: sessionState
        )
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let resumed = liveSnapshot(label: "Connected ride", speedMph: 21.6)

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: first,
            shouldBeActive: true
        )
        let identity = sessionState.rideSessionSnapshot().identity
        await coordinator.transportDisconnected(requestID: 2, atMs: 200, snapshot: first)

        XCTAssertEqual(sessionState.rideSessionSnapshot().phase, .reconnecting)
        XCTAssertEqual(sessionState.rideSessionSnapshot().identity, identity)

        await coordinator.reconcile(
            requestID: 3,
            platformIdentifier: "vesc-platform-id",
            monotonicTimeMs: 300,
            snapshot: resumed,
            shouldBeActive: true
        )

        XCTAssertEqual(sessionState.rideSessionSnapshot().phase, .active)
        let events = await manager.recordedEvents()
        let freshnessWindows = await manager.recordedFreshnessWindows()
        XCTAssertEqual(events, [.start(first), .update(first), .update(resumed)])
        XCTAssertEqual(freshnessWindows, [2_000, 0, 2_000])
    }

    func testReconnectResumesWhenTheTelemetrySnapshotIsUnchanged() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let sessionState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: sessionState
        )
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: snapshot,
            shouldBeActive: true
        )
        await coordinator.transportDisconnected(requestID: 2, atMs: 200, snapshot: snapshot)

        await coordinator.reconcile(
            requestID: 3,
            platformIdentifier: "vesc-platform-id",
            monotonicTimeMs: 300,
            snapshot: snapshot,
            shouldBeActive: true
        )

        XCTAssertEqual(sessionState.rideSessionSnapshot().phase, .active)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .update(snapshot)])
    }

    func testBackgroundTransitionExecutesRustRequestedCaptureFlushOnce() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let sessionState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: sessionState
        )
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let capture = CaptureFlushSpy()

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: snapshot,
            shouldBeActive: true
        )
        await coordinator.appDidEnterBackground(
            requestID: 2,
            atMs: 200,
            snapshot: snapshot,
            captureFlush: { await capture.flush() }
        )
        await coordinator.appDidEnterBackground(
            requestID: 3,
            atMs: 300,
            snapshot: snapshot,
            captureFlush: { await capture.flush() }
        )

        let firstBackgroundFlushCount = await capture.count()
        XCTAssertEqual(sessionState.rideSessionSnapshot().appPresence, .background)
        XCTAssertEqual(firstBackgroundFlushCount, 1)

        await coordinator.appDidBecomeActive(requestID: 4, snapshot: snapshot)
        await coordinator.appDidEnterBackground(
            requestID: 5,
            atMs: 500,
            snapshot: snapshot,
            captureFlush: { await capture.flush() }
        )

        let secondBackgroundFlushCount = await capture.count()
        XCTAssertEqual(sessionState.rideSessionSnapshot().appPresence, .background)
        XCTAssertEqual(secondBackgroundFlushCount, 2)
    }

    func testBackgroundTransitionPublishesTheLatestTelemetrySnapshot() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let unavailable = LiveActivityRideSnapshot(
            identity: .device("Connected ride"),
            rideState: EucRideScreenState(phase: .live, displayState: RideDisplayState())
        )
        let available = liveSnapshot(label: "Connected ride", speedMph: 17.9)

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: unavailable,
            shouldBeActive: true
        )
        await coordinator.appDidEnterBackground(
            requestID: 2,
            atMs: 200,
            snapshot: available,
            captureFlush: { true }
        )

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(unavailable), .update(available)])
    }

    func testReconnectExhaustionEndsOnceWithTheTypedRustReason() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let sessionState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: sessionState
        )
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: snapshot,
            shouldBeActive: true
        )
        await coordinator.transportDisconnected(requestID: 2, atMs: 200, snapshot: snapshot)
        await coordinator.reconnectExhausted(requestID: 3, snapshot: snapshot)
        await coordinator.reconnectExhausted(requestID: 4, snapshot: snapshot)

        XCTAssertEqual(
            sessionState.rideSessionSnapshot().phase,
            .ended(reason: .reconnectExhausted)
        )
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .update(snapshot), .end(.unavailable)])
    }

    func testUnrecoverableSessionFailureEndsOnceWithTheTypedRustReason() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let sessionState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            sessionState: sessionState
        )
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(
            requestID: 1,
            platformIdentifier: "vesc-platform-id",
            snapshot: snapshot,
            shouldBeActive: true
        )
        await coordinator.unrecoverableSessionFailure(requestID: 2, snapshot: snapshot)
        await coordinator.unrecoverableSessionFailure(requestID: 3, snapshot: snapshot)

        XCTAssertEqual(
            sessionState.rideSessionSnapshot().phase,
            .ended(reason: .unrecoverableSessionFailure)
        )
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .end(.unavailable)])
    }

    func testReconcileStartsUpdatesAndEndsOnce() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let second = liveSnapshot(label: "Connected ride", speedMph: 21.6)

        await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        await coordinator.reconcile(requestID: 2, snapshot: first, shouldBeActive: true)
        await coordinator.reconcile(requestID: 3, snapshot: second, shouldBeActive: true)
        await coordinator.reconcile(requestID: 4, snapshot: second, shouldBeActive: false, endReason: .disconnected)

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(first), .update(second), .end(.disconnected)])
    }

    func testReconcileWithoutSnapshotClearsAnOrphanedActivityOnce() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)

        await coordinator.reconcile(requestID: 1, snapshot: nil, shouldBeActive: true)
        await coordinator.reconcile(requestID: 2, snapshot: nil, shouldBeActive: false, endReason: .sessionEnded)

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.end(.sessionEnded)])
    }

    func testEndStopsActiveActivityOnce() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.end(requestID: 2, reason: .sessionEnded)
        await coordinator.end(requestID: 3, reason: .sessionEnded)

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .end(.sessionEnded)])
    }

    func testFailedEndKeepsActivityActiveForRetry() async {
        let manager = RecordingLiveActivityRideLifecycleManager(endError: .activityUnavailable)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.end(requestID: 2, reason: .sessionEnded)

        let error = await coordinator.lastError
        XCTAssertEqual(error, .activityUnavailable)

        await manager.setEndError(nil)
        await coordinator.end(requestID: 3, reason: .sessionEnded)

        let recoveredError = await coordinator.lastError
        let events = await manager.recordedEvents()
        XCTAssertNil(recoveredError)
        XCTAssertEqual(
            events,
            [.start(snapshot), .end(.sessionEnded), .end(.sessionEnded)]
        )
    }

    func testEndReconcilesAnOrphanedActivityWithoutPriorProcessState() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)

        await coordinator.end(requestID: 1, reason: .sessionEnded)

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.end(.sessionEnded)])
    }

    func testLiveSnapshotCanEnterThePipeline() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 27)

        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot)])
    }

    func testLiveSnapshotKeepsSpeedUnitSeparate() {
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let fastSnapshot = liveSnapshot(label: "Connected ride", speedMph: 123.4)

        XCTAssertEqual(snapshot.speed.value, "19.8")
        XCTAssertEqual(snapshot.speed.unit, "mph")
        XCTAssertEqual(fastSnapshot.speed.value, "123.4")
        XCTAssertEqual(fastSnapshot.speed.unit, "mph")
    }

    func testIdentityChangeEndsPreviousActivityBeforeStartingReplacement() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = liveSnapshot(label: "First ride", speedMph: 19.8)
        let replacement = liveSnapshot(label: "Second ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        await coordinator.reconcile(requestID: 2, snapshot: replacement, shouldBeActive: true)

        let events = await manager.recordedEvents()
        XCTAssertEqual(
            events,
            [.start(first), .end(.sessionEnded), .start(replacement)]
        )
    }

    func testIdentityChangeDoesNotStartReplacementUntilPreviousActivityEnds() async {
        let manager = RecordingLiveActivityRideLifecycleManager(endError: .activityUnavailable)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = liveSnapshot(label: "First ride", speedMph: 19.8)
        let replacement = liveSnapshot(label: "Second ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        await coordinator.reconcile(requestID: 2, snapshot: replacement, shouldBeActive: true)

        let error = await coordinator.lastError
        let eventsAfterFailure = await manager.recordedEvents()
        XCTAssertEqual(error, .activityUnavailable)
        XCTAssertEqual(
            eventsAfterFailure,
            [.start(first), .end(.sessionEnded)]
        )

        await manager.setEndError(nil)
        await coordinator.reconcile(requestID: 3, snapshot: replacement, shouldBeActive: true)

        let recoveredError = await coordinator.lastError
        let recoveredEvents = await manager.recordedEvents()
        XCTAssertNil(recoveredError)
        XCTAssertEqual(
            recoveredEvents,
            [
                .start(first),
                .end(.sessionEnded),
                .end(.sessionEnded),
                .start(replacement),
            ]
        )
    }

    func testFailedStartDoesNotMakeTheCoordinatorActive() async {
        let manager = RecordingLiveActivityRideLifecycleManager(startError: .requestFailed)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        let startError = await coordinator.lastError
        XCTAssertEqual(startError, .requestFailed)

        await coordinator.end(requestID: 2, reason: .sessionEnded)

        let recoveredError = await coordinator.lastError
        XCTAssertNil(recoveredError)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .end(.sessionEnded)])
    }

    func testFailedUpdateKeepsActivityActiveForRetry() async {
        let manager = RecordingLiveActivityRideLifecycleManager(updateError: .activityUnavailable)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let updated = liveSnapshot(label: "Connected ride", speedMph: 20.1)

        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.reconcile(requestID: 2, snapshot: updated, shouldBeActive: true)

        let error = await coordinator.lastError
        XCTAssertEqual(error, .activityUnavailable)

        await manager.setUpdateError(nil)
        await coordinator.reconcile(requestID: 3, snapshot: updated, shouldBeActive: true)

        let recoveredError = await coordinator.lastError
        let events = await manager.recordedEvents()
        XCTAssertNil(recoveredError)
        XCTAssertEqual(
            events,
            [.start(snapshot), .update(updated), .update(updated)]
        )
    }

    func testOlderEndRequestCannotUndoANewerReconciliation() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 2, snapshot: snapshot, shouldBeActive: true)
        await coordinator.end(requestID: 1, reason: .disconnected)

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot)])
    }

    func testOlderReconciliationCannotRestartAfterANewerEnd() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.end(requestID: 3, reason: .disconnected)
        await coordinator.reconcile(requestID: 2, snapshot: snapshot, shouldBeActive: true)

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .end(.disconnected)])
    }

    func testConcurrentReconciliationsDoNotOverlapLifecycleOperations() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let second = liveSnapshot(label: "Connected ride", speedMph: 21.6)

        let firstReconciliation = Task {
            await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()

        let secondReconciliation = Task {
            await coordinator.reconcile(requestID: 2, snapshot: second, shouldBeActive: true)
        }
        await coordinator.waitForOperationQueueDepthForTesting(1)

        await manager.resumeFirstStart()
        await firstReconciliation.value
        await secondReconciliation.value

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(first), .update(second)])
    }

    func testStalledPlatformStartDoesNotRetainEachSupersededPresentationCaller() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let latest = liveSnapshot(label: "Connected ride", speedMph: 21.6)
        let firstRequest = Task {
            await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()

        let callersReleased = expectation(description: "replaceable presentation callers return while Apple is blocked")
        callersReleased.expectedFulfillmentCount = 64
        let requests = (2...65).map { requestID in
            Task {
                await coordinator.reconcile(
                    requestID: UInt64(requestID), snapshot: latest, shouldBeActive: true
                )
                callersReleased.fulfill()
            }
        }
        await fulfillment(of: [callersReleased], timeout: 1)
        let eventsWhileBlocked = await manager.recordedEvents()
        XCTAssertEqual(eventsWhileBlocked, [.start(first)])

        await manager.resumeFirstStart()
        await firstRequest.value
        for request in requests { await request.value }
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(first), .update(latest)])
    }

    func testBackgroundFlushAndPresenceAdvanceWhilePlatformStartRemainsBlocked() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let state = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let firstRequest = Task {
            await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        let capture = CaptureFlushSpy()
        let flushed = expectation(description: "capture flush bypasses stalled ActivityKit")
        let background = Task {
            await coordinator.appDidEnterBackground(
                requestID: 2, atMs: 200, snapshot: snapshot,
                captureFlush: {
                    let result = await capture.flush()
                    flushed.fulfill()
                    return result
                }
            )
        }
        await fulfillment(of: [flushed], timeout: 1)
        XCTAssertEqual(state.rideSessionSnapshot().appPresence, .background)

        await manager.resumeFirstStart()
        await firstRequest.value
        await background.value
        let flushCount = await capture.count()
        XCTAssertEqual(flushCount, 1)
    }

    func testDisconnectReducesRustLifecycleWhilePlatformStartRemainsBlocked() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let state = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let firstRequest = Task {
            await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        let disconnected = expectation(description: "logical disconnect bypasses stalled ActivityKit")
        let disconnect = Task {
            await coordinator.end(requestID: 2, reason: .disconnected)
            disconnected.fulfill()
        }
        await fulfillment(of: [disconnected], timeout: 1)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .ending(reason: .userDisconnect))

        await manager.resumeFirstStart()
        await firstRequest.value
        await disconnect.value
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .end(.disconnected)])
        XCTAssertEqual(state.rideSessionSnapshot().phase, .ended(reason: .userDisconnect))
    }

    func testRecoveryIdentitySurvivesReplacementByLatestPresentationWhilePlatformIsBlocked() async throws {
        let suiteName = "LiveActivityRideLifecycleCoordinatorTests.queued-recovery.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstEnd: true)
        let state = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, markerStore: markerStore
        )
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let latest = liveSnapshot(label: "Connected ride", speedMph: 21.6)
        let firstRequest = Task {
            await coordinator.reconcile(requestID: 1, snapshot: nil, shouldBeActive: false)
        }
        await manager.waitUntilFirstEndIsBlocked()
        let source = CutoutSessionStateHandle()
        let restored = try source.reduceRideSession(input: .start(platformIdentifier: "wheel"))
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let adopted = expectation(description: "Rust recovery bypasses stalled ActivityKit")
        let recovery = Task {
            let result = await coordinator.recoverPersistedRide(
                requestID: 2, restoredPlatformIdentifier: "wheel", snapshot: first
            )
            adopted.fulfill()
            return result
        }
        await fulfillment(of: [adopted], timeout: 1)
        XCTAssertEqual(state.rideSessionSnapshot().identity, restored.snapshot.identity)
        let latestReleased = expectation(description: "latest recovery presentation is queued without a waiter")
        let latestRequest = Task {
            await coordinator.reconcile(
                requestID: 3, platformIdentifier: "wheel", snapshot: latest, shouldBeActive: true
            )
            latestReleased.fulfill()
        }
        await fulfillment(of: [latestReleased], timeout: 1)
        await manager.resumeFirstEnd()
        await firstRequest.value
        await latestRequest.value
        let result = await recovery.value
        XCTAssertEqual(result, .adopted)
        XCTAssertEqual(state.rideSessionSnapshot().identity, restored.snapshot.identity)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .active)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.end(.sessionEnded), .start(latest)])
        let identity = await manager.startedRideSessionIdentity()
        XCTAssertEqual(identity, restored.snapshot.identity.map(LiveActivityRideSessionIdentity.init))
    }

    func testDistinctRecoveryMarkerCannotReplaceInFlightIdentityOrItsDurableMarker() async throws {
        let suiteName = "LiveActivityRideLifecycleCoordinatorTests.fenced-recovery.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let markerStore = RideSessionMarkerStore(defaults: defaults)
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let state = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, markerStore: markerStore)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let firstRequest = Task {
            await coordinator.reconcile(
                requestID: 1, platformIdentifier: "wheel", snapshot: snapshot, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        let current = state.rideSessionSnapshot().identity
        let currentMarker = try XCTUnwrap(state.exportRideSessionMarker())
        let source = CutoutSessionStateHandle()
        let other = try source.reduceRideSession(input: .start(platformIdentifier: "wheel"))
        XCTAssertNotEqual(current, other.snapshot.identity)
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let fenced = expectation(description: "distinct recovery marker is fenced before Apple completes")
        let recovery = Task {
            let result = await coordinator.recoverPersistedRide(
                requestID: 2, restoredPlatformIdentifier: "wheel", snapshot: snapshot)
            fenced.fulfill()
            return result
        }
        await fulfillment(of: [fenced], timeout: 1)
        XCTAssertEqual(state.rideSessionSnapshot().identity, current)
        XCTAssertEqual(markerStore.marker, currentMarker)
        await manager.resumeFirstStart()
        await firstRequest.value
        let result = await recovery.value
        XCTAssertEqual(result, .reconnecting)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot)])
    }

    func testStartAcknowledgementPreservesDisconnectWhenBackgroundProjectionReplacesPendingStaleWork() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let state = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let firstRequest = Task {
            await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        let disconnected = expectation(description: "Rust observes transport loss before Apple start completes")
        let disconnect = Task {
            await coordinator.transportDisconnected(requestID: 2, atMs: 100, snapshot: snapshot)
            disconnected.fulfill()
        }
        await fulfillment(of: [disconnected], timeout: 1)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .reconnecting)
        let flushed = expectation(description: "background flush remains independent of Apple")
        let background = Task {
            await coordinator.appDidEnterBackground(
                requestID: 3, atMs: 200, snapshot: snapshot,
                captureFlush: {
                    flushed.fulfill()
                    return true
                }
            )
        }
        await fulfillment(of: [flushed], timeout: 1)
        await manager.resumeFirstStart()
        await firstRequest.value
        await disconnect.value
        await background.value
        XCTAssertEqual(state.rideSessionSnapshot().phase, .reconnecting)
        XCTAssertEqual(state.rideSessionSnapshot().appPresence, .background)
        let events = await manager.recordedEvents()
        let freshness = await manager.recordedFreshnessWindows()
        XCTAssertEqual(events, [.start(snapshot), .update(snapshot)])
        XCTAssertEqual(freshness, [2_000, 0])
    }

    func testBackgroundFlushBypassesPermanentlyPendingActivityEnd() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstEnd: true)
        let state = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        let ending = Task { await coordinator.end(requestID: 2, reason: .disconnected) }
        await manager.waitUntilFirstEndIsBlocked()
        let flushed = expectation(description: "capture flush bypasses stalled ActivityKit end")
        let background = Task {
            await coordinator.appDidEnterBackground(
                requestID: 3, atMs: 200, snapshot: snapshot,
                captureFlush: {
                    flushed.fulfill()
                    return true
                }
            )
        }
        await fulfillment(of: [flushed], timeout: 1)
        XCTAssertEqual(state.rideSessionSnapshot().appPresence, .background)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .ending(reason: .userDisconnect))
        await manager.resumeFirstEnd()
        await ending.value
        await background.value
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .end(.disconnected)])
        XCTAssertEqual(state.rideSessionSnapshot().phase, .ended(reason: .userDisconnect))
    }

    func testQueuedFreshPresentationExpiresDuringStalledStartWithoutRenewingOldTelemetry() async {
        let nativeTime = Mutex<UInt64>(10_000)
        let wallTime = Mutex(Date(timeIntervalSince1970: 2_000))
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state,
            presentationNow: { nativeTime.withLock { $0 } },
            wallClock: { wallTime.withLock { $0 } }
        )
        let first = liveSnapshot(label: "Parked wheel", speedMph: 0)
        let latest = liveSnapshot(label: "Parked wheel", speedMph: 1)
        let firstTask = Task {
            await coordinator.reconcile(
                requestID: 1, monotonicTimeMs: 1_000, presentationAtMs: 1_000,
                snapshot: first, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        nativeTime.withLock { $0 = 10_100 }
        wallTime.withLock { $0 = Date(timeIntervalSince1970: 2_000.1) }
        await coordinator.reconcile(
            requestID: 2, monotonicTimeMs: 1_100, presentationAtMs: 1_100,
            snapshot: latest, shouldBeActive: true)
        nativeTime.withLock { $0 = 20_000 }
        wallTime.withLock { $0 = Date(timeIntervalSince1970: 2_010) }
        await manager.resumeFirstStart()
        await firstTask.value
        let deadlines = await manager.recordedFreshnessDeadlines()
        let events = await manager.recordedEvents()
        XCTAssertEqual(state.rideSessionSnapshot().phase, .stale)
        XCTAssertEqual(events, [.start(first), .update(latest.presented(isStale: true))])
        XCTAssertEqual(deadlines, [Date(timeIntervalSince1970: 2_002), Date(timeIntervalSince1970: 2_002.1)])
    }

    func testAlreadyExpiredInitialPresentationStartsTheOriginalIdentityWithStaleContent() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state,
            presentationNow: { 10_000 }, wallClock: { Date(timeIntervalSince1970: 2_000) }
        )
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(
            requestID: 1, monotonicTimeMs: 1_000, presentationAtMs: 4_000,
            snapshot: snapshot, shouldBeActive: true)
        let logicalIdentity = state.rideSessionSnapshot().identity
        let platformIdentity = await manager.startedRideSessionIdentity()
        let events = await manager.recordedEvents()
        let deadlines = await manager.recordedFreshnessDeadlines()
        XCTAssertNotNil(logicalIdentity)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .stale)
        XCTAssertEqual(platformIdentity, logicalIdentity.map { LiveActivityRideSessionIdentity($0) })
        XCTAssertEqual(events, [.start(snapshot.presented(isStale: true))])
        XCTAssertEqual(deadlines, [Date(timeIntervalSince1970: 1_999)])
    }

    func testPresentationDeadlineUsesActualTelemetryAgeBeforeNativeQueueAdmission() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager,
            presentationNow: { 10_000 },
            wallClock: { Date(timeIntervalSince1970: 2_000) }
        )
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(
            requestID: 1, monotonicTimeMs: 1_000, presentationAtMs: 2_500,
            snapshot: snapshot, shouldBeActive: true)
        let deadlines = await manager.recordedFreshnessDeadlines()
        XCTAssertEqual(deadlines, [Date(timeIntervalSince1970: 2_000.5)])
    }

    func testPresentationIncludesDelayBeforeCoordinatorActorAdmission() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state,
            presentationNow: { 13_000 }, wallClock: { Date(timeIntervalSince1970: 2_003) }
        )
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(
            requestID: 1, monotonicTimeMs: 1_000, presentationAtMs: 1_000,
            nativeEnqueuedAtMs: 10_000, snapshot: snapshot, shouldBeActive: true)
        let events = await manager.recordedEvents()
        let deadlines = await manager.recordedFreshnessDeadlines()
        XCTAssertEqual(state.rideSessionSnapshot().phase, .stale)
        XCTAssertEqual(events, [.start(snapshot.presented(isStale: true))])
        XCTAssertEqual(deadlines, [Date(timeIntervalSince1970: 2_002)])
    }

    func testSceneReplacementPreservesNewerAdmittedTelemetryReceipt() async {
        let nativeTime = Mutex<UInt64>(10_000)
        let wallTime = Mutex(Date(timeIntervalSince1970: 2_000))
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state,
            presentationNow: { nativeTime.withLock { $0 } },
            wallClock: { wallTime.withLock { $0 } }
        )
        let first = liveSnapshot(label: "Parked wheel", speedMph: 0)
        let latest = liveSnapshot(label: "Parked wheel", speedMph: 1)
        let starting = Task {
            await coordinator.reconcile(
                requestID: 1, monotonicTimeMs: 1_000, presentationAtMs: 1_000,
                snapshot: first, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        nativeTime.withLock { $0 = 11_000 }
        wallTime.withLock { $0 = Date(timeIntervalSince1970: 2_001) }
        await coordinator.reconcile(
            requestID: 2, monotonicTimeMs: 2_000, presentationAtMs: 2_000,
            snapshot: latest, shouldBeActive: true)
        let capture = CaptureFlushSpy()
        await coordinator.appDidEnterBackground(
            requestID: 3, atMs: 2_000, snapshot: latest, captureFlush: { await capture.flush() })
        nativeTime.withLock { $0 = 12_000 }
        wallTime.withLock { $0 = Date(timeIntervalSince1970: 2_002) }
        await manager.resumeFirstStart()
        await starting.value
        let events = await manager.recordedEvents()
        let deadlines = await manager.recordedFreshnessDeadlines()
        XCTAssertEqual(state.rideSessionSnapshot().phase, .active)
        XCTAssertEqual(events, [.start(first), .update(latest)])
        XCTAssertEqual(deadlines, [Date(timeIntervalSince1970: 2_002), Date(timeIntervalSince1970: 2_003)])
    }

    func testNewerTelemetryRequestCannotDiscardRequiredBackgroundFlush() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.reconcile(requestID: 3, snapshot: snapshot, shouldBeActive: true)
        let capture = CaptureFlushSpy()
        await coordinator.appDidEnterBackground(
            requestID: 2, atMs: 100, snapshot: snapshot, captureFlush: { await capture.flush() })
        let count = await capture.count()
        XCTAssertEqual(count, 1)
        XCTAssertEqual(state.rideSessionSnapshot().appPresence, .background)
    }

    func testNewerTelemetryRequestCannotDiscardExplicitEnd() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.reconcile(requestID: 3, snapshot: snapshot, shouldBeActive: true)
        await coordinator.end(requestID: 2, reason: .disconnected)
        let events = await manager.recordedEvents()
        XCTAssertEqual(state.rideSessionSnapshot().phase, .ended(reason: .userDisconnect))
        XCTAssertEqual(events, [.start(snapshot), .end(.disconnected)])
    }

    func testLateBackgroundBehindForegroundFlushesOnceWithoutRevertingPresence() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.appDidBecomeActive(requestID: 3, snapshot: snapshot)
        let capture = CaptureFlushSpy()
        await coordinator.appDidEnterBackground(
            requestID: 2, atMs: 100, snapshot: snapshot, captureFlush: { await capture.flush() })
        await coordinator.appDidEnterBackground(
            requestID: 2, atMs: 100, snapshot: snapshot, captureFlush: { await capture.flush() })
        let count = await capture.count()
        XCTAssertEqual(count, 1)
        XCTAssertEqual(state.rideSessionSnapshot().appPresence, .foreground)
    }

    func testOldEndCannotTerminateNewSamePlatformRideIdentity() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        let firstIdentity = state.rideSessionSnapshot().identity
        await coordinator.end(requestID: 2, reason: .disconnected)
        await coordinator.reconcile(requestID: 4, snapshot: snapshot, shouldBeActive: true)
        let secondIdentity = state.rideSessionSnapshot().identity
        await coordinator.end(requestID: 3, reason: .disconnected)
        XCTAssertNotEqual(secondIdentity, firstIdentity)
        XCTAssertEqual(state.rideSessionSnapshot().identity, secondIdentity)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .active)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot), .end(.disconnected), .start(snapshot)])
    }

    func testOldEndCannotTerminateNewDistinctPlatformRideIdentity() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let first = liveSnapshot(label: "First wheel", speedMph: 0)
        let second = liveSnapshot(label: "Second wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        await coordinator.reconcile(requestID: 3, snapshot: second, shouldBeActive: true)
        let secondIdentity = state.rideSessionSnapshot().identity
        await coordinator.end(requestID: 2, reason: .disconnected)
        XCTAssertEqual(state.rideSessionSnapshot().identity, secondIdentity)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .active)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(first), .end(.sessionEnded), .start(second)])
    }

    func testBackgroundBeforeFirstStartStillFlushesAndPreservesScene() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: state)
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        let capture = CaptureFlushSpy()
        await coordinator.appDidEnterBackground(
            requestID: 2, atMs: 100, snapshot: snapshot, captureFlush: { await capture.flush() })
        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        let count = await capture.count()
        let events = await manager.recordedEvents()
        XCTAssertEqual(count, 1)
        XCTAssertEqual(state.rideSessionSnapshot().appPresence, .background)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .active)
        XCTAssertEqual(events, [.start(snapshot)])
    }

    func testBackgroundBeforeRecoveryStillFlushesAndRestoresOriginalIdentity() async throws {
        let markerStore = RideSessionMarkerStore()
        let source = CutoutSessionStateHandle()
        let original = try source.reduceRideSession(input: .start(platformIdentifier: "wheel"))
        markerStore.save(try XCTUnwrap(source.exportRideSessionMarker()))
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, markerStore: markerStore)
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        let capture = CaptureFlushSpy()
        await coordinator.appDidEnterBackground(
            requestID: 2, atMs: 100, snapshot: snapshot, captureFlush: { await capture.flush() })
        let result = await coordinator.recoverPersistedRide(
            requestID: 1, restoredPlatformIdentifier: "wheel", snapshot: snapshot)
        let count = await capture.count()
        let events = await manager.recordedEvents()
        XCTAssertEqual(count, 1)
        XCTAssertEqual(result, .adopted)
        XCTAssertEqual(state.rideSessionSnapshot().identity, original.snapshot.identity)
        XCTAssertEqual(state.rideSessionSnapshot().appPresence, .background)
        XCTAssertEqual(events, [.start(snapshot)])
    }

    func testNewerDisplayOfOldReceiptCannotDiscardActualDisconnect() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, presentationNow: { 0 })
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, monotonicTimeMs: 90, snapshot: snapshot, shouldBeActive: true)
        await coordinator.reconcile(requestID: 3, monotonicTimeMs: 90, snapshot: snapshot, shouldBeActive: true)
        await coordinator.transportDisconnected(requestID: 2, atMs: 100, snapshot: snapshot)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .reconnecting)
    }

    func testActualNewerReceiptFencesLateDisconnectFromEarlierTransport() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, presentationNow: { 0 })
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, monotonicTimeMs: 90, snapshot: snapshot, shouldBeActive: true)
        await coordinator.reconcile(requestID: 3, monotonicTimeMs: 200, snapshot: snapshot, shouldBeActive: true)
        await coordinator.transportDisconnected(requestID: 2, atMs: 100, snapshot: snapshot)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .active)
    }

    func testExplicitStopBeforeRecoveryClearsMarkerAndFencesLateRecovery() async throws {
        let markerStore = RideSessionMarkerStore()
        let original = CutoutSessionStateHandle()
        _ = try original.reduceRideSession(input: .start(platformIdentifier: "wheel"))
        markerStore.save(try XCTUnwrap(original.exportRideSessionMarker()))
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, markerStore: markerStore)
        await coordinator.end(requestID: 2, reason: .sessionEnded)
        let result = await coordinator.recoverPersistedRide(
            requestID: 1, restoredPlatformIdentifier: "wheel",
            snapshot: liveSnapshot(label: "Parked wheel", speedMph: 0))
        XCTAssertEqual(result, .noPersistedRide)
        await markerStore.waitForPersistence()
        XCTAssertNil(markerStore.marker)
        XCTAssertNil(state.rideSessionSnapshot().identity)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .ended(reason: .userStop))
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.end(.sessionEnded)])
    }

    func testQueuedOldReceiptCannotReviveDisconnectWhenHeldStartAcknowledges() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, presentationNow: { 0 })
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        let starting = Task {
            await coordinator.reconcile(requestID: 1, monotonicTimeMs: 90, snapshot: snapshot, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        await coordinator.reconcile(requestID: 3, monotonicTimeMs: 90, snapshot: snapshot, shouldBeActive: true)
        await coordinator.transportDisconnected(requestID: 2, atMs: 100, snapshot: snapshot)
        await manager.resumeFirstStart()
        await starting.value
        XCTAssertEqual(state.rideSessionSnapshot().phase, .reconnecting)
        let windows = await manager.recordedFreshnessWindows()
        XCTAssertEqual(windows, [2_000, 0])
    }

    func testRepeatedPendingStartPayloadDoesNotHideDisconnectBeforeAnyRideIdentityExists() async {
        let state = CutoutSessionStateHandle()
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstEnd: true)
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state, presentationNow: { 0 })
        let snapshot = liveSnapshot(label: "Parked wheel", speedMph: 0)
        let orphanCleanup = Task {
            await coordinator.reconcile(requestID: 1, snapshot: nil, shouldBeActive: false)
        }
        await manager.waitUntilFirstEndIsBlocked()
        await coordinator.reconcile(requestID: 2, monotonicTimeMs: 90, snapshot: snapshot, shouldBeActive: true)
        await coordinator.reconcile(requestID: 4, monotonicTimeMs: 90, snapshot: snapshot, shouldBeActive: true)
        await coordinator.transportDisconnected(requestID: 3, atMs: 100, snapshot: snapshot)
        await manager.resumeFirstEnd()
        await orphanCleanup.value
        let beforeFreshReceipt = await manager.recordedEvents()
        XCTAssertEqual(beforeFreshReceipt, [.end(.sessionEnded)])
        XCTAssertNil(state.rideSessionSnapshot().identity)
        await coordinator.reconcile(requestID: 5, monotonicTimeMs: 200, snapshot: snapshot, shouldBeActive: true)
        let afterFreshReceipt = await manager.recordedEvents()
        XCTAssertEqual(afterFreshReceipt, [.end(.sessionEnded), .start(snapshot)])
        XCTAssertEqual(state.rideSessionSnapshot().phase, .active)
    }

    func testEqualContentRenewsFreshnessOnlyWhenReceivedTelemetryAdvancesEnough() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let parked = liveSnapshot(label: "Parked wheel", speedMph: 0)
        await coordinator.reconcile(requestID: 1, monotonicTimeMs: 1_000, snapshot: parked, shouldBeActive: true)
        await coordinator.reconcile(requestID: 2, monotonicTimeMs: 1_500, snapshot: parked, shouldBeActive: true)
        await coordinator.reconcile(requestID: 3, monotonicTimeMs: 2_000, snapshot: parked, shouldBeActive: true)
        await coordinator.reconcile(requestID: 4, monotonicTimeMs: 2_000, snapshot: parked, shouldBeActive: true)
        await coordinator.reconcile(requestID: 5, monotonicTimeMs: 1_999, snapshot: parked, shouldBeActive: true)
        await coordinator.reconcile(requestID: 6, monotonicTimeMs: 3_000, snapshot: parked, shouldBeActive: true)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(parked), .update(parked), .update(parked)])
    }

    func testQueuedTerminalCleanupPrecedesNewRidePresentationAndRejectsLateStartAcknowledgement() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let sessionState = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager, sessionState: sessionState)
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let latest = liveSnapshot(label: "Connected ride", speedMph: 21.6)

        let firstReconciliation = Task {
            await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()
        let firstIdentity = sessionState.rideSessionSnapshot().identity

        let staleEnd = Task {
            await coordinator.end(requestID: 2, reason: .disconnected)
        }
        await coordinator.waitForOperationQueueDepthForTesting(1)
        let latestReconciliation = Task {
            await coordinator.reconcile(requestID: 3, snapshot: latest, shouldBeActive: true)
        }
        await coordinator.waitForOperationQueueDepthForTesting(2)

        await manager.resumeFirstStart()
        await firstReconciliation.value
        await staleEnd.value
        await latestReconciliation.value

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(first), .end(.disconnected), .start(latest)])
        XCTAssertEqual(sessionState.rideSessionSnapshot().phase, .active)
        XCTAssertNotEqual(sessionState.rideSessionSnapshot().identity, firstIdentity)
    }

    func testRelaunchReconciliationAdoptsOneMatchingActivityAndEndsEverySibling() {
        let desired = LiveActivityRideIdentity.device("Current ride")
        let stale = LiveActivityRideIdentity.device("Previous ride")

        XCTAssertEqual(
            liveActivityRideReconciliation(
                existingIdentities: [stale, desired, desired],
                desiredIdentity: desired
            ),
            LiveActivityRideReconciliation(adoptedIndex: 1, staleIndices: [0, 2])
        )
        XCTAssertEqual(
            liveActivityRideReconciliation(
                existingIdentities: [stale],
                desiredIdentity: desired
            ),
            LiveActivityRideReconciliation(adoptedIndex: nil, staleIndices: [0])
        )
    }

    func testRelaunchReconciliationMatchesRustSessionIdentityNotDisplayIdentity() {
        let previous = LiveActivityRideSessionIdentity(
            platformIdentifier: "vesc-platform-id",
            sessionID: "00000000-0000-0000-0000-000000000001"
        )
        let current = LiveActivityRideSessionIdentity(
            platformIdentifier: "vesc-platform-id",
            sessionID: "00000000-0000-0000-0000-000000000002"
        )

        XCTAssertEqual(
            liveActivityRideReconciliation(
                existingIdentities: [previous, current, current],
                desiredIdentity: current
            ),
            LiveActivityRideReconciliation(adoptedIndex: 1, staleIndices: [0, 2])
        )
    }
}

private actor RecordingLiveActivityRideLifecycleManager: LiveActivityRideLifecycleManaging {
    enum Event: Equatable {
        case start(LiveActivityRideSnapshot)
        case update(LiveActivityRideSnapshot)
        case end(LiveActivityRideLifecycleEndReason)
    }

    private var events: [Event] = []
    private var startedIdentity: LiveActivityRideSessionIdentity?
    private var freshnessDeadlines: [Date] = []
    private var freshnessWindows: [UInt64] = []
    private let startError: LiveActivityRideLifecycleError?
    private var updateError: LiveActivityRideLifecycleError?
    private var endError: LiveActivityRideLifecycleError?
    private let blockFirstStart: Bool
    private let blockFirstEnd: Bool
    private var firstStartBlocked = false
    private var firstStartWaiter: CheckedContinuation<Void, Never>?
    private var firstStartBlockedWaiter: CheckedContinuation<Void, Never>?
    private var firstEndBlocked = false
    private var firstEndWaiter: CheckedContinuation<Void, Never>?
    private var firstEndBlockedWaiter: CheckedContinuation<Void, Never>?

    init(
        startError: LiveActivityRideLifecycleError? = nil,
        updateError: LiveActivityRideLifecycleError? = nil,
        endError: LiveActivityRideLifecycleError? = nil,
        blockFirstStart: Bool = false,
        blockFirstEnd: Bool = false
    ) {
        self.startError = startError
        self.updateError = updateError
        self.endError = endError
        self.blockFirstStart = blockFirstStart
        self.blockFirstEnd = blockFirstEnd
    }

    func start(
        snapshot: LiveActivityRideSnapshot,
        rideSessionIdentity: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds: UInt64,
        freshnessExpiresAt: Date
    ) async throws -> LiveActivityRideStartOutcome {
        events.append(.start(snapshot))
        startedIdentity = rideSessionIdentity
        freshnessWindows.append(staleAfterMilliseconds)
        freshnessDeadlines.append(freshnessExpiresAt)
        if let startError { throw startError }

        if blockFirstStart, firstStartBlocked == false {
            firstStartBlocked = true
            firstStartBlockedWaiter?.resume()
            firstStartBlockedWaiter = nil
            await withCheckedContinuation { firstStartWaiter = $0 }
        }
        return .started(activityID: "activity-1")
    }

    func update(
        snapshot: LiveActivityRideSnapshot,
        staleAfterMilliseconds: UInt64,
        freshnessExpiresAt: Date
    ) throws -> LiveActivityRideUpdateOutcome {
        events.append(.update(snapshot))
        freshnessWindows.append(staleAfterMilliseconds)
        freshnessDeadlines.append(freshnessExpiresAt)
        if let updateError { throw updateError }
        return LiveActivityRideUpdateOutcome(activityID: "activity-1")
    }

    func end(reason: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        events.append(.end(reason))
        if let endError { throw endError }
        if blockFirstEnd, firstEndBlocked == false {
            firstEndBlocked = true
            firstEndBlockedWaiter?.resume()
            firstEndBlockedWaiter = nil
            await withCheckedContinuation { firstEndWaiter = $0 }
        }
        return LiveActivityRideEndOutcome(activityIDs: ["activity-1"])
    }

    func recordedEvents() -> [Event] { events }

    func startedRideSessionIdentity() -> LiveActivityRideSessionIdentity? { startedIdentity }
    func recordedFreshnessDeadlines() -> [Date] { freshnessDeadlines }

    func recordedFreshnessWindows() -> [UInt64] { freshnessWindows }

    func setEndError(_ error: LiveActivityRideLifecycleError?) {
        endError = error
    }

    func setUpdateError(_ error: LiveActivityRideLifecycleError?) {
        updateError = error
    }

    func waitUntilFirstStartIsBlocked() async {
        guard firstStartBlocked == false else { return }
        await withCheckedContinuation { firstStartBlockedWaiter = $0 }
    }

    func resumeFirstStart() {
        firstStartWaiter?.resume()
        firstStartWaiter = nil
    }

    func waitUntilFirstEndIsBlocked() async {
        guard firstEndBlocked == false else { return }
        await withCheckedContinuation { firstEndBlockedWaiter = $0 }
    }

    func resumeFirstEnd() {
        firstEndWaiter?.resume()
        firstEndWaiter = nil
    }
}

private actor CaptureFlushSpy {
    private var flushCount = 0

    func flush() -> Bool {
        flushCount += 1
        return true
    }

    func count() -> Int { flushCount }
}

private func liveSnapshot(label: String, speedMph: Double) -> LiveActivityRideSnapshot {
    let speed = Int32((speedMph * 447.04).rounded())
    return LiveActivityRideSnapshot(
        identity: .device(label),
        rideState: EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                speed: SpeedReadout(millimetersPerSecond: speed),
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: speed),
                    operatingState: .riding
                )
            )
        ),
        now: MonotonicMilliseconds(1_100)
    )
}

extension LiveActivityRideLifecycleCoordinatorTests {
    func testStaleMarkerReadFailureCannotReplaceNewerTerminalError() async {
        let loader = BlockingMarkerRead()
        let manager = RecordingLiveActivityRideLifecycleManager(endError: .authorizationDenied)
        let state = CutoutSessionStateHandle()
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: manager, sessionState: state,
            loadMarker: { try await loader.load() })
        let oldRecovery = Task {
            await coordinator.recoverPersistedRide(
                requestID: 1, restoredPlatformIdentifier: "wheel", snapshot: nil)
        }
        await loader.waitUntilEntered()
        await coordinator.end(requestID: 2, reason: .sessionEnded)
        await loader.fail()
        let result = await oldRecovery.value
        let error = await coordinator.lastError
        XCTAssertEqual(result, .noPersistedRide)
        XCTAssertEqual(error, .authorizationDenied)
        XCTAssertNil(state.rideSessionSnapshot().identity)
        XCTAssertEqual(state.rideSessionSnapshot().phase, .ended(reason: .userStop))
    }
}

private actor BlockingMarkerRead {
    private var entered = false
    private var enteredWaiter: CheckedContinuation<Void, Never>?
    private var result: CheckedContinuation<Data?, Error>?

    func load() async throws -> Data? {
        entered = true
        enteredWaiter?.resume()
        enteredWaiter = nil
        return try await withCheckedThrowingContinuation { result = $0 }
    }

    func waitUntilEntered() async {
        guard !entered else { return }
        await withCheckedContinuation { enteredWaiter = $0 }
    }

    func fail() {
        result?.resume(throwing: LiveActivityRideLifecycleError.requestFailed)
        result = nil
    }
}
