import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

/// Incomplete native text and focus stay local. Rust admits and retains each snapshot.
@MainActor
@Observable
final class CaptureResearchDraft {
    var profileID = ""
    var source = MobileCaptureObservationSourceDto.manufacturerApp
    var values: [String: String] = [:]
    var units: [String: String] = [:]
    var states: [String: String] = [:]
    var note = ""
    var earlierTime = ""
    var useEarlierTime = false
    var error: String?

    func reset() {
        profileID = ""
        values.removeAll()
        units.removeAll()
        states.removeAll()
        note = ""
        earlierTime = ""
        useEarlierTime = false
        error = nil
    }

    var catalog: MobileCaptureResearchCatalogDto? { captureResearchCatalog(profileId: profileID) }

    func selectProfile(_ profile: String) {
        guard profile != profileID else { return }
        profileID = profile
        values.removeAll()
        units.removeAll()
        states.removeAll()
        error = nil
    }

    func controlSelection(for fieldID: String) -> String {
        switch states[fieldID] {
        case "unobserved", "true", "false": return states[fieldID] ?? ""
        default: return values[fieldID] ?? ""
        }
    }

    func selectControlValue(_ selection: String, for fieldID: String) {
        switch selection {
        case "unobserved", "true", "false":
            states[fieldID] = selection
            values.removeValue(forKey: fieldID)
        default:
            states[fieldID] = "value"
            values[fieldID] = selection
        }
    }

    func observation(locale: Locale = .current) throws -> MobileCaptureSettingsDraftDto {
        guard let catalog else { throw DraftError.profile }
        let observedAt: UInt64?
        if useEarlierTime {
            let decimalSeparator = locale.decimalSeparator ?? "."
            let normalizedTime = earlierTime.replacingOccurrences(of: decimalSeparator, with: ".")
            guard let seconds = Double(normalizedTime), seconds.isFinite, seconds >= 0,
                seconds <= Double(UInt64.max / 1000)
            else { throw DraftError.time }
            let milliseconds = seconds * 1000
            guard milliseconds < Double(UInt64.max) else { throw DraftError.time }
            observedAt = UInt64(milliseconds)
        } else {
            observedAt = nil
        }
        let reported = catalog.fields.compactMap { field -> MobileCaptureSettingObservationValueDto? in
            let state = states[field.id] ?? "value"
            let text = values[field.id] ?? ""
            let value: MobileCaptureObservedValueDto
            switch state {
            case "off": value = .off
            case "true": value = .boolean(value: true)
            case "false": value = .boolean(value: false)
            case "unobserved": value = .unobserved
            default:
                guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
                value = .entered(value: text)
            }
            let unit = units[field.id] ?? CaptureResearchPresentation.defaultUnit(field)
            return MobileCaptureSettingObservationValueDto(
                settingId: field.id, value: value, unit: unit.isEmpty ? nil : unit, note: nil
            )
        }
        return MobileCaptureSettingsDraftDto(
            profile: MobileCaptureResearchProfileDto(
                profileId: catalog.profileId, pevCategory: catalog.pevCategory, model: catalog.model,
                firmware: nil, selection: .manual
            ), observedAtMs: observedAt, source: source, values: reported, note: note.isEmpty ? nil : note
        )
    }

    enum DraftError: LocalizedError {
        case profile, time
        var errorDescription: String? {
            switch self {
            case .profile: localizedAppText("capture.settings.profile_required")
            case .time: localizedAppText("capture.settings.invalid_time")
            }
        }
    }
}

