import CoreBluetooth
import CoreLocation
import CutoutMobileFFI
import Foundation

func protocolIdentityFallbackDisplayName(
    protocolFamily: DeviceDetectionProtocolFamily?
) -> String {
    switch protocolFamily {
    case .veteranLeaperkimNosfet:
        pevLocalizedText("protocol_identity.fallback.veteran_nosfet")
    case .begodeGotway:
        pevLocalizedText("protocol_identity.fallback.begode")
    case .vesc:
        pevLocalizedText("protocol_identity.fallback.vesc")
    case nil:
        pevLocalizedText("protocol_identity.fallback.unknown")
    }
}

public enum CaptureWriterHealth: Equatable, Sendable {
    case healthy
    case failed

    public func metricValue(display: String) -> PevDashboardMetricValue {
        .status(display: display, accessibility: display)
    }
}

public struct CaptureProgress: Equatable, Sendable {
    public let elapsedMilliseconds: UInt64
    public let notificationCount: UInt64
    public let fileSizeBytes: UInt64
    public let queuedMessageCount: UInt64
    public let writerError: String?

    public init(
        elapsedMilliseconds: UInt64,
        notificationCount: UInt64,
        fileSizeBytes: UInt64,
        queuedMessageCount: UInt64,
        writerError: String?
    ) {
        self.elapsedMilliseconds = elapsedMilliseconds
        self.notificationCount = notificationCount
        self.fileSizeBytes = fileSizeBytes
        self.queuedMessageCount = queuedMessageCount
        self.writerError = writerError
    }

    public var writerHealth: CaptureWriterHealth {
        writerError == nil ? .healthy : .failed
    }

    public var elapsedMetricValue: PevDashboardMetricValue {
        let value = Duration.seconds(Double(elapsedMilliseconds) / 1_000)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
        return .available(display: value, accessibility: value)
    }

    public var notificationCountMetricValue: PevDashboardMetricValue {
        let value = notificationCount.formatted()
        return .available(display: value, accessibility: value)
    }

    public var fileSizeMetricValue: PevDashboardMetricValue {
        let value = ByteCountFormatter.string(
            fromByteCount: Int64(clamping: fileSizeBytes),
            countStyle: .file
        )
        return .available(display: value, accessibility: value)
    }

    public var queuedMessageCountMetricValue: PevDashboardMetricValue {
        let value = queuedMessageCount.formatted()
        return .available(display: value, accessibility: value)
    }
}

public enum CaptureEvent: Equatable, Sendable {
    case started(fileURL: URL)
    case notificationRecorded
    case progress(CaptureProgress)
    case finished(fileURL: URL)
    case failed
}

struct ConnectionReconnectPolicy {
    static let maximumAttempts = 3

    static func delayMilliseconds(attempt: Int, jitter: Double) -> UInt64? {
        guard (1...maximumAttempts).contains(attempt) else { return nil }
        let base = 250.0 * pow(2, Double(attempt - 1))
        let boundedJitter = min(max(jitter, 0), 1)
        return UInt64((base * (0.8 + (0.4 * boundedJitter))).rounded())
    }
}

struct BegodeProbeResponsePolicy {
    static let timeoutAfter = MonotonicMilliseconds(2_000)
}

struct IdentificationProbeTransportCoordinator {
    let detectionSession: DeviceDetectionSession

    func subscribe(using sink: any CoreBluetoothOperationSink) {
        sink.subscribe(channel: .bluetooth16(0xffe1))
    }

    @discardableResult
    func notificationsEnabled(
        at now: MonotonicMilliseconds,
        using sink: any CoreBluetoothOperationSink
    ) -> IdentificationProbeOutcome {
        let outcome = detectionSession.beginIdentificationProbe(at: now)
        if case .writes(let writes) = outcome {
            writes.forEach { sink.writeWithoutResponse(channel: $0.channel, bytes: $0.bytes) }
        }
        return outcome
    }

    func observeNotification(channel: BluetoothUuid, bytes: Data) -> DeviceDetectionResolution {
        guard channel.bluetooth16Value == 0xffe1 else {
            return detectionSession.resolution
        }
        return detectionSession.observeNotification(bytes: bytes)
    }
}

struct ConnectionReconnectSchedule: Equatable {
    let attempt: Int
    let delayMilliseconds: UInt64
}

protocol ConnectionReconnectCancellable: AnyObject {
    func cancel()
}

protocol ConnectionReconnectScheduling: AnyObject {
    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void) -> any ConnectionReconnectCancellable
}

protocol RideMapWritePollingScheduling: AnyObject {
    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void)
}

final class ConnectionReconnectController {
    private let scheduler: any ConnectionReconnectScheduling
    private var pending: (any ConnectionReconnectCancellable)?
    private(set) var attempt = 0

    init(scheduler: any ConnectionReconnectScheduling) {
        self.scheduler = scheduler
    }

    func schedule(jitter: Double, operation: @escaping () -> Void) -> ConnectionReconnectSchedule? {
        attempt += 1
        guard let delayMilliseconds = ConnectionReconnectPolicy.delayMilliseconds(attempt: attempt, jitter: jitter) else {
            pending?.cancel()
            pending = nil
            return nil
        }
        pending?.cancel()
        pending = scheduler.schedule(after: delayMilliseconds, operation: operation)
        return ConnectionReconnectSchedule(attempt: attempt, delayMilliseconds: delayMilliseconds)
    }

    func cancel() {
        pending?.cancel()
        pending = nil
        attempt = 0
    }
}

private final class DispatchReconnectCancellation: ConnectionReconnectCancellable {
    private let workItem: DispatchWorkItem

    init(workItem: DispatchWorkItem) {
        self.workItem = workItem
    }

    func cancel() {
        workItem.cancel()
    }
}

private final class MainQueueReconnectScheduler: ConnectionReconnectScheduling {
    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void) -> any ConnectionReconnectCancellable {
        let workItem = DispatchWorkItem(block: operation)
        DispatchQueue.main.asyncAfter(
            deadline: .now() + .milliseconds(Int(delayMilliseconds)),
            execute: workItem
        )
        return DispatchReconnectCancellation(workItem: workItem)
    }
}

private final class MainQueueRideMapPollScheduler: RideMapWritePollingScheduling {
    private let queue = DispatchQueue(
        label: "io.cutout.ride-map-location-poll",
        qos: .utility
    )

    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void) {
        queue.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: delayMilliseconds)),
            execute: DispatchWorkItem(block: operation)
        )
    }
}

enum CoreBluetoothRestorationPolicy {
    static let restorationIdentifier = "io.cutout.central"

    static var centralManagerOptions: [String: Any] {
        [CBCentralManagerOptionRestoreIdentifierKey: restorationIdentifier]
    }

    static func selectedPlatformIdentifier(
        savedPlatformIdentifier: String?,
        restoredPlatformIdentifiers: [String]
    ) -> String? {
        guard
            let savedPlatformIdentifier,
            restoredPlatformIdentifiers.contains(savedPlatformIdentifier)
        else {
            return nil
        }
        return savedPlatformIdentifier
    }
}

enum RideMapConnectionPolicy {
    static func shouldEnsureRecording(
        hasObservedConnection: Bool,
        hasSelectedRoute: Bool
    ) -> Bool {
        !hasObservedConnection && hasSelectedRoute
    }
}

#if DEBUG
public enum CutoutSessionTestInitialBluetoothState: Sendable {
    case scanning
    case unavailable
    case permissionDenied
}

public struct CutoutSessionTestScript {
    public let candidate: DevicePickerDiscoveryCandidate
    public let telemetry: TelemetrySnapshot?
    public let telemetryUpdate: TelemetrySnapshot?
    public let telemetryUpdateDelayMilliseconds: UInt64
    public let bmsSnapshot: BmsSnapshot?
    public let startsLive: Bool
    public let initialBluetoothState: CutoutSessionTestInitialBluetoothState
    public let failsConnection: Bool
    public let identificationProbeFailure: IdentificationProbeFailure?
    public let emitsLateLiveAfterFailure: Bool
    public let reconnectsAfterFirstLive: Bool
    public let reconnectAfterLiveMilliseconds: UInt64
    public let reconnectDelayMilliseconds: UInt64
    public let bluetoothLossAfterFirstLiveMilliseconds: UInt64?
    public let emitsStaleTelemetry: Bool
    public let flushCaptureSucceeds: Bool
    public let connectionDelayMilliseconds: UInt64

    public init(
        candidate: DevicePickerDiscoveryCandidate,
        telemetry: TelemetrySnapshot?,
        telemetryUpdate: TelemetrySnapshot? = nil,
        telemetryUpdateDelayMilliseconds: UInt64 = 0,
        bmsSnapshot: BmsSnapshot? = nil,
        startsLive: Bool = false,
        initialBluetoothState: CutoutSessionTestInitialBluetoothState = .scanning,
        failsConnection: Bool = false,
        identificationProbeFailure: IdentificationProbeFailure? = nil,
        emitsLateLiveAfterFailure: Bool = false,
        reconnectsAfterFirstLive: Bool = false,
        reconnectAfterLiveMilliseconds: UInt64 = 0,
        reconnectDelayMilliseconds: UInt64 = 1_000,
        bluetoothLossAfterFirstLiveMilliseconds: UInt64? = nil,
        emitsStaleTelemetry: Bool = false,
        flushCaptureSucceeds: Bool = true,
        connectionDelayMilliseconds: UInt64 = 1_000
    ) {
        self.candidate = candidate
        self.telemetry = telemetry
        self.telemetryUpdate = telemetryUpdate
        self.telemetryUpdateDelayMilliseconds = telemetryUpdateDelayMilliseconds
        self.bmsSnapshot = bmsSnapshot
        self.startsLive = startsLive
        self.initialBluetoothState = initialBluetoothState
        self.failsConnection = failsConnection
        self.identificationProbeFailure = identificationProbeFailure
        self.emitsLateLiveAfterFailure = emitsLateLiveAfterFailure
        self.reconnectsAfterFirstLive = reconnectsAfterFirstLive
        self.reconnectAfterLiveMilliseconds = reconnectAfterLiveMilliseconds
        self.reconnectDelayMilliseconds = reconnectDelayMilliseconds
        self.bluetoothLossAfterFirstLiveMilliseconds = bluetoothLossAfterFirstLiveMilliseconds
        self.emitsStaleTelemetry = emitsStaleTelemetry
        self.flushCaptureSucceeds = flushCaptureSucceeds
        self.connectionDelayMilliseconds = connectionDelayMilliseconds
    }
}
#endif

struct BoundedDiagnosticLog {
    private let capacity: Int
    private var storage: [String] = []
    private var nextIndex = 0
    private(set) var droppedCount = 0

    init(capacity: Int) {
        precondition(capacity > 0)
        self.capacity = capacity
        storage.reserveCapacity(capacity)
    }

    var values: [String] {
        guard storage.count == capacity else { return storage }
        return Array(storage[nextIndex...]) + Array(storage[..<nextIndex])
    }

    mutating func append(_ value: String) {
        guard storage.count == capacity else {
            storage.append(value)
            return
        }

        storage[nextIndex] = value
        nextIndex = (nextIndex + 1) % capacity
        droppedCount += 1
    }
}

/// Retains phone samples until their durable map decision is published.
///
/// Rust emits polled write outcomes in enqueue order. Keeping that order here is important because
/// point sequences restart at zero for every ride; a dictionary keyed only by sequence would let a
/// late outcome for an older ride consume a newer ride's sample.
struct PendingPhoneLocationQueue {
    private struct Entry {
        let sequence: UInt64
        let sample: MobilePhoneLocationSampleDto
    }

    private var entries = [Entry]()

    var isEmpty: Bool { entries.isEmpty }

    mutating func append(_ sample: MobilePhoneLocationSampleDto, sequence: UInt64) {
        entries.append(Entry(sequence: sequence, sample: sample))
    }

    mutating func take(for decision: MobileRideMapDecisionDto) -> MobilePhoneLocationSampleDto? {
        let index: Int
        switch decision {
        case let .accepted(point, _):
            guard let matchingIndex = entries.firstIndex(where: { $0.sequence == point.sequence }) else {
                return nil
            }
            index = matchingIndex
        case .rejected, .ignored, .storageError:
            guard entries.isEmpty == false else { return nil }
            index = entries.startIndex
        case .pending:
            return nil
        }
        return entries.remove(at: index).sample
    }
}

/// Tracks the newest location admitted by the map recorder for timestamp ordering.
///
/// A location is admitted when Rust accepts it immediately or queues it for durable
/// persistence. Rejected, ignored, and storage-error outcomes must not move this
/// boundary, because they were not part of the recorded route.
struct LocationTimestampAdmission {
    private(set) var lastAcceptedTimestamp: Date?

