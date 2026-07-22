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
    static let disabledFill = pageBackground
    static let yellow = Color(uiColor: .systemYellow)
    static let cyan = Color(uiColor: .systemCyan)
    static let green = Color(uiColor: .systemGreen)
    static let orange = Color(uiColor: .systemOrange)
    static let red = Color(uiColor: .systemRed)
    static let teal = Color(uiColor: .systemTeal)
    static let brown = Color(uiColor: .systemBrown)
    static let purple = Color(uiColor: .systemPurple)
    static let brand = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
            : UIColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
    })
    #elseif os(macOS)
    static let pageBackground = Color(nsColor: .windowBackgroundColor)
    static let disabledFill = pageBackground
    static let yellow = Color(nsColor: .systemYellow)
    static let cyan = Color(nsColor: .systemCyan)
    static let green = Color(nsColor: .systemGreen)
    static let orange = Color(nsColor: .systemOrange)
    static let red = Color(nsColor: .systemRed)
    static let teal = Color(nsColor: .systemTeal)
    static let brown = Color(nsColor: .systemBrown)
    static let purple = Color(nsColor: .systemPurple)
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
    static let cardFill = PevDashboardColors.cardFill
    static let cardStroke = PevDashboardColors.cardStroke
    static let disabledFill = PevSystemColors.disabledFill
    static let primaryText = PevDashboardColors.primaryText
    static let disabledText = Color.primary.opacity(0.58)
    static let disabledSecondaryText = Color.primary.opacity(0.48)
    static let muted = PevDashboardColors.mutedText
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

func liveSafetyBars(for state: EucRideScreenState) -> [PevSafetyBar] {
    [
        state.pwmHeadroomPermille.map { headroomPermille in
            return PevSafetyBar(
                id: .pwmHeadroom,
                label: localizedAppText("ride.safety.pwm_headroom"),
                value: percentageString(fromPermille: headroomPermille),
                progress: Double(headroomPermille) / 1_000.0,
                accent: .yellow
            )
        } ?? PevSafetyBar(
            id: .pwmHeadroom,
            label: localizedAppText("ride.safety.pwm_headroom"),
            value: state.pwmHeadroomApplicability == .notApplicable
                ? localizedAppText("ride.value.not_applicable")
                : localizedAppText("ride.value.unavailable"),
            progress: 0,
            accent: .yellow
        ),
        PevSafetyBar(
            id: .sagAdjustedEnergy,
            label: localizedAppText("ride.safety.sag_adjusted_energy"),
            value: localizedAppText("ride.value.unavailable"),
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
                label: localizedAppText("ride.metric.pack"),
                value: RideUnits.voltageText(millivolts: voltage.value, fractionDigits: 1),
                unit: "V",
                detail: telemetry.chargeEstimate?.voltageSag.map(voltageSagDetail)
                    ?? localizedAppText("ride.detail.sag_unavailable"),
                accent: .cyan
            )
        } ?? PevDashboardTile(
            kind: .packVoltage,
            label: localizedAppText("ride.metric.pack"),
            metricValue: .unavailable,
            unit: "V",
            detail: localizedAppText("ride.value.unavailable"),
            accent: .cyan
        ),
        livePowerTile(from: telemetry),
        (telemetry.controllerTemperature != nil || telemetry.motorTemperature != nil || telemetry.batteryTemperature != nil)
            ? PevDashboardTile(
                kind: .thermal,
                label: localizedAppText("ride.metric.thermal"),
                value: liveThermalValue(telemetry: telemetry),
                unit: "°C",
                detail: liveThermalDetail(telemetry: telemetry),
                accent: .green
            )
            : PevDashboardTile(
                kind: .thermal,
                label: localizedAppText("ride.metric.thermal"),
                metricValue: .unavailable,
                unit: "°C",
                detail: localizedAppText("ride.value.unavailable"),
                accent: .green
            ),
        state.limpHomeRange.map { range in
            PevDashboardTile(
                kind: .limpHomeRange,
                label: localizedAppText("ride.metric.limp_home"),
                value: RideUnits.distanceText(millimetres: range.value, unit: distanceUnit, fractionDigits: 1),
                unit: distanceUnit,
                detail: localizedAppText("ride.detail.typed_range_estimate"),
                accent: .cyan
            )
        } ?? PevDashboardTile(
            kind: .limpHomeRange,
            label: localizedAppText("ride.metric.limp_home"),
            metricValue: .unavailable,
            unit: distanceUnit,
            detail: localizedAppText("ride.value.unavailable"),
            accent: .cyan
        ),
    ]
}

func chargeEstimateTile(from state: EucRideScreenState) -> PevDashboardTile {
    let estimate = state.chargeEstimate
    let detail = if let voltageSag = estimate.voltageSag {
        localizedAppText(
            "ride.charge.detail.with_voltage_sag",
            voltageSagDetail(voltageSag),
            estimate.displayDetail
        )
    } else {
        estimate.displayDetail
    }
    return PevDashboardTile(
        kind: .chargeEstimate,
        label: localizedAppText("ride.metric.charge"),
        value: estimate.displayValue,
        unit: "",
        detail: detail,
        accent: .green
    )
}

