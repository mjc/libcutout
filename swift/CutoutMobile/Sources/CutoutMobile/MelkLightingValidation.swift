import CutoutMobileFFI
import Foundation

/// Failure while matching an observed standalone MELK controller to its typed profile.
public enum MelkLightingValidationError: Error, Equatable, Sendable {
    case missingService
    case missingWriteCharacteristic
    case missingNotificationCharacteristic
    case profileRejected
}

/// Explicit result state for a lighting command.
public enum MelkLightingCommandStatus: Equatable, Sendable {
    case idle
    case requested
    case confirmed
    case unconfirmed
}

/// Small state tracker used by the live validator; a write never self-confirms.
public struct MelkLightingCommandEvidence: Equatable, Sendable {
    public private(set) var status: MelkLightingCommandStatus = .idle

    public init() {}

    public mutating func requested() {
        status = .requested
    }

    public mutating func confirmed() {
        guard status == .requested else { return }
        status = .confirmed
    }

    public mutating func unconfirmed() {
        guard status == .requested else { return }
        status = .unconfirmed
    }
}

/// One Rust-owned MELK write ready for the existing CoreBluetooth operation sink.
public struct MelkLightingWritePlan: Equatable, Sendable {
    public let operation: CoreBluetoothPlannedOperation
    public let confirmationChannel: BluetoothUuid
    public let minimumIntervalMilliseconds: UInt16?

    public init(
        operation: CoreBluetoothPlannedOperation,
        confirmationChannel: BluetoothUuid,
        minimumIntervalMilliseconds: UInt16?
    ) {
        self.operation = operation
        self.confirmationChannel = confirmationChannel
        self.minimumIntervalMilliseconds = minimumIntervalMilliseconds
    }
}

/// Smallest reusable iPhone/CoreBluetooth seam for validating `MELK-OC21`.
///
/// Rust selects the profile and emits command bytes. This type only validates the observed GATT
/// roles and adapts those typed writes to the existing CoreBluetooth operation sink.
public struct MelkLightingValidationHarness: Sendable {
    public static let service = BluetoothUuid.bluetooth16(0xfff0)
    public static let write = BluetoothUuid.bluetooth16(0xfff3)
    public static let notify = BluetoothUuid.bluetooth16(0xfff4)

    private let profile: MobileMelkLightingProfile

    public let subscription: CoreBluetoothPlannedOperation

    public init(
        name: String,
        inventory: CoreBluetoothGattInventory
    ) throws {
        guard let serviceInventory = inventory.services.first(where: { $0.uuid == Self.service }) else {
            throw MelkLightingValidationError.missingService
        }
        guard let writeCharacteristic = serviceInventory.characteristics.first(where: {
            $0.uuid == Self.write && $0.properties.contains(.writeWithoutResponse)
        }) else {
            throw MelkLightingValidationError.missingWriteCharacteristic
        }
        guard let notifyCharacteristic = serviceInventory.characteristics.first(where: {
            $0.uuid == Self.notify && ($0.properties.contains(.notify) || $0.properties.contains(.indicate))
        }) else {
            throw MelkLightingValidationError.missingNotificationCharacteristic
        }

        do {
            profile = try MobileMelkLightingProfile(
                name: name,
                evidence: MobileMelkLightingGattEvidence(
                    servicePresent: true,
                    writeWithoutResponse: writeCharacteristic.properties.contains(.writeWithoutResponse),
                    notifyOrIndicate: notifyCharacteristic.properties.contains(.notify)
                        || notifyCharacteristic.properties.contains(.indicate)
                )
            )
        } catch {
            throw MelkLightingValidationError.profileRejected
        }
        subscription = .subscribe(channel: notifyCharacteristic.uuid)
    }

    public func setPower(_ on: Bool) -> MelkLightingWritePlan {
        plan(profile.setPower(on: on))
    }

    public func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> MelkLightingWritePlan {
        plan(profile.setSolidColor(red: red, green: green, blue: blue))
    }

    public func setBrightness(_ percentage: UInt8) throws -> MelkLightingWritePlan {
        plan(try profile.setBrightness(percentage: percentage))
    }

