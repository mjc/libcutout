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
    @State private var showingTripResetConfirmation = false

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            Form {
                if model.settingsCapabilities?.validationMode == true {
                    Section {
                        Text(localizedAppText("settings.validation.description"))
                    } header: {
                        Text(localizedAppText("settings.validation.title"))
                    }
                    .accessibilityIdentifier("settings.validationMode")
                }
                Section {
                    HStack {
                        Button(localizedAppText("settings.headlight.turn_on")) {
                            _ = model.setHeadlight(true)
                        }
                        Button(localizedAppText("settings.headlight.turn_off")) {
                            _ = model.setHeadlight(false)
                        }
                    }
                    .buttonStyle(.bordered)
                    .disabled(model.phase != .live || !model.headlightControlAvailable)
                    .accessibilityHint(model.headlightStatusText)
                    .accessibilityIdentifier("settings.control.headlight")
                    if model.manualHeadlightControlVisible {
                        Toggle(
                            localizedAppText("settings.headlight.title"),
                            isOn: Binding(
                                get: { model.manualHeadlightOn },
                                set: { model.setManualHeadlight($0) }
                            )
                        )
                        .disabled(model.phase != .live || !model.manualHeadlightControlAvailable)
                        .accessibilityIdentifier("settings.manualHeadlight")
                        .accessibilityHint(model.manualHeadlightStatusText)
                    }
                    if model.pedalModeControlAvailable {
                        EucPedalModeControl(model: model)
                    }
                    if model.rollAngleControlAvailable {
                        EucRollAngleControl(model: model)
                    }
                    if model.speedAlarmModeControlAvailable {
                        EucSpeedAlarmModeControl(model: model)
                    }
                    if model.begodeMaxSpeedControlAvailable {
                        EucBegodeMaxSpeedControl(model: model)
                    }
                    if model.begodeBeeperVolumeControlAvailable {
                        EucBegodeBeeperVolumeControl(model: model)
                    }
                    if model.begodeLedModeControlAvailable {
                        EucBegodeLedModeControl(model: model)
                    }
                } header: {
                    Text(localizedAppText("settings.lights.title"))
                } footer: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(model.headlightStatusText)
                        if model.pedalModeControlAvailable {
                            Text(localizedAppText("settings.pedal_mode.footer"))
                        }
                        if model.rollAngleControlAvailable {
                            Text(localizedAppText("settings.roll_angle.footer"))
                        }
                        if model.speedAlarmModeControlAvailable {
                            Text(localizedAppText("settings.speed_alarm_mode.footer"))
                        }
                        if model.begodeMaxSpeedControlAvailable {
                            Text(localizedAppText("settings.begode_max_speed.footer"))
                        }
                        if model.begodeBeeperVolumeControlAvailable {
                            Text(localizedAppText("settings.begode_beeper_volume.footer"))
                        }
                        if model.begodeLedModeControlAvailable {
                            Text(localizedAppText("settings.begode_led_mode.footer"))
                        }
                    }
                }

                if model.resetTripMeterControlAvailable {
                    Section {
                        EucSettingReadbackRow(
                            id: "tripDistance",
                            title: localizedAppText("settings.trip_meter.distance_title"),
                            value: EucSettingReadbackPresentation.tripDistance(
                                model.displayState.telemetry?.tripDistance?.value
                            )
                        )
                        Button(localizedAppText("settings.trip_meter.reset"), role: .destructive) {
                            showingTripResetConfirmation = true
                        }
                        .disabled(model.phase != .live || model.tripMeterResetState?.kind == .pending)
                        .accessibilityIdentifier("settings.control.resetTripMeter")
                    } header: {
                        Text(localizedAppText("settings.trip_meter.title"))
                    } footer: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(localizedAppText("settings.trip_meter.footer"))
                            if let status = EucTripMeterResetPresentation.statusText(model.tripMeterResetState) {
                                Text(status)
                                    .accessibilityIdentifier("settings.tripMeterReset.status")
                            }
                        }
                    }
                    .confirmationDialog(
                        localizedAppText("settings.trip_meter.confirm_title"),
                        isPresented: $showingTripResetConfirmation,
                        titleVisibility: .visible
                    ) {
                        Button(localizedAppText("settings.trip_meter.reset"), role: .destructive) {
                            _ = model.resetTripMeter()
                        }
                        Button(localizedAppText("app.command.cancel"), role: .cancel) {}
                    }
                }

                if model.aeroTiltbackSpeedControlAvailable
                    || model.aeroPwmPercentControlAvailable
                    || model.aeroPedalHardnessControlAvailable
                    || model.aeroAlarmSpeedControlAvailable
                    || model.aeroAngleAdjustmentControlAvailable {
                    EucAeroSettingsControls(model: model)
                }

                if model.aeroDisplayBacklightControlAvailable
                    || model.aeroWheelUnitsControlAvailable
                    || model.aeroBeeperVolumeControlAvailable
                    || model.aeroDynamicAssistControlAvailable
                    || model.aeroPedalDipCompensationControlAvailable
                    || model.aeroLateralTiltLimitControlAvailable
                    || model.aeroVoltageCorrectionControlAvailable
                    || model.aeroMaxChargeVoltageRawControlAvailable
                    || model.aeroHighSpeedModeControlAvailable
                    || model.aeroLowBatteryModeControlAvailable
                    || model.aeroTransportModeControlAvailable
                    || model.aeroGyroCalibrationControlAvailable
                    || model.aeroRidingModeControlAvailable
                    || model.aeroBrakeOverpressureAlarmControlAvailable {
                    EucAeroAdditionalSettingsControls(model: model)
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
                            id: "rollAngle",
                            title: localizedAppText("settings.roll_angle.title"),
                            value: EucSettingReadbackPresentation.rollAngle(
                                model.rollAngleState,
                                fallback: settings.rollAngle
                            )
                        )
                        EucSettingReadbackRow(
                            id: "speedAlarmMode",
                            title: localizedAppText("settings.speed_alarm_mode.title"),
                            value: EucSettingReadbackPresentation.speedAlarmMode(
                                model.speedAlarmModeState,
                                fallback: settings.speedAlarmMode
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
                            id: "rollAngle",
                            title: localizedAppText("settings.roll_angle.title"),
                            value: EucSettingReadbackPresentation.rollAngle(
                                model.rollAngleState,
                                fallback: settings.rollAngle
                            )
                        )
                        EucSettingReadbackRow(
                            id: "speedAlarmMode",
                            title: localizedAppText("settings.speed_alarm_mode.title"),
                            value: EucSettingReadbackPresentation.speedAlarmMode(
                                model.speedAlarmModeState,
                                fallback: settings.speedAlarmMode
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
                        if model.manualHeadlightControlVisible {
                            EucSettingCapabilityRow(
                                id: "manualHeadlight",
                                title: localizedAppText("settings.headlight.title"),
                                support: capabilities.headlight,
                                state: model.manualHeadlightState?.kind,
                                confirmedAt: model.manualHeadlightState?.confirmedAt,
                                now: model.currentMonotonicTime
                            )
                        }
                        EucSettingCapabilityRow(
                            id: "aeroHighBeam",
                            title: localizedAppText("settings.high_beam.title"),
                            support: capabilities.aeroHighBeam,
                            state: model.aeroHighBeamState?.kind,
                            confirmedAt: model.aeroHighBeamState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "pedalMode",
                            title: localizedAppText("settings.pedal_mode.title"),
                            support: capabilities.pedalMode,
                            state: model.pedalModeState?.kind,
                            confirmedAt: model.pedalModeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "rollAngle",
                            title: localizedAppText("settings.roll_angle.title"),
                            support: capabilities.rollAngle,
                            state: model.rollAngleState?.kind,
                            confirmedAt: model.rollAngleState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "speedAlarmMode",
                            title: localizedAppText("settings.speed_alarm_mode.title"),
                            support: capabilities.speedAlarmMode,
                            state: model.speedAlarmModeState?.kind,
                            confirmedAt: model.speedAlarmModeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "begodeMaxSpeed",
                            title: localizedAppText("settings.begode_max_speed.title"),
                            support: capabilities.begodeMaxSpeed,
                            state: nil,
                            confirmedAt: nil,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "begodeBeeperVolume",
                            title: localizedAppText("settings.begode_beeper_volume.title"),
                            support: capabilities.begodeBeeperVolume,
                            state: nil,
                            confirmedAt: nil,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "begodeLedMode",
                            title: localizedAppText("settings.begode_led_mode.title"),
                            support: capabilities.begodeLedMode,
                            state: nil,
                            confirmedAt: nil,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroTiltbackSpeed",
                            title: localizedAppText("settings.aero.tiltback.title"),
                            support: capabilities.aeroTiltbackSpeed,
                            state: model.aeroTiltbackSpeedState?.kind,
                            confirmedAt: model.aeroTiltbackSpeedState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroPwmPercent",
                            title: localizedAppText("settings.aero.pwm.title"),
                            support: capabilities.aeroPwmPercent,
                            state: model.aeroPwmPercentState?.kind,
                            confirmedAt: model.aeroPwmPercentState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroPedalHardness",
                            title: localizedAppText("settings.aero.pedal_hardness.title"),
                            support: capabilities.aeroPedalHardness,
                            state: model.aeroPedalHardnessState?.kind,
                            confirmedAt: model.aeroPedalHardnessState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroAlarmSpeed",
                            title: localizedAppText("settings.aero.alarm.title"),
                            support: capabilities.aeroAlarmSpeed,
                            state: model.aeroAlarmSpeedState?.kind,
                            confirmedAt: model.aeroAlarmSpeedState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroAngleAdjustment",
                            title: localizedAppText("settings.aero.angle.title"),
                            support: capabilities.aeroAngleAdjustment,
                            state: model.aeroAngleAdjustmentState?.kind,
                            confirmedAt: model.aeroAngleAdjustmentState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroDisplayBacklight",
                            title: localizedAppText("settings.aero.display_backlight.title"),
                            support: capabilities.aeroDisplayBacklight,
                            state: model.aeroDisplayBacklightState?.kind,
                            confirmedAt: model.aeroDisplayBacklightState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroBeeperVolume",
                            title: localizedAppText("settings.aero.beeper_volume.title"),
                            support: capabilities.aeroBeeperVolume,
                            state: model.aeroBeeperVolumeState?.kind,
                            confirmedAt: model.aeroBeeperVolumeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroDynamicAssist",
                            title: localizedAppText("settings.aero.dynamic_assist.title"),
                            support: capabilities.aeroDynamicAssist,
                            state: model.aeroDynamicAssistState?.kind,
                            confirmedAt: model.aeroDynamicAssistState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroPedalDipCompensation",
                            title: localizedAppText("settings.aero.pedal_dip_compensation.title"),
                            support: capabilities.aeroPedalDipCompensation,
                            state: model.aeroPedalDipCompensationState?.kind,
                            confirmedAt: model.aeroPedalDipCompensationState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroLateralTiltLimit",
                            title: localizedAppText("settings.aero.lateral_tilt_limit.title"),
                            support: capabilities.aeroLateralTiltLimit,
                            state: model.aeroLateralTiltLimitState?.kind,
                            confirmedAt: model.aeroLateralTiltLimitState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroVoltageCorrection",
                            title: localizedAppText("settings.aero.voltage_correction.title"),
                            support: capabilities.aeroVoltageCorrection,
                            state: model.aeroVoltageCorrectionState?.kind,
                            confirmedAt: model.aeroVoltageCorrectionState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroMaxChargeVoltageRaw",
                            title: localizedAppText("settings.aero.max_charge_raw.title"),
                            support: capabilities.aeroMaxChargeVoltageRaw,
                            state: model.aeroMaxChargeVoltageRawState?.kind,
                            confirmedAt: model.aeroMaxChargeVoltageRawState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroGyroCalibration",
                            title: localizedAppText("settings.aero.gyro_calibration.title"),
                            support: capabilities.aeroGyroCalibration,
                            state: model.aeroGyroCalibrationState?.kind,
                            confirmedAt: model.aeroGyroCalibrationState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroWheelUnits",
                            title: localizedAppText("settings.aero.wheel_units.title"),
                            support: capabilities.aeroWheelUnits,
                            state: model.aeroWheelUnitsState?.kind,
                            confirmedAt: model.aeroWheelUnitsState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroHighSpeedMode",
                            title: localizedAppText("settings.aero.high_speed_mode.title"),
                            support: capabilities.aeroHighSpeedMode,
                            state: model.aeroHighSpeedModeState?.kind,
                            confirmedAt: model.aeroHighSpeedModeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroLowBatteryMode",
                            title: localizedAppText("settings.aero.low_battery_mode.title"),
                            support: capabilities.aeroLowBatteryMode,
                            state: model.aeroLowBatteryModeState?.kind,
                            confirmedAt: model.aeroLowBatteryModeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroTransportMode",
                            title: localizedAppText("settings.aero.transport_mode.title"),
                            support: capabilities.aeroTransportMode,
                            state: model.aeroTransportModeState?.kind,
                            confirmedAt: model.aeroTransportModeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroRidingMode",
                            title: localizedAppText("settings.aero.riding_mode.title"),
                            support: capabilities.aeroRidingMode,
                            state: model.aeroRidingModeState?.kind,
                            confirmedAt: model.aeroRidingModeState?.confirmedAt,
                            now: model.currentMonotonicTime
                        )
                        EucSettingCapabilityRow(
                            id: "aeroBrakeOverpressureAlarm",
                            title: localizedAppText("settings.aero.brake_overpressure.title"),
                            support: capabilities.aeroBrakeOverpressureAlarm,
                            state: model.aeroBrakeOverpressureAlarmState?.kind,
                            confirmedAt: model.aeroBrakeOverpressureAlarmState?.confirmedAt,
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
                        Text(localizedAppText(capabilities.validationMode
                            ? "settings.validation.description"
                            : "settings.capabilities.footer"))
                    }
                }
            }
        }
        .accessibilityIdentifier("settings.screen.eucTune")
    }
}

