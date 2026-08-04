import Foundation
import XCTest
@testable import CutoutApp

final class AppLocalizationTests: XCTestCase {
    func testCatalogFixtureFormatsHighlightedBmsAccessibilityValue() throws {
        let fixture = try AppLocalizationBundleFixture(strings: """
        "bms.group.accessibility.highlighted" = "Highlighted: %1$@";
        """)
        defer { fixture.remove() }

        XCTAssertEqual(
            localizedAppText(
                "bms.group.accessibility.highlighted",
                arguments: ["Voltage: 4.036"],
                bundle: fixture.bundle,
                locale: Locale(identifier: "en_US")
            ),
            "Highlighted: Voltage: 4.036"
        )
    }
}

private struct AppLocalizationBundleFixture {
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
