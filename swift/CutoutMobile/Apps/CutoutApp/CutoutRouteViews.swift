import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

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
    private struct SolidState {
        var powerOn: Bool
        var red: UInt8
        var green: UInt8
        var blue: UInt8
        var brightness: UInt8

        init(powerOn: Bool, red: UInt8, green: UInt8, blue: UInt8, brightness: UInt8) {
            self.powerOn = powerOn
            self.red = red
            self.green = green
            self.blue = blue
            self.brightness = brightness
        }

        init(_ state: MobileMelkLightingRestoreStateDto) {
            powerOn = state.powerOn
            red = state.red
            green = state.green
            blue = state.blue
            brightness = state.brightness
        }

        var dto: MobileMelkLightingRestoreStateDto {
            MobileMelkLightingRestoreStateDto(
                powerOn: powerOn,
                red: red,
                green: green,
                blue: blue,
                brightness: brightness
            )
        }
    }

    private let session = MelkLightingPeripheralSession()
    private let persistence: LightingAccessoryPersistence

    private(set) var connectionState: MelkLightingPeripheralState = .idle
    private(set) var peripheralName: String?
    private(set) var peripheralIdentifier: String?
    private(set) var commandStatus: MelkLightingCommandStatus = .idle
    private(set) var restoreEnabled: Bool
    private(set) var accessoryAlias: String?
    private(set) var vehicleIdentifier: String?
    private(set) var records: [MelkValidationLogEntry] = []
    private(set) var notificationCount = 0
    private var isRunning = false
    private var requestedState = SolidState(powerOn: false, red: 255, green: 0, blue: 0, brightness: 100)
    private var restoreAttempted = false

    init() {
        persistence = LightingAccessoryPersistence()
        restoreEnabled = persistence.restoreEnabled
        accessoryAlias = persistence.alias
        vehicleIdentifier = persistence.vehicleIdentifier
        if let requested = persistence.requestedState {
            requestedState = SolidState(requested)
        }
        session.onStateChange = { [weak self] state in
            Task { @MainActor in
                if state == .scanning {
                    self?.peripheralName = nil
                    self?.peripheralIdentifier = nil
                    self?.restoreAttempted = false
                }
                self?.connectionState = state
                if state == .ready {
                    self?.ensureRecordForConnectedAccessory()
                    self?.persistence.setConnection(.ready)
                    self?.restoreIfEligible()
                } else if state == .disconnected {
                    self?.restoreAttempted = false
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
        session.start()
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
        requestedState = SolidState(powerOn: false, red: 255, green: 0, blue: 0, brightness: 100)
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
        try? persistence.confirm(requestedState.dto)
        restoreAttempted = true
    }

    func markUnconfirmed() {
        guard commandStatus == .requested else { return }
        session.markLastCommandUnconfirmed()
        persistence.markUnconfirmed()
        commandStatus = .unconfirmed
    }

    var isReady: Bool { connectionState == .ready }

    var presets: [MobileRgbLightingPresetDto] { persistence.presets }

    var canSavePreset: Bool { persistence.platformIdentifier != nil }

    var canEditMetadata: Bool { persistence.platformIdentifier != nil }

    func savePreset(named name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        try? persistence.addPreset(name: trimmed, requested: requestedState.dto)
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
            records = Array((records + [MelkValidationLogEntry(text: record)]).suffix(12))
        } else if record.hasPrefix("notification=") {
            notificationCount += 1
            records = Array((records + [MelkValidationLogEntry(text: "FFF4 notification received")]).suffix(12))
        } else if record.hasPrefix("requested=") {
            records = Array((records + [MelkValidationLogEntry(text: "command requested")]).suffix(12))
        } else {
            records = Array((records + [MelkValidationLogEntry(text: record)]).suffix(12))
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
        try? persistence.updateRequestedState(requestedState.dto)
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
        requestedState = SolidState(requested)
        commandStatus = .requested
        records = Array((records + [MelkValidationLogEntry(text: "restore=requested")]).suffix(12))
    }
}

struct MelkValidationLogEntry: Identifiable {
    let id = UUID()
    let text: String
}

struct LightingRouteView: View {
    let model: LightingRouteModel
    let rideModel: CutoutAppModel
    @State private var brightness = 100.0
    @State private var presetName = ""
    @State private var accessoryAlias = ""
    @State private var vehicleIdentifier = ""
    @State private var showsForgetConfirmation = false

    private var controlsEnabled: Bool { model.isReady }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                PevScreenTitleBlock(
                    title: "Lighting",
                    subtitle: "Standalone RGB controller"
                )

                connectionCard
                metadataCard
                rideCard
                controlsCard
                presetsCard
                commandStatusCard
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
        }
        .background(PevColors.pageBackground.ignoresSafeArea())
        .task {
            model.start()
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
                accessoryAlias = ""
                vehicleIdentifier = ""
            }
        } message: {
            Text("Its alias, vehicle association, presets, and automatic restore preference will be removed.")
        }
        .accessibilityIdentifier("dashboard.screen.lighting")
    }

    private var connectionCard: some View {
        lightingCard {
            HStack(alignment: .top, spacing: 12) {
                Label("Controller", systemImage: "lightbulb.led.fill")
                    .font(.headline)
                Spacer(minLength: 8)
                connectionPill
            }

            Text(model.peripheralName ?? "MELK-OC21")
                .font(.title3.weight(.bold))
                .accessibilityIdentifier("lighting.controller-name")

            Text(model.peripheralName == nil ? "Scanning for a verified accessory" : "Verified MELK-OC21 profile")
                .font(.subheadline)
                .foregroundStyle(PevColors.muted)

            HStack(spacing: 8) {
                profileBadge("FFF0 service")
                profileBadge("FFF3 write")
                profileBadge("FFF4 notify")
            }

            if model.canReconnect {
                Button {
                    model.reconnect()
                } label: {
                    Label("Reconnect", systemImage: "arrow.clockwise")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("lighting.reconnect")
            }

            if model.canEditMetadata {
                Button("Forget accessory", role: .destructive) {
                    showsForgetConfirmation = true
                }
                .buttonStyle(.bordered)
                .frame(maxWidth: .infinity)
                .accessibilityIdentifier("lighting.forget-accessory")
            }

            Toggle(
                "Restore last confirmed state",
                isOn: Binding(
                    get: { model.restoreEnabled },
                    set: { model.setRestoreEnabled($0) }
                )
            )
            .accessibilityIdentifier("lighting.restore-toggle")
            Text("Only the same verified accessory can restore its last confirmed state.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var rideCard: some View {
        lightingCard {
            HStack(alignment: .top, spacing: 12) {
                Label("Ride session", systemImage: "figure.roll")
                    .font(.headline)
                Spacer(minLength: 8)
                Text(rideModel.connectionStatusText)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(PevColors.muted)
            }
            Text("Lighting uses an independent Bluetooth connection and does not replace the active ride.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)
        }
    }

    private var metadataCard: some View {
        lightingCard {
            Label("Accessory details", systemImage: "tag.fill")
                .font(.headline)
            Text("Give this controller a memorable name and optionally associate it with the installed ride.")
                .font(.footnote)
                .foregroundStyle(PevColors.muted)

            TextField("Alias", text: $accessoryAlias)
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
                model.saveAccessoryMetadata(
                    alias: accessoryAlias,
                    vehicleIdentifier: vehicleIdentifier
                )
            }
            .buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity)
            .disabled(!model.canEditMetadata)
            .accessibilityIdentifier("lighting.save-accessory-details")
        }
    }

    private var controlsCard: some View {
        lightingCard {
            HStack {
                Label("Manual controls", systemImage: "slider.horizontal.3")
                    .font(.headline)
                Spacer()
                Text(controlsEnabled ? "Ready" : "Connect first")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(controlsEnabled ? PevColors.green : PevColors.muted)
            }

            VStack(spacing: 10) {
                HStack(spacing: 10) {
                    lightingButton("Power on", systemImage: "power", identifier: "lighting.power-on") {
                        model.setPower(true)
                    }
                    lightingButton("Power off", systemImage: "power", identifier: "lighting.power-off") {
                        model.setPower(false)
                    }
                }

                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                    colorButton("Red", color: .red, identifier: "lighting.color.red") {
                        model.setSolidColor(red: 255, green: 0, blue: 0)
                    }
                    colorButton("Green", color: .green, identifier: "lighting.color.green") {
                        model.setSolidColor(red: 0, green: 255, blue: 0)
                    }
                    colorButton("Blue", color: .blue, identifier: "lighting.color.blue") {
                        model.setSolidColor(red: 0, green: 0, blue: 255)
                    }
                    colorButton("Mixed", color: .orange, identifier: "lighting.color.mixed") {
                        model.setSolidColor(red: 255, green: 96, blue: 24)
                    }
                }

                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Label("Brightness", systemImage: "sun.max")
                        Spacer()
                        Text("\(Int(brightness))%")
                            .monospacedDigit()
                    }
                    Slider(value: $brightness, in: 1...100, step: 1) { editing in
                        if !editing, controlsEnabled {
                            model.setBrightness(UInt8(brightness.rounded()))
                        }
                    }
                    .disabled(!controlsEnabled)
                    .accessibilityIdentifier("lighting.brightness")
                    .accessibilityValue("\(Int(brightness)) percent")
                }
            }
            .disabled(!controlsEnabled)
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
                if model.presets.isEmpty {
                    Text("Save a confirmed solid color and brightness as a named preset.")
                        .font(.footnote)
                        .foregroundStyle(PevColors.muted)
                } else {
                    ForEach(model.presets, id: \.name) { preset in
                        Button {
                            brightness = Double(preset.requested.brightness)
                            model.applyPreset(preset)
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

    private func profileBadge(_ text: String) -> some View {
        Text(text)
            .font(.caption2.monospaced())
            .foregroundStyle(PevColors.muted)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(PevColors.cardFill.opacity(0.7), in: Capsule())
    }

    private func lightingCard<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12, content: content)
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevDashboardCardBackground(cornerRadius: 20))
    }

    private func lightingButton(
        _ title: String,
        systemImage: String,
        identifier: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .accessibilityIdentifier(identifier)
    }

    private func colorButton(
        _ title: String,
        color: Color,
        identifier: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Label(title, systemImage: "circle.fill")
                .frame(maxWidth: .infinity)
        }
        .tint(color)
        .buttonStyle(.bordered)
        .accessibilityIdentifier(identifier)
    }
}