enum CaptureResearchPresentation {
    static func error(_ error: Error) -> String {
        guard let error = error as? MobileCaptureResearchError else { return error.localizedDescription }
        switch error {
        case .InvalidObservation(let message): return message
        case .NotRecording: return localizedAppText("capture.settings.not_recording")
        case .AdmissionLost: return localizedAppText("capture.settings.not_saved")
        case .WriterFailed: return localizedAppText("capture.settings.save_unconfirmed")
        }
    }
    static func source(_ source: MobileCaptureObservationSourceDto) -> String {
        switch source {
        case .manufacturerApp: localizedAppText("capture.settings.source.manufacturer_app")
        case .wheelDisplay: localizedAppText("capture.settings.source.wheel_display")
        case .other: localizedAppText("capture.settings.source.other")
        }
    }
    static func time(_ milliseconds: UInt64) -> String {
        let seconds = milliseconds / 1000
        return String(format: "%02llu:%02llu", seconds / 60, seconds % 60)
    }
    static func group(_ group: MobileSettingGroupDto) -> String {
        switch group {
        case .limits: localizedAppText("controls.group.limits")
        case .riding: localizedAppText("controls.group.riding")
        case .interface: localizedAppText("controls.group.interface")
        case .modes: localizedAppText("capture.settings.group.modes")
        case .diagnostics: localizedAppText("capture.settings.group.other_observations")
        case .haptics: localizedAppText("controls.group.haptics")
        case .lighting: localizedAppText("controls.group.lighting")
        case .hardware: localizedAppText("controls.group.hardware")
        case .remote: localizedAppText("controls.group.remote")
        case .battery: localizedAppText("controls.group.battery")
        }
    }
    static func defaultUnit(_ field: MobileCaptureResearchFieldDto) -> String {
        if let unit = field.unitHint { return unit }
        guard case let .number(_, _, _, _, _, unit, _) = field.control else {
            return field.id == "displayed_pack_voltage" ? "V" : ""
        }
        switch unit {
        case .percent, .pwmDutyPercent: return "%"
        case .kilometresPerHour: return "km/h"
        case .degrees: return "°"
        case .level: return "level"
        case .seconds: return "s"
        case .minutes: return "min"
        case .volts: return "V"
        case .amperes: return "A"
        case .hertz: return "Hz"
        case .degreesPerSecond: return "°/s"
        case .celsius: return "°C"
        case .scalar, .raw: return ""
        case .watts: return "W"
        case .milliseconds: return "ms"
        case .revolutionsPerMinute: return "rpm"
        case .millimetres: return "mm"
        case .ampHours: return "Ah"
        }
    }
    static func value(_ value: MobileCaptureSettingObservationValueDto, field: MobileCaptureResearchFieldDto? = nil)
        -> String
    {
        let text: String
        switch value.value {
        case .unobserved: text = localizedAppText("capture.settings.not_observed")
        case .off: text = localizedAppText("settings.choice.off")
        case .boolean(let enabled): text = localizedAppText(enabled ? "controls.on" : "settings.choice.off")
        case .entered(let entered):
            if let field, case let .choices(choices) = field.control,
                let choice = choices.first(where: { String($0.id) == entered })
            {
                text = localizedAppText(choice.labelKey)
            } else {
                text = entered
            }
        }
        guard case .entered = value.value else { return text }
        return value.unit.map { "\(text) \($0)" } ?? text
    }
    static func decoded(_ value: MobileSettingValueDto, field: MobileCaptureResearchFieldDto?) -> String {
        switch value {
        case .disabled: return localizedAppText("settings.choice.off")
        case .boolean(let enabled): return localizedAppText(enabled ? "controls.on" : "settings.choice.off")
        case .choice(let id):
            if let field, case let .choices(choices) = field.control,
                let choice = choices.first(where: { $0.id == id })
            {
                return localizedAppText(choice.labelKey)
            }
            return String(id)
        case .number(let raw):
            guard let field, case let .number(_, _, _, _, precision, _, _) = field.control else { return String(raw) }
            let amount = Double(raw) / pow(10, Double(precision))
            let text = amount.formatted(.number.precision(.fractionLength(Int(precision))))
            let unit = defaultUnit(field)
            return unit.isEmpty ? text : "\(text) \(unit)"
        }
    }
}

struct CaptureSettingsWorksheet: View {
    @Bindable var draft: CaptureResearchDraft
    let save: (MobileCaptureSettingsDraftDto) async throws -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var isSaving = false
    @State private var search = ""
    @FocusState private var focusedField: String?
    private let profiles = captureResearchCatalogs()
    private let groups: [MobileSettingGroupDto] = [.limits, .interface, .riding, .modes, .diagnostics]

