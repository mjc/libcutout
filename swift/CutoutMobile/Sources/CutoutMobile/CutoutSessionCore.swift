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

enum ProtocolDetectionFinishDecision: Equatable {
    case awaitPassiveEvidence
    case evaluateResolvedEvidence

    init(resolution: DeviceDetectionResolution) {
        self = resolution.protocolFamily == nil ? .awaitPassiveEvidence : .evaluateResolvedEvidence
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

public struct CaptureGeneration: Comparable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }

    public static let legacy = Self(rawValue: 0)

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

public enum CaptureEvent: Equatable, Sendable {
    case started(generation: CaptureGeneration, fileURL: URL)
    case notificationRecorded(generation: CaptureGeneration)
    case progress(generation: CaptureGeneration, CaptureProgress)
    case finished(generation: CaptureGeneration, fileURL: URL)
    case failed(generation: CaptureGeneration)

    // Keep hand-authored fixtures source-compatible while production events carry identity.
    public static func started(fileURL: URL) -> Self {
        .started(generation: .legacy, fileURL: fileURL)
    }

    public static var notificationRecorded: Self {
        .notificationRecorded(generation: .legacy)
    }

    public static func progress(_ progress: CaptureProgress) -> Self {
        .progress(generation: .legacy, progress)
    }

    public static func finished(fileURL: URL) -> Self {
        .finished(generation: .legacy, fileURL: fileURL)
    }

    public static var failed: Self {
        .failed(generation: .legacy)
    }
}

struct CaptureMusicContext: Equatable {
    private(set) var current: MobilePevcapMusicEventDto?

    mutating func update(_ observation: MobilePevcapMusicEventDto?) {
        current = observation
    }

    mutating func take() -> MobilePevcapMusicEventDto? {
        defer { current = nil }
        return current
    }

    mutating func reset() {
        current = nil
    }
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

struct ProtocolDetectionResponsePolicy {
    static let timeoutAfter = MonotonicMilliseconds(2_000)
}

struct IdentificationProbeTransportCoordinator {
    let detectionSession: DeviceDetectionSession

    func subscribe(using sink: any CoreBluetoothOperationSink) {
        sink.subscribe(channel: .bluetooth16(0xffe1))
    }

    func subscribeVesc(using sink: any CoreBluetoothOperationSink) {
        sink.subscribe(channel: .vescNordicUartNotify)
    }

    @discardableResult
    func notificationsEnabled(
        at now: MonotonicMilliseconds,
        using sink: any CoreBluetoothOperationSink
    ) -> IdentificationProbeOutcome {
        let outcome = detectionSession.beginIdentificationProbe(at: now)
        if case .writes(let writes) = outcome {
            writes.forEach { _ = sink.writeWithoutResponse(channel: $0.channel, bytes: $0.bytes) }
        }
        return outcome
    }

    func observeNotification(channel: BluetoothUuid, bytes: Data) -> DeviceDetectionResolution {
        return detectionSession.observeNotification(bytes: bytes)
    }

    func vescNotificationsEnabled(
        at now: MonotonicMilliseconds,
        using sink: any CoreBluetoothOperationSink
    ) {
        let session = VescOnewheelSession()
        guard let linkActions = try? session.linkUp(
            at: now,
            writeLimit: TransportWriteLimitBytes(512)
        ) else {
            return
        }
        linkActions
            .filter { $0.kind == .write }
            .forEach { action in
                guard let channel = BluetoothUuid(action.channel) else { return }
                _ = sink.writeWithoutResponse(channel: channel, bytes: action.bytes)
            }
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

#if DEBUG
public enum CutoutSessionTestInitialBluetoothState: Sendable {
    case scanning
    case unavailable
    case permissionDenied
}

public struct CutoutSessionTestScript {
    public let candidate: DevicePickerDiscoveryCandidate
    public let protocolNotifications: [Data]
    public let protocolNotificationIntervalMilliseconds: UInt64?
    public let telemetry: TelemetrySnapshot?
    public let telemetryUpdate: TelemetrySnapshot?
    public let telemetryUpdateDelayMilliseconds: UInt64
    public let bmsSnapshot: BmsSnapshot?
    public let startsLive: Bool
    public let initialBluetoothState: CutoutSessionTestInitialBluetoothState
    public let failsConnection: Bool
    public let identificationProbeFailure: IdentificationProbeFailure?
    public let detectedSupport: DevicePickerCandidateSupport?
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
        protocolNotifications: [Data] = [],
        protocolNotificationIntervalMilliseconds: UInt64? = nil,
        telemetryUpdate: TelemetrySnapshot? = nil,
        telemetryUpdateDelayMilliseconds: UInt64 = 0,
        bmsSnapshot: BmsSnapshot? = nil,
        startsLive: Bool = false,
        initialBluetoothState: CutoutSessionTestInitialBluetoothState = .scanning,
        failsConnection: Bool = false,
        identificationProbeFailure: IdentificationProbeFailure? = nil,
        detectedSupport: DevicePickerCandidateSupport? = nil,
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
        self.protocolNotifications = protocolNotifications
        self.protocolNotificationIntervalMilliseconds = protocolNotificationIntervalMilliseconds
        self.telemetry = telemetry
        self.telemetryUpdate = telemetryUpdate
        self.telemetryUpdateDelayMilliseconds = telemetryUpdateDelayMilliseconds
        self.bmsSnapshot = bmsSnapshot
        self.startsLive = startsLive
        self.initialBluetoothState = initialBluetoothState
        self.failsConnection = failsConnection
        self.identificationProbeFailure = identificationProbeFailure
        self.detectedSupport = detectedSupport
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

/// Deterministic transport for exercising the Rust-backed settings path without BLE.
private final class CutoutSessionTestOperationSink: CoreBluetoothOperationSink {
    private(set) var writes: [(BluetoothUuid, Data)] = []

    func subscribe(channel _: BluetoothUuid) {}

    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data, onReceipt: @escaping (CoreBluetoothWriteDisposition) -> Void) -> CoreBluetoothWriteDisposition {
        writes.append((channel, bytes))
        onReceipt(.submitted)
        return .submitted
    }

    func disconnect() {}
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

private final class WeakCutoutSessionCoreReference: @unchecked Sendable {
    weak var value: CutoutSessionCore?

    init(_ value: CutoutSessionCore) {
        self.value = value
    }
}

public final class CutoutSessionCore: NSObject {
    public var rideSessionStateHandle: CutoutSessionStateHandle { rustSessionState }
    public var connectionSnapshot: ConnectionSnapshot { rustSessionState.connectionAttemptSnapshot() }
    public var rideMapStateHandle: MobileRideMapState? { rideMapState }
    public private(set) var displayState = RideDisplayState()
    public private(set) var phase = SessionConnectionPhase.starting
    public var records: [String] { diagnosticLog.values }
    public var droppedRecordCount: Int { diagnosticLog.droppedCount }
    public private(set) var hasObservedSpeedSnapshot = false
    public private(set) var scanState = DevicePickerScanState(status: .idle, rows: [])
    public private(set) var faultHistoryReadback: FaultHistoryReadback?
    public private(set) var bmsSnapshot: BmsSnapshot?
    public private(set) var phoneLocationSnapshot = MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    private var storedProtocolIdentityCandidate: DevicePickerDiscoveryCandidate?
    public var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? {
        onBleQueue { storedProtocolIdentityCandidate }
    }
    public var isRecordOnlyConnection: Bool {
        onBleQueue { isRecordOnly }
    }
    public var electricUnicycleModel: ElectricUnicycleModel? {
        onBleQueue { selectedModel }
    }
    public var deviceControlsSnapshot: DeviceControlsSnapshot {
        rustSessionState.deviceControlsSnapshot()
    }

#if DEBUG
    var musicCaptureObservationForTesting: MobilePevcapMusicEventDto? {
        onBleQueue { musicCaptureContext.current }
    }
#endif

    public var onDisplayStateChange: ((RideDisplayState) -> Void)?
    public var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    public var onConnectionSnapshotChange: ((ConnectionSnapshot) -> Void)?
    public var onReconnectScheduled: ((SessionConnectionRetry) -> Void)?
    public var onRecord: ((String) -> Void)?
    public var onCaptureEvent: ((CaptureEvent) -> Void)?
    public var onScanStateChange: ((DevicePickerScanState) -> Void)?
    public var onDeviceControlsChange: ((DeviceControlsSnapshot) -> Void)?
    public var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    public var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    public var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)?
    public var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)?
    public var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)?
    public var onRideMapErrorChange: ((MobileRideMapErrorEvent) -> Void)?
    public var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)?
    public var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
    public var onBluetoothRestorationResolved: ((String?) -> Void)?


    private let clock: MonotonicClock
    private let wallClock: () -> Date
    private var diagnosticLog = BoundedDiagnosticLog(capacity: 2_048)
    private let bleQueue = DispatchQueue(label: "io.cutout.corebluetooth")
    private let bleQueueKey = DispatchSpecificKey<Void>()
    private let rideMapQueue = DispatchQueue(label: "io.cutout.ridemap", qos: .utility)
    private let rideMapQueueKey = DispatchSpecificKey<Void>()
    private let rustSessionState: CutoutSessionStateHandle
    private let selectedDeviceStore: DevicePickerSelectionStore
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var connectionAttempt: CoreBluetoothConnectionAttempt?
    private var retiringPeripheralIdentifiers: Set<UUID> = []
    private var connectionDeadlineWorkItem: DispatchWorkItem?
    private var advertisement: CoreBluetoothAdvertisement?
    private var discoveredPeripherals: [CoreBluetoothPeripheralIdentifier: CBPeripheral] = [:]
    private var liveOwner: DeviceSessionTransport?
    private var selectedModel: ElectricUnicycleModel?
    private var selectedRoute: DevicePickerConnectionRoute?
    private var chargeEstimateProfile: ChargeEstimateProfile?
    private var vescBoardProfile: VescBoardProfile?
    private var isRecordOnly = false
    private var isDetectingProtocol = false
    private var subscribedCharacteristics: [BluetoothUuid: CBCharacteristic] = [:]
    private let pendingWithoutResponseWrites = CoreBluetoothWriteQueue(capacity: 64)
    private var pendingServiceDiscoveries = Set<CBUUID>()
    private var connectionGattInventory: [MobileGattFingerprintDto] = []
    private var suppressReconnect = false
    private let reconnectController: ConnectionReconnectController
    private let reconnectJitter: () -> Double
    private var captureStartedAt: MonotonicMilliseconds?
    private var captureNotificationCount: UInt64 = 0
    private var captureBuilder: MobilePevcapCaptureBuilder?
    private var nextCaptureGeneration: UInt64 = 0
    private var captureGeneration: CaptureGeneration?
    private var musicCaptureContext = CaptureMusicContext()
    private var captureMusicHistoryPolicy = MobileMusicHistoryPolicyDto.disabled
    private var captureFileURL: URL?
    private var bmsStorageSessionIdentifier = UUID().uuidString
    private let deviceDetectionSession: DeviceDetectionSession
    private let identificationProbeTransport: IdentificationProbeTransportCoordinator
    private var begodeProbeExpiryWorkItem: DispatchWorkItem?
    private var protocolDetectionExpiryWorkItem: DispatchWorkItem?
    private var pendingDisplayState: RideDisplayState?
    private var pendingDisplayStateQueuedAt: MonotonicMilliseconds?
    private var displayPublishWorkItem: DispatchWorkItem?
    private var lastDisplayPublication: MonotonicMilliseconds?
    private var lastPublishedWarningSeverity: EucRideWarningSeverity?
    private let rideMapState: MobileRideMapState?
    private let phoneLocationState = MobilePhoneLocationState()
    private var latestRideMapSnapshot: MobileRideMapSnapshotDto?
    private var rideMapWritePoller: DispatchSourceTimer?
    private var rideMapRestorationStarted = false
    private var didRequestWhenInUseLocationAuthorization = false
    private var locationUpdatesDemanded = false
    private var locationManagerUpdatesStarted = false
    private var didResolveBluetoothRestoration = false
#if DEBUG
    private let testScript: CutoutSessionTestScript?
    var captureFinishWriterGate: (() -> Void)?
    private var testOperationSink: CutoutSessionTestOperationSink?
    private var testScriptWorkItem: DispatchWorkItem?
    private var testScriptUpdateWorkItem: DispatchWorkItem?
    private var testProtocolNotificationTimer: DispatchSourceTimer?
    private var testScriptDidReconnect = false
#endif

