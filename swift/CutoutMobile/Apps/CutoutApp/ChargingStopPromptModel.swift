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
    private(set) var prompt: MobileChargingStopPromptDto?
    private let query: @MainActor (MobilePhoneLocationSampleDto?) async -> MobileChargingStopDecisionDto
    private let acknowledge: @MainActor (MobileChargingStopPromptDto) -> String?
    private let dismissPolicy: @MainActor () -> Void
    private let saveHistory: @MainActor (String) -> Void
    private var generation: UInt64 = 0

    convenience init(
        state: MobileRideMapState?, session: CutoutSessionStateHandle,
        now: @escaping @MainActor () -> UInt64,
        defaults: UserDefaults = .standard
    ) {
        #if os(iOS)
            UIDevice.current.isBatteryMonitoringEnabled = true
        #endif
        let key = "charging-stop-prompt-history"
        let policy = MobileChargingStopPrompts(historyJson: defaults.string(forKey: key))
        self.init(
            query: { location in
                guard let state else { return .noAssociatedRide }
                return await state.chargingStopPrompt(
                    policy: policy, session: session,
                    context: MobileChargingStopContextDto(
                        location: location, nowMs: now(),
                        nowUnixSeconds: Date().timeIntervalSince1970, canPresent: true,
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
        query: @escaping @MainActor (MobilePhoneLocationSampleDto?) async -> MobileChargingStopDecisionDto,
        acknowledge: @escaping @MainActor (MobileChargingStopPromptDto) -> String?,
        dismiss: @escaping @MainActor () -> Void,
        saveHistory: @escaping @MainActor (String) -> Void
    ) {
        self.query = query
        self.acknowledge = acknowledge
        self.dismissPolicy = dismiss
        self.saveHistory = saveHistory
    }

    func refresh(location: MobilePhoneLocationSampleDto?) async {
        let expectedGeneration = generation
        let decision = await query(location)
        guard !Task.isCancelled, generation == expectedGeneration else { return }
        switch decision {
        case .offer(let candidate):
            guard let history = acknowledge(candidate) else {
                prompt = nil
                return
            }
            if prompt != candidate { saveHistory(history) }
            prompt = candidate
        default:
            prompt = nil
        }
    }

    /// Dismissal, acceptance and loss of native presentation preserve Rust suppression history.
    func dismiss() {
        generation &+= 1
        dismissPolicy()
        prompt = nil
    }
}
