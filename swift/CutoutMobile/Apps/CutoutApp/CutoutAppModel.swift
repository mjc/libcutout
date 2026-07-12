import CutoutMobile
import Foundation

@MainActor
final class CutoutAppModel: ObservableObject {
    @Published private(set) var displayState = RideDisplayState()
    @Published private(set) var phase = SessionConnectionPhase.starting
    @Published private(set) var devicePickerScanState: DevicePickerScanState?
    @Published private(set) var selectedRideTitle: String?
    @Published private(set) var selectedConnectionRoute: DevicePickerConnectionRoute?
    @Published private(set) var settingsReadback: SettingsReadback?
    @Published private(set) var faultHistoryReadback: FaultHistoryReadback?
    @Published private(set) var bmsSnapshot: BmsSnapshot?
    @Published private(set) var captureStatusText: String?
    @Published private(set) var isRecordOnlyCapture = false
    @Published private(set) var activeCaptureLabels = Set<CaptureQuickLabel>()
    @Published private(set) var recordOnlyDeviceKind: String?

    var speed: SpeedReadout {
        displayState.speed
    }

    var currentMonotonicTime: MonotonicMilliseconds {
        core.now()
    }

    var rideState: EucRideScreenState {
        EucRideScreenState(phase: phase, displayState: displayState)
    }

    var vescRideSnapshot: VescRideSnapshot? {
        VescRideSnapshot(displayState: displayState, title: selectedRideTitle)
    }

    private let core = CutoutSessionCore()
    private let liveActivityCoordinator = LiveActivityRideLifecycleCoordinator(manager: LiveActivityRideActivityKitManager())
    private let selectedDeviceStore = DevicePickerSelectionStore()
    private var liveActivityIdentity: LiveActivityRideIdentity?
    private var liveActivityGlyph = LiveActivityRideGlyph.electricUnicycle
    private var lastLiveActivitySnapshot: LiveActivityRideSnapshot?
    private var lastLiveActivityUpdate: MonotonicMilliseconds?
    private var captureFileName: String?
    private var captureNotificationCount = 0
    private var captureLabel: String?
    private static let liveActivityUpdateIntervalMilliseconds: UInt64 = 1_000

    init() {
        core.onDisplayStateChange = { [weak self] displayState in
            self?.displayState = displayState
            self?.syncLiveActivity()
        }
        core.onPhaseChange = { [weak self] phase in
            self?.handlePhaseChange(phase)
            self?.syncLiveActivity()
        }
        core.onScanStateChange = { [weak self] scanState in
            self?.handleScanStateChange(scanState)
        }
        core.onSettingsReadbackChange = { [weak self] settingsReadback in
            self?.settingsReadback = settingsReadback
        }
        core.onFaultHistoryReadbackChange = { [weak self] faultHistoryReadback in
            self?.faultHistoryReadback = faultHistoryReadback
        }
        core.onBmsSnapshotChange = { [weak self] bmsSnapshot in
            self?.bmsSnapshot = bmsSnapshot
        }
        core.onProtocolIdentityCandidateChange = { [weak self] candidate in
            self?.applyProtocolIdentityCandidate(candidate)
        }
        core.onRecord = { [weak self] message in
            self?.updateCaptureStatus(from: message)
        }
    }

    deinit {}

    func start() {
        core.start()
    }

    func pair(platformIdentifier: String) -> Bool {
        let selectedRow = devicePickerScanState?.rows.first { $0.id == platformIdentifier }
        let didPair = core.pair(platformIdentifier: platformIdentifier)
        if didPair {
            isRecordOnlyCapture = false
            captureLabel = nil
            recordOnlyDeviceKind = nil
            selectedDeviceStore.save(platformIdentifier: platformIdentifier)
            liveActivityIdentity = liveActivityIdentity(for: selectedRow)
            liveActivityGlyph = liveActivityGlyph(for: selectedRow)
            selectedRideTitle = selectedRow?.title
            selectedConnectionRoute = selectedRow?.connectionRoute
            syncLiveActivity()
        }
        return didPair
    }

