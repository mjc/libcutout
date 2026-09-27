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

enum SpotifyAuthorizationFailure {
    enum Classification: Sendable, Equatable {
        case rejectedGrant
        case recoverable
    }

    static func classify(_ error: Error) -> Classification {
        var current: NSError? = error as NSError
        var visited = Set<ObjectIdentifier>()
        for _ in 0..<8 {
            guard let value = current, visited.insert(ObjectIdentifier(value)).inserted else { break }
            if requiresNewAuthorization(domain: value.domain, code: value.code, description: value.localizedDescription) {
                return .rejectedGrant
            }
            current = value.userInfo[NSUnderlyingErrorKey] as? NSError
        }
        return .recoverable
    }

    static func requiresNewAuthorization(domain: String, code: Int, description: String) -> Bool {
        let text = "\(domain) \(description)".lowercased()
        // A generic 401 can be a misconfigured refresh endpoint or an expired
        // access token. Neither proves that the saved refresh grant was revoked.
        return text.contains("invalid_grant") || (text.contains("refresh token") && text.contains("revoked"))
    }

    static func preservesSavedSession(isRenewal: Bool, hasSavedSession: Bool, rejectedGrant: Bool) -> Bool {
        // Failure of a new permission request says nothing about the old grant.
        isRenewal ? !rejectedGrant : hasSavedSession
    }
}

struct SpotifyRenewalRetryPolicy {
    private(set) var automaticRenewalAllowed = true

    mutating func beginMonitoring() {
        automaticRenewalAllowed = true
    }

    mutating func renewalFailed() {
        automaticRenewalAllowed = false
    }

    mutating func renewalSucceeded() {
        automaticRenewalAllowed = true
    }
}

#if canImport(SpotifyiOS) && os(iOS)

/// Thin main-thread bridge to Spotify's official App Remote SDK. The SDK owns
/// authorization, playback, and provider lifecycle; only bounded projections
/// enter the shared music/Rust pipeline.
@MainActor
public final class SpotifyProviderAdapter: NSObject {
    public static let providerURL = URL(string: "spotify://")!
    private static let defaultRedirectURI = "cutout-spotify://spotify-login-callback"
    private static let artworkSize = CGSize(width: 256, height: 256)
    private static let accessTokenKey = "io.cutout.music.spotify.access-token"
    private static let sessionKey = "io.cutout.music.spotify.session-v1"
    private static let accessTokenAccount = "default"

    private final class AppRemoteBridge: NSObject, SPTAppRemoteDelegate, SPTAppRemotePlayerStateDelegate {
        weak var owner: SpotifyProviderAdapter?
        let providerGeneration: MobileMusicProviderSessionId
        let attemptID: MobileMusicConnectionAttemptId

        init(owner: SpotifyProviderAdapter, providerGeneration: MobileMusicProviderSessionId, attemptID: MobileMusicConnectionAttemptId) {
            self.owner = owner
            self.providerGeneration = providerGeneration
            self.attemptID = attemptID
        }

        nonisolated func appRemoteDidEstablishConnection(_ appRemote: SPTAppRemote) {
            owner?.enqueueConnectionEstablished(
                providerGeneration: providerGeneration,
                attemptID: attemptID
            )
        }

        nonisolated func appRemote(
            _ appRemote: SPTAppRemote,
            didFailConnectionAttemptWithError error: Error?
        ) {
            let nsError = error as NSError?
            let underlying = nsError?.userInfo[NSUnderlyingErrorKey] as? NSError
            let root = underlying?.userInfo[NSUnderlyingErrorKey] as? NSError
            owner?.enqueueConnectionFailure(
                providerGeneration: providerGeneration,
                attemptID: attemptID,
                errorDomain: nsError?.domain,
                errorCode: nsError?.code,
                underlyingDomain: underlying?.domain,
                underlyingCode: underlying?.code,
                rootDomain: root?.domain,
                rootCode: root?.code
            )
        }

        nonisolated func appRemote(_ appRemote: SPTAppRemote, didDisconnectWithError error: Error?) {
            let info = (error as NSError?).map { ($0.domain, $0.code) }
            owner?.enqueueDisconnect(
                providerGeneration: providerGeneration,
                attemptID: attemptID,
                errorDomain: info?.0,
                errorCode: info?.1
            )
        }

        nonisolated func playerStateDidChange(_ playerState: SPTAppRemotePlayerState) {
            owner?.enqueuePlayerState(
                playerState,
                providerGeneration: providerGeneration,
                attemptID: attemptID
            )
        }
    }

    private final class SessionManagerBridge: NSObject, SPTSessionManagerDelegate {
        weak var owner: SpotifyProviderAdapter?
        let generation: MobileMusicAuthorizationId

