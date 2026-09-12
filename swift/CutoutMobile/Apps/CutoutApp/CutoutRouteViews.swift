import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

struct AppMusicCompactPlayerModifier: ViewModifier {
    let model: CutoutAppModel
    @State private var isMusicSettingsPresented = false

    func body(content: Content) -> some View {
        content.musicCompactPlayer(
            nowPlaying: model.musicNowPlaying,
            timeline: model.musicTimelineEvents,
            isHidden: model.isMusicPlayerHidden,
            onCommand: { command in
                Task { @MainActor in
                    _ = await model.handleMusicCommand(command)
                }
            },
            onOpenSettings: { isMusicSettingsPresented = true },
            onDismiss: model.dismissMusicPlayer,
            onRestore: model.restoreMusicPlayer
        )
        .sheet(isPresented: $isMusicSettingsPresented) {
            AppSetupView(model: model, opensMusic: true)
        }
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
    @State private var isSetupPresented = false

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
            },
            openSetup: { isSetupPresented = true }
        )
        .sheet(isPresented: $isSetupPresented) {
            AppSetupView(model: model)
        }
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

struct EucTuneRouteView: View {
    let model: CutoutAppModel

    var body: some View {
        Form {
            HeadlightControlSection(
                status: model.headlightCommandStatus,
                isAvailable: model.phase == .live,
                submit: { state in
                    _ = model.setHeadlight(state)
                }
            )
        }
        .accessibilityIdentifier("settings.screen.eucTune")
    }
}

private struct HeadlightControlSection: View {
    let status: LightCommandStatus
    let isAvailable: Bool
    let submit: (LightState) -> Void

    var body: some View {
        Section {
            HStack {
                Button(localizedAppText("settings.headlight.turn_on")) {
                    submit(.on)
                }
                Button(localizedAppText("settings.headlight.turn_off")) {
                    submit(.off)
                }
            }
            .buttonStyle(.bordered)
            .disabled(!isAvailable)
            .accessibilityHint(localizedAppText("settings.headlight.help"))

            Text(statusText)
                .foregroundStyle(.secondary)
        } header: {
            Text(localizedAppText("settings.lights.title"))
        } footer: {
            Text(localizedAppText("settings.headlight.help"))
        }
    }

