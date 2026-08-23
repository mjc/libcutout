import CutoutMobileFFI
import Foundation

/// Rust-backed persistence for the selected standalone MELK accessory.
///
/// The store owns the versioned record and its one-time migration boundary. UI models only
/// coordinate transport events and render the typed values exposed here.
public final class LightingAccessoryPersistence {
    private enum Key {
        static let record = "lighting.accessory.record"
        static let legacyEnabled = "lighting.restore.enabled"
        static let legacyPlatformIdentifier = "lighting.restore.platformIdentifier"
        static let legacyPowerOn = "lighting.restore.powerOn"
        static let legacyRed = "lighting.restore.red"
        static let legacyGreen = "lighting.restore.green"
        static let legacyBlue = "lighting.restore.blue"
        static let legacyBrightness = "lighting.restore.brightness"

        static let legacy: [String] = [
            legacyEnabled,
            legacyPlatformIdentifier,
            legacyPowerOn,
            legacyRed,
            legacyGreen,
            legacyBlue,
            legacyBrightness,
        ]
    }

    private let defaults: UserDefaults
    private var record: MobileRgbLightingAccessoryRecord?

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        record = Self.loadRecord(from: defaults)
        if record == nil {
            migrateLegacyRecord()
        }
    }

    public var platformIdentifier: String? {
        record?.platformIdentifier()
    }

    public var requestedState: MobileMelkLightingRestoreStateDto? {
        record?.requestedState()
    }

    public var confirmedState: MobileMelkLightingRestoreStateDto? {
        record?.confirmedState()
    }

    public var confirmation: MobileRgbLightingConfirmationStateDto {
        record?.confirmation() ?? .unknown
    }

    public var restoreEnabled: Bool {
        record?.restoreEnabled() ?? false
    }

    /// Creates a fresh record for a different connected identity.
    /// - Returns: `true` only when a new record was created.
    @discardableResult
    public func ensureRecord(platformIdentifier: String) -> Bool {
        guard !platformIdentifier.isEmpty,
              record?.platformIdentifier() != platformIdentifier else {
            return false
        }
        guard let newRecord = try? MobileRgbLightingAccessoryRecord(
            platformIdentifier: platformIdentifier,
            profile: .melkOc21,
            profileVersion: 1
        ) else {
            return false
        }
        record = newRecord
        persist()
        return true
    }

    public func setConnection(_ state: MobileRgbLightingConnectionStateDto) {
        record?.setConnection(state: state)
        persist()
    }

    public func setRestoreEnabled(_ enabled: Bool) {
        record?.setRestoreEnabled(enabled: enabled)
        persist()
    }

    public func updateRequestedState(_ state: MobileMelkLightingRestoreStateDto) throws {
        guard let record else { return }
        try record.setRequestedState(state: state)
        record.setConfirmation(state: .unknown)
        persist()
    }

    public func confirm(_ state: MobileMelkLightingRestoreStateDto) throws {
        guard let record else { return }
        try record.setRequestedState(state: state)
        try record.setConfirmedState(state: state)
        record.setConfirmation(state: .confirmed)
        persist()
    }

    public func markUnconfirmed() {
        record?.setConfirmation(state: .unconfirmed)
        persist()
    }

    private static func loadRecord(from defaults: UserDefaults) -> MobileRgbLightingAccessoryRecord? {
        guard let data = defaults.data(forKey: Key.record) else { return nil }
        return try? MobileRgbLightingAccessoryRecord.decode(bytes: data)
    }

    private func migrateLegacyRecord() {
        guard let identifier = defaults.string(forKey: Key.legacyPlatformIdentifier),
              let migrated = try? MobileRgbLightingAccessoryRecord(
                  platformIdentifier: identifier,
                  profile: .melkOc21,
                  profileVersion: 1
              ) else {
            return
        }

        let state = MobileMelkLightingRestoreStateDto(
            powerOn: defaults.bool(forKey: Key.legacyPowerOn),
            red: UInt8(clamping: defaults.integer(forKey: Key.legacyRed)),
            green: UInt8(clamping: defaults.integer(forKey: Key.legacyGreen)),
            blue: UInt8(clamping: defaults.integer(forKey: Key.legacyBlue)),
            brightness: UInt8(clamping: defaults.integer(forKey: Key.legacyBrightness))
        )
        do {
            try migrated.setRequestedState(state: state)
            try migrated.setConfirmedState(state: state)
        } catch {
            return
        }
        migrated.setConfirmation(state: .confirmed)
        migrated.setRestoreEnabled(enabled: defaults.bool(forKey: Key.legacyEnabled))
        record = migrated
        persist()
        Key.legacy.forEach(defaults.removeObject(forKey:))
    }

    private func persist() {
        guard let record, let data = try? record.encode() else { return }
        defaults.set(data, forKey: Key.record)
    }
}
