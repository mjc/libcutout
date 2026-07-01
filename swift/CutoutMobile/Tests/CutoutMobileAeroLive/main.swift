import CoreBluetooth
import CutoutMobile
import Foundation

@main
struct CutoutMobileAeroLive {
    static func main() {
        let timeoutSeconds = TimeInterval(
            CommandLine.arguments.dropFirst().first.flatMap(Double.init) ?? 45
        )
        let validator = AeroLiveValidator(timeout: timeoutSeconds)
        validator.start()
        exit(validator.didValidate ? EXIT_SUCCESS : EXIT_FAILURE)
    }
}

private final class AeroLiveValidator: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    private let timeout: TimeInterval
    private let startedAt = Date()
    private let clock = MonotonicClock()
    private var central: CBCentralManager!
    private var targetPeripheral: CBPeripheral?
    private var targetAdvertisement: CoreBluetoothAdvertisement?
    private var liveOwner: CoreBluetoothLiveSessionOwner?
    private var records: [String] = []
    private var observedCandidateIds = Set<String>()
    private(set) var didValidate = false

    init(timeout: TimeInterval) {
        self.timeout = timeout
        super.init()
    }

    func start() {
        central = CBCentralManager(delegate: self, queue: nil)
        while !didValidate, Date().timeIntervalSince(startedAt) < timeout {
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
        }
        if !didValidate {
            print("validation=timeout")
            printRecords()
        }
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        records.append("central_state=\(central.state.rawValue)")
        guard central.state == .poweredOn else {
            return
        }
        let services = CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids
        records.append("scan_services=\(services.map(\.uuidString).joined(separator: ","))")
        central.scanForPeripherals(withServices: services)
    }

    func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi: NSNumber
    ) {
        let advertisement = CoreBluetoothAdvertisement(
            peripheral: peripheral,
            advertisementData: advertisementData
        )
        let advertisedServices = advertisement.advertisedServiceUuids.map(\.hexString).joined(separator: ",")
        let candidate = [
            "candidate=\(advertisement.peripheralIdentifier.rawValue)",
            "name=\(advertisement.localName ?? "")",
            "model=\(advertisement.modelHint)",
            "services=\(advertisedServices)",
            "rssi=\(rssi)",
        ].joined(separator: " ")
        if observedCandidateIds.insert(advertisement.peripheralIdentifier.rawValue).inserted {
            records.append(candidate)
        }
        guard advertisement.modelHint == .aero || advertisement.advertisedServiceUuids.contains(.bluetooth16(0xffe0)) else {
            return
        }
        targetPeripheral = peripheral
        targetAdvertisement = advertisement
        peripheral.delegate = self
        records.append("discovered=\(advertisement.peripheralIdentifier.rawValue)")
        records.append("local_name=\(advertisement.localName ?? "")")
        records.append("rssi=\(rssi)")
        records.append("advertised_services=\(advertisedServices)")
        central.stopScan()
        central.connect(peripheral)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        records.append("connected=\(peripheral.identifier.uuidString)")
        peripheral.delegate = self
        peripheral.discoverServices(CoreBluetoothScanPolicy.aeroFalcon.coreBluetoothServiceUuids)
    }

    func centralManager(_: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        records.append("connect_failed=\(peripheral.identifier.uuidString) error=\(String(describing: error))")
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        if let error {
            records.append("service_error=\(error)")
            return
        }
        let services = peripheral.services ?? []
        records.append("services=\(services.map { $0.uuid.uuidString }.joined(separator: ","))")
        services.forEach { peripheral.discoverCharacteristics(nil, for: $0) }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        if let error {
            records.append("characteristic_error=\(service.uuid.uuidString) error=\(error)")
            return
        }
        let inventory = CoreBluetoothGattInventory(services: peripheral.services ?? [])
        records.append("inventory=\(inventory.summary)")
        guard liveOwner == nil, let advertisement = targetAdvertisement else {
            liveOwner?.recordInventory(inventory)
            return
        }
        let owner: CoreBluetoothLiveSessionOwner
        do {
            owner = CoreBluetoothLiveSessionOwner(
                session: try .electricUnicycle(model: .aero),
                advertisement: advertisement,
                peripheral: peripheral
            )
        } catch {
            records.append("session_error=\(error)")
            return
        }
        liveOwner = owner
        owner.recordInventory(inventory)
        do {
            let step = try owner.handleLinkUp(at: clock.now())
            records.append("link_operations=\(step.operations.map(\.summary).joined(separator: ","))")
        } catch {
            records.append("link_error=\(error)")
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        if let error {
            records.append("notification_error=\(characteristic.uuid.uuidString) error=\(error)")
            return
        }
        guard
            let value = characteristic.value,
            let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid),
            let owner = liveOwner
        else {
            return
        }
        do {
            let step = try owner.handleNotification(bytes: value, channel: channel, at: clock.now())
            records.append("notification=\(characteristic.uuid.uuidString) bytes=\(value.count)")
            records.append("voltage_mv=\(step.snapshot?.voltageMillivolts.map(String.init) ?? "nil")")
            records.append("battery_estimated=\(step.snapshot?.batteryLevelEstimated.map(String.init) ?? "nil")")
            records.append("live_records=\(owner.records.count)")
            didValidate = step.snapshot?.voltageMillivolts != nil
            if didValidate {
                central.cancelPeripheralConnection(peripheral)
                print("validation=ok")
                printRecords()
            }
        } catch {
            records.append("notification_ingest_error=\(error)")
        }
    }

    private func printRecords() {
        records.forEach { print($0) }
    }
}

private struct MonotonicClock {
    private let base = Date()

    func now() -> MonotonicMilliseconds {
        MonotonicMilliseconds(UInt64(Date().timeIntervalSince(base) * 1_000))
    }
}

private extension CoreBluetoothGattInventory {
    var summary: String {
        services.map { service in
            let characteristics = service.characteristics
                .map { "\($0.uuid.hexString):\($0.properties.map(\.label).sorted().joined(separator: "+"))" }
                .joined(separator: "|")
            return "\(service.uuid.hexString)[\(characteristics)]"
        }.joined(separator: ";")
    }
}

private extension CoreBluetoothCharacteristicProperty {
    var label: String {
        switch self {
        case .read:
            "read"
        case .write:
            "write"
        case .writeWithoutResponse:
            "writeWithoutResponse"
        case .notify:
            "notify"
        case .indicate:
            "indicate"
        }
    }
}

private extension CoreBluetoothPlannedOperation {
    var summary: String {
        switch self {
        case .subscribe(let channel):
            "subscribe:\(channel.hexString)"
        case .writeWithoutResponse(let channel, let bytes):
            "writeWithoutResponse:\(channel.hexString):\(bytes.count)"
        case .disconnect:
            "disconnect"
        }
    }
}

private extension BluetoothUuid {
    var hexString: String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }
}
