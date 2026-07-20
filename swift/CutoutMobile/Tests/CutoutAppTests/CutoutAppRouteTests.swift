import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppRouteTests: XCTestCase {
    deinit {}

    func testScreenRoutesMatchTopLevelSections() {
        XCTAssertEqual(CutoutAppRoute.route(for: .eucRide), .eucRide)
        XCTAssertEqual(CutoutAppRoute.route(for: .vescRide), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsOverview), .eucPack(.bmsOverview))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap6S), .eucPack(.bmsCellMap6S))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap40S), .eucPack(.bmsCellMap40S))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellDetail), .eucPack(.bmsCellDetail))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsUnknownTopology), .eucPack(.bmsUnknownTopology))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsNoData), .eucPack(.bmsNoData))
        XCTAssertEqual(CutoutAppRoute.route(for: .eucGarage), .eucPack(.eucGarage))
        XCTAssertEqual(CutoutAppRoute.route(for: .vescDebug), .vescDebug)
    }

    func testRouteFocusIdentityDistinguishesEveryDestination() {
        let routes: Set<CutoutAppRoute> = [
            .devicePicker,
            .eucRide,
            .eucPack(.bmsOverview),
            .eucPack(.bmsCellMap6S),
            .eucPack(.bmsCellMap40S),
            .eucPack(.bmsCellDetail),
            .eucPack(.bmsUnknownTopology),
            .eucPack(.bmsNoData),
            .eucPack(.eucGarage),
            .vescRide,
            .vescDebug,
            .capture,
        ]

        XCTAssertEqual(routes.count, 12)
    }

    func testConnectionRoutesMatchRideSections() {
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.electricUnicycle), .eucRide)
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.vescOnewheel), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(for: nil), .devicePicker)
    }

    func testNavigationPathKeepsPickerAtRootAndReplacesConnectedDestinations() {
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .devicePicker), [])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .eucRide), [.eucRide])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .eucPack(.bmsOverview)), [.eucPack(.bmsOverview)])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .vescDebug), [.vescDebug])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .capture), [.capture])
    }

    func testPairingProgressOpensTheRideSurface() {
        XCTAssertTrue(SessionConnectionPhase.connecting(model: .falcon).opensRideScreen)
        XCTAssertTrue(SessionConnectionPhase.discoveringServices.opensRideScreen)
        XCTAssertTrue(SessionConnectionPhase.subscribing.opensRideScreen)
        XCTAssertTrue(SessionConnectionPhase.live.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.starting.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.scanning.opensRideScreen)
    }

    func testConnectionAnnouncementsCoverMeaningfulTransitionsWithoutChatter() {
        XCTAssertNil(SessionConnectionPhase.starting.accessibilityAnnouncement)
        XCTAssertNil(SessionConnectionPhase.scanning.accessibilityAnnouncement)
        XCTAssertNil(SessionConnectionPhase.discoveringServices.accessibilityAnnouncement)
        XCTAssertNil(SessionConnectionPhase.subscribing.accessibilityAnnouncement)
        XCTAssertEqual(
            SessionConnectionPhase.bluetoothUnavailable(rawState: 4).accessibilityAnnouncement,
            "Bluetooth unavailable. Turn on Bluetooth to reconnect."
        )
        XCTAssertEqual(
            SessionConnectionPhase.connecting(model: .falcon).accessibilityAnnouncement,
            "Connecting to Falcon."
        )
        XCTAssertEqual(SessionConnectionPhase.live.accessibilityAnnouncement, "Connected.")
        XCTAssertEqual(
            SessionConnectionPhase.failed(.connectFailed("timed out")).accessibilityAnnouncement,
            "Connection lost. Retrying. Connect failed: timed out"
        )
    }

    func testSafetyAnnouncementsCoverTypedEscalationsWithoutTelemetryChatter() {
        XCTAssertNil(EucRideWarningSeverity.normal.accessibilityAnnouncement)
        XCTAssertEqual(
            EucRideWarningSeverity.caution.accessibilityAnnouncement,
            "Caution. Riding headroom is getting low."
        )
        XCTAssertEqual(
            EucRideWarningSeverity.reduceAcceleration.accessibilityAnnouncement,
            "Warning. Reduce acceleration."
        )
        XCTAssertEqual(
            EucRideWarningSeverity.limpHome.accessibilityAnnouncement,
            "Critical warning. Slow down and stop safely."
        )
        XCTAssertNil(EucRideWarningSeverity.unavailable.accessibilityAnnouncement)
        XCTAssertNil(EucRideWarningSeverity.failed.accessibilityAnnouncement)

        XCTAssertNil(VescRideWarning.none.accessibilityAnnouncement)
        XCTAssertEqual(VescRideWarning.pushbackSoon.accessibilityAnnouncement, "Warning. Pushback soon.")
        XCTAssertNil(VescRideWarning.unknown.accessibilityAnnouncement)
    }

    func testBmsAnnouncementUsesHighestTypedGroupSeverity() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "test",
                seriesGroupCount: 3,
                parallelCount: 1,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            groups: [
                BmsGroupSnapshot(index: 0, alertLevel: .nominal),
                BmsGroupSnapshot(index: 1, alertLevel: .critical),
                BmsGroupSnapshot(index: 2, alertLevel: .warning),
            ]
        )

        XCTAssertEqual(snapshot.accessibilityAlertLevel, .critical)
        XCTAssertEqual(
            snapshot.accessibilityAlertLevel.accessibilityAnnouncement,
            "Critical battery warning. Check BMS details."
        )
        XCTAssertNil(BmsAlertLevel.nominal.accessibilityAnnouncement)
        XCTAssertNil(BmsAlertLevel.unknown.accessibilityAnnouncement)
    }

}
