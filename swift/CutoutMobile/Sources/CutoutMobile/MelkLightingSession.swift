import CutoutMobileFFI
import Foundation

/// Limits reconnects to a remembered CoreBluetooth identity after first pairing.
///
/// A missing identity is the only state that permits first-pairing discovery. Persisted values
/// are parsed as UUIDs before they can influence a connection; malformed values fail closed.
struct MelkLightingTargetPolicy: Equatable, Sendable {
    let preferredUUID: UUID?
    let isInvalid: Bool

    init(preferredPlatformIdentifier: String?) {
        guard let preferredPlatformIdentifier else {
            preferredUUID = nil
            isInvalid = false
            return
        }
        preferredUUID = UUID(uuidString: preferredPlatformIdentifier)
        isInvalid = preferredUUID == nil
    }

    func accepts(_ identifier: CoreBluetoothPeripheralIdentifier) -> Bool {
        guard !isInvalid else { return false }
        guard let preferredUUID else { return true }
        return UUID(uuidString: identifier.rawValue) == preferredUUID
    }

    func acceptsDiscovery(
        name: String?,
        identifier: CoreBluetoothPeripheralIdentifier
    ) -> Bool {
        guard accepts(identifier) else { return false }
        guard preferredUUID != nil else { return Self.isMelkName(name) }
        return Self.isMelkName(name)
            || UUID(uuidString: identifier.rawValue) == preferredUUID
    }

    private static func isMelkName(_ name: String?) -> Bool {
        name?.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().hasPrefix("melk") == true
    }
}

/// Failure while matching an observed standalone MELK controller to its typed profile.
enum MelkLightingProtocolError: Error, Equatable, Sendable {
    case missingService
    case missingWriteCharacteristic
    case missingNotificationCharacteristic
    case profileRejected
    case invalidWrite
}

/// Explicit result state for a lighting command.
public enum MelkLightingCommandStatus: Equatable, Sendable {
    case idle
    case requested
    case confirmed
    case unconfirmed
}

/// Small state tracker used by the live lighting session; a write never self-confirms.
struct MelkLightingCommandEvidence: Equatable, Sendable {
    private(set) var status: MelkLightingCommandStatus = .idle

    init() {}

    mutating func requested() {
        status = .requested
    }

    mutating func confirmed() {
        guard status == .requested else { return }
        status = .confirmed
    }

    mutating func unconfirmed() {
        guard status == .requested else { return }
        status = .unconfirmed
    }
}

/// One Rust-owned MELK write ready for the existing CoreBluetooth operation sink.
struct MelkLightingWritePlan: Equatable, Sendable {
    let operation: CoreBluetoothPlannedOperation
    let confirmationChannel: BluetoothUuid
    let minimumIntervalMilliseconds: UInt16?

