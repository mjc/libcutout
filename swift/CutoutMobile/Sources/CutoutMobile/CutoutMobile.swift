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
    case settingsReadback
    case faultHistoryReadback
    case bmsSnapshot

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
        case .settingsReadback:
            self = .settingsReadback
        case .faultHistoryReadback:
            self = .faultHistoryReadback
        case .bmsSnapshot:
            self = .bmsSnapshot
        }
    }
}

public struct SessionAction: Equatable, Hashable, Sendable {
    public let kind: SessionActionKind
    public let channel: Data
    public let bytes: Data
    public let settingsReadback: SettingsReadback?
    public let faultHistoryReadback: FaultHistoryReadback?
    public let bmsSnapshot: BmsSnapshot?

    public init(
        kind: SessionActionKind,
        channel: Data,
        bytes: Data,
        settingsReadback: SettingsReadback? = nil,
        faultHistoryReadback: FaultHistoryReadback? = nil,
        bmsSnapshot: BmsSnapshot? = nil
    ) {
        self.kind = kind
        self.channel = channel
        self.bytes = bytes
        self.settingsReadback = settingsReadback
        self.faultHistoryReadback = faultHistoryReadback
        self.bmsSnapshot = bmsSnapshot
    }

    fileprivate init(_ dto: MobileSessionOutputDto) {
        self.kind = SessionActionKind(dto.kind)
        self.channel = dto.channel
        self.bytes = dto.bytes
        self.settingsReadback = dto.settingsReadback.map(SettingsReadback.init)
        self.faultHistoryReadback = dto.faultHistoryReadback.map(FaultHistoryReadback.init)
        self.bmsSnapshot = dto.bmsSnapshot.map(BmsSnapshot.init)
    }
}

public struct RawSettingField: Equatable, Hashable, Sendable {
    public let id: UInt16
    public let value: Int64

    public init(id: UInt16, value: Int64) {
        self.id = id
        self.value = value
    }

    fileprivate init(_ dto: MobileRawFieldValueDto) {
        self.id = dto.id
        self.value = dto.value
    }
}

public struct FaultCode: Equatable, Hashable, Sendable {
    public let raw: RawSettingField

    public static func unknown(id: UInt16, value: Int64) -> Self {
        Self(raw: RawSettingField(id: id, value: value))
    }

    public init(raw: RawSettingField) {
        self.raw = raw
    }

    fileprivate init(_ dto: MobileFaultCodeDto) {
        self.raw = RawSettingField(dto.raw)
    }
}

public enum ReadbackSource: Equatable, Hashable, Sendable {
    case reported
    case calculated
    case estimated

    fileprivate init(_ dto: MobileValueSourceDto) {
        switch dto {
        case .reported:
            self = .reported
        case .calculated:
            self = .calculated
        case .estimated:
            self = .estimated
        }
    }
}

public enum ReadbackQuality: Equatable, Hashable, Sendable {
    case known
    case inferred

    fileprivate init(_ dto: MobileValueQualityDto) {
        switch dto {
        case .known:
            self = .known
        case .inferred:
            self = .inferred
        }
    }
}

public enum VerificationState: Equatable, Hashable, Sendable {
    case unverified
    case inferred
    case sourceVerified
    case hardwareVerified
    case sourceAndHardwareVerified

    fileprivate init(_ dto: MobileVerificationStatusDto) {
        switch dto {
        case .unverified:
            self = .unverified
        case .inferred:
            self = .inferred
        case .sourceVerified:
            self = .sourceVerified
        case .hardwareVerified:
            self = .hardwareVerified
        case .sourceAndHardwareVerified:
            self = .sourceAndHardwareVerified
        }
    }
}

public struct SettingsReadbackEntry: Equatable, Hashable, Sendable {
    public let field: RawSettingField
    public let source: ReadbackSource
    public let quality: ReadbackQuality
    public let verification: VerificationState

    public init(
        field: RawSettingField,
        source: ReadbackSource,
        quality: ReadbackQuality,
        verification: VerificationState
    ) {
        self.field = field
        self.source = source
        self.quality = quality
        self.verification = verification
    }

    fileprivate init(_ dto: MobileSettingsEntryDto) {
        self.field = RawSettingField(dto.field)
        self.source = ReadbackSource(dto.source)
        self.quality = ReadbackQuality(dto.quality)
        self.verification = VerificationState(dto.verification)
    }
}

