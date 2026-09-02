import Foundation
import CutoutMobileFFI

public struct DevicePickerSelectionStore {
    private static let key = "io.cutout.devicePicker.selectedPlatformIdentifier"
    private static let deviceNameKeyPrefix = "io.cutout.devicePicker.deviceName."
    private let defaults: UserDefaults
    private let database: RideDatabaseHandle?

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.database = defaults === UserDefaults.standard ? RustPersistenceStore.shared : nil
    }

    init(database: RideDatabaseHandle, defaults: UserDefaults = .standard) {
        self.defaults = defaults
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
                let normalizedIdentifier = legacy.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !normalizedIdentifier.isEmpty else {
                    defaults.removeObject(forKey: Self.key)
                    return nil
                }
                let legacyName = defaults.string(
                    forKey: Self.deviceNameKeyPrefix + normalizedIdentifier
                )
                if (try? database.rememberSelectedDevice(
                    platformIdentifier: legacy,
                    displayName: legacyName,
                    updatedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)
                )) != nil {
                    defaults.removeObject(forKey: Self.key)
                    defaults.removeObject(
                        forKey: Self.deviceNameKeyPrefix + normalizedIdentifier
                    )
                }
                return normalizedIdentifier
            }
            return try? database.selectedDevice()
        }
        return defaults.string(forKey: Self.key)
    }

    public func displayName(for platformIdentifier: String) -> String? {
        let trimmed = platformIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let legacyKey = Self.deviceNameKeyPrefix + trimmed
        if let database {
            if let persisted = try? database.deviceName(platformIdentifier: trimmed) {
                defaults.removeObject(forKey: legacyKey)
                return persisted
            }
        }
        if let database, let legacy = defaults.string(forKey: legacyKey) {
            do {
                let migrated = try database.migrateDeviceName(
                    platformIdentifier: trimmed,
                    displayName: legacy,
                    updatedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)
                )
                defaults.removeObject(forKey: legacyKey)
                return migrated
            } catch {
                return nil
            }
        }
        if let legacy = defaults.string(forKey: legacyKey) {
            do {
                let normalized = try normalizeDeviceDisplayName(
                    platformIdentifier: trimmed,
                    displayName: legacy
                )
                if let normalized {
                    return normalized
                }
                defaults.removeObject(forKey: legacyKey)
            } catch {
                return nil
            }
        }
        return nil
    }

    public func save(platformIdentifier: String, displayName: String? = nil) {
        let trimmed = platformIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        let updatedAtMilliseconds = UInt64(Date().timeIntervalSince1970 * 1_000)
        if let database {
            if (try? database.rememberSelectedDevice(
                platformIdentifier: trimmed,
                displayName: displayName,
                updatedAtMilliseconds: updatedAtMilliseconds
            )) != nil {
                defaults.removeObject(forKey: Self.key)
                defaults.removeObject(forKey: Self.deviceNameKeyPrefix + trimmed)
            } else {
                defaults.set(trimmed, forKey: Self.key)
                if let displayName {
                    defaults.set(displayName, forKey: Self.deviceNameKeyPrefix + trimmed)
                }
            }
            return
        }
        defaults.set(trimmed, forKey: Self.key)
        if let displayName {
            do {
                let normalized = try normalizeDeviceDisplayName(
                    platformIdentifier: trimmed,
                    displayName: displayName
                )
                if let normalized {
                    defaults.set(normalized, forKey: Self.deviceNameKeyPrefix + trimmed)
                } else {
                    defaults.removeObject(forKey: Self.deviceNameKeyPrefix + trimmed)
                }
            } catch {
                defaults.removeObject(forKey: Self.deviceNameKeyPrefix + trimmed)
            }
        }
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
