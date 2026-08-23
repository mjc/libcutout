import CutoutMobileFFI
import Foundation
import OSLog

/// The one Rust-owned SQLite service used by the mobile persistence adapters.
enum RustPersistenceStore {
    private static let logger = Logger(subsystem: "io.cutout.mobile", category: "persistence")

    static let shared: RideDatabaseHandle? = {
        let fileManager = FileManager.default
        guard
            let applicationSupport = fileManager.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first
        else {
            return nil
        }
        let directory = applicationSupport.appendingPathComponent("Cutout", isDirectory: true)
        var databaseURL = directory.appendingPathComponent("ride.sqlite")
        do {
            try fileManager.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [
                    .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication
                ]
            )
            let database = try RideDatabaseHandle.open(path: databaseURL.path)
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
            return nil
        }
    }()
}
