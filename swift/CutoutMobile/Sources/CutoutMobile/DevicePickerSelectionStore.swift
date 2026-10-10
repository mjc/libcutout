import CutoutMobileFFI
import Foundation

public struct DevicePickerSelectionSnapshot: Equatable, Sendable {
    public let platformIdentifier: String?
    public let displayName: String?

    public init(platformIdentifier: String?, displayName: String?) {
        self.platformIdentifier = platformIdentifier
        self.displayName = displayName
    }
}

/// Immutable handles; UserDefaults supports concurrent access and Rust owns database serialization.
public struct DevicePickerSelectionStore: @unchecked Sendable {
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

    public var requiresDatabaseLoad: Bool { database != nil }

    /// Injected preference-only stores can seed presentation without a database round trip.
    public var immediateSelection: DevicePickerSelectionSnapshot {
        guard database == nil else {
            return DevicePickerSelectionSnapshot(platformIdentifier: nil, displayName: nil)
        }
        let identifier = platformIdentifier
        return DevicePickerSelectionSnapshot(
            platformIdentifier: identifier, displayName: identifier.flatMap { displayName(for: $0) })
    }

    public func load() async -> DevicePickerSelectionSnapshot {
        await Task.detached(priority: .utility) { [self] in
            let identifier = platformIdentifier
            return DevicePickerSelectionSnapshot(
                platformIdentifier: identifier, displayName: identifier.flatMap { displayName(for: $0) })
        }.value
    }

    public func loadDisplayName(for identifier: String) async -> String? {
        await Task.detached(priority: .utility) { [self] in displayName(for: identifier) }.value
    }

    public func immediateDisplayName(for identifier: String) -> String? {
        guard database == nil else { return nil }
        return displayName(for: identifier)
    }

    /// Protocol naming must not change the user's saved device selection.
    public func saveDisplayName(_ name: String, for identifier: String) {
        let key = Self.deviceNameKeyPrefix + identifier
        if let database {
            if (try? database.saveDeviceName(
                platformIdentifier: identifier, displayName: name,
                updatedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000))) != nil
            {
                defaults.removeObject(forKey: key)
                return
            }
        }
        if let normalized = try? normalizeDeviceDisplayName(platformIdentifier: identifier, displayName: name) {
            defaults.set(normalized, forKey: key)
        } else {
            defaults.removeObject(forKey: key)
        }
    }

    public func saveDisplayNameAsync(
        _ name: String, for identifier: String, didFinish: @escaping @Sendable () -> Void = {}
    ) async {
        await Task.detached(priority: .utility) { [self] in
            defer { didFinish() }
            guard !Task.isCancelled else { return }
            saveDisplayName(name, for: identifier)
        }.value
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
