import XCTest
@testable import CutoutMobile

final class LiveSpeedSessionCoreTests: XCTestCase {
    func testApplyNotificationStepMarksLiveAndUpdatesDisplayState() {
        let core = LiveSpeedSessionCore()
        let snapshot = TelemetrySnapshot(
            speedMillimetersPerSecond: 1_234,
            voltageMillivolts: 117_000,
            batteryLevelEstimated: 77
        )
        let step = CoreBluetoothSessionStep(operations: [], snapshot: snapshot)
        let receivedAt = MonotonicMilliseconds(42)

        core.applyNotificationStep(step, receivedAt: receivedAt)

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.notificationCount, 1)
        XCTAssertEqual(core.displayState.lastUpdate, receivedAt)
    }

    func testSpeedObservationRemainsStickyAcrossTelemetryWithoutSpeed() {
        let core = LiveSpeedSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speedMillimetersPerSecond: 1_234,
            voltageMillivolts: 117_000,
            batteryLevelEstimated: 77
        )
        let batteryOnlySnapshot = TelemetrySnapshot(
            speedMillimetersPerSecond: nil,
            voltageMillivolts: 116_500,
            batteryLevelEstimated: 76
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
        XCTAssertEqual(core.displayState.notificationCount, 2)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(43))
    }
}
