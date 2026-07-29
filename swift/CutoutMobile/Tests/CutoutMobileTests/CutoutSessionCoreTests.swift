import XCTest
import CutoutMobileFFI
#if canImport(CoreBluetooth)
import CoreBluetooth
#endif
@testable import CutoutMobile

final class CutoutSessionCoreTests: XCTestCase {
    func testMonotonicClockUsesItsInjectedUptimeSource() {
        var now = MonotonicMilliseconds(100)
        let clock = MonotonicClock(now: { now })

        XCTAssertEqual(clock.now(), MonotonicMilliseconds(100))

        now = MonotonicMilliseconds(250)
        XCTAssertEqual(clock.now(), MonotonicMilliseconds(250))
    }

    func testConnectionReconnectPolicyBoundsJitteredBackoff() {
        XCTAssertEqual(ConnectionReconnectPolicy.delayMilliseconds(attempt: 1, jitter: 0), 200)
        XCTAssertEqual(ConnectionReconnectPolicy.delayMilliseconds(attempt: 2, jitter: 0.5), 500)
        XCTAssertEqual(ConnectionReconnectPolicy.delayMilliseconds(attempt: 3, jitter: 1), 1_200)
        XCTAssertNil(ConnectionReconnectPolicy.delayMilliseconds(attempt: 4, jitter: 0.5))
    }

    func testReconnectSchedulerCancelsSupersededAndExplicitRetries() {
        let scheduler = RecordingReconnectScheduler()
        let reconnects = ConnectionReconnectController(scheduler: scheduler)
        var completed = [String]()

        XCTAssertEqual(
            reconnects.schedule(jitter: 0) { completed.append("first") },
            ConnectionReconnectSchedule(attempt: 1, delayMilliseconds: 200)
        )
        XCTAssertEqual(
            reconnects.schedule(jitter: 0.5) { completed.append("second") },
            ConnectionReconnectSchedule(attempt: 2, delayMilliseconds: 500)
        )

        scheduler.runAll()
        XCTAssertEqual(completed, ["second"])

        XCTAssertEqual(
            reconnects.schedule(jitter: 1) { completed.append("cancelled") },
            ConnectionReconnectSchedule(attempt: 3, delayMilliseconds: 1_200)
        )
        reconnects.cancel()
        scheduler.runAll()

        XCTAssertEqual(completed, ["second"])
        XCTAssertEqual(reconnects.attempt, 0)
    }

    func testReconnectExhaustionCancelsTheLastPendingRetry() {
        let scheduler = RecordingReconnectScheduler()
        let reconnects = ConnectionReconnectController(scheduler: scheduler)
        var completed = [String]()

        XCTAssertNotNil(reconnects.schedule(jitter: 0) { completed.append("first") })
        XCTAssertNotNil(reconnects.schedule(jitter: 0) { completed.append("second") })
        XCTAssertNotNil(reconnects.schedule(jitter: 0) { completed.append("third") })
        XCTAssertNil(reconnects.schedule(jitter: 0) { completed.append("exhausted") })

        scheduler.runAll()

        XCTAssertTrue(completed.isEmpty)
        XCTAssertEqual(reconnects.attempt, ConnectionReconnectPolicy.maximumAttempts + 1)
    }

    func testNordicNotificationUUIDsRemainFullWidthForPevcap() {
        let service = CBUUID(string: "6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
        let notify = CBUUID(string: "6E400003-B5A3-F393-E0A9-E50E24DCCA9E")

        XCTAssertEqual(BluetoothUuid(coreBluetoothUuid: service)?.bytes.count, 16)
        XCTAssertEqual(BluetoothUuid(coreBluetoothUuid: notify)?.bytes.count, 16)
        XCTAssertEqual(
            BluetoothUuid(coreBluetoothUuid: service)?.bytes,
            BluetoothUuid(Data([
                0x6e, 0x40, 0x00, 0x01, 0xb5, 0xa3, 0xf3, 0x93,
                0xe0, 0xa9, 0xe5, 0x0e, 0x24, 0xdc, 0xca, 0x9e,
            ]))?.bytes
        )
    }

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
        XCTAssertEqual(core.scanState.rows.map(\.connectionRoute), [.electricUnicycle, .vescOnewheel])
        XCTAssertEqual(core.scanState.sections.supported.map(\.title), ["NOSFET Aero", "Little FOCer"])
        XCTAssertTrue(core.scanState.sections.unsupported.isEmpty)
        XCTAssertEqual(observedStates.count, 2)
    }

