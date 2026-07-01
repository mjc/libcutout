import XCTest
@testable import CutoutMobile

final class LiveSpeedSessionCoreTests: XCTestCase {
    func testObservedAdvertisementsUpdatePickerScanState() {
        let core = LiveSpeedSessionCore()
        var observedStates: [DevicePickerScanState] = []
        core.onScanStateChange = { observedStates.append($0) }

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NOSFET Aero",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-unknown"),
                localName: "Rideable-ish",
                advertisedServiceUuids: []
            )
        )

        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["NOSFET Aero", "Rideable-ish"])
        XCTAssertEqual(core.scanState.sections.supported.map(\.title), ["NOSFET Aero"])
        XCTAssertEqual(core.scanState.sections.unsupported.map(\.title), ["Rideable-ish"])
        XCTAssertEqual(observedStates.count, 2)
    }

    func testPairUnknownCandidateReturnsFalse() {
        let core = LiveSpeedSessionCore()

        XCTAssertFalse(core.pair(platformIdentifier: "ios-local-missing"))
    }

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

    func testNotificationWithoutSnapshotAdvancesLastUpdate() {
        let core = LiveSpeedSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speedMillimetersPerSecond: 1_234,
            voltageMillivolts: 117_000,
            batteryLevelEstimated: 77
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

    func testDisplayStateProvidesDebugRowsForLiveValidation() {
        let displayState = LiveSpeedDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )

        XCTAssertEqual(
            displayState.debugRows,
            [
                LiveSpeedDebugRow(label: "Notifications", value: "7"),
                LiveSpeedDebugRow(label: "Last update", value: "9876 ms"),
            ]
        )
    }
}
