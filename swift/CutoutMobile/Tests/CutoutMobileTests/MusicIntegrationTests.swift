import CoreGraphics
import CutoutMobileFFI
import XCTest

@testable import CutoutMobile

final class MusicIntegrationTests: XCTestCase {
    func testSpotifyPlaybackTransportPrefersAppRemoteAndUsesWebAsFallback() {
        XCTAssertEqual(musicPlaybackTransport(appRemoteConnected: true, webPlaybackAuthorized: true), .appRemote)
        XCTAssertEqual(musicPlaybackTransport(appRemoteConnected: true, webPlaybackAuthorized: false), .appRemote)
        XCTAssertEqual(musicPlaybackTransport(appRemoteConnected: false, webPlaybackAuthorized: true), .webApi)
        XCTAssertEqual(musicPlaybackTransport(appRemoteConnected: false, webPlaybackAuthorized: false), .unavailable)
    }

    func testSpotifyStartupPlayerAppearsOnlyBeforeTheFirstSnapshot() {
        XCTAssertTrue(shouldShowSpotifyStartupPlayer(selectedProvider: .spotify, nowPlaying: nil))
        XCTAssertFalse(
            shouldShowSpotifyStartupPlayer(
                selectedProvider: .spotify,
                nowPlaying: MusicNowPlaying(provider: .spotify, state: .unauthorized)
            ))
        XCTAssertFalse(shouldShowSpotifyStartupPlayer(selectedProvider: .appleMusic, nowPlaying: nil))
    }

    func testSpotifyHandoffPlaybackConfirmationUsesRustIdentityAndDeadline() throws {
        let lifecycle = MobileMusicProviderLifecycle()
        let first = try XCTUnwrap(lifecycle.beginPlayHandoff(nowMs: 100))
        XCTAssertNil(lifecycle.observePlayHandoff(id: first.id, state: .playing, observedAtMs: 99))
        XCTAssertNil(lifecycle.observePlayHandoff(id: first.id, state: .paused, observedAtMs: 110))
        XCTAssertEqual(lifecycle.observePlayHandoff(id: first.id, state: .playing, observedAtMs: 120), .accepted)
        XCTAssertNil(lifecycle.expirePlayHandoff(id: first.id, nowMs: first.deadlineMs))

        let second = try XCTUnwrap(lifecycle.beginPlayHandoff(nowMs: 200))
        XCTAssertNotEqual(first.id, second.id)
        XCTAssertNil(lifecycle.observePlayHandoff(id: first.id, state: .playing, observedAtMs: 210))
        XCTAssertNil(lifecycle.expirePlayHandoff(id: second.id, nowMs: second.deadlineMs - 1))
        XCTAssertEqual(lifecycle.expirePlayHandoff(id: second.id, nowMs: second.deadlineMs), .timedOut)
    }

    func testSpotifyRenewalFailureOnlyRequiresNewAuthorizationForRejectedCredentials() {
        XCTAssertFalse(
            SpotifyAuthorizationFailure.requiresNewAuthorization(
                domain: "NSURLErrorDomain", code: -1202, description: "Server certificate revoked"
            ))
        XCTAssertFalse(
            SpotifyAuthorizationFailure.requiresNewAuthorization(
                domain: "SPTSessionManagerErrorDomain",
                code: -1,
                description: "access token expired during a network timeout"
            ))
        XCTAssertFalse(
            SpotifyAuthorizationFailure.requiresNewAuthorization(
                domain: "NSURLErrorDomain",
                code: -1009,
                description: "The Internet connection appears to be offline"
            ))
        XCTAssertFalse(
            SpotifyAuthorizationFailure.requiresNewAuthorization(
                domain: "SPTSessionManagerErrorDomain",
                code: 401,
                description: "Unauthorized token refresh endpoint"
            ))
        XCTAssertFalse(
            SpotifyAuthorizationFailure.requiresNewAuthorization(
                domain: "SPTSessionManagerErrorDomain",
                code: 401,
                description: "Invalid token"
            ))
        XCTAssertTrue(
            SpotifyAuthorizationFailure.requiresNewAuthorization(
                domain: "SPTSessionManagerErrorDomain",
                code: 401,
                description: "invalid_grant"
            ))
        XCTAssertTrue(
            SpotifyAuthorizationFailure.requiresNewAuthorization(
                domain: "SPTSessionManagerErrorDomain",
                code: -1,
                description: "Refresh token revoked"
            ))
    }

    func testSpotifyRenewalFailureClassifiesWrappedRejectedGrantAndTransientCause() {
        let rejectedGrant = NSError(
            domain: "SpotifyOAuthErrorDomain",
            code: 400,
            userInfo: [NSLocalizedDescriptionKey: "invalid_grant"]
        )
        let wrappedGrant = NSError(
            domain: "SPTSessionManagerErrorDomain",
            code: -1,
            userInfo: [NSLocalizedDescriptionKey: "Renew session failed", NSUnderlyingErrorKey: rejectedGrant]
        )
        XCTAssertEqual(SpotifyAuthorizationFailure.classify(wrappedGrant), .rejectedGrant)

        let networkError = NSError(
            domain: NSURLErrorDomain,
            code: NSURLErrorTimedOut,
            userInfo: [NSLocalizedDescriptionKey: "The request timed out"]
        )
        let wrappedNetwork = NSError(
            domain: "SPTSessionManagerErrorDomain",
            code: -1,
            userInfo: [NSLocalizedDescriptionKey: "Renew session failed", NSUnderlyingErrorKey: networkError]
        )
        XCTAssertEqual(SpotifyAuthorizationFailure.classify(wrappedNetwork), .recoverable)
    }

