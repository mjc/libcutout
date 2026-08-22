import CutoutMobile
import Foundation
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
        .safeAreaInset(edge: .bottom, spacing: 12) {
            Button {
                navigate(.rideMap)
            } label: {
                Label(localizedAppText("tab.map"), systemImage: "map")
                    .frame(maxWidth: .infinity, minHeight: 44)
            }
            .buttonStyle(.borderedProminent)
            .padding(.horizontal, 24)
            .padding(.bottom, 8)
            .accessibilityIdentifier("device-picker.open-map")
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
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            Form {
                Section {
                    Toggle(
                        model.headlightControlTitle,
                        isOn: Binding(
                            get: { model.headlightOn },
                            set: { model.setHeadlight($0) }
                        )
                    )
                    .disabled(model.phase != .live || !model.headlightControlAvailable)
                    .accessibilityHint(model.headlightStatusText)
                } header: {
                    Text(localizedAppText("settings.lights.title"))
                } footer: {
                    Text(model.headlightStatusText)
                }

                if let settings = model.settingsReadback?.eucGarageSettings {
                    Section {
                        EucSettingReadbackRow(
                            id: "beepMargin",
                            title: localizedAppText("settings.beep_margin.title"),
                            value: EucSettingReadbackPresentation.speed(settings.beepMargin)
                        )
                        EucSettingReadbackRow(
                            id: "tiltback",
                            title: localizedAppText("settings.tiltback.title"),
                            value: EucSettingReadbackPresentation.speed(settings.tiltback)
                        )
                        EucSettingReadbackRow(
                            id: "pedalMode",
                            title: localizedAppText("settings.pedal_mode.title"),
                            value: EucSettingReadbackPresentation.pedalMode(settings.pedalMode)
                        )
                    } header: {
                        Text(localizedAppText("settings.readback.title"))
                    } footer: {
                        Text(localizedAppText("settings.readback.footer"))
                    }
                }

                if let capabilities = model.settingsCapabilities {
                    Section {
                        EucSettingCapabilityRow(
                            id: "pedalMode",
                            title: localizedAppText("settings.pedal_mode.title"),
                            support: capabilities.pedalMode
                        )
                        EucSettingCapabilityRow(
                            id: "accelerationAssist",
                            title: localizedAppText("settings.acceleration_assist.title"),
                            support: capabilities.accelerationAssist
                        )
                        EucSettingCapabilityRow(
                            id: "taillight",
                            title: localizedAppText("settings.taillight.title"),
                            support: capabilities.taillight
                        )
                    } header: {
                        Text(localizedAppText("settings.capabilities.title"))
                    } footer: {
                        Text(localizedAppText("settings.capabilities.footer"))
                    }
                }
            }
        }
        .accessibilityIdentifier("settings.screen.eucTune")
    }
}

private struct EucSettingReadbackRow: View {
    let id: String
    let title: String
    let value: String

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            Text(value)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("settings.readback.\(id)")
    }
}

enum EucSettingReadbackPresentation {
    static func speed(_ readback: ReadbackValue<Speed>) -> String {
        guard let value = readback.value else {
            return availabilityText(readback.availability)
        }
        let readout = SpeedReadout(millimetersPerSecond: value.value)
        return "\(readout.displayValue) \(readout.displayUnit)"
    }

    static func pedalMode(_ readback: ReadbackValue<PedalMode>) -> String {
        guard let value = readback.value else {
            return availabilityText(readback.availability)
        }
        if let percent = value.percent {
            return "\(percent)%"
        }
        if let rawMode = value.rawMode {
            return "Mode \(rawMode)"
        }
        return availabilityText(.unavailable)
    }

    private static func availabilityText(_ availability: ReadbackAvailability) -> String {
        switch availability {
        case .available:
            localizedAppText("settings.readback.unavailable")
        case .unavailable:
            localizedAppText("settings.readback.unavailable")
        case .unsupported:
            localizedAppText("settings.readback.unsupported")
        }
    }
}

private struct EucSettingCapabilityRow: View {
    let id: String
    let title: String
    let support: SettingWriteSupport

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            Text(statusText)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("settings.capability.\(id)")
    }

    private var statusText: String {
        switch support {
        case .supported:
            localizedAppText("settings.capabilities.supported")
        case .unverified:
            localizedAppText("settings.capabilities.unverified")
        case .unsupported:
            localizedAppText("settings.capabilities.unsupported")
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
