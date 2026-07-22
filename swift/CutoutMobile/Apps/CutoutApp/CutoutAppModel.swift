import CutoutMobile
import Foundation
import Observation

enum CaptureStatus: Equatable {
    case recordingLocally(fileName: String)
    case recording(label: String?, notificationCount: Int, fileName: String?)
    case labelStarted(label: String, notificationCount: Int, fileName: String?)
    case labelStopped(label: String, notificationCount: Int, fileName: String?)
    case saved(fileName: String)
    case failed

    var displayText: String {
        switch self {
        case let .recordingLocally(fileName):
            "Recording locally: \(fileName)"
        case let .recording(label, notificationCount, fileName):
            captureProgressText(label: label, notificationCount: notificationCount, fileName: fileName)
        case let .labelStarted(label, notificationCount, fileName):
            captureLabelText(label: label, action: "started", notificationCount: notificationCount, fileName: fileName)
        case let .labelStopped(label, notificationCount, fileName):
            captureLabelText(label: label, action: "stopped", notificationCount: notificationCount, fileName: fileName)
        case let .saved(fileName):
            "Saved capture: \(fileName)"
        case .failed:
            "Capture failed"
        }
    }

    var accessibilityAnnouncement: String? {
        switch self {
        case let .labelStarted(label, _, _):
            "\(label) capture started"
        case let .labelStopped(label, _, _):
            "\(label) capture stopped"
        case .saved:
            "Capture saved"
        case .failed:
            "Capture failed"
        case .recordingLocally, .recording:
            nil
        }
    }

    private func captureProgressText(label: String?, notificationCount: Int, fileName: String?) -> String {
        let prefix = label.map { "\($0):" } ?? "Recording:"
        let suffix = fileName.map { " -> \($0)" } ?? ""
        return "\(prefix) \(notificationCount) notifications\(suffix)"
    }

    private func captureLabelText(
        label: String,
        action: String,
        notificationCount: Int,
        fileName: String?
    ) -> String {
        guard let fileName else { return "\(label) \(action)" }
        return "\(label) \(action): \(notificationCount) notifications -> \(fileName)"
    }
}

@MainActor
private protocol CutoutSessionDriving: AnyObject {
    var onDisplayStateChange: ((RideDisplayState) -> Void)? { get set }
    var onPhaseChange: ((SessionConnectionPhase) -> Void)? { get set }
    var onCaptureEvent: ((CaptureEvent) -> Void)? { get set }
    var onScanStateChange: ((DevicePickerScanState) -> Void)? { get set }
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)? { get set }
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)? { get set }
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)? { get set }
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto) -> Void)? { get set }
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)? { get set }
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { get }

    func start()
    func pair(platformIdentifier: String) -> Bool
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool
    func annotateCapture(label: String)
    func annotateCapture(key: String, value: String)
    func flushCapture()
    func disconnectAndScan()
    func now() -> MonotonicMilliseconds
}

extension CutoutSessionCore: CutoutSessionDriving {}

@MainActor
@Observable
final class CutoutAppModel {
    private(set) var displayState = RideDisplayState()
    private(set) var phase = SessionConnectionPhase.starting
    private(set) var devicePickerScanState: DevicePickerScanState?
    private(set) var selectedRideTitle: String?
    private(set) var selectedConnectionRoute: DevicePickerConnectionRoute?
    private(set) var settingsReadback: SettingsReadback?
    private(set) var faultHistoryReadback: FaultHistoryReadback?
    private(set) var bmsSnapshot: BmsSnapshot?
    private(set) var phoneLocationReadback = PhoneLocationReadback(
        snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    )
    private(set) var captureStatus: CaptureStatus?
    private(set) var isRecordOnlyCapture = false
    private(set) var activeCaptureLabels = Set<CaptureQuickLabel>()
    private(set) var recordOnlyDeviceKind: String?

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

    var captureStatusText: String? {
        captureStatus?.displayText
    }