private struct EucAeroSettingsControls: View {
    let model: CutoutAppModel
    @State private var tiltbackSpeed: Int
    @State private var pwmPercent: Int
    @State private var pedalHardnessPercent: Int
    @State private var alarmSpeed: Int
    @State private var angleTenths: Int
    @State private var seededTiltback = false
    @State private var seededPwm = false
    @State private var seededPedalHardness = false
    @State private var seededAlarm = false
    @State private var seededAngle = false

    @MainActor
    init(model: CutoutAppModel) {
        self.model = model
        let values = AeroSettingsFormValues(model: model)
        _tiltbackSpeed = State(initialValue: values.tiltbackSpeed)
        _pwmPercent = State(initialValue: values.pwmPercent)
        _pedalHardnessPercent = State(initialValue: values.pedalHardnessPercent)
        _alarmSpeed = State(initialValue: values.alarmSpeed)
        _angleTenths = State(initialValue: values.angleTenths)
        _seededTiltback = State(initialValue: values.tiltback != nil)
        _seededPwm = State(initialValue: values.pwm != nil)
        _seededPedalHardness = State(initialValue: values.pedalHardness != nil)
        _seededAlarm = State(initialValue: values.alarm != nil)
        _seededAngle = State(initialValue: values.angle != nil)
    }

