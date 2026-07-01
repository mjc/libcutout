import Foundation

public enum MockupScreenID: String, CaseIterable, Equatable, Hashable, Sendable {
    case devicePicker
    case eucRide
    case eucGarage
    case vescOnewheelRide
    case vescDebug
}

public struct MockupMetric: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

public enum MockupPickerRowState: Equatable, Hashable, Sendable {
    case supported(action: String)
    case unsupported(action: String)
    case manual(action: String)
}

public struct MockupPickerRow: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { title }

    public let title: String
    public let subtitle: String
    public let detail: String
    public let state: MockupPickerRowState
    public let symbolName: String

    public init(
        title: String,
        subtitle: String,
        detail: String,
        state: MockupPickerRowState,
        symbolName: String
    ) {
        self.title = title
        self.subtitle = subtitle
        self.detail = detail
        self.state = state
        self.symbolName = symbolName
    }
}

public struct MockupScreen: Equatable, Hashable, Sendable, Identifiable {
    public let id: MockupScreenID
    public let title: String
    public let subtitle: String
    public let primaryValue: String
    public let secondaryValue: String
    public let warning: String?
    public let metrics: [MockupMetric]
    public let pickerRows: [MockupPickerRow]
    public let isFixtureOnly: Bool

    public init(
        id: MockupScreenID,
        title: String,
        subtitle: String,
        primaryValue: String,
        secondaryValue: String,
        warning: String?,
        metrics: [MockupMetric],
        pickerRows: [MockupPickerRow] = [],
        isFixtureOnly: Bool = true
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.primaryValue = primaryValue
        self.secondaryValue = secondaryValue
        self.warning = warning
        self.metrics = metrics
        self.pickerRows = pickerRows
        self.isFixtureOnly = isFixtureOnly
    }
}

public struct MockupScreenCatalog: Equatable, Hashable, Sendable {
    public let screens: [MockupScreen]

    public init(screens: [MockupScreen]) {
        self.screens = screens
    }

    public func screen(id: MockupScreenID) -> MockupScreen? {
        screens.first { $0.id == id }
    }

    public static let v2 = MockupScreenCatalog(screens: [
        MockupScreen(
            id: .devicePicker,
            title: "Device picker",
            subtitle: "Scanning Bluetooth",
            primaryValue: "Aero-126V",
            secondaryValue: "Little FOCer BT",
            warning: "Unsupported rows remain disabled fixtures.",
            metrics: [
                MockupMetric(label: "Supported EUC", value: "Aero-126V"),
                MockupMetric(label: "Supported VESC OW", value: "Little FOCer BT"),
                MockupMetric(label: "Unsupported", value: "NINEBOT-7A31"),
                MockupMetric(label: "Unsupported", value: "HX Hoverboard"),
                MockupMetric(label: "Manual add", value: "disabled"),
            ],
            pickerRows: [
                MockupPickerRow(
                    title: "Aero-126V",
                    subtitle: "Electric unicycle - telemetry profile found",
                    detail: "126.0 V - strong signal",
                    state: .supported(action: "Pair"),
                    symbolName: "circle.hexagongrid.circle"
                ),
                MockupPickerRow(
                    title: "Little FOCer BT",
                    subtitle: "VESC Onewheel - UART bridge detected",
                    detail: "75.4 V - moderate signal",
                    state: .supported(action: "Pair"),
                    symbolName: "oval.portrait"
                ),
                MockupPickerRow(
                    title: "NINEBOT-7A31",
                    subtitle: "Electric scooter - known BLE advertisement",
                    detail: "We can learn this later",
                    state: .unsupported(action: "Not yet"),
                    symbolName: "scooter"
                ),
                MockupPickerRow(
                    title: "HX Hoverboard",
                    subtitle: "Hoverboard / self-balancing board candidate",
                    detail: "Capture wizard later",
                    state: .unsupported(action: "Not yet"),
                    symbolName: "capsule"
                ),
                MockupPickerRow(
                    title: "Manual add / record unknown device",
                    subtitle: "",
                    detail: "",
                    state: .manual(action: "later"),
                    symbolName: "plus"
                ),
            ]
        ),
        MockupScreen(
            id: .eucRide,
            title: "Aero-126V",
            subtitle: "EUC - riding",
            primaryValue: "31 mph",
            secondaryValue: "PWM headroom 23%",
            warning: "Reduce acceleration - voltage sag under load: 9.4 V",
            metrics: [
                MockupMetric(label: "sag-adjusted energy", value: "62%"),
                MockupMetric(label: "pack", value: "115.8 V"),
                MockupMetric(label: "power", value: "4.2 kW"),
                MockupMetric(label: "thermal", value: "61 C"),
                MockupMetric(label: "limp-home", value: "14.2 mi"),
            ]
        ),
        MockupScreen(
            id: .eucGarage,
            title: "EUC health",
            subtitle: "Stationary diagnostics for wheel-specific data",
            primaryValue: "battery 85%",
            secondaryValue: "pack 115.8 V",
            warning: nil,
            metrics: [
                MockupMetric(label: "beep margin", value: "11.6 mph"),
                MockupMetric(label: "tiltback", value: "42 mph"),
                MockupMetric(label: "pedal mode", value: "72%"),
                MockupMetric(label: "cell delta", value: "0.018 V"),
                MockupMetric(label: "last fault", value: "none"),
            ]
        ),
        MockupScreen(
            id: .vescOnewheelRide,
            title: "Fungineers X7",
            subtitle: "VESC OW - armed",
            primaryValue: "19 mph",
            secondaryValue: "Duty headroom 18%",
            warning: "Pushback soon - duty and pack sag are both climbing.",
            metrics: [
                MockupMetric(label: "battery current", value: "38 A"),
                MockupMetric(label: "motor current", value: "71 A"),
                MockupMetric(label: "board angle", value: "-1.8 deg"),
                MockupMetric(label: "controller", value: "54 C"),
                MockupMetric(label: "motor", value: "49 C"),
            ]
        ),
        MockupScreen(
            id: .vescDebug,
            title: "VESC state",
            subtitle: "For tuning/debug. Not the riding screen",
            primaryValue: "duty cycle 82%",
            secondaryValue: "pack 75.4 V",
            warning: "Dangerous writes hidden until parked and confirmed.",
            metrics: [
                MockupMetric(label: "battery limit", value: "45 A"),
                MockupMetric(label: "motor limit", value: "90 A"),
                MockupMetric(label: "last fault", value: "FAULT_CODE_NONE"),
                MockupMetric(label: "input app", value: "ADC + balance"),
                MockupMetric(label: "logging", value: "local CSV armed"),
            ]
        ),
    ])
}
