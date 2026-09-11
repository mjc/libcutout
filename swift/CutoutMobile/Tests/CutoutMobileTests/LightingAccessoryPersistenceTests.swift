import Foundation
import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class LightingAccessoryPersistenceTests: XCTestCase {
    func testStoreMigratesLegacyLightingKeysIntoTypedRecord() throws {
        let suiteName = "LightingAccessoryPersistenceTests-migration-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        defaults.set(true, forKey: "lighting.restore.enabled")
        defaults.set("11111111-1111-1111-1111-111111111111", forKey: "lighting.restore.platformIdentifier")
        defaults.set(true, forKey: "lighting.restore.powerOn")
        defaults.set(12, forKey: "lighting.restore.red")
        defaults.set(34, forKey: "lighting.restore.green")
        defaults.set(56, forKey: "lighting.restore.blue")
        defaults.set(78, forKey: "lighting.restore.brightness")

        let store = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertEqual(store.platformIdentifier, "11111111-1111-1111-1111-111111111111")
        XCTAssertTrue(store.restoreEnabled)
        XCTAssertEqual(store.requestedState?.red, 12)
        XCTAssertEqual(store.requestedState?.green, 34)
        XCTAssertEqual(store.requestedState?.blue, 56)
        XCTAssertEqual(store.requestedState?.brightness, 78)
        XCTAssertEqual(store.confirmation, .unknown)
        XCTAssertNil(store.confirmedState)
        XCTAssertNil(defaults.string(forKey: "lighting.restore.platformIdentifier"))
        XCTAssertNotNil(defaults.data(forKey: "lighting.accessory.record"))
    }

    func testStoreReopensCanonicalRecordAndSeparatesRequestedFromConfirmedState() throws {
        let suiteName = "LightingAccessoryPersistenceTests-roundtrip-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let first = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(first.ensureRecord(platformIdentifier: "22222222-2222-2222-2222-222222222222"))
        let requested = MobileMelkLightingRestoreStateDto(
            powerOn: true,
            red: 255,
            green: 96,
            blue: 24,
            brightness: 50
        )
        try first.updateRequestedState(requested)
        XCTAssertEqual(first.confirmation, .unknown)
        XCTAssertNil(first.confirmedState)

        try first.confirm(requested)
        XCTAssertEqual(first.confirmation, .confirmed)
        XCTAssertEqual(first.confirmedState?.brightness, 50)

        let reopened = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertEqual(reopened.platformIdentifier, "22222222-2222-2222-2222-222222222222")
        XCTAssertEqual(reopened.requestedState?.red, 255)
        XCTAssertEqual(reopened.confirmedState?.blue, 24)
        XCTAssertEqual(reopened.confirmation, .confirmed)
        XCTAssertTrue(reopened.isCompatibleWithCurrentProfile)
        XCTAssertNil(defaults.string(forKey: "lighting.accessory.capabilitiesFingerprint"))
    }

    func testStoreRejectsRestoreWhenCapabilityFingerprintIsStale() throws {
        let suiteName = "LightingAccessoryPersistenceTests-compatibility-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let rawRecord = try MobileRgbLightingAccessoryRecord(
            platformIdentifier: "33333333-3333-3333-3333-333333333333",
            profile: .melkOc21,
            profileVersion: LightingAccessoryPersistence.currentProfileVersion
        )
        defaults.set(try rawRecord.encode(), forKey: "lighting.accessory.record")
        defaults.set("stale-capabilities", forKey: "lighting.accessory.capabilitiesFingerprint")
        let reopened = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertFalse(reopened.isCompatibleWithCurrentProfile)
    }

    func testLegacySeparateCurrentFingerprintCannotBlessRawRecord() throws {
        let suiteName = "LightingAccessoryPersistenceTests-torn-record-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let rawRecord = try MobileRgbLightingAccessoryRecord(
            platformIdentifier: "3A3A3A3A-3A3A-3A3A-3A3A-3A3A3A3A3A3A",
            profile: .melkOc21,
            profileVersion: LightingAccessoryPersistence.currentProfileVersion
        )
        defaults.set(try rawRecord.encode(), forKey: "lighting.accessory.record")
        let capabilities = mobileMelkLightingCapabilities()
        let fingerprint = [
            capabilities.verifiedEffectIds.sorted().map(String.init).joined(separator: ","),
            capabilities.controllerMicrophone ? "1" : "0",
            capabilities.schedules ? "1" : "0",
            capabilities.addressableZones ? "1" : "0",
            capabilities.scenes ? "1" : "0",
        ].joined(separator: "|")
        defaults.set(fingerprint, forKey: "lighting.accessory.capabilitiesFingerprint")

        let reopened = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertFalse(reopened.isCompatibleWithCurrentProfile)
        XCTAssertNil(reopened.restoreCandidate())
    }

    func testStoreBackfillsMissingCapabilityFingerprintForExistingRecord() throws {
        let suiteName = "LightingAccessoryPersistenceTests-backfill-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let record = try MobileRgbLightingAccessoryRecord(
            platformIdentifier: "44444444-4444-4444-4444-444444444444",
            profile: .melkOc21,
            profileVersion: LightingAccessoryPersistence.currentProfileVersion
        )
        defaults.set(try record.encode(), forKey: "lighting.accessory.record")
        defaults.removeObject(forKey: "lighting.accessory.capabilitiesFingerprint")

        let reopened = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertFalse(reopened.isCompatibleWithCurrentProfile)
        XCTAssertFalse(reopened.ensureRecord(platformIdentifier: "44444444-4444-4444-4444-444444444444"))
        XCTAssertTrue(reopened.isCompatibleWithCurrentProfile)
    }

    func testStoreReplacesRecordWhenTheConnectedIdentityChanges() throws {
        let suiteName = "LightingAccessoryPersistenceTests-identity-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "55555555-5555-5555-5555-555555555555"))
        try store.confirm(MobileMelkLightingRestoreStateDto(
            powerOn: true,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 4
        ))

        XCTAssertFalse(store.ensureRecord(platformIdentifier: "55555555-5555-5555-5555-555555555555"))
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "66666666-6666-6666-6666-666666666666"))
        XCTAssertEqual(store.platformIdentifier, "66666666-6666-6666-6666-666666666666")
        XCTAssertNil(store.confirmedState)
        XCTAssertEqual(store.confirmation, .unknown)
    }

    func testStorePersistsNamedPresetsAndRejectsDuplicateNames() throws {
        let suiteName = "LightingAccessoryPersistenceTests-presets-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let state = MobileMelkLightingRestoreStateDto(
            powerOn: true,
            red: 9,
            green: 8,
            blue: 7,
            brightness: 60
        )
        let store = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "77777777-7777-7777-7777-777777777777"))
        try store.addPreset(name: "Night", requested: state)
        XCTAssertEqual(store.presets.map(\.name), ["Night"])
        XCTAssertEqual(store.presets.first?.requested, state)

        XCTAssertThrowsError(try store.addPreset(name: "Night", requested: state))
        let replacement = MobileMelkLightingRestoreStateDto(
            powerOn: false,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 10
        )
        XCTAssertTrue(try store.replacePreset(named: "Night", requested: replacement))
        XCTAssertEqual(store.presets.first?.requested, replacement)
        let reopened = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertEqual(reopened.presets.first?.requested, replacement)
        XCTAssertTrue(store.removePreset(named: "Night"))
        XCTAssertFalse(store.removePreset(named: "Night"))
        XCTAssertTrue(store.presets.isEmpty)
        XCTAssertTrue(LightingAccessoryPersistence(defaults: defaults).presets.isEmpty)
    }

    func testStorePersistsAliasAndVehicleAssociationAcrossReopen() throws {
        let suiteName = "LightingAccessoryPersistenceTests-metadata-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "88888888-8888-8888-8888-888888888888"))
        try store.setAlias("Under-seat LEDs")
        try store.setVehicleIdentifier("euc-aero")

        let reopened = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertEqual(reopened.alias, "Under-seat LEDs")
        XCTAssertEqual(reopened.vehicleIdentifier, "euc-aero")
    }

    func testStoreForgetRemovesRecordAndPreventsRestoreOnReopen() throws {
        let suiteName = "LightingAccessoryPersistenceTests-forget-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "99999999-9999-9999-9999-999999999999"))
        try store.setAlias("Temporary")
        try store.setVehicleIdentifier("euc-aero")
        store.setRestoreEnabled(true)

        store.forget()

        XCTAssertNil(store.platformIdentifier)
        XCTAssertNil(store.alias)
        XCTAssertNil(store.vehicleIdentifier)
        XCTAssertFalse(store.restoreEnabled)
        XCTAssertNil(defaults.data(forKey: "lighting.accessory.record"))
        XCTAssertNil(LightingAccessoryPersistence(defaults: defaults).platformIdentifier)
    }

    func testStoreRejectsOutOfRangeLegacyChannelsWithoutClamping() throws {
        let suiteName = "LightingAccessoryPersistenceTests-invalid-legacy-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        defaults.set(true, forKey: "lighting.restore.enabled")
        defaults.set("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", forKey: "lighting.restore.platformIdentifier")
        defaults.set(true, forKey: "lighting.restore.powerOn")
        defaults.set(-1, forKey: "lighting.restore.red")
        defaults.set(0, forKey: "lighting.restore.green")
        defaults.set(0, forKey: "lighting.restore.blue")
        defaults.set(100, forKey: "lighting.restore.brightness")

        let store = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertNil(store.platformIdentifier)
        XCTAssertNil(defaults.data(forKey: "lighting.accessory.record"))
        XCTAssertEqual(defaults.integer(forKey: "lighting.restore.red"), -1)
    }

    func testCorruptCanonicalRecordDoesNotResurrectLegacyState() throws {
        let suiteName = "LightingAccessoryPersistenceTests-corrupt-canonical-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        defaults.set(Data([0xff, 0x00]), forKey: "lighting.accessory.record")
        defaults.set(true, forKey: "lighting.restore.enabled")
        defaults.set("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", forKey: "lighting.restore.platformIdentifier")
        defaults.set(true, forKey: "lighting.restore.powerOn")
        defaults.set(1, forKey: "lighting.restore.red")
        defaults.set(2, forKey: "lighting.restore.green")
        defaults.set(3, forKey: "lighting.restore.blue")
        defaults.set(4, forKey: "lighting.restore.brightness")

        let store = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertNil(store.platformIdentifier)
        XCTAssertNotNil(defaults.data(forKey: "lighting.accessory.record"))
        XCTAssertNotNil(defaults.string(forKey: "lighting.restore.platformIdentifier"))
    }

    func testFutureCanonicalProfileVersionDoesNotResurrectLegacyState() throws {
        let suiteName = "LightingAccessoryPersistenceTests-future-canonical-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let record = try MobileRgbLightingAccessoryRecord(
            platformIdentifier: "BFBFBFBF-BFBF-BFBF-BFBF-BFBFBFBFBFBF",
            profile: .melkOc21,
            profileVersion: LightingAccessoryPersistence.currentProfileVersion
        )
        var wire = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(try record.encode())) as? [String: Any]
        )
        wire["profile_version"] = Int(LightingAccessoryPersistence.currentProfileVersion) + 1
        defaults.set(try JSONSerialization.data(withJSONObject: wire), forKey: "lighting.accessory.record")
        defaults.set(true, forKey: "lighting.restore.enabled")
        defaults.set("cfcfcfcf-cfcf-cfcf-cfcf-cfcfcfcfcfcf", forKey: "lighting.restore.platformIdentifier")
        defaults.set(true, forKey: "lighting.restore.powerOn")
        defaults.set(1, forKey: "lighting.restore.red")
        defaults.set(2, forKey: "lighting.restore.green")
        defaults.set(3, forKey: "lighting.restore.blue")
        defaults.set(4, forKey: "lighting.restore.brightness")

        let store = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertNil(store.platformIdentifier)
        XCTAssertNotNil(defaults.string(forKey: "lighting.restore.platformIdentifier"))
    }

    func testCanonicalRecordWithInvalidIdentifierDoesNotLoad() throws {
        let suiteName = "LightingAccessoryPersistenceTests-invalid-canonical-identifier-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let record = try MobileRgbLightingAccessoryRecord(
            platformIdentifier: "legacy-melk",
            profile: .melkOc21,
            profileVersion: LightingAccessoryPersistence.currentProfileVersion
        )
        defaults.set(try record.encode(), forKey: "lighting.accessory.record")

        let store = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertNil(store.platformIdentifier)
    }

    func testRestoreCandidateRequiresTheCompleteSafetyContract() throws {
        let suiteName = "LightingAccessoryPersistenceTests-restore-candidate-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "CCCCCCCC-CCCC-CCCC-CCCC-CCCCCCCCCCCC"))
        let state = MobileMelkLightingRestoreStateDto(
            powerOn: true, red: 1, green: 2, blue: 3, brightness: 4
        )
        try store.updateRequestedState(state)
        XCTAssertNil(store.restoreCandidate())

        store.setRestoreEnabled(true)
        let candidate = try XCTUnwrap(store.restoreCandidate())
        XCTAssertEqual(candidate.platformIdentifier, "CCCCCCCC-CCCC-CCCC-CCCC-CCCCCCCCCCCC")
        XCTAssertEqual(candidate.requestedState, state)
    }

    func testInvalidLegacyIdentifierDoesNotMigrate() throws {
        let suiteName = "LightingAccessoryPersistenceTests-invalid-legacy-identifier-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        defaults.set(true, forKey: "lighting.restore.enabled")
        defaults.set("legacy-melk", forKey: "lighting.restore.platformIdentifier")
        defaults.set(true, forKey: "lighting.restore.powerOn")
        defaults.set(1, forKey: "lighting.restore.red")
        defaults.set(2, forKey: "lighting.restore.green")
        defaults.set(3, forKey: "lighting.restore.blue")
        defaults.set(4, forKey: "lighting.restore.brightness")

        let store = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertNil(store.platformIdentifier)
        XCTAssertNil(defaults.data(forKey: "lighting.accessory.record"))
        XCTAssertNotNil(defaults.string(forKey: "lighting.restore.platformIdentifier"))
    }
}
