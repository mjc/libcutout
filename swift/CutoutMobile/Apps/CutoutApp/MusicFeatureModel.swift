import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

@MainActor
protocol AppleMusicMonitorDriving: AnyObject {
    func requestAuthorization(allowPrompt: Bool) async -> Bool
    func unauthorizedSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto
    func startMonitoring(
        observedAtMs: @escaping @MainActor () -> UInt64,
        onObservation: @escaping @MainActor (MusicProviderObservation) -> Void
    ) async
    func stopMonitoring()
    func applySuspension(_ suspension: MobileMusicProviderSuspension)
    func refreshObservation(observedAtMs: UInt64)
}

typealias MusicMonitorPollWaiter = @MainActor @Sendable (
    UInt64,
    @escaping @MainActor @Sendable () -> UInt64
) async -> Bool
typealias MusicProviderCommandHandler = @MainActor (
    MobileMusicProviderDto,
    MobileMusicCommandDto
) async -> MusicCommandOutcome

#if canImport(MediaPlayer) && os(iOS)
    extension AppleMusicProviderAdapter: AppleMusicMonitorDriving {}
#endif

/// App-retained owner for shared music presentation, provider lifecycle, and
/// Rust-backed history effects. It reuses the existing domain authorities.
@MainActor
@Observable
final class MusicFeatureModel {
    @ObservationIgnored let playerVisibilityStore: MusicPlayerVisibilityStore
    @ObservationIgnored let providerSelectionStore: MusicProviderSelectionStore
    @ObservationIgnored let historyPolicyStore: MusicHistoryPolicyStore
    @ObservationIgnored let monitoringPreferenceStore: MusicMonitoringPreferenceStore
    @ObservationIgnored let providerLifecycle: MobileMusicProviderLifecycle
    @ObservationIgnored let effects: MusicProviderEffectExecutor
    @ObservationIgnored let coordinator: MusicIntegrationCoordinator
    @ObservationIgnored private let rideMapState: MobileRideMapState?
    @ObservationIgnored let spotifyProvider: SpotifyProviderAdapter
    @ObservationIgnored private let spotifyCallbackHandler: (@MainActor (URL) -> Bool)?
    @ObservationIgnored private let monotonicNow: @MainActor () -> UInt64
    @ObservationIgnored private let updateCapturePolicy: @MainActor (MobileMusicHistoryPolicyDto) -> Void
    @ObservationIgnored private let updateCaptureObservation: @MainActor (MobilePevcapMusicEventDto?) -> Void
    @ObservationIgnored private let invalidateHistoryForDeletion: @MainActor () -> Void
    @ObservationIgnored private let selectedHistoryRideID: @MainActor () -> String?
    @ObservationIgnored private let clearSelectedHistoryMusic: @MainActor () -> Void
    @ObservationIgnored private let setRideHistoryError: @MainActor (MobileRideMapError) -> Void
    @ObservationIgnored private let providerCommandHandler: MusicProviderCommandHandler?
    #if canImport(MediaPlayer) && os(iOS)
        @ObservationIgnored let appleProvider: AppleMusicProviderAdapter
        @ObservationIgnored private let appleMonitor: any AppleMusicMonitorDriving
        @ObservationIgnored private let waitForMonitorPoll: MusicMonitorPollWaiter
    #endif

    var settingsNowPlaying: MusicNowPlaying?
    var timelineEvents = [MobileMusicRideEventDto]()
    var selectedProvider: MobileMusicProviderDto
    var isPlayerHidden: Bool
    var historyPolicy: MobileMusicHistoryPolicyDto
    var historyUnavailable = false
    var historySaveError: MobileRideMapError?
    var commandFeedback: MusicCommandFeedback?
    @ObservationIgnored private var observationError: MobileRideMapError?
    @ObservationIgnored private var historyPersistenceError: MobileRideMapError?

    var nowPlaying: MusicNowPlaying? {
        isPlayerHidden ? nil : settingsNowPlaying
    }

    var commandStatusText: String? {
        commandFeedback?.messageKey.map { pevLocalizedText($0) }
    }

