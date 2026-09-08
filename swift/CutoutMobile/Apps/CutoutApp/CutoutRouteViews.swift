import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

struct AppMusicCompactPlayerModifier: ViewModifier {
    let model: CutoutAppModel

    func body(content: Content) -> some View {
        content.musicCompactPlayer(
            nowPlaying: model.musicNowPlaying,
            timeline: model.musicTimelineEvents,
            selectedProvider: model.selectedMusicProvider,
            isHidden: model.isMusicPlayerHidden,
            historyPolicy: model.musicHistoryPolicy,
            historyUnavailable: model.musicHistoryUnavailable,
            onCommand: { command in
                Task { @MainActor in
                    _ = await model.handleMusicCommand(command)
                }
            },
            onConnect: model.connectMusic,
            onDismiss: model.dismissMusicPlayer,
            onRestore: model.restoreMusicPlayer,
            onSelectProvider: model.selectMusicProvider,
            onSetHistoryPolicy: model.setMusicHistoryPolicy
        )
    }
}

extension View {
    func appMusicCompactPlayer(model: CutoutAppModel) -> some View {
        modifier(AppMusicCompactPlayerModifier(model: model))
    }
}

func lightingPresetSaveEligibility(
    platformIdentifier: String?,
    commandStatus: MelkLightingCommandStatus
) -> Bool {
    guard let platformIdentifier, !platformIdentifier.isEmpty else { return false }
    return commandStatus != .idle
}

func shouldAutoStartLightingSession(platformIdentifier: String?) -> Bool {
    guard let platformIdentifier else { return false }
    return UUID(uuidString: platformIdentifier) != nil
}
func lightingColorSelection(
    red: UInt8,
    green: UInt8,
    blue: UInt8
) -> (hue: Double, saturation: Double) {
    let red = Double(red) / 255
    let green = Double(green) / 255
    let blue = Double(blue) / 255
    let maximum = max(red, green, blue)
    let minimum = min(red, green, blue)
    let delta = maximum - minimum
    guard delta > 0, maximum > 0 else {
        return (hue: 0, saturation: 0)
    }

    let hue: Double
    if maximum == red {
        hue = ((green - blue) / delta).truncatingRemainder(dividingBy: 6) / 6
    } else if maximum == green {
        hue = ((blue - red) / delta + 2) / 6
    } else {
        hue = ((red - green) / delta + 4) / 6
    }
    return (hue: hue < 0 ? hue + 1 : hue, saturation: delta / maximum)
}

struct DevicePickerRouteView: View {
    let model: CutoutAppModel
    let pair: (DevicePickerRow) -> Void
    let navigate: (CutoutAppRoute) -> Void

    var body: some View {
        DevicePickerView(
            scanState: model.devicePickerScanState,
            connectionPhase: model.phase,
            captureStatusText: model.captureStatusText,
            hasSavedDevice: model.hasSavedDevice,
            pair: pair,
            forgetSavedDevice: model.forgetSavedDevice,
            probe: { row in model.startProbe(platformIdentifier: row.id) },
            recordOnly: { row, deviceKind in
                guard model.recordOnly(platformIdentifier: row.id, deviceKind: deviceKind) else { return false }
                navigate(model.isRecordOnlyCapture ? .capture : .eucRide)
                return true
            }
        )
        .safeAreaInset(edge: .bottom, spacing: 12) {
            HStack {
                Button {
                    navigate(.lighting(.euc))
                } label: {
                    Label(localizedAppText("navigation.section.lighting"), systemImage: "lightbulb.2")
                        .frame(maxWidth: .infinity, minHeight: 44)
                }
                .accessibilityIdentifier("device-picker.open-lighting")
                Button {
                    navigate(.rideMap)
                } label: {
                    Label(localizedAppText("tab.map"), systemImage: "map")
                        .frame(maxWidth: .infinity, minHeight: 44)
                }
                .accessibilityIdentifier("device-picker.open-map")
            }
            .buttonStyle(.borderedProminent)
            .padding(.horizontal, 24)
            .padding(.bottom, 8)
        }
    }
}

