import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

func lightingPresetSaveEligibility(
    platformIdentifier: String?,
    commandStatus: MelkLightingCommandStatus
) -> Bool {
    guard let platformIdentifier, !platformIdentifier.isEmpty else { return false }
    return commandStatus == .confirmed
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
    private(set) var records: [MelkLightingLogEntry] = []
    private(set) var notificationCount = 0
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
        if let requested = persistence.requestedState {
            requestedState = requested
        }
        session.onStateChange = { [weak self] state in
            Task { @MainActor in
                if state.resetsRestoreEligibility {
                    self?.restoreAttempted = false
                }
                if state == .scanning {
                    self?.peripheralName = nil
                    self?.peripheralIdentifier = nil
                }
                self?.connectionState = state
                if state == .ready {
                    self?.ensureRecordForConnectedAccessory()
                    self?.persistence.setConnection(.ready)
                    self?.restoreIfEligible()
                } else if state != .idle {
                    self?.persistence.setConnection(.disconnected)
                }
            }
        }
        session.onRecord = { [weak self] record in
            Task { @MainActor in
                self?.append(record)
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
        guard session.setSolidColor(red: red, green: green, blue: blue) else { return }
        requestedState.red = red
        requestedState.green = green
        requestedState.blue = blue
        updatePersistedRequestedState()
        commandStatus = .requested
    }

    func previewSolidColor(red: UInt8, green: UInt8, blue: UInt8) {
        let now = ProcessInfo.processInfo.systemUptime
        guard now - lastColorPreviewAt >= 1.0 / 30.0 else { return }
        guard session.setSolidColor(red: red, green: green, blue: blue) else { return }
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

    var presets: [MobileRgbLightingPresetDto] { persistence.presets }

    var canSavePreset: Bool {
        lightingPresetSaveEligibility(
            platformIdentifier: persistence.platformIdentifier,
            commandStatus: commandStatus
        )
    }

    var canEditMetadata: Bool { persistence.platformIdentifier != nil }

    func savePreset(named name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard canSavePreset, !trimmed.isEmpty else { return }
        try? persistence.addPreset(name: trimmed, requested: requestedState)
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
        setPower(preset.requested.powerOn)
        setSolidColor(
            red: preset.requested.red,
            green: preset.requested.green,
            blue: preset.requested.blue
        )
        setBrightness(preset.requested.brightness)
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

    private func append(_ record: String) {
        if record.hasPrefix("candidate=") || record.hasPrefix("restore=melk") {
            updateIdentity(from: record)
            if record.hasPrefix("candidate=") {
                let candidate = record.dropFirst("candidate=".count)
                peripheralName = String(candidate.split(separator: " id=", maxSplits: 1).first ?? candidate)
            }
            records = Array((records + [MelkLightingLogEntry(text: record)]).suffix(12))
        } else if record.hasPrefix("notification=") {
            notificationCount += 1
            records = Array((records + [MelkLightingLogEntry(text: "FFF4 notification received")]).suffix(12))
        } else if record.hasPrefix("requested=") {
            records = Array((records + [MelkLightingLogEntry(text: "command requested")]).suffix(12))
        } else {
            records = Array((records + [MelkLightingLogEntry(text: record)]).suffix(12))
        }
    }

    private func updateIdentity(from record: String) {
        guard let identifier = record.split(separator: "id=", maxSplits: 1).last?
            .split(separator: " rssi=", maxSplits: 1).first else {
            return
        }
        peripheralIdentifier = String(identifier)
    }

    private func ensureRecordForConnectedAccessory() {
        guard let identifier = peripheralIdentifier else { return }
        if persistence.ensureRecord(platformIdentifier: identifier) {
            restoreEnabled = persistence.restoreEnabled
        }
        accessoryAlias = persistence.alias
        vehicleIdentifier = persistence.vehicleIdentifier
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
        guard session.setPower(requested.powerOn) else {
            restoreAttempted = false
            return
        }
        guard session.setSolidColor(red: requested.red, green: requested.green, blue: requested.blue) else {
            restoreAttempted = false
            return
        }
        guard (try? session.setBrightness(requested.brightness)) == true else {
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

struct LightingRouteView: View {
    let model: LightingRouteModel
    let rideModel: CutoutAppModel
    @State private var brightness = 100.0
    @State private var hue = 0.0
    @State private var saturation = 1.0
    @State private var presetName = ""
    @State private var showsPairing = false
    @FocusState private var isPresetNameFocused: Bool

    private var controlsEnabled: Bool { model.isReady }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Lighting")
                    .font(.largeTitle.weight(.bold))
                    .foregroundStyle(PevColors.primaryText)
                    .accessibilityHeading(.h1)

                connectionCard
                controlsCard
                brightnessCard
                presetsCard
                commandStatusCard
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
        }
        .background(PevColors.pageBackground.ignoresSafeArea())
        .simultaneousGesture(TapGesture().onEnded {
            isPresetNameFocused = false
        })
        .task {
            model.start()
            brightness = Double(model.requestedBrightness)
            updateColorSelection()
        }
        .sheet(isPresented: $showsPairing) {
            LightingPairingSheet(model: model, rideModel: rideModel)
        }
        .accessibilityIdentifier("dashboard.screen.lighting")
    }

    private var connectionCard: some View {
        lightingCard {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "lightbulb.led.fill")
                    .font(.title2)
                    .foregroundStyle(PevColors.cyan)
                    .frame(width: 34, height: 34)
                    .background(PevColors.cyan.opacity(0.14), in: Circle())
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.accessoryAlias ?? model.peripheralName ?? "MELK-OC21 6A")
                        .font(.headline)
                    Text(connectionSummary)
                        .font(.subheadline)
                        .foregroundStyle(connectionStatusColor)
                }
                Spacer(minLength: 8)
                connectionPill
                Button {
                    showsPairing = true
                } label: {
                    Image(systemName: "info.circle")
                        .font(.title3)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Lighting accessory details")
                .accessibilityIdentifier("lighting.accessory-details")
            }

            Text(model.isReady
                ? "Verified MELK-OC21 profile · FFF0 service · FFF3 write · FFF4 notify"
                : "MELK-OC21 profile pending live verification · FFF0 service · FFF3 write · FFF4 notify")
                .font(.caption.monospaced())
                .foregroundStyle(PevColors.muted)
                .accessibilityIdentifier("lighting.profile-evidence")
            Label(
                "Ride stays \(rideModel.connectionStatusText); lighting uses an independent Bluetooth connection.",
                systemImage: "figure.roll"
            )
            .font(.footnote)
            .foregroundStyle(PevColors.muted)
        }
    }

    private var connectionSummary: String {
        switch model.connectionState {
        case .ready:
            model.commandStatus == .confirmed ? "Connected · confirmed" : "Connected · awaiting confirmation"
        case .scanning:
            "Scanning for nearby accessories…"
        case .connecting, .discovering:
            "Connecting…"
        case let .retrying(attempt, delayMilliseconds):
            "Retrying (\(attempt)) in \(max(1, Int((delayMilliseconds + 999) / 1000)))s…"
        case .disconnected:
            "Not connected"
        case .failed:
            "Connection or verification failed"
        case .idle:
            "Ready to scan"
        }
    }

    private var controlsCard: some View {
        lightingCard {
            HStack {
                Text("Power")
                    .font(.headline)
                Spacer()
                Toggle(
                    "Power",
                    isOn: Binding(
                        get: { model.requestedPowerOn },
                        set: { model.setPower($0) }
                    )
                )
                .labelsHidden()
                .tint(PevColors.cyan)
                .accessibilityIdentifier("lighting.power")
            }
            .accessibilityElement(children: .contain)

            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Solid color")
                        .font(.headline)
                    Spacer()
                    Button {
                        model.setSolidColor(red: 255, green: 255, blue: 255)
                        hue = 0
                        saturation = 0
                    } label: {
                        Image(systemName: "eyedropper")
                    }
                    .buttonStyle(.bordered)
                    .accessibilityLabel("Set white")
                    .accessibilityIdentifier("lighting.color-picker.reset")
                }
                LightingColorWheel(hue: $hue, saturation: $saturation) { red, green, blue, isFinal in
                    guard controlsEnabled else { return }
                    if isFinal {
                        model.setSolidColor(red: red, green: green, blue: blue)
                    } else {
                        model.previewSolidColor(red: red, green: green, blue: blue)
                    }
                }
                .frame(maxWidth: .infinity)
                .opacity(controlsEnabled ? 1 : 0.45)
                .accessibilityIdentifier("lighting.color-wheel")
            }
        }
    }

    private var brightnessCard: some View {
        lightingCard {
            HStack {
                Text("Brightness")
                    .font(.headline)
                Spacer()
                Text("\(Int(brightness))%")
                    .monospacedDigit()
                    .foregroundStyle(PevColors.muted)
            }
            HStack(spacing: 10) {
                Image(systemName: "sun.min")
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHidden(true)
                Slider(value: $brightness, in: 1...100, step: 1) { editing in
                    if !editing, controlsEnabled {
                        model.setBrightness(UInt8(brightness.rounded()))
                    }
                }
                .disabled(!controlsEnabled)
                .tint(PevColors.primaryText)
                .accessibilityIdentifier("lighting.brightness")
                .accessibilityValue("\(Int(brightness)) percent")
                Image(systemName: "sun.max")
                    .foregroundStyle(PevColors.muted)
                    .accessibilityHidden(true)
            }
        }
    }

    private var commandStatusCard: some View {
        lightingCard {
            HStack(spacing: 12) {
                Label("Command status", systemImage: model.commandStatus.symbolName)
                    .font(.headline)
                Spacer()
                Text(model.commandStatus.displayText)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(commandStatusColor)
            }
            .accessibilityIdentifier("lighting.command-status")
            Text("A write stays requested until the controller is explicitly observed. No guessed success is shown.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
            HStack(spacing: 10) {
                Button("Mark confirmed") { model.markConfirmed() }
                    .accessibilityIdentifier("lighting.mark-confirmed")
                Button("Mark unconfirmed") { model.markUnconfirmed() }
                    .accessibilityIdentifier("lighting.mark-unconfirmed")
            }
            .buttonStyle(.bordered)
            .disabled(model.commandStatus != .requested)
        }
    }

    private var presetsCard: some View {
        lightingCard {
            Label("Presets", systemImage: "square.stack.3d.up.fill")
                .font(.headline)
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 12) {
                    quickColorPreset("Red", color: .red, red: 255, green: 0, blue: 0)
                    quickColorPreset("Blue", color: .blue, red: 0, green: 0, blue: 255)
                    quickColorPreset("Night", color: .black, red: 16, green: 20, blue: 32)
                }

                if model.presets.isEmpty {
                    Text("Save a confirmed solid color and brightness as a named preset.")
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)
                } else {
                    ForEach(model.presets, id: \.name) { preset in
                        Button {
                            brightness = Double(preset.requested.brightness)
                            model.applyPreset(preset)
                            updateColorSelection()
                        } label: {
                            HStack {
                                Text(preset.name)
                                Spacer()
                                Text("\(preset.requested.brightness)%")
                                    .monospacedDigit()
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .buttonStyle(.bordered)
                        .disabled(!controlsEnabled)
                        .accessibilityIdentifier("lighting.preset.\(preset.name)")
                    }
                }

                HStack {
                    TextField("Preset name", text: $presetName)
                        .textFieldStyle(.roundedBorder)
                        .focused($isPresetNameFocused)
                        .submitLabel(.done)
                        .onSubmit { isPresetNameFocused = false }
                    Button("Save") {
                        model.savePreset(named: presetName)
                        presetName = ""
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(!model.canSavePreset || presetName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityIdentifier("lighting.preset.save")
                }
            }
        }
    }

    private func quickColorPreset(
        _ title: String,
        color: Color,
        red: UInt8,
        green: UInt8,
        blue: UInt8
    ) -> some View {
        Button {
            model.setSolidColor(red: red, green: green, blue: blue)
            updateColorSelection()
        } label: {
            VStack(spacing: 6) {
                Circle()
                    .fill(color)
                    .frame(width: 42, height: 42)
                    .overlay(Circle().stroke(PevColors.cardStroke, lineWidth: 1))
                Text(title)
                    .font(.caption)
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.plain)
        .disabled(!controlsEnabled)
        .accessibilityLabel("Preset \(title)")
        .accessibilityIdentifier("lighting.quick-preset.\(title.lowercased())")
    }

    private var connectionPill: some View {
        Label(model.connectionState.displayText, systemImage: model.connectionState.symbolName)
            .font(.footnote.weight(.semibold))
            .foregroundStyle(connectionStatusColor)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(connectionStatusColor.opacity(0.14), in: Capsule())
            .accessibilityIdentifier("lighting.connection-state")
    }

    private var connectionStatusColor: Color {
        switch model.connectionState {
        case .ready:
            PevColors.green
        case .failed:
            PevColors.red
        default:
            PevColors.yellow
        }
    }

    private var commandStatusColor: Color {
        switch model.commandStatus {
        case .confirmed:
            PevColors.green
        case .unconfirmed:
            PevColors.orange
        case .requested:
            PevColors.yellow
        case .idle:
            PevColors.muted
        }
    }

    private func lightingCard<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12, content: content)
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 20))
    }

    private func updateColorSelection() {
        let selection = lightingColorSelection(
            red: model.requestedRed,
            green: model.requestedGreen,
            blue: model.requestedBlue
        )
        hue = selection.hue
        saturation = selection.saturation
    }
}

