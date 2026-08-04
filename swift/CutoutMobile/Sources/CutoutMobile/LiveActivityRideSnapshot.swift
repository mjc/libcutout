import CutoutMobileFFI
import Foundation

private func localizedLiveActivityText(_ key: String, _ arguments: CVarArg...) -> String {
    String(
        format: Bundle.module.localizedString(forKey: key, value: nil, table: "Localizable"),
        locale: .current,
        arguments: arguments
    )
}

public enum LiveActivityRideIdentitySource: String, Codable, Equatable, Hashable, Sendable {
    case productionModel
    case productionDevice
    case unavailable
}

public struct LiveActivityRideIdentity: Codable, Equatable, Hashable, Sendable {
    public let label: String
    public let source: LiveActivityRideIdentitySource

    public init(label: String, source: LiveActivityRideIdentitySource) {
        self.label = label
        self.source = source
    }

    public static func model(_ model: ElectricUnicycleModel) -> Self {
        Self(label: model.displayName, source: .productionModel)
    }

    public static func device(_ label: String) -> Self {
        Self(label: label, source: .productionDevice)
    }

    public static var unavailable: Self {
        Self(label: localizedLiveActivityText("live_activity.identity.unavailable"), source: .unavailable)
    }

    public static var accessibilityLabel: String {
        localizedLiveActivityText("live_activity.accessibility.device")
    }

    public func accessibilityValue(for connectionState: LiveActivityRideConnectionState) -> String {
        let spokenIdentity = source == .unavailable ? Self.accessibilityLabel : label
        return localizedLiveActivityText(
            "live_activity.accessibility.identity_connection",
            spokenIdentity,
            connectionState.accessibilityValue
        )
    }
}

public enum LiveActivityRideConnectionState: String, Codable, Equatable, Hashable, Sendable {
    case connected
    case disconnected
    case stale
    case waitingForFirstTelemetry
    case unavailable

    public var accessibilityValue: String {
        switch self {
        case .connected:
            localizedLiveActivityText("live_activity.connection.connected")
        case .disconnected:
            localizedLiveActivityText("live_activity.connection.disconnected")
        case .stale:
            localizedLiveActivityText("live_activity.connection.stale")
        case .waitingForFirstTelemetry:
            localizedLiveActivityText("live_activity.connection.waiting_for_telemetry")
        case .unavailable:
            localizedLiveActivityText("live_activity.connection.unavailable")
        }
    }
}

public enum LiveActivityRideFreshnessPolicy {
    public static let staleAfter = TimeInterval(RideTelemetryFreshnessPolicy.staleAfter.rawValue) / 1_000

    public static func staleDate(after now: Date) -> Date {
        now.addingTimeInterval(staleAfter)
    }
}

public enum LiveActivityRideGlyph: String, Codable, Equatable, Hashable, Sendable {
    case electricUnicycle
    case floatwheelAtom
}

public enum LiveActivityRideValueState: String, Codable, Equatable, Hashable, Sendable {
    case available
    case unavailable
    case stale
    case notApplicable
    case deferred
}

public enum LiveActivityRideValueSource: String, Codable, Equatable, Hashable, Sendable {
    case sessionState
    case liveTelemetry
    case derivedTelemetry
    case appLifecycle
    case explicitlyUnavailable
    case notApplicable
    case deferred
}

public struct LiveActivityRideValue: Codable, Equatable, Hashable, Sendable {
    public let label: String
    public let value: String
    public let unit: String?
    public let normalizedProgress: Double?
    public let accessibilityDetail: String?
    public let state: LiveActivityRideValueState
    public let source: LiveActivityRideValueSource

    public init(
        label: String,
        value: String,
        unit: String?,
        normalizedProgress: Double? = nil,
        accessibilityDetail: String? = nil,
        state: LiveActivityRideValueState,
        source: LiveActivityRideValueSource
    ) {
        self.label = label
        self.value = value
        self.unit = unit
        self.normalizedProgress = normalizedProgress
        self.accessibilityDetail = accessibilityDetail
        self.state = state
        self.source = source
    }

    public static func available(
        label: String,
        value: String,
        unit: String?,
        normalizedProgress: Double? = nil,
        accessibilityDetail: String? = nil,
        source: LiveActivityRideValueSource
    ) -> Self {
        Self(
            label: label,
            value: value,
            unit: unit,
            normalizedProgress: normalizedProgress,
            accessibilityDetail: accessibilityDetail,
            state: .available,
            source: source
        )
    }

