import Foundation

public struct MonotonicMilliseconds: Equatable, Hashable, Sendable {
    public let rawValue: UInt64

    public init(_ rawValue: UInt64) {
        self.rawValue = rawValue
    }

    fileprivate var dto: MobileMonotonicMillisDto {
        MobileMonotonicMillisDto(milliseconds: rawValue)
    }
}

public struct TransportWriteLimitBytes: Equatable, Hashable, Sendable {
    public let rawValue: UInt16

    public init(_ rawValue: UInt16) {
        self.rawValue = rawValue
    }

    fileprivate var dto: MobileTransportWriteLimitDto {
        MobileTransportWriteLimitDto(bytes: rawValue)
    }
}

public enum SessionActionKind: Equatable, Hashable, Sendable {
    case subscribe
    case write
    case event
    case disconnect
    case notificationIngest

    fileprivate init(_ dto: MobileSessionOutputKindDto) {
        switch dto {
        case .subscribe:
            self = .subscribe
        case .write:
            self = .write
        case .event:
            self = .event
        case .disconnect:
            self = .disconnect
        case .notificationIngest:
            self = .notificationIngest
        }
    }
}

public struct SessionAction: Equatable, Hashable, Sendable {
    public let kind: SessionActionKind
    public let channel: Data
    public let bytes: Data

    public init(kind: SessionActionKind, channel: Data, bytes: Data) {
        self.kind = kind
        self.channel = channel
        self.bytes = bytes
    }

    fileprivate init(_ dto: MobileSessionOutputDto) {
        self.kind = SessionActionKind(dto.kind)
        self.channel = dto.channel
        self.bytes = dto.bytes
    }
}

public enum DeviceCommand: Equatable, Hashable, Sendable {
    case requestIdentity
    case requestTelemetry
    case requestFirmwareInfo
    case requestBatteryInfo
    case requestDiagnostics
    case soundHorn

    fileprivate init(_ dto: MobileCommandDto) {
        switch dto {
        case .requestIdentity:
            self = .requestIdentity
        case .requestTelemetry:
            self = .requestTelemetry
        case .requestFirmwareInfo:
            self = .requestFirmwareInfo
        case .requestBatteryInfo:
            self = .requestBatteryInfo
        case .requestDiagnostics:
            self = .requestDiagnostics
        case .soundHorn:
            self = .soundHorn
        }
    }

    fileprivate var dto: MobileCommandDto {
        switch self {
        case .requestIdentity:
            .requestIdentity
        case .requestTelemetry:
            .requestTelemetry
        case .requestFirmwareInfo:
            .requestFirmwareInfo
        case .requestBatteryInfo:
            .requestBatteryInfo
        case .requestDiagnostics:
            .requestDiagnostics
        case .soundHorn:
            .soundHorn
        }
    }
}

public struct TelemetrySnapshot: Equatable, Hashable, Sendable {
    public let voltageMillivolts: Int32?
    public let batteryLevelEstimated: UInt8?

    fileprivate init(_ dto: MobileTelemetrySnapshotDto) {
        self.voltageMillivolts = dto.voltage?.value
        self.batteryLevelEstimated = dto.batteryLevelEstimated?.value
    }
}

public struct ParserDiagnostics: Equatable, Hashable, Sendable {
    public let droppedBytes: UInt64
    public let resyncs: UInt64
    public let malformedFrames: UInt64
    public let badChecksums: UInt64
    public let timeouts: UInt64
    public let oversizedFrames: UInt64
    public let unmatchedReplies: UInt64

    fileprivate init(_ dto: MobileParserDiagnosticsDto) {
        self.droppedBytes = dto.droppedBytes.bytes
        self.resyncs = dto.resyncs.count
        self.malformedFrames = dto.malformedFrames.count
        self.badChecksums = dto.badChecksums.count
        self.timeouts = dto.timeouts.count
        self.oversizedFrames = dto.oversizedFrames.count
        self.unmatchedReplies = dto.unmatchedReplies.count
    }
}

