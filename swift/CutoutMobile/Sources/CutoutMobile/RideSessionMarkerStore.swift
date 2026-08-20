import Foundation

/// Stores opaque, Rust-owned ride-session marker bytes for process relaunch reconciliation.
// UserDefaults documents its accessors as thread-safe, but does not declare Sendable.
public struct RideSessionMarkerStore: @unchecked Sendable {
    private static let key = "io.cutout.rideSession.marker"
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var marker: Data? {
        defaults.data(forKey: Self.key)
    }

    public func save(_ marker: Data) {
        guard marker.isEmpty == false else {
            clear()
            return
        }
        defaults.set(marker, forKey: Self.key)
    }

    public func clear() {
        defaults.removeObject(forKey: Self.key)
    }
}