private struct LightingColorWheel: View {
    @Binding var hue: Double
    @Binding var saturation: Double
    let onUpdate: (UInt8, UInt8, UInt8, Bool) -> Void

    var body: some View {
        GeometryReader { proxy in
            let size = min(proxy.size.width, proxy.size.height)
            let radius = size / 2
            let pointerRadius = max(0, radius - 14) * saturation
            let pointerAngle = hue * 2 * .pi
            let pointerX = radius + cos(pointerAngle) * pointerRadius
            let pointerY = radius + sin(pointerAngle) * pointerRadius

            ZStack {
                Circle()
                    .fill(AngularGradient(
                        gradient: Gradient(colors: [
                            .red, .yellow, .green, .cyan, .blue, .purple, .red,
                        ]),
                        center: .center
                    ))
                Circle()
                    .fill(RadialGradient(
                        colors: [.white, .white.opacity(0)],
                        center: .center,
                        startRadius: 0,
                        endRadius: radius
                    ))
                Circle()
                    .stroke(PevColors.cardStroke, lineWidth: 1)
                Circle()
                    .fill(Color(hue: hue, saturation: saturation, brightness: 1))
                    .frame(width: size * 0.44, height: size * 0.44)
                    .overlay(Circle().stroke(.white.opacity(0.35), lineWidth: 1))
                Circle()
                    .fill(.white)
                    .frame(width: 28, height: 28)
                    .overlay(Circle().stroke(.black.opacity(0.5), lineWidth: 2))
                    .position(x: pointerX, y: pointerY)
            }
            .frame(width: size, height: size)
            .contentShape(Circle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { value in
                update(at: value.location, in: size, isFinal: false)
            }.onEnded { value in
                update(at: value.location, in: size, isFinal: true)
            })
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Solid color")
            .accessibilityValue("Hue \(Int(hue * 360)) degrees, saturation \(Int(saturation * 100)) percent")
            .accessibilityHint("Drag around the color wheel to choose a color")
            .accessibilityIdentifier("lighting.color-wheel.control")
        }
        .aspectRatio(1, contentMode: .fit)
        .frame(maxWidth: 300)
        .frame(maxWidth: .infinity)
    }

