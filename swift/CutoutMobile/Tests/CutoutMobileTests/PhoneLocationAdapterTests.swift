import CoreLocation
import Foundation
import Synchronization
import XCTest

@testable import CutoutMobile

@MainActor
final class PhoneLocationAdapterTests: XCTestCase {
    func testLocationRecordingCallbackRunsOffMainAndRetainsReceiptClocks() async {
        let recorded = expectation(description: "Location reached recording owner")
        let receipt = Date(timeIntervalSince1970: 1_234)
        let harness = LocationManagerHarness()
        let adapter = CutoutSessionPhoneLocationAdapter(
            clock: MonotonicClock(now: { MonotonicMilliseconds(123) }), wallClock: { receipt },
            onSnapshot: { _, _ in },
            onLocationUpdate: { update in
                XCTAssertFalse(Thread.isMainThread)
                XCTAssertEqual(update.receiptMonotonic, MonotonicMilliseconds(123))
                XCTAssertEqual(update.receiptWallClock, receipt)
                XCTAssertEqual(update.samples.first?.sourceTimestampUnixSeconds, 1_000)
                recorded.fulfill()
            }, onAvailabilityChange: {}, managerFactory: harness.makeManager, servicesEnabledQuery: { true }
        )
        adapter.inspectManagerForTesting { manager in
            manager.delegate?.locationManager?(
                manager,
                didUpdateLocations: [
                    CLLocation(
                        coordinate: CLLocationCoordinate2D(latitude: 40, longitude: -105), altitude: 0,
                        horizontalAccuracy: 1, verticalAccuracy: 1, timestamp: Date(timeIntervalSince1970: 1_000)
                    )
                ])
        }
        await fulfillment(of: [recorded], timeout: 2)
        let inspected = expectation(description: "Owner inspected manager")
        adapter.inspectManagerForTesting { manager in
            XCTAssertFalse(Thread.isMainThread)
            XCTAssertFalse(manager.pausesLocationUpdatesAutomatically)
            XCTAssertEqual(manager.desiredAccuracy, kCLLocationAccuracyBestForNavigation)
            XCTAssertEqual(manager.activityType, .fitness)
            inspected.fulfill()
        }
        await fulfillment(of: [inspected], timeout: 2)
    }

    func testLocationRecordingBackpressureDoesNotBlockMainActorDelivery() async {
        let entered = expectation(description: "Recording owns callback")
        let recorded = expectation(description: "Blocked recording returned")
        let release = DispatchSemaphore(value: 0)
        defer { release.signal() }
        let harness = LocationManagerHarness()
        let adapter = CutoutSessionPhoneLocationAdapter(
            clock: MonotonicClock(), wallClock: Date.init,
            onSnapshot: { _, _ in },
            onLocationUpdate: { _ in
                entered.fulfill()
                release.wait()
                recorded.fulfill()
            }, onAvailabilityChange: {}, managerFactory: harness.makeManager, servicesEnabledQuery: { true }
        )
        adapter.deliverLocationsForTesting([CLLocation(latitude: 40, longitude: -105)])
        await fulfillment(of: [entered], timeout: 2)
        // The owner is still blocked. Main can change its latest desired state without waiting.
        adapter.updateDemand(.record)
        adapter.updateDemand(.idle)
        let heartbeat = expectation(description: "Main progress before storage release")
        DispatchQueue.main.async { heartbeat.fulfill() }
        await fulfillment(of: [heartbeat], timeout: 1)
        release.signal()
        await fulfillment(of: [recorded], timeout: 2)
    }

