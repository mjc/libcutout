import CutoutMobileFFI
import Foundation
import OSLog
import Synchronization

/// The one Rust-owned SQLite service used by the mobile persistence adapters.
public enum RustPersistenceStore {
    private static let logger = Logger(subsystem: "io.cutout.mobile", category: "persistence")

    private static let cachedDatabase = Mutex<RideDatabaseHandle?>(nil)

    /// Reads an already-open handle; accessing this property never opens storage.
    public static var shared: RideDatabaseHandle? {
        cachedDatabase.withLock { $0 }
    }

    /// Opens the process-owned database off the main actor. Failures remain retryable.
    public static func open() async throws -> RideDatabaseHandle {
        if let database = shared { return database }
        let database = try await Task.detached(priority: .userInitiated) {
            try openDefaultDatabase()
        }.value
        cachedDatabase.withLock { $0 = database }
        return database
    }

    static func open(at databaseURL: URL) async throws -> RideDatabaseHandle {
        try await Task.detached(priority: .userInitiated) {
            try prepareDatabase(at: databaseURL)
        }.value
    }

    private static func openDefaultDatabase() throws -> RideDatabaseHandle {
        let fileManager = FileManager.default
        guard
            let applicationSupport = fileManager.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first
        else {
            throw CocoaError(.fileNoSuchFile)
        }
        let directory = applicationSupport.appendingPathComponent("Cutout", isDirectory: true)
        return try prepareDatabase(at: directory.appendingPathComponent("ride.sqlite"))
    }

    private static func prepareDatabase(at url: URL) throws -> RideDatabaseHandle {
        let fileManager = FileManager.default
        let directory = url.deletingLastPathComponent()
        var databaseURL = url
        do {
            try fileManager.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [
                    .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication
                ]
            )
            let database = try openRideDatabase(path: databaseURL.path)
            do {
                var values = URLResourceValues()
                values.isExcludedFromBackup = true
                try databaseURL.setResourceValues(values)
            } catch {
                logger.error("Could not exclude ride database from backups: \(error, privacy: .public)")
            }
            do {
                try fileManager.setAttributes(
                    [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                    ofItemAtPath: databaseURL.path
                )
            } catch {
                logger.error("Could not apply ride database file protection: \(error, privacy: .public)")
            }
            return database
        } catch {
            logger.error("Could not open Rust ride database: \(error, privacy: .public)")
            throw error
        }
    }
}
