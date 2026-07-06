import XCTest
@testable import CutoutMobile

final class LiveActivityRideLifecycleCoordinatorTests: XCTestCase {
    func testReconcileStartsUpdatesAndEndsOnce() {
        let manager = RecordingLiveActivityRideLifecycleManager()
        let coordinator = LiveActivityRideLifecycleCoordinator(manager: manager)
        let first = fixtureSnapshot(label: "Demo ride", speed: "19 mph")
        let second = fixtureSnapshot(label: "Demo ride", speed: "21 mph")

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

private func fixtureSnapshot(label: String, speed: String) -> LiveActivityRideSnapshot {
    LiveActivityRideSnapshot.fixture(
        identity: .fixture(label: label),
        speed: .available(label: "Speed", value: speed, unit: "mph", source: .fixture),
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