func voltageSagDetail(_ sag: ChargeVoltageSagEstimate) -> String {
    let voltage = RideUnits.voltageText(
        millivolts: abs(sag.deltaMillivolts),
        fractionDigits: 1
    )
    let current = RideUnits.currentText(milliamps: abs(sag.loadCurrent.value))
    return localizedAppText(
        "ride.sag.detail",
        voltage,
        current,
        Int64(sag.effectiveResistanceMilliohms)
    )
}

func livePowerTile(from telemetry: TelemetrySnapshot) -> PevDashboardTile {
    if let voltage = telemetry.voltage,
       let current = telemetry.batteryCurrent,
       current.value != 0 {
        let milliwatts = Int64(voltage.value) * Int64(current.value) / 1_000
        return PevDashboardTile(
            kind: .power,
            label: localizedAppText("ride.metric.power"),
            value: RideUnits.powerText(
                milliwatts: milliwatts,
                fractionDigits: powerFractionDigits(fromMilliwatts: milliwatts)
            ),
            unit: "kW",
            detail: powerFlowDetail(
                telemetry.powerFlow,
                fallback: localizedAppText("ride.power.calculated_pack_current")
            ),
            accent: .yellow
        )
    }

    if let power = telemetry.power {
        return PevDashboardTile(
            kind: .power,
            label: localizedAppText("ride.metric.power"),
            value: RideUnits.powerText(
                milliwatts: power.value,
                fractionDigits: powerFractionDigits(fromMilliwatts: power.value)
            ),
            unit: "kW",
            detail: powerFlowDetail(
                telemetry.powerFlow,
                fallback: localizedAppText("ride.power.live_telemetry")
            ),
            accent: .yellow
        )
    }

    return PevDashboardTile(
        kind: .power,
        label: localizedAppText("ride.metric.power"),
        metricValue: .unavailable,
        unit: "kW",
        detail: localizedAppText("ride.value.unavailable"),
        accent: .yellow
    )
}

func powerFlowDetail(_ direction: PowerFlowDirection?, fallback: String) -> String {
    switch direction {
    case .discharge:
        localizedAppText("telemetry.power_flow.discharge")
    case .zero:
        localizedAppText("telemetry.power_flow.zero")
    case .charging:
        localizedAppText("telemetry.power_flow.charging")
    case .regeneration:
        localizedAppText("telemetry.power_flow.regeneration")
    case .negativeUnknown:
        localizedAppText("telemetry.power_flow.negative_unknown")
    case nil:
        fallback
    }
}

func liveThermalValue(telemetry: TelemetrySnapshot) -> String {
    let values = [telemetry.controllerTemperature, telemetry.motorTemperature, telemetry.batteryTemperature]
        .compactMap { $0?.value }
    guard let maxValue = values.max() else {
        return "--"
    }
    return RideUnits.temperatureText(millicelsius: maxValue, fractionDigits: 0)
}

func liveThermalDetail(telemetry: TelemetrySnapshot) -> String {
    let controller = telemetry.controllerTemperature.map {
        RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 0)
    }
    let motor = telemetry.motorTemperature.map {
        RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 0)
    }
    let battery = telemetry.batteryTemperature.map {
        RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 0)
    }
    let unit = RideUnits.temperatureUnit

    switch (controller, motor, battery) {
    case let (.some(controller), .some(motor), .some(battery)):
        return localizedAppText(
            "ride.thermal.all",
            controller,
            unit,
            motor,
            unit,
            battery,
            unit
        )
    case let (.some(controller), .some(motor), nil):
        return localizedAppText("ride.thermal.controller_motor", controller, unit, motor, unit)
    case let (.some(controller), nil, .some(battery)):
        return localizedAppText("ride.thermal.controller_battery", controller, unit, battery, unit)
    case let (nil, .some(motor), .some(battery)):
        return localizedAppText("ride.thermal.motor_battery", motor, unit, battery, unit)
    case let (.some(controller), nil, nil):
        return localizedAppText("ride.thermal.controller", controller, unit)
    case let (nil, .some(motor), nil):
        return localizedAppText("ride.thermal.motor", motor, unit)
    case let (nil, nil, .some(battery)):
        return localizedAppText("ride.thermal.battery", battery, unit)
    case (nil, nil, nil):
        return localizedAppText("ride.detail.typed_telemetry")
    }
}

func percentageString<T: BinaryInteger>(fromPercent percent: T) -> String {
    localizedAppText("ride.value.percent", RideUnits.percentText(percent))
}

func percentageString<T: BinaryInteger>(fromPermille permille: T) -> String {
    localizedAppText("ride.value.percent", RideUnits.permillePercentText(permille))
}

func powerFractionDigits<T: BinaryInteger>(fromMilliwatts value: T) -> Int {
    abs(Int64(value)) < 1_000_000 ? 2 : 1
}
