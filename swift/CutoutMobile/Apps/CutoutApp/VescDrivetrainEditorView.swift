import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

typealias VescDrivetrainLoad =
    @MainActor @Sendable (ConnectionAttemptToken) async throws -> MobileVescDrivetrainDraft?
typealias VescDrivetrainSave =
    @MainActor @Sendable (ConnectionAttemptToken, MobileVescDrivetrainDraft) async throws -> MobileVescDrivetrainDraft

/// Text drafts for one captured connection; Rust owns every geometry decision.
@MainActor
@Observable
final class VescDrivetrainEditor: Identifiable {
    enum Field: String, CaseIterable, Hashable {
        case wheelDiameter = "wheel-diameter"
        case motorPolePairs = "motor-pole-pairs"
        case reduction

        var labelKey: String {
            switch self {
            case .wheelDiameter: "vesc.drivetrain.wheel_diameter"
            case .motorPolePairs: "vesc.drivetrain.motor_pole_pairs"
            case .reduction: "vesc.drivetrain.reduction"
            }
        }

        var errorKey: String {
            switch self {
            case .wheelDiameter: "vesc.drivetrain.error.wheel_diameter"
            case .motorPolePairs: "vesc.drivetrain.error.motor_pole_pairs"
            case .reduction: "vesc.drivetrain.error.reduction"
            }
        }
    }

    enum Failure: Equatable {
        case field(Field)
        case connection, unsupported, persistence, load

        var key: String {
            switch self {
            case .field(let field): field.errorKey
            case .connection: "vesc.drivetrain.error.connection"
            case .unsupported: "vesc.drivetrain.error.unsupported"
            case .persistence: "vesc.drivetrain.error.persistence"
            case .load: "vesc.drivetrain.error.load"
            }
        }
    }

    enum Stage: Equatable {
        case unloaded, loading, editing, saving
        case failed(Failure)
    }

    let id = UUID()
    let token: ConnectionAttemptToken
    var wheelDiameterMm = "" { didSet { if oldValue != wheelDiameterMm { clearFailure() } } }
    var motorPolePairs = "" { didSet { if oldValue != motorPolePairs { clearFailure() } } }
    var motorToWheelRatio = "" { didSet { if oldValue != motorToWheelRatio { clearFailure() } } }
    private(set) var stage = Stage.unloaded
    private(set) var hasLoaded = false
    @ObservationIgnored private let decimalSeparator: String
    @ObservationIgnored private let loadDraft: VescDrivetrainLoad
    @ObservationIgnored private let saveDraft: VescDrivetrainSave

    init(
        token: ConnectionAttemptToken, locale: Locale = .current,
        load: @escaping VescDrivetrainLoad, save: @escaping VescDrivetrainSave
    ) {
        self.token = token
        self.decimalSeparator = locale.decimalSeparator ?? "."
        self.loadDraft = load
        self.saveDraft = save
    }

    var isSaving: Bool { stage == .saving }
    var isBusy: Bool {
        switch stage {
        case .unloaded, .loading, .saving: true
        case .editing, .failed: false
        }
    }

    var failure: Failure? {
        guard case .failed(let failure) = stage else { return nil }
        return failure
    }

    var generalErrorKey: String? {
        guard let failure else { return nil }
        if case .field = failure { return nil }
        return failure.key
    }

    var draft: MobileVescDrivetrainDraft {
        MobileVescDrivetrainDraft(
            wheelDiameterMm: canonicalDecimalText(wheelDiameterMm), motorPolePairs: motorPolePairs,
            motorToWheelRatio: canonicalDecimalText(motorToWheelRatio)
        )
    }

    func load() async {
        guard !hasLoaded, stage != .loading, stage != .saving else { return }
        stage = .loading
        do {
            let loaded = try await loadDraft(token)
            try Task<Never, Never>.checkCancellation()
            if let loaded { apply(loaded) }
            hasLoaded = true
            stage = .editing
        } catch is CancellationError {
            stage = .unloaded
        } catch {
            stage = .failed(Self.failure(for: error, fallback: .load))
        }
    }

    @discardableResult
    func save() async -> Bool {
        guard hasLoaded, !isBusy else { return false }
        let submitted = draft
        stage = .saving
        do {
            let saved = try await saveDraft(token, submitted)
            apply(saved)
            stage = .editing
            return true
        } catch {
            stage = .failed(Self.failure(for: error, fallback: .persistence))
            return false
        }
    }

    func errorKey(for field: Field) -> String? {
        failure == .field(field) ? field.errorKey : nil
    }