public struct SettingsReadback: Equatable, Hashable, Sendable {
    public let entries: [SettingsReadbackEntry]
    public let availability: ReadbackAvailability

    public init(
        entries: [SettingsReadbackEntry],
        availability: ReadbackAvailability = .available
    ) {
        self.entries = entries
        self.availability = availability
    }

    fileprivate init(_ dto: MobileSettingsReadbackDto) {
        self.entries = dto.entries.map(SettingsReadbackEntry.init)
        self.availability = ReadbackAvailability(dto.availability)
    }
}

public extension SettingsReadback {
    var eucGarageSettings: EucGarageSettingsSnapshot {
        EucGarageSettingsSnapshot(
            beepMargin: speedReadback(for: VeteranSettingsField.speedAlertDeciKmh),
            tiltback: speedReadback(for: VeteranSettingsField.speedTiltbackDeciKmh),
            pedalHardness: missingReadback()
        )
    }

    private func speedReadback(for fieldID: UInt16) -> ReadbackValue<Speed> {
        entries
            .first { $0.field.id == fieldID }
            .flatMap { speedFromDeciKmh($0.field.value) }
            .map(ReadbackValue.available)
            ?? missingReadback()
    }

    private func speedFromDeciKmh(_ value: Int64) -> Speed? {
        guard value >= 0 else {
            return nil
        }
        let millimetersPerSecond = value * 100 * 5 / 18
        return Int32(exactly: millimetersPerSecond).map(Speed.init(value:))
    }

    private func missingReadback<Value>() -> ReadbackValue<Value> {
        switch availability {
        case .available:
            .unavailable
        case .unavailable:
            .unavailable
        case .unsupported:
            .unsupported
        }
    }
}

private enum VeteranSettingsField {
    static let speedAlertDeciKmh: UInt16 = 0x0005
    static let speedTiltbackDeciKmh: UInt16 = 0x0006
}

public struct FaultHistoryEntry: Equatable, Hashable, Sendable {
    public let code: FaultCode
    public let source: ReadbackSource
    public let quality: ReadbackQuality
    public let verification: VerificationState

    public init(
        code: FaultCode,
        source: ReadbackSource,
        quality: ReadbackQuality,
        verification: VerificationState
    ) {
        self.code = code
        self.source = source
        self.quality = quality
        self.verification = verification
    }

    fileprivate init(_ dto: MobileFaultHistoryEntryDto) {
        self.code = FaultCode(dto.code)
        self.source = ReadbackSource(dto.source)
        self.quality = ReadbackQuality(dto.quality)
        self.verification = VerificationState(dto.verification)
    }
}

public struct FaultHistoryReadback: Equatable, Hashable, Sendable {
    public let availability: ReadbackAvailability
    public let lastFault: FaultHistoryEntry?
    public let sinceDistance: Distance?

    public init(
        availability: ReadbackAvailability,
        lastFault: FaultHistoryEntry? = nil,
        sinceDistance: Distance? = nil
    ) {
        self.availability = availability
        self.lastFault = lastFault
        self.sinceDistance = sinceDistance
    }

