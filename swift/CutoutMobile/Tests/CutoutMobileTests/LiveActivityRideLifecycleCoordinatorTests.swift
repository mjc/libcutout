import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class LiveActivityRideLifecycleCoordinatorTests: XCTestCase {
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

    func testReconcileDoesNotStartWithoutSnapshot() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)

        await coordinator.reconcile(requestID: 1, snapshot: nil, shouldBeActive: true)
        await coordinator.reconcile(requestID: 2, snapshot: nil, shouldBeActive: false, endReason: .sessionEnded)

        let events = await manager.recordedEvents()
        XCTAssertTrue(events.isEmpty)
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

    func testEndDoesNothingWithoutPriorStart() async {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)

        await coordinator.end(requestID: 1, reason: .sessionEnded)

        let events = await manager.recordedEvents()
        XCTAssertTrue(events.isEmpty)
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

    func testFailedStartDoesNotMakeTheCoordinatorActive() async {
        let manager = RecordingLiveActivityRideLifecycleManager(startError: .requestFailed)
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        await coordinator.reconcile(requestID: 1, snapshot: snapshot, shouldBeActive: true)
        await coordinator.end(requestID: 2, reason: .sessionEnded)

        let error = await coordinator.lastError
        XCTAssertEqual(error, .requestFailed)
        let events = await manager.recordedEvents()
        XCTAssertEqual(events, [.start(snapshot)])
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

    func start(snapshot: LiveActivityRideSnapshot) async throws {
        events.append(.start(snapshot))
        if let startError { throw startError }

        guard blockFirstStart, firstStartBlocked == false else { return }
        firstStartBlocked = true
        firstStartBlockedWaiter?.resume()
        firstStartBlockedWaiter = nil
        await withCheckedContinuation { firstStartWaiter = $0 }
    }

    func update(snapshot: LiveActivityRideSnapshot) throws {
        events.append(.update(snapshot))
        if let updateError { throw updateError }
    }

    func end(reason: LiveActivityRideLifecycleEndReason) throws {
        events.append(.end(reason))
        if let endError { throw endError }
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