    mutating func record(
        _ timestamp: Date,
        decision: MobileRideMapDecisionDto
    ) {
        switch decision {
        case .accepted, .pending:
            guard timestamp.timeIntervalSinceReferenceDate.isFinite else { return }
            lastAcceptedTimestamp = timestamp
        case .rejected, .ignored, .storageError:
            break
        }
    }

    mutating func reset() {
        lastAcceptedTimestamp = nil
    }
}

public final class CutoutSessionCore: NSObject {
    public var rideSessionStateHandle: CutoutSessionStateHandle { rustSessionState }
    public var rideMapStateHandle: MobileRideMapState { rideMapState }
    public private(set) var rideMapStorageError: String?
    public private(set) var displayState = RideDisplayState()
    public private(set) var phase = SessionConnectionPhase.starting
    public var records: [String] { diagnosticLog.values }
    public var droppedRecordCount: Int { diagnosticLog.droppedCount }
    public private(set) var hasObservedSpeedSnapshot = false
    public private(set) var scanState = DevicePickerScanState(status: .idle, rows: [])
    public private(set) var settingsReadback: SettingsReadback?
    public private(set) var faultHistoryReadback: FaultHistoryReadback?
    public private(set) var bmsSnapshot: BmsSnapshot?
    public private(set) var phoneLocationSnapshot = MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    public private(set) var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?
    public private(set) var rideMapAvailability = MobileRideMapAvailability.checking

    public var onDisplayStateChange: ((RideDisplayState) -> Void)?
    public var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    public var onReconnectScheduled: ((SessionConnectionRetry) -> Void)?
    public var onRecord: ((String) -> Void)?
    public var onCaptureEvent: ((CaptureEvent) -> Void)?
    public var onScanStateChange: ((DevicePickerScanState) -> Void)?
    public var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    public var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    public var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    public var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)?
    public var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)?
    public var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)?
    public var onRideMapErrorChange: ((MobileRideMapError) -> Void)?
    public var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)?
    public var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
    public var onBluetoothRestorationResolved: ((String?) -> Void)?

    private let clock: MonotonicClock
    private let wallClock: WallClock
    private var locationTimestampAdmission = LocationTimestampAdmission()
    private var diagnosticLog = BoundedDiagnosticLog(capacity: 2_048)
    private let bleQueue = DispatchQueue(label: "io.cutout.corebluetooth")
    private let bleQueueKey = DispatchSpecificKey<Void>()
    private let rustSessionState: CutoutSessionStateHandle
    private let selectedDeviceStore: DevicePickerSelectionStore
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var advertisement: CoreBluetoothAdvertisement?
    private var discoveredPeripherals: [CoreBluetoothPeripheralIdentifier: CBPeripheral] = [:]
    private var liveOwner: CoreBluetoothLiveSessionOwner?
    private var selectedModel: ElectricUnicycleModel?
    private var selectedRoute: DevicePickerConnectionRoute?
    private var hasObservedRideMapConnection = false
    private var chargeEstimateProfile: ChargeEstimateProfile?
    private var vescBoardProfile: VescBoardProfile?
    private var isRecordOnly = false
    private var isProbeOnly = false
    private var subscribedCharacteristics: [BluetoothUuid: CBCharacteristic] = [:]
    private var pendingServiceDiscoveries = Set<CBUUID>()
    private var suppressReconnect = false
    private let reconnectController: ConnectionReconnectController
    private let reconnectJitter: () -> Double
    private var captureStartedAt: MonotonicMilliseconds?
    private var captureNotificationCount: UInt64 = 0
    private var captureBuilder: MobilePevcapCaptureBuilder?
    private var captureFileURL: URL?
    private var bmsPages: [BmsPageKey: BmsSnapshot] = [:]
    private let deviceDetectionSession: DeviceDetectionSession
    private let identificationProbeTransport: IdentificationProbeTransportCoordinator
    private var begodeProbeExpiryWorkItem: DispatchWorkItem?
    private var pendingDisplayState: RideDisplayState?
    private var pendingDisplayStateQueuedAt: MonotonicMilliseconds?
    private var displayPublishWorkItem: DispatchWorkItem?
    private var lastDisplayPublication: MonotonicMilliseconds?
    private var lastPublishedWarningSeverity: EucRideWarningSeverity?
    private let phoneLocationState = MobilePhoneLocationState()
    private let admittedPhoneLocationState = MobilePhoneLocationState()
    private let rideMapPollScheduler: any RideMapWritePollingScheduling
    private let pendingPhoneLocationLock = NSLock()
    private var pendingPhoneLocations = PendingPhoneLocationQueue()
    private var rideMapPollScheduled = false
    private let rideMapState: MobileRideMapState
    private var didRequestAlwaysLocationAuthorization = false
    private var didResolveBluetoothRestoration = false
#if DEBUG
    private let testScript: CutoutSessionTestScript?
    private var testScriptWorkItem: DispatchWorkItem?
    private var testScriptUpdateWorkItem: DispatchWorkItem?
    private var testScriptDidReconnect = false
#endif
    private lazy var locationManager: CLLocationManager = {
        let manager = CLLocationManager()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBestForNavigation
        manager.activityType = .fitness
        manager.allowsBackgroundLocationUpdates = true
        return manager
    }()

    public override convenience init() {
        self.init(clock: MonotonicClock())
    }

    /// Starts a fresh location-admission boundary for a new ride-map recording.
    public func resetRideMapLocationAdmission() {
        locationTimestampAdmission.reset()
    }

#if DEBUG
    public convenience init(testScript: CutoutSessionTestScript) {
        self.init(clock: MonotonicClock(), testScript: testScript)
    }

    init(
        clock: MonotonicClock,
        wallClock: WallClock = WallClock(),
        testScript: CutoutSessionTestScript? = nil,
        reconnectScheduler: any ConnectionReconnectScheduling = MainQueueReconnectScheduler(),
        reconnectJitter: @escaping () -> Double = { Double.random(in: 0...1) },
        rideMapPollScheduler: any RideMapWritePollingScheduling = MainQueueRideMapPollScheduler(),
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore()
    ) {
        let rustSessionState = CutoutSessionStateHandle()
        self.rustSessionState = rustSessionState
        let deviceDetectionSession = DeviceDetectionSession(sessionState: rustSessionState)
        self.deviceDetectionSession = deviceDetectionSession
        self.identificationProbeTransport = IdentificationProbeTransportCoordinator(
            detectionSession: deviceDetectionSession
        )
        self.clock = clock
        self.wallClock = wallClock
        self.locationTimestampAdmission = LocationTimestampAdmission()
        self.testScript = testScript
        self.reconnectController = ConnectionReconnectController(scheduler: reconnectScheduler)
        self.reconnectJitter = reconnectJitter
        self.rideMapPollScheduler = rideMapPollScheduler
        self.selectedDeviceStore = selectedDeviceStore
        if testScript == nil {
            let storage = Self.makeRideMapState()
            self.rideMapState = storage.state
            self.rideMapStorageError = storage.error
        } else {
            self.rideMapState = MobileRideMapState()
            self.rideMapStorageError = nil
        }
        super.init()
        bleQueue.setSpecific(key: bleQueueKey, value: ())
        if rideMapStorageError != nil {
            rideMapAvailability = .storageUnavailable
        }
    }
#else
    init(
        clock: MonotonicClock,
        wallClock: WallClock = WallClock(),
        rideMapPollScheduler: any RideMapWritePollingScheduling = MainQueueRideMapPollScheduler(),
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore()
    ) {
        let rustSessionState = CutoutSessionStateHandle()
        self.rustSessionState = rustSessionState
        let deviceDetectionSession = DeviceDetectionSession(sessionState: rustSessionState)
        self.deviceDetectionSession = deviceDetectionSession
        self.identificationProbeTransport = IdentificationProbeTransportCoordinator(
            detectionSession: deviceDetectionSession
        )
        self.clock = clock
        self.wallClock = wallClock
        self.locationTimestampAdmission = LocationTimestampAdmission()
        self.reconnectController = ConnectionReconnectController(scheduler: MainQueueReconnectScheduler())
        self.reconnectJitter = { Double.random(in: 0...1) }
        self.rideMapPollScheduler = rideMapPollScheduler
        self.selectedDeviceStore = selectedDeviceStore
        let storage = Self.makeRideMapState()
        self.rideMapState = storage.state
        self.rideMapStorageError = storage.error
        super.init()
        bleQueue.setSpecific(key: bleQueueKey, value: ())
        if rideMapStorageError != nil {
            rideMapAvailability = .storageUnavailable
        }
    }
#endif

    private static func makeRideMapState() -> (state: MobileRideMapState, error: String?) {
        guard let database = RustPersistenceStore.shared else {
            let error = "Rust ride database is unavailable"
            return (MobileRideMapState(storageUnavailable: error), error)
        }
        let state = MobileRideMapState(database: database)
        return (state, state.initializationError.map(String.init(describing:)))
    }

    public func start() {
#if DEBUG
        if let testScript {
            publishOnMain { self.onBluetoothRestorationResolved?(nil) }
            start(testScript: testScript)
            return
        }
#endif
        _ = locationManager
        updateRideMapAvailability(locationManager.authorizationStatus)
        startLocationUpdatesIfAuthorized(locationManager)
        return onBleQueue {
            guard central == nil else {
                return
            }
            #if os(iOS)
            central = CBCentralManager(
                delegate: self,
                queue: bleQueue,
                options: CoreBluetoothRestorationPolicy.centralManagerOptions
            )
            #else
            central = CBCentralManager(delegate: self, queue: bleQueue)
            #endif
        }
    }

    func observeAdvertisement(_ advertisement: CoreBluetoothAdvertisement) {
        onBleQueue {
            let advertisement = advertisement.withVescNordicUartFallbackName()
            _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
            let snapshot = rustSessionState.observeDiscovery(observation: DiscoveryObservation(advertisement))
            scanState = DevicePickerScanState(status: .scanning, discoverySnapshot: snapshot)
            publishScanState()
        }
    }

    @discardableResult
    public func pair(platformIdentifier: String) -> Bool {
#if DEBUG
        if let testScript {
            return onBleQueue { pair(testScript: testScript, platformIdentifier: platformIdentifier) }
        }
#endif
        return onBleQueue {
            let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
            let snapshot = rustSessionState.selectDiscoveredPlatform(platformIdentifier: platformIdentifier)
            let advertisement = snapshot.advertisement(platformIdentifier: platformIdentifier)
            let support = snapshot.pickerCandidates
                .first(where: { $0.platformIdentifier == platformIdentifier })
                .map(DevicePickerCandidateSupport.init)
            return connectIfReady(
                peripheral: discoveredPeripherals[identifier],
                advertisement: advertisement,
                route: support?.connectionRoute,
                model: support?.electricUnicycleModel
            )
        }
    }

    @discardableResult
    public func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool {
#if DEBUG
        if let testScript {
            return onBleQueue {
                pair(testScript: testScript, platformIdentifier: platformIdentifier, model: model)
            }
        }
#endif
        return onBleQueue {
            let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
            let snapshot = rustSessionState.selectDiscoveredPlatform(platformIdentifier: platformIdentifier)
            return connectIfReady(
                peripheral: discoveredPeripherals[identifier],
                advertisement: snapshot.advertisement(platformIdentifier: platformIdentifier),
                route: .electricUnicycle,
                model: model
            )
        }
    }

    @discardableResult
    public func probe(platformIdentifier: String) -> Bool {
#if DEBUG
        if let testScript {
            return onBleQueue {
                pair(testScript: testScript, platformIdentifier: platformIdentifier, model: .falcon)
            }
        }
#endif
        return onBleQueue {
            let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
            let snapshot = rustSessionState.selectDiscoveredPlatform(platformIdentifier: platformIdentifier)
            guard
                let peripheral = discoveredPeripherals[identifier],
                let advertisement = snapshot.advertisement(platformIdentifier: platformIdentifier)
            else {
                return false
            }
            connectProbeOnly(to: peripheral, using: advertisement)
            return true
        }
    }

    @discardableResult
    public func recordOnly(platformIdentifier: String, note: String? = nil, annotations: [String] = []) -> Bool {
#if DEBUG
        if let testScript {
            guard platformIdentifier == testScript.candidate.platformIdentifier else { return false }
            if testScript.flushCaptureSucceeds {
                return onBleQueue {
                    isRecordOnly = true
                    startCapture(
                        reason: note ?? "record-only",
                        annotations: annotations,
                        evidence: "simulator_fixture"
                    )
                    guard captureBuilder != nil else {
                        isRecordOnly = false
                        return false
                    }
                    publishCaptureEvent(.progress(captureProgress()))
                    return true
                }
            }
            let fileURL = URL(fileURLWithPath: "/tmp/ui-test.capture")
            captureFileURL = fileURL
            isRecordOnly = true
            publishCaptureEvent(.started(fileURL: fileURL))
            publishCaptureEvent(.progress(CaptureProgress(
                elapsedMilliseconds: 63_000,
                notificationCount: 42,
                fileSizeBytes: 12_288,
                queuedMessageCount: 0,
                writerError: nil
            )))
            return true
        }
#endif
        return onBleQueue {
            let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
            let snapshot = rustSessionState.selectDiscoveredPlatform(platformIdentifier: platformIdentifier)
            guard
                let peripheral = discoveredPeripherals[identifier],
                let advertisement = snapshot.advertisement(platformIdentifier: platformIdentifier)
            else {
                return false
            }
            connectRecordOnly(to: peripheral, using: advertisement, note: note, annotations: annotations)
            return true
        }
    }

    public func annotateCapture(label: String) {
        annotateCapture(key: "capture_label", value: label)
    }

    public func annotateCapture(key: String, value: String) {
        onBleQueue {
            let annotation = pevcapAnnotation(key: key, value: value)
            _ = captureBuilder?.addAnnotation(annotation: annotation)
            record(annotation)
        }
    }

    public func flushCapture() async -> Bool {
#if DEBUG
        if let testScript, !testScript.flushCaptureSucceeds {
            return false
        }
#endif
        if DispatchQueue.getSpecific(key: bleQueueKey) != nil {
            return captureBuilder?.flushWriter() ?? false
        }
        let queuedAt = clock.now()
        return await withCheckedContinuation { continuation in
            let workItem = DispatchWorkItem(qos: .default, flags: .enforceQoS) { [self] in
                let result = captureBuilder?.flushWriter() ?? false
                let waitMilliseconds = clock.now().elapsed(since: queuedAt).rawValue
                if waitMilliseconds > 0 {
                    record("ble_queue_wait_ms=\(waitMilliseconds)")
                }
                continuation.resume(returning: result)
            }
            bleQueue.async(execute: workItem)
        }
    }

    public func disconnectAndScan() {
        onBleQueue { disconnectAndScanOnBleQueue() }
    }

    /// Configures the Rust-owned charge estimate profile for the active or next connection.
    public func configureChargeEstimate(profile: ChargeEstimateProfile) {
        onBleQueue {
            chargeEstimateProfile = profile
            liveOwner?.configureChargeEstimate(profile: profile)
        }
    }

    /// Removes the charge estimate profile for the active connection.
    public func clearChargeEstimateProfile() {
        onBleQueue {
            chargeEstimateProfile = nil
            liveOwner?.clearChargeEstimateProfile()
        }
    }

    /// Configures the board and battery facts selected for the next VESC connection.
    public func configureVescBoard(profile: VescBoardProfile) {
        onBleQueue {
            vescBoardProfile = profile
        }
    }

    /// Removes the selected VESC board and battery facts.
    public func clearVescBoardProfile() {
        onBleQueue {
            vescBoardProfile = nil
        }
    }