    fileprivate init(_ dto: MobileFaultHistoryReadbackDto) {
        self.availability = ReadbackAvailability(dto.availability)
        self.lastFault = dto.lastFault.map(FaultHistoryEntry.init)
        self.sinceDistance = dto.sinceDistance?.value
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
    public let speed: Speed?
    public let operatingState: RideOperatingState
    public let voltage: Voltage?
    public let batteryCurrent: BatteryCurrent?
    public let motorCurrent: PhaseCurrent?
    public let power: Power?
    public let powerFlow: PowerFlowDirection?
    public let controllerTemperature: Temperature?
    public let motorTemperature: Temperature?
    public let batteryTemperature: Temperature?
    public let pwm: DutyCycle?
    public let distance: Distance?
    public let pitch: Angle?
    public let roll: Angle?
    public let batteryLevelReported: BatteryLevel?
    public let batteryLevelEstimated: BatteryLevel?

    public init(
        speed: Speed? = nil,
        operatingState: RideOperatingState = .unknown,
        voltage: Voltage? = nil,
        batteryCurrent: BatteryCurrent? = nil,
        motorCurrent: PhaseCurrent? = nil,
        power: Power? = nil,
        powerFlow: PowerFlowDirection? = nil,
        controllerTemperature: Temperature? = nil,
        motorTemperature: Temperature? = nil,
        batteryTemperature: Temperature? = nil,
        pwm: DutyCycle? = nil,
        distance: Distance? = nil,
        pitch: Angle? = nil,
        roll: Angle? = nil,
        batteryLevelReported: BatteryLevel? = nil,
        batteryLevelEstimated: BatteryLevel? = nil
    ) {
        self.speed = speed
        self.operatingState = operatingState
        self.voltage = voltage
        self.batteryCurrent = batteryCurrent
        self.motorCurrent = motorCurrent
        self.power = power
        self.powerFlow = powerFlow
        self.controllerTemperature = controllerTemperature
        self.motorTemperature = motorTemperature
        self.batteryTemperature = batteryTemperature
        self.pwm = pwm
        self.distance = distance
        self.pitch = pitch
        self.roll = roll
        self.batteryLevelReported = batteryLevelReported
        self.batteryLevelEstimated = batteryLevelEstimated
    }

    fileprivate init(_ dto: MobileTelemetrySnapshotDto) {
        self.init(
            speed: dto.speed?.value,
            operatingState: dto.operatingState,
            voltage: dto.voltage?.value,
            batteryCurrent: dto.batteryCurrent?.value,
            motorCurrent: dto.motorCurrent?.value,
            power: dto.power?.value,
            powerFlow: dto.powerFlow,
            controllerTemperature: dto.controllerTemperature?.value,
            motorTemperature: dto.motorTemperature?.value,
            batteryTemperature: dto.batteryTemperature?.value,
            pwm: dto.pwm,
            distance: dto.distance?.value,
            pitch: dto.pitch?.value,
            roll: dto.roll?.value,
            batteryLevelReported: dto.batteryLevelReported?.value,
            batteryLevelEstimated: dto.batteryLevelEstimated?.value
        )
    }
}

public enum ReadbackAvailability: Equatable, Hashable, Sendable {
    case available
    case unavailable
    case unsupported

    fileprivate init(_ dto: MobileReadbackAvailabilityDto) {
        switch dto {
        case .available:
            self = .available
        case .unavailable:
            self = .unavailable
        case .unsupported:
            self = .unsupported
        }
    }
}

public struct ReadbackValue<Value: Equatable & Hashable & Sendable>: Equatable, Hashable, Sendable {
    public let value: Value?
    public let availability: ReadbackAvailability

    public init(value: Value?, availability: ReadbackAvailability) {
        self.value = value
        self.availability = availability
    }

    public static func available(_ value: Value) -> Self {
        Self(value: value, availability: .available)
    }

    public static var unavailable: Self {
        Self(value: nil, availability: .unavailable)
    }

    public static var unsupported: Self {
        Self(value: nil, availability: .unsupported)
    }
}

public struct PedalHardness: Equatable, Hashable, Sendable {
    public let percent: UInt8

    public init(percent: UInt8) {
        self.percent = percent
    }
}

public struct EucPackHealthSnapshot: Equatable, Hashable, Sendable {
    public let energyPercent: BatteryLevel?
    public let voltage: Voltage?
    public let highGroupVoltage: Voltage?
    public let lowGroupVoltage: Voltage?
    public let cellDelta: VoltageDelta?

    public init(
        energyPercent: BatteryLevel? = nil,
        voltage: Voltage? = nil,
        highGroupVoltage: Voltage? = nil,
        lowGroupVoltage: Voltage? = nil,
        cellDelta: VoltageDelta? = nil
    ) {
        self.energyPercent = energyPercent
        self.voltage = voltage
        self.highGroupVoltage = highGroupVoltage
        self.lowGroupVoltage = lowGroupVoltage
        self.cellDelta = cellDelta
    }
}

public struct EucGarageSettingsSnapshot: Equatable, Hashable, Sendable {
    public let beepMargin: ReadbackValue<Speed>
    public let tiltback: ReadbackValue<Speed>
    public let pedalHardness: ReadbackValue<PedalHardness>

