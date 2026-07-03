import CoreBluetooth
import Foundation

<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
public final class CutoutSessionCore: NSObject {
    public private(set) var displayState = RideDisplayState()
    public private(set) var phase = SessionConnectionPhase.starting
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
public final class LiveSpeedSessionCore: NSObject {
    public private(set) var displayState = LiveSpeedDisplayState()
    public private(set) var phase = LiveSpeedConnectionPhase.starting
========
public enum LiveRideSessionLifecycleStep: Equatable, Sendable {
    case connecting(model: ElectricUnicycleModel, platformIdentifier: String)
    case discoveringServices([String])
    case subscribing([String])
}

public final class LiveRideSessionCore: NSObject {
    public private(set) var displayState = LiveRideDisplayState()
    public private(set) var phase = LiveRideConnectionPhase.starting
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
    public private(set) var records: [String] = []
    public private(set) var hasObservedSpeedSnapshot = false
    public private(set) var scanState = DevicePickerScanState(status: .idle, rows: [])
    public private(set) var settingsReadback: SettingsReadback?
    public private(set) var faultHistoryReadback: FaultHistoryReadback?
    public private(set) var bmsSnapshot: BmsSnapshot?
    public private(set) var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?

<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
    public var onDisplayStateChange: ((RideDisplayState) -> Void)?
    public var onPhaseChange: ((SessionConnectionPhase) -> Void)?
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
    public var onDisplayStateChange: ((LiveSpeedDisplayState) -> Void)?
    public var onPhaseChange: ((LiveSpeedConnectionPhase) -> Void)?
========
    public var onDisplayStateChange: ((LiveRideDisplayState) -> Void)?
    public var onPhaseChange: ((LiveRideConnectionPhase) -> Void)?
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
    public var onRecord: ((String) -> Void)?
    public var onScanStateChange: ((DevicePickerScanState) -> Void)?
    public var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    public var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    public var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    public var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?

    private let clock = MonotonicClock()
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var advertisement: CoreBluetoothAdvertisement?
    private var discoveredAdvertisements: [CoreBluetoothAdvertisement] = []
    private var discoveredPeripherals: [CoreBluetoothPeripheralIdentifier: CBPeripheral] = [:]
    private var liveOwner: CoreBluetoothLiveSessionOwner?
    private var selectedModel: ElectricUnicycleModel?
    private var subscribedCharacteristics: [BluetoothUuid: CBCharacteristic] = [:]
    private var pendingServiceDiscoveries = Set<CBUUID>()
    private var suppressReconnect = false

    public override init() {}

    deinit {}

    public func start() {
        guard central == nil else {
            return
        }
        central = CBCentralManager(delegate: self, queue: nil)
    }

    func observeAdvertisement(_ advertisement: CoreBluetoothAdvertisement) {
        discoveredAdvertisements.removeAll { $0.peripheralIdentifier == advertisement.peripheralIdentifier }
        discoveredAdvertisements.append(advertisement)
        scanState = DevicePickerScanState(status: .scanning, advertisements: discoveredAdvertisements)
        onScanStateChange?(scanState)
    }

    @discardableResult
    public func pair(platformIdentifier: String) -> Bool {
        let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
        guard
            let peripheral = discoveredPeripherals[identifier],
            let advertisement = discoveredAdvertisements.last(where: { $0.peripheralIdentifier == identifier }),
            let model = DevicePickerDiscoveryCandidate(advertisement: advertisement).support.electricUnicycleModel
        else {
            return false
        }
        connect(to: peripheral, using: advertisement, model: model)
        return true
    }

<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
    @discardableResult
    public func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool {
        let identifier = CoreBluetoothPeripheralIdentifier(platformIdentifier)
        guard
            let peripheral = discoveredPeripherals[identifier],
            let advertisement = discoveredAdvertisements.last(where: { $0.peripheralIdentifier == identifier })
        else {
            return false
        }
        connect(to: peripheral, using: advertisement, model: model)
        return true
    }

|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
========
    public func applyLifecycleStep(_ step: LiveRideSessionLifecycleStep) {
        switch step {
        case let .connecting(model, platformIdentifier):
            selectedModel = model
            setPhase(.connecting(model: model))
            record("connect_model=\(model.displayName) platform_identifier=\(platformIdentifier)")
        case let .discoveringServices(services):
            setPhase(.discoveringServices)
            record("services=\(services.joined(separator: ","))")
        case let .subscribing(channels):
            setPhase(.subscribing)
            record("subscribe_channels=\(channels.joined(separator: ","))")
        }
    }

>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
    public func disconnectAndScan() {
        suppressReconnect = true
        selectedModel = nil
        liveOwner = nil
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
        displayState = RideDisplayState()
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
        displayState = LiveSpeedDisplayState()
========
        displayState = LiveRideDisplayState()
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
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

        scanState = DevicePickerScanState(status: .scanning, advertisements: discoveredAdvertisements)
        onScanStateChange?(scanState)
        setPhase(.scanning)
        central?.scanForPeripherals(withServices: nil)
    }

