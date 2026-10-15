import MapKit
import SwiftUI
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingMapCameraSnapshotTests: XCTestCase {
    func testSnapshotRetainsLatestCameraForMapReturn() {
        let snapshot = ChargingMapCameraSnapshot()
        snapshot.capture(
            MapCamera(
                centerCoordinate: CLLocationCoordinate2D(latitude: 1, longitude: 2),
                distance: 1_000,
                heading: 0,
                pitch: 0
            ))
        let expected = MapCamera(
            centerCoordinate: CLLocationCoordinate2D(latitude: 3, longitude: 4),
            distance: 2_500,
            heading: 30,
            pitch: 15
        )
        snapshot.capture(expected)

        XCTAssertEqual(snapshot.camera?.centerCoordinate.latitude, 3)
        XCTAssertEqual(snapshot.camera?.centerCoordinate.longitude, 4)
        XCTAssertEqual(snapshot.camera?.distance, 2_500)
        XCTAssertEqual(snapshot.camera?.heading, 30)
        XCTAssertEqual(snapshot.camera?.pitch, 15)
    }
}
