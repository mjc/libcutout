import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

private func normalizedRideMapHistorySearchText(_ text: String) -> String? {
    let normalized = text.trimmingCharacters(in: .whitespacesAndNewlines)
    return normalized.isEmpty ? nil : normalized
}

private enum RideSessionRestorationState {
    case complete
    case awaitingBluetooth
    case awaitingSnapshot(platformIdentifier: String)
    case recovering
}

struct MusicMonitorSceneState: Equatable {
    private(set) var isSceneActive = true
    private(set) var isRequested = false

    mutating func request() {
        isRequested = true
    }

    mutating func cancel() {
        isRequested = false
    }

    mutating func suspend() {
        isSceneActive = false
    }

    mutating func resumeIfNeeded() -> Bool {
        let shouldResume = isRequested && !isSceneActive
        isSceneActive = true
        return shouldResume
    }
}

@MainActor
@Observable
final class CutoutAppModel {

    private nonisolated static func runCancellableDetached<Success: Sendable>(
        priority: TaskPriority,
        operation: @escaping @Sendable () throws -> Success
    ) async throws -> Success {
        let task = Task.detached(priority: priority) {
            try Task.checkCancellation()
            let result = try operation()
            try Task.checkCancellation()
            return result
        }
        return try await withTaskCancellationHandler(operation: {
            try await task.value
        }, onCancel: {
            task.cancel()
        })
    }
    enum RideMapMode: String {
        case live
        case history
    }

    enum RideMapHistoryDateFilter: String {
        case last30Days
        case allTime
    }

    nonisolated private static var rideMapLimits: MobileRideMapLimits { .rustOwned }

    private(set) var displayState = RideDisplayState()
    private(set) var phase = SessionConnectionPhase.starting
    private(set) var devicePickerScanState: DevicePickerScanState?
    private(set) var connectionState = ConnectionState.picker
    private(set) var settingsReadback: SettingsReadback?
    private(set) var faultHistoryReadback: FaultHistoryReadback?
    private(set) var bmsSnapshot: BmsSnapshot?
    private(set) var phoneLocationReadback = PhoneLocationReadback(
        snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    )
    private(set) var rideMapSnapshot: MobileRideMapSnapshotDto?
    private(set) var rideMapStorageError: String?
    private(set) var rideMapAvailability = MobileRideMapAvailability.checking
    private(set) var rideMapLiveError: MobileRideMapError?
    private(set) var rideMapHistoryError: MobileRideMapError?
    private(set) var rideMapHistoryRouteError: MobileRideMapError?
    private(set) var rideMapHistoryDetailRouteError: MobileRideMapError?
    private(set) var rideMapLiveDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var rideMapLiveCameraRegion: MobileRideMapCameraRegion?
    private(set) var rideMapLiveEndpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var rideMapLiveSegments = [MobileRideMapSegmentDisplayMetadata]()
    private(set) var rideMapLiveTelemetryState: MobileRideMapTelemetryStateDto?
    private(set) var rideMapLiveBackgroundGapCount: UInt64 = 0
    private(set) var rideMapLiveProjectionVersion: UInt64 = 0
    private(set) var rideMapLivePointsTruncated = false
    private(set) var rideMapLiveSegmentsOmittedByBudget = false
    private(set) var rideMapHistory = [MobileRideMapHistorySummaryDto]()
    private(set) var rideMapHistoryCanLoadMore = false
    var rideMapHistorySearchText = ""
    private(set) var rideMapHistoryDateFilter = RideMapHistoryDateFilter.last30Days
    private(set) var rideMapHistoryVehicleFilter: String?
    private(set) var rideMapHistoryDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var rideMapHistoryCameraRegion: MobileRideMapCameraRegion?
    private(set) var rideMapHistoryEndpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var rideMapHistorySegments = [MobileRideMapSegmentDisplayMetadata]()
    private(set) var rideMapHistoryBackgroundGapCount: UInt64 = 0
    private(set) var rideMapHistoryPointsTruncated = false
    private(set) var rideMapHistorySegmentsOmittedByBudget = false
    private(set) var rideMapHistoryDetailDisplayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var rideMapHistoryDetailMusicTimeline = [MobileMusicRideEventDto]()
    private(set) var rideMapHistoryDetailCameraRegion: MobileRideMapCameraRegion?
    private(set) var rideMapHistoryDetailEndpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var rideMapHistoryDetailSegments = [MobileRideMapSegmentDisplayMetadata]()
    private(set) var rideMapHistoryDetailBackgroundGapCount: UInt64 = 0
    private(set) var rideMapHistoryDetailPointsTruncated = false
    private(set) var rideMapHistoryDetailSourcePointsOmittedByBudget = false
    private(set) var rideMapHistoryDetailSourceSegmentsOmittedByBudget = false
    private(set) var rideMapHistoryDetailSegmentsOmittedByBudget = false
    private(set) var rideMapHistoryContextRoutes = [MobileRideMapHistoryContextRoute]()
    private(set) var rideMapHistoryContextProjection: MobileRideMapHistoryContextProjection?
    private(set) var rideMapHistoryProjectionVersion: UInt64 = 0
    private(set) var rideMapHistoryDetailProjectionVersion: UInt64 = 0
    private(set) var rideMapHistoryRouteLoading = false
    private(set) var rideMapHistoryDetailRouteLoading = false
    private(set) var rideMapHistoryVehicleIdentities = [String]()
    private(set) var rideMapHistoryVehicleNames = [String: String]()
    private(set) var selectedRideMapHistoryID: String?
    private(set) var rideMapLastDecision: MobileRideMapDecisionDto?
    var rideMapMode = RideMapMode.live
    private(set) var rideMapHistoryLoading = false
    private(set) var musicNowPlaying: MusicNowPlaying?
    private(set) var musicTimelineEvents = [MobileMusicRideEventDto]()
    private(set) var selectedMusicProvider = MobileMusicProviderDto.appleMusic
    private(set) var isMusicPlayerHidden: Bool
    private(set) var musicHistoryPolicy = MobileMusicHistoryPolicyDto.disabled

    /// Compatibility projection for callers that only display the live map.
    /// New route presentations should use the explicitly scoped error properties.
    var rideMapError: MobileRideMapError? { rideMapLiveError }
    private(set) var captureStatus: CaptureStatus?
    private(set) var captureProgress: CaptureProgress?
    private(set) var liveActivityError: LiveActivityRideLifecycleError?
    private(set) var isRecordOnlyCapture = false
    private(set) var isFinishingCapture = false
    private(set) var activeCaptureLabels = Set<CaptureQuickLabel>()
    private(set) var recordOnlyDeviceKind: String?
    private(set) var hasSavedDevice = false

    var selectedRideTitle: String? {
        connectionState.selection?.title
    }

    /// Stable identity/name pair used to relabel persisted ride-history vehicles.
    /// The persisted selection is the fallback when the connection state has not rebuilt yet.
    var rideMapVehicleIdentity: String? {
        connectionState.selection?.platformIdentifier ?? selectedDeviceStore.platformIdentifier
    }

    var rideMapVehicleName: String? {
        if let identity = rideMapVehicleIdentity {
            if let cached = rideMapVehicleNameCache[identity] {
                return cached
            }
            if let persisted = selectedDeviceStore.displayName(for: identity) {
                rideMapVehicleNameCache[identity] = persisted
                return persisted
            }
        }
        if let identity = rideMapVehicleIdentity,
           let candidate = core.protocolIdentityCandidate,
           candidate.platformIdentifier == identity,
           candidate.displayName != identity,
           !candidate.displayName.isEmpty {
            return candidate.displayName
        }
        guard let identity = rideMapVehicleIdentity else {
            return connectionState.selection?.title
        }
        return Self.meaningfulDeviceName(connectionState.selection?.title, identity: identity)
            ?? Self.meaningfulDeviceName(
                devicePickerScanState?.rows.first(where: { $0.id == identity })?.title,
                identity: identity
            )
    }

    func rideMapVehicleName(for identity: String?) -> String? {
        guard let identity else { return nil }
        if let name = rideMapHistoryVehicleNames[identity] {
            return name
        }
        if let name = rideMapVehicleNameCache[identity] {
            return name
        }
        if let name = selectedDeviceStore.displayName(for: identity) {
            rideMapVehicleNameCache[identity] = name
            return name
        }
        return identity == rideMapVehicleIdentity ? rideMapVehicleName : nil
    }

    static func meaningfulDeviceName(_ candidate: String?, identity: String) -> String? {
        guard let candidate,
              !candidate.isEmpty,
              candidate != identity
        else {
            return nil
        }
        return candidate
    }

    var selectedConnectionRoute: DevicePickerConnectionRoute? {
        connectionState.selection?.route
    }

    var speed: SpeedReadout {
        displayState.speed
    }

    var currentMonotonicTime: MonotonicMilliseconds {
        core.now()
    }

    var isRideMapRecording: Bool {
        rideMapSnapshot?.state == .active
    }

    var isRideMapPaused: Bool {
        rideMapSnapshot?.state == .paused
    }

    var rideState: EucRideScreenState {
        EucRideScreenState(phase: phase, displayState: displayState)
    }

    var eucRidePresentationState: EucRideScreenState? {
        guard selectedRideTitle != nil || phase != .starting || displayState.notificationCount != 0 else {
            return nil
        }
        return rideState
    }

    var vescRideSnapshot: VescRideSnapshot? {
        VescRideSnapshot(displayState: displayState, title: selectedRideTitle)
    }

    var captureStatusText: String? {
        captureStatus?.displayText
    }

    var connectionStatusText: String {
        connectionState.statusText ?? phase.displayText
    }