#if DEBUG
    private func start(testScript: CutoutSessionTestScript) {
        onBleQueue {
            testScriptWorkItem?.cancel()
            testScriptUpdateWorkItem?.cancel()
            testScriptDidReconnect = false
            displayState = RideDisplayState()
            publishDisplayState()
            switch testScript.initialBluetoothState {
            case .scanning:
                break
            case .unavailable:
                scanState = DevicePickerScanState(status: .bluetoothUnavailable, rows: [])
                publishScanState()
                setPhase(.bluetoothUnavailable(rawState: 4))
                return
            case .permissionDenied:
                scanState = .permissionDenied
                publishScanState()
                setPhase(.bluetoothPermissionDenied)
                return
            }
            scanState = DevicePickerScanState(status: .idle, rows: [testScript.candidate.pickerRow])
            publishScanState()
            setPhase(.scanning)
            if testScript.startsLive {
                _ = pair(testScript: testScript, platformIdentifier: testScript.candidate.platformIdentifier)
            }
        }
    }

    private func pair(
        testScript: CutoutSessionTestScript,
        platformIdentifier: String,
        model: ElectricUnicycleModel? = nil
    ) -> Bool {
        guard platformIdentifier == testScript.candidate.platformIdentifier else { return false }
        let route: DevicePickerConnectionRoute
        let candidateModel: ElectricUnicycleModel?
        switch testScript.candidate.support {
        case .supported(let supportedRoute, let supportedModel),
             .provisionalRoute(let supportedRoute, let supportedModel):
            guard let supportedRoute else { return false }
            route = supportedRoute
            candidateModel = supportedModel
        case .probeRecommended where model != nil:
            route = .electricUnicycle
            candidateModel = nil
        default:
            return false
        }
        let selectedModel = model ?? candidateModel
        if route == .electricUnicycle, selectedModel == nil {
            return false
        }

        self.selectedRoute = route
        self.selectedModel = selectedModel
        ensureRideMapRecordingForConnection(platformIdentifier: platformIdentifier)
        testScriptWorkItem?.cancel()
        testScriptUpdateWorkItem?.cancel()
        setPhase(.discoveringServices)
        setPhase(.subscribing)
        let work = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                self?.finish(testScript: testScript)
            }
        }
        testScriptWorkItem = work
        DispatchQueue.main.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: testScript.connectionDelayMilliseconds)),
            execute: work
        )
        return true
    }

    private func finish(testScript: CutoutSessionTestScript) {
        if let failure = testScript.identificationProbeFailure {
            setPhase(.failed(.identificationFailed(failure)))
            return
        }
        if testScript.failsConnection {
            setPhase(.failed(.connectFailed("deterministic fixture")))
            guard testScript.emitsLateLiveAfterFailure else { return }
            let work = DispatchWorkItem { [weak self] in
                self?.onBleQueue {
                    self?.emit(testScript: testScript)
                }
            }
            testScriptWorkItem = work
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(500), execute: work)
            return
        }
        emit(testScript: testScript)
    }

    private func emit(testScript: CutoutSessionTestScript) {
        if testScript.startsLive {
            protocolIdentityCandidate = testScript.candidate
            publishProtocolIdentityCandidate()
        }
        guard let telemetry = testScript.telemetry else {
            setPhase(.live)
            return
        }
        let now = clock.now()
        let receivedAt = if testScript.emitsStaleTelemetry {
            MonotonicMilliseconds(
                now.rawValue > RideTelemetryFreshnessPolicy.staleAfter.rawValue
                    ? now.rawValue - RideTelemetryFreshnessPolicy.staleAfter.rawValue - 1
                    : 0
            )
        } else {
            now
        }
        let actions = testScript.bmsSnapshot.map { [SessionAction.withBmsSnapshot($0)] } ?? []
        applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: telemetry, actions: actions),
            receivedAt: receivedAt
        )
        scheduleTestTelemetryUpdateIfNeeded(testScript)
        scheduleTestReconnectIfNeeded(testScript)
        scheduleTestBluetoothLossIfNeeded(testScript)
    }

    private func scheduleTestTelemetryUpdateIfNeeded(_ testScript: CutoutSessionTestScript) {
        guard let telemetry = testScript.telemetryUpdate else { return }
        let update = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                guard let self else { return }
                self.applyNotificationStep(
                    CoreBluetoothSessionStep(operations: [], snapshot: telemetry),
                    receivedAt: self.clock.now()
                )
            }
        }
        testScriptUpdateWorkItem = update
        DispatchQueue.main.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: testScript.telemetryUpdateDelayMilliseconds)),
            execute: update
        )
    }

    private func scheduleTestReconnectIfNeeded(_ testScript: CutoutSessionTestScript) {
        guard testScript.reconnectsAfterFirstLive, !testScriptDidReconnect else { return }
        testScriptDidReconnect = true
        let reconnect = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                guard let self else { return }
                self.setPhase(.discoveringServices)
                self.publishOnMain {
                    self.onReconnectScheduled?(
                        SessionConnectionRetry(
                            platformIdentifier: testScript.candidate.platformIdentifier,
                            attempt: 1,
                            deadline: self.clock.now(),
                            failure: .connectFailed("deterministic reconnect")
                        )
                    )
                }
                let resume = DispatchWorkItem { [weak self] in
                    self?.onBleQueue {
                        self?.setPhase(.subscribing)
                        self?.emit(testScript: testScript)
                    }
                }
                self.testScriptWorkItem = resume
                DispatchQueue.main.asyncAfter(
                    deadline: .now() + .milliseconds(Int(clamping: testScript.reconnectDelayMilliseconds)),
                    execute: resume
                )
            }
        }
        testScriptWorkItem = reconnect
        DispatchQueue.main.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: testScript.reconnectAfterLiveMilliseconds)),
            execute: reconnect
        )
    }

    private func scheduleTestBluetoothLossIfNeeded(_ testScript: CutoutSessionTestScript) {
        guard let delay = testScript.bluetoothLossAfterFirstLiveMilliseconds else { return }
        let loss = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                guard let self else { return }
                self.scanState = DevicePickerScanState(status: .bluetoothUnavailable, rows: [])
                self.publishScanState()
                self.setPhase(.bluetoothUnavailable(rawState: 4))
            }
        }
        testScriptWorkItem = loss
        DispatchQueue.main.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: delay)),
            execute: loss
        )
    }
