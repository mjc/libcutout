import XCTest
@testable import CutoutApp
@testable import CutoutMobile
import CutoutMobileFFI

@MainActor
final class MusicFeatureModelTests: XCTestCase {
    func testOpeningHistoricalRideDetailDoesNotDispatchMusicTransport() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let rideID = "historical-ride"
        let historyQuery = HistoricalRideQuery(rideID: rideID)
        var providerCommands = [(MobileMusicProviderDto, MobileMusicCommandDto)]()
        let music = makeModel(
            defaults: suite.defaults,
            providerCommandHandler: { provider, command in
                providerCommands.append((provider, command))
                return .unavailable
            }
        )
        let history = RideHistoryModel(
            stateProvider: { historyQuery },
            storageErrorProvider: { nil }
        )
        XCTAssertTrue(music.ingestObservation(observation(atMs: 1_000)))
        let playingProjection = try XCTUnwrap(music.settingsNowPlaying)

        history.reload(selecting: rideID)
        await waitUntil("historical ride detail load") {
            history.selectedRideID == rideID
                && !history.isLoading
                && !history.routeLoading
                && !history.detailRouteLoading
        }

        XCTAssertEqual(music.selectedProvider, .appleMusic)
        XCTAssertEqual(music.settingsNowPlaying, playingProjection)
        XCTAssertNil(history.routeError)
        XCTAssertNil(history.detailRouteError)
        XCTAssertFalse(history.detailMusicTimelineUnavailable)
        XCTAssertTrue(
            providerCommands.isEmpty,
            "loading historical ride detail must not send music transport commands"
        )
    }

#if canImport(SpotifyiOS) && os(iOS)
    func testSpotifyCallbackAdmissionSurvivesEitherSceneEventOrder() throws {
        let redirectURL = try XCTUnwrap(URL(string: "cutout-spotify://spotify-login-callback"))
        let callbackURL = try XCTUnwrap(
            URL(string: "cutout-spotify://spotify-login-callback/#access_token=test")
        )

        for callbackBeforeResume in [true, false] {
            let lifecycle = MobileMusicProviderLifecycle()
            lifecycle.requestMonitor(request: .authorize)
            XCTAssertEqual(lifecycle.beginMonitor()?.start, .authorize)
            let authorization = try XCTUnwrap(lifecycle.beginAuthorizationEffect(
                kind: .authorizing,
                nowMs: 1_000
            ))
            _ = try XCTUnwrap(lifecycle.beginProviderSession())
            XCTAssertTrue(lifecycle.suspend().observationGap)

            if callbackBeforeResume {
                XCTAssertTrue(SpotifyAuthorizationCallbackGate.accepts(
                    callbackURL,
                    redirectURL: redirectURL,
                    authorizationID: authorization.id,
                    lifecycle: lifecycle
                ))
                XCTAssertEqual(
                    lifecycle.finishAuthorization(id: authorization.id),
                    .authorizing
                )
            }

            XCTAssertEqual(lifecycle.resume(), .restored)
            XCTAssertEqual(lifecycle.beginMonitor()?.start, .observe)

            if !callbackBeforeResume {
                XCTAssertTrue(SpotifyAuthorizationCallbackGate.accepts(
                    callbackURL,
                    redirectURL: redirectURL,
                    authorizationID: authorization.id,
                    lifecycle: lifecycle
                ))
                XCTAssertEqual(
                    lifecycle.finishAuthorization(id: authorization.id),
                    .authorizing
                )
            }

            XCTAssertFalse(SpotifyAuthorizationCallbackGate.accepts(
                callbackURL,
                redirectURL: redirectURL,
                authorizationID: authorization.id,
                lifecycle: lifecycle
            ))
        }
    }
