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
    private var retryAfter: ContinuousClock.Instant?

    init(
        send: @escaping Send = SpotifyPlaybackAPI.sendRequest,
        sendArtwork: @escaping ArtworkSend = SpotifyPlaybackAPI.sendArtworkRequest
    ) {
        self.send = send
        self.sendArtwork = sendArtwork
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
        let (data, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse else { throw Failure.unavailable }
        return (data, response)
    }

    private static func sendArtworkRequest(_ request: URLRequest, maximumBytes: Int) async throws -> (
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
        let data = try await request(
            path: "", method: "GET", accessToken: accessToken,
            query: [URLQueryItem(name: "additional_types", value: "track,episode")])
        guard !data.isEmpty else { return nil }
        return try JSONDecoder().decode(SpotifyPlayback.self, from: data)
    }

    func perform(_ command: MobileMusicCommandDto, accessToken: String, deviceID: String) async throws {
        let path: String
        let method: String
        switch command {
        case .play: (path, method) = ("/play", "PUT")
        case .pause: (path, method) = ("/pause", "PUT")
        case .previous: (path, method) = ("/previous", "POST")
        case .next: (path, method) = ("/next", "POST")
        case .openProvider: throw Failure.unavailable
        }
        _ = try await request(
            path: path, method: method, accessToken: accessToken,
            query: [URLQueryItem(name: "device_id", value: deviceID)])
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

    private func request(path: String, method: String, accessToken: String, query: [URLQueryItem]) async throws -> Data
    {
        if let retryAfter, retryAfter > .now {
            let remaining = ContinuousClock.now.duration(to: retryAfter).components
            throw Failure.rateLimited(seconds: Double(remaining.seconds) + Double(remaining.attoseconds) / 1e18)
        }
        var components = URLComponents(string: "https://api.spotify.com/v1/me/player\(path)")!
        components.queryItems = query
        var request = URLRequest(url: components.url!)
        request.httpMethod = method
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await send(request)
        try Task.checkCancellation()
        switch response.statusCode {
        case 200..<300:
            guard data.count <= 1_048_576 else { throw Failure.unavailable }
            return response.statusCode == 204 ? Data() : data
        case 401: throw Failure.unauthorized
        case 403: throw Failure.forbidden
        case 429:
            let seconds = response.value(forHTTPHeaderField: "Retry-After").flatMap(Double.init) ?? 30
            let delay = seconds.isFinite ? min(max(1, seconds), 86_400) : 30
            retryAfter = .now.advanced(by: .seconds(delay))
            throw Failure.rateLimited(seconds: delay)
        default: throw Failure.unavailable
        }
    }
}

struct SpotifyPlayback: Decodable, Sendable {
    struct Device: Decodable, Sendable {
        let id: String?
        let isRestricted: Bool?
        enum CodingKeys: String, CodingKey {
            case id
            case isRestricted = "is_restricted"
        }
    }

    struct Item: Decodable, Sendable {
        struct Artist: Decodable, Sendable { let name: String }
        struct Show: Decodable, Sendable { let name: String }
        struct Image: Decodable, Sendable {
            let url: URL
            let width: Int?
        }
        struct Album: Decodable, Sendable { let images: [Image]? }
        let uri: String?
        let name: String?
        let type: String?
        let artists: [Artist]?
        let show: Show?
        let album: Album?
        let images: [Image]?
        let durationMs: Int64?
        enum CodingKeys: String, CodingKey {
            case uri, name, type, artists, show, album, images
            case durationMs = "duration_ms"
        }
    }

    struct Actions: Decodable, Sendable { let disallows: [String: Bool]? }
    let device: Device?
    let isPlaying: Bool
    let progressMs: Int64?
    let item: Item?
    let actions: Actions?
    var artworkURL: URL? {
        let images = item?.images ?? item?.album?.images ?? []
        return images.filter { ($0.width ?? 0) >= 256 }.min { ($0.width ?? 0) < ($1.width ?? 0) }?.url
            ?? images.first?.url
    }
    enum CodingKeys: String, CodingKey {
        case device, item, actions
        case isPlaying = "is_playing"
        case progressMs = "progress_ms"
    }

    var withoutActiveDevice: Self {
        Self(device: nil, isPlaying: false, progressMs: progressMs, item: item, actions: nil)
    }

    func snapshot(state: MobileMusicPlaybackStateDto, observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        let knownItem = item.flatMap { item -> MobileMusicItemDto? in
            guard item.type == "track" || item.type == "episode", let uri = item.uri else { return nil }
            return MobileMusicItemDto(
                identifier: uri, title: item.name,
                artist: item.artists?.map(\.name).joined(separator: ", ") ?? item.show?.name)
        }
        let controls =
            (state == .playing || state == .paused)
            && device?.id != nil && device?.isRestricted != true
        func allowed(_ action: String) -> Bool { controls && actions?.disallows?[action] != true }
        return MobileMusicSnapshotDto(
            provider: .spotify, sessionId: "spotify-web-api", state: state, item: knownItem,
            positionMilliseconds: progressMs.flatMap(UInt64.init(exactly:)),
            durationMilliseconds: item?.durationMs.flatMap(UInt64.init(exactly:)),
            observedAtMs: observedAtMs,
            capabilities: .init(
                previous: allowed("skipping_prev"),
                play: state == .paused && allowed("resuming"),
                pause: state == .playing && allowed("pausing"),
                next: allowed("skipping_next"), openProvider: true)
        )
    }
}