    var body: some View {
        Section {
            if model.aeroTiltbackSpeedControlAvailable {
                Stepper(value: $tiltbackSpeed, in: 10...200) {
                    Text("\(localizedAppText("settings.aero.tiltback.title")): \(tiltbackSpeed) km/h")
                }
                Button(localizedAppText("settings.aero.send"), action: sendTiltbackSpeed)
                .accessibilityIdentifier("settings.control.aeroTiltbackSpeed")
            }
            if model.aeroPwmPercentControlAvailable {
                Stepper(value: $pwmPercent, in: 0...70) {
                    Text("\(localizedAppText("settings.aero.pwm.title")): \(pwmPercent)%")
                }
                Button(localizedAppText("settings.aero.send"), action: sendPwmPercent)
                .accessibilityIdentifier("settings.control.aeroPwmPercent")
                Button(localizedAppText("settings.aero.pwm.off"), action: disablePwm)
                    .accessibilityIdentifier("settings.control.aeroPwmOff")
                EucSettingReadbackRow(
                    id: "aeroPwm",
                    title: localizedAppText("settings.aero.current_value"),
                    value: model.aeroPwmPercentState?.currentIsOff == true
                        ? localizedAppText("settings.aero.pwm.off")
                        : model.aeroPwmPercentState?.current.map { "\($0.percent)%" }
                            ?? localizedAppText("settings.readback.unavailable")
                )
            }
            if model.aeroPedalHardnessControlAvailable {
                Stepper(value: $pedalHardnessPercent, in: 0...100) {
                    Text("\(localizedAppText("settings.aero.pedal_hardness.title")): \(pedalHardnessPercent)%")
                }
                Button(localizedAppText("settings.aero.send"), action: sendPedalHardness)
                    .accessibilityIdentifier("settings.control.aeroPedalHardness")
                EucSettingReadbackRow(
                    id: "aeroPedalHardness",
                    title: localizedAppText("settings.aero.pedal_hardness.current"),
                    value: model.aeroPedalHardnessState?.current.map { "\($0.percent)%" }
                        ?? localizedAppText("settings.readback.unavailable")
                )
            }
            if model.aeroAlarmSpeedControlAvailable {
                Stepper(value: $alarmSpeed, in: 10...200) {
                    Text("\(localizedAppText("settings.aero.alarm.title")): \(alarmSpeed) km/h")
                }
                Button(localizedAppText("settings.aero.send"), action: sendAlarmSpeed)
                .accessibilityIdentifier("settings.control.aeroAlarmSpeed")
            }
            if model.aeroAngleAdjustmentControlAvailable {
                Stepper(value: $angleTenths, in: -80...80) {
                    Text("\(localizedAppText("settings.aero.angle.title")): \(Double(angleTenths) / 10, specifier: "%.1f")°")
                }
                Button(localizedAppText("settings.aero.send"), action: sendAngleAdjustment)
                .accessibilityIdentifier("settings.control.aeroAngleAdjustment")
            }
        } header: {
            Text(localizedAppText("settings.aero.title"))
        } footer: {
            Text(localizedAppText("settings.aero.footer"))
        }
        .disabled(model.phase != .live)
        .onChange(of: model.aeroTiltbackSpeedState?.current, initial: true) { _, _ in
            seedFromDeviceIfNeeded()
        }
        .onChange(of: model.aeroPwmPercentState?.current, initial: true) { _, _ in
            seedFromDeviceIfNeeded()
        }
        .onChange(of: model.aeroPedalHardnessState?.current, initial: true) { _, _ in
            seedFromDeviceIfNeeded()
        }
        .onChange(of: model.aeroAlarmSpeedState?.current, initial: true) { _, _ in
            seedFromDeviceIfNeeded()
        }
        .onChange(of: model.aeroAngleAdjustmentState?.current, initial: true) { _, _ in
            seedFromDeviceIfNeeded()
        }
    }

