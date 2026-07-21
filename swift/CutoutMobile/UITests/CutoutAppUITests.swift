import XCTest

final class CutoutAppUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
        app = XCUIApplication()
        app.launch()
        allowDeviceAuthorizationAlerts()
    }

    override func tearDown() {
        app?.terminate()
        app = nil
        super.tearDown()
    }

    private func allowDeviceAuthorizationAlerts() {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let allowLabels = Set([
            "allow",
            "allow once",
            "allow while using app",
            "allow bluetooth",
            "always allow",
            "change to always allow",
            "ok",
        ])

        for _ in 0..<3 {
            let alert = springboard.alerts.firstMatch
            guard alert.waitForExistence(timeout: 3) else { return }

            guard let button = alert.buttons.allElementsBoundByIndex.first(where: {
                allowLabels.contains($0.label.lowercased())
            }) else {
                return
            }
            button.tap()
            _ = alert.waitForNonExistence(timeout: 2)
        }
    }

    func testPickerExposesAccessibleCaptureControls() {
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        let captureKind = app.textFields["device-picker.capture-kind"]
        let recordButton = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.record.")
        ).firstMatch

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        XCTAssertTrue(captureKind.exists)
        XCTAssertEqual(captureKind.label, "Device kind for capture")
        XCTAssertTrue(captureKind.isHittable)
        XCTAssertTrue(recordButton.waitForExistence(timeout: 5))
        XCTAssertFalse(recordButton.isEnabled)

        captureKind.tap()
        captureKind.typeText("vesc floatwheel")

        XCTAssertEqual(captureKind.value as? String, "vesc floatwheel")
        XCTAssertTrue(recordButton.isEnabled)
    }

    func testProductionPickerPassesAccessibilityAudit() throws {
        let screen = app.descendants(matching: .any)["device-picker.screen"]

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        try performCompleteAccessibilityAudit()
    }

    func testPickerSurfaceRemainsReachableAtAccessibilityDynamicType() {
        relaunchAtAccessibilityDynamicType()

        let window = app.windows.firstMatch
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        let captureKind = app.textFields["device-picker.capture-kind"]

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        XCTAssertTrue(captureKind.exists)
        XCTAssertFalse(window.frame.isEmpty)
        XCTAssertFalse(screen.frame.isEmpty)
        XCTAssertFalse(captureKind.frame.isEmpty)
        XCTAssertGreaterThan(captureKind.frame.width, 0)
        XCTAssertGreaterThan(captureKind.frame.height, 0)
        XCTAssertGreaterThanOrEqual(screen.frame.minY, window.frame.minY - 2)
        XCTAssertLessThanOrEqual(screen.frame.maxY, window.frame.maxY + 2)

        for _ in 0..<4 where captureKind.frame.maxY > window.frame.maxY {
            screen.swipeUp()
        }

        XCTAssertTrue(captureKind.isHittable)
        XCTAssertGreaterThanOrEqual(captureKind.frame.minY, window.frame.minY - 2)
        XCTAssertLessThanOrEqual(captureKind.frame.maxY, window.frame.maxY + 2)
    }

    func testVescUseOpensAnAccessibleLiveRide() throws {
        try assertConnectedSurface(for: .vesc)
    }

    func testVescRidePassesAccessibilityAuditAtAccessibilityDynamicType() throws {
        relaunchAtAccessibilityDynamicType()
        try assertConnectedSurface(for: .vesc, requiredMetricLabel: "voltage")
    }

    func testEucUseOpensAnAccessibleLiveRide() throws {
        try assertConnectedSurface(for: .euc)
    }

    private func assertConnectedSurface(
        for family: ConnectedDeviceFamily,
        requiredMetricLabel: String? = nil
    ) throws {
        let pairingAttempted = pairAvailableDevice(family)

        guard pairingAttempted else {
            throw XCTSkip("Requires an advertising \(family.name) device")
        }
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The visible \(family.name) Use button was tapped, but no connected dashboard appeared")
            return
        }
        defer { disconnectIfConnected() }
        XCTAssertEqual(screen.identifier, family.screenIdentifier)
        XCTAssertFalse(app.descendants(matching: .any)["device-picker.screen"].exists)

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

        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(tabBar.exists)
        XCTAssertEqual(app.tabBars.count, 1)
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.top.navigation"].exists)

        for tab in family.tabNames {
            let element = app.descendants(matching: .any)["dashboard.nav.\(tab)"]
            XCTAssertTrue(element.exists)
            XCTAssertTrue(element.isHittable)
            XCTAssertEqual(element.isSelected, tab == "ride")
        }

        for unavailableTab in family.unavailableTabNames {
            XCTAssertFalse(app.descendants(matching: .any)["dashboard.nav.\(unavailableTab)"].exists)
        }

        if let requiredMetricLabel {
            assertMetricIsReachable(requiredMetricLabel, in: screen)
        }

        try performCompleteAccessibilityAudit()
    }

    private func relaunchAtAccessibilityDynamicType() {
        app.terminate()
        app.launchArguments = [
            "-UIPreferredContentSizeCategoryName",
            "UICTContentSizeCategoryAccessibilityXXXL",
        ]
        app.launch()
        allowDeviceAuthorizationAlerts()
    }

    private func assertMetricIsReachable(_ label: String, in screen: XCUIElement) {
        let metric = app.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", label)
        ).firstMatch

        for _ in 0..<6 where !metric.exists || !metric.isHittable {
            screen.swipeUp()
        }

        XCTAssertTrue(metric.exists, "The \(label) metric is missing at accessibility text sizes")
        XCTAssertTrue(metric.isHittable, "The \(label) metric cannot be reached by scrolling")
        XCTAssertFalse(
            (metric.value as? String)?.isEmpty ?? true,
            "The \(label) metric has no accessible value"
        )
    }

    private func performCompleteAccessibilityAudit() throws {
        continueAfterFailure = true
        defer { continueAfterFailure = false }
        try app.performAccessibilityAudit()
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

    @discardableResult
    private func pairAvailableDevice(_ family: ConnectedDeviceFamily) -> Bool {
        if let screen = connectedScreen() {
            if screen.identifier == family.screenIdentifier { return true }
            disconnectIfConnected()
        }

        let useButtons = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.use.")
        )
        guard useButtons.firstMatch.waitForExistence(timeout: 8) else { return false }

        let buttons = useButtons.allElementsBoundByIndex
        guard let button = buttons.first(where: { family.matches(label: $0.label) }) else {
            return false
        }
        guard button.isHittable else {
            XCTFail("The visible \(family.name) Use button is not hittable")
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
