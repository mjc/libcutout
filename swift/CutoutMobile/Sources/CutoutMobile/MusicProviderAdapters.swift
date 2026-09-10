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
public final class SpotifyProviderAdapter: NSObject {
    public static let providerURL = URL(string: "spotify://")!
    private static let defaultRedirectURI = "cutout-spotify://spotify-login-callback"
    private static let artworkSize = CGSize(width: 256, height: 256)
    private static let artworkRequestTimeout = Duration.seconds(5)
    private static let accessTokenKey = "io.cutout.music.spotify.access-token"
    private static let sessionKey = "io.cutout.music.spotify.session-v1"
    private static let accessTokenAccount = "default"

    private final class AppRemoteBridge: NSObject, SPTAppRemoteDelegate, SPTAppRemotePlayerStateDelegate {
        weak var owner: SpotifyProviderAdapter?
        let generation: UInt64

        init(owner: SpotifyProviderAdapter, generation: UInt64) {
            self.owner = owner
            self.generation = generation
        }

        nonisolated func appRemoteDidEstablishConnection(_ appRemote: SPTAppRemote) {
            owner?.enqueueConnectionEstablished(generation: generation)
        }

        nonisolated func appRemote(
            _ appRemote: SPTAppRemote,
            didFailConnectionAttemptWithError error: Error?
        ) {
            let info = (error as NSError?).map { ($0.domain, $0.code) }
            owner?.enqueueConnectionFailure(
                generation: generation,
                errorDomain: info?.0,
                errorCode: info?.1
            )
        }

        nonisolated func appRemote(_ appRemote: SPTAppRemote, didDisconnectWithError error: Error?) {
            let info = (error as NSError?).map { ($0.domain, $0.code) }
            owner?.enqueueDisconnect(
                generation: generation,
                errorDomain: info?.0,
                errorCode: info?.1
            )
        }

        nonisolated func playerStateDidChange(_ playerState: SPTAppRemotePlayerState) {
            owner?.enqueuePlayerState(playerState, generation: generation)
        }
    }

    private final class SessionManagerBridge: NSObject, SPTSessionManagerDelegate {
        weak var owner: SpotifyProviderAdapter?
        let generation: UInt64

        init(owner: SpotifyProviderAdapter, generation: UInt64) {
            self.owner = owner
            self.generation = generation
        }

        nonisolated func sessionManager(manager: SPTSessionManager, didInitiate session: SPTSession) {
            owner?.enqueueSession(session, generation: generation)
        }

        nonisolated func sessionManager(manager: SPTSessionManager, didRenew session: SPTSession) {
            owner?.enqueueSession(session, generation: generation)
        }

        nonisolated func sessionManager(manager: SPTSessionManager, didFailWith error: Error) {
            let nsError = error as NSError
            owner?.enqueueAuthorizationFailure(
                generation: generation,
                domain: nsError.domain,
                code: nsError.code,
                description: nsError.localizedDescription
            )
        }
    }

    private let configuration: SPTConfiguration?
    private var sessionManager: SPTSessionManager?
    private var sessionManagerBridge: SessionManagerBridge?
    private let authorizationTransaction = MobileMusicAuthorizationTransaction()
    private var authorizationGeneration: UInt64?
    private var appRemote: SPTAppRemote?
    private var appRemoteBridge: AppRemoteBridge?
    private let appRemoteEpoch = MobileMusicCallbackEpoch()
    private var accessToken: String?
    private var session: SPTSession?
    private var playerState: SPTAppRemotePlayerState?
    private var artwork: MusicArtwork?
    private var artworkGeneration: UInt64 = 0
    private var artworkCache = MusicArtworkCache()
    private let artworkRequestPolicy = MobileMusicArtworkRequest()
    private var artworkRequest: (id: UInt64, trackURI: String, generation: UInt64)?
    private var artworkRetryTask: Task<Void, Never>?
    private var artworkDeadlineTask: Task<Void, Never>?
    private var onChange: (@MainActor () -> Void)?
    private var lifecycleState: MobileMusicPlaybackStateDto = .disconnected
    private var monitoringGeneration: UInt64 = 0
    private var connectionAttemptID: UInt64?
    private var appRemoteGeneration: UInt64 = 0
    private let playerStateRequest = MobileMusicPlayerRequest()
    private var connection = MobileMusicConnection()
    private var authorizationTimeoutTask: Task<Void, Never>?
    private var authorizationNeedsUserAction = false
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
        session = Self.loadSession()
        accessToken = session?.accessToken ?? Self.loadAccessToken()
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

