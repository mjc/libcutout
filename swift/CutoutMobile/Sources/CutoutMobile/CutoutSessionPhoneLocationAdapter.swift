import CoreLocation
import Foundation
import CutoutMobileFFI

struct PhoneLocationUpdate {
    let receiptMonotonic: MonotonicMilliseconds
    let receiptWallClock: Date
    let samples: [MobilePhoneLocationSampleDto]
}

protocol CutoutSessionPhoneLocationAdapting: AnyObject {
    var latestSample: MobilePhoneLocationSampleDto? { get }
    var authorizationStatus: CLAuthorizationStatus { get }
    func start()
    func clear()
    func updateDemand(_ demanded: Bool)
    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager)
    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation])
}

/// Owns Core Location lifetime and phone-sample admission. Ride-map persistence remains in Core.
final class CutoutSessionPhoneLocationAdapter: NSObject, CLLocationManagerDelegate, CutoutSessionPhoneLocationAdapting {
    private let clock: MonotonicClock
    private let wallClock: () -> Date
    private let onSnapshot: (MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void
    private let onLocationUpdate: (PhoneLocationUpdate) -> Void
    private let onAvailabilityChange: () -> Void
    private var locationUpdatesDemanded = false
    private var locationManagerUpdatesStarted = false
    private var didRequestWhenInUseAuthorization = false
    private var state = MobilePhoneLocationState()

    private lazy var locationManager: CLLocationManager = {
        let manager = CLLocationManager()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBestForNavigation
        manager.activityType = .fitness
        return manager
    }()

    var latestSample: MobilePhoneLocationSampleDto? {
        state.currentSnapshot().latestSample
    }

    var authorizationStatus: CLAuthorizationStatus {
        locationManager.authorizationStatus
    }

    init(
        clock: MonotonicClock,
        wallClock: @escaping () -> Date,
        onSnapshot: @escaping (MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void,
        onLocationUpdate: @escaping (PhoneLocationUpdate) -> Void,
        onAvailabilityChange: @escaping () -> Void
    ) {
        self.clock = clock
        self.wallClock = wallClock
        self.onSnapshot = onSnapshot
        self.onLocationUpdate = onLocationUpdate
        self.onAvailabilityChange = onAvailabilityChange
    }

    func start() {
        _ = locationManager
    }

    func clear() {
        state.clear()
    }

    func updateDemand(_ demanded: Bool) {
        locationUpdatesDemanded = demanded
#if os(iOS)
        locationManager.allowsBackgroundLocationUpdates = demanded
#endif

        guard demanded else {
            stopUpdatesIfNeeded()
            return
        }

        guard CLLocationManager.locationServicesEnabled() else {
            onAvailabilityChange()
            return
        }

        switch locationManager.authorizationStatus {
        case .notDetermined:
            guard !didRequestWhenInUseAuthorization else { return }
            didRequestWhenInUseAuthorization = true
            locationManager.requestWhenInUseAuthorization()
        case .authorizedAlways, .authorizedWhenInUse:
            locationManager.startUpdatingLocation()
            locationManagerUpdatesStarted = true
        case .denied, .restricted:
            stopUpdatesIfNeeded()
            onAvailabilityChange()
        @unknown default:
            stopUpdatesIfNeeded()
            onAvailabilityChange()
        }
    }

    func locationManagerDidChangeAuthorization(_: CLLocationManager) {
        onAvailabilityChange()
        updateDemand(locationUpdatesDemanded)
    }

    func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard !locations.isEmpty else { return }

        let receiptMonotonic = clock.now()
        let samples = locations.compactMap(MobilePhoneLocationSampleDto.init(location:))
        guard !samples.isEmpty else { return }

        for sample in samples {
            _ = state.ingest(sample: sample)
        }
        onSnapshot(state.currentSnapshot(), receiptMonotonic)
        onLocationUpdate(PhoneLocationUpdate(
            receiptMonotonic: receiptMonotonic,
            receiptWallClock: wallClock(),
            samples: samples
        ))
    }

    private func stopUpdatesIfNeeded() {
        guard locationManagerUpdatesStarted else { return }
        locationManager.stopUpdatingLocation()
        locationManagerUpdatesStarted = false
    }
}

private extension MobilePhoneLocationSampleDto {
    init?(location: CLLocation) {
        let timestamp = location.timestamp.timeIntervalSince1970 * 1_000
        guard timestamp.isFinite, timestamp > 0, timestamp < Double(UInt64.max) else { return nil }
        let coordinate = location.coordinate
        guard coordinate.latitude.isFinite,
              coordinate.longitude.isFinite,
              location.altitude.isFinite
        else { return nil }

        self.init(
            wallClockUnixMs: UInt64(timestamp.rounded(.down)),
            latitudeDegrees: coordinate.latitude,
            longitudeDegrees: coordinate.longitude,
            altitudeMeters: location.altitude,
            horizontalAccuracyMeters: Self.nonNegativeFinite(location.horizontalAccuracy),
            verticalAccuracyMeters: Self.nonNegativeFinite(location.verticalAccuracy),
            speedMetersPerSecond: Self.nonNegativeFinite(location.speed),
            speedAccuracyMetersPerSecond: Self.nonNegativeFinite(location.speedAccuracy),
            courseDegrees: Self.nonNegativeFinite(location.course),
            courseAccuracyDegrees: Self.nonNegativeFinite(location.courseAccuracy)
        )
    }

    private static func nonNegativeFinite(_ value: CLLocationDistance) -> Double? {
        value.isFinite && value >= 0 ? value : nil
    }
}