    func testUnnamedNordicUartAdvertisementRoutesAsVesc() {
        let core = CutoutSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc-unnamed"),
                localName: nil,
                advertisedServiceUuids: [.vescNordicUartService]
            )
        )

        XCTAssertEqual(core.scanState.rows.map(\.title), ["VESC device"])
        XCTAssertEqual(core.scanState.rows.first?.connectionRoute, .vescOnewheel)
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

    func testScriptedSessionUsesTheCorePublicationPath() {
        let live = expectation(description: "scripted session reaches live")
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                connectionDelayMilliseconds: 0
            )
        )
        core.onPhaseChange = { phase in
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        wait(for: [live], timeout: 1)
        XCTAssertEqual(core.phase, .live)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 8_000)
    }

    func testExplicitDisconnectCancelsTheScriptedLateLiveCallback() {
        let live = expectation(description: "late scripted callback is ignored")
        live.isInverted = true
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                connectionDelayMilliseconds: 50
            )
        )
        core.onPhaseChange = { phase in
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        core.disconnectAndScan()

        wait(for: [live], timeout: 0.2)
        XCTAssertEqual(core.phase, .scanning)
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertNil(core.displayState.speed.millimetersPerSecond)
    }

    func testScriptedSessionPublishesReconnectAndReturnsLive() {
        let retry = expectation(description: "scripted session schedules reconnect")
        let live = expectation(description: "scripted session returns live")
        live.expectedFulfillmentCount = 2
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                reconnectsAfterFirstLive: true,
                reconnectDelayMilliseconds: 0,
                connectionDelayMilliseconds: 0
            )
        )
        core.onPhaseChange = { phase in
            if phase == .live {
                live.fulfill()
            }
        }
        core.onReconnectScheduled = { _ in retry.fulfill() }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        wait(for: [retry, live], timeout: 3)
        XCTAssertEqual(core.phase, .live)
    }

    func testRecordOnlyMissingCandidateReturnsFalse() {
        let core = CutoutSessionCore()

        XCTAssertFalse(core.recordOnly(platformIdentifier: "ios-local-missing", note: "unknown wheel"))
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

    func testApplyNotificationStepPublishesDisplayStateOnMainThread() {
        nonisolated(unsafe) let core = CutoutSessionCore()
        let published = expectation(description: "display state published")
        core.onDisplayStateChange = { _ in
            XCTAssertTrue(Thread.isMainThread)
            published.fulfill()
        }

        DispatchQueue.global().async {
            core.applyNotificationStep(
                CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot()),
                receivedAt: MonotonicMilliseconds(42)
            )
        }

        wait(for: [published], timeout: 1.0)
    }

    func testDisplayPublicationThrottleUsesMonotonicTime() {
        let clock = TestMonotonicClock(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(clock: MonotonicClock(now: { clock.now }))
        var publicationCount = 0
        core.onDisplayStateChange = { _ in publicationCount += 1 }

        let step = CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot())
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_000))

        clock.now = MonotonicMilliseconds(1_200)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_200))

        clock.now = MonotonicMilliseconds(1_333)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_333))

        XCTAssertEqual(publicationCount, 2)
    }

    func testVescRideSnapshotKeepsRideCriticalFieldsTyped() {
        let snapshot = VescRideSnapshot(
            title: "Fungineers X7",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            warning: .pushbackSoon,
            boardSpeed: speedValue(19_000),
            dutyCycle: dutyCycle(820),
            dutyHeadroom: batteryLevelValue(18),
            batteryCurrent: batteryCurrentValue(38_000),
            powerFlow: .discharge,
            motorCurrent: phaseCurrentValue(71_000),
            boardAngle: angleValue(-18),
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(snapshot.vehicleKind, .float)
        XCTAssertEqual(snapshot.subProtocol, .refloat)
        XCTAssertEqual(snapshot.controllerState, .unknown)
        XCTAssertEqual(snapshot.warning, .pushbackSoon)
        XCTAssertEqual(snapshot.boardSpeed, speedValue(19_000))
        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(820))
        XCTAssertEqual(snapshot.dutyHeadroom, batteryLevelValue(18))
        XCTAssertEqual(snapshot.batteryCurrent, batteryCurrentValue(38_000))
        XCTAssertEqual(snapshot.powerFlow, .discharge)
        XCTAssertEqual(snapshot.motorCurrent, phaseCurrentValue(71_000))
        XCTAssertEqual(snapshot.boardAngle, angleValue(-18))
        XCTAssertEqual(snapshot.controllerTemperature, temperatureValue(54_000))
        XCTAssertEqual(snapshot.motorTemperature, temperatureValue(49_000))
    }

    func testVescRideSnapshotOwnsBatteryReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            batteryLevelReported: batteryLevelValue(72),
            batteryCurrent: batteryCurrentValue(38_000)
        )

        XCTAssertEqual(
            snapshot.batteryReadback,
            .reported(level: "72", current: "38.0")
        )
    }

    func testVescRideSnapshotOwnsBoardAngleReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            boardAngle: angleValue(-18_000),
            balanceAngle: angleValue(500)
        )

        XCTAssertEqual(
            snapshot.boardAngleReadback,
            .available(orientation: .noseDown, balanceAngle: "0.5")
        )
    }

    func testRideHeroReadoutOwnsVescSpeedFreshnessAndSeverity() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            warning: .pushbackSoon,
            boardSpeed: speedValue(19_000),
            lastUpdate: MonotonicMilliseconds(1_000)
        )

        XCTAssertEqual(
            RideHeroReadout.vesc(
                snapshot: snapshot,
                now: MonotonicMilliseconds(4_000)
            ),
            .available(
                value: "42.5",
                unit: "mph",
                freshness: .stale,
                severity: .caution
            )
        )
    }

    func testTelemetryThermalReadbackOwnsSensorFormatting() {
        let snapshot = TelemetrySnapshot(
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(
            snapshot.thermalReadback,
            .controllerMotor(controller: "54", motor: "49")
        )
    }

    func testVescVehicleKindDoesNotImplySubProtocol() {
        let snapshot = VescRideSnapshot(
            title: "VESC Bike",
            vehicleKind: .bike,
            subProtocol: .generic,
            controllerState: .unknown
        )

        XCTAssertEqual(snapshot.vehicleKind, .bike)
        XCTAssertEqual(snapshot.subProtocol, .generic)

        let bike = VescRideSnapshot(
            title: "VESC Bike",
            vehicleKind: .bike,
            subProtocol: .bike,
            controllerState: .unknown
        )
        XCTAssertEqual(bike.vehicleKind, .bike)
        XCTAssertEqual(bike.subProtocol, .bike)

        let eskate = VescRideSnapshot(
            title: "VESC Skateboard",
            vehicleKind: .skateboard,
            subProtocol: .eskate,
            controllerState: .unknown
        )
        XCTAssertEqual(eskate.vehicleKind, .skateboard)
        XCTAssertEqual(eskate.subProtocol, .eskate)
    }

    func testVescRideSnapshotProjectsLiveDisplayTelemetryWithoutInventingSpeed() throws {
        let telemetry = TelemetrySnapshot(
            voltage: voltageValue(75_400),
            batteryCurrent: batteryCurrentValue(38_000),
            motorCurrent: phaseCurrentValue(71_000),
            powerFlow: .discharge,
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: "Little FOCer BT"))

        XCTAssertEqual(snapshot.title, "Little FOCer BT")
        XCTAssertEqual(snapshot.vehicleKind, .float)
        XCTAssertEqual(snapshot.subProtocol, .generic)
        XCTAssertEqual(snapshot.controllerState, .unknown)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertEqual(snapshot.batteryVoltage, voltageValue(75_400))
        XCTAssertEqual(snapshot.batteryCurrent, batteryCurrentValue(38_000))
        XCTAssertEqual(snapshot.powerFlow, .discharge)
        XCTAssertEqual(snapshot.motorCurrent, phaseCurrentValue(71_000))
        XCTAssertEqual(snapshot.controllerTemperature, temperatureValue(54_000))
        XCTAssertEqual(snapshot.motorTemperature, temperatureValue(49_000))
    }

