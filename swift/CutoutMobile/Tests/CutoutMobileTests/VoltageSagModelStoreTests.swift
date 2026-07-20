import Foundation
import XCTest
@testable import CutoutMobile

final class VoltageSagModelStoreTests: XCTestCase {
    func testModelIsScopedToOneStableDeviceIdentity() throws {
        let suiteName = "VoltageSagModelStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = VoltageSagModelStore(defaults: defaults)
        let model = MobileVoltageSagModelDto(
            schemaVersion: 1,
            effectiveResistanceMilliohms: 125,
            observations: 7,
            hardwareVerified: true
        )

        store.save(model, for: "wheel-a")

        let restored = try XCTUnwrap(store.load(for: "wheel-a"))
        XCTAssertEqual(restored.schemaVersion, 1)
        XCTAssertEqual(restored.effectiveResistanceMilliohms, 125)
        XCTAssertEqual(restored.observations, 7)
        XCTAssertTrue(restored.hardwareVerified)
        XCTAssertNil(store.load(for: "wheel-b"))
    }
}
