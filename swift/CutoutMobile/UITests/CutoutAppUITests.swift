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

    func testProductionPickerSurfaceIsAccessible() {
        let window = app.windows.firstMatch
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        let captureKind = app.textFields["device-picker.capture-kind"]

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        XCTAssertTrue(captureKind.exists)
        XCTAssertEqual(captureKind.label, "Device kind for capture")
        XCTAssertFalse(window.frame.isEmpty)
        XCTAssertFalse(screen.frame.isEmpty)
        XCTAssertFalse(captureKind.frame.isEmpty)
        XCTAssertGreaterThan(captureKind.frame.width, 0)
        XCTAssertGreaterThan(captureKind.frame.height, 0)
        XCTAssertEqual(captureKind.frame.minX - window.frame.minX, 24, accuracy: 2)
        XCTAssertEqual(window.frame.maxX - captureKind.frame.maxX, 24, accuracy: 2)
        XCTAssertGreaterThanOrEqual(screen.frame.minY, window.frame.minY - 2)
        XCTAssertLessThanOrEqual(screen.frame.maxY, window.frame.maxY + 2)

        for _ in 0..<4 where captureKind.frame.maxY > window.frame.maxY {
            screen.swipeUp()
        }

        XCTAssertTrue(captureKind.isHittable)
        XCTAssertGreaterThanOrEqual(captureKind.frame.minY, window.frame.minY - 2)
        XCTAssertLessThanOrEqual(captureKind.frame.maxY, window.frame.maxY + 2)
    }

    func testProductionPickerPassesAccessibilityAudit() throws {
        let screen = app.descendants(matching: .any)["device-picker.screen"]

        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        try app.performAccessibilityAudit()
    }

    func testPickerSurfaceRemainsReachableAtAccessibilityDynamicType() {
        app.terminate()
        app.launchArguments = [
            "-UIPreferredContentSizeCategoryName",
            "UICTContentSizeCategoryAccessibilityXXXL",
        ]
        app.launch()

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

    func testConnectedVescSurfaceHasOneCanonicalBottomNavigation() throws {
        try assertConnectedSurface(for: .vesc)
    }

    func testVescLiveFixtureRendersTheProductionDashboard() {
        app.terminate()
        app.launchArguments = [
            "--ui-test-vesc-live",
            "-UIPreferredContentSizeCategoryName",
            "UICTContentSizeCategoryL",
        ]
        app.launch()

        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        XCTAssertEqual(app.descendants(matching: .any)["ride.hero.speed"].value as? String, "19, mph")
        XCTAssertTrue(app.buttons["dashboard.disconnect"].exists)
        XCTAssertEqual(app.buttons["dashboard.disconnect"].label, "Disconnect and choose device")
        XCTAssertTrue(app.staticTexts["Fungineers X7"].exists)
        XCTAssertTrue(app.staticTexts["VESC OW · armed"].exists)
        XCTAssertTrue(app.staticTexts["Duty headroom"].exists)
        XCTAssertTrue(app.staticTexts["18%"].exists)
        XCTAssertTrue(app.staticTexts["Pushback soon"].exists)
        XCTAssertEqual(metric(named: "battery current").value as? String, "38, A, discharging")
        XCTAssertEqual(metric(named: "motor current").value as? String, "71, A, phase current")
        XCTAssertEqual(metric(named: "board angle").value as? String, "-1.8, °, nose down")
        XCTAssertEqual(metric(named: "controller").value as? String, "54, °C, motor 49 °C")
        XCTAssertEqual(app.tabBars.count, 1)
    }

    func testConnectedEucSurfaceHasOneCanonicalBottomNavigation() throws {
        try assertConnectedSurface(for: .euc)
    }

    private func assertConnectedSurface(for family: ConnectedDeviceFamily) throws {
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

        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.exists)
        XCTAssertNotEqual(speed.value as? String, "--")
        XCTAssertFalse((speed.value as? String ?? "").isEmpty)

        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(tabBar.exists)
        XCTAssertEqual(app.tabBars.count, 1)
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.top.navigation"].exists)

        let window = app.windows.firstMatch
        XCTAssertLessThanOrEqual(tabBar.frame.maxY, window.frame.maxY + 2)
        XCTAssertGreaterThan(tabBar.frame.height, 0)

        for tab in family.tabNames {
            let element = app.descendants(matching: .any)["dashboard.nav.\(tab)"]
            XCTAssertTrue(element.exists)
            XCTAssertFalse(element.frame.isEmpty)
            XCTAssertGreaterThan(element.frame.width, 0)
            XCTAssertGreaterThan(element.frame.height, 0)
            XCTAssertEqual(element.isSelected, tab == "ride")
        }

        for unavailableTab in family.unavailableTabNames {
            XCTAssertFalse(app.descendants(matching: .any)["dashboard.nav.\(unavailableTab)"].exists)
        }
    }

    private func connectedScreen(timeout: TimeInterval = 2) -> XCUIElement? {
        let screen = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "dashboard.screen.")
        ).firstMatch
        return screen.waitForExistence(timeout: timeout) ? screen : nil
    }

    private func metric(named label: String) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", label)
        ).firstMatch
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
        button.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
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
