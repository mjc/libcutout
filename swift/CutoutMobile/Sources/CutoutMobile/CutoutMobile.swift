import CutoutMobileFFI

public typealias Angle = CutoutMobileFFI.Angle
public typealias BatteryCurrent = CutoutMobileFFI.BatteryCurrent
public typealias BatteryLevel = CutoutMobileFFI.BatteryLevel
public typealias MobilePhoneLocationSnapshotDto = CutoutMobileFFI.MobilePhoneLocationSnapshotDto
public typealias PhaseCurrent = CutoutMobileFFI.PhaseCurrent
public typealias PowerFlowDirection = CutoutMobileFFI.PowerFlowDirection
public typealias Speed = CutoutMobileFFI.Speed
public typealias Temperature = CutoutMobileFFI.Temperature
public typealias Voltage = CutoutMobileFFI.Voltage
public typealias VoltageDelta = CutoutMobileFFI.VoltageDelta
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

public enum ChargeMode: Equatable, Hashable, Sendable {
    case charging
    case notCharging

    fileprivate init(_ dto: MobileChargeModeDto) {
        switch dto {
        case .charging:
            self = .charging
        case .notCharging:
            self = .notCharging
        }
    }
}

public enum ChargeEstimateConfidence: Equatable, Hashable, Sendable {
    case low
    case medium
    case high

    fileprivate init(_ dto: MobileEstimateConfidenceDto) {
        switch dto {
        case .low:
            self = .low
        case .medium:
            self = .medium
        case .high:
            self = .high
        }
    }
}

public enum ChargeEstimateKind: Equatable, Hashable, Sendable {
    case atPresentCurrent
    case profileBackedTimeToFull
    case observedTaperTimeToFull

    fileprivate init(_ dto: MobileEstimateKindDto) {
        switch dto {
        case .atPresentCurrent:
            self = .atPresentCurrent
        case .profileBackedTimeToFull:
            self = .profileBackedTimeToFull
        case .observedTaperTimeToFull:
            self = .observedTaperTimeToFull
        }
    }
}

public enum ChargeEstimateUnavailableReason: Equatable, Hashable, Sendable {
    case notCharging
    case currentMissing
    case currentDirectionUnverified
    case currentTooSmall
    case batteryLevelMissing
    case capacityMissing
    case unsupportedProfile
    case unstableCurrent
    case staleInput
    case temperatureOutOfModel
    case fullOrNearFull
    case contradictoryInputs

    fileprivate init(_ dto: MobileChargeEstimateUnavailableReasonDto) {
        switch dto {
        case .notCharging: self = .notCharging
        case .currentMissing: self = .currentMissing
        case .currentDirectionUnverified: self = .currentDirectionUnverified
        case .currentTooSmall: self = .currentTooSmall
        case .batteryLevelMissing: self = .batteryLevelMissing
        case .capacityMissing: self = .capacityMissing
        case .unsupportedProfile: self = .unsupportedProfile
        case .unstableCurrent: self = .unstableCurrent
        case .staleInput: self = .staleInput
        case .temperatureOutOfModel: self = .temperatureOutOfModel
        case .fullOrNearFull: self = .fullOrNearFull
        case .contradictoryInputs: self = .contradictoryInputs
        }
    }

    fileprivate var displayText: String {
        switch self {
        case .notCharging: pevLocalizedText("charge.estimate.unavailable.not_charging")
        case .currentMissing: pevLocalizedText("charge.estimate.unavailable.current_missing")
        case .currentDirectionUnverified: pevLocalizedText("charge.estimate.unavailable.current_direction_unverified")
        case .currentTooSmall: pevLocalizedText("charge.estimate.unavailable.current_too_small")
        case .batteryLevelMissing: pevLocalizedText("charge.estimate.unavailable.battery_level_missing")
        case .capacityMissing: pevLocalizedText("charge.estimate.unavailable.capacity_missing")
        case .unsupportedProfile: pevLocalizedText("charge.estimate.unavailable.unsupported_profile")
        case .unstableCurrent: pevLocalizedText("charge.estimate.unavailable.unstable_current")
        case .staleInput: pevLocalizedText("charge.estimate.unavailable.stale_input")
        case .temperatureOutOfModel: pevLocalizedText("charge.estimate.unavailable.temperature_out_of_model")
        case .fullOrNearFull: pevLocalizedText("charge.estimate.unavailable.full_or_near_full")
        case .contradictoryInputs: pevLocalizedText("charge.estimate.unavailable.contradictory_inputs")
        }
    }
}

public enum ChargeEstimateStateKind: Equatable, Hashable, Sendable {
    case collectingSamples
    case available
    case unavailable
    case stale
    case failed

    fileprivate init(_ dto: MobileChargeEstimateStateKindDto) {
        switch dto {
        case .collectingSamples: self = .collectingSamples
        case .available: self = .available
        case .unavailable: self = .unavailable
        case .stale: self = .stale
        case .failed: self = .failed
        }
    }

    public func dashboardMetricValue(display: String) -> PevDashboardMetricValue {
        switch self {
        case .available:
            .available(display: display, accessibility: display)
        case .collectingSamples, .stale, .unavailable, .failed:
            .status(display: display, accessibility: display)
        }
    }
}

public enum BatteryLevelBasis: Equatable, Hashable, Sendable {
    case reported
    case profileEstimated

    fileprivate init(_ dto: MobileBatteryLevelBasisDto) {
        switch dto {
        case .reported: self = .reported
        case .profileEstimated: self = .profileEstimated
        }
    }
}

public enum ChargeCapacitySource: Equatable, Hashable, Sendable {
    case protocolProfile
    case hardwareMeasured
    case estimated

    fileprivate init(_ dto: MobileChargeCapacitySourceDto) {
        switch dto {
        case .protocolProfile: self = .protocolProfile
        case .hardwareMeasured: self = .hardwareMeasured
        case .estimated: self = .estimated
        }
    }

    fileprivate var dto: MobileChargeCapacitySourceDto {
        switch self {
        case .protocolProfile: .protocolProfile
        case .hardwareMeasured: .hardwareMeasured
        case .estimated: .estimated
        }
    }
}

public enum ChargeEstimateError: Equatable, Hashable, Sendable {
    case timestampOrder
    case arithmeticOverflow

    fileprivate init(_ dto: MobileChargeEstimateErrorDto) {
        switch dto {
        case .timestampOrder: self = .timestampOrder
        case .arithmeticOverflow: self = .arithmeticOverflow
        }
    }
}

public enum ChargeEstimateResetReason: Equatable, Hashable, Sendable {
    case sessionChanged
    case chargingStopped
    case staleGap
    case timestampOrder
    case currentEvidenceChanged
    case capacityChanged
    case profileChanged
    case manual

    fileprivate init(_ dto: MobileChargeEstimateResetReasonDto) {
        switch dto {
        case .sessionChanged: self = .sessionChanged
        case .chargingStopped: self = .chargingStopped
        case .staleGap: self = .staleGap
        case .timestampOrder: self = .timestampOrder
        case .currentEvidenceChanged: self = .currentEvidenceChanged
        case .capacityChanged: self = .capacityChanged
        case .profileChanged: self = .profileChanged
        case .manual: self = .manual
        }
    }
}

public struct ChargeEstimateDuration: Equatable, Hashable, Sendable {
    public let milliseconds: UInt64

    fileprivate init(_ dto: MobileDurationDto) {
        self.milliseconds = dto.milliseconds
    }

    fileprivate var displayText: String {
        let minutes = milliseconds / 60_000
        if minutes == 0 { return pevLocalizedText("charge.estimate.duration.under_minute") }
        let hours = minutes / 60
        return hours == 0
            ? pevLocalizedText("charge.estimate.duration.minutes", Int64(minutes))
            : pevLocalizedText("charge.estimate.duration.hours_minutes", Int64(hours), Int64(minutes % 60))
    }
}

public struct ChargeCurrentRateSummary: Equatable, Hashable, Sendable {
    public let meanMilliamps: Int32
    public let minimumMilliamps: Int32
    public let maximumMilliamps: Int32
    public let variabilityPermille: UInt16

    fileprivate init(_ dto: MobileCurrentRateSummaryDto) {
        self.meanMilliamps = dto.meanMilliamps
        self.minimumMilliamps = dto.minimumMilliamps
        self.maximumMilliamps = dto.maximumMilliamps
        self.variabilityPermille = dto.variabilityPermille
    }
}