struct EucRideRouteView: View {
    let model: CutoutAppModel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            EucRideScreenView(
                rideState: model.eucRidePresentationState,
                rideTitle: model.selectedRideTitle,
                now: model.currentMonotonicTime,
                captureStatusText: model.captureStatusText,
                connectionStatusText: model.connectionStatusText,
                phoneLocationReadback: model.phoneLocationReadback
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.eucRide")
        }
        .appMusicCompactPlayer(model: model)
    }
}

struct CaptureRouteView: View {
    let model: CutoutAppModel
    let finishCapture: () -> Void

    var body: some View {
        CaptureRecordingScreen(
            deviceKind: model.recordOnlyDeviceKind,
            captureStatusText: model.captureStatusText,
            captureStatusTone: model.captureStatus?.statusStripTone ?? .nominal,
            captureProgress: model.captureProgress,
            activeLabels: model.activeCaptureLabels,
            isFinishing: model.isFinishingCapture,
            finishCapture: finishCapture,
            startCaptureLabel: model.startCaptureLabel,
            stopCaptureLabel: model.stopCaptureLabel
        )
    }
}

struct EucPackRouteView: View {
    let model: CutoutAppModel
    let packScreen: EucPackScreen
    let selectedGroupIndex: Int?
    let navigate: (CutoutAppRoute) -> Void

    private let catalog = PevScreenCatalog.live

    var body: some View {
        if let screen = bmsScreen {
            let rideState = screen.bmsContentOrUnavailable.kind == .noData ? model.rideState : nil
            BmsScreenView(
                screen: screen,
                rideState: rideState,
                bmsSnapshot: model.bmsSnapshot,
                selectedGroupIndex: selectedGroupIndex,
                showGroupDetail: { groupIndex in
                    navigate(.eucPack(.bmsCellDetail(groupIndex)))
                },
                showCellMap: {
                    navigate(.eucPack(.root))
                }
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.\(screen.id.rawValue)")
            .onChange(of: model.bmsSnapshot?.groups.map(\.index), initial: true) { _, groupIndices in
                guard !packScreen.hasAvailableSelectedGroup(in: groupIndices) else { return }
                navigate(.eucPack(.root))
            }
        }
    }

    private var bmsScreen: PevScreen? {
        if let screenID = packScreen.screenID {
            catalog.screen(id: screenID).map {
                catalog.presentedScreen(for: $0, liveBmsSnapshot: model.bmsSnapshot)
            }
        } else {
            catalog.presentedBmsScreen(liveBmsSnapshot: model.bmsSnapshot)
        }
    }
}

struct VescRideRouteView: View {
    let model: CutoutAppModel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            VescRideScreenView(
                liveSnapshot: model.vescRideSnapshot,
                phase: model.phase,
                now: model.currentMonotonicTime,
                captureStatusText: model.captureStatusText,
                connectionStatusText: model.connectionStatusText
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.vescRide")
        }
        .appMusicCompactPlayer(model: model)
    }
}

struct VescDebugRouteView: View {
    let model: CutoutAppModel

    var body: some View {
        VescDebugScreenView(
            snapshot: model.vescRideSnapshot,
            phase: model.phase,
            notificationCount: model.displayState.notificationCount,
            captureStatusText: model.captureStatusText,
            connectionStatusText: model.connectionStatusText
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dashboard.screen.vescDebug")
    }
}

private extension MelkLightingPeripheralState {
    var persistedConnectionState: MobileRgbLightingConnectionStateDto? {
        switch self {
        case .idle:
            nil
        case .disconnected:
            .disconnected
        case .ready:
            .ready
        case .scanning, .connecting, .discovering, .retrying, .failed:
            .unknown
        }
    }

    var invalidatesPendingCommand: Bool {
        switch self {
        case .retrying, .disconnected, .failed:
            true
        case .idle, .scanning, .connecting, .discovering, .ready:
            false
        }
    }
}

@MainActor
@Observable
final class LightingRouteModel {

    private let session: any MelkLightingPeripheralSessionProtocol
    private let persistence: LightingAccessoryPersistence

    private(set) var connectionState: MelkLightingPeripheralState = .idle
    private(set) var peripheralName: String?
    private(set) var peripheralIdentifier: String?
    private(set) var commandStatus: MelkLightingCommandStatus = .idle
    private(set) var restoreEnabled: Bool
    private(set) var accessoryAlias: String?
    private(set) var vehicleIdentifier: String?
    private(set) var presets: [MobileRgbLightingPresetDto] = []
    private(set) var records: [MelkLightingLogEntry] = []
    private(set) var notificationCount = 0
    private(set) var controlError: String?
    private var isRunning = false
    private var requestedState = MobileMelkLightingRestoreStateDto(
        powerOn: false,
        red: 255,
        green: 0,
        blue: 0,
        brightness: 100
    )
    private var restoreAttempted = false
    private var lastColorPreviewAt: TimeInterval = 0