        init(owner: SpotifyProviderAdapter, generation: MobileMusicAuthorizationId) {
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
                classification: SpotifyAuthorizationFailure.classify(error)
            )
        }
    }

    private let configuration: SPTConfiguration?
    private var sessionManager: SPTSessionManager?
    private var sessionManagerBridge: SessionManagerBridge?
    private let lifecycle: MobileMusicProviderLifecycle
    private let effects: MusicProviderEffectExecutor
    private var authorizationGeneration: MobileMusicAuthorizationId?
    private var appRemote: SPTAppRemote?
    private var appRemoteBridge: AppRemoteBridge?
    private var establishedConnectionID: MobileMusicEstablishedConnectionId?
    private var accessToken: String?
    private var session: SPTSession?
    private var playerState: SPTAppRemotePlayerState?
    private var artwork: MusicArtwork?
    private var artworkCache = MusicArtworkCache()
    private var artworkRequest: (id: MobileMusicArtworkRequestId, trackURI: String, generation: MobileMusicProviderSessionId)?
    private var artworkRetryID: MobileMusicArtworkRetryId?
    private var playerStateRequestID: MobileMusicPlayerStateRequestId?
    private var onChange: (@MainActor () -> Void)?
    private var lifecycleState: MobileMusicPlaybackStateDto = .disconnected
    private var appRemoteGeneration: MobileMusicProviderSessionId?
    private lazy var transport = MusicProviderTransportExecutor(
        lifecycle: lifecycle,
        effects: effects,
        nowMs: { [weak self] in self?.connectionNowMs ?? 0 }
    )
    private var authorizationNeedsUserAction = false
    private var renewalRetryPolicy = SpotifyRenewalRetryPolicy()
    private static let playbackScopes: SPTScope = [.userReadPlaybackState, .userModifyPlaybackState]
    private let playbackAPI = SpotifyPlaybackAPI()
    private var webPlayback: SpotifyPlayback?
    private var webPollTask: Task<Void, Never>?
    private var webCommandTask: Task<Void, Never>?
    private var webArtworkTask: Task<Void, Never>?
    private var webRenewalAttempted = false
    private var usesWebPlayback: Bool { session?.scope.contains(Self.playbackScopes) == true }
    private var selectedPlaybackTransport: MobileMusicPlaybackTransport {
        musicPlaybackTransport(
            appRemoteConnected: appRemote?.isConnected == true,
            webPlaybackAuthorized: usesWebPlayback
        )
    }
    private var connectionNowMs: UInt64 { UInt64(ProcessInfo.processInfo.systemUptime * 1_000) }
#if DEBUG
    private var lastObservationDiagnostic: String?
    private var renewalValidationRequested = ProcessInfo.processInfo.arguments.contains("--validate-spotify-renewal")
