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

    func testRouteOwnsTheSameTabsUsedByWindowCommandsAndContent() {
        XCTAssertTrue(CutoutAppRoute.devicePicker.navigationTabs.isEmpty)
        XCTAssertTrue(CutoutAppRoute.capture.navigationTabs.isEmpty)
        XCTAssertEqual(CutoutAppRoute.eucRide.navigationTabs.map(\.id), [.ride, .pack, .map, .tune])
        XCTAssertEqual(CutoutAppRoute.vescRide.navigationTabs.map(\.id), [.ride, .debug, .map, .logs])
        XCTAssertTrue(
            CutoutAppRoute.eucPack(.bmsOverview).navigationTabs.first(where: { $0.id == .pack })?.isSelected == true
        )
        XCTAssertTrue(
            CutoutAppRoute.vescDebug.navigationTabs.first(where: { $0.id == .debug })?.isSelected == true
        )
        XCTAssertEqual(CutoutAppRoute.route(forNavigationTarget: .vescRide), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(forNavigationTarget: .screen(.bmsOverview)), .eucPack(.bmsOverview))
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .ride), "1")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .pack), "2")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .map), "3")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .tune), "4")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .debug), "5")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .logs), "6")
    }

    func testOnlyLiveConnectionOpensTheRideSurface() {
        XCTAssertFalse(SessionConnectionPhase.connecting(model: .falcon).opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.discoveringServices.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.subscribing.opensRideScreen)
        XCTAssertTrue(SessionConnectionPhase.live.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.starting.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.bluetoothUnavailable(rawState: 0).opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.scanning.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.failed(.missingNotifyChannel).opensRideScreen)
    }

    func testEveryTelemetryRouteRequiresALiveSession() {
        XCTAssertFalse(CutoutAppRoute.devicePicker.requiresLiveSession)
        XCTAssertFalse(CutoutAppRoute.capture.requiresLiveSession)
        XCTAssertTrue(CutoutAppRoute.eucRide.requiresLiveSession)
        XCTAssertTrue(CutoutAppRoute.eucPack(.bmsOverview).requiresLiveSession)
        XCTAssertTrue(CutoutAppRoute.vescRide.requiresLiveSession)
        XCTAssertTrue(CutoutAppRoute.vescDebug.requiresLiveSession)
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
