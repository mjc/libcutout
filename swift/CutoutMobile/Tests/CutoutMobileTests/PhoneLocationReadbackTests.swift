import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class PhoneLocationReadbackTests: XCTestCase {
    func testGpsFreshnessDetailsResolveFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("gps.detail.unavailable"), "GPS unavailable")
        XCTAssertEqual(pevLocalizedText("gps.detail.fresh"), "fresh GPS")
        XCTAssertEqual(pevLocalizedText("gps.detail.stale"), "stale GPS")
    }

    func testTelemetryFreshnessUsesOneSharedTwoSecondThreshold() {
        XCTAssertEqual(RideTelemetryFreshnessPolicy.staleAfter, MonotonicMilliseconds(2_000))
    }

    func testRustGpsSpeedUsesInjectedMonotonicFreshnessBoundaries() {
        let readback = PhoneLocationReadback(
            snapshot: snapshot(sampleTime: 10_000, speed: 2_500),
            receivedAt: MonotonicMilliseconds(10_000)
        )

        XCTAssertEqual(readback.speed.millimetersPerSecond, 2_500)
        XCTAssertEqual(readback.speedUnit, RideUnits.speedUnit)
        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(11_999)), .fresh)
        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(12_000)), .fresh)
        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(12_001)), .stale)
        XCTAssertEqual(readback.detail(at: MonotonicMilliseconds(12_001)), "stale GPS")
    }

    func testGpsFreshnessUsesTheSharedRideUpdateAgeType() {
        let readback = PhoneLocationReadback(
            snapshot: snapshot(sampleTime: 10_000, speed: 2_500),
            receivedAt: MonotonicMilliseconds(10_000)
        )

        let age = rideUpdateAge(
            updatedAt: MonotonicMilliseconds(10_000),
            at: MonotonicMilliseconds(12_001),
            staleAfter: RideTelemetryFreshnessPolicy.staleAfter
        )

        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(12_001)), age.freshness)
    }

    func testMissingRustGpsSpeedIsExplicitlyUnavailable() {
        let readback = PhoneLocationReadback(snapshot: snapshot(sampleTime: 10_000, speed: nil))

        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(10_001)), .unavailable)
        XCTAssertEqual(readback.speed.displayValue, "--")
        XCTAssertEqual(readback.speedUnit, "")
        XCTAssertEqual(readback.detail(at: MonotonicMilliseconds(10_001)), "GPS unavailable")
    }

    func testCoreLocationSampleExposesUnitTypedMetrics() {
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_000,
            latitudeDegrees: 39.7,
            longitudeDegrees: -104.9,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )

        XCTAssertEqual(sample.latitudeDegrees, 39.7)
        XCTAssertEqual(sample.longitudeDegrees, -104.9)
        XCTAssertEqual(sample.altitudeMeters, 1_600)
        XCTAssertEqual(sample.horizontalAccuracyMeters, 4)
        XCTAssertEqual(sample.speedMetersPerSecond, 2)
        XCTAssertEqual(sample.courseDegrees, 90)
    }

    private func snapshot(sampleTime: UInt64, speed: Int32?) -> MobilePhoneLocationSnapshotDto {
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: sampleTime,
            latitudeDegrees: 39.7,
            longitudeDegrees: -104.9,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: speed.map { Double($0) / 1_000 },
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )
        let reading = speed.map {
            SpeedReading(
                value: Speed(value: $0),
                source: .reported,
                quality: .known,
                verification: .unverified
            )
        }
        return MobilePhoneLocationSnapshotDto(latestSample: sample, gpsSpeed: reading)
    }
}
