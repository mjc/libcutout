import CoreLocation
import CutoutMobileFFI
import Foundation

struct ChargingLocationSampleSelector {
    static let maximumAge: TimeInterval = 120

    private(set) var bestLocation: CLLocation?

    @discardableResult
    mutating func consider(_ location: CLLocation, now: Date = Date()) -> CLLocation? {
        guard location.horizontalAccuracy.isFinite, location.horizontalAccuracy >= 0,
            CLLocationCoordinate2DIsValid(location.coordinate),
            isFresh(location, at: now)
        else { return nil }

        if let bestLocation, isFresh(bestLocation, at: now) {
            let movedBeyondAccuracy =
                location.timestamp > bestLocation.timestamp
                && location.distance(from: bestLocation)
                    > bestLocation.horizontalAccuracy + location.horizontalAccuracy
            let morePrecise = location.horizontalAccuracy < bestLocation.horizontalAccuracy
            let equallyPreciseAndNewer =
                location.horizontalAccuracy == bestLocation.horizontalAccuracy
                && location.timestamp > bestLocation.timestamp
            guard movedBeyondAccuracy || morePrecise || equallyPreciseAndNewer else { return nil }
        }
        bestLocation = location
        return location
    }

    @discardableResult
    mutating func consider(
        _ sample: MobilePhoneLocationSampleDto,
        now: Date = Date()
    ) -> CLLocation? {
        let timestamp = sample.sourceTimestampUnixSeconds ?? Double(sample.wallClockUnixMs) / 1_000
        guard let accuracy = sample.horizontalAccuracyMeters, accuracy.isFinite, accuracy >= 0,
            sample.latitudeDegrees.isFinite, sample.longitudeDegrees.isFinite,
            timestamp.isFinite
        else { return nil }
        guard
            CLLocationCoordinate2DIsValid(
                CLLocationCoordinate2D(
                    latitude: sample.latitudeDegrees, longitude: sample.longitudeDegrees
                ))
        else { return nil }
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
        return consider(location, now: now)
    }

    func freshLocation(at date: Date = Date()) -> CLLocation? {
        guard let bestLocation, isFresh(bestLocation, at: date) else { return nil }
        return bestLocation
    }

    private func isFresh(_ location: CLLocation, at date: Date) -> Bool {
        let age = date.timeIntervalSince(location.timestamp)
        return age >= 0 && age <= Self.maximumAge
    }
}
