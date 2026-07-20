import XCTest
@testable import CutoutApp
import CutoutMobile

final class PevScreenThemeTests: XCTestCase {
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
}
