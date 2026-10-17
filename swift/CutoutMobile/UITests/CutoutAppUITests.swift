import CoreLocation
import ImageIO
import UIKit
import Vision
import XCTest

/// Runs against the installed app and its real preferences/provider session.
@MainActor
final class MusicPreferencesDeviceUITests: XCTestCase {
    func testSoundCloudUnavailableControlsMissingAppAndColdSelectionRestoration() throws {
        continueAfterFailure = false
        executionTimeAllowance = 360
        #if !targetEnvironment(simulator)
            throw XCTSkip("SoundCloud acceptance is restricted to Simulator")
        #endif
        let app = XCUIApplication()
        app.terminate()
        app.launch()
        app.activate()
        openMusicSettings(in: app)
        let picker = app.buttons["music.provider-picker"]
        let originalProvider = try XCTUnwrap(picker.value as? String)
        defer {
            app.activate()
            if !picker.exists { openMusicSettings(in: app) }
            picker.tap()
            let original = app.buttons[originalProvider].firstMatch
            if original.exists { original.tap() }
            app.buttons["setup.done"].tap()
        }
        picker.tap()
        app.buttons["SoundCloud"].firstMatch.tap()
        XCTAssertEqual(picker.value as? String, "SoundCloud")
        XCTAssertFalse(app.buttons["music.connect-provider"].exists)
        XCTAssertFalse(app.buttons["music.authorize-spotify"].exists)
        XCTAssertFalse(app.buttons["music.history-picker"].isEnabled)
        let status = app.staticTexts["music.connection-status"]
        XCTAssertTrue(status.label.contains("Choose music in CutOut"), app.debugDescription)
        XCTAssertTrue(status.label.contains("listening history is unavailable"), app.debugDescription)
        app.buttons["music.open-provider"].tap()
        let failure = app.alerts.staticTexts["The music command failed."]
        XCTAssertTrue(failure.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertEqual(app.state, .runningForeground)
        app.alerts.buttons["OK"].tap()
        XCUIDevice.shared.press(.home)
        app.activate()
        if !picker.waitForExistence(timeout: 2) { openMusicSettings(in: app) }
        XCTAssertEqual(picker.value as? String, "SoundCloud")
        XCTAssertTrue(status.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(status.label.contains("unavailable"))
        app.terminate()
        app.launch()
        app.activate()
        openMusicSettings(in: app)
        XCTAssertEqual(picker.value as? String, "SoundCloud")
        XCTAssertFalse(app.buttons["music.connect-provider"].exists)
        XCTAssertTrue(status.label.contains("unavailable"))
    }

    func testSoundCloudLiveStreamTransport() throws {
        continueAfterFailure = false
        executionTimeAllowance = 360
        #if !targetEnvironment(simulator)
            throw XCTSkip("Live SoundCloud acceptance is restricted to Simulator")
        #endif
        let app = XCUIApplication()
        app.launchArguments = ["-CUTOUT_UI_TEST_FIXTURE", "bluetooth-unavailable"]
        app.terminate()
        app.launch()
        openMusicSettings(in: app)
        let provider = app.buttons["music.provider-picker"]
        let originalProvider = try XCTUnwrap(provider.value as? String)
        defer {
            if app.buttons["music.soundcloud.done"].exists { app.buttons["music.soundcloud.done"].tap() }
            if provider.exists {
                provider.tap()
                app.buttons[originalProvider].firstMatch.tap()
            }
            app.terminate()
        }
        provider.tap()
        app.buttons["SoundCloud"].firstMatch.tap()
        app.buttons["music.soundcloud.choose-music"].tap()
        let search = app.textFields["music.soundcloud.search-field"]
        XCTAssertTrue(search.waitForExistence(timeout: 10), app.debugDescription)
        search.tap()
        search.typeText("house")
        app.buttons["music.soundcloud.search-button"].tap()
        let rows = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "music.soundcloud.track."))
        let catalogue = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in rows.count >= 2 }, object: app)
        XCTAssertEqual(XCTWaiter.wait(for: [catalogue], timeout: 60), .completed, app.debugDescription)
        rows.element(boundBy: 0).tap()
        let title = app.staticTexts["music.soundcloud.current-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10), app.debugDescription)
        let firstTitle = title.label
        let elapsed = app.staticTexts["music.soundcloud.elapsed"]
        func waitForPlayback(after seconds: Int) {
            let advanced = XCTNSPredicateExpectation(
                predicate: NSPredicate { _, _ in
                    guard elapsed.exists, let value = elapsed.value as? String, let current = Int(value) else {
                        return false
                    }
                    return current > seconds
                }, object: elapsed)
            XCTAssertEqual(XCTWaiter.wait(for: [advanced], timeout: 60), .completed, app.debugDescription)
            XCTAssertFalse(app.staticTexts["music.soundcloud.error"].exists, app.debugDescription)
        }
        waitForPlayback(after: 2)
        let backgroundAt = try XCTUnwrap(Int(try XCTUnwrap(elapsed.value as? String)))
        XCUIDevice.shared.press(.home)
        let enteredBackground = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                app.state == .runningBackground || app.state == .runningBackgroundSuspended
            }, object: app)
        XCTAssertEqual(XCTWaiter.wait(for: [enteredBackground], timeout: 10), .completed)
        let returnedEarly = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in app.state == .runningForeground }, object: app)
        XCTAssertEqual(XCTWaiter.wait(for: [returnedEarly], timeout: 12), .timedOut)
        let foregroundStartedAt = ProcessInfo.processInfo.systemUptime
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 10))
        let foregroundAt = try XCTUnwrap(Int(try XCTUnwrap(elapsed.value as? String)))
        let foregroundSeconds = Int(ProcessInfo.processInfo.systemUptime - foregroundStartedAt)
        XCTAssertGreaterThanOrEqual(
            foregroundAt, backgroundAt + foregroundSeconds + 10,
            "Playback did not advance while CutOut was backgrounded")
        XCTAssertEqual(title.label, firstTitle)
        let backgroundClock = XCTAttachment(
            string:
                "Before background: \(backgroundAt)s; after foreground: \(foregroundAt)s; foreground handling: \(foregroundSeconds)s"
        )
        backgroundClock.name = "SoundCloud background playback clock"
        backgroundClock.lifetime = .keepAlways
        add(backgroundClock)
        app.buttons["music.soundcloud.music.pause"].tap()
        XCTAssertTrue(app.buttons["music.soundcloud.music.play"].waitForExistence(timeout: 5))
        let pausedAt = try XCTUnwrap(Int(try XCTUnwrap(elapsed.value as? String)))
        let remainedPaused = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                guard let value = elapsed.value as? String, let current = Int(value) else { return false }
                return current > pausedAt + 1
            }, object: elapsed)
        XCTAssertEqual(
            XCTWaiter.wait(for: [remainedPaused], timeout: 3), .timedOut, "Paused audio clock kept advancing")
        app.buttons["music.soundcloud.music.play"].tap()
        waitForPlayback(after: pausedAt + 2)
        app.buttons["music.soundcloud.music.next"].tap()
        let changed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label != %@", firstTitle), object: title)
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 10), .completed, app.debugDescription)
        waitForPlayback(after: 2)
        app.buttons["music.soundcloud.music.previous"].tap()
        let restored = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", firstTitle), object: title)
        XCTAssertEqual(XCTWaiter.wait(for: [restored], timeout: 10), .completed, app.debugDescription)
        waitForPlayback(after: 2)
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "SoundCloud native playback controls"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        app.buttons["music.soundcloud.done"].tap()
        provider.tap()
        app.buttons["Apple Music"].firstMatch.tap()
        provider.tap()
        app.buttons["SoundCloud"].firstMatch.tap()
        app.buttons["music.soundcloud.choose-music"].tap()
        XCTAssertTrue(search.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertEqual(rows.count, 0, "A retired SoundCloud session retained its catalogue")
        XCTAssertFalse(title.exists, "A retired SoundCloud session retained its current track")
    }

    func testReadableHistorySelectionSurvivesSheetReopen() throws {
        continueAfterFailure = false
        executionTimeAllowance = 360
        #if !targetEnvironment(simulator)
            throw XCTSkip("Music preference regression is restricted to Simulator")
        #endif
        let app = XCUIApplication()
        app.launchArguments = ["-CUTOUT_UI_TEST_FIXTURE", "bluetooth-unavailable"]
        app.terminate()
        app.launch()
        app.activate()
        openMusicSettings(in: app)
        let providerPicker = app.buttons["music.provider-picker"]
        let originalProvider = try XCTUnwrap(providerPicker.value as? String)
        providerPicker.tap()
        app.buttons["Apple Music"].firstMatch.tap()
        let picker = app.buttons["music.history-picker"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5), app.debugDescription)
        let originalPolicy = try XCTUnwrap(picker.value as? String)
        func choosePolicy(_ identifier: String, expected: String) {
            picker.tap()
            app.buttons["music.history-policy.\(identifier)"].tap()
            let saved = XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "value == %@", expected), object: picker
            )
            XCTAssertEqual(XCTWaiter.wait(for: [saved], timeout: 5), .completed, app.debugDescription)
            XCTAssertFalse(app.staticTexts["music.history-error"].exists, app.debugDescription)
        }
        defer {
            app.activate()
            if !picker.exists { openMusicSettings(in: app) }
            let originalIdentifier =
                switch originalPolicy {
                case "Save IDs and titles": "human-readable"
                case "Save item IDs only": "opaque-item"
                default: "disabled"
                }
            choosePolicy(originalIdentifier, expected: originalPolicy)
            providerPicker.tap()
            app.buttons[originalProvider].firstMatch.tap()
            app.buttons["setup.done"].tap()
        }
        choosePolicy("human-readable", expected: "Save IDs and titles")
        choosePolicy("opaque-item", expected: "Save item IDs only")
        app.buttons["setup.done"].tap()
        openMusicSettings(in: app)
        XCTAssertEqual(picker.value as? String, "Save item IDs only")
        choosePolicy("human-readable", expected: "Save IDs and titles")
        app.buttons["setup.done"].tap()
        openMusicSettings(in: app)
        XCTAssertEqual(picker.value as? String, "Save IDs and titles")
        app.buttons["setup.done"].tap()
        app.terminate()
        app.launch()
        app.activate()
        openMusicSettings(in: app)
        XCTAssertEqual(picker.value as? String, "Save IDs and titles")
        XCTAssertFalse(app.staticTexts["music.history-error"].exists, app.debugDescription)
    }

    func testSpotifyMapPlayingWithoutRide() throws {
        continueAfterFailure = false
        guard ProcessInfo.processInfo.environment["CUTOUT_RUN_DEVICE_MUSIC_UI_TESTS"] == "1" else {
            throw XCTSkip("Requires Spotify playing on the physical iPhone with a cached authorization")
        }
        let app = XCUIApplication()
        // No fixtures, provider commands, preference writes, or ride creation.
        app.launch()
        app.activate()
        let setup = app.buttons["device-picker.open-setup"]
        XCTAssertTrue(setup.waitForExistence(timeout: 30), app.debugDescription)
        setup.tap()
        let music = app.buttons["setup.music"]
        XCTAssertTrue(music.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.buttons["music.connect-provider"].exists)
        music.tap()
        XCTAssertTrue(app.buttons["music.connect-provider"].waitForExistence(timeout: 5))
        XCTAssertTrue(
            app.buttons["music.connect-provider"].label.contains("Spotify"), "Spotify must already be selected")
        XCTAssertFalse(
            app.buttons["music.authorize-spotify"].exists, "Cached authorization must not offer a credential reset")
        XCTAssertEqual(app.state, .runningForeground)
        app.buttons["setup.done"].tap()
        XCTAssertTrue(setup.waitForExistence(timeout: 5))

        app.tabBars.buttons["Map"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["ride-map.screen"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["No active ride"].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["Set up music"].exists)
        requirePlayingTitle(in: app)

        XCUIDevice.shared.press(.home)
        app.activate()
        requirePlayingTitle(in: app)
        XCTAssertEqual(app.state, .runningForeground, "Recovery must not open Spotify authorization")
        XCTAssertTrue(app.staticTexts["No active ride"].exists)
    }

    func testSpotifyMapPlaybackRegardlessOfRideState() throws {
        continueAfterFailure = false
        guard ProcessInfo.processInfo.environment["CUTOUT_RUN_DEVICE_MUSIC_UI_TESTS"] == "1" else {
            throw XCTSkip("Requires Spotify playing on the physical iPhone; ride autostart is allowed")
        }
        let app = XCUIApplication()
        app.activate()
        let done = app.buttons["setup.done"]
        if done.exists {
            done.tap()
        }
        let openMap = app.tabBars.buttons["Map"]
        if openMap.exists {
            openMap.tap()
        } else if !app.descendants(matching: .any)["ride-map.screen"].exists,
            app.buttons["Map"].exists
        {
            app.buttons["Map"].tap()
        }
        XCTAssertTrue(app.descendants(matching: .any)["ride-map.screen"].waitForExistence(timeout: 5))
        if app.buttons["music.open-settings"].exists {
            app.buttons["music.open-settings"].tap()
            let status = app.staticTexts["music.connection-status"]
            XCTAssertTrue(status.waitForExistence(timeout: 5), app.debugDescription)
            XCTFail(
                "Expected Spotify playback, but Map has no player. Status: \(status.label); reauthorize visible: \(app.buttons["music.authorize-spotify"].exists)"
            )
            return
        }
        requirePlayingTitle(in: app)
    }

    func testSpotifyExplicitConnectionThenMapRecovery() throws {
        continueAfterFailure = false
        guard ProcessInfo.processInfo.environment["CUTOUT_RUN_DEVICE_MUSIC_UI_TESTS"] == "1" else {
            throw XCTSkip("Requires Spotify playing on the physical iPhone; exercises explicit Connect Spotify")
        }
        let app = XCUIApplication()
        let spotify = XCUIApplication(bundleIdentifier: "com.spotify.client")
        app.launch()
        app.activate()
        openMusicSettings(in: app)
        XCTAssertTrue(
            app.buttons["music.connect-provider"].label.contains("Spotify"), "Spotify must already be selected")
        app.buttons["music.connect-provider"].tap()

        // Existing consent may hand straight back. Never guess at Spotify's
        // authorization controls or start playback to make this test pass.
        if spotify.wait(for: .runningForeground, timeout: 5) {
            XCTAssertTrue(
                app.wait(for: .runningForeground, timeout: 45),
                "Spotify authorization did not return to CutOut:\n\(spotify.debugDescription)"
            )
        }
        XCTAssertEqual(app.state, .runningForeground, spotify.debugDescription)
        let done = app.buttons["setup.done"]
        XCTAssertTrue(done.waitForExistence(timeout: 10), app.debugDescription)
        done.tap()
        app.tabBars.buttons["Map"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["ride-map.screen"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["No active ride"].exists, app.debugDescription)
        requirePlayingTitle(in: app)

        app.terminate()
        app.launch()
        openMusicSettings(in: app)
        XCTAssertFalse(app.buttons["music.authorize-spotify"].exists, "Cold launch must retain authorization")
        app.buttons["setup.done"].tap()
        let map = app.tabBars.buttons["Map"]
        XCTAssertTrue(map.waitForExistence(timeout: 30), app.debugDescription)
        map.tap()
        XCTAssertTrue(app.staticTexts["No active ride"].waitForExistence(timeout: 5))
        requirePlayingTitle(in: app)
        XCTAssertEqual(app.state, .runningForeground, "Cached recovery must not reopen Spotify authorization")
    }

    private func openMusicSettings(in app: XCUIApplication) {
        let setup = app.buttons["device-picker.open-setup"]
        XCTAssertTrue(setup.waitForExistence(timeout: 30), app.debugDescription)
        setup.tap()
        let music = app.buttons["setup.music"]
        XCTAssertTrue(music.waitForExistence(timeout: 5), app.debugDescription)
        music.tap()
        XCTAssertTrue(app.buttons["music.history-picker"].waitForExistence(timeout: 5), app.debugDescription)
    }

    private func requirePlayingTitle(in app: XCUIApplication) {
        let restore = app.buttons["music.restore"]
        if restore.exists {
            restore.tap()
        }
        let title = app.buttons["music.expand"]
        let playing = NSPredicate { object, _ in
            guard let title = object as? XCUIElement, title.exists,
                let summary = title.value as? String
            else { return false }
            let components = summary.components(separatedBy: ", ")
            return components.count >= 3 && components.last == "Playing"
                && !["Playing", "Not playing", "Spotify", ""].contains(components[1])
        }
        let result = XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: playing, object: title)], timeout: 30)
        XCTAssertEqual(
            result, .completed, "Spotify must deliver an actual title without a ride:\n\(app.debugDescription)")
    }
}

@MainActor
final class CutoutAppUITests: XCTestCase {
    private var app: XCUIApplication!
    private var livePrivateSendCleanup: (() -> Void)?

    override func setUp() async throws {
        try await super.setUp()
        if name.contains("RetainsThroughRefreshFailureAndRetry") || name.contains("KeepsDateValueReadable") {
            executionTimeAllowance = 360
        }
        continueAfterFailure = false
        XCUIDevice.shared.orientation = isLandscapeTest ? .landscapeLeft : .portrait
        app = XCUIApplication()
        app.launchArguments = launchArguments
        app.launchEnvironment = launchEnvironment
        app.launch()
    }

    override func tearDown() async throws {
        continueAfterFailure = true
        let cleanup = livePrivateSendCleanup
        livePrivateSendCleanup = nil
        cleanup?()
        let activityFixtureExited = fixture.usesSystemLiveActivity && app?.state == .notRunning
        if fixture.usesSystemLiveActivity, let app {
            if activityFixtureExited {
                launchLiveActivityCleanup()
            } else {
                app.activate()
                if !app.launchArguments.contains("--ui-test-cleanup-live-activities"),
                    !app.descendants(matching: .any)["device-picker.screen"].exists,
                    !app.buttons["dashboard.disconnect"].isHittable
                {
                    tapNavigationTab("ride", title: "Ride")
                }
            }
        }
        let disconnect = app?.buttons["dashboard.disconnect"]
        if disconnect?.exists == true, disconnect?.isHittable == true {
            disconnect?.tap()
            _ = app?.descendants(matching: .any)["device-picker.screen"].waitForExistence(timeout: 5)
        }
        if fixture.usesSystemLiveActivity {
            if let app, app.state != .notRunning {
                assertSystemLiveActivitiesEnded()
            } else {
                XCTFail("The Live Activity cleanup app did not stay running")
            }
        }
        app?.terminate()
        app = nil
        XCTAssertFalse(activityFixtureExited, "The real Live Activity fixture exited before teardown")
        XCUIDevice.shared.orientation = .portrait
        try await super.tearDown()
    }

    func testPickerSetupOpensGeneralSettingsWithoutConnectingMusic() throws {
        try verifySetupNavigation()
    }

    func testPickerSurfaceDiagnosticsConsentPersistsAndPolicyLinksAreReachable() {
        app.terminate()
        app.launchEnvironment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"] = UUID().uuidString
        app.launch()

        func openDiagnostics() -> XCUIElement {
            let setup = app.buttons["device-picker.open-setup"]
            XCTAssertTrue(setup.waitForExistence(timeout: 10), app.debugDescription)
            setup.tap()
            let consent = app.switches["setup.diagnostics.automatic"]
            XCTAssertTrue(consent.waitForExistence(timeout: 10), app.debugDescription)
            scrollElementFrameIntoViewport(consent, in: app.collectionViews.firstMatch, maxScrolls: 8)
            let control = consent.switches.firstMatch
            XCTAssertTrue(control.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertTrue(control.isHittable, app.debugDescription)
            return control
        }

        func waitForValue(_ value: String, of control: XCUIElement) {
            let saved = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", value), object: control)
            XCTAssertEqual(XCTWaiter.wait(for: [saved], timeout: 10), .completed, app.debugDescription)
        }

        let consent = openDiagnostics()
        waitForValue("0", of: consent)
        consent.tap()
        waitForValue("1", of: consent)
        app.buttons["setup.done"].tap()
        app.terminate()
        app.launch()

        let restored = openDiagnostics()
        waitForValue("1", of: restored)
        restored.tap()
        waitForValue("0", of: restored)
        for identifier in ["setup.privacy-policy", "setup.support"] {
            let link = app.descendants(matching: .any)[identifier]
            scrollElementFrameIntoViewport(link, in: app.collectionViews.firstMatch, maxScrolls: 8)
            XCTAssertTrue(link.exists && link.isHittable, app.debugDescription)
        }
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Diagnostics consent and privacy-contact links"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    func testRideMapShowsUnavailableStorageWithoutFalseEmptyStateOrStart() throws {
        let map = app.tabBars.buttons["Map"]
        XCTAssertTrue(map.waitForExistence(timeout: 10), app.debugDescription)
        map.tap()

        let persistenceUnavailable = "Ride storage is unavailable."
        let status = app.descendants(matching: .any)["ride-map.recording-pill"]
        XCTAssertTrue(status.waitForExistence(timeout: 10), app.debugDescription)
        let unavailableStatus = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", persistenceUnavailable),
            object: status
        )
        XCTAssertEqual(XCTWaiter.wait(for: [unavailableStatus], timeout: 10), .completed, app.debugDescription)
        XCTAssertEqual(status.label, persistenceUnavailable, app.debugDescription)
        XCTAssertFalse(app.staticTexts["No active ride"].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["ride-map.start"].exists, app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.map"].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["ride-map.recenter"].exists, app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.persistence-warning"].exists, app.debugDescription)
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "ride-map-storage-unavailable"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    func testPickerSurfaceMusicHistoryPreferenceSurvivesRelaunchWithoutRide() throws {
        func assertNoActiveRide() {
            let map = app.tabBars.buttons["Map"]
            XCTAssertTrue(map.waitForExistence(timeout: 10), app.debugDescription)
            map.tap()
            XCTAssertTrue(app.staticTexts["No active ride"].waitForExistence(timeout: 5), app.debugDescription)
            app.tabBars.buttons["Devices"].tap()
        }

        func openHistoryPicker() -> XCUIElement {
            let setup = app.buttons["device-picker.open-setup"]
            XCTAssertTrue(setup.waitForExistence(timeout: 10), app.debugDescription)
            setup.tap()
            app.buttons["setup.music"].tap()
            let picker = app.buttons["music.history-picker"]
            scrollElementFrameIntoViewport(picker, in: app.collectionViews.firstMatch, maxScrolls: 8)
            XCTAssertTrue(picker.waitForExistence(timeout: 5), app.debugDescription)
            return picker
        }

        assertNoActiveRide()
        let picker = openHistoryPicker()
        picker.tap()
        app.buttons["music.history-policy.opaque-item"].tap()
        XCTAssertEqual(picker.value as? String, "Save item IDs only")
        picker.tap()
        app.buttons["music.history-policy.human-readable"].tap()
        XCTAssertEqual(picker.value as? String, "Save IDs and titles")
        app.buttons["setup.done"].tap()

        app.terminate()
        app.launch()

        assertNoActiveRide()
        XCTAssertEqual(openHistoryPicker().value as? String, "Save IDs and titles")
        XCTAssertFalse(app.buttons["dashboard.disconnect"].exists)
    }

    func testMoreMusicAppleOpensWithoutSnapshotAfterRelaunch() throws {
        try verifyMusicOpensWithoutSnapshotAfterRelaunch()
    }

    func testMoreMusicSpotifyOpensWithoutSnapshotAfterRelaunch() throws {
        try verifyMusicOpensWithoutSnapshotAfterRelaunch()
    }

    private func assertMinimumControlDimension(
        _ dimension: CGFloat,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        // CGRect arithmetic can return 43.99999999999997 for a logical 44pt
        // target. One millionth of a point absorbs that rounding, not undersizing.
        XCTAssertGreaterThanOrEqual(dimension + 0.000001, 44, file: file, line: line)
    }

    @discardableResult
    private func assertReachableSheetDone(_ identifier: String) -> XCUIElement {
        let done = app.buttons[identifier]
        XCTAssertTrue(done.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(done.elementType, .button)
        XCTAssertTrue(done.isHittable, app.debugDescription)
        assertMinimumControlDimension(done.frame.width)
        assertMinimumControlDimension(done.frame.height)
        XCTAssertTrue(app.windows.firstMatch.frame.insetBy(dx: -2, dy: -2).contains(done.frame), app.debugDescription)
        return done
    }

    private func verifyMusicOpensWithoutSnapshotAfterRelaunch() throws {
        for attempt in 0..<2 {
            if attempt > 0 {
                app.terminate()
                app.launch()
            }
            XCTAssertFalse(app.descendants(matching: .any)["music.compact-player"].exists)
            app.tabBars.buttons["More"].tap()
            let music = app.buttons["more.music"]
            XCTAssertTrue(music.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertTrue(music.isHittable)
            music.tap()
            XCTAssertTrue(app.descendants(matching: .any)["music.player.screen"].waitForExistence(timeout: 5))
            assertReachableSheetDone("music.done")
            if !name.contains("Spotify") {
                XCTAssertTrue(app.staticTexts["music.player.not-connected"].exists, app.debugDescription)
                XCTAssertTrue(app.buttons["music.open-provider"].isHittable)
            }
            let settings = app.buttons["music.open-settings"]
            XCTAssertTrue(settings.waitForExistence(timeout: 5), app.debugDescription)
            if !settings.isHittable { app.swipeUp() }
            settings.tap()
            XCTAssertTrue(app.descendants(matching: .any)["music.settings.screen"].waitForExistence(timeout: 5))
            XCTAssertTrue(app.buttons["music.connect-provider"].exists)
            let history = app.descendants(matching: .any)["music.history-picker"]
            scrollElementFrameIntoViewport(history, in: app.collectionViews.firstMatch, maxScrolls: 8)
            XCTAssertTrue(history.exists)
            XCTAssertTrue(history.isHittable)
            assertReachableSheetDone("setup.done").tap()
            XCTAssertTrue(music.waitForExistence(timeout: 5))
            XCTAssertTrue(music.isHittable)
        }
    }

    func testMusicPlayerCanReopenFromMoreAfterHiding() throws {
        app.buttons["music.expand"].tap()
        let expandedPlayer = app.descendants(matching: .any)["music.player.screen"]
        XCTAssertTrue(expandedPlayer.waitForExistence(timeout: 5))
        assertReachableSheetDone("music.done")
        let hide = app.buttons["music.hide"]
        scrollElementFrameIntoViewport(hide, in: expandedPlayer.collectionViews.firstMatch, maxScrolls: 8)
        hide.tap()
        XCTAssertFalse(app.descendants(matching: .any)["music.compact-player"].exists)
        app.tabBars.buttons["More"].tap()
        app.buttons["more.music"].tap()
        XCTAssertTrue(expandedPlayer.waitForExistence(timeout: 5))
        assertReachableSheetDone("music.done")
        let pause = expandedPlayer.buttons["Pause"]
        scrollElementFrameIntoViewport(pause, in: expandedPlayer.collectionViews.firstMatch, maxScrolls: 8)
        let pauseIsHittable = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in pause.isHittable }, object: pause
        )
        XCTAssertEqual(XCTWaiter.wait(for: [pauseIsHittable], timeout: 5), .completed, app.debugDescription)
        pause.tap()
        XCTAssertTrue(expandedPlayer.buttons["Play"].waitForExistence(timeout: 5), app.debugDescription)
        expandedPlayer.buttons["Play"].tap()
        XCTAssertTrue(pause.waitForExistence(timeout: 5), app.debugDescription)
        app.buttons["music.done"].tap()
        XCTAssertTrue(app.buttons["music.expand"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["music.expand"].isHittable)
        app.buttons["music.expand"].tap()
        let settings = app.buttons["music.open-settings"]
        scrollElementFrameIntoViewport(settings, in: expandedPlayer.collectionViews.firstMatch, maxScrolls: 8)
        XCTAssertTrue(settings.isHittable, app.debugDescription)
        settings.tap()
        XCTAssertTrue(app.descendants(matching: .any)["music.settings.screen"].waitForExistence(timeout: 5))
        app.buttons["setup.done"].tap()
        XCTAssertTrue(app.buttons["music.expand"].waitForExistence(timeout: 5))
    }

    func testMusicPlayerPlayingAcrossPickerMapMore() throws {
        try assertMusicPlayerAcrossRoutes(expectedTransport: "Pause")
        let player = app.descendants(matching: .any)["music.compact-player"]
        let pause = player.buttons["Pause"]
        pause.tap()
        XCTAssertTrue(player.buttons["Play"].waitForExistence(timeout: 5), app.debugDescription)
        player.buttons["Play"].tap()
        XCTAssertTrue(pause.waitForExistence(timeout: 5), app.debugDescription)
        player.buttons["Next track"].tap()
        let nextTitle = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS %@", "Everything In Its Right Place 2"),
            object: app.buttons["music.expand"]
        )
        XCTAssertEqual(XCTWaiter.wait(for: [nextTitle], timeout: 5), .completed, app.debugDescription)
        app.buttons["music.expand"].tap()
        app.buttons["Previous track"].tap()
        XCTAssertTrue(app.staticTexts["Everything In Its Right Place 1"].waitForExistence(timeout: 5))
        let hide = app.buttons["music.hide"]
        for _ in 0..<5 where !hide.isHittable { app.swipeUp() }
        XCTAssertTrue(hide.isHittable, app.debugDescription)
        hide.tap()
        XCTAssertFalse(player.exists, app.debugDescription)
        let restore = app.buttons["music.restore"]
        for _ in 0..<5 where !restore.isHittable { app.swipeUp() }
        XCTAssertTrue(restore.isHittable, app.debugDescription)
        restore.tap()
        XCTAssertTrue(player.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["music.expand"].isHittable)
    }

    func testMusicPlayerPausedAcrossEucRideMapMoreAtAccessibilityDynamicType() throws {
        // Three routes, repeated Readings checks, and rotations exceed the default two minutes.
        executionTimeAllowance = 360
        XCTAssertTrue(pairAvailableDevice(.euc))
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.screen.eucRide"].waitForExistence(timeout: 20))
        try assertMusicPlayerAcrossRoutes(expectedTransport: "Play")
        defer { XCUIDevice.shared.orientation = .portrait }

        tapNavigationTab("ride", title: "Ride")
        assertPausedMusicRideGeometry(isLandscape: false)
        XCUIDevice.shared.orientation = .landscapeLeft
        assertPausedMusicRideGeometry(isLandscape: true)
        XCUIDevice.shared.orientation = .portrait
        assertPausedMusicRideGeometry(isLandscape: false)

        app.buttons["music.expand"].tap()
        XCTAssertTrue(app.buttons["music.done"].waitForExistence(timeout: 5))
        let hide = app.buttons["music.hide"]
        scrollElementFrameIntoViewport(hide, in: app, maxScrolls: 8)
        hide.tap()
        assertPausedMusicRideGeometry(isLandscape: false, playerIsHidden: true)

        // Restore in a different orientation so a retained accessory frame cannot
        // accidentally satisfy the ride's viewport calculation.
        XCUIDevice.shared.orientation = .landscapeLeft
        assertPausedMusicRideGeometry(isLandscape: true, playerIsHidden: true)
        tapNavigationTab("more", title: "More")
        let restore = app.buttons["music.restore"]
        scrollElementFrameIntoViewport(
            restore, in: app.descendants(matching: .any)["more.screen"], maxScrolls: 8,
            occludedBy: app.tabBars.firstMatch
        )
        restore.tap()
        tapNavigationTab("ride", title: "Ride")
        assertPausedMusicRideGeometry(isLandscape: true)
    }

