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
private final class MelkValidationModel {
    private let session = MelkLightingPeripheralSession()

    private(set) var connectionState: MelkLightingPeripheralState = .idle
    private(set) var commandStatus: MelkLightingCommandStatus = .idle
    private(set) var records: [MelkValidationLogEntry] = []
    private(set) var notificationCount = 0
    private var isRunning = false

    init() {
        session.onStateChange = { [weak self] state in
            Task { @MainActor in
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

    private func append(_ record: String) {
        if record.hasPrefix("notification=") {
            notificationCount += 1
            records = Array((records + [MelkValidationLogEntry(text: "FFF4 notification received")]).suffix(12))
        } else if record.hasPrefix("requested=") {
            records = Array((records + [MelkValidationLogEntry(text: "command requested")]).suffix(12))
        } else {
            records = Array((records + [MelkValidationLogEntry(text: record)]).suffix(12))
        }
    }
}

private struct MelkValidationLogEntry: Identifiable {
    let id = UUID()
    let text: String
}

struct MelkValidationRouteView: View {
    let rideModel: CutoutAppModel
    @State private var validation = MelkValidationModel()

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
        .task { validation.start() }
        .onDisappear { validation.stop() }
    }

    private var statusCard: some View {
        GroupBox("Lighting connection") {
            Label(validation.connectionState.displayText, systemImage: validation.connectionState.symbolName)
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
                    commandButton("Power on", systemImage: "power") { validation.setPower(true) }
                    commandButton("Power off", systemImage: "power") { validation.setPower(false) }
                }
                HStack {
                    commandButton("Red", systemImage: "circle.fill") { validation.setSolidColor(red: 255, green: 0, blue: 0) }
                    commandButton("Green", systemImage: "circle.fill") { validation.setSolidColor(red: 0, green: 255, blue: 0) }
                    commandButton("Blue", systemImage: "circle.fill") { validation.setSolidColor(red: 0, green: 0, blue: 255) }
                }
                HStack {
                    commandButton("Mixed", systemImage: "circle.fill") { validation.setSolidColor(red: 255, green: 96, blue: 24) }
                    ForEach([10, 50, 100], id: \.self) { percentage in
                        commandButton("\(percentage)%", systemImage: "sun.max") {
                            validation.setBrightness(UInt8(percentage))
                        }
                    }
                }
            }
        }
    }

    private var evidenceControls: some View {
        GroupBox("Command evidence") {
            VStack(alignment: .leading, spacing: 10) {
                Label("Status: \(validation.commandStatus.displayText)", systemImage: validation.commandStatus.symbolName)
                    .accessibilityIdentifier("melk.validation.command-status")
                Text("A write is never treated as success without an explicit confirmation observation.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                HStack {
                    Button("Mark confirmed") { validation.markConfirmed() }
                    Button("Mark unconfirmed") { validation.markUnconfirmed() }
                }
                .buttonStyle(.bordered)
            }
        }
    }

    private var recordsCard: some View {
        GroupBox("Validation log") {
            VStack(alignment: .leading, spacing: 6) {
                Text("FFF4 notifications: \(validation.notificationCount)")
                    .font(.footnote.monospaced())
                ForEach(validation.records) { record in
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
