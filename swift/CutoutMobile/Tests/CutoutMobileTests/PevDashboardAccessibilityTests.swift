import SwiftUI
import XCTest
@testable import CutoutMobile

@MainActor
final class PevDashboardAccessibilityTests: XCTestCase {
    func testMetricSemanticsGroupValueUnitAndDetail() {
        let tile = PevDashboardMetricTile(
            label: "Pack voltage",
            value: "84",
            unit: "volts",
            detail: "stale",
            accent: .yellow
        )

        XCTAssertEqual(tile.accessibilityValueText, "84, volts, stale")
    }

    func testUnavailableMetricUsesTypedSpokenValueInsteadOfItsDisplaySentinel() {
        let tile = PevDashboardMetricTile(
            label: "Pack voltage",
            value: PevDashboardMetricValue.unavailable.displayText,
            unit: "volts",
            detail: "stale",
            accessibilityValue: PevDashboardMetricValue.unavailable.accessibilityText,
            accent: .yellow
        )

        XCTAssertEqual(tile.accessibilityValueText, "unavailable")
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
            progress: 1.4,
            accent: .green
        )

        XCTAssertEqual(card.clampedProgress, 1)
        XCTAssertEqual(card.accessibilityValueText, "82, percent, charging")
    }

    func testWideCardWithoutTitleUsesValueAsItsAccessibilityLabel() {
        let card = PevDashboardWideCard(
            title: nil,
            value: "Trend stable",
            detail: "No change",
            accent: .yellow
        )

        XCTAssertEqual(card.accessibilityLabelText, "Trend stable")
        XCTAssertEqual(card.accessibilityValueText, "No change")
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

    func testScanningAnimationRunsOnlyWhileScanningAndMotionIsAllowed() {
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

    func testFootpadReadoutBuildsACompleteDefaultAccessibilityValue() {
        let readout = makeFootpadReadout()

        XCTAssertEqual(
            readout.accessibilityValueText,
            "left, 1.2 volts, right, unavailable, engaged"
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
            progress: progress,
            accent: .yellow,
            track: .gray,
            labelColor: .primary,
            valueColor: .primary
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
            accent: .cyan,
            fill: .gray,
            stroke: .gray,
            textColor: .primary,
            secondaryTextColor: .secondary,
            accessibilityValue: accessibilityValue
        )
    }
}
