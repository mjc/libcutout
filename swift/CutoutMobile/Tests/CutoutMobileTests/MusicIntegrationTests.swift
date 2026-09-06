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
}