    private let core: any CutoutSessionDriving
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
        core = Self.makeSessionDriver()
        self.core.onDisplayStateChange = { [weak self] displayState in
            self?.displayState = displayState
            self?.syncLiveActivity()
        }
        self.core.onPhaseChange = { [weak self] phase in
            self?.handlePhaseChange(phase)
            self?.syncLiveActivity()
        }
        self.core.onScanStateChange = { [weak self] scanState in
            self?.handleScanStateChange(scanState)
        }
        self.core.onSettingsReadbackChange = { [weak self] settingsReadback in
            self?.settingsReadback = settingsReadback
        }
        self.core.onFaultHistoryReadbackChange = { [weak self] faultHistoryReadback in
            self?.faultHistoryReadback = faultHistoryReadback
        }
        self.core.onBmsSnapshotChange = { [weak self] bmsSnapshot in
            self?.bmsSnapshot = bmsSnapshot
        }
        self.core.onPhoneLocationSnapshotChange = { [weak self] snapshot in
            self?.phoneLocationReadback = PhoneLocationReadback(snapshot: snapshot)
        }
        self.core.onProtocolIdentityCandidateChange = { [weak self] candidate in
            self?.applyProtocolIdentityCandidate(candidate)
        }
        self.core.onCaptureEvent = { [weak self] event in
            self?.applyCaptureEvent(event)
        }
    }

    deinit {}

    func start() {
        core.start()
    }

    func pair(platformIdentifier: String) -> Bool {
        let rows = devicePickerScanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }) else {
            phase = .scanning
            devicePickerScanState = .failed("Device is no longer available", rows: rows)
            return false
        }

        selectedRideTitle = selectedRow.title
        selectedConnectionRoute = selectedRow.connectionRoute
        phase = .discoveringServices
        let didPair = core.pair(platformIdentifier: platformIdentifier)
        if didPair {
            isRecordOnlyCapture = false
            captureLabel = nil
            recordOnlyDeviceKind = nil
            selectedDeviceStore.save(platformIdentifier: platformIdentifier)
            liveActivityIdentity = liveActivityIdentity(for: selectedRow)
            liveActivityGlyph = liveActivityGlyph(for: selectedRow)
            syncLiveActivity()
        } else {
            phase = .scanning
            devicePickerScanState = .failed("Device is no longer available", rows: rows)
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
        guard activeCaptureLabels.insert(label).inserted else { return }
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_start")
        captureStatus = .labelStarted(
            label: label.title,
            notificationCount: captureNotificationCount,
            fileName: captureFileName
        )
    }

    func flushCapture() {
        core.flushCapture()
    }

    func stopCaptureLabel(_ label: CaptureQuickLabel) {
        guard activeCaptureLabels.remove(label) != nil else { return }
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_stop")
        captureStatus = .labelStopped(
            label: label.title,
            notificationCount: captureNotificationCount,
            fileName: captureFileName
        )
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
        if phase == .live,
           selectedConnectionRoute == nil,
           let candidate = core.protocolIdentityCandidate,
           candidate.support.isSupported {
            maybeSetSelectedRideTitle(from: candidate)
            selectedConnectionRoute = candidate.support.connectionRoute
        }
        guard case .failed = phase else { return }
        let rows = devicePickerScanState?.rows ?? []
        devicePickerScanState = .failed(phase.displayText, rows: rows)
    }

    private static func makeSessionDriver() -> any CutoutSessionDriving {
        #if DEBUG
        if let fixture = CutoutUITestSessionFixture(
            value: UserDefaults.standard.string(forKey: "CUTOUT_UI_TEST_FIXTURE")
        ) ?? CutoutUITestSessionFixture(
            value: ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_FIXTURE"]
        ) ?? CutoutUITestSessionFixture(arguments: CommandLine.arguments) {
            return CutoutUITestSessionDriver(fixture: fixture)
        }
        #endif
        return CutoutSessionCore()
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

    func applyCaptureEvent(_ event: CaptureEvent) {
        switch event {
        case let .started(fileURL):
            captureFileName = fileURL.lastPathComponent
            captureNotificationCount = 0
            captureStatus = captureFileName.map(CaptureStatus.recordingLocally)
        case .notificationRecorded:
            captureNotificationCount += 1
            captureStatus = .recording(
                label: captureLabel,
                notificationCount: captureNotificationCount,
                fileName: captureFileName
            )
        case let .finished(fileURL):
            captureFileName = fileURL.lastPathComponent
            captureStatus = .saved(fileName: fileURL.lastPathComponent)
        case .failed:
            captureStatus = .failed
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

    func actionTitle(isActive: Bool) -> String {
        "\(isActive ? "Stop" : "Start") \(title)"
    }

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

#if DEBUG
private enum CutoutUITestSessionFixture {
    case vesc
    case failedVesc
    case euc
    case vescLiveActivity
    case autoVescLiveActivity

    init?(value: String?) {
        switch value {
        case "vesc": self = .vesc
        case "vesc-failure": self = .failedVesc
        case "euc": self = .euc
        case "vesc-live-activity": self = .vescLiveActivity
        case "vesc-live-activity-auto": self = .autoVescLiveActivity
        default: return nil
        }
    }

    init?(arguments: [String]) {
        if arguments.contains("--ui-test-live-activity-auto") {
            self = .autoVescLiveActivity
        } else if arguments.contains("--ui-test-vesc-failure") {
            self = .failedVesc
        } else if arguments.contains("--ui-test-euc") {
            self = .euc
        } else if arguments.contains("--ui-test-live-activity") {
            self = .vescLiveActivity
        } else if arguments.contains("--ui-test-vesc") {
            self = .vesc
        } else {
            return nil
        }
    }

    var candidate: DevicePickerDiscoveryCandidate {
        switch self {
        case .euc:
            DevicePickerDiscoveryCandidate(
                platformIdentifier: "ui-test-euc",
                displayName: "Test EUC",
                productCategory: "Electric unicycle",
                evidence: "UI test fixture",
                detail: "Deterministic accessibility test device",
                support: .supported(
                    connectionRoute: .electricUnicycle,
                    electricUnicycleModel: .aero
                ),
                symbolName: "circle.hexagongrid.circle"
            )
        case .vesc, .failedVesc, .vescLiveActivity, .autoVescLiveActivity:
            DevicePickerDiscoveryCandidate(
                platformIdentifier: "ui-test-vesc",
                displayName: "Refloat VESC",
                productCategory: "VESC Onewheel",
                evidence: "UI test fixture",
                detail: "Deterministic accessibility test device",
                support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
                symbolName: "circle.hexagongrid.circle"
            )
        }
    }

    var startsLive: Bool { self == .autoVescLiveActivity }
    var failsConnection: Bool { self == .failedVesc }
}

@MainActor
private final class CutoutUITestSessionDriver: CutoutSessionDriving {
    var onDisplayStateChange: ((RideDisplayState) -> Void)?
    var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    var onCaptureEvent: ((CaptureEvent) -> Void)?
    var onScanStateChange: ((DevicePickerScanState) -> Void)?
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto) -> Void)?
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
    private(set) var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?

    private let fixture: CutoutUITestSessionFixture
    private var connectionTask: Task<Void, Never>?

    init(fixture: CutoutUITestSessionFixture) {
        self.fixture = fixture
    }

    deinit {
        connectionTask?.cancel()
    }

    func start() {
        connectionTask?.cancel()
        onDisplayStateChange?(RideDisplayState())
        if fixture.startsLive {
            emitLiveState()
            return
        }
        onScanStateChange?(DevicePickerScanState(status: .idle, rows: [fixture.candidate.pickerRow]))
        onPhaseChange?(.scanning)
    }

    func pair(platformIdentifier: String) -> Bool {
        guard platformIdentifier == fixture.candidate.platformIdentifier else { return false }
        connectionTask?.cancel()
        onPhaseChange?(.discoveringServices)
        connectionTask = Task { [weak self] in
            guard let self else { return }
            do {
                try await Task.sleep(for: .milliseconds(self.fixture.failsConnection ? 3_000 : 250))
            } catch {
                return
            }
            guard !Task.isCancelled else { return }
            if self.fixture.failsConnection {
                self.onPhaseChange?(.failed(.connectFailed("deterministic fixture")))
            } else {
                self.emitLiveState()
            }
        }
        return true
    }

    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool {
        pair(platformIdentifier: platformIdentifier)
    }

    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool {
        platformIdentifier == fixture.candidate.platformIdentifier
    }

    func annotateCapture(label: String) {}

    func annotateCapture(key: String, value: String) {}

    func flushCapture() {}

    func disconnectAndScan() {
        connectionTask?.cancel()
        protocolIdentityCandidate = nil
        onProtocolIdentityCandidateChange?(nil)
        onDisplayStateChange?(RideDisplayState())
        onScanStateChange?(DevicePickerScanState(status: .idle, rows: [fixture.candidate.pickerRow]))
        onPhaseChange?(.scanning)
    }

    func now() -> MonotonicMilliseconds {
        MonotonicMilliseconds(UInt64(ProcessInfo.processInfo.systemUptime * 1_000))
    }

    private func emitLiveState() {
        let now = now()
        if fixture.startsLive {
            protocolIdentityCandidate = fixture.candidate
            onProtocolIdentityCandidateChange?(fixture.candidate)
        }

        if fixture == .euc {
            let telemetry = TelemetrySnapshot(
                at: now,
                speed: Speed(value: 12_000),
                speedSource: .reported,
                speedQuality: .known,
                operatingState: .riding,
                voltage: Voltage(value: 82_000),
                batteryCurrent: BatteryCurrent(value: 8_000),
                controllerTemperature: Temperature(value: 31_000),
                batteryLevelReported: BatteryLevel(value: 64)
            )
            onDisplayStateChange?(RideDisplayState(
                speed: SpeedReadout(snapshot: telemetry),
                telemetry: telemetry,
                notificationCount: 1,
                lastUpdate: now
            ))
            onBmsSnapshotChange?(eucBmsSnapshot)
        } else {
            let telemetry = TelemetrySnapshot(
                at: now,
                speed: Speed(value: 8_000),
                speedSource: .reported,
                speedQuality: .known,
                operatingState: .riding,
                voltage: Voltage(value: 50_400),
                batteryCurrent: BatteryCurrent(value: 12_000),
                controllerTemperature: Temperature(value: 32_000),
                batteryLevelReported: BatteryLevel(value: 72)
            )
            onDisplayStateChange?(RideDisplayState(
                speed: SpeedReadout(snapshot: telemetry),
                telemetry: telemetry,
                notificationCount: 1,
                lastUpdate: now
            ))
        }
        onPhaseChange?(.live)
    }

    private var eucBmsSnapshot: BmsSnapshot {
        BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "20S4P test pack",
                seriesGroupCount: 20,
                parallelCount: 4,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            pageKind: "overview",
            pageVerification: .hardwareVerified,
            energyPercent: BatteryLevel(value: 64),
            voltage: Voltage(value: 82_000),
            current: BatteryCurrent(value: 8_000),
            cellDelta: VoltageDelta(value: 24),
            lowestGroupIndex: 7,
            highestTemperature: Temperature(value: 38_000),
            temperatureReadings: [Temperature(value: 38_000), Temperature(value: 34_000)],
            highestTemperatureLabel: "right pack",
            balancingSummary: "balancing 2 groups",
            balancingDetail: "groups 7 and 12",
            faultSummary: "no active faults",
            faultDetail: "last fault unavailable",
            groups: [
                BmsGroupSnapshot(
                    index: 7,
                    label: "right pack group 7",
                    voltage: Voltage(value: 4_036),
                    temperature: Temperature(value: 38_000),
                    isBalancing: true,
                    alertLevel: .warning,
                    detail: "lowest group"
                ),
                BmsGroupSnapshot(
                    index: 12,
                    label: "right pack group 12",
                    voltage: Voltage(value: 4_060),
                    temperature: Temperature(value: 34_000),
                    isBalancing: true,
                    alertLevel: .nominal
                )
            ]
        )
    }
}
#endif
