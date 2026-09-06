import CutoutMobileFFI
import Foundation
#if canImport(UIKit) && os(iOS)
import UIKit
#endif
#if canImport(MusicKit) && os(iOS)
@preconcurrency import MusicKit
#endif
#if canImport(SpotifyiOS) && os(iOS)
@preconcurrency import SpotifyiOS
#endif

#if canImport(MediaPlayer) && os(iOS)
import MediaPlayer

/// Apple Music's system-player bridge. MusicKit owns transport; MediaPlayer is
/// retained only for the system now-playing metadata/artwork surface. iOS does
/// not provide a system PCM tap for another app's playback.
@MainActor
public final class AppleMusicProviderAdapter {
    public static let providerURL = URL(string: "https://music.apple.com/")!
    private static let artworkSize = CGSize(width: 256, height: 256)
    private let player = MPMusicPlayerController.systemMusicPlayer
#if canImport(MusicKit) && os(iOS)
    private let systemPlayer = SystemMusicPlayer.shared
#endif
    private var notificationTokens = [NSObjectProtocol]()
    private var artworkCache = MusicArtworkCache()

    public init() {}

    /// Starts the system-player callbacks used to refresh bounded metadata.
    /// Polling remains the fallback for position and lifecycle reconciliation.
    public func startMonitoring(onChange: @escaping @MainActor () -> Void) {
        stopMonitoring()
        player.beginGeneratingPlaybackNotifications()
        let center = NotificationCenter.default
        let names: [Notification.Name] = [
            .MPMusicPlayerControllerPlaybackStateDidChange,
            .MPMusicPlayerControllerNowPlayingItemDidChange,
        ]
        notificationTokens = names.map { name in
            center.addObserver(forName: name, object: player, queue: .main) { _ in
                Task { @MainActor in onChange() }
            }
        }
    }

    public func stopMonitoring() {
        let center = NotificationCenter.default
        notificationTokens.forEach(center.removeObserver)
        notificationTokens.removeAll(keepingCapacity: true)
        player.endGeneratingPlaybackNotifications()
    }

    public func requestAuthorization() async -> Bool {
#if canImport(MusicKit) && os(iOS)
        return await MusicAuthorization.request() == .authorized
#else
        return await withCheckedContinuation { continuation in
            MPMediaLibrary.requestAuthorization { status in
                continuation.resume(returning: status == .authorized)
            }
        }
#endif
    }

    public func unauthorizedSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "system-music-player",
            state: .unauthorized,
            item: nil,
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: observedAtMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: false,
                pause: false,
                next: false,
                openProvider: true
            )
        )
    }

    @MainActor
    public func perform(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        switch command {
        case .previous:
#if canImport(MusicKit) && os(iOS)
            do {
                try await systemPlayer.skipToPreviousEntry()
            } catch {
                return .failed
            }
#else
            player.skipToPreviousItem()
#endif
        case .play:
#if canImport(MusicKit) && os(iOS)
            do {
                try await systemPlayer.play()
            } catch {
                return .failed
            }
#else
            player.play()
#endif
        case .pause:
#if canImport(MusicKit) && os(iOS)
            systemPlayer.pause()
#else
            player.pause()
#endif
        case .next:
#if canImport(MusicKit) && os(iOS)
            do {
                try await systemPlayer.skipToNextEntry()
            } catch {
                return .failed
            }
#else
            player.skipToNextItem()
#endif
        case .openProvider:
#if canImport(UIKit) && os(iOS)
            guard UIApplication.shared.canOpenURL(Self.providerURL) else { return .unavailable }
            guard await UIApplication.shared.open(Self.providerURL) else { return .failed }
#else
            return .unavailable
#endif
        }
        return .accepted
    }

    public func snapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        let item = player.nowPlayingItem.map {
            MobileMusicItemDto(
                identifier: String($0.persistentID),
                title: $0.title,
                artist: $0.artist
            )
        }
        let state: MobileMusicPlaybackStateDto = switch player.playbackState {
        case .playing: .playing
        case .paused: .paused
        case .interrupted: .interrupted
        case .stopped: .stopped
        default: .unavailable
        }
        let position = MusicTimeConversion.milliseconds(player.currentPlaybackTime)
        let duration = player.nowPlayingItem.flatMap {
            MusicTimeConversion.milliseconds($0.playbackDuration)
        }
        return MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "system-music-player",
            state: state,
            item: item,
            positionMilliseconds: position,
            durationMilliseconds: duration,
            observedAtMs: observedAtMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: item != nil,
                play: state == .paused || state == .stopped,
                pause: state == .playing,
                next: item != nil,
                openProvider: true
            )
        )
    }

    /// Returns the same bounded provider snapshot plus permitted artwork for
    /// SwiftUI. The artwork bytes never enter the Rust ride contract.
    public func observation(observedAtMs: UInt64) -> MusicProviderObservation {
        MusicProviderObservation(
            snapshot: snapshot(observedAtMs: observedAtMs),
            artworkData: artworkData()
        )
    }

    private func artworkData() -> Data? {
        artworkCache.artwork(for: currentItemIdentifier) {
            loadArtwork()
        }?.data
    }

    private var currentItemIdentifier: String? {
        player.nowPlayingItem.map { String($0.persistentID) }
    }

    private func loadArtwork() -> MusicArtwork? {
#if canImport(UIKit) && os(iOS)
        guard
            let artwork = player.nowPlayingItem?.artwork,
            let image = artwork.image(at: Self.artworkSize)
        else {
            return nil
        }
        return image.jpegData(compressionQuality: 0.8).flatMap(MusicArtwork.init(data:))
#else
        nil
#endif
    }
}
#endif