#endif

    private func disconnectAndScanOnBleQueue() {
#if DEBUG
        testScriptWorkItem?.cancel()
        testScriptWorkItem = nil
        testScriptUpdateWorkItem?.cancel()
        testScriptUpdateWorkItem = nil
#endif
        suppressReconnect = true
        cancelPendingReconnect()
#if DEBUG
        if testScript != nil, isRecordOnly, captureBuilder != nil {
            finishCaptureAfterLinkDown()
        } else if testScript != nil, isRecordOnly, let completedCaptureURL = captureFileURL {
            captureFileURL = nil
            publishCaptureEvent(.finished(fileURL: completedCaptureURL))
        } else {
            finishCaptureAfterLinkDown()
        }
#else
        finishCaptureAfterLinkDown()
#endif
        isRecordOnly = false
        isProbeOnly = false
        selectedModel = nil
        selectedRoute = nil
        chargeEstimateProfile = nil
        vescBoardProfile = nil
        liveOwner = nil
        deviceDetectionSession.reset()
        clearPendingBegodeProbeResponses()
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()
        displayState = RideDisplayState()
        hasObservedSpeedSnapshot = false
        clearSettingsReadback()
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        publishDisplayState()

        if let peripheral {
            central?.cancelPeripheralConnection(peripheral)
        }
        peripheral = nil
        advertisement = nil

        #if DEBUG
        if let testScript {
            scanState = DevicePickerScanState(status: .idle, rows: [testScript.candidate.pickerRow])
        } else {
            scanState = DevicePickerScanState(status: .scanning, discoverySnapshot: rustSessionState.discoverySnapshot())
        }
        #else
        scanState = DevicePickerScanState(status: .scanning, discoverySnapshot: rustSessionState.discoverySnapshot())
        #endif
        publishScanState()
        setPhase(.scanning)
        central?.scanForPeripherals(withServices: nil)
    }

    public func now() -> MonotonicMilliseconds {
        clock.now()
    }

    func applyLinkUpStep(_ step: CoreBluetoothSessionStep) {
        record("link_operations=\(step.operations.map(String.init(describing:)).joined(separator: ","))")
        guard acceptCaptureWrite(captureBuilder?.recordLinkUp(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
            maxWriteLen: peripheral.map {
                MobileTransportWriteLimitDto(bytes: UInt16(clamping: $0.maximumWriteValueLength(for: .withoutResponse)))
            }
        ) ?? false) else { return }
        if let snapshot = step.snapshot {
            hasObservedSpeedSnapshot = snapshot.speed?.value != nil
        }
        setPhase(.subscribing)
    }

    func applyNotificationStep(_ step: CoreBluetoothSessionStep, receivedAt: MonotonicMilliseconds) {
        cancelPendingReconnect()
        step.actions.forEach(applySessionAction)
        if let peripheral {
            ensureRideMapRecordingIfNeeded(platformIdentifier: peripheral.identifier.uuidString)
        }
        if step.actions.contains(where: { $0.rawTelemetry != nil }) {
            do {
                let observation = try rideMapState.observeTelemetry(atMs: receivedAt.rawValue)
                if observation == .observed {
                    publishRideMapSnapshot()
                }
            } catch {
                publishRideMapError(error)
            }
        }
        let snapshot = step.snapshot
        displayState = displayState.reducing(snapshot: snapshot, receivedAt: receivedAt)
        hasObservedSpeedSnapshot = hasObservedSpeedSnapshot || snapshot?.speed?.value != nil
        publishDisplayState()
        setPhase(.live)
    }

    private func applySessionAction(_ action: SessionAction) {
        switch action.kind {
        case .settingsReadback:
            settingsReadback = action.settingsReadback
            publishSettingsReadback()
        case .faultHistoryReadback:
            faultHistoryReadback = action.faultHistoryReadback
            publishFaultHistoryReadback()
        case .bmsSnapshot:
            let mergedSnapshot = mergedBmsSnapshot(with: action.bmsSnapshot)
            guard mergedSnapshot != bmsSnapshot else {
                return
            }
            bmsSnapshot = mergedSnapshot
            publishBmsSnapshot()
        case .event:
            applyProtocolIdentityModelId(action.veteranProtocolModelId)
        case .subscribe, .write, .disconnect, .notificationIngest:
            break
        }
    }

    private func mergedBmsSnapshot(with update: BmsSnapshot?) -> BmsSnapshot? {
        guard let update else {
            return bmsSnapshot
        }

        guard update.availability == .available else {
            bmsPages.removeAll()
            return update
        }

        let pageKey = BmsPageKey(snapshot: update)
        guard pageKey.isKnownPage || bmsPages.isEmpty else {
            return aggregateBmsSnapshot()?.mergingBmsPage(update) ?? update.withoutPageCursor()
        }

        bmsPages[pageKey] = update
        return aggregateBmsSnapshot()
    }

    private func aggregateBmsSnapshot() -> BmsSnapshot? {
        bmsPages.values
            .sorted(by: BmsPageKey.sortSnapshots)
            .reduce(nil as BmsSnapshot?) { aggregate, page in
                aggregate?.mergingBmsPage(page) ?? page.withoutPageCursor()
            }
    }

    private func applyProtocolIdentityModelId(_ modelId: UInt16?) {
        let discovery = rustSessionState.discoverySnapshot()
        switch (modelId, advertisement ?? discovery.selectedAdvertisement ?? discovery.lastAdvertisement) {
        case let (.some(modelId), .some(advertisement)):
            let candidate = mobileDiscoveryCandidateFromVeteranProtocolIdentity(
                platformIdentifier: advertisement.peripheralIdentifier.rawValue,
                displayName: advertisement.localName
                    ?? protocolIdentityFallbackDisplayName(protocolFamily: .veteranLeaperkimNosfet),
                modelId: modelId
            )
            protocolIdentityCandidate = DevicePickerDiscoveryCandidate(candidate: candidate)
            publishProtocolIdentityCandidate()
            record("protocol_identity=\(candidate.detail)")
            updateCaptureIdentity()
        case (.none, _), (_, .none):
            break
        }
    }

    private func clearSettingsReadback() {
        guard settingsReadback != nil else {
            return
        }
        settingsReadback = nil
        publishSettingsReadback()
    }

    private func clearFaultHistoryReadback() {
        guard faultHistoryReadback != nil else {
            return
        }
        faultHistoryReadback = nil
        publishFaultHistoryReadback()
    }

    private func clearBmsSnapshot() {
        guard bmsSnapshot != nil else {
            return
        }
        bmsSnapshot = nil
        bmsPages.removeAll()
        publishBmsSnapshot()
    }

    private func clearProtocolIdentityCandidate() {
        guard protocolIdentityCandidate != nil else {
            return
        }
        protocolIdentityCandidate = nil
        publishProtocolIdentityCandidate()
    }

    private func publishDetectionIdentityCandidate(_ resolution: DeviceDetectionResolution) {
        guard resolution.protocolFamily != nil
            || resolution.protocolConflict
            || resolution.modelBanner != nil
            || resolution.firmwareBanner != nil
            || resolution.imuBanner != nil
            || resolution.missingProbeResponse != nil
            || resolution.malformedProbeResponse != nil
        else {
            return
        }
        let discovery = rustSessionState.discoverySnapshot()
        guard let advertisement = advertisement ?? discovery.selectedAdvertisement ?? discovery.lastAdvertisement else {
            return
        }
        let candidate = resolution.discoveryCandidate(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            displayName: advertisement.localName
                ?? protocolIdentityFallbackDisplayName(protocolFamily: resolution.protocolFamily)
        )
        guard candidate.isPickerCandidate else {
            return
        }
        let pickerCandidate = DevicePickerDiscoveryCandidate(candidate: candidate)
        guard pickerCandidate != protocolIdentityCandidate else {
            return
        }
        protocolIdentityCandidate = pickerCandidate
        publishProtocolIdentityCandidate()
        record("protocol_identity=\(candidate.detail)")
        updateCaptureIdentity()
    }

    private func setPhase(_ phase: SessionConnectionPhase) {
        self.phase = phase
        publishOnMain { self.onPhaseChange?(phase) }
    }

    private func connect(
        to peripheral: CBPeripheral,
        using advertisement: CoreBluetoothAdvertisement,
        model: ElectricUnicycleModel
    ) {
        cancelPendingReconnect()
        suppressReconnect = false
        isRecordOnly = false
        isProbeOnly = false
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = model
        selectedRoute = .electricUnicycle
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        startCapture(reason: "pair", annotations: ["route=electric_unicycle"])
        clearSettingsReadback()
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        peripheral.delegate = self
        setPhase(.connecting(model: model))
        central?.stopScan()
        central?.connect(peripheral)
    }

    private func connectRecordOnly(to peripheral: CBPeripheral, using advertisement: CoreBluetoothAdvertisement, note: String?, annotations: [String]) {
        cancelPendingReconnect()
        suppressReconnect = false
        isRecordOnly = true
        isProbeOnly = false
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = nil
        selectedRoute = nil
        liveOwner = nil
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        startCapture(
            reason: "record-only",
            annotations: ["route=record_only"] + annotations + (note.map {
                [pevcapAnnotation(key: "user_note", value: $0)]
            } ?? [])
        )
        clearSettingsReadback()
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        peripheral.delegate = self
        setPhase(.discoveringServices)
        central?.stopScan()
        central?.connect(peripheral)
    }

    private func connectProbeOnly(to peripheral: CBPeripheral, using advertisement: CoreBluetoothAdvertisement) {
        cancelPendingReconnect()
        suppressReconnect = false
        isRecordOnly = false
        isProbeOnly = true
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = nil
        selectedRoute = nil
        liveOwner = nil
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        startCapture(reason: "identification-probe", annotations: ["route=probe_only"])
        clearSettingsReadback()
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        peripheral.delegate = self
        setPhase(.discoveringServices)
        central?.stopScan()
        central?.connect(peripheral)
    }

    private func connectVescOnewheel(to peripheral: CBPeripheral, using advertisement: CoreBluetoothAdvertisement) {
        cancelPendingReconnect()
        suppressReconnect = false
        isRecordOnly = false
        isProbeOnly = false
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = nil
        selectedRoute = .vescOnewheel
        liveOwner = nil
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        startCapture(reason: "pair", annotations: ["route=vesc_onewheel"])
        clearSettingsReadback()
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        peripheral.delegate = self
        setPhase(.discoveringServices)
        central?.stopScan()
        central?.connect(peripheral)
    }

    private func connectIfReady(
        peripheral: CBPeripheral?,
        advertisement: CoreBluetoothAdvertisement?,
        route: DevicePickerConnectionRoute?,
        model: ElectricUnicycleModel?
    ) -> Bool {
        switch (peripheral, advertisement, route) {
        case let (.some(peripheral), .some(advertisement), .electricUnicycle?):
            guard let model else { return false }
            connect(to: peripheral, using: advertisement, model: model)
            return true
        case let (.some(peripheral), .some(advertisement), .vescOnewheel?):
            connectVescOnewheel(to: peripheral, using: advertisement)
            return true
        case (.none, _, _), (_, .none, _), (_, _, .none):
            return false
        }
    }

    private func buildOwner(for peripheral: CBPeripheral) {
        guard liveOwner == nil, let advertisement, let selectedRoute else {
            return
        }
        do {
            liveOwner = CoreBluetoothLiveSessionOwner(
                session: try liveSession(for: selectedRoute),
                advertisement: advertisement,
                writeLimit: TransportWriteLimitBytes(23),
                operationSink: self,
                detectionSession: deviceDetectionSession,
                retryCommandOnLinkUp: selectedRoute == .vescOnewheel ? .requestTelemetry : nil,
                executionQueue: bleQueue,
                monotonicClock: clock
            )
            if let chargeEstimateProfile {
                liveOwner?.configureChargeEstimate(profile: chargeEstimateProfile)
            }
            setPhase(.subscribing)
            let inventory = CoreBluetoothGattInventory(services: peripheral.services ?? [])
            liveOwner?.recordInventory(inventory)
            let step = try liveOwner?.handleLinkUp(at: clock.now())
            if let step {
                applyLinkUpStep(step)
            }
        } catch {
            setPhase(.failed(.sessionFailed(error.sessionMessage)))
        }
    }

    private func liveSession(for route: DevicePickerConnectionRoute) throws -> CoreBluetoothSession {
        switch route {
        case .electricUnicycle:
            guard let selectedModel else {
                throw CutoutSessionError.unexpectedStepError("missing EUC model")
            }
            return try .electricUnicycle(
                model: selectedModel,
                deviceIdentity: advertisement?.peripheralIdentifier.rawValue
            )
        case .vescOnewheel:
            if let vescBoardProfile {
                return .vescOnewheel(boardProfile: vescBoardProfile)
            }
            return .vescOnewheel()
        }
    }

    private var discoveryServiceUuidsForSelectedRoute: [CBUUID]? {
        guard !isRecordOnly else {
            return nil
        }
        switch selectedRoute {
        case .electricUnicycle:
            return CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids
        case .vescOnewheel:
            return nil
        case nil:
            return CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids
        }
    }

    private func handleDisconnect(from peripheral: CBPeripheral, error: Error?) {
        guard self.peripheral?.identifier == peripheral.identifier else {
            return
        }
        handleTransportTermination(
            platformIdentifier: peripheral.identifier.uuidString,
            error: error,
            reconnect: { [weak self, weak peripheral] in
                guard let self, let peripheral else { return }
                self.central?.connect(peripheral)
            }
        )
    }

    func handleTransportTermination(
        platformIdentifier: String,
        error: Error?,
        reconnect: @escaping () -> Void
    ) {
        onBleQueue {
            handleTransportTerminationOnBleQueue(
                platformIdentifier: platformIdentifier,
                error: error,
                reconnect: reconnect
            )
        }
    }

    private func handleTransportTerminationOnBleQueue(
        platformIdentifier: String,
        error: Error?,
        reconnect: @escaping () -> Void
    ) {
        record("disconnected=\(platformIdentifier) error=\(String(describing: error))")
#if DEBUG
        testScriptWorkItem?.cancel()
        testScriptWorkItem = nil
        testScriptUpdateWorkItem?.cancel()
        testScriptUpdateWorkItem = nil
#endif
        markOutstandingBegodeProbeResponsesMissing()
        finishCaptureAfterLinkDown()
        let wasEphemeralConnection = isRecordOnly || isProbeOnly
        let reconnectRoute = selectedRoute
        isRecordOnly = false
        isProbeOnly = false
        selectedRoute = nil
        hasObservedRideMapConnection = false
        liveOwner = nil
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()

        guard !suppressReconnect else {
            suppressReconnect = false
            return
        }

        guard !wasEphemeralConnection else {
            setPhase(.scanning)
            central?.scanForPeripherals(withServices: nil)
            return
        }

        scheduleReconnect(
            platformIdentifier: platformIdentifier,
            route: reconnectRoute,
            error: error,
            reconnect: reconnect
        )
    }

    private func scheduleReconnect(
        platformIdentifier: String,
        route: DevicePickerConnectionRoute?,
        error: Error?,
        reconnect: @escaping () -> Void
    ) {
        guard let schedule = reconnectController.schedule(
            jitter: reconnectJitter(),
            operation: { [weak self] in
                guard let self else { return }
                self.onBleQueue {
                    guard !self.suppressReconnect else { return }
                    self.startCapture(reason: "reconnect", annotations: ["route=\(route?.rawValue ?? "unknown")"])
                    reconnect()
                }
            }
        ) else {
            setPhase(.failed(.connectFailed(error.sessionMessage)))
            central?.scanForPeripherals(withServices: nil)
            return
        }

        switch route {
        case .vescOnewheel:
            selectedRoute = .vescOnewheel
            setPhase(.discoveringServices)
        case .electricUnicycle:
            guard let selectedModel else {
                setPhase(.failed(.connectFailed(error.sessionMessage)))
                return
            }
            selectedRoute = .electricUnicycle
            setPhase(.connecting(model: selectedModel))
        case nil:
            setPhase(.failed(.connectFailed(error.sessionMessage)))
            return
        }

        let now = clock.now().rawValue
        let delay = schedule.delayMilliseconds
        let deadline = MonotonicMilliseconds(now > UInt64.max - delay ? UInt64.max : now + delay)
        let retry = SessionConnectionRetry(
            platformIdentifier: platformIdentifier,
            attempt: schedule.attempt,
            deadline: deadline,
            failure: .connectFailed(error.sessionMessage)
        )
        publishOnMain { self.onReconnectScheduled?(retry) }

        record("reconnect_attempt=\(schedule.attempt) delay_ms=\(schedule.delayMilliseconds)")
    }

    private func cancelPendingReconnect() {
        reconnectController.cancel()
    }

    private func record(_ message: String) {
        diagnosticLog.append(message)
        publishOnMain { self.onRecord?(message) }
    }

    private func publishOnMain(_ work: @escaping () -> Void) {
        if Thread.isMainThread {
            work()
        } else {
            DispatchQueue.main.async(execute: DispatchWorkItem(block: work))
        }
    }

    private func onBleQueue<T>(_ work: () -> T) -> T {
        if DispatchQueue.getSpecific(key: bleQueueKey) != nil {
            return work()
        }
        let queuedAt = clock.now()
        return bleQueue.sync {
            let result = work()
            let waitMilliseconds = clock.now().elapsed(since: queuedAt).rawValue
            if waitMilliseconds > 0 {
                record("ble_queue_wait_ms=\(waitMilliseconds)")
            }
            return result
        }
    }

    private func publishDisplayState() {
        let value = displayState
        let queuedAt = clock.now()
        publishOnMain { self.publishDisplayStateOnMain(value, queuedAt: queuedAt) }
    }

    private func publishDisplayStateOnMain(
        _ value: RideDisplayState,
        queuedAt: MonotonicMilliseconds? = nil
    ) {
        pendingDisplayState = value
        if let queuedAt {
            pendingDisplayStateQueuedAt = queuedAt
        }
        let now = clock.now()
        let intervalMilliseconds: UInt64 = 333
        let elapsed = lastDisplayPublication.map {
            now.elapsed(since: $0).rawValue
        } ?? intervalMilliseconds
        let warningSeverity = EucRideScreenState(
            phase: .live,
            displayState: value
        ).warningState.severity
        let warningChanged = lastPublishedWarningSeverity.map { $0 != warningSeverity } ?? false
        guard elapsed >= intervalMilliseconds || warningChanged else {
            guard displayPublishWorkItem == nil else { return }
            let work = DispatchWorkItem { [weak self] in
                guard let self else { return }
                self.displayPublishWorkItem = nil
                if let pending = self.pendingDisplayState {
                    self.publishDisplayStateOnMain(pending)
                }
            }
            displayPublishWorkItem = work
            DispatchQueue.main.asyncAfter(
                deadline: .now() + .milliseconds(Int(intervalMilliseconds - elapsed)),
                execute: work
            )
            return
        }
        displayPublishWorkItem?.cancel()
        displayPublishWorkItem = nil
        pendingDisplayState = nil
        let publicationDelayMilliseconds = pendingDisplayStateQueuedAt.map {
            now.elapsed(since: $0).rawValue
        } ?? 0
        pendingDisplayStateQueuedAt = nil
        lastDisplayPublication = now
        lastPublishedWarningSeverity = warningSeverity
        onDisplayStateChange?(value)
        onRecord?("snapshot_publication_ms=\(publicationDelayMilliseconds)")
    }

    private func publishScanState() {
        let value = scanState
        publishOnMain { self.onScanStateChange?(value) }
    }

    private func publishSettingsReadback() {
        let value = settingsReadback
        publishOnMain { self.onSettingsReadbackChange?(value) }
    }

    private func publishFaultHistoryReadback() {
        let value = faultHistoryReadback
        publishOnMain { self.onFaultHistoryReadbackChange?(value) }
    }

    private func publishBmsSnapshot() {
        let value = bmsSnapshot
        publishOnMain { self.onBmsSnapshotChange?(value) }
    }

    private func publishPhoneLocationSnapshot() {
        let value = phoneLocationSnapshot
        let receivedAt = clock.now()
        publishOnMain { self.onPhoneLocationSnapshotChange?(value, receivedAt) }
    }

    private func publishRideMapDecision(_ decision: MobileRideMapDecisionDto) {
        // Do not read the Rust map projection from Core Location's callback. The FFI read is
        // mutex-protected (and intentionally cheap today), but keeping it off the callback makes
        // this path safe if the projection gains a durable read or other blocking work later.
        let work = DispatchWorkItem { [weak self] in
            guard let self, let snapshot = self.rideMapState.currentSnapshot() else { return }
            self.onRideMapDecisionChange?(snapshot, decision)
        }
        DispatchQueue.main.async(execute: work)
    }

    private func publishRideMapSnapshot() {
        guard let snapshot = rideMapState.currentSnapshot() else { return }
        publishOnMain { self.onRideMapSnapshotChange?(snapshot) }
    }

    private func publishRideMapError(_ error: Error) {
        guard let error = error as? MobileRideMapError, error != .NoActiveRide else { return }
        publishOnMain { self.onRideMapErrorChange?(error) }
    }

    private func publishProtocolIdentityCandidate() {
        let value = protocolIdentityCandidate
        publishOnMain { self.onProtocolIdentityCandidateChange?(value) }
    }

    private func publishCaptureEvent(_ event: CaptureEvent) {
        publishOnMain { self.onCaptureEvent?(event) }
    }

    private func captureFrame(
        direction: String,
        characteristic: CBUUID,
        service: CBUUID? = nil,
        bytes: Data,
        telemetry: RawTelemetryReadback? = nil
    ) {
        guard let channel = BluetoothUuid(coreBluetoothUuid: characteristic) else {
            return
        }

        switch direction {
        case "notify":
            guard let serviceUuid = service.flatMap(BluetoothUuid.init(coreBluetoothUuid:)) else {
                record("capture_error=notification_missing_service characteristic=\(characteristic.uuidString)")
                publishCaptureEvent(.failed)
                setPhase(.failed(.notificationFailed("missing service UUID for \(characteristic.uuidString)")))
                return
            }
            let location = admittedPhoneLocationState.currentSnapshot().latestSample
            _ = captureBuilder?.recordNotificationWithContext(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                characteristic: channel.bytes,
                service: serviceUuid.bytes,
                bytes: bytes,
                telemetry: telemetry?.dto,
                phoneLocation: location
            )
            record("capture_queue_depth=\(captureBuilder?.writerStatus().queuedMessages ?? 0)")
        case "write_without_response":
            let accepted = captureBuilder?.recordWriteWithoutResponse(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                characteristic: channel.bytes,
                bytes: bytes
            ) ?? false
            guard acceptCaptureWrite(accepted) else { return }
            record("capture_queue_depth=\(captureBuilder?.writerStatus().queuedMessages ?? 0)")
        default:
            return
        }
    }

    private func startCapture(
        reason: String,
        annotations extraAnnotations: [String] = [],
        evidence: String = "hardware_tested"
    ) {
        captureStartedAt = clock.now()
        captureNotificationCount = 0

        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let url = directory.appendingPathComponent("cutout-btle-capture-\(Int(Date().timeIntervalSince1970)).jsonl")
        let builder = MobilePevcapCaptureBuilder(
            wallClockStartUnixMs: MobileWallClockUnixMillisDto(milliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)),
            platformId: advertisement?.peripheralIdentifier.rawValue ?? "ios",
            writeLimit: MobileTransportWriteLimitDto(bytes: 23)
        )
        (advertisement?.advertisedServiceUuids ?? []).forEach { service in
            _ = builder.addAdvertisedService(service: service.bytes)
        }
        [
            "source=ios-app",
            "capture_reason=\(reason)",
            "capture_privacy=private",
            "capture_evidence=\(evidence)",
        ].forEach { _ = builder.addAnnotation(annotation: $0) }
        extraAnnotations.forEach { _ = builder.addAnnotation(annotation: sanitizedPevcapAnnotation($0)) }
        captureBuilder = builder
        guard builder.startWriter(path: url.path) else {
            record("capture_error=writer_start_failed")
            captureBuilder = nil
            captureFileURL = nil
            publishCaptureEvent(.failed)
            setPhase(.failed(.sessionFailed("capture writer failed to start")))
            return
        }
        captureFileURL = url
        record("capture_file=\(url.path)")
        publishCaptureEvent(.started(fileURL: url))
        updateCaptureIdentity()
    }

    private func acceptCaptureWrite(_ accepted: Bool) -> Bool {
        guard !accepted else { return true }
        let status = captureBuilder?.writerStatus()
        record("capture_error=writer_failed \(status?.lastError ?? "unknown")")
        publishCaptureEvent(.failed)
        setPhase(.failed(.sessionFailed("capture writer queue overrun")))
        finishCaptureWriter()
        return false
    }

    private func finishCaptureAfterLinkDown() {
        guard captureBuilder != nil else { return }
        let linkDownAccepted = captureBuilder?.recordLinkDown(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds())
        ) ?? false
        finishCaptureWriter(publishesResult: true, priorWriteSucceeded: linkDownAccepted)
    }

    private func finishCaptureWriter(
        publishesResult: Bool = false,
        priorWriteSucceeded: Bool = true
    ) {
        guard let builder = captureBuilder else { return }
        let completedCaptureURL = captureFileURL
        captureBuilder = nil
        captureFileURL = nil
        captureStartedAt = nil
        let finish = DispatchWorkItem { [weak self] in
            let writerSucceeded = builder.finishWriter()
            let succeeded = priorWriteSucceeded && writerSucceeded
            guard publishesResult, let self else { return }
            self.onBleQueue {
                if succeeded, let completedCaptureURL {
                    self.publishCaptureEvent(.finished(fileURL: completedCaptureURL))
                } else {
                    self.record("capture_error=writer_finish_failed")
                    self.publishCaptureEvent(.failed)
                }
            }
        }
        DispatchQueue.global(qos: .utility).async(execute: finish)
    }

    private func captureElapsedMilliseconds() -> UInt64 {
        guard let captureStartedAt else {
            return 0
        }
        return captureElapsedMilliseconds(since: captureStartedAt)
    }

    func captureElapsedMilliseconds(since captureStartedAt: MonotonicMilliseconds) -> UInt64 {
        clock.now().elapsed(since: captureStartedAt).rawValue
    }

    private func captureProgress() -> CaptureProgress {
        let status = captureBuilder?.writerStatus()
        let attributes = captureFileURL.flatMap { fileURL in
            try? FileManager.default.attributesOfItem(atPath: fileURL.path)
        }
        let fileSizeBytes = (attributes?[.size] as? NSNumber)?.uint64Value ?? 0
        return CaptureProgress(
            elapsedMilliseconds: captureElapsedMilliseconds(),
            notificationCount: captureNotificationCount,
            fileSizeBytes: fileSizeBytes,
            queuedMessageCount: status?.queuedMessages ?? 0,
            writerError: status?.lastError
        )
    }

    private func updateCaptureIdentity() {
        guard let builder = captureBuilder, let identity = pevcapResolvedIdentity() else {
            return
        }
        guard acceptCaptureWrite(builder.setResolvedIdentity(identity: identity)) else {
            return
        }
        if let protocolIdentityCandidate {
            _ = builder.addAnnotation(annotation: pevcapAnnotation(
                key: "resolved_evidence",
                value: protocolIdentityCandidate.evidence
            ))
            _ = builder.addAnnotation(annotation: pevcapAnnotation(
                key: "resolved_detail",
                value: protocolIdentityCandidate.detail
            ))
        }
    }

    private func pevcapResolvedIdentity() -> MobileResolvedIdentityDto? {
        captureResolvedIdentity(protocolIdentityCandidate: protocolIdentityCandidate)
    }
}

