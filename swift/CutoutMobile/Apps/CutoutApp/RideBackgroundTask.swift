#if os(iOS)
import UIKit
#endif

/// Owns only the native background execution lease; Rust owns checkpoint semantics.
@MainActor
final class RideBackgroundTask {
#if os(iOS)
    private var identifier = UIBackgroundTaskIdentifier.invalid
#endif
    init(onExpiration: @escaping @MainActor () -> Void) {
#if os(iOS)
        identifier = UIApplication.shared.beginBackgroundTask(withName: "Ride checkpoint") { [weak self] in
            Task { @MainActor in
                onExpiration()
                self?.end()
            }
        }
#endif
    }

    isolated deinit { end() }

    func end() {
#if os(iOS)
        guard identifier != .invalid else { return }
        let completed = identifier
        identifier = .invalid
        UIApplication.shared.endBackgroundTask(completed)
#endif
    }
}
