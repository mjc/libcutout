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

public enum DeviceDetectionProtocolFamily: Equatable, Hashable, Sendable {
    case veteranLeaperkimNosfet
    case begodeGotway
    case vesc

    fileprivate init(_ dto: MobileProtocolFamilyDto) {
        switch dto {
        case .veteranLeaperkimNosfet:
            self = .veteranLeaperkimNosfet
        case .begodeGotway:
            self = .begodeGotway
        case .vesc:
            self = .vesc
        }
    }

    fileprivate var dto: MobileProtocolFamilyDto {
        switch self {
        case .veteranLeaperkimNosfet:
            .veteranLeaperkimNosfet
        case .begodeGotway:
            .begodeGotway
        case .vesc:
            .vesc
        }
    }
}

public enum DeviceDetectionPendingProbe: Equatable, Hashable, Sendable {
    case begodeName
    case begodeFirmware
    case begodeImu

    fileprivate init(_ dto: MobilePendingProbeDto) {
        switch dto {
        case .begodeName:
            self = .begodeName
        case .begodeFirmware:
            self = .begodeFirmware
        case .begodeImu:
            self = .begodeImu
        }
    }

    fileprivate var dto: MobilePendingProbeDto {
        switch self {
        case .begodeName:
            .begodeName
        case .begodeFirmware:
            .begodeFirmware
        case .begodeImu:
            .begodeImu
        }
    }
}

public struct DeviceDetectionResolution: Equatable, Hashable, Sendable {
    public let protocolFamily: DeviceDetectionProtocolFamily?
    public let protocolConflict: Bool
    public let veteranProtocolModelID: UInt16?
    public let advertisedName: Data?
    public let modelBanner: Data?
    public let firmwareBanner: Data?
    public let imuBanner: Data?
    public let missingProbeResponse: DeviceDetectionPendingProbe?
    public let malformedProbeResponse: DeviceDetectionPendingProbe?

    fileprivate init(_ record: DeviceDetectionResolutionRecord) {
        self.protocolFamily = record.protocolFamily.map(DeviceDetectionProtocolFamily.init)
        self.protocolConflict = record.protocolConflict
        self.veteranProtocolModelID = record.veteranProtocolModelId
        self.advertisedName = record.advertisedName
        self.modelBanner = record.modelBanner
        self.firmwareBanner = record.firmwareBanner
        self.imuBanner = record.imuBanner
        self.missingProbeResponse = record.missingProbeResponse.map(DeviceDetectionPendingProbe.init)
        self.malformedProbeResponse = record.malformedProbeResponse.map(DeviceDetectionPendingProbe.init)
    }
}

public extension DeviceDetectionResolution {
    func discoveryCandidate(
        platformIdentifier: String,
        displayName: String
    ) -> DiscoveryCandidate {
        mobileDiscoveryCandidateFromDetectionResolution(
            platformIdentifier: platformIdentifier,
            displayName: displayName,
            resolution: DeviceDetectionResolutionRecord(
                protocolFamily: protocolFamily.map(\.dto),
                protocolConflict: protocolConflict,
                veteranProtocolModelId: veteranProtocolModelID,
                advertisedName: advertisedName,
                modelBanner: modelBanner,
                firmwareBanner: firmwareBanner,
                imuBanner: imuBanner,
                missingProbeResponse: missingProbeResponse.map(\.dto),
                malformedProbeResponse: malformedProbeResponse.map(\.dto)
            )
        )
    }
}

public enum DeviceDetectionGattRole: Equatable, Hashable, Sendable {
    case read
    case write
    case writeWithoutResponse
    case notify
    case indicate

    fileprivate var dto: MobileGattRoleDto {
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

public struct DeviceDetectionGattFingerprint: Equatable, Hashable, Sendable {
    public let service: Data
    public let characteristic: Data
    public let roles: [DeviceDetectionGattRole]
    public let verification: VerificationState

    public init(
        service: Data,
        characteristic: Data,
        roles: [DeviceDetectionGattRole],
        verification: VerificationState
    ) {
        self.service = service
        self.characteristic = characteristic
        self.roles = roles
        self.verification = verification
    }

    fileprivate var dto: MobileGattFingerprintDto {
        MobileGattFingerprintDto(
            service: service,
            characteristic: characteristic,
            roles: roles.map(\.dto),
            verification: verification.dto
        )
    }
}

public final class DeviceDetectionSession {
    private let inner: DeviceDetectionSessionHandle

    public init() {
        self.inner = DeviceDetectionSessionHandle()
    }

    public func observeAdvertisement(name: Data?) -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeAdvertisement(name: name))
    }