    private let core: any CutoutSessionDriving
    private let liveActivityCoordinator: LiveActivityRideLifecycleCoordinator
    private let selectedDeviceStore: DevicePickerSelectionStore
    private let rideSessionMarkerStore: RideSessionMarkerStore
    private let musicPlayerVisibilityStore: MusicPlayerVisibilityStore
    private let musicHistoryPolicyStore: MusicHistoryPolicyStore
    private let musicCoordinator: MusicIntegrationCoordinator
    private let spotifyMusicProvider = SpotifyProviderAdapter()
#if canImport(MediaPlayer) && os(iOS)
    private let appleMusicProvider = AppleMusicProviderAdapter()
#endif
    private var liveActivityIdentity: LiveActivityRideIdentity?
    private var liveActivityGlyph = LiveActivityRideGlyph.electricUnicycle
    private var lastLiveActivitySnapshot: LiveActivityRideSnapshot?
    private var lastLiveActivityUpdate: MonotonicMilliseconds?
    private var liveActivityRequestID: UInt64 = 0
    private var captureFileName: String?
    private var captureNotificationCount = 0
    private var captureLabel: String?
    private var hasStarted = false
    private var permitsStoredDeviceAutoPairing = true
    private var rideSessionRestorationState = RideSessionRestorationState.complete
    private var restorationMarkerAtLaunch: Data?
    private var rideMapHistoryCursor: MobileRideCursorDto?
    private var rideMapHistoryQueryDateAfterMilliseconds: UInt64?
    private var rideMapVehicleNameCache = [String: String]()
    private var rideMapHistoryLoadTask: Task<Void, Never>?
    private var rideMapHistoryPageTask: Task<Void, Never>?
    private var rideMapHistorySelectionTask: Task<Void, Never>?
    private var rideMapHistorySelectionCancellation: MobileRideMapProjectionCancellation?
    private var rideMapHistoryViewportTask: Task<Void, Never>?
    private var rideMapHistoryViewportCancellation: MobileRideMapProjectionCancellation?
    private var rideMapHistoryContextTask: Task<Void, Never>?
    private var rideMapRestoreTask: Task<Void, Never>?
    private var musicMonitorTask: Task<Void, Never>?
    private var musicMonitorGeneration = MusicMonitorGeneration()
    private var musicMonitorSceneState = MusicMonitorSceneState()
    private var musicTransitionHintTracker = MusicTransitionHintTracker()
    private var rideMapLiveProjectionTask: Task<Void, Never>?
    private var rideMapDurationTask: Task<Void, Never>?
    private var rideMapLiveProjectionCancellation: MobileLiveRideMapProjectionCancellation?
    private var rideMapLiveProjectionGeneration: UInt64 = 0
    private var rideMapLiveProjectionEnabled = false
    private static let liveActivityUpdateIntervalMilliseconds: UInt64 = 1_000

    isolated deinit {
        stopMusicMonitoring()
    }

    convenience init() {
        #if DEBUG
        let permitsStoredDeviceAutoPairing = Self.uiTestFixture == nil
        #else
        let permitsStoredDeviceAutoPairing = true
        #endif
        self.init(
            core: Self.makeSessionDriver(),
            permitsStoredDeviceAutoPairing: permitsStoredDeviceAutoPairing,
            selectedDeviceStore: DevicePickerSelectionStore(),
            rideSessionMarkerStore: RideSessionMarkerStore(),
            liveActivityManager: LiveActivityRideActivityKitManager(),
            musicHistoryPolicyStore: MusicHistoryPolicyStore()
        )
    }

    convenience init(
        core: any CutoutSessionDriving,
        selectedDeviceStore: DevicePickerSelectionStore = DevicePickerSelectionStore(),
        rideSessionMarkerStore: RideSessionMarkerStore = RideSessionMarkerStore(),
        liveActivityManager: any LiveActivityRideLifecycleManaging = LiveActivityRideActivityKitManager(),
        musicHistoryPolicyStore: MusicHistoryPolicyStore = MusicHistoryPolicyStore()
    ) {
        self.init(
            core: core,
            permitsStoredDeviceAutoPairing: true,
            selectedDeviceStore: selectedDeviceStore,
            rideSessionMarkerStore: rideSessionMarkerStore,
            liveActivityManager: liveActivityManager,
            musicHistoryPolicyStore: musicHistoryPolicyStore
        )
    }

    private init(
        core: any CutoutSessionDriving,
        permitsStoredDeviceAutoPairing: Bool,
        selectedDeviceStore: DevicePickerSelectionStore,
        rideSessionMarkerStore: RideSessionMarkerStore,
        liveActivityManager: any LiveActivityRideLifecycleManaging,
        musicHistoryPolicyStore: MusicHistoryPolicyStore
    ) {
        self.permitsStoredDeviceAutoPairing = permitsStoredDeviceAutoPairing
        self.core = core
        rideMapStorageError = core.rideMapStorageError
        rideMapAvailability = core.rideMapAvailability
        liveActivityCoordinator = LiveActivityRideLifecycleCoordinator(
            manager: liveActivityManager,
            sessionState: core.rideSessionStateHandle,
            markerStore: rideSessionMarkerStore
        )
        self.selectedDeviceStore = selectedDeviceStore
        self.rideSessionMarkerStore = rideSessionMarkerStore
        self.musicPlayerVisibilityStore = MusicPlayerVisibilityStore()
        self.isMusicPlayerHidden = musicPlayerVisibilityStore.isHidden
        self.musicHistoryPolicyStore = musicHistoryPolicyStore
        self.musicHistoryPolicy = musicHistoryPolicyStore.policy
        self.musicCoordinator = MusicIntegrationCoordinator(rideMapState: core.rideMapStateHandle)
        self.musicTimelineEvents = musicCoordinator.recordedEvents
        hasSavedDevice = selectedDeviceStore.platformIdentifier != nil
        if let identity = selectedDeviceStore.platformIdentifier,
           let name = selectedDeviceStore.displayName(for: identity)
        {
            rideMapVehicleNameCache[identity] = name
        }
        restoreRideMapState()
        self.core.onDisplayStateChange = { [weak self] displayState in
            self?.displayState = displayState
            self?.syncLiveActivity()
        }
        self.core.onPhaseChange = { [weak self] phase in
            self?.handlePhaseChange(phase)
            self?.syncLiveActivity()
        }
        self.core.onReconnectScheduled = { [weak self] retry in
            self?.handleReconnectScheduled(retry)
        }
        self.core.onScanStateChange = { [weak self] scanState in
            self?.handleScanStateChange(scanState)
        }
        self.core.onSettingsReadbackChange = { [weak self] settingsReadback in
            self?.settingsReadback = settingsReadback
        }
        self.core.onFaultHistoryReadbackChange = { [weak self] faultHistoryReadback in
            self?.faultHistoryReadback = faultHistoryReadback
        }
        self.core.onBmsSnapshotChange = { [weak self] bmsSnapshot in
            self?.bmsSnapshot = bmsSnapshot
        }
        self.core.onPhoneLocationSnapshotChange = { [weak self] snapshot, receivedAt in
            self?.phoneLocationReadback = PhoneLocationReadback(snapshot: snapshot, receivedAt: receivedAt)
        }
        self.core.onProtocolIdentityCandidateChange = { [weak self] candidate in
            self?.applyProtocolIdentityCandidate(candidate)
        }
        self.core.onBluetoothRestorationResolved = { [weak self] platformIdentifier in
            self?.handleBluetoothRestorationResolved(platformIdentifier)
        }
        self.core.onCaptureEvent = { [weak self] event in
            self?.applyCaptureEvent(event)
        }
    }

    @discardableResult
    func handleMusicCommand(_ command: MobileMusicCommandDto) async -> MusicCommandOutcome {
        guard let nowPlaying = musicNowPlaying else { return .unavailable }
        guard nowPlaying.isCommandAvailable(command) else { return .refused }
#if canImport(MediaPlayer) && os(iOS)
        let outcome: MusicCommandOutcome
        if nowPlaying.provider == .spotify {
            outcome = await spotifyMusicProvider.perform(command)
        } else {
            outcome = await appleMusicProvider.perform(command)
        }
        if outcome == .accepted {
            switch command {
            case .previous, .next:
                musicTransitionHintTracker.issue(.skip)
            default:
                break
            }
            refreshMusicSnapshot()
        }
        return outcome
#else
        return .unavailable
#endif
    }

    func dismissMusicPlayer() {
        musicPlayerVisibilityStore.setHidden(true)
        isMusicPlayerHidden = true
        musicNowPlaying = nil
        musicTransitionHintTracker.clear()
    }

    func restoreMusicPlayer() {
        musicPlayerVisibilityStore.setHidden(false)
        isMusicPlayerHidden = false
        musicNowPlaying = projectedMusicNowPlaying()
    }

    func selectMusicProvider(_ provider: MobileMusicProviderDto) {
        let previousProvider = selectedMusicProvider
        selectedMusicProvider = provider
        musicTransitionHintTracker.clear()
        updateMusicMonitoring(from: previousProvider, to: provider)
        if !isMusicPlayerHidden {
            refreshMusicSnapshot()
        }
    }

    private func updateMusicMonitoring(
        from previousProvider: MobileMusicProviderDto,
        to provider: MobileMusicProviderDto
    ) {
        switch provider.monitoringMode {
        case .unavailable:
            musicMonitorSceneState.cancel()
            stopMusicMonitoring()
        case .appleMusicSystemPlayer where previousProvider != provider:
            musicMonitorSceneState.request()
            beginMusicMonitoring()
        case .appleMusicSystemPlayer:
            break
        }
    }

    func refreshMusicSnapshot(
        transitionHint: MusicTransitionHint? = nil
    ) {
#if canImport(MediaPlayer) && os(iOS)
        let observedAtMs = core.now().rawValue
        let observation: MusicProviderObservation
        switch selectedMusicProvider.monitoringMode {
        case .appleMusicSystemPlayer:
            observation = appleMusicProvider.observation(observedAtMs: observedAtMs)
        case .unavailable:
            observation = MusicProviderObservation(
                snapshot: spotifyMusicProvider.unavailableSnapshot(observedAtMs: observedAtMs)
            )
        }
        _ = ingestMusicObservation(
            observation,
            transitionHint: transitionHint ?? musicTransitionHintTracker.hint
        )
#endif
    }

    @discardableResult
    func ingestMusicObservation(
        _ observation: MusicProviderObservation,
        wallClockAtMs: UInt64? = nil,
        clockUncertaintyMs: UInt64 = 1_000,
        transitionHint: MusicTransitionHint? = nil
    ) -> Bool {
        let wallClockAtMs = wallClockAtMs ?? UInt64(Date().timeIntervalSince1970 * 1_000)
        let previousNowPlaying = musicCoordinator.nowPlaying
        do {
            let outcome = try musicCoordinator.ingest(
                observation: observation,
                wallClockAtMs: wallClockAtMs,
                clockUncertaintyMs: clockUncertaintyMs,
                transitionHint: transitionHint
            )
            if outcome == .recorded {
                core.updateMusicCaptureObservation(
                    pevcapMusicObservation(
                        from: observation,
                        wallClockAtMs: wallClockAtMs,
                        clockUncertaintyMs: clockUncertaintyMs,
                        rideSequence: musicCoordinator.recordedEvents.last?.sequence
                    )
                )
            } else if outcome == .disabled {
                core.updateMusicCaptureObservation(nil)
            }
            finishMusicObservation(
                previousNowPlaying: previousNowPlaying,
                appliedHint: transitionHint
            )
            return true
        } catch {
            finishMusicObservation(
                previousNowPlaying: previousNowPlaying,
                appliedHint: transitionHint
            )
            return false
        }
    }