    public init(
        beepMargin: ReadbackValue<Speed> = .unavailable,
        tiltback: ReadbackValue<Speed> = .unavailable,
        pedalHardness: ReadbackValue<PedalHardness> = .unavailable
    ) {
        self.beepMargin = beepMargin
        self.tiltback = tiltback
        self.pedalHardness = pedalHardness
    }
}

public enum EucFaultHistoryState: Equatable, Hashable, Sendable {
    case none(sinceDistance: Distance?)
    case fault(code: FaultCode, sinceDistance: Distance?)
}

public struct EucGarageSnapshot: Equatable, Hashable, Sendable {
    public let pack: EucPackHealthSnapshot
    public let settings: EucGarageSettingsSnapshot
    public let faultHistory: EucFaultHistoryState

    public init(
        pack: EucPackHealthSnapshot,
        settings: EucGarageSettingsSnapshot,
        faultHistory: EucFaultHistoryState
    ) {
        self.pack = pack
        self.settings = settings
        self.faultHistory = faultHistory
    }
}

public struct SpeedReadout: Equatable, Hashable, Sendable {
    private static let milesPerHourPerMillimeterPerSecond = 0.002_236_936_292_054_4

    public let millimetersPerSecond: Int32?

    public init(snapshot: TelemetrySnapshot?) {
        self.init(millimetersPerSecond: snapshot?.speed?.value)
    }

    public init(millimetersPerSecond: Int32?) {
        self.millimetersPerSecond = millimetersPerSecond
    }

    public var displayValue: String {
        guard let millimetersPerSecond else {
            return "--"
        }
        let milesPerHour = Double(millimetersPerSecond) * Self.milesPerHourPerMillimeterPerSecond
        return String(format: "%.1f", milesPerHour)
    }

    public var displayUnit: String {
        "mph"
    }
}

public enum BmsTopologyConfidence: Equatable, Hashable, Sendable {
    case verified
    case inferred
    case unverified

    fileprivate init(_ dto: MobileBmsTopologyConfidenceDto) {
        switch dto {
        case .verified:
            self = .verified
        case .inferred:
            self = .inferred
        case .unverified:
            self = .unverified
        }
    }
}

public enum BmsAlertLevel: Equatable, Hashable, Sendable {
    case nominal
    case warning
    case critical
    case unknown

    fileprivate init(_ dto: MobileBmsAlertLevelDto) {
        switch dto {
        case .nominal:
            self = .nominal
        case .warning:
            self = .warning
        case .critical:
            self = .critical
        case .unknown:
            self = .unknown
        }
    }
}

public struct BmsTopology: Equatable, Hashable, Sendable {
    public let layoutLabel: String
    public let seriesGroupCount: Int?
    public let parallelCount: Int?
    public let packCount: Int
    public let bmsCount: Int
    public let confidence: BmsTopologyConfidence

    public init(
        layoutLabel: String,
        seriesGroupCount: Int?,
        parallelCount: Int?,
        packCount: Int,
        bmsCount: Int,
        confidence: BmsTopologyConfidence
    ) {
        self.layoutLabel = layoutLabel
        self.seriesGroupCount = seriesGroupCount
        self.parallelCount = parallelCount
        self.packCount = packCount
        self.bmsCount = bmsCount
        self.confidence = confidence
    }

    fileprivate init(_ dto: MobileBmsTopologyDto) {
        self.init(
            layoutLabel: dto.layoutLabel,
            seriesGroupCount: dto.seriesGroupCount.map(Int.init),
            parallelCount: dto.parallelCount.map(Int.init),
            packCount: Int(dto.packCount),
            bmsCount: Int(dto.bmsCount),
            confidence: BmsTopologyConfidence(dto.confidence)
        )
    }
}

public struct BmsGroupSnapshot: Equatable, Hashable, Sendable, Identifiable {
    public var id: Int { index }

    public let index: Int
    public let label: String?
    public let voltage: Voltage?
    public let temperature: Temperature?
    public let resistance: Resistance?
    public let isBalancing: Bool?
    public let alertLevel: BmsAlertLevel
    public let detail: String?

    public init(
        index: Int,
        label: String? = nil,
        voltage: Voltage? = nil,
        temperature: Temperature? = nil,
        resistance: Resistance? = nil,
        isBalancing: Bool? = nil,
        alertLevel: BmsAlertLevel = .nominal,
        detail: String? = nil
    ) {
        self.index = index
        self.label = label
        self.voltage = voltage
        self.temperature = temperature
        self.resistance = resistance
        self.isBalancing = isBalancing
        self.alertLevel = alertLevel
        self.detail = detail
    }