func captureResolvedIdentity(
    protocolIdentityCandidate: DevicePickerDiscoveryCandidate?
) -> MobileResolvedIdentityDto? {
    protocolIdentityCandidate?
        .support
        .electricUnicycleModel?
        .pevcapResolvedIdentity(verification: .hardwareVerified)
}

func pevcapAnnotation(key: String, value: String) -> String {
    "\(sanitizePevcapAnnotationComponent(key))=\(sanitizePevcapAnnotationComponent(value))"
}

func sanitizedPevcapAnnotation(_ annotation: String) -> String {
    let parts = annotation.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
    guard parts.count == 2 else {
        return sanitizePevcapAnnotationComponent(annotation)
    }
    return pevcapAnnotation(key: String(parts[0]), value: String(parts[1]))
}

private func sanitizePevcapAnnotationComponent(_ value: String) -> String {
    String(value.map { character in
        switch character {
        case "=", "\n", "\r":
            " "
        default:
            character
        }
    })
}

private extension ElectricUnicycleModel {
    func pevcapResolvedIdentity(verification: MobileVerificationStatusDto) -> MobileResolvedIdentityDto {
        MobileResolvedIdentityDto(
            protocolFamily: pevcapProtocolFamily,
            model: MobileVerifiedStringDto(value: pevcapModelName, verification: verification),
            firmware: nil
        )
    }

