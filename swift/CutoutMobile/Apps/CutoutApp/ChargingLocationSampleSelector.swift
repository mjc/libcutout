import CoreLocation
import CutoutMobileFFI
import Foundation

struct ChargingLocationSampleSelector {
    private(set) var bestLocation: CLLocation?

    @discardableResult
    mutating func consider(_ location: CLLocation, now: Date = Date()) -> CLLocation? {
        selectCandidate(location, now: now)
    }

    @discardableResult
    mutating func consider(
        _ sample: MobilePhoneLocationSampleDto,
        now: Date = Date()
    ) -> CLLocation? {
        let timestamp = sample.sourceTimestampUnixSeconds ?? Double(sample.wallClockUnixMs) / 1_000
        guard let accuracy = sample.horizontalAccuracyMeters else { return nil }
        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(
                latitude: sample.latitudeDegrees, longitude: sample.longitudeDegrees
            ),
            altitude: sample.altitudeMeters,
            horizontalAccuracy: accuracy,
            verticalAccuracy: sample.verticalAccuracyMeters ?? -1,
            course: sample.courseDegrees ?? -1,
            speed: sample.speedMetersPerSecond ?? -1,
            timestamp: Date(timeIntervalSince1970: timestamp)
        )
        return selectCandidate(location, now: now)
    }

    func freshLocation(at date: Date = Date()) -> CLLocation? {
        guard let bestLocation,
            mobileFreshChargingLocationFix(
                fix: fix(from: bestLocation),
                nowUnixSeconds: date.timeIntervalSince1970
            ) != nil
        else { return nil }
        return bestLocation
    }

    private mutating func selectCandidate(_ location: CLLocation, now: Date) -> CLLocation? {
        guard
            mobileSelectChargingLocationFix(
                current: bestLocation.map { fix(from: $0) },
                candidate: fix(from: location),
                nowUnixSeconds: now.timeIntervalSince1970
            ) != nil
        else { return nil }
        bestLocation = location
        return location
    }

    private func fix(from location: CLLocation) -> MobileChargingLocationFixDto {
        MobileChargingLocationFixDto(
            latitudeDegrees: location.coordinate.latitude,
            longitudeDegrees: location.coordinate.longitude,
            sourceTimestampUnixSeconds: location.timestamp.timeIntervalSince1970,
            horizontalAccuracyMeters: location.horizontalAccuracy
        )
    }
}
