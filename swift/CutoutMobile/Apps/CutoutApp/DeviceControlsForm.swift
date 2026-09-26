import CutoutMobile
import CutoutMobileFFI
import Foundation
import SwiftUI

/// One native renderer for the shared semantic catalog. Drafts belong to each
/// attempt-scoped row; reported and requested values remain Rust snapshots.
struct DeviceControlsForm: View {
    let snapshot: DeviceSettings
    let submitSetting: (ConnectionAttemptToken, DeviceSettingID, DeviceSettingValue) throws -> Void
    let submitAction: (ConnectionAttemptToken, DeviceActionID) throws -> Void

    private var canInteract: Bool {
        snapshot.connection.readiness == .verified
            && snapshot.connection.transport == .connected
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text(localizedAppText("controls.tune.title")).font(.largeTitle.bold())
                if !snapshot.settingDescriptors.contains(where: { $0.access == .writable })
                    && !snapshot.actionDescriptors.contains(where: { $0.access == .available })
                {
                    Text(localizedAppText("controls.empty"))
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("settings.empty")
                }
                ForEach(DeviceControlPresentation.groups, id: \.self) { group in
                    let descriptors = DeviceControlPresentation.settings(snapshot.settingDescriptors, in: group)
                    if !descriptors.isEmpty {
                        controlsCard(title: DeviceControlPresentation.groupTitle(group)) {
                            settingRows(descriptors)
                        }
                    }
                }
                let actions = snapshot.actionDescriptors.filter { $0.access != .unverified }
                if !actions.isEmpty {
                    controlsCard(title: localizedAppText("controls.actions")) {
                        actionRows(actions)
                    }
                }
            }
            .padding(24)
        }
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
        .tint(PevColors.yellow)
        .id(snapshot.connection.generation)
    }

    private func controlsCard(title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline).accessibilityAddTraits(.isHeader)
            VStack(alignment: .leading, spacing: 0, content: content)
                .padding(.horizontal, 16)
                .background(PevDashboardCardBackground(cornerRadius: 22))
        }
    }

    private func settingRows(_ descriptors: [DeviceSettingDescriptor]) -> some View {
        ForEach(descriptors, id: \.id) { descriptor in
            VStack(spacing: 0) {
                if descriptor.id != descriptors.first?.id { Divider() }
                DeviceSettingRow(
                    descriptor: descriptor,
                    state: snapshot.setting(for: descriptor.id),
                    isEnabled: canInteract
                ) { value in
                    guard let token = snapshot.connection.token else {
                        throw DeviceSettingSubmissionError.ConnectionUnavailable
                    }
                    try submitSetting(token, descriptor.id, value)
                }
                .padding(.vertical, 12)
            }
        }
    }

    private func actionRows(_ descriptors: [DeviceActionDescriptor]) -> some View {
        ForEach(descriptors, id: \.id) { descriptor in
            DeviceActionRow(
                descriptor: descriptor,
                state: snapshot.actions.first { $0.id == descriptor.id },
                isEnabled: canInteract
            ) {
                guard let token = snapshot.connection.token else {
                    throw DeviceActionSubmissionError.ConnectionUnavailable
                }
                try submitAction(token, descriptor.id)
            }
            .padding(.vertical, 12)
        }
    }
}

private struct DeviceSettingRow: View {
    let descriptor: DeviceSettingDescriptor
    let state: DeviceSettingSnapshot?
    let isEnabled: Bool
    let submit: (DeviceSettingValue) throws -> Void
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var editing = DeviceSettingDraft()

    private var draft: DeviceSettingValue? { editing.value }

