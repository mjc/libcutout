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
public final class SpotifyProviderAdapter: NSObject, @preconcurrency SPTSessionManagerDelegate, @preconcurrency SPTAppRemoteDelegate, @preconcurrency SPTAppRemotePlayerStateDelegate {
    public static let providerURL = URL(string: "spotify://")!
    private static let defaultRedirectURI = "cutout-spotify://spotify-login-callback"
    private static let artworkSize = CGSize(width: 256, height: 256)
    private static let accessTokenKey = "io.cutout.music.spotify.access-token"
    private static let accessTokenAccount = "default"

    private let configuration: SPTConfiguration?
    private var sessionManager: SPTSessionManager?
    private var appRemote: SPTAppRemote?
    private var accessToken: String? {
        didSet {
            Self.storeAccessToken(accessToken)
        }
    }
    private var playerState: SPTAppRemotePlayerState?
    private var artwork: MusicArtwork?
    private var onChange: (@MainActor () -> Void)?
    private var lifecycleState: MobileMusicPlaybackStateDto = .disconnected
    private var monitoringGeneration: UInt64 = 0
    private var connectionAttemptIDs = [ObjectIdentifier: UInt64]()
    private let playerStateRequest = MobileMusicPlayerRequest()
    private var connection = MobileMusicConnection()
    private var authorizationTimeoutTask: Task<Void, Never>?
    private var authorizationInFlight = false
    private var connectionNowMs: UInt64 { UInt64(ProcessInfo.processInfo.systemUptime * 1_000) }
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

    /// Starts provider observation. Returns false when passive observation has
    /// no configured SDK or token, so the caller can avoid a no-op poll task.
    @discardableResult
    public func startMonitoring(
        allowAuthorization: Bool = false,
        onChange: @escaping @MainActor () -> Void
    ) -> Bool {
        stopMonitoring()
        self.onChange = onChange
#if DEBUG
        print("spotify_monitor_start allow_authorization=\(allowAuthorization) has_token=\(accessToken != nil)")
#endif
        guard let configuration else {
            lifecycleState = .unavailable
            emitChange()
            return false
        }
        // No credentials means no connection attempt, not transient buffering.
        guard accessToken != nil || allowAuthorization else {
            lifecycleState = .unauthorized
            emitChange()
            return false
        }
        if let accessToken {
            authorizationTimeoutTask?.cancel()
            authorizationTimeoutTask = nil
            connect(with: accessToken)
            return true
        } else {
            lifecycleState = .buffering
            emitChange()
            authorizationInFlight = true
            let generation = monitoringGeneration
            let sessionManager = SPTSessionManager(configuration: configuration, delegate: self)
            self.sessionManager = sessionManager
            sessionManager.initiateSession(with: .appRemoteControl, options: .default, campaign: nil)
            authorizationTimeoutTask = Task { @MainActor [weak self] in
                do {
                    try await Task.sleep(for: .seconds(20))
                } catch {
                    return
                }
                guard let self, self.monitoringGeneration == generation,
                      self.authorizationInFlight else { return }
                self.authorizationInFlight = false
                self.lifecycleState = .unauthorized
                self.emitChange()
            }
            return true
        }
    }

    public func stopMonitoring() {
        monitoringGeneration &+= 1
        onChange = nil
        appRemote?.playerAPI?.delegate = nil
        appRemote?.delegate = nil
        appRemote?.disconnect()
        appRemote = nil
        connectionAttemptIDs.removeAll()
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = nil
        playerStateRequest.reset()
        playerState = nil
        artwork = nil
        connection = MobileMusicConnection()
        lifecycleState = .disconnected
    }

    /// Reconnects an existing App Remote session after a provider-side
    /// disconnect. Missing or rejected credentials never trigger an auth loop;
    /// the next explicit setup starts authorization again.
    public func ensureConnection() {
        guard onChange != nil,
              appRemote?.isConnected != true,
              let accessToken
        else { return }
        connect(with: accessToken)
    }

