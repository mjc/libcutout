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
            accent: .yellow,
            scale: 1
        )

        XCTAssertEqual(tile.accessibilityValueText, "84, volts, stale")
    }

    func testProgressSemanticsClampOutOfRangeValues() {
        XCTAssertEqual(makeProgressBar(progress: -0.4).clampedProgress, 0)
        XCTAssertEqual(makeProgressBar(progress: 0.42).clampedProgress, 0.42)
        XCTAssertEqual(makeProgressBar(progress: 1.8).clampedProgress, 1)
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
            valueColor: .primary,
            scale: 1
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
            scale: 1,
            accessibilityValue: accessibilityValue
        )
    }
}
