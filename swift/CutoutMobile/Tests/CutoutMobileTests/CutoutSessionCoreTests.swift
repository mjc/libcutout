import XCTest
@testable import CutoutMobile

final class CutoutSessionCoreTests: XCTestCase {
    func testObservedAdvertisementsUpdatePickerScanState() {
        let core = CutoutSessionCore()
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
        let core = CutoutSessionCore()

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
        let core = CutoutSessionCore()

        XCTAssertFalse(core.pair(platformIdentifier: "ios-local-missing"))
    }

    func testObservedAdvertisementsReplaceDuplicatePeripheralRows() {
        let core = CutoutSessionCore()

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

    func testApplyNotificationStepMarksLiveAndUpdatesDisplayState() {
        let core = CutoutSessionCore()
        let snapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            operatingState: .riding,
            voltage: voltageValue(117_000),
            powerFlow: .negativeUnknown,
            batteryLevelEstimated: batteryLevelValue(77)
        )
        let step = CoreBluetoothSessionStep(operations: [], snapshot: snapshot)
        let receivedAt = MonotonicMilliseconds(42)

        core.applyNotificationStep(step, receivedAt: receivedAt)

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(
            EucRideScreenState(phase: core.phase, displayState: core.displayState).operatingState,
            .riding
        )
        XCTAssertEqual(core.displayState.telemetry?.speed, Speed(value: 1_234))
        XCTAssertEqual(core.displayState.telemetry?.powerFlow, .negativeUnknown)
        XCTAssertEqual(core.displayState.notificationCount, 1)
        XCTAssertEqual(core.displayState.lastUpdate, receivedAt)
    }

