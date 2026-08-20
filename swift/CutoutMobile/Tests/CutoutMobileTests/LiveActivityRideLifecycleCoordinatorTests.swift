import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class LiveActivityRideLifecycleCoordinatorTests: XCTestCase {
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
        XCTAssertEqual(events, [.start(first), .update(first), .update(resumed)])
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
        try? await Task.sleep(for: .milliseconds(50))

        let eventsBeforeRelease = await manager.recordedEvents()
        XCTAssertEqual(eventsBeforeRelease, [.start(first)])

        await manager.resumeFirstStart()
        await firstReconciliation.value
        await secondReconciliation.value

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(first), .update(second)])
    }

    func testQueuedRequestIsRecheckedAfterWaitingForAnInFlightOperation() async {
        let manager = RecordingLiveActivityRideLifecycleManager(blockFirstStart: true)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let latest = liveSnapshot(label: "Connected ride", speedMph: 21.6)

        let firstReconciliation = Task {
            await coordinator.reconcile(requestID: 1, snapshot: first, shouldBeActive: true)
        }
        await manager.waitUntilFirstStartIsBlocked()

        let staleEnd = Task {
            await coordinator.end(requestID: 2, reason: .disconnected)
        }
        try? await Task.sleep(for: .milliseconds(50))
        let latestReconciliation = Task {
            await coordinator.reconcile(requestID: 3, snapshot: latest, shouldBeActive: true)
        }
        try? await Task.sleep(for: .milliseconds(50))

        await manager.resumeFirstStart()
        await firstReconciliation.value
        await staleEnd.value
        await latestReconciliation.value

        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(first), .update(latest)])
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
}

private actor RecordingLiveActivityRideLifecycleManager: LiveActivityRideLifecycleManaging {
    enum Event: Equatable {
        case start(LiveActivityRideSnapshot)
        case update(LiveActivityRideSnapshot)
        case end(LiveActivityRideLifecycleEndReason)
    }

    private var events: [Event] = []
    private let startError: LiveActivityRideLifecycleError?
    private var updateError: LiveActivityRideLifecycleError?
    private var endError: LiveActivityRideLifecycleError?
    private let blockFirstStart: Bool
    private var firstStartBlocked = false
    private var firstStartWaiter: CheckedContinuation<Void, Never>?
    private var firstStartBlockedWaiter: CheckedContinuation<Void, Never>?

    init(
        startError: LiveActivityRideLifecycleError? = nil,
        updateError: LiveActivityRideLifecycleError? = nil,
        endError: LiveActivityRideLifecycleError? = nil,
        blockFirstStart: Bool = false
    ) {
        self.startError = startError
        self.updateError = updateError
        self.endError = endError
        self.blockFirstStart = blockFirstStart
    }

    func start(snapshot: LiveActivityRideSnapshot) async throws -> LiveActivityRideStartOutcome {
        events.append(.start(snapshot))
        if let startError { throw startError }

        if blockFirstStart, firstStartBlocked == false {
            firstStartBlocked = true
            firstStartBlockedWaiter?.resume()
            firstStartBlockedWaiter = nil
            await withCheckedContinuation { firstStartWaiter = $0 }
        }
        return .started(activityID: "activity-1")
    }

    func update(snapshot: LiveActivityRideSnapshot) throws -> LiveActivityRideUpdateOutcome {
        events.append(.update(snapshot))
        if let updateError { throw updateError }
        return LiveActivityRideUpdateOutcome(activityID: "activity-1")
    }

    func end(reason: LiveActivityRideLifecycleEndReason) throws -> LiveActivityRideEndOutcome {
        events.append(.end(reason))
        if let endError { throw endError }
        return LiveActivityRideEndOutcome(activityIDs: ["activity-1"])
    }

    func recordedEvents() -> [Event] { events }

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