public enum CutoutSessionError: Error, Equatable, Sendable {
    case commandRefused(DeviceCommand?, String?)
    case unsupportedFalconProfile
    case unexpectedStepError(String?)

    fileprivate init(_ dto: MobileSessionStepErrorDto) {
        switch dto.kind {
        case .commandRefused:
            self = .commandRefused(dto.command.map(DeviceCommand.init), dto.reason)
        case .unsupportedFalconProfile:
            self = .unsupportedFalconProfile
        }
    }
}

public final class AeroSession: @unchecked Sendable {
    private let inner: AeroReadOnlySession

    public init() {
        self.inner = AeroReadOnlySession()
    }

    public var diagnostics: ParserDiagnostics {
        ParserDiagnostics(inner.diagnostics())
    }

    public var currentSnapshot: TelemetrySnapshot {
        TelemetrySnapshot(inner.currentSnapshot())
    }

    public func linkUp(
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes
    ) throws -> [SessionAction] {
        try inner.step(.linkUp, at: monotonicMilliseconds, writeLimit: writeLimit)
    }

    public func ingestNotification(
        _ bytes: Data,
        channel: Data,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> TelemetrySnapshot {
        _ = try inner.step(.notification, at: monotonicMilliseconds, channel: channel, bytes: bytes)
        return TelemetrySnapshot(inner.currentSnapshot())
    }
}

public final class FalconSession: @unchecked Sendable {
    private let inner: FalconReadOnlySession

    public init() throws {
        self.inner = try FalconReadOnlySession()
    }

    public var diagnostics: ParserDiagnostics {
        ParserDiagnostics(inner.diagnostics())
    }

    public var currentSnapshot: TelemetrySnapshot {
        TelemetrySnapshot(inner.currentSnapshot())
    }

    public func linkUp(
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes
    ) throws -> [SessionAction] {
        try inner.step(.linkUp, at: monotonicMilliseconds, writeLimit: writeLimit)
    }

    public func ingestNotification(
        _ bytes: Data,
        channel: Data,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> TelemetrySnapshot {
        _ = try inner.step(.notification, at: monotonicMilliseconds, channel: channel, bytes: bytes)
        return TelemetrySnapshot(inner.currentSnapshot())
    }

    public func soundHorn(at monotonicMilliseconds: MonotonicMilliseconds) throws -> [SessionAction] {
        try inner.step(.command, at: monotonicMilliseconds, command: .soundHorn)
    }
}

private protocol MobileReadOnlySession {
    func ingestChecked(input: MobileSessionInputDto) -> MobileSessionStepResultDto
}

extension AeroReadOnlySession: MobileReadOnlySession {}
extension FalconReadOnlySession: MobileReadOnlySession {}

private extension MobileReadOnlySession {
    func step(
        _ kind: MobileSessionInputKindDto,
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes? = nil,
        channel: Data = Data(),
        bytes: Data = Data(),
        command: DeviceCommand? = nil
    ) throws -> [SessionAction] {
        let result = ingestChecked(input: MobileSessionInputDto(
            kind: kind,
            monotonicMs: monotonicMilliseconds.dto,
            maxWriteLen: writeLimit?.dto,
            channel: channel,
            bytes: bytes,
            command: command?.dto
        ))
        if let error = result.error {
            throw CutoutSessionError(error)
        }
        return result.outputs.map(SessionAction.init)
    }
}

public struct BluetoothUuid: Equatable, Hashable, Sendable {
    public let bytes: Data

    public init?(_ bytes: Data) {
        guard bytes.count == 16 else {
            return nil
        }
        self.bytes = bytes
    }

    public static func bluetooth16(_ value: UInt16) -> BluetoothUuid {
        let high = UInt8((value >> 8) & 0xff)
        let low = UInt8(value & 0xff)
        return BluetoothUuid(Data([
            0x00, 0x00, high, low,
            0x00, 0x00,
            0x10, 0x00,
            0x80, 0x00,
            0x00, 0x80,
            0x5f, 0x9b, 0x34, 0xfb,
        ]))!
    }
}

public struct CoreBluetoothPeripheralIdentifier: Equatable, Hashable, Sendable {
    public let rawValue: String