    func testServicesCheckRunsOffMainAndPendingDemandDoesNotStartLocationUpdates() async {
        let entered = expectation(description: "Services check entered")
        let resolved = expectation(description: "Services resolved")
        let release = DispatchSemaphore(value: 0)
        defer { release.signal() }
        let harness = LocationManagerHarness()
        var adapter: CutoutSessionPhoneLocationAdapter!
        adapter = makeAdapter(
            harness: harness,
            onAvailabilityChange: {
                if adapter?.servicesEnabled == true { resolved.fulfill() }
            },
            query: {
                XCTAssertFalse(Thread.isMainThread)
                entered.fulfill()
                release.wait()
                return true
            })
        adapter.start()
        adapter.updateDemand(.record)
        await fulfillment(of: [entered], timeout: 2)
        XCTAssertNil(adapter.servicesEnabled)
        XCTAssertEqual(harness.starts, 0)
        release.signal()
        await fulfillment(of: [resolved], timeout: 2)
        await waitUntil { harness.starts == 1 }
        adapter.updateDemand(.record)
        await inspect(adapter) { _ in XCTAssertEqual(harness.starts, 1) }
    }

    func testDisabledServicesAndDeniedAuthorizationStopUpdatesAndCanRecover() async {
        let result = Mutex(true)
        let harness = LocationManagerHarness()
        let adapter = makeAdapter(harness: harness, query: { result.withLock { $0 } })
        adapter.start()
        adapter.updateDemand(.record)
        await waitUntil { harness.starts == 1 }
        harness.authorization = .denied
        result.withLock { $0 = false }
        adapter.refreshAuthorizationForTesting()
        await waitUntil { adapter.servicesEnabled == false }
        XCTAssertEqual(harness.stops, 1)
        XCTAssertEqual(adapter.authorizationStatus, .denied)
        result.withLock { $0 = true }
        adapter.refreshAuthorizationForTesting()
        await waitUntil { adapter.servicesEnabled == true }
        XCTAssertEqual(harness.starts, 1, "Global services cannot override denied app permission")
        harness.authorization = .authorizedAlways
        adapter.refreshAuthorizationForTesting()
        await waitUntil { harness.starts == 2 }
        adapter.updateDemand(.idle)
        await waitUntil { harness.stops == 2 }
    }

    func testAuthorizationLossStopsUpdatesBeforeSlowServicesQueryCompletes() async {
        let harness = LocationManagerHarness()
        let blocked = Mutex(false)
        let release = DispatchSemaphore(value: 0)
        defer { release.signal() }
        let entered = expectation(description: "Denied authorization services query entered")
        let adapter = makeAdapter(
            harness: harness,
            query: {
                if blocked.withLock({ $0 }) {
                    entered.fulfill()
                    release.wait()
                }
                return true
            })
        adapter.start()
        adapter.updateDemand(.record)
        await waitUntil { harness.starts == 1 }
        harness.authorization = .denied
        blocked.withLock { $0 = true }
        adapter.refreshAuthorizationForTesting()
        await fulfillment(of: [entered], timeout: 2)
        XCTAssertEqual(harness.stops, 1)
        await waitUntil { adapter.servicesEnabled == nil && adapter.authorizationStatus == .denied }
        release.signal()
    }

    func testServicesRefreshDoesNotInterruptAlreadyAuthorizedRecording() async {
        let harness = LocationManagerHarness()
        let adapter = makeAdapter(harness: harness)
        adapter.start()
        adapter.updateDemand(.record)
        await waitUntil { harness.starts == 1 }
        adapter.refreshAuthorizationForTesting()
        await inspect(adapter) { _ in
            XCTAssertEqual(harness.starts, 1)
            XCTAssertEqual(harness.stops, 0)
        }
    }

    func testTeardownReturnsOnMainAndStopsOnTheManagerOwner() async {
        let harness = LocationManagerHarness()
        var adapter: CutoutSessionPhoneLocationAdapter? = makeAdapter(harness: harness)
        adapter?.start()
        adapter?.updateDemand(.record)
        await waitUntil { harness.starts == 1 }
        adapter = nil
        await waitUntil { harness.stops == 1 }
    }

