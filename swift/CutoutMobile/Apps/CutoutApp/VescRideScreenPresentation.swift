import CutoutMobile
import CutoutMobileFFI

struct VescRideScreenPresentation {
    let snapshot: VescRideSnapshot?
    let phase: SessionConnectionPhase
    let now: MonotonicMilliseconds
    let connectionStatusText: String?

    /// Standard Ride owns its quiet status header. Discovery and setup retain the app shell.
    static func usesOwnHeader(
        application: VescApplication?,
        classificationRequirement: MobileVehicleClassificationRequirementDto?
    ) -> Bool {
        switch classificationRequirement {
        case .pending?, .userSelectionRequired?: false
        case .resolved?, nil: application == .absent
        }
    }

    var title: String {
        snapshot?.title ?? VescRideSnapshot.defaultTitle
    }

    var subtitle: String {
        if phase != .live {
            return connectionStatusText ?? phase.displayText
        }
        if let warningCard { return warningCard.title }
        guard let snapshot, snapshot.operatingState != .unknown else {
            return connectionStatusText ?? pevLocalizedText("euc.status.connected")
        }
        return vescRideSubtitle(snapshot)
    }

    var statusTone: PevDashboardStatusPillTone {
        guard phase == .live else { return .warning }
        return warningCard == nil ? .vescRide : .warning
    }

    var speedReadout: RideHeroReadout {
        .vesc(snapshot: snapshot, now: now)
    }

    var warningCard: PevWarningCard? {
        guard let snapshot else { return nil }
        switch snapshot.warning {
        case .lowVoltage:
            return warningCard("vesc.warning.low_voltage")
        case .highVoltage:
            return warningCard("vesc.warning.high_voltage")
        case .mosfetTemperature:
            return warningCard("vesc.warning.mosfet_temperature")
        case .motorTemperature:
            return warningCard("vesc.warning.motor_temperature")
        case .current:
            return warningCard("vesc.warning.current")
        case .dutyPushback:
            return warningCard("vesc.warning.duty_pushback", showsFootpad: true)
        case .speedPushback:
            return warningCard("vesc.warning.duty_pushback")
        case .temperaturePushback:
            return warningCard("vesc.warning.temperature_pushback")
        case .wheelslip:
            return warningCard("vesc.warning.wheelslip")
        case .sensors:
            return warningCard("vesc.warning.sensors", showsFootpad: true)
        case .lowBattery:
            return warningCard("vesc.warning.low_battery")
        case .error:
            return warningCard("vesc.warning.error")
        case .bmsConnection:
            return warningCard("vesc.warning.error")
        case .none:
            return stopWarningCard
        case .unknown:
            return nil
        }
    }

    var dashboardSupport: VescRideDashboardSupport {
        VescRideSnapshot.dashboardSupport(
            for: snapshot,
            phase: phase,
            at: now,
            staleAfter: RideTelemetryFreshnessPolicy.staleAfter
        )
    }

    var dashboardTiles: [PevDashboardTile] {
        guard let snapshot else { return [] }
        if usesStandardInstruments { return standardDashboardTiles }
        return [
            PevDashboardTile(
                kind: .batteryVoltage,
                label: localizedAppText("vesc.metric.battery_voltage"),
                metricValue: snapshot.batteryVoltageMetricValue,
                unit: RideUnits.voltageUnit,
                detail: batteryDetail,
                accent: .yellow
            ),
            PevDashboardTile(
                kind: .motorCurrent,
                label: localizedAppText("vesc.metric.motor_current"),
                metricValue: snapshot.motorCurrentMetricValue,
                unit: RideUnits.currentUnit,
                detail: motorCurrentDetail,
                accent: .orange
            ),
            PevDashboardTile(
                kind: .boardAngle,
                label: localizedAppText("vesc.metric.board_angle"),
                metricValue: snapshot.boardAngleMetricValue,
                unit: RideUnits.angleUnit,
                detail: boardAngleDetail ?? localizedAppText("vesc.board_angle.unavailable"),
                accent: .cyan
            ),
            PevDashboardTile(
                kind: .controller,
                label: localizedAppText("vesc.metric.controller"),
                metricValue: snapshot.controllerTemperatureMetricValue,
                unit: RideUnits.temperatureUnit,
                detail: controllerTemperatureDetail,
                accent: .green
            ),
        ]
    }

    /// Application discovery is independent of physical class. Standard telemetry never
    /// acquires Refloat balance instruments from a manually selected skateboard class.
    var usesStandardInstruments: Bool { snapshot?.application == .absent }

    var standardEnergyTile: PevDashboardTile {
        standardDashboardTiles.first { $0.kind == .batteryLevel }
            ?? standardDashboardTiles[0]
    }

    var standardSummaryTiles: [PevDashboardTile] {
        let distanceUnit = RideUnits.distanceUnit(forSpeedUnit: RideUnits.speedUnit)
        let distance = snapshot?.tripDistance.map {
            RideUnits.distanceText(millimetres: $0.value, unit: distanceUnit)
        }
        let duration = snapshot?.rideDuration.map {
            RideUnits.durationText(milliseconds: $0.rawValue)
        }
        return [
            PevDashboardTile(
                kind: .tripDistance, label: localizedAppText("vesc.metric.trip"),
                metricValue: distance.map { .available(display: $0, accessibility: $0) } ?? .unavailable,
                unit: distanceUnit, detail: "", accent: .cyan
            ),
            PevDashboardTile(
                kind: .rideDuration, label: localizedAppText("vesc.metric.ride_time"),
                metricValue: duration.map { .available(display: $0, accessibility: $0) } ?? .unavailable,
                unit: "", detail: "", accent: .cyan
            ),
        ]
    }

