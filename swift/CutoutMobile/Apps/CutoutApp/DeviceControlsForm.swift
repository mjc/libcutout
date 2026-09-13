import CutoutMobile
import CutoutMobileFFI
import Foundation
import SwiftUI

/// One native renderer for the shared semantic catalog. Drafts belong to each
/// attempt-scoped row; reported and requested values remain Rust snapshots.
struct DeviceControlsForm: View {
    let snapshot: DeviceControlsSnapshot
    let submitSetting: (ConnectionAttemptToken, DeviceSettingID, DeviceSettingValue) throws -> Void
    let submitAction: (ConnectionAttemptToken, DeviceActionID) throws -> Void

    var body: some View {
        Form {
            ForEach(snapshot.settingDescriptors, id: \.id) { descriptor in
                DeviceSettingRow(
                    descriptor: descriptor,
                    state: snapshot.settings.first { $0.id == descriptor.id },
                    submit: { value in
                        guard let token = snapshot.connection.token else { throw DeviceSettingSubmissionError.ConnectionUnavailable }
                        try submitSetting(token, descriptor.id, value)
                    }
                )
            }
            ForEach(snapshot.actionDescriptors, id: \.id) { descriptor in
                DeviceActionRow(
                    descriptor: descriptor,
                    state: snapshot.actions.first { $0.id == descriptor.id },
                    submit: {
                        guard let token = snapshot.connection.token else { throw DeviceActionSubmissionError.ConnectionUnavailable }
                        try submitAction(token, descriptor.id)
                    }
                )
            }
        }
        .id(snapshot.connection.generation)
        .disabled(snapshot.connection.readiness != .verified || snapshot.connection.transport != .connected)
    }
}

private struct DeviceSettingRow: View {
    let descriptor: DeviceSettingDescriptor
    let state: DeviceSettingSnapshot?
    let submit: (DeviceSettingValue) throws -> Void
    @State private var draft: DeviceSettingValue?
    @State private var submissionError: String?

    var body: some View {
        Section {
            LabeledContent(localizedAppText("controls.current"), value: DeviceControlPresentation.value(state?.current, control: descriptor.control))
            if state?.current != nil, let age = state?.ageMs {
                Text(localizedAppText("controls.reported_age", Int64(clamping: age / 1_000)))
                    .foregroundStyle(.secondary)
            }
            if let requested = state?.requested {
                LabeledContent(localizedAppText("controls.requested"), value: DeviceControlPresentation.value(requested, control: descriptor.control))
            }
            if descriptor.access == .writable {
                Picker(localizedAppText("controls.new_value"), selection: $draft) {
                    Text(localizedAppText("controls.choose")).tag(nil as DeviceSettingValue?)
                    ForEach(DeviceControlPresentation.options(descriptor.control), id: \.self) { value in
                        Text(DeviceControlPresentation.value(value, control: descriptor.control))
                            .tag(Optional(value))
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier("settings.draft.\(descriptor.id)")
                Button(localizedAppText("controls.apply")) {
                    guard let draft else { return }
                    do {
                        try submit(draft)
                        self.draft = nil
                        submissionError = nil
                    } catch {
                        submissionError = DeviceControlPresentation.error(error)
                    }
                }
                .disabled(draft == nil)
                .accessibilityIdentifier("settings.apply.\(descriptor.id)")
            }
            if let submissionError {
                Text(submissionError).foregroundStyle(.red)
            }
            if let state, let status = DeviceControlPresentation.status(state.status) {
                Text(status).foregroundStyle(.secondary)
            }
            if let refusal = state?.refusal {
                Text(DeviceControlPresentation.refusal(refusal)).foregroundStyle(.secondary)
            }
        } header: {
            Text(localizedAppText(descriptor.labelKey))
                .accessibilityIdentifier("settings.control.\(descriptor.id)")
        } footer: {
            VStack(alignment: .leading) {
                if descriptor.access == .readOnly {
                    Text(localizedAppText("controls.read_only"))
                } else if descriptor.access == .unverified {
                    Text(localizedAppText("controls.unverified_setting"))
                }
                if case .number(_, _, _, _, .kilometresPerHour, _) = descriptor.control {
                    Text(localizedAppText("controls.speed_steps"))
                }
            }
        }
    }
}

private struct DeviceActionRow: View {
    let descriptor: DeviceActionDescriptor
    let state: DeviceActionSnapshot?
    let submit: () throws -> Void
    @State private var confirmsDestructiveAction = false
    @State private var submissionError: String?

    var body: some View {
        Section {
            Button(DeviceControlPresentation.actionTitle(descriptor: descriptor, state: state)) {
                if descriptor.role == .destructive {
                    confirmsDestructiveAction = true
                } else {
                    send()
                }
            }
            .disabled(!DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state))
            .accessibilityIdentifier("settings.action.\(descriptor.id)")
            if let submissionError {
                Text(submissionError).foregroundStyle(.red)
            }
            if let state, let text = DeviceControlPresentation.actionStatus(state.status) {
                Text(text).foregroundStyle(.secondary)
            }
            if let refusal = state?.refusal {
                Text(DeviceControlPresentation.refusal(refusal)).foregroundStyle(.secondary)
            }
        } header: {
            Text(localizedAppText(descriptor.labelKey))
        } footer: {
            Text(localizedAppText(descriptor.helpKey))
        }
        .confirmationDialog(localizedAppText(descriptor.labelKey), isPresented: $confirmsDestructiveAction) {
            Button(localizedAppText(descriptor.labelKey), role: .destructive, action: send)
        } message: {
            Text(localizedAppText(descriptor.helpKey))
        }
    }

    private func send() {
        do {
            try submit()
            submissionError = nil
        } catch {
            submissionError = DeviceControlPresentation.error(error)
        }
    }
}

enum DeviceControlPresentation {
    static func options(_ control: DeviceSettingControl) -> [DeviceSettingValue] {
        switch control {
        case .boolean:
            [.boolean(value: false), .boolean(value: true)]
        case let .number(minimum, maximum, step, _, _, canDisable):
            stride(from: minimum, through: maximum, by: Int(step)).map { .number(value: $0) }
                + (canDisable ? [.disabled] : [])
        case let .choices(choices):
            choices.filter(\.writable).map { .choice(id: $0.id) }
        case .readOnly:
            []
        }
    }

