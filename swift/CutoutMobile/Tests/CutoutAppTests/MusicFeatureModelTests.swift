import XCTest
@testable import CutoutApp
@testable import CutoutMobile
import CutoutMobileFFI

@MainActor
final class MusicFeatureModelTests: XCTestCase {
    func testDeletingHistoryInvalidatesDetailBeforeRustDeleteAndClearsCaptureBeforeSelection() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let policyStore = MusicHistoryPolicyStore(defaults: suite.defaults)
        let selectedRideID = state.currentSnapshot()?.rideID
        var order = [String]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: policyStore,
            selectedRideID: { selectedRideID },
            invalidateHistory: {
                XCTAssertFalse(state.currentMusicHistory()?.events.isEmpty ?? true)
                order.append("invalidate")
            },
            updateCaptureObservation: { observation in
                if observation == nil { order.append("capture-clear") }
            },
            clearSelectedHistoryMusic: { order.append("selection-clear") }
        )
        XCTAssertTrue(model.setHistoryPolicy(.humanReadable))
        XCTAssertTrue(model.ingestObservation(observation(atMs: 200), wallClockAtMs: 1_700_000_000_000))
        let rideID = try XCTUnwrap(state.currentSnapshot()?.rideID)

        XCTAssertTrue(model.forgetHistory(for: rideID))

        XCTAssertEqual(order, ["invalidate", "capture-clear", "selection-clear"])
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertTrue(model.timelineEvents.isEmpty)
        XCTAssertEqual(policyStore.policy, .humanReadable)
        XCTAssertTrue(state.currentMusicEvents().isEmpty)
    }

    func testHiddenPlayerKeepsSettingsProjectionAndMonitoringIntentIndependent() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let visibility = MusicPlayerVisibilityStore()
        let wasHidden = visibility.isHidden
        visibility.setHidden(false)
        defer { visibility.setHidden(wasHidden) }
        let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
        monitoring.setEnabled(false)
        let model = makeModel(defaults: suite.defaults, monitoringPreferenceStore: monitoring)
        XCTAssertTrue(model.ingestObservation(observation(atMs: 10)))
        let settingsProjection = model.settingsNowPlaying

        model.dismissPlayer()

        XCTAssertTrue(model.isPlayerHidden)
        XCTAssertNil(model.nowPlaying)
        XCTAssertEqual(model.settingsNowPlaying, settingsProjection)
        XCTAssertFalse(monitoring.isEnabled)
    }

    func testLateCommandFeedbackCannotOverwriteAfterProviderSwitch() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)
        let requestID = try XCTUnwrap(model.beginCommandFeedback())
        XCTAssertNotNil(model.commandFeedback)

        model.selectProvider(.spotify)
        _ = model.finishCommand(.failed, provider: .appleMusic, requestID: requestID)

        XCTAssertNil(model.commandFeedback)
    }

    func testOpaqueSpotifyLocalIdentifierIsNotCopiedIntoPevcap() {
        XCTAssertNil(MusicFeatureModel.pevcapTrackIdentifier(
            policy: .opaqueItem,
            provider: .spotify,
            identifier: "spotify:local:artist:album:track"
        ))
        XCTAssertEqual(MusicFeatureModel.pevcapTrackIdentifier(
            policy: .opaqueItem,
            provider: .spotify,
            identifier: "spotify:track:catalog-id"
        ), "spotify:track:catalog-id")
        XCTAssertEqual(MusicFeatureModel.pevcapTrackIdentifier(
            policy: .humanReadable,
            provider: .spotify,
            identifier: "spotify:local:artist:album:track"
        ), "spotify:local:artist:album:track")
    }

    func testHistoryPolicyLoadsFromItsStoreAndRedactsTheRustTimeline() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let policyStore = MusicHistoryPolicyStore(defaults: suite.defaults)
        policyStore.set(.opaqueItem)
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        let model = makeModel(state: state, defaults: suite.defaults, historyPolicyStore: policyStore)

        XCTAssertEqual(model.historyPolicy, .opaqueItem)
        XCTAssertTrue(model.setHistoryPolicy(.humanReadable))
        XCTAssertTrue(model.ingestObservation(observation(atMs: 200)))
        XCTAssertEqual(model.timelineEvents.first?.title, "Track")

        XCTAssertTrue(model.setHistoryPolicy(.opaqueItem))

        XCTAssertEqual(model.timelineEvents.count, 1)
        XCTAssertNil(model.timelineEvents.first?.title)
        XCTAssertNil(model.timelineEvents.first?.artist)
        XCTAssertNil(state.currentMusicEvents().first?.title)
        XCTAssertEqual(policyStore.policy, .opaqueItem)
    }

    func testRestoringPlayerAfterHiddenProviderSwitchDoesNotShowPreviousProvider() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let visibility = MusicPlayerVisibilityStore()
        let wasHidden = visibility.isHidden
        visibility.setHidden(false)
        defer { visibility.setHidden(wasHidden) }
        let model = makeModel(defaults: suite.defaults)
        model.restorePlayer()
        XCTAssertTrue(model.ingestObservation(observation(atMs: 10)))
        model.dismissPlayer()
        model.selectProvider(.spotify)

        model.restorePlayer()

        XCTAssertEqual(model.nowPlaying?.provider, .spotify)
        XCTAssertEqual(model.nowPlaying?.state, .unavailable)
        XCTAssertNil(model.nowPlaying?.item)
    }

