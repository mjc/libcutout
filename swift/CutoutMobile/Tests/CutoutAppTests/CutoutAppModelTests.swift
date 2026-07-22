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
    func testSupportedPickerActionStartsTheSelectedConnection() {
        let store = DevicePickerSelectionStore()
        store.clear()
        defer { store.clear() }
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "vesc-1234",
                    title: "VESC",
                    subtitle: "VESC Onewheel",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .use),
                    symbolName: "circle.hexagongrid.circle",
                    connectionRoute: .vescOnewheel
                ),
            ]
        )
        let model = CutoutAppModel(core: driver)
        model.start()

        XCTAssertTrue(model.pair(platformIdentifier: "vesc-1234"))
        XCTAssertEqual(driver.pairedPlatformIdentifiers, ["vesc-1234"])
        XCTAssertEqual(model.phase, .discoveringServices)
        XCTAssertEqual(model.selectedRideTitle, "VESC")
        XCTAssertEqual(model.selectedConnectionRoute, .vescOnewheel)
    }

    @MainActor
    func testRejectedPickerActionReturnsToThePickerWithAnError() {
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "vesc-1234",
                    title: "VESC",
                    subtitle: "VESC Onewheel",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .use),
                    symbolName: "circle.hexagongrid.circle",
                    connectionRoute: .vescOnewheel
                ),
            ],
            pairingSucceeds: false
        )
        let model = CutoutAppModel(core: driver)
        model.start()

        XCTAssertFalse(model.pair(platformIdentifier: "vesc-1234"))
        XCTAssertEqual(driver.pairedPlatformIdentifiers, ["vesc-1234"])
        XCTAssertEqual(model.phase, .scanning)
        XCTAssertEqual(model.devicePickerScanState?.statusText, "Device is no longer available")
        let presentation = DevicePickerConnectionPresentation(
            scanState: model.devicePickerScanState,
            phase: model.phase
        )
        XCTAssertEqual(presentation.title, "Device is no longer available")
        XCTAssertFalse(presentation.showsActivity)
    }

    @MainActor
    func testNonSupportedPickerRowCannotStartTheConnection() {
        let driver = SessionDriverSpy(
            rows: [
                DevicePickerRow(
                    id: "probe-1234",
                    title: "Probe first",
                    subtitle: "Unknown device",
                    detail: "Device 1234",
                    state: DevicePickerRowState(action: .probe),
                    symbolName: "questionmark.circle"
                ),
            ]
        )
        let model = CutoutAppModel(core: driver)
        model.start()

        XCTAssertFalse(model.pair(platformIdentifier: "probe-1234"))
        XCTAssertTrue(driver.pairedPlatformIdentifiers.isEmpty)
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

@MainActor
private final class SessionDriverSpy: CutoutSessionDriving {
    var onDisplayStateChange: ((RideDisplayState) -> Void)?
    var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    var onCaptureEvent: ((CaptureEvent) -> Void)?
    var onScanStateChange: ((DevicePickerScanState) -> Void)?
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)?
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto) -> Void)?
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate?
    private let scanState: DevicePickerScanState
    private let pairingSucceeds: Bool
    private(set) var pairedPlatformIdentifiers = [String]()

    init(rows: [DevicePickerRow], pairingSucceeds: Bool = true) {
        scanState = DevicePickerScanState(status: .scanning, rows: rows)
        self.pairingSucceeds = pairingSucceeds
    }

    func start() {
        onScanStateChange?(scanState)
    }

    func pair(platformIdentifier: String) -> Bool {
        pairedPlatformIdentifiers.append(platformIdentifier)
        return pairingSucceeds
    }

    func pair(platformIdentifier: String, model _: ElectricUnicycleModel) -> Bool {
        pair(platformIdentifier: platformIdentifier)
    }

    func recordOnly(platformIdentifier _: String, note _: String?, annotations _: [String]) -> Bool { true }
    func annotateCapture(label _: String) {}
    func annotateCapture(key _: String, value _: String) {}
    func flushCapture() {}
    func disconnectAndScan() {}

    func now() -> MonotonicMilliseconds {
        MonotonicMilliseconds(0)
    }
}
