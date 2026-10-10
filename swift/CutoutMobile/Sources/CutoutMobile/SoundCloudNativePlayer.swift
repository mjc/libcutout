import AVFoundation
import CutoutMobileFFI
import Foundation
import MediaPlayer
import Observation

private final class SoundCloudNoRedirectDelegate: NSObject, URLSessionTaskDelegate {
    func urlSession(
        _: URLSession, task _: URLSessionTask,
        willPerformHTTPRedirection _: HTTPURLResponse, newRequest _: URLRequest,
        completionHandler: @escaping (URLRequest?) -> Void
    ) {
        // OAuth is used only on the fixed API host, never forwarded to media/CDN hosts.
        completionHandler(nil)
    }
}

/// Executes Rust-admitted HTTP and audio effects. No audio or credentials are persisted.
@MainActor
@Observable
public final class SoundCloudNativePlayer {
    public private(set) var tracks: [MobileSoundCloudTrack] = []
    public private(set) var snapshot: MobileSoundCloudSnapshot
    public private(set) var isSearching = false
    public private(set) var errorText: String?
    public private(set) var elapsedSeconds = 0
    @ObservationIgnored private let core: MobileSoundCloudPlayer
    @ObservationIgnored private let player = AVPlayer()
    @ObservationIgnored private let developmentToken: String?
    @ObservationIgnored private let session: URLSession
    @ObservationIgnored private var searchTask: Task<Void, Never>?
    @ObservationIgnored private var streamTask: Task<Void, Never>?
    @ObservationIgnored private var itemObservation: NSKeyValueObservation?
    @ObservationIgnored private var endObservation: NSObjectProtocol?
    @ObservationIgnored private var timeObservation: Any?
    @ObservationIgnored private var remoteTargets: [(MPRemoteCommand, Any)] = []

