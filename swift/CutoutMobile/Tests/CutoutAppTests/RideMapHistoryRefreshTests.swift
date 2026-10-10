import XCTest

@testable import CutoutApp

final class RideMapHistoryRefreshTests: XCTestCase {
    @MainActor
    func testRetainedResultsUseRetryableErrorAfterRefreshFailure() {
        XCTAssertEqual(
            RideMapHistoryContentView.retainedRefreshStatus(
                isLoading: true,
                hasResults: true,
                hasHistoryError: true,
                hasRouteError: true,
                hasSelectedRide: true
            ),
            .retryableError
        )
    }

    @MainActor
    func testOverviewStatusKeepsInitialLoadingRouteLoadingAndRouteLessStatesDistinct() {
        XCTAssertEqual(
            RideMapHistoryContentView.retainedRefreshStatus(
                isLoading: true,
                hasResults: false,
                hasHistoryError: false,
                hasRouteError: false,
                hasSelectedRide: false
            ),
            .hidden,
            "Initial loading is rendered in the full history loading state"
        )
        XCTAssertEqual(
            RideMapHistoryContentView.retainedRefreshStatus(
                isLoading: false,
                hasResults: true,
                hasHistoryError: false,
                hasRouteError: true,
                hasSelectedRide: false
            ),
            .hidden,
            "A route error without a matching selected ride must not attach Retry to retained results"
        )
        XCTAssertEqual(
            RideMapHistoryContentView.retainedRefreshStatus(
                isLoading: false,
                hasResults: true,
                hasHistoryError: false,
                hasRouteError: false,
                hasSelectedRide: true
            ),
            .hidden,
            "A confirmed route-less ride has no refresh error"
        )
    }

    @MainActor
    func testRetainedRouteProjectionErrorUsesRetryableErrorStatus() {
        XCTAssertEqual(
            RideMapHistoryContentView.retainedRefreshStatus(
                isLoading: false,
                hasResults: true,
                hasHistoryError: false,
                hasRouteError: true,
                hasSelectedRide: true
            ),
            .retryableError
        )
    }
}
