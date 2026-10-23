import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Native presentation and observation delivery for the existing Rust session owner.
@MainActor
@Observable
final class PhoneDriveModel {
    struct ActivationRequest {
        let id: UUID
        let presentationID: UUID
        let token: MobileConnectionAttemptTokenDto
        let expectedInputGeneration: UInt64
    }

    typealias PreferencesLoader =
        @MainActor (
            CutoutSessionStateHandle, MobileConnectionAttemptTokenDto
        ) async throws -> MobilePhoneDriveSnapshotDto
    typealias PreferencesSaver =
        @MainActor (
            CutoutSessionStateHandle, MobileConnectionAttemptTokenDto, MobilePhoneControlModeDto, String, String
        ) async throws -> MobilePhoneDriveSnapshotDto

    private let core: any CutoutSessionDriving
    private let loadPreferences: PreferencesLoader
    private let savePreferences: PreferencesSaver
    private(set) var snapshot: MobilePhoneDriveSnapshotDto
    private(set) var settingsBusy = false
    private(set) var settingsLoaded = false
    private(set) var settingsError = false
    private(set) var limitsError = false
    private(set) var activationError = false
    private(set) var settingsMode = MobilePhoneControlModeDto.disabled
    var throttleAmpsText = "3"
    var brakeAmpsText = "3"
    private var settingsToken: MobileConnectionAttemptTokenDto?
    private var settingsPresentationID: UUID?
    private var pendingActivationRequest: ActivationRequest?
    private var activationDismissalClaimed = false
    private var settingsOperationID: UUID?
    let isInputPreview: Bool
    #if os(iOS)
        let nativeContacts = PhoneDriveNativeContactObserver()
    #endif
    #if DEBUG && targetEnvironment(simulator)
        private struct PreviewReceipt {
            let ordinal: UInt64
            let snapshot: MobilePhoneDriveSnapshotDto
        }
        private var previewReceipts: [PreviewReceipt]?
    #endif

    init(
        core: any CutoutSessionDriving,
        loadPreferences: @escaping PreferencesLoader = { state, token in
            try await Task.detached(priority: .userInitiated) {
                try state.loadPhoneDrivePreferences(token: token)
            }.value
        },
        savePreferences: @escaping PreferencesSaver = { state, token, mode, throttle, brake in
            try await Task.detached(priority: .userInitiated) {
                try state.savePhoneDrivePreferences(
                    token: token, mode: mode,
                    throttleAmpsText: throttle, brakeAmpsText: brake
                )
            }.value
        }
    ) {
        self.core = core
        self.loadPreferences = loadPreferences
        self.savePreferences = savePreferences
        snapshot = core.rideSessionStateHandle.phoneDriveSnapshot()
        #if DEBUG && targetEnvironment(simulator)
            isInputPreview =
                CommandLine.arguments.contains("--ui-test-phone-drive-input")
                && CutoutUITestSessionFixture.resolve(
                    environmentValue: ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_FIXTURE"],
                    persistedValue: nil, arguments: CommandLine.arguments
                ) == .eskate
            previewReceipts = isInputPreview ? [] : nil
        #else
            isInputPreview = false
        #endif
        #if os(iOS)
            nativeContacts.now = { core.now().rawValue }
            nativeContacts.currentGeneration = { [weak self] in self?.snapshot.generation }
            nativeContacts.observe = { [weak self] contacts, generation, observedAtMs in
                self?.contacts(contacts, generation: generation, observedAtMs: observedAtMs)
            }
            nativeContacts.cancel = { [weak self] reason in self?.cancel(reason) }
        #endif
    }

    var capturesInput: Bool {
        guard snapshot.mode != .disabled, snapshot.generation != nil else { return false }
        if case .active = snapshot.availability { return true }
        return false
    }

    var canPresentSettings: Bool {
        guard core.connectionSnapshot.token != nil else { return false }
        return switch snapshot.availability {
        case .ready, .active: true
        case .unavailable: false
        }
    }

    func updateSettingsMode(_ mode: MobilePhoneControlModeDto) {
        guard settingsLoaded, !settingsBusy else { return }
        settingsMode = mode
    }

