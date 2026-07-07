import CutoutMobile
import SwiftUI

struct CardBackground: View {
    let cornerRadius: CGFloat

    init(cornerRadius: CGFloat = 22) {
        self.cornerRadius = cornerRadius
    }

    var body: some View {
        PevDashboardCardBackground(
            cornerRadius: cornerRadius,
            fill: MockupColors.cardFill,
            stroke: MockupColors.cardStroke
        )
    }
}

extension DevicePickerRow {
    var glyphColor: Color {
        switch glyphKind {
        case .scooter:
            MockupColors.teal
        case .hoverboard:
            MockupColors.brown
        case .electricUnicycle, .onewheel, .systemSymbol:
            MockupColors.yellow
        }
    }

    var glyphBackground: Color {
        glyphColor.opacity(isSupported ? 0.12 : 0.16)
    }

    var titleColor: Color {
        isSupported ? MockupColors.primaryText : MockupColors.disabledText
    }

    var secondaryTextColor: Color {
        isSupported ? MockupColors.muted : MockupColors.disabledSecondaryText
    }
}

enum MockupColors {
    static let pageBackground = Color(red: 0.027, green: 0.031, blue: 0.043)
    static let cardFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let cardStroke = Color(red: 0.165, green: 0.188, blue: 0.239)
    static let disabledFill = Color(red: 0.067, green: 0.078, blue: 0.106)
    static let primaryText = Color(red: 0.969, green: 0.953, blue: 0.918)
    static let disabledText = Color(red: 0.455, green: 0.475, blue: 0.514)
    static let disabledSecondaryText = Color(red: 0.36, green: 0.38, blue: 0.42)
    static let muted = Color(red: 0.561, green: 0.596, blue: 0.659)
    static let yellow = Color(red: 1.0, green: 0.827, blue: 0.302)
    static let cyan = Color(red: 0.278, green: 0.824, blue: 0.933)
    static let green = Color(red: 0.376, green: 0.906, blue: 0.553)
    static let orange = Color(red: 1.0, green: 0.486, blue: 0.188)
    static let red = Color(red: 1.0, green: 0.243, blue: 0.243)
    static let warningText = Color(red: 1.0, green: 0.667, blue: 0.345)
    static let warningFill = Color(red: 0.173, green: 0.087, blue: 0.040)
    static let warningStroke = Color(red: 0.443, green: 0.216, blue: 0.102)
    static let teal = Color(red: 0.180, green: 0.384, blue: 0.459)
    static let brown = Color(red: 0.443, green: 0.259, blue: 0.141)
    static let purple = Color(red: 0.635, green: 0.459, blue: 0.918)
    static let iconFill = Color(red: 0.043, green: 0.051, blue: 0.071)
}

extension MockupAccent {
    var color: Color {
        switch self {
        case .cyan:
            MockupColors.cyan
        case .green:
            MockupColors.green
        case .orange:
            MockupColors.orange
        case .purple:
            MockupColors.purple
        case .yellow:
            MockupColors.yellow
        }
    }
}

extension DevicePickerRowState {
    var actionTitle: String {
        switch self {
        case .supported(let action), .probeRecommended(let action), .unsupported(let action), .manual(let action):
            action
        }
    }

    var isSupported: Bool {
        if case .supported = self { true } else { false }
    }
}

func liveSafetyBars(for state: EucRideScreenState) -> [MockupSafetyBar] {
    [
        state.pwmHeadroomPermille.map { headroomPermille in
            return MockupSafetyBar(
                label: "PWM headroom",
                value: percentageString(fromPermille: headroomPermille),
                progress: Double(headroomPermille) / 1_000.0,
                accent: .yellow
            )
        } ?? MockupSafetyBar(
            label: "PWM headroom",
            value: state.pwmHeadroomApplicability == .notApplicable ? "Not applicable" : "Unavailable",
            progress: 0,
            accent: .yellow
        ),
        MockupSafetyBar(label: "sag-adjusted energy", value: "Unavailable", progress: 0, accent: .cyan),
    ]
}

func liveDashboardTiles(from state: EucRideScreenState, telemetry: TelemetrySnapshot) -> [MockupDashboardTile] {
    let distanceUnit = RideUnits.distanceUnit(forSpeedUnit: state.speedUnit)
    return [
        telemetry.voltage.map { voltage in
            MockupDashboardTile(
                label: "pack",
                value: decimalString(fromMillivolts: voltage.value, fractionDigits: 1),
                unit: "V",
                detail: state.voltageSag.map {
                    decimalString(fromMillivolts: $0.value, fractionDigits: 1) + " V sag"
                } ?? "sag unavailable",
                accent: .cyan
            )
        } ?? MockupDashboardTile(label: "pack", value: "--", unit: "V", detail: "unavailable", accent: .cyan),
        livePowerTile(from: telemetry),
        (telemetry.controllerTemperature != nil || telemetry.motorTemperature != nil || telemetry.batteryTemperature != nil)
            ? MockupDashboardTile(
                label: "thermal",
                value: liveThermalValue(telemetry: telemetry),
                unit: "°C",
                detail: liveThermalDetail(telemetry: telemetry),
                accent: .green
            )
            : MockupDashboardTile(label: "thermal", value: "--", unit: "°C", detail: "unavailable", accent: .green),
        state.limpHomeRange.map { range in
            MockupDashboardTile(
                label: "limp-home",
                value: decimalString(fromMillimetres: range.value, unit: distanceUnit, fractionDigits: 1),
                unit: distanceUnit,
                detail: "typed range estimate",
                accent: .cyan
            )
        } ?? MockupDashboardTile(label: "limp-home", value: "--", unit: distanceUnit, detail: "unavailable", accent: .cyan),
    ]
}