    private func apply(_ draft: MobileVescDrivetrainDraft) {
        wheelDiameterMm = displayedDecimalText(draft.wheelDiameterMm)
        motorPolePairs = draft.motorPolePairs
        motorToWheelRatio = displayedDecimalText(draft.motorToWheelRatio)
    }

    private func canonicalDecimalText(_ text: String) -> String {
        text.replacingOccurrences(of: decimalSeparator, with: ".")
    }

    private func displayedDecimalText(_ text: String) -> String {
        text.replacingOccurrences(of: ".", with: decimalSeparator)
    }

    private func clearFailure() {
        if hasLoaded, case .failed = stage { stage = .editing }
    }

    private static func failure(for error: Error, fallback: Failure) -> Failure {
        guard let error = error as? MobileVescDrivetrainError else { return fallback }
        switch error {
        case .InvalidWheelDiameter: return .field(.wheelDiameter)
        case .InvalidMotorPolePairs: return .field(.motorPolePairs)
        case .InvalidReduction: return .field(.reduction)
        case .ConnectionUnavailable: return .connection
        case .UnsupportedDevice, .InvalidControllerTarget: return .unsupported
        case .PersistenceUnavailable, .Persistence: return fallback == .load ? .load : .persistence
        }
    }
}

struct VescDrivetrainEditorView: View {
    @Bindable var editor: VescDrivetrainEditor
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focusedField: VescDrivetrainEditor.Field?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    field(.wheelDiameter, text: $editor.wheelDiameterMm)
                    field(.motorPolePairs, text: $editor.motorPolePairs)
                    field(.reduction, text: $editor.motorToWheelRatio)
                } footer: {
                    Text(localizedAppText("vesc.drivetrain.description"))
                }
                if editor.stage == .loading {
                    ProgressView(localizedAppText("vesc.drivetrain.loading"))
                }
                if let key = editor.generalErrorKey {
                    Text(localizedAppText(key))
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("vesc.drivetrain.error")
                    if !editor.hasLoaded {
                        Button(localizedAppText("vesc.drivetrain.retry_load")) {
                            Task { await editor.load() }
                        }
                        .accessibilityIdentifier("vesc.drivetrain.retry-load")
                    }
                }
                Section {
                    Button(localizedAppText(editor.isSaving ? "vesc.drivetrain.saving" : "vesc.drivetrain.save")) {
                        Task { if await editor.save() { dismiss() } }
                    }
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.black)
                    .frame(maxWidth: .infinity, minHeight: 48)
                    .background(PevColors.yellow, in: RoundedRectangle(cornerRadius: 12))
                    .buttonStyle(.plain)
                    .disabled(editor.isBusy || !editor.hasLoaded)
                    .opacity(editor.isBusy || !editor.hasLoaded ? 0.5 : 1)
                    .keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("vesc.drivetrain.save")
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                }
            }
            .navigationTitle(localizedAppText("vesc.drivetrain.title"))
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                .scrollDismissesKeyboard(.interactively)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(localizedAppText("vesc.drivetrain.done")) { dismiss() }
                        .disabled(editor.isSaving)
                        .accessibilityIdentifier("vesc.drivetrain.done")
                }
            }
            .task { await editor.load() }
            .onChange(of: editor.failure) { _, failure in
                if let failure, case .field(let field) = failure { focusedField = field }
            }
            .accessibilityIdentifier("vesc.drivetrain.editor")
        }
        .interactiveDismissDisabled(editor.isSaving)
    }

    private func field(_ field: VescDrivetrainEditor.Field, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(localizedAppText(field.labelKey))
            TextField(localizedAppText(field.labelKey), text: text)
                .labelsHidden()
                .focused($focusedField, equals: field)
                .frame(minHeight: 44)
                .disabled(editor.isBusy || !editor.hasLoaded)
                .autocorrectionDisabled()
                #if os(iOS)
                    .textInputAutocapitalization(.never)
                    .keyboardType(field == .motorPolePairs ? .numberPad : .numbersAndPunctuation)
                #endif
                .accessibilityLabel(localizedAppText(field.labelKey))
                .accessibilityHint(editor.errorKey(for: field).map { localizedAppText($0) } ?? "")
                .accessibilityIdentifier("vesc.drivetrain.\(field.rawValue)")
            if let key = editor.errorKey(for: field) {
                Text(localizedAppText(key))
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .accessibilityIdentifier("vesc.drivetrain.\(field.rawValue).error")
            }
        }
    }
}
