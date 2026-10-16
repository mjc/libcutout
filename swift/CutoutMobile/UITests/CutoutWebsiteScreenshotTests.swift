import XCTest

@MainActor
final class CutoutWebsiteScreenshotTests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() async throws {
        #if !targetEnvironment(simulator)
            throw XCTSkip("Website screenshot tests are restricted to Simulator")
        #endif
        try await super.setUp()
        continueAfterFailure = false
        executionTimeAllowance = 180
        XCUIDevice.shared.orientation = .portrait
    }

    override func tearDown() async throws {
        app?.terminate()
        app = nil
        try await super.tearDown()
    }

    func testEucMain() throws {
        launch(scene: "euc-main")
        connect(fixture: "euc")

        let ride = element("dashboard.screen.eucRide")
        XCTAssertTrue(ride.waitForExistence(timeout: 20), app.debugDescription)
        let speed = element("ride.hero.speed")
        XCTAssertTrue(speed.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse((speed.value as? String ?? "").isEmpty, app.debugDescription)
        let player = app.buttons["music.expand"]
        XCTAssertTrue(player.waitForExistence(timeout: 10), app.debugDescription)
        let summary = player.value as? String ?? ""
        XCTAssertTrue(summary.contains("Dusk on Colfax"), app.debugDescription)
        XCTAssertTrue(summary.contains("Mira Sol"), app.debugDescription)
        attachScreenshot(named: "euc-main")
    }

    func testRgbControls() throws {
        launch(scene: "rgb", extraArguments: ["--ui-test-connected-lighting"])
        connect(fixture: "euc")
        selectTab("lighting", title: "Lighting")

        XCTAssertTrue(element("dashboard.screen.lighting").waitForExistence(timeout: 10), app.debugDescription)
        let connection = element("lighting.connection-state")
        XCTAssertTrue(connection.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(connection.label.localizedCaseInsensitiveContains("MELK-OC21"), app.debugDescription)
        XCTAssertTrue(element("lighting.color-wheel").waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(element("lighting.brightness").waitForExistence(timeout: 5), app.debugDescription)

        let power = app.switches["lighting.power"]
        XCTAssertTrue(power.waitForExistence(timeout: 5), app.debugDescription)
        if (power.value as? String) != "1" { power.tap() }
        let poweredOn = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == '1'"), object: power)
        XCTAssertEqual(XCTWaiter.wait(for: [poweredOn], timeout: 5), .completed, app.debugDescription)

        let hue = app.sliders.matching(NSPredicate(format: "label == %@", "Hue")).firstMatch
        let saturation = app.sliders.matching(NSPredicate(format: "label == %@", "Saturation")).firstMatch
        XCTAssertTrue(hue.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(saturation.waitForExistence(timeout: 5), app.debugDescription)
        hue.adjust(toNormalizedSliderPosition: 0.54)
        saturation.adjust(toNormalizedSliderPosition: 0.72)
        app.sliders["lighting.brightness"].adjust(toNormalizedSliderPosition: 0.72)
        attachScreenshot(named: "rgb")
    }

    func testTune() throws {
        launch(scene: "tune", extraArguments: ["-CUTOUT_UI_TEST_SETTINGS", "YES"])
        connect(fixture: "euc")
        selectTab("tune", title: "Tune")

        XCTAssertTrue(element("settings.screen.eucTune").waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.buttons["settings.on.highBeam"].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.buttons["settings.open-alarms"].exists, app.debugDescription)
        attachScreenshot(named: "tune")
    }

    func testRideTracking() throws {
        launch(scene: "ride-tracking")
        connect(fixture: "euc")

        XCTAssertTrue(element("dashboard.screen.eucRide").waitForExistence(timeout: 20), app.debugDescription)
        selectTab("map", title: "Map")
        XCTAssertTrue(element("ride-map.screen").waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(element("ride-map.recording-pill").waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertTrue(element("ride-map.map").waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(element("ride-map.latest-point-marker").waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(element("ride-map.summary").waitForExistence(timeout: 10), app.debugDescription)
        attachScreenshot(named: "ride-tracking")
    }

    func testChargingMap() throws {
        launch(scene: "charging-map")
        connect(fixture: "euc")
        selectTab("map", title: "Map")

        XCTAssertTrue(element("ride-map.screen").waitForExistence(timeout: 10), app.debugDescription)
        let modePicker = element("ride-map.mode-picker").segmentedControls.firstMatch
        XCTAssertTrue(modePicker.waitForExistence(timeout: 5), app.debugDescription)
        let chargingMode = modePicker.buttons["Charging"]
        XCTAssertTrue(chargingMode.waitForExistence(timeout: 5), app.debugDescription)
        chargingMode.tap()

        let demoPins = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "charging.pin.")
        )
        let seededSpotsLoaded = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in demoPins.count >= 3 }, object: app)
        XCTAssertEqual(XCTWaiter.wait(for: [seededSpotsLoaded], timeout: 25), .completed, app.debugDescription)
        for prefix in ["Juniper Corner Library", "Bluebird Coffee Room", "Riverglass Charge Stop"] {
            XCTAssertTrue(
                app.buttons.matching(
                    NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "charging.pin.", prefix)
                ).firstMatch.waitForExistence(timeout: 5),
                app.debugDescription
            )
        }
        attachScreenshot(named: "charging-map")
    }

    private func launch(scene: String, extraArguments: [String] = []) {
        app = XCUIApplication()
        app.launchArguments =
            [
                "-CUTOUT_UI_TEST_FIXTURE", "euc",
                "-CUTOUT_WEBSITE_SCREENSHOT", scene,
                "-AppleLanguages", "(en)",
                "-AppleLocale", "en_US",
                "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryL",
                "-io.cutout.music.provider.selected", "apple_music",
                "-io.cutout.music.monitoring.enabled", scene == "euc-main" ? "YES" : "NO",
                "-io.cutout.music.compact-player.hidden", "NO",
            ] + extraArguments
        app.launchEnvironment["CUTOUT_UI_TEST_FIXTURE"] = "euc"
        app.launchEnvironment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"] = UUID().uuidString
        app.launch()
        XCTAssertTrue(element("device-picker.screen").waitForExistence(timeout: 10), app.debugDescription)
    }

    private func connect(fixture: String) {
        let picker = element("device-picker.screen")
        let use = app.buttons["device-picker.use.ui-test-\(fixture)"]
        XCTAssertTrue(use.waitForExistence(timeout: 10), app.debugDescription)
        for _ in 0..<12 where !use.isHittable {
            picker.swipeUp()
        }
        XCTAssertTrue(use.isHittable, app.debugDescription)
        use.tap()
    }

    private func selectTab(_ id: String, title: String) {
        let tab = navigationTab(id, title: title)
        if tab.waitForExistence(timeout: 5) {
            XCTAssertTrue(tab.isHittable, app.debugDescription)
            tab.tap()
            return
        }

        let more = navigationTab("more", title: "More")
        XCTAssertTrue(more.waitForExistence(timeout: 5), app.debugDescription)
        more.tap()
        let item = element("dashboard.nav.\(id)")
        XCTAssertTrue(item.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(item.label, title, app.debugDescription)
        item.tap()
    }

    private func navigationTab(_ id: String, title: String) -> XCUIElement {
        let identified = app.tabBars.buttons["dashboard.nav.\(id)"]
        return identified.exists ? identified : app.tabBars.buttons[title]
    }

    private func element(_ identifier: String) -> XCUIElement {
        app.descendants(matching: .any)[identifier]
    }

    private func attachScreenshot(named scene: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "website-\(scene)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