    public static func unavailable(
        label: String,
        unit: String? = nil,
        accessibilityDetail: String? = nil
    ) -> Self {
        Self(
            label: label,
            value: "--",
            unit: unit,
            accessibilityDetail: accessibilityDetail,
            state: .unavailable,
            source: .explicitlyUnavailable
        )
    }

    public static func stale(
        label: String,
        value: String,
        unit: String?,
        normalizedProgress: Double? = nil,
        accessibilityDetail: String? = nil,
        source: LiveActivityRideValueSource
    ) -> Self {
        Self(
            label: label,
            value: value,
            unit: unit,
            normalizedProgress: normalizedProgress,
            accessibilityDetail: accessibilityDetail,
            state: .stale,
            source: source
        )
    }

    public static func notApplicable(label: String, unit: String? = nil) -> Self {
        Self(
            label: label,
            value: localizedLiveActivityText("live_activity.value.not_applicable"),
            unit: unit,
            state: .notApplicable,
            source: .notApplicable
        )
    }

    public static func deferred(label: String, unit: String? = nil) -> Self {
        Self(label: label, value: "--", unit: unit, state: .deferred, source: .deferred)
    }

    public var progressValue: Double? {
        guard
            unit == "%",
            state == .available || state == .stale
        else { return nil }

        return clampedNormalizedProgress
    }

    public var displayValue: String {
        switch state {
        case .available, .stale:
            value
        case .notApplicable:
            localizedLiveActivityText("live_activity.value.not_applicable_compact")
        case .unavailable, .deferred:
            "--"
        }
    }

    public var accessibilityText: String {
        [value, unit, accessibilityDetail]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: ", ")
    }

    public var accessibilityProvenance: String? {
        switch source {
        case .liveTelemetry:
            localizedLiveActivityText("live_activity.provenance.vehicle_telemetry")
        case .derivedTelemetry:
            localizedLiveActivityText("live_activity.provenance.derived_telemetry")
        case .sessionState:
            localizedLiveActivityText("live_activity.provenance.session_state")
        case .appLifecycle:
            localizedLiveActivityText("live_activity.provenance.app_state")
        case .explicitlyUnavailable, .notApplicable, .deferred:
            nil
        }
    }

    public var accessibilityValue: String {
        switch state {
        case .available:
            [value, unit, accessibilityProvenance, accessibilityDetail].compactMap { $0 }.joined(separator: ", ")
        case .stale:
            [value, unit, accessibilityProvenance, accessibilityDetail, localizedLiveActivityText("live_activity.connection.stale")].compactMap { $0 }.joined(separator: ", ")
        case .unavailable:
            [localizedLiveActivityText("live_activity.connection.unavailable"), unit, accessibilityDetail].compactMap { $0 }.joined(separator: ", ")
        case .notApplicable:
            [localizedLiveActivityText("live_activity.value.not_applicable_accessibility"), unit].compactMap { $0 }.joined(separator: ", ")
        case .deferred:
            [localizedLiveActivityText("live_activity.value.waiting_for_data"), unit, accessibilityDetail].compactMap { $0 }.joined(separator: ", ")
        }
    }

    public var speedGaugeProgressValue: Double? {
        guard
            unit == "mph" || unit == "km/h" || unit == "kmh",
            state == .available || state == .stale
        else { return nil }

        return clampedNormalizedProgress
    }

    private var clampedNormalizedProgress: Double? {
        normalizedProgress.map { min(max($0, 0), 1) }
    }
}

public enum LiveActivityRideHeadroomSeverity: String, Codable, Equatable, Hashable, Sendable {
    case nominal
    case reduceAcceleration
    case unavailable
    case notApplicable
}

public enum LiveActivityRidePwmSeverity: String, Codable, Equatable, Hashable, Sendable {
    case nominal
    case critical
    case unavailable

    public static let criticalThreshold = 0.80

    public init(pwm: LiveActivityRideValue) {
        guard let usage = pwm.progressValue else {
            self = .unavailable
            return
        }
        self = usage > Self.criticalThreshold ? .critical : .nominal
    }

    public var accessibilityDescription: String? {
        self == .critical ? localizedLiveActivityText("live_activity.pwm.critical") : nil
    }
}

