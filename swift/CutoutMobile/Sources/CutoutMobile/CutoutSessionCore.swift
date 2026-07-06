import CoreBluetooth
import Foundation

public final class CutoutSessionCore: NSObject {
    public private(set) var displayState = RideDisplayState()
    public private(set) var phase = SessionConnectionPhase.starting
    public private(set) var records: [String] = []
    public private(set) var hasObservedSpeedSnapshot = false
    public private(set) var scanState = DevicePickerScanState(status: .idle, rows: [])
    public private(set) var settingsReadback: SettingsReadback?
    public private(set) var faultHistoryReadback: FaultHistoryReadback?
    public private(set) var bmsSnapshot: BmsSnapshot?
    public private(set) var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?

    public var onDisplayStateChange: ((RideDisplayState) -> Void)?
    public var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    public var onRecord: ((String) -> Void)?
    public var onScanStateChange: ((DevicePickerScanState) -> Void)?
    public var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    public var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    public var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    public var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?

    private let clock = MonotonicClock()
    private let rustSessionState = CutoutSessionStateHandle()
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var advertisement: CoreBluetoothAdvertisement?
    private var discoveredPeripherals: [CoreBluetoothPeripheralIdentifier: CBPeripheral] = [:]
    private var liveOwner: CoreBluetoothLiveSessionOwner?
    private var selectedModel: ElectricUnicycleModel?
    private var isRecordOnly = false
    private var subscribedCharacteristics: [BluetoothUuid: CBCharacteristic] = [:]
    private var pendingServiceDiscoveries = Set<CBUUID>()
    private var suppressReconnect = false
    private var captureURL: URL?
    private var captureHandle: FileHandle?
    private var captureStartedAt: MonotonicMilliseconds?
    private var captureBuilder: MobilePevcapCaptureBuilder?
    private var didRecordCaptureFile = false
    private var bmsPages: [BmsPageKey: BmsSnapshot] = [:]
    private var deviceDetectionSession = DeviceDetectionSession()
    private var pendingBegodeProbeResponses = Set<DeviceDetectionPendingProbe>()

    public override init() {}

    public func start() {
        guard central == nil else {
            return
        }
        central = CBCentralManager(delegate: self, queue: nil)
    }

    func observeAdvertisement(_ advertisement: CoreBluetoothAdvertisement) {
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        let snapshot = rustSessionState.observeDiscovery(observation: DiscoveryObservation(advertisement))
        scanState = DevicePickerScanState(status: .scanning, discoverySnapshot: snapshot)
        onScanStateChange?(scanState)
    }

    @discardableResult
    public func pair(platformIdentifier: String) -> Bool {
        let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
        let snapshot = rustSessionState.selectDiscoveredPlatform(platformIdentifier: platformIdentifier)
        let advertisement = snapshot.advertisement(platformIdentifier: platformIdentifier)
        let model = snapshot.pickerCandidates
            .first { $0.platformIdentifier == platformIdentifier }
            .map(DevicePickerDiscoveryCandidate.init(candidate:))?
            .support
            .electricUnicycleModel
        return connectIfReady(
            peripheral: discoveredPeripherals[identifier],
            advertisement: advertisement,
            model: model
        )
    }

    @discardableResult
    public func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool {
        let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
        let snapshot = rustSessionState.selectDiscoveredPlatform(platformIdentifier: platformIdentifier)
        return connectIfReady(
            peripheral: discoveredPeripherals[identifier],
            advertisement: snapshot.advertisement(platformIdentifier: platformIdentifier),
            model: model
        )
    }