    var pevcapProtocolFamily: MobileProtocolFamilyDto {
        switch self {
        case .aero:
            .veteranLeaperkimNosfet
        case .falcon:
            .begodeGotway
        }
    }

    var pevcapModelName: String {
        switch self {
        case .aero:
            "NOSFET Aero"
        case .falcon:
            "Begode Falcon"
        }
    }
}

private struct BmsPageKey: Hashable {
    let selector: UInt8?
    let tag: UInt16?
    let kind: String

    init(snapshot: BmsSnapshot) {
        selector = snapshot.pageSelector
        tag = snapshot.pageTag
        kind = snapshot.pageKind ?? "unknown"
    }

    var isKnownPage: Bool {
        selector != nil || tag != nil || kind != "unknown"
    }

    static func sortSnapshots(_ lhs: BmsSnapshot, _ rhs: BmsSnapshot) -> Bool {
        let lhsKey = BmsPageKey(snapshot: lhs)
        let rhsKey = BmsPageKey(snapshot: rhs)
        switch (lhsKey.tag, rhsKey.tag) {
        case let (lhsTag?, rhsTag?) where lhsTag != rhsTag:
            return lhsTag < rhsTag
        case (nil, _?):
            return false
        case (_?, nil):
            return true
        default:
            break
        }
        switch (lhsKey.selector, rhsKey.selector) {
        case let (lhsSelector?, rhsSelector?) where lhsSelector != rhsSelector:
            return lhsSelector < rhsSelector
        case (nil, _?):
            return false
        case (_?, nil):
            return true
        default:
            return lhsKey.kind < rhsKey.kind
        }
    }
}

private extension DiscoverySnapshot {
    var selectedAdvertisement: CoreBluetoothAdvertisement? {
        selectedPlatformIdentifier.flatMap(advertisement(platformIdentifier:))
    }

    var lastAdvertisement: CoreBluetoothAdvertisement? {
        observations.last.map(CoreBluetoothAdvertisement.init(discoveryObservation:))
    }

    func advertisement(platformIdentifier: String) -> CoreBluetoothAdvertisement? {
        observations
            .last { $0.platformIdentifier == platformIdentifier }
            .map(CoreBluetoothAdvertisement.init(discoveryObservation:))
    }
}

private extension CoreBluetoothAdvertisement {
    func withVescNordicUartFallbackName() -> Self {
        guard
            localName?.isEmpty != false,
            advertisedServiceUuids.contains(.vescNordicUartService)
        else {
            return self
        }
        return Self(
            peripheralIdentifier: peripheralIdentifier,
            localName: protocolIdentityFallbackDisplayName(protocolFamily: .vesc),
            advertisedServiceUuids: advertisedServiceUuids,
            manufacturerData: manufacturerData,
            rssiDbm: rssiDbm
        )
    }
}

private extension CutoutSessionCore {
    func restoreSelectedPeripheral(from restoredPeripherals: [CBPeripheral]) {
        assertOnBleQueue()
        let restoredIdentifiers = restoredPeripherals.map(\.identifier.uuidString)
        guard
            let selectedIdentifier = CoreBluetoothRestorationPolicy.selectedPlatformIdentifier(
                savedPlatformIdentifier: selectedDeviceStore.platformIdentifier,
                restoredPlatformIdentifiers: restoredIdentifiers
            ),
            let restoredPeripheral = restoredPeripherals.first(where: {
                $0.identifier.uuidString == selectedIdentifier
            })
        else {
            record("central_restore=no_selected_peripheral")
            return
        }

        let restoredAdvertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier(selectedIdentifier),
            localName: restoredPeripheral.name,
            advertisedServiceUuids: (restoredPeripheral.services ?? [])
                .compactMap { BluetoothUuid(coreBluetoothUuid: $0.uuid) }
        ).withVescNordicUartFallbackName()
        let discovery = rustSessionState.observeDiscovery(
            observation: DiscoveryObservation(restoredAdvertisement)
        )
        let selectedDiscovery = rustSessionState.selectDiscoveredPlatform(
            platformIdentifier: selectedIdentifier
        )
        let support = selectedDiscovery.pickerCandidates
            .first(where: { $0.platformIdentifier == selectedIdentifier })
            .map(DevicePickerCandidateSupport.init)
        let route = support?.connectionRoute
            ?? (restoredAdvertisement.advertisedServiceUuids.contains(.vescNordicUartService)
                ? .vescOnewheel
                : nil)
        guard let route else {
            record("central_restore=unsupported_selected_peripheral")
            return
        }
        if case .electricUnicycle = route, support?.electricUnicycleModel == nil {
            record("central_restore=missing_euc_model")
            return
        }

        discoveredPeripherals[restoredAdvertisement.peripheralIdentifier] = restoredPeripheral
        advertisement = restoredAdvertisement
        peripheral = restoredPeripheral
        selectedRoute = route
        selectedModel = support?.electricUnicycleModel
        isRecordOnly = false
        isProbeOnly = false
        suppressReconnect = false
        restoredPeripheral.delegate = self
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(
            name: restoredAdvertisement.localName.map { Data($0.utf8) }
        )
        record("central_restore=selected state=\(restoredPeripheral.state.rawValue) observations=\(discovery.observations.count)")

        switch restoredPeripheral.state {
        case .connected:
            prepareRestoredRide(route: route)
            if central?.state == .poweredOn {
                resumeConnectedPeripheral(restoredPeripheral)
            } else {
                setPhase(.discoveringServices)
            }
        case .connecting:
            prepareRestoredRide(route: route)
            setPhase(.discoveringServices)
        case .disconnected, .disconnecting:
            record("central_restore=selected_not_connected")
            peripheral = nil
            advertisement = nil
            selectedRoute = nil
            selectedModel = nil
        @unknown default:
            record("central_restore=unknown_peripheral_state")
        }
    }

    func prepareRestoredRide(route: DevicePickerConnectionRoute) {
        startCapture(reason: "restore", annotations: ["route=\(route)"])
        clearSettingsReadback()
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
    }

    func resumeConnectedPeripheral(_ peripheral: CBPeripheral) {
        assertOnBleQueue()
        guard liveOwner == nil else { return }
        ensureRideMapRecordingIfNeeded(platformIdentifier: peripheral.identifier.uuidString)
        setPhase(.discoveringServices)
        let services = peripheral.services ?? []
        guard !services.isEmpty else {
            peripheral.discoverServices(discoveryServiceUuidsForSelectedRoute)
            return
        }

        pendingServiceDiscoveries = Set(services.map(\.uuid))
        for service in services {
            guard let characteristics = service.characteristics, !characteristics.isEmpty else {
                peripheral.discoverCharacteristics(nil, for: service)
                continue
            }
            for characteristic in characteristics {
                if let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) {
                    subscribedCharacteristics[channel] = characteristic
                }
            }
            pendingServiceDiscoveries.remove(service.uuid)
        }
        if pendingServiceDiscoveries.isEmpty {
            buildOwner(for: peripheral)
        }
    }
}

extension CutoutSessionCore: CBCentralManagerDelegate {
    public func centralManager(
        _: CBCentralManager,
        willRestoreState dict: [String: Any]
    ) {
        assertOnBleQueue()
        let restoredPeripherals = dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral] ?? []
        record("central_restore=callback peripherals=\(restoredPeripherals.count)")
        restoreSelectedPeripheral(from: restoredPeripherals)
    }

    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        resolveBluetoothRestorationIfNeeded()
        handleCentralState(central.state) {
            central.scanForPeripherals(withServices: nil)
        }
    }

    private func resolveBluetoothRestorationIfNeeded() {
        assertOnBleQueue()
        guard didResolveBluetoothRestoration == false else { return }
        didResolveBluetoothRestoration = true
        let restoredPlatformIdentifier: String? = if
            let peripheral,
            selectedRoute != nil,
            peripheral.state == .connected || peripheral.state == .connecting
        {
            peripheral.identifier.uuidString
        } else {
            nil
        }
        publishOnMain {
            self.onBluetoothRestorationResolved?(restoredPlatformIdentifier)
        }
    }

    func handleCentralState(_ state: CBManagerState, startScan: () -> Void) {
        onBleQueue {
            handleCentralStateOnBleQueue(state, startScan: startScan)
        }
    }

    private func handleCentralStateOnBleQueue(_ state: CBManagerState, startScan: () -> Void) {
        assertOnBleQueue()
        record("central_state=\(state.rawValue)")
        guard state == .poweredOn else {
            cancelPendingReconnect()
            scanState = state == .unauthorized
                ? .permissionDenied
                : DevicePickerScanState(status: .bluetoothUnavailable, rows: [])
            publishScanState()
            setPhase(
                state == .unauthorized
                    ? .bluetoothPermissionDenied
                    : .bluetoothUnavailable(rawState: state.rawValue)
            )
            return
        }
        if let peripheral, selectedRoute != nil {
            switch peripheral.state {
            case .connected:
                resumeConnectedPeripheral(peripheral)
                record("central_state=restored_session")
                return
            case .connecting:
                record("central_state=restored_session")
                return
            case .disconnected, .disconnecting:
                break
            @unknown default:
                break
            }
        }
        scanState = DevicePickerScanState(
            status: .scanning,
            discoverySnapshot: rustSessionState.discoverySnapshot()
        )
        publishScanState()
        setPhase(.scanning)
        let services = CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids
        record("scan_supported_services=\(services.map(\.uuidString).joined(separator: ","))")
        startScan()
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi: NSNumber
    ) {
        assertOnBleQueue()
        let advertisement = CoreBluetoothAdvertisement(
            peripheral: peripheral,
            advertisementData: advertisementData
        )
        discoveredPeripherals[advertisement.peripheralIdentifier] = peripheral
        observeAdvertisement(advertisement)
        let advertisedServices = advertisement.advertisedServiceUuids.map(String.init(describing:)).joined(separator: ",")
        let candidate = [
            "candidate=\(advertisement.peripheralIdentifier.rawValue)",
            "name=\(advertisement.localName ?? "")",
            "model=\(advertisement.modelHint)",
            "services=\(advertisedServices)",
            "rssi=\(rssi)",
        ].joined(separator: " ")
        record(candidate)
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        assertOnBleQueue()
        hasObservedRideMapConnection = false
        ensureRideMapRecordingIfNeeded(platformIdentifier: peripheral.identifier.uuidString)
        setPhase(.discoveringServices)
        peripheral.delegate = self
        if isRecordOnly || isProbeOnly {
            _ = captureBuilder?.recordLinkUp(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                maxWriteLen: MobileTransportWriteLimitDto(bytes: UInt16(clamping: peripheral.maximumWriteValueLength(for: .withoutResponse)))
            )
        }
        peripheral.discoverServices(discoveryServiceUuidsForSelectedRoute)
    }

    private func ensureRideMapRecordingIfNeeded(platformIdentifier: String) {
        guard RideMapConnectionPolicy.shouldEnsureRecording(
            hasObservedConnection: hasObservedRideMapConnection,
            hasSelectedRoute: selectedRoute != nil
        ) else {
            return
        }
        hasObservedRideMapConnection = true
        guard ensureRideMapRecordingForConnection(platformIdentifier: platformIdentifier) else {
            hasObservedRideMapConnection = false
            return
        }
    }

    @discardableResult
    private func ensureRideMapRecordingForConnection(platformIdentifier: String) -> Bool {
        do {
            let previousRideID = rideMapState.currentSnapshot()?.rideId
            let snapshot = try rideMapState.ensureRecordingForVehicle(
                platformIdentifier: platformIdentifier,
                atMs: clock.now().rawValue
            )
            if snapshot.rideId != previousRideID {
                resetRideMapLocationAdmission()
            }
            publishOnMain { self.onRideMapSnapshotChange?(snapshot) }
            return true
        } catch {
            publishRideMapError(error)
            return false
        }
    }

    public func centralManager(_: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        assertOnBleQueue()
        record("connect_failed=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
        handleDisconnect(from: peripheral, error: error)
    }

    public func centralManager(
        _: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        assertOnBleQueue()
        handleDisconnect(from: peripheral, error: error)
    }
}

