import CoreFoundation
import CoreLocation
import CutoutMobileFFI
import Foundation
import Synchronization

struct PhoneLocationUpdate: Sendable {
    let receiptMonotonic: MonotonicMilliseconds
    let receiptWallClock: Date
    let samples: [MobilePhoneLocationSampleDto]
}

@MainActor
protocol CutoutSessionPhoneLocationAdapting: AnyObject {
    var latestSample: MobilePhoneLocationSampleDto? { get }
    var authorizationStatus: CLAuthorizationStatus { get }
    var servicesEnabled: Bool? { get }
    func start()
    func clear()
    func updateDemand(_ demand: MobileRideMapLocationDemandDto)
    #if DEBUG
        func deliverLocationsForTesting(_ locations: [CLLocation])
        func refreshAuthorizationForTesting()
    #endif
}

#if DEBUG
    extension CutoutSessionPhoneLocationAdapting {
        func deliverLocationsForTesting(_: [CLLocation]) {}
        func refreshAuthorizationForTesting() {}
    }
#endif

/// Presentation proxy. The manager and its blocking recording sink belong to one native run loop.
@MainActor
final class CutoutSessionPhoneLocationAdapter: CutoutSessionPhoneLocationAdapting {
    private var owner: PhoneLocationOwner!
    private var snapshot = MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    private(set) var authorizationStatus = CLAuthorizationStatus.notDetermined
    private(set) var servicesEnabled: Bool?
    var latestSample: MobilePhoneLocationSampleDto? { snapshot.latestSample }

    init(
        clock: MonotonicClock,
        wallClock: @escaping @Sendable () -> Date,
        onSnapshot: @escaping @MainActor (MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void,
        onLocationUpdate: @escaping @Sendable (PhoneLocationUpdate) -> Void,
        onAvailabilityChange: @escaping @MainActor () -> Void,
        managerFactory: @escaping @Sendable () -> CLLocationManager = { CLLocationManager() },
        servicesEnabledQuery: @escaping @Sendable () -> Bool = { CLLocationManager.locationServicesEnabled() }
    ) {
        let publication = CutoutMainQueueLatest<PhoneLocationPublication> { [weak self] value in
            guard let self else { return }
            self.authorizationStatus = value.authorization
            self.servicesEnabled = value.servicesEnabled
            self.snapshot = value.snapshot
            if let receivedAt = value.receivedAt { onSnapshot(value.snapshot, receivedAt) }
            onAvailabilityChange()
        }
        owner = PhoneLocationOwner(
            clock: clock, wallClock: wallClock, managerFactory: managerFactory,
            servicesEnabledQuery: servicesEnabledQuery, onLocationUpdate: onLocationUpdate,
            publication: publication
        )
    }

    deinit { owner.shutdown() }
    func start() { owner.command(refresh: true) }
    func clear() { owner.command(clear: true) }
    func updateDemand(_ demand: MobileRideMapLocationDemandDto) { owner.command(demand: demand) }

    #if DEBUG
        func deliverLocationsForTesting(_ locations: [CLLocation]) {
            owner.performForTesting { owner in owner.receiveLocations(locations) }
        }
        func refreshAuthorizationForTesting() { owner.command(refresh: true) }
        func inspectManagerForTesting(_ inspect: @escaping @Sendable (CLLocationManager) -> Void) {
            owner.performForTesting { inspect($0.manager) }
        }
    #endif
}

private struct PhoneLocationPublication: Sendable {
    let authorization: CLAuthorizationStatus
    let servicesEnabled: Bool?
    let snapshot: MobilePhoneLocationSnapshotDto
    let receivedAt: MonotonicMilliseconds?
}

/// Core Location requires creation and delegate delivery on an active run loop. Mutable manager,
/// sample state, and recording effects are confined to that thread. Only the control mailbox is
/// shared, under its mutex; presentation crosses through one replaceable Main payload.
private final class PhoneLocationOwner: NSObject, CLLocationManagerDelegate, @unchecked Sendable {
    // Core Foundation permits PerformBlock/WakeUp from other threads. The loop itself is
    // executed and stopped only by its owner; this reference crosses solely for those APIs.
    private final class RunLoopReference: @unchecked Sendable {
        let value: CFRunLoop
        init(_ value: CFRunLoop) { self.value = value }
    }