    private func seedFromDeviceIfNeeded() {
        if !seededTiltback, let current = model.aeroTiltbackSpeedState?.current {
            tiltbackSpeed = Int(current.kilometresPerHour)
            seededTiltback = true
        }
        if !seededPwm, let current = model.aeroPwmPercentState?.current {
            pwmPercent = Int(current.percent)
            seededPwm = true
        }
        if !seededPedalHardness, let current = model.aeroPedalHardnessState?.current {
            pedalHardnessPercent = Int(current.percent)
            seededPedalHardness = true
        }
        if !seededAlarm, let current = model.aeroAlarmSpeedState?.current {
            alarmSpeed = Int(current.kilometresPerHour)
            seededAlarm = true
        }
        if !seededAngle, let current = model.aeroAngleAdjustmentState?.current {
            angleTenths = Int(current.tenthsOfDegree)
            seededAngle = true
        }
    }

    private func sendTiltbackSpeed() {
        guard let setting = AeroSpeedSetting(kilometresPerHour: UInt8(tiltbackSpeed)) else { return }
        _ = model.setAeroTiltbackSpeed(setting)
    }

    private func sendPwmPercent() {
        guard let setting = AeroPwmPercent(percent: UInt8(pwmPercent)) else { return }
        _ = model.setAeroPwmPercent(setting)
    }

