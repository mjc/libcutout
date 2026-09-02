import CutoutMobile
import XCTest

@testable import CutoutApp

final class RideMapPresentationTests: XCTestCase {
    func testLiveMapOnlyShowsRecordedBoundsForPersistedTerminalStates() {
        XCTAssertFalse(RideMapLiveContentView.showsRecordedBounds(for: nil))
        XCTAssertFalse(RideMapLiveContentView.showsRecordedBounds(for: .draft))
        XCTAssertFalse(RideMapLiveContentView.showsRecordedBounds(for: .active))
        XCTAssertFalse(RideMapLiveContentView.showsRecordedBounds(for: .paused))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .stopped))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .interrupted))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .saved))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .discarded))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .imported))
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
