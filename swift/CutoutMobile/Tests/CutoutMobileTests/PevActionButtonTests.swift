import SwiftUI
import XCTest
@testable import CutoutMobile

@MainActor
final class PevActionButtonTests: XCTestCase {
    func testHitTargetNeverShrinksBelowFortyFourPoints() {
        let compact = makeLabel(width: 32, height: 30)

        XCTAssertEqual(compact.hitWidth, 44)
        XCTAssertEqual(compact.hitHeight, 44)
    }

    func testLargerRequestedHitTargetIsPreserved() {
        let large = makeLabel(width: 80, height: 52)

        XCTAssertEqual(large.hitWidth, 80)
        XCTAssertEqual(large.hitHeight, 52)
    }

    func testFlexibleWidthRemainsFlexible() {
        XCTAssertNil(makeLabel(width: nil, height: 36).hitWidth)
    }

    private func makeLabel(width: CGFloat?, height: CGFloat) -> PevActionButtonLabel {
        PevActionButtonLabel(
            title: "Action",
            systemImageName: "record.circle",
            scale: 1,
            isEnabled: true,
            fillsAvailableWidth: width == nil,
            width: width,
            height: height,
            cornerRadius: 8,
            horizontalPadding: 0,
            iconSpacing: 8,
            foregroundEnabled: .primary,
            foregroundDisabled: .secondary,
            fillEnabled: .clear,
            fillDisabled: .clear,
            strokeEnabled: .clear,
            strokeDisabled: .clear
        )
    }
}
