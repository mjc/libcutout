import CutoutMobileFFI
import Foundation

/// The only state shape permitted to cross the persistence-to-restore boundary.
public struct LightingAccessoryRestoreCandidate: Equatable, Sendable {
    public let platformIdentifier: String
    public let requestedState: MobileMelkLightingRestoreStateDto

    init(platformIdentifier: String, requestedState: MobileMelkLightingRestoreStateDto) {
        self.platformIdentifier = platformIdentifier
        self.requestedState = requestedState
    }
}

/// Rust-backed persistence for the selected Aero-installed MELK controller.
///
/// The store owns the versioned record and its one-time migration boundary. UI models only
/// coordinate transport events and render the typed values exposed here.
public final class LightingAccessoryPersistence {
    private struct Envelope: Codable {
        let record: Data
        let capabilitiesFingerprint: String?
    }

    private enum LoadResult {
        case missing
        case valid(MobileRgbLightingAccessoryRecord, fingerprint: String?)
        case invalid
    }

    private enum Key {
        static let record = "lighting.accessory.record"
        static let capabilitiesFingerprint = "lighting.accessory.capabilitiesFingerprint"
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
    private var recordCapabilitiesFingerprint: String?
    public private(set) var lastPersistenceError: String?

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        lastPersistenceError = nil
        switch Self.loadRecord(from: defaults) {
        case .missing:
            record = nil
            recordCapabilitiesFingerprint = nil
            migrateLegacyRecord()
        case let .valid(record, fingerprint):
            self.record = record
            recordCapabilitiesFingerprint = fingerprint
        case .invalid:
            record = nil
            recordCapabilitiesFingerprint = nil
        }
    }

    public var platformIdentifier: String? {
        record?.platformIdentifier()
    }

    public var alias: String? {
        record?.alias()
    }

    public var vehicleIdentifier: String? {
        record?.vehicleIdentifier()
    }

