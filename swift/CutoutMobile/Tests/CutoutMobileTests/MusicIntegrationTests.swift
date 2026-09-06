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

    func testTransitionHintCanBeClearedWithoutIssuingAnEmptyCommand() {
        var tracker = MusicTransitionHintTracker()
        tracker.issue(.skip)

        tracker.clear()

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
