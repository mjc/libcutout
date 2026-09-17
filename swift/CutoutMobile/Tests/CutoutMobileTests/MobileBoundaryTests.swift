import CutoutMobile
import Foundation
import XCTest

final class MobileBoundaryTests: XCTestCase {
    func testAeroTelemetryCrossesMobileBoundary() throws {
        let aero = try ElectricUnicycleSession(model: .aero)
        let linkActions = try aero.linkUp(
            at: MonotonicMilliseconds(1),
            writeLimit: TransportWriteLimitBytes(185)
        )
        XCTAssertTrue(linkActions.contains { $0.kind == .subscribe })

        let telemetry = try aero.ingestNotification(
            Data(hex: """
                dc5a5c532a7c000000000000ab41001700000cff
                000000000226021ca8f607801afa000080c80000
                808080808080022880803080800e310e310e2f0e
                2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e
                310e2e9e05e3ad
            """),
            channel: try XCTUnwrap(linkActions.firstSubscribeChannel),
            at: MonotonicMilliseconds(2)
        )
        XCTAssertTrue(telemetry.voltage == Voltage(value: 108_760))
        XCTAssertTrue(telemetry.speed == Speed(value: 0))
        XCTAssertTrue(telemetry.operatingState == .standing)
        XCTAssertTrue(telemetry.powerFlow != nil)
        XCTAssertTrue(SpeedReadout(snapshot: telemetry).displayValue == "0.0")
        XCTAssertTrue(SpeedReadout(millimetersPerSecond: nil).displayValue == "--")
        XCTAssertTrue(SessionConnectionPhase.starting.displayText == "Starting Bluetooth...")
        XCTAssertTrue(SessionConnectionPhase.scanning.displayText == "Scanning for rides...")
        XCTAssertTrue(SessionConnectionPhase.live.displayText == "Live")
        XCTAssertTrue(SessionConnectionPhase.bluetoothUnavailable(rawState: 4).displayText == "Bluetooth unavailable: state 4")
        XCTAssertTrue(SessionConnectionPhase.failed(.missingNotifyChannel).displayText == "Missing notify channel")
        XCTAssertTrue(aero.diagnostics.malformedFrames == 0)

    }

    func testFalconTelemetryAndCommandRefusal() throws {
        let falcon = try ElectricUnicycleSession(model: .falcon)
        let falconLinkActions = try falcon.linkUp(
            at: MonotonicMilliseconds(10),
            writeLimit: TransportWriteLimitBytes(23)
        )
        XCTAssertTrue(falconLinkActions.contains { $0.kind == .subscribe })
        let falconChannel = try XCTUnwrap(falconLinkActions.firstSubscribeChannel)
        for (offset, chunk) in falconRidingChunks.enumerated() {
            _ = try falcon.ingestNotification(
                Data(chunk),
                channel: falconChannel,
                at: MonotonicMilliseconds(UInt64(11 + offset))
            )
        }
        XCTAssertTrue(falcon.currentSnapshot.voltage != nil)

        do {
            _ = try falcon.perform(.soundHorn, at: MonotonicMilliseconds(3))
            XCTFail("Falcon read-only facade must refuse soundHorn")
        } catch CutoutSessionError.commandRefused(let command, _) {
            XCTAssertTrue(command == .soundHorn)
        }

    }

    func testBluetoothTransportAndLiveTelemetry() throws {
        let advertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("falcon-001"),
            localName: "device-001",
            advertisedServiceUuids: [BluetoothUuid.bluetooth16(0xffe0)]
        )
        XCTAssertTrue(advertisement.modelHint == .unknown)