    func presentSettings(
        for expectedToken: MobileConnectionAttemptTokenDto? = nil,
        activationFailed: Bool = false
    ) -> Bool {
        guard let token = expectedToken ?? core.connectionSnapshot.token,
            core.connectionSnapshot.token == token
        else { return false }
        do {
            applySnapshot(
                try core.rideSessionStateHandle.setPhoneDriveSettingsPresented(
                    token: token, presented: true
                ))
            invalidateSettingsOperation()
            pendingActivationRequest = nil
            activationDismissalClaimed = false
            settingsToken = token
            settingsPresentationID = UUID()
            settingsLoaded = false
            settingsError = false
            limitsError = false
            activationError = activationFailed
            settingsMode = .disabled
            throttleAmpsText = "3"
            brakeAmpsText = "3"
            return true
        } catch {
            settingsError = true
            refresh()
            return false
        }
    }

    func dismissSettings() -> ActivationRequest? {
        let activationRequest = activationDismissalClaimed ? pendingActivationRequest : nil
        pendingActivationRequest = nil
        activationDismissalClaimed = false
        invalidateSettingsOperation()
        guard let token = settingsToken else { return nil }
        settingsToken = nil
        settingsPresentationID = nil
        settingsLoaded = false
        do {
            applySnapshot(
                try core.rideSessionStateHandle.setPhoneDriveSettingsPresented(
                    token: token, presented: false
                ))
        } catch {
            refresh()
            return nil
        }
        guard let activationRequest,
            activationRequest.token == token,
            activationRequest.expectedInputGeneration == snapshot.generation,
            core.connectionSnapshot.token == token
        else { return nil }
        return activationRequest
    }

    func saveSettings() async -> Bool {
        if case .saved = await performSaveSettings(requestActivation: false) { return true }
        return false
    }

    func saveSettingsAndRequestActivation() async -> ActivationRequest? {
        guard case .activationRequest(let request) = await performSaveSettings(requestActivation: true) else {
            return nil
        }
        return request
    }

    private enum SaveResult {
        case saved
        case activationRequest(ActivationRequest)
        case rejected
    }

    private func performSaveSettings(requestActivation: Bool) async -> SaveResult {
        guard !settingsBusy, settingsLoaded, let token = settingsToken,
            let presentationID = settingsPresentationID,
            !requestActivation || settingsMode != .disabled
        else { return .rejected }
        let operationID = beginSettingsOperation()
        defer { finishSettingsOperation(operationID) }
        let mode = settingsMode
        let throttle = throttleAmpsText
        let brake = brakeAmpsText
        do {
            let saved = try await savePreferences(
                core.rideSessionStateHandle, token, mode, throttle, brake)
            refresh()
            guard ownsSettingsOperation(operationID, token: token) else {
                return .rejected
            }
            guard snapshotMatchesCurrentProjection(saved, token: token) else {
                return .rejected
            }
            if settingsMode == mode,
                throttleAmpsText == throttle, brakeAmpsText == brake
            {
                restoreBudgetDrafts()
            }
            if requestActivation {
                guard let expectedInputGeneration = snapshot.generation else { return .rejected }
                let request = ActivationRequest(
                    id: UUID(), presentationID: presentationID, token: token,
                    expectedInputGeneration: expectedInputGeneration)
                pendingActivationRequest = request
                activationDismissalClaimed = false
                limitsError = false
                settingsError = false
                return .activationRequest(request)
            }
            limitsError = false
            settingsError = false
            return .saved
        } catch {
            refresh()
            guard ownsSettingsOperation(operationID, token: token) else {
                return .rejected
            }
            if let phoneDriveError = error as? MobilePhoneControlError,
                case .InvalidLimits = phoneDriveError
            {
                limitsError = true
                settingsError = false
            } else {
                settingsError = true
                limitsError = false
            }
            return .rejected
        }
    }

