import CutoutMobile
import CutoutMobileFFI
import SwiftUI
import XCTest

@testable import CutoutApp

#if os(macOS)
    import AppKit
#endif

@MainActor
final class EucRideInstrumentTests: XCTestCase {
    func testOrdinaryRideFixturesDoNotCreateSystemLiveActivities() {
        for fixture in [CutoutUITestSessionFixture.euc, .dynamicEuc, .staleEuc, .vesc, .unknownDevice] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is CutoutUITestLiveActivityManager)
        }
        for fixture in [
            CutoutUITestSessionFixture.autoVescLiveActivity, .autoDynamicVescLiveActivity,
            .autoCriticalVescLiveActivity, .autoUnavailableVescLiveActivity, .autoStaleVescLiveActivity,
        ] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is LiveActivityRideActivityKitManager)
        }
    }

    func testIsolatedLiveActivityPreservesLifecycleReceiptsWithoutSystemEffects() async throws {
        let manager = CutoutUITestSessionFixture.euc.makeLiveActivityManager()
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(phase: .live, displayState: RideDisplayState())
        )
        let identity = LiveActivityRideSessionIdentity(platformIdentifier: "fixture", sessionID: "ride")
        let started = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(started, .started(activityID: "ui-test-ride"))
        let adopted = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(adopted, .adopted(activityID: "ui-test-ride"))
        let updated = try await manager.update(
            snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(updated.activityID, "ui-test-ride")
        let ended = try await manager.end(reason: .disconnected)
        XCTAssertEqual(ended.activityIDs, ["ui-test-ride"])
        let endedAgain = try await manager.end(reason: .disconnected)
        XCTAssertTrue(endedAgain.activityIDs.isEmpty)
        do {
            _ = try await manager.update(
                snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
            )
            XCTFail("An ended fixture must not acknowledge another activity update")
        } catch {
            XCTAssertEqual(error as? LiveActivityRideLifecycleError, .activityUnavailable)
        }
    }

}
