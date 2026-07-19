import XCTest
@testable import CutoutMobile

final class LiveActivityRideLifecycleCoordinatorTests: XCTestCase {
    func testReconcileStartsUpdatesAndEndsOnce() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let second = liveSnapshot(label: "Connected ride", speedMph: 21.6)

        coordinator.reconcile(snapshot: first, shouldBeActive: true)
        coordinator.reconcile(snapshot: first, shouldBeActive: true)
        coordinator.reconcile(snapshot: second, shouldBeActive: true)
        coordinator.reconcile(snapshot: second, shouldBeActive: false, endReason: .disconnected)

        XCTAssertEqual(manager.events, [.start(first), .update(second), .end(.disconnected)])
    }

    func testReconcileDoesNotStartWithoutSnapshot() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)

        coordinator.reconcile(snapshot: nil, shouldBeActive: true)
        coordinator.reconcile(snapshot: nil, shouldBeActive: false, endReason: .sessionEnded)

        XCTAssertTrue(manager.events.isEmpty)
    }

    func testEndStopsActiveActivityOnce() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)

        coordinator.reconcile(snapshot: snapshot, shouldBeActive: true)
        coordinator.end(reason: .sessionEnded)
        coordinator.end(reason: .sessionEnded)

        XCTAssertEqual(manager.events, [.start(snapshot), .end(.sessionEnded)])
    }

    func testEndDoesNothingWithoutPriorStart() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)

        coordinator.end(reason: .sessionEnded)

        XCTAssertTrue(manager.events.isEmpty)
    }

    func testLiveSnapshotCanEnterThePipeline() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 27)

        coordinator.reconcile(snapshot: snapshot, shouldBeActive: true)

        XCTAssertEqual(manager.events, [.start(snapshot)])
    }

    func testLiveSnapshotKeepsSpeedUnitSeparate() {
        let snapshot = liveSnapshot(label: "Connected ride", speedMph: 19.8)
        let fastSnapshot = liveSnapshot(label: "Connected ride", speedMph: 123.4)

        XCTAssertEqual(snapshot.speed.value, "19.8")
        XCTAssertEqual(snapshot.speed.unit, "mph")
        XCTAssertEqual(fastSnapshot.speed.value, "123.4")
        XCTAssertEqual(fastSnapshot.speed.unit, "mph")
    }
}

private final class RecordingLiveActivityRideLifecycleManager: LiveActivityRideLifecycleManaging {
    enum Event: Equatable {
        case start(LiveActivityRideSnapshot)
        case update(LiveActivityRideSnapshot)
        case end(LiveActivityRideLifecycleEndReason)
    }

    private(set) var events: [Event] = []

    func start(snapshot: LiveActivityRideSnapshot) {
        events.append(.start(snapshot))
    }

    func update(snapshot: LiveActivityRideSnapshot) {
        events.append(.update(snapshot))
    }

    func end(reason: LiveActivityRideLifecycleEndReason) {
        events.append(.end(reason))
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
