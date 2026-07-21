import XCTest
@testable import CutoutApp
import CutoutMobile

final class PevScreenThemeTests: XCTestCase {
    func testRideHeroTypographyMatchesReferenceAtDefaultTextSize() {
        XCTAssertEqual(PevRideHeroStyle.electricUnicycle.speedPointSize, 104)
        XCTAssertEqual(PevRideHeroStyle.vescOnewheel.speedPointSize, 96)
        XCTAssertEqual(PevRideHeroStyle.unitPointSize, 24)
    }

    func testRideHeroReadoutSharesExplicitAvailabilitySemantics() {
        let unavailable = PevRideHeroReadout.unavailable
        XCTAssertEqual(unavailable.displayValue, "Unavailable")
        XCTAssertEqual(unavailable.displayUnit, "")
        XCTAssertEqual(unavailable.accessibilityValue, "unavailable")
        XCTAssertFalse(unavailable.isAvailable)

        let available = PevRideHeroReadout.available(value: "19", unit: "mph")
        XCTAssertEqual(available.displayValue, "19")
        XCTAssertEqual(available.displayUnit, "mph")
        XCTAssertEqual(available.accessibilityValue, "19, mph")
        XCTAssertTrue(available.isAvailable)
    }

    func testPowerFlowDetailUsesPlainStateWords() {
        XCTAssertEqual(powerFlowDetail(.discharge, fallback: "fallback"), "discharging")
        XCTAssertEqual(powerFlowDetail(.zero, fallback: "fallback"), "idle")
        XCTAssertEqual(powerFlowDetail(.charging, fallback: "fallback"), "charging input")
        XCTAssertEqual(powerFlowDetail(.regeneration, fallback: "fallback"), "regen")
        XCTAssertEqual(powerFlowDetail(.negativeUnknown, fallback: "fallback"), "regen/discharge unverified")
        XCTAssertEqual(powerFlowDetail(nil, fallback: "fallback"), "fallback")
    }

    @MainActor
    func testBmsAlertIndicatorAppearsOnlyWhenColorCannotCarrySeverity() {
        XCTAssertNil(BmsAlertIndicator.systemImageName(
            for: .critical,
            differentiateWithoutColor: false
        ))
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .critical,
            differentiateWithoutColor: true
        ), "exclamationmark.triangle.fill")
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .warning,
            differentiateWithoutColor: true
        ), "exclamationmark.triangle")
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .unknown,
            differentiateWithoutColor: true
        ), "questionmark.circle")
        XCTAssertNil(BmsAlertIndicator.systemImageName(
            for: .nominal,
            differentiateWithoutColor: true
        ))
    }

    @MainActor
    func testBmsChipGlassRespectsReduceTransparency() {
        XCTAssertTrue(BmsChip.usesGlassEffect(reduceTransparency: false))
        XCTAssertFalse(BmsChip.usesGlassEffect(reduceTransparency: true))
    }
}
