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

    func testUuidIsNotSavedAsDeviceName() {
        let suiteName = "DevicePickerSelectionStoreTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = DevicePickerSelectionStore(defaults: defaults)
        store.save(platformIdentifier: "ios-local-aero", displayName: "ios-local-aero")

        XCTAssertNil(store.displayName(for: "ios-local-aero"))
    }
}
