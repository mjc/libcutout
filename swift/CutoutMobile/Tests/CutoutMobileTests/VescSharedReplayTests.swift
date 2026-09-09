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
            if notification.name == "ordinary values complete" {
                XCTAssertFalse(
                    step.actions.contains { $0.vescRealtimeTelemetry },
                    "generic VESC values must not satisfy the Refloat startup retry"
                )
            }
            if notification.name == "Refloat 1.3 complete runtime data with alerts" {
                XCTAssertTrue(
                    step.actions.contains { $0.vescRealtimeTelemetry },
                    "a fresh Refloat realtime event must satisfy the startup retry"
                )
            }
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

    func testCompleteRefloatDescriptorSurvivesEveryNotificationSplitAtRunnerBoundary() throws {
        let fixture = try loadFixture()
        let descriptor = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete 393-byte descriptor"
        })
        let realtime = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete runtime data with alerts"
        })
        let descriptorChannel = try XCTUnwrap(BluetoothUuid(Data(descriptor.channel)))
        let realtimeChannel = try XCTUnwrap(BluetoothUuid(Data(realtime.channel)))
        XCTAssertEqual(descriptor.bytes.count, 393)

        for split in 1..<descriptor.bytes.count {
            let runner = CoreBluetoothSessionRunner(
                session: .vescOnewheel(),
                writeLimit: TransportWriteLimitBytes(20)
            )
            _ = try runner.handle(.linkUp(at: MonotonicMilliseconds(0)))

            let first = try runner.handle(.notification(
                bytes: Data(descriptor.bytes[..<split]),
                channel: descriptorChannel,
                at: MonotonicMilliseconds(1)
            ))
            XCTAssertEqual(
                first.actions.filter { $0.kind == .notificationIngest }.count,
                1,
                "descriptor prefix split at byte \(split)"
            )

            let second = try runner.handle(.notification(
                bytes: Data(descriptor.bytes[split...]),
                channel: descriptorChannel,
                at: MonotonicMilliseconds(2)
            ))
            XCTAssertEqual(
                second.actions.filter { $0.kind == .notificationIngest }.count,
                1,
                "descriptor suffix split at byte \(split)"
            )

            let runtime = try runner.handle(.notification(
                bytes: Data(realtime.bytes),
                channel: realtimeChannel,
                at: MonotonicMilliseconds(3)
            ))
            XCTAssertEqual(runtime.snapshot?.speed?.value, realtime.speed_mmps, "split \(split)")
            XCTAssertEqual(runtime.snapshot?.voltage?.value, realtime.voltage_mv, "split \(split)")
        }
    }

    func testMalformedPrefixIsReportedAndDoesNotPoisonTheFollowingRefloatFrame() throws {
        let fixture = try loadFixture()
        let malformed = try XCTUnwrap(fixture.notifications.first { $0.name == "malformed prefix" })
        let descriptor = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete 393-byte descriptor"
        })
        let realtime = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete runtime data with alerts"
        })
        let session = VescOnewheelSession()
        _ = try session.linkUp(at: MonotonicMilliseconds(0), writeLimit: TransportWriteLimitBytes(20))

        _ = try session.ingestNotificationActions(
            Data(descriptor.bytes),
            channel: Data(descriptor.channel),
            at: MonotonicMilliseconds(1)
        )

        let malformedActions = try session.ingestNotificationActions(
            Data(malformed.bytes),
            channel: Data(malformed.channel),
            at: MonotonicMilliseconds(2)
        )
        XCTAssertEqual(malformedActions.filter { $0.kind == .notificationIngest }.count, 1)

        _ = try session.ingestNotificationActions(
            Data(realtime.bytes),
            channel: Data(realtime.channel),
            at: MonotonicMilliseconds(3)
        )
        XCTAssertEqual(session.currentSnapshot.speed?.value, realtime.speed_mmps)
        XCTAssertEqual(session.currentSnapshot.voltage?.value, realtime.voltage_mv)
    }

    func testOwnerForwardsBackpressureReadinessAndClearsWritesOnReconnect() throws {
        let sink = BackpressureSink()
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("backpressure-fixture"),
                localName: "VESC fixture",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(0))
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )
        XCTAssertEqual(sink.writes.count, 3)

        owner.handlePeripheralIsReadyToSendWithoutResponse()
        XCTAssertEqual(sink.readyCallbacks, 1)

        _ = try owner.handleLinkDown(at: MonotonicMilliseconds(10))
        XCTAssertEqual(sink.clearCallbacks, 1)
        XCTAssertTrue(sink.writes.isEmpty)

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(20))
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )
        XCTAssertEqual(sink.subscriptions, 2)
        XCTAssertEqual(sink.writes.count, 3)
    }

    func testRunnerReconnectRediscoversCompleteRefloatDescriptorBeforeRuntimeData() throws {
        let fixture = try loadFixture()
        let descriptor = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete 393-byte descriptor"
        })
        let realtime = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete runtime data with alerts"
        })
        let descriptorChannel = try XCTUnwrap(BluetoothUuid(Data(descriptor.channel)))
        let realtimeChannel = try XCTUnwrap(BluetoothUuid(Data(realtime.channel)))
        let runner = CoreBluetoothSessionRunner(
            session: .vescOnewheel(),
            writeLimit: TransportWriteLimitBytes(20)
        )

        _ = try runner.handle(.linkUp(at: MonotonicMilliseconds(0)))
        _ = try runner.handle(.notification(
            bytes: Data(descriptor.bytes), channel: descriptorChannel, at: MonotonicMilliseconds(1)
        ))
        _ = try runner.handle(.notification(
            bytes: Data(realtime.bytes), channel: realtimeChannel, at: MonotonicMilliseconds(2)
        ))
        XCTAssertEqual(
            try XCTUnwrap(runner.handle(.notification(
                bytes: Data(realtime.bytes), channel: realtimeChannel, at: MonotonicMilliseconds(3)
            )).snapshot?.speed?.value),
            realtime.speed_mmps
        )

        _ = try runner.handle(.linkDown(at: MonotonicMilliseconds(4)))
        let relink = try runner.handle(.linkUp(at: MonotonicMilliseconds(5)))
        XCTAssertTrue(relink.operations.contains(.subscribe(channel: .vescNordicUartNotify)))
        XCTAssertTrue(relink.operations.contains {
            if case .writeWithoutResponse(channel: .vescNordicUartWrite, bytes: let bytes) = $0 {
                return isRefloatRequestForReplay(bytes, command: 0)
            }
            return false
        })

        _ = try runner.handle(.notification(
            bytes: Data(descriptor.bytes), channel: descriptorChannel, at: MonotonicMilliseconds(6)
        ))
        let recovered = try runner.handle(.notification(
            bytes: Data(realtime.bytes), channel: realtimeChannel, at: MonotonicMilliseconds(7)
        ))
        XCTAssertEqual(recovered.snapshot?.speed?.value, realtime.speed_mmps)
        XCTAssertEqual(recovered.snapshot?.voltage?.value, realtime.voltage_mv)
    }

    func testGenericValuesDoNotCancelRetryWhileRetainedRefloatTelemetryIsStale() throws {
        let fixture = try loadFixture()
        let descriptor = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete 393-byte descriptor"
        })
        let realtime = try XCTUnwrap(fixture.notifications.first {
            $0.name == "Refloat 1.3 complete runtime data with alerts"
        })
        let generic = try XCTUnwrap(fixture.notifications.first {
            $0.name == "ordinary values complete"
        })
        let queue = DispatchQueue(label: "cutout.vesc.stale-retry-test")
        let retried = expectation(description: "reconnect retry after generic values")
        let sink = ReplayOperationSink()
        var reconnectWriteCount = 0
        sink.onWrite = {
            if reconnectWriteCount > 0, sink.writes.count > reconnectWriteCount {
                retried.fulfill()
            }
        }
        var now: UInt64 = 1_000
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("stale-retry-fixture"),
                localName: "VESC fixture",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            maximumRetryAttempts: 1,
            retryDelay: .milliseconds(20),
            executionQueue: queue,
            monotonicClock: MonotonicClock(now: {
                now += 100
                return MonotonicMilliseconds(now)
            })
        )

        try queue.sync {
            _ = try owner.handleLinkUp(at: MonotonicMilliseconds(0))
            owner.handleNotificationStateUpdate(
                channel: .vescNordicUartNotify,
                isNotifying: true,
                error: nil
            )
            _ = try owner.handleNotification(
                bytes: Data(descriptor.bytes),
                channel: .vescNordicUartNotify,
                at: MonotonicMilliseconds(1)
            )
            _ = try owner.handleNotification(
                bytes: Data(realtime.bytes),
                channel: .vescNordicUartNotify,
                at: MonotonicMilliseconds(2)
            )
            _ = try owner.handleLinkDown(at: MonotonicMilliseconds(3))
            _ = try owner.handleLinkUp(at: MonotonicMilliseconds(4))
            owner.handleNotificationStateUpdate(
                channel: .vescNordicUartNotify,
                isNotifying: true,
                error: nil
            )
            reconnectWriteCount = sink.writes.count
            XCTAssertEqual(reconnectWriteCount, 6, "reconnect subscription must release the three startup requests")

            let genericStep = try owner.handleNotification(
                bytes: Data(generic.bytes),
                channel: .vescNordicUartNotify,
                at: MonotonicMilliseconds(5)
            )
            XCTAssertFalse(
                genericStep.actions.contains { $0.vescRealtimeTelemetry },
                "generic values must not look like fresh Refloat telemetry"
            )
            XCTAssertNotNil(genericStep.snapshot?.pitch)
            XCTAssertNotNil(genericStep.snapshot?.footpad)
            XCTAssertEqual(sink.writes.count, reconnectWriteCount)
        }

        wait(for: [retried], timeout: 2)
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

private func isRefloatRequestForReplay(_ bytes: Data, command: UInt8) -> Bool {
    bytes.count >= 7
        && bytes.first == 0x02
        && bytes.last == 0x03
        && bytes[bytes.index(bytes.startIndex, offsetBy: 2)] == 36
        && bytes[bytes.index(bytes.startIndex, offsetBy: 3)] == 101
        && bytes[bytes.index(bytes.startIndex, offsetBy: 4)] == command
}

private final class BackpressureSink: CoreBluetoothOperationSink {
    var subscriptions = 0
    var writes: [Data] = []
    var readyCallbacks = 0
    var clearCallbacks = 0

    func subscribe(channel: BluetoothUuid) {
        subscriptions += 1
    }

    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        writes.append(bytes)
    }

    func disconnect() {}

    func peripheralIsReadyToSendWithoutResponse() {
        readyCallbacks += 1
    }

    func clearPendingWithoutResponseWrites() {
        clearCallbacks += 1
        writes.removeAll()
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
    var onWrite: (() -> Void)?
    func subscribe(channel: BluetoothUuid) { subscriptions.append(channel) }
    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        writes.append(bytes)
        onWrite?()
    }
    func disconnect() {}
}
