import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

enum CaptureStatus: Equatable {
    case recordingLocally(fileName: String)
    case recording(label: String?, notificationCount: Int, fileName: String?)
    case labelStarted(label: String, notificationCount: Int, fileName: String?)
    case labelStopped(label: String, notificationCount: Int, fileName: String?)
    case saved(fileName: String)
    case failed

    var isRecording: Bool {
        switch self {
        case .recordingLocally, .recording, .labelStarted, .labelStopped:
            true
        case .saved, .failed:
            false
        }
    }

    var displayText: String {
        switch self {
        case let .recordingLocally(fileName):
            localizedAppText("capture.status.recording_locally", fileName)
        case let .recording(label, notificationCount, fileName):
            captureProgressText(label: label, notificationCount: notificationCount, fileName: fileName)
        case let .labelStarted(label, notificationCount, fileName):
            captureLabelText(label: label, action: "started", notificationCount: notificationCount, fileName: fileName)
        case let .labelStopped(label, notificationCount, fileName):
            captureLabelText(label: label, action: "stopped", notificationCount: notificationCount, fileName: fileName)
        case let .saved(fileName):
            localizedAppText("capture.status.saved", fileName)
        case .failed:
            localizedAppText("capture.announcement.failed")
        }
    }

    var accessibilityAnnouncement: String? {
        switch self {
        case let .labelStarted(label, _, _):
            localizedAppText("capture.announcement.label_started", label)
        case let .labelStopped(label, _, _):
            localizedAppText("capture.announcement.label_stopped", label)
        case .saved:
            localizedAppText("capture.announcement.saved")
        case .failed:
            localizedAppText("capture.announcement.failed")
        case .recordingLocally, .recording:
            nil
        }
    }

    var statusStripTone: PevStatusStripTone {
        switch self {
        case .failed: .critical
        default: .nominal
        }
    }

    private func captureProgressText(label: String?, notificationCount: Int, fileName: String?) -> String {
        switch (label, fileName) {
        case let (.some(label), .some(fileName)):
            localizedAppText("capture.status.recording_labeled_file", label, Int64(notificationCount), fileName)
        case let (.some(label), nil):
            localizedAppText("capture.status.recording_labeled", label, Int64(notificationCount))
        case let (nil, .some(fileName)):
            localizedAppText("capture.status.recording_file", Int64(notificationCount), fileName)
        case (nil, nil):
            localizedAppText("capture.status.recording", Int64(notificationCount))
        }
    }

    private func captureLabelText(
        label: String,
        action: String,
        notificationCount: Int,
        fileName: String?
    ) -> String {
        let key = switch (action, fileName) {
        case ("started", .some): "capture.status.label_started_file"
        case ("stopped", .some): "capture.status.label_stopped_file"
        case ("started", nil): "capture.status.label_started"
        default: "capture.status.label_stopped"
        }
        guard let fileName else { return localizedAppText(key, label) }
        return localizedAppText(key, label, Int64(notificationCount), fileName)
    }
}

struct ConnectionSelection: Equatable {
    let platformIdentifier: String
    let title: String
    let route: DevicePickerConnectionRoute
}

enum ConnectionState: Equatable {
    case picker
    case identified(ConnectionSelection)
    case connecting(ConnectionSelection, phase: SessionConnectionPhase)
    case retrying(ConnectionSelection, retry: SessionConnectionRetry)
    case connected(ConnectionSelection)
    case failed(ConnectionSelection, SessionConnectionFailure)

    var selection: ConnectionSelection? {
        switch self {
        case .picker:
            nil
        case let .identified(selection), let .connecting(selection, _), let .retrying(selection, _), let .connected(selection), let .failed(selection, _):
            selection
        }
    }

    var statusText: String? {
        switch self {
        case .picker, .identified:
            nil
        case let .connecting(_, phase):
            phase.displayText
        case .retrying:
            localizedAppText("picker.status.retrying")
        case .connected:
            SessionConnectionPhase.live.displayText
        case let .failed(_, failure):
            failure.displayText
        }
    }