    private func plan(_ write: MobileMelkLightingWriteDto) -> MelkLightingWritePlan {
        precondition(write.mode == .withoutResponse)
        guard let channel = BluetoothUuid(write.characteristic),
              let confirmationChannel = BluetoothUuid(write.confirmationCharacteristic)
        else {
            preconditionFailure("Rust MELK writes contain fixed-width UUIDs")
        }
        return MelkLightingWritePlan(
            operation: .writeWithoutResponse(channel: channel, bytes: write.payload),
            confirmationChannel: confirmationChannel,
            minimumIntervalMilliseconds: write.minimumIntervalMs
        )
    }
}

#if canImport(CoreBluetooth)
import CoreBluetooth

/// Connection state for the independent standalone MELK validator.
public enum MelkLightingPeripheralState: Equatable, Sendable {
    case idle
    case scanning
    case connecting
    case retrying(attempt: Int, delayMilliseconds: UInt64)
    case discovering
    case ready
    case disconnected
    case failed(String)
}

/// A secondary CoreBluetooth connection for validating MELK without replacing a ride session.
///
/// The validator owns its own central manager, so it can remain connected while the primary
/// EUC/VESC central connection continues to receive telemetry. A command starts as `requested`
/// and is never marked successful by a write callback; the caller must explicitly record
/// confirmation or lack of confirmation.
public final class MelkLightingPeripheralSession: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate, @unchecked Sendable {
    private let queue: DispatchQueue
    private let queueKey = DispatchSpecificKey<Void>()
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var advertisedName: String?
    private var harness: MelkLightingValidationHarness?
    private var sink: CoreBluetoothPeripheralOperationSink?
    private var reconnectEnabled = true
    private var reconnectAttempt = 0
    private var reconnectWorkItem: DispatchWorkItem?

    public private(set) var connectionState: MelkLightingPeripheralState = .idle
    public private(set) var peripheralName: String?
    public private(set) var peripheralIdentifier: String?
    public private(set) var commandEvidence = MelkLightingCommandEvidence()
    public private(set) var lastWritePlan: MelkLightingWritePlan?

    /// Called on the validator's CoreBluetooth queue.
    public var onStateChange: ((MelkLightingPeripheralState) -> Void)?

    /// Called on the validator's CoreBluetooth queue for raw FFF4 notification bytes.
    public var onNotification: ((Data) -> Void)?

    /// Called on the validator's CoreBluetooth queue for bounded diagnostic records.
    public var onRecord: ((String) -> Void)?

    public init(queue: DispatchQueue = DispatchQueue(label: "io.cutout.melk-lighting")) {
        self.queue = queue
        super.init()
        queue.setSpecific(key: queueKey, value: ())
    }

    public func start() {
        onQueue {
            guard central == nil else { return }
            reconnectEnabled = true
            reconnectAttempt = 0
            reconnectWorkItem?.cancel()
            reconnectWorkItem = nil
#if os(iOS)
            central = CBCentralManager(
                delegate: self,
                queue: queue,
                options: [CBCentralManagerOptionRestoreIdentifierKey: "io.cutout.melk-lighting"]
            )
#else
            central = CBCentralManager(delegate: self, queue: queue)
#endif
        }
    }

    public func stop() {
        onQueue {
            reconnectEnabled = false
            reconnectAttempt = 0
            reconnectWorkItem?.cancel()
            reconnectWorkItem = nil
            if commandEvidence.status == .requested {
                commandEvidence.unconfirmed()
            }
            if let peripheral {
                central?.cancelPeripheralConnection(peripheral)
            }
            central?.stopScan()
            central = nil
            peripheral = nil
            advertisedName = nil
            peripheralName = nil
            peripheralIdentifier = nil
            harness = nil
            sink = nil
            transition(to: .disconnected)
        }
    }

    @discardableResult
    public func setPower(_ on: Bool) -> Bool {
        onQueue {
            guard let harness else { return false }
            return submit(harness.setPower(on))
        }
    }

