import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class RideMapStateTests: XCTestCase {
    private func settle(
        _ state: MobileRideMapState,
        _ decision: MobileRideMapDecisionDto
    ) -> MobileRideMapDecisionDto {
        guard case .pending = decision else { return decision }
        for _ in 0 ..< 100 {
            if let terminal = state.pollLocationWrites().first {
                return terminal
            }
            Thread.sleep(forTimeInterval: 0.001)
        }
        XCTFail("timed out waiting for durable ride-map location outcome")
        return decision
    }

    func testStorageUnavailableStateCannotCreateAnInMemoryRide() {
        let state = MobileRideMapState(storageUnavailable: "database unavailable")

        XCTAssertEqual(state.initializationError, .Storage("database unavailable"))
        XCTAssertNil(state.currentSnapshot())
        XCTAssertEqual(state.pollLocationWrites(), [])
        XCTAssertThrowsError(
            try state.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .Storage("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.ingestLocation(
                monotonicMs: 100,
                wallClockUnixMs: 1_700_000_000_100,
                latitudeDegrees: 39.7392,
                longitudeDegrees: -104.9903,
                horizontalAccuracyMeters: 4
            )
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .Storage("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.storedHistoryPage(cursor: nil, limit: 10)
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .Storage("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.storedHistoryRide(rideID: "missing")
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .Storage("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.storedPointsAfter(rideId: "missing", afterCursor: nil, limit: 10)
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .Storage("database unavailable"))
        }
    }

    func testMapStateReportsTypedAdmissionReasons() throws {
        let state = MobileRideMapState()

        XCTAssertThrowsError(
            try state.ingestLocation(
                monotonicMs: 1,
                wallClockUnixMs: 1_700_000_000_001,
                latitudeDegrees: 39.7392,
                longitudeDegrees: -104.9903,
                horizontalAccuracyMeters: 4
            )
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .NoActiveRide)
        }

        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        _ = try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        )
        XCTAssertEqual(
            try state.ingestLocation(
                monotonicMs: 100,
                wallClockUnixMs: 1_700_000_000_100,
                latitudeDegrees: 39.7392,
                longitudeDegrees: -104.9903,
                horizontalAccuracyMeters: 4
            ),
            .ignored(reason: .duplicateLocation)
        )
        XCTAssertEqual(
            try state.ingestLocation(
                monotonicMs: 99,
                wallClockUnixMs: 1_700_000_000_099,
                latitudeDegrees: 39.7393,
                longitudeDegrees: -104.9902,
                horizontalAccuracyMeters: 4
            ),
            .rejected(reason: .timestampOutOfOrder)
        )
        XCTAssertEqual(
            try state.ingestLocation(
                monotonicMs: 102,
                wallClockUnixMs: 1_700_000_000_102,
                latitudeDegrees: 39.7393,
                longitudeDegrees: -104.9902,
                horizontalAccuracyMeters: 100_001
            ),
            .rejected(reason: .accuracyTooLow)
        )
    }

    func testMapStateKeepsLifecycleAndRouteProjectionTyped() throws {
        let state = MobileRideMapState()

        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: "pev-1")
        XCTAssertEqual(
            state.currentSnapshot(atMs: 1_100)?.summary.durationMilliseconds,
            1_000
        )
        let decision = settle(state, try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        ))
        XCTAssertEqual(try state.observeVehicleConnection(platformIdentifier: "pev-1", atMs: 200), .associated)

        guard case let .accepted(point, _) = decision else {
            return XCTFail("expected the location to be admitted")
        }
        XCTAssertEqual(
            settle(state, try state.ingestLocation(
                monotonicMs: 2_000,
                wallClockUnixMs: 1_700_000_002_000,
                latitudeDegrees: 39.7393,
                longitudeDegrees: -104.9902,
                horizontalAccuracyMeters: 4
            )),
            .accepted(
                point: MobileRideMapPointDto(
                    sequence: 1,
                    segmentId: 0,
                    latitudeDegrees: 39.7393,
                    longitudeDegrees: -104.9902,
                    wallClockUnixMs: 1_700_000_002_000,
                    monotonicMs: 2_000,
                    horizontalAccuracyMeters: 4,
                    telemetryState: .associatedNoTelemetry
                ),
                segmentStarted: false
            )
        )
        let stopped = try state.stop(atMs: 2_000)
        XCTAssertEqual(stopped.state, .stopped)
        XCTAssertEqual(point.sequence, 0)
        XCTAssertEqual(stopped.summary.pointCount, 2)
        let firstBatch = try state.pointsAfter(afterCursor: nil, limit: 1)
        XCTAssertEqual(firstBatch?.points.count, 1)
        XCTAssertEqual(firstBatch?.nextCursor, 0)
        XCTAssertTrue(firstBatch?.hasMore == true)
        XCTAssertEqual(try state.pointsAfter(afterCursor: firstBatch?.nextCursor, limit: 10)?.points.map(\.sequence), [1])
        XCTAssertEqual(try state.save().state, .saved)
    }

    func testMapStateProjectsBoundedLiveRouteAndRejectsInvalidBudget() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        for (monotonicMs, latitudeDegrees) in [
            (1_001, 40.0),
            (2_001, 40.0001),
            (3_001, 40.0002),
            (4_001, 40.0003),
        ] {
            _ = settle(state, try state.ingestLocation(
                monotonicMs: UInt64(monotonicMs),
                wallClockUnixMs: 1_700_000_000_000 + UInt64(monotonicMs),
                latitudeDegrees: latitudeDegrees,
                longitudeDegrees: -105.0,
                horizontalAccuracyMeters: 3
            ))
        }

        let projection = try state.projectPoints(
            budget: 2,
            viewport: MobileGeoBoundsDto(
                minimumLatitudeDegrees: 40.0,
                maximumLatitudeDegrees: 40.0002,
                minimumLongitudeDegrees: -105.0,
                maximumLongitudeDegrees: -105.0
            ),
            privacy: .grid(e7: 1_000)
        )
        XCTAssertEqual(projection.sourcePointCount, 4)
        XCTAssertEqual(projection.sourceSegmentCount, 1)
        XCTAssertEqual(projection.candidateSegmentCount, 1)
        XCTAssertEqual(projection.displayedSegmentCount, 1)
        XCTAssertFalse(projection.segmentsOmittedByBudget)
        XCTAssertEqual(projection.points.map(\.sequence), [0, 2])
        XCTAssertEqual(projection.points.map(\.privacyClass), [.gridRedacted, .gridRedacted])
        XCTAssertEqual(projection.points.map(\.latitudeDegrees), [40.0, 40.0002])

        XCTAssertThrowsError(try state.projectPoints(budget: 0)) { error in
            XCTAssertEqual(error as? MobileRideMapError, .InvalidRouteProjection)
        }
    }

    func testStoredProjectionHonorsRustCancellationToken() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        _ = settle(state, try state.ingestLocation(
            monotonicMs: 1_001,
            wallClockUnixMs: 1_700_000_001_001,
            latitudeDegrees: 40,
            longitudeDegrees: -105,
            horizontalAccuracyMeters: 3
        ))
        let rideID = try state.stop(atMs: 2_000).rideId
        _ = try state.save()

        let cancellation = MobileRideMapProjectionCancellation()
        cancellation.cancel()
        XCTAssertThrowsError(
            try state.projectStoredPoints(
                rideID: rideID,
                budget: 1,
                cancellation: cancellation
            )
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .cancelled)
        }
    }

    func testMapStateLatestRoutePointsReturnsTheRustBoundedTail() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        for offset in 0 ..< 4_100 {
            let monotonicMs = UInt64(1_001 + offset)
            _ = settle(state, try state.ingestLocation(
                monotonicMs: monotonicMs,
                wallClockUnixMs: 1_700_000_000_000 + monotonicMs,
                latitudeDegrees: 40.0,
                longitudeDegrees: -105.0,
                horizontalAccuracyMeters: 3
            ))
        }

        let tail = try state.latestRoutePoints()

        XCTAssertEqual(tail?.points.count, 4_096)
        XCTAssertEqual(tail?.points.first?.sequence, 4)
        XCTAssertEqual(tail?.points.last?.sequence, 4_099)
        XCTAssertEqual(tail?.hasMore, false)
    }
}