extension CutoutSessionCore: CBPeripheralDelegate {
    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        assertOnBleQueue()
        if let error {
            setPhase(.failed(.serviceDiscoveryFailed(error.sessionMessage)))
            return
        }
        let services = peripheral.services ?? []
        record("services=\(services.map { $0.uuid.uuidString }.joined(separator: ","))")
        pendingServiceDiscoveries = Set(services.map(\.uuid))
        services.forEach {
            peripheral.discoverCharacteristics(nil, for: $0)
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        assertOnBleQueue()
        if let error {
            setPhase(.failed(.characteristicDiscoveryFailed(error.sessionMessage)))
            return
        }
        service.characteristics?.forEach { characteristic in
            if let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) {
                subscribedCharacteristics[channel] = characteristic
            }
        }
        recordGattFingerprints(service: service)
        pendingServiceDiscoveries.remove(service.uuid)
        if isRecordOnly {
            subscribeRecordOnlyCharacteristics(service.characteristics ?? [], on: peripheral)
            if pendingServiceDiscoveries.isEmpty {
                setPhase(.live)
            }
            return
        }
        if isProbeOnly {
            if pendingServiceDiscoveries.isEmpty {
                setPhase(.subscribing)
                identificationProbeTransport.subscribe(using: self)
            }
            return
        }
        if pendingServiceDiscoveries.isEmpty {
            buildOwner(for: peripheral)
        }
    }

    public func peripheral(
        _: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        assertOnBleQueue()
        if let error {
            setPhase(.failed(.notificationFailed(error.sessionMessage)))
            return
        }
        guard
            let value = characteristic.value,
            let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid)
        else {
            return
        }
        let detectionResolution = observeDetectionNotification(channel: channel, bytes: value)
        if isProbeOnly {
            guard promoteProbeIfResolved(detectionResolution, on: characteristic.service?.peripheral) else {
                captureFrame(
                    direction: "notify",
                    characteristic: characteristic.uuid,
                    service: characteristic.service?.uuid,
                    bytes: value
                )
                captureNotificationCount += 1
                publishCaptureEvent(.progress(captureProgress()))
                return
            }
        }
        if isRecordOnly {
            captureFrame(
                direction: "notify",
                characteristic: characteristic.uuid,
                service: characteristic.service?.uuid,
                bytes: value
            )
            record("record_only_notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            captureNotificationCount += 1
            publishCaptureEvent(.progress(captureProgress()))
            return
        }
        guard let liveOwner else {
            return
        }
        do {
            let receivedAt = clock.now()
            let ingestStartedAt = receivedAt
            let step = try liveOwner.handleNotification(
                bytes: value,
                channel: channel,
                at: receivedAt
            )
            let ingestFinishedAt = clock.now()
            let ingestMilliseconds = ingestFinishedAt.rawValue >= ingestStartedAt.rawValue
                ? ingestFinishedAt.rawValue - ingestStartedAt.rawValue
                : 0
            captureFrame(
                direction: "notify",
                characteristic: characteristic.uuid,
                service: characteristic.service?.uuid,
                bytes: value,
                telemetry: step.actions.compactMap(\.rawTelemetry).last
            )
            record("notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            captureNotificationCount += 1
            publishCaptureEvent(.progress(captureProgress()))
            record("speed=\(step.snapshot?.speed.map { String($0.value) } ?? "nil")")
            record("voltage=\(step.snapshot?.voltage.map { String($0.value) } ?? "nil")")
            record("battery_estimated=\(step.snapshot?.batteryLevelEstimated.map { String($0.value) } ?? "nil")")
            record("live_records=\(liveOwner.records.count)")
            record("notification_ingest_ms=\(ingestMilliseconds)")
            record("rust_decode_ms=\(ingestMilliseconds)")
            applyNotificationStep(step, receivedAt: receivedAt)
        } catch {
            record("notification_ingest_error=\(error)")
            setPhase(.failed(.notificationIngestFailed(error.sessionMessage)))
        }
    }

    public func peripheral(
        _: CBPeripheral,
        didUpdateNotificationStateFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        assertOnBleQueue()
        guard let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) else {
            return
        }
        if let error {
            setPhase(.failed(.notificationFailed(error.sessionMessage)))
            return
        }
        if isProbeOnly, channel.bluetooth16Value == 0xffe1, characteristic.isNotifying {
            switch identificationProbeTransport.notificationsEnabled(at: clock.now(), using: self) {
            case .noProbeNeeded:
                if !promoteProbeIfResolved(
                    deviceDetectionSession.resolution,
                    on: characteristic.service?.peripheral
                ) {
                    refuseProbe(.unsupported, on: characteristic.service?.peripheral)
                }
            case .unsupported:
                refuseProbe(.unsupported, on: characteristic.service?.peripheral)
            case .writes:
                break
            case .alreadyPending:
                break
            }
            return
        }
        liveOwner?.handleNotificationStateUpdate(
            channel: channel,
            isNotifying: characteristic.isNotifying,
            error: error
        )
    }
}

private extension CutoutSessionCore {
    func assertOnBleQueue() {
        dispatchPrecondition(condition: .onQueue(bleQueue))
    }
}

extension CutoutSessionCore: CoreBluetoothOperationSink {
    public func subscribe(channel: BluetoothUuid) {
        guard let characteristic = subscribedCharacteristics[channel] else {
            setPhase(.failed(.missingNotifyChannel))
            return
        }
        peripheral?.setNotifyValue(true, for: characteristic)
    }

    public func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        observeDetectionProbeWrite(channel: channel, bytes: bytes)
        captureFrame(direction: "write_without_response", characteristic: channel.coreBluetoothUuid, bytes: bytes)
        guard let characteristic = subscribedCharacteristics[channel] else {
            setPhase(.failed(.missingWriteChannel))
            return
        }
        peripheral?.writeValue(bytes, for: characteristic, type: .withoutResponse)
        record("write_without_response=\(channel.coreBluetoothUuid.uuidString) bytes=\(bytes.count)")
    }

    public func disconnect() {
        guard let peripheral else {
            return
        }
        central?.cancelPeripheralConnection(peripheral)
    }

}

extension CutoutSessionCore {
    func recordGattFingerprints(service: CBService) {
        guard let serviceUuid = BluetoothUuid(coreBluetoothUuid: service.uuid) else {
            return
        }
        guard let builder = captureBuilder else {
            return
        }
        for characteristic in service.characteristics ?? [] {
            guard let characteristicUuid = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) else {
                continue
            }
            guard acceptCaptureWrite(builder.addGattFingerprint(fingerprint: MobileGattFingerprintDto(
                service: serviceUuid.bytes,
                characteristic: characteristicUuid.bytes,
                roles: characteristic.mobileGattRoles,
                verification: .hardwareVerified
            ))) else {
                return
            }
        }
    }

    func subscribeRecordOnlyCharacteristics(_ characteristics: [CBCharacteristic], on peripheral: CBPeripheral) {
        characteristics
            .filter { $0.properties.contains(.notify) || $0.properties.contains(.indicate) }
            .forEach { peripheral.setNotifyValue(true, for: $0) }
    }

    func observeDetectionProbeWrite(channel: BluetoothUuid, bytes: Data) {
        guard isReadOnlyBegodeProbeWrite(channel: channel, bytes: bytes) else {
            return
        }
        switch bytes.first {
        case UInt8(ascii: "N")?:
            _ = deviceDetectionSession.observeBegodeNameProbe(at: clock.now())
            scheduleBegodeProbeExpiry()
            annotateDetection("begode_probe_write=model")
        case UInt8(ascii: "V")?:
            _ = deviceDetectionSession.observeBegodeFirmwareProbe(at: clock.now())
            scheduleBegodeProbeExpiry()
            annotateDetection("begode_probe_write=firmware")
        case UInt8(ascii: "M")?:
            _ = deviceDetectionSession.observeBegodeImuProbe(at: clock.now())
            scheduleBegodeProbeExpiry()
            annotateDetection("begode_probe_write=imu")
        default:
            break
        }
    }

    func isReadOnlyBegodeProbeWrite(channel: BluetoothUuid, bytes: Data) -> Bool {
        guard channel.bluetooth16Value == 0xffe1, bytes.count == 1 else {
            return false
        }
        return switch bytes.first {
        case UInt8(ascii: "N")?, UInt8(ascii: "V")?, UInt8(ascii: "M")?:
            true
        default:
            false
        }
    }

    @discardableResult
    func observeDetectionNotification(channel: BluetoothUuid, bytes: Data) -> DeviceDetectionResolution {
        let previous = deviceDetectionSession.resolution
        let current = identificationProbeTransport.observeNotification(channel: channel, bytes: bytes)
        guard channel.bluetooth16Value == 0xffe1 else {
            return current
        }
        scheduleBegodeProbeExpiry()
        if current.modelBanner != nil, current.modelBanner != previous.modelBanner {
            annotateDetection("begode_probe_response=model")
        }
        if current.firmwareBanner != nil, current.firmwareBanner != previous.firmwareBanner {
            annotateDetection("begode_probe_response=firmware")
        }
        if current.imuBanner != nil, current.imuBanner != previous.imuBanner {
            annotateDetection("begode_probe_response=imu")
        }
        publishDetectionIdentityCandidate(current)
        guard current.malformedProbeResponse != previous.malformedProbeResponse else {
            return current
        }
        guard let malformedProbeResponse = current.malformedProbeResponse else {
            return current
        }
        switch malformedProbeResponse {
        case .begodeName:
            annotateDetection("begode_probe_malformed=model")
        case .begodeFirmware:
            annotateDetection("begode_probe_malformed=firmware")
        case .begodeImu:
            annotateDetection("begode_probe_malformed=imu")
        }
        return current
    }

    func expireOutstandingBegodeProbeResponses() {
        onBleQueue {
            let expired = deviceDetectionSession.expireBegodeProbeResponses(
                at: clock.now(),
                timeout: BegodeProbeResponsePolicy.timeoutAfter
            )
            publishMissingBegodeProbeResponses(expired)
            if let peripheral {
                _ = promoteProbeIfResolved(deviceDetectionSession.resolution, on: peripheral)
            }
            scheduleBegodeProbeExpiry()
        }
    }

    func markOutstandingBegodeProbeResponsesMissing() {
        let missing = deviceDetectionSession.markBegodeProbeResponsesMissing()
        publishMissingBegodeProbeResponses(missing)
        clearPendingBegodeProbeResponses()
    }

    private func publishMissingBegodeProbeResponses(_ probes: [DeviceDetectionPendingProbe]) {
        for (probe, label) in [
            (DeviceDetectionPendingProbe.begodeName, "model"),
            (.begodeFirmware, "firmware"),
            (.begodeImu, "imu"),
        ] where probes.contains(probe) {
            annotateDetection("begode_probe_missing=\(label)")
        }
        guard !probes.isEmpty else {
            return
        }
        publishDetectionIdentityCandidate(deviceDetectionSession.resolution)
    }

    private func clearPendingBegodeProbeResponses() {
        begodeProbeExpiryWorkItem?.cancel()
        begodeProbeExpiryWorkItem = nil
    }

    private func promoteProbeIfResolved(
        _ resolution: DeviceDetectionResolution,
        on peripheral: CBPeripheral?
    ) -> Bool {
        guard isProbeOnly, let peripheral, let advertisement else {
            return false
        }
        switch resolution.probeDisposition(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            displayName: advertisement.localName
                ?? protocolIdentityFallbackDisplayName(protocolFamily: resolution.protocolFamily)
        ) {
        case .pending:
            return false
        case .promote(let model):
            isProbeOnly = false
            selectedRoute = .electricUnicycle
            selectedModel = model
            annotateDetection("identification_probe_resolved=\(model.displayName)")
            buildOwner(for: peripheral)
            return liveOwner != nil
        case .refuse(let failure):
            refuseProbe(failure, on: peripheral)
            return false
        }
    }

    private func refuseProbe(_ failure: IdentificationProbeFailure, on peripheral: CBPeripheral?) {
        annotateDetection("identification_probe_refused=\(failure)")
        suppressReconnect = true
        setPhase(.failed(.identificationFailed(failure)))
        if let peripheral {
            central?.cancelPeripheralConnection(peripheral)
        }
    }

    private func scheduleBegodeProbeExpiry() {
        begodeProbeExpiryWorkItem?.cancel()
        guard let deadline = deviceDetectionSession.nextBegodeProbeExpiry(
            timeout: BegodeProbeResponsePolicy.timeoutAfter
        ) else {
            begodeProbeExpiryWorkItem = nil
            return
        }

        let now = clock.now().rawValue
        let delay = deadline.rawValue > now ? deadline.rawValue - now : 0
        let work = DispatchWorkItem { [weak self] in
            self?.expireOutstandingBegodeProbeResponses()
        }
        begodeProbeExpiryWorkItem = work
        bleQueue.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: delay)),
            execute: work
        )
    }

    func annotateDetection(_ annotation: String) {
        if let builder = captureBuilder {
            guard acceptCaptureWrite(builder.addAnnotation(annotation: annotation)) else {
                return
            }
        }
        record(annotation)
    }
}

