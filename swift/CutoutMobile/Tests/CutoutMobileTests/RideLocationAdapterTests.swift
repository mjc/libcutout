import CoreLocation
import CutoutMobileFFI
import XCTest
@testable import CutoutMobile

final class RideLocationAdapterTests: XCTestCase {
    @MainActor
    func testNativeLocationEffectsFollowTheRustRecordingIntent() throws {
        let manager = LocationManagerSpy()
        let adapter = RideLocationAdapter(manager: manager, onEnvironment: { _ in }, onLocations: { _ in })
        let state = MobileRideMapState()
        adapter.apply(state.observeLocationEnvironment(MobileLocationEnvironmentDto(
            authorization: .always, servicesEnabled: true, temporarilyUnavailable: false
        )))
        XCTAssertEqual(manager.starts, 0)
        XCTAssertEqual(manager.permissionRequests, 0)

        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        adapter.apply(state.locationAcquisition())
        adapter.apply(state.locationAcquisition())
        XCTAssertEqual(manager.starts, 1)
        _ = try state.pause(atMs: 2_000)
        adapter.apply(state.locationAcquisition())
        XCTAssertEqual(manager.stops, 1)
        _ = try state.resume(atMs: 3_000)
        adapter.apply(state.locationAcquisition())
        XCTAssertEqual(manager.starts, 2)
        _ = try state.stop(atMs: 4_000)
        adapter.apply(state.locationAcquisition())
        XCTAssertEqual(manager.stops, 2)
        _ = try state.save()
        adapter.apply(state.locationAcquisition())
        XCTAssertEqual(manager.starts, 2)
        XCTAssertEqual(manager.stops, 2)
    }

    @MainActor
    func testNativeLocationPromptWaitsForAnActiveRecording() throws {
        let manager = LocationManagerSpy()
        manager.authorizationStatus = .notDetermined
        let adapter = RideLocationAdapter(manager: manager, onEnvironment: { _ in }, onLocations: { _ in })
        let state = MobileRideMapState()
        adapter.apply(state.observeLocationEnvironment(MobileLocationEnvironmentDto(
            authorization: .notDetermined, servicesEnabled: true, temporarilyUnavailable: false
        )))
        XCTAssertEqual(manager.permissionRequests, 0)
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        adapter.apply(state.locationAcquisition())
        adapter.apply(state.locationAcquisition())
        XCTAssertEqual(manager.permissionRequests, 1)
        XCTAssertEqual(manager.starts, 0)
    }

    @MainActor
    func testNativeLocationEnvironmentForwardsDeniedAndDisabledServices() async {
        let manager = LocationManagerSpy()
        manager.authorizationStatus = .denied
        let observed = expectation(description: "native location environment")
        let adapter = RideLocationAdapter(
            manager: manager, servicesEnabled: { false },
            onEnvironment: { environment in
                XCTAssertEqual(environment.authorization, .denied)
                XCTAssertFalse(environment.servicesEnabled)
                observed.fulfill()
            }, onLocations: { _ in }
        )
        adapter.refreshEnvironment()
        await fulfillment(of: [observed], timeout: 2)
    }
}

private final class LocationManagerSpy: RideLocationManaging {
    weak var delegate: (any CLLocationManagerDelegate)?
    var authorizationStatus = CLAuthorizationStatus.authorizedAlways
    var desiredAccuracy: CLLocationAccuracy = 0
    var activityType = CLActivityType.other
#if os(iOS)
    var allowsBackgroundLocationUpdates = false
#endif
    var starts = 0
    var stops = 0
    var permissionRequests = 0
    func startUpdatingLocation() { starts += 1 }
    func stopUpdatingLocation() { stops += 1 }
    func requestWhenInUseAuthorization() { permissionRequests += 1 }
    func requestAlwaysAuthorization() {}
}