    public init(_ rawValue: String) {
        self.rawValue = rawValue
    }
}

public enum CutoutModelHint: Equatable, Hashable, Sendable {
    case aero
    case falcon
    case unknown
}

public struct CoreBluetoothAdvertisement: Equatable, Hashable, Sendable {
    public let peripheralIdentifier: CoreBluetoothPeripheralIdentifier
    public let localName: String?
    public let advertisedServiceUuids: [BluetoothUuid]

    public init(
        peripheralIdentifier: CoreBluetoothPeripheralIdentifier,
        localName: String?,
        advertisedServiceUuids: [BluetoothUuid]
    ) {
        self.peripheralIdentifier = peripheralIdentifier
        self.localName = localName
        self.advertisedServiceUuids = advertisedServiceUuids
    }

    public var modelHint: CutoutModelHint {
        let normalizedName = localName?.lowercased() ?? ""
        if normalizedName.contains("falcon") {
            return .falcon
        }
        if normalizedName.contains("aero") || normalizedName.contains("nosfet") {
            return .aero
        }
        return .unknown
    }
}

public enum CoreBluetoothPlannedOperation: Equatable, Hashable, Sendable {
    case subscribe(channel: BluetoothUuid)
    case writeWithoutResponse(channel: BluetoothUuid, bytes: Data)
    case disconnect
}

public struct CoreBluetoothTransportPlanner: Equatable, Hashable, Sendable {
    public let writeLimit: TransportWriteLimitBytes

    public init(writeLimit: TransportWriteLimitBytes) {
        self.writeLimit = writeLimit
    }

    public func plan(action: SessionAction) -> [CoreBluetoothPlannedOperation] {
        guard let channel = BluetoothUuid(action.channel) else {
            return []
        }
        switch action.kind {
        case .subscribe:
            return [.subscribe(channel: channel)]
        case .write:
            return chunked(action.bytes, by: Int(writeLimit.rawValue)).map {
                .writeWithoutResponse(channel: channel, bytes: $0)
            }
        case .disconnect:
            return [.disconnect]
        case .event, .notificationIngest:
            return []
        }
    }

    private func chunked(_ bytes: Data, by chunkSize: Int) -> [Data] {
        guard chunkSize > 0 else {
            return []
        }
        return stride(from: 0, to: bytes.count, by: chunkSize).map { offset in
            bytes[offset..<Swift.min(offset + chunkSize, bytes.count)]
        }.map(Data.init)
    }
}

public enum CoreBluetoothSession: Sendable {
    case aero(AeroSession)
    case falcon(FalconSession)

    fileprivate var currentSnapshot: TelemetrySnapshot {
        switch self {
        case .aero(let session):
            session.currentSnapshot
        case .falcon(let session):
            session.currentSnapshot
        }
    }

    fileprivate func linkUp(
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes
    ) throws -> [SessionAction] {
        switch self {
        case .aero(let session):
            try session.linkUp(at: monotonicMilliseconds, writeLimit: writeLimit)
        case .falcon(let session):
            try session.linkUp(at: monotonicMilliseconds, writeLimit: writeLimit)
        }
    }

    fileprivate func ingestNotification(
        _ bytes: Data,
        channel: BluetoothUuid,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> TelemetrySnapshot {
        switch self {
        case .aero(let session):
            try session.ingestNotification(bytes, channel: channel.bytes, at: monotonicMilliseconds)
        case .falcon(let session):
            try session.ingestNotification(bytes, channel: channel.bytes, at: monotonicMilliseconds)
        }
    }
}

public enum CoreBluetoothSessionEvent: Equatable, Hashable, Sendable {
    case linkUp(at: MonotonicMilliseconds)
    case notification(bytes: Data, channel: BluetoothUuid, at: MonotonicMilliseconds)
    case linkDown(at: MonotonicMilliseconds)
}

public struct CoreBluetoothSessionStep: Equatable, Hashable, Sendable {
    public let operations: [CoreBluetoothPlannedOperation]
    public let snapshot: TelemetrySnapshot?
    public let captureContext: CoreBluetoothCaptureContext?

