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

#if DEBUG
public enum CutoutSessionTestInitialBluetoothState: Sendable {
    case scanning
    case unavailable
    case permissionDenied
}

public struct CutoutSessionTestScript {
    public let candidate: DevicePickerDiscoveryCandidate
    public let telemetry: TelemetrySnapshot?
    public let bmsSnapshot: BmsSnapshot?
    public let startsLive: Bool
    public let initialBluetoothState: CutoutSessionTestInitialBluetoothState
    public let failsConnection: Bool
    public let emitsLateLiveAfterFailure: Bool
    public let reconnectsAfterFirstLive: Bool
    public let reconnectAfterLiveMilliseconds: UInt64
    public let reconnectDelayMilliseconds: UInt64
    public let emitsStaleTelemetry: Bool
    public let flushCaptureSucceeds: Bool
    public let connectionDelayMilliseconds: UInt64

    public init(
        candidate: DevicePickerDiscoveryCandidate,
        telemetry: TelemetrySnapshot?,
        bmsSnapshot: BmsSnapshot? = nil,
        startsLive: Bool = false,
        initialBluetoothState: CutoutSessionTestInitialBluetoothState = .scanning,
        failsConnection: Bool = false,
        emitsLateLiveAfterFailure: Bool = false,
        reconnectsAfterFirstLive: Bool = false,
        reconnectAfterLiveMilliseconds: UInt64 = 0,
        reconnectDelayMilliseconds: UInt64 = 1_000,
        emitsStaleTelemetry: Bool = false,
        flushCaptureSucceeds: Bool = true,
        connectionDelayMilliseconds: UInt64 = 1_000
    ) {
        self.candidate = candidate
        self.telemetry = telemetry
        self.bmsSnapshot = bmsSnapshot
        self.startsLive = startsLive
        self.initialBluetoothState = initialBluetoothState
        self.failsConnection = failsConnection
        self.emitsLateLiveAfterFailure = emitsLateLiveAfterFailure
        self.reconnectsAfterFirstLive = reconnectsAfterFirstLive
        self.reconnectAfterLiveMilliseconds = reconnectAfterLiveMilliseconds
        self.reconnectDelayMilliseconds = reconnectDelayMilliseconds
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

public final class CutoutSessionCore: NSObject {
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
    public var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?

    private let clock: MonotonicClock
    private var diagnosticLog = BoundedDiagnosticLog(capacity: 2_048)
    private let bleQueue = DispatchQueue(label: "io.cutout.corebluetooth", qos: .userInitiated)
    private let bleQueueKey = DispatchSpecificKey<Void>()
    private let rustSessionState: CutoutSessionStateHandle
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var advertisement: CoreBluetoothAdvertisement?
    private var discoveredPeripherals: [CoreBluetoothPeripheralIdentifier: CBPeripheral] = [:]
    private var liveOwner: CoreBluetoothLiveSessionOwner?
    private var selectedModel: ElectricUnicycleModel?
    private var selectedRoute: DevicePickerConnectionRoute?
    private var chargeEstimateProfile: ChargeEstimateProfile?
    private var vescBoardProfile: VescBoardProfile?
    private var isRecordOnly = false
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
    private var begodeProbeExpiryWorkItem: DispatchWorkItem?
    private var pendingDisplayState: RideDisplayState?
    private var pendingDisplayStateQueuedAt: MonotonicMilliseconds?
    private var displayPublishWorkItem: DispatchWorkItem?
    private var lastDisplayPublication: MonotonicMilliseconds?
    private let phoneLocationState = MobilePhoneLocationState()
    private var didRequestAlwaysLocationAuthorization = false
#if DEBUG
    private let testScript: CutoutSessionTestScript?
    private var testScriptWorkItem: DispatchWorkItem?
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

#if DEBUG
    public convenience init(testScript: CutoutSessionTestScript) {
        self.init(clock: MonotonicClock(), testScript: testScript)
    }

    init(
        clock: MonotonicClock,
        testScript: CutoutSessionTestScript? = nil,
        reconnectScheduler: any ConnectionReconnectScheduling = MainQueueReconnectScheduler(),
        reconnectJitter: @escaping () -> Double = { Double.random(in: 0...1) }
    ) {
        let rustSessionState = CutoutSessionStateHandle()
        self.rustSessionState = rustSessionState
        self.deviceDetectionSession = DeviceDetectionSession(sessionState: rustSessionState)
        self.clock = clock
        self.testScript = testScript
        self.reconnectController = ConnectionReconnectController(scheduler: reconnectScheduler)
        self.reconnectJitter = reconnectJitter
        super.init()
        bleQueue.setSpecific(key: bleQueueKey, value: ())
    }
#else
    init(clock: MonotonicClock) {
        let rustSessionState = CutoutSessionStateHandle()
        self.rustSessionState = rustSessionState
        self.deviceDetectionSession = DeviceDetectionSession(sessionState: rustSessionState)
        self.clock = clock
        self.reconnectController = ConnectionReconnectController(scheduler: MainQueueReconnectScheduler())
        self.reconnectJitter = { Double.random(in: 0...1) }
        super.init()
        bleQueue.setSpecific(key: bleQueueKey, value: ())
    }
#endif

    public func start() {
#if DEBUG
        if let testScript {
            start(testScript: testScript)
            return
        }
#endif
        startLocationUpdates()
        return onBleQueue {
            guard central == nil else {
                return
            }
            central = CBCentralManager(delegate: self, queue: bleQueue)
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
        pair(platformIdentifier: platformIdentifier, model: .falcon)
    }

    @discardableResult
    public func recordOnly(platformIdentifier: String, note: String? = nil, annotations: [String] = []) -> Bool {
#if DEBUG
        if let testScript {
            guard platformIdentifier == testScript.candidate.platformIdentifier else { return false }
            let fileURL = URL(fileURLWithPath: "/tmp/ui-test.capture")
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

    public func flushCapture() -> Bool {
#if DEBUG
        if let testScript {
            return testScript.flushCaptureSucceeds
        }
#endif
        return onBleQueue {
            captureBuilder?.flushWriter() ?? false
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
        testScriptWorkItem?.cancel()
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
        scheduleTestReconnectIfNeeded(testScript)
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
#endif

    private func disconnectAndScanOnBleQueue() {
#if DEBUG
        testScriptWorkItem?.cancel()
        testScriptWorkItem = nil
#endif
        suppressReconnect = true
        cancelPendingReconnect()
        finishCaptureAfterLinkDown()
        isRecordOnly = false
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

    private func connectVescOnewheel(to peripheral: CBPeripheral, using advertisement: CoreBluetoothAdvertisement) {
        cancelPendingReconnect()
        suppressReconnect = false
        isRecordOnly = false
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
#endif
        markOutstandingBegodeProbeResponsesMissing()
        finishCaptureAfterLinkDown()
        let wasRecordOnly = isRecordOnly
        let reconnectRoute = selectedRoute
        isRecordOnly = false
        selectedRoute = nil
        liveOwner = nil
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()

        guard !suppressReconnect else {
            suppressReconnect = false
            return
        }

        guard !wasRecordOnly else {
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
        guard elapsed >= intervalMilliseconds else {
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
            let location = phoneLocationState.currentSnapshot().latestSample
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

    private func startCapture(reason: String, annotations extraAnnotations: [String] = []) {
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
            "capture_evidence=hardware_tested",
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

extension CutoutSessionCore: CBCentralManagerDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        handleCentralState(central.state) {
            central.scanForPeripherals(withServices: nil)
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
        setPhase(.discoveringServices)
        peripheral.delegate = self
        if isRecordOnly {
            _ = captureBuilder?.recordLinkUp(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                maxWriteLen: MobileTransportWriteLimitDto(bytes: UInt16(clamping: peripheral.maximumWriteValueLength(for: .withoutResponse)))
            )
        }
        peripheral.discoverServices(discoveryServiceUuidsForSelectedRoute)
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
        observeDetectionNotification(channel: channel, bytes: value)
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

    func observeDetectionNotification(channel: BluetoothUuid, bytes: Data) {
        guard channel.bluetooth16Value == 0xffe1 else {
            return
        }
        let previous = deviceDetectionSession.resolution
        let current = deviceDetectionSession.observeNotification(bytes: bytes)
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
            return
        }
        guard let malformedProbeResponse = current.malformedProbeResponse else {
            return
        }
        switch malformedProbeResponse {
        case .begodeName:
            annotateDetection("begode_probe_malformed=model")
        case .begodeFirmware:
            annotateDetection("begode_probe_malformed=firmware")
        case .begodeImu:
            annotateDetection("begode_probe_malformed=imu")
        }
    }

    func expireOutstandingBegodeProbeResponses() {
        onBleQueue {
            let expired = deviceDetectionSession.expireBegodeProbeResponses(
                at: clock.now(),
                timeout: BegodeProbeResponsePolicy.timeoutAfter
            )
            publishMissingBegodeProbeResponses(expired)
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
    private func requestAlwaysLocationAuthorizationIfNeeded() {
        guard !didRequestAlwaysLocationAuthorization else { return }
        didRequestAlwaysLocationAuthorization = true
        locationManager.requestAlwaysAuthorization()
    }

    private func startLocationUpdates() {
        guard CLLocationManager.locationServicesEnabled() else { return }
        switch locationManager.authorizationStatus {
        case .notDetermined:
            locationManager.requestWhenInUseAuthorization()
        case .authorizedAlways:
            locationManager.startUpdatingLocation()
        case .authorizedWhenInUse:
            requestAlwaysLocationAuthorizationIfNeeded()
            locationManager.startUpdatingLocation()
        case .denied, .restricted:
            break
        @unknown default:
            break
        }
    }

    public func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
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

    public func locationManager(_: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let location = locations.last else { return }
        phoneLocationSnapshot = phoneLocationState.ingest(sample: MobilePhoneLocationSampleDto(
            wallClockUnixMs: UInt64(max(0, location.timestamp.timeIntervalSince1970 * 1_000)),
            latitudeDegrees: location.coordinate.latitude,
            longitudeDegrees: location.coordinate.longitude,
            altitudeMeters: location.altitude,
            horizontalAccuracyMeters: location.horizontalAccuracy,
            verticalAccuracyMeters: location.verticalAccuracy,
            speedMetersPerSecond: location.speed,
            speedAccuracyMetersPerSecond: location.speedAccuracy,
            courseDegrees: location.course,
            courseAccuracyDegrees: location.courseAccuracy
        ))
        publishPhoneLocationSnapshot()
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