    var body: some View {
        Form {
            Section {
                Text("Settings values").font(.title2.bold())
                if let catalog = draft.catalog { LabeledContent("PEV type", value: catalog.pevCategory.uppercased()) }
                Picker("Device profile", selection: Binding(get: { draft.profileID }, set: draft.selectProfile)) {
                    Text("Choose profile").tag("")
                    ForEach(profiles, id: \.profileId) { profile in
                        Text(profile.model).tag(profile.profileId)
                    }
                }.accessibilityIdentifier("capture.settings.profile")
                Picker("Read from", selection: $draft.source) {
                    Text("Manufacturer app").tag(MobileCaptureObservationSourceDto.manufacturerApp)
                    Text("Wheel display").tag(MobileCaptureObservationSourceDto.wheelDisplay)
                    Text("Other").tag(MobileCaptureObservationSourceDto.other)
                }.accessibilityIdentifier("capture.settings.source")
                Toggle("Earlier observation", isOn: $draft.useEarlierTime)
                if draft.useEarlierTime {
                    HStack {
                        TextField("Observed at", text: $draft.earlierTime)
                            #if os(iOS)
                                .keyboardType(.decimalPad)
                            #endif
                            .focused($focusedField, equals: "observation-time")
                            .accessibilityIdentifier("capture.settings.time")
                        Text("s from start").foregroundStyle(.secondary)
                    }
                } else {
                    LabeledContent("Observed at", value: localizedAppText("capture.settings.now"))
                }
            }
            if let catalog = draft.catalog {
                ForEach(groups, id: \.self) { group in
                    let fields = catalog.fields.filter { $0.group == group && !$0.researchOnly && matches($0) }
                    if !fields.isEmpty {
                        Section(CaptureResearchPresentation.group(group)) {
                            ForEach(fields, id: \.id) { field in fieldRow(field) }
                        }
                    }
                }
                let research = catalog.fields.filter { $0.researchOnly && matches($0) }
                if !research.isEmpty {
                    Section {
                        DisclosureGroup("Unverified protocol fields") {
                            ForEach(research, id: \.id) { field in fieldRow(field) }
                        }
                    }
                }
                Section("Note") {
                    TextField("Optional", text: $draft.note, axis: .vertical)
                        .focused($focusedField, equals: "observation-note")
                        .lineLimit(2...5).accessibilityIdentifier("capture.settings.note")
                }
            }
            if let error = draft.error {
                Section {
                    Text(error).foregroundStyle(PevColors.warningText).accessibilityIdentifier("capture.settings.error")
                }
            }
        }
        .disabled(isSaving)
        .searchable(text: $search, prompt: "Find a setting")
        .scrollDismissesKeyboard(.interactively)
        .navigationTitle("Record settings")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(isSaving) }
            ToolbarItemGroup(placement: .keyboard) {
                Spacer()
                Button("Done") { focusedField = nil }
                    .accessibilityIdentifier("capture.settings.keyboard.done")
            }
        }
        .safeAreaInset(edge: .bottom) {
            Button("Save values") {
                focusedField = nil
                guard !isSaving else { return }
                isSaving = true
                Task { @MainActor in
                    defer { isSaving = false }
                    do {
                        try await save(draft.observation())
                        draft.error = nil
                        dismiss()
                    } catch { draft.error = CaptureResearchPresentation.error(error) }
                }
            }
            .disabled(isSaving)
            .buttonStyle(.borderedProminent).tint(PevColors.yellow).foregroundStyle(.black)
            .frame(maxWidth: .infinity, minHeight: 44).padding()
            .background(PevColors.pageBackground)
            .accessibilityIdentifier("capture.settings.save")
        }
        .tint(PevColors.yellow)
        .interactiveDismissDisabled(isSaving)
        .accessibilityIdentifier("capture.settings.worksheet")
    }

    private func matches(_ field: MobileCaptureResearchFieldDto) -> Bool {
        search.isEmpty || localizedAppText(field.labelKey).localizedCaseInsensitiveContains(search)
    }

    @ViewBuilder private func fieldRow(_ field: MobileCaptureResearchFieldDto) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(localizedAppText(field.labelKey)).font(.body)
            switch field.control {
            case .boolean:
                Picker(localizedAppText(field.labelKey), selection: controlSelection(field.id)) {
                    Text("Unknown").tag("")
                    Text("Not observed").tag("unobserved")
                    Text("On").tag("true")
                    Text("Off").tag("false")
                }.labelsHidden()
            case .choices(let choices):
                Picker(localizedAppText(field.labelKey), selection: controlSelection(field.id)) {
                    Text("Unknown").tag("")
                    Text("Not observed").tag("unobserved")
                    ForEach(choices, id: \.id) { choice in
                        Text(localizedAppText(choice.labelKey)).tag(String(choice.id))
                    }
                }.labelsHidden()
            case .number(_, _, _, _, _, _, let canDisable):
                valueStateControl(field, canDisable: canDisable)
                if draft.states[field.id] != "off" && draft.states[field.id] != "unobserved" { entry(field) }
            case .readOnly:
                valueStateControl(field, canDisable: false)
                if draft.states[field.id] != "unobserved" { entry(field) }
            }
        }
        .padding(.vertical, 4)
        .accessibilityIdentifier("capture.settings.field.\(field.id)")
    }
    private func valueStateControl(_ field: MobileCaptureResearchFieldDto, canDisable: Bool) -> some View {
        Picker("Value", selection: state(field.id, default: "value")) {
            Text("Value").tag("value")
            if canDisable { Text("Off").tag("off") }
            Text("Not observed").tag("unobserved")
        }.pickerStyle(.segmented)
    }
    private func entry(_ field: MobileCaptureResearchFieldDto) -> some View {
        HStack {
            TextField("Unknown", text: value(field.id)).multilineTextAlignment(.trailing)
                .focused($focusedField, equals: field.id)
                .accessibilityLabel(localizedAppText(field.labelKey))
                .accessibilityIdentifier("capture.settings.value.\(field.id)")
            if CaptureResearchPresentation.defaultUnit(field) == "km/h" {
                Picker("Unit", selection: unit(field)) {
                    Text("km/h").tag("km/h")
                    Text("mph").tag("mph")
                    Text("Unknown unit").tag("")
                }.labelsHidden().fixedSize().accessibilityLabel("\(localizedAppText(field.labelKey)) unit")
            } else if !CaptureResearchPresentation.defaultUnit(field).isEmpty {
                Text(CaptureResearchPresentation.defaultUnit(field)).foregroundStyle(.secondary)
            }
        }
    }
    private func controlSelection(_ id: String) -> Binding<String> {
        Binding(get: { draft.controlSelection(for: id) }, set: { draft.selectControlValue($0, for: id) })
    }
    private func value(_ id: String) -> Binding<String> {
        Binding(get: { draft.values[id] ?? "" }, set: { draft.values[id] = $0 })
    }
    private func state(_ id: String, default fallback: String) -> Binding<String> {
        Binding(get: { draft.states[id] ?? fallback }, set: { draft.states[id] = $0 })
    }
    private func unit(_ field: MobileCaptureResearchFieldDto) -> Binding<String> {
        Binding(
            get: { draft.units[field.id] ?? CaptureResearchPresentation.defaultUnit(field) },
            set: { draft.units[field.id] = $0 })
    }
}