    private struct Control: Sendable {
        var runLoop: RunLoopReference?
        var wakeScheduled = false
        var demand: MobileRideMapLocationDemandDto?
        var refresh = false
        var clear = false
        var stopped = false
        #if DEBUG
            var testingOperation: (@Sendable (PhoneLocationOwner) -> Void)?
        #endif
    }
    private let control = Mutex(Control())
    private let clock: MonotonicClock
    private let wallClock: @Sendable () -> Date
    private let managerFactory: @Sendable () -> CLLocationManager
    private let servicesEnabledQuery: @Sendable () -> Bool
    private let onLocationUpdate: @Sendable (PhoneLocationUpdate) -> Void
    private let publication: CutoutMainQueueLatest<PhoneLocationPublication>
    fileprivate var manager: CLLocationManager!
    private let state = MobilePhoneLocationState()
    private var servicesEnabled: Bool?
    private var demand = MobileRideMapLocationDemandDto.idle
    private var updatesStarted = false
    private var permissionRequested = false
    private var receivedAt: MonotonicMilliseconds?

    init(
        clock: MonotonicClock, wallClock: @escaping @Sendable () -> Date,
        managerFactory: @escaping @Sendable () -> CLLocationManager,
        servicesEnabledQuery: @escaping @Sendable () -> Bool,
        onLocationUpdate: @escaping @Sendable (PhoneLocationUpdate) -> Void,
        publication: CutoutMainQueueLatest<PhoneLocationPublication>
    ) {
        self.clock = clock
        self.wallClock = wallClock
        self.managerFactory = managerFactory
        self.servicesEnabledQuery = servicesEnabledQuery
        self.onLocationUpdate = onLocationUpdate
        self.publication = publication
        super.init()
        let thread = Thread { [self] in run() }
        thread.name = "io.cutout.location"
        thread.start()
    }

    func command(demand: MobileRideMapLocationDemandDto? = nil, refresh: Bool = false, clear: Bool = false) {
        control.withLock { pending in
            guard !pending.stopped else { return }
            if let demand { pending.demand = demand }
            pending.refresh = pending.refresh || refresh
            pending.clear = pending.clear || clear
            schedule(&pending)
        }
    }

    func shutdown() {
        control.withLock { pending in
            pending.stopped = true
            schedule(&pending)
        }
    }

    private func schedule(_ pending: inout Control) {
        guard let runLoop = pending.runLoop?.value, !pending.wakeScheduled else { return }
        pending.wakeScheduled = true
        CFRunLoopPerformBlock(runLoop, RunLoop.Mode.default.rawValue as NSString) { [weak self] in self?.applyCommands()
        }
        CFRunLoopWakeUp(runLoop)
    }

    private func run() {
        let runLoop = CFRunLoopGetCurrent()!
        // A source keeps the run loop alive even before the manager starts acquiring locations.
        let keepAlive = Port()
        RunLoop.current.add(keepAlive, forMode: .default)
        manager = managerFactory()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBestForNavigation
        manager.activityType = .fitness
        manager.pausesLocationUpdatesAutomatically = false
        control.withLock { pending in
            pending.runLoop = RunLoopReference(runLoop)
            schedule(&pending)
        }
        // Secondary Cocoa threads need an autorelease pool per delivered run-loop event.
        // The source keeps this wait event-driven; it does not poll for recording demand.
        while !control.withLock({ $0.stopped }) {
            _ = autoreleasepool {
                CFRunLoopRunInMode(.defaultMode, .greatestFiniteMagnitude, true)
            }
        }
        stopUpdates()
        manager.delegate = nil
        manager = nil
        keepAlive.invalidate()
        control.withLock { $0.runLoop = nil }
    }

