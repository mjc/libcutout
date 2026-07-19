import CutoutMobile

func percentText(_ value: BatteryLevel?) -> String {
    guard let value else { return "--" }
    return "\(value.value)%"
}

func voltageText(_ value: Voltage?) -> String {
    value.map { RideUnits.voltageText(millivolts: $0.value) } ?? "--"
}

func currentText(_ value: BatteryCurrent?) -> String {
    value.map { RideUnits.currentText(milliamps: $0.value) } ?? "--"
}

func phaseCurrentText(_ value: PhaseCurrent?) -> String {
    value.map { RideUnits.currentText(milliamps: $0.value) } ?? "--"
}

func angleText(_ value: CutoutMobile.Angle?) -> String {
    value.map { RideUnits.angleText(millidegrees: $0.value) } ?? "--"
}

func millivoltsText(_ value: VoltageDelta?) -> String {
    value.map { String($0.value) } ?? "--"
}

func temperatureText(_ value: Temperature?) -> String {
    value.map { RideUnits.temperatureText(millicelsius: $0.value, fractionDigits: 1) } ?? "--"
}

extension PevDashboardTile {
    func replacing(
        label: String? = nil,
        value: String,
        unit: String? = nil,
        detail: String
    ) -> PevDashboardTile {
        PevDashboardTile(
            kind: kind,
            label: label ?? self.label,
            value: value,
            unit: unit ?? self.unit,
            detail: detail,
            accent: accent
        )
    }
}

extension PevScreen {
    var displaySubtitle: String {
        subtitle.replacingOccurrences(of: " - ", with: " · ")
    }

    var tabTitle: String {
        switch id {
        case .eucRide:
            "EUC"
        case .bmsOverview, .bmsNoData:
            "BMS"
        case .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail:
            "Cells"
        case .bmsUnknownTopology:
            "Faults"
        case .eucGarage:
            "Pack"
        case .vescRide:
            "VESC"
        case .vescDebug:
            "VESC"
        }
    }
}

extension SessionConnectionPhase {
    var opensRideScreen: Bool {
        switch self {
        case .live:
            true
        case .starting, .bluetoothUnavailable, .scanning, .connecting, .discoveringServices, .subscribing, .failed:
            false
        }
    }
}