    private func assertPausedMusicRideGeometry(isLandscape: Bool, playerIsHidden: Bool = false) {
        let window = app.windows.firstMatch
        let ride = app.descendants(matching: .any)["dashboard.screen.eucRide"]
        let player = app.descendants(matching: .any)["music.compact-player"]
        let tabs = app.tabBars.firstMatch
        assertRideWindowHasSettled(ride, landscape: isLandscape)
        assertRideTextSizeReadback(ride, systemCategory: rideTextSizeCategory)
        let labels =
            rideTextSizeCategory == "UICTContentSizeCategoryAccessibilityXXXL"
            ? ["Battery"] : ["Battery", "pack", "power", "thermal"]
        let metrics = labels.map { label in
            ride.descendants(matching: .any).matching(NSPredicate(format: "label == %@", label)).firstMatch
        }
        let essentialContent =
            metrics + [
                ride.descendants(matching: .any)["ride.hero.status"],
                ride.descendants(matching: .any)["ride.hero.speed"],
            ]
        let settled = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                let frame = window.frame
                guard XCUIDevice.shared.orientation == (isLandscape ? .landscapeLeft : .portrait),
                    frame.width > 0, frame.height > 0,
                    (frame.width > frame.height) == isLandscape,
                    ride.exists, tabs.exists,
                    playerIsHidden ? !player.exists : player.exists
                else { return false }
                if !playerIsHidden {
                    guard frame.contains(player.frame), player.frame.maxY <= tabs.frame.minY + 2 else { return false }
                }
                let viewport = self.unobscuredFrame(in: ride, above: playerIsHidden ? tabs : player)
                return essentialContent.allSatisfy {
                    $0.exists && $0.isHittable && viewport.insetBy(dx: -2, dy: -2).contains($0.frame)
                }
            }, object: app
        )
        XCTAssertEqual(XCTWaiter.wait(for: [settled], timeout: 5), .completed, app.debugDescription)
        XCTAssertEqual(ride.scrollViews.count, 0, app.debugDescription)
        let viewport = unobscuredFrame(in: ride, above: playerIsHidden ? tabs : player)
        for metric in essentialContent {
            XCTAssertTrue(metric.isHittable, app.debugDescription)
            XCTAssertTrue(window.frame.insetBy(dx: -2, dy: -2).contains(metric.frame), app.debugDescription)
            XCTAssertTrue(viewport.insetBy(dx: -2, dy: -2).contains(metric.frame), app.debugDescription)
        }
        guard !playerIsHidden else {
            XCTAssertFalse(player.exists, app.debugDescription)
            retainMusicRideScreenshot(isLandscape: isLandscape, playerIsHidden: true)
            if rideTextSizeCategory == "UICTContentSizeCategoryAccessibilityXXXL" {
                assertAccessibleRideReadings(["Battery", "pack", "power", "thermal"])
            }
            return
        }
        let controls = [app.buttons["music.expand"], player.buttons["Play"], player.buttons["Next track"]]
        for (index, control) in controls.enumerated() {
            XCTAssertTrue(control.isHittable, app.debugDescription)
            assertMinimumControlDimension(control.frame.width)
            assertMinimumControlDimension(control.frame.height)
            XCTAssertTrue(player.frame.insetBy(dx: -2, dy: -2).contains(control.frame), app.debugDescription)
            for other in controls.dropFirst(index + 1) {
                XCTAssertFalse(control.frame.intersects(other.frame), app.debugDescription)
            }
        }
        retainMusicRideScreenshot(isLandscape: isLandscape, playerIsHidden: false)
        if rideTextSizeCategory == "UICTContentSizeCategoryAccessibilityXXXL" {
            assertAccessibleRideReadings(["Battery", "pack", "power", "thermal"])
        }
    }

    private func retainMusicRideScreenshot(isLandscape: Bool, playerIsHidden: Bool) {
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "music-ride-\(isLandscape ? "landscape" : "portrait")-\(playerIsHidden ? "hidden" : "shown")"
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    func testMusicPlayerRecoveryAcrossPickerSurfaceMapMoreAtAccessibilityDynamicType() throws {
        try assertMusicPlayerAcrossRoutes(expectedTransport: nil)
        let summary = app.buttons["music.expand"].value as? String ?? ""
        XCTAssertTrue(summary.contains("Can’t get playback"), summary)
        let openProvider = app.descendants(matching: .any)["music.compact-player"].buttons["music.open-provider"]
        XCTAssertTrue(openProvider.isHittable, app.debugDescription)
        assertMinimumControlDimension(openProvider.frame.width)
        XCTAssertFalse(app.buttons["music.expand"].frame.intersects(openProvider.frame))
    }

    func testMusicPlayerPreviousOnlyShowsOpenProvider() throws {
        let player = app.descendants(matching: .any)["music.compact-player"]
        XCTAssertTrue(player.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(player.buttons["Previous track"].exists)
        XCTAssertFalse(player.buttons["Play"].exists)
        XCTAssertFalse(player.buttons["Pause"].exists)
        let openProvider = player.buttons["music.open-provider"]
        XCTAssertTrue(openProvider.isHittable, app.debugDescription)
        assertMinimumControlDimension(openProvider.frame.width)
        XCTAssertFalse(app.buttons["music.expand"].frame.intersects(openProvider.frame))
    }

    private func assertMusicPlayerAcrossRoutes(expectedTransport: String?) throws {
        for route in [nil, "map", "more"] as [String?] {
            if let route { tapNavigationTab(route, title: route.capitalized) }
            let player = app.descendants(matching: .any)["music.compact-player"]
            XCTAssertTrue(player.waitForExistence(timeout: 5), app.debugDescription)
            let expand = app.buttons["music.expand"]
            XCTAssertTrue(expand.isHittable, app.debugDescription)
            assertMinimumControlDimension(expand.frame.height)
            let summary = expand.value as? String ?? ""
            XCTAssertTrue(summary.contains("Everything In Its Right Place 1"), summary)
            XCTAssertTrue(summary.contains("Radiohead"), summary)
            let tabs = app.tabBars.firstMatch
            XCTAssertLessThanOrEqual(player.frame.maxY, tabs.frame.minY + 2, app.debugDescription)
            XCTAssertTrue(app.windows.firstMatch.frame.contains(player.frame), app.debugDescription)
            if route == "map" {
                let scroll = app.scrollViews["ride-map.live-viewport"]
                XCTAssertTrue(scroll.waitForExistence(timeout: 5), app.debugDescription)
                // The native scroll frame includes TabView's safe-area tail;
                // actions must scroll fully into the region above the player.
                var visibleActionCount = 0
                for action in ["pause", "resume", "save", "stop", "start", "discard"] {
                    let control = app.buttons["ride-map.\(action)"]
                    guard control.exists else { continue }
                    scrollElementFrameIntoViewport(control, in: scroll, maxScrolls: 10, occludedBy: player)
                    visibleActionCount += 1
                    let viewport = unobscuredFrame(in: scroll, above: player)
                    XCTAssertTrue(viewport.insetBy(dx: -2, dy: -2).contains(control.frame), app.debugDescription)
                    XCTAssertTrue(control.isHittable, app.debugDescription)
                    XCTAssertLessThanOrEqual(control.frame.maxY, player.frame.minY, app.debugDescription)
                    let screenshot = XCTAttachment(screenshot: app.screenshot())
                    screenshot.name = "music-map-\(action)-visible"
                    screenshot.lifetime = .keepAlways
                    add(screenshot)
                }
                XCTAssertGreaterThan(visibleActionCount, 0, app.debugDescription)
            }
            let ride = app.descendants(matching: .any)["dashboard.screen.eucRide"]
            if route == nil, ride.exists {
                XCTAssertEqual(ride.scrollViews.count, 0)
                let labels =
                    rideTextSizeCategory == "UICTContentSizeCategoryAccessibilityXXXL"
                    ? ["Battery"] : ["Battery", "pack", "power", "thermal"]
                for label in labels {
                    let metric = ride.descendants(matching: .any).matching(
                        NSPredicate(format: "label == %@", label)
                    ).firstMatch
                    XCTAssertTrue(metric.isHittable, app.debugDescription)
                    XCTAssertLessThanOrEqual(metric.frame.maxY, player.frame.minY, app.debugDescription)
                }
            }
            if let expectedTransport {
                let transport = player.buttons[expectedTransport]
                XCTAssertTrue(transport.isHittable, app.debugDescription)
                assertMinimumControlDimension(transport.frame.height)
                assertMinimumControlDimension(transport.frame.width)
                XCTAssertFalse(expand.frame.intersects(transport.frame), app.debugDescription)
            } else {
                XCTAssertFalse(player.buttons["Play"].exists)
                XCTAssertFalse(player.buttons["Pause"].exists)
            }
            try performTextClippingAudit(
                named: "music-player-\(route ?? "initial")", allowingCompactMusicTitleTruncation: true
            )
            let screenshot = XCTAttachment(screenshot: app.screenshot())
            screenshot.name = "music-player-\(route ?? "initial")"
            screenshot.lifetime = .keepAlways
            add(screenshot)
        }
        app.buttons["music.expand"].tap()
        let done = assertReachableSheetDone("music.done")
        XCTAssertTrue(app.staticTexts["Everything In Its Right Place 1"].exists, app.debugDescription)
        XCTAssertTrue(app.staticTexts["Radiohead"].exists, app.debugDescription)
        try performTextClippingAudit(named: "music-details")
        done.tap()
        XCTAssertTrue(app.buttons["music.expand"].waitForExistence(timeout: 5))
    }

    func testPickerSetupOpensGeneralSettingsInDarkAppearanceAtAccessibilityDynamicType() throws {
        try verifySetupNavigation()
    }

    func testPickerSetupOpensGeneralSettingsInLightAppearanceAtAccessibilityDynamicType() throws {
        try verifySetupNavigation()
    }

    private func verifySetupNavigation() throws {
        let setup = app.buttons["device-picker.open-setup"]
        XCTAssertTrue(setup.waitForExistence(timeout: 5))
        XCTAssertTrue(setup.isHittable)
        setup.tap()
        let music = app.buttons["setup.music"]
        XCTAssertTrue(music.waitForExistence(timeout: 5))
        XCTAssertTrue(music.isHittable)
        XCTAssertFalse(app.buttons["music.connect-provider"].exists)
        music.tap()
        XCTAssertTrue(app.buttons["music.provider-picker"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["music.provider-picker"].isHittable)
        XCTAssertTrue(app.buttons["music.history-picker"].exists)
        XCTAssertEqual(app.state, .runningForeground)
        try performVisibleLayoutAccessibilityAudit()
        let done = app.buttons["setup.done"]
        XCTAssertTrue(done.isHittable)
        done.tap()
        XCTAssertTrue(setup.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["device-picker.open-advanced-capture"].exists)
    }

    func testPickerExposesAccessibleCaptureControls() {
        XCTAssertFalse(app.buttons["device-picker.capture-status"].exists)
        XCTAssertFalse(app.buttons["device-picker.open-advanced-capture"].exists)
        let setup = openCaptureSetup()
        let description = app.textFields["captures.description"]
        let start = app.buttons["captures.start"]
        XCTAssertTrue(description.waitForExistence(timeout: 5))
        XCTAssertTrue(start.isEnabled, "Description is optional")
        assertMinimumControlDimension(start.frame.height)
        XCTAssertTrue(setup.exists)
    }

    func testPickerKeepsDetectionEvidenceBehindDeviceDetails() throws {
        try assertPickerDeviceDetails()
    }

    func testPickerKeepsDetectionEvidenceBehindDeviceDetailsAtAccessibilityDynamicType() throws {
        try assertPickerDeviceDetails()
        try performVisibleLayoutAccessibilityAudit()
    }

    private func assertPickerDeviceDetails() throws {
        let connect = app.buttons["device-picker.use.ui-test-vesc"]
        XCTAssertTrue(connect.waitForExistence(timeout: 5))
        XCTAssertTrue(connect.isHittable)
        assertMinimumControlDimension(connect.frame.height)
        XCTAssertFalse(app.staticTexts["Deterministic accessibility test device"].exists)
        XCTAssertFalse(app.staticTexts["VESC Onewheel - UI test fixture"].exists)
        let details = app.buttons["device-picker.details.ui-test-vesc"]
        XCTAssertTrue(details.isHittable)
        assertMinimumControlDimension(details.frame.height)
        details.tap()
        XCTAssertTrue(app.staticTexts["Deterministic accessibility test device"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["VESC Onewheel - UI test fixture"].exists)
        app.buttons["device-picker.device-details.done"].tap()
        XCTAssertTrue(connect.waitForExistence(timeout: 5))
        XCTAssertTrue(connect.isHittable)
    }

    func testProbeActionDoesNotFallThroughToRecordOnly() {
        disconnectIfConnected()
        let useButton = app.buttons["device-picker.use.ui-test-probe"]

        XCTAssertTrue(useButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(useButton.isEnabled)
        XCTAssertTrue(useButton.isHittable)
        XCTAssertEqual(useButton.label, "Connect to Unknown EUC")
        XCTAssertEqual(useButton.value as? String, "ROBE")

        useButton.tap()

        XCTAssertNotNil(connectedScreen(timeout: 20))
        disconnectIfConnected()
    }

    func testProbeTimeoutRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification timed out")
    }

    func testProbeTimeoutRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType() throws
    {
        try assertProbeFailure("Device identification timed out")
    }

    func testProbeTimeoutRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification timed out")
    }

    func testProbeMalformedResponseRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device returned an invalid identification response")
    }

    func
        testProbeMalformedResponseRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType()
        throws
    {
        try assertProbeFailure("Device returned an invalid identification response")
    }

    func
        testProbeMalformedResponseRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType()
        throws
    {
        try assertProbeFailure("Device returned an invalid identification response")
    }

    func testProbeConflictingEvidenceRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device identification found conflicting evidence")
    }

    func
        testProbeConflictingEvidenceRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType()
        throws
    {
        try assertProbeFailure("Device identification found conflicting evidence")
    }

    func
        testProbeConflictingEvidenceRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType()
        throws
    {
        try assertProbeFailure("Device identification found conflicting evidence")
    }

    func testProbeUnsupportedRemainsOnPickerAndExposesAccessibleFailureAtAccessibilityDynamicType() throws {
        try assertProbeFailure("Device does not support this identification probe")
    }

    func testProbeUnsupportedRemainsOnPickerAndExposesAccessibleFailureInLightAppearanceAtAccessibilityDynamicType()
        throws
    {
        try assertProbeFailure("Device does not support this identification probe")
    }

    func testProbeUnsupportedRemainsOnPickerAndExposesAccessibleFailureInDarkAppearanceAtAccessibilityDynamicType()
        throws
    {
        try assertProbeFailure("Device does not support this identification probe")
    }

    func testSupportedPickerRowUsesOneWholeRowAction() {
        let useButton = app.buttons["device-picker.use.ui-test-vesc"]

        XCTAssertTrue(useButton.waitForExistence(timeout: 5))
        XCTAssertEqual(useButton.label, "Connect to Refloat VESC")
        XCTAssertEqual(useButton.value as? String, "VESC")
        XCTAssertGreaterThanOrEqual(useButton.frame.height, 92)
    }

    func testCameraSurfaceReturnsToTheRideWithoutCameraNetwork() {
        assertCameraTabReturnsToRide(.vesc)
    }

    func testEucCameraTabReturnsToRideWithoutCameraNetwork() {
        assertCameraTabReturnsToRide(.euc)
    }

    func testEucMoreKeepsMapOnTabBar() {
        XCTAssertTrue(pairAvailableDevice(.euc))
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.screen.eucRide"].waitForExistence(timeout: 20))
        for id in ["ride", "map", "tune", "more"] {
            let title = id.capitalized
            let tab = navigationTab(id, title: title)
            XCTAssertTrue(tab.exists, app.debugDescription)
            XCTAssertEqual(tab.elementType, .button)
            XCTAssertTrue(tab.isHittable, app.debugDescription)
            assertMinimumControlDimension(tab.frame.width)
            assertMinimumControlDimension(tab.frame.height)
            XCTAssertEqual(app.tabBars.buttons.matching(NSPredicate(format: "label == %@", title)).count, 1)
        }
        for id in ["camera", "lighting", "pack"] {
            XCTAssertFalse(app.tabBars.buttons["dashboard.nav.\(id)"].exists)
            XCTAssertFalse(app.tabBars.buttons[id.capitalized].exists)
        }
        tapNavigationTab("map", title: "Map")
        XCTAssertTrue(app.descendants(matching: .any)["ride-map.screen"].waitForExistence(timeout: 5))
        tapNavigationTab("more", title: "More")
        XCTAssertTrue(app.descendants(matching: .any)["more.screen"].waitForExistence(timeout: 5))
        for id in ["camera", "lighting", "pack"] {
            XCTAssertTrue(app.descendants(matching: .any)["dashboard.nav.\(id)"].isHittable)
        }
        attachScreenshot(of: app, named: "More menu with Camera Lighting and Pack")
        tapNavigationTab("camera", title: "Camera")
        XCTAssertTrue(app.descendants(matching: .any)["camera.screen"].waitForExistence(timeout: 5))
        let back = app.buttons["dashboard.back"]
        XCTAssertTrue(back.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(back.isHittable, app.debugDescription)
        assertMinimumControlDimension(back.frame.width)
        assertMinimumControlDimension(back.frame.height)
        back.tap()
        XCTAssertTrue(app.descendants(matching: .any)["more.screen"].waitForExistence(timeout: 5))
        tapNavigationTab("ride", title: "Ride")
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.screen.eucRide"].waitForExistence(timeout: 5))
        disconnectIfConnected()
    }

    func testEucCameraKeepsTuneOnTabBar() {
        XCTAssertTrue(pairAvailableDevice(.euc))
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.screen.eucRide"].waitForExistence(timeout: 20))
        tapNavigationTab("camera", title: "Camera")
        XCTAssertTrue(app.descendants(matching: .any)["camera.screen"].waitForExistence(timeout: 5))
        XCTAssertTrue(navigationTab("tune", title: "Tune").isHittable, app.debugDescription)
        tapNavigationTab("tune", title: "Tune")
        XCTAssertTrue(app.descendants(matching: .any)["settings.screen.tune"].waitForExistence(timeout: 5))
        navigationTab("ride", title: "Ride").tap()
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.screen.eucRide"].waitForExistence(timeout: 5))
        disconnectIfConnected()
    }

    func testPickerSurfaceCameraTabReturnsToDevicesWithoutConnecting() {
        let picker = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(picker.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.tabBars.buttons["dashboard.nav.camera"].exists)
        tapNavigationTab("camera", title: "Camera")
        XCTAssertTrue(app.descendants(matching: .any)["camera.screen"].waitForExistence(timeout: 5))
        let devicesTab = app.tabBars.buttons["dashboard.nav.devices"]
        XCTAssertTrue(devicesTab.isHittable, app.debugDescription)
        devicesTab.tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.more"].isHittable)
        XCTAssertFalse(app.buttons["dashboard.disconnect"].exists)
    }

    private func assertCameraTabReturnsToRide(_ family: ConnectedDeviceFamily) {
        XCTAssertTrue(pairAvailableDevice(family))
        let ride = app.descendants(matching: .any)[family.screenIdentifier]
        XCTAssertTrue(ride.waitForExistence(timeout: 20), app.debugDescription)

        tapNavigationTab("camera", title: "Camera")

        let camera = app.descendants(matching: .any)["camera.screen"]
        XCTAssertTrue(camera.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.textFields["camera.origin.address"].exists)
        XCTAssertFalse(app.textFields["camera.origin.port"].exists)
        XCTAssertTrue(app.buttons["camera.connect"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["camera.truth.card"].exists)
        attachScreenshot(of: app, named: "Camera connection without address setup")
        let rideTab = app.tabBars.buttons["dashboard.nav.ride"]
        XCTAssertTrue(rideTab.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(rideTab.isHittable)
        rideTab.tap()

        XCTAssertTrue(ride.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(app.buttons["dashboard.disconnect"].isHittable)
        disconnectIfConnected()
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
        XCTAssertEqual(Fixture.testFixture(for: "testEucStaleTelemetryKeepsRideLayoutFixed"), .eucStale)
        XCTAssertEqual(Fixture.testFixture(for: "testEUCNoBmsSurfacePassesAccessibilityAudit"), .eucNoBms)
        XCTAssertEqual(Fixture.testFixture(for: "testEUCReconnectKeepsRideRoute"), .eucReconnect)
    }

    func testPickerSurfaceHomeMapRouteKeepsMapAndLifecycleActionsReachable() throws {
        let mapButton = app.tabBars.buttons["Map"]
        XCTAssertTrue(mapButton.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertTrue(mapButton.isHittable)
        mapButton.tap()

        let mapScreen = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(mapScreen.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "ride-map.screen").count, 1)
        let summary = app.descendants(matching: .any)["ride-map.summary"]
        XCTAssertTrue(summary.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(summary.label.contains("Elapsed time"), app.debugDescription)
        XCTAssertTrue(summary.label.contains("Moving time"), app.debugDescription)
        XCTAssertFalse((summary.value as? String ?? "").isEmpty, app.debugDescription)
        XCTAssertFalse(summary.label.contains("Recording time"), app.debugDescription)
        let modePicker = app.descendants(matching: .any)["ride-map.mode-picker"].segmentedControls.firstMatch
        XCTAssertTrue(modePicker.waitForExistence(timeout: 5), app.debugDescription)
        let pauseButton = app.buttons["ride-map.pause"]
        let restoredResumeButton = app.buttons["ride-map.resume"]
        if pauseButton.waitForExistence(timeout: 2) {
            XCTAssertTrue(pauseButton.isEnabled)
            XCTAssertTrue(pauseButton.isHittable)
        } else {
            XCTAssertTrue(restoredResumeButton.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertFalse(app.buttons["ride-map.start"].exists, app.debugDescription)
            restoredResumeButton.tap()
        }
        XCTAssertFalse(app.buttons["ride-map.start"].exists, app.debugDescription)
        XCTAssertTrue(pauseButton.waitForExistence(timeout: 5), app.debugDescription)
        let routeMap = app.descendants(matching: .any)["ride-map.map"]
        XCTAssertTrue(routeMap.waitForExistence(timeout: 5), app.debugDescription)
        let recenterButton = app.buttons["ride-map.recenter"]
        XCTAssertTrue(recenterButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(recenterButton.isEnabled, app.debugDescription)
        let cameraControlFrame = recenterButton.frame
        let cameraControlDiagnostic =
            "cameraControlFrame=\(cameraControlFrame), mapFrame=\(routeMap.frame)\n\(app.debugDescription)"
        XCTAssertEqual(cameraControlFrame.width, 44, accuracy: 0.5, cameraControlDiagnostic)
        XCTAssertEqual(cameraControlFrame.height, 44, accuracy: 0.5, cameraControlDiagnostic)
        XCTAssertTrue(
            routeMap.frame.insetBy(dx: -2, dy: -2).contains(cameraControlFrame),
            "The recenter hit target must stay inside the rendered map. \(cameraControlDiagnostic)"
        )
        XCTAssertTrue(recenterButton.isHittable, app.debugDescription)
        let latestPointMarker = app.descendants(matching: .any)["ride-map.latest-point-marker"]
        XCTAssertTrue(latestPointMarker.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(latestPointMarker.label, "Latest recorded point")
        XCTAssertFalse(app.staticTexts["Current location"].exists, app.debugDescription)
        recenterButton.tap()
        let routeScreenshot = XCTAttachment(screenshot: app.screenshot())
        routeScreenshot.name = "ride-map-rust-backed-point-route-before-marker-center-assertion"
        routeScreenshot.lifetime = .keepAlways
        add(routeScreenshot)
        // The caption group includes overlay text; the native Map pin bounds are the coordinate anchor.
        let latestPointPin = routeMap.otherElements
            .containing(.any, identifier: "ride-map.latest-point-marker")
            .matching(NSPredicate(format: "label == %@", "Map pin"))
            .firstMatch
        XCTAssertTrue(latestPointPin.waitForExistence(timeout: 5), app.debugDescription)
        let markerFrame = latestPointPin.frame
        let mapFrame = routeMap.frame
        let markerCenterDiagnostic =
            "nativeMapPinFrame=\(markerFrame), mapFrame=\(mapFrame)\n\(app.debugDescription)"
        XCTAssertEqual(markerFrame.width, 26, accuracy: 0.5, markerCenterDiagnostic)
        XCTAssertEqual(markerFrame.height, 26, accuracy: 0.5, markerCenterDiagnostic)
        XCTAssertEqual(
            markerFrame.midX, mapFrame.midX, accuracy: 12,
            "The latest recorded point marker stays centered on the single-point route after recentering. \(markerCenterDiagnostic)"
        )
        XCTAssertEqual(
            markerFrame.midY, mapFrame.midY, accuracy: 12,
            "The latest recorded point marker stays centered on the single-point route after recentering. \(markerCenterDiagnostic)"
        )
        let stopButton = app.buttons["ride-map.stop"]
        XCTAssertTrue(stopButton.exists, app.debugDescription)
        XCTAssertTrue(stopButton.isHittable, app.debugDescription)
        XCTAssertTrue(pauseButton.isHittable, app.debugDescription)
        pauseButton.tap()
        let resumeButton = app.buttons["ride-map.resume"]
        XCTAssertTrue(resumeButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(resumeButton.isHittable, app.debugDescription)
        XCTAssertTrue(stopButton.isHittable, app.debugDescription)
        resumeButton.tap()
        XCTAssertTrue(pauseButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(app.descendants(matching: .any)["ride-map.map"].exists, app.debugDescription)
        XCTAssertTrue(recenterButton.isEnabled, app.debugDescription)
    }

    func testPickerSurfaceHomeMapRouteKeepsMapAndLifecycleActionsReachableAtAccessibilityDynamicType() throws {
        let mapButton = app.tabBars.buttons["Map"]
        XCTAssertTrue(mapButton.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertTrue(mapButton.isHittable)
        mapButton.tap()

        let mapScreen = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(mapScreen.waitForExistence(timeout: 8), app.debugDescription)
        let summary = app.descendants(matching: .any)["ride-map.summary"]
        XCTAssertTrue(summary.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(summary.label.contains("Elapsed time"), app.debugDescription)
        XCTAssertTrue(summary.label.contains("Moving time"), app.debugDescription)
        XCTAssertFalse((summary.value as? String ?? "").isEmpty, app.debugDescription)
        XCTAssertFalse(summary.label.contains("Recording time"), app.debugDescription)
        let routeMap = app.descendants(matching: .any)["ride-map.map"]
        XCTAssertTrue(routeMap.waitForExistence(timeout: 8), app.debugDescription)
        let recenterButton = app.buttons["ride-map.recenter"]
        XCTAssertTrue(recenterButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(recenterButton.isEnabled, app.debugDescription)
        let cameraControlFrame = recenterButton.frame
        let cameraControlDiagnostic =
            "cameraControlFrame=\(cameraControlFrame), mapFrame=\(routeMap.frame)\n\(app.debugDescription)"
        XCTAssertEqual(cameraControlFrame.width, 44, accuracy: 0.5, cameraControlDiagnostic)
        XCTAssertEqual(cameraControlFrame.height, 44, accuracy: 0.5, cameraControlDiagnostic)
        XCTAssertTrue(
            routeMap.frame.insetBy(dx: -2, dy: -2).contains(cameraControlFrame),
            "The recenter hit target must stay inside the rendered map. \(cameraControlDiagnostic)"
        )

        let latestPointMarker = app.descendants(matching: .any)["ride-map.latest-point-marker"]
        XCTAssertTrue(latestPointMarker.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(latestPointMarker.label, "Latest recorded point")
        XCTAssertFalse(app.staticTexts["Current location"].exists, app.debugDescription)

        recenterButton.tap()
        let routeImage = XCUIScreen.main.screenshot()
        let routeScreenshot = XCTAttachment(screenshot: routeImage)
        routeScreenshot.name =
            "ride-map-rust-backed-point-route-accessibility-dynamic-type-before-marker-center-assertion"
        routeScreenshot.lifetime = .keepAlways
        add(routeScreenshot)
        assertRouteMarkerCaption(
            "Latest recorded point", in: routeMap.frame, screenshot: routeImage,
            attachmentName: "ride-map-latest-point-caption-accessibility-dynamic-type-OCR"
        )
        // The caption group includes overlay text; the native Map pin bounds are the coordinate anchor.
        let latestPointPin = routeMap.otherElements
            .containing(.any, identifier: "ride-map.latest-point-marker")
            .matching(NSPredicate(format: "label == %@", "Map pin"))
            .firstMatch
        XCTAssertTrue(latestPointPin.waitForExistence(timeout: 5), app.debugDescription)
        let markerFrame = latestPointPin.frame
        let mapFrame = routeMap.frame
        let markerCenterDiagnostic =
            "nativeMapPinFrame=\(markerFrame), mapFrame=\(mapFrame)\n\(app.debugDescription)"
        XCTAssertEqual(markerFrame.width, 26, accuracy: 0.5, markerCenterDiagnostic)
        XCTAssertEqual(markerFrame.height, 26, accuracy: 0.5, markerCenterDiagnostic)
        XCTAssertEqual(
            markerFrame.midX, mapFrame.midX, accuracy: 12,
            "The latest recorded point marker stays centered on the single-point route at accessibility text size. \(markerCenterDiagnostic)"
        )
        XCTAssertEqual(
            markerFrame.midY, mapFrame.midY, accuracy: 12,
            "The latest recorded point marker stays centered on the single-point route at accessibility text size. \(markerCenterDiagnostic)"
        )

        let liveViewport = app.scrollViews["ride-map.live-viewport"]
        XCTAssertTrue(liveViewport.waitForExistence(timeout: 5), app.debugDescription)
        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(tabBar.exists, app.debugDescription)
        let availabilityNotice = app.descendants(matching: .any)["ride-map.location-availability"]
        XCTAssertTrue(availabilityNotice.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(
            availabilityNotice.label,
            "Location access is denied. Enable it in Settings to record a route.",
            app.debugDescription
        )
        scrollElementFrameIntoViewport(
            availabilityNotice, in: liveViewport, maxScrolls: 8, occludedBy: tabBar)
        let noticeScreenshot = XCTAttachment(screenshot: app.screenshot())
        noticeScreenshot.name = "ride-map-denied-location-notice-accessibility-dynamic-type"
        noticeScreenshot.lifetime = .keepAlways
        add(noticeScreenshot)
        let noticeFrame = availabilityNotice.frame
        let viewportFrame = unobscuredFrame(in: liveViewport, above: tabBar)
        let noticeDiagnostic =
            "noticeFrame=\(noticeFrame), viewportFrame=\(viewportFrame), mapFrame=\(routeMap.frame)\n\(app.debugDescription)"
        XCTAssertTrue(
            viewportFrame.insetBy(dx: -2, dy: -2).contains(noticeFrame),
            "The location notice must fit inside the live scroll viewport. \(noticeDiagnostic)"
        )
        XCTAssertGreaterThanOrEqual(
            noticeFrame.minY, routeMap.frame.maxY - 2,
            "The location notice must remain below the rendered route map. \(noticeDiagnostic)"
        )
        XCTAssertLessThanOrEqual(
            noticeFrame.height, viewportFrame.height * 0.30,
            "The visible denial notice must retain a compact footprint at accessibility text size while its full guidance remains available to VoiceOver. \(noticeDiagnostic)"
        )

        let locationSettingsButton = app.buttons["ride-map.location-settings"]
        XCTAssertTrue(locationSettingsButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(locationSettingsButton.label, "Open location settings", app.debugDescription)
        scrollElementFrameIntoViewport(
            locationSettingsButton, in: liveViewport, maxScrolls: 8, occludedBy: tabBar)
        XCTAssertTrue(locationSettingsButton.isEnabled, app.debugDescription)
        let settingsScreenshot = XCTAttachment(screenshot: app.screenshot())
        settingsScreenshot.name = "ride-map-location-settings-accessibility-dynamic-type"
        settingsScreenshot.lifetime = .keepAlways
        add(settingsScreenshot)

        let pauseButton = app.buttons["ride-map.pause"]
        let stopButton = app.buttons["ride-map.stop"]
        scrollElementFrameIntoViewport(pauseButton, in: liveViewport, maxScrolls: 10, occludedBy: tabBar)
        XCTAssertTrue(pauseButton.isEnabled, app.debugDescription)
        XCTAssertTrue(pauseButton.isHittable, app.debugDescription)
        scrollElementFrameIntoViewport(stopButton, in: liveViewport, maxScrolls: 10, occludedBy: tabBar)
        XCTAssertTrue(stopButton.isEnabled, app.debugDescription)
        XCTAssertTrue(stopButton.isHittable, app.debugDescription)
        let controlsScreenshot = XCTAttachment(screenshot: app.screenshot())
        controlsScreenshot.name = "ride-map-lifecycle-controls-accessibility-dynamic-type"
        controlsScreenshot.lifetime = .keepAlways
        add(controlsScreenshot)
    }

    func testHomeMapConfirmedIdleHidesRouteCanvasUntilRideStarts() throws {
        let mapButton = app.tabBars.buttons["Map"]
        XCTAssertTrue(mapButton.waitForExistence(timeout: 8), app.debugDescription)
        mapButton.tap()

        let mapScreen = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(mapScreen.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertTrue(app.staticTexts["No active ride"].waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.map"].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["ride-map.recenter"].exists, app.debugDescription)
        let idleScreenshot = XCTAttachment(screenshot: app.screenshot())
        idleScreenshot.name = "ride-map-confirmed-idle"
        idleScreenshot.lifetime = .keepAlways
        add(idleScreenshot)

        let startButton = app.buttons["ride-map.start"]
        XCTAssertTrue(startButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(startButton.isHittable, app.debugDescription)
        startButton.tap()

        let pauseButton = app.buttons["ride-map.pause"]
        XCTAssertTrue(pauseButton.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.map"].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["ride-map.recenter"].exists, app.debugDescription)
        XCTAssertTrue(app.descendants(matching: .any)["ride-map.recording-pill"].exists, app.debugDescription)
        XCTAssertTrue(app.descendants(matching: .any)["ride-map.summary"].exists, app.debugDescription)
        let stopButton = app.buttons["ride-map.stop"]
        XCTAssertTrue(stopButton.exists, app.debugDescription)
        XCTAssertTrue(stopButton.isHittable, app.debugDescription)
        XCTAssertTrue(pauseButton.isHittable, app.debugDescription)
        pauseButton.tap()
        let resumeButton = app.buttons["ride-map.resume"]
        XCTAssertTrue(resumeButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(resumeButton.isHittable, app.debugDescription)
        XCTAssertTrue(stopButton.isHittable, app.debugDescription)
        resumeButton.tap()
        XCTAssertTrue(pauseButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.map"].exists, app.debugDescription)
        let startedScreenshot = XCTAttachment(screenshot: app.screenshot())
        startedScreenshot.name = "ride-map-after-start"
        startedScreenshot.lifetime = .keepAlways
        add(startedScreenshot)
    }

    func testPickerSurfaceHomeMapRouteSavedHistoryPreservesSelectionAtAccessibilityDynamicTypeShowsEndpoints() throws {
        try assertSavedHistoryEndpointCaptionsAtAccessibilityDynamicType()
    }

    func
        testPickerSurfaceHomeMapRouteSavedHistoryPreservesSelectionAtAccessibilityDynamicTypeRetainsThroughRefreshFailureAndRetry()
        throws
    {
        try assertSavedHistorySelectionSurvivesRefreshFailureAndRetryAtAccessibilityDynamicType()
    }

    func
        testPickerSurfaceHomeMapRouteSavedHistoryPreservesSelectionAtAccessibilityDynamicTypeRetainsThroughRefreshFailureAndRetryInLandscape()
        throws
    {
        try assertSavedHistorySelectionSurvivesRefreshFailureAndRetryAtAccessibilityDynamicType()
    }

    func
        testPickerSurfaceHomeMapRouteSavedHistoryPreservesSelectionAtAccessibilityDynamicTypeShowsEndpointsInLandscape()
        throws
    {
        try assertSavedHistoryEndpointCaptionsAtAccessibilityDynamicType()
    }

    func
        testPickerSurfaceHomeMapRouteSavedHistoryPreservesSelectionAtAccessibilityDynamicTypeKeepsDateValueReadable()
        throws
    {
        try assertSavedHistoryDateFilterReadableAtAccessibilityDynamicType()
    }

    func
        testPickerSurfaceHomeMapRouteSavedHistoryPreservesSelectionAtAccessibilityDynamicTypeKeepsDateValueReadableInLandscape()
        throws
    {
        try assertSavedHistoryDateFilterReadableAtAccessibilityDynamicType()
    }

    private func assertSavedHistoryDateFilterReadableAtAccessibilityDynamicType() throws {
        let mapTab = app.tabBars.buttons["Map"]
        XCTAssertTrue(mapTab.waitForExistence(timeout: 10), app.debugDescription)
        mapTab.tap()

        let screen = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(screen.waitForExistence(timeout: 8), app.debugDescription)
        let readback = try XCTUnwrap(screen.value as? String)
        let receipt = try XCTUnwrap(readback.components(separatedBy: ";").first)
        XCTAssertTrue(receipt.hasPrefix("saved-rides:"), app.debugDescription)
        let seededIDs = receipt.dropFirst("saved-rides:".count).split(separator: ",").map(String.init)
        XCTAssertEqual(seededIDs.count, 2, "This scenario uses the persisted Rust history fixture")
        XCTAssertTrue(seededIDs.allSatisfy { UUID(uuidString: $0) != nil }, app.debugDescription)

        let picker = app.descendants(matching: .any)["ride-map.mode-picker"].segmentedControls.firstMatch
        XCTAssertTrue(picker.waitForExistence(timeout: 5), app.debugDescription)
        picker.buttons["History"].tap()
        let initialGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: 0)

        let date = app.buttons["ride-map.history-date-filter"]
        let clear = app.buttons["ride-map.history-clear-filters"]
        let filters = app.descendants(matching: .any)["ride-map.history-filters"]
        let viewport = screen.scrollViews.firstMatch
        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(viewport.waitForExistence(timeout: 5), app.debugDescription)
        scrollElementFrameIntoViewport(
            date, in: viewport, maxScrolls: 8, occludedBy: tabBar, requiresFullVisibility: true
        )
        assertMinimumControlDimension(date.frame.width)
        assertMinimumControlDimension(date.frame.height)
        XCTAssertTrue(date.isHittable, app.debugDescription)

        assertVisibleHistoryDateText(
            "Last 30 days", in: date,
            attachmentName: "saved-history-date-last-30-days-accessibility-before-filter-changes"
        )

        date.tap()
        let shortRides = app.descendants(matching: .any)["ride-map.history-show-short-rides"]
        XCTAssertTrue(shortRides.waitForExistence(timeout: 5), app.debugDescription)
        scrollHistoryDateMenuOptionIntoView(shortRides)
        assertMinimumControlDimension(shortRides.frame.height)
        XCTAssertTrue(shortRides.isHittable, app.debugDescription)
        shortRides.tap()
        let shortRideGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: initialGeneration)

        XCTAssertTrue(clear.exists, app.debugDescription)
        XCTAssertTrue(clear.isEnabled, app.debugDescription)
        let originalFiltersFrame = filters.frame
        let originalViewportFrame = viewport.frame
        let historyMap = app.descendants(matching: .any)["ride-map.map"]
        XCTAssertTrue(historyMap.waitForExistence(timeout: 5), app.debugDescription)
        let originalMapFrame = historyMap.frame
        XCTAssertGreaterThan(originalMapFrame.height, 0, app.debugDescription)

        date.tap()
        let allTime = app.buttons["All time"]
        XCTAssertTrue(allTime.waitForExistence(timeout: 5), app.debugDescription)
        assertMinimumControlDimension(allTime.frame.width)
        assertMinimumControlDimension(allTime.frame.height)
        XCTAssertTrue(allTime.isHittable, app.debugDescription)
        allTime.tap()
        let allTimeGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: shortRideGeneration)
        assertVisibleHistoryDateText(
            "All time", in: date,
            attachmentName: "saved-history-date-all-time-accessibility"
        )
        XCTAssertEqual(date.label, "All time", app.debugDescription)
        XCTAssertTrue(clear.isEnabled, app.debugDescription)
        assertMinimumControlDimension(clear.frame.width)
        assertMinimumControlDimension(clear.frame.height)
        XCTAssertTrue(clear.isHittable, app.debugDescription)
        assertHistoryFrame(filters.frame, equals: originalFiltersFrame)
        assertHistoryFrame(viewport.frame, equals: originalViewportFrame)
        assertHistoryFrame(historyMap.frame, equals: originalMapFrame)

        date.tap()
        let last30Days = app.buttons["Last 30 days"]
        XCTAssertTrue(last30Days.waitForExistence(timeout: 5), app.debugDescription)
        assertMinimumControlDimension(last30Days.frame.width)
        assertMinimumControlDimension(last30Days.frame.height)
        XCTAssertTrue(last30Days.isHittable, app.debugDescription)
        last30Days.tap()
        _ = try waitForSettledHistory(in: screen, afterQueryGeneration: allTimeGeneration)
        assertVisibleHistoryDateText(
            "Last 30 days", in: date,
            attachmentName: "saved-history-date-last-30-days-accessibility"
        )
        XCTAssertEqual(date.label, "Last 30 days", app.debugDescription)
        XCTAssertTrue(clear.isEnabled, app.debugDescription)
        XCTAssertTrue(clear.isHittable, app.debugDescription)
        assertHistoryFrame(filters.frame, equals: originalFiltersFrame)
        assertHistoryFrame(viewport.frame, equals: originalViewportFrame)
        assertHistoryFrame(historyMap.frame, equals: originalMapFrame)

        let last30Generation = try XCTUnwrap(
            Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["query"].flatMap(UInt64.init)
        )
        clear.tap()
        _ = try waitForSettledHistory(in: screen, afterQueryGeneration: last30Generation)
        assertVisibleHistoryDateText(
            "Last 30 days", in: date,
            attachmentName: "saved-history-date-after-clear-accessibility"
        )
        XCTAssertTrue(clear.exists, app.debugDescription)
        XCTAssertFalse(clear.isEnabled, app.debugDescription)
        assertMinimumControlDimension(clear.frame.width)
        assertMinimumControlDimension(clear.frame.height)
        XCTAssertTrue(filters.frame.contains(clear.frame), app.debugDescription)
        XCTAssertEqual(date.label, "Last 30 days", app.debugDescription)
        XCTAssertTrue(
            app.descendants(matching: .any)["ride-map.history-empty"].waitForExistence(timeout: 5), app.debugDescription
        )
        XCTAssertFalse(historyMap.exists, "Clearing also hides the Rust fixture's short routes")
        assertMinimumControlDimension(date.frame.width)
        assertMinimumControlDimension(date.frame.height)
        XCTAssertTrue(date.isHittable, app.debugDescription)
        assertHistoryFrame(filters.frame, equals: originalFiltersFrame)
        assertHistoryFrame(viewport.frame, equals: originalViewportFrame)

        let vehicle = app.buttons["ride-map.history-vehicle-filter"]
        scrollElementFrameIntoViewport(
            vehicle, in: viewport, maxScrolls: 8, occludedBy: tabBar, requiresFullVisibility: true
        )
        assertMinimumControlDimension(vehicle.frame.width)
        assertMinimumControlDimension(vehicle.frame.height)
        XCTAssertTrue(vehicle.isHittable, app.debugDescription)
        let controlsAttachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        controlsAttachment.name = "saved-history-vehicle-control-accessibility"
        controlsAttachment.lifetime = .keepAlways
        add(controlsAttachment)
    }

    private func scrollHistoryDateMenuOptionIntoView(_ option: XCUIElement) {
        let menu = app.collectionViews.firstMatch
        XCTAssertTrue(menu.waitForExistence(timeout: 5), app.debugDescription)
        // The collection's AX frame can extend below the clipped system popup.
        // Use its smallest enclosing container and drag in the scroll-indicator gutter.
        let containers = app.otherElements.containing(.collectionView, identifier: nil)
            .allElementsBoundByIndex.map { (element: $0, frame: $0.frame) }
            .filter { $0.frame.width > 0 && $0.frame.height > 0 }
        guard
            let container = containers.min(by: {
                $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height
            })
        else {
            XCTFail("Missing native date-menu popup container. \(app.debugDescription)")
            return
        }
        let popup = container.element
        let optionFrame = option.exists ? String(describing: option.frame) : "absent"
        let diagnostic = XCTAttachment(
            string: "Native menu=\(menu.frame); popup=\(container.frame); option=\(optionFrame)"
        )
        diagnostic.name = "saved-history-native-date-menu-frames"
        diagnostic.lifetime = .keepAlways
        add(diagnostic)
        let before = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        before.name = "saved-history-date-menu-before-scroll"
        before.lifetime = .keepAlways
        add(before)
        scrollElementFrameIntoViewport(
            option, in: popup, maxScrolls: 8, horizontalFraction: 0.96
        )
        XCTAssertTrue(menu.exists, "Scrolling must preserve the native date menu")
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = "saved-history-date-menu-option-reachable"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private func assertVisibleHistoryDateText(
        _ expectedText: String,
        in element: XCUIElement,
        attachmentName: String
    ) {
        let screenshot = XCUIScreen.main.screenshot()
        let screenshotAttachment = XCTAttachment(screenshot: screenshot)
        screenshotAttachment.name = attachmentName
        screenshotAttachment.lifetime = .keepAlways
        add(screenshotAttachment)

        let screenshotImage = screenshot.image
        guard let image = screenshotImage.cgImage,
            let orientation = visionOrientation(for: screenshotImage.imageOrientation)
        else {
            XCTFail("Missing orientation-aware simulator screenshot for the selected History date")
            return
        }
        let canvas = app.frame
        let visibleFrame = element.frame.intersection(canvas)
        guard canvas.width > 0, canvas.height > 0, !visibleFrame.isNull, !visibleFrame.isEmpty else {
            XCTFail("Selected History date has no visible frame: element=\(element.frame), canvas=\(canvas)")
            return
        }
        let region = CGRect(
            x: (visibleFrame.minX - canvas.minX) / canvas.width,
            y: 1 - (visibleFrame.maxY - canvas.minY) / canvas.height,
            width: visibleFrame.width / canvas.width,
            height: visibleFrame.height / canvas.height
        ).intersection(CGRect(x: 0, y: 0, width: 1, height: 1))
        let expectedWords = Set(expectedText.lowercased().split(whereSeparator: \.isWhitespace).map(String.init))
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["en-US"]
        request.usesLanguageCorrection = false
        request.minimumTextHeight = 0
        request.regionOfInterest = region

        do {
            try VNImageRequestHandler(cgImage: image, orientation: orientation, options: [:]).perform([request])
            let lines = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
            let recognized = lines.joined(separator: " ").lowercased()
                .filter { $0.isLetter || $0.isNumber || $0.isWhitespace }
            let recognizedWords = Set(recognized.split(whereSeparator: \.isWhitespace).map(String.init))
            let diagnostic =
                "expected=\(expectedText); recognized=\(lines); frame=\(visibleFrame); canvas=\(canvas); "
                + "region=\(region); orientation=\(screenshotImage.imageOrientation); "
                + "imageSize=\(screenshotImage.size); scale=\(screenshotImage.scale); pixels=\(image.width)x\(image.height)"
            let diagnosticAttachment = XCTAttachment(string: diagnostic)
            diagnosticAttachment.name = attachmentName + "-recognition"
            diagnosticAttachment.lifetime = .keepAlways
            add(diagnosticAttachment)
            for word in expectedWords {
                XCTAssertTrue(recognizedWords.contains(word), "Selected date must be fully visible: \(diagnostic)")
            }
        } catch {
            XCTFail("Selected History date recognition failed: \(error)")
        }
    }

    private func assertSavedHistoryEndpointCaptionsAtAccessibilityDynamicType() throws {
        let mapTab = app.tabBars.buttons["Map"]
        XCTAssertTrue(mapTab.waitForExistence(timeout: 10), app.debugDescription)
        mapTab.tap()

        let screen = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(screen.waitForExistence(timeout: 8), app.debugDescription)
        let readback = try XCTUnwrap(screen.value as? String)
        let receipt = try XCTUnwrap(readback.components(separatedBy: ";").first)
        XCTAssertTrue(receipt.hasPrefix("saved-rides:"), app.debugDescription)
        let seededIDs = receipt.dropFirst("saved-rides:".count).split(separator: ",").map(String.init)
        XCTAssertEqual(seededIDs.count, 2, "The UI test reads the two Rust fixture receipts")
        let newestSeededID = try XCTUnwrap(seededIDs.last)
        XCTAssertNotNil(UUID(uuidString: newestSeededID))

        let modePicker = app.descendants(matching: .any)["ride-map.mode-picker"].segmentedControls.firstMatch
        XCTAssertTrue(modePicker.waitForExistence(timeout: 5), app.debugDescription)
        modePicker.buttons["History"].tap()
        let initialGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: 0)

        let dateFilter = app.buttons["ride-map.history-date-filter"]
        XCTAssertTrue(dateFilter.waitForExistence(timeout: 5), app.debugDescription)
        dateFilter.tap()
        let shortRidesToggle = app.descendants(matching: .any)["ride-map.history-show-short-rides"]
        XCTAssertTrue(shortRidesToggle.waitForExistence(timeout: 5), app.debugDescription)
        shortRidesToggle.tap()
        let shortRideGeneration = try waitForSettledHistory(
            in: screen, afterQueryGeneration: initialGeneration
        )
        let listedIDs = Set(
            Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["listed"]?
                .split(separator: ",").map(String.init) ?? []
        )
        XCTAssertTrue(listedIDs.contains(newestSeededID), "The newest fixture ride should be on the first page")

        let historyScroll = screen.scrollViews.firstMatch
        XCTAssertTrue(historyScroll.waitForExistence(timeout: 5), app.debugDescription)
        let newestRide = app.buttons["ride-map.history-\(newestSeededID)"]
        let tabBar = app.tabBars.firstMatch
        scrollElementFrameIntoViewport(
            newestRide, in: historyScroll, maxScrolls: 20, occludedBy: tabBar,
            requiresFullVisibility: false
        )
        XCTAssertTrue(newestRide.isHittable, app.debugDescription)
        XCTAssertTrue(newestRide.label.contains("Elapsed time:"), app.debugDescription)
        XCTAssertTrue(newestRide.label.contains("Moving time:"), app.debugDescription)
        let historyScreenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        historyScreenshot.name = "saved-history-newest-rust-ride-row-accessibility-dynamic-type"
        historyScreenshot.lifetime = .keepAlways
        add(historyScreenshot)
        newestRide.tap()

        let detailScreen = app.descendants(matching: .any)["ride-map.detail-screen"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 8), app.debugDescription)
        _ = try waitForSettledHistory(
            in: detailScreen,
            afterQueryGeneration: shortRideGeneration - 1,
            selecting: newestSeededID
        )
        XCTAssertTrue(
            detailScreen.descendants(matching: .any)["Elapsed time"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(detailScreen.descendants(matching: .any)["Moving time"].exists, app.debugDescription)
        let detailMap = detailScreen.descendants(matching: .any)["ride-map.map"]
        XCTAssertTrue(detailMap.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.detail-initial-loading"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.detail-no-points"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.detail-map-error"].exists)

        let detailScreenshot = XCUIScreen.main.screenshot()
        let detailAttachment = XCTAttachment(screenshot: detailScreenshot)
        detailAttachment.name = "saved-history-rust-backed-endpoint-captions-accessibility-dynamic-type"
        detailAttachment.lifetime = .keepAlways
        add(detailAttachment)

        let startRole = detailMap.otherElements.matching(NSPredicate(format: "label == %@", "Start"))
        let endRole = detailMap.otherElements.matching(NSPredicate(format: "label == %@", "Ride end"))
        XCTAssertTrue(startRole.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(endRole.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(startRole.count, 1, app.debugDescription)
        XCTAssertEqual(endRole.count, 1, app.debugDescription)
        XCTAssertEqual(startRole.firstMatch.label, "Start", app.debugDescription)
        XCTAssertEqual(endRole.firstMatch.label, "Ride end", app.debugDescription)

        let startMapPin = detailMap.otherElements
            .containing(NSPredicate(format: "label == %@", "Start"))
            .matching(NSPredicate(format: "label == %@", "Map pin"))
            .firstMatch
        let endMapPin = detailMap.otherElements
            .containing(NSPredicate(format: "label == %@", "Ride end"))
            .matching(NSPredicate(format: "label == %@", "Map pin"))
            .firstMatch
        XCTAssertTrue(startMapPin.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(endMapPin.waitForExistence(timeout: 5), app.debugDescription)
        let mapFrame = detailMap.frame
        let pinFrames = [startMapPin.frame, endMapPin.frame]
        for pinFrame in pinFrames {
            XCTAssertEqual(pinFrame.width, 26, accuracy: 0.5, app.debugDescription)
            XCTAssertEqual(pinFrame.height, 26, accuracy: 0.5, app.debugDescription)
            XCTAssertTrue(mapFrame.insetBy(dx: -2, dy: -2).contains(pinFrame), app.debugDescription)
        }
        XCTAssertGreaterThan(
            hypot(pinFrames[0].midX - pinFrames[1].midX, pinFrames[0].midY - pinFrames[1].midY), 1,
            "The seeded Rust route endpoints should have distinct rendered pin positions. \(app.debugDescription)"
        )

        let startCaptionFrame = try XCTUnwrap(
            assertRouteMarkerCaption(
                "Start", in: mapFrame, screenshot: detailScreenshot,
                attachmentName: "saved-history-start-caption-accessibility-dynamic-type"
            )
        )
        let endCaptionFrame = try XCTUnwrap(
            assertRouteMarkerCaption(
                "Ride end", in: mapFrame, screenshot: detailScreenshot,
                attachmentName: "saved-history-end-caption-accessibility-dynamic-type"
            )
        )
        XCTAssertFalse(
            startCaptionFrame.intersects(endCaptionFrame),
            "Endpoint caption text should not overlap on the shared History canvas. "
                + "start=\(startCaptionFrame), end=\(endCaptionFrame), map=\(mapFrame)"
        )
    }

    private func assertSavedHistorySelectionSurvivesRefreshFailureAndRetryAtAccessibilityDynamicType() throws {
        let mapTab = app.tabBars.buttons["Map"]
        XCTAssertTrue(mapTab.waitForExistence(timeout: 10), app.debugDescription)
        mapTab.tap()

        let screen = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(screen.waitForExistence(timeout: 8), app.debugDescription)
        let initialReadback = try XCTUnwrap(screen.value as? String)
        let receipt = try XCTUnwrap(initialReadback.components(separatedBy: ";").first)
        XCTAssertTrue(receipt.hasPrefix("saved-rides:"), app.debugDescription)
        let seededIDs = receipt.dropFirst("saved-rides:".count).split(separator: ",").map(String.init)
        XCTAssertEqual(seededIDs.count, 2, "The UI test reads the two Rust fixture receipts")
        let newestSeededID = try XCTUnwrap(seededIDs.last)
        XCTAssertNotNil(UUID(uuidString: newestSeededID))

        let modePicker = app.descendants(matching: .any)["ride-map.mode-picker"].segmentedControls.firstMatch
        XCTAssertTrue(modePicker.waitForExistence(timeout: 5), app.debugDescription)
        modePicker.buttons["History"].tap()
        let initialGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: 0)

        // The selected short ride is initially hidden. Show short rides while retaining Last 30 days,
        // then open its real Rust-projected detail before changing the date to All time.
        let dateFilter = app.buttons["ride-map.history-date-filter"]
        XCTAssertTrue(dateFilter.waitForExistence(timeout: 5), app.debugDescription)
        dateFilter.tap()
        let shortRidesToggle = app.descendants(matching: .any)["ride-map.history-show-short-rides"]
        XCTAssertTrue(shortRidesToggle.waitForExistence(timeout: 5), app.debugDescription)
        scrollHistoryDateMenuOptionIntoView(shortRidesToggle)
        shortRidesToggle.tap()
        let shortRideGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: initialGeneration)
        let newestRide = app.buttons["ride-map.history-\(newestSeededID)"]
        let historyScroll = screen.scrollViews.firstMatch
        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(historyScroll.waitForExistence(timeout: 5), app.debugDescription)
        scrollElementFrameIntoViewport(
            newestRide,
            in: historyScroll,
            maxScrolls: 20,
            occludedBy: tabBar,
            requiresFullVisibility: false,
            horizontalFraction: 0.5
        )
        XCTAssertTrue(newestRide.isHittable, app.debugDescription)
        newestRide.tap()

        let detailScreen = app.descendants(matching: .any)["ride-map.detail-screen"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 8), app.debugDescription)
        _ = try waitForSettledHistory(
            in: detailScreen,
            afterQueryGeneration: shortRideGeneration - 1,
            selecting: newestSeededID
        )
        let detailHeader = detailScreen.descendants(matching: .any)["ride-map.detail-header"]
        let backButton = detailHeader.buttons.firstMatch
        XCTAssertTrue(backButton.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(backButton.isHittable, app.debugDescription)
        backButton.tap()

        XCTAssertTrue(dateFilter.waitForExistence(timeout: 5), app.debugDescription)
        let settledSelectionGeneration = try waitForSettledHistory(
            in: screen,
            afterQueryGeneration: shortRideGeneration - 1,
            selecting: newestSeededID
        )
        XCTAssertEqual(newestRide.value as? String, "Selected", app.debugDescription)
        scrollElementFrameIntoViewport(
            dateFilter,
            in: historyScroll,
            maxScrolls: 20,
            occludedBy: tabBar,
            requiresFullVisibility: true,
            horizontalFraction: 0.5
        )
        let filters = app.descendants(matching: .any)["ride-map.history-filters"]
        let viewport = historyScroll
        let historyMap = app.descendants(matching: .any)["ride-map.map"]
        XCTAssertTrue(filters.exists, app.debugDescription)
        XCTAssertTrue(viewport.exists, app.debugDescription)
        XCTAssertTrue(historyMap.exists, app.debugDescription)
        let originalFiltersFrame = filters.frame
        let originalViewportFrame = viewport.frame
        let originalMapFrame = historyMap.frame
        XCTAssertGreaterThan(originalViewportFrame.height, 0, app.debugDescription)
        XCTAssertGreaterThan(originalMapFrame.height, 0, app.debugDescription)
        let originalFields = Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))
        let originalListedIDs = Set(originalFields["listed"]?.split(separator: ",").map(String.init) ?? [])
        XCTAssertTrue(originalListedIDs.contains(newestSeededID), app.debugDescription)

        attachHistoryRefreshEvidence(
            named: "saved-history-before-refresh-failure",
            screen: screen,
            viewport: viewport,
            filters: filters,
            map: historyMap,
            error: app.descendants(matching: .any)["ride-map.history-refresh-error"],
            retry: app.buttons["ride-map.history-refresh-retry"]
        )
        dateFilter.tap()
        let allTime = app.buttons["All time"]
        XCTAssertTrue(allTime.waitForExistence(timeout: 5), app.debugDescription)
        allTime.tap()
        let failedGeneration = try waitForSettledHistory(
            in: screen,
            afterQueryGeneration: settledSelectionGeneration,
            selecting: newestSeededID
        )

        let refreshError = app.descendants(matching: .any)["ride-map.history-refresh-error"]
        let mapError = app.descendants(matching: .any)["ride-map.history-map-error"]
        let retryButton = app.buttons["ride-map.history-refresh-retry"]
        let loadMoreButton = app.buttons["ride-map.history-load-more"]
        attachHistoryRefreshEvidence(
            named: "saved-history-all-time-refresh-failed",
            screen: screen,
            viewport: viewport,
            filters: filters,
            map: historyMap,
            error: refreshError,
            retry: retryButton
        )

        let failedReadback = try XCTUnwrap(screen.value as? String)
        let failedFields = Self.historyFixtureFields(from: failedReadback)
        let failedListedIDs = Set(failedFields["listed"]?.split(separator: ",").map(String.init) ?? [])
        XCTAssertEqual(dateFilter.label, "All time", app.debugDescription)
        XCTAssertEqual(failedFields["loading"], "false", app.debugDescription)
        XCTAssertEqual(failedFields["selected"], newestSeededID, app.debugDescription)
        XCTAssertEqual(failedFields["projection"], newestSeededID, app.debugDescription)
        XCTAssertEqual(failedListedIDs, originalListedIDs, app.debugDescription)
        XCTAssertGreaterThan(failedGeneration, settledSelectionGeneration)
        XCTAssertTrue(refreshError.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(
            mapError.exists,
            "A retained route with display points must not show the empty-map error. \(app.debugDescription)")
        XCTAssertTrue(retryButton.exists, app.debugDescription)
        XCTAssertEqual(retryButton.label, "Try again", app.debugDescription)
        assertHistoryFrame(filters.frame, equals: originalFiltersFrame)
        assertHistoryFrame(viewport.frame, equals: originalViewportFrame)
        assertHistoryFrame(historyMap.frame, equals: originalMapFrame)

        let retainedRow = app.buttons["ride-map.history-\(newestSeededID)"]
        scrollHistoryElement(
            retainedRow,
            in: historyScroll,
            above: tabBar,
            and: refreshError,
            minimumVisibleHeight: 44,
            requiresFullVisibility: false
        )
        attachHistoryRefreshEvidence(
            named: "saved-history-retained-row-reachable-after-refresh-error",
            screen: screen,
            viewport: viewport,
            filters: filters,
            map: historyMap,
            error: refreshError,
            retry: retryButton
        )
        let retainedRowVisibleFrame = retainedRow.frame.intersection(
            historyContentFrameAboveOverlays(in: historyScroll, tabBar: tabBar, error: refreshError)
        )
        XCTAssertGreaterThanOrEqual(retainedRowVisibleFrame.height, 44, app.debugDescription)
        XCTAssertTrue(retainedRow.isHittable, app.debugDescription)

        scrollHistoryElement(
            loadMoreButton,
            in: historyScroll,
            above: tabBar,
            and: refreshError,
            minimumVisibleHeight: 44,
            requiresFullVisibility: true
        )
        attachHistoryRefreshEvidence(
            named: "saved-history-load-more-and-refresh-error-accessibility-layout",
            screen: screen,
            viewport: viewport,
            filters: filters,
            map: historyMap,
            error: refreshError,
            retry: retryButton
        )
        XCTAssertTrue(loadMoreButton.waitForExistence(timeout: 5), app.debugDescription)
        assertMinimumControlDimension(loadMoreButton.frame.height)
        let loadMoreSafeFrame = historyContentFrameAboveOverlays(
            in: historyScroll, tabBar: tabBar, error: refreshError
        )
        XCTAssertTrue(
            loadMoreSafeFrame.contains(loadMoreButton.frame),
            "Load more must be fully visible above both overlays. \(app.debugDescription)")
        XCTAssertTrue(loadMoreButton.isHittable, app.debugDescription)

        scrollElementFrameIntoViewport(
            retryButton,
            in: historyScroll,
            maxScrolls: 1,
            occludedBy: tabBar,
            requiresFullVisibility: true,
            horizontalFraction: 0.5
        )
        assertMinimumControlDimension(retryButton.frame.width)
        assertMinimumControlDimension(retryButton.frame.height)
        XCTAssertTrue(retryButton.isEnabled, app.debugDescription)
        XCTAssertTrue(retryButton.isHittable, app.debugDescription)
        retryButton.tap()

        let retriedGeneration = try waitForSettledHistory(
            in: screen,
            afterQueryGeneration: failedGeneration,
            selecting: newestSeededID
        )
        attachHistoryRefreshEvidence(
            named: "saved-history-all-time-refresh-retried",
            screen: screen,
            viewport: viewport,
            filters: filters,
            map: historyMap,
            error: refreshError,
            retry: retryButton
        )
        let retriedFields = Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))
        let retriedListedIDs = Set(retriedFields["listed"]?.split(separator: ",").map(String.init) ?? [])
        scrollElementFrameIntoViewport(
            dateFilter,
            in: historyScroll,
            maxScrolls: 30,
            occludedBy: tabBar,
            requiresFullVisibility: true,
            horizontalFraction: 0.5
        )
        XCTAssertGreaterThan(retriedGeneration, failedGeneration)
        XCTAssertEqual(retriedFields["loading"], "false", app.debugDescription)
        XCTAssertEqual(retriedFields["selected"], newestSeededID, app.debugDescription)
        XCTAssertEqual(retriedFields["projection"], newestSeededID, app.debugDescription)
        XCTAssertEqual(retriedListedIDs, originalListedIDs, app.debugDescription)
        XCTAssertEqual(dateFilter.label, "All time", app.debugDescription)
        XCTAssertFalse(refreshError.exists, app.debugDescription)
        XCTAssertFalse(mapError.exists, app.debugDescription)
        XCTAssertEqual(filters.frame.width, originalFiltersFrame.width, accuracy: 1)
        XCTAssertEqual(filters.frame.height, originalFiltersFrame.height, accuracy: 1)
        assertHistoryFrame(viewport.frame, equals: originalViewportFrame)
        XCTAssertEqual(historyMap.frame.width, originalMapFrame.width, accuracy: 1)
        XCTAssertEqual(historyMap.frame.height, originalMapFrame.height, accuracy: 1)
    }

    private func attachHistoryRefreshEvidence(
        named name: String,
        screen: XCUIElement,
        viewport: XCUIElement,
        filters: XCUIElement,
        map: XCUIElement,
        error: XCUIElement,
        retry: XCUIElement
    ) {
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = name
        screenshot.lifetime = .keepAlways
        add(screenshot)
        let diagnostic = XCTAttachment(
            string: """
                History readback: \(screen.value as? String ?? "<nil>")
                Viewport frame: \(viewport.frame)
                Filters frame: \(filters.frame)
                Map frame: \(map.frame)
                Error frame: \(error.exists ? String(describing: error.frame) : "absent"), exists: \(error.exists)
                Retry frame: \(retry.exists ? String(describing: retry.frame) : "absent"), exists: \(retry.exists), label: \(retry.exists ? retry.label : "absent")
                \(app.debugDescription)
                """)
        diagnostic.name = "\(name)-accessibility-diagnostics"
        diagnostic.lifetime = .keepAlways
        add(diagnostic)
    }

    private func scrollHistoryElement(
        _ element: XCUIElement,
        in historyScroll: XCUIElement,
        above tabBar: XCUIElement,
        and refreshError: XCUIElement,
        minimumVisibleHeight: CGFloat,
        requiresFullVisibility: Bool
    ) {
        var previousFrame: CGRect?
        var consecutiveStalls = 0
        for _ in 0..<40 {
            let safeFrame = historyContentFrameAboveOverlays(
                in: historyScroll, tabBar: tabBar, error: refreshError
            )
            if safeFrame.height < minimumVisibleHeight { break }
            let elementFrame = element.exists ? element.frame : .zero
            let visibleFrame = elementFrame.intersection(safeFrame)
            let isVisible =
                requiresFullVisibility
                ? safeFrame.contains(elementFrame)
                : visibleFrame.height >= minimumVisibleHeight
            if element.exists, isVisible, element.isHittable { break }

            let isAbove = element.exists && elementFrame.maxY < safeFrame.minY
            let travel = safeFrame.height * 0.25
            dragVertically(
                in: historyScroll,
                from: safeFrame.midY + (isAbove ? -travel : travel),
                to: safeFrame.midY + (isAbove ? travel : -travel),
                horizontalFraction: 0.5
            )

            if element.exists {
                if previousFrame == element.frame {
                    consecutiveStalls += 1
                } else {
                    consecutiveStalls = 0
                }
                previousFrame = element.frame
                if consecutiveStalls >= 3 { break }
            }
        }
    }

    private func historyContentFrameAboveOverlays(
        in historyScroll: XCUIElement, tabBar: XCUIElement, error: XCUIElement
    ) -> CGRect {
        let viewport = unobscuredFrame(in: historyScroll, above: tabBar)
        guard error.exists else { return viewport }
        let maxY = min(viewport.maxY, error.frame.minY)
        return CGRect(
            x: viewport.minX,
            y: viewport.minY,
            width: viewport.width,
            height: max(0, maxY - viewport.minY)
        )
    }

    func testPickerSurfaceHomeMapRouteAccountDeletionRetainsLocalData() async throws {
        try await assertAccountDeletionRetainsLocalData(createsCapture: true)
    }

    func testPickerSurfaceHomeMapRouteAccountDeletionAtAccessibilityDynamicTypeRetainsLocalData() async throws {
        try await assertAccountDeletionRetainsLocalData(createsCapture: false)
    }

    private func assertAccountDeletionRetainsLocalData(createsCapture: Bool) async throws {
        executionTimeAllowance = 360
        var captureID: String?
        if createsCapture {
            enterCapture()
            let finish = app.buttons["capture.stop"]
            XCTAssertTrue(finish.waitForExistence(timeout: 5), app.debugDescription)
            finish.tap()
            XCTAssertTrue(
                app.descendants(matching: .any)["captures.detail"].waitForExistence(timeout: 10), app.debugDescription)
            app.navigationBars.buttons.firstMatch.tap()
            let capture = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "captures.database."))
                .firstMatch
            XCTAssertTrue(capture.waitForExistence(timeout: 10), app.debugDescription)
            captureID = capture.identifier
            capture.tap()
            assertStoredCaptureDetail()
            app.navigationBars.buttons.firstMatch.tap()
            app.buttons["setup.done"].tap()
        }
        app.tabBars.buttons["Map"].tap()
        let map = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(map.waitForExistence(timeout: 10), app.debugDescription)
        let rideReceipt = try XCTUnwrap((map.value as? String)?.components(separatedBy: ";").first)
        XCTAssertTrue(rideReceipt.hasPrefix("saved-rides:"), app.debugDescription)
        let seededRideIDs = rideReceipt.dropFirst("saved-rides:".count).split(separator: ",")
        XCTAssertEqual(seededRideIDs.count, 2, "The preservation check requires saved ride identifiers")
        app.tabBars.buttons["Devices"].tap()
        openAccountSetup()
        let delete = app.buttons["setup.account.delete"]
        let setup = app.descendants(matching: .any)["setup.screen"]
        scrollElementFrameIntoViewport(delete, in: setup, maxScrolls: 12)
        XCTAssertTrue(delete.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(delete.isHittable, app.debugDescription)
        delete.tap()
        let confirmation = app.descendants(matching: .any)["setup.account.confirmation"]
        XCTAssertTrue(confirmation.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(
            confirmation.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Anonymous")).firstMatch
                .exists, app.debugDescription)
        let cancel = app.buttons["setup.account.cancel-delete"]
        XCTAssertTrue(cancel.isHittable, app.debugDescription)
        let consent = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        consent.name = "account-deletion-anonymous-public-contributions-confirmation"
        consent.lifetime = .keepAlways
        add(consent)
        cancel.tap()
        try await assertAccountFixtureAuthorization(status: 403)
        XCTAssertTrue(delete.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["setup.account.deleted"].exists)
        delete.tap()
        let submit = app.buttons["setup.account.confirm-delete"]
        scrollElementFrameIntoViewport(submit, in: confirmation, maxScrolls: 12)
        XCTAssertTrue(submit.isHittable, app.debugDescription)
        submit.tap()
        let completed = app.staticTexts["setup.account.deleted"]
        scrollElementFrameIntoViewport(completed, in: setup, maxScrolls: 12)
        XCTAssertTrue(completed.waitForExistence(timeout: 30), app.debugDescription)
        try await assertAccountFixtureAuthorization(status: 401)
        XCTAssertFalse(delete.exists, app.debugDescription)
        let result = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        result.name = "account-deletion-confirmed-with-public-contributions-retained"
        result.lifetime = .keepAlways
        add(result)
        app.terminate()
        app.launch()
        openAccountSetup()
        scrollElementFrameIntoViewport(completed, in: setup, maxScrolls: 12)
        XCTAssertTrue(completed.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(delete.exists, app.debugDescription)
        if let captureID {
            let captures = app.buttons["setup.captures"]
            scrollElementFrameIntoViewport(captures, in: setup, maxScrolls: 12)
            captures.tap()
            let retainedCapture = app.buttons[captureID]
            XCTAssertTrue(retainedCapture.waitForExistence(timeout: 10), app.debugDescription)
            retainedCapture.tap()
            assertStoredCaptureDetail()
            app.navigationBars.buttons.firstMatch.tap()
        }
        app.buttons["setup.done"].tap()
        app.tabBars.buttons["Map"].tap()
        XCTAssertTrue(map.waitForExistence(timeout: 10), app.debugDescription)
        let retainedRideReceipt = try XCTUnwrap((map.value as? String)?.components(separatedBy: ";").first)
        XCTAssertEqual(retainedRideReceipt, rideReceipt, "Deletion must not make the fixture reseed missing rides")
    }

    private func assertStoredCaptureDetail() {
        XCTAssertTrue(app.navigationBars["Recording"].waitForExistence(timeout: 10), app.debugDescription)
        let export = app.buttons["Export recording"]
        XCTAssertTrue(export.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(export.isEnabled, app.debugDescription)
    }

    private func assertAccountFixtureAuthorization(status: Int) async throws {
        let origin = try XCTUnwrap(app.launchEnvironment["CUTOUT_UI_TEST_ACCOUNT_BASE_URL"])
        let url = try XCTUnwrap(URL(string: origin + "v1/moderation/audit"))
        let token = try XCTUnwrap(app.launchEnvironment["CUTOUT_UI_TEST_ACCOUNT_TOKEN"])
        var request = URLRequest(url: url, timeoutInterval: 10)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        let (_, response) = try await URLSession.shared.data(for: request)
        // A contributor is authenticated but forbidden from moderation; after deletion it is unauthorized.
        XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, status)
    }

    private func openAccountSetup() {
        let gear = app.buttons["device-picker.open-setup"]
        XCTAssertTrue(gear.waitForExistence(timeout: 8), app.debugDescription)
        gear.tap()
        XCTAssertTrue(
            app.descendants(matching: .any)["setup.screen"].waitForExistence(timeout: 8), app.debugDescription)
    }

    func testPickerSurfaceSavedHistoryPreservesSelectionAndClearsFiltersWithoutReflow() throws {
        // Traverse both pages and verify selection, reentry, filters, and relaunch in one scenario.
        executionTimeAllowance = 360
        let mapTab = app.tabBars.buttons["Map"]
        XCTAssertTrue(mapTab.waitForExistence(timeout: 10), app.debugDescription)
        mapTab.tap()
        let screen = app.descendants(matching: .any)["ride-map.screen"]
        XCTAssertTrue(screen.waitForExistence(timeout: 8), app.debugDescription)
        let readback = try XCTUnwrap(screen.value as? String)
        let receipt = try XCTUnwrap(readback.components(separatedBy: ";").first)
        XCTAssertTrue(receipt.hasPrefix("saved-rides:"), app.debugDescription)
        let seededIDs = receipt.dropFirst("saved-rides:".count).split(separator: ",").map(String.init)
        let selectedID = try XCTUnwrap(seededIDs.first)
        XCTAssertEqual(seededIDs.count, 2, "This checks fixture receipts, not the full stored history count")
        XCTAssertNotNil(UUID(uuidString: selectedID))
        XCTAssertNotEqual(seededIDs.first, seededIDs.last)

        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "ride-map.screen").count, 1)
        let picker = app.descendants(matching: .any)["ride-map.mode-picker"].segmentedControls.firstMatch
        XCTAssertTrue(picker.waitForExistence(timeout: 5), app.debugDescription)
        picker.buttons["History"].tap()
        let viewport = screen.scrollViews.firstMatch
        XCTAssertTrue(viewport.waitForExistence(timeout: 5), app.debugDescription)
        let scroll = viewport
        XCTAssertTrue(scroll.waitForExistence(timeout: 5), app.debugDescription)
        let defaultGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: 0)
        let filters = app.descendants(matching: .any)["ride-map.history-filters"]
        let date = app.buttons["ride-map.history-date-filter"]
        let clear = app.buttons["ride-map.history-clear-filters"]
        XCTAssertTrue(date.isHittable, app.debugDescription)
        XCTAssertFalse(clear.isEnabled)
        let initialFilterFrame = filters.frame
        let initialViewportFrame = viewport.frame
        XCTAssertGreaterThan(initialViewportFrame.height, 0)

        let initiallyListedIDs = Set(
            Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["listed"]?
                .split(separator: ",").map(String.init) ?? []
        )
        XCTAssertTrue(initiallyListedIDs.isDisjoint(with: seededIDs), "Short rides are hidden by default")

        date.tap()
        let showShortRides = app.descendants(matching: .any)["ride-map.history-show-short-rides"]
        XCTAssertTrue(showShortRides.waitForExistence(timeout: 5), app.debugDescription)
        showShortRides.tap()
        let shortRidesGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: defaultGeneration)
        let shortRideListedIDs = Set(
            Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["listed"]?
                .split(separator: ",").map(String.init) ?? []
        )
        let newestSeededID = try XCTUnwrap(seededIDs.last)
        XCTAssertTrue(shortRideListedIDs.contains(newestSeededID), "The session override reveals short rides")
        assertHistoryFrame(filters.frame, equals: initialFilterFrame)
        assertHistoryFrame(viewport.frame, equals: initialViewportFrame)

        let allTime = app.buttons["All time"]
        if !allTime.exists {
            date.tap()
        }
        XCTAssertTrue(allTime.waitForExistence(timeout: 5), app.debugDescription)
        allTime.tap()
        let filterApplied = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", "All time"), object: date
        )
        XCTAssertEqual(XCTWaiter.wait(for: [filterApplied], timeout: 5), .completed, app.debugDescription)
        let initialGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: shortRidesGeneration)
        XCTAssertTrue(clear.isEnabled)
        assertHistoryFrame(filters.frame, equals: initialFilterFrame)
        assertHistoryFrame(viewport.frame, equals: initialViewportFrame)
        let firstPage = Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["listed"] ?? ""
        XCTAssertFalse(firstPage.split(separator: ",").map(String.init).contains(selectedID))
        let loadMore = app.buttons["ride-map.history-load-more"]
        scrollElementFrameIntoViewport(loadMore, in: scroll, maxScrolls: 60)
        assertMinimumControlDimension(loadMore.frame.height)
        loadMore.tap()
        let selectedRow = app.buttons["ride-map.history-\(selectedID)"]
        scrollElementFrameIntoViewport(selectedRow, in: scroll, maxScrolls: 60)
        assertMinimumControlDimension(selectedRow.frame.height)
        selectedRow.tap()

        let detailScreen = app.descendants(matching: .any)["ride-map.detail-screen"]
        XCTAssertTrue(detailScreen.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "ride-map.detail-screen").count, 1)
        // The route root owns the native screen identity; its child wrapper does not add another AX element.
        let detail = detailScreen
        _ = try waitForSettledHistory(
            in: detailScreen, afterQueryGeneration: initialGeneration - 1, selecting: selectedID
        )
        XCTAssertTrue(detail.descendants(matching: .any)["ride-map.map"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.detail-initial-loading"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.detail-no-points"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["ride-map.detail-map-error"].exists)
        let detailShot = XCTAttachment(screenshot: app.screenshot())
        detailShot.name = "saved-history-selected-detail"
        detailShot.lifetime = .keepAlways
        add(detailShot)
        let header = detail.descendants(matching: .any)["ride-map.detail-header"]
        let back = header.buttons.firstMatch
        XCTAssertTrue(back.isHittable, app.debugDescription)
        assertMinimumControlDimension(back.frame.height)
        back.tap()
        XCTAssertTrue(viewport.waitForExistence(timeout: 5), app.debugDescription)
        let beforeReentry = try waitForSettledHistory(
            in: screen, afterQueryGeneration: initialGeneration - 1, selecting: selectedID
        )
        scrollElementFrameIntoViewport(selectedRow, in: scroll, maxScrolls: 60)
        XCTAssertEqual(selectedRow.value as? String, "Selected", app.debugDescription)

        app.tabBars.buttons["More"].tap()
        XCTAssertTrue(app.tabBars.buttons["More"].isSelected)
        app.tabBars.buttons["Map"].tap()
        XCTAssertTrue(viewport.waitForExistence(timeout: 5), app.debugDescription)
        _ = try waitForSettledHistory(in: screen, afterQueryGeneration: beforeReentry, selecting: selectedID)
        scrollElementFrameIntoViewport(selectedRow, in: scroll, maxScrolls: 60)
        XCTAssertEqual(selectedRow.value as? String, "Selected", app.debugDescription)
        let reenteredListedIDs = Set(
            Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["listed"]?
                .split(separator: ",").map(String.init) ?? []
        )
        XCTAssertTrue(reenteredListedIDs.contains(newestSeededID), "The override survives Map reentry")

        XCTAssertEqual(date.label, "All time", "Map reentry must preserve the selected history filter")
        XCTAssertTrue(date.isHittable, app.debugDescription)
        XCTAssertTrue(clear.isEnabled)
        XCTAssertTrue(clear.isHittable, app.debugDescription)
        date.tap()
        XCTAssertTrue(showShortRides.waitForExistence(timeout: 5), app.debugDescription)
        date.tap()
        assertHistoryFrame(filters.frame, equals: initialFilterFrame)
        assertHistoryFrame(viewport.frame, equals: initialViewportFrame)
        assertMinimumControlDimension(clear.frame.width)
        assertMinimumControlDimension(clear.frame.height)
        clear.tap()
        let filtersCleared = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", "Last 30 days"), object: date
        )
        XCTAssertEqual(XCTWaiter.wait(for: [filtersCleared], timeout: 5), .completed, app.debugDescription)
        XCTAssertFalse(clear.isEnabled)
        _ = try waitForSettledHistory(in: screen, afterQueryGeneration: beforeReentry)
        let listedAfterClear = Set(
            Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["listed"]?
                .split(separator: ",").map(String.init) ?? []
        )
        XCTAssertTrue(listedAfterClear.isDisjoint(with: seededIDs), "Clear restores the short-ride default")
        let historyEmpty = app.descendants(matching: .any)["ride-map.history-empty"]
        XCTAssertTrue(historyEmpty.waitForExistence(timeout: 5), app.debugDescription)
        let emptyStateCopy = historyEmpty.label
        XCTAssertTrue(emptyStateCopy.contains("No rides match these filters."), emptyStateCopy)
        XCTAssertTrue(
            emptyStateCopy.contains("To include shorter rides, turn on Show short rides in the date menu."),
            emptyStateCopy
        )
        XCTAssertFalse(emptyStateCopy.localizedCaseInsensitiveContains("unavailable"), emptyStateCopy)
        XCTAssertFalse(emptyStateCopy.localizedCaseInsensitiveContains("route map"), emptyStateCopy)
        assertHistoryFrame(filters.frame, equals: initialFilterFrame)
        assertHistoryFrame(viewport.frame, equals: initialViewportFrame)
        let clearedShot = XCTAttachment(screenshot: app.screenshot())
        clearedShot.name = "saved-history-filters-cleared-without-reflow"
        clearedShot.lifetime = .keepAlways
        add(clearedShot)

        app.terminate()
        app.launch()
        XCTAssertTrue(mapTab.waitForExistence(timeout: 10), app.debugDescription)
        mapTab.tap()
        XCTAssertTrue(screen.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertEqual(
            (screen.value as? String)?.components(separatedBy: ";").first, receipt,
            "Relaunch must reuse the same validated Rust fixture IDs"
        )
        let relaunchedPicker = app.descendants(matching: .any)["ride-map.mode-picker"].segmentedControls.firstMatch
        relaunchedPicker.buttons["History"].tap()
        let relaunchedGeneration = try waitForSettledHistory(in: screen, afterQueryGeneration: 0)
        let relaunchedDate = app.buttons["ride-map.history-date-filter"]
        relaunchedDate.tap()
        let relaunchedShortRideToggle = app.descendants(matching: .any)["ride-map.history-show-short-rides"]
        XCTAssertTrue(relaunchedShortRideToggle.waitForExistence(timeout: 5), app.debugDescription)
        relaunchedDate.tap()
        let relaunchedListedIDs = Set(
            Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))["listed"]?
                .split(separator: ",").map(String.init) ?? []
        )
        XCTAssertTrue(relaunchedListedIDs.isDisjoint(with: seededIDs))
        XCTAssertGreaterThan(relaunchedGeneration, 0)
    }

    private func waitForSettledHistory(
        in screen: XCUIElement, afterQueryGeneration generation: UInt64, selecting rideID: String? = nil
    ) throws -> UInt64 {
        let settled = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement, let value = element.value as? String else { return false }
                let fields = Self.historyFixtureFields(from: value)
                guard let current = fields["query"].flatMap(UInt64.init), current > generation,
                    fields["loading"] == "false"
                else { return false }
                guard let rideID else { return true }
                return fields["selected"] == rideID && fields["projection"] == rideID
            }, object: screen
        )
        XCTAssertEqual(XCTWaiter.wait(for: [settled], timeout: 10), .completed, app.debugDescription)
        let fields = Self.historyFixtureFields(from: try XCTUnwrap(screen.value as? String))
        return try XCTUnwrap(fields["query"].flatMap(UInt64.init))
    }

    nonisolated private static func historyFixtureFields(from value: String) -> [String: String] {
        Dictionary(
            uniqueKeysWithValues: value.split(separator: ";").compactMap { component in
                let pair = component.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                guard pair.count == 2 else { return nil }
                return (String(pair[0]), String(pair[1]))
            }
        )
    }

    private func assertHistoryFrame(
        _ actual: CGRect, equals expected: CGRect, file: StaticString = #filePath, line: UInt = #line
    ) {
        XCTAssertEqual(actual.minX, expected.minX, accuracy: 2, file: file, line: line)
        XCTAssertEqual(actual.minY, expected.minY, accuracy: 2, file: file, line: line)
        XCTAssertEqual(actual.width, expected.width, accuracy: 2, file: file, line: line)
        XCTAssertEqual(actual.height, expected.height, accuracy: 2, file: file, line: line)
    }

    func testCaptureAnnotationUsesOneStatefulAccessibleAction() {
        enterCapture()
        app.buttons["captures.labels"].tap()

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

    func testCaptureNavigationPreservesRecordingAndLabels() {
        enterCapture()
        app.buttons["captures.labels"].tap()
        app.buttons["capture.label.ride.action"].tap()
        let back = app.navigationBars["Recording"].buttons["BackButton"]
        XCTAssertTrue(back.isHittable)
        back.tap()
        XCTAssertTrue(app.descendants(matching: .any)["captures.home"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["captures.new"].isEnabled)
        app.buttons["captures.active"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["capture.screen"].waitForExistence(timeout: 5))
        app.buttons["captures.labels"].tap()
        XCTAssertEqual(app.buttons["capture.label.ride.action"].label, "Stop Ride")
        XCTAssertTrue(app.buttons["capture.stop"].isEnabled)
    }

    func testCaptureLabelCapacityPreservesActiveLabelsAndCanSave() {
        enterCapture()
        app.buttons["captures.labels"].tap()
        let screen = app.descendants(matching: .any)["capture.screen"]
        let ride = app.buttons["capture.label.ride.action"]
        let balance = app.buttons["capture.label.balancing.action"]
        let charging = app.buttons["capture.label.charging.action"]
        scrollElementFrameIntoViewport(ride, in: screen, maxScrolls: 8)
        ride.tap()
        scrollElementFrameIntoViewport(balance, in: screen, maxScrolls: 8)
        balance.tap()
        scrollElementFrameIntoViewport(charging, in: screen, maxScrolls: 8)
        charging.tap()
        let rejection = app.alerts["Label wasn't recorded"]
        XCTAssertTrue(rejection.waitForExistence(timeout: 5))
        rejection.buttons["OK"].tap()
        XCTAssertEqual(ride.label, "Stop Ride")
        XCTAssertEqual(balance.label, "Stop Balance")
        XCTAssertEqual(charging.label, "Start Charge")
        app.buttons["capture.stop"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["captures.detail"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.alerts["Label wasn't recorded"].exists)
    }

    func testCaptureLibraryDoneDismissesSetupWithoutEndingRecording() {
        enterCapture()
        let back = app.navigationBars["Recording"].buttons["BackButton"]
        XCTAssertTrue(back.isHittable)
        back.tap()
        let done = app.buttons["setup.done"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        XCTAssertTrue(done.isHittable)
        done.tap()
        XCTAssertTrue(app.buttons["device-picker.open-setup"].waitForExistence(timeout: 5))
        app.buttons["device-picker.open-setup"].tap()
        let captures = app.buttons["setup.captures"]
        scrollElementFrameIntoViewport(captures, in: app.descendants(matching: .any)["setup.screen"], maxScrolls: 8)
        captures.tap()
        XCTAssertTrue(app.buttons["captures.active"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["captures.new"].isEnabled)
    }

    func testCaptureExposesTypedWriterHealthDetails() {
        enterCapture()
        app.buttons["captures.technical"].tap()

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

    func testFinishCaptureOpensSavedArtifactAndShareSheet() throws {
        _ = try finishCaptureAndOpenArtifact()
        let share = app.descendants(matching: .any)["captures.share"]
        XCTAssertTrue(share.isHittable)
        share.tap()
        XCTAssertTrue(app.otherElements["ActivityListView"].waitForExistence(timeout: 5), app.debugDescription)
    }

    func testFinishCapturePrivateSendCanBeDeclinedAndCancelledWithoutLosingLocalShareAfterRelaunch() throws {
        #if !targetEnvironment(simulator)
            throw XCTSkip("Private capture Send regression is restricted to Simulator")
        #endif
        executionTimeAllowance = 180
        app.terminate()
        app.launchEnvironment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"] = UUID().uuidString
        app.launch()

        func waitForReady(_ element: XCUIElement) {
            let ready = XCTNSPredicateExpectation(
                predicate: NSPredicate { _, _ in element.exists && element.isEnabled && element.isHittable },
                object: element
            )
            XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 10), .completed, app.debugDescription)
        }

        // Database captures finish durably before an explicit file export.
        enterCapture()
        let finish = app.buttons["capture.stop"]
        waitForReady(finish)
        finish.tap()
        let detail = app.descendants(matching: .any)["captures.detail"]
        XCTAssertTrue(detail.waitForExistence(timeout: 10), app.debugDescription)
        let review = app.buttons["captures.send.review"]
        scrollElementFrameIntoViewport(review, in: detail, maxScrolls: 12)
        waitForReady(review)
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.receipt"].exists)
        review.tap()

        // Only review the locally prepared snapshot; this test never confirms Send.
        let confirmation = app.sheets["Send this capture privately?"]
        let confirm = confirmation.buttons.matching(identifier: "captures.send.confirm").firstMatch
        waitForReady(confirm)
        let disclosure = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS %@ AND label CONTAINS %@", "Music is excluded.", "CutOut maintainer")
        ).firstMatch
        XCTAssertTrue(disclosure.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(disclosure.label.contains("raw Bluetooth evidence"), disclosure.label)
        XCTAssertTrue(disclosure.label.contains("event order and timing"), disclosure.label)
        XCTAssertTrue(disclosure.label.contains("device and platform identifiers"), disclosure.label)
        XCTAssertTrue(disclosure.label.contains("Your original recording stays on this device."), disclosure.label)
        let decline = app.buttons["Not now"]
        if decline.waitForExistence(timeout: 2) {
            waitForReady(decline)
            decline.tap()
        } else {
            // Native popover presentation omits its cancel-role button.
            let dismiss = app.otherElements["PopoverDismissRegion"]
            guard dismiss.waitForExistence(timeout: 5), confirmation.exists else {
                XCTFail("Expected the native confirmation popover and its dismissal region")
                return
            }
            let outside = dismiss.coordinate(withNormalizedOffset: CGVector(dx: 0.05, dy: 0.5))
            guard dismiss.frame.contains(outside.screenPoint), !confirmation.frame.contains(outside.screenPoint) else {
                XCTFail("The dismissal coordinate must be outside the confirmation")
                return
            }
            outside.tap()
        }
        let dismissed = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in !confirmation.exists }, object: confirmation
        )
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 5), .completed, app.debugDescription)
        XCTAssertTrue(detail.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.receipt"].exists)

        let cancel = app.buttons["captures.send.cancel"]
        scrollElementFrameIntoViewport(cancel, in: detail, maxScrolls: 8)
        waitForReady(cancel)
        cancel.tap()
        let cancelled = app.staticTexts["Send cancelled"]
        XCTAssertTrue(cancelled.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.receipt"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.error"].exists)
        retainCaptureScreenshot("Private Send declined and cancelled; local capture retained")

        app.navigationBars.buttons.firstMatch.tap()
        let home = app.descendants(matching: .any)["captures.home"]
        XCTAssertTrue(home.waitForExistence(timeout: 5), app.debugDescription)
        let storedCapture = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "captures.database."))
            .firstMatch
        XCTAssertTrue(storedCapture.waitForExistence(timeout: 10), app.debugDescription)
        let storedCaptureID = storedCapture.identifier
        app.terminate()
        app.launch()

        let setupButton = app.buttons["device-picker.open-setup"]
        waitForReady(setupButton)
        setupButton.tap()
        let setup = app.descendants(matching: .any)["setup.screen"]
        XCTAssertTrue(setup.waitForExistence(timeout: 5), app.debugDescription)
        let captures = app.buttons["setup.captures"]
        scrollElementFrameIntoViewport(captures, in: setup, maxScrolls: 8)
        waitForReady(captures)
        captures.tap()
        XCTAssertTrue(home.waitForExistence(timeout: 5), app.debugDescription)
        let restoredCapture = app.buttons[storedCaptureID]
        waitForReady(restoredCapture)
        restoredCapture.tap()
        assertStoredCaptureDetail()
        let restoredDetail = app.collectionViews.firstMatch
        let restoredCancelled = app.staticTexts["Send cancelled"]
        scrollElementFrameIntoViewport(restoredCancelled, in: restoredDetail, maxScrolls: 12)
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.receipt"].exists)
        XCTAssertFalse(app.buttons["captures.send"].exists)
        XCTAssertFalse(app.buttons["captures.send.review"].exists)

        let export = app.buttons["Export recording"]
        scrollElementFrameIntoViewport(export, in: restoredDetail, maxScrolls: 12)
        waitForReady(export)
        export.tap()
        let restoredShare = app.buttons["Share recording"]
        waitForReady(restoredShare)
        restoredShare.tap()
        XCTAssertTrue(app.otherElements["ActivityListView"].waitForExistence(timeout: 5), app.debugDescription)
    }

    func testFinishCapturePrivateSendLiveUnsupportedFixturePersistsReceiptAndDeletesOnlySentCopy() throws {
        try assertLivePrivateCaptureSend(fixture: .unknownDevice, caseName: "unsupported")
    }

    func testFinishCapturePrivateSendLiveUnverifiedFixturePersistsReceiptAndDeletesOnlySentCopy() throws {
        try assertLivePrivateCaptureSend(fixture: .probeDevice, caseName: "unverified")
    }

    private func assertLivePrivateCaptureSend(fixture: Fixture, caseName: String) throws {
        #if !targetEnvironment(simulator)
            throw XCTSkip("Live private Send acceptance is restricted to Simulator")
        #endif
        guard ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_PRIVATE_SEND_LIVE"] == "1" else {
            throw XCTSkip("Set CUTOUT_UI_TEST_PRIVATE_SEND_LIVE=1 to send synthetic captures to the production service")
        }
        executionTimeAllowance = 240
        app.terminate()
        app.launchArguments =
            fixture.launchArguments + [
                "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryL",
                "-CUTOUT_UI_TEST_MUSIC", "silent", "-io.cutout.music.monitoring.enabled", "NO",
            ]
        app.launchEnvironment = fixture.launchEnvironment
        let databaseID = UUID().uuidString
        app.launchEnvironment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"] = databaseID
        app.launch()

        func waitForReady(_ element: XCUIElement) {
            let ready = XCTNSPredicateExpectation(
                predicate: NSPredicate { _, _ in element.exists && element.isEnabled && element.isHittable },
                object: element
            )
            XCTAssertEqual(XCTWaiter.wait(for: [ready], timeout: 10), .completed, app.debugDescription)
        }

        func receiptOperationID(in container: XCUIElement) throws -> String {
            let receipt = app.descendants(matching: .any)["captures.send.receipt"]
            scrollElementFrameIntoViewport(
                receipt, in: container, maxScrolls: 12
            )
            XCTAssertTrue(receipt.waitForExistence(timeout: 60), app.debugDescription)
            let values =
                [receipt.label, receipt.value as? String ?? ""]
                + receipt.staticTexts.allElementsBoundByIndex.map(\.label)
            let components = values.flatMap { $0.components(separatedBy: CharacterSet(charactersIn: " ,\n")) }
            return try XCTUnwrap(components.first(where: { UUID(uuidString: $0) != nil }), "Expected a receipt UUID")
        }

        // These existing scan fixtures exercise native entry and the real service,
        // not Bluetooth hardware or raw notification delivery.
        let captureSetup = openCaptureSetup()
        let description = app.textFields["captures.description"]
        waitForReady(description)
        description.tap()
        description.typeText("Synthetic \(caseName) fixture; no hardware measurement; acceptance \(databaseID).")
        let start = app.buttons["captures.start"]
        scrollElementFrameIntoViewport(start, in: captureSetup, maxScrolls: 8)
        waitForReady(start)
        start.tap()
        let recording = app.descendants(matching: .any)["capture.screen"]
        XCTAssertTrue(recording.waitForExistence(timeout: 10), app.debugDescription)
        app.buttons["captures.labels"].tap()
        let annotation = reachableCaptureAnnotation("ride", in: recording)
        waitForReady(annotation)
        annotation.tap()
        let finish = app.buttons["capture.stop"]
        scrollElementFrameIntoViewport(finish, in: recording, maxScrolls: 8)
        waitForReady(finish)
        finish.tap()
        let detail = app.descendants(matching: .any)["captures.detail"]
        XCTAssertTrue(detail.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.receipt"].exists)

        let review = app.buttons["captures.send.review"]
        scrollElementFrameIntoViewport(review, in: detail, maxScrolls: 12)
        waitForReady(review)
        review.tap()
        let confirmation = app.sheets["Send this capture privately?"]
        let confirm = confirmation.buttons.matching(identifier: "captures.send.confirm").firstMatch
        waitForReady(confirm)
        let disclosure = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS %@ AND label CONTAINS %@", "Music is excluded.", "CutOut maintainer")
        ).firstMatch
        XCTAssertTrue(disclosure.waitForExistence(timeout: 5), app.debugDescription)
        let identity = XCTAttachment(string: "Synthetic \(caseName) fixture; isolated database \(databaseID)")
        identity.name = "Live private Send cleanup identity"
        identity.lifetime = .keepAlways
        add(identity)
        var operationIDForCleanup: String?
        livePrivateSendCleanup = { [weak self] in
            self?.bestEffortDeleteLivePrivateCapture(
                databaseID: databaseID, caseName: caseName, operationID: operationIDForCleanup
            )
        }
        confirm.tap()
        let operationID = try receiptOperationID(in: detail)
        operationIDForCleanup = operationID
        let receipt = XCTAttachment(string: "Synthetic \(caseName) fixture; production operation \(operationID)")
        receipt.name = "Live private Send receipt"
        receipt.lifetime = .keepAlways
        add(receipt)
        XCTAssertTrue(app.staticTexts["Received privately"].exists, app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.error"].exists)

        app.navigationBars.buttons.firstMatch.tap()
        let home = app.descendants(matching: .any)["captures.home"]
        XCTAssertTrue(home.waitForExistence(timeout: 5), app.debugDescription)
        let storedCapture = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "captures.database."))
            .firstMatch
        XCTAssertTrue(storedCapture.waitForExistence(timeout: 10), app.debugDescription)
        let storedCaptureID = storedCapture.identifier
        app.terminate()
        app.launch()
        let setupButton = app.buttons["device-picker.open-setup"]
        waitForReady(setupButton)
        setupButton.tap()
        let setup = app.descendants(matching: .any)["setup.screen"]
        XCTAssertTrue(setup.waitForExistence(timeout: 5), app.debugDescription)
        let captures = app.buttons["setup.captures"]
        scrollElementFrameIntoViewport(captures, in: setup, maxScrolls: 8)
        waitForReady(captures)
        captures.tap()
        XCTAssertTrue(home.waitForExistence(timeout: 5), app.debugDescription)
        let restoredCapture = app.buttons[storedCaptureID]
        waitForReady(restoredCapture)
        restoredCapture.tap()
        assertStoredCaptureDetail()
        let restoredDetail = app.collectionViews.firstMatch
        let delete = app.buttons["captures.send.delete"]
        scrollElementFrameIntoViewport(delete, in: restoredDetail, maxScrolls: 12)
        XCTAssertEqual(try receiptOperationID(in: restoredDetail), operationID)
        waitForReady(delete)
        delete.tap()
        let deleteConfirmation = app.sheets["Delete the sent copy?"]
        let confirmDeletion = deleteConfirmation.buttons.matching(identifier: "Delete sent copy").firstMatch
        waitForReady(confirmDeletion)
        confirmDeletion.tap()
        XCTAssertTrue(app.staticTexts["Sent copy deleted"].waitForExistence(timeout: 60), app.debugDescription)
        livePrivateSendCleanup = nil
        XCTAssertFalse(app.descendants(matching: .any)["captures.send.error"].exists)

        let export = app.buttons["Export recording"]
        scrollElementFrameIntoViewport(export, in: restoredDetail, maxScrolls: 12)
        waitForReady(export)
        export.tap()
        let share = app.buttons["Share recording"]
        waitForReady(share)
        share.tap()
        XCTAssertTrue(app.otherElements["ActivityListView"].waitForExistence(timeout: 5), app.debugDescription)
        retainCaptureScreenshot("Synthetic \(caseName) Send deleted; original recording remains shareable")
    }

    /// Failure cleanup never invokes assertion helpers: the original test failure
    /// must not prevent an attempt to remove its synthetic production submission.
    private func bestEffortDeleteLivePrivateCapture(databaseID: String, caseName: String, operationID: String?) {
        let deadline = Date().addingTimeInterval(60)
        var recoveredOperationID = operationID
        var removed = false
        defer {
            let evidence = XCTAttachment(
                string:
                    "Synthetic \(caseName); database \(databaseID); operation \(recoveredOperationID ?? "not recovered"); "
                    + (removed ? "cleanup confirmed" : "cleanup unconfirmed; retain this database for operator cleanup")
            )
            evidence.name = "Live private Send failure cleanup"
            evidence.lifetime = .keepAlways
            add(evidence)
        }

        func tapIfReachable(_ element: XCUIElement, in container: XCUIElement, maxScrolls: Int = 8) -> Bool {
            for attempt in 0...maxScrolls {
                guard Date() < deadline else { return false }
                if element.exists, element.isEnabled, element.isHittable {
                    element.tap()
                    return true
                }
                guard attempt < maxScrolls, container.exists else { return false }
                if element.exists, element.frame.maxY < container.frame.minY {
                    container.swipeDown()
                } else {
                    container.swipeUp()
                }
            }
            return false
        }

        app.terminate()
        app.launch()
        let setupButton = app.buttons["device-picker.open-setup"]
        guard setupButton.waitForExistence(timeout: 5), tapIfReachable(setupButton, in: app) else { return }
        guard tapIfReachable(app.buttons["setup.captures"], in: app.collectionViews.firstMatch) else { return }
        let stored = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "captures.database."))
            .firstMatch
        guard stored.waitForExistence(timeout: 5), tapIfReachable(stored, in: app.collectionViews.firstMatch) else {
            return
        }
        let receipt = app.descendants(matching: .any)["captures.send.receipt"]
        if receipt.waitForExistence(timeout: 5) {
            let values =
                [receipt.label, receipt.value as? String ?? ""]
                + receipt.staticTexts.allElementsBoundByIndex.map(\.label)
            recoveredOperationID = values.first(where: { UUID(uuidString: $0) != nil }) ?? recoveredOperationID
        }
        let detail = app.collectionViews.firstMatch
        if tapIfReachable(app.buttons["captures.send.delete"], in: detail) {
            let confirmation = app.sheets["Delete the sent copy?"]
            let confirm = confirmation.buttons.matching(identifier: "Delete sent copy").firstMatch
            guard confirm.waitForExistence(timeout: 5), tapIfReachable(confirm, in: confirmation, maxScrolls: 0) else {
                return
            }
            removed = app.staticTexts["Sent copy deleted"].waitForExistence(
                timeout: max(0, deadline.timeIntervalSinceNow)
            )
        } else if tapIfReachable(app.buttons["captures.send.cancel"], in: detail) {
            removed = app.staticTexts["Send cancelled"].waitForExistence(timeout: max(0, deadline.timeIntervalSinceNow))
        }
    }

    func testFinishCaptureOpensAccessibleSavedArtifactInLightAppearanceAtAccessibilityDynamicType() throws {
        _ = try finishCaptureAndOpenArtifact()
        try performVisibleLayoutAccessibilityAudit()
    }

    func testFinishCaptureOpensAccessibleSavedArtifactInDarkAppearanceAtAccessibilityDynamicType() throws {
        _ = try finishCaptureAndOpenArtifact()
        try performVisibleLayoutAccessibilityAudit()
    }

    func
        testFinishCaptureOpensAccessibleSavedArtifactWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        _ = try finishCaptureAndOpenArtifact(usesLocalizedText: true)
        try performVisibleLayoutAccessibilityAudit()
    }

    private func finishCaptureAndOpenArtifact(usesLocalizedText: Bool = false) throws -> XCUIElement {
        enterCapture()
        let finish = app.buttons["capture.stop"]
        XCTAssertTrue(finish.waitForExistence(timeout: 5))
        assertMinimumControlDimension(finish.frame.height)
        finish.tap()
        let detail = app.descendants(matching: .any)["captures.detail"]
        XCTAssertTrue(detail.waitForExistence(timeout: 10), app.debugDescription)
        let share = app.descendants(matching: .any)["captures.share"]
        scrollElementFrameIntoViewport(share, in: detail, maxScrolls: 8)
        XCTAssertFalse(app.descendants(matching: .any)["capture.screen"].exists)
        retainCaptureScreenshot("Saved capture")
        XCTAssertFalse(app.buttons["device-picker.capture-status"].exists)
        if usesLocalizedText { XCTAssertFalse(share.label.isEmpty) }
        return detail
    }

    func testFinishCaptureFailureKeepsCaptureScreenVisible() throws {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible(
            auditExclusions: .dynamicType
        )
    }

    func testBackgroundFlushFailureRemainsVisibleAfterReactivatingCaptureAtAccessibilityDynamicType() throws {
        enterCapture()
        XCUIDevice.shared.press(.home)
        app.activate()
        let status = app.descendants(matching: .any)["capture.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        let failure = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "Couldn’t save"), object: status
        )
        XCTAssertEqual(XCTWaiter.wait(for: [failure], timeout: 5), .completed)
        XCTAssertTrue(app.buttons["capture.stop"].isEnabled)
        XCTAssertFalse(app.buttons["captures.share"].exists)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testBackgroundFlushRealWriterRemainsUsableAfterReactivatingCaptureAtAccessibilityDynamicType() throws {
        enterCapture()
        XCUIDevice.shared.press(.home)
        app.activate()
        let capture = app.descendants(matching: .any)["capture.screen"]
        XCTAssertTrue(capture.waitForExistence(timeout: 5))
        let finish = app.buttons["capture.stop"]
        XCTAssertTrue(finish.isHittable)
        finish.tap()
        XCTAssertTrue(app.descendants(matching: .any)["captures.detail"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["captures.share"].exists)
        try performVisibleLayoutAccessibilityAudit()
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

    func testFinishCaptureFailureKeepsCaptureScreenAccessibleWithPseudolocalizedTextAtAccessibilityDynamicType() throws
    {
        try assertFinishCaptureFailureKeepsCaptureScreenAccessible(usesLocalizedText: true)
    }

    func
        testFinishCaptureFailureKeepsCaptureScreenAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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
        let status = app.descendants(matching: .any)["capture.status"]
        let previous = status.label
        XCTAssertTrue(finish.isHittable)
        finish.tap()
        let changed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label != %@ AND label != %@", previous, ""), object: status
        )
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 5), .completed)
        XCTAssertTrue(app.descendants(matching: .any)["capture.screen"].exists)
        XCTAssertFalse(app.buttons["captures.share"].exists)
        XCTAssertTrue(finish.isEnabled, "A failed flush must permit retry")
        XCTAssertTrue(finish.isHittable)
        if !usesLocalizedText { XCTAssertTrue(status.label.contains("Couldn’t save")) }
        try performVisibleLayoutAccessibilityAudit(excluding: auditExclusions)
    }

    func testCaptureExclusivePedalModeLeavesOneActiveAccessibleAction() {
        enterCapture()
        app.buttons["captures.labels"].tap()

        let screen = app.descendants(matching: .any)["capture.screen"]
        let hardPedals = app.buttons["capture.label.pedals_hard.action"]
        let softPedals = app.buttons["capture.label.pedals_soft.action"]
        let finish = app.buttons["capture.stop"]

        scrollElementFrameIntoViewport(hardPedals, in: screen, maxScrolls: 8, occludedBy: finish)
        hardPedals.tap()
        XCTAssertEqual(hardPedals.label, "Stop Pedals hard")

        scrollElementFrameIntoViewport(softPedals, in: screen, maxScrolls: 8, occludedBy: finish)
        softPedals.tap()
        XCTAssertEqual(hardPedals.label, "Start Pedals hard")
        XCTAssertEqual(softPedals.label, "Stop Pedals soft")
    }

    func testDisconnectKeepsSavedDeviceUntilExplicitForget() throws {
        let forget = try disconnectAndRequireSavedDevice()
        forget.tap()
        XCTAssertFalse(forget.isEnabled)
    }

    func
        testDisconnectKeepsSavedDeviceAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        let forget = try disconnectAndRequireSavedDevice()
        try performVisibleLayoutAccessibilityAudit()
        forget.tap()
        XCTAssertFalse(forget.isEnabled)
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
        XCTAssertFalse(app.buttons["device-picker.forget-saved-device"].exists)
        app.buttons["device-picker.open-setup"].tap()
        let forget = app.buttons["setup.forget-saved-device"]
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
            XCTAssertTrue(
                pairAvailableDevice(family), "Cycle \(cycle) did not start from the native \(family.name) Use button")
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

    func
        testCapturePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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

    func
        testProductionPickerPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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
            excluding: .dynamicType,
            ignoringUnavailableMetricPlaceholderContrastWarning: true
        )
    }

    func testVescRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType() throws {
        try assertRidePublishesDynamicTelemetryAfterRouteMounts(.vesc)
    }

    func testEucRidePublishesDynamicTelemetryAfterRouteMountsAtAccessibilityDynamicType() throws {
        try assertRidePublishesDynamicTelemetryAfterRouteMounts(.euc)
    }

    func testEucTuneAlarmSettingsHasAReachableBackAction() throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        XCTAssertNotNil(connectedScreen(timeout: 20))
        defer { disconnectIfConnected() }
        tapNavigationTab("tune", title: "Tune")
        let tune = app.descendants(matching: .any)["settings.screen.tune"]
        XCTAssertTrue(tune.waitForExistence(timeout: 5))
        let alarms = app.buttons["settings.open-alarms"]
        for _ in 0..<8 where !alarms.isHittable { tune.swipeUp() }
        XCTAssertTrue(alarms.isHittable)
        alarms.tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.screen.alarms"].waitForExistence(timeout: 5))
        let back = app.navigationBars.buttons.firstMatch
        XCTAssertTrue(back.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(back.isHittable)
        assertMinimumControlDimension(back.frame.height)
        back.tap()
        XCTAssertTrue(tune.waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["settings.screen.alarms"].exists)
        XCTAssertTrue(navigationTab("ride", title: "Ride").isHittable)
    }

    func testEucLightingPresetNameKeepsKeyboardFocusUntilSubmission() throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        XCTAssertNotNil(connectedScreen(timeout: 20))
        defer { disconnectIfConnected() }
        tapNavigationTab("lighting", title: "Lighting")
        let lighting = app.descendants(matching: .any)["dashboard.screen.lighting"]
        XCTAssertTrue(lighting.waitForExistence(timeout: 5))
        let field = lighting.textFields.firstMatch
        for _ in 0..<8 where !field.isHittable { lighting.swipeUp() }
        XCTAssertTrue(field.isHittable)
        field.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        field.typeText("Evening ride")
        XCTAssertEqual(field.value as? String, "Evening ride")
        XCTAssertTrue(app.keyboards.firstMatch.exists)
        field.typeText("\n")
        let dismissed = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in !self.app.keyboards.firstMatch.exists }, object: nil
        )
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 5), .completed)
        XCTAssertEqual(field.value as? String, "Evening ride")
    }

    func testEucTuneUsesOrdinaryGenericControlsWithoutInventedValues() throws {
        try assertEucTuneSettings()
    }

    func testEucTuneUsesOrdinaryGenericControlsAtAccessibilityDynamicType() throws {
        try assertEucTuneSettings()
    }

    private func assertEucTuneSettings() throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic EUC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }
        tapNavigationTab("tune", title: "Tune")
        let screen = app.descendants(matching: .any)["settings.screen.tune"]
        XCTAssertTrue(screen.waitForExistence(timeout: 5))

        let turnOn = app.buttons["settings.on.highBeam"]
        let turnOff = app.buttons["settings.off.highBeam"]
        XCTAssertTrue(turnOn.waitForExistence(timeout: 5))
        XCTAssertTrue(turnOff.exists)
        XCTAssertTrue(app.staticTexts["Headlight"].exists)
        assertMinimumControlDimension(turnOn.frame.height)
        assertMinimumControlDimension(turnOff.frame.height)
        XCTAssertFalse(app.buttons["settings.apply.highBeam"].exists)
        let currentHighBeam = app.descendants(matching: .any)["settings.current.highBeam"]
        XCTAssertFalse(currentHighBeam.exists)
        for button in [turnOn, turnOff] {
            button.tap()
            let selected = XCTNSPredicateExpectation(predicate: NSPredicate(format: "selected == true"), object: button)
            XCTAssertEqual(XCTWaiter.wait(for: [selected], timeout: 5), .completed)
            XCTAssertEqual(button.value as? String, "Requested")
            XCTAssertFalse(app.staticTexts["settings.requested.highBeam"].exists)
            XCTAssertFalse(app.staticTexts["settings.status.highBeam"].exists)
            XCTAssertFalse(currentHighBeam.exists)
            XCTAssertFalse(app.staticTexts["settings.error.highBeam"].exists)
        }

        let brightness = app.steppers["settings.stepper.displayBrightness"]
        XCTAssertTrue(brightness.waitForExistence(timeout: 5))
        if brightness.isEnabled {
            for _ in 0..<8 where !brightness.isHittable { screen.swipeUp() }
            XCTAssertTrue(brightness.isHittable)
            XCTAssertFalse(app.buttons["settings.apply.displayBrightness"].exists)
            brightness.buttons.element(boundBy: 1).tap()
            XCTAssertTrue(app.staticTexts["settings.draft.displayBrightness"].exists)
            let draftValue = app.staticTexts["settings.draft.displayBrightness"].label
            let apply = app.buttons["settings.apply.displayBrightness"]
            XCTAssertTrue(apply.exists)
            for _ in 0..<4 where !apply.isHittable { screen.swipeUp() }
            apply.tap()
            XCTAssertFalse(apply.exists, "Apply should disappear after submitting the local draft")
            XCTAssertEqual(
                app.staticTexts["settings.draft.displayBrightness"].label, draftValue,
                "A pending value must not snap back to old readback")
        } else {
            XCTAssertFalse(app.buttons["settings.apply.displayBrightness"].exists)
            XCTAssertFalse(app.staticTexts["settings.draft.displayBrightness"].exists)
        }

        let controls = [
            "highBeam", "displayBrightness", "displayUnits", "beeperVolumePercent",
            "tiltbackSpeed", "pwmTiltback", "lateralTiltLimit", "speedAlarmThreshold", "brakeOverpressureAlarm",
            "pedalHardness", "dynamicAssist", "pedalDipCompensation", "pedalAngle", "ridingPreset",
            "voltageCorrection", "highSpeedMode", "lowBatteryMode", "transportMode",
        ]
        let booleans: Set<String> = ["highBeam", "highSpeedMode", "lowBatteryMode", "transportMode"]
        let choices: Set<String> = ["displayUnits", "ridingPreset"]
        for _ in 0..<8 where !turnOn.isHittable { screen.swipeDown() }
        for id in controls {
            let label = app.staticTexts["settings.control.\(id)"]
            for _ in 0..<12 where !label.isHittable { screen.swipeUp() }
            XCTAssertTrue(label.isHittable, "Missing ordinary setting \(id)")
            XCTAssertEqual(app.staticTexts.matching(identifier: "settings.control.\(id)").count, 1)
            if booleans.contains(id) {
                XCTAssertTrue(app.buttons["settings.on.\(id)"].exists)
                XCTAssertTrue(app.buttons["settings.off.\(id)"].exists)
            } else if choices.contains(id) {
                XCTAssertTrue(app.buttons["settings.picker.\(id)"].exists)
            } else {
                XCTAssertTrue(app.sliders["settings.slider.\(id)"].exists)
                XCTAssertTrue(app.steppers["settings.stepper.\(id)"].exists)
            }
            if id != "displayBrightness" {
                XCTAssertFalse(app.buttons["settings.apply.\(id)"].exists)
            }
            XCTAssertFalse(app.staticTexts["settings.requested.\(id)"].exists)
        }
        XCTAssertFalse(app.staticTexts["settings.control.chargeLimitDiagnostic"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["settings.validationDisclosure"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["settings.validationAuthorization"].exists)
        for text in [
            "Details", "New value", "Choose a value", "Reported diagnostics",
            "Advanced · needs validation", "Advanced · validation enabled",
            "Writing this setting has not been verified.", "Wheel confirmation unavailable.",
            "Requested · no confirmation available", "Adjust while parked. Check changes on the wheel.",
        ] {
            XCTAssertFalse(app.staticTexts[text].exists)
        }
        attachScreenshot(of: app, named: "Tune — all 18 ordinary settings")
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
        let screen = app.descendants(matching: .any)["device-picker.screen"]
        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["device-picker.open-setup"].isHittable)
        XCTAssertFalse(app.buttons["device-picker.capture-status"].exists)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testCaptureSetupControlsRemainReachableAtAccessibilityDynamicType() throws {
        try assertCaptureSetupAccessible()
    }

    func testCaptureSetupControlsRemainReachableInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertCaptureSetupAccessible()
    }

    func testCaptureSetupControlsRemainReachableInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertCaptureSetupAccessible()
    }

    func testCaptureSetupKeyboardWorkflowRemainsReachableAtAccessibilityDynamicType() throws {
        let setup = openCaptureSetup()
        let description = app.textFields["captures.description"]
        XCTAssertTrue(description.waitForExistence(timeout: 5))
        description.tap()
        description.typeText("Testing Bluetooth reception")
        XCTAssertTrue(app.buttons["captures.start"].isEnabled)
        setup.swipeUp()
        XCTAssertTrue(app.buttons["captures.start"].isHittable)
    }

    func testCaptureSetupCancelReturnsToLibraryAtAccessibilityDynamicType() {
        _ = openCaptureSetup()
        // Dismiss the new-capture sheet without touching the writer or transport.
        let cancel = app.buttons["captures.cancel"]
        XCTAssertTrue(cancel.isHittable)
        cancel.tap()
        XCTAssertTrue(app.descendants(matching: .any)["captures.home"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["captures.new"].isEnabled)
    }

    func testCaptureSetupControlsRemainReachableInRightToLeftLayout() throws {
        try assertCaptureSetupAccessible(exercisesKeyboard: true)
    }

    func testCaptureSetupControlsRemainReachableInLandscapeAtAccessibilityDynamicType() throws {
        try assertCaptureSetupAccessible(exercisesKeyboard: true)
    }

    private func assertCaptureSetupAccessible(
        exercisesKeyboard: Bool = false
    ) throws {
        let setup = openCaptureSetup()
        let description = app.textFields["captures.description"]
        XCTAssertTrue(description.waitForExistence(timeout: 5))
        if exercisesKeyboard {
            description.tap()
            description.typeText("Capture note")
            setup.swipeUp()
        }
        let start = app.buttons["captures.start"]
        for _ in 0..<6 where !start.isHittable { setup.swipeUp() }
        XCTAssertTrue(start.isHittable)
        XCTAssertTrue(start.isEnabled)
        try performVisibleLayoutAccessibilityAudit()
    }

    func
        testCaptureSetupPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertCaptureSetupAccessible()
    }

    func testVescUseOpensAnAccessibleLiveRide() throws {
        try assertConnectedSurface(
            for: .vesc,
            auditExclusions: .dynamicType,
            ignoringUnavailableMetricPlaceholderContrastWarning: true
        )
    }

    func testVescEssentialRideControlsRemainVisibleWithoutScrolling() throws {
        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .vesc)
    }

    func testEucPrimaryRoutesRemainUsableWithReduceMotionAndIncreasedContrastAtAccessibilityDynamicType() throws {
        // Include native Settings changes, route checks, Readings, and restoration in this bounded scenario.
        executionTimeAllowance = 360
        let settings = XCUIApplication(bundleIdentifier: "com.apple.Preferences")
        // Each preference audit starts a fresh Settings process. Failed setup
        // must not leave the next audit reusing a retained native navigation tree.
        settings.terminate()
        // Operate the real OS preference in Settings' process-local default text
        // size. CutOut must still prove its actual AX5 category and OS readbacks.
        settings.launchArguments = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryL"]
        settings.launch()
        let reduceMotion = try openReduceMotionSetting(in: settings)
        let original = try XCTUnwrap(reduceMotion.value as? String)
        XCTAssertTrue(["0", "1"].contains(original), settings.debugDescription)
        defer {
            do {
                let restored = try openReduceMotionSetting(in: settings)
                if restored.value as? String != original { tapReduceMotionToggle(in: restored, settings: settings) }
                let restoration = XCTNSPredicateExpectation(
                    predicate: NSPredicate(format: "value == %@", original), object: restored
                )
                XCTAssertEqual(XCTWaiter.wait(for: [restoration], timeout: 5), .completed, settings.debugDescription)
            } catch {
                XCTFail("Could not restore the original Simulator Reduce Motion setting: \(error)")
            }
            settings.terminate()
            app.activate()
        }
        if original == "0" { tapReduceMotionToggle(in: reduceMotion, settings: settings) }
        let enabled = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == %@", "1"), object: reduceMotion
        )
        XCTAssertEqual(XCTWaiter.wait(for: [enabled], timeout: 5), .completed, settings.debugDescription)
        app.activate()

        try assertEssentialRideControlsRemainVisibleWithoutScrolling(for: .euc) { screen in
            let preferences = XCTNSPredicateExpectation(
                predicate: NSPredicate(
                    format: "value CONTAINS %@ AND value CONTAINS %@ AND value CONTAINS %@ AND value CONTAINS %@",
                    "reduceMotion=true", "systemReduceMotion=true", "contrast=increased", "systemContrast=true"
                ), object: screen
            )
            XCTAssertEqual(XCTWaiter.wait(for: [preferences], timeout: 5), .completed, self.app.debugDescription)
            self.tapNavigationTab("map", title: "Map")
            XCTAssertTrue(self.app.descendants(matching: .any)["ride-map.screen"].waitForExistence(timeout: 5))
            self.tapNavigationTab("tune", title: "Tune")
            XCTAssertTrue(self.app.descendants(matching: .any)["settings.screen.tune"].waitForExistence(timeout: 5))
            self.tapNavigationTab("more", title: "More")
            XCTAssertTrue(self.app.descendants(matching: .any)["more.screen"].waitForExistence(timeout: 5))
            self.tapNavigationTab("camera", title: "Camera")
            XCTAssertTrue(self.app.descendants(matching: .any)["camera.screen"].waitForExistence(timeout: 5))
            let back = self.app.buttons["dashboard.back"]
            XCTAssertTrue(back.waitForExistence(timeout: 5), self.app.debugDescription)
            XCTAssertTrue(back.isHittable, self.app.debugDescription)
            self.assertMinimumControlDimension(back.frame.width)
            self.assertMinimumControlDimension(back.frame.height)
            back.tap()
            XCTAssertTrue(self.app.descendants(matching: .any)["more.screen"].waitForExistence(timeout: 5))
            self.tapNavigationTab("ride", title: "Ride")
            XCTAssertTrue(screen.waitForExistence(timeout: 5), self.app.debugDescription)
            XCTAssertEqual(screen.scrollViews.count, 0)
            self.attachScreenshot(of: self.app, named: "Ride after reduced-motion primary navigation")
        }
    }

    private func tapReduceMotionToggle(in row: XCUIElement, settings: XCUIApplication) {
        // The named Switch owns the whole Settings row. Its descendant Switch
        // is the rendered trailing toggle; tapping the row center does not change it.
        let controls = row.descendants(matching: .switch)
        XCTAssertEqual(controls.count, 1, settings.debugDescription)
        let control = controls.firstMatch
        XCTAssertEqual(control.elementType, .switch)
        XCTAssertTrue(control.isEnabled, settings.debugDescription)
        XCTAssertTrue(settings.windows.firstMatch.frame.contains(control.frame), settings.debugDescription)
        XCTAssertTrue(control.isHittable, settings.debugDescription)
        control.tap()
    }

    private func openReduceMotionSetting(in settings: XCUIApplication) throws -> XCUIElement {
        settings.activate()
        let reduceMotion = settings.switches["Reduce Motion"]
        if reduceMotion.waitForExistence(timeout: 2) { return reduceMotion }
        let accessibilityPage = settings.navigationBars["Accessibility"]
        if !accessibilityPage.exists {
            let accessibility = settings.buttons["Accessibility"]
            let list = settings.collectionViews.firstMatch
            XCTAssertTrue(list.waitForExistence(timeout: 5), settings.debugDescription)
            // Settings retains its scroll position. A prior bottom-of-list launch
            // must return toward the top before locating this lazily created button.
            for _ in 0..<8 where !accessibility.exists { list.swipeDown() }
            scrollElementFrameIntoViewport(accessibility, in: list, maxScrolls: 8)
            XCTAssertEqual(accessibility.elementType, .button)
            XCTAssertTrue(accessibility.isHittable, settings.debugDescription)
            accessibility.tap()
        }
        XCTAssertTrue(accessibilityPage.waitForExistence(timeout: 5), settings.debugDescription)
        // The table cell retains its offscreen geometry. Its native action
        // button acquires real bounds once the row scrolls into the viewport.
        let table = settings.tables.firstMatch
        XCTAssertTrue(table.waitForExistence(timeout: 5), settings.debugDescription)
        let motion = settings.cells["MOTION_TITLE"]
        scrollElementFrameIntoViewport(motion, in: table, maxScrolls: 8)
        XCTAssertEqual(motion.elementType, .cell)
        XCTAssertEqual(motion.label, "Motion")
        XCTAssertTrue(motion.isHittable, settings.debugDescription)
        let motionButton = settings.buttons["MOTION_TITLE"]
        scrollElementFrameIntoViewport(motionButton, in: table, maxScrolls: 8)
        XCTAssertEqual(motionButton.elementType, .button)
        XCTAssertEqual(motionButton.label, "Motion")
        XCTAssertTrue(motionButton.isHittable, settings.debugDescription)
        motionButton.tap()
        XCTAssertTrue(reduceMotion.waitForExistence(timeout: 5), settings.debugDescription)
        XCTAssertEqual(reduceMotion.elementType, .switch)
        XCTAssertTrue(reduceMotion.isHittable, settings.debugDescription)
        return reduceMotion
    }

    func testEucPwmReadoutHonorsRequestedAccessibilityTextSize() throws {
        try assertPwmReadoutGrowsWithAccessibilityText(for: .euc)
    }

    func testVescPwmReadoutHonorsRequestedAccessibilityTextSize() throws {
        try assertPwmReadoutGrowsWithAccessibilityText(for: .vesc)
    }

    private func assertPwmReadoutGrowsWithAccessibilityText(for family: ConnectedDeviceFamily) throws {
        let label = family == .euc ? "PWM headroom" : "Duty headroom"
        func readoutHeight(category: String) throws -> CGFloat {
            app.terminate()
            app.launchArguments =
                fixture.launchArguments + [
                    "-UIPreferredContentSizeCategoryName", category,
                    "-CUTOUT_UI_TEST_ENVIRONMENT_READBACK", "YES",
                ]
            app.launch()
            XCTAssertTrue(pairAvailableDevice(family))
            let screen = try XCTUnwrap(connectedScreen(timeout: 20))
            assertRideWindowHasSettled(screen, landscape: false)
            assertRideTextSizeReadback(screen, systemCategory: category)
            let pwm = screen.descendants(matching: .any).matching(NSPredicate(format: "label == %@", label)).firstMatch
            XCTAssertTrue(pwm.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertTrue(pwm.isHittable)
            XCTAssertEqual(screen.scrollViews.count, 0)
            let height = pwm.frame.height
            let attachment = XCTAttachment(screenshot: app.screenshot())
            attachment.name = "\(family.name)-pwm-\(category)"
            attachment.lifetime = .keepAlways
            add(attachment)
            disconnectIfConnected()
            return height
        }
        let normal = try readoutHeight(category: "UICTContentSizeCategoryL")
        let accessible = try readoutHeight(category: "UICTContentSizeCategoryAccessibilityXXXL")
        XCTAssertGreaterThan(accessible, normal, "PWM must grow instead of clamping the requested text size")
    }

    private func assertRideTextSizeReadback(_ screen: XCUIElement, systemCategory: String) {
        let expectedSize: String
        switch systemCategory {
        case "UICTContentSizeCategoryL": expectedSize = "large"
        case "UICTContentSizeCategoryXXXL": expectedSize = "xxxLarge"
        case "UICTContentSizeCategoryAccessibilityM": expectedSize = "accessibility1"
        case "UICTContentSizeCategoryAccessibilityL": expectedSize = "accessibility2"
        case "UICTContentSizeCategoryAccessibilityXL": expectedSize = "accessibility3"
        case "UICTContentSizeCategoryAccessibilityXXL": expectedSize = "accessibility4"
        case "UICTContentSizeCategoryAccessibilityXXXL": expectedSize = "accessibility5"
        default:
            XCTFail("No explicit expected SwiftUI category for \(systemCategory)")
            return
        }
        let readback = screen.value as? String ?? ""
        XCTAssertTrue(readback.contains("swiftui=\(expectedSize)"), readback)
        XCTAssertTrue(readback.contains("system=\(systemCategory)"), readback)
    }

    private func assertRideWindowHasSettled(_ screen: XCUIElement, landscape: Bool) {
        let window = app.windows.firstMatch
        let settled = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                let frame = window.frame
                let imageSize = Self.displayedScreenshotSize(self.app.screenshot().image)
                return frame.width > 0 && frame.height > 0
                    && (frame.width > frame.height) == landscape
                    && XCUIDevice.shared.orientation == (landscape ? .landscapeLeft : .portrait)
                    && screen.exists && frame.insetBy(dx: -2, dy: -2).contains(screen.frame)
                    && (imageSize.width > imageSize.height) == landscape
            }, object: app
        )
        XCTAssertEqual(XCTWaiter.wait(for: [settled], timeout: 5), .completed, app.debugDescription)
        let screenshot = app.screenshot().image
        let displayedSize = Self.displayedScreenshotSize(screenshot)
        let orientationProof = XCTAttachment(
            string: "device=\(XCUIDevice.shared.orientation.rawValue); window=\(window.frame); "
                + "imageOrientation=\(screenshot.imageOrientation.rawValue); imageSize=\(screenshot.size); "
                + "displayedSize=\(displayedSize)"
        )
        orientationProof.name = "settled Ride orientation metadata"
        orientationProof.lifetime = .keepAlways
        add(orientationProof)
        let screenshotProof = XCTAttachment(screenshot: app.screenshot())
        screenshotProof.name = "settled Ride native screenshot"
        screenshotProof.lifetime = .keepAlways
        add(screenshotProof)
        XCTAssertEqual(displayedSize.width > displayedSize.height, landscape)
    }

    private static func displayedScreenshotSize(_ image: UIImage) -> CGSize {
        switch image.imageOrientation {
        case .left, .right, .leftMirrored, .rightMirrored:
            CGSize(width: image.size.height, height: image.size.width)
        case .up, .down, .upMirrored, .downMirrored:
            image.size
        @unknown default:
            image.size
        }
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

    func testEucLightingBrightnessSliderCommitsTypedWriteAtAccessibilityDynamicType() throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        XCTAssertTrue(app.descendants(matching: .any)["dashboard.screen.eucRide"].waitForExistence(timeout: 20))
        defer { disconnectIfConnected() }
        tapNavigationTab("lighting", title: "Lighting")
        let screen = app.descendants(matching: .any)["dashboard.screen.lighting"]
        XCTAssertTrue(screen.waitForExistence(timeout: 5), app.debugDescription)
        let slider = app.sliders["lighting.brightness"]
        XCTAssertTrue(slider.waitForExistence(timeout: 5), app.debugDescription)
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: slider)
        XCTAssertEqual(XCTWaiter.wait(for: [enabled], timeout: 5), .completed, app.debugDescription)
        // The enabled color wheel owns center drags. Scroll through the padded edge, outside its controls.
        scrollElementFrameIntoViewport(
            slider, in: screen, maxScrolls: 12, requiresFullVisibility: true, horizontalFraction: 0.08)
        XCTAssertEqual(slider.elementType, .slider)
        XCTAssertEqual(slider.label, "Brightness")
        XCTAssertTrue(slider.isEnabled)
        XCTAssertTrue(slider.isHittable)
        XCTAssertTrue(app.windows.firstMatch.frame.insetBy(dx: -2, dy: -2).contains(slider.frame))
        let before = slider.value as? String
        slider.adjust(toNormalizedSliderPosition: 0.25)
        let spoken = try XCTUnwrap(slider.value as? String)
        XCTAssertNotEqual(spoken, before)
        XCTAssertTrue(spoken.hasSuffix("percent"), spoken)
        let percentage = try XCTUnwrap(UInt8(spoken.split(separator: " ").first ?? ""))
        XCTAssertGreaterThan(percentage, 0)
        XCTAssertLessThan(percentage, 100)
        let receipt = app.staticTexts["lighting.connection-state"]
        let expectedPayload = "7e0401" + String(format: "%02x", percentage) + "ff00ff00ef"
        let written = XCTNSPredicateExpectation(
            predicate: NSPredicate(
                format: "value CONTAINS %@ AND value CONTAINS %@ AND value CONTAINS %@ AND value CONTAINS %@",
                "requested=\(percentage);written=\(percentage);writes=1;", "payload=\(expectedPayload);",
                "channel=\((UInt64(0xfff3) << 32) | 0x0000_1000);", "mode=without-response"
            ), object: receipt
        )
        XCTAssertEqual(XCTWaiter.wait(for: [written], timeout: 5), .completed, app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["lighting.control-error"].exists)
        attachScreenshot(of: app, named: "connected Lighting native brightness and Rust write receipt")
    }

    func testEucLightingRouteUsesProductionControls() throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        guard connectedScreen(timeout: 20) != nil else {
            XCTFail("The deterministic EUC fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        tapNavigationTab("lighting", title: "Lighting")

        let lighting = app.descendants(matching: .any)["dashboard.screen.lighting"]
        XCTAssertTrue(lighting.waitForExistence(timeout: 5), app.debugDescription)
        for identifier in [
            "lighting.connection-state",
            "lighting.control-page",
            "lighting.power",
            "lighting.color-wheel",
            "lighting.brightness",
            "lighting.accessory-details",
            "lighting.choose-accessory",
        ] {
            let element = app.descendants(matching: .any)[identifier]
            XCTAssertTrue(element.waitForExistence(timeout: 5), "Missing live Lighting control: \(identifier)")
        }
        XCTAssertEqual(app.sliders["lighting.brightness"].label, "Brightness")
        for label in ["Hue", "Saturation"] {
            let slider = app.sliders.matching(NSPredicate(format: "label == %@", label)).firstMatch
            XCTAssertTrue(slider.waitForExistence(timeout: 5), app.debugDescription)
            XCTAssertFalse(slider.isEnabled, "Disconnected color adjustment must be unavailable to VoiceOver")
        }
        XCTAssertFalse(app.staticTexts["MELK-OC21 6A"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["lighting.command-evidence"].exists)
        app.buttons["lighting.choose-accessory"].tap()
        let addLighting = app.navigationBars["Add lighting"]
        XCTAssertTrue(addLighting.waitForExistence(timeout: 5))
        XCTAssertFalse(app.textFields["lighting.vehicle-association"].exists)
        XCTAssertFalse(app.buttons["lighting.use-current-ride"].exists)
        XCTAssertFalse(app.staticTexts["Other connected apps"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["lighting.restore-toggle"].exists)
        addLighting.buttons["Back"].tap()
        XCTAssertTrue(app.buttons["lighting.quick-preset.red"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["lighting.quick-preset.blue"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["lighting.quick-preset.night"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["lighting.mark-confirmed"].exists)
        app.segmentedControls["lighting.control-page"].buttons["Effects"].tap()
        let effect = app.buttons.matching(identifier: "lighting.effect.1").firstMatch
        if !effect.isHittable { app.swipeUp() }
        XCTAssertTrue(effect.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(effect.isEnabled)
        XCTAssertTrue(effect.isSelected, "The active effect must expose its selection state to VoiceOver")
        XCTAssertTrue(app.descendants(matching: .any)["lighting.effect-speed"].exists)
        app.segmentedControls["lighting.control-page"].buttons["Music"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["lighting.music-sensitivity"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.segmentedControls["lighting.control-page"].buttons["Schedule"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["lighting.schedule"].exists)
        XCTAssertFalse(app.descendants(matching: .any)["melk.validation"].exists)
    }

    private func assertEssentialRideControlsRemainVisibleWithoutScrolling(
        for family: ConnectedDeviceFamily,
        beforeDisconnect: ((XCUIElement) throws -> Void)? = nil
    ) throws {
        XCTAssertTrue(pairAvailableDevice(family))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic \(family.name) fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }

        assertRideWindowHasSettled(screen, landscape: isLandscapeTest)
        assertRideTextSizeReadback(screen, systemCategory: rideTextSizeCategory)
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
                assertMinimumControlDimension(element.frame.height)
            }
        }
        XCTAssertTrue(screen.exists)
        XCTAssertEqual(screen.scrollViews.count, 0)
        let metricLabels =
            family == .euc
            ? ["Battery", "pack", "power", "thermal"]
            : ["voltage", "motor current", "board angle", "controller"]
        let mainMetricLabels =
            rideTextSizeCategory == "UICTContentSizeCategoryAccessibilityXXXL"
            ? Array(metricLabels.prefix(1)) : metricLabels
        let viewport = unobscuredFrame(in: screen, above: app.tabBars.firstMatch)
        for label in mainMetricLabels {
            let metric = screen.descendants(matching: .any).matching(
                NSPredicate(format: "label == %@", label)
            ).firstMatch
            XCTAssertTrue(metric.waitForExistence(timeout: 5), "Missing Ride metric: \(label)")
            XCTAssertTrue(metric.isHittable, "Ride metric requires scrolling: \(label)")
            XCTAssertLessThanOrEqual(
                metric.frame.height, 112,
                "Ride metric row should not consume unused instrument-panel height: \(label)"
            )
            XCTAssertTrue(
                viewport.insetBy(dx: -2, dy: -2).contains(metric.frame),
                "Ride metric is clipped: \(label) \(metric.frame)")
        }
        XCTAssertTrue(navigationTab("ride", title: "Ride").isHittable, app.debugDescription)
        for identifier in ["dashboard.nav.ride", "dashboard.nav.map", "dashboard.nav.more"] {
            let id = String(identifier.dropFirst("dashboard.nav.".count))
            let tab = navigationTab(id, title: id.capitalized)
            XCTAssertTrue(tab.isHittable)
            XCTAssertTrue(
                windowFrame.insetBy(dx: -2, dy: -2).contains(tab.frame),
                "Ride tab is clipped by the viewport: \(identifier) \(tab.frame)"
            )
        }
        try performTextClippingAudit(named: "\(family.name)-ride")
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "\(family.name) ride controls"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        if rideTextSizeCategory == "UICTContentSizeCategoryAccessibilityXXXL" {
            assertAccessibleRideReadings(metricLabels)
        }
        try beforeDisconnect?(screen)
    }

    func testEucAccessibleReadingsPreserveAvailableSecondaryValuesAtAccessibilityDynamicType() throws {
        try assertAvailableSecondaryRideReadings(
            for: .euc,
            expected: [
                ("Battery", "64 and %"),
                ("Time to full", "2 min and Estimated"),
                ("pack", "82.0 and V"),
                ("power", "0.16, kW, and charging input"),
                ("thermal", "31, °C, and ESC 31 °C"),
                ("limp-home", "14.2, mi, and Estimated"),
                ("GPS speed", "6.7, mph, and fresh GPS"),
            ]
        )
    }

    func testVescAccessibleReadingsPreserveAvailableSecondaryValuesAtAccessibilityDynamicType() throws {
        try assertAvailableSecondaryRideReadings(
            for: .vesc,
            expected: [
                ("voltage", "50.4, V, and battery 72% · current 12.0 A"),
                ("motor current", "5.0, A, and phase current"),
                ("board angle", "1.5, °, and nose up"),
                ("controller", "32.0, °C, and motor 28.0 °C"),
            ],
            footpad: "both pressed"
        )
    }

    private func assertAvailableSecondaryRideReadings(
        for family: ConnectedDeviceFamily,
        expected: [(String, String)],
        footpad: String? = nil
    ) throws {
        XCTAssertTrue(pairAvailableDevice(family))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The representative secondary-readings fixture did not open Ride")
            return
        }
        defer { disconnectIfConnected() }
        assertRideWindowHasSettled(screen, landscape: false)
        assertRideTextSizeReadback(screen, systemCategory: rideTextSizeCategory)
        XCTAssertEqual(screen.scrollViews.count, 0)
        let open = app.buttons["ride.readings.open"]
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        XCTAssertTrue(open.isHittable)
        assertMinimumControlDimension(open.frame.height)
        open.tap()
        let detail = app.descendants(matching: .any)["ride.readings.detail"]
        XCTAssertTrue(detail.waitForExistence(timeout: 5))
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "ride.readings.detail").count, 1)
        let scroll = detail.scrollViews.firstMatch
        XCTAssertTrue(scroll.exists)
        for (label, spokenValue) in expected {
            let metric = detail.descendants(matching: .any).matching(NSPredicate(format: "label == %@", label))
                .firstMatch
            XCTAssertTrue(metric.waitForExistence(timeout: 5), "Missing available reading: \(label)")
            scrollElementFrameIntoViewport(metric, in: scroll, maxScrolls: 10, requiresFullVisibility: true)
            XCTAssertTrue(metric.isHittable, "Reading is unreachable: \(label)")
            XCTAssertTrue(
                scroll.frame.insetBy(dx: -2, dy: -2).contains(metric.frame),
                "Reading is clipped: \(label) \(metric.frame) inside \(scroll.frame)"
            )
            XCTAssertEqual(
                metric.value as? String, spokenValue, "Reading lost its exact value, unit, or detail: \(label)")
            let attachment = XCTAttachment(screenshot: app.screenshot())
            attachment.name = "\(family.name) available Readings \(label)"
            attachment.lifetime = .keepAlways
            add(attachment)
        }
        if let footpad {
            let contact = detail.staticTexts[footpad]
            XCTAssertTrue(contact.waitForExistence(timeout: 5))
            scrollElementFrameIntoViewport(contact, in: scroll, maxScrolls: 10, requiresFullVisibility: true)
            XCTAssertTrue(contact.isHittable)
            XCTAssertTrue(scroll.frame.insetBy(dx: -2, dy: -2).contains(contact.frame))
            XCTAssertEqual(contact.label, footpad)
        }
        let hierarchy = XCTAttachment(string: detail.debugDescription)
        hierarchy.name = "\(family.name) available Readings accessibility hierarchy"
        hierarchy.lifetime = .keepAlways
        add(hierarchy)
        let done = app.buttons["ride.readings.done"]
        XCTAssertTrue(done.isHittable)
        assertMinimumControlDimension(done.frame.height)
        assertMinimumControlDimension(done.frame.width)
        done.tap()
        let dismissed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in !detail.exists }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 5), .completed)
        assertRideWindowHasSettled(screen, landscape: false)
        XCTAssertEqual(screen.scrollViews.count, 0)
        XCTAssertTrue(open.isHittable)
        XCTAssertTrue(app.windows.firstMatch.frame.insetBy(dx: -2, dy: -2).contains(open.frame))
    }

    private func assertAccessibleRideReadings(_ labels: [String]) {
        let open = app.buttons["ride.readings.open"]
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        XCTAssertTrue(open.isHittable)
        assertMinimumControlDimension(open.frame.height)
        XCTAssertFalse((open.value as? String ?? "").isEmpty)
        open.tap()
        let detail = app.descendants(matching: .any)["ride.readings.detail"]
        XCTAssertTrue(detail.waitForExistence(timeout: 5))
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "ride.readings.detail").count, 1)
        let scroll = detail.scrollViews.firstMatch
        XCTAssertTrue(scroll.exists)
        for label in labels {
            let metric = detail.descendants(matching: .any).matching(NSPredicate(format: "label == %@", label))
                .firstMatch
            XCTAssertTrue(metric.waitForExistence(timeout: 5), "Missing secondary Ride reading: \(label)")
            scrollElementFrameIntoViewport(
                metric, in: scroll, maxScrolls: 8,
                requiresFullVisibility: metric.frame.height <= scroll.frame.height
            )
            XCTAssertFalse((metric.value as? String ?? "").isEmpty, "Reading must retain its spoken value: \(label)")
        }
        let done = app.buttons["ride.readings.done"]
        XCTAssertTrue(done.isHittable)
        assertMinimumControlDimension(done.frame.height)
        done.tap()
        let dismissed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in !detail.exists }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 5), .completed)
        XCTAssertTrue(open.isHittable)
    }

    func testVescLiveActivityLockScreenSecondarySpeechAcrossAccessibilityCategories() throws {
        // Two real ActivityKit states, native surface assertions and verified dismissal.
        executionTimeAllowance = 360
        try assertLiveActivitySecondarySpeechAtCurrentAccessibilityCategory(lockScreen: true)
    }

    func testVescLiveActivityAutoFixtureExpandedSecondarySpeechAcrossAccessibilityCategories() throws {
        // Two real ActivityKit states, native surface assertions and verified dismissal.
        executionTimeAllowance = 360
        try assertLiveActivitySecondarySpeechAtCurrentAccessibilityCategory(lockScreen: false)
    }

    private func assertLiveActivitySecondarySpeechAtCurrentAccessibilityCategory(lockScreen: Bool) throws {
        let category = UIApplication.shared.preferredContentSizeCategory.rawValue
        let accessibilityCategories = [
            UIContentSizeCategory.accessibilityMedium.rawValue,
            UIContentSizeCategory.accessibilityLarge.rawValue,
            UIContentSizeCategory.accessibilityExtraLarge.rawValue,
            UIContentSizeCategory.accessibilityExtraExtraLarge.rawValue,
            UIContentSizeCategory.accessibilityExtraExtraExtraLarge.rawValue,
        ]
        XCTAssertTrue(
            accessibilityCategories.contains(category),
            "Run this matrix cell with the supported runner's verified --content-size accessibility setting: \(category)"
        )
        // setUp uses an automatic fixture for these selectors. Retire that
        // initial activity before changing the requested state below.
        let initialScreen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(initialScreen.waitForExistence(timeout: 20), app.debugDescription)
        disconnectIfConnected()
        assertLiveActivityFixtureDismissed()
        let states: [(Fixture, LiveActivitySurfaceExpectation)] = [
            (.vescCriticalLiveActivityAuto, .critical),
            (.vescLiveActivityAuto, .nominal),
        ]
        for (stateFixture, expectation) in states {
            try XCTContext.runActivity(
                named: "\(category)-\(expectation.stateName)-\(lockScreen ? "LockScreen" : "Expanded")"
            ) { _ in
                app.terminate()
                app.launchEnvironment = stateFixture.launchEnvironment
                app.launchArguments =
                    stateFixture.launchArguments + [
                        "-UIPreferredContentSizeCategoryName", category,
                        "-CUTOUT_UI_TEST_ENVIRONMENT_READBACK", "YES",
                        "--ui-test-live-activity-readback",
                    ]
                app.launch()
                let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
                XCTAssertTrue(screen.waitForExistence(timeout: 20), app.debugDescription)
                assertRideTextSizeReadback(screen, systemCategory: category)
                if lockScreen {
                    assertVescLiveActivityLockScreen(
                        speed: expectation.speed, headroom: expectation.headroom, stateName: expectation.stateName)
                } else {
                    try assertVescLiveActivityAutoFixture(expectation, assertsSecondarySpeech: true)
                }
                assertLiveActivityFixtureDismissed()
            }
        }
    }

    private func assertLiveActivityFixtureDismissed() {
        // A disconnected fixture ends its ActivityKit activity immediately.
        // Verify dismissal before another state can create an overlapping widget.
        XCUIDevice.shared.press(.home)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        openLockScreen(in: springboard)
        let activity = springboard.descendants(matching: .any)["CutOut ride"]
        let dismissed = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in !activity.exists }, object: activity)
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 10), .completed, springboard.debugDescription)
        app.activate()
    }

    private func assertLiveActivitySafetySpeechOrder(
        in owner: XCUIElement, stateName: String, surfaceName: String
    ) {
        let readings = owner.descendants(matching: .any).matching(
            NSPredicate(format: "label == 'Speed' OR label == 'Headroom'")
        )
        XCTAssertEqual(readings.count, 2, owner.debugDescription)
        // XCTest's descendants query is breadth-first: a shallower Speed node
        // can precede Headroom even when the native speech tree orders Headroom first.
        let hierarchy = owner.debugDescription
        let attachment = XCTAttachment(string: hierarchy)
        attachment.name = "\(stateName)-\(surfaceName)-safety-speech-order"
        attachment.lifetime = .keepAlways
        add(attachment)
        guard let headroom = hierarchy.range(of: "label: 'Headroom'"),
            let speed = hierarchy.range(of: "label: 'Speed'")
        else {
            XCTFail("Missing native safety readings: \(hierarchy)")
            return
        }
        XCTAssertEqual(headroom.lowerBound < speed.lowerBound, stateName == "Critical", hierarchy)
        assertLiveActivityVisualGeometry(in: hierarchy, stateName: stateName, surfaceName: surfaceName)
    }

    private struct LiveActivityNativeGeometryNode {
        let role: String
        let label: String?
        let frame: CGRect
        let indentation: Int
        let parentIndex: Int?
        let sourceLine: String
    }

    private func liveActivityNativeGeometryNodes(in hierarchy: String) -> [LiveActivityNativeGeometryNode] {
        guard let start = hierarchy.range(of: "Element subtree:\n"),
            let end = hierarchy.range(of: "Path to element:", range: start.upperBound..<hierarchy.endIndex)
        else {
            XCTFail("Missing coherent native Element subtree: \(hierarchy)")
            return []
        }
        do {
            let framePattern = try NSRegularExpression(
                pattern: #"^\s*(?:→)?(Other|StaticText),.*?\{\{([^,]+), ([^}]+)\}, \{([^,]+), ([^}]+)\}\}"#)
            let labelPattern = try NSRegularExpression(pattern: #"label: '([^']*)'"#)
            var nodes: [LiveActivityNativeGeometryNode] = []
            var ancestors: [Int] = []
            for line in hierarchy[start.upperBound..<end.lowerBound].split(separator: "\n") {
                let text = String(line)
                let range = NSRange(text.startIndex..<text.endIndex, in: text)
                guard let match = framePattern.firstMatch(in: text, range: range) else { continue }
                let captures = (1...5).compactMap { Range(match.range(at: $0), in: text).map { String(text[$0]) } }
                guard captures.count == 5 else {
                    XCTFail("Malformed native geometry: \(text)")
                    return []
                }
                let numbers = captures.dropFirst().compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
                guard numbers.count == 4, numbers.allSatisfy(\.isFinite) else {
                    XCTFail("Nonfinite or missing native frame: \(text)")
                    return []
                }
                let label = labelPattern.firstMatch(in: text, range: range)
                    .flatMap { Range($0.range(at: 1), in: text) }.map { String(text[$0]) }
                let indentation = text.prefix(while: \.isWhitespace).count
                while let last = ancestors.last, nodes[last].indentation >= indentation {
                    ancestors.removeLast()
                }
                nodes.append(
                    LiveActivityNativeGeometryNode(
                        role: captures[0], label: label,
                        frame: CGRect(x: numbers[0], y: numbers[1], width: numbers[2], height: numbers[3]),
                        indentation: indentation, parentIndex: ancestors.last,
                        sourceLine: text))
                ancestors.append(nodes.count - 1)
            }
            return nodes
        } catch {
            XCTFail("Native geometry parser failed: \(error)")
            return []
        }
    }

    private func assertLiveActivityVisualGeometry(
        in hierarchy: String, stateName: String, surfaceName: String
    ) {
        // Reuse one immutable native snapshot. Resolving indexed elements and then
        // reading each frame races ActivityKit refresh and can change the collection.
        let nodes = liveActivityNativeGeometryNodes(in: hierarchy)
        guard let root = nodes.first, root.role == "Other" else {
            XCTFail("Missing native owner frame: \(hierarchy)")
            return
        }
        let ownerFrame = root.frame
        // SpringBoard can expose the oversized composition as regular.view while
        // a same-origin, full-width native container clips it to the actual viewport.
        // Read that native geometry rather than assuming a fixed Island height.
        let alignedContainers = nodes.filter { $0.role == "Other" }.map(\.frame).filter {
            abs($0.minX - ownerFrame.minX) <= 1
                && abs($0.minY - ownerFrame.minY) <= 1
                && abs($0.width - ownerFrame.width) <= 1
                && $0.height > 0
        }
        let viewport = alignedContainers.min(by: { $0.height < $1.height }) ?? ownerFrame
        let context = "\(stateName) \(surfaceName): viewport=\(viewport), owner=\(ownerFrame)\n\(hierarchy)"
        XCTAssertGreaterThan(viewport.width, 0, context)
        XCTAssertGreaterThan(viewport.height, 0, context)
        if stateName == "Critical" {
            assertLiveActivityVisibleCriticalWarning(in: viewport, surfaceName: surfaceName)
        }

        let speed = nodes.indices.filter { nodes[$0].role == "Other" && nodes[$0].label == "Speed" }
        let headroom = nodes.indices.filter { nodes[$0].label == "Headroom" }
        XCTAssertEqual(speed.count, 1, context)
        XCTAssertEqual(headroom.count, 1, context)
        guard let speedIndex = speed.first, let headroomIndex = headroom.first else { return }
        assertLiveActivityFrame(nodes[speedIndex].frame, fitsIn: viewport, context: context)
        // The outer owner can include virtual Footer speech beyond its physical
        // body. Bound the lowest shared native body of Speed and Footer instead.
        // The retained cropped Grid still makes that body exceed the viewport.
        var footerAncestors: Set<Int> = []
        var current: Int? = headroomIndex
        while let index = current {
            footerAncestors.insert(index)
            current = nodes[index].parentIndex
        }
        current = speedIndex
        while let index = current, !footerAncestors.contains(index) {
            current = nodes[index].parentIndex
        }
        guard let bodyIndex = current, nodes[bodyIndex].role == "Other" else {
            XCTFail("Missing native shared Speed/Footer body: \(context)")
            return
        }
        assertLiveActivityFrame(nodes[bodyIndex].frame, fitsIn: viewport, context: context)

        // Footer speech is a virtual representation, including Headroom. Do not
        // infer visible chip bounds from those replacement accessibility nodes.
        if surfaceName == "LockScreen" {
            let header = nodes.filter { $0.role == "Other" && $0.label == "CutOut ride" }
            XCTAssertEqual(header.count, 1, context)
            if header.first?.sourceLine.localizedCaseInsensitiveContains("stale") == true {
                let status = nodes.filter { $0.role == "StaticText" && $0.label == "Stale" }
                let identity = nodes.filter { $0.role == "StaticText" && $0.label == "Refloat VESC" }
                XCTAssertEqual(status.count, 1, context)
                XCTAssertEqual(identity.count, 1, context)
                if let status = status.first, let identity = identity.first {
                    // Both use the same scaled identity font. A two-line status was
                    // twice the actual one-line vehicle height in the retained AX3 failure.
                    XCTAssertLessThanOrEqual(status.frame.height, identity.frame.height + 1, context)
                    assertLiveActivityFrame(status.frame, fitsIn: viewport, context: context)
                    assertLiveActivityFrame(identity.frame, fitsIn: viewport, context: context)
                }
            }
        }
    }

    private func assertLiveActivityVisibleCriticalWarning(in viewport: CGRect, surfaceName: String) {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let canvas = springboard.frame
        let screenshot = springboard.screenshot()
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = "Critical-\(surfaceName)-visible-warning-OCR"
        attachment.lifetime = .keepAlways
        add(attachment)
        guard canvas.width > 0, canvas.height > 0, let image = screenshot.image.cgImage else {
            XCTFail("Missing native screenshot/canvas for warning recognition")
            return
        }
        let region = CGRect(
            x: (viewport.minX - canvas.minX) / canvas.width,
            y: 1 - (viewport.maxY - canvas.minY) / canvas.height,
            width: viewport.width / canvas.width, height: viewport.height / canvas.height
        ).intersection(CGRect(x: 0, y: 0, width: 1, height: 1))
        guard !region.isNull, !region.isEmpty else {
            XCTFail("Missing visible native widget region: \(viewport), canvas=\(canvas)")
            return
        }
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["en-US"]
        request.usesLanguageCorrection = false
        request.minimumTextHeight = 0
        request.regionOfInterest = region
        do {
            try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
            let lines = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
            let recognized = lines.joined(separator: " ").lowercased().filter { $0.isLetter || $0.isNumber }
            let diagnostic = XCTAttachment(
                string: "viewport=\(viewport); canvas=\(canvas); region=\(region)\n" + lines.joined(separator: "\n"))
            diagnostic.name = "Critical-\(surfaceName)-visible-warning-recognition"
            diagnostic.lifetime = .keepAlways
            add(diagnostic)
            XCTAssertTrue(
                recognized.contains("reduceacceleration"),
                "Visible warning must retain its full words, including when wrapped: \(lines)")
        } catch {
            XCTFail("Native visible warning recognition failed: \(error)")
        }
    }

    @discardableResult
    private func assertRouteMarkerCaption(
        _ expectedCaption: String,
        in mapFrame: CGRect,
        screenshot: XCUIScreenshot,
        attachmentName: String
    ) -> CGRect? {
        let screenshotImage = screenshot.image
        guard let image = screenshotImage.cgImage else {
            XCTFail("Missing simulator screenshot for route-marker caption recognition")
            return nil
        }
        guard let orientation = visionOrientation(for: screenshotImage.imageOrientation) else {
            XCTFail("Unsupported screenshot orientation: \(screenshotImage.imageOrientation)")
            return nil
        }
        let canvas = app.frame
        guard canvas.width > 0, canvas.height > 0 else {
            XCTFail("Missing simulator canvas for route-marker caption recognition: \(canvas)")
            return nil
        }
        let recognitionFrame = mapFrame.intersection(canvas)
        guard !recognitionFrame.isNull, !recognitionFrame.isEmpty else {
            XCTFail("Missing visible map frame for route-marker caption recognition: map=\(mapFrame), canvas=\(canvas)")
            return nil
        }
        let mapRegion = CGRect(
            x: (recognitionFrame.minX - canvas.minX) / canvas.width,
            y: 1 - (recognitionFrame.maxY - canvas.minY) / canvas.height,
            width: recognitionFrame.width / canvas.width,
            height: recognitionFrame.height / canvas.height
        ).intersection(CGRect(x: 0, y: 0, width: 1, height: 1))
        guard !mapRegion.isNull, !mapRegion.isEmpty else {
            XCTFail("Missing rendered map region for route-marker caption: map=\(mapFrame), canvas=\(canvas)")
            return nil
        }

        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["en-US"]
        request.usesLanguageCorrection = false
        request.minimumTextHeight = 0
        request.regionOfInterest = mapRegion
        let expectedWords = Set(expectedCaption.lowercased().split(whereSeparator: \.isWhitespace).map(String.init))
        do {
            try VNImageRequestHandler(cgImage: image, orientation: orientation, options: [:]).perform([request])
            let lines = (request.results ?? []).compactMap { observation -> (String, CGRect)? in
                guard let text = observation.topCandidates(1).first?.string else { return nil }
                let normalized = text.lowercased().filter { $0.isLetter || $0.isNumber || $0.isWhitespace }
                let words = Set(normalized.split(whereSeparator: \.isWhitespace).map(String.init))
                guard !expectedWords.isDisjoint(with: words) else { return nil }
                let box = observation.boundingBox
                let frame = CGRect(
                    x: recognitionFrame.minX + box.minX * recognitionFrame.width,
                    y: recognitionFrame.minY + (1 - box.maxY) * recognitionFrame.height,
                    width: box.width * recognitionFrame.width,
                    height: box.height * recognitionFrame.height
                )
                return (text, frame)
            }
            let recognized = lines.map(\.0).joined(separator: " ").lowercased()
                .filter { $0.isLetter || $0.isNumber || $0.isWhitespace }
            let recognizedWords = Set(recognized.split(whereSeparator: \.isWhitespace).map(String.init))
            let captionFrames = lines.map(\.1)
            let captionFrame = captionFrames.dropFirst().reduce(captionFrames.first ?? .zero) { partial, next in
                partial.union(next)
            }
            let diagnostic =
                "caption=\(expectedCaption); recognized=\(lines.map(\.0)); captionFrame=\(captionFrame); "
                + "mapFrame=\(mapFrame); recognitionFrame=\(recognitionFrame); mapRegion=\(mapRegion); canvas=\(canvas); "
                + "imageOrientation=\(screenshotImage.imageOrientation); imageSize=\(screenshotImage.size); "
                + "imageScale=\(screenshotImage.scale); pixelSize=\(image.width)x\(image.height)"
            let attachment = XCTAttachment(string: diagnostic)
            attachment.name = attachmentName + "-recognition"
            attachment.lifetime = .keepAlways
            add(attachment)

            for word in expectedWords {
                XCTAssertTrue(recognizedWords.contains(word), "Caption text must be visibly rendered: \(diagnostic)")
            }
            XCTAssertFalse(captionFrame.isEmpty, "Caption must have rendered text bounds: \(diagnostic)")
            XCTAssertTrue(
                mapFrame.insetBy(dx: -2, dy: -2).contains(captionFrame),
                "Caption text must remain inside the rendered map: \(diagnostic)"
            )
            XCTAssertLessThanOrEqual(
                captionFrame.width, mapFrame.width * 0.8,
                "Caption should wrap within a readable fraction of the map width: \(diagnostic)"
            )
            guard !captionFrame.isEmpty else { return nil }
            return captionFrame
        } catch {
            XCTFail("Route-marker caption recognition failed: \(error)")
            return nil
        }
    }

    private func visionOrientation(for orientation: UIImage.Orientation) -> CGImagePropertyOrientation? {
        switch orientation {
        case .up: return .up
        case .upMirrored: return .upMirrored
        case .down: return .down
        case .downMirrored: return .downMirrored
        case .left: return .left
        case .leftMirrored: return .leftMirrored
        case .right: return .right
        case .rightMirrored: return .rightMirrored
        @unknown default: return nil
        }
    }

    private func assertLiveActivityFrame(_ frame: CGRect, fitsIn viewport: CGRect, context: String) {
        // Native wrapper origins differ by a display pixel in retained SpringBoard
        // trees. One point tolerates that alignment, not clipped text or an oversized grid.
        let tolerance: CGFloat = 1
        XCTAssertGreaterThan(frame.width, 0, context)
        XCTAssertGreaterThan(frame.height, 0, context)
        XCTAssertGreaterThanOrEqual(frame.minX, viewport.minX - tolerance, context)
        XCTAssertGreaterThanOrEqual(frame.minY, viewport.minY - tolerance, context)
        XCTAssertLessThanOrEqual(frame.maxX, viewport.maxX + tolerance, context)
        XCTAssertLessThanOrEqual(frame.maxY, viewport.maxY + tolerance, context)
    }

    private func assertLiveActivitySecondarySpeech(in surface: XCUIApplication, stateName: String) {
        for (label, expected) in [("Beeps", "waiting for data"), ("Temp", "32, °C, vehicle telemetry")] {
            let matches = surface.descendants(matching: .any).matching(NSPredicate(format: "label == %@", label))
            XCTAssertEqual(matches.count, 1, "\(stateName): \(surface.debugDescription)")
            let actual = matches.firstMatch.value as? String
            XCTAssertTrue(
                actual == expected || (label == "Temp" && actual == expected + ", stale"),
                matches.firstMatch.debugDescription)
        }
    }

    func testVescLiveActivityAutoFixtureStartsAnAccessibleRide() throws {
        try assertVescLiveActivityAutoFixture(.nominal)
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

    func testVescLiveActivityAutoFixtureRemainsInspectableAfterAppProcessTerminates() {
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20), app.debugDescription)
        let foregroundSpeed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(foregroundSpeed.waitForExistence(timeout: 5), app.debugDescription)
        let connected = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS %@", "17.9"),
            object: foregroundSpeed
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [connected], timeout: 12),
            .completed,
            foregroundSpeed.debugDescription
        )
        defer {
            app.launch()
            disconnectIfConnected()
        }

        app.terminate()
        XCTAssertEqual(app.state, .notRunning)

        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        dismissLocationPromptIfNeeded(in: springboard)
        XCUIDevice.shared.press(.home)
        springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.04))
            .press(forDuration: 1)
        let device = springboard.descendants(matching: .any)["Device"]
        XCTAssertTrue(device.waitForExistence(timeout: 5), springboard.debugDescription)
        let deviceValue = device.value as? String
        XCTAssertTrue(
            deviceValue?.localizedCaseInsensitiveContains("stale") == true,
            device.debugDescription
        )
        XCTAssertFalse(
            deviceValue?.localizedCaseInsensitiveContains("disconnected") == true,
            device.debugDescription
        )
        attachScreenshot(of: springboard, named: "Terminated process Expanded Dynamic Island")

        XCUIDevice.shared.press(.home)
        openLockScreen(in: springboard)
        self.assertVescLiveActivityLockScreenSemantics(
            in: springboard,
            speed: "17.9",
            headroom: "good",
            connectionState: "stale",
            stateName: "Terminated process"
        )
    }

    func testVescLiveActivityAutoFixtureCleanupLaunchEndsRetainedActivity() {
        assertLiveActivityForegroundAcknowledged()
        let readback = app.staticTexts["dashboard.lifecycle-readback"]
        let nativeActivity = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS 'nativeActivityCount=1;'"), object: readback)
        XCTAssertEqual(XCTWaiter.wait(for: [nativeActivity], timeout: 15), .completed, app.debugDescription)

        app.terminate()
        XCTAssertEqual(app.state, .notRunning)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        dismissLocationPromptIfNeeded(in: springboard)
        XCUIDevice.shared.press(.home)
        springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.04))
            .press(forDuration: 1)
        let retainedDevice = springboard.descendants(matching: .any)["Device"]
        XCTAssertTrue(retainedDevice.waitForExistence(timeout: 5), springboard.debugDescription)
        attachScreenshot(of: springboard, named: "Retained Live Activity before cleanup launch")

        launchLiveActivityCleanup()
        assertSystemLiveActivitiesEnded()
        XCTAssertFalse(app.buttons["dashboard.disconnect"].exists, app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["dashboard.screen.vescRide"].exists, app.debugDescription)
    }

    func testVescCriticalLiveActivityAutoFixtureStartsAnAccessibleRide() throws {
        try assertVescLiveActivityAutoFixture(.critical, assertsLockScreen: true)
    }

    func testVescLiveActivityAutoFixtureStartsAnAccessibleRideInLandscape() throws {
        try assertVescLiveActivityAutoFixture(.nominal)
    }

    func testVescCriticalLiveActivityAutoFixtureStartsAnAccessibleRideInLandscape() throws {
        try assertVescLiveActivityAutoFixture(.critical)
    }

    func testVescUnavailableLiveActivityAutoFixturePreservesUnavailableSemantics() throws {
        try assertVescLiveActivityAutoFixture(.unavailable)
    }

    func testVescUnavailableLiveActivityAutoFixturePreservesUnavailableSemanticsInLandscape() throws {
        try assertVescLiveActivityAutoFixture(.unavailable)
    }

    func testVescStaleLiveActivityAutoFixturePreservesStaleSemantics() throws {
        try assertVescLiveActivityAutoFixture(.stale)
    }

    func testVescStaleLiveActivityAutoFixturePreservesStaleSemanticsInLandscape() throws {
        try assertVescLiveActivityAutoFixture(.stale)
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

    func
        testVescUnavailableLiveActivityLockScreenPreservesUnavailableSemanticsInLightAppearanceAtAccessibilityDynamicType()
    {
        assertVescLiveActivityLockScreen(
            speed: "unavailable",
            headroom: "unavailable",
            stateName: "Unavailable"
        )
    }

    func
        testVescUnavailableLiveActivityLockScreenPreservesUnavailableSemanticsInDarkAppearanceAtAccessibilityDynamicType()
    {
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

    private func assertLiveActivityForegroundAcknowledged() {
        let readback = app.staticTexts["dashboard.lifecycle-readback"]
        XCTAssertTrue(readback.waitForExistence(timeout: 5), app.debugDescription)
        let acknowledged = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS 'activityAcknowledged=true'"), object: readback)
        let result = XCTWaiter.wait(for: [acknowledged], timeout: 15)
        let value = readback.value as? String ?? "No lifecycle readback value"
        let attachment = XCTAttachment(string: value)
        attachment.name = "Actual foreground Live Activity lifecycle receipt"
        attachment.lifetime = .keepAlways
        add(attachment)
        XCTAssertEqual(result, .completed, value)
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

        let foregroundSpeed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(foregroundSpeed.waitForExistence(timeout: 5), app.debugDescription)
        let telemetryReceived = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS[c] %@", expectedSpeed), object: foregroundSpeed)
        XCTAssertEqual(XCTWaiter.wait(for: [telemetryReceived], timeout: 12), .completed, app.debugDescription)
        if name.contains("AcrossAccessibilityCategories") { assertLiveActivityForegroundAcknowledged() }
        XCUIDevice.shared.press(.home)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        dismissLocationPromptIfNeeded(in: springboard)
        XCUIDevice.shared.press(.home)
        let compactSpeed = springboard.descendants(matching: .any)["Speed"]
        XCTAssertTrue(compactSpeed.waitForExistence(timeout: 5), springboard.debugDescription)
        let activityReceived = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS[c] %@", expectedSpeed), object: compactSpeed)
        XCTAssertEqual(XCTWaiter.wait(for: [activityReceived], timeout: 12), .completed, springboard.debugDescription)
        openLockScreen(in: springboard)
        assertVescLiveActivityLockScreenSemantics(
            in: springboard,
            speed: expectedSpeed,
            headroom: expectedHeadroom,
            stateName: stateName
        )
    }

    private func openLockScreen(in springboard: XCUIApplication) {
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
    }

    private func assertVescLiveActivityLockScreenSemantics(
        in springboard: XCUIApplication,
        speed expectedSpeed: String,
        headroom expectedHeadroom: String,
        connectionState expectedConnectionState: String? = nil,
        stateName: String
    ) {
        let hierarchy = XCTAttachment(string: springboard.debugDescription)
        hierarchy.name = "\(stateName)-lock-screen-accessibility-hierarchy"
        hierarchy.lifetime = .keepAlways
        add(hierarchy)
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
        if stateName == "Nominal" || stateName == "Critical" {
            assertLiveActivitySecondarySpeech(in: springboard, stateName: stateName)
            let owner = springboard.otherElements["activity-content-view"]
            XCTAssertTrue(owner.exists, springboard.debugDescription)
            assertLiveActivitySafetySpeechOrder(in: owner, stateName: stateName, surfaceName: "LockScreen")
        }
        if let expectedConnectionState {
            let device = springboard.descendants(matching: .any)["Device"]
            XCTAssertTrue(device.waitForExistence(timeout: 5), springboard.debugDescription)
            XCTAssertTrue(
                (device.value as? String)?.localizedCaseInsensitiveContains(expectedConnectionState) == true,
                device.debugDescription
            )
        }
        attachScreenshot(of: springboard, named: "\(stateName) Lock Screen Live Activity")
    }

    private struct LiveActivitySurfaceExpectation {
        let stateName: String
        let speed: String
        let headroom: String
        let compactPwm: String?
        let compactTrailingLabel: String
        let compactTrailingValue: String
        let connectionStates: [String]

        static let nominal = Self(
            stateName: "Nominal",
            speed: "17.9",
            headroom: "good",
            compactPwm: "23",
            compactTrailingLabel: "Battery",
            compactTrailingValue: "72",
            connectionStates: ["connected", "stale"]
        )
        static let critical = Self(
            stateName: "Critical",
            speed: "17.9",
            headroom: "reduce acceleration",
            compactPwm: "85",
            compactTrailingLabel: "Headroom",
            compactTrailingValue: "reduce acceleration",
            connectionStates: ["connected", "stale"]
        )
        static let unavailable = Self(
            stateName: "Unavailable",
            speed: "unavailable",
            headroom: "unavailable",
            compactPwm: nil,
            compactTrailingLabel: "Battery",
            compactTrailingValue: "unavailable",
            connectionStates: ["waiting for telemetry"]
        )
        static let stale = Self(
            stateName: "Stale",
            speed: "stale",
            headroom: "good",
            compactPwm: "23",
            compactTrailingLabel: "Battery",
            compactTrailingValue: "72",
            connectionStates: ["stale"]
        )
    }

    private func assertVescLiveActivityAutoFixture(
        _ expectation: LiveActivitySurfaceExpectation,
        assertsLockScreen: Bool = false,
        assertsSecondarySpeech: Bool = false
    ) throws {
        let screen = app.descendants(matching: .any)["dashboard.screen.vescRide"]
        XCTAssertTrue(screen.waitForExistence(timeout: 20), app.debugDescription)
        defer {
            app.activate()
            disconnectIfConnected()
        }
        let speed = app.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        let liveSpeed = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in
                (speed.value as? String)?.localizedCaseInsensitiveContains(expectation.speed) == true
            },
            object: speed
        )
        XCTAssertEqual(XCTWaiter.wait(for: [liveSpeed], timeout: 10), .completed)

        if name.contains("AcrossAccessibilityCategories") { assertLiveActivityForegroundAcknowledged() }
        XCUIDevice.shared.press(.home)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        dismissLocationPromptIfNeeded(in: springboard)
        XCUIDevice.shared.press(.home)
        let stateName = expectation.stateName
        let compactSpeed = springboard.descendants(matching: .any)["Speed"]
        XCTAssertTrue(compactSpeed.waitForExistence(timeout: 5), springboard.debugDescription)
        let compactUpdate = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS[c] %@", expectation.speed),
            object: compactSpeed
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [compactUpdate], timeout: 12),
            .completed,
            compactSpeed.debugDescription
        )
        if let compactPwm = expectation.compactPwm {
            XCTAssertTrue(
                (compactSpeed.value as? String)?.localizedCaseInsensitiveContains(compactPwm) == true,
                compactSpeed.debugDescription
            )
        } else {
            XCTAssertFalse(
                (compactSpeed.value as? String)?.localizedCaseInsensitiveContains("PWM") == true,
                compactSpeed.debugDescription
            )
        }
        let compactTrailingValue = springboard.descendants(matching: .any)[expectation.compactTrailingLabel]
        XCTAssertTrue(compactTrailingValue.waitForExistence(timeout: 5), springboard.debugDescription)
        XCTAssertTrue(
            (compactTrailingValue.value as? String)?.localizedCaseInsensitiveContains(expectation.compactTrailingValue)
                == true,
            compactTrailingValue.debugDescription
        )
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
        if stateName == "Nominal" {
            let islandBeeps = springboard.descendants(matching: .any)["Beeps"]
            XCTAssertTrue(islandBeeps.waitForExistence(timeout: 5), springboard.debugDescription)
            XCTAssertGreaterThanOrEqual(
                islandHeadroom.frame.width,
                islandBeeps.frame.width + 4,
                "Expanded Dynamic Island must prioritize Headroom over secondary metrics: \(islandHeadroom.debugDescription)"
            )
        }
        XCTAssertTrue(
            (islandHeadroom.value as? String)?.localizedCaseInsensitiveContains(expectation.headroom) == true,
            islandHeadroom.debugDescription
        )
        if assertsSecondarySpeech {
            assertLiveActivitySecondarySpeech(in: springboard, stateName: stateName)
        }
        let expandedActivity = springboard.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS 'CutOut' AND label CONTAINS 'Speed' AND label CONTAINS 'Headroom'")
        ).firstMatch
        XCTAssertTrue(expandedActivity.exists, springboard.debugDescription)
        let orderedSafetyValues = expandedActivity.descendants(matching: .any).matching(
            NSPredicate(format: "label == 'Speed' OR label == 'Headroom'")
        )
        XCTAssertEqual(orderedSafetyValues.count, 2)
        assertLiveActivitySafetySpeechOrder(in: expandedActivity, stateName: stateName, surfaceName: "Expanded")
        attachScreenshot(of: springboard, named: "\(stateName) Expanded Dynamic Island")
        if assertsLockScreen {
            XCUIDevice.shared.press(.home)
            openLockScreen(in: springboard)
            assertVescLiveActivityLockScreenSemantics(
                in: springboard,
                speed: expectation.speed,
                headroom: expectation.headroom,
                stateName: stateName
            )
        }
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

    func
        testFailedVescConnectionPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertFailedVescConnectionAccessibility(
            usesLocalizedText: true
        )
    }

    func testFailedVescConnectionPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertFailedVescConnectionAccessibility()
    }

    func testVescReconnectKeepsRideAccessible() throws {
        try assertReconnectAccessibility(
            for: .vesc,
            auditExclusions: .dynamicType,
            ignoringUnavailableMetricPlaceholderContrastWarning: true
        )
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

    func
        testVescReconnectKeepsRideAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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
        try assertReconnectAccessibility(
            for: .euc,
            auditExclusions: [.dynamicType, .textClipped]
        )
    }

    func testEucReconnectKeepsRideAccessibleInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .euc)
    }

    func testEucReconnectKeepsRideAccessibleInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertReconnectAccessibility(for: .euc)
    }

    func
        testEucReconnectKeepsRideAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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
        ignoringUnavailableMetricPlaceholderContrastWarning: Bool = false,
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
        let hierarchy = XCTAttachment(string: app.debugDescription)
        hierarchy.name = "Ride accessibility hierarchy before audit"
        hierarchy.lifetime = .keepAlways
        XCTContext.runActivity(named: "Capture Ride accessibility hierarchy") { activity in
            activity.add(hierarchy)
        }
        try performVisibleLayoutAccessibilityAudit(
            excluding: auditExclusions,
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning,
            ignoringUnavailableMetricPlaceholderContrastWarning: ignoringUnavailableMetricPlaceholderContrastWarning
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
        XCTAssertTrue(
            connectionStatus.isHittable, "Connection failure must be visible without test-controlled scrolling")
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

    func testEucBmsShowsMeasurementsWithoutDeveloperDiagnostics() throws {
        _ = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }
        XCTAssertTrue(app.buttons["bms.pack.lowest"].exists)
        XCTAssertTrue(app.buttons["bms.pack.highest"].exists)
        XCTAssertFalse(app.staticTexts["bms.diagnostics"].exists)
        XCTAssertFalse(app.staticTexts["Display modes"].exists)
    }

    func testEucBms252ReadingsCanFindLastReading() throws {
        try assertLargeBmsSearch(count: 252)
    }

    func testEucBms224ReadingsCanFindLastReading() throws {
        try assertLargeBmsSearch(count: 224)
    }

    private func assertLargeBmsSearch(count: Int) throws {
        let screen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsCellMap40S"))
        XCTAssertTrue(app.buttons["bms.pack.highest"].isHittable)
        attachScreenshot(of: app, named: "Battery overview - \(count) readings")
        let disclosure = app.buttons["All cell voltages"]
        scrollElementFrameIntoViewport(disclosure, in: screen, maxScrolls: 8)
        disclosure.tap()
        let search = app.textFields["bms.pack.search"]
        scrollElementFrameIntoViewport(search, in: screen, maxScrolls: 8)
        search.tap()
        search.typeText("\(count)\n")
        let last = reachableBmsGroup(count, in: screen)
        last.tap()
        XCTAssertEqual(app.staticTexts["bms.detail.selected-group"].label, "Reading \(count)")
        XCTAssertTrue(app.staticTexts["bms.detail.voltage"].isHittable)
    }

    func testEucBmsSixtyReadingsOverviewAndDetail() throws {
        let screen = try XCTUnwrap(openEucBmsScreen(identifier: "dashboard.screen.bmsCellMap40S"))
        XCTAssertTrue(app.buttons["bms.pack.lowest"].isHittable)
        XCTAssertTrue(app.buttons["bms.pack.highest"].isHittable)
        XCTAssertFalse(app.staticTexts["Display modes"].exists)
        XCTAssertFalse(app.staticTexts["bms.diagnostics"].exists)
        attachScreenshot(of: app, named: "Battery overview - 60 readings")
        app.buttons["bms.pack.highest"].tap()
        XCTAssertTrue(app.staticTexts["bms.detail.selected-group"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["bms.detail.selected-group"].label, "Reading 60")
        XCTAssertTrue(app.staticTexts["bms.detail.voltage"].isHittable)
        XCTAssertFalse(app.staticTexts["Resistance"].exists)
        attachScreenshot(of: app, named: "Battery reading 60")
        app.buttons["bms.detail.previous"].tap()
        XCTAssertEqual(app.staticTexts["bms.detail.selected-group"].label, "Reading 59")
        app.buttons["bms.detail.back"].tap()
        let disclosure = app.buttons["All cell voltages"]
        scrollElementFrameIntoViewport(disclosure, in: screen, maxScrolls: 8)
        XCTAssertTrue(disclosure.isHittable)
        disclosure.tap()
        let last = reachableBmsGroup(60, in: screen)
        attachScreenshot(of: app, named: "Battery all readings - last row")
        last.tap()
        XCTAssertEqual(app.staticTexts["bms.detail.selected-group"].label, "Reading 60")
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

    func
        testEucBmsOverviewPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertEucBmsOverviewAccessibility(scrollsBeforeAudit: true)
    }

    func testEucBmsOverviewPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucBmsOverviewAccessibility()
    }

    func testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        let bmsScreen = try XCTUnwrap(openEucBmsMap())
        defer { disconnectIfConnected() }

        XCTAssertTrue(bmsScreen.exists)
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.more"].isSelected)
        XCTAssertTrue(reachableBmsGroup(7, in: bmsScreen).isHittable)
        restoreDashboardViewport(bmsScreen)
        revealBottomEdgeContent(in: bmsScreen)
        try performVisibleLayoutAccessibilityAudit()
    }

    func testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast()
        throws
    {
        try assertEucBmsAccessibility(
            excluding: [],
            assertsEnglishMetric: false,
            scrollsBeforeAudit: 1
        )
    }

    func
        testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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
            auditTopTitle: "Battery"
        )
    }

    func testEucBmsDetailPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucBmsDetailAccessibility(
            ignoringClippedBmsDetailBoundaryWarnings: true,
            auditTopTitle: "Battery"
        )
    }

    func testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast()
        throws
    {
        try assertEucBmsDetailAccessibility(excluding: [])
    }

    func
        testEucBmsDetailPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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

    func
        testEucNoBmsSurfacePassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast()
        throws
    {
        try assertEucNoBmsSurface(auditExclusions: [])
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithIncreasedContrast() throws {
        // The real Accessibility XXXL + Increased Contrast route owns these
        // categories; Xcode's standard-size simulation misreports semantic fonts.
        try assertEucNoBmsSurface(auditExclusions: [.dynamicType, .textClipped])
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucNoBmsSurface()
    }

    func testEucNoBmsSurfacePassesAccessibilityAuditWithIncreasedContrastInLandscapeAtAccessibilityDynamicType() throws
    {
        try assertEucNoBmsSurface(auditExclusions: [])
    }

    func
        testEucNoBmsSurfacePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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

    func testEucUnknownTopologyPassesAccessibilityAuditWithIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertEucUnknownTopologySurface(auditExclusions: [])
    }

    func
        testEucUnknownTopologyPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast()
        throws
    {
        try assertEucUnknownTopologySurface(auditExclusions: [])
    }

    func
        testEucUnknownTopologyPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertEucUnknownTopologySurface(auditExclusions: [])
    }

    func testEucUnknownTopologyPassesAccessibilityAuditInRightToLeftLayout() throws {
        try assertEucUnknownTopologySurface()
    }

    func testEucRideAndBmsPassAccessibilityAuditWithIncreasedContrast() throws {
        // The corresponding real Accessibility XXXL route owns Dynamic Type.
        try assertEucBmsAccessibility(excluding: [.dynamicType])
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
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }
        let status = screen.descendants(matching: .any)["ride.hero.status"]
        let speed = screen.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        let statusFrame = status.frame
        let speedFrame = speed.frame
        let aged = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let speed = object as? XCUIElement else { return false }
                return (speed.value as? String ?? "").contains("stale")
            }, object: speed
        )
        XCTAssertEqual(XCTWaiter.wait(for: [aged], timeout: 5), .completed)
        XCTAssertFalse(status.label.contains("Telemetry stale"))
        XCTAssertEqual(status.frame, statusFrame)
        XCTAssertEqual(speed.frame, speedFrame)
        XCTAssertEqual(screen.scrollViews.count, 0)

    }

    func testVescStaleTelemetryKeepsRideLayoutFixedAtAccessibilityDynamicType() throws {
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
            XCTFail("The deterministic Refloat fixture did not open its Ride screen")
            return
        }
        defer { disconnectIfConnected() }
        let status = screen.descendants(matching: .any)["ride.hero.status"]
        let speed = screen.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(status.label.contains(label), status.debugDescription)
        XCTAssertTrue(status.isHittable)
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertLessThan(status.frame.minY, speed.frame.minY)
        XCTAssertFalse(screen.descendants(matching: .any)["vesc.warning.active"].exists)
        XCTAssertEqual(screen.scrollViews.count, 0)

    }

    func testVescStaleTelemetryUsesStatusPillAtAccessibilityDynamicType() {
        assertStatusPillReplacesWarningBlock(
            for: .vesc,
            warningIdentifier: "vesc.warning.telemetry-stale"
        )
    }

    func testEucStaleTelemetryUsesStatusPillAtAccessibilityDynamicType() {
        assertStatusPillReplacesWarningBlock(for: .euc, warningIdentifier: "euc.warning")
    }

    func testVescPendingTelemetryUsesStatusPillAtAccessibilityDynamicType() {
        assertStatusPillReplacesWarningBlock(
            for: .vesc,
            warningIdentifier: "vesc.warning.telemetry-pending"
        )
    }

    private func assertStatusPillReplacesWarningBlock(
        for family: ConnectedDeviceFamily,
        warningIdentifier: String
    ) {
        XCTAssertTrue(pairAvailableDevice(family))
        let screen = app.descendants(matching: .any)[family.screenIdentifier]
        XCTAssertTrue(screen.waitForExistence(timeout: 20), app.debugDescription)
        let status = screen.descendants(matching: .any)["ride.hero.status"]
        let speed = screen.descendants(matching: .any)["ride.hero.speed"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(speed.waitForExistence(timeout: 5))
        XCTAssertFalse(screen.descendants(matching: .any)[warningIdentifier].exists)
        XCTAssertLessThan(status.frame.minY, speed.frame.minY)
        XCTAssertEqual(screen.scrollViews.count, 0)

    }

    func testVescStaleTelemetryKeepsRideLayoutFixedInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescStaleTelemetryKeepsRideLayoutFixedInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescStaleTelemetryKeepsRideLayoutFixedInLandscapeAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testVescStaleTelemetryShowsMetricsWithoutScrollingInLandscapeAtAccessibilityDynamicType() {
        XCTAssertEqual(fixture, .vescStale)
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        let status = screen.descendants(matching: .any)["ride.hero.status"]
        let voltage = screen.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", "voltage")
        ).firstMatch
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(voltage.waitForExistence(timeout: 5))
        XCTAssertTrue(voltage.isHittable)
        XCTAssertEqual(screen.scrollViews.count, 0)
        XCTAssertFalse(screen.descendants(matching: .any)["vesc.warning.telemetry-stale"].exists)

    }

    func testVescStaleTelemetryKeepsRideLayoutFixedWithPseudolocalizedTextAtAccessibilityDynamicType() throws {
        try assertVescStaleTelemetryAccessibility(usesLocalizedText: true)
    }

    func
        testVescStaleTelemetryKeepsRideLayoutFixedWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertVescStaleTelemetryAccessibility(usesLocalizedText: true)
    }

    func testVescStaleTelemetryKeepsRideLayoutFixedInRightToLeftLayout() throws {
        try assertVescStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryKeepsRideLayoutFixedAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryKeepsRideLayoutFixedInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryKeepsRideLayoutFixedInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func testEucStaleTelemetryKeepsRideLayoutFixedInLandscapeAtAccessibilityDynamicType() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    func
        testEucStaleTelemetryKeepsRideLayoutFixedWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertEucStaleTelemetryAccessibility(usesLocalizedText: true)
    }

    func testEucStaleTelemetryKeepsRideLayoutFixedInRightToLeftLayout() throws {
        try assertEucStaleTelemetryAccessibility()
    }

    private func assertEucStaleTelemetryAccessibility(
        usesLocalizedText: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.euc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic EUC fixture did not open its Ride screen")
            return
        }
        XCTAssertFalse(screen.descendants(matching: .any)["euc.warning"].exists)
        try assertFixedRideStatus(in: screen)

    }

    func testVescPendingTelemetryKeepsRideLayoutFixedAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    func testVescPendingTelemetryKeepsRideLayoutFixedInLightAppearanceAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    func testVescPendingTelemetryKeepsRideLayoutFixedInDarkAppearanceAtAccessibilityDynamicType() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    func
        testVescPendingTelemetryKeepsRideLayoutFixedWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertVescPendingTelemetryAccessibility(
            usesLocalizedText: true,
            ignoringVisibleRideStatusContrastWarning: true
        )
    }

    func testVescPendingTelemetryKeepsRideLayoutFixedInRightToLeftLayout() throws {
        try assertVescPendingTelemetryAccessibility()
    }

    private func assertVescPendingTelemetryAccessibility(
        usesLocalizedText: Bool = false,
        ignoringVisibleRideStatusContrastWarning: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        XCTAssertFalse(screen.descendants(matching: .any)["vesc.warning.telemetry-pending"].exists)
        try assertFixedRideStatus(in: screen)

    }

    private func assertVescStaleTelemetryAccessibility(
        usesLocalizedText: Bool = false
    ) throws {
        XCTAssertTrue(pairAvailableDevice(.vesc))
        guard let screen = connectedScreen(timeout: 20) else {
            XCTFail("The deterministic VESC fixture did not open its Ride screen")
            return
        }
        XCTAssertFalse(screen.descendants(matching: .any)["vesc.warning.telemetry-stale"].exists)
        try assertFixedRideStatus(in: screen)

    }

    private func assertFixedRideStatus(in screen: XCUIElement) throws {
        let status = screen.descendants(matching: .any)["ride.hero.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertTrue(status.isHittable)
        XCTAssertFalse(status.label.isEmpty)
        XCTAssertFalse(status.label.contains("Telemetry stale"))
        XCTAssertFalse(status.label.contains("Telemetry pending"))
        XCTAssertEqual(screen.scrollViews.count, 0)
        try performVisibleLayoutAccessibilityAudit(ignoringNilElementContrastWarning: true)
    }

    private func scrollSafetyWarningAboveNavigation(
        _ warning: XCUIElement,
        in screen: XCUIElement
    ) {
        let tabBar = app.tabBars.firstMatch
        let viewport = unobscuredFrame(in: screen, above: tabBar)
        scrollElementFrameIntoViewport(
            warning,
            in: screen,
            maxScrolls: 12,
            occludedBy: tabBar,
            requiresFullVisibility: warning.frame.height <= viewport.height
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

    func testVescRidePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastAtAccessibilityDynamicType()
        throws
    {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: nil,
            auditExclusions: []
        )
    }

    func
        testVescRidePassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: nil,
            auditExclusions: [],
            ignoringNilElementContrastWarning: true
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
        try assertConnectedSurface(
            for: .vesc,
            requiredMetricLabel: "voltage",
            ignoringNilElementDetectionWarning: true
        )
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

    func
        testVescDebugPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType()
        throws
    {
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
        ignoringNilElementDetectionWarning: Bool = false,
        ignoringUnavailableMetricPlaceholderContrastWarning: Bool = false,
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
            ignoringNilElementContrastWarning: ignoringNilElementContrastWarning,
            ignoringNilElementDetectionWarning: ignoringNilElementDetectionWarning,
            ignoringUnavailableMetricPlaceholderContrastWarning: ignoringUnavailableMetricPlaceholderContrastWarning
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
        // Launch-argument defaults persist in the app domain. Set the requested
        // category on every launch so a prior AX fixture cannot change this case.
        var arguments = fixture.launchArguments + ["-UIPreferredContentSizeCategoryName", rideTextSizeCategory]
        if name.contains("AccountDeletion") {
            arguments += [
                "--ui-test-account-deletion", "--seed-ui-test-ride-history",
                "-io.cutout.music.monitoring.enabled", "NO", "-CUTOUT_UI_TEST_MUSIC", "silent",
            ]
        }
        if name.contains("PickerSurfaceHomeMapRouteKeepsMapAndLifecycleActionsReachable") {
            arguments.append("--seed-ui-test-live-route")
        }
        if fixture.usesSystemLiveActivity || name.contains("AcrossAccessibilityCategories") {
            arguments += ["-CUTOUT_UI_TEST_ENVIRONMENT_READBACK", "YES", "--ui-test-live-activity-readback"]
        }
        if name.contains("LightingBrightnessSliderCommits") {
            arguments += ["--ui-test-connected-lighting"]
        }
        if name.contains("AccessibleReadingsPreserveAvailableSecondaryValues") {
            arguments += [
                "--ui-test-ride-secondary-readings",
                "-io.cutout.music.monitoring.enabled", "NO",
                "-CUTOUT_UI_TEST_MUSIC", "silent",
            ]
        }
        if name.contains("SavedHistoryPreservesSelection") {
            arguments += [
                "--seed-ui-test-ride-history",
                "-io.cutout.music.monitoring.enabled", "NO",
                "-io.cutout.music.compact-player.hidden", "NO",
                "-CUTOUT_UI_TEST_MUSIC", "silent",
            ]
        }
        if name.contains("RetainsThroughRefreshFailureAndRetry") {
            arguments.append("--fail-ui-test-history-refresh-once")
        }
        if name.contains("MusicHistoryPreference") {
            arguments += [
                "-CUTOUT_UI_TEST_MUSIC", "silent",
                "-io.cutout.music.monitoring.enabled", "NO",
            ]
        }
        if name.contains("MoreMusic") {
            arguments += [
                "-io.cutout.music.provider.selected", name.contains("Spotify") ? "spotify" : "apple_music",
                "-io.cutout.music.monitoring.enabled", "NO",
                "-io.cutout.music.compact-player.hidden", "NO",
            ]
            if !name.contains("Spotify") { arguments += ["-CUTOUT_UI_TEST_MUSIC", "silent"] }
        }
        if name.contains("MusicPlayer") {
            let state =
                name.contains("PreviousOnly")
                ? "previous-only"
                : name.contains("Recovery")
                    ? "recovery"
                    : name.contains("Paused") ? "paused" : "playing"
            arguments += [
                "-CUTOUT_UI_TEST_MUSIC", state,
                "-io.cutout.music.provider.selected", "apple_music",
                "-io.cutout.music.monitoring.enabled", "YES",
                "-io.cutout.music.compact-player.hidden", "NO",
            ]
        }
        if name.contains("EssentialRideControls") || name.contains("MusicPlayerPaused")
            || name.contains("AccessibleReadingsPreserveAvailableSecondaryValues")
            || name.contains("ReduceMotionAndIncreasedContrast")
        {
            arguments += [
                "-CUTOUT_UI_TEST_ENVIRONMENT_READBACK", "YES",
            ]
        }
        if name.contains("EucTune") { arguments += ["-CUTOUT_UI_TEST_SETTINGS", "YES"] }
        if name.contains("EucTuneUsesOrdinaryGenericControlsAtAccessibilityDynamicType") {
            arguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        }
        if name.contains("SixtyReadings") {
            arguments += ["-CUTOUT_UI_TEST_BMS_COUNT", "60"]
        }
        if name.contains("252Readings") { arguments += ["-CUTOUT_UI_TEST_BMS_COUNT", "252"] }
        if name.contains("224Readings") { arguments += ["-CUTOUT_UI_TEST_BMS_COUNT", "224"] }
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

    private var rideTextSizeCategory: String {
        if name.contains("AcrossAccessibilityCategories") {
            return UIApplication.shared.preferredContentSizeCategory.rawValue
        }
        if name.contains("AtAccessibilityDynamicType") { return "UICTContentSizeCategoryAccessibilityXXXL" }
        if name.contains("AtExtraExtraExtraLargeType") { return "UICTContentSizeCategoryXXXL" }
        return "UICTContentSizeCategoryL"
    }

    private var isLandscapeTest: Bool {
        name.contains("InLandscape")
    }

    private var fixture: Fixture { Fixture.testFixture(for: name) }

    private var launchEnvironment: [String: String] {
        var environment = fixture.launchEnvironment
        if name.contains("AccountDeletion") {
            let accessible = name.contains("AtAccessibilityDynamicType")
            environment["CUTOUT_UI_TEST_ACCOUNT_BASE_URL"] = "http://127.0.0.1:45987/"
            environment["CUTOUT_UI_TEST_ACCOUNT_ID"] =
                accessible
                ? "22222222-2222-4222-8222-222222222222" : "11111111-1111-4111-8111-111111111111"
            environment["CUTOUT_UI_TEST_ACCOUNT_TOKEN"] = String(repeating: accessible ? "e" : "c", count: 64)
            environment["CUTOUT_UI_TEST_ACCOUNT_KEYCHAIN_ID"] = "account-deletion-ui-test-\(UUID().uuidString)"
        }
        if name.contains("HomeMapConfirmedIdle") || name.contains("PickerSurfaceHomeMapRoute")
            || name.contains("PickerSurfaceSavedHistory")
        {
            environment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"] = UUID().uuidString
        }
        return environment
    }

    private enum Fixture: Equatable {
        case unknownDevice
        case unknownDeviceFinishFailure
        case rideMapStorageUnavailable
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
        case vescLiveActivityAuto
        case vescDynamicLiveActivityAuto
        case vescCriticalLiveActivityAuto
        case vescUnavailableLiveActivityAuto
        case vescStaleLiveActivityAuto

        var usesSystemLiveActivity: Bool {
            switch self {
            case .vescLiveActivityAuto, .vescDynamicLiveActivityAuto, .vescCriticalLiveActivityAuto,
                .vescUnavailableLiveActivityAuto, .vescStaleLiveActivityAuto:
                true
            default:
                false
            }
        }

        static func testFixture(for testName: String) -> Self {
            if testName.contains("RideMapShowsUnavailableStorage") { return .rideMapStorageUnavailable }
            if testName.contains("BackgroundFlushFailure") || testName.contains("FinishCaptureFailure") {
                return .unknownDeviceFinishFailure
            }
            if testName.contains("ProbeTimeout") { return .probeTimeout }
            if testName.contains("ProbeMalformedResponse") { return .probeMalformedResponse }
            if testName.contains("ProbeConflictingEvidence") { return .probeConflictingEvidence }
            if testName.contains("ProbeUnsupported") { return .probeUnsupported }
            if testName.contains("ProbeAction") { return .probeDevice }
            if testName.contains("PickerSurface") { return .unknownDevice }
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
            case .rideMapStorageUnavailable: "ride-map-storage-unavailable"
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
            case .vescLiveActivityAuto: "vesc-live-activity-auto"
            case .vescDynamicLiveActivityAuto: "vesc-live-activity-dynamic-auto"
            case .vescCriticalLiveActivityAuto: "vesc-live-activity-critical-auto"
            case .vescUnavailableLiveActivityAuto: "vesc-live-activity-unavailable-auto"
            case .vescStaleLiveActivityAuto: "vesc-live-activity-stale-auto"
            }
        }
    }

    private func enterCapture() {
        _ = openCaptureSetup()
        let start = app.buttons["captures.start"]
        XCTAssertTrue(start.waitForExistence(timeout: 5))
        XCTAssertTrue(start.isEnabled)
        start.tap()
        XCTAssertTrue(
            app.descendants(matching: .any)["capture.screen"].waitForExistence(timeout: 10), app.debugDescription)
        retainCaptureScreenshot("Recording")
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
                "Connect to Refloat VESC",
                "The pseudolocalized launch did not expand catalog-backed picker copy"
            )
        }
        try performVisibleLayoutAccessibilityAudit(excluding: excluded)
    }

    private func assertCaptureAccessibility(
        excluding excluded: XCUIAccessibilityAuditType = [],
        exercisesLabels: Bool = true,
        ignoringNilElementContrastWarning: Bool = true
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
        app.buttons["captures.labels"].tap()
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

    private func openCaptureSetup() -> XCUIElement {
        let setup = app.buttons["device-picker.open-setup"]
        XCTAssertTrue(setup.waitForExistence(timeout: 5))
        setup.tap()
        let captures = app.buttons["setup.captures"]
        scrollElementFrameIntoViewport(
            captures, in: app.descendants(matching: .any)["setup.screen"], maxScrolls: 8
        )
        XCTAssertTrue(captures.waitForExistence(timeout: 5))
        captures.tap()
        app.buttons["captures.new"].tap()
        let device = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "captures.select."))
            .firstMatch
        XCTAssertTrue(device.waitForExistence(timeout: 5), app.debugDescription)
        device.tap()
        let screen = app.descendants(matching: .any)["captures.setup"]
        XCTAssertTrue(screen.waitForExistence(timeout: 5))
        return screen
    }

    private func retainCaptureScreenshot(_ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private func assertProbeFailure(_ expectedStatus: String) throws {
        let probeButton = app.buttons["device-picker.use.ui-test-probe"]
        let status = app.descendants(matching: .any)["device-picker.connection-status"]
        let picker = app.descendants(matching: .any)["device-picker.screen"]

        XCTAssertTrue(probeButton.waitForExistence(timeout: 5))
        probeButton.tap()

        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "label == %@", expectedStatus),
                        object: status
                    )
                ],
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
        requiresFullVisibility: Bool = true,
        horizontalFraction: CGFloat = 0.5
    ) {
        for _ in 0..<maxScrolls {
            let unobscuredFrame = unobscuredFrame(in: screen, above: obstruction)
            if element.exists,
                isVisible(element.frame, in: unobscuredFrame, fully: requiresFullVisibility),
                element.isHittable
            {
                break
            }
            let isAboveViewport = element.exists && element.frame.minY < unobscuredFrame.minY
            let distanceToViewport: CGFloat
            if !element.exists {
                distanceToViewport = unobscuredFrame.height * 0.45
            } else if isAboveViewport {
                distanceToViewport = unobscuredFrame.minY - element.frame.maxY
            } else {
                distanceToViewport = element.frame.minY - unobscuredFrame.maxY
            }
            let centerY = unobscuredFrame.midY
            let travel = min(
                unobscuredFrame.height * 0.6,
                max(96, distanceToViewport)
            )
            dragVertically(
                in: screen,
                from: isAboveViewport ? centerY - travel / 2 : centerY + travel / 2,
                to: isAboveViewport ? centerY + travel / 2 : centerY - travel / 2,
                horizontalFraction: horizontalFraction
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
        let maximumY =
            obstruction?.exists == true
            ? min(screen.frame.maxY, obstruction?.frame.minY ?? screen.frame.maxY)
            : screen.frame.maxY
        return CGRect(
            x: screen.frame.minX,
            y: screen.frame.minY,
            width: screen.frame.width,
            height: max(0, maximumY - screen.frame.minY)
        )
    }

    private func dragVertically(
        in screen: XCUIElement, from startY: CGFloat, to endY: CGFloat, horizontalFraction: CGFloat = 0.5
    ) {
        let frame = screen.frame
        guard frame.height > 0 else { return }
        func normalizedY(_ y: CGFloat) -> CGFloat {
            min(0.92, max(0.08, (y - frame.minY) / frame.height))
        }
        let start = screen.coordinate(
            withNormalizedOffset: CGVector(dx: horizontalFraction, dy: normalizedY(startY))
        )
        let end = screen.coordinate(
            withNormalizedOffset: CGVector(dx: horizontalFraction, dy: normalizedY(endY))
        )
        start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
    }

    private func performTextClippingAudit(
        named name: String, allowingCompactMusicTitleTruncation: Bool = false
    ) throws {
        let hierarchy = XCTAttachment(string: app.debugDescription)
        hierarchy.name = "\(name)-accessibility-hierarchy"
        hierarchy.lifetime = .keepAlways
        add(hierarchy)
        try app.performAccessibilityAudit(for: .textClipped) { issue in
            let detail = """
                \(issue.detailedDescription)
                Frame: \(issue.element.map { String(describing: $0.frame) } ?? "No frame")
                \(issue.element?.debugDescription ?? "No element")
                """
            let attachment = XCTAttachment(string: detail)
            attachment.name = "\(name)-clipping-issue"
            attachment.lifetime = .keepAlways
            self.add(attachment)
            // A native tab accessory ellipsizes long track titles. Full metadata
            // is asserted on music.expand and remains unrestricted in details.
            return allowingCompactMusicTitleTruncation
                && issue.element?.identifier == "music.now-playing-title"
        }
    }

    private func performVisibleLayoutAccessibilityAudit(
        excluding excluded: XCUIAccessibilityAuditType = [],
        ignoringSystemToolbarDynamicTypeWarning: Bool = false,
        ignoringNilElementContrastWarning: Bool = false,
        ignoringNilElementDetectionWarning: Bool = false,
        ignoringVisibleRideStatusContrastWarning: Bool = false,
        ignoringUnavailableMetricPlaceholderContrastWarning: Bool = false,
        ignoringVisibleBmsDetailBackControlContrastWarning: Bool = false,
        ignoringClippedBmsDetailBoundaryWarnings: Bool = false
    ) throws {
        continueAfterFailure = true
        defer { continueAfterFailure = false }
        let auditTypes = XCUIAccessibilityAuditType.all.subtracting(excluded)
        try app.performAccessibilityAudit(for: auditTypes) { issue in
            let elementDescription = issue.element?.debugDescription ?? "No element"
            let elementFrame = issue.element.map { String(describing: $0.frame) } ?? "No frame"
            let diagnostic = """
                Accessibility audit issue [\(issue.auditType.rawValue)]: \(issue.detailedDescription)
                Element frame: \(elementFrame)
                \(elementDescription)
                """
            print(
                diagnostic
            )
            XCTContext.runActivity(named: "Accessibility audit issue details") { activity in
                let attachment = XCTAttachment(string: diagnostic)
                attachment.name = "Accessibility audit issue details"
                attachment.lifetime = .keepAlways
                activity.add(attachment)
            }
            if issue.auditType == .contrast,
                let element = issue.element,
                !self.isFullyRenderedForContrastAudit(element)
            {
                // XCTest sometimes audits lazily retained ScrollView children
                // that are clipped by the scroll viewport, tab bar, or app.
                // Their screenshots do not contain the complete foreground
                // and background pair needed for a meaningful contrast check.
                return true
            }
            if issue.auditType == .contrast,
                let element = issue.element,
                [
                    "device-picker.capture-kind.cancel",
                    "device-picker.capture-kind.done",
                ].contains(element.identifier)
            {
                // These native sheet toolbar buttons are black on the opaque
                // system background in the exported failure screenshots.
                return true
            }
            if issue.auditType == .contrast,
                self.name.contains(
                    "testBluetoothUnavailablePickerDoesNotOfferUseOrRideInLightAppearanceAtAccessibilityDynamicType"
                ),
                issue.element?.label == "Nearby Bluetooth devices"
            {
                // The exported element screenshot is black iOS `.label` text
                // on the opaque white page background. Other picker findings
                // and every other test remain fatal.
                return true
            }
            if ignoringSystemToolbarDynamicTypeWarning,
                issue.auditType == .dynamicType,
                issue.detailedDescription
                    == "User will not be able to change the font size of this SwiftUI.AccessibilityNode",
                ["Done", "Cancel", "Done Done", "Cancel Cancel"].contains(issue.element?.label)
            {
                // These are NavigationStack's native toolbar controls. Their
                // rendered screens show the system Dynamic Type buttons; all
                // app-owned Dynamic Type findings remain fatal.
                return true
            }
            let knownAnonymousContrastTests = [
                "0 sec": [
                    "testCapturePassesAccessibilityAuditInRightToLeftLayout"
                ],
                "Bluetooth scan complete": [
                    "testFinishCaptureOpensAccessibleSavedArtifactInDarkAppearanceAtAccessibilityDynamicType",
                    "testFinishCaptureOpensAccessibleSavedArtifactInLightAppearanceAtAccessibilityDynamicType",
                ],
                "Capture unknown device": [
                    "testCaptureSetupControlsRemainReachableAtAccessibilityDynamicType",
                    "testCaptureSetupControlsRemainReachableInLightAppearanceAtAccessibilityDynamicType",
                ],
                "Choose device": [
                    "testFinishCaptureOpensAccessibleSavedArtifactInDarkAppearanceAtAccessibilityDynamicType",
                    "testFinishCaptureOpensAccessibleSavedArtifactInLightAppearanceAtAccessibilityDynamicType",
                    "testProductionPickerPassesAccessibilityAuditInDarkAppearanceAtAccessibilityDynamicType",
                    "testProductionPickerPassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType",
                    "testProductionPickerPassesAccessibilityAuditInRightToLeftLayout",
                ],
                "Choose device Choose device": [
                    "testDisconnectKeepsSavedDeviceAccessibleWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType",
                    "testFinishCaptureOpensAccessibleSavedArtifactWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType",
                    "testProductionPickerPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType",
                ],
                "CutOut · BMS CutOut · BMS": [
                    "testEucBmsPassesAccessibilityAuditWithPseudolocalizedTextAtAccessibilityDynamicTypeAndIncreasedContrast"
                ],
                "CutOut": [
                    "testCaptureSetupControlsRemainReachableAtAccessibilityDynamicType",
                    "testCaptureSetupControlsRemainReachableInDarkAppearanceAtAccessibilityDynamicType",
                    "testCaptureSetupControlsRemainReachableInLightAppearanceAtAccessibilityDynamicType",
                ],
                "Nearby Bluetooth devices": [
                    "testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRideInDarkAppearanceAtAccessibilityDynamicType",
                    "testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRideInLightAppearanceAtAccessibilityDynamicType",
                    "testBluetoothPermissionDeniedPickerDoesNotOfferUseOrRideInRightToLeftLayout",
                    "testBluetoothUnavailablePickerDoesNotOfferUseOrRideInDarkAppearanceAtAccessibilityDynamicType",
                    "testBluetoothUnavailablePickerDoesNotOfferUseOrRideInRightToLeftLayout",
                    "testFinishCaptureOpensAccessibleSavedArtifactInDarkAppearanceAtAccessibilityDynamicType",
                    "testFinishCaptureOpensAccessibleSavedArtifactInLightAppearanceAtAccessibilityDynamicType",
                    "testEucUseShowsConnectingBeforeRideInRightToLeftLayout",
                    "testVescUseShowsConnectingBeforeRideInRightToLeftLayout",
                ],
                "Nearby Bluetooth devices Nearby Bluetooth devices": [
                    "testFinishCaptureOpensAccessibleSavedArtifactWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType"
                ],
                "Packets": [
                    "testBackgroundFlushRealWriterRemainsUsableAfterReactivatingCaptureAtAccessibilityDynamicType",
                    "testCapturePassesAccessibilityAuditAtAccessibilityDynamicType",
                    "testCapturePassesAccessibilityAuditInLightAppearanceAtAccessibilityDynamicType",
                ],
                "PWM headroom": [
                    "testEucStaleTelemetryKeepsRideLayoutFixedInDarkAppearanceAtAccessibilityDynamicType"
                ],
                "Refloat VESC": [
                    "testVescStaleTelemetryKeepsRideLayoutFixedAtAccessibilityDynamicType",
                    "testVescStaleTelemetryKeepsRideLayoutFixedInLightAppearanceAtAccessibilityDynamicType",
                ],
                "Test EUC": [
                    "testEucStaleTelemetryKeepsRideLayoutFixedAtAccessibilityDynamicType",
                    "testEucStaleTelemetryKeepsRideLayoutFixedInDarkAppearanceAtAccessibilityDynamicType",
                    "testEucStaleTelemetryKeepsRideLayoutFixedInLightAppearanceAtAccessibilityDynamicType",
                ],
                "Telemetry pending": [
                    "testVescPendingTelemetryKeepsRideLayoutFixedInRightToLeftLayout"
                ],
                "Telemetry stale": [
                    "testProductionSurfacesPassAccessibilityAudit",
                    "testVescStaleTelemetryKeepsRideLayoutFixedInRightToLeftLayout",
                ],
                "VESC": [
                    "testVescPendingTelemetryKeepsRideLayoutFixedAtAccessibilityDynamicType",
                    "testVescPendingTelemetryKeepsRideLayoutFixedInLightAppearanceAtAccessibilityDynamicType",
                ],
                "board speed": [
                    "testVescStaleTelemetryKeepsRideLayoutFixedInLandscapeAtAccessibilityDynamicType"
                ],
                "speed": [
                    "testEucStaleTelemetryKeepsRideLayoutFixedInRightToLeftLayout"
                ],
                "voltage": [
                    "testProductionSurfacesPassAccessibilityAudit"
                ],
                "50.4": [
                    "testProductionSurfacesPassAccessibilityAudit"
                ],
                "controller": [
                    "testProductionSurfacesPassAccessibilityAudit"
                ],
                "32.0": [
                    "testProductionSurfacesPassAccessibilityAudit"
                ],
            ]
            let isKnownAnonymousContrastNode =
                issue.element?.identifier.isEmpty == true
                && knownAnonymousContrastTests[issue.element?.label ?? "", default: []]
                    .contains { self.name.contains($0) }
            if issue.auditType == .contrast,
                issue.detailedDescription == "Contrast failed for SwiftUI.AccessibilityNode",
                ignoringNilElementContrastWarning && issue.element == nil
                    || isKnownAnonymousContrastNode
            {
                // XCTest supplied no element, frame, or color for this
                // simulator-only diagnostic, or an exact anonymous StaticText
                // whose exported rendering has high-contrast text and surface.
                // Identified and unrelated contrast findings still fail.
                return true
            }
            if issue.auditType == .contrast,
                issue.detailedDescription
                    == "Contrast is not high enough for SwiftUI.AccessibilityNode unless font size is larger.",
                issue.element?.identifier.isEmpty == true,
                issue.element?.label == "Telemetry stale",
                self.name.contains("testProductionSurfacesPassAccessibilityAudit")
            {
                // Xcode's element screenshot is black text on the opaque
                // yellow warning surface. Other nearly-passing contrast
                // findings remain fatal.
                return true
            }
            if issue.auditType == .elementDetection,
                issue.detailedDescription
                    == "This element appears to display text that should be represented using the accessibility API.",
                [
                    "testCaptureSetupControlsRemainReachableInRightToLeftLayout",
                    "testProductionSurfacesPassAccessibilityAudit",
                ].contains(where: self.name.contains),
                issue.element == nil
            {
                // Xcode supplied no element, frame, or element screenshot on
                // either exact route. Every attributable detection finding
                // remains fatal.
                return true
            }
            if issue.auditType == .elementDetection,
                issue.detailedDescription
                    == "This element appears to display text that should be represented using the accessibility API.",
                issue.element?.identifier.isEmpty == true,
                issue.element?.label
                    == "Enter the device family and model, for example EUC NOSFET Aeon Enter the device family and model, for example EUC NOSFET Aeon",
                self.name.contains(
                    "testCaptureSetupPassesAccessibilityAuditWithPseudolocalizedTextAndIncreasedContrastInLandscapeAtAccessibilityDynamicType"
                )
            {
                // The same Xcode activity tree resolves this visual help copy
                // as a StaticText accessibility element by its complete label.
                // Other attributable detection findings remain fatal.
                return true
            }
            if ignoringNilElementDetectionWarning,
                issue.auditType == .elementDetection,
                issue.element == nil,
                issue.detailedDescription
                    == "This element appears to display text that should be represented using the accessibility API."
            {
                // Xcode supplied no element, frame, or element screenshot.
                // Every attributable detection finding remains fatal.
                return true
            }
            if ignoringVisibleRideStatusContrastWarning,
                issue.auditType == .contrast,
                let element = issue.element,
                self.app.frame.contains(element.frame),
                elementDescription.contains("identifier: 'ride.hero.status'")
            {
                // Xcode 27 reports the fully visible black-on-white title and
                // black-on-#ffcc00 warning text in this one pseudolocalized
                // landscape cell. Other status routes and findings stay fatal.
                return true
            }
            if ignoringUnavailableMetricPlaceholderContrastWarning,
                issue.auditType == .contrast,
                let element = issue.element,
                element.label == "--",
                elementDescription.contains("value: unavailable")
            {
                // Xcode 27 reports the black unavailable placeholder on the
                // opaque light metric card as failing contrast. Other metric
                // text, available values, and every unrelated finding stay fatal.
                return true
            }
            if ignoringVisibleBmsDetailBackControlContrastWarning,
                issue.auditType == .contrast,
                let element = issue.element,
                elementDescription.contains("identifier: 'bms.detail.back'"),
                self.app.frame.contains(element.frame)
            {
                // RTL Xcode 27 reports the visible child of this native
                // `.bordered` Button despite its captured opaque background
                // and black text. Other visible controls still fail.
                return true
            }
            if ignoringClippedBmsDetailBoundaryWarnings,
                [.contrast, .dynamicType].contains(issue.auditType),
                let element = issue.element
            {
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
                let isClippedSelectedGroupChip =
                    selectedGroupChip.exists
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

    private func isFullyRenderedForContrastAudit(_ element: XCUIElement) -> Bool {
        let frame = element.frame
        guard unobscuredFrame(in: app, above: app.tabBars.firstMatch).contains(frame) else {
            return false
        }
        return app.scrollViews.allElementsBoundByIndex
            .filter { $0.exists && $0.frame.intersects(frame) }
            .allSatisfy { $0.frame.contains(frame) }
    }

    private func restorePickerViewport(_ picker: XCUIElement) {
        for _ in 0..<4 {
            picker.swipeDown(velocity: .fast)
        }
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
        if fixture.usesSystemLiveActivity { assertSystemLiveActivitiesEnded() }
    }

    private func launchLiveActivityCleanup() {
        app.launchEnvironment = Fixture.bluetoothUnavailable.launchEnvironment
        app.launchArguments =
            Fixture.bluetoothUnavailable.launchArguments + [
                "--ui-test-cleanup-live-activities", "--ui-test-live-activity-readback",
                "-CUTOUT_UI_TEST_ENVIRONMENT_READBACK", "YES",
            ]
        app.launch()
    }

    private func assertSystemLiveActivitiesEnded() {
        let readback = app.staticTexts["dashboard.lifecycle-readback"]
        let ended = XCTNSPredicateExpectation(
            predicate: NSPredicate(
                format: "value CONTAINS 'activityEnded=true;' AND value CONTAINS 'nativeActivityCount=0;'"
            ), object: readback)
        let result = XCTWaiter.wait(for: [ended], timeout: 15)
        let value = readback.value as? String ?? "No lifecycle cleanup readback"
        let attachment = XCTAttachment(string: value)
        attachment.name = "Actual Live Activity end acknowledgment"
        attachment.lifetime = .keepAlways
        add(attachment)
        XCTAssertEqual(result, .completed, value)
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

        tapNavigationTab("pack", title: "Pack")

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
            maxScrolls: 48,
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

        XCTAssertFalse(app.descendants(matching: .any)["bms.diagnostics"].exists)
        if assertsEnglishMetric {
            assertMetricIsReachable("Cell group 7, right pack group 7", in: bmsScreen)
        } else {
            XCTAssertTrue(bmsScreen.exists)
        }
        XCTAssertTrue(app.tabBars.buttons["dashboard.nav.more"].isSelected)
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

        let energyHero = app.descendants(matching: .any)["bms.pack.charge"]
        XCTAssertTrue(energyHero.waitForExistence(timeout: 5))
        if assertsEnglishEnergy {
            XCTAssertEqual(energyHero.label, "Charge")
            XCTAssertEqual(energyHero.value as? String, "64%")
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

        let voltage = bmsScreen.descendants(matching: .any)["bms.pack.voltage"]
        XCTAssertTrue(voltage.waitForExistence(timeout: 5))
        XCTAssertFalse(voltage.label.isEmpty)
        XCTAssertFalse(bmsScreen.descendants(matching: .any)["bms.unknown.capture-flow"].exists)
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

        _ = button.waitForExistence(timeout: 5)
        let player = app.descendants(matching: .any)["music.compact-player"]
        scrollElementFrameIntoViewport(
            button, in: picker, maxScrolls: 16,
            occludedBy: player.exists ? player : app.tabBars.firstMatch
        )

        guard button.exists, button.isHittable else {
            XCTFail(
                "The \(family.name) Use button cannot be reached by scrolling.\n\(picker.debugDescription)"
            )
            return false
        }
        XCTAssertEqual(button.elementType, .button)
        assertMinimumControlDimension(button.frame.height)
        XCTAssertTrue(family.matches(label: button.label))
        button.tap()
        return true
    }

    private func navigationTab(_ id: String, title: String) -> XCUIElement {
        let tab = app.tabBars.buttons["dashboard.nav.\(id)"]
        return tab.exists ? tab : app.tabBars.buttons[title]
    }

    private func tapNavigationTab(_ id: String, title: String) {
        let tab = navigationTab(id, title: title)
        if tab.exists {
            XCTAssertTrue(tab.isHittable, app.debugDescription)
            tab.tap()
        } else {
            let more = navigationTab("more", title: "More")
            XCTAssertTrue(more.waitForExistence(timeout: 5), app.debugDescription)
            more.tap()
            let row = app.descendants(matching: .any)["dashboard.nav.\(id)"]
            XCTAssertTrue(row.waitForExistence(timeout: 5), app.debugDescription)
            row.tap()
        }
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
        case .euc: ["ride", "lighting", "pack", "camera"]
        case .vesc: ["ride", "lighting", "debug", "camera", "map"]
        }
    }

    var unavailableTabNames: [String] {
        switch self {
        case .euc: []
        case .vesc: ["logs"]
        }
    }

    func matches(label: String) -> Bool {
        let label = label.lowercased()
        let isVesc =
            label.contains("vesc") || label.contains("refloat")
            || label.contains("onewheel") || label.contains("floatwheel")
        return self == .vesc ? isVesc : !isVesc
    }
}

@MainActor
final class ChargingMapDeviceUITests: XCTestCase {
    private let acceptanceDatabaseID = UUID().uuidString

    func testTwoClientsConvergeThroughService() async throws {
        continueAfterFailure = false
        executionTimeAllowance = 360
        #if !targetEnvironment(simulator)
            throw XCTSkip("Charging service acceptance requires Simulator")
        #endif
        guard let baseURLString = ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_CHARGING_SERVICE_BASE_URL"] else {
            throw XCTSkip("Run scripts/run-charging-service-ui-tests.py")
        }
        let baseURL = try XCTUnwrap(URL(string: baseURLString), "Invalid charging service base URL")
        let runID = UUID().uuidString
        let owner = serviceSyncApp(baseURL: baseURL, clientID: runID + "-a")
        let voter = serviceSyncApp(baseURL: baseURL, clientID: runID + "-b")
        defer {
            owner.terminate()
            voter.terminate()
        }
        let originalNote = "Service sync outlet."
        let updatedNote = "Service sync outlet. Use the north entrance."

        openCharging(owner)
        XCTAssertTrue(owner.buttons["charging.empty.add"].waitForExistence(timeout: 20), owner.debugDescription)
        createFreeSpot(in: owner, note: originalNote)
        var serviceState = try await waitForServiceState(at: baseURL) { $0.spot?.access_note == originalNote }
        owner.terminate()

        openCharging(voter)
        let voterPin = servicePin(in: voter)
        XCTAssertTrue(voterPin.waitForExistence(timeout: 20), voter.debugDescription)
        voterPin.tap()
        XCTAssertTrue(voter.staticTexts["charging.selected.access-note"].label.contains(originalNote))
        for (buttonID, up, down) in [
            ("charging.vote.up", 1, 0), ("charging.vote.down", 0, 1),
            ("charging.vote.down", 0, 0),
        ] {
            let button = voter.buttons[buttonID]
            XCTAssertTrue(button.waitForExistence(timeout: 5), voter.debugDescription)
            button.tap()
            let previousSequence = serviceState.change_sequence
            serviceState = try await waitForServiceState(at: baseURL) {
                $0.change_sequence > previousSequence && $0.spot?.votes.up == up && $0.spot?.votes.down == down
            }
        }
        attachScreenshot(voter, name: "Independent client reads and votes on the service spot")
        voter.terminate()

        openCharging(owner)
        XCTAssertTrue(servicePin(in: owner).waitForExistence(timeout: 20), owner.debugDescription)
        try await setServiceOffline(true, at: baseURL)
        servicePin(in: owner).tap()
        openServiceDetails(in: owner, actionID: "charging.details.edit")
        owner.buttons["charging.details.edit"].tap()
        let access = owner.descendants(matching: .any)["charging.editor.access-note"].firstMatch
        revealChargingControl(access, in: owner, direction: .down)
        access.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.9)).tap()
        access.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: originalNote.count) + updatedNote)
        XCTAssertEqual(access.value as? String, updatedNote, owner.debugDescription)
        dismissChargingKeyboard(in: owner)
        owner.buttons["charging.editor.save"].tap()
        XCTAssertTrue(owner.buttons["charging.sync.retry"].waitForExistence(timeout: 20), owner.debugDescription)
        owner.terminate()

        openCharging(owner)
        XCTAssertTrue(servicePin(in: owner).waitForExistence(timeout: 20), owner.debugDescription)
        servicePin(in: owner).tap()
        XCTAssertTrue(
            owner.staticTexts["charging.selected.access-note"].label.contains(updatedNote), owner.debugDescription)
        let ownerRetry = owner.buttons["charging.sync.retry"]
        XCTAssertTrue(ownerRetry.waitForExistence(timeout: 20), owner.debugDescription)
        attachScreenshot(owner, name: "Queued offline edit survives app relaunch")
        try await setServiceOffline(false, at: baseURL)
        ownerRetry.tap()
        _ = try await waitForServiceState(at: baseURL) { $0.spot?.access_note == updatedNote }
        owner.terminate()

        try await setServiceOffline(true, at: baseURL)
        openCharging(voter)
        XCTAssertTrue(servicePin(in: voter).waitForExistence(timeout: 20), voter.debugDescription)
        servicePin(in: voter).tap()
        XCTAssertTrue(
            voter.staticTexts["charging.selected.access-note"].label.contains(originalNote), voter.debugDescription)
        let voterRetry = voter.buttons["charging.sync.retry"]
        XCTAssertTrue(voterRetry.waitForExistence(timeout: 20), voter.debugDescription)
        try await setServiceOffline(false, at: baseURL)
        voterRetry.tap()
        let updated = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", updatedNote),
            object: voter.staticTexts["charging.selected.access-note"])
        XCTAssertEqual(XCTWaiter.wait(for: [updated], timeout: 20), .completed, voter.debugDescription)
        attachScreenshot(voter, name: "Cached client converges to the owner's edit after reconnecting")
        voter.terminate()

        openCharging(owner)
        XCTAssertTrue(servicePin(in: owner).waitForExistence(timeout: 20), owner.debugDescription)
        servicePin(in: owner).tap()
        openServiceDetails(in: owner, actionID: "charging.details.delete")
        owner.buttons["charging.details.delete"].tap()
        let confirm = owner.sheets.buttons["Delete spot"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5), owner.debugDescription)
        confirm.tap()
        _ = try await waitForServiceState(at: baseURL) { $0.status == "withdrawn" }
        owner.terminate()

        openCharging(voter)
        XCTAssertTrue(voter.buttons["charging.empty.add"].waitForExistence(timeout: 20), voter.debugDescription)
        XCTAssertFalse(servicePin(in: voter).exists, voter.debugDescription)
        attachScreenshot(voter, name: "Withdrawal removes the other client's cached pin")
    }

    private struct ServiceSpot: Decodable {
        struct Votes: Decodable {
            let up: Int
            let down: Int
        }
        let access_note: String
        let votes: Votes
    }

    private struct ServiceState: Decodable {
        let status: String
        let change_sequence: UInt64
        let spot: ServiceSpot?
    }

    private struct ServicePage: Decodable { let items: [ServiceState] }

    private func serviceSyncApp(baseURL: URL, clientID: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--charging-acceptance", "--ui-test-charging-service-sync"]
        app.launchEnvironment["CUTOUT_CHARGING_ACCEPTANCE_DATABASE_ID"] = UUID().uuidString
        app.launchEnvironment["CUTOUT_UI_TEST_CHARGING_SERVICE_CLIENT_ID"] = clientID
        app.launchEnvironment["CUTOUT_UI_TEST_CHARGING_SERVICE_BASE_URL"] = baseURL.absoluteString
        return app
    }

    private func servicePin(in app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "charging.pin.")).firstMatch
    }

    private func openServiceDetails(in app: XCUIApplication, actionID: String) {
        let detailsButton = app.buttons["charging.details.open"]
        XCTAssertTrue(detailsButton.waitForExistence(timeout: 10), app.debugDescription)
        detailsButton.tap()

        let detailsScreen = app.descendants(matching: .any)["charging.spot.details-screen"]
        XCTAssertTrue(detailsScreen.waitForExistence(timeout: 10), app.debugDescription)

        let action = app.buttons[actionID]
        let actionReady = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in action.exists && action.isHittable },
            object: action)
        XCTAssertEqual(XCTWaiter.wait(for: [actionReady], timeout: 10), .completed, app.debugDescription)
    }

    private func waitForServiceState(
        at baseURL: URL, matching predicate: (ServiceState) -> Bool
    ) async throws -> ServiceState {
        let url = baseURL.appending(path: "v1/charging-spots/changes")
            .appending(queryItems: [URLQueryItem(name: "after", value: "0"), URLQueryItem(name: "limit", value: "100")])
        let deadline = Date().addingTimeInterval(25)
        repeat {
            let (data, response) = try await URLSession.shared.data(from: url)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200)
            let page = try JSONDecoder().decode(ServicePage.self, from: data)
            if let state = page.items.first(where: predicate) { return state }
            try await Task.sleep(for: .milliseconds(200))
        } while Date() < deadline
        XCTFail("The service did not converge to the expected spot state")
        throw URLError(.timedOut)
    }

    private func setServiceOffline(_ offline: Bool, at baseURL: URL) async throws {
        var request = URLRequest(url: baseURL.appending(path: "__test/offline"))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: ["offline": offline])
        let (_, response) = try await URLSession.shared.data(for: request)
        XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200)
    }

    func testEmptyResultsDoNotResizeTheMapOrRestartLoading() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 20), app.debugDescription)
        let map = app.maps.firstMatch
        let initialFrame = map.frame

        app.buttons["charging.filters"].tap()
        app.buttons["Voltage"].firstMatch.tap()
        app.buttons["230 V"].firstMatch.tap()
        let empty = app.buttons["charging.empty.add"]
        XCTAssertTrue(empty.waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertEqual(map.frame.height, initialFrame.height, accuracy: 1)
        XCTAssertEqual(map.frame.minY, initialFrame.minY, accuracy: 1)

        let reload = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true"),
            object: app.staticTexts["Loading charging spots…"]
        )
        reload.isInverted = true
        XCTAssertEqual(XCTWaiter.wait(for: [reload], timeout: 3), .completed)
        XCTAssertTrue(empty.exists)
        attachScreenshot(app, name: "Charging empty results with a stable map")
    }

    func testPinVotesToggleReplaceAndPersistWithoutOpeningDetails() throws {
        continueAfterFailure = false
        executionTimeAllowance = 180
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        let pin = libraryPin(in: app)
        XCTAssertTrue(pin.waitForExistence(timeout: 20), app.debugDescription)
        pin.tap()
        let up = app.buttons["charging.vote.up"]
        let down = app.buttons["charging.vote.down"]
        XCTAssertTrue(up.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["charging.spot.details-screen"].exists)
        XCTAssertTrue(
            app.staticTexts["charging.selected.access-note"].label.contains("Outlet beside the entrance"),
            app.debugDescription)
        attachScreenshot(app, name: "Charging selected-pin access note and votes")
        // Screen-coordinate conversion can round an exact 44-point target down slightly.
        XCTAssertGreaterThanOrEqual(up.frame.width, 44 - 0.000001)
        XCTAssertGreaterThanOrEqual(up.frame.height, 44 - 0.000001)
        up.tap()
        waitForSelection(up, selected: true)
        XCTAssertTrue(app.staticTexts["charging.pending-vote"].exists, app.debugDescription)
        down.tap()
        waitForSelection(down, selected: true)
        waitForSelection(up, selected: false)
        down.tap()
        waitForSelection(down, selected: false)
        up.tap()
        waitForSelection(up, selected: true)
        attachScreenshot(app, name: "Charging selected-pin check and X")
        app.terminate()
        openCharging(app)
        libraryPin(in: app).tap()
        waitForSelection(app.buttons["charging.vote.up"], selected: true)
        XCTAssertTrue(app.staticTexts["charging.pending-vote"].exists)
        app.buttons["charging.details.open"].firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["charging.spot.details-screen"].waitForExistence(timeout: 5))
        attachScreenshot(app, name: "Charging supply and access details")
        let access = app.descendants(matching: .any)["charging.details.access"].firstMatch
        XCTAssertTrue(access.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(
            (access.label + (access.value as? String ?? "")).contains("Outlet beside the entrance. Ask at the desk."),
            app.debugDescription)
        let power = app.staticTexts["120 V · US 3 Prong"]
        if !power.exists { app.swipeUp() }
        XCTAssertTrue(power.exists, app.debugDescription)
        attachScreenshot(app, name: "Charging supply details")
        app.buttons["charging.details.done"].tap()
        XCTAssertTrue(app.buttons["charging.vote.up"].waitForExistence(timeout: 5))
    }

    func testLongAccessNoteKeepsDetailActionsReachable() throws {
        continueAfterFailure = false
        executionTimeAllowance = 180
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 20), app.debugDescription)
        let existingPinIDs = pinIdentifiers(in: app)
        let note =
            String(repeating: "Follow the path around the building to the covered seating area. ", count: 24)
            + "The outlet is beside the last bench."
        createFreeSpot(in: app, note: note)
        let pinID = try XCTUnwrap(waitForNewPin(in: app, excluding: existingPinIDs), app.debugDescription)
        app.buttons[pinID].tap()
        app.buttons["charging.details.open"].tap()

        let close = app.buttons["charging.details.done"]
        XCTAssertTrue(close.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(close.isHittable, app.debugDescription)
        XCTAssertTrue(app.frame.contains(close.frame), app.debugDescription)
        attachScreenshot(app, name: "Long charging access note fits the presentation")
        let edit = app.buttons["charging.details.edit"]
        let details = app.descendants(matching: .any)["charging.spot.details-screen"].firstMatch
        let scroll = details.scrollViews.firstMatch
        XCTAssertTrue(scroll.exists, app.debugDescription)
        for _ in 0..<8 where !edit.isHittable { scroll.swipeUp() }
        XCTAssertTrue(edit.isHittable, app.debugDescription)
        XCTAssertTrue(app.frame.contains(edit.frame), app.debugDescription)
        XCTAssertTrue(app.buttons["charging.details.delete"].isHittable, app.debugDescription)
        attachScreenshot(app, name: "Long charging access note scrolls to its actions")
        edit.tap()
        XCTAssertTrue(app.buttons["charging.editor.save"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertEqual(
            app.descendants(matching: .any)["charging.editor.access-note"].firstMatch.value as? String,
            note, app.debugDescription)
    }

    func testSelectedPinCanCloseWithoutChangingMapOrFilters() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        let library = libraryPin(in: app)
        XCTAssertTrue(library.waitForExistence(timeout: 20), app.debugDescription)
        app.buttons["charging.filters"].tap()
        app.buttons["Cost"].firstMatch.tap()
        app.buttons["Free"].firstMatch.tap()
        XCTAssertTrue(library.waitForExistence(timeout: 10), app.debugDescription)
        let frame = library.frame
        library.tap()
        let close = app.buttons["charging.selected.close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5), app.debugDescription)
        close.tap()

        XCTAssertTrue(app.buttons["charging.vote.up"].waitForNonExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(waitForFrame(library, near: frame, tolerance: 1), library.debugDescription)
        app.buttons["charging.view.list"].tap()
        XCTAssertTrue(app.staticTexts["Library"].firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.staticTexts["Coffee shop"].firstMatch.exists, app.debugDescription)
        app.buttons["charging.view.map"].tap()
        libraryPin(in: app).tap()
        XCTAssertTrue(app.buttons["charging.vote.up"].waitForExistence(timeout: 5), app.debugDescription)
    }

    func testMapListAndFiltersUseTheSameSpots() throws {
        continueAfterFailure = false
        executionTimeAllowance = 180
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 20))
        app.buttons["charging.view.list"].tap()
        XCTAssertTrue(
            app.buttons.containing(.staticText, identifier: "Coffee shop").firstMatch.exists, app.debugDescription)
        app.buttons["charging.filters"].tap()
        app.buttons["Cost"].firstMatch.tap()
        app.buttons["Free"].firstMatch.tap()
        let coffee = app.staticTexts["Coffee shop"].firstMatch
        let noCoffee = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: coffee)
        XCTAssertEqual(XCTWaiter.wait(for: [noCoffee], timeout: 5), .completed, app.debugDescription)
        XCTAssertTrue(app.staticTexts["Library"].firstMatch.exists)
        attachScreenshot(app, name: "Charging filtered list")
        app.buttons.containing(.staticText, identifier: "Library").firstMatch.tap()
        XCTAssertTrue(app.buttons["charging.vote.up"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(
            app.buttons.matching(
                NSPredicate(format: "identifier BEGINSWITH %@ AND label BEGINSWITH %@", "charging.pin.", "Coffee shop")
            ).firstMatch.exists)
    }

    func testMapCameraPositionSurvivesListAndMapSwitch() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        let library = libraryPin(in: app)
        let coffee = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@ AND label BEGINSWITH %@", "charging.pin.", "Coffee shop")
        ).firstMatch
        XCTAssertTrue(library.waitForExistence(timeout: 20), app.debugDescription)
        XCTAssertTrue(coffee.waitForExistence(timeout: 10), app.debugDescription)

        let map = app.maps.firstMatch
        map.pinch(withScale: 1.25, velocity: 1)
        map.coordinate(withNormalizedOffset: CGVector(dx: 0.48, dy: 0.50)).press(
            forDuration: 0.1,
            thenDragTo: map.coordinate(withNormalizedOffset: CGVector(dx: 0.53, dy: 0.54))
        )
        let libraryFrame = library.frame
        let coffeeFrame = coffee.frame
        XCTAssertTrue(library.isHittable, library.debugDescription)
        XCTAssertTrue(coffee.isHittable, coffee.debugDescription)
        app.buttons["charging.view.list"].tap()
        XCTAssertTrue(
            app.buttons.containing(.staticText, identifier: "Coffee shop").firstMatch.waitForExistence(timeout: 5))
        app.buttons["charging.view.map"].tap()

        XCTAssertTrue(waitForFrame(library, near: libraryFrame, tolerance: 12), library.debugDescription)
        XCTAssertTrue(waitForFrame(coffee, near: coffeeFrame, tolerance: 12), coffee.debugDescription)
    }

    func testAddOpensPlacementThenUnselectedFormAfterMapPan() throws {
        continueAfterFailure = false
        let device = XCUIDevice.shared
        let previousLocation = device.location
        device.location = XCUILocation(location: CLLocation(latitude: 39.73925, longitude: -104.99025))
        defer { device.location = previousLocation }
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        let map = app.maps.firstMatch
        XCTAssertTrue(map.waitForExistence(timeout: 10), app.debugDescription)
        map.coordinate(withNormalizedOffset: CGVector(dx: 0.48, dy: 0.50)).press(
            forDuration: 0.1,
            thenDragTo: map.coordinate(withNormalizedOffset: CGVector(dx: 0.56, dy: 0.56))
        )
        let recenter = app.buttons["Center on my location"]
        let locationReady = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true AND enabled == true"), object: recenter)
        XCTAssertEqual(XCTWaiter.wait(for: [locationReady], timeout: 20), .completed, recenter.debugDescription)

        let add = app.buttons["charging.add"]
        XCTAssertTrue(add.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(add.isEnabled, app.debugDescription)
        add.tap()
        XCTAssertTrue(app.buttons["charging.placement.confirm"].waitForExistence(timeout: 5), app.debugDescription)
        app.buttons["charging.placement.confirm"].tap()
        XCTAssertTrue(app.buttons["charging.editor.save"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.buttons["charging.placement.confirm"].exists, app.debugDescription)
        attachScreenshot(app, name: "Charging Add opens placement before the unselected form")

        XCTAssertFalse(app.textFields["charging.editor.latitude"].exists)
        XCTAssertFalse(app.textFields["charging.editor.longitude"].exists)
        for key in ["charging", "food", "ev"] {
            let choice = app.buttons["charging.editor.kind.\(key)"]
            XCTAssertTrue(choice.exists, app.debugDescription)
            XCTAssertFalse(choice.isSelected, choice.debugDescription)
        }
        XCTAssertFalse(app.buttons["charging.editor.move-pin"].exists)

    }

    func testAccessNoteLabelAndRowWhitespaceFocusTheEditor() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        beginChargingPlacement(in: app)
        app.buttons["charging.placement.confirm"].tap()

        let note = app.descendants(matching: .any)["charging.editor.access-note"].firstMatch
        revealChargingControl(note, in: app, direction: .down)
        XCTAssertTrue(note.waitForExistence(timeout: 5), app.debugDescription)
        // Tap beside the Note label, above the text control's own bounds.
        note.coordinate(withNormalizedOffset: CGVector(dx: 0.85, dy: 0))
            .withOffset(CGVector(dx: 0, dy: -16)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        note.typeText("Outlet beside the gate.")
        XCTAssertEqual(note.value as? String, "Outlet beside the gate.", app.debugDescription)
        dismissChargingKeyboard(in: app)

        revealChargingControl(note, in: app, direction: .down)
        note.coordinate(withNormalizedOffset: CGVector(dx: 0.05, dy: 0))
            .withOffset(CGVector(dx: 0, dy: -16)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        note.typeText(" Ask at reception.")
        XCTAssertTrue((note.value as? String ?? "").contains("Outlet beside the gate."), app.debugDescription)
        XCTAssertTrue((note.value as? String ?? "").contains("Ask at reception."), app.debugDescription)
        dismissChargingKeyboard(in: app)
        app.buttons["charging.editor.cancel"].tap()
    }

    func testAccessNoteAcceptsTypingFromTheBlankAreaAndAfterRefocusing() throws {
        continueAfterFailure = false
        executionTimeAllowance = 120
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        beginChargingPlacement(in: app)
        app.buttons["charging.placement.confirm"].tap()

        let note = app.descendants(matching: .any)["charging.editor.access-note"].firstMatch
        revealChargingControl(note, in: app, direction: .down)
        XCTAssertTrue(note.waitForExistence(timeout: 5), app.debugDescription)
        note.coordinate(withNormalizedOffset: CGVector(dx: 0.85, dy: 0.85)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        note.typeText("Outside outlet beside the entrance.")
        XCTAssertEqual(note.value as? String, "Outside outlet beside the entrance.", app.debugDescription)
        dismissChargingKeyboard(in: app)

        let charging = app.buttons["charging.editor.kind.charging"]
        revealChargingControl(charging, in: app)
        charging.tap()
        revealChargingControl(note, in: app, direction: .down)
        note.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        note.typeText(" Ask staff first.")
        XCTAssertTrue(
            (note.value as? String ?? "").contains("Ask staff first."), app.debugDescription)
        XCTAssertTrue(
            (note.value as? String ?? "").contains("Outside outlet beside the entrance."), app.debugDescription)
        dismissChargingKeyboard(in: app)
        XCTAssertTrue(app.buttons["charging.editor.save"].isEnabled, app.debugDescription)
        app.buttons["charging.editor.cancel"].tap()
    }

    func testAccessNoteAcceptsTypingAfterChoosingEachChargingKind() throws {
        continueAfterFailure = false
        executionTimeAllowance = 180
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        for kind in ["charging", "food", "ev"] {
            beginChargingPlacement(in: app)
            app.buttons["charging.placement.confirm"].tap()
            let choice = app.buttons["charging.editor.kind.\(kind)"]
            XCTAssertTrue(choice.waitForExistence(timeout: 5), app.debugDescription)
            choice.tap()

            let note = app.descendants(matching: .any)["charging.editor.access-note"].firstMatch
            revealChargingControl(note, in: app, direction: .down)
            note.tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
            let text = "Access instructions for \(kind)."
            note.typeText(text)
            XCTAssertEqual(note.value as? String, text, app.debugDescription)
            dismissChargingKeyboard(in: app)
            app.buttons["charging.editor.cancel"].tap()
            XCTAssertTrue(note.waitForNonExistence(timeout: 5), app.debugDescription)
        }
    }

    func testSpotCreationPersistsAcrossRelaunchAndCanBeEditedAndDeleted() throws {
        continueAfterFailure = false
        executionTimeAllowance = 240
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 20), app.debugDescription)
        let existingPinIDs = pinIdentifiers(in: app)
        createFreeSpot(in: app, note: "Outlet beside the north entrance.")
        let newPinID = try XCTUnwrap(waitForNewPin(in: app, excluding: existingPinIDs), app.debugDescription)

        app.terminate()
        openCharging(app)
        var createdPin = app.buttons[newPinID]
        XCTAssertTrue(createdPin.waitForExistence(timeout: 20), app.debugDescription)
        createdPin.tap()
        app.buttons["charging.details.open"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["charging.spot.details-screen"].waitForExistence(timeout: 5))
        let savedAccess = app.descendants(matching: .any)["charging.details.access"].firstMatch
        XCTAssertTrue(savedAccess.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(
            (savedAccess.label + (savedAccess.value as? String ?? "")).contains("Outlet beside the north entrance."),
            app.debugDescription)
        app.buttons["charging.details.done"].tap()
        createdPin = app.buttons[newPinID]
        XCTAssertTrue(createdPin.waitForExistence(timeout: 5), app.debugDescription)
        createdPin.tap()
        app.buttons["charging.details.open"].tap()

        app.buttons["charging.details.edit"].tap()
        let paidCost = app.buttons["charging.editor.kind.food"]
        revealChargingControl(paidCost, in: app)
        XCTAssertTrue(paidCost.waitForExistence(timeout: 5), app.debugDescription)
        paidCost.tap()
        app.buttons["charging.editor.save"].tap()

        let updatedPin = app.buttons[newPinID]
        XCTAssertTrue(updatedPin.waitForExistence(timeout: 10), app.debugDescription)
        updatedPin.tap()
        app.buttons["charging.details.open"].tap()
        XCTAssertTrue(app.staticTexts["Paid"].waitForExistence(timeout: 5), app.debugDescription)
        app.buttons["charging.details.delete"].tap()
        let confirmDelete = app.sheets.buttons["Delete spot"].firstMatch
        XCTAssertTrue(confirmDelete.waitForExistence(timeout: 5), app.debugDescription)
        confirmDelete.tap()
        XCTAssertTrue(app.buttons[newPinID].waitForNonExistence(timeout: 10), app.debugDescription)
    }

    func testMapPressesDoNotStartPlacementUntilAddIsTapped() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 20), app.debugDescription)
        let map = app.maps.firstMatch
        let placementPoint = map.coordinate(withNormalizedOffset: CGVector(dx: 0.65, dy: 0.65))
        for duration in [0.1, 1.2] {
            placementPoint.press(forDuration: duration)
            let placement = XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "exists == true"),
                object: app.buttons["charging.placement.confirm"]
            )
            placement.isInverted = true
            XCTAssertEqual(XCTWaiter.wait(for: [placement], timeout: 1), .completed, app.debugDescription)
        }
        XCTAssertTrue(app.buttons["charging.add"].exists, app.debugDescription)
        app.buttons["charging.add"].tap()
        XCTAssertTrue(app.buttons["charging.placement.confirm"].waitForExistence(timeout: 5), app.debugDescription)
    }

    func testSwipingAwayNewAndExistingEditorsCancelsWithoutPlacement() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)
        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 20), app.debugDescription)

        beginChargingPlacement(in: app)
        app.buttons["charging.placement.confirm"].tap()
        XCTAssertTrue(app.buttons["charging.editor.kind.charging"].waitForExistence(timeout: 5))
        dismissChargingEditor(in: app, title: "Add charging spot")
        XCTAssertTrue(app.buttons["charging.add"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.buttons["charging.placement.confirm"].exists, app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["charging.editor.screen"].exists)

        libraryPin(in: app).tap()
        app.buttons["charging.details.open"].tap()
        XCTAssertTrue(app.buttons["charging.details.edit"].waitForExistence(timeout: 5))
        app.buttons["charging.details.edit"].tap()
        XCTAssertTrue(app.buttons["charging.editor.save"].waitForExistence(timeout: 5))
        dismissChargingEditor(in: app, title: "Edit charging spot")
        XCTAssertTrue(app.buttons["charging.add"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertFalse(app.buttons["charging.placement.confirm"].exists, app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["charging.editor.screen"].exists)
    }

    func testOptionalDetailsSurvivePlacementAndSave() throws {
        continueAfterFailure = false
        executionTimeAllowance = 240
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 20), app.debugDescription)
        let existingPinIDs = pinIdentifiers(in: app)
        let map = app.maps.firstMatch
        let placementPoint = map.coordinate(withNormalizedOffset: CGVector(dx: 0.65, dy: 0.65))
        beginChargingPlacement(in: app)
        placementPoint.press(
            forDuration: 0.1,
            thenDragTo: map.coordinate(withNormalizedOffset: CGVector(dx: 0.68, dy: 0.67))
        )
        app.buttons["charging.placement.cancel"].tap()
        beginChargingPlacement(in: app)
        placementPoint.press(
            forDuration: 0.1,
            thenDragTo: map.coordinate(withNormalizedOffset: CGVector(dx: 0.68, dy: 0.67))
        )
        app.buttons["charging.placement.confirm"].tap()

        fillChargingRequiredFields(in: app, note: "Ask the front desk for access.")
        let optionalDetails = app.buttons["charging.editor.add-details"]
        revealChargingControl(optionalDetails, in: app)
        XCTAssertTrue(optionalDetails.waitForExistence(timeout: 5), app.debugDescription)
        optionalDetails.tap()

        let name = app.textFields["charging.editor.name"]
        revealChargingControl(name, in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 5), app.debugDescription)
        name.tap()
        name.typeText("Garden entrance outlet")
        dismissChargingKeyboard(in: app)
        let hours = app.descendants(matching: .any)["charging.editor.hours"]
        revealChargingControl(hours, in: app)
        XCTAssertTrue(hours.waitForExistence(timeout: 5), app.debugDescription)
        hours.tap()
        hours.typeText("Daily, 08:00–18:00")
        dismissChargingKeyboard(in: app)
        XCTAssertFalse(app.descendants(matching: .any)["charging.editor.country"].exists)
        XCTAssertFalse(app.textFields["charging.editor.latitude"].exists)
        XCTAssertFalse(app.textFields["charging.editor.longitude"].exists)
        XCTAssertFalse(app.buttons["charging.editor.move-pin"].exists)
        app.buttons["charging.editor.save"].tap()

        XCTAssertTrue(
            app.descendants(matching: .any)["charging.editor.screen"].waitForNonExistence(timeout: 10),
            app.debugDescription)
        let spotID = try XCTUnwrap(waitForNewPin(in: app, excluding: existingPinIDs), app.debugDescription)
        let savedPin = app.buttons[spotID]
        XCTAssertTrue(savedPin.waitForExistence(timeout: 15), app.debugDescription)
        savedPin.tap()
        app.buttons["charging.details.open"].tap()
        XCTAssertTrue(app.staticTexts["Garden entrance outlet"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(
            app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Daily, 08:00–18:00"))
                .firstMatch.exists, app.debugDescription)
        let savedAccess = app.descendants(matching: .any)["charging.details.access"].firstMatch
        XCTAssertTrue((savedAccess.label + (savedAccess.value as? String ?? "")).contains("Ask the front desk"))
    }

    func testPlacementSurvivesChangingRideMapMode() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        let map = app.maps.firstMatch
        XCTAssertTrue(map.waitForExistence(timeout: 15), app.debugDescription)
        beginChargingPlacement(in: app)
        map.coordinate(withNormalizedOffset: CGVector(dx: 0.65, dy: 0.65)).press(
            forDuration: 0.1,
            thenDragTo: map.coordinate(withNormalizedOffset: CGVector(dx: 0.68, dy: 0.67))
        )
        let modePicker = app.descendants(matching: .any)["ride-map.mode-picker"].segmentedControls.firstMatch
        let live = modePicker.buttons["Live"]
        XCTAssertTrue(live.waitForExistence(timeout: 5), app.debugDescription)
        live.tap()
        modePicker.buttons["Charging"].tap()

        let confirm = app.buttons["charging.placement.confirm"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 10), app.debugDescription)
        confirm.tap()
        XCTAssertTrue(app.buttons["charging.editor.kind.charging"].waitForExistence(timeout: 5), app.debugDescription)
        for kind in ["charging", "food", "ev"] {
            XCTAssertFalse(app.buttons["charging.editor.kind.\(kind)"].isSelected)
        }
        attachScreenshot(app, name: "Charging placement retained after changing map mode")
    }

    func testCollapsedOptionalPriceErrorCanReopenDetails() throws {
        continueAfterFailure = false
        let app = chargingApp()
        defer { app.terminate() }
        openCharging(app)

        let map = app.maps.firstMatch
        XCTAssertTrue(map.waitForExistence(timeout: 15), app.debugDescription)
        beginChargingPlacement(in: app)
        app.buttons["charging.placement.confirm"].tap()
        fillChargingRequiredFields(in: app, note: "Outlet beside the door.")
        let paid = app.buttons["charging.editor.kind.food"]
        let free = app.buttons["charging.editor.kind.charging"]
        paid.tap()
        XCTAssertTrue(paid.isSelected, app.debugDescription)
        XCTAssertFalse(free.isSelected, app.debugDescription)
        let addDetails = app.buttons["charging.editor.add-details"]
        revealChargingControl(addDetails, in: app)
        addDetails.tap()
        let amount = app.textFields["charging.editor.price.amount"]
        revealChargingControl(amount, in: app)
        amount.tap()
        amount.typeText("1.")
        dismissChargingKeyboard(in: app)
        revealChargingControl(addDetails, in: app, direction: .down)
        addDetails.tap()

        let summary = app.buttons["charging.editor.optional-details-error"]
        XCTAssertTrue(summary.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(summary.isHittable, app.debugDescription)
        XCTAssertFalse(app.buttons["charging.editor.save"].isEnabled)
        attachScreenshot(app, name: "Charging collapsed optional-details validation")
        summary.tap()
        let priceError = app.staticTexts["charging.editor.price.error"]
        revealChargingControl(priceError, in: app)
        XCTAssertTrue(priceError.isHittable, app.debugDescription)
        revealChargingControl(amount, in: app, direction: .down)
        XCTAssertEqual(amount.value as? String, "1.", app.debugDescription)
        amount.coordinate(withNormalizedOffset: CGVector(dx: 1, dy: 0.5))
            .withOffset(CGVector(dx: -1, dy: 0))
            .tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        amount.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 2))
        dismissChargingKeyboard(in: app)
        XCTAssertEqual(amount.value as? String, "", app.debugDescription)
        XCTAssertTrue(app.buttons["charging.editor.save"].isEnabled, app.debugDescription)
        app.buttons["charging.editor.save"].tap()
        XCTAssertTrue(
            app.descendants(matching: .any)["charging.editor.screen"].waitForNonExistence(timeout: 10),
            app.debugDescription
        )
    }

    func testChargingPlacementAndSaveAtLargestTextSize() throws {
        continueAfterFailure = false
        executionTimeAllowance = 180
        let device = XCUIDevice.shared
        let previousLocation = device.location
        device.location = XCUILocation(location: CLLocation(latitude: 39.73925, longitude: -104.99025))
        defer { device.location = previousLocation }
        let app = chargingApp(preferredContentSizeCategory: "UICTContentSizeCategoryAccessibilityXXXL")
        defer { app.terminate() }
        openCharging(app)

        let map = app.maps.firstMatch
        XCTAssertTrue(map.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(libraryPin(in: app).waitForExistence(timeout: 10), app.debugDescription)
        beginChargingPlacement(in: app)
        let placement = app.scrollViews["charging.placement.scroll"]
        let confirm = app.buttons["charging.placement.confirm"]
        let cancel = app.buttons["charging.placement.cancel"]
        let useLocation = app.buttons["charging.placement.use-my-location"]
        XCTAssertTrue(placement.waitForExistence(timeout: 5), app.debugDescription)
        let locationReady = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true AND enabled == true"), object: useLocation
        )
        XCTAssertEqual(XCTWaiter.wait(for: [locationReady], timeout: 20), .completed, useLocation.debugDescription)
        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(tabBar.exists, app.debugDescription)
        // The app-level frame extends beneath the tab bar; measure only the visible map/card area.
        let usableCombinedRect = CGRect(
            x: map.frame.minX,
            y: map.frame.minY,
            width: map.frame.width,
            height: max(0, tabBar.frame.minY - map.frame.minY)
        )
        let visiblePlacementRect = placement.frame.intersection(usableCombinedRect)
        let visibleMapRect = CGRect(
            x: map.frame.minX,
            y: map.frame.minY,
            width: map.frame.width,
            height: max(0, visiblePlacementRect.minY - map.frame.minY)
        ).intersection(usableCombinedRect)
        XCTAssertGreaterThan(usableCombinedRect.height, 0, app.debugDescription)
        XCTAssertGreaterThanOrEqual(visiblePlacementRect.height, 44, app.debugDescription)
        XCTAssertGreaterThanOrEqual(visibleMapRect.height, 108, app.debugDescription)
        let title = app.staticTexts["charging.placement.title"]
        let instructions = app.staticTexts["charging.placement.instructions"]
        XCTAssertTrue(title.isHittable, app.debugDescription)
        XCTAssertTrue(visiblePlacementRect.contains(title.frame), title.debugDescription)
        XCTAssertTrue(instructions.isHittable, app.debugDescription)
        XCTAssertTrue(visiblePlacementRect.contains(instructions.frame), instructions.debugDescription)
        XCTAssertEqual(
            app.staticTexts["charging.placement.instructions"].label,
            "Move the map to position the pin.", app.debugDescription)
        attachScreenshot(app, name: "Charging placement card at largest text size")
        for control in [cancel, useLocation, confirm] {
            revealPlacementAction(control, in: placement, visibleViewport: visiblePlacementRect)
            XCTAssertTrue(control.isHittable, control.debugDescription)
            XCTAssertTrue(visiblePlacementRect.contains(control.frame), control.debugDescription)
            XCTAssertGreaterThanOrEqual(control.frame.width, 44, control.debugDescription)
            XCTAssertGreaterThanOrEqual(control.frame.height, 44, control.debugDescription)
        }
        XCTAssertEqual(confirm.label, "Use location", app.debugDescription)
        XCTAssertEqual(cancel.label, "Cancel", app.debugDescription)
        XCTAssertEqual(useLocation.label, "Use my location", app.debugDescription)
        revealPlacementAction(confirm, in: placement, visibleViewport: visiblePlacementRect)
        XCTAssertTrue(confirm.isHittable, confirm.debugDescription)
        XCTAssertTrue(visiblePlacementRect.contains(confirm.frame), confirm.debugDescription)
        attachScreenshot(app, name: "Charging placement confirmation visible at largest text size")
        confirm.tap()
        XCTAssertTrue(
            app.descendants(matching: .any)["charging.editor.screen"].waitForExistence(timeout: 5), app.debugDescription
        )
        fillChargingRequiredFields(
            in: app,
            note: "Outlet near the main entrance.",
            keyboardScreenshot: "Charging access note keyboard at largest text size"
        )
        XCTAssertFalse(app.buttons["charging.editor.move-pin"].exists)
        let save = app.buttons["charging.editor.save"]
        XCTAssertTrue(save.isHittable, save.debugDescription)
        save.tap()
        XCTAssertTrue(
            app.descendants(matching: .any)["charging.editor.screen"].waitForNonExistence(timeout: 20),
            app.debugDescription
        )
        let savedAccessNote = "Outlet near the main entrance."
        let selectedAccessNote = app.staticTexts["charging.selected.access-note"]
        XCTAssertTrue(selectedAccessNote.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(selectedAccessNote.label.contains(savedAccessNote), app.debugDescription)
        app.buttons["charging.details.open"].tap()
        XCTAssertTrue(
            app.descendants(matching: .any)["charging.spot.details-screen"].waitForExistence(timeout: 5),
            app.debugDescription
        )
        let detailsAccess = app.descendants(matching: .any)["charging.details.access"].firstMatch
        XCTAssertTrue(detailsAccess.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(
            (detailsAccess.label + (detailsAccess.value as? String ?? "")).contains(savedAccessNote),
            app.debugDescription
        )
        app.buttons["charging.details.done"].tap()
        attachScreenshot(app, name: "Charging spot saved at largest text size")

        app.terminate()
        openCharging(app)
        let list = app.buttons["charging.view.list"]
        XCTAssertTrue(list.waitForExistence(timeout: 10), app.debugDescription)
        list.tap()
        let persistedAccessNote = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS %@", savedAccessNote)
        ).firstMatch
        let persistedSpotList = app.collectionViews.firstMatch
        XCTAssertTrue(persistedSpotList.waitForExistence(timeout: 10), app.debugDescription)
        for _ in 0..<8 where !persistedAccessNote.exists {
            persistedSpotList.swipeUp(velocity: .slow)
        }
        XCTAssertTrue(persistedAccessNote.waitForExistence(timeout: 20), app.debugDescription)
        XCTAssertTrue(persistedAccessNote.label.contains(savedAccessNote), app.debugDescription)
        attachScreenshot(app, name: "Charging spot persists in list after relaunch")
    }

    private func libraryPin(in app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@ AND label BEGINSWITH %@", "charging.pin.", "Library")
        ).firstMatch
    }

    private func chargingApp(preferredContentSizeCategory: String? = nil) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--charging-acceptance"]
        app.launchArguments += [
            "-UIPreferredContentSizeCategoryName",
            preferredContentSizeCategory ?? UIApplication.shared.preferredContentSizeCategory.rawValue,
        ]
        app.launchEnvironment["CUTOUT_CHARGING_ACCEPTANCE_DATABASE_ID"] = acceptanceDatabaseID
        return app
    }

    private func createFreeSpot(in app: XCUIApplication, note: String) {
        let map = app.maps.firstMatch
        XCTAssertTrue(map.waitForExistence(timeout: 10), app.debugDescription)
        beginChargingPlacement(in: app)
        // Keep the new spot separate from the seeded Library pin so taps select the intended annotation.
        map.coordinate(withNormalizedOffset: CGVector(dx: 0.4, dy: 0.25)).press(
            forDuration: 0.1,
            thenDragTo: map.coordinate(withNormalizedOffset: CGVector(dx: 0.57, dy: 0.25))
        )
        let confirmPlacement = app.buttons["charging.placement.confirm"]
        XCTAssertTrue(confirmPlacement.waitForExistence(timeout: 5), app.debugDescription)
        confirmPlacement.tap()
        XCTAssertTrue(app.buttons["charging.editor.kind.charging"].waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(app.buttons["charging.editor.kind.charging"].isHittable)
        XCTAssertTrue(app.buttons["charging.editor.kind.food"].isHittable)
        XCTAssertTrue(app.buttons["charging.editor.kind.ev"].isHittable)
        for kind in ["charging", "food", "ev"] {
            XCTAssertFalse(app.buttons["charging.editor.kind.\(kind)"].isSelected)
        }
        XCTAssertFalse(app.textFields["charging.editor.latitude"].exists)
        let editor = app.descendants(matching: .any)["charging.editor.screen"].firstMatch
        let compactEditor = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in editor.frame.minY > app.frame.height * 0.25 }, object: editor)
        XCTAssertEqual(XCTWaiter.wait(for: [compactEditor], timeout: 5), .completed, app.debugDescription)
        attachScreenshot(app, name: "Charging spot required fields")

        fillRequiredFieldsAndSave(in: app, note: note)
    }

    private func beginChargingPlacement(in app: XCUIApplication) {
        let add = app.buttons["charging.add"]
        XCTAssertTrue(add.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(add.isEnabled, app.debugDescription)
        add.tap()
        XCTAssertTrue(app.buttons["charging.placement.confirm"].waitForExistence(timeout: 5), app.debugDescription)
    }

    private func fillRequiredFieldsAndSave(in app: XCUIApplication, note: String) {
        fillChargingRequiredFields(in: app, note: note)
        app.buttons["charging.editor.save"].tap()
    }

    private func fillChargingRequiredFields(
        in app: XCUIApplication,
        note: String,
        keyboardScreenshot: String? = nil
    ) {
        let accessNote = app.descendants(matching: .any)["charging.editor.access-note"]
        revealChargingControl(accessNote, in: app, direction: .down)
        XCTAssertTrue(accessNote.waitForExistence(timeout: 5), app.debugDescription)
        accessNote.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), app.debugDescription)
        let noteHasFocus = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "hasKeyboardFocus == true"), object: accessNote
        )
        XCTAssertEqual(XCTWaiter.wait(for: [noteHasFocus], timeout: 5), .completed, app.debugDescription)
        accessNote.typeText(note)
        if let keyboardScreenshot {
            attachScreenshot(app, name: keyboardScreenshot)
        }
        dismissChargingKeyboard(in: app)
        let freeCost = app.buttons["charging.editor.kind.charging"]
        revealChargingControl(freeCost, in: app)
        XCTAssertTrue(freeCost.waitForExistence(timeout: 5), app.debugDescription)
        freeCost.tap()
    }

    private func dismissChargingEditor(in app: XCUIApplication, title: String) {
        let editor = app.descendants(matching: .any)["charging.editor.screen"].firstMatch
        for _ in 0..<2 where editor.exists {
            let bar = app.navigationBars[title].firstMatch
            XCTAssertTrue(bar.exists, app.debugDescription)
            bar.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).press(
                forDuration: 0.1,
                thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95))
            )
        }
    }

    private func dismissChargingKeyboard(in app: XCUIApplication) {
        let keyboard = app.keyboards.firstMatch
        XCTAssertTrue(keyboard.waitForExistence(timeout: 5), app.debugDescription)
        let done = app.buttons["charging.editor.keyboard.done"]
        XCTAssertTrue(done.waitForExistence(timeout: 5), app.debugDescription)
        XCTAssertTrue(done.isHittable, done.debugDescription)
        XCTAssertGreaterThanOrEqual(done.frame.height, 44, done.debugDescription)
        XCTAssertLessThanOrEqual(
            done.frame.maxY,
            keyboard.frame.minY,
            "The Done button should remain visible above the keyboard.\n\(app.debugDescription)"
        )
        done.tap()
        let dismissed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == false"), object: keyboard)
        XCTAssertEqual(XCTWaiter.wait(for: [dismissed], timeout: 5), .completed, app.debugDescription)
    }

    private enum ChargingEditorScrollDirection {
        case up, down
    }

    private func revealChargingControl(
        _ element: XCUIElement,
        in app: XCUIApplication,
        direction: ChargingEditorScrollDirection = .up
    ) {
        for _ in 0..<6 {
            if element.isHittable { return }
            switch direction {
            case .up: app.swipeUp()
            case .down: app.swipeDown()
            }
        }
        XCTAssertTrue(element.isHittable, element.debugDescription)
    }

    private func revealPlacementAction(
        _ element: XCUIElement, in scrollView: XCUIElement, visibleViewport: CGRect
    ) {
        for _ in 0..<8 where !(element.isHittable && visibleViewport.contains(element.frame)) {
            let elementFrame = element.frame
            let scrollFrame = scrollView.frame
            guard scrollFrame.width > 0, scrollFrame.height > 0, !visibleViewport.isEmpty else { break }
            let swipeUp = elementFrame.isEmpty || elementFrame.maxY > visibleViewport.maxY
            let overflow =
                elementFrame.isEmpty
                ? 0
                : swipeUp
                    ? elementFrame.maxY - visibleViewport.maxY
                    : visibleViewport.minY - elementFrame.minY
            let distance = min(max(overflow + 16, 30), min(60, visibleViewport.height * 0.25))
            let startYInViewport = visibleViewport.midY + (swipeUp ? distance / 2 : -distance / 2)
            let endYInViewport = visibleViewport.midY - (swipeUp ? distance / 2 : -distance / 2)
            let x = (visibleViewport.midX - scrollFrame.minX) / scrollFrame.width
            let startY = (startYInViewport - scrollFrame.minY) / scrollFrame.height
            let endY = (endYInViewport - scrollFrame.minY) / scrollFrame.height
            let start = scrollView.coordinate(withNormalizedOffset: CGVector(dx: x, dy: startY))
            let end = scrollView.coordinate(withNormalizedOffset: CGVector(dx: x, dy: endY))
            start.press(forDuration: 0.01, thenDragTo: end, withVelocity: 100, thenHoldForDuration: 0)
        }
        XCTAssertTrue(element.isHittable, element.debugDescription)
        XCTAssertTrue(visibleViewport.contains(element.frame), element.debugDescription)
    }

    private func pinIdentifiers(in app: XCUIApplication) -> Set<String> {
        Set(
            app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "charging.pin."))
                .allElementsBoundByIndex.map(\.identifier)
        )
    }

    private func waitForNewPin(in app: XCUIApplication, excluding oldIdentifiers: Set<String>) -> String? {
        let deadline = Date().addingTimeInterval(20)
        while Date() < deadline {
            if let identifier = pinIdentifiers(in: app).subtracting(oldIdentifiers).first { return identifier }
            RunLoop.current.run(until: Date().addingTimeInterval(0.25))
        }
        return nil
    }

    private func waitForFrame(_ element: XCUIElement, near frame: CGRect, tolerance: CGFloat) -> Bool {
        let deadline = Date().addingTimeInterval(10)
        while Date() < deadline {
            let current = element.frame
            if element.exists,
                abs(current.minX - frame.minX) <= tolerance,
                abs(current.minY - frame.minY) <= tolerance,
                abs(current.width - frame.width) <= tolerance,
                abs(current.height - frame.height) <= tolerance
            {
                return true
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        return false
    }

    private func openCharging(_ app: XCUIApplication) {
        app.launch()
        let map = app.tabBars.buttons["Map"].firstMatch
        XCTAssertTrue(map.waitForExistence(timeout: 20), app.debugDescription)
        map.tap()
    }

    private func waitForSelection(_ button: XCUIElement, selected: Bool) {
        let expectation = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let button = object as? XCUIElement else { return false }
                return button.isSelected == selected && button.isEnabled
            },
            object: button
        )
        XCTAssertEqual(XCTWaiter.wait(for: [expectation], timeout: 10), .completed, button.debugDescription)
    }

    private func attachScreenshot(_ app: XCUIApplication, name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
