import Foundation
import XCTest
@testable import CutoutMobile

final class VescSharedReplayTests: XCTestCase {
    func testRustOwnedFixtureThroughLiveOwnerAndDisplayCore() throws {
        let fixture = try loadFixture()
        XCTAssertEqual(fixture.version, 1)
        let sink = ReplayOperationSink()
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("redacted-replay"),
                localName: "VESC replay fixture",
                advertisedServiceUuids: [try XCTUnwrap(BluetoothUuid(Data(fixture.service)))]
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink
        )
        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(0))
        XCTAssertEqual(sink.subscriptions, [.vescNordicUartNotify])
        XCTAssertTrue(sink.writes.isEmpty, "requests wait for subscription readiness")
        owner.handleNotificationStateUpdate(channel: .vescNordicUartNotify, isNotifying: true, error: nil)
        XCTAssertFalse(sink.writes.isEmpty)

        let core = CutoutSessionCore()
        for (index, notification) in fixture.notifications.enumerated() {
            let at = MonotonicMilliseconds(notification.monotonic_ms)
            let channel = try XCTUnwrap(BluetoothUuid(Data(notification.channel)))
            let step = try owner.handleNotification(bytes: Data(notification.bytes), channel: channel, at: at)
            XCTAssertEqual(step.snapshot?.voltage?.value, notification.voltage_mv, notification.name)
            XCTAssertEqual(step.snapshot?.speed?.value, notification.speed_mmps, notification.name)
            XCTAssertEqual(step.actions.filter { $0.kind == .notificationIngest }.count,
                           notification.ingest_count, notification.name)
            XCTAssertNotNil(step.captureContext, notification.name)
            XCTAssertTrue(owner.records.contains(.notification(
                channel: channel,
                byteCount: CoreBluetoothPayloadByteCount(notification.bytes.count),
                at: at
            )), notification.name)
            core.applyNotificationStep(step, receivedAt: at)
            XCTAssertEqual(Int(core.displayState.notificationCount), index + 1, notification.name)
            XCTAssertEqual(core.displayState.telemetry?.speed?.value, notification.speed_mmps, notification.name)
        }
    }

    private func loadFixture() throws -> ReplayFixture {
        // Test-only source-relative access keeps Rust and Swift on one checked-in corpus.
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let url = repository.appendingPathComponent("crates/cutout-mobile-ffi/tests/fixtures/vesc-replay-v1.json")
        return try JSONDecoder().decode(ReplayFixture.self, from: Data(contentsOf: url))
    }
}

private struct ReplayFixture: Decodable {
    let version: Int
    let service: [UInt8]
    let notifications: [ReplayNotification]
}

private struct ReplayNotification: Decodable {
    let name: String
    let monotonic_ms: UInt64
    let channel: [UInt8]
    let bytes: [UInt8]
    let ingest_count: Int
    let voltage_mv: Int32?
    let speed_mmps: Int32?
}

private final class ReplayOperationSink: CoreBluetoothOperationSink {
    var subscriptions: [BluetoothUuid] = []
    var writes: [Data] = []
    func subscribe(channel: BluetoothUuid) { subscriptions.append(channel) }
    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) { writes.append(bytes) }
    func disconnect() {}
}