    @discardableResult
    public func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool {
        onQueue {
            guard let harness else { return false }
            return submit(harness.setSolidColor(red: red, green: green, blue: blue))
        }
    }

    /// Returns `InvalidBrightness` without issuing a write when the percentage is out of range.
    @discardableResult
    public func setBrightness(_ percentage: UInt8) throws -> Bool {
        try onQueue {
            guard let harness else { return false }
            return submit(try harness.setBrightness(percentage))
        }
    }

    /// Marks the most recent requested command confirmed by an external protocol/physical check.
    public func markLastCommandConfirmed() {
        onQueue { commandEvidence.confirmed() }
    }

    /// Marks the most recent requested command unconfirmed.
    public func markLastCommandUnconfirmed() {
        onQueue { commandEvidence.unconfirmed() }
    }

    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        onQueue {
            guard central.state == .poweredOn else {
                reconnectWorkItem?.cancel()
                reconnectWorkItem = nil
                reconnectAttempt = 0
                transition(to: .failed("Bluetooth unavailable: \(central.state.rawValue)"))
                return
            }
            if let peripheral, peripheral.state == .connected {
                transition(to: .discovering)
                peripheral.delegate = self
                peripheral.discoverServices(CoreBluetoothScanPolicy.melk.coreBluetoothServiceUuids)
                return
            }
            // MELK-OC21 does not advertise FFF0 in its advertisement packet. Filter only after
            // connecting and discovering the GATT inventory; the advertised name is the
            // candidate gate that keeps this standalone scan narrow.
            central.scanForPeripherals(withServices: nil)
            transition(to: .scanning)
            record("scan=melk services=all; gatt=FFF0 post-connect")
        }
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi: NSNumber
    ) {
        onQueue {
            let name = (advertisementData[CBAdvertisementDataLocalNameKey] as? String) ?? peripheral.name
            guard name?.trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased().hasPrefix("melk") == true else {
                return
            }
            self.peripheral = peripheral
            advertisedName = name
            peripheralName = name
            peripheralIdentifier = peripheral.identifier.uuidString
            peripheral.delegate = self
            central.stopScan()
            transition(to: .connecting)
            record("candidate=\(name ?? "") id=\(peripheral.identifier.uuidString) rssi=\(rssi)")
            central.connect(peripheral)
        }
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        onQueue {
            reconnectAttempt = 0
            reconnectWorkItem?.cancel()
            reconnectWorkItem = nil
            transition(to: .discovering)
            peripheral.discoverServices(CoreBluetoothScanPolicy.melk.coreBluetoothServiceUuids)
        }
    }

    public func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        onQueue {
            if reconnectEnabled {
                scheduleReconnect(
                    central: central,
                    peripheral: peripheral,
                    reason: error.map(String.init(describing:)) ?? "connect failed"
                )
                return
            }
            transition(to: .failed(error.map(String.init(describing:)) ?? "connect failed"))
        }
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        onQueue {
            guard self.peripheral?.identifier == peripheral.identifier else { return }
            if commandEvidence.status == .requested {
                commandEvidence.unconfirmed()
            }
            harness = nil
            sink = nil
            record("disconnected error=\(String(describing: error))")
            if reconnectEnabled, central.state == .poweredOn {
                scheduleReconnect(
                    central: central,
                    peripheral: peripheral,
                    reason: error.map(String.init(describing:)) ?? "link lost"
                )
            } else {
                self.peripheral = nil
                advertisedName = nil
                transition(to: .disconnected)
            }
        }
    }

    public func centralManager(
        _ central: CBCentralManager,
        willRestoreState dict: [String: Any]
    ) {
        onQueue {
            guard let restored = dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral],
                  let restoredPeripheral = restored.first else {
                return
            }
            peripheral = restoredPeripheral
            advertisedName = restoredPeripheral.name
            peripheralName = restoredPeripheral.name
            peripheralIdentifier = restoredPeripheral.identifier.uuidString
            restoredPeripheral.delegate = self
            if restoredPeripheral.state == .connected {
                transition(to: .discovering)
                restoredPeripheral.discoverServices(CoreBluetoothScanPolicy.melk.coreBluetoothServiceUuids)
            } else {
                transition(to: .connecting)
                central.connect(restoredPeripheral)
            }
            record("restore=melk id=\(restoredPeripheral.identifier.uuidString)")
        }
    }

    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        onQueue {
            guard error == nil else {
                transition(to: .failed(error.map(String.init(describing:)) ?? "service discovery failed"))
                return
            }
            peripheral.services?.forEach { peripheral.discoverCharacteristics(nil, for: $0) }
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        onQueue {
            guard error == nil else {
                transition(to: .failed(error.map(String.init(describing:)) ?? "characteristic discovery failed"))
                return
            }
            guard let name = advertisedName ?? peripheral.name else {
                transition(to: .failed("missing MELK name"))
                return
            }
            do {
                let candidate = try MelkLightingValidationHarness(
                    name: name,
                    inventory: CoreBluetoothGattInventory(services: peripheral.services ?? [])
                )
                harness = candidate
                sink = CoreBluetoothPeripheralOperationSink(peripheral: peripheral)
                if case let .subscribe(channel) = candidate.subscription {
                    sink?.subscribe(channel: channel)
                }
                record("gatt=FFF0 write=FFF3 notify=FFF4")
            } catch {
                transition(to: .failed(String(describing: error)))
            }
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateNotificationStateFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        onQueue {
            guard characteristic.uuid == MelkLightingValidationHarness.notify.coreBluetoothUuid else {
                return
            }
            guard error == nil, characteristic.isNotifying else {
                transition(to: .failed(error.map(String.init(describing:)) ?? "FFF4 notify unavailable"))
                return
            }
            transition(to: .ready)
            record("notify_state=true")
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        onQueue {
            guard error == nil, characteristic.uuid == MelkLightingValidationHarness.notify.coreBluetoothUuid,
                  let value = characteristic.value else { return }
            onNotification?(value)
            record("notification=\(value.count) bytes")
        }
    }

    private func submit(_ plan: MelkLightingWritePlan) -> Bool {
        guard connectionState == .ready, let sink,
              case let .writeWithoutResponse(channel, bytes) = plan.operation else {
            return false
        }
        sink.writeWithoutResponse(channel: channel, bytes: bytes)
        lastWritePlan = plan
        commandEvidence.requested()
        record("requested=\(bytes.map { String(format: "%02x", $0) }.joined())")
        return true
    }

    private func scheduleReconnect(
        central: CBCentralManager,
        peripheral: CBPeripheral,
        reason: String
    ) {
        reconnectAttempt += 1
        guard let delay = ConnectionReconnectPolicy.delayMilliseconds(
            attempt: reconnectAttempt,
            jitter: 0.5
        ) else {
            reconnectWorkItem = nil
            transition(to: .failed("Accessory reconnect exhausted after \(reconnectAttempt - 1) attempts"))
            record("reconnect_exhausted reason=\(reason)")
            return
        }

        reconnectWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            guard let self,
                  self.reconnectEnabled,
                  self.central === central,
                  self.peripheral?.identifier == peripheral.identifier,
                  central.state == .poweredOn else {
                return
            }
            self.reconnectWorkItem = nil
            self.transition(to: .connecting)
            central.connect(peripheral)
        }
        reconnectWorkItem = workItem
        transition(to: .retrying(attempt: reconnectAttempt, delayMilliseconds: delay))
        record("reconnect_attempt=\(reconnectAttempt) delay_ms=\(delay) reason=\(reason)")
        queue.asyncAfter(
            deadline: .now() + .milliseconds(Int(delay)),
            execute: workItem
        )
    }

    private func transition(to state: MelkLightingPeripheralState) {
        connectionState = state
        onStateChange?(state)
    }

    private func record(_ message: String) {
        onRecord?(message)
    }

    private func onQueue<T>(_ work: () throws -> T) rethrows -> T {
        if DispatchQueue.getSpecific(key: queueKey) != nil {
            return try work()
        }
        return try queue.sync(execute: work)
    }
}
#endif