    func testTeardownDuringBlockedRecordingLeavesMainResponsiveAndStopsAfterSettlement() async {
        let entered = expectation(description: "Recording callback entered")
        let settled = expectation(description: "Recording callback settled")
        let release = DispatchSemaphore(value: 0)
        defer { release.signal() }
        let harness = LocationManagerHarness()
        var adapter: CutoutSessionPhoneLocationAdapter? = CutoutSessionPhoneLocationAdapter(
            clock: MonotonicClock(), wallClock: Date.init,
            onSnapshot: { _, _ in },
            onLocationUpdate: { _ in
                entered.fulfill()
                release.wait()
                settled.fulfill()
            }, onAvailabilityChange: {}, managerFactory: harness.makeManager, servicesEnabledQuery: { true }
        )
        adapter?.start()
        adapter?.updateDemand(.record)
        await waitUntil { harness.starts == 1 }
        adapter?.deliverLocationsForTesting([CLLocation(latitude: 40, longitude: -105)])
        await fulfillment(of: [entered], timeout: 2)
        adapter = nil
        XCTAssertEqual(harness.stops, 0, "The owner must finish its accepted callback before stopping")
        let heartbeat = expectation(description: "Main responds after teardown before recording releases")
        DispatchQueue.main.async { heartbeat.fulfill() }
        await fulfillment(of: [heartbeat], timeout: 1)
        release.signal()
        await fulfillment(of: [settled], timeout: 2)
        await waitUntil { harness.stops == 1 }
    }

    private func makeAdapter(
        harness: LocationManagerHarness,
        onAvailabilityChange: @escaping @MainActor () -> Void = {},
        query: @escaping @Sendable () -> Bool = { true }
    ) -> CutoutSessionPhoneLocationAdapter {
        CutoutSessionPhoneLocationAdapter(
            clock: MonotonicClock(), wallClock: Date.init,
            onSnapshot: { _, _ in }, onLocationUpdate: { _ in },
            onAvailabilityChange: onAvailabilityChange,
            managerFactory: harness.makeManager, servicesEnabledQuery: query
        )
    }

    private func inspect(
        _ adapter: CutoutSessionPhoneLocationAdapter,
        _ body: @escaping @Sendable (CLLocationManager) -> Void
    ) async {
        let inspected = expectation(description: "Manager command completed on owner")
        adapter.inspectManagerForTesting { manager in
            body(manager)
            inspected.fulfill()
        }
        await fulfillment(of: [inspected], timeout: 2)
    }

    private func waitUntil(_ condition: @MainActor () -> Bool) async {
        let deadline = ContinuousClock.now.advanced(by: .seconds(2))
        while !condition(), ContinuousClock.now < deadline { try? await Task.sleep(for: .milliseconds(5)) }
        XCTAssertTrue(condition())
    }
}

private final class LocationManagerHarness: Sendable {
    struct State: Sendable {
        var authorization = CLAuthorizationStatus.authorizedAlways
        var starts = 0
        var stops = 0
    }
    let state = Mutex(State())
    var starts: Int { state.withLock { $0.starts } }
    var stops: Int { state.withLock { $0.stops } }
    var authorization: CLAuthorizationStatus {
        get { state.withLock { $0.authorization } }
        set { state.withLock { $0.authorization = newValue } }
    }
    func makeManager() -> CLLocationManager {
        XCTAssertFalse(Thread.isMainThread, "Create the manager on its active owner run loop")
        return TrackingLocationManager(harness: self)
    }
}

private final class TrackingLocationManager: CLLocationManager, @unchecked Sendable {
    private let harness: LocationManagerHarness
    private let owner = Thread.current
    private weak var ownerDelegate: (any CLLocationManagerDelegate)?
    init(harness: LocationManagerHarness) {
        self.harness = harness
        super.init()
    }
    // No operating-system authorization callbacks in deterministic tests.
    override var delegate: (any CLLocationManagerDelegate)? {
        get { ownerDelegate }
        set { ownerDelegate = newValue }
    }
    override var authorizationStatus: CLAuthorizationStatus { harness.authorization }
    override func startUpdatingLocation() {
        XCTAssertTrue(Thread.current === owner)
        harness.state.withLock { $0.starts += 1 }
    }
    override func stopUpdatingLocation() {
        XCTAssertTrue(Thread.current === owner)
        harness.state.withLock { $0.stops += 1 }
    }
}
