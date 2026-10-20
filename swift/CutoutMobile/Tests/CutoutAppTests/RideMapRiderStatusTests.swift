import MapKit
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

@MainActor
final class RideMapRiderStatusTests: XCTestCase {
    func testEmptyLiveRouteCanUseOnlyAValidPhoneLocationForItsMarkerAndCamera() {
        let location = CLLocationCoordinate2D(latitude: 40, longitude: -105)
        let invalid = CLLocationCoordinate2D(latitude: .infinity, longitude: -105)

        let marker = RideMapLiveContentView.phoneLocationAnnotationCoordinate(
            routePoints: [], location: location)
        XCTAssertEqual(marker?.latitude, location.latitude)
        XCTAssertEqual(marker?.longitude, location.longitude)
        let camera = RideMapLiveContentView.emptyRouteCameraRegion(
            location: location,
            span: RideMapPresentationState.defaultLocalSpan
        )
        XCTAssertEqual(camera?.center.latitude, location.latitude)
        XCTAssertEqual(camera?.center.longitude, location.longitude)
        XCTAssertNil(RideMapLiveContentView.phoneLocationAnnotationCoordinate(routePoints: [], location: nil))
        XCTAssertNil(RideMapLiveContentView.phoneLocationAnnotationCoordinate(routePoints: [], location: invalid))
        XCTAssertNil(
            RideMapLiveContentView.emptyRouteCameraRegion(
                location: nil,
                span: RideMapPresentationState.defaultLocalSpan))
        XCTAssertNil(
            RideMapLiveContentView.emptyRouteCameraRegion(
                location: invalid,
                span: RideMapPresentationState.defaultLocalSpan))

        let routePoint = MobileRideMapRouteDisplayPoint(
            sequence: 1,
            segmentId: 1,
            latitudeDegrees: 39,
            longitudeDegrees: -104,
            privacyClass: .precise
        )
        XCTAssertNil(
            RideMapLiveContentView.phoneLocationAnnotationCoordinate(
                routePoints: [routePoint], location: location))
    }

    func testPhoneOnlyLocationEnablesFollowForActiveRideButNotTerminalRide() {
        let activeStates: [MobileRideMapStateDto] = [.active, .paused]
        for state in activeStates {
            XCTAssertEqual(
                RideMapLiveContentView.cameraActionMode(
                    state: state,
                    hasPoints: false,
                    hasCameraRegion: false,
                    followsLatestPoint: true,
                    hasUserLocation: true
                ),
                .following
            )
        }
        XCTAssertEqual(
            RideMapLiveContentView.cameraActionMode(
                state: .paused,
                hasPoints: false,
                hasCameraRegion: false,
                followsLatestPoint: false,
                hasUserLocation: true
            ),
            .notFollowing
        )
        for state in [MobileRideMapStateDto.stopped, .saved, .interrupted, .discarded, .imported] {
            XCTAssertEqual(
                RideMapLiveContentView.cameraActionMode(
                    state: state,
                    hasPoints: false,
                    hasCameraRegion: false,
                    followsLatestPoint: true,
                    hasUserLocation: true
                ),
                .unavailable
            )
        }
        let unavailableLocations: [CLLocationCoordinate2D?] = [
            nil,
            CLLocationCoordinate2D(latitude: .infinity, longitude: -105),
        ]
        for location in unavailableLocations {
            XCTAssertNil(RideMapCanvasView.validatedCoordinate(location))
            XCTAssertNil(
                RideMapCanvasView.emptyRouteDefaultRegion(
                    center: location,
                    span: RideMapPresentationState.defaultLocalSpan))
        }
    }

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