extension CutoutSessionCore: CLLocationManagerDelegate {
    private func updateRideMapAvailability(_ status: CLAuthorizationStatus) {
        guard rideMapStorageError == nil else {
            guard rideMapAvailability != .storageUnavailable else { return }
            rideMapAvailability = .storageUnavailable
            publishOnMain { self.onRideMapAvailabilityChange?(.storageUnavailable) }
            return
        }
        let availability: MobileRideMapAvailability
        switch status {
        case .notDetermined:
            availability = .permissionRequired
        case .authorizedAlways, .authorizedWhenInUse:
            availability = .ready
        case .denied:
            availability = .denied
        case .restricted:
            availability = .restricted
        @unknown default:
            availability = .checking
        }
        guard availability != rideMapAvailability else { return }
        rideMapAvailability = availability
        publishOnMain { self.onRideMapAvailabilityChange?(availability) }
    }

    private func startLocationUpdatesIfAuthorized(_ manager: CLLocationManager) {
        switch manager.authorizationStatus {
        case .authorizedAlways:
            manager.startUpdatingLocation()
        case .authorizedWhenInUse:
            requestAlwaysLocationAuthorizationIfNeeded()
            manager.startUpdatingLocation()
        case .notDetermined, .denied, .restricted:
            break
        @unknown default:
            break
        }
    }

    private func requestAlwaysLocationAuthorizationIfNeeded() {
        guard !didRequestAlwaysLocationAuthorization else { return }
        didRequestAlwaysLocationAuthorization = true
        locationManager.requestAlwaysAuthorization()
    }

    public func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        updateRideMapAvailability(manager.authorizationStatus)
        switch manager.authorizationStatus {
        case .notDetermined:
            manager.requestWhenInUseAuthorization()
        case .authorizedAlways:
            startLocationUpdatesIfAuthorized(manager)
        case .authorizedWhenInUse:
            startLocationUpdatesIfAuthorized(manager)
        case .denied, .restricted:
            break
        @unknown default:
            break
        }
    }

    public func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard locations.isEmpty == false else { return }
        let callbackMonotonicMs = clock.now().rawValue
        let monotonicMilliseconds = monotonicMillisecondsForLocationBatch(
            timestamps: locations.map(\.timestamp),
            callbackMonotonicMs: MonotonicMilliseconds(callbackMonotonicMs),
            callbackWallClock: wallClock.now(),
            lastAcceptedTimestamp: locationTimestampAdmission.lastAcceptedTimestamp
        )
        for (location, monotonicMs) in zip(locations, monotonicMilliseconds) {
            guard let monotonicMs else { continue }
            guard let sample = MobilePhoneLocationSampleDto(location: location) else { continue }
            // Current-location readback is independent of ride recording. The map
            // admission result below only controls the canonical capture context.
            phoneLocationSnapshot = phoneLocationState.ingest(sample: sample)
            do {
                let decision = try rideMapState.ingestLocation(
                    monotonicMs: monotonicMs.rawValue,
                    sample: sample
                )
                handleRideMapLocationDecision(decision, sample: sample)
                locationTimestampAdmission.record(location.timestamp, decision: decision)
            } catch {
                publishRideMapError(error)
            }
        }
        publishPhoneLocationSnapshot()
    }

    /// Applies a location decision to the PEVCAP context and publishes it. Pending decisions are
    /// deliberately not capture-admitted; the sample is retained until Rust reports durable
    /// acceptance from `pollLocationWrites()`.
    private func handleRideMapLocationDecision(
        _ decision: MobileRideMapDecisionDto,
        sample: MobilePhoneLocationSampleDto? = nil
    ) {
        switch decision {
        case let .pending(point, _):
            guard let sample else { return }
            rememberPendingPhoneLocation(sample, sequence: point.sequence)
            scheduleRideMapWritePoll()
        case .accepted:
            let admittedSample = sample ?? takePendingPhoneLocation(for: decision)
            if let admittedSample {
                _ = capturePhoneLocationSample(
                    sample: admittedSample,
                    decision: decision,
                    state: admittedPhoneLocationState
                )
            }
        case .rejected, .ignored, .storageError:
            // A terminal outcome from the poll is FIFO with the pending write. Direct
            // rejection/error outcomes carry the current sample and therefore have no pending
            // entry to remove.
            if sample == nil {
                _ = takePendingPhoneLocation(for: decision)
            }
        }
        publishRideMapDecision(decision)
    }

    private func rememberPendingPhoneLocation(
        _ sample: MobilePhoneLocationSampleDto,
        sequence: UInt64
    ) {
        pendingPhoneLocationLock.lock()
        defer { pendingPhoneLocationLock.unlock() }
        pendingPhoneLocations.append(sample, sequence: sequence)
    }

    private func takePendingPhoneLocation(
        for decision: MobileRideMapDecisionDto
    ) -> MobilePhoneLocationSampleDto? {
        pendingPhoneLocationLock.lock()
        defer { pendingPhoneLocationLock.unlock() }
        return pendingPhoneLocations.take(for: decision)
    }

    private func scheduleRideMapWritePoll() {
        pendingPhoneLocationLock.lock()
        guard !rideMapPollScheduled else {
            pendingPhoneLocationLock.unlock()
            return
        }
        rideMapPollScheduled = true
        pendingPhoneLocationLock.unlock()

        let work = { [weak self] in
            guard let self else { return }
            let decisions = self.rideMapState.pollLocationWrites()
            decisions.forEach { self.handleRideMapLocationDecision($0) }

            self.pendingPhoneLocationLock.lock()
            self.rideMapPollScheduled = false
            let shouldPollAgain = !self.pendingPhoneLocations.isEmpty
            self.pendingPhoneLocationLock.unlock()
            if shouldPollAgain {
                self.scheduleRideMapWritePoll()
            }
        }
        rideMapPollScheduler.schedule(after: 50, operation: work)
    }
}

func monotonicMillisecondsForLocationBatch(
    timestamps: [Date],
    callbackMonotonicMs: MonotonicMilliseconds,
    callbackWallClock: Date = Date(),
    lastAcceptedTimestamp: Date? = nil
) -> [MonotonicMilliseconds?] {
    guard callbackWallClock.timeIntervalSinceReferenceDate.isFinite else {
        return Array(repeating: nil, count: timestamps.count)
    }

    var previousTimestamp = lastAcceptedTimestamp
    var accepted = [(index: Int, timestamp: Date)]()
    for (index, timestamp) in timestamps.enumerated() {
        guard timestamp.timeIntervalSinceReferenceDate.isFinite,
              timestamp <= callbackWallClock,
              previousTimestamp.map({ timestamp > $0 }) ?? true
        else {
            continue
        }
        accepted.append((index, timestamp))
        previousTimestamp = timestamp
    }

    guard let newestTimestamp = accepted.last?.timestamp else {
        return Array(repeating: nil, count: timestamps.count)
    }

    var result = Array<MonotonicMilliseconds?>(repeating: nil, count: timestamps.count)
    var previousMonotonic: UInt64?
    for entry in accepted {
        let elapsedMilliseconds = newestTimestamp.timeIntervalSince(entry.timestamp) * 1_000
        guard elapsedMilliseconds.isFinite, elapsedMilliseconds >= 0,
              elapsedMilliseconds < Double(UInt64.max)
        else {
            continue
        }
        let offsetMilliseconds = UInt64(elapsedMilliseconds.rounded(.up))
        let callbackMilliseconds = callbackMonotonicMs.rawValue
        guard callbackMilliseconds >= offsetMilliseconds else { continue }
        let monotonic = callbackMilliseconds - offsetMilliseconds
        guard previousMonotonic.map({ monotonic > $0 }) ?? true else { continue }
        result[entry.index] = MonotonicMilliseconds(monotonic)
        previousMonotonic = monotonic
    }
    return result
}

/// Returns a phone sample only when the map accepted that exact sample into the ride.
///
/// Keeping this decision at the Core Location boundary prevents PEVCAP capture context
/// from observing a raw update that the canonical map rejected or ignored.
func capturePhoneLocationSample(
    sample: MobilePhoneLocationSampleDto,
    decision: MobileRideMapDecisionDto,
    state: MobilePhoneLocationState? = nil
) -> MobilePhoneLocationSampleDto? {
    guard case .accepted = decision else { return nil }
    _ = state?.ingest(sample: sample)
    return sample
}

private extension MobilePhoneLocationSampleDto {
    init?(location: CLLocation) {
        guard let wallClockUnixMs = wallClockUnixMilliseconds(for: location.timestamp) else {
            return nil
        }
        self.init(
            wallClockUnixMs: wallClockUnixMs,
            latitudeDegrees: location.coordinate.latitude,
            longitudeDegrees: location.coordinate.longitude,
            altitudeMeters: location.altitude,
            horizontalAccuracyMeters: nonNegativeFinite(location.horizontalAccuracy),
            verticalAccuracyMeters: nonNegativeFinite(location.verticalAccuracy),
            speedMetersPerSecond: nonNegativeFinite(location.speed),
            speedAccuracyMetersPerSecond: nonNegativeFinite(location.speedAccuracy),
            courseDegrees: validCourse(location.course),
            courseAccuracyDegrees: nonNegativeFinite(location.courseAccuracy)
        )
    }
}

func wallClockUnixMilliseconds(for timestamp: Date) -> UInt64? {
    let seconds = timestamp.timeIntervalSince1970
    guard seconds.isFinite, seconds >= 0 else { return nil }

    let milliseconds = seconds * 1_000
    guard milliseconds.isFinite, milliseconds >= 0,
          milliseconds < Double(Int64.max)
    else {
        return nil
    }
    return UInt64(milliseconds.rounded(.down))
}

private func nonNegativeFinite(_ value: Double) -> Double? {
    guard value.isFinite, value >= 0 else { return nil }
    return value
}

private func validCourse(_ value: Double) -> Double? {
    guard value.isFinite, (0 ..< 360).contains(value) else { return nil }
    return value
}

private extension CBCharacteristic {
    var mobileGattRoles: [MobileGattRoleDto] {
        var roles: [MobileGattRoleDto] = []
        if properties.contains(.read) {
            roles.append(.read)
        }
        if properties.contains(.write) {
            roles.append(.write)
        }
        if properties.contains(.writeWithoutResponse) {
            roles.append(.writeWithoutResponse)
        }
        if properties.contains(.notify) {
            roles.append(.notify)
        }
        if properties.contains(.indicate) {
            roles.append(.indicate)
        }
        return roles
    }
}

struct MonotonicClock {
    private let source: () -> MonotonicMilliseconds

    init(now: @escaping () -> MonotonicMilliseconds = {
        MonotonicMilliseconds(UInt64(ProcessInfo.processInfo.systemUptime * 1_000))
    }) {
        source = now
    }

    func now() -> MonotonicMilliseconds {
        source()
    }
}

struct WallClock {
    private let source: () -> Date

    init(now: @escaping () -> Date = { Date() }) {
        source = now
    }

    func now() -> Date {
        source()
    }
}

private extension Optional where Wrapped == Error {
    var sessionMessage: String {
        map(String.init(describing:)) ?? "unknown error"
    }
}

private extension Error {
    var sessionMessage: String {
        String(describing: self)
    }
}

private extension Data {
    var hexString: String {
        map { String(format: "%02x", $0) }.joined()
    }
}
