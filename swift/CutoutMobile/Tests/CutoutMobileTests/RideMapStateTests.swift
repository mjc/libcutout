import XCTest
@testable import CutoutMobile

final class RideMapStateTests: XCTestCase {
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
