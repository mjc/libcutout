import XCTest
@testable import CutoutMobile

final class PevScreenCatalogTests: XCTestCase {
    func testLiveCatalogContainsOnlyProductionRouteMetadata() {
        XCTAssertEqual(
            PevScreenCatalog.live.screens.map(\.id),
            [
                .eucRide,
                .bmsOverview,
                .bmsCellMap6S,
                .bmsCellMap40S,
                .bmsCellDetail,
                .bmsUnknownTopology,
                .bmsNoData,
                .eucGarage,
                .vescRide,
                .vescDebug,
            ]
        )
        XCTAssertTrue(PevScreenCatalog.live.screens.allSatisfy { screen in
            screen.secondaryValue == "unavailable"
                && screen.bmsContent == nil
        })
    }

    func testVescRideTabsKeepUnavailableDestinationsDisabled() {
        let tabs = PevRideTabs.vescRideTabs()

        XCTAssertEqual(tabs.map(\.title), ["Ride", "Debug", "Map", "Logs"])
        XCTAssertEqual(tabs[2].disabledReason, "Map is not available yet.")
        XCTAssertEqual(tabs[3].disabledReason, "Logs are not available yet.")
        XCTAssertFalse(tabs[2].isEnabled)
        XCTAssertFalse(tabs[3].isEnabled)
        XCTAssertNil(tabs[2].destinationTarget)
        XCTAssertNil(tabs[3].destinationTarget)
    }

    func testRideTabsNavigateToTheirProductionSurfaces() {
        XCTAssertEqual(PevRideTabs.eucRideTabs().first?.destinationTarget, .screen(.eucRide))
        XCTAssertNil(PevRideTabs.eucRideTabs().first?.destinationScreenID)
        XCTAssertEqual(PevRideTabs.vescRideTabs().first?.destinationTarget, .vescRide)
        XCTAssertEqual(PevRideTabs.vescRideTabs()[1].destinationTarget, .screen(.vescDebug))
    }

    func testSelectedTabSemanticsTrackExplicitRoutes() {
        XCTAssertTrue(PevRideTabs.eucRideTabs(selected: .eucGarage)[1].isSelected)
        XCTAssertTrue(PevRideTabs.vescRideTabs(selected: .vescRide)[0].isSelected)
        XCTAssertFalse(PevRideTabs.vescRideTabs(selected: .vescDebug)[0].isSelected)
        XCTAssertTrue(PevRideTabs.vescRideTabs(selected: .vescDebug)[1].isSelected)
    }

    func testDevicePickerScanningHasNoInventedRows() {
        let state = DevicePickerScanState.scanning

        XCTAssertEqual(state.status, .scanning)
        XCTAssertTrue(state.rows.isEmpty)
        XCTAssertTrue(state.sections.supported.isEmpty)
        XCTAssertTrue(state.sections.unsupported.isEmpty)
        XCTAssertNil(state.sections.manual)
    }

    func testDevicePickerRowsDeriveFromDiscoveryCandidates() {
        let supported = DevicePickerDiscoveryCandidate(
            platformIdentifier: "live-euc",
            displayName: "EUC",
            productCategory: "Electric unicycle",
            evidence: "advertised service",
            detail: "supported route",
            support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
            symbolName: "circle.hexagongrid.circle"
        )
        let unsupported = DevicePickerDiscoveryCandidate(
            platformIdentifier: "unknown-board",
            displayName: "Unknown board",
            productCategory: "unknown",
            evidence: "advertised service",
            detail: "record only",
            support: .unknownRecordable(disabledReason: "unsupported device"),
            symbolName: "questionmark.circle"
        )

        let state = DevicePickerScanState(status: .idle, rows: [supported.pickerRow, unsupported.pickerRow])

        XCTAssertEqual(state.sections.supported.map(\.id), ["live-euc"])
        XCTAssertEqual(state.sections.unsupported.map(\.id), ["unknown-board"])
        XCTAssertEqual(supported.pickerRow.connectionRoute, .electricUnicycle)
        XCTAssertTrue(supported.pickerRow.isSupported)
        XCTAssertTrue(unsupported.pickerRow.isUnsupported)
    }

    func testPresentedPackScreenUsesLiveBmsState() throws {
        let pack = try XCTUnwrap(PevScreenCatalog.live.screen(id: .eucGarage))
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "6S1P pack",
                seriesGroupCount: 6,
                parallelCount: 1,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            groups: (1...6).map { index in
                BmsGroupSnapshot(index: index, voltage: Voltage(value: 4_200 - Int32(index)))
            }
        )

        let presented = PevScreenCatalog.live.presentedScreen(for: pack, liveBmsSnapshot: snapshot)

        XCTAssertEqual(presented.id, .bmsCellMap6S)
        XCTAssertEqual(presented.bmsContent?.snapshot, snapshot)
        XCTAssertEqual(presented.bmsContent?.highlightedGroupIndices, [])
        XCTAssertEqual(presented.bmsContent?.modeTitles, ["balance view"])
    }

    func testBmsPresentationIdentityDoesNotDependOnVisibleText() {
        let chips = [
            PevBmsChip(id: .topology, title: "Same label", accent: .yellow),
            PevBmsChip(id: .bmsStatus, title: "Same label", accent: .green),
        ]
        let content = PevBmsContent(
            kind: .cellMapInline,
            snapshot: BmsSnapshot(topology: BmsTopology(
                layoutLabel: "Test pack",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            )),
            chips: chips,
            modeTitles: ["Same label", "Same label"]
        )

        XCTAssertNotEqual(chips[0].id, chips[1].id)
        XCTAssertEqual(content.modes.map(\.id), [0, 1])
        XCTAssertEqual(content.modeTitles, ["Same label", "Same label"])
    }

    func testPresentedBmsScreenKeepsExplicitRouteIdentity() throws {
        let overview = try XCTUnwrap(PevScreenCatalog.live.screen(id: .bmsOverview))
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "observed pack",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            voltage: Voltage(value: 95_800)
        )

        let presented = PevScreenCatalog.live.presentedScreen(for: overview, liveBmsSnapshot: snapshot)

        XCTAssertEqual(presented.id, .bmsOverview)
        XCTAssertEqual(presented.title, "Pack overview")
        XCTAssertEqual(presented.bmsContent?.kind, .overview)
        XCTAssertEqual(presented.bmsContent?.snapshot.voltage, Voltage(value: 95_800))
    }

    func testPresentedPackWithoutBmsUsesExplicitUnavailableState() throws {
        let pack = try XCTUnwrap(PevScreenCatalog.live.screen(id: .eucGarage))

        let presented = PevScreenCatalog.live.presentedScreen(for: pack, liveBmsSnapshot: nil)

        XCTAssertEqual(presented.id, .bmsNoData)
        XCTAssertEqual(presented.secondaryValue, "no live BMS")
        XCTAssertEqual(presented.bmsContent?.snapshot.availability, .unavailable)
        XCTAssertNil(presented.bmsContent?.snapshot.energyPercent)
        XCTAssertNil(presented.bmsContent?.snapshot.voltage)
        XCTAssertTrue(presented.bmsContent?.snapshot.groups.isEmpty ?? false)
    }
}