    private func disablePwm() {
        _ = model.setAeroPwmOff()
    }

    private func sendPedalHardness() {
        guard let setting = AeroPedalHardness(percent: UInt8(pedalHardnessPercent)) else { return }
        _ = model.setAeroPedalHardness(setting)
    }

    private func sendAlarmSpeed() {
        guard let setting = AeroSpeedSetting(kilometresPerHour: UInt8(alarmSpeed)) else { return }
        _ = model.setAeroAlarmSpeed(setting)
    }

    private func sendAngleAdjustment() {
        guard let setting = AeroAngleAdjustment(tenthsOfDegree: Int8(angleTenths)) else { return }
        _ = model.setAeroAngleAdjustment(setting)
    }

}

struct AeroSettingsFormValues: Equatable {
    let tiltbackSpeed: Int
    let pwmPercent: Int
    let pedalHardnessPercent: Int
    let alarmSpeed: Int
    let angleTenths: Int
    let tiltback: AeroSpeedSetting?
    let pwm: AeroPwmPercent?
    let pedalHardness: AeroPedalHardness?
    let alarm: AeroSpeedSetting?
    let angle: AeroAngleAdjustment?

    init(
        tiltback: AeroSpeedSetting?,
        pwm: AeroPwmPercent?,
        pedalHardness: AeroPedalHardness? = nil,
        alarm: AeroSpeedSetting?,
        angle: AeroAngleAdjustment?
    ) {
        self.tiltback = tiltback
        self.pwm = pwm
        self.pedalHardness = pedalHardness
        self.alarm = alarm
        self.angle = angle
        tiltbackSpeed = Int(tiltback?.kilometresPerHour ?? 20)
        pwmPercent = Int(pwm?.percent ?? 60)
        pedalHardnessPercent = Int(pedalHardness?.percent ?? 60)
        alarmSpeed = Int(alarm?.kilometresPerHour ?? 20)
        angleTenths = Int(angle?.tenthsOfDegree ?? 0)
    }

    @MainActor
    init(model: CutoutAppModel) {
        self.init(
            tiltback: model.aeroTiltbackSpeedState?.current,
            pwm: model.aeroPwmPercentState?.current,
            pedalHardness: model.aeroPedalHardnessState?.current,
            alarm: model.aeroAlarmSpeedState?.current,
            angle: model.aeroAngleAdjustmentState?.current
        )
    }
}

private struct EucAeroAdditionalSettingsControls: View {
    let model: CutoutAppModel

