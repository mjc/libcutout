import Foundation
import XCTest

@testable import CutoutMobile

final class DevicePickerSelectionStoreTests: XCTestCase {
    func testVehicleKindUsesExistingDatabaseAndDoesNotChangeNamesOrSelection() throws {
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        let suiteName = "DevicePickerSelectionStoreTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let first = "controller-a-\(UUID().uuidString)"
        let second = "controller-b-\(UUID().uuidString)"
        let selected = try database.selectedDevice()
        let store = DevicePickerSelectionStore(database: database, defaults: defaults)
        XCTAssertNil(try store.userVehicleKind(for: first))

        try database.saveUserVehicleKind(platformIdentifier: first, kind: .eskate, updatedAtMilliseconds: 1)
        try database.saveUserVehicleKind(platformIdentifier: second, kind: .scooter, updatedAtMilliseconds: 2)
        let restored = DevicePickerSelectionStore(database: database, defaults: defaults)
        XCTAssertEqual(try restored.userVehicleKind(for: first), .eskate)
        XCTAssertEqual(try restored.userVehicleKind(for: second), .scooter)
        XCTAssertEqual(try database.selectedDevice(), selected)
        XCTAssertNil(try database.deviceName(platformIdentifier: first))
        XCTAssertThrowsError(
            try database.saveUserVehicleKind(platformIdentifier: first, kind: .unknown, updatedAtMilliseconds: 3))
        XCTAssertEqual(try restored.userVehicleKind(for: first), .eskate)
        XCTAssertTrue((defaults.persistentDomain(forName: suiteName) ?? [:]).isEmpty)
    }

    func testVehicleKindDoesNotInventPreferenceOnlyPersistence() {
        let suiteName = "DevicePickerSelectionStoreTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = DevicePickerSelectionStore(defaults: defaults)
        XCTAssertThrowsError(try store.userVehicleKind(for: "controller")) {
            XCTAssertEqual($0 as? DeviceVehicleSelectionError, .persistenceUnavailable)
        }
    }

    func testSelectedDeviceStorePersistsAndClearsPlatformIdentifier() {
        let suiteName = "DevicePickerSelectionStoreTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = DevicePickerSelectionStore(defaults: defaults)

        XCTAssertNil(store.platformIdentifier)

        store.save(platformIdentifier: "ios-local-aero", displayName: "NF2557")
        XCTAssertEqual(store.platformIdentifier, "ios-local-aero")
        XCTAssertEqual(store.displayName(for: "ios-local-aero"), "NF2557")

        store.save(platformIdentifier: "ios-local-falcon", displayName: "NF2557")
        XCTAssertEqual(store.platformIdentifier, "ios-local-falcon")
        XCTAssertEqual(store.displayName(for: "ios-local-falcon"), "NF2557")

        store.save(platformIdentifier: "   ")
        XCTAssertEqual(store.platformIdentifier, "ios-local-falcon")

        XCTAssertNoThrow(try store.clear())
        XCTAssertNil(store.platformIdentifier)
        XCTAssertEqual(store.displayName(for: "ios-local-aero"), "NF2557")
    }

    func testPlatformIdentifierIsNotSavedAsDeviceName() {
        let suiteName = "DevicePickerSelectionStoreTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = DevicePickerSelectionStore(defaults: defaults)
        store.save(platformIdentifier: "ios-local-aero", displayName: "ios-local-aero")

        XCTAssertNil(store.displayName(for: "ios-local-aero"))
    }

    func testLegacyDeviceNameIsRemovedAfterDatabaseMigration() throws {
        let suiteName = "DevicePickerSelectionStoreTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let platformIdentifier = "ios-local-aero-\(UUID().uuidString)"
        let key = "io.cutout.devicePicker.deviceName.\(platformIdentifier)"
        let database = try XCTUnwrap(MobileRideMapState.debugDatabase)
        defaults.set("NF2557", forKey: key)
        let store = DevicePickerSelectionStore(database: database, defaults: defaults)

        XCTAssertEqual(store.displayName(for: platformIdentifier), "NF2557")
        XCTAssertNil(defaults.string(forKey: key))
        XCTAssertEqual(store.displayName(for: platformIdentifier), "NF2557")
    }

    func testInvalidLegacyDeviceNameIsRemovedDuringLookup() {
        let suiteName = "DevicePickerSelectionStoreTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let platformIdentifier = "ios-local-aero-\(UUID().uuidString)"
        let key = "io.cutout.devicePicker.deviceName.\(platformIdentifier)"
        defaults.set("  \(platformIdentifier)  ", forKey: key)

        let store = DevicePickerSelectionStore(defaults: defaults)

        XCTAssertNil(store.displayName(for: platformIdentifier))
        XCTAssertNil(defaults.string(forKey: key))
    }
}