public struct LiveActivityRideSnapshot: Codable, Equatable, Hashable, Sendable {
    public let identity: LiveActivityRideIdentity
    public let glyph: LiveActivityRideGlyph
    public let connectionState: LiveActivityRideConnectionState
    public let sessionStatus: LiveActivityRideValue
    public let speed: LiveActivityRideValue
    public let battery: LiveActivityRideValue
    public let packVoltage: LiveActivityRideValue
    public let pwm: LiveActivityRideValue
    public let mode: LiveActivityRideValue
    public let duration: LiveActivityRideValue
    public let distance: LiveActivityRideValue
    public let headroom: LiveActivityRideValue
    public let headroomSeverity: LiveActivityRideHeadroomSeverity?
    public let beeps: LiveActivityRideValue
    public let temperature: LiveActivityRideValue
    public let chargeEstimate: LiveActivityRideValue

    public init(
        identity: LiveActivityRideIdentity,
        glyph: LiveActivityRideGlyph = .electricUnicycle,
        rideState: EucRideScreenState,
        now: MonotonicMilliseconds? = nil,
        staleAfter staleThreshold: MonotonicMilliseconds = RideTelemetryFreshnessPolicy.staleAfter,
        rideDuration: MonotonicMilliseconds? = nil
    ) {
        let connectionState = Self.deriveConnectionState(
            identity: identity,
            rideState: rideState,
            now: now,
            staleAfter: staleThreshold
        )
        self.identity = identity
        self.glyph = glyph
        self.connectionState = connectionState
        self.sessionStatus = Self.statusValue(rideState: rideState, connectionState: connectionState)
        self.speed = Self.speedValue(rideState: rideState, connectionState: connectionState)
        self.battery = Self.batteryValue(telemetry: rideState.telemetry, connectionState: connectionState)
        self.packVoltage = Self.voltageValue(telemetry: rideState.telemetry, connectionState: connectionState)
        self.pwm = Self.pwmValue(rideState: rideState, connectionState: connectionState)
        self.mode = .deferred(label: localizedLiveActivityText("live_activity.label.mode"))
        self.duration = Self.durationValue(rideDuration)
        self.distance = Self.distanceValue(
            telemetry: rideState.telemetry,
            speedUnit: rideState.speedUnit,
            connectionState: connectionState
        )
        self.headroom = Self.headroomValue(rideState: rideState, connectionState: connectionState)
        self.headroomSeverity = Self.headroomSeverity(rideState: rideState, connectionState: connectionState)
        self.beeps = .deferred(label: localizedLiveActivityText("live_activity.label.beeps"))
        self.temperature = Self.temperatureValue(telemetry: rideState.telemetry, connectionState: connectionState)
        self.chargeEstimate = Self.chargeEstimateValue(rideState: rideState, connectionState: connectionState)
    }

    public init(
        identity: LiveActivityRideIdentity,
        glyph: LiveActivityRideGlyph = .electricUnicycle,
        connectionState: LiveActivityRideConnectionState,
        sessionStatus: LiveActivityRideValue,
        speed: LiveActivityRideValue,
        battery: LiveActivityRideValue,
        packVoltage: LiveActivityRideValue,
        pwm: LiveActivityRideValue,
        mode: LiveActivityRideValue,
        duration: LiveActivityRideValue,
        distance: LiveActivityRideValue,
        headroom: LiveActivityRideValue,
        beeps: LiveActivityRideValue,
        temperature: LiveActivityRideValue,
        chargeEstimate: LiveActivityRideValue? = nil,
        headroomSeverity: LiveActivityRideHeadroomSeverity? = nil
    ) {
        self.identity = identity
        self.glyph = glyph
        self.connectionState = connectionState
        self.sessionStatus = sessionStatus
        self.speed = speed
        self.battery = battery
        self.packVoltage = packVoltage
        self.pwm = pwm
        self.mode = mode
        self.duration = duration
        self.distance = distance
        self.headroom = headroom
        self.headroomSeverity = headroomSeverity
        self.beeps = beeps
        self.temperature = temperature
        self.chargeEstimate = chargeEstimate ?? .deferred(label: localizedLiveActivityText("live_activity.label.charge"))
    }

    public var visibleValues: [LiveActivityRideValue] {
        [
            speed,
            battery,
            packVoltage,
            pwm,
            mode,
            duration,
            distance,
            headroom,
            beeps,
            temperature,
            chargeEstimate,
        ]
    }