    var body: some View {
        Section {
            if model.aeroWheelUnitsControlAvailable {
                EucAeroWheelUnitsControl(model: model)
            }
            if model.aeroDisplayBacklightControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.display_backlight.title"),
                    id: "aeroDisplayBacklight",
                    range: 0...100,
                    currentValue: model.aeroDisplayBacklightState?.current.map { Int($0.percent) },
                    unit: .percent
                ) { value in
                    guard let setting = AeroDisplayBacklight(percent: UInt8(value)) else { return }
                    _ = model.setAeroDisplayBacklight(setting)
                }
            }
            if model.aeroBeeperVolumeControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.beeper_volume.title"),
                    id: "aeroBeeperVolume",
                    range: 0...100,
                    currentValue: model.aeroBeeperVolumeState?.current.map { Int($0.percent) },
                    unit: .percent
                ) { value in
                    guard let setting = AeroBeeperVolume(percent: UInt8(value)) else { return }
                    _ = model.setAeroBeeperVolume(setting)
                }
            }
            if model.aeroDynamicAssistControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.dynamic_assist.title"),
                    id: "aeroDynamicAssist",
                    range: 0...100,
                    currentValue: model.aeroDynamicAssistState?.current.map { Int($0.percent) },
                    unit: .percent
                ) { value in
                    guard let setting = AeroDynamicAssist(percent: UInt8(value)) else { return }
                    _ = model.setAeroDynamicAssist(setting)
                }
            }
            if model.aeroPedalDipCompensationControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.pedal_dip_compensation.title"),
                    id: "aeroPedalDipCompensation",
                    range: 0...100,
                    currentValue: model.aeroPedalDipCompensationState?.current.map { Int($0.percent) },
                    unit: .percent
                ) { value in
                    guard let setting = AeroPedalDipCompensation(percent: UInt8(value)) else { return }
                    _ = model.setAeroPedalDipCompensation(setting)
                }
            }
            if model.aeroLateralTiltLimitControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.lateral_tilt_limit.title"),
                    id: "aeroLateralTiltLimit",
                    range: 35...75,
                    currentValue: model.aeroLateralTiltLimitState?.current.map { Int($0.degrees) },
                    unit: .degrees
                ) { value in
                    guard let setting = AeroLateralTiltLimit(degrees: UInt8(value)) else { return }
                    _ = model.setAeroLateralTiltLimit(setting)
                }
            }
            if model.aeroVoltageCorrectionControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.voltage_correction.title"),
                    id: "aeroVoltageCorrection",
                    range: -15...15,
                    currentValue: model.aeroVoltageCorrectionState?.current.map { Int($0.tenthsOfPercent) },
                    unit: .tenthsOfPercent
                ) { value in
                    guard let setting = AeroVoltageCorrection(tenthsOfPercent: Int8(value)) else { return }
                    _ = model.setAeroVoltageCorrection(setting)
                }
            }
            if model.aeroMaxChargeVoltageRawControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.max_charge_raw.title"),
                    id: "aeroMaxChargeVoltageRaw",
                    range: 0...70,
                    currentValue: model.aeroMaxChargeVoltageRawState?.current.map { Int($0.raw) },
                    unit: .raw
                ) { value in
                    guard let setting = AeroMaxChargeVoltageRaw(raw: UInt8(value)) else { return }
                    _ = model.setAeroMaxChargeVoltageRaw(setting)
                }
            }
            if model.aeroHighSpeedModeControlAvailable {
                EucAeroToggleControl(
                    title: localizedAppText("settings.aero.high_speed_mode.title"),
                    id: "aeroHighSpeedMode",
                    currentValue: model.aeroHighSpeedModeState?.current,
                    send: { _ = model.setAeroHighSpeedMode(AeroToggle(enabled: $0)) }
                )
            }
            if model.aeroLowBatteryModeControlAvailable {
                EucAeroToggleControl(
                    title: localizedAppText("settings.aero.low_battery_mode.title"),
                    id: "aeroLowBatteryMode",
                    currentValue: model.aeroLowBatteryModeState?.current,
                    send: { _ = model.setAeroLowBatteryMode(AeroToggle(enabled: $0)) }
                )
            }
            if model.aeroTransportModeControlAvailable {
                EucAeroToggleControl(
                    title: localizedAppText("settings.aero.transport_mode.title"),
                    id: "aeroTransportMode",
                    currentValue: model.aeroTransportModeState?.current,
                    send: { _ = model.setAeroTransportMode(AeroToggle(enabled: $0)) }
                )
            }
            if model.aeroRidingModeControlAvailable {
                EucAeroRidingModeControl(model: model)
            }
            if model.aeroBrakeOverpressureAlarmControlAvailable {
                EucAeroNumericSettingControl(
                    title: localizedAppText("settings.aero.brake_overpressure.title"),
                    id: "aeroBrakeOverpressureAlarm",
                    range: 90...125,
                    currentValue: model.aeroBrakeOverpressureAlarmState?.current.map { Int($0.percent) },
                    unit: .percent
                ) { value in
                    guard let setting = AeroBrakeOverpressureAlarm(percent: UInt8(value)) else { return }
                    _ = model.setAeroBrakeOverpressureAlarm(setting)
                }
            }
            if model.aeroGyroCalibrationControlAvailable {
                VStack(alignment: .leading, spacing: 8) {
                    let state = model.aeroGyroCalibrationState?.current
                    Button(localizedAppText(
                        state == .complete
                            ? "settings.aero.gyro_calibration.stop"
                            : "settings.aero.gyro_calibration.title"
                    )) {
                        _ = model.setAeroGyroCalibration()
                    }
                    .accessibilityIdentifier("settings.control.aeroGyroCalibration")
                    .disabled(state == .waiting)
                    if let state {
                        Text(localizedAppText("settings.aero.gyro_calibration.\(state.localizationKey)"))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        } header: {
            Text(localizedAppText("settings.aero.additional.title"))
        } footer: {
            Text(localizedAppText("settings.aero.footer"))
        }
        .disabled(model.phase != .live)
    }
}

private struct EucAeroRidingModeControl: View {
    let model: CutoutAppModel
    @State private var draftValue: AeroRidingMode?

    private var selectedValue: AeroRidingMode { draftValue ?? model.aeroRidingModeState?.current ?? .medium }

    var body: some View {
        Picker(localizedAppText("settings.aero.riding_mode.title"), selection: Binding(
            get: { selectedValue },
            set: { draftValue = $0 }
        )) {
            ForEach(AeroRidingMode.allCases, id: \.self) { mode in
                Text(localizedAppText("settings.aero.riding_mode.\(mode.localizationKey)"))
                    .tag(mode)
            }
        }
        .pickerStyle(.menu)
        Button(localizedAppText("settings.aero.send")) {
            _ = model.setAeroRidingMode(selectedValue)
        }
        .accessibilityIdentifier("settings.control.aeroRidingMode")
    }
}

private extension AeroRidingMode {
    var localizationKey: String {
        switch self {
        case .hard: "hard"
        case .medium: "medium"
        case .soft: "soft"
        }
    }
}

private extension AeroGyroCalibrationState {
    var localizationKey: String {
        switch self {
        case .idle: "idle"
        case .waiting: "waiting"
        case .complete: "complete"
        }
    }
}

