import CoreLocation
import CutoutMobileFFI
import Foundation

protocol RideLocationManaging: AnyObject {
    var delegate: (any CLLocationManagerDelegate)? { get set }
    var authorizationStatus: CLAuthorizationStatus { get }
    var desiredAccuracy: CLLocationAccuracy { get set }
    var activityType: CLActivityType { get set }
#if os(iOS)
    var allowsBackgroundLocationUpdates: Bool { get set }
#endif
    func startUpdatingLocation()
    func stopUpdatingLocation()
    func requestWhenInUseAuthorization()
    func requestAlwaysAuthorization()
}

extension CLLocationManager: RideLocationManaging {}

/// Owns native location effects and forwards observations to the Rust recording owner.
@MainActor
final class RideLocationAdapter: NSObject, @preconcurrency CLLocationManagerDelegate {
    private let manager: any RideLocationManaging
    private let servicesEnabled: @Sendable () -> Bool
    private let onEnvironment: (MobileLocationEnvironmentDto) -> Void
    private let onLocations: ([CLLocation]) -> Void
    private var environmentTask: Task<Void, Never>?
    private var environmentGeneration: UInt64 = 0
    private var revision: UInt64 = 0
    private var isUpdating = false
    private var requestedPermission = false
    private var requestedAlwaysPermission = false
    private var temporarilyUnavailable = false

    init(
        manager: any RideLocationManaging = CLLocationManager(),
        servicesEnabled: @escaping @Sendable () -> Bool = { CLLocationManager.locationServicesEnabled() },
        onEnvironment: @escaping (MobileLocationEnvironmentDto) -> Void,
        onLocations: @escaping ([CLLocation]) -> Void
    ) {
        self.manager = manager
        self.servicesEnabled = servicesEnabled
        self.onEnvironment = onEnvironment
        self.onLocations = onLocations
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBestForNavigation
        manager.activityType = .fitness
#if os(iOS)
        manager.allowsBackgroundLocationUpdates = true
#endif
    }

    isolated deinit {
        environmentTask?.cancel()
        manager.delegate = nil
        manager.stopUpdatingLocation()
    }

    func refreshEnvironment() {
        environmentGeneration &+= 1
        let generation = environmentGeneration
        let authorization = Self.authorization(manager.authorizationStatus)
        let unavailable = temporarilyUnavailable
        environmentTask?.cancel()
        environmentTask = Task { [weak self, servicesEnabled] in
            let enabled = await Task.detached(operation: servicesEnabled).value
            guard !Task.isCancelled, let self, generation == environmentGeneration else { return }
            environmentTask = nil
            onEnvironment(MobileLocationEnvironmentDto(
                authorization: authorization, servicesEnabled: enabled,
                temporarilyUnavailable: unavailable
            ))
        }
    }

    @discardableResult
    func apply(_ intent: MobileLocationAcquisitionDto) -> Bool {
        guard intent.revision >= revision else { return false }
        revision = intent.revision
        switch intent.demand {
        case .idle:
            if isUpdating { manager.stopUpdatingLocation(); isUpdating = false }
        case .requestPermission:
            if isUpdating { manager.stopUpdatingLocation(); isUpdating = false }
            if !requestedPermission {
                requestedPermission = true
                manager.requestWhenInUseAuthorization()
            }
        case .record:
#if os(iOS)
            if manager.authorizationStatus == .authorizedWhenInUse, !requestedAlwaysPermission {
                requestedAlwaysPermission = true
                manager.requestAlwaysAuthorization()
            }
#endif
            if !isUpdating { manager.startUpdatingLocation(); isUpdating = true }
        }
        return true
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        refreshEnvironment()
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        if temporarilyUnavailable {
            temporarilyUnavailable = false
            refreshEnvironment()
        }
        onLocations(locations)
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: any Error) {
        temporarilyUnavailable = (error as? CLError)?.code != .denied
        refreshEnvironment()
    }

    private static func authorization(_ status: CLAuthorizationStatus) -> MobileLocationAuthorizationDto {
        switch status {
        case .notDetermined: .notDetermined
        case .denied: .denied
        case .restricted: .restricted
        case .authorizedWhenInUse: .whenInUse
        case .authorizedAlways: .always
        @unknown default: .restricted
        }
    }
}