    public init(
        operations: [CoreBluetoothPlannedOperation],
        snapshot: TelemetrySnapshot?,
        captureContext: CoreBluetoothCaptureContext? = nil
    ) {
        self.operations = operations
        self.snapshot = snapshot
        self.captureContext = captureContext
    }
}

public final class CoreBluetoothSessionRunner: @unchecked Sendable {
    private let session: CoreBluetoothSession
    private let planner: CoreBluetoothTransportPlanner
    private let captureContext: CoreBluetoothCaptureContext?

    public init(
        session: CoreBluetoothSession,
        writeLimit: TransportWriteLimitBytes,
        captureContext: CoreBluetoothCaptureContext? = nil
    ) {
        self.session = session
        self.planner = CoreBluetoothTransportPlanner(writeLimit: writeLimit)
        self.captureContext = captureContext
    }

    public func handle(_ event: CoreBluetoothSessionEvent) throws -> CoreBluetoothSessionStep {
        switch event {
        case .linkUp(let monotonicMilliseconds):
            let actions = try session.linkUp(
                at: monotonicMilliseconds,
                writeLimit: planner.writeLimit
            )
            return CoreBluetoothSessionStep(
                operations: actions.flatMap(planner.plan(action:)),
                snapshot: session.currentSnapshot,
                captureContext: captureContext
            )

        case .notification(let bytes, let channel, let monotonicMilliseconds):
            let snapshot = try session.ingestNotification(
                bytes,
                channel: channel,
                at: monotonicMilliseconds
            )
            return CoreBluetoothSessionStep(
                operations: [],
                snapshot: snapshot,
                captureContext: captureContext
            )

        case .linkDown:
            return CoreBluetoothSessionStep(
                operations: [.disconnect],
                snapshot: session.currentSnapshot,
                captureContext: captureContext
            )
        }
    }
}

public protocol CoreBluetoothOperationSink: AnyObject {
    func subscribe(channel: BluetoothUuid)
    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data)
    func disconnect()
}

public struct CoreBluetoothOperationExecutor {
    private weak var sink: CoreBluetoothOperationSink?

    public init(sink: CoreBluetoothOperationSink) {
        self.sink = sink
    }

    public func execute(_ operations: [CoreBluetoothPlannedOperation]) {
        operations.forEach(execute)
    }

    public func execute(_ operation: CoreBluetoothPlannedOperation) {
        switch operation {
        case .subscribe(let channel):
            sink?.subscribe(channel: channel)
        case .writeWithoutResponse(let channel, let bytes):
            sink?.writeWithoutResponse(channel: channel, bytes: bytes)
        case .disconnect:
            sink?.disconnect()
        }
    }
}

public struct CoreBluetoothPayloadByteCount: Equatable, Hashable, Sendable {
    public let rawValue: Int

    public init(_ rawValue: Int) {
        self.rawValue = Swift.max(0, rawValue)
    }
}

public enum CoreBluetoothLiveRecord: Equatable, Hashable, Sendable {
    case linkUp(
        platformIdentifier: CoreBluetoothPeripheralIdentifier,
        writeLimit: TransportWriteLimitBytes
    )
    case gattInventory(
        platformIdentifier: CoreBluetoothPeripheralIdentifier,
        inventory: CoreBluetoothGattInventory
    )
    case operation(
        platformIdentifier: CoreBluetoothPeripheralIdentifier,
        operation: CoreBluetoothPlannedOperation
    )
    case notification(
        channel: BluetoothUuid,
        byteCount: CoreBluetoothPayloadByteCount,
        at: MonotonicMilliseconds
    )
    case linkDown(
        platformIdentifier: CoreBluetoothPeripheralIdentifier,
        at: MonotonicMilliseconds
    )
}

public typealias CoreBluetoothCentralLifecycleNotificationHandler = (BluetoothUuid, Data) -> Void
public typealias CoreBluetoothMonotonicClock = @Sendable () -> MonotonicMilliseconds
public typealias CoreBluetoothLiveSessionErrorHandler = @Sendable (Error) -> Void

public final class CoreBluetoothLiveSessionOwner: @unchecked Sendable {
    private let platformIdentifier: CoreBluetoothPeripheralIdentifier
    private let runner: CoreBluetoothSessionRunner
    private let retainedSink: CoreBluetoothOperationSink
    private let executor: CoreBluetoothOperationExecutor
    private var recorded: [CoreBluetoothLiveRecord] = []

