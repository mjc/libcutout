import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutMobile

final class ConnectionAttemptContractTests: XCTestCase {
    private let vescReply = Data([2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208, 3])

    func testSameDeviceRetryRejectsOldProtocolCallbacks() throws {
        let state = CutoutSessionStateHandle()
        let old = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
        let current = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 1).token)
        XCTAssertNil(state.observeConnectionNotification(token: old, bytes: vescReply))
        XCTAssertFalse(state.verifiedConnectionAttemptIsCurrent(token: old))
        XCTAssertEqual(state.connectionAttemptSnapshot().token, current)
        XCTAssertNil(state.deviceSessionSnapshot().identity)
    }

    func testMissingGattCallbackExpiresWithoutGrantingRideToLateSuccess() throws {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: token)
        _ = state.expireConnectionAttempt(token: token, nowMs: 15_000)
        _ = state.observeConnectionNotification(token: token, bytes: vescReply)
        let snapshot = state.resolveDeviceSession(token: token, identificationComplete: true, nowMs: 15_001)
        XCTAssertEqual(snapshot.connection.readiness, .recordOnly)
        XCTAssertEqual(snapshot.connection.transport, .connected)
        XCTAssertNil(snapshot.identity)
        XCTAssertFalse(state.verifiedConnectionAttemptIsCurrent(token: token))
    }

    func testValidatedProtocolKeepsUnknownVehicleKindAndLosesAdmissionOnDisconnect() throws {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
        _ = state.connectionLinkEstablished(token: token)
        _ = state.observeConnectionNotification(token: token, bytes: vescReply)
        let snapshot = state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
        XCTAssertEqual(snapshot.connection.readiness, .verified)
        XCTAssertEqual(snapshot.identity?.vehicleKind, .unknown)
        XCTAssertNil(snapshot.identity?.model)
        XCTAssertTrue(state.verifiedConnectionAttemptIsCurrent(token: token))
        _ = state.connectionLinkDown(token: token)
        XCTAssertFalse(state.verifiedConnectionAttemptIsCurrent(token: token))
        XCTAssertEqual(state.connectionAttemptSnapshot().transport, .disconnected)
    }

    func testImmutableSnapshotsCanBeReadFromBackgroundExecutor() async throws {
        let state = CutoutSessionStateHandle()
        let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "A", nowMs: 0).token)
        let before = await Task.detached { state.connectionAttemptSnapshot() }.value
        _ = state.beginConnectionAttempt(platformIdentifier: "B", nowMs: 1)
        let after = await Task.detached { state.connectionAttemptSnapshot() }.value
        XCTAssertEqual(before.token, token)
        XCTAssertEqual(before.token?.generation, before.generation)
        XCTAssertEqual(after.token?.generation, after.generation)
        XCTAssertGreaterThan(after.generation, before.generation)
        XCTAssertFalse(state.connectionAttemptIsCurrent(token: token))
    }
}