    private static func sessionQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: sessionKey,
            kSecAttrAccount as String: accessTokenAccount,
        ]
    }

    private static func loadSession() -> SPTSession? {
#if canImport(Security)
        var query = sessionQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data
        else { return nil }
        return try? NSKeyedUnarchiver.unarchivedObject(ofClass: SPTSession.self, from: data)
#else
        nil
#endif
    }

    private static func storeSession(_ session: SPTSession?) {
#if canImport(Security)
        let query = sessionQuery()
        guard let session,
              let data = try? NSKeyedArchiver.archivedData(withRootObject: session, requiringSecureCoding: true)
        else {
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
        authorizationNeedsUserAction = false
#if DEBUG
        print("spotify_monitor_start allow_authorization=\(allowAuthorization) has_token=\(accessToken != nil)")
#endif
        guard let configuration else {
            lifecycleState = .unavailable
            emitChange()
            return false
        }
#if canImport(UIKit) && os(iOS)
        guard UIApplication.shared.canOpenURL(Self.providerURL) else {
            lifecycleState = .unavailable
            emitChange()
            return false
        }
#endif
        // A scene transition can stop and restart observation while the SDK's
        // authorization callback is still in flight. Keep that transaction
        // alive and let the single monitor loop observe its completion.
        if authorizationGeneration != nil, sessionManager != nil {
            lifecycleState = .buffering
            emitChange()
            return true
        }
        // No credentials means no connection attempt, not transient buffering.
        guard accessToken != nil || allowAuthorization else {
            lifecycleState = .unauthorized
            emitChange()
            return false
        }
        if let session {
            if session.isExpired {
                let (sessionManager, generation) = makeSessionManager(
                    configuration: configuration,
                    kind: .renewing
                )
                sessionManager.session = session
                accessToken = nil
                authorizationGeneration = generation
                lifecycleState = .buffering
                emitChange()
                sessionManager.renewSession()
                beginAuthorizationTimeout(generation: generation)
            } else {
                accessToken = session.accessToken
                connect(with: session.accessToken)
            }
            return true
        } else if let accessToken {
            authorizationTimeoutTask?.cancel()
            authorizationTimeoutTask = nil
            connect(with: accessToken)
            return true
        } else {
            lifecycleState = .buffering
            emitChange()
            let (sessionManager, generation) = makeSessionManager(
                configuration: configuration,
                kind: .authorizing
            )
            authorizationGeneration = generation
            sessionManager.initiateSession(with: .appRemoteControl, options: .default, campaign: nil)
            beginAuthorizationTimeout(generation: generation)
            return true
        }
    }

    private func makeSessionManager(
        configuration: SPTConfiguration,
        kind: MobileMusicAuthorizationKind
    ) -> (SPTSessionManager, UInt64) {
        let generation = authorizationTransaction.begin(kind: kind)
        let bridge = SessionManagerBridge(owner: self, generation: generation)
        let manager = SPTSessionManager(configuration: configuration, delegate: bridge)
        sessionManagerBridge = bridge
        sessionManager = manager
        return (manager, generation)
    }

    private func beginAuthorizationTimeout(generation: UInt64) {
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(for: .seconds(20))
            } catch {
                return
            }
            guard let self else { return }
            let transaction = self.finishAuthorizationTransaction(generation: generation)
            guard transaction != .stale else { return }
            self.authorizationNeedsUserAction = true
            if transaction == .renewing {
                self.lifecycleState = .stale
            } else {
                self.session = nil
                self.accessToken = nil
                Self.storeSession(nil)
                Self.storeAccessToken(nil)
                self.lifecycleState = .unauthorized
            }
            self.emitChange()
        }
    }

    public func stopMonitoring() {
        monitoringGeneration &+= 1
        onChange = nil
        appRemote?.playerAPI?.delegate = nil
        appRemote?.delegate = nil
        appRemote?.disconnect()
        appRemote = nil
        appRemoteBridge = nil
        connectionAttemptID = nil
        appRemoteEpoch.invalidate()
        playerStateRequest.reset()
        playerState = nil
        artwork = nil
        connection = MobileMusicConnection()
        invalidateArtworkRequest()
        // Stopping observation must not cancel an authorization handoff. A
        // foreground/background transition can happen while Spotify is open.
        if authorizationGeneration == nil {
            authorizationTimeoutTask?.cancel()
            authorizationTimeoutTask = nil
        }
        lifecycleState = .disconnected
    }

    /// Reconnects an existing App Remote session after a provider-side
    /// disconnect. Missing or rejected credentials never trigger an auth loop;
    /// the next explicit setup starts authorization again.
    public func ensureConnection() {
        guard onChange != nil,
              authorizationGeneration == nil,
              appRemote?.isConnected != true,
              let accessToken
        else { return }
        connect(with: accessToken)
    }

    /// Keeps the app model's one monitor task alive only while there is useful
    /// provider work to do. Authorization failures stop passive polling; the
    /// next explicit setup/foreground action can start a fresh transaction.
    public var shouldContinueMonitoring: Bool {
        onChange != nil && !authorizationNeedsUserAction
            && (authorizationGeneration != nil || accessToken != nil)
    }

    private func makeAppRemote(_ configuration: SPTConfiguration) -> SPTAppRemote {
        appRemoteGeneration = appRemoteEpoch.begin()
        invalidateArtworkRequest()
        if let previous = appRemote {
            appRemote = nil
            previous.playerAPI?.delegate = nil
            previous.delegate = nil
            previous.disconnect()
        }
        let appRemote = SPTAppRemote(configuration: configuration, logLevel: .error)
        self.appRemote = appRemote
        let bridge = AppRemoteBridge(owner: self, generation: appRemoteGeneration)
        appRemoteBridge = bridge
        appRemote.delegate = bridge
        return appRemote
    }

    private func connect(with accessToken: String) {
        guard let configuration,
              let attemptID = connection.beginAttemptId(nowMs: connectionNowMs) else { return }
        // Every retry gets a fresh SDK object. This makes an old callback
        // unambiguously stale instead of letting it mutate the new attempt.
        let appRemote = makeAppRemote(configuration)
        connectionAttemptID = attemptID
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
        let appRemoteGeneration = self.appRemoteGeneration
        playerAPI.getPlayerState { [weak self] result, error in
            let playerState = result as? SPTAppRemotePlayerState
            let errorInfo = (error as NSError?).map { ($0.domain, $0.code) }
            Task { @MainActor [weak self] in
                guard let self,
                      self.monitoringGeneration == generation,
                      self.appRemoteEpoch.classify(id: appRemoteGeneration) == .current else { return }
                guard self.playerStateRequest.complete(requestId: requestID) == .accepted else { return }
                if let playerState, errorInfo == nil {
                    self.handlePlayerStateDidChange(playerState, appRemoteGeneration: appRemoteGeneration)
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
        guard let generation = authorizationGeneration,
              authorizationTransaction.classify(id: generation) != .stale,
              let sessionManager else { return false }
        return sessionManager.application(UIApplication.shared, open: url, options: [:])
    }

    private nonisolated func enqueueSession(_ session: SPTSession, generation: UInt64) {
        Task { @MainActor [weak self] in
            self?.acceptSession(generation: generation, session: session)
        }
    }

    private nonisolated func enqueueAuthorizationFailure(
        generation: UInt64,
        domain: String,
        code: Int,
        description: String
    ) {
        Task { @MainActor [weak self] in
            self?.authorizationDidFail(
                generation: generation,
                domain: domain,
                code: code,
                description: description
            )
        }
    }

    private func acceptSession(generation: UInt64, session: SPTSession) {
        guard finishAuthorizationTransaction(generation: generation) != .stale else { return }
        self.session = session
        Self.storeSession(session)
        self.accessToken = session.accessToken
        authorizationNeedsUserAction = false
        guard onChange != nil else { return }
        connection = MobileMusicConnection()
        connectionAttemptID = nil
        connect(with: session.accessToken)
    }

    private func authorizationDidFail(
        generation: UInt64,
        domain: String,
        code: Int,
        description: String
    ) {
        let transaction = finishAuthorizationTransaction(generation: generation)
        guard transaction != .stale else { return }
        let permanent = isPermanentAuthorizationFailure(domain: domain, code: code, description: description)
        if transaction == .renewing, !permanent {
            // Transport failures during renewal are recoverable. Keep the
            // refresh credential and stop this loop so a later foreground or
            // explicit setup can retry without forcing a reauthorization.
            authorizationNeedsUserAction = true
            lifecycleState = .stale
        } else {
            authorizationNeedsUserAction = true
            session = nil
            accessToken = nil
            Self.storeSession(nil)
            Self.storeAccessToken(nil)
            lifecycleState = .unauthorized
        }
#if DEBUG
        print("spotify_authorization_failed domain=\(domain) code=\(code) description=\(description)")
#endif
        emitChange()
    }

    private func finishAuthorizationTransaction(generation: UInt64) -> MobileMusicAuthorizationMatch {
        guard authorizationGeneration == generation else { return .stale }
        let outcome = authorizationTransaction.finish(id: generation)
        guard outcome != .stale else { return .stale }
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = nil
        sessionManagerBridge?.owner = nil
        sessionManagerBridge = nil
        sessionManager = nil
        authorizationGeneration = nil
        return outcome
    }

    private func invalidateAuthorizationTransaction() {
        authorizationTransaction.invalidate()
        authorizationTimeoutTask?.cancel()
        authorizationTimeoutTask = nil
        sessionManagerBridge?.owner = nil
        sessionManagerBridge = nil
        sessionManager = nil
        authorizationGeneration = nil
    }

    private func isPermanentAuthorizationFailure(domain: String, code: Int, description: String) -> Bool {
        let text = "\(domain) \(description)".lowercased()
        return code == 401
            || text.contains("invalid_grant")
            || text.contains("invalid token")
            || text.contains("unauthorized")
            || text.contains("revoked")
            || text.contains("expired")
    }

    private nonisolated func enqueueConnectionEstablished(generation: UInt64) {
        Task { @MainActor [weak self] in
            self?.handleAppRemoteDidEstablishConnection(generation: generation)
        }
    }

    private nonisolated func enqueueConnectionFailure(
        generation: UInt64,
        errorDomain: String?,
        errorCode: Int?
    ) {
        Task { @MainActor [weak self] in
            self?.handleAppRemoteConnectionFailure(
                generation: generation,
                errorDomain: errorDomain,
                errorCode: errorCode
            )
        }
    }

    private nonisolated func enqueueDisconnect(
        generation: UInt64,
        errorDomain: String?,
        errorCode: Int?
    ) {
        Task { @MainActor [weak self] in
            self?.handleAppRemoteDidDisconnect(
                generation: generation,
                errorDomain: errorDomain,
                errorCode: errorCode
            )
        }
    }

    private nonisolated func enqueuePlayerState(
        _ playerState: SPTAppRemotePlayerState,
        generation: UInt64
    ) {
        Task { @MainActor [weak self] in
            self?.handlePlayerStateDidChange(playerState, appRemoteGeneration: generation)
        }
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

    private func handleAppRemoteDidEstablishConnection(generation: UInt64) {
        guard let appRemote = self.appRemote,
              appRemoteEpoch.classify(id: generation) == .current,
              onChange != nil else { return }
        guard let attemptID = connectionAttemptID,
              connection.establishedFor(attemptId: attemptID) == .accepted else { return }
#if DEBUG
        print("spotify_connection_established")
#endif
        monitoringGeneration &+= 1
        playerStateRequest.reset()
        // A connected session that never returns player state must not remain
        // in buffering forever. Verified callbacks refresh this same window.
        playerStateRequest.markObserved(nowMs: connectionNowMs)
        lifecycleState = .buffering
        appRemote.playerAPI?.delegate = appRemoteBridge
        let generation = monitoringGeneration
        let appRemoteGeneration = self.appRemoteGeneration
        appRemote.playerAPI?.subscribe(toPlayerState: { [weak self] result, error in
            let state = result as? SPTAppRemotePlayerState
            let hasError = error != nil
            Task { @MainActor [weak self] in
                guard let self,
                      self.monitoringGeneration == generation,
                      self.appRemoteEpoch.classify(id: appRemoteGeneration) == .current else { return }
                if hasError {
                    self.lifecycleState = .stale
                    self.emitChange()
                } else if let state {
                    self.handlePlayerStateDidChange(state, appRemoteGeneration: appRemoteGeneration)
                }
            }
        })
        refreshPlayerState()
        emitChange()
    }

    private func handleAppRemoteConnectionFailure(
        generation: UInt64,
        errorDomain: String?,
        errorCode: Int?
    ) {
        guard self.appRemote != nil,
              appRemoteEpoch.classify(id: generation) == .current else { return }
        let attemptID = connectionAttemptID
        retireAppRemote(generation: generation)
        if let attemptID {
            _ = connection.failedFor(attemptId: attemptID, nowMs: connectionNowMs)
        }
        // App Remote reports transport and wakeup failures here too. A generic
        // connection failure is not evidence that the credential was rejected.
        lifecycleState = .disconnected
#if DEBUG
        if let errorDomain, let errorCode {
            print("spotify_connection_failed domain=\(errorDomain) code=\(errorCode)")
        }
#endif
        emitChange()
    }

    private func handleAppRemoteDidDisconnect(
        generation: UInt64,
        errorDomain: String?,
        errorCode: Int?
    ) {
        guard self.appRemote != nil,
              appRemoteEpoch.classify(id: generation) == .current else { return }
        let attemptID = connectionAttemptID
        retireAppRemote(generation: generation)
        if let attemptID {
            _ = connection.disconnectedFor(attemptId: attemptID, nowMs: connectionNowMs)
        }
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

    private func retireAppRemote(generation: UInt64) {
        guard appRemoteEpoch.finish(id: generation) == .current else { return }
        connectionAttemptID = nil
        monitoringGeneration &+= 1
        appRemote?.playerAPI?.delegate = nil
        appRemote?.delegate = nil
        appRemoteBridge?.owner = nil
        appRemote = nil
        appRemoteBridge = nil
        playerStateRequest.reset()
        invalidateArtworkRequest()
        playerState = nil
        artwork = nil
    }

    private func handlePlayerStateDidChange(
        _ playerState: SPTAppRemotePlayerState,
        appRemoteGeneration: UInt64
    ) {
        guard onChange != nil,
              appRemote != nil,
              appRemoteEpoch.classify(id: appRemoteGeneration) == .current else { return }
        let trackChanged = self.playerState?.track.uri != playerState.track.uri
#if DEBUG
        if trackChanged {
            print("spotify_player_state uri_bytes=\(playerState.track.uri.utf8.count) title_bytes=\(playerState.track.name.utf8.count) artist_bytes=\(playerState.track.artist.name.utf8.count) position=\(playerState.playbackPosition) duration=\(playerState.track.duration)")
        }
#endif
        self.playerStateRequest.markObserved(nowMs: connectionNowMs)
        self.playerState = playerState
        lifecycleState = playerState.isPaused ? .paused : .playing
        if trackChanged || artworkGeneration != appRemoteGeneration {
            artworkGeneration = appRemoteGeneration
            artwork = artworkCache.cachedArtwork(for: playerState.track.uri)
            invalidateArtworkRequest()
            if artwork == nil { requestArtwork(for: playerState.track, generation: appRemoteGeneration) }
        } else if artwork == nil {
            requestArtwork(for: playerState.track, generation: appRemoteGeneration)
        }
        emitChange()
    }

    private func invalidateArtworkRequest() {
        artworkRequestPolicy.reset()
        artworkRequest = nil
        artworkRetryTask?.cancel()
        artworkRetryTask = nil
        artworkDeadlineTask?.cancel()
        artworkDeadlineTask = nil
    }

    private func requestArtwork(
        for track: SPTAppRemoteTrack,
        generation: UInt64
    ) {
        guard appRemoteEpoch.classify(id: generation) == .current,
              artworkRequest == nil,
              artworkRetryTask == nil,
              let imageAPI = appRemote?.imageAPI,
              let requestID = artworkRequestPolicy.begin() else { return }
        let trackURI = track.uri
        artworkRequest = (requestID, trackURI, generation)
        beginArtworkDeadline(requestID: requestID, track: track, generation: generation)
        imageAPI.fetchImage(forItem: track, with: Self.artworkSize) { [weak self] image, error in
            let data = (image as? UIImage)?.jpegData(compressionQuality: 0.8)
            let failed = error != nil
            Task { @MainActor [weak self, data, failed] in
                guard let self,
                      let request = self.artworkRequest,
                      request.id == requestID,
                      request.trackURI == trackURI,
                      request.generation == generation,
                      self.appRemoteEpoch.classify(id: generation) == .current,
                      self.playerState?.track.uri == trackURI else { return }
                guard self.artworkRequestPolicy.complete(requestId: requestID) == .accepted else { return }
                self.artworkRequest = nil
                self.artworkDeadlineTask?.cancel()
                self.artworkDeadlineTask = nil
                if !failed, let data, let artwork = MusicArtwork(data: data) {
                    self.artworkCache.insert(artwork, for: trackURI)
                    self.artwork = artwork
                    self.emitChange()
                } else if self.artworkRequestPolicy.canRetry() {
                    self.scheduleArtworkRetry(track: track, generation: generation)
                }
            }
        }
    }

    private func beginArtworkDeadline(
        requestID: UInt64,
        track: SPTAppRemoteTrack,
        generation: UInt64
    ) {
        artworkDeadlineTask?.cancel()
        artworkDeadlineTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(for: Self.artworkRequestTimeout)
            } catch {
                return
            }
            guard let self,
                  let request = self.artworkRequest,
                  request.id == requestID,
                  request.trackURI == track.uri,
                  request.generation == generation,
                  self.artworkRequestPolicy.complete(requestId: requestID) == .accepted else { return }
            self.artworkRequest = nil
            self.artworkDeadlineTask = nil
            if self.artworkRequestPolicy.canRetry() {
                self.scheduleArtworkRetry(track: track, generation: generation)
            }
        }
    }

    private func scheduleArtworkRetry(
        track: SPTAppRemoteTrack,
        generation: UInt64
    ) {
        artworkRetryTask?.cancel()
        artworkRetryTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(for: .seconds(1))
            } catch {
                return
            }
            guard let self,
                  self.appRemoteEpoch.classify(id: generation) == .current,
                  self.playerState?.track.uri == track.uri else { return }
            self.artworkRetryTask = nil
            self.requestArtwork(for: track, generation: generation)
        }
    }

    /// Called only by the explicit Reauthorize Spotify account action.
    public func clearAuthorization() {
        stopMonitoring()
        invalidateAuthorizationTransaction()
        accessToken = nil
        session = nil
        Self.storeAccessToken(nil)
        Self.storeSession(nil)
        authorizationNeedsUserAction = true
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