        let coordinator = CoreBluetoothCentralCoordinator(
            scanPolicy: .aeroFalcon,
            writeLimit: TransportWriteLimitBytes(185)
        )
        XCTAssertTrue(coordinator.startScanning() == .scan(serviceUuids: [
            BluetoothUuid.bluetooth16(0xffe0),
            BluetoothUuid.bluetooth16(0xfff0),
        ]))
        XCTAssertTrue(coordinator.handleDiscovered(advertisement) == .connect(
            peripheralIdentifier: advertisement.peripheralIdentifier
        ))
        let unknownAdvertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("unknown-001"),
            localName: "Mystery Wheel",
            advertisedServiceUuids: []
        )
        XCTAssertTrue(coordinator.handleDiscovered(unknownAdvertisement) == nil)
        let inventory = CoreBluetoothGattInventory(services: [
            CoreBluetoothGattService(
                uuid: BluetoothUuid.bluetooth16(0xffe0),
                characteristics: [
                    CoreBluetoothGattCharacteristic(
                        uuid: BluetoothUuid.bluetooth16(0xffe1),
                        properties: [.notify, .writeWithoutResponse]
                    ),
                ]
            ),
        ])
        XCTAssertTrue(coordinator.discoverServices() == .discoverServices([
            BluetoothUuid.bluetooth16(0xffe0),
            BluetoothUuid.bluetooth16(0xfff0),
        ]))
        XCTAssertTrue(coordinator.discoverCharacteristics(in: inventory) == [
            .discoverCharacteristics(
                service: BluetoothUuid.bluetooth16(0xffe0),
                characteristics: [BluetoothUuid.bluetooth16(0xffe1)]
            ),
        ])

        let writeAction = SessionAction.write(
            channel: BluetoothUuid.bluetooth16(0xffe1).bytes,
            bytes: Data([0x01, 0x02, 0x03, 0x04, 0x05])
        )
        let writes = CoreBluetoothTransportPlanner(
            writeLimit: TransportWriteLimitBytes(2)
        ).plan(action: writeAction)
        XCTAssertTrue(writes == [
            .writeWithoutResponse(channel: BluetoothUuid.bluetooth16(0xffe1), bytes: Data([0x01, 0x02])),
            .writeWithoutResponse(channel: BluetoothUuid.bluetooth16(0xffe1), bytes: Data([0x03, 0x04])),
            .writeWithoutResponse(channel: BluetoothUuid.bluetooth16(0xffe1), bytes: Data([0x05])),
        ])
        let executorSink = RecordingCoreBluetoothOperationSink()
        CoreBluetoothOperationExecutor(sink: executorSink).execute(writes)
        XCTAssertTrue(executorSink.recordedOperations == writes)

        let runner = CoreBluetoothSessionRunner(
            session: try .electricUnicycle(model: .aero),
            writeLimit: TransportWriteLimitBytes(185),
            captureContext: CoreBluetoothCaptureContext(
                platformIdentifier: advertisement.peripheralIdentifier,
                advertisement: advertisement,
                writeLimit: TransportWriteLimitBytes(185)
            )
        )
        let runnerSubscribe = try runner.handle(.linkUp(at: MonotonicMilliseconds(20)))
        XCTAssertTrue(runnerSubscribe.operations.contains(.subscribe(channel: BluetoothUuid.bluetooth16(0xffe1))))
        XCTAssertTrue(runnerSubscribe.captureContext?.advertisedServiceUuids == [BluetoothUuid.bluetooth16(0xffe0)])
        XCTAssertTrue(runnerSubscribe.captureContext?.resolvedModelHint == .unknown)
        let runnerTelemetry = try runner.handle(.notification(
            bytes: Data(hex: """
                dc5a5c532a7c000000000000ab41001700000cff
                000000000226021ca8f607801afa000080c80000
                808080808080022880803080800e310e310e2f0e
                2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e
                310e2e9e05e3ad
            """),
            channel: BluetoothUuid.bluetooth16(0xffe1),
            at: MonotonicMilliseconds(21)
        ))
        XCTAssertTrue(runnerTelemetry.snapshot?.voltage == Voltage(value: 108_760))
        XCTAssertTrue(runnerTelemetry.snapshot?.speed == Speed(value: 0))
        XCTAssertTrue(runnerTelemetry.snapshot?.operatingState == .standing)
        XCTAssertTrue(runnerTelemetry.snapshot?.powerFlow != nil)

        let liveSink = RecordingCoreBluetoothOperationSink()
        let liveOwner = CoreBluetoothLiveSessionOwner(
            session: try .electricUnicycle(model: .aero),
            advertisement: advertisement,
            writeLimit: TransportWriteLimitBytes(185),
            operationSink: liveSink
        )
        let linkStep = try liveOwner.handleLinkUp(at: MonotonicMilliseconds(30))
        XCTAssertTrue(linkStep.operations.contains(.subscribe(channel: BluetoothUuid.bluetooth16(0xffe1))))
        XCTAssertTrue(liveSink.recordedOperations.contains(.subscribe(channel: BluetoothUuid.bluetooth16(0xffe1))))
        XCTAssertTrue(liveOwner.records.contains(.linkUp(
            platformIdentifier: advertisement.peripheralIdentifier,
            writeLimit: TransportWriteLimitBytes(185)
        )))
        liveOwner.recordInventory(inventory)
        XCTAssertTrue(liveOwner.records.contains(.gattInventory(
            platformIdentifier: advertisement.peripheralIdentifier,
            inventory: inventory
        )))
        let liveTelemetry = try liveOwner.handleNotification(
            bytes: Data(hex: """
                dc5a5c532a7c000000000000ab41001700000cff
                000000000226021ca8f607801afa000080c80000
                808080808080022880803080800e310e310e2f0e
                2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e
                310e2e9e05e3ad
            """),
            channel: BluetoothUuid.bluetooth16(0xffe1),
            at: MonotonicMilliseconds(31)
        )
        XCTAssertTrue(liveTelemetry.snapshot?.voltage == Voltage(value: 108_760))
        XCTAssertTrue(liveTelemetry.snapshot?.speed == Speed(value: 0))
        XCTAssertTrue(liveTelemetry.snapshot?.operatingState == .standing)
        XCTAssertTrue(liveTelemetry.snapshot?.powerFlow != nil)
        let initialSpeedState = RideDisplayState()
        XCTAssertTrue(initialSpeedState.speed.displayValue == "--")
        XCTAssertTrue(initialSpeedState.notificationCount == 0)
        let zeroSpeedState = initialSpeedState.reducing(
            liveTelemetry,
            receivedAt: MonotonicMilliseconds(31)
        )
        XCTAssertTrue(zeroSpeedState.speed.millimetersPerSecond == 0)
        XCTAssertTrue(zeroSpeedState.speed.displayValue == "0.0")
        XCTAssertTrue(zeroSpeedState.notificationCount == 1)
        XCTAssertTrue(zeroSpeedState.lastUpdate == MonotonicMilliseconds(31))
        let nonzeroSpeedStep = CoreBluetoothSessionStep(
            operations: [],
            snapshot: TelemetrySnapshot(
                speed: Speed(value: 1_000)
            )
        )
        let nonzeroSpeedState = zeroSpeedState.reducing(
            nonzeroSpeedStep,
            receivedAt: MonotonicMilliseconds(32)
        )
        XCTAssertTrue(nonzeroSpeedState.speed.millimetersPerSecond == 1_000)
        XCTAssertTrue(nonzeroSpeedState.notificationCount == 2)
        XCTAssertTrue(nonzeroSpeedState.lastUpdate == MonotonicMilliseconds(32))
        XCTAssertTrue(liveOwner.records.contains {
            if case .notification(let channel, let byteCount, _) = $0 {
                channel == BluetoothUuid.bluetooth16(0xffe1) && byteCount.rawValue > 0
            } else {
                false
            }
        })
        #if canImport(CoreBluetooth)
        XCTAssertTrue(CoreBluetoothScanPolicy.aeroFalcon.serviceUuids.count == 2)
        XCTAssertTrue(CoreBluetoothScanPolicy.aeroFalcon.serviceUuids.contains(.bluetooth16(0xffe0)))
        XCTAssertTrue(CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids.count == 2)
        XCTAssertTrue(CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids.map(\.uuidString) == ["FFE0", "FFF0"])
        XCTAssertTrue(coordinator.startScanning().coreBluetoothServiceUuids.count == 2)
        _ = CoreBluetoothPeripheralOperationSink.self
        _ = CoreBluetoothLiveSessionOwner.self
        #endif
    }
}