    private func finishMusicObservation(
        previousNowPlaying: MusicNowPlaying?,
        appliedHint: MusicTransitionHint?
    ) {
        musicTransitionHintTracker.resolve(
            previous: previousNowPlaying,
            current: musicCoordinator.nowPlaying,
            appliedHint: appliedHint
        )
        musicTimelineEvents = musicCoordinator.recordedEvents
        musicNowPlaying = projectedMusicNowPlaying()
    }

    private func projectedMusicNowPlaying() -> MusicNowPlaying? {
        guard !isMusicPlayerHidden, let current = musicCoordinator.nowPlaying else {
            return nil
        }
        guard current.provider != selectedMusicProvider else { return current }
        return MusicNowPlaying(
            observation: unavailableMusicObservation(observedAtMs: core.now().rawValue)
        )
    }

    private func pevcapMusicObservation(
        from observation: MusicProviderObservation,
        wallClockAtMs: UInt64,
        clockUncertaintyMs: UInt64,
        rideSequence: UInt64?
    ) -> MobilePevcapMusicEventDto? {
        guard musicHistoryPolicy != .disabled,
              let item = observation.snapshot.item
        else {
            return nil
        }
        return MobilePevcapMusicEventDto(
            provider: observation.snapshot.provider,
            trackId: item.identifier,
            monotonicAtMs: observation.snapshot.observedAtMs,
            wallClockUnixMs: wallClockAtMs,
            clockUncertaintyMs: clockUncertaintyMs,
            rideSequence: rideSequence
        )
    }

