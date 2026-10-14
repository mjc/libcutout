import CutoutMobileFFI
import MapKit
import XCTest

@testable import CutoutApp

final class ChargingMapCameraTests: XCTestCase {
    func testNormalCoordinatesHavePaddedRegionContainingBothPoints() {
        let origin = coordinate(latitude: 37.7749, longitude: -122.4194)
        let target = coordinate(latitude: 37.7849, longitude: -122.3994)

        let region = ChargingMapCamera.regionIncluding(origin: origin, target: target)

        XCTAssertTrue(region.span.latitudeDelta > abs(target.latitudeDegrees - origin.latitudeDegrees))
        XCTAssertTrue(region.span.longitudeDelta > abs(target.longitudeDegrees - origin.longitudeDegrees))
        XCTAssertTrue(contains(origin, in: region))
        XCTAssertTrue(contains(target, in: region))
    }

    func testSameCoordinateProducesFiniteNonzeroRegion() {
        let point = coordinate(latitude: 37.7749, longitude: -122.4194)

        let region = ChargingMapCamera.regionIncluding(origin: point, target: point)

        XCTAssertTrue(region.span.latitudeDelta.isFinite)
        XCTAssertTrue(region.span.longitudeDelta.isFinite)
        XCTAssertGreaterThan(region.span.latitudeDelta, 0)
        XCTAssertGreaterThan(region.span.longitudeDelta, 0)
        XCTAssertTrue(contains(point, in: region))
    }

    func testAntimeridianCoordinatesUseNarrowRegionContainingBothPoints() {
        let origin = coordinate(latitude: 10, longitude: 179.8)
        let target = coordinate(latitude: 10.1, longitude: -179.8)

        let region = ChargingMapCamera.regionIncluding(origin: origin, target: target)

        XCTAssertLessThan(region.span.longitudeDelta, 1)
        XCTAssertTrue(abs(region.center.longitude) > 179)
        XCTAssertTrue(contains(origin, in: region))
        XCTAssertTrue(contains(target, in: region))
    }

    private func coordinate(latitude: Double, longitude: Double) -> MobileMapCoordinateDto {
        MobileMapCoordinateDto(latitudeDegrees: latitude, longitudeDegrees: longitude)
    }

    private func contains(_ point: MobileMapCoordinateDto, in region: MKCoordinateRegion) -> Bool {
        let latitudeDistance = abs(point.latitudeDegrees - region.center.latitude)
        let longitudeDistance = abs(
            (point.longitudeDegrees - region.center.longitude + 540).truncatingRemainder(dividingBy: 360) - 180)
        return latitudeDistance <= region.span.latitudeDelta / 2
            && longitudeDistance <= region.span.longitudeDelta / 2
    }
}
