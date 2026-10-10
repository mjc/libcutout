import CutoutMobile
import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

final class LiveRideInitialSnapshotTests: XCTestCase {
    @MainActor
    func testStartActionIsWithheldUntilInitialSnapshotReadSettles() {
        XCTAssertTrue(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: true,
                snapshotAllowedActions: nil
            ).isEmpty
        )
        XCTAssertEqual(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: false,
                snapshotAllowedActions: nil
            ),
            [.start]
        )
    }

    @MainActor
    func testInitialSnapshotReadSettlesAfterAnAuthoritativeNilResult() async throws {
        let state = MobileRideMapState()
        let queryStarted = expectation(description: "initial snapshot query started")
        let queryFinished = expectation(description: "initial snapshot query released")
        let query = HeldNilInitialSnapshotQuery(
            state: state,
            queryStarted: queryStarted,
            queryFinished: queryFinished
        )
        defer { query.releaseQuery.signal() }

        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .checking,
            now: { 1_000 }
        )
        XCTAssertEqual(model.initialSnapshotPhase, .pending)

        let restoration = model.restore()
        await fulfillment(of: [queryStarted], timeout: 3)
        XCTAssertEqual(model.initialSnapshotPhase, .pending)
        XCTAssertTrue(model.isInitialSnapshotPending)

        query.releaseQuery.signal()
        await fulfillment(of: [queryFinished], timeout: 3)
        await restoration?.value

        XCTAssertEqual(model.initialSnapshotPhase, .settled)
        XCTAssertFalse(model.isInitialSnapshotPending)
        XCTAssertNil(model.snapshot)
    }

    @MainActor
    func testNewCommandSnapshotSettlesBeforeHeldNilRestoreAndRejectsLateResult() async throws {
        let state = MobileRideMapState()
        let queryStarted = expectation(description: "initial snapshot query started")
        let queryFinished = expectation(description: "initial snapshot query released")
        let query = HeldNilInitialSnapshotQuery(
            state: state,
            queryStarted: queryStarted,
            queryFinished: queryFinished
        )
        defer { query.releaseQuery.signal() }

        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .checking,
            now: { 1_000 }
        )
        let restoration = model.restore()
        await fulfillment(of: [queryStarted], timeout: 3)
        XCTAssertTrue(model.isInitialSnapshotPending)

        let commandSnapshot = try state.startGpsOnly(atMs: 1_000)
        model.applyCommandSnapshot(commandSnapshot, resetPoints: true)
        XCTAssertEqual(model.initialSnapshotPhase, .settled)
        XCTAssertFalse(model.isInitialSnapshotPending)
        XCTAssertEqual(model.snapshot?.rideID, commandSnapshot.rideID)

        query.releaseQuery.signal()
        await fulfillment(of: [queryFinished], timeout: 3)
        await restoration?.value

        XCTAssertEqual(model.initialSnapshotPhase, .settled)
        XCTAssertEqual(model.snapshot?.rideID, commandSnapshot.rideID)
    }
}

private final class HeldNilInitialSnapshotQuery: LiveRideQuerying {
    let releaseQuery = DispatchSemaphore(value: 0)
    private let state: MobileRideMapState
    private let queryStarted: XCTestExpectation
    private let queryFinished: XCTestExpectation

    init(
        state: MobileRideMapState,
        queryStarted: XCTestExpectation,
        queryFinished: XCTestExpectation
    ) {
        self.state = state
        self.queryStarted = queryStarted
        self.queryFinished = queryFinished
    }

    func currentSnapshot() -> MobileRideMapSnapshotDto? {
        queryStarted.fulfill()
        releaseQuery.wait()
        queryFinished.fulfill()
        return nil
    }

    func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        state.currentSnapshot(atMs: atMs)
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        try state.projectStoredPoints(
            rideID: rideID,
            budget: budget,
            viewport: viewport,
            privacy: privacy,
            cancellation: cancellation
        )
    }

    func projectCurrentRoutePoints(
        budget: UInt32,
        rideID: String?,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        durableCancellation: MobileRideMapProjectionCancellation?,
        liveCancellation: MobileLiveRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        try state.projectCurrentRoutePoints(
            budget: budget,
            rideID: rideID,
            viewport: viewport,
            privacy: privacy,
            durableCancellation: durableCancellation,
            liveCancellation: liveCancellation
        )
    }
}