public struct ChargeVoltageSagEstimate: Equatable, Hashable, Sendable {
    public let deltaMillivolts: Int32
    public let loadCurrent: BatteryCurrent
    public let loadCurrentSource: ReadbackSource
    public let loadCurrentQuality: ReadbackQuality
    public let loadCurrentVerification: VerificationState
    public let effectiveResistanceMilliohms: UInt32
    public let observations: UInt16
    public let confidence: ChargeEstimateConfidence
    public let calculatedAt: MonotonicMilliseconds
    public let validUntil: MonotonicMilliseconds

    fileprivate init(_ dto: MobileVoltageSagEstimateDto) {
        self.deltaMillivolts = dto.deltaMillivolts
        self.loadCurrent = dto.loadCurrent.value
        self.loadCurrentSource = ReadbackSource(dto.loadCurrent.source)
        self.loadCurrentQuality = ReadbackQuality(dto.loadCurrent.quality)
        self.loadCurrentVerification = VerificationState(dto.loadCurrent.verification)
        self.effectiveResistanceMilliohms = dto.effectiveResistanceMilliohms
        self.observations = dto.observations
        self.confidence = ChargeEstimateConfidence(dto.confidence)
        self.calculatedAt = MonotonicMilliseconds(dto.calculatedAt.milliseconds)
        self.validUntil = MonotonicMilliseconds(dto.validUntil.milliseconds)
    }
}

public struct ChargeTimeEstimate: Equatable, Hashable, Sendable {
    public let lower: ChargeEstimateDuration
    public let expected: ChargeEstimateDuration
    public let upper: ChargeEstimateDuration
    public let kind: ChargeEstimateKind
    public let confidence: ChargeEstimateConfidence
    public let currentRate: ChargeCurrentRateSummary
    public let batteryLevel: BatteryLevel
    public let batteryLevelBasis: BatteryLevelBasis
    public let batteryProfileID: UInt32?
    public let capacitySource: ChargeCapacitySource
    public let voltageSag: ChargeVoltageSagEstimate?
    public let calculatedAt: MonotonicMilliseconds
    public let validUntil: MonotonicMilliseconds

    fileprivate init(_ dto: MobileChargeTimeEstimateDto) {
        self.lower = ChargeEstimateDuration(dto.lower)
        self.expected = ChargeEstimateDuration(dto.expected)
        self.upper = ChargeEstimateDuration(dto.upper)
        self.kind = ChargeEstimateKind(dto.kind)
        self.confidence = ChargeEstimateConfidence(dto.confidence)
        self.currentRate = ChargeCurrentRateSummary(dto.currentRate)
        self.batteryLevel = dto.batteryLevel.value
        self.batteryLevelBasis = BatteryLevelBasis(dto.batteryLevelBasis)
        self.batteryProfileID = dto.batteryProfileId
        self.capacitySource = ChargeCapacitySource(dto.capacitySource)
        self.voltageSag = dto.voltageSag.map(ChargeVoltageSagEstimate.init)
        self.calculatedAt = MonotonicMilliseconds(dto.calculatedAt.milliseconds)
        self.validUntil = MonotonicMilliseconds(dto.validUntil.milliseconds)
    }
}

public struct ChargeEstimateState: Equatable, Hashable, Sendable {
    public let kind: ChargeEstimateStateKind
    public let estimate: ChargeTimeEstimate?
    public let voltageSag: ChargeVoltageSagEstimate?
    public let unavailableReason: ChargeEstimateUnavailableReason?
    public let error: ChargeEstimateError?
    public let resetReason: ChargeEstimateResetReason?
    public let samples: UInt16
    public let observedFor: ChargeEstimateDuration

    init(_ dto: MobileChargeEstimateStateDto) {
        self.kind = ChargeEstimateStateKind(dto.kind)
        self.estimate = dto.estimate.map(ChargeTimeEstimate.init)
        self.voltageSag = dto.voltageSag.map(ChargeVoltageSagEstimate.init)
        self.unavailableReason = dto.unavailableReason.map(ChargeEstimateUnavailableReason.init)
        self.error = dto.error.map(ChargeEstimateError.init)
        self.resetReason = dto.resetReason.map(ChargeEstimateResetReason.init)
        self.samples = dto.samples
        self.observedFor = ChargeEstimateDuration(dto.observedFor)
    }

    fileprivate static var missingProfile: Self {
        Self(MobileChargeEstimateStateDto(
            kind: .unavailable,
            estimate: nil,
            voltageSag: nil,
            unavailableReason: .capacityMissing,
            error: nil,
            resetReason: nil,
            samples: 0,
            observedFor: MobileDurationDto(milliseconds: 0)
        ))
    }

    public var displayValue: String {
        switch kind {
        case .available:
            estimate?.expected.displayText ?? pevLocalizedText("metric.availability.unavailable")
        case .collectingSamples:
            pevLocalizedText("charge.estimate.value.collecting")
        case .stale:
            pevLocalizedText("charge.estimate.value.stale")
        case .unavailable:
            unavailableReason == .fullOrNearFull
                ? pevLocalizedText("charge.estimate.value.near_full")
                : pevLocalizedText("metric.availability.unavailable")
        case .failed:
            pevLocalizedText("charge.estimate.value.failed")
        }
    }

    public var displayDetail: String {
        switch kind {
        case .available:
            guard let estimate else { return pevLocalizedText("charge.estimate.detail.unavailable") }
            return pevLocalizedText(
                "charge.estimate.detail.available",
                estimate.kind.displayText,
                estimate.confidence.displayText
            )
        case .collectingSamples:
            return samples == 1
                ? pevLocalizedText("charge.estimate.detail.collecting.singular")
                : pevLocalizedText("charge.estimate.detail.collecting.plural", Int64(samples))
        case .stale:
            return pevLocalizedText("charge.estimate.detail.stale")
        case .unavailable:
            return unavailableReason?.displayText ?? pevLocalizedText("charge.estimate.detail.unavailable")
        case .failed:
            return pevLocalizedText("charge.estimate.detail.failed")
        }
    }
}

public struct ChargeEstimateProfile: Equatable, Hashable, Sendable {
    public let sessionID: UInt64
    public let profileID: UInt32
    public let capacityMilliampHours: UInt32
    public let capacitySource: ChargeCapacitySource
    public let verification: VerificationState
    /// Independent charge-flow/polarity evidence; keep unverified until LIBCU-521 closes.
    public let chargeFlowVerification: VerificationState

    public init(
        sessionID: UInt64,
        profileID: UInt32,
        capacityMilliampHours: UInt32,
        capacitySource: ChargeCapacitySource,
        verification: VerificationState,
        chargeFlowVerification: VerificationState
    ) {
        self.sessionID = sessionID
        self.profileID = profileID
        self.capacityMilliampHours = capacityMilliampHours
        self.capacitySource = capacitySource
        self.verification = verification
        self.chargeFlowVerification = chargeFlowVerification
    }

    fileprivate var dto: MobileChargeProfileDto {
        MobileChargeProfileDto(
            sessionId: sessionID,
            profileId: profileID,
            capacityMilliampHours: capacityMilliampHours,
            capacitySource: capacitySource.dto,
            verification: verification.dto,
            chargeFlowVerification: chargeFlowVerification.dto
        )
    }
}

private extension ChargeEstimateKind {
    var displayText: String {
        switch self {
        case .atPresentCurrent: pevLocalizedText("charge.estimate.kind.present_current")
        case .profileBackedTimeToFull: pevLocalizedText("charge.estimate.kind.profile_backed")
        case .observedTaperTimeToFull: pevLocalizedText("charge.estimate.kind.observed_taper")
        }
    }
}

