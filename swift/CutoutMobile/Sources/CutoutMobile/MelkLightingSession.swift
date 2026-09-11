import CutoutMobileFFI
import Foundation

#if canImport(CoreBluetooth)
import CoreBluetooth

/// Public presentation state for a standalone MELK lighting connection.
/// The reducer and its state live in Rust; this enum keeps the existing Swift API stable.
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

public enum MelkLightingCommandStatus: Equatable, Sendable {
    case idle
    case requested
    case confirmed
    case unconfirmed
}

public struct MelkLightingPeripheralIdentity: Equatable, Sendable {
    public let name: String?
    public let platformIdentifier: String
    public let rssi: Int?

    public init(name: String?, platformIdentifier: String, rssi: Int?) {
        self.name = name
        self.platformIdentifier = platformIdentifier
        self.rssi = rssi
    }
}

public struct MelkLightingPeripheralCandidate: Equatable, Sendable, Identifiable {
    public let id: String
    public let name: String?
    public let rssi: Int

    public init(name: String?, platformIdentifier: String, rssi: Int) {
        id = platformIdentifier
        self.name = name
        self.rssi = rssi
    }
}

/// The app-facing seam for an independent MELK lighting connection.
public protocol MelkLightingPeripheralSessionProtocol: AnyObject {
    var onIdentity: ((MelkLightingPeripheralIdentity) -> Void)? { get set }
    var onStateChange: ((MelkLightingPeripheralState) -> Void)? { get set }
    var onNotification: ((Data) -> Void)? { get set }
    var onRecord: ((String) -> Void)? { get set }
    var onCandidate: ((MelkLightingPeripheralCandidate) -> Void)? { get set }

    func start(preferredPlatformIdentifier: String?)
    func stop()
    func selectCandidate(platformIdentifier: String)
    @discardableResult func setPower(_ on: Bool) -> Bool
    @discardableResult func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool
    @discardableResult func setBrightness(_ percentage: UInt8) throws -> Bool
    @discardableResult func setEffectSpeed(_ speed: UInt8) -> Bool
    @discardableResult func applyState(_ state: MobileMelkLightingRestoreStateDto) throws -> Bool
    @discardableResult func setSchedule(_ schedule: MobileMelkScheduleDto, clock: MobileMelkClockDto) throws -> Bool
    func markLastCommandConfirmed()
    func markLastCommandUnconfirmed()
}

