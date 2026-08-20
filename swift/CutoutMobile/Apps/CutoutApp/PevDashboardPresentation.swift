import CutoutMobile

func liveSafetyBars(for state: EucRideScreenState) -> [PevSafetyBar] {
    [
        PevSafetyBar(
            id: .pwmHeadroom, label: localizedAppText("ride.safety.pwm_headroom"),
            metricValue: state.pwmHeadroomMetricValue, progress: state.pwmHeadroomProgress, accent: .yellow),
        PevSafetyBar(
            id: .sagAdjustedEnergy, label: localizedAppText("ride.safety.sag_adjusted_energy"),
            metricValue: .unavailable, progress: nil, accent: .cyan),
    ]
}

func liveDashboardTiles(from state: EucRideScreenState, telemetry: TelemetrySnapshot) -> [PevDashboardTile] {
    let distanceUnit = RideUnits.distanceUnit(forSpeedUnit: state.speedUnit)
    let thermalMetricValue = telemetry.thermalMetricValue
    let limpHomeRangeMetricValue = state.limpHomeRangeMetricValue
    let thermalDetail =
        if case .available = thermalMetricValue { liveThermalDetail(telemetry: telemetry) } else {
            localizedAppText("ride.value.unavailable")
        }
    let limpHomeRangeDetail =
        if case .available = limpHomeRangeMetricValue { localizedAppText("ride.detail.typed_range_estimate") } else {
            localizedAppText("ride.value.unavailable")
        }
    return [
        chargeEstimateTile(from: state),
        PevDashboardTile(
            kind: .packVoltage, label: localizedAppText("ride.metric.pack"),
            metricValue: telemetry.packVoltageMetricValue, unit: RideUnits.voltageUnit,
            detail: livePackVoltageDetail(telemetry.packVoltageDetail), accent: .cyan),
        livePowerTile(from: telemetry),
        PevDashboardTile(
            kind: .thermal, label: localizedAppText("ride.metric.thermal"), metricValue: thermalMetricValue,
            unit: RideUnits.temperatureUnit, detail: thermalDetail, accent: .green),
        PevDashboardTile(
            kind: .limpHomeRange, label: localizedAppText("ride.metric.limp_home"),
            metricValue: limpHomeRangeMetricValue, unit: distanceUnit, detail: limpHomeRangeDetail, accent: .cyan),
    ]
}

func vescDebugTiles(_ snapshot: VescRideSnapshot) -> [PevDashboardTile] {
    [
        PevDashboardTile(
            kind: .dutyCycle, label: localizedAppText("vesc.debug.metric.duty"),
            metricValue: snapshot.dutyCycleMetricValue, unit: RideUnits.percentUnit,
            detail: localizedAppText("vesc.debug.detail.motor_duty"), accent: .orange),
        PevDashboardTile(
            kind: .headroom, label: localizedAppText("vesc.debug.metric.headroom"),
            metricValue: snapshot.dutyHeadroomMetricValue, unit: RideUnits.percentUnit,
            detail: localizedAppText("vesc.debug.detail.remaining_duty"), accent: .yellow),
        PevDashboardTile(
            kind: .boardAngle, label: localizedAppText("vesc.debug.metric.board"),
            metricValue: snapshot.boardAngleMetricValue, unit: RideUnits.angleUnit,
            detail: localizedAppText("vesc.debug.detail.balance", snapshot.balanceAngleMetricValue.accessibilityText),
            accent: .cyan),
        PevDashboardTile(
            kind: .controller, label: localizedAppText("vesc.debug.metric.controller"),
            metricValue: snapshot.controllerTemperatureMetricValue, unit: RideUnits.temperatureUnit,
            detail: localizedAppText(
                "vesc.debug.detail.motor_temperature", snapshot.motorTemperatureMetricValue.accessibilityText),
            accent: .green),
    ]
}

