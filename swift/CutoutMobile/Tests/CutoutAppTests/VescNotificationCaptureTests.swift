import XCTest
import CoreBluetooth
import CoreLocation
import CutoutMobileFFI
@testable import CutoutMobile
@testable import CutoutApp

final class VescNotificationCaptureTests: XCTestCase {
    private let candidate = DevicePickerDiscoveryCandidate(
        platformIdentifier: "capture-path-fixture",
        displayName: "VESC fixture",
        productCategory: "VESC Onewheel",
        evidence: "test fixture",
        detail: "shared PEVCAP replay",
        support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
        symbolName: "circle.hexagongrid.circle"
    )

    func testNordicAndEucNotificationsReachRealCaptureWriterWithLocation() async throws {
        let (core, url) = try await startCapture()
        defer { core.disconnectAndScan(); try? FileManager.default.removeItem(at: url) }
        core.locationManager(CLLocationManager(), didUpdateLocations: [CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1600, horizontalAccuracy: 4, verticalAccuracy: 6,
            timestamp: Date()
        )])
        let pairs = [
            ("6E400001-B5A3-F393-E0A9-E50E24DCCA9E", "6E400003-B5A3-F393-E0A9-E50E24DCCA9E"),
            ("FFE0", "FFE1"),
        ]
        for (service, characteristic) in pairs {
            core.captureFrame(direction: "notify", characteristic: CBUUID(string: characteristic),
                              service: CBUUID(string: service), bytes: Data([1, 2, 3]))
        }
        let flushed = await core.flushCapture()
        XCTAssertTrue(flushed)
        let records = try captureRecords(url).filter { $0["direction"] as? String == "Inbound" }
        XCTAssertEqual(records.count, 2)
        for (record, pair) in zip(records, pairs) {
            XCTAssertEqual(record["service"] as? [UInt8], Array(try XCTUnwrap(BluetoothUuid(coreBluetoothUuid: CBUUID(string: pair.0))).bytes))
            XCTAssertEqual(record["characteristic"] as? [UInt8], Array(try XCTUnwrap(BluetoothUuid(coreBluetoothUuid: CBUUID(string: pair.1))).bytes))
            XCTAssertEqual(record["bytes"] as? [UInt8], [1, 2, 3])
            XCTAssertNotNil(record["phone_location"] as? [String: Any])
        }
    }

    func testMissingNotificationServiceTerminatesCaptureWriter() async throws {
        let (core, url) = try await startCapture()
        defer { core.disconnectAndScan(); try? FileManager.default.removeItem(at: url) }
        core.captureFrame(direction: "notify", characteristic: CBUUID(string: "6E400003-B5A3-F393-E0A9-E50E24DCCA9E"), bytes: Data([1]))
        guard case .failed = core.phase else { return XCTFail("Missing service must fail capture") }
        let flushed = await core.flushCapture()
        XCTAssertFalse(flushed, "A failed capture must relinquish its writer")
    }

    func testSharedLegacyRefloatFixturePopulatesAppDisplayAndCapture() async throws {
        let (core, url) = try await startCapture()
        defer { core.disconnectAndScan(); try? FileManager.default.removeItem(at: url) }
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(peripheralIdentifier: CoreBluetoothPeripheralIdentifier("capture-path-fixture"), localName: nil, advertisedServiceUuids: []),
            writeLimit: TransportWriteLimitBytes(20), operationSink: CaptureFixtureSink()
        )
        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(0))
        owner.handleNotificationStateUpdate(channel: .vescNordicUartNotify, isNotifying: true, error: nil)
        var root = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { root.deleteLastPathComponent() }
        let fixture = root.appendingPathComponent("crates/cutout-cli/fixtures/pevcap/vesc-refloat-live.jsonl")
        let inbound = try captureRecords(fixture).filter { $0["direction"] as? String == "Inbound" }
        XCTAssertFalse(inbound.isEmpty)
        for record in inbound {
            let bytes = Data(try XCTUnwrap(record["bytes"] as? [UInt8]))
            let channel = try XCTUnwrap(BluetoothUuid(Data(try XCTUnwrap(record["characteristic"] as? [UInt8]))))
            let service = try XCTUnwrap(BluetoothUuid(Data(try XCTUnwrap(record["service"] as? [UInt8]))))
            let at = MonotonicMilliseconds(try XCTUnwrap(record["monotonic_ms"] as? UInt64))
            let step = try owner.handleNotification(bytes: bytes, channel: channel, at: at)
            core.captureFrame(direction: "notify", characteristic: channel.coreBluetoothUuid,
                              service: service.coreBluetoothUuid, bytes: bytes,
                              telemetry: step.actions.compactMap(\.rawTelemetry).last)
            core.applyNotificationStep(step, receivedAt: at)
        }
        XCTAssertEqual(core.displayState.notificationCount, UInt64(inbound.count))
        let ride = try XCTUnwrap(VescRideSnapshot(displayState: core.displayState, title: nil))
        XCTAssertNotNil(ride.batteryVoltage)
        XCTAssertNotNil(ride.motorCurrent)
        XCTAssertNotNil(ride.boardAngle)
        XCTAssertNotNil(ride.dutyCycle)
        for tile in vescDebugTiles(ride) where tile.kind != .headroom {
            XCTAssertNotEqual(tile.metricValue, .unavailable)
        }
        let debugRows = vescDebugRows(ride, phase: core.phase, notificationCount: core.displayState.notificationCount)
        for id in ["voltage", "motor-current", "footpad"] {
            XCTAssertNotEqual(try XCTUnwrap(debugRows.first { $0.id == id }).metricValue, .unavailable)
        }
        let flushed = await core.flushCapture()
        XCTAssertTrue(flushed)
        let captured = try captureRecords(url).filter { $0["direction"] as? String == "Inbound" }
        XCTAssertEqual(captured.count, inbound.count)
        XCTAssertEqual(captured.compactMap { $0["bytes"] as? [UInt8] }, inbound.compactMap { $0["bytes"] as? [UInt8] })
    }

    private func startCapture() async throws -> (CutoutSessionCore, URL) {
        let started = expectation(description: "capture writer starts")
        var url: URL?
        let core = CutoutSessionCore(testScript: CutoutSessionTestScript(candidate: candidate, telemetry: nil))
        core.onCaptureEvent = { event in
            if case let .started(fileURL) = event { url = fileURL; started.fulfill() }
        }
        XCTAssertTrue(core.recordOnly(platformIdentifier: candidate.platformIdentifier))
        await fulfillment(of: [started], timeout: 2)
        return (core, try XCTUnwrap(url))
    }

    private func captureRecords(_ url: URL) throws -> [[String: Any]] {
        try String(contentsOf: url, encoding: .utf8).split(separator: "\n").compactMap { line in
            let value = try JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any]
            return value?["record"] as? [String: Any]
        }
    }
}

private final class CaptureFixtureSink: CoreBluetoothOperationSink {
    func subscribe(channel: BluetoothUuid) {}
    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {}
    func disconnect() {}
}
