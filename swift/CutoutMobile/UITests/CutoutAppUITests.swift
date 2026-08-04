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
        app.launchArguments = launchArguments
        app.launchEnvironment = fixture.launchEnvironment
        app.terminate()
        app.launch()
    }

    override func tearDown() async throws {
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
        XCTAssertTrue(captureKind.isHittable)
        XCTAssertTrue(finishEditing.exists)
        XCTAssertEqual(finishEditing.label, "Done")
        XCTAssertTrue(cancelCapture.exists)
        XCTAssertEqual(cancelCapture.label, "Cancel")
        XCTAssertTrue(recordButton.waitForExistence(timeout: 5))
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

    func testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRide() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth permission denied")
    }

    func testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRideInRightToLeftLayout() throws {
        try assertBluetoothBlockedPicker(status: "Bluetooth permission denied")
    }

    private func assertBluetoothBlockedPicker(status expectedStatus: String) throws {
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let status = app.descendants(matching: .any)["device-picker.connection-status"]

        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertEqual(status.label, expectedStatus)
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

        let details = app.descendants(matching: .any)["capture.session-details"]
        let writer = details.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", "Writer")
        ).firstMatch
        let pendingWrites = details.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", "Pending writes")
        ).firstMatch

        XCTAssertTrue(details.waitForExistence(timeout: 5))
        XCTAssertTrue(pendingWrites.exists)
        XCTAssertEqual(pendingWrites.value as? String, "0")
        XCTAssertTrue(writer.exists)
        XCTAssertEqual(writer.value as? String, "Healthy")
    }

    func testFinishCaptureReturnsToPickerAfterFinalizing() throws {
        _ = try finishCaptureAndReturnToPicker()
    }

    func testFinishCaptureReturnsToAccessiblePickerWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        _ = try finishCaptureAndReturnToPicker()
        try performVisibleLayoutAccessibilityAudit()
    }

    private func finishCaptureAndReturnToPicker() throws -> XCUIElement {
        enterCapture()

        let finish = app.buttons["capture.stop"]
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(finish.waitForExistence(timeout: 5))
        XCTAssertEqual(finish.elementType, .button)
        finish.tap()

        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertTrue(picker.isHittable)
        XCTAssertFalse(app.descendants(matching: .any)["capture.screen"].isHittable)
        return picker
    }

    func testFinishCaptureFailureKeepsCaptureScreenVisible() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible()
    }

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleAtAccessibilityDynamicType() throws {
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
        usesLocalizedText: Bool = false
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
        XCTAssertTrue(finish.exists)
        if !usesLocalizedText {
            XCTAssertEqual(status.label, "Capture failed")
        }
        try performVisibleLayoutAccessibilityAudit()
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
        XCTAssertTrue(ride.waitForExistence(timeout: 20))

        app.buttons["dashboard.disconnect"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
    }

    private func assertUseDisconnectCycles(for family: ConnectedDeviceFamily) {
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let ride = app.descendants(matching: .any)[family.screenIdentifier]
        let disconnect = app.buttons["dashboard.disconnect"]

        for cycle in 1...3 {
            XCTAssertTrue(pairAvailableDevice(family), "Cycle \(cycle) did not start from the native \(family.name) Use button")
            XCTAssertTrue(ride.waitForExistence(timeout: 20), "Cycle \(cycle) did not open the \(family.name) Ride screen")
            XCTAssertTrue(disconnect.waitForExistence(timeout: 5))
            XCTAssertEqual(disconnect.elementType, .button)

            disconnect.tap()
            XCTAssertTrue(picker.waitForExistence(timeout: 5), "Cycle \(cycle) did not return to the picker")
            XCTAssertFalse(ride.exists, "Cycle \(cycle) left the previous Ride screen visible")
        }
    }

    func testCapturePassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility()
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
            ignoringSystemToolbarContrastWarning: true,
            ignoringSystemToolbarDynamicTypeWarning: true
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
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        captureKind.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.buttons["device-picker.capture-kind.done"].tap()
        XCTAssertTrue(captureKind.isHittable)
        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)
        restoreAdvancedCaptureViewport(advancedCapture, captureKind: captureKind)
        try performVisibleLayoutAccessibilityAudit(
            ignoringSystemToolbarContrastWarning: true,
            ignoringSystemToolbarDynamicTypeWarning: true
        )
    }

    func testAdvancedCaptureControlsRemainReachableInLandscapeAtAccessibilityDynamicType() throws {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        captureKind.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.buttons["device-picker.capture-kind.done"].tap()
        XCTAssertTrue(captureKind.isHittable)
        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)
        restoreAdvancedCaptureViewport(advancedCapture, captureKind: captureKind)
        try performVisibleLayoutAccessibilityAudit(
            ignoringSystemToolbarContrastWarning: true,
            ignoringSystemToolbarDynamicTypeWarning: true
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

    func testVescLiveActivityFixtureStartsFromAnAccessibleRide() throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20))
        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertFalse((speed.value as? String)?.isEmpty ?? true)
        // XCTest terminates the app in tearDown. Physical ActivityKit inspection
        // must launch the fixture with scripts/run-ios-app-on-phone.sh instead.
    }

    func testVescLiveActivityAutoFixtureStartsAnAccessibleRide() throws {
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20))
        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertFalse((speed.value as? String)?.isEmpty ?? true)
        // XCTest terminates the app in tearDown. Physical ActivityKit inspection
        // must launch the fixture with scripts/run-ios-app-on-phone.sh instead.
    }

    func testFailedVescConnectionReturnsToPickerInsteadOfLeavingRideRoute() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testFailedVescConnectionPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testFailedVescConnectionPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility(
            usesLocalizedText: true,
            auditExclusions: .contrast
        )
    }

    func testFailedVescConnectionPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertFailedVescConnectionAccessibility(
            usesLocalizedText: true,
            auditExclusions: []
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
            XCTAssertNotEqual(status.label, "Retrying connection…")
            XCTAssertFalse((status.value as? String)?.isEmpty ?? true)
        } else {
            let retrying = app.descendants(matching: .any).matching(
                NSPredicate(format: "label CONTAINS %@", "Retrying connection")
            ).firstMatch
            XCTAssertTrue(retrying.waitForExistence(timeout: 5))
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
        usesLocalizedText: Bool = false,
        auditExclusions: XCUIAccessibilityAuditType = [],
        ignoringNilElementContrastWarning: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))

        let connectionStatus = app.descendants(matching: .any)["device-picker.connection-status"]
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let connecting = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label BEGINSWITH %@", "Connecting"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [connecting], timeout: 3), .completed)
        let connectingLabel = connectionStatus.label
        restorePickerViewport(picker)
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )

        let failed = XCTNSPredicateExpectation(
            predicate: usesLocalizedText
                ? NSPredicate(format: "label != %@", connectingLabel)
                : NSPredicate(format: "label == %@", "Connect failed: deterministic fixture"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failed], timeout: 5), .completed)
        XCTAssertTrue(picker.exists)
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.screen.vescRide"].exists)
        XCTAssertFalse(connectionStatus.label.isEmpty)
        restorePickerViewport(picker)
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )

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
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )

    }

    func testEucRideAndBmsPassAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility()
    }

    func testEucBmsOverviewPassesAccessibilityAuditAtAccessibilityDynamicType() throws {
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
            ignoringNilElementContrastWarning: true
        )
    }

    func testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility(
            excluding: [],
            assertsEnglishMetric: false,
            ignoringNilElementContrastWarning: true
        )
    }

    func testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility()
    }

    func testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast() throws {
        try assertEucBmsDetailAccessibility(excluding: [])
    }

    func testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility(
            excluding: [],
            ignoringNilElementContrastWarning: true,
            ignoringClippedBmsGroupChildAuditWarnings: true
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
        try assertEucBmsDetailAccessibility(excluding: [])
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

    func testVescStaleTelemetryIsAnAccessibleWarningAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescStaleTelemetryIsAnAccessibleWarningInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescStaleTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility(usesLocalizedText: true)
    }

    func testVescStaleTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility(usesLocalizedText: true)
    }

    func testVescStaleTelemetryIsAnAccessibleWarningInRightToLeftLayout() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryIsAnAccessibleWarningAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility(usesLocalizedText: true)
    }

    func testEucStaleTelemetryIsAnAccessibleWarningInRightToLeftLayout() throws {
        try assertEucStaleTelemetryAccessibility(
            ignoringVisualProgressLabelContrastWarning: true,
            auditScrolls: 1
        )
    }

    private func assertEucStaleTelemetryAccessibility(
        usesLocalizedText: Bool = false,
        ignoringVisualProgressLabelContrastWarning: Bool = false,
        auditScrolls: Int = 0
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic stale EUC fixture did not open its Ride screen")
            return
        }

        let warning = app.descendants(matching: .any)["euc.warning"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
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
        if usesLocalizedText {
            XCTAssertFalse(status.label.isEmpty)
            XCTAssertFalse((status.value as? String ?? "").isEmpty)
        } else {
            XCTAssertTrue(status.label.contains("Telemetry stale"))
            XCTAssertEqual(status.value as? String, "warning")
        }
        for _ in 0..<auditScrolls {
            app.descendants(matching: .any)["dashboard.screen.eucRide"].swipeUp()
        }
        try performVisibleLayoutAccessibilityAudit(
            ignoringVisualProgressLabelContrastWarning: ignoringVisualProgressLabelContrastWarning
        )
    }

    func testVescPendingTelemetryIsAnAccessibleWarningAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    func testVescPendingTelemetryIsAnAccessibleWarningWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility(usesLocalizedText: true)
    }

    func testVescPendingTelemetryIsAnAccessibleWarningInRightToLeftLayout() throws {
        try assertVescPendingTelemetryAccessibility(auditScrolls: 1)
    }

    private func assertVescPendingTelemetryAccessibility(
        usesLocalizedText: Bool = false,
        auditScrolls: Int = 0
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
        for _ in 0..<auditScrolls {
            screen.swipeUp()
        }
        try performVisibleLayoutAccessibilityAudit()
    }

    private func assertVescStaleTelemetryAccessibility(
        usesLocalizedText: Bool = false,
        ignoringNilElementContrastWarning: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic stale VESC fixture did not open its Ride screen")
            return
        }

        let warning = app.descendants(matching: .any)["vesc.warning.telemetry-stale"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        let status = app.descendants(matching: .any)["ride.hero.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
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

    func testVescDutyHeadroomSpeaksPercentAtAccessibilityDynamicType() throws {
        try assertVescDutyHeadroomAccessibility(
            ignoringNilElementContrastWarning: true
        )
    }

    func testVescDutyHeadroomSpeaksPercentWithIncreasedContrastAtAccessibilityDynamicType() throws {
        try assertVescDutyHeadroomAccessibility(
            auditExclusions: [],
            ignoringNilElementContrastWarning: true
        )
    }

    private func assertVescDutyHeadroomAccessibility(
        auditExclusions: XCUIAccessibilityAuditType = [],
        ignoringNilElementContrastWarning: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        assertMetricIsReachable("Duty headroom", in: screen)
        let headroom = screen.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", "Duty headroom")
        ).firstMatch
        XCTAssertTrue(
            (headroom.value as? String)?.contains("77%") == true,
            "The VESC duty-headroom metric must speak its percent unit: \(String(describing: headroom.value))"
        )
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
    }

    func testVescRidePassesAccessibilityAuditAtExtraExtraExtraLargeType() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
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
            ignoringNilElementContrastWarning: true,
            expectsMirroredTabOrder: false
        )
    }

    func testVescDebugPassesAccessibilityAuditAtAccessibilityDynamicType() throws {
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
        if let requiredMetricLabel {
            assertMetricIsReachable(requiredMetricLabel, in: debugScreen)
        }
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions
        )
    }

    private var launchArguments: [String] {
        var arguments = fixture.launchArguments
        if name.contains("InLightAppearance") {
            arguments += ["-AppleInterfaceStyle", "Light"]
        }
        if name.contains("IncreasedContrast") {
            arguments += ["-UIAccessibilityDarkerSystemColorsEnabled", "YES"]
        }
        if name.contains("Pseudolocalized") {
            arguments += ["-NSDoubleLocalizedStrings", "YES"]
        }
        if name.contains("RightToLeft") {
            arguments += [
                "-AppleLanguages", "(ar)",
                "-AppleLocale", "ar_SA",
            ]
        }
        if name.contains("AccessibilityDynamicType") || name.contains("RightToLeft") {
            arguments += [
                "-UIPreferredContentSizeCategoryName",
                "UICTContentSizeCategoryAccessibilityXXXL",
            ]
        } else if name.contains("ExtraExtraExtraLarge") {
            arguments += [
                "-UIPreferredContentSizeCategoryName",
                "UICTContentSizeCategoryXXXL",
            ]
        }

        return arguments
    }

    private var isLandscapeTest: Bool {
        name.contains("InLandscape")
    }

    private var fixture: Fixture { Fixture.testFixture(for: name) }

    private enum Fixture {
        case unknownDevice
        case unknownDeviceFinishFailure
        case bluetoothUnavailable
        case bluetoothPermissionDenied
        case euc
        case eucStale
        case eucReconnect
        case eucOverview
        case eucNoBms
        case eucUnknownTopology
        case vesc
        case vescPending
        case vescStale
        case vescFailure
        case vescReconnect
        case vescConnecting
        case eucConnecting
        case vescLiveActivity
        case vescLiveActivityAuto

        static func testFixture(for testName: String) -> Self {
            if testName.contains("FinishCaptureFailure") { return .unknownDeviceFinishFailure }
            if testName.contains("Capture") || testName.contains("Advanced") { return .unknownDevice }
            if testName.contains("BluetoothUnavailable") { return .bluetoothUnavailable }
            if testName.contains("BluetoothPermissionDenied") { return .bluetoothPermissionDenied }
            if testName.contains("LiveActivityAutoFixture") { return .vescLiveActivityAuto }
            if testName.contains("LiveActivityFixture") { return .vescLiveActivity }
            if testName.contains("FailedVescConnection") { return .vescFailure }
            if testName.localizedCaseInsensitiveContains("EucUseShowsConnecting") { return .eucConnecting }
            if testName.contains("UseShowsConnecting") { return .vescConnecting }
            if testName.localizedCaseInsensitiveContains("EucStaleTelemetry") { return .eucStale }
            if testName.localizedCaseInsensitiveContains("EucReconnect") { return .eucReconnect }
            if testName.contains("Reconnect") { return .vescReconnect }
            if testName.contains("PendingTelemetry") { return .vescPending }
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
            case .bluetoothUnavailable: "bluetooth-unavailable"
            case .bluetoothPermissionDenied: "bluetooth-permission-denied"
            case .euc: "euc"
            case .eucStale: "euc-stale"
            case .eucReconnect: "euc-reconnect"
            case .eucOverview: "euc-overview"
            case .eucNoBms: "euc-no-bms"
            case .eucUnknownTopology: "euc-unknown-topology"
            case .vesc: "vesc"
            case .vescPending: "vesc-pending"
            case .vescStale: "vesc-stale"
            case .vescFailure: "vesc-failure"
            case .vescReconnect: "vesc-reconnect"
            case .vescConnecting: "vesc-connecting"
            case .eucConnecting: "euc-connecting"
            case .vescLiveActivity: "vesc-live-activity"
            case .vescLiveActivityAuto: "vesc-live-activity-auto"
            }
        }
    }

    private func skipLiveActivityTestsOnSimulator() throws {
        guard name.contains("LiveActivity"),
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
        exercisesLabels: Bool = true
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
        guard exercisesLabels else {
            try performVisibleLayoutAccessibilityAudit(excluding: excluded)
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

        restoreCaptureViewport(screen)
        XCTAssertTrue(stopCapture.isHittable)
        try performVisibleLayoutAccessibilityAudit(excluding: excluded)
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

    private func assertMetricIsReachable(_ label: String, in screen: XCUIElement) {
        let metric = screen.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", label)
        ).firstMatch
        let scrollView = screen.scrollViews.firstMatch
        let scrollTarget = scrollView.exists ? scrollView : screen

        if metric.exists {
            let hittableExpectation = XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "isHittable == true"),
                object: metric
            )
            if XCTWaiter.wait(for: [hittableExpectation], timeout: 2) == .completed {
                XCTAssertFalse(
                    (metric.value as? String)?.isEmpty ?? true,
                    "The \(label) metric has no accessible value"
                )
                return
            }
        }

        for index in 0..<6 where !metric.exists || !metric.isHittable {
            if index < 3 {
                let start = scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.62))
                let end = scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.28))
                start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
            } else {
                let start = scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.28))
                let end = scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.62))
                start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
            }
        }

        XCTAssertTrue(metric.exists, "The \(label) metric is missing at accessibility text sizes")
        XCTAssertTrue(metric.isHittable, "The \(label) metric cannot be reached by scrolling")
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
        maxScrolls: Int
    ) {
        for _ in 0..<maxScrolls where !screen.frame.contains(element.frame) {
            let isAboveViewport = element.frame.minY < screen.frame.minY
            let startY = isAboveViewport ? 0.28 : 0.72
            let endY = isAboveViewport ? 0.72 : 0.28
            let start = screen.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: startY))
            let end = screen.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: endY))
            start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
        }
        XCTAssertTrue(screen.frame.contains(element.frame), screen.debugDescription)
    }

    private func performVisibleLayoutAccessibilityAudit(
        excluding excluded: XCUIAccessibilityAuditType = [],
        ignoringSystemToolbarContrastWarning: Bool = false,
        ignoringSystemToolbarDynamicTypeWarning: Bool = false,
        ignoringNilElementContrastWarning: Bool = false,
        ignoringVisualProgressLabelContrastWarning: Bool = false,
        ignoringScrolledOutBmsDetailBackControlContrastWarning: Bool = false,
        ignoringVisibleBmsDetailBackControlContrastWarning: Bool = false,
        ignoringClippedBmsGroupChildAuditWarnings: Bool = false
    ) throws {
        continueAfterFailure = true
        defer { continueAfterFailure = false }
        let auditTypes = XCUIAccessibilityAuditType.all.subtracting(excluded)
        try app.performAccessibilityAudit(for: auditTypes) { issue in
            let elementDescription = issue.element?.debugDescription ?? "No element"
            print("Accessibility audit issue [\(issue.auditType.rawValue)]: \(issue.detailedDescription)\n\(elementDescription)")
            if ignoringSystemToolbarContrastWarning,
               issue.auditType == .contrast,
               [
                "device-picker.capture-kind.done",
                "device-picker.capture-kind.cancel",
               ].contains(issue.element?.identifier) {
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
            if ignoringVisualProgressLabelContrastWarning,
               issue.auditType == .contrast,
               issue.detailedDescription == "Contrast failed for SwiftUI.AccessibilityNode",
               issue.element?.label == "sag-adjusted energy" {
                // The progress bar already exposes this same typed label and
                // value on its parent. Xcode 27 reports its black-on-white
                // visual child only in this RTL simulator audit.
                return true
            }
            if ignoringScrolledOutBmsDetailBackControlContrastWarning,
               issue.auditType == .contrast,
               let element = issue.element,
               elementDescription.contains("identifier: 'bms.detail.back'"),
               !self.app.frame.contains(element.frame) {
                // The selected-detail audit intentionally scrolls this control
                // above the viewport before auditing the selected data. XCTest
                // cannot determine contrast for its clipped child text; visible
                // and all other contrast findings remain fatal.
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
            if ignoringClippedBmsGroupChildAuditWarnings,
               [.contrast, .dynamicType].contains(issue.auditType),
               let label = issue.element?.label,
               ["7", "12"].contains(label) {
                let detail = self.app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
                let group = self.app.buttons["bms.group.\(label)"]
                if detail.exists, group.exists, !detail.frame.contains(group.frame) {
                    // The row scales with Dynamic Type, but this landscape
                    // viewport clips its parent buttons while their numeral
                    // children remain in the audit region. Fully contained
                    // contrast and Dynamic Type findings remain fatal.
                    return true
                }
            }
            return false
        }
    }

    private func restorePickerViewport(_ picker: XCUIElement) {
        for _ in 0..<4 {
            picker.swipeDown()
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
        for _ in 0..<4 {
            scrollTarget.swipeDown()
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
        scrollElementIntoReachability(group, in: bmsScreen, maxScrolls: 20)

        XCTAssertTrue(group.waitForExistence(timeout: 5), bmsScreen.debugDescription)
        XCTAssertEqual(group.elementType, .button)
        XCTAssertTrue(group.isHittable, bmsScreen.debugDescription)
        return group
    }

    private func reachableCaptureAnnotation(_ id: String, in screen: XCUIElement) -> XCUIElement {
        let annotation = app.buttons["capture.label.\(id).action"]
        scrollElementIntoReachability(annotation, in: screen, maxScrolls: 20)

        XCTAssertTrue(annotation.waitForExistence(timeout: 5))
        XCTAssertTrue(annotation.isHittable, screen.debugDescription)
        return annotation
    }

    private func scrollElementIntoReachability(
        _ element: XCUIElement,
        in screen: XCUIElement,
        maxScrolls: Int
    ) {
        let scrollView = screen.scrollViews.firstMatch
        let scrollTarget = scrollView.exists ? scrollView : screen

        // Keep both drag endpoints above the persistent landscape tab bar.
        for _ in 0..<maxScrolls where !element.exists || !element.isHittable {
            if element.exists, element.frame.minY < screen.frame.minY {
                scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15))
                    .press(
                        forDuration: 0.05,
                        thenDragTo: scrollTarget.coordinate(
                            withNormalizedOffset: CGVector(dx: 0.5, dy: 0.65)
                        )
                    )
            } else {
                scrollTarget.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.65))
                    .press(
                        forDuration: 0.05,
                        thenDragTo: scrollTarget.coordinate(
                            withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)
                        )
                    )
            }
        }
    }

    private func restoreCaptureViewport(_ screen: XCUIElement) {
        let scrollView = screen.scrollViews.firstMatch
        let scrollTarget = scrollView.exists ? scrollView : screen
        for _ in 0..<6 {
            scrollTarget.swipeDown()
        }
    }

    private func assertEucBmsAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = [],
        assertsEnglishMetric: Bool = true,
        ignoringNilElementContrastWarning: Bool = false
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
        try performVisibleLayoutAccessibilityAudit(
            excluding: excluded,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
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
        ignoringNilElementContrastWarning: Bool = false,
        ignoringVisibleBmsDetailBackControlContrastWarning: Bool = false,
        ignoringClippedBmsGroupChildAuditWarnings: Bool = false
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = reachableBmsGroup(7, in: bmsScreen)
        group.tap()

        let detailScreen = app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 5))
        assertSelectedBmsGroupDetailIsReachable(in: detailScreen)
        try performVisibleLayoutAccessibilityAudit(
            excluding: excluded,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning,
            ignoringScrolledOutBmsDetailBackControlContrastWarning: true,
            ignoringVisibleBmsDetailBackControlContrastWarning: ignoringVisibleBmsDetailBackControlContrastWarning,
            ignoringClippedBmsGroupChildAuditWarnings: ignoringClippedBmsGroupChildAuditWarnings
        )
    }

    private func assertEucNoBmsSurface(
        auditExclusions: XCUIAccessibilityAuditType = [],
        ignoringNilElementContrastWarning: Bool = false
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsNoData"))
        defer { disconnectIfConnected() }

        let warning = bmsScreen.descendants(matching: .any)["bms.no-data.warning"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        XCTAssertFalse(warning.label.isEmpty)
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning
        )
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