    init(
        session: any MelkLightingPeripheralSessionProtocol = MelkLightingPeripheralSession(),
        persistence: LightingAccessoryPersistence = LightingAccessoryPersistence()
    ) {
        self.session = session
        self.persistence = persistence
        restoreEnabled = persistence.restoreEnabled
        accessoryAlias = persistence.alias
        vehicleIdentifier = persistence.vehicleIdentifier
        refreshPresets()
        if let confirmed = persistence.confirmedState {
            requestedState = confirmed
        } else if let requested = persistence.requestedState {
            requestedState = requested
        }
        session.onStateChange = { [weak self] state in
            Task { @MainActor in
                self?.handleStateChange(state)
            }
        }
        session.onIdentity = { [weak self] identity in
            Task { @MainActor in
                self?.handleIdentity(identity)
            }
        }
        session.onRecord = { [weak self] record in
            Task { @MainActor in
                self?.handleRecord(record)
            }
        }
    }

    func start() {
        guard !isRunning else { return }
        isRunning = true
        session.start(preferredPlatformIdentifier: persistence.platformIdentifier)
    }

    func startIfRemembered() {
        guard shouldAutoStartLightingSession(platformIdentifier: persistence.platformIdentifier) else {
            return
        }
        start()
    }

    func stop() {
        guard isRunning else { return }
        isRunning = false
        session.stop()
    }

    func forgetAccessory() {
        stop()
        persistence.forget()
        accessoryAlias = nil
        vehicleIdentifier = nil
        refreshPresets()
        restoreEnabled = false
        requestedState = MobileMelkLightingRestoreStateDto(
            powerOn: false,
            red: 255,
            green: 0,
            blue: 0,
            brightness: 100
        )
        commandStatus = .idle
        restoreAttempted = true
    }

    func reconnect() {
        stop()
        start()
    }

    func setPower(_ on: Bool) {
        guard session.setPower(on) else { return }
        requestedState.powerOn = on
        updatePersistedRequestedState()
        commandStatus = .requested
    }

    func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) {
        guard sendSolidColor(red: red, green: green, blue: blue) else { return }
        requestedState.playback = nil
        requestedState.red = red
        requestedState.green = green
        requestedState.blue = blue
        updatePersistedRequestedState()
        commandStatus = .requested
    }

    func previewSolidColor(red: UInt8, green: UInt8, blue: UInt8) {
        let now = ProcessInfo.processInfo.systemUptime
        guard now - lastColorPreviewAt >= 1.0 / 30.0 else { return }
        guard sendSolidColor(red: red, green: green, blue: blue) else { return }
        requestedState.playback = nil
        lastColorPreviewAt = now
        requestedState.red = red
        requestedState.green = green
        requestedState.blue = blue
        commandStatus = .requested
    }

    func setBrightness(_ percentage: UInt8) {
        guard (try? session.setBrightness(percentage)) == true else { return }
        requestedState.brightness = percentage
        updatePersistedRequestedState()
        commandStatus = .requested
    }

    var requestedPlayback: MobileLightingPlaybackDto { requestedState.playback ?? .solid }

    private func sendSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool {
        if requestedPlayback == .solid {
            return session.setSolidColor(red: red, green: green, blue: blue)
        }
        var state = requestedState
        state.red = red
        state.green = green
        state.blue = blue
        state.playback = .solid
        return (try? session.applyState(state)) == true
    }

    func setPlayback(_ playback: MobileLightingPlaybackDto) {
        var state = requestedState
        state.playback = playback
        if playback != .solid { state.powerOn = true }
        applyState(state)
    }

    func stopMusic() {
        guard case .music = requestedPlayback else { return }
        var state = requestedState
        state.playback = .solid
        applyState(state)
    }

