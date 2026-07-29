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
        let unavailable = pevLocalizedText("metric.availability.unavailable")

        XCTAssertEqual(
            PevDashboardMetricValue.unavailable.accessibilityValue(unit: "V", detail: "stale"),
            unavailable
        )
        XCTAssertEqual(
            PevDashboardMetricValue.available(display: "84", accessibility: "84").accessibilityValue(
                unit: "V",
                detail: "fresh"
            ),
            "84, V, and fresh"
        )
    }

    func testLiveCatalogContainsOnlyProductionRouteMetadata() {
        let unavailable = pevLocalizedText("metric.availability.unavailable")

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
            screen.secondaryValue == unavailable
                && screen.bmsContent == nil
        })
    }

    func testScreenWithoutBmsContentUsesTheLocalizedUnavailableBmsPresentation() {
        let screen = PevScreen(
            id: .bmsUnknownTopology,
            title: "BMS",
            subtitle: "Unavailable",
            secondaryValue: "Unavailable"
        )

        let content = screen.bmsContentOrUnavailable

        XCTAssertEqual(content.kind, .noData)
        XCTAssertEqual(content.snapshot.availability, .unavailable)
        XCTAssertEqual(content.snapshot.topology.layoutLabel, "Live BMS readback unavailable")
        XCTAssertEqual(content.chips.map(\.title), ["no live BMS"])
    }

    func testBmsAndDashboardMetadataResolveFromThePackageCatalog() {
        let catalog = PevScreenCatalog.live

        XCTAssertEqual(catalog.screen(id: .eucRide)?.title, "EUC ride")
        XCTAssertEqual(catalog.screen(id: .eucRide)?.subtitle, "Live telemetry")
        XCTAssertEqual(catalog.screen(id: .bmsCellDetail)?.title, "Cell detail")
        XCTAssertEqual(catalog.screen(id: .bmsCellDetail)?.subtitle, "Live BMS readback")
        XCTAssertEqual(catalog.screen(id: .bmsUnknownTopology)?.subtitle, "Topology unavailable")
        XCTAssertEqual(catalog.screen(id: .vescDebug)?.title, "VESC state")
        XCTAssertEqual(pevLocalizedText("bms.chip.bms_online", Int64(2)), "2 BMS online")
        XCTAssertEqual(pevLocalizedText("bms.subtitle.controller_estimate", "6S pack"), "6S pack · controller-only estimate")
    }

    func testChargeEstimatePresentationResolvesFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("charge.estimate.value.collecting"), "estimating")
        XCTAssertEqual(
            pevLocalizedText("charge.estimate.detail.collecting.plural", Int64(2)),
            "estimating charge time · 2 samples"
        )
        XCTAssertEqual(pevLocalizedText("charge.estimate.kind.profile_backed"), "profile-backed")
        XCTAssertEqual(pevLocalizedText("charge.estimate.confidence.medium"), "medium")
        XCTAssertEqual(
            pevLocalizedText("charge.estimate.unavailable.capacity_missing"),
            "usable pack capacity unavailable"
        )
        XCTAssertEqual(pevLocalizedText("charge.estimate.duration.under_minute"), "under 1 min")
        XCTAssertEqual(pevLocalizedText("charge.estimate.duration.minutes", Int64(45)), "45 min")
        XCTAssertEqual(pevLocalizedText("charge.estimate.duration.hours_minutes", Int64(1), Int64(30)), "1h 30m")
    }

    func testReadbackAvailabilityResolvesFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("readback.availability.available"), "available")
        XCTAssertEqual(pevLocalizedText("readback.availability.unavailable"), "unavailable")
        XCTAssertEqual(pevLocalizedText("readback.availability.unsupported"), "unsupported")
    }

    func testEucConnectionAndRidePresentationResolveFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("euc.connection.starting"), "Starting Bluetooth...")
        XCTAssertEqual(pevLocalizedText("euc.connection.unavailable", Int64(4)), "Bluetooth unavailable: state 4")
        XCTAssertEqual(pevLocalizedText("euc.connection.scanning"), "Scanning for rides...")
        XCTAssertEqual(pevLocalizedText("euc.connection.connecting", "Aero"), "Connecting to Aero...")
        XCTAssertEqual(pevLocalizedText("euc.connection.discovering_services"), "Discovering services...")
        XCTAssertEqual(pevLocalizedText("euc.connection.subscribing"), "Subscribing...")
        XCTAssertEqual(pevLocalizedText("euc.connection.live"), "Live")
        XCTAssertEqual(pevLocalizedText("euc.failure.missing_notify_channel"), "Missing notify channel")
        XCTAssertEqual(pevLocalizedText("euc.failure.missing_write_channel"), "Missing write channel")
        XCTAssertEqual(pevLocalizedText("euc.failure.session", "link dropped"), "Session failed: link dropped")
        XCTAssertEqual(pevLocalizedText("euc.failure.connect", "link dropped"), "Connect failed: link dropped")
        XCTAssertEqual(
            pevLocalizedText("euc.failure.service_discovery", "link dropped"),
            "Service discovery failed: link dropped"
        )
        XCTAssertEqual(
            pevLocalizedText("euc.failure.characteristic_discovery", "link dropped"),
            "Characteristic discovery failed: link dropped"
        )
        XCTAssertEqual(pevLocalizedText("euc.failure.notification", "link dropped"), "Notification failed: link dropped")
        XCTAssertEqual(
            pevLocalizedText("euc.failure.notification_ingest", "link dropped"),
            "Notification ingest failed: link dropped"
        )
        XCTAssertEqual(pevLocalizedText("euc.warning.connection_failed"), "Connection failed")
        XCTAssertEqual(pevLocalizedText("euc.warning.reduce_acceleration"), "Reduce acceleration")
        XCTAssertEqual(pevLocalizedText("euc.warning.low_pwm_headroom"), "PWM headroom is low while riding")
        XCTAssertEqual(pevLocalizedText("euc.warning.telemetry_live"), "Telemetry live")
        XCTAssertEqual(pevLocalizedText("euc.warning.waiting_for_speed"), "Waiting for speed telemetry")
        XCTAssertEqual(pevLocalizedText("euc.warning.live_telemetry_detail"), "Live telemetry from typed Rust/FFI state")
        XCTAssertEqual(pevLocalizedText("euc.warning.waiting_for_telemetry"), "Waiting for telemetry")
        XCTAssertEqual(pevLocalizedText("euc.warning.subscribed_no_values"), "Subscribed; no ride values yet")
        XCTAssertEqual(pevLocalizedText("euc.warning.telemetry_unavailable"), "Telemetry unavailable")
        XCTAssertEqual(pevLocalizedText("euc.warning.no_live_snapshot"), "No live snapshot yet")
        XCTAssertEqual(pevLocalizedText("euc.warning.waiting_for_live_telemetry"), "Waiting for live telemetry")
        XCTAssertEqual(pevLocalizedText("euc.warning.screen_inactive"), "Ride screen is not active yet")
        XCTAssertEqual(pevLocalizedText("euc.warning.telemetry_stale"), "Telemetry stale")
        XCTAssertEqual(pevLocalizedText("euc.warning.last_update", "3000"), "Last update 3000 ms ago")
        XCTAssertEqual(pevLocalizedText("euc.status.parked"), "Parked")
        XCTAssertEqual(pevLocalizedText("euc.status.standing"), "Standing")
        XCTAssertEqual(pevLocalizedText("euc.status.riding"), "Riding")
        XCTAssertEqual(pevLocalizedText("euc.status.charging"), "Charging")
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

    func testSharedPickerAndNavigationCopyResolvesFromThePackageCatalog() {
        let row = DevicePickerRow(
            id: "vesc-1234",
            title: "VESC",
            subtitle: "",
            detail: "",
            state: DevicePickerRowState(action: .probe),
            symbolName: "bolt"
        )

        XCTAssertEqual(row.captureActionTitle, "Start probe")
        XCTAssertEqual(row.useActionAccessibilityLabel, "Use VESC, device 1234")
        XCTAssertEqual(row.captureActionAccessibilityLabel, "Start probe for VESC, device 1234")
        XCTAssertEqual(DevicePickerRowState(action: .confirm).actionTitle, "Confirm")
        XCTAssertEqual(DevicePickerRowState(action: .review).actionTitle, "Review")
        XCTAssertEqual(DevicePickerRowState(action: .later).actionTitle, "Later")
        XCTAssertEqual(PevRideTabs.eucRideTabs().map(\.title), ["Ride", "Pack", "Map", "Tune"])
        XCTAssertEqual(PevRideTabs.vescRideTabs()[2].disabledReason, "Map is not available yet.")
    }

    func testSharedPickerStatusCopyResolvesFromThePackageCatalog() {
        XCTAssertEqual(DevicePickerScanState.scanning.statusText, "Scanning Bluetooth")
        XCTAssertEqual(DevicePickerScanState(status: .idle, rows: []).statusText, "No rideable devices found")
        XCTAssertEqual(
            DevicePickerScanState(
                status: .idle,
                rows: [
                    DevicePickerRow(
                        title: "VESC",
                        subtitle: "",
                        detail: "",
                        state: DevicePickerRowState(action: .use),
                        symbolName: "bolt"
                    ),
                ]
            ).statusText,
            "Bluetooth scan complete"
        )
        XCTAssertEqual(DevicePickerScanState(status: .bluetoothUnavailable, rows: []).statusText, "Bluetooth unavailable")
        XCTAssertEqual(DevicePickerScanState(status: .permissionDenied, rows: []).statusText, "Bluetooth permission denied")
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
            metricValue: .available(display: "12", accessibility: "12"),
            unit: "A",
            detail: "live telemetry",
            accent: .yellow
        )
        let localizedBattery = PevDashboardTile(
            kind: .batteryCurrent,
            label: "courant de batterie",
            metricValue: .available(display: "12", accessibility: "12"),
            unit: "A",
            detail: "télémétrie en direct",
            accent: .yellow
        )
        let motorWithSameLabel = PevDashboardTile(
            kind: .motorCurrent,
            label: "battery current",
            metricValue: .available(display: "12", accessibility: "12"),
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
            metricValue: .available(display: "75%", accessibility: "75%"),
            progress: 0.75,
            accent: .yellow
        )
        let localizedHeadroom = PevSafetyBar(
            id: .pwmHeadroom,
            label: "marge PWM",
            metricValue: .available(display: "75 %", accessibility: "75 %"),
            progress: 0.75,
            accent: .yellow
        )
        let energyWithSameLabel = PevSafetyBar(
            id: .sagAdjustedEnergy,
            label: "PWM headroom",
            metricValue: .available(display: "75%", accessibility: "75%"),
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
        XCTAssertEqual(presented.bmsContent?.modes.map(\.title), ["balance view"])
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
        XCTAssertEqual(content.modes.map(\.title), ["raw table", "overview"])
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
