import CutoutMobileFFI
import XCTest

@testable import CutoutMobile

final class SoundCloudNativePlayerTests: XCTestCase {
    @MainActor
    func testDelayedActivationFailureDoesNotFailPausedTrack() async throws {
        let core = try pausedCore()
        let audio = DelayedAudioSession()
        let player = SoundCloudNativePlayer(core: core) { try await audio.update($0) }
        player.command(.play)
        await audio.waitForActivation()
        player.command(.pause)
        await audio.finishActivation(failing: true)
        _ = await player.audioSessionTask?.value
        XCTAssertEqual(player.snapshot.state, .paused)
        XCTAssertNil(player.errorText)
    }

    @MainActor
    func testStopWaitsForActivationBeforeReleasingSessionAndRejectsLateFailure() async throws {
        let core = try pausedCore()
        let audio = DelayedAudioSession()
        let player = SoundCloudNativePlayer(core: core) { try await audio.update($0) }
        player.command(.play)
        await audio.waitForActivation()
        player.stop()
        XCTAssertEqual(player.snapshot.state, .idle)
        await audio.finishActivation(failing: true)
        _ = await player.audioSessionTask?.value
        let requests = await audio.requests
        XCTAssertEqual(requests, [true])
        XCTAssertNil(player.errorText)
        XCTAssertNil(player.snapshot.track)
    }

    @MainActor
    func testCurrentActivationFailurePublishesRetryState() async throws {
        let core = try pausedCore()
        let audio = DelayedAudioSession()
        let player = SoundCloudNativePlayer(core: core) { try await audio.update($0) }
        player.command(.play)
        await audio.waitForActivation()
        await audio.finishActivation(failing: true)
        _ = await player.audioSessionTask?.value
        XCTAssertEqual(player.snapshot.state, .failed)
        XCTAssertNotNil(player.errorText)
    }

    @MainActor
    func testNewPlaybackWaitsForOldActivationAndStopDeactivation() async throws {
        let core = try pausedCore()
        let audio = DelayedAudioSession()
        let player = SoundCloudNativePlayer(core: core) { try await audio.update($0) }
        player.command(.play)
        await audio.waitForActivation()
        player.stop()
        _ = try pausedCore(core)
        player.command(.play)
        await audio.finishActivation(failing: false)
        await audio.waitForActivation()
        let requests = await audio.requests
        XCTAssertEqual(requests, [true, false, true])
        await audio.finishActivation(failing: false)
        _ = await player.audioSessionTask?.value
        XCTAssertEqual(player.snapshot.state, .playing)
        XCTAssertNil(player.errorText)
    }

    @MainActor
    func testUnusedPlayerStopDoesNotDeactivateAnotherProvidersAudioSession() async {
        let audio = DelayedAudioSession()
        let player = SoundCloudNativePlayer(core: MobileSoundCloudPlayer()) { try await audio.update($0) }
        player.stop()
        _ = await player.audioSessionTask?.value
        let requests = await audio.requests
        XCTAssertTrue(requests.isEmpty)
    }

    @MainActor
    func testPauseBeforeActivationThenStopDoesNotReleaseAnotherSession() async throws {
        let audio = DelayedAudioSession()
        let player = SoundCloudNativePlayer(core: try pausedCore()) { try await audio.update($0) }
        player.command(.play)
        player.command(.pause)
        player.stop()
        _ = await player.audioSessionTask?.value
        let requests = await audio.requests
        XCTAssertTrue(requests.isEmpty)
    }

    @MainActor
    func testStopReleasesSuccessfullyActivatedSessionAfterCancelledPlay() async throws {
        let audio = DelayedAudioSession()
        let player = SoundCloudNativePlayer(core: try pausedCore()) { try await audio.update($0) }
        player.command(.play)
        await audio.waitForActivation()
        player.command(.pause)
        player.stop()
        await audio.finishActivation(failing: false)
        _ = await player.audioSessionTask?.value
        let requests = await audio.requests
        XCTAssertEqual(requests, [true, false])
        XCTAssertEqual(player.snapshot.state, .idle)
    }

    private func pausedCore(_ core: MobileSoundCloudPlayer = MobileSoundCloudPlayer()) throws -> MobileSoundCloudPlayer
    {
        let response =
            #"{"collection":[{"urn":"soundcloud:tracks:42","title":"Ride song","duration":120000,"permalink_url":"https://soundcloud.com/artist/song","user":{"username":"Uploader"},"access":"playable","streamable":true}]}"#
        let tracks = try core.acceptCatalogue(json: Data(response.utf8))
        guard case .fetch(let id, _) = try core.select(urn: tracks[0].urn) else {
            throw DelayedAudioSession.Failure.activation
        }
        _ = try core.streamResolved(
            id: id, url: "https://playback.media-streaming.soundcloud.cloud/manifest.m3u8?token=session")
        _ = core.playerReady(id: id)
        _ = try core.command(command: .pause)
        return core
    }

    @MainActor
    func testStopClearsCancelledSearchBeforeItCanPublishFailure() async {
        let player = SoundCloudNativePlayer()
        player.search("")
        XCTAssertTrue(player.isSearching)
        player.stop()
        XCTAssertFalse(player.isSearching)
        for _ in 0..<3 { await Task.yield() }
        XCTAssertNil(player.errorText)
        XCTAssertTrue(player.tracks.isEmpty)
        XCTAssertNil(player.snapshot.track)
    }

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

private actor DelayedAudioSession {
    enum Failure: Error { case activation }
    private(set) var requests: [Bool] = []
    private var activation: CheckedContinuation<Void, any Error>?
    private var activationStarted: CheckedContinuation<Void, Never>?

    func update(_ active: Bool) async throws {
        requests.append(active)
        guard active else { return }
        try await withCheckedThrowingContinuation {
            activation = $0
            activationStarted?.resume()
            activationStarted = nil
        }
    }

    func waitForActivation() async {
        guard activation == nil else { return }
        await withCheckedContinuation { activationStarted = $0 }
    }

    func finishActivation(failing: Bool) {
        if failing { activation?.resume(throwing: Failure.activation) } else { activation?.resume() }
        activation = nil
    }
}
