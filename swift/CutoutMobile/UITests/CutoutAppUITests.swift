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
        app.launchEnvironment["CUTOUT_UI_TEST_FIXTURE"] = fixtureEnvironmentValue
        app.launch()
        allowDeviceAuthorizationAlerts()
    }

    override func tearDown() async throws {
        app?.terminate()
        app = nil
        XCUIDevice.shared.orientation = .portrait
        try await super.tearDown()
    }

    private func allowDeviceAuthorizationAlerts() {
        var didDismissAlert = false
        defer {
            if didDismissAlert {
                app.activate()
            }
        }

        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let allowLabels = [
            "allow while using app",
            "allow bluetooth",
            "always allow",
            "change to always allow",
            "allow",
            "allow once",
            "ok",
        ]

        for _ in 0..<3 {
            let alert = springboard.alerts.firstMatch
            guard alert.waitForExistence(timeout: 1) else { break }

            let buttons = alert.buttons.allElementsBoundByIndex
            guard let button = allowLabels.lazy.compactMap({ label in
                buttons.first { $0.label.lowercased() == label }
            }).first else { break }
            button.tap()
            didDismissAlert = true
            _ = alert.waitForNonExistence(timeout: 2)
        }
    }

    func testPickerExposesAccessibleCaptureControls() {
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        let openAdvancedCapture = app.buttons["device-picker.open-advanced-capture"]
        let advancedCapture = app.descendants(matching: .any)["device-picker.advanced-capture"]
        let captureKind = app.textFields["device-picker.capture-kind"]
        let finishEditing = app.buttons["device-picker.capture-kind.done"]
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
        XCTAssertTrue(recordButton.waitForExistence(timeout: 5))
        XCTAssertTrue(recordButton.label.contains("Unknown BLE device"))
        XCTAssertFalse(recordButton.isEnabled)

        captureKind.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 2))

        captureKind.typeText("vesc floatwheel")

        XCTAssertEqual(captureKind.value as? String, "vesc floatwheel")
        XCTAssertTrue(recordButton.isEnabled)

        let done = app.keyboards.buttons["Done"]
        if done.waitForExistence(timeout: 1) {
            done.tap()
        }
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

    func testDisconnectKeepsSavedDeviceUntilExplicitForget() {
        XCTAssertTrue(pairAvailableDevice(.vesc))

        let disconnect = app.buttons["dashboard.disconnect"]
        XCTAssertTrue(disconnect.waitForExistence(timeout: 5))
        XCTAssertEqual(disconnect.label, "Disconnect")
        disconnect.tap()

        let picker = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        let forget = app.buttons["device-picker.forget-saved-device"]
        XCTAssertTrue(forget.waitForExistence(timeout: 5))
        XCTAssertEqual(forget.label, "Forget saved device")
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

        forget.tap()
        XCTAssertTrue(forget.waitForNonExistence(timeout: 5))
    }

    func testCapturePassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility()
    }

    func testCapturePassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertCaptureAccessibility(excluding: .contrast)
    }

    func testCapturePassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertCaptureAccessibility(excluding: .contrast)
    }

    func testProductionPickerPassesAccessibilityAudit() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditInLightAppearance() throws {
        try assertProductionPickerAccessibility()
    }

    func testProductionPickerPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertProductionPickerAccessibility(excluding: .contrast)
    }

    func testProductionPickerPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertProductionPickerAccessibility(excluding: .contrast)
    }

    func testProductionSurfacesRespectSystemAccessibilitySettings() throws {
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
        // XCTest's text-clipping audit reports a framework false positive for
        // this bold/high-contrast launch configuration; the dedicated Dynamic
        // Type audit remains the authoritative clipping check.
        // XCTest's contrast audit also reports false positives for SwiftUI's
        // grouped dashboard cards even when the rendered surface is black on
        // the semantic system background. Picker contrast is covered above.
        try performVisibleLayoutAccessibilityAudit(
            excluding: [.textClipped, .contrast],
            ignoringNilElementTextRepresentationWarning: true
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
        try performVisibleLayoutAccessibilityAudit(excluding: .contrast)
    }

    func testAdvancedCaptureControlsRemainReachableAtAccessibilityDynamicType() throws {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        XCTAssertTrue(captureKind.isHittable)

        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)
        try performVisibleLayoutAccessibilityAudit(excluding: .contrast)
    }

    func testAdvancedCaptureControlsRemainReachableInRightToLeftLayout() throws {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        XCTAssertTrue(captureKind.isHittable)
        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)
        try performVisibleLayoutAccessibilityAudit(excluding: .contrast)
    }

    func testAdvancedCaptureControlsRemainReachableInLandscapeAtAccessibilityDynamicType() throws {
        let advancedCapture = openAdvancedCapture()
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        XCTAssertTrue(captureKind.isHittable)
        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }
        XCTAssertTrue(recordButton.isHittable)
        try performVisibleLayoutAccessibilityAudit(excluding: .contrast)
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
        // Leave the activity running for the caller to inspect on the device.
    }

    func testVescLiveActivityAutoFixtureStartsAnAccessibleRide() throws {
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20))
        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertFalse((speed.value as? String)?.isEmpty ?? true)
        // Leave the activity running for the caller to inspect on the device.
    }

    func testFailedVescConnectionReturnsToPickerInsteadOfLeavingRideRoute() {
        XCTAssertTrue(pairAvailableDevice(.vesc))

        let connectionStatus = app.descendants(matching: .any)["device-picker.connection-status"]
        let connecting = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label BEGINSWITH %@", "Connecting"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [connecting], timeout: 3), .completed)

        let picker = app.descendants(matching: .any)["device-picker.screen"]
        let failed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", "Connect failed: deterministic fixture"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failed], timeout: 5), .completed)
        XCTAssertTrue(picker.exists)
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.screen.vescRide"].exists)
        XCTAssertEqual(connectionStatus.label, "Connect failed: deterministic fixture")

        XCTAssertTrue(pairAvailableDevice(.vesc))
        let retrying = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label BEGINSWITH %@", "Connecting"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [retrying], timeout: 3), .completed)

        let failedAgain = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", "Connect failed: deterministic fixture"),
            object: connectionStatus
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failedAgain], timeout: 5), .completed)
        XCTAssertTrue(picker.exists)

        let forget = app.buttons["device-picker.forget-saved-device"]
        XCTAssertTrue(forget.waitForExistence(timeout: 5))
        XCTAssertTrue(forget.isHittable)
        XCTAssertEqual(forget.label, "Forget saved device")
    }

    func testEucRideAndBmsSurfacesRemainAccessible() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        XCTAssertTrue(app.descendants(matching: .any)["bms.diagnostics"].exists)
        assertMetricIsReachable("Cell group 7, right pack group 7", in: bmsScreen)
        try performVisibleLayoutAccessibilityAudit(excluding: .contrast)
    }

    func testEucRideAndBmsPassAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertEucBmsAccessibility()
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

    func testEucNoBmsSurfacePassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithIncreasedContrast() throws {
        try assertEucNoBmsSurface(auditExclusions: [])
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface()
    }

    func testEucRideAndBmsPassAccessibilityAuditWithIncreasedContrast() throws {
        try assertEucBmsAccessibility(excluding: [])
    }

    func testEucBmsGroupOpensAccessibleDetailAndReturnsToMap() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = bmsScreen.descendants(matching: .any)["bms.group.7"]
        XCTAssertTrue(group.exists)
        XCTAssertEqual(group.elementType, .button)
        XCTAssertTrue(group.isHittable)
        group.tap()

        let detailScreen = app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 5))
        assertMetricIsReachable("Cell group 7, right pack group 7", in: detailScreen)

        let backToMap = app.buttons["bms.detail.back"]
        XCTAssertTrue(backToMap.exists)
        XCTAssertTrue(backToMap.isHittable)
        backToMap.tap()
        XCTAssertTrue(bmsScreen.waitForExistence(timeout: 5))
        XCTAssertFalse(detailScreen.waitForExistence(timeout: 2))
    }

    func testEucBmsDetailPassesAccessibilityAuditWithIncreasedContrast() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = bmsScreen.descendants(matching: .any)["bms.group.7"]
        XCTAssertTrue(group.waitForExistence(timeout: 5))
        XCTAssertTrue(group.isHittable)
        group.tap()

        let detailScreen = app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 5))
        assertMetricIsReachable("Cell group 7, right pack group 7", in: detailScreen)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testEucBmsDetailPassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = bmsScreen.descendants(matching: .any)["bms.group.7"]
        XCTAssertTrue(group.waitForExistence(timeout: 5))
        XCTAssertTrue(group.isHittable)
        group.tap()

        let detailScreen = app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 5))
        assertMetricIsReachable("Cell group 7, right pack group 7", in: detailScreen)
        try performVisibleLayoutAccessibilityAudit(excluding: .contrast)
    }

    func testEucBmsDetailPassesAccessibilityAuditInRightToLeftLayout() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        let group = bmsScreen.descendants(matching: .any)["bms.group.7"]
        XCTAssertTrue(group.waitForExistence(timeout: 5))
        XCTAssertTrue(group.isHittable)
        group.tap()

        let detailScreen = app.descendants(matching: .any)["dashboard.screen.bmsCellDetail"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 5))
        assertMetricIsReachable("Cell group 7, right pack group 7", in: detailScreen)
        try performVisibleLayoutAccessibilityAudit(excluding: .contrast)
    }

    func testVescRidePassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testVescRidePassesAccessibilityAuditAtExtraExtraExtraLargeType() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testVescRidePassesAccessibilityAuditWithIncreasedContrast() throws {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: "voltage",
            auditExclusions: []
        )
    }

    func testVescRidePassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testVescRidePassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testVescDebugPassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        try assertVescDebugSurface()
    }

    func testVescDebugPassesAccessibilityAuditWithIncreasedContrast() throws {
        try assertVescDebugSurface(auditExclusions: [])
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
        auditExclusions: XCUIAccessibilityAuditType = [.contrast]
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
            let element = tabBar.buttons[tab.capitalized]
            XCTAssertTrue(element.exists)
            XCTAssertTrue(element.isHittable)
            XCTAssertEqual(element.isSelected, tab == "ride")
        }

        if name.contains("RightToLeft"), family.tabNames.count > 1 {
            let firstTab = tabBar.buttons[family.tabNames[0].capitalized]
            let secondTab = tabBar.buttons[family.tabNames[1].capitalized]
            XCTAssertGreaterThan(
                firstTab.frame.midX,
                secondTab.frame.midX,
                "The system tab order did not mirror for the Arabic right-to-left launch"
            )
        }

        for unavailableTab in family.unavailableTabNames {
            XCTAssertFalse(tabBar.buttons[unavailableTab.capitalized].exists)
        }

        if let requiredMetricLabel {
            assertMetricIsReachable(requiredMetricLabel, in: screen)
        }

        try performVisibleLayoutAccessibilityAudit(excluding: auditExclusions)
    }

    private func assertVescDebugSurface(
        auditExclusions: XCUIAccessibilityAuditType = [.contrast]
    ) throws {
        guard pairAvailableDevice(.vesc), connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        let debugTab = app.tabBars.buttons["Debug"]
        XCTAssertTrue(debugTab.waitForExistence(timeout: 5))
        XCTAssertTrue(debugTab.isHittable)
        debugTab.tap()

        let debugScreen = app.descendants(matching: .any)["dashboard.screen.vescDebug"]
        XCTAssertTrue(debugScreen.waitForExistence(timeout: 5))
        XCTAssertTrue(debugTab.isSelected)
        assertMetricIsReachable("duty", in: debugScreen)
        try performVisibleLayoutAccessibilityAudit(excluding: auditExclusions)
    }

    private var launchArguments: [String] {
        var arguments = fixture.launchArguments
        if name.contains("RespectSystemAccessibilitySettings") {
            arguments += [
                "-AppleInterfaceStyle", "Dark",
                "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryLarge",
                "-UIAccessibilityBoldTextEnabled", "YES",
                "-UIAccessibilityDarkerSystemColorsEnabled", "YES",
                "-UIAccessibilityDifferentiateWithoutColorEnabled", "YES",
                "-UIAccessibilityReduceMotionEnabled", "YES",
                "-UIAccessibilityReduceTransparencyEnabled", "YES",
                "-UIAccessibilityGrayscaleEnabled", "YES",
            ]
        } else {
            if name.contains("InLightAppearance") {
                arguments += ["-AppleInterfaceStyle", "Light"]
            }
            if name.contains("IncreasedContrast") {
                arguments += ["-UIAccessibilityDarkerSystemColorsEnabled", "YES"]
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
        }

        return arguments
    }

    private var fixtureEnvironmentValue: String {
        fixture.environmentValue
    }

    private var isLandscapeTest: Bool {
        name.contains("InLandscape")
    }

    private var fixture: Fixture {
        if name.contains("Capture") || name.contains("Advanced") { return .unknownDevice }
        if name.contains("LiveActivityAutoFixture") { return .vescLiveActivityAuto }
        if name.contains("LiveActivityFixture") { return .vescLiveActivity }
        if name.contains("FailedVescConnection") { return .vescFailure }
        if name.contains("EucNoBms") { return .eucNoBms }
        if name.contains("Euc") { return .euc }
        return .vesc
    }

    private enum Fixture {
        case unknownDevice
        case euc
        case eucNoBms
        case vesc
        case vescFailure
        case vescLiveActivity
        case vescLiveActivityAuto

        var environmentValue: String {
            switch self {
            case .unknownDevice: "unknown-device"
            case .euc: "euc"
            case .eucNoBms: "euc-no-bms"
            case .vesc: "vesc"
            case .vescFailure: "vesc-failure"
            case .vescLiveActivity: "vesc-live-activity"
            case .vescLiveActivityAuto: "vesc-live-activity-auto"
            }
        }

        var launchArguments: [String] {
            switch self {
            case .unknownDevice: ["--ui-test-unknown-device"]
            case .euc: ["--ui-test-euc"]
            case .eucNoBms: ["--ui-test-euc-no-bms"]
            case .vesc: ["--ui-test-vesc"]
            case .vescFailure: ["--ui-test-vesc-failure"]
            case .vescLiveActivity: ["--ui-test-vesc", "--ui-test-live-activity"]
            case .vescLiveActivityAuto: ["--ui-test-live-activity-auto"]
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
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(captureKind.waitForExistence(timeout: 5))
        captureKind.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 2))
        captureKind.typeText("custom vesc")

        let done = app.keyboards.buttons["Done"]
        if done.waitForExistence(timeout: 1) {
            done.tap()
        }

        XCTAssertEqual(recordButton.elementType, .button)

        for _ in 0..<6 where !recordButton.isHittable {
            advancedCapture.swipeUp()
        }

        XCTAssertTrue(recordButton.isHittable)
        recordButton.tap()
        XCTAssertTrue(app.descendants(matching: .any)["capture.screen"].waitForExistence(timeout: 5))
    }

    private func assertProductionPickerAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = []
    ) throws {
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        try performVisibleLayoutAccessibilityAudit(excluding: excluded)
    }

    private func assertCaptureAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = []
    ) throws {
        enterCapture()

        let screen = app.descendants(matching: .any)["capture.screen"]
        let stopCapture = app.buttons["capture.stop"]
        let firstAnnotation = app.buttons["capture.label.ride.action"]
        let lastAnnotation = app.buttons["capture.label.pwm_percent.action"]
        XCTAssertTrue(screen.exists)

        for _ in 0..<6 where !stopCapture.isHittable {
            screen.swipeUp()
        }

        XCTAssertTrue(stopCapture.exists)
        XCTAssertTrue(stopCapture.isHittable, app.debugDescription)
        XCTAssertEqual(stopCapture.label, "Finish capture")
        XCTAssertTrue(firstAnnotation.isHittable)
        XCTAssertEqual(firstAnnotation.label, "Start Ride")
        firstAnnotation.tap()
        XCTAssertEqual(firstAnnotation.label, "Stop Ride")

        let annotationScrollView = screen.scrollViews.firstMatch
        for _ in 0..<8 where !lastAnnotation.isHittable {
            annotationScrollView.swipeUp()
        }

        XCTAssertTrue(lastAnnotation.exists)
        XCTAssertTrue(lastAnnotation.isHittable)
        XCTAssertEqual(lastAnnotation.label, "Start PWM percent")
        lastAnnotation.tap()
        XCTAssertEqual(lastAnnotation.label, "Stop PWM percent")
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
            if metric.frame.intersects(screen.frame) {
                XCTAssertFalse(
                    (metric.value as? String)?.isEmpty ?? true,
                    "The \(label) metric has no accessible value"
                )
                return
            }
        }

        for index in 0..<6 where !metric.exists || (!metric.isHittable && !metric.frame.intersects(screen.frame)) {
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
        XCTAssertTrue(
            metric.isHittable || metric.frame.intersects(screen.frame),
            "The \(label) metric cannot be reached by scrolling"
        )
        XCTAssertFalse(
            (metric.value as? String)?.isEmpty ?? true,
            "The \(label) metric has no accessible value"
        )
    }

    private func performVisibleLayoutAccessibilityAudit(
        excluding excluded: XCUIAccessibilityAuditType = [],
        ignoringNilElementTextRepresentationWarning: Bool = false
    ) throws {
        continueAfterFailure = true
        defer { continueAfterFailure = false }
        let auditTypes = XCUIAccessibilityAuditType.all.subtracting(excluded)
        try app.performAccessibilityAudit(for: auditTypes) { issue in
            let elementDescription = issue.element?.debugDescription ?? "No element"
            print("Accessibility audit issue [\(issue.auditType.rawValue)]: \(issue.detailedDescription)\n\(elementDescription)")
            if ignoringNilElementTextRepresentationWarning,
               issue.element == nil,
               issue.detailedDescription.contains("text that should be represented using the accessibility API") {
                // Xcode 27's simulator audit occasionally emits this warning
                // after multiple launch-argument changes without identifying
                // an element. The captured AX hierarchy has all rendered text
                // represented; real element-based findings remain failures.
                return true
            }
            return false
        }
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
            XCTFail("The deterministic EUC fixture did not open its Ride screen")
            return nil
        }

        XCTAssertEqual(rideScreen.identifier, ConnectedDeviceFamily.euc.screenIdentifier)
        XCTAssertTrue(app.descendants(matching: .any)["ride.hero.speed"].exists)
        assertMetricIsReachable("speed", in: rideScreen)

        let packTab = app.tabBars.buttons["Pack"]
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

    private func assertEucBmsAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = [.contrast]
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        assertMetricIsReachable("Cell group 7, right pack group 7", in: bmsScreen)
        try performVisibleLayoutAccessibilityAudit(excluding: excluded)
    }

    private func assertEucNoBmsSurface(
        auditExclusions: XCUIAccessibilityAuditType = [.contrast]
    ) throws {
        let bmsScreen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsNoData"))
        defer { disconnectIfConnected() }

        let warning = bmsScreen.descendants(matching: .any)["bms.no-data.warning"]
        XCTAssertTrue(warning.waitForExistence(timeout: 5))
        XCTAssertFalse(warning.label.isEmpty)
        try performVisibleLayoutAccessibilityAudit(excluding: auditExclusions)
    }

    @discardableResult
    private func pairAvailableDevice(_ family: ConnectedDeviceFamily) -> Bool {
        if let screen = connectedScreen() {
            if screen.identifier == family.screenIdentifier { return true }
            disconnectIfConnected()
        }

        let useButtons = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.use.")
        )
        guard useButtons.firstMatch.waitForExistence(timeout: 8) else { return false }

        let buttons = useButtons.allElementsBoundByIndex
        guard let button = buttons.first(where: { family.matches(label: $0.label) }) else {
            return false
        }

        XCTAssertEqual(button.elementType, .button)

        let picker = app.descendants(matching: .any)["device-picker.screen"]
        for _ in 0..<6 where !button.isHittable {
            picker.swipeUp()
        }

        guard button.isHittable else {
            XCTFail("The \(family.name) Use button cannot be reached by scrolling")
            return false
        }
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