    func claimActivationDismissal(_ request: ActivationRequest) -> Bool {
        guard !activationDismissalClaimed,
            let pendingActivationRequest,
            pendingActivationRequest.id == request.id,
            request.presentationID == settingsPresentationID,
            request.token == settingsToken,
            request.expectedInputGeneration == snapshot.generation,
            core.connectionSnapshot.token == request.token
        else { return false }
        activationDismissalClaimed = true
        return true
    }

    @discardableResult
    func activateAfterSettingsDismissal(_ request: ActivationRequest) -> Bool {
        guard core.connectionSnapshot.token == request.token,
            snapshot.generation == request.expectedInputGeneration
        else {
            refresh()
            return false
        }
        #if os(iOS)
            nativeContacts.reobserveAllUp()
        #endif
        do {
            applySnapshot(
                try core.activatePhoneDrive(
                    token: request.token, expectedInputGeneration: request.expectedInputGeneration))
            #if os(iOS)
                nativeContacts.reobserveAllUp()
            #endif
            activationError = false
            return true
        } catch {
            activationError = true
            refresh()
            return false
        }
    }

    private func restoreBudgetDrafts() {
        throttleAmpsText = Self.ampsText(snapshot.limits.throttleMilliamps)
        brakeAmpsText = Self.ampsText(snapshot.limits.brakeMilliamps)
    }

    private static func ampsText(_ milliamps: UInt32) -> String {
        let whole = milliamps / 1_000
        let remainder = milliamps % 1_000
        guard remainder != 0 else { return String(whole) }
        var fraction = String(format: "%03u", remainder)
        while fraction.last == "0" { fraction.removeLast() }
        return "\(whole).\(fraction)"
    }

    func refresh() {
        let current = core.rideSessionStateHandle.phoneDriveSnapshot()
        if let settingsToken,
            core.connectionSnapshot.token != settingsToken
        {
            invalidateSettingsOperation()
            pendingActivationRequest = nil
            activationDismissalClaimed = false
            self.settingsToken = nil
            settingsPresentationID = nil
            settingsLoaded = false
            settingsMode = .disabled
            throttleAmpsText = "3"
            brakeAmpsText = "3"
        }
        applySnapshot(current)
    }

    func loadSettings() async {
        guard !settingsBusy, let token = settingsToken else { return }
        let operationID = beginSettingsOperation()
        defer { finishSettingsOperation(operationID) }
        do {
            let loaded = try await loadPreferences(core.rideSessionStateHandle, token)
            refresh()
            guard ownsSettingsOperation(operationID, token: token) else {
                return
            }
            guard snapshotMatchesCurrentProjection(loaded, token: token) else { return }
            settingsMode = snapshot.mode
            restoreBudgetDrafts()
            settingsLoaded = true
            settingsError = false
        } catch {
            refresh()
            guard ownsSettingsOperation(operationID, token: token) else {
                return
            }
            settingsError = true
        }
    }

    private func beginSettingsOperation() -> UUID {
        let operationID = UUID()
        settingsOperationID = operationID
        settingsBusy = true
        return operationID
    }

    private func finishSettingsOperation(_ operationID: UUID) {
        guard settingsOperationID == operationID else { return }
        settingsOperationID = nil
        settingsBusy = false
    }

    private func ownsSettingsOperation(_ operationID: UUID, token: MobileConnectionAttemptTokenDto) -> Bool {
        settingsOperationID == operationID
            && settingsToken == token
            && core.connectionSnapshot.token == token
    }

    private func snapshotMatchesCurrentProjection(
        _ response: MobilePhoneDriveSnapshotDto,
        token: MobileConnectionAttemptTokenDto
    ) -> Bool {
        snapshot.deviceIdentity == token.platformIdentifier && snapshot == response
    }

    private func invalidateSettingsOperation() {
        settingsOperationID = nil
        settingsBusy = false
    }

    func contacts(_ contacts: MobilePhoneDriveContactsDto, generation: UInt64, observedAtMs: UInt64? = nil) {
        guard let token = core.connectionSnapshot.token else { return }
        let observedAtMs = observedAtMs ?? core.now().rawValue
        applySnapshot(
            core.observePhoneDriveContacts(
                token: token, generation: generation, contacts: contacts, observedAtMs: observedAtMs
            ))
    }

