import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class MusicIntegrationTests: XCTestCase {
    func testSpotifyCallbackAcceptsObservedRootSlashWithoutAcceptingAnotherPath() throws {
        let configured = try XCTUnwrap(URL(string: "cutout-spotify://spotify-login-callback"))
        let returned = try XCTUnwrap(URL(string: "cutout-spotify://spotify-login-callback/#access_token=test"))
        XCTAssertEqual(configured.scheme, returned.scheme)
        XCTAssertEqual(configured.host, returned.host)
        XCTAssertEqual(configured.path, "")
        XCTAssertEqual(returned.path, "/")
        XCTAssertTrue(musicCallbackPathMatches(expected: configured.path, actual: returned.path))
        XCTAssertFalse(musicCallbackPathMatches(expected: configured.path, actual: "/another-callback"))
    }

    @MainActor
    func testSpotifyEpisodeWithoutArtistStillUpdatesPlayingTitle() throws {
        let coordinator = MusicIntegrationCoordinator(rideMapState: nil)
        let snapshot = MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: "spotify-app-remote",
            state: .playing,
            item: .init(
                identifier: "spotify:episode:example",
                title: "Episode title",
                artist: ""
            ),
            positionMilliseconds: 1_777_678,
            durationMilliseconds: 3_783_235,
            observedAtMs: 100,
            capabilities: .init(previous: false, play: false, pause: true, next: true, openProvider: true)
        )
        _ = try coordinator.ingest(snapshot: snapshot, wallClockAtMs: 1_700_000_000_000, clockUncertaintyMs: 1)
        XCTAssertEqual(coordinator.nowPlaying?.title, "Episode title")
        XCTAssertEqual(coordinator.nowPlaying?.state, .playing)
        XCTAssertEqual(coordinator.nowPlaying?.playPauseCommand, .pause)
        XCTAssertNil(coordinator.nowPlaying?.item?.artist)
    }

    func testMissingSpotifyMetadataDoesNotHideConnectionStatus() {
        let disconnected = MusicNowPlaying(provider: .spotify, state: .disconnected)
        XCTAssertEqual(disconnected.title, pevLocalizedText("music.state.disconnected"))
        XCTAssertEqual(disconnected.statusText, pevLocalizedText("music.state.disconnected"))
        let playing = MusicNowPlaying(
            provider: .spotify, state: .playing,
            item: .init(identifier: "spotify:track:example", title: nil, artist: nil)
        )
        XCTAssertEqual(playing.title, pevLocalizedText("music.state.playing"))
        XCTAssertNil(playing.statusText)
    }

    func testMissingMetadataUsesPlaybackStateInsteadOfInventingStoppedPlayback() {
        for state in [MobileMusicPlaybackStateDto.playing, .paused, .buffering, .stale, .unauthorized] {
            let playing = MusicNowPlaying(provider: .spotify, state: state)
            XCTAssertEqual(playing.title, pevLocalizedText(musicPlaybackTitleKey(state: state)))
            XCTAssertNotEqual(playing.title, pevLocalizedText("music.not_playing"))
        }
    }

    func testProviderFailureStatesKeepSetupActionAvailable() {
        for state in [
            MobileMusicPlaybackStateDto.unauthorized,
            .unavailable,
            .disconnected,
            .stale,
        ] {
            XCTAssertTrue(
                MusicNowPlaying(provider: .spotify, state: state).requiresSetup,
                "expected setup action for \(state)"
            )
        }
        for state in [
            MobileMusicPlaybackStateDto.playing,
            .paused,
            .buffering,
            .interrupted,
            .stopped,
        ] {
            XCTAssertFalse(
                MusicNowPlaying(provider: .spotify, state: state).requiresSetup,
                "did not expect setup action for \(state)"
            )
        }
    }

    func testProviderMonitoringModeMatchesSupportedLifecycle() {
        XCTAssertEqual(
            MobileMusicProviderDto.appleMusic.monitoringMode,
            .appleMusicSystemPlayer
        )
#if canImport(SpotifyiOS) && os(iOS)
        XCTAssertEqual(MobileMusicProviderDto.spotify.monitoringMode, .spotifyAppRemote)
#else
        XCTAssertEqual(MobileMusicProviderDto.spotify.monitoringMode, .unavailable)
#endif
    }

    func testPlayerStateFreshnessExpiresOnlyAfterRustObservationDeadline() {
        let request = MobileMusicPlayerRequest()

        XCTAssertFalse(request.isStale(nowMs: 30_000))
        request.markObserved(nowMs: 1_000)
        XCTAssertFalse(request.isStale(nowMs: 31_000))
        XCTAssertTrue(request.isStale(nowMs: 31_001))

        request.markObserved(nowMs: 31_001)
        XCTAssertFalse(request.isStale(nowMs: 61_001))
        request.reset()
        XCTAssertFalse(request.isStale(nowMs: UInt64.max))
    }

    @MainActor
    func testSpotifyTransportStaysUnavailableUntilAppRemoteIsProven() async {
        let adapter = SpotifyProviderAdapter()

        let outcomes = await [
            adapter.perform(.previous),
            adapter.perform(.play),
            adapter.perform(.pause),
            adapter.perform(.next),
        ]

        XCTAssertEqual(outcomes, [.unavailable, .unavailable, .unavailable, .unavailable])
    }

    func testMusicHistoryPolicyStoreDefaultsToDisabledAndRoundTrips() throws {
        let suiteName = "MusicHistoryPolicyStoreTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = MusicHistoryPolicyStore(defaults: defaults)

        XCTAssertEqual(store.policy, .disabled)
        for policy in MobileMusicHistoryPolicyDto.allCases {
            store.set(policy)
            XCTAssertEqual(store.policy, policy)
        }
    }

    func testMusicProviderSelectionStoreDefaultsToAppleMusicAndRoundTrips() throws {
        let suiteName = "MusicProviderSelectionStoreTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = MusicProviderSelectionStore(defaults: defaults)

        XCTAssertEqual(store.provider, .appleMusic)
        store.set(.spotify)
        XCTAssertEqual(store.provider, .spotify)
        store.set(.appleMusic)
        XCTAssertEqual(store.provider, .appleMusic)
    }

    func testMusicMonitoringPreferenceStoreDefaultsOffAndRoundTrips() throws {
        let suiteName = "MusicMonitoringPreferenceStoreTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = MusicMonitoringPreferenceStore(defaults: defaults)

        XCTAssertFalse(store.isEnabled)
        store.setEnabled(true)
        XCTAssertTrue(store.isEnabled)
        store.setEnabled(false)
        XCTAssertFalse(store.isEnabled)
    }

    func testTransitionHintRemainsPendingUntilTheItemChanges() {
        var tracker = MusicTransitionHintTracker()
        tracker.issue(.skip)

        let unchanged = nowPlaying(trackID: "track-1")
        XCTAssertEqual(tracker.pendingHint, .skip)
        XCTAssertEqual(tracker.hint, .skip)
        tracker.resolve(previous: unchanged, current: unchanged, appliedHint: .skip)
        XCTAssertEqual(tracker.pendingHint, .skip)

        let changed = nowPlaying(trackID: "track-2")
        tracker.resolve(previous: unchanged, current: changed, appliedHint: .skip)
        XCTAssertNil(tracker.pendingHint)
    }

    func testTransitionHintsQueueAndFailedCommandClearsOnlyItsOwnToken() {
        var tracker = MusicTransitionHintTracker()
        let first = tracker.issue(.skip)
        _ = tracker.issue(.skip)

        tracker.clear(id: first)
        XCTAssertEqual(tracker.pendingHint, .skip)

        let previous = nowPlaying(trackID: "track-1")
        tracker.resolve(previous: previous, current: nowPlaying(trackID: "track-2"), appliedHint: .skip)
        XCTAssertNil(tracker.pendingHint)
    }

    func testClearingNonFrontHintDoesNotRefreshFrontHintAge() {
        var tracker = MusicTransitionHintTracker()
        _ = tracker.issue(.skip)
        let second = tracker.issue(.skip)
        let unchanged = nowPlaying(trackID: "track-1")

        for _ in 0..<4 {
            tracker.resolve(previous: unchanged, current: unchanged, appliedHint: .skip)
        }
        tracker.clear(id: second)
        tracker.resolve(previous: unchanged, current: unchanged, appliedHint: .skip)

        XCTAssertNil(tracker.pendingHint)
    }

    func testTransitionHintExpiresWhenProviderNeverChangesTheItem() {
        var tracker = MusicTransitionHintTracker()
        tracker.issue(.skip, issuedAtMs: 1_000)

        XCTAssertEqual(tracker.hint(atMonotonicMs: 6_000), .skip)
        XCTAssertNil(tracker.hint(atMonotonicMs: 6_001))
        XCTAssertNil(tracker.pendingHint)
    }

    func testTransitionHintExpiresAfterBoundedUnchangedObservations() {
        var tracker = MusicTransitionHintTracker()
        tracker.issue(.skip)

        let unchanged = nowPlaying(trackID: "track-1")
        for _ in 0..<5 {
            tracker.resolve(previous: unchanged, current: unchanged, appliedHint: .skip)
        }

        XCTAssertNil(tracker.pendingHint)
    }

    @MainActor
    func testProviderResetDropsCorrelationWithoutWritingAnEvent() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = MusicIntegrationCoordinator(rideMapState: state)

        let playing = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "apple",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_000,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
        _ = try coordinator.ingest(
            snapshot: playing,
            wallClockAtMs: 1_700_000_000_000,
            clockUncertaintyMs: 5
        )

        coordinator.resetProviderCorrelation()

        _ = try coordinator.ingest(
            snapshot: .init(
                provider: .spotify,
                sessionId: "spotify",
                state: .unavailable,
                item: nil,
                positionMilliseconds: nil,
                durationMilliseconds: nil,
                observedAtMs: 1_001,
                capabilities: .init(previous: false, play: false, pause: false, next: false, openProvider: true)
            ),
            wallClockAtMs: 1_700_000_000_001,
            clockUncertaintyMs: 5
        )

        XCTAssertEqual(coordinator.nowPlaying?.provider, .spotify)
        XCTAssertEqual(coordinator.nowPlaying?.state, .unavailable)
        XCTAssertNil(coordinator.lastRecordedSequence)
        XCTAssertEqual(coordinator.recordedEvents.count, 1)
    }

    @MainActor
    func testEnablingHistorySeedsTheCurrentTrackAfterDisabledObservation() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: state)
        let playing = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "apple",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_000,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )

        _ = try coordinator.ingest(
            snapshot: playing,
            wallClockAtMs: 1_700_000_000_000,
            clockUncertaintyMs: 5
        )
        try coordinator.setHistoryPolicy(.humanReadable)
        let nextObservation = MobileMusicSnapshotDto(
            provider: playing.provider,
            sessionId: playing.sessionId,
            state: playing.state,
            item: playing.item,
            positionMilliseconds: playing.positionMilliseconds,
            durationMilliseconds: playing.durationMilliseconds,
            observedAtMs: 1_001,
            capabilities: playing.capabilities
        )
        _ = try coordinator.ingest(
            snapshot: nextObservation,
            wallClockAtMs: 1_700_000_000_001,
            clockUncertaintyMs: 5
        )

        XCTAssertEqual(coordinator.recordedEvents.count, 1)
    }

    func testTransitionHintCanBeClearedWithoutIssuingAnEmptyCommand() {
        var tracker = MusicTransitionHintTracker()
        tracker.issue(.skip)

        tracker.clear()

        XCTAssertNil(tracker.pendingHint)
    }

    func testTransitionHintClearsWhenProviderLosesItsCurrentItem() {
        var tracker = MusicTransitionHintTracker()
        tracker.issue(.skip)

        tracker.resolve(
            previous: nowPlaying(trackID: "track-1"),
            current: MusicNowPlaying(
                provider: .appleMusic,
                state: .stopped,
                item: nil,
                capabilities: .init(
                    previous: false,
                    play: true,
                    pause: false,
                    next: false,
                    openProvider: true
                )
            ),
            appliedHint: .skip
        )

        XCTAssertNil(tracker.pendingHint)
    }

    func testTransitionHintClearsOnTerminalStateBeforeLaterItemChange() {
        var tracker = MusicTransitionHintTracker()
        tracker.issue(.skip)

        let previous = nowPlaying(trackID: "track-1")
        tracker.resolve(
            previous: previous,
            current: MusicNowPlaying(
                provider: .appleMusic,
                state: .stopped,
                item: previous.item,
                capabilities: .init(
                    previous: false,
                    play: true,
                    pause: false,
                    next: false,
                    openProvider: true
                )
            ),
            appliedHint: .skip
        )
        XCTAssertNil(tracker.pendingHint)

        tracker.resolve(
            previous: previous,
            current: nowPlaying(trackID: "track-2"),
            appliedHint: .skip
        )
        XCTAssertNil(tracker.pendingHint)
    }

    func testMusicMonitorGenerationInvalidatesOlderTasks() {
        var generation = MusicMonitorGeneration()
        let first = generation.begin()

        XCTAssertTrue(generation.owns(first))

        generation.invalidate()

        XCTAssertFalse(generation.owns(first))
        let second = generation.begin()
        XCTAssertTrue(generation.owns(second))
        XCTAssertFalse(generation.owns(first))
    }

    func testMusicAccessibilityAnnouncementsDeduplicateProjectedState() {
        var tracker = MusicAccessibilityAnnouncementTracker()
        let playing = nowPlaying(trackID: "track-1")

        XCTAssertEqual(tracker.next(for: playing), playing.accessibilitySummary)
        XCTAssertNil(tracker.next(for: playing))

        let paused = MusicNowPlaying(
            provider: .appleMusic,
            state: .paused,
            item: playing.item,
            capabilities: .init(
                previous: true,
                play: true,
                pause: false,
                next: true,
                openProvider: true
            )
        )
        XCTAssertEqual(tracker.next(for: paused), paused.accessibilitySummary)
        XCTAssertNil(tracker.next(for: paused))

        let pausedWithDifferentCapabilities = MusicNowPlaying(
            provider: .appleMusic,
            state: .paused,
            item: paused.item,
            capabilities: .init(
                previous: false,
                play: true,
                pause: false,
                next: false,
                openProvider: true
            )
        )
        XCTAssertNil(tracker.next(for: pausedWithDifferentCapabilities))
    }

    func testArtworkCacheReusesOnlyBoundedArtworkForTheSameItem() {
        var cache = MusicArtworkCache()
        var loadCount = 0
        let artwork = MusicArtwork(data: Data([1, 2, 3]))

        let first = cache.artwork(for: "track-1") {
            loadCount += 1
            return artwork
        }
        let second = cache.artwork(for: "track-1") {
            loadCount += 1
            return artwork
        }

        XCTAssertEqual(first, artwork)
        XCTAssertEqual(second, artwork)
        XCTAssertEqual(loadCount, 1)

        _ = cache.artwork(for: "track-2") {
            loadCount += 1
            return nil
        }
        XCTAssertEqual(loadCount, 2)
        XCTAssertNil(cache.artwork(for: nil) { loadCount += 1; return artwork })
        XCTAssertEqual(loadCount, 2)
    }

    @MainActor
    func testProviderArtworkReachesPresentationOnlyNowPlaying() throws {
        let rideMapState = MobileRideMapState()
        _ = try rideMapState.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMapState)
        try coordinator.setHistoryPolicy(.humanReadable)
        let snapshot = MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: "spotify-app-remote",
            state: .playing,
            item: .init(identifier: "spotify:track:artwork", title: "Track", artist: "Artist"),
            positionMilliseconds: 1,
            durationMilliseconds: 2,
            observedAtMs: 100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
        let artwork = Data([1, 2, 3])

        _ = try coordinator.ingest(
            observation: MusicProviderObservation(snapshot: snapshot, artworkData: artwork),
            wallClockAtMs: 1_700_000_000_000,
            clockUncertaintyMs: 1
        )

        XCTAssertEqual(coordinator.nowPlaying?.artwork?.data, artwork)

        let updatedSnapshot = MobileMusicSnapshotDto(
            provider: snapshot.provider,
            sessionId: snapshot.sessionId,
            state: snapshot.state,
            item: snapshot.item,
            positionMilliseconds: 2,
            durationMilliseconds: snapshot.durationMilliseconds,
            observedAtMs: 101,
            capabilities: snapshot.capabilities
        )
        let updatedArtwork = Data([4, 5, 6])
        let eventCount = coordinator.recordedEvents.count

        _ = try coordinator.ingest(
            observation: MusicProviderObservation(snapshot: updatedSnapshot, artworkData: updatedArtwork),
            wallClockAtMs: 1_700_000_000_100,
            clockUncertaintyMs: 1
        )

        XCTAssertEqual(coordinator.nowPlaying?.artwork?.data, updatedArtwork)
        XCTAssertEqual(coordinator.recordedEvents.count, eventCount)
    }

    private func nowPlaying(trackID: String) -> MusicNowPlaying {
        MusicNowPlaying(
            provider: .appleMusic,
            state: .playing,
            item: MobileMusicItemDto(identifier: trackID, title: trackID, artist: "Artist"),
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
    }

    func testNowPlayingProjectsPlayPauseAndMetadata() {
        let snapshot = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "system-music-player",
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: "Song",
                artist: "Artist"
            ),
            positionMilliseconds: 10,
            durationMilliseconds: 100,
            observedAtMs: 1_000,
            capabilities: MobileMusicCapabilitiesDto(
                previous: true,
                play: false,
                pause: true,
                next: true,
                openProvider: true
            )
        )

        let nowPlaying = MusicNowPlaying(snapshot: snapshot)

        XCTAssertEqual(nowPlaying.title, "Song")
        XCTAssertEqual(nowPlaying.artist, "Artist")
        XCTAssertEqual(nowPlaying.playPauseCommand, .pause)
        XCTAssertTrue(nowPlaying.supports(.next))
    }

    func testStaleProjectionPreservesMetadataButDisablesTransport() {
        let playing = nowPlaying(trackID: "track-1")

        let stale = playing.staleProjection

        XCTAssertEqual(stale.state, .stale)
        XCTAssertEqual(stale.item, playing.item)
        XCTAssertEqual(stale.artwork, playing.artwork)
        XCTAssertTrue(stale.isCommandAvailable(.openProvider))
        XCTAssertFalse(stale.isCommandAvailable(.previous))
        XCTAssertFalse(stale.isCommandAvailable(.pause))
        XCTAssertFalse(stale.isCommandAvailable(.next))
        XCTAssertTrue(stale.availableTransportCommands.isEmpty)
    }

    func testStaleStateRejectsRetainedSkipCapabilities() {
        let stale = MusicNowPlaying(
            provider: .spotify,
            state: .stale,
            item: .init(identifier: "track-1", title: "Song", artist: "Artist"),
            capabilities: .init(
                previous: true,
                play: false,
                pause: false,
                next: true,
                openProvider: true
            )
        )

        XCTAssertFalse(stale.isCommandAvailable(.previous))
        XCTAssertFalse(stale.isCommandAvailable(.next))
        XCTAssertTrue(stale.isCommandAvailable(.openProvider))
    }

    @MainActor
    func testCoordinatorRejectsMalformedExplicitRecordBeforeProjectingIt() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = MusicIntegrationCoordinator(rideMapState: state)
        let malformed = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: String(repeating: "é", count: 257),
                artist: "Artist"
            ),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )

        XCTAssertThrowsError(
            try coordinator.record(
                snapshot: malformed,
                kind: .itemChanged,
                monotonicAtMs: 1_100,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            )
        )
        XCTAssertNil(coordinator.nowPlaying)
    }

    @MainActor
    func testCoordinatorRejectsImpossiblePlaybackPositionBeforeProjectingIt() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = MusicIntegrationCoordinator(rideMapState: state)
        let invalid = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: 101,
            durationMilliseconds: 100,
            observedAtMs: 1_100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )

        XCTAssertNil(
            try coordinator.ingest(
                snapshot: invalid,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            )
        )
        XCTAssertNil(coordinator.nowPlaying)
    }

    @MainActor
    func testCoordinatorRejectsOversizedProviderMetadataBeforeProjectingIt() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = MusicIntegrationCoordinator(rideMapState: state)

        let valid = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: valid,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            ),
            .recorded
        )

        let invalid = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-2",
                title: String(repeating: "é", count: 257),
                artist: "Artist"
            ),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_200,
            capabilities: valid.capabilities
        )
        XCTAssertNil(
            try coordinator.ingest(
                snapshot: invalid,
                wallClockAtMs: 1_700_000_000_200,
                clockUncertaintyMs: 5
            )
        )
        XCTAssertEqual(coordinator.nowPlaying?.item?.identifier, "track-1")
        XCTAssertEqual(coordinator.nowPlaying?.item?.title, "Song")
    }

    @MainActor
    func testCoordinatorNormalizesBlankOptionalMetadataLikeRust() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = MusicIntegrationCoordinator(rideMapState: state)
        let snapshot = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: " ", artist: ""),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_100,
            capabilities: .init(previous: false, play: false, pause: true, next: true, openProvider: true)
        )

        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            ),
            .recorded
        )
        XCTAssertNil(coordinator.nowPlaying?.item?.title)
        XCTAssertNil(coordinator.nowPlaying?.item?.artist)
        XCTAssertNil(state.currentMusicEvents().first?.title)
        XCTAssertNil(state.currentMusicEvents().first?.artist)
    }

    func testNowPlayingExposesOnlySupportedTransportCommands() {
        let nowPlaying = MusicNowPlaying(
            provider: .appleMusic,
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: "Song",
                artist: "Artist"
            ),
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: true,
                pause: true,
                next: false,
                openProvider: true
            )
        )

        XCTAssertEqual(nowPlaying.availableTransportCommands, [.pause])
        XCTAssertFalse(nowPlaying.isCommandAvailable(.play))
        XCTAssertTrue(nowPlaying.isCommandAvailable(.pause))
        XCTAssertTrue(nowPlaying.isCommandAvailable(.openProvider))
    }

    func testMusicTimeConversionRejectsInvalidProviderValues() {
        XCTAssertEqual(MusicTimeConversion.milliseconds(1.5), 1_500)
        XCTAssertNil(MusicTimeConversion.milliseconds(-1))
        XCTAssertNil(MusicTimeConversion.milliseconds(.nan))
        XCTAssertNil(MusicTimeConversion.milliseconds(.infinity))
        XCTAssertNil(MusicTimeConversion.milliseconds(.greatestFiniteMagnitude))
        XCTAssertNil(MusicTimeConversion.milliseconds(Double(UInt64.max) / 1_000))
    }

    func testMusicTimelineIDsUseTheRustEventSequence() {
        func event(sequence: UInt64) -> MobileMusicRideEventDto {
            MobileMusicRideEventDto(
                sequence: sequence,
                provider: .appleMusic,
                itemIdentifier: "track-1",
                title: "Song",
                artist: "Artist",
                kind: .play,
                observedAtMs: 1_000,
                monotonicAtMs: 1_000,
                wallClockAtMs: 1_700_000_000_000,
                clockUncertaintyMs: 5
            )
        }

        XCTAssertNotEqual(event(sequence: 0).timelineID, event(sequence: 1).timelineID)
    }

    func testNowPlayingProvidesLocalizedArtworkAccessibilityLabel() {
        let nowPlaying = MusicNowPlaying(
            provider: .appleMusic,
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: "Song",
                artist: "Artist"
            )
        )

        XCTAssertEqual(nowPlaying.artworkAccessibilityLabel, pevLocalizedText("music.artwork", "Song"))
    }

    func testArtworkAccessibilityLabelUsesProviderWhenTitleIsUnavailable() {
        let nowPlaying = MusicNowPlaying(
            provider: .spotify,
            state: .unavailable,
            item: MobileMusicItemDto(identifier: "track-1", title: nil, artist: nil)
        )

        XCTAssertEqual(nowPlaying.artworkAccessibilityLabel, pevLocalizedText("music.artwork", "Spotify"))
    }

    @MainActor
    func testCoordinatorClassifiesAcceptedItemSkipSeparatelyFromItemChange() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = MusicIntegrationCoordinator(rideMapState: state)

        func observation(trackID: String, observedAtMs: UInt64) -> MusicProviderObservation {
            MusicProviderObservation(
                snapshot: MobileMusicSnapshotDto(
                    provider: .appleMusic,
                    sessionId: "session",
                    state: .playing,
                    item: MobileMusicItemDto(
                        identifier: trackID,
                        title: trackID,
                        artist: "Artist"
                    ),
                    positionMilliseconds: nil,
                    durationMilliseconds: nil,
                    observedAtMs: observedAtMs,
                    capabilities: MobileMusicCapabilitiesDto(
                        previous: true,
                        play: false,
                        pause: true,
                        next: true,
                        openProvider: true
                    )
                )
            )
        }

        XCTAssertEqual(
            try coordinator.ingest(
                observation: observation(trackID: "track-1", observedAtMs: 1_100),
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            ),
            .recorded
        )
        XCTAssertEqual(
            try coordinator.ingest(
                observation: observation(trackID: "track-2", observedAtMs: 1_200),
                wallClockAtMs: 1_700_000_000_200,
                clockUncertaintyMs: 5,
                transitionHint: .skip
            ),
            .recorded
        )
        XCTAssertEqual(coordinator.recordedEvents.map(\.kind), [.itemChanged, .skip])
    }
}
