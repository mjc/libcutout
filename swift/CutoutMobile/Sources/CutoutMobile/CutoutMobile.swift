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

#if canImport(CoreBluetooth)
import CoreBluetooth

public extension CoreBluetoothScanPolicy {
    var coreBluetoothServiceUuids: [CBUUID] {
        serviceUuids.map { CBUUID(data: $0.bytes) }
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
        let uuid = CBUUID(data: channel.bytes)
        return services?
            .lazy
            .compactMap { $0.characteristics }
            .joined()
            .first { $0.uuid == uuid }
    }
}
#endif