    func testSpotifyPermissionUpgradeFailureKeepsTheExistingGrant() {
        XCTAssertTrue(
            SpotifyAuthorizationFailure.preservesSavedSession(
                isRenewal: false, hasSavedSession: true, rejectedGrant: true))
        XCTAssertTrue(
            SpotifyAuthorizationFailure.preservesSavedSession(
                isRenewal: false, hasSavedSession: true, rejectedGrant: false))
        XCTAssertFalse(
            SpotifyAuthorizationFailure.preservesSavedSession(
                isRenewal: false, hasSavedSession: false, rejectedGrant: false))
        XCTAssertFalse(
            SpotifyAuthorizationFailure.preservesSavedSession(
                isRenewal: true, hasSavedSession: true, rejectedGrant: true))
        XCTAssertTrue(
            SpotifyAuthorizationFailure.preservesSavedSession(
                isRenewal: true, hasSavedSession: true, rejectedGrant: false))
    }

    func testSpotifyRenewalFailureWaitsForMonitoringRestartBeforeRetry() {
        var retryPolicy = SpotifyRenewalRetryPolicy()
        XCTAssertTrue(retryPolicy.automaticRenewalAllowed)

        retryPolicy.renewalFailed()
        for _ in 0..<5 {
            XCTAssertFalse(retryPolicy.automaticRenewalAllowed)
        }

        retryPolicy.beginMonitoring()
        XCTAssertTrue(retryPolicy.automaticRenewalAllowed)

        retryPolicy.renewalFailed()
        retryPolicy.renewalSucceeded()
        XCTAssertTrue(retryPolicy.automaticRenewalAllowed)
    }

    func testSpotifySettingsOfferReauthorizationOnlyForUnauthorizedSession() {
        XCTAssertFalse(
            MusicSettingsPresentation.showsReauthorize(
                provider: .spotify, state: nil
            ))
        XCTAssertFalse(
            MusicSettingsPresentation.showsReauthorize(
                provider: .spotify, state: .disconnected
            ))
        XCTAssertFalse(
            MusicSettingsPresentation.showsReauthorize(
                provider: .spotify, state: .stale
            ))
        XCTAssertTrue(
            MusicSettingsPresentation.showsReauthorize(
                provider: .spotify, state: .unauthorized
            ))
        XCTAssertFalse(
            MusicSettingsPresentation.showsReauthorize(
                provider: .appleMusic, state: .unauthorized
            ))
    }

    func testLostSpotifyUpdatesKeepTheTrackAndOfferReconnect() {
        let nowPlaying = MusicNowPlaying(
            provider: .spotify,
            state: .stale,
            item: .init(identifier: "spotify:track:example", title: "Last song", artist: "Artist")
        )
        XCTAssertEqual(nowPlaying.title, "Last song")
        XCTAssertEqual(nowPlaying.statusText, "Can’t get playback")
        XCTAssertFalse(nowPlaying.requiresSetup)
        XCTAssertEqual(
            MusicSettingsPresentation.connectionActionTitle(
                provider: nowPlaying.provider, state: nowPlaying.state
            ), "Reconnect Spotify")
        XCTAssertFalse(
            MusicSettingsPresentation.showsReauthorize(
                provider: nowPlaying.provider, state: nowPlaying.state
            ))
    }

    func testUnavailableMusicWithoutAPlayingItemDoesNotOccupyMapSpace() {
        XCTAssertFalse(MusicNowPlaying(provider: .spotify, state: .unavailable).showsCompactPlayer)
        for state in [MobileMusicPlaybackStateDto.stopped, .buffering, .interrupted, .disconnected, .stale] {
            let nowPlaying = MusicNowPlaying(provider: .spotify, state: state)
            XCTAssertTrue(nowPlaying.showsCompactPlayer, "keep the player during \(state)")
            XCTAssertFalse(nowPlaying.requiresSetup, "\(state) does not invalidate authorization")
        }
        XCTAssertTrue(
            MusicNowPlaying(
                provider: .spotify,
                state: .disconnected,
                item: MobileMusicItemDto(identifier: "track", title: "Song", artist: "Artist")
            ).showsCompactPlayer)
        XCTAssertTrue(MusicNowPlaying(provider: .spotify, state: .playing).showsCompactPlayer)
    }

    func testRecoverableSpotifyUnavailableStateKeepsPlayInsteadOfSetup() {
        let nowPlaying = MusicNowPlaying(
            provider: .spotify,
            state: .unavailable,
            item: .init(identifier: "spotify:track:example", title: "Last song", artist: "Artist"),
            capabilities: .init(previous: false, play: true, pause: false, next: false, openProvider: true)
        )

        XCTAssertTrue(nowPlaying.showsCompactPlayer)
        XCTAssertFalse(nowPlaying.requiresSetup)
        XCTAssertEqual(nowPlaying.playPauseCommand, .play)
        XCTAssertTrue(nowPlaying.isCommandAvailable(.play))
    }

    @MainActor
    private func makeCoordinator(
        rideMapState: MobileRideMapState?
    ) -> MusicIntegrationCoordinator {
        MusicIntegrationCoordinator(
            rideMapState: rideMapState,
            lifecycle: MobileMusicProviderLifecycle()
        )
    }

    func testMusicCommandFeedbackPresentsEveryNonAcceptedOutcome() {
        XCTAssertNil(MusicCommandFeedback(requestID: .init(value: 1), outcome: .accepted).messageKey)
        XCTAssertEqual(
            MusicCommandFeedback(requestID: .init(value: 2), outcome: .refused).messageKey,
            "music.command.refused"
        )
        XCTAssertEqual(
            MusicCommandFeedback(requestID: .init(value: 3), outcome: .failed).messageKey,
            "music.command.failed"
        )
        XCTAssertEqual(
            MusicCommandFeedback(requestID: .init(value: 4), outcome: .unavailable).messageKey,
            "music.command.unavailable"
        )
    }

