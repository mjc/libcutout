import CoreLocation
import CutoutMobileFFI
import XCTest

@testable import CutoutApp

final class ChargingLocationSampleSelectorTests: XCTestCase {
    func testKeepsMostAccurateFreshRealLocation() {
        let now = Date(timeIntervalSince1970: 1_000)
        var selector = ChargingLocationSampleSelector()
        let lessAccurate = location(latitude: 1, accuracy: 80, timestamp: now)
        let moreAccurate = location(latitude: 2, accuracy: 12, timestamp: now.addingTimeInterval(1))
        let equalAccuracyButNewer = location(latitude: 3, accuracy: 12, timestamp: now.addingTimeInterval(2))

        XCTAssertNotNil(selector.consider(lessAccurate, now: now))
        XCTAssertNotNil(selector.consider(moreAccurate, now: now.addingTimeInterval(1)))
        XCTAssertNotNil(selector.consider(equalAccuracyButNewer, now: now.addingTimeInterval(2)))
        XCTAssertEqual(selector.freshLocation(at: now.addingTimeInterval(2))?.coordinate.latitude, 3)
        XCTAssertEqual(selector.freshLocation(at: now.addingTimeInterval(2))?.horizontalAccuracy, 12)
    }

    func testMovementUsesCurrentFixInsteadOfKeepingAPreciseOldPosition() {
        let now = Date(timeIntervalSince1970: 1_000)
        var selector = ChargingLocationSampleSelector()
        _ = selector.consider(location(latitude: 1, accuracy: 3, timestamp: now), now: now)
        let nearbyCoarse = location(latitude: 1.00001, accuracy: 20, timestamp: now.addingTimeInterval(1))
        XCTAssertNil(selector.consider(nearbyCoarse, now: now.addingTimeInterval(1)))
        let movedCoarse = location(latitude: 1.01, accuracy: 20, timestamp: now.addingTimeInterval(2))
        XCTAssertNotNil(selector.consider(movedCoarse, now: now.addingTimeInterval(2)))
        XCTAssertEqual(selector.freshLocation(at: now.addingTimeInterval(2))?.coordinate.latitude, 1.01)
    }

    func testRejectsInvalidAndStaleLocationsAndReplacesExpiredBest() {
        let now = Date(timeIntervalSince1970: 2_000)
        var selector = ChargingLocationSampleSelector()
        let best = location(latitude: 1, accuracy: 5, timestamp: now)

        XCTAssertNotNil(selector.consider(best, now: now))
        XCTAssertNil(selector.consider(location(latitude: 2, accuracy: -1, timestamp: now), now: now))
        XCTAssertNil(
            selector.consider(location(latitude: 3, accuracy: 1, timestamp: now.addingTimeInterval(-121)), now: now))
        XCTAssertNil(selector.freshLocation(at: now.addingTimeInterval(121)))

        let current = location(latitude: 4, accuracy: 90, timestamp: now.addingTimeInterval(121))
        XCTAssertNotNil(selector.consider(current, now: now.addingTimeInterval(121)))
        XCTAssertEqual(selector.freshLocation(at: now.addingTimeInterval(121))?.coordinate.latitude, 4)
    }

    func testPhoneSampleReplacesCoarseFixAndUsesSourceTimestampWhenAvailable() {
        let now = Date(timeIntervalSince1970: 3_000)
        var selector = ChargingLocationSampleSelector()
        _ = selector.consider(location(latitude: 1, accuracy: 60, timestamp: now), now: now)
        let precisePhoneSample = phoneSample(
            latitude: 2,
            accuracy: 4,
            sourceTimestamp: now.addingTimeInterval(1).timeIntervalSince1970,
            wallClock: now.addingTimeInterval(1)
        )

        XCTAssertNotNil(selector.consider(precisePhoneSample, now: now.addingTimeInterval(1)))
        XCTAssertEqual(selector.freshLocation(at: now.addingTimeInterval(1))?.coordinate.latitude, 2)
        XCTAssertEqual(selector.freshLocation(at: now.addingTimeInterval(1))?.horizontalAccuracy, 4)

        let staleSource = phoneSample(
            latitude: 3,
            accuracy: 1,
            sourceTimestamp: now.addingTimeInterval(-121).timeIntervalSince1970,
            wallClock: now.addingTimeInterval(2)
        )
        XCTAssertNil(selector.consider(staleSource, now: now.addingTimeInterval(2)))
        XCTAssertEqual(selector.freshLocation(at: now.addingTimeInterval(2))?.coordinate.latitude, 2)
    }

    func testPhoneSampleFallsBackToWallClockTimestampAndRequiresAccuracy() {
        let now = Date(timeIntervalSince1970: 4_000)
        var selector = ChargingLocationSampleSelector()
        let sample = phoneSample(
            latitude: 5, accuracy: 8, sourceTimestamp: nil, wallClock: now
        )
        XCTAssertNotNil(selector.consider(sample, now: now))
        XCTAssertEqual(selector.freshLocation(at: now)?.coordinate.latitude, 5)

        let missingAccuracy = phoneSample(
            latitude: 6, accuracy: nil, sourceTimestamp: nil, wallClock: now
        )
        XCTAssertNil(selector.consider(missingAccuracy, now: now))
    }

    private func location(latitude: CLLocationDegrees, accuracy: CLLocationAccuracy, timestamp: Date) -> CLLocation {
        CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: latitude, longitude: 10),
            altitude: 0,
            horizontalAccuracy: accuracy,
            verticalAccuracy: 10,
            course: -1,
            speed: -1,
            timestamp: timestamp
        )
    }

    private func phoneSample(
        latitude: Double,
        accuracy: Double?,
        sourceTimestamp: Double?,
        wallClock: Date
    ) -> MobilePhoneLocationSampleDto {
        MobilePhoneLocationSampleDto(
            wallClockUnixMs: UInt64(wallClock.timeIntervalSince1970 * 1_000),
            sourceTimestampUnixSeconds: sourceTimestamp,
            latitudeDegrees: latitude,
            longitudeDegrees: 10,
            altitudeMeters: 0,
            horizontalAccuracyMeters: accuracy,
            verticalAccuracyMeters: nil,
            speedMetersPerSecond: nil,
            speedAccuracyMetersPerSecond: nil,
            courseDegrees: nil,
            courseAccuracyDegrees: nil
        )
    }
}
