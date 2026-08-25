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

    public func displayName(for platformIdentifier: String) -> String? {
        let trimmed = platformIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        if let database,
           let persisted = try? database.deviceName(platformIdentifier: trimmed),
           !persisted.isEmpty {
            return persisted
        }
        if let legacy = defaults.string(forKey: Self.deviceNameKeyPrefix + trimmed),
           let normalized = normalizedDisplayName(legacy, platformIdentifier: trimmed) {
            if let database {
                _ = try? database.saveDeviceName(
                    platformIdentifier: trimmed,
                    displayName: normalized,
                    updatedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)
                )
            }
            return normalized
        }
        return nil
    }

    public func save(platformIdentifier: String, displayName: String? = nil) {
        let trimmed = platformIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        let updatedAtMilliseconds = UInt64(Date().timeIntervalSince1970 * 1_000)
        if let normalizedName = normalizedDisplayName(displayName, platformIdentifier: trimmed) {
            if let database {
                if (try? database.saveDeviceName(
                    platformIdentifier: trimmed,
                    displayName: normalizedName,
                    updatedAtMilliseconds: updatedAtMilliseconds
                )) != nil {
                    defaults.removeObject(forKey: Self.deviceNameKeyPrefix + trimmed)
                } else {
                    defaults.set(normalizedName, forKey: Self.deviceNameKeyPrefix + trimmed)
                }
            } else {
                defaults.set(normalizedName, forKey: Self.deviceNameKeyPrefix + trimmed)
            }
        }

        if let database {
            if (try? database.saveSelectedDevice(
                platformIdentifier: trimmed,
                updatedAtMilliseconds: updatedAtMilliseconds
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

    private func normalizedDisplayName(
        _ displayName: String?,
        platformIdentifier: String
    ) -> String? {
        guard let displayName else { return nil }
        let trimmed = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, trimmed != platformIdentifier else { return nil }
        return trimmed
    }
}
