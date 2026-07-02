import XCTest
@testable import CutoutMobile

final class MockupScreenCatalogTests: XCTestCase {
    func testV2CatalogListsEveryScreenInDeviceInspectionOrder() {
        XCTAssertEqual(
            MockupScreenCatalog.v2.screens.map(\.id),
            [
                .devicePicker,
                .eucRide,
                .bmsOverview,
                .bmsCellMap6S,
                .bmsCellMap40S,
                .bmsCellDetail,
                .bmsUnknownTopology,
                .bmsNoData,
                .eucGarage,
                .vescOnewheelRide,
                .vescDebug,
            ]
        )
    }

    func testV2CatalogCarriesFixtureOnlyScreenData() {
        let screens = Dictionary(uniqueKeysWithValues: MockupScreenCatalog.v2.screens.map { ($0.id, $0) })

        XCTAssertEqual(screens[.devicePicker]?.title, "Device picker")
        XCTAssertEqual(screens[.devicePicker]?.subtitle, "Scanning Bluetooth")
        XCTAssertEqual(screens[.devicePicker]?.primaryValue, "Aero-126V")
        XCTAssertEqual(screens[.devicePicker]?.secondaryValue, "Little FOCer BT")

        XCTAssertEqual(screens[.eucRide]?.title, "Aero-126V")
        XCTAssertEqual(screens[.eucRide]?.subtitle, "EUC - riding")
        XCTAssertEqual(screens[.eucRide]?.primaryValue, "31 mph")
        XCTAssertEqual(screens[.eucRide]?.secondaryValue, "PWM headroom 23%")

        XCTAssertEqual(screens[.bmsOverview]?.title, "Pack overview")
        XCTAssertEqual(screens[.bmsOverview]?.primaryValue, "72%")
        XCTAssertEqual(screens[.bmsOverview]?.secondaryValue, "sag adjusted")

        XCTAssertEqual(screens[.bmsCellMap6S]?.title, "6S cell map")
        XCTAssertEqual(screens[.bmsCellMap6S]?.primaryValue, "12 mV spread")

        XCTAssertEqual(screens[.bmsCellMap40S]?.title, "40S cell map")
        XCTAssertEqual(screens[.bmsCellMap40S]?.secondaryValue, "scroll cells horizontally")

        XCTAssertEqual(screens[.bmsCellDetail]?.title, "Cell detail")
        XCTAssertEqual(screens[.bmsCellDetail]?.primaryValue, "4.071 V")

        XCTAssertEqual(screens[.bmsUnknownTopology]?.title, "Unknown BMS")
        XCTAssertEqual(screens[.bmsUnknownTopology]?.secondaryValue, "topology unverified")

        XCTAssertEqual(screens[.bmsNoData]?.title, "Battery")
        XCTAssertEqual(screens[.bmsNoData]?.secondaryValue, "limited data")

        XCTAssertEqual(screens[.eucGarage]?.title, "EUC health")
        XCTAssertEqual(screens[.eucGarage]?.primaryValue, "battery 85%")
        XCTAssertEqual(screens[.eucGarage]?.secondaryValue, "pack 115.8 V")

        XCTAssertEqual(screens[.vescOnewheelRide]?.title, "Fungineers X7")
        XCTAssertEqual(screens[.vescOnewheelRide]?.subtitle, "VESC OW · armed")
        XCTAssertEqual(screens[.vescOnewheelRide]?.primaryValue, "19")
        XCTAssertEqual(screens[.vescOnewheelRide]?.secondaryValue, "board speed")

        XCTAssertEqual(screens[.vescDebug]?.title, "VESC state")
        XCTAssertEqual(screens[.vescDebug]?.primaryValue, "duty cycle 82%")
        XCTAssertEqual(screens[.vescDebug]?.secondaryValue, "pack 75.4 V")

        XCTAssertTrue(MockupScreenCatalog.v2.screens.allSatisfy(\.isFixtureOnly))
    }
}

