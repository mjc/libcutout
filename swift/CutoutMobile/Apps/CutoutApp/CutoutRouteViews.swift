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
                    if model.pedalModeControlAvailable {
                        EucPedalModeControl(model: model)
                    }
                } header: {
                    Text(localizedAppText("settings.lights.title"))
                } footer: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(model.headlightStatusText)
                        if model.pedalModeControlAvailable {
                            Text(localizedAppText("settings.pedal_mode.footer"))
                        }
                    }
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
                            value: EucSettingReadbackPresentation.pedalMode(
                                model.pedalModeState,
                                fallback: settings.pedalMode
                            )
                        )
                        EucSettingReadbackRow(
                            id: "autoShutdown",
                            title: localizedAppText("settings.auto_shutdown.title"),
                            value: EucSettingReadbackPresentation.seconds(settings.autoShutdownSeconds)
                        )
                        EucSettingReadbackRow(
                            id: "chargeMode",
                            title: localizedAppText("settings.charge_mode.title"),
                            value: EucSettingReadbackPresentation.chargeMode(settings.chargeMode)
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
                            support: capabilities.pedalMode,
                            state: model.pedalModeState?.kind,
                            confirmedAt: model.pedalModeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "accelerationAssist",
                            title: localizedAppText("settings.acceleration_assist.title"),
                            support: capabilities.accelerationAssist,
                            state: model.accelerationAssistState?.kind,
                            confirmedAt: model.accelerationAssistState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "taillight",
                            title: localizedAppText("settings.taillight.title"),
                            support: capabilities.taillight,
                            state: model.taillightState?.kind,
                            confirmedAt: model.taillightState?.confirmedAt,
                            now: model.currentMonotonicTime
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

private struct EucPedalModeControl: View {
    let model: CutoutAppModel
    @State private var selectedMode: PedalMode.Kind = .hard

    private static let modes: [PedalMode.Kind] = [.hard, .medium, .soft]

    var body: some View {
        Picker(
            localizedAppText("settings.pedal_mode.title"),
            selection: Binding(
                get: { model.pedalModeState?.current ?? readbackMode ?? selectedMode },
                set: {
                    selectedMode = $0
                    _ = model.setPedalMode($0)
                }
            )
        ) {
            ForEach(Self.modes, id: \.self) { mode in
                Text(localizedAppText("settings.pedal_mode.\(mode.localizationKey)"))
                    .tag(mode)
            }
        }
        .pickerStyle(.menu)
        .disabled(model.phase != .live)
        .accessibilityHint(localizedAppText("settings.pedal_mode.footer"))
        .accessibilityIdentifier("settings.control.pedalMode")
    }

    private var readbackMode: PedalMode.Kind? {
        model.settingsReadback?.eucGarageSettings.pedalMode.value?.documentedKind
    }
}

private extension PedalMode.Kind {
    var localizationKey: String {
        switch self {
        case .hard: "hard"
        case .medium: "medium"
        case .soft: "soft"
        }
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

    static func pedalMode(
        _ state: PedalModeSettingState?,
        fallback readback: ReadbackValue<PedalMode>
    ) -> String {
        if let current = state?.current {
            return current.displayName
        }
        return pedalMode(readback)
    }

    static func pedalMode(_ readback: ReadbackValue<PedalMode>) -> String {
        guard let value = readback.value else {
            return availabilityText(readback.availability)
        }
        if let kind = value.documentedKind {
            return kind.displayName
        }
        if let percent = value.percent {
            return "\(percent)%"
        }
        if let rawMode = value.rawMode {
            return "Raw \(rawMode)"
        }
        return availabilityText(.unavailable)
    }

    static func seconds(_ readback: ReadbackValue<UInt64>) -> String {
        guard let value = readback.value else {
            return availabilityText(readback.availability)
        }
        return localizedAppText("settings.seconds.value", value)
    }

    static func chargeMode(_ readback: ReadbackValue<ChargeMode>) -> String {
        guard let value = readback.value else {
            return availabilityText(readback.availability)
        }
        switch value {
        case .charging:
            return localizedAppText("settings.charge_mode.charging")
        case .notCharging:
            return localizedAppText("settings.charge_mode.not_charging")
        }
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
    let state: SettingStateKind?
    let confirmedAt: MonotonicMilliseconds?
    let now: MonotonicMilliseconds

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            Text(EucSettingCapabilityPresentation.statusText(
                support: support,
                state: state,
                confirmedAt: confirmedAt,
                now: now
            ))
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("settings.capability.\(id)")
    }

}

enum EucSettingCapabilityPresentation {
    static func statusText(
        support: SettingWriteSupport,
        state: SettingStateKind?,
        confirmedAt: MonotonicMilliseconds? = nil,
        now: MonotonicMilliseconds? = nil
    ) -> String {
        switch state {
        case .pending:
            return localizedAppText("settings.state.pending")
        case .confirmed:
            if let confirmedAt, let now {
                return localizedAppText(
                    "settings.state.confirmed_ago",
                    Int64(now.elapsed(since: confirmedAt).rawValue / 1_000)
                )
            }
            return localizedAppText("settings.state.confirmed")
        case .refused:
            return localizedAppText("settings.state.refused")
        case .timedOut:
            return localizedAppText("settings.state.timed_out")
        case .failed:
            return localizedAppText("settings.state.failed")
        case .unknown, .current, nil:
            return supportText(support)
        }
    }

    private static func supportText(_ support: SettingWriteSupport) -> String {
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