    public var compactTrailingValue: LiveActivityRideValue {
        headroomSeverity == .reduceAcceleration ? headroom : battery
    }

    public var pwmSeverity: LiveActivityRidePwmSeverity {
        LiveActivityRidePwmSeverity(pwm: pwm)
    }

    public var showsCompactPwmBar: Bool {
        glyph == .floatwheelAtom && pwm.progressValue != nil
    }

    public static var activityAccessibilityLabel: String {
        localizedLiveActivityText("live_activity.accessibility.ride")
    }

    public var minimalAccessibilitySummary: String {
        var parts = [
            identity.accessibilityValue(for: connectionState),
            localizedLiveActivityText(
                "live_activity.accessibility.labeled_value",
                speed.label,
                speed.accessibilityValue
            ),
        ]
        if headroomSeverity == .reduceAcceleration {
            parts.append(
                localizedLiveActivityText(
                    "live_activity.accessibility.labeled_value",
                    headroom.label,
                    headroom.accessibilityValue
                )
            )
        }
        return parts.joined(separator: ", ")
    }

    public func presented(isStale: Bool) -> Self {
        guard isStale, connectionState == .connected else { return self }

        return Self(
            identity: identity,
            glyph: glyph,
            connectionState: .stale,
            sessionStatus: sessionStatus.stalePresentation,
            speed: speed.stalePresentation,
            battery: battery.stalePresentation,
            packVoltage: packVoltage.stalePresentation,
            pwm: pwm.stalePresentation,
            mode: mode.stalePresentation,
            duration: duration.stalePresentation,
            distance: distance.stalePresentation,
            headroom: headroom.stalePresentation,
            beeps: beeps.stalePresentation,
            temperature: temperature.stalePresentation,
            chargeEstimate: chargeEstimate.stalePresentation,
            headroomSeverity: headroomSeverity
        )
    }
}

private extension LiveActivityRideValue {
    var stalePresentation: Self {
        guard state == .available else { return self }
        return .stale(
            label: label,
            value: value,
            unit: unit,
            normalizedProgress: normalizedProgress,
            accessibilityDetail: accessibilityDetail,
            source: source
        )
    }
}