    func setEffectSpeed(_ speed: UInt8) {
        guard case let .effect(pattern, _) = requestedPlayback else { return }
        guard session.setEffectSpeed(speed) else {
            controlError = "Could not send effect speed. Check the connection and try again."
            return
        }
        controlError = nil
        requestedState.playback = .effect(pattern: pattern, speed: speed)
        updatePersistedRequestedState()
        commandStatus = .requested
    }

    private func applyState(_ state: MobileMelkLightingRestoreStateDto) {
        guard (try? session.applyState(state)) == true else {
            controlError = "Could not send lighting settings. Check the connection and try again."
            return
        }
        controlError = nil
        requestedState = state
        updatePersistedRequestedState()
        commandStatus = .requested
    }

    @discardableResult
    func setSchedule(_ schedule: MobileMelkScheduleDto, now: Date = Date(), calendar: Calendar = .current) -> Bool {
        let parts = calendar.dateComponents([.hour, .minute, .second, .weekday], from: now)
        guard let hour = parts.hour, let minute = parts.minute, let second = parts.second,
              let weekday = parts.weekday else { return false }
        let clock = MobileMelkClockDto(
            hour: UInt8(hour), minute: UInt8(minute), second: UInt8(second),
            weekday: UInt8((weekday + 5) % 7 + 1)
        )
        guard (try? session.setSchedule(schedule, clock: clock)) == true else {
            controlError = "Could not send the timer. Check the connection and try again."
            return false
        }
        controlError = nil
        commandStatus = .requested
        return true
    }

    func markConfirmed() {
        guard commandStatus == .requested else { return }
        session.markLastCommandConfirmed()
        commandStatus = .confirmed
        try? persistence.confirm(requestedState)
        restoreAttempted = true
    }

    func markUnconfirmed() {
        guard commandStatus == .requested else { return }
        session.markLastCommandUnconfirmed()
        persistence.markUnconfirmed()
        commandStatus = .unconfirmed
    }

    var isReady: Bool { connectionState == .ready }

    var requestedPowerOn: Bool { requestedState.powerOn }
    var requestedRed: UInt8 { requestedState.red }
    var requestedGreen: UInt8 { requestedState.green }
    var requestedBlue: UInt8 { requestedState.blue }
    var requestedBrightness: UInt8 { requestedState.brightness }

    var canSavePreset: Bool {
        lightingPresetSaveEligibility(
            platformIdentifier: persistence.platformIdentifier,
            commandStatus: commandStatus
        )
    }

    var canEditMetadata: Bool { persistence.platformIdentifier != nil }

    @discardableResult
    func savePreset(named name: String) -> Bool {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard canSavePreset, !trimmed.isEmpty else { return false }
        do {
            try persistence.addPreset(name: trimmed, requested: requestedState)
            refreshPresets()
            controlError = nil
            return true
        } catch {
            controlError = "Could not save this preset: \(error.localizedDescription)"
            return false
        }
    }

    @discardableResult
    func deletePreset(named name: String) -> Bool {
        guard persistence.removePreset(named: name) else { return false }
        refreshPresets()
        return true
    }

    @discardableResult
    func replacePreset(named name: String) -> Bool {
        guard canSavePreset else { return false }
        do {
            guard try persistence.replacePreset(named: name, requested: requestedState) else {
                return false
            }
            refreshPresets()
            controlError = nil
            return true
        } catch {
            controlError = "Could not update this preset: \(error.localizedDescription)"
            return false
        }
    }

    func saveAccessoryMetadata(alias: String, vehicleIdentifier: String?) {
        guard canEditMetadata else { return }
        let trimmedAlias = alias.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedVehicle = vehicleIdentifier?.trimmingCharacters(in: .whitespacesAndNewlines)
        try? persistence.setAlias(trimmedAlias.isEmpty ? nil : trimmedAlias)
        try? persistence.setVehicleIdentifier(trimmedVehicle?.isEmpty == true ? nil : trimmedVehicle)
        accessoryAlias = persistence.alias
        self.vehicleIdentifier = persistence.vehicleIdentifier
    }

    func applyPreset(_ preset: MobileRgbLightingPresetDto) {
        applyState(preset.requested)
    }

    func setRestoreEnabled(_ enabled: Bool) {
        restoreEnabled = enabled
        persistence.setRestoreEnabled(enabled)
        if enabled {
            restoreIfEligible()
        }
    }

