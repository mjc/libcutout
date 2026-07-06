import Foundation

public enum LiveActivityRideIdentitySource: String, Codable, Equatable, Hashable, Sendable {
    case productionModel
    case fixture
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

    public static func fixture(label: String) -> Self {
        Self(label: label, source: .fixture)
    }

    public static var unavailable: Self {
        Self(label: "Device unavailable", source: .unavailable)
    }

    public var displayLabel: String {
        switch source {
        case .productionModel:
            "\(label) connected"
        case .fixture:
            "\(label) demo"
        case .unavailable:
            label
        }
    }
}

public enum LiveActivityRideConnectionState: String, Codable, Equatable, Hashable, Sendable {
    case connected
    case disconnected
    case stale
    case waitingForFirstTelemetry
    case unavailable
    case fixture
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
    case fixture
    case explicitlyUnavailable
    case notApplicable
    case deferred
}

public struct LiveActivityRideValue: Codable, Equatable, Hashable, Sendable {
    public let label: String
    public let value: String
    public let unit: String?
    public let state: LiveActivityRideValueState
    public let source: LiveActivityRideValueSource

    public init(
        label: String,
        value: String,
        unit: String?,
        state: LiveActivityRideValueState,
        source: LiveActivityRideValueSource
    ) {
        self.label = label
        self.value = value
        self.unit = unit
        self.state = state
        self.source = source
    }

    public static func available(
        label: String,
        value: String,
        unit: String?,
        source: LiveActivityRideValueSource
    ) -> Self {
        Self(label: label, value: value, unit: unit, state: .available, source: source)
    }

    public static func unavailable(label: String, unit: String? = nil) -> Self {
        Self(label: label, value: "--", unit: unit, state: .unavailable, source: .explicitlyUnavailable)
    }

    public static func stale(
        label: String,
        value: String,
        unit: String?,
        source: LiveActivityRideValueSource
    ) -> Self {
        Self(label: label, value: value, unit: unit, state: .stale, source: source)
    }

    public static func notApplicable(label: String, unit: String? = nil) -> Self {
        Self(label: label, value: "Not applicable", unit: unit, state: .notApplicable, source: .notApplicable)
    }

    public static func deferred(label: String, unit: String? = nil) -> Self {
        Self(label: label, value: "--", unit: unit, state: .deferred, source: .deferred)
    }
}

public struct LiveActivityRideSnapshot: Codable, Equatable, Hashable, Sendable {
    public let identity: LiveActivityRideIdentity
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
    public let beeps: LiveActivityRideValue
    public let temperature: LiveActivityRideValue

    public init(
        identity: LiveActivityRideIdentity,
        rideState: EucRideScreenState,
        now: MonotonicMilliseconds? = nil,
        staleAfter staleThreshold: MonotonicMilliseconds = MonotonicMilliseconds(2_000),
        rideDuration: MonotonicMilliseconds? = nil
    ) {
        let connectionState = Self.deriveConnectionState(
            identity: identity,
            rideState: rideState,
            now: now,
            staleAfter: staleThreshold
        )
        self.identity = identity
        self.connectionState = connectionState
        self.sessionStatus = Self.statusValue(rideState: rideState, connectionState: connectionState)
        self.speed = Self.speedValue(rideState: rideState, connectionState: connectionState)
        self.battery = Self.batteryValue(telemetry: rideState.telemetry, connectionState: connectionState)
        self.packVoltage = Self.voltageValue(telemetry: rideState.telemetry, connectionState: connectionState)
        self.pwm = Self.pwmValue(rideState: rideState, connectionState: connectionState)
        self.mode = .deferred(label: "Mode")
        self.duration = Self.durationValue(rideDuration)
        self.distance = Self.distanceValue(telemetry: rideState.telemetry, connectionState: connectionState)
        self.headroom = Self.headroomValue(rideState: rideState, connectionState: connectionState)
        self.beeps = .deferred(label: "Beeps")
        self.temperature = Self.temperatureValue(telemetry: rideState.telemetry, connectionState: connectionState)
    }

    public init(
        identity: LiveActivityRideIdentity,
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
        temperature: LiveActivityRideValue
    ) {
        self.identity = identity
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
        self.beeps = beeps
        self.temperature = temperature
    }

    public static func fixture(
        identity: LiveActivityRideIdentity,
        speed: LiveActivityRideValue,
        battery: LiveActivityRideValue,
        packVoltage: LiveActivityRideValue,
        pwm: LiveActivityRideValue,
        mode: LiveActivityRideValue,
        duration: LiveActivityRideValue,
        distance: LiveActivityRideValue,
        headroom: LiveActivityRideValue,
        beeps: LiveActivityRideValue,
        temperature: LiveActivityRideValue
    ) -> Self {
        Self(
            identity: identity,
            connectionState: .fixture,
            sessionStatus: .available(label: "Status", value: "Fixture", unit: nil, source: .fixture),
            speed: speed,
            battery: battery,
            packVoltage: packVoltage,
            pwm: pwm,
            mode: mode,
            duration: duration,
            distance: distance,
            headroom: headroom,
            beeps: beeps,
            temperature: temperature
        )
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
        ]
    }
}

