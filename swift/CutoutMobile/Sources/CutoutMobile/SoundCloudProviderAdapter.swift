import CutoutMobileFFI
import Foundation

#if os(iOS)
    import UIKit
#endif

/// Executes local app opening admitted by Rust. It observes no SoundCloud player.
@MainActor
public final class SoundCloudProviderAdapter {
    public typealias OpenURL = @MainActor @Sendable (URL, @escaping @MainActor @Sendable (Bool) -> Void) -> Void

    private let lifecycle: MobileMusicProviderLifecycle
    private let transport: MusicProviderTransportExecutor
    private let nowMs: @MainActor () -> UInt64
    private let openURL: OpenURL
    private var generation: MobileMusicProviderSessionId?

    public init(
        lifecycle: MobileMusicProviderLifecycle,
        effects: MusicProviderEffectExecutor,
        nowMs: @escaping @MainActor () -> UInt64,
        openURL: @escaping OpenURL = { url, completion in
            #if os(iOS)
                UIApplication.shared.open(url, options: [:], completionHandler: completion)
            #else
                completion(false)
            #endif
        }
    ) {
        self.lifecycle = lifecycle
        self.nowMs = nowMs
        self.openURL = openURL
        transport = MusicProviderTransportExecutor(lifecycle: lifecycle, effects: effects, nowMs: nowMs)
    }

    public func start() {
        guard generation == nil else { return }
        generation = lifecycle.beginProviderSession()
    }

    public func stop() {
        guard let generation else { return }
        self.generation = nil
        transport.apply(lifecycle.retireProviderSession(id: generation))
    }

    public func applySuspension(_ suspension: MobileMusicProviderSuspension) {
        transport.apply(suspension)
    }

    public func applyCompletion(_ completion: MobileMusicTransportCompletion) {
        transport.apply(completion)
    }

    public func perform(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        guard !Task.isCancelled else { return .unavailable }
        start()
        guard let generation else { return .unavailable }
        switch lifecycle.beginSoundcloudCommand(providerGeneration: generation, command: command, nowMs: nowMs()) {
        case .refused: return .refused
        case .unavailable: return .unavailable
        case let .handoff(url, effect):
            guard let url = URL(string: url) else {
                transport.apply(lifecycle.cancelTransport(providerGeneration: generation, requestId: effect.id))
                return .unavailable
            }
            return await transport.perform(providerGeneration: generation, effect: effect) { [openURL] _, completion in
                openURL(url, completion)
            }
        }
    }
}