    func navigationIntent(isRecordOnlyCapture: Bool) -> PhaseNavigationIntent {
        guard !isRecordOnlyCapture else { return .stay }

        switch self {
        case let .connected(selection):
            return .openRide(selection.route)
        case .failed:
            return .returnToPicker
        case .picker, .identified, .connecting, .retrying:
            return .stay
        }
    }
}

enum PhaseNavigationIntent: Equatable {
    case stay
    case openRide(DevicePickerConnectionRoute)
    case returnToPicker
}

@MainActor
protocol CutoutSessionDriving: AnyObject {
    var onDisplayStateChange: ((RideDisplayState) -> Void)? { get set }
    var onPhaseChange: ((SessionConnectionPhase) -> Void)? { get set }
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)? { get set }
    var onCaptureEvent: ((CaptureEvent) -> Void)? { get set }
    var onScanStateChange: ((DevicePickerScanState) -> Void)? { get set }
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)? { get set }
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)? { get set }
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)? { get set }
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)? { get set }
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)? { get set }
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { get }

    func start()
    func pair(platformIdentifier: String) -> Bool
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool
    func probe(platformIdentifier: String) -> Bool
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool
    func annotateCapture(label: String)
    func annotateCapture(key: String, value: String)
    func flushCapture() -> Bool
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
    private(set) var connectionState = ConnectionState.picker
    private(set) var settingsReadback: SettingsReadback?
    private(set) var faultHistoryReadback: FaultHistoryReadback?
    private(set) var bmsSnapshot: BmsSnapshot?
    private(set) var phoneLocationReadback = PhoneLocationReadback(
        snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    )
    private(set) var captureStatus: CaptureStatus?
    private(set) var captureProgress: CaptureProgress?
    private(set) var liveActivityError: LiveActivityRideLifecycleError?
    private(set) var isRecordOnlyCapture = false
    private(set) var isFinishingCapture = false
    private(set) var activeCaptureLabels = Set<CaptureQuickLabel>()
    private(set) var recordOnlyDeviceKind: String?
    private(set) var hasSavedDevice = false

    var selectedRideTitle: String? {
        connectionState.selection?.title
    }

    var selectedConnectionRoute: DevicePickerConnectionRoute? {
        connectionState.selection?.route
    }

    var speed: SpeedReadout {
        displayState.speed
    }

    var currentMonotonicTime: MonotonicMilliseconds {
        core.now()
    }

    var rideState: EucRideScreenState {
        EucRideScreenState(phase: phase, displayState: displayState)
    }

    var eucRidePresentationState: EucRideScreenState? {
        guard selectedRideTitle != nil || phase != .starting || displayState.notificationCount != 0 else {
            return nil
        }
        return rideState
    }

    var vescRideSnapshot: VescRideSnapshot? {
        VescRideSnapshot(displayState: displayState, title: selectedRideTitle)
    }

    var captureStatusText: String? {
        captureStatus?.displayText
    }

    var connectionStatusText: String {
        connectionState.statusText ?? phase.displayText
    }

    private let core: any CutoutSessionDriving
    private let liveActivityCoordinator: LiveActivityRideLifecycleCoordinator
    private let selectedDeviceStore: DevicePickerSelectionStore
    private var liveActivityIdentity: LiveActivityRideIdentity?
    private var liveActivityGlyph = LiveActivityRideGlyph.electricUnicycle
    private var lastLiveActivitySnapshot: LiveActivityRideSnapshot?
    private var lastLiveActivityUpdate: MonotonicMilliseconds?
    private var captureFileName: String?
    private var captureNotificationCount = 0
    private var captureLabel: String?
    private var permitsStoredDeviceAutoPairing = true
    private static let liveActivityUpdateIntervalMilliseconds: UInt64 = 1_000

    convenience init() {
        self.init(
            core: Self.makeSessionDriver(),
            permitsStoredDeviceAutoPairing: Self.uiTestFixture == nil,
            selectedDeviceStore: DevicePickerSelectionStore(),
            liveActivityManager: LiveActivityRideActivityKitManager()
        )
    }

    convenience init(
        core: any CutoutSessionDriving,
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        liveActivityManager: any LiveActivityRideLifecycleManaging = LiveActivityRideActivityKitManager()
    ) {
        self.init(
            core: core,
            permitsStoredDeviceAutoPairing: true,
            selectedDeviceStore: selectedDeviceStore,
            liveActivityManager: liveActivityManager
        )
    }

    private init(
        core: any CutoutSessionDriving,
        permitsStoredDeviceAutoPairing: Bool,
        selectedDeviceStore: DevicePickerSelectionStore,
        liveActivityManager: any LiveActivityRideLifecycleManaging
    ) {
        self.permitsStoredDeviceAutoPairing = permitsStoredDeviceAutoPairing
        self.core = core
        liveActivityCoordinator = LiveActivityRideLifecycleCoordinator(manager: liveActivityManager)
        self.selectedDeviceStore = selectedDeviceStore
        hasSavedDevice = selectedDeviceStore.platformIdentifier != nil
        self.core.onDisplayStateChange = { [weak self] displayState in
            self?.displayState = displayState
            self?.syncLiveActivity()
        }
        self.core.onPhaseChange = { [weak self] phase in
            self?.handlePhaseChange(phase)
            self?.syncLiveActivity()
        }
        self.core.onReconnectScheduled = { [weak self] retry in
            self?.handleReconnectScheduled(retry)
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
        self.core.onPhoneLocationSnapshotChange = { [weak self] snapshot, receivedAt in
            self?.phoneLocationReadback = PhoneLocationReadback(snapshot: snapshot, receivedAt: receivedAt)
        }
        self.core.onProtocolIdentityCandidateChange = { [weak self] candidate in
            self?.applyProtocolIdentityCandidate(candidate)
        }
        self.core.onCaptureEvent = { [weak self] event in
            self?.applyCaptureEvent(event)
        }
    }

    func start() {
        core.start()
    }

    func pair(platformIdentifier: String) -> Bool {
        let rows = devicePickerScanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }) else {
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
            return false
        }
        guard selectedRow.isSupported, let route = selectedRow.connectionRoute else { return false }

        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: route
        )
        connectionState = .connecting(selection, phase: .discoveringServices)
        permitsStoredDeviceAutoPairing = true
        phase = .discoveringServices
        let didPair = core.pair(platformIdentifier: platformIdentifier)
        if didPair {
            isRecordOnlyCapture = false
            captureLabel = nil
            recordOnlyDeviceKind = nil
            selectedDeviceStore.save(platformIdentifier: platformIdentifier)
            hasSavedDevice = true
            liveActivityIdentity = liveActivityIdentity(for: selectedRow)
            liveActivityGlyph = liveActivityGlyph(for: selectedRow)
            syncLiveActivity()
        } else {
            connectionState = .picker
            permitsStoredDeviceAutoPairing = false
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
        }
        return didPair
    }

    func recordOnly(platformIdentifier: String, deviceKind: String) -> Bool {
        connectionState = .picker
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
            resetCaptureSession()
            permitsStoredDeviceAutoPairing = false
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

    func startProbe(platformIdentifier: String) -> Bool {
        let rows = devicePickerScanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }),
              selectedRow.isProbeRecommended
        else {
            return false
        }
        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: .electricUnicycle
        )
        connectionState = .connecting(selection, phase: .discoveringServices)
        phase = .discoveringServices
        let didStart = core.probe(platformIdentifier: platformIdentifier)
        if !didStart {
            connectionState = .picker
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
        }
        return didStart
    }

    private func resetCaptureSession() {
        captureStatus = nil
        captureProgress = nil
        isFinishingCapture = false
        activeCaptureLabels.removeAll()
        captureFileName = nil
        captureNotificationCount = 0
        captureLabel = nil
        recordOnlyDeviceKind = nil
    }

    func startCaptureLabel(_ label: CaptureQuickLabel) {
        guard !activeCaptureLabels.contains(label) else { return }

        if let exclusiveGroup = label.exclusiveGroup,
           let activeLabel = activeCaptureLabels.first(where: { $0.exclusiveGroup == exclusiveGroup })
        {
            activeCaptureLabels.remove(activeLabel)
            core.annotateCapture(label: "\(activeLabel.annotationValue)_stop")
        }

        activeCaptureLabels.insert(label)
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_start")
        captureStatus = .labelStarted(
            label: label.title,
            notificationCount: captureNotificationCount,
            fileName: captureFileName
        )
    }

    @discardableResult
    func flushCapture() -> Bool {
        let didFlush = core.flushCapture()
        if !didFlush, captureStatus?.isRecording == true {
            captureStatus = .failed
        }
        return didFlush
    }

    @discardableResult
    func finishCapture() -> Bool {
        guard !isFinishingCapture else { return false }
        isFinishingCapture = true

        guard flushCapture() else {
            captureStatus = .failed
            isFinishingCapture = false
            return false
        }

        disconnectTransport()
        return true
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

    func disconnectTransport() {
        endLiveActivity(reason: .disconnected)
        isRecordOnlyCapture = false
        activeCaptureLabels.removeAll()
        captureLabel = nil
        recordOnlyDeviceKind = nil
        connectionState = .picker
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        permitsStoredDeviceAutoPairing = false
        core.disconnectAndScan()
    }

    func forgetSavedDevice() {
        disconnectTransport()
        selectedDeviceStore.clear()
        hasSavedDevice = false
    }

    func endLiveActivity(reason: LiveActivityRideLifecycleEndReason = .sessionEnded) {
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.end(reason: reason)
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
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
        guard let selection = selection(from: candidate) else {
            syncLiveActivity()
            return
        }
        if connectionState.selection == nil {
            connectionState = .identified(selection)
        }
        if selection.route == .vescOnewheel {
            liveActivityIdentity = vescRideIdentity(using: selection.title)
            liveActivityGlyph = .floatwheelAtom
        }
        syncLiveActivity()
    }

    private func handleScanStateChange(_ scanState: DevicePickerScanState) {
        devicePickerScanState = scanState
        guard phase == .starting || phase == .scanning else { return }
        guard permitsStoredDeviceAutoPairing else { return }
        guard let platformIdentifier = selectedDeviceStore.platformIdentifier else { return }
        guard scanState.storedSupportedRow(platformIdentifier: platformIdentifier) != nil else { return }
        _ = pair(platformIdentifier: platformIdentifier)
    }

    private func handlePhaseChange(_ phase: SessionConnectionPhase) {
        guard !phase.supportsLiveActivity || connectionState.selection != nil || permitsStoredDeviceAutoPairing else { return }
        if case .live = phase {
            switch connectionState {
            case .connecting(_, phase: .subscribing), .identified:
                break
            default:
                return
            }
        }
        if case .failed = phase {
            guard connectionState.selection != nil else { return }
        }
        if case .failed = connectionState {
            switch phase {
            case .connecting, .discoveringServices, .subscribing, .live:
                return
            case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .failed:
                break
            }
        }
        self.phase = phase
        switch phase {
        case .connecting, .discoveringServices, .subscribing:
            if let selection = connectionState.selection {
                connectionState = .connecting(selection, phase: phase)
            }
        case .live:
            if let selection = connectionState.selection {
                connectionState = .connected(selection)
            } else if let selection = selection(from: core.protocolIdentityCandidate) {
                connectionState = .connected(selection)
            }
        case let .failed(failure):
            guard let selection = connectionState.selection else { return }
            connectionState = .failed(selection, failure)
            let rows = devicePickerScanState?.rows ?? []
            devicePickerScanState = .failed(phase.displayText, rows: rows)
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning:
            break
        }
    }

    private func handleReconnectScheduled(_ retry: SessionConnectionRetry) {
        guard let selection = connectionState.selection,
              selection.platformIdentifier == retry.platformIdentifier
        else { return }
        if case .failed = connectionState { return }
        connectionState = .retrying(selection, retry: retry)
    }

    private static func makeSessionDriver() -> any CutoutSessionDriving {
        #if DEBUG
        if let fixture = uiTestFixture {
            return CutoutSessionCore(testScript: fixture.testScript)
        }
        #endif
        return CutoutSessionCore()
    }

    private static var uiTestFixture: CutoutUITestSessionFixture? {
        #if DEBUG
        CutoutUITestSessionFixture.resolve(
            environmentValue: ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_FIXTURE"],
            persistedValue: UserDefaults.standard.string(forKey: "CUTOUT_UI_TEST_FIXTURE"),
            arguments: ProcessInfo.processInfo.arguments
        )
        #else
        nil
        #endif
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
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.reconcile(
                snapshot: snapshot,
                shouldBeActive: shouldBeActive,
                endReason: endReason
            )
            let error = await liveActivityCoordinator.lastError
            self?.liveActivityError = error
            if error != nil {
                self?.lastLiveActivitySnapshot = nil
                self?.lastLiveActivityUpdate = nil
            }
        }
    }

    private func selection(from candidate: DevicePickerDiscoveryCandidate?) -> ConnectionSelection? {
        guard let candidate, candidate.support.isSupported, let route = candidate.support.connectionRoute else {
            return nil
        }
        return ConnectionSelection(
            platformIdentifier: candidate.platformIdentifier,
            title: candidate.detail,
            route: route
        )
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
                || lastLiveActivityUpdate.map({ now.elapsed(since: $0).rawValue >= Self.liveActivityUpdateIntervalMilliseconds }) != false
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
        case let .progress(progress):
            captureProgress = progress
            captureNotificationCount = Int(clamping: progress.notificationCount)
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
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .failed:
            false
        }
    }

    var connectingModel: ElectricUnicycleModel? {
        guard case .connecting(let model) = self else { return nil }
        return model
    }
}