private extension LiveActivityRideSnapshot {
    static func deriveConnectionState(
        identity: LiveActivityRideIdentity,
        rideState: EucRideScreenState,
        now: MonotonicMilliseconds?,
        staleAfter staleThreshold: MonotonicMilliseconds
    ) -> LiveActivityRideConnectionState {
        switch (identity.source, rideState.phase, rideState.telemetryAvailability) {
        case (.fixture, _, _):
            .fixture
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
        value(label: "Status", value: rideState.statusText, unit: nil, source: .sessionState, connectionState: connectionState)
    }

    static func speedValue(
        rideState: EucRideScreenState,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        rideState.displayState.speed.millimetersPerSecond
            .map { _ in
                value(
                    label: "Speed",
                    value: rideState.speedText,
                    unit: rideState.speedUnit,
                    source: .liveTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: "Speed", unit: rideState.speedUnit)
    }

    static func batteryValue(
        telemetry: TelemetrySnapshot?,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        if let reported = telemetry?.batteryLevelReported {
            return value(
                label: "Battery",
                value: percentageString(fromPercent: reported.value),
                unit: "%",
                source: .liveTelemetry,
                connectionState: connectionState
            )
        }

        return telemetry?.batteryLevelEstimated
            .map {
                value(
                    label: "Battery",
                    value: percentageString(fromPercent: $0.value),
                    unit: "%",
                    source: .derivedTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: "Battery", unit: "%")
    }

    static func voltageValue(
        telemetry: TelemetrySnapshot?,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        telemetry?.voltage
            .map {
                value(
                    label: "Voltage",
                    value: decimalString(fromMillivolts: $0.value, fractionDigits: 1),
                    unit: "V",
                    source: .liveTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: "Voltage", unit: "V")
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
                        label: "PWM",
                        value: percentageString(fromPermille: abs($0.permille)),
                        unit: "%",
                        source: .liveTelemetry,
                        connectionState: connectionState
                    )
                } ?? .unavailable(label: "PWM", unit: "%")
        case .unavailable:
            return .unavailable(label: "PWM", unit: "%")
        case .notApplicable:
            return .notApplicable(label: "PWM", unit: "%")
        }
    }

    static func durationValue(_ rideDuration: MonotonicMilliseconds?) -> LiveActivityRideValue {
        rideDuration
            .map {
                LiveActivityRideValue.available(
                    label: "Duration",
                    value: durationString(from: $0),
                    unit: nil,
                    source: .appLifecycle
                )
            } ?? .deferred(label: "Duration")
    }

    static func distanceValue(
        telemetry: TelemetrySnapshot?,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        telemetry?.distance
            .map {
                value(
                    label: "Distance",
                    value: decimalString(fromMillimetres: $0.value, fractionDigits: 1),
                    unit: "mi",
                    source: .liveTelemetry,
                    connectionState: connectionState
                )
            } ?? .unavailable(label: "Distance", unit: "mi")
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
                        label: "Headroom",
                        value: rideState.warningState.severity == .reduceAcceleration
                            ? "Reduce acceleration"
                            : "Headroom good",
                        unit: nil,
                        source: .derivedTelemetry,
                        connectionState: connectionState
                    )
                } ?? .unavailable(label: "Headroom")
        case .unavailable:
            return .unavailable(label: "Headroom")
        case .notApplicable:
            return .notApplicable(label: "Headroom")
        }
    }

    static func temperatureValue(
        telemetry: TelemetrySnapshot?,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        [
            telemetry?.controllerTemperature,
            telemetry?.motorTemperature,
            telemetry?.batteryTemperature,
        ]
        .compactMap { $0?.value }
        .max()
        .map {
            value(
                label: "Temp",
                value: decimalString(fromMillicelsius: $0, fractionDigits: 0),
                unit: "C",
                source: .liveTelemetry,
                connectionState: connectionState
            )
        } ?? .unavailable(label: "Temp", unit: "C")
    }

    static func value(
        label: String,
        value: String,
        unit: String?,
        source: LiveActivityRideValueSource,
        connectionState: LiveActivityRideConnectionState
    ) -> LiveActivityRideValue {
        LiveActivityRideValue(
            label: label,
            value: value,
            unit: unit,
            state: connectionState == .stale ? .stale : .available,
            source: source
        )
    }

    static func percentageString<T: BinaryInteger>(fromPercent percent: T) -> String {
        "\(percent)"
    }

    static func percentageString<T: BinaryInteger>(fromPermille permille: T) -> String {
        "\(permille / 10)"
    }

    static func decimalString<T: BinaryInteger>(fromMillivolts value: T, fractionDigits: Int) -> String {
        decimalString(Double(Int64(value)) / 1_000.0, fractionDigits: fractionDigits)
    }

    static func decimalString<T: BinaryInteger>(fromMillicelsius value: T, fractionDigits: Int) -> String {
        decimalString(Double(Int64(value)) / 1_000.0, fractionDigits: fractionDigits)
    }

    static func decimalString<T: BinaryInteger>(fromMillimetres value: T, fractionDigits: Int) -> String {
        decimalString(Double(Int64(value)) / 1_609_344.0, fractionDigits: fractionDigits)
    }

    static func decimalString(_ value: Double, fractionDigits: Int) -> String {
        String(format: "%.\(fractionDigits)f", value)
    }

    static func durationString(from duration: MonotonicMilliseconds) -> String {
        let seconds = duration.rawValue / 1_000
        let minutes = seconds / 60
        return String(format: "%llu:%02llu", minutes, seconds % 60)
    }
}
