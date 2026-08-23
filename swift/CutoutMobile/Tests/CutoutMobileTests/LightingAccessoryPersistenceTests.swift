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
        defaults.set("legacy-melk", forKey: "lighting.restore.platformIdentifier")
        defaults.set(true, forKey: "lighting.restore.powerOn")
        defaults.set(12, forKey: "lighting.restore.red")
        defaults.set(34, forKey: "lighting.restore.green")
        defaults.set(56, forKey: "lighting.restore.blue")
        defaults.set(78, forKey: "lighting.restore.brightness")

        let store = LightingAccessoryPersistence(defaults: defaults)

        XCTAssertEqual(store.platformIdentifier, "legacy-melk")
        XCTAssertTrue(store.restoreEnabled)
        XCTAssertEqual(store.requestedState?.red, 12)
        XCTAssertEqual(store.requestedState?.green, 34)
        XCTAssertEqual(store.requestedState?.blue, 56)
        XCTAssertEqual(store.requestedState?.brightness, 78)
        XCTAssertEqual(store.confirmation, .confirmed)
        XCTAssertNil(defaults.string(forKey: "lighting.restore.platformIdentifier"))
        XCTAssertNotNil(defaults.data(forKey: "lighting.accessory.record"))
    }

    func testStoreReopensCanonicalRecordAndSeparatesRequestedFromConfirmedState() throws {
        let suiteName = "LightingAccessoryPersistenceTests-roundtrip-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let first = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(first.ensureRecord(platformIdentifier: "canonical-melk"))
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
        XCTAssertEqual(reopened.platformIdentifier, "canonical-melk")
        XCTAssertEqual(reopened.requestedState?.red, 255)
        XCTAssertEqual(reopened.confirmedState?.blue, 24)
        XCTAssertEqual(reopened.confirmation, .confirmed)
    }

    func testStoreReplacesRecordWhenTheConnectedIdentityChanges() throws {
        let suiteName = "LightingAccessoryPersistenceTests-identity-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "first-melk"))
        try store.confirm(MobileMelkLightingRestoreStateDto(
            powerOn: true,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 4
        ))

        XCTAssertFalse(store.ensureRecord(platformIdentifier: "first-melk"))
        XCTAssertTrue(store.ensureRecord(platformIdentifier: "second-melk"))
        XCTAssertEqual(store.platformIdentifier, "second-melk")
        XCTAssertNil(store.confirmedState)
        XCTAssertEqual(store.confirmation, .unknown)
    }
}
