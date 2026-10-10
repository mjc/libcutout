import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

#if canImport(UIKit)
    import UIKit
#endif

#if DEBUG
    @MainActor
    private struct RideUITestEnvironmentReadback: ViewModifier {
        @Environment(\.dynamicTypeSize) private var dynamicTypeSize
        @Environment(\.accessibilityReduceMotion) private var reduceMotion
        @Environment(\.colorSchemeContrast) private var contrast
        private let isEnabled =
            CutoutUITestSessionFixture(arguments: ProcessInfo.processInfo.arguments) != nil
            && UserDefaults.standard.bool(forKey: "CUTOUT_UI_TEST_ENVIRONMENT_READBACK")

        func body(content: Content) -> some View {
            if isEnabled {
                content.accessibilityValue(readback)
            } else {
                content
            }
        }

        private var readback: String {
            #if canImport(UIKit)
                "swiftui=\(categoryName);system=\(UIApplication.shared.preferredContentSizeCategory.rawValue);"
                    + "reduceMotion=\(reduceMotion);systemReduceMotion=\(UIAccessibility.isReduceMotionEnabled);"
                    + "contrast=\(contrast == .increased ? "increased" : "standard");"
                    + "systemContrast=\(UIAccessibility.isDarkerSystemColorsEnabled)"
            #else
                "swiftui=\(categoryName)"
            #endif
        }

        private var categoryName: String {
            if dynamicTypeSize == .large { return "large" }
            if dynamicTypeSize == .xxxLarge { return "xxxLarge" }
            if dynamicTypeSize == .accessibility5 { return "accessibility5" }
            return String(describing: dynamicTypeSize)
        }
    }
#endif

extension View {
    @MainActor
    fileprivate func rideUITestEnvironmentReadback() -> some View {
        #if DEBUG
            modifier(RideUITestEnvironmentReadback())
        #else
            self
        #endif
    }
}

struct AppMusicPlayerView: View {
    let model: CutoutAppModel
    let onOpenSettings: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Group {
            if let nowPlaying = model.music.settingsNowPlaying {
                MusicExpandedPlayer(
                    nowPlaying: nowPlaying,
                    timeline: model.music.timelineEvents,
                    onCommand: perform,
                    onOpenSettings: onOpenSettings,
                    onDismissPlayer: model.music.dismissPlayer
                )
            } else {
                NavigationStack {
                    Form {
                        Section(model.music.selectedProvider.title) {
                            Text(pevLocalizedText("music.state.not_connected"))
                                .foregroundStyle(.secondary)
                                .accessibilityIdentifier("music.player.not-connected")
                            if model.music.selectedProvider == .spotify {
                                Button(pevLocalizedText("music.play")) { perform(.play) }
                                    .accessibilityIdentifier("music.play")
                            }
                            Button(pevLocalizedText("music.open_named_provider", model.music.selectedProvider.title)) {
                                perform(.openProvider)
                            }
                            .accessibilityIdentifier("music.open-provider")
                        }
                        Section {
                            Button(pevLocalizedText("music.settings.open"), action: onOpenSettings)
                                .accessibilityIdentifier("music.open-settings")
                        }
                    }
                    .navigationTitle(pevLocalizedText("music.settings.title"))
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button {
                                dismiss()
                            } label: {
                                Text(pevLocalizedText("music.done"))
                                    .frame(minWidth: 44, minHeight: 44)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("music.done")
                        }
                    }
                }
            }
        }
        .accessibilityIdentifier("music.player.screen")
    }

    private func perform(_ command: MobileMusicCommandDto) {
        Task { @MainActor in _ = await model.music.handleCommand(command) }
    }
}

extension View {
    func appMusicCompactPlayer(
        model: CutoutAppModel,
        onOpenDetails: @escaping () -> Void,
        onOpenSettings: @escaping () -> Void
    ) -> some View {
        musicCompactPlayer(
            nowPlaying: model.music.nowPlaying,
            isHidden: model.music.isPlayerHidden,
            onCommand: { command in Task { @MainActor in _ = await model.music.handleCommand(command) } },
            onOpenDetails: onOpenDetails,
            onOpenSettings: onOpenSettings,
            onDismiss: model.music.dismissPlayer
        )
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
    let device: DevicePresentationModel
    let pair: (DevicePickerRow) -> Void
    let navigate: (CutoutAppRoute) -> Void
    let openSetup: () -> Void

    var body: some View {
        DevicePickerView(
            scanState: device.scanState,
            connectionPhase: device.phase,
            pair: pair,
            openSetup: openSetup
        )

    }
}

struct EucRideRouteView: View {
    let model: CutoutAppModel