    var canReconnect: Bool {
        switch connectionState {
        case .disconnected, .failed:
            true
        default:
            false
        }
    }

    private func handleIdentity(_ identity: MelkLightingPeripheralIdentity) {
        peripheralName = identity.name
        peripheralIdentifier = identity.platformIdentifier
        guard connectionState == .ready else { return }
        ensureRecordForConnectedAccessory()
        restoreIfEligible()
    }

    private func handleStateChange(_ state: MelkLightingPeripheralState) {
        if state.resetsRestoreEligibility {
            restoreAttempted = false
        }
        if state == .scanning {
            peripheralName = nil
            peripheralIdentifier = nil
        }
        connectionState = state
        if state.invalidatesPendingCommand, commandStatus == .requested {
            commandStatus = .unconfirmed
            persistence.markUnconfirmed()
        }
        if state == .ready {
            ensureRecordForConnectedAccessory()
            restoreIfEligible()
        }
        if let persistedConnectionState = state.persistedConnectionState {
            persistence.setConnection(persistedConnectionState)
        }
    }

    private func handleRecord(_ record: String) {
        append(record)
    }

    private func append(_ record: String) {
        if record.hasPrefix("notification=") {
            notificationCount += 1
            records = Array((records + [MelkLightingLogEntry(text: "FFF4 notification received")]).suffix(12))
        } else if record.hasPrefix("requested=") {
            records = Array((records + [MelkLightingLogEntry(text: "command requested")]).suffix(12))
        } else {
            records = Array((records + [MelkLightingLogEntry(text: record)]).suffix(12))
        }
    }

    private func ensureRecordForConnectedAccessory() {
        guard let identifier = peripheralIdentifier else { return }
        if persistence.ensureRecord(platformIdentifier: identifier) {
            restoreEnabled = persistence.restoreEnabled
        }
        accessoryAlias = persistence.alias
        vehicleIdentifier = persistence.vehicleIdentifier
        refreshPresets()
    }

    private func refreshPresets() {
        presets = persistence.presets
    }

    private func updatePersistedRequestedState() {
        try? persistence.updateRequestedState(requestedState)
    }

    private func restoreIfEligible() {
        guard restoreEnabled, !restoreAttempted,
              let peripheralIdentifier,
              persistence.platformIdentifier == peripheralIdentifier,
              persistence.confirmation == .confirmed,
              let requested = persistence.confirmedState else {
            return
        }
        restoreAttempted = true
        guard (try? session.applyState(requested)) == true else {
            restoreAttempted = false
            return
        }
        requestedState = requested
        commandStatus = .requested
        records = Array((records + [MelkLightingLogEntry(text: "restore=requested")]).suffix(12))
    }
}

struct MelkLightingLogEntry: Identifiable {
    let id = UUID()
    let text: String
}

extension MelkLightingPeripheralState {
    var displayText: String {
        switch self {
        case .idle: localizedAppText("lighting.state.idle")
        case .scanning: localizedAppText("lighting.state.scanning")
        case .connecting: localizedAppText("lighting.state.connecting")
        case let .retrying(attempt, delayMilliseconds):
            localizedAppText("lighting.state.retrying", Int64(attempt), Int64(max(1, Int((delayMilliseconds + 999) / 1000))))
        case .discovering: localizedAppText("lighting.state.discovering")
        case .ready: localizedAppText("lighting.state.ready")
        case .disconnected: localizedAppText("lighting.state.disconnected")
        case let .failed(reason): localizedAppText("lighting.state.failed", reason)
        }
    }

    var symbolName: String {
        switch self {
        case .ready: "checkmark.circle"
        case .failed: "exclamationmark.triangle"
        case .disconnected: "bolt.horizontal.circle"
        default: "antenna.radiowaves.left.and.right"
        }
    }
}

private extension MelkLightingCommandStatus {
    var displayText: String {
        switch self {
        case .idle: localizedAppText("lighting.command.idle")
        case .requested: localizedAppText("lighting.command.requested")
        case .confirmed: localizedAppText("lighting.command.confirmed")
        case .unconfirmed: localizedAppText("lighting.command.unconfirmed")
        }
    }

    var symbolName: String {
        switch self {
        case .idle: "circle"
        case .requested: "clock"
        case .confirmed: "checkmark.circle"
        case .unconfirmed: "questionmark.circle"
        }
    }
}
