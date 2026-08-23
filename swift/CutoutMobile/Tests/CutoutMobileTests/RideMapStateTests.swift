import XCTest
@testable import CutoutMobile

final class RideMapStateTests: XCTestCase {
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
        let decision = try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        )
        XCTAssertEqual(try state.observeVehicleConnection(platformIdentifier: "pev-1", atMs: 200), .associated)

        guard case let .accepted(point, _) = decision else {
            return XCTFail("expected the location to be admitted")
        }
        XCTAssertEqual(
            try state.ingestLocation(
                monotonicMs: 2_000,
                wallClockUnixMs: 1_700_000_002_000,
                latitudeDegrees: 39.7393,
                longitudeDegrees: -104.9902,
                horizontalAccuracyMeters: 4
            ),
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
        let stopped = try state.stop()
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
}
