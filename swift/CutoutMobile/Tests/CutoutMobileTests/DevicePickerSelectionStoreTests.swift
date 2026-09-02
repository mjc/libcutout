import Foundation
import XCTest
@testable import CutoutMobile

final class DevicePickerSelectionStoreTests: XCTestCase {
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
        guard let database = RustPersistenceStore.shared else {
            throw XCTSkip("Rust ride database is unavailable in this test environment")
        }
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
