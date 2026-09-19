import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutMobile

final class DeviceSessionTransportTests: XCTestCase {
    private let queue = DispatchQueue(label: "cutout.generic-transport.test")
    private let vescReply = Data([2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208, 3])

    private func makeTransport(_ state: CutoutSessionStateHandle, token: ConnectionAttemptToken, sink: TransportSink, writeLimit: UInt16 = 20) -> DeviceSessionTransport {
        DeviceSessionTransport(
            state: state, token: token,
            advertisement: CoreBluetoothAdvertisement(peripheralIdentifier: CoreBluetoothPeripheralIdentifier(token.platformIdentifier), localName: nil, advertisedServiceUuids: []),
            writeLimit: TransportWriteLimitBytes(writeLimit), operationSink: sink,
            queue: queue,
            clock: MonotonicClock(now: { MonotonicMilliseconds(100) })
        )
    }

    private func makeReadyTransport(writeLimit: UInt16 = 20) throws -> (CutoutSessionStateHandle, DeviceSessionTransport, TransportSink) {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "NF2557", nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: token)
        var frame = Data(repeating: 0, count: 42)
        frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        _ = state.observeConnectionNotification(token: token, bytes: frame)
        _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
        let sink = TransportSink()
        let transport = makeTransport(state, token: token, sink: sink, writeLimit: writeLimit)
        _ = try transport.handleLinkUp(at: MonotonicMilliseconds(1))
        transport.handleNotificationStateUpdate(channel: .bluetooth16(0xffe1), isNotifying: true, error: nil)
        _ = try transport.handleNotification(bytes: frame, channel: .bluetooth16(0xffe1), at: MonotonicMilliseconds(2))
        sink.receipts.removeAll()
        sink.writes.removeAll()
        return (state, transport, sink)
    }

    func testQueuedSettingReportsSubmittedWhenNativeQueueFlushes() throws {
        try queue.sync {
            let (state, transport, sink) = try makeReadyTransport()
            defer { transport.invalidate() }
            sink.dispositions = [.queued]
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            XCTAssertEqual(state.settings().setting(for: .highBeam)?.transport, .queued)
            transport.handlePeripheralIsReadyToSendWithoutResponse()
            let setting = try XCTUnwrap(state.settings().setting(for: .highBeam))
            XCTAssertEqual(setting.transport, .submitted)
            XCTAssertEqual(setting.status, .sentWithoutConfirmation)
            XCTAssertNil(setting.current)
            XCTAssertTrue(transport.records.contains { record in
                if case .writeReceipt(_, _, .submitted) = record { return true }
                return false
            })
        }
    }

    func testChunkReceiptsAggregateSettingTransportStatus() throws {
        try queue.sync {
            let cases: [([CoreBluetoothWriteDisposition], MobileSettingTransportStatusDto)] = [
                ([.submitted, .queued], .queued),
                ([.queued, .submitted], .queued),
                ([.submitted, .rejected], .rejected),
                ([.rejected, .submitted], .rejected),
            ]
            for (dispositions, expected) in cases {
                let (state, transport, sink) = try makeReadyTransport(writeLimit: 5)
                defer { transport.invalidate() }
                sink.dispositions = dispositions
                _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
                XCTAssertGreaterThan(sink.writes.count, 1)
                XCTAssertEqual(state.settings().setting(for: .highBeam)?.transport, expected)
                transport.handlePeripheralIsReadyToSendWithoutResponse()
                let setting = try XCTUnwrap(state.settings().setting(for: .highBeam))
                XCTAssertEqual(setting.transport, expected == .queued ? .submitted : .rejected)
                XCTAssertEqual(setting.status, expected == .queued ? .sentWithoutConfirmation : .failed)
            }
        }
    }

    func testOldReceiptCannotChangeRetryOfSameSettingAtSameTimestamp() throws {
        try queue.sync {
            let (state, transport, sink) = try makeReadyTransport()
            defer { transport.invalidate() }
            sink.dispositions = [.queued, .queued]
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            let oldID = state.settings().setting(for: .highBeam)?.requestId
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: false), at: MonotonicMilliseconds(3))
            XCTAssertNotEqual(oldID, state.settings().setting(for: .highBeam)?.requestId)
            sink.receipts[0](.rejected)
            XCTAssertEqual(state.settings().setting(for: .highBeam)?.transport, .queued)
            sink.receipts[1](.submitted)
            sink.receipts[0](.submitted)
            let setting = try XCTUnwrap(state.settings().setting(for: .highBeam))
            XCTAssertEqual(setting.transport, .submitted)
            XCTAssertEqual(setting.requested, .boolean(value: false))
        }
    }

    func testInvalidationRejectsQueuedWritesAndIgnoresLateReceipts() throws {
        try queue.sync {
            let (state, transport, sink) = try makeReadyTransport()
            sink.dispositions = [.queued]
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            let lateReceipt = try XCTUnwrap(sink.receipts.first)
            transport.invalidate()
            XCTAssertEqual(sink.clears, 1)
            XCTAssertEqual(state.settings().setting(for: .highBeam)?.transport, .rejected)
            lateReceipt(.submitted)
            XCTAssertEqual(state.settings().setting(for: .highBeam)?.transport, .rejected)
            XCTAssertThrowsError(try transport.submitSetting(.highBeam, value: .boolean(value: false), at: MonotonicMilliseconds(4)))
        }
    }

    func testInvalidRetryLeavesExistingQueuedRequestIntact() throws {
        try queue.sync {
            let (state, transport, sink) = try makeReadyTransport()
            defer { transport.invalidate() }
            sink.dispositions = [.queued]
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            var published: DeviceSettings?
            transport.onSettingsChange = { published = $0 }
            XCTAssertThrowsError(try transport.submitSetting(.highBeam, value: .number(value: 999), at: MonotonicMilliseconds(4)))
            let queued = try XCTUnwrap(state.settings().setting(for: .highBeam))
            XCTAssertEqual(queued.transport, .queued)
            XCTAssertNil(published, "An invalid retry changes neither the Rust snapshot nor its publication")
            sink.receipts[0](.submitted)
            XCTAssertEqual(state.settings().setting(for: .highBeam)?.transport, .submitted)
        }
    }

    func testReplacementConnectionIgnoresQueuedWriteReceipt() throws {
        try queue.sync {
            let (state, transport, sink) = try makeReadyTransport()
            defer { transport.invalidate() }
            sink.dispositions = [.queued]
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            _ = state.beginConnectionAttempt(platformIdentifier: "B", nowMs: 4)
            let before = state.settings()
            sink.receipts[0](.submitted)
            XCTAssertEqual(state.settings(), before)
        }
    }

    func testSettingWithNoPlannedWriteIsRejected() throws {
        try queue.sync {
            let (state, transport, sink) = try makeReadyTransport(writeLimit: 0)
            defer { transport.invalidate() }
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            XCTAssertTrue(sink.writes.isEmpty)
            XCTAssertEqual(state.settings().setting(for: .highBeam)?.transport, .rejected)
        }
    }

    func testNativeQueuePreservesOlderWritesOnOverflowAndReportsFlush() {
        let writes = CoreBluetoothWriteQueue(capacity: 2)
        var submitted: [Int] = []
        var receipts = [[CoreBluetoothWriteDisposition]](repeating: [], count: 3)
        for index in 0..<3 {
            let disposition = writes.submit(canSend: { false }, write: { submitted.append(index) }, onReceipt: { receipts[index].append($0) })
            XCTAssertEqual(disposition, index < 2 ? .queued : .rejected)
        }
        XCTAssertTrue(submitted.isEmpty)
        var allowance = 1
        writes.flush {
            defer { allowance -= 1 }
            return allowance > 0
        }
        XCTAssertEqual(submitted, [0])
        XCTAssertEqual(receipts, [[.queued, .submitted], [.queued], [.rejected]])
        writes.flush { true }
        XCTAssertEqual(submitted, [0, 1])
        XCTAssertEqual(receipts, [[.queued, .submitted], [.queued, .submitted], [.rejected]])
    }

    func testNativeQueueClearRejectsEveryPendingWriteExactlyOnce() {
        let writes = CoreBluetoothWriteQueue(capacity: 2)
        var receipts: [CoreBluetoothWriteDisposition] = []
        _ = writes.submit(canSend: { false }, write: { XCTFail("Cancelled write executed") }, onReceipt: { receipts.append($0) })
        writes.clear()
        writes.clear()
        writes.flush { true }
        XCTAssertEqual(receipts, [.queued, .rejected])
    }

    func testGenericVescDefersRequestsUntilSubscriptionAndRejectsReplacementCallbacks() throws {
        try queue.sync {
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
            _ = state.connectionLinkEstablished(token: token)
            _ = state.observeConnectionNotification(token: token, bytes: vescReply)
            _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
            let sink = TransportSink()
            let transport = makeTransport(state, token: token, sink: sink)
            let step = try transport.handleLinkUp(at: MonotonicMilliseconds(1))
            XCTAssertEqual(step.connectionAttempt, token)
            XCTAssertFalse(sink.subscriptions.isEmpty)
            XCTAssertTrue(sink.writes.isEmpty)
            _ = state.beginConnectionAttempt(platformIdentifier: "B", nowMs: 2)
            XCTAssertEqual(step.connectionAttempt, token)
            transport.handleNotificationStateUpdate(channel: .vescNordicUartNotify, isNotifying: true, error: nil)
            XCTAssertTrue(sink.writes.isEmpty)
            XCTAssertThrowsError(try transport.handleTick(at: MonotonicMilliseconds(3)))
            XCTAssertTrue(sink.writes.isEmpty)
            transport.invalidate()
            XCTAssertEqual(sink.clears, 0, "retiring A must not clear B's native write queue")

        }
    }

    func testGenericTransportUsesOneSettingsOwnerAndOrdinaryVerificationMode() throws {
        try queue.sync {
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
            _ = state.connectionLinkEstablished(token: token)
            var frame = Data(repeating: 0, count: 42)
            frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
            frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
            _ = state.observeConnectionNotification(token: token, bytes: frame)
            _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
            let sink = TransportSink()
            let transport = makeTransport(state, token: token, sink: sink)
            _ = try transport.handleLinkUp(at: MonotonicMilliseconds(1))
            transport.handleNotificationStateUpdate(channel: .bluetooth16(0xffe1), isNotifying: true, error: nil)
            _ = try transport.handleNotification(bytes: frame, channel: .bluetooth16(0xffe1), at: MonotonicMilliseconds(2))
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            let controls = state.settings()
            let highBeam = try XCTUnwrap(controls.setting(for: .highBeam))
            XCTAssertEqual(controls.connection.token, token)
            XCTAssertEqual(controls.defaultChargeProfile?.profileId, 43)
            XCTAssertEqual(controls.defaultChargeProfile?.sessionId, token.generation)
            XCTAssertEqual(controls.defaultChargeProfile?.chargeFlowVerification, .unverified)
            XCTAssertEqual(highBeam.requested, .boolean(value: true))
            XCTAssertEqual(highBeam.status, .sentWithoutConfirmation)
            XCTAssertNil(highBeam.current)
            let count = sink.writes.count
            XCTAssertThrowsError(try transport.submitSetting(.tiltbackSpeed, value: .number(value: 99), at: MonotonicMilliseconds(4))) { error in
                XCTAssertEqual(error as? DeviceSettingSubmissionError, .InvalidValue)
            }
            XCTAssertEqual(sink.writes.count, count)
            _ = try transport.submitSetting(.tiltbackSpeed, value: .number(value: 400), at: MonotonicMilliseconds(5))
            XCTAssertEqual(sink.writes.count, count + 1)
            XCTAssertFalse(state.settings().validationAuthorized)
            transport.invalidate()

        }
    }

    func testAdapterDoesNotRetainItsOperationSink() throws {
        try queue.sync {
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
            var sink: TransportSink? = TransportSink()
            weak var reference: TransportSink?
            reference = sink
            let transport = makeTransport(state, token: token, sink: try XCTUnwrap(sink))
            sink = nil
            XCTAssertNil(reference)
            transport.invalidate()

        }
    }

    func testAlreadyEnabledNotificationsDoNotStrandSettingWrites() throws {
        try queue.sync {
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "NF2557", nowMs: 0).token)
            _ = state.connectionLinkEstablished(token: token)
            var frame = Data(repeating: 0, count: 42)
            frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
            frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
            _ = state.observeConnectionNotification(token: token, bytes: frame)
            _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
            let sink = TransportSink()
            let transport = makeTransport(state, token: token, sink: sink)
            defer { transport.invalidate() }
            // Detection already enabled this characteristic. The native sink
            // acknowledges the existing subscription without a new BLE callback.
            sink.onSubscribe = { [weak transport] channel in
                transport?.handleNotificationStateUpdate(channel: channel, isNotifying: true, error: nil)
            }
            _ = try transport.handleLinkUp(at: MonotonicMilliseconds(1))
            _ = try transport.handleNotification(bytes: frame, channel: .bluetooth16(0xffe1), at: MonotonicMilliseconds(2))
            let before = sink.writes.count
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))
            XCTAssertEqual(sink.writes.count, before + 1, "On must reach the transport, not remain queued behind subscription")
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: false), at: MonotonicMilliseconds(4))
            XCTAssertEqual(sink.writes.count, before + 2)
            let highBeam = try XCTUnwrap(state.settings().setting(for: .highBeam))
            XCTAssertEqual(highBeam.requested, .boolean(value: false))
            XCTAssertNil(highBeam.current, "A write-only setting must not invent readback")
        }
    }

    func testUnreadySubscriptionRejectsControlsWithoutDeferringThem() throws {
        try queue.sync {
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "NF2557", nowMs: 0).token)
            _ = state.connectionLinkEstablished(token: token)
            var frame = Data(repeating: 0, count: 42)
            frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
            frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
            _ = state.observeConnectionNotification(token: token, bytes: frame)
            _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
            let sink = TransportSink()
            let transport = makeTransport(state, token: token, sink: sink)
            defer { transport.invalidate() }
            _ = try transport.handleLinkUp(at: MonotonicMilliseconds(1))
            _ = try transport.handleNotification(bytes: frame, channel: .bluetooth16(0xffe1), at: MonotonicMilliseconds(2))
            let before = state.settings()
            XCTAssertThrowsError(try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))) {
                XCTAssertEqual($0 as? DeviceSettingSubmissionError, .ConnectionUnavailable)
            }
            XCTAssertThrowsError(try transport.submitAction(.horn, at: MonotonicMilliseconds(4))) {
                XCTAssertEqual($0 as? DeviceActionSubmissionError, .ConnectionUnavailable)
            }
            XCTAssertEqual(state.settings().settings, before.settings)
            XCTAssertEqual(state.settings().actions, before.actions)
            XCTAssertTrue(sink.writes.isEmpty)
            transport.handleNotificationStateUpdate(channel: .bluetooth16(0xffe1), isNotifying: true, error: nil)
            XCTAssertTrue(sink.writes.isEmpty, "The rejected setting command must not execute after subscription")
            XCTAssertNil(state.settings().setting(for: .highBeam)?.requested)
        }
    }

    func testDisabledNotificationReportsSubscriptionFailure() throws {
        try queue.sync {
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
            _ = state.connectionLinkEstablished(token: token)
            _ = state.observeConnectionNotification(token: token, bytes: vescReply)
            _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
            let sink = TransportSink()
            let transport = makeTransport(state, token: token, sink: sink)
            _ = try transport.handleLinkUp(at: MonotonicMilliseconds(1))
            var failedChannel: BluetoothUuid?
            transport.onSubscriptionFailure = { channel, error in
                XCTAssertNil(error)
                failedChannel = channel
            }
            transport.handleNotificationStateUpdate(
                channel: .vescNordicUartNotify,
                isNotifying: false,
                error: nil
            )
            XCTAssertEqual(failedChannel, .vescNordicUartNotify)
        }
    }
}

private final class TransportSink: CoreBluetoothOperationSink {
    var subscriptions: [BluetoothUuid] = []
    var writes: [Data] = []
    var clears = 0
    var onSubscribe: ((BluetoothUuid) -> Void)?
    var dispositions: [CoreBluetoothWriteDisposition] = []
    var receipts: [(CoreBluetoothWriteDisposition) -> Void] = []
    func subscribe(channel: BluetoothUuid) {
        subscriptions.append(channel)
        onSubscribe?(channel)
    }
    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data, onReceipt: @escaping (CoreBluetoothWriteDisposition) -> Void) -> CoreBluetoothWriteDisposition {
        writes.append(bytes)
        receipts.append(onReceipt)
        let disposition = dispositions.isEmpty ? .submitted : dispositions.removeFirst()
        onReceipt(disposition)
        return disposition
    }
    func disconnect() {}
    func peripheralIsReadyToSendWithoutResponse() {
        let pending = receipts
        receipts.removeAll()
        pending.forEach { $0(.submitted) }
    }
    func clearPendingWithoutResponseWrites() {
        clears += 1
        let pending = receipts
        receipts.removeAll()
        pending.forEach { $0(.rejected) }
    }
}
