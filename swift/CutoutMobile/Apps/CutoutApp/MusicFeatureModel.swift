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
typealias MusicHistorySnapshotReader = @MainActor () async -> MobileRideMapSnapshotDto?

@MainActor
private final class MusicCommandFeedbackRequest {
    var id: MobileMusicCommandFeedbackId?

    init(id: MobileMusicCommandFeedbackId?) {
        self.id = id
    }
}

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
    @ObservationIgnored private let historySnapshotReader: MusicHistorySnapshotReader
    @ObservationIgnored let spotifyProvider: SpotifyProviderAdapter
    @ObservationIgnored lazy var nativeSoundCloud = SoundCloudNativePlayer()
    @ObservationIgnored let soundcloudProvider: SoundCloudProviderAdapter
    @ObservationIgnored private let spotifyCallbackHandler: (@MainActor (URL) -> Bool)?
    @ObservationIgnored private let monotonicNow: @MainActor () -> UInt64
    @ObservationIgnored private let invalidateHistoryForDeletion: @MainActor () -> Void
    @ObservationIgnored private let selectedHistoryRideID: @MainActor () -> String?
    @ObservationIgnored private let clearSelectedHistoryMusic: @MainActor () -> Void
    @ObservationIgnored private let removeSelectedSpotifyHistory: @MainActor () -> Void

    @ObservationIgnored private let setRideHistoryError: @MainActor (MobileRideMapError) -> Void
    @ObservationIgnored private let providerCommandHandler: MusicProviderCommandHandler?
    #if canImport(MediaPlayer) && os(iOS)
        @ObservationIgnored let appleProvider: AppleMusicProviderAdapter
        @ObservationIgnored private let appleMonitor: any AppleMusicMonitorDriving
        @ObservationIgnored private let waitForMonitorPoll: MusicMonitorPollWaiter
    #endif

    private(set) var isDisconnectingSpotify = false
    var settingsNowPlaying: MusicNowPlaying?
    var timelineEvents = [MobileMusicRideEventDto]()
    var selectedProvider: MobileMusicProviderDto
    var isPlayerHidden: Bool
    private(set) var preferredHistoryPolicy: MobileMusicHistoryPolicyDto
    var historyPolicy: MobileMusicHistoryPolicyDto
    var historyUnavailable = false
    var historySaveError: MobileRideMapError?
    var historyFailureContext: MusicHistoryFailureContext {
        historyPreferenceSaveError == nil ? .listeningHistory : .preferences
    }
    var commandFeedback: MusicCommandFeedback?
    @ObservationIgnored private var activeSpotifyHandoffFeedbackRequest: MusicCommandFeedbackRequest?
    @ObservationIgnored private var observationError: MobileRideMapError?
    @ObservationIgnored private var historyPersistenceError: MobileRideMapError?
    @ObservationIgnored private var historyPreferenceSaveError: MobileRideMapError?
    @ObservationIgnored private var historyReadbackRevisionStorage: UInt64 = 0
    @ObservationIgnored private var historyDiagnosticDeletionRevision: UInt64 = 0
    @ObservationIgnored private var spotifyDisconnectRevision: UInt64 = 0
    @ObservationIgnored private var closedHistoryReadTask: Task<Void, Never>?
    @ObservationIgnored private var closedHistoryReadRequested = false

    var historyReadbackRevision: UInt64 { historyReadbackRevisionStorage }

    var nowPlaying: MusicNowPlaying? {
        isPlayerHidden ? nil : (selectedProvider == .soundCloud ? projectedNowPlaying() : settingsNowPlaying)
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
        invalidateHistoryForDeletion: @escaping @MainActor () -> Void,
        selectedHistoryRideID: @escaping @MainActor () -> String?,
        clearSelectedHistoryMusic: @escaping @MainActor () -> Void,
        removeSelectedSpotifyHistory: @escaping @MainActor () -> Void = {},

        setRideHistoryError: @escaping @MainActor (MobileRideMapError) -> Void,
        appleMonitor: (any AppleMusicMonitorDriving)? = nil,
        monitorPollWaiter: MusicMonitorPollWaiter? = nil,
        providerCommandHandler: MusicProviderCommandHandler? = nil,
        spotifyCallbackHandler: (@MainActor (URL) -> Bool)? = nil,
        historySnapshotReader: MusicHistorySnapshotReader? = nil,
        correlationRideIDReader: (@MainActor () async -> String?)? = nil
    ) {
        let providerLifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let playerVisibilityStore = MusicPlayerVisibilityStore()
        self.playerVisibilityStore = playerVisibilityStore
        self.providerSelectionStore = providerSelectionStore
        self.historyPolicyStore = historyPolicyStore
        self.monitoringPreferenceStore = monitoringPreferenceStore
        self.rideMapState = rideMapState
        self.historySnapshotReader = historySnapshotReader ?? { await rideMapState?.currentSnapshotAsync() }
        self.monotonicNow = monotonicNow
        self.invalidateHistoryForDeletion = invalidateHistoryForDeletion
        self.selectedHistoryRideID = selectedHistoryRideID
        self.clearSelectedHistoryMusic = clearSelectedHistoryMusic
        self.removeSelectedSpotifyHistory = removeSelectedSpotifyHistory

        self.setRideHistoryError = setRideHistoryError
        self.providerCommandHandler = providerCommandHandler
        self.providerLifecycle = providerLifecycle
        self.effects = effects
        self.coordinator = MusicIntegrationCoordinator(
            rideMapState: rideMapState,
            lifecycle: providerLifecycle,
            correlationRideIDReader: correlationRideIDReader
        )
        let spotifyProvider = SpotifyProviderAdapter(
            lifecycle: providerLifecycle,
            effects: effects
        )
        self.spotifyProvider = spotifyProvider
        self.soundcloudProvider = SoundCloudProviderAdapter(
            lifecycle: providerLifecycle, effects: effects, nowMs: monotonicNow)
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
        preferredHistoryPolicy = historyPolicyStore.policy
        historyPolicy = .disabled
        timelineEvents = []
        if selectedProvider.profile.interface == .appHandoffOnly {
            soundcloudProvider.start()
            settingsNowPlaying = projectedNowPlaying()
        }
    }

    @discardableResult
    func setHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) -> Bool {
        advanceHistoryReadbackRevision()
        guard rideMapState != nil else {
            rememberFutureHistoryPolicy(policy)
            return true
        }
        #if DEBUG
            print("music_history_request policy=\(policy) has_ride_store=\(rideMapState != nil)")
        #endif
        let previous = historyPolicy
        clearHistoryErrors()
        do {
            try coordinator.setHistoryPolicy(policy)
            historyUnavailable = false
            rememberHistoryPolicy(policy)
            timelineEvents = coordinator.recordedEvents
            return true
        } catch let error as MobileRideMapError where error == .noActiveRide || error == .invalidTransition {
            rememberFutureHistoryPolicy(policy)
            return true
        } catch {
            #if DEBUG
                print("music_history_rejected error=\(error)")
            #endif
            historyPolicy = previous
            historyPreferenceSaveError = appRideMapError(error)
            refreshHistoryErrorProjection()
            return false
        }
    }

    @discardableResult
    func setHistoryPolicyAsync(_ policy: MobileMusicHistoryPolicyDto) async -> Bool {
        advanceHistoryReadbackRevision()
        let operationRevision = historyReadbackRevision
        let targetRideID = await historySnapshotReader()?.rideID
        var stillTargetsCurrentRide = true
        defer {
            if stillTargetsCurrentRide, historyReadbackRevision == operationRevision {
                advanceHistoryReadbackRevision()
            }
        }
        guard operationRevision == historyReadbackRevision else { return false }
        guard let rideMapState else {
            rememberFutureHistoryPolicy(policy)
            return true
        }
        let previous = historyPolicy
        clearHistoryErrors()
        do {
            try await rideMapState.setMusicHistoryPolicyAsync(policy)
            let current = await historySnapshotReader()
            stillTargetsCurrentRide = current?.rideID == targetRideID
            guard operationRevision == historyReadbackRevision else { return false }
            guard stillTargetsCurrentRide else {
                rememberPreferredHistoryPolicy(policy)
                return true
            }
            coordinator.restoreHistoryPolicy(policy)
            rememberHistoryPolicy(policy)
            if policy == .disabled {
                timelineEvents = []
            } else {
                do {
                    let events = try await coordinator.recordedEventsAsync()
                    let current = await historySnapshotReader()
                    stillTargetsCurrentRide = current?.rideID == targetRideID
                    guard operationRevision == historyReadbackRevision else { return false }
                    if stillTargetsCurrentRide, historyPolicy == policy { timelineEvents = events }
                } catch {
                    let current = await historySnapshotReader()
                    stillTargetsCurrentRide = current?.rideID == targetRideID
                    guard operationRevision == historyReadbackRevision else { return false }
                    if stillTargetsCurrentRide, historyPolicy == policy {
                        setHistoryPersistenceError(appRideMapError(error))
                    }
                }
            }
            let settledCurrent = await historySnapshotReader()
            stillTargetsCurrentRide = settledCurrent?.rideID == targetRideID
            guard operationRevision == historyReadbackRevision else { return false }
            return true
        } catch let error as MobileRideMapError where error == .noActiveRide || error == .invalidTransition {
            let current = await historySnapshotReader()
            stillTargetsCurrentRide = current?.rideID == targetRideID
            guard operationRevision == historyReadbackRevision else { return false }
            if stillTargetsCurrentRide {
                rememberFutureHistoryPolicy(policy)
            } else {
                rememberPreferredHistoryPolicy(policy)
            }
            return true
        } catch {
            let current = await historySnapshotReader()
            stillTargetsCurrentRide = current?.rideID == targetRideID
            guard operationRevision == historyReadbackRevision else { return false }
            if stillTargetsCurrentRide {
                historyPolicy = previous
                historyPreferenceSaveError = appRideMapError(error)
                refreshHistoryErrorProjection()
            }
            return false
        }
    }

    private func rememberHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        rememberPreferredHistoryPolicy(policy)
        historyPolicy = policy
        historyUnavailable = false
        clearHistoryErrors()
    }

    private func rememberPreferredHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        historyPolicyStore.set(policy)
        preferredHistoryPolicy = policy
    }

    private func rememberFutureHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        rememberPreferredHistoryPolicy(policy)
        historyPolicy = .disabled
        historyUnavailable = false
        clearHistoryErrors()
        coordinator.restoreHistoryPolicy(.disabled)
        refreshClosedHistoryTimeline()
    }

    /// Adopts the history Rust created atomically with the ride; this must never write a policy.
    func adoptHistoryForNewRideAsync() async -> MobileRideMapError? {
        advanceHistoryReadbackRevision()
        let readbackRevision = historyReadbackRevision
        let targetRideID = (await historySnapshotReader())?.rideID
        let policy = await rideMapState?.currentMusicHistoryPolicyAsync() ?? .disabled
        guard (await historySnapshotReader())?.rideID == targetRideID,
            readbackRevision == historyReadbackRevision
        else { return nil }
        timelineEvents = []
        historyPolicy = policy
        historyUnavailable = false
        clearHistoryErrors()
        coordinator.restoreHistoryPolicy(historyPolicy)
        do {
            let history = try await rideMapState?.currentMusicHistoryAsync()
            let current = await historySnapshotReader()
            guard
                readbackRevision == historyReadbackRevision,
                current?.rideID == targetRideID
            else { return nil }
            _ = synchronizeHistory(history, ifCurrentRevision: readbackRevision)
            return nil
        } catch {
            let current = await historySnapshotReader()
            guard
                readbackRevision == historyReadbackRevision,
                current?.rideID == targetRideID
            else { return nil }
            let mappedError = appRideMapError(error)
            setHistoryPersistenceError(mappedError)
            setRideHistoryError(mappedError)
            guard current != nil else {
                synchronizeHistory(nil)
                setHistoryPersistenceError(mappedError)
                return mappedError
            }
            return mappedError
        }
    }

    func synchronizeHistory(_ history: MobileMusicHistoryDto?) {
        advanceHistoryReadbackRevision()
        applySynchronizedHistory(history)
    }

    @discardableResult
    func synchronizeHistory(
        _ history: MobileMusicHistoryDto?,
        ifCurrentRevision revision: UInt64
    ) -> Bool {
        guard revision == historyReadbackRevision else { return false }
        advanceHistoryReadbackRevision()
        applySynchronizedHistory(history)
        return true
    }

    private func applySynchronizedHistory(_ history: MobileMusicHistoryDto?) {
        guard let history else {
            historyUnavailable = false
            clearHistoryErrors()
            historyPolicy = .disabled
            coordinator.restoreHistoryPolicy(.disabled)
            timelineEvents = []
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
    }

    @discardableResult
    func forgetHistory(for rideID: String) async -> Bool {
        guard let rideMapState else {
            setRideHistoryError(.storageError("Rust ride database is unavailable"))
            return false
        }
        let initialRevision = historyReadbackRevision
        let currentRide = await historySnapshotReader()
        guard initialRevision == historyReadbackRevision else { return false }
        let invalidatesCurrentRide = currentRide?.rideID == rideID
        if invalidatesCurrentRide { advanceHistoryReadbackRevision() }
        let operationRevision = historyReadbackRevision
        defer {
            if invalidatesCurrentRide, historyReadbackRevision == operationRevision {
                advanceHistoryReadbackRevision()
            }
        }
        invalidateHistoryForDeletion()
        do {
            try await rideMapState.deleteMusicHistoryAsync(rideID: rideID)
            let current = await historySnapshotReader()
            if operationRevision == historyReadbackRevision, current?.rideID == rideID {
                clearActiveHistory()
            }
            if selectedHistoryRideID() == rideID { clearSelectedHistoryMusic() }
            // Explicit deletion conservatively clears all retained history diagnostics
            // without altering live playback or transport state.
            historyDiagnosticDeletionRevision &+= 1
            coordinator.clearPresentationHistoryFailures()
            refreshHistoryErrorProjection()
            return true
        } catch {
            if operationRevision == historyReadbackRevision { setRideHistoryError(appRideMapError(error)) }
            return false
        }
    }

    private func clearActiveHistory() {
        historyPolicy = .disabled
        historyUnavailable = false
        clearHistoryErrors()
        coordinator.restoreHistoryPolicy(.disabled)
        providerLifecycle.clearPendingCommandCorrelation()
        timelineEvents = []
    }

    func rideMapClosed() {
        advanceHistoryReadbackRevision()
        historyPolicy = .disabled
        historyUnavailable = false
        coordinator.restoreHistoryPolicy(.disabled)
        refreshClosedHistoryTimeline()
    }

    private func refreshClosedHistoryTimeline() {
        closedHistoryReadRequested = true
        guard closedHistoryReadTask == nil else { return }
        let coordinator = coordinator
        closedHistoryReadTask = Task { [weak self] in
            while let revision = self?.takeClosedHistoryReadRevision() {
                do {
                    let events = try await coordinator.recordedEventsAsync()
                    guard let self else { return }
                    if revision == self.historyReadbackRevision { self.timelineEvents = events }
                } catch {
                    guard let self else { return }
                    if revision == self.historyReadbackRevision {
                        self.setHistoryPersistenceError(appRideMapError(error))
                    }
                }
            }
            self?.closedHistoryReadTask = nil
        }
    }

    private func takeClosedHistoryReadRevision() -> UInt64? {
        guard closedHistoryReadRequested else { return nil }
        closedHistoryReadRequested = false
        return historyReadbackRevision
    }

    private func advanceHistoryReadbackRevision() {
        historyReadbackRevisionStorage &+= 1
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
        guard !(selectedProvider == .spotify && isDisconnectingSpotify) else { return }
        if selectedProvider.profile.interface == .appHandoffOnly {
            settingsNowPlaying = projectedNowPlaying()
            return
        }
        #if canImport(MediaPlayer) && os(iOS)
            let observedAtMs = monotonicNow()
            let observation: MusicProviderObservation
            switch selectedProvider.monitoringMode {
            case .appleMusicSystemPlayer:
                appleMonitor.refreshObservation(observedAtMs: observedAtMs)
                return
            case .spotifyAppRemote:
                observation = spotifyProvider.observation(observedAtMs: observedAtMs)
            case .appHandoffOnly:
                return
            case .unavailable:
                observation = MusicProviderObservation(
                    snapshot: spotifyProvider.unavailableSnapshot(observedAtMs: observedAtMs)
                )
            }
            submitObservation(observation)
        #endif
    }

    func stopMonitoring() {
        let completion = providerLifecycle.cancelMonitor()
        soundcloudProvider.applyCompletion(completion)
        let suspension = MobileMusicProviderSuspension(
            observationGap: false,
            cancelledTransportRequestId: completion.requestId
        )
        #if canImport(MediaPlayer) && os(iOS)
            appleMonitor.applySuspension(suspension)
        #endif
        spotifyProvider.applySuspension(suspension)
        stopProviderWork()
    }

    private func stopProviderWork() {
        soundcloudProvider.stop()
        effects.cancelAll(in: .monitor)
        #if canImport(MediaPlayer) && os(iOS)
            appleMonitor.stopMonitoring()
        #endif
        #if canImport(SpotifyiOS) && os(iOS)
            spotifyProvider.stopMonitoring()
        #endif
    }

    func start(sceneIsActive: Bool) {
        guard !isDisconnectingSpotify else { return }
        if selectedProvider.profile.interface == .appHandoffOnly {
            if sceneIsActive { soundcloudProvider.start() }
            settingsNowPlaying = projectedNowPlaying()
            return
        }
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
        soundcloudProvider.applySuspension(suspension)
        if selectedProvider.profile.interface == .appHandoffOnly {
            stopProviderWork()
            settingsNowPlaying = projectedNowPlaying()
            return
        }
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
            submitObservation(observation)
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
        let request: MobileMusicProviderMonitorRequest =
            settingsNowPlaying == nil || settingsNowPlaying?.state == .unauthorized ? .authorize : .observe
        requestMonitoring(request)
    }

    private func requestMonitoring(_ request: MobileMusicProviderMonitorRequest) {
        guard !isDisconnectingSpotify else { return }
        monitoringPreferenceStore.setEnabled(true)
        _ = providerLifecycle.resume()
        if request == .observe, effects.isRunning(in: .monitor) { return }
        providerLifecycle.requestMonitor(request: request)
        beginMonitoring()
    }

    func authorizeSpotify() {
        guard !isDisconnectingSpotify else { return }
        #if canImport(SpotifyiOS) && os(iOS)
            guard selectedProvider == .spotify else { return }
            spotifyProvider.clearAuthorization()
            requestMonitoring(.authorize)
        #endif
    }

    @discardableResult
    func disconnectSpotify() async -> Bool {
        guard !isDisconnectingSpotify else { return false }
        spotifyDisconnectRevision &+= 1
        isDisconnectingSpotify = true
        defer {
            advanceHistoryReadbackRevision()
            isDisconnectingSpotify = false
        }
        // Rust retires admitted work before the ordered database cleanup.
        monitoringPreferenceStore.setEnabled(false)
        stopMonitoring()
        let credentialsRemoved = clearSpotifyAuthorization()
        activeSpotifyHandoffFeedbackRequest = nil
        providerLifecycle.invalidateCommandFeedback()
        commandFeedback = nil
        coordinator.resetProviderCorrelation()
        if selectedProvider == .spotify { settingsNowPlaying = nil }
        advanceHistoryReadbackRevision()
        historyDiagnosticDeletionRevision &+= 1
        coordinator.clearPresentationHistoryFailures()
        invalidateHistoryForDeletion()
        guard let rideMapState else {
            setRideHistoryError(.storageError("Rust ride database is unavailable"))
            return false
        }
        do {
            try await rideMapState.deleteSpotifyMusicHistoryAsync()
            timelineEvents.removeAll { $0.provider == .spotify }
            removeSelectedSpotifyHistory()
            setHistoryPersistenceError(nil)
            if !credentialsRemoved {
                setRideHistoryError(.storageError("Spotify credentials could not be removed"))
            }
            return credentialsRemoved
        } catch {
            let mapped = appRideMapError(error)
            setHistoryPersistenceError(mapped)
            setRideHistoryError(mapped)
            return false
        }
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
        requestMonitoring(.observe)
    }

    func selectProvider(_ provider: MobileMusicProviderDto) {
        guard !isDisconnectingSpotify else { return }
        let previousProvider = selectedProvider
        if previousProvider == .soundCloud, provider != .soundCloud { nativeSoundCloud.stop() }
        #if canImport(SpotifyiOS) && os(iOS)
            if previousProvider != provider {
                spotifyProvider.cancelPendingPlayHandoff()
            }
        #endif
        activeSpotifyHandoffFeedbackRequest = nil
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
        #if canImport(SpotifyiOS) && os(iOS)
            let commandDisconnectRevision = spotifyDisconnectRevision
        #endif
        guard !(commandProvider == .spotify && isDisconnectingSpotify) else { return .unavailable }
        if commandProvider == .soundCloud, nativeSoundCloud.snapshot.track != nil, command != .openProvider {
            let requestID = beginCommandFeedback()
            guard projectedNowPlaying()?.isCommandAvailable(command) == true else {
                return finishCommand(.refused, provider: commandProvider, requestID: requestID)
            }
            switch command {
            case .play: nativeSoundCloud.command(.play)
            case .pause: nativeSoundCloud.command(.pause)
            case .previous: nativeSoundCloud.command(.previous)
            case .next: nativeSoundCloud.command(.next)
            case .openProvider: break
            }
            return finishCommand(.accepted, provider: commandProvider, requestID: requestID)
        }
        if commandProvider.profile.interface == .appHandoffOnly { soundcloudProvider.start() }
        let feedbackRequest = MusicCommandFeedbackRequest(id: beginCommandFeedback())
        activeSpotifyHandoffFeedbackRequest = feedbackRequest
        let requestID = feedbackRequest.id
        if commandProvider.profile.interface == .appHandoffOnly {
            let result = await soundcloudProvider.perform(command)
            return finishCommand(result, provider: commandProvider, requestID: requestID)
        }
        #if canImport(MediaPlayer) && os(iOS)
            if command == .openProvider {
                let result = await performProviderCommand(command, provider: commandProvider)
                return finishProviderCommand(result, provider: commandProvider, requestID: requestID)
            }
        #endif
        guard let nowPlaying else {
            if command == .play, commandProvider == .spotify {
                #if canImport(SpotifyiOS) && os(iOS)
                    if providerCommandHandler == nil {
                        let outcome = await spotifyProvider.perform(
                            command,
                            onChange: { [weak self] in
                                guard let self,
                                    self.isCurrentSpotifyObservation(disconnectRevision: commandDisconnectRevision)
                                else { return }
                                self.refreshSnapshot()
                            },
                            onHandoffStarted: { [weak self] in
                                self?.beginSpotifyHandoffFeedback(feedbackRequest, provider: commandProvider)
                            },
                            onCommandFailure: { [weak self] in
                                self?.finishSpotifyHandoffFailure(feedbackRequest, provider: commandProvider)
                            })
                        return finishProviderCommand(
                            outcome,
                            provider: commandProvider,
                            requestID: feedbackRequest.id
                        )
                    }
                #endif
                let outcome = await performProviderCommand(command, provider: commandProvider)
                return finishProviderCommand(
                    outcome,
                    provider: commandProvider,
                    requestID: feedbackRequest.id
                )
            }
            return finishCommand(.unavailable, provider: commandProvider, requestID: requestID)
        }
        guard nowPlaying.isCommandAvailable(command) else {
            return finishCommand(.refused, provider: commandProvider, requestID: requestID)
        }
        #if canImport(MediaPlayer) && os(iOS)
            let result: MusicCommandOutcome
            #if canImport(SpotifyiOS) && os(iOS)
                if nowPlaying.provider == .spotify, providerCommandHandler == nil {
                    result = await spotifyProvider.perform(
                        command,
                        onChange: { [weak self] in
                            guard let self,
                                self.isCurrentSpotifyObservation(disconnectRevision: commandDisconnectRevision)
                            else { return }
                            self.refreshSnapshot()
                        },
                        onHandoffStarted: { [weak self] in
                            self?.beginSpotifyHandoffFeedback(feedbackRequest, provider: commandProvider)
                        },
                        onCommandFailure: { [weak self] in
                            self?.finishSpotifyHandoffFailure(feedbackRequest, provider: commandProvider)
                        })
                } else {
                    result = await performProviderCommand(command, provider: nowPlaying.provider)
                }
            #else
                result = await performProviderCommand(command, provider: nowPlaying.provider)
            #endif
            if result == .accepted, commandProvider != .spotify || isCurrentSpotifyCommand(feedbackRequest.id) {
                refreshSnapshot()
            }
            return finishProviderCommand(result, provider: commandProvider, requestID: feedbackRequest.id)
        #else
            return finishCommand(.unavailable, provider: commandProvider, requestID: requestID)
        #endif
    }

    private func isCurrentSpotifyCommand(_ requestID: MobileMusicCommandFeedbackId?) -> Bool {
        guard !isDisconnectingSpotify, let requestID else { return false }
        return providerLifecycle.classifyCommandFeedback(id: requestID) == .current
    }

    private func isCurrentSpotifyObservation(disconnectRevision: UInt64) -> Bool {
        !isDisconnectingSpotify
            && selectedProvider == .spotify
            && spotifyDisconnectRevision == disconnectRevision
    }

    private func finishProviderCommand(
        _ outcome: MusicCommandOutcome,
        provider: MobileMusicProviderDto,
        requestID: MobileMusicCommandFeedbackId?
    ) -> MusicCommandOutcome {
        guard provider != .spotify || isCurrentSpotifyCommand(requestID) else { return .unavailable }
        return finishCommand(outcome, provider: provider, requestID: requestID)
    }

    private func beginSpotifyHandoffFeedback(
        _ request: MusicCommandFeedbackRequest,
        provider: MobileMusicProviderDto
    ) {
        guard activeSpotifyHandoffFeedbackRequest === request,
            selectedProvider == provider
        else { return }
        request.id = beginCommandFeedback()
    }

    private func finishSpotifyHandoffFailure(
        _ request: MusicCommandFeedbackRequest,
        provider: MobileMusicProviderDto
    ) {
        guard activeSpotifyHandoffFeedbackRequest === request,
            selectedProvider == provider,
            let requestID = beginCommandFeedback()
        else { return }
        request.id = requestID
        _ = finishCommand(.failed, provider: provider, requestID: requestID)
        activeSpotifyHandoffFeedbackRequest = nil
    }

    private func performProviderCommand(
        _ command: MobileMusicCommandDto,
        provider: MobileMusicProviderDto
    ) async -> MusicCommandOutcome {
        if let providerCommandHandler {
            return await providerCommandHandler(provider, command)
        }
        #if canImport(MediaPlayer) && os(iOS)
            switch provider.profile.interface {
            case .spotifyAppRemote: return await spotifyProvider.perform(command)
            case .appleMusicSystemPlayer: return await appleProvider.perform(command)
            case .appHandoffOnly: return await soundcloudProvider.perform(command)
            }
        #else
            return .unavailable
        #endif
    }

    private func updateMonitoring(
        from previousProvider: MobileMusicProviderDto,
        to provider: MobileMusicProviderDto
    ) {
        switch provider.monitoringMode {
        case .appHandoffOnly:
            stopMonitoring()
            soundcloudProvider.start()
            settingsNowPlaying = projectedNowPlaying()
        case .unavailable:
            stopMonitoring()
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
        if selectedProvider.profile.interface == .appHandoffOnly {
            stopProviderWork()
            soundcloudProvider.start()
            settingsNowPlaying = projectedNowPlaying()
            return
        }
        guard let effect = providerLifecycle.beginMonitor() else { return }
        // Invalidate before stopping: teardown may resume cancelled command work synchronously.
        providerLifecycle.invalidateCommandFeedback()
        commandFeedback = nil
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
                        self?.submitObservation(observation)
                    },
                    refresh: { [weak self] in self?.refreshSnapshot() }
                )
                self?.finishMonitoring(generation: generation)
            }
        #else
            let observation = unavailableMusicObservation(observedAtMs: monotonicNow())
            coordinator.update(snapshot: observation.snapshot)
            settingsNowPlaying = projectedNowPlaying()
            let generation = effect.generation
            let observationTask = submitObservation(observation)
            effects.run(.monitor(generation)) { [weak self, observationTask] in
                _ = await observationTask?.value
                self?.finishMonitoring(generation: generation)
            }
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
            let succeeded = applyObservationOutcome(outcome)
            finishObservation()
            return succeeded
        } catch let MusicIntegrationIngestError.observation(error) {
            setObservationError(appRideMapError(error))
            finishObservation()
            return false
        } catch let MusicIntegrationIngestError.history(error) {
            setObservationError(nil)
            if let error = error as? MobileRideMapError, error == .noActiveRide {
                finishObservation()
                return false
            }
            setHistoryPersistenceError(appRideMapError(error))
            finishObservation()
            return false
        } catch {
            setHistoryPersistenceError(appRideMapError(error))
            finishObservation()
            return false
        }
    }

    private func clearSpotifyAuthorization() -> Bool {
        #if canImport(SpotifyiOS) && os(iOS)
            spotifyProvider.clearAuthorization()
        #else
            true
        #endif
    }

    @discardableResult
    func ingestObservationAsync(
        _ observation: MusicProviderObservation,
        wallClockAtMs: UInt64? = nil,
        clockUncertaintyMs: UInt64 = 1_000
    ) async -> Bool {
        guard
            let task = submitObservation(
                observation, wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs
            )
        else { return false }
        return await task.value
    }

    @discardableResult
    func submitObservation(
        _ observation: MusicProviderObservation,
        wallClockAtMs: UInt64? = nil,
        clockUncertaintyMs: UInt64 = 1_000,
        historyReadback: (@MainActor () async throws -> [MobileMusicRideEventDto])? = nil
    ) -> Task<Bool, Never>? {
        guard !(observation.snapshot.provider == .spotify && isDisconnectingSpotify) else { return nil }
        let coordinator = coordinator
        let submission = coordinator.beginObservation()
        switch submission.admission() {
        case .admitted:
            break
        case .full:
            setHistoryPersistenceError(.storageError("music observation queue is full"))
            submission.release()
            return nil
        case .exhausted:
            setHistoryPersistenceError(.storageError("music observation identities exhausted"))
            submission.release()
            return nil
        }
        let wallClockAtMs = wallClockAtMs ?? UInt64(Date().timeIntervalSince1970 * 1_000)
        let readbackRevision = historyReadbackRevision
        let diagnosticDeletionRevision = historyDiagnosticDeletionRevision
        let readHistory = historyReadback ?? { try await coordinator.recordedEventsAsync() }
        return Task { [weak self, coordinator, submission] in
            defer { submission.release() }
            do {
                let completion = try await coordinator.completeObservation(
                    submission: submission,
                    observation: observation,
                    wallClockAtMs: wallClockAtMs,
                    clockUncertaintyMs: clockUncertaintyMs
                )
                guard !Task.isCancelled, submission.isCurrent() else { return false }
                // This observation owns only its Rust SQLite history receipt.
                // BTLE diagnostics do not carry listening metadata.
                try submission.settle()
                var events: [MobileMusicRideEventDto]?
                var readError: MobileRideMapError?
                do { events = try await readHistory() } catch { readError = appRideMapError(error) }
                guard !Task.isCancelled, submission.isCurrent(), let self else { return false }
                if diagnosticDeletionRevision != self.historyDiagnosticDeletionRevision {
                    coordinator.clearPresentationHistoryFailures()
                }
                self.refreshHistoryErrorProjection()
                if readbackRevision == self.historyReadbackRevision {
                    self.setObservationError(nil)
                    if completion.outcome == .full {
                        self.setHistoryPersistenceError(.storageError("ride music timeline is full"))
                    } else if completion.outcome != nil {
                        self.setHistoryPersistenceError(nil)
                    }
                    if let events { self.timelineEvents = events }
                    if let readError { self.setHistoryPersistenceError(readError) }
                }
                self.settingsNowPlaying = completion.nowPlaying
                return completion.outcome != .full
            } catch {
                submission.failRequiredEffects()
                // Required failure presentation must not wait for optional history delivery.
                if !Task.isCancelled, submission.isCurrent(), let self {
                    if diagnosticDeletionRevision != self.historyDiagnosticDeletionRevision {
                        coordinator.clearPresentationHistoryFailures()
                    }
                    self.refreshHistoryErrorProjection()
                    if readbackRevision == self.historyReadbackRevision {
                        switch error {
                        case let MusicIntegrationIngestError.observation(observationError):
                            self.setObservationError(appRideMapError(observationError))
                        case let MusicIntegrationIngestError.history(historyError):
                            self.setObservationError(nil)
                            if (historyError as? MobileRideMapError) != .noActiveRide {
                                self.setHistoryPersistenceError(appRideMapError(historyError))
                            }
                        default:
                            self.setHistoryPersistenceError(appRideMapError(error))
                        }
                    }
                    self.settingsNowPlaying = self.projectedNowPlaying()
                }
                var events: [MobileMusicRideEventDto]?
                do { events = try await readHistory() } catch {}
                guard !Task.isCancelled, submission.isCurrent(), let self else { return false }
                if diagnosticDeletionRevision != self.historyDiagnosticDeletionRevision {
                    coordinator.clearPresentationHistoryFailures()
                }
                self.refreshHistoryErrorProjection()
                if readbackRevision == self.historyReadbackRevision {
                    if let events { self.timelineEvents = events }
                }
                self.settingsNowPlaying = self.projectedNowPlaying()
                return false
            }
        }
    }

    private func setObservationError(_ error: MobileRideMapError?) {
        observationError = error
        refreshHistoryErrorProjection()
    }

    private func applyObservationOutcome(_ outcome: MobileMusicTimelineOutcomeDto?) -> Bool {
        setObservationError(nil)
        if outcome == .full {
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
        historyPreferenceSaveError = nil
        observationError = nil
        historyPersistenceError = nil
        refreshHistoryErrorProjection()
    }

    private func refreshHistoryErrorProjection() {
        historySaveError =
            historyPreferenceSaveError ?? observationError ?? historyPersistenceError
            ?? coordinator.previousRideHistoryFailure.map { _ in
                .storageError("Previous ride listening history is incomplete")
            }
    }

    private func finishObservation() {
        timelineEvents = coordinator.recordedEvents
        settingsNowPlaying = projectedNowPlaying()
    }

    func projectedNowPlaying() -> MusicNowPlaying? {
        if selectedProvider == .soundCloud, nativeSoundCloud.snapshot.track != nil {
            return MusicNowPlaying(snapshot: nativeSoundCloud.musicSnapshot(nowMs: monotonicNow()))
        }
        if selectedProvider.profile.interface == .appHandoffOnly {
            return MusicNowPlaying(snapshot: soundcloudUnavailableSnapshot(nowMs: monotonicNow()))
        }
        guard let current = coordinator.nowPlaying else { return nil }
        guard current.provider != selectedProvider else { return current }
        return MusicNowPlaying(
            observation: unavailableMusicObservation(observedAtMs: monotonicNow())
        )
    }

    private func unavailableMusicObservation(observedAtMs: UInt64) -> MusicProviderObservation {
        .unavailable(provider: selectedProvider, sessionId: "music-unavailable", observedAtMs: observedAtMs)
    }
}
