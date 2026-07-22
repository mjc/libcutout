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
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellDetail), .eucPack(.bmsCellDetail(nil)))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsUnknownTopology), .eucPack(.bmsUnknownTopology))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsNoData), .eucPack(.bmsNoData))
        XCTAssertEqual(CutoutAppRoute.route(for: .vescDebug), .vescDebug)
    }

    func testEucPackRouteRejectsNonPackScreens() {
        XCTAssertNil(EucPackScreen(screenID: .vescRide))
        XCTAssertNil(EucPackScreen(screenID: .vescDebug))
        XCTAssertEqual(EucPackScreen(screenID: .bmsOverview), .bmsOverview)
    }

    func testRouteFocusIdentityDistinguishesEveryDestination() {
        let routes: Set<CutoutAppRoute> = [
            .devicePicker,
            .eucRide,
            .eucPack(.bmsOverview),
            .eucPack(.bmsCellMap6S),
            .eucPack(.bmsCellMap40S),
            .eucPack(.bmsCellDetail(nil)),
            .eucPack(.bmsUnknownTopology),
            .eucPack(.bmsNoData),
            .eucPack(.root),
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
            CutoutAppRoute.eucPack(.root).navigationTabs.first(where: { $0.id == .pack })?.isSelected == true
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

    func testNativeNavigationOmitsUnavailableDestinations() {
        XCTAssertEqual(CutoutAppRoute.eucRide.availableNavigationTabs.map(\.id), [.ride, .pack])
        XCTAssertEqual(CutoutAppRoute.eucPack(.bmsOverview).availableNavigationTabs.map(\.id), [.ride, .pack])
        XCTAssertEqual(CutoutAppRoute.vescRide.availableNavigationTabs.map(\.id), [.ride, .debug])
        XCTAssertEqual(CutoutAppRoute.vescDebug.availableNavigationTabs.map(\.id), [.ride, .debug])
        XCTAssertTrue(CutoutAppRoute.devicePicker.availableNavigationTabs.isEmpty)
        XCTAssertTrue(CutoutAppRoute.capture.availableNavigationTabs.isEmpty)
    }

    func testOnlyLivePhaseOpensTheRideSurface() {
        XCTAssertFalse(SessionConnectionPhase.connecting(model: .falcon).opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.discoveringServices.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.subscribing.opensRideScreen)
        XCTAssertTrue(SessionConnectionPhase.live.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.starting.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.scanning.opensRideScreen)
    }

    func testPickerStatusNeverShowsScanningWhenBluetoothIsUnavailableOrScanStateIsMissing() {
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: nil,
                phase: .bluetoothUnavailable(rawState: 4)
            ),
            .init(title: "Bluetooth unavailable", showsActivity: false)
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(scanState: nil, phase: .starting),
            .init(title: "Starting Bluetooth…", showsActivity: false)
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: .scanning,
                phase: .scanning
            ),
            .init(title: "Scanning Bluetooth", showsActivity: true)
        )
    }

    func testPickerStatusUsesTypedSymbolsForNonScanningStates() {
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: nil,
                phase: .bluetoothUnavailable(rawState: 4)
            ).symbolName,
            "bolt.slash.fill"
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: nil,
                phase: .failed(.connectFailed("timed out"))
            ).symbolName,
            "xmark.octagon.fill"
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(scanState: nil, phase: .live).symbolName,
            "checkmark.circle.fill"
        )
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
            "Connection failed. Choose a device to try again. Connect failed: timed out"
        )
    }

    func testReconnectLoopAnnouncesConnectionLossOnlyOnce() {
        var announcements = ConnectionAccessibilityAnnouncements()
        let messages = [
            SessionConnectionPhase.discoveringServices,
            .subscribing,
            .failed(.connectFailed("timed out")),
            .scanning,
            .discoveringServices,
            .failed(.connectFailed("still timed out")),
        ].compactMap { announcements.next(for: $0) }

        XCTAssertEqual(messages, ["Connection failed. Choose a device to try again. Connect failed: timed out"])
        XCTAssertEqual(announcements.next(for: .live), "Connected.")
        XCTAssertEqual(
            announcements.next(for: .failed(.connectFailed("lost after connecting"))),
            "Connection failed. Choose a device to try again. Connect failed: lost after connecting"
        )

        announcements.beginUserInitiatedAttempt()
        XCTAssertEqual(
            announcements.next(for: .failed(.connectFailed("timed out again"))),
            "Connection failed. Choose a device to try again. Connect failed: timed out again"
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