private enum CaptureLabelExclusiveGroup {
    case lowBeam
    case highBeam
    case pedalMode
    case softwareLock
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
        localizedAppText(isActive ? "capture.label.stop" : "capture.label.start", title)
    }

    var title: String {
        localizedAppText("capture.label.\(annotationValue)")
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

    fileprivate var exclusiveGroup: CaptureLabelExclusiveGroup? {
        switch self {
        case .lowBeamOn, .lowBeamOff:
            .lowBeam
        case .highBeamOn, .highBeamOff:
            .highBeam
        case .pedalsHard, .pedalsMedium, .pedalsSoft:
            .pedalMode
        case .softwareLock, .softwareUnlock:
            .softwareLock
        default:
            nil
        }
    }
}

#if DEBUG
enum CutoutUITestSessionFixture {
    case unknownDevice
    case unknownDeviceFinishFailure
    case probeDevice
    case probeTimeout
    case probeMalformedResponse
    case probeConflictingEvidence
    case probeUnsupported
    case bluetoothUnavailable
    case bluetoothPermissionDenied
    case vesc
    case dynamicVesc
    case pendingVesc
    case staleVesc
    case failedVesc
    case reconnectingVesc
    case connectingVesc
    case euc
    case dynamicEuc
    case staleEuc
    case reconnectingEuc
    case connectingEuc
    case eucOverview
    case eucNoBms
    case eucUnknownTopology
    case vescLiveActivity
    case autoVescLiveActivity

