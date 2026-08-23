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
    }

    private enum RestoreKeys {
        static let enabled = "lighting.restore.enabled"
        static let platformIdentifier = "lighting.restore.platformIdentifier"
        static let powerOn = "lighting.restore.powerOn"
        static let red = "lighting.restore.red"
        static let green = "lighting.restore.green"
        static let blue = "lighting.restore.blue"
        static let brightness = "lighting.restore.brightness"
    }

    private let session = MelkLightingPeripheralSession()
    private let defaults = UserDefaults.standard

    private(set) var connectionState: MelkLightingPeripheralState = .idle
    private(set) var peripheralName: String?
    private(set) var peripheralIdentifier: String?
    private(set) var commandStatus: MelkLightingCommandStatus = .idle
    private(set) var restoreEnabled: Bool
    private(set) var records: [MelkValidationLogEntry] = []
    private(set) var notificationCount = 0
    private var isRunning = false
    private var requestedState = SolidState(powerOn: false, red: 255, green: 0, blue: 0, brightness: 100)
    private var restoreMarker: MobileMelkLightingRestoreMarker?
    private var restoreAttempted = false

    init() {
        restoreEnabled = defaults.bool(forKey: RestoreKeys.enabled)
        if let platformIdentifier = defaults.string(forKey: RestoreKeys.platformIdentifier) {
            restoreMarker = try? MobileMelkLightingRestoreMarker(
                platformIdentifier: platformIdentifier,
                requested: MobileMelkLightingRestoreStateDto(
                    powerOn: defaults.bool(forKey: RestoreKeys.powerOn),
                    red: UInt8(clamping: defaults.integer(forKey: RestoreKeys.red)),
                    green: UInt8(clamping: defaults.integer(forKey: RestoreKeys.green)),
                    blue: UInt8(clamping: defaults.integer(forKey: RestoreKeys.blue)),
                    brightness: UInt8(clamping: defaults.integer(forKey: RestoreKeys.brightness))
                )
            )
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
                    self?.restoreIfEligible()
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

    func reconnect() {
        stop()
        start()
    }

    func setPower(_ on: Bool) {
        guard session.setPower(on) else { return }
        requestedState.powerOn = on
        commandStatus = .requested
    }

    func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) {
        guard session.setSolidColor(red: red, green: green, blue: blue) else { return }
        requestedState.red = red
        requestedState.green = green
        requestedState.blue = blue
        commandStatus = .requested
    }

    func setBrightness(_ percentage: UInt8) {
        guard (try? session.setBrightness(percentage)) == true else { return }
        requestedState.brightness = percentage
        commandStatus = .requested
    }

    func markConfirmed() {
        guard commandStatus == .requested else { return }
        session.markLastCommandConfirmed()
        commandStatus = .confirmed
        guard let peripheralIdentifier else { return }
        restoreMarker = try? MobileMelkLightingRestoreMarker(
            platformIdentifier: peripheralIdentifier,
            requested: MobileMelkLightingRestoreStateDto(
                powerOn: requestedState.powerOn,
                red: requestedState.red,
                green: requestedState.green,
                blue: requestedState.blue,
                brightness: requestedState.brightness
            )
        )
        persistRestoreMarker()
        restoreAttempted = true
    }

    func markUnconfirmed() {
        guard commandStatus == .requested else { return }
        session.markLastCommandUnconfirmed()
        commandStatus = .unconfirmed
    }

    var isReady: Bool { connectionState == .ready }

    func setRestoreEnabled(_ enabled: Bool) {
        restoreEnabled = enabled
        defaults.set(enabled, forKey: RestoreKeys.enabled)
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

    private func persistRestoreMarker() {
        guard restoreMarker != nil, let identifier = peripheralIdentifier else {
            return
        }
        defaults.set(identifier, forKey: RestoreKeys.platformIdentifier)
        defaults.set(requestedState.powerOn, forKey: RestoreKeys.powerOn)
        defaults.set(Int(requestedState.red), forKey: RestoreKeys.red)
        defaults.set(Int(requestedState.green), forKey: RestoreKeys.green)
        defaults.set(Int(requestedState.blue), forKey: RestoreKeys.blue)
        defaults.set(Int(requestedState.brightness), forKey: RestoreKeys.brightness)
    }

    private func restoreIfEligible() {
        guard restoreEnabled, !restoreAttempted,
              let restoreMarker,
              let peripheralIdentifier else {
            return
        }
        restoreAttempted = true
        let decision = restoreMarker.recover(
            restoredPlatformIdentifier: peripheralIdentifier,
            restoreEnabled: true
        )
        guard decision.kind == .restore, let requested = decision.requested else {
            return
        }
        guard session.setPower(requested.powerOn),
              session.setSolidColor(red: requested.red, green: requested.green, blue: requested.blue),
              (try? session.setBrightness(requested.brightness)) == true else {
            return
        }
        requestedState = SolidState(
            powerOn: requested.powerOn,
            red: requested.red,
            green: requested.green,
            blue: requested.blue,
            brightness: requested.brightness
        )
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

    private var controlsEnabled: Bool { model.isReady }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Lighting")
                    .font(.largeTitle.bold())

                Text("Standalone RGB controller")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                connectionCard
                rideCard
                controlsCard
                commandStatusCard
            }
            .padding()
        }
        .background(PevColors.pageBackground.ignoresSafeArea())
        .task { model.start() }
        .accessibilityIdentifier("dashboard.screen.lighting")
    }

    private var connectionCard: some View {
        GroupBox("Controller") {
            VStack(alignment: .leading, spacing: 8) {
                Label(model.connectionState.displayText, systemImage: model.connectionState.symbolName)
                    .accessibilityIdentifier("lighting.connection-state")
                Text(model.peripheralName ?? "Scanning for MELK-OC21")
                    .font(.headline)
                Text("Verified MELK-OC21 profile · FFF0 service · FFF3 write · FFF4 notify")
                    .font(.footnote.monospaced())
                    .foregroundStyle(.secondary)
                if model.canReconnect {
                    Button("Reconnect") { model.reconnect() }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("lighting.reconnect")
                }
                Toggle(
                    "Restore last confirmed state",
                    isOn: Binding(
                        get: { model.restoreEnabled },
                        set: { model.setRestoreEnabled($0) }
                    )
                )
                .accessibilityIdentifier("lighting.restore-toggle")
                Text("Restore is attempted only after the same verified accessory reconnects.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var rideCard: some View {
        GroupBox("Ride session") {
            VStack(alignment: .leading, spacing: 8) {
                Label(rideModel.connectionStatusText, systemImage: "figure.roll")
                Text("Lighting uses an independent CoreBluetooth central and remains available while the ride session is connected.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var controlsCard: some View {
        GroupBox("Manual controls") {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    lightingButton("Power on", systemImage: "power", identifier: "lighting.power-on") {
                        model.setPower(true)
                    }
                    lightingButton("Power off", systemImage: "power", identifier: "lighting.power-off") {
                        model.setPower(false)
                    }
                }

                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
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

                VStack(alignment: .leading, spacing: 6) {
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
        GroupBox("Command status") {
            VStack(alignment: .leading, spacing: 8) {
                Label("Status: \(model.commandStatus.displayText)", systemImage: model.commandStatus.symbolName)
                    .accessibilityIdentifier("lighting.command-status")
                Text("Writes remain requested until you explicitly observe the controller change.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                HStack {
                    Button("Mark confirmed") { model.markConfirmed() }
                        .accessibilityIdentifier("lighting.mark-confirmed")
                    Button("Mark unconfirmed") { model.markUnconfirmed() }
                        .accessibilityIdentifier("lighting.mark-unconfirmed")
                }
                .buttonStyle(.bordered)
                .disabled(model.commandStatus != .requested)
            }
        }
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
