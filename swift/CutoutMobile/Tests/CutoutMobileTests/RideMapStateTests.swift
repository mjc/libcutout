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
        let stopped = try state.stop()
        XCTAssertEqual(stopped.state, .stopped)

        guard case let .accepted(point, _) = decision else {
            return XCTFail("expected the location to be admitted")
        }
        XCTAssertEqual(point.sequence, 0)
        XCTAssertEqual(stopped.summary.pointCount, 1)
        XCTAssertEqual(state.pointsAfter(afterCursor: 0, limit: 10)?.points.count, 1)
        XCTAssertEqual(try state.save().state, .saved)
    }
}
