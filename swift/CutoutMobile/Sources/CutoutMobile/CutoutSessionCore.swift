import CoreBluetooth
import CoreLocation
import CutoutMobileFFI
import Foundation

func protocolIdentityFallbackDisplayName(
    protocolFamily: DeviceDetectionProtocolFamily?
) -> String {
    switch protocolFamily {
    case .veteranLeaperkimNosfet:
        "Veteran/NOSFET device"
    case .begodeGotway:
        "Begode device"
    case .vesc:
        "VESC device"
    case nil:
        "Detected rideable"
    }
}

public enum CaptureEvent: Equatable, Sendable {
    case started(fileURL: URL)
    case notificationRecorded
    case finished(fileURL: URL)
    case failed
}

public final class CutoutSessionCore: NSObject {
    public private(set) var displayState = RideDisplayState()
    public private(set) var phase = SessionConnectionPhase.starting
    public private(set) var records: [String] = []
    public private(set) var hasObservedSpeedSnapshot = false
    public private(set) var scanState = DevicePickerScanState(status: .idle, rows: [])
    public private(set) var settingsReadback: SettingsReadback?
    public private(set) var faultHistoryReadback: FaultHistoryReadback?
    public private(set) var bmsSnapshot: BmsSnapshot?
    public private(set) var phoneLocationSnapshot = MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    public private(set) var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?

