import Foundation
import CutoutMobileFFI

/// Stores opaque, Rust-owned ride-session marker bytes for process relaunch reconciliation.
// UserDefaults documents its accessors as thread-safe, but does not declare Sendable.
public struct RideSessionMarkerStore: @unchecked Sendable {
    private static let key = "io.cutout.rideSession.marker"
    private let defaults: UserDefaults
    private let database: RideDatabaseHandle?

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.database = defaults === UserDefaults.standard ? RustPersistenceStore.shared : nil
    }

    init(database: RideDatabaseHandle) {
        self.defaults = .standard
        self.database = database
    }

    public var marker: Data? {
        if let database {
            if let legacy = defaults.data(forKey: Self.key) {
                if legacy.isEmpty {
                    if (try? database.clearRideSessionMarker()) != nil {
                        defaults.removeObject(forKey: Self.key)
                    }
                    return nil
                }
                if (try? database.saveRideSessionMarker(marker: legacy)) != nil {
                    defaults.removeObject(forKey: Self.key)
                }
                return legacy
            }
            if let persisted = try? database.rideSessionMarker() {
                return persisted
            }
            return nil
        }
        return defaults.data(forKey: Self.key)
    }

    public func save(_ marker: Data) {
        guard marker.isEmpty == false else {
            try? clear()
            return
        }
        if let database {
            if (try? database.saveRideSessionMarker(marker: marker)) != nil {
                defaults.removeObject(forKey: Self.key)
            } else {
                defaults.set(marker, forKey: Self.key)
            }
            return
        }
        defaults.set(marker, forKey: Self.key)
    }

    public func clear() throws {
        if let database {
            do {
                try database.clearRideSessionMarker()
                defaults.removeObject(forKey: Self.key)
            } catch {
                defaults.set(Data(), forKey: Self.key)
                throw error
            }
            return
        }
        defaults.removeObject(forKey: Self.key)
    }
}