    func cancel(_ reason: MobilePhoneDriveCancelReasonDto) {
        pendingActivationRequest = nil
        activationDismissalClaimed = false
        invalidateSettingsOperation()
        applySnapshot(core.cancelPhoneDrive(reason: reason))
    }

    private func applySnapshot(_ value: MobilePhoneDriveSnapshotDto) {
        snapshot = value
        #if DEBUG && targetEnvironment(simulator)
            guard var receipts = previewReceipts else { return }
            if let last = receipts.last?.snapshot,
                last.mode == value.mode, last.generation == value.generation, last.phase == value.phase
            {
                return
            }
            let (ordinal, overflow) = (receipts.last?.ordinal ?? 0).addingReportingOverflow(1)
            guard !overflow else { return }
            receipts.append(PreviewReceipt(ordinal: ordinal, snapshot: value))
            if receipts.count > 16 { receipts.removeFirst(receipts.count - 16) }
            previewReceipts = receipts
        #endif
    }

    #if DEBUG && targetEnvironment(simulator)
        /// Bounded observations of actual Rust returns, available only to the explicit
        /// input preview fixture. This is neither an input decision nor a drive receipt.
        var previewReceiptValue: String {
            guard let previewReceipts else { return "" }
            return "v1"
                + previewReceipts.map { receipt in
                    let value = receipt.snapshot
                    let generation = value.generation.map(String.init) ?? "none"
                    let mode: String =
                        switch value.mode {
                        case .disabled: "disabled"
                        case .buttons: "buttons"
                        case .area: "area"
                        }
                    let phase: String =
                        switch value.phase {
                        case .idle: "idle"
                        case .pressed(let action): action == .brake ? "pressed:brake" : "pressed:throttle"
                        case .cancelled(let reason): "cancelled:" + Self.previewReason(reason)
                        }
                    return "|n=\(receipt.ordinal),m=\(mode),g=\(generation),p=\(phase)"
                }.joined()
        }

        private static func previewReason(_ reason: MobilePhoneDriveCancelReasonDto) -> String {
            switch reason {
            case .activationChanged: "activation_changed"
            case .inputExpired: "input_expired"
            case .transportUnavailable: "transport_unavailable"
            case .controllerFault: "controller_fault"
            case .configurationChanged: "configuration_changed"
            case .touchCancelled: "touch_cancelled"
            case .invalidContact: "invalid_contact"
            case .modeChanged: "mode_changed"
            case .layoutChanged: "layout_changed"
            case .navigation: "navigation"
            case .sceneInactive: "scene_inactive"
            case .connectionChanged: "connection_changed"
            case .regionLeft: "region_left"
            case .multipleContacts: "multiple_contacts"
            case .contactReplaced: "contact_replaced"
            case .generationExhausted: "generation_exhausted"
            }
        }
    #endif

    var limitsSummaryText: String {
        localizedAppText(
            "phone_control.limits_summary", Self.ampsText(snapshot.limits.throttleMilliamps),
            Self.ampsText(snapshot.limits.brakeMilliamps))
    }

    var availabilityText: String {
        switch snapshot.availability {
        case .unavailable(let reason):
            switch reason {
            case .disconnected: localizedAppText("phone_control.unavailable.disconnected")
            case .vehicleUnsupported: localizedAppText("phone_control.unavailable.vehicle")
            }
        case .ready: localizedAppText("phone_control.inactive")
        case .active: localizedAppText("phone_control.active")
        }
    }

    var feedbackText: String {
        switch snapshot.phase {
        case .idle: ""
        case .pressed(let action):
            localizedAppText(action == .brake ? "phone_control.brake" : "phone_control.throttle")
                + " · " + localizedAppText("phone_control.held")
        case .cancelled: localizedAppText("phone_control.cancelled")
        }
    }
}

extension MobilePhoneControlModeDto {
    var phoneControlTitle: String {
        switch self {
        case .disabled: localizedAppText("phone_control.mode.disabled")
        case .buttons: localizedAppText("phone_control.mode.buttons")
        case .area: localizedAppText("phone_control.mode.area")
        }
    }
}