    private func rideState(at now: MonotonicMilliseconds) -> EucRideScreenState? {
        #if DEBUG && targetEnvironment(simulator)
            if let fixture = CutoutUITestSecondaryRideFixture.eucRideState(at: now) { return fixture }
        #endif
        return model.device.eucRidePresentationState
    }

    private func phoneLocationReadback(at now: MonotonicMilliseconds) -> PhoneLocationReadback {
        #if DEBUG && targetEnvironment(simulator)
            if let fixture = CutoutUITestSecondaryRideFixture.phoneLocationReadback(at: now) { return fixture }
        #endif
        return model.device.phoneLocationReadback
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            EucRideScreenView(
                rideState: rideState(at: model.currentMonotonicTime),
                rideTitle: model.device.selectedRideTitle,
                now: model.currentMonotonicTime,
                captureStatusText: model.capture.status.flatMap { $0.isRecording ? $0.rideDisplayText : nil },
                connectionStatusText: model.device.connectionStatusText,
                phoneLocationReadback: phoneLocationReadback(at: model.currentMonotonicTime)
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.eucRide")
            .rideUITestEnvironmentReadback()
        }
    }
}

struct CaptureRouteView: View {
    let capture: CaptureFeatureModel

    var body: some View {
        if capture.activeGeneration == nil, capture.status != nil,
            let artifact = capture.completed.first(where: { $0.id == capture.latestGeneration })
        {
            CaptureArtifactDetailView(artifact: artifact)
        } else {
            recording
        }
    }

    private var recording: some View {
        CaptureRecordingScreen(
            deviceKind: capture.device?.title ?? capture.deviceKind,
            advertisedName: capture.device?.advertisedName,
            captureStatusText: capture.recordingSummary,
            captureStatusTone: capture.status?.statusStripTone ?? .nominal,
            captureProgress: capture.progress,
            activeLabels: capture.activeLabels,
            annotationErrorText: capture.annotationErrorText,
            dismissAnnotationError: capture.dismissAnnotationError,
            isFinishing: capture.isFinishing,
            canFinish: capture.isManualCapture,
            canAnnotate: capture.canAnnotate,
            finishCapture: finish,
            startCaptureLabel: capture.startLabel,
            stopCaptureLabel: capture.stopLabel
        )
    }

    private func finish() {
        Task { @MainActor in _ = await capture.finish() }
    }
}

struct EucPackRouteView: View {
    let device: DevicePresentationModel
    let packScreen: EucPackScreen
    let selectedGroupIndex: Int?
    let navigate: (CutoutAppRoute) -> Void
    @State private var rootScreenID: PevScreenID?

    private let catalog = PevScreenCatalog.live

    var body: some View {
        if let screen = bmsScreen {
            let rideState = screen.bmsContentOrUnavailable.kind == .noData ? device.rideState : nil
            BmsScreenView(
                screen: screen,
                rideState: rideState,
                bmsSnapshot: device.bmsSnapshot,
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
            .onChange(of: device.bmsSnapshot?.groups.map(\.index), initial: true) { _, groupIndices in
                guard !packScreen.hasAvailableSelectedGroup(in: groupIndices) else { return }
                navigate(.eucPack(.root))
            }
            .onChange(of: device.bmsSnapshot?.availability, initial: true) { _, availability in
                guard packScreen == .root else { return }
                guard availability == .available, rootScreenID == nil,
                    let snapshot = device.bmsSnapshot
                else {
                    if availability == nil || availability == .unavailable || availability == .unsupported {
                        rootScreenID = nil
                    }
                    return
                }
                rootScreenID = catalog.presentedBmsScreen(liveBmsSnapshot: snapshot).id
            }
        }
    }

    private var bmsScreen: PevScreen? {
        if let screenID = packScreen.screenID {
            catalog.screen(id: screenID).map {
                catalog.presentedScreen(for: $0, liveBmsSnapshot: device.bmsSnapshot)
            }
        } else if let rootScreenID,
            let rootScreen = catalog.screen(id: rootScreenID)
        {
            catalog.presentedScreen(for: rootScreen, liveBmsSnapshot: device.bmsSnapshot)
        } else {
            catalog.presentedBmsScreen(liveBmsSnapshot: device.bmsSnapshot)
        }
    }
}

struct EucTuneRouteView: View {
    let model: CutoutAppModel
    let submitSetting: (ConnectionAttemptToken, DeviceSettingID, DeviceSettingValue) throws -> Void
    let submitAction: (ConnectionAttemptToken, DeviceActionID) throws -> Void

