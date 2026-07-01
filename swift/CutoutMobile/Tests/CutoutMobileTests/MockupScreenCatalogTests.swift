import XCTest
@testable import CutoutMobile

final class MockupScreenCatalogTests: XCTestCase {
    func testV2CatalogListsEveryScreenInDeviceInspectionOrder() {
        XCTAssertEqual(
            MockupScreenCatalog.v2.screens.map(\.id),
            [
                .devicePicker,
                .eucRide,
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

        XCTAssertEqual(screens[.eucGarage]?.title, "EUC health")
        XCTAssertEqual(screens[.eucGarage]?.primaryValue, "battery 85%")
        XCTAssertEqual(screens[.eucGarage]?.secondaryValue, "pack 115.8 V")

        XCTAssertEqual(screens[.vescOnewheelRide]?.title, "Fungineers X7")
        XCTAssertEqual(screens[.vescOnewheelRide]?.subtitle, "VESC OW - armed")
        XCTAssertEqual(screens[.vescOnewheelRide]?.primaryValue, "19 mph")
        XCTAssertEqual(screens[.vescOnewheelRide]?.secondaryValue, "Duty headroom 18%")

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
            support: .supported(connectionRoute: "electric_unicycle"),
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
        XCTAssertEqual(unsupported.pickerRow.state, .unsupported(action: "Not yet"))
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
        XCTAssertEqual(candidate.support, .supported(connectionRoute: "electric_unicycle"))
        XCTAssertEqual(candidate.pickerRow.id, "ios-local-aero")
        XCTAssertEqual(candidate.pickerRow.state, .supported(action: "Pair"))
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
                    localName: "Rideable-ish",
                    advertisedServiceUuids: []
                ),
            ]
        )

        XCTAssertEqual(state.statusText, "Scanning Bluetooth")
        XCTAssertEqual(state.rows.map(\.title), ["NOSFET Aero", "Rideable-ish"])
        XCTAssertEqual(state.sections.supported.map(\.title), ["NOSFET Aero"])
        XCTAssertEqual(state.sections.unsupported.map(\.title), ["Rideable-ish"])
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
}