#if canImport(MediaPlayer) && os(iOS)
    func testActualMonitorTaskUsesPassiveAuthorizationAndCancelsOnBackground() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
        monitoring.setEnabled(true)
        let monitor = TestAppleMusicMonitor()
        let pollWaiter = TestMusicMonitorPollWaiter()
        var captureObservations = [MobilePevcapMusicEventDto?]()
        let model = makeModel(
            defaults: suite.defaults,
            monitoringPreferenceStore: monitoring,
            updateCaptureObservation: { captureObservations.append($0) },
            appleMonitor: monitor,
            monitorPollWaiter: { deadlineMs, _ in
                await pollWaiter.wait(until: deadlineMs)
            }
        )

        model.start(sceneIsActive: true)
        await waitUntil("first monitor poll") { pollWaiter.startedCount == 1 }

        XCTAssertEqual(monitor.authorizationPrompts, [false])
        XCTAssertEqual(monitor.startCount, 1)
        XCTAssertEqual(monitor.stopCount, 1, "starting a new generation first tears down any prior provider session")
        XCTAssertEqual(model.settingsNowPlaying?.state, MobileMusicPlaybackStateDto.playing)
        XCTAssertEqual(model.settingsNowPlaying?.item?.identifier, "monitor-track")
        XCTAssertTrue(model.timelineEvents.isEmpty)
        XCTAssertFalse(captureObservations.contains { $0 != nil })

        model.start(sceneIsActive: true)
        await waitUntil("replacement monitor poll") { pollWaiter.startedCount == 2 }
        XCTAssertEqual(monitor.startCount, 2)
        XCTAssertEqual(monitor.stopCount, 2)

        pollWaiter.releaseNext(returning: false)
        await waitUntil("superseded monitor poll") { pollWaiter.completedCount == 1 }
        XCTAssertEqual(monitor.stopCount, 2, "the superseded task must not stop the replacement monitor")

        model.sceneDidEnterBackground()
        XCTAssertEqual(monitor.stopCount, 3)
        pollWaiter.releaseNext(returning: false)
        await waitUntil("cancelled monitor poll") { pollWaiter.completedCount == 2 }
        await Task.yield()

        XCTAssertEqual(monitor.stopCount, 3, "a stale task must not tear down a later provider session")
        XCTAssertEqual(monitor.suspensionCount, 1)
        XCTAssertFalse(monitor.authorizationPrompts.contains(true))
    }
#endif