    init?(value: String?) {
        switch value {
        case "unknown-device": self = .unknownDevice
        case "unknown-device-finish-failure": self = .unknownDeviceFinishFailure
        case "probe-device": self = .probeDevice
        case "probe-timeout": self = .probeTimeout
        case "probe-malformed": self = .probeMalformedResponse
        case "probe-conflict": self = .probeConflictingEvidence
        case "probe-unsupported": self = .probeUnsupported
        case "bluetooth-unavailable": self = .bluetoothUnavailable
        case "bluetooth-permission-denied": self = .bluetoothPermissionDenied
        case "vesc": self = .vesc
        case "vesc-dynamic": self = .dynamicVesc
        case "vesc-pending": self = .pendingVesc
        case "vesc-stale": self = .staleVesc
        case "vesc-failure": self = .failedVesc
        case "vesc-reconnect": self = .reconnectingVesc
        case "vesc-connecting": self = .connectingVesc
        case "euc": self = .euc
        case "euc-dynamic": self = .dynamicEuc
        case "euc-stale": self = .staleEuc
        case "euc-reconnect": self = .reconnectingEuc
        case "euc-connecting": self = .connectingEuc
        case "euc-overview": self = .eucOverview
        case "euc-no-bms": self = .eucNoBms
        case "euc-unknown-topology": self = .eucUnknownTopology
        case "vesc-live-activity": self = .vescLiveActivity
        case "vesc-live-activity-auto": self = .autoVescLiveActivity
        default: return nil
        }
    }