    func applyLinkUpStep(_ step: CoreBluetoothSessionStep) {
        record("link_operations=\(step.operations.map(String.init(describing:)).joined(separator: ","))")
        if let snapshot = step.snapshot {
            hasObservedSpeedSnapshot = snapshot.speed?.value != nil
        }
        setPhase(.subscribing)
    }

    func applyNotificationStep(_ step: CoreBluetoothSessionStep, receivedAt: MonotonicMilliseconds) {
        step.actions.forEach(applySessionAction)
        displayState = displayState.reducing(step, receivedAt: receivedAt)
        hasObservedSpeedSnapshot = hasObservedSpeedSnapshot || step.snapshot?.speed?.value != nil
        record("display_speed=\(displayState.speed.displayValue) display_unit=\(displayState.speed.displayUnit)")
        onDisplayStateChange?(displayState)
        setPhase(.live)
    }

<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
    private func applySessionAction(_ action: SessionAction) {
        switch action.kind {
        case .settingsReadback:
            settingsReadback = action.settingsReadback
            onSettingsReadbackChange?(settingsReadback)
        case .faultHistoryReadback:
            faultHistoryReadback = action.faultHistoryReadback
            onFaultHistoryReadbackChange?(faultHistoryReadback)
        case .bmsSnapshot:
            bmsSnapshot = action.bmsSnapshot
            onBmsSnapshotChange?(bmsSnapshot)
        case .event:
            applyProtocolIdentityModelId(action.veteranProtocolModelId)
        case .subscribe, .write, .disconnect, .notificationIngest:
            break
        }
    }

    private func applyProtocolIdentityModelId(_ modelId: UInt16?) {
        guard let modelId, let advertisement = advertisement ?? discoveredAdvertisements.last else {
            return
        }
        let candidate = mobileDiscoveryCandidateFromVeteranProtocolIdentity(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            displayName: advertisement.localName ?? "Veteran/NOSFET device",
            modelId: modelId
        )
        protocolIdentityCandidate = DevicePickerDiscoveryCandidate(candidate: candidate)
        onProtocolIdentityCandidateChange?(protocolIdentityCandidate)
        record("protocol_identity=\(candidate.detail)")
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
        onBmsSnapshotChange?(nil)
    }

    private func clearProtocolIdentityCandidate() {
        guard protocolIdentityCandidate != nil else {
            return
        }
        protocolIdentityCandidate = nil
        onProtocolIdentityCandidateChange?(nil)
    }

