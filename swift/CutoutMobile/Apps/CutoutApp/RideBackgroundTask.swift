#if os(iOS)
    import UIKit
#endif

/// Owns only the native background execution lease; Rust owns checkpoint semantics.
@MainActor
final class RideBackgroundTask {
    typealias EndTask = @MainActor () -> Void
    typealias BeginTask = @MainActor (@escaping @MainActor @Sendable () -> Void) -> EndTask?

    private var endTask: EndTask?

    init(
        onExpiration: @escaping @MainActor () -> Void,
        beginTask: BeginTask = RideBackgroundTask.beginPlatformTask
    ) {
        endTask = beginTask { [weak self] in
            guard let self, self.endTask != nil else { return }
            self.end()
            onExpiration()
        }
    }

    isolated deinit { end() }

    func end() {
        let completed = endTask
        endTask = nil
        completed?()
    }

    private static func beginPlatformTask(
        onExpiration: @escaping @MainActor @Sendable () -> Void
    ) -> EndTask? {
        #if os(iOS)
            let identifier = UIApplication.shared.beginBackgroundTask(
                withName: "Ride checkpoint", expirationHandler: onExpiration
            )
            guard identifier != .invalid else { return nil }
            return { UIApplication.shared.endBackgroundTask(identifier) }
        #else
            return nil
        #endif
    }
}
