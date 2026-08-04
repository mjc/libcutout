import Foundation
import XCTest
@testable import CutoutMobile

final class PevLocalizationTests: XCTestCase {
    func testCatalogFixtureReordersBmsAccessibilityArguments() throws {
        let fixture = try LocalizationBundleFixture(strings: """
        "bms.accessibility.group_named" = "Named %2$@ — cell group %1$lld";
        "bms.accessibility.voltage" = "Voltage: %1$@";
        """)
        defer { fixture.remove() }

        XCTAssertEqual(
            pevLocalizedText(
                "bms.accessibility.group_named",
                arguments: [Int64(7), "right pack group 7"],
                bundle: fixture.bundle,
                locale: Locale(identifier: "en_US")
            ),
            "Named right pack group 7 — cell group 7"
        )
        XCTAssertEqual(
            pevLocalizedText(
                "bms.accessibility.voltage",
                arguments: ["4.036"],
                bundle: fixture.bundle,
                locale: Locale(identifier: "en_US")
            ),
            "Voltage: 4.036"
        )
    }
}

private struct LocalizationBundleFixture {
    let root: URL
    let bundle: Bundle

    init(strings: String) throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: UUID().uuidString, directoryHint: .isDirectory)
            .appendingPathExtension("bundle")
        let localization = root.appending(path: "en.lproj", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: localization, withIntermediateDirectories: true)
        try strings.write(to: localization.appending(path: "Localizable.strings"), atomically: true, encoding: .utf8)
        self.root = root
        bundle = try XCTUnwrap(Bundle(url: root))
    }

    func remove() {
        try? FileManager.default.removeItem(at: root)
    }
}