extension LiveActivityRideSnapshot {
    static func deriveConnectionState(
        identity: LiveActivityRideIdentity,
        rideState: EucRideScreenState,
        now: MonotonicMilliseconds?,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> LiveActivityRideConnectionState {
        switch (identity.source, rideState.phase, rideState.telemetryAvailability) {
        case (_, .live, .populated):
            now
                .map { rideState.updateAge(at: $0, staleAfter: staleThreshold).freshness }
                .map { $0 == .stale ? .stale : .connected } ?? .connected
        case (_, .connecting, _), (_, .discoveringServices, _), (_, .subscribing, _):
            .waitingForFirstTelemetry
        case (_, .live, _):
            .waitingForFirstTelemetry
        case (_, .bluetoothUnavailable, _), (_, .failed, _):
            .unavailable
        default:
            .disconnected
        }
    }

    static func statusValue(
        rideState: EucRideScreenState,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        value(
            label: localizedLiveActivityText("live_activity.label.status"),
            value: rideState.statusText,
            unit: nil,
            source: .sessionState,
            connectionState: connectionState
        )
    }

    static func speedValue(
        rideState: EucRideScreenState,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        rideState.displayState.speed.millimetersPerSecond
            .map { speed in
                value(
                    label: localizedLiveActivityText("live_activity.label.speed"),
                    value: rideState.speedText,
                    unit: rideState.speedUnit,
                    normalizedProgress: RideUnits.speedValue(millimetersPerSecond: speed) / 50,
                    source: .liveTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: localizedLiveActivityText("live_activity.label.speed"), unit: rideState.speedUnit)
    }

    static func batteryValue(
        telemetry: TelemetrySnapshot?,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        if let reported = telemetry?.batteryLevelReported {
            return value(
                label: localizedLiveActivityText("live_activity.label.battery"),
                value: percentageString(fromPercent: reported.value),
                unit: RideUnits.percentUnit,
                normalizedProgress: Double(reported.value) / 100,
                source: .liveTelemetry,
                connectionState: connectionState
            )
        }

        return telemetry?.batteryLevelEstimated
            .map {
                value(
                    label: localizedLiveActivityText("live_activity.label.battery"),
                    value: percentageString(fromPercent: $0.value),
                    unit: RideUnits.percentUnit,
                    normalizedProgress: Double($0.value) / 100,
                    source: .derivedTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: localizedLiveActivityText("live_activity.label.battery"), unit: RideUnits.percentUnit)
    }

    static func voltageValue(
        telemetry: TelemetrySnapshot?,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        telemetry?.voltage
            .map {
                value(
                    label: localizedLiveActivityText("live_activity.label.voltage"),
                    value: decimalString(fromMillivolts: $0.value, fractionDigits: 1),
                    unit: RideUnits.voltageUnit,
                    source: .liveTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: localizedLiveActivityText("live_activity.label.voltage"), unit: RideUnits.voltageUnit)
    }

    static func pwmValue(
        rideState: EucRideScreenState,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        switch rideState.pwmHeadroomApplicability {
        case .available:
            return rideState.telemetry?.pwm
                .map {
                    value(
                        label: localizedLiveActivityText("live_activity.label.pwm"),
                        value: percentageString(fromPermille: abs($0.permille)),
                        unit: RideUnits.percentUnit,
                        normalizedProgress: Double(abs(Int($0.permille))) / 1_000,
                        source: .liveTelemetry,
                        connectionState: connectionState
                    )
                } ?? .unavailable(label: localizedLiveActivityText("live_activity.label.pwm"), unit: RideUnits.percentUnit)
        case .unavailable:
            return .unavailable(label: localizedLiveActivityText("live_activity.label.pwm"), unit: RideUnits.percentUnit)
        case .notApplicable:
            return .notApplicable(label: localizedLiveActivityText("live_activity.label.pwm"))
        }
    }

    static func durationValue(_ rideDuration: MonotonicMilliseconds?) -> LiveActivityRideValue {
        rideDuration
            .map {
                LiveActivityRideValue.available(
                    label: localizedLiveActivityText("live_activity.label.duration"),
                    value: durationString(from: $0),
                    unit: nil,
                    source: .appLifecycle
                )
            } ?? .deferred(label: localizedLiveActivityText("live_activity.label.duration"))
    }

    static func distanceValue(
        telemetry: TelemetrySnapshot?,
        speedUnit: String,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        let unit = distanceUnit(for: speedUnit)
        return telemetry?.distance
            .map {
                value(
                    label: localizedLiveActivityText("live_activity.label.distance"),
                    value: decimalString(fromMillimetres: $0.value, unit: unit, fractionDigits: 1),
                    unit: unit,
                    source: .liveTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: localizedLiveActivityText("live_activity.label.distance"), unit: unit)
    }

    static func headroomValue(
        rideState: EucRideScreenState,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        switch rideState.pwmHeadroomApplicability {
        case .available:
            return rideState.pwmHeadroomPermille
                .map { _ in
                    value(
                        label: localizedLiveActivityText("live_activity.label.headroom"),
                        value: rideState.warningState.severity == .reduceAcceleration
                            ? localizedLiveActivityText("live_activity.headroom.reduce_acceleration")
                            : localizedLiveActivityText("live_activity.headroom.good"),
                        unit: nil,
                        source: .derivedTelemetry,
                        connectionState: connectionState
                    )
                } ?? .unavailable(label: localizedLiveActivityText("live_activity.label.headroom"))
        case .unavailable:
            return .unavailable(label: localizedLiveActivityText("live_activity.label.headroom"))
        case .notApplicable:
            return .notApplicable(label: localizedLiveActivityText("live_activity.label.headroom"))
        }
    }

    static func headroomSeverity(
        rideState: EucRideScreenState,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideHeadroomSeverity {
        guard connectionState == .connected || connectionState == .stale else { return .unavailable }

        switch rideState.pwmHeadroomApplicability {
        case .available:
            guard rideState.pwmHeadroomPermille != nil else { return .unavailable }
            return rideState.warningState.severity == .reduceAcceleration ? .reduceAcceleration : .nominal
        case .unavailable:
            return .unavailable
        case .notApplicable:
            return .notApplicable
        }
    }

    static func temperatureValue(
        telemetry: TelemetrySnapshot?,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        let unit = temperatureUnit()
        return [
            telemetry?.controllerTemperature,
            telemetry?.motorTemperature,
            telemetry?.batteryTemperature,
        ]
        .compactMap { $0?.value }
        .max()
        .map {
            value(
                label: localizedLiveActivityText("live_activity.label.temperature"),
                value: decimalString(fromMillicelsius: $0, fractionDigits: 0),
                unit: unit,
                source: .liveTelemetry,
                connectionState: connectionState
            )
        } ?? .unavailable(label: localizedLiveActivityText("live_activity.label.temperature"), unit: unit)
    }

    static func chargeEstimateValue(
        rideState: EucRideScreenState,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        let estimate = rideState.chargeEstimate
        switch estimate.kind {
        case .available:
            return value(
                label: localizedLiveActivityText("live_activity.label.charge"),
                value: estimate.displayValue,
                unit: nil,
                source: .derivedTelemetry,
                connectionState: connectionState,
                accessibilityDetail: estimate.displayDetail
            )
        case .collectingSamples:
            return value(
                label: localizedLiveActivityText("live_activity.label.charge"),
                value: estimate.displayValue,
                unit: nil,
                source: .derivedTelemetry,
                connectionState: connectionState,
                accessibilityDetail: estimate.displayDetail
            )
        case .stale:
            return value(
                label: localizedLiveActivityText("live_activity.label.charge"),
                value: estimate.displayValue,
                unit: nil,
                source: .derivedTelemetry,
                connectionState: connectionState,
                state: .stale,
                accessibilityDetail: estimate.displayDetail
            )
        case .unavailable:
            if estimate.unavailableReason == .fullOrNearFull {
                return value(
                    label: localizedLiveActivityText("live_activity.label.charge"),
                    value: estimate.displayValue,
                    unit: nil,
                    source: .derivedTelemetry,
                    connectionState: connectionState,
                    accessibilityDetail: estimate.displayDetail
                )
            }
            return .unavailable(label: localizedLiveActivityText("live_activity.label.charge"), accessibilityDetail: estimate.displayDetail)
        case .failed:
            return .unavailable(label: localizedLiveActivityText("live_activity.label.charge"), accessibilityDetail: estimate.displayDetail)
        }
    }

    static func value(
        label: String,
        value: String,
        unit: String?,
        normalizedProgress: Double? = nil,
        source: LiveActivityRideValueSource,
        connectionState: LiveActivityRideConnectionState,
        state: LiveActivityRideValueState? = nil,
        accessibilityDetail: String? = nil
    ) -> LiveActivityRideValue {
        if (source == .liveTelemetry || source == .derivedTelemetry)
            && connectionState != .connected
            && connectionState != .stale
        {
            return .unavailable(
                label: label,
                unit: unit,
                accessibilityDetail: accessibilityDetail
            )
        }

        return LiveActivityRideValue(
            label: label,
            value: value,
            unit: unit,
            normalizedProgress: normalizedProgress,
            accessibilityDetail: accessibilityDetail,
            state: state ?? (connectionState == .stale ? .stale : .available),
            source: source
        )
    }

    static func percentageString<T: BinaryInteger>(fromPercent percent: T) -> String {
        RideUnits.percentText(percent)
    }

    static func percentageString<T: BinaryInteger>(fromPermille permille: T) -> String {
        RideUnits.permillePercentText(permille)
    }

    static func decimalString<T: BinaryInteger>(fromMillivolts value: T, fractionDigits: Int) -> String {
        RideUnits.voltageText(millivolts: value, fractionDigits: fractionDigits)
    }

    static func decimalString<T: BinaryInteger>(fromMillicelsius value: T, fractionDigits: Int) -> String {
        RideUnits.temperatureText(millicelsius: value, fractionDigits: fractionDigits)
    }

    static func decimalString<T: BinaryInteger>(fromMillimetres value: T, unit: String, fractionDigits: Int) -> String {
        RideUnits.distanceText(millimetres: value, unit: unit, fractionDigits: fractionDigits)
    }

    static func decimalString(_ value: Double, fractionDigits: Int) -> String {
        RideUnits.decimalString(value, fractionDigits: fractionDigits)
    }

    static func distanceUnit(for speedUnit: String) -> String {
        RideUnits.distanceUnit(forSpeedUnit: speedUnit)
    }

    static func temperatureUnit() -> String {
        RideUnits.temperatureUnit
    }

    static func durationString(from duration: MonotonicMilliseconds) -> String {
        let seconds = duration.rawValue / 1_000
        let minutes = seconds / 60
        return String(format: "%llu:%02llu", minutes, seconds % 60)
    }
}