    /// Returns a restore request only when every safety prerequisite is satisfied.
    public func restoreCandidate() -> LightingAccessoryRestoreCandidate? {
        guard let record,
              let identifier = canonicalIdentifier(record.platformIdentifier()),
              record.restoreEnabled(),
              isCompatibleWithCurrentProfile,
              record.confirmation() == .confirmed,
              let confirmedState = record.confirmedState(),
              record.requestedState() == confirmedState else {
            return nil
        }
        return LightingAccessoryRestoreCandidate(
            platformIdentifier: identifier,
            requestedState: confirmedState
        )
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

    /// The profile identity and capability set that were present when this accessory was paired.
    /// A missing fingerprint intentionally makes restore ineligible rather than guessing that an
    /// old record is still safe after a profile change.
    public var isCompatibleWithCurrentProfile: Bool {
        guard let record,
              record.profile() == .melkOc21,
              record.profileVersion() == Self.currentProfileVersion,
              let fingerprint = recordCapabilitiesFingerprint else {
            return false
        }
        return fingerprint == Self.currentCapabilitiesFingerprint
    }

    public static var currentProfileVersion: UInt16 {
        mobileMelkLightingProfileVersion()
    }

    private static var currentCapabilitiesFingerprint: String {
        mobileMelkLightingCapabilitiesFingerprint()
    }

    public var presets: [MobileRgbLightingPresetDto] {
        record?.presets() ?? []
    }

    /// Creates a fresh record for a different connected identity.
    /// - Returns: `true` only when a new record was created.
    @discardableResult
    public func ensureRecord(platformIdentifier: String) -> Bool {
        guard let platformIdentifier = canonicalIdentifier(platformIdentifier) else {
            return false
        }
        if let existing = record?.platformIdentifier(),
           canonicalIdentifier(existing) == platformIdentifier {
            backfillMissingCapabilitiesFingerprint()
            return false
        }
        guard let newRecord = try? MobileRgbLightingAccessoryRecord(
            platformIdentifier: platformIdentifier,
            profile: .melkOc21,
            profileVersion: Self.currentProfileVersion
        ) else {
            return false
        }
        record = newRecord
        recordCapabilitiesFingerprint = Self.currentCapabilitiesFingerprint
        persist()
        return true
    }

    /// Repairs records written before capability fingerprints were introduced. A present
    /// fingerprint is never replaced here: a stale value must fail closed until the user
    /// explicitly re-pairs after profile evidence changes.
    private func backfillMissingCapabilitiesFingerprint() {
        guard record?.profile() == .melkOc21,
              record?.profileVersion() == Self.currentProfileVersion,
              recordCapabilitiesFingerprint == nil else {
            return
        }
        recordCapabilitiesFingerprint = Self.currentCapabilitiesFingerprint
        persist()
    }

    public func setConnection(_ state: MobileRgbLightingConnectionStateDto) {
        record?.setConnection(state: state)
        persist()
    }

    public func setRestoreEnabled(_ enabled: Bool) {
        record?.setRestoreEnabled(enabled: enabled)
        persist()
    }

    public func setAlias(_ alias: String?) throws {
        guard let record else { return }
        try record.setAlias(alias: alias)
        persist()
    }

    public func setVehicleIdentifier(_ identifier: String?) throws {
        guard let record else { return }
        try record.setVehicleIdentifier(identifier: identifier)
        persist()
    }

    /// Forgets the selected accessory and removes all restore-capable state.
    public func forget() {
        record = nil
        defaults.removeObject(forKey: Key.record)
        defaults.removeObject(forKey: Key.capabilitiesFingerprint)
        Key.legacy.forEach(defaults.removeObject(forKey:))
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

    /// Merges one confirmed partial command into the complete restore baseline.
    ///
    /// Returns `false` when this accessory has no complete baseline or the field does not match
    /// its stored playback mode. The requested state remains untouched until every changed field
    /// has been independently confirmed.
    @discardableResult
    public func confirmPartialState(
        _ state: MobileMelkLightingRestoreStateDto,
        field: MobileRgbLightingPartialStateDto
    ) throws -> Bool {
        guard let record else { return false }
        let merged = try record.confirmPartialState(state: state, field: field)
        if merged { persist() }
        return merged
    }

    public func markUnconfirmed() {
        record?.setConfirmation(state: .unconfirmed)
        persist()
    }

    /// Persists optional user-observed success without changing the requested lighting state.
    public func markConfirmed() {
        record?.setConfirmation(state: .confirmed)
        persist()
    }

    public func addPreset(
        name: String,
        requested: MobileMelkLightingRestoreStateDto
    ) throws {
        guard let record else { return }
        try record.addPreset(name: name, requested: requested)
        persist()
    }

    /// Removes a named app scene and persists the updated record.
    @discardableResult
    public func removePreset(named name: String) -> Bool {
        guard let record, record.removePreset(name: name) else { return false }
        persist()
        return true
    }

    /// Replaces a named app scene and persists the updated record.
    @discardableResult
    public func replacePreset(
        named name: String,
        requested: MobileMelkLightingRestoreStateDto
    ) throws -> Bool {
        guard let record else { return false }
        let replaced = try record.replacePreset(name: name, requested: requested)
        if replaced { persist() }
        return replaced
    }


    private static func loadRecord(from defaults: UserDefaults) -> LoadResult {
        guard let data = defaults.data(forKey: Key.record) else { return .missing }
        if let envelope = try? JSONDecoder().decode(Envelope.self, from: data) {
            guard let record = try? MobileRgbLightingAccessoryRecord.decode(bytes: envelope.record) else {
                return .invalid
            }
            guard canonicalIdentifier(record.platformIdentifier()) != nil else {
                return .invalid
            }
            return .valid(record, fingerprint: envelope.capabilitiesFingerprint)
        }
        guard let record = try? MobileRgbLightingAccessoryRecord.decode(bytes: data) else {
            return .invalid
        }
        guard canonicalIdentifier(record.platformIdentifier()) != nil else {
            return .invalid
        }
        // Raw records predate the atomic envelope. Never trust their separate fingerprint key.
        return .valid(record, fingerprint: nil)
    }

    private static func legacyByte(_ defaults: UserDefaults, key: String) -> UInt8? {
        guard defaults.object(forKey: key) != nil else { return nil }
        let value = defaults.integer(forKey: key)
        guard (0...255).contains(value) else { return nil }
        return UInt8(value)
    }

    private func migrateLegacyRecord() {
        guard let rawIdentifier = defaults.string(forKey: Key.legacyPlatformIdentifier),
              let identifier = canonicalIdentifier(rawIdentifier),
              let migrated = try? MobileRgbLightingAccessoryRecord(
                  platformIdentifier: identifier,
                  profile: .melkOc21,
                  profileVersion: Self.currentProfileVersion
              ) else {
            return
        }

        guard defaults.object(forKey: Key.legacyPowerOn) != nil,
              defaults.object(forKey: Key.legacyEnabled) != nil,
              let red = Self.legacyByte(defaults, key: Key.legacyRed),
              let green = Self.legacyByte(defaults, key: Key.legacyGreen),
              let blue = Self.legacyByte(defaults, key: Key.legacyBlue),
              let brightness = Self.legacyByte(defaults, key: Key.legacyBrightness),
              brightness <= 100 else {
            return
        }

        let state = MobileMelkLightingRestoreStateDto(
            powerOn: defaults.bool(forKey: Key.legacyPowerOn),
            red: red,
            green: green,
            blue: blue,
            brightness: brightness
        )
        do {
            try migrated.setRequestedState(state: state)
        } catch {
            return
        }
        // Legacy values are requests, not controller acknowledgement evidence.
        migrated.setConfirmation(state: .unknown)
        migrated.setRestoreEnabled(enabled: defaults.bool(forKey: Key.legacyEnabled))
        record = migrated
        recordCapabilitiesFingerprint = nil
        if persist() {
            Key.legacy.forEach(defaults.removeObject(forKey:))
        }
    }

    @discardableResult
    private func persist() -> Bool {
        guard let record else { return false }
        do {
            let envelope = Envelope(
                record: try record.encode(),
                capabilitiesFingerprint: recordCapabilitiesFingerprint
            )
            defaults.set(try JSONEncoder().encode(envelope), forKey: Key.record)
            defaults.removeObject(forKey: Key.capabilitiesFingerprint)
            lastPersistenceError = nil
            return true
        } catch {
            lastPersistenceError = String(describing: error)
            return false
        }
    }
}

private func canonicalIdentifier(_ identifier: String) -> String? {
    UUID(uuidString: identifier)?.uuidString
}
