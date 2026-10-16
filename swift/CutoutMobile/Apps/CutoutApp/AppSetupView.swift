import CutoutMobile
import SwiftUI

#if canImport(UIKit)
    import UIKit
#endif

struct AppSetupView: View {
    let model: CutoutAppModel
    let onAccountDeleted: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var showsSoundCloud = false
    @State private var confirmsAccountDeletion = false
    @State private var accountDeletion: ChargingCommunityAccountDeletion
    @State private var path: [Destination]

    private enum Destination: Hashable {
        case music
        case captures
    }

    init(
        model: CutoutAppModel,
        opensMusic: Bool = false,
        accountDeletion: ChargingCommunityAccountDeletion = .shared,
        onAccountDeleted: @escaping () -> Void
    ) {
        self.model = model
        self.onAccountDeleted = onAccountDeleted
        _accountDeletion = State(initialValue: accountDeletion)
        _path = State(initialValue: opensMusic ? [.music] : [])
    }

    var body: some View {
        NavigationStack(path: $path) {
            Form {
                Section {
                    if let enabled = model.rideAutostartEnabled {
                        Toggle(
                            localizedAppText("setup.ride_autostart"),
                            isOn: Binding(
                                get: { model.rideAutostartEnabled ?? enabled },
                                set: { value in
                                    Task { await model.setRideAutostartEnabled(value) }
                                }
                            )
                        )
                        .disabled(model.rideAutostartSettingsBusy)
                        .accessibilityIdentifier("setup.ride-autostart")
                        .accessibilityHint(localizedAppText("setup.ride_autostart.hint"))
                    } else if !model.rideAutostartSettingsError {
                        LabeledContent(localizedAppText("setup.ride_autostart")) {
                            ProgressView()
                        }
                    }
                    if model.rideAutostartSettingsError {
                        Text(
                            localizedAppText(
                                model.rideAutostartEnabled == nil
                                    ? "setup.ride_autostart.load_failed"
                                    : "setup.ride_autostart.save_failed")
                        )
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("setup.ride-autostart.error")
                        if model.rideAutostartEnabled == nil {
                            Button(localizedAppText("setup.ride_autostart.retry")) {
                                Task { await model.loadRideAutostartSetting() }
                            }
                            .disabled(model.rideAutostartSettingsBusy)
                        }
                    }
                }
                Section {
                    NavigationLink(value: Destination.music) {
                        Label(pevLocalizedText("music.settings.title"), systemImage: "music.note")
                    }
                    .accessibilityIdentifier("setup.music")
                }
                if model.device.hasSavedDevice {
                    Section(localizedAppText("setup.device")) {
                        Button(localizedAppText("picker.saved_device.forget"), role: .destructive) {
                            Task { @MainActor in await model.forgetSavedDevice() }
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
                Section(localizedAppText("setup.account.section")) {
                    switch accountDeletion.state {
                    case .loading:
                        LabeledContent(localizedAppText("setup.account.status")) {
                            ProgressView()
                        }
                    case .noAccount:
                        Text(localizedAppText("setup.account.no_account"))
                            .foregroundStyle(.secondary)
                    case .active:
                        Text(localizedAppText("setup.account.active"))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        if let error = accountDeletion.lastError {
                            Text(error).font(.footnote).foregroundStyle(.red)
                        }
                    case .registrationUncertain:
                        Text(localizedAppText("setup.account.registration_uncertain"))
                            .foregroundStyle(.secondary)
                    case .deleting:
                        LabeledContent(localizedAppText("setup.account.deleting")) {
                            ProgressView()
                        }
                    case .deletionPending:
                        Text(localizedAppText("setup.account.retry_description"))
                            .foregroundStyle(.red)
                        if let error = accountDeletion.lastError {
                            Text(error).font(.footnote).foregroundStyle(.red)
                        }
                        Button(localizedAppText("setup.account.retry"), role: .destructive) {
                            submitAccountDeletion()
                        }
                        .accessibilityIdentifier("setup.account.retry")
                    case .deleted:
                        Label(
                            localizedAppText("setup.account.deleted"),
                            systemImage: "checkmark.circle.fill"
                        )
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("setup.account.deleted")
                    case .failed:
                        Text(localizedAppText("setup.account.load_failed"))
                            .foregroundStyle(.red)
                        if let error = accountDeletion.lastError {
                            Text(error).font(.footnote).foregroundStyle(.red)
                        }
                        Button(localizedAppText("setup.account.reload")) {
                            Task { await accountDeletion.refresh() }
                        }
                        .accessibilityIdentifier("setup.account.reload")
                    }
                    if accountDeletion.state == .active {
                        Button(localizedAppText("setup.account.delete"), role: .destructive) {
                            confirmsAccountDeletion = true
                        }
                        .accessibilityIdentifier("setup.account.delete")
                    }
                }
            }
            .task { await model.loadRideAutostartSetting() }
            .task { await accountDeletion.refresh() }
            .formStyle(.grouped)
            .navigationTitle(localizedAppText("picker.section.setup"))
            .accessibilityIdentifier("setup.screen")
            .toolbar { doneToolbar }
            .navigationDestination(for: Destination.self) { destination in
                switch destination {
                case .captures:
                    BluetoothCapturesView(model: model)
                        .toolbar { doneToolbar }
                case .music:
                    let music = model.music
                    MusicSettingsView(
                        nowPlaying: music.selectedProvider == .soundCloud ? music.nowPlaying : music.settingsNowPlaying,
                        selectedProvider: Binding(
                            get: { music.selectedProvider },
                            set: music.selectProvider
                        ),
                        historyPolicy: Binding(
                            get: { music.preferredHistoryPolicy },
                            set: { policy in
                                Task { @MainActor in
                                    _ = await music.setHistoryPolicyAsync(policy)
                                }
                            }
                        ),
                        historyUnavailable: music.historyUnavailable,
                        historySaveError: music.historySaveError,
                        historyFailureContext: music.historyFailureContext,
                        onConnect: music.connect,
                        onAuthorizeSpotify: music.authorizeSpotify,
                        onPlaySoundCloud: { showsSoundCloud = true },
                        onOpenProvider: {
                            Task { @MainActor in
                                _ = await music.handleCommand(.openProvider)
                            }
                        }
                    )
                    .toolbar { doneToolbar }
                }
            }
        }
        .sheet(isPresented: $showsSoundCloud) {
            NavigationStack {
                SoundCloudPlayerView(player: model.music.nativeSoundCloud)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button(pevLocalizedText("music.done")) { showsSoundCloud = false }
                                .accessibilityIdentifier("music.soundcloud.done")
                        }
                    }
            }
        }
        .sheet(isPresented: $confirmsAccountDeletion) {
            NavigationStack {
                Form {
                    Section {
                        Text(localizedAppText("setup.account.confirm_message"))
                        Text(localizedAppText("setup.account.before_delete"))
                    }
                    Section {
                        Text(localizedAppText("setup.account.local_data"))
                        Text(localizedAppText("setup.account.retention"))
                    }
                    Section {
                        Button(localizedAppText("setup.account.confirm_delete"), role: .destructive) {
                            confirmsAccountDeletion = false
                            submitAccountDeletion()
                        }
                        .accessibilityIdentifier("setup.account.confirm-delete")
                    }
                }
                .navigationTitle(localizedAppText("setup.account.confirm_title"))
                .accessibilityIdentifier("setup.account.confirmation")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button(localizedAppText("common.cancel")) {
                            confirmsAccountDeletion = false
                        }
                        .accessibilityIdentifier("setup.account.cancel-delete")
                    }
                }
            }
        }
        .tint(PevDashboardColors.yellow)
    }

    @ToolbarContentBuilder
    private var doneToolbar: some ToolbarContent {
        ToolbarItem(placement: .confirmationAction) {
            Button {
                dismiss()
            } label: {
                Text(pevLocalizedText("music.done"))
                    .frame(minWidth: 44, minHeight: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("setup.done")
        }
    }

    private func submitAccountDeletion() {
        Task {
            await accountDeletion.deleteAccount()
            if accountDeletion.state == .deleted {
                onAccountDeleted()
            }
        }
    }
}

struct PhoneRideAlarmSettingsView: View {
    let model: CutoutAppModel
    @Environment(\.openURL) private var openURL

    var body: some View {
        Form {
            if let settings = model.phoneAlarmSettings {
                let deviceIdentity = settings.deviceIdentity
                Section(
                    model.rideHistory.vehicleNames[deviceIdentity]
                        ?? model.device.rideMapVehicleName(for: deviceIdentity)
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
                        in: 1...100
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
        .navigationTitle(localizedAppText("controls.alarms.title"))
        .accessibilityIdentifier("settings.screen.alarms")
    }
}
