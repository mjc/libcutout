import CutoutMobile
import SwiftUI
#if canImport(UIKit)
import UIKit
#endif

struct AppSetupView: View {
    let model: CutoutAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var path: [Destination]

    private enum Destination: Hashable {
        case music
        case phoneAlarms
        case captures
    }

    init(model: CutoutAppModel, opensMusic: Bool = false) {
        self.model = model
        _path = State(initialValue: opensMusic ? [.music] : [])
    }

    var body: some View {
        NavigationStack(path: $path) {
            Form {
                Section {
                    NavigationLink(value: Destination.phoneAlarms) {
                        LabeledContent {
                            Text(localizedAppText("phone_alarm.summary"))
                                .foregroundStyle(.secondary)
                        } label: {
                            Label(localizedAppText("phone_alarm.title"), systemImage: "iphone.radiowaves.left.and.right")
                        }
                    }
                    .accessibilityIdentifier("setup.phone-alarms")
                    NavigationLink(value: Destination.music) {
                        Label(pevLocalizedText("music.settings.title"), systemImage: "music.note")
                    }
                    .accessibilityIdentifier("setup.music")
                }
                if model.hasSavedDevice {
                    Section(localizedAppText("setup.device")) {
                        Button(localizedAppText("picker.saved_device.forget"), role: .destructive) {
                            model.forgetSavedDevice()
                        }
                        .accessibilityIdentifier("setup.forget-saved-device")
                    }
                }
                Section(localizedAppText("captures.diagnostics")) {
                    NavigationLink(value: Destination.captures) {
                        Label(localizedAppText("captures.title"), systemImage: "waveform.path")
                    }
                    .accessibilityIdentifier("setup.captures")
                }
            }
            .formStyle(.grouped)
            .navigationTitle(localizedAppText("picker.section.setup"))
            .accessibilityIdentifier("setup.screen")
            .toolbar { doneToolbar }
            .navigationDestination(for: Destination.self) { destination in
                switch destination {
                case .captures:
                    BluetoothCapturesView(model: model)
                case .music:
                    MusicSettingsView(
                        nowPlaying: model.musicSettingsNowPlaying,
                        selectedProvider: Binding(
                            get: { model.selectedMusicProvider },
                            set: model.selectMusicProvider
                        ),
                        historyPolicy: Binding(
                            get: { model.musicHistoryPolicy },
                            set: { _ = model.setMusicHistoryPolicy($0) }
                        ),
                        historyUnavailable: model.musicHistoryUnavailable,
                        historySaveError: model.musicHistorySaveError,
                        onConnect: model.connectMusic,
                        onAuthorizeSpotify: model.authorizeSpotify,
                        onOpenProvider: {
                            Task { @MainActor in
                                _ = await model.handleMusicCommand(.openProvider)
                            }
                        }
                    )
                    .toolbar { doneToolbar }
                case .phoneAlarms:
                    PhoneRideAlarmSettingsView(model: model)
                        .toolbar { doneToolbar }
                }
            }
        }
        .tint(PevDashboardColors.yellow)
    }

    @ToolbarContentBuilder
    private var doneToolbar: some ToolbarContent {
        ToolbarItem(placement: .confirmationAction) {
            Button(pevLocalizedText("music.done")) { dismiss() }
                .accessibilityIdentifier("setup.done")
        }
    }
}

private struct PhoneRideAlarmSettingsView: View {
    let model: CutoutAppModel
    @Environment(\.openURL) private var openURL

    var body: some View {
        Form {
            if let settings = model.phoneAlarmSettings {
                let deviceIdentity = settings.deviceIdentity
                Section(
                    model.phoneAlarmDeviceName
                        ?? localizedAppText("ride_map.vehicle_name_unavailable")
                ) {
                    Toggle(
                        localizedAppText("phone_alarm.enabled"),
                        isOn: Binding(
                            get: { settings.enabled },
                            set: { enabled in
                                Task {
                                    await model.setPhoneAlarmsEnabled(
                                        enabled,
                                        deviceIdentity: deviceIdentity
                                    )
                                }
                            }
                        )
                    )
                    .accessibilityIdentifier("phone-alarm.enabled")
                }

                Section {
                    Stepper(
                        value: Binding(
                            get: { Int(settings.pwmDutyPercent) },
                            set: {
                                model.setPhoneAlarmPwmDutyPercent(
                                    $0,
                                    deviceIdentity: deviceIdentity
                                )
                            }
                        ),
                        in: 1 ... 100
                    ) {
                        LabeledContent(
                            localizedAppText("phone_alarm.pwm_duty"),
                            value: "\(settings.pwmDutyPercent)%"
                        )
                    }
                    .accessibilityIdentifier("phone-alarm.pwm-duty")
                    LabeledContent(
                        localizedAppText("phone_alarm.pwm_headroom"),
                        value: "\(settings.pwmHeadroomPercent)%"
                    )
                    Text(
                        localizedAppText(
                            "phone_alarm.pwm_explanation",
                            Int64(settings.pwmDutyPercent),
                            Int64(settings.pwmHeadroomPercent)
                        )
                    )
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                } header: {
                    Text(localizedAppText("phone_alarm.pwm_section"))
                }

                Section(localizedAppText("phone_alarm.source_section")) {
                    Text(localizedAppText("phone_alarm.source_explanation"))
                }

                Section(localizedAppText("phone_alarm.notifications_section")) {
                    LabeledContent(
                        localizedAppText("phone_alarm.notifications_section"),
                        value: model.phoneAlarmAuthorizationText
                    )
                    switch model.phoneAlarmAuthorization {
                    case .notDetermined, .unavailable:
                        Button(localizedAppText("phone_alarm.request_permission")) {
                            Task { await model.requestPhoneAlarmAuthorization() }
                        }
                    case .denied, .permitted(alerts: false, sounds: _, quietly: _):
#if canImport(UIKit)
                        Button(localizedAppText("phone_alarm.open_settings")) {
                            openURL(URL(string: UIApplication.openSettingsURLString)!)
                        }
#endif
                    case .permitted:
                        EmptyView()
                    }
                    if let error = model.phoneAlarmDeliveryError {
                        Text(error)
                            .foregroundStyle(.red)
                    }
                }
            } else {
                ContentUnavailableView(
                    localizedAppText("phone_alarm.title"),
                    systemImage: "iphone.slash",
                    description: Text(localizedAppText("phone_alarm.no_device"))
                )
                if let error = model.phoneAlarmDeliveryError {
                    Text(error)
                        .foregroundStyle(.red)
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(localizedAppText("phone_alarm.title"))
    }
}
