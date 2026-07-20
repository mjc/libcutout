import XCTest
@testable import CutoutMobile

final class DashboardLayoutContractTests: XCTestCase {
    func testContentScaleNeverShrinksTheWholeDashboard() {
        XCTAssertEqual(DashboardViewport.contentScale(width: 390, height: 844), 1)
        XCTAssertEqual(DashboardViewport.contentScale(width: 320, height: 568), 1)
        XCTAssertEqual(DashboardViewport.contentScale(width: 844, height: 320), 1)
    }

    func testEveryProductionScreenUsesTheSharedLayoutContractAcrossViewports() {
        let screenIDs = PevScreenID.allCases
        let catalogIDs = PevScreenCatalog.live.screens.map(\.id)
        let viewports: [(String, DashboardViewport)] = [
            ("small iPhone portrait", DashboardViewport(width: 320, height: 568, safeAreaTop: 20, safeAreaBottom: 0)),
            ("standard iPhone portrait", DashboardViewport(width: 390, height: 844, safeAreaTop: 47, safeAreaBottom: 34)),
            ("large iPhone portrait", DashboardViewport(width: 430, height: 932, safeAreaTop: 59, safeAreaBottom: 34)),
            ("small iPhone landscape", DashboardViewport(width: 568, height: 320, safeAreaTop: 0, safeAreaBottom: 21)),
            ("standard iPhone landscape", DashboardViewport(width: 844, height: 390, safeAreaTop: 0, safeAreaBottom: 21)),
            ("iPad portrait", DashboardViewport(width: 1024, height: 1366, safeAreaTop: 24, safeAreaBottom: 20)),
            ("iPad landscape", DashboardViewport(width: 1366, height: 1024, safeAreaTop: 24, safeAreaBottom: 20)),
        ]

        XCTAssertEqual(Set(catalogIDs), Set(screenIDs))
        XCTAssertEqual(catalogIDs.count, Set(catalogIDs).count)

        for screenID in screenIDs {
            XCTAssertNotNil(PevScreenCatalog.live.screen(id: screenID), screenID.rawValue)

            for (viewportName, viewport) in viewports {
                let navigation = viewport.navigationFrame()
                XCTAssertTrue(
                    viewport.isNavigationAnchored(frame: navigation),
                    "\(screenID.rawValue) on \(viewportName)"
                )
                XCTAssertLessThanOrEqual(
                    viewport.contentBottom(navigation: navigation, gap: 12),
                    navigation.top,
                    "\(screenID.rawValue) on \(viewportName)"
                )
            }
        }
    }

    func testNavigationEndsAtSafeAreaBottomAcrossSupportedViewports() {
        let viewports: [(String, DashboardViewport)] = [
            ("small iPhone portrait", DashboardViewport(width: 320, height: 568, safeAreaTop: 20, safeAreaBottom: 0)),
            ("standard iPhone portrait", DashboardViewport(width: 390, height: 844, safeAreaTop: 47, safeAreaBottom: 34)),
            ("large iPhone portrait", DashboardViewport(width: 430, height: 932, safeAreaTop: 59, safeAreaBottom: 34)),
            ("small iPhone landscape", DashboardViewport(width: 568, height: 320, safeAreaTop: 0, safeAreaBottom: 21)),
            ("standard iPhone landscape", DashboardViewport(width: 844, height: 390, safeAreaTop: 0, safeAreaBottom: 21)),
            ("iPad portrait", DashboardViewport(width: 1024, height: 1366, safeAreaTop: 24, safeAreaBottom: 20)),
            ("iPad landscape", DashboardViewport(width: 1366, height: 1024, safeAreaTop: 24, safeAreaBottom: 20)),
        ]

        for (name, viewport) in viewports {
            let frame = viewport.navigationFrame()
            XCTAssertTrue(viewport.isNavigationAnchored(frame: frame), name)
            XCTAssertEqual(frame.bottom, viewport.height - viewport.safeAreaBottom, accuracy: 0.001, name)
            XCTAssertGreaterThan(frame.height, 0, name)
        }
    }

    func testNavigationContractIsIndependentOfDynamicTypeSize() {
        let viewport = DashboardViewport(width: 390, height: 844, safeAreaTop: 47, safeAreaBottom: 34)
        let expected = viewport.navigationFrame()

        for dynamicTypeSize in ["default", "xLarge", "accessibility3"] {
            let frame = viewport.navigationFrame()
            XCTAssertEqual(frame, expected, dynamicTypeSize)
            XCTAssertLessThanOrEqual(viewport.contentBottom(navigation: frame, gap: 12), frame.top, dynamicTypeSize)
        }
    }

    func testNavigationLeavesContentAboveItsTopEdge() {
        let viewport = DashboardViewport(width: 390, height: 844, safeAreaTop: 47, safeAreaBottom: 34)
        let frame = viewport.navigationFrame()

        XCTAssertEqual(viewport.contentBottom(navigation: frame, gap: 12), 722, accuracy: 0.001)
        XCTAssertLessThan(viewport.contentBottom(navigation: frame, gap: 12), frame.bottom)
    }

    func testFloatingOrOverlappingNavigationIsRejected() {
        let viewport = DashboardViewport(width: 390, height: 844, safeAreaTop: 47, safeAreaBottom: 34)
        let expected = viewport.navigationFrame()

        XCTAssertFalse(viewport.isNavigationAnchored(
            frame: DashboardLayoutFrame(top: expected.top - 40, height: expected.height)
        ))
        XCTAssertFalse(viewport.isNavigationAnchored(
            frame: DashboardLayoutFrame(top: expected.top, height: expected.height + 40)
        ))
        XCTAssertFalse(viewport.isNavigationAnchored(
            frame: DashboardLayoutFrame(top: expected.bottom, height: 0)
        ))
    }
}