func testVescRideSnapshotProjectsBatteryLevelAndUpdateTime() throws {
        let telemetry = TelemetrySnapshot(
            operatingState: .parked,
            voltage: voltageValue(61_000),
            batteryLevelReported: batteryLevelValue(72),
            batteryLevelEstimated: batteryLevelValue(70)
        )
        let displayState = RideDisplayState(
            telemetry: telemetry,
            notificationCount: 1,
            lastUpdate: MonotonicMilliseconds(900)
        )

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.batteryLevelReported, batteryLevelValue(72))
        XCTAssertEqual(snapshot.batteryLevelEstimated, batteryLevelValue(70))
        XCTAssertEqual(snapshot.lastUpdate, MonotonicMilliseconds(900))
        XCTAssertEqual(
            snapshot.updateAge(
                at: MonotonicMilliseconds(1_000),
                staleAfter: MonotonicMilliseconds(250)
            ),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(100), freshness: .fresh)
        )
        XCTAssertEqual(
            snapshot.updateAge(
                at: MonotonicMilliseconds(1_300),
                staleAfter: MonotonicMilliseconds(250)
            ),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(400), freshness: .stale)
        )
    }

    func testVescRideSnapshotDerivesDutyHeadroomFromLiveDutyCycle() throws {
        let balancedTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(0))
        let idleNoiseTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(10))
        let loadedTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))

        let balanced = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: balancedTelemetry, notificationCount: 1),
            title: nil
        ))
        let idleNoise = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: idleNoiseTelemetry, notificationCount: 1),
            title: nil
        ))
        let loaded = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: loadedTelemetry, notificationCount: 1),
            title: nil
        ))

        XCTAssertEqual(balanced.dutyCycle, dutyCycle(0))
        XCTAssertEqual(balanced.dutyHeadroom, batteryLevelValue(100))
        XCTAssertEqual(idleNoise.dutyCycle, dutyCycle(10))
        XCTAssertEqual(idleNoise.dutyHeadroom, batteryLevelValue(100))
        XCTAssertEqual(loaded.dutyCycle, dutyCycle(230))
        XCTAssertEqual(loaded.dutyHeadroom, batteryLevelValue(77))
        XCTAssertEqual(loaded.dutyHeadroomMetricValue, .available(display: "77", accessibility: "77"))
        XCTAssertEqual(
            loaded.dutyHeadroomProgressMetricValue,
            .available(display: "77%", accessibility: "77%")
        )
        XCTAssertEqual(try XCTUnwrap(loaded.dutyHeadroomProgress), 0.77, accuracy: 0.001)
    }

    func testVescRideSnapshotMarksParkedHeadroomNotApplicableWithoutProgress() throws {
        let telemetry = TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(10))

        let snapshot = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: telemetry, notificationCount: 1),
            title: nil
        ))

        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(10))
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertEqual(snapshot.dutyHeadroomApplicability, .notApplicable)
        XCTAssertEqual(
            snapshot.dutyHeadroomMetricValue,
            .status(display: "Not applicable", accessibility: "Not applicable")
        )
        XCTAssertEqual(
            snapshot.dutyHeadroomProgressMetricValue,
            .status(display: "Not applicable", accessibility: "Not applicable")
        )
        XCTAssertNil(snapshot.dutyHeadroomProgress)
    }

    func testVescRideSnapshotKeepsMissingDutyHeadroomUnavailable() throws {
        let telemetry = TelemetrySnapshot(operatingState: .parked, voltage: voltageValue(62_800))

        let snapshot = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: telemetry, notificationCount: 1),
            title: nil
        ))

        XCTAssertNil(snapshot.dutyCycle)
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertEqual(snapshot.dutyHeadroomApplicability, .unavailable)
        XCTAssertEqual(snapshot.dutyHeadroomMetricValue, .unavailable)
        XCTAssertEqual(snapshot.dutyHeadroomProgressMetricValue, .unavailable)
        XCTAssertNil(snapshot.dutyHeadroomProgress)
    }

    func testVescRideSnapshotProjectsFootpadFromSharedTelemetry() throws {
        let footpad = FootpadTelemetry(state: 3, adc1Milliunits: 1_250, adc2Milliunits: 875)
        let telemetry = TelemetrySnapshot(footpad: footpad)
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.footpad, footpad)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertNil(snapshot.boardAngle)
    }

    func testFootpadTelemetryExposesTypedAdcValues() {
        let footpad = FootpadTelemetry(state: 3, adc1Milliunits: 1_250, adc2Milliunits: nil)

        XCTAssertEqual(
            footpad.adc1MetricValue,
            .available(display: "1.25", accessibility: "1.25, available")
        )
        XCTAssertEqual(footpad.adc2MetricValue, .unavailable)
        XCTAssertEqual(footpad.stateDisplayText, "state 3")
        XCTAssertEqual(
            footpad.summaryText,
            "footpad state 3 · adc1 left 1.25 · adc2 right unavailable"
        )
    }

    func testFootpadTelemetryUsesTypedContactStateForDisplayAndAccessibility() {
        let footpad = FootpadTelemetry(
            state: 3,
            contactState: .both,
            adc1Milliunits: 1_250,
            adc2Milliunits: 875
        )

        XCTAssertEqual(footpad.stateDisplayText, "both pressed")
        XCTAssertEqual(
            footpad.accessibilityValue,
            "left / adc1, 1.25, available, right / adc2, 0.88, available, both pressed"
        )
        XCTAssertEqual(
            footpad.summaryText,
            "footpad both pressed · adc1 left 1.25 · adc2 right 0.88"
        )
    }

    func testFootpadTelemetryKeepsZeroAdcAvailable() {
        let footpad = FootpadTelemetry(state: 0, adc1Milliunits: 0, adc2Milliunits: 0)

        XCTAssertEqual(
            footpad.adc1MetricValue,
            .available(display: "0.00", accessibility: "0.00, available")
        )
        XCTAssertEqual(
            footpad.adc2MetricValue,
            .available(display: "0.00", accessibility: "0.00, available")
        )
    }

    func testFootpadPresentationCopyResolvesFromThePackageCatalog() {
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.state", value: nil, table: "Localizable"),
            "state %lld"
        )
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.accessibility.summary", value: nil, table: "Localizable"),
            "%1$@, %2$@, %3$@, %4$@, %5$@"
        )
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.title", value: nil, table: "Localizable"),
            "Footpad"
        )
    }

    func testVescRideSnapshotProjectsAngleOnlyTelemetry() throws {
        let telemetry = TelemetrySnapshot(pitch: angleValue(14_200))
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.boardAngle, angleValue(14_200))
        XCTAssertNil(snapshot.batteryVoltage)
    }

    func testVescRideSnapshotDoesNotUseUnverifiedFactsForLiveDefaults() throws {
        let telemetry = TelemetrySnapshot(voltage: voltageValue(62_800))
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.title, VescRideSnapshot.defaultTitle)
        XCTAssertEqual(snapshot.subProtocol, .generic)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertNil(snapshot.boardAngle)
        XCTAssertNil(snapshot.controllerTemperature)
        XCTAssertNil(snapshot.motorTemperature)
        XCTAssertNotEqual(snapshot.title, "Fungineers X7")
    }

    func testVescDebugSnapshotKeepsGuardrailAndReadOnlyStateTyped() {
        let snapshot = VescDebugSnapshot(
            profileTitle: "Profile: Street stable",
            transportDetail: "VESC Express · FW 6.x · UART bridge",
            dutyCycle: dutyCycle(820),
            maxSeenDutyCycle: dutyCycle(870),
            packVoltage: voltageValue(75_400),
            batteryCurrentLimit: batteryCurrentValue(45_000),
            motorCurrentLimit: phaseCurrentValue(90_000),
            lastFault: "FAULT_CODE_NONE",
            inputApp: "ADC + balance",
            canStatus: "single controller",
            logging: "local CSV armed",
            writeGuardrail: .policyRefusal
        )

        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(820))
        XCTAssertEqual(snapshot.maxSeenDutyCycle, dutyCycle(870))
        XCTAssertEqual(snapshot.packVoltage, voltageValue(75_400))
        XCTAssertEqual(snapshot.batteryCurrentLimit, batteryCurrentValue(45_000))
        XCTAssertEqual(snapshot.motorCurrentLimit, phaseCurrentValue(90_000))
        XCTAssertEqual(snapshot.writeGuardrail, .policyRefusal)
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

    func testVescOnewheelCoreBluetoothSessionSubscribesAndRequestsTelemetryOnLinkUp() throws {
        let session = CoreBluetoothSession.vescOnewheel()
        let runner = CoreBluetoothSessionRunner(
            session: session,
            writeLimit: TransportWriteLimitBytes(20)
        )

        let step = try runner.handle(.linkUp(at: MonotonicMilliseconds(7)))

        assertVescTelemetryRequests(step.operations, includesSubscribe: true)
        XCTAssertNil(step.snapshot?.speed)
    }

    func testVescOnewheelCoreBluetoothSessionRequestsTelemetryWithReadOnlyCommand() throws {
        let session = CoreBluetoothSession.vescOnewheel()
        let runner = CoreBluetoothSessionRunner(
            session: session,
            writeLimit: TransportWriteLimitBytes(20)
        )

        let step = try runner.handle(.command(.requestTelemetry, at: MonotonicMilliseconds(11)))

        assertVescTelemetryRequests(step.operations, includesSubscribe: false)
        XCTAssertNil(step.snapshot?.speed)
    }

    func testVescLiveOwnerWritesRequestsBeforeSubscribing() throws {
        let sink = RecordingOperationSink()
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))

        XCTAssertEqual(sink.events, [.write, .write, .write, .subscribe])
    }

    func testVescLiveOwnerRetriesTelemetryAfterLinkUp() throws {
        let sink = RecordingOperationSink()
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            retryDelay: .milliseconds(10)
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))
        XCTAssertEqual(sink.writes.count, 3)
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )
        XCTAssertEqual(sink.writes.count, 3)

        let retryExpectation = expectation(description: "vesc telemetry retries")
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(45)) {
            retryExpectation.fulfill()
        }
        wait(for: [retryExpectation], timeout: 1.0)

        XCTAssertGreaterThanOrEqual(sink.writes.count, 9)
        XCTAssertEqual(sink.writes.count % 3, 0)
        XCTAssertEqual(sink.writes.prefix(3), sink.writes.suffix(3))
    }

    func testVescLiveOwnerBoundsTelemetryRetries() throws {
        let sink = RecordingOperationSink()
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            maximumRetryAttempts: 2,
            retryDelay: .milliseconds(10)
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )

        let retriesFinish = expectation(description: "bounded VESC telemetry retries")
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(50)) {
            retriesFinish.fulfill()
        }
        wait(for: [retriesFinish], timeout: 1.0)

        XCTAssertEqual(sink.writes.count, 9)
    }


    func testVescLiveOwnerRetriesAfterNonRealtimeNotification() throws {
        let sink = RecordingOperationSink()
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            retryDelay: .milliseconds(10)
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )
        _ = try owner.handleNotification(
            bytes: Data([0x01]),
            channel: .bluetooth16(0xffff),
            at: MonotonicMilliseconds(2)
        )

        let retryExpectation = expectation(description: "bounded VESC telemetry retries after generic notification")
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(45)) {
            retryExpectation.fulfill()
        }
        wait(for: [retryExpectation], timeout: 1.0)

        XCTAssertGreaterThanOrEqual(sink.writes.count, 9)
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

        let action = SessionAction.withSettingsReadback(readback)
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

    func testNonAvailableSettingsReadbackDoesNotCarryRawEntries() {
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 42, value: 1_234),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ], availability: .unsupported)

        XCTAssertEqual(readback.availability, .unsupported)
        XCTAssertTrue(readback.entries.isEmpty)
        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .unsupported,
                tiltback: .unsupported,
                pedalMode: .unsupported
            )
        )
    }

    func testNonAvailableSettingsReadbackDoesNotCarryGarageProjection() {
        let readback = SettingsReadback(
            entries: [],
            availability: .unsupported,
            eucGarageSettings: EucGarageSettingsSnapshot(
                beepMargin: .available(Speed(value: 3_222)),
                tiltback: .available(Speed(value: 11_666)),
                pedalMode: .available(PedalMode.rawMode(1_920))
            )
        )

        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .unsupported,
                tiltback: .unsupported,
                pedalMode: .unsupported
            )
        )
    }

    func testSettingsReadbackCarriesProjectedVeteranGarageSettings() {
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
        ], eucGarageSettings: EucGarageSettingsSnapshot(
            beepMargin: .available(Speed(value: 3_222)),
            tiltback: .available(Speed(value: 11_666)),
            pedalMode: .available(PedalMode.rawMode(1_920))
        ))

        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .available(Speed(value: 3_222)),
                tiltback: .available(Speed(value: 11_666)),
                pedalMode: .available(PedalMode.rawMode(1_920))
            )
        )
    }

    func testSettingsReadbackCarriesProjectedBegodeGarageSettings() {
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
        ], eucGarageSettings: EucGarageSettingsSnapshot(
            beepMargin: .unavailable,
            tiltback: .available(Speed(value: 13_888)),
            pedalMode: .unavailable
        ))

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

        let action = SessionAction.withFaultHistoryReadback(readback)
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

    func testFaultHistoryGeneratedReadbackStripsPayloadWhenUnavailable() {
        let distance = DistanceReading(
            value: Distance(value: 61_456_941),
            source: .reported,
            quality: .known,
            verification: .sourceVerified
        )
        let unavailable = FaultHistoryReadback(
            MobileFaultHistoryReadbackDto(
                availability: .unavailable,
                lastFault: nil,
                sinceDistance: distance
            )
        )
        let unsupported = FaultHistoryReadback(
            MobileFaultHistoryReadbackDto(
                availability: .unsupported,
                lastFault: nil,
                sinceDistance: distance
            )
        )

        XCTAssertEqual(unavailable, FaultHistoryReadback.unavailable())
        XCTAssertEqual(unsupported, FaultHistoryReadback.unsupported())
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

        let action = SessionAction.withBmsSnapshot(snapshot)
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

    func testBmsSnapshotAggregatesCollectedPagesForPackOverview() {
        let core = CutoutSessionCore()
        let metadataPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 2,
            pageKind: "metadata",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            current: BatteryCurrent(value: 0)
        )
        let cellPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 3,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            cellDelta: VoltageDelta(value: 12),
            lowestGroupIndex: 1,
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090), alertLevel: .warning),
                BmsGroupSnapshot(index: 2, voltage: Voltage(value: 4_102)),
            ]
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(metadataPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(cellPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertNil(core.bmsSnapshot?.pageSelector)
        XCTAssertNil(core.bmsSnapshot?.pageKind)
        XCTAssertEqual(core.bmsSnapshot?.topology.layoutLabel, "8 observed BMS groups")
        XCTAssertEqual(core.bmsSnapshot?.voltage, Voltage(value: 95_800))
        XCTAssertEqual(core.bmsSnapshot?.current, BatteryCurrent(value: 0))
        XCTAssertEqual(core.bmsSnapshot?.cellDelta, VoltageDelta(value: 12))
        XCTAssertEqual(core.bmsSnapshot?.groups.count, 2)
    }

    func testBmsSnapshotCollectionDoesNotPublishCursorOnlyUpdates() {
        let core = CutoutSessionCore()
        let firstPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 0,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090))
            ]
        )
        let cursorOnlyPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 1,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090))
            ]
        )
        var observedSnapshots: [BmsSnapshot?] = []
        core.onBmsSnapshotChange = { observedSnapshots.append($0) }

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(firstPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(cursorOnlyPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(observedSnapshots.count, 1)
        XCTAssertNil(core.bmsSnapshot?.pageSelector)
        XCTAssertNil(core.bmsSnapshot?.pageKind)
    }

    func testBmsSnapshotCollectionKeepsSameSelectorWithDifferentProtocolTags() {
        let core = CutoutSessionCore()
        let topology = BmsTopology(
            layoutLabel: "64 observed BMS groups",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 2,
            confidence: .unverified
        )
        let firstBank = BmsSnapshot(
            topology: topology,
            pageSelector: 0,
            pageTag: 0x02,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 0))
            ]
        )
        let secondBank = BmsSnapshot(
            topology: topology,
            pageSelector: 0,
            pageTag: 0x03,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            groups: [
                BmsGroupSnapshot(index: 33, voltage: Voltage(value: 0))
            ]
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(firstBank)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(secondBank)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(core.bmsSnapshot?.groups.map(\.index), [1, 33])
    }

    func testBmsSnapshotDoesNotReplaceObservedPackIdentityWithUnknown() {
        let core = CutoutSessionCore()
        let observedPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            voltage: Voltage(value: 95_800)
        )
        let unknownPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "unknown BMS topology",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            ),
            current: BatteryCurrent(value: 0)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(observedPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(unknownPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(core.bmsSnapshot?.topology.layoutLabel, "8 observed BMS groups")
        XCTAssertEqual(core.bmsSnapshot?.topology.bmsCount, 1)
        XCTAssertEqual(core.bmsSnapshot?.current, BatteryCurrent(value: 0))
    }

    func testProtocolIdentityCandidateUpdatesFromVeteranModelId() {
        let core = CutoutSessionCore()
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "NF2557")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "NOSFET Aero confirmed by model id 43")
        XCTAssertEqual(core.protocolIdentityCandidate?.support.electricUnicycleModel, .aero)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["NOSFET Aero confirmed by model id 43"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=NOSFET Aero confirmed by model id 43")
    }

    func testProtocolIdentityFallbackDisplayNameUsesDetectedFamily() {
        let cases: [(DeviceDetectionProtocolFamily?, String, String)] = [
            (.veteranLeaperkimNosfet, "protocol_identity.fallback.veteran_nosfet", "Veteran/NOSFET device"),
            (.begodeGotway, "protocol_identity.fallback.begode", "Begode device"),
            (.vesc, "protocol_identity.fallback.vesc", "VESC device"),
            (nil, "protocol_identity.fallback.unknown", "Detected rideable"),
        ]

        for (protocolFamily, key, expected) in cases {
            XCTAssertEqual(pevLocalizedText(key), expected)
            XCTAssertEqual(protocolIdentityFallbackDisplayName(protocolFamily: protocolFamily), pevLocalizedText(key))
        }
    }

    func testPevcapIdentityDoesNotUseProvisionalSelectedModel() {
        XCTAssertNil(captureResolvedIdentity(protocolIdentityCandidate: nil))
    }

    func testPevcapIdentityUsesProtocolConfirmedCandidate() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileDiscoveryCandidateFromVeteranProtocolIdentity(
            platformIdentifier: "ios-local-aero",
            displayName: "NF2557",
            modelId: 43
        ))

        let identity = captureResolvedIdentity(protocolIdentityCandidate: candidate)

        XCTAssertEqual(identity?.protocolFamily, .veteranLeaperkimNosfet)
        XCTAssertEqual(identity?.model?.value, "NOSFET Aero")
        XCTAssertEqual(identity?.model?.verification, .hardwareVerified)
    }

    func testPevcapAnnotationSanitizesDelimiterCharacters() {
        XCTAssertEqual(
            pevcapAnnotation(key: "device_kind", value: "foo=bar\nbaz\rqux"),
            "device_kind=foo bar baz qux"
        )
        XCTAssertEqual(
            sanitizedPevcapAnnotation("user_note=one=two\nthree"),
            "user_note=one two three"
        )
    }

    func testBegodeProbeWritesAreLabeledForDetectionCapture() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))

        XCTAssertTrue(core.records.contains("begode_probe_write=model"))
        XCTAssertTrue(core.records.contains("begode_probe_write=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_write=imu"))
    }

    func testBegodeProbeWriteDoesNotUseSkippedWriteGuard() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("N".utf8))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertTrue(core.records.contains("begode_probe_write=model"))
    }

    func testVescTelemetryRequestDoesNotUseSkippedWriteGuard() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(
            channel: .vescNordicUartWrite,
            bytes: Data([0x02, 0x01, 0x04, 0x40, 0x84, 0x03])
        )

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
    }

    func testVescRealtimeTelemetryRequestDoesNotUseReadOnlyWriteGuard() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(
            channel: .vescNordicUartWrite,
            bytes: Data([0x02, 0x01, 0x0e, 0xe1, 0xce, 0x03])
        )

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
    }

    func testFalconLinkUpPlansBegodeIdentityProbeWrites() throws {
        let runner = CoreBluetoothSessionRunner(
            session: try .electricUnicycle(model: .falcon),
            writeLimit: TransportWriteLimitBytes(23)
        )

        let step = try runner.handle(.linkUp(at: MonotonicMilliseconds(42)))

        XCTAssertEqual(
            step.operations,
            [
                .subscribe(channel: .bluetooth16(0xffe1)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("N".utf8)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("V".utf8)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("M".utf8)),
            ]
        )
    }

    func testUnrelatedWriteReachesNormalTransportValidation() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data([0x01]))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertFalse(core.records.contains("begode_probe_write=model"))
    }

    func testMultiBytePayloadStartingWithProbeByteReachesNormalTransportValidation() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("NAME".utf8))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertFalse(core.records.contains("begode_probe_write=model"))
    }

    func testBegodeProbeResponsesAreLabeledFromDetectionSession() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("GW FALCON 1.0".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("MPU6500".utf8))

        XCTAssertTrue(core.records.contains("begode_probe_response=model"))
        XCTAssertTrue(core.records.contains("begode_probe_response=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_response=imu"))
    }

    func testBegodeProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Typed Begode Falcon",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Typed Begode Falcon")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/Falcon confirmed by reported model Falcon")
        XCTAssertEqual(core.protocolIdentityCandidate?.support.electricUnicycleModel, .falcon)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/Falcon confirmed by reported model Falcon"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/Falcon confirmed by reported model Falcon")
    }

    func testBegodeFirmwareProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon-code"),
                localName: "GotWay_002441",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("GW-FALCON".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "GotWay_002441")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; code GW-FALCON")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Unresolved Begode code banner")
        )
        XCTAssertNil(core.protocolIdentityCandidate?.pickerRow.connectionRoute)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; code GW-FALCON"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/GotWay identity probe collected; code GW-FALCON")
    }

    func testBegodeImuProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon-imu"),
                localName: "GotWay_002441",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("MPU6500".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "GotWay_002441")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; imu MPU6500")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Begode model not confirmed")
        )
        XCTAssertNil(core.protocolIdentityCandidate?.pickerRow.connectionRoute)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; imu MPU6500"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/GotWay identity probe collected; imu MPU6500")
    }

    func testFragmentedBegodeFrameUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        let frame: [UInt8] = [
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ]
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-gotway"),
                localName: "Mystery Wheel",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionNotification(channel: channel, bytes: Data(Array(frame.prefix(20))))
        XCTAssertNil(core.protocolIdentityCandidate)

        core.observeDetectionNotification(channel: channel, bytes: Data(Array(frame.dropFirst(20))))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Mystery Wheel")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; model not confirmed")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Begode model not confirmed")
        )
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; model not confirmed"]
        )
        XCTAssertEqual(
            core.records.last,
            "protocol_identity=Begode/GotWay identity probe collected; model not confirmed"
        )
    }

    func testMixedProtocolFamiliesUpdateProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        let begodeFrame: [UInt8] = [
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ]
        var veteranFrame = Array(repeating: UInt8(0), count: 42)
        veteranFrame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        veteranFrame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-conflict"),
                localName: "Conflicting wheel",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionNotification(channel: channel, bytes: Data(veteranFrame))
        core.observeDetectionNotification(channel: channel, bytes: Data(begodeFrame))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Conflicting wheel")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Conflicting protocol family evidence")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .conflicting(disabledReason: "Conflicting identity evidence")
        )
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            [
                "NOSFET Aero confirmed by model id 43",
                "Conflicting protocol family evidence",
            ]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Conflicting protocol family evidence")
    }

    func testMalformedBegodeProbeResponseIsLabeledFromDetectionSession() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00]))

        XCTAssertTrue(core.records.contains("begode_probe_malformed=model"))
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testMalformedBegodeModelResponseIsLabeledAfterQueuedProbeWrites() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00]))

        XCTAssertTrue(core.records.contains("begode_probe_malformed=model"))
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testOutstandingBegodeProbeResponsesAreLabeledMissing() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.markOutstandingBegodeProbeResponsesMissing()

        XCTAssertTrue(core.records.contains("begode_probe_missing=model"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=imu"))
    }

    func testAnsweredBegodeProbeIsNotLabeledMissing() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        core.markOutstandingBegodeProbeResponsesMissing()

        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testProtocolIdentityCandidatePrefersSelectedAdvertisement() {
        let core = CutoutSessionCore()
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-selected"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-last"),
                localName: "Later scan row",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        XCTAssertFalse(core.pair(platformIdentifier: "ios-local-selected"))
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.protocolIdentityCandidate?.platformIdentifier, "ios-local-selected")
        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "NF2557")
    }

    func testDisconnectAndScanClearsProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        core.disconnectAndScan()

        XCTAssertEqual(core.protocolIdentityCandidate, nil)
        XCTAssertEqual(
            observedCandidates.map { $0?.detail },
            ["NOSFET Aero confirmed by model id 43", nil]
        )
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

        XCTAssertEqual(core.phase, .scanning)
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

    func testRideStateExposesPwmHeadroomWhileStandingOrRiding() {
        let riding = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))
            )
        )
        let standing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing, pwm: dutyCycle(230))
            )
        )

        XCTAssertEqual(riding.pwmHeadroomApplicability, .available)
        XCTAssertEqual(riding.pwmHeadroomPermille, 770)
        XCTAssertEqual(standing.pwmHeadroomApplicability, .available)
        XCTAssertEqual(standing.pwmHeadroomPermille, 770)
    }

    func testRideStateTreatsIdlePwmHeadroomAsFullHeadroom() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing, pwm: dutyCycle(10))
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .available)
        XCTAssertEqual(rideState.pwmHeadroomPermille, 1_000)
    }

    func testRideStateStatusUsesOperatingStateWhenLive() {
        let parked = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked)
            )
        )
        let riding = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding)
            )
        )
        let standing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing)
            )
        )
        let charging = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .charging)
            )
        )

        XCTAssertEqual(parked.statusText, "Parked")
        XCTAssertEqual(riding.statusText, "Riding")
        XCTAssertEqual(standing.statusText, "Standing")
        XCTAssertEqual(charging.statusText, "Charging")
    }

    func testRideStateDistinguishesEmptyLiveSnapshotFromPopulatedTelemetry() {
        let waiting = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let populated = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertEqual(waiting.telemetryAvailability, .waitingForValues)
        XCTAssertEqual(populated.telemetryAvailability, .populated)
    }

    func testRideStateCarriesTypedWarningSeverity() {
        let failed = EucRideScreenState(
            phase: .failed(.connectFailed("link dropped")),
            displayState: RideDisplayState()
        )
        let inactive = EucRideScreenState(
            phase: .scanning,
            displayState: RideDisplayState()
        )
        let waiting = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let populated = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertEqual(failed.warningState.severity, .failed)
        XCTAssertEqual(inactive.warningState.severity, .unavailable)
        XCTAssertEqual(waiting.warningState.severity, .caution)
        XCTAssertEqual(populated.warningState.severity, .normal)
        XCTAssertEqual(waiting.warningState.title, "Waiting for telemetry")
    }

    func testRideStateRecommendsReducingAccelerationForLowRidingPwmHeadroom() {
        let lowHeadroom = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(800))
            )
        )
        let healthyHeadroom = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(500))
            )
        )
        let parked = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(800))
            )
        )

        XCTAssertEqual(lowHeadroom.pwmHeadroomPermille, 200)
        XCTAssertEqual(lowHeadroom.warningState.severity, .reduceAcceleration)
        XCTAssertEqual(lowHeadroom.warningState.title, "Reduce acceleration")
        XCTAssertEqual(healthyHeadroom.warningState.severity, .normal)
        XCTAssertEqual(parked.warningState.severity, .normal)
    }

    func testRideStateTreatsMissingLiveSnapshotAsTelemetryUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState()
        )

        XCTAssertEqual(rideState.telemetryAvailability, .unavailable)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .unknown)
    }

    func testRideStateTreatsParkedPwmHeadroomAsNotApplicable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .notApplicable)
        XCTAssertNil(rideState.pwmHeadroomPermille)
    }

    func testRideStateOwnsTypedPwmHeadroomPresentation() throws {
        let available = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))
            )
        )
        let notApplicable = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )
        let unavailable = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot(operatingState: .riding))
        )

        XCTAssertEqual(
            available.pwmHeadroomMetricValue,
            .available(display: "77%", accessibility: "77%")
        )
        XCTAssertEqual(try XCTUnwrap(available.pwmHeadroomProgress), 0.77, accuracy: 0.001)
        XCTAssertEqual(
            notApplicable.pwmHeadroomMetricValue,
            .status(display: "Not applicable", accessibility: "Not applicable")
        )
        XCTAssertNil(notApplicable.pwmHeadroomProgress)
        XCTAssertEqual(unavailable.pwmHeadroomMetricValue, .unavailable)
        XCTAssertNil(unavailable.pwmHeadroomProgress)
    }

    func testRideStateTreatsMissingPwmHeadroomAsUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding)
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .unavailable)
        XCTAssertNil(rideState.pwmHeadroomPermille)
    }

    func testRideStateAccountsForVisibleFieldsInPopulatedLiveSnapshot() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                speed: SpeedReadout(millimetersPerSecond: 1_234),
                telemetry: TelemetrySnapshot(
                    speed: speedValue(1_234),
                    operatingState: .riding,
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(2_000),
                    controllerTemperature: temperatureValue(31_000),
                    pwm: dutyCycle(230),
                    batteryLevelEstimated: batteryLevelValue(80)
                )
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .status), .sessionState)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .updateAge), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .pwmHeadroom), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .sagAdjustedEnergy), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .packVoltage), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .thermal), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .warningState), .sessionState)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .voltageSag), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .limpHomeRange), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .tabs), .staticNavigation)
    }

    func testRideStateRequiresRepresentativeLiveFieldsForValidation() {
        let ready = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: speedValue(1_234),
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(2_000),
                    controllerTemperature: temperatureValue(31_000),
                    pwm: dutyCycle(230)
                )
            )
        )
        let missing = EucRideScreenState(
            phase: .subscribing,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertTrue(ready.isLiveValidationReady)
        XCTAssertEqual(ready.liveValidationMissingFields, [])
        XCTAssertFalse(missing.isLiveValidationReady)
        XCTAssertEqual(
            missing.liveValidationMissingFields,
            [.livePhase, .updateAge, .speed, .power, .pwm, .thermal]
        )
    }

    func testRideStateAccountsForRegenerationPowerOnlyWhenFlowIsRegeneration() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    operatingState: .riding,
                    voltage: voltageValue(96_700),
                    batteryCurrent: batteryCurrentValue(-800),
                    powerFlow: .regeneration
                )
            )
        )

        XCTAssertEqual(rideState.regenerationPower, powerValue(-77_360))
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .regenPower), .derivedTelemetry)
    }

    func testRideStateDoesNotAccountForUnverifiedNegativePowerAsRegeneration() {
        let unknownFlowState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(96_700),
                    batteryCurrent: batteryCurrentValue(-800),
                    powerFlow: .negativeUnknown
                )
            )
        )
        let chargingState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(-2_000),
                    powerFlow: .charging
                )
            )
        )

        XCTAssertNil(unknownFlowState.regenerationPower)
        XCTAssertEqual(unknownFlowState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
        XCTAssertNil(chargingState.regenerationPower)
        XCTAssertEqual(chargingState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
    }

    func testRideStateAccountsForVoltageSagAndLimpHomeOnlyWhenTypedValuesExist() {
        let unavailableState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(96_700))
            )
        )
        let typedState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(96_700),
                    voltageSag: VoltageDelta(value: -1_200),
                    limpHomeRange: Distance(value: 22_852_500)
                )
            )
        )

        XCTAssertNil(unavailableState.voltageSag)
        XCTAssertNil(unavailableState.limpHomeRange)
        XCTAssertEqual(unavailableState.visibleFieldCoverage.source(for: .voltageSag), .explicitlyUnavailable)
        XCTAssertEqual(unavailableState.visibleFieldCoverage.source(for: .limpHomeRange), .explicitlyUnavailable)
        XCTAssertEqual(typedState.voltageSag, VoltageDelta(value: -1_200))
        XCTAssertEqual(typedState.limpHomeRange, Distance(value: 22_852_500))
        XCTAssertEqual(typedState.visibleFieldCoverage.source(for: .voltageSag), .derivedTelemetry)
        XCTAssertEqual(typedState.visibleFieldCoverage.source(for: .limpHomeRange), .derivedTelemetry)
    }

    func testRideStateBuildsControllerOnlyEstimateFromLiveTelemetry() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(117_600),
                    batteryCurrent: batteryCurrentValue(38_000),
                    voltageSag: VoltageDelta(value: 4_800),
                    batteryLevelEstimated: batteryLevelValue(71)
                )
            )
        )

        XCTAssertEqual(rideState.controllerOnlyEstimatePercent, batteryLevelValue(71))
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, .recentSag)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .medium)
    }

    func testRideStateLowersControllerOnlyEstimateConfidenceWhenSagIsUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(117_600),
                    batteryLevelReported: batteryLevelValue(68)
                )
            )
        )

        XCTAssertEqual(rideState.controllerOnlyEstimatePercent, batteryLevelValue(68))
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, .voltageCurve)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .low)
    }

    func testRideStateAccountsForParkedPwmAsNotApplicable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .pwmHeadroom), .notApplicable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .explicitlyUnavailable)
    }

    func testRideStateAccountsForEmptyLiveSnapshotAsExplicitlyUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .packVoltage), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .thermal), .explicitlyUnavailable)
    }

    func testRideStateClassifiesUpdateAgeFromMonotonicTimestamp() {
        let missing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let fresh = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000))
            )
        )
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(lastUpdate: MonotonicMilliseconds(1_000))
        )

        XCTAssertEqual(
            missing.updateAge(at: MonotonicMilliseconds(1_100), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: nil, freshness: .unavailable)
        )
        XCTAssertEqual(
            fresh.updateAge(at: MonotonicMilliseconds(1_100), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(100), freshness: .fresh)
        )
        XCTAssertEqual(
            stale.updateAge(at: MonotonicMilliseconds(1_300), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(300), freshness: .stale)
        )
        XCTAssertEqual(fresh.visibleFieldCoverage.source(for: .updateAge), .liveTelemetry)
    }

    func testRideStateUsesTypedStaleWarningWhenTelemetryIsOld() {
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000), voltage: voltageValue(118_000)),
                lastUpdate: MonotonicMilliseconds(4_000)
            )
        )
        let fresh = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(3_900), voltage: voltageValue(118_000)),
                lastUpdate: MonotonicMilliseconds(4_000)
            )
        )

        XCTAssertEqual(
            stale.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)),
            EucRideWarningState(severity: .caution, title: "Telemetry stale", detail: "Last update 3000 ms ago")
        )
        XCTAssertEqual(
            fresh.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)).severity,
            .normal
        )
    }

    func testRideStatePrefersStaleWarningOverLowPwmHeadroom() {
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    operatingState: .riding,
                    pwm: dutyCycle(800)
                )
            )
        )

        XCTAssertEqual(stale.warningState.severity, .reduceAcceleration)
        XCTAssertEqual(
            stale.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)),
            EucRideWarningState(severity: .caution, title: "Telemetry stale", detail: "Last update 3000 ms ago")
        )
    }

    func testRideStateDoesNotClaimDerivedPowerForZeroCurrent() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(0)
                )
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .explicitlyUnavailable)
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
                SessionDebugRow(
                    id: "Notifications",
                    label: "Notifications",
                    metricValue: .status(display: "7", accessibility: "7")
                ),
                SessionDebugRow(
                    id: "Last update",
                    label: "Last update",
                    metricValue: .status(display: "9876 ms", accessibility: "9876 ms")
                ),
            ]
        )
    }

    private var scriptedVescCandidate: DevicePickerDiscoveryCandidate {
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "scripted-vesc",
            displayName: "Scripted VESC",
            productCategory: "VESC Onewheel",
            evidence: "test script",
            detail: "core callback fixture",
            support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
            symbolName: "circle.hexagongrid.circle"
        )
    }
}

