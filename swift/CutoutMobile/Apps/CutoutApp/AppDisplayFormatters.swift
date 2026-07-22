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

    var accessibilityAnnouncement: String? {
        switch self {
        case .starting, .scanning, .discoveringServices, .subscribing:
            nil
        case .bluetoothUnavailable:
            "Bluetooth unavailable. Turn on Bluetooth to reconnect."
        case .connecting(let model):
            "Connecting to \(model.displayName)."
        case .live:
            "Connected."
        case .failed(let failure):
            "Connection failed. Choose a device to try again. \(failure.displayText)"
        }
    }
}

struct ConnectionAccessibilityAnnouncements {
    private var hasAnnouncedFailure = false

    mutating func beginUserInitiatedAttempt() {
        hasAnnouncedFailure = false
    }

    mutating func next(for phase: SessionConnectionPhase) -> String? {
        switch phase {
        case .starting, .live:
            hasAnnouncedFailure = false
        case .failed:
            guard !hasAnnouncedFailure else { return nil }
            hasAnnouncedFailure = true
        default:
            break
        }
        return phase.accessibilityAnnouncement
    }
}

extension EucRideWarningSeverity {
    var accessibilityAnnouncement: String? {
        switch self {
        case .caution:
            "Caution. Riding headroom is getting low."
        case .reduceAcceleration:
            "Warning. Reduce acceleration."
        case .limpHome:
            "Critical warning. Slow down and stop safely."
        case .normal, .unavailable, .failed:
            nil
        }
    }
}

extension VescRideWarning {
    var accessibilityAnnouncement: String? {
        switch self {
        case .pushbackSoon:
            "Warning. Pushback soon."
        case .none, .unknown:
            nil
        }
    }
}

extension BmsAlertLevel {
    var accessibilityAnnouncement: String? {
        switch self {
        case .warning:
            "Battery warning. Check BMS details."
        case .critical:
            "Critical battery warning. Check BMS details."
        case .nominal, .unknown:
            nil
        }
    }
}

extension BmsSnapshot {
    var accessibilityAlertLevel: BmsAlertLevel {
        if groups.contains(where: { $0.alertLevel == .critical }) {
            return .critical
        }
        if groups.contains(where: { $0.alertLevel == .warning }) {
            return .warning
        }
        if groups.contains(where: { $0.alertLevel == .nominal }) {
            return .nominal
        }
        return .unknown
    }
}
