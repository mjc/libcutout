import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class PhoneLocationReadbackTests: XCTestCase {
    func testGpsFreshnessDetailsResolveFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("gps.detail.unavailable"), "GPS unavailable")
        XCTAssertEqual(pevLocalizedText("gps.detail.fresh"), "fresh GPS")
        XCTAssertEqual(pevLocalizedText("gps.detail.stale"), "stale GPS")
    }

    func testRustGpsSpeedUsesInjectedMonotonicFreshnessBoundaries() {
        let readback = PhoneLocationReadback(
            snapshot: snapshot(sampleTime: 10_000, speed: 2_500),
            receivedAt: MonotonicMilliseconds(10_000)
        )

        XCTAssertEqual(readback.speed.millimetersPerSecond, 2_500)
        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(11_999)), .fresh)
        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(12_000)), .fresh)
        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(12_001)), .stale)
        XCTAssertEqual(readback.detail(at: MonotonicMilliseconds(12_001)), "stale GPS")
    }

    func testMissingRustGpsSpeedIsExplicitlyUnavailable() {
        let readback = PhoneLocationReadback(snapshot: snapshot(sampleTime: 10_000, speed: nil))

        XCTAssertEqual(readback.freshness(at: MonotonicMilliseconds(10_001)), .unavailable)
        XCTAssertEqual(readback.speed.displayValue, "--")
        XCTAssertEqual(readback.detail(at: MonotonicMilliseconds(10_001)), "GPS unavailable")
    }

    private func snapshot(sampleTime: UInt64, speed: Int32?) -> MobilePhoneLocationSnapshotDto {
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: sampleTime,
            latitudeDegrees: 39.7,
            longitudeDegrees: -104.9,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: speed.map { Double($0) / 1_000 } ?? -1,
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
