import CutoutMobileFFI
import Foundation
import XCTest
@testable import CutoutMobile

final class SpotifyPlaybackAPITests: XCTestCase {
    private static let track = Data(#"{"device":{"id":"phone","is_restricted":false},"is_playing":true,"progress_ms":1234,"item":{"type":"track","uri":"spotify:track:abc","name":"A track","artists":[{"name":"An artist"}],"duration_ms":90000},"actions":{"disallows":{"skipping_prev":true}}}"#.utf8)

    func testPlaybackReadsMetadataWithoutAPlaybackCommand() async throws {
        let api = SpotifyPlaybackAPI { request in
            XCTAssertEqual(request.httpMethod, "GET")
            XCTAssertEqual(request.url?.path, "/v1/me/player")
            XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer test-token")
            XCTAssertNil(request.httpBody)
            XCTAssertEqual(URLComponents(url: request.url!, resolvingAgainstBaseURL: false)?.queryItems,
                           [URLQueryItem(name: "additional_types", value: "track,episode")])
            return (Self.track, HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!)
        }
        let playback = try await api.playback(accessToken: "test-token")
        let snapshot = try XCTUnwrap(playback).snapshot(state: .playing, observedAtMs: 10)
        XCTAssertEqual(snapshot.item?.title, "A track")
        XCTAssertEqual(snapshot.item?.artist, "An artist")
        XCTAssertEqual(snapshot.positionMilliseconds, 1234)
        XCTAssertEqual(snapshot.durationMilliseconds, 90000)
        XCTAssertFalse(snapshot.capabilities.previous)
        XCTAssertTrue(snapshot.capabilities.next)
        XCTAssertTrue(snapshot.capabilities.pause)
        XCTAssertFalse(snapshot.capabilities.play)
        let stale = try XCTUnwrap(playback).snapshot(state: .stale, observedAtMs: 20)
        XCTAssertFalse(stale.capabilities.next)
        XCTAssertFalse(stale.capabilities.pause)
    }

    func testEpisodeUsesShowAndKeepsNullableProgressUnknown() throws {
        let body = Data(#"{"device":{"id":"phone"},"is_playing":false,"progress_ms":null,"item":{"type":"episode","uri":"spotify:episode:abc","name":"Episode","show":{"name":"Podcast"},"duration_ms":7200000}}"#.utf8)
        let playback = try JSONDecoder().decode(SpotifyPlayback.self, from: body)
        let snapshot = playback.snapshot(state: .paused, observedAtMs: 10)
        XCTAssertEqual(snapshot.item?.artist, "Podcast")
        XCTAssertEqual(snapshot.durationMilliseconds, 7_200_000)
        XCTAssertNil(snapshot.positionMilliseconds)
        XCTAssertTrue(snapshot.capabilities.play)
    }

    func testNoActivePlaybackDoesNotInventATrackOrStartPlayback() async throws {
        let api = SpotifyPlaybackAPI { request in
            XCTAssertEqual(request.httpMethod, "GET")
            return (Data(), HTTPURLResponse(url: request.url!, statusCode: 204, httpVersion: nil, headerFields: nil)!)
        }
        let playback = try await api.playback(accessToken: "test-token")
        XCTAssertNil(playback)
    }

    func testRestrictedDeviceAndUnknownContentHaveNoInventedMetadata() throws {
        let body = Data(#"{"device":{"id":"phone","is_restricted":true},"is_playing":true,"item":{"type":"ad","uri":"spotify:ad:abc","name":"Ad"}}"#.utf8)
        let snapshot = try JSONDecoder().decode(SpotifyPlayback.self, from: body).snapshot(state: .playing, observedAtMs: 1)
        XCTAssertNil(snapshot.item)
        XCTAssertFalse(snapshot.capabilities.next)
        XCTAssertFalse(snapshot.capabilities.pause)
    }

    func testCommandsTargetTheObservedDeviceAndNeverTransferPlayback() async throws {
        for (command, method, path) in [(MobileMusicCommandDto.play, "PUT", "play"), (.pause, "PUT", "pause"),
                                        (.next, "POST", "next"), (.previous, "POST", "previous")] {
            let api = SpotifyPlaybackAPI { request in
                XCTAssertEqual(request.httpMethod, method)
                XCTAssertEqual(request.url?.path, "/v1/me/player/\(path)")
                XCTAssertEqual(URLComponents(url: request.url!, resolvingAgainstBaseURL: false)?.queryItems,
                               [URLQueryItem(name: "device_id", value: "phone&space value")])
                XCTAssertNil(request.httpBody)
                return (Data(), HTTPURLResponse(url: request.url!, statusCode: 204, httpVersion: nil, headerFields: nil)!)
            }
            try await api.perform(command, accessToken: "test-token", deviceID: "phone&space value")
        }
    }

    func testArtworkRejectsBodiesOverTheBoundedReceiveLimit() async throws {
        let api = SpotifyPlaybackAPI(
            send: { _ in throw SpotifyPlaybackAPI.Failure.unavailable },
            sendArtwork: { request, maximumBytes in
                XCTAssertEqual(maximumBytes, 1_048_576)
                let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
                return (Data(count: maximumBytes + 1), response)
            }
        )
        let artwork = try await api.artwork(url: URL(string: "https://images.example/artwork")!)
        XCTAssertNil(artwork)
    }

    func testHTTPFailuresDistinguishAccessExpiryPermissionsAndRetryAfter() async throws {
        for (status, expected) in [(401, SpotifyPlaybackAPI.Failure.unauthorized), (403, .forbidden),
                                   (429, .rateLimited(seconds: 45)), (500, .unavailable)] {
            let api = SpotifyPlaybackAPI { request in
                (Data(), HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil,
                                        headerFields: ["Retry-After": "45"])!)
            }
            do {
                _ = try await api.playback(accessToken: "test-token")
                XCTFail("Expected HTTP failure")
            } catch {
                XCTAssertEqual(error as? SpotifyPlaybackAPI.Failure, expected)
            }
        }
    }

    func testCancelledRequestCannotReturnPlayback() async throws {
        let api = SpotifyPlaybackAPI { request in
            // Simulate a transport that still completes after cancellation.
            try? await Task.sleep(for: .milliseconds(50))
            return (Self.track, HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!)
        }
        let task = Task { try await api.playback(accessToken: "test-token") }
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("Cancelled playback must be discarded")
        } catch is CancellationError {
        }
    }

    func testRateLimitAlsoBlocksCommandsWithoutRetryingThem() async throws {
        let transport = RateLimitedTransport()
        let api = SpotifyPlaybackAPI(send: transport.send)
        _ = try? await api.playback(accessToken: "test-token")
        do {
            try await api.perform(.next, accessToken: "test-token", deviceID: "phone")
            XCTFail("A rate-limited command must not be sent or queued")
        } catch let SpotifyPlaybackAPI.Failure.rateLimited(seconds) {
            XCTAssertGreaterThan(seconds, 0)
            XCTAssertLessThanOrEqual(seconds, 45)
        }
        let requests = await transport.count
        XCTAssertEqual(requests, 1)
    }
}

private actor RateLimitedTransport {
    var count = 0
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        count += 1
        return (Data(), HTTPURLResponse(url: request.url!, statusCode: 429, httpVersion: nil,
                                       headerFields: ["Retry-After": "45"])!)
    }
}
