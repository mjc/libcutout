import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

final class RideSessionMarkerNonblockingTests: XCTestCase {
    @MainActor
    func testMarkerSaveReturnsBeforeSQLiteUnlocks() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let store = RideSessionMarkerStore(database: fixture.database)
        try await fixture.holdWrite()
        store.save(Data([1, 2, 3]))
        XCTAssertFalse(fixture.unlocked.withLock { $0 }, "Marker admission must not wait for durable SQL")
        fixture.release.signal()
        await store.waitForPersistence()
        try store.clear()
        await store.waitForPersistence()
        try await fixture.finish()
    }

    @MainActor
    func testMarkerClearReturnsBeforeSQLiteUnlocks() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let store = RideSessionMarkerStore(database: fixture.database)
        store.save(Data([1, 2, 3]))
        await store.waitForPersistence()
        try await fixture.holdWrite()
        try store.clear()
        XCTAssertFalse(fixture.unlocked.withLock { $0 }, "Marker clear admission must not wait for durable SQL")
        fixture.release.signal()
        await store.waitForPersistence()
        try store.clear()
        await store.waitForPersistence()
        try await fixture.finish()
    }

    func testBackgroundCheckpointDoesNotWaitForMarkerPersistence() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let state = CutoutSessionStateHandle()
        _ = try state.reduceRideSession(input: .start(platformIdentifier: "wheel"))
        let store = RideSessionMarkerStore(database: fixture.database)
        let coordinator = LiveActivityRideLifecycleCoordinator(
            manager: NoopMarkerLiveActivityManager(), sessionState: state, markerStore: store)
        try await fixture.holdWrite()
        let checkpoint = expectation(description: "Capture checkpoint proceeds before marker write settles")
        await coordinator.appDidEnterBackground(
            requestID: 1, atMs: 100,
            snapshot: LiveActivityRideSnapshot(
                identity: .device("wheel"),
                rideState: EucRideScreenState(phase: .live, displayState: RideDisplayState())),
            captureFlush: {
                XCTAssertFalse(fixture.unlocked.withLock { $0 }, "Marker persistence must not hold the checkpoint")
                checkpoint.fulfill()
                return true
            })
        await fulfillment(of: [checkpoint], timeout: 1)
        fixture.release.signal()
        await store.waitForPersistence()
        try store.clear()
        await store.waitForPersistence()
        try await fixture.finish()
    }
}

private actor NoopMarkerLiveActivityManager: LiveActivityRideLifecycleManaging {
    func start(
        snapshot _: LiveActivityRideSnapshot, rideSessionIdentity _: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds _: UInt64, freshnessExpiresAt _: Date
    ) async throws -> LiveActivityRideStartOutcome {
        .started(activityID: "marker-test")
    }
    func update(
        snapshot _: LiveActivityRideSnapshot, staleAfterMilliseconds _: UInt64,
        freshnessExpiresAt _: Date
    ) async throws -> LiveActivityRideUpdateOutcome { LiveActivityRideUpdateOutcome(activityID: "marker-test") }
    func end(reason _: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        LiveActivityRideEndOutcome(activityIDs: ["marker-test"])
    }
}