struct CaptureSettingsObservationReview: View {
    let observations: [MobileCaptureSettingsObservationDto]
    let readback: ([MobileCaptureResearchReadbackRequestDto]) async throws -> [MobileCaptureResearchReadbackDto]
    @State private var decoded: [MobileCaptureResearchReadbackDto] = []
    @State private var isLoading = true
    @State private var error: String?
    @State private var loadID = UUID()
    private var observationKeys: [String] {
        observations.map { "\($0.captureId):\($0.profile.profileId):\($0.observedAtMs):\($0.recordedAtMs)" }
    }
    var body: some View {
        List {
            if observations.isEmpty { Text("No settings values recorded.").foregroundStyle(.secondary) }
            ForEach(Array(observations.enumerated()), id: \.offset) { index, observation in
                let title =
                    index == 0
                    ? localizedAppText("capture.settings.baseline")
                    : localizedAppText("capture.settings.observation_number", index + 1)
                Section(
                    localizedAppText(
                        "capture.settings.observation_title", title,
                        CaptureResearchPresentation.time(observation.observedAtMs))
                ) {
                    CaptureSettingsObservationRows(
                        observation: observation,
                        decoded: decoded.indices.contains(index) ? decoded[index].values : [],
                        isLoading: isLoading, error: error
                    )
                }
            }
        }
        .navigationTitle("Settings observations")
        .accessibilityIdentifier("capture.settings.review")
        .task(id: observationKeys) { await loadReadback() }
    }
    private func loadReadback() async {
        let requestID = UUID()
        loadID = requestID
        isLoading = true
        error = nil
        decoded = []
        defer { if loadID == requestID { isLoading = false } }
        do {
            let requests = observations.map {
                MobileCaptureResearchReadbackRequestDto(profileId: $0.profile.profileId, atMs: $0.observedAtMs)
            }
            let result = try await readback(requests)
            guard loadID == requestID, !Task.isCancelled else { return }
            decoded = result
        } catch {
            guard loadID == requestID, !Task.isCancelled else { return }
            self.error = localizedAppText("capture.settings.read_error")
        }
    }
}

