import CutoutMobileFFI
import Foundation
import ImageIO

/// Spotify's account playback endpoint remains usable when its local App Remote
/// socket is unavailable. Authorization and refresh stay with the SDK.
actor SpotifyPlaybackAPI {
    enum Failure: Error, Equatable {
        case unauthorized
        case forbidden
        case rateLimited(seconds: Double)
        case unavailable
    }

    typealias Send = @Sendable (URLRequest) async throws -> (Data, HTTPURLResponse)
    typealias ArtworkSend = @Sendable (URLRequest, Int) async throws -> (Data, HTTPURLResponse)
    private let send: Send
    private let sendArtwork: ArtworkSend
    private let monotonicNowMilliseconds: @Sendable () -> UInt64
    private let httpSession = MobileSpotifyHttpSession()

    init(
        send: @escaping Send = SpotifyPlaybackAPI.sendRequest,
        sendArtwork: @escaping ArtworkSend = SpotifyPlaybackAPI.sendArtworkRequest,
        monotonicNowMilliseconds: @escaping @Sendable () -> UInt64 = {
            UInt64(DispatchTime.now().uptimeNanoseconds / 1_000_000)
        }
    ) {
        self.send = send
        self.sendArtwork = sendArtwork
        self.monotonicNowMilliseconds = monotonicNowMilliseconds
    }

    private static let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.urlCache = nil
        configuration.httpCookieStorage = nil
        configuration.timeoutIntervalForRequest = 10
        configuration.timeoutIntervalForResource = 10
        return URLSession(configuration: configuration)
    }()

    private static func sendRequest(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        try await sendBoundedRequest(request, maximumBytes: 1_048_576)
    }

    private static func sendArtworkRequest(_ request: URLRequest, maximumBytes: Int) async throws -> (
        Data, HTTPURLResponse
    ) {
        try await sendBoundedRequest(request, maximumBytes: maximumBytes)
    }

    private static func sendBoundedRequest(_ request: URLRequest, maximumBytes: Int) async throws -> (
        Data, HTTPURLResponse
    ) {
        let (bytes, response) = try await session.bytes(for: request)
        guard let response = response as? HTTPURLResponse else { throw Failure.unavailable }
        guard response.expectedContentLength < 0 || response.expectedContentLength <= Int64(maximumBytes) else {
            throw Failure.unavailable
        }
        var data = Data()
        data.reserveCapacity(min(maximumBytes, max(0, Int(response.expectedContentLength))))
        for try await byte in bytes {
            try Task.checkCancellation()
            guard data.count < maximumBytes else { throw Failure.unavailable }
            data.append(byte)
        }
        return (data, response)
    }

    func playback(accessToken: String) async throws -> SpotifyPlayback? {
        let descriptor: MobileSpotifyHttpRequest
        do {
            descriptor = try httpSession.playbackRequest(nowMs: monotonicNowMilliseconds())
        } catch let error as MobileSpotifyPlaybackError {
            throw Self.failure(for: error)
        }
        guard let data = try await request(descriptor, accessToken: accessToken) else { return nil }
        do {
            return SpotifyPlayback(try MobileSpotifyPlayback.parse(body: data))
        } catch let error as MobileSpotifyPlaybackError {
            throw Self.failure(for: error)
        }
    }

    func perform(_ command: MobileMusicCommandDto, accessToken: String, deviceID: String) async throws {
        let descriptor: MobileSpotifyHttpRequest
        do {
            descriptor = try httpSession.commandRequest(
                command: command, deviceId: deviceID, nowMs: monotonicNowMilliseconds())
        } catch let error as MobileSpotifyPlaybackError {
            throw Self.failure(for: error)
        }
        _ = try await request(descriptor, accessToken: accessToken)
    }

    func artwork(url: URL) async throws -> MusicArtwork? {
        guard url.scheme == "https", url.user == nil, url.password == nil else { return nil }
        let (data, response) = try await sendArtwork(URLRequest(url: url), 1_048_576)
        try Task.checkCancellation()
        guard response.statusCode == 200, data.count <= 1_048_576,
            let source = CGImageSourceCreateWithData(data as CFData, nil),
            let image = CGImageSourceCreateThumbnailAtIndex(
                source, 0,
                [
                    kCGImageSourceCreateThumbnailFromImageAlways: true,
                    kCGImageSourceThumbnailMaxPixelSize: 256,
                    kCGImageSourceCreateThumbnailWithTransform: true,
                ] as CFDictionary)
        else { return nil }
        return MusicArtwork(image: image)
    }

    private func request(_ descriptor: MobileSpotifyHttpRequest, accessToken: String) async throws -> Data? {
        guard let url = URL(string: descriptor.url) else { throw Failure.unavailable }
        var request = URLRequest(url: url)
        request.httpMethod = descriptor.method
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await send(request)
        try Task.checkCancellation()
        do {
            let hasBody = try httpSession.receiveResponse(
                status: UInt16(exactly: response.statusCode) ?? 0,
                retryAfter: response.value(forHTTPHeaderField: "Retry-After"),
                bodyLength: UInt64(data.count),
                nowMs: monotonicNowMilliseconds())
            return hasBody ? data : nil
        } catch let error as MobileSpotifyPlaybackError {
            throw Self.failure(for: error)
        }
    }

    private static func failure(for error: MobileSpotifyPlaybackError) -> Failure {
        switch error {
        case .InvalidPayload: .unavailable
        case .Unavailable: .unavailable
        case .Unauthorized: .unauthorized
        case .Forbidden: .forbidden
        case .RateLimited(let retryAfterMilliseconds):
            .rateLimited(seconds: Double(retryAfterMilliseconds) / 1_000)
        }
    }
}

struct SpotifyPlayback: Sendable {
    struct Device: Sendable { let id: String? }
    struct Item: Sendable {
        let uri: String?
        let name: String?
        let artist: String?
    }

    private let value: MobileSpotifyPlayback

    init(_ value: MobileSpotifyPlayback) { self.value = value }

    var device: Device? { value.deviceId().map(Device.init(id:)) }
    var item: Item? {
        guard value.itemIdentifier() != nil,
            let item = value.snapshot(state: .playing, observedAtMs: 0).item
        else { return nil }
        return Item(uri: item.identifier, name: item.title, artist: item.artist)
    }
    var isPlaying: Bool { value.isPlaying() }
    var artworkURL: URL? { value.artworkUrl().flatMap(URL.init(string:)) }
    var withoutActiveDevice: Self { Self(value.withoutActiveDevice()) }

    func snapshot(state: MobileMusicPlaybackStateDto, observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        value.snapshot(state: state, observedAtMs: observedAtMs)
    }
}