private func assertVescTelemetryRequests(
    _ operations: [CoreBluetoothPlannedOperation],
    includesSubscribe: Bool,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    let expectedWriteCount = 3
    XCTAssertEqual(operations.count, expectedWriteCount + (includesSubscribe ? 1 : 0), file: file, line: line)
    if includesSubscribe {
        XCTAssertTrue(
            operations.contains(.subscribe(channel: .vescNordicUartNotify)),
            file: file,
            line: line
        )
    }
    let writes = operations.compactMap { operation -> Data? in
        guard case .writeWithoutResponse(channel: .vescNordicUartWrite, bytes: let bytes) = operation else {
            return nil
        }
        return bytes
    }
    XCTAssertEqual(writes.count, expectedWriteCount, file: file, line: line)
    XCTAssertTrue(writes.first.map { isRefloatRequest($0, command: 32) } ?? false, file: file, line: line)
    XCTAssertEqual(writes[1], Data([2, 1, 14, 225, 206, 3]), file: file, line: line)
    XCTAssertEqual(writes[2], Data([2, 1, 4, 64, 132, 3]), file: file, line: line)
}

private func isRefloatRequest(_ bytes: Data, command: UInt8) -> Bool {
    bytes.count >= 7
        && bytes.first == 0x02
        && bytes.last == 0x03
        && bytes[bytes.index(bytes.startIndex, offsetBy: 2)] == 36
        && bytes[bytes.index(bytes.startIndex, offsetBy: 3)] == 101
        && bytes[bytes.index(bytes.startIndex, offsetBy: 4)] == command
}