private extension ChargeEstimateConfidence {
    var displayText: String {
        switch self {
        case .low: pevLocalizedText("charge.estimate.confidence.low")
        case .medium: pevLocalizedText("charge.estimate.confidence.medium")
        case .high: pevLocalizedText("charge.estimate.confidence.high")
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

public enum TelemetryPowerPresentation: Equatable, Hashable, Sendable {
    case calculatedPackCurrent(Power)
    case reported(Power)
    case unavailable

    public var power: Power? {
        switch self {
        case let .calculatedPackCurrent(power), let .reported(power):
            power
        case .unavailable:
            nil
        }
    }

    public var metricValue: PevDashboardMetricValue {
        guard let power else { return .unavailable }
        let text = RideUnits.powerText(
            milliwatts: power.value,
            fractionDigits: telemetryPowerFractionDigits(fromMilliwatts: power.value)
        )
        return .available(display: text, accessibility: text)
    }
}

private func telemetryPowerFractionDigits<T: BinaryInteger>(fromMilliwatts value: T) -> Int {
    abs(Int64(value)) < 1_000_000 ? 2 : 1
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
    public let chargeMode: ChargeMode?
    public let chargeEstimate: ChargeEstimateState?

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
        batteryLevelEstimated: BatteryLevel? = nil,
        chargeMode: ChargeMode? = nil,
        chargeEstimate: ChargeEstimateState? = nil
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
        self.chargeMode = chargeMode
        self.chargeEstimate = chargeEstimate
    }

    fileprivate init(_ dto: MobileTelemetrySnapshotDto) {
        self.init(dto, chargeEstimate: nil)
    }

    fileprivate init(_ dto: MobileTelemetrySnapshotDto, chargeEstimate: ChargeEstimateState?) {
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
            voltageSag: chargeEstimate?.voltageSag.map {
                VoltageDelta(value: $0.deltaMillivolts)
            } ?? dto.voltageSag?.value,
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
            batteryLevelEstimated: dto.batteryLevelEstimated?.value,
            chargeMode: dto.chargeMode.map { ChargeMode($0.value) },
            chargeEstimate: chargeEstimate
        )
    }

    public var packVoltageMetricValue: PevDashboardMetricValue {
        guard let voltage else { return .unavailable }
        let text = RideUnits.voltageText(millivolts: voltage.value, fractionDigits: 1)
        return .available(display: text, accessibility: text)
    }

    public var thermalMetricValue: PevDashboardMetricValue {
        let temperatures = [controllerTemperature, motorTemperature, batteryTemperature]
            .compactMap { $0?.value }
        guard let maximum = temperatures.max() else { return .unavailable }
        let text = RideUnits.temperatureText(millicelsius: maximum, fractionDigits: 0)
        return .available(display: text, accessibility: text)
    }

    public var powerPresentation: TelemetryPowerPresentation {
        if let voltage, let batteryCurrent, batteryCurrent.value != 0 {
            return .calculatedPackCurrent(
                Power(value: Int64(voltage.value) * Int64(batteryCurrent.value) / 1_000)
            )
        }
        if let power {
            return .reported(power)
        }
        return .unavailable
    }
}

public enum FootpadContactState: Equatable, Hashable, Sendable {
    case none
    case left
    case right
    case both

    fileprivate init(_ state: MobileFootpadContactState) {
        switch state {
        case .none:
            self = .none
        case .left:
            self = .left
        case .right:
            self = .right
        case .both:
            self = .both
        }
    }

    fileprivate var displayText: String {
        switch self {
        case .none:
            pevLocalizedText("footpad.contact.none")
        case .left:
            pevLocalizedText("footpad.contact.left")
        case .right:
            pevLocalizedText("footpad.contact.right")
        case .both:
            pevLocalizedText("footpad.contact.both")
        }
    }
}

public struct FootpadTelemetry: Equatable, Hashable, Sendable {
    public let state: UInt8
    public let contactState: FootpadContactState?
    public let adc1Milliunits: Int32?
    public let adc2Milliunits: Int32?

    public init(
        state: UInt8,
        contactState: FootpadContactState? = nil,
        adc1Milliunits: Int32? = nil,
        adc2Milliunits: Int32? = nil
    ) {
        self.state = state
        self.contactState = contactState
        self.adc1Milliunits = adc1Milliunits
        self.adc2Milliunits = adc2Milliunits
    }

    fileprivate init(_ dto: MobileFootpadTelemetryDto) {
        self.init(
            state: dto.state,
            contactState: dto.contactState.map(FootpadContactState.init),
            adc1Milliunits: dto.adc1Milliunits,
            adc2Milliunits: dto.adc2Milliunits
        )
    }
}

public extension FootpadTelemetry {
    var adc1MetricValue: PevDashboardMetricValue {
        footpadMetricValue(adc1Milliunits)
    }

    var adc2MetricValue: PevDashboardMetricValue {
        footpadMetricValue(adc2Milliunits)
    }

    var stateDisplayText: String {
        contactState?.displayText ?? pevLocalizedText("footpad.state", Int64(state))
    }

    var accessibilityValue: String {
        pevLocalizedText(
            "footpad.accessibility.summary",
            pevLocalizedText("footpad.adc1"),
            adc1MetricValue.accessibilityText,
            pevLocalizedText("footpad.adc2"),
            adc2MetricValue.accessibilityText,
            stateDisplayText
        )
    }

    var summaryText: String {
        pevLocalizedText(
            "footpad.summary",
            stateDisplayText,
            footpadSummaryText(adc1MetricValue),
            footpadSummaryText(adc2MetricValue)
        )
    }
}

private func footpadSummaryText(_ metricValue: PevDashboardMetricValue) -> String {
    guard case .unavailable = metricValue else { return metricValue.displayText }
    return pevLocalizedText("footpad.unavailable")
}

private func footpadMetricValue(_ value: Int32?) -> PevDashboardMetricValue {
    guard let value else { return .unavailable }
    let displayText = RideUnits.decimalString(Double(value) / 1_000, fractionDigits: 2)
    return .available(
        display: displayText,
        accessibility: pevLocalizedText("footpad.available", displayText)
    )
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

public enum VescBatteryDetail: Equatable, Hashable, Sendable {
    case reported(level: BatteryLevel, current: BatteryCurrent?)
    case estimated(level: BatteryLevel, current: BatteryCurrent?)
    case unavailable(current: BatteryCurrent?)
}

public enum VescMotorCurrentDetail: Equatable, Hashable, Sendable {
    case available(powerFlow: PowerFlowDirection?)
    case unavailable
}

public enum VescBoardOrientation: Equatable, Hashable, Sendable {
    case noseDown
    case level
    case noseUp
}

public enum VescBoardAngleDetail: Equatable, Hashable, Sendable {
    case available(orientation: VescBoardOrientation, balanceAngle: Angle?)
    case unavailable
}

public enum VescControllerTemperatureDetail: Equatable, Hashable, Sendable {
    case available(motorTemperature: Temperature)
    case unavailable
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

    public var batteryVoltageMetricValue: PevDashboardMetricValue {
        vescMetricValue(batteryVoltage) {
            RideUnits.voltageText(millivolts: $0.value)
        }
    }

    public var motorCurrentMetricValue: PevDashboardMetricValue {
        vescMetricValue(motorCurrent) {
            RideUnits.currentText(milliamps: $0.value)
        }
    }

    public var batteryCurrentMetricValue: PevDashboardMetricValue {
        vescMetricValue(batteryCurrent) {
            RideUnits.currentText(milliamps: $0.value)
        }
    }

    public var boardAngleMetricValue: PevDashboardMetricValue {
        vescMetricValue(boardAngle) {
            RideUnits.angleText(millidegrees: $0.value)
        }
    }

    public var controllerTemperatureMetricValue: PevDashboardMetricValue {
        vescMetricValue(controllerTemperature) {
            RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 1)
        }
    }

    public var dutyCycleMetricValue: PevDashboardMetricValue {
        vescMetricValue(dutyCycle) {
            RideUnits.percentText(abs(Int($0.permille)) / 10)
        }
    }

    public var dutyHeadroomMetricValue: PevDashboardMetricValue {
        switch dutyHeadroomApplicability {
        case .available:
            guard let dutyHeadroom else {
                return .unavailable
            }

            let value = RideUnits.percentText(dutyHeadroom.value)
            return .available(display: value, accessibility: value)
        case .notApplicable:
            let value = pevLocalizedText("metric.availability.not_applicable")
            return .status(display: value, accessibility: value)
        case .unavailable:
            return .unavailable
        }
    }

    public var dutyHeadroomProgress: Double? {
        dutyHeadroom.map { Double($0.value) / 100.0 }
    }

    public var dutyHeadroomProgressMetricValue: PevDashboardMetricValue {
        switch dutyHeadroomMetricValue {
        case .available(let display, let accessibility):
            .available(display: display + "%", accessibility: accessibility + "%")
        case .status, .unavailable:
            dutyHeadroomMetricValue
        }
    }

    public var balanceAngleMetricValue: PevDashboardMetricValue {
        vescMetricValue(balanceAngle) {
            RideUnits.angleText(millidegrees: $0.value)
        }
    }

    public var motorTemperatureMetricValue: PevDashboardMetricValue {
        vescMetricValue(motorTemperature) {
            RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 1)
        }
    }

    public var footpadMetricValue: PevDashboardMetricValue {
        vescMetricValue(footpad, text: \.stateDisplayText)
    }