    var standardSecondaryTiles: [PevDashboardTile] {
        standardDashboardTiles.filter {
            $0.kind == .batteryCurrent || $0.kind == .controller || $0.kind == .motorTemperature
        }
    }

    private var standardDashboardTiles: [PevDashboardTile] {
        var tiles =
            [
                PevDashboardTile(
                    kind: .batteryVoltage, label: localizedAppText("vesc.metric.pack_voltage"),
                    metricValue: snapshot?.batteryVoltageMetricValue ?? .unavailable,
                    unit: RideUnits.voltageUnit, detail: "", accent: .yellow
                ),
                PevDashboardTile(
                    kind: .batteryCurrent, label: localizedAppText("vesc.metric.battery_current"),
                    // BatteryCurrent's presentation already contains its unit and direction.
                    metricValue: snapshot?.batteryCurrentMetricValue ?? .unavailable,
                    unit: "", detail: "", accent: .cyan
                ),
                PevDashboardTile(
                    kind: .motorCurrent, label: localizedAppText("vesc.metric.motor_current"),
                    metricValue: snapshot?.motorCurrentMetricValue ?? .unavailable,
                    unit: RideUnits.currentUnit, detail: motorCurrentDetail, accent: .orange
                ),
                PevDashboardTile(
                    kind: .controller, label: localizedAppText("vesc.metric.controller_temperature"),
                    metricValue: snapshot?.controllerTemperatureMetricValue ?? .unavailable,
                    unit: RideUnits.temperatureUnit, detail: "", accent: .green
                ),
                PevDashboardTile(
                    kind: .motorTemperature, label: localizedAppText("vesc.metric.motor_temperature"),
                    metricValue: snapshot?.motorTemperatureMetricValue ?? .unavailable,
                    unit: RideUnits.temperatureUnit, detail: "", accent: .green
                ),
            ] + standardSummaryTiles
        if let level = snapshot?.batteryLevelReported ?? snapshot?.batteryLevelEstimated {
            let text = RideUnits.percentText(level.value)
            tiles.insert(
                PevDashboardTile(
                    kind: .batteryLevel, label: localizedAppText("ride.metric.battery"),
                    metricValue: .available(display: text, accessibility: text), unit: RideUnits.percentUnit,
                    detail: snapshot?.batteryLevelReported == nil ? localizedAppText("ride.detail.estimated") : "",
                    accent: .green
                ), at: 0
            )
        }
        return tiles
    }

    private var stopWarningCard: PevWarningCard? {
        guard let stopReason = snapshot?.stopReason else { return nil }
        let titleKey: String
        switch stopReason {
        case .none: return nil
        case .pitch: titleKey = "vesc.stop.pitch"
        case .roll: titleKey = "vesc.stop.roll"
        case .switchHalf: titleKey = "vesc.stop.switch_half"
        case .switchFull: titleKey = "vesc.stop.switch_full"
        case .reverse: titleKey = "vesc.stop.reverse"
        case .quickStop: titleKey = "vesc.stop.quick_stop"
        }
        return PevWarningCard(
            title: localizedAppText(titleKey),
            detail: localizedAppText("vesc.stop.detail")
        )
    }

    private func warningCard(_ titleKey: String, showsFootpad: Bool = false) -> PevWarningCard {
        PevWarningCard(
            title: localizedAppText(titleKey),
            detail: showsFootpad
                ? footpadText ?? localizedAppText("vesc.warning.live_telemetry")
                : localizedAppText("vesc.warning.live_telemetry")
        )
    }

    private var footpadText: String? {
        snapshot?.footpad?.summaryText
    }

    private var batteryDetail: String {
        guard let snapshot else { return "" }
        switch snapshot.batteryReadback {
        case .reported(let level, let current):
            let level = localizedAppText("ride.value.percent", level)
            if let current {
                return localizedAppText("vesc.battery_detail.reported_current", level, current)
            }
            return localizedAppText("vesc.battery_detail.reported_unavailable", level)
        case .estimated(let level, let current):
            let level = localizedAppText("ride.value.percent", level)
            if let current {
                return localizedAppText("vesc.battery_detail.estimated_current", level, current)
            }
            return localizedAppText("vesc.battery_detail.estimated_unavailable", level)
        case .unavailable(let current):
            if let current {
                return localizedAppText("vesc.battery_detail.unavailable_current", current)
            }
            return localizedAppText("vesc.battery_detail.unavailable_unavailable")
        }
    }

    private var motorCurrentDetail: String {
        guard let snapshot else {
            return localizedAppText("vesc.current.unavailable")
        }
        switch snapshot.motorCurrentDetail {
        case .available(let powerFlow):
            return powerFlowDetail(powerFlow, fallback: localizedAppText("vesc.phase_current"))
        case .unavailable:
            return localizedAppText("vesc.current.unavailable")
        }
    }

    private var boardAngleDetail: String? {
        guard let snapshot else { return nil }
        switch snapshot.boardAngleReadback {
        case .available(let orientation, let balanceAngle):
            let direction =
                switch orientation {
                case .noseDown: "nose_down"
                case .level: "level"
                case .noseUp: "nose_up"
                }
            if let balanceAngle {
                return localizedAppText("vesc.board_angle.\(direction)_with_balance", balanceAngle)
            }
            return localizedAppText("vesc.board_angle.\(direction)")
        case .unavailable:
            return nil
        }
    }

    private var controllerTemperatureDetail: String {
        guard let snapshot else {
            return localizedAppText("vesc.motor_temperature.unavailable")
        }
        switch snapshot.controllerTemperatureReadback {
        case .available(let motorTemperature):
            return localizedAppText(
                "vesc.motor_temperature.available",
                motorTemperature,
                RideUnits.temperatureUnit
            )
        case .unavailable:
            return localizedAppText("vesc.motor_temperature.unavailable")
        }
    }
}