    func recordOnly(platformIdentifier: String, deviceKind: String) -> Bool {
        selectedRideTitle = nil
        selectedConnectionRoute = nil
        captureLabel = nil
        let trimmedKind = deviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedKind.isEmpty else { return false }
        let annotationKind = trimmedKind
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "=", with: " ")
        let annotations = ["device_kind=\(annotationKind)"]
        let modelHint = CutoutModelHint(deviceKind: annotationKind)
        let didStart = switch modelHint {
        case .falcon:
            core.pair(platformIdentifier: platformIdentifier, model: .falcon)
        case .aero:
            core.pair(platformIdentifier: platformIdentifier, model: .aero)
        case .unknown:
            core.recordOnly(
                platformIdentifier: platformIdentifier,
                note: "unsupported picker row",
                annotations: annotations
            )
        }
        if didStart {
            selectedDeviceStore.clear()
            if modelHint != .unknown {
                core.annotateCapture(key: "device_kind", value: annotationKind)
            }
            isRecordOnlyCapture = modelHint == .unknown
            recordOnlyDeviceKind = annotationKind
            if modelHint == .unknown {
                liveActivityIdentity = nil
                liveActivityGlyph = .electricUnicycle
            }
            syncLiveActivity()
        }
        return didStart
    }

    func startCaptureLabel(_ label: CaptureQuickLabel) {
        activeCaptureLabels.insert(label)
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_start")
        if let captureFileName {
            captureStatusText = "\(label.title) started: \(captureNotificationCount) notifications -> \(captureFileName)"
        } else {
            captureStatusText = "\(label.title) started"
        }
    }

    func flushCapture() {
        core.flushCapture()
    }

    func stopCaptureLabel(_ label: CaptureQuickLabel) {
        activeCaptureLabels.remove(label)
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_stop")
        if let captureFileName {
            captureStatusText = "\(label.title) stopped: \(captureNotificationCount) notifications -> \(captureFileName)"
        } else {
            captureStatusText = "\(label.title) stopped"
        }
    }

    func disconnectAndSearch() {
        endLiveActivity(reason: .disconnected)
        isRecordOnlyCapture = false
        activeCaptureLabels.removeAll()
        captureLabel = nil
        recordOnlyDeviceKind = nil
        selectedRideTitle = nil
        selectedConnectionRoute = nil
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        selectedDeviceStore.clear()
        core.disconnectAndScan()
    }

    func endLiveActivity(reason: LiveActivityRideLifecycleEndReason = .sessionEnded) {
        liveActivityCoordinator.end(reason: reason)
        lastLiveActivitySnapshot = nil
        lastLiveActivityUpdate = nil
    }

    func applyProtocolIdentityCandidate(_ candidate: DevicePickerDiscoveryCandidate?) {
        guard isRecordOnlyCapture != true else {
            liveActivityIdentity = nil
            liveActivityGlyph = .electricUnicycle
            syncLiveActivity()
            return
        }
        if let model = candidate?.support.electricUnicycleModel {
            liveActivityIdentity = .model(model)
            liveActivityGlyph = .electricUnicycle
        }
        guard candidate?.support.isSupported == true else {
            syncLiveActivity()
            return
        }
        maybeSetSelectedRideTitle(from: candidate)
        if candidate?.support.connectionRoute == .vescOnewheel {
            liveActivityIdentity = vescRideIdentity(using: candidate?.detail)
            liveActivityGlyph = .floatwheelAtom
        }
        syncLiveActivity()
    }

    private func handleScanStateChange(_ scanState: DevicePickerScanState) {
        devicePickerScanState = scanState
        guard phase == .starting || phase == .scanning else { return }
        guard let platformIdentifier = selectedDeviceStore.platformIdentifier else { return }
        guard scanState.storedSupportedRow(platformIdentifier: platformIdentifier) != nil else { return }
        _ = pair(platformIdentifier: platformIdentifier)
    }

    private func handlePhaseChange(_ phase: SessionConnectionPhase) {
        self.phase = phase
        guard case .failed = phase else { return }
        let rows = devicePickerScanState?.rows ?? []
        devicePickerScanState = .failed(phase.displayText, rows: rows)
    }

    private func syncLiveActivity() {
        let shouldBeActive = phase.supportsLiveActivity && liveActivityIdentity != nil && isRecordOnlyCapture == false
        let snapshot = liveActivityIdentity.map {
            LiveActivityRideSnapshot(identity: $0, glyph: liveActivityGlyph, rideState: rideState, now: core.now())
        }
        let endReason: LiveActivityRideLifecycleEndReason = switch phase {
        case .scanning:
            .disconnected
        case .failed:
            .unavailable
        default:
            .sessionEnded
        }
        guard shouldReconcileLiveActivity(snapshot: snapshot, shouldBeActive: shouldBeActive) else { return }
        liveActivityCoordinator.reconcile(snapshot: snapshot, shouldBeActive: shouldBeActive, endReason: endReason)
    }

    private func maybeSetSelectedRideTitle(from candidate: DevicePickerDiscoveryCandidate?) {
        guard selectedRideTitle == nil else { return }
        selectedRideTitle = candidate?.detail
    }

    private func vescRideIdentity(using title: String?) -> LiveActivityRideIdentity {
        .device(title ?? VescRideSnapshot.defaultTitle)
    }

    private func liveActivityIdentity(for selectedRow: DevicePickerRow?) -> LiveActivityRideIdentity? {
        if selectedRow?.connectionRoute == .vescOnewheel {
            return vescRideIdentity(using: selectedRow?.title)
        }
        if core.protocolIdentityCandidate?.support.connectionRoute == .vescOnewheel {
            return vescRideIdentity(using: core.protocolIdentityCandidate?.detail)
        }
        if let model = selectedRow?.electricUnicycleModel
            ?? core.protocolIdentityCandidate?.support.electricUnicycleModel
            ?? phase.connectingModel {
            return .model(model)
        }
        return nil
    }

    private func liveActivityGlyph(for selectedRow: DevicePickerRow?) -> LiveActivityRideGlyph {
        if selectedRow?.connectionRoute == .vescOnewheel
            || core.protocolIdentityCandidate?.support.connectionRoute == .vescOnewheel {
            return .floatwheelAtom
        }
        return .electricUnicycle
    }

    private func shouldReconcileLiveActivity(
        snapshot: LiveActivityRideSnapshot?,
        shouldBeActive: Bool
    ) -> Bool {
        let now = core.now()
        guard shouldBeActive else {
            lastLiveActivitySnapshot = nil
            lastLiveActivityUpdate = nil
            return true
        }
        guard let snapshot else { return false }
        guard snapshot != lastLiveActivitySnapshot else { return false }
        let previousSnapshot = lastLiveActivitySnapshot
        guard
            previousSnapshot == nil
                || snapshot.connectionState != previousSnapshot?.connectionState
                || lastLiveActivityUpdate.map({ now.rawValue >= $0.rawValue + Self.liveActivityUpdateIntervalMilliseconds }) != false
        else { return false }

        lastLiveActivitySnapshot = snapshot
        lastLiveActivityUpdate = now
        return true
    }

    private func updateCaptureStatus(from message: String) {
        if message.hasPrefix("capture_file=") {
            captureFileName = URL(fileURLWithPath: String(message.dropFirst("capture_file=".count))).lastPathComponent
            captureNotificationCount = 0
            captureStatusText = captureFileName.map { "Recording locally: \($0)" }
        } else if message.hasPrefix("record_only_notification=") || message.hasPrefix("notification=") {
            captureNotificationCount += 1
            let suffix = captureFileName.map { " -> \($0)" } ?? ""
            let prefix = captureLabel.map { "\($0):" } ?? "Recording:"
            captureStatusText = "\(prefix) \(captureNotificationCount) notifications\(suffix)"
        } else if message.hasPrefix("capture_label=") {
            // UI already updated by labelCapture(_:).
        } else if message.hasPrefix("disconnected="), let captureFileName {
            captureStatusText = "Saved capture: \(captureFileName)"
        } else if message.hasPrefix("capture_error=") {
            captureStatusText = "Capture failed"
        }
    }
}