    fileprivate init(_ dto: MobileBmsGroupSnapshotDto) {
        self.init(
            index: Int(dto.index),
            label: dto.label,
            voltage: dto.voltage?.value,
            temperature: dto.temperature?.value,
            resistance: dto.resistance,
            isBalancing: dto.isBalancing,
            alertLevel: BmsAlertLevel(dto.alertLevel),
            detail: dto.detail
        )
    }
}

public struct BmsFault: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { code }

    public let code: String
    public let label: String
    public let level: BmsAlertLevel

    public init(code: String, label: String, level: BmsAlertLevel) {
        self.code = code
        self.label = label
        self.level = level
    }

    fileprivate init(_ dto: MobileBmsFaultDto) {
        self.init(code: dto.code, label: dto.label, level: BmsAlertLevel(dto.alertLevel))
    }
}

public struct BmsSnapshot: Equatable, Hashable, Sendable {
    public let availability: ReadbackAvailability
    public let topology: BmsTopology
    public let energyPercent: BatteryLevel?
    public let voltage: Voltage?
    public let current: BatteryCurrent?
    public let cellDelta: VoltageDelta?
    public let lowestGroupIndex: Int?
    public let highestTemperature: Temperature?
    public let highestTemperatureLabel: String?
    public let balancingSummary: String?
    public let balancingDetail: String?
    public let faultSummary: String?
    public let faultDetail: String?
    public let groups: [BmsGroupSnapshot]
    public let faults: [BmsFault]
    public let captureActionTitle: String?
    public let captureActionState: String?

    public init(
        availability: ReadbackAvailability = .available,
        topology: BmsTopology,
        energyPercent: BatteryLevel? = nil,
        voltage: Voltage? = nil,
        current: BatteryCurrent? = nil,
        cellDelta: VoltageDelta? = nil,
        lowestGroupIndex: Int? = nil,
        highestTemperature: Temperature? = nil,
        highestTemperatureLabel: String? = nil,
        balancingSummary: String? = nil,
        balancingDetail: String? = nil,
        faultSummary: String? = nil,
        faultDetail: String? = nil,
        groups: [BmsGroupSnapshot] = [],
        faults: [BmsFault] = [],
        captureActionTitle: String? = nil,
        captureActionState: String? = nil
    ) {
        self.availability = availability
        self.topology = topology
        self.energyPercent = energyPercent
        self.voltage = voltage
        self.current = current
        self.cellDelta = cellDelta
        self.lowestGroupIndex = lowestGroupIndex
        self.highestTemperature = highestTemperature
        self.highestTemperatureLabel = highestTemperatureLabel
        self.balancingSummary = balancingSummary
        self.balancingDetail = balancingDetail
        self.faultSummary = faultSummary
        self.faultDetail = faultDetail
        self.groups = groups
        self.faults = faults
        self.captureActionTitle = captureActionTitle
        self.captureActionState = captureActionState
    }

    fileprivate init(_ dto: MobileBmsSnapshotDto) {
        self.init(
            availability: ReadbackAvailability(dto.availability),
            topology: BmsTopology(dto.topology),
            energyPercent: dto.energyPercent?.value,
            voltage: dto.voltage?.value,
            current: dto.current?.value,
            cellDelta: dto.cellDelta?.value,
            lowestGroupIndex: dto.lowestGroupIndex.map(Int.init),
            highestTemperature: dto.highestTemperature?.value,
            highestTemperatureLabel: dto.highestTemperatureLabel,
            balancingSummary: dto.balancingSummary,
            balancingDetail: dto.balancingDetail,
            faultSummary: dto.faultSummary,
            faultDetail: dto.faultDetail,
            groups: dto.groups.map(BmsGroupSnapshot.init),
            faults: dto.faults.map(BmsFault.init),
            captureActionTitle: dto.captureActionTitle,
            captureActionState: dto.captureActionState
        )
    }
}

public struct SessionDebugRow: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

public struct RideDisplayState: Equatable, Hashable, Sendable {
    public let speed: SpeedReadout
    public let telemetry: TelemetrySnapshot?
    public let notificationCount: UInt64
    public let lastUpdate: MonotonicMilliseconds?

    public init(
        speed: SpeedReadout = SpeedReadout(millimetersPerSecond: nil),
        telemetry: TelemetrySnapshot? = nil,
        notificationCount: UInt64 = 0,
        lastUpdate: MonotonicMilliseconds? = nil
    ) {
        self.speed = speed
        self.telemetry = telemetry
        self.notificationCount = notificationCount
        self.lastUpdate = lastUpdate
    }

