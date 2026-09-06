import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class MusicIntegrationTests: XCTestCase {
    func testProviderMonitoringModeMatchesSupportedLifecycle() {
        XCTAssertEqual(
            MobileMusicProviderDto.appleMusic.monitoringMode,
            .appleMusicSystemPlayer
        )
        XCTAssertEqual(
            MobileMusicProviderDto.spotify.monitoringMode,
            .unavailable
        )
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
                play: false,
                pause: true,
                next: false,
                openProvider: true
            )
        )

        XCTAssertEqual(nowPlaying.availableTransportCommands, [.pause])
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

        XCTAssertEqual(nowPlaying.artworkAccessibilityLabel, "Artwork for Song")
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