    @discardableResult
    public func recordOnly(platformIdentifier: String, note: String? = nil, annotations: [String] = []) -> Bool {
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

    public func annotateCapture(label: String) {
        captureBuilder?.addAnnotation(annotation: "capture_label=\(label)")
        record("capture_label=\(label)")
        writeCapture()
    }

    public func disconnectAndScan() {
        suppressReconnect = true
        isRecordOnly = false
        selectedModel = nil
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
        onDisplayStateChange?(displayState)

        if let peripheral {
            central?.cancelPeripheralConnection(peripheral)
        }
        peripheral = nil
        advertisement = nil

        scanState = DevicePickerScanState(status: .scanning, discoverySnapshot: rustSessionState.discoverySnapshot())
        onScanStateChange?(scanState)
        setPhase(.scanning)
        central?.scanForPeripherals(withServices: nil)
    }

    func applyLinkUpStep(_ step: CoreBluetoothSessionStep) {
        record("link_operations=\(step.operations.map(String.init(describing:)).joined(separator: ","))")
        captureBuilder?.recordLinkUp(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
            maxWriteLen: peripheral.map {
                MobileTransportWriteLimitDto(bytes: UInt16(clamping: $0.maximumWriteValueLength(for: .withoutResponse)))
            }
        )
        writeCapture()
        if let snapshot = step.snapshot {
            hasObservedSpeedSnapshot = snapshot.speed?.value != nil
        }
        setPhase(.subscribing)
    }

    func applyNotificationStep(_ step: CoreBluetoothSessionStep, receivedAt: MonotonicMilliseconds) {
        step.actions.forEach(applySessionAction)
        displayState = displayState.reducing(step, receivedAt: receivedAt)
        hasObservedSpeedSnapshot = hasObservedSpeedSnapshot || step.snapshot?.speed?.value != nil
        onDisplayStateChange?(displayState)
        setPhase(.live)
    }

    private func applySessionAction(_ action: SessionAction) {
        switch action.kind {
        case .settingsReadback:
            settingsReadback = action.settingsReadback
            onSettingsReadbackChange?(settingsReadback)
        case .faultHistoryReadback:
            faultHistoryReadback = action.faultHistoryReadback
            onFaultHistoryReadbackChange?(faultHistoryReadback)
        case .bmsSnapshot:
            let mergedSnapshot = mergedBmsSnapshot(with: action.bmsSnapshot)
            guard mergedSnapshot != bmsSnapshot else {
                return
            }
            bmsSnapshot = mergedSnapshot
            onBmsSnapshotChange?(bmsSnapshot)
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
                displayName: advertisement.localName ?? "Veteran/NOSFET device",
                modelId: modelId
            )
            protocolIdentityCandidate = DevicePickerDiscoveryCandidate(candidate: candidate)
            onProtocolIdentityCandidateChange?(protocolIdentityCandidate)
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
        onSettingsReadbackChange?(nil)
    }

    private func clearFaultHistoryReadback() {
        guard faultHistoryReadback != nil else {
            return
        }
        faultHistoryReadback = nil
        onFaultHistoryReadbackChange?(nil)
    }

    private func clearBmsSnapshot() {
        guard bmsSnapshot != nil else {
            return
        }
        bmsSnapshot = nil
        bmsPages.removeAll()
        onBmsSnapshotChange?(nil)
    }

    private func clearProtocolIdentityCandidate() {
        guard protocolIdentityCandidate != nil else {
            return
        }
        protocolIdentityCandidate = nil
        onProtocolIdentityCandidateChange?(nil)
    }

    private func publishDetectionIdentityCandidate(_ resolution: DeviceDetectionResolution) {
        guard resolution.protocolFamily != nil
            || resolution.protocolConflict
            || resolution.modelBanner != nil
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
            displayName: advertisement.localName ?? "Begode device"
        )
        guard candidate.isPickerCandidate else {
            return
        }
        let pickerCandidate = DevicePickerDiscoveryCandidate(candidate: candidate)
        guard pickerCandidate != protocolIdentityCandidate else {
            return
        }
        protocolIdentityCandidate = pickerCandidate
        onProtocolIdentityCandidateChange?(protocolIdentityCandidate)
        record("protocol_identity=\(candidate.detail)")
        updateCaptureIdentity()
    }

