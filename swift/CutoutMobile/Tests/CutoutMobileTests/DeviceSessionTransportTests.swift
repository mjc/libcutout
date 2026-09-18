import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutMobile

final class DeviceSessionTransportTests: XCTestCase {
    private let queue = DispatchQueue(label: "cutout.generic-transport.test")
    private let vescReply = Data([2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208, 3])

    private func makeTransport(_ state: CutoutSessionStateHandle, token: ConnectionAttemptToken, sink: TransportSink) -> DeviceSessionTransport {
        DeviceSessionTransport(
            state: state, token: token,
            advertisement: CoreBluetoothAdvertisement(peripheralIdentifier: CoreBluetoothPeripheralIdentifier(token.platformIdentifier), localName: nil, advertisedServiceUuids: []),
            writeLimit: TransportWriteLimitBytes(20), operationSink: sink,
            queue: queue,
            clock: MonotonicClock(now: { MonotonicMilliseconds(1) })
        )
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
            let controls = state.deviceControlsSnapshot()
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
            XCTAssertFalse(state.deviceControlsSnapshot().validationAuthorized)
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

    func testAlreadyEnabledNotificationsDoNotStrandHeadlightWrites() throws {
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
            XCTAssertEqual(sink.writes.last, Data([0x4c, 0x6b, 0x41, 0x70, 0x0d, 0x01, 0x80, 0x80, 0x01, 0x57, 0xed, 0x3b, 0xd5]))
            _ = try transport.submitSetting(.highBeam, value: .boolean(value: false), at: MonotonicMilliseconds(4))
            XCTAssertEqual(sink.writes.count, before + 2)
            XCTAssertEqual(sink.writes.last, Data([0x4c, 0x6b, 0x41, 0x70, 0x0d, 0x01, 0x80, 0x80, 0x00, 0x20, 0xea, 0x0b, 0x43]))
            let highBeam = try XCTUnwrap(state.deviceControlsSnapshot().setting(for: .highBeam))
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
            let before = state.deviceControlsSnapshot()
            XCTAssertThrowsError(try transport.submitSetting(.highBeam, value: .boolean(value: true), at: MonotonicMilliseconds(3))) {
                XCTAssertEqual($0 as? DeviceSettingSubmissionError, .ConnectionUnavailable)
            }
            XCTAssertThrowsError(try transport.submitAction(.horn, at: MonotonicMilliseconds(4))) {
                XCTAssertEqual($0 as? DeviceActionSubmissionError, .ConnectionUnavailable)
            }
            XCTAssertEqual(state.deviceControlsSnapshot().settings, before.settings)
            XCTAssertEqual(state.deviceControlsSnapshot().actions, before.actions)
            XCTAssertTrue(sink.writes.isEmpty)
            transport.handleNotificationStateUpdate(channel: .bluetooth16(0xffe1), isNotifying: true, error: nil)
            XCTAssertFalse(sink.writes.contains(Data([0x4c, 0x6b, 0x41, 0x70, 0x0d, 0x01, 0x80, 0x80, 0x01, 0x57, 0xed, 0x3b, 0xd5])), "The rejected light command must not execute after subscription")
            XCTAssertNil(state.deviceControlsSnapshot().setting(for: .highBeam)?.requested)
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
    func subscribe(channel: BluetoothUuid) {
        subscriptions.append(channel)
        onSubscribe?(channel)
    }
    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) { writes.append(bytes) }
    func disconnect() {}
    func clearPendingWithoutResponseWrites() { clears += 1 }
}
