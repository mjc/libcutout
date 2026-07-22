import CutoutMobile
import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

private enum PevSystemColors {
    #if os(iOS)
    static let pageBackground = Color(uiColor: .systemBackground)
    static let cardFill = Color(uiColor: .secondarySystemBackground)
    static let disabledFill = pageBackground
    static let yellow = Color(uiColor: .systemYellow)
    static let cyan = Color(uiColor: .systemCyan)
    static let green = Color(uiColor: .systemGreen)
    static let orange = Color(uiColor: .systemOrange)
    static let red = Color(uiColor: .systemRed)
    static let teal = Color(uiColor: .systemTeal)
    static let brown = Color(uiColor: .systemBrown)
    static let purple = Color(uiColor: .systemPurple)
    static let primaryText = Color(uiColor: .label)
    static let mutedText = Color(uiColor: .label)
    static let brand = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
            : UIColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
    })
    #elseif os(macOS)
    static let pageBackground = Color(nsColor: .windowBackgroundColor)
    static let cardFill = Color(nsColor: .underPageBackgroundColor)
    static let disabledFill = pageBackground
    static let yellow = Color(nsColor: .systemYellow)
    static let cyan = Color(nsColor: .systemCyan)
    static let green = Color(nsColor: .systemGreen)
    static let orange = Color(nsColor: .systemOrange)
    static let red = Color(nsColor: .systemRed)
    static let teal = Color(nsColor: .systemTeal)
    static let brown = Color(nsColor: .systemBrown)
    static let purple = Color(nsColor: .systemPurple)
    static let primaryText = Color(nsColor: .labelColor)
    static let mutedText = Color(nsColor: .labelColor)
    static let brand = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
            : NSColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
    })
    #endif
}

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
    // Semantic system colors keep the visual hierarchy intact while honoring
    // the user's light/dark appearance and contrast settings.
    static let pageBackground = PevSystemColors.pageBackground
    static let cardFill = PevSystemColors.cardFill
    static let cardStroke = Color.secondary.opacity(0.35)
    static let disabledFill = PevSystemColors.disabledFill
    static let primaryText = PevSystemColors.primaryText
    static let disabledText = Color.primary.opacity(0.58)
    static let disabledSecondaryText = Color.primary.opacity(0.48)
    static let muted = PevSystemColors.mutedText
    static let brand = PevSystemColors.brand
    static let yellow = PevSystemColors.yellow
    static let cyan = PevSystemColors.cyan
    static let green = PevSystemColors.green
    static let orange = PevSystemColors.orange
    static let red = PevSystemColors.red
    static let warningText = PevSystemColors.orange
    static let warningFill = PevSystemColors.orange.opacity(0.14)
    static let warningStroke = PevSystemColors.orange.opacity(0.55)
    static let teal = PevSystemColors.teal
    static let brown = PevSystemColors.brown
    static let purple = PevSystemColors.purple
    static let iconFill = PevSystemColors.disabledFill
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
            metricValue: tile.metricValue,
            unit: tile.unit,
            detail: tile.detail,
            accent: tile.accent.color,
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
                id: .pwmHeadroom,
                label: "PWM headroom",
                value: percentageString(fromPermille: headroomPermille),
                progress: Double(headroomPermille) / 1_000.0,
                accent: .yellow
            )
        } ?? PevSafetyBar(
            id: .pwmHeadroom,
            label: "PWM headroom",
            value: state.pwmHeadroomApplicability == .notApplicable ? "Not applicable" : "Unavailable",
            progress: 0,
            accent: .yellow
        ),
        PevSafetyBar(
            id: .sagAdjustedEnergy,
            label: "sag-adjusted energy",
            value: "Unavailable",
            progress: 0,
            accent: .cyan
        ),
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
        } ?? PevDashboardTile(kind: .packVoltage, label: "pack", metricValue: .unavailable, unit: "V", detail: "unavailable", accent: .cyan),
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
            : PevDashboardTile(kind: .thermal, label: "thermal", metricValue: .unavailable, unit: "°C", detail: "unavailable", accent: .green),
        state.limpHomeRange.map { range in
            PevDashboardTile(
                kind: .limpHomeRange,
                label: "limp-home",
                value: decimalString(fromMillimetres: range.value, unit: distanceUnit, fractionDigits: 1),
                unit: distanceUnit,
                detail: "typed range estimate",
                accent: .cyan
            )
        } ?? PevDashboardTile(kind: .limpHomeRange, label: "limp-home", metricValue: .unavailable, unit: distanceUnit, detail: "unavailable", accent: .cyan),
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

    return PevDashboardTile(kind: .power, label: "power", metricValue: .unavailable, unit: "kW", detail: "unavailable", accent: .yellow)
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
        PevSafetyBar(id: $0.id, label: $0.label, value: "Unavailable", progress: 0, accent: $0.accent)
    }
}

func unavailableDashboardTiles(from tiles: [PevDashboardTile]) -> [PevDashboardTile] {
    tiles.map {
        PevDashboardTile(kind: $0.kind, label: $0.label, metricValue: .unavailable, unit: $0.unit, detail: "unavailable", accent: $0.accent)
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
