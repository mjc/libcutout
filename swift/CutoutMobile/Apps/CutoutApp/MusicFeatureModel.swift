import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Application-retained presentation shared by the compact player and setup.
/// Provider lifecycle and Rust-backed history effects are being moved here with
/// their existing owners; this model does not duplicate either domain state.
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
    @ObservationIgnored private let monotonicNow: @MainActor () -> UInt64
    @ObservationIgnored private let updateCapturePolicy: @MainActor (MobileMusicHistoryPolicyDto) -> Void
    @ObservationIgnored private let updateCaptureObservation: @MainActor (MobilePevcapMusicEventDto?) -> Void
    @ObservationIgnored private let invalidateHistoryForDeletion: @MainActor () -> Void
    @ObservationIgnored private let selectedHistoryRideID: @MainActor () -> String?
    @ObservationIgnored private let clearSelectedHistoryMusic: @MainActor () -> Void
    @ObservationIgnored private let setRideHistoryError: @MainActor (MobileRideMapError) -> Void
#if canImport(MediaPlayer) && os(iOS)
    @ObservationIgnored let appleProvider: AppleMusicProviderAdapter
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
        setRideHistoryError: @escaping @MainActor (MobileRideMapError) -> Void
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
        self.providerLifecycle = providerLifecycle
        self.effects = effects
        self.coordinator = MusicIntegrationCoordinator(
            rideMapState: rideMapState,
            lifecycle: providerLifecycle
        )
        self.spotifyProvider = SpotifyProviderAdapter(
            lifecycle: providerLifecycle,
            effects: effects
        )
#if canImport(MediaPlayer) && os(iOS)
        self.appleProvider = AppleMusicProviderAdapter(
            lifecycle: providerLifecycle,
            effects: effects
        )
#endif
        selectedProvider = providerSelectionStore.provider
        isPlayerHidden = playerVisibilityStore.isHidden
        historyPolicy = historyPolicyStore.policy
        timelineEvents = coordinator.recordedEvents
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

    private func rememberHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        historyPolicyStore.set(policy)
        historyPolicy = policy
        historyUnavailable = false
        clearHistoryErrors()
        updateCapturePolicy(policy)
    }

    func applyHistoryForNewRide() -> Bool {
        updateCaptureObservation(nil)
        let defaultPolicy = historyPolicyStore.policy
        historyPolicy = defaultPolicy
        do {
            try coordinator.setHistoryPolicy(defaultPolicy)
            historyUnavailable = false
            clearHistoryErrors()
            coordinator.restoreHistoryPolicy(defaultPolicy)
            timelineEvents = coordinator.recordedEvents
            updateCapturePolicy(defaultPolicy)
            return true
        } catch {
            setRideHistoryError(CutoutAppModel.mapRideMapError(error))
            guard rideMapState?.currentSnapshot() != nil else {
                historyPolicy = .disabled
                historyUnavailable = false
                coordinator.restoreHistoryPolicy(.disabled)
                timelineEvents = []
                return false
            }
            synchronizeHistory(rideMapState?.currentMusicHistory())
            return true
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
            let persistedPolicy = rideMapState?.currentMusicHistoryPolicy() ?? historyPolicy
            historyPolicy = persistedPolicy
            coordinator.restoreHistoryPolicy(persistedPolicy)
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
    func forgetHistory(for rideID: String) -> Bool {
        guard let rideMapState else {
            setRideHistoryError(.storageError("Rust ride database is unavailable"))
            return false
        }
        invalidateHistoryForDeletion()
        do {
            if rideMapState.currentSnapshot()?.rideID == rideID,
               rideMapState.currentSnapshot()?.state.isOpen == true
            {
                try rideMapState.deleteCurrentMusicHistory()
                clearActiveHistory()
            } else {
                try rideMapState.deleteMusicHistory(rideID: rideID)
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
            setObservationError(nil)
            if outcome == .recorded {
                setHistoryPersistenceError(nil)
                updateCaptureObservation(pevcapMusicObservation(
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
            finishObservation()
            return outcome != .full
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

    private func setObservationError(_ error: MobileRideMapError?) {
        observationError = error
        refreshHistoryErrorProjection()
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
