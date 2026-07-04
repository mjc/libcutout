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

    func testRideStateExposesPwmHeadroomOnlyWhileRiding() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .available)
        XCTAssertEqual(rideState.pwmHeadroomPermille, 770)
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
        let charging = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .charging)
            )
        )

        XCTAssertEqual(parked.statusText, "Parked")
        XCTAssertEqual(riding.statusText, "Riding")
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
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, "derived from voltage curve + recent sag")
        XCTAssertEqual(rideState.controllerOnlyConfidenceTitle, "medium")
        XCTAssertEqual(rideState.controllerOnlyConfidenceDetail, "not cell-safe")
        XCTAssertEqual(rideState.controllerOnlyRidingRuleProgress, 0.62, accuracy: 0.001)
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
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, "derived from voltage curve only")
        XCTAssertEqual(rideState.controllerOnlyConfidenceTitle, "low")
        XCTAssertEqual(rideState.controllerOnlyConfidenceDetail, "not cell-safe")
        XCTAssertEqual(rideState.controllerOnlyRidingRuleProgress, 0.35, accuracy: 0.001)
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

private func batteryCurrentValue(_ value: Int32) -> BatteryCurrent {
    BatteryCurrent(value: value)
}

private func powerValue(_ value: Int64) -> Power {
    Power(value: value)
}

private func temperatureValue(_ value: Int32) -> Temperature {
    Temperature(value: value)
}

private func batteryLevelValue(_ value: UInt8) -> BatteryLevel {
    BatteryLevel(value: value)
}

private func dutyCycle(_ permille: Int16) -> DutyCycle {
    DutyCycle(permille: permille)
}

private extension [EucRideVisibleFieldCoverage] {
    func source(for field: EucRideVisibleField) -> EucRideVisibleFieldSource? {
        first { $0.field == field }?.source
    }
}
