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

func batteryCurrentText(_ value: BatteryCurrent?) -> String {
    value.map { RideUnits.currentText(milliamps: abs($0.value)) } ?? "--"
}

func batteryCurrentDetail(_ value: BatteryCurrent?) -> String {
    guard let value else { return "" }
    return "current \(batteryCurrentText(value)) A"
}

func energyFlowText(_ value: PowerFlowDirection?) -> String? {
    switch value {
    case .regeneration, .charging:
        return "regen"
    case .discharge:
        return "discharge"
    case .negativeUnknown:
        return "negative flow"
    case .zero, .none:
        return nil
    }
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

extension MockupDashboardTile {
    func replacing(
        label: String? = nil,
        value: String,
        unit: String? = nil,
        detail: String
    ) -> MockupDashboardTile {
        MockupDashboardTile(
            kind: kind,
            label: label ?? self.label,
            value: value,
            unit: unit ?? self.unit,
            detail: detail,
            accent: accent
        )
    }
}

extension MockupScreen {
    var displaySubtitle: String {
        subtitle.replacingOccurrences(of: " - ", with: " · ")
    }

    var tabTitle: String {
        switch id {
        case .devicePicker:
            "Picker"
        case .eucRide:
            "EUC"
        case .liveActivity:
            "Live Activity"
        case .bmsOverview, .bmsNoData:
            "BMS"
        case .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail:
            "Cells"
        case .bmsUnknownTopology:
            "Faults"
        case .eucGarage:
            "Pack"
        case .vescOnewheelRide:
            "OW"
        case .vescDebug:
            "VESC"
        }
    }
}

extension SessionConnectionPhase {
    var opensRideScreen: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing, .live:
            true
        case .starting, .bluetoothUnavailable, .scanning, .failed:
            false
        }
    }
}