    init(
        providerSelectionStore: MusicProviderSelectionStore,
        historyPolicyStore: MusicHistoryPolicyStore,
        monitoringPreferenceStore: MusicMonitoringPreferenceStore,
        rideMapState: MobileRideMapState?,
        monotonicNow: @escaping @MainActor () -> UInt64,
        updateCapturePolicy: @escaping @MainActor (MobileMusicHistoryPolicyDto) -> Void,
        updateCaptureObservation: @escaping @MainActor (MobilePevcapMusicEventDto?) -> Void,
        invalidateHistoryForDeletion: @escaping @MainActor () -> Void,
        selectedHistoryRideID: @escaping @MainActor () -> String?,
        clearSelectedHistoryMusic: @escaping @MainActor () -> Void,
        setRideHistoryError: @escaping @MainActor (MobileRideMapError) -> Void,
        appleMonitor: (any AppleMusicMonitorDriving)? = nil,
        monitorPollWaiter: MusicMonitorPollWaiter? = nil,
        providerCommandHandler: MusicProviderCommandHandler? = nil,
        spotifyCallbackHandler: (@MainActor (URL) -> Bool)? = nil
    ) {
        let providerLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let playerVisibilityStore = MusicPlayerVisibilityStore()
        self.playerVisibilityStore = playerVisibilityStore
        self.providerSelectionStore = providerSelectionStore
        self.historyPolicyStore = historyPolicyStore
        self.monitoringPreferenceStore = monitoringPreferenceStore
        self.rideMapState = rideMapState
        self.monotonicNow = monotonicNow
        self.updateCapturePolicy = updateCapturePolicy
        self.updateCaptureObservation = updateCaptureObservation
        self.invalidateHistoryForDeletion = invalidateHistoryForDeletion
        self.selectedHistoryRideID = selectedHistoryRideID
        self.clearSelectedHistoryMusic = clearSelectedHistoryMusic
        self.setRideHistoryError = setRideHistoryError
        self.providerCommandHandler = providerCommandHandler
        self.providerLifecycle = providerLifecycle
        self.effects = effects
        self.coordinator = MusicIntegrationCoordinator(
            rideMapState: rideMapState,
            lifecycle: providerLifecycle
        )
        let spotifyProvider = SpotifyProviderAdapter(
            lifecycle: providerLifecycle,
            effects: effects
        )
        self.spotifyProvider = spotifyProvider
        self.spotifyCallbackHandler = spotifyCallbackHandler
        #if canImport(MediaPlayer) && os(iOS)
            let appleProvider = AppleMusicProviderAdapter(
                lifecycle: providerLifecycle,
                effects: effects
            )
            self.appleProvider = appleProvider
            self.appleMonitor = appleMonitor ?? appleProvider
            self.waitForMonitorPoll =
                monitorPollWaiter ?? { deadlineMs, nowMs in
                    await MusicProviderEffectExecutor.wait(until: deadlineMs, nowMs: nowMs)
                }
        #endif
        selectedProvider = providerSelectionStore.provider
        isPlayerHidden = playerVisibilityStore.isHidden
        historyPolicy = historyPolicyStore.policy
        timelineEvents = []
    }