    var body: some View {
        Group {
            if let snapshot = model.device.settings {
                DeviceControlsForm(
                    snapshot: snapshot,
                    submitSetting: submitSetting,
                    submitAction: submitAction
                ) { alarmsSection }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text(localizedAppText("controls.tune.title")).font(.largeTitle.bold())
                        ContentUnavailableView(
                            localizedAppText("settings.readback.unavailable"), systemImage: "slider.horizontal.3")
                        alarmsSection
                    }
                    .padding(24)
                }
            }
        }
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
        .tint(PevColors.yellow)
        .accessibilityIdentifier("settings.screen.eucTune")
    }

    private var alarmsSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(localizedAppText("controls.alarms.title"))
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            NavigationLink {
                PhoneRideAlarmSettingsView(model: model)
            } label: {
                HStack {
                    Label(localizedAppText("controls.alarms.title"), systemImage: "bell.badge")
                    Spacer()
                    Image(systemName: "chevron.right")
                        .foregroundStyle(PevColors.muted)
                }
                .padding(16)
                .background(PevDashboardCardBackground(cornerRadius: 22))
            }
            .accessibilityIdentifier("settings.open-alarms")
        }
    }
}

struct VescRideRouteView: View {
    let model: CutoutAppModel

    private func rideSnapshot(at now: MonotonicMilliseconds) -> VescRideSnapshot? {
        #if DEBUG && targetEnvironment(simulator)
            if let fixture = CutoutUITestSecondaryRideFixture.vescRideSnapshot(at: now) { return fixture }
        #endif
        return model.device.vescRideSnapshot
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            VescRideScreenView(
                liveSnapshot: rideSnapshot(at: model.currentMonotonicTime),
                phase: model.device.phase,
                now: model.currentMonotonicTime,
                captureStatusText: model.capture.status.flatMap { $0.isRecording ? $0.rideDisplayText : nil },
                connectionStatusText: model.device.connectionStatusText
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.vescRide")
            .rideUITestEnvironmentReadback()
        }
    }
}

struct VescDebugRouteView: View {
    let device: DevicePresentationModel
    let capture: CaptureFeatureModel

