import CoreBluetooth
import CoreLocation
import CutoutMobileFFI
import Foundation
import Synchronization

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

private struct AcceptedLiveNotificationIngress {
    let channel: BluetoothUuid
    let bytes: Data
    let receivedAt: MonotonicMilliseconds
}

enum CoreBluetoothCallbackDisposition {
    case ignored
    case failed(Error)
    case accepted
}

func coreBluetoothCallbackDisposition<Callback: AnyObject>(
    subscribed: Callback?,
    callback: Callback,
    error: Error?
) -> CoreBluetoothCallbackDisposition {
    guard subscribed === callback else {
        return .ignored
    }
    if let error {
        return .failed(error)
    }
    return .accepted
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

    public var dto: MobileCaptureGenerationDto { .init(value: rawValue) }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

public enum CaptureEvent: Equatable, Sendable {
    /// Immutable lifecycle publication from the shared Rust session owner.
    case lifecycle(MobileCaptureLifecycleSnapshotDto)
    case started(generation: CaptureGeneration, fileURL: URL)
    case notificationRecorded(generation: CaptureGeneration)
    case progress(generation: CaptureGeneration, CaptureProgress)
    case finished(generation: CaptureGeneration, fileURL: URL)
    case databaseFinished(generation: CaptureGeneration, outcome: MobileCaptureFinishOutcomeDto)
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
        guard
            let linkActions = try? session.linkUp(
                at: now,
                writeLimit: TransportWriteLimitBytes(512)
            )
        else {
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
    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void)
        -> any ConnectionReconnectCancellable
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
        guard let delayMilliseconds = ConnectionReconnectPolicy.delayMilliseconds(attempt: attempt, jitter: jitter)
        else {
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
    func schedule(after delayMilliseconds: UInt64, operation: @escaping () -> Void)
        -> any ConnectionReconnectCancellable
    {
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
        /// Optional notifications used only to establish protocol identity before
        /// the live notification replay begins.
        public let protocolDetectionNotifications: [Data]?
        /// Applies the decoded notification steps to the production display path.
        /// Most UI fixtures provide a separate presentation snapshot; this opt-in
        /// keeps those fixtures deterministic while allowing raw replay tests to
        /// prove the real parser-to-dashboard boundary.
        public let appliesProtocolNotificationSteps: Bool
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
            protocolDetectionNotifications: [Data]? = nil,
            appliesProtocolNotificationSteps: Bool = false,
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
            self.protocolDetectionNotifications = protocolDetectionNotifications
            self.appliesProtocolNotificationSteps = appliesProtocolNotificationSteps
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

        func writeWithoutResponse(
            channel: BluetoothUuid, bytes: Data, isCurrent: @escaping () -> Bool,
            onReceipt: @escaping (CoreBluetoothWriteDisposition) -> Void
        ) -> CoreBluetoothWriteDisposition {
            guard isCurrent() else {
                onReceipt(.cancelled)
                return .cancelled
            }
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
    public var rideMapStateHandle: MobileRideMapState? { rideMapStateForInitialization }
    public private(set) var displayState = RideDisplayState()
    public private(set) var phase = SessionConnectionPhase.starting
    public var records: [String] { diagnosticLog.values }
    public var droppedRecordCount: Int { diagnosticLog.droppedCount }
    public private(set) var hasObservedSpeedSnapshot = false
    private var storedScanState = DevicePickerScanState(status: .idle, rows: [])
    public var scanState: DevicePickerScanState {
        onBleQueue { storedScanState }
    }
    public private(set) var faultHistoryReadback: FaultHistoryReadback?
    public private(set) var bmsSnapshot: BmsSnapshot?
    @MainActor public private(set) var phoneLocationSnapshot = MobilePhoneLocationSnapshotDto(
        latestSample: nil, gpsSpeed: nil)
    @MainActor private var rideMapStorageStatus: (error: MobileRideMapError?, isReady: Bool) = (nil, false)
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
    public var settings: DeviceSettings {
        rustSessionState.settings()
    }

    #if DEBUG
        var musicCaptureObservationForTesting: MobilePevcapMusicEventDto? {
            onBleQueue { captureRecorder.currentMusicObservation }
        }
    #endif

    public var onDisplayStateChange: ((RideDisplayState) -> Void)?
    public var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    public var onConnectionSnapshotChange: ((ConnectionSnapshot) -> Void)?
    public var onReconnectScheduled: ((SessionConnectionRetry) -> Void)?
    public var onRecord: ((String) -> Void)?
    public var onCaptureEvent: ((CaptureEvent) -> Void)?
    public var onScanStateChange: ((DevicePickerScanState) -> Void)?
    public var onSettingsChange: ((DeviceSettings) -> Void)?
    public var onPhoneAlarmActionsAvailable: ((MobilePhoneAlarmActionsDto) -> Void)?
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
    private let wallClock: @Sendable () -> Date
    private var diagnosticLog = BoundedDiagnosticLog(capacity: 2_048)
    private let bleQueue = DispatchQueue(label: "io.cutout.corebluetooth")
    private let bleQueueKey = DispatchSpecificKey<Void>()
    private let rustSessionState: CutoutSessionStateHandle
    private let databaseForCapturePublication: RideDatabaseHandle?
    private let selectedDeviceStore: DevicePickerSelectionStore
    private let injectedNotificationEffects: CutoutSessionNotificationEffects?
    private let injectedCaptureRecorder: (any CutoutSessionCaptureRecording)?
    private let injectedDisplayPublisher: (any CutoutSessionDisplayPublishing)?
    private let injectedPhoneLocationAdapter: (any CutoutSessionPhoneLocationAdapting)?
    private let injectedRideMapRecorder: (any CutoutSessionRideMapRecording)?
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
    private lazy var bluetoothWriteAdapter = CutoutSessionBluetoothWriteAdapter(
        makeCaptureReceipt: { [weak self] channel, bytes, writeID in
            guard let self else { return { _ in true } }
            let recordReceipt = self.captureRecorder.makeWriteReceiptRecorder(
                channel: channel,
                bytes: bytes,
                writeID: writeID
            )
            return { [weak self] disposition in
                guard let self else { return true }
                return self.acceptCaptureWrite(recordReceipt(disposition))
            }
        },
        recordWrite: { [weak self] channel, bytes in
            self?.record("write_without_response=\(channel.coreBluetoothUuid.uuidString) bytes=\(bytes.count)")
        }
    )
    private var pendingServiceDiscoveries = Set<CBUUID>()
    private var connectionGattInventory: [MobileGattFingerprintDto] = []
    private var suppressReconnect = false
    private var isIngestingLiveNotification = false
    private var deferredCaptureFailureMessage: String?
    private let reconnectController: ConnectionReconnectController
    private let reconnectJitter: () -> Double
    private lazy var captureRecorder: any CutoutSessionCaptureRecording =
        injectedCaptureRecorder
        ?? CutoutSessionCaptureRecorder(
            clock: clock,
            wallClock: wallClock,
            database: databaseForCapturePublication,
            publish: { [weak self] event in self?.publishCaptureEvent(event) },
            onWriterCompletion: { [weak self] completion in
                guard let self else { return }
                self.onBleQueue { self.handleCaptureWriterCompletion(completion) }
            }
        )
    private var captureGeneration: CaptureGeneration? { captureRecorder.currentGeneration }
    private var captureProgressTimer: DispatchSourceTimer?
    #if DEBUG
        var captureDirectoryForTesting: URL?
        var captureFinishWriterGate: (() -> Void)? {
            get { captureRecorder.finishWriterGate }
            set { captureRecorder.finishWriterGate = newValue }
        }
    #endif
    private var bmsStorageSessionIdentifier = UUID().uuidString
    private let deviceDetectionSession: DeviceDetectionSession
    private let identificationProbeTransport: IdentificationProbeTransportCoordinator
    private var begodeProbeExpiryWorkItem: DispatchWorkItem?
    private var protocolDetectionExpiryWorkItem: DispatchWorkItem?
    private lazy var displayPublisher: any CutoutSessionDisplayPublishing =
        injectedDisplayPublisher
        ?? CutoutSessionDisplayPublisher(
            clock: clock,
            onDisplayStateChange: { [weak self] value in
                self?.onDisplayStateChange?(value)
            },
            onRecord: { [weak self] message in
                self?.onRecord?(message)
            }
        )
    private lazy var notificationEffects: CutoutSessionNotificationEffects =
        injectedNotificationEffects
        ?? CutoutSessionNotificationEffects(
            applyActions: { [weak self] actions in
                actions.forEach { self?.applySessionAction($0) }
            },
            observeRideMapConnection: { [weak self] receivedAt in
                self?.observeRideMapConnection(at: receivedAt)
            },
            persistBmsSamples: { [weak self] observations in
                self?.persistBmsSamples(observations)
            },
            reduceDisplayState: { state, snapshot, receivedAt, updateKind in
                switch updateKind {
                case .linkUp:
                    state.reducingLinkUpSnapshot(snapshot, receivedAt: receivedAt)
                case .notification:
                    state.reducing(snapshot: snapshot, receivedAt: receivedAt)
                }
            }
        )
    private let rideMapStateForInitialization: MobileRideMapState?
    private lazy var rideMapRecorder: any CutoutSessionRideMapRecording = {
        guard let injectedRideMapRecorder else {
            let reference = WeakCutoutSessionCoreReference(self)
            return CutoutSessionRideMapRecorder(
                state: rideMapStateForInitialization,
                clock: clock,
                wallClock: wallClock,
                publishSnapshot: { snapshot in
                    reference.value?.publishOnMain {
                        MainActor.assumeIsolated {
                            reference.value?.publishRideMapSnapshot(snapshot)
                        }
                    }
                },
                publishDecisions: { batch in
                    reference.value?.publishOnMain {
                        MainActor.assumeIsolated {
                            reference.value?.publishRideMapDecisions(batch)
                        }
                    }
                },
                publishError: { error, context in
                    reference.value?.publishOnMain {
                        MainActor.assumeIsolated {
                            reference.value?.publishRideMapError(error, context: context)
                        }
                    }
                },
                publishAvailability: { initializationError, isReady in
                    reference.value?.publishRideMapAvailability(
                        initializationError: initializationError,
                        isReady: isReady
                    )
                },
                recordDiagnostic: { message in
                    reference.value?.recordRideMapDiagnostic(message)
                },
                onLocationDemand: { active in
                    Task { @MainActor in
                        reference.value?.phoneLocationAdapter.updateDemand(active)
                    }
                }
            )
        }
        return injectedRideMapRecorder
    }()
    @MainActor private lazy var phoneLocationAdapter: any CutoutSessionPhoneLocationAdapting =
        injectedPhoneLocationAdapter
        ?? CutoutSessionPhoneLocationAdapter(
            clock: clock,
            wallClock: wallClock,
            onSnapshot: { [reference = WeakCutoutSessionCoreReference(self)] snapshot, receivedAt in
                guard let self = reference.value else { return }
                self.phoneLocationSnapshot = snapshot
                self.publishPhoneLocationSnapshot(receivedAt: receivedAt)
            },
            onLocationUpdate: { [reference = WeakCutoutSessionCoreReference(self)] update in
                reference.value?.handlePhoneLocationUpdate(update)
            },
            onAvailabilityChange: { [reference = WeakCutoutSessionCoreReference(self)] in
                reference.value?.publishRideMapAvailabilityOnMain()
            }
        )
    @MainActor private lazy var rideMapPresentation = CutoutSessionRideMapPresentation(
        onSnapshot: { [weak self] snapshot in
            self?.publishOnMain { self?.onRideMapSnapshotChange?(snapshot) }
        },
        onDecision: { [weak self] snapshot, decision in
            self?.publishOnMain { self?.onRideMapDecisionChange?(snapshot, decision) }
        },
        onError: { [weak self] event in
            self?.publishOnMain { self?.onRideMapErrorChange?(event) }
        },
        onAvailability: { [weak self] availability in
            self?.publishOnMain { self?.onRideMapAvailabilityChange?(availability) }
        },
        onLocationDemand: { [weak self] state in
            self?.updateRideLocationDemand(for: state)
        }
    )
    private var didResolveBluetoothRestoration = false
    #if DEBUG
        private let testScript: CutoutSessionTestScript?
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
        captureProgressTimer?.cancel()
    }

    /// Composes transport around a database handle opened off the main actor.
    public convenience init(rideMapState: MobileRideMapState) {
        self.init(
            clock: MonotonicClock(),
            rideMapState: rideMapState,
            database: RustPersistenceStore.shared
        )
    }

    public override convenience init() {
        let rideMapState =
            RustPersistenceStore.shared.map(MobileRideMapState.init(database:))
            ?? MobileRideMapState(storageUnavailable: "Rust ride database is unavailable")
        self.init(
            clock: MonotonicClock(),
            rideMapState: rideMapState,
            database: RustPersistenceStore.shared
        )
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
            wallClock: @escaping @Sendable () -> Date = { Date() },
            rideMapState: MobileRideMapState? = nil,
            database: RideDatabaseHandle? = nil,
            notificationEffects: CutoutSessionNotificationEffects? = nil,
            captureRecorder: (any CutoutSessionCaptureRecording)? = nil,
            displayPublisher: (any CutoutSessionDisplayPublishing)? = nil,
            phoneLocationAdapter: (any CutoutSessionPhoneLocationAdapting)? = nil,
            rideMapRecorder: (any CutoutSessionRideMapRecording)? = nil
        ) {
            let rustSessionState =
                database.map {
                    CutoutSessionStateHandle.withDatabase(database: $0)
                } ?? CutoutSessionStateHandle()
            self.rustSessionState = rustSessionState
            self.databaseForCapturePublication = database
            let deviceDetectionSession = DeviceDetectionSession(sessionState: rustSessionState)
            self.deviceDetectionSession = deviceDetectionSession
            self.identificationProbeTransport = IdentificationProbeTransportCoordinator(
                detectionSession: deviceDetectionSession
            )
            self.clock = clock
            self.wallClock = wallClock
            self.rideMapStateForInitialization = rideMapState
            self.testScript = testScript
            self.captureDirectoryForTesting = testScript.map { _ in FileManager.default.temporaryDirectory }
            self.reconnectController = ConnectionReconnectController(scheduler: reconnectScheduler)
            self.reconnectJitter = reconnectJitter
            self.selectedDeviceStore = selectedDeviceStore
            self.injectedNotificationEffects = notificationEffects
            self.injectedCaptureRecorder = captureRecorder
            self.injectedDisplayPublisher = displayPublisher
            self.injectedPhoneLocationAdapter = phoneLocationAdapter
            self.injectedRideMapRecorder = rideMapRecorder
            super.init()
            bleQueue.setSpecific(key: bleQueueKey, value: ())
        }
    #else
        init(
            clock: MonotonicClock,
            selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
            wallClock: @escaping @Sendable () -> Date = { Date() },
            rideMapState: MobileRideMapState? = nil,
            database: RideDatabaseHandle? = nil
        ) {
            let rustSessionState =
                database.map {
                    CutoutSessionStateHandle.withDatabase(database: $0)
                } ?? CutoutSessionStateHandle()
            self.rustSessionState = rustSessionState
            self.databaseForCapturePublication = database
            let deviceDetectionSession = DeviceDetectionSession(sessionState: rustSessionState)
            self.deviceDetectionSession = deviceDetectionSession
            self.identificationProbeTransport = IdentificationProbeTransportCoordinator(
                detectionSession: deviceDetectionSession
            )
            self.clock = clock
            self.wallClock = wallClock
            self.rideMapStateForInitialization = rideMapState
            self.reconnectController = ConnectionReconnectController(scheduler: MainQueueReconnectScheduler())
            self.reconnectJitter = { Double.random(in: 0...1) }
            self.selectedDeviceStore = selectedDeviceStore
            self.injectedNotificationEffects = nil
            self.injectedCaptureRecorder = nil
            self.injectedDisplayPublisher = nil
            self.injectedPhoneLocationAdapter = nil
            self.injectedRideMapRecorder = nil
            super.init()
            bleQueue.setSpecific(key: bleQueueKey, value: ())
        }
    #endif

    @MainActor
    public func start() {
        // Create CLLocationManager on the app thread before Map restoration can publish
        // availability from its storage queue.
        phoneLocationAdapter.start()
        let queue = bleQueue
        let reference = WeakCutoutSessionCoreReference(self)
        rideMapRecorder.start {
            queue.async {
                guard let core = reference.value else { return }
                core.observeRideMapConnection(at: core.clock.now())
            }
        }
        publishRideMapAvailabilityOnMain()
        #if DEBUG
            if let testScript {
                publishOnMain { self.onBluetoothRestorationResolved?(nil) }
                start(testScript: testScript)
                return
            }
        #endif
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
            storedScanState = DevicePickerScanState(status: .scanning, discoverySnapshot: snapshot)
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
            return connectForProtocolDetection(to: peripheral, using: advertisement)
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
            return connectForProtocolDetection(to: peripheral, using: advertisement)
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
            return connectForProtocolDetection(to: peripheral, using: advertisement)
        }
    }

    @discardableResult
    public func recordOnly(platformIdentifier: String, note: String? = nil, annotations: [String] = []) -> Bool {
        #if DEBUG
            if let testScript {
                guard platformIdentifier == testScript.candidate.platformIdentifier else { return false }
                return onBleQueue {
                    guard rustSessionState.captureLifecycleSnapshot().canStart else { return false }
                    isRecordOnly = true
                    guard
                        startCapture(
                            reason: note ?? "record-only", annotations: annotations,
                            evidence: "simulator_fixture", origin: .manual
                        )
                    else {
                        isRecordOnly = false
                        return false
                    }
                    publishCaptureProgress()
                    return true
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
            return connectRecordOnly(to: peripheral, using: advertisement, note: note, annotations: annotations)
        }
    }

    @discardableResult
    public func annotateCapture(label: String) -> Bool {
        annotateCapture(key: "capture_label", value: label)
    }

    /// Applies a complete label transition to the writer that owns this generation.
    public func changeCaptureLabel(
        generation: CaptureGeneration, action: MobileCaptureLabelActionDto
    ) throws -> [MobileCaptureLabelDto] {
        try onBleQueue {
            Result {
                guard captureGeneration == generation, captureRecorder.hasWriter else {
                    throw MobileCaptureAnnotationError.NotRecording
                }
                return try captureRecorder.changeLabel(action)
            }
        }.get()
    }

    @discardableResult
    public func annotateCapture(key: String, value: String) -> Bool {
        onBleQueue {
            let annotation = pevcapAnnotation(key: key, value: value)
            guard captureRecorder.hasWriter else { return false }
            let outcome = captureRecorder.addAnnotation(annotation)
            guard acceptCaptureWrite(outcome) else { return false }
            record(annotation)
            return true
        }
    }

    public func flushCapture() async -> Bool {
        if DispatchQueue.getSpecific(key: bleQueueKey) != nil {
            return flushCaptureOnBleQueue()
        }
        let queuedAt = clock.now()
        return await withCheckedContinuation { continuation in
            let workItem = DispatchWorkItem(qos: .default, flags: .enforceQoS) { [self] in
                let result = flushCaptureOnBleQueue()
                let waitMilliseconds = clock.now().elapsed(since: queuedAt).rawValue
                if waitMilliseconds > 0 {
                    record("ble_queue_wait_ms=\(waitMilliseconds)")
                }
                continuation.resume(returning: result)
            }
            bleQueue.async(execute: workItem)
        }
    }

    private func flushCaptureOnBleQueue() -> Bool {
        guard captureRecorder.hasWriter, let generation = captureGeneration else { return false }
        let succeeded: Bool
        #if DEBUG
            succeeded = testScript?.flushCaptureSucceeds == false ? false : captureRecorder.flushWriter()
        #else
            succeeded = captureRecorder.flushWriter()
        #endif
        if !succeeded {
            _ = rustSessionState.captureWriterFailed(generation: generation.dto)
            publishCaptureProgress()
        }
        return succeeded
    }

    /// Rust admits the save; native code performs the flush and releases only its owned transport.
    public func finishCapture() async -> Bool {
        guard
            let token = onBleQueue({
                guard let generation = captureGeneration,
                    let token = rustSessionState.beginCaptureFinish(generation: generation.dto)
                else { return Optional<MobileCaptureFinishTokenDto>.none }
                publishCaptureProgress()
                return token
            })
        else { return false }
        let succeeded = await flushCapture()
        return onBleQueue {
            guard rustSessionState.finishCaptureFlush(token: token, succeeded: succeeded) else {
                publishCaptureProgress()
                return false
            }
            // Admission and disconnect share the BLE queue turn: a replacement cannot interleave.
            disconnectAndScanOnBleQueue()
            return true
        }
    }

    public func disconnectAndScan() {
        onBleQueue { disconnectAndScanOnBleQueue() }
    }

    public func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue)
        throws
    {
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
    public func resetTripMeterForNewRide(token: ConnectionAttemptToken) -> Bool {
        onBleQueue {
            guard let owner = liveOwner, owner.token == token else { return false }
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
                    storedScanState = DevicePickerScanState(status: .bluetoothUnavailable, rows: [])
                    publishScanState()
                    setPhase(.bluetoothUnavailable(rawState: 4))
                    return
                case .permissionDenied:
                    storedScanState = .permissionDenied
                    publishScanState()
                    setPhase(.bluetoothPermissionDenied)
                    return
                }
                storedScanState = DevicePickerScanState(status: .idle, rows: [testScript.candidate.pickerRow])
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

            guard rustSessionState.captureLifecycleSnapshot().canPair else { return false }
            finishCaptureAfterLinkDown()

            self.selectedRoute = route
            self.selectedModel = selectedModel
            testScriptWorkItem?.cancel()
            testScriptUpdateWorkItem?.cancel()
            liveOwner?.invalidate()
            liveOwner = nil
            guard
                let token = rustSessionState.beginConnectionAttempt(
                    platformIdentifier: platformIdentifier, nowMs: clock.now().rawValue
                ).token
            else { return false }
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
                connectionLinkDownOnBleQueue(token: token)
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
            for bytes in testScript.protocolDetectionNotifications ?? testScript.protocolNotifications {
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
                attachSettingsCallback()
                do {
                    let step = try owner.handleLinkUp(at: clock.now())
                    let channels = step.operations.compactMap { operation -> BluetoothUuid? in
                        guard case let .subscribe(channel) = operation else { return nil }
                        owner.handleNotificationStateUpdate(channel: channel, isNotifying: true, error: nil)
                        return channel
                    }
                    if let channel = channels.first {
                        let replayStartedAt = clock.now()
                        let replayInterval = testScript.protocolNotificationIntervalMilliseconds ?? 1
                        for (index, bytes) in testScript.protocolNotifications.enumerated() {
                            let receivedAt = MonotonicMilliseconds(
                                replayStartedAt.rawValue + UInt64(index) * replayInterval
                            )
                            let step = try ingestAcceptedLiveNotification(
                                AcceptedLiveNotificationIngress(
                                    channel: channel,
                                    bytes: bytes,
                                    receivedAt: receivedAt
                                ),
                                using: owner
                            )
                            if testScript.appliesProtocolNotificationSteps {
                                applyNotificationStep(step, receivedAt: receivedAt)
                            }
                        }
                        repeatTestProtocolNotifications(testScript, token: token, channel: channel)
                    }
                } catch {
                    setPhase(.failed(.sessionFailed(error.sessionMessage)))
                    return
                }
            }
            if testScript.appliesProtocolNotificationSteps {
                if testScript.protocolNotifications.isEmpty {
                    setPhase(.live)
                }
                scheduleTestTelemetryUpdateIfNeeded(testScript, token: token)
                scheduleTestReconnectIfNeeded(testScript, token: token)
                scheduleTestBluetoothLossIfNeeded(testScript, token: token)
                return
            }
            emit(testScript: testScript, token: token)
        }

        private func emit(testScript: CutoutSessionTestScript, token: ConnectionAttemptToken) {
            guard rustSessionState.connectionAttemptIsCurrent(token: token),
                rustSessionState.connectionAttemptSnapshot().transport == .connected
            else { return }
            if testScript.startsLive {
                storedProtocolIdentityCandidate = testScript.candidate
                publishProtocolIdentityCandidate()
            }
            guard let telemetry = testScript.telemetry else {
                setPhase(.live)
                return
            }
            let now = clock.now()
            let receivedAt =
                if testScript.emitsStaleTelemetry {
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

        private func repeatTestProtocolNotifications(
            _ script: CutoutSessionTestScript, token: ConnectionAttemptToken, channel: BluetoothUuid
        ) {
            testProtocolNotificationTimer?.cancel()
            testProtocolNotificationTimer = nil
            guard let interval = script.protocolNotificationIntervalMilliseconds, interval > 0 else { return }
            let timer = DispatchSource.makeTimerSource(queue: bleQueue)
            timer.schedule(
                deadline: .now() + .milliseconds(Int(clamping: interval)),
                repeating: .milliseconds(Int(clamping: interval)))
            timer.setEventHandler { [weak self] in
                guard let self else { return }
                guard self.rustSessionState.verifiedConnectionAttemptIsCurrent(token: token),
                    let owner = self.liveOwner, owner.token == token
                else {
                    self.testProtocolNotificationTimer?.cancel()
                    self.testProtocolNotificationTimer = nil
                    return
                }
                do {
                    for bytes in script.protocolNotifications {
                        let at = self.clock.now()
                        self.applyNotificationStep(
                            try self.ingestAcceptedLiveNotification(
                                AcceptedLiveNotificationIngress(channel: channel, bytes: bytes, receivedAt: at),
                                using: owner
                            ),
                            receivedAt: at
                        )
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

        private func scheduleTestTelemetryUpdateIfNeeded(
            _ testScript: CutoutSessionTestScript, token: ConnectionAttemptToken
        ) {
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

        private func scheduleTestReconnectIfNeeded(_ testScript: CutoutSessionTestScript, token: ConnectionAttemptToken)
        {
            guard testScript.reconnectsAfterFirstLive, !testScriptDidReconnect else { return }
            testScriptDidReconnect = true
            let reconnect = DispatchWorkItem { [weak self] in
                self?.onBleQueue {
                    guard let self, self.rustSessionState.connectionAttemptIsCurrent(token: token) else { return }
                    self.connectionLinkDownOnBleQueue(token: token)
                    self.testScriptUpdateWorkItem?.cancel()
                    guard
                        let retryToken = self.rustSessionState.beginConnectionAttempt(
                            platformIdentifier: token.platformIdentifier, nowMs: self.clock.now().rawValue
                        ).token
                    else { return }
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

        private func scheduleTestBluetoothLossIfNeeded(
            _ testScript: CutoutSessionTestScript, token: ConnectionAttemptToken
        ) {
            guard let delay = testScript.bluetoothLossAfterFirstLiveMilliseconds else { return }
            let loss = DispatchWorkItem { [weak self] in
                self?.onBleQueue {
                    guard let self, self.rustSessionState.connectionAttemptIsCurrent(token: token) else { return }
                    self.connectionLinkDownOnBleQueue(token: token)
                    self.storedScanState = DevicePickerScanState(status: .bluetoothUnavailable, rows: [])
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
        let phoneAlarmActions = rustSessionState.deactivatePhoneAlarmDevice()
        if !phoneAlarmActions.schedule.isEmpty || !phoneAlarmActions.cancelRequestIds.isEmpty {
            publishOnMain { self.onPhoneAlarmActionsAvailable?(phoneAlarmActions) }
        }
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
        captureRecorder.resetMusicContext()
        #if DEBUG
            if testScript != nil, isRecordOnly, captureRecorder.hasWriter {
                finishCaptureAfterLinkDown()
            } else if testScript != nil, isRecordOnly, captureRecorder.activeFileURL != nil {
                captureRecorder.finishSynthetic()
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
                storedScanState = DevicePickerScanState(status: .idle, rows: [testScript.candidate.pickerRow])
            } else {
                storedScanState = DevicePickerScanState(
                    status: .scanning, discoverySnapshot: rustSessionState.discoverySnapshot())
            }
        #else
            storedScanState = DevicePickerScanState(
                status: .scanning, discoverySnapshot: rustSessionState.discoverySnapshot())
        #endif
        publishScanState()
        setPhase(.scanning)
        central?.scanForPeripherals(withServices: nil)
    }

    private func connectionLinkDownOnBleQueue(token: ConnectionAttemptToken) {
        let phoneAlarmActions = rustSessionState.deactivatePhoneAlarmDevice()
        publishOnMain { self.onPhoneAlarmActionsAvailable?(phoneAlarmActions) }
        _ = rustSessionState.connectionLinkDown(token: token)
    }

    public func now() -> MonotonicMilliseconds {
        clock.now()
    }

    func applyLinkUpStep(_ step: CoreBluetoothSessionStep) {
        record("link_operations=\(step.operations.map(String.init(describing:)).joined(separator: ","))")
        recordCaptureLinkUp()
        let receivedAt = clock.now()
        applyAcceptedSessionStep(step, receivedAt: receivedAt, displayUpdateKind: .linkUp)
        setPhase(.subscribing)
    }

    func applyNotificationStep(_ step: CoreBluetoothSessionStep, receivedAt: MonotonicMilliseconds) {
        if case .failed = phase {
            return
        }
        cancelPendingReconnect()
        applyAcceptedSessionStep(step, receivedAt: receivedAt, displayUpdateKind: .notification)
        setPhase(.live)
    }

    private func applyAcceptedSessionStep(
        _ step: CoreBluetoothSessionStep,
        receivedAt: MonotonicMilliseconds,
        displayUpdateKind: RideDisplayUpdateKind
    ) {
        let bmsObservations = step.actions.compactMap { action in
            action.kind == .bmsSnapshot ? action.bmsSnapshot : nil
        }.flatMap(\.rawObservations)
        notificationEffects.applyActions(step.actions)
        notificationEffects.observeRideMapConnection(receivedAt)
        notificationEffects.persistBmsSamples(bmsObservations)
        let snapshot = step.snapshot
        displayState = notificationEffects.reduceDisplayState(
            displayState,
            snapshot,
            receivedAt,
            displayUpdateKind
        )
        hasObservedSpeedSnapshot = hasObservedSpeedSnapshot || snapshot?.speed?.value != nil
        publishDisplayState()
        publishPhoneAlarmActionsAvailable()
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
        guard
            resolution.protocolFamily != nil
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
        publishSettings(settings)
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
        if let previous,
            previous.state == .connected || previous.state == .connecting || previous.state == .disconnecting
        {
            retiringPeripheralIdentifiers.insert(previous.identifier)
            previous.delegate = nil
            central?.cancelPeripheralConnection(previous)
        }
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.rustSessionState.connectionAttemptIsCurrent(token: token),
                self.connectionSnapshot.readiness == .pending
            else { return }
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
        guard
            let selectedIdentifier = CoreBluetoothRestorationPolicy.selectedPlatformIdentifier(
                savedPlatformIdentifier: selectedDeviceStore.platformIdentifier,
                restoredPlatformIdentifiers: restoredPlatformIdentifiers
            )
        else {
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

    private func connectRecordOnly(
        to peripheral: CBPeripheral, using advertisement: CoreBluetoothAdvertisement, note: String?,
        annotations: [String]
    ) -> Bool {
        guard rustSessionState.captureLifecycleSnapshot().canStart else { return false }
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
        guard
            startCapture(
                reason: "record-only",
                annotations: ["route=record_only"] + annotations
                    + (note.map {
                        [pevcapAnnotation(key: "user_note", value: $0)]
                    } ?? []), origin: .manual
            )
        else {
            isRecordOnly = false
            return false
        }
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        setPhase(.discoveringServices)
        central?.stopScan()
        startPreparedConnection()
        return true
    }

    @discardableResult
    private func connectForProtocolDetection(
        to peripheral: CBPeripheral,
        using advertisement: CoreBluetoothAdvertisement,
        intent: DeviceConnectionIntentDto = .use
    ) -> Bool {
        guard rustSessionState.captureLifecycleSnapshot().canPair else { return false }
        finishCaptureAfterLinkDown()
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
        _ = startCapture(reason: "protocol-detection", annotations: ["intent=manual_use"])
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        setPhase(.discoveringServices)
        central?.stopScan()
        startPreparedConnection()
        return true
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
            _ = self.rustSessionState.connectionTransportFailed(token: token)
            self.publishConnectionSnapshot()
            self.cancelFailedConnectionAttempt()
            self.setPhase(
                .failed(
                    .notificationFailed(
                        error?.sessionMessage ?? "notifications disabled for \(channel)"
                    )))
        }
        if let chargeEstimateProfile { owner.configureChargeEstimate(profile: chargeEstimateProfile) }
        return owner
    }

    private func buildOwner(for peripheral: CBPeripheral) {
        guard liveOwner == nil, let advertisement, let token = connectionSnapshot.token,
            rustSessionState.verifiedConnectionAttemptIsCurrent(token: token)
        else { return }
        do {
            bmsStorageSessionIdentifier = UUID().uuidString
            let owner = makeDeviceTransport(
                token: token,
                advertisement: advertisement,
                sink: self,
                writeLimit: TransportWriteLimitBytes(
                    UInt16(clamping: peripheral.maximumWriteValueLength(for: .withoutResponse))
                )
            )
            liveOwner = owner
            attachSettingsCallback()
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
            acceptsConnectionCallback(peripheral, token: attempt.token)
        else {
            return
        }
        let wasDetecting = connectionSnapshot.readiness == .pending
        liveOwner?.invalidate()
        connectionLinkDownOnBleQueue(token: attempt.token)
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
                _ = self.deviceDetectionSession.observeAdvertisement(
                    name: self.advertisement?.localName.map { Data($0.utf8) })
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
        guard
            let schedule = reconnectController.schedule(
                jitter: reconnectJitter(),
                operation: { [weak self] in
                    guard let self else { return }
                    self.onBleQueue {
                        guard !self.suppressReconnect,
                            self.connectionSnapshot.generation == connectionGeneration
                        else { return }
                        _ = self.startCapture(
                            reason: "protocol-detection",
                            annotations: ["intent=previously_connected"]
                        )
                        reconnect()
                    }
                }
            )
        else {
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
        publishOnMain { self.displayPublisher.submit(value, queuedAt: queuedAt) }
    }

    private func publishScanState() {
        let value = scanState
        let generation = connectionSnapshot.generation
        publishOnMain {
            guard self.connectionSnapshot.generation == generation else { return }
            self.onScanStateChange?(value)
        }
    }

    private func publishSettings(_ value: DeviceSettings) {
        publishOnMain { [weak self] in
            guard let self, self.connectionSnapshot.revision == value.connection.revision else { return }
            self.onSettingsChange?(value)
        }
    }

    private func publishPhoneAlarmActionsAvailable() {
        let actions = rustSessionState.drainPhoneAlarmActions()
        guard !actions.schedule.isEmpty || !actions.cancelRequestIds.isEmpty else { return }
        publishOnMain { self.onPhoneAlarmActionsAvailable?(actions) }
    }

    private func attachSettingsCallback() {
        guard let owner = liveOwner else { return }
        owner.onSettingsChange = { [weak self, weak owner] state in
            guard let self, let owner, self.liveOwner === owner else { return }
            self.publishSettings(state)
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

    @MainActor
    private func publishPhoneLocationSnapshot(receivedAt: MonotonicMilliseconds) {
        let value = phoneLocationSnapshot
        publishOnMain { self.onPhoneLocationSnapshotChange?(value, receivedAt) }
    }

    private func publishProtocolIdentityCandidate() {
        #if DEBUG
            if testScript == nil {
                storedScanState = DevicePickerScanState(
                    status: storedScanState.status,
                    discoverySnapshot: rustSessionState.discoverySnapshot()
                )
            }
        #else
            storedScanState = DevicePickerScanState(
                status: storedScanState.status,
                discoverySnapshot: rustSessionState.discoverySnapshot()
            )
        #endif
        publishScanState()
        let value = protocolIdentityCandidate
        let generation = connectionSnapshot.generation
        publishOnMain {
            guard self.connectionSnapshot.generation == generation else { return }
            self.onProtocolIdentityCandidateChange?(value)
        }
    }

    private func publishCaptureEvent(_ event: CaptureEvent) {
        let lifecycle = rustSessionState.captureLifecycleSnapshot()
        publishOnMain {
            self.onCaptureEvent?(.lifecycle(lifecycle))
            self.onCaptureEvent?(event)
        }
    }

    private func publishCaptureProgress() {
        guard let generation = captureGeneration else { return }
        let progress = captureRecorder.publishProgress()
        if progress.writerError != nil {
            _ = rustSessionState.captureWriterFailed(generation: generation.dto)
        }
    }

    private func publishCaptureFailure() {
        captureRecorder.publishFailure()
    }

    private func observeRideMapConnection(at receivedAt: MonotonicMilliseconds) {
        let snapshot = connectionSnapshot
        guard let token = snapshot.token else { return }
        let queue = bleQueue
        let reference = WeakCutoutSessionCoreReference(self)
        rideMapRecorder.observeConnection(
            at: receivedAt,
            token: token,
            connectionState: rustSessionState,
            resetTripMeter: { token in
                queue.async {
                    reference.value?.resetTripMeterForNewRide(token: token)
                }
            }
        )
    }

    private func persistBmsSamples(_ observations: [BmsRawVoltageObservation]) {
        rideMapRecorder.persistBmsSamples(
            observations,
            deviceIdentity: protocolIdentityCandidate?.platformIdentifier
                ?? peripheral?.identifier.uuidString,
            sessionIdentifier: bmsStorageSessionIdentifier
        )
    }

    @MainActor
    private func publishRideMapDecisions(_ batch: RideMapDecisionBatch) {
        for outcome in batch.outcomes {
            switch outcome.decision {
            case let .storageError(message):
                publishRideMapError(
                    .storageError(message),
                    context: MobileRideMapErrorContext(snapshot: outcome.snapshot)
                )
            default:
                rideMapPresentation.publishDecision(outcome.decision, for: outcome.snapshot)
            }
        }
    }

    @MainActor
    private func publishRideMapSnapshot(_ snapshot: MobileRideMapSnapshotDto) {
        rideMapPresentation.publishSnapshot(snapshot)
    }

    @MainActor
    private func publishRideMapError(
        _ error: MobileRideMapError,
        context: MobileRideMapErrorContext
    ) {
        let event = MobileRideMapErrorEvent(context: context, error: error)
        rideMapPresentation.publishError(event)
    }

    private func publishRideMapAvailability(
        initializationError: MobileRideMapError?,
        isReady: Bool
    ) {
        let reference = WeakCutoutSessionCoreReference(self)
        Task { @MainActor in
            guard let core = reference.value else { return }
            core.rideMapStorageStatus = (initializationError, isReady)
            core.publishRideMapAvailabilityOnMain()
        }
    }

    @MainActor
    private func publishRideMapAvailabilityOnMain() {
        let availability: MobileRideMapAvailability
        if rideMapStorageStatus.error != nil {
            availability = .storageUnavailable
        } else if !rideMapStorageStatus.isReady {
            availability = .checking
        } else if !CLLocationManager.locationServicesEnabled() {
            availability = .servicesDisabled
        } else {
            switch phoneLocationAdapter.authorizationStatus {
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
        rideMapPresentation.publishAvailability(availability)
    }

    /// Records one bounded music observation independently of BLE frame arrival.
    /// The Rust writer owns the capture event and keeps metadata low-rate.
    public func updateMusicCaptureObservation(_ observation: MobilePevcapMusicEventDto?) {
        onBleQueue {
            _ = self.acceptCaptureWrite(self.captureRecorder.recordMusicObservation(observation))
        }
    }

    /// Applies the ride's Rust-owned retention policy to future PEVCAP music writes.
    public func updateMusicCapturePolicy(_ policy: MobileMusicHistoryPolicyDto) {
        onBleQueue {
            _ = self.acceptCaptureWrite(self.captureRecorder.updateMusicPolicy(policy))
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
                failCaptureWriter(
                    message: "missing service UUID for \(characteristic.uuidString)"
                )
                return false
            }
            let outcome = captureRecorder.recordNotification(
                characteristic: channel,
                service: serviceUuid,
                bytes: bytes,
                telemetry: telemetry
            )
            guard acceptCaptureWrite(outcome) else { return false }
            record("capture_queue_depth=\(captureRecorder.writerStatus()?.queuedMessages ?? 0)")
        default:
            return false
        }
        return true
    }

    @discardableResult
    private func startCapture(
        reason: String,
        annotations extraAnnotations: [String] = [],
        evidence: String = "hardware_tested",
        origin: MobileCaptureOriginDto = .automatic
    ) -> Bool {
        guard let identity = rustSessionState.beginCapture(origin: origin) else { return false }
        let generation = CaptureGeneration(rawValue: identity.value)
        var directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        #if DEBUG
            directory = captureDirectoryForTesting ?? directory
        #endif
        let started = captureRecorder.start(
            generation: generation,
            platformIdentifier: advertisement?.peripheralIdentifier.rawValue ?? "ios",
            advertisedServices: advertisement?.advertisedServiceUuids ?? [],
            directory: directory,
            reason: reason,
            annotations: extraAnnotations,
            evidence: evidence,
            origin: origin,
            advertisedName: advertisement?.localName
        )
        guard started else {
            record("capture_error=writer_start_failed")
            _ = rustSessionState.captureWriterStartFailed(generation: identity)
            publishCaptureEvent(.failed(generation: generation))
            return false
        }
        _ = rustSessionState.captureWriterStarted(generation: identity)
        captureRecorder.publishStarted()
        startCaptureProgressUpdates()
        if let fileURL = captureRecorder.activeFileURL { record("capture_file=\(fileURL.path)") }
        updateCaptureIdentity()
        return true
    }

    /// Refreshes the existing typed snapshot even while the Bluetooth link is quiet.
    /// Scheduling is native; byte counts and writer health still come from Rust.
    private func startCaptureProgressUpdates() {
        captureProgressTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: bleQueue)
        timer.schedule(deadline: .now() + .seconds(1), repeating: .seconds(1))
        timer.setEventHandler { [weak self] in self?.publishCaptureProgress() }
        captureProgressTimer = timer
        timer.resume()
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

    private func acceptCaptureWrite(_ outcome: MobileCaptureWriteOutcomeDto) -> Bool {
        switch outcome {
        case .accepted:
            return true
        case .rejected:
            return false
        case .failed:
            failCaptureWrite()
            return false
        }
    }

    private func acceptCaptureWrite(_ accepted: Bool) -> Bool {
        guard !accepted else { return true }
        failCaptureWrite()
        return false
    }

    private func failCaptureWrite() {
        let status = captureRecorder.writerStatus()
        record("capture_error=writer_failed \(status?.lastError ?? "unknown")")
        failCaptureWriter(message: "capture writer queue overrun")
    }

    private func failCaptureWriter(message: String) {
        if isIngestingLiveNotification {
            deferredCaptureFailureMessage = message
            return
        }
        record("capture_error=writer_failed reason=\(message)")
        if let generation = captureGeneration {
            _ = rustSessionState.captureWriterFailed(generation: generation.dto)
        }
        publishCaptureFailure()
        finishCaptureWriter(priorWriteSucceeded: false)
    }

    private func finishDeferredCaptureFailure() {
        guard let message = deferredCaptureFailureMessage else { return }
        deferredCaptureFailureMessage = nil
        record("capture_error=writer_failed reason=\(message)")
        if let generation = captureGeneration {
            _ = rustSessionState.captureWriterFailed(generation: generation.dto)
        }
        publishCaptureFailure()
        finishCaptureWriter(priorWriteSucceeded: false)
    }

    private func finishCaptureAfterLinkDown() {
        guard captureRecorder.hasWriter else { return }
        let outcome = captureRecorder.recordLinkDown()
        finishCaptureWriter(priorWriteSucceeded: outcome == .accepted)
    }

    private func recordCaptureLinkUp() {
        let maxWriteLength = peripheral.map {
            UInt16(clamping: $0.maximumWriteValueLength(for: .withoutResponse))
        }
        _ = acceptCaptureWrite(captureRecorder.recordLinkUp(maxWriteLength: maxWriteLength))
    }

    private func finishCaptureWriter(
        priorWriteSucceeded: Bool = true
    ) {
        guard let completedCaptureGeneration = captureGeneration, captureRecorder.hasWriter else { return }
        captureProgressTimer?.cancel()
        captureProgressTimer = nil
        publishCaptureProgress()
        _ = rustSessionState.retireCaptureWriter(generation: completedCaptureGeneration.dto)
        captureRecorder.finish(publishesResult: true, priorWriteSucceeded: priorWriteSucceeded)
    }

    private func handleCaptureWriterCompletion(_ completion: CaptureWriterCompletion) {
        let succeeded = completion.succeeded
        _ = rustSessionState.completeCaptureWriter(
            generation: completion.generation.dto,
            succeeded: succeeded
        )
        if succeeded {
            if completion.databasePublicationSucceeded == false {
                record("capture_warning=database_publication_failed; saved_file_retained=true")
            }
            switch completion.outcome {
            case let .artifactAvailable(artifact):
                publishCaptureEvent(
                    .finished(
                        generation: completion.generation,
                        fileURL: URL(fileURLWithPath: artifact.path)
                    ))
            case .databaseFinished:
                publishCaptureEvent(
                    .databaseFinished(generation: completion.generation, outcome: completion.outcome))
            case .notStarted, .finalizing, .failed:
                record("capture_error=writer_finish_failed")
                publishCaptureEvent(.failed(generation: completion.generation))
            }
        } else {
            record("capture_error=writer_finish_failed")
            publishCaptureEvent(.failed(generation: completion.generation))
        }
    }

    #if DEBUG
        func finishCaptureForTesting(priorWriteSucceeded: Bool = true) {
            onBleQueue {
                self.finishCaptureWriter(priorWriteSucceeded: priorWriteSucceeded)
            }
        }

        func acceptCaptureWriteOutcomeForTesting(_ outcome: MobileCaptureWriteOutcomeDto) -> Bool {
            onBleQueue { acceptCaptureWrite(outcome) }
        }
    #endif

    private func captureElapsedMilliseconds() -> UInt64 {
        captureRecorder.elapsedMilliseconds()
    }

    func captureElapsedMilliseconds(since captureStartedAt: MonotonicMilliseconds) -> UInt64 {
        captureRecorder.elapsedMilliseconds(since: captureStartedAt)
    }

    private func updateCaptureIdentity() {
        guard captureRecorder.hasWriter, let identity = pevcapResolvedIdentity() else {
            return
        }
        let candidate = protocolIdentityCandidate
        guard
            acceptCaptureWrite(
                captureRecorder.setResolvedIdentity(
                    identity,
                    evidence: candidate?.evidence,
                    detail: candidate?.detail
                ))
        else { return }
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
    String(
        value.map { character in
            switch character {
            case "=", "\n", "\r":
                " "
            default:
                character
            }
        })
}

extension ElectricUnicycleModel {
    fileprivate func pevcapResolvedIdentity(verification: MobileVerificationStatusDto) -> MobileResolvedIdentityDto {
        MobileResolvedIdentityDto(
            protocolFamily: pevcapProtocolFamily,
            model: MobileVerifiedStringDto(value: pevcapModelName, verification: verification),
            firmware: nil
        )
    }

    fileprivate var pevcapProtocolFamily: MobileProtocolFamilyDto {
        switch self {
        case .aero:
            .veteranLeaperkimNosfet
        case .falcon:
            .begodeGotway
        }
    }

    fileprivate var pevcapModelName: String {
        switch self {
        case .aero:
            "NOSFET Aero"
        case .falcon:
            "Begode Falcon"
        }
    }
}

extension DiscoverySnapshot {
    fileprivate var selectedAdvertisement: CoreBluetoothAdvertisement? {
        selectedPlatformIdentifier.flatMap(advertisement(platformIdentifier:))
    }

    fileprivate var lastAdvertisement: CoreBluetoothAdvertisement? {
        observations.last.map(CoreBluetoothAdvertisement.init(discoveryObservation:))
    }

    fileprivate func advertisement(platformIdentifier: String) -> CoreBluetoothAdvertisement? {
        observations
            .last { $0.platformIdentifier == platformIdentifier }
            .map(CoreBluetoothAdvertisement.init(discoveryObservation:))
    }
}

extension CoreBluetoothAdvertisement {
    fileprivate func withVescNordicUartFallbackName() -> Self {
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

extension CutoutSessionCore {
    fileprivate func restoreSelectedPeripheral(from restoredPeripherals: [CBPeripheral]) {
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
        record(
            "central_restore=selected state=\(restoredPeripheral.state.rawValue) observations=\(discovery.observations.count)"
        )

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
    fileprivate func prepareRestoredRide() -> Bool {
        _ = startCapture(reason: "protocol-detection", annotations: ["intent=previously_connected"])
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        return true
    }

    fileprivate func resumeConnectedPeripheral(_ peripheral: CBPeripheral) {
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
        let restoredPlatformIdentifier: String? =
            if let peripheral,
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
                connectionLinkDownOnBleQueue(token: token)
                connectionDeadlineWorkItem?.cancel()
                publishConnectionSnapshot()
            }
            storedScanState =
                state == .unauthorized
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
        storedScanState = DevicePickerScanState(
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
        let advertisedServices = advertisement.advertisedServiceUuids.map(String.init(describing:)).joined(
            separator: ",")
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
            acceptsConnectionCallback(peripheral, token: attempt.token)
        else { return }
        _ = rustSessionState.connectionLinkEstablished(token: attempt.token)
        publishConnectionSnapshot()
        setPhase(.discoveringServices)
        peripheral.delegate = attempt
        if isRecordOnly || isDetectingProtocol {
            recordCaptureLinkUp()
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
        guard let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) else {
            return
        }
        switch coreBluetoothCallbackDisposition(
            subscribed: subscribedCharacteristics[channel],
            callback: characteristic,
            error: error
        ) {
        case .ignored:
            record(
                "notification_ignored=unbound_characteristic service=\(characteristic.service?.uuid.uuidString ?? "unknown") characteristic=\(characteristic.uuid.uuidString)"
            )
            return
        case .failed(let error):
            setPhase(.failed(.notificationFailed(error.sessionMessage)))
            return
        case .accepted:
            break
        }
        guard let value = characteristic.value else {
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
                _ = captureFrame(
                    direction: "notify",
                    characteristic: characteristic.uuid,
                    service: characteristic.service?.uuid,
                    bytes: value
                )
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
            _ = captureFrame(
                direction: "notify",
                characteristic: characteristic.uuid,
                service: characteristic.service?.uuid,
                bytes: value
            )
            record("record_only_notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            publishCaptureProgress()
            return
        }
        guard let liveOwner else {
            return
        }
        isIngestingLiveNotification = true
        defer {
            isIngestingLiveNotification = false
            finishDeferredCaptureFailure()
        }
        do {
            let receivedAt = clock.now()
            let ingestStartedAt = receivedAt
            let step = try ingestAcceptedLiveNotification(
                AcceptedLiveNotificationIngress(channel: channel, bytes: value, receivedAt: receivedAt),
                using: liveOwner
            )
            let ingestFinishedAt = clock.now()
            let ingestMilliseconds =
                ingestFinishedAt.rawValue >= ingestStartedAt.rawValue
                ? ingestFinishedAt.rawValue - ingestStartedAt.rawValue
                : 0
            let captureAccepted = captureFrame(
                direction: "notify",
                characteristic: characteristic.uuid,
                service: characteristic.service?.uuid,
                bytes: value,
                telemetry: step.actions.compactMap(\.rawTelemetry).last
            )
            record("notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            publishCaptureProgress()
            record("speed=\(step.snapshot?.speed.map { String($0.value) } ?? "nil")")
            record("voltage=\(step.snapshot?.voltage.map { String($0.value) } ?? "nil")")
            record("battery_estimated=\(step.snapshot?.batteryLevelEstimated.map { String($0.value) } ?? "nil")")
            record("live_records=\(liveOwner.records.count)")
            record("notification_ingest_ms=\(ingestMilliseconds)")
            record("rust_decode_ms=\(ingestMilliseconds)")
            applyNotificationStep(step, receivedAt: receivedAt)
            if !captureAccepted {
                record("display_reduction_preserved_after_capture_failure=true")
            }
        } catch {
            record("notification_ingest_error=\(error)")
            setPhase(.failed(.notificationIngestFailed(error.sessionMessage)))
        }
    }

    private func ingestAcceptedLiveNotification(
        _ ingress: AcceptedLiveNotificationIngress,
        using owner: DeviceSessionTransport
    ) throws -> CoreBluetoothSessionStep {
        try owner.handleNotification(
            bytes: ingress.bytes,
            channel: ingress.channel,
            at: ingress.receivedAt
        )
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
        switch coreBluetoothCallbackDisposition(
            subscribed: subscribedCharacteristics[channel],
            callback: characteristic,
            error: error
        ) {
        case .ignored:
            record(
                "notification_state_ignored=unbound_characteristic service=\(characteristic.service?.uuid.uuidString ?? "unknown") characteristic=\(characteristic.uuid.uuidString)"
            )
            return
        case .failed(let error):
            setPhase(.failed(.notificationFailed(error.sessionMessage)))
            return
        case .accepted:
            break
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

extension CutoutSessionCore {
    fileprivate func bindDiscoveredCharacteristic(_ channel: BluetoothUuid, _ characteristic: CBCharacteristic) {
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

    fileprivate func preferredServiceUuid(for route: DevicePickerConnectionRoute?) -> CBUUID? {
        switch route {
        case .vescOnewheel:
            return BluetoothUuid.vescNordicUartService.coreBluetoothUuid
        case .electricUnicycle:
            return BluetoothUuid.bluetooth16(0xffe0).coreBluetoothUuid
        case nil:
            return nil
        }
    }

    fileprivate func assertOnBleQueue() {
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

    public func writeWithoutResponse(
        channel: BluetoothUuid, bytes: Data, isCurrent: @escaping () -> Bool,
        onReceipt: @escaping (CoreBluetoothWriteDisposition) -> Void
    ) -> CoreBluetoothWriteDisposition {
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
        guard isCurrent() else {
            onReceipt(.cancelled)
            return .cancelled
        }
        return bluetoothWriteAdapter.submit(
            channel: channel,
            bytes: bytes,
            peripheral: peripheral,
            characteristic: characteristic,
            isCurrent: { [weak self] in
                self?.peripheral === peripheral && isCurrent()
            },
            onReceipt: onReceipt
        )
    }

    public func canSubmitWithoutResponse() -> Bool {
        peripheral?.canSendWriteWithoutResponse ?? false
    }

    private func flushPendingWithoutResponseWrites() {
        guard let peripheral else { return }
        bluetoothWriteAdapter.flush(peripheral: peripheral) { [weak self] in
            self?.peripheral === peripheral
        }
    }

    public func peripheralIsReadyToSendWithoutResponse() {
        flushPendingWithoutResponseWrites()
    }

    public func clearPendingWithoutResponseWrites() {
        bluetoothWriteAdapter.clear()
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
            guard acceptCaptureWrite(captureRecorder.addGattFingerprint(fingerprint)) else { return }
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
            let resolution = rustSessionState.observeConnectionNotification(token: token, bytes: bytes)
        {
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
        advanceIdentificationQueries()
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
            advanceIdentificationQueries()
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

    private func advanceIdentificationQueries() {
        guard isDetectingProtocol || liveOwner != nil else { return }
        // Rust owns family eligibility, ordering and pending-response state.
        // Native code only executes the next admitted query on this attempt.
        identificationProbeTransport.notificationsEnabled(at: clock.now(), using: self)
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
        let candidate = rustSessionState.connectionAdmissionCandidate(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            displayName: advertisement.localName
                ?? protocolIdentityFallbackDisplayName(protocolFamily: resolution.protocolFamily),
            allowClosestMatch: allowClosestMatch
        )
        switch DevicePickerCandidateSupport(candidate) {
        case .supported(let route, let model):
            guard let route else { return false }
            // The Rust candidate is the authoritative result of protocol detection. Resolve
            // the connection from that exact supported evidence before any family-specific
            // fallback probe can be sent. In particular, a Veteran model frame must promote
            // an Aero attempt before the Begode N/V/M probe sequence runs.
            let resolved = rustSessionState.resolveDeviceSession(
                token: token,
                identificationComplete: true,
                nowMs: clock.now().rawValue
            )
            guard resolved.connection.readiness == .verified else {
                return false
            }
            connectionDeadlineWorkItem?.cancel()
            publishConnectionSnapshot()
            isDetectingProtocol = false
            clearProtocolDetectionExpiry()
            selectedRoute = route
            selectedModel = model
            annotateDetection("protocol_detection_resolved=\(route.rawValue)")
            buildOwner(for: peripheral)
            return liveOwner != nil
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise,
            .manualEntry, .unsupported:
            let resolved = rustSessionState.resolveDeviceSession(
                token: token,
                identificationComplete: allowClosestMatch,
                nowMs: clock.now().rawValue
            )
            switch resolved.connection.readiness {
            case .recordOnly:
                recordUnresolvedProtocolDetection(.unsupported, on: peripheral)
                return false
            case .pending, .disconnected, .failed, .conflicted:
                return false
            case .verified:
                connectionDeadlineWorkItem?.cancel()
                publishConnectionSnapshot()
            }
            let failure: IdentificationProbeFailure =
                candidate.support == .conflicting
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

        guard
            !promoteProtocolDetectionIfResolved(
                resolution,
                on: peripheral,
                allowClosestMatch: true
            )
        else {
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
        guard
            let deadline = deviceDetectionSession.nextBegodeProbeExpiry(
                timeout: BegodeProbeResponsePolicy.timeoutAfter
            )
        else {
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
                self.isDetectingProtocol
            else { return }
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
        guard acceptCaptureWrite(captureRecorder.addAnnotation(annotation)) else { return }
        record(annotation)
    }
}

extension CutoutSessionCore {
    @MainActor
    public func updateRideLocationDemand(for state: MobileRideMapStateDto) {
        phoneLocationAdapter.updateDemand(state == .active)
    }

    @MainActor
    public func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        phoneLocationAdapter.locationManagerDidChangeAuthorization(manager)
    }

    @MainActor
    public func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        phoneLocationAdapter.locationManager(
            manager,
            didUpdateLocations: locations
        )
    }

    private func handlePhoneLocationUpdate(_ update: PhoneLocationUpdate) {
        let captureResult = captureRecorder.recordLocationUpdate(update)
        if captureResult.outcome != .accepted, let generation = captureResult.generation {
            onBleQueue {
                guard self.captureGeneration == generation else { return }
                switch captureResult.outcome {
                case .accepted:
                    break
                case .rejected:
                    self.record("capture_warning=location_batch_rejected")
                case .failed:
                    _ = self.acceptCaptureWrite(.failed)
                }
            }
        }
        rideMapRecorder.ingestLocation(update)
    }

    public func startRideMapGpsOnly(atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        try await rideMapRecorder.startGpsOnly(atMs: atMs)
    }

    public func pauseRideMap(atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        try await rideMapRecorder.pause(atMs: atMs)
    }

    public func resumeRideMap(atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        try await rideMapRecorder.resume(atMs: atMs)
    }

    public func stopRideMap(atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        try await rideMapRecorder.stop(atMs: atMs)
    }

    public func saveRideMap() async throws -> MobileRideMapSnapshotDto {
        try await rideMapRecorder.save()
    }

    public func discardRideMap() async throws -> MobileRideMapSnapshotDto {
        try await rideMapRecorder.discard()
    }

    /// Clears the Rust-owned location context before starting a new capture.
    @MainActor
    public func resetRideMapLocationAdmission() {
        phoneLocationAdapter.clear()
    }
}

func unixMilliseconds(for date: Date) -> UInt64? {
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
extension CBCharacteristic {
    fileprivate var mobileGattRoles: [MobileGattRoleDto] {
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

struct MonotonicClock: Sendable {
    private let source: @Sendable () -> MonotonicMilliseconds

    init(
        now: @escaping @Sendable () -> MonotonicMilliseconds = {
            MonotonicMilliseconds(UInt64(ProcessInfo.processInfo.systemUptime * 1_000))
        }
    ) {
        source = now
    }

    func now() -> MonotonicMilliseconds {
        source()
    }
}

extension Optional where Wrapped == Error {
    fileprivate var sessionMessage: String {
        map(String.init(describing:)) ?? "unknown error"
    }
}

extension Error {
    fileprivate var sessionMessage: String {
        String(describing: self)
    }
}

extension Data {
    fileprivate var hexString: String {
        map { String(format: "%02x", $0) }.joined()
    }
}
