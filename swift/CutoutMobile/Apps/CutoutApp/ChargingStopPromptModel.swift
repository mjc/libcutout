import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

#if os(iOS)
    import UIKit
#endif

/// Native scheduling, local preferences and presentation for the Rust-owned prompt policy.
@MainActor
@Observable
final class ChargingStopPromptModel {
    private struct PromptPresentation {
        let candidate: MobileChargingStopPromptDto
        let generation: UInt64
    }

    private var promptPresentation: PromptPresentation?
    private(set) var isNativePresentationAvailable = false
    private var acknowledgedGeneration: UInt64?
    private let query: @MainActor (MobilePhoneLocationSampleDto?, Bool) async -> MobileChargingStopDecisionDto
    private let acknowledge: @MainActor (MobileChargingStopPromptDto) -> String?
    private let dismissPolicy: @MainActor () -> Void
    private let saveHistory: @MainActor (String) -> Void
    private var generation: UInt64 = 0

    var prompt: MobileChargingStopPromptDto? { promptPresentation?.candidate }
    var promptGeneration: UInt64? { promptPresentation?.generation }

    convenience init(
        state: MobileRideMapState?, session: CutoutSessionStateHandle,
        now: @escaping @MainActor () -> UInt64,
        defaults: UserDefaults = .standard
    ) {
        let key = "charging-stop-prompt-history"
        let policy = MobileChargingStopPrompts(historyJson: defaults.string(forKey: key))
        self.init(
            query: { location, canPresent in
                guard let state else { return .noAssociatedRide }
                return await state.chargingStopPrompt(
                    policy: policy, session: session,
                    context: MobileChargingStopContextDto(
                        location: location, nowMs: now(),
                        nowUnixSeconds: Date().timeIntervalSince1970, canPresent: canPresent,
                        phoneConnectedToPower: Self.phoneConnectedToPower
                    )
                )
            },
            acknowledge: { policy.presented(prompt: $0) },
            dismiss: { policy.dismiss() },
            saveHistory: { defaults.set($0, forKey: key) }
        )
    }

    private static var phoneConnectedToPower: Bool {
        #if os(iOS)
            switch UIDevice.current.batteryState {
            case .charging, .full: true
            case .unknown, .unplugged: false
            @unknown default: false
            }
        #else
            false
        #endif
    }

    init(
        query: @escaping @MainActor (MobilePhoneLocationSampleDto?, Bool) async -> MobileChargingStopDecisionDto,
        acknowledge: @escaping @MainActor (MobileChargingStopPromptDto) -> String?,
        dismiss: @escaping @MainActor () -> Void,
        saveHistory: @escaping @MainActor (String) -> Void
    ) {
        self.query = query
        self.acknowledge = acknowledge
        self.dismissPolicy = dismiss
        self.saveHistory = saveHistory
    }

    func refresh(location: MobilePhoneLocationSampleDto?, canPresent: Bool = true) async {
        generation &+= 1
        let expectedGeneration = generation
        isNativePresentationAvailable = canPresent
        if !canPresent { retirePrompt() }
        let decision = await query(location, canPresent)
        guard !Task.isCancelled, generation == expectedGeneration else { return }
        guard canPresent else {
            return
        }
        switch decision {
        case .offer(let candidate):
            if promptPresentation?.candidate != candidate {
                promptPresentation = PromptPresentation(candidate: candidate, generation: expectedGeneration)
                acknowledgedGeneration = nil
            }
        default:
            retirePrompt()
        }
    }

    /// Records Rust's suppression history only after this exact candidate is visibly presented.
    @discardableResult
    func didPresent(
        _ candidate: MobileChargingStopPromptDto,
        generation: UInt64,
        canPresent: Bool = true
    ) -> Bool {
        guard let current = promptPresentation,
            current.candidate == candidate,
            current.generation == generation,
            acknowledgedGeneration != generation
        else { return false }

        guard canPresent, isNativePresentationAvailable else {
            dismiss()
            return false
        }

        guard let history = acknowledge(candidate) else {
            retirePrompt()
            return false
        }
        acknowledgedGeneration = generation
        saveHistory(history)
        return true
    }

    func setBatteryMonitoring(_ enabled: Bool) {
        #if os(iOS)
            UIDevice.current.isBatteryMonitoringEnabled = enabled
        #endif
    }

    private func retirePrompt() {
        promptPresentation = nil
        acknowledgedGeneration = nil
    }

    /// Dismissal, acceptance and loss of native presentation preserve Rust suppression history.
    func dismiss() {
        generation &+= 1
        dismissPolicy()
        isNativePresentationAvailable = false
        retirePrompt()
    }
}