    private var displayedValue: String {
        DeviceControlPresentation.value(draft ?? state?.current, control: descriptor.control)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            switch descriptor.control {
            case .boolean:
                ViewThatFits(in: .horizontal) {
                    if !dynamicTypeSize.isAccessibilitySize {
                        HStack(spacing: 12) {
                            titleAndValue.fixedSize(horizontal: true, vertical: false)
                            Spacer(minLength: 0)
                            booleanButtons
                        }
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        titleAndValue
                        booleanButtons
                    }
                }
            case .choices:
                ViewThatFits(in: .horizontal) {
                    HStack {
                        title.fixedSize(horizontal: true, vertical: false)
                        Spacer(minLength: 8)
                        choicePicker
                    }
                    VStack(alignment: .leading) {
                        title
                        choicePicker
                    }
                }
                applyButton
            case let .number(minimum, maximum, step, _, _, canDisable):
                titleAndValue
                let layout =
                    dynamicTypeSize.isAccessibilitySize
                    ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8))
                    : AnyLayout(HStackLayout(spacing: 12))
                layout {
                    if minimum < maximum, step > 0 {
                        Slider(
                            value: Binding(
                                get: {
                                    if case let .number(value) = draft ?? state?.current {
                                        return Double(min(max(value, minimum), maximum))
                                    }
                                    return Double(minimum)
                                },
                                set: { value in
                                    edit(.number(value: Int32(value.rounded())))
                                }), in: Double(minimum)...Double(maximum), step: Double(step)
                        )
                        .accessibilityLabel(localizedAppText(descriptor.labelKey))
                        .accessibilityValue(displayedValue)
                        .accessibilityIdentifier("settings.slider.\(descriptor.id)")
                        .disabled(!hasNumericBaseValue)
                    }
                    numericStepper
                }
                HStack(spacing: 12) {
                    if canDisable {
                        Button {
                            edit(.disabled)
                        } label: {
                            Text(localizedAppText("settings.choice.off")).frame(minWidth: 44, minHeight: 44)
                        }
                        .buttonStyle(.borderless)
                        .accessibilityIdentifier("settings.disable.\(descriptor.id)")
                    }
                    applyButton
                }
            case .readOnly:
                titleAndValue
            }
            if let feedback = DeviceControlPresentation.feedback(
                state,
                submissionError: editing.submissionError,
                draft: editing.value
            ) {
                Text(feedback.text).font(.footnote)
                    .foregroundStyle(feedback.isError ? PevColors.red : PevColors.muted)
                    .accessibilityIdentifier("settings.\(feedback.isError ? "error" : "status").\(descriptor.id)")
            }
        }
        .disabled(!isEnabled)
        .onChange(of: state) { editing.reconcile(state) }
    }

    private var title: some View {
        Text(localizedAppText(descriptor.labelKey))
            .font(.body.weight(.semibold))
            .accessibilityIdentifier("settings.control.\(descriptor.id)")
    }

    private var titleAndValue: some View {
        HStack(alignment: .firstTextBaseline) {
            title
            Spacer(minLength: 8)
            valueSummary
        }
    }

    private var valueSummary: some View {
        VStack(alignment: .trailing, spacing: 2) {
            if let draft {
                labeledValue(
                    editing.label(state),
                    draft,
                    identifier: "settings.draft.\(descriptor.id)"
                )
            }
            if let current = state?.current {
                labeledValue(
                    DeviceControlPresentation.sourceLabel(state?.currentSource),
                    current,
                    identifier: "settings.current.\(descriptor.id)"
                )
            } else {
                labeledValue(
                    DeviceControlPresentation.sourceLabel(nil),
                    nil,
                    identifier: "settings.current.\(descriptor.id)"
                )
            }
        }
        .accessibilityElement(children: .contain)
    }

    private func labeledValue(
        _ label: String,
        _ value: DeviceSettingValue?,
        identifier: String
    ) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
            Text(DeviceControlPresentation.value(value, control: descriptor.control))
                .monospacedDigit()
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(identifier)
    }

    private var booleanButtons: some View {
        HStack(spacing: 8) {
            booleanButton(false)
            booleanButton(true)
        }
    }

    private var choicePicker: some View {
        Menu {
            ForEach(DeviceControlPresentation.options(descriptor.control), id: \.self) { value in
                Button(DeviceControlPresentation.value(value, control: descriptor.control)) { edit(value) }
                    .accessibilityIdentifier("settings.choice.\(descriptor.id).\(value)")
            }
        } label: {
            HStack(spacing: 6) {
                Text(displayedValue)
                Image(systemName: "chevron.up.chevron.down").accessibilityHidden(true)
            }
            .frame(minWidth: 44, minHeight: 44)
        }
        .accessibilityLabel(localizedAppText(descriptor.labelKey))
        .accessibilityValue(displayedValue)
        .accessibilityIdentifier("settings.picker.\(descriptor.id)")
    }

    private var numericStepper: some View {
        Stepper {
            Text(localizedAppText(descriptor.labelKey))
        } onIncrement: {
            edit(
                DeviceControlPresentation.steppedValue(
                    draft: draft, current: state?.current, control: descriptor.control, increasing: true))
        } onDecrement: {
            edit(
                DeviceControlPresentation.steppedValue(
                    draft: draft, current: state?.current, control: descriptor.control, increasing: false))
        }
        .labelsHidden()
        .fixedSize(horizontal: true, vertical: false)
        .frame(minHeight: 44)
        .accessibilityLabel(localizedAppText(descriptor.labelKey))
        .accessibilityValue(displayedValue)
        .accessibilityIdentifier("settings.stepper.\(descriptor.id)")
        .disabled(!hasNumericBaseValue)
    }

    private var hasNumericBaseValue: Bool {
        editing.value != nil || state?.current != nil
    }

    private func booleanButton(_ value: Bool) -> some View {
        let selected =
            DeviceControlPresentation.booleanSelection(state, submissionError: editing.submissionError) == value
        return Button {
            send(.boolean(value: value))
        } label: {
            Text(DeviceControlPresentation.value(.boolean(value: value), control: .boolean))
                .font(.body.weight(.semibold))
                .padding(.horizontal, 8)
                .frame(minWidth: 44, minHeight: 44)
                .background(selected ? PevColors.yellow.opacity(0.2) : .clear, in: .rect(cornerRadius: 10))
        }
        .buttonStyle(.borderless)
        .accessibilityLabel(
            localizedAppText(
                "controls.boolean.action", localizedAppText(descriptor.labelKey),
                DeviceControlPresentation.value(.boolean(value: value), control: .boolean))
        )
        .accessibilityAddTraits(selected ? [.isSelected] : [])
        .accessibilityValue(
            selected
                ? localizedAppText(
                    state?.current == .boolean(value: value)
                        ? "controls.accessibility.reported" : "controls.accessibility.requested") : ""
        )
        .accessibilityIdentifier("settings.\(value ? "on" : "off").\(descriptor.id)")
    }

    @ViewBuilder private var applyButton: some View {
        if editing.canApply(state) {
            Button {
                guard let draft else { return }
                send(draft)
            } label: {
                Text(localizedAppText("controls.apply")).frame(minWidth: 44, minHeight: 44)
            }
            .buttonStyle(.borderless)
            .accessibilityIdentifier("settings.apply.\(descriptor.id)")
        }
    }

    private func send(_ value: DeviceSettingValue) {
        editing.send(value, submit: submit)
        editing.reconcile(state)
    }

    private func edit(_ value: DeviceSettingValue?) {
        editing.edit(value)
    }
}