    init(
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
struct MelkLightingCommandProfile: Sendable {
    static let service = BluetoothUuid.bluetooth16(0xfff0)
    static let write = BluetoothUuid.bluetooth16(0xfff3)
    static let notify = BluetoothUuid.bluetooth16(0xfff4)

    private let profile: MobileMelkLightingProfile

    let subscription: CoreBluetoothPlannedOperation

    init(
        name: String,
        inventory: CoreBluetoothGattInventory
    ) throws {
        guard let serviceInventory = inventory.services.first(where: { $0.uuid == Self.service }) else {
            throw MelkLightingProtocolError.missingService
        }
        guard let writeCharacteristic = serviceInventory.characteristics.first(where: {
            $0.uuid == Self.write && $0.properties.contains(.writeWithoutResponse)
        }) else {
            throw MelkLightingProtocolError.missingWriteCharacteristic
        }
        guard let notifyCharacteristic = serviceInventory.characteristics.first(where: {
            $0.uuid == Self.notify && ($0.properties.contains(.notify) || $0.properties.contains(.indicate))
        }) else {
            throw MelkLightingProtocolError.missingNotificationCharacteristic
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
            throw MelkLightingProtocolError.profileRejected
        }
        subscription = .subscribe(channel: notifyCharacteristic.uuid)
    }
    func initialization() throws -> [MelkLightingWritePlan] {
        try profile.initialization().map { try plan($0) }
    }


    func setPower(_ on: Bool) throws -> MelkLightingWritePlan {
        try plan(profile.setPower(on: on))
    }

    func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) throws -> MelkLightingWritePlan {
        try plan(profile.setSolidColor(red: red, green: green, blue: blue))
    }

    func setBrightness(_ percentage: UInt8) throws -> MelkLightingWritePlan {
        try plan(profile.setBrightness(percentage: percentage))
    }

    func setEffectSpeed(_ speed: UInt8) throws -> MelkLightingWritePlan {
        try plan(profile.setEffectSpeed(speed: speed))
    }

    func applyState(_ state: MobileMelkLightingRestoreStateDto) throws -> [MelkLightingWritePlan] {
        try profile.applyState(state: state).map { try plan($0) }
    }

    func setSchedule(_ schedule: MobileMelkScheduleDto, clock: MobileMelkClockDto) throws -> [MelkLightingWritePlan] {
        try profile.setSchedule(schedule: schedule, clock: clock).map { try plan($0) }
    }

    private func plan(_ write: MobileMelkLightingWriteDto) throws -> MelkLightingWritePlan {
        guard write.mode == .withoutResponse else {
            throw MelkLightingProtocolError.invalidWrite
        }
        guard let channel = BluetoothUuid(write.characteristic),
              let confirmationChannel = BluetoothUuid(write.confirmationCharacteristic)
        else {
            throw MelkLightingProtocolError.invalidWrite
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

/// Connection state for the independent standalone MELK lighting session.
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

public extension MelkLightingPeripheralState {
    /// Returns whether a state transition invalidates a one-shot restore attempt.
    var resetsRestoreEligibility: Bool {
        switch self {
        case .scanning, .connecting, .retrying, .disconnected, .failed:
            true
        case .idle, .discovering, .ready:
            false
        }
    }
}

/// A typed identity observation emitted when CoreBluetooth has selected a MELK peripheral.
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

/// A nearby MELK advertisement awaiting explicit selection during first pairing.
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
///
/// Keeping the CoreBluetooth implementation behind this protocol lets the route model test
/// lifecycle and restore behavior without sharing the ride session or requiring hardware.
public protocol MelkLightingPeripheralSessionProtocol: AnyObject {
    var onIdentity: ((MelkLightingPeripheralIdentity) -> Void)? { get set }
    var onStateChange: ((MelkLightingPeripheralState) -> Void)? { get set }
    var onNotification: ((Data) -> Void)? { get set }
    var onRecord: ((String) -> Void)? { get set }
    var onCandidate: ((MelkLightingPeripheralCandidate) -> Void)? { get set }

    func start(preferredPlatformIdentifier: String?)
    func stop()
    func selectCandidate(platformIdentifier: String)
    @discardableResult
    func setPower(_ on: Bool) -> Bool
    @discardableResult
    func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool
    @discardableResult
    func setBrightness(_ percentage: UInt8) throws -> Bool
    @discardableResult
    func setEffectSpeed(_ speed: UInt8) -> Bool
    @discardableResult
    func applyState(_ state: MobileMelkLightingRestoreStateDto) throws -> Bool
    @discardableResult
    func setSchedule(_ schedule: MobileMelkScheduleDto, clock: MobileMelkClockDto) throws -> Bool
    func markLastCommandConfirmed()
    func markLastCommandUnconfirmed()
}

/// A secondary CoreBluetooth connection for validating MELK without replacing a ride session.
///
/// The lighting session owns its own central manager, so it can remain connected while the primary
/// EUC/VESC central connection continues to receive telemetry. A command starts as `requested`
/// and is never marked successful by a write callback; the caller must explicitly record
/// confirmation or lack of confirmation.
public final class MelkLightingPeripheralSession: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate, @unchecked Sendable {
    private let queue: DispatchQueue
    private let queueKey = DispatchSpecificKey<Void>()
    private let reconnectController: ConnectionReconnectController
    private var central: CBCentralManager?
    private var peripheral: CBPeripheral?
    private var advertisedName: String?
    private var harness: MelkLightingCommandProfile?
    private var sink: CoreBluetoothPeripheralOperationSink?
    private var targetPolicy = MelkLightingTargetPolicy(preferredPlatformIdentifier: nil)
    private var discoveredPeripherals: [UUID: CBPeripheral] = [:]
    private var reconnectEnabled = true
    private var pendingWrites: [MelkLightingWritePlan] = []
    private var writeDrainTask: DispatchWorkItem?
    private var pendingInitialization: [MelkLightingWritePlan] = []
    private var initializationTask: DispatchWorkItem?
    private var notificationReady = false
    private var connectionAttemptTask: DispatchWorkItem?

    public private(set) var connectionState: MelkLightingPeripheralState = .idle
    public private(set) var peripheralName: String?
    public private(set) var peripheralIdentifier: String?
    private var commandEvidence = MelkLightingCommandEvidence()

    /// Called on the lighting session's CoreBluetooth queue.
    public var onStateChange: ((MelkLightingPeripheralState) -> Void)?

    /// Called on the lighting session's CoreBluetooth queue for raw FFF4 notification bytes.
    public var onNotification: ((Data) -> Void)?

    /// Called on the lighting session's CoreBluetooth queue for the selected peripheral identity.
    public var onIdentity: ((MelkLightingPeripheralIdentity) -> Void)?

    /// Called on the lighting session's CoreBluetooth queue for every advertisement observed while scanning.
    /// This is intended for bounded validator diagnostics; production callers should leave it unset.
    public var onAdvertisement: ((String?, String, Int) -> Void)?

    /// Called on the lighting session's CoreBluetooth queue for bounded diagnostic records.
    public var onRecord: ((String) -> Void)?

    /// Called on the CoreBluetooth queue for each first-pairing candidate.
    public var onCandidate: ((MelkLightingPeripheralCandidate) -> Void)?

    public init(queue: DispatchQueue = DispatchQueue(label: "io.cutout.melk-lighting")) {
        self.queue = queue
        self.reconnectController = ConnectionReconnectController(
            scheduler: DispatchQueueReconnectScheduler(queue: queue)
        )
        super.init()
        queue.setSpecific(key: queueKey, value: ())
    }

    public func start(preferredPlatformIdentifier: String? = nil) {
        onQueue {
            guard central == nil else { return }
            targetPolicy = MelkLightingTargetPolicy(
                preferredPlatformIdentifier: preferredPlatformIdentifier
            )
            discoveredPeripherals.removeAll(keepingCapacity: true)
            reconnectEnabled = true
            reconnectController.cancel()
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
            reconnectController.cancel()
            cancelConnectionAttempt()
            resetInitialization()
            if commandEvidence.status == .requested {
                commandEvidence.unconfirmed()
            }
            if let peripheral {
                central?.cancelPeripheralConnection(peripheral)
            }
            central?.stopScan()
            discoveredPeripherals.removeAll(keepingCapacity: false)
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

    public func selectCandidate(platformIdentifier: String) {
        onQueue {
            guard targetPolicy.preferredUUID == nil,
                  !targetPolicy.isInvalid,
                  peripheral == nil,
                  let identifier = UUID(uuidString: platformIdentifier),
                  let candidate = discoveredPeripherals[identifier],
                  let central else { return }
            central.stopScan()
            record("selected=melk id=\(platformIdentifier)")
            connect(
                central: central,
                peripheral: candidate,
                advertisedName: candidate.name
            )
        }
    }

    @discardableResult
    public func setPower(_ on: Bool) -> Bool {
        onQueue {
            guard let harness else { return false }
            guard let plan = try? harness.setPower(on) else { return false }
            return submit(plan)
        }
    }

    @discardableResult
    public func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool {
        onQueue {
            guard let harness else { return false }
            guard let plan = try? harness.setSolidColor(red: red, green: green, blue: blue) else { return false }
            return submit(plan)
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

    @discardableResult
    public func setEffectSpeed(_ speed: UInt8) -> Bool {
        onQueue {
            guard let harness else { return false }
            guard let plan = try? harness.setEffectSpeed(speed) else { return false }
            return submit(plan)
        }
    }

    @discardableResult
    public func applyState(_ state: MobileMelkLightingRestoreStateDto) throws -> Bool {
        try onQueue {
            guard let harness else { return false }
            return submit(try harness.applyState(state))
        }
    }

    @discardableResult
    public func setSchedule(_ schedule: MobileMelkScheduleDto, clock: MobileMelkClockDto) throws -> Bool {
        try onQueue {
            guard let harness else { return false }
            return submit(try harness.setSchedule(schedule, clock: clock))
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
            guard central === self.central else { return }
            guard !targetPolicy.isInvalid else {
                transition(to: .failed("Remembered lighting identity is invalid"))
                record("scan=refused invalid remembered identity")
                return
            }
            guard central.state == .poweredOn else {
                reconnectController.cancel()
                harness = nil
                sink = nil
                transition(to: .failed("Bluetooth unavailable: \(central.state.rawValue)"))
                return
            }
            if let peripheral, peripheral.state == .connected {
                transition(to: .discovering)
                peripheral.delegate = self
                peripheral.discoverServices(CoreBluetoothScanPolicy.melk.coreBluetoothServiceUuids)
                return
            }
            if let peripheral {
                connect(central: central, peripheral: peripheral)
                return
            }
            if let preferredUUID = targetPolicy.preferredUUID,
               let restoredPeripheral = central.retrievePeripherals(withIdentifiers: [preferredUUID]).first {
                connect(central: central, peripheral: restoredPeripheral)
                record("target=melk id=\(preferredUUID.uuidString)")
                return
            }
            if targetPolicy.preferredUUID != nil,
               let connectedPeripheral = central.retrieveConnectedPeripherals(
                withServices: [MelkLightingCommandProfile.service.coreBluetoothUuid]
            ).first(where: {
                targetPolicy.acceptsDiscovery(
                    name: $0.name,
                    identifier: CoreBluetoothPeripheralIdentifier($0.identifier.uuidString)
                )
            }) {
                connect(
                    central: central,
                    peripheral: connectedPeripheral,
                    advertisedName: connectedPeripheral.name
                )
                record("connected=melk id=\(connectedPeripheral.identifier.uuidString)")
                return
            }
            // MELK-OC21 does not advertise FFF0 in its advertisement packet. Filter only after
            // connecting and discovering the GATT inventory; the advertised name is the
            // candidate gate that keeps this standalone scan narrow. When a remembered identity
            // exists, didDiscoverPeripheral applies the identity filter before this gate.
            central.scanForPeripherals(withServices: nil)
            transition(to: .scanning)
            let target = targetPolicy.preferredUUID.map { " id=\($0.uuidString)" } ?? ""
            record("scan=melk services=all; gatt=FFF0 post-connect\(target)")
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
            onAdvertisement?(name, peripheral.identifier.uuidString, rssi.intValue)
            guard targetPolicy.accepts(
                CoreBluetoothPeripheralIdentifier(peripheral.identifier.uuidString)
            ) else {
                return
            }
            let identifier = CoreBluetoothPeripheralIdentifier(peripheral.identifier.uuidString)
            guard targetPolicy.acceptsDiscovery(name: name, identifier: identifier) else {
                return
            }
            if discoveredPeripherals[peripheral.identifier] == nil,
               discoveredPeripherals.count >= 32 {
                return
            }
            discoveredPeripherals[peripheral.identifier] = peripheral
            record("candidate=\(name ?? "") id=\(peripheral.identifier.uuidString) rssi=\(rssi)")
            if targetPolicy.preferredUUID != nil {
                central.stopScan()
                connect(central: central, peripheral: peripheral, advertisedName: name, rssi: rssi.intValue)
            } else {
                onCandidate?(MelkLightingPeripheralCandidate(
                    name: name,
                    platformIdentifier: peripheral.identifier.uuidString,
                    rssi: rssi.intValue
                ))
            }
        }
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        onQueue {
            guard central === self.central, peripheral === self.peripheral else { return }
            cancelConnectionAttempt()
            reconnectController.cancel()
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
            guard central === self.central, peripheral === self.peripheral else { return }
            cancelConnectionAttempt()
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
            guard central === self.central, peripheral === self.peripheral else { return }
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
            guard central === self.central,
                  let restored = dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral],
                  let restoredPeripheral = restored.first(where: { $0.identifier == targetPolicy.preferredUUID }) else {
                return
            }
            guard targetPolicy.preferredUUID != nil,
                  targetPolicy.accepts(
                      CoreBluetoothPeripheralIdentifier(restoredPeripheral.identifier.uuidString)
                  ) else {
                record("restore=melk ignored different identity")
                return
            }
            peripheral = restoredPeripheral
            advertisedName = restoredPeripheral.name
            peripheralName = restoredPeripheral.name
            peripheralIdentifier = restoredPeripheral.identifier.uuidString
            onIdentity?(MelkLightingPeripheralIdentity(
                name: restoredPeripheral.name,
                platformIdentifier: restoredPeripheral.identifier.uuidString,
                rssi: nil
            ))
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

    private func connect(
        central: CBCentralManager,
        peripheral: CBPeripheral,
        advertisedName: String? = nil,
        rssi: Int? = nil
    ) {
        self.peripheral = peripheral
        self.advertisedName = advertisedName ?? peripheral.name
        peripheralName = advertisedName ?? peripheral.name
        peripheralIdentifier = peripheral.identifier.uuidString
        onIdentity?(MelkLightingPeripheralIdentity(
            name: advertisedName ?? peripheral.name,
            platformIdentifier: peripheral.identifier.uuidString,
            rssi: rssi
        ))
        peripheral.delegate = self
        if peripheral.state == .connected {
            transition(to: .discovering)
            peripheral.discoverServices(CoreBluetoothScanPolicy.melk.coreBluetoothServiceUuids)
        } else {
            transition(to: .connecting)
            central.connect(peripheral)
            scheduleConnectionAttemptTimeout(central: central, peripheral: peripheral)
        }
    }

    private func scheduleConnectionAttemptTimeout(central: CBCentralManager, peripheral: CBPeripheral) {
        cancelConnectionAttempt()
        let task = DispatchWorkItem { [weak self, weak peripheral] in
            guard let self, let peripheral else { return }
            self.onQueue {
                guard self.central === central,
                      self.peripheral === peripheral,
                      self.connectionState == .connecting,
                      self.reconnectEnabled else {
                    return
                }

                self.connectionAttemptTask = nil
                self.peripheral = nil
                self.advertisedName = nil
                self.peripheralName = nil
                self.peripheralIdentifier = nil
                self.harness = nil
                self.sink = nil
                central.cancelPeripheralConnection(peripheral)
                self.transition(to: .scanning)
                self.record("connect_timeout id=\(peripheral.identifier.uuidString)")
                central.scanForPeripherals(withServices: nil)
            }
        }
        connectionAttemptTask = task
        queue.asyncAfter(deadline: .now() + 15, execute: task)
    }

    private func cancelConnectionAttempt() {
        connectionAttemptTask?.cancel()
        connectionAttemptTask = nil
    }

    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        onQueue {
            guard peripheral === self.peripheral else { return }
            guard error == nil else {
                transition(to: .failed(error.map(String.init(describing:)) ?? "service discovery failed"))
                return
            }
            guard let service = peripheral.services?.first(where: {
                $0.uuid == MelkLightingCommandProfile.service.coreBluetoothUuid
            }) else {
                transition(to: .failed("missing FFF0 service"))
                record("gatt=missing FFF0 service")
                return
            }
            peripheral.discoverCharacteristics(nil, for: service)
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        onQueue {
            guard peripheral === self.peripheral,
                  service.uuid == MelkLightingCommandProfile.service.coreBluetoothUuid else { return }
            guard error == nil else {
                transition(to: .failed(error.map(String.init(describing:)) ?? "characteristic discovery failed"))
                return
            }
            let name = advertisedName
                ?? peripheral.name
                ?? (targetPolicy.preferredUUID == peripheral.identifier ? "MELK-OC21" : nil)
            guard let name else {
                transition(to: .failed("missing MELK name"))
                return
            }
            do {
                let candidate = try MelkLightingCommandProfile(
                    name: name,
                    inventory: CoreBluetoothGattInventory(services: peripheral.services ?? [])
                )
                harness = candidate
                sink = CoreBluetoothPeripheralOperationSink(peripheral: peripheral)
                pendingInitialization = try candidate.initialization()
                notificationReady = false
                drainInitialization()
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
            guard peripheral === self.peripheral,
                  characteristic.uuid == MelkLightingCommandProfile.notify.coreBluetoothUuid else {
                return
            }
            guard error == nil, characteristic.isNotifying else {
                transition(to: .failed(error.map(String.init(describing:)) ?? "FFF4 notify unavailable"))
                return
            }
            notificationReady = true
            finishReadyIfPossible()
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        onQueue {
            guard peripheral === self.peripheral, error == nil,
                  characteristic.uuid == MelkLightingCommandProfile.notify.coreBluetoothUuid,
                  let value = characteristic.value else { return }
            onNotification?(value)
            record("notification=\(value.count) bytes")
        }
    }

    private func submit(_ plan: MelkLightingWritePlan) -> Bool {
        submit([plan])
    }

    private func submit(_ plans: [MelkLightingWritePlan]) -> Bool {
        // Admit a complete state together; never retain unbounded color-drag traffic.
        guard connectionState == .ready, sink != nil, peripheral != nil,
              !plans.isEmpty,
              plans.allSatisfy({
                  if case .writeWithoutResponse = $0.operation { return true }
                  return false
              }) else { return false }

        if plans.allSatisfy(Self.isCoalescibleColorWrite) {
            // A drag produces many superseded colors; keep only the newest one.
            pendingWrites.removeAll(where: Self.isCoalescibleColorWrite)
        }
        guard pendingWrites.count + plans.count <= 32 else { return false }

        pendingWrites.append(contentsOf: plans)
        commandEvidence.requested()
        drainWrites()
        return true
    }

    private static func isCoalescibleColorWrite(_ plan: MelkLightingWritePlan) -> Bool {
        guard plan.confirmationChannel == MelkLightingCommandProfile.notify,
              case let .writeWithoutResponse(channel, bytes) = plan.operation,
              channel == MelkLightingCommandProfile.write,
              bytes.count == 9 else {
            return false
        }
        return bytes[0] == 0x7e
            && bytes[1] == 0
            && bytes[2] == 5
            && bytes[3] == 3
            && bytes[8] == 0xef
    }

    private func drainInitialization() {
        guard connectionState == .discovering,
              initializationTask == nil,
              let peripheral,
              let sink,
              !pendingInitialization.isEmpty,
              peripheral.canSendWriteWithoutResponse else {
            return
        }
        let plan = pendingInitialization.removeFirst()
        guard case let .writeWithoutResponse(channel, bytes) = plan.operation else {
            resetInitialization()
            transition(to: .failed("invalid MELK initialization write"))
            return
        }
        sink.writeWithoutResponse(channel: channel, bytes: bytes)
        record("initialization=\(bytes.map { String(format: "%02x", $0) }.joined())")
        guard !pendingInitialization.isEmpty else {
            finishReadyIfPossible()
            return
        }

        let task = DispatchWorkItem { [weak self, weak peripheral] in
            guard let self, let peripheral else { return }
            self.onQueue {
                guard self.peripheral === peripheral,
                      self.connectionState == .discovering else {
                    return
                }
                self.initializationTask = nil
                self.drainInitialization()
            }
        }
        initializationTask = task
        queue.asyncAfter(deadline: .now() + 1, execute: task)
    }

    private func finishReadyIfPossible() {
        guard notificationReady,
              pendingInitialization.isEmpty,
              initializationTask == nil else {
            return
        }
        transition(to: .ready)
        record("notify_state=true")
    }

    private func drainWrites() {
        guard connectionState == .ready,
              let peripheral,
              let sink,
              writeDrainTask == nil,
              peripheral.canSendWriteWithoutResponse,
              !pendingWrites.isEmpty else { return }

        let plan = pendingWrites.removeFirst()
        if case let .writeWithoutResponse(channel, bytes) = plan.operation {
            sink.writeWithoutResponse(channel: channel, bytes: bytes)
            record("requested=\(bytes.map { String(format: "%02x", $0) }.joined())")
        }

        // MELK accepts write-without-response frames, but a burst can exhaust its
        // small controller-side queue. Keep the profile-provided cadence when present;
        // MELK currently falls back to the 50 ms cadence observed on hardware.
        let delayMilliseconds = Int(plan.minimumIntervalMilliseconds ?? 50)
        let task = DispatchWorkItem { [weak self, weak peripheral] in
            guard let self, let peripheral else { return }
            self.onQueue {
                guard self.peripheral === peripheral,
                      self.connectionState == .ready else { return }
                self.writeDrainTask = nil
                self.drainWrites()
            }
        }
        writeDrainTask = task
        queue.asyncAfter(deadline: .now() + .milliseconds(delayMilliseconds), execute: task)
    }

    public func peripheralIsReady(toSendWriteWithoutResponse peripheral: CBPeripheral) {
        onQueue {
            guard peripheral === self.peripheral else { return }
            drainInitialization()
            drainWrites()
        }
    }

    private func scheduleReconnect(
        central: CBCentralManager,
        peripheral: CBPeripheral,
        reason: String
    ) {
        guard let schedule = reconnectController.schedule(jitter: 0.5, operation: { [weak self] in
            guard let self,
                  self.reconnectEnabled,
                  self.central === central,
                  self.peripheral?.identifier == peripheral.identifier,
                  central.state == .poweredOn else {
                return
            }
            self.transition(to: .connecting)
            central.connect(peripheral)
            self.scheduleConnectionAttemptTimeout(central: central, peripheral: peripheral)
        }) else {
            transition(to: .failed(
                "Accessory reconnect exhausted after \(ConnectionReconnectPolicy.maximumAttempts) attempts"
            ))
            record("reconnect_exhausted reason=\(reason)")
            return
        }

        transition(to: .retrying(
            attempt: schedule.attempt,
            delayMilliseconds: schedule.delayMilliseconds
        ))
        record(
            "reconnect_attempt=\(schedule.attempt) delay_ms=\(schedule.delayMilliseconds) reason=\(reason)"
        )
    }

    private func transition(to state: MelkLightingPeripheralState) {
        if state != .ready {
            pendingWrites.removeAll()
            writeDrainTask?.cancel()
            writeDrainTask = nil
            resetInitialization()
        }
        connectionState = state
        onStateChange?(state)
    }

    private func resetInitialization() {
        initializationTask?.cancel()
        initializationTask = nil
        pendingInitialization.removeAll(keepingCapacity: true)
        notificationReady = false
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

extension MelkLightingPeripheralSession: MelkLightingPeripheralSessionProtocol {}

#endif
