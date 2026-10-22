import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Native presentation and observation delivery for the existing Rust session owner.
@MainActor
@Observable
final class PhoneDriveModel {
    private let core: any CutoutSessionDriving
    private(set) var snapshot: MobilePhoneDriveSnapshotDto
    private(set) var settingsBusy = false
    private(set) var settingsLoaded = false
    private(set) var settingsError = false
    private(set) var limitsError = false
    private(set) var activationError = false
    var throttleAmpsText = "3"
    var brakeAmpsText = "3"
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

    init(core: any CutoutSessionDriving) {
        self.core = core
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

    var canActivate: Bool {
        guard settingsLoaded, !settingsBusy, snapshot.mode != .disabled else { return false }
        if case .ready = snapshot.availability { return true }
        return false
    }

    func activate() {
        guard canActivate, let token = core.connectionSnapshot.token else { return }
        do {
            #if os(iOS)
                nativeContacts.reobserveAllUp()
            #endif
            applySnapshot(try core.activatePhoneDrive(token: token))
            #if os(iOS)
                nativeContacts.reobserveAllUp()
            #endif
            activationError = false
        } catch {
            activationError = true
            refresh()
        }
    }

    func saveLimits() async {
        guard !settingsBusy, settingsLoaded else { return }
        settingsBusy = true
        defer { settingsBusy = false }
        let state = core.rideSessionStateHandle
        let throttle = throttleAmpsText
        let brake = brakeAmpsText
        do {
            _ = try await Task.detached(priority: .userInitiated) {
                try state.savePhoneDriveLimits(throttleAmpsText: throttle, brakeAmpsText: brake)
            }.value
            refresh()
            if throttleAmpsText == throttle, brakeAmpsText == brake { restoreBudgetDrafts() }
            limitsError = false
        } catch {
            limitsError = true
            refresh()
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
        applySnapshot(core.rideSessionStateHandle.phoneDriveSnapshot())
    }

    func loadMode() async {
        guard !settingsBusy else { return }
        settingsBusy = true
        defer { settingsBusy = false }
        let state = core.rideSessionStateHandle
        do {
            _ = try await Task.detached(priority: .userInitiated) {
                try state.loadPhoneControlMode()
            }.value
            // Read the current projection after awaiting: a concurrent connection or
            // lifecycle cancellation must not be replaced by a captured older snapshot.
            refresh()
            restoreBudgetDrafts()
            settingsLoaded = true
            settingsError = false
        } catch {
            settingsError = true
            refresh()
        }
    }

    func saveMode(_ mode: MobilePhoneControlModeDto) async {
        guard !settingsBusy, settingsLoaded else { return }
        cancel(.modeChanged)
        settingsBusy = true
        defer { settingsBusy = false }
        let state = core.rideSessionStateHandle
        do {
            _ = try await Task.detached(priority: .userInitiated) {
                try state.savePhoneControlMode(mode: mode)
            }.value
            refresh()
            settingsError = false
        } catch {
            settingsError = true
            refresh()
        }
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