    var body: some View {
        VescDebugScreenView(
            snapshot: device.vescRideSnapshot,
            phase: device.phase,
            notificationCount: device.displayState.notificationCount,
            captureStatusText: capture.status?.displayText,
            connectionStatusText: device.connectionStatusText
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dashboard.screen.vescDebug")
    }
}

extension MelkLightingPeripheralState {
    fileprivate var persistedConnectionState: MobileRgbLightingConnectionStateDto? {
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

    fileprivate var invalidatesPendingCommand: Bool {
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
    private var currentVehicleIdentifier: String?
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
        if !isRunning {
            isRunning = true
            callbackGeneration &+= 1
            installSessionCallbacks(for: callbackGeneration)
            candidates.removeAll()
        }
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
        session.onCandidateRemoved = { [weak self] identifier in
            Task { @MainActor [weak self] in
                guard let self, self.isRunning, self.callbackGeneration == generation else { return }
                self.candidates.removeAll { $0.id == identifier }
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
        guard case .effect(let pattern, _) = requestedPlayback else { return false }
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
            let weekday = parts.weekday
        else { return false }
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
        guard session.markLastCommandConfirmed() else { return }
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
            persistence.markScheduleConfirmed()
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
        case .none:
            break
        case .schedule:
            persistence.markScheduleUnconfirmed()
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

    func saveAccessoryName(_ alias: String) {
        guard canEditMetadata else { return }
        let trimmed = alias.trimmingCharacters(in: .whitespacesAndNewlines)
        try? persistence.setAlias(trimmed.isEmpty ? nil : trimmed)
        accessoryAlias = persistence.alias
    }

    func selectVehicle(_ identifier: String?) {
        guard currentVehicleIdentifier != identifier else { return }
        let wasRunning = isRunning
        stop()
        currentVehicleIdentifier = identifier
        persistence.selectVehicle(identifier)
        connectionState = .idle
        peripheralName = nil
        peripheralIdentifier = nil
        accessoryAlias = persistence.alias
        vehicleIdentifier = persistence.vehicleIdentifier
        restoreEnabled = persistence.restoreEnabled
        restoreAttempted = false
        commandStatus = .idle
        pendingCommandScope = .none
        controlError = nil
        requestedState =
            persistence.requestedState ?? persistence.confirmedState
            ?? MobileMelkLightingRestoreStateDto(powerOn: false, red: 255, green: 0, blue: 0, brightness: 100)
        refreshPresets()
        if wasRunning || persistence.platformIdentifier != nil { start() }
    }

    func applyPreset(_ preset: MobileRgbLightingPresetDto) {
        applyState(preset.requested)
    }

    func setRestoreEnabled(_ enabled: Bool) {
        guard canEditMetadata else { return }
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
            persistence.platformIdentifier == peripheralIdentifier
        else {
            return
        }
        guard let candidate = persistence.restoreCandidate(),
            candidate.platformIdentifier == peripheralIdentifier
        else {
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
        case .retrying(_, let delayMilliseconds):
            localizedAppText(
                "lighting.state.retrying", Int64(max(1, Int((delayMilliseconds + 999) / 1000))))
        case .discovering: localizedAppText("lighting.state.discovering")
        case .ready: localizedAppText("lighting.state.ready")
        case .disconnected: localizedAppText("lighting.state.disconnected")
        case .failed(let reason): localizedAppText("lighting.state.failed", reason)
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

#if DEBUG && targetEnvironment(simulator)
    // Supplies only native transport facts. Rust verifies the profile, admits
    // commands, and produces the exact write captured by this Simulator fixture.
    @Observable
    private final class CutoutUITestLightingSession: MelkLightingPeripheralSessionProtocol {
        static let identifier = "11111111-1111-1111-1111-111111111111"
        private let core = MobileMelkLightingSessionCore()
        private(set) var brightnessWriteCount = 0
        private(set) var writtenBrightness: UInt8?
        private(set) var lastBrightnessWrite: MobileMelkLightingWriteDto?
        var onIdentity: ((MelkLightingPeripheralIdentity) -> Void)?
        var onStateChange: ((MelkLightingPeripheralState) -> Void)?
        var onNotification: ((Data) -> Void)?
        var onRecord: ((String) -> Void)?
        var onCandidate: ((MelkLightingPeripheralCandidate) -> Void)?
        var onCandidateRemoved: ((String) -> Void)?

        var commandStatus: MelkLightingCommandStatus {
            switch core.snapshot().commandStatus {
            case 0: .idle
            case 1: .requested
            case 2: .confirmed
            default: .unconfirmed
            }
        }

        private static func channel(_ short: UInt64) -> MobileBluetoothUuid {
            MobileBluetoothUuid(
                mostSignificantBits: (short << 32) | 0x0000_1000,
                leastSignificantBits: 0x8000_0080_5f9b_34fb
            )
        }

        func start(preferredPlatformIdentifier: String?) {
            if case .ready = core.snapshot().state {
                publishReadyIfVerified()
                return
            }
            let name = "MELK-OC21  6A"
            core.start(preferredPlatformIdentifier: Self.identifier)
            core.handle(event: .bluetoothState(poweredOn: true, stateCode: 5))
            core.handle(event: .restoreUnavailable)
            core.handle(event: .discovered(name: name, platformIdentifier: Self.identifier, rssi: -40))
            _ = core.drainActions()
            core.handle(event: .connected(name: name, platformIdentifier: Self.identifier))
            _ = core.drainActions()
            core.handle(event: .servicesDiscovered(serviceUuids: [Self.channel(0xfff0)], error: nil))
            _ = core.drainActions()
            core.handle(
                event: .characteristicsDiscovered(
                    name: name, serviceUuid: Self.channel(0xfff0),
                    characteristics: [
                        MobileMelkLightingCharacteristicEvidenceDto(
                            uuid: Self.channel(0xfff3), writeWithoutResponse: true, notifyOrIndicate: false),
                        MobileMelkLightingCharacteristicEvidenceDto(
                            uuid: Self.channel(0xfff4), writeWithoutResponse: false, notifyOrIndicate: true),
                    ], error: nil))
            _ = core.drainActions()
            core.handle(
                event: .notificationState(
                    characteristic: Self.channel(0xfff4), ready: true, canSend: true, error: nil))
            _ = core.drainActions()
            for _ in 0..<2 {
                core.handle(event: .timerFired(timer: .initialization, canSend: true))
                _ = core.drainActions()
            }
            publishReadyIfVerified()
        }

        private func publishReadyIfVerified() {
            guard case .ready = core.snapshot().state else {
                onStateChange?(.failed("Simulator Lighting profile verification failed"))
                return
            }
            onIdentity?(
                MelkLightingPeripheralIdentity(
                    name: "MELK-OC21  6A", platformIdentifier: Self.identifier, rssi: -40))
            onStateChange?(.ready)
        }

        func stop() {
            core.stop()
            onStateChange?(.disconnected)
        }
        func selectCandidate(platformIdentifier: String) {
            core.selectCandidate(platformIdentifier: platformIdentifier)
        }
        func setPower(_ on: Bool) -> Bool { core.setPower(on: on) }
        func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool {
            core.setSolidColor(red: red, green: green, blue: blue)
        }
        func setBrightness(_ percentage: UInt8) throws -> Bool {
            guard try core.setBrightness(percentage: percentage) else { return false }
            core.flushWrites(canSend: true)
            for action in core.drainActions() {
                guard case let .write(identifier, write) = action, identifier == Self.identifier else { continue }
                writtenBrightness = percentage
                lastBrightnessWrite = write
                brightnessWriteCount += 1
            }
            return true
        }
        func setEffectSpeed(_ speed: UInt8) -> Bool { core.setEffectSpeed(speed: speed) }
        func applyState(_ state: MobileMelkLightingRestoreStateDto) throws -> Bool { try core.applyState(state: state) }
        func setSchedule(_ schedule: MobileMelkScheduleDto, clock: MobileMelkClockDto) throws -> Bool {
            try core.setSchedule(schedule: schedule, clock: clock)
        }
        func markLastCommandConfirmed() -> Bool { false }
        func markLastCommandUnconfirmed() { core.markLastCommandUnconfirmed() }
    }

    extension LightingRouteModel {
        static func uiTestConnectedLightingModel() -> LightingRouteModel? {
            guard CommandLine.arguments.contains("--ui-test-connected-lighting") else { return nil }
            guard let defaults = UserDefaults(suiteName: "io.cutout.ui-test.connected-lighting") else { return nil }
            defaults.removePersistentDomain(forName: "io.cutout.ui-test.connected-lighting")
            return LightingRouteModel(
                session: CutoutUITestLightingSession(), persistence: LightingAccessoryPersistence(defaults: defaults))
        }

        var uiTestBrightnessWriteReceipt: String? {
            guard let session = session as? CutoutUITestLightingSession else { return nil }
            let brightness = session.writtenBrightness.map(String.init) ?? "none"
            let payload = session.lastBrightnessWrite?.payload.map { String(format: "%02x", $0) }.joined() ?? "none"
            let channel = session.lastBrightnessWrite?.characteristic.mostSignificantBits ?? 0
            let mode = session.lastBrightnessWrite?.mode == .withoutResponse ? "without-response" : "none"
            return "requested=\(requestedBrightness);written=\(brightness);writes=\(session.brightnessWriteCount);"
                + "payload=\(payload);channel=\(channel);mode=\(mode)"
        }
    }
#endif
