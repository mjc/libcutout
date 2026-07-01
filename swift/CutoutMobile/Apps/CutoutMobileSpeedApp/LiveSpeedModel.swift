import CoreBluetooth
import CutoutMobile
import Foundation

final class LiveSpeedModel: NSObject, ObservableObject {
    @Published private(set) var displayState = LiveSpeedDisplayState()
    @Published private(set) var status = "Starting Bluetooth..."

    var speed: SpeedReadout {
        displayState.speed
    }

    private let clock = MonotonicClock()
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var advertisement: CoreBluetoothAdvertisement?
    private var liveOwner: CoreBluetoothLiveSessionOwner?
    private var subscribedCharacteristics: [BluetoothUuid: CBCharacteristic] = [:]
    private var pendingServiceDiscoveries = Set<CBUUID>()

    func start() {
        guard central == nil else {
            return
        }
        central = CBCentralManager(delegate: self, queue: nil)
    }

    private func setStatus(_ status: String) {
        self.status = status
    }

    private func connect(to peripheral: CBPeripheral, using advertisement: CoreBluetoothAdvertisement) {
        self.peripheral = peripheral
        self.advertisement = advertisement
        peripheral.delegate = self
        setStatus("Connecting to Aero...")
        central?.stopScan()
        central?.connect(peripheral)
    }

    private func buildOwner(for peripheral: CBPeripheral) {
        guard liveOwner == nil, let advertisement else {
            return
        }
        do {
            liveOwner = CoreBluetoothLiveSessionOwner(
                session: try .electricUnicycle(model: .aero),
                advertisement: advertisement,
                writeLimit: TransportWriteLimitBytes(23),
                operationSink: self
            )
            setStatus("Subscribing...")
            let inventory = CoreBluetoothGattInventory(services: peripheral.services ?? [])
            liveOwner?.recordInventory(inventory)
            _ = try liveOwner?.handleLinkUp(at: clock.now())
        } catch {
            setStatus("Session failed: \(error)")
        }
    }
}

extension LiveSpeedModel: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        guard central.state == .poweredOn else {
            setStatus("Bluetooth unavailable: state \(central.state.rawValue)")
            return
        }
        setStatus("Scanning for Aero...")
        central.scanForPeripherals(withServices: CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids)
    }

    func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi _: NSNumber
    ) {
        let advertisement = CoreBluetoothAdvertisement(
            peripheral: peripheral,
            advertisementData: advertisementData
        )
        guard advertisement.modelHint == .aero || advertisement.advertisedServiceUuids.contains(.bluetooth16(0xffe0)) else {
            return
        }
        connect(to: peripheral, using: advertisement)
    }

    func centralManager(_: CBCentralManager, didConnect peripheral: CBPeripheral) {
        setStatus("Discovering services...")
        peripheral.delegate = self
        peripheral.discoverServices(CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids)
    }

    func centralManager(_: CBCentralManager, didFailToConnect _: CBPeripheral, error: Error?) {
        setStatus("Connect failed: \(error.map(String.init(describing:)) ?? "unknown error")")
    }
}

extension LiveSpeedModel: CBPeripheralDelegate {
    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        if let error {
            setStatus("Service discovery failed: \(error)")
            return
        }
        let services = peripheral.services ?? []
        pendingServiceDiscoveries = Set(services.map(\.uuid))
        services.forEach {
            peripheral.discoverCharacteristics(nil, for: $0)
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        if let error {
            setStatus("Characteristic discovery failed: \(error)")
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

    func peripheral(
        _: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        if let error {
            setStatus("Notification failed: \(error)")
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
            if step.snapshot != nil {
                displayState = displayState.reducing(step, receivedAt: receivedAt)
                setStatus("Live")
            }
        } catch {
            setStatus("Notification ingest failed: \(error)")
        }
    }
}

extension LiveSpeedModel: CoreBluetoothOperationSink {
    func subscribe(channel: BluetoothUuid) {
        guard let characteristic = subscribedCharacteristics[channel] else {
            setStatus("Missing notify channel")
            return
        }
        peripheral?.setNotifyValue(true, for: characteristic)
    }

    func writeWithoutResponse(channel _: BluetoothUuid, bytes _: Data) {
        setStatus("Read-only MVP skipped a write operation")
    }

    func disconnect() {
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