private struct EucAeroToggleControl: View {
    let title: String
    let id: String
    let currentValue: Bool?
    let send: (Bool) -> Void
    @State private var draftValue: Bool?

    private var selectedValue: Bool { draftValue ?? currentValue ?? false }

    var body: some View {
        Toggle(title, isOn: Binding(
            get: { selectedValue },
            set: { draftValue = $0 }
        ))
        Button(localizedAppText("settings.aero.send")) {
            send(selectedValue)
        }
        .accessibilityIdentifier("settings.control.\(id)")
    }
}

private struct EucAeroWheelUnitsControl: View {
    let model: CutoutAppModel
    @State private var draftValue: AeroWheelUnits?

    private var selectedValue: AeroWheelUnits? { draftValue ?? model.aeroWheelUnitsState?.current }

    var body: some View {
        Picker(localizedAppText("settings.aero.wheel_units.title"), selection: Binding(
            get: { selectedValue },
            set: { draftValue = $0 }
        )) {
            Text(localizedAppText("settings.readback.unavailable")).tag(AeroWheelUnits?.none)
            Text(localizedAppText("settings.aero.wheel_units.metric")).tag(AeroWheelUnits?.some(.metric))
            Text(localizedAppText("settings.aero.wheel_units.imperial")).tag(AeroWheelUnits?.some(.imperial))
        }
        .pickerStyle(.menu)
        Button(localizedAppText("settings.aero.send")) {
            guard let selectedValue else { return }
            _ = model.setAeroWheelUnits(selectedValue)
        }
        .disabled(selectedValue == nil)
        .accessibilityIdentifier("settings.control.aeroWheelUnits")
    }
}

private struct EucAeroNumericSettingControl: View {
    let title: String
    let id: String
    let range: ClosedRange<Int>
    let currentValue: Int?
    let unit: EucNumericSettingUnit
    let send: (Int) -> Void
    @State private var draftValue: Int?

    private var selectedValue: Int { draftValue ?? currentValue ?? range.lowerBound }

    var body: some View {
        Stepper(
            value: Binding(get: { selectedValue }, set: { draftValue = $0 }),
            in: range
        ) {
            Text("\(title): \(unit.text(selectedValue))")
        }
        Button(localizedAppText("settings.aero.send")) {
            send(selectedValue)
        }
        .accessibilityIdentifier("settings.control.\(id)")
        EucSettingReadbackRow(
            id: id,
            title: localizedAppText("settings.aero.current_value"),
            value: currentValue.map(unit.text) ?? localizedAppText("settings.readback.unavailable")
        )
    }
}

enum EucNumericSettingUnit {
    case percent
    case degrees
    case tenthsOfPercent
    case raw

