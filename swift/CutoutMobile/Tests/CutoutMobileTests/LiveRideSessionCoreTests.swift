import XCTest
@testable import CutoutMobile

final class LiveRideSessionCoreTests: XCTestCase {
    func testSessionErrorWrapsNewRustInputAndBufferFailures() {
        XCTAssertEqual(
            CutoutSessionError(
                MobileSessionStepErrorDto(kind: .invalidInput, command: nil, reason: "bad uuid")
            ),
            .unexpectedStepError("bad uuid")
        )
        XCTAssertEqual(
            CutoutSessionError(
                MobileSessionStepErrorDto(
                    kind: .outputBufferFull,
                    command: nil,
                    reason: "session output capacity exceeded"
                )
            ),
            .unexpectedStepError("session output capacity exceeded")
        )
    }

    func testObservedAdvertisementsUpdatePickerScanState() {
        let core = LiveRideSessionCore()
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
                localName: "Little FOCer",
                advertisedServiceUuids: [.bluetooth16(0xFFF0)]
            )
        )

        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["NOSFET Aero", "Little FOCer"])
        XCTAssertEqual(core.scanState.sections.supported.map(\.title), ["NOSFET Aero"])
        XCTAssertEqual(core.scanState.sections.unsupported.map(\.title), ["Little FOCer"])
        XCTAssertEqual(observedStates.count, 2)
    }

    func testObservedAdvertisementsHideNonPevRows() {
        let core = LiveRideSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-keyboard"),
                localName: "Keyboard",
                advertisedServiceUuids: []
            )
        )

        XCTAssertTrue(core.scanState.rows.isEmpty)
    }

    func testPairUnknownCandidateReturnsFalse() {
        let core = LiveRideSessionCore()

        XCTAssertFalse(core.pair(platformIdentifier: "ios-local-missing"))
    }

    func testObservedAdvertisementsReplaceDuplicatePeripheralRows() {
        let core = LiveRideSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Begode Falcon",
                advertisedServiceUuids: []
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Begode Falcon",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        XCTAssertEqual(core.scanState.rows.map(\.id), ["ios-local-falcon"])
        XCTAssertEqual(core.scanState.sections.supported.map(\.title), ["Begode Falcon"])
    }

    func testSyntheticLifecycleStepsCoverConnectDiscoverSubscribeRecords() {
        let core = LiveRideSessionCore()
        var phases: [LiveRideConnectionPhase] = []
        core.onPhaseChange = { phases.append($0) }

        core.applyLifecycleStep(
            .connecting(model: .falcon, platformIdentifier: "ios-local-falcon")
        )
        core.applyLifecycleStep(.discoveringServices(["FFE0", "FFF0"]))
        core.applyLifecycleStep(.subscribing(["FFE1"]))

        XCTAssertEqual(phases, [.connecting(model: .falcon), .discoveringServices, .subscribing])
        XCTAssertEqual(core.phase, .subscribing)
        XCTAssertEqual(
            core.records,
            [
                "connect_model=Falcon platform_identifier=ios-local-falcon",
                "services=FFE0,FFF0",
                "subscribe_channels=FFE1",
            ]
        )
    }

    func testApplyNotificationStepMarksLiveAndUpdatesDisplayState() {
        let core = LiveRideSessionCore()
        let snapshot = TelemetrySnapshot(
            speed: telemetryReading(1_234),
            voltage: telemetryReading(117_000),
            batteryLevelEstimated: telemetryReading(77)
        )
        let step = CoreBluetoothSessionStep(operations: [], snapshot: snapshot)
        let receivedAt = MonotonicMilliseconds(42)

        core.applyNotificationStep(step, receivedAt: receivedAt)

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.telemetry?.speed?.value, 1_234)
        XCTAssertEqual(core.displayState.notificationCount, 1)
        XCTAssertEqual(core.displayState.lastUpdate, receivedAt)
        XCTAssertTrue(core.records.contains("display_speed=2.8 display_unit=mph"))
    }

    func testSpeedObservationRemainsStickyAcrossTelemetryWithoutSpeed() {
        let core = LiveRideSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: telemetryReading(1_234),
            voltage: telemetryReading(117_000),
            batteryLevelEstimated: telemetryReading(77)
        )
        let batteryOnlySnapshot = TelemetrySnapshot(
            voltage: telemetryReading(116_500),
            batteryLevelEstimated: telemetryReading(76)
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
        XCTAssertEqual(core.displayState.telemetry?.voltage?.value, 116_500)
        XCTAssertEqual(core.displayState.notificationCount, 2)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(43))
    }

    func testNotificationWithoutSnapshotAdvancesLastUpdate() {
        let core = LiveRideSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: telemetryReading(1_234),
            voltage: telemetryReading(117_000),
            batteryLevelEstimated: telemetryReading(77)
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

    func testDisconnectAndScanClearsRideStateAndReturnsPickerToScanning() {
        let core = LiveRideSessionCore()
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NOSFET Aero",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(speed: telemetryReading(1_234))
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        core.disconnectAndScan()

        XCTAssertEqual(core.phase, .scanning(model: .aero))
        XCTAssertEqual(core.displayState, LiveRideDisplayState())
        XCTAssertFalse(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["NOSFET Aero"])
    }

    func testRideStateCarriesPhaseAndTelemetrySnapshot() {
        let displayState = LiveRideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            telemetry: TelemetrySnapshot(speed: telemetryReading(1_234)),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )
        let rideState = EucRideScreenState(phase: .subscribing, displayState: displayState)

        XCTAssertEqual(rideState.phaseText, "Subscribing...")
        XCTAssertEqual(rideState.speedText, "2.8")
        XCTAssertEqual(rideState.speedUnit, "mph")
        XCTAssertEqual(rideState.telemetry?.speed?.value, 1_234)
    }

    func testDisplayStateProvidesDebugRowsForLiveValidation() {
        let displayState = LiveRideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )

        XCTAssertEqual(
            displayState.debugRows,
            [
                LiveRideDebugRow(label: "Notifications", value: "7"),
                LiveRideDebugRow(label: "Last update", value: "9876 ms"),
            ]
        )
    }

    func testTimeoutDiagnosticIdentifiesNoCandidateScanBlocker() {
        let core = LiveRideSessionCore()
        core.disconnectAndScan()

        XCTAssertEqual(
            core.timeoutDiagnosticRecords(timeoutSeconds: 5),
            [
                "timeout_seconds=5",
                "phase=scanning(model: CutoutMobile.ElectricUnicycleModel.aero)",
                "candidate_count=0",
                "selected_model=Aero",
                "blocker=no_candidate",
            ]
        )
    }

    func testTimeoutDiagnosticClassifiesConnectedTelemetryAndParsedNoSpeedBlockers() {
        let core = LiveRideSessionCore()
        core.applyLifecycleStep(.connecting(model: .aero, platformIdentifier: "peripheral-1"))
        core.applyLifecycleStep(.subscribing(["FFE1"]))

        XCTAssertEqual(
            core.timeoutDiagnosticRecords(timeoutSeconds: 3),
            [
                "timeout_seconds=3",
                "phase=subscribing",
                "candidate_count=0",
                "selected_model=Aero",
                "blocker=connected_no_telemetry",
            ]
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(speed: nil)
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(
            core.timeoutDiagnosticRecords(timeoutSeconds: 3).last,
            "blocker=parsed_no_speed"
        )
    }
}

private func telemetryReading(_ value: Int32) -> TelemetryReading<Int32> {
    TelemetryReading(
        value: value,
        source: .reported,
        quality: .known,
        verification: .sourceVerified
    )
}

private func telemetryReading(_ value: UInt8) -> TelemetryReading<UInt8> {
    TelemetryReading(
        value: value,
        source: .reported,
        quality: .known,
        verification: .sourceVerified
    )
}