    public init(
        session: CoreBluetoothSession,
        advertisement: CoreBluetoothAdvertisement,
        writeLimit: TransportWriteLimitBytes,
        operationSink: CoreBluetoothOperationSink
    ) {
        self.platformIdentifier = advertisement.peripheralIdentifier
        self.runner = CoreBluetoothSessionRunner(
            session: session,
            writeLimit: writeLimit,
            captureContext: CoreBluetoothCaptureContext(
                platformIdentifier: advertisement.peripheralIdentifier,
                advertisement: advertisement,
                writeLimit: writeLimit
            )
        )
        self.retainedSink = operationSink
        self.executor = CoreBluetoothOperationExecutor(sink: operationSink)
    }

    public var records: [CoreBluetoothLiveRecord] {
        recorded
    }

    @discardableResult
    public func handleLinkUp(at monotonicMilliseconds: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        let step = try runner.handle(.linkUp(at: monotonicMilliseconds))
        recorded.append(.linkUp(
            platformIdentifier: platformIdentifier,
            writeLimit: step.captureContext?.writeLimit ?? TransportWriteLimitBytes(0)
        ))
        executeAndRecord(step.operations)
        return step
    }

    public func recordInventory(_ inventory: CoreBluetoothGattInventory) {
        recorded.append(.gattInventory(
            platformIdentifier: platformIdentifier,
            inventory: inventory
        ))
    }

    @discardableResult
    public func handleNotification(
        bytes: Data,
        channel: BluetoothUuid,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> CoreBluetoothSessionStep {
        recorded.append(.notification(
            channel: channel,
            byteCount: CoreBluetoothPayloadByteCount(bytes.count),
            at: monotonicMilliseconds
        ))
        let step = try runner.handle(.notification(
            bytes: bytes,
            channel: channel,
            at: monotonicMilliseconds
        ))
        executeAndRecord(step.operations)
        return step
    }

    @discardableResult
    public func handleLinkDown(at monotonicMilliseconds: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        let step = try runner.handle(.linkDown(at: monotonicMilliseconds))
        recorded.append(.linkDown(
            platformIdentifier: platformIdentifier,
            at: monotonicMilliseconds
        ))
        executeAndRecord(step.operations)
        return step
    }

    public func notificationHandler(
        clock: @escaping CoreBluetoothMonotonicClock,
        onError: @escaping CoreBluetoothLiveSessionErrorHandler
    ) -> CoreBluetoothCentralLifecycleNotificationHandler {
        { [weak self] channel, bytes in
            do {
                try self?.handleNotification(
                    bytes: bytes,
                    channel: channel,
                    at: clock()
                )
            } catch {
                onError(error)
            }
        }
    }

    private func executeAndRecord(_ operations: [CoreBluetoothPlannedOperation]) {
        operations.forEach { operation in
            executor.execute(operation)
            recorded.append(.operation(
                platformIdentifier: platformIdentifier,
                operation: operation
            ))
        }
    }
}

public struct CoreBluetoothCaptureContext: Equatable, Hashable, Sendable {
    public let platformIdentifier: CoreBluetoothPeripheralIdentifier
    public let advertisedServiceUuids: [BluetoothUuid]
    public let writeLimit: TransportWriteLimitBytes
    public let resolvedModelHint: CutoutModelHint

