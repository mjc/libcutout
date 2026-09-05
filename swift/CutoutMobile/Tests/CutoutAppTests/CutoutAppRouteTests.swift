import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppRouteTests: XCTestCase {
    func testAeroSettingsFormUsesCurrentValuesWhenAvailable() {
        let values = AeroSettingsFormValues(
            tiltback: AeroSpeedSetting(kilometresPerHour: 31),
            pwm: AeroPwmPercent(percent: 74),
            alarm: AeroSpeedSetting(kilometresPerHour: 42),
            angle: AeroAngleAdjustment(tenthsOfDegree: -12)
        )

        XCTAssertEqual(values.tiltbackSpeed, 31)
        XCTAssertEqual(values.pwmPercent, 74)
        XCTAssertEqual(values.alarmSpeed, 42)
        XCTAssertEqual(values.angleTenths, -12)
    }

    func testAeroSettingsFormKeepsSafeDefaultsWhenValuesAreUnavailable() {
        let values = AeroSettingsFormValues(
            tiltback: nil,
            pwm: nil,
            alarm: nil,
            angle: nil
        )

        XCTAssertEqual(values.tiltbackSpeed, 20)
        XCTAssertEqual(values.pwmPercent, 60)
        XCTAssertEqual(values.alarmSpeed, 20)
        XCTAssertEqual(values.angleTenths, 0)
    }

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
        XCTAssertEqual(localizedAppText("navigation.section.tune"), "Tune")
        XCTAssertEqual(localizedAppText("settings.lights.title"), "Lights")
        XCTAssertEqual(localizedAppText("settings.headlight.title"), "Headlight")
        XCTAssertEqual(localizedAppText("settings.high_beam.title"), "High beam")
        XCTAssertEqual(localizedAppText("settings.capabilities.title"), "Other settings")
        XCTAssertEqual(localizedAppText("settings.capabilities.unverified"), "Needs validation")
        XCTAssertEqual(localizedAppText("settings.capabilities.unsupported"), "Not supported")
        XCTAssertEqual(localizedAppText("settings.state.pending"), "Pending")
        XCTAssertEqual(localizedAppText("settings.state.confirmed"), "Confirmed")
        XCTAssertEqual(localizedAppText("settings.state.confirmed_ago", Int64(2)), "Confirmed 2s ago")
        XCTAssertEqual(localizedAppText("settings.state.refused"), "Refused")
        XCTAssertEqual(localizedAppText("settings.state.timed_out"), "Timed out")
        XCTAssertEqual(localizedAppText("settings.state.failed"), "Failed")
        XCTAssertEqual(localizedAppText("settings.pedal_mode.title"), "Pedal mode")
        XCTAssertEqual(localizedAppText("settings.roll_angle.title"), "Roll angle")
        XCTAssertEqual(localizedAppText("settings.roll_angle.footer"), "Change only while parked.")
        XCTAssertEqual(localizedAppText("settings.roll_angle.high"), "High")
        XCTAssertEqual(localizedAppText("settings.acceleration_assist.title"), "Acceleration assist")
        XCTAssertEqual(localizedAppText("settings.taillight.title"), "Taillight")
        XCTAssertEqual(
            localizedAppText("settings.headlight.help"),
            "Changes are sent immediately to the connected wheel."
        )
        XCTAssertEqual(
            localizedAppText("settings.headlight.waiting"),
            "Waiting for wheel confirmation."
        )
        XCTAssertEqual(
            localizedAppText("settings.headlight.confirmed"),
            "Confirmed by wheel telemetry."
        )
        XCTAssertEqual(
            localizedAppText("settings.headlight.confirmed_ago", Int64(2)),
            "Confirmed by wheel telemetry 2s ago."
        )
        XCTAssertEqual(
            localizedAppText("settings.high_beam.sent_unconfirmed"),
            "Command sent. This wheel does not report high-beam state."
        )
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

    func testSettingReadbackPresentationKeepsKnownValuesAndUnknownStatesDistinct() {
        XCTAssertEqual(
            EucSettingReadbackPresentation.speed(.available(Speed(value: 11_666))),
            "26.1 mph"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.pedalMode(.available(.rawMode(3))),
            "Raw 3"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.pedalMode(.available(.documented(.medium))),
            "Medium"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.pedalMode(
                PedalModeSettingState(kind: .current, current: .hard),
                fallback: .available(.documented(.soft))
            ),
            "Hard"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.rollAngle(
                RollAngleSettingState(kind: .pending, requested: .high),
                fallback: .available(.documented(.low))
            ),
            "Low"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.rollAngle(
                RollAngleSettingState(kind: .refused, requested: .high),
                fallback: .available(.documented(.low))
            ),
            "Low"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.rollAngle(
                RollAngleSettingState(kind: .timedOut, requested: .high),
                fallback: .available(.documented(.low))
            ),
            "Low"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.rollAngle(
                RollAngleSettingState(kind: .failed, requested: .high),
                fallback: .available(.documented(.low))
            ),
            "Low"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.speed(.unavailable),
            "Unavailable"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.pedalMode(.unsupported),
            "Not supported"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.seconds(.available(900)),
            "900 s"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.chargeMode(.available(.charging)),
            "Charging"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.tripDistance(1_609_344),
            "1.0 mi"
        )
        XCTAssertEqual(
            EucSettingReadbackPresentation.tripDistance(nil),
            "Unavailable"
        )
    }

    func testSettingCapabilityPresentationPrefersLifecycleStatusWhenActionable() {
        XCTAssertEqual(
            EucSettingCapabilityPresentation.statusText(support: .unverified, state: .pending),
            "Pending"
        )
        XCTAssertEqual(
            EucSettingCapabilityPresentation.statusText(support: .unsupported, state: .refused),
            "Refused"
        )
        XCTAssertEqual(
            EucSettingCapabilityPresentation.statusText(
                support: .supported,
                state: .confirmed,
                confirmedAt: MonotonicMilliseconds(1_000),
                now: MonotonicMilliseconds(3_999)
            ),
            "Confirmed 2s ago"
        )
        XCTAssertEqual(
            EucSettingCapabilityPresentation.statusText(support: .supported, state: .timedOut),
            "Timed out"
        )
        XCTAssertEqual(
            EucSettingCapabilityPresentation.statusText(support: .supported, state: .failed),
            "Failed"
        )
        XCTAssertEqual(
            EucSettingCapabilityPresentation.statusText(support: .unverified, state: .unknown),
            "Needs validation"
        )
        XCTAssertEqual(
            EucSettingCapabilityPresentation.statusText(support: .unsupported, state: nil),
            "Not supported"
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
            .eucTune,
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
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .eucTune), [.eucTune])
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
            [.ride, .pack, .map, .tune]
        )
        XCTAssertEqual(
            CutoutAppRoute.vescRide.navigationTabs(for: .vescOnewheel).map(\.id),
            [.ride, .debug, .map, .logs]
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
        XCTAssertEqual(
            CutoutAppRoute.eucTune
                .navigationTabs(for: .electricUnicycle)
                .filter(\.isSelected)
                .map(\.id),
            [.tune]
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
            [.ride, .pack, .map, .tune]
        )
        XCTAssertEqual(
            CutoutAppRoute.eucPack(.bmsOverview).availableNavigationTabs(for: .electricUnicycle).map(\.id),
            [.ride, .pack, .map, .tune]
        )
        XCTAssertEqual(
            CutoutAppRoute.vescRide.availableNavigationTabs(for: .vescOnewheel).map(\.id),
            [.ride, .debug, .map]
        )
        XCTAssertEqual(
            CutoutAppRoute.vescDebug.availableNavigationTabs(for: .vescOnewheel).map(\.id),
            [.ride, .debug, .map]
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

        XCTAssertEqual(eucTabs.map(\.id), [.ride, .pack, .map, .tune])
        XCTAssertEqual(vescTabs.map(\.id), [.ride, .debug, .map])
        XCTAssertEqual(eucTabs.first(where: { $0.isSelected })?.id, .map)
        XCTAssertEqual(vescTabs.first(where: { $0.isSelected })?.id, .map)
        XCTAssertEqual(
            CutoutAppRoute.eucTune.availableNavigationTabs(for: .electricUnicycle).map(\.id),
            [.ride, .pack, .map, .tune]
        )
    }

    func testNestedPackRouteSurvivesSharedTabRendering() {
        let nestedPackRoute = CutoutAppRoute.eucPack(.bmsCellDetail(7))
        let tabs = nestedPackRoute.availableNavigationTabs(for: .electricUnicycle)

        XCTAssertEqual(nestedPackRoute.destination(for: tabs[0]), .eucRide)
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[1]), nestedPackRoute)
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[2]), .rideMap)
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[3]), .eucTune)
        XCTAssertEqual(
            CutoutAppRoute.vescDebug.destination(
                for: CutoutAppRoute.vescDebug.availableNavigationTabs(for: .vescOnewheel)[1]
            ),
            .vescDebug
        )
    }

    func testUnavailableTabHasNoDestination() {
        let unavailableLogsTab = CutoutAppRoute.vescRide.navigationTabs(for: .vescOnewheel)[3]

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

}