    private func update(at location: CGPoint, in size: CGFloat, isFinal: Bool) {
        let center = CGPoint(x: size / 2, y: size / 2)
        let dx = location.x - center.x
        let dy = location.y - center.y
        let radius = max(1, size / 2 - 14)
        let distance = min(radius, hypot(dx, dy))
        saturation = max(0, min(1, distance / radius))
        var angle = atan2(dy, dx) / (2 * .pi)
        if angle < 0 { angle += 1 }
        hue = angle
        let rgb = Self.rgb(hue: hue, saturation: saturation)
        onUpdate(rgb.red, rgb.green, rgb.blue, isFinal)
    }

    private static func rgb(hue: Double, saturation: Double) -> (red: UInt8, green: UInt8, blue: UInt8) {
        let scaled = hue * 6
        let sector = Int(scaled.rounded(.down)) % 6
        let fraction = scaled - floor(scaled)
        let value = 1.0
        let p = value * (1 - saturation)
        let q = value * (1 - fraction * saturation)
        let t = value * (1 - (1 - fraction) * saturation)
        let channels: (Double, Double, Double) = switch sector {
        case 0: (value, t, p)
        case 1: (q, value, p)
        case 2: (p, value, t)
        case 3: (p, q, value)
        case 4: (t, p, value)
        default: (value, p, q)
        }
        return (
            UInt8((channels.0 * 255).rounded()),
            UInt8((channels.1 * 255).rounded()),
            UInt8((channels.2 * 255).rounded())
        )
    }
}

