import XCTest
@testable import CutoutMobile

final class RideUnitsTests: XCTestCase {
    func testSpeedReadoutKeepsValueAndUnitSeparate() {
        let readout = SpeedReadout(millimetersPerSecond: 12_070)

        XCTAssertEqual(readout.displayValue, "27.0")
        XCTAssertEqual(readout.displayUnit, "mph")
    }

    func testDistanceUnitFollowsMetricSpeedSpellings() {
        XCTAssertEqual(RideUnits.distanceUnit(forSpeedUnit: "km/h"), "km")
        XCTAssertEqual(RideUnits.distanceUnit(forSpeedUnit: "kmh"), "km")
        XCTAssertEqual(RideUnits.distanceUnit(forSpeedUnit: "kph"), "km")
        XCTAssertEqual(RideUnits.distanceUnit(forSpeedUnit: "mph"), "mi")
    }

    func testDistanceTextConvertsForTheDisplayedUnit() {
        XCTAssertEqual(RideUnits.distanceText(millimeters: 1_000_000, unit: "km"), "1.0")
        XCTAssertEqual(RideUnits.distanceText(millimeters: 1_609_344, unit: "mi"), "1.0")
        XCTAssertEqual(RideUnits.distanceText(millimetres: 1_000_000, unit: "km"), "1.0")
    }

    func testTemperatureUnitUsesDegreeSymbol() {
        XCTAssertEqual(RideUnits.temperatureText(millicelsius: 34_000), "34")
        XCTAssertEqual(RideUnits.temperatureUnit, "°C")
    }
}
