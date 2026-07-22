import XCTest
@testable import CutoutApp
import CutoutMobile

final class CutoutAppModelTests: XCTestCase {
    func testCaptureQuickLabelProvidesOneStatefulActionName() {
        XCTAssertEqual(CaptureQuickLabel.ride.actionTitle(isActive: false), "Start Ride")
        XCTAssertEqual(CaptureQuickLabel.ride.actionTitle(isActive: true), "Stop Ride")
    }

    func testCaptureQuickLabelsResolveCatalogTitlesForVisibleAndAccessibleActions() {
        for label in CaptureQuickLabel.allCases {
            XCTAssertFalse(label.title.hasPrefix("capture.label."))
            XCTAssertFalse(label.actionTitle(isActive: false).hasPrefix("capture.label."))
            XCTAssertFalse(label.actionTitle(isActive: true).hasPrefix("capture.label."))
        }
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.title, "Low beam on")
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.actionTitle(isActive: false), "Start Low beam on")
        XCTAssertEqual(CaptureQuickLabel.lowBeamOn.actionTitle(isActive: true), "Stop Low beam on")
    }

    @MainActor
    func testPairFailureIsVisibleInsteadOfDoingNothing() {
        let model = CutoutAppModel()

        XCTAssertFalse(model.pair(platformIdentifier: "missing-device"))
        XCTAssertEqual(model.phase, .scanning)
        XCTAssertEqual(model.devicePickerScanState?.statusText, "Device is no longer available")
    }

    @MainActor
    func testDisconnectKeepsSavedDeviceUntilExplicitForget() {
        let store = DevicePickerSelectionStore()
        store.save(platformIdentifier: "saved-device")
        defer { store.clear() }
        let model = CutoutAppModel()

        model.disconnectTransport()

        XCTAssertEqual(store.platformIdentifier, "saved-device")

        model.forgetSavedDevice()

        XCTAssertNil(store.platformIdentifier)
    }

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

    func testCaptureStatusPreservesVisibleLifecycleText() {
        XCTAssertEqual(
            CaptureStatus.recordingLocally(fileName: "ride.cutout").displayText,
            "Recording locally: ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.recording(label: nil, notificationCount: 2, fileName: nil).displayText,
            "Recording: 2 notifications"
        )
        XCTAssertEqual(
            CaptureStatus.recording(label: "Ride", notificationCount: 3, fileName: "ride.cutout").displayText,
            "Ride: 3 notifications → ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.labelStarted(label: "Ride", notificationCount: 3, fileName: "ride.cutout").displayText,
            "Ride started: 3 notifications → ride.cutout"
        )
        XCTAssertEqual(
            CaptureStatus.labelStopped(label: "Ride", notificationCount: 4, fileName: nil).displayText,
            "Ride stopped"
        )
        XCTAssertEqual(
            CaptureStatus.saved(fileName: "ride.cutout").displayText,
            "Saved capture: ride.cutout"
        )
    }

    @MainActor
    func testCaptureStatusConsumesTypedCoreEvents() {
        let model = CutoutAppModel()
        let fileURL = URL(fileURLWithPath: "/tmp/ride.cutout")

        model.applyCaptureEvent(.started(fileURL: fileURL))
        XCTAssertEqual(model.captureStatus, .recordingLocally(fileName: "ride.cutout"))

        model.applyCaptureEvent(.notificationRecorded)
        XCTAssertEqual(
            model.captureStatus,
            .recording(label: nil, notificationCount: 1, fileName: "ride.cutout")
        )

        model.applyCaptureEvent(.finished(fileURL: fileURL))
        XCTAssertEqual(model.captureStatus, .saved(fileName: "ride.cutout"))

        model.applyCaptureEvent(.failed)
        XCTAssertEqual(model.captureStatus, .failed)
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
