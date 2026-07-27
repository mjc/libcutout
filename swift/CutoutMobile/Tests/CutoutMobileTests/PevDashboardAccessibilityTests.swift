import SwiftUI
import XCTest
@testable import CutoutMobile

@MainActor
final class PevDashboardAccessibilityTests: XCTestCase {
    func testRideDisconnectControlCopyResolvesFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("ride.action.disconnect"), "Disconnect")
    }

    func testMetricSemanticsGroupValueUnitAndDetail() {
        let tile = PevDashboardMetricTile(
            label: "Pack voltage",
            metricValue: .available(display: "84", accessibility: "84"),
            unit: "volts",
            detail: "stale"
        )

        XCTAssertEqual(tile.accessibilityValueText, "84, volts, and stale")
    }

    func testMetricTileProminenceOwnsDashboardGeometry() {
        let compact = PevDashboardMetricTile(
            label: "Speed",
            metricValue: .available(display: "12", accessibility: "12"),
            prominence: .compactDashboard
        )
        let dashboard = PevDashboardMetricTile(
            label: "Speed",
            metricValue: .available(display: "12", accessibility: "12"),
            prominence: .dashboard
        )
        let standard = PevDashboardMetricTile(
            label: "Speed",
            metricValue: .available(display: "12", accessibility: "12")
        )

        XCTAssertEqual(compact.cornerRadius, 16)
        XCTAssertEqual(compact.minHeight, 96)
        XCTAssertEqual(dashboard.cornerRadius, 16)
        XCTAssertEqual(dashboard.minHeight, 104)
        XCTAssertEqual(standard.cornerRadius, 20)
        XCTAssertEqual(standard.minHeight, 106)
    }

    func testTypedDashboardTileKeepsItsAccessibilityValue() {
        let tile = PevDashboardMetricTile(
            PevDashboardTile(
                kind: .chargeEstimate,
                label: "Charge",
                metricValue: .status(display: "stale", accessibility: "stale"),
                unit: "",
                detail: "waiting for telemetry",
                accent: .green
            )
        )

        XCTAssertEqual(tile.accessibilityValueText, "stale and waiting for telemetry")
    }

    func testUnavailableMetricRequiresTypedPresentation() {
        let tile = PevDashboardMetricTile(
            label: "Pack voltage",
            metricValue: .unavailable,
            unit: "volts",
            detail: "stale"
        )

        XCTAssertEqual(tile.metricValue, .unavailable)
        XCTAssertEqual(tile.accessibilityValueText, "unavailable")
    }

    func testStatusMetricKeepsItsStateAndDetailForAccessibility() {
        let tile = PevDashboardMetricTile(
            label: "Charge",
            metricValue: .status(display: "stale", accessibility: "stale"),
            unit: "",
            detail: "waiting for fresh telemetry"
        )

        XCTAssertEqual(tile.metricValue.displayText, "stale")
        XCTAssertEqual(tile.accessibilityValueText, "stale and waiting for fresh telemetry")
    }

    func testKeyValueRowSpeaksTypedUnavailableValue() {
        let row = PevDashboardKeyValueRow(
            id: "pack-voltage",
            label: "Pack voltage",
            metricValue: .unavailable
        )

        XCTAssertEqual(row.value, "--")
        XCTAssertEqual(row.accessibilityValueText, "unavailable")
    }

    func testProgressSemanticsClampOutOfRangeValues() {
        XCTAssertEqual(makeProgressBar(progress: -0.4).clampedProgress, 0)
        XCTAssertEqual(makeProgressBar(progress: 0.42).clampedProgress, 0.42)
        XCTAssertEqual(makeProgressBar(progress: 1.8).clampedProgress, 1)
    }

    func testHeroProgressAndReadbackUseTheSameTypedValues() {
        let card = PevDashboardHeroCard(
            eyebrow: "Battery",
            value: "82",
            unit: "percent",
            detail: "charging",
            progress: 1.4
        )

        XCTAssertEqual(card.clampedProgress, 1)
        XCTAssertEqual(card.accessibilityValueText, "82, percent, and charging")
    }

    func testWideCardWithoutTitleUsesValueAsItsAccessibilityLabel() {
        let card = PevDashboardWideCard(
            title: nil,
            metricValue: .available(display: "Trend stable", accessibility: "Trend stable"),
            detail: "No change"
        )

        XCTAssertEqual(card.accessibilityLabelText, "Trend stable")
        XCTAssertEqual(card.accessibilityValueText, "No change")
    }

    func testWideCardSpeaksUnavailableInsteadOfTheDisplaySentinel() {
        let card = PevDashboardWideCard(
            title: "Pack telemetry",
            metricValue: .unavailable,
            detail: "connect device"
        )

        XCTAssertEqual(card.accessibilityValueText, "unavailable")
    }

    func testWarningCardUsesTypedDetailAsItsAccessibilityValue() {
        let card = PevDashboardWarningCard(
            title: "Connection warning",
            detail: "Retrying in 5 seconds",
            accent: .orange,
            detailColor: .primary,
            fill: .secondary,
            stroke: .orange
        )

        XCTAssertEqual(card.accessibilityValueText, "Retrying in 5 seconds")
    }

    func testStatusStripUsesVisibleTextAsItsAccessibilityLabel() {
        let strip = PevStatusStrip(text: "Saved capture")

        XCTAssertEqual(strip.accessibilityLabelText, "Saved capture")
    }

    func testPickerStateBuildsItsSharedStatusPill() {
        let supported = PevDashboardStatusPill(
            devicePickerState: .supported(action: "Use")
        )
        let unsupported = PevDashboardStatusPill(
            devicePickerState: .unsupported(action: "Unavailable")
        )

        XCTAssertEqual(supported.title, "Use")
        XCTAssertEqual(supported.width, 76)
        XCTAssertEqual(supported.height, 38)
        XCTAssertEqual(unsupported.title, "Unavailable")
        XCTAssertEqual(unsupported.width, 64)
        XCTAssertEqual(unsupported.height, 30)
    }

    func testStatusPillTonePreservesRideAndWarningSemantics() {
        let euc = PevDashboardStatusPill(title: "Riding", tone: .eucRide)
        let vesc = PevDashboardStatusPill(title: "Riding", tone: .vescRide)
        let warning = PevDashboardStatusPill(title: "Limited data", tone: .warning)

        XCTAssertEqual(euc.tone, .eucRide)
        XCTAssertEqual(vesc.tone, .vescRide)
        XCTAssertEqual(warning.tone, .warning)
        XCTAssertEqual(euc.height, 30)
        XCTAssertEqual(vesc.height, 30)
        XCTAssertEqual(warning.height, 30)
    }

    func testScanningAnimationRunsOnlyWhileScanningAndMotionIsAllowed() {
        XCTAssertTrue(PevDashboardScanningPill.showsIndicators(isScanning: true))
        XCTAssertFalse(PevDashboardScanningPill.showsIndicators(isScanning: false))
        XCTAssertTrue(
            PevDashboardScanningPill.shouldAnimate(
                isScanning: true,
                reduceMotion: false
            )
        )
        XCTAssertFalse(
            PevDashboardScanningPill.shouldAnimate(
                isScanning: false,
                reduceMotion: false
            )
        )
        XCTAssertFalse(
            PevDashboardScanningPill.shouldAnimate(
                isScanning: true,
                reduceMotion: true
            )
        )
    }

    func testCardBordersStrengthenForIncreasedContrast() {
        XCTAssertEqual(
            PevDashboardCardBackground.resolvedLineWidth(base: 1, contrast: .standard),
            1
        )
        XCTAssertEqual(
            PevDashboardCardBackground.resolvedLineWidth(base: 1, contrast: .increased),
            2
        )
        XCTAssertEqual(
            PevDashboardCardBackground.resolvedLineWidth(base: 2, contrast: .increased),
            3
        )
        XCTAssertEqual(
            pevDashboardResolvedLineWidth(base: 0.8, contrast: .increased),
            2
        )
    }

    func testDashedCardBackgroundKeepsItsSharedDashPattern() {
        let background = PevDashboardCardBackground(
            cornerRadius: 18,
            lineWidth: 1.2,
            dashPattern: [5, 5]
        )

        XCTAssertEqual(background.dashPattern, [5, 5])
        XCTAssertEqual(
            PevDashboardCardBackground.resolvedLineWidth(base: background.lineWidth, contrast: .increased),
            2
        )
    }

    func testFootpadReadoutBuildsACompleteDefaultAccessibilityValue() {
        let readout = makeFootpadReadout()

        XCTAssertEqual(
            readout.accessibilityValueText,
            "left, 1.2 volts, right, unavailable, and engaged"
        )
    }

    func testFootpadReadoutAcceptsTypedAvailabilitySemantics() {
        let readout = makeFootpadReadout(
            accessibilityValue: "left, 1.2 volts, available, right, unavailable, engaged"
        )

        XCTAssertEqual(
            readout.accessibilityValueText,
            "left, 1.2 volts, available, right, unavailable, engaged"
        )
    }

    private func makeProgressBar(progress: Double) -> PevDashboardProgressBar {
        PevDashboardProgressBar(
            label: "Headroom",
            value: "42 percent",
            progress: progress
        )
    }

    private func makeFootpadReadout(
        accessibilityValue: String? = nil
    ) -> PevDashboardFootpadReadout {
        PevDashboardFootpadReadout(
            leftLabel: "left",
            leftValue: "1.2 volts",
            rightLabel: "right",
            rightValue: "unavailable",
            detail: "engaged",
            accessibilityValue: accessibilityValue
        )
    }
}