#if !os(iOS)
    func testUnavailableMusicCommandPublishesVisibleFeedback() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)

        let outcome = await model.handleCommand(.play)

        XCTAssertEqual(outcome, .unavailable)
        XCTAssertEqual(model.commandStatusText, pevLocalizedText("music.command.unavailable"))
    }

    func testOlderSameProviderCompletionAndDismissalCannotReplaceNewerFeedback() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)
        let first = try XCTUnwrap(model.beginCommandFeedback())
        let second = try XCTUnwrap(model.beginCommandFeedback())

        _ = model.finishCommand(.failed, provider: .appleMusic, requestID: first)
        XCTAssertNil(model.commandStatusText)

        _ = model.finishCommand(.refused, provider: .appleMusic, requestID: second)
        XCTAssertEqual(model.commandStatusText, pevLocalizedText("music.command.refused"))

        model.dismissCommandFeedback(requestID: first)
        XCTAssertEqual(model.commandStatusText, pevLocalizedText("music.command.refused"))
        model.dismissCommandFeedback(requestID: second)
        XCTAssertNil(model.commandStatusText)
    }

    func testSystemAlertDismissalClearsCurrentMusicCommandFeedback() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)
        let requestID = try XCTUnwrap(model.beginCommandFeedback())
        _ = model.finishCommand(.failed, provider: .appleMusic, requestID: requestID)

        model.dismissCommandFeedback()

        XCTAssertNil(model.commandFeedback)
    }

    func testMusicSetupShowsUnavailableOnUnsupportedPlatform() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)

        model.connect()

        XCTAssertEqual(model.nowPlaying?.state, .unavailable)
        XCTAssertEqual(model.nowPlaying?.provider, .appleMusic)
    }

    func testValidObservationClearsRecoveredValidationErrorWithoutATransition() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)
        let capabilities = MobileMusicCapabilitiesDto(
            previous: true,
            play: false,
            pause: true,
            next: true,
            openProvider: true
        )
        func observation(
            atMs: UInt64,
            positionMilliseconds: UInt64?,
            durationMilliseconds: UInt64?
        ) -> MusicProviderObservation {
            MusicProviderObservation(snapshot: MobileMusicSnapshotDto(
                provider: .appleMusic,
                sessionId: "session",
                state: .playing,
                item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
                positionMilliseconds: positionMilliseconds,
                durationMilliseconds: durationMilliseconds,
                observedAtMs: atMs,
                capabilities: capabilities
            ))
        }

        XCTAssertTrue(model.ingestObservation(observation(
            atMs: 1,
            positionMilliseconds: 10,
            durationMilliseconds: 100
        )))
        XCTAssertFalse(model.ingestObservation(observation(
            atMs: 2,
            positionMilliseconds: 101,
            durationMilliseconds: 100
        )))
        XCTAssertNotNil(model.historySaveError)
        XCTAssertEqual(model.settingsNowPlaying?.state, .stale)
        XCTAssertFalse(model.settingsNowPlaying?.capabilities.pause ?? true)

        XCTAssertTrue(model.ingestObservation(observation(
            atMs: 3,
            positionMilliseconds: 10,
            durationMilliseconds: 100
        )))
        XCTAssertNil(model.historySaveError)
        XCTAssertEqual(model.settingsNowPlaying?.state, .playing)
        XCTAssertTrue(model.settingsNowPlaying?.capabilities.pause ?? false)
    }
