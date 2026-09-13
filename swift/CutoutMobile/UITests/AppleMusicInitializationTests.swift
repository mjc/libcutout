import MediaPlayer
import MusicKit
import XCTest
@testable import CutoutMobile

@MainActor
final class AppleMusicInitializationTests: XCTestCase {
    func testCreatingAndReleasingUnusedAdapterDoesNotCreatePlayers() {
        let factories = PlayerFactories()
        var adapter: AppleMusicProviderAdapter? = factories.makeAdapter()
        weak let releasedAdapter = adapter
        XCTAssertEqual(factories.mediaPlayerCalls, 0)
        XCTAssertEqual(factories.musicKitCalls, 0)

        adapter = nil

        XCTAssertNil(releasedAdapter)
        XCTAssertEqual(factories.mediaPlayerCalls, 0)
        XCTAssertEqual(factories.musicKitCalls, 0)
    }

    func testUnauthorizedSnapshotAndInactiveStopDoNotCreatePlayers() {
        let factories = PlayerFactories()
        let adapter = factories.makeAdapter()

        let snapshot = adapter.unauthorizedSnapshot(observedAtMs: 123)
        adapter.stopMonitoring()
        adapter.stopMonitoring()

        XCTAssertEqual(snapshot.state, .unauthorized)
        XCTAssertEqual(snapshot.observedAtMs, 123)
        XCTAssertEqual(factories.mediaPlayerCalls, 0)
        XCTAssertEqual(factories.musicKitCalls, 0)
    }

    func testMonitoringReusesMetadataPlayerWithoutCreatingTransportPlayer() {
        let factories = PlayerFactories()
        let adapter = factories.makeAdapter()

        adapter.startMonitoring(onChange: {})
        adapter.stopMonitoring()
        adapter.startMonitoring(onChange: {})
        adapter.stopMonitoring()

        XCTAssertEqual(factories.mediaPlayerCalls, 1)
        XCTAssertEqual(factories.musicKitCalls, 0)
    }
}

@MainActor
private final class PlayerFactories {
    var mediaPlayerCalls = 0
    var musicKitCalls = 0

    func makeAdapter() -> AppleMusicProviderAdapter {
        AppleMusicProviderAdapter(
            makePlayer: {
                self.mediaPlayerCalls += 1
                return .systemMusicPlayer
            },
            makeSystemPlayer: {
                self.musicKitCalls += 1
                return .shared
            }
        )
    }
}