    @MainActor
    func testOlderMusicCommandTaskCannotClearNewerTask() {
        let slot = MusicCommandTaskSlot()
        let firstID = slot.reserve()
        let firstTask = Task {}
        slot.install(firstTask, for: firstID)

        let secondID = slot.reserve()
        let secondTask = Task {}
        slot.install(secondTask, for: secondID)

        slot.finish(firstID)
        XCTAssertEqual(slot.currentID, secondID)

        slot.finish(secondID)
        XCTAssertNil(slot.currentID)
    }

    @MainActor
    func testProviderTransportCompletesMissingDelayedAndDuplicateCallbacksOnce() async throws {
        let lifecycle = MobileMusicProviderLifecycle()
        let coordinator = MusicTransportCoordinator(lifecycle: lifecycle)
        let provider = try! XCTUnwrap(lifecycle.beginProviderSession())
        var callbacks = [(MobileMusicTransportRequestId, MusicCommandOutcome)]()

        let first = try XCTUnwrap(
            lifecycle.beginTransportEffect(
                owner: .provider(providerGeneration: provider),
                command: .play,
                nowMs: 1_000
            ))
        XCTAssertTrue(
            coordinator.register(
                providerGeneration: provider,
                effect: first,
                completion: { callbacks.append(($0, $1)) }
            ))
        XCTAssertNil(
            lifecycle.beginTransportEffect(
                owner: .provider(providerGeneration: provider),
                command: .play,
                nowMs: 1_001
            ))
        coordinator.expire(providerGeneration: provider, requestID: first.id, nowMs: 10_999)
        XCTAssertTrue(callbacks.isEmpty)
        coordinator.expire(providerGeneration: provider, requestID: first.id, nowMs: 11_000)
        coordinator.finish(providerGeneration: provider, requestID: first.id, accepted: true)
        coordinator.finish(providerGeneration: provider, requestID: first.id, accepted: false)
        XCTAssertEqual(callbacks.map(\.1), [.failed])

        let second = try XCTUnwrap(
            lifecycle.beginTransportEffect(
                owner: .provider(providerGeneration: provider),
                command: .play,
                nowMs: 11_001
            ))
        XCTAssertTrue(
            coordinator.register(
                providerGeneration: provider,
                effect: second,
                completion: { callbacks.append(($0, $1)) }
            ))
        coordinator.finish(providerGeneration: provider, requestID: second.id, accepted: true)
        coordinator.finish(providerGeneration: provider, requestID: second.id, accepted: false)
        XCTAssertEqual(callbacks.map(\.1), [.failed, .accepted])
    }

    @MainActor
    func testProviderTransportDisconnectResumesPendingCommandAndRejectsLateCallback() throws {
        let lifecycle = MobileMusicProviderLifecycle()
        let coordinator = MusicTransportCoordinator(lifecycle: lifecycle)
        let provider = try! XCTUnwrap(lifecycle.beginProviderSession())
        var callbacks = [(MobileMusicTransportRequestId, MusicCommandOutcome)]()
        let request = try XCTUnwrap(
            lifecycle.beginTransportEffect(
                owner: .provider(providerGeneration: provider),
                command: .play,
                nowMs: 100
            ))
        XCTAssertTrue(
            coordinator.register(
                providerGeneration: provider,
                effect: request,
                completion: { callbacks.append(($0, $1)) }
            ))

        coordinator.apply(lifecycle.retireProviderSession(id: provider))
        coordinator.finish(providerGeneration: provider, requestID: request.id, accepted: true)

        XCTAssertEqual(callbacks.map(\.0), [request.id])
        XCTAssertEqual(callbacks.map(\.1), [.unavailable])
    }

    func testSpotifyCallbackAcceptsObservedRootSlashWithoutAcceptingAnotherPath() throws {
        let configured = try XCTUnwrap(URL(string: "cutout-spotify://spotify-login-callback"))
        let returned = try XCTUnwrap(URL(string: "cutout-spotify://spotify-login-callback/#access_token=test"))
        XCTAssertEqual(configured.scheme, returned.scheme)
        XCTAssertEqual(configured.host, returned.host)
        XCTAssertEqual(configured.path, "")
        XCTAssertEqual(returned.path, "/")
        XCTAssertTrue(musicCallbackPathMatches(expected: configured.path, actual: returned.path))
        XCTAssertFalse(musicCallbackPathMatches(expected: configured.path, actual: "/another-callback"))
    }