/// Local editing feedback only; the snapshot owns command and transport outcomes.
struct DeviceSettingDraft {
    private(set) var value: DeviceSettingValue?
    private(set) var submittedValue: DeviceSettingValue?
    private(set) var submissionError: String?
    private var failedValue: DeviceSettingValue?

    mutating func edit(_ value: DeviceSettingValue?) {
        self.value = value
        submittedValue = nil
        submissionError = nil
        failedValue = nil
    }

    mutating func send(_ value: DeviceSettingValue, submit: (DeviceSettingValue) throws -> Void) {
        do {
            try submit(value)
            submittedValue = value
            submissionError = nil
            failedValue = nil
        } catch {
            submittedValue = nil
            submissionError = DeviceControlPresentation.error(error)
            failedValue = value
        }
    }

    mutating func reconcile(_ state: DeviceSettingSnapshot?) {
        guard let current = state?.current else { return }
        if value == current {
            edit(nil)
        } else if failedValue == current {
            submissionError = nil
            failedValue = nil
        }
    }

    func label(_ state: DeviceSettingSnapshot?) -> String {
        let accepted =
            value != nil && value == submittedValue && value == state?.requested
            && state?.status != .idle && state?.status != .refused && state?.transport != .rejected
        return localizedAppText(accepted ? "settings.value.requested" : "settings.value.draft")
    }

