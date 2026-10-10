import CutoutMobileFFI
import XCTest

@testable import CutoutMobile

final class SoundCloudNativePlayerTests: XCTestCase {
    func testNativeQueueProjectsTrackAndLocalControlsIntoExistingPlayer() throws {
        let player = MobileSoundCloudPlayer()
        let response =
            #"{"collection":[{"urn":"soundcloud:tracks:42","title":"Ride song","duration":120000,"permalink_url":"https://soundcloud.com/artist/song","user":{"username":"Uploader"},"access":"playable","streamable":true}]}"#
        let tracks = try player.acceptCatalogue(json: Data(response.utf8))
        XCTAssertEqual(tracks.count, 1)
        guard case .fetch(let id, let endpoint) = try player.select(urn: tracks[0].urn) else {
            return XCTFail("Track selection must acquire an official stream")
        }
        XCTAssertEqual(endpoint, "https://api.soundcloud.com/tracks/soundcloud:tracks:42/streams")
        guard
            case .prepare = try player.streamResolved(
                id: id, url: "https://playback.media-streaming.soundcloud.cloud/manifest.m3u8?token=session"
            )
        else { return XCTFail("Current stream must prepare the platform player") }
        guard case .play = player.playerReady(id: id) else { return XCTFail("Ready selection must play") }
        let playing = MusicNowPlaying(snapshot: player.musicSnapshot(nowMs: 10))
        XCTAssertEqual(playing.title, "Ride song")
        XCTAssertEqual(playing.artist, "Uploader")
        XCTAssertEqual(playing.playPauseCommand, .pause)
        XCTAssertTrue(playing.showsCompactPlayer)
        guard case .pause = try player.command(command: .pause) else { return XCTFail("Pause must be local") }
        let paused = MusicNowPlaying(snapshot: player.musicSnapshot(nowMs: 11))
        XCTAssertEqual(paused.playPauseCommand, .play)
        guard case .play = try player.command(command: .play) else { return XCTFail("Resume must be local") }
        _ = player.failed(id: id)
        let failed = MusicNowPlaying(snapshot: player.musicSnapshot(nowMs: 12))
        XCTAssertTrue(failed.showsCompactPlayer, "A failed native stream must keep the explicit retry control")
        XCTAssertEqual(failed.playPauseCommand, .play)
        _ = player.stop()
        XCTAssertNil(player.snapshot().track)
        XCTAssertEqual(player.snapshot().state, .idle)
    }
}
