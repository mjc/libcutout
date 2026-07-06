import CutoutMobile
import Foundation

final class CutoutAppModel: ObservableObject {
    @Published private(set) var displayState = RideDisplayState()
    @Published private(set) var phase = SessionConnectionPhase.starting
    @Published private(set) var devicePickerScanState: DevicePickerScanState?
    @Published private(set) var selectedRideTitle: String?
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

    var rideState: EucRideScreenState {
        EucRideScreenState(phase: phase, displayState: displayState)
    }

    private let core = CutoutSessionCore()
    private let selectedDeviceStore = DevicePickerSelectionStore()
    private var captureFileName: String?
    private var captureNotificationCount = 0
    private var captureLabel: String?

    init() {
        core.onDisplayStateChange = { [weak self] displayState in
            self?.displayState = displayState
        }
        core.onPhaseChange = { [weak self] phase in
            self?.phase = phase
        }
        core.onScanStateChange = { [weak self] scanState in
            self?.devicePickerScanState = scanState
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
            guard self?.isRecordOnlyCapture != true else { return }
            guard case .supported = candidate?.support else { return }
            self?.selectedRideTitle = candidate?.detail
        }
        core.onRecord = { [weak self] message in
            self?.updateCaptureStatus(from: message)
        }
    }

    func start() {
        core.start()
    }

    func pair(platformIdentifier: String) -> Bool {
        let didPair = core.pair(platformIdentifier: platformIdentifier)
        if didPair {
            isRecordOnlyCapture = false
            captureLabel = nil
            recordOnlyDeviceKind = nil
            selectedDeviceStore.save(platformIdentifier: platformIdentifier)
            selectedRideTitle = devicePickerScanState?.rows.first(where: { $0.id == platformIdentifier })?.title
        }
        return didPair
    }

    func recordOnly(platformIdentifier: String, deviceKind: String) -> Bool {
        selectedRideTitle = nil
        captureLabel = nil
        let trimmedKind = deviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedKind.isEmpty else { return false }
        let annotationKind = trimmedKind
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
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
                core.annotateCapture(label: "device_kind=\(annotationKind)")
            }
            isRecordOnlyCapture = modelHint == .unknown
            recordOnlyDeviceKind = annotationKind
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
        isRecordOnlyCapture = false
        activeCaptureLabels.removeAll()
        captureLabel = nil
        recordOnlyDeviceKind = nil
        selectedRideTitle = nil
        selectedDeviceStore.clear()
        core.disconnectAndScan()
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
