import CutoutMobile
import XCTest

@testable import CutoutApp

@MainActor
final class RideMapRouteTruthTests: XCTestCase {
    func testBoundedRoutePreviewNoticeCoversPointAndSegmentOmissionOnlyForPresentRoutes() {
        XCTAssertTrue(
            RideMapRouteTruthView.shouldShowRoutePreview(
                routeIsPresent: true,
                pointsOmittedByBudget: true,
                segmentsOmittedByBudget: false
            ),
            "A point-only bounded projection is still an incomplete route preview"
        )
        XCTAssertTrue(
            RideMapRouteTruthView.shouldShowRoutePreview(
                routeIsPresent: true,
                pointsOmittedByBudget: false,
                segmentsOmittedByBudget: true
            )
        )
        XCTAssertTrue(
            RideMapRouteTruthView.shouldShowRoutePreview(
                routeIsPresent: true,
                pointsOmittedByBudget: true,
                segmentsOmittedByBudget: true
            )
        )
        XCTAssertFalse(
            RideMapRouteTruthView.shouldShowRoutePreview(
                routeIsPresent: true,
                pointsOmittedByBudget: false,
                segmentsOmittedByBudget: false
            )
        )
        XCTAssertFalse(
            RideMapRouteTruthView.shouldShowRoutePreview(
                routeIsPresent: false,
                pointsOmittedByBudget: true,
                segmentsOmittedByBudget: true
            ),
            "An empty route must not show an incomplete-preview notice"
        )
    }
}