    deinit {
#if DEBUG
        testProtocolNotificationTimer?.cancel()
#endif
        connectionDeadlineWorkItem?.cancel()
        protocolDetectionExpiryWorkItem?.cancel()
        rideMapWritePoller?.cancel()
    }

    private lazy var locationManager: CLLocationManager = {
        let manager = CLLocationManager()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBestForNavigation
        manager.activityType = .fitness
        return manager
    }()

    /// Composes transport around a database handle opened off the main actor.
    public convenience init(rideMapState: MobileRideMapState) {
        self.init(clock: MonotonicClock(), rideMapState: rideMapState)
    }

    public override convenience init() {
        let rideMapState = RustPersistenceStore.shared.map(MobileRideMapState.init(database:))
            ?? MobileRideMapState(storageUnavailable: "Rust ride database is unavailable")
        self.init(clock: MonotonicClock(), rideMapState: rideMapState)
    }

#if DEBUG
    public convenience init(
        testScript: CutoutSessionTestScript,
        rideMapState: MobileRideMapState? = nil,
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore()
    ) {
        self.init(
            clock: MonotonicClock(),
            testScript: testScript,
            selectedDeviceStore: selectedDeviceStore,
            rideMapState: rideMapState
        )
    }

    init(
        clock: MonotonicClock,
        testScript: CutoutSessionTestScript? = nil,
        reconnectScheduler: any ConnectionReconnectScheduling = MainQueueReconnectScheduler(),
        reconnectJitter: @escaping () -> Double = { Double.random(in: 0...1) },
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        wallClock: @escaping () -> Date = { Date() },
        rideMapState: MobileRideMapState? = nil
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
        self.rideMapState = rideMapState
        self.testScript = testScript
        self.reconnectController = ConnectionReconnectController(scheduler: reconnectScheduler)
        self.reconnectJitter = reconnectJitter
        self.selectedDeviceStore = selectedDeviceStore
        super.init()
        bleQueue.setSpecific(key: bleQueueKey, value: ())
        rideMapQueue.setSpecific(key: rideMapQueueKey, value: ())
    }
#else
    init(
        clock: MonotonicClock,
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        wallClock: @escaping () -> Date = { Date() },
        rideMapState: MobileRideMapState? = nil
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
        self.rideMapState = rideMapState
        self.reconnectController = ConnectionReconnectController(scheduler: MainQueueReconnectScheduler())
        self.reconnectJitter = { Double.random(in: 0...1) }
        self.selectedDeviceStore = selectedDeviceStore
        super.init()
        bleQueue.setSpecific(key: bleQueueKey, value: ())
        rideMapQueue.setSpecific(key: rideMapQueueKey, value: ())
    }
#endif

    public func start() {
        startRideMapWritePolling()
        publishRideMapAvailability()
        startRideMapRestoration()
#if DEBUG
        if let testScript {
            publishOnMain { self.onBluetoothRestorationResolved?(nil) }
            start(testScript: testScript)
            return
        }
#endif
        _ = locationManager
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
            guard let peripheral = discoveredPeripherals[identifier], let advertisement else {
                return false
            }
            connectForProtocolDetection(to: peripheral, using: advertisement)
            return true
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
            guard let peripheral = discoveredPeripherals[identifier],
                  let advertisement = snapshot.advertisement(platformIdentifier: platformIdentifier)
            else { return false }
            connectForProtocolDetection(to: peripheral, using: advertisement)
            return true
        }
    }