    public init(
        platformIdentifier: CoreBluetoothPeripheralIdentifier,
        advertisement: CoreBluetoothAdvertisement,
        writeLimit: TransportWriteLimitBytes
    ) {
        self.platformIdentifier = platformIdentifier
        self.advertisedServiceUuids = advertisement.advertisedServiceUuids
        self.writeLimit = writeLimit
        self.resolvedModelHint = advertisement.modelHint
    }
}

public struct CoreBluetoothScanPolicy: Equatable, Hashable, Sendable {
    public let serviceUuids: [BluetoothUuid]

    public init(serviceUuids: [BluetoothUuid]) {
        self.serviceUuids = serviceUuids
    }

    public static let aeroFalcon = CoreBluetoothScanPolicy(serviceUuids: [
        .bluetooth16(0xffe0),
        .bluetooth16(0xfff0),
    ])
}

public enum CoreBluetoothCharacteristicProperty: Equatable, Hashable, Sendable {
    case read
    case write
    case writeWithoutResponse
    case notify
    case indicate
}

public struct CoreBluetoothGattCharacteristic: Equatable, Hashable, Sendable {
    public let uuid: BluetoothUuid
    public let properties: Set<CoreBluetoothCharacteristicProperty>

    public init(uuid: BluetoothUuid, properties: Set<CoreBluetoothCharacteristicProperty>) {
        self.uuid = uuid
        self.properties = properties
    }
}

public struct CoreBluetoothGattService: Equatable, Hashable, Sendable {
    public let uuid: BluetoothUuid
    public let characteristics: [CoreBluetoothGattCharacteristic]

    public init(uuid: BluetoothUuid, characteristics: [CoreBluetoothGattCharacteristic]) {
        self.uuid = uuid
        self.characteristics = characteristics
    }
}

public struct CoreBluetoothGattInventory: Equatable, Hashable, Sendable {
    public let services: [CoreBluetoothGattService]

    public init(services: [CoreBluetoothGattService]) {
        self.services = services
    }
}

public enum CoreBluetoothCentralAction: Equatable, Hashable, Sendable {
    case scan(serviceUuids: [BluetoothUuid])
    case connect(peripheralIdentifier: CoreBluetoothPeripheralIdentifier)
    case discoverServices([BluetoothUuid])
    case discoverCharacteristics(service: BluetoothUuid, characteristics: [BluetoothUuid])
}

public struct CoreBluetoothCentralCoordinator: Equatable, Hashable, Sendable {
    public let scanPolicy: CoreBluetoothScanPolicy
    public let writeLimit: TransportWriteLimitBytes

    public init(
        scanPolicy: CoreBluetoothScanPolicy = .aeroFalcon,
        writeLimit: TransportWriteLimitBytes
    ) {
        self.scanPolicy = scanPolicy
        self.writeLimit = writeLimit
    }

    public func startScanning() -> CoreBluetoothCentralAction {
        .scan(serviceUuids: scanPolicy.serviceUuids)
    }

    public func handleDiscovered(_ advertisement: CoreBluetoothAdvertisement) -> CoreBluetoothCentralAction? {
        guard scanPolicy.matches(advertisement), advertisement.modelHint != .unknown else {
            return nil
        }
        return .connect(peripheralIdentifier: advertisement.peripheralIdentifier)
    }

    public func discoverServices() -> CoreBluetoothCentralAction {
        .discoverServices(scanPolicy.serviceUuids)
    }

    public func discoverCharacteristics(in inventory: CoreBluetoothGattInventory) -> [CoreBluetoothCentralAction] {
        inventory.services
            .filter { scanPolicy.serviceUuids.contains($0.uuid) }
            .map { service in
                .discoverCharacteristics(
                    service: service.uuid,
                    characteristics: service.characteristics.map(\.uuid)
                )
            }
    }
}

private extension CoreBluetoothScanPolicy {
    func matches(_ advertisement: CoreBluetoothAdvertisement) -> Bool {
        !Set(serviceUuids).isDisjoint(with: advertisement.advertisedServiceUuids)
    }
}

#if canImport(CoreBluetooth)
import CoreBluetooth

public extension CoreBluetoothScanPolicy {
    var coreBluetoothServiceUuids: [CBUUID] {
        serviceUuids.map(\.coreBluetoothUuid)
    }
}

public extension CoreBluetoothCentralAction {
    var coreBluetoothServiceUuids: [CBUUID] {
        switch self {
        case .scan(let serviceUuids), .discoverServices(let serviceUuids):
            serviceUuids.map(\.coreBluetoothUuid)
        case .discoverCharacteristics(_, let characteristics):
            characteristics.map(\.coreBluetoothUuid)
        case .connect:
            []
        }
    }
}

public extension BluetoothUuid {
    var coreBluetoothUuid: CBUUID {
        if let shortUuid = bluetooth16Value {
            return CBUUID(string: String(format: "%04X", shortUuid))
        }
        return CBUUID(data: bytes)
    }

