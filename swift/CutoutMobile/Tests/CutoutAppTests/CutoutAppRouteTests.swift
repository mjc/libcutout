import Observation
import XCTest
@testable import CutoutApp
@testable import CutoutMobile
import CutoutMobileFFI

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
        XCTAssertEqual(localizedAppText("lighting.power"), "Power")
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
        XCTAssertEqual(localizedAppText("bms.display_modes"), "Display modes")
        XCTAssertEqual(localizedAppText("picker.section.setup"), "Setup")
        XCTAssertEqual(localizedAppText("picker.advanced_capture"), "Capture unknown device")
        XCTAssertEqual(localizedAppText("picker.capture_kind.label"), "Device kind for capture")
        XCTAssertEqual(localizedAppText("picker.capture_kind.placeholder"), "Device model")
        XCTAssertEqual(localizedAppText("picker.capture_kind.hint"), "Enter the device family and model, for example EUC NOSFET Aeon")
        XCTAssertEqual(localizedAppText("picker.section.supported_now"), "Supported now")
        XCTAssertEqual(localizedAppText("picker.section.probe_first"), "Probe first")
        XCTAssertEqual(localizedAppText("picker.section.record_only"), "Record only")
        XCTAssertEqual(localizedAppText("picker.capture_kind_required_hint"), "Enter a device kind above to enable capture")
        XCTAssertEqual(localizedAppText("capture.stop"), "Finish capture")
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

    func testLightingPresetSavingRequiresRequestedSettingsAndIdentity() {
        XCTAssertTrue(
            lightingPresetSaveEligibility(
                platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB",
                commandStatus: .confirmed
            )
        )
        XCTAssertFalse(
            lightingPresetSaveEligibility(
                platformIdentifier: nil,
                commandStatus: .confirmed
            )
        )
        XCTAssertTrue(
            lightingPresetSaveEligibility(
                platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB",
                commandStatus: .requested
            )
        )
        XCTAssertFalse(
            lightingPresetSaveEligibility(
                platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB",
                commandStatus: .idle
            )
        )
    }

    func testLightingNavigationSurvivesWheelLossAndKeepsMapDeviceFamily() {
        XCTAssertTrue(CutoutAppRoute.lighting(.euc).preservesNavigationOnConnectionLoss)
        XCTAssertTrue(CutoutAppRoute.lighting(.vesc).preservesNavigationOnConnectionLoss)
        XCTAssertEqual(
            CutoutAppRoute.rideMap.destination(forNavigationTarget: .lighting, connectionRoute: .vescOnewheel),
            .lighting(.vesc)
        )
    }

    func testLightingAutoStartRequiresRememberedIdentity() {
        XCTAssertTrue(shouldAutoStartLightingSession(platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"))
        XCTAssertFalse(shouldAutoStartLightingSession(platformIdentifier: nil))
        XCTAssertFalse(shouldAutoStartLightingSession(platformIdentifier: ""))
        XCTAssertFalse(shouldAutoStartLightingSession(platformIdentifier: "legacy-melk"))
    }

    func testLightingColorSelectionKeepsPrimaryAndNeutralColorsStable() {
        let red = lightingColorSelection(red: 255, green: 0, blue: 0)
        XCTAssertEqual(red.hue, 0, accuracy: 0.0001)
        XCTAssertEqual(red.saturation, 1, accuracy: 0.0001)

        let cyan = lightingColorSelection(red: 0, green: 255, blue: 255)
        XCTAssertEqual(cyan.hue, 0.5, accuracy: 0.0001)
        XCTAssertEqual(cyan.saturation, 1, accuracy: 0.0001)

        let white = lightingColorSelection(red: 255, green: 255, blue: 255)
        XCTAssertEqual(white.hue, 0, accuracy: 0.0001)
        XCTAssertEqual(white.saturation, 0, accuracy: 0.0001)
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

    func testNavigationPathNestsRideMapDetailUnderMap() {
        XCTAssertEqual(
            CutoutAppRoute.navigationPath(for: .rideMapDetail(rideID: "ride-1")),
            [.rideMap, .rideMapDetail(rideID: "ride-1")]
        )
    }

    func testRouteOwnsTheSameTabsUsedByWindowCommandsAndContent() {
        XCTAssertTrue(CutoutAppRoute.devicePicker.navigationTabs(for: nil).isEmpty)
        XCTAssertTrue(CutoutAppRoute.capture.navigationTabs(for: nil).isEmpty)
        XCTAssertEqual(
            CutoutAppRoute.eucRide.navigationTabs(for: .electricUnicycle).map(\.id),
            [.ride, .lighting, .pack, .map, .tune]
        )
        XCTAssertEqual(
            CutoutAppRoute.vescRide.navigationTabs(for: .vescOnewheel).map(\.id),
            [.ride, .lighting, .debug, .map, .logs]
        )
        XCTAssertTrue(
            CutoutAppRoute.eucPack(.bmsOverview)
                .navigationTabs(for: .electricUnicycle)
                .first(where: { $0.id == .pack })?.isSelected == true
        )
        XCTAssertTrue(
            CutoutAppRoute.eucPack(.root)
                .navigationTabs(for: .electricUnicycle)
                .first(where: { $0.id == .pack })?.isSelected == true
        )
        XCTAssertTrue(
            CutoutAppRoute.vescDebug
                .navigationTabs(for: .vescOnewheel)
                .first(where: { $0.id == .debug })?.isSelected == true
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
        XCTAssertEqual(
            CutoutAppRoute.eucRide.availableNavigationTabs(for: .electricUnicycle).map(\.id),
            [.ride, .lighting, .pack, .map]
        )
        XCTAssertEqual(
            CutoutAppRoute.eucPack(.bmsOverview).availableNavigationTabs(for: .electricUnicycle).map(\.id),
            [.ride, .lighting, .pack, .map]
        )
        XCTAssertEqual(
            CutoutAppRoute.vescRide.availableNavigationTabs(for: .vescOnewheel).map(\.id),
            [.ride, .lighting, .debug, .map]
        )
        XCTAssertEqual(
            CutoutAppRoute.vescDebug.availableNavigationTabs(for: .vescOnewheel).map(\.id),
            [.ride, .lighting, .debug, .map]
        )
        XCTAssertTrue(CutoutAppRoute.devicePicker.availableNavigationTabs(for: nil).isEmpty)
        XCTAssertTrue(CutoutAppRoute.capture.availableNavigationTabs(for: nil).isEmpty)
    }

    func testDisconnectedMapKeepsMapCommandAvailable() {
        let tabs = CutoutAppRoute.rideMap.availableNavigationTabs(for: nil)

        XCTAssertEqual(tabs.map(\.id), [.map])
        XCTAssertTrue(tabs[0].isSelected)
        XCTAssertEqual(tabs[0].destinationTarget, .rideMap)
    }

    func testConnectionLossKeepsStandaloneMapNavigation() {
        XCTAssertTrue(CutoutAppRoute.rideMap.preservesNavigationOnConnectionLoss)
        XCTAssertTrue(CutoutAppRoute.rideMapDetail(rideID: "ride-1").preservesNavigationOnConnectionLoss)
        XCTAssertFalse(CutoutAppRoute.eucRide.preservesNavigationOnConnectionLoss)
    }

    func testDisconnectCommandRequiresAConnection() {
        XCTAssertFalse(CutoutNavigationCommands.canDisconnect(currentRoute: .rideMap, hasConnection: false))
        XCTAssertTrue(CutoutNavigationCommands.canDisconnect(currentRoute: .rideMap, hasConnection: true))
        XCTAssertFalse(CutoutNavigationCommands.canDisconnect(currentRoute: .devicePicker, hasConnection: true))
    }

    func testConnectedMapRoutesUseTheDeviceFamilyAndSelectMap() {
        let eucTabs = CutoutAppRoute.rideMap.availableNavigationTabs(for: .electricUnicycle)
        let vescTabs = CutoutAppRoute.rideMapDetail(rideID: "ride-1")
            .availableNavigationTabs(for: .vescOnewheel)

        XCTAssertEqual(eucTabs.map(\.id), [.ride, .lighting, .pack, .map])
        XCTAssertEqual(vescTabs.map(\.id), [.ride, .lighting, .debug, .map])
        XCTAssertEqual(eucTabs.first(where: { $0.isSelected })?.id, .map)
        XCTAssertEqual(vescTabs.first(where: { $0.isSelected })?.id, .map)
    }

    func testNestedPackRouteSurvivesSharedTabRendering() {
        let nestedPackRoute = CutoutAppRoute.eucPack(.bmsCellDetail(7))
        let tabs = nestedPackRoute.availableNavigationTabs(for: .electricUnicycle)

        XCTAssertEqual(nestedPackRoute.destination(for: tabs[0]), .eucRide)
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[2]), nestedPackRoute)
        XCTAssertEqual(
            CutoutAppRoute.vescDebug.destination(
                for: CutoutAppRoute.vescDebug.availableNavigationTabs(for: .vescOnewheel)[2]
            ),
            .vescDebug
        )
    }

    func testUnavailableTabHasNoDestination() {
        let unavailableLogsTab = CutoutAppRoute.vescRide.navigationTabs(for: .vescOnewheel)[4]

        XCTAssertNil(CutoutAppRoute.vescRide.destination(for: unavailableLogsTab))
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
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.timedOut)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device identification timed out"
        )
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.malformedResponse)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device returned an invalid identification response"
        )
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.conflictingEvidence)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device identification found conflicting evidence"
        )
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.unsupported)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device does not support this identification probe"
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
        XCTAssertEqual(
            localizedAppText("accessibility.vesc_warning.duty_pushback"),
            "Warning. Pushback soon. Reduce acceleration."
        )
        XCTAssertEqual(localizedAppText("vesc.warning.wheelslip"), "Wheel slip")
        let stopCopy: [(String, String)] = [
            ("vesc.stop.pitch", "Stopped: pitch"),
            ("vesc.stop.roll", "Stopped: roll"),
            ("vesc.stop.switch_half", "Half-footpad stop"),
            ("vesc.stop.switch_full", "Footpad stop"),
            ("vesc.stop.reverse", "Reverse stop"),
            ("vesc.stop.quick_stop", "Quick stop"),
            ("vesc.stop.detail", "Board stopped balancing. Re-engage only when safe."),
        ]
        for (key, expected) in stopCopy {
            XCTAssertEqual(localizedAppText(key), expected)
        }
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
        let vescAnnouncements: [(VescRideWarning, String)] = [
            (.lowVoltage, "Warning. Controller voltage is low. Slow down and stop safely."),
            (.highVoltage, "Warning. Controller voltage is high. Stop safely."),
            (.mosfetTemperature, "Warning. Controller temperature is high. Stop safely and let it cool."),
            (.motorTemperature, "Warning. Motor temperature is high. Stop safely and let it cool."),
            (.current, "Warning. Current limit reached. Reduce acceleration."),
            (.dutyPushback, "Warning. Pushback soon. Reduce acceleration."),
            (.speedPushback, "Warning. Speed pushback. Reduce speed."),
            (.temperaturePushback, "Warning. Temperature pushback. Stop safely and let the board cool."),
            (.wheelslip, "Warning. Wheel slip detected. Reduce acceleration and regain control."),
            (.sensors, "Warning. Stop safely and check the board sensors."),
            (.lowBattery, "Warning. Battery is low. Slow down and stop safely."),
            (.error, "Critical warning. Controller error. Stop safely."),
            (.bmsConnection, "Critical warning. Battery-management connection failed. Stop safely."),
        ]
        for (warning, announcement) in vescAnnouncements {
            XCTAssertEqual(warning.accessibilityAnnouncement, announcement)
        }
        XCTAssertNil(VescRideWarning.unknown.accessibilityAnnouncement)

        let stopAnnouncements: [(VescRideStopReason, String)] = [
            (.pitch, "Board stopped balancing because of pitch. Re-engage only when safe."),
            (.roll, "Board stopped balancing because of roll. Re-engage only when safe."),
            (.switchHalf, "Board stopped balancing because half the footpad released. Re-engage only when safe."),
            (.switchFull, "Board stopped balancing because the footpad released. Re-engage only when safe."),
            (.reverse, "Board stopped with reverse-stop. Re-engage only when safe."),
            (.quickStop, "Board quick-stopped. Re-engage only when safe."),
        ]
        XCTAssertNil(VescRideStopReason.none.accessibilityAnnouncement)
        for (reason, announcement) in stopAnnouncements {
            XCTAssertEqual(reason.accessibilityAnnouncement, announcement)
        }
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

    func testConnectionLossStatesResetLightingRestoreEligibility() {
        let resetStates: [MelkLightingPeripheralState] = [
            .scanning,
            .connecting,
            .retrying(attempt: 1, delayMilliseconds: 250),
            .disconnected,
            .failed("Bluetooth unavailable"),
        ]
        let stableStates: [MelkLightingPeripheralState] = [
            .idle,
            .discovering,
            .ready,
        ]

        XCTAssertTrue(resetStates.allSatisfy(\.resetsRestoreEligibility))
        XCTAssertTrue(stableStates.allSatisfy { !$0.resetsRestoreEligibility })
    }

    @MainActor
    func testLightingRouteModelUsesInjectedSessionLifecycle() throws {
        let suiteName = "CutoutAppRouteTests.lightingSession"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let fake = TestLightingSession()
        let model = LightingRouteModel(
            session: fake,
            persistence: LightingAccessoryPersistence(defaults: defaults)
        )

        model.start()
        XCTAssertEqual(fake.startCalls, [nil])
        model.stop()
        XCTAssertEqual(fake.stopCalls, 1)
    }

    @MainActor
    func testLightingRouteModelsKeepSimultaneousControllersIsolated() throws {
        let suiteNames = [
            "CutoutAppRouteTests.lightingSessionA",
            "CutoutAppRouteTests.lightingSessionB",
        ]
        let defaults = try suiteNames.map { name in
            let defaults = try XCTUnwrap(UserDefaults(suiteName: name))
            defaults.removePersistentDomain(forName: name)
            return defaults
        }
        defer {
            for (name, defaults) in zip(suiteNames, defaults) {
                defaults.removePersistentDomain(forName: name)
            }
        }

        let sessionA = TestLightingSession()
        let sessionB = TestLightingSession()
        let modelA = LightingRouteModel(
            session: sessionA,
            persistence: LightingAccessoryPersistence(defaults: defaults[0])
        )
        let modelB = LightingRouteModel(
            session: sessionB,
            persistence: LightingAccessoryPersistence(defaults: defaults[1])
        )

        modelA.start()
        modelB.start()
        modelA.setPower(true)
        modelB.setSolidColor(red: 1, green: 2, blue: 3)

        XCTAssertEqual(sessionA.startCalls, [nil])
        XCTAssertEqual(sessionB.startCalls, [nil])
        XCTAssertEqual(sessionA.powerRequests, [true])
        XCTAssertTrue(sessionA.colorRequests.isEmpty)
        XCTAssertEqual(sessionB.colorRequests, ["1,2,3"])
        XCTAssertTrue(sessionB.powerRequests.isEmpty)

        modelA.stop()
        XCTAssertEqual(sessionA.stopCalls, 1)
        XCTAssertEqual(sessionB.stopCalls, 0)
        modelB.stop()
        XCTAssertEqual(sessionB.stopCalls, 1)
    }

    @MainActor
    func testLightingRouteModelPublishesPresetChanges() throws {
        let suiteName = "CutoutAppRouteTests.lightingPresets"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let persistence = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(
            persistence.ensureRecord(
                platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"
            )
        )
        let model = LightingRouteModel(
            session: TestLightingSession(),
            persistence: persistence
        )
        model.setPower(true)
        model.markConfirmed()

        let flag = ObservationFlag()
        withObservationTracking {
            _ = model.presets
        } onChange: {
            flag.value = true
        }

        model.savePreset(named: "Night")

        XCTAssertTrue(flag.value)
        XCTAssertEqual(model.presets.map(\.name), ["Night"])
    }

    @MainActor
    func testLightingRouteModelMarksPendingCommandUnconfirmedAfterLinkLoss() async throws {
        let suiteName = "CutoutAppRouteTests.lightingPendingCommand"
        let defaults = try! XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: persistence)
        model.start()

        fake.emitState(.ready)
        await Task.yield()
        model.setPower(true)
        XCTAssertEqual(model.commandStatus, .requested)

        fake.emitState(.retrying(attempt: 1, delayMilliseconds: 250))
        await Task.yield()

        XCTAssertEqual(model.commandStatus, .unconfirmed)
    }

    @MainActor
    func testLightingRouteModelReportsCommandRefusalWhenDisconnected() throws {
        let suiteName = "CutoutAppRouteTests.lightingRefusal"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let fake = TestLightingSession()
        fake.powerResult = false
        let model = LightingRouteModel(session: fake, persistence: LightingAccessoryPersistence(defaults: defaults))

        model.setPower(true)

        XCTAssertEqual(model.controlError, "Power was not sent because the lighting controller is not ready.")
        XCTAssertEqual(model.commandStatus, .idle)
    }

    @MainActor
    func testLightingRouteModelClearsCommandErrorAfterRecovery() throws {
        let suiteName = "CutoutAppRouteTests.lightingRefusalRecovery-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let fake = TestLightingSession()
        fake.powerResult = false
        let model = LightingRouteModel(session: fake, persistence: LightingAccessoryPersistence(defaults: defaults))
        model.setPower(true)
        XCTAssertNotNil(model.controlError)

        fake.powerResult = true
        model.setPower(true)

        XCTAssertNil(model.controlError)
        XCTAssertEqual(model.commandStatus, .requested)
    }

    @MainActor
    func testLightingRouteModelCountsOneNotificationOnce() async throws {
        let suiteName = "CutoutAppRouteTests.lightingNotification"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: LightingAccessoryPersistence(defaults: defaults))
        model.start()

        fake.emitNotification(Data([0x59, 0x42]))
        await Task.yield()

        XCTAssertEqual(model.notificationCount, 1)
        XCTAssertEqual(model.records.filter { $0.text == "FFF4 notification received" }.count, 1)
    }

    @MainActor
    func testLightingRouteModelPersistsOnlyStableConnectionStates() async throws {
        let suiteName = "CutoutAppRouteTests.lightingConnectionState"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let persistence = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(persistence.ensureRecord(platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"))
        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: persistence)
        model.start()
        struct PersistenceEnvelope: Decodable {
            let record: Data
        }
        func persistedRecord() throws -> MobileRgbLightingAccessoryRecord {
            let data = try XCTUnwrap(defaults.data(forKey: "lighting.accessory.record"))
            let envelope = try JSONDecoder().decode(PersistenceEnvelope.self, from: data)
            return try MobileRgbLightingAccessoryRecord.decode(bytes: envelope.record)
        }

        let transientStates: [MelkLightingPeripheralState] = [
            .scanning,
            .connecting,
            .discovering,
            .retrying(attempt: 1, delayMilliseconds: 250),
            .failed("Bluetooth unavailable"),
        ]
        for state in transientStates {
            fake.emitState(state)
            await Task.yield()
            let record = try persistedRecord()
            XCTAssertEqual(record.connection(), .unknown, "unexpected persisted state for \(state)")
        }

        fake.emitState(.ready)
        await Task.yield()
        var record = try persistedRecord()
        XCTAssertEqual(record.connection(), .ready)

        fake.emitState(.disconnected)
        await Task.yield()
        record = try persistedRecord()
        XCTAssertEqual(record.connection(), .disconnected)
    }

    @MainActor
    func testLightingRouteModelRestoresConfirmedStateForRememberedIdentity() async throws {
        let suiteName = "CutoutAppRouteTests.lightingRestore"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let platformIdentifier = "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(persistence.ensureRecord(platformIdentifier: platformIdentifier))
        let requested = MobileMelkLightingRestoreStateDto(
            powerOn: true,
            red: 12,
            green: 34,
            blue: 56,
            brightness: 78
        )
        try persistence.confirm(requested)
        persistence.setRestoreEnabled(true)

        let fake = TestLightingSession()
        let model = LightingRouteModel(
            session: fake,
            persistence: LightingAccessoryPersistence(defaults: defaults)
        )

        model.start()
        XCTAssertEqual(fake.startCalls, [platformIdentifier])
        fake.emitIdentity(
            MelkLightingPeripheralIdentity(
                name: "MELK-OC21 6A",
                platformIdentifier: platformIdentifier,
                rssi: -40
            )
        )
        fake.emitRecord("candidate=MELK-OC21 6A id=\(platformIdentifier) rssi=-40")
        await Task.yield()
        fake.emitState(.ready)
        await Task.yield()

        XCTAssertEqual(fake.stateRequests, [requested])
        XCTAssertTrue(fake.powerRequests.isEmpty)
        XCTAssertEqual(model.commandStatus, .requested)
    }

    @MainActor
    func testLightingRouteModelUsesConfirmedPowerForInitialToggle() throws {
        let suiteName = "CutoutAppRouteTests.lightingPowerState"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let persistence = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(persistence.ensureRecord(platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"))
        let confirmedOn = MobileMelkLightingRestoreStateDto(
            powerOn: true, red: 1, green: 2, blue: 3, brightness: 50
        )
        try persistence.confirm(confirmedOn)
        persistence.setRestoreEnabled(true)

        let model = LightingRouteModel(
            session: TestLightingSession(),
            persistence: LightingAccessoryPersistence(defaults: defaults)
        )

        XCTAssertTrue(model.requestedPowerOn)
    }

    @MainActor
    func testLightingRouteModelStopsMusicThroughTheValidatedStatePath() {
        let suiteName = "CutoutAppRouteTests.lightingMusic"
        let defaults = try! XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: persistence)

        model.setPlayback(.music(effect: 1, sensitivity: 50))
        model.stopMusic()

        XCTAssertEqual(model.requestedPlayback, .solid)
        XCTAssertEqual(fake.stateRequests.last?.playback, .solid)
    }

    @MainActor
    func testLightingRouteModelRejectsRestoreBatchWithoutChangingState() async throws {
        let suiteName = "CutoutAppRouteTests.lightingRestoreFailure"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let platformIdentifier = "B1C2D3E4-F5A6-4789-ABCD-0123456789AB"
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(persistence.ensureRecord(platformIdentifier: platformIdentifier))
        let requested = MobileMelkLightingRestoreStateDto(
            powerOn: true,
            red: 90,
            green: 80,
            blue: 70,
            brightness: 60
        )
        try persistence.confirm(requested)
        persistence.setRestoreEnabled(true)

        let fake = TestLightingSession()
        fake.stateResult = false
        let model = LightingRouteModel(
            session: fake,
            persistence: LightingAccessoryPersistence(defaults: defaults)
        )

        model.start()
        fake.emitIdentity(
            MelkLightingPeripheralIdentity(
                name: "MELK-OC21 6A",
                platformIdentifier: platformIdentifier,
                rssi: -40
            )
        )
        fake.emitRecord("candidate=MELK-OC21 6A id=\(platformIdentifier) rssi=-40")
        await Task.yield()
        fake.emitState(.ready)
        await Task.yield()

        XCTAssertEqual(fake.stateRequests, [requested])
        XCTAssertTrue(fake.powerRequests.isEmpty)
        XCTAssertTrue(fake.colorRequests.isEmpty)
        XCTAssertTrue(fake.brightnessRequests.isEmpty)
        XCTAssertEqual(model.commandStatus, .idle)
    }

    @MainActor
    func testLightingPlaybackPresetDoesNotRequireManualConfirmation() throws {
        let suiteName = "CutoutAppRouteTests.playbackPreset"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(persistence.ensureRecord(platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"))
        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: persistence)
        model.setPlayback(.effect(pattern: 16, speed: 75))
        XCTAssertEqual(model.requestedPlayback, .effect(pattern: 16, speed: 75))
        XCTAssertEqual(model.commandStatus, .requested)
        XCTAssertTrue(model.canSavePreset)
        model.savePreset(named: "Effect")
        let preset = try XCTUnwrap(model.presets.first)
        XCTAssertEqual(preset.requested.playback, .effect(pattern: 16, speed: 75))
        model.setSolidColor(red: 255, green: 0, blue: 0)
        XCTAssertEqual(fake.stateRequests.last?.playback, .solid)
        model.applyPreset(preset)
        XCTAssertEqual(fake.stateRequests.last, preset.requested)
        fake.stateResult = false
        model.setPlayback(.effect(pattern: 1, speed: 255))
        XCTAssertEqual(model.requestedPlayback, .effect(pattern: 16, speed: 75))
        fake.stateResult = true
        model.setSolidColor(red: 1, green: 2, blue: 3)
        XCTAssertTrue(model.replacePreset(named: "Effect"))
        XCTAssertNil(model.presets.first?.requested.playback)
        XCTAssertTrue(model.deletePreset(named: "Effect"))
        XCTAssertTrue(model.presets.isEmpty)
        XCTAssertFalse(model.deletePreset(named: "Effect"))
    }

    func testLightingControlPagesResolveFromTheAppCatalog() {
        XCTAssertEqual(LightingControlPage.allCases.map(\.title), ["Color", "Effects", "Music", "Schedule"])
    }

    func testLightingPatternNamesKeepWireIDsAndUnmappedModesExplicit() {
        let groups = LightingPatternCatalog.groups
        XCTAssertEqual(groups.flatMap(\.ids).sorted(), Array(0...227))
        XCTAssertEqual(groups.first?.name, "Basic")
        XCTAssertEqual(groups.first?.ids.prefix(3), [1, 2, 212])
        XCTAssertEqual(groups.first(where: { $0.name == "Curtain" })?.ids, Array(57...76))
        XCTAssertTrue((1...212).allSatisfy { LightingPatternCatalog.isMapped($0) })
        XCTAssertEqual(LightingPatternCatalog.verifiedEffectIDs, Set([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 22, 75]))
        XCTAssertTrue([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 22, 75].allSatisfy { LightingPatternCatalog.isVerified($0) })
        XCTAssertFalse([0, 11, 212, 213, 227].contains { LightingPatternCatalog.isVerified($0) })
        XCTAssertFalse([0, 213, 220, 255, -1].contains { LightingPatternCatalog.isMapped($0) })
        for id in 1...212 {
            XCTAssertFalse(LightingPatternCatalog.name(for: id).isEmpty)
            XCTAssertFalse(LightingPatternCatalog.name(for: id).contains("Unmapped"))
        }
        XCTAssertEqual(LightingPatternCatalog.name(for: 1), "Magic Forward")
        XCTAssertEqual(LightingPatternCatalog.name(for: 16), "6-Color to Cyan Back")
        XCTAssertEqual(LightingPatternCatalog.name(for: 75), "White Close")
        XCTAssertEqual(LightingPatternCatalog.name(for: 115), "Green-Dot in Red Running (reference)")
        XCTAssertEqual(LightingPatternCatalog.name(for: 169), "Green-Dot in Red Running (reference)")
        XCTAssertEqual(LightingPatternCatalog.name(for: 212), "7-Color Energy (reference)")
        for id in [0] + Array(213...227) {
            XCTAssertFalse(LightingPatternCatalog.name(for: id).isEmpty)
            XCTAssertFalse(LightingPatternCatalog.name(for: id).contains("Unmapped"))
            XCTAssertFalse(LightingPatternCatalog.isMapped(id))
        }
        XCTAssertEqual(LightingPatternCatalog.name(for: 0), "Auto Play (reference)")
        XCTAssertEqual(LightingPatternCatalog.name(for: 213), "Fade 73 (reference)")
        XCTAssertEqual(LightingPatternCatalog.name(for: 220), "Music Flow Flash (reference)")
        XCTAssertEqual(LightingPatternCatalog.name(for: 221), "Music Flash (reference)")
        XCTAssertEqual(LightingPatternCatalog.name(for: 227), "Music Pulse 2 (reference)")
        for id in [255, -1] {
            XCTAssertEqual(LightingPatternCatalog.name(for: id), "Unmapped effect \(id)")
        }
    }

    @MainActor
    func testLightingSpeedChangesDoNotRestartOrPowerOnAnEffect() throws {
        let suiteName = "CutoutAppRouteTests.effectSpeed"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        XCTAssertTrue(persistence.ensureRecord(platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"))
        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: persistence)
        model.setEffectSpeed(0)
        XCTAssertTrue(fake.speedRequests.isEmpty)
        model.setPlayback(.effect(pattern: 16, speed: 128))
        model.setPower(false)
        let batches = fake.stateRequests.count
        for speed: UInt8 in [0, 255] {
            model.setEffectSpeed(speed)
            XCTAssertEqual(model.requestedPlayback, .effect(pattern: 16, speed: speed))
            XCTAssertEqual(persistence.requestedState?.playback, .effect(pattern: 16, speed: speed))
        }
        XCTAssertEqual(fake.speedRequests, [0, 255])
        XCTAssertEqual(fake.stateRequests.count, batches)
        XCTAssertEqual(fake.powerRequests, [false])
        XCTAssertFalse(model.requestedPowerOn)
        fake.speedResult = false
        model.setEffectSpeed(100)
        XCTAssertEqual(model.requestedPlayback, .effect(pattern: 16, speed: 255))
        XCTAssertNotNil(model.controlError)
        model.setPlayback(.music(effect: 1, sensitivity: 50))
        model.setEffectSpeed(0)
        XCTAssertEqual(fake.speedRequests, [0, 255, 100])
    }

    @MainActor
    func testLightingScheduleDoesNotChangePlaybackOrSaveOnOpening() throws {
        let suiteName = "CutoutAppRouteTests.lightingSchedule"
        let defaults = try! XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: persistence)
        XCTAssertTrue(fake.scheduleRequests.isEmpty)
        let schedule = MobileMelkScheduleDto(powerOn: false, hour: 23, minute: 15, days: 31, enabled: true)
        XCTAssertTrue(model.setSchedule(schedule))
        XCTAssertEqual(fake.scheduleRequests, [schedule])
        XCTAssertEqual(model.commandStatus, .requested)
        XCTAssertEqual(model.requestedPlayback, .solid)
        XCTAssertTrue(fake.stateRequests.isEmpty)
    }

    @MainActor
    func testLightingRouteModelPublishesCandidatesUntilExplicitSelection() async throws {
        let suiteName = "CutoutAppRouteTests.lightingCandidates"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let fake = TestLightingSession()
        let model = LightingRouteModel(
            session: fake,
            persistence: LightingAccessoryPersistence(defaults: defaults)
        )
        model.start()

        let farther = MelkLightingPeripheralCandidate(
            name: "MELK-OC21 6A",
            platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB",
            rssi: -70
        )
        let nearer = MelkLightingPeripheralCandidate(
            name: "MELK-OC21 6B",
            platformIdentifier: "B1B2C3D4-E5F6-4789-ABCD-0123456789AB",
            rssi: -40
        )
        fake.emitCandidate(farther)
        fake.emitCandidate(nearer)
        await Task.yield()

        XCTAssertEqual(model.candidates, [nearer, farther])
        XCTAssertTrue(model.peripheralIdentifier == nil)
        model.selectCandidate(nearer)
        XCTAssertEqual(fake.candidateSelections, [nearer.id])
    }

    @MainActor
    func testLightingRouteModelConsumesTypedIdentityEvents() async {
        let suiteName = "CutoutAppRouteTests.lightingIdentity"
        let defaults = try! XCTUnwrap(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let persistence = LightingAccessoryPersistence(defaults: defaults)
        let fake = TestLightingSession()
        let model = LightingRouteModel(session: fake, persistence: persistence)
        model.start()

        fake.emitIdentity(
            MelkLightingPeripheralIdentity(
                name: "MELK-OC21 6A",
                platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB",
                rssi: -40
            )
        )
        fake.emitRecord("candidate=MELK-OC21 6A id=legacy-melk rssi=-40")
        await Task.yield()

        XCTAssertEqual(model.peripheralName, "MELK-OC21 6A")
        XCTAssertEqual(model.peripheralIdentifier, "A1B2C3D4-E5F6-4789-ABCD-0123456789AB")
    }
}

private final class ObservationFlag: @unchecked Sendable {
    var value = false
}

private final class TestLightingSession: MelkLightingPeripheralSessionProtocol {
    var commandStatus: MelkLightingCommandStatus = .idle
    var onIdentity: ((MelkLightingPeripheralIdentity) -> Void)?
    var onStateChange: ((MelkLightingPeripheralState) -> Void)?
    var onNotification: ((Data) -> Void)?
    var onRecord: ((String) -> Void)?
    var onCandidate: ((MelkLightingPeripheralCandidate) -> Void)?
    var startCalls: [String?] = []
    var stopCalls = 0
    var candidateSelections: [String] = []
    var speedRequests: [UInt8] = []
    var speedResult = true
    var stateResult = true
    var stateRequests: [MobileMelkLightingRestoreStateDto] = []
    var scheduleRequests: [MobileMelkScheduleDto] = []
    var powerResult = true
    var colorResult = true
    var brightnessResult = true
    var powerRequests: [Bool] = []
    var colorRequests: [String] = []
    var brightnessRequests: [UInt8] = []

    func start(preferredPlatformIdentifier: String?) {
        startCalls.append(preferredPlatformIdentifier)
    }

    func stop() {
        stopCalls += 1
    }

    func selectCandidate(platformIdentifier: String) {
        candidateSelections.append(platformIdentifier)
    }

    func setPower(_ on: Bool) -> Bool {
        powerRequests.append(on)
        return powerResult
    }

    func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> Bool {
        colorRequests.append("\(red),\(green),\(blue)")
        return colorResult
    }

    func setBrightness(_ percentage: UInt8) throws -> Bool {
        brightnessRequests.append(percentage)
        return brightnessResult
    }

    func setEffectSpeed(_ speed: UInt8) -> Bool {
        speedRequests.append(speed)
        return speedResult
    }

    func applyState(_ state: MobileMelkLightingRestoreStateDto) throws -> Bool {
        stateRequests.append(state)
        return stateResult
    }

    func setSchedule(_ schedule: MobileMelkScheduleDto, clock: MobileMelkClockDto) throws -> Bool {
        scheduleRequests.append(schedule)
        return stateResult
    }

    func markLastCommandConfirmed() {}
    func markLastCommandUnconfirmed() {}

    func emitIdentity(_ identity: MelkLightingPeripheralIdentity) {
        onIdentity?(identity)
    }

    func emitRecord(_ record: String) {
        onRecord?(record)
    }

    func emitNotification(_ data: Data) {
        onNotification?(data)
    }

    func emitState(_ state: MelkLightingPeripheralState) {
        onStateChange?(state)
    }

    func emitCandidate(_ candidate: MelkLightingPeripheralCandidate) {
        onCandidate?(candidate)
    }
}