    public var batteryDetail: VescBatteryDetail {
        if let batteryLevelReported {
            return .reported(level: batteryLevelReported, current: batteryCurrent)
        }
        if let batteryLevelEstimated {
            return .estimated(level: batteryLevelEstimated, current: batteryCurrent)
        }
        return .unavailable(current: batteryCurrent)
    }

    public var motorCurrentDetail: VescMotorCurrentDetail {
        motorCurrent == nil ? .unavailable : .available(powerFlow: powerFlow)
    }

    public var boardAngleDetail: VescBoardAngleDetail {
        guard let boardAngle else { return .unavailable }
        let orientation: VescBoardOrientation
        if boardAngle.value < 0 {
            orientation = .noseDown
        } else if boardAngle.value > 0 {
            orientation = .noseUp
        } else {
            orientation = .level
        }
        return .available(orientation: orientation, balanceAngle: balanceAngle)
    }

    public var controllerTemperatureDetail: VescControllerTemperatureDetail {
        motorTemperature.map(VescControllerTemperatureDetail.available) ?? .unavailable
    }
}

private func vescMetricValue<Value>(
    _ value: Value?,
    text: (Value) -> String
) -> PevDashboardMetricValue {
    guard let value else { return .unavailable }
    let text = text(value)
    return .available(display: text, accessibility: text)
}