    init?(coreBluetoothUuid: CBUUID) {
        switch coreBluetoothUuid.data.count {
        case 2:
            let value = coreBluetoothUuid.data.reduce(UInt16(0)) { ($0 << 8) | UInt16($1) }
            self = .bluetooth16(value)
        case 16:
            self.init(coreBluetoothUuid.data)
        default:
            return nil
        }
    }

    private var bluetooth16Value: UInt16? {
        let baseSuffix = Data([
            0x00, 0x00,
            0x10, 0x00,
            0x80, 0x00,
            0x00, 0x80,
            0x5f, 0x9b, 0x34, 0xfb,
        ])
        guard bytes.count == 16, bytes.prefix(2) == Data([0x00, 0x00]), bytes.suffix(12) == baseSuffix else {
            return nil
        }
        return (UInt16(bytes[2]) << 8) | UInt16(bytes[3])
    }
}

public extension CoreBluetoothAdvertisement {
    init(peripheral: CBPeripheral, advertisementData: [String: Any]) {
        let localName = advertisementData[CBAdvertisementDataLocalNameKey] as? String
        let serviceUuids = (
            advertisementData[CBAdvertisementDataServiceUUIDsKey] as? [CBUUID] ?? []
        ).compactMap(BluetoothUuid.init(coreBluetoothUuid:))
        self.init(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier(peripheral.identifier.uuidString),
            localName: localName ?? peripheral.name,
            advertisedServiceUuids: serviceUuids
        )
    }
}

public extension CoreBluetoothCharacteristicProperty {
    init?(coreBluetoothProperty: CBCharacteristicProperties) {
        switch coreBluetoothProperty {
        case .read:
            self = .read
        case .write:
            self = .write
        case .writeWithoutResponse:
            self = .writeWithoutResponse
        case .notify:
            self = .notify
        case .indicate:
            self = .indicate
        default:
            return nil
        }
    }
}

public extension CoreBluetoothGattCharacteristic {
    init?(characteristic: CBCharacteristic) {
        guard let uuid = BluetoothUuid(coreBluetoothUuid: characteristic.uuid) else {
            return nil
        }
        let candidateProperties: [CoreBluetoothCharacteristicProperty] = [
            .read,
            .write,
            .writeWithoutResponse,
            .notify,
            .indicate,
        ]
        let properties = Set(candidateProperties.filter {
            characteristic.properties.contains($0.coreBluetoothProperty)
        })
        self.init(uuid: uuid, properties: properties)
    }
}

public extension CoreBluetoothGattService {
    init?(service: CBService) {
        guard let uuid = BluetoothUuid(coreBluetoothUuid: service.uuid) else {
            return nil
        }
        self.init(
            uuid: uuid,
            characteristics: (service.characteristics ?? []).compactMap(CoreBluetoothGattCharacteristic.init)
        )
    }
}

public extension CoreBluetoothGattInventory {
    init(services: [CBService]) {
        self.init(services: services.compactMap(CoreBluetoothGattService.init))
    }
}

public final class CoreBluetoothCentralLifecycle: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    public typealias ActionHandler = (CoreBluetoothCentralAction) -> Void
    public typealias NotificationHandler = CoreBluetoothCentralLifecycleNotificationHandler

