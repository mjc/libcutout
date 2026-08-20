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

    func testProgressSemanticsKeepsTypedUnavailableStateOutOfTheFill() {
        let bar = PevDashboardProgressBar(
            label: "Headroom",
            metricValue: .unavailable,
            progress: nil
        )

        XCTAssertNil(bar.clampedProgress)
        XCTAssertEqual(bar.metricValue.accessibilityText, "unavailable")
    }

    func testProgressSemanticsRejectsAFillForNonNumericMetricState() {
        let values: [PevDashboardMetricValue] = [
            .unavailable,
            .status(display: "Not applicable", accessibility: "Not applicable"),
        ]

        for metricValue in values {
            let bar = PevDashboardProgressBar(
                label: "Headroom",
                metricValue: metricValue,
                progress: 0.77
            )

            XCTAssertNil(bar.clampedProgress)
        }
    }

    func testProgressCardUsesTheSameTypedUnavailableState() {
        let card = PevDashboardProgressCard(
            label: "Headroom",
            metricValue: .unavailable,
            detail: "",
            progress: nil
        )

        XCTAssertEqual(card.metricValue, .unavailable)
        XCTAssertNil(card.progress)
    }

    func testHeroProgressAndReadbackUseTheSameTypedValues() {
        let card = PevDashboardHeroCard(
            eyebrow: "Battery",
            metricValue: .available(display: "82", accessibility: "82"),
            unit: "percent",
            detail: "charging",
            progress: 1.4
        )

        XCTAssertEqual(card.clampedProgress, 1)
        XCTAssertEqual(card.accessibilityValueText, "82, percent, and charging")
    }

    func testHeroCardSpeaksTypedUnavailableValue() {
        let card = PevDashboardHeroCard(
            eyebrow: "Battery",
            metricValue: .unavailable,
            unit: "percent",
            detail: "charging",
            progress: 0.72
        )

        XCTAssertNil(card.clampedProgress)
        XCTAssertEqual(card.accessibilityValueText, "unavailable")
    }

    func testHeroCanUseVisualUnitWithoutRepeatingItToVoiceOver() {
        let card = PevDashboardHeroCard(
            eyebrow: "Battery",
            metricValue: .available(display: "72", accessibility: "72%"),
            unit: "%",
            accessibilityUnit: "",
            detail: "split pack",
            progress: 0.72
        )

        XCTAssertEqual(card.accessibilityValueText, "72% and split pack")
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

    func testWarningToneOwnsTheObservedVescPresentation() {
        let card = PevDashboardWarningCard(
            title: "Telemetry pending",
            detail: "Waiting for the first sample",
            tone: .vesc
        )

        XCTAssertEqual(card.tone, .vesc)
        XCTAssertEqual(card.cornerRadius, 24)
    }

    func testStatusStripUsesVisibleTextAsItsAccessibilityLabel() {
        let cases: [(PevStatusStripTone, String)] = [
            (.nominal, ""),
            (.critical, "critical"),
        ]

        for (tone, expectedAccessibilityValue) in cases {
            let strip = PevStatusStrip(text: "Saved capture", tone: tone)

            XCTAssertEqual(strip.accessibilityLabelText, "Saved capture")
            XCTAssertEqual(strip.accessibilityValueText, expectedAccessibilityValue)
            XCTAssertEqual(strip.tone, tone)
        }
    }

    func testPickerStateBuildsItsSharedStatusPill() {
        let supported = PevDashboardStatusPill(
            devicePickerState: .supported(action: "Use")
        )
        let unsupported = PevDashboardStatusPill(
            devicePickerState: .unsupported(action: "Unavailable")
        )

        XCTAssertEqual(supported.title, "Use")
        XCTAssertEqual(supported.tone, .pickerSupported)
        XCTAssertEqual(supported.width, 76)
        XCTAssertEqual(supported.height, 38)
        XCTAssertFalse(supported.fixedHorizontal)
        XCTAssertEqual(unsupported.title, "Unavailable")
        XCTAssertEqual(unsupported.tone, .pickerUnavailable)
        XCTAssertEqual(unsupported.width, 64)
        XCTAssertEqual(unsupported.height, 30)
        XCTAssertFalse(unsupported.fixedHorizontal)
    }

    func testStatusPillTonePreservesRideAndWarningSemantics() {
        let cases: [(PevDashboardStatusPillTone, String, CGFloat)] = [
            (.eucRide, "", 30),
            (.vescRide, "", 30),
            (.warning, "warning", 30),
            (.pickerSupported, "", 38),
            (.pickerUnavailable, "", 30),
        ]

        for (tone, expectedAccessibilityValue, expectedHeight) in cases {
            let pill = PevDashboardStatusPill(title: "Status", tone: tone)

            XCTAssertEqual(pill.tone, tone)
            XCTAssertEqual(pill.accessibilityValueText, expectedAccessibilityValue)
            XCTAssertEqual(pill.height, expectedHeight)
        }
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

    func testAccessibleForegroundOverridesColorForContrastOrColorDifferentiation() {
        let original = PevDashboardColors.warningText

        XCTAssertEqual(
            pevDashboardResolvedForegroundColor(
                original,
                contrast: .standard,
                differentiateWithoutColor: false,
                colorScheme: .light
            ),
            original
        )
        XCTAssertEqual(
            pevDashboardResolvedForegroundColor(
                original,
                contrast: .increased,
                differentiateWithoutColor: false,
                colorScheme: .light
            ),
            .black
        )
        XCTAssertEqual(
            pevDashboardResolvedForegroundColor(
                original,
                contrast: .standard,
                differentiateWithoutColor: true,
                colorScheme: .dark
            ),
            .white
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

        XCTAssertEqual(readout.title, "Footpad")
        XCTAssertEqual(
            readout.accessibilityValueText,
            "left, 1.2 volts, available, right, unavailable, and engaged"
        )
    }

    func testFootpadReadoutLocalizesItsDefaultSideLabels() {
        let readout = PevDashboardFootpadReadout(
            leftMetricValue: .available(display: "1.2", accessibility: "1.2, available"),
            rightMetricValue: .unavailable,
            detail: "engaged"
        )

        XCTAssertEqual(readout.leftLabel, "Left")
        XCTAssertEqual(readout.rightLabel, "Right")
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.left", value: nil, table: "Localizable"),
            "Left"
        )
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.right", value: nil, table: "Localizable"),
            "Right"
        )
    }

    func testFootpadReadoutBuildsTypedAvailabilitySemantics() {
        let readout = makeFootpadReadout()

        XCTAssertEqual(
            readout.accessibilityValueText,
            "left, 1.2 volts, available, right, unavailable, and engaged"
        )
    }

    private func makeProgressBar(progress: Double) -> PevDashboardProgressBar {
        PevDashboardProgressBar(
            label: "Headroom",
            metricValue: .available(display: "42 percent", accessibility: "42 percent"),
            progress: progress
        )
    }

    private func makeFootpadReadout() -> PevDashboardFootpadReadout {
        PevDashboardFootpadReadout(
            leftLabel: "left",
            leftMetricValue: .available(display: "1.2 volts", accessibility: "1.2 volts, available"),
            rightLabel: "right",
            rightMetricValue: .unavailable,
            detail: "engaged"
        )
    }
}
