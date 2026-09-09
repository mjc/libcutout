import XCTest
@testable import CutoutMobile

final class VescPollingTests: XCTestCase {
    func testOwnerPollsQuietConnectionAfterSubscriptionAndCancelsOnLinkDown() throws {
        let queue = DispatchQueue(label: "cutout.vesc.polling-test")
        let polled = expectation(description: "quiet connection polling")
        let stopped = expectation(description: "disconnected polling stopped")
        let sink = PollingSink()
        var now: UInt64 = 0
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(peripheralIdentifier: CoreBluetoothPeripheralIdentifier("poll-test"), localName: "VESC", advertisedServiceUuids: []),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            executionQueue: queue,
            monotonicClock: MonotonicClock(now: { now += 100; return MonotonicMilliseconds(now) })
        )
        try queue.sync {
            _ = try owner.handleLinkUp(at: MonotonicMilliseconds(0))
            XCTAssertEqual(sink.writes, 0)
            sink.onWrite = { if sink.writes == 6 { polled.fulfill() } }
            owner.handleNotificationStateUpdate(channel: .vescNordicUartNotify, isNotifying: true, error: nil)
            XCTAssertEqual(sink.writes, 3)
        }
        wait(for: [polled], timeout: 2)
        try queue.sync {
            _ = try owner.handleLinkDown(at: MonotonicMilliseconds(201))
            let count = sink.writes
            queue.asyncAfter(deadline: .now() + .milliseconds(250)) {
                XCTAssertEqual(sink.writes, count)
                stopped.fulfill()
            }
        }
        wait(for: [stopped], timeout: 2)
    }

    func testMissingPackagePollsAndManualOverlapIsPacedAcrossReconnect() throws {
        let runner = CoreBluetoothSessionRunner(session: .vescOnewheel(), writeLimit: TransportWriteLimitBytes(20))
        _ = try runner.handle(.linkUp(at: MonotonicMilliseconds(0)))
        XCTAssertEqual(try runner.handle(.tick(at: MonotonicMilliseconds(100))).operations.count, 3)
        XCTAssertEqual(try runner.handle(.command(.requestTelemetry, at: MonotonicMilliseconds(100))).operations.count, 0)
        XCTAssertEqual(try runner.handle(.tick(at: MonotonicMilliseconds(199))).operations.count, 0)
        XCTAssertEqual(try runner.handle(.tick(at: MonotonicMilliseconds(200))).operations.count, 3)
        _ = try runner.handle(.linkDown(at: MonotonicMilliseconds(201)))
        XCTAssertEqual(try runner.handle(.tick(at: MonotonicMilliseconds(500))).operations.count, 0)
        XCTAssertEqual(try runner.handle(.command(.requestTelemetry, at: MonotonicMilliseconds(500))).operations.count, 0)
        XCTAssertEqual(try runner.handle(.linkUp(at: MonotonicMilliseconds(1000))).operations.count, 4)
    }
}

private final class PollingSink: CoreBluetoothOperationSink, @unchecked Sendable {
    var writes = 0
    var onWrite: (() -> Void)?
    func subscribe(channel: BluetoothUuid) {}
    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        writes += 1
        onWrite?()
    }
    func disconnect() {}
}
