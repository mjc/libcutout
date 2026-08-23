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
        XCTAssertEqual(localizedAppText("navigation.tab.faults"), "Faults")
        XCTAssertEqual(localizedAppText("navigation.section.lighting"), "Lighting")
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

    func testLightingPresetSavingRequiresConfirmedCommandAndIdentity() {
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
        XCTAssertFalse(
            lightingPresetSaveEligibility(
                platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB",
                commandStatus: .requested
            )
        )
        XCTAssertFalse(
            lightingPresetSaveEligibility(
                platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB",
                commandStatus: .unconfirmed
            )
        )
    }

    func testLightingAutoStartRequiresRememberedIdentity() {
        XCTAssertTrue(shouldAutoStartLightingSession(platformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB"))
        XCTAssertFalse(shouldAutoStartLightingSession(platformIdentifier: nil))
        XCTAssertFalse(shouldAutoStartLightingSession(platformIdentifier: ""))
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
            .lighting(.euc),
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

        XCTAssertEqual(routes.count, 13)
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
        XCTAssertEqual(CutoutAppRoute.eucRide.navigationTabs.map(\.id), [.ride, .lighting, .pack, .map, .tune])
        XCTAssertEqual(CutoutAppRoute.vescRide.navigationTabs.map(\.id), [.ride, .lighting, .debug, .map, .logs])
        XCTAssertEqual(CutoutAppRoute.lighting(.euc).navigationTabs.first(where: { $0.id == .lighting })?.isSelected, true)
        XCTAssertEqual(CutoutAppRoute.lighting(.vesc).navigationTabs.first(where: { $0.id == .lighting })?.isSelected, true)
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
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .lighting), "7")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .pack), "2")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .map), "3")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .tune), "4")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .debug), "5")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .logs), "6")
    }

    func testNativeNavigationOmitsUnavailableDestinations() {
        XCTAssertEqual(CutoutAppRoute.eucRide.availableNavigationTabs.map(\.id), [.ride, .lighting, .pack])
        XCTAssertEqual(CutoutAppRoute.eucPack(.bmsOverview).availableNavigationTabs.map(\.id), [.ride, .lighting, .pack])
        XCTAssertEqual(CutoutAppRoute.vescRide.availableNavigationTabs.map(\.id), [.ride, .lighting, .debug])
        XCTAssertEqual(CutoutAppRoute.vescDebug.availableNavigationTabs.map(\.id), [.ride, .lighting, .debug])
        XCTAssertTrue(CutoutAppRoute.devicePicker.availableNavigationTabs.isEmpty)
        XCTAssertTrue(CutoutAppRoute.capture.availableNavigationTabs.isEmpty)
    }

    func testNestedPackRouteSurvivesSharedTabRendering() {
        let nestedPackRoute = CutoutAppRoute.eucPack(.bmsCellDetail(7))
        let tabs = nestedPackRoute.availableNavigationTabs

        XCTAssertEqual(nestedPackRoute.destination(for: tabs[0]), .eucRide)
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[1]), .lighting(.euc))
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[2]), nestedPackRoute)
        XCTAssertEqual(CutoutAppRoute.vescDebug.destination(for: CutoutAppRoute.vescDebug.availableNavigationTabs[2]), .vescDebug)
    }

    func testUnavailableTabHasNoDestination() {
        let unavailableMapTab = CutoutAppRoute.eucRide.navigationTabs[3]

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
            (.temperaturePushback, "Warning. Temperature pushback. Stop safely and let the board cool."),
            (.wheelslip, "Warning. Wheel slip detected. Reduce acceleration and regain control."),
            (.sensors, "Warning. Stop safely and check the board sensors."),
            (.lowBattery, "Warning. Battery is low. Slow down and stop safely."),
            (.error, "Critical warning. Controller error. Stop safely."),
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

    func testMELKScanPolicyRoutesStandaloneAccessoryWithoutAnEUCModelHint() {
        let service = BluetoothUuid.bluetooth16(0xfff0)
        let advertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("melk-1"),
            localName: "MELK-OC21  6A",
            advertisedServiceUuids: [service]
        )
        let coordinator = CoreBluetoothCentralCoordinator(
            scanPolicy: .melk,
            writeLimit: TransportWriteLimitBytes(23)
        )

        XCTAssertEqual(coordinator.scanPolicy.serviceUuids, [service])
        XCTAssertEqual(
            coordinator.handleDiscovered(advertisement),
            .connect(peripheralIdentifier: CoreBluetoothPeripheralIdentifier("melk-1"))
        )
    }

    func testMELKCommandEvidenceNeverTreatsAWriteAsConfirmedByDefault() {
        var evidence = MelkLightingCommandEvidence()
        XCTAssertEqual(evidence.status, .idle)

        evidence.requested()
        XCTAssertEqual(evidence.status, .requested)
        evidence.unconfirmed()
        XCTAssertEqual(evidence.status, .unconfirmed)
        evidence.requested()
        evidence.confirmed()
        XCTAssertEqual(evidence.status, .confirmed)
    }

    func testObservedMELKInventoryPlansTypedWriteAndNotificationSubscription() throws {
        let service = BluetoothUuid.bluetooth16(0xfff0)
        let write = BluetoothUuid.bluetooth16(0xfff3)
        let notify = BluetoothUuid.bluetooth16(0xfff4)
        let harness = try MelkLightingCommandProfile(
            name: "MELK-OC21  6A",
            inventory: CoreBluetoothGattInventory(services: [
                CoreBluetoothGattService(
                    uuid: service,
                    characteristics: [
                        CoreBluetoothGattCharacteristic(
                            uuid: write,
                            properties: [.writeWithoutResponse]
                        ),
                        CoreBluetoothGattCharacteristic(
                            uuid: notify,
                            properties: [.notify]
                        ),
                    ]
                ),
            ])
        )

        let plan = harness.setPower(true)
        XCTAssertEqual(plan.operation, .writeWithoutResponse(
            channel: write,
            bytes: Data([0x7e, 0x00, 0x04, 0x01, 0, 0, 0, 0, 0xef])
        ))
        XCTAssertEqual(plan.confirmationChannel, notify)
        XCTAssertEqual(harness.subscription, .subscribe(channel: notify))
    }

    func testMELKProfileRejectsUnverifiedIdentityAndCharacteristicRoles() {
        XCTAssertThrowsError(
            try MelkLightingCommandProfile(
                name: "Govee_H607C_D635",
                inventory: CoreBluetoothGattInventory(services: [] as [CoreBluetoothGattService])
            )
        )
    }

    func testRememberedMELKTargetAcceptsOnlyTheSamePlatformIdentity() {
        let target = MelkLightingTargetPolicy(preferredPlatformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB")

        XCTAssertTrue(target.accepts(CoreBluetoothPeripheralIdentifier("a1b2c3d4-e5f6-4789-abcd-0123456789ab")))
        XCTAssertFalse(target.accepts(CoreBluetoothPeripheralIdentifier("B1B2C3D4-E5F6-4789-ABCD-0123456789AB")))
        XCTAssertFalse(target.isInvalid)
    }

    func testFirstPairingTargetAcceptsAnyPlatformIdentity() {
        let target = MelkLightingTargetPolicy(preferredPlatformIdentifier: nil)

        XCTAssertTrue(target.accepts(CoreBluetoothPeripheralIdentifier("first-melk")))
        XCTAssertFalse(target.isInvalid)
    }

    func testMalformedRememberedTargetFailsClosed() {
        let target = MelkLightingTargetPolicy(preferredPlatformIdentifier: "legacy-melk")

        XCTAssertTrue(target.isInvalid)
        XCTAssertFalse(target.accepts(CoreBluetoothPeripheralIdentifier("legacy-melk")))
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

}