extension MockupScreenCatalogTests {
    func testDevicePickerFixtureCarriesRowsAndActionsFromMockup() throws {
        let picker = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .devicePicker))

        XCTAssertEqual(picker.pickerRows.map(\.title), [
            "Aero-126V",
            "Little FOCer BT",
            "NINEBOT-7A31",
            "HX Hoverboard",
            "Manual add / record unknown device",
        ])
        XCTAssertEqual(picker.pickerRows.map(\.state), [
            .supported(action: "Pair"),
            .supported(action: "Pair"),
            .unsupported(action: "Not yet"),
            .unsupported(action: "Not yet"),
            .manual(action: "later"),
        ])
        XCTAssertEqual(picker.pickerRows.map(\.connectionRoute), [
            .electricUnicycle,
            .vescOnewheel,
            nil,
            nil,
            nil,
        ])
    }
    func testDevicePickerFixtureRowsComeFromDiscoveryCandidates() throws {
        let picker = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .devicePicker))

        XCTAssertEqual(picker.discoveryCandidates.map(\.displayName), [
            "Aero-126V",
            "Little FOCer BT",
            "NINEBOT-7A31",
            "HX Hoverboard",
        ])
        XCTAssertEqual(Array(picker.pickerRows.prefix(4)), picker.discoveryCandidates.map(\.pickerRow))
    }

    func testDevicePickerSectionsHideEmptyGroups() {
        let supported = MockupPickerRow(
            title: "Aero-126V",
            subtitle: "Electric unicycle",
            detail: "strong signal",
            state: .supported(action: "Pair"),
            symbolName: "circle"
        )
        let unsupported = MockupPickerRow(
            title: "NINEBOT-7A31",
            subtitle: "Electric scooter",
            detail: "unsupported",
            state: .unsupported(action: "Not yet"),
            symbolName: "scooter"
        )

        XCTAssertEqual(MockupPickerSections(rows: [supported]).supported, [supported])
        XCTAssertTrue(MockupPickerSections(rows: [supported]).unsupported.isEmpty)
        XCTAssertNil(MockupPickerSections(rows: [supported]).manual)

        XCTAssertTrue(MockupPickerSections(rows: [unsupported]).supported.isEmpty)
        XCTAssertEqual(MockupPickerSections(rows: [unsupported]).unsupported, [unsupported])
        XCTAssertNil(MockupPickerSections(rows: [unsupported]).manual)
    }

    func testDevicePickerScanStateCoversUnavailableAndEmptyStates() {
        XCTAssertEqual(DevicePickerScanState.bluetoothUnavailable.statusText, "Bluetooth unavailable")
        XCTAssertEqual(DevicePickerScanState.permissionDenied.statusText, "Bluetooth permission denied")

        let empty = DevicePickerScanState(status: .idle, rows: [])
        XCTAssertEqual(empty.statusText, "No rideable devices found")
        XCTAssertTrue(empty.sections.supported.isEmpty)
        XCTAssertTrue(empty.sections.unsupported.isEmpty)
        XCTAssertNil(empty.sections.manual)
    }

    func testDiscoveryCandidatesMapToPickerRows() {
        let supported = DevicePickerDiscoveryCandidate(
            platformIdentifier: "ios-local-1",
            displayName: "Aero-126V",
            productCategory: "Electric unicycle",
            evidence: "telemetry profile found",
            detail: "126.0 V - strong signal",
            support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
            symbolName: "circle"
        )
        let unsupported = DevicePickerDiscoveryCandidate(
            platformIdentifier: "ios-local-2",
            displayName: "NINEBOT-7A31",
            productCategory: "Electric scooter",
            evidence: "known BLE advertisement",
            detail: "We can learn this later",
            support: .unsupported(disabledReason: "Not yet"),
            symbolName: "scooter"
        )

        XCTAssertEqual(supported.pickerRow.state, .supported(action: "Pair"))
        XCTAssertEqual(supported.pickerRow.subtitle, "Electric unicycle - telemetry profile found")
        XCTAssertEqual(supported.pickerRow.connectionRoute, .electricUnicycle)
        XCTAssertEqual(unsupported.pickerRow.state, .unsupported(action: "Not yet"))
        XCTAssertNil(unsupported.pickerRow.connectionRoute)
    }

    func testCoreBluetoothAdvertisementMapsToPickerCandidateWithoutMacAddress() {
        let advertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
            localName: "NOSFET Aero",
            advertisedServiceUuids: [.bluetooth16(0xFFE0)]
        )
        let candidate = DevicePickerDiscoveryCandidate(advertisement: advertisement)

        XCTAssertEqual(candidate.platformIdentifier, "ios-local-aero")
        XCTAssertEqual(candidate.displayName, "NOSFET Aero")
        XCTAssertEqual(candidate.productCategory, "Electric unicycle")
        XCTAssertEqual(candidate.support, .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero))
        XCTAssertEqual(candidate.pickerRow.id, "ios-local-aero")
        XCTAssertEqual(candidate.pickerRow.state, .supported(action: "Pair"))
        XCTAssertEqual(candidate.pickerRow.connectionRoute, .electricUnicycle)
    }

    func testUnknownSupportedConnectionRouteStillPairsWithoutMockupDestination() {
        let dto = MobileDiscoveryCandidateDto(
            platformIdentifier: "ios-local-supported-future",
            displayName: "Future rideable",
            productCategory: "Rideable",
            evidence: "supported by core",
            detail: "route not mapped in mockups yet",
            isPickerCandidate: true,
            support: .supported,
            connectionRoute: "future_route",
            electricUnicycleModel: nil,
            disabledReason: nil
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(
            candidate.support,
            DevicePickerCandidateSupport.supported(connectionRoute: nil, electricUnicycleModel: nil)
        )
        XCTAssertEqual(candidate.pickerRow.state, MockupPickerRowState.supported(action: "Pair"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testCoreBluetoothAdvertisementsMapToScanningPickerState() {
        let state = DevicePickerScanState(
            status: .scanning,
            advertisements: [
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                    localName: "NOSFET Aero",
                    advertisedServiceUuids: [.bluetooth16(0xFFE0)]
                ),
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-unknown"),
                    localName: "Little FOCer",
                    advertisedServiceUuids: [.bluetooth16(0xFFF0)]
                ),
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-keyboard"),
                    localName: "Keyboard",
                    advertisedServiceUuids: []
                ),
            ]
        )

        XCTAssertEqual(state.statusText, "Scanning Bluetooth")
        XCTAssertEqual(state.rows.map(\.title), ["NOSFET Aero", "Little FOCer"])
        XCTAssertEqual(state.rows.map(\.connectionRoute), [.electricUnicycle, nil])
        XCTAssertEqual(state.sections.supported.map(\.title), ["NOSFET Aero"])
        XCTAssertEqual(state.sections.unsupported.map(\.title), ["Little FOCer"])
        XCTAssertEqual(state.sections.unsupported.first?.state, .unsupported(action: "Not yet supported"))
        XCTAssertNil(state.sections.manual)
    }

    func testEucRideFixtureCarriesDashboardStructureFromMockup() throws {
        let ride = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .eucRide))

        XCTAssertEqual(ride.safetyBars, [
            MockupSafetyBar(label: "PWM headroom", value: "23%", progress: 0.77, accent: .yellow),
            MockupSafetyBar(label: "sag-adjusted energy", value: "62%", progress: 0.62, accent: .cyan),
        ])
        XCTAssertEqual(
            ride.warningCard,
            MockupWarningCard(title: "Reduce acceleration", detail: "Voltage sag under load: 9.4 V")
        )
        XCTAssertEqual(ride.dashboardTiles, [
            MockupDashboardTile(label: "pack", value: "115.8", unit: "V", detail: "-9.4 V sag", accent: .cyan),
            MockupDashboardTile(label: "power", value: "4.2", unit: "kW", detail: "regen -0.3 kW", accent: .yellow),
            MockupDashboardTile(label: "thermal", value: "61", unit: "°C", detail: "ESC 48 · motor 61", accent: .green),
            MockupDashboardTile(label: "limp-home", value: "14.2", unit: "mi", detail: "at this pace", accent: .cyan),
        ])
        XCTAssertEqual(ride.tabs, [
            MockupScreenTab(title: "Ride", isSelected: true),
            MockupScreenTab(title: "Pack", isSelected: false),
            MockupScreenTab(title: "Map", isSelected: false),
            MockupScreenTab(title: "Tune", isSelected: false),
        ])
    }
    func testEucGarageFixtureCarriesPackHealthStructureFromMockup() throws {
        let garage = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .eucGarage))

        XCTAssertEqual(
            garage.deviceCard,
            MockupDeviceCard(
                title: "Aero-126V",
                detail: "126 V nominal · 20s? mapped profile · BLE",
                status: "Safe",
                accent: .green
            )
        )
        XCTAssertEqual(garage.dashboardTiles, [
            MockupDashboardTile(label: "battery", value: "85", unit: "%", detail: "115.8 V", accent: .cyan),
            MockupDashboardTile(label: "beep margin", value: "11.6", unit: "mph", detail: "to configured alarm", accent: .yellow),
            MockupDashboardTile(label: "tiltback", value: "42", unit: "mph", detail: "wheel setting", accent: .orange),
            MockupDashboardTile(label: "pedal mode", value: "72", unit: "%", detail: "hardness normalized", accent: .purple),
        ])
        XCTAssertEqual(garage.summaryTitle, "Cell / BMS summary")
        XCTAssertEqual(garage.summaryRows, [
            MockupSummaryRow(label: "high group", value: "4.18 V", accent: nil),
            MockupSummaryRow(label: "low group", value: "4.13 V", accent: nil),
            MockupSummaryRow(label: "delta", value: "0.05 V", accent: .green),
        ])
        XCTAssertEqual(garage.faultCard, MockupFaultCard(title: "Last fault", detail: "none since 38.2 mi ago", accent: .green))
    }

    func testVescOnewheelRideFixtureCarriesRideCriticalStructureFromMockup() throws {
        let ride = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .vescOnewheelRide))

        XCTAssertEqual(ride.subtitle, "VESC OW · armed")
        XCTAssertEqual(ride.primaryValue, "19")
        XCTAssertEqual(ride.secondaryValue, "board speed")
        XCTAssertEqual(ride.safetyBars, [
            MockupSafetyBar(label: "Duty headroom", value: "18%", progress: 0.82, accent: .orange),
        ])
        XCTAssertEqual(
            ride.warningCard,
            MockupWarningCard(title: "Pushback soon", detail: "Duty and pack sag are both climbing.")
        )
        XCTAssertEqual(ride.dashboardTiles, [
            MockupDashboardTile(label: "battery current", value: "38", unit: "A", detail: "limit 45 A", accent: .yellow),
            MockupDashboardTile(label: "motor current", value: "71", unit: "A", detail: "phase estimate", accent: .orange),
            MockupDashboardTile(label: "board angle", value: "-1.8", unit: "°", detail: "nose down", accent: .cyan),
            MockupDashboardTile(label: "controller", value: "54", unit: "°C", detail: "motor 49 °C", accent: .green),
        ])
        XCTAssertEqual(ride.tabs, [
            MockupScreenTab(title: "Ride", isSelected: true),
            MockupScreenTab(title: "VESC", isSelected: false),
            MockupScreenTab(title: "Map", isSelected: false),
            MockupScreenTab(title: "Logs", isSelected: false),
        ])
    }

    func testVescDebugFixtureCarriesGuardedReadOnlyStateFromMockup() throws {
        let debug = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .vescDebug))

        XCTAssertEqual(
            debug.deviceCard,
            MockupDeviceCard(
                title: "Profile: Street stable",
                detail: "VESC Express · FW 6.x · UART bridge",
                status: "",
                accent: .cyan
            )
        )
        XCTAssertEqual(debug.dashboardTiles, [
            MockupDashboardTile(label: "duty cycle", value: "82", unit: "%", detail: "max seen 87%", accent: .orange),
            MockupDashboardTile(label: "pack", value: "75.4", unit: "V", detail: "20s lithium", accent: .cyan),
            MockupDashboardTile(label: "battery limit", value: "45", unit: "A", detail: "current max", accent: .yellow),
            MockupDashboardTile(label: "motor limit", value: "90", unit: "A", detail: "phase current", accent: .orange),
        ])
        XCTAssertEqual(debug.summaryTitle, "Fault / app channels")
        XCTAssertEqual(debug.summaryRows, [
            MockupSummaryRow(label: "last fault", value: "FAULT_CODE_NONE", accent: .green),
            MockupSummaryRow(label: "input app", value: "ADC + balance", accent: nil),
            MockupSummaryRow(label: "CAN status", value: "single controller", accent: nil),
            MockupSummaryRow(label: "logging", value: "local CSV armed", accent: .yellow),
        ])
        XCTAssertEqual(
            debug.faultCard,
            MockupFaultCard(
                title: "Guardrails",
                detail: "Hide dangerous writes until parked + confirmed.",
                accent: .orange
            )
        )
    }

    func testBmsFixturesCarryTypedSnapshotsForEveryBmsScreen() throws {
        let screenIDs: [MockupScreenID] = [
            .bmsOverview,
            .bmsCellMap6S,
            .bmsCellMap40S,
            .bmsCellDetail,
            .bmsUnknownTopology,
            .bmsNoData,
        ]

        let screens = try screenIDs.map {
            try XCTUnwrap(MockupScreenCatalog.v2.screen(id: $0))
        }

        XCTAssertEqual(screens.map(\.bmsContent?.kind), [
            .overview,
            .cellMapInline,
            .cellMapScrollable,
            .cellDetail,
            .unknownTopology,
            .noData,
        ])
        XCTAssertTrue(screens.allSatisfy { $0.bmsContent?.snapshot != nil })
    }

    func testBmsOverviewFixtureCapturesTopologyAndFaultSummary() throws {
        let overview = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .bmsOverview))
        let content = try XCTUnwrap(overview.bmsContent)
        let snapshot = content.snapshot

        XCTAssertEqual(snapshot.topology.layoutLabel, "20S4P split pack")
        XCTAssertEqual(snapshot.topology.seriesGroupCount, 20)
        XCTAssertEqual(snapshot.topology.parallelCount, 4)
        XCTAssertEqual(snapshot.topology.packCount, 2)
        XCTAssertEqual(snapshot.topology.bmsCount, 2)
        XCTAssertEqual(snapshot.energyPercent?.value, 72)
        XCTAssertEqual(snapshot.voltage?.value, 81_600)
        XCTAssertEqual(snapshot.cellDeltaMillivolts?.value, 18)
        XCTAssertEqual(snapshot.lowestGroupIndex, 17)
        XCTAssertEqual(snapshot.balancingSummary, "idle • top groups only")
        XCTAssertEqual(snapshot.faultSummary, "no active faults")
    }

    func testBmsCellFixturesPreserveHighlightedGroupsAndDetailSelection() throws {
        let inline = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .bmsCellMap6S)?.bmsContent)
        XCTAssertEqual(inline.snapshot.groups.map(\.index), [1, 2, 3, 4, 5, 6])
        XCTAssertEqual(inline.highlightedGroupIndices, [3, 6])

        let scrollable = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .bmsCellMap40S)?.bmsContent)
        XCTAssertEqual(scrollable.snapshot.groups.count, 40)
        XCTAssertEqual(scrollable.highlightedGroupIndices, [17, 18, 19, 31])

        let detail = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .bmsCellDetail)?.bmsContent)
        XCTAssertEqual(detail.selectedGroupIndex, 17)
        XCTAssertEqual(detail.snapshot.groups.first(where: { $0.index == 17 })?.voltage?.value, 4_071)
        XCTAssertEqual(detail.snapshot.groups.first(where: { $0.index == 17 })?.resistanceMilliohms, 21)
    }

    func testUnknownTopologyFixtureKeepsConfidenceLowAndAvoidsFakeMapping() throws {
        let unknown = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .bmsUnknownTopology)?.bmsContent)
        let snapshot = unknown.snapshot

        XCTAssertEqual(snapshot.topology.confidence, .unverified)
        XCTAssertNil(snapshot.topology.seriesGroupCount)
        XCTAssertEqual(snapshot.groups.count, 0)
        XCTAssertEqual(snapshot.faults.map(\.code), ["0x0040"])
        XCTAssertEqual(snapshot.captureActionTitle, "record unsupported pack")
    }

    func testNoDataFixtureMarksControllerOnlyEstimate() throws {
        let noData = try XCTUnwrap(MockupScreenCatalog.v2.screen(id: .bmsNoData)?.bmsContent)
        let snapshot = noData.snapshot

        XCTAssertEqual(snapshot.topology.layoutLabel, "non-smart BMS")
        XCTAssertEqual(snapshot.topology.confidence, .inferred)
        XCTAssertEqual(snapshot.energyPercent?.value, 71)
        XCTAssertEqual(snapshot.energyPercent?.source, .estimated)
        XCTAssertEqual(snapshot.energyPercent?.quality, .inferred)
        XCTAssertEqual(snapshot.energyPercent?.verification, .inferred)
        XCTAssertEqual(snapshot.voltage?.value, 117_600)
        XCTAssertEqual(snapshot.current?.value, 38_000)
        XCTAssertEqual(snapshot.captureActionState, "limited data")
    }
}