#endif

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

    func testExplicitShutdownInvalidatesLateMonitorObservationBeforeAdapterTeardown() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
        monitoring.setEnabled(true)
        let monitor = TestAppleMusicMonitor()
        let pollWaiter = TestMusicMonitorPollWaiter()
        let model = makeModel(
            defaults: suite.defaults,
            monitoringPreferenceStore: monitoring,
            appleMonitor: monitor,
            monitorPollWaiter: { deadlineMs, _ in
                await pollWaiter.wait(until: deadlineMs)
            }
        )

        model.start(sceneIsActive: true)
        await waitUntil("monitor poll before shutdown") { pollWaiter.startedCount == 1 }
        let nowPlayingBeforeShutdown = try XCTUnwrap(model.settingsNowPlaying)

        monitor.emitOnNextStop(observation(atMs: 2_000, identifier: "teardown-track"))
        model.stopMonitoring()
        monitor.emit(observation(atMs: 2_000, identifier: "late-track"))
        pollWaiter.releaseNext(returning: false)
        await waitUntil("shutdown poll completion") { pollWaiter.completedCount == 1 }

        XCTAssertEqual(monitor.stopCount, 2)
        XCTAssertEqual(model.settingsNowPlaying, nowPlayingBeforeShutdown)
        XCTAssertTrue(model.timelineEvents.isEmpty)
    }

    func testAppModelDeallocationStopsMusicMonitor() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
        monitoring.setEnabled(true)
        let monitor = TestAppleMusicMonitor()
        var model: CutoutAppModel? = CutoutAppModel(
            core: MusicMonitorSessionDriver(),
            musicHistoryPolicyStore: MusicHistoryPolicyStore(defaults: suite.defaults),
            musicProviderSelectionStore: MusicProviderSelectionStore(defaults: suite.defaults),
            musicMonitoringPreferenceStore: monitoring,
            appleMusicMonitor: monitor
        )
        weak var weakModel = model

        model?.music.start(sceneIsActive: true)
        await waitUntil("app-owned music monitor start") { monitor.startCount == 1 }

        model = nil

        XCTAssertNil(weakModel)
        await waitUntil("app-model deallocation stops music adapter") { monitor.stopCount == 2 }
    }

    func testAppModelKeepsOneMusicMonitorAcrossStartupAndSceneResume() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
        monitoring.setEnabled(true)
        let monitor = TestAppleMusicMonitor()
        let model = CutoutAppModel(
            core: MusicMonitorSessionDriver(),
            musicHistoryPolicyStore: MusicHistoryPolicyStore(defaults: suite.defaults),
            musicProviderSelectionStore: MusicProviderSelectionStore(defaults: suite.defaults),
            musicMonitoringPreferenceStore: monitoring,
            appleMusicMonitor: monitor
        )

        model.start(sceneIsActive: true)
        await waitUntil("root-owned music monitor start") { monitor.startCount == 1 }

        model.start(sceneIsActive: true)
        model.appDidBecomeActive()
        await Task.yield()
        XCTAssertEqual(monitor.startCount, 1, "repeated startup and active notifications must not duplicate the monitor")

        model.appDidEnterBackground()
        await waitUntil("root music monitor suspension") { monitor.stopCount >= 2 }
        model.appDidBecomeActive()
        await waitUntil("root music monitor restoration") { monitor.startCount == 2 }

        model.music.stopMonitoring()
        await waitUntil("root music monitor shutdown") { monitor.stopCount >= 4 }
        XCTAssertEqual(monitor.startCount, 2)
    }

    func testExplicitConnectAllowsAuthorizationPrompt() async throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let monitor = TestAppleMusicMonitor()
        let pollWaiter = TestMusicMonitorPollWaiter()
        let model = makeModel(
            defaults: suite.defaults,
            appleMonitor: monitor,
            monitorPollWaiter: { deadlineMs, _ in
                await pollWaiter.wait(until: deadlineMs)
            }
        )

        model.connect()
        await waitUntil("explicit authorization monitor poll") { pollWaiter.startedCount == 1 }

        XCTAssertEqual(monitor.authorizationPrompts, [true])
        XCTAssertEqual(monitor.startCount, 1)

        model.stopMonitoring()
        pollWaiter.releaseNext(returning: false)
        await waitUntil("explicit authorization monitor cancellation") { pollWaiter.completedCount == 1 }
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
        monitorPollWaiter: MusicMonitorPollWaiter? = nil,
        providerCommandHandler: MusicProviderCommandHandler? = nil
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
            monitorPollWaiter: monitorPollWaiter,
            providerCommandHandler: providerCommandHandler
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

    private func observation(atMs: UInt64, identifier: String = "track-1") -> MusicProviderObservation {
        MusicProviderObservation(snapshot: MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "feature-test",
            state: .playing,
            item: MobileMusicItemDto(identifier: identifier, title: "Track", artist: "Artist"),
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

private struct HistoricalRideQuery: RideHistoryQuerying {
    private let summary: MobileRideMapHistorySummaryDto

    init(rideID: String) {
        summary = MobileRideMapHistorySummaryDto(
            rideID: rideID,
            state: .saved,
            summary: MobileRideMapSummaryDto(
                pointCount: 0,
                distanceMeters: 0,
                durationMilliseconds: 0
            ),
            segmentCount: 0,
            createdAtMilliseconds: 0,
            candidateVehicle: nil,
            associatedVehicle: nil,
            associatedVehicleName: nil,
            telemetryState: .associatedNoTelemetry
        )
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        _ = (rideID, budget, viewport, privacy, cancellation)
        return MobileRideMapRouteProjection(
            points: [],
            segments: [],
            sourcePointCount: 0,
            sourceSegmentCount: 0,
            candidatePointCount: 0,
            candidateSegmentCount: 0,
            displayedSegmentCount: 0,
            backgroundGapCount: 0,
            presence: .emptyRide
        )
    }

    func storedMusicHistory(rideID: String) throws -> MobileMusicHistoryDto {
        _ = rideID
        return MobileMusicHistoryDto(status: .unavailable, events: [])
    }

    func storedHistoryVehicleOptions() throws -> [MobileRideMapHistoryVehicleOptionDto] {
        []
    }

    func storedHistoryRide(rideID: String) throws -> MobileRideMapHistorySummaryDto? {
        summary.rideID == rideID ? summary : nil
    }

    func storedHistoryPage(
        cursor: MobileRideCursorDto?,
        limit: UInt32,
        filter: MobileRideHistoryFilterDto?
    ) throws -> MobileRideMapHistoryPageDto {
        _ = (cursor, limit, filter)
        return MobileRideMapHistoryPageDto(summaries: [summary], nextCursor: nil)
    }
}

#if canImport(MediaPlayer) && os(iOS)
@MainActor
final class TestAppleMusicMonitor: AppleMusicMonitorDriving {
    private(set) var authorizationPrompts = [Bool]()
    private(set) var startCount = 0
    private(set) var stopCount = 0
    private(set) var suspensionCount = 0
    private var onObservation: (@MainActor (MusicProviderObservation) -> Void)?
    private var onNextStopObservation: MusicProviderObservation?

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
        self.onObservation = onObservation
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

    func emit(_ observation: MusicProviderObservation) {
        onObservation?(observation)
    }

    func emitOnNextStop(_ observation: MusicProviderObservation) {
        onNextStopObservation = observation
    }

    func stopMonitoring() {
        stopCount += 1
        if let onNextStopObservation {
            self.onNextStopObservation = nil
            onObservation?(onNextStopObservation)
        }
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
private final class MusicMonitorSessionDriver: CutoutSessionDriving {
    let rideSessionStateHandle = CutoutSessionStateHandle()
    var onDisplayStateChange: ((RideDisplayState) -> Void)?
    var onPhaseChange: ((SessionConnectionPhase) -> Void)?
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)?
    var onCaptureEvent: ((CaptureEvent) -> Void)?
    var onScanStateChange: ((DevicePickerScanState) -> Void)?
    var onSettingsChange: ((DeviceSettings) -> Void)?
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)?
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)?
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)?
    var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)?
    var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)?
    var onRideMapErrorChange: ((MobileRideMapErrorEvent) -> Void)?
    var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)?
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)?
    var onBluetoothRestorationResolved: ((String?) -> Void)?
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { nil }
    var electricUnicycleModel: ElectricUnicycleModel? { nil }
    var settings: DeviceSettings { rideSessionStateHandle.settings() }

    func start() {}
    func pair(platformIdentifier: String) -> Bool { false }
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool { false }
    func probe(platformIdentifier: String) -> Bool { false }
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool { false }
    func changeCaptureLabel(
        generation: CaptureGeneration,
        action: MobileCaptureLabelActionDto
    ) throws -> [MobileCaptureLabelDto] { [] }
    func updateMusicCapturePolicy(_ policy: MobileMusicHistoryPolicyDto) {}
    func updateMusicCaptureObservation(_ observation: MobilePevcapMusicEventDto?) {}
    func flushCapture() async -> Bool { true }
    func finishCapture() async -> Bool { true }
    func disconnectAndScan() {}
    func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue) throws {}
    func submitDeviceAction(token: ConnectionAttemptToken, id: DeviceActionID) throws {}
    func now() -> MonotonicMilliseconds { MonotonicMilliseconds(0) }
    func updateRideLocationDemand(for state: MobileRideMapStateDto) {}
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
