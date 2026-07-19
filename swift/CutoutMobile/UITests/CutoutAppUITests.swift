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
        let screen = app.otherElements["device-picker.screen"]
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
        XCTAssertLessThanOrEqual(captureKind.frame.maxY, window.frame.maxY + 2)
    }

    func testPickerSurfaceRemainsReachableAtAccessibilityDynamicType() {
        app.terminate()
        app.launchArguments = [
            "-UIPreferredContentSizeCategoryName",
            "UICTContentSizeCategoryAccessibilityXXXL",
        ]
        app.launch()

        let window = app.windows.firstMatch
        let screen = app.otherElements["device-picker.screen"]
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
        XCTAssertLessThanOrEqual(captureKind.frame.maxY, window.frame.maxY + 2)
    }

    func testConnectedSurfaceHasOneCanonicalBottomNavigation() throws {
        let pairingAttempted = pairAvailableVescIfNeeded()

        guard let screen = connectedScreen(timeout: 20) else {
            if pairingAttempted {
                XCTFail("The visible Use button was tapped, but no connected dashboard appeared")
                return
            }
            throw XCTSkip("Requires a connected EUC or VESC device")
        }
        defer { disconnectIfConnected() }

        let bottomBars = app.otherElements.matching(identifier: "dashboard.bottom.navigation")
        XCTAssertEqual(bottomBars.count, 1)
        XCTAssertTrue(app.otherElements["dashboard.top.navigation"].exists)

        let window = app.windows.firstMatch
        let bottomBar = bottomBars.element
        XCTAssertLessThanOrEqual(bottomBar.frame.maxY, window.frame.maxY + 2)
        XCTAssertGreaterThan(bottomBar.frame.height, 0)

        let tabNames = screen.identifier == "dashboard.screen.vescRide"
            ? ["ride", "debug", "map", "logs"]
            : ["ride", "pack", "map", "tune"]
        let selectedTab = screen.identifier == "dashboard.screen.vescDebug" ? "debug" : "ride"
        for tab in tabNames {
            let element = screen.descendants(matching: .any)["dashboard.nav.\(tab)"]
            XCTAssertTrue(element.exists)
            XCTAssertFalse(element.frame.isEmpty)
            XCTAssertGreaterThan(element.frame.width, 0)
            XCTAssertGreaterThan(element.frame.height, 0)

            let expectedValue = tab == selectedTab
                ? "Selected"
                : ["map", "logs", "tune"].contains(tab) ? "Unavailable" : "Available"
            XCTAssertEqual(element.value as? String, expectedValue)
        }
    }

    private func connectedScreen(timeout: TimeInterval = 2) -> XCUIElement? {
        for screenID in ["dashboard.screen.eucRide", "dashboard.screen.vescRide", "dashboard.screen.vescDebug"] {
            let screen = app.otherElements[screenID]
            if screen.waitForExistence(timeout: timeout) {
                return screen
            }
        }
        return nil
    }

    private func disconnectIfConnected() {
        let disconnect = app.buttons["dashboard.disconnect"]
        guard disconnect.waitForExistence(timeout: 2) else { return }
        disconnect.tap()
        _ = app.otherElements["device-picker.screen"].waitForExistence(timeout: 5)
    }

    @discardableResult
    private func pairAvailableVescIfNeeded() -> Bool {
        guard connectedScreen() == nil else { return true }

        let useButtons = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "device-picker.use.")
        )
        guard useButtons.firstMatch.waitForExistence(timeout: 8) else { return false }

        let buttons = useButtons.allElementsBoundByIndex
        let vescButton = buttons.first {
            let label = $0.label.lowercased()
            return label.contains("vesc") || label.contains("refloat") || label.contains("onewheel")
        }
        guard let button = vescButton ?? (buttons.count == 1 ? buttons.first : nil) else {
            return false
        }
        guard button.isHittable else {
            XCTFail("The visible Use button is not hittable")
            return false
        }
        button.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        return true
    }
}