    func text(_ value: Int) -> String {
        switch self {
        case .percent: "\(value)%"
        case .degrees: "\(value)°"
        case .tenthsOfPercent: String(format: "%.1f%%", Double(value) / 10)
        case .raw: "(value)"
        }
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

private struct EucRollAngleControl: View {
    let model: CutoutAppModel
    @State private var selectedAngle: RollAngle.Kind = .medium

    private static let angles: [RollAngle.Kind] = [.low, .medium, .high]

    var body: some View {
        Picker(
            localizedAppText("settings.roll_angle.title"),
            selection: Binding(
                get: { model.rollAngleState?.current ?? readbackAngle ?? selectedAngle },
                set: {
                    selectedAngle = $0
                    _ = model.setRollAngle($0)
                }
            )
        ) {
            ForEach(Self.angles, id: \.self) { angle in
                Text(localizedAppText("settings.roll_angle.\(angle.localizationKey)"))
                    .tag(angle)
            }
        }
        .pickerStyle(.menu)
        .disabled(model.phase != .live)
        .accessibilityHint(localizedAppText("settings.roll_angle.footer"))
        .accessibilityIdentifier("settings.control.rollAngle")
    }

    private var readbackAngle: RollAngle.Kind? {
        model.settingsReadback?.eucGarageSettings.rollAngle.value?.documentedKind
    }
}

private struct EucSpeedAlarmModeControl: View {
    let model: CutoutAppModel
    @State private var selectedMode: SpeedAlarmMode.Kind = .both

    private static let modes: [SpeedAlarmMode.Kind] = [.both, .stageOneOnly]

    var body: some View {
        Picker(
            localizedAppText("settings.speed_alarm_mode.title"),
            selection: Binding(
                get: { model.speedAlarmModeState?.current ?? readbackMode ?? selectedMode },
                set: {
                    selectedMode = $0
                    _ = model.setSpeedAlarmMode($0)
                }
            )
        ) {
            ForEach(Self.modes, id: \.self) { mode in
                Text(localizedAppText("settings.speed_alarm_mode.\(mode.localizationKey)"))
                    .tag(mode)
            }
        }
        .pickerStyle(.menu)
        .disabled(model.phase != .live)
        .accessibilityHint(localizedAppText("settings.speed_alarm_mode.footer"))
        .accessibilityIdentifier("settings.control.speedAlarmMode")
    }

    private var readbackMode: SpeedAlarmMode.Kind? {
        model.settingsReadback?.eucGarageSettings.speedAlarmMode.value?.documentedKind
    }
}

private struct EucBegodeMaxSpeedControl: View {
    let model: CutoutAppModel
    @State private var selectedSpeed = 30

    var body: some View {
        Stepper(
            value: Binding(
                get: { selectedSpeed },
                set: { newValue in
                    if let speed = BegodeMaxSpeed(kilometresPerHour: UInt8(newValue)) {
                        let result = model.setBegodeMaxSpeed(speed)
                        if case .accepted = result { selectedSpeed = newValue }
                    }
                }
            ),
            in: 0...99
        ) {
            Text("\(localizedAppText("settings.begode_max_speed.title")): \(selectedSpeed) km/h")
        }
        .disabled(model.phase != .live)
        .accessibilityIdentifier("settings.control.begodeMaxSpeed")
    }
}

private struct EucBegodeBeeperVolumeControl: View {
    let model: CutoutAppModel
    @State private var selectedVolume = 5

    var body: some View {
        Picker(localizedAppText("settings.begode_beeper_volume.title"), selection: Binding(
            get: { selectedVolume },
            set: { newValue in
                if let volume = BegodeBeeperVolume(level: UInt8(newValue)) {
                    let result = model.setBegodeBeeperVolume(volume)
                    if case .accepted = result { selectedVolume = newValue }
                }
            }
        )) {
            ForEach(1...9, id: \.self) { value in
                Text("\(value)").tag(value)
            }
        }
        .pickerStyle(.menu)
        .disabled(model.phase != .live)
        .accessibilityIdentifier("settings.control.begodeBeeperVolume")
    }
}

private struct EucBegodeLedModeControl: View {
    let model: CutoutAppModel
    @State private var selectedMode = 0

    var body: some View {
        Picker(localizedAppText("settings.begode_led_mode.title"), selection: Binding(
            get: { selectedMode },
            set: { newValue in
                if let mode = BegodeLedMode(mode: UInt8(newValue)) {
                    let result = model.setBegodeLedMode(mode)
                    if case .accepted = result { selectedMode = newValue }
                }
            }
        )) {
            ForEach(0...9, id: \.self) { value in
                Text("\(value)").tag(value)
            }
        }
        .pickerStyle(.menu)
        .disabled(model.phase != .live)
        .accessibilityIdentifier("settings.control.begodeLedMode")
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

private extension RollAngle.Kind {
    var localizationKey: String {
        switch self {
        case .low: "low"
        case .medium: "medium"
        case .high: "high"
        }
    }
}

private extension SpeedAlarmMode.Kind {
    var localizationKey: String {
        switch self {
        case .both: "both"
        case .stageOneOnly: "stage_one_only"
        case .off: "off"
        case .pwmTiltback: "pwm_tiltback"
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

    static func rollAngle(
        _ state: RollAngleSettingState?,
        fallback readback: ReadbackValue<RollAngle>
    ) -> String {
        if let state, let value = state.current {
            return value.displayName
        }
        guard let value = readback.value else {
            return readback.availability == .unsupported
                ? localizedAppText("settings.readback.unsupported")
                : localizedAppText("settings.readback.unavailable")
        }
        return value.documentedKind?.displayName ?? value.rawAngle.map(String.init) ?? "—"
    }

    static func speedAlarmMode(
        _ state: SpeedAlarmModeSettingState?,
        fallback readback: ReadbackValue<SpeedAlarmMode>
    ) -> String {
        if let state, let value = state.current {
            return value.displayName
        }
        guard let value = readback.value else {
            return readback.availability == .unsupported
                ? localizedAppText("settings.readback.unsupported")
                : localizedAppText("settings.readback.unavailable")
        }
        return value.documentedKind?.displayName ?? value.rawMode.map(String.init) ?? "—"
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

    static func tripDistance(_ millimetres: UInt64?) -> String {
        guard let millimetres else {
            return localizedAppText("settings.readback.unavailable")
        }
        let unit = RideUnits.distanceUnit(forSpeedUnit: RideUnits.speedUnit)
        let value = RideUnits.distanceText(
            millimetres: millimetres,
            unit: unit,
            fractionDigits: 1
        )
        return "\(value) \(unit)"
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

enum EucTripMeterResetPresentation {
    static func statusText(_ state: TripMeterResetState?) -> String? {
        switch state?.kind {
        case .pending:
            localizedAppText("settings.trip_meter.pending")
        case .confirmed:
            localizedAppText("settings.trip_meter.confirmed")
        case .timedOut:
            localizedAppText("settings.trip_meter.timed_out")
        case .failed:
            localizedAppText("settings.trip_meter.failed")
        case .refused:
            switch state?.refusalReason {
            case .missingArm:
                localizedAppText("settings.trip_meter.refused.stationary")
            case .expiredArm:
                localizedAppText("settings.trip_meter.refused.expired")
            case .busy:
                localizedAppText("settings.trip_meter.refused.busy")
            default:
                localizedAppText("settings.trip_meter.refused")
            }
        case .unknown, .current, nil:
            nil
        }
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
            persistence.markConfirmed()
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
            persistence.markUnconfirmed()
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
