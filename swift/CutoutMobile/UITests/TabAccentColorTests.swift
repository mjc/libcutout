import UIKit
import XCTest

@MainActor
final class TabAccentColorTests: XCTestCase {
    func testPurpleResolvesOffMainActorInBothAppearances() async {
        let color = TabAccentColors.purple
        await Task.detached {
            XCTAssertFalse(Thread.isMainThread)
            let light = UITraitCollection(userInterfaceStyle: .light)
            let dark = UITraitCollection(userInterfaceStyle: .dark)
            XCTAssertEqual(
                color.resolvedColor(with: light),
                UIColor(red: 0.34, green: 0.08, blue: 0.52, alpha: 1)
            )
            XCTAssertEqual(color.resolvedColor(with: dark), UIColor.systemPurple.resolvedColor(with: dark))
        }.value
    }

    func testYellowResolvesOffMainActorInBothAppearances() async {
        let color = TabAccentColors.yellow
        await Task.detached {
            XCTAssertFalse(Thread.isMainThread)
            let light = UITraitCollection(userInterfaceStyle: .light)
            let dark = UITraitCollection(userInterfaceStyle: .dark)
            XCTAssertEqual(
                color.resolvedColor(with: light),
                UIColor(red: 0.45, green: 0.25, blue: 0.0, alpha: 1)
            )
            XCTAssertEqual(color.resolvedColor(with: dark), UIColor.systemYellow.resolvedColor(with: dark))
        }.value
    }
}