    @MainActor
    func testSpotifyEpisodeWithoutArtistStillUpdatesPlayingTitle() throws {
        let coordinator = makeCoordinator(rideMapState: nil)
        let snapshot = MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: "spotify-app-remote",
            state: .playing,
            item: .init(
                identifier: "spotify:episode:example",
                title: "Episode title",
                artist: ""
            ),
            positionMilliseconds: 1_777_678,
            durationMilliseconds: 3_783_235,
            observedAtMs: 1_100,
            capabilities: .init(previous: false, play: false, pause: true, next: true, openProvider: true)
        )
        _ = try coordinator.ingest(snapshot: snapshot, wallClockAtMs: 1_700_000_000_000, clockUncertaintyMs: 1)
        XCTAssertEqual(coordinator.nowPlaying?.title, "Episode title")
        XCTAssertEqual(coordinator.nowPlaying?.state, .playing)
        XCTAssertEqual(coordinator.nowPlaying?.playPauseCommand, .pause)
        XCTAssertNil(coordinator.nowPlaying?.item?.artist)
    }

    func testMissingSpotifyMetadataDoesNotHideConnectionStatus() {
        let disconnected = MusicNowPlaying(provider: .spotify, state: .disconnected)
        XCTAssertEqual(disconnected.title, pevLocalizedText("music.state.disconnected"))
        XCTAssertEqual(disconnected.statusText, pevLocalizedText("music.state.disconnected"))
        let playing = MusicNowPlaying(
            provider: .spotify, state: .playing,
            item: .init(identifier: "spotify:track:example", title: nil, artist: nil)
        )
        XCTAssertEqual(playing.title, pevLocalizedText("music.state.playing"))
        XCTAssertNil(playing.statusText)
    }

    func testMissingMetadataUsesPlaybackStateInsteadOfInventingStoppedPlayback() {
        for state in [MobileMusicPlaybackStateDto.playing, .paused, .buffering, .stale, .unauthorized] {
            let playing = MusicNowPlaying(provider: .spotify, state: state)
            XCTAssertEqual(playing.title, pevLocalizedText(musicPlaybackTitleKey(state: state)))
            XCTAssertNotEqual(playing.title, pevLocalizedText("music.not_playing"))
        }
    }

    func testProviderFailureStatesKeepSetupActionAvailable() {
        for state in [
            MobileMusicPlaybackStateDto.unauthorized,
            .unavailable,
        ] {
            XCTAssertTrue(
                MusicNowPlaying(provider: .spotify, state: state).requiresSetup,
                "expected setup action for \(state)"
            )
        }
        for state in [
            MobileMusicPlaybackStateDto.playing,
            .paused,
            .buffering,
            .interrupted,
            .stopped,
            .disconnected,
            .stale,
        ] {
            XCTAssertFalse(
                MusicNowPlaying(provider: .spotify, state: state).requiresSetup,
                "did not expect setup action for \(state)"
            )
        }
    }

    func testProviderMonitoringModeMatchesSupportedLifecycle() {
        XCTAssertEqual(
            MobileMusicProviderDto.appleMusic.monitoringMode,
            .appleMusicSystemPlayer
        )
        #if canImport(SpotifyiOS) && os(iOS)
            XCTAssertEqual(MobileMusicProviderDto.spotify.monitoringMode, .spotifyAppRemote)
        #else
            XCTAssertEqual(MobileMusicProviderDto.spotify.monitoringMode, .unavailable)
        #endif
    }

    func testPlayerStateFreshnessExpiresOnlyAfterRustObservationDeadline() {
        let lifecycle = MobileMusicProviderLifecycle()

        XCTAssertFalse(lifecycle.isPlayerStateStale(nowMs: 30_000))
        lifecycle.markPlayerStateObserved(nowMs: 1_000)
        XCTAssertFalse(lifecycle.isPlayerStateStale(nowMs: 31_000))
        XCTAssertTrue(lifecycle.isPlayerStateStale(nowMs: 31_001))

        lifecycle.markPlayerStateObserved(nowMs: 31_001)
        XCTAssertFalse(lifecycle.isPlayerStateStale(nowMs: 61_001))
        _ = lifecycle.beginProviderSession()
        _ = lifecycle.suspend()
        XCTAssertFalse(lifecycle.isPlayerStateStale(nowMs: UInt64.max))
    }

    @MainActor
    func testSpotifyTransportStaysUnavailableUntilAppRemoteIsProven() async {
        let adapter = SpotifyProviderAdapter()

        let outcomes = await [
            adapter.perform(.previous),
            adapter.perform(.play),
            adapter.perform(.pause),
            adapter.perform(.next),
        ]

        XCTAssertEqual(outcomes, [.unavailable, .unavailable, .unavailable, .unavailable])
    }

    func testMusicHistoryPolicyStoreDefaultsToDisabledAndRoundTrips() throws {
        let suiteName = "MusicHistoryPolicyStoreTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = MusicHistoryPolicyStore(defaults: defaults)

        XCTAssertEqual(store.policy, .disabled)
        for policy in MobileMusicHistoryPolicyDto.allCases {
            store.set(policy)
            XCTAssertEqual(store.policy, policy)
        }
    }

    func testMusicProviderSelectionStoreDefaultsToAppleMusicAndRoundTrips() throws {
        let suiteName = "MusicProviderSelectionStoreTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = MusicProviderSelectionStore(defaults: defaults)

        XCTAssertEqual(store.provider, .appleMusic)
        store.set(.spotify)
        XCTAssertEqual(store.provider, .spotify)
        store.set(.appleMusic)
        XCTAssertEqual(store.provider, .appleMusic)
    }

    func testMusicMonitoringPreferenceStoreDefaultsOffAndRoundTrips() throws {
        let suiteName = "MusicMonitoringPreferenceStoreTests-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let store = MusicMonitoringPreferenceStore(defaults: defaults)

        XCTAssertFalse(store.isEnabled)
        store.setEnabled(true)
        XCTAssertTrue(store.isEnabled)
        store.setEnabled(false)
        XCTAssertFalse(store.isEnabled)
    }

    @MainActor
    func testProviderResetDropsCorrelationWithoutWritingAnEvent() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        try state.setMusicHistoryPolicy(.humanReadable)
        let lifecycle = MobileMusicProviderLifecycle()
        let coordinator = MusicIntegrationCoordinator(rideMapState: state, lifecycle: lifecycle)

        let playing = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "apple",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_000,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
        _ = try coordinator.ingest(
            snapshot: playing,
            wallClockAtMs: 1_700_000_000_000,
            clockUncertaintyMs: 5
        )

        coordinator.resetProviderCorrelation()

        _ = try coordinator.ingest(
            snapshot: .init(
                provider: .spotify,
                sessionId: "spotify",
                state: .unavailable,
                item: nil,
                positionMilliseconds: nil,
                durationMilliseconds: nil,
                observedAtMs: 1_001,
                capabilities: .init(previous: false, play: false, pause: false, next: false, openProvider: true)
            ),
            wallClockAtMs: 1_700_000_000_001,
            clockUncertaintyMs: 5
        )

        XCTAssertEqual(coordinator.nowPlaying?.provider, .spotify)
        XCTAssertEqual(coordinator.nowPlaying?.state, .unavailable)
        XCTAssertNil(coordinator.lastRecordedSequence)
        XCTAssertEqual(coordinator.recordedEvents.count, 1)
    }

    @MainActor
    func testEnablingHistorySeedsTheCurrentTrackAfterDisabledObservation() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        let coordinator = makeCoordinator(rideMapState: state)
        let playing = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "apple",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_000,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )

        _ = try coordinator.ingest(
            snapshot: playing,
            wallClockAtMs: 1_700_000_000_000,
            clockUncertaintyMs: 5
        )
        try coordinator.setHistoryPolicy(.humanReadable)
        let nextObservation = MobileMusicSnapshotDto(
            provider: playing.provider,
            sessionId: playing.sessionId,
            state: playing.state,
            item: playing.item,
            positionMilliseconds: playing.positionMilliseconds,
            durationMilliseconds: playing.durationMilliseconds,
            observedAtMs: 1_001,
            capabilities: playing.capabilities
        )
        _ = try coordinator.ingest(
            snapshot: nextObservation,
            wallClockAtMs: 1_700_000_000_001,
            clockUncertaintyMs: 5
        )

        XCTAssertEqual(coordinator.recordedEvents.count, 1)
    }

    func testRustMusicMonitorLifecycleInvalidatesOlderEffects() throws {
        let lifecycle = MobileMusicProviderLifecycle()
        lifecycle.requestMonitor(request: .observe)
        let first = try XCTUnwrap(lifecycle.beginMonitor())

        XCTAssertEqual(lifecycle.classifyMonitor(generation: first.generation), .current)

        _ = lifecycle.suspend()

        XCTAssertEqual(lifecycle.classifyMonitor(generation: first.generation), .stale)
        _ = lifecycle.resume()
        let second = try XCTUnwrap(lifecycle.beginMonitor())
        XCTAssertEqual(lifecycle.classifyMonitor(generation: second.generation), .current)
        XCTAssertEqual(lifecycle.classifyMonitor(generation: first.generation), .stale)
    }

    func testMusicAccessibilityAnnouncementsDeduplicateProjectedState() {
        var tracker = MusicAccessibilityAnnouncementTracker()
        let playing = nowPlaying(trackID: "track-1")

        XCTAssertEqual(tracker.next(for: playing), playing.accessibilitySummary)
        XCTAssertNil(tracker.next(for: playing))

        let paused = MusicNowPlaying(
            provider: .appleMusic,
            state: .paused,
            item: playing.item,
            capabilities: .init(
                previous: true,
                play: true,
                pause: false,
                next: true,
                openProvider: true
            )
        )
        XCTAssertEqual(tracker.next(for: paused), paused.accessibilitySummary)
        XCTAssertNil(tracker.next(for: paused))

        let pausedWithDifferentCapabilities = MusicNowPlaying(
            provider: .appleMusic,
            state: .paused,
            item: paused.item,
            capabilities: .init(
                previous: false,
                play: true,
                pause: false,
                next: false,
                openProvider: true
            )
        )
        XCTAssertNil(tracker.next(for: pausedWithDifferentCapabilities))
    }

    func testArtworkCacheReusesOnlyBoundedArtworkForTheSameItem() {
        var cache = MusicArtworkCache()
        var loadCount = 0
        let artwork = testArtwork(gray: 0.25)

        let first = cache.artwork(for: "track-1") {
            loadCount += 1
            return artwork
        }
        let second = cache.artwork(for: "track-1") {
            loadCount += 1
            return artwork
        }

        XCTAssertEqual(first, artwork)
        XCTAssertEqual(second, artwork)
        XCTAssertEqual(loadCount, 1)

        _ = cache.artwork(for: "track-2") {
            loadCount += 1
            return nil
        }
        XCTAssertEqual(loadCount, 2)
        XCTAssertNil(
            cache.artwork(for: nil) {
                loadCount += 1
                return artwork
            })
        XCTAssertEqual(loadCount, 2)
    }

    func testArtworkCacheRejectsOldTrackAfterIdentityChanges() {
        var cache = MusicArtworkCache()
        let first = testArtwork(gray: 0.25)
        let second = testArtwork(gray: 0.75)

        cache.insert(first, for: "spotify:track:first")
        XCTAssertEqual(cache.cachedArtwork(for: "spotify:track:first"), first)
        XCTAssertNil(cache.cachedArtwork(for: "spotify:track:second"))

        cache.insert(second, for: "spotify:track:second")
        XCTAssertNil(cache.cachedArtwork(for: "spotify:track:first"))
        XCTAssertEqual(cache.cachedArtwork(for: "spotify:track:second"), second)
    }

    @MainActor
    func testProviderArtworkReachesPresentationOnlyNowPlaying() throws {
        let rideMapState = MobileRideMapState()
        _ = try rideMapState.startGpsOnly(atMs: 1_000)
        let coordinator = makeCoordinator(rideMapState: rideMapState)
        try coordinator.setHistoryPolicy(.humanReadable)
        let snapshot = MobileMusicSnapshotDto(
            provider: .spotify,
            sessionId: "spotify-app-remote",
            state: .playing,
            item: .init(identifier: "spotify:track:artwork", title: "Track", artist: "Artist"),
            positionMilliseconds: 1,
            durationMilliseconds: 2,
            observedAtMs: 1_100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
        let artwork = testArtwork(gray: 0.25)

        let firstOutcome = try coordinator.ingest(
            observation: MusicProviderObservation(snapshot: snapshot, artwork: artwork),
            wallClockAtMs: 1_700_000_000_000,
            clockUncertaintyMs: 1
        )

        XCTAssertEqual(firstOutcome, .recorded)
        XCTAssertEqual(coordinator.recordedEvents.count, 1)
        XCTAssertEqual(coordinator.nowPlaying?.artwork, artwork)
        let recordedEvents = coordinator.recordedEvents

        let updatedSnapshot = MobileMusicSnapshotDto(
            provider: snapshot.provider,
            sessionId: snapshot.sessionId,
            state: snapshot.state,
            item: snapshot.item,
            positionMilliseconds: 2,
            durationMilliseconds: snapshot.durationMilliseconds,
            observedAtMs: 1_200,
            capabilities: snapshot.capabilities
        )
        let updatedArtwork = testArtwork(gray: 0.75)
        let secondOutcome = try coordinator.ingest(
            observation: MusicProviderObservation(snapshot: updatedSnapshot, artwork: updatedArtwork),
            wallClockAtMs: 1_700_000_000_100,
            clockUncertaintyMs: 1
        )

        XCTAssertNil(secondOutcome)
        XCTAssertEqual(coordinator.nowPlaying?.artwork, updatedArtwork)
        XCTAssertEqual(coordinator.recordedEvents, recordedEvents)
        XCTAssertEqual(coordinator.recordedEvents.count, 1)
    }

    func testArtworkRejectsImagesLargerThanThePresentationBound() {
        XCTAssertNil(
            MusicArtwork(
                image: testImage(width: MusicArtwork.maxPixelDimension + 1, gray: 0.5)
            )
        )
    }

    private func nowPlaying(trackID: String) -> MusicNowPlaying {
        MusicNowPlaying(
            provider: .appleMusic,
            state: .playing,
            item: MobileMusicItemDto(identifier: trackID, title: trackID, artist: "Artist"),
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
    }

    func testNowPlayingProjectsPlayPauseAndMetadata() {
        let snapshot = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "system-music-player",
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: "Song",
                artist: "Artist"
            ),
            positionMilliseconds: 10,
            durationMilliseconds: 100,
            observedAtMs: 1_000,
            capabilities: MobileMusicCapabilitiesDto(
                previous: true,
                play: false,
                pause: true,
                next: true,
                openProvider: true
            )
        )

        let nowPlaying = MusicNowPlaying(snapshot: snapshot)

        XCTAssertEqual(nowPlaying.title, "Song")
        XCTAssertEqual(nowPlaying.artist, "Artist")
        XCTAssertEqual(nowPlaying.playPauseCommand, .pause)
        XCTAssertTrue(nowPlaying.supports(.next))
    }

    func testStaleProjectionPreservesMetadataButDisablesTransport() {
        let playing = nowPlaying(trackID: "track-1")

        let stale = playing.staleProjection

        XCTAssertEqual(stale.state, .stale)
        XCTAssertEqual(stale.item, playing.item)
        XCTAssertEqual(stale.artwork, playing.artwork)
        XCTAssertTrue(stale.isCommandAvailable(.openProvider))
        XCTAssertFalse(stale.isCommandAvailable(.previous))
        XCTAssertFalse(stale.isCommandAvailable(.pause))
        XCTAssertFalse(stale.isCommandAvailable(.next))
        XCTAssertTrue(stale.availableTransportCommands.isEmpty)
    }

    func testCachedObservationProjectsRequestedTimestampWithoutChangingState() {
        let observation = MusicProviderObservation(
            snapshot: .init(
                provider: .appleMusic,
                sessionId: "system-music-player",
                state: .playing,
                item: .init(identifier: "track-1", title: "Song", artist: "Artist"),
                positionMilliseconds: nil,
                durationMilliseconds: nil,
                observedAtMs: 1_000,
                capabilities: .init(
                    previous: true,
                    play: false,
                    pause: true,
                    next: true,
                    openProvider: true
                )
            )
        )

        let projected = observation.observedAt(2_000)

        XCTAssertEqual(projected.snapshot.observedAtMs, 2_000)
        XCTAssertEqual(projected.snapshot.state, MobileMusicPlaybackStateDto.playing)
        XCTAssertEqual(projected.snapshot.capabilities, observation.snapshot.capabilities)
    }

    func testStaleStateRejectsRetainedSkipCapabilities() {
        let stale = MusicNowPlaying(
            provider: .spotify,
            state: .stale,
            item: .init(identifier: "track-1", title: "Song", artist: "Artist"),
            capabilities: .init(
                previous: true,
                play: false,
                pause: false,
                next: true,
                openProvider: true
            )
        )

        XCTAssertFalse(stale.isCommandAvailable(.previous))
        XCTAssertFalse(stale.isCommandAvailable(.next))
        XCTAssertTrue(stale.isCommandAvailable(.openProvider))
    }

    @MainActor
    func testCoordinatorTruncatesOversizedExplicitRecordBeforeProjectingIt() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        try state.setMusicHistoryPolicy(.humanReadable)
        let lifecycle = MobileMusicProviderLifecycle()
        let coordinator = MusicIntegrationCoordinator(rideMapState: state, lifecycle: lifecycle)
        let malformed = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: String(repeating: "é", count: 257),
                artist: "Artist"
            ),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )

        XCTAssertNoThrow(
            try coordinator.record(
                snapshot: malformed,
                kind: .itemChanged,
                monotonicAtMs: 1_100,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            )
        )
        XCTAssertEqual(coordinator.nowPlaying?.item?.identifier, "track-1")
        XCTAssertEqual(coordinator.nowPlaying?.item?.title?.utf8.count, 512)
    }

    @MainActor
    func testCoordinatorRejectsImpossiblePlaybackPositionBeforeProjectingIt() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = makeCoordinator(rideMapState: state)
        let invalid = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: 101,
            durationMilliseconds: 100,
            observedAtMs: 1_100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )

        XCTAssertThrowsError(
            try coordinator.ingest(
                snapshot: invalid,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            )
        )
        XCTAssertNil(coordinator.nowPlaying)
    }

    @MainActor
    func testCoordinatorBoundsOversizedProviderMetadataBeforeProjectingIt() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = makeCoordinator(rideMapState: state)

        let valid = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Song", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_100,
            capabilities: .init(previous: true, play: false, pause: true, next: true, openProvider: true)
        )
        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: valid,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            ),
            .recorded
        )

        let invalid = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-2",
                title: String(repeating: "é", count: 257),
                artist: "Artist"
            ),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_200,
            capabilities: valid.capabilities
        )
        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: invalid,
                wallClockAtMs: 1_700_000_000_200,
                clockUncertaintyMs: 5
            ),
            .recorded
        )
        XCTAssertEqual(coordinator.nowPlaying?.item?.identifier, "track-2")
        XCTAssertEqual(coordinator.nowPlaying?.item?.title?.utf8.count, 512)
    }

    @MainActor
    func testCoordinatorNormalizesBlankOptionalMetadataLikeRust() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        try state.setMusicHistoryPolicy(.humanReadable)
        let coordinator = makeCoordinator(rideMapState: state)
        let snapshot = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: " ", artist: ""),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_100,
            capabilities: .init(previous: false, play: false, pause: true, next: true, openProvider: true)
        )

        XCTAssertEqual(
            try coordinator.ingest(
                snapshot: snapshot,
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            ),
            .recorded
        )
        XCTAssertNil(coordinator.nowPlaying?.item?.title)
        XCTAssertNil(coordinator.nowPlaying?.item?.artist)
        XCTAssertNil(state.currentMusicEvents().first?.title)
        XCTAssertNil(state.currentMusicEvents().first?.artist)
    }

    func testNowPlayingExposesOnlySupportedTransportCommands() {
        let nowPlaying = MusicNowPlaying(
            provider: .appleMusic,
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: "Song",
                artist: "Artist"
            ),
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: true,
                pause: true,
                next: false,
                openProvider: true
            )
        )

        XCTAssertEqual(nowPlaying.availableTransportCommands, [.pause])
        XCTAssertFalse(nowPlaying.isCommandAvailable(.play))
        XCTAssertTrue(nowPlaying.isCommandAvailable(.pause))
        XCTAssertTrue(nowPlaying.isCommandAvailable(.openProvider))
    }

    func testMusicTimeConversionRejectsInvalidProviderValues() {
        XCTAssertEqual(MusicTimeConversion.milliseconds(1.5), 1_500)
        XCTAssertNil(MusicTimeConversion.milliseconds(-1))
        XCTAssertNil(MusicTimeConversion.milliseconds(.nan))
        XCTAssertNil(MusicTimeConversion.milliseconds(.infinity))
        XCTAssertNil(MusicTimeConversion.milliseconds(.greatestFiniteMagnitude))
        XCTAssertNil(MusicTimeConversion.milliseconds(Double(UInt64.max) / 1_000))
    }

    func testMusicTimelineIDsUseTheRustEventSequence() {
        func event(sequence: UInt64) -> MobileMusicRideEventDto {
            MobileMusicRideEventDto(
                sequence: sequence,
                provider: .appleMusic,
                itemIdentifier: "track-1",
                title: "Song",
                artist: "Artist",
                kind: .play,
                observedAtMs: 1_000,
                monotonicAtMs: 1_000,
                wallClockAtMs: 1_700_000_000_000,
                clockUncertaintyMs: 5
            )
        }

        XCTAssertNotEqual(event(sequence: 0).timelineID, event(sequence: 1).timelineID)
    }

    func testListeningHistoryExcludesDisconnectsAndPreservesTrackMetadata() {
        let event = MobileMusicRideEventDto(
            sequence: 1,
            provider: .spotify,
            itemIdentifier: "track-1",
            title: "A long podcast title",
            artist: "Artist",
            kind: .providerDisconnected,
            observedAtMs: 1_000,
            monotonicAtMs: 1_000,
            wallClockAtMs: 1_700_000_000_000,
            clockUncertaintyMs: 5
        )

        let withoutMetadata = MobileMusicRideEventDto(
            sequence: 2,
            provider: .spotify,
            itemIdentifier: nil,
            title: nil,
            artist: nil,
            kind: .providerDisconnected,
            observedAtMs: 2_000,
            monotonicAtMs: 2_000,
            wallClockAtMs: 1_700_000_001_000,
            clockUncertaintyMs: 5
        )
        let track = MobileMusicRideEventDto(
            sequence: 3,
            provider: .spotify,
            itemIdentifier: "spotify:track:track-1",
            title: "Song",
            artist: "Artist",
            kind: .itemChanged,
            observedAtMs: 3_000,
            monotonicAtMs: 3_000,
            wallClockAtMs: 1_700_000_002_000,
            clockUncertaintyMs: 5
        )
        var itemlessStop = withoutMetadata
        itemlessStop.kind = .stopped
        var identifiedStop = track
        identifiedStop.kind = .stopped
        let stored = [event, track, withoutMetadata, itemlessStop]
        XCTAssertEqual(stored.listeningHistoryEvents, [track])
        XCTAssertTrue([event, withoutMetadata].listeningHistoryEvents.isEmpty)
        XCTAssertEqual(stored.count, 4)
        XCTAssertEqual([itemlessStop, identifiedStop].listeningHistoryEvents, [identifiedStop])
        XCTAssertEqual(stored.listeningHistoryEvents.first?.itemIdentifier, "spotify:track:track-1")
        XCTAssertEqual(stored.listeningHistoryEvents.first?.title, "Song")
        XCTAssertEqual(stored.listeningHistoryEvents.first?.artist, "Artist")
    }

    func testNowPlayingProvidesLocalizedArtworkAccessibilityLabel() {
        let nowPlaying = MusicNowPlaying(
            provider: .appleMusic,
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: "Song",
                artist: "Artist"
            )
        )

        XCTAssertEqual(nowPlaying.artworkAccessibilityLabel, pevLocalizedText("music.artwork", "Song"))
    }

    func testArtworkAccessibilityLabelUsesProviderWhenTitleIsUnavailable() {
        let nowPlaying = MusicNowPlaying(
            provider: .spotify,
            state: .unavailable,
            item: MobileMusicItemDto(identifier: "track-1", title: nil, artist: nil)
        )

        XCTAssertEqual(nowPlaying.artworkAccessibilityLabel, pevLocalizedText("music.artwork", "Spotify"))
    }

    @MainActor
    func testCoordinatorClassifiesAcceptedItemSkipSeparatelyFromItemChange() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000)
        try state.setMusicHistoryPolicy(.humanReadable)
        let lifecycle = MobileMusicProviderLifecycle()
        let provider = try XCTUnwrap(lifecycle.beginProviderSession())
        let coordinator = MusicIntegrationCoordinator(rideMapState: state, lifecycle: lifecycle)

        func observation(trackID: String, observedAtMs: UInt64) -> MusicProviderObservation {
            MusicProviderObservation(
                snapshot: MobileMusicSnapshotDto(
                    provider: .appleMusic,
                    sessionId: "session",
                    state: .playing,
                    item: MobileMusicItemDto(
                        identifier: trackID,
                        title: trackID,
                        artist: "Artist"
                    ),
                    positionMilliseconds: nil,
                    durationMilliseconds: nil,
                    observedAtMs: observedAtMs,
                    capabilities: MobileMusicCapabilitiesDto(
                        previous: true,
                        play: false,
                        pause: true,
                        next: true,
                        openProvider: true
                    )
                )
            )
        }

        XCTAssertEqual(
            try coordinator.ingest(
                observation: observation(trackID: "track-1", observedAtMs: 1_100),
                wallClockAtMs: 1_700_000_000_100,
                clockUncertaintyMs: 5
            ),
            .recorded
        )
        let transport = try XCTUnwrap(
            lifecycle.beginTransportEffect(
                owner: .provider(providerGeneration: provider),
                command: .next,
                nowMs: 1_150
            ))
        XCTAssertEqual(
            lifecycle.finishTransport(
                providerGeneration: provider,
                requestId: transport.id,
                outcome: .accepted
            ).state,
            .finished
        )
        XCTAssertEqual(
            try coordinator.ingest(
                observation: observation(trackID: "track-2", observedAtMs: 1_200),
                wallClockAtMs: 1_700_000_000_200,
                clockUncertaintyMs: 5
            ),
            .recorded
        )
        XCTAssertEqual(coordinator.recordedEvents.map(\.kind), [.itemChanged, .skip])
    }

    func testRustHistoryTransitionRemainsPendingUntilSwiftAcknowledgesIt() throws {
        let lifecycle = MobileMusicProviderLifecycle()

        func snapshot(trackID: String, observedAtMs: UInt64) -> MobileMusicSnapshotDto {
            MobileMusicSnapshotDto(
                provider: .appleMusic,
                sessionId: "session",
                state: .playing,
                item: .init(identifier: trackID, title: trackID, artist: nil),
                positionMilliseconds: nil,
                durationMilliseconds: nil,
                observedAtMs: observedAtMs,
                capabilities: .init(
                    previous: true,
                    play: false,
                    pause: true,
                    next: true,
                    openProvider: true
                )
            )
        }

        let first = try XCTUnwrap(
            lifecycle.observeMusic(
                snapshot: snapshot(trackID: "first", observedAtMs: 100),
                wallClockAtMs: 1_000,
                clockUncertaintyMs: 5
            )?.historyTransition)
        XCTAssertEqual(lifecycle.acknowledgeHistoryTransition(id: first.id), .acknowledged)

        let pending = try XCTUnwrap(
            lifecycle.observeMusic(
                snapshot: snapshot(trackID: "second", observedAtMs: 200),
                wallClockAtMs: 2_000,
                clockUncertaintyMs: 5
            )?.historyTransition)
        let retry = try XCTUnwrap(
            lifecycle.observeMusic(
                snapshot: snapshot(trackID: "second", observedAtMs: 300),
                wallClockAtMs: 3_000,
                clockUncertaintyMs: 5
            )?.historyTransition)

        XCTAssertEqual(retry.id, pending.id)
        XCTAssertEqual(retry.snapshot.observedAtMs, 200)
        XCTAssertEqual(retry.wallClockAtMs, 2_000)
        XCTAssertEqual(lifecycle.acknowledgeHistoryTransition(id: retry.id), .acknowledged)
        XCTAssertNil(
            try lifecycle.observeMusic(
                snapshot: snapshot(trackID: "second", observedAtMs: 400),
                wallClockAtMs: 4_000,
                clockUncertaintyMs: 5
            )?.historyTransition)
    }
}

private func testArtwork(gray: CGFloat) -> MusicArtwork {
    MusicArtwork(image: testImage(gray: gray))!
}

private func testImage(width: Int = 1, gray: CGFloat) -> CGImage {
    let context = CGContext(
        data: nil,
        width: width,
        height: 1,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    context.setFillColor(red: gray, green: gray, blue: gray, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: width, height: 1))
    return context.makeImage()!
}
