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

        let bottomBars = app.descendants(matching: .any).matching(identifier: "dashboard.bottom.navigation")
        XCTAssertEqual(bottomBars.count, 1)
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.top.navigation"].exists)

        let window = app.windows.firstMatch
        let bottomBar = bottomBars.element
        XCTAssertLessThanOrEqual(bottomBar.frame.maxY, window.frame.maxY + 2)
        XCTAssertGreaterThan(bottomBar.frame.height, 0)

        for tab in family.tabNames {
            let element = app.descendants(matching: .any)["dashboard.nav.\(tab)"]
            XCTAssertTrue(element.exists)
            XCTAssertFalse(element.frame.isEmpty)
            XCTAssertGreaterThan(element.frame.width, 0)
            XCTAssertGreaterThan(element.frame.height, 0)

            let expectedValue = tab == "ride"
                ? "Selected"
                : ["map", "logs", "tune"].contains(tab) ? "Unavailable" : "Available"
            XCTAssertEqual(element.value as? String, expectedValue)
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
        case .euc: ["ride", "pack", "map", "tune"]
        case .vesc: ["ride", "debug", "map", "logs"]
        }
    }

    func matches(label: String) -> Bool {
        let label = label.lowercased()
        let isVesc = label.contains("vesc") || label.contains("refloat")
            || label.contains("onewheel") || label.contains("floatwheel")
        return self == .vesc ? isVesc : !isVesc
    }
}
