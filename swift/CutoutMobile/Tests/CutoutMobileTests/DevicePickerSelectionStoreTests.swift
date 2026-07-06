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

        store.save(platformIdentifier: "ios-local-aero")
        XCTAssertEqual(store.platformIdentifier, "ios-local-aero")

        store.save(platformIdentifier: "ios-local-falcon")
        XCTAssertEqual(store.platformIdentifier, "ios-local-falcon")

        store.save(platformIdentifier: "   ")
        XCTAssertEqual(store.platformIdentifier, "ios-local-falcon")

        store.clear()
        XCTAssertNil(store.platformIdentifier)
    }
}
