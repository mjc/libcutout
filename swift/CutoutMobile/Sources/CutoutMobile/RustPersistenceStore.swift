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

    private struct CaptureFileDeduplicationState {
        var isRunning = false
        var rerunRequested = false
    }

    private static let logger = Logger(subsystem: "io.cutout.mobile", category: "persistence")

    private static let cachedDatabase = Mutex<RideDatabaseHandle?>(nil)
    private static let openingAttempt = Mutex<OpenAttempt?>(nil)
    private static let captureFileDeduplicationState = Mutex(CaptureFileDeduplicationState())

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

    /// Reconciles managed original and share-cache files against Rust's exact SQLite-backed bytes.
    /// The database is opened off the main actor and Rust owns path validation and deletion proof.
    public static func deduplicateManagedCaptureFiles() async {
        let shouldStart = captureFileDeduplicationState.withLock { state in
            guard !state.isRunning else {
                state.rerunRequested = true
                return false
            }
            state.isRunning = true
            return true
        }
        guard shouldStart else { return }

        repeat {
            await deduplicateManagedCaptureFilesPass()
        } while captureFileDeduplicationState.withLock({ state in
            guard state.rerunRequested else {
                state.isRunning = false
                return false
            }
            state.rerunRequested = false
            return true
        })
    }

    private static func deduplicateManagedCaptureFilesPass() async {
        #if DEBUG
            if ChargingAcceptanceEnvironment.databaseID != nil
                || ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"] != nil
            {
                return
            }
        #endif
        let fileManager = FileManager.default
        var directories: [String] = []
        if let documents = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first {
            directories.append(documents.path)
        }
        directories.append(
            fileManager.temporaryDirectory.appendingPathComponent("CaptureExports", isDirectory: true).path
        )
        guard !directories.isEmpty else {
            logger.error("Could not locate app-owned directories for managed capture-file cleanup")
            return
        }
        do {
            let database = try await open()
            let result = try await Task.detached(priority: .utility) {
                var removedFiles: UInt64 = 0
                var removedBytes: UInt64 = 0
                for directory in directories {
                    let pass = try database.deduplicateManagedCaptureFiles(directoryPath: directory)
                    removedFiles += pass.removedFiles
                    removedBytes += pass.removedBytes
                }
                return (removedFiles, removedBytes)
            }.value
            if result.0 > 0 {
                logger.info(
                    "Removed \(result.0, privacy: .public) duplicate managed capture files (\(result.1, privacy: .public) bytes)"
                )
            }
        } catch {
            logger.error("Managed capture-file cleanup failed: \(error, privacy: .public)")
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
        #if DEBUG
            if let id = ChargingAcceptanceEnvironment.databaseID {
                let directory = fileManager.temporaryDirectory
                    .appendingPathComponent("CutoutChargingAcceptance", isDirectory: true)
                return try prepareDatabase(at: directory.appendingPathComponent("\(id.uuidString).sqlite"))
            }
        #endif
        #if DEBUG && targetEnvironment(simulator)
            if let rawID = ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"],
                let id = UUID(uuidString: rawID)
            {
                let directory = fileManager.temporaryDirectory
                    .appendingPathComponent("CutoutUITestRideMaps", isDirectory: true)
                return try prepareDatabase(at: directory.appendingPathComponent("\(id.uuidString).sqlite"))
            }
        #endif
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

#if DEBUG
    /// An explicit developer launch uses isolated storage and never sends community traffic.
    public enum ChargingAcceptanceEnvironment {
        public static var databaseID: UUID? {
            databaseID(arguments: CommandLine.arguments, environment: ProcessInfo.processInfo.environment)
        }

        static func databaseID(arguments: [String], environment: [String: String]) -> UUID? {
            guard arguments.contains("--charging-acceptance"),
                let value = environment["CUTOUT_CHARGING_ACCEPTANCE_DATABASE_ID"]
            else { return nil }
            return UUID(uuidString: value)
        }
    }
#endif