    public var onDisplayStateChange: ((RideDisplayState) -> Void)?
    public var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    public var onRecord: ((String) -> Void)?
    public var onCaptureEvent: ((CaptureEvent) -> Void)?
    public var onScanStateChange: ((DevicePickerScanState) -> Void)?
    public var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    public var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    public var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    public var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto) -> Void)?
    public var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?

    private let clock = MonotonicClock()
    private let bleQueue = DispatchQueue(label: "io.cutout.corebluetooth", qos: .userInitiated)
    private let bleQueueKey = DispatchSpecificKey<Void>()
    private let rustSessionState = CutoutSessionStateHandle()
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
    private var captureStartedAt: MonotonicMilliseconds?
    private var captureBuilder: MobilePevcapCaptureBuilder?
    private var captureFileURL: URL?
    private var bmsPages: [BmsPageKey: BmsSnapshot] = [:]
    private var deviceDetectionSession = DeviceDetectionSession()
    private var pendingBegodeProbeResponses = Set<DeviceDetectionPendingProbe>()
    private var pendingDisplayState: RideDisplayState?
    private var pendingDisplayStateQueuedAt: Date?
    private var displayPublishWorkItem: DispatchWorkItem?
    private var lastDisplayPublication = Date.distantPast
    private let phoneLocationState = MobilePhoneLocationState()
    private var didRequestAlwaysLocationAuthorization = false
    private lazy var locationManager: CLLocationManager = {
        let manager = CLLocationManager()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBestForNavigation
        manager.activityType = .fitness
        manager.allowsBackgroundLocationUpdates = true
        return manager
    }()

    public override init() {
        super.init()
        bleQueue.setSpecific(key: bleQueueKey, value: ())
    }

    public func start() {
        startLocationUpdates()
        onBleQueue {
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
        onBleQueue {
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
        onBleQueue {
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
    public func recordOnly(platformIdentifier: String, note: String? = nil, annotations: [String] = []) -> Bool {
        onBleQueue {
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

    public func flushCapture() {
        onBleQueue {
            _ = captureBuilder?.flushWriter()
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

    private func disconnectAndScanOnBleQueue() {
        suppressReconnect = true
        isRecordOnly = false
        selectedModel = nil
        selectedRoute = nil
        chargeEstimateProfile = nil
        vescBoardProfile = nil
        liveOwner = nil
        deviceDetectionSession = DeviceDetectionSession()
        pendingBegodeProbeResponses.removeAll()
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

        scanState = DevicePickerScanState(status: .scanning, discoverySnapshot: rustSessionState.discoverySnapshot())
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
        suppressReconnect = false
        isRecordOnly = false
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = model
        selectedRoute = .electricUnicycle
        deviceDetectionSession = DeviceDetectionSession()
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
        suppressReconnect = false
        isRecordOnly = true
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = nil
        selectedRoute = nil
        liveOwner = nil
        deviceDetectionSession = DeviceDetectionSession()
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
        suppressReconnect = false
        isRecordOnly = false
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = nil
        selectedRoute = .vescOnewheel
        liveOwner = nil
        deviceDetectionSession = DeviceDetectionSession()
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
                retryCommandOnLinkUp: selectedRoute == .vescOnewheel ? .requestTelemetry : nil,
                executionQueue: bleQueue
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
        record("disconnected=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
        guard self.peripheral?.identifier == peripheral.identifier else {
            return
        }
        markOutstandingBegodeProbeResponsesMissing()
        _ = captureBuilder?.recordLinkDown(monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()))
        let completedCaptureURL = captureFileURL
        finishCaptureWriter()
        if let completedCaptureURL {
            publishCaptureEvent(.finished(fileURL: completedCaptureURL))
        }
        let wasRecordOnly = isRecordOnly
        let reconnectRoute = selectedRoute
        isRecordOnly = false
        selectedRoute = nil
        liveOwner = nil
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()
        pendingBegodeProbeResponses.removeAll()

        guard !suppressReconnect else {
            suppressReconnect = false
            return
        }

        guard !wasRecordOnly else {
            setPhase(.scanning)
            central?.scanForPeripherals(withServices: nil)
            return
        }

        switch reconnectRoute {
        case .vescOnewheel:
            selectedRoute = .vescOnewheel
            setPhase(.discoveringServices)
            startCapture(reason: "reconnect", annotations: ["route=vesc_onewheel"])
            central?.connect(peripheral)
        case .electricUnicycle, nil:
            guard let selectedModel else {
                setPhase(.failed(.connectFailed(error.sessionMessage)))
                return
            }

            selectedRoute = .electricUnicycle
            setPhase(.connecting(model: selectedModel))
            startCapture(reason: "reconnect", annotations: ["route=electric_unicycle"])
            central?.connect(peripheral)
        }
    }

    private func record(_ message: String) {
        if records.count < 2_048 {
            records.append(message)
        }
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
        let queuedAt = Date()
        return bleQueue.sync {
            let result = work()
            let waitMilliseconds = max(0, Int(Date().timeIntervalSince(queuedAt) * 1_000))
            if waitMilliseconds > 0 {
                record("ble_queue_wait_ms=\(waitMilliseconds)")
            }
            return result
        }
    }

    private func publishDisplayState() {
        let value = displayState
        let queuedAt = Date()
        publishOnMain { self.publishDisplayStateOnMain(value, queuedAt: queuedAt) }
    }

    private func publishDisplayStateOnMain(_ value: RideDisplayState, queuedAt: Date? = nil) {
        pendingDisplayState = value
        if let queuedAt {
            pendingDisplayStateQueuedAt = queuedAt
        }
        let elapsed = Date().timeIntervalSince(lastDisplayPublication)
        let interval = 0.333
        guard elapsed >= interval else {
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
                deadline: .now() + interval - elapsed,
                execute: work
            )
            return
        }
        pendingDisplayState = nil
        let publicationDelayMilliseconds = max(
            0,
            Int(Date().timeIntervalSince(pendingDisplayStateQueuedAt ?? Date()) * 1_000)
        )
        pendingDisplayStateQueuedAt = nil
        lastDisplayPublication = Date()
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
        publishOnMain { self.onPhoneLocationSnapshotChange?(value) }
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

    private func finishCaptureWriter() {
        guard let builder = captureBuilder else { return }
        captureBuilder = nil
        captureFileURL = nil
        captureStartedAt = nil
        DispatchQueue.global(qos: .utility).async {
            _ = builder.finishWriter()
        }
    }

    private func captureElapsedMilliseconds() -> UInt64 {
        guard let captureStartedAt else {
            return 0
        }
        return clock.now().rawValue.saturatingSubtracting(captureStartedAt.rawValue)
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

private extension UInt64 {
    func saturatingSubtracting(_ other: UInt64) -> UInt64 {
        self >= other ? self - other : 0
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
            localName: "VESC device",
            advertisedServiceUuids: advertisedServiceUuids,
            manufacturerData: manufacturerData,
            rssiDbm: rssiDbm
        )
    }
}

extension CutoutSessionCore: CBCentralManagerDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        assertOnBleQueue()
        record("central_state=\(central.state.rawValue)")
        guard central.state == .poweredOn else {
            setPhase(.bluetoothUnavailable(rawState: central.state.rawValue))
            return
        }
        setPhase(.scanning)
        let services = CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids
        record("scan_supported_services=\(services.map(\.uuidString).joined(separator: ","))")
        central.scanForPeripherals(withServices: nil)
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
        isRecordOnly = false
        selectedRoute = nil
        setPhase(.failed(.connectFailed(error.sessionMessage)))
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
            publishCaptureEvent(.notificationRecorded)
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
            publishCaptureEvent(.notificationRecorded)
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
        guard isAllowedReadOnlyWrite(channel: channel, bytes: bytes) else {
            setPhase(.failed(.skippedReadOnlyWrite))
            return
        }
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
            _ = deviceDetectionSession.observeBegodeNameProbe()
            pendingBegodeProbeResponses.insert(.begodeName)
            annotateDetection("begode_probe_write=model")
        case UInt8(ascii: "V")?:
            _ = deviceDetectionSession.observeBegodeFirmwareProbe()
            pendingBegodeProbeResponses.insert(.begodeFirmware)
            annotateDetection("begode_probe_write=firmware")
        case UInt8(ascii: "M")?:
            _ = deviceDetectionSession.observeBegodeImuProbe()
            pendingBegodeProbeResponses.insert(.begodeImu)
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

    func isAllowedReadOnlyWrite(channel: BluetoothUuid, bytes: Data) -> Bool {
        isReadOnlyBegodeProbeWrite(channel: channel, bytes: bytes)
            || isReadOnlyVescRequest(channel: channel, bytes: bytes)
    }

    func isReadOnlyVescRequest(channel: BluetoothUuid, bytes: Data) -> Bool {
        channel == .vescNordicUartWrite
            && (
                bytes == Data([0x02, 0x01, 0x04, 0x40, 0x84, 0x03])
                    || isReadOnlyRefloatRequest(bytes: bytes)
            )
    }

    func isReadOnlyRefloatRequest(bytes: Data) -> Bool {
        guard bytes.count >= 7,
              bytes.first == 0x02,
              bytes.last == 0x03,
              Int(bytes[bytes.index(after: bytes.startIndex)]) + 5 == bytes.count
        else {
            return false
        }
        let payloadStart = bytes.index(bytes.startIndex, offsetBy: 2)
        let payloadLength = Int(bytes[bytes.index(after: bytes.startIndex)])
        guard payloadLength >= 3,
              bytes[payloadStart] == 36,
              bytes[bytes.index(after: payloadStart)] == 101
        else {
            return false
        }
        let command = bytes[bytes.index(payloadStart, offsetBy: 2)]
        return command == 31 || command == 32
    }

    func observeDetectionNotification(channel: BluetoothUuid, bytes: Data) {
        guard channel.bluetooth16Value == 0xffe1 else {
            return
        }
        let previous = deviceDetectionSession.resolution
        let current = deviceDetectionSession.observeNotification(bytes: bytes)
        if current.modelBanner != nil, current.modelBanner != previous.modelBanner {
            pendingBegodeProbeResponses.remove(.begodeName)
            annotateDetection("begode_probe_response=model")
        }
        if current.firmwareBanner != nil, current.firmwareBanner != previous.firmwareBanner {
            pendingBegodeProbeResponses.remove(.begodeFirmware)
            annotateDetection("begode_probe_response=firmware")
        }
        if current.imuBanner != nil, current.imuBanner != previous.imuBanner {
            pendingBegodeProbeResponses.remove(.begodeImu)
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
            pendingBegodeProbeResponses.remove(.begodeName)
            annotateDetection("begode_probe_malformed=model")
        case .begodeFirmware:
            pendingBegodeProbeResponses.remove(.begodeFirmware)
            annotateDetection("begode_probe_malformed=firmware")
        case .begodeImu:
            pendingBegodeProbeResponses.remove(.begodeImu)
            annotateDetection("begode_probe_malformed=imu")
        }
    }

    func markOutstandingBegodeProbeResponsesMissing() {
        [
            (DeviceDetectionPendingProbe.begodeName, "model"),
            (.begodeFirmware, "firmware"),
            (.begodeImu, "imu"),
        ]
            .filter { pendingBegodeProbeResponses.contains($0.0) }
            .forEach { probe, label in
                let current = switch probe {
                case .begodeName:
                    deviceDetectionSession.observeBegodeNameProbeTimeout()
                case .begodeFirmware:
                    deviceDetectionSession.observeBegodeFirmwareProbeTimeout()
                case .begodeImu:
                    deviceDetectionSession.observeBegodeImuProbeTimeout()
                }
                annotateDetection("begode_probe_missing=\(label)")
                publishDetectionIdentityCandidate(current)
            }
        pendingBegodeProbeResponses.removeAll()
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

private struct MonotonicClock {
    private let base = Date()

    func now() -> MonotonicMilliseconds {
        MonotonicMilliseconds(UInt64(Date().timeIntervalSince(base) * 1_000))
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