    private func applyCommands() {
        let pending = control.withLock { pending in
            let result = pending
            pending.wakeScheduled = false
            pending.demand = nil
            pending.refresh = false
            pending.clear = false
            #if DEBUG
                pending.testingOperation = nil
            #endif
            return result
        }
        if pending.stopped {
            CFRunLoopStop(CFRunLoopGetCurrent())
            return
        }
        if pending.clear {
            state.clear()
            receivedAt = nil
            publish()
        }
        if let demand = pending.demand { self.demand = demand }
        if pending.refresh { refreshServices() }
        applyDemand()
        #if DEBUG
            pending.testingOperation?(self)
        #endif
    }

    func locationManagerDidChangeAuthorization(_: CLLocationManager) { refreshServices() }

    private func refreshServices() {
        switch manager.authorizationStatus {
        case .authorizedAlways, .authorizedWhenInUse:
            if servicesEnabled != true {
                servicesEnabled = nil
                stopUpdates()
            }
        default:
            servicesEnabled = nil
            stopUpdates()
        }
        publish()
        servicesEnabled = servicesEnabledQuery()
        applyDemand()
        publish()
    }

    private func applyDemand() {
        #if os(iOS)
            manager.allowsBackgroundLocationUpdates = demand == .record
        #endif
        guard servicesEnabled == true, demand != .idle else {
            stopUpdates()
            return
        }
        switch (demand, manager.authorizationStatus) {
        case (.requestPermission, .notDetermined):
            stopUpdates()
            if !permissionRequested {
                permissionRequested = true
                manager.requestWhenInUseAuthorization()
            }
        case (.record, .authorizedAlways), (.record, .authorizedWhenInUse):
            if !updatesStarted {
                updatesStarted = true
                manager.startUpdatingLocation()
            }
        default: stopUpdates()
        }
    }

    private func stopUpdates() {
        if updatesStarted {
            updatesStarted = false
            manager.stopUpdatingLocation()
        }
    }

    func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        receiveLocations(locations)
    }

    fileprivate func receiveLocations(_ locations: [CLLocation]) {
        guard !locations.isEmpty else { return }
        let monotonic = clock.now()
        let wall = wallClock()
        let samples = locations.map(MobilePhoneLocationSampleDto.init(location:))
        for sample in samples { _ = state.ingest(sample: sample) }
        receivedAt = monotonic
        publish()
        onLocationUpdate(PhoneLocationUpdate(receiptMonotonic: monotonic, receiptWallClock: wall, samples: samples))
    }

    private func publish() {
        publication.submit(
            PhoneLocationPublication(
                authorization: manager.authorizationStatus, servicesEnabled: servicesEnabled,
                snapshot: state.currentSnapshot(), receivedAt: receivedAt
            ))
    }

    #if DEBUG
        // Test operations execute on the same manager run loop; production callbacks never enqueue
        // one closure per batch and instead apply backpressure directly to their native producer.
        func performForTesting(_ operation: @escaping @Sendable (PhoneLocationOwner) -> Void) {
            control.withLock { pending in
                precondition(pending.testingOperation == nil, "Only one test operation may be pending")
                pending.testingOperation = operation
                schedule(&pending)
            }
        }
    #endif
}

extension MobilePhoneLocationSampleDto {
    fileprivate init(location: CLLocation) {
        let coordinate = location.coordinate
        self.init(
            wallClockUnixMs: 0, sourceTimestampUnixSeconds: location.timestamp.timeIntervalSince1970,
            latitudeDegrees: coordinate.latitude, longitudeDegrees: coordinate.longitude,
            altitudeMeters: location.altitude, horizontalAccuracyMeters: location.horizontalAccuracy,
            verticalAccuracyMeters: location.verticalAccuracy, speedMetersPerSecond: location.speed,
            speedAccuracyMetersPerSecond: location.speedAccuracy, courseDegrees: location.course,
            courseAccuracyDegrees: location.courseAccuracy
        )
    }
}