    private var statusText: String {
        switch status {
        case .unknown:
            localizedAppText("settings.headlight.state_unknown")
        case .requested(.off):
            localizedAppText("settings.headlight.last_request_off")
        case .requested(.on):
            localizedAppText("settings.headlight.last_request_on")
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
    private(set) var candidates: [MelkLightingPeripheralCandidate] = []
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
    private var callbackGeneration = 0
    private enum PendingCommandScope: Equatable {
        case none
        case power
        case color
        case brightness
        case effectSpeed
        case schedule
        case completeState
    }
    private var pendingCommandScope: PendingCommandScope = .none

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
        if let requested = persistence.requestedState {
            requestedState = requested
        } else if let confirmed = persistence.confirmedState {
            requestedState = confirmed
        }
        installSessionCallbacks(for: callbackGeneration)
    }

    func start() {
        guard !isRunning else { return }
        isRunning = true
        callbackGeneration &+= 1
        installSessionCallbacks(for: callbackGeneration)
        candidates.removeAll()
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
        callbackGeneration &+= 1
        candidates.removeAll()
        session.stop()
    }

    /// A first-pairing scan is owned by the Lighting route. Remembered accessories are
    /// intentionally kept alive so reconnect and restore can continue outside the route.
    func stopIfUnpaired() {
        guard persistence.platformIdentifier == nil else { return }
        stop()
    }

    func forgetAccessory() {
        stop()
        persistence.forget()
        connectionState = .idle
        peripheralName = nil
        peripheralIdentifier = nil
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
        pendingCommandScope = .none
        candidates.removeAll()
        restoreAttempted = true
    }

    private func installSessionCallbacks(for generation: Int) {
        session.onStateChange = { [weak self] state in
            Task { @MainActor [weak self] in
                guard let self, self.isRunning, self.callbackGeneration == generation else { return }
                self.handleStateChange(state)
            }
        }
        session.onIdentity = { [weak self] identity in
            Task { @MainActor [weak self] in
                guard let self, self.isRunning, self.callbackGeneration == generation else { return }
                self.handleIdentity(identity)
            }
        }
        session.onRecord = { [weak self] record in
            Task { @MainActor [weak self] in
                guard let self, self.isRunning, self.callbackGeneration == generation else { return }
                self.handleRecord(record)
            }
        }
        session.onNotification = { [weak self] data in
            Task { @MainActor [weak self] in
                guard let self, self.isRunning, self.callbackGeneration == generation else { return }
                self.append("notification=FFF4 (\(data.count) bytes)")
            }
        }
        session.onCandidate = { [weak self] candidate in
            Task { @MainActor [weak self] in
                guard let self, self.isRunning, self.callbackGeneration == generation else { return }
                self.candidates.removeAll { $0.id == candidate.id }
                self.candidates.append(candidate)
                self.candidates.sort { $0.rssi > $1.rssi }
            }
        }
    }

    func reconnect() {
        stop()
        start()
    }

    func selectCandidate(_ candidate: MelkLightingPeripheralCandidate) {
        guard isRunning else { return }
        session.selectCandidate(platformIdentifier: candidate.id)
    }

    @discardableResult
    func setPower(_ on: Bool) -> Bool {
        var state = requestedState
        state.powerOn = on
        guard session.setPower(on) else {
            controlError = localizedAppText("lighting.error.power_not_ready")
            return false
        }
        controlError = nil
        requestedState = state
        updatePersistedRequestedState()
        commandStatus = .requested
        pendingCommandScope = .power
        return true
    }

    @discardableResult
    func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool {
        let wasSolid = requestedPlayback == .solid
        var state = requestedState
        state.playback = nil
        state.red = red
        state.green = green
        state.blue = blue
        let sent: Bool
        if !wasSolid {
            sent = (try? session.applyState(state)) == true
        } else {
            sent = session.setSolidColor(red: red, green: green, blue: blue)
        }
        guard sent else {
            controlError = localizedAppText("lighting.error.color_not_ready")
            return false
        }
        controlError = nil
        requestedState = state
        updatePersistedRequestedState()
        commandStatus = .requested
        pendingCommandScope = !wasSolid ? .completeState : .color
        return true
    }

    func previewSolidColor(red: UInt8, green: UInt8, blue: UInt8) {
        let now = ProcessInfo.processInfo.systemUptime
        guard now - lastColorPreviewAt >= 1.0 / 30.0 else { return }
        let wasSolid = requestedPlayback == .solid
        guard sendSolidColor(red: red, green: green, blue: blue) else { return }
        controlError = nil
        requestedState.playback = nil
        lastColorPreviewAt = now
        requestedState.red = red
        requestedState.green = green
        requestedState.blue = blue
        commandStatus = .requested
        pendingCommandScope = wasSolid ? .color : .completeState
    }

    @discardableResult
    func setBrightness(_ percentage: UInt8) -> Bool {
        var state = requestedState
        state.brightness = percentage
        guard (try? session.setBrightness(percentage)) == true else {
            controlError = localizedAppText("lighting.error.brightness_not_ready")
            return false
        }
        controlError = nil
        requestedState = state
        updatePersistedRequestedState()
        commandStatus = .requested
        pendingCommandScope = .brightness
        return true
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

    @discardableResult
    func setEffectSpeed(_ speed: UInt8) -> Bool {
        guard case let .effect(pattern, _) = requestedPlayback else { return false }
        var state = requestedState
        state.playback = .effect(pattern: pattern, speed: speed)
        guard session.setEffectSpeed(speed) else {
            controlError = localizedAppText("lighting.error.effect_speed_failed")
            return false
        }
        controlError = nil
        requestedState = state
        updatePersistedRequestedState()
        commandStatus = .requested
        pendingCommandScope = .effectSpeed
        return true
    }

    private func applyState(_ state: MobileMelkLightingRestoreStateDto) {
        guard (try? session.applyState(state)) == true else {
            controlError = localizedAppText("lighting.error.state_failed")
            return
        }
        controlError = nil
        requestedState = state
        updatePersistedRequestedState()
        commandStatus = .requested
        pendingCommandScope = .completeState
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
            controlError = localizedAppText("lighting.error.schedule_failed")
            return false
        }
        controlError = nil
        commandStatus = .requested
        pendingCommandScope = .schedule
        return true
    }

    func markConfirmed() {
        guard commandStatus == .requested, pendingCommandScope != .none else { return }
        session.markLastCommandConfirmed()
        commandStatus = .confirmed
        switch pendingCommandScope {
        case .none:
            break
        case .power:
            confirmPartialState(field: .power)
        case .color:
            confirmPartialState(field: .color)
        case .brightness:
            confirmPartialState(field: .brightness)
        case .effectSpeed:
            confirmPartialState(field: .effectSpeed)
        case .schedule:
            break
        case .completeState:
            try? persistence.confirm(requestedState)
        }
        pendingCommandScope = .none
        restoreAttempted = true
    }

    private func confirmPartialState(field: MobileRgbLightingPartialStateDto) {
        guard (try? persistence.confirmPartialState(requestedState, field: field)) == true else {
            persistence.markUnconfirmed()
            return
        }
    }

    private func markPendingCommandUnconfirmed() {
        switch pendingCommandScope {
        case .power, .color, .brightness, .effectSpeed, .completeState:
            persistence.markUnconfirmed()
        case .none, .schedule:
            break
        }
    }

    func markUnconfirmed() {
        guard commandStatus == .requested else { return }
        session.markLastCommandUnconfirmed()
        markPendingCommandUnconfirmed()
        commandStatus = .unconfirmed
        pendingCommandScope = .none
    }

    var isReady: Bool { connectionState == .ready }

    /// Candidates are only selectable while the session is scanning. Once CoreBluetooth has a
    /// connection attempt in flight, selecting a stale row cannot change the active peripheral.
    var canSelectCandidate: Bool {
        connectionState == .idle || connectionState == .scanning
    }

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
            controlError = localizedAppText("lighting.error.preset_save")
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
            controlError = localizedAppText("lighting.error.preset_update")
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
            markPendingCommandUnconfirmed()
            pendingCommandScope = .none
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
              persistence.platformIdentifier == peripheralIdentifier else {
            return
        }
        guard let candidate = persistence.restoreCandidate(),
              candidate.platformIdentifier == peripheralIdentifier else {
            if !persistence.isCompatibleWithCurrentProfile {
                restoreAttempted = true
                controlError = localizedAppText("lighting.error.restore_incompatible")
                append("restore=skipped incompatible-profile")
            }
            return
        }
        restoreAttempted = true
        guard (try? session.applyState(candidate.requestedState)) == true else {
            restoreAttempted = false
            return
        }
        requestedState = candidate.requestedState
        commandStatus = .requested
        pendingCommandScope = .completeState
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