private final class RecordingOperationSink: CoreBluetoothOperationSink {
    enum Event: Equatable {
        case subscribe
        case write
    }

    var writes: [Data] = []
    var events: [Event] = []

    func subscribe(channel: BluetoothUuid) {
        events.append(.subscribe)
    }

    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        writes.append(bytes)
        events.append(.write)
    }

    func disconnect() {}
}

private func speedValue(_ value: Int32) -> Speed {
    Speed(value: value)
}

private func voltageValue(_ value: Int32) -> Voltage {
    Voltage(value: value)
}

private func batteryCurrentValue(_ value: Int32) -> BatteryCurrent {
    BatteryCurrent(value: value)
}

private func phaseCurrentValue(_ value: Int32) -> PhaseCurrent {
    PhaseCurrent(value: value)
}

private func powerValue(_ value: Int64) -> Power {
    Power(value: value)
}

private func temperatureValue(_ value: Int32) -> Temperature {
    Temperature(value: value)
}

private func angleValue(_ value: Int32) -> Angle {
    Angle(value: value)
}

private func batteryLevelValue(_ value: UInt8) -> BatteryLevel {
    BatteryLevel(value: value)
}

private func dutyCycle(_ permille: Int16) -> DutyCycle {
    DutyCycle(permille: permille)
}

private final class TestMonotonicClock {
    var now: MonotonicMilliseconds

    init(_ now: MonotonicMilliseconds) {
        self.now = now
    }
}

private extension [EucRideVisibleFieldCoverage] {
    func source(for field: EucRideVisibleField) -> EucRideVisibleFieldSource? {
        first { $0.field == field }?.source
    }
}

private final class RecordingReconnectScheduler: ConnectionReconnectScheduling {
    private final class Token: ConnectionReconnectCancellable {
        var isCancelled = false

        func cancel() {
            isCancelled = true
        }
    }

    private var scheduled: [(token: Token, operation: () -> Void)] = []

    func schedule(after _: UInt64, operation: @escaping () -> Void) -> any ConnectionReconnectCancellable {
        let token = Token()
        scheduled.append((token, operation))
        return token
    }

    func runAll() {
        let scheduled = scheduled
        self.scheduled.removeAll()
        for entry in scheduled where !entry.token.isCancelled {
            entry.operation()
        }
    }
}
