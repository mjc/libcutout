import CutoutMobileFFI
import Foundation
import OSLog
import Synchronization

/// The one Rust-owned SQLite service used by the mobile persistence adapters.
public enum RustPersistenceStore {
    /// The detached opening operation owns this filesystem adapter; test recorders synchronize their state.
    private final class FileManagerAccess: @unchecked Sendable {
        let manager: FileManager

        init(_ manager: FileManager) {
            self.manager = manager
        }
    }

    private final class OpenAttempt: @unchecked Sendable {
        let task: Task<RideDatabaseHandle, Error>

        init(task: Task<RideDatabaseHandle, Error>) {
            self.task = task
        }
    }

    private static let logger = Logger(subsystem: "io.cutout.mobile", category: "persistence")

    private static let cachedDatabase = Mutex<RideDatabaseHandle?>(nil)
    private static let openingAttempt = Mutex<OpenAttempt?>(nil)

    /// Reads an already-open handle; accessing this property never opens storage.
    public static var shared: RideDatabaseHandle? {
        cachedDatabase.withLock { $0 }
    }

    /// Opens the process-owned database off the main actor. Failed attempts remain retryable.
    public static func open() async throws -> RideDatabaseHandle {
        if let database = shared { return database }
        let attempt = openingAttempt.withLock { current in
            if let current { return current }
            let task = Task.detached(priority: .userInitiated) {
                try openDefaultDatabase()
            }
            let created = OpenAttempt(task: task)
            current = created
            return created
        }

        do {
            let database = try await attempt.task.value
            cachedDatabase.withLock { $0 = database }
            return database
        } catch {
            openingAttempt.withLock { current in
                if current === attempt {
                    current = nil
                }
            }
            throw error
        }
    }

    static func open(
        at databaseURL: URL,
        fileManager: FileManager = .default,
        openDatabase: @escaping @Sendable (String) throws -> RideDatabaseHandle = { try openRideDatabase(path: $0) }
    ) async throws -> RideDatabaseHandle {
        let fileManagerAccess = FileManagerAccess(fileManager)
        return try await Task.detached(priority: .userInitiated) {
            try prepareDatabase(at: databaseURL, fileManager: fileManagerAccess.manager, openDatabase: openDatabase)
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

    private static func prepareDatabase(
        at url: URL,
        fileManager: FileManager = .default,
        openDatabase: (String) throws -> RideDatabaseHandle = { try openRideDatabase(path: $0) }
    ) throws -> RideDatabaseHandle {
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
            // createDirectory does not update an existing directory. SQLite
            // sidecars must inherit protection that permits a locked ride.
            try fileManager.setAttributes(
                [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                ofItemAtPath: directory.path
            )
            do {
                try excludeFromBackup(at: directory)
            } catch {
                logger.error(
                    "Could not exclude ride database directory from backups: \(error, privacy: .public)"
                )
            }
            try protectExistingDatabaseFiles(at: databaseURL, fileManager: fileManager)
            let database = try openDatabase(databaseURL.path)
            try protectExistingDatabaseFiles(at: databaseURL, fileManager: fileManager)
            do {
                var values = URLResourceValues()
                values.isExcludedFromBackup = true
                try databaseURL.setResourceValues(values)
            } catch {
                logger.error("Could not exclude ride database from backups: \(error, privacy: .public)")
            }
            return database
        } catch {
            logger.error("Could not open Rust ride database: \(error, privacy: .public)")
            throw error
        }
    }

    private static func protectExistingDatabaseFiles(at url: URL, fileManager: FileManager) throws {
        for path in [url.path, url.path + "-wal", url.path + "-shm"] {
            guard fileManager.fileExists(atPath: path) else { continue }
            try fileManager.setAttributes(
                [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                ofItemAtPath: path
            )
        }
    }

    static func excludeFromBackup(at url: URL) throws {
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var mutableURL = url
        try mutableURL.setResourceValues(values)
    }
}