func vescDebugRows(_ snapshot: VescRideSnapshot?, phase: SessionConnectionPhase, notificationCount: UInt64)
    -> [PevDashboardKeyValueRow]
{
    guard let snapshot else {
        return [
            PevDashboardKeyValueRow(
                id: "phase", label: localizedAppText("vesc.debug.row.session"),
                metricValue: .status(display: phase.displayText, accessibility: phase.displayText))
        ]
    }
    let phaseText = phase.displayText
    let protocolText = vescDebugProtocolText(snapshot.subProtocol)
    let stateText = vescOperatingStateText(snapshot)
    let notificationText = notificationCount.formatted()
    return [
        PevDashboardKeyValueRow(
            id: "phase", label: localizedAppText("vesc.debug.row.session"),
            metricValue: .status(display: phaseText, accessibility: phaseText)),
        PevDashboardKeyValueRow(
            id: "protocol", label: localizedAppText("vesc.debug.row.protocol"),
            metricValue: .status(display: protocolText, accessibility: protocolText)),
        PevDashboardKeyValueRow(
            id: "state", label: localizedAppText("vesc.debug.row.state"),
            metricValue: .status(display: stateText, accessibility: stateText)),
        PevDashboardKeyValueRow(
            id: "notifications", label: localizedAppText("vesc.debug.row.notifications"),
            metricValue: .status(display: notificationText, accessibility: notificationText)),
        PevDashboardKeyValueRow(
            id: "voltage", label: localizedAppText("vesc.debug.row.pack_voltage"),
            metricValue: vescDebugMetricValue(snapshot.batteryVoltageMetricValue, format: "vesc.debug.value.voltage")),
        PevDashboardKeyValueRow(
            id: "battery-current", label: localizedAppText("vesc.debug.row.battery_current"),
            metricValue: vescDebugMetricValue(snapshot.batteryCurrentMetricValue, format: "vesc.debug.value.current")),
        PevDashboardKeyValueRow(
            id: "motor-current", label: localizedAppText("vesc.debug.row.motor_current"),
            metricValue: vescDebugMetricValue(snapshot.motorCurrentMetricValue, format: "vesc.debug.value.current")),
        PevDashboardKeyValueRow(
            id: "footpad", label: localizedAppText("vesc.debug.row.footpad"), metricValue: snapshot.footpadMetricValue),
    ]
}

private func vescDebugMetricValue(_ metricValue: PevDashboardMetricValue, format key: String) -> PevDashboardMetricValue
{
    switch metricValue {
    case .available(let display, let accessibility):
        .available(display: localizedAppText(key, display), accessibility: localizedAppText(key, accessibility))
    case .status:
        metricValue
    case .unavailable:
        .unavailable
    }
}

func vescDebugProtocolText(_ subProtocol: VescSubProtocol) -> String {
    switch subProtocol {
    case .refloat: localizedAppText("vesc.debug.protocol.refloat")
    case .bike: localizedAppText("vesc.debug.protocol.bike")
    case .eskate: localizedAppText("vesc.debug.protocol.eskate")
    case .generic: localizedAppText("vesc.debug.protocol.generic")
    }
}

func vescOperatingStateText(_ snapshot: VescRideSnapshot) -> String {
    switch snapshot.operatingState {
    case .parked: localizedAppText("vesc.debug.state.parked")
    case .standing: localizedAppText("vesc.debug.state.standing")
    case .riding: localizedAppText("vesc.debug.state.riding")
    case .charging: localizedAppText("vesc.debug.state.charging")
    case .unknown: localizedAppText("vesc.debug.state.unknown")
    }
}

func vescVehicleKindText(_ vehicleKind: VescVehicleKind) -> String {
    switch vehicleKind {
    case .float: localizedAppText("vesc.vehicle.float")
    case .bike: localizedAppText("vesc.vehicle.bike")
    case .skateboard: localizedAppText("vesc.vehicle.skateboard")
    case .electricUnicycle: localizedAppText("vesc.vehicle.electric_unicycle")
    case .unknown: localizedAppText("vesc.vehicle.unknown")
    }
}

func vescRideSubtitle(_ snapshot: VescRideSnapshot) -> String {
    switch snapshot.operatingMode {
    case .darkride: return localizedAppText("vesc.mode.darkride")
    case .handtest: return localizedAppText("vesc.mode.handtest")
    case .flywheel: return localizedAppText("vesc.mode.flywheel")
    case .normal, .unknown: break
    }
    if case .unknown = snapshot.operatingState { return vescVehicleKindText(snapshot.vehicleKind) }
    return vescOperatingStateText(snapshot)
}

