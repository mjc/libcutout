import CutoutMobile
import SwiftUI

extension DevicePickerRow {
    var glyphColor: Color {
        switch glyphKind {
        case .scooter:
            PevColors.teal
        case .hoverboard:
            PevColors.brown
        case .electricUnicycle, .onewheel, .systemSymbol:
            PevColors.yellow
        }
    }

    var glyphBackground: Color {
        glyphColor.opacity(isSupported ? 0.12 : 0.16)
    }

    var titleColor: Color {
        isSupported ? PevColors.primaryText : PevColors.disabledText
    }

    var secondaryTextColor: Color {
        isSupported ? PevColors.muted : PevColors.disabledSecondaryText
    }
}

enum PevColors {
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

extension PevAccent {
    var color: Color {
        switch self {
        case .cyan:
            PevColors.cyan
        case .green:
            PevColors.green
        case .orange:
            PevColors.orange
        case .purple:
            PevColors.purple
        case .yellow:
            PevColors.yellow
        }
    }
}

extension PevDashboardMetricTile {
    init(
        _ tile: PevDashboardTile,
        scale: CGFloat,
        fill: Color = PevDashboardColors.cardFill,
        stroke: Color = PevDashboardColors.cardStroke,
        labelColor: Color = PevDashboardColors.mutedText,
        valueColor: Color = PevDashboardColors.primaryText,
        unitColor: Color? = nil,
        detailColor: Color = PevDashboardColors.mutedText,
        cornerRadius: CGFloat = 20,
        minHeight: CGFloat = 106
    ) {
        self.init(
            label: tile.label,
            value: tile.value,
            unit: tile.unit,
            detail: tile.detail,
            accent: tile.accent.color,
            scale: scale,
            fill: fill,
            stroke: stroke,
            labelColor: labelColor,
            valueColor: valueColor,
            unitColor: unitColor,
            detailColor: detailColor,
            cornerRadius: cornerRadius,
            minHeight: minHeight
        )
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

func liveSafetyBars(for state: EucRideScreenState) -> [PevSafetyBar] {
    [
        state.pwmHeadroomPermille.map { headroomPermille in
            return PevSafetyBar(
                label: "PWM headroom",
                value: percentageString(fromPermille: headroomPermille),
                progress: Double(headroomPermille) / 1_000.0,
                accent: .yellow
            )
        } ?? PevSafetyBar(
            label: "PWM headroom",
            value: state.pwmHeadroomApplicability == .notApplicable ? "Not applicable" : "Unavailable",
            progress: 0,
            accent: .yellow
        ),
        PevSafetyBar(label: "sag-adjusted energy", value: "Unavailable", progress: 0, accent: .cyan),
    ]
}

func liveDashboardTiles(from state: EucRideScreenState, telemetry: TelemetrySnapshot) -> [PevDashboardTile] {
    let distanceUnit = RideUnits.distanceUnit(forSpeedUnit: state.speedUnit)
    return [
        chargeEstimateTile(from: state),
        telemetry.voltage.map { voltage in
            PevDashboardTile(
                kind: .packVoltage,
                label: "pack",
                value: decimalString(fromMillivolts: voltage.value, fractionDigits: 1),
                unit: "V",
                detail: telemetry.chargeEstimate?.voltageSag.map(voltageSagDetail)
                    ?? "sag unavailable",
                accent: .cyan
            )
        } ?? PevDashboardTile(kind: .packVoltage, label: "pack", value: "--", unit: "V", detail: "unavailable", accent: .cyan),
        livePowerTile(from: telemetry),
        (telemetry.controllerTemperature != nil || telemetry.motorTemperature != nil || telemetry.batteryTemperature != nil)
            ? PevDashboardTile(
                kind: .thermal,
                label: "thermal",
                value: liveThermalValue(telemetry: telemetry),
                unit: "°C",
                detail: liveThermalDetail(telemetry: telemetry),
                accent: .green
            )
            : PevDashboardTile(kind: .thermal, label: "thermal", value: "--", unit: "°C", detail: "unavailable", accent: .green),
        state.limpHomeRange.map { range in
            PevDashboardTile(
                kind: .limpHomeRange,
                label: "limp-home",
                value: decimalString(fromMillimetres: range.value, unit: distanceUnit, fractionDigits: 1),
                unit: distanceUnit,
                detail: "typed range estimate",
                accent: .cyan
            )
        } ?? PevDashboardTile(kind: .limpHomeRange, label: "limp-home", value: "--", unit: distanceUnit, detail: "unavailable", accent: .cyan),
    ]
}

func chargeEstimateTile(from state: EucRideScreenState) -> PevDashboardTile {
    let estimate = state.chargeEstimate
    let detail = if let voltageSag = estimate.voltageSag {
        "\(voltageSagDetail(voltageSag)) · \(estimate.displayDetail)"
    } else {
        estimate.displayDetail
    }
    return PevDashboardTile(
        kind: .chargeEstimate,
        label: "charge",
        value: estimate.displayValue,
        unit: "",
        detail: detail,
        accent: .green
    )
}

func voltageSagDetail(_ sag: ChargeVoltageSagEstimate) -> String {
    let voltage = decimalString(
        fromMillivolts: abs(sag.deltaMillivolts),
        fractionDigits: 1
    )
    let current = RideUnits.currentText(milliamps: abs(sag.loadCurrent.value))
    return "\(voltage) V sag at \(current) A · \(sag.effectiveResistanceMilliohms) mΩ"
}

func livePowerTile(from telemetry: TelemetrySnapshot) -> PevDashboardTile {
    if let voltage = telemetry.voltage,
       let current = telemetry.batteryCurrent,
       current.value != 0 {
        let milliwatts = Int64(voltage.value) * Int64(current.value) / 1_000
        return PevDashboardTile(
            kind: .power,
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
        return PevDashboardTile(
            kind: .power,
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

    return PevDashboardTile(kind: .power, label: "power", value: "--", unit: "kW", detail: "unavailable", accent: .yellow)
}

func powerFlowDetail(_ direction: PowerFlowDirection?, fallback: String) -> String {
    switch direction {
    case .discharge:
        "discharging"
    case .zero:
        "idle"
    case .charging:
        "charging input"
    case .regeneration:
        "regen"
    case .negativeUnknown:
        "regen/discharge unverified"
    case nil:
        fallback
    }
}

func unavailableSafetyBars(from bars: [PevSafetyBar]) -> [PevSafetyBar] {
    bars.map {
        PevSafetyBar(label: $0.label, value: "Unavailable", progress: 0, accent: $0.accent)
    }
}

func unavailableDashboardTiles(from tiles: [PevDashboardTile]) -> [PevDashboardTile] {
    tiles.map {
        PevDashboardTile(kind: $0.kind, label: $0.label, value: "--", unit: $0.unit, detail: "unavailable", accent: $0.accent)
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
