import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutMobile

final class DeviceControlsTests: XCTestCase {
    func testUnknownConnectionHasNoInventedControls() {
        let state = CutoutSessionStateHandle()
        let attempt = state.beginConnectionAttempt(platformIdentifier: "unknown", nowMs: 0)
        let settings: DeviceSettingsDescriptorSnapshot = state.settingsDescriptors(validationMode: true)
        let actions: DeviceActionDescriptors = state.actionDescriptors(validationMode: true)
        XCTAssertEqual(settings.connection, attempt)
        XCTAssertEqual(actions.connection, attempt)
        XCTAssertTrue(settings.descriptors.isEmpty)
        XCTAssertTrue(actions.descriptors.isEmpty)
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
            channel: Data(), bytes: Data(), command: nil
        ))
        _ = state.ingestDeviceSession(token: token, input: MobileSessionInputDto(
            kind: .notification, monotonicMs: .init(milliseconds: 1), maxWriteLen: nil,
            channel: BluetoothUuid.bluetooth16(0xffe1).bytes, bytes: frame, command: nil
        ))
        let requested = DeviceSettingValue.boolean(value: true)
        _ = try state.submitSetting(token: token, id: .highBeam, value: requested, validationMode: false, monotonicMs: 2)
        let snapshot: DeviceSettingsSnapshot = state.settingsSnapshot()
        let highBeam = try XCTUnwrap(snapshot.settings.first { $0.id == .highBeam })
        XCTAssertEqual(snapshot.connection.token, token)
        XCTAssertEqual(highBeam.requested, requested)
        XCTAssertNil(highBeam.current)
        XCTAssertEqual(highBeam.status, .sentWithoutConfirmation)
        let replacement = state.beginConnectionAttempt(platformIdentifier: "B", nowMs: 3)
        XCTAssertThrowsError(try state.submitSetting(token: token, id: .highBeam, value: .boolean(value: false), validationMode: false, monotonicMs: 4)) { error in
            XCTAssertEqual(error as? DeviceSettingSubmissionError, .ConnectionUnavailable)
        }
        XCTAssertEqual(state.settingsSnapshot().connection, replacement)
        XCTAssertTrue(state.settingsSnapshot().settings.isEmpty)
    }
}