    public func observeGatt(fingerprints: [DeviceDetectionGattFingerprint]) -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeGatt(fingerprints: fingerprints.map(\.dto)))
    }

    public func observeNotification(bytes: Data) -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeNotification(bytes: bytes))
    }

    public func observeBegodeNameProbe() -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeBegodeNameProbe())
    }

    public func observeBegodeFirmwareProbe() -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeBegodeFirmwareProbe())
    }

    public func observeBegodeImuProbe() -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeBegodeImuProbe())
    }

    public func observeBegodeNameProbeTimeout() -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeBegodeNameProbeTimeout())
    }

    public func observeBegodeFirmwareProbeTimeout() -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeBegodeFirmwareProbeTimeout())
    }

    public func observeBegodeImuProbeTimeout() -> DeviceDetectionResolution {
        DeviceDetectionResolution(inner.observeBegodeImuProbeTimeout())
    }

    public var resolution: DeviceDetectionResolution {
        DeviceDetectionResolution(inner.resolution())
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
    public let rawTelemetry: RawTelemetryReadback?
    public let veteranProtocolModelId: UInt16?

    private init(
        kind: SessionActionKind,
        channel: Data,
        bytes: Data,
        settingsReadback: SettingsReadback? = nil,
        faultHistoryReadback: FaultHistoryReadback? = nil,
        bmsSnapshot: BmsSnapshot? = nil,
        rawTelemetry: RawTelemetryReadback? = nil,
        veteranProtocolModelId: UInt16? = nil
    ) {
        self.kind = kind
        self.channel = channel
        self.bytes = bytes
        self.settingsReadback = settingsReadback
        self.faultHistoryReadback = faultHistoryReadback
        self.bmsSnapshot = bmsSnapshot
        self.rawTelemetry = rawTelemetry
        self.veteranProtocolModelId = veteranProtocolModelId
    }

    public static func subscribe(channel: Data) -> Self {
        Self(kind: .subscribe, channel: channel, bytes: Data())
    }

    public static func write(channel: Data, bytes: Data) -> Self {
        Self(kind: .write, channel: channel, bytes: bytes)
    }

    public static func event(channel: Data = Data(), bytes: Data = Data()) -> Self {
        Self(kind: .event, channel: channel, bytes: bytes)
    }

    public static func protocolIdentity(veteranModelId: UInt16) -> Self {
        Self(
            kind: .event,
            channel: Data(),
            bytes: Data(),
            veteranProtocolModelId: veteranModelId
        )
    }

    public static func disconnect() -> Self {
        Self(kind: .disconnect, channel: Data(), bytes: Data())
    }

    public static func notificationIngest() -> Self {
        Self(kind: .notificationIngest, channel: Data(), bytes: Data())
    }

    public static func withSettingsReadback(_ readback: SettingsReadback) -> Self {
        Self(
            kind: .settingsReadback,
            channel: Data(),
            bytes: Data(),
            settingsReadback: readback
        )
    }

    public static func withFaultHistoryReadback(_ readback: FaultHistoryReadback) -> Self {
        Self(
            kind: .faultHistoryReadback,
            channel: Data(),
            bytes: Data(),
            faultHistoryReadback: readback
        )
    }

    public static func withBmsSnapshot(_ snapshot: BmsSnapshot) -> Self {
        Self(
            kind: .bmsSnapshot,
            channel: Data(),
            bytes: Data(),
            bmsSnapshot: snapshot
        )
    }

    fileprivate init(_ dto: MobileSessionOutputDto) {
        self.kind = SessionActionKind(dto.kind)
        self.channel = dto.channel
        self.bytes = dto.bytes
        self.settingsReadback = dto.settingsReadback.map(SettingsReadback.init)
        self.faultHistoryReadback = dto.faultHistoryReadback.map(FaultHistoryReadback.init)
        self.bmsSnapshot = dto.bmsSnapshot.map(BmsSnapshot.init)
        self.rawTelemetry = dto.rawTelemetry.map(RawTelemetryReadback.init)
        self.veteranProtocolModelId = dto.veteranProtocolModelId
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

public struct RawFloatField: Equatable, Hashable, Sendable {
    public let id: UInt16
    public let valueBits: UInt32

    fileprivate init(_ dto: MobileRawFloatFieldValueDto) {
        id = dto.id
        valueBits = dto.valueBits
    }
}

public struct RawTelemetryReadback: Equatable, Hashable, Sendable {
    public let fields: [RawSettingField]
    public let floatFields: [RawFloatField]

    fileprivate init(_ dto: MobileRawTelemetryReadbackDto) {
        fields = dto.fields.map(RawSettingField.init)
        floatFields = dto.floatFields.map(RawFloatField.init)
    }

    var dto: MobileRawTelemetryReadbackDto {
        MobileRawTelemetryReadbackDto(
            fields: fields.map { MobileRawFieldValueDto(id: $0.id, value: $0.value) },
            floatFields: floatFields.map {
                MobileRawFloatFieldValueDto(id: $0.id, valueBits: $0.valueBits)
            }
        )
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

    fileprivate var dto: MobileVerificationStatusDto {
        switch self {
        case .unverified:
            .unverified
        case .inferred:
            .inferred
        case .sourceVerified:
            .sourceVerified
        case .hardwareVerified:
            .hardwareVerified
        case .sourceAndHardwareVerified:
            .sourceAndHardwareVerified
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
    public let eucGarageSettings: EucGarageSettingsSnapshot

    public init(
        entries: [SettingsReadbackEntry],
        availability: ReadbackAvailability = .available,
        eucGarageSettings: EucGarageSettingsSnapshot? = nil
    ) {
        self.availability = availability
        self.entries = availability == .available ? entries : []
        self.eucGarageSettings = switch availability {
        case .available:
            eucGarageSettings ?? Self.missingGarageSettings(for: availability)
        case .unavailable, .unsupported:
            Self.missingGarageSettings(for: availability)
        }
    }

    fileprivate init(_ dto: MobileSettingsReadbackDto) {
        self.init(
            entries: dto.entries.map(SettingsReadbackEntry.init),
            availability: ReadbackAvailability(dto.availability),
            eucGarageSettings: EucGarageSettingsSnapshot(dto.eucGarage)
        )
    }

    private static func missingGarageSettings(
        for availability: ReadbackAvailability
    ) -> EucGarageSettingsSnapshot {
        EucGarageSettingsSnapshot(
            beepMargin: Self.missingReadback(for: availability),
            tiltback: Self.missingReadback(for: availability),
            pedalMode: Self.missingReadback(for: availability)
        )
    }

    private static func missingReadback<Value>(
        for availability: ReadbackAvailability
    ) -> ReadbackValue<Value> {
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

    private init(
        availability: ReadbackAvailability,
        lastFault: FaultHistoryEntry?,
        sinceDistance: Distance?
    ) {
        self.availability = availability
        self.lastFault = lastFault
        self.sinceDistance = sinceDistance
    }

    public static func unavailable() -> Self {
        Self(availability: .unavailable, lastFault: nil, sinceDistance: nil)
    }

    public static func unsupported() -> Self {
        Self(availability: .unsupported, lastFault: nil, sinceDistance: nil)
    }

    public static func noFaultSince(_ sinceDistance: Distance) -> Self {
        Self(availability: .available, lastFault: nil, sinceDistance: sinceDistance)
    }

    public static func faultSince(
        _ lastFault: FaultHistoryEntry,
        sinceDistance: Distance? = nil
    ) -> Self {
        Self(
            availability: .available,
            lastFault: lastFault,
            sinceDistance: sinceDistance
        )
    }

    init(_ dto: MobileFaultHistoryReadbackDto) {
        let availability = ReadbackAvailability(dto.availability)
        let lastFault = dto.lastFault.map(FaultHistoryEntry.init)
        let sinceDistance = dto.sinceDistance?.value

        switch availability {
        case .available where lastFault == nil && sinceDistance == nil:
            self = .unavailable()
        case .available:
            self.init(
                availability: availability,
                lastFault: lastFault,
                sinceDistance: sinceDistance
            )
        case .unavailable:
            self = .unavailable()
        case .unsupported:
            self = .unsupported()
        }
    }
}

public enum DeviceCommand: Equatable, Hashable, Sendable {
    case requestIdentity
    case requestTelemetry
    case requestFirmwareInfo
    case requestBatteryInfo
    case requestDiagnostics
    case requestFaultHistory
    case requestSettings
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
        case .requestFaultHistory:
            self = .requestFaultHistory
        case .requestSettings:
            self = .requestSettings
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
        case .requestFaultHistory:
            .requestFaultHistory
        case .requestSettings:
            .requestSettings
        case .soundHorn:
            .soundHorn
        }
    }
}

public struct TelemetrySnapshot: Equatable, Hashable, Sendable {
    public let at: MonotonicMilliseconds?
    public let speed: Speed?
    public let speedSource: ReadbackSource?
    public let speedQuality: ReadbackQuality?
    public let operatingState: RideOperatingState
    public let voltage: Voltage?
    public let batteryCurrent: BatteryCurrent?
    public let motorCurrent: PhaseCurrent?
    public let power: Power?
    public let powerFlow: PowerFlowDirection?
    public let voltageSag: VoltageDelta?
    public let controllerTemperature: Temperature?
    public let motorTemperature: Temperature?
    public let batteryTemperature: Temperature?
    public let pwm: DutyCycle?
    public let distance: Distance?
    public let limpHomeRange: Distance?
    public let pitch: Angle?
    public let balanceAngle: Angle?
    public let roll: Angle?
    public let footpad: FootpadTelemetry?
    public let batteryLevelReported: BatteryLevel?
    public let batteryLevelEstimated: BatteryLevel?

    public init(
        at: MonotonicMilliseconds? = nil,
        speed: Speed? = nil,
        speedSource: ReadbackSource? = nil,
        speedQuality: ReadbackQuality? = nil,
        operatingState: RideOperatingState = .unknown,
        voltage: Voltage? = nil,
        batteryCurrent: BatteryCurrent? = nil,
        motorCurrent: PhaseCurrent? = nil,
        power: Power? = nil,
        powerFlow: PowerFlowDirection? = nil,
        voltageSag: VoltageDelta? = nil,
        controllerTemperature: Temperature? = nil,
        motorTemperature: Temperature? = nil,
        batteryTemperature: Temperature? = nil,
        pwm: DutyCycle? = nil,
        distance: Distance? = nil,
        limpHomeRange: Distance? = nil,
        pitch: Angle? = nil,
        balanceAngle: Angle? = nil,
        roll: Angle? = nil,
        footpad: FootpadTelemetry? = nil,
        batteryLevelReported: BatteryLevel? = nil,
        batteryLevelEstimated: BatteryLevel? = nil
    ) {
        self.at = at
        self.speed = speed
        self.speedSource = speedSource
        self.speedQuality = speedQuality
        self.operatingState = operatingState
        self.voltage = voltage
        self.batteryCurrent = batteryCurrent
        self.motorCurrent = motorCurrent
        self.power = power
        self.powerFlow = powerFlow
        self.voltageSag = voltageSag
        self.controllerTemperature = controllerTemperature
        self.motorTemperature = motorTemperature
        self.batteryTemperature = batteryTemperature
        self.pwm = pwm
        self.distance = distance
        self.limpHomeRange = limpHomeRange
        self.pitch = pitch
        self.balanceAngle = balanceAngle
        self.roll = roll
        self.footpad = footpad
        self.batteryLevelReported = batteryLevelReported
        self.batteryLevelEstimated = batteryLevelEstimated
    }

    fileprivate init(_ dto: MobileTelemetrySnapshotDto) {
        self.init(
            at: dto.atMs.map { MonotonicMilliseconds($0.milliseconds) },
            speed: dto.speed?.value,
            speedSource: dto.speed.map { ReadbackSource($0.source) },
            speedQuality: dto.speed.map { ReadbackQuality($0.quality) },
            operatingState: dto.operatingState,
            voltage: dto.voltage?.value,
            batteryCurrent: dto.batteryCurrent?.value,
            motorCurrent: dto.motorCurrent?.value,
            power: dto.power?.value,
            powerFlow: dto.powerFlow,
            voltageSag: dto.voltageSag?.value,
            controllerTemperature: dto.controllerTemperature?.value,
            motorTemperature: dto.motorTemperature?.value,
            batteryTemperature: dto.batteryTemperature?.value,
            pwm: dto.pwm,
            distance: dto.distance?.value,
            limpHomeRange: dto.limpHomeRange?.value,
            pitch: dto.pitch?.value,
            balanceAngle: dto.balanceAngle?.value,
            roll: dto.roll?.value,
            footpad: dto.footpad.map(FootpadTelemetry.init),
            batteryLevelReported: dto.batteryLevelReported?.value,
            batteryLevelEstimated: dto.batteryLevelEstimated?.value
        )
    }
}

public struct FootpadTelemetry: Equatable, Hashable, Sendable {
    public let state: UInt8
    public let adc1Milliunits: Int32?
    public let adc2Milliunits: Int32?

    public init(state: UInt8, adc1Milliunits: Int32? = nil, adc2Milliunits: Int32? = nil) {
        self.state = state
        self.adc1Milliunits = adc1Milliunits
        self.adc2Milliunits = adc2Milliunits
    }

    fileprivate init(_ dto: MobileFootpadTelemetryDto) {
        self.init(
            state: dto.state,
            adc1Milliunits: dto.adc1Milliunits,
            adc2Milliunits: dto.adc2Milliunits
        )
    }
}

public extension FootpadTelemetry {
    var adc1DisplayText: String {
        formatFootpadReading(adc1Milliunits)
    }

    var adc2DisplayText: String {
        formatFootpadReading(adc2Milliunits)
    }

    var stateDisplayText: String {
        "state \(state)"
    }
}

private func formatFootpadReading(_ value: Int32?) -> String {
    guard let value else {
        return "--"
    }
    return String(format: "%.2f", Double(value) / 1_000)
}

public enum VescControllerState: Equatable, Hashable, Sendable {
    case armed
    case disarmed
    case unknown

    fileprivate init(_ dto: MobileVescControllerStateDto) {
        switch dto {
        case .armed:
            self = .armed
        case .disarmed:
            self = .disarmed
        case .unknown:
            self = .unknown
        }
    }
}

public enum VescRideWarning: Equatable, Hashable, Sendable {
    case none
    case pushbackSoon
    case unknown

    fileprivate init(_ dto: MobileVescRideWarningDto) {
        switch dto {
        case .none:
            self = .none
        case .pushbackSoon:
            self = .pushbackSoon
        case .unknown:
            self = .unknown
        }
    }
}

public enum VescVehicleKind: Equatable, Hashable, Sendable {
    case float
    case bike
    case skateboard
    case electricUnicycle
    case unknown

    public var displayName: String {
        self == .unknown ? "VESC" : "VESC \(shortDisplayName)"
    }

    public var shortDisplayName: String {
        switch self {
        case .float:
            "Float"
        case .bike:
            "Bike"
        case .skateboard:
            "Skateboard"
        case .electricUnicycle:
            "EUC"
        case .unknown:
            "VESC"
        }
    }

    fileprivate init(_ dto: MobileVescVehicleKindDto) {
        switch dto {
        case .float:
            self = .float
        case .bike:
            self = .bike
        case .skateboard:
            self = .skateboard
        case .electricUnicycle:
            self = .electricUnicycle
        case .unknown:
            self = .unknown
        }
    }
}

public enum VescSubProtocol: Equatable, Hashable, Sendable {
    case refloat
    case bike
    case eskate
    case generic

    public var displayName: String {
        switch self {
        case .refloat:
            "Refloat"
        case .bike:
            "Bike"
        case .eskate:
            "eSkate"
        case .generic:
            "VESC"
        }
    }

    fileprivate init(_ dto: MobileVescSubProtocolDto) {
        switch dto {
        case .refloat:
            self = .refloat
        case .bike:
            self = .bike
        case .eskate:
            self = .eskate
        case .generic:
            self = .generic
        }
    }
}

public struct VescRideSnapshot: Equatable, Hashable, Sendable {
    public static let defaultTitle = "VESC"

    public let title: String
    public let vehicleKind: VescVehicleKind
    public let subProtocol: VescSubProtocol
    public let controllerState: VescControllerState
    public let operatingState: RideOperatingState
    public let warning: VescRideWarning
    public let boardSpeed: Speed?
    public let dutyCycle: DutyCycle?
    public let dutyHeadroom: BatteryLevel?
    public let dutyHeadroomApplicability: EucRideMetricApplicability
    public let batteryVoltage: Voltage?
    public let batteryLevelReported: BatteryLevel?
    public let batteryLevelEstimated: BatteryLevel?
    public let batteryCurrent: BatteryCurrent?
    public let powerFlow: PowerFlowDirection?
    public let motorCurrent: PhaseCurrent?
    public let boardAngle: Angle?
    public let balanceAngle: Angle?
    public let controllerTemperature: Temperature?
    public let motorTemperature: Temperature?
    public let footpad: FootpadTelemetry?
    public let lastUpdate: MonotonicMilliseconds?

    public init(
        title: String,
        vehicleKind: VescVehicleKind,
        subProtocol: VescSubProtocol,
        controllerState: VescControllerState,
        operatingState: RideOperatingState = .unknown,
        warning: VescRideWarning = .unknown,
        boardSpeed: Speed? = nil,
        dutyCycle: DutyCycle? = nil,
        dutyHeadroom: BatteryLevel? = nil,
        dutyHeadroomApplicability: EucRideMetricApplicability = .unavailable,
        batteryVoltage: Voltage? = nil,
        batteryLevelReported: BatteryLevel? = nil,
        batteryLevelEstimated: BatteryLevel? = nil,
        batteryCurrent: BatteryCurrent? = nil,
        powerFlow: PowerFlowDirection? = nil,
        motorCurrent: PhaseCurrent? = nil,
        boardAngle: Angle? = nil,
        balanceAngle: Angle? = nil,
        controllerTemperature: Temperature? = nil,
        motorTemperature: Temperature? = nil,
        footpad: FootpadTelemetry? = nil,
        lastUpdate: MonotonicMilliseconds? = nil
    ) {
        self.title = title
        self.vehicleKind = vehicleKind
        self.subProtocol = subProtocol
        self.controllerState = controllerState
        self.operatingState = operatingState
        self.warning = warning
        self.boardSpeed = boardSpeed
        self.dutyCycle = dutyCycle
        self.dutyHeadroom = dutyHeadroom
        self.dutyHeadroomApplicability = dutyHeadroom == nil ? dutyHeadroomApplicability : .available
        self.batteryVoltage = batteryVoltage
        self.batteryLevelReported = batteryLevelReported
        self.batteryLevelEstimated = batteryLevelEstimated
        self.batteryCurrent = batteryCurrent
        self.powerFlow = powerFlow
        self.motorCurrent = motorCurrent
        self.boardAngle = boardAngle
        self.balanceAngle = balanceAngle
        self.controllerTemperature = controllerTemperature
        self.motorTemperature = motorTemperature
        self.footpad = footpad
        self.lastUpdate = lastUpdate
    }

    public init?(displayState: RideDisplayState, title: String?) {
        guard let telemetry = displayState.telemetry, telemetry.hasVisibleRideValues else {
            return nil
        }
        self.init(
            title: title ?? Self.defaultTitle,
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown,
            operatingState: telemetry.operatingState,
            warning: .unknown,
            boardSpeed: telemetry.speed,
            dutyCycle: telemetry.pwm,
            dutyHeadroom: telemetry.dutyHeadroom,
            dutyHeadroomApplicability: telemetry.pwmHeadroomApplicability,
            batteryVoltage: telemetry.voltage,
            batteryLevelReported: telemetry.batteryLevelReported,
            batteryLevelEstimated: telemetry.batteryLevelEstimated,
            batteryCurrent: telemetry.batteryCurrent,
            powerFlow: telemetry.powerFlow,
            motorCurrent: telemetry.motorCurrent,
            boardAngle: telemetry.pitch,
            balanceAngle: telemetry.balanceAngle,
            controllerTemperature: telemetry.controllerTemperature,
            motorTemperature: telemetry.motorTemperature,
            footpad: telemetry.footpad,
            lastUpdate: telemetry.at ?? displayState.lastUpdate
        )
    }

    public var screenSubtitle: String {
        switch operatingState {
        case .parked:
            "Parked"
        case .standing:
            "Standing"
        case .riding:
            "Riding"
        case .charging:
            "Charging"
        case .unknown:
            vehicleKind.shortDisplayName
        }
    }

    public var displayedDutyHeadroom: BatteryLevel? {
        switch dutyHeadroomApplicability {
        case .available:
            dutyHeadroom
        case .notApplicable:
            BatteryLevel(value: 100)
        case .unavailable:
            nil
        }
    }
}

public extension VescRideSnapshot {
    func updateAge(
        at now: MonotonicMilliseconds,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> EucRideUpdateAge {
        rideUpdateAge(updatedAt: lastUpdate, at: now, staleAfter: staleThreshold)
    }
}

private let dutyHeadroomIdleDeadbandPermille = 20

private func dutyHeadroomPermille(from dutyCycle: DutyCycle) -> Int {
    let rawUsedPermille = min(1_000, abs(Int(dutyCycle.permille)))
    let usedPermille = rawUsedPermille <= dutyHeadroomIdleDeadbandPermille ? 0 : rawUsedPermille
    return max(0, 1_000 - usedPermille)
}

private func dutyHeadroomBatteryLevel(from dutyCycle: DutyCycle) -> BatteryLevel {
    BatteryLevel(value: UInt8(dutyHeadroomPermille(from: dutyCycle) / 10))
}

private extension TelemetrySnapshot {
    var pwmHeadroomApplicability: EucRideMetricApplicability {
        guard pwm != nil else {
            return .unavailable
        }

        return switch operatingState {
        case .riding, .standing:
            .available
        case .parked, .charging, .unknown:
            .notApplicable
        }
    }

    var dutyHeadroom: BatteryLevel? {
        guard pwmHeadroomApplicability == .available, let pwm else {
            return nil
        }

        return dutyHeadroomBatteryLevel(from: pwm)
    }

    var pwmHeadroomPermille: Int? {
        guard pwmHeadroomApplicability == .available, let pwm else {
            return nil
        }

        return dutyHeadroomPermille(from: pwm)
    }
}

public enum VescWriteGuardrail: Equatable, Hashable, Sendable {
    case readOnly
    case unsupportedCommand
    case policyRefusal
    case authorizedButUnimplemented
    case parkedAndConfirmed
    case unknown

    fileprivate init(_ dto: MobileVescWriteGuardrailDto) {
        switch dto {
        case .readOnly:
            self = .readOnly
        case .unsupportedCommand:
            self = .unsupportedCommand
        case .policyRefusal:
            self = .policyRefusal
        case .authorizedButUnimplemented:
            self = .authorizedButUnimplemented
        case .parkedAndConfirmed:
            self = .parkedAndConfirmed
        case .unknown:
            self = .unknown
        }
    }
}

public struct VescDebugSnapshot: Equatable, Hashable, Sendable {
    public let profileTitle: String
    public let transportDetail: String
    public let dutyCycle: DutyCycle?
    public let maxSeenDutyCycle: DutyCycle?
    public let packVoltage: Voltage?
    public let batteryCurrentLimit: BatteryCurrent?
    public let motorCurrentLimit: PhaseCurrent?
    public let lastFault: String?
    public let inputApp: String?
    public let canStatus: String?
    public let logging: String?
    public let writeGuardrail: VescWriteGuardrail

    public init(
        profileTitle: String,
        transportDetail: String,
        dutyCycle: DutyCycle? = nil,
        maxSeenDutyCycle: DutyCycle? = nil,
        packVoltage: Voltage? = nil,
        batteryCurrentLimit: BatteryCurrent? = nil,
        motorCurrentLimit: PhaseCurrent? = nil,
        lastFault: String? = nil,
        inputApp: String? = nil,
        canStatus: String? = nil,
        logging: String? = nil,
        writeGuardrail: VescWriteGuardrail
    ) {
        self.profileTitle = profileTitle
        self.transportDetail = transportDetail
        self.dutyCycle = dutyCycle
        self.maxSeenDutyCycle = maxSeenDutyCycle
        self.packVoltage = packVoltage
        self.batteryCurrentLimit = batteryCurrentLimit
        self.motorCurrentLimit = motorCurrentLimit
        self.lastFault = lastFault
        self.inputApp = inputApp
        self.canStatus = canStatus
        self.logging = logging
        self.writeGuardrail = writeGuardrail
    }

    fileprivate init(_ dto: MobileVescDebugSnapshotDto) {
        self.init(
            profileTitle: dto.profileTitle,
            transportDetail: dto.transportDetail,
            dutyCycle: dto.dutyCycle,
            maxSeenDutyCycle: dto.maxSeenDutyCycle,
            packVoltage: dto.packVoltage?.value,
            batteryCurrentLimit: dto.batteryCurrentLimit?.value,
            motorCurrentLimit: dto.motorCurrentLimit?.value,
            lastFault: dto.lastFault,
            inputApp: dto.inputApp,
            canStatus: dto.canStatus,
            logging: dto.logging,
            writeGuardrail: VescWriteGuardrail(dto.writeGuardrail)
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

    private init(value: Value?, availability: ReadbackAvailability) {
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

public struct PedalMode: Equatable, Hashable, Sendable {
    public enum Value: Equatable, Hashable, Sendable {
        case hardnessPercent(UInt8)
        case rawMode(UInt16)
    }

    public let value: Value

    public var percent: UInt8? {
        guard case let .hardnessPercent(percent) = value else {
            return nil
        }
        return percent
    }

    public var rawMode: UInt16? {
        guard case let .rawMode(rawMode) = value else {
            return nil
        }
        return rawMode
    }

    public init(hardnessPercent: UInt8) {
        self.value = .hardnessPercent(hardnessPercent)
    }

    public static func rawMode(_ value: UInt16) -> Self {
        Self(value: .rawMode(value))
    }

    private init(value: Value) {
        self.value = value
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
    public let pedalMode: ReadbackValue<PedalMode>

    public init(
        beepMargin: ReadbackValue<Speed> = .unavailable,
        tiltback: ReadbackValue<Speed> = .unavailable,
        pedalMode: ReadbackValue<PedalMode> = .unavailable
    ) {
        self.beepMargin = beepMargin
        self.tiltback = tiltback
        self.pedalMode = pedalMode
    }

    fileprivate init(_ dto: MobileEucGarageSettingsDto) {
        let availability = ReadbackAvailability(dto.availability)
        self.init(
            beepMargin: Self.readback(dto.beepMargin?.value, availability: availability),
            tiltback: Self.readback(dto.tiltback?.value, availability: availability),
            pedalMode: Self.readback(
                dto.pedalMode.flatMap(PedalMode.init),
                availability: availability
            )
        )
    }

    private static func readback<Value>(
        _ value: Value?,
        availability: ReadbackAvailability
    ) -> ReadbackValue<Value> {
        if let value {
            return .available(value)
        }

        return availability == .unsupported ? .unsupported : .unavailable
    }
}

private extension PedalMode {
    init?(_ dto: MobilePedalModeDto) {
        guard let rawMode = dto.rawMode else {
            return nil
        }
        self = .rawMode(rawMode)
    }
}

public enum EucFaultHistoryState: Equatable, Hashable, Sendable {
    case none(sinceDistance: Distance)
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
    public let millimetersPerSecond: Int32?
    public let source: ReadbackSource?
    public let quality: ReadbackQuality?

    public init(snapshot: TelemetrySnapshot?) {
        self.init(
            millimetersPerSecond: snapshot?.speed?.value,
            source: snapshot?.speedSource,
            quality: snapshot?.speedQuality
        )
    }

    public init(
        millimetersPerSecond: Int32?,
        source: ReadbackSource? = nil,
        quality: ReadbackQuality? = nil
    ) {
        self.millimetersPerSecond = millimetersPerSecond
        self.source = source
        self.quality = quality
    }

    public var displayValue: String {
        guard let millimetersPerSecond else {
            return "--"
        }
        return RideUnits.speedText(millimetersPerSecond: millimetersPerSecond)
    }

    public var displayUnit: String {
        RideUnits.speedUnit
    }
}

public enum PhoneLocationFreshness: String, Equatable, Hashable, Sendable {
    case unavailable
    case fresh
    case stale
}

public struct PhoneLocationReadback: Equatable, Hashable, Sendable {
    public let speed: SpeedReadout
    public let sampleWallClockUnixMilliseconds: UInt64?

    public init(snapshot: MobilePhoneLocationSnapshotDto) {
        self.speed = SpeedReadout(millimetersPerSecond: snapshot.gpsSpeed?.value.value)
        self.sampleWallClockUnixMilliseconds = snapshot.latestSample?.wallClockUnixMs
    }

    public func freshness(
        at wallClockUnixMilliseconds: UInt64,
        staleAfterMilliseconds: UInt64 = 3_000
    ) -> PhoneLocationFreshness {
        guard speed.millimetersPerSecond != nil, let sampleWallClockUnixMilliseconds else {
            return .unavailable
        }
        guard wallClockUnixMilliseconds >= sampleWallClockUnixMilliseconds else {
            return .stale
        }
        return wallClockUnixMilliseconds - sampleWallClockUnixMilliseconds <= staleAfterMilliseconds ? .fresh : .stale
    }

    public func detail(at wallClockUnixMilliseconds: UInt64) -> String {
        switch freshness(at: wallClockUnixMilliseconds) {
        case .unavailable:
            "GPS unavailable"
        case .fresh:
            "fresh GPS"
        case .stale:
            "stale GPS"
        }
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
    public let pageSelector: UInt8?
    public let pageTag: UInt16?
    public let pageKind: String?
    public let pageVerification: VerificationState?
    public let energyPercent: BatteryLevel?
    public let voltage: Voltage?
    public let current: BatteryCurrent?
    public let bmsPackCurrent0: BatteryCurrent?
    public let bmsPackCurrent1: BatteryCurrent?
    public let cellDelta: VoltageDelta?
    public let lowestGroupIndex: Int?
    public let highestTemperature: Temperature?
    public let temperatureReadings: [Temperature]
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
        pageSelector: UInt8? = nil,
        pageTag: UInt16? = nil,
        pageKind: String? = nil,
        pageVerification: VerificationState? = nil,
        energyPercent: BatteryLevel? = nil,
        voltage: Voltage? = nil,
        current: BatteryCurrent? = nil,
        bmsPackCurrent0: BatteryCurrent? = nil,
        bmsPackCurrent1: BatteryCurrent? = nil,
        cellDelta: VoltageDelta? = nil,
        lowestGroupIndex: Int? = nil,
        highestTemperature: Temperature? = nil,
        temperatureReadings: [Temperature] = [],
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
        let hasReadbackData = availability == .available
        self.availability = availability
        self.topology = topology
        self.pageSelector = hasReadbackData ? pageSelector : nil
        self.pageTag = hasReadbackData ? pageTag : nil
        self.pageKind = hasReadbackData ? pageKind : nil
        self.pageVerification = hasReadbackData ? pageVerification : nil
        self.energyPercent = hasReadbackData ? energyPercent : nil
        self.voltage = hasReadbackData ? voltage : nil
        self.current = hasReadbackData ? current : nil
        self.bmsPackCurrent0 = hasReadbackData ? bmsPackCurrent0 : nil
        self.bmsPackCurrent1 = hasReadbackData ? bmsPackCurrent1 : nil
        self.cellDelta = hasReadbackData ? cellDelta : nil
        self.lowestGroupIndex = hasReadbackData ? lowestGroupIndex : nil
        self.highestTemperature = hasReadbackData ? highestTemperature : nil
        self.temperatureReadings = hasReadbackData ? temperatureReadings : []
        self.highestTemperatureLabel = hasReadbackData ? highestTemperatureLabel : nil
        self.balancingSummary = hasReadbackData ? balancingSummary : nil
        self.balancingDetail = hasReadbackData ? balancingDetail : nil
        self.faultSummary = hasReadbackData ? faultSummary : nil
        self.faultDetail = hasReadbackData ? faultDetail : nil
        self.groups = hasReadbackData ? groups : []
        self.faults = hasReadbackData ? faults : []
        self.captureActionTitle = hasReadbackData ? captureActionTitle : nil
        self.captureActionState = hasReadbackData ? captureActionState : nil
    }

    fileprivate init(_ dto: MobileBmsSnapshotDto) {
        self.init(
            availability: ReadbackAvailability(dto.availability),
            topology: BmsTopology(dto.topology),
            pageSelector: dto.pageSelector,
            pageTag: dto.pageTag,
            pageKind: dto.pageKind,
            pageVerification: dto.pageVerification.map(VerificationState.init),
            energyPercent: dto.energyPercent?.value,
            voltage: dto.voltage?.value,
            current: dto.current?.value,
            bmsPackCurrent0: dto.bmsPackCurrent0?.value,
            bmsPackCurrent1: dto.bmsPackCurrent1?.value,
            cellDelta: dto.cellDelta?.value,
            lowestGroupIndex: dto.lowestGroupIndex.map(Int.init),
            highestTemperature: dto.highestTemperature?.value,
            temperatureReadings: dto.temperatures.map(\.value),
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

    public var shouldRenderReadback: Bool {
        availability != .available
            || energyPercent != nil
            || pageSelector != nil
            || pageTag != nil
            || pageKind != nil
            || voltage != nil
            || current != nil
            || bmsPackCurrent0 != nil
            || bmsPackCurrent1 != nil
            || !temperatureReadings.isEmpty
            || highestTemperature != nil
    }

    public var readbackRows: [SessionDebugRow] {
        var rows = [
            SessionDebugRow(label: "availability", value: availability.displayText),
            SessionDebugRow(label: "page", value: bmsPageText(selector: pageSelector, tag: pageTag, kind: pageKind)),
            SessionDebugRow(label: "page verification", value: pageVerification?.displayText ?? "unavailable"),
            SessionDebugRow(label: "charge", value: bmsPercentText(energyPercent)),
            SessionDebugRow(label: "voltage", value: bmsVoltageText(voltage)),
            SessionDebugRow(label: "current", value: bmsCurrentText(current)),
        ]
        if let bmsPackCurrent0 {
            rows.append(
                SessionDebugRow(label: "bms current 0", value: bmsCurrentText(bmsPackCurrent0))
            )
        }
        if let bmsPackCurrent1 {
            rows.append(
                SessionDebugRow(label: "bms current 1", value: bmsCurrentText(bmsPackCurrent1))
            )
        }
        rows += [
            SessionDebugRow(label: "high group", value: bmsGroupVoltageText(highGroupVoltage)),
            SessionDebugRow(label: "low group", value: bmsGroupVoltageText(lowGroupVoltage)),
            SessionDebugRow(label: "delta", value: bmsMillivoltsText(cellDelta)),
            SessionDebugRow(label: "lowest group", value: lowestGroupIndex.map(String.init) ?? "unavailable"),
            SessionDebugRow(label: "temperature", value: bmsTemperatureText(highestTemperature)),
            SessionDebugRow(label: "temperature sensors", value: bmsCountText(temperatureReadings.count)),
            SessionDebugRow(label: "topology", value: topology.layoutLabel),
        ]
        return rows
    }

    public func mergingBmsPage(_ update: BmsSnapshot) -> BmsSnapshot {
        guard availability == .available, update.availability == .available else {
            return update
        }

        return BmsSnapshot(
            topology: topology.mergingBmsPage(update.topology),
            pageSelector: nil,
            pageTag: nil,
            pageKind: nil,
            pageVerification: update.pageVerification ?? pageVerification,
            energyPercent: update.energyPercent ?? energyPercent,
            voltage: update.voltage ?? voltage,
            current: update.current ?? current,
            bmsPackCurrent0: update.bmsPackCurrent0 ?? bmsPackCurrent0,
            bmsPackCurrent1: update.bmsPackCurrent1 ?? bmsPackCurrent1,
            cellDelta: update.cellDelta ?? cellDelta,
            lowestGroupIndex: update.lowestGroupIndex ?? lowestGroupIndex,
            highestTemperature: update.highestTemperature ?? highestTemperature,
            temperatureReadings: update.temperatureReadings.isEmpty ? temperatureReadings : update.temperatureReadings,
            highestTemperatureLabel: update.highestTemperatureLabel ?? highestTemperatureLabel,
            balancingSummary: update.balancingSummary ?? balancingSummary,
            balancingDetail: update.balancingDetail ?? balancingDetail,
            faultSummary: update.faultSummary ?? faultSummary,
            faultDetail: update.faultDetail ?? faultDetail,
            groups: update.groups.isEmpty ? groups : mergeGroups(update.groups, into: groups),
            faults: update.faults.isEmpty ? faults : update.faults,
            captureActionTitle: update.captureActionTitle ?? captureActionTitle,
            captureActionState: update.captureActionState ?? captureActionState
        )
    }

    public var averageGroupVoltage: Voltage? {
        guard !groupVoltages.isEmpty else {
            return nil
        }

        let total = groupVoltages.reduce(0) { partial, voltage in
            partial + Int64(voltage.value)
        }
        let count = Int64(groupVoltages.count)
        return Voltage(value: Int32(total / count))
    }

    public var lowestGroupLabel: String? {
        lowestGroupIndex.map { "group \($0)" }
    }

    public var cellMapVisibilitySummary: String {
        "\(groups.count) groups visible"
    }

    public var cellMapSpreadSummary: String {
        cellDelta.map { "\($0.value) mV spread" } ?? "delta unavailable"
    }

    public var cellMapFocusSummary: String {
        let flaggedIndices = flaggedGroups.map(\.index)
        guard !flaggedIndices.isEmpty else {
            return lowestGroupLabel.map { "\($0) lowest" } ?? topology.layoutLabel
        }
        return "groups \(flaggedIndices.map(String.init).joined(separator: ", ")) flagged"
    }

    public var cellMapFocusDetail: String? {
        flaggedGroups.lazy.compactMap(\.detail).first ?? highestTemperatureLabel
    }

    public func detailGroupStatus(for index: Int) -> String {
        guard let group = groups.first(where: { $0.index == index }) else {
            return topology.layoutLabel
        }

        if lowestGroupIndex == group.index {
            return cellDelta.map { "lowest group · \($0.value) mV below pack avg" } ?? "lowest group"
        }

        guard
            let averageGroupVoltage,
            let groupVoltage = group.voltage
        else {
            return group.label ?? "group \(group.index)"
        }

        let delta = abs(Int(averageGroupVoltage.value) - Int(groupVoltage.value))
        return "group \(group.index) · \(delta) mV from pack avg"
    }

    public func detailGroupTrend(for index: Int) -> String {
        groups.first(where: { $0.index == index })?.detail ?? "not enough history"
    }

    public func detailGroupTrendDetail(for index: Int) -> String {
        guard groups.contains(where: { $0.index == index }) else {
            return topology.layoutLabel
        }
        return cellMapSpreadSummary
    }

    public var unknownTopologyVoltageDetail: String {
        topology.layoutLabel
    }

    public var unknownTopologyCellCountValue: String {
        topology.seriesGroupCount.map(String.init) ?? "?"
    }

    public var unknownTopologyCellCountDetail: String {
        switch topology.confidence {
        case .verified:
            "layout verified"
        case .inferred:
            "layout inferred"
        case .unverified:
            "layout unverified"
        }
    }

    public var unknownTopologyTemperatureValue: String {
        let sensorCount = groups.compactMap(\.temperature).count
        guard sensorCount > 0 else {
            return "--"
        }
        return String(sensorCount)
    }

    public var unknownTopologyTemperatureDetail: String {
        highestTemperatureLabel ?? "sensor names unavailable"
    }

    public var unknownTopologyCaptureDetail: String {
        faultDetail ?? topology.layoutLabel
    }

    public var inlineCellMapModeTitles: [String] {
        var titles = ["balance view"]
        if groups.contains(where: { $0.temperature != nil }) {
            titles.append("temps")
        }
        if !faults.isEmpty || !flaggedGroups.isEmpty {
            titles.append("faults")
        }
        return titles
    }

    public var scrollableCellMapModeTitles: [String] {
        var titles = ["overview", "strip", "raw table"]
        if groups.contains(where: { $0.temperature != nil }) {
            titles.append("temps")
        } else if !faults.isEmpty || !flaggedGroups.isEmpty {
            titles.append("faults")
        }
        return titles
    }

    public var cellMapInteractionHint: String {
        if groups.contains(where: { $0.resistance != nil }) {
            return "tap a group for history, IR estimate, and BMS raw fields"
        }
        if groups.contains(where: { $0.temperature != nil }) {
            return "tap a group for voltage and temperature detail"
        }
        return "tap a group for exact voltage detail"
    }

    public var scrollableCellMapRule: String {
        guard !groups.isEmpty else {
            return topology.layoutLabel
        }
        return "\(groups.count) groups need overview before exact cells"
    }

    public var scrollableCellMapFocusHint: String {
        flaggedGroups.isEmpty ? "scan the raw table after the overview" : "show flagged groups before the raw table"
    }

    public var noDataWarningTitle: String {
        "No cell-level BMS data"
    }

    public var noDataWarningLines: [String] {
        [
            "CutOut can’t see individual cell balance or weak groups.",
            "BMS temperature, faults, or cutout reason stay unavailable.",
        ]
    }

    public var noDataUnknownRows: [String] {
        [
            "individual cell/group voltages",
            "cell balance / weak parallel group",
            "BMS temperature, faults, and cutout reason",
        ]
    }

    private var groupVoltages: [Voltage] {
        groups.compactMap(\.voltage)
    }

    private var flaggedGroups: [BmsGroupSnapshot] {
        groups.filter { group in
            switch group.alertLevel {
            case .warning, .critical:
                true
            case .nominal, .unknown:
                false
            }
        }
    }

    private var highGroupVoltage: Voltage? {
        groupVoltages.max { left, right in
            left.value < right.value
        }
    }

    private var lowGroupVoltage: Voltage? {
        groupVoltages.min { left, right in
            left.value < right.value
        }
    }
}

public extension BmsSnapshot {
    func withoutPageCursor() -> BmsSnapshot {
        BmsSnapshot(
            availability: availability,
            topology: topology,
            pageSelector: nil,
            pageTag: nil,
            pageKind: nil,
            pageVerification: pageVerification,
            energyPercent: energyPercent,
            voltage: voltage,
            current: current,
            bmsPackCurrent0: bmsPackCurrent0,
            bmsPackCurrent1: bmsPackCurrent1,
            cellDelta: cellDelta,
            lowestGroupIndex: lowestGroupIndex,
            highestTemperature: highestTemperature,
            temperatureReadings: temperatureReadings,
            highestTemperatureLabel: highestTemperatureLabel,
            balancingSummary: balancingSummary,
            balancingDetail: balancingDetail,
            faultSummary: faultSummary,
            faultDetail: faultDetail,
            groups: groups,
            faults: faults,
            captureActionTitle: captureActionTitle,
            captureActionState: captureActionState
        )
    }
}

public extension BmsTopology {
    func mergingBmsPage(_ update: BmsTopology) -> BmsTopology {
        guard update.hasObservedBmsTopology else {
            return self
        }
        guard hasObservedBmsTopology else {
            return update
        }
        return update.observedRank >= observedRank ? update : self
    }

    var hasObservedBmsTopology: Bool {
        bmsCount > 0 || packCount > 0 || seriesGroupCount != nil || layoutLabel.localizedCaseInsensitiveContains("observed")
    }

    var observedRank: Int {
        var rank = 0
        if packCount > 0 { rank += 1 }
        if bmsCount > 0 { rank += 1 }
        if seriesGroupCount != nil { rank += 2 }
        if parallelCount != nil { rank += 1 }
        if layoutLabel.localizedCaseInsensitiveContains("observed") { rank += 1 }
        switch confidence {
        case .verified:
            rank += 3
        case .inferred:
            rank += 2
        case .unverified:
            rank += 1
        }
        return rank
    }
}

private func mergeGroups(_ updates: [BmsGroupSnapshot], into existing: [BmsGroupSnapshot]) -> [BmsGroupSnapshot] {
    var groupsByIndex = Dictionary(uniqueKeysWithValues: existing.map { ($0.index, $0) })
    for update in updates {
        groupsByIndex[update.index] = update
    }
    return groupsByIndex.values.sorted { $0.index < $1.index }
}

private func bmsPercentText(_ value: BatteryLevel?) -> String {
    guard let value else { return "--" }
    return RideUnits.percentText(value.value) + "%"
}

private func bmsPageText(selector: UInt8?, tag: UInt16?, kind: String?) -> String {
    var parts: [String] = []
    if let kind {
        parts.append(kind)
    }
    if let tag {
        parts.append(String(format: "0x%02x", Int(tag)))
    }
    if let selector {
        parts.append("#\(selector)")
    }
    return parts.isEmpty ? "--" : parts.joined(separator: " ")
}

private func bmsVoltageText(_ value: Voltage?) -> String {
    value.map { RideUnits.voltageText(millivolts: $0.value) } ?? "--"
}

private func bmsCurrentText(_ value: BatteryCurrent?) -> String {
    value.map { RideUnits.currentText(milliamps: $0.value) } ?? "--"
}

private func bmsMillivoltsText(_ value: VoltageDelta?) -> String {
    value.map { String($0.value) } ?? "--"
}

private func bmsCountText(_ value: Int) -> String {
    value == 0 ? "--" : String(value)
}

private func bmsTemperatureText(_ value: Temperature?) -> String {
    value.map { RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 1) } ?? "--"
}

private func bmsGroupVoltageText(_ value: Voltage?) -> String {
    guard let value else { return "--" }
    return RideUnits.voltageText(millivolts: value.value, fractionDigits: 3)
}

public struct SessionDebugRow: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

public extension ReadbackAvailability {
    var displayText: String {
        switch self {
        case .available:
            "available"
        case .unavailable:
            "unavailable"
        case .unsupported:
            "unsupported"
        }
    }
}

public extension VerificationState {
    var displayText: String {
        switch self {
        case .unverified:
            "unverified"
        case .inferred:
            "inferred"
        case .sourceVerified:
            "source verified"
        case .hardwareVerified:
            "hardware verified"
        case .sourceAndHardwareVerified:
            "source and hardware verified"
        }
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
        reducing(snapshot: step.snapshot, receivedAt: receivedAt)
    }

    public func reducing(
        snapshot: TelemetrySnapshot?,
        receivedAt: MonotonicMilliseconds
    ) -> RideDisplayState {
        let nextSpeed: SpeedReadout
        if let snapshot, snapshot.speed != nil {
            nextSpeed = SpeedReadout(snapshot: snapshot)
        } else {
            nextSpeed = speed
        }
        return RideDisplayState(
            speed: nextSpeed,
            telemetry: snapshot ?? telemetry,
            notificationCount: notificationCount + 1,
            lastUpdate: receivedAt
        )
    }

    private var lastUpdateText: String {
        lastUpdate.map { "\($0.rawValue) ms" } ?? "never"
    }
}

public enum EucRideMetricApplicability: Equatable, Hashable, Sendable {
    case available
    case unavailable
    case notApplicable
}

public enum EucRideTelemetryAvailability: Equatable, Hashable, Sendable {
    case unavailable
    case waitingForValues
    case populated
}

public enum EucRideUpdateFreshness: Equatable, Hashable, Sendable {
    case unavailable
    case fresh
    case stale
}

public struct EucRideUpdateAge: Equatable, Hashable, Sendable {
    public let elapsed: MonotonicMilliseconds?
    public let freshness: EucRideUpdateFreshness

    public init(elapsed: MonotonicMilliseconds?, freshness: EucRideUpdateFreshness) {
        self.elapsed = elapsed
        self.freshness = freshness
    }
}

public func rideUpdateAge(
    updatedAt: MonotonicMilliseconds?,
    at now: MonotonicMilliseconds,
    staleAfter staleThreshold: MonotonicMilliseconds
) -> EucRideUpdateAge {
    guard let updatedAt else {
        return EucRideUpdateAge(elapsed: nil, freshness: .unavailable)
    }

    let elapsed = now.rawValue >= updatedAt.rawValue ? now.rawValue - updatedAt.rawValue : 0
    let freshness: EucRideUpdateFreshness = elapsed > staleThreshold.rawValue ? .stale : .fresh
    return EucRideUpdateAge(elapsed: MonotonicMilliseconds(elapsed), freshness: freshness)
}

public enum EucRideWarningSeverity: Equatable, Hashable, Sendable {
    case normal
    case caution
    case reduceAcceleration
    case limpHome
    case unavailable
    case failed
}

public struct EucRideWarningState: Equatable, Hashable, Sendable {
    public let severity: EucRideWarningSeverity
    public let title: String
    public let detail: String

    public init(severity: EucRideWarningSeverity, title: String, detail: String) {
        self.severity = severity
        self.title = title
        self.detail = detail
    }
}

public enum EucRideVisibleField: Equatable, Hashable, Sendable {
    case status
    case speed
    case updateAge
    case pwmHeadroom
    case sagAdjustedEnergy
    case packVoltage
    case power
    case thermal
    case warningState
    case voltageSag
    case regenPower
    case limpHomeRange
    case tabs
}

public enum EucRideVisibleFieldSource: Equatable, Hashable, Sendable {
    case sessionState
    case liveTelemetry
    case derivedTelemetry
    case explicitlyUnavailable
    case notApplicable
    case staticNavigation
}

public struct EucRideVisibleFieldCoverage: Equatable, Hashable, Sendable {
    public let field: EucRideVisibleField
    public let source: EucRideVisibleFieldSource

    public init(field: EucRideVisibleField, source: EucRideVisibleFieldSource) {
        self.field = field
        self.source = source
    }
}

public enum EucRideLiveValidationField: String, Equatable, Hashable, Sendable {
    case livePhase
    case updateAge
    case speed
    case packVoltage
    case power
    case pwm
    case thermal
}

public struct EucRideScreenState: Equatable, Hashable, Sendable {
    private static let reduceAccelerationPwmHeadroomThreshold = 250

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

    public var telemetryAvailability: EucRideTelemetryAvailability {
        guard let telemetry else {
            return .unavailable
        }

        return telemetry.hasVisibleRideValues ? .populated : .waitingForValues
    }

    public var pwmHeadroomApplicability: EucRideMetricApplicability {
        telemetry?.pwmHeadroomApplicability ?? .unavailable
    }

    public var pwmHeadroomPermille: Int? {
        telemetry?.pwmHeadroomPermille
    }

    public var regenerationPower: Power? {
        guard telemetry?.powerFlow == .regeneration else {
            return nil
        }

        return telemetry?.displayPower
    }

    public var voltageSag: VoltageDelta? {
        telemetry?.voltageSag
    }

    public var limpHomeRange: Distance? {
        telemetry?.limpHomeRange
    }

    public var controllerOnlyEstimatePercent: BatteryLevel? {
        telemetry?.batteryLevelReported ?? telemetry?.batteryLevelEstimated
    }

    public var controllerOnlyEstimateDetail: String {
        if telemetry?.voltage != nil, voltageSag != nil {
            return "derived from voltage curve + recent sag"
        }
        if telemetry?.voltage != nil {
            return "derived from voltage curve only"
        }
        return "estimate unavailable"
    }

    public var controllerOnlyConfidenceTitle: String {
        if controllerOnlyEstimatePercent != nil, voltageSag != nil {
            return "medium"
        }
        if controllerOnlyEstimatePercent != nil || telemetry?.voltage != nil {
            return "low"
        }
        return "unknown"
    }

    public var controllerOnlyConfidenceDetail: String {
        controllerOnlyConfidenceTitle == "unknown" ? "telemetry unavailable" : "not cell-safe"
    }

    public var controllerOnlyRidingRuleProgress: Double {
        switch controllerOnlyConfidenceTitle {
        case "medium":
            0.62
        case "low":
            0.35
        default:
            0.15
        }
    }

    public func updateAge(
        at now: MonotonicMilliseconds,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> EucRideUpdateAge {
        guard let updatedAt = telemetry?.at ?? displayState.lastUpdate else {
            return EucRideUpdateAge(elapsed: nil, freshness: .unavailable)
        }

        let elapsed = now.rawValue >= updatedAt.rawValue ? now.rawValue - updatedAt.rawValue : 0
        let freshness: EucRideUpdateFreshness = elapsed > staleThreshold.rawValue ? .stale : .fresh
        return EucRideUpdateAge(elapsed: MonotonicMilliseconds(elapsed), freshness: freshness)
    }

    public var warningState: EucRideWarningState {
        switch phase {
        case .failed(let failure):
            return EucRideWarningState(severity: .failed, title: "Connection failed", detail: failure.displayText)
        case .live where telemetryAvailability == .populated:
            if shouldReduceAcceleration {
                return EucRideWarningState(
                    severity: .reduceAcceleration,
                    title: "Reduce acceleration",
                    detail: "PWM headroom is low while riding"
                )
            }

            return EucRideWarningState(
                severity: .normal,
                title: "Telemetry live",
                detail: telemetry?.speed == nil ? "Waiting for speed telemetry" : "Live telemetry from typed Rust/FFI state"
            )
        case .live where telemetryAvailability == .waitingForValues:
            return EucRideWarningState(
                severity: .caution,
                title: "Waiting for telemetry",
                detail: "Subscribed; no ride values yet"
            )
        case .live:
            return EucRideWarningState(
                severity: .unavailable,
                title: "Telemetry unavailable",
                detail: "No live snapshot yet"
            )
        case .connecting, .discoveringServices, .subscribing:
            return EucRideWarningState(severity: .caution, title: phaseText, detail: "Waiting for live telemetry")
        case .starting, .bluetoothUnavailable, .scanning:
            return EucRideWarningState(severity: .unavailable, title: phaseText, detail: "Ride screen is not active yet")
        }
    }

    public func warningState(
        at now: MonotonicMilliseconds,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> EucRideWarningState {
        let age = updateAge(at: now, staleAfter: staleThreshold)
        if phase == .live, age.freshness == .stale, let elapsed = age.elapsed {
            return EucRideWarningState(
                severity: .caution,
                title: "Telemetry stale",
                detail: "Last update \(elapsed.rawValue) ms ago"
            )
        }
        return warningState
    }

    public var phaseText: String {
        phase.displayText
    }

    public var statusText: String {
        guard phase == .live else {
            return phaseText
        }

        switch operatingState {
        case .parked:
            return "Parked"
        case .standing:
            return "Standing"
        case .riding:
            return "Riding"
        case .charging:
            return "Charging"
        case .unknown:
            return "Live"
        }
    }

    public var speedText: String {
        displayState.speed.displayValue
    }

    public var speedUnit: String {
        displayState.speed.displayUnit
    }

    public var visibleFieldCoverage: [EucRideVisibleFieldCoverage] {
        [
            EucRideVisibleFieldCoverage(field: .status, source: .sessionState),
            EucRideVisibleFieldCoverage(field: .speed, source: speedCoverage),
            EucRideVisibleFieldCoverage(field: .updateAge, source: updateAgeCoverage),
            EucRideVisibleFieldCoverage(field: .pwmHeadroom, source: pwmHeadroomCoverage),
            EucRideVisibleFieldCoverage(field: .sagAdjustedEnergy, source: .explicitlyUnavailable),
            EucRideVisibleFieldCoverage(
                field: .packVoltage,
                source: telemetry?.voltage == nil ? .explicitlyUnavailable : .liveTelemetry
            ),
            EucRideVisibleFieldCoverage(field: .power, source: powerCoverage),
            EucRideVisibleFieldCoverage(field: .thermal, source: thermalCoverage),
            EucRideVisibleFieldCoverage(field: .warningState, source: .sessionState),
            EucRideVisibleFieldCoverage(field: .voltageSag, source: voltageSagCoverage),
            EucRideVisibleFieldCoverage(field: .regenPower, source: regenerationPowerCoverage),
            EucRideVisibleFieldCoverage(field: .limpHomeRange, source: limpHomeRangeCoverage),
            EucRideVisibleFieldCoverage(field: .tabs, source: .staticNavigation),
        ]
    }

    public var liveValidationMissingFields: [EucRideLiveValidationField] {
        var missing: [EucRideLiveValidationField] = []

        if phase != .live {
            missing.append(.livePhase)
        }
        if telemetry?.at == nil && displayState.lastUpdate == nil {
            missing.append(.updateAge)
        }
        if telemetry?.speed == nil {
            missing.append(.speed)
        }
        if telemetry?.voltage == nil {
            missing.append(.packVoltage)
        }
        if telemetry?.displayPower == nil {
            missing.append(.power)
        }
        if telemetry?.pwm == nil {
            missing.append(.pwm)
        }
        if telemetry?.hasTemperature != true {
            missing.append(.thermal)
        }

        return missing
    }

    public var isLiveValidationReady: Bool {
        liveValidationMissingFields.isEmpty
    }

    private var speedCoverage: EucRideVisibleFieldSource {
        displayState.speed.millimetersPerSecond == nil ? .explicitlyUnavailable : .liveTelemetry
    }

    private var updateAgeCoverage: EucRideVisibleFieldSource {
        telemetry?.at == nil && displayState.lastUpdate == nil ? .explicitlyUnavailable : .liveTelemetry
    }

    private var shouldReduceAcceleration: Bool {
        guard let pwmHeadroomPermille else {
            return false
        }

        return pwmHeadroomPermille <= Self.reduceAccelerationPwmHeadroomThreshold
    }

    private var pwmHeadroomCoverage: EucRideVisibleFieldSource {
        switch pwmHeadroomApplicability {
        case .available:
            .derivedTelemetry
        case .unavailable:
            .explicitlyUnavailable
        case .notApplicable:
            .notApplicable
        }
    }

    private var powerCoverage: EucRideVisibleFieldSource {
        guard let telemetry else {
            return .explicitlyUnavailable
        }
        if telemetry.derivedPackPower != nil {
            return .derivedTelemetry
        }
        if telemetry.power != nil {
            return .liveTelemetry
        }
        return .explicitlyUnavailable
    }

    private var regenerationPowerCoverage: EucRideVisibleFieldSource {
        regenerationPower == nil ? .explicitlyUnavailable : .derivedTelemetry
    }

    private var voltageSagCoverage: EucRideVisibleFieldSource {
        voltageSag == nil ? .explicitlyUnavailable : .derivedTelemetry
    }

    private var limpHomeRangeCoverage: EucRideVisibleFieldSource {
        limpHomeRange == nil ? .explicitlyUnavailable : .derivedTelemetry
    }

    private var thermalCoverage: EucRideVisibleFieldSource {
        guard let telemetry else {
            return .explicitlyUnavailable
        }
        return telemetry.hasTemperature ? .liveTelemetry : .explicitlyUnavailable
    }
}

private extension TelemetrySnapshot {
    var displayPower: Power? {
        derivedPackPower ?? power
    }

    var derivedPackPower: Power? {
        guard let voltage, let batteryCurrent, batteryCurrent.value != 0 else {
            return nil
        }

        return Power(value: Int64(voltage.value) * Int64(batteryCurrent.value) / 1_000)
    }

    var hasVisibleRideValues: Bool {
        speed != nil
            || voltage != nil
            || batteryCurrent != nil
            || motorCurrent != nil
            || power != nil
            || controllerTemperature != nil
            || motorTemperature != nil
            || batteryTemperature != nil
            || pwm != nil
            || footpad != nil
            || pitch != nil
            || roll != nil
            || batteryLevelReported != nil
            || batteryLevelEstimated != nil
    }

    var hasTemperature: Bool {
        controllerTemperature != nil || motorTemperature != nil || batteryTemperature != nil
    }
}

public enum SessionConnectionFailure: Equatable, Hashable, Sendable {
    case missingNotifyChannel
    case missingWriteChannel
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
        case .missingWriteChannel:
            "Missing write channel"
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
    case scanning
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
        case .scanning:
            "Scanning for rides..."
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
    init(_ dto: DiscoveryElectricUnicycleModel) {
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

public final class VescOnewheelSession: @unchecked Sendable {
    private let inner: VescReadOnlySession

    public init() {
        self.inner = VescReadOnlySession()
    }

    public init(boardProfile: VescBoardProfile) {
        self.inner = VescReadOnlySession.withBoardProfile(boardProfile: boardProfile)
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
        try step(.linkUp, at: monotonicMilliseconds, writeLimit: writeLimit)
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
        try inner.step(
            kind,
            at: monotonicMilliseconds,
            writeLimit: writeLimit,
            channel: channel,
            bytes: bytes,
            command: command
        )
    }
}

private protocol MobileReadOnlySession {
    func ingestChecked(input: MobileSessionInputDto) -> MobileSessionStepResultDto
}

extension AeroReadOnlySession: MobileReadOnlySession {}
extension FalconReadOnlySession: MobileReadOnlySession {}
extension VescReadOnlySession: MobileReadOnlySession {}

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

    public static let vescNordicUartNotify = BluetoothUuid(Data([
        0x6e, 0x40, 0x00, 0x03,
        0xb5, 0xa3,
        0xf3, 0x93,
        0xe0, 0xa9,
        0xe5, 0x0e, 0x24, 0xdc, 0xca, 0x9e,
    ]))!

    public static let vescNordicUartService = BluetoothUuid(Data([
        0x6e, 0x40, 0x00, 0x01,
        0xb5, 0xa3,
        0xf3, 0x93,
        0xe0, 0xa9,
        0xe5, 0x0e, 0x24, 0xdc, 0xca, 0x9e,
    ]))!

    public static let vescNordicUartWrite = BluetoothUuid(Data([
        0x6e, 0x40, 0x00, 0x02,
        0xb5, 0xa3,
        0xf3, 0x93,
        0xe0, 0xa9,
        0xe5, 0x0e, 0x24, 0xdc, 0xca, 0x9e,
    ]))!

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

    public init(deviceKind: String?) {
        switch deviceKind.flatMap(mobileElectricUnicycleModelHintFromDeviceKind) {
        case .some(.aero):
            self = .aero
        case .some(.falcon):
            self = .falcon
        case .none:
            self = .unknown
        }
    }
}

public struct CoreBluetoothManufacturerDataSummary: Equatable, Hashable, Sendable {
    public let companyIdentifier: UInt16
    public let payloadLength: UInt64

    public init(companyIdentifier: UInt16, payloadLength: UInt64) {
        self.companyIdentifier = companyIdentifier
        self.payloadLength = payloadLength
    }
}

public struct CoreBluetoothAdvertisement: Equatable, Hashable, Sendable {
    public let peripheralIdentifier: CoreBluetoothPeripheralIdentifier
    public let localName: String?
    public let advertisedServiceUuids: [BluetoothUuid]
    public let manufacturerData: [CoreBluetoothManufacturerDataSummary]
    public let rssiDbm: Int16?

    public init(
        peripheralIdentifier: CoreBluetoothPeripheralIdentifier,
        localName: String?,
        advertisedServiceUuids: [BluetoothUuid],
        manufacturerData: [CoreBluetoothManufacturerDataSummary] = [],
        rssiDbm: Int16? = nil
    ) {
        self.peripheralIdentifier = peripheralIdentifier
        self.localName = localName
        self.advertisedServiceUuids = advertisedServiceUuids
        self.manufacturerData = manufacturerData
        self.rssiDbm = rssiDbm
    }

    public var modelHint: CutoutModelHint {
        CutoutModelHint(deviceKind: localName)
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
    case vescOnewheel(VescOnewheelSession)

    public static func electricUnicycle(model: ElectricUnicycleModel) throws -> CoreBluetoothSession {
        try .electricUnicycle(ElectricUnicycleSession(model: model))
    }

    public static func vescOnewheel() -> CoreBluetoothSession {
        .vescOnewheel(VescOnewheelSession())
    }

    public static func vescOnewheel(boardProfile: VescBoardProfile) -> CoreBluetoothSession {
        .vescOnewheel(VescOnewheelSession(boardProfile: boardProfile))
    }

    fileprivate var currentSnapshot: TelemetrySnapshot {
        switch self {
        case .electricUnicycle(let session):
            session.currentSnapshot
        case .vescOnewheel(let session):
            session.currentSnapshot
        }
    }

    fileprivate var startupProbeOperations: [CoreBluetoothPlannedOperation] {
        switch self {
        case .electricUnicycle(let session) where session.model == .falcon:
            [
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("N".utf8)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("V".utf8)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("M".utf8)),
            ]
        case .electricUnicycle:
            []
        case .vescOnewheel:
            []
        }
    }

    fileprivate func linkUp(
        at monotonicMilliseconds: MonotonicMilliseconds,
        writeLimit: TransportWriteLimitBytes
    ) throws -> [SessionAction] {
        switch self {
        case .electricUnicycle(let session):
            try session.linkUp(at: monotonicMilliseconds, writeLimit: writeLimit)
        case .vescOnewheel(let session):
            try session.perform(.requestTelemetry, at: monotonicMilliseconds)
                + session.linkUp(at: monotonicMilliseconds, writeLimit: writeLimit)
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
        case .vescOnewheel(let session):
            try session.ingestNotificationActions(bytes, channel: channel.bytes, at: monotonicMilliseconds)
        }
    }

    fileprivate func perform(
        _ command: DeviceCommand,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> [SessionAction] {
        switch self {
        case .electricUnicycle(let session):
            try session.perform(command, at: monotonicMilliseconds)
        case .vescOnewheel(let session):
            try session.perform(command, at: monotonicMilliseconds)
        }
    }
}

public enum CoreBluetoothSessionEvent: Equatable, Hashable, Sendable {
    case linkUp(at: MonotonicMilliseconds)
    case notification(bytes: Data, channel: BluetoothUuid, at: MonotonicMilliseconds)
    case command(DeviceCommand, at: MonotonicMilliseconds)
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
            let operations = actions.flatMap(planner.plan(action:)) + session.startupProbeOperations
            return CoreBluetoothSessionStep(
                operations: operations,
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

        case .command(let command, let monotonicMilliseconds):
            let actions = try session.perform(command, at: monotonicMilliseconds)
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

public final class CoreBluetoothLiveSessionOwner: @unchecked Sendable {
    private let platformIdentifier: CoreBluetoothPeripheralIdentifier
    private let runner: CoreBluetoothSessionRunner
    private let retainedSink: CoreBluetoothOperationSink
    private let executor: CoreBluetoothOperationExecutor
    private let executionQueue: DispatchQueue?
    private let retryCommandOnLinkUp: DeviceCommand?
    private let retryDelay: DispatchTimeInterval
    private var recorded: [CoreBluetoothLiveRecord] = []
    private var pendingRetry: DispatchWorkItem?
    private var pendingRetryTimestamp: MonotonicMilliseconds?
    private var receivedRealtimeTelemetrySinceLinkUp = false
    private var pendingOperationsAfterSubscription: [CoreBluetoothPlannedOperation] = []
    private var waitingForSubscriptionChannel: BluetoothUuid?

    public init(
        session: CoreBluetoothSession,
        advertisement: CoreBluetoothAdvertisement,
        writeLimit: TransportWriteLimitBytes,
        operationSink: CoreBluetoothOperationSink,
        retryCommandOnLinkUp: DeviceCommand? = nil,
        retryDelay: DispatchTimeInterval = .seconds(1),
        executionQueue: DispatchQueue? = nil
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
        self.executionQueue = executionQueue
        self.retryCommandOnLinkUp = retryCommandOnLinkUp
        self.retryDelay = retryDelay
    }

    public var records: [CoreBluetoothLiveRecord] {
        recorded
    }

    @discardableResult
    public func handleLinkUp(at monotonicMilliseconds: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        cancelPendingRetry()
        receivedRealtimeTelemetrySinceLinkUp = false
        let step = try runner.handle(.linkUp(at: monotonicMilliseconds))
        record(.linkUp(
            platformIdentifier: platformIdentifier,
            writeLimit: step.captureContext?.writeLimit ?? TransportWriteLimitBytes(0)
        ))
        executeAndRecord(step.operations)
        scheduleRetryIfNeeded(at: monotonicMilliseconds)
        return step
    }

    public func recordInventory(_ inventory: CoreBluetoothGattInventory) {
        record(.gattInventory(
            platformIdentifier: platformIdentifier,
            inventory: inventory
        ))
    }

    @discardableResult
    public func handleCommand(
        _ command: DeviceCommand,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> CoreBluetoothSessionStep {
        let step = try runner.handle(.command(command, at: monotonicMilliseconds))
        executeAndRecord(step.operations)
        return step
    }

    @discardableResult
    public func handleNotification(
        bytes: Data,
        channel: BluetoothUuid,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) throws -> CoreBluetoothSessionStep {
        record(.notification(
            channel: channel,
            byteCount: CoreBluetoothPayloadByteCount(bytes.count),
            at: monotonicMilliseconds
        ))
        let step = try runner.handle(.notification(
            bytes: bytes,
            channel: channel,
            at: monotonicMilliseconds
        ))
        receivedRealtimeTelemetrySinceLinkUp =
            receivedRealtimeTelemetrySinceLinkUp
            || step.snapshot?.pitch != nil
            || step.snapshot?.roll != nil
            || step.snapshot?.footpad != nil
        if receivedRealtimeTelemetrySinceLinkUp {
            cancelPendingRetry()
        }
        executeAndRecord(step.operations)
        return step
    }

    @discardableResult
    public func handleLinkDown(at monotonicMilliseconds: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        cancelPendingRetry()
        pendingOperationsAfterSubscription.removeAll()
        waitingForSubscriptionChannel = nil
        let step = try runner.handle(.linkDown(at: monotonicMilliseconds))
        record(.linkDown(
            platformIdentifier: platformIdentifier,
            at: monotonicMilliseconds
        ))
        executeAndRecord(step.operations)
        return step
    }

    /// Completes link-up operations after CoreBluetooth confirms notifications are enabled.
    public func handleNotificationStateUpdate(
        channel: BluetoothUuid,
        isNotifying: Bool,
        error: Error?
    ) {
        guard waitingForSubscriptionChannel == channel else { return }
        waitingForSubscriptionChannel = nil
        guard error == nil, isNotifying else {
            pendingOperationsAfterSubscription.removeAll()
            return
        }
        let pending = pendingOperationsAfterSubscription
        pendingOperationsAfterSubscription.removeAll()
        executeAndRecord(pending)
    }

    private func executeAndRecord(_ operations: [CoreBluetoothPlannedOperation]) {
        if waitingForSubscriptionChannel != nil {
            pendingOperationsAfterSubscription.append(contentsOf: operations)
            return
        }

        var deferAfterSubscription = false
        operations.forEach { operation in
            if deferAfterSubscription {
                pendingOperationsAfterSubscription.append(operation)
                return
            }
            executor.execute(operation)
            record(.operation(
                platformIdentifier: platformIdentifier,
                operation: operation
            ))
            if case let .subscribe(channel) = operation {
                waitingForSubscriptionChannel = channel
                deferAfterSubscription = true
            }
        }
    }

    private func record(_ value: CoreBluetoothLiveRecord) {
        guard recorded.count < 2_048 else { return }
        recorded.append(value)
    }

    private func scheduleRetryIfNeeded(at monotonicMilliseconds: MonotonicMilliseconds) {
        guard let retryCommandOnLinkUp else {
            return
        }
        pendingRetryTimestamp = monotonicMilliseconds
        let retry = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            let runRetry = {
                self.runRetryCommandIfNeeded(
                    retryCommandOnLinkUp,
                    at: monotonicMilliseconds
                )
            }
            if let executionQueue = self.executionQueue {
                executionQueue.async(execute: DispatchWorkItem(block: runRetry))
            } else {
                runRetry()
            }
        }
        pendingRetry = retry
        DispatchQueue.main.asyncAfter(deadline: .now() + retryDelay, execute: retry)
    }

    private func runRetryCommandIfNeeded(
        _ retryCommandOnLinkUp: DeviceCommand,
        at monotonicMilliseconds: MonotonicMilliseconds
    ) {
        guard pendingRetryTimestamp == monotonicMilliseconds else {
            return
        }
        pendingRetry = nil
        pendingRetryTimestamp = nil
        do {
            _ = try handleCommand(retryCommandOnLinkUp, at: monotonicMilliseconds)
        } catch {
            scheduleRetryIfNeeded(at: monotonicMilliseconds)
            return
        }
        guard !receivedRealtimeTelemetrySinceLinkUp else {
            return
        }
        scheduleRetryIfNeeded(at: monotonicMilliseconds)
    }

    private func cancelPendingRetry() {
        pendingRetry?.cancel()
        pendingRetry = nil
        pendingRetryTimestamp = nil
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
    init(peripheral: CBPeripheral, advertisementData: [String: Any], rssi: NSNumber? = nil) {
        let localName = advertisementData[CBAdvertisementDataLocalNameKey] as? String
        let serviceUuids = (
            advertisementData[CBAdvertisementDataServiceUUIDsKey] as? [CBUUID] ?? []
        ).compactMap(BluetoothUuid.init(coreBluetoothUuid:))
        let manufacturerData = (advertisementData[CBAdvertisementDataManufacturerDataKey] as? Data)
            .flatMap(CoreBluetoothManufacturerDataSummary.init(advertisementData:))
            .map { [$0] } ?? []
        self.init(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier(peripheral.identifier.uuidString),
            localName: localName ?? peripheral.name,
            advertisedServiceUuids: serviceUuids,
            manufacturerData: manufacturerData,
            rssiDbm: rssi.map(Int16.init(truncating:))
        )
    }

    init(discoveryObservation observation: DiscoveryObservationSnapshot) {
        self.init(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier(observation.platformIdentifier),
            localName: observation.advertisedNameText,
            advertisedServiceUuids: observation.advertisedServiceUuids.map(BluetoothUuid.bluetooth16),
            manufacturerData: observation.manufacturerData.map(CoreBluetoothManufacturerDataSummary.init),
            rssiDbm: observation.rssiDbm
        )
    }
}

public extension DiscoveryObservation {
    init(_ advertisement: CoreBluetoothAdvertisement) {
        self.init(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            advertisedName: advertisement.localName.map { Data($0.utf8) },
            advertisedServiceUuids: advertisement.advertisedServiceUuids.compactMap(\.bluetooth16Value),
            manufacturerData: advertisement.manufacturerData.map(DiscoveryManufacturerDataSummary.init),
            rssiDbm: advertisement.rssiDbm
        )
    }
}

public extension CoreBluetoothManufacturerDataSummary {
    init(_ summary: DiscoveryManufacturerDataSummary) {
        self.init(companyIdentifier: summary.companyIdentifier, payloadLength: summary.payloadLen)
    }
}

public extension DiscoveryManufacturerDataSummary {
    init(_ summary: CoreBluetoothManufacturerDataSummary) {
        self.init(companyIdentifier: summary.companyIdentifier, payloadLen: summary.payloadLength)
    }
}

private extension CoreBluetoothManufacturerDataSummary {
    init?(advertisementData: Data) {
        if advertisementData.count >= 2 {
            let companyIdentifier = UInt16(advertisementData[0]) | (UInt16(advertisementData[1]) << 8)
            self.init(
                companyIdentifier: companyIdentifier,
                payloadLength: UInt64(advertisementData.count.saturatingSubtracting(2))
            )
        } else {
            return nil
        }
    }
}

private extension Int {
    func saturatingSubtracting(_ rhs: Int) -> Int {
        Swift.max(0, self - rhs)
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