    func testSpeedObservationRemainsStickyAcrossTelemetryWithoutSpeed() {
        let core = CutoutSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            voltage: voltageValue(117_000),
            batteryLevelEstimated: batteryLevelValue(77)
        )
        let batteryOnlySnapshot = TelemetrySnapshot(
            voltage: voltageValue(116_500),
            batteryLevelEstimated: batteryLevelValue(76)
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
        XCTAssertEqual(core.displayState.telemetry?.voltage, Voltage(value: 116_500))
        XCTAssertEqual(core.displayState.notificationCount, 2)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(43))
    }

    func testNotificationWithoutSnapshotAdvancesLastUpdate() {
        let core = CutoutSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            voltage: voltageValue(117_000),
            batteryLevelEstimated: batteryLevelValue(77)
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

    func testSettingsReadbackUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 42, value: 1_234),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ], availability: .available)
        var observedReadbacks: [SettingsReadback?] = []
        core.onSettingsReadbackChange = { observedReadbacks.append($0) }

        let action = SessionAction(
            kind: .settingsReadback,
            channel: Data(),
            bytes: Data(),
            settingsReadback: readback
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.settingsReadback, readback)
        XCTAssertEqual(core.settingsReadback?.availability, .available)
        XCTAssertEqual(observedReadbacks, [readback])

        core.disconnectAndScan()

        XCTAssertNil(core.settingsReadback)
        XCTAssertEqual(observedReadbacks, [readback, nil])
    }

    func testSettingsReadbackProjectsKnownVeteranGarageSettings() {
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x0005, value: 116),
                source: .reported,
                quality: .known,
                verification: .sourceAndHardwareVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x0006, value: 420),
                source: .reported,
                quality: .known,
                verification: .sourceAndHardwareVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x001e, value: 1_920),
                source: .reported,
                quality: .known,
                verification: .sourceAndHardwareVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x9999, value: 123),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ])

        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .available(Speed(value: 3_222)),
                tiltback: .available(Speed(value: 11_666)),
                pedalMode: .available(PedalMode.rawMode(1_920))
            )
        )
    }

    func testSettingsReadbackProjectsKnownBegodeGarageSettings() {
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x040a, value: 50),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x0406, value: 0),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ])

        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .unavailable,
                tiltback: .available(Speed(value: 13_888)),
                pedalMode: .unavailable
            )
        )
    }

    func testFaultHistoryReadbackUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let readback = FaultHistoryReadback.faultSince(
            FaultHistoryEntry(
                code: FaultCode.unknown(id: 0x0040, value: 1),
                source: .reported,
                quality: .known,
                verification: .hardwareVerified
            ),
            sinceDistance: Distance(value: 61_456_941)
        )
        var observedReadbacks: [FaultHistoryReadback?] = []
        core.onFaultHistoryReadbackChange = { observedReadbacks.append($0) }

        let action = SessionAction(
            kind: .faultHistoryReadback,
            channel: Data(),
            bytes: Data(),
            faultHistoryReadback: readback
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.faultHistoryReadback, readback)
        XCTAssertEqual(core.faultHistoryReadback?.availability, .available)
        XCTAssertEqual(observedReadbacks, [readback])

        core.disconnectAndScan()

        XCTAssertNil(core.faultHistoryReadback)
        XCTAssertEqual(observedReadbacks, [readback, nil])
    }

    func testFaultHistoryReadbackConstructorsKeepNoFaultEvidenceExplicit() {
        let distance = Distance(value: 61_456_941)
        let noFault = FaultHistoryReadback.noFaultSince(distance)
        let unavailable = FaultHistoryReadback.unavailable()
        let unsupported = FaultHistoryReadback.unsupported()

        XCTAssertEqual(noFault.availability, .available)
        XCTAssertNil(noFault.lastFault)
        XCTAssertEqual(noFault.sinceDistance, distance)
        XCTAssertEqual(unavailable.availability, .unavailable)
        XCTAssertNil(unavailable.lastFault)
        XCTAssertNil(unavailable.sinceDistance)
        XCTAssertEqual(unsupported.availability, .unsupported)
        XCTAssertNil(unsupported.lastFault)
        XCTAssertNil(unsupported.sinceDistance)
    }

    func testBmsSnapshotUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "unknown BMS topology",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            ),
            energyPercent: BatteryLevel(value: 72),
            voltage: Voltage(value: 81_600),
            current: BatteryCurrent(value: -1_250),
            highestTemperature: Temperature(value: 37_800)
        )
        var observedSnapshots: [BmsSnapshot?] = []
        core.onBmsSnapshotChange = { observedSnapshots.append($0) }

        let action = SessionAction(
            kind: .bmsSnapshot,
            channel: Data(),
            bytes: Data(),
            bmsSnapshot: snapshot
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.bmsSnapshot, snapshot)
        XCTAssertEqual(core.bmsSnapshot?.topology.confidence, .unverified)
        XCTAssertEqual(observedSnapshots, [snapshot])

        core.disconnectAndScan()

        XCTAssertNil(core.bmsSnapshot)
        XCTAssertEqual(observedSnapshots, [snapshot, nil])
    }

    func testDisconnectAndScanClearsRideStateAndReturnsPickerToScanning() {
        let core = CutoutSessionCore()
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
                snapshot: TelemetrySnapshot(speed: speedValue(1_234))
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        core.disconnectAndScan()

        XCTAssertEqual(core.phase, .scanning(model: .aero))
        XCTAssertEqual(core.displayState, RideDisplayState())
        XCTAssertFalse(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["NOSFET Aero"])
    }

    func testRideStateCarriesPhaseAndTelemetrySnapshot() {
        let displayState = RideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            telemetry: TelemetrySnapshot(speed: speedValue(1_234), operatingState: .riding),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )
        let rideState = EucRideScreenState(phase: .subscribing, displayState: displayState)

        XCTAssertEqual(rideState.phaseText, "Subscribing...")
        XCTAssertEqual(rideState.speedText, "2.8")
        XCTAssertEqual(rideState.speedUnit, "mph")
        XCTAssertEqual(rideState.operatingState, .riding)
        XCTAssertEqual(rideState.telemetry?.speed, Speed(value: 1_234))
    }

    func testDisplayStateProvidesDebugRowsForLiveValidation() {
        let displayState = RideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )

        XCTAssertEqual(
            displayState.debugRows,
            [
                SessionDebugRow(label: "Notifications", value: "7"),
                SessionDebugRow(label: "Last update", value: "9876 ms"),
            ]
        )
    }
}

private func speedValue(_ value: Int32) -> Speed {
    Speed(value: value)
}

private func voltageValue(_ value: Int32) -> Voltage {
    Voltage(value: value)
}

private func batteryLevelValue(_ value: UInt8) -> BatteryLevel {
    BatteryLevel(value: value)
}