    public var debugRows: [SessionDebugRow] {
        [
            SessionDebugRow(label: "Notifications", value: "\(notificationCount)"),
            SessionDebugRow(label: "Last update", value: lastUpdateText),
        ]
    }

    public func reducing(
        _ step: CoreBluetoothSessionStep,
        receivedAt: MonotonicMilliseconds
    ) -> RideDisplayState {
        let nextSpeed =
            step.snapshot?.speed.map { SpeedReadout(millimetersPerSecond: $0.value) } ?? speed
        return RideDisplayState(
            speed: nextSpeed,
            telemetry: step.snapshot ?? telemetry,
            notificationCount: notificationCount + 1,
            lastUpdate: receivedAt
        )
    }

    private var lastUpdateText: String {
        lastUpdate.map { "\($0.rawValue) ms" } ?? "never"
    }
}

public struct EucRideScreenState: Equatable, Hashable, Sendable {
    public let phase: SessionConnectionPhase
    public let displayState: RideDisplayState

    public init(phase: SessionConnectionPhase, displayState: RideDisplayState) {
        self.phase = phase
        self.displayState = displayState
    }

    public var telemetry: TelemetrySnapshot? {
        displayState.telemetry
    }

    public var operatingState: RideOperatingState {
        telemetry?.operatingState ?? .unknown
    }

    public var phaseText: String {
        phase.displayText
    }

    public var speedText: String {
        displayState.speed.displayValue
    }

    public var speedUnit: String {
        displayState.speed.displayUnit
    }
}

public enum SessionConnectionFailure: Equatable, Hashable, Sendable {
    case missingNotifyChannel
    case sessionFailed(String)
    case connectFailed(String)
    case serviceDiscoveryFailed(String)
    case characteristicDiscoveryFailed(String)
    case notificationFailed(String)
    case notificationIngestFailed(String)
    case skippedReadOnlyWrite

    public var displayText: String {
        switch self {
        case .missingNotifyChannel:
            "Missing notify channel"
        case .sessionFailed(let message):
            "Session failed: \(message)"
        case .connectFailed(let message):
            "Connect failed: \(message)"
        case .serviceDiscoveryFailed(let message):
            "Service discovery failed: \(message)"
        case .characteristicDiscoveryFailed(let message):
            "Characteristic discovery failed: \(message)"
        case .notificationFailed(let message):
            "Notification failed: \(message)"
        case .notificationIngestFailed(let message):
            "Notification ingest failed: \(message)"
        case .skippedReadOnlyWrite:
            "Read-only MVP skipped a write operation"
        }
    }
}

public enum SessionConnectionPhase: Equatable, Hashable, Sendable {
    case starting
    case bluetoothUnavailable(rawState: Int)
    case scanning(model: ElectricUnicycleModel)
    case connecting(model: ElectricUnicycleModel)
    case discoveringServices
    case subscribing
    case live
    case failed(SessionConnectionFailure)