#if canImport(SpotifyiOS) && os(iOS)

/// Thin main-thread bridge to Spotify's official App Remote SDK. The SDK owns
/// authorization, playback, and provider lifecycle; only bounded projections
/// enter the shared music/Rust pipeline.
@MainActor
public final class SpotifyProviderAdapter: NSObject, @preconcurrency SPTAppRemoteDelegate, @preconcurrency SPTAppRemotePlayerStateDelegate {
    public static let providerURL = URL(string: "spotify://")!
    private static let defaultRedirectURI = "cutout-spotify://spotify-login-callback"

    private let configuration: SPTConfiguration?
    private var appRemote: SPTAppRemote?
    private var accessToken: String?
    private var playerState: SPTAppRemotePlayerState?
    private var onChange: (@MainActor (MusicProviderObservation) -> Void)?
    private var lifecycleState: MobileMusicPlaybackStateDto = .disconnected

    public override init() {
        let clientID = Bundle.main.object(forInfoDictionaryKey: "SpotifyClientID") as? String
        let redirectURI = (Bundle.main.object(forInfoDictionaryKey: "SpotifyRedirectURI") as? String)
            ?? Self.defaultRedirectURI
        if let clientID,
           !clientID.isEmpty,
           !clientID.hasPrefix("$("),
           let redirectURL = URL(string: redirectURI),
           !redirectURI.isEmpty
        {
            configuration = SPTConfiguration(clientID: clientID, redirectURL: redirectURL)
        } else {
            configuration = nil
        }
        super.init()
    }

    public func startMonitoring(onChange: @escaping @MainActor (MusicProviderObservation) -> Void) {
        self.onChange = onChange
        guard let configuration else {
            lifecycleState = .unavailable
            emitChange()
            return
        }
        let appRemote = self.appRemote ?? SPTAppRemote(configuration: configuration, logLevel: .error)
        self.appRemote = appRemote
        appRemote.delegate = self
        if let accessToken {
            appRemote.connectionParameters.accessToken = accessToken
            appRemote.connect()
        } else {
            appRemote.authorizeAndPlayURI("") { [weak self] installed in
                guard !installed else { return }
                Task { @MainActor [weak self] in
                    guard let self else { return }
                    self.lifecycleState = .unavailable
                    self.emitChange()
                }
            }
        }
    }

    public func stopMonitoring() {
        appRemote?.disconnect()
        onChange = nil
        playerState = nil
        lifecycleState = .disconnected
    }

    /// Handles the redirect URL returned by Spotify after App Remote auth.
    @discardableResult
    public func handleCallback(_ url: URL) -> Bool {
        guard let appRemote else { return false }
        let parameters = appRemote.authorizationParameters(from: url)
        guard let parameters else { return false }
        if let token = parameters[SPTAppRemoteAccessTokenKey] {
            accessToken = token
            appRemote.connectionParameters.accessToken = token
            appRemote.connect()
            return true
        }
        lifecycleState = .unauthorized
        emitChange()
        return true
    }