    private let coordinator: CoreBluetoothCentralCoordinator
    private let onAction: ActionHandler
    private let onNotification: NotificationHandler
    private lazy var centralManager = CBCentralManager(delegate: self, queue: nil)

    public init(
        coordinator: CoreBluetoothCentralCoordinator,
        onAction: @escaping ActionHandler,
        onNotification: @escaping NotificationHandler
    ) {
        self.coordinator = coordinator
        self.onAction = onAction
        self.onNotification = onNotification
        super.init()
    }

    public func start() {
        _ = centralManager
        if centralManager.state == .poweredOn {
            scan()
        }
    }

    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        guard central.state == .poweredOn else {
            return
        }
        scan()
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi _: NSNumber
    ) {
        let advertisement = CoreBluetoothAdvertisement(
            peripheral: peripheral,
            advertisementData: advertisementData
        )
        guard case .connect = coordinator.handleDiscovered(advertisement) else {
            return
        }
        onAction(.connect(peripheralIdentifier: advertisement.peripheralIdentifier))
        peripheral.delegate = self
        central.connect(peripheral)
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        central.stopScan()
        peripheral.delegate = self
        let action = coordinator.discoverServices()
        onAction(action)
        peripheral.discoverServices(action.coreBluetoothServiceUuids)
    }

    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard error == nil else {
            return
        }
        (peripheral.services ?? []).forEach { service in
            peripheral.discoverCharacteristics(nil, for: service)
        }
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        guard error == nil else {
            return
        }
        let inventory = CoreBluetoothGattInventory(services: [service])
        coordinator.discoverCharacteristics(in: inventory).forEach(onAction)
    }

    public func peripheral(
        _: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        guard
            error == nil,
            let value = characteristic.value,
            let channel = BluetoothUuid(coreBluetoothUuid: characteristic.uuid)
        else {
            return
        }
        onNotification(channel, value)
    }

    private func scan() {
        let action = coordinator.startScanning()
        onAction(action)
        centralManager.scanForPeripherals(withServices: action.coreBluetoothServiceUuids)
    }
}

public extension CoreBluetoothLiveSessionOwner {
    convenience init(
        session: CoreBluetoothSession,
        advertisement: CoreBluetoothAdvertisement,
        peripheral: CBPeripheral
    ) {
        self.init(
            session: session,
            advertisement: advertisement,
            writeLimit: TransportWriteLimitBytes(peripheral.withoutResponseWriteLimit),
            operationSink: CoreBluetoothPeripheralOperationSink(peripheral: peripheral)
        )
    }
}

private extension CBPeripheral {
    var withoutResponseWriteLimit: UInt16 {
        UInt16(clamping: maximumWriteValueLength(for: .withoutResponse))
    }
}

private extension CoreBluetoothCharacteristicProperty {
    var coreBluetoothProperty: CBCharacteristicProperties {
        switch self {
        case .read:
            .read
        case .write:
            .write
        case .writeWithoutResponse:
            .writeWithoutResponse
        case .notify:
            .notify
        case .indicate:
            .indicate
        }
    }
}

public final class CoreBluetoothPeripheralOperationSink: CoreBluetoothOperationSink {
    private let peripheral: CBPeripheral

    public init(peripheral: CBPeripheral) {
        self.peripheral = peripheral
    }

    public func subscribe(channel: BluetoothUuid) {
        guard let characteristic = peripheral.characteristic(for: channel) else {
            return
        }
        peripheral.setNotifyValue(true, for: characteristic)
    }

    public func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        guard let characteristic = peripheral.characteristic(for: channel) else {
            return
        }
        peripheral.writeValue(bytes, for: characteristic, type: .withoutResponse)
    }

    public func disconnect() {
        // Disconnect is owned by CBCentralManager; this sink only owns peripheral operations.
    }
}

private extension CBPeripheral {
    func characteristic(for channel: BluetoothUuid) -> CBCharacteristic? {
        let uuid = channel.coreBluetoothUuid
        return services?
            .lazy
            .compactMap { $0.characteristics }
            .joined()
            .first { $0.uuid == uuid }
    }
}
#endif
