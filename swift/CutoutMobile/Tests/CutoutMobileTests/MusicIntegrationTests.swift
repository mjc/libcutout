import CutoutMobile
import CutoutMobileFFI
import Foundation
import XCTest

@MainActor
final class MusicIntegrationTests: XCTestCase {
    func testCoordinatorRecordsOpaqueMetadataThroughRustOwnedRide() throws {
        let rideMap = MobileRideMapState()
        _ = try rideMap.startGpsOnly(atMs: 1, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMap)
        try coordinator.setHistoryPolicy(.opaqueItem)

        let result = try coordinator.record(
            snapshot: snapshot(),
            kind: .itemChanged,
            monotonicAtMs: 10,
            wallClockAtMs: 20,
            clockUncertaintyMs: 3
        )

        XCTAssertEqual(result, .recorded)
        XCTAssertEqual(coordinator.nowPlaying?.title, "Track")
        XCTAssertEqual(coordinator.recordedEvents.count, 1)
        XCTAssertEqual(coordinator.recordedEvents[0].itemIdentifier, "track-1")
        XCTAssertNil(coordinator.recordedEvents[0].title)
        XCTAssertNil(coordinator.recordedEvents[0].artist)
    }

    func testDisabledHistoryStillPublishesNowPlayingWithoutRecording() throws {
        let rideMap = MobileRideMapState()
        _ = try rideMap.startGpsOnly(atMs: 1, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMap)

        let result = try coordinator.record(
            snapshot: snapshot(),
            kind: .play,
            monotonicAtMs: 10,
            wallClockAtMs: 20,
            clockUncertaintyMs: 3
        )

        XCTAssertEqual(result, .disabled)
        XCTAssertEqual(coordinator.nowPlaying?.provider, .appleMusic)
        XCTAssertTrue(coordinator.recordedEvents.isEmpty)
    }