    @MainActor
    public func perform(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        if case .openProvider = command {
            guard UIApplication.shared.canOpenURL(Self.providerURL) else { return .unavailable }
            guard await UIApplication.shared.open(Self.providerURL) else { return .failed }
            return .accepted
        }
        guard lifecycleState == .playing || lifecycleState == .paused,
              let playerAPI = appRemote?.playerAPI
        else { return .unavailable }
        return await withCheckedContinuation { continuation in
            let callback: SPTAppRemoteCallback = { _, error in
                continuation.resume(returning: error == nil ? .accepted : .failed)
            }
            switch command {
            case .previous: playerAPI.skip(toPrevious: callback)
            case .play: playerAPI.resume(callback)
            case .pause: playerAPI.pause(callback)
            case .next: playerAPI.skip(toNext: callback)
            case .openProvider: break
            }
        }
    }

    public func observation(observedAtMs: UInt64) -> MusicProviderObservation {
        let snapshot = MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: "spotify-app-remote",
            state: lifecycleState,
            item: playerState.map {
                MobileMusicItemDto(
                    identifier: $0.track.uri,
                    title: $0.track.name,
                    artist: $0.track.artist.name
                )
            },
            positionMilliseconds: playerState.flatMap { UInt64(exactly: max(0, $0.playbackPosition)) },
            durationMilliseconds: playerState.flatMap { UInt64(exactly: $0.track.duration) },
            observedAtMs: observedAtMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: playerState?.playbackRestrictions.canSkipPrevious == true,
                play: lifecycleState == .paused,
                pause: lifecycleState == .playing,
                next: playerState?.playbackRestrictions.canSkipNext == true,
                openProvider: true
            )
        )
        return MusicProviderObservation(snapshot: snapshot)
    }

    public func unavailableSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        observation(observedAtMs: observedAtMs).snapshot
    }

    public func unauthorizedSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: "spotify-app-remote",
            state: .unauthorized,
            item: nil,
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: observedAtMs,
            capabilities: .init(previous: false, play: false, pause: false, next: false, openProvider: true)
        )
    }

    public func appRemoteDidEstablishConnection(_ appRemote: SPTAppRemote) {
        lifecycleState = .paused
        appRemote.playerAPI?.delegate = self
        appRemote.playerAPI?.subscribe(toPlayerState: { [weak self] _, error in
            guard let self else { return }
            if error != nil {
                self.lifecycleState = .disconnected
                self.emitChange()
            }
        })
        emitChange()
    }

    public func appRemote(
        _ appRemote: SPTAppRemote,
        didFailConnectionAttemptWithError error: Error?
    ) {
        lifecycleState = error == nil ? .unavailable : .disconnected
        emitChange()
    }

    public func appRemote(_ appRemote: SPTAppRemote, didDisconnectWithError error: Error?) {
        lifecycleState = error == nil ? .disconnected : .stale
        emitChange()
    }

    public func playerStateDidChange(_ playerState: SPTAppRemotePlayerState) {
        self.playerState = playerState
        lifecycleState = playerState.isPaused ? .paused : .playing
        emitChange()
    }

    private func emitChange() {
        onChange?(MusicProviderObservation(snapshot: observation(observedAtMs: UInt64(Date().timeIntervalSince1970 * 1_000)).snapshot))
    }
}

#else

/// Build-time fallback used by macOS and iOS builds without configured SDK
/// credentials. It preserves a typed handoff/unavailable state.
public struct SpotifyProviderAdapter: Sendable {
    public static let providerURL = URL(string: "spotify://")!

    public init() {}

    @MainActor
    public func perform(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        guard case .openProvider = command else { return .unavailable }
#if canImport(UIKit) && os(iOS)
        guard UIApplication.shared.canOpenURL(Self.providerURL) else { return .unavailable }
        guard await UIApplication.shared.open(Self.providerURL) else { return .failed }
        return .accepted
#else
        return .unavailable
#endif
    }

    public func unavailableSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
        MusicProviderObservation.unavailable(
            provider: .spotify,
            sessionId: "spotify-unavailable",
            observedAtMs: observedAtMs,
            openProvider: true
        ).snapshot
    }
}

#endif
