import Foundation
import CutoutMobileFFI

public struct DevicePickerSelectionStore {
    private static let key = "io.cutout.devicePicker.selectedPlatformIdentifier"
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

    public var platformIdentifier: String? {
        if let database {
            if let legacy = defaults.string(forKey: Self.key) {
                if legacy.isEmpty {
                    if (try? database.clearSelectedDevice()) != nil {
                        defaults.removeObject(forKey: Self.key)
                    }
                    return nil
                }
                if (try? database.saveSelectedDevice(
                    platformIdentifier: legacy,
                    updatedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)
                )) != nil {
                    defaults.removeObject(forKey: Self.key)
                }
                return legacy
            }
            if let persisted = try? database.selectedDevice() {
                return persisted
            }
            return nil
        }
        return defaults.string(forKey: Self.key)
    }

    public func save(platformIdentifier: String) {
        let trimmed = platformIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        if let database {
            if (try? database.saveSelectedDevice(
                platformIdentifier: trimmed,
                updatedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)
            )) != nil {
                defaults.removeObject(forKey: Self.key)
            } else {
                defaults.set(trimmed, forKey: Self.key)
            }
            return
        }
        defaults.set(trimmed, forKey: Self.key)
    }

    public func clear() throws {
        if let database {
            do {
                try database.clearSelectedDevice()
                defaults.removeObject(forKey: Self.key)
            } catch {
                defaults.set("", forKey: Self.key)
                throw error
            }
            return
        }
        defaults.removeObject(forKey: Self.key)
    }
}