    func testIngestRecordsOnlyMeaningfulPlaybackTransitions() throws {
        let rideMap = MobileRideMapState()
        _ = try rideMap.startGpsOnly(atMs: 1, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMap)
        try coordinator.setHistoryPolicy(.opaqueItem)

        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot(state: .playing, observedAtMs: 10),
                wallClockAtMs: 20,
                clockUncertaintyMs: 1
            ),
            .recorded
        )
        XCTAssertNil(
            try coordinator.ingest(
                snapshot: snapshot(state: .playing, observedAtMs: 20),
                wallClockAtMs: 30,
                clockUncertaintyMs: 1
            )
        )
        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot(state: .paused, observedAtMs: 30),
                wallClockAtMs: 40,
                clockUncertaintyMs: 1
            ),
            .recorded
        )
        XCTAssertEqual(coordinator.recordedEvents.map(\.kind), [.itemChanged, .pause])
    }

    func testCurrentTrackSeedsARideWhenPlaybackStartedBeforeRecording() throws {
        let rideMap = MobileRideMapState()
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMap)

        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot(state: .playing, observedAtMs: 10),
                wallClockAtMs: 20,
                clockUncertaintyMs: 1
            ),
            .disabled
        )

        _ = try rideMap.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        try coordinator.setHistoryPolicy(.opaqueItem)
        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot(state: .playing, observedAtMs: 20),
                wallClockAtMs: 30,
                clockUncertaintyMs: 1
            ),
            .recorded
        )
        XCTAssertEqual(coordinator.recordedEvents.count, 1)
    }

    func testOutOfOrderProviderObservationDoesNotRegressPlayerOrHistory() throws {
        let rideMap = MobileRideMapState()
        _ = try rideMap.startGpsOnly(atMs: 1, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMap)
        try coordinator.setHistoryPolicy(.opaqueItem)

        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot(state: .playing, observedAtMs: 20),
                wallClockAtMs: 30,
                clockUncertaintyMs: 1
            ),
            .recorded
        )

        XCTAssertNil(
            try coordinator.ingest(
                snapshot: snapshot(state: .paused, observedAtMs: 10),
                wallClockAtMs: 40,
                clockUncertaintyMs: 1
            )
        )
        XCTAssertEqual(coordinator.nowPlaying?.state, .playing)
        XCTAssertEqual(coordinator.recordedEvents.map(\.kind), [.itemChanged])
    }

    func testProviderFailureDoesNotBecomeItemChanged() throws {
        let rideMap = MobileRideMapState()
        _ = try rideMap.startGpsOnly(atMs: 1, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMap)
        try coordinator.setHistoryPolicy(.opaqueItem)

        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot(state: .playing, observedAtMs: 10),
                wallClockAtMs: 20,
                clockUncertaintyMs: 1
            ),
            .recorded
        )
        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: SpotifyProviderAdapter().unavailableSnapshot(observedAtMs: 20),
                wallClockAtMs: 30,
                clockUncertaintyMs: 1
            ),
            .recorded
        )
        XCTAssertNil(
            try coordinator.ingest(
                snapshot: SpotifyProviderAdapter().unavailableSnapshot(observedAtMs: 30),
                wallClockAtMs: 40,
                clockUncertaintyMs: 1
            )
        )
        XCTAssertEqual(
            coordinator.recordedEvents.map(\.kind),
            [.itemChanged, .providerDisconnected]
        )
    }

    func testSpotifyAdapterIsExplicitlyMetadataOnlyUntilAppRemoteIsProven() {
        let snapshot = SpotifyProviderAdapter().unavailableSnapshot(observedAtMs: 42)

        XCTAssertEqual(snapshot.provider, .spotify)
        XCTAssertEqual(snapshot.state, .unavailable)
        XCTAssertTrue(snapshot.capabilities.openProvider)
        XCTAssertFalse(snapshot.capabilities.play)
        XCTAssertNil(snapshot.item)
    }

    func testSpotifyAdapterUsesTheProviderDeepLinkForOpenProvider() {
        XCTAssertEqual(SpotifyProviderAdapter.providerURL.absoluteString, "spotify://")
    }

    func testNowPlayingKeepsUnavailableStateVisibleToTheCompactPlayer() {
        let nowPlaying = MusicNowPlaying(snapshot: SpotifyProviderAdapter().unavailableSnapshot(observedAtMs: 42))

        XCTAssertEqual(nowPlaying.title, "Not playing")
        XCTAssertEqual(nowPlaying.statusText, "Unavailable")
        XCTAssertTrue(nowPlaying.capabilities.openProvider)
    }

    func testNowPlayingRefusesCommandsOutsideProviderCapabilities() {
        let nowPlaying = MusicNowPlaying(
            provider: .appleMusic,
            state: .playing,
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: false,
                pause: true,
                next: false,
                openProvider: false
            )
        )

        XCTAssertTrue(nowPlaying.supports(.pause))
        XCTAssertFalse(nowPlaying.supports(.play))
        XCTAssertFalse(nowPlaying.supports(.previous))
        XCTAssertFalse(nowPlaying.supports(.next))
        XCTAssertFalse(nowPlaying.supports(.openProvider))
    }

    func testHistoryPolicyChoicesExplainTheirStorageBoundary() {
        XCTAssertEqual(MobileMusicHistoryPolicyDto.allCases, [.disabled, .opaqueItem, .humanReadable])
        XCTAssertEqual(MobileMusicHistoryPolicyDto.disabled.title, "Don't save")
        XCTAssertTrue(MobileMusicHistoryPolicyDto.opaqueItem.explanation.contains("opaque track identifier"))
        XCTAssertTrue(MobileMusicHistoryPolicyDto.humanReadable.explanation.contains("title"))
    }

    func testProviderObservationFeedsRecordingAndCompactPlayerTogether() throws {
        let rideMap = MobileRideMapState()
        _ = try rideMap.startGpsOnly(atMs: 1, lastConnectedVehicle: nil)
        let coordinator = MusicIntegrationCoordinator(rideMapState: rideMap)
        try coordinator.setHistoryPolicy(.opaqueItem)

        let outcome = try coordinator.ingest(
            observation: MusicProviderObservation(snapshot: snapshot()),
            wallClockAtMs: 20,
            clockUncertaintyMs: 1
        )

        XCTAssertEqual(outcome, .recorded)
        XCTAssertEqual(coordinator.recordedEvents.count, 1)
    }

    func testProviderObservationKeepsBoundedArtworkInSwiftOnly() {
        let artwork = Data([0x01, 0x02, 0x03])
        let observation = MusicProviderObservation(
            snapshot: snapshot(),
            artworkData: artwork
        )

        XCTAssertEqual(observation.artwork?.data, artwork)
        XCTAssertEqual(
            MusicNowPlaying(observation: observation).artwork?.data,
            artwork
        )
    }

    func testProviderObservationRejectsArtworkAboveThePresentationBound() {
        let oversized = Data(repeating: 0x01, count: MusicArtwork.maxBytes + 1)

        let observation = MusicProviderObservation(
            snapshot: snapshot(),
            artworkData: oversized
        )

        XCTAssertNil(observation.artwork)
    }

    func testMusicProviderChoicesRemainExplicitlyBounded() {
        XCTAssertEqual(MobileMusicProviderDto.allCases, [.appleMusic, .spotify])
        XCTAssertEqual(MobileMusicProviderDto.appleMusic.title, "Apple Music")
        XCTAssertEqual(MobileMusicProviderDto.spotify.title, "Spotify")
    }

    func testTimelineProjectionHasStableIdentityAndPrivacyFallback() {
        let event = MobileMusicRideEventDto(
            provider: .appleMusic,
            itemIdentifier: "opaque-track",
            title: nil,
            artist: nil,
            kind: .providerDisconnected,
            monotonicAtMs: 10,
            wallClockAtMs: 20,
            clockUncertaintyMs: 1
        )

        XCTAssertEqual(event.timelineItemTitle, "opaque-track")
        XCTAssertEqual(event.kind.timelineTitle, "Provider disconnected")
        XCTAssertFalse(event.timelineID.isEmpty)

        let redacted = MobileMusicRideEventDto(
            provider: .appleMusic,
            itemIdentifier: nil,
            title: nil,
            artist: nil,
            kind: .pause,
            monotonicAtMs: 11,
            wallClockAtMs: 21,
            clockUncertaintyMs: 1
        )
        XCTAssertEqual(redacted.timelineItemTitle, "No track metadata")
    }

    func testMusicPlayerVisibilityStorePersistsTheHideChoice() {
        let defaults = UserDefaults(suiteName: "music-integration-tests")!
        defaults.removePersistentDomain(forName: "music-integration-tests")
        let store = MusicPlayerVisibilityStore(defaults: defaults)

        XCTAssertFalse(store.isHidden)
        store.setHidden(true)
        XCTAssertTrue(MusicPlayerVisibilityStore(defaults: defaults).isHidden)
        store.setHidden(false)
        XCTAssertFalse(MusicPlayerVisibilityStore(defaults: defaults).isHidden)
    }

    private func snapshot(
        state: MobileMusicPlaybackStateDto = .playing,
        observedAtMs: UInt64 = 10
    ) -> MobileMusicSnapshotDto {
        MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session-1",
            state: state,
            item: MobileMusicItemDto(identifier: "track-1", title: "Track", artist: "Artist"),
            positionMilliseconds: 5,
            durationMilliseconds: 100,
            observedAtMs: observedAtMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: true,
                play: state == .paused,
                pause: state == .playing,
                next: true,
                openProvider: false
            )
        )
    }
}