    func setMusicHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) -> Bool {
        let previous = musicHistoryPolicy
        do {
            try musicCoordinator.setHistoryPolicy(policy)
            rememberMusicHistoryPolicy(policy)
            if policy == .disabled {
                core.updateMusicCaptureObservation(nil)
            }
            musicTimelineEvents = musicCoordinator.recordedEvents
            return true
        } catch MobileRideMapError.noActiveRide {
            // Keep the choice as the default for the next ride.
            rememberMusicHistoryPolicy(policy)
            if policy == .disabled {
                core.updateMusicCaptureObservation(nil)
            }
            return true
        } catch {
            musicHistoryPolicy = previous
            return false
        }
    }

    private func rememberMusicHistoryPolicy(_ policy: MobileMusicHistoryPolicyDto) {
        musicHistoryPolicyStore.set(policy)
        musicHistoryPolicy = policy
    }

    private func monitorMusic(generation: UInt64) async {
#if canImport(MediaPlayer) && os(iOS)
        guard musicMonitorGeneration.owns(generation) else { return }
        defer { finishMusicMonitoring(generation: generation) }
        guard selectedMusicProvider.monitoringMode == .appleMusicSystemPlayer else {
            refreshMusicSnapshot()
            return
        }
        guard await appleMusicProvider.requestAuthorization() else {
            guard !Task.isCancelled, musicMonitorGeneration.owns(generation) else { return }
            _ = ingestMusicObservation(MusicProviderObservation(
                snapshot: appleMusicProvider.unauthorizedSnapshot(observedAtMs: core.now().rawValue)
            ))
            return
        }
        guard !Task.isCancelled,
              musicMonitorGeneration.owns(generation),
              selectedMusicProvider.monitoringMode == .appleMusicSystemPlayer
        else { return }
        appleMusicProvider.startMonitoring { [weak self] in
            self?.refreshMusicSnapshot()
        }
        while !Task.isCancelled && musicMonitorGeneration.owns(generation) {
            refreshMusicSnapshot()
            do {
                try await Task.sleep(for: .seconds(1))
            } catch {
                return
            }
        }
#endif
    }

    private func finishMusicMonitoring(generation: UInt64) {
        guard musicMonitorGeneration.owns(generation) else { return }
        stopMusicMonitoring()
    }

    private func unavailableMusicObservation(observedAtMs: UInt64) -> MusicProviderObservation {
        MusicProviderObservation.unavailable(
            provider: selectedMusicProvider,
            sessionId: "music-unavailable",
            observedAtMs: observedAtMs
        )
    }

    private func stopMusicMonitoring() {
        musicMonitorGeneration.invalidate()
        musicMonitorTask?.cancel()
        musicMonitorTask = nil
#if canImport(MediaPlayer) && os(iOS)
        appleMusicProvider.stopMonitoring()
#endif
    }

    private func beginMusicMonitoring() {
        guard musicMonitorSceneState.isSceneActive else { return }
#if os(iOS)
        stopMusicMonitoring()
        let generation = musicMonitorGeneration.begin()
        musicMonitorTask = Task { [weak self] in
            await self?.monitorMusic(generation: generation)
        }
#else
        _ = ingestMusicObservation(unavailableMusicObservation(observedAtMs: core.now().rawValue))
#endif
    }

    func connectMusic() {
        musicMonitorSceneState.request()
        beginMusicMonitoring()
    }

    private func restoreRideMapState() {
        guard let state = core.rideMapStateHandle else { return }
        rideMapSnapshot = state.currentSnapshot()
        if rideMapSnapshot != nil {
            musicHistoryPolicy = state.currentMusicHistoryPolicy()
        }
        musicCoordinator.restoreHistoryPolicy(musicHistoryPolicy)
        musicTimelineEvents = musicCoordinator.recordedEvents
        rideMapLiveTelemetryState = rideMapSnapshot?.associatedVehicle == nil
            ? .gpsOnly
            : .associatedNoTelemetry
        updateRideMapDurationTicker()
        guard rideMapSnapshot != nil else { return }
        rideMapRestoreTask?.cancel()
        let previewLimit = Self.rideMapLimits.liveTailPointLimit
        let restorationGeneration = rideMapLiveProjectionGeneration
        rideMapRestoreTask = Task { [weak self] in
            do {
                let result = try await Self.runCancellableDetached(priority: .userInitiated) {
                    try state.projectPoints(budget: previewLimit)
                }
                guard !Task.isCancelled, let self else { return }
                guard Self.shouldApplyRestoredLiveProjection(
                    restorationGeneration: restorationGeneration,
                    currentGeneration: self.rideMapLiveProjectionGeneration,
                    liveProjectionEnabled: self.rideMapLiveProjectionEnabled
                ) else {
                    return
                }
                self.applyLiveProjection(result)
            } catch {
                guard !Task.isCancelled, let self else { return }
                guard Self.shouldApplyRestoredLiveProjection(
                    restorationGeneration: restorationGeneration,
                    currentGeneration: self.rideMapLiveProjectionGeneration,
                    liveProjectionEnabled: self.rideMapLiveProjectionEnabled
                ) else {
                    return
                }
                self.rideMapLiveError = Self.mapRideMapError(error)
                self.clearLiveProjectionState()
            }
        }
    }

    func start() {
        guard hasStarted == false else { return }
        hasStarted = true
        permitsStoredDeviceAutoPairing = false
        restorationMarkerAtLaunch = rideSessionMarkerStore.marker
        rideSessionRestorationState = .awaitingBluetooth
        core.start()
    }

    @discardableResult
    func startGpsOnlyRide() -> Bool {
        let nextRideMusicPolicy = musicHistoryPolicyStore.policy
        core.resetRideMapLocationAdmission()
        let started = applyRideMapCommand(resetPoints: true) {
            try core.startRideMapGpsOnly(
                atMs: currentMonotonicTime.rawValue,
                lastConnectedVehicle: selectedDeviceStore.platformIdentifier
            )
        }
        guard started else { return false }
        // Apply the user's default to the fresh Rust-owned ride timeline.
        core.updateMusicCaptureObservation(nil)
        do {
            try musicCoordinator.setHistoryPolicy(nextRideMusicPolicy)
            musicHistoryPolicy = nextRideMusicPolicy
        } catch {
            // A fresh ride starts disabled; keep the Swift projection aligned if
            // durable policy application is unavailable.
            musicHistoryPolicy = core.rideMapStateHandle?.currentMusicHistoryPolicy() ?? .disabled
            musicCoordinator.restoreHistoryPolicy(musicHistoryPolicy)
        }
        musicTimelineEvents = musicCoordinator.recordedEvents
        return true
    }

    @discardableResult
    func pauseRideMap() -> Bool {
        applyRideMapCommand {
            try core.pauseRideMap(atMs: currentMonotonicTime.rawValue)
        }
    }

    @discardableResult
    func resumeRideMap() -> Bool {
        applyRideMapCommand {
            try core.resumeRideMap(atMs: currentMonotonicTime.rawValue)
        }
    }

    @discardableResult
    func stopRideMap() -> Bool {
        let stopped = applyRideMapCommand {
            try core.stopRideMap(atMs: currentMonotonicTime.rawValue)
        }
        if stopped {
            invalidateLiveProjection(clearPoints: false)
        }
        return stopped
    }

    func refreshRideMapDuration() {
        guard let snapshot = core.rideMapStateHandle?.currentSnapshot(atMs: currentMonotonicTime.rawValue),
              snapshot.state == .active
        else {
            return
        }
        rideMapSnapshot = snapshot
    }

    private func updateRideMapDurationTicker() {
        rideMapDurationTask?.cancel()
        guard rideMapSnapshot?.state == .active else {
            rideMapDurationTask = nil
            return
        }
        rideMapDurationTask = Task { [weak self] in
            while Task.isCancelled == false {
                self?.refreshRideMapDuration()
                do {
                    try await Task.sleep(for: .seconds(1))
                } catch {
                    return
                }
            }
        }
    }

    @discardableResult
    func saveRideMap() -> Bool {
        guard applyRideMapCommand({ try core.saveRideMap() }) else {
            return false
        }
        invalidateLiveProjection(clearPoints: false)
        core.updateMusicCaptureObservation(nil)
        musicTimelineEvents = musicCoordinator.recordedEvents
        loadRideMapHistory()
        return true
    }

    @discardableResult
    func discardRideMap() -> Bool {
        guard applyRideMapCommand({ try core.discardRideMap() }) else {
            return false
        }
        invalidateLiveProjection(clearPoints: true)
        core.updateMusicCaptureObservation(nil)
        musicTimelineEvents = musicCoordinator.recordedEvents
        clearRideMapHistoryRouteProjection()
        rideMapHistoryRouteLoading = false
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryDetailRouteLoading = false
        loadRideMapHistory()
        return true
    }

    func loadRideMapHistory(selecting requestedRideID: String? = nil) {
        rideMapHistoryLoadTask?.cancel()
        rideMapHistoryPageTask?.cancel()
        // A reload changes the query that owns the selected route. Do not let an
        // older route request repopulate points after the new page arrives.
        rideMapHistorySelectionTask?.cancel()
        rideMapHistorySelectionCancellation?.cancel()
        rideMapHistoryViewportCancellation?.cancel()
        rideMapHistoryViewportTask?.cancel()
        rideMapHistoryContextTask?.cancel()
        rideMapHistoryContextTask = nil
        rideMapHistoryContextProjection = nil
        rideMapHistoryContextRoutes.removeAll(keepingCapacity: true)
        rideMapHistoryError = nil
        rideMapHistoryRouteError = nil
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryLoading = true
        rideMapHistoryRouteLoading = false
        rideMapHistoryDetailRouteLoading = false
        rideMapHistoryDetailMusicTimeline.removeAll(keepingCapacity: true)
        rideMapHistoryQueryDateAfterMilliseconds = historyDateAfterMilliseconds
        if let rideMapStorageError {
            rideMapHistoryLoading = false
            rideMapHistoryError = .storageError(rideMapStorageError)
            return
        }
        guard let state = core.rideMapStateHandle else {
            rideMapHistoryLoading = false
            rideMapHistoryError = .storageError("Rust ride database is unavailable")
            return
        }
        let filter = rideMapHistoryFilter
        let existingSelectedID = selectedRideMapHistoryID
        rideMapHistoryLoadTask = Task { [weak self] in
            do {
                let result = try await Self.runCancellableDetached(priority: .userInitiated) {
                    let page = try state.storedHistoryPage(
                        cursor: nil,
                        limit: Self.rideMapLimits.historyPageLimit,
                        filter: filter
                    )
                    let vehicleOptions = try state.storedHistoryVehicleOptions()
                    var summaries = page.summaries
                    if let requestedRideID,
                       summaries.contains(where: { $0.rideID == requestedRideID }) == false,
                       let requestedRide = try state.storedHistoryRide(rideID: requestedRideID)
                    {
                        let insertionIndex = summaries.firstIndex {
                            $0.createdAtMilliseconds < requestedRide.createdAtMilliseconds
                        } ?? summaries.endIndex
                        summaries.insert(requestedRide, at: insertionIndex)
                    }
                    return (summaries, page.nextCursor, vehicleOptions)
                }
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryLoadTask = nil
                self.rideMapHistoryLoading = false
                self.rideMapHistory = result.0
                self.rideMapHistoryVehicleIdentities = Self.mergeRideMapHistoryVehicleIdentities(
                    existing: result.2.map(\.platformIdentifier),
                    incoming: result.0.flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }
                )
                self.rideMapHistoryVehicleNames = Self.historyVehicleNames(
                    result.2,
                    summaries: result.0
                )
                self.rideMapVehicleNameCache.merge(
                    self.rideMapHistoryVehicleNames,
                    uniquingKeysWith: { _, incoming in incoming }
                )
                self.rideMapHistoryCursor = result.1
                self.rideMapHistoryCanLoadMore = result.1 != nil
                self.rideMapHistoryError = nil
                let selectionError = Self.historySelectionError(
                    requestedID: requestedRideID,
                    summaryIDs: result.0.map(\.rideID)
                )
                let selectedID = Self.preferredHistorySelection(
                    requestedID: requestedRideID,
                    currentID: existingSelectedID,
                    summaries: result.0
                )
                guard let selectedID else {
                    self.selectedRideMapHistoryID = nil
                    self.clearRideMapHistoryRouteProjection()
                    self.rideMapHistoryRouteLoading = false
                    self.rideMapHistoryRouteError = selectionError
                    self.rideMapHistoryDetailRouteError = selectionError
                    self.rideMapHistoryDetailRouteLoading = false
                    return
                }
                self.selectRideMapHistory(selectedID)
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryLoadTask = nil
                self.rideMapHistoryLoading = false
                // Preserve the last good page so a transient storage failure does not
                // turn an otherwise usable history screen into an empty state.
                self.rideMapHistoryError = Self.mapRideMapError(error)
                self.rideMapHistoryRouteLoading = false
                self.rideMapHistoryDetailRouteLoading = false
            }
        }
    }

    @MainActor
    static func preferredHistorySelection(
        requestedID: String?,
        currentID: String?,
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> String? {
        preferredHistorySelection(
            requestedID: requestedID,
            currentID: currentID,
            summaryIDs: summaries.map(\.rideID)
        )
    }

    @MainActor
    static func preferredHistorySelection(
        requestedID: String?,
        currentID: String?,
        summaryIDs: [String]
    ) -> String? {
        if let requestedID {
            return summaryIDs.first(where: { $0 == requestedID })
        }
        return summaryIDs.first(where: { $0 == currentID }) ?? summaryIDs.first
    }

    @MainActor
    static func historySelectionError(
        requestedID: String?,
        summaryIDs: [String]
    ) -> MobileRideMapError? {
        guard let requestedID, summaryIDs.contains(requestedID) == false else { return nil }
        return .rideNotFound
    }

    @MainActor
    static func appendingUniqueHistory<T>(
        existing: [T],
        incoming: [T],
        id: (T) -> String
    ) -> [T] {
        var seen = Set(existing.map(id))
        return existing + incoming.filter { seen.insert(id($0)).inserted }
    }

    @MainActor
    static func mergeRideMapHistoryVehicleIdentities(
        existing: [String],
        incoming: [String]
    ) -> [String] {
        Array(Set(existing + incoming)).sorted()
    }

    @MainActor
    static func historyVehicleNames(
        _ options: [MobileRideMapHistoryVehicleOptionDto],
        summaries: [MobileRideMapHistorySummaryDto]
    ) -> [String: String] {
        var names = Dictionary(
            options.compactMap { option in
                option.displayName.map { (option.platformIdentifier, $0) }
            },
            uniquingKeysWith: { first, _ in first }
        )
        for summary in summaries {
            if let identity = summary.associatedVehicle,
               let name = summary.associatedVehicleName
            {
                names[identity] = name
            }
            if let identity = summary.candidateVehicle,
               let name = summary.candidateVehicleName
            {
                names[identity] = name
            }
        }
        return names
    }

    @MainActor
    static func mergeRideMapHistoryVehicleNames(
        existing: [String: String],
        incoming: [MobileRideMapHistorySummaryDto]
    ) -> [String: String] {
        var names = existing
        for summary in incoming {
            if let identity = summary.associatedVehicle,
               let name = summary.associatedVehicleName
            {
                names[identity] = name
            }
            if let identity = summary.candidateVehicle,
               let name = summary.candidateVehicleName
            {
                names[identity] = name
            }
        }
        return names
    }

    func loadMoreRideMapHistory() {
        guard rideMapHistoryCanLoadMore,
              rideMapHistoryLoadTask == nil,
              rideMapHistoryLoading == false
        else { return }
        rideMapHistoryPageTask?.cancel()
        guard let state = core.rideMapStateHandle else { return }
        let cursor = rideMapHistoryCursor
        let filter = rideMapHistoryFilter
        rideMapHistoryPageTask = Task { [weak self] in
            do {
                let page = try await Self.runCancellableDetached(priority: .userInitiated) {
                    try state.storedHistoryPage(
                        cursor: cursor,
                        limit: Self.rideMapLimits.historyPageLimit,
                        filter: filter
                    )
                }
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistory = Self.appendingUniqueHistory(
                    existing: self.rideMapHistory,
                    incoming: page.summaries,
                    id: \.rideID
                )
                self.rideMapHistoryVehicleIdentities = Self.mergeRideMapHistoryVehicleIdentities(
                    existing: self.rideMapHistoryVehicleIdentities,
                    incoming: page.summaries.flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }
                )
                self.rideMapHistoryVehicleNames = Self.mergeRideMapHistoryVehicleNames(
                    existing: self.rideMapHistoryVehicleNames,
                    incoming: page.summaries
                )
                self.rideMapVehicleNameCache.merge(
                    self.rideMapHistoryVehicleNames,
                    uniquingKeysWith: { _, incoming in incoming }
                )
                self.rideMapHistoryCursor = page.nextCursor
                self.rideMapHistoryCanLoadMore = page.nextCursor != nil
                self.rideMapHistoryError = nil
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryError = Self.mapRideMapError(error)
            }
        }
    }

    var filteredRideMapHistory: [MobileRideMapHistorySummaryDto] {
        rideMapHistory
    }

    func setRideMapHistoryDateFilter(_ filter: RideMapHistoryDateFilter) {
        guard rideMapHistoryDateFilter != filter else { return }
        rideMapHistoryDateFilter = filter
        loadRideMapHistory()
    }

    func setRideMapHistoryVehicleFilter(_ identity: String?) {
        guard rideMapHistoryVehicleFilter != identity else { return }
        rideMapHistoryVehicleFilter = identity
        loadRideMapHistory()
    }

    func setRideMapHistorySearchText(_ text: String) {
        guard rideMapHistorySearchText != text else { return }
        rideMapHistorySearchText = text
        loadRideMapHistory()
    }

    func clearRideMapHistoryFilters() {
        rideMapHistorySearchText = ""
        rideMapHistoryDateFilter = .last30Days
        rideMapHistoryVehicleFilter = nil
        loadRideMapHistory()
    }

    private var historyDateAfterMilliseconds: UInt64? {
        guard rideMapHistoryDateFilter == .last30Days else { return nil }
        let now = Date().timeIntervalSince1970 * 1_000
        guard now.isFinite, now > 0 else { return 0 }
        let window = Double(Self.rideMapLimits.historyRecentWindowMilliseconds)
        return UInt64(max(0, now - window))
    }

    private var rideMapHistoryFilter: MobileRideHistoryFilterDto {
        MobileRideHistoryFilterDto(
            createdAfterMilliseconds: rideMapHistoryQueryDateAfterMilliseconds ?? historyDateAfterMilliseconds,
            vehicleIdentity: rideMapHistoryVehicleFilter,
            searchText: normalizedRideMapHistorySearchText(rideMapHistorySearchText)
        )
    }

    func selectRideMapHistory(_ rideID: String) {
        selectRideMapHistory(rideID, requestedPointLimit: Int(Self.rideMapLimits.liveTailPointLimit))
    }

    /// Removes only the selected ride's persisted music metadata.
    @discardableResult
    func forgetMusicHistory(for rideID: String) -> Bool {
        guard let state = core.rideMapStateHandle else {
            rideMapHistoryError = .storageError("Rust ride database is unavailable")
            return false
        }
        do {
            if rideMapSnapshot?.rideID == rideID,
               rideMapSnapshot?.state.isOpen == true
            {
                try clearActiveMusicHistory(using: state)
            } else {
                try state.deleteMusicHistory(rideID: rideID)
                if rideMapSnapshot?.rideID == rideID {
                    musicHistoryPolicy = .disabled
                    musicCoordinator.restoreHistoryPolicy(.disabled)
                    musicTransitionHintTracker.clear()
                    core.updateMusicCaptureObservation(nil)
                    musicTimelineEvents = musicCoordinator.recordedEvents
                }
            }
            if selectedRideMapHistoryID == rideID {
                rideMapHistoryDetailMusicTimeline.removeAll(keepingCapacity: true)
            }
            return true
        } catch {
            rideMapHistoryError = Self.mapRideMapError(error)
            return false
        }
    }

    private func clearActiveMusicHistory(using state: MobileRideMapState) throws {
        // Route active-ride deletion through the Rust state owner so its
        // in-memory timeline and durable policy change together.
        try state.setMusicHistoryPolicy(.disabled)
        musicHistoryPolicy = .disabled
        musicCoordinator.restoreHistoryPolicy(.disabled)
        musicTransitionHintTracker.clear()
        core.updateMusicCaptureObservation(nil)
        musicTimelineEvents = musicCoordinator.recordedEvents
    }

    static func detailPointsAreTruncated(
        sourcePointsOmittedByBudget: Bool,
        viewportPointsOmittedByBudget: Bool
    ) -> Bool {
        sourcePointsOmittedByBudget || viewportPointsOmittedByBudget
    }

    static func detailSegmentsAreOmitted(
        sourceSegmentsOmittedByBudget: Bool,
        viewportSegmentsOmittedByBudget: Bool
    ) -> Bool {
        sourceSegmentsOmittedByBudget || viewportSegmentsOmittedByBudget
    }

    func projectRideMapHistoryDetailViewport(_ viewport: MobileGeoBoundsDto?) {
        guard let viewport,
              let selectedRideMapHistoryID,
              rideMapHistory.contains(where: { $0.rideID == selectedRideMapHistoryID })
        else {
            return
        }
        rideMapHistoryViewportCancellation?.cancel()
        rideMapHistoryViewportTask?.cancel()
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryDetailRouteLoading = true
        let cancellation = MobileRideMapProjectionCancellation()
        rideMapHistoryViewportCancellation = cancellation
        guard let state = core.rideMapStateHandle else {
            rideMapHistoryDetailRouteLoading = false
            rideMapHistoryDetailRouteError = .storageError("Rust ride database is unavailable")
            return
        }
        let budget = Self.rideMapLimits.historyPreviewPointLimit
        rideMapHistoryViewportTask = Task { [weak self] in
            do {
                let result = try await withTaskCancellationHandler(operation: {
                    try await Self.runCancellableDetached(priority: .userInitiated) {
                        try state.projectStoredPoints(
                            rideID: selectedRideMapHistoryID,
                            budget: budget,
                            viewport: viewport,
                            cancellation: cancellation
                        )
                    }
                }, onCancel: {
                    cancellation.cancel()
                })
                guard !Task.isCancelled, let self else { return }
                self.replaceRideMapHistoryDetailDisplayPoints(
                    result.points,
                    cameraRegion: result.cameraRegion,
                    endpointMetadata: result.endpointMetadata,
                    segments: result.segments,
                    backgroundGapCount: result.backgroundGapCount,
                    truncated: Self.detailPointsAreTruncated(
                        sourcePointsOmittedByBudget: self.rideMapHistoryDetailSourcePointsOmittedByBudget,
                        viewportPointsOmittedByBudget: result.pointsOmittedByBudget
                    ),
                    segmentsOmittedByBudget: Self.detailSegmentsAreOmitted(
                        sourceSegmentsOmittedByBudget: self.rideMapHistoryDetailSourceSegmentsOmittedByBudget,
                        viewportSegmentsOmittedByBudget: result.segmentsOmittedByBudget
                    )
                )
                self.rideMapHistoryDetailRouteError = nil
                self.rideMapHistoryDetailRouteLoading = false
            } catch {
                guard !Task.isCancelled, let self else { return }
                let mappedError = Self.mapRideMapError(error)
                if mappedError == .cancelled {
                    return
                }
                self.rideMapHistoryDetailRouteError = mappedError
                self.rideMapHistoryDetailRouteLoading = false
                self.replaceRideMapHistoryDetailDisplayPoints([], truncated: false)
            }
        }
    }

    /// Loads the largest Rust-bounded route preview, never an unbounded route.
    func loadRoutePreviewMapHistory() {
        guard let selectedRideMapHistoryID else { return }
        selectRideMapHistory(selectedRideMapHistoryID, requestedPointLimit: nil)
    }

    private func selectRideMapHistory(_ rideID: String, requestedPointLimit: Int?) {
        guard rideMapHistory.contains(where: { $0.rideID == rideID }) else {
            rideMapHistoryRouteLoading = false
            rideMapHistoryRouteError = nil
            rideMapHistoryDetailRouteLoading = false
            rideMapHistoryDetailRouteError = nil
            return
        }
        rideMapHistorySelectionTask?.cancel()
        rideMapHistorySelectionCancellation?.cancel()
        rideMapHistoryViewportCancellation?.cancel()
        rideMapHistoryViewportTask?.cancel()
        rideMapHistoryContextTask?.cancel()
        rideMapHistoryContextTask = nil
        rideMapHistoryContextProjection = nil
        rideMapHistoryContextRoutes.removeAll(keepingCapacity: true)
        let selectingDifferentRide = selectedRideMapHistoryID != rideID
        if selectingDifferentRide {
            clearRideMapHistoryRouteProjection()
        }
        selectedRideMapHistoryID = rideID
        rideMapHistoryRouteError = nil
        rideMapHistoryDetailRouteError = nil
        rideMapHistoryRouteLoading = true
        rideMapHistoryDetailRouteLoading = true
        guard let state = core.rideMapStateHandle else {
            rideMapHistoryRouteLoading = false
            rideMapHistoryDetailRouteLoading = false
            rideMapHistoryRouteError = .storageError("Rust ride database is unavailable")
            rideMapHistoryDetailRouteError = rideMapHistoryRouteError
            return
        }
        let cancellation = MobileRideMapProjectionCancellation()
        rideMapHistorySelectionCancellation = cancellation
        let budget = UInt32(
            min(
                requestedPointLimit ?? Int(Self.rideMapLimits.historyPreviewPointLimit),
                Int(Self.rideMapLimits.historyPreviewPointLimit)
            )
        )
        rideMapHistorySelectionTask = Task { [weak self] in
            do {
                let result = try await withTaskCancellationHandler(operation: {
                    try await Self.runCancellableDetached(priority: .userInitiated) {
                        let projection = try state.projectStoredPoints(
                            rideID: rideID,
                            budget: budget,
                            cancellation: cancellation
                        )
                        let musicTimeline = (try? state.storedMusicEvents(rideID: rideID)) ?? []
                        return (projection, musicTimeline)
                    }
                }, onCancel: {
                    cancellation.cancel()
                })
                guard !Task.isCancelled, let self else { return }
                let (projection, musicTimeline) = result
                self.rideMapHistoryRouteError = nil
                self.rideMapHistoryDetailRouteError = nil
                self.replaceRideMapHistoryDisplayPoints(
                    projection.points,
                    cameraRegion: projection.cameraRegion,
                    endpointMetadata: projection.endpointMetadata,
                    segments: projection.segments,
                    backgroundGapCount: projection.backgroundGapCount,
                    truncated: projection.pointsOmittedByBudget,
                    segmentsOmittedByBudget: projection.segmentsOmittedByBudget
                )
                self.rideMapHistoryDetailSourcePointsOmittedByBudget = projection.pointsOmittedByBudget
                self.rideMapHistoryDetailSourceSegmentsOmittedByBudget = projection.segmentsOmittedByBudget
                self.rideMapHistoryDetailMusicTimeline = musicTimeline
                self.replaceRideMapHistoryDetailDisplayPoints(
                    projection.points,
                    cameraRegion: projection.cameraRegion,
                    endpointMetadata: projection.endpointMetadata,
                    segments: projection.segments,
                    backgroundGapCount: projection.backgroundGapCount,
                    truncated: projection.pointsOmittedByBudget,
                    segmentsOmittedByBudget: Self.detailSegmentsAreOmitted(
                        sourceSegmentsOmittedByBudget: self.rideMapHistoryDetailSourceSegmentsOmittedByBudget,
                        viewportSegmentsOmittedByBudget: projection.segmentsOmittedByBudget
                    )
                )
                self.rideMapHistoryRouteLoading = false
                self.rideMapHistoryDetailRouteLoading = false
                self.projectRideMapHistoryContext(for: rideID)
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.rideMapHistoryRouteError = Self.mapRideMapError(error)
                self.rideMapHistoryRouteLoading = false
                self.rideMapHistoryDetailRouteError = self.rideMapHistoryRouteError
                self.rideMapHistoryDetailRouteLoading = false
                self.rideMapHistoryDetailMusicTimeline.removeAll(keepingCapacity: true)
                self.replaceRideMapHistoryDetailDisplayPoints([], truncated: false)
            }
        }
    }

    private static func mapRideMapError(_ error: Error) -> MobileRideMapError {
        if let error = error as? MobileRideMapError {
            return error
        }
        return .storageError(String(describing: error))
    }

    private func replaceRideMapHistoryDisplayPoints(
        _ points: [MobileRideMapRouteDisplayPoint],
        cameraRegion: MobileRideMapCameraRegion? = nil,
        endpointMetadata: MobileRideMapRouteEndpointMetadata = .empty,
        segments: [MobileRideMapSegmentDisplayMetadata] = [],
        backgroundGapCount: UInt64 = 0,
        truncated: Bool,
        segmentsOmittedByBudget: Bool = false
    ) {
        rideMapHistoryDisplayPoints = points
        rideMapHistoryCameraRegion = cameraRegion
        rideMapHistoryEndpointMetadata = endpointMetadata
        rideMapHistorySegments = segments
        rideMapHistoryBackgroundGapCount = backgroundGapCount
        rideMapHistoryPointsTruncated = truncated
        rideMapHistorySegmentsOmittedByBudget = segmentsOmittedByBudget
        rideMapHistoryProjectionVersion &+= 1
    }

    private func replaceRideMapHistoryDetailDisplayPoints(
        _ points: [MobileRideMapRouteDisplayPoint],
        cameraRegion: MobileRideMapCameraRegion? = nil,
        endpointMetadata: MobileRideMapRouteEndpointMetadata = .empty,
        segments: [MobileRideMapSegmentDisplayMetadata] = [],
        backgroundGapCount: UInt64 = 0,
        truncated: Bool,
        segmentsOmittedByBudget: Bool = false
    ) {
        rideMapHistoryDetailDisplayPoints = points
        rideMapHistoryDetailCameraRegion = cameraRegion
        rideMapHistoryDetailEndpointMetadata = endpointMetadata
        rideMapHistoryDetailSegments = segments
        rideMapHistoryDetailBackgroundGapCount = backgroundGapCount
        rideMapHistoryDetailPointsTruncated = truncated
        rideMapHistoryDetailSegmentsOmittedByBudget = segmentsOmittedByBudget
        rideMapHistoryDetailProjectionVersion &+= 1
    }

    private func clearRideMapHistoryRouteProjection() {
        rideMapHistoryContextTask?.cancel()
        rideMapHistoryContextTask = nil
        rideMapHistoryContextProjection = nil
        rideMapHistoryContextRoutes.removeAll(keepingCapacity: true)
        replaceRideMapHistoryDisplayPoints([], truncated: false)
        rideMapHistoryDetailMusicTimeline.removeAll(keepingCapacity: true)
        rideMapHistoryDetailSourcePointsOmittedByBudget = false
        rideMapHistoryDetailSourceSegmentsOmittedByBudget = false
        replaceRideMapHistoryDetailDisplayPoints([], truncated: false)
    }

    /// Loads the bounded surrounding-route context for the selected history ride. Rust performs
    /// history filtering, route projection, privacy, and all point budgeting; Swift stores only
    /// the already-bounded display projections needed by the map canvas.
    private func projectRideMapHistoryContext(for rideID: String) {
        rideMapHistoryContextTask?.cancel()
        rideMapHistoryContextProjection = nil
        rideMapHistoryContextRoutes.removeAll(keepingCapacity: true)
        guard let state = core.rideMapStateHandle else { return }
        let filter = rideMapHistoryFilter
        let budget = MobileRideMapHistoryContextBudget.overview
        rideMapHistoryContextTask = Task { [weak self] in
            do {
                let projection = try await Self.runCancellableDetached(priority: .userInitiated) {
                    try state.projectStoredHistoryContext(
                        filter: filter,
                        selectedRideID: rideID,
                        budget: budget
                    )
                }
                guard !Task.isCancelled, let self,
                      self.selectedRideMapHistoryID == rideID
                else { return }
                self.rideMapHistoryContextProjection = projection
                self.rideMapHistoryContextRoutes = projection.routes
            } catch {
                guard !Task.isCancelled, let self,
                      self.selectedRideMapHistoryID == rideID
                else { return }
                // Context is supplementary. Keep the selected route usable if this bounded
                // secondary projection fails, while ensuring stale context cannot remain visible.
                self.rideMapHistoryContextProjection = nil
                self.rideMapHistoryContextRoutes.removeAll(keepingCapacity: true)
            }
        }
    }

    static func shouldApplyLiveProjection(
        generation: UInt64,
        currentGeneration: UInt64,
        enabled: Bool
    ) -> Bool {
        enabled && generation == currentGeneration
    }

    static func shouldApplyRestoredLiveProjection(
        restorationGeneration: UInt64,
        currentGeneration: UInt64,
        liveProjectionEnabled: Bool
    ) -> Bool {
        !liveProjectionEnabled && restorationGeneration == currentGeneration
    }

    private func applyRideMapDecision(
        snapshot: MobileRideMapSnapshotDto,
        decision: MobileRideMapDecisionDto
    ) {
        rideMapLiveError = nil
        rideMapSnapshot = snapshot
        rideMapLastDecision = decision
        switch decision {
        case let .pending(point, _):
            rideMapLiveTelemetryState = point.telemetryState
        case let .accepted(point, _):
            rideMapLiveTelemetryState = point.telemetryState
            requestLiveProjection()
        case .rejected, .ignored, .storageError:
            break
        }
    }

    /// Serializes live projections while allowing a burst of accepted points to coalesce.
    ///
    /// The Rust projection snapshots the recorder before doing its work, and the detached
    /// operation receives a live-only cancellation token. A generation change cancels the
    /// in-flight operation; the task remains alive until that operation returns so projections
    /// never overlap on the same map core.
    private func requestLiveProjection() {
        rideMapLiveProjectionGeneration &+= 1
        rideMapLiveProjectionEnabled = true
        rideMapLiveProjectionCancellation?.cancel()
        guard rideMapLiveProjectionTask == nil else { return }

        guard let state = core.rideMapStateHandle else { return }
        let budget = Self.rideMapLimits.liveTailPointLimit
        rideMapLiveProjectionTask = Task { [weak self] in
            defer {
                self?.rideMapLiveProjectionTask = nil
                self?.rideMapLiveProjectionCancellation = nil
            }
            while let self {
                guard self.rideMapLiveProjectionEnabled else { break }
                let generation = self.rideMapLiveProjectionGeneration
                let cancellation = MobileLiveRideMapProjectionCancellation()
                self.rideMapLiveProjectionCancellation = cancellation
                do {
                    let projection = try await Self.runCancellableDetached(priority: .userInitiated) {
                        try state.projectPoints(budget: budget, cancellation: cancellation)
                    }
                    guard self.rideMapLiveProjectionEnabled else {
                        break
                    }
                    guard Self.shouldApplyLiveProjection(
                        generation: generation,
                        currentGeneration: self.rideMapLiveProjectionGeneration,
                        enabled: self.rideMapLiveProjectionEnabled
                    ) else {
                        continue
                    }
                    self.applyLiveProjection(projection)
                } catch {
                    guard self.rideMapLiveProjectionEnabled else {
                        break
                    }
                    guard Self.shouldApplyLiveProjection(
                        generation: generation,
                        currentGeneration: self.rideMapLiveProjectionGeneration,
                        enabled: self.rideMapLiveProjectionEnabled
                    ) else {
                        continue
                    }
                    self.rideMapLiveError = Self.mapRideMapError(error)
                    self.clearLiveProjectionState()
                }
                return
            }
        }
    }

    private func applyLiveProjection(_ projection: MobileRideMapRouteProjection) {
        rideMapLiveProjectionVersion &+= 1
        rideMapLiveDisplayPoints = projection.points
        rideMapLiveCameraRegion = projection.cameraRegion
        rideMapLiveEndpointMetadata = projection.endpointMetadata
        rideMapLiveSegments = projection.segments
        rideMapLiveBackgroundGapCount = projection.backgroundGapCount
        rideMapLivePointsTruncated = projection.pointsOmittedByBudget
        rideMapLiveSegmentsOmittedByBudget = projection.segmentsOmittedByBudget
    }

    private func clearLiveProjectionState() {
        rideMapLiveProjectionVersion &+= 1
        rideMapLiveDisplayPoints.removeAll(keepingCapacity: true)
        rideMapLiveCameraRegion = nil
        rideMapLiveEndpointMetadata = .empty
        rideMapLiveSegments.removeAll(keepingCapacity: true)
        rideMapLiveTelemetryState = nil
        rideMapLiveBackgroundGapCount = 0
        rideMapLivePointsTruncated = false
        rideMapLiveSegmentsOmittedByBudget = false
    }

    private func invalidateLiveProjection(clearPoints: Bool) {
        rideMapLiveProjectionGeneration &+= 1
        rideMapLiveProjectionEnabled = false
        rideMapLiveProjectionCancellation?.cancel()
        if clearPoints {
            clearLiveProjectionState()
            rideMapLastDecision = nil
        }
    }

    private func applyRideMapCommand(
        resetPoints: Bool = false,
        _ command: () throws -> MobileRideMapSnapshotDto
    ) -> Bool {
        do {
            rideMapSnapshot = try command()
            rideMapLiveError = nil
            updateRideMapDurationTicker()
            if resetPoints {
                invalidateLiveProjection(clearPoints: true)
            }
            rideMapLiveTelemetryState = rideMapSnapshot?.associatedVehicle == nil
                ? .gpsOnly
                : .associatedNoTelemetry
            return true
        } catch {
            rideMapLiveError = Self.mapRideMapError(error)
            return false
        }
    }

    func pair(platformIdentifier: String) -> Bool {
        switch connectionState {
        case .connecting, .retrying, .connected:
            let isSameSelection = connectionState.selection?.platformIdentifier == platformIdentifier
            guard !isSameSelection || liveActivityError != nil else { return false }
        case .picker, .identified, .failed:
            break
        }
        let rows = devicePickerScanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }) else {
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
            return false
        }
        guard selectedRow.isSupported, let route = selectedRow.connectionRoute else { return false }

        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: route
        )
        liveActivityError = nil
        connectionState = .connecting(selection, phase: .discoveringServices)
        permitsStoredDeviceAutoPairing = true
        phase = .discoveringServices
        let didPair = core.pair(platformIdentifier: platformIdentifier)
        if didPair {
            isRecordOnlyCapture = false
            captureLabel = nil
            recordOnlyDeviceKind = nil
            let displayName = Self.meaningfulDeviceName(
                selectedRow.title,
                identity: platformIdentifier
            )
            let persistedDisplayName = selectedDeviceStore.displayName(for: platformIdentifier)
            let selectionChanged = selectedDeviceStore.platformIdentifier != platformIdentifier
            if selectionChanged {
                selectedDeviceStore.save(
                    platformIdentifier: platformIdentifier,
                    displayName: displayName
                )
            } else if let displayName, displayName != persistedDisplayName {
                selectedDeviceStore.save(
                    platformIdentifier: platformIdentifier,
                    displayName: displayName
                )
            }
            if let displayName, selectionChanged || displayName != persistedDisplayName {
                rideMapVehicleNameCache[platformIdentifier] = displayName
            }
            hasSavedDevice = true
            liveActivityIdentity = liveActivityIdentity(for: selectedRow)
            liveActivityGlyph = liveActivityGlyph(for: selectedRow)
            syncLiveActivity()
        } else {
            connectionState = .picker
            permitsStoredDeviceAutoPairing = false
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
        }
        return didPair
    }

    func recordOnly(platformIdentifier: String, deviceKind: String) -> Bool {
        connectionState = .picker
        let trimmedKind = deviceKind.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedKind.isEmpty else { return false }
        let annotationKind = trimmedKind
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "=", with: " ")
        let annotations = ["device_kind=\(annotationKind)"]
        let modelHint = CutoutModelHint(deviceKind: annotationKind)
        let previousCapture = (
            status: captureStatus,
            progress: captureProgress,
            isFinishing: isFinishingCapture,
            activeLabels: activeCaptureLabels,
            fileName: captureFileName,
            notificationCount: captureNotificationCount,
            label: captureLabel,
            deviceKind: recordOnlyDeviceKind
        )
        resetCaptureSession()
        let didStart = switch modelHint {
        case .falcon:
            core.pair(platformIdentifier: platformIdentifier, model: .falcon)
        case .aero:
            core.pair(platformIdentifier: platformIdentifier, model: .aero)
        case .unknown:
            core.recordOnly(
                platformIdentifier: platformIdentifier,
                note: "unsupported picker row",
                annotations: annotations
            )
        }
        guard didStart else {
            captureStatus = previousCapture.status
            captureProgress = previousCapture.progress
            isFinishingCapture = previousCapture.isFinishing
            activeCaptureLabels = previousCapture.activeLabels
            captureFileName = previousCapture.fileName
            captureNotificationCount = previousCapture.notificationCount
            captureLabel = previousCapture.label
            recordOnlyDeviceKind = previousCapture.deviceKind
            return false
        }

        permitsStoredDeviceAutoPairing = false
        if modelHint != .unknown {
            core.annotateCapture(key: "device_kind", value: annotationKind)
        }
        isRecordOnlyCapture = modelHint == .unknown
        recordOnlyDeviceKind = annotationKind
        if modelHint == .unknown {
            liveActivityIdentity = nil
            liveActivityGlyph = .electricUnicycle
        }
        syncLiveActivity()
        return true
    }

    func startProbe(platformIdentifier: String) -> Bool {
        let rows = devicePickerScanState?.rows ?? []
        guard let selectedRow = rows.first(where: { $0.id == platformIdentifier }),
              selectedRow.isProbeRecommended
        else {
            return false
        }
        let selection = ConnectionSelection(
            platformIdentifier: selectedRow.id,
            title: selectedRow.title,
            route: .electricUnicycle
        )
        connectionState = .connecting(selection, phase: .discoveringServices)
        phase = .discoveringServices
        let didStart = core.probe(platformIdentifier: platformIdentifier)
        if !didStart {
            connectionState = .picker
            phase = .scanning
            devicePickerScanState = .failed(
                localizedAppText("picker.error.device_no_longer_available"),
                rows: rows
            )
        }
        return didStart
    }

    private func resetCaptureSession() {
        captureStatus = nil
        captureProgress = nil
        isFinishingCapture = false
        activeCaptureLabels.removeAll()
        captureFileName = nil
        captureNotificationCount = 0
        captureLabel = nil
        recordOnlyDeviceKind = nil
    }

    func startCaptureLabel(_ label: CaptureQuickLabel) {
        guard !activeCaptureLabels.contains(label) else { return }

        if let activeLabel = activeCaptureLabels.first(where: label.isMutuallyExclusive)
        {
            activeCaptureLabels.remove(activeLabel)
            core.annotateCapture(label: "\(activeLabel.annotationValue)_stop")
        }

        activeCaptureLabels.insert(label)
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_start")
        captureStatus = .labelStarted(
            label: label.title,
            notificationCount: captureNotificationCount,
            fileName: captureFileName
        )
    }

    @discardableResult
    func flushCapture() async -> Bool {
        let didFlush = await core.flushCapture()
        if !didFlush, captureStatus?.isRecording == true {
            captureStatus = .failed
        }
        return didFlush
    }

    func appDidEnterBackground() {
        musicMonitorSceneState.suspend()
        stopMusicMonitoring()
        if let nowPlaying = musicCoordinator.nowPlaying {
            musicNowPlaying = nowPlaying.staleProjection
        }
        guard let snapshot = currentLiveActivitySnapshot() else {
            guard isRecordOnlyCapture else { return }
            Task { [weak self] in _ = await self?.flushCapture() }
            return
        }
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.appDidEnterBackground(
                requestID: requestID,
                snapshot: snapshot,
                captureFlush: { [weak self] in
                    await self?.flushCapture() ?? false
                }
            )
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
    }

    func appDidBecomeActive() {
        if musicMonitorSceneState.resumeIfNeeded() {
            beginMusicMonitoring()
        }
        guard let snapshot = currentLiveActivitySnapshot() else { return }
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.appDidBecomeActive(
                requestID: requestID,
                snapshot: snapshot
            )
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
    }

    @discardableResult
    func finishCapture() async -> Bool {
        guard !isFinishingCapture else { return false }
        isFinishingCapture = true

        guard await flushCapture() else {
            captureStatus = .failed
            isFinishingCapture = false
            return false
        }

        disconnectTransport()
        return true
    }

    func stopCaptureLabel(_ label: CaptureQuickLabel) {
        guard activeCaptureLabels.remove(label) != nil else { return }
        captureLabel = label.title
        core.annotateCapture(label: "\(label.annotationValue)_stop")
        captureStatus = .labelStopped(
            label: label.title,
            notificationCount: captureNotificationCount,
            fileName: captureFileName
        )
    }

    func disconnectTransport() {
        endLiveActivity(reason: .disconnected)
        isRecordOnlyCapture = false
        activeCaptureLabels.removeAll()
        captureLabel = nil
        recordOnlyDeviceKind = nil
        connectionState = .picker
        phase = .scanning
        liveActivityIdentity = nil
        liveActivityGlyph = .electricUnicycle
        permitsStoredDeviceAutoPairing = false
        core.disconnectAndScan()
    }

    func forgetSavedDevice() {
        disconnectTransport()
        try? selectedDeviceStore.clear()
        hasSavedDevice = false
    }

    func endLiveActivity(reason: LiveActivityRideLifecycleEndReason = .sessionEnded) {
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.end(requestID: requestID, reason: reason)
            self?.liveActivityError = await liveActivityCoordinator.lastError
        }
        lastLiveActivitySnapshot = nil
        lastLiveActivityUpdate = nil
    }

    func applyProtocolIdentityCandidate(_ candidate: DevicePickerDiscoveryCandidate?) {
        guard isRecordOnlyCapture != true else {
            liveActivityIdentity = nil
            liveActivityGlyph = .electricUnicycle
            syncLiveActivity()
            return
        }
        if let candidate,
           let displayName = Self.meaningfulDeviceName(
               candidate.displayName,
               identity: candidate.platformIdentifier
           ) {
            // Persist every resolved identity, not only the currently selected one. History can
            // contain rides from an older CoreBluetooth identifier and must still be relabelable.
            let persistedDisplayName = selectedDeviceStore.displayName(
                for: candidate.platformIdentifier
            )
            if persistedDisplayName != displayName {
                selectedDeviceStore.save(
                    platformIdentifier: candidate.platformIdentifier,
                    displayName: displayName
                )
                rideMapVehicleNameCache[candidate.platformIdentifier] = displayName
            }
        }
        if let model = candidate?.support.electricUnicycleModel {
            liveActivityIdentity = .model(model)
            liveActivityGlyph = .electricUnicycle
        }
        guard let selection = selection(from: candidate) else {
            syncLiveActivity()
            return
        }
        if connectionState.selection == nil {
            connectionState = .identified(selection)
        }
        if selection.route == .vescOnewheel {
            liveActivityIdentity = vescRideIdentity(using: selection.title)
            liveActivityGlyph = .floatwheelAtom
        }
        syncLiveActivity()
    }

    private func handleScanStateChange(_ scanState: DevicePickerScanState) {
        devicePickerScanState = scanState
        guard phase == .starting || phase == .scanning else { return }
        switch rideSessionRestorationState {
        case .complete:
            break
        case let .awaitingSnapshot(platformIdentifier):
            guard selectedDeviceStore.platformIdentifier == platformIdentifier else { return }
        case .awaitingBluetooth, .recovering:
            return
        }
        guard permitsStoredDeviceAutoPairing else { return }
        guard let platformIdentifier = selectedDeviceStore.platformIdentifier else { return }
        guard scanState.storedSupportedRow(platformIdentifier: platformIdentifier) != nil else { return }
        _ = pair(platformIdentifier: platformIdentifier)
    }

    private func handlePhaseChange(_ phase: SessionConnectionPhase) {
        switch (phase, connectionState) {
        case (.starting, .identified), (.starting, .connecting), (.starting, .retrying), (.starting, .connected),
             (.scanning, .identified), (.scanning, .connecting), (.scanning, .retrying), (.scanning, .connected):
            return
        default:
            break
        }
        guard !phase.supportsLiveActivity || connectionState.selection != nil || permitsStoredDeviceAutoPairing else { return }
        if case .live = phase {
            switch connectionState {
            case .connecting(_, phase: .subscribing), .identified:
                break
            default:
                return
            }
        }
        if case .failed = phase {
            guard connectionState.selection != nil else { return }
        }
        if case .failed = connectionState {
            switch phase {
            case .connecting, .discoveringServices, .subscribing, .live:
                return
            case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .failed:
                break
            }
        }
        self.phase = phase
        switch phase {
        case .connecting, .discoveringServices, .subscribing:
            if let selection = connectionState.selection {
                connectionState = .connecting(selection, phase: phase)
            }
        case .live:
            if let selection = connectionState.selection {
                connectionState = .connected(selection)
            } else if let selection = selection(from: core.protocolIdentityCandidate) {
                connectionState = .connected(selection)
            }
        case let .failed(failure):
            guard let selection = connectionState.selection else { return }
            connectionState = .failed(selection, failure)
            let rows = devicePickerScanState?.rows ?? []
            devicePickerScanState = .failed(phase.displayText, rows: rows)
        case .bluetoothPermissionDenied, .bluetoothUnavailable:
            connectionState = .picker
        case .starting, .scanning:
            break
        }
    }

    private func handleReconnectScheduled(_ retry: SessionConnectionRetry) {
        guard let selection = connectionState.selection,
              selection.platformIdentifier == retry.platformIdentifier
        else { return }
        if case .failed = connectionState { return }
        connectionState = .retrying(selection, retry: retry)
    }

    private func handleBluetoothRestorationResolved(_ platformIdentifier: String?) {
        guard case .awaitingBluetooth = rideSessionRestorationState else { return }
        let marker = restorationMarkerAtLaunch
        guard platformIdentifier != nil || marker != nil else {
            permitsStoredDeviceAutoPairing = true
            rideSessionRestorationState = .complete
            if let scanState = devicePickerScanState {
                handleScanStateChange(scanState)
            }
            return
        }
        if platformIdentifier != nil, marker == nil {
            permitsStoredDeviceAutoPairing = false
            rideSessionRestorationState = .complete
            return
        }
        if let platformIdentifier, let marker {
            let markerMatches = (try? core.rideSessionStateHandle
                .rideSessionMarkerMatchesPlatformIdentifier(
                    marker: marker,
                    platformIdentifier: platformIdentifier
                )) == true
            permitsStoredDeviceAutoPairing = markerMatches
            if !markerMatches {
                beginRideSessionRecovery(
                    restoredPlatformIdentifier: platformIdentifier,
                    snapshot: nil
                )
                return
            }
        }
        guard let platformIdentifier else {
            beginRideSessionRecovery(restoredPlatformIdentifier: nil, snapshot: nil)
            return
        }
        rideSessionRestorationState = .awaitingSnapshot(platformIdentifier: platformIdentifier)
        syncLiveActivity()
    }

    private func beginRideSessionRecovery(
        restoredPlatformIdentifier: String?,
        snapshot: LiveActivityRideSnapshot?
    ) {
        rideSessionRestorationState = .recovering
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        Task { [weak self, liveActivityCoordinator] in
            let recoveryResult = await liveActivityCoordinator.recoverPersistedRide(
                requestID: requestID,
                restoredPlatformIdentifier: restoredPlatformIdentifier,
                snapshot: snapshot
            )
            let error = await liveActivityCoordinator.lastError
            guard let self else { return }
            rideSessionRestorationState = .complete
            liveActivityError = error
            if case .adopted = recoveryResult,
               error == nil,
               core.rideSessionStateHandle.rideSessionSnapshot().phase == .active
            {
                lastLiveActivitySnapshot = snapshot
                lastLiveActivityUpdate = snapshot == nil ? nil : core.now()
            }
            syncLiveActivity()
        }
    }

    private func waitForRideSessionRecoveryIfNeeded(
        snapshot: LiveActivityRideSnapshot?
    ) -> Bool {
        switch rideSessionRestorationState {
        case .complete:
            return false
        case .awaitingBluetooth, .recovering:
            return true
        case let .awaitingSnapshot(platformIdentifier):
            if let snapshot {
                beginRideSessionRecovery(
                    restoredPlatformIdentifier: platformIdentifier,
                    snapshot: snapshot
                )
            }
            return true
        }
    }

    private static func makeSessionDriver() -> any CutoutSessionDriving {
        #if DEBUG
        if let fixture = uiTestFixture {
            let rideMapState = RustPersistenceStore.shared.map(MobileRideMapState.init(database:))
            return CutoutSessionCore(testScript: fixture.testScript, rideMapState: rideMapState)
        }
        #endif
        return CutoutSessionCore()
    }

    #if DEBUG
    private static var uiTestFixture: CutoutUITestSessionFixture? {
        CutoutUITestSessionFixture.resolve(
            environmentValue: ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_FIXTURE"],
            persistedValue: UserDefaults.standard.string(forKey: "CUTOUT_UI_TEST_FIXTURE"),
            arguments: ProcessInfo.processInfo.arguments
        )
    }
    #endif

    private func syncLiveActivity() {
        let snapshot = currentLiveActivitySnapshot()
        guard waitForRideSessionRecoveryIfNeeded(snapshot: snapshot) == false else { return }
        if case .failed = phase, let snapshot {
            liveActivityRequestID += 1
            let requestID = liveActivityRequestID
            lastLiveActivitySnapshot = nil
            lastLiveActivityUpdate = nil
            Task { [weak self, liveActivityCoordinator] in
                await liveActivityCoordinator.reconnectExhausted(
                    requestID: requestID,
                    snapshot: snapshot
                )
                self?.liveActivityError = await liveActivityCoordinator.lastError
            }
            return
        }

        switch phase {
        case .bluetoothPermissionDenied, .bluetoothUnavailable:
            guard let snapshot else { break }
            liveActivityRequestID += 1
            let requestID = liveActivityRequestID
            lastLiveActivitySnapshot = nil
            lastLiveActivityUpdate = nil
            Task { [weak self, liveActivityCoordinator] in
                await liveActivityCoordinator.unrecoverableSessionFailure(
                    requestID: requestID,
                    snapshot: snapshot
                )
                self?.liveActivityError = await liveActivityCoordinator.lastError
            }
            return
        default:
            break
        }

        let rideLifecyclePhase = core.rideSessionStateHandle.rideSessionSnapshot().phase
        if phase.isReconnectingTransport,
           rideLifecyclePhase == .active || rideLifecyclePhase == .reconnecting,
           let previousSnapshot = lastLiveActivitySnapshot {
            guard rideLifecyclePhase == .active else { return }
            let staleSnapshot = previousSnapshot.presented(isStale: true)
            liveActivityRequestID += 1
            let requestID = liveActivityRequestID
            let atMs = core.now().rawValue
            lastLiveActivitySnapshot = staleSnapshot
            lastLiveActivityUpdate = core.now()
            Task { [weak self, liveActivityCoordinator] in
                await liveActivityCoordinator.transportDisconnected(
                    requestID: requestID,
                    atMs: atMs,
                    snapshot: staleSnapshot
                )
                self?.liveActivityError = await liveActivityCoordinator.lastError
            }
            return
        }

        let shouldBeActive = phase.supportsLiveActivity && liveActivityIdentity != nil && isRecordOnlyCapture == false
        let endReason: LiveActivityRideLifecycleEndReason = switch phase {
        case .scanning:
            .disconnected
        case .bluetoothPermissionDenied, .bluetoothUnavailable, .failed:
            .unavailable
        default:
            .sessionEnded
        }
        guard shouldReconcileLiveActivity(snapshot: snapshot, shouldBeActive: shouldBeActive) else { return }
        liveActivityRequestID += 1
        let requestID = liveActivityRequestID
        let platformIdentifier = connectionState.selection?.platformIdentifier
        let monotonicTimeMs = core.now().rawValue
        Task { [weak self, liveActivityCoordinator] in
            await liveActivityCoordinator.reconcile(
                requestID: requestID,
                platformIdentifier: platformIdentifier,
                monotonicTimeMs: monotonicTimeMs,
                snapshot: snapshot,
                shouldBeActive: shouldBeActive,
                endReason: endReason
            )
            let error = await liveActivityCoordinator.lastError
            self?.liveActivityError = error
            if error != nil {
                self?.lastLiveActivitySnapshot = nil
                self?.lastLiveActivityUpdate = nil
            }
        }
    }

    private func currentLiveActivitySnapshot() -> LiveActivityRideSnapshot? {
        liveActivityIdentity.map {
            LiveActivityRideSnapshot(identity: $0, glyph: liveActivityGlyph, rideState: rideState, now: core.now())
        }
    }

    private func selection(from candidate: DevicePickerDiscoveryCandidate?) -> ConnectionSelection? {
        guard let candidate, candidate.support.isSupported, let route = candidate.support.connectionRoute else {
            return nil
        }
        return ConnectionSelection(
            platformIdentifier: candidate.platformIdentifier,
            title: candidate.displayName,
            route: route
        )
    }

    private func vescRideIdentity(using title: String?) -> LiveActivityRideIdentity {
        .device(title ?? VescRideSnapshot.defaultTitle)
    }

    private func liveActivityIdentity(for selectedRow: DevicePickerRow?) -> LiveActivityRideIdentity? {
        if selectedRow?.connectionRoute == .vescOnewheel {
            return vescRideIdentity(using: selectedRow?.title)
        }
        if core.protocolIdentityCandidate?.support.connectionRoute == .vescOnewheel {
            return vescRideIdentity(using: core.protocolIdentityCandidate?.displayName)
        }
        if let model = selectedRow?.electricUnicycleModel
            ?? core.protocolIdentityCandidate?.support.electricUnicycleModel
            ?? phase.connectingModel {
            return .model(model)
        }
        return nil
    }

    private func liveActivityGlyph(for selectedRow: DevicePickerRow?) -> LiveActivityRideGlyph {
        if selectedRow?.connectionRoute == .vescOnewheel
            || core.protocolIdentityCandidate?.support.connectionRoute == .vescOnewheel {
            return .floatwheelAtom
        }
        return .electricUnicycle
    }

    private func shouldReconcileLiveActivity(
        snapshot: LiveActivityRideSnapshot?,
        shouldBeActive: Bool
    ) -> Bool {
        let now = core.now()
        guard shouldBeActive else {
            lastLiveActivitySnapshot = nil
            lastLiveActivityUpdate = nil
            return true
        }
        guard let snapshot else { return false }
        if core.rideSessionStateHandle.rideSessionSnapshot().phase == .reconnecting {
            lastLiveActivitySnapshot = snapshot
            lastLiveActivityUpdate = now
            return true
        }
        guard snapshot != lastLiveActivitySnapshot else { return false }
        let previousSnapshot = lastLiveActivitySnapshot
        guard
            previousSnapshot == nil
                || snapshot.connectionState != previousSnapshot?.connectionState
                || lastLiveActivityUpdate.map({ now.elapsed(since: $0).rawValue >= Self.liveActivityUpdateIntervalMilliseconds }) != false
        else { return false }

        lastLiveActivitySnapshot = snapshot
        lastLiveActivityUpdate = now
        return true
    }

    func applyCaptureEvent(_ event: CaptureEvent) {
        switch event {
        case let .started(fileURL):
            captureFileName = fileURL.lastPathComponent
            captureNotificationCount = 0
            captureStatus = captureFileName.map(CaptureStatus.recordingLocally)
        case .notificationRecorded:
            captureNotificationCount += 1
            captureStatus = .recording(
                label: captureLabel,
                notificationCount: captureNotificationCount,
                fileName: captureFileName
            )
        case let .progress(progress):
            captureProgress = progress
            captureNotificationCount = Int(clamping: progress.notificationCount)
            captureStatus = .recording(
                label: captureLabel,
                notificationCount: captureNotificationCount,
                fileName: captureFileName
            )
        case let .finished(fileURL):
            captureFileName = fileURL.lastPathComponent
            captureStatus = .saved(fileName: fileURL.lastPathComponent)
        case .failed:
            captureStatus = .failed
        }
    }
}

private extension SessionConnectionPhase {
    var supportsLiveActivity: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing, .live:
            true
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .failed:
            false
        }
    }

    var connectingModel: ElectricUnicycleModel? {
        guard case .connecting(let model) = self else { return nil }
        return model
    }

    var isReconnectingTransport: Bool {
        switch self {
        case .connecting, .discoveringServices, .subscribing:
            true
        case .starting, .bluetoothPermissionDenied, .bluetoothUnavailable, .scanning, .live, .failed:
            false
        }
    }
}