func livePowerTile(from telemetry: TelemetrySnapshot) -> MockupDashboardTile {
    if let voltage = telemetry.voltage,
       let current = telemetry.batteryCurrent,
       current.value != 0 {
        let milliwatts = Int64(voltage.value) * Int64(current.value) / 1_000
        return MockupDashboardTile(
            label: "power",
            value: decimalString(
                fromMilliwatts: milliwatts,
                fractionDigits: powerFractionDigits(fromMilliwatts: milliwatts)
            ),
            unit: "kW",
            detail: powerFlowDetail(telemetry.powerFlow, fallback: "calculated from pack current"),
            accent: .yellow
        )
    }

    if let power = telemetry.power {
        return MockupDashboardTile(
            label: "power",
            value: decimalString(
                fromMilliwatts: power.value,
                fractionDigits: powerFractionDigits(fromMilliwatts: power.value)
            ),
            unit: "kW",
            detail: powerFlowDetail(telemetry.powerFlow, fallback: "live telemetry"),
            accent: .yellow
        )
    }

    return MockupDashboardTile(label: "power", value: "--", unit: "kW", detail: "unavailable", accent: .yellow)
}

func powerFlowDetail(_ direction: PowerFlowDirection?, fallback: String) -> String {
    switch direction {
    case .discharge:
        fallback
    case .zero:
        "zero signed pack flow"
    case .charging:
        "charging input"
    case .regeneration:
        "regeneration"
    case .negativeUnknown:
        "negative signed flow; charge/regen unverified"
    case nil:
        fallback
    }
}

func unavailableSafetyBars(from bars: [MockupSafetyBar]) -> [MockupSafetyBar] {
    bars.map {
        MockupSafetyBar(label: $0.label, value: "Unavailable", progress: 0, accent: $0.accent)
    }
}

func unavailableDashboardTiles(from tiles: [MockupDashboardTile]) -> [MockupDashboardTile] {
    tiles.map {
        MockupDashboardTile(label: $0.label, value: "--", unit: $0.unit, detail: "unavailable", accent: $0.accent)
    }
}

func liveThermalValue(telemetry: TelemetrySnapshot) -> String {
    let values = [telemetry.controllerTemperature, telemetry.motorTemperature, telemetry.batteryTemperature]
        .compactMap { $0?.value }
    guard let maxValue = values.max() else {
        return "--"
    }
    return decimalString(fromMillicelsius: maxValue, fractionDigits: 0)
}

func liveThermalDetail(telemetry: TelemetrySnapshot) -> String {
    let parts = [
        telemetry.controllerTemperature.map {
            "ESC " + decimalString(fromMillicelsius: $0.value, fractionDigits: 0) + " " + RideUnits.temperatureUnit
        },
        telemetry.motorTemperature.map {
            "motor " + decimalString(fromMillicelsius: $0.value, fractionDigits: 0) + " " + RideUnits.temperatureUnit
        },
        telemetry.batteryTemperature.map {
            "battery " + decimalString(fromMillicelsius: $0.value, fractionDigits: 0) + " " + RideUnits.temperatureUnit
        },
    ].compactMap { $0 }
    return parts.isEmpty ? "typed telemetry" : parts.joined(separator: " · ")
}

func percentageString<T: BinaryInteger>(fromPercent percent: T) -> String {
    RideUnits.percentText(percent) + "%"
}

func percentageString<T: BinaryInteger>(fromPermille permille: T) -> String {
    RideUnits.permillePercentText(permille) + "%"
}

func decimalString<T: BinaryInteger>(fromMillivolts value: T, fractionDigits: Int) -> String {
    RideUnits.voltageText(millivolts: value, fractionDigits: fractionDigits)
}

func decimalString<T: BinaryInteger>(fromMilliwatts value: T, fractionDigits: Int) -> String {
    RideUnits.powerText(milliwatts: value, fractionDigits: fractionDigits)
}

func powerFractionDigits<T: BinaryInteger>(fromMilliwatts value: T) -> Int {
    abs(Int64(value)) < 1_000_000 ? 2 : 1
}

func decimalString<T: BinaryInteger>(fromMillicelsius value: T, fractionDigits: Int) -> String {
    RideUnits.temperatureText(millicelsius: value, fractionDigits: fractionDigits)
}

func decimalString<T: BinaryInteger>(fromMillimetres value: T, unit: String, fractionDigits: Int) -> String {
    RideUnits.distanceText(millimetres: value, unit: unit, fractionDigits: fractionDigits)
}

func decimalString(_ value: Double, fractionDigits: Int) -> String {
    RideUnits.decimalString(value, fractionDigits: fractionDigits)
}
