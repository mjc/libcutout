import XCTest
import CutoutMobileFFI
import SwiftUI
@testable import CutoutMobile

final class PevScreenCatalogTests: XCTestCase {
    func testAdaptiveDashboardGridWidensOnlyForAccessibilityTextSizes() {
        XCTAssertEqual(
            PevDashboardGrid<EmptyView>.adaptiveMinimumColumnWidth(
                for: .large,
                default: 150,
                accessibility: 240
            ),
            150
        )
        XCTAssertEqual(
            PevDashboardGrid<EmptyView>.adaptiveMinimumColumnWidth(
                for: .accessibility3,
                default: 150,
                accessibility: 240
            ),
            240
        )
    }

    func testDashboardMetricAccessibilityKeepsUnavailableTyped() {
        XCTAssertEqual(
            PevDashboardMetricValue.unavailable.accessibilityValue(unit: "V", detail: "stale"),
            "unavailable"
        )
        XCTAssertEqual(
            PevDashboardMetricValue.available(display: "84", accessibility: "84").accessibilityValue(
                unit: "V",
                detail: "fresh"
            ),
            "84, V, fresh"
        )
    }

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
        XCTAssertEqual(PevRideTabs.eucRideTabs()[1].destinationTarget, .eucPack)
        XCTAssertEqual(PevRideTabs.vescRideTabs().first?.destinationTarget, .vescRide)
        XCTAssertEqual(PevRideTabs.vescRideTabs()[1].destinationTarget, .screen(.vescDebug))
    }

    func testSelectedTabSemanticsTrackExplicitRoutes() {
        XCTAssertTrue(PevRideTabs.eucRideTabs(selected: .bmsOverview)[1].isSelected)
        XCTAssertTrue(PevRideTabs.vescRideTabs(selected: .vescRide)[0].isSelected)
        XCTAssertFalse(PevRideTabs.vescRideTabs(selected: .vescDebug)[0].isSelected)
        XCTAssertTrue(PevRideTabs.vescRideTabs(selected: .vescDebug)[1].isSelected)
    }

    func testTabIdentityDoesNotDependOnVisibleTitle() {
        let ride = PevScreenTab(id: .ride, title: "Ride", isSelected: true)
        let localizedRide = PevScreenTab(id: .ride, title: "Conduire", isSelected: true)
        let differentRoleWithSameTitle = PevScreenTab(id: .debug, title: "Ride", isSelected: false)

        XCTAssertEqual(ride.id, localizedRide.id)
        XCTAssertNotEqual(ride.id, differentRoleWithSameTitle.id)
        XCTAssertEqual(ride.accessibilityIdentifier, "dashboard.nav.ride")
        XCTAssertEqual(ride.accessibilityIdentifier, localizedRide.accessibilityIdentifier)
        XCTAssertNotEqual(ride.accessibilityIdentifier, differentRoleWithSameTitle.accessibilityIdentifier)
    }

    func testDashboardTileIdentityDoesNotDependOnVisibleLabel() {
        let battery = PevDashboardTile(
            kind: .batteryCurrent,
            label: "battery current",
            value: "12",
            unit: "A",
            detail: "live telemetry",
            accent: .yellow
        )
        let localizedBattery = PevDashboardTile(
            kind: .batteryCurrent,
            label: "courant de batterie",
            value: "12",
            unit: "A",
            detail: "télémétrie en direct",
            accent: .yellow
        )
        let motorWithSameLabel = PevDashboardTile(
            kind: .motorCurrent,
            label: "battery current",
            value: "12",
            unit: "A",
            detail: "live telemetry",
            accent: .orange
        )

        XCTAssertEqual(battery.id, localizedBattery.id)
        XCTAssertNotEqual(battery.id, motorWithSameLabel.id)
    }

    func testSafetyBarIdentityDoesNotDependOnVisibleLabel() {
        let headroom = PevSafetyBar(
            id: .pwmHeadroom,
            label: "PWM headroom",
            value: "75%",
            progress: 0.75,
            accent: .yellow
        )
        let localizedHeadroom = PevSafetyBar(
            id: .pwmHeadroom,
            label: "marge PWM",
            value: "75 %",
            progress: 0.75,
            accent: .yellow
        )
        let energyWithSameLabel = PevSafetyBar(
            id: .sagAdjustedEnergy,
            label: "PWM headroom",
            value: "75%",
            progress: 0.75,
            accent: .cyan
        )

        XCTAssertEqual(headroom.id, localizedHeadroom.id)
        XCTAssertNotEqual(headroom.id, energyWithSameLabel.id)
    }

    func testDevicePickerScanningHasNoInventedRows() {
        let state = DevicePickerScanState.scanning

        XCTAssertEqual(state.status, .scanning)
        XCTAssertTrue(state.rows.isEmpty)
        XCTAssertTrue(state.sections.supported.isEmpty)
        XCTAssertTrue(state.sections.unsupported.isEmpty)
        XCTAssertNil(state.sections.manual)
    }

    func testDevicePickerActionsHaveUniqueSpokenNamesForDuplicateModels() {
        let first = DevicePickerRow(
            id: "0000-00A1",
            title: "Twin Board",
            subtitle: "VESC",
            detail: "first nearby board",
            state: .supported(action: "Use"),
            symbolName: "circle.hexagongrid.circle"
        )
        let second = DevicePickerRow(
            id: "0000-00B2",
            title: "Twin Board",
            subtitle: "VESC",
            detail: "second nearby board",
            state: .supported(action: "Use"),
            symbolName: "circle.hexagongrid.circle"
        )

        XCTAssertEqual(first.useActionAccessibilityLabel, "Use Twin Board, device 00A1")
        XCTAssertEqual(first.captureActionAccessibilityLabel, "Start capture for Twin Board, device 00A1")
        XCTAssertNotEqual(first.useActionAccessibilityLabel, second.useActionAccessibilityLabel)
        XCTAssertNotEqual(first.captureActionAccessibilityLabel, second.captureActionAccessibilityLabel)
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

        let presented = PevScreenCatalog.live.presentedBmsScreen(liveBmsSnapshot: snapshot)

        XCTAssertEqual(presented.id, .bmsCellMap6S)
        XCTAssertEqual(presented.bmsContent?.snapshot, snapshot)
        XCTAssertEqual(presented.bmsContent?.highlightedGroupIndices, [])
        XCTAssertEqual(presented.bmsContent?.modeTitles, ["balance view"])
    }

    func testBmsPresentationIdentityDoesNotDependOnOrder() {
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
            modes: [.rawTable, .overview]
        )

        XCTAssertNotEqual(chips[0].id, chips[1].id)
        XCTAssertEqual(content.modes.map(\.id), [.rawTable, .overview])
        XCTAssertEqual(content.modeTitles, ["raw table", "overview"])
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
        let presented = PevScreenCatalog.live.presentedBmsScreen(liveBmsSnapshot: nil)

        XCTAssertEqual(presented.id, .bmsNoData)
        XCTAssertEqual(presented.secondaryValue, "no live BMS")
        XCTAssertEqual(presented.bmsContent?.snapshot.availability, .unavailable)
        XCTAssertNil(presented.bmsContent?.snapshot.energyPercent)
        XCTAssertNil(presented.bmsContent?.snapshot.voltage)
        XCTAssertTrue(presented.bmsContent?.snapshot.groups.isEmpty ?? false)
    }
}