struct MelkValidationRouteView: View {
    let rideModel: CutoutAppModel
    let lighting: LightingRouteModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("MELK-OC21 validation")
                    .font(.largeTitle.bold())

                Text("Standalone accessory harness. LotusLamp X is only the historical official app label.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                statusCard
                rideCard
                commandControls
                evidenceControls
                testCoverageCard
            }
            .padding()
        }
        .background(PevColors.pageBackground.ignoresSafeArea())
        .task { lighting.start() }
    }

    private var statusCard: some View {
        GroupBox("Lighting connection") {
            Label(lighting.connectionState.displayText, systemImage: lighting.connectionState.symbolName)
                .accessibilityIdentifier("melk.validation.connection-state")
            Text("FFF0 service · FFF3 write · FFF4 notify")
                .font(.footnote.monospaced())
                .foregroundStyle(.secondary)
        }
    }

    private var rideCard: some View {
        GroupBox("Primary ride session") {
            Label(rideModel.connectionStatusText, systemImage: "figure.roll")
            Text("The MELK session uses an independent CoreBluetooth central.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }

    private var commandControls: some View {
        GroupBox("Typed commands") {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    commandButton("Power on", systemImage: "power") { lighting.setPower(true) }
                    commandButton("Power off", systemImage: "power") { lighting.setPower(false) }
                }
                HStack {
                    commandButton("Red", systemImage: "circle.fill") { lighting.setSolidColor(red: 255, green: 0, blue: 0) }
                    commandButton("Green", systemImage: "circle.fill") { lighting.setSolidColor(red: 0, green: 255, blue: 0) }
                    commandButton("Blue", systemImage: "circle.fill") { lighting.setSolidColor(red: 0, green: 0, blue: 255) }
                }
                HStack {
                    commandButton("Mixed", systemImage: "circle.fill") { lighting.setSolidColor(red: 255, green: 96, blue: 24) }
                    ForEach([10, 50, 100], id: \.self) { percentage in
                        commandButton("\(percentage)%", systemImage: "sun.max") {
                            lighting.setBrightness(UInt8(percentage))
                        }
                    }
                }
            }
        }
    }

    private var evidenceControls: some View {
        GroupBox("Command evidence") {
            VStack(alignment: .leading, spacing: 10) {
                Label("Status: \(lighting.commandStatus.displayText)", systemImage: lighting.commandStatus.symbolName)
                    .accessibilityIdentifier("melk.validation.command-status")
                Text("A write is never treated as success without an explicit confirmation observation.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                HStack {
                    Button("Mark confirmed") { lighting.markConfirmed() }
                    Button("Mark unconfirmed") { lighting.markUnconfirmed() }
                }
                .buttonStyle(.bordered)
            }
        }
    }

    private var testCoverageCard: some View {
        GroupBox("Branch-added tests") {
            VStack(alignment: .leading, spacing: 6) {
                Text("Core RGB model: preserves all channels, bounds brightness, keeps commands typed, and gates restore on opt-in plus the same accessory identity.")
                Text("MELK protocol: verifies ELK-BLEDOM frame encoding, exact MELK-OC21 and GATT matching, and FFF3 write/FFF4 confirmation policy.")
                Text("Mobile FFI: checks typed GATT evidence, exported payloads, invalid brightness, and restore DTO decisions.")
                Text("CoreBluetooth harness: covers standalone scanning, explicit command evidence, typed writes/subscription, and observed identity roles.")
                Text("Ride navigation: covers Lighting tab routing, selection, shortcuts, localization, and unavailable-tab behavior for EUC and VESC.")
            }
            .font(.footnote)
            .foregroundStyle(.secondary)
        }
    }

    private func commandButton(_ title: String, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
    }
}

private extension MelkLightingPeripheralState {
    var displayText: String {
        switch self {
        case .idle: "Idle"
        case .scanning: "Scanning for MELK-OC21"
        case .connecting: "Connecting"
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
