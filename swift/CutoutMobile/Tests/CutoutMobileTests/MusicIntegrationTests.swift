import CutoutMobile
import CutoutMobileFFI
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

    func testSpotifyAdapterIsExplicitlyMetadataOnlyUntilAppRemoteIsProven() {
        let snapshot = SpotifyProviderAdapter().unavailableSnapshot(observedAtMs: 42)

        XCTAssertEqual(snapshot.provider, .spotify)
        XCTAssertEqual(snapshot.state, .unavailable)
        XCTAssertTrue(snapshot.capabilities.openProvider)
        XCTAssertFalse(snapshot.capabilities.play)
        XCTAssertNil(snapshot.item)
    }

    private func snapshot() -> MobileMusicSnapshotDto {
        MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session-1",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Track", artist: "Artist"),
            positionMilliseconds: 5,
            durationMilliseconds: 100,
            observedAtMs: 10,
            capabilities: MobileMusicCapabilitiesDto(
                previous: true,
                play: false,
                pause: true,
                next: true,
                openProvider: false
            )
        )
    }
}