private struct LightingPairingSheet: View {
    let model: LightingRouteModel
    let rideModel: CutoutAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var accessoryAlias = ""
    @State private var vehicleIdentifier = ""
    @State private var showsForgetConfirmation = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    pairingStatusCard
                    profileEvidenceCard
                    Button {
                        if model.canReconnect { model.reconnect() } else { model.start() }
                    } label: {
                        Label(model.isReady ? "Connected" : "Connect", systemImage: "link")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.isReady)
                    .accessibilityIdentifier("lighting.pairing.connect")

                    Toggle(
                        "Restore last confirmed state",
                        isOn: Binding(
                            get: { model.restoreEnabled },
                            set: { model.setRestoreEnabled($0) }
                        )
                    )
                    .tint(PevColors.cyan)
                    .accessibilityIdentifier("lighting.restore-toggle")
                    Text("Re-apply the last confirmed state only after the same accessory is verified again.")
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)

                    metadataCard
                    statusGuideCard
                    warningCard

                    if model.canEditMetadata {
                        Button("Forget accessory", role: .destructive) {
                            showsForgetConfirmation = true
                        }
                        .buttonStyle(.bordered)
                        .frame(maxWidth: .infinity)
                        .accessibilityIdentifier("lighting.forget-accessory")
                    }
                }
                .padding(20)
            }
            .background(PevColors.pageBackground.ignoresSafeArea())
            .navigationTitle("Add lighting")
