import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutMobile

final class DeviceControlsTests: XCTestCase {
    func testUnknownConnectionHasNoInventedControls() {
        let state = CutoutSessionStateHandle()
        let attempt = state.beginConnectionAttempt(platformIdentifier: "unknown", nowMs: 0)
        let settings: DeviceSettingsDescriptorSnapshot = state.settingsDescriptors()
        let actions: DeviceActionDescriptors = state.actionDescriptors()
        XCTAssertEqual(settings.connection, attempt)
        XCTAssertEqual(actions.connection, attempt)
        XCTAssertTrue(settings.descriptors.isEmpty)
        XCTAssertTrue(actions.descriptors.isEmpty)
    }

    func testHornDoesNotNeedTelemetryOrStationaryArming() throws {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: token)
        var frame = Data(repeating: 0, count: 42)
        frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        _ = state.observeConnectionNotification(token: token, bytes: frame)
        _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
        _ = state.ingestDeviceSession(token: token, input: MobileSessionInputDto(
            kind: .linkUp, monotonicMs: .init(milliseconds: 1), maxWriteLen: nil,
            channel: Data(), bytes: Data()
        ))
        let result = try state.submitAction(token: token, id: .horn, monotonicMs: 2)
        XCTAssertNil(result.telemetry.speed)
        XCTAssertNil(result.result.error)
        XCTAssertTrue(result.result.outputs.contains { $0.kind == .write })
        let horn = try XCTUnwrap(state.actionsSnapshot().actions.first { $0.id == .horn })
        XCTAssertEqual(horn.status, .sentWithoutConfirmation)
    }

    func testSemanticSubmissionKeepsRequestedValueAndOwningAttempt() throws {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: token)
        var frame = Data(repeating: 0, count: 42)
        frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        _ = state.observeConnectionNotification(token: token, bytes: frame)
        _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
        _ = state.ingestDeviceSession(token: token, input: MobileSessionInputDto(
            kind: .linkUp, monotonicMs: .init(milliseconds: 1), maxWriteLen: nil,
            channel: Data(), bytes: Data()
        ))
        _ = state.ingestDeviceSession(token: token, input: MobileSessionInputDto(
            kind: .notification, monotonicMs: .init(milliseconds: 1), maxWriteLen: nil,
            channel: BluetoothUuid.bluetooth16(0xffe1).bytes, bytes: frame
        ))
        let requested = DeviceSettingValue.boolean(value: true)
        _ = try state.submitSetting(token: token, id: .highBeam, value: requested, monotonicMs: 2)
        let snapshot: DeviceSettingsSnapshot = state.settingsSnapshot()
        let highBeam = try XCTUnwrap(snapshot.setting(for: .highBeam))
        XCTAssertEqual(snapshot.connection.token, token)
        XCTAssertEqual(highBeam.requested, requested)
        XCTAssertNil(highBeam.current)
        XCTAssertEqual(highBeam.status, .sentWithoutConfirmation)
        let replacement = state.beginConnectionAttempt(platformIdentifier: "B", nowMs: 3)
        XCTAssertThrowsError(try state.submitSetting(token: token, id: .highBeam, value: .boolean(value: false), monotonicMs: 4)) { error in
            XCTAssertEqual(error as? DeviceSettingSubmissionError, .ConnectionUnavailable)
        }
        XCTAssertEqual(state.settingsSnapshot().connection, replacement)
        XCTAssertTrue(state.settingsSnapshot().settings.isEmpty)
    }

    func testValidationAuthorizationIsAttemptScopedAndAtomic() throws {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(
            state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token
        )
        _ = state.connectionLinkEstablished(token: token)
        var frame = Data(repeating: 0, count: 42)
        frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        _ = state.observeConnectionNotification(token: token, bytes: frame)
        _ = state.resolveDeviceSession(
            token: token,
            identificationComplete: false,
            nowMs: 1
        )

        let ordinary = state.deviceControlsSnapshot()
        XCTAssertFalse(ordinary.validationAuthorized)
        XCTAssertEqual(
            ordinary.descriptor(for: .pwmTiltback)?.access,
            .unverified
        )
        XCTAssertEqual(
            ordinary.descriptor(for: .pwmTiltback)?.id,
            .pwmTiltback
        )

        XCTAssertTrue(state.authorizeDeviceControls(token: token))
        let validation = state.deviceControlsSnapshot()
        XCTAssertTrue(validation.validationAuthorized)
        XCTAssertEqual(
            validation.descriptor(for: .pwmTiltback)?.access,
            .writable
        )

        let replacement = try XCTUnwrap(
            state.beginConnectionAttempt(platformIdentifier: "B", nowMs: 2).token
        )
        XCTAssertFalse(state.deviceControlsSnapshot().validationAuthorized)
        XCTAssertFalse(state.authorizeDeviceControls(token: token))

        _ = state.connectionLinkEstablished(token: replacement)
        _ = state.observeConnectionNotification(token: replacement, bytes: frame)
        _ = state.resolveDeviceSession(token: replacement, identificationComplete: false, nowMs: 3)
        XCTAssertTrue(state.authorizeDeviceControls(token: replacement))
    }

    func testSemanticSnapshotLookupDoesNotDependOnVendorNames() throws {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: token)
        var frame = Data(repeating: 0, count: 42)
        frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        _ = state.observeConnectionNotification(token: token, bytes: frame)
        _ = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)

        let controls = state.deviceControlsSnapshot()
        XCTAssertEqual(controls.descriptor(for: .highBeam)?.group, .interface)
        XCTAssertNil(controls.setting(for: .highBeam))
    }
}