    @discardableResult
    public func probe(platformIdentifier: String) -> Bool {
#if DEBUG
        if let testScript {
            return onBleQueue {
                pair(testScript: testScript, platformIdentifier: platformIdentifier)
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
            connectForProtocolDetection(to: peripheral, using: advertisement)
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
                    guard startCapture(
                        reason: note ?? "record-only",
                        annotations: annotations,
                        evidence: "simulator_fixture"
                    ) else {
                        isRecordOnly = false
                        return false
                    }
                    guard captureBuilder != nil else {
                        isRecordOnly = false
                        return false
                    }
                    publishCaptureProgress()
                    return true
                }
            }
            let fileURL = URL(fileURLWithPath: "/tmp/ui-test.capture")
            let generation = beginCaptureGeneration()
            captureFileURL = fileURL
            isRecordOnly = true
            publishCaptureEvent(.started(generation: generation, fileURL: fileURL))
            publishCaptureEvent(.progress(generation: generation, CaptureProgress(
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

    public func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue) throws {
        try onBleQueue {
            Result {
                guard let owner = liveOwner, owner.token == token else {
                    throw DeviceSettingSubmissionError.ConnectionUnavailable
                }
                _ = try owner.submitSetting(id, value: value, at: clock.now())
            }
        }.get()
    }

    public func submitDeviceAction(token: ConnectionAttemptToken, id: DeviceActionID) throws {
        try onBleQueue {
            Result {
                guard let owner = liveOwner, owner.token == token else {
                    throw DeviceActionSubmissionError.ConnectionUnavailable
                }
                _ = try owner.submitAction(id, at: clock.now())
            }
        }.get()
    }
    /// Attempts the generic trip-meter reset action for a new ride when the
    /// selected device profile exposes that capability.
    ///
    /// The wheel has no completion readback for this action, so this reports only
    /// whether the command was accepted by the live transport.
    @discardableResult
    public func resetTripMeterForNewRide() -> Bool {
        onBleQueue {
            guard let owner = liveOwner else { return false }
            do {
                _ = try owner.submitAction(.resetTripMeter, at: clock.now())
                record("trip_meter_reset_on_new_ride=submitted")
                return true
            } catch {
                record("trip_meter_reset_on_new_ride=failed")
                return false
            }
        }
    }

    public func setDeviceControlsValidation(token: ConnectionAttemptToken, authorized: Bool) throws {
        try onBleQueue {
            Result {
                guard let owner = liveOwner, owner.token == token else {
                    throw DeviceSettingSubmissionError.ConnectionUnavailable
                }
                try owner.setValidationAuthorization(authorized, at: clock.now())
            }
        }.get()
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
            testOperationSink = nil
            liveOwner = nil
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
        case .supported(let supportedRoute, let supportedModel):
            guard let supportedRoute else { return false }
            route = supportedRoute
            candidateModel = supportedModel
        case .probeRecommended:
            guard case let .supported(detectedRoute, detectedModel) = testScript.detectedSupport,
                  let detectedRoute
            else { return false }
            route = detectedRoute
            candidateModel = detectedModel
        default:
            return false
        }
        let selectedModel = model ?? candidateModel
        if route == .electricUnicycle, selectedModel == nil {
            return false
        }

        self.selectedRoute = route
        self.selectedModel = selectedModel
        testScriptWorkItem?.cancel()
        testScriptUpdateWorkItem?.cancel()
        liveOwner?.invalidate()
        liveOwner = nil
        guard let token = rustSessionState.beginConnectionAttempt(
            platformIdentifier: platformIdentifier, nowMs: clock.now().rawValue
        ).token else { return false }
        if let vescBoardProfile {
            _ = rustSessionState.configureConnectionVescProfile(token: token, profile: vescBoardProfile)
        }
        setPhase(.discoveringServices)
        setPhase(.subscribing)
        let work = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                self?.finish(testScript: testScript, token: token)
            }
        }
        testScriptWorkItem = work
        DispatchQueue.main.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: testScript.connectionDelayMilliseconds)),
            execute: work
        )
        return true
    }

    private func finish(testScript: CutoutSessionTestScript, token: ConnectionAttemptToken) {
        guard rustSessionState.connectionAttemptIsCurrent(token: token) else { return }
        if testScript.identificationProbeFailure != nil || testScript.failsConnection {
            _ = rustSessionState.connectionLinkDown(token: token)
            _ = rustSessionState.connectionTransportFailed(token: token)
        }
        if let failure = testScript.identificationProbeFailure {
            setPhase(.failed(.identificationFailed(failure)))
            return
        }
        if testScript.failsConnection {
            setPhase(.failed(.connectFailed("deterministic fixture")))
            guard testScript.emitsLateLiveAfterFailure else { return }
            let work = DispatchWorkItem { [weak self] in
                self?.onBleQueue {
                    self?.emit(testScript: testScript, token: token)
                }
            }
            testScriptWorkItem = work
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(500), execute: work)
            return
        }
        _ = rustSessionState.connectionLinkEstablished(token: token)
        for bytes in testScript.protocolNotifications {
            _ = rustSessionState.observeConnectionNotification(token: token, bytes: bytes)
        }
        _ = rustSessionState.resolveDeviceSession(
            token: token, identificationComplete: true, nowMs: clock.now().rawValue
        )
        if rustSessionState.verifiedConnectionAttemptIsCurrent(token: token) {
            let sink = CutoutSessionTestOperationSink()
            testOperationSink = sink
            let advertisement = CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier(token.platformIdentifier),
                localName: testScript.candidate.displayName, advertisedServiceUuids: []
            )
            let owner = makeDeviceTransport(token: token, advertisement: advertisement, sink: sink)
            liveOwner = owner
            attachDeviceControlsCallback()
            do {
                let step = try owner.handleLinkUp(at: clock.now())
                let channels = step.operations.compactMap { operation -> BluetoothUuid? in
                    guard case let .subscribe(channel) = operation else { return nil }
                    owner.handleNotificationStateUpdate(channel: channel, isNotifying: true, error: nil)
                    return channel
                }
                if let channel = channels.first {
                    for bytes in testScript.protocolNotifications {
                        _ = try owner.handleNotification(bytes: bytes, channel: channel, at: clock.now())
                    }
                    repeatTestProtocolNotifications(testScript, token: token, channel: channel)
                }
            } catch {
                setPhase(.failed(.sessionFailed(error.sessionMessage)))
                return
            }
        }
        emit(testScript: testScript, token: token)
    }

    private func emit(testScript: CutoutSessionTestScript, token: ConnectionAttemptToken) {
        guard rustSessionState.connectionAttemptIsCurrent(token: token),
              rustSessionState.connectionAttemptSnapshot().transport == .connected else { return }
        if testScript.startsLive {
            storedProtocolIdentityCandidate = testScript.candidate
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
        scheduleTestTelemetryUpdateIfNeeded(testScript, token: token)
        scheduleTestReconnectIfNeeded(testScript, token: token)
        scheduleTestBluetoothLossIfNeeded(testScript, token: token)
    }

    private func repeatTestProtocolNotifications(_ script: CutoutSessionTestScript, token: ConnectionAttemptToken, channel: BluetoothUuid) {
        testProtocolNotificationTimer?.cancel()
        testProtocolNotificationTimer = nil
        guard let interval = script.protocolNotificationIntervalMilliseconds, interval > 0 else { return }
        let timer = DispatchSource.makeTimerSource(queue: bleQueue)
        timer.schedule(deadline: .now() + .milliseconds(Int(clamping: interval)), repeating: .milliseconds(Int(clamping: interval)))
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            guard self.rustSessionState.verifiedConnectionAttemptIsCurrent(token: token),
                  let owner = self.liveOwner, owner.token == token else {
                self.testProtocolNotificationTimer?.cancel()
                self.testProtocolNotificationTimer = nil
                return
            }
            do {
                for bytes in script.protocolNotifications {
                    let at = self.clock.now()
                    self.applyNotificationStep(try owner.handleNotification(bytes: bytes, channel: channel, at: at), receivedAt: at)
                }
            } catch {
                self.testProtocolNotificationTimer?.cancel()
                self.testProtocolNotificationTimer = nil
                self.setPhase(.failed(.notificationIngestFailed(error.sessionMessage)))
            }
        }
        testProtocolNotificationTimer = timer
        timer.resume()
    }

    private func scheduleTestTelemetryUpdateIfNeeded(_ testScript: CutoutSessionTestScript, token: ConnectionAttemptToken) {
        guard let telemetry = testScript.telemetryUpdate else { return }
        let update = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                guard let self, self.rustSessionState.connectionAttemptIsCurrent(token: token) else { return }
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

    private func scheduleTestReconnectIfNeeded(_ testScript: CutoutSessionTestScript, token: ConnectionAttemptToken) {
        guard testScript.reconnectsAfterFirstLive, !testScriptDidReconnect else { return }
        testScriptDidReconnect = true
        let reconnect = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                guard let self, self.rustSessionState.connectionAttemptIsCurrent(token: token) else { return }
                _ = self.rustSessionState.connectionLinkDown(token: token)
                self.testScriptUpdateWorkItem?.cancel()
                guard let retryToken = self.rustSessionState.beginConnectionAttempt(
                    platformIdentifier: token.platformIdentifier, nowMs: self.clock.now().rawValue
                ).token else { return }
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
                        self?.finish(testScript: testScript, token: retryToken)
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

    private func scheduleTestBluetoothLossIfNeeded(_ testScript: CutoutSessionTestScript, token: ConnectionAttemptToken) {
        guard let delay = testScript.bluetoothLossAfterFirstLiveMilliseconds else { return }
        let loss = DispatchWorkItem { [weak self] in
            self?.onBleQueue {
                guard let self, self.rustSessionState.connectionAttemptIsCurrent(token: token) else { return }
                _ = self.rustSessionState.connectionLinkDown(token: token)
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
        liveOwner?.invalidate()
        _ = rustSessionState.disconnectConnectionAttempt()
        connectionDeadlineWorkItem?.cancel()
        connectionAttempt = nil
        publishConnectionSnapshot()
#if DEBUG
        if testScript != nil { _ = rustSessionState.disconnectConnectionAttempt() }
        testScriptWorkItem?.cancel()
        testScriptWorkItem = nil
        testScriptUpdateWorkItem?.cancel()
        testScriptUpdateWorkItem = nil
#endif
        suppressReconnect = true
        cancelPendingReconnect()
        musicCaptureContext.reset()
#if DEBUG
        if testScript != nil, isRecordOnly, captureBuilder != nil {
            finishCaptureAfterLinkDown()
        } else if testScript != nil, isRecordOnly, let completedCaptureURL = captureFileURL {
            captureFileURL = nil
            let generation = captureGeneration ?? .legacy
            captureGeneration = nil
            publishCaptureEvent(.finished(generation: generation, fileURL: completedCaptureURL))
        } else {
            finishCaptureAfterLinkDown()
        }
#else
        finishCaptureAfterLinkDown()
#endif
        isRecordOnly = false
        isDetectingProtocol = false
        selectedModel = nil
        selectedRoute = nil
        chargeEstimateProfile = nil
        vescBoardProfile = nil
        liveOwner = nil
        clearPendingWithoutResponseWrites()
        deviceDetectionSession.reset()
        clearPendingBegodeProbeResponses()
        clearProtocolDetectionExpiry()
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()
        connectionGattInventory.removeAll()
        displayState = RideDisplayState()
        hasObservedSpeedSnapshot = false
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        publishDisplayState()

        if let peripheral {
            if peripheral.state != .disconnected {
                retiringPeripheralIdentifiers.insert(peripheral.identifier)
            }
            peripheral.delegate = nil
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
        if case .failed = phase {
            return
        }
        cancelPendingReconnect()
        let bmsObservations = step.actions.compactMap { action in
            action.kind == .bmsSnapshot ? action.bmsSnapshot : nil
        }.flatMap(\.rawObservations)
        step.actions.forEach(applySessionAction)
        observeRideMapConnection(at: receivedAt)
        persistBmsSamples(bmsObservations)
        let snapshot = step.snapshot
        displayState = displayState.reducing(snapshot: snapshot, receivedAt: receivedAt)
        hasObservedSpeedSnapshot = hasObservedSpeedSnapshot || snapshot?.speed?.value != nil
        publishDisplayState()
        setPhase(.live)
    }

    private func applySessionAction(_ action: SessionAction) {
        switch action.kind {

        case .faultHistoryReadback:
            faultHistoryReadback = action.faultHistoryReadback
            publishFaultHistoryReadback()
        case .bmsSnapshot:
            guard let snapshot = action.bmsSnapshot, snapshot != bmsSnapshot else {
                return
            }
            bmsSnapshot = snapshot
            publishBmsSnapshot()
        case .event:
            applyProtocolIdentityModelId(action.veteranProtocolModelId)
        case .subscribe, .write, .disconnect, .notificationIngest:
            break
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
            storedProtocolIdentityCandidate = DevicePickerDiscoveryCandidate(candidate: candidate)
            publishProtocolIdentityCandidate()
            record("protocol_identity=\(candidate.detail)")
            updateCaptureIdentity()
        case (.none, _), (_, .none):
            break
        }
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
        publishBmsSnapshot()
    }

    private func clearProtocolIdentityCandidate() {
        guard protocolIdentityCandidate != nil else {
            return
        }
        storedProtocolIdentityCandidate = nil
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
        storedProtocolIdentityCandidate = pickerCandidate
        publishProtocolIdentityCandidate()
        record("protocol_identity=\(candidate.detail)")
        updateCaptureIdentity()
    }

    private func setPhase(_ phase: SessionConnectionPhase) {
        if case .failed = phase, let token = connectionSnapshot.token {
            liveOwner?.invalidate()
            let wasPending = connectionSnapshot.readiness == .pending
            _ = rustSessionState.connectionTransportFailed(token: token)
            publishConnectionSnapshot()
            if wasPending {
                recordUnresolvedProtocolDetection(.unsupported, on: peripheral?.state == .connected ? peripheral : nil)
                return
            }
        }
        self.phase = phase
        // Publish the phase first so the app model can accept the dependent
        // settings snapshot only after it has entered the live link.
        let generation = connectionSnapshot.generation
        publishOnMain {
            guard self.connectionSnapshot.generation == generation else { return }
            self.onPhaseChange?(phase)
        }
        publishDeviceControls(deviceControlsSnapshot)
    }

    func acceptsConnectionCallback(_ peripheral: CBPeripheral, token: ConnectionAttemptToken) -> Bool {
        assertOnBleQueue()
        return self.peripheral === peripheral
            && connectionAttempt?.token == token
            && !retiringPeripheralIdentifiers.contains(peripheral.identifier)
            && rustSessionState.connectionAttemptIsCurrent(token: token)
    }

    private func publishConnectionSnapshot() {
        let snapshot = connectionSnapshot
        publishOnMain { [weak self] in
            guard let self, self.connectionSnapshot.revision == snapshot.revision else { return }
            self.onConnectionSnapshotChange?(snapshot)
        }
    }

    private func prepareConnectionAttempt(to peripheral: CBPeripheral) {
        assertOnBleQueue()
        let previous = self.peripheral
        liveOwner?.invalidate()
        liveOwner = nil
        let snapshot = rustSessionState.beginConnectionAttempt(
            platformIdentifier: peripheral.identifier.uuidString,
            nowMs: clock.now().rawValue
        )
        guard let token = snapshot.token else { return }
        if let vescBoardProfile {
            _ = rustSessionState.configureConnectionVescProfile(token: token, profile: vescBoardProfile)
        }
        connectionDeadlineWorkItem?.cancel()
        clearProtocolDetectionExpiry()
        clearPendingBegodeProbeResponses()
        connectionGattInventory.removeAll()
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()
        clearPendingWithoutResponseWrites()
        connectionAttempt = CoreBluetoothConnectionAttempt(token: token, peripheral: peripheral, owner: self)
        if let previous, previous.state == .connected || previous.state == .connecting || previous.state == .disconnecting {
            retiringPeripheralIdentifiers.insert(previous.identifier)
            previous.delegate = nil
            central?.cancelPeripheralConnection(previous)
        }
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.rustSessionState.connectionAttemptIsCurrent(token: token),
                  self.connectionSnapshot.readiness == .pending else { return }
            let expired = self.rustSessionState.expireConnectionAttempt(token: token, nowMs: self.clock.now().rawValue)
            guard expired.readiness == .recordOnly else { return }
            self.recordUnresolvedProtocolDetection(.timedOut, on: self.peripheral)
            self.publishConnectionSnapshot()
        }
        connectionDeadlineWorkItem = work
        let now = clock.now().rawValue
        let deadline = snapshot.deadlineMs ?? now
        let delay = deadline > now ? deadline - now : 0
        bleQueue.asyncAfter(deadline: .now() + .milliseconds(Int(clamping: delay)), execute: work)
        publishConnectionSnapshot()
    }

    func prepareRestoredConnection(from restoredPlatformIdentifiers: [String]) -> String? {
        guard let selectedIdentifier = CoreBluetoothRestorationPolicy.selectedPlatformIdentifier(
            savedPlatformIdentifier: selectedDeviceStore.platformIdentifier,
            restoredPlatformIdentifiers: restoredPlatformIdentifiers
        ) else {
            return nil
        }
        rustSessionState.setDeviceConnectionIntent(intent: .reconnect)
        return selectedIdentifier
    }

    private func startPreparedConnection() {
        guard let attempt = connectionAttempt,
              rustSessionState.connectionAttemptIsCurrent(token: attempt.token),
              !retiringPeripheralIdentifiers.contains(attempt.peripheral.identifier)
        else { return }
        attempt.peripheral.delegate = attempt
        central?.connect(attempt.peripheral)
    }

    private func drainRetiredConnection(_ peripheral: CBPeripheral) -> Bool {
        guard retiringPeripheralIdentifiers.remove(peripheral.identifier) != nil else { return false }
        // Start after the terminal callback's queue turn, so a same-device retry cannot
        // receive the previous attempt's queued native completion as its own.
        if connectionAttempt?.peripheral.identifier == peripheral.identifier {
            let reference = WeakCutoutSessionCoreReference(self)
            bleQueue.async { reference.value?.startPreparedConnection() }
        }
        return true
    }

    private func connectRecordOnly(to peripheral: CBPeripheral, using advertisement: CoreBluetoothAdvertisement, note: String?, annotations: [String]) {
        cancelPendingReconnect()
        rustSessionState.setDeviceConnectionIntent(intent: .recordOnly)
        prepareConnectionAttempt(to: peripheral)
        suppressReconnect = false
        isRecordOnly = true
        isDetectingProtocol = false
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = nil
        selectedRoute = nil
        liveOwner = nil
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        guard startCapture(
            reason: "record-only",
            annotations: ["route=record_only"] + annotations + (note.map {
                [pevcapAnnotation(key: "user_note", value: $0)]
            } ?? [])
        ) else { return }
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        setPhase(.discoveringServices)
        central?.stopScan()
        startPreparedConnection()
    }

    private func connectForProtocolDetection(
        to peripheral: CBPeripheral,
        using advertisement: CoreBluetoothAdvertisement,
        intent: DeviceConnectionIntentDto = .use
    ) {
        cancelPendingReconnect()
        rustSessionState.setDeviceConnectionIntent(intent: intent)
        prepareConnectionAttempt(to: peripheral)
        suppressReconnect = false
        isRecordOnly = false
        isDetectingProtocol = true
        clearProtocolDetectionExpiry()
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = nil
        selectedRoute = nil
        liveOwner = nil
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        guard startCapture(reason: "protocol-detection", annotations: ["intent=manual_use"])
        else { return }
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        setPhase(.discoveringServices)
        central?.stopScan()
        startPreparedConnection()
    }

    private func makeDeviceTransport(
        token: ConnectionAttemptToken,
        advertisement: CoreBluetoothAdvertisement,
        sink: CoreBluetoothOperationSink,
        writeLimit: TransportWriteLimitBytes = TransportWriteLimitBytes(23)
    ) -> DeviceSessionTransport {
        let owner = DeviceSessionTransport(
            state: rustSessionState, token: token, advertisement: advertisement,
            writeLimit: writeLimit, operationSink: sink,
            queue: bleQueue, clock: clock
        )
        owner.onSubscriptionFailure = { [weak self] channel, error in
            guard let self, self.liveOwner?.token == token else { return }
            self.failConnectionCapture()
            self.cancelFailedConnectionAttempt()
            self.setPhase(.failed(.notificationFailed(
                error?.sessionMessage ?? "notifications disabled for \(channel)"
            )))
        }
        if let chargeEstimateProfile { owner.configureChargeEstimate(profile: chargeEstimateProfile) }
        return owner
    }

    private func buildOwner(for peripheral: CBPeripheral) {
        guard liveOwner == nil, let advertisement, let token = connectionSnapshot.token,
              rustSessionState.verifiedConnectionAttemptIsCurrent(token: token) else { return }
        do {
            beginBmsStorageSession()
            let owner = makeDeviceTransport(
                token: token,
                advertisement: advertisement,
                sink: self,
                writeLimit: TransportWriteLimitBytes(
                    UInt16(clamping: peripheral.maximumWriteValueLength(for: .withoutResponse))
                )
            )
            liveOwner = owner
            attachDeviceControlsCallback()
            setPhase(.subscribing)
            owner.recordInventory(CoreBluetoothGattInventory(services: peripheral.services ?? []))
            applyLinkUpStep(try owner.handleLinkUp(at: clock.now()))
        } catch {
            setPhase(.failed(.sessionFailed(error.sessionMessage)))
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
            return nil
        }
    }

    private func handleDisconnect(from peripheral: CBPeripheral, error: Error?) {
        guard let attempt = connectionAttempt,
              acceptsConnectionCallback(peripheral, token: attempt.token) else {
            return
        }
        let wasDetecting = connectionSnapshot.readiness == .pending
        liveOwner?.invalidate()
        _ = rustSessionState.connectionLinkDown(token: attempt.token)
        connectionDeadlineWorkItem?.cancel()
        publishConnectionSnapshot()
        if wasDetecting, !rustSessionState.shouldRetryIdentification() {
            recordUnresolvedProtocolDetection(.unsupported, on: nil)
            finishCaptureAfterLinkDown()
            return
        }
        handleTransportTermination(
            platformIdentifier: peripheral.identifier.uuidString,
            error: error,
            reconnect: { [weak self, weak peripheral] in
                guard let self, let peripheral else { return }
                self.prepareConnectionAttempt(to: peripheral)
                _ = self.deviceDetectionSession.observeAdvertisement(name: self.advertisement?.localName.map { Data($0.utf8) })
                self.startPreparedConnection()
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

    func recoverConnection(after failure: SessionConnectionFailure, disconnect: () -> Void) {
        onBleQueue {
            record("connection_attempt_failed=\(failure)")
            clearProtocolDetectionExpiry()
            clearPendingBegodeProbeResponses()
            clearPendingWithoutResponseWrites()
            liveOwner = nil
            setPhase(.discoveringServices)
            disconnect()
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
        clearProtocolDetectionExpiry()
        markOutstandingBegodeProbeResponsesMissing()
        finishCaptureAfterLinkDown()
        let wasRecordOnlyConnection = isRecordOnly
        isRecordOnly = false
        isDetectingProtocol = selectedRoute == nil
        liveOwner = nil
        if let token = connectionAttempt?.token {
            _ = rustSessionState.resetDeviceDetectionLinkForAttempt(token: token)
        }
        subscribedCharacteristics.removeAll()
        clearPendingWithoutResponseWrites()
        pendingServiceDiscoveries.removeAll()

        guard !suppressReconnect else {
            suppressReconnect = false
            return
        }

        guard !wasRecordOnlyConnection else {
            setPhase(.scanning)
            central?.scanForPeripherals(withServices: nil)
            return
        }

        scheduleReconnect(
            platformIdentifier: platformIdentifier,
            error: error,
            reconnect: reconnect
        )
    }

    private func scheduleReconnect(
        platformIdentifier: String,
        error: Error?,
        reconnect: @escaping () -> Void
    ) {
        rustSessionState.setDeviceConnectionIntent(intent: .reconnect)
        let connectionGeneration = connectionSnapshot.generation
        guard let schedule = reconnectController.schedule(
            jitter: reconnectJitter(),
            operation: { [weak self] in
                guard let self else { return }
                self.onBleQueue {
                    guard !self.suppressReconnect,
                          self.connectionSnapshot.generation == connectionGeneration else { return }
                    guard self.startCapture(
                        reason: "protocol-detection",
                        annotations: ["intent=previously_connected"]
                    ) else { return }
                    reconnect()
                }
            }
        ) else {
            rustSessionState.setDeviceConnectionIntent(intent: .recordOnly)
            setPhase(.failed(.connectFailed(error.sessionMessage)))
            central?.scanForPeripherals(withServices: nil)
            return
        }

        isDetectingProtocol = selectedRoute == nil
        setPhase(.discoveringServices)

        let now = clock.now().rawValue
        let delay = schedule.delayMilliseconds
        let deadline = MonotonicMilliseconds(now > UInt64.max - delay ? UInt64.max : now + delay)
        let retry = SessionConnectionRetry(
            platformIdentifier: platformIdentifier,
            attempt: schedule.attempt,
            deadline: deadline,
            failure: .connectFailed(error.sessionMessage)
        )
        publishOnMain {
            guard self.connectionSnapshot.generation == connectionGeneration else { return }
            self.onReconnectScheduled?(retry)
        }

        record("reconnect_attempt=\(schedule.attempt) delay_ms=\(schedule.delayMilliseconds)")
    }

    private func cancelPendingReconnect() {
        reconnectController.cancel()
    }

    private func record(_ message: String) {
        diagnosticLog.append(message)
        publishOnMain { self.onRecord?(message) }
    }

    private func recordRideMapDiagnostic(_ message: String) {
        let reference = WeakCutoutSessionCoreReference(self)
        bleQueue.async {
            reference.value?.record(message)
        }
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


    private func publishDeviceControls(_ value: DeviceControlsSnapshot) {
        publishOnMain { [weak self] in
            guard let self, self.connectionSnapshot.revision == value.connection.revision else { return }
            self.onDeviceControlsChange?(value)
        }
    }

    private func attachDeviceControlsCallback() {
        guard let owner = liveOwner else { return }
        owner.onControlsChange = { [weak self, weak owner] state in
            guard let self, let owner, self.liveOwner === owner else { return }
            self.publishDeviceControls(state)
        }
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

    private func publishProtocolIdentityCandidate() {
        let value = protocolIdentityCandidate
        let generation = connectionSnapshot.generation
        publishOnMain {
            guard self.connectionSnapshot.generation == generation else { return }
            self.onProtocolIdentityCandidateChange?(value)
        }
    }

    private func publishCaptureEvent(_ event: CaptureEvent) {
        publishOnMain { self.onCaptureEvent?(event) }
    }

    private func beginCaptureGeneration() -> CaptureGeneration {
        nextCaptureGeneration = nextCaptureGeneration == .max ? 1 : nextCaptureGeneration + 1
        let generation = CaptureGeneration(rawValue: nextCaptureGeneration)
        captureGeneration = generation
        return generation
    }

    private func publishCaptureProgress() {
        guard let generation = captureGeneration else { return }
        publishCaptureEvent(.progress(generation: generation, captureProgress()))
    }

    private func publishCaptureFailure() {
        guard let generation = captureGeneration else { return }
        publishCaptureEvent(.failed(generation: generation))
    }

    private func startRideMapWritePolling() {
        let queue = rideMapQueue
        let reference = WeakCutoutSessionCoreReference(self)
        queue.async {
            guard let self = reference.value, self.rideMapWritePoller == nil else { return }
            let timer = DispatchSource.makeTimerSource(queue: queue)
            timer.schedule(
                deadline: .now() + .milliseconds(100),
                repeating: .milliseconds(100),
                leeway: .milliseconds(25)
            )
            timer.setEventHandler { [reference] in
                reference.value?.drainRideMapWrites()
            }
            self.rideMapWritePoller = timer
            timer.resume()
        }
    }

    private func startRideMapRestoration() {
        let queue = rideMapQueue
        let reference = WeakCutoutSessionCoreReference(self)
        queue.async {
            guard let self = reference.value,
                  !self.rideMapRestorationStarted
            else { return }
            self.rideMapRestorationStarted = true
            guard let rideMapState = self.rideMapState else {
                self.publishRideMapAvailability()
                return
            }
            do {
                let snapshot = try rideMapState.restore(atMs: self.clock.now().rawValue)
                if let snapshot {
                    self.publishRideMapSnapshot(snapshot)
                }
                self.publishRideMapAvailability()
                self.synchronizeRideMapLocationDemand()
                // A verified connection can complete while restoration is pending. Replay the
                // current BLE admission after readiness so that auto-start and reassociation do
                // not depend on a second notification arriving from the peripheral.
                self.onBleQueue { [weak self] in
                    guard let self else { return }
                    self.observeRideMapConnection(at: self.clock.now())
                }
            } catch let error as MobileRideMapError {
                self.rideMapRestorationStarted = false
                self.publishRideMapError(
                    error,
                    context: MobileRideMapErrorContext(snapshot: nil)
                )
                self.publishRideMapAvailability()
            } catch {
                self.rideMapRestorationStarted = false
                let mapped = MobileRideMapError.storageError(String(describing: error))
                self.publishRideMapError(
                    mapped,
                    context: MobileRideMapErrorContext(snapshot: nil)
                )
                self.publishRideMapAvailability()
            }
        }
    }

    private func drainRideMapWrites() {
        guard let rideMapState,
              rideMapState.initializationError == nil,
              rideMapState.isReady
        else { return }
        guard rideMapState.hasPendingLocationWrites
            || rideMapState.currentSnapshot(atMs: clock.now().rawValue)?.state.isOpen == true
        else {
            return
        }
        publishRideMapDecisions(rideMapState.pollLocationWrites())
    }

    private func observeRideMapConnection(at receivedAt: MonotonicMilliseconds) {
        guard let rideMapState,
              rideMapState.initializationError == nil,
              rideMapState.isReady
        else {
            return
        }

        let snapshot = connectionSnapshot
        guard let token = snapshot.token else { return }
        let connectionGeneration = token.generation
        let queue = rideMapQueue
        let reference = WeakCutoutSessionCoreReference(self)
        queue.async {
            guard let self = reference.value,
                  self.connectionSnapshot.generation == connectionGeneration
            else { return }
            do {
                let previousRideID = rideMapState
                    .currentSnapshot(atMs: receivedAt.rawValue)?.rideID
                let snapshot = try rideMapState.ensureRecordingForVerifiedConnection(
                    connectionState: self.rustSessionState,
                    token: token,
                    atMs: receivedAt.rawValue,
                )
                if snapshot != nil {
                    // Admission returns the current ride for repeated notifications too.
                    // Only reset when Rust created a different ride, so reconnects and
                    // telemetry notifications cannot clear the same trip repeatedly.
                    if snapshot?.rideID != previousRideID {
                        _ = self.resetTripMeterForNewRide()
                    }
                    _ = try rideMapState.observeTelemetry(atMs: receivedAt.rawValue)
                    if let snapshot = rideMapState.currentSnapshot(atMs: receivedAt.rawValue) {
                        self.publishRideMapSnapshot(snapshot)
                    }
                }
                self.synchronizeRideMapLocationDemand()
            } catch let error as MobileRideMapError where error == .staleConnection {
                return
            } catch let error as MobileRideMapError {
                // Admission can create or replace the ride while this queue is awaiting Rust.
                // Capture the context at the failure boundary so a first auto-start and a
                // terminal-ride replacement cannot publish an unscoped error that the app
                // model incorrectly rejects as stale.
                let snapshot = rideMapState.currentSnapshot(atMs: receivedAt.rawValue)
                // Publish the authoritative post-admission snapshot before its scoped error.
                // The model rejects ride-scoped errors until it has seen that ride; ordering the
                // pair makes the stale-work invariant hold for first starts and replacements.
                if let snapshot {
                    self.publishRideMapSnapshot(snapshot)
                }
                let errorContext = MobileRideMapErrorContext(snapshot: snapshot)
                self.publishRideMapError(error, context: errorContext)
                self.recordRideMapDiagnostic("ride_map_connection_error=\(error)")
            } catch {
                self.recordRideMapDiagnostic("ride_map_connection_error=\(error)")
            }
        }
    }

    private func persistBmsSamples(_ observations: [BmsRawVoltageObservation]) {
        guard let rideMapState,
              rideMapState.initializationError == nil,
              rideMapState.isReady,
              let deviceIdentity = protocolIdentityCandidate?.platformIdentifier
                ?? peripheral?.identifier.uuidString,
              let wallClockMilliseconds = unixMilliseconds(for: wallClock())
        else {
            return
        }
        let samples = bmsStorageSamples(
            observations: observations,
            wallClockMilliseconds: wallClockMilliseconds,
            sessionIdentifier: bmsStorageSessionIdentifier
        )
        guard !samples.isEmpty else { return }

        let queue = rideMapQueue
        let reference = WeakCutoutSessionCoreReference(self)
        queue.async {
            guard let self = reference.value else { return }
            do {
                try rideMapState.recordBmsVoltageSamples(
                    deviceIdentity: deviceIdentity,
                    samples: samples
                )
            } catch {
                self.recordRideMapDiagnostic("bms_storage_error=\(error)")
            }
        }
    }

    private func beginBmsStorageSession() {
        bmsStorageSessionIdentifier = UUID().uuidString
    }
    private func publishRideMapDecisions(_ decisions: [MobileRideMapDecisionDto]) {
        guard let rideMapState else { return }
        let snapshot = rideMapState.currentSnapshot(atMs: clock.now().rawValue)
            ?? latestRideMapSnapshot
        for decision in decisions {
            switch decision {
            case let .storageError(message):
                publishRideMapError(
                    .storageError(message),
                    context: MobileRideMapErrorContext(snapshot: snapshot)
                )
            default:
                guard let snapshot else { continue }
                publishOnMain {
                    self.onRideMapDecisionChange?(snapshot, decision)
                }
            }
        }
    }

    private func publishRideMapSnapshot(_ snapshot: MobileRideMapSnapshotDto) {
        latestRideMapSnapshot = snapshot
        updateRideLocationDemand(for: snapshot.state)
        publishOnMain { self.onRideMapSnapshotChange?(snapshot) }
    }

    private func publishRideMapError(
        _ error: MobileRideMapError,
        context: MobileRideMapErrorContext
    ) {
        let event = MobileRideMapErrorEvent(context: context, error: error)
        publishOnMain { self.onRideMapErrorChange?(event) }
    }

    private func publishRideMapAvailability() {
        let availability: MobileRideMapAvailability
        if rideMapState?.initializationError != nil {
            availability = .storageUnavailable
        } else if rideMapState?.isReady == false {
            availability = .checking
        } else if !CLLocationManager.locationServicesEnabled() {
            availability = .servicesDisabled
        } else {
            switch locationManager.authorizationStatus {
            case .notDetermined:
                availability = .permissionRequired
            case .authorizedAlways, .authorizedWhenInUse:
                availability = .ready
            case .denied:
                availability = .denied
            case .restricted:
                availability = .restricted
            @unknown default:
                availability = .locationUnavailable
            }
        }
        publishOnMain { self.onRideMapAvailabilityChange?(availability) }
    }

    private func synchronizeRideMapLocationDemand() {
        let shouldReceiveLocations = onRideMapQueue {
            rideMapState?.currentSnapshot(atMs: clock.now().rawValue)?.state == .active
        }
        onBleQueue {
            guard shouldReceiveLocations else {
                locationManager.stopUpdatingLocation()
                return
            }
            guard CLLocationManager.locationServicesEnabled() else {
                locationManager.stopUpdatingLocation()
                return
            }
            switch locationManager.authorizationStatus {
            case .authorizedAlways, .authorizedWhenInUse:
                locationManager.startUpdatingLocation()
            case .notDetermined:
                guard !didRequestWhenInUseLocationAuthorization else { return }
                didRequestWhenInUseLocationAuthorization = true
                locationManager.requestWhenInUseAuthorization()
            case .denied, .restricted:
                locationManager.stopUpdatingLocation()
            @unknown default:
                locationManager.stopUpdatingLocation()
            }
        }
    }

    /// Records one bounded music observation independently of BLE frame arrival.
    /// The Rust writer owns the capture event and keeps metadata low-rate.
    public func updateMusicCaptureObservation(_ observation: MobilePevcapMusicEventDto?) {
        onBleQueue {
            self.musicCaptureContext.update(observation)
            guard let observation else {
                _ = self.captureBuilder?.setMusicContext(music: nil)
                return
            }
            guard let builder = self.captureBuilder
            else { return }
            _ = self.acceptCaptureWrite(builder.recordMusicEvent(music: observation))
        }
    }

    /// Applies the ride's Rust-owned retention policy to future PEVCAP music writes.
    public func updateMusicCapturePolicy(_ policy: MobileMusicHistoryPolicyDto) {
        captureMusicHistoryPolicy = policy
        onBleQueue {
            _ = self.captureBuilder?.setMusicHistoryPolicy(policy: policy)
        }
    }

    @discardableResult
    func captureFrame(
        direction: String,
        characteristic: CBUUID,
        service: CBUUID? = nil,
        bytes: Data,
        telemetry: RawTelemetryReadback? = nil
    ) -> Bool {
        guard let channel = BluetoothUuid(coreBluetoothUuid: characteristic) else {
            return false
        }

        switch direction {
        case "notify":
            guard let serviceUuid = service.flatMap(BluetoothUuid.init(coreBluetoothUuid:)) else {
                record("capture_error=notification_missing_service characteristic=\(characteristic.uuidString)")
                publishCaptureFailure()
                setPhase(.failed(.notificationFailed("missing service UUID for \(characteristic.uuidString)")))
                finishCaptureWriter()
                return false
            }
            guard let builder = captureBuilder else { return true }
            let location = phoneLocationState.currentSnapshot().latestSample
            let accepted = builder.recordNotificationWithContext(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                characteristic: channel.bytes,
                service: serviceUuid.bytes,
                bytes: bytes,
                telemetry: telemetry?.dto,
                phoneLocation: location
            )
            guard acceptCaptureWrite(accepted) else { return false }
            record("capture_queue_depth=\(captureBuilder?.writerStatus().queuedMessages ?? 0)")
        case "write_without_response":
            guard let builder = captureBuilder else { return true }
            let accepted = builder.recordWriteWithoutResponse(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                characteristic: channel.bytes,
                bytes: bytes
            )
            guard acceptCaptureWrite(accepted) else { return false }
            record("capture_queue_depth=\(captureBuilder?.writerStatus().queuedMessages ?? 0)")
        default:
            return false
        }
        return true
    }

    @discardableResult
    private func startCapture(
        reason: String,
        annotations extraAnnotations: [String] = [],
        evidence: String = "hardware_tested"
    ) -> Bool {
        let generation = beginCaptureGeneration()
        captureStartedAt = clock.now()
        captureNotificationCount = 0

        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let url = directory.appendingPathComponent("cutout-btle-capture-\(Int(Date().timeIntervalSince1970))-\(UUID().uuidString).jsonl")
        let builder = MobilePevcapCaptureBuilder(
            wallClockStartUnixMs: MobileWallClockUnixMillisDto(milliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)),
            platformId: advertisement?.peripheralIdentifier.rawValue ?? "ios",
            writeLimit: MobileTransportWriteLimitDto(bytes: 23)
        )
        _ = builder.setMusicHistoryPolicy(policy: captureMusicHistoryPolicy)
        _ = builder.setMusicCaptureStartMonotonicMs(monotonicMs: captureStartedAt?.rawValue ?? 0)
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
        captureFileURL = url
        _ = builder.setMusicContext(music: musicCaptureContext.current)
        publishCaptureEvent(.started(generation: generation, fileURL: url))
        guard builder.startWriter(path: url.path) else {
            failConnectionCapture()
            record("capture_error=writer_start_failed")
            captureBuilder = nil
            captureGeneration = nil
            captureFileURL = nil
            captureStartedAt = nil
            publishCaptureEvent(.failed(generation: generation))
            setPhase(.failed(.sessionFailed("capture writer failed to start")))
            cancelFailedConnectionAttempt()
            return false
        }
        musicCaptureContext.reset()
        record("capture_file=\(url.path)")
        updateCaptureIdentity()
        return true
    }

    private func cancelFailedConnectionAttempt() {
        connectionDeadlineWorkItem?.cancel()
        connectionDeadlineWorkItem = nil
        liveOwner?.invalidate()
        liveOwner = nil
        if let attempt = connectionAttempt {
            attempt.peripheral.delegate = nil
            if attempt.peripheral.state != .disconnected {
                retiringPeripheralIdentifiers.insert(attempt.peripheral.identifier)
                central?.cancelPeripheralConnection(attempt.peripheral)
            }
        }
        connectionAttempt = nil
        peripheral = nil
    }

    private func acceptCaptureWrite(_ accepted: Bool) -> Bool {
        guard !accepted else { return true }
        failConnectionCapture()
        let status = captureBuilder?.writerStatus()
        record("capture_error=writer_failed \(status?.lastError ?? "unknown")")
        publishCaptureFailure()
        setPhase(.failed(.sessionFailed("capture writer queue overrun")))
        finishCaptureWriter()
        return false
    }

    private func failConnectionCapture() {
        guard let token = connectionAttempt?.token else { return }
        _ = rustSessionState.failConnectionCapture(token: token)
        publishConnectionSnapshot()
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
        musicCaptureContext.reset()
        guard let builder = captureBuilder else { return }
        let completedCaptureURL = captureFileURL
        let completedCaptureGeneration = captureGeneration ?? .legacy
        captureBuilder = nil
        captureGeneration = nil
        captureFileURL = nil
        captureStartedAt = nil
        musicCaptureContext.reset()
        let finish = DispatchWorkItem { [weak self] in
            #if DEBUG
            self?.captureFinishWriterGate?()
            #endif
            let writerSucceeded = builder.finishWriter()
            let succeeded = priorWriteSucceeded && writerSucceeded
            guard publishesResult, let self else { return }
            self.onBleQueue {
                if succeeded, let completedCaptureURL {
                    self.publishCaptureEvent(
                        .finished(generation: completedCaptureGeneration, fileURL: completedCaptureURL)
                    )
                } else {
                    self.record("capture_error=writer_finish_failed")
                    self.publishCaptureEvent(.failed(generation: completedCaptureGeneration))
                }
            }
        }
        DispatchQueue.global(qos: .utility).async(execute: finish)
    }

#if DEBUG
    func finishCaptureForTesting(priorWriteSucceeded: Bool = true) {
        onBleQueue {
            self.finishCaptureWriter(publishesResult: true, priorWriteSucceeded: priorWriteSucceeded)
        }
    }
#endif

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
            let selectedIdentifier = prepareRestoredConnection(from: restoredIdentifiers),
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
        _ = rustSessionState.selectDiscoveredPlatform(
            platformIdentifier: selectedIdentifier
        )

        discoveredPeripherals[restoredAdvertisement.peripheralIdentifier] = restoredPeripheral
        prepareConnectionAttempt(to: restoredPeripheral)
        advertisement = restoredAdvertisement
        peripheral = restoredPeripheral
        selectedRoute = nil
        selectedModel = nil
        isRecordOnly = false
        isDetectingProtocol = true
        suppressReconnect = false
        restoredPeripheral.delegate = connectionAttempt
        deviceDetectionSession.reset()
        _ = deviceDetectionSession.observeAdvertisement(
            name: restoredAdvertisement.localName.map { Data($0.utf8) }
        )
        record("central_restore=selected state=\(restoredPeripheral.state.rawValue) observations=\(discovery.observations.count)")

        switch restoredPeripheral.state {
        case .connected:
            if let token = connectionAttempt?.token {
                _ = rustSessionState.connectionLinkEstablished(token: token)
                publishConnectionSnapshot()
            }
            guard prepareRestoredRide() else { return }
            if central?.state == .poweredOn {
                resumeConnectedPeripheral(restoredPeripheral)
            } else {
                setPhase(.discoveringServices)
            }
        case .connecting:
            guard prepareRestoredRide() else { return }
            setPhase(.discoveringServices)
        case .disconnected, .disconnecting:
            record("central_restore=selected_not_connected")
            _ = rustSessionState.disconnectConnectionAttempt()
            cancelFailedConnectionAttempt()
            publishConnectionSnapshot()
            advertisement = nil
            selectedRoute = nil
            selectedModel = nil
        @unknown default:
            record("central_restore=unknown_peripheral_state")
        }
    }

    @discardableResult
    func prepareRestoredRide() -> Bool {
        guard startCapture(reason: "protocol-detection", annotations: ["intent=previously_connected"])
        else { return false }
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        return true
    }

    func resumeConnectedPeripheral(_ peripheral: CBPeripheral) {
        assertOnBleQueue()
        guard liveOwner == nil else { return }
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
                    bindDiscoveredCharacteristic(channel, characteristic)
                }
            }
            recordGattFingerprints(service: service)
            pendingServiceDiscoveries.remove(service.uuid)
        }
        guard pendingServiceDiscoveries.isEmpty else { return }
        if let token = connectionAttempt?.token {
            _ = rustSessionState.observeConnectionGatt(token: token, fingerprints: connectionGattInventory)
        }
        if isDetectingProtocol {
            beginProtocolDetection(on: peripheral)
        } else {
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
            selectedRoute != nil || isDetectingProtocol,
            peripheral.state == .connected || peripheral.state == .connecting
        {
            peripheral.identifier.uuidString
        } else {
            nil
        }
        publishBluetoothRestoration(restoredPlatformIdentifier)
    }

    func publishBluetoothRestoration(_ restoredPlatformIdentifier: String?) {
        let restoredPhase = phase
        publishOnMain {
            self.onBluetoothRestorationResolved?(restoredPlatformIdentifier)
            if restoredPlatformIdentifier != nil {
                self.onPhaseChange?(restoredPhase)
            }
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
            if let token = connectionSnapshot.token {
                _ = rustSessionState.connectionLinkDown(token: token)
                connectionDeadlineWorkItem?.cancel()
                publishConnectionSnapshot()
            }
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
        if let peripheral, selectedRoute != nil || isDetectingProtocol {
            switch peripheral.state {
            case .connected:
                if connectionSnapshot.transport != .connected, let advertisement {
                    connectForProtocolDetection(
                        to: peripheral,
                        using: advertisement,
                        intent: .reconnect
                    )
                    return
                }
                resumeConnectedPeripheral(peripheral)
                record("central_state=restored_session")
                return
            case .connecting:
                record("central_state=restored_session")
                return
            case .disconnected, .disconnecting:
                if let advertisement {
                    connectForProtocolDetection(
                        to: peripheral,
                        using: advertisement,
                        intent: .reconnect
                    )
                    return
                }
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
        guard central === self.central, let attempt = connectionAttempt,
              acceptsConnectionCallback(peripheral, token: attempt.token) else { return }
        _ = rustSessionState.connectionLinkEstablished(token: attempt.token)
        publishConnectionSnapshot()
        setPhase(.discoveringServices)
        peripheral.delegate = attempt
        if isRecordOnly || isDetectingProtocol {
            _ = captureBuilder?.recordLinkUp(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                maxWriteLen: MobileTransportWriteLimitDto(bytes: UInt16(clamping: peripheral.maximumWriteValueLength(for: .withoutResponse)))
            )
        }
        peripheral.discoverServices(discoveryServiceUuidsForSelectedRoute)
    }

    public func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        assertOnBleQueue()
        guard central === self.central else { return }
        if drainRetiredConnection(peripheral) { return }
        record("connect_failed=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
        handleDisconnect(from: peripheral, error: error)
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        assertOnBleQueue()
        guard central === self.central else { return }
        if drainRetiredConnection(peripheral) { return }
        handleDisconnect(from: peripheral, error: error)
    }
}

extension CutoutSessionCore: CBPeripheralDelegate {
    public func peripheralIsReady(toSendWriteWithoutResponse peripheral: CBPeripheral) {
        assertOnBleQueue()
        guard self.peripheral === peripheral else { return }
        flushPendingWithoutResponseWrites()
    }

    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        assertOnBleQueue()
        if let error {
            setPhase(.failed(.serviceDiscoveryFailed(error.sessionMessage)))
            return
        }
        let services = peripheral.services ?? []
        if services.isEmpty {
            if let token = connectionAttempt?.token {
                _ = rustSessionState.observeConnectionGatt(token: token, fingerprints: [])
            }
            recordUnresolvedProtocolDetection(.unsupported, on: peripheral)
            return
        }
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
                bindDiscoveredCharacteristic(channel, characteristic)
            }
        }
        recordGattFingerprints(service: service)
        pendingServiceDiscoveries.remove(service.uuid)
        if pendingServiceDiscoveries.isEmpty, let token = connectionAttempt?.token {
            _ = rustSessionState.observeConnectionGatt(token: token, fingerprints: connectionGattInventory)
        }
        if isRecordOnly {
            subscribeRecordOnlyCharacteristics(service.characteristics ?? [], on: peripheral)
            if pendingServiceDiscoveries.isEmpty {
                if let token = connectionAttempt?.token {
                    _ = rustSessionState.connectionTransportFailed(token: token)
                    publishConnectionSnapshot()
                }
                setPhase(.live)
            }
            return
        }
        if isDetectingProtocol {
            if pendingServiceDiscoveries.isEmpty {
                beginProtocolDetection(on: peripheral)
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
        guard subscribedCharacteristics[channel] === characteristic else {
            record("notification_ignored=unbound_characteristic service=\(characteristic.service?.uuid.uuidString ?? "unknown") characteristic=\(characteristic.uuid.uuidString)")
            return
        }
        let detectionResolution = observeDetectionNotification(channel: channel, bytes: value)
        if rustSessionState.connectionAttemptSnapshot().readiness == .conflicted {
            setPhase(.failed(.identificationFailed(.conflictingEvidence)))
            finishCaptureAfterLinkDown()
            return
        }
        if isDetectingProtocol {
            guard promoteProtocolDetectionIfResolved(detectionResolution, on: characteristic.service?.peripheral) else {
                guard captureFrame(
                    direction: "notify",
                    characteristic: characteristic.uuid,
                    service: characteristic.service?.uuid,
                    bytes: value
                ) else { return }
                captureNotificationCount += 1
                publishCaptureProgress()
                if isDetectingProtocol, channel.bluetooth16Value == 0xffe1,
                   deviceDetectionSession.nextBegodeProbeExpiry(
                       timeout: BegodeProbeResponsePolicy.timeoutAfter
                   ) == nil
                {
                    finishProtocolDetectionOrRecord(
                        detectionResolution,
                        on: characteristic.service?.peripheral
                    )
                }
                return
            }
        }
        if isRecordOnly {
            guard captureFrame(
                direction: "notify",
                characteristic: characteristic.uuid,
                service: characteristic.service?.uuid,
                bytes: value
            ) else { return }
            record("record_only_notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            captureNotificationCount += 1
            publishCaptureProgress()
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
            guard captureFrame(
                direction: "notify",
                characteristic: characteristic.uuid,
                service: characteristic.service?.uuid,
                bytes: value,
                telemetry: step.actions.compactMap(\.rawTelemetry).last
            ) else { return }
            record("notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            captureNotificationCount += 1
            publishCaptureProgress()
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
        guard subscribedCharacteristics[channel] === characteristic else {
            record("notification_state_ignored=unbound_characteristic service=\(characteristic.service?.uuid.uuidString ?? "unknown") characteristic=\(characteristic.uuid.uuidString)")
            return
        }
        if let error {
            setPhase(.failed(.notificationFailed(error.sessionMessage)))
            return
        }
        if isDetectingProtocol, channel.bluetooth16Value == 0xffe1, characteristic.isNotifying {
            startEucProtocolDetection(on: characteristic.service?.peripheral)
            return
        }
        if isDetectingProtocol, channel == .vescNordicUartNotify, characteristic.isNotifying {
            identificationProbeTransport.vescNotificationsEnabled(at: clock.now(), using: self)
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
    func bindDiscoveredCharacteristic(_ channel: BluetoothUuid, _ characteristic: CBCharacteristic) {
        guard let existing = subscribedCharacteristics[channel] else {
            subscribedCharacteristics[channel] = characteristic
            return
        }
        guard preferredServiceUuid(for: selectedRoute) == characteristic.service?.uuid else {
            return
        }
        if existing.service?.uuid != characteristic.service?.uuid {
            subscribedCharacteristics[channel] = characteristic
        }
    }

    func preferredServiceUuid(for route: DevicePickerConnectionRoute?) -> CBUUID? {
        switch route {
        case .vescOnewheel:
            return BluetoothUuid.vescNordicUartService.coreBluetoothUuid
        case .electricUnicycle:
            return BluetoothUuid.bluetooth16(0xffe0).coreBluetoothUuid
        case nil:
            return nil
        }
    }

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
        guard characteristic.properties.contains(.notify) || characteristic.properties.contains(.indicate) else {
            setPhase(.failed(.missingNotifyChannel))
            return
        }
        if characteristic.isNotifying {
            // Protocol detection owns the initial subscription. The live owner
            // must adopt it without waiting for a second state-change callback.
            liveOwner?.handleNotificationStateUpdate(channel: channel, isNotifying: true, error: nil)
            return
        }
        peripheral?.setNotifyValue(true, for: characteristic)
    }

    public func writeWithoutResponse(channel: BluetoothUuid, bytes: Data, onReceipt: @escaping (CoreBluetoothWriteDisposition) -> Void) -> CoreBluetoothWriteDisposition {
        observeDetectionProbeWrite(channel: channel, bytes: bytes)
        guard let characteristic = subscribedCharacteristics[channel] else {
            setPhase(.failed(.missingWriteChannel))
            onReceipt(.rejected)
            return .rejected
        }
        guard characteristic.properties.contains(.writeWithoutResponse) else {
            setPhase(.failed(.missingWriteChannel))
            onReceipt(.rejected)
            return .rejected
        }
        guard let peripheral else {
            onReceipt(.rejected)
            return .rejected
        }
        guard captureFrame(
            direction: "write_without_response",
            characteristic: channel.coreBluetoothUuid,
            bytes: bytes
        ) else {
            onReceipt(.rejected)
            return .rejected
        }
        return pendingWithoutResponseWrites.submit(
            canSend: { [weak self] in
                self?.peripheral === peripheral && peripheral.canSendWriteWithoutResponse
            },
            write: { [weak self] in
                peripheral.writeValue(bytes, for: characteristic, type: .withoutResponse)
                self?.record("write_without_response=\(channel.coreBluetoothUuid.uuidString) bytes=\(bytes.count)")
            },
            onReceipt: onReceipt
        )
    }

    public func canSubmitWithoutResponse() -> Bool {
        peripheral?.canSendWriteWithoutResponse ?? false
    }

    private func flushPendingWithoutResponseWrites() {
        guard let peripheral else { return }
        pendingWithoutResponseWrites.flush { [weak self] in
            self?.peripheral === peripheral && peripheral.canSendWriteWithoutResponse
        }
    }

    public func peripheralIsReadyToSendWithoutResponse() {
        flushPendingWithoutResponseWrites()
    }

    public func clearPendingWithoutResponseWrites() {
        pendingWithoutResponseWrites.clear()
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
        connectionGattInventory.removeAll { $0.service == serviceUuid.bytes }
        for characteristic in service.characteristics ?? [] {
            guard let characteristicUuid = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) else {
                continue
            }
            let fingerprint = MobileGattFingerprintDto(
                service: serviceUuid.bytes,
                characteristic: characteristicUuid.bytes,
                roles: characteristic.mobileGattRoles,
                verification: .hardwareVerified
            )
            connectionGattInventory.append(fingerprint)
            if let builder = captureBuilder {
                guard acceptCaptureWrite(builder.addGattFingerprint(fingerprint: fingerprint)) else { return }
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
        let current: DeviceDetectionResolution
        if let token = connectionAttempt?.token,
           let resolution = rustSessionState.observeConnectionNotification(token: token, bytes: bytes) {
            current = DeviceDetectionResolution(resolution)
        } else if connectionAttempt == nil {
            // Standalone decoder fixtures have no platform connection attempt.
            current = identificationProbeTransport.observeNotification(channel: channel, bytes: bytes)
        } else {
            return previous
        }
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
            scheduleBegodeProbeExpiry()
            if !expired.isEmpty, begodeProbeExpiryWorkItem == nil, let peripheral {
                finishProtocolDetectionOrRecord(deviceDetectionSession.resolution, on: peripheral)
            }
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

    private func promoteProtocolDetectionIfResolved(
        _ resolution: DeviceDetectionResolution,
        on peripheral: CBPeripheral?,
        allowClosestMatch: Bool = false
    ) -> Bool {
        guard isDetectingProtocol, let peripheral, let advertisement else {
            return false
        }
        guard let token = connectionAttempt?.token else { return false }
        let resolved = rustSessionState.resolveDeviceSession(
            token: token,
            identificationComplete: allowClosestMatch,
            nowMs: clock.now().rawValue
        )
        switch resolved.connection.readiness {
        case .recordOnly:
            recordUnresolvedProtocolDetection(.unsupported, on: peripheral)
            return false
        case .verified:
            connectionDeadlineWorkItem?.cancel()
            publishConnectionSnapshot()
        case .pending, .disconnected, .failed, .conflicted:
            return false
        }
        let candidate = rustSessionState.connectionAdmissionCandidate(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            displayName: advertisement.localName
                ?? protocolIdentityFallbackDisplayName(protocolFamily: resolution.protocolFamily),
            allowClosestMatch: allowClosestMatch
        )
        switch DevicePickerCandidateSupport(candidate) {
        case .supported(let route, let model):
            guard let route else { return false }
            isDetectingProtocol = false
            clearProtocolDetectionExpiry()
            selectedRoute = route
            selectedModel = model
            annotateDetection("protocol_detection_resolved=\(route.rawValue)")
            buildOwner(for: peripheral)
            return liveOwner != nil
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise, .manualEntry, .unsupported:
            let failure: IdentificationProbeFailure = candidate.support == .conflicting
                ? .conflictingEvidence
                : (candidate.support == .unknownRecordable ? .unsupported : .unsupported)
            guard allowClosestMatch else { return false }
            recordUnresolvedProtocolDetection(failure, on: peripheral)
            return false
        }
    }

    private func finishProtocolDetectionOrRecord(
        _ resolution: DeviceDetectionResolution,
        on peripheral: CBPeripheral?
    ) {
        // Transport setup and unanswered probes are not protocol identity.
        // Leave this attempt pending so a valid frame can promote it; the
        // connection deadline owns the capture-only fallback if no protocol
        // evidence ever arrives.
        guard isDetectingProtocol else { return }
        switch ProtocolDetectionFinishDecision(resolution: resolution) {
        case .awaitPassiveEvidence:
            return
        case .evaluateResolvedEvidence:
            break
        }

        guard !promoteProtocolDetectionIfResolved(
            resolution,
            on: peripheral,
            allowClosestMatch: true
        ) else {
            return
        }

        recordUnresolvedProtocolDetection(.unsupported, on: peripheral)
    }

    private func beginProtocolDetection(on peripheral: CBPeripheral) {
        setPhase(.subscribing)
        var subscribed = false
        if let characteristic = subscribedCharacteristics[.bluetooth16(0xffe1)] {
            if characteristic.isNotifying {
                startEucProtocolDetection(on: peripheral)
            } else {
                identificationProbeTransport.subscribe(using: self)
            }
            subscribed = true
        }
        if let characteristic = subscribedCharacteristics[.vescNordicUartNotify] {
            scheduleProtocolDetectionExpiry(on: peripheral)
            if characteristic.isNotifying {
                identificationProbeTransport.vescNotificationsEnabled(at: clock.now(), using: self)
            } else {
                identificationProbeTransport.subscribeVesc(using: self)
            }
            subscribed = true
        }
        if !subscribed {
            recordUnresolvedProtocolDetection(.unsupported, on: peripheral)
        }
    }

    private func startEucProtocolDetection(on peripheral: CBPeripheral?) {
        switch identificationProbeTransport.notificationsEnabled(at: clock.now(), using: self) {
        case .noProbeNeeded:
            if !promoteProtocolDetectionIfResolved(deviceDetectionSession.resolution, on: peripheral) {
                finishProtocolDetectionOrRecord(deviceDetectionSession.resolution, on: peripheral)
            }
        case .unsupported:
            finishProtocolDetectionOrRecord(deviceDetectionSession.resolution, on: peripheral)
        case .writes, .alreadyPending:
            break
        }
    }

    func recordUnresolvedProtocolDetection(
        _ failure: IdentificationProbeFailure,
        on peripheral: CBPeripheral?
    ) {
        onBleQueue {
            if rustSessionState.shouldRetryIdentification() {
                annotateDetection("protocol_detection_retry=\(failure)")
                recoverConnection(after: .identificationFailed(failure)) {
                    if let peripheral { self.central?.cancelPeripheralConnection(peripheral) }
                }
                return
            }
            enterUnresolvedCapture(failure, on: peripheral)
        }
    }

    private func enterUnresolvedCapture(
        _ failure: IdentificationProbeFailure,
        on peripheral: CBPeripheral?
    ) {
        if let token = connectionSnapshot.token, connectionSnapshot.readiness == .pending {
            _ = rustSessionState.connectionTransportFailed(token: token)
        }
        connectionDeadlineWorkItem?.cancel()
        publishConnectionSnapshot()
        isDetectingProtocol = false
        isRecordOnly = true
        clearProtocolDetectionExpiry()
        clearPendingBegodeProbeResponses()
        selectedRoute = nil
        selectedModel = nil
        liveOwner = nil
        annotateDetection("protocol_detection_unresolved=\(failure)")
        if let peripheral {
            peripheral.services?.forEach {
                subscribeRecordOnlyCharacteristics($0.characteristics ?? [], on: peripheral)
            }
        }
        setPhase(.live)
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
        let generation = connectionSnapshot.generation
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.connectionSnapshot.generation == generation else { return }
            self.expireOutstandingBegodeProbeResponses()
        }
        begodeProbeExpiryWorkItem = work
        bleQueue.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: delay)),
            execute: work
        )
    }

    private func scheduleProtocolDetectionExpiry(on peripheral: CBPeripheral) {
        protocolDetectionExpiryWorkItem?.cancel()
        let delay = ProtocolDetectionResponsePolicy.timeoutAfter.rawValue
        let generation = connectionSnapshot.generation
        let work = DispatchWorkItem { [weak self, weak peripheral] in
            guard let self, self.connectionSnapshot.generation == generation,
                  self.isDetectingProtocol else { return }
            self.finishProtocolDetectionOrRecord(self.deviceDetectionSession.resolution, on: peripheral)
        }
        protocolDetectionExpiryWorkItem = work
        bleQueue.asyncAfter(
            deadline: .now() + .milliseconds(Int(clamping: delay)),
            execute: work
        )
    }

    private func clearProtocolDetectionExpiry() {
        protocolDetectionExpiryWorkItem?.cancel()
        protocolDetectionExpiryWorkItem = nil
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
    public func updateRideLocationDemand(for state: MobileRideMapStateDto) {
        publishOnMain {
            self.locationUpdatesDemanded = state == .active
            self.updateLocationManagerDemand()
        }
    }

    private func updateLocationManagerDemand() {
#if os(iOS)
        locationManager.allowsBackgroundLocationUpdates = locationUpdatesDemanded
#endif

        guard locationUpdatesDemanded else {
            if locationManagerUpdatesStarted {
                locationManager.stopUpdatingLocation()
                locationManagerUpdatesStarted = false
            }
            return
        }

        guard CLLocationManager.locationServicesEnabled() else {
            publishRideMapAvailability()
            return
        }

        switch locationManager.authorizationStatus {
        case .notDetermined:
            locationManager.requestWhenInUseAuthorization()
        case .authorizedAlways:
            locationManager.startUpdatingLocation()
            locationManagerUpdatesStarted = true
        case .authorizedWhenInUse:
            locationManager.startUpdatingLocation()
            locationManagerUpdatesStarted = true
        case .denied, .restricted:
            publishRideMapAvailability()
        @unknown default:
            publishRideMapAvailability()
        }
    }

    public func locationManagerDidChangeAuthorization(_: CLLocationManager) {
        // Authorization changes are presentation state even when no ride currently demands
        // updates. Publish first so granting permission clears a stale warning immediately.
        publishRideMapAvailability()
        updateLocationManagerDemand()
    }

    public func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard !locations.isEmpty else { return }

        let receiptMonotonicMs = clock.now().rawValue
        let receiptWallClock = wallClock()
        let samples = locations.compactMap(MobilePhoneLocationSampleDto.init(location:))
        guard !samples.isEmpty else { return }

        for sample in samples {
            phoneLocationSnapshot = phoneLocationState.ingest(sample: sample)
        }
        publishPhoneLocationSnapshot()

        guard let rideMapState,
              let receiptWallClockUnixMs = unixMilliseconds(for: receiptWallClock)
        else { return }

        let recordingToken = rideMapState
            .currentSnapshot(atMs: receiptMonotonicMs)?
            .recordingToken
        let errorContext = MobileRideMapErrorContext(recordingToken: recordingToken)

        let queue = rideMapQueue
        let reference = WeakCutoutSessionCoreReference(self)
        queue.async {
            guard let self = reference.value else { return }
            do {
                let decisions = try rideMapState.ingestLocationBatch(
                    recordingToken: recordingToken,
                    receiptMonotonicMs: receiptMonotonicMs,
                    receiptWallClockUnixMs: receiptWallClockUnixMs,
                    samples: samples
                )
                self.publishRideMapDecisions(decisions)
            } catch let error as MobileRideMapError {
                self.publishRideMapError(error, context: errorContext)
                self.recordRideMapDiagnostic("ride_map_ingest_error=\(error)")
            } catch {
                self.recordRideMapDiagnostic("ride_map_ingest_error=\(error)")
            }
        }
    }

    private func onRideMapQueue<T>(_ operation: () throws -> T) rethrows -> T {
        if DispatchQueue.getSpecific(key: rideMapQueueKey) != nil {
            return try operation()
        }
        return try rideMapQueue.sync(execute: operation)
    }

    private func requireRideMapStateForCommand() throws -> MobileRideMapState {
        guard let rideMapState else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        return rideMapState
    }

    public func startRideMapGpsOnly(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try onRideMapQueue {
            try requireRideMapStateForCommand().startGpsOnly(atMs: atMs)
        }
        synchronizeRideMapLocationDemand()
        return snapshot
    }

    public func pauseRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try onRideMapQueue { try requireRideMapStateForCommand().pause(atMs: atMs) }
        synchronizeRideMapLocationDemand()
        return snapshot
    }

    public func resumeRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try onRideMapQueue { try requireRideMapStateForCommand().resume(atMs: atMs) }
        synchronizeRideMapLocationDemand()
        return snapshot
    }

    public func stopRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try onRideMapQueue { try requireRideMapStateForCommand().stop(atMs: atMs) }
        synchronizeRideMapLocationDemand()
        return snapshot
    }

    public func saveRideMap() throws -> MobileRideMapSnapshotDto {
        let snapshot = try onRideMapQueue { try requireRideMapStateForCommand().save() }
        synchronizeRideMapLocationDemand()
        return snapshot
    }

    public func discardRideMap() throws -> MobileRideMapSnapshotDto {
        let snapshot = try onRideMapQueue { try requireRideMapStateForCommand().discard() }
        synchronizeRideMapLocationDemand()
        return snapshot
    }

    /// Clears the Rust-owned location context before starting a new capture.
    public func resetRideMapLocationAdmission() {
        phoneLocationState.clear()
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

private func unixMilliseconds(for date: Date) -> UInt64? {
    let milliseconds = date.timeIntervalSince1970 * 1_000
    guard milliseconds.isFinite, milliseconds >= 0, milliseconds < Double(UInt64.max) else { return nil }
    return UInt64(milliseconds.rounded(.down))
}

func bmsStorageSamples(
    observations: [BmsRawVoltageObservation],
    wallClockMilliseconds: UInt64,
    sessionIdentifier: String
) -> [MobileStoredBmsVoltageSampleDto] {
    observations.map { observation in
        MobileStoredBmsVoltageSampleDto(
            sessionIdentifier: sessionIdentifier,
            eventSequence: observation.eventSequence,
            monotonicMilliseconds: observation.observedAtMilliseconds,
            wallClockMilliseconds: wallClockMilliseconds,
            observationIndex: observation.observationIndex,
            packIndex: observation.packIndex,
            packObservationIndex: observation.packObservationIndex,
            voltage: observation.voltage
        )
    }
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