#endif

    public override convenience init() {
        self.init(
            lifecycle: MobileMusicProviderLifecycle(),
            effects: MusicProviderEffectExecutor()
        )
    }

    public convenience init(lifecycle: MobileMusicProviderLifecycle) {
        self.init(lifecycle: lifecycle, effects: MusicProviderEffectExecutor())
    }

    public init(
        lifecycle: MobileMusicProviderLifecycle,
        effects: MusicProviderEffectExecutor
    ) {
        self.lifecycle = lifecycle
        self.effects = effects
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
        renewalRetryPolicy.beginMonitoring()
#if DEBUG
        print("spotify_monitor_start allow_authorization=\(allowAuthorization) has_token=\(accessToken != nil) has_session=\(session != nil) has_refresh_token=\(session.map { !$0.refreshToken.isEmpty } ?? false) session_expired=\(session?.isExpired ?? false)")
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
        guard let providerGeneration = lifecycle.beginProviderSession() else {
            lifecycleState = .unavailable
            emitChange()
            return false
        }
        appRemoteGeneration = providerGeneration
        if authorizationGeneration != nil, sessionManager != nil {
            lifecycleState = .buffering
            emitChange()
            return true
        }
        // The Rust-issued explicit Connect intent may upgrade an older grant.
        // Passive launch always reuses the existing grant without opening Spotify.
        if allowAuthorization, !usesWebPlayback {
            return beginAuthorization(configuration: configuration)
        }
        if let session {
#if DEBUG
            // Opt-in device validation exercises a real silent SDK refresh
            // without changing the persisted expiration or deleting credentials.
            if usesWebPlayback, renewalValidationRequested {
                renewalValidationRequested = false
                beginRenewal(configuration: configuration, session: session)
                return true
            }
#endif
            if session.isExpired {
                beginRenewal(configuration: configuration, session: session)
            } else {
                accessToken = session.accessToken
                lifecycleState = .buffering
                emitChange()
                connect(with: session.accessToken)
            }
            return true
        } else if let accessToken {
            connect(with: accessToken)
            return true
        } else {
            // No credentials means no connection attempt, not transient buffering.
            guard accessToken != nil || allowAuthorization else {
                lifecycleState = .unauthorized
                emitChange()
                return false
            }
            return beginAuthorization(configuration: configuration)
        }
    }

    private func beginAuthorization(configuration: SPTConfiguration) -> Bool {
        lifecycleState = .buffering
        emitChange()
        guard let (manager, effect) = makeSessionManager(configuration: configuration, kind: .authorizing) else {
            lifecycleState = .unavailable
            emitChange()
            return false
        }
        authorizationGeneration = effect.id
        manager.initiateSession(with: Self.playbackScopes.union(.appRemoteControl), options: .default, campaign: nil)
        return true
    }

    private func beginRenewal(configuration: SPTConfiguration, session: SPTSession) {
#if DEBUG
        print("spotify_renewal_start has_refresh_token=\(!session.refreshToken.isEmpty)")
#endif
        guard let (sessionManager, effect) = makeSessionManager(
            configuration: configuration,
            kind: .renewing
        ) else {
            renewalRetryPolicy.renewalFailed()
            lifecycleState = .unavailable
            emitChange()
            return
        }
        sessionManager.session = session
        accessToken = nil
        authorizationGeneration = effect.id
        lifecycleState = .buffering
        emitChange()
        sessionManager.renewSession()
        beginAuthorizationTimeout(effect, kind: .renewing)
    }

    private func makeSessionManager(
        configuration: SPTConfiguration,
        kind: MobileMusicProviderAuthorizationKind
    ) -> (SPTSessionManager, MobileMusicAuthorizationEffect)? {
        guard let effect = lifecycle.beginAuthorizationEffect(kind: kind, nowMs: connectionNowMs) else {
            return nil
        }
        let bridge = SessionManagerBridge(owner: self, generation: effect.id)
        let manager = SPTSessionManager(configuration: configuration, delegate: bridge)
        sessionManagerBridge = bridge
        sessionManager = manager
        return (manager, effect)
    }

    private func beginAuthorizationTimeout(
        _ effect: MobileMusicAuthorizationEffect,
        kind: MobileMusicProviderAuthorizationKind
    ) {
        guard kind == .renewing else { return }
        effects.run(
            .authorization(effect.id),
            until: effect.deadlineMs,
            nowMs: { [weak self] in self?.connectionNowMs ?? effect.deadlineMs }
        ) { [weak self] in
            guard let self else { return }
            let transaction = self.finishAuthorizationTransaction(generation: effect.id)
            guard transaction != .stale else { return }
            if transaction == .renewing {
                // A timed-out renewal does not prove the saved session was revoked.
                self.renewalRetryPolicy.renewalFailed()
                self.authorizationNeedsUserAction = false
                self.lifecycleState = .stale
            } else {
                self.authorizationNeedsUserAction = true
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
        onChange = nil
        webPollTask?.cancel()
        webPollTask = nil
        webCommandTask?.cancel()
        webCommandTask = nil
        webArtworkTask?.cancel()
        webArtworkTask = nil
        webPlayback = nil
        webRenewalAttempted = false
        appRemote?.playerAPI?.delegate = nil
        appRemote?.delegate = nil
        appRemote?.disconnect()
        appRemote = nil
        appRemoteBridge = nil
        if let appRemoteGeneration {
            transport.apply(lifecycle.retireProviderSession(id: appRemoteGeneration))
        }
        appRemoteGeneration = nil
        playerStateRequestID = nil
        playerState = nil
        artwork = nil
        invalidateArtworkRequest()
        // Stopping observation must not cancel an authorization handoff. A
        // foreground/background transition can happen while Spotify is open.
        lifecycleState = .disconnected
    }

    public func applySuspension(_ suspension: MobileMusicProviderSuspension) {
        transport.apply(suspension)
    }

    /// Reconnects an existing App Remote session after a provider-side
    /// disconnect. Missing or rejected credentials never trigger an auth loop;
    /// the next explicit setup starts authorization again.
    public func ensureConnection() {
        guard onChange != nil,
              authorizationGeneration == nil,
              appRemote?.isConnected != true,
              let configuration
        else { return }
        if let session {
            if session.isExpired {
                guard renewalRetryPolicy.automaticRenewalAllowed else { return }
                beginRenewal(configuration: configuration, session: session)
            } else {
                accessToken = session.accessToken
                connect(with: session.accessToken)
            }
        } else if let accessToken {
            connect(with: accessToken)
        }
    }

    public var monitoringWorkState: MobileMusicProviderWorkState {
        if onChange == nil { return .unavailable }
        if authorizationNeedsUserAction { return .requiresUserAction }
        if authorizationGeneration != nil { return .authorizationPending }
        if selectedPlaybackTransport == .webApi, accessToken != nil, lifecycleState != .unavailable { return .remotePolling }
        if appRemote?.isConnected == true { return .active }
        if lifecycleState == .unavailable { return .unavailable }
        if accessToken != nil { return .credentialsAvailable }
        return .unavailable
    }

    private func makeAppRemote(
        _ configuration: SPTConfiguration,
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId
    ) -> SPTAppRemote {
        invalidateArtworkRequest()
        establishedConnectionID = nil
        if let previous = appRemote {
            appRemote = nil
            previous.playerAPI?.delegate = nil
            previous.delegate = nil
            previous.disconnect()
        }
        let appRemote = SPTAppRemote(configuration: configuration, logLevel: .error)
        self.appRemote = appRemote
        let bridge = AppRemoteBridge(
            owner: self,
            providerGeneration: providerGeneration,
            attemptID: attemptID
        )
        appRemoteBridge = bridge
        appRemote.delegate = bridge
        return appRemote
    }

    private func connect(with accessToken: String) {
        guard let configuration,
              let providerGeneration = appRemoteGeneration,
              lifecycle.classifyProviderSession(id: providerGeneration) == .current
        else { return }
        let attemptEffect: MobileMusicProviderConnectionAttemptEffect
        switch lifecycle.beginConnectionAttempt(nowMs: connectionNowMs) {
        case let .started(effect):
            attemptEffect = effect
        case .pending, .waitingToRetry:
            return
        case .exhausted:
            lifecycleState = .unavailable
            emitChange()
            return
        }
        let attemptID = attemptEffect.attemptId
        transport.apply(attemptEffect.transport)
        let appRemote = makeAppRemote(
            configuration,
            providerGeneration: providerGeneration,
            attemptID: attemptID
        )
        appRemote.connectionParameters.accessToken = accessToken
        appRemote.connect()
    }

    /// Polls the current Spotify player state so a track that was already
    /// playing before App Remote connected is reflected without waiting for a
    /// change notification.
    public func refreshPlayerState() {
        if selectedPlaybackTransport == .webApi {
            refreshWebPlayback()
            return
        }
        guard appRemote?.isConnected == true else { return }
        let nowMs = connectionNowMs
        if lifecycle.isPlayerStateStale(nowMs: nowMs), lifecycleState != .stale {
            lifecycleState = .stale
            emitChange()
        }
        guard let playerAPI = appRemote?.playerAPI,
              let bridge = appRemoteBridge,
              let requestID = lifecycle.beginPlayerStateRequest(nowMs: nowMs) else { return }
        let observationRevision = lifecycle.playerStateObservationRevision()
        playerStateRequestID = requestID
        let providerGeneration = bridge.providerGeneration
        let attemptID = bridge.attemptID
        playerAPI.getPlayerState { [weak self] result, error in
            let playerState = result as? SPTAppRemotePlayerState
            let errorInfo = (error as NSError?).map { ($0.domain, $0.code) }
            Task { @MainActor [weak self] in
                guard let self,
                      self.lifecycle.classifyProviderSession(id: providerGeneration) == .current,
                      self.lifecycle.classifyConnection(id: attemptID, nowMs: self.connectionNowMs) == .accepted else { return }
                let completion = self.lifecycle.completePlayerStateRequestIfCurrent(
                    id: requestID,
                    observationRevision: observationRevision,
                    nowMs: self.connectionNowMs
                )
                guard completion == .accepted else {
                    if self.playerStateRequestID == requestID {
                        self.playerStateRequestID = nil
                    }
                    return
                }
                if self.playerStateRequestID == requestID {
                    self.playerStateRequestID = nil
                }
                if let playerState, errorInfo == nil {
                    self.handlePlayerStateDidChange(
                        playerState,
                        providerGeneration: providerGeneration,
                        attemptID: attemptID
                    )
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

    private func refreshWebPlayback() {
        guard onChange != nil, authorizationGeneration == nil, webPollTask == nil,
              let accessToken, let generation = appRemoteGeneration,
              lifecycle.classifyProviderSession(id: generation) == .current,
              let requestID = lifecycle.beginPlayerStateRequest(nowMs: connectionNowMs) else { return }
        let revision = lifecycle.playerStateObservationRevision()
        webPollTask = Task { [weak self, playbackAPI] in
            guard let self else { return }
            defer { if !Task.isCancelled { self.webPollTask = nil } }
            do {
                let playback = try await playbackAPI.playback(accessToken: accessToken)
                guard !Task.isCancelled,
                      self.lifecycle.classifyProviderSession(id: generation) == .current else { return }
                guard self.lifecycle.completePlayerStateRequestIfCurrent(
                    id: requestID, observationRevision: revision, nowMs: self.connectionNowMs
                ) == .accepted else {
                    self.lifecycleState = .stale
                    self.emitChange()
                    return
                }
                guard self.accessToken == accessToken else { return }
                let previousURI = self.webPlayback?.item?.uri
                self.webPlayback = playback
                if previousURI != playback?.item?.uri {
                    self.webArtworkTask?.cancel()
                    self.webArtworkTask = nil
                    self.lifecycle.resetArtwork()
                    self.artwork = self.artworkCache.cachedArtwork(for: playback?.item?.uri)
                }
                self.webRenewalAttempted = false
                self.lifecycle.markPlayerStateObserved(nowMs: self.connectionNowMs)
                self.lifecycleState = playback.map { $0.isPlaying ? .playing : .paused } ?? .stopped
#if DEBUG
                print("spotify_web_playback_received playing=\(playback?.isPlaying ?? false) has_item=\(playback?.item != nil)")
#endif
                self.emitChange()
                self.refreshWebArtwork(generation: generation)
            } catch {
                guard !Task.isCancelled,
                      self.lifecycle.classifyProviderSession(id: generation) == .current else { return }
                _ = self.lifecycle.completePlayerStateRequestIfCurrent(
                    id: requestID, observationRevision: revision, nowMs: self.connectionNowMs
                )
                guard self.accessToken == accessToken else { return }
                self.handleWebFailure(error)
                if case let SpotifyPlaybackAPI.Failure.rateLimited(seconds) = error {
                    // Honor the server's Retry-After while this one cancellable
                    // request owns the poll slot. Never retry a playback command.
                    try? await Task.sleep(for: .seconds(seconds))
                }
            }
        }
    }

    private func refreshWebArtwork(generation: MobileMusicProviderSessionId) {
        guard artwork == nil, webArtworkTask == nil,
              let uri = webPlayback?.item?.uri, let url = webPlayback?.artworkURL,
              let effect = lifecycle.beginArtworkEffect(providerGeneration: generation, nowMs: connectionNowMs) else { return }
        webArtworkTask = Task { [weak self, playbackAPI] in
            let artwork = try? await playbackAPI.artwork(url: url)
            guard !Task.isCancelled, let self,
                  self.lifecycle.classifyProviderSession(id: generation) == .current,
                  self.webPlayback?.item?.uri == uri else { return }
            guard self.lifecycle.completeArtworkRequest(providerGeneration: generation, id: effect.id) == .accepted else { return }
            self.webArtworkTask = nil
            if let artwork {
                self.artworkCache.insert(artwork, for: uri)
                self.artwork = artwork
                self.emitChange()
            }
        }
    }

    private func handleWebFailure(_ error: Error) {
        switch error {
        case SpotifyPlaybackAPI.Failure.unauthorized:
            if !webRenewalAttempted, let configuration, let session {
                webRenewalAttempted = true
                beginRenewal(configuration: configuration, session: session)
                return
            }
            // A failed access token does not revoke the persisted refresh grant.
            lifecycleState = .unavailable
        case SpotifyPlaybackAPI.Failure.forbidden:
            lifecycleState = .unavailable
        default:
            lifecycleState = .stale
        }
#if DEBUG
        let code = (error as? SpotifyPlaybackAPI.Failure).map { String(describing: $0) } ?? "network"
        print("spotify_web_playback_failed reason=\(code)")
#endif
        emitChange()
    }

    private func performWebCommand(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        guard lifecycleState == .playing || lifecycleState == .paused,
              let deviceID = webPlayback?.device?.id, let accessToken,
              let generation = appRemoteGeneration,
              lifecycle.classifyProviderSession(id: generation) == .current else { return .unavailable }
        return await transport.perform(owner: .provider(providerGeneration: generation), command: command,
                                       onTerminal: { [weak self] _ in
                                           self?.webCommandTask?.cancel()
                                           self?.webCommandTask = nil
                                       }) { [weak self, playbackAPI] _, completion in
            guard let self else { completion(false); return }
            self.webCommandTask = Task { [weak self] in
                do {
                    try await playbackAPI.perform(command, accessToken: accessToken, deviceID: deviceID)
                    guard !Task.isCancelled else { return }
                    completion(true)
                } catch {
                    guard !Task.isCancelled, let self,
                          self.lifecycle.classifyProviderSession(id: generation) == .current else { return }
                    if self.accessToken == accessToken {
                        self.handleWebFailure(error)
                    }
                    completion(false)
                }
            }
        }
    }

    /// Handles the redirect URL returned by Spotify authorization.
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
              lifecycle.classifyAuthorization(id: generation) != .stale,
              let sessionManager else { return false }
        return sessionManager.application(UIApplication.shared, open: url, options: [:])
    }

    private nonisolated func enqueueSession(_ session: SPTSession, generation: MobileMusicAuthorizationId) {
        Task { @MainActor [weak self] in
            self?.acceptSession(generation: generation, session: session)
        }
    }

    private nonisolated func enqueueAuthorizationFailure(
        generation: MobileMusicAuthorizationId,
        domain: String,
        code: Int,
        classification: SpotifyAuthorizationFailure.Classification
    ) {
        Task { @MainActor [weak self] in
            self?.authorizationDidFail(
                generation: generation,
                domain: domain,
                code: code,
                classification: classification
            )
        }
    }

    private func acceptSession(generation: MobileMusicAuthorizationId, session: SPTSession) {
        let transaction = finishAuthorizationTransaction(generation: generation)
        guard transaction != .stale else { return }
#if DEBUG
        print("spotify_session_accepted transaction=\(transaction) has_refresh_token=\(!session.refreshToken.isEmpty) session_expired=\(session.isExpired)")
#endif
        self.session = session
        renewalRetryPolicy.renewalSucceeded()
        Self.storeSession(session)
        self.accessToken = session.accessToken
        // Keep the short-lived App Remote credential as a fallback as well as
        // the refreshable SDK session. This preserves an already-authorized
        // account even if a future SDK/session archive cannot be restored.
        Self.storeAccessToken(session.accessToken)
        authorizationNeedsUserAction = false
        guard onChange != nil else { return }
        connect(with: session.accessToken)
    }

    private func authorizationDidFail(
        generation: MobileMusicAuthorizationId,
        domain: String,
        code: Int,
        classification: SpotifyAuthorizationFailure.Classification
    ) {
        let transaction = finishAuthorizationTransaction(generation: generation)
        guard transaction != .stale else { return }
        let permanent = classification == .rejectedGrant
        if SpotifyAuthorizationFailure.preservesSavedSession(
            isRenewal: transaction == .renewing, hasSavedSession: session != nil, rejectedGrant: permanent
        ) {
            // Keep the grant. Retry only after monitoring restarts or the user
            // explicitly reconnects; the monitor loop must not renew each tick.
            renewalRetryPolicy.renewalFailed()
            authorizationNeedsUserAction = false
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
        print("spotify_authorization_failed transaction=\(transaction) domain=\(domain) code=\(code) permanent=\(permanent)")
#endif
        emitChange()
    }

    private func finishAuthorizationTransaction(generation: MobileMusicAuthorizationId) -> MobileMusicProviderAuthorizationMatch {
        guard authorizationGeneration == generation else { return .stale }
        let outcome = lifecycle.finishAuthorization(id: generation)
        guard outcome != .stale else { return .stale }
        effects.cancel(.authorization(generation))
        sessionManagerBridge?.owner = nil
        sessionManagerBridge = nil
        sessionManager = nil
        authorizationGeneration = nil
        return outcome
    }

    private func invalidateAuthorizationTransaction() {
        lifecycle.invalidateAuthorization()
        if let authorizationGeneration {
            effects.cancel(.authorization(authorizationGeneration))
        }
        sessionManagerBridge?.owner = nil
        sessionManagerBridge = nil
        sessionManager = nil
        authorizationGeneration = nil
    }

    private nonisolated func enqueueConnectionEstablished(
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId
    ) {
        Task { @MainActor [weak self] in
            self?.handleAppRemoteDidEstablishConnection(
                providerGeneration: providerGeneration,
                attemptID: attemptID
            )
        }
    }

    private nonisolated func enqueueConnectionFailure(
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId,
        errorDomain: String?,
        errorCode: Int?,
        underlyingDomain: String?,
        underlyingCode: Int?,
        rootDomain: String?,
        rootCode: Int?
    ) {
        Task { @MainActor [weak self] in
            self?.handleAppRemoteConnectionFailure(
                providerGeneration: providerGeneration,
                attemptID: attemptID,
                errorDomain: errorDomain,
                errorCode: errorCode,
                underlyingDomain: underlyingDomain,
                underlyingCode: underlyingCode,
                rootDomain: rootDomain,
                rootCode: rootCode
            )
        }
    }

    private nonisolated func enqueueDisconnect(
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId,
        errorDomain: String?,
        errorCode: Int?
    ) {
        Task { @MainActor [weak self] in
            self?.handleAppRemoteDidDisconnect(
                providerGeneration: providerGeneration,
                attemptID: attemptID,
                errorDomain: errorDomain,
                errorCode: errorCode
            )
        }
    }

    private nonisolated func enqueuePlayerState(
        _ playerState: SPTAppRemotePlayerState,
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId
    ) {
        Task { @MainActor [weak self] in
            self?.handlePlayerStateDidChange(
                playerState,
                providerGeneration: providerGeneration,
                attemptID: attemptID
            )
        }
    }

    @MainActor
    public func perform(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        if case .openProvider = command {
            guard UIApplication.shared.canOpenURL(Self.providerURL) else { return .unavailable }
            guard await UIApplication.shared.open(Self.providerURL) else { return .failed }
            return .accepted
        }
        if selectedPlaybackTransport == .webApi {
            return await performWebCommand(command)
        }
        guard lifecycleState == .playing || lifecycleState == .paused,
              let playerAPI = appRemote?.playerAPI,
              let bridge = appRemoteBridge,
              let connectionID = establishedConnectionID,
              lifecycle.classifyProviderSession(id: bridge.providerGeneration) == .current,
              lifecycle.classifyConnection(id: bridge.attemptID, nowMs: connectionNowMs) == .accepted
        else { return .unavailable }
        return await transport.perform(
            owner: .connection(
                providerGeneration: bridge.providerGeneration,
                connectionId: connectionID
            ),
            command: command
        ) { _, completion in
            let callback: SPTAppRemoteCallback = { _, error in
                Task { @MainActor in completion(error == nil) }
            }
            switch command {
            case .previous: playerAPI.skip(toPrevious: callback)
            case .play: playerAPI.resume(callback)
            case .pause: playerAPI.pause(callback)
            case .next: playerAPI.skip(toNext: callback)
            case .openProvider: completion(false)
            }
        }
    }

    public func observation(observedAtMs: UInt64) -> MusicProviderObservation {
        if selectedPlaybackTransport == .webApi, let webPlayback {
            return MusicProviderObservation(snapshot: webPlayback.snapshot(state: lifecycleState, observedAtMs: observedAtMs), artwork: artwork)
        }
        let controlsAvailable = lifecycleState == .playing || lifecycleState == .paused
        let snapshot = MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: selectedPlaybackTransport == .webApi ? "spotify-web-api" : "spotify-app-remote",
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
        return MusicProviderObservation(snapshot: snapshot, artwork: artwork)
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

    private func handleAppRemoteDidEstablishConnection(
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId
    ) {
        guard let appRemote = self.appRemote,
              let bridge = appRemoteBridge,
              bridge.providerGeneration == providerGeneration,
              bridge.attemptID == attemptID,
              lifecycle.classifyProviderSession(id: providerGeneration) == .current,
              onChange != nil else { return }
        guard let connectionID = lifecycle.connectionEstablished(id: attemptID, nowMs: connectionNowMs) else {
            // This callback belongs to the still-installed SDK object, but
            // Rust has already expired its attempt. Disconnect that object so
            // `isConnected` cannot permanently block the next retry.
            appRemote.disconnect()
            let expired = lifecycle.connectionExpiredEffect(id: attemptID, nowMs: connectionNowMs)
            guard expired.callback == .accepted else { return }
            transport.apply(expired.transport)
            detachAppRemote(providerGeneration: providerGeneration, attemptID: attemptID)
            lifecycleState = .disconnected
            emitChange()
            return
        }
        establishedConnectionID = connectionID
#if DEBUG
        print("spotify_connection_established")
#endif
        // A connected session that never returns player state must not remain
        // in buffering forever. Verified callbacks refresh this same window.
        lifecycle.markPlayerStateObserved(nowMs: connectionNowMs)
        lifecycleState = .buffering
        appRemote.playerAPI?.delegate = appRemoteBridge
        appRemote.playerAPI?.subscribe(toPlayerState: { [weak self] result, error in
            let state = result as? SPTAppRemotePlayerState
            let hasError = error != nil
            Task { @MainActor [weak self] in
                guard let self,
                      self.lifecycle.classifyProviderSession(id: providerGeneration) == .current,
                      self.lifecycle.classifyConnection(id: attemptID, nowMs: self.connectionNowMs) == .accepted else { return }
                if hasError {
                    self.lifecycleState = .stale
                    self.emitChange()
                } else if let state {
                    self.handlePlayerStateDidChange(
                        state,
                        providerGeneration: providerGeneration,
                        attemptID: attemptID
                    )
                }
            }
        })
        refreshPlayerState()
        emitChange()
    }

    private func handleAppRemoteConnectionFailure(
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId,
        errorDomain: String?,
        errorCode: Int?,
        underlyingDomain: String?,
        underlyingCode: Int?,
        rootDomain: String?,
        rootCode: Int?
    ) {
        guard lifecycle.classifyProviderSession(id: providerGeneration) == .current else { return }
        let connection = lifecycle.connectionFailedEffect(id: attemptID, nowMs: connectionNowMs)
        guard connection.callback == .accepted else { return }
        transport.apply(connection.transport)
        detachAppRemote(providerGeneration: providerGeneration, attemptID: attemptID)
        // App Remote reports transport and wakeup failures here too. A generic
        // connection failure is not evidence that the credential was rejected.
        lifecycleState = .disconnected
#if DEBUG
        if let errorDomain, let errorCode {
            let cause = "underlying_domain=\(underlyingDomain ?? "-") underlying_code=\(underlyingCode.map { String($0) } ?? "-")"
            let rootCause = "root_domain=\(rootDomain ?? "-") root_code=\(rootCode.map { String($0) } ?? "-")"
            print("spotify_connection_failed domain=\(errorDomain) code=\(errorCode) \(cause) \(rootCause)")
        }
#endif
        emitChange()
        if selectedPlaybackTransport == .webApi { refreshWebPlayback() }
    }

    private func handleAppRemoteDidDisconnect(
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId,
        errorDomain: String?,
        errorCode: Int?
    ) {
        guard lifecycle.classifyProviderSession(id: providerGeneration) == .current else { return }
        let connection = lifecycle.connectionDisconnectedEffect(id: attemptID, nowMs: connectionNowMs)
        guard connection.callback == .accepted else { return }
        transport.apply(connection.transport)
        detachAppRemote(providerGeneration: providerGeneration, attemptID: attemptID)
        lifecycleState = .disconnected
#if DEBUG
        if let errorDomain, let errorCode {
            print("spotify_disconnected domain=\(errorDomain) code=\(errorCode)")
        } else {
            print("spotify_disconnected without_error")
        }
#endif
        emitChange()
        if selectedPlaybackTransport == .webApi { refreshWebPlayback() }
    }

    private func detachAppRemote(providerGeneration: MobileMusicProviderSessionId, attemptID: MobileMusicConnectionAttemptId) {
        guard let bridge = appRemoteBridge,
              bridge.providerGeneration == providerGeneration,
              bridge.attemptID == attemptID else { return }
        playerStateRequestID = nil
        appRemote?.playerAPI?.delegate = nil
        appRemote?.delegate = nil
        appRemoteBridge?.owner = nil
        appRemote = nil
        appRemoteBridge = nil
        establishedConnectionID = nil
        invalidateArtworkRequest()
        playerState = nil
        artwork = nil
    }

    private func handlePlayerStateDidChange(
        _ playerState: SPTAppRemotePlayerState,
        providerGeneration: MobileMusicProviderSessionId,
        attemptID: MobileMusicConnectionAttemptId
    ) {
        guard onChange != nil,
              appRemote != nil,
              lifecycle.classifyProviderSession(id: providerGeneration) == .current,
              lifecycle.classifyConnection(id: attemptID, nowMs: connectionNowMs) == .accepted else { return }
        let trackChanged = self.playerState?.track.uri != playerState.track.uri
#if DEBUG
        if trackChanged {
            print("spotify_player_state uri_bytes=\(playerState.track.uri.utf8.count) title_bytes=\(playerState.track.name.utf8.count) artist_bytes=\(playerState.track.artist.name.utf8.count) position=\(playerState.playbackPosition) duration=\(playerState.track.duration)")
        }
#endif
        self.lifecycle.markPlayerStateObserved(nowMs: connectionNowMs)
        self.playerState = playerState
        lifecycleState = playerState.isPaused ? .paused : .playing
        if trackChanged {
            artwork = artworkCache.cachedArtwork(for: playerState.track.uri)
            invalidateArtworkRequest()
            if artwork == nil { requestArtwork(for: playerState.track, generation: providerGeneration) }
        } else if artwork == nil {
            requestArtwork(for: playerState.track, generation: providerGeneration)
        }
        emitChange()
    }

    private func invalidateArtworkRequest() {
        if let requestID = artworkRequest?.id {
            effects.cancel(.artwork(requestID))
        }
        if let artworkRetryID {
            effects.cancel(.artworkRetry(artworkRetryID))
        }
        lifecycle.resetArtwork()
        artworkRequest = nil
        artworkRetryID = nil
    }

    private func requestArtwork(
        for track: SPTAppRemoteTrack,
        generation: MobileMusicProviderSessionId
    ) {
        guard lifecycle.classifyProviderSession(id: generation) == .current,
              artworkRequest == nil,
              artworkRetryID == nil,
              let imageAPI = appRemote?.imageAPI,
              let effect = lifecycle.beginArtworkEffect(
                  providerGeneration: generation,
                  nowMs: connectionNowMs
              ) else { return }
        artworkRetryID = nil
        let trackURI = track.uri
        artworkRequest = (effect.id, trackURI, generation)
        beginArtworkDeadline(effect: effect, track: track, generation: generation)
        imageAPI.fetchImage(forItem: track, with: Self.artworkSize) { [weak self] image, error in
            let failed = error != nil
            Task { @MainActor [weak self, image, failed] in
                let artwork = (image as? UIImage)?.cgImage.flatMap(MusicArtwork.init(image:))
                guard let self,
                      let request = self.artworkRequest,
                      request.id == effect.id,
                      request.trackURI == trackURI,
                      request.generation == generation,
                      self.lifecycle.classifyProviderSession(id: generation) == .current,
                      self.playerState?.track.uri == trackURI else { return }
                guard self.lifecycle.completeArtworkRequest(
                    providerGeneration: generation,
                    id: effect.id
                ) == .accepted else { return }
                self.artworkRequest = nil
                self.effects.cancel(.artwork(effect.id))
                if !failed, let artwork {
                    self.artworkCache.insert(artwork, for: trackURI)
                    self.artwork = artwork
                    self.emitChange()
                } else {
                    self.scheduleArtworkRetry(track: track, generation: generation)
                }
            }
        }
    }

    private func beginArtworkDeadline(
        effect: MobileMusicArtworkEffect,
        track: SPTAppRemoteTrack,
        generation: MobileMusicProviderSessionId
    ) {
        effects.run(
            .artwork(effect.id),
            until: effect.deadlineMs,
            nowMs: { [weak self] in self?.connectionNowMs ?? effect.deadlineMs }
        ) { [weak self] in
            guard let self,
                  let request = self.artworkRequest,
                  request.id == effect.id,
                  request.trackURI == track.uri,
                  request.generation == generation,
                  self.lifecycle.completeArtworkRequest(
                      providerGeneration: generation,
                      id: effect.id
                  ) == .accepted else { return }
            self.artworkRequest = nil
            self.scheduleArtworkRetry(track: track, generation: generation)
        }
    }

    private func scheduleArtworkRetry(
        track: SPTAppRemoteTrack,
        generation: MobileMusicProviderSessionId
    ) {
        guard let effect = lifecycle.beginArtworkRetryEffect(
            providerGeneration: generation,
            nowMs: connectionNowMs
        ) else { return }
        artworkRetryID = effect.id
        effects.run(
            .artworkRetry(effect.id),
            until: effect.deadlineMs,
            nowMs: { [weak self] in self?.connectionNowMs ?? effect.deadlineMs }
        ) { [weak self] in
            guard let self,
                  self.lifecycle.completeArtworkRetry(
                      providerGeneration: generation,
                      id: effect.id
                  ) == .current,
                  self.lifecycle.classifyProviderSession(id: generation) == .current,
                  self.playerState?.track.uri == track.uri else { return }
            self.artworkRetryID = nil
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

    public init(lifecycle: MobileMusicProviderLifecycle = MobileMusicProviderLifecycle()) {
        _ = lifecycle
    }

    public init(
        lifecycle: MobileMusicProviderLifecycle,
        effects: MusicProviderEffectExecutor
    ) {
        _ = lifecycle
        _ = effects
    }

    public func applySuspension(_ suspension: MobileMusicProviderSuspension) {
        _ = suspension
    }

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