    @discardableResult
    func setHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) -> Bool {
        #if DEBUG
            print("music_history_request policy=\(policy) has_ride_store=\(rideMapState != nil)")
        #endif
        let previous = historyPolicy
        clearHistoryErrors()
        do {
            try coordinator.setHistoryPolicy(policy)
            historyUnavailable = false
            rememberHistoryPolicy(policy)
            if policy == .disabled { clearMusicCaptureContext() }
            timelineEvents = coordinator.recordedEvents
            return true
        } catch let error as MobileRideMapError where error == .noActiveRide || error == .invalidTransition {
            rememberHistoryPolicy(policy)
            coordinator.restoreHistoryPolicy(policy)
            historyUnavailable = false
            if policy == .disabled { clearMusicCaptureContext() }
            return true
        } catch {
            #if DEBUG
                print("music_history_rejected error=\(error)")
            #endif
            historyPolicy = previous
            setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
            return false
        }
    }

    @discardableResult
    func setHistoryPolicyAsync(_ policy: MobileMusicHistoryPolicyDto) async -> Bool {
        #if DEBUG
            print("music_history_request policy=\(policy) has_ride_store=\(rideMapState != nil)")
        #endif
        let previous = historyPolicy
        clearHistoryErrors()
        do {
            try await coordinator.setHistoryPolicyAsync(policy)
            rememberHistoryPolicy(policy)
            if policy == .disabled {
                clearMusicCaptureContext()
                timelineEvents = []
            } else {
                do {
                    timelineEvents = try await coordinator.recordedEventsAsync()
                } catch {
                    setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
                }
            }
            return true
        } catch let error as MobileRideMapError where error == .noActiveRide || error == .invalidTransition {
            rememberHistoryPolicy(policy)
            coordinator.restoreHistoryPolicy(policy)
            if policy == .disabled { clearMusicCaptureContext() }
            return true
        } catch {
            #if DEBUG
                print("music_history_rejected error=\(error)")
            #endif
            historyPolicy = previous
            setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
            return false
        }
    }

    private func rememberHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        historyPolicyStore.set(policy)
        historyPolicy = policy
        historyUnavailable = false
        clearHistoryErrors()
        updateCapturePolicy(policy)
    }

    func applyHistoryForNewRideAsync() async -> MobileRideMapError? {
        updateCaptureObservation(nil)
        let defaultPolicy = historyPolicyStore.policy
        historyPolicy = defaultPolicy
        do {
            try await coordinator.setHistoryPolicyAsync(defaultPolicy)
            historyUnavailable = false
            clearHistoryErrors()
            coordinator.restoreHistoryPolicy(defaultPolicy)
            if defaultPolicy == .disabled {
                timelineEvents = []
            } else {
                timelineEvents = try await coordinator.recordedEventsAsync()
            }
            updateCapturePolicy(defaultPolicy)
            return nil
        } catch {
            let mappedError = CutoutAppModel.mapRideMapError(error)
            setRideHistoryError(mappedError)
            guard rideMapState?.currentSnapshot() != nil else {
                historyPolicy = .disabled
                historyUnavailable = false
                coordinator.restoreHistoryPolicy(.disabled)
                timelineEvents = []
                return mappedError
            }
            if let history = try? await rideMapState?.currentMusicHistoryAsync() {
                synchronizeHistory(history)
            }
            return mappedError
        }
    }

    func synchronizeHistory(_ history: MobileMusicHistoryDto?) {
        guard let history else {
            historyUnavailable = false
            clearHistoryErrors()
            historyPolicy = .disabled
            coordinator.restoreHistoryPolicy(.disabled)
            timelineEvents = []
            updateCapturePolicy(.disabled)
            return
        }
        switch history.status {
        case .available:
            historyUnavailable = false
            clearHistoryErrors()
            historyPolicy = .humanReadable
            coordinator.restoreHistoryPolicy(.humanReadable)
            timelineEvents = history.events
        case .redacted:
            historyUnavailable = false
            clearHistoryErrors()
            historyPolicy = .opaqueItem
            coordinator.restoreHistoryPolicy(.opaqueItem)
            timelineEvents = history.events
        case .unavailable:
            historyUnavailable = true
            coordinator.restoreHistoryPolicy(historyPolicy)
            timelineEvents = []
        case .missing, .disabled, .deleted:
            historyUnavailable = false
            clearHistoryErrors()
            historyPolicy = .disabled
            coordinator.restoreHistoryPolicy(.disabled)
            timelineEvents = history.events
        }
        updateCapturePolicy(historyPolicy)
    }

    @discardableResult
    func forgetHistory(for rideID: String) async -> Bool {
        guard let rideMapState else {
            setRideHistoryError(.storageError("Rust ride database is unavailable"))
            return false
        }
        invalidateHistoryForDeletion()
        let currentRide = rideMapState.currentSnapshot()
        let isOpenCurrentRide = currentRide?.rideID == rideID && currentRide?.state.isOpen == true
        do {
            if isOpenCurrentRide {
                try await rideMapState.deleteCurrentMusicHistoryAsync()
                if rideMapState.currentSnapshot()?.rideID == rideID {
                    clearActiveHistory()
                }
            } else {
                try await rideMapState.deleteMusicHistoryAsync(rideID: rideID)
                if rideMapState.currentSnapshot()?.rideID == rideID {
                    historyPolicy = .disabled
                    historyUnavailable = false
                    coordinator.restoreHistoryPolicy(.disabled)
                    providerLifecycle.clearPendingCommandCorrelation()
                    clearMusicCaptureContext()
                    timelineEvents = coordinator.recordedEvents
                }
            }
            if selectedHistoryRideID() == rideID { clearSelectedHistoryMusic() }
            return true
        } catch {
            setRideHistoryError(CutoutAppModel.mapRideMapError(error))
            return false
        }
    }

    private func clearActiveHistory() {
        historyPolicy = .disabled
        historyUnavailable = false
        clearHistoryErrors()
        coordinator.restoreHistoryPolicy(.disabled)
        providerLifecycle.clearPendingCommandCorrelation()
        clearMusicCaptureContext()
        timelineEvents = coordinator.recordedEvents
    }

    func clearMusicCaptureContext() {
        updateCaptureObservation(nil)
    }

    func rideMapClosed() {
        clearMusicCaptureContext()
        timelineEvents = coordinator.recordedEvents
    }

    func handleProviderURL(_ url: URL) -> Bool {
        #if canImport(SpotifyiOS) && os(iOS)
            guard selectedProvider == .spotify else { return false }
            if let spotifyCallbackHandler { return spotifyCallbackHandler(url) }
            return spotifyProvider.handleCallback(url)
        #else
            _ = url
            return false
        #endif
    }

    func refreshSnapshot() {
        #if canImport(MediaPlayer) && os(iOS)
            let observedAtMs = monotonicNow()
            let observation: MusicProviderObservation
            switch selectedProvider.monitoringMode {
            case .appleMusicSystemPlayer:
                appleMonitor.refreshObservation(observedAtMs: observedAtMs)
                return
            case .spotifyAppRemote:
                observation = spotifyProvider.observation(observedAtMs: observedAtMs)
            case .unavailable:
                observation = MusicProviderObservation(
                    snapshot: spotifyProvider.unavailableSnapshot(observedAtMs: observedAtMs)
                )
            }
            Task { @MainActor [weak self] in
                _ = await self?.ingestObservationAsync(observation)
            }
        #endif
    }

    func stopMonitoring() {
        _ = providerLifecycle.cancelMonitor()
        stopProviderWork()
    }

    private func stopProviderWork() {
        effects.cancelAll(in: .monitor)
        #if canImport(MediaPlayer) && os(iOS)
            appleMonitor.stopMonitoring()
        #endif
        #if canImport(SpotifyiOS) && os(iOS)
            spotifyProvider.stopMonitoring()
        #endif
    }

    func start(sceneIsActive: Bool) {
        guard monitoringPreferenceStore.isEnabled else { return }
        providerLifecycle.requestMonitor(request: .observe)
        guard sceneIsActive else {
            _ = providerLifecycle.suspend()
            return
        }
        beginMonitoring()
    }

    func sceneDidEnterBackground() {
        let suspension = providerLifecycle.suspend()
        #if canImport(MediaPlayer) && os(iOS)
            appleMonitor.applySuspension(suspension)
        #endif
        spotifyProvider.applySuspension(suspension)
        if suspension.observationGap {
            let observedAtMs = monotonicNow()
            let observation = MusicProviderObservation(
                snapshot: MobileMusicSnapshotDto(
                    provider: selectedProvider,
                    sessionId: "music-observation-gap",
                    state: .disconnected,
                    item: coordinator.nowPlaying?.item,
                    positionMilliseconds: nil,
                    durationMilliseconds: nil,
                    observedAtMs: observedAtMs,
                    capabilities: .init(
                        previous: false,
                        play: false,
                        pause: false,
                        next: false,
                        openProvider: true
                    )
                ))
            Task { @MainActor [weak self] in
                _ = await self?.ingestObservationAsync(observation)
            }
        }
        stopProviderWork()
        if let nowPlaying = coordinator.nowPlaying {
            settingsNowPlaying = nowPlaying.staleProjection
        }
    }

    func sceneDidBecomeActive() -> Bool {
        providerLifecycle.resume() == .restored
    }

    func connect() {
        monitoringPreferenceStore.setEnabled(true)
        _ = providerLifecycle.resume()
        providerLifecycle.requestMonitor(request: .authorize)
        beginMonitoring()
    }

    func authorizeSpotify() {
        #if canImport(SpotifyiOS) && os(iOS)
            guard selectedProvider == .spotify else { return }
            spotifyProvider.clearAuthorization()
            connect()
        #endif
    }

    func dismissPlayer() {
        playerVisibilityStore.setHidden(true)
        isPlayerHidden = true
        providerLifecycle.clearPendingCommandCorrelation()
    }

    func restorePlayer() {
        playerVisibilityStore.setHidden(false)
        isPlayerHidden = false
        settingsNowPlaying = projectedNowPlaying()
        monitoringPreferenceStore.setEnabled(true)
        providerLifecycle.requestMonitor(request: .observe)
        beginMonitoring()
    }

    func selectProvider(_ provider: MobileMusicProviderDto) {
        let previousProvider = selectedProvider
        coordinator.resetProviderCorrelation()
        providerLifecycle.invalidateCommandFeedback()
        commandFeedback = nil
        selectedProvider = provider
        settingsNowPlaying = projectedNowPlaying()
        providerSelectionStore.set(provider)
        monitoringPreferenceStore.setEnabled(true)
        updateMonitoring(from: previousProvider, to: provider)
    }

    func beginCommandFeedback() -> MobileMusicCommandFeedbackId? {
        guard let requestID = providerLifecycle.beginCommandFeedback() else {
            commandFeedback = nil
            return nil
        }
        commandFeedback = MusicCommandFeedback(requestID: requestID, outcome: .accepted)
        return requestID
    }

    func dismissCommandFeedback(requestID: MobileMusicCommandFeedbackId) {
        guard commandFeedback?.requestID == requestID else { return }
        _ = providerLifecycle.dismissCommandFeedback(id: requestID)
        commandFeedback = nil
    }

    func dismissCommandFeedback() {
        guard let requestID = commandFeedback?.requestID else { return }
        dismissCommandFeedback(requestID: requestID)
    }

    func finishCommand(
        _ outcome: MusicCommandOutcome,
        provider: MobileMusicProviderDto,
        requestID: MobileMusicCommandFeedbackId?
    ) -> MusicCommandOutcome {
        if let requestID,
            selectedProvider == provider,
            providerLifecycle.classifyCommandFeedback(id: requestID) == .current
        {
            commandFeedback = MusicCommandFeedback(requestID: requestID, outcome: outcome)
        }
        return outcome
    }

    func handleCommand(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        let commandProvider = selectedProvider
        let requestID = beginCommandFeedback()
        #if canImport(MediaPlayer) && os(iOS)
            if command == .openProvider {
                let result = await performProviderCommand(command, provider: commandProvider)
                return finishCommand(result, provider: commandProvider, requestID: requestID)
            }
        #endif
        guard let nowPlaying else {
            return finishCommand(.unavailable, provider: commandProvider, requestID: requestID)
        }
        guard nowPlaying.isCommandAvailable(command) else {
            return finishCommand(.refused, provider: commandProvider, requestID: requestID)
        }
        #if canImport(MediaPlayer) && os(iOS)
            let result = await performProviderCommand(command, provider: nowPlaying.provider)
            if result == .accepted { refreshSnapshot() }
            return finishCommand(result, provider: commandProvider, requestID: requestID)
        #else
            return finishCommand(.unavailable, provider: commandProvider, requestID: requestID)
        #endif
    }

    private func performProviderCommand(
        _ command: MobileMusicCommandDto,
        provider: MobileMusicProviderDto
    ) async -> MusicCommandOutcome {
        if let providerCommandHandler {
            return await providerCommandHandler(provider, command)
        }
        #if canImport(MediaPlayer) && os(iOS)
            return provider == .spotify
                ? await spotifyProvider.perform(command)
                : await appleProvider.perform(command)
        #else
            return .unavailable
        #endif
    }

    private func updateMonitoring(
        from previousProvider: MobileMusicProviderDto,
        to provider: MobileMusicProviderDto
    ) {
        switch provider.monitoringMode {
        case .unavailable:
            let suspension = MobileMusicProviderSuspension(
                observationGap: false,
                cancelledTransportRequestId: providerLifecycle.cancelMonitor().requestId
            )
            #if canImport(MediaPlayer) && os(iOS)
                appleProvider.applySuspension(suspension)
            #endif
            spotifyProvider.applySuspension(suspension)
            stopProviderWork()
        case .appleMusicSystemPlayer where previousProvider != provider:
            providerLifecycle.requestMonitor(request: .observe)
            beginMonitoring()
        case .appleMusicSystemPlayer:
            break
        case .spotifyAppRemote where previousProvider != provider:
            providerLifecycle.requestMonitor(request: .observe)
            beginMonitoring()
        case .spotifyAppRemote:
            break
        }
    }

    func beginMonitoring() {
        // Invalidate before stopping: teardown may resume cancelled command work synchronously.
        providerLifecycle.invalidateCommandFeedback()
        commandFeedback = nil
        guard let effect = providerLifecycle.beginMonitor() else { return }
        #if os(iOS) && canImport(MediaPlayer)
            if let settingsNowPlaying {
                self.settingsNowPlaying = settingsNowPlaying.staleProjection
            }
            stopProviderWork()
            let generation = effect.generation
            let provider = selectedProvider
            let lifecycle = providerLifecycle
            let appleMonitor = self.appleMonitor
            let spotifyProvider = self.spotifyProvider
            let waitForPoll = waitForMonitorPoll
            effects.run(.monitor(generation)) { [weak self, appleMonitor, spotifyProvider] in
                await Self.monitor(
                    provider: provider,
                    generation: generation,
                    allowAuthorization: effect.start == .authorize,
                    lifecycle: lifecycle,
                    appleMonitor: appleMonitor,
                    spotifyProvider: spotifyProvider,
                    waitForPoll: waitForPoll,
                    isCurrent: { [weak self] in
                        guard let self else { return false }
                        return lifecycle.classifyMonitor(generation: generation) == .current
                            && self.selectedProvider == provider
                    },
                    observedAtMs: { [weak self] in self?.monotonicNow() },
                    record: { [weak self] observation in
                        Task { @MainActor [weak self] in
                            _ = await self?.ingestObservationAsync(observation)
                        }
                    },
                    refresh: { [weak self] in self?.refreshSnapshot() }
                )
                self?.finishMonitoring(generation: generation)
            }
        #else
            _ = ingestObservation(unavailableMusicObservation(observedAtMs: monotonicNow()))
            _ = providerLifecycle.finishMonitor(generation: effect.generation)
        #endif
    }

    private func finishMonitoring(generation: MobileMusicMonitorId) {
        guard providerLifecycle.finishMonitor(generation: generation) == .current else { return }
        #if canImport(MediaPlayer) && os(iOS)
            appleMonitor.stopMonitoring()
        #endif
        #if canImport(SpotifyiOS) && os(iOS)
            spotifyProvider.stopMonitoring()
        #endif
    }

    #if canImport(MediaPlayer) && os(iOS)
        @MainActor
        private static func monitor(
            provider: MobileMusicProviderDto,
            generation: MobileMusicMonitorId,
            allowAuthorization: Bool,
            lifecycle: MobileMusicProviderLifecycle,
            appleMonitor: any AppleMusicMonitorDriving,
            spotifyProvider: SpotifyProviderAdapter,
            waitForPoll: MusicMonitorPollWaiter,
            isCurrent: @escaping @MainActor () -> Bool,
            observedAtMs: @escaping @MainActor () -> UInt64?,
            record: @escaping @MainActor (MusicProviderObservation) -> Void,
            refresh: @escaping @MainActor () -> Void
        ) async {
            guard isCurrent() else { return }
            #if canImport(SpotifyiOS) && os(iOS)
                if provider.monitoringMode == .spotifyAppRemote {
                    let started = spotifyProvider.startMonitoring(allowAuthorization: allowAuthorization) {
                        guard isCurrent() else { return }
                        refresh()
                    }
                    guard started else { return }
                    defer { if isCurrent() { spotifyProvider.stopMonitoring() } }
                    while !Task.isCancelled && isCurrent() {
                        spotifyProvider.ensureConnection()
                        guard let nowMs = observedAtMs(),
                            let poll = lifecycle.nextMonitorPoll(
                                generation: generation,
                                workState: spotifyProvider.monitoringWorkState,
                                nowMs: nowMs
                            )
                        else { return }
                        spotifyProvider.refreshPlayerState()
                        guard await waitForPoll(poll.deadlineMs, { observedAtMs() ?? poll.deadlineMs }) else { return }
                    }
                    return
                }
            #endif
            guard provider.monitoringMode == .appleMusicSystemPlayer else {
                guard !Task.isCancelled, isCurrent(), let nowMs = observedAtMs() else { return }
                record(
                    .unavailable(
                        provider: provider,
                        sessionId: "music-unavailable",
                        observedAtMs: nowMs
                    ))
                return
            }
            guard await appleMonitor.requestAuthorization(allowPrompt: allowAuthorization) else {
                guard !Task.isCancelled, isCurrent(), let nowMs = observedAtMs() else { return }
                record(
                    MusicProviderObservation(
                        snapshot: appleMonitor.unauthorizedSnapshot(observedAtMs: nowMs)
                    ))
                return
            }
            guard !Task.isCancelled, isCurrent() else { return }
            await appleMonitor.startMonitoring(
                observedAtMs: { observedAtMs() ?? 0 },
                onObservation: { observation in
                    guard isCurrent() else { return }
                    record(observation)
                }
            )
            guard !Task.isCancelled, isCurrent() else { return }
            defer { if isCurrent() { appleMonitor.stopMonitoring() } }
            while !Task.isCancelled {
                guard isCurrent(), let nowMs = observedAtMs(),
                    let poll = lifecycle.nextMonitorPoll(
                        generation: generation,
                        workState: .active,
                        nowMs: nowMs
                    )
                else { return }
                appleMonitor.refreshObservation(observedAtMs: nowMs)
                guard await waitForPoll(poll.deadlineMs, { observedAtMs() ?? poll.deadlineMs }) else { return }
            }
        }
    #endif

    @discardableResult
    func ingestObservation(
        _ observation: MusicProviderObservation,
        wallClockAtMs: UInt64? = nil,
        clockUncertaintyMs: UInt64 = 1_000
    ) -> Bool {
        let wallClockAtMs = wallClockAtMs ?? UInt64(Date().timeIntervalSince1970 * 1_000)
        do {
            let outcome = try coordinator.ingest(
                observation: observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            let succeeded = applyObservationOutcome(
                outcome,
                observation: observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            finishObservation()
            return succeeded
        } catch let MusicIntegrationIngestError.observation(error) {
            setObservationError(CutoutAppModel.mapRideMapError(error))
            finishObservation()
            return false
        } catch let MusicIntegrationIngestError.history(error) {
            setObservationError(nil)
            if let error = error as? MobileRideMapError, error == .noActiveRide {
                finishObservation()
                return false
            }
            setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
            finishObservation()
            return false
        } catch {
            setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
            finishObservation()
            return false
        }
    }

    @discardableResult
    func ingestObservationAsync(
        _ observation: MusicProviderObservation,
        wallClockAtMs: UInt64? = nil,
        clockUncertaintyMs: UInt64 = 1_000
    ) async -> Bool {
        let wallClockAtMs = wallClockAtMs ?? UInt64(Date().timeIntervalSince1970 * 1_000)
        do {
            let outcome = try await coordinator.ingestAsync(
                observation: observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            let succeeded = applyObservationOutcome(
                outcome,
                observation: observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
            await finishObservationAsync()
            return succeeded
        } catch let MusicIntegrationIngestError.observation(error) {
            setObservationError(CutoutAppModel.mapRideMapError(error))
            await finishObservationAsync()
            return false
        } catch let MusicIntegrationIngestError.history(error) {
            setObservationError(nil)
            if let error = error as? MobileRideMapError, error == .noActiveRide {
                await finishObservationAsync()
                return false
            }
            setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
            await finishObservationAsync()
            return false
        } catch {
            setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
            await finishObservationAsync()
            return false
        }
    }

    private func setObservationError(_ error: MobileRideMapError?) {
        observationError = error
        refreshHistoryErrorProjection()
    }

    private func applyObservationOutcome(
        _ outcome: MobileMusicTimelineOutcomeDto?,
        observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64
    ) -> Bool {
        setObservationError(nil)
        if outcome == .recorded {
            setHistoryPersistenceError(nil)
            updateCaptureObservation(
                pevcapMusicObservation(
                    from: observation,
                    wallClockAtMs: wallClockAtMs,
                    clockUncertaintyMs: clockUncertaintyMs,
                    rideSequence: coordinator.lastRecordedSequence
                ))
        } else if outcome == .disabled {
            updateCaptureObservation(nil)
            setHistoryPersistenceError(nil)
        } else if outcome == .full {
            updateCaptureObservation(nil)
            setHistoryPersistenceError(.storageError("ride music timeline is full"))
        } else if outcome != nil {
            setHistoryPersistenceError(nil)
        }
        return outcome != .full
    }

    func setHistoryPersistenceError(_ error: MobileRideMapError?) {
        historyPersistenceError = error
        refreshHistoryErrorProjection()
    }

    func clearHistoryErrors() {
        observationError = nil
        historyPersistenceError = nil
        refreshHistoryErrorProjection()
    }

    private func refreshHistoryErrorProjection() {
        historySaveError = observationError ?? historyPersistenceError
    }

    private func finishObservation() {
        timelineEvents = coordinator.recordedEvents
        settingsNowPlaying = projectedNowPlaying()
    }

    private func finishObservationAsync() async {
        do {
            timelineEvents = try await coordinator.recordedEventsAsync()
        } catch {
            setHistoryPersistenceError(CutoutAppModel.mapRideMapError(error))
        }
        settingsNowPlaying = projectedNowPlaying()
    }

    func projectedNowPlaying() -> MusicNowPlaying? {
        guard let current = coordinator.nowPlaying else { return nil }
        guard current.provider != selectedProvider else { return current }
        return MusicNowPlaying(
            observation: unavailableMusicObservation(observedAtMs: monotonicNow())
        )
    }

    private func pevcapMusicObservation(
        from observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64,
        rideSequence: UInt64?
    ) -> MobilePevcapMusicEventDto? {
        guard !historyUnavailable,
            historyPolicy != .disabled,
            let item = observation.snapshot.item,
            let trackID = Self.pevcapTrackIdentifier(
                policy: historyPolicy,
                provider: observation.snapshot.provider,
                identifier: item.identifier
            )
        else { return nil }
        return MobilePevcapMusicEventDto(
            provider: observation.snapshot.provider,
            trackId: trackID,
            monotonicAtMs: observation.snapshot.observedAtMs,
            wallClockUnixMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs,
            rideSequence: rideSequence
        )
    }

    static func pevcapTrackIdentifier(
        policy: MobileMusicHistoryPolicyDto,
        provider: MobileMusicProviderDto,
        identifier: String
    ) -> String? {
        pevcapMusicTrackIdentifier(policy: policy, provider: provider, identifier: identifier)
    }

    private func unavailableMusicObservation(observedAtMs: UInt64) -> MusicProviderObservation {
        .unavailable(provider: selectedProvider, sessionId: "music-unavailable", observedAtMs: observedAtMs)
    }
}
