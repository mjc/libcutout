import CutoutMobile
import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

private enum PevSystemColors {
    #if os(iOS)
    static let brand = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 1.0, green: 0.84, blue: 0.15, alpha: 1)
            : UIColor(red: 0.48, green: 0.29, blue: 0.0, alpha: 1)
    })
    #elseif os(macOS)
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
    static let pageBackground = PevDashboardColors.pageBackground
    static let cardFill = PevDashboardColors.cardFill
    static let cardStroke = PevDashboardColors.cardStroke
    static let disabledFill = PevDashboardColors.disabledFill
    static let primaryText = PevDashboardColors.primaryText
    static let disabledText = Color.primary.opacity(0.58)
    static let disabledSecondaryText = Color.primary.opacity(0.48)
    static let muted = PevDashboardColors.mutedText
    static let brand = PevSystemColors.brand
    static let yellow = PevDashboardColors.yellow
    static let cyan = PevDashboardColors.cyan
    static let green = PevDashboardColors.green
    static let orange = PevDashboardColors.orange
    static let red = PevDashboardColors.red
    static let warningText = PevDashboardColors.warningText
    static let warningFill = PevDashboardColors.warningFill
    static let warningStroke = PevDashboardColors.warningStroke
    static let teal = PevDashboardColors.teal
    static let brown = PevDashboardColors.brown
    static let purple = PevDashboardColors.purple
    static let iconFill = PevDashboardColors.disabledFill
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
    let thermalMetricValue = telemetry.thermalMetricValue
    let limpHomeRangeMetricValue = state.limpHomeRangeMetricValue
    let thermalDetail = if case .available = thermalMetricValue {
        liveThermalDetail(telemetry: telemetry)
    } else {
        localizedAppText("ride.value.unavailable")
    }
    let limpHomeRangeDetail = if case .available = limpHomeRangeMetricValue {
        localizedAppText("ride.detail.typed_range_estimate")
    } else {
        localizedAppText("ride.value.unavailable")
    }

    return [
        chargeEstimateTile(from: state),
        PevDashboardTile(
            kind: .packVoltage,
            label: localizedAppText("ride.metric.pack"),
            metricValue: telemetry.packVoltageMetricValue,
            unit: RideUnits.voltageUnit,
            detail: telemetry.voltage == nil
                ? localizedAppText("ride.value.unavailable")
                : telemetry.chargeEstimate?.voltageSag.map(voltageSagDetail)
                    ?? localizedAppText("ride.detail.sag_unavailable"),
            accent: .cyan
        ),
        livePowerTile(from: telemetry),
        PevDashboardTile(
            kind: .thermal,
            label: localizedAppText("ride.metric.thermal"),
            metricValue: thermalMetricValue,
            unit: RideUnits.temperatureUnit,
            detail: thermalDetail,
            accent: .green
        ),
        PevDashboardTile(
            kind: .limpHomeRange,
            label: localizedAppText("ride.metric.limp_home"),
            metricValue: limpHomeRangeMetricValue,
            unit: distanceUnit,
            detail: limpHomeRangeDetail,
            accent: .cyan
        ),
    ]
}

func vescDebugTiles(_ snapshot: VescRideSnapshot) -> [PevDashboardTile] {
    return [
        PevDashboardTile(
            kind: .dutyCycle,
            label: localizedAppText("vesc.debug.metric.duty"),
            metricValue: snapshot.dutyCycleMetricValue,
            unit: RideUnits.percentUnit,
            detail: localizedAppText("vesc.debug.detail.motor_duty"),
            accent: .orange
        ),
        PevDashboardTile(
            kind: .headroom,
            label: localizedAppText("vesc.debug.metric.headroom"),
            metricValue: snapshot.dutyHeadroomMetricValue,
            unit: RideUnits.percentUnit,
            detail: localizedAppText("vesc.debug.detail.remaining_duty"),
            accent: .yellow
        ),
        PevDashboardTile(
            kind: .boardAngle,
            label: localizedAppText("vesc.debug.metric.board"),
            metricValue: snapshot.boardAngleMetricValue,
            unit: RideUnits.angleUnit,
            detail: localizedAppText("vesc.debug.detail.balance", snapshot.balanceAngleMetricValue.accessibilityText),
            accent: .cyan
        ),
        PevDashboardTile(
            kind: .controller,
            label: localizedAppText("vesc.debug.metric.controller"),
            metricValue: snapshot.controllerTemperatureMetricValue,
            unit: RideUnits.temperatureUnit,
            detail: localizedAppText(
                "vesc.debug.detail.motor_temperature",
                snapshot.motorTemperatureMetricValue.accessibilityText
            ),
            accent: .green
        ),
    ]
}

