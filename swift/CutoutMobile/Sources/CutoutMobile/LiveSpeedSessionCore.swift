import CoreBluetooth
import Foundation

public enum LiveSpeedSessionLifecycleStep: Equatable, Sendable {
    case connecting(model: ElectricUnicycleModel, platformIdentifier: String)
    case discoveringServices([String])
    case subscribing([String])
}

public final class LiveSpeedSessionCore: NSObject {
    public private(set) var displayState = LiveSpeedDisplayState()
    public private(set) var phase = LiveSpeedConnectionPhase.starting
    public private(set) var records: [String] = []
    public private(set) var hasObservedSpeedSnapshot = false
    public private(set) var scanState = DevicePickerScanState(status: .idle, rows: [])

    public var onDisplayStateChange: ((LiveSpeedDisplayState) -> Void)?
    public var onPhaseChange: ((LiveSpeedConnectionPhase) -> Void)?
    public var onRecord: ((String) -> Void)?
    public var onScanStateChange: ((DevicePickerScanState) -> Void)?

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

    public func applyLifecycleStep(_ step: LiveSpeedSessionLifecycleStep) {
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

    public func disconnectAndScan() {
        suppressReconnect = true
        selectedModel = nil
        liveOwner = nil
        subscribedCharacteristics.removeAll()
        pendingServiceDiscoveries.removeAll()
        displayState = LiveSpeedDisplayState()
        hasObservedSpeedSnapshot = false
        onDisplayStateChange?(displayState)

        if let peripheral {
            central?.cancelPeripheralConnection(peripheral)
        }
        peripheral = nil
        advertisement = nil

        scanState = DevicePickerScanState(status: .scanning, advertisements: discoveredAdvertisements)
        onScanStateChange?(scanState)
        setPhase(.scanning(model: .aero))
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
        displayState = displayState.reducing(step, receivedAt: receivedAt)
        hasObservedSpeedSnapshot = hasObservedSpeedSnapshot || step.snapshot?.speed?.value != nil
        record("display_speed=\(displayState.speed.displayValue) display_unit=\(displayState.speed.displayUnit)")
        onDisplayStateChange?(displayState)
        setPhase(.live)
    }

    public func timeoutDiagnosticRecords(timeoutSeconds: Int) -> [String] {
        [
            "timeout_seconds=\(timeoutSeconds)",
            "phase=\(phase)",
            "candidate_count=\(scanState.rows.count)",
            "selected_model=\(timeoutDiagnosticModelName)",
            "blocker=\(timeoutDiagnosticBlocker)",
        ]
    }

    private func setPhase(_ phase: LiveSpeedConnectionPhase) {
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
            setPhase(.failed(.sessionFailed(error.liveSpeedMessage)))
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
            setPhase(.failed(.connectFailed(error.liveSpeedMessage)))
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

extension LiveSpeedSessionCore: CBCentralManagerDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        record("central_state=\(central.state.rawValue)")
        guard central.state == .poweredOn else {
            setPhase(.bluetoothUnavailable(rawState: central.state.rawValue))
            return
        }
        setPhase(.scanning(model: .aero))
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
        observeAdvertisement(advertisement)
        discoveredPeripherals[advertisement.peripheralIdentifier] = peripheral
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
        setPhase(.failed(.connectFailed(error.liveSpeedMessage)))
    }

    public func centralManager(
        _: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        handleDisconnect(from: peripheral, error: error)
    }
}

extension LiveSpeedSessionCore: CBPeripheralDelegate {
    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        if let error {
            setPhase(.failed(.serviceDiscoveryFailed(error.liveSpeedMessage)))
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
            setPhase(.failed(.characteristicDiscoveryFailed(error.liveSpeedMessage)))
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
            setPhase(.failed(.notificationFailed(error.liveSpeedMessage)))
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
            record("speed_mm_s=\(step.snapshot?.speed.map { String($0.value) } ?? "nil")")
            record("voltage_mv=\(step.snapshot?.voltage.map { String($0.value) } ?? "nil")")
            record("battery_estimated=\(step.snapshot?.batteryLevelEstimated.map { String($0.value) } ?? "nil")")
            record("live_records=\(liveOwner.records.count)")
            applyNotificationStep(step, receivedAt: receivedAt)
        } catch {
            record("notification_ingest_error=\(error)")
            setPhase(.failed(.notificationIngestFailed(error.liveSpeedMessage)))
        }
    }
}

extension LiveSpeedSessionCore: CoreBluetoothOperationSink {
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
    var liveSpeedMessage: String {
        map(String.init(describing:)) ?? "unknown error"
    }
}

private extension Error {
    var liveSpeedMessage: String {
        String(describing: self)
    }
}