private extension SessionConnectionPhase {
    var supportsLiveActivity: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing, .live:
            true
        case .starting, .bluetoothUnavailable, .scanning, .failed:
            false
        }
    }

    var connectingModel: ElectricUnicycleModel? {
        guard case .connecting(let model) = self else { return nil }
        return model
    }
}

enum CaptureQuickLabel: CaseIterable, Hashable, Identifiable {
    case ride
    case charge
    case balance
    case lowBeamOn
    case lowBeamOff
    case highBeamOn
    case highBeamOff
    case horn
    case pedalsHard
    case pedalsMedium
    case pedalsSoft
    case resetTrip
    case softwareLock
    case softwareUnlock
    case tiltbackSpeed
    case alarmSpeed
    case angleAdjustment
    case rideMode
    case pwmPercent

    var id: String { annotationValue }

    var title: String {
        switch self {
        case .ride:
            "Ride"
        case .charge:
            "Charge"
        case .balance:
            "Balance"
        case .lowBeamOn:
            "Low beam on"
        case .lowBeamOff:
            "Low beam off"
        case .highBeamOn:
            "High beam on"
        case .highBeamOff:
            "High beam off"
        case .horn:
            "Horn"
        case .pedalsHard:
            "Pedals hard"
        case .pedalsMedium:
            "Pedals medium"
        case .pedalsSoft:
            "Pedals soft"
        case .resetTrip:
            "Reset trip"
        case .softwareLock:
            "Software lock"
        case .softwareUnlock:
            "Software unlock"
        case .tiltbackSpeed:
            "Tiltback speed"
        case .alarmSpeed:
            "Alarm speed"
        case .angleAdjustment:
            "Angle adjust"
        case .rideMode:
            "Ride mode"
        case .pwmPercent:
            "PWM percent"
        }
    }

    var annotationValue: String {
        switch self {
        case .ride:
            "ride"
        case .charge:
            "charging"
        case .balance:
            "balancing"
        case .lowBeamOn:
            "low_beam_on"
        case .lowBeamOff:
            "low_beam_off"
        case .highBeamOn:
            "high_beam_on"
        case .highBeamOff:
            "high_beam_off"
        case .horn:
            "horn"
        case .pedalsHard:
            "pedals_hard"
        case .pedalsMedium:
            "pedals_medium"
        case .pedalsSoft:
            "pedals_soft"
        case .resetTrip:
            "reset_trip"
        case .softwareLock:
            "software_lock"
        case .softwareUnlock:
            "software_unlock"
        case .tiltbackSpeed:
            "tiltback_speed"
        case .alarmSpeed:
            "alarm_speed"
        case .angleAdjustment:
            "angle_adjustment"
        case .rideMode:
            "ride_mode"
        case .pwmPercent:
            "pwm_percent"
        }
    }
}
