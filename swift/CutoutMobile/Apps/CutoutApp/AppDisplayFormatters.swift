import Foundation
import CutoutMobile

#if SWIFT_PACKAGE
let appLocalizationBundle = Bundle.module
#else
let appLocalizationBundle = Bundle.main
#endif

func localizedAppText(_ key: String, _ arguments: CVarArg...) -> String {
    String(
        format: appLocalizationBundle.localizedString(forKey: key, value: nil, table: "Localizable"),
        locale: .current,
        arguments: arguments
    )
}

struct DevicePickerConnectionPresentation: Equatable {
    let title: String
    let showsActivity: Bool
    let symbolName: String?

    init(title: String, showsActivity: Bool, symbolName: String? = nil) {
        self.title = title
        self.showsActivity = showsActivity
        self.symbolName = symbolName
    }

    init(scanState: DevicePickerScanState?, phase: SessionConnectionPhase?) {
        switch phase {
        case .bluetoothUnavailable:
            self = .init(
                title: String(localized: "picker.status.bluetooth_unavailable", table: "Localizable", bundle: appLocalizationBundle),
                showsActivity: false,
                symbolName: "bolt.slash.fill"
            )
        case .failed(let failure):
            self = .init(title: failure.displayText, showsActivity: false, symbolName: "xmark.octagon.fill")
        case .connecting, .discoveringServices, .subscribing:
            self = .init(title: String(localized: "picker.status.connecting", table: "Localizable", bundle: appLocalizationBundle), showsActivity: true)
        case .starting:
            self = .init(
                title: String(localized: "picker.status.starting_bluetooth", table: "Localizable", bundle: appLocalizationBundle),
                showsActivity: false,
                symbolName: "bolt.horizontal.circle"
            )
        case .scanning:
            self = .init(
                title: pickerScanStatusTitle(scanState),
                showsActivity: scanState?.status == .scanning || scanState == nil
            )
        case .live:
            self = .init(
                title: String(localized: "picker.status.live", table: "Localizable", bundle: appLocalizationBundle),
                showsActivity: false,
                symbolName: "checkmark.circle.fill"
            )
        case nil:
            self = .init(
                title: pickerScanStatusTitle(scanState),
                showsActivity: false,
                symbolName: "bolt.horizontal.circle"
            )
        }
    }
}

private func pickerScanStatusTitle(_ scanState: DevicePickerScanState?) -> String {
    guard let scanState else {
        return String(localized: "picker.status.starting_bluetooth", table: "Localizable", bundle: appLocalizationBundle)
    }

    return switch scanState.status {
    case .scanning:
        String(localized: "picker.status.scanning_bluetooth", table: "Localizable", bundle: appLocalizationBundle)
    case .idle where scanState.rows.isEmpty:
        String(localized: "picker.status.no_devices", table: "Localizable", bundle: appLocalizationBundle)
    case .idle:
        String(localized: "picker.status.scan_complete", table: "Localizable", bundle: appLocalizationBundle)
    case .bluetoothUnavailable:
        String(localized: "picker.status.bluetooth_unavailable", table: "Localizable", bundle: appLocalizationBundle)
    case .permissionDenied:
        String(localized: "picker.status.bluetooth_permission_denied", table: "Localizable", bundle: appLocalizationBundle)
    case .failed(let message):
        message
    }
}

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
            localizedAppText("navigation.tab.cells")
        case .bmsUnknownTopology:
            localizedAppText("navigation.tab.faults")
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
            String(
                localized: "picker.announcement.bluetooth_unavailable",
                table: "Localizable",
                bundle: appLocalizationBundle
            )
        case .connecting(let model):
            String(
                format: String(
                    localized: "picker.announcement.connecting",
                    table: "Localizable",
                    bundle: appLocalizationBundle
                ),
                locale: .current,
                model.displayName
            )
        case .live:
            String(localized: "picker.announcement.connected", table: "Localizable", bundle: appLocalizationBundle)
        case .failed(let failure):
            String(
                format: String(
                    localized: "picker.announcement.connection_failed",
                    table: "Localizable",
                    bundle: appLocalizationBundle
                ),
                locale: .current,
                failure.displayText
            )
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