    private func setPhase(_ phase: SessionConnectionPhase) {
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
    private func setPhase(_ phase: LiveSpeedConnectionPhase) {
========
    public func timeoutDiagnosticRecords(timeoutSeconds: Int) -> [String] {
        [
            "timeout_seconds=\(timeoutSeconds)",
            "phase=\(phase)",
            "candidate_count=\(scanState.rows.count)",
            "selected_model=\(timeoutDiagnosticModelName)",
            "blocker=\(timeoutDiagnosticBlocker)",
        ]
    }

    private func setPhase(_ phase: LiveRideConnectionPhase) {
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
        self.phase = phase
        onPhaseChange?(phase)
    }

    private func connect(
        to peripheral: CBPeripheral,
        using advertisement: CoreBluetoothAdvertisement,
        model: ElectricUnicycleModel
    ) {
        suppressReconnect = false
        self.peripheral = peripheral
        self.advertisement = advertisement
        selectedModel = model
        clearSettingsReadback()
        clearFaultHistoryReadback()
        clearBmsSnapshot()
        clearProtocolIdentityCandidate()
        peripheral.delegate = self
        setPhase(.connecting(model: model))
        central?.stopScan()
        central?.connect(peripheral)
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
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
            setPhase(.failed(.sessionFailed(error.sessionMessage)))
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
            setPhase(.failed(.sessionFailed(error.liveSpeedMessage)))
========
            setPhase(.failed(.sessionFailed(error.liveRideMessage)))
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
        }
    }

    private func handleDisconnect(from peripheral: CBPeripheral, error: Error?) {
        record("disconnected=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
        guard self.peripheral?.identifier == peripheral.identifier else {
            return
        }
        liveOwner = nil
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()

        guard !suppressReconnect else {
            suppressReconnect = false
            return
        }

        guard let selectedModel else {
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
            setPhase(.failed(.connectFailed(error.sessionMessage)))
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
            setPhase(.failed(.connectFailed(error.liveSpeedMessage)))
========
            setPhase(.failed(.connectFailed(error.liveRideMessage)))
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
            return
        }

        setPhase(.connecting(model: selectedModel))
        central?.connect(peripheral)
    }

    private func record(_ message: String) {
        records.append(message)
        onRecord?(message)
    }

    private var timeoutDiagnosticModelName: String {
        switch phase {
        case .scanning(let model), .connecting(let model):
            model.displayName
        default:
            selectedModel?.displayName ?? "unknown"
        }
    }

    private var timeoutDiagnosticBlocker: String {
        switch phase {
        case .starting:
            "bluetooth_pending"
        case .bluetoothUnavailable:
            "bluetooth_unavailable"
        case .scanning:
            scanState.rows.isEmpty ? "no_candidate" : "discovered_no_connect"
        case .connecting, .discoveringServices:
            "discovered_no_connect"
        case .subscribing:
            "connected_no_telemetry"
        case .live:
            hasObservedSpeedSnapshot ? "speed_observed" : "parsed_no_speed"
        case .failed:
            "code_failure"
        }
    }
}

<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
extension CutoutSessionCore: CBCentralManagerDelegate {
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
extension LiveSpeedSessionCore: CBCentralManagerDelegate {
========
extension LiveRideSessionCore: CBCentralManagerDelegate {
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
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
        peripheral.discoverServices(CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids)
    }

    public func centralManager(_: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        record("connect_failed=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
        setPhase(.failed(.connectFailed(error.sessionMessage)))
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
        setPhase(.failed(.connectFailed(error.liveSpeedMessage)))
========
        setPhase(.failed(.connectFailed(error.liveRideMessage)))
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
    }

    public func centralManager(
        _: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        handleDisconnect(from: peripheral, error: error)
    }
}

<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
extension CutoutSessionCore: CBPeripheralDelegate {
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
extension LiveSpeedSessionCore: CBPeripheralDelegate {
========
extension LiveRideSessionCore: CBPeripheralDelegate {
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        if let error {
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
            setPhase(.failed(.serviceDiscoveryFailed(error.sessionMessage)))
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
            setPhase(.failed(.serviceDiscoveryFailed(error.liveSpeedMessage)))
========
            setPhase(.failed(.serviceDiscoveryFailed(error.liveRideMessage)))
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
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
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
            setPhase(.failed(.characteristicDiscoveryFailed(error.sessionMessage)))
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
            setPhase(.failed(.characteristicDiscoveryFailed(error.liveSpeedMessage)))
========
            setPhase(.failed(.characteristicDiscoveryFailed(error.liveRideMessage)))
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
            return
        }
        service.characteristics?.forEach { characteristic in
            if let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) {
                subscribedCharacteristics[channel] = characteristic
            }
        }
        pendingServiceDiscoveries.remove(service.uuid)
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
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
            setPhase(.failed(.notificationFailed(error.sessionMessage)))
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
            setPhase(.failed(.notificationFailed(error.liveSpeedMessage)))
========
            setPhase(.failed(.notificationFailed(error.liveRideMessage)))
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
            return
        }
        guard
            let value = characteristic.value,
            let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid),
            let liveOwner
        else {
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
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
            setPhase(.failed(.notificationIngestFailed(error.sessionMessage)))
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
            setPhase(.failed(.notificationIngestFailed(error.liveSpeedMessage)))
========
            setPhase(.failed(.notificationIngestFailed(error.liveRideMessage)))
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
        }
    }
}

<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
extension CutoutSessionCore: CoreBluetoothOperationSink {
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
extension LiveSpeedSessionCore: CoreBluetoothOperationSink {
========
extension LiveRideSessionCore: CoreBluetoothOperationSink {
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
    public func subscribe(channel: BluetoothUuid) {
        guard let characteristic = subscribedCharacteristics[channel] else {
            setPhase(.failed(.missingNotifyChannel))
            return
        }
        peripheral?.setNotifyValue(true, for: characteristic)
    }

    public func writeWithoutResponse(channel _: BluetoothUuid, bytes _: Data) {
        setPhase(.failed(.skippedReadOnlyWrite))
    }

    public func disconnect() {
        guard let peripheral else {
            return
        }
        central?.cancelPeripheralConnection(peripheral)
    }
}

private struct MonotonicClock {
    private let base = Date()

    func now() -> MonotonicMilliseconds {
        MonotonicMilliseconds(UInt64(Date().timeIntervalSince(base) * 1_000))
    }
}

private extension Optional where Wrapped == Error {
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
    var sessionMessage: String {
        map(String.init(describing:)) ?? "unknown error"
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
    var liveSpeedMessage: String {
        map(String.init(describing:)) ?? "unknown error"
========
    var liveRideMessage: String {
        self?.liveRideMessage ?? "unknown error"
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
    }
}

private extension Error {
<<<<<<<< HEAD:swift/CutoutMobile/Sources/CutoutMobile/CutoutSessionCore.swift
    var sessionMessage: String {
|||||||| 2bdc2c8e:swift/CutoutMobile/Sources/CutoutMobile/LiveSpeedSessionCore.swift
    var liveSpeedMessage: String {
========
    var liveRideMessage: String {
>>>>>>>> mjc/libcu-doc1-hardening:swift/CutoutMobile/Sources/CutoutMobile/LiveRideSessionCore.swift
        String(describing: self)
    }
}
