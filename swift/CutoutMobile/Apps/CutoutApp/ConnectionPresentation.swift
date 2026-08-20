import CutoutMobile

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
        case .bluetoothPermissionDenied:
            self = .init(
                title: localizedAppText("picker.status.bluetooth_permission_denied"), showsActivity: false,
                symbolName: "lock.slash.fill")
        case .bluetoothUnavailable:
            self = .init(
                title: localizedAppText("picker.status.bluetooth_unavailable"), showsActivity: false,
                symbolName: "bolt.slash.fill")
        case .failed(let failure):
            self = .init(title: failure.displayText, showsActivity: false, symbolName: "xmark.octagon.fill")
        case .connecting, .discoveringServices, .subscribing:
            self = .init(title: localizedAppText("picker.status.connecting"), showsActivity: true)
        case .starting:
            self = .init(
                title: localizedAppText("picker.status.starting_bluetooth"), showsActivity: false,
                symbolName: "bolt.horizontal.circle")
        case .scanning:
            self = .init(
                title: pickerScanStatusTitle(scanState),
                showsActivity: scanState?.status == .scanning || scanState == nil)
        case .live:
            self = .init(
                title: localizedAppText("picker.status.live"), showsActivity: false, symbolName: "checkmark.circle.fill"
            )
        case nil:
            self = .init(
                title: pickerScanStatusTitle(scanState), showsActivity: false, symbolName: "bolt.horizontal.circle")
        }
    }
}

private func pickerScanStatusTitle(_ scanState: DevicePickerScanState?) -> String {
    guard let scanState else { return localizedAppText("picker.status.starting_bluetooth") }
    return switch scanState.status {
    case .scanning: localizedAppText("picker.status.scanning_bluetooth")
    case .idle where scanState.rows.isEmpty: localizedAppText("picker.status.no_devices")
    case .idle: localizedAppText("picker.status.scan_complete")
    case .bluetoothUnavailable: localizedAppText("picker.status.bluetooth_unavailable")
    case .permissionDenied: localizedAppText("picker.status.bluetooth_permission_denied")
    case .failed(let message): message
    }
}

extension PevScreen {
    var displaySubtitle: String { subtitle.replacingOccurrences(of: " - ", with: " · ") }

    var tabTitle: String {
        switch id {
        case .eucRide: "EUC"
        case .bmsOverview, .bmsNoData: "BMS"
        case .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail: localizedAppText("navigation.tab.cells")
        case .bmsUnknownTopology: localizedAppText("navigation.tab.faults")
        case .vescRide, .vescDebug: "VESC"
        }
    }
}

extension SessionConnectionPhase {
    var opensRideScreen: Bool {
        switch self {
        case .live: true
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .connecting, .discoveringServices,
            .subscribing, .failed:
            false
        }
    }

    var accessibilityAnnouncement: String? {
        switch self {
        case .starting, .scanning, .discoveringServices, .subscribing: nil
        case .bluetoothPermissionDenied: localizedAppText("picker.announcement.bluetooth_permission_denied")
        case .bluetoothUnavailable: localizedAppText("picker.announcement.bluetooth_unavailable")
        case .connecting(let model): localizedAppText("picker.announcement.connecting", model.displayName)
        case .live: localizedAppText("picker.announcement.connected")
        case .failed(let failure): localizedAppText("picker.announcement.connection_failed", failure.displayText)
        }
    }
}

extension LiveActivityRideLifecycleError {
    var accessibilityAnnouncement: String {
        switch self {
        case .authorizationDenied: localizedAppText("live_activity.error.authorization_denied")
        case .requestFailed: localizedAppText("live_activity.error.request_failed")
        case .activityUnavailable: localizedAppText("live_activity.error.unavailable")
        }
    }
}

struct ConnectionAccessibilityAnnouncements {
    private var hasAnnouncedFailure = false
    private var hasAnnouncedRetry = false

    mutating func beginUserInitiatedAttempt() {
        hasAnnouncedFailure = false
        hasAnnouncedRetry = false
    }

    mutating func next(for phase: SessionConnectionPhase) -> String? {
        switch phase {
        case .starting, .live: hasAnnouncedFailure = false
        case .failed:
            guard !hasAnnouncedFailure else { return nil }
            hasAnnouncedFailure = true
        default: break
        }
        return phase.accessibilityAnnouncement
    }

    mutating func next(for state: ConnectionState) -> String? {
        switch state {
        case .retrying:
            guard !hasAnnouncedRetry else { return nil }
            hasAnnouncedRetry = true
            return localizedAppText("picker.announcement.retrying")
        case .connected:
            hasAnnouncedRetry = false
            return nil
        case .picker, .identified, .connecting, .failed: return nil
        }
    }

    mutating func next(for scanState: DevicePickerScanState) -> String? {
        guard case .failed = scanState.status, !hasAnnouncedFailure else { return nil }
        hasAnnouncedFailure = true
        return scanState.statusText
    }
}
