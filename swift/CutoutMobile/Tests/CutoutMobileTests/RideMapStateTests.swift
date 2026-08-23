import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutMobile

final class RideMapStateTests: XCTestCase {
    func testMapStateUsesMainRustDatabaseForLifecycleAndRoutePoints() throws {
        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("cutout-map-\(UUID().uuidString).sqlite")
        let database = try openRideDatabase(path: path.path)
        defer {
            try? database.shutdown()
            try? FileManager.default.removeItem(at: path)
        }
        let state = MobileRideMapState(database: database)

        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: "pev-1")
        let decision = try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        )
        XCTAssertEqual(try state.observeVehicleConnection(platformIdentifier: "pev-1", atMs: 200), .associated)
        XCTAssertEqual(try state.stop().state, .stopped)
        _ = try state.save()

        guard case let .accepted(point, _) = decision else {
            return XCTFail("expected the location to be admitted")
        }
        XCTAssertEqual(point.sequence, 0)
        let summaries = try state.storedSummaries(limit: 10)
        XCTAssertEqual(summaries.count, 1)
        XCTAssertEqual(summaries[0].summary.pointCount, 1)
        let route = try state.storedPointsAfter(rideId: summaries[0].rideId, afterCursor: 0, limit: 10)
        XCTAssertEqual(route?.points.count, 1)
        XCTAssertEqual(route?.points.first?.latitudeDegrees, 39.7392)
    }
}
