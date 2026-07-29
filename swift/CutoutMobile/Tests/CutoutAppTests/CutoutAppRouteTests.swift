import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppRouteTests: XCTestCase {
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

    func testNavigationLabelsResolveFromTheAppCatalog() {
        XCTAssertEqual(localizedAppText("navigation.tab.cells"), "Cells")
        XCTAssertEqual(localizedAppText("navigation.tab.faults"), "Faults")
        XCTAssertEqual(localizedAppText("picker.title"), "Choose device")
        XCTAssertEqual(localizedAppText("picker.subtitle.nearby_devices"), "Nearby Bluetooth devices")
        XCTAssertEqual(localizedAppText("bms.detail.back_to_cell_map"), "Back to cell map")
        XCTAssertEqual(localizedAppText("bms.detail.group", Int64(3)), "Group 3")
        XCTAssertEqual(localizedAppText("bms.detail.trend", "stable"), "Trend: stable")
        XCTAssertEqual(localizedAppText("bms.overview.usable_energy"), "Usable energy")
        XCTAssertEqual(localizedAppText("bms.overview.average_group"), "Average group")
        XCTAssertEqual(localizedAppText("bms.overview.lowest_group"), "Lowest group")
        XCTAssertEqual(localizedAppText("bms.overview.highest_temperature"), "Highest temperature")
        XCTAssertEqual(localizedAppText("bms.overview.balancing"), "Balancing")
        XCTAssertEqual(localizedAppText("bms.overview.fault_state"), "Fault state")
        XCTAssertEqual(localizedAppText("bms.overview.pack_telemetry"), "Pack telemetry")
        XCTAssertEqual(localizedAppText("bms.unknown.title"), "Do not pretend certainty")
        XCTAssertEqual(localizedAppText("bms.unknown.reported_voltage"), "Reported voltage")
        XCTAssertEqual(localizedAppText("bms.unknown.cell_count"), "Cell count")
        XCTAssertEqual(localizedAppText("bms.unknown.temperatures"), "Temperatures")
        XCTAssertEqual(localizedAppText("bms.unknown.fault_bits"), "Fault bits")
        XCTAssertEqual(localizedAppText("bms.unknown.next_capture_flow"), "Next capture flow")
        XCTAssertEqual(localizedAppText("bms.unknown.capture_unavailable"), "Unavailable")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence.medium"), "Medium")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence.low"), "Low")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence.unknown"), "Unknown")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence_detail.not_cell_safe"), "Not cell-safe")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence_detail.telemetry_unavailable"), "Telemetry unavailable")
        XCTAssertEqual(localizedAppText("bms.no_data.pack_estimate"), "PACK ESTIMATE")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence"), "CONFIDENCE")
        XCTAssertEqual(localizedAppText("bms.no_data.what_we_can_see"), "WHAT WE CAN SEE")
        XCTAssertEqual(localizedAppText("bms.no_data.pack_voltage"), "Pack voltage")
        XCTAssertEqual(localizedAppText("bms.no_data.ride_sag"), "Ride sag")
        XCTAssertEqual(localizedAppText("bms.no_data.load_now"), "Load now")
        XCTAssertEqual(localizedAppText("bms.no_data.what_is_unknown"), "WHAT IS UNKNOWN")
        XCTAssertEqual(localizedAppText("bms.no_data.pack_estimate_accessibility"), "Pack estimate")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence_accessibility"), "Confidence")
        XCTAssertEqual(localizedAppText("bms.no_data.estimate_detail.recent_sag"), "Derived from voltage curve + recent sag")
        XCTAssertEqual(localizedAppText("bms.no_data.estimate_detail.voltage_curve"), "Derived from voltage curve only")
        XCTAssertEqual(localizedAppText("bms.no_data.estimate_detail.unavailable"), "Estimate unavailable")
        XCTAssertEqual(localizedAppText("bms.diagnostics.title"), "BMS diagnostics")
        XCTAssertEqual(localizedAppText("bms.diagnostics.detail"), "Raw readback, available when we need to debug")
        XCTAssertEqual(localizedAppText("picker.section.setup"), "Setup")
        XCTAssertEqual(localizedAppText("picker.capture_kind.label"), "Device kind for capture")
        XCTAssertEqual(localizedAppText("picker.capture_kind.placeholder"), "Device model")
        XCTAssertEqual(localizedAppText("picker.capture_kind.hint"), "Enter the device family and model, for example EUC NOSFET Aeon")
        XCTAssertEqual(localizedAppText("picker.section.supported_now"), "Supported now")
        XCTAssertEqual(localizedAppText("picker.section.probe_first"), "Probe first")
        XCTAssertEqual(localizedAppText("picker.section.record_only"), "Record only")
        XCTAssertEqual(localizedAppText("picker.capture_kind_required_hint"), "Enter a device kind above to enable capture")
        XCTAssertEqual(localizedAppText("picker.use_action.hint"), "Connect to this device")
        XCTAssertEqual(localizedAppText("picker.error.device_no_longer_available"), "Device is no longer available")
        XCTAssertEqual(localizedAppText("app.command.no_connected_device"), "No connected device")
        XCTAssertEqual(localizedAppText("app.command.disconnect"), "Disconnect")
        XCTAssertEqual(localizedAppText("app.command.navigate"), "Navigate")
        XCTAssertEqual(
            localizedAppText("bms.no_data.pack_estimate_accessibility_value", "71", "Derived from voltage curve"),
            "71%. Derived from voltage curve"
        )
        XCTAssertEqual(
            localizedAppText("bms.no_data.confidence_accessibility_value", "Medium", "Not cell-safe"),
            "Medium. Not cell-safe"
        )
        XCTAssertEqual(
            PevScreen(id: .bmsCellDetail, title: "", subtitle: "", secondaryValue: "").tabTitle,
            "Cells"
        )
        XCTAssertEqual(
            PevScreen(id: .bmsUnknownTopology, title: "", subtitle: "", secondaryValue: "").tabTitle,
            "Faults"
        )
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

    func testBmsDetailRouteStaysSelectedOnlyWhileItsGroupExists() {
        XCTAssertTrue(EucPackScreen.bmsCellDetail(4).hasAvailableSelectedGroup(in: [1, 4, 7]))
        XCTAssertFalse(EucPackScreen.bmsCellDetail(4).hasAvailableSelectedGroup(in: [1, 7]))
        XCTAssertTrue(EucPackScreen.bmsCellDetail(4).hasAvailableSelectedGroup(in: nil))
        XCTAssertTrue(EucPackScreen.bmsCellDetail(nil).hasAvailableSelectedGroup(in: [1, 7]))
        XCTAssertTrue(EucPackScreen.bmsOverview.hasAvailableSelectedGroup(in: [1, 7]))
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

    func testNestedPackRouteSurvivesSharedTabRendering() {
        let nestedPackRoute = CutoutAppRoute.eucPack(.bmsCellDetail(7))
        let tabs = nestedPackRoute.availableNavigationTabs

        XCTAssertEqual(nestedPackRoute.destination(for: tabs[0]), .eucRide)
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[1]), nestedPackRoute)
        XCTAssertEqual(CutoutAppRoute.vescDebug.destination(for: CutoutAppRoute.vescDebug.availableNavigationTabs[1]), .vescDebug)
    }

    func testUnavailableTabHasNoDestination() {
        let unavailableMapTab = CutoutAppRoute.eucRide.navigationTabs[2]

        XCTAssertNil(CutoutAppRoute.eucRide.destination(for: unavailableMapTab))
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
            .init(title: "Bluetooth unavailable", showsActivity: false, symbolName: "bolt.slash.fill")
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(scanState: nil, phase: .starting),
            .init(title: "Starting Bluetooth…", showsActivity: false, symbolName: "bolt.horizontal.circle")
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

    func testPickerPermissionDenialUsesDistinctVisualAndSpokenRecovery() {
        XCTAssertEqual(
            DevicePickerConnectionPresentation(scanState: nil, phase: .bluetoothPermissionDenied),
            .init(
                title: "Bluetooth permission denied",
                showsActivity: false,
                symbolName: "lock.slash.fill"
            )
        )
        XCTAssertEqual(
            SessionConnectionPhase.bluetoothPermissionDenied.accessibilityAnnouncement,
            "Bluetooth permission denied. Allow Bluetooth access in Settings to scan for rides."
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

    func testConnectionAnnouncementsSpeakRejectedPickerActionOnlyOnce() {
        var announcements = ConnectionAccessibilityAnnouncements()
        announcements.beginUserInitiatedAttempt()
        let rejectedAction = DevicePickerScanState.failed("Device is no longer available")

        XCTAssertEqual(
            announcements.next(for: rejectedAction),
            "Device is no longer available"
        )
        XCTAssertNil(announcements.next(for: .failed(.connectFailed("timed out"))))
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

    func testReconnectStateAnnouncesRetryOnlyOnce() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )
        let retry = SessionConnectionRetry(
            platformIdentifier: selection.platformIdentifier,
            attempt: 1,
            deadline: MonotonicMilliseconds(0),
            failure: .connectFailed("timed out")
        )
        var announcements = ConnectionAccessibilityAnnouncements()

        XCTAssertEqual(
            announcements.next(for: .retrying(selection, retry: retry)),
            "Connection lost. Retrying connection."
        )
        XCTAssertNil(announcements.next(for: .retrying(selection, retry: retry)))
    }

    func testReconnectStateAnnouncesAgainAfterConnectionRestores() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )
        let retry = SessionConnectionRetry(
            platformIdentifier: selection.platformIdentifier,
            attempt: 1,
            deadline: MonotonicMilliseconds(0),
            failure: .connectFailed("timed out")
        )
        var announcements = ConnectionAccessibilityAnnouncements()

        XCTAssertEqual(
            announcements.next(for: .retrying(selection, retry: retry)),
            "Connection lost. Retrying connection."
        )
        XCTAssertNil(announcements.next(for: .connected(selection)))
        XCTAssertEqual(
            announcements.next(for: .retrying(selection, retry: retry)),
            "Connection lost. Retrying connection."
        )
    }

    func testSafetyAnnouncementCopyResolvesFromTheAppCatalog() {
        XCTAssertEqual(
            localizedAppText("accessibility.euc_warning.caution"),
            "Caution. Riding headroom is getting low."
        )
        XCTAssertEqual(
            localizedAppText("accessibility.euc_warning.reduce_acceleration"),
            "Warning. Reduce acceleration."
        )
        XCTAssertEqual(
            localizedAppText("accessibility.euc_warning.limp_home"),
            "Critical warning. Slow down and stop safely."
        )
        XCTAssertEqual(localizedAppText("accessibility.vesc_warning.pushback"), "Warning. Pushback soon.")
        XCTAssertEqual(
            localizedAppText("accessibility.bms_alert.warning"),
            "Battery warning. Check BMS details."
        )
        XCTAssertEqual(
            localizedAppText("accessibility.bms_alert.critical"),
            "Critical battery warning. Check BMS details."
        )
    }

    func testLiveActivityLifecycleErrorsHaveTypedAnnouncements() {
        XCTAssertEqual(
            LiveActivityRideLifecycleError.authorizationDenied.accessibilityAnnouncement,
            "Live Activity permission is unavailable."
        )
        XCTAssertEqual(
            LiveActivityRideLifecycleError.requestFailed.accessibilityAnnouncement,
            "Couldn't start the Live Activity."
        )
        XCTAssertEqual(
            LiveActivityRideLifecycleError.activityUnavailable.accessibilityAnnouncement,
            "The Live Activity is unavailable."
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
