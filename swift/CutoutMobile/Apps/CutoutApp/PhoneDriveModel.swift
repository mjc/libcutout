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
    let isInputPreview: Bool
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
    }

    /// Current production profiles cannot grant drive authority. A simulator fixture
    /// may exercise semantic input feedback using the same Rust processor.
    var capturesInput: Bool {
        isInputPreview && snapshot.mode != .disabled && snapshot.generation != nil
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

    func contacts(_ contacts: MobilePhoneDriveContactsDto, generation: UInt64) {
        guard capturesInput, let token = core.connectionSnapshot.token else { return }
        applySnapshot(core.observePhoneDriveContacts(token: token, generation: generation, contacts: contacts))
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

    var unavailableText: String {
        switch snapshot.unavailability {
        case .disconnected: localizedAppText("phone_control.unavailable.disconnected")
        case .vehicleUnsupported: localizedAppText("phone_control.unavailable.vehicle")
        case .controllerUnqualified: localizedAppText("phone_control.unavailable.controller")
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