private struct CaptureSettingsObservationRows: View {
    let observation: MobileCaptureSettingsObservationDto
    let decoded: [MobileCaptureResearchDecodedValueDto]
    let isLoading: Bool
    let error: String?
    private var catalog: MobileCaptureResearchCatalogDto? {
        captureResearchCatalog(profileId: observation.profile.profileId)
    }
    private var unusual: [String] { captureResearchUnusualFields(observation: observation) }
    var body: some View {
        LabeledContent("Device profile", value: observation.profile.model ?? observation.profile.profileId)
        LabeledContent("Read from", value: CaptureResearchPresentation.source(observation.source))
        LabeledContent("Entered at", value: CaptureResearchPresentation.time(observation.recordedAtMs))
        ForEach(observation.values, id: \.settingId) { value in
            let field = catalog?.fields.first(where: { $0.id == value.settingId })
            let decodedValue = decoded.first(where: { $0.settingId == value.settingId })
            VStack(alignment: .leading, spacing: 8) {
                Text(field.map { localizedAppText($0.labelKey) } ?? value.settingId).font(.headline)
                LabeledContent("You recorded", value: CaptureResearchPresentation.value(value, field: field))
                Text(
                    "\(CaptureResearchPresentation.source(observation.source)) · \(CaptureResearchPresentation.time(observation.observedAtMs))"
                )
                .font(.caption).foregroundStyle(.secondary)
                if isLoading {
                    ProgressView("Reading captured packets…").font(.caption)
                } else if let error {
                    Text(error).font(.caption).foregroundStyle(PevColors.warningText)
                } else if let decodedValue {
                    if let reported = decodedValue.value {
                        LabeledContent(
                            "CutOut decoded", value: CaptureResearchPresentation.decoded(reported, field: field))
                        if captureResearchValuesDiffer(
                            profileId: observation.profile.profileId, reported: value, decoded: reported) == true
                        {
                            Text("Values differ.").font(.caption).foregroundStyle(PevColors.warningText)
                        }
                    } else {
                        LabeledContent("CutOut decoded", value: localizedAppText("settings.empty"))
                    }
                    Text("Packet · \(CaptureResearchPresentation.time(decodedValue.observedAtMs))")
                        .font(.caption).foregroundStyle(.secondary)
                } else {
                    LabeledContent("CutOut decoded", value: localizedAppText("capture.settings.not_available"))
                        .foregroundStyle(.secondary)
                }
                if unusual.contains(value.settingId) {
                    Text("Outside the usual range.").font(.caption).foregroundStyle(PevColors.warningText)
                }
            }.padding(.vertical, 4)
        }
        if let note = observation.note, !note.isEmpty { Text(note) }
    }

}

struct CaptureSavedSettingsSection: View {
    let source: MobileCaptureSubmissionSourceDto
    @State private var observations: [MobileCaptureSettingsObservationDto] = []
    @State private var error: String?
    var body: some View {
        Section {
            if let error { Text(error).foregroundStyle(PevColors.warningText) }
            if !observations.isEmpty {
                NavigationLink {
                    CaptureSettingsObservationReview(observations: observations) { requests in
                        let database = try await RustPersistenceStore.open()
                        let selectedSource = source
                        return try await Task.detached {
                            try database.captureResearchReadbackBatch(source: selectedSource, requests: requests)
                        }.value
                    }
                } label: {
                    LabeledContent("Settings observations", value: observations.count.formatted())
                }.accessibilityIdentifier("capture.settings.saved")
            }
        }
        .task {
            do {
                let database = try await RustPersistenceStore.open()
                let selectedSource = source
                observations = try await Task.detached {
                    try database.captureSettingsObservations(source: selectedSource)
                }.value
                error = nil
            } catch { self.error = localizedAppText("capture.settings.load_error") }
        }
    }
}