    public init() {
        let core = MobileSoundCloudPlayer()
        self.core = core
        snapshot = core.snapshot()
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 25
        configuration.urlCache = nil
        session = URLSession(configuration: configuration, delegate: SoundCloudNoRedirectDelegate(), delegateQueue: nil)
        #if DEBUG && targetEnvironment(simulator)
            // Development provisioning only. Release token exchange is tracked in LIBCU-909.
            developmentToken = ProcessInfo.processInfo.environment["CUTOUT_SOUNDCLOUD_ACCESS_TOKEN"]
        #else
            developmentToken = nil
        #endif
        timeObservation = player.addPeriodicTimeObserver(
            forInterval: CMTime(seconds: 1, preferredTimescale: 600), queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                let seconds = player.currentTime().seconds
                guard seconds.isFinite, seconds >= 0 else { return }
                elapsedSeconds = Int(seconds)
                publishNowPlaying()
            }
        }
    }

    public func musicSnapshot(nowMs: UInt64) -> MobileMusicSnapshotDto {
        _ = snapshot  // Observe native changes when projecting the existing compact player.
        return core.musicSnapshot(nowMs: nowMs)
    }

    isolated deinit {
        stop()
        if let timeObservation { player.removeTimeObserver(timeObservation) }
        session.invalidateAndCancel()
    }

    public func search(_ query: String) {
        searchTask?.cancel()
        isSearching = true
        errorText = nil
        searchTask = Task { [weak self] in
            guard let self else { return }
            do {
                let endpoint = try core.searchEndpoint(query: query)
                let (data, response) = try await request(endpoint)
                try requireSuccess(response)
                try Task.checkCancellation()
                tracks = try core.acceptCatalogue(json: data)
                isSearching = false
            } catch {
                guard !Task.isCancelled else { return }
                isSearching = false
                errorText = message(error)
            }
        }
    }

    public func select(_ track: MobileSoundCloudTrack) {
        do { apply(try core.select(urn: track.urn)) } catch { errorText = message(error) }
    }

    public func command(_ command: MobileSoundCloudCommand) {
        do { apply(try core.command(command: command)) } catch { errorText = message(error) }
    }

    public func stop() {
        searchTask?.cancel()
        streamTask?.cancel()
        itemObservation = nil
        removeEndObservation()
        apply(core.stop())
        player.replaceCurrentItem(with: nil)
        elapsedSeconds = 0
        if !remoteTargets.isEmpty {
            for (command, target) in remoteTargets { command.removeTarget(target) }
            remoteTargets.removeAll()
            MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
        }
        #if os(iOS)
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        #endif
    }

    private enum Failure: Error { case notConfigured, expiredToken, invalidResponse, player }

    private func authorization() async throws -> String {
        let now = DispatchTime.now().uptimeNanoseconds / 1_000_000
        if let header = core.authorization(nowMs: now) { return header }
        if let developmentToken, !developmentToken.isEmpty {
            let data = try JSONSerialization.data(withJSONObject: [
                "access_token": developmentToken, "token_type": "bearer", "expires_in": 3600,
            ])
            try core.acceptAuthorization(json: data, nowMs: now)
        } else {
            guard let url = URL(string: core.tokenEndpoint()), url.scheme == "https",
                url.host == "soundcloud.cutout.lol"
            else { throw Failure.notConfigured }
            let (data, response) = try await session.data(from: url)
            guard let response = response as? HTTPURLResponse else { throw Failure.invalidResponse }
            try requireSuccess(response)
            try core.acceptAuthorization(json: data, nowMs: DispatchTime.now().uptimeNanoseconds / 1_000_000)
        }
        guard let header = core.authorization(nowMs: DispatchTime.now().uptimeNanoseconds / 1_000_000)
        else { throw Failure.expiredToken }
        return header
    }

    private func request(_ endpoint: String) async throws -> (Data, HTTPURLResponse) {
        guard let url = URL(string: endpoint), url.scheme == "https", url.host == "api.soundcloud.com" else {
            throw Failure.invalidResponse
        }
        for attempt in 0...1 {
            var request = URLRequest(url: url)
            request.setValue(try await authorization(), forHTTPHeaderField: "Authorization")
            let (data, response) = try await session.data(for: request)
            guard let response = response as? HTTPURLResponse else { throw Failure.invalidResponse }
            if response.statusCode == 401 {
                core.invalidateAuthorization()
                if attempt == 0 { continue }
                throw Failure.expiredToken
            }
            guard data.count <= 512 * 1024 else { throw Failure.invalidResponse }
            return (data, response)
        }
        throw Failure.expiredToken
    }

    private func requireSuccess(_ response: HTTPURLResponse) throws {
        guard response.statusCode == 200 else { throw Failure.invalidResponse }
    }

    private func apply(_ effect: MobileSoundCloudEffect) {
        switch effect {
        case .none: break
        case .pause: player.pause()
        case .play:
            #if os(iOS)
                do {
                    try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
                    try AVAudioSession.sharedInstance().setActive(true)
                } catch {
                    errorText = pevLocalizedText("music.soundcloud.play_failed")
                    apply(core.failed(id: core.snapshot().playbackId))
                    return
                }
            #endif
            player.play()
        case .fetch(let id, let endpoint):
            registerRemoteCommands()
            streamTask?.cancel()
            player.pause()
            itemObservation = nil
            removeEndObservation()
            player.replaceCurrentItem(with: nil)
            elapsedSeconds = 0
            errorText = nil
            streamTask = Task { [weak self] in
                guard let self else { return }
                do {
                    let (data, response) = try await request(endpoint)
                    try requireSuccess(response)
                    let streamEndpoint = try core.streamEndpoint(json: data)
                    let (_, redirect) = try await request(streamEndpoint)
                    guard redirect.statusCode == 302,
                        let mediaURL = redirect.value(forHTTPHeaderField: "Location")
                    else { throw Failure.invalidResponse }
                    try Task.checkCancellation()
                    apply(try core.streamResolved(id: id, url: mediaURL))
                } catch {
                    guard !Task.isCancelled else { return }
                    apply(core.failed(id: id))
                    errorText = message(error)
                }
            }
        case .prepare(let id, let url):
            guard let url = URL(string: url) else {
                apply(core.failed(id: id))
                return
            }
            let item = AVPlayerItem(url: url)
            itemObservation = item.observe(\.status, options: [.initial, .new]) { [weak self] item, _ in
                let status = item.status
                Task { @MainActor [weak self] in
                    guard let self else { return }
                    switch status {
                    case .readyToPlay: apply(core.playerReady(id: id))
                    case .failed:
                        let effect = core.failed(id: id)
                        if case .none = effect { return }
                        apply(effect)
                        errorText = pevLocalizedText("music.soundcloud.play_failed")
                    case .unknown: break
                    @unknown default: break
                    }
                }
            }
            removeEndObservation()
            endObservation = NotificationCenter.default.addObserver(
                forName: AVPlayerItem.didPlayToEndTimeNotification, object: item, queue: .main
            ) { [weak self] _ in
                Task { @MainActor [weak self] in
                    guard let self else { return }
                    do {
                        let effect = try core.ended(id: id)
                        if case .pause = effect { player.seek(to: .zero) }
                        apply(effect)
                    } catch { errorText = message(error) }
                }
            }
            player.replaceCurrentItem(with: item)
        }
        snapshot = core.snapshot()
        publishNowPlaying()
    }

    private func registerRemoteCommands() {
        guard remoteTargets.isEmpty else { return }
        let center = MPRemoteCommandCenter.shared()
        for (remote, command) in [
            (center.playCommand, MobileSoundCloudCommand.play),
            (center.pauseCommand, .pause),
            (center.previousTrackCommand, .previous),
            (center.nextTrackCommand, .next),
        ] {
            let target = remote.addTarget { [weak self] _ in
                Task { @MainActor [weak self] in self?.command(command) }
                return .success
            }
            remoteTargets.append((remote, target))
        }
        let target = center.togglePlayPauseCommand.addTarget { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                command(snapshot.state == .playing || snapshot.state == .loading ? .pause : .play)
            }
            return .success
        }
        remoteTargets.append((center.togglePlayPauseCommand, target))
    }

    private func publishNowPlaying() {
        guard !remoteTargets.isEmpty, let track = snapshot.track else { return }
        let center = MPRemoteCommandCenter.shared()
        center.playCommand.isEnabled = snapshot.state == .paused || snapshot.state == .failed
        center.pauseCommand.isEnabled = snapshot.state == .playing || snapshot.state == .loading
        center.previousTrackCommand.isEnabled = snapshot.hasPrevious
        center.nextTrackCommand.isEnabled = snapshot.hasNext
        MPNowPlayingInfoCenter.default().nowPlayingInfo = [
            MPMediaItemPropertyTitle: track.title,
            MPMediaItemPropertyArtist: track.uploader,
            MPMediaItemPropertyPlaybackDuration: Double(track.durationMs) / 1000,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: Double(elapsedSeconds),
            MPNowPlayingInfoPropertyPlaybackRate: player.rate,
        ]
    }

    private func removeEndObservation() {
        if let endObservation { NotificationCenter.default.removeObserver(endObservation) }
        endObservation = nil
    }

    private func message(_ error: Error) -> String {
        switch error {
        case Failure.notConfigured: pevLocalizedText("music.soundcloud.not_configured")
        case Failure.expiredToken: pevLocalizedText("music.soundcloud.authorization_expired")
        case is URLError: pevLocalizedText("music.soundcloud.network_failed")
        default: pevLocalizedText("music.soundcloud.play_failed")
        }
    }
}