#endif

    private func makeModel(
        state: MobileRideMapState? = nil,
        defaults: UserDefaults,
        historyPolicyStore: MusicHistoryPolicyStore? = nil,
        monitoringPreferenceStore: MusicMonitoringPreferenceStore? = nil,
        selectedRideID: @escaping @MainActor () -> String? = { nil },
        invalidateHistory: @escaping @MainActor () -> Void = {},
        updateCaptureObservation: @escaping @MainActor (MobilePevcapMusicEventDto?) -> Void = { _ in },
        clearSelectedHistoryMusic: @escaping @MainActor () -> Void = {},
        setRideHistoryError: @escaping @MainActor (MobileRideMapError) -> Void = { _ in },
        appleMonitor: (any AppleMusicMonitorDriving)? = nil,
        monitorPollWaiter: MusicMonitorPollWaiter? = nil
    ) -> MusicFeatureModel {
        MusicFeatureModel(
            providerSelectionStore: MusicProviderSelectionStore(defaults: defaults),
            historyPolicyStore: historyPolicyStore ?? MusicHistoryPolicyStore(defaults: defaults),
            monitoringPreferenceStore: monitoringPreferenceStore ?? MusicMonitoringPreferenceStore(defaults: defaults),
            rideMapState: state,
            monotonicNow: { 1_000 },
            updateCapturePolicy: { _ in },
            updateCaptureObservation: updateCaptureObservation,
            invalidateHistoryForDeletion: invalidateHistory,
            selectedHistoryRideID: selectedRideID,
            clearSelectedHistoryMusic: clearSelectedHistoryMusic,
            setRideHistoryError: setRideHistoryError,
            appleMonitor: appleMonitor,
            monitorPollWaiter: monitorPollWaiter
        )
    }

    private func waitUntil(
        _ description: String,
        maxTurns: Int = 10_000,
        file: StaticString = #filePath,
        line: UInt = #line,
        condition: @escaping @MainActor () -> Bool
    ) async {
        for _ in 0 ..< maxTurns {
            if condition() { return }
            await Task.yield()
        }
        XCTFail("timed out waiting for \(description)", file: file, line: line)
    }

    private func makeDefaults() throws -> (name: String, defaults: UserDefaults) {
        let name = "MusicFeatureModelTests-\(UUID().uuidString)"
        return (name, try XCTUnwrap(UserDefaults(suiteName: name)))
    }

    private func observation(atMs: UInt64) -> MusicProviderObservation {
        MusicProviderObservation(snapshot: MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "feature-test",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Track", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: atMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: true,
                pause: true,
                next: false,
                openProvider: true
            )
        ))
    }
}

#if canImport(MediaPlayer) && os(iOS)
@MainActor
private final class TestAppleMusicMonitor: AppleMusicMonitorDriving {
    private(set) var authorizationPrompts = [Bool]()
    private(set) var startCount = 0
    private(set) var stopCount = 0
    private(set) var suspensionCount = 0

    func requestAuthorization(allowPrompt: Bool) async -> Bool {
        authorizationPrompts.append(allowPrompt)
        return true
    }

    func unauthorizedSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "test-monitor",
            state: .unauthorized,
            item: nil,
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: observedAtMs,
            capabilities: .init(previous: false, play: false, pause: false, next: false, openProvider: true)
        )
    }

    func startMonitoring(
        observedAtMs: @escaping @MainActor () -> UInt64,
        onObservation: @escaping @MainActor (MusicProviderObservation) -> Void
    ) async {
        startCount += 1
        onObservation(MusicProviderObservation(snapshot: MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "test-monitor",
            state: .playing,
            item: .init(identifier: "monitor-track", title: "Track", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: observedAtMs(),
            capabilities: .init(previous: false, play: true, pause: true, next: false, openProvider: true)
        )))
    }

    func stopMonitoring() {
        stopCount += 1
    }

    func applySuspension(_ suspension: MobileMusicProviderSuspension) {
        _ = suspension
        suspensionCount += 1
    }

    func refreshObservation(observedAtMs: UInt64) {
        _ = observedAtMs
    }
}

@MainActor
private final class TestMusicMonitorPollWaiter {
    private var continuations = [CheckedContinuation<Bool, Never>]()
    private(set) var startedCount = 0
    private(set) var completedCount = 0

    func wait(until _: UInt64) async -> Bool {
        startedCount += 1
        let result = await withCheckedContinuation { continuations.append($0) }
        completedCount += 1
        return result
    }

    func releaseNext(returning result: Bool) {
        guard !continuations.isEmpty else { return }
        continuations.removeFirst().resume(returning: result)
    }
}
#endif
