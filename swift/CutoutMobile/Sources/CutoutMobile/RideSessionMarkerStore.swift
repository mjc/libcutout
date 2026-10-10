import CutoutMobileFFI
import Foundation
import OSLog
import Synchronization

/// Native storage boundary for opaque Rust-owned ride identity bytes.
public struct RideSessionMarkerStore: @unchecked Sendable {
    private static let key = "io.cutout.rideSession.marker"
    private let defaults: UserDefaults
    private let database: RideDatabaseHandle?
    private let writer: RideSessionMarkerNativeWorker?

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        let database = defaults === UserDefaults.standard ? RustPersistenceStore.shared : nil
        self.database = database
        writer = database.map { RideSessionMarkerNativeWorker($0, defaults: defaults, key: Self.key) }
    }

    init(database: RideDatabaseHandle) {
        defaults = .standard
        self.database = database
        writer = RideSessionMarkerNativeWorker(database, defaults: .standard, key: Self.key)
    }

    public var requiresDatabaseLoad: Bool { database != nil }

    /// Reads already-acknowledged bytes without a SQLite round trip.
    public var marker: Data? {
        if let database { return database.acknowledgedRideSessionMarker() }
        return defaults.data(forKey: Self.key)
    }

    /// Loads durable state off the caller's executor after earlier marker intent settles.
    public func load() async throws -> Data? {
        guard let database, let writer else { return marker }
        if let nativePreference = defaults.data(forKey: Self.key) {
            writer.migrate(nativePreference.isEmpty ? nil : nativePreference)
            await writer.waitForPersistence()
            defaults.removeObject(forKey: Self.key)
        } else {
            await writer.waitForPersistence()
        }
        return try await Task.detached(priority: .userInitiated) {
            try database.rideSessionMarker()
        }.value
    }

    /// Admits current desired bytes; Rust acknowledges them only after durable storage success.
    public func save(_ marker: Data) {
        if let writer {
            writer.request(marker.isEmpty ? nil : marker)
        } else if marker.isEmpty {
            defaults.removeObject(forKey: Self.key)
        } else {
            defaults.set(marker, forKey: Self.key)
        }
    }

    /// Admits removal without waiting for SQLite. Failed removal remains pending in Rust.
    public func clear() throws {
        if let writer {
            writer.request(nil)
        } else {
            defaults.removeObject(forKey: Self.key)
        }
    }

    /// Waits off the caller's executor for the latest admitted marker intent to become durable.
    public func waitForPersistence() async {
        await writer?.waitForPersistence()
    }
}

/// One owned native polling task. Admission, deduplication, ordering, and retry are Rust-owned.
private final class RideSessionMarkerNativeWorker: @unchecked Sendable {
    private static let logger = Logger(subsystem: "io.cutout.mobile", category: "ride-session-marker")
    private let database: RideDatabaseHandle
    private let defaults: UserDefaults
    private let key: String
    private let task = Mutex<Task<Void, Never>?>(nil)

    init(_ database: RideDatabaseHandle, defaults: UserDefaults, key: String) {
        self.database = database
        self.defaults = defaults
        self.key = key
    }

    private func removeOlderPreferenceAfterAcknowledgement() {
        guard defaults.object(forKey: key) != nil else { return }
        defaults.removeObject(forKey: key)
    }

    private static var nowMs: UInt64 { UInt64(ProcessInfo.processInfo.systemUptime * 1_000) }

    func request(_ marker: Data?) {
        task.withLock { current in
            let status = database.requestRideSessionMarker(marker: marker, nowMs: Self.nowMs)
            if status == .committed { removeOlderPreferenceAfterAcknowledgement() }
            startIfNeeded(status, task: &current)
        }
    }

    func migrate(_ marker: Data?) {
        task.withLock { current in
            let status = database.migrateRideSessionMarker(marker: marker, nowMs: Self.nowMs)
            if status == .committed { removeOlderPreferenceAfterAcknowledgement() }
            startIfNeeded(status, task: &current)
        }
    }

    private func startIfNeeded(_ status: MobileRideSessionMarkerWriteStatusDto, task: inout Task<Void, Never>?) {
        if status == .committed { return }
        guard task == nil else { return }
        task = Task.detached(priority: .utility) { [self] in
            var lastError: String?
            while true {
                let status = database.pollRideSessionMarker(nowMs: Self.nowMs)
                if case .retrying(let message) = status, message != lastError {
                    Self.logger.error("Ride marker persistence will retry: \(message, privacy: .public)")
                    lastError = message
                }
                let finished = self.task.withLock { current in
                    // This read cannot recover or wait for SQLite. It fences an admission that
                    // races the preceding poll, so pending intent never loses its native worker.
                    guard database.rideSessionMarkerWriteStatus() == .committed else { return false }
                    removeOlderPreferenceAfterAcknowledgement()
                    current = nil
                    return true
                }
                if finished { return }
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
    }

    func waitForPersistence() async {
        while let current = task.withLock({ current in
            startIfNeeded(database.rideSessionMarkerWriteStatus(), task: &current)
            return current
        }) {
            await current.value
        }
    }
}
