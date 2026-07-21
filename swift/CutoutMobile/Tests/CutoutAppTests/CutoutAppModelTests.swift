import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppModelTests: XCTestCase {
    func testCaptureStatusAnnouncesOnlyMeaningfulTransitions() {
        XCTAssertEqual(
            CaptureStatus.labelStarted(label: "Ride", notificationCount: 3, fileName: "ride.cutout")
                .accessibilityAnnouncement,
            "Ride capture started"
        )
        XCTAssertEqual(
            CaptureStatus.labelStopped(label: "Ride", notificationCount: 4, fileName: "ride.cutout")
                .accessibilityAnnouncement,
            "Ride capture stopped"
        )
        XCTAssertEqual(CaptureStatus.saved(fileName: "ride.cutout").accessibilityAnnouncement, "Capture saved")
        XCTAssertEqual(CaptureStatus.failed.accessibilityAnnouncement, "Capture failed")
        XCTAssertNil(
            CaptureStatus.recording(label: "Ride", notificationCount: 200, fileName: "ride.cutout")
                .accessibilityAnnouncement
        )
        XCTAssertNil(CaptureStatus.recordingLocally(fileName: "ride.cutout").accessibilityAnnouncement)
    }

    @MainActor
    func testCaptureLabelActionsIgnoreInvalidRepeatedTransitions() {
        let model = CutoutAppModel()

        model.stopCaptureLabel(.ride)
        XCTAssertNil(model.captureStatusText)
        XCTAssertTrue(model.activeCaptureLabels.isEmpty)

        model.startCaptureLabel(.ride)
        XCTAssertEqual(model.captureStatusText, "Ride started")
        XCTAssertEqual(model.activeCaptureLabels, [.ride])

        model.stopCaptureLabel(.ride)
        XCTAssertEqual(model.captureStatusText, "Ride stopped")
        XCTAssertTrue(model.activeCaptureLabels.isEmpty)

        model.stopCaptureLabel(.ride)
        XCTAssertEqual(model.captureStatusText, "Ride stopped")
    }

    @MainActor
    func testProtocolIdentityCandidateDoesNotOverwriteSelectedRideTitle() {
        let model = CutoutAppModel()
        let supported = DevicePickerCandidateSupport.supported(
            connectionRoute: .vescOnewheel,
            electricUnicycleModel: nil
        )
        let first = DevicePickerDiscoveryCandidate(
            platformIdentifier: "vesc-1",
            displayName: "Little FOCer BT",
            productCategory: "VESC Onewheel",
            evidence: "advertisement",
            detail: "Little FOCer BT",
            support: supported,
            symbolName: "circle.hexagongrid.circle"
        )
        let second = DevicePickerDiscoveryCandidate(
            platformIdentifier: "vesc-1",
            displayName: "VESC stream",
            productCategory: "VESC Onewheel",
            evidence: "advertisement",
            detail: "VESC device",
            support: supported,
            symbolName: "circle.hexagongrid.circle"
        )

        model.applyProtocolIdentityCandidate(first)
        XCTAssertEqual(model.selectedRideTitle, "Little FOCer BT")

        model.applyProtocolIdentityCandidate(second)
        XCTAssertEqual(model.selectedRideTitle, "Little FOCer BT")
    }
}
