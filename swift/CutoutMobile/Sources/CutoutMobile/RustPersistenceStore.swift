import CutoutMobileFFI
import Foundation

/// The one Rust-owned SQLite service used by the mobile persistence adapters.
enum RustPersistenceStore {
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
        do {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
            return try RideDatabaseHandle.open(
                path: directory.appendingPathComponent("ride.sqlite").path
            )
        } catch {
            return nil
        }
    }()
}
