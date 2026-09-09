import CutoutMobileFFI
import Foundation
#if canImport(UIKit) && os(iOS)
import UIKit
#endif
#if canImport(Security)
import Security
#endif
#if canImport(SpotifyiOS) && os(iOS)
@preconcurrency import SpotifyiOS
#endif

#if canImport(SpotifyiOS) && os(iOS)

/// Thin main-thread bridge to Spotify's official App Remote SDK. The SDK owns
/// authorization, playback, and provider lifecycle; only bounded projections
/// enter the shared music/Rust pipeline.
@MainActor
public final class SpotifyProviderAdapter: NSObject, @preconcurrency SPTAppRemoteDelegate, @preconcurrency SPTAppRemotePlayerStateDelegate {
    public static let providerURL = URL(string: "spotify://")!
    private static let defaultRedirectURI = "cutout-spotify://spotify-login-callback"
    private static let accessTokenKey = "io.cutout.music.spotify.access-token"
    private static let accessTokenAccount = "default"

    private let configuration: SPTConfiguration?
    private var appRemote: SPTAppRemote?
    private var accessToken: String? {
        didSet {
            Self.storeAccessToken(accessToken)
        }
    }
    private var playerState: SPTAppRemotePlayerState?
    private var onChange: (@MainActor () -> Void)?
    private var lifecycleState: MobileMusicPlaybackStateDto = .disconnected
    private var monitoringGeneration: UInt64 = 0
    private var playerStateRequestPending = false
    private var connectionAttemptInFlight = false
    private var nextConnectionAttemptAt = Date.distantPast
    private var connectionAttemptCount = 0
    private static let maximumConnectionAttempts = 3
#if DEBUG
    private var lastObservationDiagnostic: String?
#endif

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
        accessToken = Self.loadAccessToken()
        super.init()
    }

    private static func keychainQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: accessTokenKey,
            kSecAttrAccount as String: accessTokenAccount,
        ]
    }

    private static func loadAccessToken() -> String? {
#if canImport(Security)
        var query = keychainQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data,
              let token = String(data: data, encoding: .utf8),
              !token.isEmpty
        else { return nil }
        return token
#else
        nil
#endif
    }

    private static func storeAccessToken(_ token: String?) {
#if canImport(Security)
        let query = keychainQuery()
        guard let token, let data = token.data(using: .utf8) else {
            SecItemDelete(query as CFDictionary)
            return
        }
        let attributes = [kSecValueData as String: data]
        if SecItemUpdate(query as CFDictionary, attributes as CFDictionary) != errSecSuccess {
            var item = query
            item[kSecValueData as String] = data
            SecItemAdd(item as CFDictionary, nil)
        }
#endif
    }

    public func startMonitoring(onChange: @escaping @MainActor () -> Void) {
        stopMonitoring()
        self.onChange = onChange
        guard let configuration else {
            lifecycleState = .unavailable
            emitChange()
            return
        }
        let appRemote = SPTAppRemote(configuration: configuration, logLevel: .error)
        self.appRemote = appRemote
        appRemote.delegate = self
        lifecycleState = .buffering
        emitChange()
        if let accessToken {
            connect(appRemote, with: accessToken)
        } else {
            connectionAttemptInFlight = true
            let generation = monitoringGeneration
            appRemote.authorizeAndPlayURI("") { [weak self] installed in
                guard !installed else { return }
                Task { @MainActor [weak self] in
                    guard let self, self.monitoringGeneration == generation else { return }
                    self.connectionAttemptInFlight = false
                    self.lifecycleState = .unavailable
                    self.emitChange()
                }
            }
        }
    }

    public func stopMonitoring() {
        monitoringGeneration &+= 1
        onChange = nil
        appRemote?.playerAPI?.delegate = nil
        appRemote?.delegate = nil
        appRemote?.disconnect()
        appRemote = nil
        playerStateRequestPending = false
        playerState = nil
        connectionAttemptInFlight = false
        nextConnectionAttemptAt = .distantPast
        connectionAttemptCount = 0
        lifecycleState = .disconnected
    }

    /// Reconnects an existing App Remote session after a provider-side
    /// disconnect. Missing or rejected credentials never trigger an auth loop;
    /// the next explicit setup starts authorization again.
    public func ensureConnection() {
        guard onChange != nil,
              let appRemote,
              !appRemote.isConnected,
              !connectionAttemptInFlight,
              Date() >= nextConnectionAttemptAt,
              let accessToken,
              connectionAttemptCount < Self.maximumConnectionAttempts
        else { return }
        connect(appRemote, with: accessToken)
    }

    private func connect(_ appRemote: SPTAppRemote, with accessToken: String) {
        connectionAttemptCount += 1
        appRemote.connectionParameters.accessToken = accessToken
        connectionAttemptInFlight = true
        nextConnectionAttemptAt = Date().addingTimeInterval(2)
        appRemote.connect()
    }

    /// Polls the current Spotify player state so a track that was already
    /// playing before App Remote connected is reflected without waiting for a
    /// change notification.
    public func refreshPlayerState() {
        guard appRemote?.isConnected == true,
              !playerStateRequestPending,
              let playerAPI = appRemote?.playerAPI else { return }
        playerStateRequestPending = true
        let generation = monitoringGeneration
        playerAPI.getPlayerState { [weak self] result, error in
            guard let self, self.monitoringGeneration == generation else { return }
            self.playerStateRequestPending = false
            if let playerState = result as? SPTAppRemotePlayerState, error == nil {
                self.playerStateDidChange(playerState)
            } else if let error = error as NSError? {
#if DEBUG
                print("spotify_player_state_failed domain=\(error.domain) code=\(error.code)")
#endif
                self.lifecycleState = .stale
                self.emitChange()
            }
        }
    }

    /// Handles the redirect URL returned by Spotify after App Remote auth.
    @discardableResult
    public func handleCallback(_ url: URL) -> Bool {
        guard let configuration,
              url.scheme == configuration.redirectURL.scheme,
              url.host == configuration.redirectURL.host,
              url.path == configuration.redirectURL.path else { return false }
        // The URL can arrive before the scene resumes monitoring after handoff.
        let appRemote = self.appRemote ?? SPTAppRemote(configuration: configuration, logLevel: .error)
        let parameters = appRemote.authorizationParameters(from: url)
        guard let parameters else { return false }
        if let token = parameters[SPTAppRemoteAccessTokenKey], !token.isEmpty {
            accessToken = token
            self.appRemote = appRemote
            appRemote.delegate = self
            appRemote.connectionParameters.accessToken = token
            if onChange != nil {
                connectionAttemptCount = 0
                connect(appRemote, with: token)
            }
            return true
        }
        connectionAttemptInFlight = false
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
        guard appRemote === self.appRemote, onChange != nil else { return }
        connectionAttemptInFlight = false
        nextConnectionAttemptAt = .distantPast
        connectionAttemptCount = 0
        lifecycleState = .buffering
        appRemote.playerAPI?.delegate = self
        let generation = monitoringGeneration
        appRemote.playerAPI?.subscribe(toPlayerState: { [weak self] result, error in
            guard let self, self.monitoringGeneration == generation else { return }
            if error != nil {
                self.lifecycleState = .stale
                self.emitChange()
            } else if let state = result as? SPTAppRemotePlayerState {
                self.playerStateDidChange(state)
            }
        })
        refreshPlayerState()
        emitChange()
    }

    public func appRemote(
        _ appRemote: SPTAppRemote,
        didFailConnectionAttemptWithError error: Error?
    ) {
        guard appRemote === self.appRemote, onChange != nil else { return }
        connectionAttemptInFlight = false
        // A failed connection means the cached token is no longer usable (for
        // example after revocation or account switching). Drop it so the next
        // explicit setup can authorize instead of retrying forever.
        accessToken = nil
        nextConnectionAttemptAt = .distantFuture
        lifecycleState = .unauthorized
#if DEBUG
        if let error = error as NSError? {
            print("spotify_connection_failed domain=\(error.domain) code=\(error.code)")
        }
#endif
        emitChange()
    }

    public func appRemote(_ appRemote: SPTAppRemote, didDisconnectWithError error: Error?) {
        guard appRemote === self.appRemote, onChange != nil else { return }
        connectionAttemptInFlight = false
        nextConnectionAttemptAt = Date().addingTimeInterval(2)
        lifecycleState = error == nil ? .disconnected : .stale
#if DEBUG
        if let error = error as NSError? {
            print("spotify_disconnected domain=\(error.domain) code=\(error.code)")
        } else {
            print("spotify_disconnected without_error")
        }
#endif
        emitChange()
    }

    public func playerStateDidChange(_ playerState: SPTAppRemotePlayerState) {
        guard onChange != nil else { return }
#if DEBUG
        if self.playerState?.track.uri != playerState.track.uri {
            print("spotify_player_state uri_bytes=\(playerState.track.uri.utf8.count) title_bytes=\(playerState.track.name.utf8.count) artist_bytes=\(playerState.track.artist.name.utf8.count) position=\(playerState.playbackPosition) duration=\(playerState.track.duration)")
        }
#endif
        self.playerState = playerState
        lifecycleState = playerState.isPaused ? .paused : .playing
        emitChange()
    }

    private func emitChange() {
#if DEBUG
        let snapshot = observation(observedAtMs: 0).snapshot
        let diagnostic = "spotify_observation state=\(lifecycleState) has_item=\(snapshot.item != nil) monitoring=\(onChange != nil)"
        if lastObservationDiagnostic != diagnostic {
            lastObservationDiagnostic = diagnostic
            print(diagnostic)
        }
#endif
        onChange?()
    }
}

#else

/// Build-time fallback used by macOS and iOS builds without configured SDK
/// credentials. It preserves a typed handoff/unavailable state.
public struct SpotifyProviderAdapter: Sendable {
    public static let providerURL = URL(string: "spotify://")!

    public init() {}

    public func refreshPlayerState() {}

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