private let falconRidingChunks: [[UInt8]] = [
    [0, 0, 0, 0, 0, 0, 3, 2, 90, 90, 90, 90, 85, 170, 0, 17, 118, 110, 73, 1],
    [28, 21, 0, 45, 0, 1, 0, 0, 0, 18, 4, 24, 90, 90, 90, 90, 85, 170, 0, 28],
    [0, 147, 0, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 24, 90, 90, 90, 90],
    [71, 87, 49, 54, 50, 49, 48, 48, 51],
    [85, 170, 25, 153, 0, 0, 0, 63, 0, 1, 255, 136, 244, 151, 0, 136, 0, 1, 0, 24],
    [90, 90, 90, 90, 85, 170, 0, 75, 255, 253, 3, 215, 0, 0, 0, 0, 19, 136, 0, 0],
    [0, 0, 1, 3, 90, 90, 90, 90, 85, 170, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
]

private final class RecordingCoreBluetoothOperationSink: CoreBluetoothOperationSink {
    private(set) var recordedOperations: [CoreBluetoothPlannedOperation] = []

    func subscribe(channel: BluetoothUuid) {
        recordedOperations.append(.subscribe(channel: channel))
    }

    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        recordedOperations.append(.writeWithoutResponse(channel: channel, bytes: bytes))
    }

    func disconnect() {
        recordedOperations.append(.disconnect)
    }
}

private extension Array where Element == SessionAction {
    var firstSubscribeChannel: Data? {
        first { $0.kind == .subscribe }?.channel
    }
}

private extension Data {
    init(hex text: String) {
        let digits = text.filter { !$0.isWhitespace }
        XCTAssertTrue(digits.count.isMultiple(of: 2))
        self = stride(from: 0, to: digits.count, by: 2).reduce(into: Data()) { bytes, offset in
            let start = digits.index(digits.startIndex, offsetBy: offset)
            let end = digits.index(start, offsetBy: 2)
            bytes.append(UInt8(digits[start..<end], radix: 16)!)
        }
    }
}
