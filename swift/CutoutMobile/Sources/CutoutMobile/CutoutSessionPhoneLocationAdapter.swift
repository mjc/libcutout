import CoreLocation
import CutoutMobileFFI
import Foundation

struct PhoneLocationUpdate: Sendable {
    let receiptMonotonic: MonotonicMilliseconds
    let receiptWallClock: Date
    let samples: [MobilePhoneLocationSampleDto]
}

@MainActor
protocol CutoutSessionPhoneLocationAdapting: AnyObject {
    var latestSample: MobilePhoneLocationSampleDto? { get }
    var authorizationStatus: CLAuthorizationStatus { get }
    func start()
    func clear()
    func updateDemand(_ demand: MobileRideMapLocationDemandDto)
    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager)
    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation])
}

/// Owns Core Location lifetime and phone-sample admission. Ride-map persistence remains in Core.
@MainActor
final class CutoutSessionPhoneLocationAdapter: NSObject, CutoutSessionPhoneLocationAdapting {
    private let clock: MonotonicClock
    private let wallClock: () -> Date
    private let onSnapshot: @MainActor (MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void
    private let onLocationUpdate: @MainActor (PhoneLocationUpdate) -> Void
    private let onAvailabilityChange: @MainActor () -> Void
    private var locationDemand = MobileRideMapLocationDemandDto.idle
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
        onSnapshot: @escaping @MainActor (MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void,
        onLocationUpdate: @escaping @MainActor (PhoneLocationUpdate) -> Void,
        onAvailabilityChange: @escaping @MainActor () -> Void
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

    func updateDemand(_ demand: MobileRideMapLocationDemandDto) {
        locationDemand = demand
        #if os(iOS)
            locationManager.allowsBackgroundLocationUpdates = demand == .record
        #endif

        guard demand != .idle else {
            stopUpdatesIfNeeded()
            return
        }

        guard CLLocationManager.locationServicesEnabled() else {
            onAvailabilityChange()
            return
        }

        switch (demand, locationManager.authorizationStatus) {
        case (.requestPermission, .notDetermined):
            guard !didRequestWhenInUseAuthorization else { return }
            didRequestWhenInUseAuthorization = true
            locationManager.requestWhenInUseAuthorization()
        case (.requestPermission, _), (.record, .notDetermined):
            onAvailabilityChange()
        case (.record, .authorizedAlways), (.record, .authorizedWhenInUse):
            locationManager.startUpdatingLocation()
            locationManagerUpdatesStarted = true
        case (.record, .denied), (.record, .restricted):
            stopUpdatesIfNeeded()
            onAvailabilityChange()
        case (.record, _):
            stopUpdatesIfNeeded()
            onAvailabilityChange()
        case (.idle, _):
            stopUpdatesIfNeeded()
        case (.requestPermission, .authorizedAlways), (.requestPermission, .authorizedWhenInUse),
            (.requestPermission, .denied), (.requestPermission, .restricted):
            onAvailabilityChange()
        }
    }

    nonisolated func locationManagerDidChangeAuthorization(_: CLLocationManager) {
        MainActor.assumeIsolated {
            onAvailabilityChange()
            updateDemand(locationDemand)
        }
    }

    nonisolated func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        MainActor.assumeIsolated {
            receiveLocations(locations)
        }
    }

    private func receiveLocations(_ locations: [CLLocation]) {
        guard !locations.isEmpty else { return }

        let receiptMonotonic = clock.now()
        let samples = locations.map(MobilePhoneLocationSampleDto.init(location:))

        for sample in samples {
            _ = state.ingest(sample: sample)
        }
        onSnapshot(state.currentSnapshot(), receiptMonotonic)
        onLocationUpdate(
            PhoneLocationUpdate(
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

extension CutoutSessionPhoneLocationAdapter: CLLocationManagerDelegate {}

extension MobilePhoneLocationSampleDto {
    fileprivate init(location: CLLocation) {
        let coordinate = location.coordinate

        self.init(
            wallClockUnixMs: 0,
            sourceTimestampUnixSeconds: location.timestamp.timeIntervalSince1970,
            latitudeDegrees: coordinate.latitude,
            longitudeDegrees: coordinate.longitude,
            altitudeMeters: location.altitude,
            horizontalAccuracyMeters: location.horizontalAccuracy,
            verticalAccuracyMeters: location.verticalAccuracy,
            speedMetersPerSecond: location.speed,
            speedAccuracyMetersPerSecond: location.speedAccuracy,
            courseDegrees: location.course,
            courseAccuracyDegrees: location.courseAccuracy
        )
    }
}
