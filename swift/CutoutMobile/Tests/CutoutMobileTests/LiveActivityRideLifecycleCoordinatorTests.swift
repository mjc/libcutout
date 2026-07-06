import XCTest
@testable import CutoutMobile

final class LiveActivityRideLifecycleCoordinatorTests: XCTestCase {
    func testReconcileStartsUpdatesAndEndsOnce() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = fixtureSnapshot(label: "Demo ride", speedMph: 19)
        let second = fixtureSnapshot(label: "Demo ride", speedMph: 21)

        coordinator.reconcile(snapshot: first, shouldBeActive: true)
        coordinator.reconcile(snapshot: first, shouldBeActive: true)
        coordinator.reconcile(snapshot: second, shouldBeActive: true)
        coordinator.reconcile(snapshot: second, shouldBeActive: false, endReason: .disconnected)

        XCTAssertEqual(manager.events, [
            .start(first),
            .update(second),
            .end(.disconnected),
        ])
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
        let snapshot = fixtureSnapshot(label: "Demo ride", speedMph: 19)

        coordinator.reconcile(snapshot: snapshot, shouldBeActive: true)
        coordinator.end(reason: .sessionEnded)
        coordinator.end(reason: .sessionEnded)

        XCTAssertEqual(manager.events, [
            .start(snapshot),
            .end(.sessionEnded),
        ])
    }

    func testFixtureSnapshotCanEnterThePipeline() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let snapshot = LiveActivityRideSnapshot.fixture(
            identity: .fixture(label: "Lynx-S"),
            speed: .available(label: "Speed", value: "27", unit: "mph", source: .fixture),
            battery: .available(label: "Battery", value: "68", unit: "%", source: .fixture),
            packVoltage: .available(label: "Voltage", value: "118.4", unit: "V", source: .fixture),
            pwm: .available(label: "PWM", value: "54", unit: "%", source: .fixture),
            mode: .available(label: "Mode", value: "Sport", unit: nil, source: .fixture),
            duration: .available(label: "Duration", value: "18:42", unit: nil, source: .fixture),
            distance: .available(label: "Distance", value: "7.8", unit: "mi", source: .fixture),
            headroom: .available(label: "Headroom", value: "Headroom good", unit: nil, source: .fixture),
            beeps: .available(label: "Beeps", value: "Beeps armed", unit: nil, source: .fixture),
            temperature: .available(label: "Temp", value: "34", unit: "C", source: .fixture)
        )

        coordinator.reconcile(snapshot: snapshot, shouldBeActive: true)

        XCTAssertEqual(manager.events, [.start(snapshot)])
    }

    func testFixtureHelperKeepsSpeedUnitSeparate() {
        let snapshot = fixtureSnapshot(label: "Demo ride", speedMph: 19)

        XCTAssertEqual(snapshot.speed.value, "19")
        XCTAssertEqual(snapshot.speed.unit, "mph")
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

private func fixtureSnapshot(label: String, speedMph: Int) -> LiveActivityRideSnapshot {
    LiveActivityRideSnapshot.fixture(
        identity: .fixture(label: label),
        speed: .available(label: "Speed", value: String(speedMph), unit: "mph", source: .fixture),
        battery: .available(label: "Battery", value: "68", unit: "%", source: .fixture),
        packVoltage: .available(label: "Voltage", value: "118.4", unit: "V", source: .fixture),
        pwm: .available(label: "PWM", value: "54", unit: "%", source: .fixture),
        mode: .available(label: "Mode", value: "Sport", unit: nil, source: .fixture),
        duration: .available(label: "Duration", value: "18:42", unit: nil, source: .fixture),
        distance: .available(label: "Distance", value: "7.8", unit: "mi", source: .fixture),
        headroom: .available(label: "Headroom", value: "Headroom good", unit: nil, source: .fixture),
        beeps: .available(label: "Beeps", value: "Beeps armed", unit: nil, source: .fixture),
        temperature: .available(label: "Temp", value: "34", unit: "C", source: .fixture)
    )
}