func vescDebugRows(
    _ snapshot: VescRideSnapshot?,
    phase: SessionConnectionPhase,
    notificationCount: UInt64
) -> [PevDashboardKeyValueRow] {
    guard let snapshot else {
        return [PevDashboardKeyValueRow(
            id: "phase",
            label: localizedAppText("vesc.debug.row.session"),
            metricValue: .available(display: phase.displayText, accessibility: phase.displayText)
        )]
    }

    let phaseText = phase.displayText
    let protocolText = vescDebugProtocolText(snapshot.subProtocol)
    let stateText = vescOperatingStateText(snapshot)
    let notificationText = String(notificationCount)
    return [
        PevDashboardKeyValueRow(id: "phase", label: localizedAppText("vesc.debug.row.session"), metricValue: .available(display: phaseText, accessibility: phaseText)),
        PevDashboardKeyValueRow(id: "protocol", label: localizedAppText("vesc.debug.row.protocol"), metricValue: .available(display: protocolText, accessibility: protocolText)),
        PevDashboardKeyValueRow(id: "state", label: localizedAppText("vesc.debug.row.state"), metricValue: .available(display: stateText, accessibility: stateText)),
        PevDashboardKeyValueRow(id: "notifications", label: localizedAppText("vesc.debug.row.notifications"), metricValue: .available(display: notificationText, accessibility: notificationText)),
        PevDashboardKeyValueRow(
            id: "voltage",
            label: localizedAppText("vesc.debug.row.pack_voltage"),
            metricValue: vescDebugMetricValue(
                snapshot.batteryVoltageMetricValue,
                format: "vesc.debug.value.voltage"
            )
        ),
        PevDashboardKeyValueRow(
            id: "battery-current",
            label: localizedAppText("vesc.debug.row.battery_current"),
            metricValue: vescDebugMetricValue(
                snapshot.batteryCurrentMetricValue,
                format: "vesc.debug.value.current"
            )
        ),
        PevDashboardKeyValueRow(
            id: "motor-current",
            label: localizedAppText("vesc.debug.row.motor_current"),
            metricValue: vescDebugMetricValue(
                snapshot.motorCurrentMetricValue,
                format: "vesc.debug.value.current"
            )
        ),
        PevDashboardKeyValueRow(
            id: "footpad",
            label: localizedAppText("vesc.debug.row.footpad"),
            metricValue: snapshot.footpadMetricValue
        ),
    ]
}

private func vescDebugMetricValue(
    _ metricValue: PevDashboardMetricValue,
    format key: String
) -> PevDashboardMetricValue {
    switch metricValue {
    case .available(let display, let accessibility):
        .available(
            display: localizedAppText(key, display),
            accessibility: localizedAppText(key, accessibility)
        )
    case .status:
        metricValue
    case .unavailable:
        .unavailable
    }
}

func vescDebugProtocolText(_ subProtocol: VescSubProtocol) -> String {
    switch subProtocol {
    case .refloat:
        localizedAppText("vesc.debug.protocol.refloat")
    case .bike:
        localizedAppText("vesc.debug.protocol.bike")
    case .eskate:
        localizedAppText("vesc.debug.protocol.eskate")
    case .generic:
        localizedAppText("vesc.debug.protocol.generic")
    }
}

func vescOperatingStateText(_ snapshot: VescRideSnapshot) -> String {
    switch snapshot.operatingState {
    case .parked:
        localizedAppText("vesc.debug.state.parked")
    case .standing:
        localizedAppText("vesc.debug.state.standing")
    case .riding:
        localizedAppText("vesc.debug.state.riding")
    case .charging:
        localizedAppText("vesc.debug.state.charging")
    case .unknown:
        localizedAppText("vesc.debug.state.unknown")
    }
}

func vescVehicleKindText(_ vehicleKind: VescVehicleKind) -> String {
    switch vehicleKind {
    case .float:
        localizedAppText("vesc.vehicle.float")
    case .bike:
        localizedAppText("vesc.vehicle.bike")
    case .skateboard:
        localizedAppText("vesc.vehicle.skateboard")
    case .electricUnicycle:
        localizedAppText("vesc.vehicle.electric_unicycle")
    case .unknown:
        localizedAppText("vesc.vehicle.unknown")
    }
}

func vescRideSubtitle(_ snapshot: VescRideSnapshot) -> String {
    if case .unknown = snapshot.operatingState {
        return vescVehicleKindText(snapshot.vehicleKind)
    }
    return vescOperatingStateText(snapshot)
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
        metricValue: estimate.kind.dashboardMetricValue(display: estimate.displayValue),
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
    let presentation = telemetry.powerPresentation
    let detail: String
    switch presentation {
    case .calculatedPackCurrent:
        detail = powerFlowDetail(
            telemetry.powerFlow,
            fallback: localizedAppText("ride.power.calculated_pack_current")
        )
    case .reported:
        detail = powerFlowDetail(
            telemetry.powerFlow,
            fallback: localizedAppText("ride.power.live_telemetry")
        )
    case .unavailable:
        detail = localizedAppText("ride.value.unavailable")
    }
    return PevDashboardTile(
        kind: .power,
        label: localizedAppText("ride.metric.power"),
        metricValue: presentation.metricValue,
        unit: RideUnits.powerUnit,
        detail: detail,
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

func liveThermalValue(telemetry: TelemetrySnapshot) -> PevDashboardMetricValue {
    telemetry.thermalMetricValue
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
