import XCTest
@testable import CutoutApp
import CutoutMobile

final class EucReadbackTests: XCTestCase {
    func testSettingsRowsKeepFieldIdentityWhenOrderChanges() {
        let first = SettingsReadbackEntry(
            field: RawSettingField(id: 42, value: 1_234),
            source: .reported,
            quality: .known,
            verification: .sourceVerified
        )
        let second = SettingsReadbackEntry(
            field: RawSettingField(id: 7, value: 900),
            source: .calculated,
            quality: .inferred,
            verification: .unverified
        )

        let forward = SettingsReadback(entries: [first, second], availability: .available)
        let reversed = SettingsReadback(entries: [second, first], availability: .available)

        XCTAssertEqual(forward.dashboardRows.map(\.id), ["setting-42", "setting-7"])
        XCTAssertEqual(reversed.dashboardRows.map(\.id), ["setting-7", "setting-42"])
    }
}