    func canApply(_ state: DeviceSettingSnapshot?) -> Bool {
        DeviceControlPresentation.hasChanges(draft: value, current: state?.current)
            && (value != submittedValue || state?.status == .failed || state?.status == .refused
                || state?.status == .timedOut || state?.transport == .rejected)
    }
}

private struct DeviceActionRow: View {
    let descriptor: DeviceActionDescriptor
    let state: DeviceActionSnapshot?
    let isEnabled: Bool
    let submit: () throws -> Void
    @State private var confirmsDestructiveAction = false
    @State private var submissionError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button(DeviceControlPresentation.actionTitle(descriptor: descriptor, state: state)) {
                if descriptor.role == .destructive {
                    confirmsDestructiveAction = true
                } else {
                    send()
                }
            }
            .disabled(
                !isEnabled
                    || !DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state)
            )
            .accessibilityIdentifier("settings.action.\(descriptor.id)")
            if let submissionError {
                Text(submissionError).foregroundStyle(.red)
            } else if let refusal = state?.refusal {
                Text(DeviceControlPresentation.refusal(refusal)).foregroundStyle(PevColors.red)
            } else if let state, let text = DeviceControlPresentation.actionStatus(state.status) {
                Text(text).foregroundStyle(.secondary)
            }
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
    static let groups: [DeviceSettingGroup] = [.interface, .limits, .riding, .modes, .diagnostics]

    static func groupTitle(_ group: DeviceSettingGroup) -> String {
        switch group {
        case .interface: localizedAppText("controls.group.interface")
        case .limits: localizedAppText("controls.group.limits")
        case .riding: localizedAppText("controls.group.riding")
        case .modes: localizedAppText("controls.group.modes")
        case .diagnostics: localizedAppText("controls.group.diagnostics")
        }
    }

    static func settings(_ descriptors: [DeviceSettingDescriptor], in group: DeviceSettingGroup)
        -> [DeviceSettingDescriptor]
    {
        descriptors.filter { $0.group == group && $0.access == .writable }.sorted { $0.order < $1.order }
    }

    static func hasChanges(draft: DeviceSettingValue?, current: DeviceSettingValue?) -> Bool {
        draft != nil && draft != current
    }

    static func acceptsRequestedSelection(_ status: DeviceSettingStatus?) -> Bool {
        status == .waitingForConfirmation || status == .sentWithoutConfirmation
    }

    static func booleanSelection(_ state: DeviceSettingSnapshot?, submissionError: String? = nil) -> Bool? {
        if submissionError == nil, state?.transport != .rejected,
            acceptsRequestedSelection(state?.status), case let .boolean(value) = state?.requested
        {
            return value
        }
        if case let .boolean(value) = state?.current { return value }
        return nil
    }

    /// Adjust a local draft on the descriptor's fixed-point lattice. Readback
    /// can seed an edit, but never becomes a draft before a user interaction.
    static func steppedValue(
        draft: DeviceSettingValue?, current: DeviceSettingValue?, control: DeviceSettingControl, increasing: Bool
    ) -> DeviceSettingValue? {
        guard case let .number(minimum, maximum, step, _, _, _) = control,
            minimum <= maximum, step > 0
        else { return nil }
        // Unknown wheel state is not a default. Do not manufacture the
        // minimum as a first draft from a +/− tap. Explicit Off is a known
        // value, so it can step back onto the numeric lattice.
        let source = draft ?? current
        let value: Int32
        switch source {
        case let .number(number):
            value = number
        case .disabled:
            return .number(value: minimum)
        case .boolean, .choice, nil:
            return nil
        }
        let lower = Int64(minimum)
        let stride = Int64(step)
        let lastIndex = (Int64(maximum) - lower) / stride
        let offset = Int64(value) - lower
        let index: Int64
        if increasing {
            index = offset < 0 ? 0 : offset / stride + 1
        } else {
            index = offset <= 0 ? 0 : (offset - 1) / stride
        }
        return .number(value: Int32(lower + min(max(index, 0), lastIndex) * stride))
    }

    static func options(_ control: DeviceSettingControl) -> [DeviceSettingValue] {
        switch control {
        case .boolean:
            [.boolean(value: false), .boolean(value: true)]
        case .number:
            []
        case let .choices(choices):
            choices.filter(\.writable).map { .choice(id: $0.id) }
        case .readOnly:
            []
        }
    }

    static func value(_ value: DeviceSettingValue?, control: DeviceSettingControl) -> String {
        guard let value else { return "—" }
        switch value {
        case .disabled:
            return localizedAppText("settings.choice.off")
        case let .boolean(value):
            return localizedAppText(value ? "controls.on" : "settings.choice.off")
        case let .choice(id):
            guard case let .choices(choices) = control,
                let choice = choices.first(where: { $0.id == id })
            else {
                return "—"
            }
            return localizedAppText(choice.labelKey)
        case let .number(value):
            guard case let .number(_, _, _, precision, unit, _) = control else { return String(value) }
            let amount = Double(value) / pow(10, Double(precision))
            let text = amount.formatted(.number.precision(.fractionLength(Int(precision))))
            switch unit {
            case .pwmDutyPercent, .percent: return "\(text)%"
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

    static func sourceLabel(_ source: DeviceSettingValueSource?) -> String {
        switch source {
        case .liveReadback: localizedAppText("settings.value.wheel")
        case .captureReplay: localizedAppText("settings.value.replay")
        case .userRequest: localizedAppText("settings.value.requested")
        case .unknown, nil: localizedAppText("settings.value.wheel")
        }
    }

    static func status(_ status: DeviceSettingStatus, transport: MobileSettingTransportStatusDto? = nil) -> String? {
        // Confirmation settles the row. Earlier host evidence must not linger.
        if status == .confirmed { return nil }
        if status == .refused { return localizedAppText("settings.state.refused") }
        if transport == .rejected { return localizedAppText("settings.transport.rejected") }
        if transport == .cancelled { return localizedAppText("settings.transport.cancelled") }
        if status == .failed { return localizedAppText("settings.state.failed") }
        switch transport {
        case .accepted: return localizedAppText("settings.transport.accepted")
        case .queued: return localizedAppText("settings.transport.queued")
        case .submitted, .rejected, .cancelled, nil: break
        }
        return switch status {
        case .idle, .confirmed: nil
        case .sentWithoutConfirmation: nil
        case .waitingForConfirmation: localizedAppText("settings.state.pending")
        case .refused: localizedAppText("settings.state.refused")
        case .timedOut: nil
        case .failed: localizedAppText("settings.state.failed")
        }
    }

    struct Feedback: Equatable {
        let text: String
        let isError: Bool
    }

    static func feedback(
        _ state: DeviceSettingSnapshot?,
        submissionError: String? = nil,
        draft: DeviceSettingValue? = nil
    ) -> Feedback? {
        if let submissionError { return Feedback(text: submissionError, isError: true) }
        guard let state else { return nil }
        if let draft, draft != state.requested { return nil }
        if state.status != .confirmed, let reason = state.refusal {
            return Feedback(text: refusal(reason), isError: true)
        }
        guard let text = status(state.status, transport: state.transport) else { return nil }
        let isError =
            state.status == .refused || state.status == .failed
            || state.transport == .rejected || state.transport == .cancelled
            || (state.status == .timedOut && state.transport != .accepted && state.transport != .queued)
        return Feedback(text: text, isError: isError)
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
        case .sentWithoutConfirmation: nil
        case .waitingForProgress: localizedAppText("controls.waiting")
        case .readyForNextStep: localizedAppText("controls.ready")
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
        case let CutoutSessionError.commandRefused(reason):
            switch reason {
            case .missingArm: refusal(.missingArm)
            case .expiredArm: refusal(.expiredArm)
            case .busy: refusal(.busy)
            case .wrongModel: refusal(.wrongModel)
            case .unsupportedCommand: refusal(.unsupportedCommand)
            case .currentLimitExceeded: refusal(.currentLimitExceeded)
            case .wrongSafetyClass: refusal(.wrongSafetyClass)
            case nil: localizedAppText("settings.state.refused")
            }
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