/// CoreBluetooth-only adapter for the Rust-owned MELK session reducer.
public final class MelkLightingPeripheralSession: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate,
    @unchecked Sendable
{
    private let queue: DispatchQueue
    private let queueKey = DispatchSpecificKey<Void>()
    private let core = MobileMelkLightingSessionCore()
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var discoveredPeripherals: [String: CBPeripheral] = [:]
    private var timerTask: DispatchWorkItem?
    private var preferredPlatformIdentifier: String?
    private static let melkServiceUuid = MobileBluetoothUuid(
        mostSignificantBits: 0x0000_fff0_0000_1000,
        leastSignificantBits: 0x8000_0080_5f9b_34fb
    )

    public private(set) var connectionState: MelkLightingPeripheralState = .idle
    public private(set) var peripheralName: String?
    public private(set) var peripheralIdentifier: String?

    public var onStateChange: ((MelkLightingPeripheralState) -> Void)?
    public var onNotification: ((Data) -> Void)?
    public var onIdentity: ((MelkLightingPeripheralIdentity) -> Void)?
    public var onAdvertisement: ((String?, String, Int) -> Void)?
    public var onRecord: ((String) -> Void)?
    public var onCandidate: ((MelkLightingPeripheralCandidate) -> Void)?

    public init(queue: DispatchQueue = DispatchQueue(label: "io.cutout.melk-lighting")) {
        self.queue = queue
        super.init()
        queue.setSpecific(key: queueKey, value: ())
    }

    public func start(preferredPlatformIdentifier: String? = nil) {
        onQueue {
            guard central == nil else { return }
            self.preferredPlatformIdentifier = preferredPlatformIdentifier
            discoveredPeripherals.removeAll(keepingCapacity: true)
            core.start(preferredPlatformIdentifier: preferredPlatformIdentifier)
#if os(iOS)
            central = CBCentralManager(
                delegate: self,
                queue: queue,
                options: [CBCentralManagerOptionRestoreIdentifierKey: "io.cutout.melk-lighting"]
            )
#else
            central = CBCentralManager(delegate: self, queue: queue)
#endif
            syncCore()
        }
    }

    public func stop() {
        onQueue {
            timerTask?.cancel()
            timerTask = nil
            if let peripheral { central?.cancelPeripheralConnection(peripheral) }
            central?.stopScan()
            core.stop()
            syncCore()
            discoveredPeripherals.removeAll(keepingCapacity: false)
            central = nil
            peripheral = nil
            peripheralName = nil
            peripheralIdentifier = nil
        }
    }

    public func selectCandidate(platformIdentifier: String) {
        onQueue {
            guard discoveredPeripherals[platformIdentifier] != nil else { return }
            core.selectCandidate(platformIdentifier: platformIdentifier)
            syncCore()
        }
    }

    @discardableResult
    public func setPower(_ on: Bool) -> Bool {
        onQueue {
            let result = core.setPower(on: on)
            core.flushWrites(canSend: peripheral?.canSendWriteWithoutResponse == true)
            syncCore()
            return result
        }
    }

    @discardableResult
    public func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool {
        onQueue {
            let result = core.setSolidColor(red: red, green: green, blue: blue)
            core.flushWrites(canSend: peripheral?.canSendWriteWithoutResponse == true)
            syncCore()
            return result
        }
    }

    @discardableResult
    public func setBrightness(_ percentage: UInt8) throws -> Bool {
        try onQueue {
            let result = try core.setBrightness(percentage: percentage)
            core.flushWrites(canSend: peripheral?.canSendWriteWithoutResponse == true)
            syncCore()
            return result
        }
    }

    @discardableResult
    public func setEffectSpeed(_ speed: UInt8) -> Bool {
        onQueue {
            let result = core.setEffectSpeed(speed: speed)
            core.flushWrites(canSend: peripheral?.canSendWriteWithoutResponse == true)
            syncCore()
            return result
        }
    }

    @discardableResult
    public func applyState(_ state: MobileMelkLightingRestoreStateDto) throws -> Bool {
        try onQueue {
            let result = try core.applyState(state: state)
            core.flushWrites(canSend: peripheral?.canSendWriteWithoutResponse == true)
            syncCore()
            return result
        }
    }

    @discardableResult
    public func setSchedule(_ schedule: MobileMelkScheduleDto, clock: MobileMelkClockDto) throws -> Bool {
        try onQueue {
            let result = try core.setSchedule(schedule: schedule, clock: clock)
            core.flushWrites(canSend: peripheral?.canSendWriteWithoutResponse == true)
            syncCore()
            return result
        }
    }

    public func markLastCommandConfirmed() {
        onQueue { core.markLastCommandConfirmed(); syncCore() }
    }

    public func markLastCommandUnconfirmed() {
        onQueue { core.markLastCommandUnconfirmed(); syncCore() }
    }

    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        onQueue {
            guard central === self.central else { return }
            core.handle(event: .bluetoothState(
                poweredOn: central.state == .poweredOn,
                stateCode: Int32(central.state.rawValue)
            ))
            if central.state == .poweredOn, let preferredPlatformIdentifier {
                if let uuid = UUID(uuidString: preferredPlatformIdentifier),
                   let restored = central.retrievePeripherals(withIdentifiers: [uuid]).first
                {
                    peripheral = restored
                    peripheralName = restored.name
                    peripheralIdentifier = restored.identifier.uuidString
                    restored.delegate = self
                    core.handle(event: .restored(
                        name: restored.name,
                        platformIdentifier: restored.identifier.uuidString,
                        connected: restored.state == .connected
                    ))
                } else {
                    core.handle(event: .restoreUnavailable)
                }
            }
            syncCore()
        }
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi: NSNumber
    ) {
        onQueue {
            guard central === self.central, self.peripheral == nil else { return }
            let name = (advertisementData[CBAdvertisementDataLocalNameKey] as? String) ?? peripheral.name
            let identifier = peripheral.identifier.uuidString
            onAdvertisement?(name, identifier, rssi.intValue)
            guard preferredPlatformIdentifier != nil
                || name?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false else { return }
            if discoveredPeripherals.count >= 32, discoveredPeripherals[identifier] == nil {
                if let first = discoveredPeripherals.keys.first {
                    discoveredPeripherals.removeValue(forKey: first)
                }
            }
            discoveredPeripherals[identifier] = peripheral
            core.handle(event: .discovered(name: name, platformIdentifier: identifier, rssi: Int32(rssi.intValue)))
            syncCore()
        }
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        onQueue {
            guard central === self.central, peripheral.identifier.uuidString == peripheralIdentifier else { return }
            peripheral.delegate = self
            core.handle(event: .connected(name: peripheral.name, platformIdentifier: peripheral.identifier.uuidString))
            syncCore()
        }
    }

    public func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        onQueue {
            guard central === self.central, peripheral.identifier.uuidString == peripheralIdentifier else { return }
            core.handle(event: .connectFailed(reason: error.map(String.init(describing:)) ?? "connect failed"))
            syncCore()
        }
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        onQueue {
            guard central === self.central, peripheral.identifier.uuidString == peripheralIdentifier else { return }
            core.handle(event: .disconnected(
                reason: error.map(String.init(describing:)) ?? "link lost",
                poweredOn: central.state == .poweredOn
            ))
            syncCore()
        }
    }

    public func centralManager(_ central: CBCentralManager, willRestoreState dict: [String: Any]) {
        onQueue {
            guard central === self.central,
                  let restored = dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral]
            else { return }
            let preferred = preferredPlatformIdentifier.flatMap(UUID.init(uuidString:))
            let restoredPeripheral = preferred.flatMap { identifier in
                restored.first(where: { $0.identifier == identifier })
            } ?? (preferredPlatformIdentifier == nil ? restored.first : nil)
            guard let restoredPeripheral else {
                peripheral = nil
                peripheralName = nil
                peripheralIdentifier = nil
                core.handle(event: .restoreUnavailable)
                syncCore()
                return
            }
            core.handle(event: .restored(
                name: restoredPeripheral.name,
                platformIdentifier: restoredPeripheral.identifier.uuidString,
                connected: restoredPeripheral.state == .connected
            ))
            guard core.snapshot().platformIdentifier == restoredPeripheral.identifier.uuidString else {
                peripheral = nil
                peripheralName = nil
                peripheralIdentifier = nil
                syncCore()
                return
            }
            peripheral = restoredPeripheral
            peripheralName = restoredPeripheral.name
            peripheralIdentifier = restoredPeripheral.identifier.uuidString
            restoredPeripheral.delegate = self
            emitIdentity(rssi: nil)
            syncCore()
        }
    }

    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        onQueue {
            guard peripheral === self.peripheral else { return }
            let services = peripheral.services?.compactMap { service in
                MobileBluetoothUuid(coreBluetoothUuid: service.uuid)
            } ?? []
            core.handle(event: .servicesDiscovered(serviceUuids: services, error: error.map(String.init(describing:))))
            syncCore()
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        onQueue {
            guard peripheral === self.peripheral else { return }
            let characteristics = service.characteristics?.compactMap { characteristic in
                MobileBluetoothUuid(coreBluetoothUuid: characteristic.uuid).map {
                    MobileMelkLightingCharacteristicEvidenceDto(
                        uuid: $0,
                        writeWithoutResponse: characteristic.properties.contains(.writeWithoutResponse),
                        notifyOrIndicate: characteristic.properties.contains(.notify) || characteristic.properties.contains(.indicate)
                    )
                }
            } ?? []
            guard let serviceUuid = MobileBluetoothUuid(coreBluetoothUuid: service.uuid) else { return }
            core.handle(event: .characteristicsDiscovered(
                name: peripheralName ?? peripheral.name,
                serviceUuid: serviceUuid,
                characteristics: characteristics,
                error: error.map(String.init(describing:))
            ))
            syncCore()
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateNotificationStateFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        onQueue {
            guard peripheral === self.peripheral else { return }
            guard let uuid = MobileBluetoothUuid(coreBluetoothUuid: characteristic.uuid) else { return }
            core.handle(event: .notificationState(
                characteristic: uuid,
                ready: characteristic.isNotifying,
                canSend: peripheral.canSendWriteWithoutResponse,
                error: error.map(String.init(describing:))
            ))
            syncCore()
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        onQueue {
            guard peripheral === self.peripheral,
                  error == nil,
                  let uuid = MobileBluetoothUuid(coreBluetoothUuid: characteristic.uuid),
                  let value = characteristic.value else { return }
            core.handle(event: .notification(
                characteristic: uuid,
                bytes: value
            ))
            syncCore()
        }
    }

    public func peripheralIsReady(toSendWriteWithoutResponse peripheral: CBPeripheral) {
        onQueue {
            guard peripheral === self.peripheral else { return }
            core.handle(event: .writeReady(canSend: peripheral.canSendWriteWithoutResponse))
            syncCore()
        }
    }

    private func syncCore() {
        let snapshot = core.snapshot()
        let nextState = Self.state(snapshot.state)
        if nextState != connectionState {
            connectionState = nextState
            onStateChange?(nextState)
        }
        peripheralIdentifier = snapshot.platformIdentifier
        peripheralName = snapshot.name ?? peripheralName
        for candidate in core.drainCandidates() {
            onCandidate?(MelkLightingPeripheralCandidate(
                name: candidate.name,
                platformIdentifier: candidate.platformIdentifier,
                rssi: Int(candidate.rssi)
            ))
        }
        for record in core.drainRecords() { onRecord?(record) }
        for bytes in core.drainNotifications() { onNotification?(Data(bytes)) }
        execute(core.drainActions())
    }

    private func execute(_ actions: [MobileMelkLightingSessionActionDto]) {
        guard let central else { return }
        for action in actions {
            switch action {
            case .scan:
                central.scanForPeripherals(withServices: nil)
            case .stopScan:
                central.stopScan()
            case let .connect(identifier):
                guard let candidate = discoveredPeripherals[identifier]
                    ?? central.retrievePeripherals(withIdentifiers: [UUID(uuidString: identifier) ?? UUID()]).first else { continue }
                peripheral = candidate
                peripheralIdentifier = identifier
                peripheralName = candidate.name
                candidate.delegate = self
                central.connect(candidate)
                emitIdentity(rssi: nil)
            case let .cancelConnect(identifier):
                if let activePeripheral = peripheral,
                   activePeripheral.identifier.uuidString == identifier {
                    central.cancelPeripheralConnection(activePeripheral)
                    peripheral = nil
                    peripheralName = nil
                    peripheralIdentifier = nil
                } else if let candidate = discoveredPeripherals[identifier] {
                    central.cancelPeripheralConnection(candidate)
                }
            case let .discoverServices(identifier, service):
                guard peripheral?.identifier.uuidString == identifier else { continue }
                peripheral?.discoverServices([service.coreBluetoothUuid])
            case let .discoverCharacteristics(identifier, service):
                guard peripheral?.identifier.uuidString == identifier,
                      let gattService = peripheral?.services?.first(where: { $0.uuid == service.coreBluetoothUuid }) else { continue }
                peripheral?.discoverCharacteristics(nil, for: gattService)
            case let .subscribe(identifier, characteristic):
                guard let target = melkCharacteristic(identifier, uuid: characteristic) else { continue }
                peripheral?.setNotifyValue(true, for: target)
            case let .write(identifier, write):
                guard let targetCharacteristic = melkCharacteristic(identifier, uuid: write.characteristic),
                      targetCharacteristic.properties.contains(.writeWithoutResponse) else { continue }
                peripheral?.writeValue(Data(write.payload), for: targetCharacteristic, type: .withoutResponse)
            case let .armTimer(timer, delayMilliseconds):
                arm(timer, delayMilliseconds: delayMilliseconds)
            }
        }
    }

    private func melkCharacteristic(
        _ identifier: String,
        uuid: MobileBluetoothUuid
    ) -> CBCharacteristic? {
        guard peripheral?.identifier.uuidString == identifier,
              let service = peripheral?.services?.first(where: {
                  $0.uuid == Self.melkServiceUuid.coreBluetoothUuid
              }) else { return nil }
        return service.characteristics?.first(where: { $0.uuid == uuid.coreBluetoothUuid })
    }

    private func arm(_ timer: MobileMelkLightingTimerDto, delayMilliseconds: UInt64) {
        timerTask?.cancel()
        let task = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.onQueue {
                self.core.handle(event: .timerFired(
                    timer: timer,
                    canSend: self.peripheral?.canSendWriteWithoutResponse == true
                ))
                self.syncCore()
            }
        }
        timerTask = task
        queue.asyncAfter(deadline: .now() + .milliseconds(Int(delayMilliseconds)), execute: task)
    }

    private func emitIdentity(rssi: Int? = nil) {
        guard let peripheralIdentifier else { return }
        onIdentity?(MelkLightingPeripheralIdentity(name: peripheralName, platformIdentifier: peripheralIdentifier, rssi: rssi))
    }

    private static func state(_ dto: MobileMelkLightingSessionStateDto) -> MelkLightingPeripheralState {
        switch dto {
        case .idle: .idle
        case .scanning: .scanning
        case .connecting: .connecting
        case let .retrying(attempt, delayMilliseconds): .retrying(attempt: Int(attempt), delayMilliseconds: delayMilliseconds)
        case .discovering: .discovering
        case .ready: .ready
        case .disconnected: .disconnected
        case let .failed(reason): .failed(reason)
        }
    }

    private func onQueue<T>(_ work: () throws -> T) rethrows -> T {
        if DispatchQueue.getSpecific(key: queueKey) != nil { return try work() }
        return try queue.sync(execute: work)
    }
}

