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
        XCTAssertEqual(connection.label, "Connected", app.debugDescription)
        let accessoryName = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "MELK-OC21")
        ).firstMatch
        XCTAssertTrue(accessoryName.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(accessoryName.label.contains("6A"), app.debugDescription)
        XCTAssertTrue(element("lighting.color-wheel").waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(element("lighting.brightness").waitForExistence(timeout: 5), app.debugDescription)

        let power = app.switches.matching(identifier: "lighting.power").firstMatch
        XCTAssertTrue(power.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(power.isEnabled, app.debugDescription)
        if (power.value as? String) != "1" { power.tap() }
        let poweredOn = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == '1'"), object: power)
        XCTAssertEqual(XCTWaiter.wait(for: [poweredOn], timeout: 5), .completed, app.debugDescription)

        let hue = app.sliders.matching(NSPredicate(format: "label == %@", "Hue")).firstMatch
        let saturation = app.sliders.matching(NSPredicate(format: "label == %@", "Saturation")).firstMatch
        XCTAssertTrue(hue.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(saturation.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(hue.isEnabled, app.debugDescription)
        XCTAssertTrue(saturation.isEnabled, app.debugDescription)
        hue.adjust(toNormalizedSliderPosition: 0.54)
        saturation.adjust(toNormalizedSliderPosition: 0.72)
        XCTAssertFalse((hue.value as? String ?? "").isEmpty, app.debugDescription)
        XCTAssertFalse((saturation.value as? String ?? "").isEmpty, app.debugDescription)
        let brightness = app.sliders.matching(identifier: "lighting.brightness").firstMatch
        XCTAssertTrue(brightness.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(brightness.isEnabled, app.debugDescription)
        brightness.adjust(toNormalizedSliderPosition: 0.72)
        XCTAssertFalse((brightness.value as? String ?? "").isEmpty, app.debugDescription)
        attachScreenshot(named: "rgb")
    }

    func testTune() throws {
        launch(scene: "tune", extraArguments: ["-CUTOUT_UI_TEST_SETTINGS", "YES"])
        connect(fixture: "euc")
        selectTab("tune", title: "Tune")

        XCTAssertTrue(element("settings.screen.eucTune").waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.buttons["settings.on.highBeam"].waitForExistence(timeout: 10), app.debugDescription)
        let pwmTiltback = app.sliders.matching(identifier: "settings.slider.pwmTiltback").firstMatch
        XCTAssertTrue(pwmTiltback.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(pwmTiltback.isEnabled, app.debugDescription)
        XCTAssertTrue((pwmTiltback.value as? String ?? "").contains("79"), app.debugDescription)
        let displayBrightness = app.sliders.matching(identifier: "settings.slider.displayBrightness").firstMatch
        XCTAssertTrue(displayBrightness.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(displayBrightness.isEnabled, app.debugDescription)
        XCTAssertTrue((displayBrightness.value as? String ?? "").contains("30"), app.debugDescription)
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
        let summary = element("ride-map.summary")
        XCTAssertTrue(summary.waitForExistence(timeout: 10), app.debugDescription)
        assertRideSummaryHasCompleteReadback(summary)
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

    private func assertRideSummaryHasCompleteReadback(_ summary: XCUIElement) {
        let predicate = NSPredicate { object, _ in
            guard let element = object as? XCUIElement else { return false }
            let value = element.value as? String ?? ""
            let text = "\(element.label) \(value)"
            return Self.hasCompleteRideReadback(text)
        }
        let completeReadback = XCTNSPredicateExpectation(predicate: predicate, object: summary)
        XCTAssertEqual(XCTWaiter.wait(for: [completeReadback], timeout: 10), .completed, summary.debugDescription)
        let value = summary.value as? String ?? ""
        XCTAssertTrue(Self.hasCompleteRideReadback("\(summary.label) \(value)"), summary.debugDescription)
    }

    private static func hasCompleteRideReadback(_ text: String) -> Bool {
        let normalized = text.lowercased()
        guard normalized.contains("elapsed time"), normalized.contains("moving time"),
            !normalized.contains("≥"), !normalized.contains("at least"),
            !normalized.contains("coverage is partial"),
            !normalized.contains("some intervals unavailable")
        else {
            return false
        }

        let minuteDurations = matches(
            pattern: #"(?i)\b[1-9]\d*\s*min\b"#,
            in: text
        )
        let positiveSpeeds = matches(
            pattern: #"(?i)\b[1-9]\d*(?:\.\d+)?\s*(?:mph|km/h|kmh|mi/h)\b"#,
            in: text
        )
        return minuteDurations.count >= 2 && !positiveSpeeds.isEmpty
    }

    private static func matches(pattern: String, in text: String) -> [NSTextCheckingResult] {
        guard let expression = try? NSRegularExpression(pattern: pattern) else { return [] }
        return expression.matches(in: text, range: NSRange(text.startIndex..., in: text))
    }

    private func attachScreenshot(named scene: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "website-\(scene)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
