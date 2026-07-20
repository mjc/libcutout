import XCTest
@testable import CutoutMobile

@MainActor
final class PevDashboardAccessibilityTests: XCTestCase {
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
}