extension MelkLightingPeripheralSession: MelkLightingPeripheralSessionProtocol {}

extension MobileBluetoothUuid {
    init?(coreBluetoothUuid: CBUUID) {
        let bytes: [UInt8]
        switch coreBluetoothUuid.data.count {
        case 2:
            let short = Array(coreBluetoothUuid.data)
            bytes = [
                0, 0, short[0], short[1], 0, 0, 0x10, 0,
                0x80, 0, 0, 0x80, 0x5f, 0x9b, 0x34, 0xfb,
            ]
        case 16:
            bytes = Array(coreBluetoothUuid.data)
        default:
            return nil
        }
        self.init(
            mostSignificantBits: Self.word(bytes[0 ..< 8]),
            leastSignificantBits: Self.word(bytes[8 ..< 16])
        )
    }

    var coreBluetoothUuid: CBUUID {
        CBUUID(data: Data(Self.bytes(mostSignificantBits) + Self.bytes(leastSignificantBits)))
    }

    private static func word(_ bytes: ArraySlice<UInt8>) -> UInt64 {
        bytes.reduce(0) { ($0 << 8) | UInt64($1) }
    }

    private static func bytes(_ word: UInt64) -> [UInt8] {
        (0 ..< 8).map { UInt8(truncatingIfNeeded: word >> (56 - ($0 * 8))) }
    }
}

#endif
