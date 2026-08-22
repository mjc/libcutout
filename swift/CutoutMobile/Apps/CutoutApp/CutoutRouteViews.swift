import CutoutMobile
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
    private let session = MelkLightingPeripheralSession()

    private(set) var connectionState: MelkLightingPeripheralState = .idle
    private(set) var peripheralName: String?
    private(set) var commandStatus: MelkLightingCommandStatus = .idle
    private(set) var records: [MelkValidationLogEntry] = []
    private(set) var notificationCount = 0
    private var isRunning = false

    init() {
        session.onStateChange = { [weak self] state in
            Task { @MainActor in
                if state == .scanning {
                    self?.peripheralName = nil
                }
                self?.connectionState = state
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
        commandStatus = .requested
    }

    func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) {
        guard session.setSolidColor(red: red, green: green, blue: blue) else { return }
        commandStatus = .requested
    }

    func setBrightness(_ percentage: UInt8) {
        guard (try? session.setBrightness(percentage)) == true else { return }
        commandStatus = .requested
    }

    func markConfirmed() {
        session.markLastCommandConfirmed()
        commandStatus = .confirmed
    }

    func markUnconfirmed() {
        session.markLastCommandUnconfirmed()
        commandStatus = .unconfirmed
    }

    var isReady: Bool { connectionState == .ready }

    var canReconnect: Bool {
        switch connectionState {
        case .disconnected, .failed:
            true
        default:
            false
        }
    }

    private func append(_ record: String) {
        if record.hasPrefix("candidate=") {
            let candidate = record.dropFirst("candidate=".count)
            peripheralName = String(candidate.split(separator: " rssi=", maxSplits: 1).first ?? candidate)
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

                HStack {
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
                recordsCard
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

    private var recordsCard: some View {
        GroupBox("Validation log") {
            VStack(alignment: .leading, spacing: 6) {
                Text("FFF4 notifications: \(lighting.notificationCount)")
                .font(.footnote.monospaced())
                ForEach(lighting.records) { record in
                    Text(record.text)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                }
            }
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
