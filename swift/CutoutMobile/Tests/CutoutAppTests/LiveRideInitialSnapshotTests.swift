import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

final class LiveRideInitialSnapshotTests: XCTestCase {
    @MainActor
    func testRideControlsDistinguishPendingEmptyFailedAndAuthoritativeSnapshot() throws {
        let pendingModel = LiveRideModel(
            state: MobileRideMapState(),
            storageError: nil,
            availability: .checking,
            now: { 1_000 }
        )
        XCTAssertTrue(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: pendingModel.isInitialSnapshotPending,
                availability: pendingModel.availability,
                storageError: pendingModel.storageError,
                snapshotAllowedActions: nil
            ).isEmpty
        )
        XCTAssertEqual(
            RideMapLiveContentView.emptySnapshotStatusKey(
                isInitialSnapshotPending: pendingModel.isInitialSnapshotPending,
                availability: pendingModel.availability,
                storageError: pendingModel.storageError
            ),
            "ride_map.initial_snapshot_loading"
        )

        let failedModel = LiveRideModel(
            state: nil,
            storageError: "test database open failure",
            availability: .ready,
            now: { 1_000 }
        )
        XCTAssertTrue(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: false,
                availability: failedModel.availability,
                storageError: failedModel.storageError,
                snapshotAllowedActions: nil
            ).isEmpty
        )
        XCTAssertEqual(
            RideMapLiveContentView.emptySnapshotStatusKey(
                isInitialSnapshotPending: false,
                availability: failedModel.availability,
                storageError: failedModel.storageError
            ),
            "ride_map.persistence_unavailable"
        )

        XCTAssertTrue(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: false,
                availability: .ready,
                storageError: nil,
                mapError: .storageError("test database query failure"),
                snapshotAllowedActions: nil
            ).isEmpty
        )
        XCTAssertEqual(
            RideMapLiveContentView.emptySnapshotStatusKey(
                isInitialSnapshotPending: false,
                availability: .ready,
                storageError: nil,
                mapError: .storageError("test database query failure")
            ),
            "ride_map.persistence_unavailable"
        )

        let unavailableModel = LiveRideModel(
            state: nil,
            storageError: nil,
            availability: .storageUnavailable,
            now: { 1_000 }
        )
        XCTAssertTrue(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: false,
                availability: unavailableModel.availability,
                storageError: unavailableModel.storageError,
                snapshotAllowedActions: nil
            ).isEmpty
        )

        let readyEmptyModel = LiveRideModel(
            state: nil,
            storageError: nil,
            availability: .ready,
            now: { 1_000 }
        )
        XCTAssertEqual(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: readyEmptyModel.isInitialSnapshotPending,
                availability: readyEmptyModel.availability,
                storageError: readyEmptyModel.storageError,
                snapshotAllowedActions: nil
            ),
            [.start]
        )
        XCTAssertEqual(
            RideMapLiveContentView.emptySnapshotStatusKey(
                isInitialSnapshotPending: readyEmptyModel.isInitialSnapshotPending,
                availability: readyEmptyModel.availability,
                storageError: readyEmptyModel.storageError
            ),
            "ride_map.no_active"
        )

        let state = MobileRideMapState()
        let snapshot = try state.startGpsOnly(atMs: 1_000)
        let snapshotModel = LiveRideModel(
            state: state,
            storageError: "a later storage warning",
            availability: .storageUnavailable,
            now: { 1_000 }
        )
        snapshotModel.applySnapshot(snapshot)
        XCTAssertEqual(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: snapshotModel.isInitialSnapshotPending,
                availability: snapshotModel.availability,
                storageError: snapshotModel.storageError,
                snapshotAllowedActions: snapshotModel.snapshot?.allowedActions
            ),
            snapshot.allowedActions
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
    func testStorageUnavailableStateSettlesNilWithoutOfferingStart() async {
        let failure = "test database open failure"
        let state = MobileRideMapState(storageUnavailable: failure)
        let model = LiveRideModel(
            state: state,
            storageError: failure,
            availability: .storageUnavailable,
            now: { 1_000 }
        )
        XCTAssertTrue(model.isInitialSnapshotPending)

        let restoration = model.restore()
        await restoration?.value

        XCTAssertFalse(model.isInitialSnapshotPending)
        XCTAssertNil(model.snapshot)
        XCTAssertTrue(
            RideMapLiveContentView.allowedControlActions(
                isInitialSnapshotPending: model.isInitialSnapshotPending,
                availability: model.availability,
                storageError: model.storageError,
                mapError: model.error,
                snapshotAllowedActions: model.snapshot?.allowedActions
            ).isEmpty
        )
        XCTAssertEqual(
            RideMapLiveContentView.emptySnapshotStatusKey(
                isInitialSnapshotPending: model.isInitialSnapshotPending,
                availability: model.availability,
                storageError: model.storageError,
                mapError: model.error
            ),
            "ride_map.persistence_unavailable"
        )
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
