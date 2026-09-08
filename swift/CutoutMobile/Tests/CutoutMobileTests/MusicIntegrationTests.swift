import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class MusicIntegrationTests: XCTestCase {
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