#if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
#endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Back") { dismiss() }
                }
            }
        }
        .task {
            accessoryAlias = model.accessoryAlias ?? ""
            vehicleIdentifier = model.vehicleIdentifier ?? ""
        }
        .confirmationDialog(
            "Forget this RGB accessory?",
            isPresented: $showsForgetConfirmation,
            titleVisibility: .visible
        ) {
            Button("Forget accessory", role: .destructive) {
                model.forgetAccessory()
                dismiss()
            }
        } message: {
            Text("Its alias, vehicle association, presets, and automatic restore preference will be removed.")
        }
    }

    private var pairingStatusCard: some View {
        lightingCard {
            HStack(spacing: 12) {
                Image(systemName: "lightbulb.led.fill")
                    .foregroundStyle(PevColors.cyan)
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.peripheralName ?? "Scanning")
                        .font(.headline)
                    Text(model.connectionState == .scanning ? "Looking for nearby accessories…" : model.connectionState.displayText)
                        .font(.subheadline)
                        .foregroundStyle(PevColors.muted)
                }
                Spacer()
                connectionPill
            }
            Text("Signal and name are candidate evidence; the live profile is verified after connection.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var profileEvidenceCard: some View {
        lightingCard {
            HStack {
                Label("Verified profile evidence", systemImage: "checkmark.shield")
                    .font(.headline)
                Spacer()
                Text(model.isReady ? "Ready" : "Pending")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(model.isReady ? PevColors.green : PevColors.yellow)
            }
            ForEach(["FFF0 service", "FFF3 write", "FFF4 notify"], id: \.self) { evidence in
                HStack {
                    Image(systemName: model.isReady ? "checkmark.circle.fill" : "circle")
                        .foregroundStyle(model.isReady ? PevColors.cyan : PevColors.muted)
                    Text(evidence)
                    Spacer()
                    Text(model.isReady ? "Verified" : "Not yet")
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                }
                .accessibilityElement(children: .combine)
            }
            Text("Profile evidence confirms structure only. Command confirmation is checked after a write.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var metadataCard: some View {
        lightingCard {
            Text("Alias (optional)")
                .font(.headline)
            TextField("Help identify this accessory", text: $accessoryAlias)
                .textFieldStyle(.roundedBorder)
                .accessibilityIdentifier("lighting.accessory-alias")

            HStack(spacing: 10) {
                TextField("Installed vehicle identifier", text: $vehicleIdentifier)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("lighting.vehicle-association")
                if let selectedRideIdentifier = rideModel.selectedRideIdentifier {
                    Button("Use current ride") {
                        vehicleIdentifier = selectedRideIdentifier
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("lighting.use-current-ride")
                }
            }

            Button("Save details") {
                model.saveAccessoryMetadata(alias: accessoryAlias, vehicleIdentifier: vehicleIdentifier)
            }
            .buttonStyle(.bordered)
            .frame(maxWidth: .infinity)
            .disabled(!model.canEditMetadata)
            .accessibilityIdentifier("lighting.save-accessory-details")
        }
    }

    private var statusGuideCard: some View {
        lightingCard {
            Text("Status guide")
                .font(.headline)
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                statusGuide("Ready", color: PevColors.cyan, detail: "Profile verified")
                statusGuide("Pending confirmation", color: PevColors.yellow, detail: "Checking live response")
                statusGuide("Unconfirmed", color: PevColors.orange, detail: "Write not proven")
                statusGuide("Error", color: PevColors.red, detail: "Connection failed")
            }
        }
    }

    private var warningCard: some View {
        lightingCard {
            Label("Competing client", systemImage: "exclamationmark.triangle")
                .foregroundStyle(PevColors.yellow)
            Text("If LotusLamp X or another app is connected, commands may remain unconfirmed until it releases the controller.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
    }

    private func statusGuide(_ title: String, color: Color, detail: String) -> some View {
        VStack(spacing: 4) {
            Image(systemName: "circle")
                .foregroundStyle(color)
            Text(title)
                .font(.caption.weight(.semibold))
                .multilineTextAlignment(.center)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(PevColors.muted)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .combine)
    }

    private var connectionPill: some View {
        Label(model.connectionState.displayText, systemImage: model.connectionState.symbolName)
            .font(.caption.weight(.semibold))
            .foregroundStyle(model.connectionState == .ready ? PevColors.green : PevColors.yellow)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background((model.connectionState == .ready ? PevColors.green : PevColors.yellow).opacity(0.14), in: Capsule())
    }

    private func lightingCard<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12, content: content)
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 18))
    }
}



private extension MelkLightingPeripheralState {
    var displayText: String {
        switch self {
        case .idle: "Idle"
        case .scanning: "Scanning for MELK-OC21"
        case .connecting: "Connecting"
        case let .retrying(attempt, delayMilliseconds):
            "Retrying (\(attempt)) in \(max(1, Int((delayMilliseconds + 999) / 1000)))s"
        case .discovering: "Discovering FFF0/FFF3/FFF4"
        case .ready: "Ready"
        case .disconnected: "Disconnected"
        case let .failed(reason): "Failed: \(reason)"
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
        case .idle: "Idle"
        case .requested: "Requested; awaiting confirmation"
        case .confirmed: "Confirmed by external observation"
        case .unconfirmed: "Unconfirmed"
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