func livePackVoltageDetail(_ detail: TelemetryPackVoltageDetail) -> String {
    switch detail {
    case .unavailable: localizedAppText("ride.value.unavailable")
    case .voltageSag(let sag): voltageSagDetail(sag.detailReadback)
    case .sagUnavailable: localizedAppText("ride.detail.sag_unavailable")
    }
}

func chargeEstimateTile(from state: EucRideScreenState) -> PevDashboardTile {
    let presentation = state.chargeEstimate.dashboardPresentation
    return PevDashboardTile(
        kind: .chargeEstimate, label: localizedAppText("ride.metric.charge"), metricValue: presentation.metricValue,
        unit: "", detail: chargeEstimateDetail(presentation.detail), accent: .green)
}

func chargeEstimateDetail(_ detail: ChargeEstimateDashboardDetail) -> String {
    switch detail {
    case .voltageSag(let voltageSag, let estimateDetail):
        localizedAppText(
            "ride.charge.detail.with_voltage_sag", voltageSagDetail(voltageSag.detailReadback), estimateDetail)
    case .standard(let estimateDetail): estimateDetail
    }
}

func voltageSagDetail(_ readback: ChargeVoltageSagReadback) -> String {
    localizedAppText(
        "ride.sag.detail", readback.voltage, readback.current, Int64(readback.effectiveResistanceMilliohms))
}

func livePowerTile(from telemetry: TelemetrySnapshot) -> PevDashboardTile {
    let presentation = telemetry.powerPresentation
    let detail: String
    switch presentation {
    case .calculatedPackCurrent:
        detail = powerFlowDetail(telemetry.powerFlow, fallback: localizedAppText("ride.power.calculated_pack_current"))
    case .reported:
        detail = powerFlowDetail(telemetry.powerFlow, fallback: localizedAppText("ride.power.live_telemetry"))
    case .unavailable: detail = localizedAppText("ride.value.unavailable")
    }
    return PevDashboardTile(
        kind: .power, label: localizedAppText("ride.metric.power"), metricValue: presentation.metricValue,
        unit: RideUnits.powerUnit, detail: detail, accent: .yellow)
}

func powerFlowDetail(_ direction: PowerFlowDirection?, fallback: String) -> String {
    switch direction {
    case .discharge: localizedAppText("telemetry.power_flow.discharge")
    case .zero: localizedAppText("telemetry.power_flow.zero")
    case .charging: localizedAppText("telemetry.power_flow.charging")
    case .regeneration: localizedAppText("telemetry.power_flow.regeneration")
    case .negativeUnknown: localizedAppText("telemetry.power_flow.negative_unknown")
    case nil: fallback
    }
}

func liveThermalDetail(telemetry: TelemetrySnapshot) -> String {
    let unit = RideUnits.temperatureUnit
    switch telemetry.thermalReadback {
    case .all(let controller, let motor, let battery):
        return localizedAppText("ride.thermal.all", controller, unit, motor, unit, battery, unit)
    case .controllerMotor(let controller, let motor):
        return localizedAppText("ride.thermal.controller_motor", controller, unit, motor, unit)
    case .controllerBattery(let controller, let battery):
        return localizedAppText("ride.thermal.controller_battery", controller, unit, battery, unit)
    case .motorBattery(let motor, let battery):
        return localizedAppText("ride.thermal.motor_battery", motor, unit, battery, unit)
    case .controller(let controller): return localizedAppText("ride.thermal.controller", controller, unit)
    case .motor(let motor): return localizedAppText("ride.thermal.motor", motor, unit)
    case .battery(let battery): return localizedAppText("ride.thermal.battery", battery, unit)
    case .unavailable: return localizedAppText("ride.detail.typed_telemetry")
    }
}

func percentageString<T: BinaryInteger>(fromPercent percent: T) -> String {
    localizedAppText("ride.value.percent", RideUnits.percentText(percent))
}

func percentageString<T: BinaryInteger>(fromPermille permille: T) -> String {
    localizedAppText("ride.value.percent", RideUnits.permillePercentText(permille))
}