    private func setPhase(_ phase: SessionConnectionPhase) {
        self.phase = phase
        onPhaseChange?(phase)
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
        liveOwner = nil
        deviceDetectionSession = DeviceDetectionSession()
        _ = deviceDetectionSession.observeAdvertisement(name: advertisement.localName.map { Data($0.utf8) })
        startCapture(reason: "record-only", annotations: ["route=record_only"] + annotations + (note.map { ["user_note=\($0)"] } ?? []))
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
        model: ElectricUnicycleModel?
    ) -> Bool {
        switch (peripheral, advertisement, model) {
        case let (.some(peripheral), .some(advertisement), .some(model)):
            connect(to: peripheral, using: advertisement, model: model)
            return true
        case (.none, _, _), (_, .none, _), (_, _, .none):
            return false
        }
    }

    private func buildOwner(for peripheral: CBPeripheral) {
        guard liveOwner == nil, let advertisement, let selectedModel else {
            return
        }
        do {
            liveOwner = CoreBluetoothLiveSessionOwner(
                session: try .electricUnicycle(model: selectedModel),
                advertisement: advertisement,
                writeLimit: TransportWriteLimitBytes(23),
                operationSink: self
            )
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

    private func handleDisconnect(from peripheral: CBPeripheral, error: Error?) {
        record("disconnected=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
        guard self.peripheral?.identifier == peripheral.identifier else {
            return
        }
        markOutstandingBegodeProbeResponsesMissing()
        captureBuilder?.recordLinkDown(monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()))
        writeCapture()
        let wasRecordOnly = isRecordOnly
        isRecordOnly = false
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

        guard let selectedModel else {
            setPhase(.failed(.connectFailed(error.sessionMessage)))
            return
        }

        setPhase(.connecting(model: selectedModel))
        startCapture(reason: "reconnect", annotations: ["route=electric_unicycle"])
        central?.connect(peripheral)
    }

    private func record(_ message: String) {
        records.append(message)
        onRecord?(message)
    }

    private func captureFrame(direction: String, characteristic: CBUUID, bytes: Data) {
        guard let channel = BluetoothUuid(coreBluetoothUuid: characteristic) else {
            return
        }

        switch direction {
        case "notify":
            guard let service = pevcapServiceUuid(for: channel) else {
                return
            }
            captureBuilder?.recordNotification(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                characteristic: channel.bytes,
                service: service.bytes,
                bytes: bytes
            )
        case "write_without_response":
            captureBuilder?.recordWriteWithoutResponse(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                characteristic: channel.bytes,
                bytes: bytes
            )
        default:
            return
        }

        writeCapture()
    }

    private func startCapture(reason: String, annotations extraAnnotations: [String] = []) {
        try? captureHandle?.close()
        captureHandle = nil
        captureStartedAt = clock.now()
        didRecordCaptureFile = false

        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        captureURL = directory.appendingPathComponent("cutout-btle-capture-\(Int(Date().timeIntervalSince1970)).jsonl")
        let builder = MobilePevcapCaptureBuilder(
            wallClockStartUnixMs: MobileWallClockUnixMillisDto(milliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)),
            platformId: advertisement?.peripheralIdentifier.rawValue ?? "ios",
            writeLimit: MobileTransportWriteLimitDto(bytes: 23)
        )
        (advertisement?.advertisedServiceUuids ?? []).forEach { service in
            builder.addAdvertisedService(service: service.bytes)
        }
        [
            "source=ios-app",
            "capture_reason=\(reason)",
            "capture_privacy=private",
            "capture_evidence=hardware_tested",
        ].forEach { builder.addAnnotation(annotation: $0) }
        extraAnnotations.forEach { builder.addAnnotation(annotation: $0) }
        captureBuilder = builder
        updateCaptureIdentity()
        writeCapture()
    }

    private func writeCapture() {
        do {
            guard let builder = captureBuilder else { return }
            let url = captureURL ?? FileManager.default
                .urls(for: .documentDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("cutout-btle-capture-\(Int(Date().timeIntervalSince1970)).jsonl")
            captureURL = url
            let bytes = try builder.export(encoding: .jsonl)
            try bytes.write(to: url, options: [.atomic])
            if !didRecordCaptureFile {
                didRecordCaptureFile = true
                record("capture_file=\(url.path)")
            }
        } catch {
            record("capture_error=\(error)")
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
        builder.setResolvedIdentity(identity: identity)
        if let protocolIdentityCandidate {
            builder.addAnnotation(annotation: "resolved_evidence=\(protocolIdentityCandidate.evidence)")
            builder.addAnnotation(annotation: "resolved_detail=\(protocolIdentityCandidate.detail)")
        }
        writeCapture()
    }

    private func pevcapResolvedIdentity() -> MobileResolvedIdentityDto? {
        captureResolvedIdentity(
            protocolIdentityCandidate: protocolIdentityCandidate,
            selectedModel: selectedModel
        )
    }

    private func pevcapServiceUuid(for characteristic: BluetoothUuid) -> BluetoothUuid? {
        guard let shortUuid = characteristic.bluetooth16Value else {
            return nil
        }
        switch shortUuid {
        case 0xffe1:
            return .bluetooth16(0xffe0)
        case 0xfff1:
            return .bluetooth16(0xfff0)
        default:
            return nil
        }
    }
}

private extension UInt64 {
    func saturatingSubtracting(_ other: UInt64) -> UInt64 {
        self >= other ? self - other : 0
    }
}

func captureResolvedIdentity(
    protocolIdentityCandidate: DevicePickerDiscoveryCandidate?,
    selectedModel _: ElectricUnicycleModel?
) -> MobileResolvedIdentityDto? {
    protocolIdentityCandidate?
        .support
        .electricUnicycleModel?
        .pevcapResolvedIdentity(verification: .hardwareVerified)
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

extension CutoutSessionCore: CBCentralManagerDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
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
        setPhase(.discoveringServices)
        peripheral.delegate = self
        if isRecordOnly {
            captureBuilder?.recordLinkUp(
                monotonicMs: MobileMonotonicMillisDto(milliseconds: captureElapsedMilliseconds()),
                maxWriteLen: MobileTransportWriteLimitDto(bytes: UInt16(clamping: peripheral.maximumWriteValueLength(for: .withoutResponse)))
            )
            writeCapture()
        }
        peripheral.discoverServices(isRecordOnly ? nil : CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids)
    }

    public func centralManager(_: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        record("connect_failed=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
        isRecordOnly = false
        setPhase(.failed(.connectFailed(error.sessionMessage)))
    }

    public func centralManager(
        _: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        handleDisconnect(from: peripheral, error: error)
    }
}

extension CutoutSessionCore: CBPeripheralDelegate {
    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
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
        captureFrame(direction: "notify", characteristic: characteristic.uuid, bytes: value)
        observeDetectionNotification(channel: channel, bytes: value)
        if isRecordOnly {
            record("record_only_notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            return
        }
        guard let liveOwner else {
            return
        }
        do {
            let receivedAt = clock.now()
            let step = try liveOwner.handleNotification(
                bytes: value,
                channel: channel,
                at: receivedAt
            )
            record("notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            record("speed=\(step.snapshot?.speed.map { String($0.value) } ?? "nil")")
            record("voltage=\(step.snapshot?.voltage.map { String($0.value) } ?? "nil")")
            record("battery_estimated=\(step.snapshot?.batteryLevelEstimated.map { String($0.value) } ?? "nil")")
            record("live_records=\(liveOwner.records.count)")
            applyNotificationStep(step, receivedAt: receivedAt)
        } catch {
            record("notification_ingest_error=\(error)")
            setPhase(.failed(.notificationIngestFailed(error.sessionMessage)))
        }
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
        guard isReadOnlyBegodeProbeWrite(channel: channel, bytes: bytes) else {
            setPhase(.failed(.skippedReadOnlyWrite))
            return
        }
        guard let characteristic = subscribedCharacteristics[channel] else {
            setPhase(.failed(.missingNotifyChannel))
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
        service.characteristics?.forEach { characteristic in
            guard let characteristicUuid = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) else {
                return
            }
            captureBuilder?.addGattFingerprint(fingerprint: MobileGattFingerprintDto(
                service: serviceUuid.bytes,
                characteristic: characteristicUuid.bytes,
                roles: characteristic.mobileGattRoles,
                verification: .hardwareVerified
            ))
        }
        writeCapture()
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
        captureBuilder?.addAnnotation(annotation: annotation)
        record(annotation)
        writeCapture()
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