    private func makeAppRemote(_ configuration: SPTConfiguration) -> SPTAppRemote {
        if let previous = appRemote {
            appRemote = nil
            connectionAttemptIDs.removeValue(forKey: ObjectIdentifier(previous))
            previous.playerAPI?.delegate = nil
            previous.delegate = nil
            previous.disconnect()
        }
        let appRemote = SPTAppRemote(configuration: configuration, logLevel: .error)
        self.appRemote = appRemote
        appRemote.delegate = self
        return appRemote
    }

    private func connect(with accessToken: String) {
        guard let configuration,
              let attemptID = connection.beginAttemptId(nowMs: connectionNowMs) else { return }
        // Every retry gets a fresh SDK object. This makes an old callback
        // unambiguously stale instead of letting it mutate the new attempt.
        let appRemote = makeAppRemote(configuration)
        connectionAttemptIDs[ObjectIdentifier(appRemote)] = attemptID
        appRemote.connectionParameters.accessToken = accessToken
        appRemote.connect()
    }

    /// Polls the current Spotify player state so a track that was already
    /// playing before App Remote connected is reflected without waiting for a
    /// change notification.
    public func refreshPlayerState() {
        guard appRemote?.isConnected == true else { return }
        let nowMs = connectionNowMs
        if playerStateRequest.isStale(nowMs: nowMs), lifecycleState != .stale {
            lifecycleState = .stale
            emitChange()
        }
        guard let playerAPI = appRemote?.playerAPI,
              let requestID = playerStateRequest.begin(nowMs: nowMs) else { return }
        let generation = monitoringGeneration
        playerAPI.getPlayerState { [weak self] result, error in
            let playerState = result as? SPTAppRemotePlayerState
            let errorInfo = (error as NSError?).map { ($0.domain, $0.code) }
            Task { @MainActor [weak self] in
                guard let self, self.monitoringGeneration == generation else { return }
                guard self.playerStateRequest.complete(requestId: requestID) == .accepted else { return }
                if let playerState, errorInfo == nil {
                    self.handlePlayerStateDidChange(playerState)
                } else if let errorDomain = errorInfo?.0, let errorCode = errorInfo?.1 {
#if DEBUG
                    print("spotify_player_state_failed domain=\(errorDomain) code=\(errorCode)")
#endif
                    self.lifecycleState = .stale
                    self.emitChange()
                }
            }
        }
    }

    /// Handles the redirect URL returned by Spotify after App Remote auth.
    @discardableResult
    public func handleCallback(_ url: URL) -> Bool {
        guard let configuration,
              url.scheme == configuration.redirectURL.scheme,
              url.host == configuration.redirectURL.host,
              musicCallbackPathMatches(expected: configuration.redirectURL.path, actual: url.path)
        else { return false }
        // SessionManager owns the authorization-code/PKCE callback. It never
        // asks Spotify to start playback; the returned session is connected to
        // App Remote below after the callback delegate fires.
        let sessionManager = self.sessionManager
            ?? SPTSessionManager(configuration: configuration, delegate: self)
        self.sessionManager = sessionManager
        return sessionManager.application(UIApplication.shared, open: url, options: [:])
    }

    public nonisolated func sessionManager(manager: SPTSessionManager, didInitiate session: SPTSession) {
        let managerID = UInt(bitPattern: ObjectIdentifier(manager))
        let accessToken = session.accessToken
        Task { @MainActor [weak self] in
            self?.acceptSession(managerID: managerID, accessToken: accessToken)
        }
    }

    public nonisolated func sessionManager(manager: SPTSessionManager, didRenew session: SPTSession) {
        let managerID = UInt(bitPattern: ObjectIdentifier(manager))
        let accessToken = session.accessToken
        Task { @MainActor [weak self] in
            self?.acceptSession(managerID: managerID, accessToken: accessToken)
        }
    }

    public nonisolated func sessionManager(manager: SPTSessionManager, didFailWith error: Error) {
        let managerID = UInt(bitPattern: ObjectIdentifier(manager))
        let nsError = error as NSError
        let errorDomain = nsError.domain
        let errorCode = nsError.code
        Task { @MainActor [weak self] in
            self?.authorizationDidFail(managerID: managerID, domain: errorDomain, code: errorCode)
        }
    }