    static func value(_ value: DeviceSettingValue?, control: DeviceSettingControl) -> String {
        guard let value else { return localizedAppText("settings.readback.unavailable") }
        switch value {
        case .disabled:
            return localizedAppText("settings.choice.off")
        case let .boolean(value):
            return localizedAppText(value ? "controls.on" : "settings.choice.off")
        case let .choice(id):
            guard case let .choices(choices) = control,
                  let choice = choices.first(where: { $0.id == id }) else {
                return localizedAppText("settings.readback.unavailable")
            }
            return localizedAppText(choice.labelKey)
        case let .number(value):
            guard case let .number(_, _, _, precision, unit, _) = control else { return String(value) }
            let amount = Double(value) / pow(10, Double(precision))
            let text = amount.formatted(.number.precision(.fractionLength(Int(precision))))
            switch unit {
            case .pwmDutyPercent: return localizedAppText("controls.pwm_duty.value", text)
            case .percent: return "\(text)%"
            case .kilometresPerHour:
                let speed = SpeedReadout(millimetersPerSecond: Int32((amount / 3.6 * 1_000).rounded()))
                return "\(speed.displayValue) \(speed.displayUnit)"
            case .degrees: return "\(text)°"
            case .level: return text
            case .seconds: return "\(text) s"
            case .minutes: return "\(text) min"
            }
        }
    }

    static func status(_ status: DeviceSettingStatus) -> String? {
        switch status {
        case .idle: nil
        case .waitingForConfirmation: localizedAppText("settings.state.pending")
        case .sentWithoutConfirmation: localizedAppText("controls.sent_unconfirmed")
        case .confirmed: localizedAppText("settings.state.confirmed")
        case .refused: localizedAppText("settings.state.refused")
        case .timedOut: localizedAppText("settings.state.timed_out")
        case .failed: localizedAppText("settings.state.failed")
        }
    }

    static func actionAvailable(descriptor: DeviceActionDescriptor, state: DeviceActionSnapshot?) -> Bool {
        guard descriptor.access == .available, case .available = state?.nextStep else { return false }
        return true
    }

    static func actionTitle(descriptor: DeviceActionDescriptor, state: DeviceActionSnapshot?) -> String {
        switch state?.nextStep {
        case .available(.prepareGyroCalibration): localizedAppText("controls.prepare")
        case .available(.startGyroCalibration): localizedAppText("controls.start")
        case .available(.invoke): localizedAppText(descriptor.labelKey)
        case .busy: localizedAppText("controls.waiting")
        case .restartRequired: localizedAppText("controls.restart")
        case nil: localizedAppText(descriptor.labelKey)
        }
    }

    static func actionStatus(_ status: DeviceActionStatus) -> String? {
        switch status {
        case .idle: nil
        case .waitingForProgress: localizedAppText("controls.waiting")
        case .readyForNextStep: localizedAppText("controls.ready")
        case .sentWithoutConfirmation: localizedAppText("controls.sent_unconfirmed")
        case .refused: localizedAppText("settings.state.refused")
        case .failed: localizedAppText("settings.state.failed")
        }
    }

    static func refusal(_ refusal: CutoutMobileFFI.MobileControlRefusalReasonDto) -> String {
        switch refusal {
        case .missingArm: localizedAppText("controls.refusal.stationary")
        case .expiredArm: localizedAppText("controls.refusal.expired")
        case .busy: localizedAppText("controls.refusal.busy")
        case .wrongModel: localizedAppText("controls.refusal.model")
        case .unsupportedCommand: localizedAppText("controls.refusal.unsupported")
        case .currentLimitExceeded: localizedAppText("controls.refusal.limit")
        case .wrongSafetyClass: localizedAppText("controls.refusal.state")
        }
    }

    static func error(_ error: Error) -> String {
        switch error {
        case DeviceSettingSubmissionError.ConnectionUnavailable, DeviceActionSubmissionError.ConnectionUnavailable:
            localizedAppText("controls.error.connection")
        case DeviceSettingSubmissionError.Unverified, DeviceActionSubmissionError.Unverified:
            localizedAppText("controls.error.unverified")
        case DeviceSettingSubmissionError.ReadOnly: localizedAppText("controls.error.read_only")
        case DeviceSettingSubmissionError.InvalidValue: localizedAppText("controls.error.value")
        case DeviceActionSubmissionError.Busy: localizedAppText("controls.refusal.busy")
        case DeviceActionSubmissionError.RestartRequired: localizedAppText("controls.error.restart")
        case DeviceSettingSubmissionError.Unavailable, DeviceActionSubmissionError.Unavailable:
            localizedAppText("controls.error.unavailable")
        case DeviceActionSubmissionError.InvalidStep: localizedAppText("controls.error.step")
        default: localizedAppText("controls.error.send")
        }
    }
}
