import XCTest

@MainActor
final class CutoutAppUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        try skipLiveActivityTestsOnSimulator()
        XCUIDevice.shared.orientation = isLandscapeTest ? .landscapeLeft : .portrait
        app = XCUIApplication()
        app.terminate()
        app.launchArguments = launchArguments
        app.launchEnvironment = fixture.launchEnvironment
        app.launch()
    }

    override func tearDown() async throws {
        let disconnect = app?.buttons["dashboard.disconnect"]
        if disconnect?.exists == true, disconnect?.isHittable == true {
            disconnect?.tap()
            _ = app?.descendants(matching: .any)["device-picker.screen"].waitForExistence(timeout: 5)
        }
        app?.terminate()
        app = nil
        XCUIDevice.shared.orientation = .portrait
        try await super.tearDown()
    }

    func testPickerExposesAccessibleCaptureControls() {
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        let openAdvancedCapture = app.buttons["device-picker.open-advanced-capture"]
        let advancedCapture = app.descendants(matching: .any)["device-picker.advanced-capture"]
        let captureKind = app.textFields["device-picker.capture-kind"]
        let finishEditing = app.buttons["device-picker.capture-kind.done"]
        let cancelCapture = app.buttons["device-picker.capture-kind.cancel"]
        let recordButton = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        XCTAssertTrue(openAdvancedCapture.exists)
        XCTAssertTrue(openAdvancedCapture.isHittable)
        XCTAssertFalse(captureKind.exists)
        openAdvancedCapture.tap()

        XCTAssertTrue(advancedCapture.waitForExistence(timeout: 5))
        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        XCTAssertEqual(captureKind.label, "Device kind for capture")
        scrollElementFrameIntoViewport(captureKind, in: advancedCapture, maxScrolls: 8)
        XCTAssertTrue(finishEditing.exists)
        XCTAssertEqual(finishEditing.label, "Done")
        XCTAssertTrue(cancelCapture.exists)
        XCTAssertEqual(cancelCapture.label, "Cancel")
        XCTAssertTrue(recordButton.waitForExistence(timeout: 5))
        XCTAssertGreaterThanOrEqual(recordButton.frame.height, 44)
        XCTAssertTrue(recordButton.label.contains("Unknown BLE device"))
        XCTAssertFalse(recordButton.isEnabled)

        captureKind.tap()
        captureKind.typeText("vesc floatwheel")

        XCTAssertEqual(captureKind.value as? String, "vesc floatwheel")
        XCTAssertTrue(recordButton.isEnabled)

        finishEditing.tap()

        cancelCapture.tap()
        XCTAssertFalse(advancedCapture.waitForExistence(timeout: 2))
    }

    func testProbeActionDoesNotFallThroughToRecordOnly() {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let probeButton = app.buttons["device-picker.probe.ui-test-probe"]

        XCTAssertTrue(advancedCapture.exists)
        XCTAssertTrue(captureKind.exists)
        XCTAssertTrue(probeButton.waitForExistence(timeout: 5))
        XCTAssertTrue(probeButton.isEnabled)
        XCTAssertTrue(probeButton.isHittable)
        XCTAssertTrue(probeButton.label.contains("Start probe"))
        XCTAssertTrue(probeButton.label.contains("Unknown EUC"))

        probeButton.tap()

        XCTAssertFalse(advancedCapture.waitForExistence(timeout: 2))
        XCTAssertNotNil(connectedScreen(timeout: 20))
        disconnectIfConnected()
    }

    func testProbeTimeoutRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification timed out")
    }

    func testProbeTimeoutRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification timed out")
    }

    func testProbeTimeoutRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification timed out")
    }

    func testProbeMalformedResponseRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device returned an invalid identification response")
    }

    func testProbeMalformedResponseRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device returned an invalid identification response")
    }

    func testProbeMalformedResponseRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device returned an invalid identification response")
    }

    func testProbeConflictingEvidenceRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification found conflicting evidence")
    }

    func testProbeConflictingEvidenceRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification found conflicting evidence")
    }

    func testProbeConflictingEvidenceRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification found conflicting evidence")
    }

    func testProbeUnsupportedRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device does not support this identification probe")
    }

    func testProbeUnsupportedRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device does not support this identification probe")
    }

    func testProbeUnsupportedRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device does not support this identification probe")
    }

    func testSupportedPickerRowUsesOneWholeRowAction() {
        let useButton = app.buttons["device-picker.use.ui-test-vesc"]

        XCTAssertTrue(useButton.waitForExistence(timeout: 5))
        XCTAssertEqual(useButton.label, "Use Refloat VESC, device VESC")
        XCTAssertGreaterThanOrEqual(useButton.frame.height, 92)
    }

    func testBluetoothUnavailablePickerDoesNotOfferUseOrRide() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth unavailable")
    }

    func testBluetoothUnavailablePickerDoesNotOfferUseOrRideInRightToLeftLayout() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth unavailable")
    }

    func testBluetoothUnavailablePickerDoesNotOfferUseOrRideInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth unavailable")
    }

    func testBluetoothUnavailablePickerDoesNotOfferUseOrRideInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth unavailable")
    }

    func testBluetoothUnavailableAfterLiveReturnsToAccessiblePickerAtAccessibilityDynamicType() throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        let ride = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(ride.waitForExistence(timeout: 5))

        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let status = app.descendants(matching: .any)["device-picker.connection-status"]
        XCTAssertTrue(picker.waitForExistence(timeout: 8))
        XCTAssertTrue(status.waitForExistence(timeout: 2))
        XCTAssertEqual(status.label, "Bluetooth unavailable")
        XCTAssertTrue(status.isHittable)
        XCTAssertFalse(ride.exists)
    }

    func testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRide() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth permission denied")
    }

    func testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRideInRightToLeftLayout() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth permission denied")
    }

    func testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRideInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth permission denied")
    }

    func testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRideInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth permission denied")
    }

    private func assertBluetoothBlockedPicker(status expectedStatus: String) throws {
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let status = app.descendants(matching: .any)["device-picker.connection-status"]

        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertEqual(status.label, expectedStatus)
        XCTAssertTrue(status.isHittable, "The blocking Bluetooth status must be visible without scrolling")
        XCTAssertFalse(app.buttons["device-picker.use.ui-test-vesc"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.screen.vescRide"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.screen.eucRide"].exists)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testEucFixtureSelectionIgnoresXCTestSelectorCase() {
        XCTAssertEqual(Fixture.testFixture(for: "testEUCBmsDetailPassesAccessibilityAudit"), .euc)
        XCTAssertEqual(Fixture.testFixture(for: "testEucBmsOverviewPassesAccessibilityAudit"), .eucOverview)
        XCTAssertEqual(Fixture.testFixture(for: "testEucStaleTelemetryIsAnAccessibleWarning"), .eucStale)
        XCTAssertEqual(Fixture.testFixture(for: "testEUCNoBmsSurfacePassesAccessibilityAudit"), .eucNoBms)
        XCTAssertEqual(Fixture.testFixture(for: "testEUCReconnectKeepsRideRoute"), .eucReconnect)
    }

    func testCaptureAnnotationUsesOneStatefulAccessibleAction() {
        enterCapture()

        let rideActions = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "capture.label.ride.")
        )
        let action = app.buttons["capture.label.ride.action"]
        XCTAssertEqual(rideActions.count, 1)
        XCTAssertTrue(action.waitForExistence(timeout: 5))
        XCTAssertEqual(action.label, "Start Ride")

        action.tap()
        XCTAssertEqual(action.label, "Stop Ride")

        action.tap()
        XCTAssertEqual(action.label, "Start Ride")
    }

    func testCaptureExposesTypedWriterHealthDetails() {
        enterCapture()

        for rowID in [
            "capture-elapsed",
            "capture-packets",
            "capture-file-size",
            "capture-queued-messages",
            "capture-writer-health",
        ] {
            let row = app.descendants(matching: .any)["dashboard.key-value.\(rowID)"]
            XCTAssertTrue(row.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertFalse(row.label.isEmpty)
            XCTAssertFalse((row.value as? String)?.isEmpty ?? true)
        }

        let writer = app.descendants(matching: .any)["dashboard.key-value.capture-writer-health"]
        let pendingWrites = app.descendants(matching: .any)["dashboard.key-value.capture-queued-messages"]

        XCTAssertEqual(pendingWrites.value as? String, "0")
        XCTAssertEqual(writer.value as? String, "Healthy")
    }

    func testFinishCaptureReturnsToPickerAfterFinalizing() throws {
        _ = try finishCaptureAndReturnToPicker()
    }

    func testFinishCaptureReturnsToAccessiblePickerInLightAppearanceAtAccessibilityDynamicType() throws {
        _ = try finishCaptureAndReturnToPicker()
        try performVisibleLayoutAccessibilityAudit()
    }

    func testFinishCaptureReturnsToAccessiblePickerInDarkAppearanceAtAccessibilityDynamicType() throws {
        _ = try finishCaptureAndReturnToPicker()
        try performVisibleLayoutAccessibilityAudit()
    }

    func testFinishCaptureReturnsToAccessiblePickerWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        _ = try finishCaptureAndReturnToPicker(usesLocalizedText: true)
        try performVisibleLayoutAccessibilityAudit()
    }

    private func finishCaptureAndReturnToPicker(
        usesLocalizedText: Bool = false
    ) throws -> XCUIElement {
        enterCapture()

        let finish = app.buttons["capture.stop"]
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let savedCapture = app.descendants(matching: .any)["device-picker.capture-status"]
        XCTAssertTrue(finish.waitForExistence(timeout: 5))
        XCTAssertEqual(finish.elementType, .button)
        XCTAssertGreaterThanOrEqual(finish.frame.height, 44)
        finish.tap()

        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertTrue(picker.isHittable)
        XCTAssertTrue(savedCapture.waitForExistence(timeout: 5))
        XCTAssertTrue(savedCapture.isHittable, "The saved-capture result must be visible without scrolling")
        XCTAssertFalse(savedCapture.label.isEmpty)
        XCTAssertTrue(savedCapture.label.contains("cutout-btle-capture-"))
        XCTAssertTrue(savedCapture.label.contains(".jsonl"))
        if usesLocalizedText {
            let filenameStart = try XCTUnwrap(savedCapture.label.range(of: "cutout-btle-capture-"))
            let filename = String(savedCapture.label[filenameStart.lowerBound...])
            XCTAssertNotEqual(savedCapture.label, "Saved capture: \(filename)")
        } else {
            XCTAssertTrue(savedCapture.label.hasPrefix("Saved capture:"))
        }
        XCTAssertFalse(app.descendants(matching: .any)["capture.screen"].isHittable)
        return picker
    }

    func testFinishCaptureFailureKeepsCaptureScreenVisible() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible(
            auditExclusions: .dynamicType
        )
    }

    func testBackgroundFlushFailureRemainsVisibleAfterReactivatingCaptureAtAccessibilityDynamicType() throws {
        enterCapture()

        let capture = app.descendants(matching: .any)["capture.screen"]
        let status = app.descendants(matching: .any)["capture.status"]
        let finish = app.buttons["capture.stop"]
        XCTAssertTrue(capture.waitForExistence(timeout: 5))
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertNotEqual(status.label, "Capture failed")

        XCUIDevice.shared.press(.home)
        app.activate()

        XCTAssertTrue(capture.waitForExistence(timeout: 5))
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        let failure = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", "Capture failed"),
            object: status
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failure], timeout: 5), .completed)
        XCTAssertEqual(status.label, "Capture failed")
        XCTAssertTrue(status.isHittable, "Background flush failure must remain visible after reactivation")
        XCTAssertTrue(finish.isHittable, "Finish capture must remain usable after a background flush failure")
        try performVisibleLayoutAccessibilityAudit()
    }

    func testBackgroundFlushRealWriterRemainsUsableAfterReactivatingCaptureAtAccessibilityDynamicType() throws {
        enterCapture()

        let capture = app.descendants(matching: .any)["capture.screen"]
        let status = app.descendants(matching: .any)["capture.status"]
        let fileSize = app.descendants(matching: .any)["dashboard.key-value.capture-file-size"]
        let finish = app.buttons["capture.stop"]
        XCTAssertTrue(capture.waitForExistence(timeout: 5))
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(fileSize.waitForExistence(timeout: 5))
        XCTAssertNotEqual(fileSize.value as? String, "0 B")

        XCUIDevice.shared.press(.home)
        app.activate()

        XCTAssertTrue(capture.waitForExistence(timeout: 5))
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertNotEqual(status.label, "Capture failed")
        XCTAssertTrue(status.isHittable)
        XCTAssertTrue(finish.isHittable)
        XCTAssertTrue(app.frame.contains(finish.frame))
        XCTAssertGreaterThanOrEqual(finish.frame.width, capture.frame.width - 36)
        XCTAssertLessThanOrEqual(status.frame.maxY, finish.frame.minY)
        try performVisibleLayoutAccessibilityAudit(ignoringNilElementContrastWarning: true)
        finish.tap()

        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let savedCapture = app.descendants(matching: .any)["device-picker.capture-status"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertTrue(savedCapture.waitForExistence(timeout: 5))
        XCTAssertTrue(savedCapture.label.contains("cutout-btle-capture-"))
        XCTAssertTrue(savedCapture.label.contains(".jsonl"))
    }

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleAtAccessibilityDynamicType() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible()
    }

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible()
    }

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible()
    }

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible(usesLocalizedText: true)
    }

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible(usesLocalizedText: true)
    }

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleInRightToLeftLayout() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible()
    }

    private func assertFinishCaptureFailureKeepsCaptureScreenAccessible(
        usesLocalizedText: Bool = false,
        auditExclusions: XCUIAccessibilityAuditType = []
    ) throws {
        enterCapture()

        let finish = app.buttons["capture.stop"]
        let capture = app.descendants(matching: .any)["capture.screen"]
        let status = app.descendants(matching: .any)["capture.status"]

        XCTAssertTrue(finish.waitForExistence(timeout: 5))
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        let initialStatus = status.label
        finish.tap()

        let failure = XCTNSPredicateExpectation(
            predicate: usesLocalizedText
                ? NSPredicate(format: "label != %@ AND label != %@", initialStatus, "")
                : NSPredicate(format: "label == %@", "Capture failed"),
            object: status
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failure], timeout: 5), .completed)
        if usesLocalizedText {
            XCTAssertNotEqual(status.label, "Capture failed")
        }
        XCTAssertTrue(capture.waitForExistence(timeout: 5))
        XCTAssertTrue(capture.isHittable)
        XCTAssertTrue(status.isHittable, "Capture failure must be visible without scrolling")
        XCTAssertTrue(finish.isHittable, "Finish capture must remain usable after a failure")
        if !usesLocalizedText {
            XCTAssertEqual(status.label, "Capture failed")
        }
        try performVisibleLayoutAccessibilityAudit(excluding: auditExclusions)
    }

    func testCaptureExclusivePedalModeLeavesOneActiveAccessibleAction() {
        enterCapture()

        let screen = app.descendants(matching: .any)["capture.screen"]
        let hardPedals = app.buttons["capture.label.pedals_hard.action"]
        let softPedals = app.buttons["capture.label.pedals_soft.action"]

        for _ in 0..<6 where !hardPedals.isHittable {
            screen.swipeUp()
        }
        XCTAssertTrue(hardPedals.isHittable)
        XCTAssertTrue(softPedals.isHittable)

        hardPedals.tap()
        XCTAssertEqual(hardPedals.label, "Stop Pedals hard")

        softPedals.tap()
        XCTAssertEqual(hardPedals.label, "Start Pedals hard")
        XCTAssertEqual(softPedals.label, "Stop Pedals soft")
    }

    func testDisconnectKeepsSavedDeviceUntilExplicitForget() throws {
        let forget = try disconnectAndRequireSavedDevice()
        forget.tap()
        XCTAssertTrue(forget.waitForNonExistence(timeout: 5))
    }

    func testDisconnectKeepsSavedDeviceAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        let forget = try disconnectAndRequireSavedDevice()
        try performVisibleLayoutAccessibilityAudit()
        forget.tap()
        XCTAssertTrue(forget.waitForNonExistence(timeout: 5))
    }

    private func disconnectAndRequireSavedDevice() throws -> XCUIElement {
        XCTAssertTrue(pairAvailableDevice(.vesc))

        let disconnect = app.buttons["dashboard.disconnect"]
        XCTAssertTrue(disconnect.waitForExistence(timeout: 5))
        XCTAssertEqual(disconnect.elementType, .button)
        XCTAssertTrue(disconnect.isHittable)
        if name.contains("Pseudolocalized") {
            XCTAssertFalse(disconnect.label.isEmpty)
            XCTAssertNotEqual(disconnect.label, "Disconnect")
        } else {
            XCTAssertEqual(disconnect.label, "Disconnect")
        }
        disconnect.tap()

        let picker = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(
            picker.waitForExistence(timeout: 5),
            "Disconnect did not return to the picker:\n\(app.debugDescription)"
        )
        let forget = app.buttons["device-picker.forget-saved-device"]
        XCTAssertTrue(forget.waitForExistence(timeout: 5))
        if name.contains("Pseudolocalized") {
            XCTAssertFalse(forget.label.isEmpty)
            XCTAssertNotEqual(forget.label, "Forget saved device")
        } else {
            XCTAssertEqual(forget.label, "Forget saved device")
        }
        XCTAssertTrue(forget.isHittable)

        let dashboard = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "dashboard.screen.")
        ).firstMatch
        let reconnect = expectation(
            for: NSPredicate(format: "exists == 1"),
            evaluatedWith: dashboard
        )
        reconnect.isInverted = true
        wait(for: [reconnect], timeout: 2)

        return forget
    }

    func testVescUseDisconnectCycleKeepsOneNativeRideRoute() {
        assertUseDisconnectCycles(for: .vesc)
    }

    func testEucUseDisconnectCycleKeepsOneNativeRideRoute() {
        assertUseDisconnectCycles(for: .euc)
    }

    func testVescUseShowsConnectingBeforeRide() throws {
        try assertUseShowsConnectingBeforeRide(for: .vesc)
    }

    func testEucUseShowsConnectingBeforeRide() throws {
        try assertUseShowsConnectingBeforeRide(for: .euc)
    }

    func testVescUseShowsConnectingBeforeRideInRightToLeftLayout() throws {
        try assertUseShowsConnectingBeforeRide(for: .vesc)
    }

    func testEucUseShowsConnectingBeforeRideInRightToLeftLayout() throws {
        try assertUseShowsConnectingBeforeRide(for: .euc)
    }

    private func assertUseShowsConnectingBeforeRide(for family: ConnectedDeviceFamily) throws {
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let connectionStatus = app.descendants(matching: .any)["device-picker.connection-status"]

        XCTAssertTrue(pairAvailableDevice(family))

        let connecting = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label BEGINSWITH %@", "Connecting"),
            object: connectionStatus
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [connecting], timeout: 3),
            .completed,
            "Use for \(family.name) did not expose the Connecting state"
        )

        let ride = app.descendants(matching: .any)[family.screenIdentifier]
        XCTAssertFalse(ride.exists, "\(family.name) opened Ride before showing Connecting")
        try performVisibleLayoutAccessibilityAudit()
        assertFirstRideRouteMatches(family, timeout: 20)

        app.buttons["dashboard.disconnect"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
    }

    private func assertUseDisconnectCycles(for family: ConnectedDeviceFamily) {
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let ride = app.descendants(matching: .any)[family.screenIdentifier]
        let disconnect = app.buttons["dashboard.disconnect"]

        for cycle in 1...3 {
            XCTAssertTrue(pairAvailableDevice(family), "Cycle \(cycle) did not start from the native \(family.name) Use button")
            assertFirstRideRouteMatches(family, timeout: 20)
            XCTAssertTrue(ride.exists, "Cycle \(cycle) did not open the \(family.name) Ride screen")
            XCTAssertTrue(disconnect.waitForExistence(timeout: 5))
            XCTAssertEqual(disconnect.elementType, .button)

            disconnect.tap()
            XCTAssertTrue(picker.waitForExistence(timeout: 5), "Cycle \(cycle) did not return to the picker")
            XCTAssertFalse(ride.exists, "Cycle \(cycle) left the previous Ride screen visible")
        }
    }

    private func assertFirstRideRouteMatches(
        _ family: ConnectedDeviceFamily,
        timeout: TimeInterval
    ) {
        let firstRide = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "dashboard.screen.")
        ).firstMatch
        XCTAssertTrue(
            firstRide.waitForExistence(timeout: timeout),
            "Use for \(family.name) did not open a Ride screen"
        )
        XCTAssertEqual(
            firstRide.identifier,
            family.screenIdentifier,
            "Use for \(family.name) opened the wrong Ride screen first"
        )
    }

    func testCapturePassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility()
    }

    func testCapturePassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility()
    }

    func testCapturePassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility(ignoringNilElementContrastWarning: true)
    }

    func testCapturePassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility()

        let stopCapture = app.buttons["capture.stop"]
        XCTAssertFalse(stopCapture.label.isEmpty)
        XCTAssertNotEqual(
            stopCapture.label,
            "Finish capture",
            "The pseudolocalized launch did not expand catalog-backed Capture copy"
        )
    }

    func testCapturePassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertCaptureAccessibility()
    }

    func testCapturePassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility()
    }

    func testCapturePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility(exercisesLabels: false)
    }

    func testProductionPickerPassesAccessibilityAudit() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditInLightAppearance() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertProductionPickerAccessibility(assertsPseudolocalizedCopy: true)
    }

    func testProductionPickerPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertProductionPickerAccessibility(assertsPseudolocalizedCopy: true)
    }

    func testProductionSurfacesPassAccessibilityAudit() throws {
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        try performVisibleLayoutAccessibilityAudit()

        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.exists)
        XCTAssertFalse((speed.value as? String)?.isEmpty ?? true)
        try performVisibleLayoutAccessibilityAudit(
            // This default-size route owns semantics and clipping. The
            // Accessibility-XXXL VESC route owns rendered Dynamic Type.
            excluding: .dynamicType
        )
    }

    func testVescRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType() throws {
        try assertRidePublishesDynamicTelemetryAfterRouteMounts(.vesc)
    }

    func testEucRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType() throws {
        try assertRidePublishesDynamicTelemetryAfterRouteMounts(.euc)
    }

    private func assertRidePublishesDynamicTelemetryAfterRouteMounts(_ family: ConnectedDeviceFamily) throws {
        XCTAssertTrue(pairAvailableDevice(family))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The dynamic \(family.name) fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        let initialValue = try XCTUnwrap(speed.value as? String)
        let waitStarted = ContinuousClock.now
        let changed = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else { return false }
                return element.exists && value != initialValue
            },
            object: speed
        )

        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 3), .completed)
        let latency = waitStarted.duration(to: .now)
        XCTAssertLessThanOrEqual(latency, .seconds(3))
        XCTContext.runActivity(named: "Mounted Ride telemetry latency: \(latency)") { _ in }
        XCTAssertFalse(app.descendants(matching: .any)["device-picker.screen"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["device-picker.capture"].exists)
    }

    func testPickerSurfaceRemainsReachableAtAccessibilityDynamicType() throws {
        let window = app.windows.firstMatch
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        let openAdvancedCapture = app.buttons["device-picker.open-advanced-capture"]

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        XCTAssertTrue(openAdvancedCapture.exists)
        XCTAssertFalse(window.frame.isEmpty)
        XCTAssertFalse(screen.frame.isEmpty)
        XCTAssertGreaterThanOrEqual(screen.frame.minY, window.frame.minY - 2)
        XCTAssertLessThanOrEqual(screen.frame.maxY, window.frame.maxY + 2)

        for _ in 0..<4 where !openAdvancedCapture.isHittable {
            screen.swipeUp()
        }

        XCTAssertTrue(openAdvancedCapture.isHittable)
        restorePickerViewport(screen)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testAdvancedCaptureControlsRemainReachableAtAccessibilityDynamicType() throws {
        try assertAdvancedCaptureControlsReachableAndAccessible()
    }

    func testAdvancedCaptureControlsRemainReachableInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertAdvancedCaptureControlsReachableAndAccessible()
    }

    func testAdvancedCaptureControlsRemainReachableInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertAdvancedCaptureControlsReachableAndAccessible(
            ignoringAdvancedCaptureTitleContrastWarning: true
        )
    }

    func testAdvancedCaptureKeyboardWorkflowRemainsReachableAtAccessibilityDynamicType() throws {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let finishEditing = app.buttons["device-picker.capture-kind.done"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        captureKind.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))

        captureKind.typeText("vesc floatwheel")
        XCTAssertTrue(recordButton.isEnabled)
        XCTAssertTrue(finishEditing.isHittable)
        finishEditing.tap()
        XCTAssertFalse(app.keyboards.firstMatch.waitForExistence(timeout: 2))

        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)
    }

    func testAdvancedCaptureCancelReturnsToPickerAtAccessibilityDynamicType() {
        let advancedCapture = openAdvancedCapture()
        let cancelCapture = app.buttons["device-picker.capture-kind.cancel"]
        let picker = app.descendants(matching: .any)["device-picker.screen"]

        XCTAssertTrue(cancelCapture.waitForExistence(timeout: 5))
        XCTAssertTrue(cancelCapture.isHittable)
        cancelCapture.tap()

        XCTAssertFalse(advancedCapture.waitForExistence(timeout: 2))
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
    }

    func testAdvancedCaptureControlsRemainReachableInRightToLeftLayout() throws {
        try assertAdvancedCaptureControlsReachableAndAccessible(exercisesKeyboard: true)
    }

    func testAdvancedCaptureControlsRemainReachableInLandscapeAtAccessibilityDynamicType() throws {
        try assertAdvancedCaptureControlsReachableAndAccessible(exercisesKeyboard: true)
    }

    private func assertAdvancedCaptureControlsReachableAndAccessible(
        exercisesKeyboard: Bool = false,
        ignoringAdvancedCaptureTitleContrastWarning: Bool = false
    ) throws {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        if exercisesKeyboard {
            captureKind.tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
            app.buttons["device-picker.capture-kind.done"].tap()
        } else {
            for _ in 0..<6 where !captureKind.isHittable {
                advancedCapture.swipeUp()
            }
        }
        XCTAssertTrue(captureKind.isHittable)

        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)
        restoreAdvancedCaptureViewport(advancedCapture, captureKind: captureKind)
        try performVisibleLayoutAccessibilityAudit(
            ignoringSystemToolbarDynamicTypeWarning: true,
            ignoringAdvancedCaptureTitleContrastWarning: ignoringAdvancedCaptureTitleContrastWarning
        )
    }

    func testAdvancedCapturePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        for _ in 0..<6 where !captureKind.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(captureKind.isHittable)

        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)

        restoreAdvancedCaptureViewport(advancedCapture, captureKind: captureKind)
        try performVisibleLayoutAccessibilityAudit(
            ignoringSystemToolbarDynamicTypeWarning: true
        )
    }

    func testVescUseOpensAnAccessibleLiveRide() throws {
        try assertConnectedSurface(for: .vesc)
    }

    func testVescEssentialRideControlsRemainVisibleWithoutScrolling() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .vesc)
    }

    func testVescEssentialRideControlsRemainVisibleWithoutScrollingAtAccessibilityDynamicType() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .vesc)
    }

    func testVescEssentialRideControlsRemainVisibleWithoutScrollingInLandscape() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .vesc)
    }

    func testVescEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtExtraExtraExtraLargeType() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .vesc)
    }

    func testVescEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtAccessibilityDynamicType() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .vesc)
    }

    func testEucEssentialRideControlsRemainVisibleWithoutScrolling() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .euc)
    }

    func testEucEssentialRideControlsRemainVisibleWithoutScrollingAtAccessibilityDynamicType() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .euc)
    }

    func testEucEssentialRideControlsRemainVisibleWithoutScrollingInLandscape() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .euc)
    }

    func testEucEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtExtraExtraExtraLargeType() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .euc)
    }

    func testEucEssentialRideControlsRemainVisibleWithoutScrollingInLandscapeAtAccessibilityDynamicType() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .euc)
    }

    private func assertEssentialRideControlsRemainVisibleWithoutScrolling(
        for family: ConnectedDeviceFamily
    ) throws {
        XCTAssertTrue(pairAvailableDevice(family))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic \(family.name) fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let windowFrame = app.windows.firstMatch.frame
        for identifier in ["ride.hero.speed", "ride.hero.status", "dashboard.disconnect"] {
            let element = app.descendants(matching: .any)[identifier]
            XCTAssertTrue(element.waitForExistence(timeout: 5), "Missing essential Ride control: \(identifier)")
            XCTAssertTrue(element.isHittable, "Essential Ride control requires scrolling: \(identifier)")
            XCTAssertTrue(
                windowFrame.insetBy(dx: -2, dy: -2).contains(element.frame),
                "Essential Ride control is clipped by the viewport: \(identifier) \(element.frame)"
            )
            if identifier == "dashboard.disconnect" {
                XCTAssertGreaterThanOrEqual(element.frame.height, 44)
            }
        }
        XCTAssertTrue(screen.exists)
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.ride"].isHittable)
        let secondaryRoute = switch family {
        case .vesc: "dashboard.nav.debug"
        case .euc: "dashboard.nav.pack"
        }
        for identifier in ["dashboard.nav.ride", secondaryRoute] {
            let tab = app.tabBars.buttons[identifier]
            XCTAssertTrue(tab.isHittable)
            XCTAssertTrue(
                windowFrame.insetBy(dx: -2, dy: -2).contains(tab.frame),
                "Ride tab is clipped by the viewport: \(identifier) \(tab.frame)"
            )
        }
    }

    func testVescLiveActivityFixtureStartsFromAnAccessibleRide() throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20))
        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertFalse((speed.value as? String)?.isEmpty ?? true)
        // XCTest disconnects before termination so it cannot prove persistence.
        // Use scripts/run-ios-app-on-phone.sh for physical ActivityKit inspection.
    }

    func testVescLiveActivityAutoFixtureStartsAnAccessibleRide() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescLiveActivityContinuesUpdatingWhileBackgrounded() {
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20), app.debugDescription)
        let foregroundSpeed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(foregroundSpeed.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(
            (foregroundSpeed.value as? String)?.contains("17.9") == true,
            foregroundSpeed.debugDescription
        )
        defer {
            app.activate()
            disconnectIfConnected()
        }

        XCUIDevice.shared.press(.home)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        dismissLocationPromptIfNeeded(in: springboard)
        XCUIDevice.shared.press(.home)
        springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.01))
            .press(
                forDuration: 0.1,
                thenDragTo: springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.8))
            )

        let speed = springboard.descendants(matching: .any)["Speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5), springboard.debugDescription)
        let updated = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS %@", "35.8"),
            object: speed
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [updated], timeout: 12),
            .completed,
            speed.debugDescription
        )
    }

    func testVescCriticalLiveActivityAutoFixtureStartsAnAccessibleRide() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescLiveActivityAutoFixtureStartsAnAccessibleRideInLandscape() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescCriticalLiveActivityAutoFixtureStartsAnAccessibleRideInLandscape() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescUnavailableLiveActivityAutoFixturePreservesUnavailableSemantics() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescUnavailableLiveActivityAutoFixturePreservesUnavailableSemanticsInLandscape() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescStaleLiveActivityAutoFixturePreservesStaleSemantics() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescStaleLiveActivityAutoFixturePreservesStaleSemanticsInLandscape() throws {
        try assertVescLiveActivityAutoFixture()
    }

    func testVescCriticalLiveActivityLockScreenPreservesSafetySemantics() {
        assertVescLiveActivityLockScreen(
            speed: "17.9",
            headroom: "reduce acceleration",
            stateName: "Critical"
        )
    }

    func testVescCriticalLiveActivityLockScreenPreservesSafetySemanticsInLightAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(
            speed: "17.9",
            headroom: "reduce acceleration",
            stateName: "Critical"
        )
    }

    func testVescCriticalLiveActivityLockScreenPreservesSafetySemanticsInDarkAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(
            speed: "17.9",
            headroom: "reduce acceleration",
            stateName: "Critical"
        )
    }

    func testVescLiveActivityLockScreenPreservesNominalSemantics() {
        assertVescLiveActivityLockScreen(speed: "17.9", headroom: "good", stateName: "Nominal")
    }

    func testVescLiveActivityLockScreenPreservesNominalSemanticsInLightAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(speed: "17.9", headroom: "good", stateName: "Nominal")
    }

    func testVescLiveActivityLockScreenPreservesNominalSemanticsInDarkAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(speed: "17.9", headroom: "good", stateName: "Nominal")
    }

    func testVescUnavailableLiveActivityLockScreenPreservesUnavailableSemantics() {
        assertVescLiveActivityLockScreen(
            speed: "unavailable",
            headroom: "unavailable",
            stateName: "Unavailable"
        )
    }

    func testVescUnavailableLiveActivityLockScreenPreservesUnavailableSemanticsInLightAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(
            speed: "unavailable",
            headroom: "unavailable",
            stateName: "Unavailable"
        )
    }

    func testVescUnavailableLiveActivityLockScreenPreservesUnavailableSemanticsInDarkAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(
            speed: "unavailable",
            headroom: "unavailable",
            stateName: "Unavailable"
        )
    }

    func testVescStaleLiveActivityLockScreenPreservesStaleSemantics() {
        assertVescLiveActivityLockScreen(speed: "stale", headroom: "good", stateName: "Stale")
    }

    func testVescStaleLiveActivityLockScreenPreservesStaleSemanticsInLightAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(speed: "stale", headroom: "good", stateName: "Stale")
    }

    func testVescStaleLiveActivityLockScreenPreservesStaleSemanticsInDarkAppearanceAtAccessibilityDynamicType() {
        assertVescLiveActivityLockScreen(speed: "stale", headroom: "good", stateName: "Stale")
    }

    private func assertVescLiveActivityLockScreen(
        speed expectedSpeed: String,
        headroom expectedHeadroom: String,
        stateName: String
    ) {
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20), app.debugDescription)
        defer {
            app.activate()
            disconnectIfConnected()
        }

        XCUIDevice.shared.press(.home)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        dismissLocationPromptIfNeeded(in: springboard)
        XCUIDevice.shared.press(.home)
        springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.01))
            .press(
                forDuration: 0.1,
                thenDragTo: springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.8))
            )

        let allowLiveActivities = springboard.buttons["Allow"]
        if allowLiveActivities.waitForExistence(timeout: 1) {
            allowLiveActivities.tap()
        }
        XCTAssertFalse(
            allowLiveActivities.waitForExistence(timeout: 2),
            "The system Live Activity permission prompt still obscures rendered Lock Screen evidence"
        )
        let alwaysAllowLiveActivities = springboard.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] 'Always'")
        ).firstMatch
        if alwaysAllowLiveActivities.waitForExistence(timeout: 1) {
            alwaysAllowLiveActivities.tap()
        }
        XCTAssertFalse(
            alwaysAllowLiveActivities.waitForExistence(timeout: 2),
            "The continuing Live Activity permission prompt still obscures rendered Lock Screen evidence"
        )

        let activity = springboard.descendants(matching: .any)["CutOut ride"]
        XCTAssertTrue(activity.waitForExistence(timeout: 5), springboard.debugDescription)
        let speed = springboard.descendants(matching: .any)["Speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5), springboard.debugDescription)
        XCTAssertTrue(
            (speed.value as? String)?.localizedCaseInsensitiveContains(expectedSpeed) == true,
            speed.debugDescription
        )
        let headroom = springboard.descendants(matching: .any)["Headroom"]
        XCTAssertTrue(headroom.waitForExistence(timeout: 5), springboard.debugDescription)
        XCTAssertTrue(
            (headroom.value as? String)?.localizedCaseInsensitiveContains(expectedHeadroom) == true,
            headroom.debugDescription
        )
        attachScreenshot(of: springboard, named: "\(stateName) Lock Screen Live Activity")
    }

    private func assertVescLiveActivityAutoFixture() throws {
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20), app.debugDescription)
        defer {
            app.activate()
            disconnectIfConnected()
        }
        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertFalse((speed.value as? String)?.isEmpty ?? true)

        XCUIDevice.shared.press(.home)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        dismissLocationPromptIfNeeded(in: springboard)
        XCUIDevice.shared.press(.home)
        let expectation = if name.contains("Critical") {
            (
                stateName: "Critical",
                speed: "17.9",
                headroom: "reduce acceleration",
                connectionStates: ["connected", "stale"]
            )
        } else if name.contains("Unavailable") {
            (
                stateName: "Unavailable",
                speed: "unavailable",
                headroom: "unavailable",
                connectionStates: ["waiting for telemetry"]
            )
        } else if name.contains("Stale") {
            (stateName: "Stale", speed: "stale", headroom: "good", connectionStates: ["stale"])
        } else {
            (
                stateName: "Nominal",
                speed: "17.9",
                headroom: "good",
                connectionStates: ["connected", "stale"]
            )
        }
        let stateName = expectation.stateName
        attachScreenshot(of: springboard, named: "\(stateName) Compact Dynamic Island")
        springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.04))
            .press(forDuration: 1)
        let islandSpeed = springboard.descendants(matching: .any)["Speed"]
        XCTAssertTrue(islandSpeed.waitForExistence(timeout: 5), springboard.debugDescription)
        XCTAssertTrue(
            (islandSpeed.value as? String)?.localizedCaseInsensitiveContains(expectation.speed) == true,
            islandSpeed.debugDescription
        )
        let islandDevice = springboard.descendants(matching: .any)["Device"]
        XCTAssertTrue(islandDevice.waitForExistence(timeout: 5), springboard.debugDescription)
        XCTAssertTrue(
            (islandDevice.value as? String)?.localizedCaseInsensitiveContains("Refloat VESC") == true,
            islandDevice.debugDescription
        )
        let deviceValue = islandDevice.value as? String
        XCTAssertTrue(
            expectation.connectionStates.contains {
                deviceValue?.localizedCaseInsensitiveContains($0) == true
            },
            islandDevice.debugDescription
        )
        let islandHeadroom = springboard.descendants(matching: .any)["Headroom"]
        XCTAssertTrue(islandHeadroom.waitForExistence(timeout: 5), springboard.debugDescription)
        XCTAssertTrue(
            (islandHeadroom.value as? String)?.localizedCaseInsensitiveContains(expectation.headroom) == true,
            islandHeadroom.debugDescription
        )
        let expandedActivity = springboard.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS 'CutOut' AND label CONTAINS 'Speed' AND label CONTAINS 'Headroom'")
        ).firstMatch
        XCTAssertTrue(expandedActivity.exists, springboard.debugDescription)
        let orderedSafetyValues = expandedActivity.descendants(matching: .any).matching(
            NSPredicate(format: "label == 'Speed' OR label == 'Headroom'")
        )
        XCTAssertEqual(orderedSafetyValues.count, 2)
        XCTAssertEqual(orderedSafetyValues.element(boundBy: 0).label, stateName == "Critical" ? "Headroom" : "Speed")
        attachScreenshot(of: springboard, named: "\(stateName) Expanded Dynamic Island")
    }

    private func dismissLocationPromptIfNeeded(in springboard: XCUIApplication) {
        let locationPrompt = springboard.alerts.firstMatch
        if locationPrompt.waitForExistence(timeout: 1) {
            locationPrompt.buttons["Don’t Allow"].tap()
        }
    }

    private func attachScreenshot(of application: XCUIApplication, named name: String) {
        let attachment = XCTAttachment(screenshot: application.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testFailedVescConnectionReturnsToPickerInsteadOfLeavingRideRoute() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testFailedVescConnectionPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testFailedVescConnectionPassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testFailedVescConnectionPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testFailedVescConnectionPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility(
            usesLocalizedText: true
        )
    }

    func testFailedVescConnectionPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility(
            usesLocalizedText: true
        )
    }

    func testFailedVescConnectionPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testVescReconnectKeepsRideAccessible() throws {
        try assertReconnectAccessibility(for: .vesc)
    }

    func testVescReconnectKeepsRideAccessibleAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .vesc)
    }

    func testVescReconnectKeepsRideAccessibleInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .vesc)
    }

    func testVescReconnectKeepsRideAccessibleInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .vesc)
    }

    func testVescReconnectKeepsRideAccessibleWithIncreasedContrastAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .vesc, auditExclusions: [])
    }

    func testVescReconnectKeepsRideAccessibleInLandscapeAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .vesc)
    }

    func testVescReconnectKeepsRideAccessibleWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .vesc, usesLocalizedText: true)
    }

    func testVescReconnectKeepsRideAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(
            for: .vesc,
            usesLocalizedText: true,
            auditExclusions: []
        )
    }

    func testVescReconnectKeepsRideAccessibleInRightToLeftLayout() throws {
        try assertReconnectAccessibility(
            for: .vesc,
            ignoringNilElementContrastWarning: true,
            auditScrolls: 1
        )
    }

    func testEucReconnectKeepsRideRoute() throws {
        try assertReconnectAccessibility(for: .euc)
    }

    func testEucReconnectKeepsRideAccessibleInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .euc)
    }

    func testEucReconnectKeepsRideAccessibleInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .euc)
    }

    func testEucReconnectKeepsRideAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(
            for: .euc,
            usesLocalizedText: true,
            auditExclusions: []
        )
    }

    func testEucReconnectKeepsRideAccessibleInRightToLeftLayout() throws {
        try assertReconnectAccessibility(
            for: .euc,
            ignoringNilElementContrastWarning: true,
            auditScrolls: 2
        )
    }

    func testVescReconnectPrioritizesWarningForAccessibility() {
        assertReconnectWarningPrecedesSpeed(for: .vesc)
    }

    func testEucReconnectPrioritizesWarningForAccessibility() {
        assertReconnectWarningPrecedesSpeed(for: .euc)
    }

    private func assertReconnectWarningPrecedesSpeed(for family: ConnectedDeviceFamily) {
        XCTAssertTrue(pairAvailableDevice(family))
        guard let rideScreen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic \(family.name) fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let status = app.descendants(matching: .any)["ride.hero.status"]
        let retrying = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "Retrying connection"),
            object: status
        )
        XCTAssertEqual(XCTWaiter.wait(for: [retrying], timeout: 5), .completed)

        let orderedTransitionValues = rideScreen.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier == 'ride.hero.status' OR identifier == 'ride.hero.speed'"
            )
        )
        XCTAssertEqual(orderedTransitionValues.count, 2, rideScreen.debugDescription)
        XCTAssertEqual(
            orderedTransitionValues.element(boundBy: 0).identifier,
            "ride.hero.status"
        )
    }

    private func assertReconnectAccessibility(
        for family: ConnectedDeviceFamily,
        usesLocalizedText: Bool = false,
        auditExclusions: XCUIAccessibilityAuditType = [],
        ignoringNilElementContrastWarning: Bool = false,
        auditScrolls: Int = 0
    ) throws {
        XCTAssertTrue(pairAvailableDevice(family))
        guard let rideScreen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic \(family.name) fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        if usesLocalizedText {
            let status = app.descendants(matching: .any)["ride.hero.status"]
            XCTAssertTrue(status.waitForExistence(timeout: 5))
            let liveStatus = status.label
            let retrying = XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "label != %@ AND label != %@", liveStatus, ""),
                object: status
            )
            XCTAssertEqual(XCTWaiter.wait(for: [retrying], timeout: 5), .completed)
            XCTAssertTrue(status.isHittable, "Retrying status must be visible when the transition occurs")
            XCTAssertNotEqual(status.label, "Retrying connection…")
            XCTAssertFalse((status.value as? String)?.isEmpty ?? true)
        } else {
            let retrying = app.descendants(matching: .any).matching(
                NSPredicate(format: "label CONTAINS %@", "Retrying connection")
            ).firstMatch
            XCTAssertTrue(retrying.waitForExistence(timeout: 5))
            XCTAssertTrue(retrying.isHittable, "Retrying warning must be visible when the transition occurs")
            XCTAssertEqual(retrying.value as? String, "warning")
        }
        XCTAssertEqual(rideScreen.identifier, family.screenIdentifier)
        XCTAssertTrue(app.descendants(matching: .any)["ride.hero.speed"].exists)
        for _ in 0..<auditScrolls {
            rideScreen.swipeUp()
        }
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
    }

    private func assertFailedVescConnectionAccessibility(
        usesLocalizedText: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))

        let connectionStatus = app.descendants(matching: .any)["device-picker.connection-status"]
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let connecting = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label BEGINSWITH %@", "Connecting"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [connecting], timeout: 3), .completed)
        XCTAssertTrue(connectionStatus.isHittable, "Connecting status must be visible when the transition occurs")
        let connectingLabel = connectionStatus.label

        let failed = XCTNSPredicateExpectation(
            predicate: usesLocalizedText
                ? NSPredicate(format: "label != %@", connectingLabel)
                : NSPredicate(format: "label == %@", "Connect failed: deterministic fixture"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failed], timeout: 5), .completed)
        XCTAssertTrue(picker.exists)
        XCTAssertTrue(connectionStatus.isHittable, "Connection failure must be visible without test-controlled scrolling")
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.screen.vescRide"].exists)
        XCTAssertFalse(connectionStatus.label.isEmpty)
        restorePickerViewport(picker)
        try performVisibleLayoutAccessibilityAudit()

        let lateRide = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        let resurrected = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true"),
            object: lateRide
        )
        XCTAssertEqual(XCTWaiter.wait(for: [resurrected], timeout: 1), .timedOut)
        XCTAssertFalse(connectionStatus.label.isEmpty)

        let retryLabel = connectionStatus.label
        XCTAssertTrue(pairAvailableDevice(.vesc))
        let retrying = XCTNSPredicateExpectation(
            predicate: usesLocalizedText
                ? NSPredicate(format: "label != %@", retryLabel)
                : NSPredicate(format: "label BEGINSWITH %@", "Connecting"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [retrying], timeout: 3), .completed)
        let retryingLabel = connectionStatus.label

        let failedAgain = XCTNSPredicateExpectation(
            predicate: usesLocalizedText
                ? NSPredicate(format: "label != %@", retryingLabel)
                : NSPredicate(format: "label == %@", "Connect failed: deterministic fixture"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failedAgain], timeout: 5), .completed)
        XCTAssertTrue(picker.exists)
        restorePickerViewport(picker)
        try performVisibleLayoutAccessibilityAudit()

    }

    func testEucRideAndBmsPassAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility()
    }

    func testEucRideAndBmsPassAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility()
    }

    func testEucRideAndBmsPassAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility()
    }

    func testEucBmsDiagnosticsExposeStableAccessibleDataRows() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let diagnostics = app.staticTexts["bms.diagnostics"].firstMatch
        scrollElementFrameIntoViewport(diagnostics, in: bmsScreen, maxScrolls: 20)
        XCTAssertTrue(diagnostics.isHittable, bmsScreen.debugDescription)
        diagnostics.tap()

        let voltage = app.descendants(matching: .any)["dashboard.key-value.voltage"]
        XCTAssertTrue(voltage.waitForExistence(timeout: 5), bmsScreen.debugDescription)
        XCTAssertEqual(voltage.label, "voltage")
        XCTAssertEqual(voltage.value as? String, "82.0 V")
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.key-value.page"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.key-value.page-verification"].exists)
    }

    func testEucBmsOverviewPassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertEucBmsOverviewAccessibility(assertsEnglishEnergy: true)
    }

    func testEucBmsOverviewPassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertEucBmsOverviewAccessibility(assertsEnglishEnergy: true)
    }

    func testEucBmsOverviewPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucBmsOverviewAccessibility(assertsEnglishEnergy: true)
    }

    func testEucBmsOverviewPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucBmsOverviewAccessibility(scrollsBeforeAudit: true)
    }

    func testEucBmsOverviewPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucBmsOverviewAccessibility()
    }

    func testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        XCTAssertTrue(bmsScreen.exists)
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.pack"].isSelected)
        XCTAssertTrue(reachableBmsGroup(7, in: bmsScreen).isHittable)
        restoreDashboardViewport(bmsScreen)
        revealBottomEdgeContent(in: bmsScreen)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast() throws {
        try assertEucBmsAccessibility(
            excluding: [],
            assertsEnglishMetric: false,
            scrollsBeforeAudit: 1
        )
    }

    func testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility(
            excluding: [],
            assertsEnglishMetric: false,
            scrollsBeforeAudit: 1
        )
    }

    func testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility()
    }

    func testEucBmsDetailPassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility(
            ignoringClippedBmsDetailBoundaryWarnings: true,
            auditTopTitle: "Cell detail"
        )
    }

    func testEucBmsDetailPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility(
            ignoringClippedBmsDetailBoundaryWarnings: true,
            auditTopTitle: "Cell detail"
        )
    }

    func testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast() throws {
        try assertEucBmsDetailAccessibility(excluding: [])
    }

    func testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility(
            excluding: [],
            ignoringClippedBmsDetailBoundaryWarnings: true
        )
    }

    func testEucBmsPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucBmsAccessibility()
    }

    func testEucBmsPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfaceDoesNotInventARidingRule() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsNoData"))
        defer { disconnectIfConnected() }

        XCTAssertFalse(
            bmsScreen.staticTexts["RIDING RULE"].exists,
            "No-BMS telemetry does not provide a riding rule; do not relabel a capture action as safety guidance."
        )
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast() throws {
        try assertEucNoBmsSurface(auditExclusions: [])
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithIncreasedContrast() throws {
        try assertEucNoBmsSurface(auditExclusions: [])
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface(auditExclusions: [])
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface(auditExclusions: [])
    }

    func testEucUnknownTopologyPassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertEucUnknownTopologySurface()
    }

    func testEucUnknownTopologyPassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertEucUnknownTopologySurface()
    }

    func testEucUnknownTopologyPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucUnknownTopologySurface()
    }

    func testEucUnknownTopologyPassesAccessibilityAuditWithIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucUnknownTopologySurface(auditExclusions: [])
    }

    func testEucUnknownTopologyPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast() throws {
        try assertEucUnknownTopologySurface(auditExclusions: [])
    }

    func testEucUnknownTopologyPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucUnknownTopologySurface(auditExclusions: [])
    }

    func testEucUnknownTopologyPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucUnknownTopologySurface()
    }

    func testEucRideAndBmsPassAccessibilityAuditWithIncreasedContrast() throws {
        try assertEucBmsAccessibility(excluding: [])
    }

    func testEucRideAndBmsPassAccessibilityAuditWithIncreasedContrastAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility(excluding: [])
    }

    func testEucBmsGroupOpensAccessibleDetailAndReturnsToMap() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = reachableBmsGroup(7, in: bmsScreen)
        group.tap()

        let detailScreen = app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 5))
        assertSelectedBmsGroupDetailIsReachable(in: detailScreen)

        let backToMap = app.buttons["bms.detail.back"]
        XCTAssertTrue(backToMap.exists)
        XCTAssertTrue(backToMap.isHittable)
        backToMap.tap()
        XCTAssertTrue(bmsScreen.waitForExistence(timeout: 5))
        XCTAssertFalse(detailScreen.waitForExistence(timeout: 2))
    }

    func testEucBmsFormattedAccessibilityCopyResolvesWithoutPlaceholders() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = reachableBmsGroup(7, in: bmsScreen)
        let accessibilityText = [group.label, group.value as? String]
            .compactMap { $0 }
            .joined(separator: " ")

        XCTAssertEqual(group.label, "Cell group 7, right pack group 7")
        XCTAssertFalse(accessibilityText.contains("$"))
        XCTAssertFalse(accessibilityText.split(separator: " ").contains("@"))
        XCTAssertTrue(accessibilityText.contains("7"))
        XCTAssertTrue(accessibilityText.contains("4.036"))
    }

    func testEucBmsDetailPassesAccessibilityAuditWithIncreasedContrast() throws {
        try assertEucBmsDetailAccessibility(excluding: .all.subtracting(.contrast))
    }

    func testEucBmsDetailPassesAccessibilityAuditWithIncreasedContrastAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility(
            excluding: [],
            ignoringClippedBmsDetailBoundaryWarnings: true
        )
    }

    func testEucBmsDetailPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility()
    }

    func testEucBmsDetailPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucBmsDetailAccessibility(
            ignoringVisibleBmsDetailBackControlContrastWarning: true
        )
    }

    func testVescRidePassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: "voltage"
        )
    }

    func testVescRidePassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: "voltage"
        )
    }

    func testVescRidePassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: "voltage"
        )
    }

    func testEucRideTelemetryAgesWithoutAnotherSampleAtAccessibilityDynamicType() {
        assertMountedTelemetryAges(for: .euc)
    }

    func testVescRideTelemetryAgesWithoutAnotherSampleAtAccessibilityDynamicType() {
        assertMountedTelemetryAges(for: .vesc)
    }

    private func assertMountedTelemetryAges(for family: ConnectedDeviceFamily) {
        XCTAssertTrue(pairAvailableDevice(family))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic \(family.name) fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let status = app.descendants(matching: .any)["ride.hero.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertFalse(status.label.contains("Telemetry stale"), "The fixture must mount with fresh telemetry")

        let stale = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let status = object as? XCUIElement else { return false }
                return status.exists && status.label.contains("Telemetry stale")
            },
            object: status
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [stale], timeout: 4),
            .completed,
            "The mounted \(family.name) Ride screen did not become stale when telemetry stopped"
        )
    }

    func testVescStaleTelemetryIsAnAccessibleWarningAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescWheelslipIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Wheel slip")
    }

    func testVescLowVoltageIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Low voltage")
    }

    func testVescHighVoltageIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "High voltage")
    }

    func testVescMosfetTemperatureIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Controller overheating")
    }

    func testVescMotorTemperatureIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Motor overheating")
    }

    func testVescCurrentLimitIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Current limit")
    }

    func testVescDutyPushbackIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Pushback soon")
    }

    func testVescTemperaturePushbackIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Temperature pushback")
    }

    func testVescSensorWarningIsAccessibleAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Sensor warning")
    }

    func testVescLowBatteryIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Low battery")
    }

    func testVescControllerErrorIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Controller error")
    }

    func testVescPitchStopIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Stopped: pitch")
    }

    func testVescRollStopIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Stopped: roll")
    }

    func testVescSwitchHalfStopIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Half-footpad stop")
    }

    func testVescSwitchFullStopIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Footpad stop")
    }

    func testVescReverseStopIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Reverse stop")
    }

    func testVescQuickStopIsAnAccessibleWarningAtAccessibilityDynamicType() {
        assertVescTypedWarning(label: "Quick stop")
    }

    func testVescHandtestModeIsVisibleAtAccessibilityDynamicType() {
        assertVescOperatingMode(label: "Hand test")
    }

    func testVescDarkrideModeIsVisibleAtAccessibilityDynamicType() {
        assertVescOperatingMode(label: "Darkride")
    }

    func testVescFlywheelModeIsVisibleAtAccessibilityDynamicType() {
        assertVescOperatingMode(label: "Flywheel test")
    }

    private func assertVescOperatingMode(label: String) {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic Refloat mode fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let status = app.descendants(matching: .any)["ride.hero.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(status.label.contains(label), status.debugDescription)
        XCTAssertTrue(status.isHittable)
    }

    private func assertVescTypedWarning(label: String) {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic Refloat warning fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let warning = screen.descendants(matching: .any)["vesc.warning.active"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5), screen.debugDescription)
        XCTAssertEqual(warning.label, label)
        XCTAssertFalse((warning.value as? String ?? "").isEmpty)

        let speed = screen.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        let ordered = screen.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier == 'ride.hero.speed' OR identifier == 'vesc.warning.active'"
            )
        )
        XCTAssertEqual(ordered.count, 2, screen.debugDescription)
        XCTAssertEqual(ordered.element(boundBy: 0).identifier, "vesc.warning.active")
    }

    func testVescStaleTelemetryPrioritizesWarningForAccessibilityAtAccessibilityDynamicType() {
        assertWarningPrecedesSpeed(
            for: .vesc,
            warningIdentifier: "vesc.warning.telemetry-stale"
        )
    }

    func testEucStaleTelemetryPrioritizesWarningForAccessibilityAtAccessibilityDynamicType() {
        assertWarningPrecedesSpeed(for: .euc, warningIdentifier: "euc.warning")
    }

    func testVescPendingTelemetryPrioritizesWarningForAccessibilityAtAccessibilityDynamicType() {
        assertWarningPrecedesSpeed(
            for: .vesc,
            warningIdentifier: "vesc.warning.telemetry-pending"
        )
    }

    private func assertWarningPrecedesSpeed(
        for family: ConnectedDeviceFamily,
        warningIdentifier: String
    ) {
        XCTAssertTrue(pairAvailableDevice(family))
        let familyScreen = app.descendants(matching: .any)[family.screenIdentifier]
        XCTAssertTrue(familyScreen.waitForExistence(timeout: 20), app.debugDescription)

        let orderedSafetyValues = familyScreen.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier == 'ride.hero.speed' OR identifier == %@",
                warningIdentifier
            )
        )
        XCTAssertEqual(orderedSafetyValues.count, 2, familyScreen.debugDescription)
        XCTAssertEqual(
            orderedSafetyValues.element(boundBy: 0).identifier,
            warningIdentifier
        )
    }

    func testVescStaleTelemetryIsAnAccessibleWarningInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescStaleTelemetryIsAnAccessibleWarningInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility(ignoringNilElementContrastWarning: true)
    }

    func testVescStaleTelemetryIsAnAccessibleWarningInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescStaleTelemetryWarningPrecedesMetricsInLandscapeAtAccessibilityDynamicType() {
        XCTAssertEqual(fixture, .vescStale)
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic stale VESC fixture did not open its Ride screen")
            return
        }

        let warning = screen.descendants(matching: .any)["vesc.warning.telemetry-stale"]
        let voltage = screen.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", "voltage")
        ).firstMatch
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        for _ in 0..<6 where !voltage.exists {
            let start = screen.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75))
            let end = screen.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2))
            start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .fast, thenHoldForDuration: 0)
        }
        XCTAssertTrue(voltage.waitForExistence(timeout: 5))
        XCTAssertLessThan(
            warning.frame.minY,
            voltage.frame.minY,
            "The stale safety warning must appear before ordinary metrics"
        )
    }

    func testVescStaleTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility(
            usesLocalizedText: true,
            ignoringNilElementContrastWarning: true
        )
    }

    func testVescStaleTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility(
            usesLocalizedText: true,
            ignoringNilElementContrastWarning: true
        )
    }

    func testVescStaleTelemetryIsAnAccessibleWarningInRightToLeftLayout() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryIsAnAccessibleWarningAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryIsAnAccessibleWarningInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryIsAnAccessibleWarningInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility(ignoringNilElementContrastWarning: true)
    }

    func testEucStaleTelemetryIsAnAccessibleWarningInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility(usesLocalizedText: true)
    }

    func testEucStaleTelemetryIsAnAccessibleWarningInRightToLeftLayout() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    private func assertEucStaleTelemetryAccessibility(
        usesLocalizedText: Bool = false,
        ignoringNilElementContrastWarning: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic stale EUC fixture did not open its Ride screen")
            return
        }

        let warning = app.descendants(matching: .any)["euc.warning"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        if !isLandscapeTest {
            XCTAssertTrue(warning.isHittable, "The EUC stale warning must be visible without scrolling")
        }
        if usesLocalizedText {
            XCTAssertNotEqual(warning.label, "Telemetry stale")
            XCTAssertFalse(warning.label.isEmpty)
            XCTAssertFalse((warning.value as? String ?? "").isEmpty)
        } else {
            XCTAssertEqual(warning.label, "Telemetry stale")
            XCTAssertTrue((warning.value as? String)?.hasPrefix("Last update ") == true)
        }

        let status = app.descendants(matching: .any)["ride.hero.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(status.isHittable, "The EUC stale operating status must be visible without scrolling")
        if usesLocalizedText {
            XCTAssertFalse(status.label.isEmpty)
            XCTAssertFalse((status.value as? String ?? "").isEmpty)
        } else {
            XCTAssertTrue(status.label.contains("Telemetry stale"))
            XCTAssertEqual(status.value as? String, "warning")
        }
        scrollSafetyWarningAboveNavigation(warning, in: screen)
        XCTAssertTrue(warning.isHittable, "The EUC stale warning cannot be reached by scrolling")
        try performVisibleLayoutAccessibilityAudit(
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
    }

    func testVescPendingTelemetryIsAnAccessibleWarningAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    func testVescPendingTelemetryIsAnAccessibleWarningInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    func testVescPendingTelemetryIsAnAccessibleWarningInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    func testVescPendingTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility(usesLocalizedText: true)
    }

    func testVescPendingTelemetryIsAnAccessibleWarningInRightToLeftLayout() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    private func assertVescPendingTelemetryAccessibility(
        usesLocalizedText: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic pending VESC fixture did not open its Ride screen")
            return
        }

        let warning = app.descendants(matching: .any)["vesc.warning.telemetry-pending"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        let status = app.descendants(matching: .any)["ride.hero.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(status.isHittable, "The pending operating status must be visible without scrolling")
        scrollSafetyWarningAboveNavigation(warning, in: screen)
        XCTAssertTrue(warning.isHittable, "The pending warning cannot be reached by scrolling")
        if usesLocalizedText {
            XCTAssertNotEqual(warning.label, "Telemetry pending")
            XCTAssertFalse(warning.label.isEmpty)
            XCTAssertFalse((warning.value as? String ?? "").isEmpty)
            XCTAssertFalse(status.label.isEmpty)
            XCTAssertFalse((status.value as? String ?? "").isEmpty)
        } else {
            XCTAssertEqual(warning.label, "Telemetry pending")
            XCTAssertEqual(warning.value as? String, "Waiting for live values.")
            XCTAssertTrue(status.label.contains("Telemetry pending"))
            XCTAssertEqual(status.value as? String, "warning")
        }
        try performVisibleLayoutAccessibilityAudit(ignoringNilElementContrastWarning: true)
    }

    private func assertVescStaleTelemetryAccessibility(
        usesLocalizedText: Bool = false,
        ignoringNilElementContrastWarning: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic stale VESC fixture did not open its Ride screen")
            return
        }

        let warning = app.descendants(matching: .any)["vesc.warning.telemetry-stale"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        let status = app.descendants(matching: .any)["ride.hero.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(status.isHittable, "The stale operating status must be visible without scrolling")
        scrollSafetyWarningAboveNavigation(warning, in: screen)
        XCTAssertTrue(warning.isHittable, "The stale warning cannot be reached by scrolling")
        if usesLocalizedText {
            XCTAssertNotEqual(warning.label, "Telemetry stale")
            XCTAssertFalse(warning.label.isEmpty)
            XCTAssertFalse((warning.value as? String ?? "").isEmpty)
            XCTAssertFalse(status.label.isEmpty)
            XCTAssertFalse((status.value as? String ?? "").isEmpty)
        } else {
            XCTAssertEqual(warning.label, "Telemetry stale")
            XCTAssertTrue(status.label.contains("Telemetry stale"))
            XCTAssertEqual(status.value as? String, "warning")
            XCTAssertTrue(
                (warning.value as? String)?.hasPrefix("Last update ") == true,
                "The stale warning must expose its elapsed-telemetry detail: \(String(describing: warning.value))"
            )
        }
        try performVisibleLayoutAccessibilityAudit(
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
    }

    private func scrollSafetyWarningAboveNavigation(
        _ warning: XCUIElement,
        in screen: XCUIElement
    ) {
        let navigationTop = [
            app.tabBars.buttons["dashboard.nav.ride"],
            app.tabBars.buttons["dashboard.nav.debug"],
        ]
        .filter(\.exists)
        .map(\.frame.minY)
        .min() ?? screen.frame.maxY
        let unobscuredFrame = CGRect(
            x: screen.frame.minX,
            y: screen.frame.minY,
            width: screen.frame.width,
            height: max(0, navigationTop - screen.frame.minY)
        )
        let isReachable: (CGRect) -> Bool = { frame in
            if frame.height <= unobscuredFrame.height {
                return unobscuredFrame.contains(frame)
            }
            return unobscuredFrame.contains(CGPoint(x: frame.midX, y: frame.minY))
        }
        for _ in 0..<12 where !isReachable(warning.frame) {
            let isAboveViewport = warning.frame.minY < unobscuredFrame.minY
            let startY = isAboveViewport ? 0.15 : 0.55
            let endY = isAboveViewport ? 0.55 : 0.15
            let start = screen.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: startY))
            let end = screen.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: endY))
            start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
        }
        XCTAssertTrue(isReachable(warning.frame), screen.debugDescription)
    }

    func testVescRidePassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        XCTAssertTrue(screen.exists)
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.ride"].exists)
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.debug"].exists)

        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertFalse((speed.value as? String)?.isEmpty ?? true)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testVescRidePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: nil,
            auditExclusions: []
        )
    }

    func testVescRidePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: nil,
            auditExclusions: []
        )
    }

    func testVescDutyHeadroomSpeaksPercentAtAccessibilityDynamicType() {
        assertVescDutyHeadroomAccessibility()
    }

    func testVescDutyHeadroomSpeaksPercentWithIncreasedContrastAtAccessibilityDynamicType() {
        assertVescDutyHeadroomAccessibility()
    }

    private func assertVescDutyHeadroomAccessibility() {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let headroom = screen.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", "Duty headroom")
        ).firstMatch
        XCTAssertTrue(headroom.waitForExistence(timeout: 5))
        scrollElementFrameIntoViewport(
            headroom,
            in: screen,
            maxScrolls: 4,
            occludedBy: app.tabBars.firstMatch,
            requiresFullVisibility: false
        )
        XCTAssertTrue(headroom.isHittable)
        XCTAssertTrue(unobscuredFrame(in: screen, above: app.tabBars.firstMatch).intersects(headroom.frame))
        XCTAssertTrue(
            (headroom.value as? String)?.contains("28%") == true,
            "The VESC duty-headroom metric must speak its percent unit: \(String(describing: headroom.value))"
        )
    }

    func testVescRidePassesAccessibilityAuditAtExtraExtraExtraLargeType() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testVescRidePassesAccessibilityAuditInLandscapeAtExtraExtraExtraLargeType() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testEucRidePassesAccessibilityAuditAtExtraExtraExtraLargeType() throws {
        try assertConnectedSurface(for: .euc, requiredMetricLabel: "pack")
    }

    func testEucRidePassesAccessibilityAuditInLandscapeAtExtraExtraExtraLargeType() throws {
        try assertConnectedSurface(for: .euc, requiredMetricLabel: "pack")
    }

    func testVescRidePassesAccessibilityAuditWithIncreasedContrast() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: "voltage",
            // The dedicated Accessibility-XXXL routes exercise rendered
            // Dynamic Type. This fixed-size scenario owns contrast.
            auditExclusions: .dynamicType
        )
    }

    func testVescRidePassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testVescRideRecordsUnmirroredTabOrderInRightToLeftLayout() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: "voltage",
            expectsMirroredTabOrder: false
        )
    }

    func testVescDebugPassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface()
    }

    func testVescDebugPassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface()
    }

    func testVescDebugPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface()
    }

    func testVescDebugPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface(requiredMetricLabel: nil)
    }

    func testVescDebugPassesAccessibilityAuditWithIncreasedContrast() throws {
        try assertVescDebugSurface(auditExclusions: [])
    }

    func testVescDebugPassesAccessibilityAuditWithIncreasedContrastAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface(auditExclusions: [])
    }

    func testVescDebugPassesAccessibilityAuditWithIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface(auditExclusions: [])
    }

    func testVescDebugPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface(
            auditExclusions: [],
            requiredMetricLabel: nil
        )
    }

    func testVescDebugPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface()
    }

    func testVescDebugPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertVescDebugSurface()
    }

    private func assertConnectedSurface(
        for family: ConnectedDeviceFamily,
        requiredMetricLabel: String? = nil,
        auditExclusions: XCUIAccessibilityAuditType = [],
        ignoringNilElementContrastWarning: Bool = false,
        expectsMirroredTabOrder: Bool = true
    ) throws {
        let pairingAttempted = pairAvailableDevice(family)

        guard pairingAttempted else {
            XCTFail("The deterministic \(family.name) fixture did not expose a Use button")
            return
        }
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The visible \(family.name) Use button was tapped, but no connected dashboard appeared")
            return
        }
        let screen = app.descendants(matching: .any)[family.screenIdentifier]
        XCTAssertTrue(screen.exists)
        defer { disconnectIfConnected() }
        XCTAssertEqual(screen.identifier, family.screenIdentifier)
        XCTAssertFalse(app.descendants(matching: .any)["device-picker.screen"].isHittable)

        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        let liveTelemetry = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let speed = object as? XCUIElement,
                      let value = speed.value as? String
                else { return false }
                return speed.exists && !value.isEmpty && value != "unavailable"
            },
            object: speed
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [liveTelemetry], timeout: 20),
            .completed,
            "The \(family.name) Ride screen never exposed live speed through accessibility"
        )
        if family == .vesc {
            let spokenSpeed = try XCTUnwrap(speed.value as? String)
            for qualifier in ["available", "vehicle telemetry"] {
                XCTAssertTrue(
                    spokenSpeed.contains(qualifier),
                    "The VESC speed accessibility value is missing \(qualifier): \(spokenSpeed)"
                )
            }
            XCTAssertTrue(
                ["fresh", "stale", "freshness unavailable"].contains(where: spokenSpeed.contains),
                "The VESC speed accessibility value has no freshness: \(spokenSpeed)"
            )
            XCTAssertTrue(
                ["nominal", "caution", "critical", "severity unavailable"].contains(where: spokenSpeed.contains),
                "The VESC speed accessibility value has no severity: \(spokenSpeed)"
            )
        }

        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(tabBar.exists)
        XCTAssertEqual(app.tabBars.count, 1)
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.top.navigation"].exists)

        for tab in family.tabNames {
            let element = tabBar.buttons["dashboard.nav.\(tab)"]
            XCTAssertTrue(element.exists)
            XCTAssertTrue(element.isHittable)
            XCTAssertEqual(element.isSelected, tab == "ride")
        }

        if name.contains("RightToLeft"), family.tabNames.count > 1 {
            let firstTab = tabBar.buttons["dashboard.nav.\(family.tabNames[0])"]
            let secondTab = tabBar.buttons["dashboard.nav.\(family.tabNames[1])"]
            XCTAssertEqual(
                firstTab.frame.midX > secondTab.frame.midX,
                expectsMirroredTabOrder,
                expectsMirroredTabOrder
                    ? "The system tab order did not mirror for the Arabic right-to-left launch"
                    : "The VESC tab order unexpectedly mirrored; update the documented RTL evidence"
            )
        }

        for unavailableTab in family.unavailableTabNames {
            XCTAssertFalse(tabBar.buttons["dashboard.nav.\(unavailableTab)"].exists)
        }

        if let requiredMetricLabel {
            assertMetricIsReachable(requiredMetricLabel, in: screen)
            restoreDashboardViewport(screen)
        }

        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
    }

    private func assertVescDebugSurface(
        auditExclusions: XCUIAccessibilityAuditType = [],
        requiredMetricLabel: String? = "duty"
    ) throws {
        guard pairAvailableDevice(.vesc), connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let debugTab = app.tabBars.buttons["dashboard.nav.debug"]
        XCTAssertTrue(debugTab.waitForExistence(timeout: 5))
        XCTAssertTrue(debugTab.isHittable)
        debugTab.tap()

        let debugScreen = app.descendants(matching: .any)["dashboard.screen.vescDebug"]
        XCTAssertTrue(debugScreen.waitForExistence(timeout: 5))
        XCTAssertTrue(debugTab.isSelected)
        for rowID in ["phase", "voltage"] {
            let row = app.descendants(matching: .any)["dashboard.key-value.\(rowID)"]
            XCTAssertTrue(row.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertFalse(row.label.isEmpty)
            XCTAssertFalse((row.value as? String)?.isEmpty ?? true)
        }
        if let requiredMetricLabel {
            assertMetricIsReachable(requiredMetricLabel, in: debugScreen)
        }
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions
        )
    }

    private var launchArguments: [String] {
        var arguments = fixture.launchArguments
        if name.contains("Pseudolocalized") {
            arguments += ["-NSDoubleLocalizedStrings", "YES"]
        }
        if name.contains("RightToLeft") {
            arguments += [
                "-AppleLanguages", "(ar)",
                "-AppleLocale", "ar_SA",
            ]
        }

        return arguments
    }

    private var isLandscapeTest: Bool {
        name.contains("InLandscape")
    }

    private var fixture: Fixture { Fixture.testFixture(for: name) }

    private enum Fixture: Equatable {
        case unknownDevice
        case unknownDeviceFinishFailure
        case probeDevice
        case probeTimeout
        case probeMalformedResponse
        case probeConflictingEvidence
        case probeUnsupported
        case bluetoothUnavailable
        case bluetoothPermissionDenied
        case euc
        case eucDynamic
        case eucStale
        case eucReconnect
        case eucOverview
        case eucNoBms
        case eucUnknownTopology
        case vesc
        case vescDynamic
        case vescLowVoltage
        case vescHighVoltage
        case vescMosfetTemperature
        case vescMotorTemperature
        case vescCurrent
        case vescDutyPushback
        case vescTemperaturePushback
        case vescWheelslip
        case vescSensors
        case vescLowBattery
        case vescError
        case vescPitchStop
        case vescRollStop
        case vescSwitchHalfStop
        case vescSwitchFullStop
        case vescReverseStop
        case vescQuickStop
        case vescHandtest
        case vescDarkride
        case vescFlywheel
        case vescPending
        case vescStale
        case vescFailure
        case vescReconnect
        case vescBluetoothLoss
        case vescConnecting
        case eucConnecting
        case vescLiveActivity
        case vescLiveActivityAuto
        case vescDynamicLiveActivityAuto
        case vescCriticalLiveActivityAuto
        case vescUnavailableLiveActivityAuto
        case vescStaleLiveActivityAuto

        static func testFixture(for testName: String) -> Self {
            if testName.contains("BackgroundFlushFailure") || testName.contains("FinishCaptureFailure") {
                return .unknownDeviceFinishFailure
            }
            if testName.contains("ProbeTimeout") { return .probeTimeout }
            if testName.contains("ProbeMalformedResponse") { return .probeMalformedResponse }
            if testName.contains("ProbeConflictingEvidence") { return .probeConflictingEvidence }
            if testName.contains("ProbeUnsupported") { return .probeUnsupported }
            if testName.contains("ProbeAction") { return .probeDevice }
            if testName.contains("Capture") || testName.contains("Advanced") { return .unknownDevice }
            if testName.contains("BluetoothUnavailableAfterLive") { return .vescBluetoothLoss }
            if testName.contains("BluetoothUnavailable") { return .bluetoothUnavailable }
            if testName.contains("BluetoothPermissionDenied") { return .bluetoothPermissionDenied }
            if testName.contains("LiveActivityContinuesUpdatingWhileBackgrounded") {
                return .vescDynamicLiveActivityAuto
            }
            if testName.contains("CriticalLiveActivityAutoFixture")
                || testName.contains("CriticalLiveActivityLockScreen")
            {
                return .vescCriticalLiveActivityAuto
            }
            if testName.contains("UnavailableLiveActivityLockScreen") {
                return .vescUnavailableLiveActivityAuto
            }
            if testName.contains("StaleLiveActivityLockScreen") {
                return .vescStaleLiveActivityAuto
            }
            if testName.contains("UnavailableLiveActivityAutoFixture") {
                return .vescUnavailableLiveActivityAuto
            }
            if testName.contains("StaleLiveActivityAutoFixture") {
                return .vescStaleLiveActivityAuto
            }
            if testName.contains("LiveActivityLockScreen") { return .vescLiveActivityAuto }
            if testName.contains("LiveActivityAutoFixture") { return .vescLiveActivityAuto }
            if testName.contains("LiveActivityFixture") { return .vescLiveActivity }
            if testName.contains("FailedVescConnection") { return .vescFailure }
            if testName.localizedCaseInsensitiveContains("EucUseShowsConnecting") { return .eucConnecting }
            if testName.contains("UseShowsConnecting") { return .vescConnecting }
            if testName.localizedCaseInsensitiveContains("EucStaleTelemetry") { return .eucStale }
            if testName.localizedCaseInsensitiveContains("EucReconnect") { return .eucReconnect }
            if testName.contains("Reconnect") { return .vescReconnect }
            if testName.contains("PendingTelemetry") { return .vescPending }
            if testName.contains("VescLowVoltage") { return .vescLowVoltage }
            if testName.contains("VescHighVoltage") { return .vescHighVoltage }
            if testName.contains("VescMosfetTemperature") { return .vescMosfetTemperature }
            if testName.contains("VescMotorTemperature") { return .vescMotorTemperature }
            if testName.contains("VescCurrentLimit") { return .vescCurrent }
            if testName.contains("VescDutyPushback") { return .vescDutyPushback }
            if testName.contains("VescTemperaturePushback") { return .vescTemperaturePushback }
            if testName.contains("VescWheelslip") { return .vescWheelslip }
            if testName.contains("VescSensorWarning") { return .vescSensors }
            if testName.contains("VescLowBattery") { return .vescLowBattery }
            if testName.contains("VescControllerError") { return .vescError }
            if testName.contains("VescPitchStop") { return .vescPitchStop }
            if testName.contains("VescRollStop") { return .vescRollStop }
            if testName.contains("VescSwitchHalfStop") { return .vescSwitchHalfStop }
            if testName.contains("VescSwitchFullStop") { return .vescSwitchFullStop }
            if testName.contains("VescReverseStop") { return .vescReverseStop }
            if testName.contains("VescQuickStop") { return .vescQuickStop }
            if testName.contains("VescHandtest") { return .vescHandtest }
            if testName.contains("VescDarkride") { return .vescDarkride }
            if testName.contains("VescFlywheel") { return .vescFlywheel }
            if testName.contains("DutyHeadroom") { return .vescDynamic }
            if testName.localizedCaseInsensitiveContains("Euc"), testName.contains("DynamicTelemetry") {
                return .eucDynamic
            }
            if testName.contains("DynamicTelemetry") { return .vescDynamic }
            if testName.contains("StaleTelemetry") { return .vescStale }
            if testName.localizedCaseInsensitiveContains("EucBmsOverview") { return .eucOverview }
            if testName.localizedCaseInsensitiveContains("EucNoBms") { return .eucNoBms }
            if testName.localizedCaseInsensitiveContains("EucUnknownTopology") { return .eucUnknownTopology }
            if testName.localizedCaseInsensitiveContains("Euc") { return .euc }
            return .vesc
        }

        var launchArguments: [String] {
            ["-CUTOUT_UI_TEST_FIXTURE", value]
        }

        var launchEnvironment: [String: String] {
            ["CUTOUT_UI_TEST_FIXTURE": value]
        }

        private var value: String {
            switch self {
            case .unknownDevice: "unknown-device"
            case .unknownDeviceFinishFailure: "unknown-device-finish-failure"
            case .probeDevice: "probe-device"
            case .probeTimeout: "probe-timeout"
            case .probeMalformedResponse: "probe-malformed"
            case .probeConflictingEvidence: "probe-conflict"
            case .probeUnsupported: "probe-unsupported"
            case .bluetoothUnavailable: "bluetooth-unavailable"
            case .bluetoothPermissionDenied: "bluetooth-permission-denied"
            case .euc: "euc"
            case .eucDynamic: "euc-dynamic"
            case .eucStale: "euc-stale"
            case .eucReconnect: "euc-reconnect"
            case .eucOverview: "euc-overview"
            case .eucNoBms: "euc-no-bms"
            case .eucUnknownTopology: "euc-unknown-topology"
            case .vesc: "vesc"
            case .vescDynamic: "vesc-dynamic"
            case .vescLowVoltage: "vesc-low-voltage"
            case .vescHighVoltage: "vesc-high-voltage"
            case .vescMosfetTemperature: "vesc-mosfet-temperature"
            case .vescMotorTemperature: "vesc-motor-temperature"
            case .vescCurrent: "vesc-current"
            case .vescDutyPushback: "vesc-duty-pushback"
            case .vescTemperaturePushback: "vesc-temperature-pushback"
            case .vescWheelslip: "vesc-wheelslip"
            case .vescSensors: "vesc-sensors"
            case .vescLowBattery: "vesc-low-battery"
            case .vescError: "vesc-error"
            case .vescPitchStop: "vesc-pitch-stop"
            case .vescRollStop: "vesc-roll-stop"
            case .vescSwitchHalfStop: "vesc-switch-half-stop"
            case .vescSwitchFullStop: "vesc-switch-full-stop"
            case .vescReverseStop: "vesc-reverse-stop"
            case .vescQuickStop: "vesc-quick-stop"
            case .vescHandtest: "vesc-handtest"
            case .vescDarkride: "vesc-darkride"
            case .vescFlywheel: "vesc-flywheel"
            case .vescPending: "vesc-pending"
            case .vescStale: "vesc-stale"
            case .vescFailure: "vesc-failure"
            case .vescReconnect: "vesc-reconnect"
            case .vescBluetoothLoss: "vesc-bluetooth-loss"
            case .vescConnecting: "vesc-connecting"
            case .eucConnecting: "euc-connecting"
            case .vescLiveActivity: "vesc-live-activity"
            case .vescLiveActivityAuto: "vesc-live-activity-auto"
            case .vescDynamicLiveActivityAuto: "vesc-live-activity-dynamic-auto"
            case .vescCriticalLiveActivityAuto: "vesc-live-activity-critical-auto"
            case .vescUnavailableLiveActivityAuto: "vesc-live-activity-unavailable-auto"
            case .vescStaleLiveActivityAuto: "vesc-live-activity-stale-auto"
            }
        }
    }

    private func skipLiveActivityTestsOnSimulator() throws {
        guard name.contains("LiveActivityFixture"),
              ProcessInfo.processInfo.environment["SIMULATOR_DEVICE_NAME"] != nil
        else { return }
        throw XCTSkip("Live Activity inspection is reserved for a physical-device ActivityKit run")
    }

    private func enterCapture() {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let finishEditing = app.buttons["device-picker.capture-kind.done"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        XCTAssertTrue(finishEditing.waitForExistence(timeout: 5))
        captureKind.tap()
        captureKind.typeText("custom vesc")
        finishEditing.tap()

        for _ in 0..<6 where !recordButton.exists || !recordButton.isHittable {
            advancedCapture.swipeUp()
        }

        XCTAssertTrue(recordButton.exists, app.debugDescription)
        XCTAssertEqual(recordButton.elementType, .button)
        XCTAssertTrue(recordButton.isHittable)
        recordButton.tap()
        XCTAssertTrue(app.descendants(matching: .any)["capture.screen"].waitForExistence(timeout: 5))
    }

    private func assertProductionPickerAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = [],
        assertsPseudolocalizedCopy: Bool = false
    ) throws {
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        if assertsPseudolocalizedCopy {
            let useButton = app.buttons["device-picker.use.ui-test-vesc"]
            XCTAssertTrue(useButton.waitForExistence(timeout: 5))
            XCTAssertNotEqual(
                useButton.label,
                "Use Refloat VESC, device VESC",
                "The pseudolocalized launch did not expand catalog-backed picker copy"
            )
        }
        try performVisibleLayoutAccessibilityAudit(excluding: excluded)
    }

    private func assertCaptureAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = [],
        exercisesLabels: Bool = true,
        ignoringNilElementContrastWarning: Bool = false
    ) throws {
        enterCapture()

        let screen = app.descendants(matching: .any)["capture.screen"]
        let stopCapture = app.buttons["capture.stop"]
        XCTAssertTrue(screen.exists)

        for _ in 0..<6 where !stopCapture.isHittable {
            screen.swipeUp()
        }

        XCTAssertTrue(stopCapture.exists)
        XCTAssertTrue(stopCapture.isHittable, app.debugDescription)
        try performVisibleLayoutAccessibilityAudit(
            excluding: excluded,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
        guard exercisesLabels else {
            return
        }
        let firstAnnotation = reachableCaptureAnnotation("ride", in: screen)
        XCTAssertTrue(firstAnnotation.isHittable)
        let firstAnnotationInitialLabel = firstAnnotation.label
        XCTAssertFalse(firstAnnotationInitialLabel.isEmpty)
        firstAnnotation.tap()
        XCTAssertNotEqual(firstAnnotation.label, firstAnnotationInitialLabel)

        let lastAnnotation = reachableCaptureAnnotation("pwm_percent", in: screen)
        XCTAssertTrue(lastAnnotation.exists)
        XCTAssertTrue(lastAnnotation.isHittable)
        let lastAnnotationInitialLabel = lastAnnotation.label
        XCTAssertFalse(lastAnnotationInitialLabel.isEmpty)
        lastAnnotation.tap()
        XCTAssertNotEqual(lastAnnotation.label, lastAnnotationInitialLabel)
    }

    private func openAdvancedCapture() -> XCUIElement {
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        let openAdvancedCapture = app.buttons["device-picker.open-advanced-capture"]
        let advancedCapture = app.descendants(matching: .any)["device-picker.advanced-capture"]

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        for _ in 0..<4 where !openAdvancedCapture.isHittable {
            screen.swipeUp()
        }
        XCTAssertTrue(openAdvancedCapture.isHittable)

        openAdvancedCapture.tap()
        XCTAssertTrue(advancedCapture.waitForExistence(timeout: 5))
        return advancedCapture
    }

    private func assertProbeFailure(_ expectedStatus: String) throws {
        _ = openAdvancedCapture()
        let probeButton = app.buttons["device-picker.probe.ui-test-probe"]
        let status = app.descendants(matching: .any)["device-picker.connection-status"]
        let picker = app.descendants(matching: .any)["device-picker.screen"]

        XCTAssertTrue(probeButton.waitForExistence(timeout: 5))
        probeButton.tap()

        XCTAssertEqual(
            XCTWaiter.wait(
                for: [XCTNSPredicateExpectation(
                    predicate: NSPredicate(format: "label == %@", expectedStatus),
                    object: status
                )],
                timeout: 5
            ),
            .completed
        )
        XCTAssertTrue(picker.exists)
        XCTAssertTrue(status.isHittable, "Probe failure must be visible when the transition occurs")
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.screen.eucRide"].exists)
        restorePickerViewport(picker)
        try performVisibleLayoutAccessibilityAudit()
    }

    private func assertMetricIsReachable(_ label: String, in screen: XCUIElement) {
        let metric = screen.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", label)
        ).firstMatch
        scrollElementFrameIntoViewport(
            metric,
            in: screen,
            maxScrolls: 16,
            occludedBy: app.tabBars.firstMatch,
            requiresFullVisibility: false
        )
        XCTAssertFalse(
            (metric.value as? String)?.isEmpty ?? true,
            "The \(label) metric has no accessible value"
        )
    }

    private func assertSelectedBmsGroupDetailIsReachable(in screen: XCUIElement) {
        let heading = screen.staticTexts["bms.detail.selected-group"]
        XCTAssertTrue(heading.exists, "The selected BMS group heading is missing")
        let voltage = screen.staticTexts["bms.detail.voltage"]
        XCTAssertTrue(voltage.exists, "The selected BMS group voltage is missing")
        scrollElementFrameIntoViewport(voltage, in: screen, maxScrolls: 8)
        XCTAssertTrue(voltage.isHittable, screen.debugDescription)
        XCTAssertFalse((voltage.value as? String)?.isEmpty ?? true)
        scrollElementFrameIntoViewport(heading, in: screen, maxScrolls: 8)
        XCTAssertTrue(heading.isHittable, screen.debugDescription)
        scrollElementFrameIntoViewport(voltage, in: screen, maxScrolls: 8)
        XCTAssertTrue(voltage.isHittable, screen.debugDescription)
    }

    private func scrollElementFrameIntoViewport(
        _ element: XCUIElement,
        in screen: XCUIElement,
        maxScrolls: Int,
        occludedBy obstruction: XCUIElement? = nil,
        requiresFullVisibility: Bool = true
    ) {
        for _ in 0..<maxScrolls {
            let unobscuredFrame = unobscuredFrame(in: screen, above: obstruction)
            if element.exists,
               isVisible(element.frame, in: unobscuredFrame, fully: requiresFullVisibility),
               element.isHittable {
                break
            }
            let isAboveViewport = element.exists && element.frame.minY < unobscuredFrame.minY
            let centerY = unobscuredFrame.midY
            let travel = min(48, unobscuredFrame.height * 0.18)
            dragVertically(
                in: screen,
                from: isAboveViewport ? centerY - travel : centerY + travel,
                to: isAboveViewport ? centerY + travel : centerY - travel
            )
        }
        XCTAssertTrue(element.waitForExistence(timeout: 5), screen.debugDescription)
        XCTAssertTrue(
            isVisible(
                element.frame,
                in: unobscuredFrame(in: screen, above: obstruction),
                fully: requiresFullVisibility
            ),
            screen.debugDescription
        )
        XCTAssertTrue(element.isHittable, screen.debugDescription)
    }

    private func isVisible(_ elementFrame: CGRect, in viewport: CGRect, fully: Bool) -> Bool {
        fully ? viewport.contains(elementFrame) : viewport.intersects(elementFrame)
    }

    private func unobscuredFrame(in screen: XCUIElement, above obstruction: XCUIElement?) -> CGRect {
        let maximumY = obstruction?.exists == true
            ? min(screen.frame.maxY, obstruction?.frame.minY ?? screen.frame.maxY)
            : screen.frame.maxY
        return CGRect(
            x: screen.frame.minX,
            y: screen.frame.minY,
            width: screen.frame.width,
            height: max(0, maximumY - screen.frame.minY)
        )
    }

    private func dragVertically(in screen: XCUIElement, from startY: CGFloat, to endY: CGFloat) {
        let frame = screen.frame
        guard frame.height > 0 else { return }
        func normalizedY(_ y: CGFloat) -> CGFloat {
            min(0.92, max(0.08, (y - frame.minY) / frame.height))
        }
        let start = screen.coordinate(
            withNormalizedOffset: CGVector(dx: 0.5, dy: normalizedY(startY))
        )
        let end = screen.coordinate(
            withNormalizedOffset: CGVector(dx: 0.5, dy: normalizedY(endY))
        )
        start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
    }

    private func performVisibleLayoutAccessibilityAudit(
        excluding excluded: XCUIAccessibilityAuditType = [],
        ignoringSystemToolbarDynamicTypeWarning: Bool = false,
        ignoringNilElementContrastWarning: Bool = false,
        ignoringAdvancedCaptureTitleContrastWarning: Bool = false,
        ignoringVisibleBmsDetailBackControlContrastWarning: Bool = false,
        ignoringClippedBmsDetailBoundaryWarnings: Bool = false
    ) throws {
        continueAfterFailure = true
        defer { continueAfterFailure = false }
        let auditTypes = XCUIAccessibilityAuditType.all.subtracting(excluded)
        try app.performAccessibilityAudit(for: auditTypes) { issue in
            let elementDescription = issue.element?.debugDescription ?? "No element"
            print("Accessibility audit issue [\(issue.auditType.rawValue)]: \(issue.detailedDescription)\n\(elementDescription)")
            if issue.auditType == .contrast,
               let element = issue.element,
               !self.app.frame.contains(element.frame) {
                // XCTest sometimes audits lazily retained ScrollView children
                // that are clipped by or outside the rendered app window.
                // Their screenshots do not contain the complete foreground
                // and background pair needed for a meaningful contrast check.
                return true
            }
            if ignoringSystemToolbarDynamicTypeWarning,
               issue.auditType == .dynamicType,
               issue.detailedDescription == "User will not be able to change the font size of this SwiftUI.AccessibilityNode",
               ["Done", "Cancel", "Done Done", "Cancel Cancel"].contains(issue.element?.label) {
                // These are NavigationStack's native toolbar controls. Their
                // rendered screens show the system Dynamic Type buttons; all
                // app-owned Dynamic Type findings remain fatal.
                return true
            }
            if ignoringNilElementContrastWarning,
               issue.auditType == .contrast,
               issue.element == nil,
               issue.detailedDescription == "Contrast failed for SwiftUI.AccessibilityNode" {
                // XCTest supplied no element, frame, or color for this
                // simulator-only diagnostic. Attributable contrast findings
                // still fail this test.
                return true
            }
            if ignoringAdvancedCaptureTitleContrastWarning,
               issue.auditType == .contrast,
               ["Capture unknown device", "Device kind for capture"].contains(issue.element?.label) {
                // Xcode 27 reports these white-on-dark visual heading children
                // only in the Dark advanced-capture cell. Every other finding stays fatal.
                return true
            }
            if ignoringVisibleBmsDetailBackControlContrastWarning,
               issue.auditType == .contrast,
               let element = issue.element,
               elementDescription.contains("identifier: 'bms.detail.back'"),
               self.app.frame.contains(element.frame) {
                // RTL Xcode 27 reports the visible child of this native
                // `.bordered` Button despite its captured opaque background
                // and black text. Other visible controls still fail.
                return true
            }
            if ignoringClippedBmsDetailBoundaryWarnings,
               [.contrast, .dynamicType].contains(issue.auditType),
               let element = issue.element {
                let detail = self.app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
                let groups = self.app.buttons.matching(
                    NSPredicate(format: "identifier BEGINSWITH %@", "bms.group.")
                ).allElementsBoundByIndex
                let selectedGroupChip = self.app.staticTexts["bms.chip.selectedGroup"]
                let tabBar = self.app.tabBars.firstMatch
                let unobscuredFrame = CGRect(
                    x: detail.frame.minX,
                    y: detail.frame.minY,
                    width: detail.frame.width,
                    height: max(
                        0,
                        min(detail.frame.maxY, tabBar.exists ? tabBar.frame.minY : detail.frame.maxY)
                            - detail.frame.minY
                    )
                )
                let isClippedSelectedGroupChip = selectedGroupChip.exists
                    && !unobscuredFrame.contains(selectedGroupChip.frame)
                    && selectedGroupChip.frame.contains(element.frame)
                let isClippedGroupButton = groups.contains {
                    !unobscuredFrame.contains($0.frame) && $0.frame.contains(element.frame)
                }
                if detail.exists, isClippedSelectedGroupChip || isClippedGroupButton {
                    // The viewport intersects an identified chip or group
                    // Button outside the region unobscured by the native tab
                    // bar. Fully visible and unrelated findings stay fatal.
                    return true
                }
            }
            return false
        }
    }

    private func restorePickerViewport(_ picker: XCUIElement) {
        for _ in 0..<4 {
            picker.swipeDown(velocity: .fast)
        }
    }

    private func restoreAdvancedCaptureViewport(
        _ advancedCapture: XCUIElement,
        captureKind: XCUIElement
    ) {
        for _ in 0..<6 where !captureKind.isHittable {
            advancedCapture.swipeDown()
        }
        XCTAssertTrue(captureKind.isHittable)
    }

    private func restoreDashboardViewport(_ screen: XCUIElement) {
        let scrollView = screen.scrollViews.firstMatch
        let scrollTarget = scrollView.exists ? scrollView : screen
        let unobscuredFrame = unobscuredFrame(in: scrollTarget, above: app.tabBars.firstMatch)
        let edgeInset = min(44, unobscuredFrame.height * 0.2)
        for _ in 0..<4 {
            dragVertically(
                in: scrollTarget,
                from: unobscuredFrame.minY + edgeInset,
                to: unobscuredFrame.maxY - edgeInset
            )
        }
    }

    private func revealBottomEdgeContent(in screen: XCUIElement) {
        let scrollView = screen.scrollViews.firstMatch
        let scrollTarget = scrollView.exists ? scrollView : screen
        let start = scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.72))
        let end = scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.48))
        start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
    }

    private func connectedScreen(timeout: TimeInterval = 2) -> XCUIElement? {
        let screen = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "dashboard.screen.")
        ).firstMatch
        return screen.waitForExistence(timeout: timeout) ? screen : nil
    }

    private func disconnectIfConnected() {
        let disconnect = app.buttons["dashboard.disconnect"]
        guard disconnect.waitForExistence(timeout: 2) else { return }
        disconnect.tap()
        _ = app.descendants(matching: .any)["device-picker.screen"].waitForExistence(timeout: 5)
    }

    private func openEucBmsMap() -> XCUIElement? {
        openEucBmsScreen(identifier: "dashboard.screen.bmsCellMap6S")
    }

    private func openEucBmsScreen(identifier: String) -> XCUIElement? {
        guard pairAvailableDevice(.euc) else { return nil }
        guard let rideScreen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic EUC fixture did not open its Ride screen.\n\(app.debugDescription)")
            return nil
        }

        XCTAssertEqual(rideScreen.identifier, ConnectedDeviceFamily.euc.screenIdentifier)
        XCTAssertTrue(app.descendants(matching: .any)["ride.hero.speed"].exists)
        if !name.contains("Pseudolocalized") {
            assertMetricIsReachable("speed", in: rideScreen)
        }

        let packTab = app.tabBars.buttons["dashboard.nav.pack"]
        guard packTab.waitForExistence(timeout: 5), packTab.isHittable else {
            XCTFail("The Pack tab is not available from the EUC Ride screen")
            return nil
        }
        packTab.tap()

        let bmsScreen = app.descendants(matching: .any)[identifier]
        guard bmsScreen.waitForExistence(timeout: 5) else {
            XCTFail("The Pack tab did not open \(identifier)")
            return nil
        }
        return bmsScreen
    }

    private func reachableBmsGroup(_ index: Int, in bmsScreen: XCUIElement) -> XCUIElement {
        let group = app.buttons["bms.group.\(index)"]
        scrollElementFrameIntoViewport(group, in: bmsScreen, maxScrolls: 20)

        XCTAssertTrue(group.waitForExistence(timeout: 5), bmsScreen.debugDescription)
        XCTAssertEqual(group.elementType, .button)
        XCTAssertTrue(group.isHittable, bmsScreen.debugDescription)
        return group
    }

    private func reachableCaptureAnnotation(_ id: String, in screen: XCUIElement) -> XCUIElement {
        let annotation = app.buttons["capture.label.\(id).action"]
        scrollElementFrameIntoViewport(
            annotation,
            in: screen,
            maxScrolls: 30,
            occludedBy: app.buttons["capture.stop"]
        )

        XCTAssertTrue(annotation.waitForExistence(timeout: 5))
        XCTAssertTrue(annotation.isHittable, screen.debugDescription)
        return annotation
    }

    private func assertEucBmsAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = [],
        assertsEnglishMetric: Bool = true,
        scrollsBeforeAudit: Int = 0
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        XCTAssertTrue(app.descendants(matching: .any)["bms.diagnostics"].exists)
        if assertsEnglishMetric {
            assertMetricIsReachable("Cell group 7, right pack group 7", in: bmsScreen)
        } else {
            XCTAssertTrue(bmsScreen.exists)
        }
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.pack"].isSelected)
        restoreDashboardViewport(bmsScreen)
        for _ in 0..<scrollsBeforeAudit {
            bmsScreen.swipeUp()
        }
        try performVisibleLayoutAccessibilityAudit(excluding: excluded)
    }

    private func assertEucBmsOverviewAccessibility(
        assertsEnglishEnergy: Bool = false,
        scrollsBeforeAudit: Bool = false
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsOverview"))
        defer { disconnectIfConnected() }

        let energyHero = app.progressIndicators["bms.energy.hero"]
        XCTAssertTrue(energyHero.waitForExistence(timeout: 5))
        XCTAssertTrue(energyHero.isHittable)
        if assertsEnglishEnergy {
            XCTAssertEqual(energyHero.label, "Usable energy")
            XCTAssertEqual(energyHero.value as? String, "64% and 20S4P test pack")
        } else {
            XCTAssertFalse(energyHero.label.isEmpty)
            XCTAssertFalse((energyHero.value as? String ?? "").isEmpty)
        }
        if scrollsBeforeAudit {
            bmsScreen.swipeUp()
        }
        try performVisibleLayoutAccessibilityAudit()
    }

    private func assertEucBmsDetailAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = [],
        ignoringVisibleBmsDetailBackControlContrastWarning: Bool = false,
        ignoringClippedBmsDetailBoundaryWarnings: Bool = false,
        auditTopTitle: String? = nil
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = reachableBmsGroup(7, in: bmsScreen)
        group.tap()

        let detailScreen = app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 5))
        assertSelectedBmsGroupDetailIsReachable(in: detailScreen)
        restoreDashboardViewport(detailScreen)
        if let auditTopTitle {
            let title = detailScreen.staticTexts[auditTopTitle]
            XCTAssertTrue(title.waitForExistence(timeout: 5))
            XCTAssertTrue(title.isHittable, detailScreen.debugDescription)
        }
        try performVisibleLayoutAccessibilityAudit(
            excluding: excluded,
            ignoringVisibleBmsDetailBackControlContrastWarning: ignoringVisibleBmsDetailBackControlContrastWarning,
            ignoringClippedBmsDetailBoundaryWarnings: ignoringClippedBmsDetailBoundaryWarnings
        )
    }

    private func assertEucNoBmsSurface(
        auditExclusions: XCUIAccessibilityAuditType = []
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsNoData"))
        defer { disconnectIfConnected() }

        let warning = bmsScreen.descendants(matching: .any)["bms.no-data.warning"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        XCTAssertFalse(warning.label.isEmpty)
        try performVisibleLayoutAccessibilityAudit(excluding: auditExclusions)
    }

    private func assertEucUnknownTopologySurface(
        auditExclusions: XCUIAccessibilityAuditType = []
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsUnknownTopology"))
        defer { disconnectIfConnected() }

        let captureFlow = bmsScreen.descendants(matching: .any)["bms.unknown.capture-flow"]
        XCTAssertTrue(captureFlow.waitForExistence(timeout: 5))
        XCTAssertFalse(captureFlow.label.isEmpty)
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions
        )
    }

    @discardableResult
    private func pairAvailableDevice(_ family: ConnectedDeviceFamily) -> Bool {
        if let screen = connectedScreen() {
            if screen.identifier == family.screenIdentifier { return true }
            disconnectIfConnected()
        }

        let button = app.buttons[family.useButtonIdentifier]
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        guard picker.waitForExistence(timeout: 8) else { return false }

        for _ in 0..<6 where !button.exists || !button.isHittable {
            picker.swipeUp()
        }

        guard button.exists, button.isHittable else {
            XCTFail(
                "The \(family.name) Use button cannot be reached by scrolling.\n\(picker.debugDescription)"
            )
            return false
        }
        XCTAssertEqual(button.elementType, .button)
        XCTAssertGreaterThanOrEqual(button.frame.height, 44)
        XCTAssertTrue(family.matches(label: button.label))
        button.tap()
        return true
    }
}

private enum ConnectedDeviceFamily: Equatable {
    case euc
    case vesc

    var name: String {
        switch self {
        case .euc: "EUC"
        case .vesc: "VESC"
        }
    }

    var screenIdentifier: String {
        switch self {
        case .euc: "dashboard.screen.eucRide"
        case .vesc: "dashboard.screen.vescRide"
        }
    }

    var useButtonIdentifier: String {
        switch self {
        case .euc: "device-picker.use.ui-test-euc"
        case .vesc: "device-picker.use.ui-test-vesc"
        }
    }

    var tabNames: [String] {
        switch self {
        case .euc: ["ride", "pack"]
        case .vesc: ["ride", "debug"]
        }
    }

    var unavailableTabNames: [String] {
        switch self {
        case .euc: ["map", "tune"]
        case .vesc: ["map", "logs"]
        }
    }

    func matches(label: String) -> Bool {
        let label = label.lowercased()
        let isVesc = label.contains("vesc") || label.contains("refloat")
            || label.contains("onewheel") || label.contains("floatwheel")
        return self == .vesc ? isVesc : !isVesc
    }
}