    private func acceptSession(managerID: UInt, accessToken: String) {
        guard let sessionManager,
              UInt(bitPattern: ObjectIdentifier(sessionManager)) == managerID else { return }
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = nil
        authorizationInFlight = false
        self.accessToken = accessToken
        guard onChange != nil else { return }
        connection = MobileMusicConnection()
        connectionAttemptIDs.removeAll()
        connect(with: accessToken)
    }

    private func authorizationDidFail(managerID: UInt, domain: String, code: Int) {
        guard let sessionManager,
              UInt(bitPattern: ObjectIdentifier(sessionManager)) == managerID else { return }
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = nil
        authorizationInFlight = false
        lifecycleState = .unauthorized
#if DEBUG
        print("spotify_authorization_failed domain=\(domain) code=\(code)")
#endif
        emitChange()
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
        let controlsAvailable = lifecycleState == .playing || lifecycleState == .paused
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
                previous: controlsAvailable && playerState?.playbackRestrictions.canSkipPrevious == true,
                play: lifecycleState == .paused,
                pause: lifecycleState == .playing,
                next: controlsAvailable && playerState?.playbackRestrictions.canSkipNext == true,
                openProvider: true
            )
        )
        return MusicProviderObservation(snapshot: snapshot, artworkData: artwork?.data)
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

    public nonisolated func appRemoteDidEstablishConnection(_ appRemote: SPTAppRemote) {
        let appRemoteID = UInt(bitPattern: ObjectIdentifier(appRemote))
        Task { @MainActor [weak self] in
            self?.handleAppRemoteDidEstablishConnection(appRemoteID: appRemoteID)
        }
    }

    private func handleAppRemoteDidEstablishConnection(appRemoteID: UInt) {
        guard let appRemote = self.appRemote,
              UInt(bitPattern: ObjectIdentifier(appRemote)) == appRemoteID,
              onChange != nil else { return }
        guard let attemptID = connectionAttemptIDs[ObjectIdentifier(appRemote)],
              connection.establishedFor(attemptId: attemptID) == .accepted else { return }
#if DEBUG
        print("spotify_connection_established")
#endif
        monitoringGeneration &+= 1
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = nil
        playerStateRequest.reset()
        // A connected session that never returns player state must not remain
        // in buffering forever. Verified callbacks refresh this same window.
        playerStateRequest.markObserved(nowMs: connectionNowMs)
        lifecycleState = .buffering
        appRemote.playerAPI?.delegate = self
        let generation = monitoringGeneration
        appRemote.playerAPI?.subscribe(toPlayerState: { [weak self] result, error in
            let state = result as? SPTAppRemotePlayerState
            let hasError = error != nil
            Task { @MainActor [weak self] in
                guard let self, self.monitoringGeneration == generation else { return }
                if hasError {
                    self.lifecycleState = .stale
                    self.emitChange()
                } else if let state {
                    self.handlePlayerStateDidChange(state)
                }
            }
        })
        refreshPlayerState()
        emitChange()
    }

    public nonisolated func appRemote(
        _ appRemote: SPTAppRemote,
        didFailConnectionAttemptWithError error: Error?
    ) {
        let appRemoteID = UInt(bitPattern: ObjectIdentifier(appRemote))
        let errorInfo = (error as NSError?).map { ($0.domain, $0.code) }
        Task { @MainActor [weak self] in
            self?.handleAppRemoteConnectionFailure(
                appRemoteID: appRemoteID,
                errorDomain: errorInfo?.0,
                errorCode: errorInfo?.1
            )
        }
    }

    private func handleAppRemoteConnectionFailure(
        appRemoteID: UInt,
        errorDomain: String?,
        errorCode: Int?
    ) {
        guard let appRemote = self.appRemote,
              UInt(bitPattern: ObjectIdentifier(appRemote)) == appRemoteID,
              onChange != nil else { return }
        guard let attemptID = connectionAttemptIDs[ObjectIdentifier(appRemote)],
              connection.failedFor(attemptId: attemptID, nowMs: connectionNowMs) == .accepted else { return }
        connectionAttemptIDs.removeValue(forKey: ObjectIdentifier(appRemote))
        monitoringGeneration &+= 1
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = nil
        // App Remote reports transport and wakeup failures here too. A generic
        // connection failure is not evidence that the credential was rejected.
        lifecycleState = .disconnected
        playerStateRequest.reset()
        self.appRemote = nil
#if DEBUG
        if let errorDomain, let errorCode {
            print("spotify_connection_failed domain=\(errorDomain) code=\(errorCode)")
        }
#endif
        emitChange()
    }

    public nonisolated func appRemote(_ appRemote: SPTAppRemote, didDisconnectWithError error: Error?) {
        let appRemoteID = UInt(bitPattern: ObjectIdentifier(appRemote))
        let errorInfo = (error as NSError?).map { ($0.domain, $0.code) }
        Task { @MainActor [weak self] in
            self?.handleAppRemoteDidDisconnect(
                appRemoteID: appRemoteID,
                errorDomain: errorInfo?.0,
                errorCode: errorInfo?.1
            )
        }
    }

    private func handleAppRemoteDidDisconnect(
        appRemoteID: UInt,
        errorDomain: String?,
        errorCode: Int?
    ) {
        guard let appRemote = self.appRemote,
              UInt(bitPattern: ObjectIdentifier(appRemote)) == appRemoteID,
              onChange != nil else { return }
        guard let attemptID = connectionAttemptIDs[ObjectIdentifier(appRemote)],
              connection.disconnectedFor(attemptId: attemptID, nowMs: connectionNowMs) == .accepted else { return }
        connectionAttemptIDs.removeValue(forKey: ObjectIdentifier(appRemote))
        monitoringGeneration &+= 1
        self.appRemote = nil
        playerStateRequest.reset()
        lifecycleState = errorDomain == nil ? .disconnected : .stale
#if DEBUG
        if let errorDomain, let errorCode {
            print("spotify_disconnected domain=\(errorDomain) code=\(errorCode)")
        } else {
            print("spotify_disconnected without_error")
        }
#endif
        emitChange()
    }

    public nonisolated func playerStateDidChange(_ playerState: SPTAppRemotePlayerState) {
        Task { @MainActor [weak self] in
            self?.handlePlayerStateDidChange(playerState)
        }
    }

    private func handlePlayerStateDidChange(_ playerState: SPTAppRemotePlayerState) {
        guard onChange != nil else { return }
        let trackChanged = self.playerState?.track.uri != playerState.track.uri
#if DEBUG
        if trackChanged {
            print("spotify_player_state uri_bytes=\(playerState.track.uri.utf8.count) title_bytes=\(playerState.track.name.utf8.count) artist_bytes=\(playerState.track.artist.name.utf8.count) position=\(playerState.playbackPosition) duration=\(playerState.track.duration)")
        }
#endif
        self.playerStateRequest.markObserved(nowMs: connectionNowMs)
        self.playerState = playerState
        lifecycleState = playerState.isPaused ? .paused : .playing
        if trackChanged {
            artwork = nil
            let trackURI = playerState.track.uri
            appRemote?.imageAPI?.fetchImage(
                forItem: playerState.track,
                with: Self.artworkSize,
                callback: { [weak self] image, error in
                    guard error == nil,
                          let data = (image as? UIImage)?.jpegData(compressionQuality: 0.8)
                    else { return }
                    Task { @MainActor [weak self, data, trackURI] in
                        guard let self, self.playerState?.track.uri == trackURI else { return }
                        self.artwork = MusicArtwork(data: data)
                        self.emitChange()
                    }
                }
            )
        }
        emitChange()
    }

    /// Called only by the explicit Reauthorize Spotify account action.
    public func clearAuthorization() {
        stopMonitoring()
        sessionManager = nil
        accessToken = nil
        authorizationInFlight = false
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