    init?(arguments: [String]) {
        guard let value = Self.standardLaunchArgumentValue(arguments), let fixture = Self(value: value) else {
            return nil
        }
        self = fixture
    }

    static func resolve(
        environmentValue: String? = nil,
        persistedValue: String?,
        arguments: [String]
    ) -> Self? {
        Self(value: environmentValue)
            ?? Self(arguments: arguments)
            ?? Self(value: persistedValue)
    }

    private static func standardLaunchArgumentValue(_ arguments: [String]) -> String? {
        guard let keyIndex = arguments.firstIndex(of: "-CUTOUT_UI_TEST_FIXTURE") else { return nil }
        let valueIndex = arguments.index(after: keyIndex)
        guard valueIndex < arguments.endIndex else { return nil }
        return arguments[valueIndex]
    }

    var candidate: DevicePickerDiscoveryCandidate {
        switch self {
        case .unknownDevice, .unknownDeviceFinishFailure:
            DevicePickerDiscoveryCandidate(
                platformIdentifier: "ui-test-unknown-device",
                displayName: "Unknown BLE device",
                productCategory: "Unknown personal electric vehicle",
                evidence: "UI test fixture",
                detail: "Deterministic record-only capture device",
                support: .unknownRecordable(disabledReason: "Unknown device fixture"),
                symbolName: "questionmark.circle"
            )
        case .probeDevice, .probeTimeout, .probeMalformedResponse, .probeConflictingEvidence, .probeUnsupported:
            DevicePickerDiscoveryCandidate(
                platformIdentifier: "ui-test-probe",
                displayName: "Unknown EUC",
                productCategory: "Electric unicycle",
                evidence: "UI test fixture",
                detail: "Deterministic identification probe device",
                support: .probeRecommended(disabledReason: "Identity probe required"),
                symbolName: "magnifyingglass"
            )
        case .euc, .dynamicEuc, .staleEuc, .reconnectingEuc, .connectingEuc, .eucOverview, .eucNoBms, .eucUnknownTopology:
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
        case .bluetoothUnavailable, .bluetoothPermissionDenied, .vesc, .dynamicVesc, .pendingVesc, .staleVesc, .failedVesc, .reconnectingVesc, .connectingVesc, .vescLiveActivity, .autoVescLiveActivity:
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
    var initialBluetoothState: CutoutSessionTestInitialBluetoothState {
        switch self {
        case .bluetoothUnavailable: .unavailable
        case .bluetoothPermissionDenied: .permissionDenied
        default: .scanning
        }
    }
    var failsConnection: Bool { self == .failedVesc }
    var identificationProbeFailure: IdentificationProbeFailure? {
        switch self {
        case .probeTimeout: .timedOut
        case .probeMalformedResponse: .malformedResponse
        case .probeConflictingEvidence: .conflictingEvidence
        case .probeUnsupported: .unsupported
        default: nil
        }
    }
    var reconnectsAfterFirstLive: Bool { self == .reconnectingVesc || self == .reconnectingEuc }
    var emitsPendingTelemetry: Bool { self == .pendingVesc }
    var emitsStaleTelemetry: Bool { self == .staleVesc || self == .staleEuc }
    var flushCaptureSucceeds: Bool { self != .unknownDeviceFinishFailure }
    var isEuc: Bool {
        self == .probeDevice
            || self == .probeTimeout
            || self == .probeMalformedResponse
            || self == .probeConflictingEvidence
            || self == .probeUnsupported
            || self == .euc
            || self == .dynamicEuc
            || self == .staleEuc
            || self == .reconnectingEuc
            || self == .connectingEuc
            || self == .eucOverview
            || self == .eucNoBms
            || self == .eucUnknownTopology
    }

    private var testBmsSnapshot: BmsSnapshot? {
        switch self {
        case .euc: eucBmsSnapshot
        case .eucOverview: eucBmsOverviewSnapshot
        case .eucUnknownTopology: eucUnknownTopologyBmsSnapshot
        default: nil
        }
    }

    var testScript: CutoutSessionTestScript {
        CutoutSessionTestScript(
            candidate: candidate,
            telemetry: emitsPendingTelemetry ? nil : telemetry,
            telemetryUpdate: dynamicTelemetryUpdate,
            telemetryUpdateDelayMilliseconds: dynamicTelemetryUpdate == nil ? 0 : 1_500,
            bmsSnapshot: testBmsSnapshot,
            startsLive: startsLive,
            initialBluetoothState: initialBluetoothState,
            failsConnection: failsConnection,
            identificationProbeFailure: identificationProbeFailure,
            emitsLateLiveAfterFailure: failsConnection,
            reconnectsAfterFirstLive: reconnectsAfterFirstLive,
            reconnectAfterLiveMilliseconds: reconnectsAfterFirstLive ? 1_500 : 0,
            reconnectDelayMilliseconds: reconnectsAfterFirstLive ? 5_000 : 0,
            emitsStaleTelemetry: emitsStaleTelemetry,
            flushCaptureSucceeds: flushCaptureSucceeds,
            connectionDelayMilliseconds: startsLive ? 0 : (failsConnection ? 3_000 : connectingDelayMilliseconds)
        )
    }

    private var connectingDelayMilliseconds: UInt64 {
        switch self {
        case .connectingVesc, .connectingEuc: 5_000
        default: 1_000
        }
    }

    private var telemetry: TelemetrySnapshot {
        if isEuc {
            return TelemetrySnapshot(
                speed: Speed(value: 12_000),
                speedSource: .reported,
                speedQuality: .known,
                operatingState: .riding,
                voltage: Voltage(value: 82_000),
                batteryCurrent: BatteryCurrent(value: 8_000),
                controllerTemperature: Temperature(value: 31_000),
                batteryLevelReported: BatteryLevel(value: 64)
            )
        }
        return TelemetrySnapshot(
            speed: Speed(value: 8_000),
            speedSource: .reported,
            speedQuality: .known,
            operatingState: .riding,
            voltage: Voltage(value: 50_400),
            batteryCurrent: BatteryCurrent(value: 12_000),
            controllerTemperature: Temperature(value: 32_000),
            pwm: DutyCycle(permille: 230),
            batteryLevelReported: BatteryLevel(value: 72)
        )
    }

    private var dynamicTelemetryUpdate: TelemetrySnapshot? {
        switch self {
        case .dynamicVesc:
            TelemetrySnapshot(
                speed: Speed(value: 16_000),
                speedSource: .reported,
                speedQuality: .known,
                operatingState: .riding,
                voltage: Voltage(value: 62_000),
                batteryCurrent: BatteryCurrent(value: 12_000),
                motorCurrent: PhaseCurrent(value: 20_000),
                controllerTemperature: Temperature(value: 43_000),
                motorTemperature: Temperature(value: 49_000),
                pwm: DutyCycle(permille: 720),
                batteryLevelReported: BatteryLevel(value: 71)
            )
        case .dynamicEuc:
            TelemetrySnapshot(
                speed: Speed(value: 18_000),
                speedSource: .reported,
                speedQuality: .known,
                operatingState: .riding,
                voltage: Voltage(value: 80_000),
                batteryCurrent: BatteryCurrent(value: 10_000),
                controllerTemperature: Temperature(value: 35_000),
                batteryLevelReported: BatteryLevel(value: 61)
            )
        default:
            nil
        }
    }

    private var eucBmsSnapshot: BmsSnapshot {
        makeEucBmsSnapshot(groups: [
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
        ])
    }

    private var eucBmsOverviewSnapshot: BmsSnapshot {
        makeEucBmsSnapshot(groups: [])
    }

    private func makeEucBmsSnapshot(groups: [BmsGroupSnapshot]) -> BmsSnapshot {
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
            groups: groups
        )
    }

    private var eucUnknownTopologyBmsSnapshot: BmsSnapshot {
        BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "topology unverified",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            voltage: Voltage(value: 82_000),
            faultSummary: "BMS found, map unknown",
            faultDetail: "Awaiting a verified topology.",
            faults: [BmsFault(code: "0x0040", label: "needs decoder", level: .warning)],
            captureActionTitle: "Record unsupported pack",
            captureActionState: "disabled for launch"
        )
    }
}

#endif
