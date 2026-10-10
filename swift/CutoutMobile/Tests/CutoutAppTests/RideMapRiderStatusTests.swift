import CutoutMobile
import XCTest

@testable import CutoutApp

@MainActor
final class RideMapRiderStatusTests: XCTestCase {
    func testCameraActionSelectionMatchesLifecycleAndAvailableRouteData() {
        for state in [MobileRideMapStateDto.active, .paused] {
            XCTAssertEqual(
                RideMapLiveContentView.cameraActionMode(
                    state: state,
                    hasPoints: true,
                    hasCameraRegion: true,
                    followsLatestPoint: true
                ),
                .following
            )
            XCTAssertEqual(
                RideMapLiveContentView.cameraActionMode(
                    state: state,
                    hasPoints: true,
                    hasCameraRegion: true,
                    followsLatestPoint: false
                ),
                .notFollowing
            )
        }

        for state in [MobileRideMapStateDto.stopped, .saved, .interrupted, .discarded, .imported] {
            for followsLatestPoint in [true, false] {
                XCTAssertEqual(
                    RideMapLiveContentView.cameraActionMode(
                        state: state,
                        hasPoints: true,
                        hasCameraRegion: true,
                        followsLatestPoint: followsLatestPoint
                    ),
                    .showFullRide
                )
            }
        }

        let unavailableStates: [MobileRideMapStateDto?] = [.draft, nil]
        for state in unavailableStates {
            XCTAssertEqual(
                RideMapLiveContentView.cameraActionMode(
                    state: state,
                    hasPoints: true,
                    hasCameraRegion: true,
                    followsLatestPoint: true
                ),
                .unavailable
            )
        }
        XCTAssertEqual(
            RideMapLiveContentView.cameraActionMode(
                state: .active,
                hasPoints: false,
                hasCameraRegion: true,
                followsLatestPoint: true
            ),
            .unavailable
        )
        XCTAssertEqual(
            RideMapLiveContentView.cameraActionMode(
                state: .stopped,
                hasPoints: true,
                hasCameraRegion: false,
                followsLatestPoint: true
            ),
            .unavailable
        )
    }

    func testLatestPointFollowGuardUsesOnlyActiveOrPausedLifecycle() {
        XCTAssertTrue(RideMapLiveContentView.supportsLatestPointFollow(state: .active))
        XCTAssertTrue(RideMapLiveContentView.supportsLatestPointFollow(state: .paused))

        for state in [MobileRideMapStateDto.stopped, .saved, .interrupted, .discarded, .imported, .draft] {
            XCTAssertFalse(RideMapLiveContentView.supportsLatestPointFollow(state: state))
        }
        XCTAssertFalse(RideMapLiveContentView.supportsLatestPointFollow(state: nil))
    }
}
