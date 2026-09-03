import CutoutMobile
import XCTest

@testable import CutoutApp

final class RideMapPresentationTests: XCTestCase {
    func testLiveMapOnlyShowsRecordedBoundsForPersistedTerminalStates() {
        let summary = MobileRideMapSummaryDto(
            pointCount: 0,
            distanceMeters: 0,
            durationMilliseconds: 0
        )
        let snapshot = { (state: MobileRideMapStateDto, available: Bool) in
            MobileRideMapSnapshotDto(
                rideID: "ride",
                state: state,
                summary: summary,
                segmentCount: 0,
                associatedVehicle: nil,
                recordedBoundsAvailable: available
            )
        }

        XCTAssertFalse(snapshot(.draft, false).recordedBoundsAvailable)
        XCTAssertFalse(snapshot(.active, false).recordedBoundsAvailable)
        XCTAssertFalse(snapshot(.paused, false).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.stopped, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.interrupted, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.saved, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.discarded, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.imported, true).recordedBoundsAvailable)
    }

    func testHistoryPresentationUsesSingularAndPluralPointStrings() {
        XCTAssertEqual(RideMapHistoryListView.pointCountText(1), "1 point")
        XCTAssertEqual(RideMapHistoryListView.pointCountText(2), "2 points")
    }

    func testHistorySelectionAccessibilityTextIsLocalized() {
        XCTAssertEqual(
            RideMapHistoryListView.selectionAccessibilityValue(isSelected: true),
            "Selected"
        )
        XCTAssertEqual(
            RideMapHistoryListView.selectionAccessibilityValue(isSelected: false),
            "Not selected"
        )
    }

    func testRideMapStringsResolveFromTheAppCatalog() {
        XCTAssertEqual(localizedAppText("ride_map.status.recording"), "Recording")
        XCTAssertEqual(localizedAppText("ride_map.start"), "Start GPS-only ride")
        XCTAssertEqual(localizedAppText("ride_map.stop"), "Stop")
        XCTAssertEqual(localizedAppText("ride_map.history_recent"), "Recent rides")
        XCTAssertEqual(localizedAppText("ride_map.vehicle_name_unavailable"), "Vehicle name unavailable")
    }
}
