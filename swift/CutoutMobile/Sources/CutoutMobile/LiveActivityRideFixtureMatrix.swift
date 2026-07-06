public enum LiveActivityRideFixtureKind: String, CaseIterable, Codable, Equatable, Hashable, Sendable {
    case demo
    case populated
    case partial
    case waitingForFirstTelemetry
    case stale
    case disconnected
    case parked
}

public struct LiveActivityRideFixture: Codable, Equatable, Hashable, Sendable {
    public let kind: LiveActivityRideFixtureKind
    public let snapshot: LiveActivityRideSnapshot

    public init(kind: LiveActivityRideFixtureKind, snapshot: LiveActivityRideSnapshot) {
        self.kind = kind
        self.snapshot = snapshot
    }
}

public struct LiveActivityRideFixtureMatrix: Codable, Equatable, Hashable, Sendable {
    public let fixtures: [LiveActivityRideFixture]

    public init(fixtures: [LiveActivityRideFixture]) {
        self.fixtures = fixtures
    }
}

public extension LiveActivityRideFixtureMatrix {
    static let v1 = Self(fixtures: [
        .init(kind: .demo, snapshot: .fixture(
            identity: .fixture(label: "Demo ride"),
            speed: .available(label: "Speed", value: "19 mph", unit: "mph", source: .fixture),
            battery: .available(label: "Battery", value: "82%", unit: "%", source: .fixture),
            packVoltage: .available(label: "Pack voltage", value: "112.4 V", unit: "V", source: .fixture),
            pwm: .available(label: "PWM", value: "41%", unit: "%", source: .fixture),
            mode: .available(label: "Mode", value: "Ride", unit: nil, source: .fixture),
            duration: .available(label: "Duration", value: "12:48", unit: nil, source: .fixture),
            distance: .available(label: "Distance", value: "4.2 mi", unit: "mi", source: .fixture),
            headroom: .available(label: "Headroom", value: "59%", unit: "%", source: .fixture),
            beeps: .available(label: "Beeps", value: "2", unit: nil, source: .fixture),
            temperature: .available(label: "Temperature", value: "31 C", unit: "C", source: .fixture)
        )),
        .init(kind: .populated, snapshot: .fixture(
            identity: .fixture(label: "Aero-126V"),
            speed: .available(label: "Speed", value: "31 mph", unit: "mph", source: .fixture),
            battery: .available(label: "Battery", value: "74%", unit: "%", source: .fixture),
            packVoltage: .available(label: "Pack voltage", value: "115.8 V", unit: "V", source: .fixture),
            pwm: .available(label: "PWM", value: "77%", unit: "%", source: .fixture),
            mode: .available(label: "Mode", value: "Ride", unit: nil, source: .fixture),
            duration: .available(label: "Duration", value: "38:12", unit: nil, source: .fixture),
            distance: .available(label: "Distance", value: "14.2 mi", unit: "mi", source: .fixture),
            headroom: .available(label: "Headroom", value: "23%", unit: "%", source: .fixture),
            beeps: .available(label: "Beeps", value: "1", unit: nil, source: .fixture),
            temperature: .available(label: "Temperature", value: "61 C", unit: "C", source: .fixture)
        )),
        .init(kind: .partial, snapshot: .init(
            identity: .fixture(label: "Partial ride"),
            connectionState: .fixture,
            sessionStatus: .available(label: "Status", value: "Live", unit: nil, source: .fixture),
            speed: .available(label: "Speed", value: "13 mph", unit: "mph", source: .fixture),
            battery: .deferred(label: "Battery", unit: "%"),
            packVoltage: .available(label: "Pack voltage", value: "109.2 V", unit: "V", source: .fixture),
            pwm: .available(label: "PWM", value: "48%", unit: "%", source: .fixture),
            mode: .deferred(label: "Mode"),
            duration: .available(label: "Duration", value: "08:03", unit: nil, source: .fixture),
            distance: .available(label: "Distance", value: "2.1 mi", unit: "mi", source: .fixture),
            headroom: .deferred(label: "Headroom", unit: "%"),
            beeps: .available(label: "Beeps", value: "0", unit: nil, source: .fixture),
            temperature: .available(label: "Temperature", value: "29 C", unit: "C", source: .fixture)
        )),
        .init(kind: .waitingForFirstTelemetry, snapshot: .init(
            identity: .fixture(label: "Waiting for telemetry"),
            connectionState: .waitingForFirstTelemetry,
            sessionStatus: .unavailable(label: "Status"),
            speed: .unavailable(label: "Speed", unit: "mph"),
            battery: .unavailable(label: "Battery", unit: "%"),
            packVoltage: .unavailable(label: "Pack voltage", unit: "V"),
            pwm: .unavailable(label: "PWM", unit: "%"),
            mode: .deferred(label: "Mode"),
            duration: .deferred(label: "Duration"),
            distance: .deferred(label: "Distance", unit: "mi"),
            headroom: .unavailable(label: "Headroom", unit: "%"),
            beeps: .deferred(label: "Beeps"),
            temperature: .unavailable(label: "Temperature", unit: "C")
        )),
        .init(kind: .stale, snapshot: .init(
            identity: .fixture(label: "Stale ride"),
            connectionState: .stale,
            sessionStatus: .available(label: "Status", value: "Stale", unit: nil, source: .fixture),
            speed: .stale(label: "Speed", value: "21 mph", unit: "mph", source: .fixture),
            battery: .stale(label: "Battery", value: "67%", unit: "%", source: .fixture),
            packVoltage: .stale(label: "Pack voltage", value: "107.9 V", unit: "V", source: .fixture),
            pwm: .stale(label: "PWM", value: "52%", unit: "%", source: .fixture),
            mode: .stale(label: "Mode", value: "Ride", unit: nil, source: .fixture),
            duration: .stale(label: "Duration", value: "44:09", unit: nil, source: .fixture),
            distance: .stale(label: "Distance", value: "16.8 mi", unit: "mi", source: .fixture),
            headroom: .stale(label: "Headroom", value: "18%", unit: "%", source: .fixture),
            beeps: .stale(label: "Beeps", value: "3", unit: nil, source: .fixture),
            temperature: .stale(label: "Temperature", value: "58 C", unit: "C", source: .fixture)
        )),
        .init(kind: .disconnected, snapshot: .init(
            identity: .fixture(label: "Disconnected ride"),
            connectionState: .disconnected,
            sessionStatus: .unavailable(label: "Status"),
            speed: .unavailable(label: "Speed", unit: "mph"),
            battery: .unavailable(label: "Battery", unit: "%"),
            packVoltage: .unavailable(label: "Pack voltage", unit: "V"),
            pwm: .unavailable(label: "PWM", unit: "%"),
            mode: .deferred(label: "Mode"),
            duration: .deferred(label: "Duration"),
            distance: .deferred(label: "Distance", unit: "mi"),
            headroom: .unavailable(label: "Headroom", unit: "%"),
            beeps: .deferred(label: "Beeps"),
            temperature: .unavailable(label: "Temperature", unit: "C")
        )),
        .init(kind: .parked, snapshot: .init(
            identity: .fixture(label: "Parked ride"),
            connectionState: .connected,
            sessionStatus: .available(label: "Status", value: "Parked", unit: nil, source: .fixture),
            speed: .notApplicable(label: "Speed", unit: "mph"),
            battery: .available(label: "Battery", value: "91%", unit: "%", source: .fixture),
            packVoltage: .available(label: "Pack voltage", value: "116.1 V", unit: "V", source: .fixture),
            pwm: .notApplicable(label: "PWM", unit: "%"),
            mode: .available(label: "Mode", value: "Parked", unit: nil, source: .fixture),
            duration: .notApplicable(label: "Duration"),
            distance: .notApplicable(label: "Distance", unit: "mi"),
            headroom: .notApplicable(label: "Headroom", unit: "%"),
            beeps: .notApplicable(label: "Beeps"),
            temperature: .available(label: "Temperature", value: "27 C", unit: "C", source: .fixture)
        )),
    ])
}
