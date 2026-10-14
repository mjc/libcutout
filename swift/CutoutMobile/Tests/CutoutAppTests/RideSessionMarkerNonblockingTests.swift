import CutoutMobileFFI
import Foundation
import Synchronization
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

    @MainActor
    func testMarkerLoadAwaitsSQLiteWithoutBlockingItsCaller() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let store = RideSessionMarkerStore(database: fixture.database)
        let expected = Data([1, 2, 3])
        store.save(expected)
        await store.waitForPersistence()
        try await fixture.holdWrite()

        let completed = Mutex(false)
        let loadStarted = expectation(description: "Marker load started")
        let load = Task {
            loadStarted.fulfill()
            defer { completed.withLock { $0 = true } }
            return try await store.load()
        }
        await fulfillment(of: [loadStarted], timeout: 1)
        let callerProgressed = Mutex(false)
        let callerProgress = expectation(description: "The caller progresses while SQLite remains locked")
        Task { @MainActor in
            XCTAssertFalse(fixture.unlocked.withLock { $0 })
            XCTAssertFalse(completed.withLock { $0 }, "The marker read must remain suspended behind SQLite work")
            callerProgressed.withLock { $0 = true }
            callerProgress.fulfill()
        }
        await fulfillment(of: [callerProgress], timeout: 0.25)
        XCTAssertTrue(callerProgressed.withLock { $0 })
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        XCTAssertFalse(completed.withLock { $0 }, "The marker read must wait behind SQLite work")

        fixture.release.signal()
        let loaded = try await load.value
        XCTAssertEqual(loaded, expected)
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