    public var displayText: String {
        switch self {
        case .starting:
            "Starting Bluetooth..."
        case .bluetoothUnavailable(let rawState):
            "Bluetooth unavailable: state \(rawState)"
        case .scanning(let model):
            "Scanning for \(model.displayName)..."
        case .connecting(let model):
            "Connecting to \(model.displayName)..."
        case .discoveringServices:
            "Discovering services..."
        case .subscribing:
            "Subscribing..."
        case .live:
            "Live"
        case .failed(let failure):
            failure.displayText
        }
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

public enum ElectricUnicycleModel: Equatable, Hashable, Sendable {
    case aero
    case falcon
}

public extension ElectricUnicycleModel {
    init(_ dto: MobileElectricUnicycleModelDto) {
        switch dto {
        case .aero:
            self = .aero
        case .falcon:
            self = .falcon
        }
    }

    var displayName: String {
        switch self {
        case .aero:
            "Aero"
        case .falcon:
            "Falcon"
        }
    }
}

public final class ElectricUnicycleSession: @unchecked Sendable {
    private enum Inner {
        case aero(AeroReadOnlySession)
        case falcon(FalconReadOnlySession)
    }

    public let model: ElectricUnicycleModel
    private let inner: Inner

    public init(model: ElectricUnicycleModel) throws {
        self.model = model
        self.inner = switch model {
        case .aero:
            .aero(AeroReadOnlySession())
        case .falcon:
            .falcon(try FalconReadOnlySession())
        }
    }

    public var diagnostics: ParserDiagnostics {
        switch inner {
        case .aero(let session):
            ParserDiagnostics(session.diagnostics())
        case .falcon(let session):
            ParserDiagnostics(session.diagnostics())
        }
    }

    public var currentSnapshot: TelemetrySnapshot {
        switch inner {
        case .aero(let session):
            TelemetrySnapshot(session.currentSnapshot())
        case .falcon(let session):
            TelemetrySnapshot(session.currentSnapshot())
        }
    }

    public func linkUp(
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes
    ) throws -> [SessionAction] {
        try step(.linkUp, at: monotonicMilliseconds, writeLimit: writeLimit)
    }

    public func ingestNotification(
        _ bytes: Data,
        channel: Data,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> TelemetrySnapshot {
        _ = try ingestNotificationActions(bytes, channel: channel, at: monotonicMilliseconds)
        return currentSnapshot
    }

    public func ingestNotificationActions(
        _ bytes: Data,
        channel: Data,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> [SessionAction] {
        try step(.notification, at: monotonicMilliseconds, channel: channel, bytes: bytes)
    }

    public func perform(
        _ command: DeviceCommand,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> [SessionAction] {
        try step(.command, at: monotonicMilliseconds, command: command)
    }

    private func step(
        _ kind: MobileSessionInputKindDto,
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes? = nil,
        channel: Data = Data(),
        bytes: Data = Data(),
        command: DeviceCommand? = nil
    ) throws -> [SessionAction] {
        switch inner {
        case .aero(let session):
            try session.step(
                kind,
                at: monotonicMilliseconds,
                writeLimit: writeLimit,
                channel: channel,
                bytes: bytes,
                command: command
            )
        case .falcon(let session):
            try session.step(
                kind,
                at: monotonicMilliseconds,
                writeLimit: writeLimit,
                channel: channel,
                bytes: bytes,
                command: command
            )
        }
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
        switch action.kind {
        case .subscribe:
            guard let channel = BluetoothUuid(action.channel) else {
                return []
            }
            return [.subscribe(channel: channel)]
        case .write:
            guard let channel = BluetoothUuid(action.channel) else {
                return []
            }
            return chunked(action.bytes, by: Int(writeLimit.rawValue)).map {
                .writeWithoutResponse(channel: channel, bytes: $0)
            }
        case .disconnect:
            return [.disconnect]
        case .event, .notificationIngest, .settingsReadback, .faultHistoryReadback, .bmsSnapshot:
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
    case electricUnicycle(ElectricUnicycleSession)

    public static func electricUnicycle(model: ElectricUnicycleModel) throws -> CoreBluetoothSession {
        try .electricUnicycle(ElectricUnicycleSession(model: model))
    }

    fileprivate var currentSnapshot: TelemetrySnapshot {
        switch self {
        case .electricUnicycle(let session):
            session.currentSnapshot
        }
    }

    fileprivate func linkUp(
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes
    ) throws -> [SessionAction] {
        switch self {
        case .electricUnicycle(let session):
            try session.linkUp(at: monotonicMilliseconds, writeLimit: writeLimit)
        }
    }

    fileprivate func ingestNotification(
        _ bytes: Data,
        channel: BluetoothUuid,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> [SessionAction] {
        switch self {
        case .electricUnicycle(let session):
            try session.ingestNotificationActions(bytes, channel: channel.bytes, at: monotonicMilliseconds)
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
    public let actions: [SessionAction]
    public let captureContext: CoreBluetoothCaptureContext?

    public init(
        operations: [CoreBluetoothPlannedOperation],
        snapshot: TelemetrySnapshot?,
        actions: [SessionAction] = [],
        captureContext: CoreBluetoothCaptureContext? = nil
    ) {
        self.operations = operations
        self.snapshot = snapshot
        self.actions = actions
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
            let actions = try session.ingestNotification(
                bytes,
                channel: channel,
                at: monotonicMilliseconds
            )
            return CoreBluetoothSessionStep(
                operations: actions.flatMap(planner.plan(action:)),
                snapshot: session.currentSnapshot,
                actions: actions,
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

    var bluetooth16Value: UInt16? {
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