public extension VescRideSnapshot {
    func updateAge(
        at now: MonotonicMilliseconds,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> EucRideUpdateAge {
        rideUpdateAge(updatedAt: lastUpdate, at: now, staleAfter: staleThreshold)
    }

    func staleTelemetryElapsed(
        at now: MonotonicMilliseconds,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> MonotonicMilliseconds? {
        let age = updateAge(at: now, staleAfter: staleThreshold)
        guard age.freshness == .stale else { return nil }
        return age.elapsed
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
    public let receivedAt: MonotonicMilliseconds?

    public init(
        snapshot: MobilePhoneLocationSnapshotDto,
        receivedAt: MonotonicMilliseconds? = nil
    ) {
        self.speed = SpeedReadout(millimetersPerSecond: snapshot.gpsSpeed?.value.value)
        self.receivedAt = speed.millimetersPerSecond == nil || snapshot.latestSample == nil
            ? nil
            : receivedAt
    }

    public func freshness(
        at now: MonotonicMilliseconds,
        staleAfter: MonotonicMilliseconds = RideTelemetryFreshnessPolicy.staleAfter
    ) -> PhoneLocationFreshness {
        guard speed.millimetersPerSecond != nil, let receivedAt else {
            return .unavailable
        }
        let elapsed = now.rawValue >= receivedAt.rawValue ? now.rawValue - receivedAt.rawValue : 0
        return elapsed <= staleAfter.rawValue ? .fresh : .stale
    }

    public func detail(at now: MonotonicMilliseconds) -> String {
        switch freshness(at: now) {
        case .unavailable:
            pevLocalizedText("gps.detail.unavailable")
        case .fresh:
            pevLocalizedText("gps.detail.fresh")
        case .stale:
            pevLocalizedText("gps.detail.stale")
        }
    }

    public var speedMetricValue: PevDashboardMetricValue {
        guard let speed = speed.millimetersPerSecond else { return .unavailable }
        let text = RideUnits.speedText(millimetersPerSecond: speed)
        return .available(display: text, accessibility: text)
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

public extension BmsGroupSnapshot {
    var resistanceMetricValue: PevDashboardMetricValue {
        guard let resistance else { return .unavailable }
        let text = RideUnits.decimalString(Double(resistance.value), fractionDigits: 0)
        return .available(display: text, accessibility: text)
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

public enum BmsNoDataTextRow: Hashable, Sendable, Identifiable {
    case cellBalanceWarning
    case bmsDiagnosticsWarning
    case cellVoltages
    case weakGroups
    case bmsDiagnostics

    public var id: Self { self }

    public var text: String {
        switch self {
        case .cellBalanceWarning:
            pevLocalizedText("bms.no_data.row.cell_balance_warning")
        case .bmsDiagnosticsWarning:
            pevLocalizedText("bms.no_data.row.diagnostics_warning")
        case .cellVoltages:
            pevLocalizedText("bms.no_data.row.cell_voltages")
        case .weakGroups:
            pevLocalizedText("bms.no_data.row.weak_groups")
        case .bmsDiagnostics:
            pevLocalizedText("bms.no_data.row.diagnostics")
        }
    }
}

public struct BmsOverviewPresentation: Equatable, Hashable, Sendable {
    public let averageGroupVoltage: Voltage?
    public let lowestGroupVoltage: Voltage?
    public let lowestGroupLabel: String
    public let highestTemperature: Temperature?
    public let highestTemperatureLabel: String
}

public struct BmsUnknownTopologyCapturePresentation: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String
    public let state: String
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

    public var energyProgress: Double? {
        energyPercent.map { min(max(Double($0.value) / 100, 0), 1) }
    }

    public var energyMetricValue: PevDashboardMetricValue {
        bmsMetricValue(energyPercent) {
            RideUnits.percentText($0.value) + "%"
        }
    }

    public func noDataPackEstimateMetricValue(
        controllerEstimatePercent: BatteryLevel?
    ) -> PevDashboardMetricValue {
        bmsMetricValue(controllerEstimatePercent ?? energyPercent) {
            RideUnits.decimalString(Double($0.value), fractionDigits: 0)
        }
    }

    public var captureActionMetricValue: PevDashboardMetricValue {
        bmsMetricValue(captureActionTitle) { $0 }
    }

    public var voltageMetricValue: PevDashboardMetricValue {
        bmsMetricValue(voltage) {
            RideUnits.voltageText(millivolts: $0.value)
        }
    }

    public var readbackRows: [SessionDebugRow] {
        var rows = [
            SessionDebugRow(
                id: "availability",
                label: "availability",
                metricValue: .status(
                    display: availability.displayText,
                    accessibility: availability.displayText
                )
            ),
            SessionDebugRow(
                id: "page",
                label: "page",
                metricValue: bmsPageMetricValue(
                    selector: pageSelector,
                    tag: pageTag,
                    kind: pageKind
                ),
                role: .transportMetadata
            ),
            SessionDebugRow(
                id: "page-verification",
                label: "page verification",
                metricValue: pageVerification.map { verification in
                    .status(
                        display: verification.displayText,
                        accessibility: verification.displayText
                    )
                } ?? .unavailable,
                role: .transportMetadata
            ),
            SessionDebugRow(
                id: "charge",
                label: "charge",
                metricValue: energyMetricValue
            ),
            SessionDebugRow(
                id: "voltage",
                label: "voltage",
                metricValue: voltageMetricValue
            ),
            SessionDebugRow(
                id: "current",
                label: "current",
                metricValue: bmsMetricValue(current) {
                    RideUnits.currentText(milliamps: $0.value)
                }
            ),
        ]
        if let bmsPackCurrent0 {
            rows.append(
                SessionDebugRow(
                    id: "bms-current-0",
                    label: "bms current 0",
                    metricValue: bmsMetricValue(bmsPackCurrent0) {
                        RideUnits.currentText(milliamps: $0.value)
                    }
                )
            )
        }
        if let bmsPackCurrent1 {
            rows.append(
                SessionDebugRow(
                    id: "bms-current-1",
                    label: "bms current 1",
                    metricValue: bmsMetricValue(bmsPackCurrent1) {
                        RideUnits.currentText(milliamps: $0.value)
                    }
                )
            )
        }
        rows += [
            SessionDebugRow(
                id: "high-group",
                label: "high group",
                metricValue: bmsMetricValue(highGroupVoltage) {
                    RideUnits.voltageText(millivolts: $0.value, fractionDigits: 3)
                }
            ),
            SessionDebugRow(
                id: "low-group",
                label: "low group",
                metricValue: bmsMetricValue(lowGroupVoltage) {
                    RideUnits.voltageText(millivolts: $0.value, fractionDigits: 3)
                }
            ),
            SessionDebugRow(
                id: "delta",
                label: "delta",
                metricValue: bmsMetricValue(cellDelta) { String($0.value) }
            ),
            SessionDebugRow(
                id: "lowest-group",
                label: "lowest group",
                metricValue: bmsMetricValue(lowestGroupIndex, format: String.init)
            ),
            SessionDebugRow(
                id: "temperature",
                label: "temperature",
                metricValue: bmsMetricValue(highestTemperature) {
                    RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 1)
                }
            ),
            SessionDebugRow(
                id: "temperature-sensors",
                label: "temperature sensors",
                metricValue: temperatureReadings.isEmpty
                    ? .unavailable
                    : bmsMetricValue(temperatureReadings.count, format: String.init)
            ),
            SessionDebugRow(
                id: "topology",
                label: "topology",
                metricValue: .status(
                    display: topology.layoutLabel,
                    accessibility: topology.layoutLabel
                )
            ),
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
        lowestGroupIndex.map { pevLocalizedText("bms.cell_map.group_label", Int64($0)) }
    }

    public var overviewPresentation: BmsOverviewPresentation {
        let hasCellVoltageEvidence = groups.contains { group in
            group.voltage.map { $0.value > 0 } ?? false
        }
        let averageGroupVoltage = hasCellVoltageEvidence
            ? self.averageGroupVoltage.flatMap { $0.value > 0 ? $0 : nil }
            : nil
        let lowestGroupVoltage = lowestGroupIndex
            .flatMap { index in groups.first { $0.index == index }?.voltage }
            .flatMap { $0.value > 0 ? $0 : nil }
        let hasTemperatureEvidence = highestTemperature != nil
            && (!temperatureReadings.isEmpty || highestTemperatureLabel != nil)

        return BmsOverviewPresentation(
            averageGroupVoltage: averageGroupVoltage,
            lowestGroupVoltage: lowestGroupVoltage,
            lowestGroupLabel: lowestGroupLabel ?? "",
            highestTemperature: hasTemperatureEvidence ? highestTemperature : nil,
            highestTemperatureLabel: highestTemperatureLabel ?? ""
        )
    }

    public var cellMapVisibilitySummary: String {
        pevLocalizedText("bms.cell_map.groups_visible", Int64(groups.count))
    }

    public var cellMapVisibilityMetricValue: PevDashboardMetricValue {
        .available(display: cellMapVisibilitySummary, accessibility: cellMapVisibilitySummary)
    }

    public var cellMapSpreadSummary: String {
        cellDelta.map { pevLocalizedText("bms.cell_map.spread", Int64($0.value)) }
            ?? pevLocalizedText("bms.cell_map.spread_unavailable")
    }

    public var cellMapSpreadMetricValue: PevDashboardMetricValue {
        guard cellDelta != nil else {
            return .status(display: cellMapSpreadSummary, accessibility: cellMapSpreadSummary)
        }
        return .available(display: cellMapSpreadSummary, accessibility: cellMapSpreadSummary)
    }

    public var cellMapFocusSummary: String {
        let flaggedIndices = flaggedGroups.map(\.index)
        guard !flaggedIndices.isEmpty else {
            return lowestGroupLabel.map { pevLocalizedText("bms.cell_map.lowest", $0) } ?? topology.layoutLabel
        }
        return pevLocalizedText("bms.cell_map.flagged", flaggedIndices.map(String.init).joined(separator: ", "))
    }

    public var cellMapFocusMetricValue: PevDashboardMetricValue {
        .status(display: cellMapFocusSummary, accessibility: cellMapFocusSummary)
    }

    public var cellMapFocusDetail: String? {
        flaggedGroups.lazy.compactMap(\.detail).first ?? highestTemperatureLabel
    }

    public func detailGroupStatus(for index: Int) -> String {
        guard let group = groups.first(where: { $0.index == index }) else {
            return topology.layoutLabel
        }

        if lowestGroupIndex == group.index {
            return cellDelta.map { pevLocalizedText("bms.detail.lowest_below_average", Int64($0.value)) }
                ?? pevLocalizedText("bms.detail.lowest_group")
        }

        guard
            let averageGroupVoltage,
            let groupVoltage = group.voltage
        else {
            return group.label ?? pevLocalizedText("bms.detail.unnamed_group", Int64(group.index))
        }

        let delta = abs(Int(averageGroupVoltage.value) - Int(groupVoltage.value))
        return pevLocalizedText("bms.detail.group_from_average", Int64(group.index), Int64(delta))
    }

    public func detailGroupTrend(for index: Int) -> String {
        groups.first(where: { $0.index == index })?.detail ?? pevLocalizedText("bms.detail.history_unavailable")
    }

    public func detailGroupTrendMetricValue(for index: Int) -> PevDashboardMetricValue {
        let value = pevLocalizedText("bms.detail.trend", detailGroupTrend(for: index))
        return .status(display: value, accessibility: value)
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

    public var balancingMetricValue: PevDashboardMetricValue {
        guard let balancingSummary else { return .unavailable }
        return .status(display: balancingSummary, accessibility: balancingSummary)
    }

    public var balancingMetricDetail: String {
        balancingDetail ?? ""
    }

    public var faultMetricValue: PevDashboardMetricValue {
        guard let faultSummary else { return .unavailable }
        return .status(display: faultSummary, accessibility: faultSummary)
    }

    public var faultMetricDetail: String {
        faultDetail ?? ""
    }

    public var unknownTopologyCellCountMetricValue: PevDashboardMetricValue {
        guard let count = topology.seriesGroupCount else { return .unavailable }
        let value = RideUnits.decimalString(Double(count), fractionDigits: 0)
        return .available(display: value, accessibility: value)
    }

    public var unknownTopologyCellCountDetail: String {
        switch topology.confidence {
        case .verified:
            pevLocalizedText("bms.topology.layout_verified")
        case .inferred:
            pevLocalizedText("bms.topology.layout_inferred")
        case .unverified:
            pevLocalizedText("bms.topology.layout_unverified")
        }
    }

    public var unknownTopologyTemperatureSensorCountMetricValue: PevDashboardMetricValue {
        let sensorCount = groups.compactMap(\.temperature).count
        guard sensorCount > 0 else { return .unavailable }
        let value = RideUnits.decimalString(Double(sensorCount), fractionDigits: 0)
        return .available(display: value, accessibility: value)
    }

    public var unknownTopologyTemperatureDetail: String {
        highestTemperatureLabel ?? pevLocalizedText("bms.topology.sensor_names_unavailable")
    }

    public var unknownTopologySummaryMetricValue: PevDashboardMetricValue {
        guard let faultSummary else { return .unavailable }
        return .status(display: faultSummary, accessibility: faultSummary)
    }

    public var unknownTopologySummaryDetail: String {
        faultDetail ?? ""
    }

    public var unknownTopologyFaultMetricValue: PevDashboardMetricValue {
        guard let fault = faults.first else { return .unavailable }
        return .available(display: fault.code, accessibility: fault.code)
    }

    public var unknownTopologyFaultDetail: String {
        faults.first?.label ?? ""
    }

    public var unknownTopologyCaptureDetail: String {
        faultDetail ?? topology.layoutLabel
    }

    public var unknownTopologyCapturePresentation: BmsUnknownTopologyCapturePresentation {
        BmsUnknownTopologyCapturePresentation(
            title: captureActionTitle ?? pevLocalizedText("bms.unknown.capture_unavailable"),
            detail: unknownTopologyCaptureDetail,
            state: captureActionState ?? ""
        )
    }

    public var inlineCellMapModes: [PevBmsMode] {
        var modes: [PevBmsMode] = [.balanceView]
        if groups.contains(where: { $0.temperature != nil }) {
            modes.append(.temperatures)
        }
        if !faults.isEmpty || !flaggedGroups.isEmpty {
            modes.append(.faults)
        }
        return modes
    }

    public var scrollableCellMapModes: [PevBmsMode] {
        var modes: [PevBmsMode] = [.overview, .strip, .rawTable]
        if groups.contains(where: { $0.temperature != nil }) {
            modes.append(.temperatures)
        } else if !faults.isEmpty || !flaggedGroups.isEmpty {
            modes.append(.faults)
        }
        return modes
    }

    public var cellMapInteractionHint: String {
        if groups.contains(where: { $0.resistance != nil }) {
            return pevLocalizedText("bms.cell_map.hint.resistance")
        }
        if groups.contains(where: { $0.temperature != nil }) {
            return pevLocalizedText("bms.cell_map.hint.temperature")
        }
        return pevLocalizedText("bms.cell_map.hint.voltage")
    }

    public var scrollableCellMapRule: String {
        guard !groups.isEmpty else {
            return topology.layoutLabel
        }
        return pevLocalizedText("bms.cell_map.overview_rule", Int64(groups.count))
    }

    public var scrollableCellMapFocusHint: String {
        flaggedGroups.isEmpty
            ? pevLocalizedText("bms.cell_map.focus.none")
            : pevLocalizedText("bms.cell_map.focus.flagged")
    }

    public var noDataWarningTitle: String {
        pevLocalizedText("bms.no_data.title")
    }

    public var noDataWarningLines: [BmsNoDataTextRow] {
        [
            .cellBalanceWarning,
            .bmsDiagnosticsWarning,
        ]
    }

    public var noDataUnknownRows: [BmsNoDataTextRow] {
        [
            .cellVoltages,
            .weakGroups,
            .bmsDiagnostics,
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

private func bmsMetricValue<Value>(
    _ value: Value?,
    format: (Value) -> String
) -> PevDashboardMetricValue {
    guard let value else { return .unavailable }
    let text = format(value)
    return .available(display: text, accessibility: text)
}

public func bmsGroupVoltageMetricValue(_ group: BmsGroupSnapshot?) -> PevDashboardMetricValue {
    bmsVoltageMetricValue(group?.voltage)
}

public func bmsVoltageMetricValue(_ voltage: Voltage?) -> PevDashboardMetricValue {
    bmsMetricValue(voltage) {
        RideUnits.voltageText(millivolts: $0.value, fractionDigits: 3)
    }
}

public func bmsPackVoltageMetricValue(_ voltage: Voltage?) -> PevDashboardMetricValue {
    bmsMetricValue(voltage) {
        RideUnits.voltageText(millivolts: $0.value)
    }
}

public func bmsVoltageSagMetricValue(_ voltageSag: VoltageDelta?) -> PevDashboardMetricValue {
    bmsMetricValue(voltageSag) {
        RideUnits.decimalString(abs(Double($0.value)) / 1_000.0, fractionDigits: 1)
    }
}

public func bmsBatteryCurrentMetricValue(_ current: BatteryCurrent?) -> PevDashboardMetricValue {
    bmsMetricValue(current) {
        RideUnits.decimalString(Double($0.value) / 1_000.0, fractionDigits: 0)
    }
}

public func bmsTemperatureMetricValue(_ temperature: Temperature?) -> PevDashboardMetricValue {
    bmsMetricValue(temperature) {
        RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 1)
    }
}

private func bmsPageMetricValue(
    selector: UInt8?,
    tag: UInt16?,
    kind: String?
) -> PevDashboardMetricValue {
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
    guard !parts.isEmpty else { return .unavailable }
    let text = parts.joined(separator: " ")
    return .status(display: text, accessibility: text)
}

public enum SessionDebugRowRole: Equatable, Hashable, Sendable {
    case data
    case transportMetadata
}

public struct SessionDebugRow: Equatable, Hashable, Sendable {
    public let id: String
    public let label: String
    public let metricValue: PevDashboardMetricValue
    public let role: SessionDebugRowRole

    public init(
        id: String,
        label: String,
        metricValue: PevDashboardMetricValue,
        role: SessionDebugRowRole = .data
    ) {
        self.id = id
        self.label = label
        self.metricValue = metricValue
        self.role = role
    }

}

public extension ReadbackAvailability {
    var displayText: String {
        switch self {
        case .available:
            pevLocalizedText("readback.availability.available")
        case .unavailable:
            pevLocalizedText("readback.availability.unavailable")
        case .unsupported:
            pevLocalizedText("readback.availability.unsupported")
        }
    }
}

public extension VerificationState {
    var displayText: String {
        switch self {
        case .unverified:
            pevLocalizedText("verification.unverified")
        case .inferred:
            pevLocalizedText("verification.inferred")
        case .sourceVerified:
            pevLocalizedText("verification.source_verified")
        case .hardwareVerified:
            pevLocalizedText("verification.hardware_verified")
        case .sourceAndHardwareVerified:
            pevLocalizedText("verification.source_and_hardware_verified")
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
            SessionDebugRow(
                id: "Notifications",
                label: "Notifications",
                metricValue: .status(
                    display: "\(notificationCount)",
                    accessibility: "\(notificationCount)"
                )
            ),
            SessionDebugRow(
                id: "Last update",
                label: "Last update",
                metricValue: .status(display: lastUpdateText, accessibility: lastUpdateText)
            ),
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

public enum RideTelemetryFreshnessPolicy {
    public static let staleAfter = MonotonicMilliseconds(2_000)
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
    case chargeEstimate
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

public enum ControllerOnlyEstimateConfidence: Equatable, Hashable, Sendable {
    case medium
    case low
    case unknown
}

public enum ControllerOnlyEstimateDetail: Equatable, Hashable, Sendable {
    case recentSag
    case voltageCurve
    case unavailable
}

public struct BmsNoDataPresentation: Equatable, Hashable, Sendable {
    public let controllerEstimateMetricValue: PevDashboardMetricValue
    public let controllerEstimateDetail: ControllerOnlyEstimateDetail
    public let controllerConfidence: ControllerOnlyEstimateConfidence
    public let packVoltageMetricValue: PevDashboardMetricValue
    public let rideSagMetricValue: PevDashboardMetricValue
    public let loadMetricValue: PevDashboardMetricValue

    public init(snapshot: BmsSnapshot, rideState: EucRideScreenState?) {
        let fallbackEstimateDetail: ControllerOnlyEstimateDetail
        let fallbackConfidence: ControllerOnlyEstimateConfidence
        if snapshot.voltage != nil, snapshot.current != nil {
            fallbackEstimateDetail = .recentSag
            fallbackConfidence = .medium
        } else if snapshot.voltage != nil {
            fallbackEstimateDetail = .voltageCurve
            fallbackConfidence = .low
        } else {
            fallbackEstimateDetail = .unavailable
            fallbackConfidence = snapshot.energyPercent == nil ? .unknown : .low
        }

        let telemetry = rideState?.telemetry
        controllerEstimateMetricValue = snapshot.noDataPackEstimateMetricValue(
            controllerEstimatePercent: rideState?.controllerOnlyEstimatePercent
        )
        controllerEstimateDetail = rideState?.controllerOnlyEstimateDetail ?? fallbackEstimateDetail
        controllerConfidence = rideState?.controllerOnlyConfidence ?? fallbackConfidence
        packVoltageMetricValue = bmsPackVoltageMetricValue(telemetry?.voltage ?? snapshot.voltage)
        rideSagMetricValue = bmsVoltageSagMetricValue(rideState?.voltageSag)
        loadMetricValue = bmsBatteryCurrentMetricValue(telemetry?.batteryCurrent ?? snapshot.current)
    }
}

public extension BmsSnapshot {
    func noDataPresentation(rideState: EucRideScreenState?) -> BmsNoDataPresentation {
        BmsNoDataPresentation(snapshot: self, rideState: rideState)
    }
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

    public var pwmHeadroomMetricValue: PevDashboardMetricValue {
        switch pwmHeadroomApplicability {
        case .available:
            guard let pwmHeadroomPermille else {
                return .unavailable
            }

            let value = RideUnits.permillePercentText(pwmHeadroomPermille) + "%"
            return .available(display: value, accessibility: value)
        case .notApplicable:
            let value = pevLocalizedText("metric.availability.not_applicable")
            return .status(display: value, accessibility: value)
        case .unavailable:
            return .unavailable
        }
    }

    public var pwmHeadroomProgress: Double? {
        pwmHeadroomPermille.map { Double($0) / 1_000.0 }
    }

    public var regenerationPower: Power? {
        guard telemetry?.powerFlow == .regeneration else {
            return nil
        }

        return telemetry?.powerPresentation.power
    }

    public var voltageSag: VoltageDelta? {
        telemetry?.voltageSag
    }

    public var chargeEstimate: ChargeEstimateState {
        telemetry?.chargeEstimate ?? .missingProfile
    }

    public var limpHomeRange: Distance? {
        telemetry?.limpHomeRange
    }

    public var limpHomeRangeMetricValue: PevDashboardMetricValue {
        guard let limpHomeRange else { return .unavailable }
        let text = RideUnits.distanceText(
            millimetres: limpHomeRange.value,
            unit: RideUnits.distanceUnit(forSpeedUnit: speedUnit),
            fractionDigits: 1
        )
        return .available(display: text, accessibility: text)
    }

    public var controllerOnlyEstimatePercent: BatteryLevel? {
        telemetry?.batteryLevelReported ?? telemetry?.batteryLevelEstimated
    }

    public var controllerOnlyEstimateDetail: ControllerOnlyEstimateDetail {
        if telemetry?.voltage != nil, voltageSag != nil {
            return .recentSag
        }
        if telemetry?.voltage != nil {
            return .voltageCurve
        }
        return .unavailable
    }

    public var controllerOnlyConfidence: ControllerOnlyEstimateConfidence {
        if controllerOnlyEstimatePercent != nil, voltageSag != nil {
            return .medium
        }
        if controllerOnlyEstimatePercent != nil || telemetry?.voltage != nil {
            return .low
        }
        return .unknown
    }

    public func updateAge(
        at now: MonotonicMilliseconds,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> EucRideUpdateAge {
        rideUpdateAge(
            updatedAt: telemetry?.at ?? displayState.lastUpdate,
            at: now,
            staleAfter: staleThreshold
        )
    }

    public var warningState: EucRideWarningState {
        switch phase {
        case .failed(let failure):
            return EucRideWarningState(
                severity: .failed,
                title: pevLocalizedText("euc.warning.connection_failed"),
                detail: failure.displayText
            )
        case .live where telemetryAvailability == .populated:
            if shouldReduceAcceleration {
                return EucRideWarningState(
                    severity: .reduceAcceleration,
                    title: pevLocalizedText("euc.warning.reduce_acceleration"),
                    detail: pevLocalizedText("euc.warning.low_pwm_headroom")
                )
            }

            return EucRideWarningState(
                severity: .normal,
                title: pevLocalizedText("euc.warning.telemetry_live"),
                detail: telemetry?.speed == nil
                    ? pevLocalizedText("euc.warning.waiting_for_speed")
                    : pevLocalizedText("euc.warning.live_telemetry_detail")
            )
        case .live where telemetryAvailability == .waitingForValues:
            return EucRideWarningState(
                severity: .caution,
                title: pevLocalizedText("euc.warning.waiting_for_telemetry"),
                detail: pevLocalizedText("euc.warning.subscribed_no_values")
            )
        case .live:
            return EucRideWarningState(
                severity: .unavailable,
                title: pevLocalizedText("euc.warning.telemetry_unavailable"),
                detail: pevLocalizedText("euc.warning.no_live_snapshot")
            )
        case .connecting, .discoveringServices, .subscribing:
            return EucRideWarningState(
                severity: .caution,
                title: phaseText,
                detail: pevLocalizedText("euc.warning.waiting_for_live_telemetry")
            )
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning:
            return EucRideWarningState(
                severity: .unavailable,
                title: phaseText,
                detail: pevLocalizedText("euc.warning.screen_inactive")
            )
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
                title: pevLocalizedText("euc.warning.telemetry_stale"),
                detail: pevLocalizedText("euc.warning.last_update", String(elapsed.rawValue))
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
            return pevLocalizedText("euc.status.parked")
        case .standing:
            return pevLocalizedText("euc.status.standing")
        case .riding:
            return pevLocalizedText("euc.status.riding")
        case .charging:
            return pevLocalizedText("euc.status.charging")
        case .unknown:
            return pevLocalizedText("euc.connection.live")
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
            EucRideVisibleFieldCoverage(field: .chargeEstimate, source: chargeEstimateCoverage),
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
        if telemetry?.powerPresentation.power == nil {
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
        if case .calculatedPackCurrent = telemetry.powerPresentation {
            return .derivedTelemetry
        }
        if case .reported = telemetry.powerPresentation {
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

    private var chargeEstimateCoverage: EucRideVisibleFieldSource {
        switch chargeEstimate.kind {
        case .available, .collectingSamples:
            .derivedTelemetry
        case .unavailable, .stale, .failed:
            .explicitlyUnavailable
        }
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

    public var displayText: String {
        switch self {
        case .missingNotifyChannel:
            pevLocalizedText("euc.failure.missing_notify_channel")
        case .missingWriteChannel:
            pevLocalizedText("euc.failure.missing_write_channel")
        case .sessionFailed(let message):
            pevLocalizedText("euc.failure.session", message)
        case .connectFailed(let message):
            pevLocalizedText("euc.failure.connect", message)
        case .serviceDiscoveryFailed(let message):
            pevLocalizedText("euc.failure.service_discovery", message)
        case .characteristicDiscoveryFailed(let message):
            pevLocalizedText("euc.failure.characteristic_discovery", message)
        case .notificationFailed(let message):
            pevLocalizedText("euc.failure.notification", message)
        case .notificationIngestFailed(let message):
            pevLocalizedText("euc.failure.notification_ingest", message)
        }
    }
}

public struct SessionConnectionRetry: Equatable, Hashable, Sendable {
    public let platformIdentifier: String
    public let attempt: Int
    public let deadline: MonotonicMilliseconds
    public let failure: SessionConnectionFailure

    public init(
        platformIdentifier: String,
        attempt: Int,
        deadline: MonotonicMilliseconds,
        failure: SessionConnectionFailure
    ) {
        self.platformIdentifier = platformIdentifier
        self.attempt = attempt
        self.deadline = deadline
        self.failure = failure
    }
}

public enum SessionConnectionPhase: Equatable, Hashable, Sendable {
    case starting
    case bluetoothPermissionDenied
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
            pevLocalizedText("euc.connection.starting")
        case .bluetoothPermissionDenied:
            pevLocalizedText("euc.connection.permission_denied")
        case .bluetoothUnavailable(let rawState):
            pevLocalizedText("euc.connection.unavailable", rawState)
        case .scanning:
            pevLocalizedText("euc.connection.scanning")
        case .connecting(let model):
            pevLocalizedText("euc.connection.connecting", model.displayName)
        case .discoveringServices:
            pevLocalizedText("euc.connection.discovering_services")
        case .subscribing:
            pevLocalizedText("euc.connection.subscribing")
        case .live:
            pevLocalizedText("euc.connection.live")
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

    var dto: DiscoveryElectricUnicycleModel {
        switch self {
        case .aero:
            .aero
        case .falcon:
            .falcon
        }
    }
}

struct VoltageSagModelStore {
    private struct Record: Codable {
        let schemaVersion: UInt16
        let deviceIdentity: String
        let effectiveResistanceMilliohms: UInt32
        let observations: UInt16
        let hardwareVerified: Bool
        let lastLearnedWallClockMilliseconds: Int64
    }

    private static let keyPrefix = "io.cutout.voltage-sag.v1."
    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func load(for deviceIdentity: String) -> MobileVoltageSagModelDto? {
        guard
            !deviceIdentity.isEmpty,
            let data = defaults.data(forKey: Self.keyPrefix + deviceIdentity),
            let record = try? JSONDecoder().decode(Record.self, from: data),
            record.schemaVersion == 1,
            record.deviceIdentity == deviceIdentity
        else {
            return nil
        }
        return MobileVoltageSagModelDto(
            schemaVersion: record.schemaVersion,
            effectiveResistanceMilliohms: record.effectiveResistanceMilliohms,
            observations: record.observations,
            hardwareVerified: record.hardwareVerified
        )
    }

    func save(_ model: MobileVoltageSagModelDto, for deviceIdentity: String) {
        guard !deviceIdentity.isEmpty else { return }
        let record = Record(
            schemaVersion: model.schemaVersion,
            deviceIdentity: deviceIdentity,
            effectiveResistanceMilliohms: model.effectiveResistanceMilliohms,
            observations: model.observations,
            hardwareVerified: model.hardwareVerified,
            lastLearnedWallClockMilliseconds: Int64(Date().timeIntervalSince1970 * 1_000)
        )
        guard let data = try? JSONEncoder().encode(record) else { return }
        defaults.set(data, forKey: Self.keyPrefix + deviceIdentity)
    }

    func remove(for deviceIdentity: String) {
        guard !deviceIdentity.isEmpty else { return }
        defaults.removeObject(forKey: Self.keyPrefix + deviceIdentity)
    }
}

public final class ElectricUnicycleSession: @unchecked Sendable {
    private enum Inner {
        case aero(AeroReadOnlySession)
        case falcon(FalconReadOnlySession)
    }

    public let model: ElectricUnicycleModel
    private let inner: Inner
    private let chargeEstimator = MobileChargeEstimator()
    private let voltageSagIdentity: String?
    private let voltageSagStore = VoltageSagModelStore()
    private var persistedVoltageSagObservations: UInt16 = 0
    private var chargeEstimateState = ChargeEstimateState.missingProfile

    public init(model: ElectricUnicycleModel, deviceIdentity: String? = nil) throws {
        self.model = model
        self.voltageSagIdentity = deviceIdentity
        self.inner = switch model {
        case .aero:
            .aero(AeroReadOnlySession())
        case .falcon:
            .falcon(try FalconReadOnlySession())
        }
        chargeEstimator.configureElectricUnicycleProfile(model: model.dto)
        if
            let deviceIdentity,
            let model = voltageSagStore.load(for: deviceIdentity),
            chargeEstimator.restoreVoltageSagModel(model: model)
        {
            persistedVoltageSagObservations = model.observations
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
            TelemetrySnapshot(session.currentSnapshot(), chargeEstimate: chargeEstimateState)
        case .falcon(let session):
            TelemetrySnapshot(session.currentSnapshot(), chargeEstimate: chargeEstimateState)
        }
    }

    public var chargeEstimate: ChargeEstimateState {
        chargeEstimateState
    }

    public func configureChargeEstimate(profile: ChargeEstimateProfile) {
        let hadVoltageSagModel = chargeEstimator.voltageSagModel() != nil
        chargeEstimator.configureProfile(profile: profile.dto)
        if hadVoltageSagModel, chargeEstimator.voltageSagModel() == nil {
            persistedVoltageSagObservations = 0
            if let voltageSagIdentity {
                voltageSagStore.remove(for: voltageSagIdentity)
            }
        }
        refreshChargeEstimate(at: currentSnapshot.at ?? MonotonicMilliseconds(0))
    }

    public func clearChargeEstimateProfile() {
        chargeEstimator.clearProfile()
        chargeEstimateState = .missingProfile
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
        let actions = switch inner {
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
        if kind == .linkDown {
            persistVoltageSagModelIfChanged(force: true)
            chargeEstimator.reset()
            chargeEstimateState = .missingProfile
        } else {
            refreshChargeEstimate(at: monotonicMilliseconds)
        }
        return actions
    }

    private func refreshChargeEstimate(at monotonicMilliseconds: MonotonicMilliseconds) {
        let snapshot: MobileTelemetrySnapshotDto = switch inner {
        case .aero(let session): session.currentSnapshot()
        case .falcon(let session): session.currentSnapshot()
        }
        chargeEstimateState = ChargeEstimateState(chargeEstimator.update(input: MobileChargeEstimateInputDto(
            at: monotonicMilliseconds.dto,
            snapshot: snapshot,
            freshness: MobileDurationDto(milliseconds: 30_000)
        )))
        persistVoltageSagModelIfChanged()
    }

    private func persistVoltageSagModelIfChanged(force: Bool = false) {
        guard
            let voltageSagIdentity,
            let model = chargeEstimator.voltageSagModel(),
            model.observations != persistedVoltageSagObservations,
            force || persistedVoltageSagObservations == 0
                || UInt32(model.observations) >= UInt32(persistedVoltageSagObservations) + 8
        else {
            return
        }
        voltageSagStore.save(model, for: voltageSagIdentity)
        persistedVoltageSagObservations = model.observations
    }

}

public final class VescOnewheelSession: @unchecked Sendable {
    private let inner: VescReadOnlySession
    private let chargeEstimator = MobileChargeEstimator()
    private var chargeEstimateState = ChargeEstimateState.missingProfile

    public init() {
        self.inner = VescReadOnlySession()
    }

    public init(boardProfile: VescBoardProfile) {
        self.inner = VescReadOnlySession.withBoardProfile(boardProfile: boardProfile)
        chargeEstimator.configureVescBoardProfile(boardProfile: boardProfile)
    }

    public var diagnostics: ParserDiagnostics {
        ParserDiagnostics(inner.diagnostics())
    }

    public var currentSnapshot: TelemetrySnapshot {
        TelemetrySnapshot(inner.currentSnapshot(), chargeEstimate: chargeEstimateState)
    }

    public var chargeEstimate: ChargeEstimateState {
        chargeEstimateState
    }

    public func configureChargeEstimate(profile: ChargeEstimateProfile) {
        chargeEstimator.configureProfile(profile: profile.dto)
        refreshChargeEstimate(at: currentSnapshot.at ?? MonotonicMilliseconds(0))
    }

    public func clearChargeEstimateProfile() {
        chargeEstimator.clearProfile()
        chargeEstimateState = .missingProfile
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
        let actions = try inner.step(
            kind,
            at: monotonicMilliseconds,
            writeLimit: writeLimit,
            channel: channel,
            bytes: bytes,
            command: command
        )
        if kind == .linkDown {
            chargeEstimator.reset()
            chargeEstimateState = .missingProfile
        } else {
            refreshChargeEstimate(at: monotonicMilliseconds)
        }
        return actions
    }

    private func refreshChargeEstimate(at monotonicMilliseconds: MonotonicMilliseconds) {
        chargeEstimateState = ChargeEstimateState(chargeEstimator.update(input: MobileChargeEstimateInputDto(
            at: monotonicMilliseconds.dto,
            snapshot: inner.currentSnapshot(),
            freshness: MobileDurationDto(milliseconds: 30_000)
        )))
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

    public static func electricUnicycle(
        model: ElectricUnicycleModel,
        deviceIdentity: String? = nil
    ) throws -> CoreBluetoothSession {
        try .electricUnicycle(ElectricUnicycleSession(
            model: model,
            deviceIdentity: deviceIdentity
        ))
    }

    public static func vescOnewheel() -> CoreBluetoothSession {
        .vescOnewheel(VescOnewheelSession())
    }

    public static func vescOnewheel(boardProfile: VescBoardProfile) -> CoreBluetoothSession {
        .vescOnewheel(VescOnewheelSession(boardProfile: boardProfile))
    }

    /// Configures the Rust-owned charge estimate profile for this live session.
    public func configureChargeEstimate(profile: ChargeEstimateProfile) {
        switch self {
        case .electricUnicycle(let session):
            session.configureChargeEstimate(profile: profile)
        case .vescOnewheel(let session):
            session.configureChargeEstimate(profile: profile)
        }
    }

    /// Removes the charge estimate profile and clears its bounded history.
    public func clearChargeEstimateProfile() {
        switch self {
        case .electricUnicycle(let session):
            session.clearChargeEstimateProfile()
        case .vescOnewheel(let session):
            session.clearChargeEstimateProfile()
        }
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
            try session.linkUp(at: monotonicMilliseconds, writeLimit: writeLimit)
                + session.perform(.requestTelemetry, at: monotonicMilliseconds)
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

    /// Configures the Rust-owned charge estimate profile for the live session.
    public func configureChargeEstimate(profile: ChargeEstimateProfile) {
        session.configureChargeEstimate(profile: profile)
    }

    /// Removes the charge estimate profile and clears its bounded history.
    public func clearChargeEstimateProfile() {
        session.clearChargeEstimateProfile()
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
    private let maximumRetryAttempts: Int
    private let retryDelay: DispatchTimeInterval
    private var recorded: [CoreBluetoothLiveRecord] = []
    private var pendingRetry: DispatchWorkItem?
    private var pendingRetryTimestamp: MonotonicMilliseconds?
    private var retryAttempts = 0
    private var receivedRealtimeTelemetrySinceLinkUp = false
    private var pendingOperationsAfterSubscription: [CoreBluetoothPlannedOperation] = []
    private var waitingForSubscriptionChannel: BluetoothUuid?

    public init(
        session: CoreBluetoothSession,
        advertisement: CoreBluetoothAdvertisement,
        writeLimit: TransportWriteLimitBytes,
        operationSink: CoreBluetoothOperationSink,
        retryCommandOnLinkUp: DeviceCommand? = nil,
        maximumRetryAttempts: Int = 3,
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
        self.maximumRetryAttempts = max(0, maximumRetryAttempts)
        self.retryDelay = retryDelay
    }

    public var records: [CoreBluetoothLiveRecord] {
        recorded
    }

    /// Configures the Rust-owned charge estimate profile for this connection.
    public func configureChargeEstimate(profile: ChargeEstimateProfile) {
        runner.configureChargeEstimate(profile: profile)
    }

    /// Removes the charge estimate profile and clears its bounded history.
    public func clearChargeEstimateProfile() {
        runner.clearChargeEstimateProfile()
    }

    @discardableResult
    public func handleLinkUp(at monotonicMilliseconds: MonotonicMilliseconds) throws -> CoreBluetoothSessionStep {
        cancelPendingRetry()
        receivedRealtimeTelemetrySinceLinkUp = false
        retryAttempts = 0
        let step = try runner.handle(.linkUp(at: monotonicMilliseconds))
        record(.linkUp(
            platformIdentifier: platformIdentifier,
            writeLimit: step.captureContext?.writeLimit ?? TransportWriteLimitBytes(0)
        ))
        let writes = step.operations.filter { operation in
            if case .subscribe = operation { false } else { true }
        }
        let subscriptions = step.operations.filter { operation in
            if case .subscribe = operation { true } else { false }
        }
        executeAndRecord(writes + subscriptions)
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
        guard
            let retryCommandOnLinkUp,
            retryAttempts < maximumRetryAttempts,
            pendingRetry == nil
        else {
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
        retryAttempts += 1
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
